//! The in-memory data store. No database required to compile or run.
//!
//! This is the Rust analogue of a module-level `const notes = new Map()`
//! in a Node app — except sharing it across concurrent requests is
//! explicit: `Arc` for shared ownership across tasks, `Mutex` for safe
//! mutation. See ../../../17-database/README.md for swapping in real SQL.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::{CreateNote, Note};

/// Shared application state. `axum` clones this (cheap — it's just `Arc`s)
/// for every request handler that asks for it via `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    /// id -> Note. `Arc<Mutex<..>>` = "shared, mutable, thread-safe".
    notes: Arc<Mutex<HashMap<u64, Note>>>,
    /// Monotonic id generator. Atomic so we never hand out a duplicate.
    next_id: Arc<AtomicU64>,
}

impl AppState {
    /// Build a store pre-seeded with one welcome note, so the UI has
    /// something to render on first load.
    pub fn new() -> Self {
        let state = Self {
            notes: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        };
        state.create(CreateNote {
            title: "Welcome".to_string(),
            body: "This note came from the Rust backend.".to_string(),
        });
        state
    }

    /// Return every note, newest first.
    pub fn list(&self) -> Vec<Note> {
        let guard = self.notes.lock().expect("notes mutex poisoned");
        let mut notes: Vec<Note> = guard.values().cloned().collect();
        notes.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        notes
    }

    /// Insert a new note and return the stored copy (with server fields).
    pub fn create(&self, input: CreateNote) -> Note {
        // `fetch_add` returns the *previous* value and bumps the counter
        // atomically, so concurrent creators never collide.
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let note = Note {
            id,
            title: input.title,
            body: input.body,
            created_at: now_millis(),
        };
        self.notes
            .lock()
            .expect("notes mutex poisoned")
            .insert(id, note.clone());
        note
    }

    /// Remove a note by id; `true` if something was actually deleted.
    pub fn delete(&self, id: u64) -> bool {
        self.notes
            .lock()
            .expect("notes mutex poisoned")
            .remove(&id)
            .is_some()
    }
}

impl Default for AppState {
    /// Same as [`AppState::new`]; satisfies `clippy::new_without_default`.
    fn default() -> Self {
        Self::new()
    }
}

/// Current time as Unix milliseconds (like JS `Date.now()`).
fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before 1970")
        .as_millis()
}
