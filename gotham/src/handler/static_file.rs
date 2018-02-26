use handler::{Handler, HandlerFuture};
use state::State;
use hyper::StatusCode;
use http::response::create_response;
use futures::future;
use mime;

use hyper::Uri;

pub struct StaticFileHandler {
    path: &'static str,
}

impl StaticFileHandler {
    pub fn new(path: &'static str) -> StaticFileHandler {
        StaticFileHandler { path: path }
    }
}

impl Handler for StaticFileHandler {
    fn handle(self, state: State) -> Box<HandlerFuture> {
        let response = {
            let uri = state.try_borrow::<Uri>();
            create_response(
                &state,
                StatusCode::Ok,
                Some((
                    String::from(uri.unwrap().path()).into_bytes(),
                    mime::TEXT_PLAIN,
                )),
            )
        };
        Box::new(future::ok((state, response)))
    }
}
