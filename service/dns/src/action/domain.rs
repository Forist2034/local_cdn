use std::{
    borrow::{Borrow, Cow},
    sync::Arc,
};

use hickory_proto::rr::domain::Name;
use hickory_resolver::name_server::ConnectionProvider;
use hickory_server::server::RequestHandler;
use radix_trie::{Trie, TrieKey};
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

#[derive(PartialEq, Eq)]
struct NameKey<'a>(Cow<'a, Name>);
impl<'a> TrieKey for NameKey<'a> {
    fn encode_bytes(&self) -> Vec<u8> {
        let mut ret = Vec::with_capacity(self.0.len());
        for l in self.0.iter().rev() {
            ret.push(b'.');
            ret.extend_from_slice(l);
        }
        ret
    }
}

pub struct DomainAction<A> {
    default: A,
    domains: Trie<NameKey<'static>, Arc<A>>,
}
impl<P: ConnectionProvider, A: FromConfig<P>> FromConfig<P> for DomainAction<A> {
    type Config<'a> = Config<A::Config<'a>>;
    type Error = A::Error;
    fn from_config(
        config: Self::Config<'_>,
        upstream: &std::collections::HashMap<&'_ str, Arc<super::Upstream<P>>>,
    ) -> Result<Self, Self::Error> {
        let mut domains = radix_trie::Trie::new();
        for cfg in config.actions {
            let act = Arc::new(A::from_config(cfg.action, upstream)?);
            for d in cfg.domains {
                domains.insert(NameKey(Cow::Owned(d)), Arc::clone(&act));
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
        match self
            .domains
            .get_ancestor_value(&NameKey(Cow::Borrowed(name)))
        {
            Some(a) => a.handle_request(request, response_handle),
            None => self.default.handle_request(request, response_handle),
        }
    }
}
