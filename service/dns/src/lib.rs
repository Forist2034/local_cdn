pub mod config {
    use std::{collections::HashMap, net::SocketAddr};

    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ResolverConfig {
        Google,
        GoogleTls,
        GoogleHttps,
        Cloudflare,
        CloudflareTls,
        CloudflareHttps,
        Quad9,
        Quad9Tls,
        Quad9Https,
        Custom(hickory_resolver::config::ResolverConfig),
    }
    impl From<ResolverConfig> for hickory_resolver::config::ResolverConfig {
        fn from(value: ResolverConfig) -> Self {
            match value {
                ResolverConfig::Google => Self::google(),
                ResolverConfig::GoogleTls => Self::google_tls(),
                ResolverConfig::GoogleHttps => Self::google_https(),
                ResolverConfig::Cloudflare => Self::cloudflare(),
                ResolverConfig::CloudflareTls => Self::cloudflare_tls(),
                ResolverConfig::CloudflareHttps => Self::cloudflare_https(),
                ResolverConfig::Quad9 => Self::quad9(),
                ResolverConfig::Quad9Tls => Self::quad9_tls(),
                ResolverConfig::Quad9Https => Self::quad9_https(),
                ResolverConfig::Custom(c) => c,
            }
        }
    }

    #[derive(Deserialize)]
    pub struct Upstream {
        #[serde(default)]
        pub options: hickory_resolver::config::ResolverOpts,
        pub config: ResolverConfig,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Listen {
        Udp(SocketAddr),
        Tcp {
            address: SocketAddr,
            timeout_sec: u16,
        },
    }

    #[derive(Deserialize)]
    #[serde(bound = "'de:'a")]
    pub struct Server<'a> {
        pub action: crate::action::domain::Config<crate::action::ActionCfg<'a>>,
        pub listen: Vec<Listen>,
    }

    #[derive(Default, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum LogLevel {
        Off,
        Error,
        Warn,
        #[default]
        Info,
        Debug,
        Trace,
    }

    #[derive(Deserialize)]
    #[serde(bound = "'de:'a")]
    pub struct Config<'a> {
        #[serde(default)]
        pub log_level: LogLevel,
        #[serde(default)]
        pub json_log: Option<&'a str>,
        pub upstream: HashMap<&'a str, Upstream>,
        pub servers: HashMap<&'a str, Server<'a>>,
    }
}

pub mod action;

fn failed_response_info() -> hickory_server::server::ResponseInfo {
    let mut header = hickory_proto::op::Header::new();
    header.set_response_code(hickory_proto::op::ResponseCode::ServFail);
    hickory_server::server::ResponseInfo::from(header)
}
fn send_response_failed<E: std::error::Error>(error: E) -> hickory_server::server::ResponseInfo {
    tracing::error!(
        error = tracing::field::debug(error),
        "failed to send response"
    );
    failed_response_info()
}

pub mod server {
    use hickory_proto::{
        op::{Header, MessageType, OpCode, ResponseCode},
        rr::DNSClass,
    };
    use hickory_server::{authority::MessageResponseBuilder, server::RequestHandler};

    pub struct InQueryHandler<A>(pub A);
    impl<A: RequestHandler> RequestHandler for InQueryHandler<A> {
        fn handle_request<'life0, 'life1, 'async_trait, R>(
            &'life0 self,
            request: &'life1 hickory_server::server::Request,
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
            if request.message_type() != MessageType::Query {
                return Box::pin(async move {
                    response_handle
                        .send_response(
                            MessageResponseBuilder::from_message_request(request)
                                .error_msg(request.header(), ResponseCode::FormErr),
                        )
                        .await
                        .unwrap_or_else(crate::send_response_failed)
                });
            }
            if request.op_code() != OpCode::Query {
                return Box::pin(async move {
                    response_handle
                        .send_response(
                            MessageResponseBuilder::from_message_request(request)
                                .error_msg(request.header(), ResponseCode::NotImp),
                        )
                        .await
                        .unwrap_or_else(crate::send_response_failed)
                });
            }
            let r = request.query();
            if r.query_class() != DNSClass::IN {
                return Box::pin(async move {
                    response_handle
                        .send_response(
                            MessageResponseBuilder::from_message_request(request)
                                .build_no_records(Header::response_from_request(request.header())),
                        )
                        .await
                        .unwrap_or_else(crate::send_response_failed)
                });
            }
            self.0.handle_request(request, response_handle)
        }
    }
}
