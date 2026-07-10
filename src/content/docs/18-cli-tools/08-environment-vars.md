---
title: "Environment Variables"
description: "Read environment config in Rust with std::env::var, dotenvy, and envy: where Node's string-or-undefined process.env becomes a typed Result you must handle."
---

## Quick Overview

Environment variables are how a process inherits configuration from its surroundings: database URLs, ports, API keys, feature flags, and `CI`/`NODE_ENV`-style switches. In Node you reach for `process.env` and often the `dotenv` package; in Rust you reach for `std::env::var` and, when you want a `.env` file or struct-shaped config, the **dotenvy** and **envy** crates. This page shows how to read, parse, and validate environment configuration the Rust way, and where the experience diverges sharply from `process.env`.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. The crate examples here use **dotenvy 0.15**, **envy 0.4**, and **serde 1.0**.

> **Note:** Reading the *first positional arguments* of your program (`std::env::args`) is a different topic covered under argument parsing. See [Argument Parsing with clap](/18-cli-tools/00-clap-basics/). This page is strictly about the process *environment* (`std::env::var` and friends).

---

## TypeScript/JavaScript Example

A typical Node service reads configuration from `process.env`, loads a `.env` file in development, and supplies defaults inline:

```typescript
// config.ts — run with: npx tsx config.ts
// Depends on: npm install dotenv
import "dotenv/config"; // loads .env into process.env as a side effect

interface Config {
  databaseUrl: string;
  port: number;
  logLevel: string;
  debug: boolean;
}

function loadConfig(): Config {
  const databaseUrl = process.env.DATABASE_URL;
  if (!databaseUrl) {
    throw new Error("DATABASE_URL is required");
  }

  return {
    databaseUrl,
    // process.env values are ALWAYS strings | undefined — you must parse.
    port: parseInt(process.env.PORT ?? "8080", 10),
    logLevel: process.env.LOG_LEVEL ?? "info",
    debug: process.env.DEBUG === "true" || process.env.DEBUG === "1",
  };
}

const config = loadConfig();
console.log(config);
```

```text
$ DATABASE_URL=postgres://localhost/app PORT=3000 DEBUG=1 npx tsx config.ts
{
  databaseUrl: 'postgres://localhost/app',
  port: 3000,
  logLevel: 'info',
  debug: true
}
```

Three things to internalize about Node's model, because Rust will challenge each one:

1. **Every value is `string | undefined`.** `process.env.PORT` is the string `"3000"`, never the number `3000`. `typeof process.env.PORT` prints `undefined` when unset and `"string"` otherwise. Parsing and validation are entirely on you, and a typo like `PORT=abc` silently becomes `NaN` after `parseInt`.
2. **Reading and writing `process.env` is trivial and global.** Anyone can do `process.env.X = "y"` from any module at any time.
3. **`dotenv` mutates `process.env` as an import side effect** and, by default, does not overwrite variables that are already set.

---

## Rust Equivalent

The same configuration loader in Rust, using `std::env::var` for reading and `dotenvy` to load a `.env` file in development:

```rust
// src/main.rs
// Depends on: cargo add dotenvy
use std::env;
use std::process;

#[derive(Debug)]
struct Config {
    database_url: String,
    port: u16,
    log_level: String,
    debug: bool,
}

fn load_config() -> Result<Config, String> {
    // Load .env into the process environment if it exists.
    // A missing file is fine in production where real env vars are set.
    let _ = dotenvy::dotenv();

    // env::var returns Result<String, VarError>, so a missing required
    // value is an honest error, not a silent `undefined`.
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required".to_string())?;

    // PORT must parse as a u16; we choose a default when it is absent.
    let port: u16 = match env::var("PORT") {
        Ok(s) => s
            .parse()
            .map_err(|_| format!("PORT='{s}' is not a valid port number"))?,
        Err(_) => 8080,
    };

    let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

    let debug = env::var("DEBUG")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    Ok(Config {
        database_url,
        port,
        log_level,
        debug,
    })
}

fn main() {
    let config = load_config().unwrap_or_else(|err| {
        eprintln!("configuration error: {err}");
        process::exit(1);
    });

    println!("{config:#?}");
}
```

```text
$ DATABASE_URL=postgres://localhost/app PORT=3000 DEBUG=1 cargo run --quiet
Config {
    database_url: "postgres://localhost/app",
    port: 3000,
    log_level: "info",
    debug: true,
}
```

The shape is familiar, but every read is a `Result` you must handle, and `PORT` is a real `u16`. If it does not parse, you find out at startup with a clear message rather than discovering `NaN` mid-request.

---

## Detailed Explanation

### `std::env::var` returns a `Result`, not a string-or-undefined

The single most important difference: `env::var(key)` has the signature

```rust
// from the standard library
pub fn var<K: AsRef<OsStr>>(key: K) -> Result<String, VarError>
```

There is no `undefined`. A missing variable is `Err(VarError::NotPresent)`; a present-but-non-UTF-8 value is `Err(VarError::NotUnicode(_))`. This forces you to decide, at the call site, what "missing" means: a default, an error, or `None`.

The idioms map cleanly onto the `?.`/`??` patterns you already know:

```rust playground
use std::env;

fn main() {
    // ?? "default"  ->  unwrap_or_else with a closure (avoids allocating when set)
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());

    // optional value  ->  .ok() converts Result into Option
    let log_level: Option<String> = env::var("LOG_LEVEL").ok();

    // "is the flag present at all?"  ->  .is_ok()
    let in_ci = env::var("CI").is_ok();

    // empty-string fallback  ->  unwrap_or_default()
    let extra = env::var("EXTRA_ARGS").unwrap_or_default();

    println!("port={port} log_level={log_level:?} in_ci={in_ci} extra={extra:?}");
}
```

```text
$ cargo run --quiet
port=8080 log_level=None in_ci=false extra=""

$ CI=1 EXTRA_ARGS=--fast cargo run --quiet
port=8080 log_level=None in_ci=true extra="--fast"
```

### Parsing numbers and booleans

Because `env::var` yields a `String`, parsing looks like any other Rust parse. A compact "default if unset *or* unparseable" reads as a chain:

```rust playground
use std::env;

fn main() {
    let port: u16 = env::var("PORT")
        .ok()                       // Result -> Option
        .and_then(|s| s.parse().ok()) // Option<String> -> Option<u16>
        .unwrap_or(8080);           // fall back

    let debug = env::var("DEBUG")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    println!("port={port} debug={debug}");
}
```

```text
$ cargo run --quiet
port=8080 debug=false

$ PORT=abc DEBUG=true cargo run --quiet
port=8080 debug=true
```

> **Tip:** The chain above *silently* falls back to `8080` when `PORT=abc`, mirroring Node's `parseInt(...) || 8080`. That is convenient but hides typos. For production config, prefer the explicit `match` from the Rust Equivalent section, which turns `PORT=abc` into a fatal, descriptive error. Silent fallbacks are how a `PORT=80800` typo lands you on the default port in production.

### Distinguishing the two failure modes

When you genuinely need to tell "not set" apart from "set to garbage bytes", match on `VarError`:

```rust playground
use std::env;

fn main() {
    match env::var("CONFIG_PATH") {
        Ok(value) => println!("CONFIG_PATH = {value}"),
        Err(env::VarError::NotPresent) => println!("CONFIG_PATH is not set"),
        Err(env::VarError::NotUnicode(raw)) => {
            println!("CONFIG_PATH contains non-UTF-8 bytes: {raw:?}");
        }
    }
}
```

```text
$ cargo run --quiet
CONFIG_PATH is not set

$ CONFIG_PATH=/etc/app.toml cargo run --quiet
CONFIG_PATH = /etc/app.toml
```

If you want the raw bytes regardless of encoding (paths on Linux/macOS can legally be non-UTF-8), use `env::var_os`, which returns `Option<OsString>` and never errors:

```rust playground
use std::env;

fn main() {
    // var_os never fails on encoding; it just gives you the raw OsString.
    let home = env::var_os("HOME");
    println!("HOME present: {}", home.is_some());
}
```

```text
$ cargo run --quiet
HOME present: true
```

### Loading a `.env` file with dotenvy

`dotenvy` is the maintained successor to the older `dotenv` crate. Add it and call `dotenvy::dotenv()` once, early in `main`:

```bash
cargo add dotenvy
```

Given a `.env` file in the working directory:

```bash
# .env
DATABASE_URL=postgres://localhost/myapp
PORT=4000
# comment lines are ignored
LOG_LEVEL=debug
```

```rust
// src/main.rs
use std::env;

fn main() {
    // Returns Ok(path) with the file it loaded, or Err if no .env was found.
    match dotenvy::dotenv() {
        Ok(path) => println!("loaded env from {}", path.display()),
        Err(e) => println!("no .env loaded: {e}"),
    }

    let url = env::var("DATABASE_URL").unwrap_or_else(|_| "<unset>".into());
    let port = env::var("PORT").unwrap_or_else(|_| "<unset>".into());
    println!("DATABASE_URL = {url}");
    println!("PORT         = {port}");
}
```

```text
$ cargo run --quiet
loaded env from /path/to/project/.env
DATABASE_URL = postgres://localhost/myapp
PORT         = 4000

$ PORT=9999 cargo run --quiet
loaded env from /path/to/project/.env
DATABASE_URL = postgres://localhost/myapp
PORT         = 9999
```

Notice the second run: the real environment variable `PORT=9999` **wins** over the `.env` value `4000`. Like Node's `dotenv`, `dotenvy::dotenv()` does **not** overwrite variables that are already present in the environment. That precedence is exactly what you want: real env vars (set by your shell, Docker, or systemd) override the development `.env` defaults.

> **Note:** `dotenvy::dotenv()` returns an `Err` when no `.env` file exists. In production you usually *want* that to be a no-op, so discard it with `let _ = dotenvy::dotenv();`. Do not `unwrap()` it, or your binary will refuse to start anywhere a `.env` is absent.

### Struct-shaped config with envy

Reading a dozen variables by hand gets tedious. The **envy** crate deserializes the environment straight into a `serde` struct, applying defaults and type-checking each field, the closest Rust equivalent to validating `process.env` with a schema library like `zod` or `envalid` in Node.

```bash
cargo add envy
cargo add serde --features derive
```

```rust
// src/main.rs
use serde::Deserialize;
use std::process;

#[derive(Debug, Deserialize)]
struct Config {
    // Required: deserialization fails if APP_DATABASE_URL is missing.
    database_url: String,
    // Default applied when APP_PORT is absent.
    #[serde(default = "default_port")]
    port: u16,
    // Option becomes None when APP_LOG_LEVEL is absent.
    log_level: Option<String>,
    // bool parsing via serde accepts only "true"/"false" (not "1"/"0").
    #[serde(default)]
    debug: bool,
}

fn default_port() -> u16 {
    8080
}

fn main() {
    // envy maps APP_DATABASE_URL -> database_url, APP_PORT -> port, etc.
    let config = envy::prefixed("APP_")
        .from_env::<Config>()
        .unwrap_or_else(|err| {
            eprintln!("configuration error: {err}");
            process::exit(1);
        });

    println!("{config:#?}");
}
```

```text
$ APP_DATABASE_URL=postgres://db/app APP_PORT=5000 APP_LOG_LEVEL=info APP_DEBUG=true cargo run --quiet
Config {
    database_url: "postgres://db/app",
    port: 5000,
    log_level: Some(
        "info",
    ),
    debug: true,
}

$ APP_DATABASE_URL=postgres://db/app cargo run --quiet
Config {
    database_url: "postgres://db/app",
    port: 8080,
    log_level: None,
    debug: false,
}

$ cargo run --quiet
configuration error: missing value for field database_url

$ APP_DATABASE_URL=x APP_PORT=notanumber cargo run --quiet
configuration error: invalid digit found in string while parsing value 'notanumber' provided by PORT
```

envy lowercases variable names and strips the prefix to match field names: `APP_DATABASE_URL` → `database_url`. A missing required field, or a value that does not parse to the field's type, is a single descriptive `Err`: no per-field plumbing. envy even splits a comma-separated value into a `Vec`:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Settings {
    host: String,
    #[serde(default)]
    allowed_origins: Vec<String>,
}

fn main() {
    let s = envy::prefixed("SVC_").from_env::<Settings>().unwrap();
    println!("host = {}", s.host);
    println!("origins = {:?}", s.allowed_origins);
}
```

```text
$ SVC_HOST=0.0.0.0 SVC_ALLOWED_ORIGINS="https://a.com,https://b.com" cargo run --quiet
host = 0.0.0.0
origins = ["https://a.com", "https://b.com"]
```

---

## Key Differences

| Aspect | Node (`process.env`) | Rust (`std::env`) |
| --- | --- | --- |
| Read a variable | `process.env.KEY` → `string \| undefined` | `env::var("KEY")` → `Result<String, VarError>` |
| Missing variable | `undefined` (silent) | `Err(VarError::NotPresent)` (must handle) |
| Type | Always `string` | Always `String`; you `parse()` to a real type |
| Default value | `process.env.KEY ?? "x"` | `env::var("KEY").unwrap_or_else(\|_\| "x".into())` |
| Optional value | truthiness checks | `env::var("KEY").ok()` → `Option<String>` |
| Non-UTF-8 values | coerced/garbled | explicit `VarError::NotUnicode`, or use `var_os` |
| Writing a variable | `process.env.KEY = "v"` (anytime) | `unsafe { env::set_var(..) }` (edition 2024) |
| `.env` files | `dotenv` (import side effect) | `dotenvy::dotenv()` (explicit call) |
| Schema validation | `zod` / `envalid` (external) | `envy` + `serde` derive |
| `.env` vs real env | real env wins (dotenv default) | real env wins (dotenvy default) |

### Why is `set_var` `unsafe` now?

This surprises everyone coming from JavaScript. In the latest stable edition (2024), `std::env::set_var` and `remove_var` are `unsafe` functions:

```rust playground
use std::env;

fn main() {
    // SAFETY: called before any threads are spawned, so no other thread
    // can be reading the environment concurrently.
    unsafe {
        env::set_var("APP_MODE", "production");
    }
    println!("APP_MODE = {}", env::var("APP_MODE").unwrap());
}
```

```text
$ cargo run --quiet
APP_MODE = production
```

The reason is genuine: on Unix, the C `getenv`/`setenv` machinery underneath is **not thread-safe**. One thread writing the environment while another reads it is a data race that can crash or corrupt memory. JavaScript hides this because Node is single-threaded for `process.env`. Rust makes the danger visible: mutate the environment only at startup, before spawning threads, and mark it `unsafe` to acknowledge the contract. In practice you rarely set environment variables at runtime at all; prefer passing config values through your own data structures.

### The environment is a snapshot, not a live binding

Both languages read the environment that existed when the process started (plus any in-process mutations). Neither sees changes a *parent* shell makes after launch. The difference is that Rust's standard library, and most config crates, encourage you to read the environment **once** into a typed struct at startup, then pass that struct around, rather than calling `env::var` scattered throughout the code the way `process.env.X` tends to proliferate in Node.

---

## Common Pitfalls

### Pitfall 1: Calling `set_var` without `unsafe`

Code ported from a single-threaded mindset trips on edition 2024 immediately:

```rust playground edition="2024"
use std::env;

fn main() {
    // does not compile (error[E0133]: call to unsafe function is unsafe)
    env::set_var("APP_MODE", "production");
    println!("{}", env::var("APP_MODE").unwrap());
}
```

The real compiler error:

```text
error[E0133]: call to unsafe function `set_var` is unsafe and requires unsafe block
 --> src/main.rs:5:5
  |
5 |     env::set_var("APP_MODE", "production");
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
  |
  = note: consult the function's documentation for information on how to avoid undefined behavior

For more information about this error, try `rustc --explain E0133`.
```

The fix is to wrap it in an `unsafe { .. }` block *and* ensure you only do it before any threads start. Better yet, avoid mutating the environment entirely.

### Pitfall 2: `unwrap()`-ing a missing variable

`env::var("KEY").unwrap()` is the Rust equivalent of assuming `process.env.KEY` is always defined. If the variable is absent, the program panics:

```rust
use std::env;

fn main() {
    // Panics at runtime if API_KEY is not set.
    let key = env::var("API_KEY").unwrap();
    println!("{key}");
}
```

```text
$ cargo run --quiet
thread 'main' panicked at src/main.rs:5:35:
called `Result::unwrap()` on an `Err` value: NotPresent
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

`NotPresent` is a cryptic panic message for an operator who forgot to export a variable. Prefer `unwrap_or_else` with a clear `eprintln!` + `process::exit`, or `expect("API_KEY must be set")`, or — best — bundle required variables into an `envy` struct so the error names the field.

### Pitfall 3: `unwrap()`-ing `dotenvy::dotenv()` in production

```rust
fn main() {
    // logic bug: panics anywhere there is no .env file (i.e. production).
    dotenvy::dotenv().unwrap();
}
```

`.env` files are a *development* convenience and are typically git-ignored and absent in production, where config comes from real environment variables. `unwrap()` here turns a normal situation into a crash. Use `let _ = dotenvy::dotenv();` (or `dotenvy::dotenv().ok();`) so a missing file is a silent no-op.

### Pitfall 4: Expecting silent string coercion

In Node, `process.env.PORT + 1` produces the string `"30001"` because `+` concatenates. In Rust, `env::var("PORT")` is a `String`; you cannot do arithmetic on it without an explicit `parse()`. This is a feature: it stops the classic JavaScript bug where a numeric env var silently becomes string-concatenated. But it means every numeric or boolean variable needs a deliberate parse step, as shown above.

### Pitfall 5: `.env` precedence confusion

A frequent question: "I set `PORT` in `.env` but it's using the old value." Remember that any variable already present in the real environment overrides `.env`. If your shell exported `PORT` in a previous session, or your IDE injects it, the `.env` line is ignored. To see what is actually in effect, print the resolved config at startup (as the Real-World Example does).

---

## Best Practices

- **Read the environment once, into a typed struct.** Parse `env::var`/`envy` results into a `Config` struct at startup and pass it (or an `Arc<Config>`) around. Scattering `env::var("X")` calls through the codebase is the Rust version of `process.env.X` sprawl and makes config impossible to audit.
- **Fail fast with a clear message.** Validate all required variables in `main` before doing real work. A missing `DATABASE_URL` should produce one readable line and a non-zero exit code, not a panic 200 lines later. See [Cross-Platform Considerations](/18-cli-tools/09-cross-platform/) for choosing meaningful exit codes.
- **Make `.env` loading optional and explicit.** Call `let _ = dotenvy::dotenv();` early; never `unwrap()` it. Keep `.env` out of version control and commit a `.env.example` listing required keys instead.
- **Prefer `envy` for anything beyond a couple of variables.** Struct deserialization gives you defaults, type-checking, and field-named errors for free, and reads like a schema.
- **Use a consistent prefix** (`APP_`, `SVC_`, your tool's name) so your variables do not collide with the dozens of unrelated ones already in the environment, and so `envy::prefixed("APP_")` can scope cleanly.
- **Never log secrets.** When you print resolved config for debugging, redact `API_KEY`, `DATABASE_URL` passwords, and tokens. Rust will happily `Debug`-print a secret-bearing struct; that is on you to prevent.
- **Reach for the `config` crate when you need layering.** For tools that merge defaults + a config file + environment + flags, the `config` crate composes all of these; `envy` alone is best when the environment is your single source of truth.

---

## Real-World Example

A small file-upload service that loads `.env` in development, reads `APP_`-prefixed configuration into a struct with sensible defaults, validates it, and exits with a meaningful code on misconfiguration:

```rust
// src/main.rs
// Depends on: cargo add dotenvy envy ; cargo add serde --features derive
use serde::Deserialize;
use std::process;

#[derive(Debug, Deserialize)]
struct Config {
    database_url: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_max_upload")]
    max_upload_mb: u32,
    #[serde(default)]
    verbose: bool,
}

fn default_port() -> u16 {
    8080
}

fn default_max_upload() -> u32 {
    10
}

impl Config {
    /// Load `.env` (if present), then read `APP_`-prefixed variables.
    fn load() -> Result<Self, envy::Error> {
        // Ignore a missing .env file; only deserialization errors propagate.
        let _ = dotenvy::dotenv();
        envy::prefixed("APP_").from_env::<Config>()
    }
}

fn main() {
    let config = Config::load().unwrap_or_else(|err| {
        eprintln!("failed to load configuration: {err}");
        eprintln!("hint: set APP_DATABASE_URL (and optionally APP_PORT, APP_MAX_UPLOAD_MB)");
        // 78 = EX_CONFIG from sysexits.h: a configuration error.
        process::exit(78);
    });

    println!("starting uploader on port {}", config.port);
    println!("  database: {}", config.database_url);
    println!("  max upload: {} MB", config.max_upload_mb);
    println!("  verbose: {}", config.verbose);
    // ... start the real server with `config` here ...
}
```

With a development `.env`:

```bash
# .env
APP_DATABASE_URL=postgres://localhost/uploader
APP_PORT=8000
APP_MAX_UPLOAD_MB=25
```

```text
$ cargo run --quiet
starting uploader on port 8000
  database: postgres://localhost/uploader
  max upload: 25 MB
  verbose: false

$ APP_PORT=9000 APP_VERBOSE=true cargo run --quiet
starting uploader on port 9000
  database: postgres://localhost/uploader
  max upload: 25 MB
  verbose: true

$ cargo run --quiet   # no .env, no env vars
failed to load configuration: missing value for field database_url
hint: set APP_DATABASE_URL (and optionally APP_PORT, APP_MAX_UPLOAD_MB)
$ echo $?
78
```

The second run shows real env vars overriding `.env`; the third shows a clean, actionable failure with a conventional exit code instead of a panic.

---

## Further Reading

- [`std::env` module](https://doc.rust-lang.org/std/env/index.html): official docs for `var`, `var_os`, `vars`, `set_var`, and `args`.
- [`std::env::VarError`](https://doc.rust-lang.org/std/env/enum.VarError.html): the two failure modes of `env::var`.
- [dotenvy on docs.rs](https://docs.rs/dotenvy): `.env` loading, the maintained successor to `dotenv`.
- [envy on docs.rs](https://docs.rs/envy): deserialize the environment into a `serde` struct.
- [config crate on docs.rs](https://docs.rs/config) — layered configuration (files + environment + overrides) when env vars are not your only source.
- Related guide sections:
  - [Argument Parsing with clap](/18-cli-tools/00-clap-basics/) and [clap derive API](/18-cli-tools/01-clap-derive/) — for command-line *arguments* (env vars and flags often layer together).
  - [Subcommands](/18-cli-tools/02-subcommands/) — git-style verbs that may each read configuration.
  - [File System Operations](/18-cli-tools/06-file-io/) and [Path Handling](/18-cli-tools/07-path-handling/) — for config that lives in files referenced by an env var.
  - [Cross-Platform Considerations](/18-cli-tools/09-cross-platform/) — exit codes and OS differences in environment handling.
  - [Error Handling](/08-error-handling/) — `Result`, `?`, and modeling configuration errors.
  - [Serialization](/15-serialization/) — `serde` derive, which powers `envy`.
  - [Getting Started](/01-getting-started/) and [Rust Basics](/02-basics/) — `cargo`, `Result`, and `match` fundamentals used here.
  - Compiling to [WebAssembly](/19-wasm/) changes how (and whether) you can read the environment — worth noting if your CLI logic is shared with a Wasm target.

---

## Exercises

### Exercise 1: A validated numeric variable with a default

**Difficulty:** Beginner

**Objective:** Practice converting `env::var` into a parsed, validated value with a fallback, the way you would harden a Node `parseInt(process.env.X)` call.

**Instructions:** Write a program that reads a `WORKERS` environment variable. If it is set to a valid integer of at least `1`, use it. If it is set but invalid (zero, negative, or non-numeric), print a warning to stderr and fall back to `1`. If it is unset, default to `4`. Print `spawning N workers`.

```rust playground
use std::env;

fn main() {
    let workers: usize = /* ??? read, parse, validate, default */ 0;
    println!("spawning {workers} workers");
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::env;

fn main() {
    let workers: usize = match env::var("WORKERS") {
        Ok(s) => match s.parse::<usize>() {
            Ok(n) if n >= 1 => n,
            Ok(_) => {
                eprintln!("WORKERS must be at least 1; using 1");
                1
            }
            Err(_) => {
                eprintln!("WORKERS='{s}' is not a number; using 1");
                1
            }
        },
        Err(_) => 4, // sensible default when unset
    };
    println!("spawning {workers} workers");
}
```

Verified output:

```text
$ cargo run --quiet
spawning 4 workers

$ WORKERS=8 cargo run --quiet
spawning 8 workers

$ WORKERS=0 cargo run --quiet
WORKERS must be at least 1; using 1
spawning 1 workers

$ WORKERS=foo cargo run --quiet
WORKERS='foo' is not a number; using 1
spawning 1 workers
```

</details>

### Exercise 2: Struct-shaped config with envy

**Difficulty:** Intermediate

**Objective:** Replace hand-rolled `env::var` calls with an `envy`-deserialized struct, including a default and a comma-separated list.

**Instructions:** Define a `Settings` struct with fields `host: String` (required), `port: u16` (default `3000`), and `allowed_origins: Vec<String>` (default empty, populated from a comma-separated value). Read it with the `SVC_` prefix and print each field. Add `cargo add envy` and `cargo add serde --features derive`.

```rust playground
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Settings {
    // TODO: host, port (default 3000), allowed_origins (default empty)
}

fn main() {
    // TODO: envy::prefixed("SVC_").from_env::<Settings>() and print fields
}
```

<details>
<summary>Solution</summary>

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Settings {
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    // envy splits a comma-separated value into a Vec.
    #[serde(default)]
    allowed_origins: Vec<String>,
}

fn default_port() -> u16 {
    3000
}

fn main() {
    match envy::prefixed("SVC_").from_env::<Settings>() {
        Ok(s) => {
            println!("host = {}", s.host);
            println!("port = {}", s.port);
            println!("origins = {:?}", s.allowed_origins);
        }
        Err(e) => eprintln!("config error: {e}"),
    }
}
```

Verified output:

```text
$ SVC_HOST=0.0.0.0 SVC_ALLOWED_ORIGINS="https://a.com,https://b.com" cargo run --quiet
host = 0.0.0.0
port = 3000
origins = ["https://a.com", "https://b.com"]

$ SVC_HOST=localhost cargo run --quiet
host = localhost
port = 3000
origins = []
```

</details>

### Exercise 3: Layered precedence — flag beats env beats default

**Difficulty:** Advanced

**Objective:** Implement the common configuration precedence rule "command-line flag > environment variable > built-in default" by hand, so you understand what clap + env layering does under the hood.

**Instructions:** Resolve a `log level` from three sources, highest priority first: a `--log-level <value>` (or `--log-level=<value>`) command-line argument, then the `LOG_LEVEL` environment variable, then the default `"info"`. Use `std::env::args` for the flag and `std::env::var` for the variable. Print `log level = X`.

```rust playground
use std::env;

fn resolve_log_level() -> String {
    // TODO: check --log-level arg, then LOG_LEVEL env, then default "info"
    String::from("info")
}

fn main() {
    println!("log level = {}", resolve_log_level());
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::env;

fn resolve_log_level() -> String {
    // 1. Highest priority: a --log-level <value> CLI argument.
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--log-level" {
            if let Some(v) = args.next() {
                return v;
            }
        } else if let Some(v) = arg.strip_prefix("--log-level=") {
            return v.to_string();
        }
    }
    // 2. Next: the LOG_LEVEL environment variable.
    // 3. Fallback: a built-in default.
    env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string())
}

fn main() {
    println!("log level = {}", resolve_log_level());
}
```

Verified output:

```text
$ cargo run --quiet
log level = info

$ LOG_LEVEL=warn cargo run --quiet
log level = warn

$ LOG_LEVEL=warn cargo run --quiet -- --log-level debug
log level = debug

$ cargo run --quiet -- --log-level=trace
log level = trace
```

> In a real tool you would let clap parse the flag and supply `env = "LOG_LEVEL"` on the argument so this layering is declarative — see [clap derive API](/18-cli-tools/01-clap-derive/). Doing it by hand here shows exactly which source wins.

</details>

---

_Next: [Cross-Platform Considerations](/18-cli-tools/09-cross-platform/) — line endings, path separators, `cfg!(windows)`, and exit codes._
