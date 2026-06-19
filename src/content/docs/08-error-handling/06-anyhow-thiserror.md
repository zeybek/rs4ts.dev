---
title: "`anyhow` & `thiserror`"
description: "thiserror derives Display, Error, and From for typed library errors; anyhow gives apps one error type with context, versus TS's untyped throw."
---

Writing `Display`, `Debug`, `Error`, and `From` impls by hand for every error type gets tedious fast. The `thiserror` and `anyhow` crates are the community-standard answer: `thiserror` **derives** those impls for your library's typed errors, and `anyhow` gives applications one ergonomic error type with rich context.

---

## Quick Overview

In TypeScript you `throw` anything and rarely think about error *types*; every catch block just gets an `Error` (or `unknown`). Rust pushes you to be explicit, and these two crates make that explicitness cheap. Use **`thiserror`** when you are writing a **library** and want callers to be able to `match` on specific failure variants; use **`anyhow`** when you are writing an **application** and just want to attach context and propagate any error to `main`. They are designed to work together: a library defines a `thiserror` enum, and the application that consumes it wraps everything in `anyhow`.

> **Note:** This file covers the two crates and their current APIs. Defining error types *by hand* (without `thiserror`) is covered in [Custom Errors](/08-error-handling/04-custom-errors/) and [The `Error` Trait](/08-error-handling/05-error-trait/); the `?` operator that ties it all together is in [The `?` Operator](/08-error-handling/01-question-mark/); and the higher-level "which one, when?" decision lives in [Best Practices](/08-error-handling/08-best-practices/).

---

## TypeScript/JavaScript Example

In TypeScript, distinguishing error kinds means subclassing `Error` (verbose) or, more commonly, tagging plain objects with a discriminant and checking it at the catch site. Adding context usually means re-throwing a new error whose `cause` points at the original.

```typescript
// A "library" module: it defines its own error kinds so callers can branch.
class ConfigError extends Error {
  readonly kind: "not-found" | "invalid-port" | "missing-key";
  constructor(kind: ConfigError["kind"], message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "ConfigError";
    this.kind = kind;
  }
}

function parsePort(raw: string): number {
  const port = Number(raw);
  if (!Number.isInteger(port)) {
    // Re-wrap the low-level reason as `cause` so it isn't lost.
    throw new ConfigError("invalid-port", "invalid port number", {
      cause: new TypeError(`'${raw}' is not an integer`),
    });
  }
  return port;
}

// An "application" layer: it adds context and ultimately just reports.
function loadServerConfig(raw: string): number {
  try {
    return parsePort(raw);
  } catch (e) {
    // Attaching context = throw a new error pointing at the old one.
    throw new Error(`while loading server config from '${raw}'`, { cause: e });
  }
}

try {
  loadServerConfig("not-a-number");
} catch (e) {
  if (e instanceof Error) {
    // Walk the cause chain manually.
    let msg = e.message;
    let cur = e.cause;
    while (cur instanceof Error) {
      msg += `: ${cur.message}`;
      cur = cur.cause;
    }
    console.log(msg); // while loading server config from 'not-a-number': invalid port number: ...
  }
  // To branch on the *kind*, you must downcast manually:
  if (e instanceof ConfigError && e.kind === "invalid-port") {
    // handle specifically
  }
}
```

Two jobs are tangled together here, and TypeScript gives you no help separating them:

- **The library job:** define distinguishable error *kinds* (the `kind` discriminant).
- **The application job:** add human-readable context and walk the cause chain.

Rust splits these cleanly across two crates.

---

## Rust Equivalent

### `thiserror` for the library layer

```rust
use thiserror::Error;

// One derive generates Display, Error, source(), and From impls.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found at `{0}`")]
    NotFound(String),

    // `#[from]` generates `From<ParseIntError>` AND wires up `source()`.
    #[error("invalid port number")]
    InvalidPort(#[from] std::num::ParseIntError),

    // Named fields are interpolated by name in the message string.
    #[error("missing required key `{key}`")]
    MissingKey { key: String },
}

fn parse_port(raw: &str) -> Result<u16, ConfigError> {
    // `?` converts ParseIntError -> ConfigError via the derived `From`.
    let port: u16 = raw.parse()?;
    Ok(port)
}
```

### `anyhow` for the application layer

```rust
use anyhow::{Context, Result};

// `anyhow::Result<T>` is shorthand for `Result<T, anyhow::Error>`.
fn read_config(path: &str) -> Result<String> {
    std::fs::read_to_string(path)
        // `.with_context` adds a human-readable layer; the original error
        // becomes the *source* beneath it.
        .with_context(|| format!("could not read config file `{path}`"))
}

fn main() -> Result<()> {
    let _contents = read_config("/no/such/config.toml")?;
    Ok(())
}
```

Running that `main` prints a formatted report and exits non-zero. Verified output:

```text
Error: could not read config file `/no/such/config.toml`

Caused by:
    No such file or directory (os error 2)
```

The library code (`thiserror`) produces a precise enum a caller can `match` on; the application code (`anyhow`) attaches context and lets `?` and `main` do the reporting. No hand-written `impl Display`, no hand-written `impl Error`, no manual cause-chain walking.

---

## Detailed Explanation

### Setting up the crates

Both are ordinary dependencies. From your project directory:

```bash
cargo add anyhow thiserror
```

```text
      Adding anyhow v1.0.102 to dependencies
      Adding thiserror v2.0.18 to dependencies
     Locking 7 packages to latest Rust 1.96.0 compatible versions
```

> **Note:** `cargo add` is built into Cargo (since 1.62); you do **not** need the old `cargo-edit` crate. This guide uses **anyhow 1.x** (currently `1.0.102`) and **thiserror 2.x** (currently `2.0.18`), the latest stable releases.

### `thiserror`: a derive macro, not a runtime

`#[derive(Error)]` is a **procedural macro** that, at compile time, writes the `impl std::fmt::Display`, `impl std::error::Error`, and any `impl From<...>` you requested. It adds **zero runtime cost and zero runtime dependency**: `thiserror` is purely a code generator. (This is unlike TypeScript decorators, which are real runtime function calls; see [Macros](/14-macros/) for why the two are fundamentally different.)

The pieces of the derive:

- **`#[derive(Debug, Error)]`**: `Error` requires `Debug` (the developer-facing form) and `Display` (the user-facing form). You always write `Debug` yourself via derive; `thiserror` writes `Display` for you from the `#[error(...)]` attribute.
- **`#[error("...")]`**: the format string for `Display`. Positional tuple fields are `{0}`, `{1}`; named struct fields are `{field_name}`. The same `{}` formatting and inline-variable syntax you saw in [Output](/02-basics/04-output/) applies.
- **`#[from]`** — on exactly one field of a variant, generates `From<ThatType> for YourError` so `?` can convert automatically, *and* makes that field the variant's `source()`.
- **`#[source]`** — marks a field as the `source()` (the underlying cause) **without** generating a `From` impl. Use it when you want the cause chain but the conversion would be ambiguous (e.g., two variants wrap the same error type).

### `anyhow`: one error type to rule the application

`anyhow::Error` is a single, dynamically-typed error (think of it as a smarter `Box<dyn Error>`) that:

- can be built from **any** type implementing `std::error::Error + Send + Sync + 'static` (via `?` or `.into()`),
- carries a **chain** of contexts and causes you can iterate,
- formats itself as a clean multi-line report,
- can still be **downcast** back to a concrete type when you need to branch.

The key trait is `Context`, which adds methods to *both* `Result` and `Option`:

```rust
use anyhow::{anyhow, bail, ensure, Context, Result};

fn load_settings(raw: &str) -> Result<u16> {
    let timeout: u16 = raw
        .parse()
        // `.with_context` is LAZY: the closure only runs on the error path.
        .with_context(|| format!("failed to parse timeout from {raw:?}"))?;

    // `ensure!` is like `assert!`, but returns Err instead of panicking.
    ensure!(timeout > 0, "timeout must be greater than zero");
    Ok(timeout)
}

fn pick(raw: &str) -> Result<u16> {
    if raw.is_empty() {
        // `bail!` == `return Err(anyhow!(...))`.
        bail!("no value provided");
    }
    // `anyhow!` builds an ad-hoc error from a formatted message.
    load_settings(raw).map_err(|e| anyhow!("pick failed: {e}"))
}
```

- **`anyhow!("...")`**: construct an error from a message (or wrap an existing error).
- **`bail!("...")`**: early-return that error.
- **`ensure!(cond, "...")`**: bail unless a condition holds.
- **`.context(msg)`**: attach an **eager** message (computed even on the happy path; cheap for string literals).
- **`.with_context(|| msg)`**: attach a **lazy** message (the closure runs only when there is actually an error; prefer this when the message requires a `format!` allocation).

### Reading the chain and recovering types

`anyhow::Error` formats itself three ways, all verified:

```rust
use anyhow::{Context, Result};

fn load(raw: &str) -> Result<u16> {
    raw.parse::<u16>()
        .with_context(|| format!("failed to parse timeout from {raw:?}"))
}

fn main() {
    let e = load("abc").unwrap_err();
    println!("{e}");    // Display: just the top context
    println!("{e:#}");  // alternate: single-line chain joined by ": "
    for (i, cause) in e.chain().enumerate() {
        println!("[{i}] {cause}"); // iterate top -> bottom
    }
}
```

Output:

```text
failed to parse timeout from "abc"
failed to parse timeout from "abc": invalid digit found in string
[0] failed to parse timeout from "abc"
[1] invalid digit found in string
```

> **Tip:** `{e}` shows only the outermost message; `{e:#}` (alternate flag) shows the whole chain on one line; and when an `anyhow::Error` is returned from `main`, the `{e:?}` (Debug) form is used automatically, which is the multi-line `Caused by:` report you saw earlier.

To branch on a specific concrete error after the fact, `downcast_ref`:

```rust
use anyhow::Result;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("user {id} not found")]
struct UserNotFound { id: u32 }

fn find_user(id: u32) -> Result<String> {
    if id != 42 {
        return Err(UserNotFound { id }.into()); // typed error -> anyhow::Error
    }
    Ok("Ada".to_string())
}

fn main() {
    if let Err(e) = find_user(7) {
        // Recover the original concrete type.
        if let Some(unf) = e.downcast_ref::<UserNotFound>() {
            println!("typed recovery: missing id {}", unf.id);
        }
    }
}
```

Output:

```text
typed recovery: missing id 7
```

This is exactly the TypeScript `e instanceof ConfigError` pattern, except Rust checks the type at runtime through a real type identity, not a fragile prototype chain.

---

## Key Differences

| Concern | TypeScript / JavaScript | `thiserror` (libraries) | `anyhow` (applications) |
| --- | --- | --- | --- |
| Error type | One `Error` class (or `unknown`) | A concrete enum/struct **you** define | One opaque `anyhow::Error` |
| Caller can `match`/branch | `instanceof` + manual discriminant | Yes, on enum variants | Only via `downcast_ref` |
| Boilerplate | Subclass + set `name`/`cause` | `#[derive(Error)]` writes it all | Nothing to define |
| Add context | `throw new Error(msg, { cause })` | (caller's job) | `.context` / `.with_context` |
| Auto-convert with `?` | `throw` accepts anything | `#[from]` generates `From` | absorbs any `Error` type |
| Runtime cost | runtime classes | **zero** (compile-time codegen) | small heap allocation per error |
| When to use | always (no choice) | public API surface | binaries, scripts, tests, `main` |

The decision rule, in one sentence: **libraries return `thiserror` enums so their callers keep choices; applications use `anyhow` because they are the end of the line and only need to report.**

> **Warning:** Do not put `anyhow::Error` in a *public library* return type. It erases the variants, so your library's users can no longer `match` on what went wrong; they would be stuck with `downcast_ref` guesses. Reserve `anyhow` for the application that *consumes* libraries. More on this split in [Best Practices](/08-error-handling/08-best-practices/).

A second key difference from TypeScript: in TS, `try { ... } catch (e) { switch (e.kind) }` is the only tool, and the compiler never checks that you covered every kind. With a `thiserror` enum, `match` is exhaustive: add a variant and every `match` that forgot it stops compiling. That is the same exhaustiveness you get from any Rust enum (see [Control Flow](/04-control-flow/)).

---

## Common Pitfalls

### Pitfall 1: Trying to `match` directly on an `anyhow::Error`

Because `anyhow::Error` is opaque, you cannot pattern-match its "variants"; it has none.

```rust
use anyhow::{anyhow, Result};

#[derive(Debug)]
enum MyError { TooBig, TooSmall }

fn check(n: i32) -> Result<()> {
    if n > 100 { return Err(anyhow!("too big")); }
    Ok(())
}

fn main() {
    if let Err(e) = check(200) {
        // does not compile (error[E0308]: mismatched types)
        match e {
            MyError::TooBig => println!("big"),
            MyError::TooSmall => println!("small"),
        }
    }
}
```

The real compiler error:

```text
error[E0308]: mismatched types
  --> src/main.rs:15:13
   |
 4 | enum MyError { TooBig, TooSmall }
   |                ------ unit variant defined here
...
14 |         match e {
   |               - this expression has type `anyhow::Error`
15 |             MyError::TooBig => println!("big"),
   |             ^^^^^^^^^^^^^^^ expected `Error`, found `MyError`
```

**Fix:** if you genuinely need to branch on kinds, your error should be a `thiserror` enum (not `anyhow`), or you recover the concrete type with `e.downcast_ref::<MyError>()`.

### Pitfall 2: Forgetting `#[from]`, then using `?`

A `thiserror` variant only converts automatically if you ask it to with `#[from]`. Without it, `?` has no `From` impl to use.

```rust
use thiserror::Error;

#[derive(Debug, Error)]
enum AppError {
    #[error("parse failed")]
    Parse(std::num::ParseIntError), // no #[from]
}

fn parse(s: &str) -> Result<i32, AppError> {
    let n: i32 = s.parse()?; // tries ParseIntError -> AppError via From
    Ok(n)
}
```

The real compiler error:

```text
error[E0271]: type mismatch resolving `<i32 as FromStr>::Err == AppError`
  --> src/main.rs:10:20
   |
10 |     let n: i32 = s.parse()?; // tries ParseIntError -> AppError via From
   |                    ^^^^^ expected `AppError`, found `ParseIntError`
```

**Fix:** add `#[from]` to the field — `Parse(#[from] std::num::ParseIntError)` — which generates the `From` impl that `?` needs. (The `?`/`From` relationship is the whole subject of [The `?` Operator](/08-error-handling/01-question-mark/).)

### Pitfall 3: `#[from]` on two variants wrapping the same type

You can only have **one** `From<T>` impl for a given `T`. Putting `#[from] std::io::Error` on two different variants creates conflicting impls and will not compile.

**Fix:** keep `#[from]` on at most one variant per source type; mark the others with `#[source]` and construct them explicitly with `.map_err(...)`. Handling several overlapping error sources is covered in depth in [Multiple Error Types](/08-error-handling/07-multiple-errors/).

### Pitfall 4: `.context()` that allocates on the happy path

`.context(format!("..."))` builds the string **every time**, even when the `Result` is `Ok`. On a hot path that is wasted work.

**Fix:** use `.with_context(|| format!("..."))` so the `format!` only runs on the error branch. Reserve the eager `.context("literal")` for cheap string literals.

### Pitfall 5: Expecting `thiserror::Error` to give you a stack trace

Unlike a JavaScript `Error`, which captures `.stack` automatically, a `thiserror` error carries only what you put in its fields: the message and its `source()`. There is no implicit backtrace.

**Fix:** for backtraces in applications, enable `anyhow`'s `backtrace` feature and run with `RUST_BACKTRACE=1`; `anyhow::Error` captures one at creation. For libraries, expose the `source()` chain and let the application decide.

---

## Best Practices

- **Libraries → `thiserror`; applications → `anyhow`.** This is the single most important rule. A crate published to crates.io should expose typed errors; a binary or service should funnel everything into `anyhow`.
- **One error enum per module/layer is usually enough.** Over-splitting into dozens of tiny enums is as bad as one giant one. Granularity guidance lives in [Best Practices](/08-error-handling/08-best-practices/).
- **Prefer `#[from]` for the common conversions** so `?` is frictionless, and `#[source]` when you need the cause chain without an automatic conversion.
- **Add context at boundaries.** Wrap an error with `.with_context(...)` when it crosses a meaningful layer ("while reading config", "while connecting to the database"), not on every line.
- **Use `#[error(transparent)]` for pass-through variants.** It forwards `Display` and `source()` to the inner error so you do not invent a redundant message:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
enum DataError {
    #[error("record {index} is malformed")]
    Malformed { index: usize },

    // No message of its own: delegates Display + source to the inner io::Error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn first_line(path: &str) -> Result<String, DataError> {
    let text = std::fs::read_to_string(path)?; // io::Error -> DataError::Io
    let line = text.lines().next().ok_or(DataError::Malformed { index: 0 })?;
    Ok(line.to_string())
}

fn main() {
    if let Err(e) = first_line("/definitely/missing.txt") {
        println!("error: {e}"); // prints the io error's own message
    }
}
```

Output:

```text
error: No such file or directory (os error 2)
```

- **Return `anyhow::Result<()>` from `main`** in binaries. The `?` operator then propagates anything, and Rust prints the `Caused by:` report and sets a non-zero exit code for free.
- **`anyhow::Error` requires `Send + Sync + 'static`.** That is exactly what async runtimes and thread pools need (see [Async](/11-async/)), so it composes well with `tokio`. If a hand-rolled error type is not `Send + Sync`, `?` into an `anyhow::Result` will fail; derive your errors with `thiserror` and they will satisfy these bounds automatically.

---

## Real-World Example

A two-layer program: a `store` module is the "library" with a precise `thiserror` enum, and the application layer uses `anyhow` to add context, recover specific variants, and report the full chain.

```rust
// ---- "library" layer: precise, typed errors with thiserror ----
mod store {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum StoreError {
        #[error("user {0} does not exist")]
        NotFound(u32),

        #[error("database is offline")]
        Offline,

        #[error("corrupt row")]
        Corrupt(#[from] std::num::ParseIntError),
    }

    #[derive(Debug)]
    pub struct User {
        pub id: u32,
        pub age: u32,
    }

    pub fn fetch(id: u32) -> Result<User, StoreError> {
        match id {
            0 => Err(StoreError::Offline),
            42 => Ok(User { id, age: "thirty".parse()? }), // bad data -> Corrupt
            7 => Ok(User { id, age: "30".parse()? }),
            _ => Err(StoreError::NotFound(id)),
        }
    }
}

// ---- "application" layer: anyhow for ergonomic propagation + context ----
use anyhow::{Context, Result};
use store::StoreError;

fn greet(id: u32) -> Result<String> {
    let user = store::fetch(id)
        .with_context(|| format!("while greeting user {id}"))?;
    Ok(format!("Hello user {} (age {})", user.id, user.age))
}

fn run() -> Result<()> {
    println!("{}", greet(7)?);

    // Recover a specific typed variant after the fact.
    if let Err(e) = greet(99) {
        if let Some(StoreError::NotFound(missing)) = e.downcast_ref::<StoreError>() {
            println!("(handled) no such user: {missing}");
        }
    }

    // A corrupt-row failure carries its source through the context layer.
    if let Err(e) = greet(42) {
        println!("chain for id 42:");
        for cause in e.chain() {
            println!("  - {cause}");
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}
```

Output:

```text
Hello user 7 (age 30)
(handled) no such user: 99
chain for id 42:
  - while greeting user 42
  - corrupt row
  - invalid digit found in string
```

Notice the three-level chain for id 42: the application's context (`while greeting user 42`), the library's variant message (`corrupt row`), and the original standard-library cause (`invalid digit found in string`). Each layer added exactly the information it knew about, and `thiserror`'s `#[from]` stitched the `source()` together automatically.

---

## Further Reading

- [`anyhow` crate documentation](https://docs.rs/anyhow) — `Context`, `anyhow!`, `bail!`, `ensure!`, `Error::chain`, `downcast_ref`.
- [`thiserror` crate documentation](https://docs.rs/thiserror) — every attribute: `#[error]`, `#[from]`, `#[source]`, `#[error(transparent)]`.
- [Rust By Example: Boxing errors](https://doc.rust-lang.org/rust-by-example/error/multiple_error_types/boxing_errors.html) — the `Box<dyn Error>` approach these crates build on.
- [The `?` Operator](/08-error-handling/01-question-mark/) — the `From`-based conversion that `#[from]` generates for `?`.
- [Custom Errors](/08-error-handling/04-custom-errors/) and [The `Error` Trait](/08-error-handling/05-error-trait/) — what `thiserror` writes for you, done by hand.
- [Multiple Error Types](/08-error-handling/07-multiple-errors/) — aggregating several error sources, `#[from]` conflicts, and `Box<dyn Error>`.
- [Best Practices](/08-error-handling/08-best-practices/) — the libraries-vs-applications design decision in full.
- [Macros](/14-macros/) — why `#[derive(Error)]` is compile-time codegen, not a runtime decorator.
- [Generics & Traits](/09-generics-traits/) — the `Error` trait and trait bounds (`Send + Sync + 'static`) `anyhow` relies on.

---

## Exercises

### Exercise 1

**Difficulty:** Beginner

**Objective:** Replace a hand-written error type with a `thiserror` enum that carries a source chain.

**Instructions:** Write a function `sum_first_column(text: &str) -> Result<i64, CsvError>` that parses the first comma-separated column of each line as an `i64` and returns the sum. Define `CsvError` with `thiserror` so that (a) an I/O-style variant uses `#[from] std::io::Error`, and (b) a parse failure variant records the **1-based line number** *and* keeps the original `ParseIntError` as its `source()`. Demonstrate the `source()` chain on bad input.

<details>
<summary>Solution</summary>

```rust
use thiserror::Error;

#[derive(Debug, Error)]
enum CsvError {
    #[error("file error")]
    Io(#[from] std::io::Error),

    #[error("bad number on line {line}")]
    Parse {
        line: usize,
        // #[source] keeps the cause chain WITHOUT generating a From impl
        // (we need the line number, so we build this variant manually).
        #[source]
        source: std::num::ParseIntError,
    },
}

fn sum_first_column(text: &str) -> Result<i64, CsvError> {
    let mut total = 0i64;
    for (i, line) in text.lines().enumerate() {
        let first = line.split(',').next().unwrap_or("");
        let n: i64 = first
            .parse()
            .map_err(|e| CsvError::Parse { line: i + 1, source: e })?;
        total += n;
    }
    Ok(total)
}

fn main() {
    println!("{:?}", sum_first_column("10,a\n20,b")); // Ok(30)

    let e = sum_first_column("10\nxx").unwrap_err();
    println!("{e}");
    if let Some(src) = std::error::Error::source(&e) {
        println!("  caused by: {src}");
    }
}
```

Output:

```text
Ok(30)
bad number on line 2
  caused by: invalid digit found in string
```

</details>

### Exercise 2

**Difficulty:** Intermediate

**Objective:** Combine a `thiserror` library error with an `anyhow` application that recovers a specific variant.

**Instructions:** Define `HttpError` (a `thiserror` struct holding a `status: u16`). Write `fetch(url: &str) -> anyhow::Result<String>` that fails with a `404` `HttpError` wrapped in context. In `main` (returning `anyhow::Result<()>`), call `fetch`, and if the error downcasts to an `HttpError` with status `404`, treat it as recoverable (print a message and return `Ok(())`); otherwise propagate it.

<details>
<summary>Solution</summary>

```rust
use anyhow::{Context, Result};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("HTTP {status}")]
struct HttpError {
    status: u16,
}

fn fetch(url: &str) -> Result<String> {
    if url.is_empty() {
        return Err(HttpError { status: 400 }.into());
    }
    // Simulate a 404, with a context layer describing the request.
    Err(HttpError { status: 404 }).context(format!("requesting {url}"))
}

fn main() -> Result<()> {
    match fetch("https://example.com/x") {
        Ok(body) => println!("{body}"),
        Err(e) => {
            // Recover the concrete typed error to decide if it is recoverable.
            if let Some(h) = e.downcast_ref::<HttpError>() {
                if h.status == 404 {
                    println!("not found (recovered, status {})", h.status);
                    return Ok(());
                }
            }
            return Err(e); // any other error propagates and is reported by main
        }
    }
    Ok(())
}
```

Output:

```text
not found (recovered, status 404)
```

</details>

### Exercise 3

**Difficulty:** Advanced

**Objective:** Use `downcast_ref` on an `anyhow::Error` to implement retry logic that depends on the underlying library error variant.

**Instructions:** Build an `api` module with `thiserror` enum `ApiError` having variants `RateLimited { retry_after_secs: u64 }`, `BadRequest(#[from] std::num::ParseIntError)`, and `Server { code: u16 }`, plus `fn call(attempt: u32) -> Result<u32, ApiError>` that returns `RateLimited` on attempt 0, succeeds on attempt 1, and returns a `Server` error otherwise. In the application, write `call_with_retry() -> anyhow::Result<u32>` that loops up to 3 attempts: on each error, add context, then `downcast_ref::<ApiError>()`; if it is `RateLimited`, log and retry; otherwise propagate. On the final failure, print the full `chain()`.

<details>
<summary>Solution</summary>

```rust
mod api {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum ApiError {
        #[error("rate limited; retry after {retry_after_secs}s")]
        RateLimited { retry_after_secs: u64 },

        #[error("bad request")]
        BadRequest(#[from] std::num::ParseIntError),

        #[error("server error {code}")]
        Server { code: u16 },
    }

    pub fn call(attempt: u32) -> Result<u32, ApiError> {
        match attempt {
            0 => Err(ApiError::RateLimited { retry_after_secs: 2 }),
            1 => Ok("123".parse::<u32>()?), // succeeds
            _ => Err(ApiError::Server { code: 500 }),
        }
    }
}

use anyhow::{Context, Result};
use api::ApiError;

fn call_with_retry() -> Result<u32> {
    for attempt in 0..3 {
        match api::call(attempt).context("calling upstream API") {
            Ok(v) => return Ok(v),
            Err(e) => match e.downcast_ref::<ApiError>() {
                Some(ApiError::RateLimited { retry_after_secs }) => {
                    println!("rate limited, would sleep {retry_after_secs}s; retrying");
                    continue;
                }
                _ => return Err(e), // non-retryable: propagate with context intact
            },
        }
    }
    anyhow::bail!("exhausted retries")
}

fn main() {
    match call_with_retry() {
        Ok(v) => println!("got {v}"),
        Err(e) => {
            eprintln!("failed: {e:#}");
            for cause in e.chain() {
                eprintln!("  - {cause}");
            }
        }
    }
}
```

Output:

```text
rate limited, would sleep 2s; retrying
got 123
```

</details>
