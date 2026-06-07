---
title: "Error-Handling Best Practices"
description: "Choosing the right error tool in Rust: thiserror enums for libraries, anyhow with context for apps, and Result versus panic, mapped from TypeScript habits."
---

Once you know the mechanics (`Result`, `Option`, `?`, `panic!`, custom error types, and the `anyhow`/`thiserror` crates) the next question is *design*: how do you decide **which** tool to reach for, how granular your error types should be, what your messages should say, and where a real failure ends and a programmer bug begins. This page is the decision guide that ties the rest of Section 08 together.

---

## Quick Overview

Good Rust error handling is mostly about a few deliberate choices: **libraries** expose precise, matchable error types (typically a `thiserror` enum) so callers can react; **applications** prioritize getting a useful message to a human and bubbling failures up (typically `anyhow` with context). On top of that, you decide what is **recoverable** (return a `Result`) versus an **unrecoverable bug** (let it `panic!`). Get these axes right and the rest of error handling falls into place.

> **Note:** This file is about *strategy*. The how-to for each piece lives in its sibling page: defining error types in [Custom Error Types](/08-error-handling/04-custom-errors/), the `Error` trait in [The `Error` Trait](/08-error-handling/05-error-trait/), the crates in [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/), and propagation in [The `?` Operator](/08-error-handling/01-question-mark/).

---

## TypeScript/JavaScript Example

In a Node.js codebase, error-handling strategy is mostly *convention*, because the language gives you almost no help. A typical service mixes several styles, and nothing in the type system forces consistency:

```typescript
// orders.ts — a service module in a Node app

// Style 1: throw a generic Error with a string. Loses all structure.
function validateQuantity(qty: number): void {
  if (qty <= 0) {
    throw new Error("quantity must be positive"); // caller can only read .message
  }
}

// Style 2: a custom subclass, so callers *can* branch — if they remember to.
class OutOfStockError extends Error {
  constructor(public readonly sku: string, public readonly available: number) {
    super(`SKU ${sku} has only ${available} in stock`);
    this.name = "OutOfStockError";
  }
}

async function reserve(sku: string, qty: number): Promise<number> {
  validateQuantity(qty);
  const available = await lookupStock(sku); // may itself throw a DB error
  if (qty > available) {
    throw new OutOfStockError(sku, available);
  }
  return available - qty;
}

// The caller has to *know* what might be thrown — the signature says `Promise<number>`,
// not "or it might throw OutOfStockError, or a TypeError, or a DB connection error".
async function handleRequest(sku: string, qty: number) {
  try {
    const left = await reserve(sku, qty);
    console.log(`reserved, ${left} left`);
  } catch (err) {
    // `err` is typed `unknown`. We guess at the shapes we care about.
    if (err instanceof OutOfStockError) {
      console.warn(`restock ${err.sku}`);
    } else {
      console.error("unexpected:", err); // everything else is a black box
    }
  }
}

declare function lookupStock(sku: string): Promise<number>;
```

The three pain points that drive every decision below:

1. **The signature hides what can fail.** `Promise<number>` says nothing about `OutOfStockError`.
2. **There's no enforced granularity.** One function throws a bare string, another throws a structured class, and the compiler is fine with both.
3. **Bug vs. expected failure is blurred.** A `TypeError` from a typo and a legitimate "out of stock" both arrive in the same `catch` as `unknown`.

---

## Rust Equivalent

Rust pushes you to make those choices explicit. A **library** module returns a precise enum the caller can match on; an **application** layer consumes it, adds human context, and lets it bubble up, only matching the specific variant it actually wants to handle.

```rust
// Real-world: a "billing" library module exposes a typed error.
// The application layer (main) consumes it with anyhow + context.

mod billing {
    use thiserror::Error;

    /// Public, stable error type. Callers can match on these variants;
    /// adding a variant later is a semver concern, so we keep it #[non_exhaustive].
    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum ChargeError {
        #[error("amount must be positive, got {0} cents")]
        NonPositiveAmount(i64),

        #[error("card declined: {reason}")]
        Declined { reason: String },

        #[error("payment gateway I/O failed")]
        Gateway(#[from] std::io::Error),
    }

    pub fn charge_cents(card: &str, amount: i64) -> Result<String, ChargeError> {
        if amount <= 0 {
            return Err(ChargeError::NonPositiveAmount(amount));
        }
        if card == "4000000000000002" {
            return Err(ChargeError::Declined { reason: "insufficient funds".into() });
        }
        if card == "io-fail" {
            // The `?` converts io::Error into ChargeError via #[from].
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "gateway timeout").into());
        }
        Ok(format!("txn_{}", amount))
    }
}

use anyhow::{Context, Result};

fn process_order(card: &str, amount: i64) -> Result<()> {
    // The app doesn't care about each billing variant here; it adds context
    // and lets the failure bubble up to a top-level handler.
    let txn = billing::charge_cents(card, amount)
        .with_context(|| format!(
            "charging card ending {} for {amount}c",
            &card[card.len().saturating_sub(4)..]
        ))?;
    println!("charged ok: {txn}");
    Ok(())
}

fn main() {
    // Where the app DOES want to react to a specific kind, it can downcast
    // back to the concrete library error.
    if let Err(e) = process_order("4000000000000002", 1299) {
        println!("order failed: {e:#}");
        if let Some(billing::ChargeError::Declined { reason }) =
            e.downcast_ref::<billing::ChargeError>()
        {
            println!("  -> declined specifically because: {reason}");
        }
    }

    let _ = process_order("4111111111111111", 500);

    if let Err(e) = process_order("io-fail", 700) {
        println!("order failed: {e:#}");
    }
}
```

Real output:

```text
order failed: charging card ending 0002 for 1299c: card declined: insufficient funds
  -> declined specifically because: insufficient funds
charged ok: txn_500
order failed: charging card ending fail for 700c: payment gateway I/O failed: gateway timeout
```

The signature `Result<String, ChargeError>` documents the failure modes; the caller chooses between treating the error as opaque (`{e:#}`) and matching a specific variant (`downcast_ref`).

---

## Detailed Explanation

### The library half: `thiserror`, precise variants, `#[from]`

- **`#[derive(Error)]`** (from [thiserror](/08-error-handling/06-anyhow-thiserror/)) generates the `Display` and `std::error::Error` impls so `ChargeError` is a fully-fledged error. The `#[error("…")]` attribute is the human message for each variant.
- **Each failure mode is its own variant.** `NonPositiveAmount`, `Declined`, and `Gateway` are distinct so a caller can match exactly the one it can handle. This is the structured equivalent of the TypeScript `OutOfStockError` subclass, except the compiler *forces* the function to declare it in the return type.
- **`#[from] std::io::Error`** generates `From<io::Error> for ChargeError`, which is what lets `?` convert a low-level error into our domain error (covered in [The `?` Operator](/08-error-handling/01-question-mark/) and [Handling Multiple Error Types](/08-error-handling/07-multiple-errors/)).
- **`#[non_exhaustive]`** tells downstream crates "I may add variants later," so their `match` must include a `_ =>` arm. This keeps adding a variant a non-breaking change.

### The application half: `anyhow`, context, bubble up

- **`fn process_order(...) -> anyhow::Result<()>`** uses `anyhow::Error`, a type-erased "any error" wrapper. The application generally does *not* want to enumerate every possible failure; it wants to attach a breadcrumb and move on.
- **`.with_context(|| …)`** wraps whatever error came up with a human-readable layer. The closure form runs the `format!` *only on the error path*, so it's free on success.
- **`e.downcast_ref::<billing::ChargeError>()`** is the escape hatch: when the app genuinely needs to branch on a library variant, it can recover the concrete type from the erased `anyhow::Error`. This is the one place the app re-introduces structure.

### Where the analogy to TypeScript breaks down

In TypeScript, *throwing* is the same act whether the cause is bad user input or a bug; both unwind the stack and land in `catch (err: unknown)`. In Rust the two are different mechanisms entirely: recoverable failures are **values** (`Result`) the type system tracks, and bugs are **panics** that abort the current thread. You choose which one a given situation deserves. That choice is the heart of error-handling design and the next section covers it.

---

## Key Differences

### Libraries vs. applications

| Axis | Library crate | Application (binary) |
| --- | --- | --- |
| Primary goal | Let *callers* react programmatically | Get a useful message to a human; fail fast |
| Typical error type | A concrete enum via `thiserror` | `anyhow::Error` (type-erased) |
| Return type | `Result<T, MyError>` | `anyhow::Result<T>` |
| Granularity | Fine: one variant per distinct failure | Coarse: context strings, not variants |
| Stability | Public API; variant changes are semver | Internal; change freely |
| Dependency cost | `thiserror` is compile-time only, no runtime type | `anyhow` adds a small runtime type |

> **Tip:** A crate can be *both*. Many crates expose a `thiserror` enum publicly **and** use `anyhow` internally in their own binary/`examples`/tests. The rule is about the *boundary you expose*, not the whole project.

### Recoverable vs. unrecoverable

| Situation | Mechanism | Why |
| --- | --- | --- |
| Bad user input, missing file, network timeout, parse failure | `Result` / `Option` | Expected at runtime; the caller can reasonably handle it |
| A broken invariant your code is supposed to guarantee | `panic!` / `assert!` / `expect` | It's a *bug*; continuing would corrupt state |
| "This index is always valid because I just checked `len`" | `expect("…")` with a reason | Provable invariant; see [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/) |
| Library code reacting to external conditions | almost always `Result` | A library should rarely decide to abort the caller's process |

The litmus test: **"Could a correct program, given valid inputs, still hit this?"** If yes, it's a `Result`. If it can only happen because some code is wrong, it's a panic. See [Panicking](/08-error-handling/02-panic/) for the full treatment.

### Error granularity

The TypeScript instinct is often "one big error class with a `code` field." In Rust, prefer **one variant per thing a caller might branch on**, but no finer. If two failures are always handled identically, merge them. If a caller will never distinguish two cases, don't split them just because they have different messages; a single variant with a `String` detail (like `NotFound(String)`) is fine.

---

## Common Pitfalls

### Pitfall 1: Using `String` (or `Box<dyn Error>`) as a library's error type

`fn parse(...) -> Result<T, String>` compiles and feels easy, but it throws away everything a caller needs. They can `.contains("not found")` on your message, and then your next message tweak silently breaks them. For a public API, return an enum. (`Box<dyn Error>` has the same downside for *libraries*; it's fine inside an app where you'd reach for `anyhow` anyway. See [Handling Multiple Error Types](/08-error-handling/07-multiple-errors/).)

### Pitfall 2: Trying to `?` an error your type can't convert

`?` only propagates an error if there is a `From` impl from the source error into your function's error type. Forget it and you get a precise, real compiler error:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
#[error("validation failed: {0}")]
struct ValidationError(String);

// Returns ValidationError, but tries to `?` a std::num::ParseIntError with no
// From<ParseIntError> for ValidationError. The ? cannot convert it.
fn parse_age(raw: &str) -> Result<u8, ValidationError> {
    let age: u8 = raw.parse()?; // does not compile (error[E0271])
    Ok(age)
}
```

The real message from `cargo build`:

```text
error[E0271]: type mismatch resolving `<u8 as FromStr>::Err == ValidationError`
  --> src/main.rs:10:23
   |
10 |     let age: u8 = raw.parse()?; // does not compile (error[E0271])
   |                       ^^^^^ expected `ValidationError`, found `ParseIntError`

For more information about this error, try `rustc --explain E0271`.
```

The fix is a `From` impl, usually `#[from]` on a variant in your `thiserror` enum, covered in [The `?` Operator](/08-error-handling/01-question-mark/).

### Pitfall 3: Fat error enums that bloat every `Result`

A `Result<T, E>` is as large as `T` *or* `E`, whichever is bigger, on **every** return, including the happy path. Inline a big field into one variant and you pay for it everywhere. Clippy catches both halves of this:

```rust
#[derive(Debug)]
pub enum ApiError {
    NotFound,
    Detailed([u8; 256]), // 256 bytes inlined into the error
}

pub fn find(id: u32) -> Result<u32, ApiError> {
    if id == 0 { Err(ApiError::NotFound) } else { Ok(id) }
}
```

Real `cargo clippy` warnings (the snippet above placed at the top of `src/main.rs`):

```text
warning: large size difference between variants
 --> src/main.rs:2:1
  |
2 | / pub enum ApiError {
3 | |     NotFound,
  | |     -------- the second-largest variant carries no data at all
4 | |     Detailed([u8; 256]), // 256 bytes inlined into the error
  | |     ------------------- the largest variant contains at least 256 bytes
5 | | }
  | |_^ the entire enum is at least 257 bytes
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#large_enum_variant
  = note: `#[warn(clippy::large_enum_variant)]` on by default
help: consider boxing the large fields or introducing indirection in some other way to reduce the total size of the enum
  |
4 -     Detailed([u8; 256]), // 256 bytes inlined into the error
4 +     Detailed(Box<[u8; 256]>), // 256 bytes inlined into the error
  |

warning: the `Err`-variant returned from this function is very large
 --> src/main.rs:7:25
  |
4 |     Detailed([u8; 256]), // 256 bytes inlined into the error
  |     ------------------- the largest variant contains at least 256 bytes
...
7 | pub fn find(id: u32) -> Result<u32, ApiError> {
  |                         ^^^^^^^^^^^^^^^^^^^^^
  |
  = help: try reducing the size of `ApiError`, for example by boxing large elements or replacing it with `Box<ApiError>`
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#result_large_err
  = note: `#[warn(clippy::result_large_err)]` on by default
```

The fix is to `Box` the large field (`Detailed(Box<[u8; 256]>)`) so the enum stays pointer-sized.

### Pitfall 4: Panicking inside a library on bad *input*

A library function that does `let n: u32 = raw.parse().unwrap();` will abort the *caller's* whole program when a user types garbage. Bad input is recoverable: return a `Result`. Reserve panics for violated invariants in your own code. (When `unwrap`/`expect` *are* acceptable — tests, prototypes, provable invariants — see [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/).)

### Pitfall 5: Sloppy error messages

Rust messages compose into chains, so a message that reads fine alone can read badly in a chain. Avoid capital letters at the start, trailing periods, and the word "error:"; the framework adds the framing. "config key `port` is missing" composes into "loading config: config key `port` is missing"; "Error: Config key Port is missing." does not.

---

## Best Practices

### 1. Decide the boundary first: who consumes this error?

If the answer is "other code that needs to branch," design a `thiserror` enum. If it's "a log file and a human," use `anyhow`. Most projects are a tree: leaf library modules return enums; the binary at the root collects them with `anyhow`.

### 2. Write error messages like log lines: lowercase, no period, no "error:"

Make each message a self-contained noun-phrase or short clause describing *what failed*, and let context layers describe *what you were doing*. The two combine cleanly:

```rust
use anyhow::{Context, Result};

fn read_setting(raw: Option<&str>) -> Result<u32> {
    // .context() takes an already-built value (cheap, eager).
    let raw = raw.context("setting `max_retries` is required")?;
    // .with_context() takes a closure: only runs the format! on the error path.
    let n: u32 = raw
        .parse()
        .with_context(|| format!("setting `max_retries` has bad value `{raw}`"))?;
    Ok(n)
}

fn main() {
    if let Err(e) = read_setting(Some("five")) {
        // {:#} = single-line, context chain joined with ": "
        println!("single-line: {e:#}");
        // {:?} = multi-line "Caused by:" report (great for top-level logging)
        println!("\nmulti-line:\n{e:?}");
    }
}
```

Real output:

```text
single-line: setting `max_retries` has bad value `five`: invalid digit found in string

multi-line:
setting `max_retries` has bad value `five`

Caused by:
    invalid digit found in string
```

> **Tip:** Use `.context(value)` when the message is a constant string and `.with_context(|| …)` when building it costs a `format!`; the closure only runs on the error path.

### 3. Return `anyhow::Result<()>` from `main` in applications

You get a free top-level handler: an `Err` is printed via its `Debug` impl (the full "Caused by:" chain) and the process exits non-zero.

```rust
use anyhow::{Context, Result};

fn load_threshold() -> Result<u32> {
    "abc".parse::<u32>().context("THRESHOLD must be an integer")
}

// A top-level Err is printed via its Debug impl and the process exits with code 1.
fn main() -> Result<()> {
    let t = load_threshold()?;
    println!("threshold = {t}");
    Ok(())
}
```

Real output (and the shell reports exit code `1`):

```text
Error: THRESHOLD must be an integer

Caused by:
    invalid digit found in string
```

### 4. Keep public error enums `#[non_exhaustive]`

It lets you add variants later without a breaking change, at the small cost of requiring callers to include a `_ =>` arm. Worth it for any error type you publish.

### 5. Match granularity to caller behavior, not to message text

Split a variant only when a caller would handle the two cases differently. Otherwise keep one variant and put the distinguishing detail in a field.

### 6. Panic for bugs, `Result` for conditions — and make panics loud

When you do panic, use `expect("reason this can't happen")` rather than a bare `unwrap()`, so the message explains the violated invariant. See [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/) and [Panicking](/08-error-handling/02-panic/).

---

## Real-World Example

A common production shape: a recoverable parser that returns a precise enum, alongside a helper whose contract is enforced by a panic. The granularity is chosen so callers can recover from a *miss* but not from a *broken store*.

```rust
// Granularity chosen so callers can recover from one variant and not the other.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CacheError {
    /// Caller CAN recover: treat it as a miss and recompute.
    #[error("key `{0}` not found")]
    NotFound(String),

    /// Caller CANNOT recover here, but it's still an `Err`, not a panic:
    /// the backing store is broken and the request should fail upward.
    #[error("backing store unavailable: {0}")]
    StoreUnavailable(String),
}

struct Cache;

impl Cache {
    fn get(&self, key: &str) -> Result<String, CacheError> {
        match key {
            "user:1" => Ok("Ada".to_string()),
            "down" => Err(CacheError::StoreUnavailable("connection refused".to_string())),
            other => Err(CacheError::NotFound(other.to_string())),
        }
    }
}

fn lookup_or_default(cache: &Cache, key: &str) -> Result<String, CacheError> {
    match cache.get(key) {
        Ok(value) => Ok(value),
        // Recover from the variant we know how to handle...
        Err(CacheError::NotFound(_)) => Ok(format!("<default for {key}>")),
        // ...and propagate the one we can't.
        Err(e) => Err(e),
    }
}

fn main() {
    let cache = Cache;
    for key in ["user:1", "user:999", "down"] {
        match lookup_or_default(&cache, key) {
            Ok(v) => println!("{key} -> {v}"),
            Err(e) => println!("{key} -> ERROR: {e}"),
        }
    }
}
```

Real output:

```text
user:1 -> Ada
user:999 -> <default for user:999>
down -> ERROR: backing store unavailable: connection refused
```

Notice the design decisions baked in: two variants because callers treat them differently; both are `Err` (not a panic) because both are runtime conditions; the message is lowercase and chain-friendly; and `lookup_or_default` recovers from exactly one variant while propagating the rest.

---

## Further Reading

- [The Rust Book — Error Handling chapter](https://doc.rust-lang.org/book/ch09-00-error-handling.html): the canonical recoverable-vs-unrecoverable framing.
- [The Rust Book — "To `panic!` or Not to `panic!`"](https://doc.rust-lang.org/book/ch09-03-to-panic-or-not-to-panic.html) — the official version of the decision guide on this page.
- [Rust API Guidelines — Error types](https://rust-lang.github.io/api-guidelines/interoperability.html#error-types-are-meaningful-and-well-behaved-c-good-err): what a *good* public error type looks like.
- [`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html) — the trait every error implements; see [The `Error` Trait](/08-error-handling/05-error-trait/).
- [Clippy lint: `result_large_err`](https://rust-lang.github.io/rust-clippy/master/index.html#result_large_err) and [`large_enum_variant`](https://rust-lang.github.io/rust-clippy/master/index.html#large_enum_variant).

### Related sections in this guide

- [Result and Option](/08-error-handling/00-result-option/): the `Result`/`Option` types these decisions are built on.
- [The `?` Operator](/08-error-handling/01-question-mark/) — `?` and `From`-based conversion.
- [Panicking](/08-error-handling/02-panic/): when a panic is the right call.
- [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/) — when `unwrap`/`expect` are acceptable.
- [Custom Error Types](/08-error-handling/04-custom-errors/): defining the enums/structs by hand.
- [The `Error` Trait](/08-error-handling/05-error-trait/) — `Display`, `Debug`, and the `source()` chain.
- [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/): the crates that implement the library/app split.
- [Handling Multiple Error Types](/08-error-handling/07-multiple-errors/) — aggregating several error types.
- Foundations: [Section 00 — Introduction](/00-introduction/), [Section 01 — Getting Started](/01-getting-started/), [Section 02 — Basics](/02-basics/).
- Next up: [Section 09 — Generics & Traits](/09-generics-traits/), which explains the trait machinery (`From`, `Display`, `Error`) these patterns rely on.

---

## Exercises

### Exercise 1: Turn a stringly-typed error into a matchable enum

**Difficulty:** Easy

**Objective:** Practice choosing the right granularity for a library function's error type.

**Instructions:**

1. Start from `fn parse_celsius(raw: &str) -> Result<f64, String>` that fails on (a) empty input, (b) non-numeric input, and (c) a value below absolute zero (`-273.15`).
2. Replace the `String` error with a `thiserror` enum that has one variant per failure mode, with clear, chain-friendly messages.
3. Add `#[derive(PartialEq)]` so the cases are easy to assert on, and verify with a few assertions.

<details>
<summary>Solution</summary>

```rust
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseTempError {
    #[error("empty input")]
    Empty,
    #[error("`{0}` is not a number")]
    NotANumber(String),
    #[error("{0}°C is below absolute zero (-273.15)")]
    BelowAbsoluteZero(f64),
}

pub fn parse_celsius(raw: &str) -> Result<f64, ParseTempError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(ParseTempError::Empty);
    }
    let n: f64 = raw
        .parse()
        .map_err(|_| ParseTempError::NotANumber(raw.to_string()))?;
    if n < -273.15 {
        return Err(ParseTempError::BelowAbsoluteZero(n));
    }
    Ok(n)
}

fn main() {
    assert_eq!(parse_celsius("21.5"), Ok(21.5));
    assert_eq!(parse_celsius("   "), Err(ParseTempError::Empty));
    assert_eq!(
        parse_celsius("hot"),
        Err(ParseTempError::NotANumber("hot".to_string()))
    );
    assert_eq!(
        parse_celsius("-300"),
        Err(ParseTempError::BelowAbsoluteZero(-300.0))
    );
    println!("exercise 1 ok");
}
```

Each failure is a distinct variant, so a caller can branch on exactly the one it cares about. The messages are lowercase and self-contained so they compose into context chains. Output: `exercise 1 ok`.

</details>

### Exercise 2: Draw the library/application boundary

**Difficulty:** Medium

**Objective:** Build a small library module with a typed error and an application layer that consumes it with `anyhow`, recovering structure only where needed.

**Instructions:**

1. Write a module `inventory` with `fn reserve(sku: &str, requested: u32) -> Result<u32, InventoryError>`. `InventoryError` (a `thiserror`, `#[non_exhaustive]` enum) should distinguish an unknown SKU from insufficient stock.
2. In application code, write `fn fulfill(sku: &str, qty: u32) -> anyhow::Result<()>` that calls `reserve`, adds context with `.with_context`, and propagates with `?`.
3. In `main`, when fulfilling an *unknown* SKU fails, `downcast_ref` back to `InventoryError` and print a SKU-specific suggestion.

<details>
<summary>Solution</summary>

```rust
mod inventory {
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum InventoryError {
        #[error("no such SKU `{0}`")]
        UnknownSku(String),
        #[error("SKU `{sku}` has only {available} in stock, requested {requested}")]
        Insufficient { sku: String, requested: u32, available: u32 },
    }

    pub fn reserve(sku: &str, requested: u32) -> Result<u32, InventoryError> {
        let available = match sku {
            "WIDGET" => 3,
            "GADGET" => 0,
            other => return Err(InventoryError::UnknownSku(other.to_string())),
        };
        if requested > available {
            return Err(InventoryError::Insufficient {
                sku: sku.to_string(),
                requested,
                available,
            });
        }
        Ok(available - requested)
    }
}

use anyhow::{Context, Result};

fn fulfill(sku: &str, qty: u32) -> Result<()> {
    let remaining = inventory::reserve(sku, qty)
        .with_context(|| format!("fulfilling order for {qty}x {sku}"))?;
    println!("reserved {qty}x {sku}, {remaining} left");
    Ok(())
}

fn main() {
    let _ = fulfill("WIDGET", 2);
    if let Err(e) = fulfill("WIDGET", 99) {
        println!("error: {e:#}");
    }
    if let Err(e) = fulfill("MYSTERY", 1) {
        // App reacts specifically to one library variant by downcasting.
        if let Some(inventory::InventoryError::UnknownSku(sku)) =
            e.downcast_ref::<inventory::InventoryError>()
        {
            println!("please add `{sku}` to the catalog");
        }
    }
}
```

Output:

```text
reserved 2x WIDGET, 1 left
error: fulfilling order for 99x WIDGET: SKU `WIDGET` has only 3 in stock, requested 99
please add `MYSTERY` to the catalog
```

The library exposes structure (`InventoryError`); the app stays coarse with `anyhow` and only re-introduces structure via `downcast_ref` where it genuinely reacts to a variant.

</details>

### Exercise 3: Choose recoverable vs. unrecoverable

**Difficulty:** Medium

**Objective:** Decide, for two functions, whether failure is a recoverable condition (`Result`) or a programmer bug (`panic`), and back the decision with tests.

**Instructions:**

1. Write `parse_hex_byte(raw: &str) -> Result<u8, String>` (recoverable — input may be malformed) and `channel_name(index: usize) -> &'static str` mapping `0..4` to `"red"/"green"/"blue"/"alpha"`.
2. Treat an out-of-range `index` as a *bug*: let indexing panic rather than returning a `Result`.
3. Write tests proving the parser returns `Err` on bad input and that `channel_name(9)` panics (use `#[should_panic]`).

<details>
<summary>Solution</summary>

```rust
/// Recoverable: user input may be malformed -> Result.
pub fn parse_hex_byte(raw: &str) -> Result<u8, String> {
    u8::from_str_radix(raw.trim_start_matches("0x"), 16)
        .map_err(|_| format!("`{raw}` is not a valid hex byte"))
}

/// Internal invariant: callers must pass an index < 4. A violation is a BUG,
/// so we let indexing panic rather than returning Result.
pub fn channel_name(index: usize) -> &'static str {
    const CHANNELS: [&str; 4] = ["red", "green", "blue", "alpha"];
    CHANNELS[index]
}

fn main() {
    println!("{:?}", parse_hex_byte("0xFF"));
    println!("{:?}", parse_hex_byte("zz"));
    println!("channel 2 = {}", channel_name(2));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_hex() {
        assert_eq!(parse_hex_byte("0x1A"), Ok(26));
    }

    #[test]
    fn rejects_invalid_hex() {
        assert!(parse_hex_byte("nope").is_err());
    }

    #[test]
    #[should_panic]
    fn channel_out_of_range_panics() {
        let _ = channel_name(9);
    }
}
```

`cargo run` prints:

```text
Ok(255)
Err("`zz` is not a valid hex byte")
channel 2 = blue
```

And `cargo test` reports `3 passed`. The parser returns a `Result` because malformed input is an expected runtime condition; `channel_name` panics on a bad index because only buggy calling code could produce one.

</details>
