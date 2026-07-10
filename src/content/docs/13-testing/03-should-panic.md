---
title: "Testing Panics and Returning `Result` from Tests"
description: "Rust's #[should_panic(expected=...)] asserts a panic like Jest's toThrow, while Result-returning tests let ? propagate errors."
---

Not every test is a simple "given input, assert output." Sometimes the *correct* behavior is to fail loudly, and sometimes the test body itself is full of fallible steps. Rust gives you two dedicated tools for these cases: the `#[should_panic]` attribute and tests that return `Result<(), E>` so you can use the `?` operator.

---

## Quick Overview

In Jest or Vitest you assert that code throws with `expect(fn).toThrow(...)`, and you write fallible test bodies as `async` functions that `await` and let rejections bubble up. Rust splits these into two distinct mechanisms: a test marked **`#[should_panic(expected = "...")]`** passes only if its body **panics** with a message containing the expected substring, and a test whose signature is **`-> Result<(), E>`** lets you use the **`?` operator** so any `Err` cleanly fails the test instead of forcing a sea of `.unwrap()` calls. Knowing which to reach for is the difference between a test that documents a real invariant and one that quietly passes for the wrong reason.

---

## TypeScript/JavaScript Example

In a Jest/Vitest suite, asserting that something throws and writing a fallible test body look like this:

```typescript
// banking.ts
export function withdraw(balance: number, amount: number): number {
  if (amount > balance) {
    throw new Error(
      `insufficient funds: balance is ${balance}, tried to withdraw ${amount}`,
    );
  }
  return balance - amount;
}

// banking.test.ts
import { describe, it, expect } from "vitest";
import { withdraw } from "./banking";

describe("withdraw", () => {
  // Asserting a throw. The matcher wraps the call in a try/catch for you.
  it("rejects overdrafts", () => {
    expect(() => withdraw(100, 150)).toThrow("insufficient funds");
  });

  it("succeeds within balance", () => {
    expect(withdraw(100, 30)).toBe(70);
  });

  // A fallible test body: the function is `async`, and any rejected promise
  // (or thrown error) automatically fails the test.
  it("loads and parses config", async () => {
    const raw = await readFile("config.json", "utf8"); // may reject
    const config = JSON.parse(raw); // may throw
    expect(config.port).toBe(8080);
  });
});
```

Two things are happening here. `expect(() => ...).toThrow(...)` passes the *function* (not its result) so the matcher can catch the throw itself, and it matches the error message as a substring. Separately, the `async` test body lets a rejected `await` or a thrown `JSON.parse` fail the test without any explicit error plumbing; the test runner treats a rejected promise as a failure.

---

## Rust Equivalent

Rust expresses the first case with the `#[should_panic]` attribute and the second with a `Result`-returning test signature:

```rust
use std::num::ParseIntError;

/// Withdraw `amount` from `balance`, panicking if it would overdraw.
pub fn withdraw(balance: u32, amount: u32) -> u32 {
    if amount > balance {
        panic!("insufficient funds: balance is {balance}, tried to withdraw {amount}");
    }
    balance - amount
}

#[derive(Debug, PartialEq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Parse a `#rrggbb` hex color string.
pub fn parse_hex_color(s: &str) -> Result<Rgb, ParseIntError> {
    let s = s.trim_start_matches('#');
    let r = u8::from_str_radix(&s[0..2], 16)?;
    let g = u8::from_str_radix(&s[2..4], 16)?;
    let b = u8::from_str_radix(&s[4..6], 16)?;
    Ok(Rgb { r, g, b })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Asserting a panic, matching part of the message.
    #[test]
    #[should_panic(expected = "insufficient funds")]
    fn overdraw_panics() {
        withdraw(100, 150);
    }

    #[test]
    fn withdraw_succeeds() {
        assert_eq!(withdraw(100, 30), 70);
    }

    // A fallible test body: `?` propagates any `Err`, failing the test.
    #[test]
    fn parses_white() -> Result<(), ParseIntError> {
        let color = parse_hex_color("#ffffff")?;
        assert_eq!(color, Rgb { r: 255, g: 255, b: 255 });
        Ok(())
    }
}
```

Running `cargo test` produces:

```text
running 3 tests
test tests::parses_white ... ok
test tests::withdraw_succeeds ... ok
test tests::overdraw_panics - should panic ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The `- should panic` annotation in the runner output tells you the test was expected to panic and did. The `?` in `parses_white` is the direct analog of `await` in an async Jest test: if `parse_hex_color` returns an `Err`, the `?` returns it from the test function, and the runner marks the test failed.

---

## Detailed Explanation

### `#[should_panic]` inverts the pass/fail condition

A normal `#[test]` passes when its body runs to completion without panicking. Adding `#[should_panic]` **inverts** that contract: the test passes *only if* the body panics, and **fails if it returns normally**. The attribute is stacked below `#[test]`:

```rust
#[test]
#[should_panic]
fn overdraw_panics() {
    withdraw(100, 150); // must panic, or the test fails
}
```

This is the analog of Jest's `expect(fn).toThrow()` with no argument — "I don't care about the message, just that it throws."

### `expected` matches a substring, not the whole message

The bare `#[should_panic]` is blunt: it passes if the body panics *for any reason at all*, which is dangerous (see Common Pitfalls). Add `expected = "..."` to require that the panic message **contains** that substring:

```rust
#[test]
#[should_panic(expected = "insufficient funds")]
fn overdraw_panics() {
    withdraw(100, 150);
}
```

The check is a substring match (via `str::contains`), not an exact-equality or regex match, exactly like Jest's `toThrow("...")` with a string argument. If the actual panic message does not contain the expected substring, the test fails and the runner prints both strings so you can see the mismatch. With our `withdraw` panicking on a `"account frozen"` expectation, the failure looks like this:

```text
thread 'tests::wrong_expected_substring' panicked at src/lib.rs:3:9:
insufficient funds: balance is 100, tried to withdraw 150
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
note: panic did not contain expected string
      panic message: "insufficient funds: balance is 100, tried to withdraw 150"
 expected substring: "account frozen"
```

> **Tip:** Always prefer `#[should_panic(expected = "...")]` over the bare form. The expected string nails down *why* the code panicked, so the test cannot pass because of an unrelated panic (a typo'd array index, a different `unwrap` failing, and so on).

### `#[should_panic]` catches any panic, including `assert!` and overflow

The body just needs to panic. It does not matter whether the panic came from an explicit `panic!`, from a failed `assert!`/`assert_eq!`, from an `.unwrap()` on `None`, or from an arithmetic overflow in a debug build. For example, a panic raised by `assert!` is caught the same way:

```rust
pub fn checked_div(a: i32, b: i32) -> i32 {
    assert!(b != 0, "division by zero is undefined");
    a / b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "division by zero")]
    fn divide_by_zero_panics() {
        checked_div(10, 0);
    }
}
```

```text
running 1 test
test tests::divide_by_zero_panics - should panic ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Tests can return `Result<(), E>` to use `?`

A test function may return `Result<(), E>` for any error type `E` that implements `Debug`. The runner treats `Ok(())` as a pass and any `Err(e)` as a failure, printing the error with its `Debug` representation. This lets you use the `?` operator inside the test body:

```rust
#[test]
fn parses_white() -> Result<(), ParseIntError> {
    let color = parse_hex_color("#ffffff")?; // `?` returns Err on failure
    assert_eq!(color, Rgb { r: 255, g: 255, b: 255 });
    Ok(()) // explicit success value is required
}
```

Without this feature you would have to write `parse_hex_color("#ffffff").unwrap()` on every fallible line. The `Result` return type lets you write the same straight-line "happy path" code you would write in the library itself. See [The `?` Operator](/08-error-handling/01-question-mark/) for the full mechanics of `?`. The error type can be anything `Debug`: a concrete error like `ParseIntError`, a custom enum, or the catch-all `Box<dyn std::error::Error>` when several different errors flow through one test.

When such a test does fail, the `Debug` of the returned error is shown under the test's name. A test that propagates a `ParseIntError` from invalid hex prints:

```text
running 1 test
test tests::rejects_garbage ... FAILED

failures:

---- tests::rejects_garbage stdout ----
Error: ParseIntError { kind: InvalidDigit }


failures:
    tests::rejects_garbage

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

> **Note:** `Result`-returning tests cannot be combined with `#[should_panic]`. The two model opposite outcomes: `#[should_panic]` says "this must panic," while a `Result` test says "this must finish and return `Ok`." A test that needs to assert an `Err` value should return `Result` *and* check the error explicitly (shown below), not use `#[should_panic]`.

---

## Key Differences

| Concern | TypeScript (Jest/Vitest) | Rust |
| --- | --- | --- |
| Assert that code throws/panics | `expect(() => fn()).toThrow("msg")` | `#[should_panic(expected = "msg")]` on the test |
| Pass a callback vs. run inline | Must wrap in `() => ...` so the matcher catches the throw | Body runs inline; the attribute inverts pass/fail |
| Message matching | Substring (string arg) or regex (`/.../ `) | Substring only (`str::contains`); no regex |
| Fallible test body | `async` test; rejected `await`/throw fails it | `-> Result<(), E>` lets `?` propagate `Err` |
| Explicit success value | Implicit (test returns `undefined`) | Must end with `Ok(())` |
| Assert on the error *value* | `try { ... } catch (e) { expect(e).toEqual(...) }` | Return `Result` and check `.unwrap_err()`, or `match` |
| What "panic" / "throw" means | Any thrown value unwinds the stack | A `panic!` is an *unrecoverable* bug signal, not control flow |

The single most important conceptual difference: in TypeScript, `throw` is an ordinary, expected control-flow mechanism — you `throw` to signal validation failures and `catch` to recover. In Rust, a **panic** signals an *unrecoverable bug* and is **not** how you report expected failures; recoverable failures return [`Result`](/08-error-handling/00-result-option/). So `#[should_panic]` is for testing genuine "this should be impossible" invariants (a precondition violation, an index out of bounds), whereas testing an *expected* failure (bad user input, a missing file) means asserting on a returned `Err`, not on a panic. See [Panicking](/08-error-handling/02-panic/) for the panic-vs-`Result` decision.

---

## Common Pitfalls

### Pitfall 1: bare `#[should_panic]` passing for the wrong reason

A test marked with the bare attribute passes if the body panics *for any reason*. If you later introduce a bug — say, an out-of-bounds index *before* the line you meant to test — the test still passes, hiding the regression.

```rust
#[test]
#[should_panic] // passes if ANY panic occurs, even an unrelated one
fn overdraw_panics() {
    let accounts = vec![100u32];
    let _ = accounts[5]; // this panics first — test "passes" for the wrong reason!
    withdraw(accounts[0], 150);
}
```

**Fix:** always pin the message with `expected`, so an unrelated panic (here, the slice-index panic `index out of bounds`) fails the substring check instead of silently satisfying it.

### Pitfall 2: a `#[should_panic]` test that does not panic

If the body completes normally, the test fails. This is the failure mode you *want* (it tells you the panic you expected never happened), but the message surprises newcomers:

```rust
#[test]
#[should_panic]
fn does_not_actually_panic() {
    let _ = 2 + 2; // no panic — so the test fails
}
```

```text
running 1 test
test tests::does_not_actually_panic - should panic ... FAILED

failures:

---- tests::does_not_actually_panic stdout ----
note: test did not panic as expected at src/lib.rs:9:8

failures:
    tests::does_not_actually_panic

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Pitfall 3: using `?` in a test that does not return `Result`

The `?` operator only works in a function whose return type can carry the error. A default test returns `()`, so `?` will not compile:

```rust
#[test]
fn parses_ff() {                       // returns () — no place for `?` to send the Err
    let value = parse_hex_byte("ff")?; // does not compile (E0277)
    assert_eq!(value, 255);
}
```

The real compiler error:

```text
error[E0277]: the `?` operator can only be used in a function that returns `Result` or `Option` (or another type that implements `FromResidual`)
  --> src/lib.rs:14:41
   |
13 |     fn parses_ff() {
   |     -------------- this function should return `Result` or `Option` to accept `?`
14 |         let value = parse_hex_byte("ff")?;
   |                                         ^ cannot use the `?` operator in a function that returns `()`
   |
help: consider adding return type
   |
13 ~     fn parses_ff() -> Result<(), Box<dyn std::error::Error>> {
14 |         let value = parse_hex_byte("ff")?;
15 |         assert_eq!(value, 255);
16 ~         Ok(())
17 ~     }
```

**Fix:** add the return type the compiler suggests (`-> Result<(), Box<dyn std::error::Error>>` or a concrete error type) and end the body with `Ok(())`.

### Pitfall 4: trying to assert an `Err` *value* with `#[should_panic]`

A returned `Err` is not a panic, so `#[should_panic]` will never trigger on it; the test would fail with "did not panic." To assert that a function returns a specific error, return `Result` from the test and inspect the error directly:

```rust
#[test]
fn missing_key_is_reported() {
    // `unwrap_err` extracts the Err; it panics only if the call unexpectedly succeeds.
    let err = parse_hex_color("nothex").unwrap_err();
    assert_eq!(err.to_string(), "invalid digit found in string");
}
```

This is the idiomatic way to test the "expected failure" cases that you would write as `expect(...).toThrow(...)` in Jest but which in Rust are *recoverable* `Result` errors, not panics.

### Pitfall 5: forgetting `Ok(())` at the end

A `Result`-returning test must yield a value on the success path. Ending the body with the last `assert!` is not enough, because `assert!` evaluates to `()`, not `Result`. Add an explicit `Ok(())` as the final expression.

---

## Best Practices

- **Always use `expected = "..."`** with `#[should_panic]`. The bare form is a foot-gun that passes on any panic. Pick a substring stable enough to survive small wording changes but specific enough to pin the cause.
- **Reserve `#[should_panic]` for genuine invariants** — preconditions, "unreachable" branches, index bounds — i.e. the things that *should* panic in production because they represent a bug. Do not use it to test ordinary input validation; that belongs in a `Result` return and an `assert_eq!` on the error.
- **Return `Result<(), E>` whenever a test body has more than one fallible step**, so `?` keeps the happy path readable. Use a concrete error type (`ParseIntError`, your domain enum) when one error flows through, and `Box<dyn std::error::Error>` when several different errors do.
- **Assert error *values* by returning `Result` and inspecting `.unwrap_err()`** (or `match`/`matches!`), not by catching a panic.
- **Keep `Ok(())` at the very end** of a `Result` test; treat it as the test's "I reached the end successfully" marker.
- **A `Result` test that itself fails on the success path can still use `.unwrap()` for the *negative* check**: e.g. `unwrap_err()` is fine because a precondition guarantees it. Reserve unconditional `unwrap()` for tests; in library code, prefer `?` (see [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/)).

---

## Real-World Example

A small configuration store that reads typed settings. The error type uses [`thiserror`](/08-error-handling/06-anyhow-thiserror/) (add it with `cargo add thiserror`; this pulls in `thiserror = "2"`). The test module mixes all three styles: a `Result`-returning happy-path test that uses `?`, a negative test that asserts on the returned `Err` value, and a test that exercises the `Display` impl.

```rust
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur while reading a typed setting from the config.
#[derive(Debug, Error, PartialEq)]
pub enum ConfigError {
    #[error("missing required key: {0}")]
    Missing(String),
    #[error("key `{key}` is not a valid integer: {value:?}")]
    NotAnInt { key: String, value: String },
}

/// A tiny string-keyed configuration store.
pub struct Config {
    values: HashMap<String, String>,
}

impl Config {
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        let values = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config { values }
    }

    /// Read a required `u16` setting, failing if it is missing or malformed.
    pub fn require_u16(&self, key: &str) -> Result<u16, ConfigError> {
        let raw = self
            .values
            .get(key)
            .ok_or_else(|| ConfigError::Missing(key.to_string()))?;
        raw.parse::<u16>().map_err(|_| ConfigError::NotAnInt {
            key: key.to_string(),
            value: raw.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A Result-returning test: every `?` either gives us the value or fails
    // the test by returning the Err. No `.unwrap()` noise on the happy path.
    #[test]
    fn reads_valid_port() -> Result<(), ConfigError> {
        let config = Config::from_pairs(&[("port", "8080"), ("host", "localhost")]);
        let port = config.require_u16("port")?;
        assert_eq!(port, 8080);
        Ok(())
    }

    // For the *failure* paths we assert on the returned Err directly,
    // rather than using `?` (which would abort the test).
    #[test]
    fn missing_key_is_reported() {
        let config = Config::from_pairs(&[("host", "localhost")]);
        let err = config.require_u16("port").unwrap_err();
        assert_eq!(err, ConfigError::Missing("port".to_string()));
    }

    #[test]
    fn malformed_int_is_reported() -> Result<(), Box<dyn std::error::Error>> {
        let config = Config::from_pairs(&[("port", "not-a-number")]);
        let err = config.require_u16("port").unwrap_err();
        // `to_string()` exercises the `#[error(...)]` Display impl.
        assert!(err.to_string().contains("not a valid integer"));
        Ok(())
    }
}
```

Running `cargo test` for this module:

```text
running 3 tests
test tests::malformed_int_is_reported ... ok
test tests::missing_key_is_reported ... ok
test tests::reads_valid_port ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Notice the division of labor: the success path uses `?` for clean propagation, the failure paths use `unwrap_err()` and assert on the `ConfigError` value, and none of them uses `#[should_panic]`, because *none of these are bugs*, they are recoverable errors. That is the idiomatic Rust split.

---

## Further Reading

- [The Rust Programming Language — How to Write Tests (Checking for Panics with `should_panic`)](https://doc.rust-lang.org/book/ch11-01-writing-tests.html#checking-for-panics-with-should_panic)
- [The Rust Programming Language — Using `Result<T, E>` in Tests](https://doc.rust-lang.org/book/ch11-01-writing-tests.html#using-resultt-e-in-tests)
- [Rust By Example — Testing](https://doc.rust-lang.org/rust-by-example/testing.html)
- Related sections in this guide:
  - [Unit Tests](/13-testing/00-unit-tests/): the `#[test]` and `#[cfg(test)] mod tests` basics these tests build on
  - [Assertions](/13-testing/02-assertions/): `assert!`/`assert_eq!`/`assert_ne!` and custom messages, which trigger the panics `#[should_panic]` catches
  - [Integration Tests](/13-testing/04-integration-tests/): `Result`-returning tests also work in the `tests/` directory
  - [Doc Tests](/13-testing/09-doc-tests/): doc tests support a `should_panic` attribute and can also return `Result`
  - [Test Organization](/13-testing/01-test-organization/): where these tests live and the `tests` submodule convention
  - [The `?` Operator](/08-error-handling/01-question-mark/): the mechanics of `?` that `Result`-returning tests rely on
  - [Panicking](/08-error-handling/02-panic/) — when a panic is the right signal versus returning a `Result`
  - [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/) — acceptable in tests, including the `unwrap_err()` pattern above
  - [Section 14: Macros](/14-macros/): `#[test]`, `#[should_panic]`, and `assert!` are all built on Rust's attribute and macro system

---

## Exercises

### Exercise 1: assert a precondition panic

**Difficulty:** Easy

**Objective:** Use `#[should_panic(expected = "...")]` to verify that a method panics when called on an invalid state.

**Instructions:**

1. Given the `Stack<T>` below, whose `pop` panics on an empty stack, write a test that confirms calling `pop` on a fresh stack panics with a message containing `"empty stack"`.
2. Add a second, ordinary test that pushes two values and asserts `pop` returns them in last-in-first-out order.

```rust
pub struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    pub fn new() -> Self {
        Stack { items: Vec::new() }
    }
    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }
    /// Remove and return the top item, panicking if the stack is empty.
    pub fn pop(&mut self) -> T {
        self.items.pop().expect("pop called on an empty stack")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // TODO: write the two tests
}
```

<details>
<summary>Solution</summary>

```rust
pub struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    pub fn new() -> Self {
        Stack { items: Vec::new() }
    }
    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }
    pub fn pop(&mut self) -> T {
        self.items.pop().expect("pop called on an empty stack")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "empty stack")]
    fn pop_on_empty_panics() {
        let mut stack: Stack<i32> = Stack::new();
        stack.pop();
    }

    #[test]
    fn push_then_pop() {
        let mut stack = Stack::new();
        stack.push(1);
        stack.push(2);
        assert_eq!(stack.pop(), 2);
        assert_eq!(stack.pop(), 1);
    }
}
```

The `expected` substring matches the message inside `.expect(...)`. Running `cargo test` reports `pop_on_empty_panics - should panic ... ok` and `push_then_pop ... ok`.

</details>

### Exercise 2: convert an `unwrap`-heavy test to a `Result` test

**Difficulty:** Medium

**Objective:** Replace `.unwrap()` calls in a test body with the `?` operator by giving the test a `Result` return type.

**Instructions:**

1. The parser below turns a `"x,y"` row into a `Point`. Write a test `parses_a_point` that parses `"3, 4"` and asserts it equals `Point { x: 3, y: 4 }`.
2. Use `?` rather than `.unwrap()`, which means the test must return `Result<(), ParseError>` and end with `Ok(())`.

```rust
use std::num::ParseIntError;

#[derive(Debug, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    WrongFieldCount(usize),
    BadInt(ParseIntError),
}

impl From<ParseIntError> for ParseError {
    fn from(e: ParseIntError) -> Self {
        ParseError::BadInt(e)
    }
}

pub fn parse_point(row: &str) -> Result<Point, ParseError> {
    let fields: Vec<&str> = row.split(',').collect();
    if fields.len() != 2 {
        return Err(ParseError::WrongFieldCount(fields.len()));
    }
    let x = fields[0].trim().parse::<i32>()?;
    let y = fields[1].trim().parse::<i32>()?;
    Ok(Point { x, y })
}

#[cfg(test)]
mod tests {
    use super::*;
    // TODO: write `parses_a_point` using `?`
}
```

<details>
<summary>Solution</summary>

```rust
use std::num::ParseIntError;

#[derive(Debug, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    WrongFieldCount(usize),
    BadInt(ParseIntError),
}

impl From<ParseIntError> for ParseError {
    fn from(e: ParseIntError) -> Self {
        ParseError::BadInt(e)
    }
}

pub fn parse_point(row: &str) -> Result<Point, ParseError> {
    let fields: Vec<&str> = row.split(',').collect();
    if fields.len() != 2 {
        return Err(ParseError::WrongFieldCount(fields.len()));
    }
    let x = fields[0].trim().parse::<i32>()?;
    let y = fields[1].trim().parse::<i32>()?;
    Ok(Point { x, y })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_point() -> Result<(), ParseError> {
        let point = parse_point("3, 4")?;
        assert_eq!(point, Point { x: 3, y: 4 });
        Ok(())
    }
}
```

Because the test returns `Result<(), ParseError>`, the `?` on `parse_point` propagates any `Err` as a test failure, and the happy path stays free of `.unwrap()`. The test reports `parses_a_point ... ok`.

</details>

### Exercise 3: test both the success and the two failure modes

**Difficulty:** Medium-Hard

**Objective:** Cover one function with a `Result` test for success and `Err`-value assertions for each failure path — *without* `#[should_panic]`, because these are recoverable errors, not bugs.

**Instructions:**

Using the same `parse_point` from Exercise 2, write three tests:

1. `wrong_field_count_is_an_error`: parsing `"1,2,3"` returns `Err(ParseError::WrongFieldCount(3))`.
2. `bad_int_is_an_error`: parsing a valid `"10,-7"` succeeds via `?`, and parsing `"x,2"` returns a `ParseError::BadInt(_)`.
3. Decide for each whether the test should return `Result` and explain (in a comment) why you did *not* use `#[should_panic]`.

<details>
<summary>Solution</summary>

```rust
use std::num::ParseIntError;

#[derive(Debug, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    WrongFieldCount(usize),
    BadInt(ParseIntError),
}

impl From<ParseIntError> for ParseError {
    fn from(e: ParseIntError) -> Self {
        ParseError::BadInt(e)
    }
}

pub fn parse_point(row: &str) -> Result<Point, ParseError> {
    let fields: Vec<&str> = row.split(',').collect();
    if fields.len() != 2 {
        return Err(ParseError::WrongFieldCount(fields.len()));
    }
    let x = fields[0].trim().parse::<i32>()?;
    let y = fields[1].trim().parse::<i32>()?;
    Ok(Point { x, y })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A wrong field count is a recoverable, *expected* failure — not a bug —
    // so we assert on the returned Err value rather than expecting a panic.
    #[test]
    fn wrong_field_count_is_an_error() {
        let err = parse_point("1,2,3").unwrap_err();
        assert_eq!(err, ParseError::WrongFieldCount(3));
    }

    // Same reasoning: a malformed integer returns Err, it does not panic.
    // This test returns Result so the *valid* row can use `?` on the happy path.
    #[test]
    fn bad_int_is_an_error() -> Result<(), ParseError> {
        let ok = parse_point("10,-7")?;
        assert_eq!(ok, Point { x: 10, y: -7 });
        assert!(matches!(parse_point("x,2"), Err(ParseError::BadInt(_))));
        Ok(())
    }
}
```

Both failures are returned as `Err` values, so `#[should_panic]` would be wrong here — it would only catch a *panic*, and `parse_point` never panics. `matches!` is a concise way to assert the *variant* of an error without comparing the inner `ParseIntError`, which does not have a convenient literal. The suite reports `wrong_field_count_is_an_error ... ok` and `bad_int_is_an_error ... ok`.

</details>
