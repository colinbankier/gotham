use handler::{Handler, HandlerFuture, NewHandler};
use state::State;
use hyper::StatusCode;
use http::response::create_response;
use futures::future;
use mime;
use std::env::current_dir;
use std::io;

use hyper::Uri;

#[derive(Clone, Debug)]
pub struct StaticFileHandler {
    path: &'static str,
    uri_prefix: String,
}

impl StaticFileHandler {
    pub fn new(uri_prefix: String, path: &'static str) -> StaticFileHandler {
        StaticFileHandler {
            uri_prefix: uri_prefix,
            path: path,
        }
    }
}

impl NewHandler for StaticFileHandler {
    type Instance = Self;

    fn new_handler(&self) -> io::Result<Self::Instance> {
        Ok(StaticFileHandler {
            path: self.path,
            uri_prefix: self.uri_prefix.clone(),
        })
    }
}

impl Handler for StaticFileHandler {
    fn handle(self, state: State) -> Box<HandlerFuture> {
        let response = {
            let uri = state.try_borrow::<Uri>();
            let wd = current_dir().unwrap();
            trace!("{:?}", wd);

            let body = format!(
                "Got {:?} from uri {}, cwd {:?}",
                self,
                String::from(uri.unwrap().path()),
                wd
            );

            create_response(
                &state,
                StatusCode::Ok,
                Some((body.into_bytes(), mime::TEXT_PLAIN)),
            )
        };
        Box::new(future::ok((state, response)))
    }
}
