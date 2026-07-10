//! Core URL-shortener handlers: create a short link, and redirect by code.

use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use url::Url;

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
/// Security note: any public shortener is an **open redirector** by design —
/// it will happily redirect to whatever URL was submitted, which phishers love.
/// A production deployment should mitigate: require auth on `POST /shorten`,
/// keep a domain allow/deny list, and/or show an interstitial page instead of
/// a silent redirect for untrusted targets.
fn validate_url(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidUrl("url must not be empty".into()));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::InvalidUrl(
            "url must not contain control characters".into(),
        ));
    }

    let parsed = Url::parse(trimmed)
        .map_err(|_| AppError::InvalidUrl("url must be a valid absolute URL".into()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::InvalidUrl(
            "url must start with http:// or https://".into(),
        ));
    }
    if parsed.host().is_none() {
        return Err(AppError::InvalidUrl("url must include a host".into()));
    }

    Ok(parsed.to_string())
}
