---
title: "Popular Crates and the npm Packages They Replace"
description: "Map the npm packages you know to their Rust crates: serde for JSON, tokio for async, clap, reqwest, anyhow and thiserror — what to install and why on day one."
---

## Quick Overview

A **crate** is Rust's unit of distribution (the equivalent of an npm package), and [crates.io](https://crates.io) is the registry, the way npm is for Node. Unlike Node, where the standard library is large and `node_modules` fills the gaps, Rust ships a deliberately small standard library and leans on a tight set of community crates that have become near-universal: **serde** for JSON and serialization, **tokio** for async, **clap** for command-line parsing, **reqwest** for HTTP requests, and **anyhow**/**thiserror** for error handling. This page maps the npm packages you already know onto their Rust counterparts so you can reach for the right crate on day one.

> **Note:** This page is the "what to install and why" overview. The deeper mechanics live in dedicated pages: web frameworks in [Web Frameworks](/23-ecosystem/01-web-frameworks/), async runtimes in [Async Runtimes](/23-ecosystem/02-async-runtimes/), HTTP clients in [HTTP Clients](/23-ecosystem/06-http-clients/), date/time in [Date and Time](/23-ecosystem/07-date-time/), regex in [Regular Expressions with the `regex` Crate](/23-ecosystem/08-regex/), and a grab-bag of other essentials in [Other Essential Crates](/23-ecosystem/10-useful-crates/).

---

## TypeScript/JavaScript Example

A typical Node service's `package.json` is a roll-call of small, focused libraries. Each line below is a dependency a working TypeScript developer reaches for without thinking:

```jsonc
// package.json (excerpt) — the everyday Node toolbox.
{
  "dependencies": {
    "express": "^4.19.2",      // web server / routing
    "axios": "^1.7.2",         // HTTP client
    "zod": "^3.23.8",          // runtime validation of JSON shapes
    "commander": "^12.1.0",    // CLI argument parsing
    "winston": "^3.13.0",      // logging
    "dotenv": "^16.4.5",       // load .env into process.env
    "uuid": "^10.0.0",         // generate UUIDs
    "date-fns": "^3.6.0"       // date manipulation
  }
}
```

And the code that uses them is dense with implicit conversions. `JSON.parse` hands you an `any`, and you bolt on `zod` to recover the types you thought you had:

```typescript
// service.ts — parse some JSON, validate it, log, serve.
import { z } from "zod";

const ConfigSchema = z.object({
  name: z.string(),
  port: z.number(),
  verbose: z.boolean().default(false),
});

type Config = z.infer<typeof ConfigSchema>;

function loadConfig(raw: string): Config {
  // JSON.parse returns `any`; without zod the shape is unchecked.
  const parsed = JSON.parse(raw);
  return ConfigSchema.parse(parsed); // throws ZodError if wrong
}

const config = loadConfig('{ "name": "api", "port": 8080 }');
console.log(config); // { name: 'api', port: 8080, verbose: false }
```

Three things to notice, because they shape what Rust does differently:

- **Validation is a separate, runtime step.** `JSON.parse` does not know your types; `zod` re-checks the shape at runtime because TypeScript types are erased after compilation.
- **The toolbox is many small packages.** Each does one thing, and they do not coordinate.
- **Errors are thrown.** `ConfigSchema.parse` throws; you catch with `try`/`catch` somewhere up the stack, and the type system does not force you to.

---

## Rust Equivalent

In Rust, the same job is done by a smaller, more standardized set of crates, and parsing-with-validation collapses into one step. You add dependencies with `cargo add` (built into Cargo since 1.62 — no `cargo-edit` needed) in a project created by `cargo new`, which selects the newest edition automatically. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition.

```toml
# Cargo.toml — the everyday Rust toolbox. Run:
#   cargo add serde --features derive
#   cargo add serde_json tokio anyhow thiserror clap reqwest
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "2"
```

Here is the direct counterpart to the `loadConfig` example. The `#[derive(Deserialize)]` line is what replaces `zod`: the shape and its validation are generated at compile time from the struct definition itself.

```rust playground
// Cargo.toml: serde = { version = "1", features = ["derive"] }, serde_json = "1", anyhow = "1"
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    name: String,
    port: u16,
    #[serde(default)] // missing in JSON -> bool::default() == false
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    let json = r#"{ "name": "api", "port": 8080 }"#;

    // Parse JSON text directly into a typed Config. If the shape is wrong
    // (missing "name", "port" not a number), this returns an Err — no
    // separate validation library needed.
    let config: Config = serde_json::from_str(json)?;
    println!("parsed: {config:?}");

    // Serialize back to pretty JSON.
    let out = serde_json::to_string_pretty(&config)?;
    println!("{out}");

    Ok(())
}
```

Running it prints real, typed output:

```text
parsed: Config { name: "api", port: 8080, verbose: false }
{
  "name": "api",
  "port": 8080,
  "verbose": false
}
```

Notice that `serde_json::from_str` already gave you a fully typed, validated `Config`. There is no `any` stage and no second library: `serde` is `JSON.parse` and `zod` fused into one, checked at compile time.

---

## Detailed Explanation

### The crate ↔ npm package map

The fastest way to get oriented is a translation table. Reach for the Rust column when you would have reached for the npm column.

| Job | npm package(s) | Rust crate | Notes |
| --- | --- | --- | --- |
| JSON / serialization | `JSON` built-in, `zod`, `class-transformer` | **serde** + `serde_json` | Derive-based; compile-time, no `any`. |
| Async runtime | built into Node (libuv) | **tokio** | You install and start it — see [Async Runtimes](/23-ecosystem/02-async-runtimes/). |
| HTTP client | `axios`, `node-fetch`, `got` | **reqwest** | See [HTTP Clients](/23-ecosystem/06-http-clients/). |
| Web framework | `express`, `fastify`, `koa` | **axum**, `actix-web` | See [Web Frameworks](/23-ecosystem/01-web-frameworks/). |
| CLI args | `commander`, `yargs`, `minimist` | **clap** | Derive structs straight from `--flags`. |
| Error context | `Error`, custom error classes | **anyhow** (apps), **thiserror** (libraries) | Two crates, two jobs; see below. |
| Logging | `winston`, `pino`, `debug` | **log** + `env_logger`, **tracing** | See [Logging with the `log` Facade and `env_logger`](/23-ecosystem/03-logging/), [Structured Logging and Spans with `tracing`](/23-ecosystem/04-tracing/). |
| Date / time | `date-fns`, `dayjs`, `luxon`, `Temporal` | **chrono**, **time**, **jiff** | jiff mirrors the Temporal API — see [Date and Time](/23-ecosystem/07-date-time/). |
| Regex | built-in `RegExp` | **regex** | Linear-time, no catastrophic backtracking — see [Regular Expressions with the `regex` Crate](/23-ecosystem/08-regex/). |
| UUID | `uuid` | **uuid** | See [Other Essential Crates](/23-ecosystem/10-useful-crates/). |
| `.env` files | `dotenv` | **dotenvy** | Maintained fork of the original `dotenv` crate. |
| Random | `Math.random`, `crypto` | **rand** | Use `rand::rng()` / `random()` (rand 0.9+). |
| Parallel data | `worker_threads` | **rayon** | Data-parallel iterators — see [Other Essential Crates](/23-ecosystem/10-useful-crates/). |

### serde: the one you will use in almost every project

**serde** (SERialize/DEserialize) is the most-downloaded crate on crates.io and the closest thing Rust has to a universal dependency. It is a *framework*: `serde` defines the `Serialize`/`Deserialize` traits, and format crates like `serde_json`, `serde_yaml`, `toml`, `bincode`, and `rmp-serde` (MessagePack) plug into it. You derive the traits once and get every format for free.

```rust
// One derive, many formats. Switching from JSON to TOML is a one-line change.
#[derive(serde::Serialize, serde::Deserialize)]
struct Point { x: i32, y: i32 }
```

The `derive` feature (enabled by `cargo add serde --features derive`) is what generates the implementation from your struct. This is the analogue of `zod`'s schema, except it is code generation at compile time, so there is zero runtime reflection and the JSON shape is guaranteed to match your type or fail to parse.

### tokio: the async runtime you bring yourself

In Node the event loop is the platform: it is always running and you never install it. In Rust, `async`/`await` is only syntax; an `async fn` is inert until an **executor** polls it. **tokio** is that executor, plus timers, TCP/UDP, channels, and synchronization primitives. Roughly the entire async ecosystem (reqwest, axum, sqlx, tonic) builds on it, which is why it is the default.

```rust
// cargo add tokio --features full
#[tokio::main] // starts the runtime, then runs your async main
async fn main() {
    println!("running on tokio");
}
```

> **Note:** Rust futures are **lazy** — the opposite of JavaScript Promises. A `Promise` starts executing the moment it is created; a Rust future does nothing until awaited or spawned onto the runtime. This is covered in depth in [Async Runtimes](/23-ecosystem/02-async-runtimes/) and section 11.

### clap: CLI parsing as a struct

**clap** replaces `commander`/`yargs`. The idiomatic style is `derive`: you describe your arguments as a struct and clap generates the parser, the `--help` text, validation, and error messages.

```rust
// cargo add clap --features derive
use clap::Parser;

/// Greet a user a number of times
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Name of the person to greet
    #[arg(short, long)]
    name: String,

    /// Number of times to greet
    #[arg(short, long, default_value_t = 1)]
    count: u8,
}

fn main() {
    let args = Args::parse();
    for _ in 0..args.count {
        println!("Hello, {}!", args.name);
    }
}
```

That single struct produces a complete CLI. Running it:

```text
$ cargo run -- --name Ada --count 2
Hello, Ada!
Hello, Ada!

$ cargo run -- --help
Greet a user a number of times

Usage: probe [OPTIONS] --name <NAME>

Options:
  -n, --name <NAME>    Name of the person to greet
  -c, --count <COUNT>  Number of times to greet [default: 1]
  -h, --help           Print help
  -V, --version        Print version

$ cargo run --          # missing the required --name
error: the following required arguments were not provided:
  --name <NAME>

Usage: probe --name <NAME>

For more information, try '--help'.
```

The `--help` page, the `[default: 1]` annotation, and the "required argument" error were all generated; you wrote none of that text. Section 18 ([CLI Tools](/18-cli-tools/)) covers clap in full.

### reqwest: the HTTP client

**reqwest** is `axios`/`fetch` for Rust: an async, high-level HTTP client built on `hyper`. The key idiom — and a real performance difference from naïve `fetch` use — is to build one `Client` and reuse it, because it holds a connection pool. See [HTTP Clients](/23-ecosystem/06-http-clients/) for the full treatment.

### anyhow vs thiserror: two crates, two audiences

This pairing has no single npm equivalent, so it surprises Node developers. Rust splits error handling into two crates by *who consumes the error*:

- **thiserror**, for **libraries**. It derives the `std::error::Error` trait for your own typed enum, so callers can `match` on specific variants. Use it when the *caller* needs to react differently to different failures.
- **anyhow** — for **applications**. Its `anyhow::Error` is a single boxed type that can hold *any* error, with cheap `.context("...")` annotations and an automatic backtrace. Use it in `main` and binaries where you just want to bubble failures up with a readable message.

```rust playground
// cargo add anyhow thiserror serde --features derive ; cargo add serde_json
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Deserialize)]
struct Settings {
    database_url: String,
    max_connections: u32,
}

// A LIBRARY-style typed error: callers can match on each variant.
#[derive(Debug, Error)]
enum LoadError {
    #[error("config file is empty")]
    Empty,
    #[error("missing required key: {0}")]
    MissingKey(String),
}

fn parse_kv(raw: &str) -> Result<Settings, LoadError> {
    if raw.trim().is_empty() {
        return Err(LoadError::Empty);
    }
    let mut map = HashMap::new();
    for line in raw.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim(), v.trim());
        }
    }
    let database_url = map
        .get("database_url")
        .ok_or_else(|| LoadError::MissingKey("database_url".into()))?
        .to_string();
    let max_connections = map
        .get("max_connections")
        .ok_or_else(|| LoadError::MissingKey("max_connections".into()))?
        .parse()
        .map_err(|_| LoadError::MissingKey("max_connections".into()))?;
    Ok(Settings { database_url, max_connections })
}

// An APPLICATION-style boundary: many error types collapse into anyhow::Error,
// and `.context` adds a human-readable breadcrumb.
fn load_app() -> Result<Settings> {
    let raw = "database_url = postgres://localhost/app\nmax_connections = 16";
    let settings = parse_kv(raw).context("failed to parse settings")?;
    Ok(settings)
}

fn main() -> Result<()> {
    let settings = load_app()?;
    println!("db = {}, pool = {}", settings.database_url, settings.max_connections);
    Ok(())
}
```

Output:

```text
db = postgres://localhost/app, pool = 16
```

The `?` operator (covered in [section 08](/08-error-handling/)) is what makes this ergonomic: it propagates an error upward, converting it to the function's return error type along the way. With `anyhow` that target type accepts anything; with `thiserror` you control exactly which variants exist.

---

## Key Differences

| Concept | TypeScript / Node | Rust |
| --- | --- | --- |
| Registry | npm / `package.json` | crates.io / `Cargo.toml` |
| Install command | `npm install x` | `cargo add x` (no extra tool needed) |
| Standard library size | large; many built-ins | small; community crates fill gaps |
| JSON parsing | `JSON.parse` returns `any` | `serde_json::from_str` returns a typed value |
| Runtime validation | needs `zod`/`io-ts` (types are erased) | the type *is* the schema (derived at compile time) |
| Async runtime | built in, always running | a crate you add and start (tokio) |
| Versioning | `^1.2.3` caret by default | `"1.2.3"` is *also* caret by default (SemVer) |
| Lockfile | `package-lock.json` | `Cargo.lock` |
| Transitive duplicate versions | hoisted / deduped, sometimes both | Cargo allows multiple major versions side by side |

### "1.2.3" in Cargo.toml is not an exact version

A frequent misread for Node developers: in `Cargo.toml`, writing `serde = "1.2.3"` is a **caret** requirement (`>=1.2.3, <2.0.0`), exactly like npm's `^1.2.3`. It is *not* pinned. To pin an exact version you must write `serde = "=1.2.3"`. Day-to-day you should depend on the major version only — `serde = "1"` — and let `Cargo.lock` record the exact resolved version.

### The ecosystem is more centralized

Node has many overlapping options for every job (a dozen HTTP clients, several test runners). Rust tends to converge on one or two near-canonical crates per job — serde, tokio, clap, reqwest. This means less decision fatigue, but also that picking the off-canonical crate (say, a non-tokio async runtime) can cut you off from a large chunk of the ecosystem, because libraries assume the default.

---

## Common Pitfalls

### Forgetting the `derive` feature on serde

`#[derive(Serialize)]` lives behind serde's `derive` feature flag, which is *off* by default. If you `cargo add serde` without it and then derive, the compiler cannot find the macro:

```rust playground
// does not compile — serde added WITHOUT the `derive` feature
use serde::Serialize;

#[derive(Serialize)]
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let _p = Point { x: 1, y: 2 };
}
```

The real error from `cargo build`:

```text
error: cannot find derive macro `Serialize` in this scope
 --> src/main.rs:3:10
  |
3 | #[derive(Serialize)]
  |          ^^^^^^^^^
  |
note: `Serialize` is imported here, but it is only a trait, without a derive macro
 --> src/main.rs:1:5
  |
1 | use serde::Serialize;
  |     ^^^^^^^^^^^^^^^^
```

**Fix:** `cargo add serde --features derive` (or add `features = ["derive"]` in `Cargo.toml`). The error message even tells you the trait was imported but the derive macro was missing — a strong hint that the feature is off.

### Writing `async fn main` without a runtime attribute

In Node `async function main()` just works. In Rust, `main` cannot be `async` on its own; there is no runtime polling it. You must annotate it (`#[tokio::main]`) so the macro starts a runtime and drives the future:

```rust
// does not compile (error E0752): main may not be async by itself
async fn fetch() -> u32 {
    42
}

async fn main() {
    let n = fetch().await;
    println!("{n}");
}
```

The real compiler error:

```text
error[E0752]: `main` function is not allowed to be `async`
 --> src/main.rs:5:1
  |
5 | async fn main() {
  | ^^^^^^^^^^^^^^^ `main` function is not allowed to be `async`
```

**Fix:** add `#[tokio::main]` above `async fn main()` (and `cargo add tokio --features full`). This is the most common first stumble for Node developers; see [Async Runtimes](/23-ecosystem/02-async-runtimes/).

### Reaching for `anyhow` inside a library

`anyhow::Error` erases the concrete error type. That is great in an application, but in a *library* it forces every caller to give up matching on specific failures. Libraries should expose a `thiserror`-derived enum so consumers can branch on `LoadError::MissingKey` versus `LoadError::Empty`. Mixing them up is not a compiler error; it is a design smell that frustrates your downstream users.

### Assuming JavaScript `number` semantics carry over

Node's `number` is always an IEEE-754 `f64`, so a large integer silently *loses precision* (it does not wrap). When you map a JSON integer into a Rust `u64` via serde, you get exact 64-bit semantics. If you instead deserialize into `f64` to "match JavaScript," you reintroduce the precision loss on purpose — pick the integer type the data actually warrants.

---

## Best Practices

- **Add crates with `cargo add`, depend on the major version.** Prefer `serde = "1"` over a pinned patch; let `Cargo.lock` record the exact resolved version. Commit `Cargo.lock` for binaries; it is optional for libraries.
- **Enable only the features you use.** Many crates gate functionality behind features (`serde`'s `derive`, `tokio`'s `full`, `reqwest`'s `json`). Smaller feature sets mean faster builds. `cargo add tokio --features rt-multi-thread,macros,net` is leaner than `--features full` once you know what you need.
- **Use `thiserror` for libraries, `anyhow` for binaries.** This is the single most useful error-handling convention in Rust. Reserve `anyhow` for the application boundary (your `main`, request handlers) where you only need a readable message and a backtrace.
- **Lean on the canonical crate.** For JSON it is serde, for async it is tokio, for CLIs it is clap. Going off-canonical is sometimes right, but understand the ecosystem cost first.
- **Audit your tree.** Run `cargo tree` to see transitive dependencies and `cargo audit` (from `cargo install cargo-audit`) to check for known vulnerabilities, the rough equivalent of `npm audit`. More on this in [Tooling](/24-tooling/).
- **Read the crate's docs on [docs.rs](https://docs.rs).** Every published crate gets auto-generated, versioned API docs. It is the Rust equivalent of a package's README plus full type documentation.

---

## Real-World Example

A small but production-flavored task that touches the core toolbox at once: fetch a record over HTTP, deserialize it into a typed struct, and surface failures cleanly. It uses **reqwest** (HTTP), **serde** (deserialize), **tokio** (runtime), and **anyhow** (error boundary) — the four crates you will see together constantly.

```rust
// cargo add reqwest --features json
// cargo add serde --features derive
// cargo add tokio --features full
// cargo add anyhow
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Todo {
    id: u32,
    title: String,
    completed: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build the Client once and reuse it — it owns a connection pool.
    let client = reqwest::Client::new();

    let todo: Todo = client
        .get("https://jsonplaceholder.typicode.com/todos/1")
        .send()
        .await?
        .error_for_status()? // turn a 4xx/5xx response into an Err
        .json() // deserialize the body straight into Todo via serde
        .await?;

    println!(
        "#{}: {} ({})",
        todo.id,
        todo.title,
        if todo.completed { "done" } else { "open" }
    );
    Ok(())
}
```

Real output against the live endpoint:

```text
#1: delectus aut autem (open)
```

The `.error_for_status()?` line is worth calling out: unlike `fetch`, which resolves successfully even on a 404, reqwest lets you convert a non-2xx status into an error that `?` propagates — turning an HTTP-level failure into a normal Rust `Result` failure. The `.json()` call deserializes directly into `Todo` because the struct derives `Deserialize`; there is no intermediate `any` and no separate validation step.

---

## Further Reading

- [crates.io](https://crates.io): the registry; search and browse crates.
- [docs.rs](https://docs.rs): auto-generated, versioned API docs for every published crate.
- [The Cargo Book — Specifying Dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html): how version requirements and features work.
- [serde.rs](https://serde.rs): the serde guide, including attributes like `#[serde(rename_all = ...)]`.
- [clap docs](https://docs.rs/clap) and [tokio.rs](https://tokio.rs): the official guides.
- Related guide pages: [Web Frameworks](/23-ecosystem/01-web-frameworks/), [Async Runtimes](/23-ecosystem/02-async-runtimes/), [HTTP Clients](/23-ecosystem/06-http-clients/), [Logging with the `log` Facade and `env_logger`](/23-ecosystem/03-logging/), [Structured Logging and Spans with `tracing`](/23-ecosystem/04-tracing/), [Date and Time](/23-ecosystem/07-date-time/), [Regular Expressions with the `regex` Crate](/23-ecosystem/08-regex/), [Parsing](/23-ecosystem/09-parsing/), [Other Essential Crates](/23-ecosystem/10-useful-crates/).
- Foundations: [Getting Started](/01-getting-started/) and [Cargo Basics](/01-getting-started/03-cargo-basics/); error handling in [section 08](/08-error-handling/); CLI tools in [section 18](/18-cli-tools/); tooling and auditing in [Tooling](/24-tooling/).

---

## Exercises

### Exercise 1: Serde field renaming and optional fields

**Difficulty:** Beginner

**Objective:** Use serde attributes to bridge a `camelCase` JSON API and an idiomatic `snake_case` Rust struct, and handle a field that may be absent.

**Instructions:** Given the JSON `{ "userId": 7, "fullName": "Grace Hopper" }`, define a `User` struct that deserializes it. The JSON uses `camelCase` but your Rust fields should be `snake_case` (`user_id`, `full_name`). Add an `email: Option<String>` field that is allowed to be missing on input and is *omitted* from the output when `None`. Print the parsed struct and then re-serialize it to JSON.

> **Tip:** Look at `#[serde(rename_all = "camelCase")]` for the struct and `#[serde(skip_serializing_if = "Option::is_none")]` for the field.

<details>
<summary>Solution</summary>

```rust playground
// cargo add serde --features derive ; cargo add serde_json ; cargo add anyhow
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    user_id: u64,
    full_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let json = r#"{ "userId": 7, "fullName": "Grace Hopper" }"#;
    let user: User = serde_json::from_str(json)?;
    println!("{user:?}");
    println!("{}", serde_json::to_string(&user)?);
    Ok(())
}
```

Output:

```text
User { user_id: 7, full_name: "Grace Hopper", email: None }
{"userId":7,"fullName":"Grace Hopper"}
```

`rename_all` handled the casing in both directions, and `skip_serializing_if` kept the absent `email` out of the output entirely.

</details>

### Exercise 2: A typed library error with thiserror

**Difficulty:** Intermediate

**Objective:** Build a `parse_port` function that returns a typed `thiserror` error, using `#[from]` to auto-convert a standard-library error.

**Instructions:** Write `fn parse_port(s: &str) -> Result<u16, PortError>`. It should parse the string to a number; if parsing fails, the error should wrap `std::num::ParseIntError` (use `#[from]` so the `?` operator converts it automatically). If the number is outside `1..=65535`, return an `OutOfRange` variant carrying the offending value. Drive it with the inputs `"8080"`, `"70000"`, and `"abc"` and print the result of each.

<details>
<summary>Solution</summary>

```rust playground
// cargo add thiserror
use std::num::ParseIntError;
use thiserror::Error;

#[derive(Debug, Error)]
enum PortError {
    #[error("port string was not a number")]
    NotANumber(#[from] ParseIntError),
    #[error("port {0} is outside the valid range 1..=65535")]
    OutOfRange(u32),
}

fn parse_port(s: &str) -> Result<u16, PortError> {
    let raw: u32 = s.parse()?; // ParseIntError -> PortError via #[from]
    if !(1..=65535).contains(&raw) {
        return Err(PortError::OutOfRange(raw));
    }
    Ok(raw as u16)
}

fn main() {
    for input in ["8080", "70000", "abc"] {
        match parse_port(input) {
            Ok(p) => println!("{input:>6} -> ok: {p}"),
            Err(e) => println!("{input:>6} -> err: {e}"),
        }
    }
}
```

Output:

```text
  8080 -> ok: 8080
 70000 -> err: port 70000 is outside the valid range 1..=65535
   abc -> err: port string was not a number
```

The `#[from]` attribute generated the `From<ParseIntError>` impl, so a bare `?` on `s.parse()` produced the correct `PortError` automatically.

</details>

### Exercise 3: A complete CLI that fetches JSON

**Difficulty:** Advanced

**Objective:** Combine clap, reqwest, serde, tokio, and anyhow into a single small tool — the real-world Rust toolbox working together.

**Instructions:** Build a CLI that takes an `--id` argument (defaulting to `1`), fetches `https://jsonplaceholder.typicode.com/todos/{id}`, deserializes the response into a `Todo { id, title, completed }`, and prints a line like `#1 [ ] delectus aut autem` (use `x` inside the brackets when completed). Use `#[tokio::main]`, an `anyhow::Result<()>` return type, and `.error_for_status()?` so a bad HTTP status becomes an error.

<details>
<summary>Solution</summary>

```rust
// cargo add clap --features derive
// cargo add reqwest --features json
// cargo add serde --features derive
// cargo add tokio --features full
// cargo add anyhow
use clap::Parser;
use serde::Deserialize;

#[derive(Parser)]
#[command(about = "Fetch a placeholder TODO by id")]
struct Args {
    /// The TODO id to fetch
    #[arg(short, long, default_value_t = 1)]
    id: u32,
}

#[derive(Debug, Deserialize)]
struct Todo {
    id: u32,
    title: String,
    completed: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let url = format!("https://jsonplaceholder.typicode.com/todos/{}", args.id);

    let client = reqwest::Client::new();
    let todo: Todo = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    println!(
        "#{} [{}] {}",
        todo.id,
        if todo.completed { "x" } else { " " },
        todo.title
    );
    Ok(())
}
```

Output:

```text
$ cargo run
#1 [ ] delectus aut autem

$ cargo run -- --id 5
#5 [ ] laboriosam mollitia et enim quasi adipisci quia provident illum
```

Five crates, one focused program: clap parsed the flag, reqwest fetched, serde deserialized, tokio ran the async work, and anyhow let every failure flow out of `main` through `?`.

</details>
