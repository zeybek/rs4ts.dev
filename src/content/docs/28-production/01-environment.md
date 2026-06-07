---
title: "Environment-Based Configuration"
description: "Turn environment variables into validated, typed Rust config at startup with std::env::var and dotenvy, the 12-factor counterpart to Node's process.env and zod."
---

A production service is configured by its **environment**, not by code. The same compiled binary must run unchanged in development, staging, and production, with only the surrounding environment variables differing. This is the heart of the [Twelve-Factor App](https://12factor.net/config) methodology, and Rust's type system lets you turn loosely-typed environment strings into a validated, typed configuration object at startup, failing loudly the moment something is missing or malformed.

---

## Quick Overview

The **12-factor** principle is simple: store configuration in the environment, keep it strictly separate from code, and never commit secrets to your repository. In Node.js you reach for `process.env` (often paired with `dotenv` and a schema validator like `zod`); in Rust you read `std::env::var`, layer in `dotenvy` for local development, and parse everything into a typed struct. The big win in Rust is that **validation happens once, at startup**: if `DATABASE_URL` is missing, the process refuses to boot instead of crashing on the first request three hours later.

> **Note:** This page focuses specifically on the *environment* as a config source: the 12-factor model, loading a `.env` file in development with `dotenvy`, and validating required variables when the program starts. For richer layered configuration (file + environment + defaults merged into typed settings via the `config`/`figment` crates) see [Application Configuration](/28-production/00-configuration/), and for failing-but-staying-alive readiness signals see [Health and Readiness Endpoints](/28-production/03-health-checks/).

---

## TypeScript/JavaScript Example

In a Node.js service, environment variables arrive as `process.env`, where every value is either a `string` or `undefined`. The idiomatic modern approach loads a `.env` file in development and validates the result against a schema so the app fails fast.

```typescript
// config.ts
import "dotenv/config"; // loads .env into process.env (dev convenience)
import { z } from "zod";

// process.env values are ALWAYS `string | undefined` — TypeScript cannot
// know what's actually set at runtime, so we validate explicitly.
const EnvSchema = z.object({
  DATABASE_URL: z.string().url(),
  PORT: z.coerce.number().int().min(1).max(65535).default(8080),
  LOG_LEVEL: z.enum(["debug", "info", "warn", "error"]).default("info"),
  JWT_SECRET: z.string().min(16),
});

const result = EnvSchema.safeParse(process.env);

if (!result.success) {
  console.error("Invalid environment configuration:");
  for (const issue of result.error.issues) {
    console.error(`  - ${issue.path.join(".")}: ${issue.message}`);
  }
  process.exit(1);
}

export const config = result.data;
// config.PORT is `number`, config.LOG_LEVEL is the union, etc.
```

Running this against an environment where `DATABASE_URL` is unset and `PORT` is out of range prints real `zod` errors and exits:

```text
Invalid environment configuration:
  - DATABASE_URL: Invalid input: expected string, received undefined
  - PORT: Too big: expected number to be <=65535
```

This pattern works, but it relies on discipline: nothing in the language *forces* you to validate. Plenty of Node services read `process.env.PORT` directly, get a `string`, and quietly pass `"3000"` where a `number` was expected. Or worse, `Number(process.env.PORT)` silently becomes `0` or `NaN` when the variable is empty or malformed.

---

## Rust Equivalent

Rust reads the same environment with `std::env::var`, which returns a `Result<String, VarError>` — the type system makes the "this might be missing" case impossible to ignore. We load a `.env` file in development with the `dotenvy` crate, then parse everything into a typed `Config` struct, returning a structured error if anything is wrong.

Add the dependencies in a fresh project (`cargo new` selects the latest stable toolchain, currently Rust 1.96.0 on the 2024 edition):

```toml
# Cargo.toml
[dependencies]
dotenvy = "0.15"
```

```rust
use std::env;

#[derive(Debug)]
struct Config {
    database_url: String,
    port: u16,
    log_level: String,
}

#[derive(Debug)]
enum ConfigError {
    Missing(&'static str),
    Invalid { key: &'static str, reason: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Missing(key) => {
                write!(f, "required environment variable `{key}` is not set")
            }
            ConfigError::Invalid { key, reason } => {
                write!(f, "environment variable `{key}` is invalid: {reason}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

// Fetch a required string, or report exactly which key is missing.
fn required(key: &'static str) -> Result<String, ConfigError> {
    env::var(key).map_err(|_| ConfigError::Missing(key))
}

// Parse a value into any `FromStr` type, falling back to a default when unset.
fn parsed<T>(key: &'static str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(key) {
        Ok(raw) => raw.parse::<T>().map_err(|e| ConfigError::Invalid {
            key,
            reason: e.to_string(),
        }),
        Err(_) => Ok(default),
    }
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Config {
            database_url: required("DATABASE_URL")?,
            port: parsed("PORT", 8080)?,
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".into()),
        })
    }
}

fn main() {
    // In real code, `dotenvy::dotenv()` runs here first (shown below).
    match Config::from_env() {
        Ok(config) => println!("loaded config: {config:?}"),
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(1);
        }
    }
}
```

With `DATABASE_URL=postgres://localhost/app` and `PORT=3000` set in the environment, this prints:

```text
loaded config: Config { database_url: "postgres://localhost/app", port: 3000, log_level: "info" }
```

Run it with `DATABASE_URL` unset and the process refuses to start, with a clear message and a non-zero exit code:

```text
configuration error: required environment variable `DATABASE_URL` is not set
```

That non-zero exit is exactly what an orchestrator like Kubernetes wants: a misconfigured pod should crash-loop visibly, not start and serve errors.

---

## Detailed Explanation

### `std::env::var` returns a `Result`, not a nullable string

In Node, `process.env.DATABASE_URL` has type `string | undefined`, and it is entirely up to you to remember the `undefined` half. In Rust:

```rust
use std::env;

fn main() {
    let result: Result<String, env::VarError> = env::var("DATABASE_URL");
    match result {
        Ok(value) => println!("got {value}"),
        Err(env::VarError::NotPresent) => println!("not set"),
        Err(env::VarError::NotUnicode(_)) => println!("set, but not valid UTF-8"),
    }
}
```

The return type is `Result<String, VarError>`. You cannot accidentally use the value as if it were always present — the compiler forces you to handle the `Err` arm. `VarError` even distinguishes "not present" from "present but not valid Unicode", a case Node hides from you entirely.

### Loading `.env` in development with `dotenvy`

In development you don't want to type `DATABASE_URL=... PORT=... cargo run` every time. The `dotenvy` crate reads a `.env` file and injects its keys into the process environment, exactly like Node's `dotenv` package.

> **Note:** Use **`dotenvy`**, not the older `dotenv` crate. The original `dotenv` is unmaintained; `dotenvy` is its actively-maintained fork and the community standard. The crate is named `dotenvy` and so is the import path.

Given a `.env` file in your project root:

```text
DATABASE_URL=postgres://localhost/myapp
PORT=8080
# comments and blank lines are allowed
LOG_LEVEL=debug
```

You load it once, as early as possible in `main`:

```rust
fn main() {
    // Load `.env` into the process environment. In production there is usually
    // no `.env` file — the platform injects real variables — so a missing file
    // is not an error.
    match dotenvy::dotenv() {
        Ok(path) => println!("loaded env from {}", path.display()),
        Err(e) if e.not_found() => println!("no .env file, using real environment"),
        Err(e) => {
            eprintln!("failed to read .env: {e}");
            std::process::exit(1);
        }
    }

    let db = std::env::var("DATABASE_URL").unwrap_or_else(|_| "<unset>".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "<unset>".into());
    println!("DATABASE_URL={db} PORT={port}");
}
```

With the `.env` above present, this prints:

```text
loaded env from /path/to/project/.env
DATABASE_URL=postgres://localhost/myapp PORT=5432
```

Two important behaviors:

- **`dotenvy::dotenv()` never overrides variables that are already set.** A real environment variable always wins over a `.env` entry. This is correct: your container's injected `DATABASE_URL` should beat whatever is in a stray `.env` file.
- **A missing `.env` file returns `Err`, and `e.not_found()` lets you treat that as benign.** This is the key to making the *same* code path work in development (file present) and production (no file). Only a genuine I/O or parse error should abort startup.

> **Tip:** Commit a `.env.example` with placeholder values to document required variables, and add `.env` to your `.gitignore`. Never commit real secrets. This mirrors the convention every 12-factor Node project already uses.

### Parsing strings into real types

Environment values are always strings. The `parsed` helper above uses the `FromStr` trait — the same machinery behind `"3000".parse::<u16>()` — to turn `"3000"` into a real `u16`, and to *reject* `"70000"` (out of `u16` range) or `"abc"` (not a number) with a descriptive error instead of a silent `NaN`. Compare this to the JavaScript footgun where `Number(process.env.PORT)` yields `0` for an empty string and `NaN` for garbage, both of which sail past the type checker.

### Failing fast at startup

`Config::from_env()` is called once in `main`, before the server binds a port or opens a connection pool. If any required variable is missing or malformed, the process prints the error and calls `std::process::exit(1)`. The binary either starts fully configured or not at all. There is no half-configured intermediate state to debug at 3 a.m.

---

## Key Differences

| Concern | TypeScript / Node.js | Rust |
| --- | --- | --- |
| Reading a variable | `process.env.KEY` → `string \| undefined` | `env::var("KEY")` → `Result<String, VarError>` |
| Missing variable | `undefined`, silently flows on | `Err(VarError::NotPresent)`, must be handled |
| Non-UTF-8 value | invisible / coerced | distinct `VarError::NotUnicode` variant |
| Parsing to a number | `Number(x)` → `0`/`NaN` on bad input | `x.parse::<u16>()` → `Err` with a reason |
| `.env` loading | `dotenv` / `dotenv/config` | `dotenvy` crate |
| Validation | opt-in (`zod`, `joi`, manual) | the type system + your `from_env` |
| When errors surface | often at first use, mid-request | once, at process startup |
| Secrets in `Debug`/logs | leak unless you redact manually | leak unless you redact; but you can enforce it with a newtype |

The deepest difference is *when* you find out something is wrong. A Node service with an unvalidated `process.env.STRIPE_KEY` boots happily and fails on the first payment. The Rust pattern collapses that gap: a missing key is a startup failure, surfaced before any traffic arrives.

> **Note:** Unlike TypeScript, Rust does not validate the environment for you just because you declared a typed struct. The struct's *fields* are typed, but the *bridge* from `String` env values into those fields is code you write: `from_env`. The payoff is that once that bridge runs successfully, the rest of your program manipulates real `u16`s and validated `String`s, never raw `Option<String>`.

---

## Common Pitfalls

### Pitfall 1: Mutating the environment without `unsafe`

On the current stable toolchain (2024 edition), `std::env::set_var` and `remove_var` are `unsafe` functions, because changing the environment is not thread-safe: another thread could be reading it. Writing the obvious code fails to compile:

```rust
fn main() {
    std::env::set_var("KEY", "value"); // does not compile (error[E0133])
}
```

The real compiler error is:

```text
error[E0133]: call to unsafe function `set_var` is unsafe and requires unsafe block
 --> src/main.rs:2:5
  |
2 |     std::env::set_var("KEY", "value");
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
  |
  = note: consult the function's documentation for information on how to avoid undefined behavior
```

The lesson: **don't mutate the environment at runtime.** Read it once into your `Config` at startup and pass that struct around. If you genuinely must set a variable (for example in a test, before any threads spawn), wrap it in an `unsafe` block, but treat that as a smell, not a habit. `dotenvy::dotenv()` does the setting for you, before your threads exist, which is why it is safe to use.

### Pitfall 2: Reaching for `unwrap()` on `env::var`

It is tempting to write `env::var("DATABASE_URL").unwrap()`. This compiles, but at runtime a missing variable produces a panic with a stack trace instead of a readable message:

```text
thread 'main' panicked at src/main.rs:3:42:
called `Result::unwrap()` on an `Err` value: NotPresent
```

`NotPresent` tells you *nothing* about which variable was missing; you'd have to read the line number. Use a helper like `required()` that names the key, or `expect("DATABASE_URL must be set")` at the very least. For configuration, a structured error and `process::exit(1)` is far better than a panic.

### Pitfall 3: Assuming `.env` overrides the real environment

Developers sometimes set `PORT=8080` in `.env`, then export `PORT=3000` in their shell, and are surprised the app uses `3000`. That is correct and intentional: `dotenvy::dotenv()` only fills in variables that are *not already set*. If you truly want the file to win (rarely a good idea), `dotenvy` offers `dotenv_override()`, but in production the platform-injected variable should always take precedence.

### Pitfall 4: Logging the whole config, secrets included

`println!("{config:?}")` on a struct containing a `jwt_secret: String` will happily print your secret to stdout, where it lands in your log aggregator forever. Derive `Debug` only on configs without secrets, or wrap secret fields in a newtype with a redacting `Debug` impl (shown in the real-world example below). This is the same hazard as `console.log(config)` in Node, but Rust gives you a clean way to make leaks impossible by construction.

---

## Best Practices

- **Read the environment exactly once, at startup**, into a typed `Config`. Pass that struct (often inside an `Arc`) to the rest of the app; never sprinkle `env::var` calls throughout your code.
- **Fail fast and loudly.** A missing required variable should print a clear message and exit non-zero, so orchestrators restart and surface the problem immediately.
- **Distinguish required from optional.** Required variables have no default and abort on absence; optional ones get a sensible default via `unwrap_or_else` or a default in your parse helper.
- **Use `dotenvy` for development only.** Load it at the top of `main`, tolerate a missing file with `e.not_found()`, and let the platform inject real variables in production.
- **Validate values, not just presence.** A `PORT` of `"70000"` is "present" but invalid; parse into `u16` so the range check is free.
- **Keep secrets out of `Debug` output** with a redacting newtype.
- **Prefer one prefix for your app's variables** (e.g. `APP_DATABASE_URL`) so they don't collide with system or library variables. The `envy` crate (below) makes this ergonomic.
- **Document every variable in `.env.example`** and `.gitignore` the real `.env`.

> **Tip:** For deserializing the whole environment into a struct in one call — much like `zod`'s `safeParse(process.env)` — the `envy` crate maps environment variables onto a `serde`-derived struct. Add `serde = { version = "1", features = ["derive"] }` and `envy = "0.4"`, then:
>
> ```rust
> use serde::Deserialize;
>
> #[derive(Debug, Deserialize)]
> struct Config {
>     database_url: String,
>     #[serde(default = "default_port")]
>     port: u16,
>     #[serde(default = "default_log_level")]
>     log_level: String,
> }
>
> fn default_port() -> u16 { 8080 }
> fn default_log_level() -> String { "info".into() }
>
> fn main() {
>     match envy::prefixed("APP_").from_env::<Config>() {
>         Ok(config) => println!("{config:?}"),
>         Err(e) => {
>             eprintln!("configuration error: {e}");
>             std::process::exit(1);
>         }
>     }
> }
> ```
>
> With `APP_DATABASE_URL` and `APP_PORT` set, this prints
> `Config { database_url: "postgres://localhost/app", port: 3000, log_level: "info" }`.
> With `APP_DATABASE_URL` unset, `envy` returns the error
> `missing value for field database_url` and the process exits 1. This is the closest analog to the `zod` approach: concise, but it stops at the *first* error. The next example shows how to collect *all* of them.

---

## Real-World Example

A production service should do three things on boot: load `.env` in dev, validate **every** required variable (reporting all problems at once, not one at a time), and keep secrets out of its logs. Here is a self-contained version that does all three. It uses `thiserror` for the aggregate error and a `Secret` newtype whose `Debug` impl redacts the value.

```toml
# Cargo.toml
[dependencies]
dotenvy = "0.15"
thiserror = "2"
```

```rust
use std::env;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid configuration:\n{}", .0.join("\n"))]
struct ConfigErrors(Vec<String>);

/// A string that never prints its contents, so secrets stay out of logs.
struct Secret(String);

impl Secret {
    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("\"***REDACTED***\"")
    }
}

#[derive(Debug)]
enum AppEnv {
    Development,
    Staging,
    Production,
}

#[derive(Debug)]
struct Config {
    app_env: AppEnv,
    database_url: Secret,
    jwt_secret: Secret,
    port: u16,
}

/// Accumulates problems so we can report every misconfigured variable at once.
struct Builder {
    errors: Vec<String>,
}

impl Builder {
    fn new() -> Self {
        Self { errors: Vec::new() }
    }

    fn required(&mut self, key: &str) -> Option<String> {
        match env::var(key) {
            Ok(v) if !v.trim().is_empty() => Some(v),
            _ => {
                self.errors
                    .push(format!("  - {key} is required and must be non-empty"));
                None
            }
        }
    }

    fn parsed<T: std::str::FromStr>(&mut self, key: &str, default: T) -> Option<T>
    where
        T::Err: fmt::Display,
    {
        match env::var(key) {
            Ok(raw) => match raw.parse::<T>() {
                Ok(v) => Some(v),
                Err(e) => {
                    self.errors.push(format!("  - {key} is invalid: {e}"));
                    None
                }
            },
            Err(_) => Some(default),
        }
    }
}

impl Config {
    fn from_env() -> Result<Self, ConfigErrors> {
        let mut b = Builder::new();

        let app_env = match env::var("APP_ENV").as_deref() {
            Ok("production") => AppEnv::Production,
            Ok("staging") => AppEnv::Staging,
            Ok("development") | Err(_) => AppEnv::Development,
            Ok(other) => {
                b.errors.push(format!(
                    "  - APP_ENV `{other}` is not one of development|staging|production"
                ));
                AppEnv::Development
            }
        };

        let database_url = b.required("DATABASE_URL");
        let jwt_secret = b.required("JWT_SECRET");
        let port = b.parsed::<u16>("PORT", 8080);

        if !b.errors.is_empty() {
            return Err(ConfigErrors(b.errors));
        }

        Ok(Config {
            app_env,
            database_url: Secret(database_url.unwrap()),
            jwt_secret: Secret(jwt_secret.unwrap()),
            port: port.unwrap(),
        })
    }
}

fn main() {
    // Development convenience: load `.env` if present, ignore if absent.
    if let Err(e) = dotenvy::dotenv() {
        if !e.not_found() {
            eprintln!("failed to read .env: {e}");
            std::process::exit(1);
        }
    }

    let config = Config::from_env().unwrap_or_else(|e| {
        eprintln!("{e}");
        eprintln!("\nrefusing to start with invalid configuration");
        std::process::exit(1);
    });

    // Safe to log: the `Secret` Debug impl redacts the values.
    println!("starting service with {config:?}");

    // Real code would hand `config.database_url.expose()` to the connection
    // pool and `config.jwt_secret.expose()` to the auth layer here.
    let _ = config.database_url.expose();
}
```

A fully-configured run logs the config with secrets hidden:

```text
starting service with Config { app_env: Production, database_url: "***REDACTED***", jwt_secret: "***REDACTED***", port: 8443 }
```

A misconfigured run — `DATABASE_URL` unset, `JWT_SECRET` empty, `APP_ENV=prod` (a typo), `PORT=70000` (out of `u16` range) — reports **all four problems at once** and exits non-zero:

```text
invalid configuration:
  - APP_ENV `prod` is not one of development|staging|production
  - DATABASE_URL is required and must be non-empty
  - JWT_SECRET is required and must be non-empty
  - PORT is invalid: number too large to fit in target type

refusing to start with invalid configuration
```

Notice `number too large to fit in target type`: that is the real `<u16 as FromStr>::Err` message, not a fabricated one. A developer fixing the deployment sees every issue in one pass instead of redeploying four times.

> **Note:** Collecting all errors (the `Builder` pattern here) versus stopping at the first (`?` propagation, or `envy`) is a genuine design choice. Aggregating is friendlier for human-facing startup config, where you want one trip to fix everything; short-circuiting is fine when failures are rare or independent.

---

## Further Reading

- [The Twelve-Factor App — Config](https://12factor.net/config): the canonical statement of "store config in the environment".
- [`std::env::var` — Rust standard library](https://doc.rust-lang.org/std/env/fn.var.html) and the [`VarError`](https://doc.rust-lang.org/std/env/enum.VarError.html) enum.
- [`dotenvy` on docs.rs](https://docs.rs/dotenvy): the maintained `.env` loader.
- [`envy` on docs.rs](https://docs.rs/envy): deserialize the environment into a `serde` struct.
- [`thiserror` on docs.rs](https://docs.rs/thiserror): ergonomic custom error types, used for the aggregate error above.
- Related guide sections:
  - [Application Configuration](/28-production/00-configuration/) — layered config and typed settings with the `config`/`figment` crates (the next step up from raw env vars).
  - [Health and Readiness Endpoints](/28-production/03-health-checks/): turning "is my config valid and are my dependencies reachable" into a readiness endpoint.
  - [Graceful Shutdown](/28-production/02-graceful-shutdown/) — the other half of a clean process lifecycle.
  - [Production Readiness Checklist](/28-production/09-production-checklist/): where environment validation sits in the broader readiness picture.
  - [Error Handling](/08-error-handling/) — the `Result`, `?`, and custom-error machinery this page relies on.
  - [Serialization](/15-serialization/): `serde`, behind the `envy` approach.
  - [Variables and Mutability](/02-basics/00-variables/) and [Understanding Cargo](/01-getting-started/03-cargo-basics/) — refreshers on the fundamentals used here.
  - [Migration Guide](/29-migration-guide/): porting a Node service (including its `dotenv`/`zod` config layer) to Rust.

---

## Exercises

### Exercise 1: A required variable with a friendly error

**Difficulty:** Beginner

**Objective:** Practice reading a required environment variable and reporting a clear, named error instead of panicking.

**Instructions:** Write a function `fn api_key() -> Result<String, String>` that reads the `API_KEY` environment variable. On success return the value; if it is missing, return `Err("API_KEY is required".to_string())`. In `main`, call it and either print `Using key: <key>` or print the error and exit with code `1`. Verify that running without `API_KEY` set exits non-zero and prints the message.

<details>
<summary>Solution</summary>

```rust
use std::env;

fn api_key() -> Result<String, String> {
    env::var("API_KEY").map_err(|_| "API_KEY is required".to_string())
}

fn main() {
    match api_key() {
        Ok(key) => println!("Using key: {key}"),
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(1);
        }
    }
}
```

Running with `API_KEY=abc123 cargo run` prints `Using key: abc123`. Running with `API_KEY` unset prints `configuration error: API_KEY is required` and exits with status `1`.

</details>

### Exercise 2: An optional, validated numeric variable

**Difficulty:** Intermediate

**Objective:** Provide a default for an optional variable while still rejecting malformed values.

**Instructions:** Write `fn max_connections() -> Result<u32, String>` that reads `MAX_CONNECTIONS`. If the variable is unset, return `Ok(10)`. If it is set but does not parse as a `u32`, return an `Err` containing both the key name and the parse error. Test three cases: unset (returns `10`), `MAX_CONNECTIONS=50` (returns `50`), and `MAX_CONNECTIONS=lots` (returns an error).

<details>
<summary>Solution</summary>

```rust
use std::env;

fn max_connections() -> Result<u32, String> {
    match env::var("MAX_CONNECTIONS") {
        Ok(raw) => raw
            .parse::<u32>()
            .map_err(|e| format!("MAX_CONNECTIONS is invalid: {e}")),
        Err(_) => Ok(10),
    }
}

fn main() {
    match max_connections() {
        Ok(n) => println!("max connections: {n}"),
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(1);
        }
    }
}
```

- Unset: prints `max connections: 10`.
- `MAX_CONNECTIONS=50`: prints `max connections: 50`.
- `MAX_CONNECTIONS=lots`: prints `configuration error: MAX_CONNECTIONS is invalid: invalid digit found in string` and exits `1`. (`invalid digit found in string` is the real `<u32 as FromStr>::Err` message.)

</details>

### Exercise 3: Validate-everything config with `.env` support

**Difficulty:** Advanced

**Objective:** Build a small typed `Config` that loads `.env` in development and reports *all* configuration problems in a single startup pass.

**Instructions:** Using `dotenvy = "0.15"`, write a `Config` struct with fields `database_url: String`, `port: u16`, and `workers: u32`. Implement `Config::from_env()` that loads `.env` (tolerating a missing file via `e.not_found()`), then validates: `DATABASE_URL` is required and non-empty; `PORT` defaults to `8080` but must parse as `u16`; `WORKERS` defaults to `4` but must parse as `u32`. Collect *all* errors into a `Vec<String>` and return them together rather than stopping at the first. In `main`, print the config on success or all errors and exit `1` on failure.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
dotenvy = "0.15"
```

```rust
use std::env;

#[derive(Debug)]
struct Config {
    database_url: String,
    port: u16,
    workers: u32,
}

impl Config {
    fn from_env() -> Result<Self, Vec<String>> {
        // Load `.env` in development; a missing file is fine.
        if let Err(e) = dotenvy::dotenv() {
            if !e.not_found() {
                return Err(vec![format!("failed to read .env: {e}")]);
            }
        }

        let mut errors = Vec::new();

        let database_url = match env::var("DATABASE_URL") {
            Ok(v) if !v.trim().is_empty() => Some(v),
            _ => {
                errors.push("DATABASE_URL is required and must be non-empty".to_string());
                None
            }
        };

        let port = match env::var("PORT") {
            Ok(raw) => match raw.parse::<u16>() {
                Ok(p) => Some(p),
                Err(e) => {
                    errors.push(format!("PORT is invalid: {e}"));
                    None
                }
            },
            Err(_) => Some(8080),
        };

        let workers = match env::var("WORKERS") {
            Ok(raw) => match raw.parse::<u32>() {
                Ok(w) => Some(w),
                Err(e) => {
                    errors.push(format!("WORKERS is invalid: {e}"));
                    None
                }
            },
            Err(_) => Some(4),
        };

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(Config {
            database_url: database_url.unwrap(),
            port: port.unwrap(),
            workers: workers.unwrap(),
        })
    }
}

fn main() {
    match Config::from_env() {
        Ok(config) => println!("loaded: {config:?}"),
        Err(errors) => {
            eprintln!("invalid configuration:");
            for e in &errors {
                eprintln!("  - {e}");
            }
            std::process::exit(1);
        }
    }
}
```

With `DATABASE_URL=postgres://db PORT=9000 WORKERS=8 cargo run` it prints
`loaded: Config { database_url: "postgres://db", port: 9000, workers: 8 }`.

With `DATABASE_URL` unset and `PORT=99999 WORKERS=many` it reports all three problems at once:

```text
invalid configuration:
  - DATABASE_URL is required and must be non-empty
  - PORT is invalid: number too large to fit in target type
  - WORKERS is invalid: invalid digit found in string
```

and exits with status `1`.

</details>
