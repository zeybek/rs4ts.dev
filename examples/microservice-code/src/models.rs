//! Request and response data-transfer objects.
//!
//! These `serde`-derived structs are this service's contract — the Rust
//! analogue of the `interface`s / `zod` schemas you would define for an
//! Express handler's body and JSON response.

use serde::{Deserialize, Serialize};

/// Body of `POST /shorten`.
#[derive(Debug, Deserialize)]
pub struct ShortenRequest {
    /// The long URL to shorten.
    pub url: String,
}

/// Successful response from `POST /shorten`.
#[derive(Debug, Serialize)]
pub struct ShortenResponse {
    /// The generated short code (e.g. `aZ3kP9q`).
    pub code: String,
    /// The full short link, ready to share.
    pub short_url: String,
    /// Echo of the original URL.
    pub target: String,
}

/// Response body for `GET /health` and `GET /ready`.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// `ok` when the service is healthy.
    pub status: &'static str,
    /// Process uptime in seconds.
    pub uptime_secs: u64,
    /// Number of links currently stored.
    pub links: usize,
    /// Total redirects served since startup.
    pub redirects: u64,
}
