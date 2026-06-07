//! Liveness and readiness probes.
//!
//! Kubernetes (and most orchestrators) distinguish two checks:
//! - **liveness** (`/health`): is the process up? Restart it if not.
//! - **readiness** (`/ready`): can it serve traffic *right now*? Pull it from
//!   the load balancer if not.
//!
//! See `../28-production/03_health-checks.md` for the production rationale.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::models::HealthResponse;
use crate::state::AppState;
use crate::store::Store;

/// `GET /health` — liveness. Always returns `200 OK` if the process can route.
///
/// Liveness must not depend on store state (that is readiness' job), so the
/// link count is best-effort: a store hiccup still yields a healthy `200`
/// rather than triggering a needless restart.
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        uptime_secs: state.uptime_secs(),
        links: state.store.len().unwrap_or(0),
        redirects: state.store.redirect_count(),
    })
}

/// `GET /ready` — readiness. Probes the store; returns `503` if it is broken.
///
/// For the in-memory store this only fails if the lock is poisoned, but the
/// same shape covers a real backend: ping Redis / run `SELECT 1` here and map a
/// failure to `503 Service Unavailable` so the orchestrator stops routing to
/// this instance until it recovers.
pub async fn ready(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<serde_json::Value>)> {
    // `is_empty` exercises the same connectivity path as a real backend ping.
    match state.store.is_empty().and_then(|_| state.store.len()) {
        Ok(links) => Ok(Json(HealthResponse {
            status: "ready",
            uptime_secs: state.uptime_secs(),
            links,
            redirects: state.store.redirect_count(),
        })),
        Err(_) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "unavailable",
                "error": "store_unreachable"
            })),
        )),
    }
}
