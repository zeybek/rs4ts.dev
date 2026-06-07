//! Binary entry point: initialize logging, build the app, bind a TCP
//! listener, and serve.
//!
//! In Node this is your `app.listen(3000, () => ...)`. In axum 0.8 you create
//! a `tokio::net::TcpListener` yourself and hand it to `axum::serve`, which
//! gives you full control over the socket.

use std::net::SocketAddr;

use rest_api::{TaskStore, app};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() {
    // Structured logging. `RUST_LOG=info cargo run` controls verbosity; we
    // default to `info` for our crate and tower-http if `RUST_LOG` is unset.
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("rest_api=info,tower_http=info")),
        )
        .init();

    // Create the shared in-memory store and build the router.
    let store = TaskStore::new();
    let app = app(store);

    // Bind to 127.0.0.1:3000.
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    tracing::info!("listening on http://{addr}");

    // Serve until the process is killed. `axum::serve` drives the accept loop.
    axum::serve(listener, app).await.expect("server error");
}
