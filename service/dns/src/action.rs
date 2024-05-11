use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use hickory_resolver::name_server::{ConnectionProvider, TokioConnectionProvider};
use hickory_server::server::{RequestHandler, ResponseInfo};
use serde::Deserialize;

pub mod block;
pub use block::Block;

pub mod fixed;
pub use fixed::Fixed;

pub mod forward;
pub use forward::Forward;

pub mod local_srv;
pub use local_srv::UnixService;

pub mod domain;
pub use domain::DomainAction;
use tokio::{sync::RwLock, time::timeout};

pub struct Upstream<R: ConnectionProvider> {
    pub name: String,
    pub config: hickory_resolver::config::ResolverConfig,
    pub options: hickory_resolver::config::ResolverOpts,
    timeout: Duration,
    resolver: RwLock<hickory_resolver::AsyncResolver<R>>,
}
impl Upstream<hickory_resolver::name_server::TokioConnectionProvider> {
    pub fn new(
        name: String,
        config: hickory_resolver::config::ResolverConfig,
        options: hickory_resolver::config::ResolverOpts,
    ) -> Self {
        Self {
            name,
            config: config.clone(),
            options: options.clone(),
            timeout: options.timeout,
            resolver: RwLock::new(hickory_resolver::AsyncResolver::tokio(config, options)),
        }
    }
    // workaround for https://github.com/hickory-dns/hickory-dns/issues/2050
    pub async fn lookup<N: hickory_resolver::IntoName>(
        &self,
        name: N,
        record_type: hickory_proto::rr::RecordType,
    ) -> Result<hickory_resolver::lookup::Lookup, hickory_resolver::error::ResolveError> {
        let ret = timeout(
            self.timeout,
            self.resolver.read().await.lookup(name, record_type),
        )
        .await;

        match ret {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    "dns resolver timeout after {} seconds: {e:?}",
                    self.timeout.as_secs()
                );
                *self.resolver.write().await = hickory_resolver::AsyncResolver::tokio(
                    self.config.clone(),
                    self.options.clone(),
                );
                Err(hickory_resolver::error::ResolveErrorKind::Timeout.into())
            }
        }
    }
}

pub trait FromConfig<P: ConnectionProvider>: Sized {
    type Config<'a>: Deserialize<'a>;
    type Error;

    fn from_config(
        config: Self::Config<'_>,
        upstream: &HashMap<&'_ str, Arc<Upstream<P>>>,
    ) -> Result<Self, Self::Error>;
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", bound = "'de:'a")]
pub enum ActionCfg<'a> {
    Block(block::Config),
    Fixed(fixed::Config),
    Forward(forward::Config<'a>),
    UnixSrvOrBlock {
        path: String,
        active: fixed::Config,
        inactive: block::Config,
    },
    UnixSrvOrForward {
        path: String,
        active: fixed::Config,
        forward: forward::Config<'a>,
    },
}

pub enum Action<P: ConnectionProvider> {
    Block(Block),
    Fixed(Fixed),
    Forward(Forward<P>),
    UnixSrvOrForward(UnixService<Fixed, Forward<P>>),
    UnixSrvOrBlock(UnixService<Fixed, Block>),
}
impl<P: ConnectionProvider> FromConfig<P> for Action<P> {
    type Config<'a> = ActionCfg<'a>;
    type Error = forward::UnknownUpstream;
    fn from_config(
        config: Self::Config<'_>,
        upstream: &HashMap<&'_ str, Arc<Upstream<P>>>,
    ) -> Result<Self, Self::Error> {
        match config {
            ActionCfg::Block(b) => Ok(Self::Block(b)),
            ActionCfg::Fixed(f) => Ok(Self::Fixed(f)),
            ActionCfg::Forward(f) => Ok(Self::Forward(Forward::from_config(f, upstream)?)),
            ActionCfg::UnixSrvOrBlock {
                path,
                active,
                inactive,
            } => Ok(Self::UnixSrvOrBlock(UnixService {
                path: PathBuf::from(path),
                active,
                inactive,
            })),
            ActionCfg::UnixSrvOrForward {
                path,
                active,
                forward,
            } => Ok(Self::UnixSrvOrForward(UnixService {
                path: PathBuf::from(path),
                active,
                inactive: Forward::from_config(forward, upstream)?,
            })),
        }
    }
}

impl RequestHandler for Action<TokioConnectionProvider> {
    fn handle_request<'life0, 'life1, 'async_trait, R>(
        &'life0 self,
        request: &'life1 hickory_server::server::Request,
        response_handle: R,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = ResponseInfo> + core::marker::Send + 'async_trait>,
    >
    where
        R: 'async_trait + hickory_server::server::ResponseHandler,
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        match self {
            Self::Block(b) => b.handle_request(request, response_handle),
            Self::Fixed(f) => f.handle_request(request, response_handle),
            Self::Forward(f) => f.handle_request(request, response_handle),
            Self::UnixSrvOrForward(s) => s.handle_request(request, response_handle),
            Self::UnixSrvOrBlock(s) => s.handle_request(request, response_handle),
        }
    }
}

pub struct ArcAction<A>(pub Arc<A>);
impl<A: RequestHandler> RequestHandler for ArcAction<A> {
    #[inline]
    fn handle_request<'life0, 'life1, 'async_trait, R>(
        &'life0 self,
        request: &'life1 hickory_server::server::Request,
        response_handle: R,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = ResponseInfo> + core::marker::Send + 'async_trait>,
    >
    where
        R: 'async_trait + hickory_server::server::ResponseHandler,
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        self.0.handle_request(request, response_handle)
    }
}
