//! In-memory data store.
//!
//! This replaces a database. It is an `Arc<RwLock<HashMap<Uuid, Task>>>`:
//!   - `HashMap<Uuid, Task>` is the table.
//!   - `RwLock` allows many concurrent readers OR one writer (a read/write
//!     mutex) — handy for a read-heavy API.
//!   - `Arc` (atomically reference-counted pointer) lets every request handler
//!     share the same store across threads/tasks. Cloning an `Arc` is cheap:
//!     it just bumps a counter, like sharing a reference in JavaScript.
//!
//! In Node you'd reach for a `Map` plus maybe a mutex library; here the type
//! system forces you to acknowledge shared mutable state explicitly.
//!
//! To swap in a real database (Postgres via `sqlx`), you would replace this
//! struct with a `sqlx::PgPool` and make these methods `async`. See
//! ../../17-database/00_sqlx-intro.md.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use uuid::Uuid;

use crate::models::Task;

/// The shared application state injected into every handler.
///
/// `Clone` is cheap here because the only field is an `Arc`; cloning shares
/// the underlying map rather than copying it.
#[derive(Clone, Default)]
pub struct TaskStore {
    inner: Arc<RwLock<HashMap<Uuid, Task>>>,
}

impl TaskStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return all tasks, sorted by creation time (newest last) so list output
    /// is deterministic. Takes a read lock.
    pub fn list(&self) -> Vec<Task> {
        let map = self.inner.read().expect("store lock poisoned");
        let mut tasks: Vec<Task> = map.values().cloned().collect();
        tasks.sort_by_key(|t| t.created_at);
        tasks
    }

    /// Fetch one task by id. Returns `None` if it does not exist.
    pub fn get(&self, id: Uuid) -> Option<Task> {
        let map = self.inner.read().expect("store lock poisoned");
        map.get(&id).cloned()
    }

    /// Insert a new task. Takes a write lock.
    pub fn insert(&self, task: Task) -> Task {
        let mut map = self.inner.write().expect("store lock poisoned");
        map.insert(task.id, task.clone());
        task
    }

    /// Replace an existing task identified by `id`. Returns the updated task,
    /// or `None` if the id was not present.
    pub fn update(&self, id: Uuid, task: Task) -> Option<Task> {
        let mut map = self.inner.write().expect("store lock poisoned");
        if let std::collections::hash_map::Entry::Occupied(mut entry) = map.entry(id) {
            entry.insert(task.clone());
            Some(task)
        } else {
            None
        }
    }

    /// Delete a task by id. Returns `true` if something was removed.
    pub fn delete(&self, id: Uuid) -> bool {
        let mut map = self.inner.write().expect("store lock poisoned");
        map.remove(&id).is_some()
    }
}
