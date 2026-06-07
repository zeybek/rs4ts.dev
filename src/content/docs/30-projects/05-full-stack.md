---
title: "Project 6: Full-Stack App (Axum API + WASM frontend)"
description: "Build a full-stack notes app entirely in Rust: an axum JSON API plus a wasm-bindgen frontend, sharing serde types in one Cargo workspace, the"
---

This capstone builds a complete full-stack application **entirely in Rust**: an
[axum](https://github.com/tokio-rs/axum) JSON API on the backend and a
[wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/) + `web-sys` single-page
frontend that compiles to WebAssembly, fetches from that API, and updates the
DOM by hand. It is a small **notes** app (create a note, list notes, delete a
note), but it exercises the full shape of a real product: a typed HTTP API, an
in-memory data store shared across concurrent requests, JSON (de)serialization
on both ends, and a browser client written in the same language as the server.

If you come from Node, this is the Rust analogue of an **Express (or Fastify)
API plus a vanilla-JS / React SPA** living in one monorepo. The twist is that
both halves are Rust, and they even share the same `serde` data shapes
conceptually. The backend compiles to a native binary; the frontend compiles to
a `.wasm` module the browser loads as an ES module. We wire them together as a
single Cargo **workspace**, Cargo's answer to npm/pnpm workspaces.

> **Built and verified with:** Rust 1.96.0 (2024 edition), a local
> `rustc`/`cargo` 1.96.0 toolchain, `axum` 0.8.9, `tokio` 1.52, `tower-http`
> 0.6, `tracing` 0.1, `serde`/`serde_json` 1.0, `wasm-bindgen` 0.2.122,
> `wasm-bindgen-futures` 0.4, `js-sys`/`web-sys` 0.3, `gloo-net` 0.7, and
> `wasm-pack` 0.13.1. The `wasm32-unknown-unknown` target is installed via
> `rustup target add wasm32-unknown-unknown`. Every command, log line, and HTTP
> response shown below was produced by actually building and running the code in
> [`full-stack-code/`](https://github.com/zeybek/rs4ts.dev/tree/main/examples/full-stack-code).

## What You'll Build

A two-crate workspace serving one app:

| Piece | Crate | Compiles to | Role |
| --- | --- | --- | --- |
| Backend API | `backend` | native binary | `GET`/`POST`/`DELETE` JSON over HTTP; serves the static frontend too |
| Frontend SPA | `frontend` | `wasm32` `.wasm` | fetches the API and renders/mutates the DOM in the browser |

The JSON API has three endpoints:

| Method & path | Body | Response | Node/Express analogue |
| --- | --- | --- | --- |
| `GET /api/notes` | — | `200` + `Note[]` (newest first) | `app.get('/api/notes', ...)` |
| `POST /api/notes` | `{ "title", "body" }` | `201` + the created `Note`, or `400` `{ "error" }` | `app.post('/api/notes', ...)` |
| `DELETE /api/notes/{id}` | — | `204`, or `404` `{ "error" }` | `app.delete('/api/notes/:id', ...)` |

The same backend process also serves `index.html` and the compiled wasm bundle
at `/`, so in development you run **one** command and open one URL: no separate
static server, no CORS dance.

In the browser the page looks like this (described in words, since it is a live
DOM): a heading "Notes", a small form with a **Title** input, a **Body**
textarea, and an **Add note** button, then a status line, then a list of note
cards. Each card shows the title (`<h3>`), the body (`<p>`), a locale-formatted
timestamp (`<small>`), and a **Delete** button. Clicking **Add note** POSTs the
form and re-renders the list; clicking **Delete** removes that note and
re-renders. On first load the list already contains one seeded "Welcome" note
that came from the Rust backend.

A real request/response pair from the running server:

```bash
$ curl -s -X POST http://127.0.0.1:3000/api/notes \
    -H 'Content-Type: application/json' \
    -d '{"title":"Buy milk","body":"2 liters, oat"}'
{"id":2,"title":"Buy milk","body":"2 liters, oat","created_at":1780384172457}
```

## Prerequisites

This project ties together threads from across the guide. If any concept below
feels shaky, the linked section has the full treatment:

- [05 — Ownership](/05-ownership/): why the shared store is
  `Arc<Mutex<HashMap<..>>>` and not just a global variable.
- [08 — Error Handling](/08-error-handling/): `Result`, the `?`
  operator, and returning typed HTTP errors instead of throwing.
- [09 — Generics & Traits](/09-generics-traits/): axum extractors and
  `IntoResponse`; `JsCast` in the browser.
- [11 — Async](/11-async/): `async`/`await`, the Tokio runtime, and
  `spawn_local` for browser futures. Remember: Rust futures are **lazy** and
  need a runtime to drive them, unlike eager JavaScript Promises.
- [15 — Serialization](/15-serialization/): the `serde` derives that
  turn structs into JSON and back.
- [16 — Web APIs](/16-web-apis/): axum routing, handlers, state, and
  middleware in depth. This project is the "now build the whole thing" follow-up.
- [19 — WebAssembly](/19-wasm/): `wasm-bindgen`, `web-sys`, and
  `wasm-pack` fundamentals.

You should also have the wasm target installed:

```bash
rustup target add wasm32-unknown-unknown
```

and `wasm-pack` (`cargo install wasm-pack`, or via the official installer).

## Project Structure

The code lives in [`full-stack-code/`](https://github.com/zeybek/rs4ts.dev/tree/main/examples/full-stack-code) as a Cargo workspace:

```text
full-stack-code/
├── Cargo.toml              # workspace root: members + shared deps + release profile
├── build.sh                # convenience: build wasm into static/pkg, then build backend
├── .gitignore              # ignores /target and the regenerated /static/pkg
├── backend/
│   ├── Cargo.toml          # native crate: axum, tokio, tower-http, tracing, serde
│   └── src/
│       ├── main.rs         # entry point: boot Tokio, build router, serve on :3000
│       ├── models.rs       # Note / CreateNote / ApiError — the JSON shapes
│       ├── state.rs        # AppState: Arc<Mutex<HashMap>> in-memory store
│       ├── handlers.rs     # the three async request handlers
│       └── router.rs       # routes + static file serving + CORS + trace middleware
├── frontend/
│   ├── Cargo.toml          # wasm crate: cdylib, wasm-bindgen, web-sys, gloo-net
│   └── src/
│       ├── lib.rs          # #[wasm_bindgen(start)] entry; render + event wiring
│       ├── api.rs          # gloo-net fetch wrappers (the API client)
│       ├── dom.rs          # typed web-sys DOM helpers
│       └── models.rs       # client-side mirror of the backend JSON shapes
└── static/
    ├── index.html          # the page shell; loads the wasm ES module
    └── pkg/                # GENERATED by wasm-pack (frontend.js + frontend_bg.wasm)
```

> **Why two crates, not one?** The backend and frontend compile for **different
> targets**: your machine's native triple versus `wasm32-unknown-unknown`. A
> single crate can't be both. A workspace lets them share a `Cargo.lock`,
> dependency versions, and one `cargo` invocation, exactly like a pnpm workspace
> shares one lockfile across packages.

## Walkthrough

We'll build from the inside out: the workspace, then the data shapes, the store,
the handlers, the router, the binary, and finally the WASM client.

### Step 1 — The workspace root

The root `Cargo.toml` declares the two member crates and a couple of shared
dependency versions. It also sets a size-optimized release profile, which is
what we want for the wasm bundle.

```toml
# full-stack-code/Cargo.toml
# Workspace root: ties the backend (native) and frontend (wasm) crates together.
# Think of this as the monorepo `package.json` "workspaces" field, but for Cargo.
[workspace]
resolver = "3"
members = ["backend", "frontend"]

# Shared dependency versions live here so both crates stay in sync.
# Each crate opts in with `serde = { workspace = true }`, etc.
[workspace.dependencies]
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"

# A release profile tuned for small WASM output (used by the frontend crate).
[profile.release]
opt-level = "z"   # optimize for size, not speed (good default for wasm bundles)
lto = true        # link-time optimization trims dead code across crates
```

`resolver = "3"` is the dependency resolver that ships with the 2024 edition.
The `[workspace.dependencies]` table is the Cargo equivalent of pinning a
version once in a monorepo and having every package reference it; below you'll
see each crate write `serde = { workspace = true }`.

> **Node contrast:** in a pnpm/npm workspace you'd hoist `serde`'s analogue into
> the root `package.json` and let workspace protocol resolve it. Same idea, but
> Cargo also lets you set a shared **build profile** here; there's no npm
> equivalent of "optimize every package for size."

### Step 2 — The data shapes (`backend/src/models.rs`)

These three structs are the API's contract. `serde`'s derive macros generate the
JSON (de)serialization: no hand-written `JSON.parse`/`JSON.stringify`, and no
runtime validation library like `zod`, because the types *are* the schema.

```rust
// full-stack-code/backend/src/models.rs
//! Data shapes shared across the API, with `serde` derives so they
//! serialize to/from JSON. The TypeScript analogue would be a set of
//! `interface`s plus hand-written (or `zod`-validated) parsing.

use serde::{Deserialize, Serialize};

/// A single note. `Serialize` turns it into JSON for responses;
/// `Clone` lets us hand copies out of the in-memory store.
#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub id: u64,
    pub title: String,
    pub body: String,
    /// Unix-millis timestamp; the frontend formats it for display.
    pub created_at: u128,
}

/// The request body for creating a note. Only `Deserialize` —
/// clients send these fields; the server assigns `id`/`created_at`.
#[derive(Debug, Deserialize)]
pub struct CreateNote {
    pub title: String,
    pub body: String,
}

/// A uniform error envelope so the frontend can always read `error`.
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}
```

Notice the asymmetry that TypeScript can't express as cleanly: `Note` is
`Serialize` (it leaves the server) while `CreateNote` is `Deserialize` (it enters
the server). The compiler enforces that you never accidentally accept a
client-supplied `id` or `created_at`; those fields simply don't exist on the
input type. In Express you'd reach for a separate DTO/validation schema to get
the same guarantee.

### Step 3 — The in-memory store (`backend/src/state.rs`)

No database is required to compile or run this. The store is a `HashMap` behind
an `Arc<Mutex<..>>`. In Node you might write `const notes = new Map()` at module
scope and mutate it freely, because there's a single thread and event loop. Rust
makes the sharing **explicit**: `Arc` for shared ownership across async tasks,
`Mutex` for safe concurrent mutation, `AtomicU64` for a collision-free id
counter.

```rust
// full-stack-code/backend/src/state.rs
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

/// Current time as Unix milliseconds (like JS `Date.now()`).
fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before 1970")
        .as_millis()
}
```

A few things worth pausing on:

- `#[derive(Clone)]` on `AppState` is cheap: cloning an `Arc` just bumps a
  reference count, it does **not** copy the `HashMap`. Every request handler gets
  a clone of the *handles*, all pointing at the same store.
- `.lock()` returns a `Result` because the mutex could be *poisoned* (a thread
  panicked while holding it). We `.expect(..)` here for brevity; production code
  would handle it. There's no `await` while the guard is held, so we use the
  standard-library `Mutex`, not Tokio's async one.
- `fetch_add(1, Ordering::Relaxed)` is a lock-free atomic increment: the id
  source can't hand out duplicates even under concurrent `POST`s.

> **Swapping in a real database:** replace `AppState`'s internals with a
> connection pool (e.g. `sqlx::SqlitePool` or `PgPool`) and make these methods
> `async`. The handler signatures barely change. See
> [17 — Database](/17-database/) for the full pattern, including
> in-memory SQLite for tests.

### Step 4 — The handlers (`backend/src/handlers.rs`)

Each handler is an `async fn` whose **return type is the response**. axum's
*extractors* (`State`, `Json`, `Path`) pull typed data out of the request before
your code runs, the equivalent of Express middleware that parses `req.body`,
`req.params`, etc., except failures are handled by the framework and the types
are checked at compile time.

```rust
// full-stack-code/backend/src/handlers.rs
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
pub async fn delete_note(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
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
```

Two important framework details:

- **Automatic body validation.** Because `create_note` takes `Json<CreateNote>`,
  axum returns `422 Unprocessable Entity` on its own if the JSON is malformed or
  missing fields; that branch never reaches your function. Our explicit
  `400` is *additional* business-rule validation (non-empty title).
- **axum 0.8 path syntax.** The dynamic segment is `{id}`, not the `:id` used by
  older axum and by Express. `Path<u64>` even parses the string to a number for
  you; a non-numeric id yields a `400` automatically.

`IntoResponse` is the trait that lets us return different concrete types and have
them all turn into an HTTP `Response`. Tuples like `(StatusCode, Json<T>)`
implement it, which is why `(StatusCode::CREATED, Json(note)).into_response()`
just works.

### Step 5 — The router and static serving (`backend/src/router.rs`)

This is the `app.use(...)` / `app.get(...)` section, assembled as a value we can
build and (in a larger app) unit-test. It nests the API under `/api`, serves the
compiled frontend from `static/` as a fallback, and layers on CORS and request
tracing.

```rust
// full-stack-code/backend/src/router.rs
//! Wiring: maps URL patterns to handlers and attaches middleware.
//! This is the `app.use(...)` / `app.get(...)` section of an Express app,
//! but assembled as a value you can return and test.

use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::handlers::{create_note, delete_note, list_notes};
use crate::state::AppState;

/// Build the full application router.
///
/// `static_dir` is served at `/` so the same process can hand out the
/// compiled WASM frontend AND the JSON API — no separate static server
/// needed in dev. In production you'd usually put a CDN in front.
pub fn build_router(state: AppState, static_dir: &str) -> Router {
    // The JSON API, nested under /api.
    let api = Router::new()
        .route("/notes", get(list_notes).post(create_note))
        .route("/notes/{id}", axum::routing::delete(delete_note))
        .with_state(state);

    Router::new()
        .nest("/api", api)
        // Serve index.html + the wasm bundle from the static directory.
        .fallback_service(ServeDir::new(static_dir))
        // Permissive CORS so you can also run the frontend from a
        // different dev port if you prefer. Tighten this in production.
        .layer(CorsLayer::permissive())
        // Structured request logging, like `morgan` in Express.
        .layer(TraceLayer::new_for_http())
}
```

`get(list_notes).post(create_note)` attaches two methods to the same path,
exactly like chaining `.get().post()` in Express's `app.route()`. `ServeDir` is
a ready-made `tower` service that serves files from a directory; using it as the
`fallback_service` means any request that didn't match an API route (like `/` or
`/pkg/frontend_bg.wasm`) is answered from `static/`. The `.layer(..)` calls add
middleware that wraps every request, the way `tower` services compose.

> **Why serve the frontend from the API process?** It keeps the dev loop to a
> single command and avoids CORS entirely (the page and the API share an origin).
> In production you'd typically serve the static assets from a CDN and point the
> frontend at the API's URL; the permissive CORS layer is already here for that.

### Step 6 — The binary (`backend/src/main.rs`)

The entry point boots the Tokio runtime, sets up logging, builds the router, and
serves. `#[tokio::main]` is the macro that turns an `async fn main` into a normal
one that starts the runtime first. Rust has no built-in event loop, so you opt
into one. (Contrast Node, where the event loop always exists.)

```rust
// full-stack-code/backend/src/main.rs
//! Backend entry point. Boots Tokio, builds the router, and serves.
//!
//! Node analogue:
//! ```js
//! const app = express();
//! app.listen(3000, () => console.log("listening"));
//! ```

mod handlers;
mod models;
mod router;
mod state;

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::router::build_router;
use crate::state::AppState;

/// `#[tokio::main]` turns this async `main` into a sync one that starts
/// the runtime first — Rust has no built-in event loop, you opt into one.
#[tokio::main]
async fn main() {
    // Logging. `RUST_LOG=info` (the default below) controls verbosity.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .init();

    // Where the compiled frontend lives. Override with STATIC_DIR.
    // The default is resolved at build time from this crate's directory
    // (`CARGO_MANIFEST_DIR` = the `backend/` dir), so it is correct no
    // matter the current working directory `cargo run` was invoked from.
    let static_dir = std::env::var("STATIC_DIR")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../static").to_string());

    let state = AppState::new();
    let app = build_router(state, &static_dir);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(addr)
        .await
        .expect("failed to bind to port 3000");

    tracing::info!("backend listening on http://{addr}");
    tracing::info!("serving static files from {static_dir}");

    // axum 0.8: serve a listener + a router. No `app.listen` magic.
    axum::serve(listener, app)
        .await
        .expect("server error");
}
```

In axum 0.8 you bind a `TcpListener` yourself and hand it to `axum::serve(..)`.
There's no `app.listen(port, cb)`; the listener and the router are separate
values you compose. The `STATIC_DIR` default uses
`concat!(env!("CARGO_MANIFEST_DIR"), "/../static")`: `CARGO_MANIFEST_DIR` is set
by Cargo at **compile time** to this crate's directory (`backend/`), so the
default resolves to the project's `static/` as an absolute path baked into the
binary. That matters because `cargo run -p backend` runs the binary with the
working directory set to wherever you invoked `cargo` (the workspace root), **not**
the `backend/` package dir. A plain `"../static"` relative path would point one
level above the workspace and 404. The build-time absolute path sidesteps the cwd
question entirely, so the running binary always finds the wasm bundle wasm-pack
produced. (Override it at runtime with the `STATIC_DIR` env var if you relocate the
assets.)

### Step 7 — The backend `Cargo.toml`

```toml
# full-stack-code/backend/Cargo.toml
[package]
name = "backend"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8.9"
tokio = { version = "1.52.3", features = ["full"] }
tower-http = { version = "0.6.11", features = ["fs", "cors", "trace"] }
tracing = "0.1.41"
tracing-subscriber = { version = "0.3.20", features = ["env-filter"] }
serde = { workspace = true }
serde_json = { workspace = true }
```

The `tower-http` features map one-to-one to the middleware we used: `fs` for
`ServeDir`, `cors` for `CorsLayer`, `trace` for `TraceLayer`. `serde` is pulled
from the workspace so the version matches the frontend's.

### Step 8 — The WASM frontend

Now the browser half. It's a `cdylib` (a dynamic library the wasm toolchain
post-processes), and it depends on the wasm-specific crates.

```toml
# full-stack-code/frontend/Cargo.toml
[package]
name = "frontend"
version = "0.1.0"
edition = "2024"

# `cdylib` produces a `.wasm` artifact that wasm-bindgen post-processes
# into something the browser can load (like emitting an ES module).
[lib]
crate-type = ["cdylib"]

[dependencies]
wasm-bindgen = "0.2.122"
wasm-bindgen-futures = "0.4.72"
js-sys = "0.3.99"
gloo-net = { version = "0.7.0", features = ["json"] }
serde = { workspace = true }
serde_json = { workspace = true }
console_error_panic_hook = "0.1.7"

# web-sys is feature-gated: you enable exactly the Web APIs you touch,
# which keeps the generated bindings (and the wasm) small.
[dependencies.web-sys]
version = "0.3.99"
features = [
    "Document",
    "Element",
    "HtmlElement",
    "HtmlInputElement",
    "HtmlTextAreaElement",
    "Window",
    "Event",
    "MouseEvent",
]
```

`web-sys` is unusual: you enable the exact Web APIs you call as Cargo *features*.
Forget to list `HtmlInputElement` and the code won't compile. This is the
mechanism that keeps wasm bundles lean: you only pay for the bindings you use.

#### The data shapes, mirrored (`frontend/src/models.rs`)

Because the two crates target different platforms, they can't trivially share a
module, so the client keeps a small mirror of the JSON shapes. The field names
must match the backend's `serde` output exactly. That *is* the wire contract.

```rust
// full-stack-code/frontend/src/models.rs
//! Client-side mirror of the backend's JSON shapes.
//!
//! In a Node monorepo you'd share one `types.ts` between client and
//! server. Here the two crates compile for different targets (native vs
//! wasm), so we keep a small mirrored copy. The field names must match
//! the backend's `serde` output exactly — that's the contract.

use serde::{Deserialize, Serialize};

/// A note as returned by `GET /api/notes`.
#[derive(Debug, Clone, Deserialize)]
pub struct Note {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub created_at: u128,
}

/// The body we send to `POST /api/notes`.
#[derive(Debug, Serialize)]
pub struct CreateNote {
    pub title: String,
    pub body: String,
}
```

> **Sharing for real:** to avoid the mirror, you can extract these structs into a
> third `shared` crate that both `backend` and `frontend` depend on, as long as
> it only uses `#![no_std]`-friendly, target-agnostic deps (`serde` qualifies).
> The mirror is shown here to keep the workspace to two crates and the moving
> parts visible.

#### The API client (`frontend/src/api.rs`)

`gloo-net` wraps the browser `fetch` API in a Rusty `async`/`Result` interface,
the analogue of a small `apiClient.ts`. Every fallible step returns a `Result`
and `?` short-circuits on the first error. There's no `try/catch`: the type
system forces the caller to handle failure.

```rust
// full-stack-code/frontend/src/api.rs
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
    let notes = Request::get(&format!("{API_BASE}/notes"))
        .send()
        .await?
        .json::<Vec<Note>>()
        .await?;
    Ok(notes)
}

/// `POST /api/notes` with a JSON body -> the created `Note`.
pub async fn create_note(input: &CreateNote) -> Result<Note, gloo_net::Error> {
    let note = Request::post(&format!("{API_BASE}/notes"))
        .json(input)?            // serialize body + set Content-Type
        .send()
        .await?
        .json::<Note>()
        .await?;
    Ok(note)
}

/// `DELETE /api/notes/{id}` -> `Ok(())` on success.
pub async fn delete_note(id: u64) -> Result<(), gloo_net::Error> {
    Request::delete(&format!("{API_BASE}/notes/{id}"))
        .send()
        .await?;
    Ok(())
}
```

Compare `fetch_notes` to its JavaScript twin:

```typescript
// the rough Node/browser equivalent
async function fetchNotes(): Promise<Note[]> {
  const res = await fetch("/api/notes");
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();         // unchecked: TS trusts you it's Note[]
}
```

In TypeScript `res.json()` returns `any`/`Promise<any>` and the `Note[]`
annotation is an unchecked assertion. In Rust, `.json::<Vec<Note>>()` actually
*parses into* that type with `serde`; a shape mismatch is a real, handled error,
not a runtime surprise three components later.

#### Typed DOM helpers (`frontend/src/dom.rs`)

`web-sys` gives you the DOM, but every lookup returns an `Option` (the element
might not exist) and every "is this element actually an `<input>`?" cast returns
a `Result` via `JsCast`. These helpers wrap the ceremony.

```rust
// full-stack-code/frontend/src/dom.rs
//! DOM helpers built on `web-sys`. These are the typed Rust equivalents
//! of `document.getElementById`, `el.textContent = ...`, and
//! `el.addEventListener(...)` — the difference is that every call that
//! could fail returns a `Result`/`Option` you must handle.

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{Document, Element, HtmlInputElement, HtmlTextAreaElement, window};

/// Grab `document`, panicking with a clear message if we're somehow not
/// in a browser. In a real app you'd surface this more gracefully.
pub fn document() -> Document {
    window()
        .expect("no global window (are we in a browser?)")
        .document()
        .expect("window has no document")
}

/// `document.getElementById`, but returns a typed `Element`.
pub fn get_by_id(doc: &Document, id: &str) -> Element {
    doc.get_element_by_id(id)
        .unwrap_or_else(|| panic!("missing #{id} in the DOM"))
}

/// Read the trimmed value out of an `<input>` by id.
pub fn input_value(doc: &Document, id: &str) -> String {
    get_by_id(doc, id)
        .dyn_into::<HtmlInputElement>()
        .expect("element is not an <input>")
        .value()
        .trim()
        .to_string()
}

/// Read the trimmed value out of a `<textarea>` by id.
pub fn textarea_value(doc: &Document, id: &str) -> String {
    get_by_id(doc, id)
        .dyn_into::<HtmlTextAreaElement>()
        .expect("element is not a <textarea>")
        .value()
        .trim()
        .to_string()
}

/// Clear the value of an `<input>` by id (after a successful submit).
pub fn clear_input(doc: &Document, id: &str) {
    if let Some(el) = doc.get_element_by_id(id) {
        if let Ok(input) = el.dyn_into::<HtmlInputElement>() {
            input.set_value("");
        }
    }
}

/// Clear the value of a `<textarea>` by id.
pub fn clear_textarea(doc: &Document, id: &str) {
    if let Some(el) = doc.get_element_by_id(id) {
        if let Ok(area) = el.dyn_into::<HtmlTextAreaElement>() {
            area.set_value("");
        }
    }
}

/// Attach a click listener. We `forget()` the closure so it lives for the
/// lifetime of the page — the wasm equivalent of not dropping a JS
/// callback you still need. (For dynamic UIs you'd store and reuse these.)
pub fn on_click<F: 'static + FnMut()>(element: &Element, mut handler: F) {
    let closure = Closure::<dyn FnMut()>::new(move || handler());
    element
        .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
        .expect("failed to attach click listener");
    closure.forget();
}
```

The interesting one is `on_click`. A Rust closure can't be handed to the DOM
directly; `Closure::new` boxes it into something with a stable address that JS
can call. `closure.forget()` deliberately leaks it so it outlives the function;
otherwise Rust would drop it at the end of the scope and the listener would fire
into freed memory. This is the wasm version of the JavaScript rule "keep a
reference to a callback you still need." For a long-lived SPA with churning
listeners you'd store the `Closure`s instead of forgetting them, but for
page-lifetime handlers `forget` is the idiomatic shortcut.

#### The app entry point (`frontend/src/lib.rs`)

This ties it together. `#[wasm_bindgen(start)]` marks `run()` as the function the
runtime calls automatically on module load: no manual init from JavaScript. It
wires the **Add note** button and kicks off the first fetch. Because browser
futures can't block, we drive them with `spawn_local` (the wasm analogue of
firing off an async function without `await`ing it).

```rust
// full-stack-code/frontend/src/lib.rs
//! WASM frontend entry point.
//!
//! Compiles to a `.wasm` module that the browser loads. `run()` runs once
//! on startup (like a top-level `<script type="module">`): it wires up the
//! "Add note" button, then kicks off an async load of the note list.
//!
//! The whole thing is the Rust analogue of a tiny vanilla-JS SPA: fetch
//! JSON, build DOM nodes, re-render on change.

mod api;
mod dom;
mod models;

use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::Document;

use crate::models::{CreateNote, Note};

/// `#[wasm_bindgen(start)]` marks the function the runtime calls
/// automatically once the module is instantiated — no manual init from JS.
#[wasm_bindgen(start)]
pub fn run() {
    // Route Rust panics to the browser console with a readable stack,
    // instead of an opaque "unreachable" trap.
    console_error_panic_hook::set_once();

    let doc = dom::document();

    // Wire up the "Add note" button. The closure captures `doc` (cheaply
    // cloneable — it's a handle) and spawns an async task on click.
    let add_button = dom::get_by_id(&doc, "add-btn");
    let doc_for_click = doc.clone();
    dom::on_click(&add_button, move || {
        let doc = doc_for_click.clone();
        spawn_local(async move {
            submit_new_note(&doc).await;
        });
    });

    // Initial load: fetch existing notes and render them.
    spawn_local(async move {
        refresh_notes(&doc).await;
    });
}

/// Read the form, POST a new note, clear the form, then re-render.
async fn submit_new_note(doc: &Document) {
    let title = dom::input_value(doc, "title-input");
    let body = dom::textarea_value(doc, "body-input");

    if title.is_empty() {
        set_status(doc, "Title is required.");
        return;
    }

    set_status(doc, "Saving...");
    let payload = CreateNote { title, body };

    match api::create_note(&payload).await {
        Ok(_) => {
            dom::clear_input(doc, "title-input");
            dom::clear_textarea(doc, "body-input");
            set_status(doc, "");
            refresh_notes(doc).await;
        }
        Err(err) => set_status(doc, &format!("Failed to save: {err}")),
    }
}

/// Fetch all notes from the API and rebuild the list in the DOM.
async fn refresh_notes(doc: &Document) {
    match api::fetch_notes().await {
        Ok(notes) => render_notes(doc, &notes),
        Err(err) => set_status(doc, &format!("Failed to load notes: {err}")),
    }
}

/// Replace the contents of `#notes` with a freshly built list.
fn render_notes(doc: &Document, notes: &[Note]) {
    let list = dom::get_by_id(doc, "notes");
    list.set_inner_html(""); // clear previous render

    if notes.is_empty() {
        let empty = doc
            .create_element("p")
            .expect("create <p> failed");
        empty.set_text_content(Some("No notes yet. Add one above."));
        list.append_child(&empty).expect("append failed");
        return;
    }

    for note in notes {
        list.append_child(&note_element(doc, note))
            .expect("append note failed");
    }
}

/// Build a single note `<li>` with a title, body, and delete button.
fn note_element(doc: &Document, note: &Note) -> web_sys::Element {
    let item = doc.create_element("li").expect("create <li> failed");
    item.set_class_name("note");

    let title = doc.create_element("h3").expect("create <h3> failed");
    title.set_text_content(Some(&note.title));
    item.append_child(&title).expect("append title failed");

    if !note.body.is_empty() {
        let body = doc.create_element("p").expect("create <p> failed");
        body.set_text_content(Some(&note.body));
        item.append_child(&body).expect("append body failed");
    }

    // Render the server-assigned timestamp via the browser's own
    // `Date` (js-sys), so we use the `created_at` field the API returns.
    let meta = doc.create_element("small").expect("create <small> failed");
    let when = js_sys::Date::new(&JsValue::from_f64(note.created_at as f64));
    let formatted: String = when
        .to_locale_string("en-US", &JsValue::UNDEFINED)
        .into();
    meta.set_text_content(Some(&formatted));
    item.append_child(&meta).expect("append meta failed");

    let del = doc.create_element("button").expect("create button failed");
    del.set_text_content(Some("Delete"));
    del.set_class_name("delete");

    // `Rc` lets the click closure and the surrounding code share the same
    // document handle without fighting the borrow checker.
    let doc_rc = Rc::new(doc.clone());
    let id = note.id;
    dom::on_click(&del, move || {
        let doc = doc_rc.clone();
        spawn_local(async move {
            if api::delete_note(id).await.is_ok() {
                refresh_notes(&doc).await;
            } else {
                set_status(&doc, "Failed to delete note.");
            }
        });
    });
    item.append_child(&del).expect("append delete failed");

    item
}

/// Write a short message into the `#status` line.
fn set_status(doc: &Document, message: &str) {
    if let Some(el) = doc.get_element_by_id("status") {
        el.set_text_content(Some(message));
    }
}
```

The rendering is deliberately old-school: clear `#notes`, rebuild it from the
latest data. That's the "re-render the world" model React popularized, done by
hand. Note the use of `set_text_content` (not `set_inner_html`) for
user-supplied strings. That's the safe path that escapes content, the wasm
equivalent of React's default text interpolation rather than
`dangerouslySetInnerHTML`.

`Rc` (reference-counted, single-threaded) appears in `note_element` because the
delete closure and the surrounding loop both need the `Document` handle. In the
browser everything runs on one thread, so `Rc` is the right tool; its
thread-safe cousin `Arc` would be overkill here. See
[10 — Smart Pointers](/10-smart-pointers/) for the `Rc` vs `Arc`
distinction.

#### The page shell (`static/index.html`)

Plain HTML with a sprinkle of CSS. The only dynamic part is the module script at
the bottom: it imports the wasm-pack-generated ES module and calls its default
export to initialize. `wasm-pack`'s `--target web` output is a self-contained
module: no bundler required.

```html
<!-- full-stack-code/static/index.html -->
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Notes — Rust Full-Stack</title>
    <style>
      :root { color-scheme: light dark; }
      body {
        font-family: system-ui, sans-serif;
        max-width: 40rem;
        margin: 2rem auto;
        padding: 0 1rem;
        line-height: 1.5;
      }
      h1 { margin-bottom: 0.25rem; }
      form { display: grid; gap: 0.5rem; margin: 1rem 0; }
      input, textarea, button { font: inherit; padding: 0.5rem; }
      ul { list-style: none; padding: 0; display: grid; gap: 0.75rem; }
      .note {
        border: 1px solid currentColor;
        border-radius: 0.5rem;
        padding: 0.75rem 1rem;
      }
      .note h3 { margin: 0 0 0.25rem; }
      .note p { margin: 0 0 0.5rem; }
      .delete { cursor: pointer; }
      #status { min-height: 1.25rem; color: crimson; }
    </style>
  </head>
  <body>
    <h1>Notes</h1>
    <p>Backend: Rust + axum. Frontend: Rust compiled to WebAssembly.</p>

    <form id="note-form" onsubmit="return false">
      <input id="title-input" type="text" placeholder="Title" />
      <textarea id="body-input" rows="3" placeholder="Body (optional)"></textarea>
      <button id="add-btn" type="submit">Add note</button>
    </form>

    <p id="status"></p>
    <ul id="notes"></ul>

    <!-- wasm-pack (--target web) emits an ES module + the .wasm file in
         pkg/. We import it and call the default export to initialize. -->
    <script type="module">
      import init from "./pkg/frontend.js";
      init();
    </script>
  </body>
</html>
```

## Running It

### The dev workflow, step by step

The workflow is: **build the wasm bundle into `static/pkg`, then build and run
the backend, which serves both the bundle and the API.** A `build.sh` is included
that does the first two steps:

```bash
# full-stack-code/build.sh
#!/usr/bin/env bash
# Build the WASM frontend into static/pkg, then build the backend.
# Run from anywhere; paths are resolved relative to this script.
set -euo pipefail

cd "$(dirname "$0")"

echo "==> Building WASM frontend (wasm-pack, --target web)"
( cd frontend && wasm-pack build --target web --out-dir ../static/pkg --no-typescript )

echo "==> Building native backend"
cargo build -p backend

echo "==> Done. Start the server with:"
echo "    cargo run -p backend"
echo "    # then open http://127.0.0.1:3000"
```

#### 1. Add the wasm target (once)

```bash
rustup target add wasm32-unknown-unknown
```

#### 2. Build the frontend to WebAssembly

From the workspace root:

```bash
cd frontend
wasm-pack build --target web --out-dir ../static/pkg --no-typescript
```

Real (tail of the) output from this directory:

```text
   Compiling frontend v0.1.0 (.../examples/full-stack-code/frontend)
    Finished `release` profile [optimized] target(s) in 32.35s
[INFO]: found wasm-opt at "/opt/homebrew/bin/wasm-opt"
[INFO]: Optimizing wasm binaries with `wasm-opt`...
[INFO]:   Done in 33.39s
[INFO]:   Your wasm pkg is ready to publish at .../full-stack-code/static/pkg.
```

That produces `static/pkg/frontend.js` (the ES-module glue) and
`static/pkg/frontend_bg.wasm` (the actual module):

```bash
$ ls static/pkg
frontend.js  frontend_bg.wasm  package.json  .gitignore
$ du -h static/pkg/frontend_bg.wasm
100K	static/pkg/frontend_bg.wasm
```

A 100 KB wasm module, size-optimized by the `opt-level = "z"` + `lto` profile in
the workspace root and then by `wasm-opt`.

If you only want to **type-check** the frontend without a full build (faster, and
exactly what CI might run), use:

```bash
cargo check -p frontend --target wasm32-unknown-unknown
```

which finishes with:

```text
    Checking frontend v0.1.0 (.../examples/full-stack-code/frontend)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.53s
```

#### 3. Run the backend (which serves everything)

```bash
cargo run -p backend
```

Real startup log:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.03s
     Running `target/debug/backend`
2026-06-02T07:17:59.348977Z  INFO backend: backend listening on http://127.0.0.1:3000
2026-06-02T07:17:59.349043Z  INFO backend: serving static files from .../full-stack-code/backend/../static
```

Now open <http://127.0.0.1:3000> in a browser. The wasm module loads, calls
`run()`, fetches `GET /api/notes`, and renders the seeded "Welcome" note. Type a
title and body, click **Add note**, and the list updates; click **Delete** on a
card and it disappears.

> **Re-building after a frontend change:** re-run the `wasm-pack build` command
> (or `./build.sh`). The backend serves `static/pkg` fresh on each request, so a
> browser refresh picks up the new bundle; no backend restart needed. Backend
> changes do need a `cargo run` restart (or use `cargo watch -x 'run -p
> backend'`).

### Exercising the API directly with curl

With the server running, here is a **real** end-to-end session against the live
API (responses are verbatim):

```bash
$ curl -s http://127.0.0.1:3000/api/notes
[{"id":1,"title":"Welcome","body":"This note came from the Rust backend.","created_at":1780384166393}]

$ curl -s -X POST http://127.0.0.1:3000/api/notes \
    -H 'Content-Type: application/json' \
    -d '{"title":"Buy milk","body":"2 liters, oat"}'
{"id":2,"title":"Buy milk","body":"2 liters, oat","created_at":1780384172457}

$ curl -s http://127.0.0.1:3000/api/notes
[{"id":2,"title":"Buy milk","body":"2 liters, oat","created_at":1780384172457},{"id":1,"title":"Welcome","body":"This note came from the Rust backend.","created_at":1780384166393}]
```

Validation and the error paths, with status codes:

```bash
$ curl -s -w "\n[HTTP %{http_code}]\n" -X POST http://127.0.0.1:3000/api/notes \
    -H 'Content-Type: application/json' -d '{"title":"  ","body":"x"}'
{"error":"title must not be empty"}
[HTTP 400]

$ curl -s -o /dev/null -w "[HTTP %{http_code}]\n" \
    -X DELETE http://127.0.0.1:3000/api/notes/2
[HTTP 204]

$ curl -s -w "\n[HTTP %{http_code}]\n" -X DELETE http://127.0.0.1:3000/api/notes/999
{"error":"no note with id 999"}
[HTTP 404]
```

And confirming the same process serves the frontend bundle with the correct MIME
types:

```bash
$ curl -s -o /dev/null -w "[HTTP %{http_code}] content-type=%{content_type}\n" \
    http://127.0.0.1:3000/
[HTTP 200] content-type=text/html

$ curl -s -o /dev/null \
    -w "[HTTP %{http_code}] content-type=%{content_type} size=%{size_download}\n" \
    http://127.0.0.1:3000/pkg/frontend_bg.wasm
[HTTP 200] content-type=application/wasm size=99691
```

With `RUST_LOG="info,tower_http=debug"` set, the `TraceLayer` logs each request.
Real output captured from a run:

```text
DEBUG request{method=GET uri=/api/notes version=HTTP/1.1}: tower_http::trace::on_request: started processing request
DEBUG request{method=GET uri=/api/notes version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=1 ms status=200
DEBUG request{method=POST uri=/api/notes version=HTTP/1.1}: tower_http::trace::on_request: started processing request
DEBUG request{method=POST uri=/api/notes version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=0 ms status=201
```

## Key Concepts

This project cements the ideas that make Rust full-stack work:

- **One language, two targets.** A Cargo workspace builds a native binary and a
  `wasm32` module side by side, sharing dependency versions and a lockfile. The
  data shapes that cross the wire are `serde` structs on both ends. See
  [12 — Modules & Packages](/12-modules-packages/) for workspaces and
  [15 — Serialization](/15-serialization/) for `serde`.
- **Explicit shared state.** `Arc<Mutex<HashMap<..>>>` is how you share mutable
  data across concurrent async tasks safely; `AtomicU64` gives lock-free id
  generation. The borrow checker makes the sharing visible instead of implicit.
  See [05 — Ownership](/05-ownership/) and
  [10 — Smart Pointers](/10-smart-pointers/).
- **Types as the API contract.** `Note` is `Serialize`-only, `CreateNote` is
  `Deserialize`-only, and axum's `Json<T>`/`Path<T>` extractors validate input
  before your handler runs. Errors are `Result` values and HTTP status codes,
  not exceptions. See [08 — Error Handling](/08-error-handling/) and
  [16 — Web APIs](/16-web-apis/).
- **Async on two runtimes.** The backend runs on **Tokio** (`#[tokio::main]`,
  `axum::serve`); the browser uses `wasm-bindgen-futures::spawn_local` to drive
  futures on the JS event loop. Both rely on the fact that Rust futures are lazy
  and need a runtime. See [11 — Async](/11-async/).
- **WASM ↔ DOM interop.** `web-sys` exposes typed DOM bindings gated by Cargo
  features; `JsCast`/`dyn_into` perform checked casts; `Closure` + `forget`
  bridge Rust closures into JS event listeners; `gloo-net` wraps `fetch`. See
  [19 — WebAssembly](/19-wasm/).

## Extending It

Concrete next steps if you want to push this further:

1. **Swap the in-memory store for a real database.** Replace `AppState`'s
   `HashMap` with a `sqlx::SqlitePool` (in-memory `sqlite::memory:` for tests, a
   file or Postgres in production) and make the `list`/`create`/`delete` methods
   `async`. The handler signatures barely move. Follow
   [17 — Database](/17-database/).
2. **Add update + edit.** Introduce `PUT /api/notes/{id}` with an `UpdateNote`
   body and an "Edit" button on each card. This exercises a fourth method and a
   second `Deserialize`-only DTO.
3. **Add authentication.** Put a `tower` middleware layer in front of `/api` that
   checks a bearer token or a session cookie, returning `401` on failure, the
   same `IntoResponse` pattern as the `400`/`404` paths here. See
   [28 — Production](/28-production/) for hardening.
4. **Replace the hand-rolled DOM with a framework.** Rewrite the frontend in
   [Leptos](https://leptos.dev/) or [Yew](https://yew.rs/) for reactive,
   component-based rendering with signals instead of "clear and rebuild." The API
   client and `serde` shapes carry over unchanged.

## Further Reading

- [16 — Web APIs](/16-web-apis/) — axum routing, extractors, state,
  and middleware in depth.
- [17 — Database](/17-database/) — connection pools and `sqlx` for a
  real persistence layer.
- [19 — WebAssembly](/19-wasm/) — `wasm-bindgen`, `web-sys`, and
  `wasm-pack` fundamentals.
- [11 — Async](/11-async/) — futures, Tokio, and `spawn_local`.
- [15 — Serialization](/15-serialization/) — `serde` in depth.
- Other projects in this section:
  [REST API](/30-projects/00-rest-api/), [CLI Tool](/30-projects/01-cli-tool/),
  [WASM App](/30-projects/02-wasm-app/), [WebSocket Chat](/30-projects/03-websocket-chat/),
  [Microservice](/30-projects/04-microservice/).
- Official docs: [axum](https://docs.rs/axum/latest/axum/),
  [tower-http](https://docs.rs/tower-http/latest/tower_http/),
  [wasm-bindgen guide](https://rustwasm.github.io/wasm-bindgen/),
  [web-sys](https://docs.rs/web-sys/latest/web_sys/),
  [gloo-net](https://docs.rs/gloo-net/latest/gloo_net/).
