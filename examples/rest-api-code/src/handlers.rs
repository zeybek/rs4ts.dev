//! HTTP handlers for the `tasks` resource — the controller layer.
//!
//! Each function is an axum handler. Compared to an Express route
//! `(req, res) => {...}`, axum hands you typed *extractors* as arguments
//! (state, path params, JSON body) and you return a typed value that
//! implements `IntoResponse`. There is no `res` object you can forget to
//! call — the return value *is* the response.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{CreateTask, Task, UpdateTask};
use crate::store::TaskStore;

/// `GET /tasks` — list every task.
///
/// `State(store)` pulls the shared `TaskStore` out of the router state.
/// Returning `Json<Vec<Task>>` sets `Content-Type: application/json` and a
/// `200 OK` automatically.
pub async fn list_tasks(State(store): State<TaskStore>) -> Json<Vec<Task>> {
    Json(store.list())
}

/// `GET /tasks/{id}` — fetch one task or 404.
///
/// `Path(id)` parses the `{id}` segment into a `Uuid`. If it is not a valid
/// UUID, axum rejects the request with a 400 before this code even runs.
pub async fn get_task(
    State(store): State<TaskStore>,
    Path(id): Path<Uuid>,
) -> Result<Json<Task>, AppError> {
    let task = store.get(id).ok_or(AppError::NotFound)?;
    Ok(Json(task))
}

/// `POST /tasks` — create a task.
///
/// The body is extracted as `Result<Json<CreateTask>, JsonRejection>` so we
/// can turn malformed JSON into our own typed error (HTTP 400) rather than
/// axum's default plain-text rejection. On success we validate, build the
/// `Task`, store it, and return `201 Created`.
pub async fn create_task(
    State(store): State<TaskStore>,
    payload: Result<Json<CreateTask>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    let Json(input) = payload?;
    input.validate()?;
    let task = store.insert(input.into_task());
    Ok((StatusCode::CREATED, Json(task)))
}

/// `PUT /tasks/{id}` — update an existing task (partial: omitted fields are
/// left unchanged). 404 if the id does not exist.
pub async fn update_task(
    State(store): State<TaskStore>,
    Path(id): Path<Uuid>,
    payload: Result<Json<UpdateTask>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Task>, AppError> {
    let Json(input) = payload?;
    input.validate()?;

    // Load, mutate, write back. With a real DB this would be a single
    // `UPDATE ... RETURNING *`.
    let mut task = store.get(id).ok_or(AppError::NotFound)?;
    input.apply_to(&mut task);
    let updated = store.update(id, task).ok_or(AppError::NotFound)?;
    Ok(Json(updated))
}

/// `DELETE /tasks/{id}` — remove a task. Returns `204 No Content` on success,
/// 404 if it was not there.
pub async fn delete_task(
    State(store): State<TaskStore>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    if store.delete(id) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

/// `GET /health` — a trivial liveness probe, like the one your container
/// orchestrator hits. Returns `{"status":"ok"}`.
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}
