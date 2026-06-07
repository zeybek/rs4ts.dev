//! Library crate for the URL-shortener microservice.
//!
//! The binary (`main.rs`) is a thin wrapper around this library: it loads
//! [`config::Settings`], initialises [`telemetry`], builds the router with
//! [`routes::build_router`], and serves it. Exposing the internals as a library
//! is what lets the integration tests in `tests/` boot the real app in-process.

pub mod config;
pub mod error;
pub mod models;
pub mod routes;
pub mod shutdown;
pub mod state;
pub mod store;
pub mod telemetry;
