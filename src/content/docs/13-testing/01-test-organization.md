---
title: "Test Organization"
description: "Rust puts unit tests in a #[cfg(test)] mod tests beside the code so they reach private items, while tests/ integration files, like Jest, see only the public API."
---

In TypeScript and JavaScript, tests almost always live in separate files that import the code under test. Rust takes a different default: small unit tests live *inside* the same file as the code they exercise, in a dedicated `tests` submodule. This file explains where Rust tests go, why the `#[cfg(test)] mod tests` convention exists, and how it enables something Jest and Vitest cannot easily do: testing private functions directly.

---

## Quick Overview

Rust recognizes two kinds of tests and gives each a home:

- **Unit tests** live in the same file as the code, in a `#[cfg(test)] mod tests` submodule. Because they are *inside* the crate, they can reach **private** functions and types.
- **Integration tests** live in a top-level `tests/` directory. Each file there is compiled as a separate crate that links your library from the outside, so it can only touch the **public** API.

> **Note:** This file focuses on *where tests live* and *what they can see*: the module layout and the private-versus-public distinction. The mechanics of writing a `#[test]` and running `cargo test` are covered in [Unit Tests](/13-testing/00-unit-tests/), and the `tests/` directory gets a full treatment in [Integration Tests](/13-testing/04-integration-tests/).

---

## TypeScript/JavaScript Example

In a TypeScript project using Jest or Vitest, tests live in their own files. The two most common conventions are co-located test files next to the source, or a parallel `__tests__` directory.

```typescript
// src/username.ts — the code under test
export class UsernameError extends Error {}

// Exported: part of the public API.
export function parseUsername(raw: string): string {
  const trimmed = raw.trim();
  checkLength(trimmed);
  const bad = firstInvalidChar(trimmed);
  if (bad !== null) {
    throw new UsernameError(`invalid character: '${bad}'`);
  }
  return trimmed.toLowerCase();
}

// NOT exported: a private helper, invisible outside this module.
function checkLength(s: string): void {
  if (s.length < 3) throw new UsernameError("too short");
  if (s.length > 20) throw new UsernameError("too long");
}

function firstInvalidChar(s: string): string | null {
  for (const c of s) {
    if (!/[a-z0-9_]/i.test(c)) return c;
  }
  return null;
}
```

```typescript
// src/username.test.ts — the test file, a SEPARATE module
import { parseUsername, UsernameError } from "./username";

describe("parseUsername", () => {
  it("normalizes valid input", () => {
    expect(parseUsername("  AdaLovelace ")).toBe("adalovelace");
  });

  it("rejects bad characters", () => {
    expect(() => parseUsername("ada lovelace")).toThrow(UsernameError);
  });
});

// You CANNOT test `checkLength` or `firstInvalidChar` here:
// they are not exported, so `import` cannot reach them. To test them
// you would either export them (widening your public API for tests'
// sake) or reach for a hack like `rewire` / `babel-plugin-rewire`.
```

**Key points about the TypeScript layout:**

- Tests are a separate file/module that `import`s the code.
- The test file can only see **exported** (`export`) members.
- Private functions are genuinely unreachable from tests unless you export them or use a module-internals hack.
- The test runner (Jest/Vitest) discovers files by a glob like `**/*.test.ts` configured in `jest.config.js` / `vitest.config.ts`.

---

## Rust Equivalent

In Rust, the unit tests for `username.rs` live *in the same file*, inside a child module annotated with `#[cfg(test)]`. That module can reach the private helpers directly via `use super::*`.

```rust
// src/lib.rs — code and its unit tests in ONE file

/// A validated, normalized username.
#[derive(Debug, PartialEq, Eq)]
pub struct Username(String);

/// Errors that can occur while validating a username.
#[derive(Debug, PartialEq, Eq)]
pub enum UsernameError {
    TooShort,
    TooLong,
    InvalidChar(char),
}

impl Username {
    /// Build a `Username` from raw input, enforcing the rules. (public API)
    pub fn parse(raw: &str) -> Result<Username, UsernameError> {
        let trimmed = raw.trim();
        check_length(trimmed)?;
        if let Some(bad) = first_invalid_char(trimmed) {
            return Err(UsernameError::InvalidChar(bad));
        }
        Ok(Username(trimmed.to_lowercase()))
    }

    /// Borrow the normalized value. (public API)
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// --- Private helpers: implementation details, not part of the public API. ---

fn check_length(s: &str) -> Result<(), UsernameError> {
    match s.chars().count() {
        0..=2 => Err(UsernameError::TooShort),
        3..=20 => Ok(()),
        _ => Err(UsernameError::TooLong),
    }
}

fn first_invalid_char(s: &str) -> Option<char> {
    s.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '_'))
}

#[cfg(test)]
mod tests {
    // Pull the PARENT module's items into scope — including private ones.
    use super::*;

    #[test]
    fn accepts_and_normalizes_valid_input() {
        let name = Username::parse("  AdaLovelace  ").unwrap();
        assert_eq!(name.as_str(), "adalovelace");
    }

    #[test]
    fn rejects_bad_characters() {
        assert_eq!(
            Username::parse("ada lovelace"),
            Err(UsernameError::InvalidChar(' ')),
        );
    }

    // Testing a PRIVATE helper directly — no need to make it public.
    #[test]
    fn length_boundaries() {
        assert_eq!(check_length("ab"), Err(UsernameError::TooShort));
        assert_eq!(check_length("abc"), Ok(()));
        assert_eq!(check_length(&"x".repeat(21)), Err(UsernameError::TooLong));
    }

    #[test]
    fn finds_first_invalid_char() {
        assert_eq!(first_invalid_char("ok_name"), None);
        assert_eq!(first_invalid_char("no!"), Some('!'));
    }
}
```

Running `cargo test` produces:

```text
running 4 tests
test tests::rejects_bad_characters ... ok
test tests::length_boundaries ... ok
test tests::finds_first_invalid_char ... ok
test tests::accepts_and_normalizes_valid_input ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Notice that `check_length` and `first_invalid_char` are private (no `pub`), yet the tests call them. This is the headline difference from TypeScript: **co-located unit tests can test private code without weakening your public API.**

---

## Detailed Explanation

### The `tests` submodule, line by line

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_normalizes_valid_input() { /* ... */ }
}
```

- **`mod tests { ... }`** declares a child module. The name `tests` is a strong convention, not a keyword. The compiler does not care what you call it, but every Rust project you read will use `tests`. Because it is a child module, it sits *below* the code in the module tree, which is what gives it access to the parent's private items.
- **`#[cfg(test)]`** is a **conditional compilation** attribute. `cfg` means "configuration", and `test` is a flag Cargo sets only when building the test harness. So the module (and everything in it) is compiled *only* during `cargo test` and completely vanishes from `cargo build` and `cargo run`. Your release binary carries zero test code and zero test dependencies.
- **`use super::*;`** imports everything from the **parent** module (`super` = "one level up") into the test module. Without it, you would have to write `super::Username::parse(...)` and `super::check_length(...)` on every line. The glob `*` is idiomatic *here specifically* because test modules are small and tightly coupled to their parent. In normal application code, glob imports are discouraged.
- **`#[test]`** marks a function as a test case. See [Unit Tests](/13-testing/00-unit-tests/) for the full story on `#[test]`.

### Why "inside the file" instead of "next to it"?

This is the part that feels backwards coming from TypeScript. Rust's module system makes child modules privileged: **a child module can see all of its ancestors' private items, but not vice versa.** By placing tests in a child module, you grant them access to internals *for free*, without exporting anything.

> **Tip:** Think of `#[cfg(test)] mod tests` as a private workshop bolted onto each source file. It is part of the crate, so it sees everything, but it is sealed off (`cfg(test)`) so it never ships.

### Visibility recap: who can see what

| Item visibility | Unit test (`#[cfg(test)] mod tests`) | Integration test (`tests/` dir) |
| --------------- | ------------------------------------ | ------------------------------- |
| `fn foo` | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes (it is an ancestor's item) | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> No — private |
| `pub(crate) fn` | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> No — not visible outside crate |
| `pub fn foo` | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes |

The `pub(crate)` row matters: an item that is public *within* the crate is reachable by unit tests (same crate) but invisible to integration tests (separate crate). For more on `pub`, `pub(crate)`, and the module tree, see [Modules](/12-modules-packages/00-modules/).

### Where tests physically live

| Test kind          | Location                                  | Compiled as           | Sees private code? |
| ------------------ | ----------------------------------------- | --------------------- | ------------------ |
| Unit tests | same file, in `#[cfg(test)] mod tests` | part of your crate | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes |
| Integration tests | `tests/*.rs` (top-level `tests/` dir) | one crate per file | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Public API only |
| Documentation tests| inside `///` doc comments | one crate per example | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Public API only |

This file is about the first row; [Integration Tests](/13-testing/04-integration-tests/) and [Documentation Tests](/13-testing/09-doc-tests/) cover the others.

---

## Key Differences

### 1. Co-location vs. separate files

| Aspect                  | TypeScript (Jest/Vitest)                     | Rust                                            |
| ----------------------- | -------------------------------------------- | ----------------------------------------------- |
| Default unit-test home  | Separate file (`*.test.ts` / `__tests__/`)   | Same file, in `#[cfg(test)] mod tests`          |
| Test discovery          | Config glob (`testMatch` / `include`)        | Built into `cargo`; no config needed            |
| Stripped from prod build| Bundler/`tsconfig` excludes test files       | `#[cfg(test)]` excludes at compile time         |
| Reaches private members | No (unless exported or a rewire hack)        | Yes (child module sees ancestor's privates)     |

### 2. The compiler enforces the prod/test split

In TypeScript, keeping test code out of your shipped bundle is a *build configuration* concern: forget to exclude `*.test.ts` and it ships. In Rust, `#[cfg(test)]` is a *compiler* feature: code behind it does not exist in non-test builds, so it cannot accidentally leak into production. Test-only dependencies go in `[dev-dependencies]` in `Cargo.toml` and are likewise absent from release builds.

### 3. Tests follow the module tree

Because each source file is (or contains) a module, your tests naturally organize themselves the same way your code does. A crate split into `money` and `cart` modules ends up with `money::tests` and `cart::tests`, and the test names in the output reflect that path:

```rust
// src/lib.rs
pub mod money;
pub mod cart;
```

```rust
// src/money.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cents(pub u64);

impl Cents {
    /// Apply a whole-percent discount, rounding down.
    pub fn discounted(self, percent: u8) -> Cents {
        let kept = 100u64.saturating_sub(percent as u64);
        Cents(self.0 * kept / 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_percent_off() {
        assert_eq!(Cents(1000).discounted(10), Cents(900));
    }

    #[test]
    fn rounds_down() {
        assert_eq!(Cents(999).discounted(10), Cents(899));
    }
}
```

```rust
// src/cart.rs
use crate::money::Cents;

/// Sum the line items in a cart.
pub fn subtotal(line_items: &[Cents]) -> Cents {
    Cents(line_items.iter().map(|c| c.0).sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_line_items() {
        let items = [Cents(500), Cents(250), Cents(125)];
        assert_eq!(subtotal(&items), Cents(875));
    }

    #[test]
    fn empty_cart_is_zero() {
        assert_eq!(subtotal(&[]), Cents(0));
    }
}
```

`cargo test` reports each test under its module path, so you always know where a failing test lives:

```text
running 4 tests
test cart::tests::empty_cart_is_zero ... ok
test cart::tests::sums_line_items ... ok
test money::tests::rounds_down ... ok
test money::tests::ten_percent_off ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

> **Tip:** You can run just one module's tests with a filter: `cargo test money::` runs only `money::tests::*`. The filter is a substring match against the full test path.

### 4. Binary crates test the same way

A binary crate (`src/main.rs`) uses the identical pattern — a `#[cfg(test)] mod tests` with `use super::*` works in `main.rs` just as it does in `lib.rs`:

```rust playground
// src/main.rs
fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

fn main() {
    println!("{}", celsius_to_fahrenheit(100.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boiling_point() {
        assert_eq!(celsius_to_fahrenheit(100.0), 212.0);
    }

    #[test]
    fn freezing_point() {
        assert_eq!(celsius_to_fahrenheit(0.0), 32.0);
    }
}
```

```text
running 2 tests
test tests::boiling_point ... ok
test tests::freezing_point ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

> **Note:** Integration tests in `tests/` can only link a **library** crate (`src/lib.rs`), not a binary's internals. A common production layout is to keep all logic in `src/lib.rs` and make `src/main.rs` a thin wrapper that calls into it. That way both unit tests *and* integration tests can exercise the real code. See [Integration Tests](/13-testing/04-integration-tests/).

---

## Common Pitfalls

### Pitfall 1: Trying to test a private function from `tests/`

A TypeScript developer's instinct is "tests go in a separate place." If you put a test that needs internals into the `tests/` directory, it is a separate crate and can only see your public API.

```rust
// tests/api.rs — an EXTERNAL crate linking your library
#[test]
fn cannot_reach_private_helper() {
    // does not compile (error[E0603]: function `collapse_separators` is private)
    let _ = my_crate::collapse_separators("a  b");
}
```

The real error from `cargo test --test api`:

```text
error[E0603]: function `collapse_separators` is private
  --> tests/api.rs:8:20
   |
 8 |     let _ = probe::collapse_separators("a  b");
   |                    ^^^^^^^^^^^^^^^^^^^ private function
   |
note: the function `collapse_separators` is defined here
  --> src/lib.rs:10:1
   |
10 | fn collapse_separators(input: &str) -> String {
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**Fix:** put tests that need internals in a `#[cfg(test)] mod tests` *inside* the source file. Keep `tests/` for black-box testing of the public API.

### Pitfall 2: Forgetting `use super::*`

The test module is a *child*; it does not automatically see the parent's items. Omit the import and you get unresolved-name errors:

```rust
pub fn double(n: i32) -> i32 {
    n * 2
}

#[cfg(test)]
mod tests {
    // Missing: use super::*;
    #[test]
    fn doubles() {
        // does not compile (error[E0425]: cannot find function `double` in this scope)
        assert_eq!(double(21), 42);
    }
}
```

**Fix:** add `use super::*;` as the first line inside `mod tests`.

### Pitfall 3: A test helper placed outside `#[cfg(test)]`

If you write a helper function used *only* by tests but forget to gate it behind `#[cfg(test)]`, the compiler includes it in normal builds and warns that it is unused dead code:

```rust
pub fn double(n: i32) -> i32 {
    n * 2
}

// Helper only used by tests, but NOT gated behind #[cfg(test)].
fn test_input() -> i32 {
    21
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn doubles() {
        assert_eq!(double(test_input()), 42);
    }
}
```

A plain `cargo build` (not `cargo test`) reports:

```text
warning: function `test_input` is never used
 --> src/lib.rs:6:4
  |
6 | fn test_input() -> i32 {
  |    ^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` on by default

warning: `probe` (lib) generated 1 warning
```

**Fix:** put test-only helpers *inside* the `#[cfg(test)] mod tests` block (or in another `#[cfg(test)]`-gated module). Anything inside the test module is automatically excluded from non-test builds, so it never triggers a dead-code warning.

### Pitfall 4: Expecting Jest-style config and file globs

There is no `cargo` equivalent of `jest.config.js` or `testMatch`. Cargo discovers tests by convention: `#[test]` functions anywhere in the crate, every file in `tests/`, and doc examples. You do not register them or configure a glob. If a test is not running, the cause is almost always a missing `#[test]` attribute or a `mod` that is not declared, not a misconfigured matcher.

---

## Best Practices

- **Default to a `#[cfg(test)] mod tests` at the bottom of each source file.** Keep the tests next to the code they verify so they move, rename, and get deleted together.
- **Always start the module with `use super::*;`.** It is the one place a glob import is idiomatic in Rust.
- **Test private functions where it pays off.** You *can* reach internals — use that to test tricky helpers in isolation. But prefer testing through the public API when you can, so tests survive internal refactors.
- **Keep test-only code inside the `cfg(test)` boundary.** Helpers, fixtures, fake data builders — all of it goes behind `#[cfg(test)]` so it never bloats your release build. For shared setup, see [Test Fixtures](/13-testing/05-test-fixtures/).
- **Mirror your module structure.** Let each module own its own `tests` submodule rather than collecting everything into one giant test file. The module-path test names keep failures easy to locate.
- **Push logic into `src/lib.rs` and keep `src/main.rs` thin.** This lets both unit tests and `tests/` integration tests exercise the same code. See [Integration Tests](/13-testing/04-integration-tests/).
- **Put test-only dependencies in `[dev-dependencies]`.** Crates like `proptest` or `mockall` belong there, not in `[dependencies]`, so they are absent from production builds.

---

## Real-World Example

A common production pattern: a library crate where each module carries its own unit tests for private invariants, plus a `tests/` directory that exercises only the public API. Here is the full layout of an `inventory` crate.

```text
inventory/
├── Cargo.toml
├── src/
│   ├── lib.rs        # module declarations + public re-exports
│   ├── sku.rs        # SKU validation (private parser + unit tests)
│   └── warehouse.rs  # stock logic (unit tests for private helpers)
└── tests/
    └── public_api.rs # black-box tests against the public API only
```

```rust
// src/lib.rs
//! Inventory management.

pub mod sku;
pub mod warehouse;

// Re-export the headline types so callers write `inventory::Sku`.
pub use sku::Sku;
pub use warehouse::Warehouse;
```

```rust
// src/sku.rs
/// A validated stock-keeping unit, e.g. "ABC-12345".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sku(String);

impl Sku {
    /// Parse a raw SKU string. (public API)
    pub fn parse(raw: &str) -> Option<Sku> {
        let normalized = raw.trim().to_uppercase();
        if is_well_formed(&normalized) {
            Some(Sku(normalized))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Private: the SKU grammar. Tested directly below.
fn is_well_formed(s: &str) -> bool {
    let (prefix, digits) = match s.split_once('-') {
        Some(parts) => parts,
        None => return false,
    };
    prefix.len() == 3
        && prefix.chars().all(|c| c.is_ascii_uppercase())
        && digits.len() == 5
        && digits.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_normalizes_case_and_whitespace() {
        let sku = Sku::parse("  abc-12345 ").unwrap();
        assert_eq!(sku.as_str(), "ABC-12345");
    }

    #[test]
    fn parse_rejects_malformed_input() {
        assert_eq!(Sku::parse("AB-12345"), None); // prefix too short
        assert_eq!(Sku::parse("ABC-1234"), None); // too few digits
        assert_eq!(Sku::parse("ABCD12345"), None); // missing dash
    }

    // Unit-test the PRIVATE grammar directly.
    #[test]
    fn well_formed_grammar() {
        assert!(is_well_formed("XYZ-00001"));
        assert!(!is_well_formed("xyz-00001")); // lowercase rejected pre-normalization
        assert!(!is_well_formed("XYZ-0000A")); // non-digit
    }
}
```

```rust
// src/warehouse.rs
use crate::sku::Sku;
use std::collections::HashMap;

/// Tracks on-hand quantity per SKU.
#[derive(Debug, Default)]
pub struct Warehouse {
    stock: HashMap<String, u32>,
}

impl Warehouse {
    pub fn new() -> Warehouse {
        Warehouse::default()
    }

    /// Add `qty` units of `sku`. (public API)
    pub fn receive(&mut self, sku: &Sku, qty: u32) {
        let entry = self.stock.entry(sku.as_str().to_string()).or_insert(0);
        *entry = saturating_total(*entry, qty);
    }

    /// Current quantity on hand. (public API)
    pub fn quantity(&self, sku: &Sku) -> u32 {
        self.stock.get(sku.as_str()).copied().unwrap_or(0)
    }
}

// Private helper: total that never overflows u32.
fn saturating_total(current: u32, added: u32) -> u32 {
    current.saturating_add(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sku(s: &str) -> Sku {
        Sku::parse(s).expect("test SKU should be valid")
    }

    #[test]
    fn receiving_accumulates() {
        let mut wh = Warehouse::new();
        let item = sku("ABC-12345");
        wh.receive(&item, 10);
        wh.receive(&item, 5);
        assert_eq!(wh.quantity(&item), 15);
    }

    #[test]
    fn private_total_saturates() {
        assert_eq!(saturating_total(u32::MAX, 1), u32::MAX);
    }
}
```

```rust
// tests/public_api.rs — separate crate; PUBLIC API only.
use inventory::{Sku, Warehouse};

#[test]
fn end_to_end_receiving_flow() {
    let mut wh = Warehouse::new();
    let widget = Sku::parse("WID-00042").expect("valid SKU");

    wh.receive(&widget, 3);
    wh.receive(&widget, 7);

    assert_eq!(wh.quantity(&widget), 10);

    // `is_well_formed` and `saturating_total` are private and simply
    // do not exist from out here — this crate cannot name them.
}
```

The unit tests in `sku.rs` and `warehouse.rs` lock down the private parsing grammar and the saturating arithmetic: internal invariants a black-box test could never reach. The `tests/public_api.rs` file verifies the *contract* a consumer of the crate sees. Together they give you fine-grained coverage of internals and confidence that the public surface behaves, without ever widening the API for the sake of testability.

> **Note:** The helper `fn sku(...)` inside `warehouse::tests` is a fixture-style constructor. Because it lives inside the `#[cfg(test)]` module, it is compiled only for tests and never warns as unused. Setup and teardown patterns like this are explored further in [Test Fixtures](/13-testing/05-test-fixtures/).

---

## Further Reading

- [The Rust Book — Test Organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html) — the canonical explanation of unit vs. integration tests.
- [The Rust Book — How to Write Tests](https://doc.rust-lang.org/book/ch11-01-writing-tests.html) — the `#[test]` and `#[cfg(test)]` basics.
- [The Rust Reference — Conditional compilation (`cfg`)](https://doc.rust-lang.org/reference/conditional-compilation.html) — what `#[cfg(test)]` actually does.
- [Cargo Book — Tests](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#tests) — how Cargo discovers and builds test targets.

**Related sections of this guide:**

- [Unit Tests](/13-testing/00-unit-tests/): writing `#[test]` functions and running `cargo test`.
- [Integration Tests](/13-testing/04-integration-tests/): the `tests/` directory and black-box testing.
- [Documentation Tests](/13-testing/09-doc-tests/): runnable examples in `///` comments.
- [Test Fixtures](/13-testing/05-test-fixtures/): setup/teardown and shared test state.
- [Modules](/12-modules-packages/00-modules/): visibility, `pub`/`pub(crate)`, and the module tree that this whole convention rests on.
- [Cargo and Crates](/12-modules-packages/04-cargo/): `[dev-dependencies]` and crate structure.
- [Macros](/14-macros/): how attributes like `#[test]` and `#[cfg(...)]` fit into Rust's macro and attribute system.

---

## Exercises

### Exercise 1: Move tests into a `tests` submodule

**Difficulty:** Easy

**Objective:** Practice the basic `#[cfg(test)] mod tests` layout and the `use super::*` import.

**Instructions:** You are given a function and a loose `#[test]` function. Wrap the test in a proper `#[cfg(test)] mod tests` submodule so it is excluded from non-test builds and follows convention.

```rust
pub fn is_palindrome(s: &str) -> bool {
    let cleaned: String = s.chars().filter(|c| c.is_alphanumeric()).collect();
    let lowered = cleaned.to_lowercase();
    lowered.chars().eq(lowered.chars().rev())
}

// TODO: move this into a proper `#[cfg(test)] mod tests` submodule.
#[test]
fn detects_palindrome() {
    assert!(is_palindrome("Race car"));
}
```

<details>
<summary>Solution</summary>

```rust
pub fn is_palindrome(s: &str) -> bool {
    let cleaned: String = s.chars().filter(|c| c.is_alphanumeric()).collect();
    let lowered = cleaned.to_lowercase();
    lowered.chars().eq(lowered.chars().rev())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_palindrome() {
        assert!(is_palindrome("Race car"));
    }

    #[test]
    fn rejects_non_palindrome() {
        assert!(!is_palindrome("hello"));
    }
}
```

Running `cargo test`:

```text
running 2 tests
test tests::detects_palindrome ... ok
test tests::rejects_non_palindrome ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

</details>

### Exercise 2: Test a private function

**Difficulty:** Medium

**Objective:** Exploit the child-module privilege to test an internal helper that is *not* part of the public API.

**Instructions:** The crate exposes `public fn normalize_phone` but the digit-extraction logic lives in a private `digits_only` helper. Add a `#[cfg(test)] mod tests` that tests **both** the public function and the private `digits_only` helper directly. Do **not** add `pub` to `digits_only`.

```rust
/// Strip a phone number down to its digits and format it. (public API)
pub fn normalize_phone(raw: &str) -> Option<String> {
    let digits = digits_only(raw);
    if digits.len() == 10 {
        Some(format!(
            "({}) {}-{}",
            &digits[0..3],
            &digits[3..6],
            &digits[6..10]
        ))
    } else {
        None
    }
}

// Private helper.
fn digits_only(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_digit()).collect()
}

// TODO: add a tests submodule that tests BOTH functions.
```

<details>
<summary>Solution</summary>

```rust
/// Strip a phone number down to its digits and format it. (public API)
pub fn normalize_phone(raw: &str) -> Option<String> {
    let digits = digits_only(raw);
    if digits.len() == 10 {
        Some(format!(
            "({}) {}-{}",
            &digits[0..3],
            &digits[3..6],
            &digits[6..10]
        ))
    } else {
        None
    }
}

fn digits_only(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_digit()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_valid_number() {
        assert_eq!(
            normalize_phone("415-867-5309"),
            Some("(415) 867-5309".to_string())
        );
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(normalize_phone("12345"), None);
    }

    // Testing the PRIVATE helper directly — only possible from inside the crate.
    #[test]
    fn digits_only_strips_punctuation() {
        assert_eq!(digits_only("(415) 867-5309"), "4158675309");
        assert_eq!(digits_only("no digits here"), "");
    }
}
```

Running `cargo test`:

```text
running 3 tests
test tests::digits_only_strips_punctuation ... ok
test tests::formats_valid_number ... ok
test tests::rejects_wrong_length ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

</details>

### Exercise 3: Split unit tests and integration tests

**Difficulty:** Hard

**Objective:** Decide which test belongs where, given the public/private boundary, and lay out a crate accordingly.

**Instructions:** Build a `temperature` library crate with:

1. A public `c_to_f(celsius: f64) -> f64`.
2. A **private** `round1(x: f64) -> f64` that rounds to one decimal place.
3. A public `c_to_f_rounded(celsius: f64) -> f64` that combines them.

Then write:

- **Unit tests** (in `src/lib.rs`) that cover `c_to_f` *and* the private `round1`.
- An **integration test** (in `tests/public_api.rs`) that exercises `c_to_f_rounded` from the outside. Confirm the integration test cannot name `round1`.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
//! Temperature conversion library.

/// Convert Celsius to Fahrenheit. (public API)
pub fn c_to_f(celsius: f64) -> f64 {
    celsius * 9.0 / 5.0 + 32.0
}

/// Round to one decimal place. (private helper)
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Convert and round in one step. (public API)
pub fn c_to_f_rounded(celsius: f64) -> f64 {
    round1(c_to_f(celsius))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boiling() {
        assert_eq!(c_to_f(100.0), 212.0);
    }

    // Testing the PRIVATE helper directly.
    #[test]
    fn rounds_to_one_place() {
        assert_eq!(round1(98.347), 98.3);
    }
}
```

```rust
// tests/public_api.rs — separate crate, PUBLIC API only.
#[test]
fn rounded_conversion_via_public_api() {
    assert_eq!(temperature::c_to_f_rounded(37.0), 98.6);
}

// Uncommenting the next line fails to compile with
// error[E0603]: function `round1` is private — the integration crate
// cannot see private items.
// let _ = temperature::round1(1.23);
```

Running `cargo test` runs both the unit tests and the integration test:

```text
running 2 tests
test tests::boiling ... ok
test tests::rounds_to_one_place ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 1 test
test rounded_conversion_via_public_api ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

The private `round1` is verified by a unit test inside the crate, while the public `c_to_f_rounded` is verified by a black-box integration test. That is the canonical division of labor: **unit tests for internals, integration tests for the contract.**

</details>
