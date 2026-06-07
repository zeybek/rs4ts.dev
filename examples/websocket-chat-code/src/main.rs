//! A real-time multi-room chat server over WebSockets.
//!
//! This is the Rust analogue of a small Socket.IO app in Node: clients connect
//! over a WebSocket, join named rooms, and every message is fanned out to
//! everyone else in the same room. State lives entirely in memory.
//!
//! Architecture:
//!   * [`state::AppState`] — the room registry (`Arc<Mutex<HashMap<..>>>`),
//!     one `tokio::sync::broadcast` channel per room.
//!   * [`ws`] — the WebSocket upgrade handler and per-connection duplex pump.
//!   * [`message`] — serde-tagged JSON message framing for both directions.
//!
//! Run it with `cargo run`, then open http://127.0.0.1:3000 in two browser
//! tabs and chat between them.

mod message;
mod state;
mod ws;

use std::net::SocketAddr;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

#[tokio::main]
async fn main() {
    // Structured logging. Override verbosity with e.g.
    // `RUST_LOG=websocket_chat_code=debug,tower_http=debug cargo run`.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("websocket_chat_code=info,tower_http=info")),
        )
        .init();

    let state = AppState::new();

    // Serve the browser client (static/index.html) at `/` and assets under it.
    let static_files = ServeDir::new("static").append_index_html_on_directories(true);

    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
        .route("/api/rooms", get(list_rooms))
        .fallback_service(static_files)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Bind address: override with `CHAT_ADDR=0.0.0.0:8080 cargo run`.
    let addr: SocketAddr = std::env::var("CHAT_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
        .parse()
        .expect("CHAT_ADDR must be a valid socket address, e.g. 127.0.0.1:3000");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    tracing::info!("chat server listening on http://{addr}");
    tracing::info!("open the URL in two browser tabs to try it out");

    // axum 0.8 serves via `axum::serve` over a plain tokio listener.
    axum::serve(listener, app).await.expect("server error");
}

/// `GET /api/rooms` — a tiny debug endpoint returning active rooms and how many
/// users each holds. Handy for `curl` while testing.
async fn list_rooms(State(state): State<AppState>) -> Json<serde_json::Value> {
    let rooms: Vec<serde_json::Value> = state
        .snapshot()
        .into_iter()
        .map(|(name, count)| serde_json::json!({ "room": name, "users": count }))
        .collect();
    Json(serde_json::json!({ "rooms": rooms }))
}
