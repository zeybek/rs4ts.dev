---
title: "The `Error` Trait"
description: "Rust's std::error::Error is a trait, not a class like TypeScript's Error: its Debug and Display bounds, the source() cause chain, and Box<dyn Error>."
---

In TypeScript, `Error` is a runtime class you `throw`. In Rust, `std::error::Error` is a **trait** that custom error types opt into so they can be reported uniformly, chained to their underlying cause, and boxed behind a single return type.

---

## Quick Overview

The standard-library trait `std::error::Error` is Rust's answer to "what does it mean to be an error?" Any type that implements it can be displayed to a user, inspected by a developer, and asked for the cause beneath it via `source()`. Because it is a trait, you can erase the concrete type behind `Box<dyn Error>` and let a function return *any* error, much like a TypeScript function can `throw` any value.

> **Note:** This file focuses on the `Error` trait itself: its `Display + Debug` super-traits, the `source()` cause chain, and `Box<dyn Error>`. Defining your own error enums and structs is covered in [Custom Errors](/08-error-handling/04-custom-errors/); the ergonomic crates that generate these impls for you live in [anyhow & thiserror](/08-error-handling/06-anyhow-thiserror/).

---

## TypeScript/JavaScript Example

```typescript
// TypeScript: Error is a class. Subclass it to make domain errors.
class ConfigError extends Error {
  constructor(
    public key: string,
    message: string,
    // `cause` standardized in ES2022 / Node 16.9+
    options?: { cause?: unknown },
  ) {
    super(message, options);
    this.name = "ConfigError";
  }
}

function parsePort(raw: string): number {
  const port = Number(raw);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    // Wrap the lower-level reason as `cause` so it isn't lost.
    throw new ConfigError("PORT", `invalid port: ${raw}`, {
      cause: new RangeError("expected 1..=65535"),
    });
  }
  return port;
}

try {
  parsePort("not-a-number");
} catch (e) {
  if (e instanceof Error) {
    console.log(e.message);        // user-facing string
    console.log(e.stack);          // developer-facing detail
    console.log(e.cause);          // the wrapped underlying error
  }
}
```

Three things to notice, because they map almost one-to-one onto Rust:

- `Error` has a **`message`** (human-readable) and a **`stack`/`name`** (developer detail).
- ES2022 added **`cause`** so a high-level error can point at the low-level one that triggered it.
- You can `throw` *any* error subclass and catch it as the common `Error` base type.

---

## Rust Equivalent

```rust playground
use std::error::Error;
use std::fmt;

// A custom error type for parsing a config value.
#[derive(Debug)] // Debug is REQUIRED by the Error trait.
struct ConfigError {
    key: String,
    message: String,
}

// Display is also REQUIRED: this is the user-facing message.
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "config error for key `{}`: {}", self.key, self.message)
    }
}

// Opt in to the Error trait. With Display + Debug in place, the body is empty:
// the default `source()` returns None (no underlying cause yet).
impl Error for ConfigError {}

fn load_port() -> Result<u16, ConfigError> {
    Err(ConfigError {
        key: "PORT".to_string(),
        message: "expected a number between 1 and 65535".to_string(),
    })
}

fn main() {
    match load_port() {
        Ok(port) => println!("listening on {port}"),
        Err(e) => {
            println!("Display: {e}");    // the {} / Display form
            println!("Debug:   {e:?}");  // the {:?} / Debug form
        }
    }
}
```

Real output:

```text
Display: config error for key `PORT`: expected a number between 1 and 65535
Debug:   ConfigError { key: "PORT", message: "expected a number between 1 and 65535" }
```

The trait's full definition in the standard library is essentially:

```rust
// From the standard library (simplified).
pub trait Error: Debug + Display {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None // default: "I have no underlying cause"
    }
    // (a few more provided methods, all with defaults)
}
```

---

## Detailed Explanation

### The two super-traits: `Debug` and `Display`

The trait declaration is `pub trait Error: Debug + Display`. The `: Debug + Display` part means **you cannot implement `Error` for a type unless that type also implements `Debug` and `Display`**. These are *super-trait bounds*, and they encode two distinct audiences:

| Trait     | Format string | Audience    | TypeScript analogy                |
| --------- | ------------- | ----------- | --------------------------------- |
| `Display` | `{}`          | End user    | `error.message`                   |
| `Debug`   | `{:?}`        | Developer   | `console.log(error)` / `.stack`   |

- **`Display`** is what you show a user or write to a log line. You write it by hand with `impl fmt::Display`. There is no derive for it, on purpose: a good user-facing message requires human judgment.
- **`Debug`** is the developer view, almost always produced with `#[derive(Debug)]`. It prints the struct's fields verbatim.

> **Tip:** When `main` returns `Result<(), E>` and exits with an error, Rust prints the error using its **`Debug`** representation, not `Display`. That is a deliberate choice: `main` is a developer context. We'll see this below.

### `write!` and the `Formatter`

The `Display` impl receives a `Formatter` and uses the `write!` macro, which behaves like `println!` but targets the formatter instead of stdout. Returning `fmt::Result` (an alias for `Result<(), fmt::Error>`) lets the `?`-style propagation inside `write!` work. You almost never construct `fmt::Error` yourself; just `write!(...)` and return its result.

### `source()`: the cause chain

`source()` is the Rust counterpart to ES2022's `error.cause`. It returns `Option<&(dyn Error + 'static)>`:

- `None` — "I am the root cause" (the default).
- `Some(&inner)` — "I was caused by `inner`, which is itself some `dyn Error`."

By returning a *trait object* (`&dyn Error`), the chain is heterogeneous: each link can be a completely different concrete error type, yet you can walk the whole chain uniformly. Here is a high-level error that wraps a standard-library `ParseIntError`:

```rust playground
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;

// High-level error that WRAPS a lower-level cause.
#[derive(Debug)]
struct ConfigError {
    key: String,
    source: ParseIntError, // the underlying cause
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // NOTE: do NOT repeat the source's message here; source() exposes it.
        write!(f, "invalid value for config key `{}`", self.key)
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source) // hand back the cause
    }
}

fn parse_port(raw: &str) -> Result<u16, ConfigError> {
    raw.parse::<u16>().map_err(|e| ConfigError {
        key: "PORT".to_string(),
        source: e,
    })
}

// Walk and print the full chain of causes.
fn print_chain(err: &dyn Error) {
    eprintln!("error: {err}");
    let mut cause = err.source();
    while let Some(e) = cause {
        eprintln!("  caused by: {e}");
        cause = e.source();
    }
}

fn main() {
    if let Err(e) = parse_port("not-a-number") {
        print_chain(&e);
    }
}
```

Real output (written to stderr):

```text
error: invalid value for config key `PORT`
  caused by: invalid digit found in string
```

The `print_chain` loop is the canonical pattern: print the top error, then follow `source()` link by link until it returns `None`. The `anyhow` crate and reporting tools like `eyre` do exactly this for you, but the trait is what makes it possible.

> **Note:** Each `Display` impl should describe only **its own layer**. `ConfigError` says "invalid value for config key `PORT`"; the `ParseIntError` underneath says "invalid digit found in string". The chain printer joins them. If every layer re-printed its source, you would get duplicated, noisy messages.

### `Box<dyn Error>`: type erasure for errors

A function that can fail in several unrelated ways would otherwise need a single enum covering every case. `Box<dyn Error>` sidesteps that: it is a **heap-allocated trait object** that can hold *any* type implementing `Error`. This is the closest Rust gets to TypeScript's "throw anything, catch as `Error`."

```rust
use std::error::Error;
use std::fs;

// Box<dyn Error> = "some error type, decided at runtime".
// The ? operator converts each concrete error into the boxed trait object
// via the blanket `impl<E: Error + 'static> From<E> for Box<dyn Error>`.
fn read_config(path: &str) -> Result<u16, Box<dyn Error>> {
    let contents = fs::read_to_string(path)?; // io::Error -> Box<dyn Error>
    let port: u16 = contents.trim().parse()?; // ParseIntError -> Box<dyn Error>
    Ok(port)
}

fn main() -> Result<(), Box<dyn Error>> {
    // This path does not exist, so read_to_string fails.
    let port = read_config("does-not-exist.toml")?;
    println!("port = {port}");
    Ok(())
}
```

Running this prints (and the process exits with status `1`):

```text
Error: Os { code: 2, kind: NotFound, message: "No such file or directory" }
```

Two things just happened automatically:

1. **`?` converted both error types** — `std::io::Error` and `std::num::ParseIntError` — into the same `Box<dyn Error>`. There is a blanket `From` impl in the standard library: any `E: Error + 'static` (and, for the thread-safe form, any `E: Error + Send + Sync + 'static`) can become a `Box<dyn Error>`. The `'static` bound is mandatory. The mechanics of that conversion are covered in [The `?` Operator](/08-error-handling/01-question-mark/).
2. **`main` printed the error with `Debug`** (note the `Os { code: 2, ... }` struct form), not `Display`, as mentioned above.

### Recovering the concrete type with `downcast`

Type erasure is not a one-way door. Because `dyn Error` carries enough type information, you can attempt to recover the original concrete type with `downcast_ref` — the Rust analog of `e instanceof ConfigError`:

```rust playground
use std::error::Error;
use std::fmt;

#[derive(Debug)]
struct NotFound {
    id: u64,
}

impl fmt::Display for NotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "user {} not found", self.id)
    }
}
impl Error for NotFound {}

fn lookup(id: u64) -> Result<String, Box<dyn Error>> {
    Err(Box::new(NotFound { id }))
}

fn main() {
    if let Err(e) = lookup(42) {
        // Try to recover the concrete type behind the trait object.
        if let Some(nf) = e.downcast_ref::<NotFound>() {
            println!("recovered: missing id = {}", nf.id);
        } else {
            println!("some other error: {e}");
        }
    }
}
```

Real output:

```text
recovered: missing id = 42
```

`downcast_ref::<T>()` returns `Some(&T)` if the trait object really holds a `T`, and `None` otherwise, exactly like a checked cast. There is also `downcast` (consuming, returns `Result<Box<T>, Box<dyn Error>>`) and `downcast_mut`.

---

## Key Differences

| Concept                | TypeScript / JavaScript                       | Rust                                                            |
| ---------------------- | --------------------------------------------- | --------------------------------------------------------------- |
| What "error" *is*      | The `Error` **class** (and subclasses)        | The `std::error::Error` **trait** (opt-in for any type)         |
| User message           | `error.message` (always present)              | `Display` impl (`{}`) — you write it                            |
| Developer detail       | `error.stack`, `error.name`                   | `Debug` impl (`{:?}`) — usually derived                         |
| Underlying cause       | `error.cause` (ES2022)                        | `source()` returning `Option<&dyn Error>`                       |
| "Throw anything"       | `throw` any value                             | `Box<dyn Error>` (a boxed trait object)                         |
| Recover concrete type  | `e instanceof Foo`                            | `e.downcast_ref::<Foo>()`                                       |
| Stack trace            | Captured automatically on `new Error()`       | **Not** captured by `Error`; needs a backtrace (see below)      |
| Cost                   | Always allocates + captures stack             | Zero-cost until you choose to `Box` or capture a backtrace      |

### Why a trait and not a base class?

Rust has no inheritance. Instead of *extending* a base `Error` class, your type *implements* a trait — and it can implement many traits at once. This is closer to TypeScript's structural `interface`, except the implementation is explicit (`impl Error for ConfigError {}`) and checked at compile time. See [Section 09: Generics & Traits](/09-generics-traits/) for the full mental model of traits vs. interfaces and inheritance.

### No automatic stack traces

The biggest surprise for TypeScript developers: constructing a Rust error does **not** capture a stack trace. JavaScript's `new Error()` always snapshots the call stack (a cost you pay even when you catch and ignore it). Rust errors are plain values — often zero-allocation — and a backtrace is captured only if you opt in (for example, `std::backtrace::Backtrace::capture()` in a field, gated by the `RUST_BACKTRACE` env var, or via `anyhow`). The `source()` chain is Rust's lightweight substitute for "where did this come from."

---

## Common Pitfalls

### Pitfall 1: Implementing `Error` without `Display` and `Debug`

A TypeScript developer expects `class MyError extends Error {}` to just work. In Rust, opting into `Error` *requires* the two super-traits first:

```rust
use std::error::Error;

struct MyError; // does not compile (E0277: missing Display + Debug)

impl Error for MyError {}

fn main() {
    let _e = MyError;
}
```

The real compiler error (`cargo build`):

```text
error[E0277]: `MyError` doesn't implement `std::fmt::Display`
 --> src/main.rs:5:16
  |
5 | impl Error for MyError {}
  |                ^^^^^^^ the trait `std::fmt::Display` is not implemented for `MyError`
  |
note: required by a bound in `std::error::Error`
 ...
53 | pub trait Error: Debug + Display {
  |                          ^^^^^^^ required by this bound in `Error`

error[E0277]: `MyError` doesn't implement `Debug`
 --> src/main.rs:5:16
  |
5 | impl Error for MyError {}
  |                ^^^^^^^ the trait `Debug` is not implemented for `MyError`
  |
  = note: add `#[derive(Debug)]` to `MyError` or manually `impl Debug for MyError`
...
help: consider annotating `MyError` with `#[derive(Debug)]`
  |
3 + #[derive(Debug)]
4 | struct MyError;
  |
```

**Fix:** add `#[derive(Debug)]` and write an `impl fmt::Display`. Note how precisely the compiler points at the missing bounds. This is the norm, not the exception.

### Pitfall 2: `Box<dyn Error>` cannot cross a thread boundary

`Box<dyn Error>` is **not** `Send`, so returning it from a spawned thread fails to compile. TypeScript has no equivalent restriction because everything runs on one event loop.

```rust
use std::error::Error;
use std::thread;

fn work() -> Result<(), Box<dyn Error>> {
    Err("boom".into())
}

fn main() {
    // does not compile (E0277: `dyn Error` cannot be sent between threads safely)
    let handle = thread::spawn(|| -> Result<(), Box<dyn Error>> {
        work()
    });
    let _ = handle.join();
}
```

The real error (`cargo build`, abridged):

```text
error[E0277]: `dyn std::error::Error` cannot be sent between threads safely
  --> src/main.rs:9:18
   |
 9 |       let handle = thread::spawn(|| -> Result<(), Box<dyn Error>> {
   |  __________________^
   | |______^ `dyn std::error::Error` cannot be sent between threads safely
   |
   = help: the trait `Send` is not implemented for `dyn std::error::Error`
...
note: required by a bound in `spawn`
```

**Fix:** use the thread-safe alias `Box<dyn Error + Send + Sync>`:

```rust playground
use std::error::Error;
use std::thread;

// The thread-safe boxed error alias.
type BoxError = Box<dyn Error + Send + Sync>;

fn work() -> Result<(), BoxError> {
    Err("boom".into()) // &str -> Box<dyn Error + Send + Sync> via From
}

fn main() {
    let handle = thread::spawn(|| -> Result<(), BoxError> { work() });
    match handle.join().unwrap() {
        Ok(()) => println!("ok"),
        Err(e) => println!("worker failed: {e}"),
    }
}
```

Real output:

```text
worker failed: boom
```

> **Tip:** `Box<dyn Error + Send + Sync + 'static>` is the form `anyhow::Error` uses internally and the one most application code should reach for. The `+ Send + Sync` makes it usable across threads and inside async tasks.

### Pitfall 3: duplicating the cause in your `Display` message

Because `source()` already exposes the inner error, *also* baking it into your `Display` string produces double-printed messages once a chain printer (or `anyhow`) walks the chain. Keep each layer's `Display` to its own concern, and let `source()` carry the rest.

### Pitfall 4: expecting a stack trace from `{:?}`

`{:?}` prints the *struct fields*, not a stack trace. If you `#[derive(Debug)]` a `ConfigError`, you get `ConfigError { key: "PORT", ... }`, never a call stack. To get backtraces, add a `std::backtrace::Backtrace` field, or use `anyhow` (which captures one when `RUST_BACKTRACE=1`).

---

## Best Practices

- **Derive `Debug`, hand-write `Display`.** `#[derive(Debug)]` is correct for nearly every error type; the `Display` message deserves a human's wording.
- **Implement `source()` whenever you wrap a lower-level error.** It preserves the cause chain and is what reporting tooling relies on. Return `Some(&self.inner)`.
- **Prefer `Box<dyn Error + Send + Sync>` over a bare `Box<dyn Error>`** in application code and anything touching threads or async. It is a one-character-heavier alias that avoids Pitfall 2 entirely.
- **Reach for `Box<dyn Error>` in application/`main` code; reach for a concrete enum in libraries.** Library callers want to `match` on specific variants (which a boxed trait object hides). Applications usually just want to report and exit. This library-vs-application split is a recurring theme across this section's error-handling topics.
- **Don't hand-roll the boilerplate in real projects.** The `thiserror` crate derives `Display`, `Error`, and `From`/`source()` wiring for you; `anyhow` provides a ready-made `Box<dyn Error + Send + Sync>` replacement with backtraces and context. See [anyhow & thiserror](/08-error-handling/06-anyhow-thiserror/). Understanding the trait first (this file) is what makes those crates feel obvious instead of magical.
- **Write a small `report(&dyn Error)` chain printer** (like the one above) for binaries that don't pull in `anyhow`. It is a dozen lines and turns one-line errors into useful diagnostics.

---

## Real-World Example

A price-feed loader that parses lines like `AAPL,189.95`. The domain error carries a human-readable message *and* keeps the lower-level cause reachable through `source()`, so a single reporting helper can print the full diagnostic chain.

```rust playground
use std::error::Error;
use std::fmt;
use std::num::ParseFloatError;

/// Domain error for the price-feed loader. It carries a human-readable
/// context AND keeps the lower-level cause reachable via `source()`.
#[derive(Debug)]
struct PriceFeedError {
    symbol: String,
    kind: PriceFeedErrorKind,
}

#[derive(Debug)]
enum PriceFeedErrorKind {
    /// The raw line could not be split into the expected fields.
    Malformed,
    /// The price field was present but not a valid number.
    BadPrice(ParseFloatError),
}

impl fmt::Display for PriceFeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            PriceFeedErrorKind::Malformed => {
                write!(f, "malformed record for symbol `{}`", self.symbol)
            }
            PriceFeedErrorKind::BadPrice(_) => {
                write!(f, "invalid price for symbol `{}`", self.symbol)
            }
        }
    }
}

impl Error for PriceFeedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            // Only BadPrice has an underlying cause to expose.
            PriceFeedErrorKind::BadPrice(e) => Some(e),
            PriceFeedErrorKind::Malformed => None,
        }
    }
}

fn parse_quote(line: &str) -> Result<(String, f64), PriceFeedError> {
    let (symbol, raw_price) = line.split_once(',').ok_or_else(|| PriceFeedError {
        symbol: line.to_string(),
        kind: PriceFeedErrorKind::Malformed,
    })?;

    let price = raw_price.trim().parse::<f64>().map_err(|e| PriceFeedError {
        symbol: symbol.to_string(),
        kind: PriceFeedErrorKind::BadPrice(e),
    })?;

    Ok((symbol.to_string(), price))
}

/// Reusable helper: print an error and every cause beneath it.
fn report(err: &dyn Error) {
    eprintln!("error: {err}");
    let mut source = err.source();
    while let Some(cause) = source {
        eprintln!("  caused by: {cause}");
        source = cause.source();
    }
}

fn main() {
    let inputs = ["AAPL,189.95", "TSLA,not-a-price", "BROKENLINE"];
    for line in inputs {
        match parse_quote(line) {
            Ok((symbol, price)) => println!("{symbol}: {price:.2}"),
            Err(e) => report(&e),
        }
    }
}
```

Real output (stdout and stderr interleaved):

```text
AAPL: 189.95
error: invalid price for symbol `TSLA`
  caused by: invalid float literal
error: malformed record for symbol `BROKENLINE`
```

Notice that `Malformed` returns `None` from `source()` (no inner cause to show), while `BadPrice` exposes the standard-library `ParseFloatError`: the same trait, two different shapes of chain, one reporting function that handles both.

---

## Further Reading

- [`std::error::Error` — standard library docs](https://doc.rust-lang.org/std/error/trait.Error.html)
- [`std::fmt::Display`](https://doc.rust-lang.org/std/fmt/trait.Display.html) and [`std::fmt::Debug`](https://doc.rust-lang.org/std/fmt/trait.Debug.html)
- [`Box<dyn Error>` and the `From` conversion](https://doc.rust-lang.org/std/boxed/struct.Box.html#impl-From%3CE%3E-for-Box%3Cdyn+Error%3E)
- [`std::backtrace::Backtrace`](https://doc.rust-lang.org/std/backtrace/struct.Backtrace.html): opt-in stack traces
- [The Rust Book: "Defining an Error Type"](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html)

Related sections in this guide:

- [Result & Option](/08-error-handling/00-result-option/): the values that carry these errors
- [The `?` Operator](/08-error-handling/01-question-mark/): how errors get *converted* into `Box<dyn Error>` automatically
- [Custom Errors](/08-error-handling/04-custom-errors/): defining error enums and structs (the impls shown here, by hand)
- [Multiple Error Types](/08-error-handling/07-multiple-errors/): aggregating many error types behind one return type
- [anyhow & thiserror](/08-error-handling/06-anyhow-thiserror/): crates that generate these impls for you
- [Section 09: Generics & Traits](/09-generics-traits/) — traits, trait objects, and `dyn` in depth
- [Section 05: Ownership](/05-ownership/) — why `Box` and lifetimes (`'static`) appear in these signatures

---

## Exercises

### Exercise 1

**Difficulty:** Beginner

**Objective:** Implement the `Error` trait from scratch for a simple type.

**Instructions:** Define a unit struct `EmptyInputError` and make it a valid `std::error::Error`. It must print `input was empty` via `Display`. Then write `first_char(s: &str) -> Result<char, EmptyInputError>` that returns the error when the string is empty.

```rust
use std::error::Error;
use std::fmt;

struct EmptyInputError; // TODO: derive Debug + impl Display + impl Error

fn first_char(s: &str) -> Result<char, EmptyInputError> {
    /* ??? */
}

fn main() {
    println!("{:?}", first_char("rust"));
    match first_char("") {
        Ok(c) => println!("got {c}"),
        Err(e) => println!("error: {e}"),
    }
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::error::Error;
use std::fmt;

#[derive(Debug)]
struct EmptyInputError;

impl fmt::Display for EmptyInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "input was empty")
    }
}

impl Error for EmptyInputError {}

fn first_char(s: &str) -> Result<char, EmptyInputError> {
    s.chars().next().ok_or(EmptyInputError)
}

fn main() {
    println!("{:?}", first_char("rust"));
    match first_char("") {
        Ok(c) => println!("got {c}"),
        Err(e) => println!("error: {e}"),
    }
}
```

Output:

```text
Ok('r')
error: input was empty
```

</details>

### Exercise 2

**Difficulty:** Intermediate

**Objective:** Wrap a lower-level error and expose it through `source()`.

**Instructions:** Define `EnvError { var: String, source: ParseIntError }`. Implement `Display` (mention only the variable name, not the parse detail) and `Error` with a working `source()`. Write `read_retries(raw: &str) -> Result<u32, EnvError>` that wraps a failed `parse::<u32>()`. Finally, write a *generic* function `print_chain<E: Error>(err: &E)` that prints the error and every cause beneath it.

<details>
<summary>Solution</summary>

```rust playground
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
struct EnvError {
    var: String,
    source: ParseIntError,
}

impl fmt::Display for EnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "environment variable `{}` is not a valid integer", self.var)
    }
}

impl Error for EnvError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

fn read_retries(raw: &str) -> Result<u32, EnvError> {
    raw.parse::<u32>().map_err(|e| EnvError {
        var: "MAX_RETRIES".to_string(),
        source: e,
    })
}

// Generic over any error type.
fn print_chain<E: Error>(err: &E) {
    println!("error: {err}");
    let mut cause: Option<&dyn Error> = err.source();
    while let Some(e) = cause {
        println!("  caused by: {e}");
        cause = e.source();
    }
}

fn main() {
    if let Err(e) = read_retries("five") {
        print_chain(&e);
    }
}
```

Output:

```text
error: environment variable `MAX_RETRIES` is not a valid integer
  caused by: invalid digit found in string
```

</details>

### Exercise 3

**Difficulty:** Advanced

**Objective:** Erase an error behind `Box<dyn Error>`, collect its full chain, then recover the concrete type with `downcast_ref`.

**Instructions:** Reuse a wrapping error `ParseRecordError { line: usize, source: ParseIntError }` (Debug + Display + Error with `source()`). Write `chain_messages(err: &dyn Error) -> Vec<String>` that returns the top error message followed by each cause's message. In `main`, box the error as `Box<dyn Error>`, print the collected chain, then use `downcast_ref::<ParseRecordError>()` to recover and print the offending `line` number.

<details>
<summary>Solution</summary>

```rust playground
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
struct ParseRecordError {
    line: usize,
    source: ParseIntError,
}

impl fmt::Display for ParseRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to parse record on line {}", self.line)
    }
}

impl Error for ParseRecordError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

// Collect the full chain (top error + every cause) into owned strings.
fn chain_messages(err: &dyn Error) -> Vec<String> {
    let mut msgs = vec![err.to_string()];
    let mut cause = err.source();
    while let Some(e) = cause {
        msgs.push(e.to_string());
        cause = e.source();
    }
    msgs
}

fn parse(line: usize, raw: &str) -> Result<i64, ParseRecordError> {
    raw.parse::<i64>().map_err(|e| ParseRecordError { line, source: e })
}

fn main() {
    let err = parse(7, "1.5").unwrap_err();
    let boxed: Box<dyn Error> = Box::new(err);

    // 1. Full chain as strings.
    println!("{:?}", chain_messages(boxed.as_ref()));

    // 2. Recover the concrete top-level type.
    if let Some(pre) = boxed.downcast_ref::<ParseRecordError>() {
        println!("offending line: {}", pre.line);
    }
}
```

Output:

```text
["failed to parse record on line 7", "invalid digit found in string"]
offending line: 7
```

> **Note:** `err.to_string()` works because anything implementing `Display` automatically gets `ToString` via a blanket impl in the standard library, the same mechanism that gives `Error` types a free `.to_string()`.

</details>
