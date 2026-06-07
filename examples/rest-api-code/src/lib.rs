//! `rest-api` — a small but production-flavored JSON REST API for a `tasks`
//! resource, built with axum 0.8.
//!
//! The crate is organized as a library (this file) plus a thin binary
//! (`main.rs`). Keeping the app in a library lets integration tests in
//! `tests/` build the exact same `Router` and exercise it in-process without
//! binding a real TCP port.

pub mod error;
pub mod handlers;
pub mod models;
pub mod routes;
pub mod store;

pub use routes::app;
pub use store::TaskStore;
