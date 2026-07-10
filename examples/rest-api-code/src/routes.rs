//! Router assembly and middleware — the equivalent of building your Express
//! `app`, mounting routers, and `app.use(...)`-ing middleware.

use axum::{Router, routing::get};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::store::TaskStore;

/// Build the application `Router`, wiring routes to handlers, attaching
/// middleware, and injecting the shared store as state.
///
/// This is split out from `main` so integration tests can build the same app
/// and drive it in-process (see `tests/api.rs`).
pub fn app(store: TaskStore) -> Router {
    // A permissive CORS policy, fine for a demo / public read API. Tighten
    // `allow_origin` for production. See ../../16-web-apis/11_cors.md.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(handlers::health))
        // Collection routes: GET list + POST create.
        .route(
            "/tasks",
            get(handlers::list_tasks).post(handlers::create_task),
        )
        // Item routes: GET one + PUT update + DELETE. Note axum 0.8 uses
        // `{id}` (not the old `:id`) for path parameters.
        .route(
            "/tasks/{id}",
            get(handlers::get_task)
                .put(handlers::update_task)
                .delete(handlers::delete_task),
        )
        // `TraceLayer` logs each request/response (method, path, status,
        // latency) via the `tracing` crate — like `morgan` in Express.
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        // Inject the store so every handler can pull it out with `State`.
        .with_state(store)
}
