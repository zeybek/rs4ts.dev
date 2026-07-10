---
title: "Error Handling in Web Handlers"
description: "Instead of Express try/catch and four-arg error middleware, an Axum handler returns Result and one IntoResponse impl maps each error to a status."
---

In Express.js, an error usually means an `try/catch` that calls `res.status(500).json(...)`, or a thrown error that lands in a four-argument error middleware. Axum takes a different route: a handler simply *returns* `Result<T, AppError>`, and you teach the framework (once) how to turn `AppError` into an HTTP response. This chapter builds a production-grade error type with `thiserror`, wires it to Axum's `IntoResponse` trait, and maps every error variant to the right status code.

---

## Quick Overview

In a Rust web app you do not throw exceptions and you do not pepper handlers with `try/catch`. Instead you define **one application error type** (an `enum`), implement the `IntoResponse` trait for it so Axum knows the status code and JSON body to send, and let the `?` operator propagate failures out of handlers automatically. The payoff is that error handling becomes part of the type system: a handler's signature tells you exactly what can go wrong, and you cannot forget to handle an error path because the compiler will not let you.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. This page targets **axum 0.8**, **thiserror 2**, and **anyhow 1**.

---

## TypeScript/JavaScript Example

Here is the way most Express APIs handle errors: a custom error class carrying a status code, `try/catch` in async handlers, and a centralized error-handling middleware as a backstop.

```typescript
// errors.ts — a custom error class with an HTTP status
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
    public code: string,
  ) {
    super(message);
    this.name = "ApiError";
  }

  static notFound(what: string) {
    return new ApiError(404, `${what} was not found`, "not_found");
  }

  static validation(message: string) {
    return new ApiError(422, message, "validation");
  }

  static unauthorized() {
    return new ApiError(401, "you are not authorized to do that", "unauthorized");
  }
}
```

```typescript
// app.ts — handlers throw, middleware catches
import express, { Request, Response, NextFunction } from "express";
import { ApiError } from "./errors";

const app = express();
app.use(express.json());

const users = new Map<number, string>([[1, "Alice"]]);

app.get("/users/:id", (req: Request, res: Response, next: NextFunction) => {
  try {
    const id = Number(req.params.id);
    const name = users.get(id);
    if (name === undefined) {
      throw ApiError.notFound(`user ${id}`); // jumps to the error middleware
    }
    res.json({ id, name });
  } catch (err) {
    next(err); // hand off to the centralized handler
  }
});

// Centralized error-handling middleware: MUST have 4 args, registered LAST.
app.use((err: unknown, _req: Request, res: Response, _next: NextFunction) => {
  if (err instanceof ApiError) {
    res.status(err.status).json({ error: err.message, code: err.code });
  } else {
    console.error("unhandled error:", err); // log the real thing
    res.status(500).json({ error: "internal server error" }); // hide details
  }
});

app.listen(3000);
```

**Key points and pain points:**

- Every async handler needs `try/catch` + `next(err)`, or a thrown error silently hangs the request (in classic Express without `express-async-errors`).
- The error middleware is identified *only* by its arity (four parameters). Forget one and Express treats it as a normal middleware, a notorious footgun.
- Nothing in a handler's type signature tells you which errors it can produce; you discover them at runtime.
- It is easy to accidentally leak an internal error's `message` to the client.

---

## Rust Equivalent

Axum's model: a handler returns `Result<T, AppError>`, the `?` operator replaces `try/catch`, and one `impl IntoResponse for AppError` replaces the error middleware. Set up the dependencies first:

```bash
cargo new my-api
cd my-api
cargo add axum@0.8
cargo add tokio@1 --features full
cargo add serde --features derive
cargo add serde_json
cargo add thiserror@2
cargo add tracing
```

```rust
// src/main.rs — the same API in Axum, with a typed error
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

// The application's single error type. `thiserror` derives `Display` + `Error`
// from the `#[error("...")]` messages, so we never hand-write a match for them.
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("user {0} was not found")]
    NotFound(u64),

    #[error("invalid input: {0}")]
    Validation(String),

    #[error("you are not authorized to do that")]
    Unauthorized,

    // `#[from]` lets `?` turn a serde_json::Error into AppError automatically.
    #[error("failed to (de)serialize JSON")]
    Json(#[from] serde_json::Error),

    // A catch-all for unexpected failures. We never leak the inner message.
    #[error("internal server error")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

// The shape every error sends back to the client.
#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

// THIS is the bridge: it teaches Axum how to turn an AppError into an HTTP
// response. Each variant chooses its own status code.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Json(_) => StatusCode::BAD_REQUEST,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        // Log the *full* error server-side (5xx especially), but only send a
        // safe, public message to the client.
        if status.is_server_error() {
            tracing::error!(error = ?self, "request failed");
        }

        let body = Json(ErrorBody {
            error: self.to_string(),
        });
        (status, body).into_response()
    }
}

type Db = Arc<RwLock<HashMap<u64, String>>>;

// Handlers return `Result<T, AppError>`. `T: IntoResponse` for the success arm,
// `AppError: IntoResponse` for the failure arm — Axum responds to either.
async fn get_user(
    State(db): State<Db>,
    Path(id): Path<u64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let users = db.read().await;
    let name = users.get(&id).ok_or(AppError::NotFound(id))?;
    Ok(Json(json!({ "id": id, "name": name })))
}

async fn risky(Path(n): Path<i64>) -> Result<String, AppError> {
    if n < 0 {
        return Err(AppError::Validation("n must be >= 0".into()));
    }
    // `?` converts serde_json::Error into AppError::Json via the `#[from]` impl.
    let parsed: i64 = serde_json::from_str("not a number")?;
    Ok(parsed.to_string())
}

async fn secret() -> Result<String, AppError> {
    Err(AppError::Unauthorized)
}

#[tokio::main]
async fn main() {
    let db: Db = Arc::new(RwLock::new(HashMap::from([(1, "Alice".to_string())])));

    let app = Router::new()
        .route("/users/{id}", get(get_user))
        .route("/risky/{n}", get(risky))
        .route("/secret", get(secret))
        .with_state(db);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Running this server and hitting each route produces the following **real** responses (captured with `curl -w "\nHTTP %{http_code}\n"`):

```text
$ curl -s http://127.0.0.1:3000/users/1
{"id":1,"name":"Alice"}            # HTTP 200

$ curl -s http://127.0.0.1:3000/users/99
{"error":"user 99 was not found"}  # HTTP 404

$ curl -s http://127.0.0.1:3000/risky/-5
{"error":"invalid input: n must be >= 0"}   # HTTP 422

$ curl -s http://127.0.0.1:3000/risky/5
{"error":"failed to (de)serialize JSON"}    # HTTP 400

$ curl -s http://127.0.0.1:3000/secret
{"error":"you are not authorized to do that"}   # HTTP 401
```

One enum, one `IntoResponse` impl, and every handler stays clean. There is no error middleware to register in the right order, and no way to forget the error path: if `get_user` did not handle the missing-user case, the `?` on `ok_or(...)` simply would not type-check.

---

## Detailed Explanation

### `thiserror` derives the boilerplate

`thiserror` is the idiomatic crate for **library/application error enums**. The `#[derive(thiserror::Error)]` macro reads each variant's `#[error("...")]` attribute and generates:

- an `impl std::fmt::Display` whose output is the formatted message, and
- an `impl std::error::Error` (including `source()` when you mark an inner field with `#[source]` or `#[from]`).

The message strings support inline field interpolation: `#[error("user {0} was not found")]` pulls in the tuple field `.0`, just like `format!("user {0}", self.0)`. This is why `self.to_string()` in `into_response` produces the message you saw in the JSON. You write the message once; `thiserror` wires up `Display`, `Error`, and `source()` for free. It is *not* a decorator. It is a compile-time code generator that expands into plain trait impls.

> **Note:** `thiserror` adds **zero runtime cost** and no dependency at runtime: it is a `proc-macro` that runs at compile time and disappears. Contrast this with extending an `Error` class in JavaScript, which is an ordinary runtime object.

### `IntoResponse` is the contract

Axum can only respond with types that implement the `IntoResponse` trait. Many types already do: `String`, `&str`, `StatusCode`, `Json<T>`, and tuples like `(StatusCode, Json<T>)`. By implementing it for `AppError`, you make your error a first-class response. The `match &self` chooses a status code per variant; the success and failure arms of a handler's `Result` are *both* converted through `IntoResponse`, which is why `Result<Json<...>, AppError>` works as a return type.

> **Tip:** The `(StatusCode, Json<T>)` tuple is the workhorse here. Returning `(StatusCode::NOT_FOUND, Json(body)).into_response()` sets the status line and a JSON body in one expression. See [Request and Response](/16-web-apis/07-request-response/) for the full set of `IntoResponse` implementations.

### `?` replaces `try/catch`

The `?` operator is the engine of Rust error handling. When you write `users.get(&id).ok_or(AppError::NotFound(id))?`, the `?` says "if this is `Err`/`None`, return that error from the function now." Because `serde_json::Error` has a `#[from]` conversion into `AppError::Json`, the line `serde_json::from_str("not a number")?` automatically wraps the underlying error. This is the moral equivalent of `throw`, but it is **explicit, local, and visible in the type signature**: you can see at the function boundary exactly what error a handler may emit. See [Section 08: Error Handling](/08-error-handling/) for the `?` operator and the `From`-based conversion mechanics in depth.

### Logging vs. leaking

The branch `if status.is_server_error()` logs the *full* `Debug` representation of the error (including the `#[source]` chain) with `tracing::error!`, but the client only ever receives `self.to_string()`, the public `Display` message. For 5xx errors we deliberately send the generic `"internal server error"` so we never leak a database connection string or a stack-trace-like detail to the outside world. This mirrors the `console.error(err)` + `res.status(500).json({error:"internal server error"})` split in the Express example, but here it is enforced by which field each variant exposes.

---

## Key Differences

| Concern | Express.js | Axum / Rust |
| --- | --- | --- |
| Signaling failure | `throw` an exception | `return Err(...)` / the `?` operator |
| Error transport | thrown value bubbles up the call stack | `Result<T, E>` returned through the stack |
| Central handling | 4-arg error middleware, registered last | one `impl IntoResponse for AppError` |
| Status mapping | `err.status` field read at runtime | `match` on the variant, checked at compile time |
| "What can fail here?" | invisible until runtime | visible in the function's return type |
| Forgetting an error path | silently 500s / hangs | does not compile |
| Leaking internals | easy (`err.message` to client) | explicit: choose which field is public |
| Cost of the machinery | exception unwinding | a tagged-union return value (no unwinding) |

The deepest difference: in TypeScript, error handling is a *runtime convention* you can forget. In Rust, it is a *type-system obligation*. A function that returns `Result<T, AppError>` cannot be used without acknowledging the `Err` case, so a whole class of "forgot to handle that" bugs disappears.

> **Note:** Unlike a thrown JavaScript exception that unwinds an arbitrary number of frames, `?` returns from exactly *one* function: the one it appears in. Errors travel up one `Result`-returning call at a time. This is more verbose than `throw`, but it is also why the control flow is obvious from the signatures.

---

## Common Pitfalls

### Pitfall 1: Returning an error type that is not `IntoResponse`

A handler may only return types Axum knows how to respond with. If your error does not implement `IntoResponse`, the handler is not a valid `Handler` and you get a confusing trait-bound error rather than a friendly "implement IntoResponse" message.

```rust
use axum::{routing::get, Router};

// does not compile (error[E0277]): ParseIntError is not IntoResponse,
// so this fn does not satisfy the `Handler` trait.
async fn handler() -> Result<String, std::num::ParseIntError> {
    let n: i32 = "x".parse()?;
    Ok(n.to_string())
}

fn main() {
    let _app: Router = Router::new().route("/", get(handler));
}
```

The real error from `cargo check` (axum 0.8.9, Rust 1.96.0) is:

```text
error[E0277]: the trait bound `fn() -> impl Future<Output = Result<String, ParseIntError>> {handler}: Handler<_, _>` is not satisfied
   --> src/bin/pitfall.rs:11:53
    |
 11 |     let _app: Router = Router::new().route("/", get(handler));
    |                                                 --- ^^^^^^^ the trait `Handler<_, _>` is not implemented for fn item ...
    |                                                 |
    |                                                 required by a bound introduced by this call
    |
    = note: Consider using `#[axum::debug_handler]` to improve the error message
note: required by a bound in `axum::routing::get`
```

> **Tip:** When you see "the trait `Handler<_, _>` is not implemented", add `#[axum::debug_handler]` above the handler (it requires the `macros` feature) and recompile. It rewrites the error to point at the *exact* parameter or return type that is the problem. The fix here is to map `ParseIntError` into your `AppError` (give `AppError` a `#[from] ParseIntError` variant) so the handler returns `Result<String, AppError>`.

### Pitfall 2: Using `unwrap()`/`expect()` instead of returning an error

```rust
async fn get_user(id: u64) -> String {
    let users = std::collections::HashMap::<u64, String>::new();
    users.get(&id).unwrap().clone() // panics if the user is missing
}
```

`unwrap()` on a missing key panics, which Axum/Tower catches and turns into a bare `500` with no body, and crashes that task. Always return `Err(AppError::NotFound(id))` via `?` instead. Reserve `unwrap()`/`expect()` for setup code (like `TcpListener::bind(...).await.unwrap()` in `main`) where a failure genuinely means the program cannot start.

### Pitfall 3: Forgetting that built-in extractor rejections are not your error type

Axum's own extractors (`Path`, `Query`, `Json`) reject malformed input *before your handler runs*, and they reply with their **own** plain-text error, not your JSON shape. Requesting `/users/abc` against the example (where the route is `/users/{id}` with `Path<u64>`) returns this real response:

```text
$ curl -s http://127.0.0.1:3000/users/abc
Invalid URL: Cannot parse `abc` to a `u64`       # HTTP 400, plain text
```

That is a `PathRejection`, not `AppError::NotFound`, so it does not go through your `IntoResponse`. If you want a *consistent* JSON error envelope across both your domain errors and extractor failures, wrap the extractor; see the Real-World example below.

### Pitfall 4: Leaking internal details in 5xx responses

```rust
// Anti-pattern: sends the raw error (maybe a DB DSN!) to the client.
AppError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
```

Send a generic message to the client and log the detail server-side, as the working example does. This is a security practice, not just a style preference: internal error strings frequently contain file paths, SQL, or connection details.

---

## Best Practices

- **One error type per crate/service.** A single `AppError` enum that every handler returns keeps the response format uniform. Add a variant when you discover a new failure mode.
- **`thiserror` for typed enums; `anyhow` for the catch-all.** Use `thiserror` when callers need to *match* on specific variants (the typical web-handler case). Use `anyhow` (or an `anyhow`-wrapping variant) for the "anything unexpected becomes a 500" path. The two compose well; see Exercise 3.
- **Map status codes in the `match`, not at the throw site.** Keep the variant-to-status mapping in one `into_response` so it is easy to audit.
- **Always log 5xx with the full source chain, return a generic message.** `tracing::error!(error = ?self, ...)` records the `Debug` (with `source()`); the client gets `Display`.
- **Use `#[from]` to make `?` ergonomic.** Mark the most common foreign errors (`serde_json::Error`, your DB driver's error, `std::io::Error`) with `#[from]` so handlers can `?` them directly.
- **Initialize a `tracing` subscriber** so those `error!` logs actually appear. Add `cargo add tracing-subscriber --features env-filter` and call `tracing_subscriber::fmt::init();` at the top of `main`. See [Middleware and Layers](/16-web-apis/05-middleware/) for `TraceLayer`, which logs every request/response automatically.
- **Pick semantically correct status codes.** `400 Bad Request` for syntactically malformed input, `422 Unprocessable Entity` for well-formed-but-invalid input, `404` for missing resources, `401`/`403` for auth, `409` for conflicts. The [Validation](/16-web-apis/09-validation/) chapter covers returning helpful 400/422 bodies.

---

## Real-World Example

A production API wants **every** error — whether a domain error you raised or a malformed-JSON rejection from the `Json` extractor — to come back in the same JSON envelope. The clean way to do this in axum 0.8 is a custom extractor built with `#[derive(FromRequest)]` that reuses `Json` but routes its rejection through your `AppError`. This needs the `macros` feature:

```bash
cargo add axum@0.8 --features macros
cargo add thiserror@2 serde --features serde/derive
cargo add serde_json
```

```rust
// src/main.rs — a uniform JSON error envelope for domain + extractor errors
use axum::{
    extract::{rejection::JsonRejection, FromRequest},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
enum AppError {
    // `#[from]` pulls in axum's own JsonRejection so we can reuse its status code.
    #[error("invalid request body: {0}")]
    Body(#[from] JsonRejection),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // axum's JsonRejection already knows the right code (400 vs 422 vs 415).
        let status = match &self {
            AppError::Body(rejection) => rejection.status(),
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

// A custom extractor: behaves exactly like `Json<T>`, but its rejection is OUR
// AppError, so malformed bodies come back in the same envelope as domain errors.
#[derive(FromRequest)]
#[from_request(via(Json), rejection(AppError))]
struct AppJson<T>(T);

#[derive(Deserialize, Serialize)]
struct NewUser {
    name: String,
}

async fn create(AppJson(user): AppJson<NewUser>) -> Json<NewUser> {
    // In a real handler you would persist `user`; here we echo it back.
    Json(user)
}

#[tokio::main]
async fn main() {
    let app: Router = Router::new().route("/users", post(create));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Hitting `/users` with various bodies gives these **real** responses. Notice the status codes are chosen by axum's `JsonRejection::status()`, and the body shape is always *ours*:

```text
$ curl -s -X POST /users -H 'content-type: application/json' -d '{"name":"Alice"}'
{"name":"Alice"}                                                         # HTTP 200

$ curl -s -X POST /users -H 'content-type: application/json' -d '{}'
{"error":"invalid request body: Failed to deserialize the JSON body into the target type: missing field `name` at line 1 column 2"}
                                                                          # HTTP 422

$ curl -s -X POST /users -H 'content-type: application/json' -d '{not json'
{"error":"invalid request body: Failed to parse the request body as JSON: key must be a string at line 1 column 2"}
                                                                          # HTTP 400

$ curl -s -X POST /users -d '{"name":"Alice"}'
{"error":"invalid request body: Expected request with `Content-Type: application/json`"}
                                                                          # HTTP 415
```

Now malformed input (400/422/415) and your domain errors share one error format, and the status codes are still semantically correct: `422` for a well-formed JSON object missing a field, `400` for syntactically broken JSON, `415` for a missing/wrong `Content-Type`. To add domain errors (a `NotFound`, a `Conflict`) you just add variants to `AppError` and extend the `match` in `into_response`. The [JSON APIs](/16-web-apis/08-json-apis/) and [Validation](/16-web-apis/09-validation/) chapters build full CRUD resources on top of this pattern, and [Extractors](/16-web-apis/04-extractors/) explains the `FromRequest`/`FromRequestParts` machinery behind `#[derive(FromRequest)]`.

---

## Further Reading

### Official Documentation

- [axum `error_handling` module](https://docs.rs/axum/0.8/axum/error_handling/index.html) — the framework's own guidance on the `Result`-returning approach
- [`IntoResponse` trait](https://docs.rs/axum/0.8/axum/response/trait.IntoResponse.html): every type Axum can respond with, and how to implement your own
- [`#[derive(FromRequest)]` (`axum-macros`)](https://docs.rs/axum/0.8/axum/extract/derive.FromRequest.html) — the `via(...)` + `rejection(...)` attributes used above
- [`thiserror` crate docs](https://docs.rs/thiserror/2/thiserror/) — `#[error(...)]`, `#[from]`, `#[source]`, `#[error(transparent)]`
- [`anyhow` crate docs](https://docs.rs/anyhow/1/anyhow/): the dynamic catch-all error type
- [axum `anyhow-error-response` example](https://github.com/tokio-rs/axum/blob/main/examples/anyhow-error-response/src/main.rs) — the official wrap-`anyhow` pattern

### Related Topics

- [Section 08: Error Handling](/08-error-handling/) — `Result`, `Option`, the `?` operator, and `From`-based conversions (read this first if `?` is new)
- [Request and Response](/16-web-apis/07-request-response/): `IntoResponse`, status codes, and `(StatusCode, Json)` tuples
- [Extractors](/16-web-apis/04-extractors/) — `FromRequest`/`FromRequestParts` and built-in extractor rejections
- [Validation](/16-web-apis/09-validation/): returning helpful `400`/`422` bodies for invalid input
- [Middleware and Layers](/16-web-apis/05-middleware/) — `TraceLayer` for request logging and short-circuiting with errors
- [JSON APIs](/16-web-apis/08-json-apis/) — a CRUD resource that uses this error type throughout
- [Section 02: Basics](/02-basics/) and [Section 01: Getting Started](/01-getting-started/) — enums, `match`, and `cargo add`
- Next section: [Databases](/17-database/): mapping `sqlx`/`diesel` errors into `AppError` with `#[from]`

---

## Exercises

### Exercise 1: A `code` field and a new status

**Difficulty:** Easy

**Objective:** Extend an `AppError` with a machine-readable `code` field and a `429 Too Many Requests` variant.

**Instructions:**

1. Define an `AppError` enum with `NotFound`, `RateLimited`, and `Internal` variants using `thiserror`.
2. Implement `IntoResponse` so each variant returns the right status (`404`, `429`, `500`) and a JSON body of the form `{ "error": "...", "code": "..." }`.
3. The `code` should be a short stable string like `"rate_limited"`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("resource not found")]
    NotFound,
    #[error("rate limit exceeded, retry later")]
    RateLimited,
    #[error("internal server error")]
    Internal,
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            AppError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = match self {
            AppError::NotFound => "not_found",
            AppError::RateLimited => "rate_limited",
            AppError::Internal => "internal",
        };
        (status, Json(json!({ "error": self.to_string(), "code": code }))).into_response()
    }
}

async fn h() -> Result<String, AppError> {
    Err(AppError::RateLimited)
}

#[tokio::main]
async fn main() {
    let app: Router = Router::new().route("/", get(h));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`GET /` returns `{"error":"rate limit exceeded, retry later","code":"rate_limited"}` with `HTTP 429`.

</details>

### Exercise 2: Convert a foreign error with `#[from]`

**Difficulty:** Medium

**Objective:** Let a handler use `?` directly on `str::parse`, converting `ParseIntError` into your `AppError`, and add a second validation step.

**Instructions:**

1. Give `AppError` a `Parse(#[from] ParseIntError)` variant (status `400`) and an `OutOfRange` variant (status `422`).
2. Write a function that parses a string to an integer with `?`, then checks it fits in a `u8`, returning `OutOfRange` otherwise.
3. Confirm the `?` conversion compiles without an explicit `.map_err`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::num::ParseIntError;

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("bad number: {0}")]
    Parse(#[from] ParseIntError),
    #[error("value out of range")]
    OutOfRange,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::Parse(_) => StatusCode::BAD_REQUEST,
            AppError::OutOfRange => StatusCode::UNPROCESSABLE_ENTITY,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

async fn parse_and_check(input: &str) -> Result<u8, AppError> {
    let n: i64 = input.parse()?; // ParseIntError -> AppError::Parse via `?`
    u8::try_from(n).map_err(|_| AppError::OutOfRange)
}

async fn h() -> Result<String, AppError> {
    let n = parse_and_check("300").await?; // 300 doesn't fit in a u8
    Ok(n.to_string())
}

#[tokio::main]
async fn main() {
    let app: Router = Router::new().route("/", get(h));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`parse_and_check("abc")` would yield `400 {"error":"bad number: invalid digit found in string"}`, while `"300"` yields `422 {"error":"value out of range"}`. The `?` on `input.parse()` needs no `.map_err` because of the `#[from]` impl.

</details>

### Exercise 3: Typed variants plus an `anyhow` catch-all

**Difficulty:** Hard

**Objective:** Build an `AppError` that has explicit domain variants *and* a transparent `anyhow` catch-all, so unmodeled failures become `500`s without you enumerating them, while never leaking the internal message.

**Instructions:**

1. Add `anyhow` (`cargo add anyhow`).
2. Define `AppError` with `NotFound(u64)`, `Forbidden`, and `Unexpected(#[from] anyhow::Error)` (use `#[error(transparent)]` on the catch-all).
3. In `IntoResponse`, map domain variants to `404`/`403`, and the catch-all to `500`, but send the generic `"internal server error"` message for the `500`, while logging the real error with `tracing::error!`.
4. Show a function that returns `Err(anyhow::anyhow!("...").into())` and confirm it surfaces as a `500`.

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
use serde_json::json;

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("user {0} not found")]
    NotFound(u64),
    #[error("forbidden")]
    Forbidden,
    // Catch-all: any error that isn't a known domain case becomes a 500.
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    kind: &'static str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, kind) = match &self {
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::Unexpected(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };

        // Public message: hide the inner detail for 5xx, expose it otherwise.
        let public_message = match &self {
            AppError::Unexpected(_) => "internal server error".to_string(),
            other => other.to_string(),
        };

        if status.is_server_error() {
            tracing::error!(error = ?self, "request failed");
        }

        (status, Json(ErrorBody { error: public_message, kind })).into_response()
    }
}

fn lookup(id: u64) -> Result<String, AppError> {
    if id == 0 {
        // A deep failure we never modeled: bubbles up as anyhow -> 500.
        return Err(anyhow::anyhow!("db connection reset").into());
    }
    if id == 42 {
        return Err(AppError::Forbidden);
    }
    Ok(format!("user-{id}"))
}

async fn h() -> Result<String, AppError> {
    let name = lookup(0)?; // triggers the anyhow catch-all
    Ok(name)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init(); // needs `cargo add tracing-subscriber`
    let app: Router = Router::new().route("/", get(h));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`GET /` returns `{"error":"internal server error","kind":"internal"}` with `HTTP 500`, while the server log records the real `db connection reset` cause via `tracing::error!`. Notice `#[error(transparent)]` means `AppError::Unexpected`'s `Display` forwards to the inner `anyhow::Error`, but `into_response` deliberately overrides that for the public 5xx message. Typed domain errors and an open-ended catch-all coexist in one enum.

</details>
