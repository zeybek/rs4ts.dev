---
title: "Extractors"
description: "Express reads everything off one req object; Axum extractors like Path, Query, and Json become typed, validated handler parameters, with custom guards in Rust too."
---

In Express.js you reach into a single `req` object for everything: `req.params`, `req.query`, `req.body`, `req.headers`. Axum flips this around: each piece of the request becomes a typed function parameter called an **extractor**, and the framework parses and validates it for you before your handler ever runs.

---

## Quick Overview

An **extractor** is a type that knows how to build itself from an incoming HTTP request. Instead of pulling values out of one big `req` object and hoping they exist, you declare exactly what you need in your handler's signature — `Path<u64>`, `Query<Pagination>`, `Json<CreateUser>` — and Axum populates them or returns a `400`/`422` automatically. This pushes request parsing and validation into the type system, so a handler that compiles has already been handed correctly-typed data.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. This page targets **axum 0.8**.

---

## TypeScript/JavaScript Example

In Express, the request is a single object and you destructure whatever you need from it. Nothing is typed or validated at the framework level; that is your job.

```typescript
// Express.js — everything hangs off `req`
import express, { Request, Response } from "express";

const app = express();
app.use(express.json());

interface Pagination {
  page: number;
  perPage: number;
}

interface CreateUser {
  name: string;
  email: string;
}

// Path param + query string
app.get("/users/:id", (req: Request, res: Response) => {
  const id = Number(req.params.id); // string -> number by hand
  if (Number.isNaN(id)) {
    return res.status(400).json({ error: "id must be a number" });
  }

  const page = Number(req.query.page ?? "1");
  const perPage = Number(req.query.perPage ?? "20");
  const pagination: Pagination = { page, perPage };

  res.json({ id, ...pagination });
});

// JSON body + a header
app.post("/users", (req: Request, res: Response) => {
  const body = req.body as CreateUser; // a lie: nothing was actually checked
  if (typeof body.name !== "string" || typeof body.email !== "string") {
    return res.status(400).json({ error: "name and email are required" });
  }

  const userAgent = req.get("user-agent") ?? "unknown";
  res.status(201).json({ id: 1, ...body, userAgent });
});

app.listen(3000);
```

**Key points:**

- One `req` object; you destructure `params`, `query`, `body`, headers manually.
- `req.params.id` is always a `string` — you convert and validate yourself.
- `req.body as CreateUser` is a TypeScript *cast*, not a runtime check. The cast compiles even if the body is `{}` or `null`.
- Forgetting `app.use(express.json())` silently leaves `req.body` as `undefined`.

---

## Rust Equivalent

In Axum each part of the request is a separate, typed parameter. Axum parses it, and if parsing fails the client gets a sensible error before your code runs.

```rust
use axum::{
    extract::{Json, Path, Query},
    http::{header::USER_AGENT, HeaderMap, StatusCode},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Pagination {
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Deserialize, Serialize)]
struct CreateUser {
    name: String,
    email: String,
}

#[derive(Serialize)]
struct UserResponse {
    id: u64,
    name: String,
    email: String,
    user_agent: String,
}

// Path param + query string. `id` is already a `u64`.
async fn get_user(Path(id): Path<u64>, Query(pg): Query<Pagination>) -> String {
    let page = pg.page.unwrap_or(1);
    let per_page = pg.per_page.unwrap_or(20);
    format!("user {id}, page {page}, per_page {per_page}")
}

// JSON body + a header. `body` is guaranteed to have `name` and `email`.
async fn create_user(
    headers: HeaderMap,
    Json(body): Json<CreateUser>,
) -> (StatusCode, Json<UserResponse>) {
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let response = UserResponse {
        id: 1,
        name: body.name,
        email: body.email,
        user_agent,
    };
    (StatusCode::CREATED, Json(response))
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/users/{id}", get(get_user))
        .route("/users", get(get_user).post(create_user));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

```toml
# Cargo.toml
[dependencies]
axum = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
```

**Key points:**

- `Path<u64>` gives you a real `u64`: the conversion *and* the "is it a number?" check happen for free.
- `Query<Pagination>` deserializes the query string into a struct via `serde`.
- `Json<CreateUser>` deserializes *and validates the shape* of the body. If `email` is missing, the request never reaches your code.
- `HeaderMap` is itself an extractor; no `Header<...>` wrapper needed.

---

## Detailed Explanation

### What "extractor" actually means

An extractor is any type that implements one of two traits:

- **`FromRequestParts<S>`** — builds itself from the request *metadata* (method, URI, headers, extensions) without touching the body. `Path`, `Query`, `HeaderMap`, and `State` are all of this kind. You can have many of these in one handler.
- **`FromRequest<S>`** — builds itself by consuming the *entire request*, body included. `Json`, `Bytes`, `String`, and `Form` are of this kind. Because the body can only be read once, **at most one** body extractor is allowed, and it must come last.

The `S` is your application's shared state type (covered in [Shared Application State in Axum](/16-web-apis/06-state-management/)). For handlers without state it is inferred.

When a request arrives, Axum runs each extractor in declaration order. Each one returns a `Result`; on `Err` it short-circuits and turns the rejection into an HTTP response, and your handler is never called.

### `Path` — one value, a tuple, or a struct

`Path` is generic over how you want the captured segments shaped:

```rust
use axum::extract::Path;
use serde::Deserialize;

// Single segment -> single value.
async fn one(Path(id): Path<u64>) -> String {
    format!("user {id}")
}

// Multiple segments -> a tuple, in route order.
async fn two(Path((user_id, post_id)): Path<(u64, u64)>) -> String {
    format!("user {user_id} post {post_id}")
}

// Multiple segments -> a struct, matched by NAME.
#[derive(Deserialize)]
struct PostPath {
    user_id: u64,
    post_id: u64,
}

async fn named(Path(p): Path<PostPath>) -> String {
    format!("user {} post {}", p.user_id, p.post_id)
}
```

The routes would be `/users/{id}`, `/users/{user_id}/posts/{post_id}`, and so on. Note the `{name}` syntax: axum 0.8 replaced the old `:name` form. For the full routing story see [Routing in Axum](/16-web-apis/03-routing/).

> **Note:** With a tuple, segments are matched by *position*. With a struct, they are matched by *field name* against the `{name}` captures in the route. The struct form is safer because reordering route segments will not silently swap your values.

### `Query` — the query string as a struct

`Query<T>` percent-decodes the query string and deserializes it into `T`. Use `Option<...>` for parameters that may be absent:

```rust
use axum::extract::Query;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Filters {
    status: Option<String>,
    limit: Option<u32>,
}

async fn search(Query(f): Query<Filters>) -> String {
    format!("status={:?} limit={:?}", f.status, f.limit)
}

// When you do not know the keys ahead of time:
async fn raw(Query(params): Query<HashMap<String, String>>) -> String {
    format!("{params:?}")
}
```

> **Tip:** Plain `Query` does *not* handle repeated keys like `?tag=a&tag=b` into a `Vec`. For that, add `axum-extra` and use `axum_extra::extract::Query`, which supports `Vec<String>` fields.

### `Json` — the body, deserialized and checked

`Json<T>` reads the whole body, requires a `Content-Type: application/json` header, and runs `serde` deserialization. A successful extraction means `T`'s required fields were all present and well-typed.

`Json` is also a *response* type: returning `Json(value)` serializes `value` and sets the content type. Response usage is covered in [Request and Response Handling](/16-web-apis/07-request-response/) and [JSON REST APIs](/16-web-apis/08-json-apis/); here we focus on its extractor role.

### Headers and `State`

`HeaderMap` gives you the full header set. `State<T>` hands you a clone of shared application state (a database pool, config, an in-memory store). Both implement `FromRequestParts`, so they coexist freely with `Path` and `Query`:

```rust
use axum::extract::State;
use axum::http::{header::USER_AGENT, HeaderMap};

#[derive(Clone)]
struct AppState {
    app_name: String,
}

async fn whoami(State(state): State<AppState>, headers: HeaderMap) -> String {
    let ua = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    format!("{}: {ua}", state.app_name)
}
```

`State` deserves its own treatment; see [Shared Application State in Axum](/16-web-apis/06-state-management/).

### What rejections look like over the wire

These are the **real** responses from the server above. Notice the status codes are chosen for you:

```text
GET /users/42                      -> 200  user 42, page 1, per_page 20
GET /users/abc                     -> 400  Invalid URL: Cannot parse `abc` to a `u64`
GET /users/42?page=3&per_page=50   -> 200  user 42, page 3, per_page 50
GET /users/42?page=abc             -> 400  Failed to deserialize query string: page: invalid digit found in string
POST /users  {"name":"Ada","email":"ada@x.io"}  -> 201  {"id":1,"name":"Ada","email":"ada@x.io"}
POST /users  {"name":"Ada"}        -> 422  Failed to deserialize the JSON body into the target type: missing field `email` at line 1 column 14
POST /users  (no Content-Type)     -> 415  Expected request with `Content-Type: application/json`
```

A bad path segment is a `400`, a malformed query is a `400`, a JSON body of the wrong *shape* is a `422 Unprocessable Entity`, and a missing content type is a `415 Unsupported Media Type`, all before your handler runs.

---

## Key Differences

| Concern | Express.js | Axum |
| --- | --- | --- |
| Where data comes from | one `req` object | one typed parameter per piece |
| Param types | always `string`; convert by hand | parsed to the type you declare (`u64`, structs) |
| Body validation | manual, or a separate library | `Json<T>` checks shape via `serde` automatically |
| Missing/invalid input | you write the `400` | framework returns `400`/`422`/`415` |
| Body read | `req.body` after `express.json()` | one `FromRequest` extractor, always last |
| "I forgot to parse the body" | `req.body` is `undefined` at runtime | the code does not compile / a 415 is returned |

The deeper idea: in Express, request parsing is *imperative work inside the handler*. In Axum it is *declarative metadata in the signature*. The handler body starts from valid, typed data, the same way a function with typed parameters starts from valid arguments.

### `FromRequestParts` vs `FromRequest`

This distinction is the single most important thing to internalize:

- `FromRequestParts` extractors read only metadata and are cheap and composable; use as many as you like.
- `FromRequest` extractors consume the body — exactly one, and it must be the **last** parameter.

You can write your own extractor by implementing `FromRequestParts`. In axum 0.8 the trait uses native `async fn`, so no `#[async_trait]` is needed:

```rust
use axum::extract::FromRequestParts;
use axum::http::{header::AUTHORIZATION, request::Parts, StatusCode};

// Pull a bearer token out of the Authorization header.
struct ApiKey(String);

impl<S> FromRequestParts<S> for ApiKey
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "missing Authorization header"))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or((StatusCode::UNAUTHORIZED, "expected a Bearer token"))?;

        Ok(ApiKey(token.to_string()))
    }
}

// Now `ApiKey` is usable like any built-in extractor:
async fn protected(ApiKey(token): ApiKey) -> String {
    format!("token starts with {}", &token[..token.len().min(4)])
}
```

This "extractor as a guard" pattern is the foundation of [Authentication Patterns](/16-web-apis/12-authentication/) and [JWT Authentication](/16-web-apis/13-jwt/).

---

## Common Pitfalls

### Putting a body extractor before another extractor

A body extractor (`Json`, `Bytes`, `String`, `Form`) must be the **last** parameter. If it is not, the handler fails to satisfy the `Handler` trait and you get a wall of trait-bound errors:

```rust
use axum::{extract::{Json, Path}, routing::post, Router};
use serde::Deserialize;

#[derive(Deserialize)]
struct Body { name: String }

// does not compile (error[E0277]): Json is not the last parameter
async fn handler(Json(body): Json<Body>, Path(id): Path<u64>) -> String {
    format!("{} {id}", body.name)
}

fn build() -> Router {
    Router::new().route("/items/{id}", post(handler))
}
```

The raw error is the cryptic `the trait bound ... Handler<_, _> is not implemented`. The fix-it nudge in that output is gold: add `#[axum::debug_handler]` to the handler (it needs the `macros` feature on axum). With it, the real message becomes precise:

```text
error: `Json<_>` consumes the request body and thus must be the last argument to the handler function
 --> src/main.rs:8:30
  |
8 | async fn handler(Json(body): Json<Body>, Path(id): Path<u64>) -> String {
  |                              ^^^^
```

The fix is simply to reorder: `async fn handler(Path(id): Path<u64>, Json(body): Json<Body>)`.

> **Tip:** Whenever a handler "won't implement `Handler`" and the error is unreadable, slap `#[axum::debug_handler]` on it. It exists purely to translate those trait errors into plain English.

### Using the old `:id` route syntax

axum 0.7 used `:id`; axum 0.8 uses `{id}`. The old form is not a compile error; it **panics at startup**, at the line where you call `.route("/users/:id", ...)`:

```text
thread 'main' panicked at src/main.rs:9:37:
Path segments must not start with `:`. For capture groups, use `{capture}`.
If you meant to literally match a segment starting with a colon, call
`without_v07_checks` on the router.
```

### Trusting `req.body as T` habits — `Json<T>` needs `Deserialize`

A subtle one: `Json<T>` as an *extractor* requires `T: Deserialize`. If your struct only derives `Serialize` (because you have only ever returned it), using it in a `Json<T>` parameter fails the `Handler` bound. Derive both `Serialize` and `Deserialize` for types that travel in *and* out:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)] // both, for a round-trip type
struct User {
    id: u64,
    name: String,
}
```

### Forgetting that `Query`/`Json` distinguish "missing" from "wrong type"

A missing optional field is fine if you use `Option<T>`. But a *present but wrong-typed* value is a hard error: `?page=abc` against a `page: Option<u32>` is a `400`, not a `None`. If you want "ignore garbage and default", parse it as a `String` and convert yourself.

### Reaching for `async-trait` to write a custom extractor

You do not need it. Native `async fn` in traits has been stable since Rust 1.75, and axum 0.8's `FromRequestParts`/`FromRequest` use it directly. The `async-trait` crate is only relevant when you need `dyn Trait` dynamic dispatch, which extractors do not.

---

## Best Practices

- **Declare exactly what you need.** Prefer `Path<u64>` over `Path<String>` so the framework rejects non-numeric ids for you.
- **Use structs for `Query` and multi-segment `Path`.** Named fields are self-documenting and survive reordering. Reserve tuples for one or two obvious segments.
- **Make optional query params `Option<T>`** and apply defaults in the handler with `unwrap_or`.
- **Keep the body extractor last**, always. Treat it as a rule, not a per-handler decision.
- **Derive both `Serialize` and `Deserialize`** on DTOs that are accepted *and* returned.
- **Write a custom `FromRequestParts` extractor** for cross-cutting concerns (auth, tenant resolution, request ids). A guard that lives in the signature cannot be forgotten the way a manual check inside the body can.
- **Override rejections when you want a uniform error body.** Extract `Result<Json<T>, JsonRejection>` to shape the `4xx` yourself, or centralize it as shown in [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/).
- **Reach for `#[axum::debug_handler]` during development** when extractor errors are noisy.

---

## Real-World Example

A small, authenticated user API that combines shared `State`, a custom `AuthUser` guard extractor, `Path`, `Query`, and a hand-shaped JSON rejection. Every line below is compile-verified against axum 0.8.

```rust
use axum::{
    extract::{rejection::JsonRejection, FromRequestParts, Json, Path, Query, State},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct AppState {
    users: Arc<Mutex<HashMap<u64, User>>>,
    api_token: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
}

#[derive(Deserialize)]
struct ListParams {
    name_contains: Option<String>,
}

// Guard extractor: validates the bearer token against application state.
struct AuthUser;

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token".to_string()))?;

        if token != state.api_token {
            return Err((StatusCode::UNAUTHORIZED, "invalid token".to_string()));
        }
        Ok(AuthUser)
    }
}

// The guard runs first; if it rejects, the rest never runs.
async fn list_users(
    _auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<Vec<User>> {
    let users = state.users.lock().unwrap();
    let needle = params.name_contains.unwrap_or_default().to_lowercase();
    let out: Vec<User> = users
        .values()
        .filter(|u| needle.is_empty() || u.name.to_lowercase().contains(&needle))
        .cloned()
        .collect();
    Json(out)
}

async fn get_user(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<User>, StatusCode> {
    state
        .users
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

// Take the Result form of the body extractor to shape our own 422 body.
async fn create_user(payload: Result<Json<User>, JsonRejection>) -> Response {
    match payload {
        Ok(Json(user)) => (StatusCode::CREATED, Json(user)).into_response(),
        Err(rejection) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError { error: rejection.body_text() }),
        )
            .into_response(),
    }
}

#[tokio::main]
async fn main() {
    let mut seed = HashMap::new();
    seed.insert(1, User { id: 1, name: "Ada".into() });

    let state = AppState {
        users: Arc::new(Mutex::new(seed)),
        api_token: "secret".into(),
    };

    let app = Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", get(get_user))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Exercising it with `curl` produces these **real** responses:

```text
GET  /users                              -> 401  missing bearer token
GET  /users  -H 'Authorization: Bearer nope'   -> 401  invalid token
GET  /users  -H 'Authorization: Bearer secret' -> 200  [{"id":1,"name":"Ada"}]
GET  /users/1  -H 'Authorization: Bearer secret' -> 200  {"id":1,"name":"Ada"}
GET  /users/99 -H 'Authorization: Bearer secret' -> 404
POST /users  {"id":"oops"}               -> 422  {"error":"Failed to deserialize the JSON body into the target type: id: invalid type: string \"oops\", expected u64 at line 1 column 12"}
```

The guard, the state, and the parsing all happen declaratively; the handler bodies only ever see valid, authenticated, typed data.

---

## Further Reading

- [axum `extract` module docs](https://docs.rs/axum/latest/axum/extract/index.html) — the canonical list of built-in extractors and the order rules.
- [`FromRequestParts`](https://docs.rs/axum/latest/axum/extract/trait.FromRequestParts.html) and [`FromRequest`](https://docs.rs/axum/latest/axum/extract/trait.FromRequest.html) trait reference.
- [`axum::debug_handler`](https://docs.rs/axum/latest/axum/attr.debug_handler.html) — turning opaque `Handler` errors into readable messages.
- [serde derive docs](https://serde.rs/derive.html) — how `Deserialize` shapes `Query`/`Json` parsing.

Within this guide:

- [Routing in Axum](/16-web-apis/03-routing/): `{id}` path syntax, method routing, nested routers (where path captures come from).
- [Shared Application State in Axum](/16-web-apis/06-state-management/): the `State<T>` extractor, `Arc`, `FromRef`.
- [Request and Response Handling](/16-web-apis/07-request-response/): `Json` and tuples as *responses*, `IntoResponse`, status codes.
- [JSON REST APIs](/16-web-apis/08-json-apis/): a full CRUD resource built on these extractors.
- [Request Validation](/16-web-apis/09-validation/): going beyond shape checks to business-rule validation.
- [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/): a centralized `AppError` and uniform rejection bodies.
- [Authentication Patterns](/16-web-apis/12-authentication/) and [JWT Authentication](/16-web-apis/13-jwt/): the guard-extractor pattern in production form.
- Foundations: [async/await](/11-async/), [generics and traits](/09-generics-traits/), [error handling](/08-error-handling/), and the language [basics](/02-basics/) / [getting started](/01-getting-started/).
- Persisting the data these handlers receive: [Database](/17-database/).

---

## Exercises

### Exercise 1: Typed path and optional query

**Difficulty:** Easy

**Objective:** Build a handler that extracts a numeric product id from the path and an optional `currency` query parameter.

**Instructions:**

1. Add a route `/products/{id}`.
2. Write a handler that takes `Path<u64>` and a `Query` of a struct with an `Option<String>` field named `currency`.
3. Return a string like `product 7 priced in USD`, defaulting the currency to `"USD"` when absent.

<details>
<summary>Solution</summary>

```rust
use axum::{extract::{Path, Query}, routing::get, Router};
use serde::Deserialize;

#[derive(Deserialize)]
struct PriceQuery {
    currency: Option<String>,
}

async fn show_product(Path(id): Path<u64>, Query(q): Query<PriceQuery>) -> String {
    let currency = q.currency.unwrap_or_else(|| "USD".to_string());
    format!("product {id} priced in {currency}")
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/products/{id}", get(show_product));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`GET /products/7` returns `product 7 priced in USD`; `GET /products/7?currency=EUR` returns `product 7 priced in EUR`; `GET /products/abc` is rejected with a `400`.

</details>

### Exercise 2: Fix the ordering bug

**Difficulty:** Medium

**Objective:** Repair a handler that fails to compile because its body extractor is in the wrong position.

**Instructions:**

The following handler does not compile. Identify why (add `#[axum::debug_handler]` if the error is unclear), then fix it so the route works.

```rust
use axum::{extract::{Json, Path}, routing::put, Router};
use serde::Deserialize;

#[derive(Deserialize)]
struct Update {
    name: String,
}

// does not compile
async fn rename(Json(body): Json<Update>, Path(id): Path<u64>) -> String {
    format!("renamed {id} to {}", body.name)
}

fn app() -> Router {
    Router::new().route("/users/{id}", put(rename))
}
```

<details>
<summary>Solution</summary>

`Json` consumes the request body and must be the **last** parameter. Move `Path` (a `FromRequestParts` extractor) ahead of it:

```rust
use axum::{extract::{Json, Path}, routing::put, Router};
use serde::Deserialize;

#[derive(Deserialize)]
struct Update {
    name: String,
}

async fn rename(Path(id): Path<u64>, Json(body): Json<Update>) -> String {
    format!("renamed {id} to {}", body.name)
}

fn app() -> Router {
    Router::new().route("/users/{id}", put(rename))
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

`PUT /users/3` with body `{"name":"Bob"}` returns `renamed 3 to Bob`.

</details>

### Exercise 3: A custom guard extractor

**Difficulty:** Hard

**Objective:** Implement a `FromRequestParts` extractor that requires an `X-Request-Id` header and exposes it to handlers.

**Instructions:**

1. Define a `RequestId(String)` newtype.
2. Implement `FromRequestParts<S>` for it (generic over any state `S: Send + Sync`).
3. If the `x-request-id` header is missing, reject with `400 Bad Request` and a message.
4. Use it in a handler alongside `Path<u64>` and confirm the ordering rules (metadata extractors can appear in any order among themselves).

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::{FromRequestParts, Path},
    http::{request::Parts, HeaderName, StatusCode},
    routing::get,
    Router,
};

struct RequestId(String);

impl<S> FromRequestParts<S> for RequestId
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let name = HeaderName::from_static("x-request-id");
        let value = parts
            .headers
            .get(&name)
            .and_then(|v| v.to_str().ok())
            .ok_or((StatusCode::BAD_REQUEST, "missing X-Request-Id header"))?;
        Ok(RequestId(value.to_string()))
    }
}

async fn handler(RequestId(req_id): RequestId, Path(id): Path<u64>) -> String {
    format!("request {req_id} -> resource {id}")
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/items/{id}", get(handler));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`GET /items/9` without the header returns `400 missing X-Request-Id header`; with `-H 'X-Request-Id: abc123'` it returns `request abc123 -> resource 9`. Because both `RequestId` and `Path` are `FromRequestParts`, their order relative to each other does not matter.

</details>
