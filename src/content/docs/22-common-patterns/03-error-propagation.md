---
title: "The Error-Propagation Pattern"
description: "Layer typed errors in Rust: ? plus From carries failures up, thiserror types library errors, anyhow adds context at the app edge. No invisible JS exceptions."
---

Most Rust error code follows one repeating shape: each layer defines (or wraps) a typed error, the `?` operator carries failures upward by converting them through the `From` trait, and the outermost layer either reports or recovers. This file is about that *architecture*: how to layer errors, how `?` + `From` glue the layers together, and the discipline of using `thiserror` inside libraries while reaching for `anyhow` at application edges.

---

## Quick Overview

In TypeScript an exception is thrown once and travels up the whole call stack untouched until some `catch` happens to grab it. Rust has no exceptions: a failing function returns `Result<T, E>`, and every caller must visibly decide to handle it or pass it along. The `?` operator makes "pass it along" a one-character act, and because `?` converts the error type via `From` on the way out, you can give each layer its *own* error type that wraps the layer beneath it.

The **error-propagation pattern** is the convention that emerges from this: **`thiserror` for typed, matchable errors in libraries; `anyhow` for context-rich, opaque errors at the application edge; `? + From` to move between layers.** This is the design-pattern view. For the underlying mechanics (the `Result` type, the `?` operator, defining custom errors) see [Section 08: Error Handling](/08-error-handling/).

> **Note:** This page assumes you already know what `Result`, `?`, and a custom error enum *are*. If not, read [The `?` Operator](/08-error-handling/01-question-mark/) and [Custom Errors](/08-error-handling/04-custom-errors/) first, then come back for the architecture.

---

## TypeScript/JavaScript Example

A realistic data-access stack in TypeScript: a low-level line parser, a repository on top of it, and an application handler at the edge. Errors are thrown and bubble through every layer implicitly.

```typescript
// --- Lowest layer: parse a "key=value" record line ---
class ParseError extends Error {
  constructor(
    public line: number,
    public raw: string,
    options?: { cause?: unknown },
  ) {
    super(`line ${line}: malformed record '${raw}'`, options);
    this.name = "ParseError";
  }
}

function parseLine(lineNo: number, raw: string): { id: number; score: number } {
  const [idPart, scorePart] = raw.split("=");
  if (scorePart === undefined) throw new ParseError(lineNo, raw);
  const id = Number(idPart);
  const score = Number(scorePart);
  if (!Number.isInteger(id) || !Number.isInteger(score)) {
    throw new ParseError(lineNo, raw, {
      cause: new TypeError("not an integer"),
    });
  }
  return { id, score };
}

// --- Middle layer: a repository over a file ---
class RepoError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "RepoError";
  }
}

function find(path: string, wanted: number): { id: number; score: number } {
  let contents: string;
  try {
    contents = require("node:fs").readFileSync(path, "utf8");
  } catch (e) {
    // Re-wrap to add a layer of meaning. The original becomes `cause`.
    throw new RepoError(`could not read store file '${path}'`, { cause: e });
  }
  const lines = contents.split("\n").filter((l) => l.length > 0);
  for (let i = 0; i < lines.length; i++) {
    const rec = parseLine(i + 1, lines[i]); // ParseError propagates untyped
    if (rec.id === wanted) return rec;
  }
  throw new RepoError(`no record found for id ${wanted}`);
}

// --- Application edge: report or recover ---
try {
  const rec = find("store.txt", 2);
  console.log(`found: ${JSON.stringify(rec)}`);
} catch (e) {
  // What KIND of error is this? You must reconstruct that by inspection.
  if (e instanceof RepoError) console.error(`repo failure: ${e.message}`);
  else if (e instanceof ParseError) console.error(`bad data at line ${e.line}`);
  else throw e; // unknown — re-throw
}
```

Two things to notice, because Rust inverts both:

1. **Propagation is invisible.** `parseLine` throws and the exception silently skips through `find`'s body — nothing in `find`'s signature mentions that it can fail with a `ParseError`. The type system does not track it.
2. **The error type is erased at the boundary.** Every `catch` receives `unknown`; to branch on the failure you re-discover its type with `instanceof`. If a refactor changes what `find` can throw, no compiler tells the caller.

---

## Rust Equivalent

The same three layers. Each has a typed error; `?` converts as it propagates; the edge uses `anyhow` to add context and report.

```rust
// ===== A "library" with layered, typed errors (thiserror) =====
mod store {
    use thiserror::Error;

    // Lowest layer: parsing one record line.
    #[derive(Debug, Error)]
    pub enum ParseError {
        #[error("line {line}: expected `key=value`, got `{raw}`")]
        Malformed { line: usize, raw: String },
        #[error("line {line}: invalid number")]
        BadNumber {
            line: usize,
            #[source]
            source: std::num::ParseIntError,
        },
    }

    // Middle layer: the repository. It WRAPS lower-layer errors instead of
    // leaking them, so callers see one coherent error type.
    #[derive(Debug, Error)]
    pub enum RepoError {
        // `#[from]` derives `From<std::io::Error>` and wires up `source()`.
        #[error("could not read store file")]
        Io(#[from] std::io::Error),

        // No `#[from]` here: we want to attach the file path at THIS layer,
        // so we wrap ParseError as a `#[source]` and build the variant by hand.
        #[error("malformed store file `{path}`")]
        Parse {
            path: String,
            #[source]
            source: ParseError,
        },

        #[error("no record found for id {0}")]
        NotFound(u64),
    }

    #[derive(Debug)]
    pub struct Record {
        pub id: u64,
        pub score: i64,
    }

    fn parse_line(line_no: usize, raw: &str) -> Result<Record, ParseError> {
        let (id_part, score_part) = raw.split_once('=').ok_or(ParseError::Malformed {
            line: line_no,
            raw: raw.to_string(),
        })?;
        let id = id_part
            .trim()
            .parse()
            .map_err(|source| ParseError::BadNumber { line: line_no, source })?;
        let score = score_part
            .trim()
            .parse()
            .map_err(|source| ParseError::BadNumber { line: line_no, source })?;
        Ok(Record { id, score })
    }

    pub fn find(path: &str, wanted: u64) -> Result<Record, RepoError> {
        // `?` converts io::Error -> RepoError::Io automatically (via #[from]).
        let contents = std::fs::read_to_string(path)?;
        for (i, line) in contents.lines().enumerate() {
            // Add the path as context when crossing the parse -> repo boundary.
            let rec = parse_line(i + 1, line).map_err(|source| RepoError::Parse {
                path: path.to_string(),
                source,
            })?;
            if rec.id == wanted {
                return Ok(rec);
            }
        }
        Err(RepoError::NotFound(wanted))
    }
}

// ===== The "application" edge (anyhow) =====
use anyhow::{Context, Result};

fn run() -> Result<()> {
    // Write a store file with one deliberately broken line.
    let path = std::env::temp_dir().join("probe_store.txt");
    std::fs::write(&path, "1=100\n2=oops\n3=300\n")?;
    let path = path.to_string_lossy().into_owned();

    // `?` converts RepoError -> anyhow::Error (anyhow accepts any std error).
    // `.with_context` adds a human-readable layer at the edge.
    let rec = store::find(&path, 2)
        .with_context(|| format!("looking up record 2 in `{path}`"))?;
    println!("found id={} score={}", rec.id, rec.score);
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        // `{:#}` prints the whole context chain on one line.
        eprintln!("Error: {err:#}");
        eprintln!("---- debug ----");
        // `{:?}` (Debug) prints the chain stacked, plus a backtrace section.
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}
```

Add the two dependencies in your probe project:

```bash
cargo add thiserror anyhow
```

Running it produces (the temp path will differ on your machine):

```text
Error: looking up record 2 in `/tmp/probe_store.txt`: malformed store file `/tmp/probe_store.txt`: line 2: invalid number: invalid digit found in string
---- debug ----
looking up record 2 in `/tmp/probe_store.txt`

Caused by:
    0: malformed store file `/tmp/probe_store.txt`
    1: line 2: invalid number
    2: invalid digit found in string
```

That four-level chain (*edge context → repo layer → parse layer → the raw `ParseIntError`*) is assembled for free because every layer wired its lower-level cause in as a `source`.

---

## Detailed Explanation

**`?` is `From`-powered propagation.** When you write `let x = fallible()?;`, Rust desugars it to roughly:

```rust
// What `let contents = std::fs::read_to_string(path)?;` expands to:
let contents = match std::fs::read_to_string(path) {
    Ok(value) => value,
    // `From::from` converts the error to the function's declared error type.
    Err(e) => return Err(From::from(e)),
};
```

So `?` does two jobs: it early-returns on `Err`, and it calls `From::from` to convert the inner error into the function's return error type. That single conversion step is the entire mechanism behind layered errors: you do not have to convert manually as long as a `From` impl exists.

**`#[from]` generates the `From` impl for you.** In the example, `#[error("...")] Io(#[from] std::io::Error)` makes `thiserror` emit `impl From<std::io::Error> for RepoError`. That is why `std::fs::read_to_string(path)?` inside `find` "just works" — `?` finds the generated `From` and uses it. The same derive also implements `Error::source()` so the `io::Error` shows up beneath `RepoError::Io` in the cause chain.

**When you need to add data at the boundary, use `#[source]` + `map_err`.** A `#[from]` conversion is automatic but *loses the opportunity to attach context*, because `From::from` only receives the inner error. In `find`, we want to record *which file* failed to parse, so we cannot use `#[from]` for `RepoError::Parse`; instead we `map_err(|source| RepoError::Parse { path: ..., source })`. The `#[source]` attribute tells `thiserror` "this field is the cause," so it still appears in the chain.

**`anyhow` at the edge.** `run` returns `anyhow::Result<()>` (an alias for `Result<(), anyhow::Error>`). `anyhow::Error` implements `From<E>` for *any* `E: std::error::Error + Send + Sync + 'static`, which is why `store::find(...)?` works even though `find` returns a `RepoError` and `run` returns an `anyhow::Error`. `.with_context(...)` wraps the error in a new layer whose message is the closure's output and whose `source` is the original. Exactly like throwing a new `Error` with `{ cause: e }` in TypeScript, but type-checked and zero-boilerplate.

**The closure form `with_context(|| ...)` is lazy.** It only builds the message string on the error path. There is also `.context("static message")` for cheap, always-allocated context. Prefer `with_context` when the message needs `format!`.

---

## Key Differences

| Concern | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Propagation | Implicit `throw` skips through every frame | Explicit: `?` at each call site, visible in the signature |
| What can a function fail with? | Not in the type — `throws` is untracked | In the return type: `Result<T, MyError>` |
| Carrying errors up | Automatic (exception unwinding) | `?` + a `From` conversion per layer |
| Branching on error kind | `instanceof` / discriminant at runtime | `match` on an enum, checked at compile time |
| Adding context | `new Error(msg, { cause })` | `.context(...)` / a wrapping enum variant with `#[source]` |
| Cause chain | `error.cause` (walk it manually) | `Error::source()` (anyhow/thiserror walk it for you) |
| Library vs app | Same `Error` everywhere | `thiserror` (typed, matchable) vs `anyhow` (opaque, contextual) |

The headline conceptual difference: **Rust makes the set of possible failures part of the function's type.** A TypeScript caller cannot tell from a signature whether `find` throws `RepoError`, `ParseError`, or something from `fs`. A Rust caller reads `-> Result<Record, RepoError>` and knows exactly what to handle.

**The `thiserror`-vs-`anyhow` split is the heart of the pattern:**

- **Libraries return `thiserror` enums.** Your consumers may want to *recover* from specific failures (retry on a timeout, fall back on a cache miss). That requires a concrete, matchable type. An opaque error would force them to parse your error messages, a brittle anti-pattern.
- **Applications return `anyhow::Error`.** A binary's `main` usually just logs the failure and exits non-zero. It does not benefit from a giant hand-written enum that unions every dependency's error; it benefits from cheap context and a readable cause chain. `anyhow` gives exactly that.

> **Tip:** A useful mnemonic: *"`thiserror` is for code other code calls; `anyhow` is for code humans run."* The same crate can do both — a library crate's internal binary (an example, a small CLI front-end) can use `anyhow` even while the library API uses `thiserror`.

---

## Common Pitfalls

### Using `?` when no `From` impl connects the two error types

```rust
#[derive(Debug)]
struct AppError(String);

fn read_count() -> Result<u32, AppError> {
    // does not compile (error[E0277]): io::Error has no `From` into AppError
    let text = std::fs::read_to_string("data.txt")?;
    Ok(text.len() as u32)
}

fn main() {
    let _ = read_count();
}
```

The real compiler error:

```text
error[E0277]: `?` couldn't convert the error to `AppError`
 --> src/main.rs:6:51
  |
4 | fn read_count() -> Result<u32, AppError> {
  |                    --------------------- expected `AppError` because of this
5 |     // does not compile (error[E0277]): io::Error has no `From` into AppError
6 |     let text = std::fs::read_to_string("data.txt")?;
  |                -----------------------------------^ the trait `From<std::io::Error>` is not implemented for `AppError`
  |                |
  |                this can't be annotated with `?` because it has type `Result<_, std::io::Error>`
  |
note: `AppError` needs to implement `From<std::io::Error>`
 --> src/main.rs:2:1
  |
2 | struct AppError(String);
  | ^^^^^^^^^^^^^^^
  = note: the question mark operation (`?`) implicitly performs a conversion on the error value using the `From` trait

For more information about this error, try `rustc --explain E0277`.
```

The compiler tells you exactly what is missing: implement `From<std::io::Error> for AppError`, or (in a library) add a `#[from]` variant, or (at an app edge) return `anyhow::Result` instead.

### Trying to `match` on an `anyhow::Error`

`anyhow::Error` is *opaque*: it has no variants to match. If at some boundary you need to recover from a *specific* underlying error you wrapped, use `downcast_ref`:

```rust playground
use anyhow::{anyhow, Result};
use thiserror::Error;

#[derive(Debug, Error)]
enum DbError {
    #[error("row {0} not found")]
    NotFound(u64),
    #[error("connection lost")]
    ConnectionLost,
}

fn fetch(id: u64) -> Result<String> {
    Err(anyhow!(DbError::NotFound(id))) // a typed error, carried by anyhow
}

fn main() {
    match fetch(42) {
        Ok(name) => println!("got {name}"),
        Err(err) => {
            // You cannot `match err { DbError::... }` — anyhow::Error is opaque.
            // Recover the concrete type with downcast_ref:
            if let Some(DbError::NotFound(id)) = err.downcast_ref::<DbError>() {
                println!("recovered: row {id} was missing (will retry)");
            } else {
                eprintln!("unhandled: {err:#}");
            }
        }
    }
}
```

Output:

```text
recovered: row 42 was missing (will retry)
```

But notice the friction: you *threw away* the type by using `anyhow`, then had to dynamically recover it. That is the signal that this code wanted a `thiserror` enum, not `anyhow`. **Reach for `downcast_ref` rarely; if you do it routinely, your boundary should return a typed error.**

### Putting `anyhow` in a public library API

If your published crate's functions return `anyhow::Result<T>`, every consumer is forced to depend on `anyhow` and loses the ability to handle your failures programmatically. Keep `anyhow::Error` out of public signatures; expose a `thiserror` enum instead. The decision tree is covered further in [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/).

### Forgetting that `?` returns early — including in the middle of cleanup

Because `?` is an early `return`, a `?` that fires before a manual cleanup line will skip that cleanup. Rust's answer is not "remember to clean up before every `?`" but the **RAII / Drop pattern**: tie cleanup to a value's scope so it runs no matter how the function exits. See [RAII and Drop Guards](/22-common-patterns/10-raii-pattern/).

---

## Best Practices

- **One error type per layer, each wrapping the one below.** The parser returns `ParseError`; the repository returns `RepoError` that *contains* a `ParseError`; the app collapses everything into `anyhow::Error`. This keeps each layer's error vocabulary focused.
- **Use `#[from]` for pure pass-through, `#[source]` + `map_err` when you add data.** `#[from]` is great for "this layer can also fail with the layer below, unchanged." Switch to `#[source]` the moment you want to attach context (a path, an ID, a request) at the boundary.
- **`#[error(transparent)]` for a variant that should be indistinguishable from its inner error.** Use it when a variant exists only to carry a foreign error through without adding any message of its own:

  ```rust
  use thiserror::Error;

  #[derive(Debug, Error)]
  #[error("TOML parse error: {0}")]
  pub struct TomlError(pub String);

  #[derive(Debug, Error)]
  pub enum ConfigError {
      #[error("failed to read config from `{path}`")]
      Read {
          path: String,
          #[source]
          source: std::io::Error,
      },
      // Delegate Display + source entirely to the inner error.
      #[error(transparent)]
      Toml(#[from] TomlError),
      #[error("invalid config: port {0} is out of range (1..=65535)")]
      PortOutOfRange(u32),
  }
  ```

- **Add context at the edge with `with_context`, not by reformatting the message.** `.with_context(|| format!("starting server with `{path}`"))` preserves the underlying error as a `source`; rewriting it as `format!("{e}: starting server")` flattens the chain and loses structure.
- **Print `{:#}` for users, `{:?}` for logs.** `anyhow`'s alternate `Display` (`{:#}`) gives a compact one-line chain; `Debug` (`{:?}`) gives the stacked `Caused by:` list (plus a backtrace when `RUST_BACKTRACE=1`). For comparison:

  ```text
  Display  {}:  initializing the database
  Alt      {:#}: initializing the database: reading the seed list: No such file or directory (os error 2)
  ---- Debug {:?} ----
  initializing the database

  Caused by:
      0: reading the seed list
      1: No such file or directory (os error 2)
  ```

- **`Box<dyn Error>` is the dependency-free middle ground.** Before reaching for `anyhow`, know that the standard library already lets `?` converge many error types into `Box<dyn std::error::Error>`:

  ```rust
  use std::error::Error;

  fn count_lines(path: &str) -> Result<usize, Box<dyn Error>> {
      let text = std::fs::read_to_string(path)?; // io::Error -> Box<dyn Error>
      let _first: u32 = text.lines().next().unwrap_or("0").trim().parse()?; // and ParseIntError
      Ok(text.lines().count())
  }
  ```

  Calling `count_lines("/no/such/file")` yields an `Err` whose message is `No such file or directory (os error 2)`. It works because every `E: Error + 'static` converts into `Box<dyn Error>`. `anyhow` is essentially this plus context, backtraces, and `downcast` — but if you cannot add a dependency, `Box<dyn Error>` at a binary's `main` is perfectly idiomatic.

> **Warning:** Do not derive `Clone` on error types reflexively. Many wrapped errors (like `std::io::Error`) are not `Clone`, and forcing it usually means stringifying the cause and losing the chain. Errors are meant to be *moved* up and consumed once.

---

## Real-World Example

A configuration loader as a small library, consumed by an application `main`. The library uses a layered `thiserror` enum (IO, parse via `#[from]` + `transparent`, and validation); the binary uses `anyhow` to add a startup-context layer and let `main` report. This mirrors how a real service wires `config` loading to `fn main`.

```rust
mod config {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum ConfigError {
        #[error("failed to read config from `{path}`")]
        Read {
            path: String,
            #[source]
            source: std::io::Error,
        },

        // `transparent` makes this variant display exactly like the inner error.
        #[error(transparent)]
        Toml(#[from] TomlError),

        #[error("invalid config: port {0} is out of range (1..=65535)")]
        PortOutOfRange(u32),
    }

    // Stand-in for a real crate's parse error (e.g. `toml::de::Error`).
    #[derive(Debug, Error)]
    #[error("TOML parse error: {0}")]
    pub struct TomlError(pub String);

    #[derive(Debug)]
    pub struct Config {
        pub port: u16,
    }

    pub fn load(path: &str) -> Result<Config, ConfigError> {
        let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_string(),
            source,
        })?;
        // Pretend-parse: expect a single line `port = N`.
        let value: u32 = raw
            .trim()
            .strip_prefix("port = ")
            .ok_or_else(|| TomlError(format!("expected `port = N`, got `{}`", raw.trim())))?
            .parse()
            // `?` converts TomlError -> ConfigError::Toml via the derived #[from].
            .map_err(|_| TomlError("port is not a number".into()))?;
        if !(1..=65535).contains(&value) {
            return Err(ConfigError::PortOutOfRange(value));
        }
        Ok(Config { port: value as u16 })
    }
}

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let path = std::env::temp_dir().join("probe_config.toml");
    std::fs::write(&path, "port = 70000\n")?; // out-of-range on purpose
    let path = path.to_string_lossy().into_owned();

    // The library's typed ConfigError flows into anyhow::Error via `?`,
    // and we add an application-level context layer.
    let cfg = config::load(&path).with_context(|| format!("starting server with `{path}`"))?;
    println!("listening on port {}", cfg.port);
    Ok(())
}
```

Because `main` returns `anyhow::Result<()>`, Rust prints the returned error using `anyhow`'s formatting and exits with a non-zero status. Real output:

```text
Error: starting server with `/tmp/probe_config.toml`

Caused by:
    invalid config: port 70000 is out of range (1..=65535)
```

The library kept a *typed* error (a downstream caller could `match` on `ConfigError::PortOutOfRange` to suggest a fix), while the binary got a *contextual* report for free: the pattern, working end to end. This style scales directly to web handlers and database layers; see how it threads through real services in [Section 16: Web APIs](/16-web-apis/).

---

## Further Reading

- [The `?` Operator](/08-error-handling/01-question-mark/): the propagation primitive this pattern is built on.
- [Custom Errors](/08-error-handling/04-custom-errors/) and [The `Error` Trait](/08-error-handling/05-error-trait/): defining error types and the `source()` chain by hand.
- [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/) — the two crates' full APIs and the "which one, when?" decision.
- [Error Handling: Best Practices](/08-error-handling/08-best-practices/): broader guidance on `unwrap`, `expect`, and `panic` boundaries.
- [The Newtype Pattern](/22-common-patterns/01-newtype/): wrapping a foreign error type to add your own `From` impls without hitting the orphan rule.
- [RAII and Drop Guards](/22-common-patterns/10-raii-pattern/) — ensuring cleanup runs even when `?` returns early.
- [Generics & Traits](/09-generics-traits/): `From`, `Into`, and the trait machinery behind `?`.
- [Section 23: The Ecosystem](/23-ecosystem/): where crates like `thiserror`, `anyhow`, `eyre`, and `snafu` fit in the wider landscape.
- Official: [`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html), [The `?` operator (Rust Book)](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html#a-shortcut-for-propagating-errors-the--operator), [`thiserror` docs](https://docs.rs/thiserror), [`anyhow` docs](https://docs.rs/anyhow).

---

## Exercises

### Exercise 1: Make `?` work across two source errors

**Difficulty:** Easy

**Objective:** Define a single layered error type so a function can use `?` over both `std::io::Error` and a parse failure.

**Instructions:**

1. Write `fn load_count(path: &str) -> Result<u32, LoadError>` that reads a file and parses its trimmed contents into a `u32`.
2. Define `LoadError` with `thiserror` so the IO failure records the `path` (as a `#[source]`) and the parse failure records the offending string.
3. Make the two fallible steps propagate cleanly. (`#[from]` cannot record the path — think about why.)

```rust
use thiserror::Error;

#[derive(Debug, Error)]
enum LoadError {
    // TODO: an Io variant that keeps the path + the io::Error source
    // TODO: a Parse variant that keeps the bad input string
}

fn load_count(path: &str) -> Result<u32, LoadError> {
    /* ??? */
}
```

<details>
<summary>Solution</summary>

```rust playground
use thiserror::Error;

#[derive(Debug, Error)]
enum LoadError {
    #[error("could not read `{path}`")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`{0}` is not a valid count")]
    Parse(String),
}

fn load_count(path: &str) -> Result<u32, LoadError> {
    // `#[from]` would discard the path, so we map_err to attach it here.
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_string(),
        source,
    })?;
    let trimmed = text.trim();
    let count = trimmed
        .parse()
        .map_err(|_| LoadError::Parse(trimmed.to_string()))?;
    Ok(count)
}

fn main() {
    match load_count("/no/such/file") {
        Ok(n) => println!("count = {n}"),
        Err(e) => println!("error: {e}"),
    }
}
```

Running against a missing file prints:

```text
error: could not read `/no/such/file`
```

</details>

### Exercise 2: Bridge a library error into an `anyhow` edge with selective recovery

**Difficulty:** Medium

**Objective:** Consume a `thiserror`-based library error at an application edge, recovering from one variant and propagating the other with context.

**Instructions:**

1. Given a `CacheError` enum with `Miss(String)` and `Unavailable` variants, write `fn handle(key: &str) -> anyhow::Result<String>`.
2. On a cache *miss*, recover by returning a default value (do not propagate).
3. On *unavailable*, attach context and propagate as an `anyhow::Error`.

<details>
<summary>Solution</summary>

```rust playground
use anyhow::{Context, Result};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("cache miss for key `{0}`")]
    Miss(String),
    #[error("cache backend unavailable")]
    Unavailable,
}

fn get(key: &str) -> Result<String, CacheError> {
    if key == "warm" {
        Ok("hot-value".to_string())
    } else {
        Err(CacheError::Miss(key.to_string()))
    }
}

fn handle(key: &str) -> Result<String> {
    match get(key) {
        Ok(v) => Ok(v),
        // A miss is recoverable: fall back to a default.
        Err(CacheError::Miss(_)) => Ok("default-value".to_string()),
        // A real outage is fatal at this edge: add context and propagate.
        Err(e @ CacheError::Unavailable) => {
            Err(e).with_context(|| format!("while serving key `{key}`"))
        }
    }
}

fn main() -> Result<()> {
    println!("{}", handle("warm")?);
    println!("{}", handle("cold")?);
    Ok(())
}
```

Output:

```text
hot-value
default-value
```

The key insight: because the *library* returns a typed `CacheError`, the edge can `match` and decide per-variant. Had `get` returned `anyhow::Result`, that branch would have required a fragile `downcast_ref`.

</details>

### Exercise 3: Recover a typed error from `anyhow` at the boundary

**Difficulty:** Medium

**Objective:** Carry a typed error through `anyhow`, then recover it at the edge with `downcast_ref` to drive a retry decision.

**Instructions:**

1. Define a `RateLimited { retry_after_secs: u64 }` error with `thiserror`.
2. Write `fn call_api() -> anyhow::Result<()>` that returns it via `anyhow!(...)`.
3. In `main`, recover the concrete `RateLimited` from the `anyhow::Error` and print the back-off duration; otherwise print a fatal message.

<details>
<summary>Solution</summary>

```rust playground
use anyhow::{anyhow, Result};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("rate limited: retry after {retry_after_secs}s")]
struct RateLimited {
    retry_after_secs: u64,
}

fn call_api() -> Result<()> {
    Err(anyhow!(RateLimited { retry_after_secs: 30 }))
}

fn main() {
    if let Err(err) = call_api() {
        // Recover the concrete type that was wrapped by anyhow.
        if let Some(rl) = err.downcast_ref::<RateLimited>() {
            println!("backing off for {}s", rl.retry_after_secs);
        } else {
            eprintln!("fatal: {err:#}");
        }
    }
}
```

Output:

```text
backing off for 30s
```

This works, but it is the exception, not the rule: if you find yourself `downcast_ref`-ing routinely, the boundary should expose a typed `thiserror` enum so callers can `match` directly — exactly the trade-off this pattern is about.

</details>
