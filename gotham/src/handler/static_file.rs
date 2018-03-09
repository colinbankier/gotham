use handler::{Handler, HandlerFuture, NewHandler};
use state::State;
use hyper::StatusCode;
use http::response::create_response;
use futures::future;
use mime;
use std::env::current_dir;
use std::io;
use std::path::{Component, PathBuf, Path};
use url::percent_encoding::percent_decode;


use hyper::Uri;

#[derive(Clone, Debug)]
pub struct StaticFileHandler {
    path: &'static str,
    uri_prefix: &'static str,
}

impl StaticFileHandler {
    pub fn new(uri_prefix: &'static str, path: &'static str) -> StaticFileHandler {
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
            let uri = state.try_borrow::<Uri>().unwrap();
            let decoded_path = percent_decode(uri.path().as_bytes()).decode_utf8().unwrap().into_owned();
            let req_path = Path::new(&decoded_path);
            let mut path = PathBuf::from(self.path);
            path.extend(&normalize_path(req_path).strip_prefix(self.uri_prefix));

            let wd = current_dir().unwrap();
            trace!("{:?}", wd);


            let body = format!(
                "Got {:?}, serving {:?} from cwd {:?}",
                self,
                path,
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

fn normalize_path(path: &Path) -> PathBuf {
    path.components().fold(PathBuf::new(), |mut result, p| {
        match p {
            Component::Normal(x) => {
                result.push(x);
                result
            }
            Component::ParentDir => {
                result.pop();
                result
            },
            _ => result
        }
    })
}
