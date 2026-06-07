---
title: "JWT Authentication"
description: "Issue and verify HMAC-signed JWTs in Rust with the jsonwebtoken crate. Where Express casts jwt.verify to any, Axum decodes typed Claims into an auth extractor."
---

In a TypeScript backend you reach for `jsonwebtoken` (`jwt.sign` / `jwt.verify`) and an Express middleware that stuffs the decoded payload onto `req.user`. Rust's story is almost identical in shape — there is a crate literally called `jsonwebtoken` — but the verification step becomes a typed **extractor**, so a handler that compiles has already been handed a validated set of claims.

---

## Quick Overview

A **JSON Web Token (JWT)** is a signed, URL-safe string of the form `header.payload.signature`. The server signs it with a secret (or private key), hands it to the client at login, and the client sends it back in the `Authorization: Bearer <token>` header on every request. On each request the server re-verifies the signature and the expiry, then trusts the **claims** inside. This page covers issuing and verifying HMAC-signed tokens with the [`jsonwebtoken`](https://docs.rs/jsonwebtoken) crate, modelling claims as a `struct`, enforcing expiry, and verifying tokens inside an Axum extractor so the rest of your code only ever sees a valid `Claims`.

The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. This page targets **axum 0.8** and **`jsonwebtoken` 10**.

> **Note:** `jsonwebtoken` 10 split its cryptography backend into Cargo features. You must enable **exactly one** of `rust_crypto` (pure Rust) or `aws_lc_rs`; the default features do not include a signer, and the program will panic at the first `encode`/`decode` if you forget. This page uses `rust_crypto`, which needs no system C toolchain.

---

## TypeScript/JavaScript Example

A typical Express setup: a `/login` route that signs a token, and an auth middleware that verifies it and populates `req.user`.

```typescript
// auth.ts — Express 5 + jsonwebtoken
import express, { Request, Response, NextFunction } from "express";
import jwt from "jsonwebtoken";

const SECRET = process.env.JWT_SECRET ?? "dev-only-secret";

interface Claims {
  sub: string; // user id
  role: "user" | "admin";
}

const app = express();
app.use(express.json());

// Issue a token at login.
app.post("/login", (req: Request, res: Response) => {
  const { username, password } = req.body as { username: string; password: string };
  // A real app verifies a password hash here.
  if (username !== "alice" || password !== "hunter2") {
    return res.status(401).json({ error: "invalid credentials" });
  }
  const token = jwt.sign({ sub: username, role: "user" } satisfies Claims, SECRET, {
    expiresIn: "1h",
  });
  res.json({ access_token: token, token_type: "Bearer" });
});

// Verify a token on protected routes.
function requireAuth(req: Request, res: Response, next: NextFunction) {
  const header = req.headers.authorization;
  const token = header?.startsWith("Bearer ") ? header.slice(7) : undefined;
  if (!token) return res.status(401).json({ error: "missing bearer token" });
  try {
    // `jwt.verify` checks the signature AND the `exp` claim, throwing on failure.
    (req as Request & { user: Claims }).user = jwt.verify(token, SECRET) as Claims;
    next();
  } catch {
    res.status(401).json({ error: "invalid or expired token" });
  }
}

app.get("/me", requireAuth, (req: Request, res: Response) => {
  const user = (req as Request & { user: Claims }).user;
  res.json({ user: user.sub, role: user.role });
});

app.listen(3000, () => console.log("listening on http://127.0.0.1:3000"));
```

Two things to notice, because Rust will tighten both:

1. `jwt.verify(token, SECRET)` returns `any` (you cast it to `Claims`). Nothing guarantees the token actually contained those fields; a typo in a claim name fails silently at runtime.
2. The decoded payload travels on a manually-attached `req.user` property that TypeScript does not know about, hence the `req as Request & { user: Claims }` dance.

---

## Rust Equivalent

First add the dependencies in a project created with `cargo new`:

```bash
cargo add jsonwebtoken --features rust_crypto
cargo add serde --features derive
cargo add serde_json
```

The signing and verifying logic on its own — no web framework yet — looks like this:

```rust
use jsonwebtoken::{
    decode, encode, get_current_timestamp, Algorithm, DecodingKey, EncodingKey, Header,
    Validation,
};
use serde::{Deserialize, Serialize};

// The claims ARE a struct. serde turns it into the JSON payload and back.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,  // subject — usually the user id
    role: String, // a custom claim
    exp: u64,     // expiry, seconds since the Unix epoch (a registered claim)
    iat: u64,     // issued-at
}

fn make_token(secret: &[u8]) -> String {
    let now = get_current_timestamp();
    let claims = Claims {
        sub: "user_42".to_string(),
        role: "admin".to_string(),
        iat: now,
        exp: now + 3600, // valid for one hour
    };
    // Header::default() == HS256. EncodingKey::from_secret takes raw bytes.
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret)).unwrap()
}

fn verify(token: &str, secret: &[u8]) -> jsonwebtoken::errors::Result<Claims> {
    // Validation::new(HS256) checks the signature, the algorithm, AND `exp`.
    let validation = Validation::new(Algorithm::HS256);
    let data = decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)?;
    Ok(data.claims)
}

fn main() {
    let secret = b"super-secret-key";
    let token = make_token(secret);
    println!("token starts with: {}", &token[..16]);

    let claims = verify(&token, secret).unwrap();
    println!("decoded: sub={} role={}", claims.sub, claims.role);
}
```

Running it prints real output:

```text
token starts with: eyJ0eXAiOiJKV1Qi
decoded: sub=user_42 role=admin
```

The key difference from the TypeScript version: `decode::<Claims>(...)` is generic over your claims type. There is no `any`, no cast. If the JSON payload is missing a non-`Option` field or has the wrong type, `decode` returns an `Err` instead of handing you a half-populated struct.

---

## Detailed Explanation

### A JWT is three base64url parts

Decode the token your server hands out and you find three dot-separated, base64url-encoded segments: `header.payload.signature`. The first two are just JSON. Here is a real token issued by the server later in this page, split and decoded:

```text
parts: 3
header (base64url): eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9

decoded header:  {"typ": "JWT", "alg": "HS256"}
decoded payload: {"sub": "alice", "role": "user", "exp": 1780316473, "iat": 1780315573}
```

> **Warning:** The payload is **encoded, not encrypted**. Anyone holding the token can read the claims (paste it into <https://jwt.io> and see). The signature only guarantees the server issued it and nobody tampered with it. Never put secrets (passwords, full credit-card numbers) in a JWT payload.

### `Claims` is a `serde` struct

`encode` takes any `T: Serialize`; `decode::<T>` takes any `T: DeserializeOwned`. So your claims type is an ordinary `serde` struct (see [Serialization](/15-serialization/)). The **registered claim names** from the JWT spec — `exp`, `iat`, `nbf`, `iss`, `aud`, `sub` — are just fields you name accordingly; everything else (`role` here) is a custom claim. Times are seconds since the Unix epoch as a `u64`, and `get_current_timestamp()` gives you "now" in exactly that format.

### `Header::default()` selects HS256

`Header::default()` is `Header::new(Algorithm::HS256)`: HMAC-SHA256, a symmetric algorithm where the same secret signs and verifies. That is the right default for a monolith that issues and checks its own tokens. If a third party needs to verify without being able to mint tokens, switch to an asymmetric algorithm (`RS256`, `ES256`, `EdDSA`) and hand out only the public key. With `jsonwebtoken` you would build the keys with `EncodingKey::from_rsa_pem(...)` / `DecodingKey::from_rsa_pem(...)` instead of `from_secret`.

### `Validation` is where the security lives

`Validation::new(Algorithm::HS256)` does more than name an algorithm. By default it:

- requires the `exp` claim to be present and rejects expired tokens (`validate_exp = true`);
- allows **60 seconds of clock leeway** so a token that just expired on a slightly-skewed clock still passes;
- pins the accepted algorithm to HS256, so an attacker cannot downgrade the token to `alg: none` or swap algorithms.

You can tighten it further — `validation.set_audience(&["my-api"])`, `validation.set_issuer(&["my-auth-server"])`, `validation.validate_nbf = true`, `validation.leeway = 0`. Importantly, **pinning the algorithm is automatic** here. In some other languages the classic JWT vulnerability is verifying with whatever algorithm the *token's own header* claims; `jsonwebtoken` only accepts algorithms you listed in `Validation`.

### `decode` returns a `Result`, not an exception

Where `jwt.verify` *throws*, `decode` returns `jsonwebtoken::errors::Result<TokenData<T>>`. `TokenData` holds both `.header` and `.claims`. The `?` operator propagates the error; you decide how to map each `ErrorKind` to an HTTP status. This is the same `Result`-vs-exceptions story as the rest of Rust (see [Error Handling](/08-error-handling/)).

---

## Key Differences

| Concern | TypeScript (`jsonwebtoken`) | Rust (`jsonwebtoken` 10) |
| --- | --- | --- |
| Claims type | `object` / cast to interface | A `serde` `struct`; checked at decode time |
| Signing | `jwt.sign(payload, secret, opts)` | `encode(&Header, &claims, &EncodingKey)` |
| Verifying | `jwt.verify(token, secret)` → throws | `decode::<T>(token, &key, &validation)` → `Result` |
| Expiry | `expiresIn: "1h"` string sugar | You set `exp` numerically; `Validation` enforces it |
| Algorithm safety | depends on options | Pinned by `Validation`; never trusts the token's own header |
| Crypto backend | bundled (Node `crypto`) | Cargo feature: `rust_crypto` or `aws_lc_rs` |
| Decoded payload | attached to `req.user` (untyped) | A typed value extracted into the handler signature |
| Failure mode | runtime exception, `any` payload | compile-time-typed claims, explicit error mapping |

The biggest conceptual shift: in Express, verification is *middleware that mutates the request*. In Axum, verification is *an extractor that produces a value*. A handler that takes `claims: Claims` as a parameter is, by construction, unreachable without a valid token: there is no `req.user` that might be `undefined`.

---

## Common Pitfalls

### 1. Forgetting to enable a crypto backend (a runtime panic)

With `jsonwebtoken` 10, `cargo add jsonwebtoken` alone pulls in the default features, which do **not** include a signer. The code compiles, but the first `encode`/`decode` panics:

```text
thread 'main' panicked at jsonwebtoken-10.4.0/src/crypto/mod.rs:124:40:

Could not automatically determine the process-level CryptoProvider from
jsonwebtoken crate features. Call CryptoProvider::install_default() before
this point to select a provider manually, or make sure exactly one of the
'rust_crypto' and 'aws_lc_rs' features is enabled.
See the documentation of the CryptoProvider type for more information.
```

That is the real panic message (file path and line re-wrapping trimmed for width). The fix is to enable a backend: `cargo add jsonwebtoken --features rust_crypto` (or `--features aws_lc_rs`). This is new in version 10; version 9 bundled `ring` and "just worked", so older tutorials omit the feature.

### 2. Omitting `exp` from your claims struct

`Validation` requires the `exp` claim by default. If your `Claims` struct has no `exp` field, every token you mint is missing it, and verification fails — not with a vague error, but a precise one:

```rust
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct NoExpClaims {
    sub: String,
}

fn main() {
    let secret = b"k";
    let token = encode(
        &Header::default(),
        &NoExpClaims { sub: "x".into() },
        &EncodingKey::from_secret(secret),
    )
    .unwrap();

    let v = Validation::new(Algorithm::HS256);
    let result = decode::<NoExpClaims>(&token, &DecodingKey::from_secret(secret), &v);
    println!("{:?}", result.unwrap_err().kind());
}
```

Real output:

```text
MissingRequiredClaim("exp")
```

Either add an `exp: u64` field (the right answer; tokens should expire) or, only if you have a deliberate reason, relax the validator with `validation.required_spec_claims.clear()` and `validation.validate_exp = false`.

### 3. Returning the raw `jsonwebtoken` error from a handler

A handler's return type must be something Axum can turn into a response (`impl IntoResponse`). `jsonwebtoken::errors::Error` is not, so this does not compile:

```rust
use axum::{routing::get, Json, Router};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: u64,
}

// does not compile (error[E0277]): jsonwebtoken's Error is not IntoResponse,
// so this is not a valid Axum handler.
async fn me() -> Result<Json<Claims>, jsonwebtoken::errors::Error> {
    let token = "x.y.z";
    let v = Validation::new(Algorithm::HS256);
    let data = decode::<Claims>(token, &DecodingKey::from_secret(b"k"), &v)?;
    Ok(Json(data.claims))
}

#[tokio::main]
async fn main() {
    let app: Router = Router::new().route("/me", get(me));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

The real compiler error is the classic "handler not satisfied" message:

```text
error[E0277]: the trait bound `fn() -> impl Future<Output = Result<..., ...>> {me}: Handler<_, _>` is not satisfied
  --> src/main.rs:22:54
   |
22 |     let app: Router = Router::new().route("/me", get(me));
   |                                                  --- ^^ unsatisfied trait bound
   |
   = help: the trait `Handler<_, _>` is not implemented for fn item `...Result<axum::Json<Claims>, jsonwebtoken::errors::Error>...`
   = note: Consider using `#[axum::debug_handler]` to improve the error message
```

The fix is to define your own error type that implements `IntoResponse` (shown in the next section), and `.map_err(...)` into it. As the note suggests, adding `#[axum::debug_handler]` to the function gives a far clearer message about *which* bound failed; reach for it whenever you see "Handler is not satisfied". See [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/).

### 4. Confusing "decoded" with "verified"

`jsonwebtoken::decode_header` and the `dangerous` module can read a token *without* checking the signature. That is only for inspecting the `kid`/`alg` to pick a key; never trust those claims. Real verification is always `decode::<T>(...)` with a real `Validation`.

---

## Best Practices

- **Always enable exactly one crypto backend** (`rust_crypto` or `aws_lc_rs`) and keep it consistent across your workspace.
- **Always include `exp`** and keep access tokens short-lived (minutes, not days). Pair a short access token with a longer-lived refresh token stored server-side or in an `HttpOnly` cookie (see [Sessions](/16-web-apis/14-sessions/)).
- **Load the secret from the environment, never hard-code it.** `std::env::var("JWT_SECRET")`. A weak or leaked HMAC secret means anyone can forge tokens. See [Deployment](/16-web-apis/19-deployment/) for env config.
- **Build `EncodingKey`/`DecodingKey` once at startup** and store them in your `State`. They own the parsed key material; rebuilding them per request is wasted work. See [State Management](/16-web-apis/06-state-management/).
- **Pin the algorithm** via `Validation::new(Algorithm::HS256)` (the default behavior) so a token cannot dictate its own verification algorithm.
- **Verify inside an extractor**, not by hand in every handler, so the type system enforces "this route requires auth". Layer a second extractor for authorization (role checks).
- **Map errors deliberately**: a bad signature, an expired token, and a missing header are all `401`, but a valid token whose *role* is insufficient is `403`. Do not leak which one failed in detail to clients.
- For password hashing at the `/login` step, use Argon2/bcrypt — never store or compare plaintext (covered in [Section 27: Security](/27-security/)).

---

## Real-World Example

A production-flavored auth module: a `Keys` bundle in shared state, a `Claims` struct with a typed `Role`, a custom `AuthError` that implements `IntoResponse`, an `AuthUser` extractor for "any logged-in user", and an `AdminUser` extractor that layers a role check on top. Add the crates first:

```bash
cargo add axum
cargo add tokio --features full
cargo add jsonwebtoken --features rust_crypto
cargo add serde --features derive
cargo add serde_json
cargo add thiserror
```

```rust
use axum::{
    extract::{FromRequestParts, State},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use jsonwebtoken::{
    decode, encode, get_current_timestamp, Algorithm, DecodingKey, EncodingKey, Header,
    Validation,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const ACCESS_TTL_SECS: u64 = 15 * 60; // 15 minutes

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Role {
    User,
    Admin,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    role: Role,
    exp: u64,
    iat: u64,
}

#[derive(thiserror::Error, Debug)]
enum AuthError {
    #[error("missing or malformed Authorization header")]
    MissingBearer,
    #[error("token is invalid or expired")]
    InvalidToken,
    #[error("you do not have permission to access this resource")]
    Forbidden,
    #[error("invalid credentials")]
    BadCredentials,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = match self {
            AuthError::MissingBearer | AuthError::InvalidToken | AuthError::BadCredentials => {
                StatusCode::UNAUTHORIZED
            }
            AuthError::Forbidden => StatusCode::FORBIDDEN,
        };
        let body = Json(serde_json::json!({ "error": self.to_string() }));
        (status, body).into_response()
    }
}

#[derive(Clone)]
struct AppState {
    keys: Arc<Keys>,
}

struct Keys {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
}

impl Keys {
    fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            validation: Validation::new(Algorithm::HS256),
        }
    }
}

// The base extractor: any authenticated user. Verifies the bearer token.
struct AuthUser(Claims);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(AuthError::MissingBearer)?;

        let data = decode::<Claims>(token, &state.keys.decoding, &state.keys.validation)
            .map_err(|_| AuthError::InvalidToken)?;

        Ok(AuthUser(data.claims))
    }
}

// A second extractor that layers an authorization check on top of authentication.
struct AdminUser(Claims);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let AuthUser(claims) = AuthUser::from_request_parts(parts, state).await?;
        if claims.role != Role::Admin {
            return Err(AuthError::Forbidden);
        }
        Ok(AdminUser(claims))
    }
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: u64,
}

fn issue_token(keys: &Keys, sub: &str, role: Role) -> Result<String, AuthError> {
    let now = get_current_timestamp();
    let claims = Claims {
        sub: sub.to_string(),
        role,
        iat: now,
        exp: now + ACCESS_TTL_SECS,
    };
    encode(&Header::default(), &claims, &keys.encoding).map_err(|_| AuthError::InvalidToken)
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, AuthError> {
    // A real implementation looks the user up and verifies an Argon2 hash.
    let role = match (body.username.as_str(), body.password.as_str()) {
        ("alice", "hunter2") => Role::User,
        ("root", "toor") => Role::Admin,
        _ => return Err(AuthError::BadCredentials),
    };
    let token = issue_token(&state.keys, &body.username, role)?;
    Ok(Json(TokenResponse {
        access_token: token,
        token_type: "Bearer",
        expires_in: ACCESS_TTL_SECS,
    }))
}

// `AuthUser(claims)` in the signature is the guard: this only runs for valid tokens.
async fn profile(AuthUser(claims): AuthUser) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "user": claims.sub, "role": claims.role }))
}

async fn admin_metrics(AdminUser(claims): AdminUser) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "secret_metrics": 42, "viewed_by": claims.sub }))
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/me", get(profile))
        .route("/admin/metrics", get(admin_metrics))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-only-secret".to_string());
    let state = AppState {
        keys: Arc::new(Keys::new(secret.as_bytes())),
    };
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on http://0.0.0.0:3000");
    axum::serve(listener, app(state)).await.unwrap();
}
```

Exercising it with `curl` against the running server produces exactly this (tokens trimmed for brevity):

```text
# POST /login as the user "alice"
{"access_token": "eyJ0eXAiOiJKV1Qi...", "token_type": "Bearer", "expires_in": 900}

# GET /me with alice's token            -> 200
{"role":"user","user":"alice"}

# GET /admin/metrics with alice's token -> 403 (authenticated but not an admin)
{"error":"you do not have permission to access this resource"}

# GET /admin/metrics with root's token  -> 200
{"secret_metrics":42,"viewed_by":"root"}

# GET /me with no Authorization header   -> 401
{"error":"missing or malformed Authorization header"}
```

The payoff: `profile` and `admin_metrics` contain **zero** auth code. The presence of `AuthUser` / `AdminUser` in their signatures means the router cannot route to them without a valid token (and the right role). Authentication and authorization are encoded in the function signature, checked once, and impossible to forget.

> **Tip:** This `AuthUser` / `AdminUser` extractor pattern is the heart of the broader [Authentication](/16-web-apis/12-authentication/) page — here we focus on the JWT mechanics; that page compares extractor-guards against middleware-based auth more generally.

---

## Further Reading

- [`jsonwebtoken` crate docs (docs.rs)](https://docs.rs/jsonwebtoken): `encode`, `decode`, `Validation`, `Header`, and the algorithm list.
- [`Validation` (docs.rs)](https://docs.rs/jsonwebtoken/latest/jsonwebtoken/struct.Validation.html). Every knob: leeway, required claims, audience, issuer.
- [RFC 7519: JSON Web Token](https://datatracker.ietf.org/doc/html/rfc7519): the spec, including the registered claim names (`exp`, `iat`, `iss`, `aud`, `sub`, `nbf`).
- [jwt.io](https://jwt.io): paste a token to inspect its header and payload (a reminder that the payload is readable).
- Within this guide:
  - [Authentication](/16-web-apis/12-authentication/): extractor-guard vs middleware auth; the `AuthUser` pattern in general.
  - [Extractors](/16-web-apis/04-extractors/): how `FromRequestParts` works and why ordering matters.
  - [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/): `AppError: IntoResponse`, `thiserror`, mapping errors to status codes.
  - [Sessions](/16-web-apis/14-sessions/): cookie-based and server-side sessions, and where refresh tokens live.
  - [State Management](/16-web-apis/06-state-management/): holding your `Keys` in `State<T>` + `Arc`.
  - [Middleware](/16-web-apis/05-middleware/): Tower layers, if you prefer middleware-based auth.
  - [Section 27: Security](/27-security/): password hashing (Argon2) for the login step.
  - Background: [Error Handling](/08-error-handling/), [Serialization](/15-serialization/), [Traits](/09-generics-traits/).

---

## Exercises

### Exercise 1: Add audience and issuer validation

**Difficulty:** Beginner

**Objective:** Lock a token down to a specific `aud` (audience) and `iss` (issuer), and observe the rejection when they do not match.

**Instructions:** Define a `Claims` struct with `sub`, `aud`, `iss`, and `exp`. Encode a token with `aud = "my-api"` and `iss = "my-auth-server"`. Build a `Validation` that calls `set_audience(&["my-api"])` and `set_issuer(&["my-auth-server"])`, and confirm a matching token decodes. Then build a second validator expecting `aud = "other-api"` and confirm it is rejected. Print the `ErrorKind` of the rejection.

<details>
<summary>Solution</summary>

```rust
use jsonwebtoken::{
    decode, encode, errors::ErrorKind, get_current_timestamp, Algorithm, DecodingKey,
    EncodingKey, Header, Validation,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    aud: String,
    iss: String,
    exp: u64,
}

fn main() {
    let secret = b"k";
    let claims = Claims {
        sub: "user_1".into(),
        aud: "my-api".into(),
        iss: "my-auth-server".into(),
        exp: get_current_timestamp() + 3600,
    };
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(secret)).unwrap();

    // Matching validator.
    let mut v = Validation::new(Algorithm::HS256);
    v.set_audience(&["my-api"]);
    v.set_issuer(&["my-auth-server"]);
    let ok = decode::<Claims>(&token, &DecodingKey::from_secret(secret), &v);
    println!("matching aud+iss -> {:?}", ok.is_ok());

    // Wrong audience.
    let mut v2 = Validation::new(Algorithm::HS256);
    v2.set_audience(&["other-api"]);
    match decode::<Claims>(&token, &DecodingKey::from_secret(secret), &v2) {
        Err(e) if matches!(e.kind(), ErrorKind::InvalidAudience) => {
            println!("wrong aud -> InvalidAudience")
        }
        Err(e) => println!("wrong aud -> {:?}", e.kind()),
        Ok(_) => println!("wrong aud -> unexpectedly Ok"),
    }
}
```

Real output:

```text
matching aud+iss -> true
wrong aud -> InvalidAudience
```

</details>

### Exercise 2: Distinguish the rejection reasons

**Difficulty:** Intermediate

**Objective:** Verify that the three common failures — wrong secret, expired token, and tampered body — surface as distinct `ErrorKind`s, and observe the default leeway.

**Instructions:** Issue a valid HS256 token. Then (a) verify it with the wrong secret, (b) issue a token whose `exp` is in the past and verify it, and (c) flip one character in the middle of a valid token and verify it. Print the `ErrorKind` for each. Finally, issue a token that expired 30 seconds ago and show it still passes the default validator (60-second leeway).

<details>
<summary>Solution</summary>

```rust
use jsonwebtoken::{
    decode, encode, get_current_timestamp, Algorithm, DecodingKey, EncodingKey, Header,
    Validation,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: u64,
}

fn token_with_exp(secret: &[u8], exp: u64) -> String {
    let claims = Claims { sub: "u".into(), exp };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret)).unwrap()
}

fn verify(token: &str, secret: &[u8]) -> jsonwebtoken::errors::Result<()> {
    decode::<Claims>(token, &DecodingKey::from_secret(secret), &Validation::new(Algorithm::HS256))?;
    Ok(())
}

fn main() {
    let secret = b"super-secret-key";
    let now = get_current_timestamp();
    let token = token_with_exp(secret, now + 3600);

    // (a) wrong secret
    println!("wrong secret  -> {:?}", verify(&token, b"nope").unwrap_err().kind());

    // (b) expired an hour ago
    let expired = token_with_exp(secret, now - 3600);
    println!("expired       -> {:?}", verify(&expired, secret).unwrap_err().kind());

    // (c) tamper with one char
    let mut chars: Vec<char> = token.chars().collect();
    let mid = chars.len() / 2;
    chars[mid] = if chars[mid] == 'a' { 'b' } else { 'a' };
    let tampered: String = chars.into_iter().collect();
    println!("tampered      -> {:?}", verify(&tampered, secret).unwrap_err().kind());

    // (d) leeway: expired 30s ago still passes the default 60s leeway
    let recent = token_with_exp(secret, now - 30);
    println!("expired 30s, 60s leeway -> {:?}", verify(&recent, secret).is_ok());
}
```

Real output:

```text
wrong secret  -> InvalidSignature
expired       -> ExpiredSignature
tampered      -> InvalidSignature
expired 30s, 60s leeway -> true
```

</details>

### Exercise 3: A `RefreshToken` extractor that rejects access tokens

**Difficulty:** Hard

**Objective:** Model two token *kinds* (`access` and `refresh`) with a `token_type` claim, and write an extractor that only accepts refresh tokens — so an attacker cannot present a short-lived access token at the `/refresh` endpoint.

**Instructions:** Add a `token_type: TokenType` field to your claims (an enum serialized as a lowercase string). Issue access tokens at login and refresh tokens separately. Write a `RefreshToken` extractor (`FromRequestParts<AppState>`) that decodes the bearer token and then returns `AuthError::InvalidToken` unless `token_type == TokenType::Refresh`. Wire up `POST /refresh` that takes the `RefreshToken` extractor and mints a fresh access token.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::{FromRequestParts, State},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use jsonwebtoken::{
    decode, encode, get_current_timestamp, Algorithm, DecodingKey, EncodingKey, Header,
    Validation,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TokenType {
    Access,
    Refresh,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    token_type: TokenType,
    exp: u64,
}

#[derive(Debug)]
enum AuthError {
    MissingBearer,
    InvalidToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let msg = match self {
            AuthError::MissingBearer => "missing bearer token",
            AuthError::InvalidToken => "invalid token",
        };
        (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

#[derive(Clone)]
struct AppState {
    encoding: Arc<EncodingKey>,
    decoding: Arc<DecodingKey>,
}

fn mint(state: &AppState, sub: &str, kind: TokenType, ttl: u64) -> String {
    let claims = Claims {
        sub: sub.to_string(),
        token_type: kind,
        exp: get_current_timestamp() + ttl,
    };
    encode(&Header::default(), &claims, &state.encoding).unwrap()
}

// Only accepts tokens whose token_type is Refresh.
struct RefreshToken(Claims);

impl FromRequestParts<AppState> for RefreshToken {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(AuthError::MissingBearer)?;

        let validation = Validation::new(Algorithm::HS256);
        let data = decode::<Claims>(token, &state.decoding, &validation)
            .map_err(|_| AuthError::InvalidToken)?;

        if data.claims.token_type != TokenType::Refresh {
            return Err(AuthError::InvalidToken);
        }
        Ok(RefreshToken(data.claims))
    }
}

#[derive(Serialize)]
struct AccessTokenResponse {
    access_token: String,
}

// Trade a valid refresh token for a fresh, short-lived access token.
async fn refresh(
    State(state): State<AppState>,
    RefreshToken(claims): RefreshToken,
) -> Json<AccessTokenResponse> {
    let access = mint(&state, &claims.sub, TokenType::Access, 15 * 60);
    Json(AccessTokenResponse { access_token: access })
}

fn app(state: AppState) -> Router {
    Router::new().route("/refresh", post(refresh)).with_state(state)
}

#[tokio::main]
async fn main() {
    let secret = b"dev-only-secret";
    let state = AppState {
        encoding: Arc::new(EncodingKey::from_secret(secret)),
        decoding: Arc::new(DecodingKey::from_secret(secret)),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app(state)).await.unwrap();
}
```

Presenting a refresh token at `POST /refresh` returns `200` with a new `access_token`. Presenting an *access* token (or no token) returns `401 {"error":"invalid token"}`, because the extractor rejects any `token_type` that is not `Refresh` before the handler runs. This is exactly why the `token_type` claim matters: signature validity alone is not enough — the token must be the *right kind* for the endpoint.

</details>
