---
title: "Routing in Axum"
description: "Route in Axum vs Express: the {id} path syntax, typed path and query params, method routing, nested routers, and fallbacks, with bad input rejected as a clean 400."
---

## Quick Overview

Routing maps an incoming HTTP method and URL path to the function that handles it. If you have written `app.get("/users/:id", handler)` in Express, you already understand the job. Axum does the same with a `Router`, but with compile-time-checked handlers, typed path and query extraction, and a composition model (`nest`, `merge`, `fallback`) that scales cleanly to large APIs. This page covers handlers, path parameters (which use `{id}` in Axum 0.8, **not** the Express-style `:id`), query parameters, method routing, nested routers, and fallbacks.

> **Note:** This page assumes you have an Axum project running (see [Axum Setup](/16-web-apis/02-axum-setup/)) and understand the basic `Router` + `axum::serve` shape (see [Axum Basics](/16-web-apis/01-axum-basics/)). The mechanics of *how* extractors like `Path` and `Query` pull data out of a request live in [Extractors](/16-web-apis/04-extractors/); this page focuses on the routing layer that decides *which* handler runs.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. All Rust in this page is compile-verified against Axum 0.8.9 and Tokio 1.52.

---

## TypeScript/JavaScript Example

Here is a realistic Express.js router for a small REST API: a users resource with nested posts, list/create/read/update/delete, a query-string-driven list endpoint, a method-specific 405, and a catch-all 404:

```typescript
// app.ts — Express 5
import express, { Request, Response } from "express";

const app = express();
app.use(express.json());

// GET / — a plain string handler
app.get("/", (_req: Request, res: Response) => {
  res.send("API root");
});

// A sub-router mounted under /api/users (Express's version of "nesting")
const users = express.Router();

// GET /api/users?page=2&sort=name — query params arrive as strings
users.get("/", (req: Request, res: Response) => {
  const page = Number(req.query.page ?? 1);
  const perPage = Number(req.query.per_page ?? 20);
  const sort = String(req.query.sort ?? "id");
  res.json({ page, perPage, sort });
});

// POST /api/users — JSON body
users.post("/", (req: Request, res: Response) => {
  res.status(201).json({ id: 42, name: req.body.name });
});

// Path params use the ":name" syntax in Express
users.get("/:id", (req: Request, res: Response) => {
  const id = Number(req.params.id); // NOTE: req.params.id is a string!
  res.json({ id, name: `User ${id}` });
});

users.delete("/:id", (_req: Request, res: Response) => {
  res.status(204).end();
});

// Multiple path params
users.get("/:userId/posts/:postId", (req: Request, res: Response) => {
  res.send(`user ${req.params.userId}, post ${req.params.postId}`);
});

app.use("/api/users", users);

// Catch-all 404 — must be registered LAST
app.use((_req: Request, res: Response) => {
  res.status(404).send("nothing to see here");
});

app.listen(3000, () => console.log("listening on 3000"));
```

Two things a TypeScript/JavaScript developer takes for granted here will change in Rust:

- **Everything is a string.** `req.params.id` and `req.query.page` are always `string` (or `undefined`). You parse and validate by hand, and a typo produces `NaN` at runtime, not a compile error.
- **Order matters and is implicit.** The catch-all `app.use(...)` works only because it is registered last. Express walks the middleware/route stack top-to-bottom; the first match wins.

---

## Rust Equivalent

The same API in Axum 0.8. Notice the typed extractors (`Path<u64>`, `Query<Pagination>`, `Json<CreateUser>`), the `{id}` path syntax, and the `.nest`/`.fallback` composition:

```rust
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}

// A plain handler: no extractors, returns a static string.
async fn root() -> &'static str {
    "Hello from Axum"
}

// Single path param. `{id}` in the route binds to `Path<u64>` here.
// If the segment is not a valid u64, Axum returns 400 before this runs.
async fn get_user(Path(id): Path<u64>) -> Json<User> {
    Json(User { id, name: format!("User {id}") })
}

// Two path params destructured into a tuple, in route order.
async fn get_post(Path((user_id, post_id)): Path<(u64, u64)>) -> String {
    format!("user {user_id}, post {post_id}")
}

// Query params with defaults: Option<T> + unwrap_or.
#[derive(Deserialize)]
struct Pagination {
    page: Option<u32>,
    per_page: Option<u32>,
    sort: Option<String>,
}
async fn list_users(Query(p): Query<Pagination>) -> String {
    let page = p.page.unwrap_or(1);
    let per_page = p.per_page.unwrap_or(20);
    let sort = p.sort.unwrap_or_else(|| "id".to_string());
    format!("page={page} per_page={per_page} sort={sort}")
}

#[derive(Deserialize)]
struct CreateUser {
    name: String,
}
async fn create_user(Json(payload): Json<CreateUser>) -> (StatusCode, Json<User>) {
    (StatusCode::CREATED, Json(User { id: 42, name: payload.name }))
}

async fn delete_user(Path(id): Path<u64>) -> StatusCode {
    let _ = id; // pretend we deleted it
    StatusCode::NO_CONTENT
}

// The fallback runs when no route matches.
async fn not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "nothing to see here")
}

// A sub-router for the users resource — Express's `express.Router()`.
fn users_router() -> Router {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/{id}", get(get_user).delete(delete_user))
        .route("/{user_id}/posts/{post_id}", get(get_post))
}

fn app() -> Router {
    Router::new()
        .route("/", get(root))
        .nest("/api/users", users_router())
        .fallback(not_found)
}

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("listening on {addr}");
    axum::serve(listener, app()).await.unwrap();
}
```

Hitting this server with `curl` produces these real responses:

```text
$ curl -s http://127.0.0.1:3000/
Hello from Axum

$ curl -s http://127.0.0.1:3000/api/users/7
{"id":7,"name":"User 7"}

$ curl -s "http://127.0.0.1:3000/api/users?page=2&sort=name"
page=2 per_page=20 sort=name

$ curl -s http://127.0.0.1:3000/api/users
page=1 per_page=20 sort=id

$ curl -s http://127.0.0.1:3000/api/users/3/posts/99
user 3, post 99

$ curl -s -i -X POST http://127.0.0.1:3000/api/users \
    -H 'content-type: application/json' -d '{"name":"Ada"}'
HTTP/1.1 201 Created
content-type: application/json
content-length: 22

{"id":42,"name":"Ada"}

$ curl -s -o /dev/null -w 'status=%{http_code}\n' -X DELETE http://127.0.0.1:3000/api/users/7
status=204

$ curl -s http://127.0.0.1:3000/nope
nothing to see here
```

The big difference from Express is that `Path<u64>` already parsed and validated the id for you: `get_user` never sees a string, and a non-numeric id is rejected automatically (more on that below).

---

## Detailed Explanation

### Handlers are just `async fn`s that return something printable

In Express a handler is `(req, res) => { ... }` and you mutate `res`. In Axum a handler is an `async fn` (or async closure) whose **return value** becomes the response. `root` returns `&'static str`; Axum knows how to turn that into a `200 OK` with `text/plain`. `get_user` returns `Json<User>` → `200 OK` with `application/json`. `create_user` returns a `(StatusCode, Json<User>)` tuple → `201 Created` with a JSON body. The trait that powers this is `IntoResponse`, covered in [Request & Response](/16-web-apis/07-request-response/).

A handler's **parameters** are extractors. Axum looks at each parameter's type, runs the corresponding extraction against the request, and either calls your function with the values or short-circuits with an error response. That is why `get_user(Path(id): Path<u64>)` receives a ready-to-use `u64` instead of a string.

### `route(path, method_router)` and method routing

```rust
Router::new().route("/{id}", get(get_user).delete(delete_user))
```

`route` takes a path pattern and a **`MethodRouter`**, an object that maps HTTP methods to handlers for that one path. You build a `MethodRouter` by starting with a method function like `get(...)`, `post(...)`, `put(...)`, `patch(...)`, `delete(...)`, `head(...)`, or `options(...)`, then chaining more methods onto it:

```rust
use axum::routing::get;
// GET and POST on the same path, different handlers:
get(list_users).post(create_user)
// GET, PUT, and DELETE on the same path:
get(get_user).put(update_user).delete(delete_user)
```

This is more explicit than Express, where `users.get(...)` and `users.post(...)` are separate registrations. In Axum, one `route` call owns one path and *all* its methods. If a request hits a known path with an unsupported method, Axum returns `405 Method Not Allowed` with an `Allow` header listing what *is* supported — automatically. There is also `any(handler)` to match every method, and `MethodRouter::fallback` for a per-path method fallback.

### Path parameters: `{id}`, not `:id`

This is the single most important migration detail for an Express developer. Axum's router uses `matchit`, which uses **curly braces** for captures:

| Pattern | Matches | Express equivalent |
| --- | --- | --- |
| `/users/{id}` | `/users/7` | `/users/:id` |
| `/users/{user_id}/posts/{post_id}` | `/users/3/posts/9` | `/users/:userId/posts/:postId` |
| `/files/{*path}` | `/files/css/app.css` (rest of path) | `/files/*` |

You extract them with `Path`:

- **One param:** `Path(id): Path<u64>`. The value is parsed to your chosen type.
- **Several params:** `Path((user_id, post_id)): Path<(u64, u64)>`, a tuple in route order.
- **Several params, named:** deserialize into a struct whose fields match the capture names:

```rust
use axum::extract::Path;
use serde::Deserialize;

#[derive(Deserialize)]
struct CommentPath {
    post_id: u64,
    comment_id: u64,
}

// Route: "/posts/{post_id}/comments/{comment_id}"
async fn get_comment(Path(p): Path<CommentPath>) -> String {
    format!("post {} comment {}", p.post_id, p.comment_id)
}
```

```text
$ curl -s http://127.0.0.1:3000/posts/5/comments/8
post 5 comment 8
```

A **catch-all** uses `{*name}` and captures the remaining path (including slashes). This is how static-file servers and SPA fallbacks work (see [Static Files](/16-web-apis/18-static-files/)):

```rust
use axum::extract::Path;

// Route: "/files/{*path}"
async fn serve_file(Path(rest): Path<String>) -> String {
    format!("serving file: {rest}")
}
```

```text
$ curl -s http://127.0.0.1:3000/files/css/app.css
serving file: css/app.css
```

### Query parameters

Query strings are deserialized by `serde_urlencoded` into a struct (or a map). Mark optional fields `Option<T>`, or use serde defaults:

```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
struct ListParams {
    #[serde(default = "default_limit")] // used when `limit` is absent
    limit: u32,
    tag: Option<String>, // None when absent
}
fn default_limit() -> u32 {
    10
}

async fn list(Query(p): Query<ListParams>) -> String {
    let tag = p.tag.unwrap_or_else(|| "all".into());
    format!("limit={} tag={}", p.limit, tag)
}
```

For a free-form query string with no fixed schema (like reading `req.query` directly in Express), deserialize into a `HashMap`:

```rust
use axum::extract::Query;
use std::collections::HashMap;

async fn raw_query(Query(params): Query<HashMap<String, String>>) -> String {
    let mut keys: Vec<_> = params.keys().cloned().collect();
    keys.sort();
    format!("{keys:?}")
}
```

```text
$ curl -s "http://127.0.0.1:3000/raw?a=1&b=2&color=red"
["a", "b", "color"]
```

### Nesting and merging routers

Two composition tools replace Express's `app.use("/prefix", subRouter)`:

- **`nest("/prefix", router)`** mounts a router under a path prefix. Routes inside the nested router are written relative to the prefix (`/` inside `users_router` becomes `/api/users`).
- **`merge(router)`** combines two routers at the *same* level, useful for splitting a flat set of routes across modules without adding a prefix.

```rust
use axum::{routing::get, Router};

async fn health() -> &'static str { "ok" }
async fn root() -> &'static str { "API root" }

fn build() -> Router {
    let api = Router::new().route("/health", get(health));
    Router::new()
        .nest("/api/v1", api)                        // mounts under a prefix
        .merge(Router::new().route("/", get(root)))  // same level, no prefix
}
```

### Fallbacks: the typed catch-all

`fallback(handler)` registers the handler that runs when **no route matches**: the equivalent of Express's final `app.use(...)`. Unlike Express, where forgetting to put it last silently breaks it, an Axum fallback is a distinct method call and is order-independent: it only ever runs on a miss. You can attach a fallback at the top level or inside a nested router to scope a custom 404 to one subtree.

---

## Key Differences

| Concept | Express.js | Axum 0.8 |
| --- | --- | --- |
| Path param syntax | `:id` | `{id}` |
| Catch-all syntax | `*` / `/*splat` | `{*name}` |
| Param/query types | always `string` | parsed into your chosen type (`u64`, structs, …) |
| Bad param value | your code gets `NaN`/garbage | `400 Bad Request` *before* your handler runs |
| Methods per path | separate `get`/`post` calls | one `MethodRouter` per path (`get(h).post(h2)`) |
| Unsupported method | you handle it (or 404) | automatic `405` + `Allow` header |
| Sub-routers | `app.use("/x", router)` | `nest("/x", router)` or `merge(router)` |
| 404 catch-all | last `app.use(...)`; order-sensitive | `fallback(handler)`; order-independent |
| Match strategy | first registered match wins | static segments beat dynamic; conflicts panic at startup |

### Static segments win over dynamic ones

Axum's router is not "first match wins." It is a radix-tree router that prefers the **more specific** route. A literal segment always beats a capture at the same position, so you can register both `/users/me` and `/users/{id}` and they coexist:

```rust
use axum::{extract::Path, routing::get, Router};

async fn me() -> &'static str { "the special /users/me route" }
async fn by_id(Path(id): Path<u64>) -> String { format!("user {id}") }

fn build() -> Router {
    Router::new()
        .route("/users/me", get(me))       // static — higher priority
        .route("/users/{id}", get(by_id))  // dynamic
}
```

```text
$ curl -s http://127.0.0.1:3000/users/me
the special /users/me route
$ curl -s http://127.0.0.1:3000/users/5
user 5
```

In Express you would have to register `/users/me` *before* `/users/:id` to get this behavior; in Axum the order of the two `.route(...)` calls does not matter.

### Routing problems are caught at startup, not at request time

Two routes that capture the same position conflict, and Axum **panics when you build the router**, not silently on some later request. That is a feature: misconfigured routing fails fast and loud at boot.

---

## Common Pitfalls

### Pitfall 1: Using Express's `:id` syntax

A reflexive habit from Express. In Axum 0.8 a colon-prefixed segment is rejected: the server **panics at startup** with a message that tells you exactly what to do:

```rust
use axum::{routing::get, Router};

async fn h() -> &'static str { "x" }

#[tokio::main]
async fn main() {
    // panics at startup — `:id` is not valid in axum 0.8
    let app = Router::new().route("/users/:id", get(h));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 3000)).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Real panic:

```text
thread 'main' panicked at src/main.rs:9:29:
Path segments must not start with `:`. For capture groups, use `{capture}`. If you meant to literally match a segment starting with a colon, call `without_v07_checks` on the router.
```

The fix is to write `/users/{id}`. (Axum 0.7 used `:id`; the 0.7→0.8 release made the switch to `{id}`, which is why this check exists.)

### Pitfall 2: A path param type mismatch is a 400, not a panic

If a client requests `/api/users/abc` but your handler declares `Path<u64>`, Axum cannot parse `abc` as a `u64`. It does **not** crash and does **not** call your handler — it returns `400 Bad Request`:

```text
$ curl -s -i http://127.0.0.1:3000/api/users/abc
HTTP/1.1 400 Bad Request

Invalid URL: Cannot parse `abc` to a `u64`
```

This is usually what you want, but it surprises people expecting the route simply not to match. If you want `/users/abc` to fall through to a *different* route instead, extract `Path<String>` and parse manually, or use distinct path prefixes.

### Pitfall 3: A required query field is also a 400

Unlike Express where a missing `?q=` just gives you `undefined`, a non-`Option` query field that is absent fails deserialization:

```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
struct Search {
    q: String, // required — no Option, no default
}
async fn search(Query(s): Query<Search>) -> String {
    format!("searching: {}", s.q)
}
```

```text
$ curl -s -i "http://127.0.0.1:3000/search"
HTTP/1.1 400 Bad Request

Failed to deserialize query string: missing field `q`
```

Make the field `Option<String>` (or give it a `#[serde(default)]`) if the parameter is genuinely optional. For friendlier validation messages, see [Validation](/16-web-apis/09-validation/).

### Pitfall 4: Overlapping routes panic at boot

Registering two captures at the same position is a configuration error, and Axum refuses to start:

```rust
use axum::{routing::get, Router};

async fn a() -> &'static str { "a" }
async fn b() -> &'static str { "b" }

#[tokio::main]
async fn main() {
    // panics at startup — the two routes conflict
    let app = Router::new()
        .route("/users/{id}", get(a))
        .route("/users/{user_id}", get(b));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 3000)).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Real panic:

```text
thread 'main' panicked at src/main.rs:11:10:
Invalid route "/users/{user_id}": Insertion failed due to conflict with previously registered route: /users/{id}
```

The fix is to collapse them into a single `route("/users/{id}", ...)` and branch inside the handler if you really need two behaviors.

### Pitfall 5: Calling `405` a `404`

Because Express usually 404s on an unknown-method request unless you wire it up yourself, developers sometimes assume Axum does too. Axum returns a proper `405 Method Not Allowed` with an `Allow` header whenever the *path* exists but the *method* does not:

```text
$ curl -s -i -X POST http://127.0.0.1:3000/api/users/7
HTTP/1.1 405 Method Not Allowed
allow: GET,HEAD,DELETE
```

(Note that `GET` automatically brings `HEAD` along.) This is correct HTTP behavior; don't "fix" it back to a 404.

---

## Best Practices

- **Use `{id}` everywhere and grep your codebase for `":"` route literals** when migrating from Express or Axum 0.7. The startup panic will catch you, but a search is faster.
- **Build one `Router`-returning function per resource** (`fn users_router() -> Router`, `fn articles_router() -> Router`) and `nest` them. This keeps modules small, makes routes unit-testable in isolation, and mirrors how you would split Express `Router`s across files. See [Modules & Packages](/12-modules-packages/) for the file-organization patterns.
- **Prefer named-struct `Path`/`Query` extraction over positional tuples** once you have more than one or two params: `p.comment_id` is clearer than `tuple.1`, and the field names document the route.
- **Make a field `Option<T>` or give it `#[serde(default)]` only when it is truly optional.** Letting a missing required value 400 early is a feature, not a bug.
- **Version your API with `nest("/api/v1", ...)`.** Adding `/api/v2` later is then a one-line change that cannot collide with v1.
- **Attach a typed JSON `fallback`** so unmatched routes return a structured error your front-end can parse, instead of an empty 404 body.
- **Reach for `nest` for prefixed subtrees and `merge` for same-level composition.** Don't hand-build prefixes by string-concatenating paths.

---

## Real-World Example

A versioned JSON REST API for an "articles" resource, assembled from small per-resource routers with full method routing, query-driven listing, a typed `404` fallback, and a `merge`d API root. This compiles and runs as-is (`axum = "0.8"`, `tokio = { version = "1", features = ["full"] }`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`):

```rust
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone)]
struct Article {
    id: u64,
    title: String,
}

#[derive(Deserialize)]
struct NewArticle {
    title: String,
}

#[derive(Deserialize)]
struct ListParams {
    #[serde(default = "default_limit")]
    limit: u32,
    tag: Option<String>,
}
fn default_limit() -> u32 {
    10
}

async fn list_articles(Query(p): Query<ListParams>) -> Json<Vec<Article>> {
    let tag = p.tag.unwrap_or_else(|| "all".into());
    Json(vec![Article {
        id: 1,
        title: format!("limit={} tag={}", p.limit, tag),
    }])
}

async fn create_article(Json(body): Json<NewArticle>) -> (StatusCode, Json<Article>) {
    (StatusCode::CREATED, Json(Article { id: 100, title: body.title }))
}

// Returning Result<_, StatusCode> lets a handler choose its status code.
async fn get_article(Path(id): Path<u64>) -> Result<Json<Article>, StatusCode> {
    if id == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(Article { id, title: format!("Article {id}") }))
}

async fn update_article(Path(id): Path<u64>, Json(body): Json<NewArticle>) -> Json<Article> {
    Json(Article { id, title: body.title })
}

async fn delete_article(Path(_id): Path<u64>) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn health() -> &'static str {
    "ok"
}

// A typed JSON 404 instead of an empty body.
async fn api_fallback() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "route not found" })),
    )
}

// One router per resource keeps modules small and testable.
fn articles_routes() -> Router {
    Router::new()
        .route("/", get(list_articles).post(create_article))
        .route(
            "/{id}",
            get(get_article).put(update_article).delete(delete_article),
        )
}

fn api_v1() -> Router {
    Router::new()
        .route("/health", get(health))
        .nest("/articles", articles_routes())
}

fn app() -> Router {
    Router::new()
        .nest("/api/v1", api_v1())
        .merge(Router::new().route("/", get(|| async { "API root" })))
        .fallback(api_fallback)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 3000)).await.unwrap();
    println!("listening on http://127.0.0.1:3000");
    axum::serve(listener, app()).await.unwrap();
}
```

Exercising every endpoint produces these real responses:

```text
$ curl -s http://127.0.0.1:3000/
API root

$ curl -s http://127.0.0.1:3000/api/v1/health
ok

$ curl -s "http://127.0.0.1:3000/api/v1/articles?tag=rust"
[{"id":1,"title":"limit=10 tag=rust"}]

$ curl -s -X POST http://127.0.0.1:3000/api/v1/articles \
    -H 'content-type: application/json' -d '{"title":"Hello"}'
{"id":100,"title":"Hello"}

$ curl -s http://127.0.0.1:3000/api/v1/articles/5
{"id":5,"title":"Article 5"}

$ curl -s -o /dev/null -w 'status=%{http_code}\n' http://127.0.0.1:3000/api/v1/articles/0
status=404

$ curl -s -X PUT http://127.0.0.1:3000/api/v1/articles/5 \
    -H 'content-type: application/json' -d '{"title":"Edited"}'
{"id":5,"title":"Edited"}

$ curl -s -o /dev/null -w 'status=%{http_code}\n' -X DELETE http://127.0.0.1:3000/api/v1/articles/5
status=204

$ curl -s http://127.0.0.1:3000/api/v1/unknown
{"error":"route not found"}
```

This pattern, a tree of small, per-resource `Router` functions stitched together with `nest`, `merge`, and a typed `fallback`, scales from this toy example to a production API with dozens of resources. The next step is to share a database pool and config across these handlers, which is where [State Management](/16-web-apis/06-state-management/) comes in.

> **Tip:** This example deliberately returns errors as bare `StatusCode` and an ad-hoc JSON object. In a real application you would define one error type that implements `IntoResponse` so every handler can use `?` and produce consistent error bodies — see [Error Handling in Web APIs](/16-web-apis/10-error-handling-web/).

---

## Further Reading

- [Axum routing module docs](https://docs.rs/axum/latest/axum/routing/index.html) — `Router`, `MethodRouter`, `nest`, `merge`, `fallback`.
- [`axum::extract::Path`](https://docs.rs/axum/latest/axum/extract/struct.Path.html) and [`axum::extract::Query`](https://docs.rs/axum/latest/axum/extract/struct.Query.html) — the official extractor reference.
- [`matchit` crate](https://docs.rs/matchit/latest/matchit/) — the radix-tree router Axum uses, including the `{name}` / `{*name}` syntax rules.
- [Axum 0.8 changelog / migration notes](https://github.com/tokio-rs/axum/blob/main/axum/CHANGELOG.md) — the `:id` → `{id}` change and other 0.7→0.8 differences.

Within this guide:

- [Axum Setup](/16-web-apis/02-axum-setup/) and [Axum Basics](/16-web-apis/01-axum-basics/) — getting a server running before you route.
- [Extractors](/16-web-apis/04-extractors/) — how `Path`, `Query`, and `Json` actually pull data out of a request, and extractor ordering.
- [Request & Response](/16-web-apis/07-request-response/) — what handlers can return and how `IntoResponse` builds the response.
- [State Management](/16-web-apis/06-state-management/) — sharing a DB pool and config across the handlers you route to.
- [Middleware & Layers](/16-web-apis/05-middleware/) — attaching logging, CORS, and other cross-cutting behavior to routers.
- [JSON APIs](/16-web-apis/08-json-apis/) and [Validation](/16-web-apis/09-validation/) — fleshing out a CRUD resource and validating its input.
- [Static Files](/16-web-apis/18-static-files/) — the catch-all (`{*path}`) and SPA-fallback patterns in depth.
- [Error Handling in Web APIs](/16-web-apis/10-error-handling-web/): a single `AppError` type for consistent error responses.
- Foundations: [Why Rust](/00-introduction/), [Basics](/02-basics/), [Error Handling](/08-error-handling/), [Async](/11-async/), and the upcoming [Database](/17-database/) section for wiring real persistence behind these routes.

---

## Exercises

### Exercise 1: Add a nested comments resource

**Difficulty:** Easy

**Objective:** Practice path parameters and nested routers.

**Instructions:** Starting from the real-world articles API, add a comments resource so that `GET /api/v1/articles/{article_id}/comments/{comment_id}` returns a JSON object containing both ids. Use a named struct for the `Path` extraction (not a tuple). Verify that requesting `/api/v1/articles/3/comments/8` returns `{"article_id":3,"comment_id":8}`.

<details>
<summary>Solution</summary>

```rust
use axum::{extract::Path, routing::get, Json, Router};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct CommentPath {
    article_id: u64,
    comment_id: u64,
}

#[derive(Serialize)]
struct CommentRef {
    article_id: u64,
    comment_id: u64,
}

async fn get_comment(Path(p): Path<CommentPath>) -> Json<CommentRef> {
    Json(CommentRef {
        article_id: p.article_id,
        comment_id: p.comment_id,
    })
}

// Nest this under "/api/v1" alongside the articles routes:
//   .nest("/api/v1", Router::new()
//       .route("/articles/{article_id}/comments/{comment_id}", get(get_comment)))
fn app() -> Router {
    Router::new().nest(
        "/api/v1",
        Router::new().route(
            "/articles/{article_id}/comments/{comment_id}",
            get(get_comment),
        ),
    )
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 3000)).await.unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

Real response:

```text
$ curl -s http://127.0.0.1:3000/api/v1/articles/3/comments/8
{"article_id":3,"comment_id":8}
```

</details>

### Exercise 2: A search endpoint with required and optional query params

**Difficulty:** Medium

**Objective:** Distinguish required from optional query parameters and observe the 400 behavior.

**Instructions:** Write a `GET /search` handler whose query struct has a **required** `q: String`, an **optional** `limit` that defaults to `5`, and an **optional** `category: Option<String>`. Return a string summarizing all three. Then confirm with `curl` that (a) `/search?q=rust` works and uses the defaults, and (b) `/search` (no `q`) returns `400 Bad Request`.

<details>
<summary>Solution</summary>

```rust
use axum::{extract::Query, routing::get, Router};
use serde::Deserialize;

#[derive(Deserialize)]
struct SearchParams {
    q: String, // required: absence -> 400
    #[serde(default = "default_limit")]
    limit: u32,
    category: Option<String>,
}
fn default_limit() -> u32 {
    5
}

async fn search(Query(p): Query<SearchParams>) -> String {
    let category = p.category.unwrap_or_else(|| "any".into());
    format!("q={} limit={} category={}", p.q, p.limit, category)
}

fn app() -> Router {
    Router::new().route("/search", get(search))
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 3000)).await.unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

Real responses:

```text
$ curl -s "http://127.0.0.1:3000/search?q=rust"
q=rust limit=5 category=any

$ curl -s "http://127.0.0.1:3000/search?q=axum&limit=20&category=web"
q=axum limit=20 category=web

$ curl -s -i "http://127.0.0.1:3000/search"
HTTP/1.1 400 Bad Request

Failed to deserialize query string: missing field `q`
```

</details>

### Exercise 3: A static-vs-dynamic precedence route with a scoped fallback

**Difficulty:** Hard

**Objective:** Combine static-segment precedence, a catch-all, and a nested fallback.

**Instructions:** Build a router where:

1. `GET /pages/home` returns the literal string `"home page"`.
2. `GET /pages/{slug}` returns `"dynamic page: <slug>"` for any other single segment.
3. `GET /assets/{*path}` returns `"asset: <path>"` for any path under `/assets`, including ones with slashes.
4. Any other URL under the app returns a `404` with the JSON body `{"error":"not found"}`.

Verify that `/pages/home` hits the static route (not the dynamic one), `/pages/about` hits the dynamic route, `/assets/img/logo.png` captures the full sub-path, and `/totally/unknown` hits the fallback.

<details>
<summary>Solution</summary>

```rust
use axum::{extract::Path, http::StatusCode, routing::get, Json, Router};

async fn home() -> &'static str {
    "home page"
}

async fn dynamic_page(Path(slug): Path<String>) -> String {
    format!("dynamic page: {slug}")
}

async fn asset(Path(path): Path<String>) -> String {
    format!("asset: {path}")
}

async fn not_found() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "not found" })),
    )
}

fn app() -> Router {
    Router::new()
        .route("/pages/home", get(home))      // static beats dynamic, regardless of order
        .route("/pages/{slug}", get(dynamic_page))
        .route("/assets/{*path}", get(asset)) // catch-all captures slashes
        .fallback(not_found)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 3000)).await.unwrap();
    println!("listening on http://127.0.0.1:3000");
    axum::serve(listener, app()).await.unwrap();
}
```

Real responses:

```text
$ curl -s http://127.0.0.1:3000/pages/home
home page

$ curl -s http://127.0.0.1:3000/pages/about
dynamic page: about

$ curl -s http://127.0.0.1:3000/assets/img/logo.png
asset: img/logo.png

$ curl -s -i http://127.0.0.1:3000/totally/unknown
HTTP/1.1 404 Not Found
content-type: application/json

{"error":"not found"}
```

The key insight: the order of the `/pages/home` and `/pages/{slug}` registrations does not matter — Axum's router always prefers the more specific static segment.

</details>
