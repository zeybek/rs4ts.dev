---
title: "The `?` Operator"
description: "Rust's ? operator propagates an Err or None to the caller with one early return, replacing TypeScript's invisible exception unwinding and converting errors via From."
---

The `?` operator is Rust's lightweight error-propagation syntax. It replaces the "let it bubble up" behavior you get for free with exceptions in TypeScript/JavaScript — but it does so explicitly, in the type system, with zero hidden control flow.

---

## Quick Overview

In TypeScript, a `throw` deep inside a call stack silently unwinds until something `catch`es it; the function signatures say nothing about what can go wrong. In Rust there are no exceptions — fallible functions return a [`Result<T, E>`](/08-error-handling/00-result-option/) (or an [`Option<T>`](/08-error-handling/00-result-option/)), and the **`?` operator** is the ergonomic way to say "if this is an `Err`/`None`, stop here and return it to my caller; otherwise give me the value inside." Importantly, `?` also runs an automatic `From`-based **error conversion**, so a function can collect several different underlying error types into one declared error type.

---

## TypeScript/JavaScript Example

In TypeScript, propagation is implicit. A thrown error travels up the stack on its own, and the type system does not record which functions can fail:

```typescript
// Each step can throw; the throws are invisible in the signatures.
function parsePort(raw: string): number {
  const port = Number.parseInt(raw, 10);
  if (Number.isNaN(port)) {
    throw new Error(`invalid port: ${raw}`);
  }
  return port;
}

function loadConfig(env: Record<string, string>): { host: string; port: number } {
  const host = env.HOST;           // could be undefined — TS won't force a check here
  if (host === undefined) {
    throw new Error("missing HOST");
  }
  const port = parsePort(env.PORT); // if this throws, loadConfig throws too — implicitly
  return { host, port };
}

try {
  const config = loadConfig({ HOST: "localhost", PORT: "oops" });
  console.log(config);
} catch (err) {
  // `err` is typed `unknown` in modern TS — you must narrow it yourself.
  console.error("failed:", (err as Error).message); // failed: invalid port: oops
}
```

The call `parsePort(env.PORT)` has no syntactic marker that it might throw. The failure path is real but invisible, and the only place it becomes visible is the far-away `try/catch`.

---

## Rust Equivalent

In Rust the fallibility is in the return type, and each propagation point is marked with a single `?`:

```rust
use std::fmt;
use std::num::ParseIntError;

// A custom error enum that aggregates the two ways this code can fail.
#[derive(Debug)]
enum ConfigError {
    Missing(String),
    Parse(ParseIntError),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(key) => write!(f, "missing key: {key}"),
            ConfigError::Parse(e) => write!(f, "invalid number: {e}"),
        }
    }
}

// This is the piece that makes `?` magical: it tells `?` how to turn a
// ParseIntError into a ConfigError automatically.
impl From<ParseIntError> for ConfigError {
    fn from(e: ParseIntError) -> Self {
        ConfigError::Parse(e)
    }
}

fn read_port(raw: Option<&str>) -> Result<u16, ConfigError> {
    // `ok_or_else` turns the `Option` into a `Result` so `?` can propagate it.
    let raw = raw.ok_or_else(|| ConfigError::Missing("PORT".to_string()))?;
    // `?` here converts ParseIntError -> ConfigError via the `From` impl above.
    let port: u16 = raw.parse()?;
    Ok(port)
}

fn main() {
    println!("{:?}", read_port(Some("8080")));
    match read_port(Some("notanumber")) {
        Ok(p) => println!("port = {p}"),
        Err(e) => println!("error: {e}"),
    }
    match read_port(None) {
        Ok(p) => println!("port = {p}"),
        Err(e) => println!("error: {e}"),
    }
}
```

Real output:

```text
Ok(8080)
error: invalid number: invalid digit found in string
error: missing key: PORT
```

Every line that can fail ends in `?`. There is no hidden unwinding: the `?` *is* the early return, and it is right there in the source.

---

## Detailed Explanation

### What `?` actually does

When you write `expr?` and `expr` is a `Result<T, E>`:

- If it is `Ok(value)`, the whole `expr?` expression **evaluates to `value`** and execution continues.
- If it is `Err(e)`, the function **returns early** with `Err(e.into())`. Note the `.into()`, which performs the `From` conversion.

So this single character:

```rust
let port: u16 = raw.parse()?;
```

is shorthand for this explicit `match` (verified equivalent):

```rust
use std::num::ParseIntError;

fn manual(s: &str) -> Result<i32, ParseIntError> {
    let n = match s.parse::<i32>() {
        Ok(v) => v,
        Err(e) => return Err(From::from(e)), // `?` inserts this From::from conversion
    };
    Ok(n * 2)
}
```

> **Note:** The early `return` is the key contrast with TypeScript. In TS, `throw` unwinds the stack through *every* frame until a `catch`. In Rust, `?` returns from *exactly one* function: the one it appears in. To propagate further, the caller must also use `?` (or otherwise handle the `Result`). Propagation is opt-in at every level.

### The `From` conversion is the secret sauce

The `.into()` that `?` inserts is what lets a function unify *different* error types. In `read_port`, `raw.parse()` produces a `Result<u16, ParseIntError>`, but the function returns `Result<u16, ConfigError>`. Those error types differ. `?` bridges the gap by calling `ConfigError::from(parse_int_error)`, which exists because we implemented `From<ParseIntError> for ConfigError`.

This means a single `?` does two jobs at once:

1. **Propagate** the error (early return on `Err`).
2. **Convert** it to the function's declared error type.

If no suitable `From` impl exists, the code does not compile; see [Common Pitfalls](#common-pitfalls).

### `?` on `Option`

`?` is not limited to `Result`. On an `Option<T>`:

- `Some(value)` evaluates to `value`.
- `None` returns `None` from the enclosing function early.

```rust
fn first_char_upper(s: &str) -> Option<char> {
    let first = s.chars().next()?; // returns None early if the string is empty
    Some(first.to_ascii_uppercase())
}

fn main() {
    println!("{:?}", first_char_upper("hello")); // Some('H')
    println!("{:?}", first_char_upper(""));       // None
}
```

This is the closest Rust analogue to chaining JavaScript's optional chaining (`?.`): `a?.b?.c` short-circuits to `undefined` on the first nullish hop, just as `?` short-circuits to `None` on the first `None`.

```rust
use std::collections::HashMap;

// Look up an outer key, then an inner key — short-circuit to None on the first miss.
fn nested_lookup<'a>(
    data: &'a HashMap<String, HashMap<String, String>>,
    outer: &str,
    inner: &str,
) -> Option<&'a str> {
    let inner_map = data.get(outer)?; // None if `outer` missing
    let value = inner_map.get(inner)?; // None if `inner` missing
    Some(value.as_str())
}
```

> **Tip:** `?` works in any function whose return type implements the `Try` machinery; in practice that means `Result<T, E>` and `Option<T>`. You cannot mix them implicitly: `?` on a `Result` inside an `Option`-returning function will not compile. Convert between them with `.ok()` (Result → Option) or `.ok_or(...)` / `.ok_or_else(...)` (Option → Result), shown below.

### Bridging `Option` and `Result`

Because `?` demands the right "shape," you will often convert one into the other so `?` lines up with your function's return type:

```rust
use std::num::ParseIntError;

fn bridges() {
    // Option -> Result, so `?` can propagate in a Result-returning function.
    let some: Option<i32> = Some(5);
    let as_result: Result<i32, &str> = some.ok_or("was none");
    println!("{:?}", as_result); // Ok(5)

    // Result -> Option (discarding the error), for an Option-returning function.
    let res: Result<i32, ParseIntError> = "abc".parse::<i32>();
    let as_option: Option<i32> = res.ok();
    println!("{:?}", as_option); // None
}
```

### `?` in `fn main`

`main` may itself return a `Result`, which lets you use `?` at the top level. If `main` returns `Err`, the runtime prints the error's `Debug` representation and exits with a non-zero status code:

```rust
use std::error::Error;

fn parse_config(raw: &str) -> Result<i32, Box<dyn Error>> {
    let n: i32 = raw.parse()?; // ParseIntError -> Box<dyn Error> automatically
    Ok(n * 10)
}

fn main() -> Result<(), Box<dyn Error>> {
    let value = parse_config("oops")?; // propagates; main exits with Err
    println!("value = {value}");
    Ok(())
}
```

Real output (and the process exits with status `1`):

```text
Error: ParseIntError { kind: InvalidDigit }
```

> **Note:** `Box<dyn Error>` is a trait object that can hold *any* error type, which is why `?` can absorb both a `ParseIntError` and an `std::io::Error` in the same function: the standard library provides blanket `From` impls into `Box<dyn Error>`. This is the easy-mode error type for applications and `main`; see [Box\<dyn Error\>](/08-error-handling/05-error-trait/) and [handling multiple errors](/08-error-handling/07-multiple-errors/).

---

## Key Differences

| Aspect | TypeScript/JavaScript | Rust (`?`) |
| --- | --- | --- |
| Propagation | Implicit `throw` unwinds the whole stack | Explicit `?` returns from one function only |
| Visible in signature? | No: any function may throw anything | Yes: `Result<T, E>` / `Option<T>` declares it |
| What is propagated | Any value (usually `Error`, but anything) | A typed `E` (or `None`) |
| Type conversion | None; you `catch` and re-`throw` manually | Automatic via the `From` trait at each `?` |
| Catch site | A `try/catch` somewhere up the stack | A `match`, `if let`, or combinator on the `Result` |
| Cost | Stack unwinding machinery | A plain conditional branch + early return |

The headline difference: in TypeScript the *absence* of error handling is the default and propagation is free; in Rust the *presence* of an error in the type is mandatory and propagation costs you one `?`. This is more typing, but the compiler now guarantees you have not silently ignored a failure path.

> **Warning:** `?` is **not** a `try/catch`. It does not *handle* an error — it *forwards* it. The actual handling (logging, retrying, mapping to an HTTP status) happens wherever you finally stop using `?` and pattern-match the `Result` instead. Think of `?` as the `await`-style ergonomics for the unhappy path, not as a recovery mechanism.

---

## Common Pitfalls

### Pitfall 1: Using `?` in a function that returns a plain value

`?` needs a `Result`- or `Option`-shaped return type. Putting it in a function that returns, say, `i32` fails to compile:

```rust
fn parse_double(s: &str) -> i32 {
    let n = s.parse::<i32>()?; // does not compile (error[E0277])
    n * 2
}
```

Real compiler error:

```text
error[E0277]: the `?` operator can only be used in a function that returns `Result` or `Option` (or another type that implements `FromResidual`)
 --> src/main.rs:2:29
  |
1 | fn parse_double(s: &str) -> i32 {
  | ------------------------------- this function should return `Result` or `Option` to accept `?`
2 |     let n = s.parse::<i32>()?; // does not compile (error[E0277])
  |                             ^ cannot use the `?` operator in a function that returns `i32`
```

The fix is to change the return type to `Result<i32, ParseIntError>` (and return `Ok(n * 2)`), or to handle the error locally with `match`, `unwrap_or`, etc.

### Pitfall 2: Using `?` on a `Result` inside an `Option`-returning function

`?` will not silently change a `Result` into an `Option`. The error types and shapes must match the function's return type:

```rust
fn first_num(s: &str) -> Option<i32> {
    let n = s.parse::<i32>()?; // does not compile (error[E0277])
    Some(n)
}
```

Real compiler error:

```text
error[E0277]: the `?` operator can only be used on `Option`s, not `Result`s, in a function that returns `Option`
 --> src/main.rs:2:29
  |
1 | fn first_num(s: &str) -> Option<i32> {
  | ------------------------------------ this function returns an `Option`
2 |     let n = s.parse::<i32>()?; // does not compile (error[E0277])
  |                             ^ use `.ok()?` if you want to discard the `Result<Infallible, ParseIntError>` error information
```

The compiler even suggests the fix: `s.parse::<i32>().ok()?` turns the `Result` into an `Option` (throwing away the error detail) before applying `?`.

### Pitfall 3: No `From` impl for the function's error type

When the error type produced by `?` cannot be converted into the function's declared error type, the code does not compile. This is the most common surprise for people defining their own error enums:

```rust
#[derive(Debug)]
struct MyError;

fn read_number(s: &str) -> Result<i32, MyError> {
    let n = s.parse::<i32>()?; // does not compile (error[E0277]): no From<ParseIntError> for MyError
    Ok(n)
}
```

Real compiler error (trimmed):

```text
error[E0277]: `?` couldn't convert the error to `MyError`
 --> src/main.rs:5:29
  |
4 | fn read_number(s: &str) -> Result<i32, MyError> {
  |                            -------------------- expected `MyError` because of this
5 |     let n = s.parse::<i32>()?; // does not compile ...
  |               --------------^ the trait `From<ParseIntError>` is not implemented for `MyError`
...
note: `MyError` needs to implement `From<ParseIntError>`
  = note: the question mark operation (`?`) implicitly performs a conversion on the error value using the `From` trait
```

The fix is to `impl From<ParseIntError> for MyError`, or — far more commonly in real code — derive it with `thiserror`'s `#[from]` attribute (see [anyhow & thiserror](/08-error-handling/06-anyhow-thiserror/)). The compiler note spelling out "implicitly performs a conversion on the error value using the `From` trait" is your reminder that `?` and `From` are inseparable.

### Pitfall 4: Expecting `?` to "handle" the error

A TypeScript developer sometimes reads `value?` as "try this and recover." It does not recover — it forwards. If you want a fallback value instead of propagation, reach for `unwrap_or`, `unwrap_or_else`, `unwrap_or_default`, or a `match`. (`unwrap`/`expect` are a different, panicking story; see [unwrap & expect](/08-error-handling/03-unwrap-expect/).)

---

## Best Practices

### Prefer `?` over manual `match` for pure propagation

If a `match` arm's only job is `Err(e) => return Err(e.into())`, replace the whole thing with `?`. It is shorter, conventional, and makes the happy path readable top-to-bottom.

### Let `?` do your conversions — design error types with `From` in mind

The most ergonomic custom error types implement `From` for each underlying error they wrap, so that `?` "just works" at every call site. Writing those impls by hand is tedious, so libraries should use `thiserror`'s `#[from]`:

```rust
// Sketch — see the anyhow & thiserror topic for the full, compile-verified version.
#[derive(Debug, thiserror::Error)]
enum ConfigError {
    #[error("missing key: {0}")]
    Missing(String),
    #[error("invalid number")]
    Parse(#[from] std::num::ParseIntError), // generates From<ParseIntError>
}
```

### In applications and `main`, reach for `Box<dyn Error>` or `anyhow`

When you do not need to *match* on specific error variants (you just want to propagate and eventually log), `Result<T, Box<dyn Error>>` or `anyhow::Result<T>` lets `?` absorb any error with no per-type `From` impls. Reserve precise enums for *library* APIs whose callers must distinguish failure modes. This application-vs-library split is covered in [best practices](/08-error-handling/08-best-practices/).

### Keep `?` chains shallow and readable

`?` lets you write straight-line code where each step assumes the previous one succeeded. Lean into that: name intermediate values, keep one fallible operation per line, and let the early returns flatten what would otherwise be deeply nested error handling.

### Use `let ... else` when you want to bail without a `?`-compatible type

When you are pattern-matching an `Option`/`Result` and want to `return` (or `continue`/`break`) on the failing case with a *custom* action rather than propagation, `let ... else` is often clearer than forcing a `?`:

```rust
fn describe(maybe_name: Option<&str>) -> String {
    let Some(name) = maybe_name else {
        return "anonymous".to_string();
    };
    format!("Hello, {name}")
}
```

---

## Real-World Example

A small but production-flavored configuration loader. It parses a `KEY = VALUE` block, collects two distinct error kinds (missing fields and bad numbers) into one error type, and uses `?` at every fallible step. Note how `?` short-circuits on the *first* failure encountered.

```rust
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
struct ServerConfig {
    host: String,
    port: u16,
    max_connections: u32,
}

#[derive(Debug)]
enum ConfigError {
    MissingField(&'static str),
    InvalidNumber { field: &'static str, source: ParseIntError },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingField(name) => write!(f, "missing required field `{name}`"),
            ConfigError::InvalidNumber { field, source } => {
                write!(f, "field `{field}` is not a valid number: {source}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

fn get<'a>(lines: &[(&'a str, &'a str)], key: &'static str) -> Result<&'a str, ConfigError> {
    lines
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .ok_or(ConfigError::MissingField(key)) // Option -> Result so `?` can use it
}

fn parse_num<T: std::str::FromStr<Err = ParseIntError>>(
    raw: &str,
    field: &'static str,
) -> Result<T, ConfigError> {
    // Attach context (which field) while converting the error.
    raw.parse::<T>()
        .map_err(|source| ConfigError::InvalidNumber { field, source })
}

fn load_config(input: &str) -> Result<ServerConfig, ConfigError> {
    let lines: Vec<(&str, &str)> = input
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect();

    // Each `?` short-circuits and returns the first error encountered.
    let host = get(&lines, "host")?.to_string();
    let port = parse_num::<u16>(get(&lines, "port")?, "port")?;
    let max_connections = parse_num::<u32>(get(&lines, "max_connections")?, "max_connections")?;

    Ok(ServerConfig { host, port, max_connections })
}

fn main() {
    let good = "host = localhost\nport = 8080\nmax_connections = 256";
    match load_config(good) {
        Ok(cfg) => println!("loaded: {cfg:?}"),
        Err(e) => eprintln!("config error: {e}"),
    }

    let bad_port = "host = localhost\nport = http\nmax_connections = 256";
    match load_config(bad_port) {
        Ok(cfg) => println!("loaded: {cfg:?}"),
        Err(e) => eprintln!("config error: {e}"),
    }

    let missing = "host = localhost\nport = 8080";
    match load_config(missing) {
        Ok(cfg) => println!("loaded: {cfg:?}"),
        Err(e) => eprintln!("config error: {e}"),
    }
}
```

Real output:

```text
loaded: ServerConfig { host: "localhost", port: 8080, max_connections: 256 }
config error: field `port` is not a valid number: invalid digit found in string
config error: missing required field `max_connections`
```

The body of `load_config` reads like the happy path — host, then port, then max connections — and the `?`s quietly guarantee that any failure stops and returns immediately. That is the same readability you get from `await`-laden async code, but for errors instead of asynchrony.

> **Note:** The standard library does not provide a blanket `From` between two *different* concrete error types, but it *does* provide blanket conversions into `Box<dyn Error>`. So if you swap `ConfigError` for `Box<dyn Error>`, `?` can absorb `ParseIntError`, `std::io::Error`, and friends in the same function with no hand-written `From` impls at all:
>
> ```rust
> use std::error::Error;
>
> fn process(num: &str, path: &str) -> Result<usize, Box<dyn Error>> {
>     let n: i32 = num.parse()?;                      // ParseIntError
>     let contents = std::fs::read_to_string(path)?;  // std::io::Error
>     Ok(contents.len() + n as usize)
> }
> ```
>
> Verified: calling `process("10", "/no/such/file")` yields `No such file or directory (os error 2)`, and `process("ten", "/tmp")` yields `invalid digit found in string`: two different error types, one `?` each.

---

## Further Reading

### Official documentation

- [The Rust Book - "A Shortcut for Propagating Errors: the `?` Operator"](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html#a-shortcut-for-propagating-errors-the--operator)
- [The Rust Reference - The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)
- [`std::convert::From`](https://doc.rust-lang.org/std/convert/trait.From.html): the conversion `?` relies on
- [`Result::ok`](https://doc.rust-lang.org/std/result/enum.Result.html#method.ok) and [`Option::ok_or`](https://doc.rust-lang.org/std/option/enum.Option.html#method.ok_or): bridging the two types

### Related sections in this guide

- [Result and Option](/08-error-handling/00-result-option/) — the types `?` operates on, and how to match on them
- [Custom error types](/08-error-handling/04-custom-errors/): defining the `E` your `?` will produce
- [The `Error` trait & `Box<dyn Error>`](/08-error-handling/05-error-trait/) — the trait object that lets `?` absorb anything
- [Handling multiple error types](/08-error-handling/07-multiple-errors/): `#[from]` and enum aggregation in depth
- [anyhow & thiserror](/08-error-handling/06-anyhow-thiserror/) — deriving the `From` impls `?` needs
- [Panics](/08-error-handling/02-panic/) and [unwrap & expect](/08-error-handling/03-unwrap-expect/) — what to use when you do *not* want to propagate
- [Error-handling best practices](/08-error-handling/08-best-practices/) — libraries vs. applications
- Background: [Why Rust](/01-getting-started/00-why-rust/), [Operators](/02-basics/02-operators/), and [the introduction](/00-introduction/)
- Coming next: [Generics & Traits](/09-generics-traits/): `From` and the trait machinery behind `?`

---

## Exercises

### Exercise 1: Sum a list of strings

**Difficulty:** Easy

**Objective:** Use `?` to propagate a parse error out of a loop.

**Instructions:** Write `sum_all(inputs: &[&str]) -> Result<i32, ParseIntError>` that parses each string to an `i32` and returns their sum. The first unparseable string should make the whole function return its `Err`. Call it with `["1", "2", "3"]` and with `["1", "x", "3"]`.

```rust
use std::num::ParseIntError;

fn sum_all(inputs: &[&str]) -> Result<i32, ParseIntError> {
    // TODO: loop over inputs, use `?` on each parse, accumulate the total
    /* ??? */
}

fn main() {
    println!("{:?}", sum_all(&["1", "2", "3"]));
    println!("{:?}", sum_all(&["1", "x", "3"]));
}
```

<details>
<summary>Solution</summary>

```rust
use std::num::ParseIntError;

fn sum_all(inputs: &[&str]) -> Result<i32, ParseIntError> {
    let mut total = 0;
    for s in inputs {
        total += s.parse::<i32>()?; // `?` returns early on the first bad string
    }
    Ok(total)
}

fn main() {
    println!("{:?}", sum_all(&["1", "2", "3"])); // Ok(6)
    println!("{:?}", sum_all(&["1", "x", "3"])); // Err(ParseIntError { kind: InvalidDigit })
}
```

Verified output:

```text
Ok(6)
Err(ParseIntError { kind: InvalidDigit })
```

</details>

### Exercise 2: Chained `Option` lookups

**Difficulty:** Medium

**Objective:** Use `?` on `Option` to short-circuit a two-level lookup, mirroring TypeScript's `?.` chaining.

**Instructions:** Given a `HashMap<String, HashMap<String, String>>`, write `nested_lookup(data, outer, inner) -> Option<&str>` that returns the inner value if both keys exist, or `None` if either is missing. Use `?` (not nested `match`). Test with an existing outer+inner pair, an existing outer with a missing inner, and a missing outer.

```rust
use std::collections::HashMap;

fn nested_lookup<'a>(
    data: &'a HashMap<String, HashMap<String, String>>,
    outer: &str,
    inner: &str,
) -> Option<&'a str> {
    // TODO: use `?` twice, then return Some(...)
    /* ??? */
}

fn main() {
    let mut data = HashMap::new();
    let mut user = HashMap::new();
    user.insert("email".to_string(), "ada@example.com".to_string());
    data.insert("ada".to_string(), user);

    println!("{:?}", nested_lookup(&data, "ada", "email"));
    println!("{:?}", nested_lookup(&data, "ada", "phone"));
    println!("{:?}", nested_lookup(&data, "bob", "email"));
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

fn nested_lookup<'a>(
    data: &'a HashMap<String, HashMap<String, String>>,
    outer: &str,
    inner: &str,
) -> Option<&'a str> {
    let inner_map = data.get(outer)?; // None early if `outer` is missing
    let value = inner_map.get(inner)?; // None early if `inner` is missing
    Some(value.as_str())
}

fn main() {
    let mut data = HashMap::new();
    let mut user = HashMap::new();
    user.insert("email".to_string(), "ada@example.com".to_string());
    data.insert("ada".to_string(), user);

    println!("{:?}", nested_lookup(&data, "ada", "email")); // Some("ada@example.com")
    println!("{:?}", nested_lookup(&data, "ada", "phone")); // None
    println!("{:?}", nested_lookup(&data, "bob", "email")); // None
}
```

Verified output:

```text
Some("ada@example.com")
None
None
```

</details>

### Exercise 3: Custom error type with `From`

**Difficulty:** Hard

**Objective:** Make `?` perform an automatic error conversion by implementing `From`, and combine it with an early-return for a second error variant.

**Instructions:** Define a `CartError` enum with two variants: `Empty` (the cart has no items) and `BadQuantity(ParseIntError)`. Implement `Display` and `From<ParseIntError> for CartError`. Then write `total_items(quantities: &[&str]) -> Result<u32, CartError>` that returns `CartError::Empty` if the slice is empty, otherwise parses and sums the quantities using `?` (relying on your `From` impl). Test with valid input, an unparseable entry, and an empty slice.

```rust
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
enum CartError {
    Empty,
    BadQuantity(ParseIntError),
}

// TODO: impl fmt::Display for CartError
// TODO: impl From<ParseIntError> for CartError

fn total_items(quantities: &[&str]) -> Result<u32, CartError> {
    // TODO: return CartError::Empty if empty; otherwise sum with `?`
    /* ??? */
}

fn main() {
    println!("{:?}", total_items(&["2", "3", "1"]));
    // ... also test &["2", "oops"] and &[]
}
```

<details>
<summary>Solution</summary>

```rust
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
enum CartError {
    Empty,
    BadQuantity(ParseIntError),
}

impl fmt::Display for CartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CartError::Empty => write!(f, "cart is empty"),
            CartError::BadQuantity(e) => write!(f, "bad quantity: {e}"),
        }
    }
}

// This is what lets `?` turn a ParseIntError into a CartError automatically.
impl From<ParseIntError> for CartError {
    fn from(e: ParseIntError) -> Self {
        CartError::BadQuantity(e)
    }
}

fn total_items(quantities: &[&str]) -> Result<u32, CartError> {
    if quantities.is_empty() {
        return Err(CartError::Empty); // explicit early return for the non-parse failure
    }
    let mut total = 0;
    for q in quantities {
        total += q.parse::<u32>()?; // ParseIntError -> CartError via `From`
    }
    Ok(total)
}

fn main() {
    println!("{:?}", total_items(&["2", "3", "1"]));
    match total_items(&["2", "oops"]) {
        Ok(n) => println!("total = {n}"),
        Err(e) => println!("error: {e}"),
    }
    match total_items(&[]) {
        Ok(n) => println!("total = {n}"),
        Err(e) => println!("error: {e}"),
    }
}
```

Verified output:

```text
Ok(6)
error: bad quantity: invalid digit found in string
error: cart is empty
```

> **Tip:** Writing `impl From<...>` by hand is fine for one or two variants, but real libraries derive it with `thiserror`'s `#[from]`. See [anyhow & thiserror](/08-error-handling/06-anyhow-thiserror/).

</details>
