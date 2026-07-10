---
title: "Sessions and Cookies"
description: "Replace Express express-session with Axum's tower-sessions: a session layer, a Session extractor, server-side stores (memory, SQLx, Redis), signed cookies, and CSRF."
---

## Quick Overview

A **session** is how a stateless HTTP server remembers who you are between requests: the server stores some data, hands the browser a small opaque **cookie**, and the browser returns that cookie on every subsequent request. In Express you reach for `express-session`; in Axum the equivalent is the **`tower-sessions`** crate, which adds a session layer to your `Router` and gives every handler a `Session` extractor. This chapter maps the Express session/cookie workflow onto Axum, covers server-side stores (memory, SQLx, Redis) versus signed client-side cookies, and explains the **CSRF** considerations that come with cookie-based auth.

> **Note:** This page is about *sessions* — server-remembered state keyed by a cookie. If you are issuing stateless bearer tokens instead, see [JWT](/16-web-apis/13-jwt/). For the broader question of where auth checks live (guards, middleware), see [Authentication](/16-web-apis/12-authentication/).

---

## TypeScript/JavaScript Example

Here is a typical Express app using `express-session` with a cookie-based login flow:

```typescript
// app.ts — Express sessions with express-session
import express, { Request, Response } from "express";
import session from "express-session";

// Augment the session type so `req.session.user` is typed.
declare module "express-session" {
  interface SessionData {
    user?: { id: number; username: string };
  }
}

const app = express();
app.use(express.json());
const sessionSecret = process.env.SESSION_SECRET;
if (!sessionSecret || Buffer.byteLength(sessionSecret, "utf8") < 32) {
  throw new Error("SESSION_SECRET must contain at least 32 unpredictable bytes");
}

app.use(
  session({
    name: "id", // cookie name
    secret: sessionSecret,
    resave: false,
    saveUninitialized: false,
    cookie: {
      httpOnly: true, // JS cannot read the cookie
      secure: true, // HTTPS only
      sameSite: "lax", // sent on top-level navigations, blocked on cross-site POSTs
      maxAge: 1000 * 60 * 60 * 2, // 2 hours
    },
  }),
);

app.post("/login", (req: Request, res: Response) => {
  const { username, password } = req.body;
  if (username !== "alice" || password !== "correct horse") {
    return res.status(401).send("invalid credentials");
  }
  // Prevent session fixation: rotate the session ID on privilege change.
  req.session.regenerate(() => {
    req.session.user = { id: 1, username };
    res.send("logged in");
  });
});

app.get("/me", (req: Request, res: Response) => {
  if (!req.session.user) return res.status(401).send("not logged in");
  res.json(req.session.user);
});

app.post("/logout", (req: Request, res: Response) => {
  req.session.destroy(() => res.redirect("/"));
});

app.listen(3000);
```

**Key points:**

- `app.use(session(...))` adds session middleware globally; every handler then sees `req.session`.
- The cookie carries only an opaque **session ID**; the `user` object lives server-side (in memory by default, or Redis/Postgres in production).
- `req.session.user = ...` writes; reading `req.session.user` reads; `req.session.destroy()` deletes.
- `regenerate()` swaps the session ID to defend against **session fixation**.

---

## Rust Equivalent

Axum delegates sessions to the **`tower-sessions`** crate. You add a `SessionManagerLayer` (the analog of `app.use(session(...))`) and every handler can extract a `Session`. Add the dependencies:

```bash
cargo new my-api
cd my-api
cargo add axum@0.8
cargo add tokio@1 --features full
cargo add tower-sessions@0.14
cargo add serde --features derive
cargo add time
```

> **Note:** `tower-sessions` is pinned to `0.14` here, not the absolute latest `tower-sessions` release. The official store crates (`tower-sessions-sqlx-store`, `tower-sessions-redis-store`) currently target the `tower-sessions-core` `0.14` line, so pinning the facade crate to `0.14` keeps every part of the ecosystem on one `-core` version. The `Session` API used below is identical across these releases. See the version-mismatch pitfall below for the exact error you hit if you mix them.

```rust
// src/main.rs — equivalent Axum session flow
use axum::{
    Router,
    routing::{get, post},
    extract::Json,
    response::{IntoResponse, Redirect},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tower_sessions::{Session, SessionManagerLayer, MemoryStore, Expiry};
use time::Duration;

// What we keep in the session. Any Serialize + Deserialize type works.
#[derive(Serialize, Deserialize, Clone)]
struct SessionUser {
    id: u64,
    username: String,
}

// The key under which we store the user inside the session map.
const USER_KEY: &str = "user";

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

// POST /login — verify credentials, then write the user into the session.
async fn login(session: Session, Json(form): Json<LoginForm>) -> impl IntoResponse {
    // Pretend we looked this up in a database and verified the password hash.
    if form.username != "alice" || form.password != "correct horse" {
        return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response();
    }

    let user = SessionUser { id: 1, username: form.username };

    // Rotate the session ID on privilege change to prevent session fixation.
    session.cycle_id().await.expect("failed to cycle session id");
    session.insert(USER_KEY, user).await.expect("failed to write session");

    (StatusCode::OK, "logged in").into_response()
}

// GET /me — read the user back out of the session (None if not logged in).
async fn me(session: Session) -> impl IntoResponse {
    match session.get::<SessionUser>(USER_KEY).await {
        Ok(Some(user)) => (StatusCode::OK, Json(user)).into_response(),
        Ok(None) => (StatusCode::UNAUTHORIZED, "not logged in").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "session error").into_response(),
    }
}

// POST /logout — destroy the whole server-side session.
async fn logout(session: Session) -> impl IntoResponse {
    session.delete().await.expect("failed to delete session");
    Redirect::to("/")
}

#[tokio::main]
async fn main() {
    // In-memory store: fine for a demo, lost on restart. Use Redis/SQL in prod.
    let store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(store)
        .with_secure(true)                       // Secure flag: HTTPS only
        .with_http_only(true)                    // JS cannot read the cookie
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_name("id")                         // cookie name
        .with_expiry(Expiry::OnInactivity(Duration::hours(2)));

    let app = Router::new()
        .route("/login", post(login))
        .route("/me", get(me))
        .route("/logout", post(logout))
        .layer(session_layer);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Hitting this server through HTTPS shows the same cookie-based flow as Express. With `with_secure(true)` as configured above, `POST /login` returns:

```text
HTTP/1.1 200 OK
set-cookie: id=KT9nUyq52viouFYYwWJFkQ; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=7200
```

That `id=...` value is the opaque session ID. Note it is **not** the username or any user data. Sending the cookie back to `GET /me` returns the stored user (`{"id":1,"username":"alice"}`), and `POST /logout` returns `303 See Other` and wipes the server-side session, so a subsequent `GET /me` replies `not logged in`.

> **Tip:** The safe default above intentionally does not work through plain HTTP: browsers refuse to return a `Secure` cookie there. For a local-only `http://localhost` exercise you may temporarily set `with_secure(false)`, but keep that override in development configuration and require `true` at startup in production.

---

## Detailed Explanation

### The layer is the middleware

`SessionManagerLayer::new(store)` is the Tower layer that does what `app.use(session(...))` does in Express: on each request it reads the session-ID cookie, loads (or lazily creates) the session, and after the handler runs it persists any changes and emits a `Set-Cookie` header if needed. Because it is a layer, it follows the same composition rules as every other Axum middleware; see [Middleware and Layers](/16-web-apis/05-middleware/). Apply it with `.layer(...)` so it wraps all routes that need sessions.

### `Session` is an extractor

Once the layer is installed, any handler can take a `Session` parameter and Axum injects it, exactly like `Path`, `Query`, or `State` (see [Extractors](/16-web-apis/04-extractors/)). The `Session` itself is a cheap, cloneable handle to an in-memory map that the layer flushes to the store at the end of the request. Its core methods mirror `req.session`:

| Express                        | `tower-sessions`                              |
| ------------------------------ | --------------------------------------------- |
| `req.session.user = value`     | `session.insert("user", value).await?`        |
| `req.session.user` (read)      | `session.get::<T>("user").await?`             |
| `delete req.session.user`      | `session.remove::<T>("user").await?`          |
| `req.session.destroy(cb)`      | `session.delete().await?`                     |
| `req.session.regenerate(cb)`   | `session.cycle_id().await?`                   |

### Everything is `async` and returns `Result`

This is the biggest difference from Express. `req.session.user` in Express is a synchronous property access. In Rust, `session.get`/`insert`/`delete` are **async** because the backing store might be a database or Redis, and they return `Result` because that I/O can fail. So you `.await` them and handle the error. That is why `me` matches on `Ok(Some(_))` / `Ok(None)` / `Err(_)` instead of a simple truthiness check. For why these futures must be awaited and do nothing until polled, see [Async/Await](/11-async/).

### Values must be serializable

`session.insert` has the signature `pub async fn insert(&self, key: &str, value: impl Serialize)`. The store serializes your value (the memory store keeps the deserialized form; SQL/Redis stores serialize to bytes), and `get::<T>` deserializes it back, so `T` must implement `serde::Serialize + DeserializeOwned`. That is why `SessionUser` derives `Serialize, Deserialize`. This is stricter than Express, where you can stash any JS object on `req.session` — but it is the same discipline as [Serialization with serde](/15-serialization/).

### `cycle_id` defends against session fixation

`session.cycle_id()` issues a fresh session ID while keeping the data, the analog of Express's `req.session.regenerate()`. Call it right after a successful login so an attacker who planted a known session ID before login cannot ride the authenticated session afterward.

---

## Key Differences

| Concept                | Express (`express-session`)                  | Axum (`tower-sessions`)                                |
| ---------------------- | -------------------------------------------- | ------------------------------------------------------ |
| Install                | `app.use(session({...}))`                    | `.layer(SessionManagerLayer::new(store))`              |
| Access in handler      | `req.session` (always present)               | `session: Session` extractor parameter                 |
| Read/write             | Synchronous property access                  | `async` methods returning `Result`                     |
| Stored value type      | Any JS value                                 | Must be `Serialize + DeserializeOwned`                 |
| Default store          | `MemoryStore` (with a warning)               | `MemoryStore` (explicit; you must choose)              |
| Cookie config          | `cookie: { httpOnly, secure, sameSite }`     | `.with_http_only(_)`, `.with_secure(_)`, `.with_same_site(_)` |
| Rotate ID              | `req.session.regenerate(cb)`                 | `session.cycle_id().await?`                            |
| Destroy                | `req.session.destroy(cb)`                    | `session.delete().await?`                              |
| Signing secret         | `secret:` (always signs the cookie)          | The store ID is opaque; cookies are signed only with the `signed`/`private` cookie controllers |

### Why an opaque ID instead of a signed payload?

Both frameworks default to a **server-side** session: the cookie holds only a random ID and the real data lives in the store. The cookie does not need to be cryptographically signed for the data to be safe, because the data never leaves the server: guessing another user's random ID is the only attack, and IDs are 128-bit random values. The alternative — putting signed data *in* the cookie — is covered under "Best Practices" below; it trades server storage for cookie size and revocation difficulty.

### Lazy persistence

`tower-sessions` only writes to the store and only emits a `Set-Cookie` header when the session actually changed during the request. A handler that merely reads the session produces no `Set-Cookie`. This is similar to `express-session`'s `saveUninitialized: false` / `resave: false`, but it is the default and not configurable away.

---

## Common Pitfalls

### Pitfall 1: Storing a non-serializable value

Because the store must serialize whatever you insert, trying to stash a type that is not `Serialize` is a compile error, not a runtime surprise:

```rust
use tower_sessions::Session;
use std::time::Instant; // Instant is NOT Serialize

async fn handler(session: Session) {
    // does not compile (E0277: Instant does not implement serde::Serialize)
    session.insert("started", Instant::now()).await.unwrap();
}
```

The real compiler error is:

```text
error[E0277]: the trait bound `std::time::Instant: serde::Serialize` is not satisfied
   --> src/bin/err_nonserialize.rs:7:31
    |
  7 |     session.insert("started", Instant::now()).await.unwrap();
    |             ------            ^^^^^^^^^^^^^^ the trait `serde_core::ser::Serialize` is not implemented for `std::time::Instant`
    |             |
    |             required by a bound introduced by this call
    |
    = note: for types from other crates check whether the crate offers a `serde` feature flag
note: required by a bound in `tower_sessions::Session::insert`
```

The fix is to store something serializable. For a timestamp, use a `time::OffsetDateTime` (with serde) or store the seconds since epoch as an `i64`.

### Pitfall 2: Forgetting the layer

If you take a `Session` extractor but never add the `SessionManagerLayer`, the handler will fail at runtime, not compile time: the extractor cannot find the session in the request extensions. The error surfaces as a `500` with a message about a missing required extension. Always remember the layer is what *provides* the extractor, exactly like `State` needs `.with_state(...)`. See [State Management](/16-web-apis/06-state-management/) for the same provide-then-extract pattern.

### Pitfall 3: Mixing incompatible store versions

The store crates lag the facade crate, so a naive `cargo add tower-sessions` + `cargo add tower-sessions-sqlx-store` can pull in two different `tower-sessions-core` versions. The trait bound then fails with a confusing message:

```text
error[E0277]: the trait bound `SqliteStore: SessionStore` is not satisfied
   |
note: there are multiple different versions of crate `tower_sessions_core` in the dependency graph
   |
  4 | use tower_sessions::{Session, SessionManagerLayer, Expiry};
    |     -------------- one version of crate `tower_sessions_core` used here, as a dependency of crate `tower_sessions`
  5 | use tower_sessions_sqlx_store::SqliteStore;
    |     ------------------------- one version of crate `tower_sessions_core` used here, as a dependency of crate `tower_sessions_sqlx_store`
```

The fix is to pin `tower-sessions` to the version line the store targets (here, `tower-sessions = "0.14"` to match `tower-sessions-sqlx-store = "0.15"`). When you add a store, check its `tower-sessions-core` dependency and align the facade crate to it. This is the same kind of duplicate-version trap covered in [Modules and Packages](/12-modules-packages/).

### Pitfall 4: Assuming `same_site` "lax" stops all CSRF

`SameSite=Lax` (the default both frameworks use) blocks the cookie on *cross-site* sub-requests like an attacker's hidden `<form>` POST, which neutralizes the classic CSRF attack for most apps. But it is not a complete defense: `GET` requests count as "safe" navigations and still carry the cookie, and `SameSite` is browser-enforced (older or non-browser clients ignore it). For state-changing endpoints, add an explicit anti-CSRF token (shown below). Do **not** treat `SameSite=Lax` as a license to skip CSRF tokens on sensitive operations.

### Pitfall 5: Leaving the `MemoryStore` in production

`MemoryStore` keeps sessions in a process-local `HashMap`: they vanish on restart and are not shared across multiple instances behind a load balancer (a user "logs out" whenever they hit a different replica). It is great for tests and demos, but pick a persistent, shared store (SQL or Redis) before you scale past one process — exactly as you would replace `express-session`'s default `MemoryStore`, which prints a warning for the same reason.

---

## Best Practices

### Always set the cookie security flags

Configure the layer explicitly rather than relying on defaults:

```rust
use tower_sessions::{SessionManagerLayer, MemoryStore, Expiry, cookie::SameSite};
use time::Duration;

fn session_layer() -> SessionManagerLayer<MemoryStore> {
    SessionManagerLayer::new(MemoryStore::default())
        .with_secure(true)              // HTTPS only
        .with_http_only(true)           // not readable from JS — blocks XSS cookie theft
        .with_same_site(SameSite::Lax)  // blocks cross-site sends of the cookie
        .with_name("id")                // a neutral name that does not advertise the stack
        .with_expiry(Expiry::OnInactivity(Duration::hours(2)))
}
```

`HttpOnly` is your XSS mitigation (script cannot read the cookie), `Secure` is your eavesdropping mitigation, and `SameSite` is your first line of CSRF defense.

### Wrap the session API in a typed helper

Repeating `session.get::<SessionUser>("user")` everywhere invites typos in the key. Centralize it:

```rust
use tower_sessions::Session;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct SessionUser {
    id: u64,
    username: String,
}

const USER_KEY: &str = "user";

async fn current_user(session: &Session) -> Option<SessionUser> {
    session.get::<SessionUser>(USER_KEY).await.ok().flatten()
}

async fn set_current_user(session: &Session, user: SessionUser) {
    let _ = session.insert(USER_KEY, user).await;
}
```

This is the seam where a custom guard extractor naturally lives: turn the session lookup into an `AuthUser` extractor so handlers can declare "I need a logged-in user" in their signature (see the Real-World Example).

### Choose the right store for your topology

- **`MemoryStore`** — tests and single-process demos only.
- **`tower-sessions-sqlx-store`** (`SqliteStore` / `PostgresStore` / `MySqlStore`) — when you already run a SQL database; survives restarts, shared across replicas. Reuses the [database connection pool](/17-database/) you already have.
- **`tower-sessions-redis-store`** (`RedisStore`) — when you want fast, ephemeral, easily-expiring session storage and already run Redis.

### Prefer server-side sessions; reach for signed cookies deliberately

A client-side **signed cookie** stores the data *in* the cookie, signed so the client cannot tamper with it (but can read it). It needs no server storage, which is appealing for horizontally-scaled stateless services. The trade-offs: the cookie is bigger, the data is visible to the client, and you cannot revoke a session before it expires (there is nothing server-side to delete). Use `axum-extra`'s `SignedCookieJar` (or `PrivateCookieJar` for encrypted, non-readable values) when those trade-offs are acceptable:

```bash
cargo add axum-extra --features cookie-signed
```

```rust
use axum::{Router, routing::get, response::IntoResponse};
use axum_extra::extract::cookie::{Cookie, Key, SignedCookieJar};

// A signed (tamper-evident) client-side cookie — no server storage at all.
// The value is visible to the client but cannot be modified without the key.
async fn set_cookie(jar: SignedCookieJar) -> impl IntoResponse {
    jar.add(Cookie::new("theme", "dark"))
}

async fn read_cookie(jar: SignedCookieJar) -> impl IntoResponse {
    match jar.get("theme") {
        Some(c) => format!("theme = {}", c.value()),
        None => "no theme set".to_string(),
    }
}

#[tokio::main]
async fn main() {
    // The signing key. Generate once with Key::generate() and load it from an
    // env var / secret manager in production — never hard-code it.
    let key = Key::generate();

    let app = Router::new()
        .route("/set", get(set_cookie))
        .route("/read", get(read_cookie))
        // SignedCookieJar reads the key out of router state via FromRef.
        .with_state(key);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

---

## Real-World Example

A production session setup wants three things the demo lacked: a **persistent store** so sessions survive restarts and span replicas, a **guard extractor** so protected handlers stay terse, and **CSRF protection** for state-changing routes. Here is a compile-verified slice that ties them together.

First, the auth guard. By implementing `FromRequestParts`, you make `AuthUser` an extractor that *fails the request* with `401` when there is no logged-in user; protected handlers just list it as a parameter (the pattern from [Authentication](/16-web-apis/12-authentication/), backed here by the session):

```rust
// src/auth.rs — a session-backed guard extractor
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

#[derive(Serialize, Deserialize, Clone)]
pub struct SessionUser {
    pub id: u64,
    pub username: String,
}

pub const USER_KEY: &str = "user";

// A guard extractor: a handler that takes `AuthUser` only runs for logged-in users.
pub struct AuthUser(pub SessionUser);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Pull the Session out of the request first (the session layer inserted it).
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|(status, msg)| (status, msg).into_response())?;

        match session.get::<SessionUser>(USER_KEY).await {
            Ok(Some(user)) => Ok(AuthUser(user)),
            Ok(None) => Err((StatusCode::UNAUTHORIZED, "login required").into_response()),
            Err(_) => Err((StatusCode::INTERNAL_SERVER_ERROR, "session error").into_response()),
        }
    }
}
```

A protected handler now declares its requirement in its signature; no in-body auth check:

```rust
use axum::{response::IntoResponse, Json};
// use crate::auth::AuthUser;  // (same crate in a real project)

// This handler is only reachable with a valid session — the extractor enforces it.
async fn dashboard(AuthUser(user): AuthUser) -> impl IntoResponse {
    Json(user)
}
```

Next, a **synchronizer CSRF token**: mint a random token, store it in the server-side session, and require the client to echo it in a header on state-changing requests. An attacker's cross-site request cannot read the token out of the session, so it cannot forge the header:

```bash
cargo add rand@0.9
```

```rust
// src/csrf.rs — synchronizer-token CSRF protection backed by the session
use axum::{
    response::IntoResponse,
    http::{HeaderMap, StatusCode},
};
use rand::Rng;
use tower_sessions::Session;

const CSRF_KEY: &str = "csrf_token";

fn random_token() -> String {
    // 32 random bytes, hex-encoded — unpredictable per session.
    let bytes: [u8; 32] = rand::rng().random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// GET /form — mint a CSRF token, store it in the session, and hand it to the page.
async fn show_form(session: Session) -> impl IntoResponse {
    let token = random_token();
    session.insert(CSRF_KEY, token.clone()).await.unwrap();
    // The page embeds `token` in a hidden field or a meta tag for the JS client.
    format!("csrf token: {token}")
}

// POST /transfer — reject the request unless the submitted token matches the session.
async fn transfer(session: Session, headers: HeaderMap) -> impl IntoResponse {
    let expected: Option<String> = session.get(CSRF_KEY).await.unwrap();
    let provided = headers.get("x-csrf-token").and_then(|v| v.to_str().ok());

    match (expected.as_deref(), provided) {
        (Some(want), Some(got)) if want == got => {
            (StatusCode::OK, "transfer accepted").into_response()
        }
        _ => (StatusCode::FORBIDDEN, "CSRF check failed").into_response(),
    }
}
```

> **Note:** For a true constant-time comparison of the tokens (to avoid a timing side-channel), use the `subtle` crate's `ConstantTimeEq` instead of `==`. For most apps the random 256-bit token makes a timing attack impractical, but constant-time is the belt-and-suspenders choice.

Finally, the **persistent SQL store**. Reuse a `sqlx` pool (the same one your data layer uses — see [Connection Pooling](/17-database/)) so sessions live in the database. Note the matched versions: `tower-sessions = "0.14"` alongside `tower-sessions-sqlx-store = "0.15"`:

```bash
cargo add tower-sessions@0.14
cargo add tower-sessions-sqlx-store@0.15 --features sqlite
cargo add sqlx --features "runtime-tokio,sqlite"
```

```rust
// src/main.rs — Axum with a persistent SQLite-backed session store
use axum::{Router, routing::get, response::IntoResponse};
use sqlx::sqlite::SqlitePoolOptions;
use time::Duration;
use tower_sessions::{Session, SessionManagerLayer, Expiry};
use tower_sessions_sqlx_store::SqliteStore;

const COUNTER_KEY: &str = "counter";

async fn count(session: Session) -> impl IntoResponse {
    let n: usize = session.get(COUNTER_KEY).await.unwrap().unwrap_or(0) + 1;
    session.insert(COUNTER_KEY, n).await.unwrap();
    format!("visit #{n}")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A real connection pool — sessions now survive process restarts.
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;

    let store = SqliteStore::new(pool);
    store.migrate().await?; // creates the session table if it does not exist

    let session_layer = SessionManagerLayer::new(store)
        .with_secure(true)
        .with_expiry(Expiry::OnInactivity(Duration::days(1)));

    let app = Router::new()
        .route("/count", get(count))
        .layer(session_layer);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

Swap `SqliteStore` + `SqlitePoolOptions` for `PostgresStore` + `PgPoolOptions` (feature `postgres`) in production. The handler code does not change; only the store and pool type do, which is the whole point of the `SessionStore` trait abstraction.

---

## Further Reading

### Official Documentation

- [`tower-sessions` crate docs](https://docs.rs/tower-sessions/) — the `Session` API, `SessionManagerLayer`, cookie configuration, and `Expiry`
- [`tower-sessions-sqlx-store`](https://docs.rs/tower-sessions-sqlx-store/) and [`tower-sessions-redis-store`](https://docs.rs/tower-sessions-redis-store/) — persistent backends
- [`axum-extra` cookie extractors](https://docs.rs/axum-extra/latest/axum_extra/extract/cookie/index.html) — `CookieJar`, `SignedCookieJar`, `PrivateCookieJar`
- [MDN: `Set-Cookie` and `SameSite`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Set-Cookie) — the cookie attributes the layer emits
- [OWASP: Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html) and [CSRF Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)

### Related Topics

- [Authentication](/16-web-apis/12-authentication/): where auth checks live; turning the session lookup into a guard
- [JWT](/16-web-apis/13-jwt/) — the stateless, token-based alternative to server-side sessions
- [Extractors](/16-web-apis/04-extractors/): how `Session` and a custom `AuthUser` are injected
- [Middleware and Layers](/16-web-apis/05-middleware/) — the layer mechanism `SessionManagerLayer` plugs into
- [State Management](/16-web-apis/06-state-management/): the `provide-then-extract` pattern, and sharing the DB pool
- [Request and Response](/16-web-apis/07-request-response/) — `IntoResponse`, status codes, and setting headers
- [CORS](/16-web-apis/11-cors/): cross-origin rules that interact with cookie credentials
- [Serialization](/15-serialization/) — why session values need `Serialize`/`Deserialize`
- [Async/Await](/11-async/) — why session methods are awaited
- Next section: [Databases](/17-database/) — the SQL pool a persistent session store reuses

---

## Exercises

### Exercise 1: A per-session visit counter

**Difficulty:** Easy

**Objective:** Practice the read-modify-write session cycle with a non-string value.

**Instructions:**

1. Start an Axum app with a `MemoryStore`-backed `SessionManagerLayer`.
2. Add a `GET /count` route whose handler reads a `usize` under the key `"counter"` (defaulting to `0` when absent), increments it, writes it back, and returns `"You have visited this page N time(s) in this session."`.
3. Verify that repeated requests *with the same cookie* increment, while a fresh client starts again at 1.

<details>
<summary>Solution</summary>

```rust
use axum::{Router, routing::get, response::IntoResponse};
use tower_sessions::{Session, SessionManagerLayer, MemoryStore};

const COUNTER_KEY: &str = "counter";

// GET /count — increment a per-session view counter and report it.
async fn count(session: Session) -> impl IntoResponse {
    // get::<T> returns Result<Option<T>>; default to 0 the first time.
    let n: usize = session.get(COUNTER_KEY).await.unwrap().unwrap_or(0);
    let n = n + 1;
    session.insert(COUNTER_KEY, n).await.unwrap();
    format!("You have visited this page {n} time(s) in this session.")
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/count", get(count))
        .layer(SessionManagerLayer::new(MemoryStore::default()));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Dependencies: `cargo add axum@0.8 tokio@1 --features full` (tokio) and `cargo add tower-sessions@0.14`. Because `get::<usize>` returns `Result<Option<usize>>`, `.unwrap().unwrap_or(0)` turns "no error, no value yet" into `0`.

</details>

### Exercise 2: A login/logout flow with a guard extractor

**Difficulty:** Medium

**Objective:** Combine writing the session at login with a custom `FromRequestParts` extractor that protects a route.

**Instructions:**

1. Define `SessionUser { id, username }` deriving `Serialize, Deserialize, Clone`.
2. Add `POST /login` that (for hard-coded valid credentials) calls `cycle_id()` then stores the user, and `POST /logout` that calls `session.delete()`.
3. Implement an `AuthUser` extractor that pulls the `Session`, reads the user, and rejects with `401` when absent.
4. Add a `GET /profile` handler taking `AuthUser` and returning the user as JSON.

<details>
<summary>Solution</summary>

```rust
use axum::{
    Router,
    routing::{get, post},
    extract::{Json, FromRequestParts},
    response::{IntoResponse, Response, Redirect},
    http::{StatusCode, request::Parts},
};
use serde::{Deserialize, Serialize};
use tower_sessions::{Session, SessionManagerLayer, MemoryStore};

#[derive(Serialize, Deserialize, Clone)]
struct SessionUser {
    id: u64,
    username: String,
}

const USER_KEY: &str = "user";

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

struct AuthUser(SessionUser);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|(s, m)| (s, m).into_response())?;
        match session.get::<SessionUser>(USER_KEY).await {
            Ok(Some(user)) => Ok(AuthUser(user)),
            Ok(None) => Err((StatusCode::UNAUTHORIZED, "login required").into_response()),
            Err(_) => Err((StatusCode::INTERNAL_SERVER_ERROR, "session error").into_response()),
        }
    }
}

async fn login(session: Session, Json(form): Json<LoginForm>) -> impl IntoResponse {
    if form.username != "alice" || form.password != "correct horse" {
        return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response();
    }
    session.cycle_id().await.unwrap();
    session
        .insert(USER_KEY, SessionUser { id: 1, username: form.username })
        .await
        .unwrap();
    (StatusCode::OK, "logged in").into_response()
}

async fn logout(session: Session) -> impl IntoResponse {
    session.delete().await.unwrap();
    Redirect::to("/")
}

async fn profile(AuthUser(user): AuthUser) -> impl IntoResponse {
    Json(user)
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/profile", get(profile))
        .layer(SessionManagerLayer::new(MemoryStore::default()));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

The `AuthUser` extractor is the key idea: protected handlers declare `AuthUser` in their signature and the framework rejects anonymous requests *before* the handler body runs, no manual `if (!user) return 401` in every handler.

</details>

### Exercise 3: Session-backed synchronizer-token CSRF protection

**Difficulty:** Hard

**Objective:** Defend a state-changing endpoint with a session-stored CSRF token, and reason about why it works.

**Instructions:**

1. Add a `GET /form` route that generates a random token, stores it in the session under `"csrf_token"`, and returns it to the client.
2. Add a `POST /transfer` route that reads the stored token from the session and compares it to an `x-csrf-token` request header, returning `403` on mismatch and `200` on match.
3. In a comment, explain why an attacker's cross-site form POST cannot pass this check even though the browser *does* send the session cookie.

<details>
<summary>Solution</summary>

```rust
use axum::{
    Router,
    routing::{get, post},
    response::IntoResponse,
    http::{HeaderMap, StatusCode},
};
use rand::Rng;
use tower_sessions::{Session, SessionManagerLayer, MemoryStore};

const CSRF_KEY: &str = "csrf_token";

fn random_token() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn show_form(session: Session) -> impl IntoResponse {
    let token = random_token();
    session.insert(CSRF_KEY, token.clone()).await.unwrap();
    format!("csrf token: {token}")
}

async fn transfer(session: Session, headers: HeaderMap) -> impl IntoResponse {
    // Why this is safe: the token lives only in the server-side session and is
    // echoed by OUR page's JavaScript into the x-csrf-token header. An attacker's
    // cross-site page can make the browser send the cookie, but the Same-Origin
    // Policy stops it from READING our token to put it in the header — so the
    // forged request arrives with the cookie but no/ wrong token, and fails here.
    let expected: Option<String> = session.get(CSRF_KEY).await.unwrap();
    let provided = headers.get("x-csrf-token").and_then(|v| v.to_str().ok());

    match (expected.as_deref(), provided) {
        (Some(want), Some(got)) if want == got => {
            (StatusCode::OK, "transfer accepted").into_response()
        }
        _ => (StatusCode::FORBIDDEN, "CSRF check failed").into_response(),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/form", get(show_form))
        .route("/transfer", post(transfer))
        .layer(SessionManagerLayer::new(MemoryStore::default()));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Dependencies: `cargo add rand@0.9` plus the usual axum/tokio/tower-sessions. The defense works because the cookie is sent automatically by the browser but the *token* is not: reading it requires same-origin script access, which the attacker's page does not have. To harden it further, pair this with `SameSite=Lax` cookies and a constant-time comparison via the `subtle` crate.

</details>
