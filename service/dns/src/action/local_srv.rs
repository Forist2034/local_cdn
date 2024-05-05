use std::path::PathBuf;

use hickory_server::server::RequestHandler;

pub struct UnixService<A, I> {
    pub path: PathBuf,
    pub active: A,
    pub inactive: I,
}

impl<A: RequestHandler, I: RequestHandler> RequestHandler for UnixService<A, I> {
    #[inline]
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
        if self.path.exists() {
            self.active.handle_request(request, response_handle)
        } else {
            self.inactive.handle_request(request, response_handle)
        }
    }
}
