---
title: "Project 1: REST API (Express → Axum)"
description: "Build a CRUD JSON API in Rust with Axum, porting an Express tasks service: typed errors, extractors, validation, and Arc/RwLock shared state."
---

This is the first of the capstone projects. You will build a small but
production-flavored JSON REST API for a `tasks` resource, the kind of service
you have written a hundred times in Express or NestJS, now in Rust with
[axum](https://docs.rs/axum) 0.8. It does full CRUD (list, get, create, update,
delete), returns consistent JSON error envelopes, validates input, logs every
request, and handles CORS, and it runs with **zero external services** thanks
to an in-memory store.

If you have built a Node service that looks like this:

```typescript
// app.ts (Express)
const app = express();
app.use(express.json());
app.use(cors());
app.get("/tasks", listTasks);
app.post("/tasks", createTask);
app.get("/tasks/:id", getTask);
app.put("/tasks/:id", updateTask);
app.delete("/tasks/:id", deleteTask);
app.listen(3000);
```

...then this project will feel familiar. The shapes line up almost
one-to-one. What changes is that Rust makes the *contracts* explicit: the
request body type, the error type, the shared state, and the response type are
all checked at compile time. There is no `res` object you can forget to send,
no `any` sneaking through, and no runtime `TypeError: Cannot read property`.

> **Stack at a glance**
> `axum` 0.8 (router + extractors + `axum::serve`), `tokio` (async runtime),
> `serde` / `serde_json` (JSON), `uuid` (ids), `time` (timestamps),
> `thiserror` (typed errors), `tower-http` (trace + CORS middleware), and
> `tracing` (structured logs).

## What You'll Build

A JSON API on `http://127.0.0.1:3000` with these endpoints:

| Method   | Path           | Description                       | Success status |
| -------- | -------------- | --------------------------------- | -------------- |
| `GET`    | `/health`      | Liveness probe                    | `200 OK`       |
| `GET`    | `/tasks`       | List all tasks                    | `200 OK`       |
| `POST`   | `/tasks`       | Create a task                     | `201 Created`  |
| `GET`    | `/tasks/{id}`  | Get one task by id                | `200 OK`       |
| `PUT`    | `/tasks/{id}`  | Update a task (partial)           | `200 OK`       |
| `DELETE` | `/tasks/{id}`  | Delete a task                     | `204 No Content` |

A `Task` looks like this on the wire:

```json
{
  "id": "fef960c2-0714-41ad-96b3-229623b39f6b",
  "title": "Buy milk",
  "description": "2 liters, oat",
  "completed": false,
  "created_at": "2026-06-02T07:08:24.058469Z",
  "updated_at": "2026-06-02T07:08:24.058469Z"
}
```

A sample exchange. Create a task, get `201` back with a generated id:

```bash
$ curl -s -X POST http://127.0.0.1:3000/tasks \
    -H 'Content-Type: application/json' \
    -d '{"title":"Buy milk","description":"2 liters, oat"}'
```

```json
{"id":"fef960c2-0714-41ad-96b3-229623b39f6b","title":"Buy milk","description":"2 liters, oat","completed":false,"created_at":"2026-06-02T07:08:24.058469Z","updated_at":"2026-06-02T07:08:24.058469Z"}
```

Errors are consistent and typed. A missing task:

```json
{"error":{"code":404,"message":"task not found"}}
```

A validation failure (`422`):

```json
{"error":{"code":422,"message":"validation failed: title must not be empty"}}
```

## Prerequisites

This project assembles ideas from earlier sections. If any step feels unfamiliar,
follow the link:

- [Section 11 — Async](/11-async/): `async fn`, `.await`, and the
  Tokio runtime. Axum handlers are `async`.
- [Section 11 — Arc/Mutex pattern](/11-async/12-arc-mutex-pattern/): shared
  mutable state across tasks, which is exactly our store.
- [Section 16 — Web APIs](/16-web-apis/): the axum fundamentals:
  [routing](/16-web-apis/03-routing/), [extractors](/16-web-apis/04-extractors/),
  [JSON APIs](/16-web-apis/08-json-apis/), [error handling](/16-web-apis/10-error-handling-web/),
  [validation](/16-web-apis/09-validation/), and [CORS](/16-web-apis/11-cors/).
- [Section 08 — Error handling](/08-error-handling/): `Result`, the
  `?` operator, and [custom errors with `thiserror`](/08-error-handling/06-anyhow-thiserror/).
- [Section 15 — Serialization](/15-serialization/): `serde`
  `Serialize` / `Deserialize` derives.
- [Section 13 — Testing](/13-testing/): the integration tests at the
  end use the same patterns.

You will also need a recent stable Rust toolchain (this project was built and
verified on Rust 2024 edition; the version sheet targets Rust 1.96.0).

## Project Structure

The code lives in [`rest-api-code/`](https://github.com/zeybek/rs4ts.dev/tree/main/examples/rest-api-code). It is a real Cargo
project with a clean module layout, not one giant `main.rs`:

```text
rest-api-code/
├── Cargo.toml         # dependencies (axum 0.8, tokio, serde, …) and metadata
├── Cargo.lock         # exact resolved versions (committed for apps)
├── src/
│   ├── main.rs        # binary entry point: init tracing, bind socket, serve
│   ├── lib.rs         # library root: declares the modules, re-exports app()
│   ├── error.rs       # AppError enum + IntoResponse (the error contract)
│   ├── models.rs      # Task + CreateTask/UpdateTask DTOs + validation
│   ├── store.rs       # in-memory Arc<RwLock<HashMap<Uuid, Task>>> store
│   ├── handlers.rs    # the CRUD handlers (the "controller" layer)
│   └── routes.rs      # router assembly + middleware (trace, CORS, state)
└── tests/
    └── api.rs         # in-process integration tests (no real socket)
```

> **Why split a library out from the binary?**
> Everything lives in `lib.rs` and its modules; `main.rs` is a thin wrapper.
> This lets the integration tests in `tests/api.rs` build the *exact same*
> `Router` and drive it in-process: the Rust analogue of pointing `supertest`
> at your Express `app` instance instead of a running server.

In Node you might split this as `src/index.ts`, `src/routes/tasks.ts`,
`src/models/task.ts`, `src/errors.ts`, `src/store.ts`. The mapping is direct.

## Walkthrough

We will build from the inside out: errors and models first (the data
contracts), then the store, then the handlers, then wire it all together in the
router and `main`.

### Step 1: Dependencies (`Cargo.toml`)

These were added with `cargo add`, which resolves the current version of each
crate automatically.

```toml
# Cargo.toml
# Empty [workspace] table makes this directory its own workspace root,
# so it is NOT absorbed into the parent guide's Cargo.toml.
[workspace]

[package]
name = "rest-api"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
thiserror = "2.0.18"
time = { version = "0.3.47", features = ["serde", "formatting", "parsing"] }
tokio = { version = "1.52.3", features = ["full"] }
tower-http = { version = "0.6.11", features = ["trace", "cors"] }
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }
uuid = { version = "1.23.2", features = ["v4", "serde"] }

[dev-dependencies]
http-body-util = "0.1.3"
tower = { version = "0.5.3", features = ["util"] }
```

This is your `package.json` dependency list. A few things a Node developer
should note:

- The `features = [...]` arrays are *compile-time feature flags*. Crates ship
  with optional functionality turned off by default, and you opt in to exactly
  what you use. For example, `uuid`'s `v4` feature pulls in random-UUID
  generation, and `serde` lets `uuid` serialize. There is no runtime cost for
  features you do not enable; it is closer to tree-shaking than to npm's "you
  get the whole package."
- `tokio`'s `full` feature is the kitchen-sink convenience flag (runtime,
  macros, networking, etc.). In production you would trim it.
- `[dev-dependencies]` only compile for tests/benches, like `devDependencies`
  in `package.json`. We use `tower`'s `util` (for `ServiceExt::oneshot`) and
  `http-body-util` (to read response bodies) only in the test file.

> **The `[workspace]` line.** This guide is one big repository, so the empty
> `[workspace]` table declares this folder as its own self-contained project.
> You do not need it in a standalone repo; `cargo new` omits it.

### Step 2: The error type (`src/error.rs`)

In Express you eventually centralize error handling into one middleware:

```typescript
// error-handler.ts (Express)
app.use((err, req, res, next) => {
  const status = err.status ?? 500;
  res.status(status).json({ error: { code: status, message: err.message } });
});
```

Rust does the same thing, but the "error handler" is a trait implementation on
a typed enum. Every failure mode is a named variant, and the `IntoResponse`
impl is the single place that decides status code and body.

```rust
// src/error.rs
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Every way a request can fail in this API.
///
/// `thiserror` derives the `std::error::Error` impl and the `Display`
/// messages from the `#[error("...")]` attributes — no boilerplate.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested task id does not exist.
    #[error("task not found")]
    NotFound,

    /// The request body failed validation. Carries a human-readable reason.
    #[error("validation failed: {0}")]
    Validation(String),

    /// The JSON body was malformed or had the wrong shape. Axum's own
    /// `JsonRejection` is converted into this variant (see the `From` impl).
    #[error("invalid request body: {0}")]
    BadRequest(String),
}

impl AppError {
    /// The HTTP status code this error maps to.
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
        }
    }
}

/// This is what lets a handler return `Result<_, AppError>`: axum calls
/// `into_response()` on the `Err` value to build the actual HTTP response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        // A consistent error envelope, like you'd standardize in an Express
        // error handler: { "error": { "code": 404, "message": "..." } }.
        let body = Json(json!({
            "error": {
                "code": status.as_u16(),
                "message": self.to_string(),
            }
        }));
        (status, body).into_response()
    }
}

/// Convert axum's built-in JSON extractor rejection into our error type.
///
/// With this in place, `Json<CreateTask>` failures (bad syntax, missing
/// `Content-Type`, wrong field types) become a clean `400` JSON response
/// instead of axum's default plain-text error.
impl From<axum::extract::rejection::JsonRejection> for AppError {
    fn from(rejection: axum::extract::rejection::JsonRejection) -> Self {
        AppError::BadRequest(rejection.body_text())
    }
}
```

What is happening here, in TypeScript terms:

- `#[derive(thiserror::Error)]` generates the equivalent of a custom
  `Error` subclass, including the `message` (from `#[error("...")]`). See
  [anyhow vs thiserror](/08-error-handling/06-anyhow-thiserror/).
- The `match` in `status()` is an exhaustive `switch`. If you add a new
  variant later and forget to give it a status, the compiler *refuses to
  build*. That is the safety net Express never gave you.
- `impl IntoResponse for AppError` is the trait that axum requires for any type
  returned from a handler. By implementing it once, we let every handler write
  `-> Result<Json<Task>, AppError>` and just `?`-propagate errors.
- The `From<JsonRejection>` impl is the clever part: it means the `?` operator
  can turn axum's own body-parsing failures into our typed `BadRequest`. We will
  use that in the create/update handlers.

> **Callout: `?` is `try/catch` without the ceremony.** Where Node forces
> `try { ... } catch (e) { next(e); }`, Rust's `?` operator unwraps a `Result`
> on success or returns the error early on failure. And because of the `From`
> impls above, it auto-converts the error into `AppError` on the way out.

### Step 3: Models and validation (`src/models.rs`)

This is your `interface Task` plus the `class CreateTaskDto` you would decorate
with `class-validator` in NestJS, except validation is plain code, not
decorators.

```rust
// src/models.rs
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
```

The three structs map to three TypeScript types:

```typescript
// equivalent TypeScript shapes
interface Task {
  id: string;
  title: string;
  description: string;
  completed: boolean;
  created_at: string; // RFC 3339
  updated_at: string;
}
interface CreateTask { title: string; description?: string; completed?: boolean; }
interface UpdateTask { title?: string; description?: string; completed?: boolean; }
```

Key correspondences:

- `#[derive(Serialize, Deserialize)]` is what makes a struct convertible
  to/from JSON. `Task` derives both (it goes out *and* could come in);
  the DTOs derive only `Deserialize` (they only come in).
- `Option<String>` is the real type behind TypeScript's optional `title?:
  string`. The difference: TypeScript's optional can silently be `undefined`
  and blow up later; Rust's `Option` *forces* you to handle the `None` case.
  See [the Option enum](/06-data-structures/03-option-enum/).
- `#[serde(default)]` fills in a default (`""`, `false`) when the field is
  absent, like a default parameter value.

Now the validation and the small conversion helpers:

```rust
// src/models.rs (continued)
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
```

A few things worth a Node developer's attention:

- `into_task(self)` *consumes* the `CreateTask` (note `self`, not `&self`):
  it moves the strings into the new `Task` instead of cloning them. This is
  ownership doing zero-copy work for you. There is no equivalent footgun in
  JavaScript because everything is a reference, but there is also no guarantee
  you are not aliasing data you meant to copy.
- `apply_to(self, task: &mut Task)` mutates the existing task through a mutable
  reference. The `if let Some(x) = ...` pattern is "if this optional field was
  provided, use it." Fields left `None` are untouched — true partial update.
- `.chars().count()` counts *Unicode scalar values*, not bytes. A Node
  `string.length` counts UTF-16 code units, which over-counts emoji; Rust's
  `.len()` on a `String` counts UTF-8 bytes. Using `.chars().count()` is the
  closest to "number of characters a human sees."

> **Where a validation crate would go.** For richer rules you would reach for
> the `validator` crate and derive `#[validate(length(min = 1, max = 200))]`,
> mirroring `class-validator`. See [validation](/16-web-apis/09-validation/).
> Hand-rolled validation keeps this project dependency-light and explicit.

### Step 4: The in-memory store (`src/store.rs`)

No Postgres, no Redis — just a thread-safe `HashMap`. This is the one piece you
swap out for a real database later.

```rust
// src/store.rs
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
```

The type `Arc<RwLock<HashMap<Uuid, Task>>>` looks intimidating, so read it from
the inside out:

- `HashMap<Uuid, Task>` — the "table": keys are UUIDs, values are tasks. This
  is your `Map<string, Task>` in Node.
- `RwLock<...>`: a read/write lock. Many handlers can hold a *read* lock at
  once (`list`, `get`), but a *write* lock (`insert`, `update`, `delete`) is
  exclusive. In single-threaded Node you never think about this; in Rust the
  type system *forces* you to acknowledge that the API serves requests across
  multiple threads.
- `Arc<...>` — an atomically reference-counted pointer. It lets every handler
  share *the same* map. Cloning an `Arc` does not copy the map; it bumps a
  counter, like passing a reference around in JavaScript. See
  [reference counting](/05-ownership/07-reference-counting/) and the
  [Arc/Mutex pattern](/11-async/12-arc-mutex-pattern/).

Because `TaskStore` derives `Clone` and only holds an `Arc`, axum can hand a
cheap clone to every request while they all see the same data.

> **Why `RwLock` and not `Mutex`?** This is a read-heavy API (lots of `GET`s).
> `RwLock` lets concurrent reads proceed in parallel; a plain `Mutex` would
> serialize them. For a tutorial it barely matters, but it is the idiomatic
> choice.

> **`.expect("store lock poisoned")`.** A lock becomes "poisoned" only if a
> thread panics *while holding it*. Since our critical sections are trivial and
> cannot panic, this is effectively unreachable; `expect` documents the
> invariant. In a hardened service you would handle it explicitly.

### Step 5: The handlers (`src/handlers.rs`)

These are your Express route handlers / NestJS controller methods. Note how
each one declares exactly what it needs as a typed argument (the *extractors*),
and returns a typed value.

```rust
// src/handlers.rs
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

    // Load, mutate, write back. Note this read-then-write is NOT atomic:
    // two concurrent PUTs to the same id can interleave and one update wins
    // (a lost update). Fine for this in-memory demo; with a real DB you'd
    // make it a single `UPDATE ... RETURNING *` or use optimistic versioning.
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
```

This is where axum's design really diverges from Express, and it is worth
slowing down. Compare `get_task` with its Node twin:

```typescript
// Express
async function getTask(req: Request, res: Response) {
  const id = req.params.id;            // string, unvalidated
  const task = store.get(id);          // could be undefined
  if (!task) {
    return res.status(404).json({ error: { code: 404, message: "task not found" } });
  }
  res.json(task);                      // you must remember to send
}
```

```rust
// axum
pub async fn get_task(
    State(store): State<TaskStore>,    // dependency-injected state
    Path(id): Path<Uuid>,              // parsed + validated UUID
) -> Result<Json<Task>, AppError> {    // the return type IS the response
    let task = store.get(id).ok_or(AppError::NotFound)?;
    Ok(Json(task))
}
```

The differences that matter:

- **Extractors are typed arguments, not a `req` grab-bag.** `Path<Uuid>` does
  the `req.params.id` lookup *and* parses it into a real `Uuid`. If the path
  segment is not a valid UUID, the request is rejected with `400` before your
  code runs; you literally cannot receive a bad id. Compare:
  [extractors](/16-web-apis/04-extractors/).
- **`State<TaskStore>` is dependency injection.** No module-level singletons,
  no `req.app.locals`. The store is part of the router's typed state and axum
  hands a clone to each handler.
- **The return type is the response.** There is no `res` object, so it is
  impossible to forget to send one or to send twice. `Json(task)` *is* a
  `200 OK` JSON response; `Err(AppError::NotFound)` *is* a `404`.
- **`.ok_or(AppError::NotFound)?`** turns `Option<Task>` (the "maybe missing"
  result of a lookup) into `Result<Task, AppError>` and then `?`-returns the
  `404` early if it was `None`. That one line is the entire `if (!task) return
  res.status(404)...` block above.

For `create_task`, accepting `Result<Json<CreateTask>, JsonRejection>` instead
of plain `Json<CreateTask>` is a deliberate choice: it lets *us* own the error
response for bad bodies (a clean JSON `400`) via the `?` and the `From` impl
from Step 2, rather than axum's default plain-text rejection. See
[web error handling](/16-web-apis/10-error-handling-web/).

### Step 6: Routing and middleware (`src/routes.rs`)

This is `app.use(...)` + route registration, in one composable builder.

```rust
// src/routes.rs
use axum::{Router, routing::get};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::store::TaskStore;

/// Build the application `Router`, wiring routes to handlers, attaching
/// middleware, and injecting the shared store as state.
///
/// This is split out from `main` so integration tests can build the same app
/// and drive it in-process (see `tests/api.rs`).
pub fn app(store: TaskStore) -> Router {
    // A permissive CORS policy, fine for a demo / public read API. Tighten
    // `allow_origin` for production. See ../../16-web-apis/11_cors.md.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(handlers::health))
        // Collection routes: GET list + POST create.
        .route("/tasks", get(handlers::list_tasks).post(handlers::create_task))
        // Item routes: GET one + PUT update + DELETE. Note axum 0.8 uses
        // `{id}` (not the old `:id`) for path parameters.
        .route(
            "/tasks/{id}",
            get(handlers::get_task)
                .put(handlers::update_task)
                .delete(handlers::delete_task),
        )
        // `TraceLayer` logs each request/response (method, path, status,
        // latency) via the `tracing` crate — like `morgan` in Express.
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        // Inject the store so every handler can pull it out with `State`.
        .with_state(store)
}
```

Notes for the Node developer:

- **Methods chain onto a path.** `get(list_tasks).post(create_task)` registers
  both verbs on `/tasks`, the way you would call `router.route("/tasks").get(...).post(...)`.
- **`{id}`, not `:id`.** axum 0.8 changed path-parameter syntax to
  `{id}` (curly braces). If you copy an older tutorial using `:id`, it will not
  compile. See [routing](/16-web-apis/03-routing/).
- **`.layer(...)` is middleware.** `TraceLayer` is structured request logging
  (the `morgan`/`pino-http` of this stack); `CorsLayer` is your `cors()`
  middleware. These come from `tower-http`, a library of ready-made middleware
  ("layers") that work with any `tower`-based service.
- **`.with_state(store)`** binds the shared state to the router so the
  `State<TaskStore>` extractor in the handlers can find it.

> **CORS warning.** `allow_origin(Any)` mirrors a wide-open `cors()` call —
> fine for a demo, too loose for production. Restrict it to your front-end
> origin(s) before shipping; see [CORS](/16-web-apis/11-cors/).

### Step 7: Library root and entry point (`src/lib.rs`, `src/main.rs`)

`lib.rs` declares the modules and re-exports the two things the binary and
tests need, `app` and `TaskStore`:

```rust
// src/lib.rs
pub mod error;
pub mod handlers;
pub mod models;
pub mod routes;
pub mod store;

pub use routes::app;
pub use store::TaskStore;
```

`main.rs` is the thin runnable wrapper — `app.listen(3000)` in Node terms:

```rust
// src/main.rs
use std::net::SocketAddr;

use rest_api::{TaskStore, app};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() {
    // Structured logging. `RUST_LOG=info cargo run` controls verbosity; we
    // default to `info` for our crate and tower-http if `RUST_LOG` is unset.
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("rest_api=info,tower_http=info")),
        )
        .init();

    // Create the shared in-memory store and build the router.
    let store = TaskStore::new();
    let app = app(store);

    // Bind to 127.0.0.1:3000.
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    tracing::info!("listening on http://{addr}");

    // Serve until we receive a shutdown signal. `axum::serve` drives the
    // accept loop; `with_graceful_shutdown` stops accepting new connections
    // and lets in-flight requests finish — the equivalent of calling
    // `server.close()` on SIGTERM in Node.
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    tracing::info!("shutdown complete");
}

/// Resolve when the process receives Ctrl+C (SIGINT) or, on Unix, SIGTERM —
/// the signal orchestrators like Kubernetes and Docker send before a kill.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
```

The pieces:

- `#[tokio::main]` turns `async fn main` into a regular `main` that spins up the
  Tokio async runtime first. Rust has no built-in event loop the way Node does;
  you pick and start one. See [Tokio setup](/11-async/03-tokio-setup/).
- `EnvFilter` reads the `RUST_LOG` environment variable, so you control log
  verbosity per-crate at runtime (`RUST_LOG=rest_api=debug` for verbose).
- In axum 0.8 you create the `TcpListener` yourself and pass it to
  `axum::serve`. This is more explicit than `app.listen(3000)` but gives you
  control over the socket (e.g. binding to `0.0.0.0` in a container).
- Note the crate is referred to as `rest_api` (underscore) in code even though
  the package is `rest-api` (hyphen): Cargo normalizes hyphens to underscores
  for Rust identifiers.
- `shutdown_signal` waits for SIGINT or SIGTERM via `tokio::select!`, so a
  `Ctrl+C` in the terminal or a rolling deploy both drain cleanly instead of
  dropping in-flight requests. The full production pattern (readiness probes,
  bounded drain timeouts) is in
  [Section 28: Graceful Shutdown](/28-production/02-graceful-shutdown/).

## Running It

All commands run from inside [`rest-api-code/`](https://github.com/zeybek/rs4ts.dev/tree/main/examples/rest-api-code).

### Build

```bash
cargo build
```

Real output (first build compiles all dependencies; subsequent builds are
near-instant):

```text
   Compiling proc-macro2 v1.0.106
   # ... ~100 dependency crates elided (tokio, hyper, axum, serde, ...) ...
   Compiling time v0.3.47
   Compiling rest-api v0.1.0 (.../examples/rest-api-code)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.77s
```

### Run

```bash
RUST_LOG=rest_api=info,tower_http=info cargo run
```

Real startup output:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.85s
     Running `target/debug/rest-api`
2026-06-02T07:08:11.096300Z  INFO rest_api: listening on http://127.0.0.1:3000
```

The server is now listening. Open a second terminal for the `curl` calls below.

### Exercising the API with `curl`

Every response block below is **real output** captured from the running server.

**1. Health check**

```bash
curl -s -i http://127.0.0.1:3000/health
```

```text
HTTP/1.1 200 OK
content-type: application/json
vary: origin, access-control-request-method, access-control-request-headers
access-control-allow-origin: *
content-length: 15
date: Tue, 02 Jun 2026 07:08:16 GMT

{"status":"ok"}
```

**2. List tasks (empty to start)**

```bash
curl -s http://127.0.0.1:3000/tasks
```

```json
[]
```

**3. Create a task → `201 Created`**

```bash
curl -s -i -X POST http://127.0.0.1:3000/tasks \
  -H 'Content-Type: application/json' \
  -d '{"title":"Write the REST API chapter","description":"axum 0.8 + serde"}'
```

```text
HTTP/1.1 201 Created
content-type: application/json
content-length: 219
...

{"id":"5c2341ef-3ac9-4468-a15a-0852a9c0d7fa","title":"Write the REST API chapter","description":"axum 0.8 + serde","completed":false,"created_at":"2026-06-02T07:08:16.764237Z","updated_at":"2026-06-02T07:08:16.764237Z"}
```

**4. Get one task by id → `200 OK`**

```bash
curl -s http://127.0.0.1:3000/tasks/fef960c2-0714-41ad-96b3-229623b39f6b
```

```json
{"id":"fef960c2-0714-41ad-96b3-229623b39f6b","title":"Buy milk","description":"2 liters, oat","completed":false,"created_at":"2026-06-02T07:08:24.058469Z","updated_at":"2026-06-02T07:08:24.058469Z"}
```

**5. Update a task (mark completed) → `200 OK`**

A partial body: only `completed` is sent, the rest is preserved. Notice that
`updated_at` advances while `created_at` stays put.

```bash
curl -s -X PUT http://127.0.0.1:3000/tasks/fef960c2-0714-41ad-96b3-229623b39f6b \
  -H 'Content-Type: application/json' \
  -d '{"completed":true}'
```

```json
{"id":"fef960c2-0714-41ad-96b3-229623b39f6b","title":"Buy milk","description":"2 liters, oat","completed":true,"created_at":"2026-06-02T07:08:24.058469Z","updated_at":"2026-06-02T07:08:24.353693Z"}
```

**6. List again (now populated, sorted by `created_at`)**

```bash
curl -s http://127.0.0.1:3000/tasks
```

```json
[{"id":"5c2341ef-3ac9-4468-a15a-0852a9c0d7fa","title":"Write the REST API chapter","description":"axum 0.8 + serde","completed":false,"created_at":"2026-06-02T07:08:16.764237Z","updated_at":"2026-06-02T07:08:16.764237Z"},{"id":"fef960c2-0714-41ad-96b3-229623b39f6b","title":"Buy milk","description":"2 liters, oat","completed":true,"created_at":"2026-06-02T07:08:24.058469Z","updated_at":"2026-06-02T07:08:24.353693Z"}]
```

**7. Delete a task → `204 No Content`** (empty body)

```bash
curl -s -i -X DELETE http://127.0.0.1:3000/tasks/fef960c2-0714-41ad-96b3-229623b39f6b
```

```text
HTTP/1.1 204 No Content
vary: origin, access-control-request-method, access-control-request-headers
access-control-allow-origin: *
date: Tue, 02 Jun 2026 07:08:24 GMT

```

**8. Get the deleted task → `404 Not Found`** (typed error envelope)

```bash
curl -s -i http://127.0.0.1:3000/tasks/fef960c2-0714-41ad-96b3-229623b39f6b
```

```text
HTTP/1.1 404 Not Found
content-type: application/json
content-length: 49
...

{"error":{"code":404,"message":"task not found"}}
```

### Error paths (also real output)

**Validation failure → `422 Unprocessable Entity`**

```bash
curl -s -X POST http://127.0.0.1:3000/tasks \
  -H 'Content-Type: application/json' -d '{"title":"   "}'
```

```json
{"error":{"code":422,"message":"validation failed: title must not be empty"}}
```

**Malformed JSON → `400 Bad Request`** (handled by our `From<JsonRejection>`)

```bash
curl -s -X POST http://127.0.0.1:3000/tasks \
  -H 'Content-Type: application/json' -d '{"title": '
```

```json
{"error":{"code":400,"message":"invalid request body: Failed to parse the request body as JSON: title: EOF while parsing a value at line 1 column 10"}}
```

**Wrong field type (`title` is a number) → `400 Bad Request`**

```bash
curl -s -X POST http://127.0.0.1:3000/tasks \
  -H 'Content-Type: application/json' -d '{"title": 42}'
```

```json
{"error":{"code":400,"message":"invalid request body: Failed to deserialize the JSON body into the target type: title: invalid type: integer `42`, expected a string at line 1 column 12"}}
```

**Invalid UUID in the path → `400 Bad Request`** (rejected by the `Path<Uuid>`
extractor before the handler runs; this one is axum's built-in plain-text
rejection):

```bash
curl -s -i http://127.0.0.1:3000/tasks/not-a-uuid
```

```text
HTTP/1.1 400 Bad Request
content-type: text/plain; charset=utf-8
...

Invalid URL: Cannot parse `id` with value `not-a-uuid`: UUID parsing failed: invalid character: found `n` at 0
```

### Request logging

With `RUST_LOG=rest_api=debug,tower_http=debug`, the `TraceLayer` logs every
request and its status + latency. Real output from a few requests:

```text
2026-06-02T07:08:59.262577Z DEBUG request{method=GET uri=/health version=HTTP/1.1}: tower_http::trace::on_request: started processing request
2026-06-02T07:08:59.262665Z DEBUG request{method=GET uri=/health version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=0 ms status=200
2026-06-02T07:08:59.276842Z DEBUG request{method=GET uri=/tasks version=HTTP/1.1}: tower_http::trace::on_request: started processing request
2026-06-02T07:08:59.276927Z DEBUG request{method=GET uri=/tasks version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=0 ms status=200
2026-06-02T07:08:59.317456Z DEBUG request{method=POST uri=/tasks version=HTTP/1.1}: tower_http::trace::on_request: started processing request
2026-06-02T07:08:59.317617Z DEBUG request{method=POST uri=/tasks version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=0 ms status=201
2026-06-02T07:08:59.350451Z DEBUG request{method=GET uri=/tasks/nope version=HTTP/1.1}: tower_http::trace::on_request: started processing request
2026-06-02T07:08:59.350515Z DEBUG request{method=GET uri=/tasks/nope version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=0 ms status=400
```

### Tests

The project ships with in-process integration tests in `tests/api.rs` that
build the same `Router` and drive it without binding a socket (using
`tower::ServiceExt::oneshot`, the Rust equivalent of `supertest`):

```bash
cargo test
```

Real output:

```text
     Running tests/api.rs (target/debug/deps/api-8a4ce12d73e541f8)

running 7 tests
test empty_title_is_rejected ... ok
test create_then_get_roundtrip ... ok
test get_missing_returns_404 ... ok
test health_check_returns_ok ... ok
test malformed_json_is_bad_request ... ok
test list_is_empty_initially ... ok
test update_and_delete ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

One representative test, for flavor:

```rust
// tests/api.rs (excerpt)
#[tokio::test]
async fn get_missing_returns_404() {
    let id = uuid::Uuid::new_v4();
    let req = Request::builder()
        .uri(format!("/tasks/{id}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], 404);
}
```

## Key Concepts

This project cements a cluster of Rust ideas that recur in every web service:

- **Typed errors as a first-class concept.** `AppError` + `IntoResponse` is the
  pattern: enumerate every failure, map each to a status code with an
  exhaustive `match`, and let `?` propagate. The compiler guarantees you handle
  every variant. See [custom errors](/08-error-handling/04-custom-errors/) and
  [thiserror](/08-error-handling/06-anyhow-thiserror/).
- **Extractors decode the request for you.** `State`, `Path<Uuid>`, and
  `Json<T>` turn the raw HTTP request into validated, typed values *before*
  your handler runs. Invalid inputs never reach your logic. See
  [extractors](/16-web-apis/04-extractors/).
- **Shared state without data races.** `Arc<RwLock<HashMap<...>>>` is the
  canonical way to share mutable state across async tasks. `Arc` shares
  ownership cheaply; `RwLock` enforces safe access. The borrow checker makes
  data races a compile error, not a 2 a.m. page. See
  [reference counting](/05-ownership/07-reference-counting/) and the
  [Arc/Mutex pattern](/11-async/12-arc-mutex-pattern/).
- **`Option` is "maybe missing," made explicit.** Every lookup returns
  `Option<Task>`, and `.ok_or(AppError::NotFound)?` is the idiom for turning a
  missing value into a `404`. There is no `undefined` to forget about. See
  [the Option enum](/06-data-structures/03-option-enum/).
- **`async`/`await` on a real runtime.** Handlers are `async fn`, and
  `#[tokio::main]` starts the runtime. Unlike JavaScript Promises (which run
  eagerly), Rust futures are *lazy* and need a runtime to drive them. See
  [Section 11](/11-async/).
- **`serde` derive for free serialization.** One `#[derive(Serialize,
  Deserialize)]` and your struct round-trips to JSON. See
  [Section 15](/15-serialization/).
- **Middleware as composable layers.** `tower-http`'s `TraceLayer` and
  `CorsLayer` stack onto the router with `.layer(...)`, the same way `tower`
  composes any service. See [middleware](/16-web-apis/05-middleware/).

## Extending It

Concrete next steps to turn this into something you would actually deploy:

1. **Swap the in-memory store for Postgres.** Replace `TaskStore`'s
   `Arc<RwLock<HashMap<...>>>` with a `sqlx::PgPool`, make the store methods
   `async`, and turn the body of each into a SQL query
   (`sqlx::query_as!(...)`). The handler signatures barely change because they
   already `.await` nothing today; you would add `.await` to the store calls.
   See [Section 17 — sqlx intro](/17-database/00-sqlx-intro/) and
   [connection pooling](/17-database/08-connection-pooling/).
2. **Add pagination and filtering to `GET /tasks`.** Accept a
   `Query<ListParams>` extractor (`?completed=true&limit=20&offset=0`) and apply
   it in the store. This introduces axum's `Query` extractor.
3. **Add authentication.** A `tower` middleware that checks a `Bearer` token (or
   a JWT) and rejects with `401` before the handler runs. See
   [JWT](/16-web-apis/13-jwt/) and [authentication](/16-web-apis/12-authentication/).
4. **Richer validation with the `validator` crate.** Replace the hand-rolled
   `validate_*` functions with `#[derive(Validate)]` and field attributes,
   closer to NestJS `class-validator`. See [validation](/16-web-apis/09-validation/).

## Further Reading

Earlier sections this project builds on:

- [Section 11 — Async / Tokio](/11-async/) ·
  [Arc/Mutex pattern](/11-async/12-arc-mutex-pattern/) ·
  [Tokio setup](/11-async/03-tokio-setup/)
- [Section 16 — Web APIs](/16-web-apis/) ·
  [axum basics](/16-web-apis/01-axum-basics/) ·
  [routing](/16-web-apis/03-routing/) ·
  [extractors](/16-web-apis/04-extractors/) ·
  [JSON APIs](/16-web-apis/08-json-apis/) ·
  [error handling](/16-web-apis/10-error-handling-web/) ·
  [validation](/16-web-apis/09-validation/) ·
  [CORS](/16-web-apis/11-cors/)
- [Section 17 — Database](/17-database/) (for the Postgres swap)
- [Section 08 — Error handling](/08-error-handling/)
- [Section 15 — Serialization](/15-serialization/)
- [Section 13 — Testing](/13-testing/)

Other projects in this section:

- [Project 2: CLI Tool](/30-projects/01-cli-tool/) — a task/notes manager on the command line.
- Project 5: Production Microservice — a URL shortener with more production
  hardening (coming later in this section).
- [Project 6: Full-Stack App](/30-projects/05-full-stack/) — this kind of API paired with a
  WASM front-end.

Official docs:

- [axum documentation](https://docs.rs/axum/latest/axum/)
- [axum examples on GitHub](https://github.com/tokio-rs/axum/tree/main/examples)
- [tower-http documentation](https://docs.rs/tower-http/latest/tower_http/)
- [The Tokio tutorial](https://tokio.rs/tokio/tutorial)
