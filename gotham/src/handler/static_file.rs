use handler::{Handler, HandlerFuture};
use state::State;
use hyper::StatusCode;
use http::response::create_response;
use futures::future;
use mime;

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
        let response = create_response(
            &state,
            StatusCode::Ok,
            Some((String::from("Hello static!").into_bytes(), mime::TEXT_PLAIN)),
        );
        Box::new(future::ok((state, response)))
    }
}
