---
title: "Unit Tests"
description: "Unlike Jest or Vitest, Rust unit tests are #[test] functions in a #[cfg(test)] mod beside the code, run by cargo test with no config, and can reach private items."
---

Coming from Jest or Vitest, you are used to a separate test runner, a config file, and `*.test.ts` files living somewhere in your project. Rust folds all of that into the language and the build tool: tests are ordinary functions marked with `#[test]`, the test runner ships with `cargo`, and there is nothing to install or configure.

---

## Quick Overview

A **unit test** in Rust is a normal function annotated with the `#[test]` attribute, almost always grouped inside a `#[cfg(test)] mod tests` module that sits in the *same file* as the code it exercises. You run the whole suite with `cargo test`: no `package.json` script, no `jest.config.js`, no extra dependency. Because the tests live next to the code, they can reach **private** functions, which is the single biggest difference from the black-box habits most JavaScript test suites fall into.

---

## TypeScript/JavaScript Example

Here is a small pricing module and a Vitest suite for it, the kind of thing you would write every day.

```typescript
// cart.ts
export function cartTotalCents(
  items: { unitPriceCents: number; quantity: number }[],
  discountPercent: number,
): number {
  const subtotal = items.reduce(
    (acc, it) => acc + it.unitPriceCents * it.quantity,
    0,
  );
  const percent = Math.min(discountPercent, 100);
  // Integer-cents math: truncate the discount, never carry fractional cents.
  return subtotal - Math.trunc((subtotal * percent) / 100);
}
```

```typescript
// cart.test.ts
import { describe, it, expect } from "vitest";
import { cartTotalCents } from "./cart";

const cart = [
  { unitPriceCents: 4999, quantity: 1 },
  { unitPriceCents: 999, quantity: 3 },
];

describe("cartTotalCents", () => {
  it("sums all lines with no discount", () => {
    expect(cartTotalCents(cart, 0)).toBe(7996);
  });

  it("applies a whole-cart discount", () => {
    expect(cartTotalCents(cart, 10)).toBe(7197);
  });

  it("clamps a discount over 100%", () => {
    expect(cartTotalCents(cart, 250)).toBe(0);
  });
});
```

Run it with `npx vitest run`:

```text
 RUN  v4.1.7

 Test Files  1 passed (1)
      Tests  3 passed (3)
```

Key things a JavaScript developer takes for granted here:

- Tests live in a **separate file** (`cart.test.ts`) and import the module's **public** exports.
- A **test runner** (Vitest/Jest) is a dependency you add and configure.
- `describe`/`it`/`expect` are functions that the runner provides.

Every one of those assumptions changes in Rust.

---

## Rust Equivalent

The same module and its tests, in one file. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically, so nothing below needs an explicit edition.

```rust
// src/lib.rs

/// Computes the cart total in cents after applying a whole-cart
/// percentage discount (clamped to 0..=100).
pub fn cart_total_cents(items: &[(u64, u32)], discount_percent: u8) -> u64 {
    // Each tuple is (unit_price_cents, quantity).
    let subtotal: u64 = items.iter().map(|(price, qty)| price * *qty as u64).sum();
    let percent = discount_percent.min(100) as u64;
    subtotal - (subtotal * percent) / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cart() -> Vec<(u64, u32)> {
        vec![(4_999, 1), (999, 3)]
    }

    #[test]
    fn sums_all_lines_with_no_discount() {
        assert_eq!(cart_total_cents(&sample_cart(), 0), 7_996);
    }

    #[test]
    fn applies_a_whole_cart_discount() {
        assert_eq!(cart_total_cents(&sample_cart(), 10), 7_197);
    }

    #[test]
    fn clamps_a_discount_over_100_percent() {
        assert_eq!(cart_total_cents(&sample_cart(), 250), 0);
    }
}
```

Run it with `cargo test`:

```text
running 3 tests
test tests::applies_a_whole_cart_discount ... ok
test tests::clamps_a_discount_over_100_percent ... ok
test tests::sums_all_lines_with_no_discount ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

> **Note:** Rust's integer division truncates *the division*, so `(7996 * 10) / 100` is `799`, giving `7996 - 799 = 7197`. The TypeScript version above uses `Math.trunc` on the inner division to match. If you wrote the JavaScript without that `Math.trunc`, the inner division yields `799.6` and the final result would be `7196.4` (a non-integer), a genuinely different answer. This is a recurring theme: when porting numeric code, the *order* of truncation matters.

---

## Detailed Explanation

Let's walk through the Rust test block line by line and contrast each piece with the Vitest version.

### `#[cfg(test)]` — conditional compilation

```rust
#[cfg(test)]
mod tests {
    // ...
}
```

`#[cfg(test)]` is an **attribute** that tells the compiler: only compile this module when building in *test mode* (`cargo test`). During a normal `cargo build` or `cargo build --release`, the entire `tests` module (and anything it imports) is stripped out. There is **zero cost** to your production binary.

This is the conceptual equivalent of Jest/Vitest only loading `*.test.ts` files when the runner is invoked, except Rust does it at compile time rather than by file-name convention. There is no separate test file to forget to exclude from your bundle.

> **Note:** The module is named `tests` purely by convention. The name is not magic; `#[cfg(test)] mod whatever { ... }` works identically. The community standard is `tests`, and following it makes your code instantly readable to other Rustaceans.

### `mod tests` — a child module

```rust
mod tests {
    use super::*;
    // ...
}
```

`mod tests` declares a child module. Because it is a *child* of the module that defines `cart_total_cents`, it can see that module's **private** items too. `use super::*;` pulls everything from the parent module (`super`) into scope so you can call `cart_total_cents` directly instead of writing `super::cart_total_cents` every time.

This is the superpower JavaScript test suites lack. In Vitest you can only test what a module `export`s; to test an unexported helper you must either export it (polluting the public surface) or reach in with a tool like `rewire`. In Rust, a `#[cfg(test)] mod tests` inside the file sees the private helpers for free. (More on where to put tests in [Test Organization](/13-testing/01-test-organization/).)

### `#[test]` — marking a test function

```rust
#[test]
fn sums_all_lines_with_no_discount() {
    assert_eq!(cart_total_cents(&sample_cart(), 0), 7_996);
}
```

`#[test]` marks a function as a test. The function must take **no arguments** and (in the simplest case) return **no value** — it is just a `fn name() { ... }`. There is no `it("...", () => {})` wrapper: the test's *name* is the function's name, and the test "passes" if the function returns normally and "fails" if it **panics**.

That is the core mental model:

| Vitest/Jest                          | Rust                                  |
| ------------------------------------ | ------------------------------------- |
| `it("name", () => { ... })`          | `#[test] fn name() { ... }`           |
| test fails if an `expect` throws     | test fails if the function **panics** |
| `describe("group", ...)` for nesting | a `mod` for nesting                   |

### `assert_eq!` — the assertion

```rust
assert_eq!(cart_total_cents(&sample_cart(), 0), 7_996);
```

`assert_eq!(left, right)` panics if the two values are not equal, which is exactly what makes a `#[test]` fail. It is the rough analogue of `expect(actual).toBe(expected)`. Rust ships a small family (`assert!`, `assert_eq!`, `assert_ne!`) covered in detail in [Assertions](/13-testing/02-assertions/). For now, the important point is that an assertion failure is just a panic with a nicely formatted message.

### `sample_cart()` — a plain helper

```rust
fn sample_cart() -> Vec<(u64, u32)> {
    vec![(4_999, 1), (999, 3)]
}
```

There is no `beforeEach`. To share setup between tests you write an ordinary function and call it. This keeps data flow explicit — each test constructs exactly what it needs. Richer setup/teardown patterns (and the rare cases where you *do* want shared state) live in [Test Fixtures](/13-testing/05-test-fixtures/).

---

## Key Differences

| Concept              | Jest / Vitest                                    | Rust                                                       |
| -------------------- | ------------------------------------------------ | ---------------------------------------------------------- |
| Test runner          | Separate dependency (`jest`, `vitest`)           | Built into `cargo`; nothing to install                     |
| Config               | `jest.config.js` / `vitest.config.ts`            | None required                                              |
| Where tests live     | Usually a separate `*.test.ts` file              | Same file, in a `#[cfg(test)] mod tests`                   |
| Declaring a test     | `it("...", () => {})`                            | `#[test] fn ...() {}`                                      |
| Grouping             | `describe(...)`                                   | nested `mod`                                               |
| Pass / fail signal   | assertion throws an exception                    | function **panics**                                        |
| Setup                | `beforeEach` / `beforeAll`                        | a helper function you call                                 |
| Access to internals  | only `export`ed items (without hacks)            | private items visible to the in-file test module          |
| Excluded from builds | by file-name convention / bundler config         | by `#[cfg(test)]` at compile time                          |
| Parallelism          | configurable, file-level by default              | tests run **in parallel threads** by default               |

Two differences deserve emphasis:

**Tests run in parallel by default.** Vitest parallelizes across files; Rust runs *individual test functions* on multiple threads at once. That is great for speed but means tests must not depend on shared mutable global state or on each other's ordering. If you need serial execution, run `cargo test -- --test-threads=1`.

**A test passes by *not panicking*.** There is no "expected 0 assertions" concept. A `#[test] fn` with an empty body passes. A test that calls `.unwrap()` on a `None` fails because `.unwrap()` panics. Once you internalize "failure == panic," the rest of Rust testing clicks into place.

---

## Common Pitfalls

### Pitfall 1: Giving a `#[test]` function parameters

JavaScript test callbacks sometimes take a `done` argument or a fixture. A Rust test takes none.

```rust
pub fn double(n: i32) -> i32 {
    n * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles(n: i32) {
        // does not compile: functions used as tests can not have any arguments
        assert_eq!(double(n), n * 2);
    }
}
```

Real output from `cargo test`:

```text
error: functions used as tests can not have any arguments
  --> src/lib.rs:10:5
   |
10 | /     fn doubles(n: i32) {
11 | |         assert_eq!(double(n), n * 2);
12 | |     }
   | |_____^
```

The fix is to pick the inputs inside the test body. If you genuinely want to test many inputs, that is what [property testing](/13-testing/07-property-testing/) (proptest) is for.

### Pitfall 2: Trying to reach a private item across module boundaries

The in-file test module can see private items of *its parent*, but not private items of an *unrelated* module.

```rust
mod math {
    // private (no `pub`)
    fn double(n: i32) -> i32 {
        n * 2
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn doubles() {
        // does not compile (error[E0603]): `double` is private to `math`
        assert_eq!(crate::math::double(21), 42);
    }
}
```

Real error:

```text
error[E0603]: function `double` is private
  --> src/lib.rs:13:33
   |
13 |         assert_eq!(crate::math::double(21), 42);
   |                                 ^^^^^^ private function
   |
note: the function `double` is defined here
  --> src/lib.rs:3:5
   |
 3 |     fn double(n: i32) -> i32 {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^
```

The fix is to put the test module *inside* `mod math` (so it becomes a child and inherits visibility), or make `double` public if it is genuinely part of the API. This visibility-vs-location trade-off is the heart of [Test Organization](/13-testing/01-test-organization/).

### Pitfall 3: Forgetting `#[cfg(test)]`

If you write `mod tests` without the `#[cfg(test)]` attribute, the test code is compiled into your normal build. At best you get dead-code warnings; at worst, a test-only dependency leaks into your release binary. Always pair the two:

```rust
#[cfg(test)]   // <- do not omit this
mod tests {
    // ...
}
```

### Pitfall 4: Expecting `--exact` to match a bare name

`cargo test some_name` filters by **substring** of the *full path*. The exact-match flag matches the full path, not the bare function name:

```bash
# Test is `tests::discount_full`. This matches NOTHING, because the full
# path is "tests::discount_full", not "discount_full":
cargo test discount_full -- --exact   # 0 tests run

# Use the full module path:
cargo test tests::discount_full -- --exact   # 1 test runs
```

Real output of the working form:

```text
running 1 test
test tests::discount_full ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s
```

Without `--exact`, `cargo test discount` happily runs *every* test whose path contains `discount`.

---

## Best Practices

### Keep unit tests in the file, next to the code

The idiomatic place for unit tests is a `#[cfg(test)] mod tests` at the bottom of the same file. This keeps a function and its tests within scrolling distance and grants access to private helpers. Reserve the separate `tests/` directory for black-box [integration tests](/13-testing/04-integration-tests/).

### Name tests after the behavior they assert

Because the function name *is* the test name in the output, descriptive names pay off:

```rust
// reads like a spec in the test report
#[test]
fn clamps_a_discount_over_100_percent() { /* ... */ }

// tells you nothing when it fails
#[test]
fn test1() { /* ... */ }
```

The `tests::` prefix and `snake_case` give you sentence-like output: `test tests::clamps_a_discount_over_100_percent ... ok`.

### Use `#[ignore]` for expensive tests, not commenting-out

Mark slow or environment-dependent tests with `#[ignore]` and a reason. They are skipped by default and run on demand.

```rust
#[test]
#[ignore = "slow: only run on demand"]
fn expensive_property_sweep() {
    for a in 0..1000 {
        assert_eq!(a + 0, a);
    }
}
```

A normal `cargo test` reports it as skipped:

```text
test tests::expensive_property_sweep ... ignored, slow: only run on demand
```

Run only the ignored ones with `cargo test -- --ignored`, or everything with `cargo test -- --include-ignored`.

### Know the everyday `cargo test` flags

- `cargo test name_substring`: run only tests whose path contains the substring.
- `cargo test -- --show-output`: print stdout even from **passing** tests (by default it is captured).
- `cargo test -- --nocapture`: let `println!` stream to the terminal live.
- `cargo test -- --test-threads=1`: run serially (useful for debugging order-dependent flakiness).
- `cargo test -- --list`: list the tests without running them.

By default Rust **captures** stdout from passing tests and only shows it on failure. With `--show-output`, a passing test that does `println!("about to add 2 + 2")` reports:

```text
running 3 tests
test tests::adds_small_numbers ... ok
test tests::ten_percent_off ... ok
test tests::prints_while_testing ... ok

successes:

---- tests::prints_while_testing stdout ----
about to add 2 + 2
```

---

## Real-World Example

A production-flavored pricing module with a typed `LineItem`, a **private** helper, and a unit-test module that exercises both the public function and the private one. This is the full, compile-verified file.

```rust
// src/lib.rs
//! A small shopping-cart pricing module.

/// A line item in a shopping cart.
#[derive(Debug, Clone, PartialEq)]
pub struct LineItem {
    pub name: String,
    pub unit_price_cents: u64,
    pub quantity: u32,
}

impl LineItem {
    pub fn new(name: &str, unit_price_cents: u64, quantity: u32) -> Self {
        LineItem {
            name: name.to_string(),
            unit_price_cents,
            quantity,
        }
    }
}

/// Private helper: subtotal for a single line, in cents.
fn line_subtotal(item: &LineItem) -> u64 {
    item.unit_price_cents * item.quantity as u64
}

/// Computes the cart total in cents after applying a whole-cart
/// percentage discount (clamped to 0..=100).
pub fn cart_total_cents(items: &[LineItem], discount_percent: u8) -> u64 {
    let subtotal: u64 = items.iter().map(line_subtotal).sum();
    let percent = discount_percent.min(100) as u64;
    subtotal - (subtotal * percent) / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cart() -> Vec<LineItem> {
        vec![
            LineItem::new("Keyboard", 4_999, 1),
            LineItem::new("Cable", 999, 3),
        ]
    }

    #[test]
    fn subtotal_of_a_line_multiplies_quantity() {
        // `line_subtotal` is private, but the test module sees it via `super`.
        let item = LineItem::new("Cable", 999, 3);
        assert_eq!(line_subtotal(&item), 2_997);
    }

    #[test]
    fn total_without_discount_sums_all_lines() {
        assert_eq!(cart_total_cents(&sample_cart(), 0), 4_999 + 2_997);
    }

    #[test]
    fn discount_is_applied_to_the_whole_cart() {
        // 10% off 7996 -> 7996 - 799 = 7197 (integer division truncates).
        assert_eq!(cart_total_cents(&sample_cart(), 10), 7_197);
    }

    #[test]
    fn discount_over_100_is_clamped() {
        assert_eq!(cart_total_cents(&sample_cart(), 250), 0);
    }

    #[test]
    fn empty_cart_is_free() {
        assert_eq!(cart_total_cents(&[], 0), 0);
    }
}
```

Running `cargo test` produces real output:

```text
running 5 tests
test tests::discount_is_applied_to_the_whole_cart ... ok
test tests::discount_over_100_is_clamped ... ok
test tests::empty_cart_is_free ... ok
test tests::subtotal_of_a_line_multiplies_quantity ... ok
test tests::total_without_discount_sums_all_lines ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

When a test fails (say someone changes the discount formula and `discount_is_applied_to_the_whole_cart` now returns `900` instead of the expected `950`), the report tells you exactly what diverged:

```text
running 3 tests
test tests::adds_small_numbers ... ok
test tests::prints_while_testing ... ok
test tests::ten_percent_off ... FAILED

failures:

---- tests::ten_percent_off stdout ----

thread 'tests::ten_percent_off' panicked at src/lib.rs:17:9:
assertion `left == right` failed
  left: 900
 right: 950
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    tests::ten_percent_off

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The `left`/`right` labels come from `assert_eq!`: `left` is your computed value, `right` is the expected one. `cargo test` exits with a non-zero status (`101`) when any test fails, so CI catches it automatically.

> **Tip:** The exact same suite would run as black-box [integration tests](/13-testing/04-integration-tests/) by moving the test functions into a file under `tests/` and importing the crate, but then they could only call the **public** `cart_total_cents`, not the private `line_subtotal`. Choosing between the two is exactly the topic of [Test Organization](/13-testing/01-test-organization/).

---

## Further Reading

- [The Rust Book — How to Write Tests](https://doc.rust-lang.org/book/ch11-01-writing-tests.html)
- [The Rust Book — Running Tests](https://doc.rust-lang.org/book/ch11-02-running-tests.html)
- [Rust by Example — Unit Testing](https://doc.rust-lang.org/rust-by-example/testing/unit_testing.html)
- [`cargo test` reference](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- Sibling topics in this section:
  - [Test Organization](/13-testing/01-test-organization/): where tests live; private vs public testing.
  - [Assertions](/13-testing/02-assertions/): `assert!`, `assert_eq!`, `assert_ne!`, and custom messages.
  - [`#[should_panic]` and `Result` tests](/13-testing/03-should-panic/): testing failure paths and using `?` in tests.
  - [Integration Tests](/13-testing/04-integration-tests/): the `tests/` directory and black-box testing.
  - [Test Fixtures](/13-testing/05-test-fixtures/): setup/teardown patterns.
  - [Mocking](/13-testing/06-mocking/), [Property Testing](/13-testing/07-property-testing/), [Benchmarking](/13-testing/08-benchmarking/), [Doc Tests](/13-testing/09-doc-tests/), [Coverage](/13-testing/10-coverage/), [TDD Workflow](/13-testing/11-tdd-workflow/).
- Foundations used above:
  - [Cargo Basics](/01-getting-started/03-cargo-basics/): `cargo` is the test runner, too.
  - [Modules and Packages](/12-modules-packages/): how `mod`, `super`, and visibility work.
  - [Result and Option](/08-error-handling/00-result-option/): `.unwrap()` panics, which is how those panics surface in tests.
  - [Macros](/14-macros/): `#[test]`, `assert_eq!`, and friends are macros/attributes; this section explains how that machinery works.

---

## Exercises

### Exercise 1: Your first `#[test]`

**Difficulty:** Easy

**Objective:** Add a unit-test module to an existing function.

**Instructions:** Given the `fizzbuzz` function below, add a `#[cfg(test)] mod tests` module with one `#[test]` that checks the four interesting cases: `1 -> "1"`, `3 -> "Fizz"`, `5 -> "Buzz"`, `15 -> "FizzBuzz"`. Run `cargo test` and confirm it passes.

```rust
pub fn fizzbuzz(n: u32) -> String {
    match (n % 3, n % 5) {
        (0, 0) => "FizzBuzz".to_string(),
        (0, _) => "Fizz".to_string(),
        (_, 0) => "Buzz".to_string(),
        _ => n.to_string(),
    }
}

// TODO: add a #[cfg(test)] mod tests here
```

<details>
<summary>Solution</summary>

```rust
pub fn fizzbuzz(n: u32) -> String {
    match (n % 3, n % 5) {
        (0, 0) => "FizzBuzz".to_string(),
        (0, _) => "Fizz".to_string(),
        (_, 0) => "Buzz".to_string(),
        _ => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fizzbuzz_basics() {
        assert_eq!(fizzbuzz(1), "1");
        assert_eq!(fizzbuzz(3), "Fizz");
        assert_eq!(fizzbuzz(5), "Buzz");
        assert_eq!(fizzbuzz(15), "FizzBuzz");
    }
}
```

`cargo test` output:

```text
running 1 test
test tests::fizzbuzz_basics ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

</details>

### Exercise 2: Test a generic type's behavior

**Difficulty:** Medium

**Objective:** Write multiple focused tests for a small data structure.

**Instructions:** Implement a generic `Stack<T>` with `new`, `push`, `pop`, `len`, and `is_empty`. Then write tests verifying that (a) a new stack is empty, and (b) pushing two items and popping returns them last-in-first-out, ending in `None`.

```rust
#[derive(Default)]
pub struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    // TODO: new, push, pop, len, is_empty
}

// TODO: add tests
```

<details>
<summary>Solution</summary>

```rust
#[derive(Default)]
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
    pub fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stack_is_empty() {
        let stack: Stack<i32> = Stack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
    }

    #[test]
    fn push_then_pop_is_lifo() {
        let mut stack = Stack::new();
        stack.push(1);
        stack.push(2);
        assert_eq!(stack.pop(), Some(2));
        assert_eq!(stack.pop(), Some(1));
        assert_eq!(stack.pop(), None);
    }
}
```

`cargo test` output:

```text
running 2 tests
test tests::new_stack_is_empty ... ok
test tests::push_then_pop_is_lifo ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

</details>

### Exercise 3: Test a private helper through the test module

**Difficulty:** Medium

**Objective:** Exploit the in-file test module's access to private items, something a Vitest black-box suite cannot do without exporting internals.

**Instructions:** Write a **private** `collapse_whitespace(&str) -> String` that turns any run of whitespace into a single space, and a **public** `slugify(&str) -> String` that collapses whitespace, lowercases, and replaces spaces with hyphens. Write tests that check `slugify` *and* that call the private `collapse_whitespace` directly.

```rust
// TODO: a private collapse_whitespace and a public slugify, plus tests
```

<details>
<summary>Solution</summary>

```rust
fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn slugify(title: &str) -> String {
    collapse_whitespace(title)
        .to_lowercase()
        .replace(' ', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_collapses_and_lowercases() {
        assert_eq!(slugify("  Hello   Rust  World "), "hello-rust-world");
    }

    #[test]
    fn collapse_whitespace_is_testable_directly() {
        // Reaching a private fn — only possible because tests live in-file.
        assert_eq!(collapse_whitespace("a    b\tc"), "a b c");
    }
}
```

`cargo test` output:

```text
running 2 tests
test tests::collapse_whitespace_is_testable_directly ... ok
test tests::slugify_collapses_and_lowercases ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

</details>
