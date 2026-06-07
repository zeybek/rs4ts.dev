//! Core URL-shortener handlers: create a short link, and redirect by code.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;

use crate::error::AppError;
use crate::models::{ShortenRequest, ShortenResponse};
use crate::state::AppState;
use crate::store::{InMemoryStore, Store};

/// `POST /shorten` — validate the URL, generate a code, store it, return the link.
///
/// `#[tracing::instrument]` creates a span for the whole handler. We `skip` the
/// state (it is large and not interesting) but record the submitted URL, so
/// every log line emitted inside this handler is automatically tagged with it —
/// far less boilerplate than threading a `requestId` through Express callbacks.
#[tracing::instrument(skip(state, payload), fields(url = %payload.url))]
pub async fn shorten(
    State(state): State<AppState>,
    Json(payload): Json<ShortenRequest>,
) -> Result<Json<ShortenResponse>, AppError> {
    let target = validate_url(&payload.url)?;

    // Generate a code and claim it. `insert` returns `false` on the (astronomically
    // unlikely) chance the random code is already taken, so we retry a few times
    // instead of silently clobbering an existing link.
    const MAX_ATTEMPTS: usize = 8;
    let mut code = None;
    for _ in 0..MAX_ATTEMPTS {
        let candidate = InMemoryStore::generate_code(state.settings.code_length);
        if state.store.insert(candidate.clone(), target.clone())? {
            code = Some(candidate);
            break;
        }
    }
    let code = code.ok_or(AppError::Store)?;

    let short_url = format!("{}/{}", state.settings.base_url, code);
    tracing::info!(%code, %target, "created short link");

    Ok(Json(ShortenResponse {
        code,
        short_url,
        target,
    }))
}

/// `GET /{code}` — look up the code and issue a `307` redirect to the target.
#[tracing::instrument(skip(state))]
pub async fn redirect(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Response, AppError> {
    match state.store.resolve(&code)? {
        Some(target) => {
            tracing::info!(%code, %target, "redirecting");
            Ok(Redirect::temporary(&target).into_response())
        }
        None => Err(AppError::NotFound(code)),
    }
}

/// Reject empty input and anything that is not an absolute `http`/`https` URL.
///
/// A real service would use the `url` crate for full RFC parsing; this keeps the
/// example dependency-light while still demonstrating typed validation errors.
fn validate_url(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidUrl("url must not be empty".into()));
    }
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .ok_or_else(|| AppError::InvalidUrl("url must start with http:// or https://".into()))?;
    // Require a non-empty host before any path/query/fragment, so inputs like
    // "http://" or "https://?x=1" are rejected rather than stored as dead links.
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        return Err(AppError::InvalidUrl("url must include a host".into()));
    }
    Ok(trimmed.to_string())
}
