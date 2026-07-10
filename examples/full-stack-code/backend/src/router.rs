//! Wiring: maps URL patterns to handlers and attaches middleware.
//! This is the `app.use(...)` / `app.get(...)` section of an Express app,
//! but assembled as a value you can return and test.

use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::handlers::{create_note, delete_note, list_notes};
use crate::state::AppState;

/// Build the full application router.
///
/// `static_dir` is served at `/` so the same process can hand out the
/// compiled WASM frontend AND the JSON API — no separate static server
/// needed in dev. In production you'd usually put a CDN in front.
pub fn build_router(state: AppState, static_dir: &str) -> Router {
    // The JSON API, nested under /api.
    let api = Router::new()
        .route("/notes", get(list_notes).post(create_note))
        .route("/notes/{id}", axum::routing::delete(delete_note))
        .with_state(state);

    Router::new()
        .nest("/api", api)
        // Serve index.html + the wasm bundle from the static directory.
        .fallback_service(ServeDir::new(static_dir))
        // Permissive CORS so you can also run the frontend from a
        // different dev port if you prefer. Tighten this in production.
        .layer(CorsLayer::permissive())
        // Structured request logging, like `morgan` in Express.
        .layer(TraceLayer::new_for_http())
}
