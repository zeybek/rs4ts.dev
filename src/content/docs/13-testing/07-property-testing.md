---
title: "Property-Based Testing"
description: "Rust's proptest asserts invariants over hundreds of random inputs and shrinks failures to a minimal case, like fast-check but with auto-saved cases."
---

Most of the tests you write in Jest or Vitest are **example tests**: you pick a handful of inputs, compute the expected output by hand, and assert. Property-based testing flips that around: you state a *rule* that must hold for **every** input, and the framework generates hundreds of random inputs trying to break it. In Rust, the `proptest` crate brings this style to `cargo test`, complete with automatic **shrinking** that boils a wild failing input down to the smallest one that still fails.

---

## Quick Overview

A **property test** asserts an invariant (`decode(encode(x)) == x`, "the output is always sorted", "addition is commutative") and lets the framework throw randomized inputs at it. When it finds a counterexample, **proptest** *shrinks* it, repeatedly simplifying the input while the test keeps failing, so you get the minimal reproduction instead of a 40-element vector of noise. If you have reached for [fast-check](https://github.com/dubzzz/fast-check) in TypeScript, proptest is the direct equivalent; if you have not, think of it as "fuzzing with assertions and a built-in minimizer." This complements the example-based [unit tests](/13-testing/00-unit-tests/); it does not replace them.

---

## TypeScript/JavaScript Example

In the JavaScript world, property testing is not built in; the standard tool is **fast-check**, usually driven by Jest or Vitest. Here is a realistic pair of properties for a `reverse` helper and a (buggy) `mergeSorted`.

```typescript
// merge.test.ts
import { test } from "vitest";
import fc from "fast-check";

function reverse(s: string): string {
  return [...s].reverse().join("");
}

// BUG: when both fronts are equal it advances *both* cursors,
// silently dropping one of the duplicates.
function mergeSorted(a: number[], b: number[]): number[] {
  const out: number[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] < b[j]) out.push(a[i++]);
    else if (a[i] > b[j]) out.push(b[j++]);
    else {
      out.push(a[i]);
      i++;
      j++; // <- drops a duplicate
    }
  }
  return out.concat(a.slice(i), b.slice(j));
}

test("reversing twice is the identity", () => {
  fc.assert(fc.property(fc.string(), (s) => reverse(reverse(s)) === s));
});

test("merge preserves total length", () => {
  fc.assert(
    fc.property(
      fc.array(fc.integer({ min: 0, max: 9 })),
      fc.array(fc.integer({ min: 0, max: 9 })),
      (a, b) => {
        a.sort((x, y) => x - y);
        b.sort((x, y) => x - y);
        return mergeSorted(a, b).length === a.length + b.length;
      },
    ),
  );
});
```

Running this against the buggy `mergeSorted` (here driven directly through Node, `node merge.mjs`) produces a *shrunk* counterexample:

```text
reverse roundtrip: ok
Property failed after 2 tests
{ seed: -1021748401, path: "1:1:1:1:1:3", endOnFailure: true }
Counterexample: [[0],[0]]
Shrunk 5 time(s)
```

The key moves to notice — and that Rust mirrors almost exactly:

- You add a **dependency** (`fast-check`) and wire it into your existing runner.
- `fc.property(...generators, predicate)` declares the rule; `fc.assert` runs ~100 random cases.
- On failure, fast-check **shrinks** the random `[[3,7,7],[1,7,9]]`-style input down to the minimal `[[0],[0]]`.

---

## Rust Equivalent

The same two properties with `proptest`. Add the dependency to a project's dev-dependencies (it is only needed for tests):

```toml
# Cargo.toml
[dev-dependencies]
proptest = "1.11"
```

> **Tip:** `cargo add proptest --dev` does this for you. The `cargo add` subcommand is built into Cargo (since 1.62); there is no `cargo-edit` to install. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition, and `cargo new` selects that edition automatically.

```rust
// src/lib.rs

/// Returns the input string reversed (by Unicode scalar value).
pub fn reverse(s: &str) -> String {
    s.chars().rev().collect()
}

/// Merges two already-sorted slices into one sorted `Vec`.
/// BUG: when the two fronts are equal it advances *both* cursors,
/// silently dropping one of the duplicates.
pub fn merge_sorted(a: &[i32], b: &[i32]) -> Vec<i32> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] < b[j] {
            out.push(a[i]);
            i += 1;
        } else if a[i] > b[j] {
            out.push(b[j]);
            j += 1;
        } else {
            out.push(a[i]); // BUG: keeps one, advances both
            i += 1;
            j += 1;
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn reversing_twice_is_identity(s in ".*") {
            prop_assert_eq!(reverse(&reverse(&s)), s);
        }

        #[test]
        fn merge_preserves_total_length(
            mut a in prop::collection::vec(0i32..10, 0..20),
            mut b in prop::collection::vec(0i32..10, 0..20),
        ) {
            a.sort();
            b.sort();
            let merged = merge_sorted(&a, &b);
            prop_assert_eq!(merged.len(), a.len() + b.len());
        }
    }
}
```

Running `cargo test` finds the bug and shrinks it to the minimal failing pair:

```text
running 2 tests
test tests::reversing_twice_is_identity ... ok
test tests::merge_preserves_total_length ... FAILED

failures:

---- tests::merge_preserves_total_length stdout ----
proptest: Saving this and future failures in .../proptest-regressions/lib.txt
proptest: If this test was run on a CI system, you may wish to add the following line to your copy of the file. (You may need to create it.)
cc f79f5196ae6f4bf6e635c36ee5c9fea44d5939fe632a2f66faaae07b209bfe5e

thread 'tests::merge_preserves_total_length' panicked at src/lib.rs:31:5:
Test failed: assertion failed: `(left == right)`
  left: `1`,
 right: `2` at src/lib.rs:40.
minimal failing input: mut a = [
    1,
], mut b = [
    1,
]
	successes: 0
	local rejects: 0
	global rejects: 0

note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    tests::merge_preserves_total_length

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Just like fast-check, proptest shrank a randomly-generated pair of vectors down to the irreducible counterexample `a = [1], b = [1]`, the smallest input that exposes the dropped-duplicate bug. Unlike fast-check, it also wrote a **regression file** that will re-run this exact case on every future `cargo test`.

---

## Detailed Explanation

### The `proptest!` macro and the `x in strategy` syntax

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn reversing_twice_is_identity(s in ".*") {
        prop_assert_eq!(reverse(&reverse(&s)), s);
    }
}
```

`proptest!` is a macro that wraps one or more `#[test]` functions. Where a plain unit test takes **no arguments** (see [Unit Tests](/13-testing/00-unit-tests/)), a proptest test takes *parameters* whose values proptest will generate. Each parameter uses the special `name in strategy` syntax: a **strategy** is a recipe for producing-and-shrinking values of a type.

- `s in ".*"` — a string literal is interpreted as a **regular expression**; proptest generates random strings matching it. `".*"` means "any string." (This is a proptest convenience; it is not a normal Rust pattern.)
- `prop::collection::vec(0i32..10, 0..20)`: a `Vec<i32>` of length `0..20`, each element drawn from `0..10`.
- `any::<u8>()`: the "natural" strategy for a type, here every possible `u8`.

The macro expands each function into a real `#[test]` that loops over generated cases (256 by default), so `cargo test` discovers and runs them with no extra runner, exactly like the built-in `#[test]` machinery described in [Unit Tests](/13-testing/00-unit-tests/).

### `prop_assert!` / `prop_assert_eq!` vs `assert!`

```rust
prop_assert_eq!(reverse(&reverse(&s)), s);
```

Inside `proptest!`, prefer `prop_assert!`, `prop_assert_eq!`, and `prop_assert_ne!` over the standard [`assert!` family](/13-testing/02-assertions/). The reason is *shrinking*: a `prop_assert*` failure returns an error that proptest catches and uses to drive shrinking, rather than unwinding through a panic. A plain `assert_eq!` still *works* (proptest catches the panic too), but `prop_assert*` integrates more cleanly and lets the harness keep running the shrink loop.

### Shrinking: the headline feature

When a property fails, proptest does **not** report the random input it happened to find. Instead it enters a shrink loop: it tries "smaller" variants of the failing input (shorter vectors, smaller numbers, characters closer to `'a'`) and keeps any variant that *still fails*. It repeats until it cannot shrink further. The `minimal failing input` line is the result of that search.

This is why the merge bug reported `a = [1], b = [1]` instead of something like `a = [2, 5, 5, 8], b = [1, 5, 9]`. Both fail, but the shrunk version tells you instantly: "two single-element lists with the same value." Shrinking is the single biggest reason property testing is worth the upfront cost.

### The regression file

```text
proptest: Saving this and future failures in .../proptest-regressions/lib.txt
```

The first time a property fails, proptest writes the failing seed to a `proptest-regressions/` directory next to your source:

```text
# Seeds for failure cases proptest has generated in the past. It is
# automatically read and these particular cases re-run before any
# novel cases are generated.
cc f79f5196ae6f4bf6e635c36ee5c9fea44d5939fe632a2f66faaae07b209bfe5e # shrinks to mut a = [1], mut b = [1]
```

On every subsequent run, proptest replays these seeds *first*. That turns a flaky, probabilistic failure into a deterministic one. Once a bug is found, it stays found until you fix it. **Check this file into version control** (the file itself says so). It is the property-testing analogue of a fast-check `seed`/`path`, but persisted automatically.

---

## Key Differences

| Concept | fast-check (TypeScript) | proptest (Rust) |
| --- | --- | --- |
| Installation | `npm i -D fast-check`, plus a runner | `cargo add proptest --dev`; runner is built in |
| Declaring a property | `fc.assert(fc.property(gen, pred))` | `proptest! { #[test] fn p(x in strat) { ... } }` |
| Generators / strategies | `fc.string()`, `fc.integer()`, `fc.array(...)` | `".*"`, `any::<i32>()`, `prop::collection::vec(...)` |
| Assertion | return a `boolean`, or `expect`/throw | `prop_assert!` / `prop_assert_eq!` |
| Default case count | ~100 | 256 (`ProptestConfig::cases`) |
| Shrinking | yes, automatic | yes, automatic |
| Reproducing a failure | copy the printed `seed`/`path` | auto-saved to `proptest-regressions/` |
| Type information | erased at runtime; generators are values | strategies are values; types are monomorphized |

> **Note:** A TypeScript predicate can return a `boolean` *or* throw. Rust has no exceptions, so a proptest body signals failure by returning an `Err` from a `prop_assert*` macro (the macro does `return Err(...)` for you); the body's real return type is `Result<(), TestCaseError>`, which the `proptest!` macro supplies. This is the same explicit-error philosophy you saw in [Error Handling](/08-error-handling/), applied to tests.

### Property tests vs example tests

They answer different questions and you want **both**:

| | Example test (`#[test]`) | Property test (`proptest!`) |
| --- | --- | --- |
| You provide | specific input + expected output | an invariant over *all* inputs |
| Catches | the cases you thought of | the cases you *didn't* think of |
| Reads like | a worked example / regression | a specification |
| Best for | known edge cases, exact values | round-trips, algebraic laws, "never panics" |
| Failure clarity | exact, by construction | excellent, thanks to shrinking |

A healthy suite uses example tests to pin down the cases that matter to humans (`encode("a b") == "a%20b"`) and property tests to patrol the infinite space of inputs you would never enumerate by hand.

---

## Common Pitfalls

### Pitfall 1: Importing the macro but not the prelude

The `proptest!` macro relies on `prop_assert_eq!` and the strategy combinators being in scope. Bringing in just the macro is a frequent first mistake:

```rust
pub fn double(n: i32) -> i32 {
    n * 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::proptest; // only the macro, NOT the prelude

    proptest! {
        #[test]
        fn doubling_is_even(n in 0i32..1000) {
            prop_assert_eq!(double(n) % 2, 0); // does not compile
        }
    }
}
```

Real `cargo test` output:

```text
error: cannot find macro `prop_assert_eq` in this scope
  --> src/lib.rs:11:13
   |
11 |             prop_assert_eq!(double(n) % 2, 0);
   |             ^^^^^^^^^^^^^^
   |
help: consider importing this macro
   |
 5 +     use proptest::prop_assert_eq;
   |
```

The fix is the idiomatic glob: `use proptest::prelude::*;`, which brings in the macro, the assertions, `any`, and the `prop` module together.

### Pitfall 2: Asserting exact equality on floating-point

A property like "addition is associative" feels obviously true, but IEEE-754 `f64` (the same representation as a JavaScript `number`) is **not** associative because of rounding:

```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn addition_is_associative(a in -1e6f64..1e6, b in -1e6f64..1e6, c in -1e6f64..1e6) {
            // fails: floating point addition is not associative
            prop_assert_eq!((a + b) + c, a + (b + c));
        }
    }
}
```

Real output (abridged):

```text
thread 'tests::addition_is_associative' panicked at src/lib.rs:5:5:
Test failed: assertion failed: `(left == right)`
  left: `-25968.140341530205`,
 right: `-25968.140341530176` at src/lib.rs:9.
minimal failing input: a = 168661.21983607815, b = -947504.2093726996, c = 752874.8491950913
```

The values differ in the last few bits. This is not a proptest quirk; the identical predicate fails in JavaScript too. The fix is to assert *approximate* equality: `prop_assert!(((a + b) + c - (a + (b + c))).abs() < 1e-6)`, or test an exact-arithmetic type. Property testing is excellent at surfacing this class of "I assumed math worked the way I learned in school" bug.

### Pitfall 3: Over-filtering inputs with `prop_assume!`

`prop_assume!(condition)` *rejects* a generated case that does not satisfy a precondition (the analogue of fast-check's `fc.pre`). It is fine when most inputs pass, but if you filter out almost everything, proptest cannot find enough valid cases and aborts:

```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn rarely_satisfiable(n in any::<u64>()) {
            prop_assume!(n == 42); // rejects essentially every input
            prop_assert_eq!(n, 42);
        }
    }
}
```

Real output:

```text
thread 'tests::rarely_satisfiable' panicked at src/lib.rs:5:5:
Test aborted: Too many global rejects
	successes: 0
	local rejects: 0
	global rejects: 1024
		1024 times at src/lib.rs:9:13: n == 42
```

The fix is to **generate** the constrained value directly instead of filtering for it: narrow the strategy (`n in 40u64..=44`) or build a custom strategy with `prop_map`. Reserve `prop_assume!` for preconditions that the *majority* of inputs satisfy, such as `prop_assume!(lo <= hi)` where you generate `lo` and `hi` independently.

### Pitfall 4: Tautological properties

A property that restates the implementation proves nothing:

```rust
// useless: this just re-runs the function and compares to itself
proptest! {
    #[test]
    fn double_equals_double(n in any::<i32>()) {
        prop_assert_eq!(n.wrapping_mul(2), n.wrapping_mul(2));
    }
}
```

Good properties are *independent* of the implementation: round-trips (`decode(encode(x)) == x`), relations to a slower-but-obviously-correct reference (a naive sort), or universal invariants ("output length equals input length", "result is sorted", "never panics"). If you cannot state the property without copying the function body, you probably want an [example test](/13-testing/00-unit-tests/) with a hand-computed answer instead.

---

## Best Practices

### Look for round-trips, invariants, and oracles

The three most productive property shapes:

- **Round-trip:** `decode(encode(x)) == x`, `from_str(to_string(x)) == Ok(x)`, `deserialize(serialize(x)) == x`. These catch asymmetric encode/decode bugs instantly.
- **Invariant:** something always true of the output regardless of input: "the result is sorted", "length is preserved", "the function does not panic".
- **Oracle (model-based):** compare your fast implementation against a simple, obviously-correct reference. Your optimized parser vs. a naive one; your custom collection vs. `std`.

### Tune the case count and shrink iterations with `ProptestConfig`

Per-block configuration goes in an inner attribute at the top of the `proptest!` block:

```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        #[test]
        fn runs_a_thousand_cases(n in 0i32..1000) {
            prop_assert!(n >= 0 && n < 1000);
        }
    }
}
```

`with_cases(1000)` runs more inputs (default is 256); `ProptestConfig { cases: 1000, max_shrink_iters: 10_000, ..Default::default() }` tunes more. You can also override globally with the `PROPTEST_CASES` environment variable, which is handy for a heavier nightly CI run without touching code.

### Build reusable strategies with `prop_compose!`

For domain types, factor the generator into a named strategy so multiple properties can share it:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Money {
    pub cents: u64,
}

impl Money {
    pub fn add(&self, other: &Money) -> Money {
        Money { cents: self.cents + other.cents }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // A reusable strategy producing a bounded `Money`.
    prop_compose! {
        fn arb_money()(cents in 0u64..1_000_000) -> Money {
            Money { cents }
        }
    }

    proptest! {
        #[test]
        fn addition_is_commutative(a in arb_money(), b in arb_money()) {
            prop_assert_eq!(a.add(&b), b.add(&a));
        }

        #[test]
        fn adding_zero_is_identity(a in arb_money()) {
            prop_assert_eq!(a.add(&Money { cents: 0 }), a);
        }
    }
}
```

This passes cleanly:

```text
running 2 tests
test tests::adding_zero_is_identity ... ok
test tests::addition_is_commutative ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

> **Tip:** `prop_compose!` is sugar for combining strategies. The double-parens `fn arb_money()(cents in ...)` read as "no outer arguments, then generate `cents` from this strategy, then map to a `Money`." For one-off mapping you can also write `(0u64..1_000_000).prop_map(|cents| Money { cents })` inline.

### Commit the regression directory

Add `proptest-regressions/` to source control. Each saved seed is a discovered bug; replaying them turns probabilistic coverage into a deterministic guard. Pair property tests with the example-based [unit tests](/13-testing/00-unit-tests/) and [assertions](/13-testing/02-assertions/) you already write: property testing widens coverage, it does not replace targeted regression cases.

---

## Real-World Example

A production-flavored percent-encoder for URL query values, validated with two properties (a round-trip and an output invariant) plus one example test for a human-meaningful edge case. This is the full, compile-verified file.

```rust
// src/lib.rs
//! A tiny URL-safe percent-encoder/decoder for query-string values.

/// Percent-encodes a string: every byte that is not an unreserved
/// character (`A-Z a-z 0-9 - _ . ~`) becomes `%XX` with uppercase hex.
pub fn encode(input: &str) -> String {
    let mut out = String::new();
    for &byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Decodes a percent-encoded string back into the original string,
/// returning `None` on malformed input.
pub fn decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return None;
                }
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // The central round-trip property: decode(encode(s)) == s for ANY string.
        #[test]
        fn encode_then_decode_roundtrips(s in ".*") {
            let restored = decode(&encode(&s));
            prop_assert_eq!(restored.as_deref(), Some(s.as_str()));
        }

        // Output invariant: an encoded string contains only URL-safe bytes.
        #[test]
        fn encoded_output_is_url_safe(s in ".*") {
            let encoded = encode(&s);
            for c in encoded.chars() {
                prop_assert!(
                    c.is_ascii_alphanumeric()
                        || matches!(c, '-' | '_' | '.' | '~' | '%')
                        || c.is_ascii_hexdigit(),
                    "unexpected char {:?} in encoded output",
                    c
                );
            }
        }
    }

    // A classic example test still earns its place for a known edge case.
    #[test]
    fn encodes_a_space_as_percent_20() {
        assert_eq!(encode("a b"), "a%20b");
    }
}
```

Running `cargo test` produces real output:

```text
running 3 tests
test tests::encodes_a_space_as_percent_20 ... ok
test tests::encoded_output_is_url_safe ... ok
test tests::encode_then_decode_roundtrips ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
```

The round-trip property is doing real work here: it generates Unicode strings with embedded spaces, control characters, multi-byte UTF-8, and percent signs — inputs you would rarely enumerate by hand — and verifies that every one of them survives the encode/decode cycle. If a future "optimization" forgot to encode some byte, or mishandled a multi-byte character boundary, the property would fail and shrink to the shortest offending string. Meanwhile, the one example test documents the single most important human-readable fact: a space becomes `%20`.

---

## Further Reading

- [The `proptest` book](https://proptest-rs.github.io/proptest/intro.html) — the official guide to strategies, shrinking, and configuration.
- [`proptest` on docs.rs](https://docs.rs/proptest/latest/proptest/) — API reference for the prelude, `prop_compose!`, and `ProptestConfig`.
- [`proptest` on crates.io](https://crates.io/crates/proptest) — current version and changelog.
- [QuickCheck](https://github.com/BurntSushi/quickcheck) — the other major Rust property-testing crate, modeled on Haskell's QuickCheck (lighter, fewer strategy combinators).
- [fast-check](https://github.com/dubzzz/fast-check) — the TypeScript/JavaScript tool this chapter compares against.
- Sibling topics in this section:
  - [Unit Tests](/13-testing/00-unit-tests/) — example-based `#[test]` functions; property tests build on the same harness.
  - [Assertions](/13-testing/02-assertions/) — `assert!`/`assert_eq!` and how `prop_assert!` differs.
  - [`#[should_panic]` and `Result` tests](/13-testing/03-should-panic/) — other ways tests signal failure.
  - [Mocking](/13-testing/06-mocking/) — trait-based test doubles with `mockall`.
  - [Benchmarking](/13-testing/08-benchmarking/) — measuring performance with `criterion`.
  - [Test Organization](/13-testing/01-test-organization/) and [Integration Tests](/13-testing/04-integration-tests/) — where these tests live.
- Foundations used above:
  - [Cargo Basics](/01-getting-started/03-cargo-basics/): `cargo add --dev` and `cargo test`.
  - [Types](/02-basics/01-types/) — why `f64` equality is brittle (it is JavaScript's `number`).
  - [Error Handling](/08-error-handling/): the `Result`-returning model behind `prop_assert!`.
  - [Macros](/14-macros/): how `proptest!` and `prop_compose!` expand at compile time.

---

## Exercises

### Exercise 1: Your first property

**Difficulty:** Easy

**Objective:** Write a property test for a symmetric function.

**Instructions:** Given `abs_diff` below, write a `proptest!` block asserting that the absolute difference is **symmetric** — `abs_diff(a, b) == abs_diff(b, a)` — for any two `i32` values. Use `any::<i32>()` as the strategy. Run `cargo test` and confirm it passes.

```rust
pub fn abs_diff(a: i32, b: i32) -> u32 {
    a.abs_diff(b)
}

// TODO: add a #[cfg(test)] mod tests with a proptest! property
```

<details>
<summary>Solution</summary>

```rust
pub fn abs_diff(a: i32, b: i32) -> u32 {
    a.abs_diff(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn abs_diff_is_symmetric(a in any::<i32>(), b in any::<i32>()) {
            prop_assert_eq!(abs_diff(a, b), abs_diff(b, a));
        }
    }
}
```

`cargo test` output:

```text
running 1 test
test tests::abs_diff_is_symmetric ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

</details>

### Exercise 2: Invariants of a sort

**Difficulty:** Medium

**Objective:** Assert *multiple* invariants about a function's output, not a single exact value.

**Instructions:** Write `my_sort(v: Vec<i32>) -> Vec<i32>` that returns the input sorted ascending. Then write one property over a generated `Vec<i32>` (use `prop::collection::vec(any::<i32>(), 0..50)`) that checks **two** invariants: (a) the output length equals the input length, and (b) the output is non-decreasing. Why is "length is preserved" worth asserting even though you only called `.sort()`?

```rust
pub fn my_sort(v: Vec<i32>) -> Vec<i32> {
    // TODO
}

// TODO: add a #[cfg(test)] mod tests with a proptest! property
```

<details>
<summary>Solution</summary>

```rust
pub fn my_sort(mut v: Vec<i32>) -> Vec<i32> {
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn sort_keeps_len_and_orders(v in prop::collection::vec(any::<i32>(), 0..50)) {
            let original_len = v.len();
            let sorted = my_sort(v);
            // (a) sorting must not add or drop elements
            prop_assert_eq!(sorted.len(), original_len);
            // (b) the result is non-decreasing
            prop_assert!(sorted.windows(2).all(|w| w[0] <= w[1]));
        }
    }
}
```

`cargo test` output:

```text
running 1 test
test tests::sort_keeps_len_and_orders ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

The length invariant matters for *other* sort implementations you might write: a hand-rolled merge sort (like the buggy `merge_sorted` earlier) can easily drop or duplicate elements while still producing a sorted-looking result. "Output is sorted" alone would not catch that; "output is sorted **and** same length" gets much closer (a multiset comparison would be airtight).

</details>

### Exercise 3: A precondition with `prop_assume!`

**Difficulty:** Medium

**Objective:** Generate three independent values, discard the cases that violate a precondition, and assert a bounded-output property.

**Instructions:** Write `clamp(value, lo, hi)` that returns `value` clamped to the range `lo..=hi`. Write a property over three independent `i32` strategies. Because `lo` and `hi` are generated separately, use `prop_assume!(lo <= hi)` to discard the inconsistent cases, then assert the result is always within `lo..=hi`. (This is a *good* use of `prop_assume!` — roughly half of all `(lo, hi)` pairs pass, so proptest finds plenty of valid cases.)

```rust
pub fn clamp(value: i32, lo: i32, hi: i32) -> i32 {
    // TODO
}

// TODO: add a #[cfg(test)] mod tests with a proptest! property
```

<details>
<summary>Solution</summary>

```rust
pub fn clamp(value: i32, lo: i32, hi: i32) -> i32 {
    value.max(lo).min(hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn clamp_result_is_within_bounds(
            value in any::<i32>(),
            lo in any::<i32>(),
            hi in any::<i32>(),
        ) {
            prop_assume!(lo <= hi); // discard inconsistent ranges
            let c = clamp(value, lo, hi);
            prop_assert!(c >= lo && c <= hi);
        }
    }
}
```

`cargo test` output:

```text
running 1 test
test tests::clamp_result_is_within_bounds ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

> **Tip:** A more efficient alternative avoids rejection entirely by generating an *ordered* pair with a custom strategy, for example `(any::<i32>(), any::<i32>()).prop_map(|(a, b)| if a <= b { (a, b) } else { (b, a) })`. Generating valid inputs directly is almost always better than filtering for them.

</details>
