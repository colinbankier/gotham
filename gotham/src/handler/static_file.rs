use futures::future::{self, Either};
use futures::Future;
use handler::{Handler, HandlerFuture, IntoHandlerError, NewHandler};
use helpers::http::response::{create_response, extend_response};
use hyper::header::{
    AcceptEncoding, ETag, Encoding, EntityTag, Headers, HttpDate, IfModifiedSince, IfNoneMatch,
    LastModified, QualityItem,
};
use hyper::Response;
use hyper::StatusCode;
use mime::{self, Mime};
use mime_guess::guess_mime_type_opt;
use router::response::extender::StaticResponseExtender;
use state::{FromState, State, StateData};
use std::convert::From;
use std::fs::Metadata;
use std::io;
use std::iter::FromIterator;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;
use tokio_fs;
use tokio_io;

/// Represents a handler for any files under the path `root`.
#[derive(Clone)]
pub struct FileSystemHandler {
    root: PathBuf,
}

/// Represents a handler for a single file at `path`.
#[derive(Clone)]
pub struct FileHandler {
    path: PathBuf,
}

impl FileHandler {
    /// Create a new `FileHandler` for the given path.
    pub fn new<P: AsRef<Path>>(path: P) -> FileHandler
    where
        PathBuf: From<P>,
    {
        FileHandler {
            path: PathBuf::from(path),
        }
    }
}

impl FileSystemHandler {
    /// Create a new `FileSystemHandler` with the given root path.
    pub fn new<P: AsRef<Path>>(root: P) -> FileSystemHandler
    where
        PathBuf: From<P>,
    {
        FileSystemHandler {
            root: PathBuf::from(root),
        }
    }
}

impl NewHandler for FileHandler {
    type Instance = Self;

    fn new_handler(&self) -> io::Result<Self::Instance> {
        Ok(self.clone())
    }
}

impl NewHandler for FileSystemHandler {
    type Instance = Self;

    fn new_handler(&self) -> io::Result<Self::Instance> {
        Ok(self.clone())
    }
}

impl Handler for FileSystemHandler {
    fn handle(self, state: State) -> Box<HandlerFuture> {
        let path = {
            let mut base_path = PathBuf::from(self.root);
            let file_path = PathBuf::from_iter(&FilePathExtractor::borrow_from(&state).parts);
            base_path.extend(&normalize_path(&file_path));
            base_path
        };
        create_file_response(path, state)
    }
}

impl Handler for FileHandler {
    fn handle(self, state: State) -> Box<HandlerFuture> {
        create_file_response(self.path, state)
    }
}

enum FileResult {
    NotModified,
    Contents(Vec<u8>, Metadata),
}

// Serve a file by asynchronously reading it entirely into memory.
// Uses tokio_fs to open file asynchronously, then tokio_io to read into
// memory asynchronously.
fn create_file_response(path: PathBuf, state: State) -> Box<HandlerFuture> {
    let (if_none_match, if_modified_since, accept_encoding) = extract_headers(&state);

    // be sure to check content type remains, and set content-encoding
    let file_path = check_compressed_files(accept_encoding);
    let mime_type = mime_for_path(&file_path);

    let data_future = tokio_fs::file::File::open(path)
        .and_then(|file| file.metadata())
        .and_then(move |(file, meta)| {
            if not_modified(&meta, if_none_match, if_modified_since) {
                Either::A(future::ok(FileResult::NotModified))
            } else {
                let contents = Vec::with_capacity(meta.len() as usize);
                Either::B(
                    tokio_io::io::read_to_end(file, contents)
                        .and_then(move |item| Ok(FileResult::Contents(item.1, meta))),
                )
            }
        });
    Box::new(data_future.then(move |result| match result {
        Ok(FileResult::Contents(data, metadata)) => {
            let res = create_response(&state, StatusCode::Ok, Some((data, mime_type)));
            Ok((state, append_headers(res, &metadata)))
        }
        Ok(FileResult::NotModified) => {
            let res = create_response(&state, StatusCode::NotModified, None);
            Ok((state, res))
        }
        Err(err) => {
            let status = error_status(&err);
            Err((state, err.into_handler_error().with_status(status)))
        }
    }))
}

fn check_compressed_files(
    path: PathBuf,
    accept: Option<AcceptEncoding>,
) -> Option<(PathBuf, Encoding)> {
    match accept {
        Some(AcceptEncoding(items)) => {
            let supported_encodings = vec![Encoding::Gzip, Encoding::Brotli];
            let accept_items: Vec<&QualityItem<Encoding>> = items
                .iter()
                .filter(|e| supported_encodings.contains(&e.item))
                .collect();
            accept_items.sort_by_key(|i| i.quality);
            accept_items
                .iter()
                .filter_map(|i| match encoding_extension(i.item) {
                    Some(ext) => Some((path.with_extension(ext), i.item)),
                    _ => None,
                })
                .filter(|(path, encoding)| Path.exists(path))
                .take(1)
        }
    }
}

fn encoding_extension(encoding: &Encoding) -> Option<String> {
    match encoding {
        Encoding::Gzip => Some(".gz".to_string()),
        Encoding::Brotli => Some(".br".to_string()),
        _ => None,
    }
}

fn extract_headers(
    state: &State,
) -> (
    Option<IfNoneMatch>,
    Option<IfModifiedSince>,
    Option<AcceptEncoding>,
) {
    let headers: &Headers = Headers::borrow_from(&state);
    let if_none_match = headers.get::<IfNoneMatch>().map(|h| h.clone());
    let if_modified_since = headers.get::<IfModifiedSince>().map(|h| h.clone());
    let accept_encoding = headers.get::<AcceptEncoding>().map(|h| h.clone());
    (if_none_match, if_modified_since, accept_encoding)
}

fn not_modified(
    metadata: &Metadata,
    if_none_match: Option<IfNoneMatch>,
    if_modified_since: Option<IfModifiedSince>,
) -> bool {
    // If-None-Match header takes precedence over If-Modified-Since
    if let Some(IfNoneMatch::Items(items)) = if_none_match {
        entity_tag(&metadata)
            .map(|etag| items.contains(&etag))
            .unwrap_or(false)
    } else if let Some(IfModifiedSince(if_modified_time)) = if_modified_since {
        metadata
            .modified()
            .map(|modified| HttpDate::from(modified) <= if_modified_time)
            .unwrap_or(false)
    } else {
        false
    }
}

fn entity_tag(metadata: &Metadata) -> Option<EntityTag> {
    metadata.modified().ok().and_then(|modified| {
        modified.duration_since(UNIX_EPOCH).ok().map(|duration| {
            EntityTag::weak(format!(
                "{0:x}-{1:x}.{2:x}",
                metadata.len(),
                duration.as_secs(),
                duration.subsec_nanos()
            ))
        })
    })
}

fn append_headers(res: Response, metadata: &Metadata) -> Response {
    let res = match entity_tag(metadata) {
        Some(tag) => res.with_header(ETag(tag)),
        _ => res,
    };
    match metadata.modified() {
        Ok(modified) => res.with_header(LastModified(modified.into())),
        _ => res,
    }
}

fn mime_for_path(path: &Path) -> Mime {
    guess_mime_type_opt(path).unwrap_or_else(|| mime::APPLICATION_OCTET_STREAM)
}

fn normalize_path(path: &Path) -> PathBuf {
    path.components()
        .fold(PathBuf::new(), |mut result, p| match p {
            Component::Normal(x) => {
                result.push(x);
                result
            }
            Component::ParentDir => {
                result.pop();
                result
            }
            _ => result,
        })
}

fn error_status(e: &io::Error) -> StatusCode {
    match e.kind() {
        io::ErrorKind::NotFound => StatusCode::NotFound,
        io::ErrorKind::PermissionDenied => StatusCode::Forbidden,
        _ => StatusCode::InternalServerError,
    }
}

/// Responsible for extracting the file path matched by the glob segment from the URL.
#[derive(Debug, Deserialize)]
pub struct FilePathExtractor {
    #[serde(rename = "*")]
    parts: Vec<String>,
}

impl StateData for FilePathExtractor {}

impl StaticResponseExtender for FilePathExtractor {
    fn extend(state: &mut State, res: &mut Response) {
        extend_response(state, res, StatusCode::BadRequest, None);
    }
}

#[cfg(test)]
mod tests {
    use hyper::header::{ContentType, ETag, HttpDate, IfModifiedSince, IfNoneMatch, LastModified};
    use hyper::StatusCode;
    use mime;
    use router::builder::{build_simple_router, DefineSingleRoute, DrawRoutes};
    use router::Router;
    use std::str;
    use std::time::SystemTime;
    use test::TestServer;

    #[test]
    fn static_files_guesses_content_type() {
        let expected_docs = vec![
            ("doc.html", mime::TEXT_HTML, "<html>I am a doc.</html>"),
            ("file.txt", mime::TEXT_PLAIN, "I am a file"),
            (
                "styles/style.css",
                mime::TEXT_CSS,
                ".styled { border: none; }",
            ),
            (
                "scripts/script.js",
                "application/javascript".parse().unwrap(),
                "console.log('I am javascript!');",
            ),
        ];

        for doc in expected_docs {
            let response = test_server()
                .client()
                .get(&format!("http://localhost/{}", doc.0))
                .perform()
                .unwrap();

            assert_eq!(response.status(), StatusCode::Ok);
            assert_eq!(
                response.headers().get::<ContentType>().unwrap(),
                &ContentType(doc.1)
            );

            let body = response.read_body().unwrap();
            assert_eq!(&body[..], doc.2.as_bytes());
        }
    }

    // Examples derived from https://www.owasp.org/index.php/Path_Traversal
    #[test]
    fn static_path_traversal() {
        let traversal_attempts = vec![
            r"../private_files/secret.txt",
            r"%2e%2e%2fprivate_files/secret.txt",
            r"%2e%2e/private_files/secret.txt",
            r"..%2fprivate_files/secret.txt",
            r"%2e%2e%5cprivate_files/secret.txt",
            r"%2e%2e\private_files/secret.txt",
            r"..%5cprivate_files/secret.txt",
            r"%252e%252e%255cprivate_files/secret.txt",
            r"..%255cprivate_files/secret.txt",
            r"..%c0%afprivate_files/secret.txt",
            r"..%c1%9cprivate_files/secret.txt",
            "/etc/passwd",
        ];
        for attempt in traversal_attempts {
            let response = test_server()
                .client()
                .get(&format!("http://localhost/{}", attempt))
                .perform()
                .unwrap();

            assert_eq!(response.status(), StatusCode::NotFound);
        }
    }

    #[test]
    fn static_single_file() {
        let test_server = TestServer::new(build_simple_router(|route| {
            route
                .get("/")
                .to_file("resources/test/static_files/doc.html")
        })).unwrap();

        let response = test_server
            .client()
            .get("http://localhost/")
            .perform()
            .unwrap();

        assert_eq!(response.status(), StatusCode::Ok);
        assert_eq!(
            response.headers().get::<ContentType>().unwrap(),
            &ContentType::html()
        );
        assert!(response.headers().get::<LastModified>().is_some());
        assert!(response.headers().get::<ETag>().is_some());

        let body = response.read_body().unwrap();
        assert_eq!(&body[..], b"<html>I am a doc.</html>");
    }

    #[test]
    fn static_if_modified_since() {
        let response = test_server()
            .client()
            .get("http://localhost/doc.html")
            .with_header(IfModifiedSince(SystemTime::now().into()))
            .perform()
            .unwrap();

        assert_eq!(response.status(), StatusCode::NotModified);
    }

    #[test]
    fn static_if_none_match() {
        let response = test_server()
            .client()
            .get("http://localhost/doc.html")
            .perform()
            .unwrap();
        let ETag(etag) = response.headers().get::<ETag>().unwrap();

        let next_response = test_server()
            .client()
            .get("http://localhost/doc.html")
            .with_header(IfNoneMatch::Items(vec![etag.clone()]))
            .perform()
            .unwrap();
        assert_eq!(next_response.status(), StatusCode::NotModified);
    }

    fn test_server() -> TestServer {
        TestServer::new(static_router("/*", "resources/test/static_files")).unwrap()
    }

    fn static_router(mount: &str, path: &str) -> Router {
        build_simple_router(|route| route.get(mount).to_filesystem(path))
    }
}
