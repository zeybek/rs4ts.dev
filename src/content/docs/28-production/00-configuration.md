---
title: "Application Configuration"
description: "Load Rust app settings into a typed struct with the config and figment crates: layer defaults, files, and env vars, validated once at startup, vs Node's zod."
---

Every production service needs settings — the port it listens on, the database URL, log levels, timeouts, feature flags — and those settings change between your laptop, CI, staging, and production. This page is about loading those settings into your Rust program in a disciplined, type-safe way: layering defaults, files, and environment variables into a single **typed settings struct** you validate once at startup.

---

## Quick Overview

In Node you usually reach for `process.env` directly, sprinkle in `dotenv`, and maybe validate with `zod`. The values are strings until you coerce them, and a typo in an env var name is a silent `undefined` at runtime. Rust's configuration story centers on two crates — `config` and `figment` — that **merge layered sources** (defaults → files → environment variables, in precedence order) and **deserialize the result into a struct** via Serde. The win for a TypeScript developer: a missing or mistyped value becomes a loud error at process startup, not an `undefined` that surfaces three requests later.

---

## TypeScript/JavaScript Example

A typical Node service loads config from environment variables, applies defaults, and validates shape. Here is a realistic loader using `zod` for validation (the closest JS analogue to Rust's typed deserialization):

```typescript
// config.ts — Node v22, using zod for validation
import { z } from "zod";

// 1. Define the shape and coercions.
const ConfigSchema = z.object({
  server: z.object({
    host: z.string().default("127.0.0.1"),
    port: z.coerce.number().int().min(1).max(65535).default(8080),
  }),
  database: z.object({
    url: z.string().url(),
    maxConnections: z.coerce.number().int().positive().default(10),
  }),
  logLevel: z.enum(["trace", "debug", "info", "warn", "error"]).default("info"),
});

export type Config = z.infer<typeof ConfigSchema>;

// 2. Map flat env vars onto the nested shape, then validate.
export function loadConfig(): Config {
  const raw = {
    server: {
      host: process.env.APP_SERVER_HOST,
      port: process.env.APP_SERVER_PORT,
    },
    database: {
      url: process.env.APP_DATABASE_URL,
      maxConnections: process.env.APP_DATABASE_MAX_CONNECTIONS,
    },
    logLevel: process.env.APP_LOG_LEVEL,
  };

  const result = ConfigSchema.safeParse(raw);
  if (!result.success) {
    // Fail fast at startup instead of crashing mid-request.
    console.error("Invalid configuration:", result.error.format());
    process.exit(1);
  }
  return result.data;
}
```

This is solid TypeScript, but notice the friction:

- You manually wire each env var into the nested object (`process.env.APP_SERVER_HOST` → `server.host`).
- Everything from `process.env` is a `string | undefined`, so you lean on `z.coerce` to turn `"8080"` into a number.
- There is no built-in notion of layered **files** (a committed `default` plus a per-environment override); you would add `dotenv` or hand-roll it.

---

## Rust Equivalent

The `config` crate handles the layering and the env-var-to-struct mapping for you. Add it alongside Serde:

```bash
cargo add config
cargo add serde --features derive
```

> The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically, and `cargo add` (built into Cargo since 1.62, no `cargo-edit` needed) resolves the newest compatible versions. The examples here use `config` 0.15, `serde` 1, and `figment` 0.10.

Define the settings as a normal struct and let Serde deserialize the merged config into it:

```rust
use config::{Config, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Settings {
    server: ServerConfig,
    database: DatabaseConfig,
    log_level: String,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
    workers: usize,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    url: String,
    max_connections: u32,
}

impl Settings {
    fn load() -> Result<Self, config::ConfigError> {
        // Choose the active environment (development, staging, production).
        let env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".into());

        let config = Config::builder()
            // 1. Defaults: committed to the repo, lowest priority.
            .add_source(File::with_name("config/default"))
            // 2. Per-environment overrides; missing file is fine.
            .add_source(File::with_name(&format!("config/{env}")).required(false))
            // 3. Environment variables, highest priority.
            //    APP__SERVER__PORT=9000 overrides server.port.
            .add_source(Environment::with_prefix("APP").separator("__"))
            .build()?;

        config.try_deserialize()
    }
}

fn main() -> Result<(), config::ConfigError> {
    let settings = Settings::load()?;
    println!("{settings:#?}");
    Ok(())
}
```

With these two files in a `config/` directory:

```toml
# config/default.toml
log_level = "info"

[server]
host = "127.0.0.1"
port = 8080
workers = 4

[database]
url = "postgres://localhost/app_dev"
max_connections = 10
```

```toml
# config/production.toml — only the values that differ
log_level = "warn"

[server]
host = "0.0.0.0"
workers = 16

[database]
max_connections = 50
```

Running with the defaults prints:

```text
Settings {
    server: ServerConfig {
        host: "127.0.0.1",
        port: 8080,
        workers: 4,
    },
    database: DatabaseConfig {
        url: "postgres://localhost/app_dev",
        max_connections: 10,
    },
    log_level: "info",
}
```

Running with `APP_ENV=production APP__SERVER__PORT=9000` prints (note how each layer wins where it sets a value):

```text
Settings {
    server: ServerConfig {
        host: "0.0.0.0",
        port: 9000,
        workers: 16,
    },
    database: DatabaseConfig {
        url: "postgres://localhost/app_dev",
        max_connections: 50,
    },
    log_level: "warn",
}
```

Trace where each field came from:

- `server.host` and `workers` come from `production.toml`.
- `server.port` (9000) comes from the **environment variable**, which sits above both files.
- `database.url` was *not* set in `production.toml`, so it **falls through** to `default.toml`.
- `database.max_connections` (50) is the production override.

That fall-through behavior is the whole point of layered config, and you get it without writing any merge logic.

---

## Detailed Explanation

### `Config::builder()` and source precedence

`Config::builder()` returns a builder you feed sources into with `.add_source(...)`. **Order matters: later sources override earlier ones.** So the canonical production order is defaults → environment file → process environment variables. The builder collects everything into a flat key tree (e.g. `server.port`), then `try_deserialize()` walks that tree into your struct.

### File sources and format detection

`File::with_name("config/default")` deliberately omits the extension. The `config` crate probes the supported formats (`.toml`, `.yaml`, `.json`, `.ini`, `.ron`, `.json5`) and loads whichever file exists. To pin a format, use `File::with_name("config/default").format(config::FileFormat::Toml)` or the typed constructor. Marking the per-environment file `.required(false)` means a missing `config/staging.toml` is silently skipped rather than an error: exactly what you want for optional overrides.

> **Note:** A common TOML gotcha bites here. In TOML, **top-level keys must appear before any `[table]` header**, because a table header captures every key that follows it until the next header. Writing `log_level = "info"` *after* a `[database]` block makes it a key of `database`, not a top-level key, and deserialization then complains that the top-level `log_level` is missing. Put scalar top-level keys at the very top of the file.

### Environment variables: prefix and separator

`Environment::with_prefix("APP").separator("__")` means an env var named `APP__SERVER__PORT` maps to the config key `server.port`. The prefix namespaces *your* variables so unrelated env vars (`PATH`, `HOME`) are ignored, and the `__` separator (double underscore) descends into nested tables. The crate parses scalar strings into the target type during deserialization — `"9000"` becomes a `u16` — which is the Rust equivalent of zod's `z.coerce.number()`, except it is the *type* of the struct field that drives the coercion, not a separate schema.

### `try_deserialize()` — the type-safe payoff

`config.try_deserialize::<Settings>()` is where the strings become a real, typed value. Because `Settings` derives `serde::Deserialize`, the compiler-generated code knows every field's type and whether it is required. A missing required field, an unparseable number, or an unknown enum variant all become a `ConfigError` returned from `load()`. Propagating that error out of `main` (via `?` and `Result<(), config::ConfigError>`) makes the process exit with a non-zero status and a printed reason, the Rust spelling of `process.exit(1)` in the zod example, but driven entirely by the type system.

### In-code defaults with `set_default`

You do not have to keep defaults in a file. The builder can seed them in code, which is handy for values that should always have a sensible fallback:

```rust
use config::{Config, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Settings {
    server: ServerConfig,
    #[serde(default = "default_log_level")]
    log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

impl Settings {
    fn load() -> Result<Self, config::ConfigError> {
        Config::builder()
            // In-code defaults: lowest precedence of all.
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 8080)?
            // Optional file; safe to omit on a fresh checkout.
            .add_source(File::with_name("config/app").required(false))
            .add_source(Environment::with_prefix("APP").separator("__"))
            .build()?
            .try_deserialize()
    }
}

fn main() -> Result<(), config::ConfigError> {
    let settings = Settings::load()?;
    println!("{settings:#?}");
    Ok(())
}
```

With no config file at all, this prints:

```text
Settings {
    server: ServerConfig {
        host: "127.0.0.1",
        port: 8080,
    },
    log_level: "info",
}
```

Note there are *two* kinds of default here: `set_default` on the builder (a config-layer default) and Serde's `#[serde(default = "...")]` on the field (a deserialization-time default for a key that no source provided at all). They cover slightly different cases: builder defaults participate in the layered merge; Serde defaults fill in keys absent from the entire merged tree.

---

## Key Differences

| Concern | TypeScript / Node | Rust (`config` / `figment`) |
| --- | --- | --- |
| Where values come from | `process.env` (+ `dotenv`), manually wired | Layered sources merged by the crate |
| Type of a raw value | `string \| undefined` | Parsed into the struct field's real type |
| Coercion | Explicit (`Number(x)`, `z.coerce`) | Driven by the target type via Serde |
| Nested env vars | Manual mapping per key | `APP__A__B` → `a.b` automatically |
| Missing required value | `undefined`, often silent | `ConfigError`, fails at startup |
| Validation | Separate library (`zod`, `joi`) | Type system + a small `validate()` method |
| Files | Hand-rolled or `dotenv` | First-class layered file sources |
| Result | A plain object | A typed, immutable `struct` |

The deeper conceptual shift: in Node, configuration is a runtime bag of strings you defensively poke at. In Rust, configuration is **parsed once into a value with a known type**, after which the rest of your program never touches a raw string or worries whether a field exists. This is the same "parse, don't validate" discipline you may already apply with zod. Rust just makes it the default path and ties it to the type system instead of a separate schema object.

> **Note:** Unlike TypeScript, where `z.infer` derives the *type* from a runtime schema, in Rust the struct *is* the schema. There is no separate validator object to keep in sync. The `#[derive(Deserialize)]` on the struct generates the parsing code at compile time.

### `config` vs `figment`

Both crates do layered config into a Serde struct. `config` is the most widely used standalone choice. `figment` is the configuration engine behind the Rocket web framework and has an ergonomic provider API plus rich error messages that point at *which provider* supplied a bad value. The same example in `figment`:

```bash
cargo add figment --features toml,env
cargo add serde --features derive
```

```rust
use figment::{Figment, providers::{Format, Toml, Env, Serialized}};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct Settings {
    host: String,
    port: u16,
    log_level: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            host: "127.0.0.1".into(),
            port: 8080,
            log_level: "info".into(),
        }
    }
}

fn main() -> Result<(), figment::Error> {
    let settings: Settings = Figment::from(Serialized::defaults(Settings::default()))
        .merge(Toml::file("App.toml"))
        .merge(Env::prefixed("APP_"))
        .extract()?;

    println!("{settings:#?}");
    Ok(())
}
```

With an `App.toml` of `port = 3000` and `log_level = "debug"`, this prints:

```text
Settings {
    host: "127.0.0.1",
    port: 3000,
    log_level: "debug",
}
```

And with `APP_PORT=9999` set, `port` becomes `9999` while the rest is unchanged. Note figment's nice touch: `Serialized::defaults(Settings::default())` lets your `Default` impl *be* the base layer, so defaults live in ordinary Rust rather than a separate file. Choose `figment` if you want its provider model and error quality (or if you are already on Rocket); choose `config` for a smaller, format-agnostic dependency. Both are current and well maintained.

---

## Common Pitfalls

### Pitfall 1: Wrong environment-variable separator or prefix

If you expect `APP_SERVER_PORT` to map to `server.port`, it will not; a single underscore is ambiguous (`server_port` vs `server.port`). Use a distinct nested separator like `__` and write `APP__SERVER__PORT`. Set the separator explicitly: `Environment::with_prefix("APP").separator("__")`.

### Pitfall 2: A type mismatch from the environment is a real, descriptive error

Set `APP__SERVER__PORT=not_a_number` and the deserialization fails at startup with the actual message:

```text
Error: invalid type: string "not_a_number", expected an integer for key `server.port` in the environment
```

That is the genuine `config`-crate error, and it tells you both the offending key and the source (the environment). This is the behavior you want: a bad deploy-time value stops the process immediately instead of returning a confusing 500 on the first request.

### Pitfall 3: A missing required field stops startup

If no source provides a field your struct declares (and it has no Serde default), `try_deserialize` fails. Removing `url` from the database config produces:

```text
Error: missing configuration field "database.url"
```

In the Node version this would have been `undefined` flowing into your database client and blowing up later. Here it is caught before the server binds a socket. To make a field optional, model it as `Option<String>` or give it a `#[serde(default)]`.

### Pitfall 4: TOML top-level keys after a table header

As noted above, this TOML silently misfiles `log_level`:

```toml
[database]
url = "postgres://localhost/app"

log_level = "info"   # this is now database.log_level, not top-level
```

The fix is to move all top-level scalars above every `[table]` header. This is a TOML semantics rule, not a Rust bug, but it is the single most common confusion when hand-writing config files.

### Pitfall 5: Treating config as global mutable state

It is tempting to stuff settings into a `static mut` so any function can read them. Don't — `static mut` is unsafe and a data-race footgun. Load the config once in `main`, then pass `&Settings` (or an `Arc<Settings>` for shared async tasks) to the code that needs it, or use a `OnceLock` for a read-only global (shown in Best Practices). See [Ownership & Borrowing](/05-ownership/02-borrowing/) for why passing references is the idiomatic alternative.

---

## Best Practices

### Validate beyond the type system at load time

Types catch "is this a `u16`?" but not "is this port privileged?" Add a `validate()` method and call it inside `load()` so invalid-but-well-typed values still fail fast:

```rust
use config::{Config, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Settings {
    server: ServerConfig,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    port: u16,
}

impl Settings {
    fn load() -> Result<Self, config::ConfigError> {
        let settings: Settings = Config::builder()
            .set_default("server.port", 8080)?
            .add_source(Environment::with_prefix("APP").separator("__"))
            .build()?
            .try_deserialize()?;
        settings.validate()?;
        Ok(settings)
    }

    fn validate(&self) -> Result<(), config::ConfigError> {
        if self.server.port < 1024 {
            return Err(config::ConfigError::Message(format!(
                "server.port must be >= 1024, got {}",
                self.server.port
            )));
        }
        Ok(())
    }
}

fn main() -> Result<(), config::ConfigError> {
    let settings = Settings::load()?;
    println!("{settings:#?}");
    Ok(())
}
```

Running with `APP__SERVER__PORT=80` produces the real error:

```text
Error: server.port must be >= 1024, got 80
```

> **Tip:** For richer cross-field validation, the `validator` crate adds `#[validate(...)]` attributes to your struct, similar in spirit to zod refinements.

### Model enums and domains as Rust enums, not strings

Don't keep `log_level: String` and re-check it everywhere. Deserialize straight into an enum so an invalid value is rejected at load time:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
```

`#[serde(rename_all = "lowercase")]` lets the TOML/env value `"info"` map to `LogLevel::Info`. Now a typo like `"infoo"` fails to deserialize instead of slipping through. See [Enums](/06-data-structures/02-enums/) for more on this pattern.

### Provide a read-only global with `OnceLock` when threading references is painful

For a value loaded once and never mutated, the standard library's `OnceLock` gives a safe, lazily-initialized global without external crates:

```rust
use config::{Config, Environment, File};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
struct Settings {
    server: ServerConfig,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();

impl Settings {
    fn load() -> Result<Self, config::ConfigError> {
        Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 8080)?
            .add_source(File::with_name("config/app").required(false))
            .add_source(Environment::with_prefix("APP").separator("__"))
            .build()?
            .try_deserialize()
    }

    fn global() -> &'static Settings {
        SETTINGS.get_or_init(|| Settings::load().expect("failed to load config"))
    }
}

fn main() {
    let s = Settings::global();
    println!("listen on {}:{}", s.server.host, s.server.port);
}
```

This prints `listen on 127.0.0.1:8080`. The `get_or_init` closure runs exactly once, and every later call returns the same `&'static Settings`. Prefer passing `&Settings` explicitly where you can; reach for the global only when wiring a reference through many layers is genuinely noisy.

### Other practical rules

- **Commit `config/default.toml`; never commit secrets.** Keep real secrets in environment variables (the top layer) and out of version control; see [environment-based config](/28-production/01-environment/).
- **Make config immutable after load.** Bind it to a non-`mut` `let`. Reloading config at runtime is a deliberate feature, not a default.
- **Fail fast.** Load and validate in `main` before binding a socket or opening a pool. A bad config should crash the process, not degrade requests.
- **Pin sensible defaults but require the dangerous things.** A default port is friendly; a default production database URL is a footgun. Make truly environment-specific values required.

---

## Real-World Example

A production HTTP service typically has nested config for the server, database, and logging, with strongly-typed enums and a `Duration` derived from a plain seconds value. Here is a self-contained, compile-verified version:

```rust
use config::{Config, Environment, File};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum AppEnv {
    Development,
    Staging,
    Production,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Deserialize)]
struct Settings {
    environment: AppEnv,
    server: ServerConfig,
    database: DatabaseConfig,
    logging: LoggingConfig,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
    request_timeout_secs: u64,
}

impl ServerConfig {
    fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    url: String,
    max_connections: u32,
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    level: LogLevel,
    format: LogFormat,
}

impl Settings {
    fn load() -> Result<Self, config::ConfigError> {
        let env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".into());

        Config::builder()
            .add_source(File::with_name("config/default"))
            .add_source(File::with_name(&format!("config/{env}")).required(false))
            .add_source(Environment::with_prefix("APP").separator("__"))
            .build()?
            .try_deserialize()
    }
}

fn main() -> Result<(), config::ConfigError> {
    let settings = Settings::load()?;
    println!("env       = {:?}", settings.environment);
    println!("listen    = {}:{}", settings.server.host, settings.server.port);
    println!("timeout   = {:?}", settings.server.request_timeout());
    println!("db url    = {}", settings.database.url);
    println!("db pool   = {}", settings.database.max_connections);
    println!(
        "logging   = {:?} / {:?}",
        settings.logging.level, settings.logging.format
    );
    Ok(())
}
```

With these config files:

```toml
# config/default.toml
environment = "development"

[server]
host = "127.0.0.1"
port = 8080
request_timeout_secs = 30

[database]
url = "postgres://localhost/app_dev"
max_connections = 10

[logging]
level = "info"
format = "pretty"
```

```toml
# config/production.toml
environment = "production"

[server]
host = "0.0.0.0"
port = 8080
request_timeout_secs = 15

[database]
max_connections = 50

[logging]
level = "info"
format = "json"
```

Running in development prints:

```text
env       = Development
listen    = 127.0.0.1:8080
timeout   = 30s
db url    = postgres://localhost/app_dev
db pool   = 10
logging   = Info / Pretty
```

Running with `APP_ENV=production APP__DATABASE__URL='postgres://prod-host/app'` prints:

```text
env       = Production
listen    = 0.0.0.0:8080
timeout   = 15s
db url    = postgres://prod-host/app
db pool   = 50
logging   = Info / Json
```

Every value is now a real type: `AppEnv` and `LogLevel` are enums you can `match` on, `request_timeout()` hands back a `Duration` ready for `tokio::time::timeout`, and the database URL is supplied from the environment (where a secret belongs) rather than checked into a file. From here you would wire `settings.server.host`/`port` into your `axum::serve` listener and pass `&settings.database` to your pool builder. The web side is covered in [Section 16: Web APIs](/16-web-apis/) and [Section 17: Database](/17-database/).

---

## Further Reading

- [`config` crate documentation](https://docs.rs/config): sources, formats, and the builder API
- [`figment` crate documentation](https://docs.rs/figment): providers, profiles, and rich error reporting
- [Serde derive documentation](https://serde.rs/derive.html): `Deserialize`, `#[serde(default)]`, and `rename_all`
- [The Twelve-Factor App: Config](https://12factor.net/config): the principle behind environment-based overrides
- Related guide sections:
  - [Environment-based config](/28-production/01-environment/) — 12-factor config, `dotenvy` in development, validating required env at startup
  - [Graceful shutdown](/28-production/02-graceful-shutdown/) — using your config's listen address with `axum` shutdown
  - [Production checklist](/28-production/09-production-checklist/) — where configuration fits in overall readiness
  - [Section 15: Serialization](/15-serialization/) — the Serde mechanics powering `try_deserialize`
  - [Enums](/06-data-structures/02-enums/) and [Borrowing](/05-ownership/02-borrowing/) — modeling settings as enums and passing them by reference
  - [Section 29: Migration Guide](/29-migration-guide/) — porting a Node service's config layer to Rust

---

## Exercises

### Exercise 1: A two-layer config loader

**Difficulty:** Beginner

**Objective:** Build a typed config that merges defaults with environment variables.

**Instructions:** Create a `Settings` struct with a nested `server` section (`host: String`, `port: u16`). Use `Config::builder()` to set in-code defaults (`127.0.0.1`, `8080`), then layer an `Environment` source with prefix `APP` and separator `__`. Print the deserialized struct. Verify that running with `APP__SERVER__PORT=3000` changes the port while the host stays at the default.

<details>
<summary>Solution</summary>

```rust
use config::{Config, Environment};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Settings {
    server: ServerConfig,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

impl Settings {
    fn load() -> Result<Self, config::ConfigError> {
        Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 8080)?
            .add_source(Environment::with_prefix("APP").separator("__"))
            .build()?
            .try_deserialize()
    }
}

fn main() -> Result<(), config::ConfigError> {
    let settings = Settings::load()?;
    println!("{settings:#?}");
    Ok(())
}
```

With no env vars set this prints `host: "127.0.0.1", port: 8080`; with `APP__SERVER__PORT=3000` the port becomes `3000` while the host is unchanged. (`cargo add config` and `cargo add serde --features derive` first.)

</details>

### Exercise 2: Enums and startup validation

**Difficulty:** Intermediate

**Objective:** Reject invalid values at load time using a Rust enum and a custom check.

**Instructions:** Extend Exercise 1 with a `log_level` field typed as an enum (`Trace`/`Debug`/`Info`/`Warn`/`Error`) using `#[serde(rename_all = "lowercase")]`, defaulting to `info`. Add a `validate()` method that returns an error if `server.port` is below `1024`, and call it inside `load()`. Confirm that `APP__SERVER__PORT=80` produces a descriptive error and that an invalid `APP__LOG_LEVEL` is also rejected.

<details>
<summary>Solution</summary>

```rust
use config::{Config, Environment};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Deserialize)]
struct Settings {
    server: ServerConfig,
    #[serde(default = "default_log_level")]
    log_level: LogLevel,
}

fn default_log_level() -> LogLevel {
    LogLevel::Info
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    port: u16,
}

impl Settings {
    fn load() -> Result<Self, config::ConfigError> {
        let settings: Settings = Config::builder()
            .set_default("server.port", 8080)?
            .add_source(Environment::with_prefix("APP").separator("__"))
            .build()?
            .try_deserialize()?;
        settings.validate()?;
        Ok(settings)
    }

    fn validate(&self) -> Result<(), config::ConfigError> {
        if self.server.port < 1024 {
            return Err(config::ConfigError::Message(format!(
                "server.port must be >= 1024, got {}",
                self.server.port
            )));
        }
        Ok(())
    }
}

fn main() -> Result<(), config::ConfigError> {
    let settings = Settings::load()?;
    println!("{settings:#?}");
    Ok(())
}
```

Default run prints `LogLevel::Info` and port `8080`. `APP__SERVER__PORT=80` fails with `server.port must be >= 1024, got 80`. An unknown level such as `APP__LOG_LEVEL=verbose` fails during deserialization because it matches no enum variant.

</details>

### Exercise 3: Layered files plus a lazy global

**Difficulty:** Advanced

**Objective:** Combine a committed default file, a per-environment override file, env vars, and a `OnceLock` global accessor.

**Instructions:** Write a `config/default.toml` and a `config/production.toml`. Build the config from `default` → `config/{APP_ENV}` (optional) → environment variables. Expose the settings through `Settings::global()` backed by a `static OnceLock<Settings>`. Include a `Vec<String>` field (for example, `cors.allowed_origins`) and confirm it loads from TOML; bonus: allow overriding the list from a comma-separated env var. Splitting a string into a list requires `.try_parsing(true)` (so the value is parsed rather than kept as a raw string), `.list_separator(",")`, and `.with_list_parse_key("cors.allowed_origins")` to register which key holds a list.

<details>
<summary>Solution</summary>

```rust
use config::{Config, Environment, File};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
struct Settings {
    server: ServerConfig,
    cors: CorsConfig,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
struct CorsConfig {
    allowed_origins: Vec<String>,
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();

impl Settings {
    fn load() -> Result<Self, config::ConfigError> {
        let env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".into());

        Config::builder()
            .add_source(File::with_name("config/default"))
            .add_source(File::with_name(&format!("config/{env}")).required(false))
            .add_source(
                Environment::with_prefix("APP")
                    .separator("__")
                    .try_parsing(true)
                    .list_separator(",")
                    .with_list_parse_key("cors.allowed_origins"),
            )
            .build()?
            .try_deserialize()
    }

    fn global() -> &'static Settings {
        SETTINGS.get_or_init(|| Settings::load().expect("failed to load config"))
    }
}

fn main() {
    let s = Settings::global();
    println!("listen on {}:{}", s.server.host, s.server.port);
    println!("origins: {:?}", s.cors.allowed_origins);
}
```

```toml
# config/default.toml
[server]
host = "127.0.0.1"
port = 8080

[cors]
allowed_origins = ["https://app.example.com", "https://admin.example.com"]
```

This prints `listen on 127.0.0.1:8080` and the two origins from TOML. A `config/production.toml` can override the host/port, and `Settings::global()` initializes the config exactly once on first access. With the `try_parsing`/`with_list_parse_key` combination above, running `APP__CORS__ALLOWED_ORIGINS='https://a.com,https://b.com'` overrides the list to `["https://a.com", "https://b.com"]`.

</details>
