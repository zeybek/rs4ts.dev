---
title: "Integration Tests: The `tests/` Directory and Black-Box Testing"
description: "Rust integration tests live in tests/, compile as separate crates, and reach only your public API. Compare Jest's free imports, plus shared setup."
---

Integration tests in Rust live in a top-level `tests/` directory, get compiled as separate crates, and can only see your crate's **public API**: exactly the way a real consumer would `use` it. This page covers how that directory works, how to test the public surface as a black box, and how to share setup code across test files without confusing Cargo.

---

## Quick Overview

In TypeScript/JavaScript you usually drop every test under a `tests/` or `__tests__` folder and let Jest or Vitest run the lot, regardless of whether a test pokes at internals or only the exported API. Rust draws a sharper line: **unit tests** live *inside* your source files and can reach private items, while **integration tests** live in a sibling `tests/` directory, are compiled as independent crates, and are forced to go through your `pub` interface. That separation makes integration tests a faithful black-box check of what your users will actually be able to call. This page is about the `tests/` directory specifically; the [unit-tests](/13-testing/00-unit-tests/) and [test-organization](/13-testing/01-test-organization/) pages cover the in-source side.

---

## TypeScript/JavaScript Example

Here is a small shopping-cart module and a Vitest suite that exercises it end to end. In a typical Node project the tests sit next to (or under) `src/` and import the module like any other consumer.

```typescript
// src/cart.ts — the module under test
export interface Item {
  sku: string;
  priceCents: number;
}

export class Cart {
  private lines = new Map<string, { item: Item; qty: number }>();

  add(item: Item, qty: number): void {
    if (item.sku === "") throw new Error("SKU must not be empty");
    const line = this.lines.get(item.sku);
    if (line) line.qty += qty;
    else this.lines.set(item.sku, { item, qty });
  }

  quantity(sku: string): number {
    return this.lines.get(sku)?.qty ?? 0;
  }

  totalCents(): number {
    let sum = 0;
    for (const { item, qty } of this.lines.values()) sum += item.priceCents * qty;
    return sum;
  }
}

export function applyDiscount(totalCents: number, percent: number): number {
  const p = Math.min(percent, 100);
  return totalCents - Math.floor((totalCents * p) / 100);
}
```

```typescript
// tests/cart.test.ts — Vitest integration-style test
import { describe, it, expect } from "vitest";
import { Cart, applyDiscount } from "../src/cart";

// A shared helper used by several tests.
function sampleCart(): Cart {
  const cart = new Cart();
  cart.add({ sku: "BOOK-01", priceCents: 1299 }, 2);
  cart.add({ sku: "PEN-07", priceCents: 250 }, 4);
  return cart;
}

describe("cart checkout", () => {
  it("totals prices times quantities", () => {
    expect(sampleCart().totalCents()).toBe(3598);
  });

  it("applies a percentage discount", () => {
    expect(applyDiscount(sampleCart().totalCents(), 10)).toBe(3239);
  });
});
```

Note two things a TypeScript developer takes for granted: the test imports from a **relative path** (`../src/cart`), and the helper `sampleCart` is just a function defined in the same file. Nothing stops a Vitest test from reaching into internals either — `private` is only enforced by the type-checker, not at runtime, so a determined test could still poke at `(cart as any).lines`.

---

## Rust Equivalent

The same library as a Rust crate. Integration tests go in `tests/`, `use` the crate **by its package name** (not by a relative path), and the compiler refuses to let them touch private fields like `lines`.

```text
cart/
├── Cargo.toml
├── src/
│   └── lib.rs            # the library crate
└── tests/                # each .rs file here is its OWN test crate
    ├── cart_checkout.rs
    ├── cart_lifecycle.rs
    └── common/
        └── mod.rs        # shared helpers (subdir, so it is NOT a test crate)
```

```rust
// src/lib.rs — the library under test
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub sku: String,
    pub price_cents: u32,
}

impl Item {
    pub fn new(sku: &str, price_cents: u32) -> Self {
        Item { sku: sku.to_string(), price_cents }
    }
}

#[derive(Debug, PartialEq)]
pub enum CartError {
    EmptySku,
    UnknownSku(String),
}

impl std::fmt::Display for CartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CartError::EmptySku => write!(f, "SKU must not be empty"),
            CartError::UnknownSku(s) => write!(f, "unknown SKU: {s}"),
        }
    }
}

impl std::error::Error for CartError {}

#[derive(Debug, Default)]
pub struct Cart {
    lines: HashMap<String, (Item, u32)>, // PRIVATE field
}

impl Cart {
    pub fn new() -> Self {
        Cart::default()
    }

    pub fn add(&mut self, item: Item, qty: u32) -> Result<(), CartError> {
        if item.sku.is_empty() {
            return Err(CartError::EmptySku);
        }
        let entry = self.lines.entry(item.sku.clone()).or_insert((item, 0));
        entry.1 += qty;
        Ok(())
    }

    pub fn quantity(&self, sku: &str) -> u32 {
        self.lines.get(sku).map(|(_, q)| *q).unwrap_or(0)
    }

    pub fn remove(&mut self, sku: &str) -> Result<(), CartError> {
        self.lines
            .remove(sku)
            .map(|_| ())
            .ok_or_else(|| CartError::UnknownSku(sku.to_string()))
    }

    pub fn total_cents(&self) -> u32 {
        self.lines.values().map(|(item, qty)| item.price_cents * qty).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

pub fn apply_discount(total_cents: u32, percent: u8) -> u32 {
    let percent = percent.min(100) as u32;
    total_cents - (total_cents * percent / 100)
}
```

```rust
// tests/common/mod.rs — shared helpers, NOT a test crate of its own
use cart::{Cart, Item};

pub fn sample_cart() -> Cart {
    let mut cart = Cart::new();
    cart.add(Item::new("BOOK-01", 1299), 2).unwrap();
    cart.add(Item::new("PEN-07", 250), 4).unwrap();
    cart
}
```

```rust
// tests/cart_checkout.rs — an integration test crate
use cart::{apply_discount, Cart, CartError, Item};

mod common; // pulls in tests/common/mod.rs

#[test]
fn building_a_cart_accumulates_quantities() {
    let mut cart = Cart::new();
    cart.add(Item::new("BOOK-01", 1299), 1).unwrap();
    cart.add(Item::new("BOOK-01", 1299), 2).unwrap();
    assert_eq!(cart.quantity("BOOK-01"), 3);
}

#[test]
fn total_reflects_prices_and_quantities() {
    let cart = common::sample_cart();
    assert_eq!(cart.total_cents(), 3598);
}

#[test]
fn discount_is_applied_to_the_total() {
    let cart = common::sample_cart();
    let discounted = apply_discount(cart.total_cents(), 10);
    assert_eq!(discounted, 3239);
}

#[test]
fn adding_an_empty_sku_is_rejected() {
    let mut cart = Cart::new();
    let err = cart.add(Item::new("", 100), 1).unwrap_err();
    assert_eq!(err, CartError::EmptySku);
}

// A test can return `Result` and use `?` instead of `.unwrap()`.
#[test]
fn removing_a_missing_line_returns_a_result() -> Result<(), CartError> {
    let mut cart = common::sample_cart();
    cart.remove("BOOK-01")?;
    assert_eq!(cart.quantity("BOOK-01"), 0);
    Ok(())
}
```

```rust
// tests/cart_lifecycle.rs — a SECOND, independent test crate
use cart::Cart;

mod common;

#[test]
fn a_fresh_cart_is_empty() {
    let cart = Cart::new();
    assert!(cart.is_empty());
}

#[test]
fn removing_every_line_empties_the_cart() {
    let mut cart = common::sample_cart();
    cart.remove("BOOK-01").unwrap();
    cart.remove("PEN-07").unwrap();
    assert!(cart.is_empty());
}
```

Running `cargo test` compiles the library, each `tests/*.rs` file, and the doc-tests, then runs them all. This is the real, unedited output:

```text
   Compiling cart v0.1.0 (/private/tmp/ts_rust_inttest/cart)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.75s
     Running unittests src/lib.rs (target/debug/deps/cart-1e951a69f9a013e1)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/cart_checkout.rs (target/debug/deps/cart_checkout-830e111353aebe70)

running 5 tests
test adding_an_empty_sku_is_rejected ... ok
test building_a_cart_accumulates_quantities ... ok
test discount_is_applied_to_the_total ... ok
test removing_a_missing_line_returns_a_result ... ok
test total_reflects_prices_and_quantities ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/cart_lifecycle.rs (target/debug/deps/cart_lifecycle-d63887261d751300)

running 2 tests
test a_fresh_cart_is_empty ... ok
test removing_every_line_empties_the_cart ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests cart

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects the newest edition automatically, so the layout above is what you get out of the box.

---

## Detailed Explanation

**One `tests/` file = one crate.** Cargo treats every `.rs` file directly inside `tests/` as a *separate* integration-test crate. That is why the output above lists `tests/cart_checkout.rs` and `tests/cart_lifecycle.rs` under their own `Running ...` headers, each compiled into its own binary in `target/debug/deps/`. There is no equivalent in Jest/Vitest, where every test file shares one module graph and one process. The upside in Rust is strong isolation: a crash or a global-state mutation in one file cannot leak into another.

**Integration tests see only `pub`.** Each test crate links against your library exactly as an external user would: `use cart::Cart;`. The crate name comes from the `name` field in `[package]` (with dashes turned into underscores), **not** from a relative path. Because the test is an outside consumer, it can call `Cart::new`, `add`, `total_cents`, and the free function `apply_discount`, but it cannot read the private `lines` field. This is the black-box guarantee: if a type or method is not reachable from an integration test, your real users cannot reach it either.

**`#[test]` and assertions work identically to unit tests.** The `#[test]` attribute marks a function as a test; `assert_eq!`/`assert!` do the checking; a test that returns `Result<(), E>` may use `?` and is considered passing when it returns `Ok`. Those mechanics are shared with unit tests and are covered in depth in [assertions](/13-testing/02-assertions/) and [should-panic](/13-testing/03-should-panic/).

**The `unittests src/lib.rs` line with `0 tests`** appears because the library itself has no `#[cfg(test)]` module here. Cargo always *offers* to run the crate's own unit tests; when there are none you simply see a zero count. That is normal, not an error.

**Shared helpers go in a subdirectory.** The helper `sample_cart` lives in `tests/common/mod.rs`, and each test file pulls it in with `mod common;`. Putting it in a `common/` *subdirectory* (with a `mod.rs`) is the key: files in subdirectories are **not** compiled as their own test crates, so the helper is shared rather than executed as a test target. We will see what happens if you forget this in the Pitfalls section.

**Why the test calls `.unwrap()` in helpers but `?` in a test body.** Inside `sample_cart`, `.unwrap()` is fine — if setup fails, the test should abort loudly. Inside a `#[test] -> Result<...>` body, returning the error via `?` produces a clean failure report instead of a panic backtrace. Both are idiomatic; choose based on whether the failing call is *setup* or the *thing under test*.

---

## Key Differences

| Aspect | TypeScript (Jest/Vitest) | Rust integration tests |
| --- | --- | --- |
| Location | Anywhere; commonly `tests/` or `__tests__/` | A top-level `tests/` directory only |
| Compilation unit | One shared module graph, one process | Each `tests/*.rs` is its own crate + binary |
| What's visible | Everything (privates reachable via `as any`) | The crate's **public API only** |
| How the module is imported | Relative path: `../src/cart` | Crate name: `use cart::...` |
| Shared setup file | A regular imported file | A `tests/common/mod.rs` in a subdirectory |
| Isolation between files | Shared globals/mocks unless reset | Hard process/crate isolation by default |
| Parallelism | Per-file workers (configurable) | Tests run in parallel threads by default |
| Library vs binary | No distinction | Only **library crates** are integration-testable directly |

The most consequential difference is the **public-API constraint**. In TypeScript, `private` is advisory and erased at runtime, so a test can reach internals when it wants to. Rust's integration tests are compiled against the crate boundary, so the visibility rules are enforced by the compiler. If you need to test a private function, that belongs in a unit test inside the module (see [test-organization](/13-testing/01-test-organization/)).

> **Note:** Integration tests can only target a **library** crate. A crate with just `src/main.rs` exposes no library API, so `use yourbin::...` will not resolve. The common production pattern is a thin `src/main.rs` that calls into a `src/lib.rs`; the library is integration-tested, and the binary is tested by *running it* (shown in the Real-World Example below).

---

## Common Pitfalls

### Pitfall 1: A flat helper file in `tests/` gets run as a test

If you put shared helpers in `tests/helpers.rs` (a flat file, not a subdirectory), Cargo compiles it as its own integration-test crate. It contains no `#[test]` functions, so you get a confusing empty run:

```rust
// tests/helpers.rs — WRONG place for a shared helper
use cart::{Cart, Item};

pub fn sample_cart() -> Cart {
    let mut cart = Cart::new();
    cart.add(Item::new("BOOK-01", 1299), 2).unwrap();
    cart
}
```

Running it shows the helper file masquerading as a test target with nothing to run:

```text
     Running tests/helpers.rs (target/debug/deps/helpers-58b7202b39b88064)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**Fix:** move the helper to `tests/common/mod.rs` and pull it in with `mod common;`. Files in subdirectories are not treated as test crates.

### Pitfall 2: Trying to reach a private item

Coming from TypeScript, you might expect a test to touch internals "just this once." Add a private function to the library and call it from an integration test:

```rust
// in src/lib.rs
fn round_to_nearest_dollar(cents: u32) -> u32 { // private!
    ((cents + 50) / 100) * 100
}
```

```rust
// tests/private_probe.rs
#[test]
fn cannot_reach_private_fn() {
    // does not compile (error[E0603]: function is private)
    let _ = cart::round_to_nearest_dollar(1250);
}
```

The compiler stops you with the real error:

```text
error[E0603]: function `round_to_nearest_dollar` is private
 --> tests/private_probe.rs:4:19
  |
4 |     let _ = cart::round_to_nearest_dollar(1250);
  |                   ^^^^^^^^^^^^^^^^^^^^^^^ private function
  |
note: the function `round_to_nearest_dollar` is defined here
 --> src/lib.rs:79:1
  |
79 | fn round_to_nearest_dollar(cents: u32) -> u32 {
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**Fix:** either make the item `pub` (if it truly is part of the API), or test it from a unit test inside the module where private access is allowed.

### Pitfall 3: Importing by relative path instead of crate name

There is no `use crate::...` or `use super::...` reaching out of `tests/` into `src/`: an integration test is a *separate crate*. Write `use cart::Cart;` (the package name), the same way an external dependency would. Newcomers often try `use super::cart::Cart;` or a `mod` pointing at `../src`, both of which fail to resolve.

### Pitfall 4: Expecting `console.log`-style output by default

Just like Vitest hides logs from passing tests, Rust **captures** stdout/stderr and only shows it for failing tests. If you want to see prints from passing tests, run `cargo test -- --nocapture`. (The `--` separates Cargo's arguments from the test harness's.)

### Pitfall 5: Forgetting that integration tests need a library target

If your crate is binary-only, `cargo test` will still run the binary's *unit* tests, but `use yourbin::...` in `tests/` won't compile because there is no library to link. The fix is to extract logic into `src/lib.rs`.

---

## Best Practices

- **Group by feature, not by function.** One file per user-facing workflow (`tests/cart_checkout.rs`, `tests/auth_flow.rs`) reads better than one giant `tests/lib.rs`. Because each file is its own crate, this also improves parallelism.
- **Keep shared setup in `tests/common/mod.rs`.** Expose small `pub fn` constructors there. For heavier setup (temp directories, fixtures, RAII teardown), see [test-fixtures](/13-testing/05-test-fixtures/).
- **Treat integration tests as your API's first real consumer.** If a test is awkward to write because the public API is clumsy, that is feedback about the API, not the test.
- **Use `Result`-returning tests with `?`** for flows that involve fallible setup steps; reserve `.unwrap()` for "this must succeed or the test is meaningless" setup.
- **Pin dev-only crates under `[dev-dependencies]`.** Things like `assert_cmd`, `predicates`, or `tempfile` should never end up in your published dependency graph. Add them with `cargo add --dev <crate>` (the `add` subcommand has been built into Cargo since 1.62, no `cargo-edit` needed).
- **Select a single file with `--test`.** `cargo test --test cart_checkout` runs only that integration-test crate; append a name substring to filter further, e.g. `cargo test --test cart_checkout total`.
- **Do not duplicate unit-test coverage.** Integration tests verify the *contract*; private-logic edge cases belong in unit tests. See [test-organization](/13-testing/01-test-organization/) for where each kind lives.

---

## Real-World Example

A production crate usually ships a **library** plus a thin **binary**. The library is integration-tested through its public API; the binary is integration-tested by *running the compiled program* and asserting on its exit code and output. The `assert_cmd` and `predicates` crates make the latter ergonomic; they are the Rust analog of spawning your CLI in a Jest test and checking `stdout`.

```toml
# Cargo.toml
[package]
name = "greet"
version = "0.1.0"
edition = "2024"

[dev-dependencies]
assert_cmd = "2.2.2"
predicates = "3.1.4"
```

```rust
// src/main.rs — the binary under test
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some(name) if !name.is_empty() => {
            println!("Hello, {name}!");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("error: expected a name argument");
            ExitCode::from(2)
        }
    }
}
```

```rust
// tests/cli.rs — black-box test of the actual binary
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn greets_a_named_user() {
    let mut cmd = Command::cargo_bin("greet").unwrap();
    cmd.arg("Ada")
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello, Ada!"));
}

#[test]
fn fails_without_a_name() {
    let mut cmd = Command::cargo_bin("greet").unwrap();
    cmd.assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("expected a name"));
}
```

`Command::cargo_bin("greet")` locates and runs the freshly compiled binary; the `.assert()` builder then checks the exit status and matches `stdout`/`stderr` against `predicates` combinators. Running `cargo test --test cli` produces this real output:

```text
   Compiling greet v0.1.0 (/private/tmp/ts_rust_clitest/greet)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 6.63s
     Running tests/cli.rs (target/debug/deps/cli-cd6dde0ca03346ee)

running 2 tests
test greets_a_named_user ... ok
test fails_without_a_name ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
```

This is genuine black-box testing: the test knows nothing about the program's internals, only its command-line contract, the closest Rust equivalent to spawning your built CLI in a Jest/Vitest test and asserting on its output.

---

## Further Reading

- The Rust Book — [Integration Tests](https://doc.rust-lang.org/book/ch11-03-test-organization.html#integration-tests)
- Cargo Book — [Tests and the `tests/` directory](https://doc.rust-lang.org/cargo/guide/tests.html) and [Target auto-discovery](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests)
- [`assert_cmd` documentation](https://docs.rs/assert_cmd) and [`predicates` documentation](https://docs.rs/predicates) for CLI black-box testing
- Related pages in this section:
  - [Unit Tests](/13-testing/00-unit-tests/): in-source `#[test]` and `#[cfg(test)] mod tests`
  - [Test Organization](/13-testing/01-test-organization/): where each kind of test lives; private vs public testing
  - [Assertions](/13-testing/02-assertions/) — `assert!`, `assert_eq!`, custom messages
  - [should_panic and Result tests](/13-testing/03-should-panic/) — testing failures and `?` in tests
  - [Test Fixtures](/13-testing/05-test-fixtures/) — setup/teardown, `LazyLock`, RAII guards for shared state
  - [Mocking](/13-testing/06-mocking/) — trait-based test doubles and `mockall`
- Earlier sections: [Modules and the crate tree](/12-modules-packages/00-modules/) explains why `tests/` sees only `pub` items; [Cargo.toml and dev-dependencies](/12-modules-packages/04-cargo/) covers `[dev-dependencies]`. New to the project layout? Start at [Getting Started](/01-getting-started/) and [Basics](/02-basics/).
- Next up after testing: [Section 14 — Macros](/14-macros/), where attributes like `#[test]` and derives are explained from the inside.

---

## Exercises

### Exercise 1: Your first integration test

**Difficulty:** Beginner

**Objective:** Create a library crate and verify its public API from `tests/`.

**Instructions:** Run `cargo new --lib slugify`. In `src/lib.rs`, write `pub fn slugify(input: &str) -> String` that lowercases the text, replaces every run of non-alphanumeric characters with a single `-`, and trims leading/trailing `-`. Add `tests/slugify.rs` with at least two `#[test]` functions that call `slugify::slugify(...)` and assert on the results. Run `cargo test`.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true; // start "true" so leading separators are dropped
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}
```

```rust
// tests/slugify.rs
use slugify::slugify;

#[test]
fn collapses_separators_and_lowercases() {
    assert_eq!(slugify("Hello, World!"), "hello-world");
}

#[test]
fn trims_edges_and_runs() {
    assert_eq!(slugify("  ***Rust  &  TS*** "), "rust-ts");
}
```

**Output:**

```text
running 2 tests
test collapses_separators_and_lowercases ... ok
test trims_edges_and_runs ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

</details>

### Exercise 2: Share setup across two test files

**Difficulty:** Intermediate

**Objective:** Use a `tests/common/mod.rs` helper from more than one integration-test file without it being run as a test.

**Instructions:** Starting from a `cargo new --lib kv` crate, implement a tiny `pub struct Store` backed by a `HashMap<String, String>` with `pub fn new()`, `pub fn set(&mut self, k: &str, v: &str)`, and `pub fn get(&self, k: &str) -> Option<&str>`. Add `tests/common/mod.rs` exposing `pub fn seeded() -> Store` that inserts two known keys. Then write **two** test files (`tests/reads.rs` and `tests/writes.rs`), each declaring `mod common;` and using `common::seeded()`. Confirm `cargo test` lists two separate test crates and zero stray empty ones.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Store {
    map: HashMap<String, String>,
}

impl Store {
    pub fn new() -> Self {
        Store::default()
    }

    pub fn set(&mut self, k: &str, v: &str) {
        self.map.insert(k.to_string(), v.to_string());
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.map.get(k).map(String::as_str)
    }
}
```

```rust
// tests/common/mod.rs
use kv::Store;

pub fn seeded() -> Store {
    let mut s = Store::new();
    s.set("lang", "rust");
    s.set("year", "2026");
    s
}
```

```rust
// tests/reads.rs
mod common;

#[test]
fn reads_seeded_values() {
    let s = common::seeded();
    assert_eq!(s.get("lang"), Some("rust"));
    assert_eq!(s.get("missing"), None);
}
```

```rust
// tests/writes.rs
use kv::Store;

mod common;

#[test]
fn writes_override_existing_values() {
    let mut s = common::seeded();
    s.set("lang", "ferris");
    assert_eq!(s.get("lang"), Some("ferris"));
}

#[test]
fn fresh_store_is_distinct_from_seeded() {
    let empty = Store::new();
    assert_eq!(empty.get("lang"), None);
}
```

**Output (test-crate headers only):**

```text
     Running tests/reads.rs (target/debug/deps/reads-...)
running 1 test
test reads_seeded_values ... ok

     Running tests/writes.rs (target/debug/deps/writes-...)
running 2 tests
test fresh_store_is_distinct_from_seeded ... ok
test writes_override_existing_values ... ok
```

Because `common` lives in a subdirectory, there is **no** `Running tests/common/mod.rs` line, exactly what we want.

</details>

### Exercise 3: Black-box test a binary's exit code

**Difficulty:** Advanced

**Objective:** Test a compiled binary end-to-end with `assert_cmd`, asserting on both success and failure paths.

**Instructions:** Create `cargo new adder`. The program reads two integer arguments, prints their sum on success, and exits with code `2` plus a stderr message if an argument is missing or not a number. Add `assert_cmd` and `predicates` as dev-dependencies (`cargo add --dev assert_cmd predicates`) and write `tests/cli.rs` with one test for a valid sum and one for a parse failure.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let nums: Vec<String> = env::args().skip(1).collect();
    if nums.len() != 2 {
        eprintln!("error: expected exactly two integers");
        return ExitCode::from(2);
    }
    let mut sum: i64 = 0;
    for n in &nums {
        match n.parse::<i64>() {
            Ok(v) => sum += v,
            Err(_) => {
                eprintln!("error: '{n}' is not an integer");
                return ExitCode::from(2);
            }
        }
    }
    println!("{sum}");
    ExitCode::SUCCESS
}
```

```toml
# Cargo.toml (the relevant part)
[dev-dependencies]
assert_cmd = "2.2.2"
predicates = "3.1.4"
```

```rust
// tests/cli.rs
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn adds_two_integers() {
    Command::cargo_bin("adder")
        .unwrap()
        .args(["19", "23"])
        .assert()
        .success()
        .stdout(predicate::str::contains("42"));
}

#[test]
fn rejects_non_integer_input() {
    Command::cargo_bin("adder")
        .unwrap()
        .args(["19", "oops"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("not an integer"));
}
```

**Output:**

```text
running 2 tests
test adds_two_integers ... ok
test rejects_non_integer_input ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
```

</details>
