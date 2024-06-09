use std::{
    borrow::Cow, env, error, fmt::Display, fs, process::ExitCode, sync::Arc, time::Duration,
};

use tracing::{level_filters::LevelFilter, Instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use local_cdn_dns::{
    action::{DomainAction, FromConfig},
    config::Listen,
};

#[derive(Debug)]
struct Error {
    message: Cow<'static, str>,
    source: Option<Box<dyn error::Error + Send + 'static>>,
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
impl<T, E: error::Error + Send + 'static> ResultExt<T> for Result<T, E> {
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

async fn start_server(
    handler: impl hickory_server::server::RequestHandler,
    listen: Vec<Listen>,
) -> Result<(), Error> {
    let mut server =
        hickory_server::ServerFuture::new(local_cdn_dns::server::InQueryHandler(handler));
    for l in listen {
        match l {
            Listen::Udp(address) => {
                server.register_socket(
                    tokio::net::UdpSocket::bind(address)
                        .await
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
                    tokio::net::TcpListener::bind(address)
                        .await
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

    server.block_until_done().await.context("server error")
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
    let config: local_cdn_dns::config::Config<'_> =
        serde_json::from_slice(&config_txt).context("failed to decode config file")?;
    {
        let reg = tracing_subscriber::registry()
            .with(match config.log_level {
                local_cdn_dns::config::LogLevel::Off => LevelFilter::OFF,
                local_cdn_dns::config::LogLevel::Error => LevelFilter::ERROR,
                local_cdn_dns::config::LogLevel::Warn => LevelFilter::WARN,
                local_cdn_dns::config::LogLevel::Info => LevelFilter::INFO,
                local_cdn_dns::config::LogLevel::Debug => LevelFilter::DEBUG,
                local_cdn_dns::config::LogLevel::Trace => LevelFilter::TRACE,
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
                    Arc::new(local_cdn_dns::action::Upstream::new(
                        name.to_owned(),
                        cfg.config.into(),
                        cfg.options,
                    )),
                )
            })
            .collect()
    };
    let mut servers = tokio::task::JoinSet::new();
    for (name, cfg) in config.servers {
        let handler: DomainAction<local_cdn_dns::action::Action<_>> =
            DomainAction::from_config(cfg.action, &upstream)
                .with_context(|| format!("{name}: invalid config"))?;
        let _guard = runtime.enter();
        servers.spawn(
            async {
                let ret = start_server(handler, cfg.listen).await;
                if let Err(ref e) = ret {
                    tracing::error!("{e:?}");
                }
                ret
            }
            .instrument(tracing::info_span!("server", server = name)),
        );
    }

    runtime.block_on(async move {
        let mut success = true;
        while let Some(r) = servers.join_next().await {
            success &= r.unwrap().is_ok();
        }
        Ok(if success {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        })
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
