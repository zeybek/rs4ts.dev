---
title: "Choosing a Rust Web Framework: Axum vs Actix Web vs Rocket"
description: "Map Rust's Axum, Actix Web, and Rocket onto the Express, Nest, and Fastify tradeoffs you know from Node, and see why this guide picks Axum as the default."
---

Coming from Node.js, picking a web framework felt easy: Express for the unopinionated default, Nest for the batteries-included enterprise option, Fastify when you cared about throughput. Rust offers a similar spectrum, but the tradeoffs land in different places. This guide maps the three mainstream Rust web frameworks onto the mental model you already have from Express and Nest, and gives you a concrete way to choose.

---

## Quick Overview

Rust's web ecosystem has consolidated around three production-grade frameworks: **Axum** (the tower/tokio-native default), **Actix Web** (the throughput-focused veteran), and **Rocket** (the ergonomic, macro-driven option). All three are mature and used in production; they differ mainly in *philosophy*: how much magic, how much boilerplate, and how they compose with the wider async ecosystem. For most TypeScript/JavaScript developers building a new API, **Axum is the closest analogue to "modern Express" and the recommended starting point**, which is why the rest of this section focuses on it.

> **Note:** A "web framework" in Rust is just a regular crate (Rust's word for a package; see [Section 01: Cargo Basics](/01-getting-started/03-cargo-basics/)). There is no global install, no CLI scaffolder you must adopt, and no runtime baked in. You add the crate with `cargo add`, you bring your own async runtime (almost always **tokio**), and you compile a single self-contained binary.

---

## TypeScript/JavaScript Example

In Node.js, the framework choice usually comes down to three archetypes. Here is the same "get a user by id" endpoint in each, so the comparison is apples-to-apples.

```typescript
// Express.js — unopinionated, minimal, the lingua franca
import express from "express";

const app = express();
const users = [{ id: 1, name: "Ada" }];

app.get("/users/:id", (req, res) => {
  const id = Number(req.params.id);
  const user = users.find((u) => u.id === id);
  if (!user) return res.status(404).json({ error: "not found" });
  res.json(user);
});

app.listen(3000);
```

```typescript
// NestJS — opinionated, decorator-driven, DI container, "Angular for the backend"
import { Controller, Get, Param, NotFoundException } from "@nestjs/common";

@Controller("users")
export class UsersController {
  private users = [{ id: 1, name: "Ada" }];

  @Get(":id")
  getUser(@Param("id") id: string) {
    const user = this.users.find((u) => u.id === Number(id));
    if (!user) throw new NotFoundException();
    return user; // serialized to JSON automatically
  }
}
```

```typescript
// Fastify — Express-shaped but schema-first, optimized for throughput
import Fastify from "fastify";

const app = Fastify();
const users = [{ id: 1, name: "Ada" }];

app.get<{ Params: { id: string } }>("/users/:id", async (req, reply) => {
  const user = users.find((u) => u.id === Number(req.params.id));
  if (!user) return reply.code(404).send({ error: "not found" });
  return user;
});

app.listen({ port: 3000 });
```

**The Node.js mental model:**

- **Express** = unopinionated, glue-it-yourself, huge middleware ecosystem.
- **Nest** = opinionated structure, dependency injection, decorators, larger learning curve.
- **Fastify** = Express-shaped, but built for speed and schema validation.

---

## Rust Equivalent

The same endpoint in each Rust framework. Every snippet below was compiled against current crate versions (axum 0.8.9, actix-web 4.13.0, rocket 0.5.1).

```rust
// Axum 0.8 — tower/tokio-native, type-driven extractors, "modern Express"
// Cargo.toml: axum = "0.8", tokio = { version = "1", features = ["full"] }, serde = { version = "1", features = ["derive"] }
use axum::{routing::get, Router, Json, extract::Path};
use serde::Serialize;

#[derive(Serialize)]
struct User {
    id: u32,
    name: String,
}

async fn get_user(Path(id): Path<u32>) -> Json<User> {
    Json(User { id, name: "Ada".to_string() })
}

#[tokio::main]
async fn main() {
    // Path params use {id} in 0.8 (NOT the old :id from 0.7).
    let app: Router = Router::new().route("/users/{id}", get(get_user));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

```rust
// Actix Web 4 — actor-influenced, macro routing, raw-throughput focus
// Cargo.toml: actix-web = "4", serde = { version = "1", features = ["derive"] }
use actix_web::{get, web, App, HttpServer, Responder};
use serde::Serialize;

#[derive(Serialize)]
struct User {
    id: u32,
    name: String,
}

#[get("/users/{id}")]
async fn get_user(path: web::Path<u32>) -> impl Responder {
    let id = path.into_inner();
    web::Json(User { id, name: "Ada".to_string() })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // The app factory closure runs once per worker thread.
    HttpServer::new(|| App::new().service(get_user))
        .bind(("0.0.0.0", 3000))?
        .run()
        .await
}
```

```rust
// Rocket 0.5 — the most "Nest-like": heavy macros, attribute routing, batteries included
// Cargo.toml: rocket = { version = "0.5", features = ["json"] }, serde = { version = "1", features = ["derive"] }
#[macro_use]
extern crate rocket;

use rocket::serde::{json::Json, Serialize};

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct User {
    id: u32,
    name: String,
}

#[get("/users/<id>")]
fn get_user(id: u32) -> Json<User> {
    Json(User { id, name: "Ada".to_string() })
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![get_user])
}
```

Notice three things a TypeScript/JavaScript developer will spot immediately:

1. **You bring the runtime.** Axum and Actix start an async runtime explicitly via `#[tokio::main]` / `#[actix_web::main]`; Rocket hides it behind `#[launch]`. There is no always-on event loop like Node's — see [Section 11: Async](/11-async/) for why Rust futures are lazy and require a runtime.
2. **The type system does the parsing.** `Path<u32>` parses and validates the path segment into a `u32` *before* your handler runs. A non-numeric id never reaches your code. In Express you would call `Number(req.params.id)` by hand and check for `NaN`.
3. **JSON serialization is a return-type concern, not a side effect.** You return `Json<User>` instead of calling `res.json(user)`. The framework turns it into a response with the right `Content-Type`.

---

## Detailed Explanation

### Axum: tower-native, type-driven, the recommended default

Axum is built by the tokio team on top of two foundations you will meet repeatedly: **tokio** (the async runtime) and **tower** (a generic middleware abstraction). Its defining idea is **extractors**: a handler's *parameter types* declare what it needs from the request, and the framework supplies them.

```rust
// Cargo.toml: axum = "0.8", tokio = { version = "1", features = ["full"] }
//             serde = { version = "1", features = ["derive"] }
use std::sync::{Arc, RwLock};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
struct Todo {
    id: u32,
    title: String,
    done: bool,
}

#[derive(Deserialize)]
struct NewTodo {
    title: String,
}

#[derive(Clone, Default)]
struct AppState {
    todos: Arc<RwLock<Vec<Todo>>>,
}

async fn create_todo(
    State(state): State<AppState>,   // shared state extractor
    Json(input): Json<NewTodo>,      // parsed + validated JSON body
) -> (StatusCode, Json<Todo>) {      // status + body, declared in the type
    let mut todos = state.todos.write().unwrap();
    let id = todos.len() as u32 + 1;
    let todo = Todo { id, title: input.title, done: false };
    todos.push(todo.clone());
    (StatusCode::CREATED, Json(todo))
}
```

Each handler argument is an extractor; each return type implements `IntoResponse`. Because everything is "just types," the compiler checks your wiring. Forget to register the state? You get a compile error (shown in [Common Pitfalls](#common-pitfalls)), not a `undefined is not a function` at 3 a.m. Middleware is `tower::Layer`, which means Axum can reuse the entire tower/tower-http ecosystem (tracing, compression, timeouts, CORS), covered in [Middleware and Layers](/16-web-apis/05-middleware/).

> **Tip:** Axum has *no* macros for routing or handlers. A handler is a plain `async fn`. This keeps the model close to Express's "a route is just a function" while adding compile-time guarantees.

### Actix Web: the throughput veteran

Actix Web is the oldest of the three and historically tops the public throughput benchmarks (TechEmpower). It originally grew out of the `actix` actor framework, but today the actor model is optional; most code looks like the example above. Its distinguishing operational detail is the **multi-threaded, share-nothing worker model**: `HttpServer::new` takes a *closure* that builds a fresh `App` per worker thread, so per-worker state is created once per thread, and truly shared state must live behind `web::Data` (an `Arc` internally).

Actix Web is an excellent choice when raw requests-per-second is your top metric, or when your team already knows it. Its API is slightly more "framework-y" than Axum's (more bespoke types, the `App`/`service` builder, the worker-closure quirk), and it composes with the generic tower ecosystem less directly than Axum does.

### Rocket: the ergonomic, Nest-like option

Rocket optimizes for *developer ergonomics*. It leans hard on attribute macros (`#[get("/users/<id>")]`, `#[launch]`), provides built-in request guards, typed forms, managed state, and a `Rocket.toml` config file: the closest thing in Rust to Nest's "everything included" feel. It hides the async runtime entirely.

The tradeoff: the heavy macro magic makes some compiler errors harder to read, and historically Rocket's release cadence has been slower (the jump to a stable async 0.5 took years). It composes with the tower ecosystem less naturally than Axum. For learning-by-reading-the-types — the whole premise of this guide — Rocket's magic can obscure what is actually happening.

### Why this guide standardizes on Axum

This entire Section 16 uses Axum because it hits the sweet spot for a TypeScript/JavaScript developer: minimal magic (handlers are plain functions), maximal compile-time help (extractors and `IntoResponse` are checked types), and smooth access to the tower/tokio ecosystem you will also use for databases ([Section 17](/17-database/)), tracing, and more. The concepts transfer: once you understand extractors and layers, picking up Actix or Rocket later is straightforward.

---

## Key Differences

| Dimension | Express / Nest / Fastify | Axum | Actix Web | Rocket |
| --- | --- | --- | --- | --- |
| Closest JS analogue | — | Modern Express | Fastify (speed) | NestJS (ergonomics) |
| Async runtime | Built-in event loop (always on) | tokio (you start it) | tokio via `actix-rt` (you start it) | tokio (hidden) |
| Routing style | `app.get("/x", fn)` | `Router::new().route("/x", get(fn))` | `#[get("/x")]` macro | `#[get("/x")]` macro |
| Path param syntax | `:id` | `{id}` | `{id}` | `<id>` |
| Request parsing | manual (`req.params`, `req.body`) | extractors (typed args) | extractors (typed args) | typed args + guards |
| Middleware | `app.use(fn)` | `tower::Layer` (huge ecosystem) | actix middleware | fairings |
| Macro magic | low (Express) / high (Nest) | very low | medium | high |
| Compile-time route/wiring checks | none (runtime) | strong | medium | medium |
| Maturity | very high | high | very high (oldest) | high |
| Raw throughput | moderate | very high | highest (typically) | high |
| Best when… | — | new API, want the default | max RPS / existing team | ergonomics-first |

### The runtime difference is the big one

In Node, the event loop is *always running*: `import express` and you have concurrency for free. In Rust there is no ambient runtime. You opt in with tokio, which is why `main` is `async` and annotated with `#[tokio::main]`. This is the same lazy-future model from [Section 11: Async](/11-async/): a future does nothing until a runtime polls it. The upside is control (you choose thread count, you can run multiple runtimes); the cost is one line of ceremony.

### "Decorators" are not what they look like

Rocket's and Actix's `#[get(...)]` look like Nest's `@Get()` decorators, but they are **procedural macros** that generate code at compile time, not runtime metadata read by a DI container. Macros are *not* decorators — see [Section 14: Macros](/14-macros/). Axum deliberately avoids them so a route is just `get(handler)`.

---

## Common Pitfalls

### Pitfall 1: Using the old `:id` path syntax in Axum 0.8

Axum 0.7 used Express-style `:id`. Axum 0.8 switched to `{id}` (matching the OpenAPI/RFC 6570 style). The old syntax still *compiles* but **panics at startup**, which can be confusing because the type checker does not catch it.

```rust
// panics at runtime (NOT a compile error)
let app: Router = Router::new().route("/users/:id", get(handler));
```

Running it produces this real panic:

```text
thread 'main' panicked at src/main.rs:10:37:
Path segments must not start with `:`. For capture groups, use `{capture}`. If you meant to literally match a segment starting with a colon, call `without_v07_checks` on the router.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

The fix is to use `{id}`. See [Routing in Axum](/16-web-apis/03-routing/) for the full path-parameter story.

### Pitfall 2: Forgetting `.with_state(...)` in Axum

Because extractors are types, a missing piece of wiring becomes a compile error, but the message names framework internals, which is unfamiliar. If a handler takes `State<AppState>` but you never call `.with_state(...)`, the router's state type (`()`) does not match the handler's (`AppState`):

```rust
// does not compile (error[E0308]: mismatched types)
async fn handler(State(state): State<AppState>) -> String {
    state.name
}

#[tokio::main]
async fn main() {
    // Forgot `.with_state(AppState::default())`
    let app: Router = Router::new().route("/", get(handler));
    // ...
}
```

The actual compiler output:

```text
error[E0308]: mismatched types
   --> src/main.rs:15:48
    |
 15 |     let app: Router = Router::new().route("/", get(handler));
    |                                     -----      ^^^^^^^^^^^^ expected `MethodRouter`, found `MethodRouter<AppState>`
    |                                     |
    |                                     arguments to this method are incorrect
    |
    = note: expected struct `MethodRouter<()>`
               found struct `MethodRouter<AppState>`
```

The fix: `Router::new().route("/", get(handler)).with_state(AppState::default())`. Read `MethodRouter<AppState>` as "this router needs `AppState` supplied." State is covered in depth in [Shared Application State in Axum](/16-web-apis/06-state-management/).

### Pitfall 3: Expecting the framework to start a runtime for you

A `main` that calls async code without a runtime will not compile, and an `async fn main()` without `#[tokio::main]` will not even be a valid entry point. This trips up developers used to Node's always-on loop. Always annotate (`#[tokio::main]`, `#[actix_web::main]`, or `#[launch]` for Rocket). See [Setting Up an Axum Project](/16-web-apis/02-axum-setup/).

### Pitfall 4: Choosing on benchmarks alone

TechEmpower numbers are real, but for the vast majority of APIs the bottleneck is your database, not the framework's request dispatch. All three frameworks are *far* faster than Node. Choose on ecosystem fit, team familiarity, and readability — not a 5% benchmark delta.

### Pitfall 5: Mixing up Actix's worker closure

`HttpServer::new(|| App::new()...)` runs the closure **once per worker thread**. State created *inside* the closure is per-worker, not shared. Truly shared state must be created outside and moved in via `web::Data` (see the [exercise solution](#exercises) below). This has no Express equivalent because Node is single-threaded by default.

---

## Best Practices

- **Default to Axum for new projects** unless you have a specific reason not to. It has the gentlest "read the types to understand the code" story, the broadest middleware ecosystem (tower-http), and first-party alignment with tokio.
- **Pick Actix Web** when maximum throughput is a hard requirement, or when your team/codebase already standardizes on it.
- **Pick Rocket** when developer ergonomics and a batteries-included feel matter more than ecosystem composability, and you are comfortable with heavier macro magic.
- **Don't fragment your stack.** Standardize on *one* framework per service. The extractor/middleware concepts do not transfer line-for-line between them.
- **Lean on the type system.** In all three, prefer typed extractors (`Path<u32>`, `Json<T>`) over hand-parsing strings. Let invalid input fail at the boundary, not deep in a handler.
- **Verify the version's API before you write it.** These crates move fast (the axum 0.7 → 0.8 route-syntax break is a perfect example). Run `cargo add <crate>` in a scratch project and check docs.rs for the version you actually pulled.

> **Note:** The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. All examples here target that toolchain.

---

## Real-World Example

A small but production-shaped Axum service: an in-memory todo API with shared state, typed JSON in/out, a nested resource, proper status codes, and **graceful shutdown** (drain in-flight requests on Ctrl+C, the equivalent of handling `SIGTERM` in a Node process). This is the shape your real services will take; subsequent files swap the `RwLock<Vec<_>>` for a real database pool ([Section 17](/17-database/)).

```rust
// Cargo.toml:
//   axum  = "0.8"
//   tokio = { version = "1", features = ["full"] }
//   serde = { version = "1", features = ["derive"] }
use std::sync::{Arc, RwLock};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
struct Todo {
    id: u32,
    title: String,
    done: bool,
}

#[derive(Deserialize)]
struct NewTodo {
    title: String,
}

// Shared application state: a thread-safe, in-memory store.
// `Arc` lets every worker share one store; `RwLock` guards mutation.
#[derive(Clone, Default)]
struct AppState {
    todos: Arc<RwLock<Vec<Todo>>>,
}

async fn list_todos(State(state): State<AppState>) -> Json<Vec<Todo>> {
    let todos = state.todos.read().unwrap();
    Json(todos.clone())
}

async fn create_todo(
    State(state): State<AppState>,
    Json(input): Json<NewTodo>,
) -> (StatusCode, Json<Todo>) {
    let mut todos = state.todos.write().unwrap();
    let id = todos.len() as u32 + 1;
    let todo = Todo { id, title: input.title, done: false };
    todos.push(todo.clone());
    (StatusCode::CREATED, Json(todo))
}

async fn get_todo(
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> Result<Json<Todo>, StatusCode> {
    let todos = state.todos.read().unwrap();
    todos
        .iter()
        .find(|t| t.id == id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND) // missing -> 404, no panic
}

fn app() -> Router {
    Router::new()
        .route("/todos", get(list_todos).post(create_todo))
        .route("/todos/{id}", get(get_todo))
        .with_state(AppState::default())
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
}
```

This compiles and runs on axum 0.8.9. Once started, `curl localhost:3000/todos` returns `[]`, a `POST` with `{"title":"ship it"}` returns `201 Created` with the new todo, and `GET /todos/99` returns a clean `404`. Pressing Ctrl+C lets in-flight requests finish before the process exits — exactly the lifecycle behavior you want behind a load balancer. The full request/response, error-handling, and validation details are expanded in [JSON REST APIs](/16-web-apis/08-json-apis/), [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/), and [Request Validation](/16-web-apis/09-validation/).

---

## Further Reading

- **Axum docs:** <https://docs.rs/axum/latest/axum/> and the tutorial <https://github.com/tokio-rs/axum/tree/main/examples>
- **Actix Web:** <https://actix.rs/docs/> and <https://docs.rs/actix-web/latest/actix_web/>
- **Rocket:** <https://rocket.rs/guide/> and <https://docs.rs/rocket/latest/rocket/>
- **tower / tower-http** (Axum's middleware ecosystem): <https://docs.rs/tower-http/latest/tower_http/>
- **TechEmpower benchmarks** (read with the grain of salt from Pitfall 4): <https://www.techempower.com/benchmarks/>
- Guide cross-links: [Setting Up an Axum Project](/16-web-apis/02-axum-setup/) · [Axum Fundamentals](/16-web-apis/01-axum-basics/) · [Routing in Axum](/16-web-apis/03-routing/) · [Extractors](/16-web-apis/04-extractors/) · [Middleware and Layers](/16-web-apis/05-middleware/) · [Shared Application State in Axum](/16-web-apis/06-state-management/) · [Section 11: Async](/11-async/) · [Section 17: Database](/17-database/)

---

## Exercises

### Exercise 1: Read the types, predict the framework

**Difficulty:** Beginner

**Objective:** Build the mental model that "a handler's argument types declare its dependencies."

**Instructions:** Without running anything, decide which framework each snippet is from and what the path parameter syntax is. Then explain, in one sentence, what `Path<u32>` guarantees that Express's `req.params.id` does not.

1. `#[get("/users/<id>")] fn f(id: u32) -> ...`
2. `Router::new().route("/users/{id}", get(f))`
3. `#[get("/users/{id}")] async fn f(path: web::Path<u32>) -> ...`

<details>
<summary>Solution</summary>

1. **Rocket:** angle-bracket `<id>` syntax and a non-`async` `fn` returning `_` under `#[launch]`.
2. **Axum:** the `Router::new().route(..., get(f))` builder with `{id}` path syntax.
3. **Actix Web:** the `#[get(...)]` attribute macro with `web::Path<u32>` extractor.

`Path<u32>` (or `web::Path<u32>`) **parses and validates the segment into a `u32` before the handler runs**: a request to `/users/abc` is rejected at the boundary with a 4xx and never reaches your code. Express's `req.params.id` is always a `string`; you must call `Number(...)` and check for `NaN` yourself, and forgetting to do so is a runtime bug, not a compile error.

</details>

### Exercise 2: Axum router with a nested resource and a fallback

**Difficulty:** Intermediate

**Objective:** Compose an Axum `Router` with a health check, a nested API sub-router, a 404 fallback, and request tracing: the skeleton of a real service.

**Instructions:** Write an Axum app that:

- serves `GET /health` returning `"ok"`,
- nests a sub-router under `/api/v1` that serves `GET /users`,
- returns a custom 404 (`"no such route"`) for anything else,
- attaches `tower_http::trace::TraceLayer` so each request is logged.

Dependencies you will need: `axum`, `tokio`, `tower-http` (feature `trace`), `tracing`, `tracing-subscriber`.

<details>
<summary>Solution</summary>

```rust
// Cargo.toml:
//   axum  = "0.8"
//   tokio = { version = "1", features = ["full"] }
//   tower-http = { version = "0.6", features = ["trace"] }
//   tracing = "0.1"
//   tracing-subscriber = "0.3"
use axum::{http::StatusCode, routing::get, Router};
use tower_http::trace::TraceLayer;

async fn list_users() -> &'static str {
    "users"
}

async fn health() -> &'static str {
    "ok"
}

async fn fallback() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "no such route")
}

fn api_routes() -> Router {
    Router::new().route("/users", get(list_users))
}

fn app() -> Router {
    Router::new()
        .route("/health", get(health))
        .nest("/api/v1", api_routes()) // -> GET /api/v1/users
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

`nest` prefixes every route in the sub-router, so `list_users` becomes `GET /api/v1/users`. `fallback` is Axum's catch-all for unmatched routes (Express's app-level 404 handler). `TraceLayer` is a `tower::Layer` — middleware that wraps the whole router — and is the idiomatic way to log requests. Routing details are in [Routing in Axum](/16-web-apis/03-routing/); layers in [Middleware and Layers](/16-web-apis/05-middleware/).

</details>

### Exercise 3: Shared state across Actix Web's worker threads

**Difficulty:** Advanced

**Objective:** Understand why Actix's per-worker app factory means shared state must be built *outside* the closure, a concept with no Express analogue.

**Instructions:** Write an Actix Web app with a single `GET /` route that increments and returns a global visit counter. The counter must be shared across all worker threads (so `/` returns `1, 2, 3, ...` regardless of which worker handles the request). Use an atomic to avoid a lock. Explain why the state cannot be created inside the `HttpServer::new(|| ...)` closure.

<details>
<summary>Solution</summary>

```rust
// Cargo.toml: actix-web = "4"
use std::sync::atomic::{AtomicU64, Ordering};

use actix_web::{get, web, App, HttpServer, Responder};

struct AppState {
    visits: AtomicU64,
}

#[get("/")]
async fn count(data: web::Data<AppState>) -> impl Responder {
    let n = data.visits.fetch_add(1, Ordering::Relaxed) + 1;
    format!("visit #{n}")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Build the shared state ONCE, outside the closure...
    let state = web::Data::new(AppState { visits: AtomicU64::new(0) });

    // ...then move a CLONE of the handle into each worker.
    // `web::Data` is an `Arc` internally, so clones share the same AtomicU64.
    HttpServer::new(move || App::new().app_data(state.clone()).service(count))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
```

The closure passed to `HttpServer::new` runs **once per worker thread**. If you wrote `App::new().app_data(web::Data::new(AppState { ... }))` *inside* the closure, every worker would get its *own* counter, and the returned numbers would depend on which worker happened to serve the request. Creating `state` outside and cloning the `web::Data` (an `Arc`) handle in gives every worker a pointer to the *same* `AtomicU64`. `fetch_add` increments it atomically without a lock. This share-nothing-by-default worker model is unique to a multi-threaded runtime; Node's single-threaded loop has no equivalent. Axum's `with_state` solves the same problem with a single `Arc` clone per request — see [Shared Application State in Axum](/16-web-apis/06-state-management/).

</details>
