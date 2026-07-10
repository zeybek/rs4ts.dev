//! Request handlers — the equivalent of Express route callbacks
//! `(req, res) => { ... }`, but each is a typed async function whose
//! return type *is* the response.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::models::{ApiError, CreateNote, Note};
use crate::state::AppState;

/// `GET /api/notes` -> `200` with a JSON array of notes.
pub async fn list_notes(State(state): State<AppState>) -> Json<Vec<Note>> {
    Json(state.list())
}

/// `POST /api/notes` -> `201` with the created note.
///
/// `Json(payload)` is an extractor: axum parses and validates the body
/// against `CreateNote` before this function runs. A malformed body
/// never reaches us — axum returns `422` automatically.
pub async fn create_note(
    State(state): State<AppState>,
    Json(payload): Json<CreateNote>,
) -> Response {
    // A little hand-rolled validation to show returning a typed error.
    if payload.title.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "title must not be empty".to_string(),
            }),
        )
            .into_response();
    }
    let note = state.create(payload);
    (StatusCode::CREATED, Json(note)).into_response()
}

/// `DELETE /api/notes/{id}` -> `204` if deleted, `404` otherwise.
///
/// `Path(id)` pulls the `{id}` segment from the URL and parses it into a
/// `u64`. axum 0.8 uses `{id}` syntax (not the old `:id`).
pub async fn delete_note(State(state): State<AppState>, Path(id): Path<u64>) -> Response {
    if state.delete(id) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("no note with id {id}"),
            }),
        )
            .into_response()
    }
}
