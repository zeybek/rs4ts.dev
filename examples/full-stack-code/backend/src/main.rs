//! Backend entry point. Boots Tokio, builds the router, and serves.
//!
//! Node analogue:
//! ```js
//! const app = express();
//! app.listen(3000, () => console.log("listening"));
//! ```

mod handlers;
mod models;
mod router;
mod state;

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::router::build_router;
use crate::state::AppState;

/// `#[tokio::main]` turns this async `main` into a sync one that starts
/// the runtime first — Rust has no built-in event loop, you opt into one.
#[tokio::main]
async fn main() {
    // Logging. `RUST_LOG=info` (the default below) controls verbosity.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .init();

    // Where the compiled frontend lives. Override with STATIC_DIR.
    // The default is resolved at build time from this crate's directory
    // (`CARGO_MANIFEST_DIR` = the `backend/` dir), so it is correct no
    // matter the current working directory `cargo run` was invoked from.
    let static_dir = std::env::var("STATIC_DIR")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../static").to_string());

    let state = AppState::new();
    let app = build_router(state, &static_dir);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(addr)
        .await
        .expect("failed to bind to port 3000");

    tracing::info!("backend listening on http://{addr}");
    tracing::info!("serving static files from {static_dir}");

    // axum 0.8: serve a listener + a router. No `app.listen` magic.
    axum::serve(listener, app)
        .await
        .expect("server error");
}
