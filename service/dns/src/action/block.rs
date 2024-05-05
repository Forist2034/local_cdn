use std::{
    convert::Infallible,
    iter::{empty, once},
    net::{Ipv4Addr, Ipv6Addr},
};

use hickory_proto::{
    op::Header,
    rr::{rdata, RData, Record, RecordType},
};
use hickory_resolver::{name_server::ConnectionProvider, Name};
use hickory_server::{
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler},
};
use serde::Deserialize;

use super::FromConfig;

#[derive(Debug, Clone, Deserialize)]
pub struct Block {
    pub ttl: u32,
}

pub type Config = Block;
impl<P: ConnectionProvider> FromConfig<P> for Block {
    type Config<'a> = Self;
    type Error = Infallible;
    fn from_config(
        config: Self::Config<'_>,
        _: &std::collections::HashMap<&'_ str, std::sync::Arc<super::Upstream<P>>>,
    ) -> Result<Self, Self::Error> {
        Ok(config)
    }
}

impl RequestHandler for Block {
    fn handle_request<'life0, 'life1, 'async_trait, R>(
        &'life0 self,
        request: &'life1 Request,
        mut response_handle: R,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = hickory_server::server::ResponseInfo>
                + Send
                + 'async_trait,
        >,
    >
    where
        R: 'async_trait + hickory_server::server::ResponseHandler,
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        let name: Name = request.query().name().into();
        let resp = MessageResponseBuilder::from_message_request(request);
        Box::pin(async move {
            match request.query().query_type() {
                RecordType::A => {
                    response_handle
                        .send_response(resp.build(
                            Header::response_from_request(request.header()),
                            once(&Record::from_rdata(
                                name,
                                self.ttl,
                                RData::A(rdata::A(Ipv4Addr::UNSPECIFIED)),
                            )),
                            empty(),
                            empty(),
                            empty(),
                        ))
                        .await
                }
                RecordType::AAAA => {
                    response_handle
                        .send_response(resp.build(
                            Header::response_from_request(request.header()),
                            once(&Record::from_rdata(
                                name,
                                self.ttl,
                                RData::AAAA(rdata::AAAA(Ipv6Addr::UNSPECIFIED)),
                            )),
                            empty(),
                            empty(),
                            empty(),
                        ))
                        .await
                }
                _ => {
                    response_handle
                        .send_response(resp.build(
                            Header::response_from_request(request.header()),
                            empty(),
                            empty(),
                            empty(),
                            empty(),
                        ))
                        .await
                }
            }
            .unwrap_or_else(crate::send_response_failed)
        })
    }
}
