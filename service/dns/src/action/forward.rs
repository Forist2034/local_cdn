use std::{
    fmt::Display,
    iter::{empty, once},
    sync::Arc,
};

use hickory_proto::{
    op::{Header, ResponseCode},
    rr::{rdata, RData, Record},
};
use hickory_resolver::{error::ResolveErrorKind, name_server::ConnectionProvider};
use hickory_server::{
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler},
};
use serde::Deserialize;
use tracing::Instrument;

use super::{FromConfig, Upstream};

#[derive(Deserialize)]
#[serde(bound = "'de:'a")]
pub struct Config<'a> {
    /// try upstream in order
    pub upstream: Vec<&'a str>,
}

pub struct Forward<R: ConnectionProvider> {
    pub resolvers: Vec<Arc<Upstream<R>>>,
}

#[derive(Debug)]
pub struct UnknownUpstream(String);
impl Display for UnknownUpstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown upstream: {}", self.0)
    }
}
impl std::error::Error for UnknownUpstream {}

impl<P: ConnectionProvider> FromConfig<P> for Forward<P> {
    type Config<'a> = Config<'a>;
    type Error = UnknownUpstream;
    fn from_config(
        config: Self::Config<'_>,
        upstream: &std::collections::HashMap<&'_ str, Arc<Upstream<P>>>,
    ) -> Result<Self, Self::Error> {
        let mut ret = Vec::with_capacity(config.upstream.len());
        for up in config.upstream {
            match upstream.get(up) {
                Some(s) => ret.push(Arc::clone(s)),
                None => return Err(UnknownUpstream(up.to_owned())),
            }
        }
        Ok(Self { resolvers: ret })
    }
}

impl<P: ConnectionProvider> RequestHandler for Forward<P> {
    fn handle_request<'life0, 'life1, 'async_trait, R>(
        &'life0 self,
        request: &'life1 Request,
        mut response_handle: R,
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
        let q = request.query();
        Box::pin(async move {
            let mut code = None;
            for r in self.resolvers.iter() {
                match r
                    .resolver
                    .lookup(q.name(), q.query_type())
                    .instrument(tracing::info_span!("resolver_lookup", upstream = r.name))
                    .await
                {
                    Ok(l) => {
                        tracing::debug!(upstream = r.name, "forwarded request to upstream");
                        return response_handle
                            .send_response(
                                MessageResponseBuilder::from_message_request(request).build(
                                    Header::response_from_request(request.header()),
                                    l.records(),
                                    empty(),
                                    empty(),
                                    once(&Record::from_rdata(
                                        q.name().into(),
                                        0,
                                        RData::TXT(rdata::TXT::new(Vec::from([format!(
                                            "upstream {}",
                                            r.name
                                        )]))),
                                    )),
                                ),
                            )
                            .await
                            .unwrap_or_else(crate::send_response_failed);
                    }
                    Err(e) => {
                        tracing::error!(
                            error = tracing::field::debug(e.clone()),
                            "failed to forward request to upstream {}",
                            r.name
                        );
                        if let ResolveErrorKind::NoRecordsFound { response_code, .. } = e.kind() {
                            code = Some(response_code.clone());
                        }
                    }
                }
            }
            tracing::error!("forward request to all upstream failed");
            response_handle
                .send_response(
                    MessageResponseBuilder::from_message_request(request)
                        .error_msg(request.header(), code.unwrap_or(ResponseCode::ServFail)),
                )
                .await
                .unwrap_or_else(crate::send_response_failed)
        })
    }
}
