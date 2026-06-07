//! Graceful shutdown: stop accepting new connections on a signal, then let
//! in-flight requests finish before the process exits.
//!
//! In Node you would listen for `process.on('SIGTERM', ...)` and call
//! `server.close()`. Here we return a future that resolves on the first signal;
//! `axum::serve(...).with_graceful_shutdown(future)` does the rest.
//! See `../28-production/02_graceful-shutdown.md`.

/// Resolve when the process receives `Ctrl-C` (SIGINT) or, on Unix, SIGTERM.
///
/// SIGTERM is what container orchestrators (Docker, Kubernetes) send when they
/// want a pod to stop, so handling it is what makes rolling deploys graceful.
pub async fn signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    // Whichever signal arrives first wins.
    tokio::select! {
        () = ctrl_c => tracing::info!("received SIGINT (Ctrl-C), shutting down"),
        () = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
