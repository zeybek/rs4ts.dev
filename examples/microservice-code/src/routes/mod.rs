//! Router assembly: wire handlers to paths and stack the middleware layers.

mod health;
mod links;

use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Build the complete application router from shared state.
///
/// The layer order matters: each `.layer(...)` call wraps everything added
/// before it, so the layer added **last** is the **outermost**. Here
/// `TraceLayer` (added last) sits outermost and sees every request, while
/// `TimeoutLayer` (added first) runs closer to the handler. This is the typed,
/// compile-checked version of `app.use(...)` middleware stacking in Express.
pub fn build_router(state: AppState) -> Router {
    let timeout = state.settings.request_timeout;

    Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready))
        .route("/shorten", post(links::shorten))
        // axum 0.8 path params use `{name}` syntax (not the old `:name`).
        .route("/{code}", get(links::redirect))
        // A request exceeding the budget gets `408 Request Timeout`.
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            timeout,
        ))
        // Emit a structured span + access log for every request/response.
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
