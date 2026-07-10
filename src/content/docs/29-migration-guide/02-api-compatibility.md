---
title: "Maintaining API Compatibility During Migration"
description: "Keep a Rust endpoint byte-compatible with the Node.js one it replaces: Serde controls JSON casing, null-vs-omitted, and big ints; golden fixtures prove it."
---

When you replace a Node.js endpoint with a Rust one, the endpoint's *contract* must not change. Existing mobile apps, browser bundles, and downstream services keep talking to the same URL and expect the same bytes back: the same JSON keys, the same status codes, the same headers. This page is about making the Rust response indistinguishable from the Node response it replaces.

---

## Quick Overview

A migration is safe only if clients cannot tell the difference. That means three things must match byte-for-byte (or header-for-header):

- **JSON shape:** field names, casing, nesting, `null`-vs-omitted, number formats, date strings.
- **Status codes:** 200 vs 201 vs 204, and the exact error codes (400 vs 404 vs 422 vs 500).
- **Headers:** `Content-Type`, caching, `Location`, custom `X-*` headers, and their exact values.

The good news: Rust's `serde` gives you precise, declarative control over the wire format, and `axum` lets you set status and headers explicitly. The discipline that makes this work is **golden-fixture testing**: capture real responses from the Node service, then assert the Rust service reproduces them exactly.

> **Note:** This page assumes you have already chosen *what* to port (see [Incremental Migration](/29-migration-guide/00-incremental/)) and how a single endpoint moves from Express to Axum (see [Porting a Node.js Service to Rust](/29-migration-guide/01-node-to-rust/)). Here we focus narrowly on keeping the wire contract identical.

---

## TypeScript/JavaScript Example

Here is a representative Express handler. Notice every detail that a client might depend on: the casing of `fullName`, the omitted `avatarUrl`, the `X-Request-Id` header, the cache policy, and the shape of the error envelope.

```typescript
// src/routes/users.ts (Express + TypeScript, Node v22)
import { Router, Request, Response } from "express";

const router = Router();

interface UserDto {
  id: number;
  fullName: string;
  email: string;
  avatarUrl?: string; // omitted from JSON when undefined
  isActive: boolean;
}

router.get("/users/:id", (req: Request, res: Response) => {
  const id = Number(req.params.id);
  const user = findUser(id); // returns UserDto | null

  if (user === null) {
    // The error envelope every client already parses.
    return res.status(404).json({
      error: { code: "USER_NOT_FOUND", message: "User not found" },
    });
  }

  res
    .status(200)
    .set("X-Request-Id", req.header("X-Request-Id") ?? "req-abc")
    .set("Cache-Control", "private, max-age=30")
    .json(user);
});

function findUser(id: number): UserDto | null {
  if (id === 0) return null;
  return { id, fullName: "Ada Lovelace", email: "ada@example.com", isActive: true };
}

export default router;
```

A success returns:

```json
{ "id": 42, "fullName": "Ada Lovelace", "email": "ada@example.com", "isActive": true }
```

Note that `avatarUrl` is **absent**, not `null`. In JavaScript, `JSON.stringify` drops keys whose value is `undefined`. A naive Rust port that emits `"avatarUrl": null` would be a contract change.

---

## Rust Equivalent

The same handler in Axum. The `serde` attributes encode the wire contract directly on the type, so the format is enforced by the compiler and visible at a glance.

```rust
use axum::{
    Json, Router,
    extract::Path,
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use serde_json::json;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")] // full_name -> "fullName", is_active -> "isActive"
struct UserDto {
    id: u64,
    full_name: String,
    email: String,
    #[serde(skip_serializing_if = "Option::is_none")] // omit when None, like JS `undefined`
    avatar_url: Option<String>,
    is_active: bool,
}

async fn get_user(Path(id): Path<u64>) -> Response {
    let user = find_user(id);

    let Some(user) = user else {
        // Exact same error envelope the Node service produced.
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": { "code": "USER_NOT_FOUND", "message": "User not found" } })),
        )
            .into_response();
    };

    (
        StatusCode::OK,
        [
            (HeaderName::from_static("x-request-id"), HeaderValue::from_static("req-abc")),
            (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=30")),
        ],
        Json(user),
    )
        .into_response()
}

fn find_user(id: u64) -> Option<UserDto> {
    if id == 0 {
        return None;
    }
    Some(UserDto {
        id,
        full_name: "Ada Lovelace".to_string(),
        email: "ada@example.com".to_string(),
        avatar_url: None,
        is_active: true,
    })
}

pub fn router() -> Router {
    // Axum 0.8 uses `{id}` for path params, not `:id`.
    Router::new().route("/users/{id}", get(get_user))
}
```

Serializing a `UserDto { id: 42, .., avatar_url: None, .. }` produces exactly:

```json
{
  "id": 42,
  "fullName": "Ada Lovelace",
  "email": "ada@example.com",
  "isActive": true
}
```

The `avatarUrl` key is absent, matching the Express output byte-for-byte. This is real `cargo run` output from a probe project using `serde 1.0`, `serde_json 1.0`, and `axum 0.8`.

> **Note:** Axum 0.8 changed the path-parameter syntax from `:id` (Axum 0.7) to `{id}`. If you copy an older tutorial you will hit a routing error. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically.

---

## Detailed Explanation

### Field casing: `rename_all`

`serde` serializes Rust field names verbatim by default, so a `full_name` field becomes `"full_name"` in JSON: snake_case, which would break a JS client expecting `fullName`. The container attribute `#[serde(rename_all = "camelCase")]` rewrites every field's key. Without it:

```rust playground
use serde::Serialize;

#[derive(Serialize)]
struct AccountSnakeDefault {
    user_id: u64,
    is_active: bool,
}

fn main() {
    let a = AccountSnakeDefault { user_id: 7, is_active: true };
    println!("{}", serde_json::to_string(&a).unwrap());
}
```

Real output:

```text
{"user_id":7,"is_active":true}
```

That `user_id`/`is_active` shape silently breaks any client reading `userId`/`isActive`. `rename_all` is the single most common attribute you will reach for during a Node migration, because Node codebases almost always use camelCase JSON.

### `null` versus omitted

JavaScript and Rust disagree about absent values, and the difference is observable on the wire:

- JS `JSON.stringify({ a: undefined })` → `{}` (key dropped).
- JS `JSON.stringify({ a: null })` → `{"a":null}` (key present, value `null`).
- Rust `Option<T>` serializes `None` as `null` **by default**.

So a plain `avatar_url: Option<String>` would emit `"avatarUrl": null`, different from the Express `undefined` behavior. The fix is `#[serde(skip_serializing_if = "Option::is_none")]`, which omits the key entirely when the value is `None`. Choose deliberately per field: if the Node service emitted `null`, *keep* the field and do not add the skip attribute; if it omitted the key, add the skip attribute. They are not interchangeable.

### Numbers: the big-integer trap

JavaScript's `number` is always an IEEE-754 double (`f64`). Any integer above 2^53 cannot be represented exactly, and `JSON.parse` rounds it. This is a real loss, not theoretical:

```text
raw string : 9007199254740993
after parse: 9007199254740992
round-trip equal to original string: false
```

(Real Node v22 output — `9007199254740993` becomes `9007199254740992` after `JSON.parse`.)

If your Node service already sends large IDs as JSON **strings** to dodge this, your Rust service must do the same. An `i64` serializes as a JSON *number* by default, so you encode it as a string explicitly:

```rust playground
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    // Serialize the 64-bit id as a JSON string so JS clients never lose precision.
    #[serde(serialize_with = "as_string")]
    account_id: i64,
    balance_cents: i64,
}

fn as_string<S>(value: &i64, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(&value.to_string())
}

fn main() {
    let acct = Account { account_id: 9_007_199_254_740_993, balance_cents: 4999 };
    println!("{}", serde_json::to_string(&acct).unwrap());
}
```

Real output:

```text
{"accountId":"9007199254740993","balanceCents":4999}
```

The id is a quoted string; the `balanceCents`, which stays comfortably within the safe range, remains a number, matching whatever the Node service did field by field.

### Dates and times

Node usually serializes a `Date` with `toISOString()`, producing RFC 3339 strings like `"2026-06-02T10:00:00.000Z"`. With `chrono` (add `chrono` with the `serde` feature), a `DateTime<Utc>` serializes to RFC 3339 automatically:

```rust playground
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    created_at: DateTime<Utc>,
}

fn main() {
    let event = Event { created_at: "2026-06-02T10:00:00Z".parse().unwrap() };
    println!("{}", serde_json::to_string(&event).unwrap());
}
```

Real output:

```text
{"createdAt":"2026-06-02T10:00:00Z"}
```

> **Warning:** Watch the fractional-seconds detail. Node's `toISOString()` always includes milliseconds (`.000`), while `chrono`'s default RFC 3339 output omits them when they are zero. If a client does string comparison or strict schema validation, normalize the format on one side. You can force a specific layout with `created_at.to_rfc3339_opts(SecondsFormat::Millis, true)` inside a custom `serialize_with`.

### Status codes and headers in Axum

In Express you chain `res.status(200).set(...).json(...)`. In Axum you return a tuple that Axum turns into a response via the `IntoResponse` trait. The tuple convention is `(StatusCode, [headers], Json(body))`, and order matters: status first, headers next, body last. Each element must itself implement `IntoResponse` or be a recognized header collection.

Driving the handler in-process (via `tower`'s `oneshot`) gives reproducible, real output:

```text
GET /users/42 -> 200 OK
x-request-id: "req-abc"
cache-control: "private, max-age=30"
content-type: "application/json"
body: {"id":42,"fullName":"Ada Lovelace","email":"ada@example.com","isActive":true}
GET /users/0 -> 404 Not Found
body: {"error":{"code":"USER_NOT_FOUND","message":"User not found"}}
```

Two things to notice. First, `Json(...)` sets `Content-Type: application/json` for you, exactly as Express's `res.json()` does. You do not (and should not) set it manually. Second, the 404 branch returns the identical error envelope. The status line and the body are both part of the contract; matching one without the other is still a breaking change.

---

## Key Differences

| Concern | Express / Node | Axum / Rust |
| --- | --- | --- |
| Field casing | Whatever the object literal uses (usually camelCase) | snake_case by default; use `#[serde(rename_all = "camelCase")]` |
| Absent value | `undefined` → key dropped | `None` → `null` by default; add `skip_serializing_if` to drop |
| Explicit null | `null` → `{"k":null}` | `None` → `{"k":null}` (matches without extra attributes) |
| Large integers | All numbers are `f64`; precision lost > 2^53 | `i64`/`u64` exact; serialize as string to match a string-id contract |
| Dates | `Date.toISOString()` (always `.000` ms) | `chrono` RFC 3339 (omits zero ms by default) |
| Status code | `res.status(n)` | First element of the returned tuple, e.g. `StatusCode::CREATED` |
| Content-Type | `res.json()` sets `application/json` | `Json(...)` sets `application/json` |
| Custom headers | `res.set("X-Foo", v)` | `[(HeaderName::from_static("x-foo"), HeaderValue::from_static(v))]` |
| Header name case | Sent as written; HTTP/2 lowercases | Always stored lowercase internally |
| Contract enforcement | Runtime only; types erased | Declared on the type; checked at compile time |

The deepest difference is *where the contract lives*. In TypeScript the `interface UserDto` is erased at runtime — nothing stops you returning a differently-shaped object. In Rust the `serde` attributes are the serialization, so the wire format is welded to the type and verified when you compile. That is exactly the property you want during a migration: the format cannot drift by accident.

> **Tip:** Header names in HTTP are case-insensitive, and Axum normalizes them to lowercase. If you assert against `"X-Request-Id"` with exact casing in a test, look it up as `x-request-id`. Clients should already treat header names case-insensitively, but a strict test harness might not.

---

## Common Pitfalls

### Forgetting `rename_all`

The most frequent break. You port the struct, forget the container attribute, and every key ships as snake_case. The Rust compiles cleanly and the tests you forgot to write pass — but the mobile app shows blank fields. Always diff the first real response against a captured Node payload.

### Returning a tuple where a `Response` is expected

If a function's return type is `Response` but you return the convenience tuple in some branches, the types do not line up and Rust stops you:

```rust
use axum::{Json, http::StatusCode, response::Response};
use serde_json::json;

// does not compile (error[E0308]: mismatched types)
async fn handler(found: bool) -> Response {
    if found {
        (StatusCode::OK, Json(json!({ "ok": true })))
    } else {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "missing" })))
    }
}

fn main() {}
```

The real compiler error:

```text
error[E0308]: mismatched types
 --> src/main.rs:9:9
  |
7 | async fn handler(found: bool) -> Response {
  |                                  -------- expected `Response<Body>` because of return type
8 |     if found {
9 |         (StatusCode::OK, Json(json!({ "ok": true })))
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `Response<Body>`, found `(StatusCode, Json<Value>)`
  |
  = note: expected struct `Response<Body>`
              found tuple `(StatusCode, Json<Value>)`
```

The fix is to call `.into_response()` on each branch (as the Rust Equivalent above does), or change the return type to `impl IntoResponse` when every branch has the same tuple type. This compile error is a *feature*: it forces every branch to agree on the response type.

### `null` versus omitted, again

It is worth repeating because it is silent. Default `Option<T>` emits `null`; the Node code may have emitted nothing. The mismatch will not error — clients that do `if (user.avatarUrl)` keep working, but clients that do `"avatarUrl" in user` or schema-validate with `additionalProperties: false` will diverge. Decide per field and write a test that pins it.

### Trailing-slash and path-param mismatches

Express matches `/users/42` and `/users/42/` somewhat loosely depending on configuration. Axum is stricter and uses `{id}`, not `:id`. If clients call a trailing-slash variant, add explicit routes or a normalization layer rather than assuming parity.

### Deserialize errors are 422, syntax errors are 400

When a request body is valid JSON but the wrong *shape* — a missing field, a wrong type — Axum's `Json` extractor rejects it with `422 Unprocessable Entity` and a plain-text message, not your JSON error envelope. (A genuine JSON *syntax* error returns `400 Bad Request` instead.) Either way the underlying `serde` error is precise and useful (``missing field `fullName` at line 1 column 27``), but it is plain text — not the `422` JSON envelope your Node validation layer may have used — so you must catch the rejection and remap it to keep the contract. See the Best Practices and the Real-World example below.

---

## Best Practices

### Capture golden fixtures from the Node service

Before you delete a single line of Node, record real responses — status, headers, body — for representative requests. A few `curl -i` captures committed as files become your contract. Then assert the Rust service reproduces them. Compare bodies as parsed `serde_json::Value`, not as strings, so key ordering and whitespace do not cause false failures while still catching every shape difference:

```rust playground
use serde_json::json;

fn main() {
    // `actual` would come from your handler's response body in a real test.
    let actual: serde_json::Value =
        serde_json::from_str(r#"{"fullName":"Ada","id":42}"#).unwrap();

    // Field order differs, but the documents are equal.
    let golden = json!({ "id": 42, "fullName": "Ada" });

    assert_eq!(actual, golden);
    println!("contract holds");
}
```

Real output: `contract holds`. Parsed-value comparison ignores key order (JSON objects are unordered) but catches missing keys, extra keys, wrong casing, `null`-vs-omitted, and type changes.

### Centralize the error envelope with one `IntoResponse` type

Do not hand-write the error JSON at every call site; you will eventually get one wrong. Define a single error enum whose `IntoResponse` impl maps each variant to a status code and the shared envelope. Every handler returns it, so the format cannot drift:

```rust
use axum::{Json, http::StatusCode, response::{IntoResponse, Response}};
use serde_json::json;

enum ApiError {
    NotFound(String),
    Validation(String),
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::NotFound(what) => {
                (StatusCode::NOT_FOUND, "NOT_FOUND", format!("{what} not found"))
            }
            ApiError::Validation(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR", msg)
            }
            ApiError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL",
                "Internal server error".to_string(),
            ),
        };
        (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
    }
}

fn main() {
    for err in [
        ApiError::NotFound("User".into()),
        ApiError::Validation("email is required".into()),
        ApiError::Internal,
    ] {
        println!("status = {}", err.into_response().status());
    }
}
```

Real output:

```text
status = 404 Not Found
status = 422 Unprocessable Entity
status = 500 Internal Server Error
```

This is the single biggest lever for error-contract parity. Map your Node error codes onto variants once, and every endpoint inherits the right status and the right body.

### Stay backward-compatible on input with `#[serde(default)]`

When the Rust version adds an optional request field, old clients omit it. Mark it `#[serde(default)]` so their requests still parse:

```rust playground
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct CreateUser {
    full_name: String,
    email: String,
    #[serde(default)] // old clients omit `marketingOptIn`; defaults to false
    marketing_opt_in: bool,
}

fn main() {
    let body = r#"{"fullName":"Ada","email":"ada@example.com"}"#;
    let parsed: CreateUser = serde_json::from_str(body).unwrap();
    println!("{parsed:?}");
}
```

Real output:

```text
CreateUser { full_name: "Ada", email: "ada@example.com", marketing_opt_in: false }
```

### Be deliberate about unknown input fields

By default `serde` ignores unknown fields when deserializing, which is forgiving and usually what you want during a migration (a client sending an extra key keeps working). If you instead want to reject unexpected input, add `#[serde(deny_unknown_fields)]`, but only if your Node service also rejected them, or you will introduce a new failure mode that did not exist before.

### Match the format, not just the values

`Content-Type` charset (`application/json` vs `application/json; charset=utf-8`), header ordering, and cache directives are all part of what some clients and CDNs key on. When in doubt, capture the Node response with `curl -i` and diff the full header block.

---

## Real-World Example

A production-flavored order endpoint that ties everything together: camelCase keys, a big id serialized as a string, RFC 3339 dates, an omitted optional field, a custom cache header, a centralized error type, and a contract assertion against a golden fixture captured from the Node service it replaces.

```rust
use axum::{
    Json, Router,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderDto {
    id: String,
    #[serde(serialize_with = "as_string")] // 64-bit id as JSON string for JS safety
    account_id: i64,
    amount_cents: i64,
    currency: String,
    status: String,
    created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")] // omit, do not emit null
    coupon: Option<String>,
}

fn as_string<S: serde::Serializer>(v: &i64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}

enum ApiError {
    NotFound,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "ORDER_NOT_FOUND", "Order not found"),
        };
        (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
    }
}

async fn get_order(Path(id): Path<String>) -> Result<Response, ApiError> {
    if id != "ord_123" {
        return Err(ApiError::NotFound);
    }
    let order = OrderDto {
        id,
        account_id: 9_007_199_254_740_993,
        amount_cents: 4999,
        currency: "USD".to_string(),
        status: "paid".to_string(),
        created_at: "2026-06-02T10:00:00Z".parse().unwrap(),
        coupon: None,
    };
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=30"))],
        Json(order),
    )
        .into_response())
}

pub fn router() -> Router {
    Router::new().route("/orders/{id}", get(get_order))
}

#[tokio::main]
async fn main() {
    use tower::ServiceExt; // for `oneshot`

    let resp = router()
        .oneshot(
            axum::http::Request::builder()
                .uri("/orders/ord_123")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let cache = resp.headers().get("cache-control").cloned();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let actual: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Golden contract fixture captured from the Node service being replaced.
    let golden = json!({
        "id": "ord_123",
        "accountId": "9007199254740993",
        "amountCents": 4999,
        "currency": "USD",
        "status": "paid",
        "createdAt": "2026-06-02T10:00:00Z"
    });

    println!("status: {status}");
    println!("cache-control: {cache:?}");
    println!("body matches golden fixture: {}", actual == golden);
}
```

`Cargo.toml` dependencies (resolved with `cargo add`):

```toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tower = { version = "0.5", features = ["util"] } # oneshot for in-process tests
```

Real output:

```text
status: 200 OK
cache-control: Some("private, max-age=30")
body matches golden fixture: true
```

The `oneshot` call drives the router in-process, so this same pattern becomes a `#[tokio::test]`: build a request, call `oneshot`, and assert the status, headers, and parsed body against fixtures. Run it against every endpoint you port and the contract is guarded by CI. For more on testing Axum services, see [Web APIs](/16-web-apis/) and [Error Handling](/08-error-handling/).

---

## Further Reading

- [Serde derive: container & field attributes](https://serde.rs/attributes.html) — `rename_all`, `skip_serializing_if`, `default`, `deny_unknown_fields`, and `serialize_with`.
- [Serde field renaming](https://serde.rs/field-attrs.html): per-field control when one or two keys deviate from the casing rule.
- [Axum `IntoResponse` documentation](https://docs.rs/axum/latest/axum/response/index.html) — how tuples of `(StatusCode, headers, body)` become responses.
- [`http::StatusCode`](https://docs.rs/http/latest/http/status/struct.StatusCode.html) and [`http::header`](https://docs.rs/http/latest/http/header/index.html) — the canonical status and header constants.
- [chrono serde integration](https://docs.rs/chrono/latest/chrono/serde/index.html): controlling timestamp formats on the wire.
- Guide cross-links: [Serialization](/15-serialization/) for the full serde story; [Web APIs](/16-web-apis/) for building Axum services; [Error Handling](/08-error-handling/) for error types.
- Siblings in this section: [Incremental Migration](/29-migration-guide/00-incremental/) (what to port first), [Porting a Node.js Service to Rust](/29-migration-guide/01-node-to-rust/) (the Express→Axum walkthrough), [Data Migration Strategies](/29-migration-guide/03-data-migration/) (keeping the database in sync), [Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/) (measuring the payoff honestly), and [Common Migration Challenges](/29-migration-guide/05-common-challenges/) (the human and ecosystem hurdles).
- Apply this end-to-end in [Projects](/30-projects/).

---

## Exercises

### Exercise 1: Match an exact JSON shape

**Difficulty:** Beginner

**Objective:** Produce JSON that is byte-for-byte identical to a captured Node payload.

**Instructions:** A Node endpoint returns this body for a product:

```json
{ "productId": "p_99", "displayName": "Keyboard", "priceCents": 7999, "inStock": true }
```

Define a Rust struct with `#[derive(Serialize)]` and the right `serde` attributes so that serializing it produces exactly those keys (camelCase) in a struct whose Rust fields are snake_case. Print the result and confirm it matches.

<details>
<summary>Solution</summary>

```rust playground
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Product {
    product_id: String,
    display_name: String,
    price_cents: i64,
    in_stock: bool,
}

fn main() {
    let product = Product {
        product_id: "p_99".to_string(),
        display_name: "Keyboard".to_string(),
        price_cents: 7999,
        in_stock: true,
    };

    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&product).unwrap()).unwrap();

    let golden: serde_json::Value = serde_json::from_str(
        r#"{ "productId": "p_99", "displayName": "Keyboard", "priceCents": 7999, "inStock": true }"#,
    )
    .unwrap();

    println!("{}", serde_json::to_string(&product).unwrap());
    assert_eq!(actual, golden);
    println!("matches golden fixture");
}
```

Real output:

```text
{"productId":"p_99","displayName":"Keyboard","priceCents":7999,"inStock":true}
matches golden fixture
```

`#[serde(rename_all = "camelCase")]` does all the work; comparing parsed `Value`s makes the assertion order-insensitive.

</details>

### Exercise 2: Reproduce a `201 Created` with a `Location` header

**Difficulty:** Intermediate

**Objective:** Match a Node create endpoint's status code and headers along with its body.

**Instructions:** A Node endpoint handles `POST /articles`. On success it does `res.status(201).location('/articles/' + id).json(article)`. Write an Axum handler that accepts `{ "title": "...", "tags": [...] }` (tags optional), returns `201 Created`, sets a `Location` header pointing at the new resource, and returns the created article as camelCase JSON. Drive it with `oneshot` and print the status, the `Location` header, and the body.

<details>
<summary>Solution</summary>

```rust
use axum::{
    Json, Router,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewArticle {
    title: String,
    #[serde(default)] // old clients may omit tags entirely
    tags: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Article {
    id: u64,
    title: String,
    tags: Vec<String>,
}

async fn create_article(Json(body): Json<NewArticle>) -> Response {
    let id = 101; // a real handler would insert and get the generated id
    let article = Article { id, title: body.title, tags: body.tags };
    let location = format!("/articles/{id}");
    (
        StatusCode::CREATED,
        [(header::LOCATION, HeaderValue::from_str(&location).unwrap())],
        Json(article),
    )
        .into_response()
}

fn router() -> Router {
    Router::new().route("/articles", post(create_article))
}

#[tokio::main]
async fn main() {
    use tower::ServiceExt;
    let resp = router()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/articles")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"title":"Hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    println!("status: {}", resp.status());
    println!("location: {:?}", resp.headers().get("location").unwrap());
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    println!("body: {}", String::from_utf8_lossy(&body));
}
```

Real output:

```text
status: 201 Created
location: "/articles/101"
body: {"id":101,"title":"Hello","tags":[]}
```

`#[serde(default)]` on `tags` lets a request omit the field; the empty `Vec` serializes back as `[]`, matching what the Node default would produce. `HeaderValue::from_str` is used (not `from_static`) because the value is built at runtime.

</details>

### Exercise 3: Match a TypeScript discriminated union

**Difficulty:** Advanced

**Objective:** Reproduce a tagged-union JSON shape that a TypeScript client expects.

**Instructions:** A Node service returns a payment method as a discriminated union:

```typescript
type PaymentMethod =
  | { type: "card"; last4: string }
  | { type: "paypal"; email: string };
```

Model this in Rust so that serializing it produces JSON with a `"type"` discriminator key, e.g. `{ "type": "card", "last4": "4242" }` and `{ "type": "paypal", "email": "..." }`. Serialize a list containing one of each variant and print the result.

<details>
<summary>Solution</summary>

```rust playground
use serde::Serialize;

// `tag = "type"` makes serde emit an internally-tagged enum,
// matching the TS discriminated union with a `type` discriminator.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PaymentMethod {
    Card { last4: String },
    Paypal { email: String },
}

fn main() {
    let methods = vec![
        PaymentMethod::Card { last4: "4242".to_string() },
        PaymentMethod::Paypal { email: "ada@example.com".to_string() },
    ];
    println!("{}", serde_json::to_string_pretty(&methods).unwrap());
}
```

Real output:

```text
[
  {
    "type": "card",
    "last4": "4242"
  },
  {
    "type": "paypal",
    "email": "ada@example.com"
  }
]
```

`#[serde(tag = "type")]` selects the internally-tagged representation — the variant name becomes the value of the `type` key, and `rename_all = "snake_case"` lowercases `Card`/`Paypal` to `card`/`paypal` to match the TypeScript string literals. The same enum can `#[derive(Deserialize)]` too, so requests in this shape parse back into the right variant.

</details>
