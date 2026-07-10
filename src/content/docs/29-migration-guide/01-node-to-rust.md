---
title: "Porting a Node.js Service to Rust"
description: "Port one Express endpoint to Axum with identical JSON, status codes, and headers, learning where ownership, typed extractors, and Serde shift the behavior."
---

Migrating a working Node.js service to Rust is a careful translation, not a rewrite-from-the-spec exercise: the **observable behavior stays identical** while the implementation changes underneath. This page walks one Express endpoint over to [Axum](/16-web-apis/01-axum-basics/), byte-for-byte on the wire, so you can see exactly what changes and what does not.

---

## Quick Overview

The goal of a service migration is that clients cannot tell the difference: the same routes return the same JSON bodies, the same status codes, and the same headers. Rust changes *how* you write the handler — explicit types, ownership, `Result`-based errors instead of thrown exceptions — but the contract your existing TypeScript/JavaScript callers depend on must not move. This walkthrough takes a small Express user service and reproduces it in Axum, verifying the responses match at every step.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. The examples here use Axum 0.8, Tokio 1.52, and Serde 1.

---

## TypeScript/JavaScript Example

Here is a realistic Express service: a couple of read endpoints over an in-memory user store, with a query parameter and a not-found case.

```typescript
// server.ts — Express 4, the service we are migrating
import express from "express";

interface User {
  id: number;
  name: string;
  email: string;
}

const users = new Map<number, User>([
  [1, { id: 1, name: "Ada", email: "ada@example.com" }],
  [2, { id: 2, name: "Linus", email: "linus@example.com" }],
]);

const app = express();

// GET /users?limit=10
app.get("/users", (req, res) => {
  const limit = Number(req.query.limit ?? 10);
  const list = [...users.values()]
    .sort((a, b) => a.id - b.id)
    .slice(0, limit);
  res.json(list);
});

// GET /users/:id
app.get("/users/:id", (req, res) => {
  const id = Number(req.params.id);
  const user = users.get(id);
  if (!user) {
    return res.status(404).json({ error: "User not found" });
  }
  res.json(user);
});

app.listen(3001, () => console.log("listening on 3001"));
```

Running it against `curl` (Express 4.22, Node v22) gives the baseline we must reproduce:

```text
GET /users/1        -> 200 {"id":1,"name":"Ada","email":"ada@example.com"}
GET /users/99       -> 404 {"error":"User not found"}
GET /users?limit=1  -> 200 [{"id":1,"name":"Ada","email":"ada@example.com"}]
GET /users/abc      -> 404 {"error":"User not found"}
```

That last line is important and easy to miss: `Number("abc")` is `NaN`, `users.get(NaN)` is `undefined`, so Express falls through to the **404** branch. Keep that case in mind. It is where the naive Rust port will diverge.

---

## Rust Equivalent

The same service in Axum. Add the dependencies first:

```bash
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json
```

```rust
use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Serialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

#[derive(Clone)]
struct AppState {
    users: Arc<HashMap<u32, User>>,
}

#[derive(Deserialize)]
struct ListParams {
    limit: Option<usize>,
}

// GET /users?limit=10
async fn list_users(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<Vec<User>> {
    let limit = params.limit.unwrap_or(10);
    let mut users: Vec<User> = state.users.values().cloned().collect();
    users.sort_by_key(|u| u.id);
    users.truncate(limit);
    Json(users)
}

// GET /users/{id}
async fn get_user(State(state): State<AppState>, Path(id): Path<u32>) -> Response {
    match state.users.get(&id) {
        Some(user) => Json(user.clone()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        )
            .into_response(),
    }
}

fn seed() -> HashMap<u32, User> {
    let mut m = HashMap::new();
    m.insert(1, User { id: 1, name: "Ada".into(), email: "ada@example.com".into() });
    m.insert(2, User { id: 2, name: "Linus".into(), email: "linus@example.com".into() });
    m
}

fn app() -> Router {
    let state = AppState { users: Arc::new(seed()) };
    Router::new()
        .route("/users", get(list_users))
        .route("/users/{id}", get(get_user))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

Running the same probes against this server produces:

```text
GET /users/1        -> 200 {"id":1,"name":"Ada","email":"ada@example.com"}
GET /users/99       -> 404 {"error":"User not found"}
GET /users?limit=1  -> 200 [{"id":1,"name":"Ada","email":"ada@example.com"}]
GET /users/abc      -> 400 Invalid URL: Cannot parse `abc` to a `u32`
```

Three of the four match exactly. The fourth does not — and that gap is the whole lesson of a careful port. We will fix it in [Detailed Explanation](#detailed-explanation).

---

## Detailed Explanation

Walking the Rust version line by line and contrasting with the Express original:

**State instead of a module-level `const`.** In Express, `users` is a `Map` captured by the route closures via JavaScript's lexical scope, a shared mutable global that "just works" because Node runs your handlers on a single thread. Rust has no implicit shared global. State is threaded explicitly through `Router::with_state` and pulled into each handler with the `State` extractor. The store is wrapped in `Arc` (an atomically reference-counted pointer) so every concurrent request can share one read-only copy cheaply. This is not bureaucracy: Axum runs handlers across a multi-threaded Tokio runtime, and the `Arc` is what makes that sound. See [state management](/16-web-apis/06-state-management/) and [reference counting](/05-ownership/07-reference-counting/).

**The query parameter is a typed struct, not `req.query`.** Express hands you `req.query.limit` as `string | undefined` and you coerce it with `Number(...)`. Axum's `Query<ListParams>` extractor deserializes the query string into a typed struct via Serde. `limit: Option<usize>` models "may be absent" exactly like `??`; `params.limit.unwrap_or(10)` is the direct analogue of `req.query.limit ?? 10`. See [extractors](/16-web-apis/04-extractors/).

**Returning data is `Json(value)`, not `res.json(value)`.** Express mutates a response object imperatively (`res.status(404).json(...)`). Axum handlers *return* a value that implements `IntoResponse`. `Json(users)` serializes the `Vec<User>` to a JSON array with `content-type: application/json`: the same wire output as `res.json([...])`. Returning a tuple `(StatusCode, Json(...))` sets the status and body together. See [JSON APIs](/16-web-apis/08-json-apis/).

**No thrown exceptions; errors are values.** The not-found path returns a 404 response value rather than throwing. Rust handlers can return `Result`, and the error type's `IntoResponse` decides the status. There is no `throw` that unwinds to a framework error handler. This is the single biggest mental shift; see [error handling](/08-error-handling/00-result-option/) and [web error handling](/16-web-apis/10-error-handling-web/).

**The route path uses `{id}`, not `:id`.** Axum 0.8 changed path syntax from the colon form to braces. A route written `/users/:id` will not match the way you expect on current Axum; use `/users/{id}`. See [routing](/16-web-apis/03-routing/).

### Closing the behavior gap on `/users/abc`

The naive port returns **400** for `/users/abc` because `Path<u32>` tries to parse the segment into a `u32` *before your handler runs*, and a non-numeric segment fails extraction. Express, by contrast, accepts any string segment and only later turns it into `NaN`, landing on 404.

If your clients (or your contract tests) depend on the 404 behavior, extract the segment as a `String` and parse it yourself, falling through to the same not-found branch:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::json;

#[derive(Clone, Serialize)]
struct User { id: u32, name: String, email: String }

#[derive(Clone)]
struct AppState { users: Arc<HashMap<u32, User>> }

// Path<String>: a non-numeric id won't 400 at the extractor.
// We parse it ourselves and fall through to 404, matching Express.
async fn get_user(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let user = id.parse::<u32>().ok().and_then(|n| state.users.get(&n));
    match user {
        Some(u) => Json(u.clone()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        )
            .into_response(),
    }
}
```

With this handler the probe now matches the Node baseline exactly:

```text
GET /users/abc -> 404 {"error":"User not found"}
GET /users/1   -> 200 {"id":1,"name":"Ada","email":"ada@example.com"}
```

> **Note:** Whether 400 or 404 is "more correct" is a real design question; a 400 is arguably better. But during a migration, *correctness is defined by the existing contract*, not by your taste. Change behavior in a separate, deliberate release, never silently as a side effect of the port. Matching shapes, codes, and headers is covered in depth in [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/).

---

## Key Differences

| Concern | Express (Node v22) | Axum 0.8 (Rust) |
| --- | --- | --- |
| Shared state | Lexically-captured global `Map` | `Arc<...>` in `State`, threaded explicitly |
| Handler shape | Mutate `res`, no return value | Return a value implementing `IntoResponse` |
| Path param | `req.params.id` is always a `string` | `Path<u32>` parses + 400s on failure; `Path<String>` to opt out |
| Query param | `req.query.x`, `string \| undefined` | `Query<T>` deserialized via Serde, typed |
| Errors | `throw` / `res.status(...).json(...)` | `Result<T, E>` where `E: IntoResponse` |
| JSON keys | Whatever you name the object fields | Struct field names; `#[serde(rename_all = ...)]` to remap |
| Concurrency model | Single-threaded event loop | Multi-threaded runtime; sharing must be `Send + Sync` |
| Route syntax | `/users/:id` | `/users/{id}` |
| Startup | `app.listen(port)` | `axum::serve(TcpListener, app)` |

The deepest difference is the **concurrency model**. Node's single-threaded event loop means your handler code never runs truly in parallel, so a plain shared `Map` is safe by accident. Axum spreads handlers across OS threads, so the compiler *requires* that shared state be safe to touch from many threads at once (`Send + Sync`). The `Arc` (and, when you need writes, an `RwLock` or `Mutex` inside it) is the price — and the guarantee — of that parallelism. This is "fearless concurrency": parallel by opt-in, with data races rejected at compile time rather than discovered in production.

> **Warning:** Do not describe Rust as "multi-threaded by default" to yourself as a Node dev: the distinction is that Rust *lets* you be parallel safely. Your Axum service is parallel because the Tokio runtime is, and the type system makes that safe.

---

## Common Pitfalls

### Pitfall 1: Mismatched response types across `match` arms

In JavaScript a handler can `res.json(...)` an object on one branch and a different object on another with no ceremony — there is no static return type. In Rust every branch of a `match` must produce the *same* type. This trips up new arrivals constantly:

```rust
use axum::{Json, http::StatusCode, response::Response};
use serde_json::json;

// does not compile (error[E0308]: mismatched types)
async fn get_user(found: bool) -> Response {
    match found {
        true => Json(json!({ "id": 1 })),
        false => (StatusCode::NOT_FOUND, Json(json!({ "error": "nope" }))),
    }
}

fn main() {}
```

The real compiler error:

```text
error[E0308]: mismatched types
 --> src/bin/pitfall.rs:7:17
  |
5 | async fn get_user(found: bool) -> Response {
  |                                   -------- expected `Response<Body>` because of return type
6 |     match found {
7 |         true => Json(json!({ "id": 1 })),
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^ expected `Response<Body>`, found `Json<Value>`
  |
  = note: expected struct `Response<Body>`
             found struct `Json<Value>`
```

The fix is to call `.into_response()` on every arm so each yields a `Response`, exactly as the worked example does. The type system is enforcing what an Express handler only hopes for: that both branches actually produce a valid HTTP response.

### Pitfall 2: JSON key casing changes silently

JavaScript objects carry whatever keys you wrote, typically camelCase. Rust structs are conventionally snake_case, and Serde serializes field names verbatim by default, so the JSON shape moves the moment you port a `fullName` field to `full_name`. This is a contract break that no test catches unless you check the actual bytes:

```rust playground
use serde::Serialize;

#[derive(Serialize)]
struct UserSnake {
    id: u32,
    full_name: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserCamel {
    id: u32,
    full_name: String,
    created_at: String,
}

fn main() {
    let snake = UserSnake { id: 1, full_name: "Ada".into(), created_at: "2026-06-02".into() };
    let camel = UserCamel { id: 1, full_name: "Ada".into(), created_at: "2026-06-02".into() };
    println!("default:  {}", serde_json::to_string(&snake).unwrap());
    println!("camelCase: {}", serde_json::to_string(&camel).unwrap());
}
```

Real output:

```text
default:  {"id":1,"full_name":"Ada","created_at":"2026-06-02"}
camelCase: {"id":1,"fullName":"Ada","createdAt":"2026-06-02"}
```

If your Node service emitted `fullName`, add `#[serde(rename_all = "camelCase")]` to keep the wire shape. This and other shape-matching details live in [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/) and [serde attributes](/15-serialization/05-attributes/).

### Pitfall 3: Assuming the extractor never rejects

`Path<u32>`, `Query<T>`, and `Json<T>` all reject malformed input *before your code runs*, returning a 400 with a plain-text body that almost certainly differs from what your Node service returned. A malformed JSON POST body, for example, yields:

```text
Failed to parse the request body as JSON: key must be a string at line 1 column 2
```

That is not the `{"error": "..."}` envelope your clients expect. If you need a custom error body for malformed input, use a custom rejection or a validation extractor, covered in [validation](/16-web-apis/09-validation/) and [web error handling](/16-web-apis/10-error-handling-web/). The point during migration: **enumerate the failure modes your Node service produced, then verify each one against the Rust service**, not just the happy path.

### Pitfall 4: Forgetting that futures are lazy

A JavaScript `Promise` starts running the moment it is created. A Rust `async fn` returns a future that does *nothing* until it is awaited by a runtime — the opposite of an eager Promise. Calling an async helper and discarding the future runs no code at all. Inside an Axum handler the runtime drives your future for you, but when you write helper functions, remember to `.await` them. See [promises vs futures](/11-async/00-promises-vs-futures/).

---

## Best Practices

- **Pin the contract with tests before you port.** Capture the Node service's real responses (status, body, key casing, headers) for representative requests and turn them into assertions. Run them against the Rust service until green. The migration is "done" when those byte-level tests pass, not when the code compiles.
- **Port one endpoint at a time, behind the existing service.** Stand the Rust service up next to Node and route a single path to it (a reverse-proxy split, or the strangler-fig pattern). Verify, then move the next path. The mechanics are in [Incremental Migration](/29-migration-guide/00-incremental/).
- **Make the error envelope a single type.** Define one `ApiError` enum that implements `IntoResponse` and maps each variant to the exact status + JSON your clients expect. Every handler returns `Result<T, ApiError>`, so the response shape is centralized and impossible to drift.
- **Use `#[serde(rename_all = "camelCase")]` proactively** on any DTO whose JSON crosses the wire to existing clients, so a snake_case Rust field never changes a key.
- **Keep handlers thin; share read-only state via `Arc`.** Reach for `Arc<RwLock<T>>` or a real database only where you genuinely mutate; an `Arc<T>` is enough for read-only config and caches.
- **Diff the headers, not just the body.** `content-type`, caching headers, and CORS are part of the contract. See [CORS](/16-web-apis/11-cors/) and [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/).

---

## Real-World Example

A fuller slice of a production service: a read endpoint *and* a `POST` that validates input, allocates an id, and returns `201 Created` — all funneling errors through one `ApiError` type so every response matches the Node envelope. The store uses `Arc<RwLock<...>>` because this version mutates.

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Serialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct CreateUser {
    name: String,
    email: String,
}

#[derive(Clone)]
struct AppState {
    users: Arc<RwLock<HashMap<u32, User>>>,
    next_id: Arc<RwLock<u32>>,
}

// One error type, mapped to the same JSON envelope the Node service returned.
enum ApiError {
    NotFound,
    Validation(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "User not found".to_string()),
            ApiError::Validation(m) => (StatusCode::BAD_REQUEST, m),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> Result<Json<User>, ApiError> {
    state
        .users
        .read()
        .unwrap()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(ApiError::NotFound)
}

async fn create_user(
    State(state): State<AppState>,
    Json(body): Json<CreateUser>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    if body.name.trim().is_empty() {
        return Err(ApiError::Validation("name is required".into()));
    }
    if !body.email.contains('@') {
        return Err(ApiError::Validation("email is invalid".into()));
    }
    let mut id_guard = state.next_id.write().unwrap();
    let id = *id_guard;
    *id_guard += 1;
    let user = User { id, name: body.name, email: body.email };
    state.users.write().unwrap().insert(id, user.clone());
    Ok((StatusCode::CREATED, Json(user)))
}

#[tokio::main]
async fn main() {
    let state = AppState {
        users: Arc::new(RwLock::new(HashMap::new())),
        next_id: Arc::new(RwLock::new(1)),
    };
    let app = Router::new()
        .route("/users", get(|| async { "ok" }).post(create_user))
        .route("/users/{id}", get(get_user))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3003")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Probing it confirms the validation, the `201`, and the error envelope:

```text
POST /users {"name":"Grace","email":"grace@example.com"} -> 201 {"id":1,"name":"Grace","email":"grace@example.com"}
POST /users {"name":"Grace","email":"nope"}              -> 400 {"error":"email is invalid"}
GET  /users/1                                            -> 200 {"id":1,"name":"Grace","email":"grace@example.com"}
POST /users {bad}                                        -> 400 Failed to parse the request body as JSON: ...
```

Note the last line: malformed JSON is rejected by the `Json` extractor *before* `create_user` runs, so it bypasses your `ApiError` envelope. If your contract requires a JSON error body for malformed input too, wrap the body in a custom extractor; see [validation](/16-web-apis/09-validation/). In real services this state would be a database pool rather than an in-memory map; see [connection pooling](/17-database/08-connection-pooling/) and [sqlx](/17-database/00-sqlx-intro/), and [Data Migration Strategies](/29-migration-guide/03-data-migration/) for moving the data itself.

---

## Further Reading

- [Axum basics](/16-web-apis/01-axum-basics/) and [Axum setup](/16-web-apis/02-axum-setup/) — the framework this port targets
- [Extractors](/16-web-apis/04-extractors/), [routing](/16-web-apis/03-routing/), [JSON APIs](/16-web-apis/08-json-apis/), [validation](/16-web-apis/09-validation/) — the building blocks used above
- [Web error handling](/16-web-apis/10-error-handling-web/) and [`Result`/`Option`](/08-error-handling/00-result-option/): errors as values
- [State management](/16-web-apis/06-state-management/), [reference counting](/05-ownership/07-reference-counting/), [Arc/Mutex pattern](/11-async/12-arc-mutex-pattern/) — sharing state across handlers
- [Promises vs futures](/11-async/00-promises-vs-futures/) and [async/await](/11-async/01-async-await/) — the concurrency model shift
- [Serde attributes](/15-serialization/05-attributes/): keeping JSON shapes stable
- Companion pages in this section: [Incremental Migration](/29-migration-guide/00-incremental/), [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/), [Data Migration Strategies](/29-migration-guide/03-data-migration/), [Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/), [Common Migration Challenges](/29-migration-guide/05-common-challenges/)
- Official: [Axum docs](https://docs.rs/axum/latest/axum/), [Tokio tutorial](https://tokio.rs/tokio/tutorial), [Serde](https://serde.rs/)
- New to the project? Start at the [introduction](/00-introduction/), [getting started](/01-getting-started/), and [the basics](/02-basics/). Apply it all in the [capstone projects](/30-projects/).

---

## Exercises

### Exercise 1: Add a `DELETE` endpoint that matches Node

**Difficulty:** Beginner

**Objective:** Reproduce an idempotent delete that returns `204 No Content` whether or not the user existed — the behavior of a typical Express `res.status(204).end()` handler.

**Instructions:** Starting from the real-world example, add `DELETE /users/{id}`. Remove the user from the store if present, and return `204` in both cases (present and absent). Verify with `curl -i` that the status is `204` and the body is empty for an existing id, a missing id, and a repeated delete.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::delete,
};
use serde::Serialize;

#[derive(Clone, Serialize)]
struct User { id: u32, name: String, email: String }

#[derive(Clone)]
struct AppState { users: Arc<RwLock<HashMap<u32, User>>> }

// Idempotent: 204 whether or not the user was there, like res.status(204).end().
async fn delete_user(State(state): State<AppState>, Path(id): Path<u32>) -> StatusCode {
    state.users.write().unwrap().remove(&id);
    StatusCode::NO_CONTENT
}

fn seed() -> HashMap<u32, User> {
    let mut m = HashMap::new();
    m.insert(1, User { id: 1, name: "Ada".into(), email: "ada@example.com".into() });
    m
}

#[tokio::main]
async fn main() {
    let state = AppState { users: Arc::new(RwLock::new(seed())) };
    let app = Router::new()
        .route("/users/{id}", delete(delete_user))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3004").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Returning a bare `StatusCode` produces an empty body, which is what `204` requires. `curl -i` on an existing id, a missing id, and a repeated delete all show `HTTP/1.1 204 No Content` with no body.

</details>

### Exercise 2: Match Express's lenient `:id` parsing

**Difficulty:** Intermediate

**Objective:** Make `GET /users/{id}` return `404` (not `400`) for a non-numeric id, reproducing the Node behavior, *without* losing the `200`/`404` paths for valid numeric ids.

**Instructions:** Change the handler so a request to `/users/abc` yields `404 {"error":"User not found"}` while `/users/1` and `/users/99` keep returning `200` and `404` respectively. Explain in a comment why `Path<u32>` could not do this.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use serde_json::json;

#[derive(Clone, Serialize)]
struct User { id: u32, name: String, email: String }

#[derive(Clone)]
struct AppState { users: Arc<HashMap<u32, User>> }

// Path<u32> would reject "abc" with a 400 *before this runs*. Taking the
// segment as a String lets us parse it ourselves and fall through to 404,
// exactly like Number("abc") -> NaN -> Map.get(NaN) -> undefined in Express.
async fn get_user(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let user = id.parse::<u32>().ok().and_then(|n| state.users.get(&n));
    match user {
        Some(u) => Json(u.clone()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        )
            .into_response(),
    }
}

fn seed() -> HashMap<u32, User> {
    let mut m = HashMap::new();
    m.insert(1, User { id: 1, name: "Ada".into(), email: "ada@example.com".into() });
    m
}

#[tokio::main]
async fn main() {
    let state = AppState { users: Arc::new(seed()) };
    let app = Router::new()
        .route("/users/{id}", get(get_user))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3005").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`/users/abc` now returns `404 {"error":"User not found"}`; `/users/1` returns `200`; `/users/99` returns `404`. The key insight: typed extractors reject bad input at the boundary, so to opt into lenient parsing you must extract the raw `String` and parse inside the handler.

</details>

### Exercise 3: Centralize the error envelope and reject mismatched casing in a test

**Difficulty:** Advanced

**Objective:** Prove the migrated service preserves the JSON contract by asserting on the exact serialized bytes, and catch a casing regression.

**Instructions:** Suppose the Node service emitted `{"id":1,"fullName":"Ada","createdAt":"..."}`. Define a `User` DTO whose Rust fields are `id`, `full_name`, `created_at`, serialize it, and write a `#[test]` (or a `main` with `assert_eq!`) that fails unless the output keys are camelCase. Then make it pass with the right Serde attribute.

<details>
<summary>Solution</summary>

```rust playground
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct User {
    id: u32,
    full_name: String,
    created_at: String,
}

fn main() {
    let user = User {
        id: 1,
        full_name: "Ada".into(),
        created_at: "2026-06-02".into(),
    };
    let json = serde_json::to_string(&user).unwrap();
    // The contract from the Node service uses camelCase keys.
    assert_eq!(json, r#"{"id":1,"fullName":"Ada","createdAt":"2026-06-02"}"#);
    println!("contract held: {json}");
}
```

Output:

```text
contract held: {"id":1,"fullName":"Ada","createdAt":"2026-06-02"}
```

Remove the `#[serde(rename_all = "camelCase")]` line and the same `assert_eq!` fails, because Serde would emit `full_name`/`created_at`. Byte-level assertions like this are what turn "looks the same" into "is provably the same" during a migration. For richer field control, see [serde attributes](/15-serialization/05-attributes/) and [custom serialization](/15-serialization/07-custom-serialization/).

</details>
