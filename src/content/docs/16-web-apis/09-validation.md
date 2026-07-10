---
title: "Request Validation"
description: "Shape-correct isn't valid. Rust's validator crate plays the role of Zod: derive Validate, enforce business rules, and return a structured 422 on failure."
---

In Express.js you reach for a library like Zod, Joi, or `express-validator` and wire it up as middleware that runs before your handler. Axum's [extractors](/16-web-apis/04-extractors/) already guarantee a request *deserialized* into the right shape, but "shape-correct" is not the same as "valid". This page covers the next layer: enforcing business rules (a name is 2-50 characters, an email is well-formed, an age is in range) and returning a `400`/`422` with a helpful, structured body when those rules are broken.

---

## Quick Overview

Deserialization answers "is this the right *type*?"; validation answers "is this *acceptable*?". A JSON body can parse perfectly into a `CreateUser { name: String, age: u8 }` and still be garbage: an empty name, an email with no `@`, an age of `0`. In Rust the idiomatic tool is the **`validator`** crate, which adds a `#[derive(Validate)]` macro and field attributes (`length`, `email`, `range`, `custom`, `nested`) that feel a lot like Zod's schema methods. You then call `.validate()` and translate any `ValidationErrors` into a clean JSON response.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. This page targets **axum 0.8** and **validator 0.20**.

> **Note:** Axum returns **422 Unprocessable Entity** (not 400) when a JSON body has the right syntax but the wrong shape: e.g. a missing field. We follow the same convention for *semantic* validation failures, and reserve **400 Bad Request** for malformed input (broken JSON, wrong `Content-Type`). Both are perfectly valid choices; pick one and be consistent.

---

## TypeScript/JavaScript Example

A typical Express handler validates with Zod, then maps any error into a `400` with a per-field breakdown.

```typescript
// Express.js + Zod
import express, { Request, Response } from "express";
import { z } from "zod";

const app = express();
app.use(express.json());

const CreateUser = z.object({
  name: z.string().min(2).max(50),
  email: z.string().email(),
  age: z.number().int().min(18).max(120),
  username: z.string().regex(/^[A-Za-z0-9_]+$/, "letters, numbers, underscores only"),
});

type CreateUser = z.infer<typeof CreateUser>;

app.post("/users", (req: Request, res: Response) => {
  const result = CreateUser.safeParse(req.body);

  if (!result.success) {
    // Map Zod issues into { field: [messages] }
    const details: Record<string, string[]> = {};
    for (const issue of result.error.issues) {
      const field = issue.path.join(".");
      (details[field] ??= []).push(issue.message);
    }
    return res.status(400).json({ error: "validation failed", details });
  }

  const user: CreateUser = result.data; // fully typed AND validated
  res.status(201).json({ name: user.name });
});

app.listen(3000);
```

**Key points:**

- The schema is a runtime value; `safeParse` either returns typed `data` or an `error` you inspect.
- `z.infer` derives the static TypeScript type from the schema, so the type and the runtime checks stay in sync.
- You manually walk `error.issues` to build a client-friendly response. There is no single canonical shape; every team invents its own.

---

## Rust Equivalent

The `validator` crate plays the role of Zod. You annotate the struct that your [`Json` extractor](/16-web-apis/04-extractors/) already produces, then call `.validate()`. Here we wrap both steps in a custom **`ValidatedJson<T>`** extractor so handlers stay clean. The request never reaches the handler body unless it parsed *and* validated.

Add the dependencies in a new project:

```bash
cargo new user-api && cd user-api
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json
cargo add validator --features derive
```

```rust
use std::collections::HashMap;

use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use validator::{Validate, ValidationError, ValidationErrors};

#[derive(Debug, Deserialize, Validate)]
struct CreateUser {
    #[validate(length(min = 2, max = 50, message = "name must be 2-50 characters"))]
    name: String,

    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(range(min = 18, max = 120, message = "age must be between 18 and 120"))]
    age: u8,

    #[validate(custom(function = "validate_username"))]
    username: String,
}

// A custom validator: takes &str, returns Result<(), ValidationError>.
fn validate_username(username: &str) -> Result<(), ValidationError> {
    if username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        let mut err = ValidationError::new("username_charset");
        err.message = Some("username may only contain letters, numbers, and underscores".into());
        Err(err)
    }
}

// The JSON shape we return on failure: { "error": ..., "details": { field: [msgs] } }
#[derive(Serialize)]
struct ValidationProblem {
    error: &'static str,
    details: HashMap<String, Vec<String>>,
}

fn to_problem(errors: &ValidationErrors) -> ValidationProblem {
    let mut details: HashMap<String, Vec<String>> = HashMap::new();
    for (field, errs) in errors.field_errors() {
        let messages = errs
            .iter()
            .map(|e| {
                // Prefer our human message; fall back to the machine code.
                e.message
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| e.code.to_string())
            })
            .collect();
        details.insert(field.to_string(), messages);
    }
    ValidationProblem { error: "validation failed", details }
}

// A reusable extractor: parse JSON, THEN validate, before the handler runs.
struct ValidatedJson<T>(T);

enum ApiError {
    JsonRejection(JsonRejection), // bad/missing JSON  -> 400/415/422 from axum
    Validation(ValidationErrors), // shape OK, rules broken -> 422
}

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(ApiError::JsonRejection)?;
        value.validate().map_err(ApiError::Validation)?;
        Ok(ValidatedJson(value))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::JsonRejection(rejection) => (
                rejection.status(), // axum picks 400 / 415 / 422 as appropriate
                Json(json!({ "error": rejection.body_text() })),
            )
                .into_response(),
            ApiError::Validation(errors) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(to_problem(&errors))).into_response()
            }
        }
    }
}

// Handlers receive already-valid data. No validation noise here.
async fn create_user(ValidatedJson(user): ValidatedJson<CreateUser>) -> impl IntoResponse {
    (StatusCode::CREATED, Json(json!({ "name": user.name })))
}

fn app() -> Router {
    Router::new().route("/users", post(create_user))
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

Sending an invalid body returns a real `422` with a per-field breakdown. Hitting the running server confirms it:

```bash
curl -s -i -X POST http://127.0.0.1:3000/users \
  -H 'content-type: application/json' \
  -d '{"name":"x","email":"nope","age":9,"username":"bad name!"}'
```

The verified response status and body (captured from an end-to-end request against the server above):

```text
STATUS 422 Unprocessable Entity
{
  "details": {
    "age": [
      "age must be between 18 and 120"
    ],
    "email": [
      "must be a valid email address"
    ],
    "name": [
      "name must be 2-50 characters"
    ],
    "username": [
      "username may only contain letters, numbers, and underscores"
    ]
  },
  "error": "validation failed"
}
```

A valid body sails through to the handler:

```text
STATUS 201 Created
{"name":"Ada Lovelace"}
```

---

## Detailed Explanation

**The attributes mirror Zod methods.** Each `#[validate(...)]` declares one rule on one field:

| validator attribute | Zod equivalent | Checks |
| --- | --- | --- |
| `length(min = 2, max = 50)` | `.min(2).max(50)` | string/collection length |
| `email` | `.email()` | HTML5-style email shape |
| `range(min = 18, max = 120)` | `.int().min(18).max(120)` | numeric bounds |
| `url` | `.url()` | parseable URL |
| `must_match(other = "field")` | `.refine(...)` | two fields are equal |
| `contains(pattern = "x")` | `.includes("x")` | substring present |
| `custom(function = "fn")` | `.refine(fn, msg)` | arbitrary logic |
| `nested` | nested `z.object` | validates a child struct |
| `required` | non-optional field | `Option` is `Some` |

**`#[validate(custom(function = "validate_username"))]`** points at a free function with the exact signature `fn(&T) -> Result<(), ValidationError>`. Returning `Err(ValidationError::new("code"))` records a failure; setting `.message` gives the client a human-readable string. This is where any rule the built-in attributes can't express lives (checking a value against a database, cross-field math, format rules).

> **Warning:** The custom-function syntax changed in newer `validator` releases. The current form is `custom(function = "name")`. The older `custom = "name"` no longer compiles — see [Common Pitfalls](#common-pitfalls).

**`value.validate()`** comes from the `Validate` trait that `#[derive(Validate)]` implements. It returns `Result<(), ValidationErrors>`. On success you have a value you *know* is valid; on failure you get a structured `ValidationErrors`.

**`errors.field_errors()`** returns a map of field name to a slice of `ValidationError`. Each `ValidationError` carries a machine `code` (`"length"`, `"email"`, `"range"`, or your custom code) and an optional human `message`. We prefer the message and fall back to the code, then collect into `{ field: [messages] }`: exactly the shape the Zod example produced. Standardizing this in one `to_problem` helper means every endpoint returns identical-looking errors.

**The `ValidatedJson<T>` extractor** is the real payoff. By implementing [`FromRequest`](/16-web-apis/04-extractors/) for it, we run *parse-then-validate* before the handler. A handler signature of `ValidatedJson(user): ValidatedJson<CreateUser>` is a compile-time promise that `user` is valid: the type system carries the guarantee, so handler bodies never re-check. This is closer to what Zod gives TypeScript than scattered `if` statements, and it composes with every route.

**`JsonRejection` vs `ValidationErrors`.** These are two genuinely different failure modes. `JsonRejection` fires when the bytes are not valid JSON, a required field is missing, a field has the wrong type, or the `Content-Type` header is wrong. Axum already produced a sensible status (400, 415, or 422) and message, so we forward `rejection.status()` and `rejection.body_text()`. `ValidationErrors` fires only after a *successful* parse, so it is always our `422`.

---

## Key Differences

| Concept | TypeScript (Zod) | Rust (validator) |
| --- | --- | --- |
| Schema location | A runtime value (`z.object({...})`) | Attributes on a compile-time `struct` |
| Type ↔ rules sync | `z.infer` derives the type from the schema | The struct *is* the type; attributes annotate it |
| Run validation | `schema.safeParse(data)` | `data.validate()` |
| Result on failure | `error.issues[]` | `ValidationErrors` (map of field -> errors) |
| Custom rule | `.refine(fn, msg)` | `fn(&T) -> Result<(), ValidationError>` |
| Where it runs | Middleware or top of handler | A reusable `FromRequest` extractor |
| Default failure status | Whatever you write (often 400) | You choose; 422 is idiomatic for shape/semantic errors |

**The schema is the type, not a separate object.** In Zod the schema is a value you can compose, pass around, and `.partial()`. In Rust the *struct* is the source of truth and validation rules are attributes baked onto it at compile time. You cannot build a validator dynamically at runtime the way you compose Zod schemas — but you also cannot accidentally let the type and the rules drift apart, because there is only one declaration.

**Validation is opt-in and explicit.** Deriving `Validate` does nothing on its own; *something* must call `.validate()`. There is no "this struct is automatically validated whenever it's deserialized." The `ValidatedJson<T>` extractor is how you make that automatic at the boundary: a deliberate seam, unlike a Zod schema that only validates where you remember to call it.

**Parsing and validating are separate phases.** Serde gives you a well-typed value or a parse error; `validator` then judges that value. In Zod both happen in one `safeParse` call. Keeping them separate is why a malformed body and a business-rule violation can return different statuses with almost no extra code.

---

## Common Pitfalls

### Using the old `custom = "..."` syntax

Older tutorials and `validator` versions used `#[validate(custom = "fn_name")]`. In validator 0.20 that is a hard compile error. This snippet does **not** compile:

```rust
use serde::Deserialize;
use validator::{Validate, ValidationError};

#[derive(Deserialize, Validate)]
struct Form {
    #[validate(custom = "validate_username")] // does not compile (Unexpected type `string`)
    username: String,
}

fn validate_username(_: &str) -> Result<(), ValidationError> { Ok(()) }
fn main() {}
```

The real compiler error is:

```text
error: Unexpected type `string`
 --> src/main.rs:6:25
  |
6 |     #[validate(custom = "validate_username")]
  |                         ^^^^^^^^^^^^^^^^^^^^
```

Use `custom(function = "validate_username")` instead. The function name is still a string literal, but it must sit inside `function = ...`.

### Deriving `Validate` but never calling `.validate()`

```rust
#[derive(serde::Deserialize, validator::Validate)]
struct CreateUser {
    #[validate(email)]
    email: String,
}

// A plain `Json<CreateUser>` parses but is NEVER validated:
async fn handler(axum::Json(_user): axum::Json<CreateUser>) {
    // `email` could be "not-an-email" here. The derive did nothing on its own.
}
```

This compiles and runs, accepting invalid emails. The derive only *generates* a `validate` method — nothing calls it for you. Use a `ValidatedJson<T>` extractor (as above) or call `user.validate()?` at the top of the handler. There is no runtime warning; the bug is silent, which is exactly why the extractor pattern is worth the boilerplate.

### Returning the raw `ValidationErrors` debug output to clients

`ValidationErrors` implements `Serialize`, so it is tempting to do `Json(errors)`. But its serialized form leaks internal `code`/`params` details and, for nested structs, a deeply nested object that is awkward to consume:

```text
{
  "quantity": [
    {
      "code": "range",
      "message": "quantity must be at least 1",
      "params": { "min": 1, "value": 0 }
    }
  ],
  "shipping": {
    "country": [
      {
        "code": "length",
        "message": "country must be a 2-letter code",
        "params": { "value": "USA", "equal": 2 }
      }
    ]
  }
}
```

(That output is real — it comes from serializing a `ValidationErrors` for a struct using `#[validate(nested)]`.) It exposes implementation details and an unstable shape. Map it through a small DTO like `ValidationProblem` so the client contract is *yours*, not the crate's.

### Confusing a parse failure with a validation failure

A request like `{"name":"Ada Lovelace","email":"ada@example.com"}` (missing `age`) never reaches `.validate()`; serde rejects it first. Against the server above it returns:

```text
STATUS 422 Unprocessable Entity
{"error":"Failed to deserialize the JSON body into the target type: missing field `age` at line 1 column 49"}
```

And a request with no JSON `Content-Type` is rejected even earlier:

```text
STATUS 415 Unsupported Media Type
{"error":"Expected request with `Content-Type: application/json`"}
```

If you only handle `ValidationErrors`, these cases fall through to a generic `500`. Handle the `JsonRejection` arm too (the `ApiError` enum above does), or wire a centralized error type; see [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/).

### Forgetting `#[validate(nested)]` on a child struct

If `Order` contains `shipping: Address` and you omit `#[validate(nested)]` on that field, `order.validate()` checks the `Order`'s own fields but silently skips the `Address`. It compiles and passes invalid children. Always annotate sub-structs you want recursively validated.

---

## Best Practices

- **Validate at the boundary, once.** A single `ValidatedJson<T>` (or `ValidatedQuery<T>`) extractor means handlers receive types that are *provably* valid. Don't re-validate deeper in the call stack.
- **Own your error contract.** Map `ValidationErrors` into a stable DTO (`{ "error": ..., "details": { field: [msg] } }`). Document it. Clients and your frontend depend on its shape far more than on any single endpoint.
- **Write helpful `message`s.** A bare `code` like `"length"` tells the user nothing. `"name must be 2-50 characters"` does. Set `message = "..."` on every attribute, and on custom errors.
- **Pick a status and stick to it.** `422 Unprocessable Entity` for semantic/shape failures, `400 Bad Request` for malformed input, `415 Unsupported Media Type` for the wrong content type. Consistency beats cleverness.
- **Reach for the validator crate before hand-rolling.** Manual `if` checks (shown below) are fine for one or two fields, but they don't compose, don't standardize messages, and grow into a swamp. Use `validator` for anything beyond trivial.
- **Keep custom validators pure and synchronous.** `validator`'s `custom` functions are not async and shouldn't touch a database. Do async checks (uniqueness, existence) in the handler after structural validation passes; see [JSON REST APIs](/16-web-apis/08-json-apis/) and [Database](/17-database/).
- **Trim/normalize before validating** when needed (e.g. lowercasing an email) so `"  Ada@Example.com "` and `"ada@example.com"` are treated the same.

### When manual validation is enough

For a tiny payload you don't need a crate at all. A function returning `Result<(), Vec<String>>` keeps full control:

```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Signup {
    email: String,
    password: String,
}

fn validate_signup(input: &Signup) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if !input.email.contains('@') {
        errors.push("email must contain @".to_string());
    }
    if input.password.len() < 8 {
        errors.push("password must be at least 8 characters".to_string());
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

async fn signup(Json(input): Json<Signup>) -> Response {
    match validate_signup(&input) {
        Ok(()) => (StatusCode::CREATED, Json(json!({ "ok": true }))).into_response(),
        Err(errors) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "validation failed", "details": errors })),
        )
            .into_response(),
    }
}
```

> **Tip:** Collect *all* errors and return them together (as above), rather than bailing on the first one. Returning one error at a time forces the client into a frustrating round-trip-per-mistake loop; both Zod and the `validator` crate return the full set by default.

---

## Real-World Example

A registration endpoint with realistic rules: name length, a valid email, a password of at least 8 characters that must match a confirmation field, and a custom check that the username has an allowed charset. It uses the `ValidatedJson<T>` extractor so the handler body is pure business logic.

```rust
use std::collections::HashMap;

use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use validator::{Validate, ValidationError, ValidationErrors};

#[derive(Debug, Deserialize, Validate)]
struct Register {
    #[validate(length(min = 2, max = 50, message = "name must be 2-50 characters"))]
    name: String,

    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(custom(function = "validate_username"))]
    username: String,

    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    password: String,

    #[validate(must_match(other = "password", message = "passwords do not match"))]
    password_confirm: String,
}

fn validate_username(username: &str) -> Result<(), ValidationError> {
    let ok = !username.is_empty()
        && username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        let mut err = ValidationError::new("username_charset");
        err.message = Some("username may only contain letters, numbers, and underscores".into());
        Err(err)
    }
}

#[derive(Serialize)]
struct ValidationProblem {
    error: &'static str,
    details: HashMap<String, Vec<String>>,
}

fn to_problem(errors: &ValidationErrors) -> ValidationProblem {
    let mut details: HashMap<String, Vec<String>> = HashMap::new();
    for (field, errs) in errors.field_errors() {
        let messages = errs
            .iter()
            .map(|e| {
                e.message
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| e.code.to_string())
            })
            .collect();
        details.insert(field.to_string(), messages);
    }
    ValidationProblem { error: "validation failed", details }
}

struct ValidatedJson<T>(T);

enum ApiError {
    JsonRejection(JsonRejection),
    Validation(ValidationErrors),
}

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(ApiError::JsonRejection)?;
        value.validate().map_err(ApiError::Validation)?;
        Ok(ValidatedJson(value))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::JsonRejection(rejection) => (
                rejection.status(),
                Json(json!({ "error": rejection.body_text() })),
            )
                .into_response(),
            ApiError::Validation(errors) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(to_problem(&errors))).into_response()
            }
        }
    }
}

async fn register(ValidatedJson(input): ValidatedJson<Register>) -> impl IntoResponse {
    // Reached only when every rule passed. In a real app, async checks
    // (is the email already taken?) and the INSERT happen here.
    (
        StatusCode::CREATED,
        Json(json!({ "username": input.username, "email": input.email })),
    )
}

fn app() -> Router {
    Router::new().route("/register", post(register))
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

A mismatched-password request:

```bash
curl -s -X POST http://127.0.0.1:3000/register \
  -H 'content-type: application/json' \
  -d '{"name":"Ada","email":"ada@example.com","username":"ada_l","password":"hunter22","password_confirm":"hunter99"}'
```

returns `422` with `{"error":"validation failed","details":{"password_confirm":["passwords do not match"]}}`, while a fully-valid body returns `201` with the created user. The handler itself contains zero validation code. That is the goal.

> **Note:** `must_match` compares `password_confirm` against `password`, and the failure is reported on `password_confirm`. Order your fields so the confirmation comes after the original.

---

## Further Reading

- [`validator` crate docs](https://docs.rs/validator/latest/validator/) — the full attribute list, `Validate` trait, and `ValidationErrors` structure.
- [`validator` derive attributes](https://docs.rs/validator/latest/validator/derive.Validate.html) — exact syntax for `length`, `range`, `email`, `must_match`, `custom`, `nested`, and more.
- [axum `JsonRejection`](https://docs.rs/axum/latest/axum/extract/rejection/enum.JsonRejection.html) — every way a `Json` extractor can fail and the status each produces.
- [HTTP 422 Unprocessable Content (MDN)](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/422) — when to use 422 vs 400.

Within this guide:

- [Extractors](/16-web-apis/04-extractors/) — `FromRequest`/`FromRequestParts`, how `Json` parses, and the extractor pattern `ValidatedJson<T>` builds on.
- [Request and Response Handling](/16-web-apis/07-request-response/) — `IntoResponse`, `(StatusCode, Json)` tuples, and setting status codes.
- [JSON REST APIs](/16-web-apis/08-json-apis/) — a full CRUD resource; where validated input gets persisted.
- [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/) — folding validation, parse, and database failures into one `AppError` with `thiserror`.
- [Middleware and Layers](/16-web-apis/05-middleware/) — when a tower layer is a better home for cross-cutting concerns than per-handler logic.
- Foundations: [error handling](/08-error-handling/) (`Result`, `?`), [serialization](/15-serialization/) (serde derive that powers parsing), the language [basics](/02-basics/), and [getting started](/01-getting-started/) / [introduction](/00-introduction/).
- Persisting validated data and doing async uniqueness checks: [Database](/17-database/).

---

## Exercises

### Exercise 1: Validate a query string

**Difficulty:** Beginner

**Objective:** Apply `#[derive(Validate)]` to a pagination struct and reject out-of-range values.

**Instructions:** Define a `Pagination { page: u32, per_page: u32 }` struct. Require `page >= 1` and `per_page` between `1` and `100` using `range`. Write a function `check(p: &Pagination) -> Result<(), validator::ValidationErrors>` that validates it. Test it against `{ page: 0, per_page: 500 }` and confirm both fields fail.

<details>
<summary>Solution</summary>

```rust
use validator::{Validate, ValidationErrors};

#[derive(Debug, Validate)]
struct Pagination {
    #[validate(range(min = 1, message = "page must be at least 1"))]
    page: u32,
    #[validate(range(min = 1, max = 100, message = "per_page must be 1-100"))]
    per_page: u32,
}

fn check(p: &Pagination) -> Result<(), ValidationErrors> {
    p.validate()
}

fn main() {
    let bad = Pagination { page: 0, per_page: 500 };
    match check(&bad) {
        Ok(()) => println!("valid"),
        Err(e) => {
            for (field, errs) in e.field_errors() {
                for err in errs {
                    let msg = err
                        .message
                        .as_ref()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| err.code.to_string());
                    println!("{field}: {msg}");
                }
            }
        }
    }
}
```

Running it prints (field order may vary):

```text
page: page must be at least 1
per_page: per_page must be 1-100
```

</details>

### Exercise 2: A custom cross-field validator

**Difficulty:** Intermediate

**Objective:** Use a struct-level custom validator to enforce a rule spanning two fields.

**Instructions:** Define a `DateRange { start: u32, end: u32 }` (days since epoch, say). The built-in attributes can't express "end must be after start", so write a struct-level validator. Apply `#[validate(schema(function = "validate_range"))]` to the struct and implement `validate_range(value: &DateRange) -> Result<(), ValidationError>`. Reject `{ start: 10, end: 5 }`.

<details>
<summary>Solution</summary>

```rust
use validator::{Validate, ValidationError};

#[derive(Debug, Validate)]
#[validate(schema(function = "validate_range"))]
struct DateRange {
    start: u32,
    end: u32,
}

fn validate_range(range: &DateRange) -> Result<(), ValidationError> {
    if range.end > range.start {
        Ok(())
    } else {
        let mut err = ValidationError::new("date_order");
        err.message = Some("end must be after start".into());
        Err(err)
    }
}

fn main() {
    let bad = DateRange { start: 10, end: 5 };
    match bad.validate() {
        Ok(()) => println!("valid"),
        Err(e) => println!("invalid: {e}"),
    }

    let good = DateRange { start: 5, end: 10 };
    println!("good is_ok: {}", good.validate().is_ok());
}
```

Running it prints:

```text
invalid: __all__: end must be after start
good is_ok: true
```

Struct-level errors are recorded under the special `__all__` key; map that to a top-level message in your API response rather than to a single field.

</details>

### Exercise 3: A `ValidatedJson<T>` extractor with a clean error body

**Difficulty:** Advanced

**Objective:** Build the reusable extractor end-to-end and prove it returns a `422` with a per-field map.

**Instructions:** Define `ValidatedJson<T>` implementing `FromRequest` (parse with `Json::<T>::from_request`, then `.validate()`). On a `ValidationErrors`, respond `422` with body `{ "error": "validation failed", "details": { field: [messages] } }`; on a `JsonRejection`, forward its status and text. Wire it to a `POST /products` route taking a `Product { name (1-100 chars), price (range 1..=1_000_000) }`, run the server, and POST an invalid product to confirm the response.

<details>
<summary>Solution</summary>

Dependencies: `cargo add axum tokio --features tokio/full`, then `cargo add serde --features derive`, `cargo add serde_json`, `cargo add validator --features derive`.

```rust
use std::collections::HashMap;

use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use validator::{Validate, ValidationErrors};

#[derive(Debug, Deserialize, Validate)]
struct Product {
    #[validate(length(min = 1, max = 100, message = "name must be 1-100 characters"))]
    name: String,
    #[validate(range(min = 1, max = 1_000_000, message = "price must be 1-1000000"))]
    price: u32,
}

#[derive(Serialize)]
struct ValidationProblem {
    error: &'static str,
    details: HashMap<String, Vec<String>>,
}

fn to_problem(errors: &ValidationErrors) -> ValidationProblem {
    let mut details = HashMap::new();
    for (field, errs) in errors.field_errors() {
        let messages = errs
            .iter()
            .map(|e| {
                e.message
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| e.code.to_string())
            })
            .collect();
        details.insert(field.to_string(), messages);
    }
    ValidationProblem { error: "validation failed", details }
}

struct ValidatedJson<T>(T);

enum ApiError {
    JsonRejection(JsonRejection),
    Validation(ValidationErrors),
}

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(ApiError::JsonRejection)?;
        value.validate().map_err(ApiError::Validation)?;
        Ok(ValidatedJson(value))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::JsonRejection(r) => {
                (r.status(), Json(json!({ "error": r.body_text() }))).into_response()
            }
            ApiError::Validation(errors) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(to_problem(&errors))).into_response()
            }
        }
    }
}

async fn create_product(ValidatedJson(p): ValidatedJson<Product>) -> impl IntoResponse {
    (StatusCode::CREATED, Json(json!({ "name": p.name, "price": p.price })))
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/products", post(create_product));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Posting `{"name":"","price":0}`:

```bash
curl -s -i -X POST http://127.0.0.1:3000/products \
  -H 'content-type: application/json' -d '{"name":"","price":0}'
```

returns `422 Unprocessable Entity` with `{"error":"validation failed","details":{"name":["name must be 1-100 characters"],"price":["price must be 1-1000000"]}}`, while a valid product returns `201 Created`.

</details>
