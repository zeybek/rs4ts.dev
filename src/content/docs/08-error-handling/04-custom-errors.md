---
title: "Custom Error Types"
description: "Instead of subclassing Error like TS, Rust models failures as an enum or struct with Display and the Error trait, for compiler-checked, exhaustive cases."
---

In TypeScript you usually `throw new Error("...")` or subclass `Error`. Rust has no exceptions, so you instead **define a type** that lists exactly what can go wrong and return it inside `Result<T, E>`. This page is about designing those types by hand: as **enums** or **structs**, and wiring them up with the `Display` and `Error` traits so they behave like first-class errors.

---

## Quick Overview

A **custom error type** is just a normal Rust type (an `enum` or a `struct`) that you use as the `E` in `Result<T, E>`. Two things turn an ordinary type into a real error:

- **`Display`**: produces the human-readable, end-user-facing message (what `{}` prints).
- **`std::error::Error`**: the marker/interop trait that lets your type flow through `Box<dyn Error>`, the `?` operator, and crates like anyhow.

> **Note:** This page covers the hand-written approach so you understand what is really going on. In real libraries you will almost always derive these with the **thiserror** crate; see [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/). The mechanics here are exactly what that derive macro generates for you.

---

## TypeScript/JavaScript Example

In TypeScript, the idiomatic way to model distinct failures is subclassing `Error` and branching on the subclass with `instanceof`:

```typescript
// errors.ts — a small config loader with typed failures

class ConfigError extends Error {
  // `name` is what shows up in stack traces and logs
  constructor(message: string) {
    super(message);
    this.name = "ConfigError";
  }
}

class MissingKeyError extends ConfigError {
  constructor(public readonly key: string) {
    super(`missing required config key \`${key}\``);
    this.name = "MissingKeyError";
  }
}

class OutOfRangeError extends ConfigError {
  constructor(
    public readonly key: string,
    public readonly value: number,
    public readonly min: number,
    public readonly max: number,
  ) {
    super(`config key \`${key}\` = ${value} is out of range (${min}..${max})`);
    this.name = "OutOfRangeError";
  }
}

function readPort(key: string, raw: string | undefined): number {
  if (raw === undefined) {
    throw new MissingKeyError(key);
  }
  const parsed = Number(raw);
  if (!Number.isInteger(parsed)) {
    throw new ConfigError(`config key \`${key}\` has non-numeric value \`${raw}\``);
  }
  if (parsed < 1 || parsed > 65535) {
    throw new OutOfRangeError(key, parsed, 1, 65535);
  }
  return parsed;
}

try {
  console.log(readPort("PORT", "70000"));
} catch (err) {
  // The caller has to KNOW which subclasses exist to branch correctly
  if (err instanceof OutOfRangeError) {
    console.error(`bad range: ${err.message}`);
  } else if (err instanceof ConfigError) {
    console.error(`config problem: ${err.message}`);
  } else {
    throw err; // not ours — rethrow
  }
}
```

Two pain points a TypeScript developer feels here:

- `readPort` can throw, but its signature (`: number`) does not say so. The compiler will not stop you from forgetting the `try/catch`.
- In the `catch`, `err` is typed `unknown` (or `any`), so you reach for `instanceof` checks and hope you covered every subclass.

---

## Rust Equivalent

Rust encodes the failure modes in the type system. The set of things that can go wrong becomes an `enum`, and the function signature (`Result<u16, ConfigError>`) makes the possibility of failure impossible to ignore.

```rust playground
use std::fmt;

/// Possible failures when reading and validating a config value.
#[derive(Debug)]
enum ConfigError {
    /// A required key was absent from the source.
    MissingKey { key: String },
    /// The value was present but could not be parsed as a number.
    InvalidNumber { key: String, value: String },
    /// The value parsed but fell outside the accepted range.
    OutOfRange { key: String, value: i64, min: i64, max: i64 },
}

// `Display` = the human-readable, end-user-facing message.
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingKey { key } => {
                write!(f, "missing required config key `{key}`")
            }
            ConfigError::InvalidNumber { key, value } => {
                write!(f, "config key `{key}` has non-numeric value `{value}`")
            }
            ConfigError::OutOfRange { key, value, min, max } => {
                write!(f, "config key `{key}` = {value} is out of range ({min}..={max})")
            }
        }
    }
}

// Opting into the standard `Error` trait makes the type interoperable
// with `Box<dyn Error>`, the `?` operator, anyhow, and so on.
impl std::error::Error for ConfigError {}

/// Look up `key`, parse it as an integer, and check it is a valid port.
fn read_port(key: &str, raw: Option<&str>) -> Result<u16, ConfigError> {
    let value = raw.ok_or_else(|| ConfigError::MissingKey { key: key.to_string() })?;

    let parsed: i64 = value.parse().map_err(|_| ConfigError::InvalidNumber {
        key: key.to_string(),
        value: value.to_string(),
    })?;

    if !(1..=65535).contains(&parsed) {
        return Err(ConfigError::OutOfRange {
            key: key.to_string(),
            value: parsed,
            min: 1,
            max: 65535,
        });
    }

    Ok(parsed as u16)
}

fn main() {
    let cases = [
        ("PORT", Some("8080")),
        ("PORT", None),
        ("PORT", Some("eighty")),
        ("PORT", Some("70000")),
    ];

    for (key, raw) in cases {
        match read_port(key, raw) {
            Ok(port) => println!("ok: {port}"),
            Err(e) => println!("error: {e}"),
        }
    }
}
```

Running it prints (real output):

```text
ok: 8080
error: missing required config key `PORT`
error: config key `PORT` has non-numeric value `eighty`
error: config key `PORT` = 70000 is out of range (1..=65535)
```

> **Tip:** The `match` in `main` is **exhaustive on the `Result`**, not on the error variants, but a caller who *wants* to branch on the specific variant can `match` the `ConfigError` directly, and the compiler will force them to handle every case. That is the type-checked replacement for chained `instanceof` checks.

---

## Detailed Explanation

Let's walk through the pieces that turn a plain `enum` into a usable error.

### `#[derive(Debug)]` is mandatory

```rust
#[derive(Debug)]
enum ConfigError { /* ... */ }
```

Every error type must implement `Debug`. You almost never write it by hand; `#[derive(Debug)]` does it. It is required because the `Error` trait demands it (more below) and because `unwrap`/`expect`/test assertions print errors with `Debug`. This is the developer-facing representation; it shows the structure, e.g. `OutOfRange { key: "PORT", value: 70000, min: 1, max: 65535 }`.

### Each variant carries its own data

```rust
enum ConfigError {
    MissingKey { key: String },                              // struct-like variant
    InvalidNumber { key: String, value: String },
    OutOfRange { key: String, value: i64, min: i64, max: i64 },
}
```

Unlike a TypeScript subclass hierarchy, the variants live in one closed set. A variant can be:

- **unit-like**: `Empty` (no data),
- **tuple-like**: `UnknownProduct(String)`,
- **struct-like**: `OutOfRange { key, value, .. }` (named fields, as above).

Named fields read well when there are several pieces of context. Enums are covered in depth in [Section 06: Data Structures](/06-data-structures/); here we are just using them to enumerate failure modes.

### `impl Display` is the user-facing message

```rust
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { /* one arm per variant */ }
    }
}
```

`Display` is what `{}` in `println!`/`format!` uses, what a logger typically writes, and what `Box<dyn Error>`'s message comes from. You `match self` and `write!` the message for each variant. The `write!` macro returns `fmt::Result`, which is why the last expression in each arm doubles as the function's return value.

This is the rough analogue of setting `message` on a JavaScript `Error`, except it is computed on demand from the structured fields rather than baked in at construction time. That means the message can never drift out of sync with the data.

### `impl Error` is the interop opt-in

```rust
impl std::error::Error for ConfigError {}
```

The body is empty because `std::error::Error` has default methods for everything. Implementing it is a deliberate statement: *"this type is an error and may be used wherever an error is expected."* It enables:

- conversion into `Box<dyn Error>` (so `?` works in functions returning `Result<_, Box<dyn Error>>`),
- the `source()` cause chain (see [The `Error` Trait](/08-error-handling/05-error-trait/)),
- automatic acceptance by anyhow's `Context` and `?`.

The trait has a supertrait bound — `trait Error: Debug + Display` — which is precisely why you must provide both `Debug` and `Display` *before* you can implement `Error`.

### Constructing and returning errors

```rust
let value = raw.ok_or_else(|| ConfigError::MissingKey { key: key.to_string() })?;
```

`ok_or_else` converts an `Option` into a `Result`, building the error lazily only if the value is `None`. The `?` then returns early on `Err`. For converting a *different* error type (like `ParseIntError`) into yours, here we use `.map_err(...)`; the `From`-based automatic conversion that `?` can do is covered in [The `?` Operator](/08-error-handling/01-question-mark/).

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| Declaring failures | `class FooError extends Error` per case | one `enum` with a variant per case, or a `struct` |
| Is failure in the signature? | No, any function may `throw` | Yes, `Result<T, E>` is explicit |
| Branching on the case | `instanceof` chains on `unknown` | exhaustive `match` on the enum, compiler-checked |
| Human message | `this.message` set in constructor | `impl Display` computed from fields |
| "This is an error" marker | implicit (it extends `Error`) | explicit `impl std::error::Error` |
| Debug/developer view | `console.log(err)` / stack trace | `#[derive(Debug)]`, printed with `{:?}` |
| Underlying cause | `error.cause` (ES2022) | `Error::source()` returns `Option<&dyn Error>` |
| Forgetting a case | silent — runtime surprise | non-exhaustive `match` is a compile error |

**The big idea:** in TypeScript an error is "a thing you throw and discover at runtime." In Rust an error is "a value of a type the compiler tracks," so the *set* of failures is part of your API and the compiler enforces that every one is handled.

> **Note:** Unlike a TypeScript `class` hierarchy, a Rust error `enum` is **closed**: callers cannot add new variants, and you cannot subclass it. This is a feature: a `match` over the variants is provably complete. If you need an open-ended set, you reach for `Box<dyn Error>` instead (see [The `Error` Trait](/08-error-handling/05-error-trait/) and [Handling Multiple Error Types](/08-error-handling/07-multiple-errors/)).

### Enum vs struct: which to use?

Use an **enum** when a function can fail in several *distinct* ways the caller might want to tell apart:

```rust
enum CheckoutError {
    UnknownProduct(String),
    InsufficientStock { product: String, requested: u32, available: u32 },
    PaymentDeclined { reason: String },
}
```

Use a **struct** when there is essentially *one* failure mode, optionally wrapping an underlying cause:

```rust
use std::num::ParseIntError;

#[derive(Debug)]
struct ParseConfigError {
    key: String,
    source: ParseIntError, // the underlying error we wrap
}
```

The struct form shines when you want to attach context and expose a cause via `source()`:

```rust playground
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
struct ParseConfigError {
    key: String,
    source: ParseIntError,
}

impl fmt::Display for ParseConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to parse config key `{}`", self.key)
    }
}

impl Error for ParseConfigError {
    // Expose the underlying error so callers can walk the cause chain.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

fn parse_key(key: &str, value: &str) -> Result<i64, ParseConfigError> {
    value.parse().map_err(|source| ParseConfigError {
        key: key.to_string(),
        source,
    })
}

fn main() {
    if let Err(e) = parse_key("MAX_RETRIES", "abc") {
        println!("error: {e}");
        // Walk and print each underlying cause.
        let mut cause = e.source();
        while let Some(c) = cause {
            println!("  caused by: {c}");
            cause = c.source();
        }
    }
}
```

Real output:

```text
error: failed to parse config key `MAX_RETRIES`
  caused by: invalid digit found in string
```

The `source()` method is the structured equivalent of JavaScript's `error.cause`. The full cause-chain story lives in [The `Error` Trait](/08-error-handling/05-error-trait/).

---

## Common Pitfalls

### Pitfall 1: implementing `Error` before `Display`

`Error` requires `Display` as a supertrait. Implement `Error` on a type that has no `Display` and you get a clear error:

```rust
use std::error::Error;

#[derive(Debug)]
struct MyError {
    message: String,
}

impl Error for MyError {} // does not compile (error[E0277]: Display not implemented)

fn main() {
    let _ = MyError { message: "x".into() };
}
```

Real `rustc` output:

```text
error[E0277]: `MyError` doesn't implement `std::fmt::Display`
  --> src/main.rs:8:16
   |
 8 | impl Error for MyError {} // does not compile (error[E0277]: Display not implemented)
   |                ^^^^^^^ the trait `std::fmt::Display` is not implemented for `MyError`
   |
note: required by a bound in `std::error::Error`
  --> .../library/core/src/error.rs:53:26
   |
53 | pub trait Error: Debug + Display {
   |                          ^^^^^^^ required by this bound in `Error`
```

The fix: implement `Display` (and `#[derive(Debug)]`) first, then `impl Error`.

### Pitfall 2: forgetting `impl Error`, then trying to use `?` into `Box<dyn Error>`

Having `Debug` + `Display` is *not* enough to flow through `Box<dyn Error>`. You must also opt into `Error`:

```rust
use std::error::Error;
use std::fmt;

#[derive(Debug)]
struct MyError {
    message: String,
}

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

// NOTE: no `impl Error for MyError {}`

fn do_work() -> Result<(), Box<dyn Error>> {
    Err(MyError { message: "boom".into() })?; // does not compile (error[E0277])
    Ok(())
}

fn main() {
    let _ = do_work();
}
```

Real `rustc` output (abridged):

```text
error[E0277]: `?` couldn't convert the error: `MyError: std::error::Error` is not satisfied
  --> src/main.rs:18:44
   |
17 | fn do_work() -> Result<(), Box<dyn Error>> {
   |                 -------------------------- required `MyError: std::error::Error` because of this
18 |     Err(MyError { message: "boom".into() })?;
   |     ---------------------------------------^ the trait `std::error::Error` is not implemented for `MyError`
   |
   = note: the question mark operation (`?`) implicitly performs a conversion on the error value using the `From` trait
   = note: required for `Box<dyn std::error::Error>` to implement `From<MyError>`
```

The fix: add `impl Error for MyError {}`.

### Pitfall 3: printing with `{}` when you only derived `Debug`

A TypeScript developer expects "I can always print an error." In Rust, `{}` needs `Display`, which is *not* derivable:

```rust
#[derive(Debug)]
enum ValidationError {
    Empty,
    TooLong(usize),
}

fn main() {
    let e = ValidationError::TooLong(120);
    println!("{}", e); // does not compile (error[E0277]: Display not implemented)
}
```

Real `rustc` output (abridged):

```text
error[E0277]: `ValidationError` doesn't implement `std::fmt::Display`
 --> src/main.rs:9:20
  |
9 |     println!("{}", e); // does not compile (error[E0277]: Display not implemented)
  |               --   ^ `ValidationError` cannot be formatted with the default formatter
  |
  = help: the trait `std::fmt::Display` is not implemented for `ValidationError`
  = note: in format strings you may be able to use `{:?}` (or {:#?} for pretty-print) instead
```

The fix: implement `Display`, or (for quick debugging only) print with `{:?}` using the derived `Debug`.

> **Warning:** Do not implement `Display` by just delegating to `Debug` to silence this error. `Debug` is for developers and may include internals; `Display` is for users and should be a clean sentence. Keeping them distinct pays off the first time an error message ends up in a log, a CLI, or an HTTP response body.

---

## Best Practices

- **Always derive `Debug`; always implement `Display`; then implement `Error`.** That trio is the contract every error type should satisfy. If you find yourself writing all three by hand repeatedly, switch to thiserror; see [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/).
- **Make `Display` a clean, lowercase, no-trailing-period sentence.** The convention in the ecosystem is `"failed to read config file"`, not `"Failed to read config file."`. Callers compose messages (`"{context}: {err}"`), so leading capitals and periods read badly when chained.
- **Put the data in the variant, build the string in `Display`.** Store `OutOfRange { value, min, max }` rather than a pre-formatted `String`. Structured fields let callers `match` and react programmatically, and they keep the message in one place.
- **Choose granularity to match what callers can *do*.** A variant is worth adding only if a caller might handle it differently. If everyone just logs and bails, a single variant (or `Box<dyn Error>`) is fine. Granularity is discussed further in [Error-Handling Best Practices](/08-error-handling/08-best-practices/).
- **Implement `source()` whenever you wrap another error.** It preserves the cause chain for debugging and for tools that print full chains. Covered in [The `Error` Trait](/08-error-handling/05-error-trait/).
- **Keep `Debug` developer-oriented and `Display` user-oriented.** They serve different audiences; do not collapse them.

Here is the recommended shape, all three traits in place, with the three formatters shown side by side:

```rust playground
use std::fmt;

#[derive(Debug)]
enum OrderError {
    EmptyCart,
    ItemNotFound { sku: String },
}

impl fmt::Display for OrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderError::EmptyCart => write!(f, "cannot check out an empty cart"),
            OrderError::ItemNotFound { sku } => write!(f, "item not found: {sku}"),
        }
    }
}

impl std::error::Error for OrderError {}

fn main() {
    let e = OrderError::ItemNotFound { sku: "A-100".to_string() };
    println!("{e}");    // Display -> for users / logs
    println!("{e:?}");  // Debug   -> for developers
    println!("{e:#?}"); // Debug pretty-printed
}
```

Real output:

```text
item not found: A-100
ItemNotFound { sku: "A-100" }
ItemNotFound {
    sku: "A-100",
}
```

> **Tip:** Once you have written a hand-rolled enum like this a couple of times, the thiserror version is a near drop-in. The same `OrderError` becomes a derive with `#[error("...")]` attributes that generate exactly the `Display` impl you would have typed. See [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/).

---

## Real-World Example

A checkout flow that can fail in three meaningfully different ways. The domain layer returns a `CheckoutError`; the "transport" layer `match`es the variant to pick an HTTP-like status code. This is the pattern you would use behind an API handler.

```rust playground
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
enum CheckoutError {
    /// No product with this id exists.
    UnknownProduct(String),
    /// Requested more units than are in stock.
    InsufficientStock { product: String, requested: u32, available: u32 },
    /// The card was declined by the processor.
    PaymentDeclined { reason: String },
}

impl fmt::Display for CheckoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckoutError::UnknownProduct(id) => {
                write!(f, "unknown product `{id}`")
            }
            CheckoutError::InsufficientStock { product, requested, available } => {
                write!(
                    f,
                    "not enough `{product}` in stock: requested {requested}, only {available} available"
                )
            }
            CheckoutError::PaymentDeclined { reason } => {
                write!(f, "payment declined: {reason}")
            }
        }
    }
}

impl Error for CheckoutError {}

struct Inventory {
    stock: HashMap<String, u32>,
}

impl Inventory {
    fn checkout(&self, product: &str, qty: u32) -> Result<u32, CheckoutError> {
        let available = self
            .stock
            .get(product)
            .copied()
            .ok_or_else(|| CheckoutError::UnknownProduct(product.to_string()))?;

        if qty > available {
            return Err(CheckoutError::InsufficientStock {
                product: product.to_string(),
                requested: qty,
                available,
            });
        }

        if qty > 100 {
            return Err(CheckoutError::PaymentDeclined {
                reason: "amount exceeds single-order limit".to_string(),
            });
        }

        Ok(available - qty)
    }
}

/// Map a domain error to an HTTP-like status code by matching on the variant.
fn status_code(err: &CheckoutError) -> u16 {
    match err {
        CheckoutError::UnknownProduct(_) => 404,
        CheckoutError::InsufficientStock { .. } => 409,
        CheckoutError::PaymentDeclined { .. } => 402,
    }
}

fn main() {
    let inv = Inventory {
        stock: HashMap::from([("widget".to_string(), 5), ("gadget".to_string(), 200)]),
    };

    let attempts = [("widget", 2), ("widget", 9), ("bolt", 1), ("gadget", 150)];
    for (product, qty) in attempts {
        match inv.checkout(product, qty) {
            Ok(remaining) => println!("200 OK  -> {remaining} {product} left"),
            Err(e) => println!("{} -> {e}", status_code(&e)),
        }
    }
}
```

Real output:

```text
200 OK  -> 3 widget left
409 -> not enough `widget` in stock: requested 9, only 5 available
404 -> unknown product `bolt`
402 -> payment declined: amount exceeds single-order limit
```

Notice how `status_code` is an exhaustive `match`: add a fourth variant to `CheckoutError` and the compiler immediately points at this function until you decide what status it deserves. That is the safety net `instanceof` chains never give you.

---

## Further Reading

### Official documentation

- [The Rust Book — Defining an Error Type](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html)
- [`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html)
- [`std::fmt::Display`](https://doc.rust-lang.org/std/fmt/trait.Display.html)
- [Rust by Example — Defining error types](https://doc.rust-lang.org/rust-by-example/error/multiple_error_types/define_error_type.html)

### Related sections in this guide

- [Result and Option](/08-error-handling/00-result-option/) — how `Result<T, E>` and `Option<T>` replace `try/catch`; where your custom error becomes the `E`.
- [The `?` Operator](/08-error-handling/01-question-mark/) — the `?` operator and `From`-based conversion between error types.
- [The `Error` Trait](/08-error-handling/05-error-trait/) — `std::error::Error` in depth: `Debug + Display` bounds, the `source()` chain, `Box<dyn Error>`.
- [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/) — deriving everything on this page automatically with **thiserror 2.x** (and when to reach for anyhow instead).
- [Handling Multiple Error Types](/08-error-handling/07-multiple-errors/) — aggregating several error types into one enum with `#[from]`.
- [Error-Handling Best Practices](/08-error-handling/08-best-practices/) — designing errors for libraries vs applications, and choosing granularity.
- [Section 06: Data Structures](/06-data-structures/) — enums and structs, the building blocks used here.
- [Section 09: Generics & Traits](/09-generics-traits/) — how `Display` and `Error` fit into Rust's trait system.

---

## Exercises

### Exercise 1: A single-failure struct error

**Difficulty:** Easy

**Objective:** Define a struct error and implement `Display` and `Error`.

**Instructions:** Write a function `validate_username(name: &str) -> Result<&str, EmptyUsername>` that returns `Err` when the (trimmed) name is empty. Define `EmptyUsername` as a unit struct, derive `Debug`, implement `Display` with the message `"username must not be empty"`, and implement `std::error::Error`.

```rust
use std::error::Error;
use std::fmt;

#[derive(Debug)]
struct EmptyUsername;

// TODO: impl Display for EmptyUsername
// TODO: impl Error for EmptyUsername

fn validate_username(name: &str) -> Result<&str, EmptyUsername> {
    // TODO
    todo!()
}

fn main() {
    println!("{:?}", validate_username("alice"));
    match validate_username("   ") {
        Ok(n) => println!("ok: {n}"),
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
struct EmptyUsername;

impl fmt::Display for EmptyUsername {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "username must not be empty")
    }
}

impl Error for EmptyUsername {}

fn validate_username(name: &str) -> Result<&str, EmptyUsername> {
    if name.trim().is_empty() {
        Err(EmptyUsername)
    } else {
        Ok(name)
    }
}

fn main() {
    println!("{:?}", validate_username("alice"));
    match validate_username("   ") {
        Ok(n) => println!("ok: {n}"),
        Err(e) => println!("error: {e}"),
    }
}
```

Output:

```text
Ok("alice")
error: username must not be empty
```

</details>

### Exercise 2: An enum error that exposes a cause

**Difficulty:** Medium

**Objective:** Build a multi-variant error, one of whose variants wraps a standard-library error, and expose that cause via `source()`.

**Instructions:** Write `parse_temp(raw: &str) -> Result<f64, TempError>` that:

- returns `TempError::Empty` if `raw` is blank,
- returns `TempError::NotANumber(ParseFloatError)` if it does not parse as `f64`,
- returns `TempError::BelowAbsoluteZero(f64)` if the value is below `-273.15`.

Implement `Display` for all three variants and implement `Error::source()` so that the `NotANumber` variant returns its inner `ParseFloatError`.

```rust
use std::error::Error;
use std::fmt;
use std::num::ParseFloatError;

#[derive(Debug)]
enum TempError {
    Empty,
    NotANumber(ParseFloatError),
    BelowAbsoluteZero(f64),
}

// TODO: impl Display
// TODO: impl Error with a source() that returns the inner ParseFloatError

fn parse_temp(raw: &str) -> Result<f64, TempError> {
    // TODO
    todo!()
}

fn main() {
    for raw in ["21.5", "", "hot", "-300"] {
        match parse_temp(raw) {
            Ok(c) => println!("ok: {c} C"),
            Err(e) => {
                print!("error: {e}");
                if let Some(src) = e.source() {
                    print!(" (cause: {src})");
                }
                println!();
            }
        }
    }
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::error::Error;
use std::fmt;
use std::num::ParseFloatError;

#[derive(Debug)]
enum TempError {
    Empty,
    NotANumber(ParseFloatError),
    BelowAbsoluteZero(f64),
}

impl fmt::Display for TempError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TempError::Empty => write!(f, "no temperature provided"),
            TempError::NotANumber(_) => write!(f, "temperature is not a valid number"),
            TempError::BelowAbsoluteZero(t) => {
                write!(f, "{t} C is below absolute zero (-273.15 C)")
            }
        }
    }
}

impl Error for TempError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TempError::NotANumber(e) => Some(e),
            _ => None,
        }
    }
}

fn parse_temp(raw: &str) -> Result<f64, TempError> {
    if raw.trim().is_empty() {
        return Err(TempError::Empty);
    }
    let celsius: f64 = raw.trim().parse().map_err(TempError::NotANumber)?;
    if celsius < -273.15 {
        return Err(TempError::BelowAbsoluteZero(celsius));
    }
    Ok(celsius)
}

fn main() {
    for raw in ["21.5", "", "hot", "-300"] {
        match parse_temp(raw) {
            Ok(c) => println!("ok: {c} C"),
            Err(e) => {
                print!("error: {e}");
                if let Some(src) = e.source() {
                    print!(" (cause: {src})");
                }
                println!();
            }
        }
    }
}
```

Output:

```text
ok: 21.5 C
error: no temperature provided
error: temperature is not a valid number (cause: invalid float literal)
error: -300 C is below absolute zero (-273.15 C)
```

</details>

### Exercise 3: A library-grade error with a cause chain

**Difficulty:** Hard

**Objective:** Design a complete, library-style error and verify the full cause chain prints correctly.

**Instructions:** Build a tiny CSV column summer:

- `CsvError::BadHeader { expected, found }` when the input has no rows.
- `CsvError::BadCell { row, source: ParseIntError }` when a data row does not parse as an integer.

Implement `Display` and `Error` (with `source()` returning the inner `ParseIntError` for `BadCell`). Then write `sum_column(lines: &[&str]) -> Result<i64, CsvError>` that treats `lines[0]` as a header and sums the rest, propagating parse failures with `?`. Finally, print the error and walk its `source()` chain.

<details>
<summary>Solution</summary>

```rust playground
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
enum CsvError {
    BadHeader { expected: usize, found: usize },
    BadCell { row: usize, source: ParseIntError },
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CsvError::BadHeader { expected, found } => {
                write!(f, "bad header: expected {expected} columns, found {found}")
            }
            CsvError::BadCell { row, .. } => {
                write!(f, "could not parse integer in row {row}")
            }
        }
    }
}

impl Error for CsvError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CsvError::BadCell { source, .. } => Some(source),
            CsvError::BadHeader { .. } => None,
        }
    }
}

fn parse_row(row: usize, line: &str) -> Result<i64, CsvError> {
    line.trim()
        .parse()
        .map_err(|source| CsvError::BadCell { row, source })
}

fn sum_column(lines: &[&str]) -> Result<i64, CsvError> {
    if lines.is_empty() {
        return Err(CsvError::BadHeader { expected: 1, found: 0 });
    }
    let mut total = 0;
    for (i, line) in lines.iter().enumerate().skip(1) {
        total += parse_row(i, line)?;
    }
    Ok(total)
}

fn report(result: Result<i64, CsvError>) {
    match result {
        Ok(total) => println!("total = {total}"),
        Err(e) => {
            print!("error: {e}");
            let mut src = e.source();
            while let Some(c) = src {
                print!(" -> {c}");
                src = c.source();
            }
            println!();
        }
    }
}

fn main() {
    report(sum_column(&["amount", "10", "20", "30"]));
    report(sum_column(&["amount", "10", "oops", "30"]));
    report(sum_column(&[]));
}
```

Output:

```text
total = 60
error: could not parse integer in row 2 -> invalid digit found in string
error: bad header: expected 1 columns, found 0
```

Once this compiles, try replacing the hand-written `Display` and `Error` impls with thiserror's `#[derive(Error)]` and `#[error("...")]`/`#[source]` attributes from [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/); the behavior is identical, with far less boilerplate.

</details>
