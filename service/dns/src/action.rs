use std::{collections::HashMap, path::PathBuf, sync::Arc};

use hickory_resolver::name_server::ConnectionProvider;
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

pub struct Upstream<R: ConnectionProvider> {
    pub name: String,
    pub resolver: hickory_resolver::AsyncResolver<R>,
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

impl<P: ConnectionProvider> RequestHandler for Action<P> {
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
