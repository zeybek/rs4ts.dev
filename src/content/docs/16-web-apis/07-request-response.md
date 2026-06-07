---
title: "Request and Response Handling"
description: "Express mutates a res object; an Axum handler returns a value whose type implements IntoResponse. Master status codes, headers, and the (StatusCode, Json) tuple."
---

## Quick Overview

In Express you build a response by mutating a `res` object: `res.status(201).set('X-Foo', 'bar').json(data)`. In Axum a handler **returns a value**, and any value whose type implements the `IntoResponse` trait becomes the HTTP response. This page is about the output side of a handler: how `IntoResponse` works, how to set status codes and headers, and the `(StatusCode, Json)` tuple idiom that does most of the day-to-day work. Getting this model right is what lets you stop reaching for a `res` object that does not exist.

The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. This page targets **axum 0.8** (0.8.9), started with `axum::serve` and a `tokio::net::TcpListener`.

> **Note:** Inputs (extractors like `Path`, `Query`, `Json` as a *request* body) are covered in [Extractors](/16-web-apis/04-extractors/). This page is the mirror image: it covers `Json`, `StatusCode`, tuples, and `HeaderMap` as *responses*.

---

## TypeScript/JavaScript Example

A realistic Express handler set: a JSON read, a `201 Created` with a `Location` header, a `404` with a JSON error body, a redirect, and a CSV download. Notice how every response is built by *calling methods on `res`*.

```typescript
// reports.ts — Express 4/5
import express, { Request, Response } from "express";

const app = express();
app.use(express.json());

interface Report {
  id: number;
  title: string;
  body: string;
}

const reports = new Map<number, Report>();
let nextId = 0;

// 200 with a JSON body
app.get("/reports/:id", (req: Request, res: Response) => {
  const report = reports.get(Number(req.params.id));
  if (!report) {
    // 404 with a structured JSON error
    return res.status(404).json({ error: "report not found", code: "not_found" });
  }
  res.json(report); // 200, Content-Type: application/json
});

// 201 with a Location header
app.post("/reports", (req: Request, res: Response) => {
  const { title, body } = req.body as { title: string; body: string };
  const report: Report = { id: ++nextId, title, body };
  reports.set(report.id, report);
  res
    .status(201)
    .set("Location", `/reports/${report.id}`)
    .json(report);
});

// A 303 redirect
app.get("/go", (_req: Request, res: Response) => {
  res.redirect("/reports/1");
});

// A non-JSON response: a CSV download
app.get("/reports.csv", (_req: Request, res: Response) => {
  const rows = ["id,title", ...[...reports.values()].map((r) => `${r.id},${r.title}`)];
  res
    .status(200)
    .set("Content-Type", "text/csv; charset=utf-8")
    .set("Content-Disposition", 'attachment; filename="reports.csv"')
    .send(rows.join("\n") + "\n");
});

app.listen(3000);
```

Everything funnels through `res`: `res.status(n)` sets the code, `res.set(k, v)` adds a header, `res.json(x)` / `res.send(x)` writes the body and ends the response. The body type is whatever you pass; Express does not check it. If you forget to call a `res` method, the request hangs.

---

## Rust Equivalent

The same five responses in Axum. There is no `res` object; each handler **returns** the response it wants.

```rust
use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Serialize)]
struct Report {
    id: u64,
    title: String,
    body: String,
}

#[derive(Deserialize)]
struct NewReport {
    title: String,
    body: String,
}

#[derive(Serialize)]
struct ApiError {
    error: &'static str,
    code: &'static str,
}

#[derive(Clone, Default)]
struct AppState {
    reports: Arc<Mutex<Vec<Report>>>,
    next_id: Arc<Mutex<u64>>,
}

// 200 with JSON, or 404 with a JSON error. The two arms have DIFFERENT response
// shapes, so we return `Response` and call `.into_response()` on each.
async fn get_report(State(s): State<AppState>, Path(id): Path<u64>) -> Response {
    let reports = s.reports.lock().unwrap();
    match reports.iter().find(|r| r.id == id) {
        Some(r) => Json(r.clone()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiError { error: "report not found", code: "not_found" }),
        )
            .into_response(),
    }
}

// 201 Created + a Location header + the created body.
async fn create_report(
    State(s): State<AppState>,
    Json(body): Json<NewReport>,
) -> impl IntoResponse {
    let mut id = s.next_id.lock().unwrap();
    *id += 1;
    let report = Report { id: *id, title: body.title, body: body.body };
    s.reports.lock().unwrap().push(report.clone());

    let location = HeaderValue::from_str(&format!("/reports/{}", report.id))
        .expect("a numeric id is always a valid header value");

    (StatusCode::CREATED, [(header::LOCATION, location)], Json(report))
}

// A 303 redirect — `Redirect` implements IntoResponse.
async fn go() -> Redirect {
    Redirect::to("/reports/1")
}

// A non-JSON body: a CSV download with two headers.
async fn export_csv(State(s): State<AppState>) -> impl IntoResponse {
    let reports = s.reports.lock().unwrap();
    let mut csv = String::from("id,title\n");
    for r in reports.iter() {
        csv.push_str(&format!("{},{}\n", r.id, r.title));
    }
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"reports.csv\""),
        ],
        csv,
    )
}

fn app() -> Router {
    Router::new()
        .route("/reports", post(create_report))
        .route("/reports/{id}", get(get_report))
        .route("/reports.csv", get(export_csv))
        .route("/go", get(go))
        .with_state(AppState::default())
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

The dependencies (in a fresh `cargo new` project, `cargo add` resolves the current versions):

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

Exercising it with `curl -i` produces these **real** responses (captured against the compiled server):

```text
$ curl -s -i -X POST http://127.0.0.1:3000/reports \
       -H 'content-type: application/json' -d '{"title":"Q1","body":"numbers"}'
HTTP/1.1 201 Created
content-type: application/json
location: /reports/1
content-length: 38
date: Mon, 01 Jun 2026 11:55:26 GMT

{"id":1,"title":"Q1","body":"numbers"}

$ curl -s -i http://127.0.0.1:3000/reports/1
HTTP/1.1 200 OK
content-type: application/json
content-length: 38

{"id":1,"title":"Q1","body":"numbers"}

$ curl -s -i http://127.0.0.1:3000/reports/999
HTTP/1.1 404 Not Found
content-type: application/json
content-length: 47

{"error":"report not found","code":"not_found"}

$ curl -s -i http://127.0.0.1:3000/reports.csv
HTTP/1.1 200 OK
content-type: text/csv; charset=utf-8
content-disposition: attachment; filename="reports.csv"
content-length: 14

id,title
1,Q1
```

---

## Detailed Explanation

### `IntoResponse`: the one trait that defines "what a handler may return"

A handler's return type must implement `IntoResponse`. That is the entire contract on the output side. The same way `Handler` requires the *parameters* to be extractors, it requires the *return type* to be `IntoResponse`. The trait has a single method:

```rust
use axum::response::Response;

// (signature, from the axum source — for understanding only)
trait IntoResponse {
    fn into_response(self) -> Response;
}
```

`Response` is `axum::response::Response`, an alias for `http::Response<Body>`. You rarely build one by hand; instead you return something Axum already knows how to convert, and the framework calls `.into_response()` for you. Axum ships `IntoResponse` impls for a large catalogue of types:

| You return | Becomes |
| --- | --- |
| `&'static str`, `String` | `200 OK`, `content-type: text/plain; charset=utf-8` |
| `Json<T>` (where `T: Serialize`) | `200 OK`, `content-type: application/json` |
| `StatusCode` | that status, **empty body** |
| `()` (the unit type) | `200 OK`, empty body |
| `(StatusCode, T)` | `T`'s response, but with that status |
| `(StatusCode, headers, T)` | that status, those headers, plus `T`'s body |
| `Redirect` | a `3xx` with a `Location` header |
| `Result<T, E>` (`T`, `E` both `IntoResponse`) | the `Ok` response or the `Err` response |
| `Response` | itself (the identity impl) |

This is the deep contrast with Express: there, the *body content* is dynamic and the *status/headers* are mutations on a fixed `res` object. In Axum the **type you return encodes the response shape**, and the compiler checks it. A handler `-> Json<Report>` cannot accidentally send plain text.

### `-> impl IntoResponse` vs a concrete type

```rust
async fn create_report(/* ... */) -> impl IntoResponse { /* ... */ }
async fn get_report(/* ... */) -> Response { /* ... */ }
```

`-> impl IntoResponse` means "I return *some* type that implements `IntoResponse`; don't make me name it." It is convenient when the type is an ugly tuple like `(StatusCode, [(HeaderName, HeaderValue); 1], Json<Report>)`. But `impl Trait` still demands **one single concrete type** across the whole function body. If different branches return different concrete types, it will not compile (see Common Pitfalls). When that happens, return `Response` and call `.into_response()` on each branch: that erases the differences into one type.

> **Tip:** Rule of thumb: a handler that always returns the *same* shape can use `-> Json<T>` or `-> impl IntoResponse`. A handler that returns *different* shapes per branch should use `-> Response` (or a `Result`, or a custom enum, see [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/)).

### Status codes

`StatusCode` comes from the `http` crate, re-exported as `axum::http::StatusCode`. It has named associated constants: `StatusCode::OK` (200), `StatusCode::CREATED` (201), `StatusCode::NO_CONTENT` (204), `StatusCode::NOT_FOUND` (404), `StatusCode::UNPROCESSABLE_ENTITY` (422), and so on. Returned alone it produces an **empty-bodied** response with that status:

```rust
use axum::http::StatusCode;

// 204 No Content — empty body, like Express's res.sendStatus(204).
async fn delete_thing() -> StatusCode {
    StatusCode::NO_CONTENT
}
```

```text
$ curl -s -i http://127.0.0.1:3000/no-content
HTTP/1.1 204 No Content
date: Mon, 01 Jun 2026 11:53:19 GMT

```

To attach a body, pair the status with something in a tuple.

### The `(StatusCode, Json)` tuple — the workhorse

This is the idiom you will type most. A 2-tuple `(StatusCode, T)` produces `T`'s response with the status replaced:

```rust
use axum::{http::StatusCode, Json};
use serde::Serialize;

#[derive(Serialize)]
struct Article {
    id: u64,
    title: String,
}

async fn created() -> (StatusCode, Json<Article>) {
    (StatusCode::CREATED, Json(Article { id: 7, title: "New".to_string() }))
}
```

```text
$ curl -s -i http://127.0.0.1:3000/created
HTTP/1.1 201 Created
content-type: application/json
content-length: 22

{"id":7,"title":"New"}
```

`Json` already supplied `content-type: application/json` and the serialized body; the tuple only overrode the status from `200` to `201`. **Order matters**: the status code must come *first*. The body-producing part (`Json`, `String`, etc.) must be *last*. We will see what happens when you flip them in Common Pitfalls.

### Setting headers

A 3-tuple `(StatusCode, headers, body)` lets you add response headers. The "headers" slot can be an **array of `(HeaderName, value)` pairs**, where the value is anything convertible into a `HeaderValue` (a `&'static str`, a `String`, or a `HeaderValue`):

```rust
use axum::{http::{header, HeaderValue, StatusCode}, Json};
use serde::Serialize;

#[derive(Serialize)]
struct Article { id: u64, title: String }

async fn with_header() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, HeaderValue::from_static("max-age=60"))],
        Json(Article { id: 2, title: "Cached".to_string() }),
    )
}
```

```text
$ curl -s -i http://127.0.0.1:3000/with-header
HTTP/1.1 200 OK
content-type: application/json
cache-control: max-age=60
content-length: 25

{"id":2,"title":"Cached"}
```

The `header` module (`axum::http::header`) holds constants for standard header names: `header::CACHE_CONTROL`, `header::LOCATION`, `header::CONTENT_TYPE`, `header::ETAG`, etc. For a custom header, use `HeaderName::from_static("x-trace")`. For dynamic header *values*, use `HeaderValue::from_str(&s)`, which returns a `Result` because not every string is a legal header value (control characters, for instance, are rejected: a built-in defense against header injection).

When you need many headers, or to *append* multiple values for the same name, use `AppendHeaders`:

```rust
use axum::{http::{header, HeaderName}, response::{AppendHeaders, IntoResponse}};

async fn multi_header() -> impl IntoResponse {
    (
        AppendHeaders([
            (header::CACHE_CONTROL, "no-store"),
            (HeaderName::from_static("x-trace"), "abc"),
        ]),
        "with two headers",
    )
}
```

```text
$ curl -s -i http://127.0.0.1:3000/multi-header
HTTP/1.1 200 OK
content-type: text/plain; charset=utf-8
cache-control: no-store
x-trace: abc
content-length: 16

with two headers
```

You can also return a `HeaderMap` directly as part of a tuple when you build the set imperatively:

```rust
use axum::http::{HeaderMap, HeaderValue};

async fn header_map() -> (HeaderMap, &'static str) {
    let mut headers = HeaderMap::new();
    headers.insert("x-custom", HeaderValue::from_static("hi"));
    (headers, "body")
}
```

### `Redirect`

`axum::response::Redirect` builds a redirect response with the `Location` header set. `Redirect::to(uri)` is a `303 See Other`; there are also `Redirect::permanent` (`308`) and `Redirect::temporary` (`307`):

```rust
use axum::response::Redirect;

async fn go() -> Redirect {
    Redirect::to("/reports/1")
}
```

```text
$ curl -s -i http://127.0.0.1:3000/go
HTTP/1.1 303 See Other
location: /reports/1
content-length: 0

```

### Building a `Response` by hand

For full control you can construct a `Response` with the builder from the `http` crate. This is the closest analogue to mutating `res`, and you rarely need it, but it is there:

```rust
use axum::{http::{header, StatusCode}, response::{IntoResponse, Response}};

async fn manual() -> Response {
    Response::builder()
        .status(StatusCode::IM_A_TEAPOT)
        .header(header::CONTENT_TYPE, "text/plain")
        .body("I'm a teapot".to_string())
        .unwrap()
        .into_response()
}
```

```text
$ curl -s -i http://127.0.0.1:3000/manual
HTTP/1.1 418 I'm a teapot
content-type: text/plain
content-length: 12

I'm a teapot
```

### Writing your own `IntoResponse`

Because `IntoResponse` is just a trait, you can implement it for your own types. This is how you give a domain type — a CSV export, an error enum, a custom envelope — a single, reusable response shape:

```rust
use axum::{http::{header, StatusCode}, response::{IntoResponse, Response}};

struct Csv(String);

impl IntoResponse for Csv {
    fn into_response(self) -> Response {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/csv; charset=utf-8")],
            self.0,
        )
            .into_response()
    }
}

async fn export() -> Csv {
    Csv("id,title\n1,Hello\n".to_string())
}
```

```text
$ curl -s -i http://127.0.0.1:3000/export
HTTP/1.1 200 OK
content-type: text/csv; charset=utf-8
content-length: 17

id,title
1,Hello
```

Implementing `IntoResponse` for an error type is the foundation of centralized error handling; see [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/).

---

## Key Differences

| Concern | Express.js | Axum (0.8) |
| --- | --- | --- |
| How a response is produced | mutate `res` (`res.status().json()`) | **return** a value implementing `IntoResponse` |
| Status code | `res.status(201)` | first element of a tuple, or a bare `StatusCode` |
| JSON body | `res.json(obj)` | return `Json(value)` (any `T: Serialize`) |
| Headers | `res.set(k, v)` | `(status, [(name, value)], body)` tuple, or `HeaderMap` |
| Redirect | `res.redirect(url)` | return `Redirect::to(url)` |
| Empty body with status | `res.sendStatus(204)` | return `StatusCode::NO_CONTENT` |
| Error response | `res.status(404).json(...)` | return `Err(e)` where `e: IntoResponse`, or `(StatusCode, Json(..))` |
| Body type checking | none — `res.send(anything)` | type-checked; body must be a known `IntoResponse` |
| Forgetting to respond | request hangs | impossible — the function must return a value |

The single most important shift: **a handler is a function from request to response, not a procedure that pokes at a mutable response object.** Because the return type is checked, you cannot ship a handler that forgets to send a body, sends two conflicting bodies, or — short of `unwrap` panics — leaves a request hanging. The "shape" of every response is visible in the function signature.

> **Note:** `Json` does double duty. As a *parameter* (`Json<T>` where `T: Deserialize`) it is a request-body extractor; as a *return value* (`Json<T>` where `T: Serialize`) it is a response. Same wrapper, opposite direction. The extractor side lives in [Extractors](/16-web-apis/04-extractors/).

---

## Common Pitfalls

### 1. Putting the body before the status in a tuple

The status code must be the **first** element of a response tuple and the body-producing type must be **last**. Flip them and it does not compile:

```rust
use axum::{http::StatusCode, routing::get, Json, Router};
use serde::Serialize;

#[derive(Serialize)]
struct Article { id: u64 }

// does not compile: Json must be the LAST element, not the first
#[axum::debug_handler]
async fn bad() -> (Json<Article>, StatusCode) {
    (Json(Article { id: 1 }), StatusCode::CREATED)
}

fn build() -> Router {
    Router::new().route("/bad", get(bad))
}
```

Without `#[axum::debug_handler]` you get the usual opaque `the trait bound ... Handler<_, _> is not implemented`. With it (enable the `macros` feature: `cargo add axum --features macros`), the real message is precise:

```text
error: `Json<_>` must be the last element in a response tuple
  --> src/main.rs:11:20
   |
11 | async fn bad() -> (Json<Article>, StatusCode) {
   |                    ^^^^^^^^^^^^^
```

The fix is to swap them: `(StatusCode::CREATED, Json(Article { id: 1 }))`.

### 2. `impl IntoResponse` with branches that return different types

`-> impl IntoResponse` returns *one* concrete type. If your branches return different concrete types, the error is the plain Rust `if`/`else` mismatch, not an axum-specific message:

```rust
use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

#[derive(Serialize)]
struct Item { id: u64 }

// does not compile (error[E0308]): the two branches are different types
async fn handler(ok: bool) -> impl IntoResponse {
    if ok {
        Json(Item { id: 1 })
    } else {
        StatusCode::NOT_FOUND
    }
}
```

The real error from `cargo check`:

```text
error[E0308]: `if` and `else` have incompatible types
  --> src/main.rs:11:9
   |
8  | /     if ok {
9  | |         Json(Item { id: 1 })
   | |         -------------------- expected because of this
10 | |     } else {
11 | |         StatusCode::NOT_FOUND
   | |         ^^^^^^^^^^^^^^^^^^^^^ expected `Json<Item>`, found `StatusCode`
12 | |     }
   | |_____- `if` and `else` have incompatible types
```

The fix is to unify the types by erasing them to `Response`:

```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;

#[derive(Serialize)]
struct Item { id: u64 }

async fn handler(ok: bool) -> Response {
    if ok {
        Json(Item { id: 1 }).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
```

### 3. A header you set is silently overridden by a later part

The parts of a response tuple are applied left to right, but the **header array runs *after* the body's own headers**, so it can clobber them. If you set `content-type: text/plain` in a tuple *and* include a `Json` body, the explicit header wins and you end up with JSON bytes labelled `text/plain`:

```rust
use axum::{http::{header, HeaderValue, StatusCode}, response::IntoResponse, Json};
use serde::Serialize;

#[derive(Serialize)]
struct X { a: u8 }

// content-type ends up "text/plain" even though the body is JSON — surprising!
async fn confused() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"))],
        Json(X { a: 1 }),
    )
}
```

Checking the produced response confirms it: the tuple header overrode `Json`'s `application/json`.

```text
headers-then-Json content-type = Some("text/plain")
Json-only content-type   = Some("application/json")
```

Do not set `content-type` by hand alongside a typed body; let `Json` (or `String`) set it. Reserve explicit `content-type` for raw bodies (`String`/`Vec<u8>`) and custom `IntoResponse` impls.

### 4. Building a `HeaderValue` from arbitrary, possibly-invalid input

`HeaderValue::from_static` only accepts a `&'static str` and *panics* on an illegal value, so it is for literals you control. For dynamic strings (a path, a user-derived token) use `HeaderValue::from_str`, which returns a `Result`:

```rust
use axum::http::HeaderValue;

fn check() {
    // Illegal: a value with a newline (header-injection attempt) is rejected.
    let bad = HeaderValue::from_str("ok\ninjected");
    println!("is_err = {}", bad.is_err()); // is_err = true

    let good = HeaderValue::from_str("fine");
    println!("is_ok = {}", good.is_ok()); // is_ok = true
}
```

Handle the `Err` rather than `.unwrap()`-ing it on untrusted input, or you turn a malformed header into a `500`.

### 5. Reaching for a `res`-style mutable object

There is no `res`. New TypeScript-to-Rust developers sometimes look for a `&mut Response` parameter to mutate. The model is different and simpler: assemble the whole response as a value and return it. If you find yourself wanting to "set a header partway through," build the pieces into local variables and combine them in the `return` expression, or implement `IntoResponse` for a type that carries them.

---

## Best Practices

- **Default to the `(StatusCode, Json<T>)` tuple.** It is the clearest, most common response and reads almost like `res.status(n).json(x)`. Reach for fancier shapes only when you need headers or branching.
- **Use `-> Json<T>` or `-> impl IntoResponse` for single-shape handlers; use `-> Response` (or `Result`/an enum) when branches differ.** Matching the return type to the situation keeps the compiler on your side and the signature honest.
- **Let typed bodies own their `content-type`.** `Json` sets `application/json`, `String`/`&str` set `text/plain`. Only set `content-type` manually for raw bytes or custom formats — and never alongside `Json` (pitfall 3).
- **Prefer `header::*` constants and `HeaderName::from_static` over stringly-typed names.** Typos in a string header name compile fine and fail silently; the constants do not.
- **Use `HeaderValue::from_str` for dynamic values and handle the error.** Reserve `from_static` for compile-time literals.
- **Implement `IntoResponse` for your domain/error types.** A `Csv`, an `ApiError`, a `Created<T>` envelope: one impl, reused everywhere, keeps handlers tiny. This is the on-ramp to [centralized error handling](/16-web-apis/10-error-handling-web/).
- **Return `StatusCode::NO_CONTENT` for successful deletes and other body-less successes** instead of a `200` with an empty JSON object.

---

## Real-World Example

A production-flavored pattern: a single `ApiResponse` enum with one `IntoResponse` impl that centralizes the status code, headers, and body for every kind of success and failure a resource can produce. Handlers just return the right variant; the response shaping lives in one place. Every line is compile-verified against axum 0.8.

```rust
use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Serialize)]
struct Report {
    id: u64,
    title: String,
    body: String,
}

#[derive(Deserialize)]
struct NewReport {
    title: String,
    body: String,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
    code: &'static str,
}

// One enum, one IntoResponse impl. Each variant fully specifies its response:
// status code, headers, and body. Handlers never touch StatusCode directly.
enum ApiResponse {
    Report(Report),
    Created(Report),
    NotFound,
    Csv(String),
}

impl IntoResponse for ApiResponse {
    fn into_response(self) -> Response {
        match self {
            ApiResponse::Report(r) => (StatusCode::OK, Json(r)).into_response(),
            ApiResponse::Created(r) => {
                let location = HeaderValue::from_str(&format!("/reports/{}", r.id))
                    .unwrap_or_else(|_| HeaderValue::from_static("/reports"));
                (StatusCode::CREATED, [(header::LOCATION, location)], Json(r)).into_response()
            }
            ApiResponse::NotFound => (
                StatusCode::NOT_FOUND,
                Json(ApiError { error: "report not found".to_string(), code: "not_found" }),
            )
                .into_response(),
            ApiResponse::Csv(body) => (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8")),
                    (
                        header::CONTENT_DISPOSITION,
                        HeaderValue::from_static("attachment; filename=\"reports.csv\""),
                    ),
                ],
                body,
            )
                .into_response(),
        }
    }
}

#[derive(Clone, Default)]
struct AppState {
    reports: Arc<Mutex<Vec<Report>>>,
    next_id: Arc<Mutex<u64>>,
}

async fn get_report(State(s): State<AppState>, Path(id): Path<u64>) -> ApiResponse {
    let reports = s.reports.lock().unwrap();
    match reports.iter().find(|r| r.id == id) {
        Some(r) => ApiResponse::Report(r.clone()),
        None => ApiResponse::NotFound,
    }
}

async fn create_report(State(s): State<AppState>, Json(body): Json<NewReport>) -> ApiResponse {
    let mut id = s.next_id.lock().unwrap();
    *id += 1;
    let report = Report { id: *id, title: body.title, body: body.body };
    s.reports.lock().unwrap().push(report.clone());
    ApiResponse::Created(report)
}

async fn export_csv(State(s): State<AppState>) -> ApiResponse {
    let reports = s.reports.lock().unwrap();
    let mut csv = String::from("id,title\n");
    for r in reports.iter() {
        csv.push_str(&format!("{},{}\n", r.id, r.title));
    }
    ApiResponse::Csv(csv)
}

fn app() -> Router {
    Router::new()
        .route("/reports", post(create_report))
        .route("/reports/{id}", get(get_report))
        .route("/reports.csv", get(export_csv))
        .with_state(AppState::default())
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

Exercising it produces these **real** responses:

```text
$ curl -s -i -X POST http://127.0.0.1:3000/reports \
       -H 'content-type: application/json' -d '{"title":"Q1","body":"numbers"}'
HTTP/1.1 201 Created
content-type: application/json
location: /reports/1
content-length: 38

{"id":1,"title":"Q1","body":"numbers"}

$ curl -s -i http://127.0.0.1:3000/reports/999
HTTP/1.1 404 Not Found
content-type: application/json
content-length: 47

{"error":"report not found","code":"not_found"}

$ curl -s -i http://127.0.0.1:3000/reports.csv
HTTP/1.1 200 OK
content-type: text/csv; charset=utf-8
content-disposition: attachment; filename="reports.csv"
content-length: 14

id,title
1,Q1
```

Why this pattern scales: the status-code/header/body decisions for the whole resource live in *one* `match`, so they cannot drift apart between handlers. Handlers stay declarative ("this is a `Created`, this is a `NotFound`"), and adding a new response kind is one new enum variant plus one new match arm. For *error* types specifically, the same technique — implementing `IntoResponse` for an error enum, often with [`thiserror`](/08-error-handling/) — is covered in depth in [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/).

---

## Further Reading

- [`axum::response` module docs](https://docs.rs/axum/latest/axum/response/index.html): `IntoResponse`, `Response`, `Redirect`, `AppendHeaders`, and the full list of impls.
- [`IntoResponse` trait reference](https://docs.rs/axum/latest/axum/response/trait.IntoResponse.html): the canonical table of what converts into a response.
- [`axum::Json`](https://docs.rs/axum/latest/axum/struct.Json.html): the JSON wrapper as both extractor and response.
- [`http::StatusCode`](https://docs.rs/http/latest/http/status/struct.StatusCode.html) and [`http::header`](https://docs.rs/http/latest/http/header/index.html): status and header-name constants.

Within this guide:

- [Axum Fundamentals](/16-web-apis/01-axum-basics/) — the handler loop, `Router`, `axum::serve`; where this page's return values plug in.
- [Extractors](/16-web-apis/04-extractors/) — the input mirror image: `Json`/`Path`/`Query` as request data.
- [JSON APIs](/16-web-apis/08-json-apis/): a full CRUD resource built on these response shapes.
- [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/): a custom `AppError: IntoResponse` mapping errors to status codes.
- [Routing](/16-web-apis/03-routing/): `{id}` path syntax and method routing for the routes these handlers serve.
- [State Management](/16-web-apis/06-state-management/): the `State<T>` + `Arc` the Real-World Example relies on.
- [Middleware](/16-web-apis/05-middleware/) — Tower layers that wrap or rewrite responses after a handler returns.
- Foundations: [traits](/09-generics-traits/) (what `IntoResponse` *is*), [error handling](/08-error-handling/) (`Result` as a response), the language [basics](/02-basics/) and [getting started](/01-getting-started/).
- Persisting what these handlers return: [Database](/17-database/).

---

## Exercises

### Exercise 1: A cache-friendly health check

**Difficulty:** Easy

**Objective:** Return JSON with a status code and a custom header in one tuple.

**Instructions:**

1. Add a `GET /ping` route.
2. Return a `200 OK` whose body is the JSON `{"pong":true}` and which carries a `Cache-Control: no-cache` header.
3. Use a `(StatusCode, [...], Json<...>)` tuple. Define a `#[derive(Serialize)]` struct for the body.

<details>
<summary>Solution</summary>

```rust
use axum::{
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;

#[derive(Serialize)]
struct Pong {
    pong: bool,
}

async fn ping() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))],
        Json(Pong { pong: true }),
    )
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/ping", get(ping));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`GET /ping` returns `200 OK`, `cache-control: no-cache`, and the body `{"pong":true}`.

</details>

### Exercise 2: A reusable error type

**Difficulty:** Medium

**Objective:** Implement `IntoResponse` for an error enum so handlers can return `Result<_, AppError>` and get the right status plus a JSON error body automatically.

**Instructions:**

1. Define `enum AppError { NotFound(String), RateLimited }`.
2. Implement `IntoResponse` for it: `NotFound(what)` maps to `404` with body `{"error":"<what> not found"}`; `RateLimited` maps to `429 Too Many Requests` with body `{"error":"slow down"}`.
3. Write two handlers, `Err(AppError::NotFound("widget".into()))` and `Err(AppError::RateLimited)`, returning `Result<&'static str, AppError>`, and confirm the statuses.

<details>
<summary>Solution</summary>

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

enum AppError {
    NotFound(String),
    RateLimited,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound(what) => (StatusCode::NOT_FOUND, format!("{what} not found")),
            AppError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "slow down".to_string()),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

async fn missing() -> Result<&'static str, AppError> {
    Err(AppError::NotFound("widget".to_string()))
}

async fn limited() -> Result<&'static str, AppError> {
    Err(AppError::RateLimited)
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/missing", get(missing))
        .route("/limited", get(limited));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`GET /missing` returns `404` with `{"error":"widget not found"}`; `GET /limited` returns `429` with `{"error":"slow down"}`. Because `AppError: IntoResponse`, `Result<T, AppError>` is itself a valid handler return type — the `?` operator and `Err(..)` now produce well-formed HTTP errors. This is exactly the seed of [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/).

</details>

### Exercise 3: A conditional `304 Not Modified`

**Difficulty:** Hard

**Objective:** Branch on a request header and return *different response shapes* — the case where you must reach for `-> Response`.

**Instructions:**

1. Add a `GET /docs/{id}` route. Give the document a fixed ETag, e.g. `"v1"` (including the quotes, per the HTTP spec).
2. Read the `If-None-Match` request header (`HeaderMap` is an extractor).
3. If it equals the current ETag, return `304 Not Modified` with an empty body. Otherwise return `200 OK` with the document as JSON **and** an `ETag` response header.
4. Because the two branches differ, the handler must return `Response`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::Path,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;

#[derive(Serialize)]
struct Doc {
    id: u64,
    etag: &'static str,
}

async fn get_doc(Path(id): Path<u64>, headers: HeaderMap) -> Response {
    let current_etag = "\"v1\"";

    let matches = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == current_etag)
        .unwrap_or(false);

    if matches {
        return StatusCode::NOT_MODIFIED.into_response();
    }

    (
        StatusCode::OK,
        [(header::ETAG, current_etag)],
        Json(Doc { id, etag: current_etag }),
    )
        .into_response()
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/docs/{id}", get(get_doc));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`GET /docs/1` (no `If-None-Match`) returns `200` with `etag: "v1"` and body `{"id":1,"etag":"\"v1\""}`. `GET /docs/1 -H 'If-None-Match: "v1"'` returns `304 Not Modified` with an empty body. The two branches return different concrete types (`StatusCode` vs a 3-tuple), so each is `.into_response()`-ed to the common `Response` type — this is the canonical reason to write `-> Response` instead of `-> impl IntoResponse`.

</details>
