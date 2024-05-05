use std::{
    borrow::Cow, env, error, fmt::Display, fs, process::ExitCode, sync::Arc, time::Duration,
};

use local_cdn_dns::{
    action::{DomainAction, FromConfig},
    config::Listen,
};
use serde::Deserialize;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

#[derive(Deserialize)]
#[serde(bound = "'de:'a")]
struct Config<'a> {
    #[serde(default)]
    log_level: LogLevel,
    #[serde(default)]
    json_log: Option<&'a str>,
    server: local_cdn_dns::config::Config<'a>,
}

#[derive(Debug)]
struct Error {
    message: Cow<'static, str>,
    source: Option<Box<dyn error::Error + 'static>>,
}
impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match &self.source {
            Some(e) => Some(e.as_ref()),
            None => None,
        }
    }
}

trait ResultExt<T> {
    fn context(self, message: &'static str) -> Result<T, Error>;
    fn with_context(self, f: impl FnOnce() -> String) -> Result<T, Error>;
}
impl<T, E: error::Error + 'static> ResultExt<T> for Result<T, E> {
    fn context(self, message: &'static str) -> Result<T, Error> {
        match self {
            Self::Ok(o) => Ok(o),
            Self::Err(e) => Err(Error {
                message: Cow::Borrowed(message),
                source: Some(Box::new(e)),
            }),
        }
    }
    fn with_context(self, f: impl FnOnce() -> String) -> Result<T, Error> {
        match self {
            Self::Ok(o) => Ok(o),
            Self::Err(e) => Err(Error {
                message: Cow::Owned(f()),
                source: Some(Box::new(e)),
            }),
        }
    }
}

fn start_server(config: local_cdn_dns::config::Config) -> Result<(), Error> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    let upstream = {
        let _guard = runtime.enter();
        config
            .upstream
            .into_iter()
            .map(|(name, cfg)| {
                (
                    name,
                    Arc::new(local_cdn_dns::action::Upstream {
                        name: name.to_owned(),
                        resolver: hickory_resolver::TokioAsyncResolver::tokio(
                            cfg.config.into(),
                            cfg.options,
                        ),
                    }),
                )
            })
            .collect()
    };
    let handler: DomainAction<local_cdn_dns::action::Action<_>> =
        local_cdn_dns::action::DomainAction::from_config(config.actions, &upstream)
            .context("invalid config")?;
    let mut server =
        hickory_server::ServerFuture::new(local_cdn_dns::server::InQueryHandler(handler));
    for l in config.listen {
        let _guard = runtime.enter();
        match l {
            Listen::Udp(address) => {
                server.register_socket(
                    runtime
                        .block_on(tokio::net::UdpSocket::bind(address))
                        .with_context(|| format!("failed to bind udp socket to {address}"))?,
                );
                tracing::info!(
                    socket = tracing::field::display(address),
                    "registered udp socket"
                );
            }
            Listen::Tcp {
                address,
                timeout_sec,
            } => {
                server.register_listener(
                    runtime
                        .block_on(tokio::net::TcpListener::bind(address))
                        .with_context(|| format!("failed to bind tcp listener to {address}"))?,
                    Duration::from_secs(timeout_sec as u64),
                );
                tracing::info!(
                    listener = tracing::field::display(address),
                    "registered tcp listener"
                );
            }
        }
    }
    tracing::info!("server started");

    runtime
        .block_on(server.block_until_done())
        .context("server error")
}

fn run() -> Result<ExitCode, Error> {
    let config_txt = fs::read({
        let mut a = env::args();
        let _ = a.next();
        a.next().ok_or_else(|| Error {
            message: Cow::Borrowed("missing config path"),
            source: None,
        })?
    })
    .context("failed to read config file")?;
    let config: Config =
        serde_json::from_slice(&config_txt).context("failed to decode config file")?;
    {
        let reg = tracing_subscriber::registry()
            .with(match config.log_level {
                LogLevel::Off => LevelFilter::OFF,
                LogLevel::Error => LevelFilter::ERROR,
                LogLevel::Warn => LevelFilter::WARN,
                LogLevel::Info => LevelFilter::INFO,
                LogLevel::Debug => LevelFilter::DEBUG,
                LogLevel::Trace => LevelFilter::TRACE,
            })
            .with(tracing_subscriber::fmt::layer());
        match &config.json_log {
            Some(l) => reg
                .with(
                    tracing_subscriber::fmt::layer().json().with_writer(
                        fs::File::options()
                            .create(true)
                            .write(true)
                            .open(l)
                            .context("failed to open log file")?,
                    ),
                )
                .init(),
            None => reg.init(),
        }
    }

    Ok(match start_server(config.server) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e:?}");
            ExitCode::FAILURE
        }
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e:?}");
            ExitCode::FAILURE
        }
    }
}
