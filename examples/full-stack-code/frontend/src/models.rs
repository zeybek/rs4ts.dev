//! Client-side mirror of the backend's JSON shapes.
//!
//! In a Node monorepo you'd share one `types.ts` between client and
//! server. Here the two crates compile for different targets (native vs
//! wasm), so we keep a small mirrored copy. The field names must match
//! the backend's `serde` output exactly — that's the contract.

use serde::{Deserialize, Serialize};

/// A note as returned by `GET /api/notes`.
#[derive(Debug, Clone, Deserialize)]
pub struct Note {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub created_at: u128,
}

/// The body we send to `POST /api/notes`.
#[derive(Debug, Serialize)]
pub struct CreateNote {
    pub title: String,
    pub body: String,
}
