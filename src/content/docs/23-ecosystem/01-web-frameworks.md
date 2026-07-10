---
title: "Web Frameworks: Axum, Actix Web, Rocket, and Poem"
description: "Axum, Actix Web, Rocket, and Poem mapped to Express and NestJS: how each Rust web framework declares routes, handles extractors, and which to pick."
---

## Quick Overview

In Node you pick a web framework — Express, Fastify, NestJS, Koa — and the choice is mostly about ergonomics and middleware; they all run on the same V8/libuv runtime. Rust has the same shape (router, handlers, middleware) but the choice carries more weight: a framework also commits you to an async runtime, a middleware abstraction, and a set of compile-time guarantees. This page is the **ecosystem map**: what the four frameworks a Node developer will actually encounter (**Axum**, **Actix Web**, **Rocket**, **Poem**) are, how mature each is, and which one fits which job, so you can choose with the same confidence you'd choose Express vs NestJS.

> **Note:** This page is the *ecosystem-level "which framework and why."* The hands-on Axum guide (routing, extractors, middleware, state, auth, WebSockets, deployment) lives in [Section 16: Web APIs](/16-web-apis/), and a focused build-oriented comparison is in [Framework Comparison](/16-web-apis/00-framework-comparison/). Here we stay at the survey altitude.

---

## TypeScript/JavaScript Example

In the Node world the framework landscape is familiar, and switching between them is a matter of taste and team convention. Here is the same tiny JSON API in two popular Node frameworks, to anchor the comparison:

```typescript
// Express — the de-facto default, minimal and unopinionated.
import express from "express";

const app = express();
app.use(express.json());

app.get("/hello/:name", (req, res) => {
  res.json({ message: `Hello, ${req.params.name}!` });
});

app.post("/tasks", (req, res) => {
  res.status(201).json({ id: 1, title: req.body.title });
});

app.listen(8080, () => console.log("listening on http://127.0.0.1:8080"));
```

```typescript
// NestJS — opinionated, decorator-driven, batteries-included (DI, modules).
import { Controller, Get, Post, Param, Body } from "@nestjs/common";

@Controller()
export class AppController {
  @Get("hello/:name")
  hello(@Param("name") name: string) {
    return { message: `Hello, ${name}!` };
  }

  @Post("tasks")
  createTask(@Body() body: { title: string }) {
    return { id: 1, title: body.title };
  }
}
```

Notice the spectrum: Express is a thin, explicit core; NestJS layers a decorator-and-DI framework on top. The Rust ecosystem has the **same spectrum**, and the mapping is surprisingly clean:

| Node framework | Closest Rust framework | Shared trait |
| --- | --- | --- |
| Express / Fastify | **Axum** | Minimal core, composable middleware, explicit |
| (raw, maximum throughput) | **Actix Web** | Performance-first, large feature surface |
| NestJS (decorator-driven) | **Rocket** | Attribute macros, batteries-included, ergonomic |
| Fastify-with-plugins / OpenAPI-first | **Poem** | Modern, ergonomic, first-class OpenAPI |

---

## Rust Equivalent

Here is that same JSON API in each of the four frameworks, so you can compare the handler ergonomics side by side. The recorded comparison used axum 0.8.9, actix-web 4.13.0, rocket 0.5.1, poem 3.1.12, the repository's [pinned Rust toolchain](/00-introduction/05-version-policy/), and the 2024 edition. Those are reproducibility coordinates, not permanent “current version” claims.

### Axum (Tokio + Tower)

```rust
// Cargo.toml: cargo add axum tokio --features tokio/full && cargo add serde --features derive
use axum::{Json, Router, extract::Path, routing::{get, post}};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct Greeting {
    message: String,
}

#[derive(Deserialize)]
struct NewTask {
    title: String,
}

#[derive(Serialize)]
struct Task {
    id: u64,
    title: String,
}

// Handlers are plain async fns; inputs are typed "extractors".
async fn hello(Path(name): Path<String>) -> Json<Greeting> {
    Json(Greeting { message: format!("Hello, {name}!") })
}

async fn create_task(Json(body): Json<NewTask>) -> Json<Task> {
    Json(Task { id: 1, title: body.title })
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/hello/{name}", get(hello)) // {name}, not :name, in axum 0.8
        .route("/tasks", post(create_task));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

### Actix Web

```rust
// Cargo.toml: cargo add actix-web && cargo add serde --features derive
use actix_web::{App, HttpServer, Responder, get, post, web};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct Greeting {
    message: String,
}

#[derive(Deserialize)]
struct NewTask {
    title: String,
}

#[derive(Serialize)]
struct Task {
    id: u64,
    title: String,
}

// Attribute macros declare the route, like a NestJS @Get / @Post decorator.
#[get("/hello/{name}")]
async fn hello(name: web::Path<String>) -> impl Responder {
    web::Json(Greeting { message: format!("Hello, {name}!") })
}

#[post("/tasks")]
async fn create_task(body: web::Json<NewTask>) -> impl Responder {
    web::Json(Task { id: 1, title: body.title.clone() })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("listening on http://127.0.0.1:8080");
    // The closure is a per-worker App factory: Actix runs one App per thread.
    HttpServer::new(|| App::new().service(hello).service(create_task))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
```

### Rocket

```rust
// Cargo.toml: cargo add rocket --features json
#[macro_use]
extern crate rocket;

use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct Greeting {
    message: String,
}

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
struct NewTask {
    title: String,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct Task {
    id: u64,
    title: String,
}

// Typed path segments are parsed and injected for you.
#[get("/hello/<name>")]
fn hello(name: &str) -> Json<Greeting> {
    Json(Greeting { message: format!("Hello, {name}!") })
}

#[post("/tasks", data = "<body>")]
fn create_task(body: Json<NewTask>) -> Json<Task> {
    Json(Task { id: 1, title: body.title.clone() })
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![hello, create_task])
}
```

### Poem

```rust
// Cargo.toml: cargo add poem tokio --features tokio/full && cargo add serde --features derive
use poem::listener::TcpListener;
use poem::web::{Json, Path};
use poem::{Route, Server, get, handler, post};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct Greeting {
    message: String,
}

#[derive(Deserialize)]
struct NewTask {
    title: String,
}

#[derive(Serialize)]
struct Task {
    id: u64,
    title: String,
}

#[handler]
fn hello(Path(name): Path<String>) -> Json<Greeting> {
    Json(Greeting { message: format!("Hello, {name}!") })
}

#[handler]
fn create_task(Json(body): Json<NewTask>) -> Json<Task> {
    Json(Task { id: 1, title: body.title })
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let app = Route::new()
        .at("/hello/:name", get(hello))
        .at("/tasks", post(create_task));
    println!("listening on http://127.0.0.1:3001");
    Server::new(TcpListener::bind("127.0.0.1:3001")).run(app).await
}
```

All four serve identical JSON. Hitting the Axum version with `curl`:

```text
$ curl -s localhost:8080/hello/Bob
{"message":"Hello, Bob!"}

$ curl -s -X POST localhost:8080/tasks -H 'content-type: application/json' -d '{"title":"write docs"}'
{"id":1,"title":"write docs"}
```

---

## Detailed Explanation

The four frameworks differ less in *what* they do than in *how* they ask you to express it. Three axes matter to a Node developer.

### How a route is declared

- **Axum** uses **method-router builders**: `get(handler)`, `post(handler)`, chained onto `Router::new().route(...)`. This is the Express `app.get("/path", fn)` model, just type-checked. There are no macros on your handlers: a handler is any `async fn` whose parameters all implement the extractor traits.
- **Actix Web** and **Rocket** use **attribute macros** on the handler — `#[get("/hello/{name}")]` and `#[get("/hello/<name>")]`. This is the NestJS `@Get()` decorator feel. The route string and the function live together, which many developers find readable, at the cost of more macro magic between you and the code.
- **Poem** is a hybrid: handlers are tagged with `#[handler]`, but routes are wired explicitly with `Route::new().at("/path", get(handler))`, very close to Axum.

### How request data reaches the handler

All four use the **extractor** pattern (Rust's typed answer to Express's `req.params` / `req.body`): a handler parameter's *type* declares what to pull out of the request and how to parse it. `Path<String>` parses a URL segment; `Json<T>` deserializes the body with Serde and rejects malformed input with a `400` automatically. The difference is naming and a few details: Axum and Poem destructure in the parameter (`Path(name): Path<String>`), Actix wraps in a smart-pointer-like `web::Path<String>`, Rocket injects the bare type (`name: &str`). Mechanically they are the same idea: **the type system parses and validates your inputs before your code runs**, which a Node developer typically gets only by hand-writing Zod or `class-validator` schemas.

### What runtime and middleware ecosystem you inherit

This is the deciding factor and the one without a Node analogue, because in Node the runtime is fixed:

- **Axum** is built directly on **Tokio** and the **Tower** middleware ecosystem. Choosing Axum means every Tower/`tower-http` layer (tracing, CORS, compression, timeouts, rate-limiting, auth) is available to you, and you interoperate with the huge Tokio ecosystem (sqlx, redis, tonic/gRPC) for free. See [Async Runtimes](/23-ecosystem/02-async-runtimes/) for why Tokio is the gravitational center.
- **Actix Web** runs on its own actor-flavored runtime (`actix-rt`, itself a thin layer over Tokio) and has its own middleware trait. It is largely self-contained: a strength (cohesive, fast) and a constraint (less reuse of the broader Tower ecosystem).
- **Rocket** also runs on Tokio under the hood but presents its own "fairings" middleware abstraction and its own request-guard model rather than Tower layers.
- **Poem** runs on Tokio and has its own endpoint/middleware traits, with a Tower-compatibility bridge.

### A note on the macro flavor

Rocket and Actix lean on attribute macros, which can feel like decorators — but they are **not** runtime decorators. A Rust attribute macro runs at **compile time**: it generates code that the compiler then type-checks. There is no reflection, no runtime metadata, and no DI container reading annotations at startup the way NestJS does. If a route signature is wrong, you get a compile error, not a 500 at request time.

---

## Key Differences

| Framework | Version | Runtime | Routing style | Middleware model | Maturity / fit |
| --- | --- | --- | --- | --- | --- |
| **Axum** | 0.8.9 | Tokio + Tower | Builder (`get(fn)`) | `tower`/`tower-http` layers | The community default; widest ecosystem reuse. Pick when in doubt. |
| **Actix Web** | 4.13.0 | actix-rt (on Tokio) | Attribute macros | Own `Transform`/`Service` | Battle-tested, top of throughput benchmarks. Pick for max raw performance or an existing Actix codebase. |
| **Rocket** | 0.5.1 | Tokio | Attribute macros | Fairings + request guards | Most ergonomic / "Rails-like". Pick for fast iteration and a guided, batteries-included feel. |
| **Poem** | 3.1.12 | Tokio | Builder + `#[handler]` | Own endpoints (+ Tower bridge) | Modern, ergonomic, **first-class OpenAPI** via `poem-openapi`. Pick when you want spec-driven APIs. |

> **Tip:** If you would reach for Express in Node, reach for **Axum** in Rust. It is the closest match in philosophy (minimal, composable, explicit) and it inherits the largest ecosystem. The rest of the Rust web ecosystem (database drivers, gRPC, observability) assumes Tokio, and Axum is the framework that sits most naturally on top of it.

### Why "maturity" looks different than in Node

A Node developer reads a `0.x` version number as "not production-ready." In Rust that heuristic misleads you. **Axum is `0.8` and is the most widely deployed Rust web framework in production**; it is maintained by the Tokio team and a `0.x` number reflects a willingness to make breaking changes between minor versions (as `0.7`→`0.8` did, swapping `:id` for `{id}`), not instability. Conversely **Actix Web is `4.x`** and equally production-grade. Judge a Rust crate by download counts, release cadence, and who maintains it, not by whether it has crossed `1.0`.

---

## Common Pitfalls

### Pitfall 1: Using the old axum 0.7 route syntax or server bootstrap

Two things changed in axum 0.8 that break copy-pasted older examples and most LLM-generated code:

- Path parameters now use brace syntax: `"/users/{id}"`, **not** the old colon form `"/users/:id"`.
- Servers start with `axum::serve(listener, app)` over a `tokio::net::TcpListener`. The old `axum::Server::bind(&addr).serve(app.into_make_service())` builder was **removed**.

Mixing the colon syntax into axum 0.8 does not give a friendly "use braces" message: depending on the route it panics at startup or silently never matches. Always use `{name}` and `axum::serve` on 0.8.

### Pitfall 2: Putting a body extractor before another extractor

Each request body can be consumed **once**. In Axum (and Poem), the body-consuming extractor — `Json`, `String`, `Bytes`, or `Request` — must be the **last** parameter. Put two body consumers in the wrong order and the handler simply *stops being a handler*, which produces a famously dense trait error rather than a clear message. This handler places `Json` before `Request`:

```rust
// does not compile (error[E0277]: the trait bound `... : Handler<_, _>` is not satisfied)
// Cargo.toml: cargo add axum tokio --features tokio/full && cargo add serde --features derive
use axum::{Json, Router, extract::Request, routing::post};
use serde::Deserialize;

#[derive(Deserialize)]
struct Payload {
    name: String,
}

// Json consumes the body, but Request (which also consumes it) comes after — invalid.
async fn handler(Json(payload): Json<Payload>, request: Request) -> String {
    format!("{} {}", payload.name, request.method())
}

#[tokio::main]
async fn main() {
    let _app: Router = Router::new().route("/", post(handler));
}
```

The real compiler output:

```text
error[E0277]: the trait bound `fn(Json<Payload>, Request<Body>) -> ... {handler}: Handler<_, _>` is not satisfied
   --> src/main.rs:16:54
    |
 16 |     let _app: Router = Router::new().route("/", post(handler));
    |                                                 ---- ^^^^^^^ the trait `Handler<_, _>` is not implemented for fn item `fn(Json<Payload>, Request<Body>) -> ... {handler}`
    |                                                 |
    |                                                 required by a bound introduced by this call
    |
    = note: Consider using `#[axum::debug_handler]` to improve the error message
note: required by a bound in `post`
```

The fix is to put `Request` (or any body extractor) **last**. The note is the important hint: annotate the handler with `#[axum::debug_handler]` and the compiler will tell you *exactly* which parameter is the problem instead of pointing at the route registration.

### Pitfall 3: Picking a framework on benchmarks alone

It is tempting to grep TechEmpower, see Actix at the top, and choose it. For an I/O-bound JSON API the framework is almost never your bottleneck: the database, serialization, and network are. **Axum and Actix are within noise of each other** for realistic workloads, and the ecosystem fit (does my DB driver, my tracing setup, my gRPC stack assume Tokio + Tower?) matters far more than a microbenchmark. Choose for maintainability and ecosystem first.

### Pitfall 4: Expecting decorators to mean reflection

If you come from NestJS, Rocket's and Actix's attribute macros look like decorators, and you may expect runtime DI, metadata reflection, or annotation scanning. There is none. The macros expand at compile time into ordinary registration code; dependency injection is just passing state explicitly (Axum's `State<T>`, Actix's `web::Data<T>`). See the broader treatment in [Macros are not Decorators](/14-macros/).

---

## Best Practices

- **Default to Axum unless you have a specific reason not to.** It is the lowest-risk choice: largest ecosystem, maintained by the Tokio team, and the framework most third-party guides and crates target.
- **Choose Actix Web** if you need the last few percent of raw throughput, are integrating with an existing Actix codebase, or specifically want its actor model.
- **Choose Rocket** for the most guided, ergonomic developer experience — internal tools, prototypes, and teams who liked Rails or NestJS and want the framework to make more decisions for them.
- **Choose Poem** when an **OpenAPI specification** is a first-class deliverable; `poem-openapi` generates the spec from your typed handlers, which is hard to match elsewhere.
- **Commit to one runtime.** All four sit on Tokio (directly or via a thin shim), so in practice you are in the Tokio world regardless. Don't fight it; embrace `tokio` and `tower-http`. See [Async Runtimes](/23-ecosystem/02-async-runtimes/).
- **Lean on the type system for validation.** Extractors reject malformed input before your code runs; pair them with the `validator` crate or a hand-written check rather than re-implementing Zod by hand.
- **Add observability early.** Wrap your app in a tracing layer from day one — it is one line and pays for itself. See the real-world example below and [Tracing](/23-ecosystem/04-tracing/).

---

## Real-World Example

A production JSON service needs three things beyond "return some JSON": typed error responses, shared application state, and request logging. This Axum service has all three: a domain `ApiError` that implements `IntoResponse` (the way an Express error-handling middleware maps errors to status codes), shared read-only state behind `Arc`, and a `tower-http` `TraceLayer` for structured request logs.

```rust
// Cargo.toml:
//   cargo add axum tokio --features tokio/full
//   cargo add serde --features derive
//   cargo add serde_json
//   cargo add tower-http --features trace
//   cargo add tracing tracing-subscriber
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

#[derive(Clone, Serialize)]
struct User {
    id: u64,
    name: String,
}

// A domain error type. `IntoResponse` is how Axum turns it into an HTTP reply —
// the typed equivalent of an Express error-handling middleware.
enum ApiError {
    NotFound,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "user not found"),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

#[derive(Clone)]
struct AppState {
    users: Arc<Vec<User>>,
}

// Returning Result<_, ApiError> lets the `?`-free path stay clean; the error
// arm is converted to a 404 JSON body automatically.
async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<User>, ApiError> {
    state
        .users
        .iter()
        .find(|u| u.id == id)
        .cloned()
        .map(Json)
        .ok_or(ApiError::NotFound)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .init();

    let state = AppState {
        users: Arc::new(vec![
            User { id: 1, name: "Ada".into() },
            User { id: 2, name: "Linus".into() },
        ]),
    };

    let app = Router::new()
        .route("/users/{id}", get(get_user))
        .layer(TraceLayer::new_for_http()) // request-logging middleware (a Tower layer)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3002").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Exercising it with `curl`:

```text
$ curl -s localhost:3002/users/1
{"id":1,"name":"Ada"}

$ curl -s localhost:3002/users/99
{"error":"user not found"}

$ curl -s -o /dev/null -w "%{http_code}\n" localhost:3002/users/99
404
```

And the structured request logs the `TraceLayer` emits to the server's stdout (real output, colors stripped):

```text
listening on http://127.0.0.1:3002
2026-06-02T06:34:14.079161Z DEBUG request{method=GET uri=/users/1 version=HTTP/1.1}: started processing request
2026-06-02T06:34:14.079234Z DEBUG request{method=GET uri=/users/1 version=HTTP/1.1}: finished processing request latency=0 ms status=200
2026-06-02T06:34:14.089943Z DEBUG request{method=GET uri=/users/99 version=HTTP/1.1}: started processing request
2026-06-02T06:34:14.090021Z DEBUG request{method=GET uri=/users/99 version=HTTP/1.1}: finished processing request latency=0 ms status=404
```

Each request gets its own span (`request{method=... uri=... version=...}`) with the method, URI, latency, and status: the kind of structured, correlatable logging you'd configure `pino` or `winston` for in Node, here available as a single `.layer(TraceLayer::new_for_http())`. The `IntoResponse` impl, the `Arc`-shared state, and the Tower layer are the three patterns that scale this toy service up to a real one. The full treatment of each lives in [Section 16: Web APIs](/16-web-apis/).

---

## Further Reading

- [Axum documentation](https://docs.rs/axum/latest/axum/): the framework this guide treats as the default.
- [Actix Web guide](https://actix.rs/): official docs and examples.
- [Rocket guide](https://rocket.rs/guide/): the ergonomic, decorator-style framework.
- [Poem documentation](https://docs.rs/poem/latest/poem/) and [`poem-openapi`](https://docs.rs/poem-openapi/latest/poem_openapi/): modern framework with first-class OpenAPI.
- [Tower](https://docs.rs/tower/latest/tower/) and [`tower-http`](https://docs.rs/tower-http/latest/tower_http/): the middleware ecosystem Axum and Poem plug into.

### Related sections in this guide

- [Section 16: Web APIs](/16-web-apis/) — the full hands-on Axum guide (routing, extractors, middleware, state, auth, WebSockets, deployment).
- [Framework Comparison](/16-web-apis/00-framework-comparison/) — a build-oriented Axum vs Actix vs Rocket comparison.
- [Async Runtimes](/23-ecosystem/02-async-runtimes/) — Tokio vs the alternatives, and why nearly every framework targets Tokio.
- [Popular Crates](/23-ecosystem/00-popular-crates/) — the wider set of crates (serde, tokio, reqwest, ...) and the npm packages they replace.
- [HTTP Clients](/23-ecosystem/06-http-clients/) — the client side: `reqwest` as your axios/fetch.
- [Tracing](/23-ecosystem/04-tracing/) and [Logging](/23-ecosystem/03-logging/) — the observability layers shown in the real-world example.
- [Section 11: Async Programming](/11-async/) — the `async`/`await` and futures model these frameworks are built on.
- [Section 24: Tooling](/24-tooling/) — formatting, linting, and CI for your web service.

---

## Exercises

### Exercise 1: Add a health-check route

**Difficulty:** Beginner

**Objective:** Get comfortable adding a route and returning JSON in Axum.

**Instructions:** Starting from the Axum example, add a `GET /health` route whose handler returns `Json` of `{ "status": "ok" }`. Use `serde_json::json!` so you don't need a new struct (`cargo add serde_json`). Run it and confirm `curl localhost:8080/health` returns the JSON.

<details>
<summary>Solution</summary>

```rust
// Cargo.toml:
//   cargo add axum tokio --features tokio/full
//   cargo add serde_json
use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/health", get(health));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Real output of `curl -s localhost:8080/health`:

```text
{"status":"ok"}
```

`serde_json::Value` is the dynamic JSON type — the Rust equivalent of an untyped JS object — and `json!` builds one inline. For a real endpoint you'd usually define a typed struct, but for a fixed health payload this is idiomatic.

</details>

### Exercise 2: Map a domain error to a 400

**Difficulty:** Intermediate

**Objective:** Practice the `IntoResponse` pattern that turns a Rust error into an HTTP status.

**Instructions:** Write an Axum handler `GET /divide/{a}/{b}` that parses two `i64` path params and returns `Json({ "result": a / b })`. Define an `ApiError::DivideByZero` variant, implement `IntoResponse` so it returns `400 Bad Request` with a JSON `{ "error": "division by zero" }`, and return it when `b == 0`. Confirm `curl localhost:8080/divide/10/2` gives `5` and `curl localhost:8080/divide/10/0` gives a 400.

<details>
<summary>Solution</summary>

```rust
// Cargo.toml:
//   cargo add axum tokio --features tokio/full
//   cargo add serde_json
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};

enum ApiError {
    DivideByZero,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::DivideByZero => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "division by zero" })),
            )
                .into_response(),
        }
    }
}

async fn divide(Path((a, b)): Path<(i64, i64)>) -> Result<Json<Value>, ApiError> {
    if b == 0 {
        return Err(ApiError::DivideByZero);
    }
    Ok(Json(json!({ "result": a / b })))
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/divide/{a}/{b}", get(divide));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Real output:

```text
$ curl -s localhost:8080/divide/10/2
{"result":5}

$ curl -s localhost:8080/divide/10/0
{"error":"division by zero"}

$ curl -s -o /dev/null -w "%{http_code}\n" localhost:8080/divide/10/0
400
```

Two path params are extracted as a tuple, `Path<(i64, i64)>`. The `Err` arm flows through `IntoResponse` to become a 400: no manual status-code plumbing in the happy path.

</details>

### Exercise 3: Compare two frameworks on the same endpoint

**Difficulty:** Advanced

**Objective:** Build the same route in two different frameworks and articulate the ergonomic difference.

**Instructions:** Implement `GET /sum?a=..&b=..` that reads two query parameters `a` and `b` as `i64` and returns `Json({ "sum": a + b })` in **both** Axum and Actix Web. Use Axum's `Query` extractor and Actix's `web::Query`. Then write two or three sentences on which routing/extraction style you found clearer and why.

<details>
<summary>Solution</summary>

Axum:

```rust
// Cargo.toml:
//   cargo add axum tokio --features tokio/full
//   cargo add serde --features derive
//   cargo add serde_json
use axum::{Json, Router, extract::Query, routing::get};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Pair {
    a: i64,
    b: i64,
}

async fn sum(Query(p): Query<Pair>) -> Json<Value> {
    Json(json!({ "sum": p.a + p.b }))
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/sum", get(sum));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Actix Web:

```rust
// Cargo.toml:
//   cargo add actix-web
//   cargo add serde --features derive
//   cargo add serde_json
use actix_web::{App, HttpServer, Responder, get, web};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Pair {
    a: i64,
    b: i64,
}

#[get("/sum")]
async fn sum(q: web::Query<Pair>) -> impl Responder {
    web::Json(json!({ "sum": q.a + q.b }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("listening on http://127.0.0.1:8080");
    HttpServer::new(|| App::new().service(sum))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
```

Real output (identical for both, on the Axum build shown):

```text
$ curl -s 'localhost:8080/sum?a=2&b=40'
{"sum":42}
```

**Observation:** Both use a `Deserialize` struct as the query schema, so the *parsing* story is the same. The visible difference is route declaration: Axum keeps the path in the builder (`route("/sum", get(sum))`) while Actix attaches it to the function (`#[get("/sum")]`). Axum's destructuring `Query(p)` reads slightly cleaner than Actix's wrapper `q` that you deref through, but both are a long way from Express's hand-validated `Number(req.query.a)`. Which you prefer is the same Express-vs-NestJS taste call you already have opinions about.

</details>
