---
title: "Documentation Tests"
description: "Rust compiles and runs the examples in /// doc comments via cargo test, so they cannot rot like an inert JSDoc @example. Covers hidden lines, ?"
---

In TypeScript you might put a usage example in a JSDoc `@example` block, but that text is never executed — it can rot the moment you rename a parameter. Rust takes the opposite stance: the code examples you write in `///` doc comments are *compiled and run* by `cargo test`. Your documentation and your tests become the same artifact, and an out-of-date example is a build failure.

---

## Quick Overview

A **documentation test** (or **doc test**) is a fenced code block written inside a Rust documentation comment (`///` on an item, or `//!` at the top of a module). When you run `cargo test`, `rustdoc` extracts each block, wraps it in a hidden `fn main`, compiles it as a tiny standalone program, and runs it. This guarantees that every example in your published docs actually works against the current public Application Programming Interface (API). There is no equivalent in Jest or Vitest — a JSDoc `@example` is inert prose.

---

## TypeScript/JavaScript Example

A common JavaScript pattern is to document a function with a JSDoc `@example`. It looks helpful, but nothing checks it.

```typescript
// money.ts

/**
 * Formats an amount given in integer cents as a currency string.
 *
 * @example
 * formatCents(199); // => "$1.99"   <-- this comment is NEVER executed
 */
export function formatCents(cents: number): string {
  const dollars = Math.floor(cents / 100);
  const remainder = String(cents % 100).padStart(2, "0");
  return `$${dollars}.${remainder}`;
}
```

The `@example` block is just text inside a comment. If you later change `formatCents` to take a `{ cents }` object, the example still *says* `formatCents(199)` and nobody is warned. The only way to verify documentation in the JavaScript world is to write a *separate* test that happens to mirror the docs:

```typescript
// money.test.ts
import { describe, it, expect } from "vitest";
import { formatCents } from "./money";

describe("formatCents", () => {
  it("matches the documented example", () => {
    expect(formatCents(199)).toBe("$1.99");
  });
});
```

Now you maintain the truth in two places — the doc comment and the test — and they can silently drift apart. Tools like `eslint-plugin-jsdoc` or `tsdoc` can lint the *shape* of a JSDoc comment, but they do not run the code inside `@example`.

> **Note:** Running `money.ts` directly prints `$1.99` and `$123.00` from real `console.log` calls — but the `@example` line in the comment contributes nothing to that output. It is documentation, not behavior.

---

## Rust Equivalent

In Rust, the example *is* the test. Here is the same function as a library crate. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically.

```rust
// src/lib.rs

/// Formats an amount given in integer cents as a currency string.
///
/// The amount is always rendered with exactly two decimal places.
///
/// # Examples
///
/// ```
/// use doctest_probe::format_cents;
///
/// assert_eq!(format_cents(1_99), "$1.99");
/// assert_eq!(format_cents(0), "$0.00");
/// assert_eq!(format_cents(12_300), "$123.00");
/// ```
pub fn format_cents(cents: u64) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}
```

Run `cargo test` and the example runs as a test. (The crate here is named `doctest_probe`; substitute your own crate name in the `use` line.)

```text
   Doc-tests doctest_probe

running 1 test
test src/lib.rs - format_cents (line 9) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

The test name — `src/lib.rs - format_cents (line 9)` — points straight at the file and line of the doc comment. If you renamed `format_cents` or changed its output, this block would fail to compile or assert, and `cargo test` would go red. The documentation can no longer lie.

> **Tip:** Doc tests only run for **library** targets. A binary-only crate (`src/main.rs`) has no public API to document, so `cargo test` skips doc tests there. If you want doc-tested examples in a binary project, move the logic into a `lib.rs` (a `src/lib.rs` alongside `src/main.rs`) — the common "thin binary, fat library" layout covered in [Modules and Packages](/12-modules-packages/).

---

## Detailed Explanation

Let's unpack exactly what `rustdoc` does with that fenced block, contrasting each step with the JavaScript mental model.

### The fence with no language is treated as Rust

````text
/// ```
/// use doctest_probe::format_cents;
/// assert_eq!(format_cents(1_99), "$1.99");
/// ```
````

Inside a doc comment, a bare ```` ``` ```` fence defaults to Rust. (You can write ```` ```rust ```` explicitly, and you should when the doc is rendered somewhere that does not assume Rust.) A fence tagged with another language — ```` ```json ```` or ```` ```text ```` — is shown in the rendered docs but **not** compiled or run. So only Rust blocks become tests.

### `rustdoc` auto-wraps the block in `fn main`

The single most surprising thing for a newcomer: the snippet above has no `fn main`, yet it runs. Before compiling, `rustdoc` wraps the block in a generated `fn main` and adds `extern crate` glue, roughly like this:

```rust
fn main() {
    use doctest_probe::format_cents;
    assert_eq!(format_cents(1_99), "$1.99");
}
```

That is why a doc test reads like the *body* of a function. You almost never write `fn main` yourself; if you do (because you need a custom signature), `rustdoc` detects it and uses yours instead.

### Your crate is in scope, but not auto-imported

A doc test is compiled as if it were an *external* user of your crate. That means:

- You refer to your items through the crate path: `use doctest_probe::format_cents;` (or the full path `doctest_probe::format_cents(...)`).
- Only **public** (`pub`) items are reachable — exactly like a downstream consumer. A doc test cannot see private functions, which makes it a genuine black-box check of your public surface.

This is the opposite of the in-file `#[cfg(test)] mod tests` from [Unit Tests](/13-testing/00-unit-tests/), which *can* reach private items. Doc tests document and verify the public API; unit tests probe the internals.

### A doc test passes by not panicking

Just like a `#[test]` function (see [Unit Tests](/13-testing/00-unit-tests/)), a doc test "passes" if its generated `main` returns normally and "fails" if it **panics**. The `assert_eq!` macro panics on mismatch, which is how the example doubles as an assertion. There is no `expect(...).toBe(...)` — you use the same `assert!`/`assert_eq!` macros described in [Assertions](/13-testing/02-assertions/).

### Numeric literals: `1_99` is just `199`

The underscore in `1_99` is a digit separator (Rust allows `_` anywhere in a numeric literal). It is written `1_99` purely to read as "1 dollar, 99 cents"; the compiler sees `199`. This mirrors a JavaScript habit but is a real language feature here, not a string trick.

---

## Key Differences

| Concept                         | JSDoc `@example` (TypeScript/JavaScript)        | Rust doc test                                          |
| ------------------------------- | ----------------------------------------------- | ------------------------------------------------------ |
| Is the example executed?        | No — it is inert text                           | **Yes** — compiled and run by `cargo test`             |
| What enforces correctness?      | A separate, hand-written test (if any)          | The example *is* the test                              |
| Where it lives                  | A comment above the function                    | A ```` ``` ```` block in a `///` / `//!` comment       |
| Visibility scope                | n/a                                             | Public API only (compiled as an external consumer)     |
| Boilerplate needed              | n/a                                             | None — `rustdoc` wraps it in `fn main`                 |
| How failures surface            | Never (drift is silent)                         | Compile error or panic during `cargo test`             |
| Hiding setup from rendered docs | n/a                                             | Lines prefixed with `# ` in the comment                |

The deeper point is a philosophy difference. JavaScript treats documentation and tests as separate concerns that you keep in sync by discipline. Rust fuses them so that *drift is impossible*: an example that no longer compiles is a failing build, not a stale comment. This is why high-quality crates like `serde` and `std` itself are saturated with runnable examples — the examples cost nothing to keep honest.

> **Note:** Doc tests are slower to compile than unit tests because each block becomes its own tiny crate. On modern toolchains `rustdoc` *merges* doc tests from the same crate into a single compilation (you can see "merged doctests compilation took ..." in the output), which has made them dramatically faster than they once were. Still, for a function with dozens of micro-examples, prefer a handful of meaningful ones plus a `#[cfg(test)]` module.

---

## Hidden lines, `?`, and the doc-test attributes

Doc tests have a few conventions that have no JavaScript analogue. Each is a small `rustdoc` feature you control from inside the comment.

### Hidden setup lines with `# `

Sometimes a runnable example needs setup that would clutter the rendered documentation — an import, a helper value, or a trailing `Ok(())`. Prefix such a line *inside the doc comment* with `# ` (hash-space) and `rustdoc` will **compile and run it but hide it from the published HTML**. This is the one and only place the `# ` prefix is meaningful in Rust source.

```rust
// src/lib.rs
use std::num::ParseIntError;

/// Doubles a number parsed from a string.
///
/// The setup line is hidden from the rendered docs but still runs:
///
/// ```
/// # use hidden_probe::double_str;
/// let result = double_str("21")?;
/// assert_eq!(result, 42);
/// # Ok::<(), std::num::ParseIntError>(())
/// ```
pub fn double_str(s: &str) -> Result<i32, ParseIntError> {
    Ok(s.parse::<i32>()? * 2)
}
```

The reader of your HTML docs sees only the two interesting lines:

```rust
let result = double_str("21")?;
assert_eq!(result, 42);
```

…while `rustdoc` actually compiles all four. Two things to notice:

- **The `?` operator works** because the hidden final line `# Ok::<(), ParseIntError>(())` gives the generated `main` a `Result` return type. Without it, you cannot use `?` in a doc test, just as you cannot in a normal `fn main` that returns `()`. (Returning `Result` from tests is covered for `#[test]` functions in [`#[should_panic]` and `Result` tests](/13-testing/03-should-panic/); the same idea applies here.)
- The `# ` prefix is purely a `rustdoc` directive. **Do not** confuse it with anything you would write in ordinary Rust code.

> **Warning:** This `# ` hidden-line syntax is specific to doc comments inside `.rs` files. In plain Markdown like this guide, a line starting with `# ` is just a heading or literal text — never use it to "hide" lines in a Markdown code block, because there is no `rustdoc` to interpret it.

### `should_panic` — assert the example *does* panic

Attach `should_panic` to the fence to assert the example panics (rather than runs cleanly). This documents the failure contract of a function right next to it.

```rust
// src/lib.rs

/// Divides `total` evenly across `people`, returning cents per person.
///
/// # Panics
///
/// Panics if `people` is zero:
///
/// ```should_panic
/// use doctest_probe::split_evenly;
///
/// split_evenly(1000, 0); // division by zero -> panic
/// ```
pub fn split_evenly(total: u64, people: u64) -> u64 {
    total / people
}
```

`cargo test` reports it with a `- should panic` suffix:

```text
test src/lib.rs - split_evenly (line 52) - should panic ... ok
```

It is "ok" precisely because the example panicked as promised. (For the full semantics, including matching a specific message, see [`#[should_panic]`](/13-testing/03-should-panic/).)

### `no_run` — compile but do not execute

Some examples must *type-check* but cannot *run* in a test harness — a live HTTP request, an infinite loop, or anything touching the network or filesystem. Tag the fence `no_run`: `rustdoc` still compiles it (so it cannot rot), but skips execution.

```rust
// src/lib.rs

/// Pretends to perform a network request.
///
/// This example compiles but is not executed, because running it would
/// require a live server:
///
/// ```no_run
/// use hidden_probe::fetch_title;
///
/// let title = fetch_title("https://example.com")?;
/// println!("{title}");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn fetch_title(_url: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok("Example Domain".to_string())
}
```

In the output, `no_run` blocks are marked `- compile`:

```text
test src/lib.rs - fetch_title (line 24) - compile ... ok
```

### `ignore` — neither compile nor run

`ignore` is the escape hatch: the block is shown in docs but completely skipped — not even compiled. Use it sparingly, for pseudocode or examples that depend on a type that does not exist yet. Prefer `no_run` whenever the code can at least be type-checked, because `ignore` lets the example rot.

```rust
// src/lib.rs

/// A documented idea that is not yet implemented.
///
/// ```ignore
/// // This snippet is neither compiled nor run.
/// let result = hidden_probe::not_done_yet(3);
/// ```
pub fn placeholder() {}
```

```text
test src/lib.rs - placeholder (line 38) ... ignored
```

### `compile_fail` — assert the example *fails to compile*

Occasionally you want to prove that something is *rejected* by the compiler — for example, that a private field cannot be mutated from outside the crate. `compile_fail` makes the doc test pass when the block fails to compile.

```rust
// src/lib.rs

/// A counter whose internal field is private.
///
/// This example asserts at doc-test time that you CANNOT mutate the
/// private field from outside the crate:
///
/// ```compile_fail
/// use cf_probe::Counter;
/// let mut c = Counter::new();
/// c.count = 99; // private field -> compile error, which is expected
/// ```
pub struct Counter {
    count: u32,
}

impl Counter {
    pub fn new() -> Self {
        Counter { count: 0 }
    }
    pub fn get(&self) -> u32 {
        self.count
    }
}
```

```text
test src/lib.rs - Counter (line 8) - compile fail ... ok
```

> **Tip:** All of these — `should_panic`, `no_run`, `ignore`, `compile_fail` — are written directly after the opening fence (``` ```no_run ```), and you can combine some of them (e.g. ``` ```ignore,no_run ```). Plain Rust (no attribute) means "compile and run."

---

## Common Pitfalls

### Pitfall 1: Forgetting to import the item under test

Because a doc test is compiled as an *external* consumer, you must bring your item into scope. Omit the `use` and the block fails to compile — which is a feature, but a confusing one the first time.

```rust
/// Greets a name.
///
/// ```
/// // Forgot: use cf_probe::greet;
/// assert_eq!(greet("Ada"), "Hello, Ada!");
/// ```
pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}
```

Real output from `cargo test --doc`:

```text
running 1 test
test src/lib.rs - greet (line 5) ... FAILED

failures:

---- src/lib.rs - greet (line 5) stdout ----
error[E0425]: cannot find function `greet` in this scope
 --> src/lib.rs:7:12
  |
4 | assert_eq!(greet("Ada"), "Hello, Ada!");
  |            ^^^^^ not found in this scope
  |
help: consider importing this function
  |
2 + use cf_probe::greet;
  |

error: aborting due to 1 previous error

For more information about this error, try `rustc --explain E0425`.
Couldn't compile the test.

failures:
    src/lib.rs - greet (line 5)

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

The fix is to add the `use`. If the import is noise you would rather not show readers, hide it with the `# ` prefix described above.

### Pitfall 2: A stale example silently becoming wrong

This is the pitfall doc tests *prevent*, and it is worth seeing fail. If the documented expected value drifts from the real output, `cargo test` catches it:

```rust
/// Adds one. The doc example below has a WRONG expected value on purpose.
///
/// ```
/// use fail_probe::add_one;
/// assert_eq!(add_one(1), 3);
/// ```
pub fn add_one(n: i32) -> i32 {
    n + 1
}
```

Real failure output:

```text
running 1 test
test src/lib.rs - add_one (line 5) ... FAILED

failures:

---- src/lib.rs - add_one (line 5) stdout ----
Test executable failed (exit status: 101).

stderr:

thread 'main' panicked at /tmp/.../doctest_bundle_2024.rs:7:1:
assertion `left == right` failed
  left: 2
 right: 3
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    src/lib.rs - add_one (line 5)

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Note the panic points at a generated file (`doctest_bundle_2024.rs`) rather than your source — that file is the merged wrapper `rustdoc` built. The test *name* (`src/lib.rs - add_one (line 5)`) is what tells you where to look in your own code.

### Pitfall 3: Trying to use `?` without a `Result`-returning wrapper

The generated `main` returns `()` by default. Using `?` then fails, because `?` needs a function whose return type implements `FromResidual` (i.e. a `Result` or `Option`). The fix is the hidden trailing line `# Ok::<(), SomeError>(())`, which retypes the generated `main`:

```rust
/// ```
/// use temperature::Celsius;
/// let t = Celsius::parse("37C")?;   // needs the line below to compile
/// assert_eq!(t, Celsius::new(37.0));
/// # Ok::<(), temperature::ParseTempError>(())
/// ```
```

Without that last line you would get an error that `?` cannot be used in a function returning `()`. This is the same constraint as a real `fn main` and is explained alongside `Result`-returning tests in [`#[should_panic]` and `Result` tests](/13-testing/03-should-panic/).

### Pitfall 4: Reaching for a private item

A doc test cannot see non-`pub` items — it is an external consumer. If you find yourself wanting to doc-test a private helper, that is a signal to test it with an in-file `#[cfg(test)] mod tests` instead (see [Test Organization](/13-testing/01-test-organization/)). Doc tests are for the public, documented surface.

### Pitfall 5: Putting `# ` lines in plain Markdown

The `# ` hidden-line feature exists *only* inside `///`/`//!` comments in `.rs` files, where `rustdoc` processes it. If you copy a doc-test snippet into a Markdown file (like a README that is **not** run through `rustdoc`'s `--test` mode), those `# ` lines render literally. When sharing examples in plain Markdown, write the full, visible code.

---

## Best Practices

### Lead with a `# Examples` section

By convention, public items get an `# Examples` heading in their doc comment, followed by one or more runnable blocks. This is what `rustdoc` renders on docs.rs and what readers expect:

```rust
/// Returns the underlying value.
///
/// # Examples
///
/// ```
/// use temperature::Celsius;
/// assert_eq!(Celsius::new(37.0).value(), 37.0);
/// ```
pub fn example_placeholder() {}
```

### Write examples a reader would actually copy

A doc test is dual-purpose: a test *and* the example a user will paste into their own code. Favor realistic, copyable snippets over contrived ones. If a block needs setup that distracts from the point, hide it with `# `, but keep the visible part something a reader can run.

### Prefer `no_run` over `ignore`

`no_run` still type-checks, so it catches API drift (a renamed method, a changed signature). `ignore` catches nothing. Reach for `ignore` only when the code genuinely cannot compile in isolation.

### Run doc tests in CI, and know the commands

- `cargo test` — runs unit tests, integration tests, **and** doc tests.
- `cargo test --doc` — runs *only* the doc tests (fast feedback while editing docs).
- `cargo test --doc <substring>` — filters doc tests by name. For example, `cargo test --doc format_cents` runs just that one and reports the rest as filtered out:

```text
   Doc-tests doctest_probe

running 1 test
test src/lib.rs - format_cents (line 9) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.06s
```

> **Note:** Doc tests are *not* run by `cargo nextest`, the faster runner discussed in [Coverage](/13-testing/10-coverage/). If you rely on nextest, add a separate `cargo test --doc` step so your examples are still verified.

### Keep heavy logic out of doc tests

A doc test should illustrate usage in a few lines. Exhaustive case coverage, edge cases, and private-internals checks belong in `#[cfg(test)]` modules ([Unit Tests](/13-testing/00-unit-tests/)) or [property tests](/13-testing/07-property-testing/). One or two crisp examples per public item is the sweet spot.

---

## Real-World Example

A small temperature-conversion library where every public item carries a runnable example. It shows a crate-level `//!` example, examples on methods, a `?`-using example with a hidden setup line, and an error path — all compile-verified.

```rust
// src/lib.rs
//! A small temperature-conversion library.
//!
//! Every public item carries runnable examples. Because the crate root has
//! its own `//!` doc comment, you can even put a top-level example here:
//!
//! ```
//! use temperature::Celsius;
//!
//! let boiling = Celsius::new(100.0);
//! assert_eq!(boiling.to_fahrenheit(), 212.0);
//! ```

/// An error returned when parsing a temperature string fails.
#[derive(Debug, PartialEq)]
pub struct ParseTempError(pub String);

impl std::fmt::Display for ParseTempError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid temperature: {}", self.0)
    }
}

impl std::error::Error for ParseTempError {}

/// A temperature in degrees Celsius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Celsius(f64);

impl Celsius {
    /// Creates a new `Celsius` value.
    ///
    /// ```
    /// use temperature::Celsius;
    ///
    /// let body = Celsius::new(37.0);
    /// assert_eq!(body.value(), 37.0);
    /// ```
    pub fn new(degrees: f64) -> Self {
        Celsius(degrees)
    }

    /// Returns the underlying value.
    pub fn value(self) -> f64 {
        self.0
    }

    /// Converts to degrees Fahrenheit.
    ///
    /// ```
    /// use temperature::Celsius;
    ///
    /// assert_eq!(Celsius::new(0.0).to_fahrenheit(), 32.0);
    /// assert_eq!(Celsius::new(100.0).to_fahrenheit(), 212.0);
    /// ```
    pub fn to_fahrenheit(self) -> f64 {
        self.0 * 9.0 / 5.0 + 32.0
    }

    /// Parses a string like `"37C"` into a `Celsius`.
    ///
    /// The `?` operator works because the hidden last line gives the
    /// example a `Result` return type:
    ///
    /// ```
    /// use temperature::Celsius;
    ///
    /// let t = Celsius::parse("37C")?;
    /// assert_eq!(t, Celsius::new(37.0));
    /// # Ok::<(), temperature::ParseTempError>(())
    /// ```
    ///
    /// Bad input is reported as an error rather than panicking:
    ///
    /// ```
    /// use temperature::Celsius;
    ///
    /// assert!(Celsius::parse("warm").is_err());
    /// ```
    pub fn parse(s: &str) -> Result<Celsius, ParseTempError> {
        let number = s
            .strip_suffix('C')
            .ok_or_else(|| ParseTempError(s.to_string()))?;
        let degrees: f64 = number
            .parse()
            .map_err(|_| ParseTempError(s.to_string()))?;
        Ok(Celsius(degrees))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trips() {
        assert_eq!(Celsius::parse("21C"), Ok(Celsius::new(21.0)));
    }
}
```

Running `cargo test` exercises the in-file unit test *and* all five doc tests in one command — real output:

```text
     Running unittests src/lib.rs (target/debug/deps/temperature-57d02ff4a26d2f96)

running 1 test
test tests::parse_round_trips ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests temperature

running 5 tests
test src/lib.rs - (line 6) ... ok
test src/lib.rs - Celsius::parse (line 64) ... ok
test src/lib.rs - Celsius::new (line 32) ... ok
test src/lib.rs - Celsius::to_fahrenheit (line 49) ... ok
test src/lib.rs - Celsius::parse (line 74) ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

A few things to read out of that output:

- The crate-level `//!` example shows up as `src/lib.rs - (line 6)` — the module root has no item name.
- `Celsius::parse` appears **twice** (lines 64 and 74) because it has two separate fenced blocks, and each block is its own test.
- The float comparisons (`32.0`, `212.0`) are exact here because these specific conversions land on representable values; for fuzzy float math you would assert within a tolerance rather than with `assert_eq!`.

Running `cargo doc --no-deps` then renders this into browsable HTML where every example is displayed exactly as written — minus the `# Ok::<(), ParseTempError>(())` line, which `rustdoc` hides. The documentation a user reads is, byte for byte, code that passed your test suite.

---

## Further Reading

- [The Rust Book — Documentation Comments as Tests](https://doc.rust-lang.org/book/ch14-02-publishing-to-crates-io.html#documentation-comments-as-tests)
- [The `rustdoc` Book — Documentation tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html)
- [The `rustdoc` Book — Hiding portions of the example](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html#hiding-portions-of-the-example)
- [Rust by Example — Documentation testing](https://doc.rust-lang.org/rust-by-example/testing/doc_testing.html)
- Sibling topics in this section:
  - [Unit Tests](/13-testing/00-unit-tests/) — `#[test]` and the in-file `#[cfg(test)] mod tests`; the "passes by not panicking" model doc tests share.
  - [Assertions](/13-testing/02-assertions/) — the `assert!`/`assert_eq!`/`assert_ne!` macros you use inside doc tests.
  - [`#[should_panic]` and `Result` tests](/13-testing/03-should-panic/) — the `should_panic` attribute and the `?`/`Result` pattern, shown here for doc tests.
  - [Test Organization](/13-testing/01-test-organization/) — when to use a doc test (public API) versus an in-file test (private internals).
  - [Integration Tests](/13-testing/04-integration-tests/) — black-box testing in `tests/`, the closest sibling to doc tests in spirit.
  - [Coverage](/13-testing/10-coverage/) — `cargo-llvm-cov` and why `cargo nextest` does not run doc tests.
- Foundations used above:
  - [Cargo Basics](/01-getting-started/03-cargo-basics/) — `cargo test` and `cargo doc`.
  - [Modules and Packages](/12-modules-packages/) — `pub` visibility and the library-vs-binary split that determines what gets doc-tested.
  - [Result and Option](/08-error-handling/00-result-option/) — why `?` needs a `Result`-returning wrapper.
  - [Macros](/14-macros/) — `#[derive(...)]`, `assert_eq!`, and the attribute machinery behind tests.

---

## Exercises

### Exercise 1: Your first doc test

**Difficulty:** Easy

**Objective:** Turn a usage comment into a runnable, verified example.

**Instructions:** Given the `count_vowels` function below, add an `# Examples` doc comment with a single fenced block that asserts the vowel counts for `"hello"` (2), `"XYZ"` (0), and `"AEIOU"` (5). Remember to `use` the function. Run `cargo test --doc` and confirm it passes.

```rust
pub fn count_vowels(s: &str) -> usize {
    s.chars()
        .filter(|c| "aeiou".contains(c.to_ascii_lowercase()))
        .count()
}

// TODO: add an /// # Examples doc test above the function
```

<details>
<summary>Solution</summary>

```rust
/// Returns the number of vowels in `s` (ASCII, case-insensitive).
///
/// # Examples
///
/// ```
/// use ex_probe::count_vowels;
///
/// assert_eq!(count_vowels("hello"), 2);
/// assert_eq!(count_vowels("XYZ"), 0);
/// assert_eq!(count_vowels("AEIOU"), 5);
/// ```
pub fn count_vowels(s: &str) -> usize {
    s.chars()
        .filter(|c| "aeiou".contains(c.to_ascii_lowercase()))
        .count()
}
```

`cargo test --doc` output:

```text
running 1 test
test src/lib.rs - count_vowels (line 6) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

</details>

### Exercise 2: A doc test that uses `?`

**Difficulty:** Medium

**Objective:** Write a doc test for a fallible function, using a hidden line so the `?` operator compiles.

**Instructions:** Write a `sum_csv(row: &str) -> Result<i64, ParseIntError>` that parses a comma-separated row of integers (trimming whitespace) and sums them. Add a doc test that parses `"1,2,3,4"` with `?`, asserts the total is `10`, and uses a hidden trailing line so the example's generated `main` returns `Result`.

```rust
pub fn sum_csv(row: &str) -> Result<i64, std::num::ParseIntError> {
    // TODO: split on ',', trim, parse, sum
    todo!()
}

// TODO: add a doc test that uses `?` and a hidden Ok(()) line
```

<details>
<summary>Solution</summary>

```rust
/// Parses a CSV row of integers and sums them.
///
/// ```
/// use ex_probe::sum_csv;
///
/// let total = sum_csv("1,2,3,4")?;
/// assert_eq!(total, 10);
/// # Ok::<(), std::num::ParseIntError>(())
/// ```
pub fn sum_csv(row: &str) -> Result<i64, std::num::ParseIntError> {
    row.split(',').map(|n| n.trim().parse::<i64>()).sum()
}
```

The visible example shows only the two meaningful lines; the hidden `# Ok::<(), ParseIntError>(())` retypes the generated `main` so `?` is legal. `cargo test --doc` output:

```text
running 1 test
test src/lib.rs - sum_csv (line 22) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

> **Tip:** `Iterator::sum` over `Result` items short-circuits: the first parse error becomes the overall error, and `?` propagates it. That is why the whole body fits on one line.

</details>

### Exercise 3: Document a panic with `should_panic`

**Difficulty:** Medium

**Objective:** Use the `should_panic` attribute to make the panic contract part of the documentation.

**Instructions:** Write `nth(slice: &[i32], index: usize) -> i32` that indexes into the slice (panicking on out-of-bounds, which is the default for slice indexing). Add a `# Panics` doc section with a `should_panic` example that indexes past the end of a 3-element vector.

```rust
pub fn nth(slice: &[i32], index: usize) -> i32 {
    // TODO: index into the slice
    todo!()
}

// TODO: add a /// # Panics doc test using ```should_panic
```

<details>
<summary>Solution</summary>

```rust
/// Returns the element at `index`, panicking if out of bounds.
///
/// # Panics
///
/// ```should_panic
/// use ex_probe::nth;
///
/// let data = vec![10, 20, 30];
/// nth(&data, 5); // out of bounds -> panic
/// ```
pub fn nth(slice: &[i32], index: usize) -> i32 {
    slice[index]
}
```

`cargo test --doc` output — the block "passes" because it panicked as documented:

```text
running 1 test
test src/lib.rs - nth (line 5) - should panic ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

</details>
