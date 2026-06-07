//! Shared application state passed to every handler.
//!
//! axum clones the state per request, so it must be cheap to clone. We wrap the
//! settings in `Arc` and rely on `InMemoryStore` already being `Arc`-backed.
//! This is the typed equivalent of stashing things on `app.locals` in Express.

use std::sync::Arc;
use std::time::Instant;

use crate::config::Settings;
use crate::store::InMemoryStore;

/// Everything a handler might need: configuration, the store, and start time.
#[derive(Clone)]
pub struct AppState {
    /// Immutable, shared configuration.
    pub settings: Arc<Settings>,
    /// The link store (swap this type to change backends).
    pub store: InMemoryStore,
    /// When the process started, used to report uptime on `/health`.
    pub started_at: Instant,
}

impl AppState {
    /// Assemble state from already-loaded settings.
    pub fn new(settings: Settings) -> Self {
        AppState {
            settings: Arc::new(settings),
            store: InMemoryStore::new(),
            started_at: Instant::now(),
        }
    }

    /// Seconds elapsed since the process started.
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}
