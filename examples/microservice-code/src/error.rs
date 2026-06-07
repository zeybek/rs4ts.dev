//! A single typed error for the whole service.
//!
//! Every fallible handler returns `Result<T, AppError>`. Because `AppError`
//! implements [`IntoResponse`], axum converts a returned error into a proper
//! HTTP response automatically — there is no equivalent of Express's
//! `next(err)` plumbing or a global error-handling middleware to register.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// All the ways a request can fail in this service.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The submitted URL was empty or not a valid `http(s)` URL.
    #[error("invalid url: {0}")]
    InvalidUrl(String),

    /// No short code matched the requested path.
    #[error("short code not found: {0}")]
    NotFound(String),

    /// The store could not satisfy the request (e.g. lock poisoned).
    #[error("internal store error")]
    Store,
}

impl AppError {
    /// Map each variant to its HTTP status code.
    fn status(&self) -> StatusCode {
        match self {
            AppError::InvalidUrl(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Store => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// A short, stable machine-readable error code for clients.
    fn code(&self) -> &'static str {
        match self {
            AppError::InvalidUrl(_) => "invalid_url",
            AppError::NotFound(_) => "not_found",
            AppError::Store => "internal_error",
        }
    }
}

/// The JSON body returned for any error.
#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Server-side faults are logged at error level; client mistakes at debug.
        if status.is_server_error() {
            tracing::error!(error = %self, code = self.code(), "request failed");
        } else {
            tracing::debug!(error = %self, code = self.code(), "request rejected");
        }

        let body = ErrorBody {
            error: self.code(),
            message: self.to_string(),
        };
        (status, Json(body)).into_response()
    }
}
