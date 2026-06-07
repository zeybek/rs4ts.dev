---
title: "Secrets Management"
description: "Load and hold secrets in Rust beyond Node's process.env: secrecy wrapper types that refuse to log themselves and zeroize to wipe secret bytes from memory."
---

API keys, database passwords, signing keys, and OAuth client secrets are the crown jewels of any service. In Node you have probably reached for `process.env` and a `.env` file and called it done. Rust gives you the same environment-variable workflow *plus* two type-level tools that JavaScript simply cannot offer: wrapper types that refuse to print themselves in logs, and types that scrub their bytes from memory when dropped. This page shows how to load, hold, and dispose of secrets safely.

---

## Quick Overview

A **secret** is any value whose disclosure is a security incident: a database password, a third-party API key, a JWT signing key, a TLS private key. Managing them well means three things: **getting them in** from a trusted source (environment variables or a secret store, never hard-coded), **holding them** so they cannot accidentally end up in a log line or a panic message, and **getting rid of them** by clearing their bytes from memory once you are done.

For a TypeScript/JavaScript developer the loading half feels identical: you still read `process.env` / `std::env::var`. The new ideas are the [`secrecy`](https://docs.rs/secrecy) crate, whose `SecretString`/`SecretBox` wrappers redact themselves from `Debug`/`Display` output, and the [`zeroize`](https://docs.rs/zeroize) crate, which overwrites a secret's memory on drop. Neither has a real JavaScript equivalent, because a garbage-collected runtime gives you no control over when (or whether) a string's bytes are wiped.

> **Note:** This page is about *handling* secrets your application already trusts. For the cryptography you perform *with* those secrets see [Cryptography Done Right](/27-security/03-cryptography/) and [Password Hashing](/27-security/04-password-hashing/); for generating new secret material see [Secure Randomness](/27-security/06-secure-randomness/); for keeping the dependencies that touch your secrets free of known vulnerabilities see [Auditing Dependencies and Supply-Chain Hygiene](/27-security/08-security-audit/).

---

## TypeScript/JavaScript Example

A typical Node service loads configuration from the environment, usually with `dotenv` in development:

```typescript
// npm install dotenv
import "dotenv/config"; // loads .env into process.env (dev only)

interface Config {
  serviceName: string;
  bindAddr: string;
  databasePassword: string;
  jwtSigningKey: string;
}

function loadConfig(): Config {
  const required = (name: string): string => {
    const value = process.env[name];
    if (!value) throw new Error(`missing required env var: ${name}`);
    return value;
  };

  return {
    serviceName: required("SERVICE_NAME"),
    bindAddr: process.env.BIND_ADDR ?? "0.0.0.0:8080",
    databasePassword: required("DATABASE_PASSWORD"),
    jwtSigningKey: required("JWT_SIGNING_KEY"),
  };
}

const config = loadConfig();

// The classic foot-gun: logging the whole config object.
console.log("service starting", config);
// service starting {
//   serviceName: 'billing-api',
//   bindAddr: '0.0.0.0:8080',
//   databasePassword: 'vault:db/prod#aB9',   <-- LEAKED into the log!
//   jwtSigningKey: 'vault:jwt/prod#Zx1'       <-- LEAKED!
// }
```

**Key points:**

- `process.env` values are always `string | undefined`, so every read needs a presence check.
- `databasePassword` is just a `string`. Nothing stops it from being printed, JSON-serialized into an error response, or shipped to your logging service.
- A plain `console.log(config)` in production is the single most common way real secrets end up in log aggregators.
- The bytes of that string live in the V8 heap until the garbage collector decides to reclaim them. You cannot force a wipe.

> **Warning:** The same risk exists with structured loggers (`pino`, `winston`) and with `JSON.stringify(config)` in an error handler. Mitigation in JavaScript is purely *convention*: a redaction allowlist you must remember to maintain. There is no type that *enforces* it.

---

## Rust Equivalent

The same loader, but `SecretString` makes the leak structurally impossible. Add the dependencies first:

```bash
cargo add secrecy
cargo add tracing tracing-subscriber
```

This pulls in `secrecy = "0.10"`. `SecretString` is a wrapper whose `Debug` impl prints `[REDACTED]` instead of the value, and which exposes the plaintext only through an explicit `.expose_secret()` call.

```rust
use secrecy::{ExposeSecret, SecretString};
use std::env;
use std::fmt;

struct Config {
    service_name: String,
    bind_addr: String,
    database_password: SecretString,
    jwt_signing_key: SecretString,
}

// A redacting Debug so logging the WHOLE config can never leak a secret.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("service_name", &self.service_name)
            .field("bind_addr", &self.bind_addr)
            // SecretString already redacts itself; being explicit documents intent.
            .field("database_password", &"[REDACTED]")
            .field("jwt_signing_key", &"[REDACTED]")
            .finish()
    }
}

impl Config {
    fn from_env() -> Result<Self, String> {
        let get = |k: &str| env::var(k).map_err(|_| format!("missing env var: {k}"));
        Ok(Config {
            service_name: get("SERVICE_NAME")?,
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            database_password: SecretString::from(get("DATABASE_PASSWORD")?),
            jwt_signing_key: SecretString::from(get("JWT_SIGNING_KEY")?),
        })
    }

    fn database_url(&self) -> String {
        // Expose the secret only at the exact site that needs the plaintext.
        format!("postgres://app:{}@db/prod", self.database_password.expose_secret())
    }
}

fn main() {
    tracing_subscriber::fmt().without_time().with_target(false).init();

    // In production the deployment platform sets these; we set them here to run.
    unsafe {
        env::set_var("SERVICE_NAME", "billing-api");
        env::set_var("DATABASE_PASSWORD", "vault:db/prod#aB9");
        env::set_var("JWT_SIGNING_KEY", "vault:jwt/prod#Zx1");
    }

    let config = Config::from_env().expect("config error");

    // Logging the whole config is now SAFE — the secrets are redacted.
    tracing::info!(?config, "service starting");

    let url = config.database_url();
    tracing::info!(url_len = url.len(), "built database connection string");
}
```

Real output (ANSI colour codes stripped for the page):

```text
 INFO service starting config=Config { service_name: "billing-api", bind_addr: "0.0.0.0:8080", database_password: "[REDACTED]", jwt_signing_key: "[REDACTED]" }
 INFO built database connection string url_len=40
```

**Key points:**

- `SecretString` comes from `secrecy`; `SecretString::from(String)` moves the plaintext inside the wrapper.
- Its `Debug` output is `[REDACTED]`, so the `tracing::info!(?config, ...)` line that would have leaked the password in Node prints a safe placeholder instead.
- To read the plaintext you must call `.expose_secret()` from the `ExposeSecret` trait: an explicit, greppable, code-review-visible action.
- The whole-struct `Debug` impl is belt-and-braces: even non-secret-typed fields added later are not accidentally dumped, and the `[REDACTED]` markers make intent obvious to reviewers.

---

## Detailed Explanation

### Why a wrapper type instead of a `string`

In JavaScript, "don't log the secret" is a rule you enforce by *remembering*, and you remember in dozens of places: the startup log, the request logger, the error handler that serializes context, the debugger watch you forgot to remove. `SecretString` flips this from opt-out to opt-in. The default behaviour of the type is *not to reveal itself*; revealing requires the conscious act of calling `.expose_secret()`. You can `grep` your codebase for `expose_secret` and audit every single place a secret becomes plaintext. There is no equivalent grep for "every place a plain `string` might get logged."

### What `SecretString` actually is

In `secrecy` 0.10, `SecretString` is an alias for `SecretBox<str>`, and `SecretBox<T>` is the general wrapper for any secret value. The wrapper provides:

- A redacting `Debug` (no `Display` at all; see the pitfall below).
- `.expose_secret()` (from the `ExposeSecret` trait) to borrow the inner value.
- Zeroization of the inner bytes when the `SecretBox` is dropped, because `SecretBox` requires its contents to implement `Zeroize` (covered below). A `SecretString` wipes the string's heap buffer on drop; you do not manage that yourself.

```rust
use secrecy::{ExposeSecret, SecretBox, SecretString};

fn main() {
    // A string secret.
    let token = SecretString::from("super-secret-token".to_string());
    println!("debug:   {token:?}");          // redacted
    println!("exposed: {}", token.expose_secret());

    // SecretBox<T> for any zeroizable type — e.g. a 32-byte binary key.
    let key: SecretBox<[u8; 32]> = SecretBox::new(Box::new([7u8; 32]));
    println!("key dbg: {key:?}");             // redacted
    println!("first byte: {}", key.expose_secret()[0]);
}
```

Real output:

```text
debug:   SecretBox<str>([REDACTED])
exposed: super-secret-token
key dbg: SecretBox<[u8; 32]>([REDACTED])
first byte: 7
```

The `Debug` shows the *type* (so logs are still useful for "a database password is present") but never the *value*.

### Getting secrets in: environment variables

The most portable source is the process environment, exactly as in Node. The only Rust-specific wrinkle is that `std::env::var` returns `Result<String, VarError>`, so you handle the missing case explicitly:

```rust
use secrecy::SecretString;
use std::env;

/// Read a required secret from the environment into a SecretString.
fn require_secret(name: &str) -> SecretString {
    let value = env::var(name)
        .unwrap_or_else(|_| panic!("missing required env var: {name}"));
    SecretString::from(value)
}

fn main() {
    unsafe { env::set_var("DATABASE_PASSWORD", "p@ss-from-vault"); }

    let db_password = require_secret("DATABASE_PASSWORD");
    println!("db_password = {db_password:?}"); // redacted
}
```

Real output:

```text
db_password = SecretBox<str>([REDACTED])
```

> **Note:** `env::set_var` is `unsafe` in the current edition because mutating the environment is not thread-safe. You only need it in tests/examples; real deployments set env vars *outside* the process (systemd unit, Kubernetes `Secret`, container `--env`, your platform's secret manager) so your code only ever *reads* them.

### Loading a `.env` file in development

In Node `dotenv` reads a `.env` file into `process.env`. The Rust counterpart is [`dotenvy`](https://docs.rs/dotenvy) (the maintained fork of the original `dotenv` crate):

```bash
cargo add dotenvy
```

```rust
use secrecy::{ExposeSecret, SecretString};

fn main() {
    // Loads key=value pairs from .env into the process environment.
    // Use in DEVELOPMENT ONLY; never commit the .env file.
    dotenvy::dotenv().ok();

    let key = SecretString::from(
        std::env::var("STRIPE_SECRET_KEY").expect("STRIPE_SECRET_KEY not set"),
    );
    println!("loaded secret: {key:?}");
    println!("exposed len:   {}", key.expose_secret().len());
}
```

With a `.env` of `STRIPE_SECRET_KEY=sk_test_loaded_from_dotenv` the real output is:

```text
loaded secret: SecretBox<str>([REDACTED])
exposed len:   26
```

> **Warning:** Add `.env` to your `.gitignore` *before* you ever write a real secret into it. A committed `.env` is one of the most common ways secrets reach a public repository. (See the [project `.gitignore` conventions](/00-introduction/) and section 24's tooling notes.) `dotenvy::dotenv()` should be a dev convenience only; in production, real secret managers inject values directly.

### Secret stores and managers

Environment variables are fine for many services, but they have downsides: they are visible to every child process, may appear in `/proc`, and offer no rotation or audit trail. Mature deployments use a **secret manager** — HashiCorp Vault, AWS Secrets Manager, Google Secret Manager, Azure Key Vault, or Kubernetes Secrets — and your code fetches at startup (or on a refresh interval). Whatever the source, the pattern is the same: fetch the plaintext, immediately wrap it in `SecretString`/`SecretBox`, and from then on the rest of your program handles only the wrapper. Vendor SDK crates (for example `aws-sdk-secretsmanager`) return the value as a `String`; your job is to wrap it the moment it crosses into your code.

### Getting secrets out of memory: `zeroize`

When a `String` is dropped its heap allocation is freed, but the *bytes are not overwritten*; they linger until something else reuses that memory. A core dump, a swapped-out page, or a memory-scraping exploit can recover them. The [`zeroize`](https://docs.rs/zeroize) crate overwrites secret bytes with zeros, and importantly does so in a way the optimizer is not allowed to elide:

```bash
cargo add zeroize --features derive
```

```rust
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
struct Credentials {
    username: String,
    password: String,
}

fn main() {
    // Manual zeroize of a buffer you are done with.
    let mut secret = String::from("api-token-abc123");
    println!("before: {secret:?}");
    secret.zeroize();
    println!("after:  {secret:?} (len = {})", secret.len());

    // ZeroizeOnDrop wipes every field automatically when the value drops.
    {
        let creds = Credentials {
            username: "alice".into(),
            password: "hunter2".into(),
        };
        println!("using creds for {}", creds.username);
    } // <- password bytes overwritten with zeros HERE
    println!("creds dropped and wiped");
}
```

Real output:

```text
before: "api-token-abc123"
after:  "" (len = 0)
using creds for alice
creds dropped and wiped
```

`#[derive(Zeroize)]` gives you a `.zeroize()` method that clears every field; `#[derive(ZeroizeOnDrop)]` runs it automatically in the destructor. `secrecy`'s `SecretBox` uses exactly this machinery internally, which is why a `SecretString` already wipes itself. You typically reach for raw `zeroize` only for buffers you manage yourself (a decrypted plaintext, a key derivation scratch buffer).

> **Note:** Zeroization is a *defence in depth*, not a guarantee. The OS may have already copied the page to swap, and a value moved on the stack can leave copies behind. It meaningfully shrinks the window in which a secret sits in recoverable memory; it does not make a secret impossible to recover.

---

## Key Differences

| Concern | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Reading env vars | `process.env.X` → `string \| undefined` | `std::env::var("X")` → `Result<String, VarError>` |
| `.env` files | `dotenv` | `dotenvy` (dev only) |
| Preventing accidental logging | Convention + logger redaction allowlists | `SecretString`/`SecretBox` redact in `Debug`, no `Display` |
| Revealing the plaintext | Implicit: it is just a `string` | Explicit `.expose_secret()`, greppable |
| Wiping memory | Not possible (GC owns the heap) | `zeroize` overwrites bytes deterministically |
| Wiping on scope exit | Not possible | `#[derive(ZeroizeOnDrop)]` / `Drop` |
| Serialization safety | `JSON.stringify` leaks by default | `SecretString` opts out of `serde::Serialize` unless you opt in |

The deep difference is **enforcement vs. discipline**. JavaScript can only ask you to remember not to log a secret. Rust's type system lets a secret *refuse to be logged*: the compiler will not let you format a `SecretString` with `{}`, and its `Debug` is redacted. Combined with deterministic destruction (`Drop`/`zeroize`), you get controls a garbage-collected runtime cannot provide.

---

## Common Pitfalls

### Trying to print a secret directly

The compiler stops you from interpolating a `SecretString`, because it deliberately does not implement `Display`:

```rust
use secrecy::SecretString;

fn main() {
    let token = SecretString::from("super-secret".to_string());
    println!("token = {token}"); // does not compile (error[E0277]: SecretBox<str> doesn't implement Display)
}
```

The real compiler error:

```text
error[E0277]: `SecretBox<str>` doesn't implement `std::fmt::Display`
 --> src/main.rs:6:24
  |
6 |     println!("token = {token}");
  |                       -^^^^^-
  |                       ||
  |                       |`SecretBox<str>` cannot be formatted with the default formatter
  |                       required by this formatting parameter
  |
  = help: the trait `std::fmt::Display` is not implemented for `SecretBox<str>`
```

This is the feature working as designed: the *only* way to get the plaintext into a string is the explicit `.expose_secret()`. Treat this error as a prompt to ask "do I really need the plaintext here, or am I about to log a secret?"

### Exposing too early and too widely

`.expose_secret()` returns a borrow of the plaintext. If you call it at the top of a function and pass the resulting `&str` around, you have effectively widened the secret back into an ordinary string for that whole scope. Call it at the *narrowest* possible site — directly inside the `format!` or the SDK call that needs it — so the plaintext's lifetime is as short as possible:

```rust
use secrecy::{ExposeSecret, SecretString};

fn build_auth_header(api_key: &SecretString) -> String {
    // Good: expose only inside the one expression that needs it.
    format!("Bearer {}", api_key.expose_secret())
}

fn main() {
    let key = SecretString::from("sk-live-9f8b7a6c".to_string());
    let header = build_auth_header(&key);
    println!("header len = {}", header.len());
}
```

Real output:

```text
header len = 23
```

### Forgetting that derived `Serialize` leaks

If you `#[derive(Serialize)]` on a config struct and the field is a plain `String`, `serde_json::to_string(&config)` will happily emit the secret, the same trap as `JSON.stringify`. `SecretString` does *not* implement `Serialize` by default (you must enable the `serde` feature *and* opt in explicitly), so a derived `Serialize` on a struct containing a `SecretString` fails to compile rather than silently leaking. Keep secrets in `SecretString`, or give the struct a hand-written `Serialize`/`Debug` that omits them.

### Putting secrets on the command line

Passing a secret as a CLI argument (`--api-key sk-live-...`) exposes it in `ps` output and shell history to every user on the box, the same hazard as in any language. Read secrets from the environment or a file, not from `argv`. See [18-cli-tools](/18-cli-tools/) for argument parsing patterns and how to accept secrets safely (e.g. from stdin or an env var).

### Assuming zeroize guarantees erasure

`zeroize` cannot un-leak a value the OS already copied to swap, nor reach copies left by earlier moves. It is a strong mitigation, not a proof of erasure. Do not let it lull you into being careless about *where* a secret travels.

---

## Best Practices

- **Never hard-code secrets.** No literals in source, no committed `.env`. Inject via the environment or a secret manager at deploy time. Gitignore `.env` before the first write.
- **Wrap on arrival.** The instant a secret enters your process (env read, Vault fetch, file read), wrap it in `SecretString`/`SecretBox`. The rest of the program should only ever see the wrapper.
- **Expose at the last possible moment.** Call `.expose_secret()` inline at the one call that needs plaintext, never at the top of a function. Audit every call site with `grep -r expose_secret`.
- **Give config structs a redacting `Debug`.** Even with `SecretString` fields, a hand-written `Debug` documents intent and protects future non-secret fields from accidental dumps.
- **Keep secrets out of `serde::Serialize`.** Do not let a config struct that holds secrets be JSON-serialized. If you must serialize a struct that *contains* a secret, write the impl by hand and skip the secret field.
- **Zeroize raw buffers you own.** For decrypted plaintext or key-derivation scratch space you manage manually, use `#[derive(ZeroizeOnDrop)]` or call `.zeroize()` when done.
- **Prefer a secret manager over bare env vars** for production: rotation, audit logs, and least-privilege access beat a static `DATABASE_PASSWORD`.
- **Scrub your logs, error responses, and panics too.** A redacted `Debug` only helps if you log via `Debug`. Make sure error types and HTTP error bodies (see [28-production](/28-production/) for observability) never embed exposed secrets.

---

## Real-World Example

A production service config that combines every technique: load from the environment, wrap secrets in `SecretString`, redact the whole struct in `Debug`, fail closed if a required secret is missing, and expose plaintext only at the connection-string boundary.

```bash
cargo add secrecy
cargo add tracing tracing-subscriber
```

```rust
use secrecy::{ExposeSecret, SecretString};
use std::env;
use std::fmt;

/// All application configuration. Secrets are wrapped so they cannot leak via logs.
struct Config {
    service_name: String,
    bind_addr: String,
    database_password: SecretString,
    jwt_signing_key: SecretString,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("service_name", &self.service_name)
            .field("bind_addr", &self.bind_addr)
            .field("database_password", &"[REDACTED]")
            .field("jwt_signing_key", &"[REDACTED]")
            .finish()
    }
}

impl Config {
    /// Fail-closed loader: a missing required secret returns Err, never a default.
    fn from_env() -> Result<Self, String> {
        let get = |k: &str| env::var(k).map_err(|_| format!("missing env var: {k}"));
        Ok(Config {
            service_name: get("SERVICE_NAME")?,
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            database_password: SecretString::from(get("DATABASE_PASSWORD")?),
            jwt_signing_key: SecretString::from(get("JWT_SIGNING_KEY")?),
        })
    }

    /// Build a connection string, exposing the password only here.
    fn database_url(&self) -> String {
        format!("postgres://app:{}@db/prod", self.database_password.expose_secret())
    }
}

fn main() {
    tracing_subscriber::fmt().without_time().with_target(false).init();

    // A real deployment sets these outside the process; set them here to run.
    unsafe {
        env::set_var("SERVICE_NAME", "billing-api");
        env::set_var("DATABASE_PASSWORD", "vault:db/prod#aB9");
        env::set_var("JWT_SIGNING_KEY", "vault:jwt/prod#Zx1");
    }

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            // Note: the error mentions the var NAME, never a value.
            tracing::error!(error = %e, "failed to load configuration");
            std::process::exit(1);
        }
    };

    // Safe: secrets are redacted in the struct's Debug impl.
    tracing::info!(?config, "service starting");

    // Plaintext is exposed only inside database_url(), nowhere else.
    let url = config.database_url();
    tracing::info!(url_len = url.len(), "built database connection string");
}
```

Real output (ANSI colour codes stripped):

```text
 INFO service starting config=Config { service_name: "billing-api", bind_addr: "0.0.0.0:8080", database_password: "[REDACTED]", jwt_signing_key: "[REDACTED]" }
 INFO built database connection string url_len=40
```

The startup log line — the one that leaks secrets in the Node version — is now safe, and the only place plaintext exists is the single `format!` inside `database_url()`.

---

## Further Reading

- [`secrecy` crate documentation](https://docs.rs/secrecy) — `SecretString`, `SecretBox`, `ExposeSecret`.
- [`zeroize` crate documentation](https://docs.rs/zeroize) — `Zeroize`, `ZeroizeOnDrop`, and why the wipe is not optimized away.
- [`dotenvy` crate documentation](https://docs.rs/dotenvy) — loading `.env` files in development.
- [OWASP Secrets Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html) — vendor-neutral guidance on stores, rotation, and access.
- [`std::env::var` documentation](https://doc.rust-lang.org/std/env/fn.var.html) — reading environment variables.
- Related guide sections: [Cryptography Done Right](/27-security/03-cryptography/) · [Password Hashing](/27-security/04-password-hashing/) · [Secure Randomness](/27-security/06-secure-randomness/) · [Auditing Dependencies and Supply-Chain Hygiene](/27-security/08-security-audit/) · [Input Validation and Sanitization](/27-security/00-input-validation/).
- For where secrets surface in operations: [28-production](/28-production/). For the `Debug`/`Drop`/`Display` traits underpinning redaction and zeroization: [02-basics](/02-basics/) and [01-getting-started](/01-getting-started/).

---

## Exercises

### Exercise 1: A self-redacting secret newtype

**Difficulty:** Beginner

**Objective:** Understand how a wrapper type controls its own `Debug` output.

**Instructions:** Without using the `secrecy` crate, write a newtype `ApiToken(String)` with a `new` constructor, an `expose(&self) -> &str` accessor, and a hand-written `Debug` impl that prints `ApiToken([REDACTED N bytes])` (where `N` is the length) instead of the value. Prove with an `assert_eq!` that `expose()` still returns the real token.

<details>
<summary>Solution</summary>

```rust
use std::fmt;

struct ApiToken(String);

impl ApiToken {
    fn new(raw: impl Into<String>) -> Self {
        ApiToken(raw.into())
    }
    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ApiToken([REDACTED {} bytes])", self.0.len())
    }
}

fn main() {
    let token = ApiToken::new("sk-live-abc123");
    println!("{token:?}");
    assert_eq!(token.expose(), "sk-live-abc123");
}
```

Real output:

```text
ApiToken([REDACTED 14 bytes])
```

This is exactly the principle `SecretString` automates: the default behaviour reveals nothing, and the only way to the plaintext is a named accessor.

</details>

### Exercise 2: Wipe a buffer on drop

**Difficulty:** Intermediate

**Objective:** Use `zeroize` to clear a secret from memory deterministically.

**Instructions:** Add `zeroize` with the `derive` feature. Define a `SessionKey` struct holding a `Vec<u8>` and a `String`, derive `Zeroize` and `ZeroizeOnDrop`, construct one in an inner scope, use it, and let it drop. Then, in the same `main`, demonstrate a *manual* `.zeroize()` on a standalone `String` and print its length before and after to show it was cleared.

<details>
<summary>Solution</summary>

```bash
cargo add zeroize --features derive
```

```rust
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
struct SessionKey {
    bytes: Vec<u8>,
    label: String,
}

fn main() {
    {
        let key = SessionKey {
            bytes: vec![0xAB; 16],
            label: "session-2026".into(),
        };
        println!("using key '{}' ({} bytes)", key.label, key.bytes.len());
    } // <- bytes and label overwritten with zeros here

    let mut token = String::from("temporary-token-xyz");
    println!("before zeroize: len = {}", token.len());
    token.zeroize();
    println!("after zeroize:  len = {}, value = {token:?}", token.len());
}
```

Real output:

```text
using key 'session-2026' (16 bytes)
before zeroize: len = 19
after zeroize:  len = 0, value = ""
```

`.zeroize()` empties the buffer in place: the length drops from 19 to 0 and the bytes are overwritten.

</details>

### Exercise 3: A fail-closed config loader with redacted logging

**Difficulty:** Advanced

**Objective:** Combine `SecretString`, a redacting `Debug`, and fail-closed loading into a realistic config type.

**Instructions:** Using `secrecy`, write a `ServiceConfig` with one plain field (`port: u16`) and one secret field (`webhook_secret: SecretString`). Provide a `from_env()` that returns `Result<ServiceConfig, String>` and fails if `WEBHOOK_SECRET` is missing or if `PORT` is missing/not a valid `u16`. Give it a `Debug` impl that redacts the secret. In `main`, set the env vars, load, log the whole struct, and then expose the secret once to print only its length.

<details>
<summary>Solution</summary>

```bash
cargo add secrecy
```

```rust
use secrecy::{ExposeSecret, SecretString};
use std::env;
use std::fmt;

struct ServiceConfig {
    port: u16,
    webhook_secret: SecretString,
}

impl fmt::Debug for ServiceConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceConfig")
            .field("port", &self.port)
            .field("webhook_secret", &"[REDACTED]")
            .finish()
    }
}

impl ServiceConfig {
    fn from_env() -> Result<Self, String> {
        let port = env::var("PORT")
            .map_err(|_| "missing env var: PORT".to_string())?
            .parse::<u16>()
            .map_err(|e| format!("PORT is not a valid u16: {e}"))?;

        let webhook_secret = env::var("WEBHOOK_SECRET")
            .map(SecretString::from)
            .map_err(|_| "missing env var: WEBHOOK_SECRET".to_string())?;

        Ok(ServiceConfig { port, webhook_secret })
    }
}

fn main() {
    unsafe {
        env::set_var("PORT", "8443");
        env::set_var("WEBHOOK_SECRET", "whsec_3f9a1b7c2d");
    }

    let config = match ServiceConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            std::process::exit(1);
        }
    };

    // Safe whole-struct log.
    println!("{config:?}");

    // Expose once, reveal nothing but the length.
    let len = config.webhook_secret.expose_secret().len();
    println!("webhook secret length = {len}");
}
```

Real output:

```text
ServiceConfig { port: 8443, webhook_secret: "[REDACTED]" }
webhook secret length = 16
```

The loader fails closed (a missing or malformed value returns `Err` rather than a silent default), the secret is redacted everywhere it could be logged, and the plaintext is exposed at exactly one auditable site.

</details>
