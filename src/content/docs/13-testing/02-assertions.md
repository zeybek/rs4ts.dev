---
title: "Assertions: `assert!`, `assert_eq!`, and `assert_ne!`"
description: "Rust's assert!, assert_eq! and assert_ne! replace Jest's expect matchers: a failure panics with both values, needs no import, and relies on Debug and PartialEq."
---

Assertions are the checks inside your tests that decide pass or fail. Rust ships three built-in assertion macros that cover almost everything you reach for `expect(...).toBe(...)` for in Jest or Vitest, with a key twist: when an assertion fails, Rust **panics** and prints a precise, value-rich report instead of throwing a JavaScript exception.

---

## Quick Overview

Rust's standard library provides `assert!`, `assert_eq!`, and `assert_ne!`: no matcher library, no `import`, no setup. They are available everywhere because they live in the prelude. A failing assertion panics with the file, line, and the actual values involved, which is how a `#[test]` function is marked as failed. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically, and these macros have been stable since Rust 1.0.

> **Note:** This page covers the assertion macros themselves and how their failure output reads. For writing and running the surrounding tests, see [Unit Tests](/13-testing/00-unit-tests/); for asserting that code *panics on purpose*, see [Testing for Panics](/13-testing/03-should-panic/); for returning `Result` from a test and using `?`, also see [Testing for Panics](/13-testing/03-should-panic/).

---

## TypeScript/JavaScript Example

In Jest or Vitest you assert through a fluent **matcher** API: `expect(value)` returns an object, and you chain a matcher like `.toBe`, `.toEqual`, or `.toBeTruthy`. The matcher both performs the comparison and formats the diff when it fails.

```typescript
// cart.ts
export interface LineItem {
  sku: string;
  quantity: number;
}

export function slugify(title: string): string {
  return title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function parsePort(raw: string): number | null {
  const n = Number(raw.trim());
  return Number.isInteger(n) && n > 0 && n <= 65535 ? n : null;
}
```

```typescript
// cart.test.ts (Vitest / Jest share this API)
import { describe, it, expect } from "vitest";
import { slugify, parsePort } from "./cart";

describe("slugify", () => {
  it("lowercases and collapses separators", () => {
    expect(slugify("  Rust   &  TS  ")).toBe("rust-ts");
  });

  it("rejects port 0", () => {
    expect(parsePort("0")).toBeNull();
    // A custom message is the optional second arg to the matcher in Vitest:
    expect(parsePort("8080"), "8080 should be a valid port").not.toBeNull();
  });
});
```

Key things to notice for the comparison below:

- `expect(...).toBe(x)` is **reference/`Object.is` equality**; `expect(...).toEqual(x)` is **deep structural** equality. JavaScript devs constantly choose between them.
- A failing matcher **throws**, which the test runner catches and reports.
- Custom failure messages are matcher-specific second arguments (and not all matchers accept them).

---

## Rust Equivalent

Rust collapses all of this into three macros. There is no `expect()` wrapper and no `.toBe` vs `.toEqual` distinction. `assert_eq!` always compares **by value** (via the `PartialEq` trait), which is the structural comparison you almost always want.

```rust
// src/lib.rs
pub fn slugify(title: &str) -> String {
    title
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn parse_port(raw: &str) -> Option<u16> {
    raw.trim().parse::<u16>().ok().filter(|&p| p > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_lowercases_and_collapses() {
        assert_eq!(slugify("  Rust   &  TS  "), "rust-ts");
    }

    #[test]
    fn port_rejects_zero() {
        assert!(parse_port("0").is_none());
        // assert_ne! with an optional custom message (note the trailing args).
        assert_ne!(parse_port("8080"), None, "8080 should be a valid port");
    }
}
```

Running `cargo test` reports each `#[test]` function:

```text
running 2 tests
test tests::port_rejects_zero ... ok
test tests::slug_lowercases_and_collapses ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The three macros, at a glance:

| Macro            | Checks                | Jest/Vitest analogue            |
| ---------------- | --------------------- | ------------------------------- |
| `assert!(expr)`  | `expr` is `true`      | `expect(x).toBe(true)` / `.toBeTruthy()` |
| `assert_eq!(a, b)` | `a == b`            | `expect(a).toEqual(b)`          |
| `assert_ne!(a, b)` | `a != b`            | `expect(a).not.toEqual(b)`      |

---

## Detailed Explanation

### `assert!` takes a `bool`, not a "truthy" value

`assert!(cond)` panics unless `cond` is exactly the boolean `true`. This is the biggest mental shift from JavaScript: there is **no truthiness**. You cannot write `assert!(items.len())` hoping that a non-zero length counts as true. `items.len()` is a `usize`, and Rust will reject it at compile time:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn non_bool_assert() {
        let count = 3;
        assert!(count); // does not compile (error[E0308]: mismatched types)
    }
}
```

The real compiler error:

```text
error[E0308]: mismatched types
 --> src/lib.rs:6:9
  |
6 |         assert!(count); // does not compile (error[E0308]: mismatched types)
  |         ^^^^^^^^^^^^^^ expected `bool`, found integer

For more information about this error, try `rustc --explain E0308`.
```

You must write an explicit comparison: `assert!(count > 0)` or `assert!(count == 3)`. Unlike a JavaScript `if (count)`, this forces you to state what "true" means, which catches a whole category of "I meant `=== 0`" bugs.

### `assert_eq!` / `assert_ne!` print both sides on failure

The reason to prefer `assert_eq!(a, b)` over `assert!(a == b)` is the failure message. `assert!(a == b)` can only tell you "the expression was false." `assert_eq!` knows both operands, so it prints them:

```rust
pub fn slugify(title: &str) -> String {
    // BUG (intentional): does not collapse repeated separators.
    title
        .trim()
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_collapses_separators() {
        assert_eq!(slugify("Hello, World!"), "hello-world"); // fails at runtime
    }
}
```

The real output from `cargo test`:

```text
running 1 test
test tests::slug_collapses_separators ... FAILED

failures:

---- tests::slug_collapses_separators stdout ----

thread 'tests::slug_collapses_separators' panicked at src/lib.rs:12:9:
assertion `left == right` failed
  left: "hello--world-"
 right: "hello-world"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`left` is the first argument (your computed value), `right` is the second (your expected value). The convention is **actual on the left, expected on the right**, mirroring `assert_eq!(got, want)`. Rust does not enforce this (the labels are literally `left`/`right`, not "actual"/"expected"), but staying consistent makes failures readable.

### A failed assertion is a panic, which is how a test fails

There is no `throw`/`catch` here. `assert_eq!` expands to roughly "if the values differ, call `panic!` with this message." The test harness runs each `#[test]` function, catches the panic via the unwinding machinery, and records the test as `FAILED`. That is the entire mechanism: assertions and `#[test]` are decoupled. You can even call these macros in non-test code (e.g. to enforce an invariant at startup), where a failure aborts the program. See [Panics](/08-error-handling/02-panic/) for how panicking works in general.

### Comparing requires `PartialEq`; printing the failure requires `Debug`

`assert_eq!(a, b)` needs to do two things when it fails:

1. **Compare** `a` and `b`, so both must implement the [`PartialEq`](https://doc.rust-lang.org/std/cmp/trait.PartialEq.html) trait (that is what the `==` operator dispatches to).
2. **Print** `a` and `b` in the failure message, so both must implement [`Debug`](https://doc.rust-lang.org/std/fmt/trait.Debug.html), the formatter used by `{:?}`.

For your own structs and enums, you get both with a one-line derive. This is the single most important habit for testing custom types in Rust:

```rust
#[derive(Debug, PartialEq)]
pub struct LineItem {
    pub sku: String,
    pub quantity: u32,
}

pub fn parse_line(raw: &str) -> Option<LineItem> {
    let (sku, qty) = raw.split_once('x')?;
    Some(LineItem {
        sku: sku.trim().to_string(),
        quantity: qty.trim().parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_line_item() {
        let got = parse_line("WIDGET x 3").unwrap();
        // Expected quantity is wrong on purpose to show the diff.
        assert_eq!(
            got,
            LineItem { sku: "WIDGET".to_string(), quantity: 5 }
        ); // fails at runtime
    }
}
```

Because `LineItem` derives `Debug`, the failure prints the full structures, not just "not equal":

```text
thread 'tests::parses_line_item' panicked at src/lib.rs:23:9:
assertion `left == right` failed
  left: LineItem { sku: "WIDGET", quantity: 3 }
 right: LineItem { sku: "WIDGET", quantity: 5 }
```

This is the equivalent of Jest's structural diff for objects, except you opt into it explicitly with `#[derive(Debug)]`. Unlike TypeScript, where `console.log(obj)` and matcher diffs work on any value reflectively, Rust has no runtime reflection: the `Debug` impl is generated at compile time, and a type without it simply cannot be auto-printed.

---

## Key Differences

| Concept                  | TypeScript (Jest/Vitest)                          | Rust                                                     |
| ------------------------ | ------------------------------------------------- | -------------------------------------------------------- |
| API shape                | Fluent `expect(x).matcher(y)`                     | Macros `assert_eq!(x, y)` (no wrapper object)            |
| Import needed            | `import { expect } from "vitest"`                 | None — in the prelude, always available                  |
| Equality kinds           | `toBe` (Object.is) vs `toEqual` (deep)            | One `assert_eq!`, always by value via `PartialEq`        |
| Truthiness               | `expect(x).toBeTruthy()` accepts any value        | `assert!` requires a real `bool` (no truthiness)         |
| Failure mechanism        | Throws an `Error` the runner catches              | Panics; the harness records the panic as a failure       |
| Printing values          | Reflective, automatic for any object              | Needs `Debug` (usually `#[derive(Debug)]`)               |
| Comparing custom types   | Automatic deep compare                            | Needs `PartialEq` (usually `#[derive(PartialEq)]`)       |
| Custom message           | Matcher-specific extra argument                   | Trailing `format!`-style args on any of the three macros |
| Approximate float compare| `expect(x).toBeCloseTo(y)`                        | No built-in; compare `(x - y).abs() < eps` yourself      |

The deepest conceptual difference: in JavaScript the matcher library does the heavy lifting at runtime, inspecting arbitrary values reflectively. In Rust the work is split between **traits resolved at compile time** (`PartialEq` for comparing, `Debug` for printing) and a tiny macro that wires them into a `panic!`. If a type doesn't implement those traits, the test does not compile: the failure moves from runtime to compile time, which is the recurring Rust theme.

---

## Custom Failure Messages

All three macros accept optional trailing arguments after the values, using the exact same syntax as [`println!`](/02-basics/04-output/) — a format string plus interpolated values. This is the analogue of a Jest custom matcher message, and it is appended to the standard report rather than replacing it.

```rust
pub fn discount_price(price: f64, percent_off: f64) -> f64 {
    price * (1.0 - percent_off / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_assert_fails() {
        let total = discount_price(100.0, 25.0);
        assert!(total < 50.0); // false: total is 75.0
    }

    #[test]
    fn custom_message_assert() {
        let total = discount_price(100.0, 25.0);
        assert!(
            total < 50.0,
            "expected discounted total under 50, got {total}"
        ); // fails with the custom message
    }

    #[test]
    fn custom_message_eq() {
        let users = vec!["alice", "bob"];
        assert_eq!(
            users.len(),
            3,
            "roster should have 3 members but had {}: {:?}",
            users.len(),
            users
        ); // fails with the custom message
    }
}
```

The real output shows how a bare `assert!` is the *least* informative, and how the custom message rides along with `assert_eq!`'s automatic `left`/`right` dump:

```text
---- tests::custom_message_eq stdout ----
thread 'tests::custom_message_eq' panicked at src/lib.rs:27:9:
assertion `left == right` failed: roster should have 3 members but had 2: ["alice", "bob"]
  left: 2
 right: 3

---- tests::custom_message_assert stdout ----
thread 'tests::custom_message_assert' panicked at src/lib.rs:18:9:
expected discounted total under 50, got 75

---- tests::plain_assert_fails stdout ----
thread 'tests::plain_assert_fails' panicked at src/lib.rs:12:9:
assertion failed: total < 50.0
```

Notice that the bare `assert!(total < 50.0)` can only echo the *source text* of the condition (`total < 50.0`); it cannot show that `total` was `75`. That is exactly when a custom message earns its keep: for `assert!`, include the runtime value in the message so a failure is debuggable.

> **Tip:** The format string uses inline captures like `{total}` for variables in scope (stable since Rust 1.58). Use `{:?}` (Debug) for collections and structs, and positional `{}` for values you pass explicitly. Avoid the old redundant `format!("{x}", x = x)` style.

---

## Common Pitfalls

### Forgetting `#[derive(PartialEq)]` on a compared type

If you `assert_eq!` two values of a type that can't be compared with `==`, the error is about the missing trait, not about the assertion:

```rust
#[derive(Debug)] // has Debug, but NO PartialEq
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_equal() {
        let a = Point { x: 1, y: 2 };
        let b = Point { x: 1, y: 2 };
        assert_eq!(a, b); // does not compile (error[E0369])
    }
}
```

The real compiler error tells you precisely what to add:

```text
error[E0369]: binary operation `==` cannot be applied to type `Point`
  --> src/lib.rs:15:9
   |
15 |         assert_eq!(a, b); // does not compile (error[E0369])
   |         ^^^^^^^^^^^^^^^^
   |         |
   |         Point
   |         Point
   |
note: an implementation of `PartialEq` might be missing for `Point`
help: consider annotating `Point` with `#[derive(PartialEq)]`
```

### Forgetting `#[derive(Debug)]` (so the failure can't be printed)

This is the more confusing one, because the type compares fine; the error only appears because the *failure message* needs to print the values. If `Point` derives `PartialEq` but not `Debug`:

```text
error[E0277]: `Point` doesn't implement `Debug`
  --> src/lib.rs:15:9
   |
15 |         assert_eq!(a, b); // does not compile (error[E0277])
   |         ^^^^^^^^^^^^^^^^ the trait `Debug` is not implemented for `Point`
   |
   = note: add `#[derive(Debug)]` to `Point` or manually `impl Debug for Point`
help: consider annotating `Point` with `#[derive(Debug)]`
```

The fix for both pitfalls is the same single line you should make a reflex on any type you test: `#[derive(Debug, PartialEq)]`.

### Comparing floats with `assert_eq!`

JavaScript devs know `0.1 + 0.2 !== 0.3`, but it is easy to forget when reaching for `assert_eq!`. Rust uses the same IEEE-754 `f64`, so exact equality fails the same way:

```rust
pub fn add(a: f64, b: f64) -> f64 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naive_float_eq() {
        assert_eq!(add(0.1, 0.2), 0.3); // fails: not exactly 0.3
    }

    #[test]
    fn epsilon_compare() {
        let got = add(0.1, 0.2);
        let want = 0.3;
        assert!(
            (got - want).abs() < 1e-10,
            "expected ~{want}, got {got} (diff {})",
            (got - want).abs()
        );
    }
}
```

The naive test fails exactly as it would in Node, and the epsilon test passes:

```text
running 2 tests
test tests::epsilon_compare ... ok
test tests::naive_float_eq ... FAILED

---- tests::naive_float_eq stdout ----
thread 'tests::naive_float_eq' panicked at src/lib.rs:12:9:
assertion `left == right` failed
  left: 0.30000000000000004
 right: 0.3
```

Rust's standard library deliberately has no `assert_approx_eq!`: compare against an epsilon yourself, or pull in a crate (see Best Practices). This is Jest's `toBeCloseTo`, hand-rolled.

### Putting a stray comma where a custom message goes

`assert!(a == b,)` or `assert_eq!(a, b,)` with a trailing comma is fine, but `assert_eq!(a, b, c)` treats `c` as the **format string** of a custom message. If `c` isn't a string literal you'll get a format-string error, not the comparison you intended. The third positional argument is always the message, never a third value to compare.

---

## Best Practices

- **Default to `assert_eq!`/`assert_ne!` over `assert!` for equality.** They print both operands; `assert!(a == b)` cannot. Reserve bare `assert!` for genuine boolean predicates (`assert!(cart.is_empty())`, `assert!(result.is_ok())`).
- **Derive `Debug` and `PartialEq` on every type you assert on.** `#[derive(Debug, PartialEq)]` is idiomatic and free; without it your tests won't compile.
- **Order arguments as `(got, want)`** — actual first, expected second. The labels are only `left`/`right`, so consistency is on you, but it makes every failure read the same way.
- **Put runtime values in `assert!` messages.** Since `assert!` can't introspect operands, `assert!(n > 0, "n was {n}")` turns an opaque failure into an obvious one.
- **For structs with many fields, consider the [`pretty_assertions`](https://crates.io/crates/pretty_assertions) crate** as a drop-in replacement. It overrides `assert_eq!`/`assert_ne!` to print a colored, line-by-line diff — much easier to scan than two long one-line `Debug` dumps. Add it as a dev-dependency:

  ```bash
  cargo add pretty_assertions --dev
  ```

  ```rust
  #[derive(Debug, PartialEq)]
  pub struct Config {
      pub host: String,
      pub port: u16,
      pub tls: bool,
  }

  pub fn default_config() -> Config {
      Config { host: "localhost".to_string(), port: 8080, tls: false }
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      use pretty_assertions::assert_eq; // shadow std's macro in this module only

      #[test]
      fn config_matches() {
          assert_eq!(
              default_config(),
              Config { host: "localhost".to_string(), port: 9090, tls: true }
          ); // fails with a diff
      }
  }
  ```

  The failure highlights only the fields that differ (colors shown here as `<`/`>` markers):

  ```text
  thread 'tests::config_matches' panicked at src/lib.rs:19:9:
  assertion failed: `(left == right)`

  Diff < left / right > :
   Config {
       host: "localhost",
  <    port: 8080,
  <    tls: false,
  >    port: 9090,
  >    tls: true,
   }
  ```

- **There's also `debug_assert!` / `debug_assert_eq!` / `debug_assert_ne!`.** These are identical but compile to nothing in release builds (`cargo build --release`). They are for invariants in *library code* that you don't want to pay for in production, not generally for tests, since tests run in debug mode anyway.

---

## Real-World Example

A small shopping-cart module with a test suite that uses all three macros, a custom message, and the `Debug, PartialEq` derive habit. This is the shape of a typical unit-test module living alongside the code it tests.

```rust
//! A tiny shopping-cart module demonstrating the assertion macros.

#[derive(Debug, Clone, PartialEq)]
pub struct LineItem {
    pub sku: String,
    pub unit_price_cents: u64,
    pub quantity: u32,
}

#[derive(Debug, Default)]
pub struct Cart {
    items: Vec<LineItem>,
}

impl Cart {
    pub fn new() -> Self {
        Cart::default()
    }

    /// Adds an item; merges quantity if the SKU already exists.
    pub fn add(&mut self, sku: &str, unit_price_cents: u64, quantity: u32) {
        if let Some(existing) = self.items.iter_mut().find(|i| i.sku == sku) {
            existing.quantity += quantity;
        } else {
            self.items.push(LineItem {
                sku: sku.to_string(),
                unit_price_cents,
                quantity,
            });
        }
    }

    pub fn items(&self) -> &[LineItem] {
        &self.items
    }

    pub fn total_cents(&self) -> u64 {
        self.items
            .iter()
            .map(|i| i.unit_price_cents * i.quantity as u64)
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cart_is_empty() {
        let cart = Cart::new();
        assert!(cart.is_empty(), "a freshly constructed cart must be empty");
        assert_eq!(cart.total_cents(), 0);
    }

    #[test]
    fn adding_distinct_items_keeps_them_separate() {
        let mut cart = Cart::new();
        cart.add("APPLE", 50, 3);
        cart.add("BREAD", 200, 1);

        assert_eq!(cart.items().len(), 2);
        assert_eq!(cart.total_cents(), 50 * 3 + 200);
    }

    #[test]
    fn adding_same_sku_merges_quantity() {
        let mut cart = Cart::new();
        cart.add("APPLE", 50, 3);
        cart.add("APPLE", 50, 2);

        assert_eq!(
            cart.items().len(),
            1,
            "same SKU should merge into one line, got {:#?}",
            cart.items()
        );
        assert_eq!(cart.items()[0].quantity, 5);
        assert_ne!(cart.total_cents(), 0);
    }
}
```

All three tests pass:

```text
running 3 tests
test tests::adding_distinct_items_keeps_them_separate ... ok
test tests::adding_same_sku_merges_quantity ... ok
test tests::new_cart_is_empty ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Two details worth copying: `LineItem` derives `Debug, PartialEq` so it can appear in `assert_eq!`, and the merge test uses `{:#?}` (pretty Debug) in its custom message so that if the merge logic ever regresses, the failure dumps the full item list on multiple lines for easy reading.

---

## Further Reading

- [`assert!` macro](https://doc.rust-lang.org/std/macro.assert.html) — official std documentation
- [`assert_eq!` macro](https://doc.rust-lang.org/std/macro.assert_eq.html) and [`assert_ne!` macro](https://doc.rust-lang.org/std/macro.assert_ne.html)
- [`debug_assert!` macro](https://doc.rust-lang.org/std/macro.debug_assert.html) — the release-stripped variants
- [`PartialEq` trait](https://doc.rust-lang.org/std/cmp/trait.PartialEq.html) and [`Debug` trait](https://doc.rust-lang.org/std/fmt/trait.Debug.html) — the two traits assertions rely on
- [The Rust Book: How to Write Tests](https://doc.rust-lang.org/book/ch11-01-writing-tests.html)
- [`pretty_assertions` crate](https://crates.io/crates/pretty_assertions) — colored diffs for `assert_eq!`
- Related sections in this guide:
  - [Unit Tests](/13-testing/00-unit-tests/) — the `#[test]` and `#[cfg(test)]` machinery these assertions live inside
  - [Testing for Panics](/13-testing/03-should-panic/) — `#[should_panic]` and `Result`-returning tests with `?`
  - [Test Organization](/13-testing/01-test-organization/) — where test modules belong
  - [Integration Tests](/13-testing/04-integration-tests/) — assertions in `tests/` against the public API
  - [Property Testing](/13-testing/07-property-testing/) — generating assertion inputs automatically with proptest
  - [Panics](/08-error-handling/02-panic/) — the panic mechanism a failing assertion uses
  - [Output and Formatting](/02-basics/04-output/) — the `{}`/`{:?}`/`{:#?}` format syntax custom messages share
  - [Macros](/14-macros/) — how `assert_eq!` and friends are implemented as macros

---

## Exercises

### Exercise 1: Convert and compare

**Difficulty:** Easy

**Objective:** Practice all three macros, including an approximate float comparison.

**Instructions:**

1. Write `celsius_to_fahrenheit(c: f64) -> f64` using the formula `c * 9/5 + 32`.
2. In a `#[cfg(test)] mod tests`, write one test that:
   - uses `assert_eq!` to check that `0.0 C` is exactly `32.0 F`,
   - uses `assert!` with an epsilon (`< 1e-9`) to check that `100.0 C` is about `212.0 F`, including a custom message that prints the actual value,
   - uses `assert_ne!` to confirm `37.0 C` is not `0.0 F`.

<details>
<summary>Solution</summary>

```rust
pub fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freezing_and_boiling() {
        assert_eq!(celsius_to_fahrenheit(0.0), 32.0);

        let boiling = celsius_to_fahrenheit(100.0);
        assert!(
            (boiling - 212.0).abs() < 1e-9,
            "100C should be ~212F, got {boiling}"
        );

        assert_ne!(celsius_to_fahrenheit(37.0), 0.0);
    }
}
```

`0.0` and `100.0` happen to convert to values representable exactly, so `assert_eq!` is safe for freezing; in general prefer the epsilon form for float results, as the boiling check shows.

</details>

### Exercise 2: Assert on a custom type

**Difficulty:** Medium

**Objective:** Experience the `Debug` + `PartialEq` requirement firsthand and write a custom failure message.

**Instructions:**

1. Define `struct Rgb { r: u8, g: u8, b: u8 }` and derive whatever traits you need to compare it in `assert_eq!`.
2. Write `parse_hex(code: &str) -> Option<Rgb>` that parses `"#RRGGBB"` (return `None` for a missing `#`, wrong length, or non-hex digits).
3. Write tests: one asserting `parse_hex("#FF8000")` equals `Some(Rgb { r: 255, g: 128, b: 0 })`, and one asserting several malformed inputs return `None`, each with a custom message saying which input failed.

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, PartialEq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub fn parse_hex(code: &str) -> Option<Rgb> {
    let hex = code.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Rgb { r, g, b })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_hex() {
        assert_eq!(parse_hex("#FF8000"), Some(Rgb { r: 255, g: 128, b: 0 }));
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(parse_hex("FF8000"), None, "missing # should be rejected");
        assert_eq!(parse_hex("#FFF"), None, "wrong length should be rejected");
        assert!(parse_hex("#GG0000").is_none());
    }
}
```

Without `#[derive(Debug, PartialEq)]` on `Rgb`, the `assert_eq!` lines fail to compile (E0369 for the missing `PartialEq`, E0277 for the missing `Debug`).

</details>

### Exercise 3: Summary statistics with a structural assertion

**Difficulty:** Medium-Hard

**Objective:** Combine `Option`, a derived struct, and a `{:#?}`-style failure message.

**Instructions:**

1. Define `struct Stats { count: usize, sum: i64, max: i64 }` with the right derives.
2. Write `summarize(values: &[i64]) -> Option<Stats>` that returns `None` for an empty slice and otherwise computes count, sum, and max.
3. Write tests that: assert the fields for `&[3, 7, 2, 9]`, and assert that an empty slice yields `None` with a custom message explaining the expectation. Use `.expect(...)` to unwrap the non-empty case.

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, PartialEq)]
pub struct Stats {
    pub count: usize,
    pub sum: i64,
    pub max: i64,
}

pub fn summarize(values: &[i64]) -> Option<Stats> {
    if values.is_empty() {
        return None;
    }
    Some(Stats {
        count: values.len(),
        sum: values.iter().sum(),
        max: *values.iter().max().unwrap(), // safe: slice is non-empty here
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_values() {
        let stats = summarize(&[3, 7, 2, 9]).expect("non-empty slice should summarize");
        assert_eq!(stats.count, 4);
        assert_eq!(stats.sum, 21);
        assert_eq!(stats.max, 9);
    }

    #[test]
    fn empty_slice_is_none() {
        assert_eq!(
            summarize(&[]),
            None,
            "empty input must produce None, not a zeroed Stats"
        );
    }
}
```

You could also assert the whole struct at once with `assert_eq!(stats, Stats { count: 4, sum: 21, max: 9 })`. Thanks to the `PartialEq` derive, comparing the entire value in one assertion is idiomatic and gives the clearest diff on failure.

</details>
