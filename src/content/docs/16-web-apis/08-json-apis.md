---
title: "JSON REST APIs"
description: "Build a full CRUD resource in Axum where serde and the Json extractor type-check request and response bodies — no more unsafe req.body cast from Express."
---

## Quick Overview

A JSON REST API is the bread and butter of backend work: accept a typed JSON body, do something with it, and return a typed JSON response with a sensible status code. In Express you wire this up with `express.json()` and hand-rolled validation; in **Axum** the `Json` extractor and the `Json` response type, both powered by **serde**, give you the same workflow with the request and response shapes checked at compile time. This page builds a complete CRUD resource (`/notes`) and shows how serde's derive macros control exactly what your JSON looks like on the wire.

> **Note:** The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. This page targets **axum 0.8** (`axum::serve` + `tokio::net::TcpListener`, `{id}` path captures — not the old `:id`). For the Express-to-Axum fundamentals (router, handlers, starting the server) see [Axum Fundamentals](/16-web-apis/01-axum-basics/); for how `Json` works as an *input* extractor alongside `Path`/`Query`/`State`, see [Extractors](/16-web-apis/04-extractors/).

---

## TypeScript/JavaScript Example

Here is a realistic in-memory CRUD resource in Express: list, create, read, update, and delete "notes". It is the kind of thing you would write before reaching for a database.

```typescript
// notes-api.ts — Express 5
import express, { Request, Response } from "express";
import { randomUUID } from "node:crypto";

interface Note {
  id: string;
  title: string;
  body: string;
  done: boolean;
}

// Body shapes accepted from clients (no server-assigned fields).
interface CreateNote {
  title: string;
  body: string;
}
interface UpdateNote {
  title?: string;
  body?: string;
  done?: boolean;
}

const notes = new Map<string, Note>();

const app = express();
app.use(express.json()); // populate req.body for JSON requests

// GET /notes — list everything
app.get("/notes", (_req: Request, res: Response) => {
  res.json([...notes.values()]);
});

// POST /notes — create, reply 201
app.post("/notes", (req: Request, res: Response) => {
  const input = req.body as CreateNote; // a cast, NOT a runtime check
  if (typeof input.title !== "string" || typeof input.body !== "string") {
    return res.status(422).json({ error: "title and body are required" });
  }
  const note: Note = {
    id: randomUUID(),
    title: input.title,
    body: input.body,
    done: false,
  };
  notes.set(note.id, note);
  res.status(201).json(note);
});

// GET /notes/:id — one note or 404
app.get("/notes/:id", (req: Request, res: Response) => {
  const note = notes.get(req.params.id);
  if (!note) return res.status(404).json({ error: "note not found" });
  res.json(note);
});

// PUT /notes/:id — partial update or 404
app.put("/notes/:id", (req: Request, res: Response) => {
  const note = notes.get(req.params.id);
  if (!note) return res.status(404).json({ error: "note not found" });
  const patch = req.body as UpdateNote;
  if (patch.title !== undefined) note.title = patch.title;
  if (patch.body !== undefined) note.body = patch.body;
  if (patch.done !== undefined) note.done = patch.done;
  res.json(note);
});

// DELETE /notes/:id — 204 or 404
app.delete("/notes/:id", (req: Request, res: Response) => {
  if (notes.delete(req.params.id)) return res.status(204).end();
  res.status(404).json({ error: "note not found" });
});

app.listen(3000, () => console.log("listening on http://127.0.0.1:3000"));
```

What a TypeScript developer relies on here: `express.json()` parses the body into `req.body`, but `req.body as CreateNote` is a **compile-time cast that does nothing at runtime**. If the client sends `{}` or `{"title": 42}`, the cast still "succeeds" and you only find out when something downstream breaks. So you write the `typeof` checks by hand. The `UpdateNote` fields are optional (`title?`), giving a PATCH-style partial update.

---

## Rust Equivalent

The same resource in Axum. The `Json<T>` extractor deserializes *and* validates the request body's shape against the target type before the handler runs; returning `Json<T>` serializes the response and sets `Content-Type: application/json`.

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// The resource we store and return. `Serialize` lets it become a JSON response.
#[derive(Clone, Serialize)]
struct Note {
    id: Uuid,
    title: String,
    body: String,
    done: bool,
}

// The body shape for POST /notes — no `id`, the server assigns it.
// `Deserialize` lets `Json<CreateNote>` parse it out of the request body.
#[derive(Deserialize)]
struct CreateNote {
    title: String,
    body: String,
}

// The body shape for PUT /notes/{id} — every field optional (partial update).
#[derive(Deserialize)]
struct UpdateNote {
    title: Option<String>,
    body: Option<String>,
    done: Option<bool>,
}

// Shared, thread-safe store. `RwLock` allows many concurrent readers.
type Db = Arc<RwLock<HashMap<Uuid, Note>>>;

#[derive(Clone, Default)]
struct AppState {
    notes: Db,
}

// GET /notes — list every note. A `Vec<T>` serializes to a JSON array.
async fn list_notes(State(state): State<AppState>) -> Json<Vec<Note>> {
    let notes = state.notes.read().unwrap();
    Json(notes.values().cloned().collect())
}

// POST /notes — create one, reply 201 + the created resource.
async fn create_note(
    State(state): State<AppState>,
    Json(input): Json<CreateNote>,
) -> impl IntoResponse {
    let note = Note {
        id: Uuid::new_v4(),
        title: input.title,
        body: input.body,
        done: false,
    };
    state.notes.write().unwrap().insert(note.id, note.clone());
    (StatusCode::CREATED, Json(note))
}

// GET /notes/{id} — one note, or 404.
async fn get_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Note>, StatusCode> {
    state
        .notes
        .read()
        .unwrap()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

// PUT /notes/{id} — partial update; 404 if missing.
async fn update_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateNote>,
) -> Result<Json<Note>, StatusCode> {
    let mut notes = state.notes.write().unwrap();
    let note = notes.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;
    if let Some(title) = input.title {
        note.title = title;
    }
    if let Some(body) = input.body {
        note.body = body;
    }
    if let Some(done) = input.done {
        note.done = done;
    }
    Ok(Json(note.clone()))
}

// DELETE /notes/{id} — 204 on success, 404 if it was not there.
async fn delete_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    if state.notes.write().unwrap().remove(&id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

fn app() -> Router {
    Router::new()
        .route("/notes", get(list_notes).post(create_note))
        .route(
            "/notes/{id}",
            get(get_note).put(update_note).delete(delete_note),
        )
        .with_state(AppState::default())
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

The dependencies (run these in a fresh `cargo new` project; `cargo add` resolves the current versions automatically):

```bash
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json
cargo add uuid --features v4,serde
```

This produces a `Cargo.toml` with the current stable versions:

```toml
[dependencies]
axum = "0.8.9"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
tokio = { version = "1.52.3", features = ["full"] }
uuid = { version = "1.23.2", features = ["v4", "serde"] }
```

> **Note:** `serde_json` is the engine `Json` uses under the hood. The `axum::Json` type pulls it in transitively, but you add `serde_json` explicitly because you will reach for `serde_json::json!` and `serde_json::Value` directly (shown later). The `uuid` crate's `serde` feature is what lets `Uuid` round-trip through JSON; its `v4` feature provides random id generation.

Run it with `cargo run`. Exercising the endpoints with `curl` produces this **real** output (captured against the compiled server):

```text
$ curl -s -i -X POST http://127.0.0.1:3000/notes \
       -H 'content-type: application/json' -d '{"title":"Buy milk","body":"2 liters"}'
HTTP/1.1 201 Created
content-type: application/json
content-length: 95
date: Mon, 01 Jun 2026 11:53:53 GMT

{"id":"bff6b79a-d1cc-42f0-b7f0-30ff8e4f0094","title":"Buy milk","body":"2 liters","done":false}

$ curl -s http://127.0.0.1:3000/notes/69ca8514-0ec2-4164-9677-76cb63cd613d
{"id":"69ca8514-0ec2-4164-9677-76cb63cd613d","title":"Walk dog","body":"around the block","done":false}

$ curl -s -i -X PUT http://127.0.0.1:3000/notes/69ca8514-0ec2-4164-9677-76cb63cd613d \
       -H 'content-type: application/json' -d '{"done":true}'
HTTP/1.1 200 OK
content-type: application/json
content-length: 102

{"id":"69ca8514-0ec2-4164-9677-76cb63cd613d","title":"Walk dog","body":"around the block","done":true}

$ curl -s -i -X DELETE http://127.0.0.1:3000/notes/69ca8514-0ec2-4164-9677-76cb63cd613d
HTTP/1.1 204 No Content
date: Mon, 01 Jun 2026 11:53:54 GMT
```

The validation you wrote by hand in Express comes for free. These are also **real** responses from the same server:

```text
$ curl -s -i http://127.0.0.1:3000/notes/not-a-uuid           # bad path param
HTTP/1.1 400 Bad Request
content-type: text/plain; charset=utf-8

Invalid URL: Cannot parse `id` with value `not-a-uuid`: UUID parsing failed: invalid character: found `n` at 0

$ curl -s -i -X POST http://127.0.0.1:3000/notes \            # missing `body`
       -H 'content-type: application/json' -d '{"title":"oops"}'
HTTP/1.1 422 Unprocessable Entity
content-type: text/plain; charset=utf-8

Failed to deserialize the JSON body into the target type: missing field `body` at line 1 column 16

$ curl -s -i -X POST http://127.0.0.1:3000/notes \            # wrong type for `title`
       -H 'content-type: application/json' -d '{"title":1,"body":"y"}'
HTTP/1.1 422 Unprocessable Entity

Failed to deserialize the JSON body into the target type: title: invalid type: integer `1`, expected a string at line 1 column 10

$ curl -s -i -X POST http://127.0.0.1:3000/notes \            # no Content-Type header
       -d '{"title":"x","body":"y"}'
HTTP/1.1 415 Unsupported Media Type

Expected request with `Content-Type: application/json`

$ curl -s -i -X POST http://127.0.0.1:3000/notes \            # malformed JSON
       -H 'content-type: application/json' -d '{"title":'
HTTP/1.1 400 Bad Request

Failed to parse the request body as JSON: title: EOF while parsing a value at line 1 column 9
```

Note the precise status-code choices, all made *before your handler runs*: a malformed-syntax body is `400`, a body that parses but has the wrong *shape* is `422 Unprocessable Entity`, and a missing/incorrect content type is `415`.

---

## Detailed Explanation

### `Json<T>` is both an extractor and a response

This is the single most important idea on this page. The same type `axum::Json<T>` plays two roles:

- **As a handler parameter** (`Json(input): Json<CreateNote>`), it implements `FromRequest`: it reads the whole body, checks `Content-Type: application/json`, and runs `serde_json` deserialization into `T`. `T` must be `Deserialize`.
- **As a return value** (`-> Json<Note>` or inside a tuple), it implements `IntoResponse`: it serializes the wrapped value with `serde_json` and sets the `Content-Type` header. `T` must be `Serialize`.

```rust
// Input:  Json<T>  where T: Deserialize  (body -> struct)
// Output: Json<T>  where T: Serialize    (struct -> body)
```

This symmetry is why a "round-trip" type — one accepted *and* returned — derives both: `#[derive(Serialize, Deserialize)]`. In our example `Note` is only ever *returned*, so it derives only `Serialize`; `CreateNote`/`UpdateNote` are only ever *received*, so they derive only `Deserialize`. Deriving exactly what each type needs documents its direction.

> **Note:** Because `Json` (as an extractor) consumes the request body, it must be the **last** parameter in a handler, after metadata extractors like `State` and `Path`. See [Extractors](/16-web-apis/04-extractors/) for the `FromRequest` vs `FromRequestParts` ordering rule.

### serde does the work; derive macros control the wire format

`#[derive(Serialize)]` and `#[derive(Deserialize)]` generate, at compile time, code that maps your struct to and from JSON. By default a Rust field `display_name` becomes the JSON key `"display_name"`. Real APIs usually want `camelCase`, optional fields that disappear when empty, and defaults for missing fields. serde attributes give you all of that declaratively. Here is a compile-verified demonstration:

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Product {
    id: u64,
    display_name: String, // <-> JSON "displayName"
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>, // omitted from output when None
    #[serde(default)]
    tags: Vec<String>, // defaults to [] if absent in the input
}

fn main() {
    // Serialize: snake_case Rust fields become camelCase JSON.
    let p = Product {
        id: 1,
        display_name: "Widget".to_string(),
        description: None,
        tags: vec!["new".to_string()],
    };
    println!("OUT: {}", serde_json::to_string(&p).unwrap());

    // Deserialize: camelCase JSON in, and the missing `tags` defaults to [].
    let json = r#"{"id":2,"displayName":"Gadget"}"#;
    let parsed: Product = serde_json::from_str(json).unwrap();
    println!("IN:  {parsed:?}");
}
```

The **real** output:

```text
OUT: {"id":1,"displayName":"Widget","tags":["new"]}
IN:  Product { id: 2, display_name: "Gadget", description: None, tags: [] }
```

Three attributes earn their keep on almost every API type:

| Attribute | Effect | TypeScript analogue |
| --- | --- | --- |
| `#[serde(rename_all = "camelCase")]` | maps `snake_case` fields to `camelCase` JSON keys | nothing automatic; you name fields by hand |
| `#[serde(skip_serializing_if = "Option::is_none")]` | omits a `None` field from the output entirely | `if (x !== undefined) obj.x = x` |
| `#[serde(default)]` | uses the type's `Default` when the key is absent on input | `const x = body.x ?? defaultValue` |

This whole topic — serde's model, attributes, custom (de)serialization — is covered in depth in [Serialization](/15-serialization/). Here we use it specifically to shape an HTTP JSON body.

### `rename_all` keeps idiomatic Rust *and* idiomatic JSON

JavaScript/TypeScript clients expect `camelCase`; idiomatic Rust uses `snake_case`. You do **not** have to choose. `#[serde(rename_all = "camelCase")]` lets you keep `created_at` in Rust while the JSON says `"createdAt"`, with zero per-field annotation. (Other casings are available too: `"kebab-case"`, `"SCREAMING_SNAKE_CASE"`, etc.)

### `Vec<T>` is a JSON array; `Option<T>` is "field may be absent or null"

- Returning `Json<Vec<Note>>` produces a JSON array `[...]` (an empty list serializes to `[]`, never `null`). No wrapper object is added unless you ask for one.
- An `Option<String>` field deserializes from a present value, an explicit `null`, or — with `#[serde(default)]` or because `Option` defaults to `None` for missing keys — an absent key. This is exactly what powers the PATCH-style `UpdateNote`: any field the client omits stays `None`, so the handler leaves it untouched.

### The status code is part of the response, set by the return type

`create_note` returns `(StatusCode::CREATED, Json(note))`. A tuple `(StatusCode, Json<T>)` implements `IntoResponse` as "this status **plus** this JSON body". A bare `Json<T>` defaults to `200 OK`. `delete_note` returns a bare `StatusCode` (an empty-bodied response). And `get_note`/`update_note` return `Result<Json<Note>, StatusCode>`: the `Ok` arm is the JSON, the `Err` arm is the status. This is how you express "200 with a body, or 404" as a single type. The full `IntoResponse` story (headers, custom statuses, the `Result` pattern) lives in [Request and Response Handling](/16-web-apis/07-request-response/).

### Why `Uuid` instead of an auto-increment integer

For an in-memory store keyed by a `HashMap`, a `Uuid` (`uuid::Uuid` with the `v4` feature) gives unique ids without a shared counter. With the `serde` feature, `Uuid` serializes to its canonical string form (`"bff6b79a-..."`) and deserializes back, with a non-UUID path segment rejected as a `400`. If you prefer integer ids, `Path<u64>` works identically; the database section ([Database](/17-database/)) shows server-assigned ids from a real backing store.

---

## Key Differences

| Concern | Express.js | Axum |
| --- | --- | --- |
| Parse body | `express.json()` middleware, then `req.body` | `Json<T>` extractor parameter |
| Body type safety | `req.body as T` (a no-op cast) | `Json<T>` deserializes into `T`, or returns 4xx |
| Missing/wrong field | silent `undefined`; you check by hand | `422` with the exact field and reason, automatically |
| Field renaming | name fields manually | `#[serde(rename_all = "camelCase")]` |
| Optional output field | `if (x) obj.x = x` | `#[serde(skip_serializing_if = "Option::is_none")]` |
| Send JSON response | `res.json(value)` | `return Json(value)` |
| Set status | `res.status(201).json(...)` | return `(StatusCode::CREATED, Json(...))` |
| "200 or 404" | `if (!x) return res.status(404)...` | `Result<Json<T>, StatusCode>` |
| Partial update | optional interface fields | `Option<T>` struct fields |

The conceptual shift: in Express, **parsing and validating JSON is imperative work inside the handler**; in Axum it is **declarative metadata on the type and in the signature**. A handler whose parameter is `Json<CreateNote>` cannot run with a malformed or wrong-shaped body. The type system and the framework guarantee it, the same way a function with typed parameters never sees the wrong argument types.

> **Tip:** Think of your `Deserialize` DTOs as the runtime-checked version of TypeScript interfaces. A TS `interface CreateNote` vanishes at compile time; a Rust `#[derive(Deserialize)] struct CreateNote` becomes real parsing-and-validation code that rejects bad input at the door.

---

## Common Pitfalls

### 1. Using a type as a `Json` extractor when it only derives `Serialize`

A type only becomes parseable from a request body when it derives `Deserialize`. If you have so far only *returned* a type (so it derives only `Serialize`) and then try to *accept* it, the handler fails the `Handler` trait bound. Annotating with `#[axum::debug_handler]` (enable the `macros` feature: `cargo add axum --features macros`) turns the cryptic bound error into the real cause:

```rust
use axum::{routing::post, Json, Router};
use serde::Serialize;

#[derive(Serialize)] // only Serialize, not Deserialize
struct CreateNote {
    title: String,
}

// does not compile (error[E0277]): CreateNote is not Deserialize,
// so Json<CreateNote> cannot be used as an extractor.
#[axum::debug_handler]
async fn create(Json(_note): Json<CreateNote>) -> &'static str {
    "ok"
}

fn main() {
    let _app: Router = Router::new().route("/notes", post(create));
}
```

The **real** `cargo build` error:

```text
error[E0277]: the trait bound `CreateNote: serde::Deserialize<'de>` is not satisfied
  --> src/bin/badjson.rs:12:30
   |
12 | async fn create(Json(_note): Json<CreateNote>) -> &'static str {
   |                              ^^^^ the trait `for<'de> Deserialize<'de>` is not implemented for `CreateNote`
   |
   = note: for local types consider adding `#[derive(serde::Deserialize)]` to your `CreateNote` type
   = note: for types from other crates check whether the crate offers a `serde` feature flag
   ...
   = note: required for `CreateNote` to implement `DeserializeOwned`
```

The fix is exactly what the note says: add `#[derive(Deserialize)]` (or both `Serialize, Deserialize` for a round-trip type).

### 2. Expecting extra/unknown fields to be rejected by default

By default, serde **ignores** JSON keys that do not match a struct field. A client can send `{"title":"x","body":"y","admin":true}` to `CreateNote` and the `admin` field is silently dropped: handy for forward compatibility, surprising if you wanted to reject it. To make unknown fields an error, add `#[serde(deny_unknown_fields)]`:

```rust playground
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct CreateNote {
    title: String,
    body: String,
}

fn main() {
    let extra = r#"{"title":"a","body":"b","admin":true}"#;
    match serde_json::from_str::<CreateNote>(extra) {
        Ok(v) => println!("parsed: {v:?}"),
        Err(e) => println!("ERR: {e}"),
    }
}
```

**Real** output:

```text
ERR: unknown field `admin`, expected `title` or `body` at line 1 column 31
```

Inside a handler, this surfaces as a `422` with that same message in the body. Use `deny_unknown_fields` on inbound DTOs when silently accepting unexpected keys would be a security or correctness problem (e.g. mass-assignment).

### 3. Returning the wrong default status for a write

A freshly compiled handler that returns `Json(note)` from a `POST` sends `200 OK`, not `201 Created`. Axum will not guess your intent. Be explicit: return `(StatusCode::CREATED, Json(note))` for creates and `StatusCode::NO_CONTENT` (204) for a body-less delete. Forgetting this is not a compiler error; it is a contract bug a test will catch.

### 4. Treating `Option<T>` "absent" and "null" as different in JSON

For a plain `Option<String>` field, both a missing key and an explicit `null` deserialize to `None`. If your API needs to distinguish "the client did not mention this field" from "the client set it to null" (true PATCH semantics), a single `Option` cannot express it: you need a nested `Option<Option<T>>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`, or a dedicated patch enum. For most CRUD this distinction does not matter; just know the default collapses the two.

### 5. Forgetting the `Content-Type` header on the client

The `Json<T>` extractor requires `Content-Type: application/json`. A request without it gets a `415 Unsupported Media Type` (shown in the real output above) before your handler runs. This trips people testing with `curl` who forget `-H 'content-type: application/json'`. It is a feature: it stops form posts and other content types from being misread as JSON.

---

## Best Practices

- **Derive exactly the direction each type needs.** `Serialize` for response-only types, `Deserialize` for request-only types, both for round-trip types. It documents intent and avoids the pitfall above.
- **Separate the request DTO from the stored/returned model.** `CreateNote` (no `id`, no `done`) is deliberately not `Note`. The client should not be able to set server-owned fields like `id`, `created_at`, or `done`-on-create. Distinct types make that impossible by construction: the Rust equivalent of guarding against mass assignment.
- **Apply `#[serde(rename_all = "camelCase")]` at the type level** so Rust stays idiomatic (`snake_case`) and the JSON stays idiomatic for JS/TS clients (`camelCase`).
- **Use `Option<T>` + `skip_serializing_if` for sparse responses**, and `Option<T>` fields for PATCH-style partial updates.
- **Pick correct status codes deliberately:** `201` for create, `200` for read/update, `204` for delete-with-no-body, `404`/`422`/`400` for the failure paths. Encode "found or not" as `Result<Json<T>, StatusCode>`.
- **Reach for a single error type that implements `IntoResponse`** once you have more than one failure mode, so every error returns a consistent JSON envelope (shown next, and fully developed in [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/)).
- **Keep an `fn app() -> Router` builder** separate from `main`, so tests can drive the router with `tower::ServiceExt::oneshot` without binding a port. See [Testing](/13-testing/).
- **For business-rule validation** (non-empty title, valid email, length limits) go beyond shape-checking; see [Validation](/16-web-apis/09-validation/).

---

## Real-World Example

A production-flavored `/notes` API: `camelCase` JSON, server-owned fields the client cannot set, unknown-field rejection on input, and a single `ApiError` type that gives every failure a consistent JSON body (`{"error": "..."}`) with the right status. This is the shape of a real service just before you swap the in-memory `HashMap` for a database. Every line is compile-verified against axum 0.8.

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// Stored + returned. camelCase on the wire; `created_at` is server-owned.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Note {
    id: Uuid,
    title: String,
    body: String,
    done: bool,
    created_at: String,
}

// Inbound create: client supplies only title + body. Unknown keys rejected.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateNote {
    title: String,
    body: String,
}

// Inbound partial update: every field optional.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateNote {
    title: Option<String>,
    body: Option<String>,
    done: Option<bool>,
}

// One error type for the whole resource. Implementing IntoResponse once means
// every handler can `?`-propagate or return it and get the same JSON envelope.
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn not_found(what: &str) -> Self {
        ApiError {
            status: StatusCode::NOT_FOUND,
            message: format!("{what} not found"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

type Db = Arc<RwLock<HashMap<Uuid, Note>>>;

#[derive(Clone, Default)]
struct AppState {
    notes: Db,
}

async fn list_notes(State(state): State<AppState>) -> Json<Vec<Note>> {
    Json(state.notes.read().unwrap().values().cloned().collect())
}

async fn create_note(
    State(state): State<AppState>,
    Json(input): Json<CreateNote>,
) -> (StatusCode, Json<Note>) {
    let note = Note {
        id: Uuid::new_v4(),
        title: input.title,
        body: input.body,
        done: false,
        created_at: "2026-06-01T00:00:00Z".to_string(),
    };
    state.notes.write().unwrap().insert(note.id, note.clone());
    (StatusCode::CREATED, Json(note))
}

async fn get_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Note>, ApiError> {
    state
        .notes
        .read()
        .unwrap()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("note"))
}

async fn update_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateNote>,
) -> Result<Json<Note>, ApiError> {
    let mut notes = state.notes.write().unwrap();
    let note = notes.get_mut(&id).ok_or_else(|| ApiError::not_found("note"))?;
    if let Some(t) = input.title {
        note.title = t;
    }
    if let Some(b) = input.body {
        note.body = b;
    }
    if let Some(d) = input.done {
        note.done = d;
    }
    Ok(Json(note.clone()))
}

async fn delete_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if state.notes.write().unwrap().remove(&id).is_some() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("note"))
    }
}

fn app() -> Router {
    Router::new()
        .route("/notes", get(list_notes).post(create_note))
        .route(
            "/notes/{id}",
            get(get_note).put(update_note).delete(delete_note),
        )
        .with_state(AppState::default())
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

Exercising it produces these **real** responses. Note the `camelCase` `createdAt` in the output and the consistent JSON error envelope:

```text
$ curl -s -X POST http://127.0.0.1:3000/notes \
       -H 'content-type: application/json' -d '{"title":"Ship v1","body":"cut the release"}'
{"id":"c2c6d0c5-b545-4817-afee-bec328538892","title":"Ship v1","body":"cut the release","done":false,"createdAt":"2026-06-01T00:00:00Z"}

$ curl -s -i http://127.0.0.1:3000/notes/00000000-0000-0000-0000-000000000000
HTTP/1.1 404 Not Found
content-type: application/json
content-length: 26

{"error":"note not found"}

$ curl -s -i -X POST http://127.0.0.1:3000/notes \
       -H 'content-type: application/json' -d '{"title":"x","body":"y","admin":true}'
HTTP/1.1 422 Unprocessable Entity
content-type: text/plain; charset=utf-8

Failed to deserialize the JSON body into the target type: admin: unknown field `admin`, expected `title` or `body` at line 1 column 31
```

Two production-relevant notes on this code:

- **The `created_at` field is set by the server, never by the client.** Because `CreateNote` has no `created_at` field and uses `deny_unknown_fields`, a client *cannot* inject one. Separating the inbound DTO from the stored model is how Rust's type system gives you mass-assignment protection for free.
- **`ApiError` centralizes error responses.** Implementing `IntoResponse` once means handlers return `Result<_, ApiError>` and `?`-propagate failures, all yielding the same `{"error": "..."}` JSON shape. The serde extractor rejections (the `422` above) still produce serde's default plain-text body; to bring *those* into the same JSON envelope, override the `Json` rejection, fully covered in [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/). For real persistence, swap `HashMap` for a database pool in [State Management](/16-web-apis/06-state-management/) and [Database](/17-database/).

---

## Further Reading

- [`axum::Json`](https://docs.rs/axum/latest/axum/struct.Json.html): the extractor/response type used throughout this page.
- [serde derive attributes](https://serde.rs/attributes.html): the full list of `#[serde(...)]` knobs (`rename_all`, `default`, `skip_serializing_if`, `deny_unknown_fields`, `flatten`, and more).
- [`serde_json`](https://docs.rs/serde_json/latest/serde_json/): the JSON engine, including the `json!` macro and `serde_json::Value`.
- [`uuid` crate](https://docs.rs/uuid/latest/uuid/): the `Uuid` type, its `v4` (random) and `serde` features.

Within this guide:

- [Axum Fundamentals](/16-web-apis/01-axum-basics/): Router, handlers, and starting the server (the foundation for this page).
- [Extractors](/16-web-apis/04-extractors/): how `Json` works as an input extractor alongside `Path`, `Query`, `State`, and the body-extractor ordering rule.
- [Request and Response Handling](/16-web-apis/07-request-response/): `IntoResponse`, status codes, headers, the `(StatusCode, Json<T>)` tuple, and the `Result` return pattern.
- [Validation](/16-web-apis/09-validation/): business-rule validation beyond serde's shape-checking, and helpful `400` messages.
- [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/): a full `AppError` with `thiserror`, mapping error kinds to status codes, and overriding extractor rejections.
- [State Management](/16-web-apis/06-state-management/): sharing a database pool / config via `State<T>`.
- [Routing](/16-web-apis/03-routing/): `{id}` path captures, method routing, nested routers.
- Foundations: [Serialization](/15-serialization/) (serde in depth), [Error Handling](/08-error-handling/) (`Result`, `?`), [Generics and Traits](/09-generics-traits/), the language [Basics](/02-basics/) and [Getting Started](/01-getting-started/).
- Persisting these resources: [Database](/17-database/).

---

## Exercises

### Exercise 1: Add a `GET /notes/count` endpoint

**Difficulty:** Beginner

**Objective:** Return a small JSON object built from shared state.

**Instructions:** Starting from the first Rust CRUD server in this page, add a `GET /notes/count` route that returns `{"count": N}` where `N` is the number of stored notes. Define a `#[derive(Serialize)]` response struct with a single `count: usize` field and read the length from the `RwLock`.

<details>
<summary>Solution</summary>

```rust
use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

#[derive(Clone, Serialize)]
struct Note {
    id: Uuid,
    title: String,
}

#[derive(Clone, Default)]
struct AppState {
    notes: Arc<RwLock<HashMap<Uuid, Note>>>,
}

#[derive(Serialize)]
struct CountResponse {
    count: usize,
}

async fn count_notes(State(state): State<AppState>) -> Json<CountResponse> {
    let count = state.notes.read().unwrap().len();
    Json(CountResponse { count })
}

fn app() -> Router {
    Router::new()
        .route("/notes/count", get(count_notes))
        .with_state(AppState::default())
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

`GET /notes/count` returns `{"count":0}` on a fresh server.

</details>

### Exercise 2: Filter the list by a query parameter

**Difficulty:** Intermediate

**Objective:** Combine the `Json` response with a `Query` extractor to filter results.

**Instructions:** Add a `GET /notes?done=true` (or `?done=false`) endpoint. Use a `Query<ListQuery>` extractor where `ListQuery` has a field `done: Option<bool>`. When `done` is provided, return only notes whose `done` matches; when absent, return all of them. Return `Json<Vec<Note>>`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

#[derive(Clone, Serialize)]
struct Note {
    id: u64,
    title: String,
    done: bool,
}

#[derive(Deserialize)]
struct ListQuery {
    done: Option<bool>,
}

#[derive(Clone)]
struct AppState {
    notes: Arc<RwLock<Vec<Note>>>,
}

async fn list_notes(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Json<Vec<Note>> {
    let notes = state.notes.read().unwrap();
    let out: Vec<Note> = notes
        .iter()
        // `is_none_or`: keep everything when no filter, else match `done`.
        .filter(|n| q.done.is_none_or(|d| n.done == d))
        .cloned()
        .collect();
    Json(out)
}

fn app() -> Router {
    let state = AppState {
        notes: Arc::new(RwLock::new(vec![
            Note { id: 1, title: "a".into(), done: true },
            Note { id: 2, title: "b".into(), done: false },
        ])),
    };
    Router::new()
        .route("/notes", get(list_notes))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

`GET /notes` returns both notes; `GET /notes?done=true` returns `[{"id":1,"title":"a","done":true}]`; `GET /notes?done=false` returns only the second. (Note `Option::is_none_or`, which Clippy recommends over `map_or(true, ...)`.)

</details>

### Exercise 3: Reject empty titles with a JSON `400`

**Difficulty:** Advanced

**Objective:** Add a business-rule check on top of serde's shape-checking and return a structured `400`.

**Instructions:** Extend `POST /notes` so that a blank or whitespace-only `title` is rejected with `400 Bad Request` and a JSON body `{"error":"title must not be empty"}`, while a valid create still returns `201` with the note. Use `impl IntoResponse` and the `serde_json::json!` macro to build the error body. (Shape-checking — required fields, correct types — is still handled by `Json<CreateNote>`; you are adding a *value* rule on top.)

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, RwLock};

#[derive(Clone, Serialize)]
struct Note {
    id: u64,
    title: String,
    done: bool,
}

#[derive(Deserialize)]
struct CreateNote {
    title: String,
}

#[derive(Clone, Default)]
struct AppState {
    notes: Arc<RwLock<Vec<Note>>>,
}

async fn create_note(
    State(state): State<AppState>,
    Json(input): Json<CreateNote>,
) -> impl IntoResponse {
    let title = input.title.trim();
    if title.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "title must not be empty" })),
        )
            .into_response();
    }
    let mut notes = state.notes.write().unwrap();
    let id = notes.len() as u64 + 1;
    let note = Note { id, title: title.to_string(), done: false };
    notes.push(note.clone());
    (StatusCode::CREATED, Json(note)).into_response()
}

fn app() -> Router {
    Router::new()
        .route("/notes", post(create_note))
        .with_state(AppState::default())
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

`POST /notes` with `{"title":"   "}` returns `400` and `{"error":"title must not be empty"}`; with `{"title":"c"}` it returns `201` and `{"id":1,"title":"c","done":false}`. The `.into_response()` calls are needed because the two arms return different concrete types; both are unified to the `Response` type that `impl IntoResponse` promises. For richer rule-based validation across many fields, see [Validation](/16-web-apis/09-validation/).

</details>
