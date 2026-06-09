//! Production-ready URL-shortener microservice (binary entry point).
//!
//! This is a thin wrapper: all logic lives in the library crate (`lib.rs`).
//! `main` wires the pieces together and demonstrates the Section 28 production
//! patterns end to end:
//! - layered configuration (`config`)
//! - structured JSON logging (`telemetry`)
//! - a single typed error (`error`)
//! - `/health` + `/ready` probes (`routes`)
//! - graceful shutdown on SIGINT/SIGTERM (`shutdown`)
//! - an in-memory store behind a trait (`store`) so Redis can drop in later.

use tokio::net::TcpListener;

use url_shortener::config::Settings;
use url_shortener::state::AppState;
use url_shortener::{routes, shutdown, telemetry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load configuration from the environment. Absent variables fall back
    //    to defaults; malformed ones abort startup (fail fast, Section 28).
    let settings = Settings::from_env()?;

    // 2. Bring up structured logging before anything else can emit events.
    telemetry::init(&settings);

    tracing::info!(
        bind = %settings.bind_addr,
        base_url = %settings.base_url,
        code_length = settings.code_length,
        timeout_secs = settings.request_timeout.as_secs(),
        "starting url-shortener"
    );

    // 3. Build shared state and the router.
    let bind_addr = settings.bind_addr;
    let state = AppState::new(settings);
    let app = routes::build_router(state);

    // 4. Bind the TCP listener and serve with graceful shutdown.
    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!(addr = %listener.local_addr()?, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown::signal())
        .await?;

    tracing::info!("shutdown complete");
    Ok(())
}
