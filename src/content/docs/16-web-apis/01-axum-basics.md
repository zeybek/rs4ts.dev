---
title: "Axum Fundamentals: From Express to Axum"
description: "Axum's Router, async handlers, and axum::serve mapped from Express: where Express mutates req and res, Rust handlers take typed extractors and return responses."
---

## Quick Overview

[Axum](https://docs.rs/axum) is the most popular Rust web framework, built on top of the [Tokio](/11-async/02-tokio-intro/) async runtime and the Tower middleware ecosystem. If you know Express.js, the mental model transfers almost directly: you build a **router**, attach **handlers** (Express's "route handlers" or "controllers") to method-plus-path combinations, and start a server. This page covers the core loop (`Router`, async handler functions, and starting the server with `axum::serve` and a `tokio::net::TcpListener`) so you can build everything else in this section on top of it.

> **Note:** This page targets the **axum 0.8** API line (recorded with 0.8.9). Two things changed from older tutorials you may find online: the server is started with `axum::serve(listener, app)` (the old `Server::bind().serve()` builder is gone), and path parameters use `{id}` syntax (the old `:id` colon syntax was removed). If a snippet uses `:id` or `Server::bind`, it targets an older Axum API.

---

## TypeScript/JavaScript Example

Here is a minimal-but-realistic Express server: a health check, a JSON endpoint that reads a path parameter, and a `POST` that parses a JSON body and replies with `201 Created`.

```typescript
// server.ts — Express 4/5
import express, { Request, Response } from "express";

const app = express();
app.use(express.json()); // body-parsing middleware for JSON

interface Task {
  id: number;
  title: string;
  done: boolean;
}

// GET / — a plain text response
app.get("/", (_req: Request, res: Response) => {
  res.send("Hello from Express!");
});

// GET /health — a JSON response
app.get("/health", (_req: Request, res: Response) => {
  res.json({ status: "ok" });
});

// GET /tasks/:id — read a path parameter
app.get("/tasks/:id", (req: Request, res: Response) => {
  const id = Number(req.params.id);
  const task: Task = { id, title: "Write the docs", done: false };
  res.json(task);
});

// POST /tasks — parse a JSON body, reply 201
app.post("/tasks", (req: Request, res: Response) => {
  const { title } = req.body as { title: string };
  const task: Task = { id: 42, title, done: false };
  res.status(201).json(task);
});

app.listen(3000, () => {
  console.log("listening on http://127.0.0.1:3000");
});
```

Things a TypeScript developer relies on here: handlers receive `(req, res)`, you pull inputs off `req` (`req.params`, `req.body`, `req.query`), and you push output through `res` (`res.send`, `res.json`, `res.status`). The runtime (Node's event loop) is always running; `app.listen` just registers the server with it.

---

## Rust Equivalent

The same server in Axum. Read it top to bottom. It maps almost one-to-one to the Express version, but the differences are where the learning is.

```rust
use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Deserialize)]
struct CreateTask {
    title: String,
}

#[derive(Serialize)]
struct Task {
    id: u64,
    title: String,
    done: bool,
}

// GET / — a plain text response. `&'static str` is a valid response body.
async fn root() -> &'static str {
    "Hello from Axum!"
}

// GET /health — `Json<T>` serializes T and sets Content-Type: application/json.
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

// GET /tasks/{id} — the `Path` extractor pulls and parses the path parameter.
async fn get_task(Path(id): Path<u64>) -> Json<Task> {
    Json(Task { id, title: "Write the docs".to_string(), done: false })
}

// POST /tasks — the `Json` extractor parses the body; the tuple sets the status.
async fn create_task(Json(payload): Json<CreateTask>) -> impl IntoResponse {
    let task = Task { id: 42, title: payload.title, done: false };
    (StatusCode::CREATED, Json(task))
}

#[tokio::main]
async fn main() {
    // 1. Build the router: map (method, path) -> handler.
    let app: Router = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task));

    // 2. Bind a TCP listener — tokio's async listener, not std's.
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());

    // 3. Hand the listener and the router to axum::serve and await it forever.
    axum::serve(listener, app).await.unwrap();
}
```

The dependencies (run these in a fresh `cargo new` project; `cargo add` resolves the current versions automatically):

```bash
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json
```

This produces a `Cargo.toml` with the current stable versions:

```toml
[dependencies]
axum = "0.8.9"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
tokio = { version = "1.52.3", features = ["full"] }
```

Run it with `cargo run`. Hitting the endpoints with `curl` produces this **real** output (captured against the compiled server):

```text
$ curl -s http://127.0.0.1:3000/
Hello from Axum!

$ curl -s http://127.0.0.1:3000/health
{"status":"ok"}

$ curl -s http://127.0.0.1:3000/tasks/7
{"id":7,"title":"Write the docs","done":false}

$ curl -s -i -X POST http://127.0.0.1:3000/tasks \
       -H 'content-type: application/json' -d '{"title":"Ship it"}'
HTTP/1.1 201 Created
content-type: application/json
content-length: 40
date: Mon, 01 Jun 2026 11:43:49 GMT

{"id":42,"title":"Ship it","done":false}
```

---

## Detailed Explanation

Let's walk through each piece and contrast it with Express.

### The handler is just an `async fn`

```rust
async fn root() -> &'static str {
    "Hello from Axum!"
}
```

In Express, a handler is `(req, res) => { ... }` and you produce output by *calling methods on `res`*. In Axum, a handler is an `async fn` (or async closure) and you produce output by **returning a value**. There is no `res` object to mutate; whatever you `return` becomes the response. This is closer to a pure function: input in the signature, response in the return type.

> **Note:** Rust futures are **lazy**: an `async fn` does nothing until it is `.await`ed (or driven by a runtime). This is the opposite of JavaScript Promises, which start executing the moment they are created. Axum's runtime drives your handler futures for you when a request arrives. See [Promises vs Futures](/11-async/00-promises-vs-futures/).

### Inputs come from extractors, not a `req` object

```rust
async fn get_task(Path(id): Path<u64>) -> Json<Task> { /* ... */ }
async fn create_task(Json(payload): Json<CreateTask>) -> impl IntoResponse { /* ... */ }
```

Where Express reads `req.params.id` and `req.body`, Axum declares what it needs **in the function parameters**. `Path<u64>` says "pull the path parameter and parse it into a `u64`". `Json<CreateTask>` says "read the request body and deserialize it into a `CreateTask`". These are called **extractors**, and they are typed: if the path parameter is not a valid `u64`, or the body is not valid JSON for the target type, Axum returns an error response *before your handler ever runs*. Express, by contrast, hands you whatever string or `any` is there and trusts you to validate it.

The `Path(id)` and `Json(payload)` syntax is **pattern destructuring** of a tuple struct: the same `let Point(x, y) = p;` pattern you would use anywhere in Rust. `Path` wraps the extracted value; the pattern unwraps it into a local binding.

> Extractors are a deep topic with their own ordering rules. This page only uses them; [Extractors](/16-web-apis/04-extractors/) covers `FromRequest`/`FromRequestParts` and how to write your own.

### Outputs are return values implementing `IntoResponse`

```rust
async fn create_task(Json(payload): Json<CreateTask>) -> impl IntoResponse {
    let task = Task { id: 42, title: payload.title, done: false };
    (StatusCode::CREATED, Json(task))
}
```

Anything a handler returns must implement the `IntoResponse` trait, which knows how to turn the value into an HTTP response. Axum implements it for a huge set of types out of the box:

- `&'static str` / `String` → `200 OK`, `text/plain`
- `Json<T>` (for any `Serialize` `T`) → `200 OK`, `application/json`
- `StatusCode` → an empty response with that status
- A tuple like `(StatusCode, Json<T>)` → that status **plus** that JSON body
- `Result<T, E>` where both `T` and `E` implement `IntoResponse` → success or error response

`-> impl IntoResponse` is the idiomatic return type when you want to return *different* concrete response shapes from one handler without naming them all. It's the Rust equivalent of TypeScript's "this returns a `Response`, don't worry about the exact union."

> Status codes, headers, and the full `IntoResponse` story live in [Request and Response Handling](/16-web-apis/07-request-response/).

### `Router`: the Express `app`

```rust
let app: Router = Router::new()
    .route("/", get(root))
    .route("/health", get(health))
    .route("/tasks", post(create_task))
    .route("/tasks/{id}", get(get_task));
```

`Router::new()` is your `express()`. Instead of `app.get(path, handler)`, you call `.route(path, method_router)`, where the **method router** (`get(...)`, `post(...)`, `put(...)`, `delete(...)`) wraps the handler and declares which HTTP method it answers. Each `.route()` returns the router by value, so you chain them (a *builder pattern*). The path parameter syntax is `{id}`, matching the modern Express 5 / URL Pattern style, **not** the old `:id`.

You can attach several methods to the same path by chaining on the method router:

```rust
// One path, two methods — like app.route('/tasks').get(...).post(...)
.route("/tasks", get(list_tasks).post(create_task))
```

### Starting the server: `axum::serve` + `TcpListener`

```rust
let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
axum::serve(listener, app).await.unwrap();
```

This is the part most outdated tutorials get wrong. In axum 0.8 you:

1. Create a **`tokio::net::TcpListener`** (the async listener) and `bind` it — note the `.await`, because binding is itself an async operation.
2. Pass the listener and the router to **`axum::serve`**.
3. `.await` the resulting future. It runs until the process is killed (or until a graceful-shutdown signal you wire in).

There is no `app.listen(3000, callback)`. The `axum::serve(...).await` *is* the running server, and the line after it never executes under normal operation.

### `#[tokio::main]`: there is no built-in event loop

```rust
#[tokio::main]
async fn main() {
    // ...
}
```

In Node, the event loop exists before your code runs. In Rust, **`async` is just syntax and nothing drives it until you start a runtime.** The `#[tokio::main]` attribute macro rewrites your `async fn main` into a normal `fn main` that boots a Tokio runtime and blocks on your async body. Without a runtime, `axum::serve(...).await` would have nothing to poll it. See [Tokio Setup](/11-async/03-tokio-setup/) for the details and the manual `Runtime::new()` alternative.

---

## Key Differences

| Concept | Express.js | Axum (0.8) |
| --- | --- | --- |
| App object | `const app = express()` | `Router::new()` |
| Register route | `app.get("/x", handler)` | `.route("/x", get(handler))` |
| Path parameter | `:id` → `req.params.id` (string) | `{id}` → `Path<u64>` (parsed, typed) |
| Read JSON body | `express.json()` + `req.body` (`any`) | `Json<T>` extractor (typed, validated) |
| Query string | `req.query` | `Query<T>` extractor |
| Produce a response | call `res.json()` / `res.send()` | **return** a value implementing `IntoResponse` |
| Set status | `res.status(201).json(...)` | return `(StatusCode::CREATED, Json(...))` |
| Async | optional; callbacks or `async` | every handler is `async`; needs a runtime |
| Start server | `app.listen(3000)` | `axum::serve(TcpListener, app).await` |
| Runtime | always-on event loop | you start Tokio (`#[tokio::main]`) |
| Input validation | manual / middleware | extractors fail with a 4xx before the handler |

The deepest conceptual shift: **Express hands you raw, untyped request data and a mutable response object; Axum makes you declare typed inputs and return typed outputs.** A handler signature like `async fn get_task(Path(id): Path<u64>) -> Json<Task>` documents and enforces its contract at compile time. The framework rejects malformed requests for you, which in Express you would write by hand.

> **Tip:** A handler in Axum is any `async fn` (or async closure) whose parameters are all extractors and whose return type implements `IntoResponse`. That single sentence defines the entire handler contract.

---

## Common Pitfalls

### 1. Returning a type that isn't a response

Every handler's return type must implement `IntoResponse`. Returning a bare struct that doesn't will fail, but with a *confusing* error, because the missing trait shows up as the whole function failing the `Handler` bound:

```rust
use axum::{routing::get, Router};

struct Task {
    id: u64,
}

// does not compile: Task does not implement IntoResponse
async fn get_task() -> Task {
    Task { id: 1 }
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/task", get(get_task));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

The real error from `cargo check` points at the `get(get_task)` call site, not the handler:

```text
error[E0277]: the trait bound `fn() -> impl Future<Output = Task> {get_task}: Handler<_, _>` is not satisfied
   --> src/main.rs:14:48
    |
 14 |     let app = Router::new().route("/task", get(get_task));
    |                                            --- ^^^^^^^^ the trait `Handler<_, _>` is not implemented for fn item `fn() -> impl Future<Output = Task> {get_task}`
    |
    = note: Consider using `#[axum::debug_handler]` to improve the error message
```

As the note says, annotate the handler with `#[axum::debug_handler]` (enable the `macros` feature: `cargo add axum --features macros`). The error then points at the *actual* problem:

```text
error[E0277]: the trait bound `Task: IntoResponse` is not satisfied
 --> src/main.rs:8:24
  |
8 | async fn get_task() -> Task {
  |                        ^^^^ the trait `IntoResponse` is not implemented for `Task`
```

The fix: wrap it in `Json` (and derive `Serialize`), or return a `String`, a `(StatusCode, ...)` tuple, etc.

### 2. Using the old `:id` path syntax

Axum 0.8 removed colon-style parameters. Writing `.route("/tasks/:id", get(get_task))` panics at router-construction time with `Path segments must not start with ":". For capture groups, use {capture}.` — a built-in guard carried over from the 0.7→0.8 migration that fires for **any** `:`-prefixed segment (you don't need to mix old and new syntax to trigger it). Always use braces: `{id}`. Wildcards use `{*rest}`. See [Routing](/16-web-apis/03-routing/).

### 3. Using `std::net::TcpListener` instead of Tokio's

`axum::serve` wants an async listener. If you reach for `std::net::TcpListener` (no `.await` on `bind`), the types won't line up. Always use `tokio::net::TcpListener::bind(...).await`. A handy detail: binding async means you must be inside the runtime, which `#[tokio::main]` guarantees.

### 4. Forgetting the async runtime

If you write a plain `fn main` and call `axum::serve(...).await`, it won't even parse (`.await` is only valid in an async context). And if you build a runtime but never block on the serve future, the program exits immediately. `#[tokio::main]` handles both: it starts the runtime and blocks on your async `main`. If you see your server "start and instantly exit," you almost certainly dropped the `.await` on `axum::serve`.

### 5. Expecting handlers to run in parallel by default

Tokio's multi-thread scheduler (the default for `#[tokio::main]`) *can* run handlers on different threads, but a single handler is still one future. Don't block it with synchronous CPU-heavy work or `std::thread::sleep` — that stalls the worker thread. Use `.await`able async operations, or `tokio::task::spawn_blocking` for unavoidable blocking work. Rust is **not** "multi-threaded by default" in the sense of magically parallelizing your code; concurrency is explicit and opt-in. See [Concurrency](/11-async/10-concurrency/).

---

## Best Practices

- **Extract a `fn app() -> Router` builder.** Keep router construction separate from `main`. It makes the router reusable from tests (you can call handlers via `tower::ServiceExt::oneshot` without binding a real port) and keeps `main` to just "bind and serve."
- **Bind `0.0.0.0` in containers, `127.0.0.1` locally.** `127.0.0.1` is loopback-only; inside Docker you must bind `0.0.0.0` to accept traffic from outside the container.
- **Reach for `#[axum::debug_handler]` when handler errors are cryptic.** It costs nothing at runtime and turns "this function isn't a `Handler`" into "this specific type isn't an extractor / isn't a response."
- **Prefer `-> impl IntoResponse` for handlers that return mixed shapes**, and a concrete type (`Json<T>`, `Result<Json<T>, AppError>`) when the shape is fixed and you want it documented in the signature.
- **Let extractors validate.** Don't accept `String` and parse by hand if `Path<u64>` or `Json<T>` already does it and returns a clean 4xx. Push as much validation into the type system as you can.
- **Keep handlers small and `async`-pure.** Inject dependencies (DB pools, config) via shared state rather than globals; see [State Management](/16-web-apis/06-state-management/).

---

## Real-World Example

A small in-memory task API with three endpoints, shared state behind `Arc<Mutex<...>>`, a `Result`-returning handler that maps "not found" to a `404`, and a `fn app()` builder. This is the shape of a real Axum service before you swap the `Vec` for a database.

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Serialize)]
struct Task {
    id: u64,
    title: String,
    done: bool,
}

#[derive(Deserialize)]
struct NewTask {
    title: String,
}

// Shared application state. `Clone` is cheap: the Arcs are reference-counted
// pointers, so cloning the state just bumps the counters.
#[derive(Clone, Default)]
struct AppState {
    tasks: Arc<Mutex<Vec<Task>>>,
    next_id: Arc<Mutex<u64>>,
}

// GET /tasks — return the whole list.
async fn list_tasks(State(state): State<AppState>) -> Json<Vec<Task>> {
    let tasks = state.tasks.lock().unwrap();
    Json(tasks.clone())
}

// POST /tasks — create a task, reply 201 with the created resource.
async fn create_task(
    State(state): State<AppState>,
    Json(body): Json<NewTask>,
) -> impl IntoResponse {
    let mut id_guard = state.next_id.lock().unwrap();
    *id_guard += 1;
    let task = Task { id: *id_guard, title: body.title, done: false };
    state.tasks.lock().unwrap().push(task.clone());
    (StatusCode::CREATED, Json(task))
}

// GET /tasks/{id} — 200 with the task, or 404 if it doesn't exist.
// Returning Result<T, E> where both implement IntoResponse is idiomatic.
async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Task>, StatusCode> {
    let tasks = state.tasks.lock().unwrap();
    tasks
        .iter()
        .find(|t| t.id == id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

// The router builder — reusable from tests and from main.
fn app() -> Router {
    let state = AppState::default();
    Router::new()
        .route("/tasks", get(list_tasks).post(create_task))
        .route("/tasks/{id}", get(get_task))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

A few production-relevant notes on this code:

- **`State<AppState>` is the dependency-injection mechanism.** `.with_state(state)` attaches it to the router; the `State(state)` extractor pulls it into each handler. In a real app, `AppState` holds a database connection pool, config, an HTTP client, etc. (Full treatment in [State Management](/16-web-apis/06-state-management/) and [Database Integration](/17-database/).)
- **`Arc` + `Mutex` is how you share mutable state across concurrent handlers.** `Arc` gives shared ownership (multiple handlers point at the same data); `Mutex` makes concurrent mutation safe. The compiler will not let you share `&mut` across tasks without one of these — that's the borrow checker preventing data races at compile time. See [Arc/Mutex Pattern](/11-async/12-arc-mutex-pattern/).
- **The `Result<Json<Task>, StatusCode>` return** lets `get_task` produce either a `200` with JSON or a bare `404`. For richer, structured errors (a JSON error body, mapping different error kinds to different statuses), see [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/).

> **Note:** Handlers can also be inline async closures, which feel even more Express-like for trivial routes: `.route("/", get(|| async { "root" }))`. Use named `async fn`s once a handler does anything non-trivial — they're easier to test and read.

---

## Further Reading

- [Axum documentation (docs.rs)](https://docs.rs/axum/latest/axum/): the canonical reference; the crate-level docs are excellent.
- [Axum examples (GitHub)](https://github.com/tokio-rs/axum/tree/main/examples): runnable examples for nearly every feature in this section.
- [`axum::serve` docs](https://docs.rs/axum/latest/axum/fn.serve.html) and [`Router` docs](https://docs.rs/axum/latest/axum/struct.Router.html).
- Within this guide:
  - [Setting Up an Axum Project](/16-web-apis/02-axum-setup/): deps, features, and a hello-server from scratch.
  - [Routing](/16-web-apis/03-routing/): path/query params, method routing, nested routers, fallbacks.
  - [Extractors](/16-web-apis/04-extractors/): how `Path`, `Query`, `Json`, `State` work and how to write your own.
  - [Request and Response Handling](/16-web-apis/07-request-response/) — `IntoResponse`, status codes, headers.
  - [JSON APIs](/16-web-apis/08-json-apis/) — a fuller CRUD resource with serde.
  - [State Management](/16-web-apis/06-state-management/) — sharing a DB pool / config via `State<T>`.
  - [Middleware](/16-web-apis/05-middleware/) — Tower layers, `tower-http`, `from_fn`.
  - [Framework Comparison](/16-web-apis/00-framework-comparison/) — Axum vs Actix Web vs Rocket vs Express/Nest.
  - Background: [The Tokio Runtime](/11-async/02-tokio-intro/), [Tokio Setup](/11-async/03-tokio-setup/), [Promises vs Futures](/11-async/00-promises-vs-futures/).

---

## Exercises

### Exercise 1: A second endpoint

**Difficulty:** Beginner

**Objective:** Get comfortable adding routes and returning JSON.

**Instructions:** Starting from the first Rust server in this page, add a `GET /version` route that returns the JSON `{"version":"1.0.0","name":"task-api"}`. Define a `#[derive(Serialize)]` struct for the response and wire it into the router.

<details>
<summary>Solution</summary>

```rust
use axum::{routing::get, Json, Router};
use serde::Serialize;

#[derive(Serialize)]
struct Version {
    version: &'static str,
    name: &'static str,
}

async fn version() -> Json<Version> {
    Json(Version { version: "1.0.0", name: "task-api" })
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/version", get(version));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Hitting `GET /version` returns `{"version":"1.0.0","name":"task-api"}`.

</details>

### Exercise 2: Echo with a path parameter and a status code

**Difficulty:** Intermediate

**Objective:** Combine a `Path` extractor with a tuple `(StatusCode, ...)` response.

**Instructions:** Write a `GET /echo/{message}` handler that returns the message wrapped in JSON as `{"echo":"<message>"}` with an explicit `200 OK` status, using a `(StatusCode, Json<...>)` tuple. The `{message}` parameter is a `String`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;

#[derive(Serialize)]
struct Echo {
    echo: String,
}

async fn echo(Path(message): Path<String>) -> impl IntoResponse {
    (StatusCode::OK, Json(Echo { echo: message }))
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/echo/{message}", get(echo));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`GET /echo/hello` returns `{"echo":"hello"}` with status `200 OK`.

</details>

### Exercise 3: Update a task (PUT) against shared state

**Difficulty:** Advanced

**Objective:** Add a mutating endpoint to the real-world example, using `State` and a `Result` return that distinguishes "found and updated" from "not found."

**Instructions:** Extend the Real-World Example with `PUT /tasks/{id}` that accepts a JSON body `{"done": true}` and toggles the matching task's `done` field. Return `Json<Task>` on success and `StatusCode::NOT_FOUND` if no task has that id. Chain the new method onto the existing `/tasks/{id}` route.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Serialize)]
struct Task {
    id: u64,
    title: String,
    done: bool,
}

#[derive(Deserialize)]
struct UpdateTask {
    done: bool,
}

#[derive(Clone, Default)]
struct AppState {
    tasks: Arc<Mutex<Vec<Task>>>,
}

async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Task>, StatusCode> {
    let tasks = state.tasks.lock().unwrap();
    tasks.iter().find(|t| t.id == id).cloned().map(Json).ok_or(StatusCode::NOT_FOUND)
}

// PUT /tasks/{id} — toggle `done`, or 404 if the task is missing.
async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(body): Json<UpdateTask>,
) -> Result<Json<Task>, StatusCode> {
    let mut tasks = state.tasks.lock().unwrap();
    match tasks.iter_mut().find(|t| t.id == id) {
        Some(task) => {
            task.done = body.done;
            Ok(Json(task.clone()))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

fn app() -> Router {
    let state = AppState {
        tasks: Arc::new(Mutex::new(vec![Task {
            id: 1,
            title: "Write the docs".to_string(),
            done: false,
        }])),
    };
    Router::new()
        .route("/tasks/{id}", get(get_task).put(update_task))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

`PUT /tasks/1` with body `{"done":true}` returns `{"id":1,"title":"Write the docs","done":true}`; `PUT /tasks/999` returns `404 Not Found`. Note `iter_mut()` — you need a mutable iterator to modify the task in place, and the `Mutex` guard (`tasks`) is what makes that mutation safe across concurrent requests.

</details>
