use std::{
    fmt::Display, fs::Permissions, os::unix::fs::PermissionsExt, path::PathBuf, process::ExitCode,
    str::FromStr,
};

use anyhow::Context;
use bytes::Bytes;
use clap::{Arg, ArgGroup, Args, FromArgMatches, Parser};
use http::{header, uri::Authority, Request, Response, StatusCode};
use http_body_util::{Either, Full};
use hyper::{
    body::{Body, Incoming},
    rt::{Read, Write},
};
use hyper_util::rt::{TokioExecutor, TokioIo};
use local_cdn_proxy::{CachedResponse, ProxyError};
use tracing::Instrument;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
enum LogOutput {
    #[default]
    Stdout,
    Journal,
}
impl Display for LogOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stdout => "stdout",
            Self::Journal => "journal",
        })
    }
}
#[derive(Debug, Clone)]
enum Listen {
    Unix(String),
    Tcp(std::net::SocketAddr),
}

#[derive(Debug, clap::Parser)]
struct Cli {
    #[arg(long, default_value_t)]
    log_output: LogOutput,
    #[command(flatten)]
    listen: Listen,
    root: String,
    server: String,
}

impl Args for Listen {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        cmd.arg(Arg::new("unix").long("unix"))
            .arg(
                Arg::new("tcp")
                    .long("tcp")
                    .value_parser(clap::value_parser!(std::net::SocketAddr)),
            )
            .group(ArgGroup::new("listen").args(["unix", "tcp"]).required(true))
    }
    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        Self::augment_args(cmd)
    }
}
impl FromArgMatches for Listen {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        match (
            matches.get_one::<String>("unix"),
            matches.get_one::<std::net::SocketAddr>("tcp"),
        ) {
            (Some(u), None) => Ok(Self::Unix(u.clone())),
            (None, Some(t)) => Ok(Self::Tcp(t.clone())),
            _ => unreachable!(),
        }
    }
    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        match (
            matches.get_one::<String>("unix"),
            matches.get_one::<std::net::SocketAddr>("tcp"),
        ) {
            (Some(u), None) => *self = Self::Unix(u.clone()),
            (None, Some(t)) => *self = Self::Tcp(t.clone()),
            (None, None) => (),
            _ => unreachable!(),
        }
        Ok(())
    }
}

async fn serve_connection<S, B, I>(
    builder: hyper_util::server::conn::auto::Builder<TokioExecutor>,
    service: S,
    conn: I,
) where
    S: Clone + Send + 'static,
    S: tower_service::Service<Request<Incoming>, Response = http::Response<B>>,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send,
    B: Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    I: Read + Write + Unpin + 'static,
{
    tracing::info!("client connected");
    match builder
        .serve_connection(conn, hyper_util::service::TowerToHyperService::new(service))
        .await
    {
        Ok(()) => {
            tracing::info!("client disconnected")
        }
        Err(e) => {
            tracing::error!("serve error: {e:?}",)
        }
    }
}

fn map_result<E: std::error::Error + Send + Sync + 'static>(
    r: Result<CachedResponse, ProxyError<E>>,
) -> Result<CachedResponse, ProxyError<E>> {
    fn error_response(
        status: StatusCode,
        err: impl std::error::Error + Send + Sync + 'static,
    ) -> local_cdn_proxy::CachedResponse {
        Response::builder()
            .status(status)
            .header(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/plain"),
            )
            .body(Either::Right(Full::new(Bytes::from(
                format!("{:?}", anyhow::Error::new(err)).into_bytes(),
            ))))
            .unwrap()
    }
    match r {
        Ok(r) => Ok(r),
        Err(e) => {
            tracing::error!("{e}");
            match &e {
                ProxyError::MissingHost
                | ProxyError::InvalidHost(_, _)
                | ProxyError::UnexpectedHost(_)
                | ProxyError::InvalidUri(_)
                | ProxyError::InvalidPath(_, _) => Ok(error_response(StatusCode::BAD_REQUEST, e)),
                ProxyError::Upstream(_) | ProxyError::BoxedUpstream(_) => {
                    Ok(error_response(StatusCode::BAD_GATEWAY, e))
                }
                ProxyError::ReadCache(_) | ProxyError::Decode(_) | ProxyError::WriteCache(_) => {
                    Ok(error_response(StatusCode::INTERNAL_SERVER_ERROR, e))
                }
            }
        }
    }
}

fn run(root: PathBuf, server: String, listen: Listen) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;

    let authority = Authority::from_str(&server).context("invalid server name")?;

    let client = hyper_util::client::legacy::Builder::new(hyper_util::rt::TokioExecutor::new())
        .build::<_, local_cdn_proxy::UpstreamBody>(local_cdn_proxy::connector::Connector(
        hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .context("failed to get certificate roots")?
            .https_only()
            .with_server_name_resolver(hyper_rustls::FixedServerNameResolver::new(
                server.try_into().context("invalid server name")?,
            ))
            .enable_all_versions()
            .build(),
    ));
    let service = tower::ServiceBuilder::new()
        .layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(
                    tower_http::trace::DefaultMakeSpan::new().level(tracing::Level::INFO),
                )
                .on_request(tower_http::trace::DefaultOnRequest::new().level(tracing::Level::INFO))
                .on_response(
                    tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO),
                ),
        )
        .map_result(map_result)
        .layer(local_cdn_proxy::CacheLayer::new(root, authority))
        .service(client);

    let builder =
        hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());

    match listen {
        Listen::Tcp(s) => rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind(s)
                .await
                .with_context(|| format!("failed to bind to tcp addr {s}"))?;
            tracing::info!(addr = %s, "listening tcp connection");
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        tokio::spawn(
                            serve_connection(
                                builder.clone(),
                                service.clone(),
                                TokioIo::new(stream),
                            )
                            .instrument(tracing::info_span!("tcp_client", addr = %addr)),
                        );
                    }
                    Err(e) => {
                        tracing::error!("failed to get client {:?}", anyhow::Error::new(e))
                    }
                }
            }
        }),
        Listen::Unix(u) => rt.block_on(async {
            let listener = tokio::net::UnixListener::bind(u.as_str())
                .with_context(|| format!("failed to bind to unix socket: {u}"))?;
            tracing::info!(addr = u, "listening unix socket");
            std::fs::set_permissions(u, Permissions::from_mode(0o666))
                .context("failed to set socket permission")?;
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        tokio::spawn(
                            serve_connection(
                                builder.clone(),
                                service.clone(),
                                TokioIo::new(stream),
                            )
                            .instrument(tracing::info_span!("unix_client", addr = ?addr)),
                        );
                    }
                    Err(e) => {
                        tracing::error!("failed to get client {:?}", anyhow::Error::new(e))
                    }
                }
            }
        }),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let reg = tracing_subscriber::registry().with({
        #[cfg(feature = "local")]
        {
            tracing_subscriber::filter::EnvFilter::builder()
                .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
                .from_env()
                .unwrap()
        }
        #[cfg(not(feature = "local"))]
        {
            use tracing_subscriber::filter::LevelFilter;
            match std::env::var("RUST_LOG") {
                Ok(v) => match v.as_str() {
                    "trace" => LevelFilter::TRACE,
                    "debug" => LevelFilter::DEBUG,
                    "info" => LevelFilter::INFO,
                    "warn" => LevelFilter::WARN,
                    "error" => LevelFilter::ERROR,
                    "off" => LevelFilter::OFF,
                    _ => panic!("invalid log filter"),
                },
                Err(_) => LevelFilter::INFO,
            }
        }
    });
    match cli.log_output {
        LogOutput::Stdout => reg.with(tracing_subscriber::fmt::layer()).init(),
        LogOutput::Journal => reg
            .with(tracing_journald::layer().expect("failed to open journal"))
            .init(),
    }

    match run(cli.root.into(), cli.server, cli.listen) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("error: {e:?}");
            ExitCode::FAILURE
        }
    }
}
