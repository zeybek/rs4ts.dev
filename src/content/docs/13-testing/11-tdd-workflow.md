---
title: "TDD Workflow in Rust"
description: "Run red-green-refactor in Rust, where your first failure is often a compile error. Stub with todo!(), read assert_eq! diffs, and add cargo watch or bacon."
---

## Quick Overview

Test-driven development (TDD) is the **red → green → refactor** loop: write a failing test, write the minimum code to pass it, then clean up with the test as a safety net. The mechanics you know from Jest or Vitest carry straight over, but Rust adds one twist worth internalizing up front. Because Rust is compiled and statically typed, your *first* "red" is often a **compile error**, not a failing assertion. This file shows how to run that loop comfortably in Rust, including how to get Vitest-style instant re-runs on save.

---

## TypeScript/JavaScript Example

A typical TDD session with Vitest. You start the runner in watch mode, and it re-runs the relevant tests every time you save.

```typescript
// slugify.test.ts
import { describe, it, expect } from "vitest";
import { slugify } from "./slugify";

describe("slugify", () => {
  it("lowercases and hyphenates", () => {
    expect(slugify("Hello World")).toBe("hello-world");
  });
});
```

```typescript
// slugify.ts  — the very first stub, written to make the test *compile* but fail
export function slugify(title: string): string {
  return "";
}
```

You launch the watcher once and leave it running:

```bash
npx vitest          # watch mode by default in a TTY; `vitest run` is single-shot
```

The watcher reports a failure, you flesh out `slugify`, save, and within milliseconds it flips to green. The feedback loop is the product: you almost never type `npx vitest` a second time.

A few things a JavaScript developer relies on here:

- A **watch mode** built into the runner (`vitest`, `jest --watch`).
- Tests and code in **separate files** that the runner re-runs on change.
- A failing test means an `expect` **threw**; there is no compile step that could fail first.

Rust keeps the first two ideas (with different tools) and changes the third.

---

## Rust Equivalent

The same `slugify`, grown test-first. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically, so nothing below pins an edition.

We will walk the loop in stages. **Stage 0** is the failing-on-purpose starting point: a stub that compiles but does not work yet, using the `todo!()` macro:

```rust
// src/lib.rs

/// Turns a human title into a URL slug.
pub fn slugify(title: &str) -> String {
    todo!("not implemented yet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_hyphenates() {
        assert_eq!(slugify("Hello World"), "hello-world");
    }
}
```

`cargo test` gives us our **red**, a real, run-it-yourself failure:

```text
running 1 test
test tests::lowercases_and_hyphenates ... FAILED

failures:

---- tests::lowercases_and_hyphenates stdout ----

thread 'tests::lowercases_and_hyphenates' panicked at src/lib.rs:3:5:
not yet implemented: not implemented yet
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    tests::lowercases_and_hyphenates

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

> **Tip:** `todo!()` (and its sibling `unimplemented!()`) are the idiomatic way to stub a function during TDD. They type-check as *any* return type — so `fn slugify(..) -> String { todo!() }` compiles — but **panic** when reached, which is exactly what makes the test fail. This is much cleaner than returning a dummy `String::new()` that might accidentally pass a weak test.

**Stage 1 — green.** Write the least code that satisfies the one test:

```rust
// src/lib.rs

/// Turns a human title into a URL slug.
pub fn slugify(title: &str) -> String {
    title.to_lowercase().replace(' ', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_hyphenates() {
        assert_eq!(slugify("Hello World"), "hello-world");
    }
}
```

```text
running 1 test
test tests::lowercases_and_hyphenates ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**Stage 2: drive out a bug with a new red.** Add a test for messy whitespace. The naive `replace(' ', "-")` mishandles runs of spaces:

```rust
    #[test]
    fn collapses_runs_of_whitespace() {
        assert_eq!(slugify("  Rust   is   Great  "), "rust-is-great");
    }
```

`assert_eq!` prints a precise diff of what diverged:

```text
running 2 tests
test tests::lowercases_and_hyphenates ... ok
test tests::collapses_runs_of_whitespace ... FAILED

failures:

---- tests::collapses_runs_of_whitespace stdout ----

thread 'tests::collapses_runs_of_whitespace' panicked at src/lib.rs:17:9:
assertion `left == right` failed
  left: "--rust---is---great--"
 right: "rust-is-great"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    tests::collapses_runs_of_whitespace

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**Stage 2 — green.** Switch to `split_whitespace`, which collapses any run of whitespace and trims the ends:

```rust
/// Turns a human title into a URL slug.
pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}
```

```text
running 2 tests
test tests::collapses_runs_of_whitespace ... ok
test tests::lowercases_and_hyphenates ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**Stage 3: refactor under green.** With two passing tests pinning the behavior, we can restructure freely: extract a private `normalize_word` helper and also strip punctuation (driven by one more test, omitted here for brevity; see the [Real-World Example](#real-world-example) for the full final file). The point of TDD is that the green bar tells you the refactor preserved behavior.

---

## Detailed Explanation

### The loop, step by step

| Step         | What you do                                   | Rust signal you watch for                          |
| ------------ | --------------------------------------------- | -------------------------------------------------- |
| **Red**      | Write a test for behavior that does not exist | a test that **panics** (or, first, a compile error) |
| **Green**    | Write the minimum code to pass                | `test result: ok`                                  |
| **Refactor** | Improve the code, keep tests passing          | the bar stays green                                |

This is identical to the Vitest loop in spirit. The differences are all in the *signals*.

### Your first "red" is frequently a compile error

This is the single biggest mental adjustment for a TypeScript/JavaScript developer. In Vitest, if you reference a function that does not exist, the test file simply throws a `ReferenceError` at runtime and shows up as a red test. In Rust, the test **binary will not build at all**. `cargo test` stops at the compiler:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn slug_works() {
        // slugify does not exist yet
        assert_eq!(slugify("Hello World"), "hello-world");
    }
}
```

```text
error[E0425]: cannot find function `slugify` in this scope
 --> src/lib.rs:6:20
  |
6 |         assert_eq!(slugify("Hello World"), "hello-world");
  |                    ^^^^^^^ not found in this scope

For more information about this error, try `rustc --explain E0425`.
error: could not compile `slugify_probe` (lib test) due to 1 previous error
```

That is still a perfectly good red: it tells you exactly what to build next. The disciplined Rust TDD habit is: write the test, then write a *stub* (`fn slugify(_: &str) -> String { todo!() }`) so the code **compiles** and you get a genuine *test* failure rather than a *build* failure. From there the loop feels just like Vitest.

> **Note:** Because the compiler runs first, Rust gives you a second, free layer of red. A test that calls your function with the wrong argument types, or expects a `u32` where you return a `String`, fails *at compile time*. You can never ship a test that lies about your types. TypeScript's `tsc` offers something similar, but Vitest by default transpiles each file independently and will happily run code that `tsc` would reject.

### `todo!()` vs returning a dummy value

`todo!()` expands to a panic, so it is never mistaken for a real implementation, and it satisfies *any* return type so the surrounding code type-checks. Compare:

```rust
fn slugify(title: &str) -> String {
    String::new() // bad stub: an under-specified test ("non-empty?") could pass by accident
}
```

```rust
fn slugify(title: &str) -> String {
    todo!() // good stub: compiles, but every test that calls it fails loudly
}
```

### Why `assert_eq!` output is your fastest debugger

When the whitespace test failed, the report showed `left: "--rust---is---great--"` next to `right: "rust-is-great"`. The `left` value is what your code produced; `right` is what you expected. Seeing the *actual* wrong output (leading hyphens, tripled hyphens) points straight at the bug: `replace` substitutes each space one-for-one instead of collapsing. This is the same information `expect(a).toBe(b)` gives you in Vitest, formatted for the terminal. Assertions and their messages are covered in depth in [Assertions](/13-testing/02-assertions/).

### Keeping the loop fast

The reason TDD is pleasant in JavaScript is the sub-second watch loop. Rust needs a watcher too, because typing `cargo test` by hand after every edit kills the rhythm. The next two sections cover the tools.

---

## Key Differences

| Concept                | Vitest / Jest                                  | Rust                                                        |
| ---------------------- | ---------------------------------------------- | ---------------------------------------------------------- |
| First failure for new code | runtime `ReferenceError` → red test        | **compile error** → fix with a `todo!()` stub               |
| Stub a not-yet-written fn | return `null`, throw, or leave undefined     | `todo!()` / `unimplemented!()`: panics, type-checks anywhere |
| Built-in watch mode    | `vitest` (default), `jest --watch`             | none built in; add `cargo watch` or `bacon`                 |
| Re-run only affected tests | smart, file-graph based                    | `cargo test <substring>`; runners like nextest help        |
| Failure signal         | an `expect` throws                             | the test function **panics**                                |
| Type mistakes in tests | caught only if you also run `tsc`              | caught by `cargo test` itself, every time                   |

The headline: **Rust has no watch mode out of the box.** This surprises people coming from `vitest`/`jest --watch`. The compiler/borrow-checker safety net is excellent, but the instant-rerun ergonomics are something you install. Two community tools fill the gap.

---

## Common Pitfalls

### Pitfall 1: Expecting a runtime failure when you actually get a compile error

A TypeScript developer writes the test first, runs `cargo test`, and is briefly confused that there is no "1 failed" line, just an `error[E0425]`. That *is* the red. Do not fight it: add the stub so the code compiles, and then you will see the familiar `FAILED` test line. Treat "it does not compile" as the zeroth red of every cycle.

### Pitfall 2: A stub that accidentally passes a weak test

If you stub with `String::new()` and your first test only checks `assert!(!slugify("x").is_empty())` — wait, that would fail — but a test like `assert_eq!(slugify(""), "")` would *pass* against the empty-string stub, giving a false green. `todo!()` cannot do this: it panics on every call, so a green bar always means real code ran.

### Pitfall 3: Forgetting tests run in parallel, so watch-mode reruns are non-deterministic in order

`cargo test` runs test functions on multiple threads. With a watcher firing on every save, output ordering shifts run to run. That is fine for independent tests, but if a test depends on shared mutable global state it will flake intermittently under the watcher. Keep tests independent (see [Test Fixtures](/13-testing/05-test-fixtures/)), or force serial execution while debugging with `cargo test -- --test-threads=1`.

### Pitfall 4: Recompiling the whole world on every keystroke

A naive watch setup that runs `cargo test` on *any* file change (including `Cargo.toml` or generated files) can trigger full rebuilds and ruin the fast loop. Scope the watcher to source files and prefer `cargo test` (debug) over `--release` during TDD; release builds optimize and are far slower to compile. Also consider narrowing to the module you are working on with a name filter:

```bash
cargo test slugify   # runs only tests whose path contains "slugify"
```

```text
running 1 test
test tests::strips_punctuation ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s
```

The `4 filtered out` line confirms the other tests were skipped, a real speed win on a large suite.

---

## Best Practices

### Install a watcher for a Vitest-like loop

Rust does not ship a watch mode, so install one. Two good choices, both fetched from crates.io:

```bash
cargo install cargo-watch   # adds the `cargo watch` subcommand
# or, the more actively maintained modern option:
cargo install bacon
```

**`cargo-watch`** wraps any cargo command and reruns it when files under the project change. The canonical TDD invocation:

```bash
cargo watch -x test          # rerun `cargo test` on every save
cargo watch -x check         # even faster: just type-check, no test run
cargo watch -x 'test slugify'  # rerun only matching tests
cargo watch -c -x test       # `-c` clears the screen before each run
```

`-x` ("execute") takes the cargo subcommand to run; `cargo watch -x check -x test` chains them, so you get a fast `check` first and only run tests if it compiles.

**`bacon`** is a background code checker built for exactly this loop. You run it once:

```bash
bacon test          # live test results; press `t`/`c`/`l` to switch jobs
```

and it stays open in a side terminal, recompiling and re-testing on save, showing a compact pass/fail summary and jumping you to the first error. Many Rustaceans keep `bacon` running in a split pane the way they kept `vitest` open.

> **Tip:** Whichever you pick, `cargo watch -x check` (or `bacon`'s default `check` job) is the *fastest* possible inner loop: `cargo check` skips code generation and linking, so it confirms your code type-checks and borrow-checks in a fraction of the time a full `cargo test` takes. Many people TDD with `check` running continuously and run the full test suite less often.

### Lean on `cargo check` and `cargo clippy` as part of red-green

Rust's compiler is a participant in your TDD loop, not an obstacle. A quick `cargo check` after writing a test confirms the test *compiles* (catching type-level mistakes) before you have written any implementation. After going green, a `cargo clippy` pass during the refactor step catches non-idiomatic code while the safety net is up:

```bash
cargo clippy --all-targets
```

```text
    Checking slugify_probe v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.36s
```

(No warnings means clean.)

### Refactor only when green

The discipline that makes TDD safe: never refactor and add behavior at the same time. Go green first, *then* restructure with the passing tests guarding you. Rust's ownership and type guarantees make many refactors mechanical, but they cannot tell you that you changed an *observable behavior*; only your tests can.

### Use a faster runner on big suites

As the suite grows, compile-and-link time dominates the loop. `cargo nextest` runs the test binaries with a faster, more parallel harness and clearer output; pairing it with a watcher (`cargo watch -x 'nextest run'`) keeps the loop snappy. See [Coverage](/13-testing/10-coverage/) for installing and using nextest.

---

## Real-World Example

Here is the full, compile-verified end state of the `slugify` TDD session: a public function, a private `normalize_word` helper extracted during the refactor step, a doc test (which also runs under `cargo test`; see [Doc Tests](/13-testing/09-doc-tests/)), and the unit-test module that grew one test at a time.

```rust
// src/lib.rs
//! A tiny URL-slug builder, grown test-first.

/// Turns a human title into a URL slug:
/// lowercased, ASCII-alphanumeric words joined by single hyphens.
///
/// ```
/// assert_eq!(slugify_probe::slugify("Rust: A Language!"), "rust-a-language");
/// ```
pub fn slugify(title: &str) -> String {
    title
        .split_whitespace()
        .map(normalize_word)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Lowercases a single word and drops everything that is not
/// an ASCII letter or digit.
fn normalize_word(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_hyphenates() {
        assert_eq!(slugify("Hello World"), "hello-world");
    }

    #[test]
    fn collapses_runs_of_whitespace() {
        assert_eq!(slugify("  Rust   is   Great  "), "rust-is-great");
    }

    #[test]
    fn strips_punctuation() {
        assert_eq!(slugify("Rust: A Language!"), "rust-a-language");
    }

    #[test]
    fn empty_input_is_empty_slug() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn normalize_word_is_testable_directly() {
        // Private helper, reachable from the in-file test module.
        assert_eq!(normalize_word("C++!?"), "c");
    }
}
```

A full `cargo test` run exercises both the unit tests and the doc test:

```text
running 5 tests
test tests::collapses_runs_of_whitespace ... ok
test tests::empty_input_is_empty_slug ... ok
test tests::lowercases_and_hyphenates ... ok
test tests::normalize_word_is_testable_directly ... ok
test tests::strips_punctuation ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

```text
   Doc-tests slugify_probe

running 1 test
test src/lib.rs - slugify (line 6) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

The whole session — five tests and a doc test — was built by repeating red → green → refactor. In practice you would have had `cargo watch -x test` (or `bacon`) open the entire time, watching each save flip the bar. The refactor that introduced `normalize_word` (which both strips punctuation *and* let us delete the separate `to_lowercase()` call) was done with the bar green; the moment any test had gone red, the watcher would have told you instantly.

> **Note:** Notice the private `normalize_word` is tested directly from the in-file `#[cfg(test)] mod tests`. TDD on internal helpers is one of the places Rust shines compared to black-box Vitest suites: you can test-drive a private function into existence without exporting it. Where to put these tests, and the trade-off versus black-box [Integration Tests](/13-testing/04-integration-tests/), is the subject of [Test Organization](/13-testing/01-test-organization/).

---

## Further Reading

- [The Rust Book — Writing Automated Tests](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [The Rust Book — Test-Driven Development of a Library's Functionality](https://doc.rust-lang.org/book/ch12-04-testing-the-librarys-functionality.html): a worked TDD example in the `minigrep` project
- [`cargo-watch` on crates.io](https://crates.io/crates/cargo-watch)
- [`bacon` — background rust code checker](https://dystroy.org/bacon/)
- [`todo!` macro documentation](https://doc.rust-lang.org/std/macro.todo.html) and [`unimplemented!`](https://doc.rust-lang.org/std/macro.unimplemented.html)
- Sibling topics in this section:
  - [Unit Tests](/13-testing/00-unit-tests/): `#[test]`, `#[cfg(test)] mod tests`, and `cargo test`.
  - [Assertions](/13-testing/02-assertions/): reading `assert_eq!` failure output, your fastest debugger in the loop.
  - [`#[should_panic]` and `Result` tests](/13-testing/03-should-panic/) — driving out error paths test-first.
  - [Test Organization](/13-testing/01-test-organization/): where the tests you write in the loop should live.
  - [Test Fixtures](/13-testing/05-test-fixtures/) — keeping tests independent so watch-mode reruns stay deterministic.
  - [Integration Tests](/13-testing/04-integration-tests/), [Mocking](/13-testing/06-mocking/), [Property Testing](/13-testing/07-property-testing/), [Doc Tests](/13-testing/09-doc-tests/), [Coverage](/13-testing/10-coverage/) (`cargo nextest` for a faster loop).
- Foundations used above:
  - [Cargo Basics](/01-getting-started/03-cargo-basics/): `cargo` is the test runner you are looping on.
  - [Hello World](/01-getting-started/02-hello-world/): the `cargo new` / project layout these examples assume.
  - [Macros](/14-macros/) — `todo!`, `assert_eq!`, and `#[test]` are the macro/attribute machinery powering the loop.

---

## Exercises

### Exercise 1: Run a full red-green-refactor cycle

**Difficulty:** Easy

**Objective:** Internalize the loop by building one function entirely test-first.

**Instructions:** Starting from an empty `lib.rs`, write a test for `is_palindrome(&str) -> bool` *before* the function exists. Get a compile error, add a `todo!()` stub to turn it into a real red, implement the simplest passing version, then add tests that drive out case-insensitivity and punctuation-stripping (so `"A man, a plan, a canal: Panama"` is a palindrome). Run `cargo test` after each step.

```rust
// Write the test first; add `pub fn is_palindrome(s: &str) -> bool { todo!() }`
// to make it compile, then implement and grow the tests.
```

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
pub fn is_palindrome(s: &str) -> bool {
    let cleaned: Vec<char> = s
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let reversed: Vec<char> = cleaned.iter().rev().copied().collect();
    cleaned == reversed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_palindrome() {
        assert!(is_palindrome("racecar"));
    }

    #[test]
    fn ignores_case_and_punctuation() {
        assert!(is_palindrome("A man, a plan, a canal: Panama"));
    }

    #[test]
    fn rejects_non_palindrome() {
        assert!(!is_palindrome("rust"));
    }
}
```

`cargo test` output:

```text
running 3 tests
test tests::ignores_case_and_punctuation ... ok
test tests::plain_palindrome ... ok
test tests::rejects_non_palindrome ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

</details>

### Exercise 2: TDD a small stateful type

**Difficulty:** Medium

**Objective:** Drive an `Option`-returning method into existence with tests, mirroring how `??`/`?.` callers in TypeScript expect a "no data yet" case.

**Instructions:** Build a `RunningAverage` struct test-first. Start with a test asserting that a brand-new average reports `None` (no data). Make it pass, then add a test that adds `2.0`, `4.0`, `6.0` and expects `Some(4.0)`. Implement `new`, `add`, and `mean` so both pass. The `mean()` method must return `Option<f64>` so the empty case is encoded in the type, not a sentinel like `NaN`.

```rust
#[derive(Default)]
pub struct RunningAverage {
    // TODO: fields
}

impl RunningAverage {
    // TODO: new, add, mean -> Option<f64>
}

// TODO: tests, written first
```

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
#[derive(Default)]
pub struct RunningAverage {
    count: u64,
    sum: f64,
}

impl RunningAverage {
    pub fn new() -> Self {
        RunningAverage::default()
    }

    pub fn add(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
    }

    /// Returns `None` until at least one value has been added.
    pub fn mean(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum / self.count as f64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_average_is_none() {
        let avg = RunningAverage::new();
        assert_eq!(avg.mean(), None);
    }

    #[test]
    fn averages_added_values() {
        let mut avg = RunningAverage::new();
        avg.add(2.0);
        avg.add(4.0);
        avg.add(6.0);
        assert_eq!(avg.mean(), Some(4.0));
    }
}
```

`cargo test` output:

```text
running 2 tests
test tests::averages_added_values ... ok
test tests::empty_average_is_none ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

</details>

### Exercise 3: TDD a fallible parser with `Result`-returning tests

**Difficulty:** Hard

**Objective:** Combine the red-green loop with error-path testing, using tests that return `Result<(), E>` and the `?` operator (see [`#[should_panic]` and `Result` tests](/13-testing/03-should-panic/)).

**Instructions:** Test-drive `parse_hex_color(&str) -> Result<Rgb, ParseError>` that parses `"#rrggbb"`. Grow it one test at a time: a valid color, a missing `#`, a wrong length, and a non-hex digit, with a distinct `ParseError` variant for each. Write the happy-path test as a function that returns `Result<(), ParseError>` and uses `?` instead of `unwrap`, so a parse failure surfaces as a test failure automatically.

```rust
#[derive(Debug, PartialEq)]
pub struct Rgb { /* r, g, b: u8 */ }

#[derive(Debug, PartialEq)]
pub enum ParseError { /* MissingHash, WrongLength, BadDigit */ }

pub fn parse_hex_color(input: &str) -> Result<Rgb, ParseError> {
    todo!()
}

// TODO: tests, written first — one per behavior
```

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
#[derive(Debug, PartialEq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    MissingHash,
    WrongLength,
    BadDigit,
}

/// Parses `#rrggbb` into an `Rgb`.
pub fn parse_hex_color(input: &str) -> Result<Rgb, ParseError> {
    let body = input.strip_prefix('#').ok_or(ParseError::MissingHash)?;
    if body.len() != 6 {
        return Err(ParseError::WrongLength);
    }
    let component = |range| {
        u8::from_str_radix(&body[range], 16).map_err(|_| ParseError::BadDigit)
    };
    Ok(Rgb {
        r: component(0..2)?,
        g: component(2..4)?,
        b: component(4..6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A test that returns Result so it can use `?` instead of unwrap.
    #[test]
    fn parses_a_valid_color() -> Result<(), ParseError> {
        let color = parse_hex_color("#ff8800")?;
        assert_eq!(color, Rgb { r: 255, g: 136, b: 0 });
        Ok(())
    }

    #[test]
    fn rejects_missing_hash() {
        assert_eq!(parse_hex_color("ff8800"), Err(ParseError::MissingHash));
    }

    #[test]
    fn rejects_bad_length() {
        assert_eq!(parse_hex_color("#fff"), Err(ParseError::WrongLength));
    }

    #[test]
    fn rejects_non_hex_digits() {
        assert_eq!(parse_hex_color("#gggggg"), Err(ParseError::BadDigit));
    }
}
```

`cargo test` output:

```text
running 4 tests
test tests::parses_a_valid_color ... ok
test tests::rejects_bad_length ... ok
test tests::rejects_missing_hash ... ok
test tests::rejects_non_hex_digits ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

</details>
