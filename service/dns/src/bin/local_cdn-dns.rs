use std::{borrow::Cow, error, fmt::Display, fs, process::ExitCode, sync::Arc, time::Duration};

use clap::Parser;
use tracing::{level_filters::LevelFilter, Instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use local_cdn_dns::{
    action::{DomainAction, FromConfig},
    config::Listen,
};

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}
impl Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        })
    }
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum LogOutput {
    Stdout,
    Journal,
}

#[derive(Debug, clap::Parser)]
struct Cli {
    #[arg(long, default_value_t)]
    log_level: LogLevel,
    #[arg(long)]
    log_output: LogOutput,
    config: String,
}

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

fn run(cli: Cli) -> Result<ExitCode, Error> {
    {
        let reg = tracing_subscriber::registry().with(match cli.log_level {
            LogLevel::Off => LevelFilter::OFF,
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
        });
        match cli.log_output {
            LogOutput::Stdout => reg.with(tracing_subscriber::fmt::layer()).init(),
            LogOutput::Journal => reg
                .with(tracing_journald::layer().context("failed to init journald output")?)
                .init(),
        }
    }

    let config_txt = fs::read(cli.config).context("failed to read config file")?;
    let config: local_cdn_dns::config::Config<'_> =
        serde_json::from_slice(&config_txt).context("failed to decode config file")?;

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
    let cli = Cli::parse();
    match run(cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e:?}");
            ExitCode::FAILURE
        }
    }
}
