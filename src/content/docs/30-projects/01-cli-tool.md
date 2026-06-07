---
title: "Project 2: CLI Tool (a task manager)"
description: "Build taskr, a Rust CLI task manager with clap, serde, and anyhow: subcommands, JSON persistence, and a single static binary with no node_modules."
---

This project builds **`taskr`**, a command-line task manager: you `add` tasks,
`list` them, mark them `done`, `remove` them, and `clear` the lot. Tasks persist
to a JSON file in your operating system's config directory, so they survive
between runs. Exactly the kind of small, sharp tool you might reach for
[`commander`](https://github.com/tj/commander.js) or
[`oclif`](https://oclif.io/) to build in Node.

The point is not the to-do list (the world has enough of those). The point is
that a real CLI exercises a surprising amount of Rust at once: argument parsing,
subcommands, file IO, JSON serialization, error propagation, and colored
terminal output. By the end you will have a single statically-linked binary you
can drop on any machine: no `node_modules`, no runtime to install.

> **Built and verified with:** Rust 1.96.0 (2024 edition), `clap` 4.6, `serde`
> 1.0, `serde_json` 1.0, `anyhow` 1.0, `owo-colors` 4.3, `directories` 6.0, and
> `chrono` 0.4. Every command and every snippet of output below was produced by
> actually running the code in this directory.

## What You'll Build

A binary called `taskr` with five subcommands:

| Command | What it does | Node analogy |
| --- | --- | --- |
| `taskr add <text...>` | Add a task (multiple words allowed, no quotes needed) | `commander`'s `.argument('<text...>')` |
| `taskr list [--pending\|--done]` | List tasks, optionally filtered | `.option('--pending')` |
| `taskr done <id>` | Mark a task complete | a positional `<id>` argument |
| `taskr remove <id>` | Delete a task | — |
| `taskr clear [--yes]` | Delete every task (with a confirmation prompt) | `inquirer`/`prompts` confirm |

A session looks like this (colors are rendered in a real terminal; shown here as
plain text):

```text
$ taskr add Buy milk
Added task #1 "Buy milk"

$ taskr add "Write the CLI chapter"
Added task #2 "Write the CLI chapter"

$ taskr list
Tasks (0/2 done)
  [ ] #1 Buy milk
  [ ] #2 Write the CLI chapter

$ taskr done 2
Completed #2 "Write the CLI chapter"

$ taskr list
Tasks (1/2 done)
  [ ] #1 Buy milk
  [x] #2 Write the CLI chapter
```

In a real terminal, `Added`/`Completed` are green and bold, ids like `#1` are
cyan, an open `[ ]` checkbox is yellow, a finished `[x]` is green, and a
completed task's text is dimmed and struck through.

The data lives in a JSON file, so you can inspect or hand-edit it:

```json
[
  {
    "id": 2,
    "title": "Write the CLI chapter",
    "done": true,
    "created_at": "2026-06-02T10:05:14.765081+03:00",
    "completed_at": "2026-06-02T10:05:15.821210+03:00"
  },
  {
    "id": 3,
    "title": "Ship it",
    "done": false
  }
]
```

## Prerequisites

This project ties together threads from across the guide. If any of these feel
shaky, the linked sections are worth a refresher:

- [01 Getting Started](/01-getting-started/) — `cargo new`, `cargo
  run`, project layout.
- [06 Data Structures](/06-data-structures/) — `struct`s and `enum`s;
  the CLI itself is modeled as an `enum`.
- [07 Collections](/07-collections/) — `Vec`, iterators, and
  `.filter()`/`.find()`.
- [08 Error Handling](/08-error-handling/) — `Result`, the `?`
  operator, and especially [anyhow & thiserror](/08-error-handling/06-anyhow-thiserror/).
- [12 Modules & Packages](/12-modules-packages/) — splitting code
  across files with `mod` and `use`.
- [15 Serialization](/15-serialization/) — `serde` derive macros and
  [`serde_json`](/15-serialization/03-json/).
- [18 CLI Tools](/18-cli-tools/) — the full coverage of
  [`clap` derive](/18-cli-tools/01-clap-derive/),
  [subcommands](/18-cli-tools/02-subcommands/),
  [colored output](/18-cli-tools/05-colored-output/), and
  [path handling](/18-cli-tools/07-path-handling/).

## Project Structure

The code lives in [`cli-tool-code/`](https://github.com/zeybek/rs4ts/tree/main/examples/cli-tool-code). It is a small, idiomatic
multi-file binary crate. Each file has one job, the way you'd split a Node CLI
into `cli.ts`, `store.ts`, `commands.ts`, and `types.ts`:

```text
cli-tool-code/
├── Cargo.toml          # package metadata + dependency versions
├── Cargo.lock          # exact resolved versions (commit this for a binary)
└── src/
    ├── main.rs         # entry point: parse args, open the store, dispatch
    ├── cli.rs          # the CLI shape (clap derive structs/enums)
    ├── commands.rs     # one function per subcommand + colored output
    ├── store.rs        # load/save the JSON file in the config dir
    └── task.rs         # the Task model + (de)serialization
```

> **Why split it up?** In Node you'd happily put this in one `index.js`. You can
> do that in Rust too, but separating the *shape* of the CLI (`cli.rs`), the
> *data* (`task.rs`), the *persistence* (`store.rs`), and the *behavior*
> (`commands.rs`) keeps each module testable in isolation. See
> [12 Modules & Packages](/12-modules-packages/00-modules/).

## Walkthrough

### Step 1 — Scaffold the project

`cargo new` creates a binary crate. The `--name` flag sets the binary name
independently of the directory name:

```bash
cargo new --bin cli-tool-code --name taskr
cd cli-tool-code
```

Then add the dependencies. `cargo add` (built into Cargo since 1.62 — no
`cargo-edit` needed) writes the latest compatible versions into `Cargo.toml`:

```bash
cargo add clap --features derive
cargo add serde --features derive
cargo add serde_json
cargo add anyhow
cargo add owo-colors
cargo add directories
cargo add chrono --features serde
```

The resulting `Cargo.toml`:

```toml
[package]
name = "taskr"
version = "0.1.0"
edition = "2024"

[dependencies]
anyhow = "1.0.102"
chrono = { version = "0.4.44", features = ["serde"] }
clap = { version = "4.6.1", features = ["derive"] }
directories = "6.0.0"
owo-colors = "4.3.0"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
```

> **`package.json` vs `Cargo.toml`.** The mental model is similar, with two big
> differences. First, `"1.0.102"` is a **caret** range (`^1.0.102`), not an exact
> pin. Cargo will accept `1.x` but never `2.0`. Second, **features** replace the
> "install this plugin / sub-package" pattern: `serde` and `clap` ship their
> derive macros behind a `derive` feature you opt into, instead of as separate
> packages like `@types/...`. `Cargo.lock` plays the role of
> `package-lock.json`; commit it for a binary.

Here is what each crate does, and its Node counterpart:

| Crate | Role | Node analogy |
| --- | --- | --- |
| `clap` | Argument parsing, subcommands, `--help` | `commander` / `oclif` / `yargs` |
| `serde` + `serde_json` | (De)serialize structs to/from JSON | `JSON.parse` / `JSON.stringify` (but type-checked) |
| `anyhow` | Ergonomic application error handling | `try/catch` with rich error messages |
| `owo-colors` | ANSI terminal colors | `chalk` / `picocolors` |
| `directories` | Per-OS config/data paths | `env-paths` |
| `chrono` | Date/time types | `Date` / `dayjs` |

### Step 2 — Model a task (`src/task.rs`)

Start with the data. A `Task` is a plain struct, and the two `derive` attributes
generate the JSON (de)serialization at compile time:

```rust
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
```

A few things a TypeScript developer should notice:

- **`#[derive(Serialize, Deserialize)]`** is the headline. It is *not* a runtime
  decorator like you'd see in a NestJS DTO; it is a macro that runs at compile
  time and generates real `serialize`/`deserialize` methods for `Task`. The
  upshot: when you load `tasks.json`, the bytes are *parsed into a typed `Task`*,
  and malformed data is a recoverable error, not an `any` that blows up three
  functions later. Contrast with `JSON.parse(fs.readFileSync(...))` in Node,
  which hands you an untyped object you have to trust. See
  [15 Serialization › derive Serialize](/15-serialization/02-derive-serialize/).
- **`Option<DateTime<Local>>`** is Rust's answer to `completedAt?: Date`. The
  `#[serde(skip_serializing_if = "Option::is_none", default)]` attribute means a
  task that isn't done omits the `completed_at` key entirely (you saw that in the
  JSON sample above), and a missing key deserializes back to `None`. See
  [06 Data Structures › Option](/06-data-structures/03-option-enum/) and
  [15 Serialization › attributes](/15-serialization/05-attributes/).

The `impl` block holds the two operations a task supports:

```rust
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
```

`mark_done` takes `&mut self` (a mutable borrow) because it changes the task in
place, and returns a `bool` so the command layer can tell "I just completed it"
from "it was already done." This is the kind of small, honest API the borrow
checker nudges you toward. See [05 Ownership › mutable references](/05-ownership/03-mutable-references/).

The module ends with unit tests, colocated in the same file under
`#[cfg(test)]`, the way Rust likes it (see [13 Testing](/13-testing/)):

```rust
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
```

### Step 3 — Persist to a JSON file (`src/store.rs`)

The `Store` owns both the on-disk path and the in-memory `Vec<Task>`. It is the
only module that touches the filesystem:

```rust
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
```

The constructors load the file if it exists (or start empty on first run), and
let `TASKR_STORE` override the path; invaluable for tests and reproducible
demos:

```rust
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
```

The CRUD operations are ordinary `Vec` work. Notice `next_id` and how `remove`
returns an `Option<Task>` — `None` *is* the "not found" case, so the caller never
has to guess:

```rust
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
```

> **Reading `remove`.** `self.tasks.iter().position(...)?` finds the index of the
> matching task. If there is no match, `position` returns `None`, and the `?`
> *returns `None` from the whole function*. That single character replaces an
> `if (index === -1) return undefined;`. See [08 Error Handling › the `?`
> operator](/08-error-handling/01-question-mark/).

Saving writes pretty-printed JSON, creating the config directory first. This is
where `anyhow`'s `.context()` earns its keep: every failure carries a
human-readable note about *what we were doing*:

```rust
    /// Persist the current task list back to disk as pretty-printed JSON.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("creating config directory {}", parent.display())
            })?;
        }
        let json = serde_json::to_string_pretty(&self.tasks)
            .context("serializing tasks to JSON")?;
        fs::write(&self.path, json)
            .with_context(|| format!("writing {}", self.path.display()))?;
        Ok(())
    }
}
```

The free functions compute the path and parse the file. `directories` does the
cross-platform path resolution Node developers usually pull in `env-paths` for:

```rust
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
    let bytes = fs::read(path)
        .with_context(|| format!("reading {}", path.display()))?;
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    let tasks: Vec<Task> = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {} as a task list", path.display()))?;
    Ok(tasks)
}
```

> **`with_context` vs `context`.** Use `.context("static string")` when the
> message is constant, and `.with_context(|| format!(...))` when building the
> message costs something (here, formatting a path). The closure form is lazy:
> it only runs *if* there is an error, so the happy path pays nothing. This is
> the central pattern of [anyhow & thiserror](/08-error-handling/06-anyhow-thiserror/).

This module also carries tests that write to a temp file and reload it, proving
the round-trip works (the full set lives in
[`src/store.rs`](https://github.com/zeybek/rs4ts/blob/main/examples/cli-tool-code/src/store.rs)):

```rust
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
```

### Step 4 — Describe the CLI (`src/cli.rs`)

Here's the part that feels most different from Node. Instead of *registering*
commands imperatively (`program.command('add').argument(...).action(...)`), you
*describe* the CLI as a data type and let `clap`'s derive macro generate the
parser, the `--help` text, `--version`, and all the error messages:

```rust
//! Command-line interface, declared with clap's derive API.
//!
//! This is the Rust counterpart of a `commander`/`oclif` setup in Node: instead
//! of registering commands imperatively, we describe the CLI as a data type and
//! clap generates the parser, `--help`, `--version`, and error messages for us.

use clap::{Parser, Subcommand};

/// `taskr` — a tiny task manager that persists to a JSON file.
#[derive(Debug, Parser)]
#[command(
    name = "taskr",
    version,
    about = "A tiny task manager (TypeScript-developer's guide to Rust)",
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}
```

The subcommands are an `enum`. Each variant is a subcommand; its fields become
positional arguments or flags depending on the `#[arg(...)]` attributes:

```rust
/// The subcommands the tool understands. Each variant becomes a subcommand;
/// its fields become positional args or flags.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add a new task.
    Add {
        /// The task description. Multiple words are joined with spaces, so you
        /// can write `taskr add buy milk` without quotes.
        #[arg(required = true, num_args = 1.., value_name = "TEXT")]
        title: Vec<String>,
    },
    /// List tasks.
    List {
        /// Show only tasks that are still open.
        #[arg(long, conflicts_with = "done")]
        pending: bool,
        /// Show only tasks that are already done.
        #[arg(long)]
        done: bool,
    },
    /// Mark a task as done by id.
    Done {
        /// The id shown by `taskr list`.
        id: u32,
    },
    /// Remove a task by id.
    Remove {
        /// The id shown by `taskr list`.
        id: u32,
    },
    /// Delete every task (asks for confirmation unless `--yes`).
    Clear {
        /// Skip the confirmation prompt.
        #[arg(long, short = 'y')]
        yes: bool,
    },
}
```

This tiny declaration buys a lot:

- **The doc comments become the help text.** The `///` over `Add` is exactly
  what shows up next to `add` in `taskr --help`. No duplicated description
  strings.
- **`id: u32`** means clap parses and *validates* the id for you. Pass `taskr
  done abc` and clap rejects it before your code runs; you never see a `NaN`.
- **`title: Vec<String>` with `num_args = 1..`** collects all trailing words, so
  `taskr add buy some milk` works without quoting, and `required = true` rejects
  an empty `add`.
- **`conflicts_with = "done"`** makes `--pending` and `--done` mutually
  exclusive; clap enforces it and explains the conflict if you pass both.

> **The big shift.** In `commander`, the CLI is *code that runs*. In `clap`
> derive, the CLI is *a type that exists*. Adding a subcommand means adding an
> `enum` variant; the compiler then forces you to handle it in the `match` (Step
> 5) or your code won't build. There is no way to forget to wire up a command.
> More on this: [18 CLI Tools › clap derive](/18-cli-tools/01-clap-derive/) and
> [subcommands](/18-cli-tools/02-subcommands/).

### Step 5 — Implement the commands (`src/commands.rs`)

`run` is the dispatcher. It `match`es on the parsed command, and because
`Command` is an `enum`, the compiler guarantees every variant is handled
(exhaustiveness). Forget one and the program won't compile:

```rust
//! The actual behavior behind each subcommand.
//!
//! Each function takes the parsed arguments plus a mutable `Store`, does its
//! work, prints colored feedback with `owo-colors`, and returns a `Result` so
//! that any IO failure bubbles up to `main` via the `?` operator.

use std::io::{self, Write};

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::cli::Command;
use crate::store::Store;
use crate::task::Task;

/// Dispatch a parsed command against the store, saving when needed.
pub fn run(command: Command, store: &mut Store) -> Result<()> {
    match command {
        Command::Add { title } => add(store, title)?,
        Command::List { pending, done } => list(store, pending, done),
        Command::Done { id } => done(store, id)?,
        Command::Remove { id } => remove(store, id)?,
        Command::Clear { yes } => clear(store, yes)?,
    }
    Ok(())
}
```

`add` joins the words, stores the task, **saves**, then prints a colored
confirmation. The `.green().bold()` calls come from `owo-colors`'
`OwoColorize` trait, Rust's `chalk`:

```rust
fn add(store: &mut Store, title_words: Vec<String>) -> Result<()> {
    let title = title_words.join(" ");
    let id = store.add(title.clone());
    store.save()?;
    println!(
        "{} task {} {}",
        "Added".green().bold(),
        format!("#{id}").cyan(),
        format!("\"{title}\"").dimmed()
    );
    Ok(())
}
```

> **`format!("#{id}")`** uses an *inline* format argument: the `id` variable is
> named directly inside the braces, the same idea as a JavaScript template
> literal `` `#${id}` ``. This has been the idiomatic style since Rust 1.58.

`list` filters with an iterator and prints a header plus one styled line per
task. The completed/open distinction drives the checkbox color and whether the
title is dimmed and struck through:

```rust
fn list(store: &Store, pending_only: bool, done_only: bool) {
    let tasks: Vec<&Task> = store
        .tasks()
        .iter()
        .filter(|task| {
            if pending_only {
                !task.done
            } else if done_only {
                task.done
            } else {
                true
            }
        })
        .collect();

    if tasks.is_empty() {
        println!("{}", "No tasks yet. Add one with `taskr add ...`.".dimmed());
        return;
    }

    let total = store.tasks().len();
    let completed = store.tasks().iter().filter(|task| task.done).count();
    println!(
        "{}",
        format!("Tasks ({completed}/{total} done)").bold().underline()
    );

    for task in tasks {
        let checkbox = if task.done {
            "[x]".green().to_string()
        } else {
            "[ ]".yellow().to_string()
        };
        let id = format!("#{}", task.id);
        // Dim and strike-through-ish styling for completed items.
        let title = if task.done {
            task.title.dimmed().strikethrough().to_string()
        } else {
            task.title.to_string()
        };
        println!("  {checkbox} {} {title}", id.cyan());
    }
}
```

`done` looks the task up *mutably*, calls `mark_done`, and reports either
"Completed" or a "was already done" note. If there's no such id, it returns an
error (more on that below):

```rust
fn done(store: &mut Store, id: u32) -> Result<()> {
    match store.get_mut(id) {
        Some(task) => {
            if task.mark_done() {
                let title = task.title.clone();
                store.save()?;
                println!(
                    "{} {} {}",
                    "Completed".green().bold(),
                    format!("#{id}").cyan(),
                    format!("\"{title}\"").dimmed()
                );
            } else {
                println!(
                    "{} #{id} was already done.",
                    "Note:".yellow().bold()
                );
            }
        }
        None => return Err(missing(id)),
    }
    Ok(())
}
```

`remove` mirrors it, and `clear` shows off an interactive confirmation prompt
(the Rust equivalent of `inquirer`/`prompts`) unless `--yes` is passed:

```rust
fn remove(store: &mut Store, id: u32) -> Result<()> {
    match store.remove(id) {
        Some(task) => {
            store.save()?;
            println!(
                "{} task {} {}",
                "Removed".red().bold(),
                format!("#{id}").cyan(),
                format!("\"{}\"", task.title).dimmed()
            );
        }
        None => return Err(missing(id)),
    }
    Ok(())
}

fn clear(store: &mut Store, skip_confirm: bool) -> Result<()> {
    if store.tasks().is_empty() {
        println!("{}", "Nothing to clear.".dimmed());
        return Ok(());
    }
    if !skip_confirm && !confirm("Delete ALL tasks? [y/N] ")? {
        println!("{}", "Aborted.".dimmed());
        return Ok(());
    }
    let count = store.clear();
    store.save()?;
    println!(
        "{} {count} task(s).",
        "Cleared".red().bold()
    );
    Ok(())
}
```

Finally, the two helpers. `missing` builds the "not found" error, and `confirm`
prints a prompt and reads a line from stdin — note the explicit
`io::stdout().flush()`, because Rust's stdout is line-buffered and a `print!`
without a newline wouldn't appear until you flush:

```rust
/// Build a consistent "no such task" error. Returning it from a command makes
/// `main` print `Error: ...` to stderr and exit with a non-zero status, the way
/// a Node CLI would `process.exit(1)`.
fn missing(id: u32) -> anyhow::Error {
    anyhow::anyhow!("no task with id #{id}")
}

/// Prompt on stdout and read a yes/no answer from stdin.
fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES"))
}
```

### Step 6 — Wire it together (`src/main.rs`)

`main` is tiny, and that's the point: declare the modules, parse the args, open
the store, dispatch. The return type `anyhow::Result<()>` is the keystone:

```rust
//! `taskr` entry point.
//!
//! `main` returns `anyhow::Result<()>`: any error produced with `?` is printed
//! with its full context chain and the process exits non-zero — the Rust way to
//! do what an unhandled `throw` plus `process.exit(1)` does in Node.

mod cli;
mod commands;
mod store;
mod task;

use anyhow::Result;
use clap::Parser;

use crate::cli::Cli;
use crate::store::Store;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut store = Store::open_default()?;
    commands::run(cli.command, &mut store)?;
    Ok(())
}
```

When `main` returns `Err(...)`, Rust prints `Error:` followed by the message and
its context chain to **stderr**, and the process exits with a non-zero code. You
get the Node "unhandled rejection crashes the process with a useful trace"
behavior, but explicit, typed, and without a single `try/catch`. See
[08 Error Handling › best practices](/08-error-handling/08-best-practices/).

## Running It

Build it once, then either run through `cargo run --` or call the compiled
binary directly. For a reproducible demo, point the store at a temp file with
`TASKR_STORE`:

```bash
cargo build
export TASKR_STORE=/tmp/taskr-demo.json
```

**Help text** (generated entirely by clap from the doc comments):

```bash
cargo run -q -- --help
```

```text
A tiny task manager (TypeScript-developer's guide to Rust)

Usage: taskr <COMMAND>

Commands:
  add     Add a new task
  list    List tasks
  done    Mark a task as done by id
  remove  Remove a task by id
  clear   Delete every task (asks for confirmation unless `--yes`)
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

**Add a few tasks** (each `add` saves to disk and prints a colored line):

```bash
cargo run -q -- add Buy milk
cargo run -q -- add "Write the CLI chapter"
cargo run -q -- add Ship it
```

```text
Added task #1 "Buy milk"
Added task #2 "Write the CLI chapter"
Added task #3 "Ship it"
```

**List, complete one, and list again:**

```bash
cargo run -q -- list
cargo run -q -- done 2
cargo run -q -- list
```

```text
Tasks (0/3 done)
  [ ] #1 Buy milk
  [ ] #2 Write the CLI chapter
  [ ] #3 Ship it

Completed #2 "Write the CLI chapter"

Tasks (1/3 done)
  [ ] #1 Buy milk
  [x] #2 Write the CLI chapter
  [ ] #3 Ship it
```

**Filter, remove, and trigger an error:**

```bash
cargo run -q -- list --pending
cargo run -q -- list --done
cargo run -q -- remove 1
cargo run -q -- done 99
```

```text
Tasks (1/3 done)
  [ ] #1 Buy milk
  [ ] #3 Ship it

Tasks (1/3 done)
  [x] #2 Write the CLI chapter

Removed task #1 "Buy milk"

Error: no task with id #99
```

That last command writes `Error: ...` to **stderr** and exits non-zero, which
you can confirm:

```bash
cargo run -q -- done 99; echo "exit code: $?"
```

```text
Error: no task with id #99
exit code: 1
```

clap's own validation produces a similar failure for a bad argument type,
before any of your code runs:

```bash
cargo run -q -- done abc
```

```text
error: invalid value 'abc' for '<ID>': invalid digit found in string

For more information, try '--help'.
```

**The confirmation prompt** on `clear` (answering `n` aborts):

```bash
echo "n" | cargo run -q -- clear
```

```text
Delete ALL tasks? [y/N] Aborted.
```

**Skip it with `--yes`:**

```bash
cargo run -q -- clear --yes
cargo run -q -- list
```

```text
Cleared 2 task(s).
No tasks yet. Add one with `taskr add ...`.
```

**The unit tests** all pass:

```bash
cargo test
```

```text
running 6 tests
test store::tests::add_assigns_increasing_ids ... ok
test task::tests::mark_done_is_idempotent ... ok
test task::tests::round_trips_through_json ... ok
test store::tests::remove_returns_the_task ... ok
test task::tests::new_task_is_not_done ... ok
test store::tests::save_then_reload_preserves_tasks ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**A release build** produces a single self-contained binary: no runtime, no
`node_modules`:

```bash
cargo build --release
./target/release/taskr --version
```

```text
taskr 0.1.0
```

Drop `target/release/taskr` (or `taskr.exe` on Windows) on any compatible
machine and it just runs. That portability is one of the best reasons to reach
for Rust over Node when you're shipping a CLI to other people. See
[18 CLI Tools › distribution](/18-cli-tools/10-distribution/).

## Key Concepts

This project cements a cluster of Rust ideas. Here's the map back to the rest
of the guide:

- **Enums model the command set.** `Command` is a sum type, and the `match` in
  `run` is *exhaustive*: add a variant and the compiler makes you handle it.
  This is the safety net `switch`-on-a-string CLIs in Node never had.
  [06 Data Structures › enums](/06-data-structures/02-enums/),
  [04 Control Flow › match](/04-control-flow/02-match/).
- **Derive macros do the boring work.** `#[derive(Parser)]`, `#[derive(Subcommand)]`,
  and `#[derive(Serialize, Deserialize)]` generate the parser and the JSON codec
  at compile time. They are *not* runtime decorators. [14 Macros](/14-macros/),
  [15 Serialization › derive](/15-serialization/02-derive-serialize/).
- **`Result` + `?` + `anyhow` replace `try/catch`.** Every fallible step returns
  a `Result`; `?` propagates failures; `.context()` annotates them; `main`
  returning `Result` turns an error into a clean stderr message and a non-zero
  exit. [08 Error Handling](/08-error-handling/).
- **`Option` is "maybe."** `Store::remove` returns `Option<Task>`; `completed_at`
  is `Option<DateTime>`. The compiler won't let you ignore the `None` case, so
  there is no `undefined is not a function` at 2 a.m.
  [06 Data Structures › Option](/06-data-structures/03-option-enum/).
- **Ownership and borrowing show up naturally.** `list` takes `&Store` (read-only),
  the mutating commands take `&mut Store`, and `get_mut` hands back a `&mut Task`.
  [05 Ownership](/05-ownership/).
- **Iterators are the data-wrangling toolkit.** `.iter().filter(...).collect()`,
  `.find(...)`, `.position(...)`, `.max()`: the same shape as `Array.prototype`
  methods, but lazy and zero-cost. [07 Collections](/07-collections/).
- **Modules keep it tidy.** One responsibility per file, wired with `mod`/`use`.
  [12 Modules & Packages](/12-modules-packages/00-modules/).

## Extending It

The project is deliberately small so you have room to grow it. Some concrete
next steps:

1. **Add `edit` and `priority`.** Add a `Command::Edit { id, text }` variant and
   a `priority: Option<u8>` field on `Task`. The compiler will walk you straight
   to every place you need to update: that's the enum-exhaustiveness payoff.
2. **Swap the JSON file for a real database.** The `Store` API (`add`/`get_mut`/
   `remove`/`save`) is the seam. Re-implement it on top of in-memory SQLite via
   [`rusqlite`](https://docs.rs/rusqlite), or async [`sqlx`](https://docs.rs/sqlx),
   and the command layer doesn't change. The guide's
   [17 Database](/17-database/) section (especially
   [sqlx-intro](/17-database/00-sqlx-intro/)) shows how.
3. **Add a progress bar or spinner.** For a longer operation (say, syncing tasks
   to a server), wrap it in an [`indicatif`](https://docs.rs/indicatif) progress
   bar. See [18 CLI Tools › progress bars](/18-cli-tools/04-progress-bars/).
4. **Add shell completions.** `clap_complete` can generate bash/zsh/fish
   completion scripts straight from your `Cli` type: another freebie from
   describing the CLI as data. See
   [18 CLI Tools › distribution](/18-cli-tools/10-distribution/).
5. **Respect `NO_COLOR` and TTY detection.** `owo-colors` has an
   `if_supports_color` helper so output stays clean when piped to a file or a
   non-color terminal. See [18 CLI Tools › colored output](/18-cli-tools/05-colored-output/).

## Further Reading

- [18 CLI Tools](/18-cli-tools/) — the full chapter behind this
  project: [clap derive](/18-cli-tools/01-clap-derive/),
  [subcommands](/18-cli-tools/02-subcommands/),
  [colored output](/18-cli-tools/05-colored-output/),
  [file IO](/18-cli-tools/06-file-io/),
  [path handling](/18-cli-tools/07-path-handling/),
  [environment variables](/18-cli-tools/08-environment-vars/),
  [cross-platform](/18-cli-tools/09-cross-platform/),
  [distribution](/18-cli-tools/10-distribution/).
- [08 Error Handling › anyhow & thiserror](/08-error-handling/06-anyhow-thiserror/)
- [15 Serialization](/15-serialization/) and [serde_json](/15-serialization/03-json/)
- [13 Testing](/13-testing/)
- Official docs: [clap](https://docs.rs/clap), [serde](https://serde.rs/),
  [anyhow](https://docs.rs/anyhow), [owo-colors](https://docs.rs/owo-colors),
  [directories](https://docs.rs/directories), [chrono](https://docs.rs/chrono).
- Other projects in this section: [Project 1: REST API](/30-projects/00-rest-api/),
  [Project 3: WASM App](/30-projects/02-wasm-app/),
  [Project 4: WebSocket Chat](/30-projects/03-websocket-chat/),
  [Project 5: Microservice](/30-projects/04-microservice/),
  [Project 6: Full-Stack App](/30-projects/05-full-stack/).
