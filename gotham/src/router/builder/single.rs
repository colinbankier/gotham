use std::panic::RefUnwindSafe;

use extractor::{PathExtractor, QueryStringExtractor};
use pipeline::chain::PipelineHandleChain;
use router::builder::SingleRouteBuilder;
use router::builder::replace::{ReplacePathExtractor, ReplaceQueryStringExtractor};
use router::route::{Delegation, Extractors, RouteImpl};
use router::route::matcher::RouteMatcher;
use router::route::dispatch::DispatcherImpl;
use handler::{Handler, NewHandler};

// Temporary
use state::State;
use hyper::{Response, StatusCode};
use http::response::create_response;
use mime;
use handler::static_file::StaticFileHandler;

/// Describes the API for defining a single route, after determining which request paths will be
/// dispatched here. The API here uses chained function calls to build and add the route into the
/// `RouterBuilder` which created it.
///
/// # Examples
///
/// ```rust
/// # extern crate gotham;
/// # extern crate hyper;
/// #
/// # use hyper::{Response, StatusCode};
/// # use gotham::state::State;
/// # use gotham::router::Router;
/// # use gotham::router::builder::*;
/// # use gotham::pipeline::new_pipeline;
/// # use gotham::pipeline::single::*;
/// # use gotham::middleware::session::NewSessionMiddleware;
/// # use gotham::test::TestServer;
/// #
/// fn my_handler(state: State) -> (State, Response) {
///     // Handler implementation elided.
/// #   (state, Response::new().with_status(StatusCode::Accepted))
/// }
/// #
/// # fn router() -> Router {
/// #   let (chain, pipelines) = single_pipeline(
/// #       new_pipeline().add(NewSessionMiddleware::default()).build()
/// #   );
/// #
/// build_router(chain, pipelines, |route| {
///     route.get("/request/path") // <- This value implements `DefineSingleRoute`
///          .to(my_handler);
/// })
/// # }
/// #
/// # fn main() {
/// #   let test_server = TestServer::new(router()).unwrap();
/// #   let response = test_server.client()
/// #       .get("https://example.com/request/path")
/// #       .perform()
/// #       .unwrap();
/// #   assert_eq!(response.status(), StatusCode::Accepted);
/// # }
/// ```
pub trait DefineSingleRoute {
    /// Directs the route to the given `Handler`, automatically creating a `NewHandler` which
    /// copies the `Handler`. This is the easiest option for code which is using bare functions as
    /// `Handler` functions.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate gotham;
    /// # extern crate hyper;
    /// #
    /// # use hyper::{Response, StatusCode};
    /// # use gotham::state::State;
    /// # use gotham::router::Router;
    /// # use gotham::router::builder::*;
    /// # use gotham::pipeline::new_pipeline;
    /// # use gotham::pipeline::single::*;
    /// # use gotham::middleware::session::NewSessionMiddleware;
    /// # use gotham::test::TestServer;
    /// #
    /// fn my_handler(state: State) -> (State, Response) {
    ///     // Handler implementation elided.
    /// #   (state, Response::new().with_status(StatusCode::Accepted))
    /// }
    /// #
    /// # fn router() -> Router {
    /// #   let (chain, pipelines) = single_pipeline(
    /// #       new_pipeline().add(NewSessionMiddleware::default()).build()
    /// #   );
    ///
    /// build_router(chain, pipelines, |route| {
    ///     route.get("/request/path").to(my_handler);
    /// })
    /// #
    /// # }
    /// #
    /// # fn main() {
    /// #   let test_server = TestServer::new(router()).unwrap();
    /// #   let response = test_server.client()
    /// #       .get("https://example.com/request/path")
    /// #       .perform()
    /// #       .unwrap();
    /// #   assert_eq!(response.status(), StatusCode::Accepted);
    /// # }
    /// ```
    fn to<H>(self, handler: H)
    where
        H: Handler + RefUnwindSafe + Copy + Send + Sync + 'static;

    fn to_filesystem(self, path: &'static str);

    /// Directs the route to the given `NewHandler`. This gives more control over how `Handler`
    /// values are constructed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate gotham;
    /// # extern crate hyper;
    /// # extern crate futures;
    /// #
    /// # use std::io;
    /// # use hyper::{Response, StatusCode};
    /// # use futures::future;
    /// # use gotham::handler::{Handler, HandlerFuture, NewHandler};
    /// # use gotham::state::State;
    /// # use gotham::router::Router;
    /// # use gotham::router::builder::*;
    /// # use gotham::pipeline::new_pipeline;
    /// # use gotham::pipeline::single::*;
    /// # use gotham::middleware::session::NewSessionMiddleware;
    /// # use gotham::test::TestServer;
    /// #
    /// struct MyNewHandler;
    /// struct MyHandler;
    ///
    /// impl NewHandler for MyNewHandler {
    ///     type Instance = MyHandler;
    ///
    ///     fn new_handler(&self) -> io::Result<Self::Instance> {
    ///         Ok(MyHandler)
    ///     }
    /// }
    ///
    /// impl Handler for MyHandler {
    ///     fn handle(self, state: State) -> Box<HandlerFuture> {
    ///         // Handler implementation elided.
    /// #       let response = Response::new().with_status(StatusCode::Accepted);
    /// #       Box::new(future::ok((state, response)))
    ///     }
    /// }
    /// #
    /// # fn router() -> Router {
    /// #   let (chain, pipelines) = single_pipeline(
    /// #       new_pipeline().add(NewSessionMiddleware::default()).build()
    /// #   );
    ///
    /// build_router(chain, pipelines, |route| {
    ///     route.get("/request/path").to_new_handler(MyNewHandler);
    /// })
    /// # }
    /// #
    /// # fn main() {
    /// #   let test_server = TestServer::new(router()).unwrap();
    /// #   let response = test_server.client()
    /// #       .get("https://example.com/request/path")
    /// #       .perform()
    /// #       .unwrap();
    /// #   assert_eq!(response.status(), StatusCode::Accepted);
    /// # }
    /// ```
    fn to_new_handler<NH>(self, new_handler: NH)
    where
        NH: NewHandler + 'static;

    /// Applies a `PathExtractor` type to the current route, to extract path parameters into
    /// `State` with the given type.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate gotham;
    /// # #[macro_use]
    /// # extern crate gotham_derive;
    /// # #[macro_use]
    /// # extern crate serde_derive;
    /// # extern crate hyper;
    /// #
    /// # use hyper::{Response, StatusCode};
    /// # use gotham::state::{State, FromState};
    /// # use gotham::router::Router;
    /// # use gotham::router::builder::*;
    /// # use gotham::pipeline::new_pipeline;
    /// # use gotham::pipeline::set::*;
    /// # use gotham::middleware::session::NewSessionMiddleware;
    /// # use gotham::test::TestServer;
    /// #
    /// #[derive(Deserialize, StateData, StaticResponseExtender)]
    /// struct MyPathParams {
    /// #   #[allow(dead_code)]
    ///     name: String,
    /// }
    ///
    /// fn my_handler(state: State) -> (State, Response) {
    /// #   {
    ///     let params = MyPathParams::borrow_from(&state);
    ///
    ///     // Handler implementation elided.
    /// #   assert_eq!(params.name, "world");
    /// #   }
    /// #   (state, Response::new().with_status(StatusCode::Accepted))
    /// }
    /// #
    /// # fn router() -> Router {
    /// #   let pipelines = new_pipeline_set();
    /// #   let (pipelines, default) =
    /// #       pipelines.add(new_pipeline().add(NewSessionMiddleware::default()).build());
    /// #
    /// #   let pipelines = finalize_pipeline_set(pipelines);
    /// #
    /// #   let default_pipeline_chain = (default, ());
    ///
    /// build_router(default_pipeline_chain, pipelines, |route| {
    ///     route.get("/hello/:name")
    ///          .with_path_extractor::<MyPathParams>()
    ///          .to(my_handler);
    /// })
    /// # }
    /// #
    /// # fn main() {
    /// #   let test_server = TestServer::new(router()).unwrap();
    /// #   let response = test_server.client()
    /// #       .get("https://example.com/hello/world")
    /// #       .perform()
    /// #       .unwrap();
    /// #   assert_eq!(response.status(), StatusCode::Accepted);
    /// # }
    /// ```
    fn with_path_extractor<NPE>(self) -> <Self as ReplacePathExtractor<NPE>>::Output
    where
        NPE: PathExtractor + Send + Sync + 'static,
        Self: ReplacePathExtractor<NPE>,
        Self::Output: DefineSingleRoute;

    /// Applies a `QueryStringExtractor` type to the current route, to extract query parameters into
    /// `State` with the given type.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate gotham;
    /// # #[macro_use]
    /// # extern crate gotham_derive;
    /// # extern crate hyper;
    /// # extern crate serde;
    /// # #[macro_use]
    /// # extern crate serde_derive;
    /// #
    /// # use hyper::{Response, StatusCode};
    /// # use gotham::state::{State, FromState};
    /// # use gotham::router::Router;
    /// # use gotham::router::builder::*;
    /// # use gotham::pipeline::new_pipeline;
    /// # use gotham::pipeline::set::*;
    /// # use gotham::middleware::session::NewSessionMiddleware;
    /// # use gotham::test::TestServer;
    /// #
    /// #[derive(StateData, Deserialize, StaticResponseExtender)]
    /// struct MyQueryParams {
    /// #   #[allow(dead_code)]
    ///     id: u64,
    /// }
    ///
    /// fn my_handler(state: State) -> (State, Response) {
    ///     let id = MyQueryParams::borrow_from(&state).id;
    ///
    ///     // Handler implementation elided.
    /// #   assert_eq!(id, 42);
    /// #   (state, Response::new().with_status(StatusCode::Accepted))
    /// }
    /// #
    /// # fn router() -> Router {
    /// #   let pipelines = new_pipeline_set();
    /// #   let (pipelines, default) =
    /// #       pipelines.add(new_pipeline().add(NewSessionMiddleware::default()).build());
    /// #
    /// #   let pipelines = finalize_pipeline_set(pipelines);
    /// #
    /// #   let default_pipeline_chain = (default, ());
    ///
    /// build_router(default_pipeline_chain, pipelines, |route| {
    ///     route.get("/request/path")
    ///          .with_query_string_extractor::<MyQueryParams>()
    ///          .to(my_handler);
    /// })
    /// # }
    /// #
    /// # fn main() {
    /// #   let test_server = TestServer::new(router()).unwrap();
    /// #   let response = test_server.client()
    /// #       .get("https://example.com/request/path?id=42")
    /// #       .perform()
    /// #       .unwrap();
    /// #   assert_eq!(response.status(), StatusCode::Accepted);
    /// # }
    /// ```
    fn with_query_string_extractor<NQSE>(
        self,
    ) -> <Self as ReplaceQueryStringExtractor<NQSE>>::Output
    where
        NQSE: QueryStringExtractor + Send + Sync + 'static,
        Self: ReplaceQueryStringExtractor<NQSE>,
        Self::Output: DefineSingleRoute;
}

impl<'a, M, C, P, PE, QSE> DefineSingleRoute for SingleRouteBuilder<'a, M, C, P, PE, QSE>
where
    M: RouteMatcher + Send + Sync + 'static,
    C: PipelineHandleChain<P> + Send + Sync + 'static,
    P: RefUnwindSafe + Send + Sync + 'static,
    PE: PathExtractor + Send + Sync + 'static,
    QSE: QueryStringExtractor + Send + Sync + 'static,
{
    fn to<H>(self, handler: H)
    where
        H: Handler + RefUnwindSafe + Copy + Send + Sync + 'static,
    {
        self.to_new_handler(move || Ok(handler))
    }

    fn to_filesystem(self, path: &'static str) {
        self.to_new_handler(move || Ok(StaticFileHandler::new(path)))
    }

    fn to_new_handler<NH>(self, new_handler: NH)
    where
        NH: NewHandler + 'static,
    {
        let dispatcher = DispatcherImpl::new(new_handler, self.pipeline_chain, self.pipelines);
        let route: RouteImpl<M, PE, QSE> = RouteImpl::new(
            self.matcher,
            Box::new(dispatcher),
            Extractors::new(),
            Delegation::Internal,
        );
        self.node_builder.add_route(Box::new(route));
    }

    fn with_path_extractor<NPE>(self) -> <Self as ReplacePathExtractor<NPE>>::Output
    where
        NPE: PathExtractor + Send + Sync + 'static,
    {
        self.replace_path_extractor()
    }

    fn with_query_string_extractor<NQSE>(
        self,
    ) -> <Self as ReplaceQueryStringExtractor<NQSE>>::Output
    where
        NQSE: QueryStringExtractor + Send + Sync + 'static,
    {
        self.replace_query_string_extractor()
    }
}
