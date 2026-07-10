//! Domain types and request/response DTOs for the `tasks` resource.
//!
//! Think of this as your TypeScript `interface Task { ... }` plus the
//! `class CreateTaskDto { ... }` you'd annotate with `class-validator`
//! decorators in NestJS — except the validation lives in plain methods.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::AppError;

/// A task as it is stored and returned to clients.
///
/// `#[serde(...)]` controls the JSON shape. `OffsetDateTime` is serialized as
/// an RFC 3339 string (e.g. `2026-06-02T10:00:00Z`) via the `time` crate's
/// `serde::rfc3339` module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub completed: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// Body for `POST /tasks`. Mirrors a NestJS `CreateTaskDto`.
///
/// `description` is optional with a default of empty string, and `completed`
/// defaults to `false` — so clients can omit them.
#[derive(Debug, Deserialize)]
pub struct CreateTask {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub completed: bool,
}

/// Body for `PUT /tasks/{id}`. Every field is optional so this doubles as a
/// partial update (PATCH-like semantics). `Option<T>` is exactly TypeScript's
/// `field?: T` — present or absent.
#[derive(Debug, Deserialize)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub completed: Option<bool>,
}

impl CreateTask {
    /// Validate the incoming payload. Returns `AppError::Validation` (HTTP 422)
    /// on the first problem, like a guard clause at the top of a handler.
    pub fn validate(&self) -> Result<(), AppError> {
        validate_title(&self.title)?;
        validate_description(&self.description)?;
        Ok(())
    }

    /// Build a fresh `Task` from a validated create payload.
    pub fn into_task(self) -> Task {
        let now = OffsetDateTime::now_utc();
        Task {
            id: Uuid::new_v4(),
            title: self.title,
            description: self.description,
            completed: self.completed,
            created_at: now,
            updated_at: now,
        }
    }
}

impl UpdateTask {
    /// Validate only the fields that were actually provided.
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(title) = &self.title {
            validate_title(title)?;
        }
        if let Some(description) = &self.description {
            validate_description(description)?;
        }
        Ok(())
    }

    /// Apply the provided fields onto an existing task in place and bump
    /// `updated_at`. Fields left as `None` are untouched.
    pub fn apply_to(self, task: &mut Task) {
        if let Some(title) = self.title {
            task.title = title;
        }
        if let Some(description) = self.description {
            task.description = description;
        }
        if let Some(completed) = self.completed {
            task.completed = completed;
        }
        task.updated_at = OffsetDateTime::now_utc();
    }
}

/// Title must be non-empty (after trimming) and at most 200 characters.
fn validate_title(title: &str) -> Result<(), AppError> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("title must not be empty".into()));
    }
    if trimmed.chars().count() > 200 {
        return Err(AppError::Validation(
            "title must be at most 200 characters".into(),
        ));
    }
    Ok(())
}

/// Description is optional but capped at 2000 characters.
fn validate_description(description: &str) -> Result<(), AppError> {
    if description.chars().count() > 2000 {
        return Err(AppError::Validation(
            "description must be at most 2000 characters".into(),
        ));
    }
    Ok(())
}
