use std::{fmt::Display, future::Future, pin::Pin};

use http::Uri;
use hyper::rt::{Read, Write};
use hyper_rustls::MaybeHttpsStream;
use hyper_util::{client::legacy::connect::Connection, rt::TokioIo};
use tower_service::Service;

#[derive(Debug)]
pub enum HttpsError {
    ExpectHttps,
    Inner(Box<dyn std::error::Error + Send + Sync>),
}
impl Display for HttpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExpectHttps => f.write_str("expect https connection"),
            Self::Inner(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for HttpsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ExpectHttps => None,
            Self::Inner(e) => Some(e.as_ref()),
        }
    }
}

type InnerStream<C> = TokioIo<tokio_rustls::client::TlsStream<TokioIo<C>>>;

pub struct HttpsStream<C>(InnerStream<C>);
impl<C: Unpin> HttpsStream<C> {
    fn inner(self: Pin<&mut Self>) -> Pin<&mut InnerStream<C>> {
        Pin::new(&mut Pin::get_mut(self).0)
    }
}
impl<T: Read + Write + Connection + Unpin> Connection for HttpsStream<T> {
    fn connected(&self) -> hyper_util::client::legacy::connect::Connected {
        let (tcp, tls) = self.0.inner().get_ref();
        if tls.alpn_protocol() == Some(b"h2") {
            tcp.inner().connected().negotiated_h2()
        } else {
            tcp.inner().connected()
        }
    }
}
impl<T: Read + Write + Unpin> Read for HttpsStream<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.inner().poll_read(cx, buf)
    }
}
impl<T: Read + Write + Unpin> Write for HttpsStream<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        self.inner().poll_write(cx, buf)
    }
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        self.inner().poll_write_vectored(cx, bufs)
    }
    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.inner().poll_flush(cx)
    }
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.inner().poll_shutdown(cx)
    }
    fn is_write_vectored(&self) -> bool {
        self.0.is_write_vectored()
    }
}

#[derive(Clone)]
pub struct Connector<T>(pub hyper_rustls::HttpsConnector<T>);
impl<T> Service<Uri> for Connector<T>
where
    T: Service<Uri>,
    T::Response: Connection + Read + Write + Send + Unpin + 'static,
    T::Future: Send + 'static,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = HttpsStream<T::Response>;
    type Error = HttpsError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx).map_err(HttpsError::Inner)
    }
    fn call(&mut self, req: Uri) -> Self::Future {
        let fut = self.0.call(req);
        Box::pin(async move {
            match fut.await {
                Ok(MaybeHttpsStream::Https(s)) => Ok(HttpsStream(s)),
                Ok(MaybeHttpsStream::Http(_)) => Err(HttpsError::ExpectHttps),
                Err(e) => Err(HttpsError::Inner(e)),
            }
        })
    }
}
