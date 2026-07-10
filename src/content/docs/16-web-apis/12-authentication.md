---
title: "Authentication Patterns"
description: "Express stuffs req.user onto a request; Axum makes auth a guard extractor in the handler signature, so a protected route can't compile without a user."
---

Authentication answers one question on every request: *who is making this call, and are they allowed to?* In Express you usually answer it with a middleware that stuffs a `req.user` onto the request. Axum gives you two cleaner mechanics: a **guard extractor** that lives in the handler's argument list, and **`from_fn` middleware** that injects the user into request extensions. This chapter shows both, and how to layer role and scope checks on top.

---

## Quick Overview

In Axum, authentication is just an **extractor** or a **layer**, not a special framework feature. The idiomatic pattern is an `AuthUser` extractor: a struct that implements `FromRequestParts`, reads the credential (a bearer token, cookie, or API key), looks up the principal, and either yields the authenticated user or short-circuits with a `401`/`403`. Because the guard lives in the handler **signature**, a route that needs a user *cannot compile without asking for one*. The "forgot to add the auth middleware" class of bug largely disappears. This file covers the patterns; the mechanics of decoding a JWT live in [JWT Authentication](/16-web-apis/13-jwt/), and cookie/server-side sessions live in [Sessions and Cookies](/16-web-apis/14-sessions/).

> **Scope note:** The credential here is a generic opaque token validated against a store. Swap in JWT verification or a session-cookie lookup without changing the *shape* of the guard.

---

## TypeScript/JavaScript Example

A typical Express setup: an auth middleware that verifies a bearer token, attaches `req.user`, and a role guard that builds on it.

```typescript
// auth.ts — Express authentication middleware
import express, { Request, Response, NextFunction } from "express";

interface User {
  id: number;
  name: string;
  role: "admin" | "member";
}

// Pretend this is a session store / token cache (Redis, a DB, etc.).
const sessions = new Map<string, number>([
  ["admin-token", 1],
  ["member-token", 2],
]);
const users = new Map<number, User>([
  [1, { id: 1, name: "Ada", role: "admin" }],
  [2, { id: 2, name: "Bob", role: "member" }],
]);

// Augment Express's Request type so `req.user` type-checks.
declare global {
  namespace Express {
    interface Request {
      user?: User;
    }
  }
}

// Middleware: authenticate, then attach the user to the request object.
function requireAuth(req: Request, res: Response, next: NextFunction) {
  const header = req.headers.authorization ?? "";
  const token = header.startsWith("Bearer ") ? header.slice(7) : null;
  const userId = token ? sessions.get(token) : undefined;
  const user = userId ? users.get(userId) : undefined;

  if (!user) {
    return res.status(401).json({ error: "invalid or missing token" });
  }
  req.user = user; // <-- shared mutable state; downstream handlers read this
  next();
}

// Role guard built ON TOP of requireAuth. Must run after it.
function requireAdmin(req: Request, res: Response, next: NextFunction) {
  if (req.user?.role !== "admin") {
    return res.status(403).json({ error: "admin role required" });
  }
  next();
}

const app = express();

app.get("/me", requireAuth, (req, res) => {
  res.json(req.user); // non-null only because requireAuth ran first
});

app.get("/admin/users", requireAuth, requireAdmin, (_req, res) => {
  res.json([...users.values()]);
});

app.listen(3000);
```

**The fragile part:** nothing in the type system stops you from writing `app.get("/me", (req, res) => res.json(req.user))` and *forgetting* `requireAuth`. `req.user` is typed `User | undefined`, so `res.json(req.user)` happily compiles and ships `undefined` at runtime. The guarantee that "this handler always has a user" lives only in your memory and your route registration order.

---

## Rust Equivalent

Axum turns that runtime convention into a **compile-time requirement**. We write an `AuthUser` extractor; any handler that wants the user just names it as a parameter. Set up a project:

```bash
cargo new auth-demo
cd auth-demo
cargo add axum@0.8
cargo add tokio@1 --features full
cargo add serde --features derive
cargo add serde_json
```

```rust
// src/main.rs — AuthUser as a guard extractor
use axum::{
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Serialize)]
struct User {
    id: u64,
    name: String,
    role: Role,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Role {
    Admin,
    Member,
}

// A trivial "session store": token -> user id. A real app hits a DB or Redis.
#[derive(Clone)]
struct AppState {
    sessions: Arc<HashMap<String, u64>>,
    users: Arc<HashMap<u64, User>>,
}

// The authenticated principal, available as a handler argument.
struct AuthUser(User);

// One reusable rejection type so every auth failure looks the same on the wire.
struct AuthError(StatusCode, &'static str);

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(AuthError(StatusCode::UNAUTHORIZED, "missing bearer token"))?;

        let user_id = state
            .sessions
            .get(token)
            .ok_or(AuthError(StatusCode::UNAUTHORIZED, "invalid or expired token"))?;

        let user = state
            .users
            .get(user_id)
            .cloned()
            .ok_or(AuthError(StatusCode::UNAUTHORIZED, "user no longer exists"))?;

        Ok(AuthUser(user))
    }
}

// A second guard built ON TOP of AuthUser: requires the admin role.
struct AdminUser(User);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        if user.role != Role::Admin {
            return Err(AuthError(StatusCode::FORBIDDEN, "admin role required"));
        }
        Ok(AdminUser(user))
    }
}

// Any logged-in user can read their own profile.
async fn me(AuthUser(user): AuthUser) -> Json<User> {
    Json(user)
}

// Only admins reach this handler — the type system enforces it.
async fn list_all(AdminUser(_admin): AdminUser, State(state): State<AppState>) -> Json<Vec<User>> {
    let mut all: Vec<User> = state.users.values().cloned().collect();
    all.sort_by_key(|u| u.id);
    Json(all)
}

fn app() -> Router {
    let mut users = HashMap::new();
    users.insert(1, User { id: 1, name: "Ada".into(), role: Role::Admin });
    users.insert(2, User { id: 2, name: "Bob".into(), role: Role::Member });

    let mut sessions = HashMap::new();
    sessions.insert("admin-token".to_string(), 1);
    sessions.insert("member-token".to_string(), 2);

    let state = AppState { sessions: Arc::new(sessions), users: Arc::new(users) };

    Router::new()
        .route("/me", get(me))
        .route("/admin/users", get(list_all))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let app = app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Running it and hitting the routes with `curl` produces this **real** output:

```text
$ curl -s -i http://127.0.0.1:3000/me | head -1
HTTP/1.1 401 Unauthorized
$ curl -s http://127.0.0.1:3000/me
{"error":"missing bearer token"}

$ curl -s -H "Authorization: Bearer member-token" http://127.0.0.1:3000/me
{"id":2,"name":"Bob","role":"member"}

$ curl -s -i -H "Authorization: Bearer member-token" http://127.0.0.1:3000/admin/users | head -1
HTTP/1.1 403 Forbidden
$ curl -s -H "Authorization: Bearer member-token" http://127.0.0.1:3000/admin/users
{"error":"admin role required"}

$ curl -s -H "Authorization: Bearer admin-token" http://127.0.0.1:3000/admin/users
[{"id":1,"name":"Ada","role":"admin"},{"id":2,"name":"Bob","role":"member"}]
```

---

## Detailed Explanation

### The guard lives in the signature

`async fn me(AuthUser(user): AuthUser)` is the whole point. Before `me` runs, Axum calls `AuthUser::from_request_parts`. If that returns `Err`, the handler body **never executes** and the rejection becomes the response. There is no `req.user` that *might* be undefined. By the time you are inside `me`, `user` is a concrete, non-optional `User`. Compare this to the Express version, where `req.user` stays `User | undefined` forever.

This is the `FromRequestParts` trait, the same mechanism behind `Path`, `Query`, and `State` (see [Extractors](/16-web-apis/04-extractors/) for the full machinery). The key facts:

- **`from_request_parts` only sees request *metadata*** — method, URI, headers, extensions — not the body. That is exactly what auth needs, and it means `AuthUser` composes freely with a body extractor like `Json` in the same handler.
- **The `State` is handed in as `&AppState`.** Because we implemented `FromRequestParts<AppState>` (a concrete state type, not a generic `S`), we can reach into `state.sessions` directly. That is the difference between this guard and the generic `RequestId` example in [Extractors](/16-web-apis/04-extractors/): a generic guard needs no state, an auth guard usually does.
- **The `?` operator threads rejections out.** Every `.ok_or(...)?` converts a `None` into an early `Err(AuthError)`. Rust's [error handling](/08-error-handling/) machinery does the short-circuiting that `return res.status(401)...` does manually in Express.

### One rejection type, one response shape

`AuthError(StatusCode, &'static str)` implements `IntoResponse`, so the framework knows how to turn a rejected auth into an HTTP response. We build a JSON body with `serde_json::json!`. Every auth failure (missing token, bad token, deleted user) funnels through this single type, so the wire format is consistent. (For a richer application-wide error type using `thiserror`, see [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/).)

### Layering guards by composition

`AdminUser` does not re-implement token parsing. It calls `AuthUser::from_request_parts(parts, state).await?` and then adds a role check. This mirrors `requireAuth` + `requireAdmin` in Express, but the dependency is explicit: `AdminUser` literally *calls* `AuthUser`, so you cannot accidentally run the role check without the authentication that precedes it.

### Why `Arc` and `Clone`

`AppState` derives `Clone` and wraps its maps in `Arc`. Axum clones the state once per request to hand to extractors; `Arc` makes that clone a cheap pointer-bump rather than a deep copy of every user. This is the standard [shared-state](/16-web-apis/06-state-management/) pattern. (Cloning shared state is covered in depth there; the relevant ownership rules are in [section 05](/05-ownership/).)

---

## Key Differences

| Concern | Express.js | Axum |
| --- | --- | --- |
| Where auth runs | `app.use` / per-route middleware | extractor in the handler signature, or a `from_fn` layer |
| How the user is exposed | mutate `req.user` (typed `User \| undefined`) | a handler **parameter** of type `AuthUser` (always present) |
| Forgetting the guard | compiles; `req.user` is `undefined` at runtime | the handler won't compile without naming the extractor |
| Short-circuiting | `return res.status(401)...` and skip `next()` | extractor returns `Err`; handler body never runs |
| Role/scope layering | a second middleware that reads `req.user` | a second extractor that calls the first, or a method check |
| Rejection format | whatever each middleware writes | one `IntoResponse` type, uniform across the app |

The deepest difference: in Express, the link between "this route is protected" and "this handler assumes a user" is a *convention* enforced by route-registration order. In Axum it is a *type*. A handler that takes `AuthUser` is, by construction, only reachable with a valid user.

> **Note:** This compile-time guarantee covers *requesting* the user, not *applying* the layer. If you instead use the middleware-injection pattern below (`Extension<CurrentUser>`), you reintroduce a runtime failure mode; see the pitfall on missing extensions.

---

## Middleware-based authentication (the `req.user` analog)

Sometimes you want the Express feeling exactly: one middleware authenticates, stashes the user, and many downstream handlers read it. Axum supports this with `middleware::from_fn_with_state` plus **request extensions**: a typed bag attached to the request, which is the closest analog to mutating `req`.

```rust
// src/main.rs — middleware-based auth that injects the user via request extensions
use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Serialize)]
struct CurrentUser {
    id: u64,
    name: String,
}

#[derive(Clone)]
struct AppState {
    sessions: Arc<HashMap<String, CurrentUser>>,
}

// The middleware: authenticate, then stash the user in the request's extensions
// so downstream handlers (and other middleware) can read it.
async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let user = state.sessions.get(token).ok_or(StatusCode::UNAUTHORIZED)?.clone();

    // Make the authenticated user available to handlers via Extension<CurrentUser>.
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

// Handlers read the injected user with the Extension extractor.
async fn me(Extension(user): Extension<CurrentUser>) -> Json<CurrentUser> {
    Json(user)
}

async fn dashboard(Extension(user): Extension<CurrentUser>) -> impl IntoResponse {
    format!("Welcome back, {}!", user.name)
}

fn app() -> Router {
    let mut sessions = HashMap::new();
    sessions.insert("member-token".to_string(), CurrentUser { id: 2, name: "Bob".into() });
    let state = AppState { sessions: Arc::new(sessions) };

    // Protected routes share one auth layer applied via route_layer.
    let protected = Router::new()
        .route("/me", get(me))
        .route("/dashboard", get(dashboard))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .route("/", get(|| async { "public" }))
        .merge(protected)
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let app = app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Real output:

```text
$ curl -s http://127.0.0.1:3001/
public
$ curl -s -i http://127.0.0.1:3001/me | head -1
HTTP/1.1 401 Unauthorized
$ curl -s -H "Authorization: Bearer member-token" http://127.0.0.1:3001/me
{"id":2,"name":"Bob"}
$ curl -s -H "Authorization: Bearer member-token" http://127.0.0.1:3001/dashboard
Welcome back, Bob!
```

**How it maps to Express:**

- `auth_middleware` is the `requireAuth` analog. `next.run(req).await` is `next()`.
- `req.extensions_mut().insert(user)` is `req.user = user`, except extensions are keyed by **type**, not by string. Each type can hold one value.
- `Extension<CurrentUser>` is `req.user` — except the read can *fail at runtime* if nothing inserted it (see the pitfall below).
- `.route_layer(...)` applies the middleware to those routes only. Unlike `.layer()`, it does not run for unmatched paths, so a 404 stays a 404. Layer-vs-route_layer ordering is covered in [Middleware and Layers](/16-web-apis/05-middleware/).

### Extractor guard vs. middleware injection — which to use?

| | Extractor (`AuthUser`) | Middleware + `Extension<CurrentUser>` |
| --- | --- | --- |
| "Forgot the guard" failure | **compile error** (must name the extractor) | runtime `500` (extension missing) |
| Per-handler opt-in | yes — only handlers that name it pay the cost | no — applies to every route under the layer |
| Run for *every* route in a group | verbose (repeat in each signature) | one `.route_layer(...)` |
| Modify the request before handlers | no | yes (rate-limit headers, request IDs, etc.) |
| Best for | authorization that varies per handler; pulling typed user data | a blanket "this whole subtree requires login" gate |

A common production setup uses **both**: a `from_fn` layer that authenticates a route group, plus a thin `AuthUser` extractor that reads the injected `CurrentUser` out of extensions and returns a clean `401` if it is absent, getting the compile-time guarantee back even under a blanket layer.

---

## Common Pitfalls

### Reading an `Extension` that was never inserted

The middleware pattern's sharp edge: if a handler asks for `Extension<CurrentUser>` but its route is **not** behind the auth layer, it compiles fine and fails at runtime with a `500`.

```rust
// Compiles, but every request 500s: nothing inserts CurrentUser.
use axum::{routing::get, Extension, Json, Router};
use serde::Serialize;

#[derive(Clone, Serialize)]
struct CurrentUser { id: u64, name: String }

async fn me(Extension(user): Extension<CurrentUser>) -> Json<CurrentUser> {
    Json(user)
}

fn app() -> Router {
    // No auth middleware here — nothing inserts CurrentUser.
    Router::new().route("/me", get(me))
}
```

Hitting `/me` returns a real `500` with this body:

```text
HTTP/1.1 500 Internal Server Error
Missing request extension: Extension of type `probe::CurrentUser` was not found. Perhaps you forgot to add it? See `axum::Extension`.
```

This is the Rust echo of Express's `req.user` being `undefined`, except Express would silently serialize `undefined`, while Axum at least shouts. The `AuthUser` *extractor* pattern avoids this entirely, since it does the lookup itself rather than trusting a prior layer.

### Reaching into a generic state parameter

A guard that needs `AppState` must implement `FromRequestParts<AppState>`, a concrete type. Writing it generic over `S` and then touching a field does not compile:

```rust
// does not compile (error[E0609]: no field `api_token` on type `&S`)
use axum::{extract::FromRequestParts, http::{request::Parts, StatusCode}};

struct AppState { api_token: String }
struct AuthUser;

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;
    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if state.api_token.is_empty() {   // <-- S is opaque; it has no fields
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(AuthUser)
    }
}

fn main() {}
```

The exact compiler message:

```text
error[E0609]: no field `api_token` on type `&S`
  --> src/main.rs:13:18
   |
 7 | impl<S> FromRequestParts<S> for AuthUser
   |      - type parameter 'S' declared here
...
13 |         if state.api_token.is_empty() {   // <-- S is opaque; it has no fields
   |                  ^^^^^^^^^ unknown field
```

**Fix:** either implement `FromRequestParts<AppState>` for the concrete state (as in the main example), or keep it generic but require `AppState: FromRef<S>` and call `AppState::from_ref(state)` to pull your sub-state out: the `FromRef` pattern from [Shared Application State in Axum](/16-web-apis/06-state-management/).

### Forgetting that `from_request_parts` is `async` and `Next` is last

In axum 0.8 the trait uses native `async fn` (stable since Rust 1.75), so you write `async fn from_request_parts(...)` with **no `#[async_trait]`**. For `from_fn` middleware, `next: Next` must be the **last** parameter, after any extractors like `State`. Getting either wrong yields trait-bound errors that name `Handler` or `FromRequestParts` and are easy to misread. The fix is almost always parameter order or a missing `async`.

> **Warning:** `async-trait` is **not** needed for extractors or middleware. It is only for `dyn Trait` dynamic dispatch, which authentication does not require.

### Timing-unsafe token comparison

`token != state.api_token` for a *secret* compares byte-by-byte and can leak length/prefix information through timing. For static API keys or session tokens, prefer a constant-time comparison (e.g. the `subtle` crate's `ConstantTimeEq`, or comparing fixed-size hashes). For passwords, never compare plaintext — verify against a hash (see the Real-World Example). This matters more for secrets you compare on every request; opaque random session-IDs looked up in a map are less exposed because the map lookup, not a string compare, decides the match.

### Returning `403` when you mean `401` (and vice versa)

`401 Unauthorized` means *"I do not know who you are"* (no/invalid credentials). `403 Forbidden` means *"I know who you are, and you may not do this"* (authenticated but lacking the role/scope). The example returns `401` from `AuthUser` and `403` from the role check; keep that distinction, clients and proxies rely on it.

---

## Best Practices

- **Prefer the extractor guard for authorization that varies per route.** A guard in the signature cannot be forgotten the way a layer can be omitted.
- **Compose guards, don't copy them.** `AdminUser` calls `AuthUser`; a `BillingAdmin` would call `AdminUser`. Each layer adds exactly one check.
- **Use one `IntoResponse` error type** for all auth failures so the wire format is uniform; reach for an app-wide error enum (`thiserror`) once you have more than a couple of failure modes — see [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/).
- **Keep `AuthUser` cheap to produce.** If the lookup hits a database, that cost is paid once per request per guarded handler. With the composition pattern, `AdminUser` reuses `AuthUser`'s single lookup rather than querying twice.
- **Distinguish `401` from `403`** as above.
- **For "whole subtree requires login," use `.route_layer(from_fn_with_state(...))`** so unmatched paths still 404 instead of 401.
- **Never log tokens or `Authorization` headers.** If you use `TraceLayer`, configure it not to record sensitive headers.
- **Hash passwords with Argon2 (or bcrypt/scrypt); never store or compare plaintext.** The login flow below shows the Argon2 pattern.

---

## Real-World Example

A self-contained mini-API: a `POST /login` endpoint that verifies a password with **Argon2** and issues an opaque session token, an `AuthUser` guard, and an `AdminUser` role guard on top. This is production-shaped (constant-time password verification, identical error for unknown-email vs. wrong-password, unguessable UUID tokens), with the store kept in memory for brevity. Swap `RwLock<HashMap>` for a database pool from [section 17](/17-database/) in a real service.

```bash
cargo add axum@0.8
cargo add tokio@1 --features full
cargo add serde --features derive
cargo add serde_json
cargo add argon2 --features std
cargo add uuid --features v4
```

```rust
// src/main.rs — login + AuthUser guard + AdminUser role guard
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

#[derive(Clone, Serialize)]
struct User {
    id: u64,
    email: String,
    role: Role,
    #[serde(skip)] // never serialize the hash out to clients
    password_hash: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Role {
    Admin,
    Member,
}

#[derive(Clone)]
struct AppState {
    users: Arc<RwLock<HashMap<String, User>>>, // email -> user
    sessions: Arc<RwLock<HashMap<String, u64>>>, // token -> user id
}

// ---- One reusable auth error type -------------------------------------------

struct AuthError {
    status: StatusCode,
    message: &'static str,
}

impl AuthError {
    fn new(status: StatusCode, message: &'static str) -> Self {
        Self { status, message }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (self.status, Json(serde_json::json!({ "error": self.message }))).into_response()
    }
}

// ---- The AuthUser guard extractor -------------------------------------------

struct AuthUser(User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or_else(|| AuthError::new(StatusCode::UNAUTHORIZED, "missing bearer token"))?;

        let user_id = {
            let sessions = state.sessions.read().unwrap();
            *sessions
                .get(token)
                .ok_or_else(|| AuthError::new(StatusCode::UNAUTHORIZED, "invalid session"))?
        };

        let user = {
            let users = state.users.read().unwrap();
            users
                .values()
                .find(|u| u.id == user_id)
                .cloned()
                .ok_or_else(|| AuthError::new(StatusCode::UNAUTHORIZED, "user not found"))?
        };

        Ok(AuthUser(user))
    }
}

// AdminUser reuses AuthUser, then checks the role.
struct AdminUser(User);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        if user.role != Role::Admin {
            return Err(AuthError::new(StatusCode::FORBIDDEN, "admin role required"));
        }
        Ok(AdminUser(user))
    }
}

// ---- Handlers ----------------------------------------------------------------

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AuthError> {
    // 1. Look up the user by email.
    let user = {
        let users = state.users.read().unwrap();
        users.get(&body.email).cloned()
    }
    // Same error whether the email is unknown or the password is wrong:
    // never leak which accounts exist.
    .ok_or_else(|| AuthError::new(StatusCode::UNAUTHORIZED, "invalid credentials"))?;

    // 2. Verify the password against the stored Argon2 hash (constant-time).
    let parsed = PasswordHash::new(&user.password_hash)
        .map_err(|_| AuthError::new(StatusCode::INTERNAL_SERVER_ERROR, "corrupt hash"))?;
    Argon2::default()
        .verify_password(body.password.as_bytes(), &parsed)
        .map_err(|_| AuthError::new(StatusCode::UNAUTHORIZED, "invalid credentials"))?;

    // 3. Issue an opaque, unguessable session token.
    let token = Uuid::new_v4().to_string();
    state.sessions.write().unwrap().insert(token.clone(), user.id);

    Ok(Json(LoginResponse { token }))
}

async fn me(AuthUser(user): AuthUser) -> Json<User> {
    Json(user)
}

async fn list_users(AdminUser(_admin): AdminUser, State(state): State<AppState>) -> Json<Vec<User>> {
    let mut all: Vec<User> = state.users.read().unwrap().values().cloned().collect();
    all.sort_by_key(|u| u.id);
    Json(all)
}

// Helper to hash a password when seeding the store.
fn hash_password(plain: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn app() -> Router {
    let mut users = HashMap::new();
    users.insert(
        "ada@example.com".to_string(),
        User { id: 1, email: "ada@example.com".into(), role: Role::Admin, password_hash: hash_password("hunter2") },
    );
    users.insert(
        "bob@example.com".to_string(),
        User { id: 2, email: "bob@example.com".into(), role: Role::Member, password_hash: hash_password("s3cret") },
    );

    let state = AppState {
        users: Arc::new(RwLock::new(users)),
        sessions: Arc::new(RwLock::new(HashMap::new())),
    };

    Router::new()
        .route("/login", post(login))
        .route("/me", get(me))
        .route("/admin/users", get(list_users))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let app = app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3003").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

End-to-end with `curl`, every response below is **real**:

```text
# Wrong password -> generic 401 (no account enumeration)
$ curl -s -i -X POST http://127.0.0.1:3003/login \
       -H 'content-type: application/json' \
       -d '{"email":"ada@example.com","password":"nope"}' | head -1
HTTP/1.1 401 Unauthorized
$ curl -s -X POST http://127.0.0.1:3003/login \
       -H 'content-type: application/json' \
       -d '{"email":"ada@example.com","password":"nope"}'
{"error":"invalid credentials"}

# Correct login -> opaque session token
$ curl -s -X POST http://127.0.0.1:3003/login \
       -H 'content-type: application/json' \
       -d '{"email":"ada@example.com","password":"hunter2"}'
{"token":"bc89e48c-02ac-4a46-999c-e900f5e599cd"}

# Use the token (note: password_hash is omitted thanks to #[serde(skip)])
$ curl -s -H "Authorization: Bearer bc89e48c-02ac-4a46-999c-e900f5e599cd" \
       http://127.0.0.1:3003/me
{"id":1,"email":"ada@example.com","role":"admin"}

# Ada is an admin -> allowed
$ curl -s -H "Authorization: Bearer bc89e48c-02ac-4a46-999c-e900f5e599cd" \
       http://127.0.0.1:3003/admin/users
[{"id":1,"email":"ada@example.com","role":"admin"},{"id":2,"email":"bob@example.com","role":"member"}]

# Bob (member) is forbidden from the admin route
$ curl -s -i -H "Authorization: Bearer <bob-token>" \
       http://127.0.0.1:3003/admin/users | head -1
HTTP/1.1 403 Forbidden
$ curl -s -H "Authorization: Bearer <bob-token>" http://127.0.0.1:3003/admin/users
{"error":"admin role required"}
```

> **Tip:** This issues an **opaque** server-side token (a session ID). The alternative is a self-contained **JWT** that carries the claims in the token itself: no per-request store lookup, at the cost of harder revocation. See [JWT Authentication](/16-web-apis/13-jwt/). For storing the token in a cookie rather than an `Authorization` header (and the CSRF considerations that follow), see [Sessions and Cookies](/16-web-apis/14-sessions/).

---

## Further Reading

- [`FromRequestParts`](https://docs.rs/axum/latest/axum/extract/trait.FromRequestParts.html) — the trait every guard extractor implements.
- [`axum::middleware::from_fn_with_state`](https://docs.rs/axum/latest/axum/middleware/fn.from_fn_with_state.html) — middleware that needs access to state.
- [`axum::Extension`](https://docs.rs/axum/latest/axum/struct.Extension.html) — the typed request-extensions extractor.
- [`argon2` crate](https://docs.rs/argon2/latest/argon2/) — password hashing used in the login example.
- Guide cross-links:
  - [Extractors](/16-web-apis/04-extractors/): how `FromRequestParts`/`FromRequest` work and extractor ordering.
  - [Middleware and Layers](/16-web-apis/05-middleware/): `from_fn`, layers, and `route_layer` vs `layer`.
  - [Shared Application State in Axum](/16-web-apis/06-state-management/): `State<T>`, `Arc`, and the `FromRef` sub-state pattern.
  - [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/): building an app-wide `AppError` that implements `IntoResponse`.
  - [JWT Authentication](/16-web-apis/13-jwt/): verifying JWTs inside an extractor.
  - [Sessions and Cookies](/16-web-apis/14-sessions/): cookie-based sessions and CSRF.
  - [CORS with Axum and tower-http](/16-web-apis/11-cors/): CORS for browser clients that send credentials.
  - [Section 08: Error Handling](/08-error-handling/) and [Section 09: Generics & Traits](/09-generics-traits/) for the trait/`Result` foundations.

---

## Exercises

### Exercise 1: An optional-auth extractor

**Difficulty:** Easy

**Objective:** Build a route that personalizes its greeting for logged-in users but still works for anonymous visitors: the auth equivalent of an optional `req.user`.

**Instructions:**

1. Implement an `AuthUser(String)` extractor over an `AppState` whose `sessions: Arc<HashMap<String, String>>` maps token to username.
2. Also implement `OptionalFromRequestParts<AppState>` for `AuthUser` so that `Option<AuthUser>` works as a handler argument and *never* rejects.
3. Write `GET /greet` taking `Option<AuthUser>`: greet by name if present, otherwise `"Hello, guest!"`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::{FromRequestParts, OptionalFromRequestParts},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    sessions: Arc<HashMap<String, String>>, // token -> username
}

struct AuthUser(String);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = StatusCode;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let name = state.sessions.get(token).cloned().ok_or(StatusCode::UNAUTHORIZED)?;
        Ok(AuthUser(name))
    }
}

// Make `Option<AuthUser>` an extractor: Some on a valid token, None otherwise,
// and never an error.
impl OptionalFromRequestParts<AppState> for AuthUser {
    type Rejection = Infallible;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(<AuthUser as FromRequestParts<AppState>>::from_request_parts(parts, state).await.ok())
    }
}

async fn greet(user: Option<AuthUser>) -> impl IntoResponse {
    match user {
        Some(AuthUser(name)) => format!("Hello, {name}!"),
        None => "Hello, guest!".to_string(),
    }
}

fn app() -> Router {
    let mut sessions = HashMap::new();
    sessions.insert("tok".to_string(), "Ada".to_string());
    let state = AppState { sessions: Arc::new(sessions) };
    Router::new().route("/greet", get(greet)).with_state(state)
}

#[tokio::main]
async fn main() {
    let app = app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3004").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Verified behavior:

```text
$ curl -s http://127.0.0.1:3004/greet
Hello, guest!
$ curl -s -H "Authorization: Bearer tok" http://127.0.0.1:3004/greet
Hello, Ada!
$ curl -s -H "Authorization: Bearer wrong" http://127.0.0.1:3004/greet
Hello, guest!
```

In axum 0.8, `Option<T>` and `Result<T, T::Rejection>` are extractors only when `T` implements `OptionalFromRequestParts`/`FromRequestParts`. Implementing it explicitly (delegating to the strict impl with `.ok()`) makes the optional behavior intentional rather than accidental.

</details>

### Exercise 2: Fix the missing-guard bug

**Difficulty:** Medium

**Objective:** Diagnose and fix a handler that 500s because it reads an extension nobody inserted.

**Instructions:** The router below has `/me` reading `Extension<CurrentUser>`, but the `auth` middleware is only applied to `/dashboard`. Every request to `/me` returns `500 Missing request extension`. Fix it two ways: (a) extend the layer to cover `/me`, and (b) explain why converting `/me` to use the `AuthUser` *extractor* would have made the bug a compile error instead.

```rust
// Starting point (buggy): /me 500s.
let protected = Router::new()
    .route("/dashboard", get(dashboard))
    .route_layer(middleware::from_fn_with_state(state.clone(), auth));
let app = Router::new()
    .route("/me", get(me))            // <-- not behind `auth`
    .merge(protected)
    .with_state(state);
```

<details>
<summary>Solution</summary>

**Fix (a): put `/me` inside the protected group so the layer covers it.**

```rust
use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::get,
    Extension, Json, Router,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Serialize)]
struct CurrentUser { id: u64, name: String }

#[derive(Clone)]
struct AppState { sessions: Arc<HashMap<String, CurrentUser>> }

async fn auth(State(state): State<AppState>, mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let token = req.headers().get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let user = state.sessions.get(token).ok_or(StatusCode::UNAUTHORIZED)?.clone();
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

async fn me(Extension(user): Extension<CurrentUser>) -> Json<CurrentUser> { Json(user) }
async fn dashboard(Extension(user): Extension<CurrentUser>) -> String {
    format!("Welcome back, {}!", user.name)
}

fn app() -> Router {
    let mut sessions = HashMap::new();
    sessions.insert("tok".to_string(), CurrentUser { id: 1, name: "Ada".into() });
    let state = AppState { sessions: Arc::new(sessions) };

    // Both routes now sit under the same auth layer.
    Router::new()
        .route("/me", get(me))
        .route("/dashboard", get(dashboard))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let app = app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

**Fix (b):** if `me` instead took an `AuthUser` extractor that *does its own lookup*, then a route serving `me` without the necessary state/header would either be rejected with a clean `401` at request time, or — if you forgot to provide the state the extractor needs — fail to compile, because the handler would not satisfy `Handler` for that router's state type. The extension pattern defers the "is the user here?" question to runtime; the extractor pattern answers it at the type level. That is the core reason to prefer the extractor guard when you can.

</details>

### Exercise 3: A scope/permission guard

**Difficulty:** Hard

**Objective:** Go beyond a single `role` field to fine-grained **scopes** (like OAuth `posts:read`, `posts:write`), and enforce a specific scope per handler.

**Instructions:**

1. Give `AuthUser` a `scopes: HashSet<String>` and a `require_scope(&self, scope) -> Result<(), Response>` helper that returns a `403` response when the scope is absent.
2. Wire `GET /posts` to require `posts:read` and `POST /posts` to require `posts:write` on the same path.
3. Verify a read-only user can `GET` but not `POST`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    sessions: Arc<HashMap<String, AuthUser>>,
}

#[derive(Clone)]
struct AuthUser {
    name: String,
    scopes: HashSet<String>,
}

impl AuthUser {
    // Guard helper: returns a 403 response unless the user holds the scope.
    fn require_scope(&self, scope: &str) -> Result<(), Response> {
        if self.scopes.contains(scope) {
            Ok(())
        } else {
            Err((StatusCode::FORBIDDEN, format!("missing scope: {scope}")).into_response())
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = (StatusCode, String);
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or((StatusCode::UNAUTHORIZED, "missing token".to_string()))?;
        state
            .sessions
            .get(token)
            .cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "invalid token".to_string()))
    }
}

async fn read_posts(user: AuthUser) -> Result<Response, Response> {
    user.require_scope("posts:read")?;
    Ok(format!("{} can read posts", user.name).into_response())
}

async fn write_posts(user: AuthUser) -> Result<Response, Response> {
    user.require_scope("posts:write")?;
    Ok(format!("{} can write posts", user.name).into_response())
}

fn app() -> Router {
    let mut sessions = HashMap::new();
    sessions.insert(
        "reader".to_string(),
        AuthUser { name: "Bob".into(), scopes: HashSet::from(["posts:read".to_string()]) },
    );
    sessions.insert(
        "editor".to_string(),
        AuthUser {
            name: "Ada".into(),
            scopes: HashSet::from(["posts:read".to_string(), "posts:write".to_string()]),
        },
    );
    let state = AppState { sessions: Arc::new(sessions) };
    Router::new()
        .route("/posts", get(read_posts).post(write_posts))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let app = app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3005").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Verified behavior:

```text
$ curl -s -H "Authorization: Bearer reader" http://127.0.0.1:3005/posts
Bob can read posts
$ curl -s -i -X POST -H "Authorization: Bearer reader" http://127.0.0.1:3005/posts | head -1
HTTP/1.1 403 Forbidden
$ curl -s -X POST -H "Authorization: Bearer reader" http://127.0.0.1:3005/posts
missing scope: posts:write
$ curl -s -X POST -H "Authorization: Bearer editor" http://127.0.0.1:3005/posts
Ada can write posts
```

Using `Result<Response, Response>` as the handler return type lets `require_scope`'s `403` short-circuit with `?` — both arms are already responses, so no extra conversion is needed. For a guard that enforces the scope *before* the handler body runs (so the scope cannot be forgotten), wrap this in a dedicated extractor that reads the scope from a typed marker; the method approach shown here is the simplest stable design and keeps the required scope visible right at the top of each handler.

</details>
