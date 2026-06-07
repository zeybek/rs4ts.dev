//! The `Task` domain model and its serialized shape.
//!
//! In a Node project these types would live in a `types.ts` and you'd trust
//! TypeScript at compile time but get plain JS objects at runtime. Here the
//! `#[derive(Serialize, Deserialize)]` attributes generate real (de)serializing
//! code, so the JSON on disk is validated into a `Task` struct every time we
//! load it.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// A single task in the to-do list.
///
/// `serde` turns this into / from JSON. Field names are used verbatim as JSON
/// keys, so the on-disk format is `{"id":1,"title":"...","done":false,...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Stable, 1-based identifier shown to the user and used by `done`/`remove`.
    pub id: u32,
    /// What the task is about.
    pub title: String,
    /// Whether the task has been completed.
    pub done: bool,
    /// When the task was created (RFC 3339, local timezone).
    pub created_at: DateTime<Local>,
    /// When the task was marked done, if ever. `Option` maps to JSON `null`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub completed_at: Option<DateTime<Local>>,
}

impl Task {
    /// Create a fresh, not-yet-done task stamped with the current local time.
    pub fn new(id: u32, title: String) -> Self {
        Self {
            id,
            title,
            done: false,
            created_at: Local::now(),
            completed_at: None,
        }
    }

    /// Mark this task done, recording the completion time. Returns `false` if it
    /// was already done so the caller can report a no-op.
    pub fn mark_done(&mut self) -> bool {
        if self.done {
            return false;
        }
        self.done = true;
        self.completed_at = Some(Local::now());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_task_is_not_done() {
        let task = Task::new(1, "write tests".to_string());
        assert_eq!(task.id, 1);
        assert!(!task.done);
        assert!(task.completed_at.is_none());
    }

    #[test]
    fn mark_done_is_idempotent() {
        let mut task = Task::new(1, "write tests".to_string());
        assert!(task.mark_done()); // first call changes state
        assert!(task.done);
        assert!(task.completed_at.is_some());
        assert!(!task.mark_done()); // second call is a no-op
    }

    #[test]
    fn round_trips_through_json() {
        let task = Task::new(7, "round trip".to_string());
        let json = serde_json::to_string(&task).unwrap();
        let back: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, task.id);
        assert_eq!(back.title, task.title);
        assert_eq!(back.done, task.done);
    }
}
