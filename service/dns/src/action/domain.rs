use std::{borrow::Borrow, collections::HashMap, sync::Arc};

use hickory_proto::rr::domain::Name;
use hickory_resolver::name_server::ConnectionProvider;
use hickory_server::server::RequestHandler;
use serde::Deserialize;

use super::FromConfig;

#[derive(Deserialize)]
pub struct DomainConfig<A> {
    pub domains: Vec<Name>,
    pub action: A,
}

#[derive(Deserialize)]
pub struct Config<A> {
    pub default_action: A,
    pub actions: Vec<DomainConfig<A>>,
}

struct NameTree<A> {
    value: Option<A>,
    child: HashMap<Box<[u8]>, NameTree<A>>,
}
impl<A> NameTree<A> {
    fn new() -> Self {
        Self {
            value: None,
            child: HashMap::new(),
        }
    }
    fn insert(&mut self, name: &Name, value: A) {
        let mut pos = self;
        for l in name.iter().rev() {
            pos = pos
                .child
                .entry(l.to_vec().into_boxed_slice())
                .or_insert_with(|| Self::new());
        }
        pos.value = Some(value)
    }
    fn get(&self, name: &Name) -> Option<&A> {
        let mut pos = self;
        let mut ret = self.value.as_ref();
        for l in name.iter().rev() {
            match pos.child.get(l) {
                Some(v) => {
                    pos = v;
                    if let Some(val) = &v.value {
                        ret = Some(val);
                    }
                }
                None => break,
            }
        }
        ret
    }
}

pub struct DomainAction<A> {
    default: A,
    domains: NameTree<Arc<A>>,
}
impl<P: ConnectionProvider, A: FromConfig<P>> FromConfig<P> for DomainAction<A> {
    type Config<'a> = Config<A::Config<'a>>;
    type Error = A::Error;
    fn from_config(
        config: Self::Config<'_>,
        upstream: &std::collections::HashMap<&'_ str, Arc<super::Upstream<P>>>,
    ) -> Result<Self, Self::Error> {
        let mut domains = NameTree::new();
        for cfg in config.actions {
            let act = Arc::new(A::from_config(cfg.action, upstream)?);
            for d in cfg.domains {
                domains.insert(&d, Arc::clone(&act));
            }
        }
        Ok(Self {
            default: A::from_config(config.default_action, upstream)?,
            domains,
        })
    }
}
impl<A: RequestHandler> RequestHandler for DomainAction<A> {
    fn handle_request<'life0, 'life1, 'async_trait, R>(
        &'life0 self,
        request: &'life1 hickory_server::server::Request,
        response_handle: R,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = hickory_server::server::ResponseInfo>
                + core::marker::Send
                + 'async_trait,
        >,
    >
    where
        R: 'async_trait + hickory_server::server::ResponseHandler,
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        let name: &Name = request.query().name().borrow();
        match self.domains.get(&name) {
            Some(a) => a.handle_request(request, response_handle),
            None => self.default.handle_request(request, response_handle),
        }
    }
}
