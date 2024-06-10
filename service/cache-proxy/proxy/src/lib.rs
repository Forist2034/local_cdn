use std::{
    fmt::Display,
    future::Future,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    task::Poll,
    time::SystemTime,
};

use futures_util::{future::BoxFuture, FutureExt};
use http::{header, uri::Authority, Request, Response, Uri};
use http_body_util::{BodyExt, Either, Empty, Full};
use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use hyper::body::{Bytes, Incoming};
use tower_http::{
    classify::MakeClassifier,
    decompression::Decompression,
    trace::{HttpMakeClassifier, MakeSpan, OnRequest, OnResponse, Trace},
};
use tower_layer::Layer;
use tower_service::Service;

pub mod connector;

fn should_cache_req<B>(req: &Request<B>) -> bool {
    if req.method() != http::Method::GET {
        return false;
    }
    let h = req.headers();
    if h.contains_key(header::AUTHORIZATION) {
        return false;
    }
    true
}

#[derive(Debug)]
pub enum ProxyError<E> {
    MissingHost,
    InvalidHost(header::HeaderValue, http::uri::InvalidUri),
    UnexpectedHost(http::uri::Authority),
    InvalidUri(http::uri::InvalidUriParts),
    InvalidPath(String, http::Error),
    Upstream(E),
    BoxedUpstream(tower_http::BoxError),
    ReadCache(cacache::Error),
    WriteCache(cacache::Error),
    Decode(ciborium::de::Error<io::Error>),
}
impl<E: Display> Display for ProxyError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingHost => f.write_str("missing host header"),
            Self::InvalidHost(h, e) => write!(f, "invalid host {h:?}: {e}"),
            Self::UnexpectedHost(h) => write!(f, "unexpected host {h}"),
            Self::InvalidUri(e) => write!(f, "invalid uri: {e}"),
            Self::InvalidPath(p, e) => write!(f, "invalid path {p:?}: {e}"),
            Self::Upstream(e) => write!(f, "failed to send request to upstream: {e}"),
            Self::BoxedUpstream(e) => write!(f, "failed to send request to upstream: {e}"),
            Self::ReadCache(e) => write!(f, "failed to read cache: {e}"),
            Self::WriteCache(e) => write!(f, "failed to write cache: {e}"),
            Self::Decode(e) => write!(f, "failed to decode cache entry: {e}"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for ProxyError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MissingHost => None,
            Self::InvalidHost(_, e) => Some(e),
            Self::UnexpectedHost(_) => None,
            Self::InvalidUri(e) => Some(e),
            Self::InvalidPath(_, e) => Some(e),
            Self::Upstream(s) => Some(s),
            Self::BoxedUpstream(e) => Some(e.as_ref()),
            Self::ReadCache(e) => Some(e),
            Self::WriteCache(e) => Some(e),
            Self::Decode(e) => Some(e),
        }
    }
}

fn add_uri_authority<E>(
    upstream_host: &Authority,
    mut pts: http::request::Parts,
) -> Result<http::request::Parts, ProxyError<E>> {
    let mut u = pts.uri.into_parts();
    let host = pts
        .headers
        .get(header::HOST)
        .ok_or(ProxyError::MissingHost)?;
    let host = Authority::try_from(host.as_bytes())
        .map_err(|e| ProxyError::InvalidHost(host.clone(), e))?;

    if &host != upstream_host {
        return Err(ProxyError::UnexpectedHost(host));
    }
    u.scheme = Some(http::uri::Scheme::HTTPS);
    u.authority = Some(host);
    pts.uri = Uri::from_parts(u).map_err(ProxyError::InvalidUri)?;
    Ok(pts)
}

#[pin_project::pin_project(project = Proj)]
pub enum ProxyFuture<F, E> {
    Forward(#[pin] F),
    Boxed(BoxFuture<'static, Result<CachedResponse, ProxyError<E>>>),
    Ready(Option<Result<CachedResponse, ProxyError<E>>>),
}
impl<F, E> ProxyFuture<F, E> {
    fn cached(mut pts: http::response::Parts, body: Bytes) -> Self {
        pts.headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
        Self::Ready(Some(Ok(Response::from_parts(
            pts,
            Either::Right(Full::new(body)),
        ))))
    }
    fn ready_err(err: ProxyError<E>) -> Self {
        Self::Ready(Some(Err(err)))
    }
}
impl<F, E> Future for ProxyFuture<F, E>
where
    F: Future<Output = Result<CachedResponse, ProxyError<E>>>,
{
    type Output = Result<CachedResponse, ProxyError<E>>;
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            Proj::Forward(f) => f.poll(cx),
            Proj::Boxed(b) => b.as_mut().poll(cx),
            Proj::Ready(e) => Poll::Ready(e.take().unwrap()),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    policy: CachePolicy,
    body: Bytes,
}

type ClassifyEos = <HttpMakeClassifier as MakeClassifier>::ClassifyEos;
type Classifier = <HttpMakeClassifier as MakeClassifier>::Classifier;

pub type UpstreamBody = Either<Incoming, Empty<Bytes>>;
pub type CachedBody = Either<tower_http::trace::ResponseBody<Incoming, ClassifyEos>, Full<Bytes>>;
pub type CachedResponse = Response<CachedBody>;

#[derive(Clone, Copy)]
struct ForwardMkSpan;
impl<B> MakeSpan<B> for ForwardMkSpan {
    fn make_span(&mut self, request: &Request<B>) -> tracing::Span {
        tracing::info_span!("forwarding", method = %request.method(), uri = %request.uri(),)
    }
}

#[derive(Clone, Copy)]
struct ForwardOnRequest;
impl<B> OnRequest<B> for ForwardOnRequest {
    fn on_request(&mut self, request: &Request<B>, _: &tracing::Span) {
        tracing::info!("start request");
        tracing::debug!(headers = ?request.headers(), "request headers");
    }
}

#[derive(Clone, Copy)]
pub struct ForwardOnResponse {}
impl<B> OnResponse<B> for ForwardOnResponse {
    fn on_response(
        self,
        response: &Response<B>,
        latency: std::time::Duration,
        span: &tracing::Span,
    ) {
        tower_http::trace::DefaultOnResponse::new()
            .level(tracing::Level::INFO)
            .include_headers(tracing::enabled!(tracing::Level::DEBUG))
            .on_response(response, latency, span)
    }
}

#[derive(Clone, Copy)]
struct UpstreamMkSpan;
impl<B> MakeSpan<B> for UpstreamMkSpan {
    fn make_span(&mut self, request: &Request<B>) -> tracing::Span {
        tracing::info_span!(
            "upstream",
            uri = %request.uri(),
            headers = ?request.headers()
        )
    }
}

#[derive(Clone)]
pub struct CacheProxy<S> {
    root: Arc<Path>,
    authority: Arc<Authority>,
    forwarded: Trace<S, HttpMakeClassifier, ForwardMkSpan, ForwardOnRequest, ForwardOnResponse>,
    upstream: Decompression<Trace<S, HttpMakeClassifier, UpstreamMkSpan>>,
}

type IncomingReq = Request<Incoming>;
type IncomingResp = Response<Incoming>;

type ForwardedBody = tower_http::trace::ResponseBody<Incoming, ClassifyEos>;
type ForwardFn<E> = fn(Result<Response<ForwardedBody>, E>) -> Result<CachedResponse, ProxyError<E>>;
type ForwardFuture<F, E> = futures_util::future::Map<
    tower_http::trace::ResponseFuture<F, Classifier, ForwardOnResponse>,
    ForwardFn<E>,
>;

impl<S: Clone> CacheProxy<S> {
    fn with_path(root: Arc<Path>, authority: Arc<Authority>, upstream: S) -> Self {
        Self {
            root,
            authority,
            forwarded: Trace::new_for_http(upstream.clone())
                .make_span_with(ForwardMkSpan)
                .on_request(ForwardOnRequest)
                .on_response(ForwardOnResponse {}),
            upstream: Decompression::new(
                Trace::new_for_http(upstream)
                    .make_span_with(UpstreamMkSpan)
                    .on_request(
                        tower_http::trace::DefaultOnRequest::new().level(tracing::Level::INFO),
                    )
                    .on_response(
                        tower_http::trace::DefaultOnResponse::new()
                            .level(tracing::Level::INFO)
                            .include_headers(true),
                    ),
            ),
        }
    }
    pub fn new(root: PathBuf, authority: Authority, upstream: S) -> Self {
        Self::with_path(
            Arc::from(root.into_boxed_path()),
            Arc::new(authority),
            upstream,
        )
    }
}
impl<S> CacheProxy<S> {
    fn write_entry(&self, key: &str, entry: &CacheEntry) -> Result<(), cacache::Error> {
        let mut buf = Vec::new();
        ciborium::into_writer(entry, &mut buf).unwrap();
        cacache::write_sync(&self.root, key, buf)?;
        Ok(())
    }
}
impl<S> CacheProxy<S>
where
    S: Service<Request<UpstreamBody>, Response = IncomingResp>,
    S::Error: Display + 'static,
{
    fn forward(
        &mut self,
        req: IncomingReq,
    ) -> ProxyFuture<ForwardFuture<S::Future, S::Error>, S::Error> {
        tracing::warn!("forwarding request to upstream");
        let (pts, body) = req.into_parts();
        ProxyFuture::Forward(
            self.forwarded
                .call(Request::from_parts(
                    match add_uri_authority(&self.authority, pts) {
                        Ok(v) => v,
                        Err(e) => return ProxyFuture::ready_err(e),
                    },
                    Either::Left(body),
                ))
                .map(|r| match r {
                    Ok(resp) => {
                        let (pts, body) = resp.into_parts();
                        Ok(Response::from_parts(pts, Either::Left(body)))
                    }
                    Err(e) => Err(ProxyError::Upstream(e)),
                }),
        )
    }
    fn cached_or_forward(
        &mut self,
        entry: CacheEntry,
        orig_req: IncomingReq,
        req: http::request::Parts,
    ) -> ProxyFuture<ForwardFuture<S::Future, S::Error>, S::Error> {
        match entry.policy.before_request(&req, SystemTime::now()) {
            BeforeRequest::Fresh(pts) => {
                tracing::debug!("using response from cache");
                ProxyFuture::cached(pts, entry.body)
            }
            BeforeRequest::Stale { .. } => {
                tracing::warn!("cached response can't be used, forward request to upstream");
                self.forward(orig_req)
            }
        }
    }
    async fn req_upstream(
        &mut self,
        mut req: http::request::Parts,
    ) -> Result<(http::response::Parts, Bytes), ProxyError<S::Error>> {
        {
            let mut uri = req.uri.into_parts();
            uri.scheme = Some(http::uri::Scheme::HTTPS);
            uri.authority = Some(self.authority.as_ref().clone());
            req.uri = Uri::from_parts(uri).map_err(ProxyError::InvalidUri)?;
        }
        req.headers
            .insert(header::USER_AGENT, header::HeaderValue::from_static("curl"));
        let (mut pts, body) = self
            .upstream
            .call(Request::from_parts(req, Either::Right(Empty::new())))
            .await
            .map_err(ProxyError::Upstream)?
            .into_parts();
        pts.headers.remove(header::CONTENT_ENCODING);
        Ok((
            pts,
            body.collect()
                .await
                .map_err(ProxyError::BoxedUpstream)?
                .to_bytes(),
        ))
    }
    async fn update_entry(
        &mut self,
        key: &str,
        entry: CacheEntry,
    ) -> Result<CacheEntry, ProxyError<S::Error>> {
        match entry.policy.before_request(
            &Request::get(key)
                .header(header::HOST, self.authority.as_str())
                .body(())
                .map_err(|e| ProxyError::InvalidPath(key.to_string(), e))?,
            SystemTime::now(),
        ) {
            BeforeRequest::Fresh(_) => {
                tracing::warn!("cached response is fresh but can't be used");
                Ok(entry)
            }
            BeforeRequest::Stale { request, .. } => {
                tracing::info!("revalidating cached response");
                let (resp, upd_body) = self.req_upstream(request.clone()).await?;
                let entry = match entry
                    .policy
                    .after_response(&request, &resp, SystemTime::now())
                {
                    AfterResponse::Modified(cp, _) => {
                        tracing::debug!("response is updated");
                        CacheEntry {
                            policy: cp,
                            body: upd_body,
                        }
                    }
                    AfterResponse::NotModified(cp, _) => {
                        tracing::debug!("response is not modified");
                        CacheEntry {
                            policy: cp,
                            body: entry.body,
                        }
                    }
                };
                self.write_entry(key, &entry)
                    .map_err(ProxyError::WriteCache)?;
                Ok(entry)
            }
        }
    }
    async fn get_missing(
        &mut self,
        key: &str,
        uri: &Uri,
    ) -> Result<CacheEntry, ProxyError<S::Error>> {
        tracing::info!(key, "get response from remote");
        let upstream_req = Request::get(uri)
            .header(header::HOST, self.authority.as_str())
            .body(())
            .map_err(|e| ProxyError::InvalidPath(uri.to_string(), e))?
            .into_parts()
            .0;
        let (pts, body) = self.req_upstream(upstream_req.clone()).await?;
        let entry = CacheEntry {
            policy: CachePolicy::new(&upstream_req, &pts),
            body,
        };
        self.write_entry(key, &entry)
            .map_err(ProxyError::WriteCache)?;
        Ok(entry)
    }
}

fn cache_key(req: &http::request::Parts) -> &str {
    req.uri.path_and_query().map_or("", |p| p.as_str())
}

pub struct CacheLayer(Arc<Path>, Arc<Authority>);
impl CacheLayer {
    pub fn new(root: PathBuf, authority: Authority) -> Self {
        Self(Arc::from(root.into_boxed_path()), Arc::new(authority))
    }
}

impl<S: Clone> Layer<S> for CacheLayer {
    type Service = CacheProxy<S>;
    fn layer(&self, inner: S) -> Self::Service {
        CacheProxy::with_path(Arc::clone(&self.0), Arc::clone(&self.1), inner)
    }
}

impl<S, E> Service<Request<Incoming>> for CacheProxy<S>
where
    S: Clone + Send + 'static,
    S: Service<Request<UpstreamBody>, Response = Response<Incoming>, Error = E>,
    S::Future: Send,
    E: Display + Send + 'static,
{
    type Response = CachedResponse;
    type Error = ProxyError<E>;
    type Future = ProxyFuture<ForwardFuture<S::Future, E>, E>;
    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.forwarded.poll_ready(cx).map_err(ProxyError::Upstream)
    }
    fn call(&mut self, req: Request<Incoming>) -> Self::Future {
        if !should_cache_req(&req) {
            return self.forward(req);
        }
        let (req, orig_req) = {
            let (pts, body) = req.into_parts();

            let mut norm_pts = pts.clone();
            norm_pts.headers.remove(header::ACCEPT_ENCODING);

            (norm_pts, Request::from_parts(pts, body))
        };
        tracing::debug!(key = cache_key(&req), "cache key");
        tracing::debug!(req = ?req, "normalized request");

        match cacache::read_sync(&self.root, cache_key(&req)) {
            Ok(v) => {
                let entry: CacheEntry = match ciborium::from_reader(v.as_slice()) {
                    Ok(v) => v,
                    Err(e) => return ProxyFuture::ready_err(ProxyError::Decode(e)),
                };
                if !entry.policy.is_storable() {
                    tracing::warn!("request is not storable");
                    return self.forward(orig_req);
                }
                match entry.policy.before_request(&req, SystemTime::now()) {
                    BeforeRequest::Fresh(pts) => {
                        tracing::debug!("use cached response");
                        ProxyFuture::cached(pts, entry.body)
                    }
                    BeforeRequest::Stale { matches: false, .. } => {
                        tracing::warn!("cached response does not match request");
                        self.forward(orig_req)
                    }
                    BeforeRequest::Stale { matches: true, .. } => {
                        let mut cloned_self = self.clone();
                        ProxyFuture::Boxed(
                            async move {
                                let key = cache_key(&req);
                                let entry = cloned_self.update_entry(key, entry).await?;
                                cloned_self.cached_or_forward(entry, orig_req, req).await
                            }
                            .boxed(),
                        )
                    }
                }
            }
            Err(cacache::Error::EntryNotFound(_, _)) => {
                let mut cloned_self = self.clone();
                ProxyFuture::Boxed(
                    async move {
                        /* if request authority does not match self.authority,
                           a request to upstream is still sent, but response will
                           not be used and return an error
                        */
                        let entry = cloned_self.get_missing(cache_key(&req), &req.uri).await?;
                        cloned_self.cached_or_forward(entry, orig_req, req).await
                    }
                    .boxed(),
                )
            }
            Err(e) => ProxyFuture::ready_err(ProxyError::ReadCache(e)),
        }
    }
}
