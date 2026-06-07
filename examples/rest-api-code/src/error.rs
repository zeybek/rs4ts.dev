//! Typed application errors.
//!
//! This is the Rust equivalent of a centralized Express error-handling
//! middleware (`app.use((err, req, res, next) => ...)`) combined with a
//! NestJS `HttpException`. Each variant maps to one HTTP status code, and
//! the `IntoResponse` impl turns it into a JSON body — so handlers can just
//! `return Err(AppError::NotFound)` and the framework does the rest.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Every way a request can fail in this API.
///
/// `thiserror` derives the `std::error::Error` impl and the `Display`
/// messages from the `#[error("...")]` attributes — no boilerplate.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested task id does not exist.
    #[error("task not found")]
    NotFound,

    /// The request body failed validation. Carries a human-readable reason.
    #[error("validation failed: {0}")]
    Validation(String),

    /// The JSON body was malformed or had the wrong shape. Axum's own
    /// `JsonRejection` is converted into this variant (see the `From` impl).
    #[error("invalid request body: {0}")]
    BadRequest(String),
}

impl AppError {
    /// The HTTP status code this error maps to.
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
        }
    }
}

/// This is what lets a handler return `Result<_, AppError>`: axum calls
/// `into_response()` on the `Err` value to build the actual HTTP response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        // A consistent error envelope, like you'd standardize in an Express
        // error handler: { "error": { "code": 404, "message": "..." } }.
        let body = Json(json!({
            "error": {
                "code": status.as_u16(),
                "message": self.to_string(),
            }
        }));
        (status, body).into_response()
    }
}

/// Convert axum's built-in JSON extractor rejection into our error type.
///
/// With this in place, `Json<CreateTask>` failures (bad syntax, missing
/// `Content-Type`, wrong field types) become a clean `400` JSON response
/// instead of axum's default plain-text error.
impl From<axum::extract::rejection::JsonRejection> for AppError {
    fn from(rejection: axum::extract::rejection::JsonRejection) -> Self {
        AppError::BadRequest(rejection.body_text())
    }
}
