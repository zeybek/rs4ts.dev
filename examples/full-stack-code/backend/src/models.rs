//! Data shapes shared across the API, with `serde` derives so they
//! serialize to/from JSON. The TypeScript analogue would be a set of
//! `interface`s plus hand-written (or `zod`-validated) parsing.

use serde::{Deserialize, Serialize};

/// A single note. `Serialize` turns it into JSON for responses;
/// `Clone` lets us hand copies out of the in-memory store.
#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub id: u64,
    pub title: String,
    pub body: String,
    /// Unix-millis timestamp; the frontend formats it for display.
    pub created_at: u128,
}

/// The request body for creating a note. Only `Deserialize` —
/// clients send these fields; the server assigns `id`/`created_at`.
#[derive(Debug, Deserialize)]
pub struct CreateNote {
    pub title: String,
    pub body: String,
}

/// A uniform error envelope so the frontend can always read `error`.
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}
