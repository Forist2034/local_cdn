use std::{convert::Infallible, iter::empty};

use hickory_proto::{
    op::Header,
    rr::{RData, Record},
};
use hickory_resolver::{name_server::ConnectionProvider, Name};
use hickory_server::{
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler},
};
use serde::Deserialize;

use super::FromConfig;

#[derive(Debug, Clone, Deserialize)]
pub struct Fixed {
    pub ttl: u32,
    pub data: Vec<RData>,
}

pub type Config = Fixed;
impl<P: ConnectionProvider> FromConfig<P> for Fixed {
    type Config<'a> = Self;
    type Error = Infallible;
    fn from_config(
        config: Self::Config<'_>,
        _: &std::collections::HashMap<&'_ str, std::sync::Arc<super::Upstream<P>>>,
    ) -> Result<Self, Self::Error> {
        Ok(config)
    }
}

impl RequestHandler for Fixed {
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
        Box::pin(async move {
            let name: Name = request.query().name().into();
            response_handle
                .send_response(
                    MessageResponseBuilder::from_message_request(request).build(
                        Header::response_from_request(request.header()),
                        self.data
                            .iter()
                            .map(|d| Record::from_rdata(name.clone(), self.ttl, d.clone()))
                            .collect::<Vec<_>>()
                            .iter(),
                        empty(),
                        empty(),
                        empty(),
                    ),
                )
                .await
                .unwrap_or_else(crate::send_response_failed)
        })
    }
}
