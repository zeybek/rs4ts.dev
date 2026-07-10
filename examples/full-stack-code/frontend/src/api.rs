//! The API client layer. `gloo-net` wraps the browser `fetch` API in a
//! Rusty, `async`/`Result` interface — the analogue of a small
//! `apiClient.ts` built on `fetch`.

use gloo_net::http::Request;

use crate::models::{CreateNote, Note};

/// Base URL of the JSON API. Relative, so it works whether the page is
/// served by the Rust backend itself or by any static host that proxies
/// `/api` to it.
const API_BASE: &str = "/api";

/// `GET /api/notes` -> deserialize into `Vec<Note>`.
///
/// Every step that can fail returns a `Result`, and `?` short-circuits
/// on the first error — no `try/catch`, the type system forces you to
/// acknowledge failure. `gloo_net::Error` is the unified error type.
pub async fn fetch_notes() -> Result<Vec<Note>, gloo_net::Error> {
    let resp = Request::get(&format!("{API_BASE}/notes")).send().await?;
    // gloo-net does NOT treat a non-2xx status as an error, so check it
    // ourselves — otherwise a 4xx/5xx would either fail to deserialize or be
    // mistaken for success.
    if !resp.ok() {
        return Err(gloo_net::Error::GlooError(format!(
            "GET /notes failed: HTTP {}",
            resp.status()
        )));
    }
    let notes = resp.json::<Vec<Note>>().await?;
    Ok(notes)
}

/// `POST /api/notes` with a JSON body -> the created `Note`.
pub async fn create_note(input: &CreateNote) -> Result<Note, gloo_net::Error> {
    let resp = Request::post(&format!("{API_BASE}/notes"))
        .json(input)? // serialize body + set Content-Type
        .send()
        .await?;
    if !resp.ok() {
        return Err(gloo_net::Error::GlooError(format!(
            "POST /notes failed: HTTP {}",
            resp.status()
        )));
    }
    let note = resp.json::<Note>().await?;
    Ok(note)
}

/// `DELETE /api/notes/{id}` -> `Ok(())` on success.
pub async fn delete_note(id: u64) -> Result<(), gloo_net::Error> {
    let resp = Request::delete(&format!("{API_BASE}/notes/{id}"))
        .send()
        .await?;
    if !resp.ok() {
        return Err(gloo_net::Error::GlooError(format!(
            "DELETE /notes/{id} failed: HTTP {}",
            resp.status()
        )));
    }
    Ok(())
}
