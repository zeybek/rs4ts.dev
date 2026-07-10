---
title: "HTTP Clients"
description: "Call JSON APIs in Rust with reqwest, the axios/fetch equivalent: typed responses, a reusable pooled client, async on Tokio, and explicit non-2xx handling."
---

In Node you reach for `fetch` (built in since Node 18) or `axios` to call an API. In Rust the de facto choice is **reqwest**, a high-level, async, batteries-included HTTP client. This chapter maps `fetch`/`axios` patterns (GET/POST JSON, headers, query strings, a reusable client) onto reqwest, and explains where the low-level **hyper** crate fits in.

---

## Quick Overview

`reqwest` is to Rust what `axios` is to Node: an ergonomic client with JSON support, a connection pool, timeouts, and a builder API. It is async by default and runs on the Tokio runtime, but also ships an optional blocking API for scripts and build tools. Like `axios`, you create **one client and reuse it** so connections (and TLS handshakes) are pooled across requests.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition, and `cargo new` selects that edition automatically. The examples here use `reqwest` 0.13, `tokio` 1.52, and `serde` 1.

---

## TypeScript/JavaScript Example

A typical Node service that talks to a JSON API with `axios`: a shared client instance, a typed GET, a POST with a JSON body, and a request with custom headers and query parameters.

```typescript
// npm install axios
import axios, { AxiosInstance } from "axios";

interface Todo {
  userId: number;
  id: number;
  title: string;
  completed: boolean;
}

interface NewPost {
  title: string;
  body: string;
  userId: number;
}

// Create ONE client and reuse it — connection keep-alive, shared config.
const client: AxiosInstance = axios.create({
  baseURL: "https://jsonplaceholder.typicode.com",
  timeout: 10_000,
  headers: { "User-Agent": "ts-to-rust-guide/1.0" },
});

async function main(): Promise<void> {
  // GET JSON — axios parses the body for you.
  const { data: todo } = await client.get<Todo>("/todos/1");
  console.log("GET ->", todo);

  // POST JSON — body is serialized, Content-Type set automatically.
  const newPost: NewPost = {
    title: "Learning Rust",
    body: "axios is familiar",
    userId: 1,
  };
  const { data: created } = await client.post("/posts", newPost);
  console.log("POST -> id =", created.id);

  // Headers + query params; axios throws on 4xx/5xx by default.
  const res = await client.get("/comments", {
    headers: { Accept: "application/json" },
    params: { postId: 1 },
  });
  console.log("status =", res.status);
}

main().catch((err) => {
  // axios bundles the response on the error object.
  console.error("request failed:", err.message);
  process.exit(1);
});
```

The same code with the built-in `fetch` is more manual: you call `res.json()` yourself, set `Content-Type` by hand, and check `res.ok` because `fetch` does **not** throw on 4xx/5xx. Keep that detail in mind; reqwest behaves like `fetch` here, not like `axios`.

---

## Rust Equivalent

```toml
# Cargo.toml — or run:
#   cargo add reqwest --features json,query
#   cargo add tokio --features full
#   cargo add serde --features derive
[dependencies]
reqwest = { version = "0.13", features = ["json", "query"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
```

```rust
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct Todo {
    id: u32,
    title: String,
    completed: bool,
}

#[derive(Debug, Serialize)]
struct NewPost {
    title: String,
    body: String,
    // JSON uses camelCase; Rust uses snake_case. serde renames the field.
    #[serde(rename = "userId")]
    user_id: u32,
}

#[derive(Debug, Deserialize)]
struct CreatedPost {
    id: u32,
    title: String,
}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    // Create ONE client and reuse it: it owns the connection pool.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("ts-to-rust-guide/1.0")
        .build()?;

    // GET JSON: .json::<T>() deserializes the body for you.
    let todo: Todo = client
        .get("https://jsonplaceholder.typicode.com/todos/1")
        .send()
        .await?
        .json()
        .await?;
    println!("GET -> {todo:?}");

    // POST JSON: .json(&body) serializes and sets Content-Type.
    let new_post = NewPost {
        title: "Learning Rust".to_string(),
        body: "reqwest is great".to_string(),
        user_id: 1,
    };
    let created: CreatedPost = client
        .post("https://jsonplaceholder.typicode.com/posts")
        .json(&new_post)
        .send()
        .await?
        .json()
        .await?;
    println!("POST -> id={}, title={:?}", created.id, created.title);

    // Headers + query params + status handling.
    let resp = client
        .get("https://jsonplaceholder.typicode.com/comments")
        .header("Accept", "application/json")
        .query(&[("postId", "1")])
        .send()
        .await?;
    println!("status = {}", resp.status());
    let resp = resp.error_for_status()?; // turn 4xx/5xx into an Err
    let body = resp.text().await?;
    println!("body bytes = {}", body.len());

    Ok(())
}
```

Running this prints (real output):

```text
GET -> Todo { id: 1, title: "delectus aut autem", completed: false }
POST -> id=101, title="Learning Rust"
status = 200 OK
body bytes = 1510
```

---

## Detailed Explanation

Read the request chain as a sequence: **build a request, send it, then read the response**. Each `await` marks a point where the task yields back to the runtime.

### `reqwest::Client` and the builder

`reqwest::Client::builder()` returns a `ClientBuilder`. You set defaults — `.timeout(...)`, `.user_agent(...)`, `.default_headers(...)`, redirect and connection-pool policy — then `.build()?` produces a `Client`. The `?` propagates a `reqwest::Error` if, say, TLS fails to initialize.

> **Tip:** `Client` is an `Arc` inside, so cloning it is cheap — it shares the same connection pool. Pass `client.clone()` into tasks freely; do **not** wrap it in another `Arc`.

### `.get(url)` / `.post(url)` → `RequestBuilder`

These return a `RequestBuilder` you decorate before sending: `.header(...)`, `.query(...)`, `.json(...)`, `.bearer_auth(...)`, `.basic_auth(...)`, `.timeout(...)` (per-request override). Nothing has been sent yet; this mirrors `axios.create(...).get(...)` building a config object.

### `.send().await?` → `Response`

`.send()` returns a **future**; `.await` drives it to completion and `?` unwraps the `Result`. The important part: the future is **lazy**. Unlike a JavaScript Promise, which starts running the moment you call `axios.get(...)`, a reqwest future does *nothing* until you `.await` it (or hand it to the runtime via `tokio::join!`/`tokio::spawn`). Forgetting `.await` is the single most common reqwest mistake; the compiler catches it. See [Common Pitfalls](#common-pitfalls).

### Reading the body: two `await`s

JSON decoding is itself async because the body streams in. So `.json::<T>().await?` is a second `await`: read the bytes, then deserialize with serde into your `T`. The chain `client.get(...).send().await?.json().await?` is the reqwest idiom for "GET and parse JSON," equivalent to `(await client.get<T>(url)).data` in axios. Other readers: `.text().await?`, `.bytes().await?`, or `.json::<serde_json::Value>().await?` for untyped JSON.

### `.query(&[("postId", "1")])`

This URL-encodes a `Serialize` value into the query string: pairs like `&[("a", "1"), ("b", "2")]`, a `HashMap`, or your own `#[derive(Serialize)]` struct. It is the equivalent of axios's `params` option. In reqwest 0.13 this lives behind an optional `query` feature (see pitfalls).

### `.error_for_status()?`

By default reqwest does **not** treat a 404 or 500 as an error — like `fetch`, a completed HTTP exchange is a success even if the server says "Not Found." Call `.error_for_status()` to convert any 4xx/5xx into an `Err(reqwest::Error)`, recovering axios's throw-on-non-2xx behavior. This is one place the `fetch`/`axios` analogy breaks down, so make the choice explicit.

---

## Key Differences

| Concern | axios / fetch (Node) | reqwest (Rust) |
| --- | --- | --- |
| Install | `npm install axios` (fetch built in) | `cargo add reqwest --features json,query` |
| JSON parse | `res.data` (axios) / `await res.json()` (fetch) | `.json::<T>().await?` into a typed struct |
| JSON body | `client.post(url, obj)` | `.json(&obj)` (needs `json` feature) |
| Non-2xx | axios throws; fetch does **not** | does **not** by default; call `.error_for_status()` |
| Reuse | `axios.create(...)` instance | `reqwest::Client` (clone is cheap, pools connections) |
| Concurrency | `Promise.all([...])` | `tokio::join!` / `futures::future::try_join_all` |
| Laziness | Promise starts immediately | future is lazy until `.await` |
| Timeout | `timeout` option | `.timeout(Duration)` on client or request |
| Query string | `params: { ... }` | `.query(&[...])` (needs `query` feature) |
| Blocking | n/a (always async) | `reqwest::blocking` (opt-in feature) |
| TLS | OpenSSL/system, transparent | rustls by default (pure-Rust), or native-tls |

A few differences worth internalizing:

- **Typed by default.** axios returns `any` unless you annotate `get<T>`, and even then the type is erased; nothing checks the wire data matches. reqwest deserializes into a concrete struct, and a shape mismatch is a real runtime `Err` you must handle, not a silent `undefined` three lines later.
- **rustls, not OpenSSL.** reqwest 0.13 uses the pure-Rust `rustls` stack by default, so there is no system OpenSSL to install, great for slim Docker images and cross-compilation. You can opt into `native-tls` if you need the OS trust store.
- **One runtime.** reqwest's async API needs a runtime (Tokio). The `#[tokio::main]` attribute starts one. There is no global event loop the way Node has one for free.

---

## Common Pitfalls

### Forgetting `.await` on `.send()`

A reqwest future does nothing until awaited. If you forget, you are holding a `Future`, not a `Response`, and method calls fail to resolve:

```rust
#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    let resp = client.get("https://example.com").send(); // missing .await
    let body = resp.json::<serde_json::Value>().await?;
    println!("{body:?}");
    Ok(())
}
```

The real compiler error:

```text
error[E0599]: no method named `json` found for opaque type `impl Future<Output = Result<Response, reqwest::Error>>` in the current scope
 --> src/main.rs:5:21
  |
5 |     let body = resp.json::<serde_json::Value>().await?;
  |                     ^^^^ method not found in `impl Future<Output = Result<Response, reqwest::Error>>`
```

The phrase `impl Future<Output = ...>` is the tell: you forgot to `.await` the previous step. Add `.await?` after `.send()`.

### The `query` (or `json`) feature is not enabled

reqwest is feature-gated to keep builds lean. `.json(...)` needs the `json` feature and `.query(...)` needs the `query` feature; without them the methods simply do not exist:

```text
error[E0599]: no method named `query` found for struct `RequestBuilder` in the current scope
  --> src/main.rs:61:10
   |
61 |         .query(&[("page", "2"), ("limit", "10")])
   |          ^^^^^ method not found in `RequestBuilder`
```

The fix is in `Cargo.toml`, not the code: `cargo add reqwest --features json,query`. This trips up Node developers because `axios` bundles everything; reqwest makes you pay only for what you use.

### Expecting a 404 to be an error

```rust
// (fragment) assume `url: &str`, a `client: reqwest::Client`, and a `User` deserialize type.
// This does NOT return Err on a 404 — the request "succeeded".
let resp = client.get(url).send().await?;
let user: User = resp.json().await?; // may instead fail to decode an error body
```

Like `fetch`, reqwest only errors on transport problems (DNS, connection, timeout), not on HTTP status. Add `.error_for_status()?` after `.send().await?` to opt into axios-style behavior, or inspect `resp.status()` yourself.

### Creating a new `Client` per request

```rust
// Anti-pattern: rebuilds the connection pool and re-handshakes TLS each call.
async fn fetch(url: &str) -> Result<String, reqwest::Error> {
    let client = reqwest::Client::new(); // new pool every time!
    client.get(url).send().await?.text().await
}
```

`reqwest::Client::new()` allocates a fresh connection pool. Build it once and share clones, exactly as you would create one `axios` instance, not one per call. The free function `reqwest::get(url)` is convenient for one-off scripts but also spins up a throwaway client, so avoid it in hot paths.

### Calling the blocking client inside an async runtime

`reqwest::blocking` spins up its own internal Tokio runtime. Calling it from inside `#[tokio::main]` will panic, because the blocking client tries to create and drop that runtime inside the async one. The real message you see is:

```text
thread 'main' panicked at .../tokio-1.52.3/src/runtime/blocking/shutdown.rs:51:21:
Cannot drop a runtime in a context where blocking is not allowed. This happens when a runtime is dropped from within an asynchronous context.
```

Use the async `Client` in async code; reserve `reqwest::blocking` for genuinely synchronous programs.

---

## Best Practices

- **Build the client once, clone to share.** Store a `reqwest::Client` in your app state (or behind a typed wrapper) and clone it into handlers and tasks. Cloning shares the pool.
- **Always set a timeout.** The default has no overall timeout. Set `.timeout(Duration::from_secs(...))` on the client (and override per request when needed) so a hung server cannot stall a task forever.
- **Deserialize into concrete types.** Prefer `#[derive(Deserialize)]` structs over `serde_json::Value`; you get validation and autocomplete instead of stringly-typed lookups. Use `#[serde(rename = "...")]` or `#[serde(rename_all = "camelCase")]` to bridge JSON casing.
- **Call `.error_for_status()` when you want axios semantics.** Be explicit about whether a non-2xx is an error in your domain.
- **Enable only the features you use.** `json`, `query`, `stream`, `gzip`, `brotli`, `cookies`, `multipart`, `blocking` are all opt-in. Smaller feature sets mean faster builds and smaller binaries.
- **Run concurrent requests with `tokio::join!` or `try_join_all`.** Sequential `await`s are like `await`ing Promises one at a time; batch them when they are independent.
- **Wrap errors with context in apps.** Convert `reqwest::Error` into your own error type (or use `anyhow` with `.context(...)`) so failures say *which* request failed. See [Error Handling](/08-error-handling/).

---

## Real-World Example

A small, production-flavored typed API client: it owns one reusable `reqwest::Client`, sets a timeout and user agent, attaches context to errors with `anyhow`, and runs several requests concurrently. This is the shape you would expose as a service layer.

```toml
# cargo add reqwest --features json
# cargo add tokio --features full
# cargo add serde --features derive
# cargo add anyhow
[dependencies]
reqwest = { version = "0.13", features = ["json"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
```

```rust
use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

/// A typed client that holds one reusable `reqwest::Client`.
#[derive(Clone)]
struct ApiClient {
    http: reqwest::Client,
    base_url: String,
}

impl ApiClient {
    fn new(base_url: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self { http, base_url: base_url.into() })
    }

    async fn get_user(&self, id: u32) -> Result<User> {
        let url = format!("{}/users/{id}", self.base_url);
        let user = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .context("server returned an error status")?
            .json::<User>()
            .await
            .context("failed to decode User JSON")?;
        Ok(user)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let api = ApiClient::new("https://jsonplaceholder.typicode.com")?;

    // Fire three independent requests concurrently and await them all.
    let (a, b, c) = tokio::join!(
        api.get_user(1),
        api.get_user(2),
        api.get_user(3),
    );
    for user in [a?, b?, c?] {
        println!("#{} {} <{}>", user.id, user.name, user.email);
    }

    // A 404 becomes an Err thanks to .error_for_status().
    match api.get_user(99_999).await {
        Ok(u) => println!("got {u:?}"),
        Err(e) => println!("expected error: {e:#}"),
    }

    Ok(())
}
```

Real output:

```text
#1 Leanne Graham <Sincere@april.biz>
#2 Ervin Howell <Shanna@melissa.tv>
#3 Clementine Bauch <Nathan@yesenia.net>
expected error: server returned an error status: HTTP status client error (404 Not Found) for url (https://jsonplaceholder.typicode.com/users/99999)
```

> **Note:** `{e:#}` prints the full `anyhow` error chain — the top-level context plus the underlying cause — which is why you see both "server returned an error status" and the reqwest detail.

### Where hyper fits in

reqwest is built **on top of** [hyper](https://hyper.rs/), the low-level HTTP library that also powers Axum and many other Rust web stacks (see [Web Frameworks](/23-ecosystem/01-web-frameworks/)). hyper gives you fine-grained control over the connection, the HTTP state machine, and bring-your-own connection pool. But you assemble URIs, bodies, and TLS yourself. For application code that just needs "call this JSON API," reqwest is the right tool; reach for hyper only when you are building infrastructure (a proxy, a custom client with unusual pooling, or your own framework) where reqwest's conveniences get in the way. Think of it as `fetch`/`axios` (reqwest) versus Node's raw `http`/`https` modules (hyper).

### Shared default headers

When every request needs the same headers (an `Authorization` token, an `Accept`), set them once with `.default_headers(...)` instead of repeating `.header(...)`:

```rust
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer my-token"));

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()?;

    let status = client
        .get("https://jsonplaceholder.typicode.com/todos/1")
        .send()
        .await?
        .status();
    println!("status with default headers = {status}");
    Ok(())
}
```

Real output: `status with default headers = 200 OK`. For a single per-request token, `.bearer_auth("my-token")` on the `RequestBuilder` is shorter.

---

## Further Reading

- [reqwest documentation (docs.rs)](https://docs.rs/reqwest) — the full API, feature flags, and examples.
- [reqwest on crates.io](https://crates.io/crates/reqwest): current version and feature list.
- [hyper](https://hyper.rs/) — the low-level HTTP library reqwest is built on.
- [serde documentation](https://serde.rs/): `#[derive(Serialize/Deserialize)]`, field renaming, and attributes.
- [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/) — where reqwest sits among the most-used crates and the npm packages they replace.
- [Async Runtimes](/23-ecosystem/02-async-runtimes/): Tokio, the runtime reqwest's async API runs on.
- [Web Frameworks](/23-ecosystem/01-web-frameworks/) — Axum and friends, which (like reqwest) build on hyper.
- [Date and Time](/23-ecosystem/07-date-time/) and [Regular Expressions with the `regex` Crate](/23-ecosystem/08-regex/): sibling ecosystem chapters.
- [Error Handling](/08-error-handling/): `Result`, `?`, `anyhow`, and `thiserror` for request errors.
- [Async](/11-async/): `async`/`await`, futures' laziness, and concurrency primitives like `tokio::join!`.
- [Tooling](/24-tooling/) — managing dependencies and features with Cargo.

---

## Exercises

### Exercise 1: Typed GET with query and auth

**Difficulty:** Beginner

**Objective:** Fetch a filtered list from a JSON API into a typed `Vec`, sending a query parameter and an `Authorization` header.

**Instructions:** Using `reqwest` with the `json` and `query` features, GET `https://jsonplaceholder.typicode.com/posts`, filtered to `userId=1` via `.query(...)`, with an `Authorization: Bearer demo-token` header. Deserialize the body into a `Vec<Post>` where `Post` has `id: u32` and `title: String`. Call `.error_for_status()` before decoding, then print how many posts came back and the first title.

<details>
<summary>Solution</summary>

```toml
# cargo add reqwest --features json,query
# cargo add tokio --features full
# cargo add serde --features derive
# cargo add anyhow
[dependencies]
reqwest = { version = "0.13", features = ["json", "query"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
```

```rust
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Post {
    id: u32,
    title: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = reqwest::Client::new();

    let posts: Vec<Post> = client
        .get("https://jsonplaceholder.typicode.com/posts")
        .query(&[("userId", "1")])
        .header(reqwest::header::AUTHORIZATION, "Bearer demo-token")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    println!("user 1 has {} posts", posts.len());
    println!("first: {:?}", posts.first());
    Ok(())
}
```

Real output:

```text
user 1 has 10 posts
first: Some(Post { id: 1, title: "sunt aut facere repellat provident occaecati excepturi optio reprehenderit" })
```

</details>

### Exercise 2: POST JSON and read the created resource

**Difficulty:** Intermediate

**Objective:** Serialize a struct as a JSON body, POST it, and deserialize the response into a different type.

**Instructions:** Define `NewComment { name: String, body: String, post_id: u32 }` with `#[serde(rename = "postId")]` on the id field. POST it to `https://jsonplaceholder.typicode.com/comments` with `.json(&body)`. Deserialize the response into `CreatedComment { id: u32 }` and print the new id. Reuse a single `Client` built with a 10-second timeout.

<details>
<summary>Solution</summary>

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize)]
struct NewComment {
    name: String,
    body: String,
    #[serde(rename = "postId")]
    post_id: u32,
}

#[derive(Debug, Deserialize)]
struct CreatedComment {
    id: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let new_comment = NewComment {
        name: "reqwest reader".to_string(),
        body: "Great chapter!".to_string(),
        post_id: 1,
    };

    let created: CreatedComment = client
        .post("https://jsonplaceholder.typicode.com/comments")
        .json(&new_comment)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    println!("created comment id = {}", created.id);
    Ok(())
}
```

The placeholder API echoes back a newly assigned id (it prints `created comment id = 501`). The point is that the request body is serialized from your struct and the response is parsed into a different one, strongly typed on both ends.

</details>

### Exercise 3: Concurrent fetch with graceful failure

**Difficulty:** Advanced

**Objective:** Fetch several resources concurrently and collect results, distinguishing successes from failures instead of aborting on the first error.

**Instructions:** Given a list of user ids `[1, 2, 99999, 3]`, fetch `https://jsonplaceholder.typicode.com/users/{id}` for each **concurrently**. Use `.error_for_status()` so a 404 becomes an `Err`. Collect the outcomes so that the missing user (99999) does not abort the others; print each success and each failure. Hint: spawn the futures, collect them into a `Vec`, and `await` each, matching on the `Result` rather than using `?`.

<details>
<summary>Solution</summary>

```rust playground
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct User {
    id: u32,
    name: String,
}

async fn get_user(client: &reqwest::Client, id: u32) -> Result<User, reqwest::Error> {
    client
        .get(format!("https://jsonplaceholder.typicode.com/users/{id}"))
        .send()
        .await?
        .error_for_status()?
        .json::<User>()
        .await
}

#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();
    let ids = [1u32, 2, 99_999, 3];

    // Build one future per id, then await them all concurrently.
    let futures = ids.iter().map(|&id| {
        let client = client.clone();
        async move { (id, get_user(&client, id).await) }
    });
    let results = futures::future::join_all(futures).await;

    for (id, result) in results {
        match result {
            Ok(user) => println!("ok  #{} {}", user.id, user.name),
            Err(e) => println!("err #{id}: {}", e.status()
                .map(|s| s.to_string())
                .unwrap_or_else(|| e.to_string())),
        }
    }
}
```

Add the `futures` crate: `cargo add futures`. `join_all` runs every future concurrently and returns a `Vec` of results in order, so one 404 cannot abort the rest. The successful users print with their names; user 99999 prints its `404 Not Found` status. (For a fixed, small number of requests you could use `tokio::join!` instead; `join_all` shines when the count is dynamic.)

</details>
