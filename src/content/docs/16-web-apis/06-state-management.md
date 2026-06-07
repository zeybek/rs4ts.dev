---
title: "Shared Application State in Axum"
description: "Where Express threads dependencies through module variables, Axum bundles them in one Clone-able State struct. Share with Arc, mutate with Mutex, split with FromRef."
---

## Quick Overview

Almost every real web service needs **shared state**: a database connection pool, parsed configuration, an outbound HTTP client, a cache, a metrics handle. In Express you usually reach for module-level variables or `app.locals`/`req.app.get(...)`; in Axum you put everything in one `State<T>` value that the framework hands to each handler. This page covers the idiomatic pattern: a `Clone`-able state struct, `Arc` for the bits that need sharing, when to use request **extensions** instead, and how to inject a pool or config without globals.

> **Note:** This page uses **axum 0.8** (current stable is 0.8.9, on the latest stable edition, 2024). State is attached with `Router::with_state(...)` and read with the `State<T>` extractor. There is no `Server::bind()` builder and path parameters use `{id}`, not `:id`. The `#[derive(FromRef)]` macro shown later needs `cargo add axum --features macros`.

---

## TypeScript/JavaScript Example

A typical Express service threads dependencies through closures or module scope. Here is a small users API that holds a config object, a database handle, and a request counter:

```typescript
// server.ts — Express 5
import express, { Request, Response } from "express";

// Loaded once at startup.
interface Config {
  serviceName: string;
  maxItems: number;
}

// Stand-in for a real connection pool (e.g. a `pg.Pool`).
class Db {
  private notes: { id: number; text: string }[] = [];
  list() {
    return this.notes;
  }
  create(text: string) {
    const note = { id: this.notes.length + 1, text };
    this.notes.push(note);
    return note;
  }
}

// Shared, mutable across all requests — just a closure variable.
const config: Config = { serviceName: "notes-api", maxItems: 100 };
const db = new Db();
let requestCount = 0;

const app = express();
app.use(express.json());

app.get("/info", (_req: Request, res: Response) => {
  requestCount += 1;
  res.json({ app: config.serviceName, maxItems: config.maxItems, requestsServed: requestCount });
});

app.get("/notes", (_req: Request, res: Response) => {
  res.json(db.list());
});

app.post("/notes", (req: Request, res: Response) => {
  if (db.list().length >= config.maxItems) {
    res.status(409).json({ error: "too many notes" });
    return;
  }
  const note = db.create(req.body.text as string);
  res.status(201).json(note);
});

app.listen(3000, () => console.log("listening on http://127.0.0.1:3000"));
```

This works because JavaScript is single-threaded: `requestCount += 1` and `db.create(...)` can never run concurrently, so nobody worries about data races. The dependencies are just variables in scope. There is also no type-level guarantee that a handler has access to what it needs: if you forget to define `db`, you find out at runtime.

---

## Rust Equivalent

In Axum you bundle the dependencies into one struct, attach it with `.with_state(...)`, and each handler asks for it via the `State` extractor. The struct must be `Clone`; anything that should be **shared** (rather than copied per request) goes behind an `Arc`.

```rust
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

// Immutable configuration loaded once at startup.
#[derive(Clone)]
struct Config {
    app_name: String,
    max_items: usize,
}

// A stand-in for a real database connection pool. In a real app this is
// `sqlx::PgPool` or similar — already internally an Arc, so cloning is cheap.
#[derive(Clone, Default)]
struct Db {
    notes: Arc<Mutex<Vec<Note>>>,
}

#[derive(Clone, Serialize)]
struct Note {
    id: u64,
    text: String,
}

#[derive(Deserialize)]
struct NewNote {
    text: String,
}

// The whole application state. One struct holds every dependency a handler
// might need. It derives `Clone` so the router can hand each request its own
// (cheap) clone — every field is itself cheap to clone.
#[derive(Clone)]
struct AppState {
    config: Config,
    db: Db,
    request_count: Arc<AtomicU64>,
}

// GET /info — read-only config access, plus a lock-free counter bump.
async fn info(State(state): State<AppState>) -> Json<serde_json::Value> {
    let n = state.request_count.fetch_add(1, Ordering::Relaxed) + 1;
    Json(serde_json::json!({
        "app": state.config.app_name,
        "max_items": state.config.max_items,
        "requests_served": n,
    }))
}

// GET /notes — read the shared list behind the mutex.
async fn list_notes(State(state): State<AppState>) -> Json<Vec<Note>> {
    let notes = state.db.notes.lock().unwrap();
    Json(notes.clone())
}

// POST /notes — mutate shared state, honoring a config limit.
async fn create_note(
    State(state): State<AppState>,
    Json(body): Json<NewNote>,
) -> Result<impl IntoResponse, StatusCode> {
    let mut notes = state.db.notes.lock().unwrap();
    if notes.len() >= state.config.max_items {
        return Err(StatusCode::CONFLICT);
    }
    let note = Note { id: notes.len() as u64 + 1, text: body.text };
    notes.push(note.clone());
    Ok((StatusCode::CREATED, Json(note)))
}

// Router builder: attach state once with `.with_state(...)`.
fn app(state: AppState) -> Router {
    Router::new()
        .route("/info", get(info))
        .route("/notes", get(list_notes).post(create_note))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState {
        config: Config { app_name: "notes-api".to_string(), max_items: 100 },
        db: Db::default(),
        request_count: Arc::new(AtomicU64::new(0)),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app(state)).await.unwrap();
}
```

Dependencies (run in a fresh `cargo new` project; `cargo add` resolves current versions):

```bash
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json
```

```toml
[dependencies]
axum = "0.8.9"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
tokio = { version = "1.52.3", features = ["full"] }
```

Running it and hitting the endpoints produces this **real** output (captured against the compiled server):

```text
$ curl -s http://127.0.0.1:3000/info
{"app":"notes-api","max_items":100,"requests_served":1}

$ curl -s http://127.0.0.1:3000/info
{"app":"notes-api","max_items":100,"requests_served":2}

$ curl -s -i -X POST http://127.0.0.1:3000/notes \
       -H 'content-type: application/json' -d '{"text":"buy milk"}'
HTTP/1.1 201 Created
content-type: application/json
content-length: 26
date: Mon, 01 Jun 2026 11:49:16 GMT

{"id":1,"text":"buy milk"}

$ curl -s http://127.0.0.1:3000/notes
[{"id":1,"text":"buy milk"}]
```

---

## Detailed Explanation

### One struct, attached once, extracted per handler

```rust
#[derive(Clone)]
struct AppState {
    config: Config,
    db: Db,
    request_count: Arc<AtomicU64>,
}
```

Where Express uses free-floating closure variables, Axum collects them into a single `struct`. You attach it to the router **once** with `.with_state(state)`, and every handler that needs it declares a `State<AppState>` parameter. This is dependency injection done through the type system: a handler's signature lists exactly what it depends on, and the program will not compile if you wire the state up wrong (more on that in Common Pitfalls).

### Why `State<T>` requires `Clone`

Axum gives each incoming request access to the state by **cloning** it. That sounds expensive until you see *what* is being cloned:

- `Config` is a couple of fields: cloning it copies a `String` and a `usize`. Fine, but you could also wrap it in `Arc` to avoid even that.
- `Db { notes: Arc<Mutex<Vec<Note>>> }` clones to a *new `Arc` pointer to the same `Vec`*. Cloning an `Arc` just increments an atomic reference count; it does **not** copy the `Vec`. Every handler shares the same underlying data.
- `Arc<AtomicU64>` clones the same way: all clones point at the one counter.

So `AppState: Clone` is cheap by construction: the *sharable* parts are `Arc`s, and cloning an `Arc` is a pointer-plus-refcount-bump. This is the central idea — **`Arc` is how you say "shared," and `Clone` on the state is how Axum distributes that shared handle.**

> **Note:** `Arc` stands for *atomically reference-counted*. It is the multi-threaded sibling of `Rc`. Axum's default scheduler can run handlers on different OS threads, so shared state must be `Arc`, not `Rc`. See [Reference Counting with Rc and Arc](/10-smart-pointers/01-rc-arc/) and [The Arc + Mutex Pattern](/11-async/12-arc-mutex-pattern/).

### Shared *mutable* state needs a lock

```rust
let mut notes = state.db.notes.lock().unwrap();
notes.push(note.clone());
```

In JavaScript, `notes.push(...)` is safe by accident: the single-threaded event loop guarantees no two handlers touch `notes` at the same time. Rust makes no such promise: handlers can run on multiple threads concurrently, so the compiler **refuses** to let you mutate shared data without a synchronization primitive. `Arc<Mutex<Vec<Note>>>` is the answer: `Arc` for shared ownership, `Mutex` so only one handler mutates at a time. `.lock()` returns a `Result` (it errors only if a previous holder panicked while holding the lock — "poisoning"); `.unwrap()` is the pragmatic default for a guard you expect to always be healthy.

> **Tip:** For a counter you do not need a full `Mutex`. `Arc<AtomicU64>` with `fetch_add(1, Ordering::Relaxed)` is lock-free and faster, exactly what `request_count` uses here.

### Read-only config needs no lock

`Config` is never mutated after startup, so `state.config.app_name` reads it directly with no lock. This is the common case: **most state is immutable config plus already-synchronized handles (a pool, a client), so very little of it actually needs a `Mutex`.** Reach for a lock only around data you genuinely mutate at request time.

### `std::sync::Mutex` vs `tokio::sync::Mutex`

The `Mutex` above is `std::sync::Mutex`. That is the right choice when you lock, do a quick in-memory operation, and unlock **without `.await`ing in between**, which is exactly what `create_note` does. If you need to hold a lock *across* an `.await` (for example, awaiting I/O while the guard is alive), use `tokio::sync::Mutex` (or `RwLock`) instead, because a blocking std guard held across an await can stall the async runtime. The Real-World Example below uses `tokio::sync::RwLock` for precisely that reason. See [Synchronization Primitives in Async Rust](/11-async/11-sync-primitives/).

---

## Key Differences

| Concern | Express.js | Axum (0.8) |
| --- | --- | --- |
| Where deps live | closure/module variables, `app.locals` | one `State<T>` struct attached with `.with_state` |
| Access in handler | refer to the variable in scope | declare a `State<T>` parameter |
| Sharing model | implicit (single thread) | explicit `Arc` for shared ownership |
| Mutating shared data | just mutate it | `Mutex`/`RwLock`/atomics — enforced by the compiler |
| Missing a dependency | runtime `undefined` | **compile error** for `State`; runtime 500 for extensions |
| Per-request data | `req.locals` / monkey-patch `req` | request **extensions** (`Extension<T>`) |
| Cost of "sharing" | free (one runtime) | `Arc::clone` = one atomic refcount bump |

The conceptual shift: **Express lets you share state implicitly because there is one thread; Rust forces you to name the sharing (`Arc`) and the mutation discipline (`Mutex`/atomics) because handlers may run in parallel.** What feels like ceremony is the compiler eliminating an entire class of data-race bugs at build time.

### State vs Extensions

Axum has two ways to make a value available to handlers, and choosing correctly avoids a lot of confusion:

| | `State<T>` | `Extension<T>` |
| --- | --- | --- |
| Attached with | `Router::with_state(value)` | `.layer(Extension(value))` |
| Keyed by | the router's state type | the value's Rust type |
| Missing value | **compile-time** error | **runtime** 500 error |
| Type-checked | yes, statically | no, looked up at runtime |
| Best for | your app's main state struct | values injected by middleware, or interop |

**Prefer `State<T>` for your application state.** It is checked at compile time. Use `Extension<T>` when a value is produced by a middleware layer (for example, an authenticated user attached by an auth layer; see [Authentication](/16-web-apis/12-authentication/)), or when a library expects the extension mechanism. The cost of `Extension` is that forgetting to add the layer is only caught when a request hits the handler.

---

## Common Pitfalls

### 1. Forgetting `.with_state(...)` (a confusing compile error)

If a handler asks for `State<AppState>` but you never call `.with_state(...)`, the router's state type stays the default `()`, and the types no longer line up. This is a **compile-time** error, which is good, but the message points at the `route` call, not the missing `with_state`:

```rust
use axum::{extract::State, routing::get, Router};

#[derive(Clone)]
struct AppState {
    name: String,
}

async fn handler(State(state): State<AppState>) -> String {
    state.name
}

#[tokio::main]
async fn main() {
    // does not compile (error[E0308]): forgot `.with_state(...)`,
    // so the router's state type is `()`, not `AppState`.
    let app: Router = Router::new().route("/", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

The real error from `cargo check`:

```text
error[E0308]: mismatched types
   --> examples/err_no_state.rs:16:48
    |
 16 |     let app: Router = Router::new().route("/", get(handler));
    |                                     -----      ^^^^^^^^^^^^ expected `MethodRouter`, found `MethodRouter<AppState>`
    |                                     |
    |                                     arguments to this method are incorrect
    |
    = note: expected struct `MethodRouter<()>`
               found struct `MethodRouter<AppState>`
```

The fix is to add `.with_state(AppState { ... })`. The phrase "found `MethodRouter<AppState>`" is the tell: a handler in this router wants `AppState`, but the router has no state of that type yet.

### 2. Forgetting the `Extension` layer (a runtime 500)

With extensions there is no compile-time check. If a handler reads `Extension<Thing>` but no `.layer(Extension(thing))` was added, the request fails at runtime with a `500`:

```text
status = 500 Internal Server Error
body = Missing request extension: Extension of type `app::Thing` was not found. Perhaps you forgot to add it? See `axum::Extension`.
```

That message (captured by driving the router with `tower`'s `oneshot`) is the runtime cost of extensions. It is the strongest argument for using `State<T>` for your core dependencies and reserving `Extension` for middleware-injected values.

### 3. Holding a `std::sync::Mutex` guard across `.await`

This compiles, but it is a real hazard, and Clippy flags it. A `std::sync::MutexGuard` held while the task is suspended at an `.await` can block the runtime and risks deadlocks:

```rust
use std::sync::{Arc, Mutex};

struct Store {
    items: Arc<Mutex<Vec<String>>>,
}

async fn do_async_thing() {}

async fn handler(store: Store) {
    let mut items = store.items.lock().unwrap();
    // guard is still alive across this await — Clippy warns.
    do_async_thing().await;
    items.push("x".to_string());
}

fn main() {}
```

`cargo clippy` produces this **real** warning:

```text
warning: this `MutexGuard` is held across an await point
  --> examples/clippy_await_lock.rs:10:9
   |
10 |     let mut items = store.items.lock().unwrap();
   |         ^^^^^^^^^
   |
   = help: consider using an async-aware `Mutex` type or ensuring the `MutexGuard` is dropped before calling `await`
note: these are all the await points this lock is held through
  --> examples/clippy_await_lock.rs:12:22
   |
12 |     do_async_thing().await;
   |                      ^^^^^
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#await_holding_lock
   = note: `#[warn(clippy::await_holding_lock)]` on by default
```

Fix it by either dropping the guard before the `.await` (do the locked work in a tight scope, then await), or switching to `tokio::sync::Mutex`/`RwLock` whose guards are designed to be held across awaits.

### 4. Trying to share `Rc` instead of `Arc`

`Rc<T>` is single-threaded reference counting; it is **not** `Send`, so it cannot cross threads. Putting an `Rc` in your state will fail to satisfy the `Handler` bounds (state must be `Send + Sync + Clone + 'static`). Use `Arc` for anything shared across handlers. See [Rc vs Arc](/10-smart-pointers/01-rc-arc/).

### 5. Wrapping the whole state in `Arc` *and* deriving `Clone` redundantly

You have two valid styles and should pick one:

- A `#[derive(Clone)] struct AppState { ... }` whose *fields* are `Arc`s (the main example), or
- A plain `struct AppState { ... }` used as `State<Arc<AppState>>`, where the **outer** `Arc` is what gets cloned (no `Clone` derive on the struct needed).

Doing both — `Arc<AppState>` where `AppState` also has `Arc` fields and a `Clone` derive — is harmless but pointless double-wrapping. Choose field-level `Arc` when different handlers want different *parts* of the state (so you can use `FromRef`, below); choose the outer `Arc<AppState>` when handlers always want the whole thing and the state has private fields you would rather not make individually `Clone`.

---

## Best Practices

- **One `AppState` struct, attached once.** Keep router construction in a `fn app(state: AppState) -> Router` so tests can build a router with a fake state and drive it via `tower::ServiceExt::oneshot` (no real port needed).
- **`Arc` only what is shared; lock only what mutates.** Config and connection pools are `Arc` (or already internally shared). A `Mutex`/`RwLock` belongs only around data you actually mutate at request time.
- **Pools are already `Arc` inside.** `sqlx::PgPool`, `reqwest::Client`, and most ecosystem clients are designed to be cloned cheaply (clone = bump a refcount). Put them directly in `AppState` and `#[derive(Clone)]`; do not wrap them in another `Arc` or a `Mutex`.
- **Prefer `State<T>` over `Extension<T>`** for first-party dependencies, so wiring mistakes are compile errors. Reserve `Extension` for middleware-produced, per-request values.
- **Use `tokio::sync::RwLock` for read-mostly state held across `.await`,** `std::sync::Mutex` for quick non-async critical sections, and atomics for counters/flags.
- **Split big state with `#[derive(FromRef)]`** so a handler can extract just the substate it needs (`State<Config>`, `State<PgPool>`) instead of the whole struct. It documents each handler's true dependencies.
- **Globals (`OnceLock`/`LazyLock`) are a last resort.** For truly process-wide, set-once values you can use a `static` with `OnceLock`, but explicit `State` is more testable and clearer about dependencies.

### Splitting state with `FromRef`

When state grows, forcing every handler to extract the entire blob is noisy. The `FromRef` derive lets handlers ask for a *substate*:

```rust
use axum::{
    extract::{FromRef, State},
    routing::get,
    Json, Router,
};
use std::sync::Arc;

#[derive(Clone)]
struct Config {
    app_name: String,
}

struct Pool {
    url: String,
}

// `#[derive(FromRef)]` generates the conversions that let a handler ask for
// just `State<Config>` or `State<Arc<Pool>>` instead of the whole `AppState`.
#[derive(Clone, FromRef)]
struct AppState {
    config: Config,
    pool: Arc<Pool>,
}

// This handler only needs the config — so it asks for exactly that substate.
async fn info(State(config): State<Config>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "app": config.app_name }))
}

// This one only needs the pool.
async fn db_status(State(pool): State<Arc<Pool>>) -> String {
    format!("connected to {}", pool.url)
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/info", get(info))
        .route("/db", get(db_status))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState {
        config: Config { app_name: "notes-api".to_string() },
        pool: Arc::new(Pool { url: "postgres://localhost/notes".to_string() }),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app(state)).await.unwrap();
}
```

`FromRef` requires the `macros` feature: `cargo add axum --features macros`. Each field becomes individually extractable as long as it is `Clone` (note `Arc<Pool>`, so the non-`Clone` `Pool` is still shareable).

### The global alternative (`OnceLock`)

For a value that is set once and read everywhere — and that you do not need to fake in tests — a `static` initialized lazily works without any `State` plumbing:

```rust
use std::sync::OnceLock;

#[derive(Debug)]
struct Config {
    name: String,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

fn config() -> &'static Config {
    CONFIG.get_or_init(|| Config { name: "notes-api".to_string() })
}

fn main() {
    println!("{}", config().name);
    println!("{}", config().name); // same instance; initialized exactly once
}
```

Running this prints `notes-api` twice; the closure runs only on the first call. This is occasionally handy for constants, but for anything a handler genuinely *depends on* (and that you want to swap in tests), prefer `State`.

---

## Real-World Example

A users API with three real concerns: configuration loaded from the environment, a repository standing in for a database, and a read-mostly cache behind a `tokio::sync::RwLock` (so concurrent reads do not block each other, and the guard can safely be held across `.await`). The state struct bundles all three and is cheap to clone.

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;

// Configuration, loaded once from the environment at startup.
#[derive(Clone)]
struct Config {
    cache_ttl: Duration,
    service_name: String,
}

impl Config {
    fn from_env() -> Self {
        let ttl_secs = std::env::var("CACHE_TTL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        Config {
            cache_ttl: Duration::from_secs(ttl_secs),
            service_name: std::env::var("SERVICE_NAME")
                .unwrap_or_else(|_| "users-api".to_string()),
        }
    }
}

#[derive(Clone, Serialize)]
struct User {
    id: u64,
    name: String,
}

// A fake repository standing in for a real DB pool. Internally Arc-shared,
// so cloning the repo is just a refcount bump — like cloning a `sqlx::PgPool`.
#[derive(Clone)]
struct UserRepo {
    users: Arc<Vec<User>>,
}

impl UserRepo {
    fn seed() -> Self {
        UserRepo {
            users: Arc::new(vec![
                User { id: 1, name: "Ada".to_string() },
                User { id: 2, name: "Linus".to_string() },
            ]),
        }
    }

    fn find(&self, id: u64) -> Option<User> {
        self.users.iter().find(|u| u.id == id).cloned()
    }
}

// A read-mostly cache guarded by an async RwLock so reads don't block reads.
type Cache = Arc<RwLock<HashMap<u64, User>>>;

#[derive(Clone)]
struct AppState {
    config: Config,
    repo: UserRepo,
    cache: Cache,
}

// GET /healthz — surfaces config without any locking.
async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": state.config.service_name,
        "cache_ttl_secs": state.config.cache_ttl.as_secs(),
    }))
}

// GET /users/{id} — cache read (shared lock), fall back to the repo on a miss.
async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<User>, StatusCode> {
    // Fast path: a shared read lock lets many requests read at once.
    if let Some(user) = state.cache.read().await.get(&id).cloned() {
        return Ok(Json(user));
    }
    // Miss: look it up, then take a write lock to populate the cache.
    let user = state.repo.find(id).ok_or(StatusCode::NOT_FOUND)?;
    state.cache.write().await.insert(id, user.clone());
    Ok(Json(user))
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/users/{id}", get(get_user))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState {
        config: Config::from_env(),
        repo: UserRepo::seed(),
        cache: Arc::new(RwLock::new(HashMap::new())),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app(state)).await.unwrap();
}
```

Running it and exercising the cache produces this **real** output:

```text
$ curl -s http://127.0.0.1:3000/healthz
{"cache_ttl_secs":60,"service":"users-api"}

$ curl -s http://127.0.0.1:3000/users/1      # cache miss → repo lookup
{"id":1,"name":"Ada"}

$ curl -s http://127.0.0.1:3000/users/1      # cache hit
{"id":1,"name":"Ada"}

$ curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:3000/users/99
404
```

Production-relevant notes on this design:

- **Config comes from `Config::from_env()`, not globals.** It is a plain field of `AppState`, so a test can build the state with any config it likes. Real services parse this with a crate such as `figment` or `config`, or just `std::env::var` as here. See [Deployment](/16-web-apis/19-deployment/) for env-based configuration.
- **`UserRepo` models a pool: `Arc` inside, `Clone` cheap.** Swap `Arc<Vec<User>>` for a `sqlx::PgPool` and the shape is identical: clone the pool freely; the connections are pooled internally. See [Database Integration](/17-database/).
- **`tokio::sync::RwLock` is deliberate.** The cache is read on every request and written rarely, and the guard is alive across `.await` points (`.read().await`, `.write().await`). An async `RwLock` is correct here; a `std::sync::Mutex` held across those awaits would draw the Clippy warning from Pitfall 3 and risk stalling the runtime.

---

## Further Reading

- [Axum `State` extractor (docs.rs)](https://docs.rs/axum/latest/axum/extract/struct.State.html). The canonical reference and examples.
- [`FromRef` trait and derive (docs.rs)](https://docs.rs/axum/latest/axum/extract/trait.FromRef.html) — substate extraction.
- [`axum::Extension` (docs.rs)](https://docs.rs/axum/latest/axum/struct.Extension.html). The runtime-keyed alternative to `State`.
- [Tokio `RwLock` (docs.rs)](https://docs.rs/tokio/latest/tokio/sync/struct.RwLock.html) and [`Mutex`](https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html).
- Within this guide:
  - [Axum Fundamentals](/16-web-apis/01-axum-basics/) — the `Router`/handler/`axum::serve` loop this builds on.
  - [Extractors](/16-web-apis/04-extractors/): how `State` fits the extractor system; ordering rules.
  - [Middleware](/16-web-apis/05-middleware/) — Tower layers, including where `Extension` values come from.
  - [Authentication](/16-web-apis/12-authentication/): a real use of middleware-injected `Extension<AuthUser>`.
  - [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/) — turning state/lookup errors into clean responses.
  - [Database Integration](/17-database/) — putting a `sqlx`/`diesel` pool in your state.
  - Background: [Rc vs Arc](/10-smart-pointers/01-rc-arc/), [The Arc + Mutex Pattern](/11-async/12-arc-mutex-pattern/), [Synchronization Primitives](/11-async/11-sync-primitives/), [Structs](/06-data-structures/00-structs/).

---

## Exercises

### Exercise 1: Split state with `FromRef`

**Difficulty:** Beginner

**Objective:** Practice the substate pattern so handlers depend only on what they use.

**Instructions:** Build an `AppState` with three fields — a `Config { app_name: String }`, a `Features { dark_mode: bool }`, and a pretend pool (`Arc<String>`). Derive `Clone` and `FromRef` (remember `cargo add axum --features macros`). Write `GET /name` that extracts only `State<Config>` and returns the app name, and `GET /flags` that extracts only `State<Features>` and returns the flags as JSON. Wire both into a router with a single `.with_state(...)`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::{FromRef, State},
    routing::get,
    Json, Router,
};
use std::sync::Arc;

#[derive(Clone)]
struct Config {
    app_name: String,
}

#[derive(Clone)]
struct Features {
    dark_mode: bool,
}

#[derive(Clone, FromRef)]
struct AppState {
    config: Config,
    features: Features,
    pool: Arc<String>, // pretend DB pool
}

async fn name(State(config): State<Config>) -> String {
    config.app_name
}

async fn flags(State(features): State<Features>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "dark_mode": features.dark_mode }))
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/name", get(name))
        .route("/flags", get(flags))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState {
        config: Config { app_name: "shop".to_string() },
        features: Features { dark_mode: true },
        pool: Arc::new("postgres://...".to_string()),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app(state)).await.unwrap();
}
```

`GET /name` returns `shop`; `GET /flags` returns `{"dark_mode":true}`. Each handler extracts only its slice of the state.

</details>

### Exercise 2: Hot-reloadable settings behind `RwLock`

**Difficulty:** Intermediate

**Objective:** Mutate shared state safely across concurrent requests using an async `RwLock`.

**Instructions:** Hold a `Settings { rate_limit: u32 }` (deriving `Serialize`/`Deserialize` and `Clone`) inside `Arc<RwLock<Settings>>` in `AppState`. Add `GET /settings` (read lock → return current settings as JSON) and `PUT /settings` (write lock → replace settings with the JSON body and return the new value). Use `tokio::sync::RwLock`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::State,
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    rate_limit: u32,
}

#[derive(Clone)]
struct AppState {
    settings: Arc<RwLock<Settings>>,
}

async fn get_settings(State(state): State<AppState>) -> Json<Settings> {
    Json(state.settings.read().await.clone())
}

async fn put_settings(
    State(state): State<AppState>,
    Json(new): Json<Settings>,
) -> Json<Settings> {
    let mut guard = state.settings.write().await;
    *guard = new;
    Json(guard.clone())
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/settings", get(get_settings))
        .route("/settings", put(put_settings))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState {
        settings: Arc::new(RwLock::new(Settings { rate_limit: 100 })),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app(state)).await.unwrap();
}
```

Verified behavior: `GET /settings` returns `{"rate_limit":100}`; after `PUT /settings` with `{"rate_limit":500}`, a subsequent `GET /settings` returns `{"rate_limit":500}`. The two `.route("/settings", ...)` calls can also be merged into `.route("/settings", get(get_settings).put(put_settings))`.

</details>

### Exercise 3: Inject a client and a counter via `Arc<AppState>`

**Difficulty:** Advanced

**Objective:** Use the outer-`Arc` style for state with private, non-`Clone`-derived fields, combining an injected client with a lock-free atomic counter.

**Instructions:** Define a non-`Clone`-deriving `AppState` holding an `ApiClient { base_url: String }` (with an `async fn ping(&self) -> String`) and an `AtomicU64` named `calls`. Use it as `State<Arc<AppState>>`. Write `GET /proxy` that increments the counter with `fetch_add`, calls `client.ping().await`, and returns a string combining the ping result and the call number.

<details>
<summary>Solution</summary>

```rust
use axum::{extract::State, routing::get, Router};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

// In a real app this is `reqwest::Client` (itself cheaply cloneable).
#[derive(Clone)]
struct ApiClient {
    base_url: String,
}

impl ApiClient {
    async fn ping(&self) -> String {
        format!("pinged {}", self.base_url)
    }
}

// No `#[derive(Clone)]` — the outer `Arc` is what gets cloned per request.
struct AppState {
    client: ApiClient,
    calls: AtomicU64,
}

async fn proxy(State(state): State<Arc<AppState>>) -> String {
    let n = state.calls.fetch_add(1, Ordering::Relaxed) + 1;
    let result = state.client.ping().await;
    format!("{result} (call #{n})")
}

fn app(state: Arc<AppState>) -> Router {
    Router::new().route("/proxy", get(proxy)).with_state(state)
}

#[tokio::main]
async fn main() {
    let state = Arc::new(AppState {
        client: ApiClient { base_url: "https://api.example.com".to_string() },
        calls: AtomicU64::new(0),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app(state)).await.unwrap();
}
```

Each `GET /proxy` returns `pinged https://api.example.com (call #N)` with `N` incrementing. The `AtomicU64` needs no `Mutex` (`fetch_add` is lock-free), and because the whole state lives behind one `Arc`, `AppState` itself does not need to be `Clone`.

</details>
