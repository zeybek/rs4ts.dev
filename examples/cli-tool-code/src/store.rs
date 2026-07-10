//! Persistence: load and save the task list as JSON in the user's config dir.
//!
//! This is the moral equivalent of reading/writing a `tasks.json` with `fs` in
//! Node, except every error is a typed `Result` rather than a thrown exception,
//! and `directories` figures out the right per-OS config path for us
//! (`~/.config/taskr/` on Linux, `~/Library/Application Support/...` on macOS,
//! `%APPDATA%\...` on Windows).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;

use crate::task::Task;

/// Owns the on-disk location and the in-memory list of tasks.
pub struct Store {
    path: PathBuf,
    tasks: Vec<Task>,
}

impl Store {
    /// Open the default store, creating the config directory and an empty
    /// `tasks.json` on first run. The path can be overridden with the
    /// `TASKR_STORE` environment variable, which is handy for tests and demos.
    pub fn open_default() -> Result<Self> {
        let path = default_store_path()?;
        Self::open_at(path)
    }

    /// Open (or initialize) a store at an explicit path.
    pub fn open_at(path: PathBuf) -> Result<Self> {
        let tasks = if path.exists() {
            load_tasks(&path)?
        } else {
            Vec::new()
        };
        Ok(Self { path, tasks })
    }

    /// Where this store lives on disk. Used by the tests and available to
    /// callers that want to print the config path.
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read-only view of all tasks.
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// Append a new task, allocating the next free id, and return its id.
    pub fn add(&mut self, title: String) -> u32 {
        let id = self.next_id();
        self.tasks.push(Task::new(id, title));
        id
    }

    /// Find a task by id for mutation, or `None` if no such id exists.
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|task| task.id == id)
    }

    /// Remove the task with `id`, returning it if it existed.
    pub fn remove(&mut self, id: u32) -> Option<Task> {
        let index = self.tasks.iter().position(|task| task.id == id)?;
        Some(self.tasks.remove(index))
    }

    /// Delete every task; returns how many were cleared.
    pub fn clear(&mut self) -> usize {
        let count = self.tasks.len();
        self.tasks.clear();
        count
    }

    /// The smallest id not yet used (max + 1, or 1 for an empty list).
    fn next_id(&self) -> u32 {
        self.tasks.iter().map(|task| task.id).max().unwrap_or(0) + 1
    }

    /// Persist the current task list back to disk as pretty-printed JSON.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating config directory {}", parent.display()))?;
        }
        let json =
            serde_json::to_string_pretty(&self.tasks).context("serializing tasks to JSON")?;
        // Write to a sibling temp file, then atomically rename it into place, so
        // a crash mid-write can't leave a half-written tasks.json that fails to
        // parse on the next run.
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, &self.path)
            .with_context(|| format!("replacing {}", self.path.display()))?;
        Ok(())
    }
}

/// Compute the default `tasks.json` path, honoring `TASKR_STORE` if set.
fn default_store_path() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("TASKR_STORE") {
        return Ok(PathBuf::from(custom));
    }
    let dirs = ProjectDirs::from("dev", "zeybek", "taskr")
        .context("could not determine a home directory for the config file")?;
    Ok(dirs.config_dir().join("tasks.json"))
}

/// Read and parse the task list, turning IO and JSON failures into context-rich
/// errors instead of panics.
fn load_tasks(path: &Path) -> Result<Vec<Task>> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    let tasks: Vec<Task> = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {} as a task list", path.display()))?;
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Build a store backed by a temporary file unique to this test. Cargo runs
    /// tests on multiple threads in one process, so the path includes a per-call
    /// counter (not just the pid) to keep concurrent tests from sharing a file.
    fn temp_store() -> Store {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("taskr-test-{}-{}.json", std::process::id(), n));
        let _ = fs::remove_file(&path);
        Store::open_at(path).unwrap()
    }

    #[test]
    fn add_assigns_increasing_ids() {
        let mut store = temp_store();
        assert_eq!(store.add("first".into()), 1);
        assert_eq!(store.add("second".into()), 2);
        assert_eq!(store.tasks().len(), 2);
    }

    #[test]
    fn remove_returns_the_task() {
        let mut store = temp_store();
        store.add("doomed".into());
        let removed = store.remove(1).expect("task should exist");
        assert_eq!(removed.title, "doomed");
        assert!(store.remove(1).is_none());
    }

    #[test]
    fn save_then_reload_preserves_tasks() {
        let mut store = temp_store();
        store.add("persisted".into());
        store.save().unwrap();
        let reloaded = Store::open_at(store.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.tasks().len(), 1);
        assert_eq!(reloaded.tasks()[0].title, "persisted");
        let _ = fs::remove_file(store.path());
    }
}
