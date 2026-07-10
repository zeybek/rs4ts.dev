---
title: "Documentation with `rustdoc`"
description: "Unlike JSDoc plus TypeDoc, rustdoc ships with the toolchain, checks cross-references, and runs your examples as tests so docs cannot drift out of date."
---

In Node you describe your API with JSDoc comments and run a tool like TypeDoc to turn them into a website. In Rust, documentation is a first-class, built-in feature: `rustdoc` ships with the toolchain, reads ordinary `///` comments written in Markdown, links your types together automatically, **runs the code in your examples as tests**, and publishes to [docs.rs](https://docs.rs) for free the moment you release a crate.

## Quick Overview

`rustdoc` is to Rust what TypeDoc-plus-JSDoc is to TypeScript, only it is part of the standard toolchain and far more tightly integrated. The three ideas a TypeScript/JavaScript developer needs to internalize:

- **Doc comments are Markdown** attached to items with `///` (outer) or `//!` (inner). `cargo doc` renders them into a searchable HTML site.
- **Intra-doc links** like `` [`Invoice::total`] `` resolve to other items in your crate (and its dependencies) and the compiler *errors* if a link is broken: no more rotted `{@link}` tags.
- **Examples are tests.** Every fenced ` ```rust ` block in your docs is compiled and executed by `cargo test`. Your documentation literally cannot drift out of date without turning your test suite red.

> **Note:** This page is about writing and publishing docs. For the wider tour of the ecosystem and the crates you will document against, see [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/). For the broader toolchain (formatting, linting, CI), see [Tooling](/24-tooling/).

---

## TypeScript/JavaScript Example

A well-documented TypeScript module uses JSDoc tags for descriptions, parameters, returns, examples, and cross-references. TypeDoc turns these into a site, but nothing checks that the `@example` blocks still compile or that `{@link}` targets still exist.

```typescript
// billing.ts

/** A currency code supported by {@link Invoice}. */
export enum Currency {
  Usd = "USD",
  Eur = "EUR",
}

/** A single billable line item. */
export interface LineItem {
  /** Human-readable description shown on the invoice. */
  description: string;
  /** Unit price in the smallest currency unit (cents) to avoid float drift. */
  unitPriceCents: number;
  /** Number of units billed. */
  quantity: number;
}

/**
 * Sums every line item to a grand total in cents.
 *
 * @param items - the line items on the invoice
 * @returns the total in cents
 *
 * @example
 * ```ts
 * const total = invoiceTotal([
 *   { description: "Seat", unitPriceCents: 1200, quantity: 3 },
 *   { description: "Add-on", unitPriceCents: 500, quantity: 1 },
 * ]);
 * console.log(total); // 4100   <-- but is this still true? nobody checks.
 * ```
 */
export function invoiceTotal(items: LineItem[]): number {
  return items.reduce((sum, i) => sum + i.unitPriceCents * i.quantity, 0);
}
```

Two weaknesses are baked in here. First, the `@example` block is an opaque string — if you rename `invoiceTotal` or change its return units, the example still "passes" because nobody runs it. Second, `{@link Invoice}` points at a type that does not even exist in this file; TypeDoc will emit a warning at best, but `tsc` itself is perfectly happy. Rust closes both gaps at the compiler level.

---

## Rust Equivalent

The same library in Rust. Create it with `cargo new --lib billing` (which selects the latest stable edition, 2024). Doc comments use `///`, Markdown formatting, intra-doc links in square brackets, and a `# Examples` section whose code is run as a test.

```rust
// src/lib.rs

//! A tiny billing library demonstrating rustdoc.
//!
//! The entry points are [`Invoice`] and the [`tax`] function. See the
//! [`Currency`] enum for supported currencies.

/// A currency code. Used by [`Invoice::total`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    /// United States dollar.
    Usd,
    /// Euro.
    Eur,
}

/// A single billable line item: a description, unit price in cents, and quantity.
#[derive(Debug, Clone)]
pub struct LineItem {
    /// Human-readable description shown on the invoice.
    pub description: String,
    /// Unit price in the smallest currency unit (e.g. cents), to avoid float drift.
    pub unit_price_cents: u64,
    /// Number of units billed.
    pub quantity: u32,
}

impl LineItem {
    /// The subtotal for this line, in cents: `unit_price_cents * quantity`.
    ///
    /// # Examples
    ///
    /// ```
    /// use billing::LineItem;
    /// let item = LineItem {
    ///     description: "API calls".into(),
    ///     unit_price_cents: 5,
    ///     quantity: 1_000,
    /// };
    /// assert_eq!(item.subtotal_cents(), 5_000);
    /// ```
    pub fn subtotal_cents(&self) -> u64 {
        self.unit_price_cents * self.quantity as u64
    }
}

/// An invoice: a set of [`LineItem`]s billed in a [`Currency`].
#[derive(Debug, Clone)]
pub struct Invoice {
    /// The currency every line is billed in.
    pub currency: Currency,
    /// The line items on this invoice.
    pub items: Vec<LineItem>,
}

impl Invoice {
    /// Sums every line's [`LineItem::subtotal_cents`] to a grand total in cents.
    ///
    /// # Examples
    ///
    /// ```
    /// use billing::{Currency, Invoice, LineItem};
    /// let invoice = Invoice {
    ///     currency: Currency::Usd,
    ///     items: vec![
    ///         LineItem { description: "Seat".into(), unit_price_cents: 1_200, quantity: 3 },
    ///         LineItem { description: "Add-on".into(), unit_price_cents: 500, quantity: 1 },
    ///     ],
    /// };
    /// assert_eq!(invoice.total(), 4_100);
    /// ```
    pub fn total(&self) -> u64 {
        self.items.iter().map(LineItem::subtotal_cents).sum()
    }
}

/// Applies a tax `rate` (e.g. `0.2` for 20%) to an amount in `cents`,
/// rounding to the nearest cent.
///
/// # Examples
///
/// ```
/// use billing::tax;
/// assert_eq!(tax(10_000, 0.2), 2_000);
/// ```
pub fn tax(cents: u64, rate: f64) -> u64 {
    (cents as f64 * rate).round() as u64
}
```

Run `cargo test` and the example blocks become a test suite:

```text
   Doc-tests billing

running 3 tests
test src/lib.rs - tax (line 80) ... ok
test src/lib.rs - Invoice::total (line 59) ... ok
test src/lib.rs - LineItem::subtotal_cents (line 31) ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

Run `cargo doc --open` and you get a searchable website where every `` [`Invoice`] `` in the prose is a clickable link, generated from the same comments.

---

## Detailed Explanation

### `///` vs `//!`: outer vs inner doc comments

There are two kinds of doc comment, and the distinction trips up newcomers:

- `///` is an **outer** doc comment. It documents *the item that follows it*: the next `struct`, `fn`, `enum`, field, etc. This is the everyday workhorse, equivalent to a JSDoc `/** ... */` block placed above a declaration.
- `//!` is an **inner** doc comment. It documents *the thing it is inside of*. At the top of `lib.rs` it documents the whole crate; inside a `mod foo { ... }` it documents that module. There is no clean JSDoc analogue — it is like a file-level `@module` description, except the compiler genuinely associates it with the module.

Both are pure sugar for the `#[doc = "..."]` attribute, and both interpret their contents as **CommonMark Markdown**: headings (`#`), lists, tables, fenced code, links, and inline `code` all render. (Note: those Markdown `#` headings live *inside* the comment string; they are unrelated to Rust's `#[...]` attributes.)

### Doc comments are Markdown, and conventions matter

`rustdoc` recognizes a handful of conventional section headings. None are mandatory, but the community uses them consistently, so readers expect them:

| Heading | Purpose |
| --- | --- |
| `# Examples` | Runnable usage examples (also your tests) |
| `# Panics` | Conditions under which the function panics |
| `# Errors` | What the `Err` variants mean for a function returning `Result` |
| `# Safety` | Invariants the caller must uphold for an `unsafe fn` |

The first line (more precisely, the first paragraph) of a doc comment is the **summary line**. It appears next to the item in module listings and in search results, so keep it to a single tight sentence, exactly like the first line of a good JSDoc block.

### Intra-doc links: cross-references the compiler checks

The bracketed names in the comments above — `` [`Invoice`] ``, `` [`LineItem::subtotal_cents`] ``, `` [`Currency`] `` — are **intra-doc links**. `rustdoc` resolves each one to the actual item using the same name resolution the compiler uses, so:

- You write the path the way you would in code (`Invoice::total`, `crate::Currency`, `std::vec::Vec`), not a hand-maintained URL.
- Links to items in your **dependencies** and in **`std`** work too: `` [`Vec`] `` and `` [`std::collections::HashMap`] `` link straight to those crates' docs.
- If a target does not exist, `rustdoc` warns (and you can promote that to a hard error; see Best Practices). This is the structural fix for JSDoc's silently-rotting `{@link}` tags.

The backticks inside the brackets are optional styling (they render the link in monospace); `[Invoice]` and `` [`Invoice`] `` resolve to the same item.

### Examples really are tests

When `cargo test` runs, it extracts each fenced code block from your docs, wraps it in a `fn main`, compiles it as a standalone program against your crate, and runs it. This is the single most important rustdoc feature for a TypeScript developer to appreciate: **your documentation examples cannot lie**. Rename a method and the example stops compiling; change behavior and the `assert_eq!` fails. A red test suite forces you to update the docs.

Because each example is compiled as if a user wrote it, you import your own crate by name (`use billing::Invoice;`). You are documenting from the consumer's perspective, which also doubles as an integration test of your public API surface.

---

## Key Differences

| Concept | TypeScript / JSDoc + TypeDoc | Rust / rustdoc |
| --- | --- | --- |
| Tooling | External (`typedoc`), separately installed | Built into the toolchain (`cargo doc`) |
| Comment syntax | `/** ... */` with `@tags` | `///` (outer), `//!` (inner), Markdown body |
| Parameter docs | `@param name - ...` | Described in prose; no per-parameter tag |
| Cross-references | `{@link Foo}`, not checked by `tsc` | `` [`Foo`] ``, resolved and checked by the compiler |
| Examples | `@example` blocks, never executed | ` ```rust ` blocks, compiled and run by `cargo test` |
| Types in signatures | Re-stated in `@param {Type}` tags | Taken from the real signature; never duplicated |
| Hosting | You build and host the site yourself | [docs.rs](https://docs.rs) builds and hosts every release for free |

### You never repeat the types

JSDoc forces you to restate types you already wrote in TypeScript: `@param {number} cents`. Because Rust's signatures are always fully typed and rustdoc renders the real signature, doc comments describe *intent and behavior*, never the types. There is deliberately no `@param` tag. You write a `# Examples` section and a prose description, and the parameter names and types come from the function itself.

### Documentation is part of the test suite, not adjacent to it

In a Node project, examples in the README and JSDoc are aspirational; CI does not run them. In Rust, `cargo test` runs unit tests, integration tests, **and** doctests in one command. Doctests are first-class test cases that gate your CI exactly like any other test.

---

## Common Pitfalls

### Pitfall 1: assuming the example block is just decoration

A doctest is real code. If its assertion is wrong, `cargo test` fails. Suppose someone "fixes" the `tax` example to claim the wrong result:

```rust
/// ```
/// use billing::tax;
/// assert_eq!(tax(10_000, 0.2), 9_999); // wrong: 20% of 10_000 is 2_000
/// ```
```

`cargo test` reports a genuine failure (real output):

```text
---- src/lib.rs - tax (line 82) stdout ----
Test executable failed (exit status: 101).

stderr:

thread 'main' panicked at ...:
assertion `left == right` failed
  left: 2000
 right: 9999

test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
error: doctest failed, to rerun pass `--doc`
```

This is the feature working as intended. Treat a failing doctest the way you treat any failing test — fix the code or fix the docs.

### Pitfall 2: broken intra-doc links pass silently if you do not opt in

By default a broken `` [`Foo`] `` link only *warns* during `cargo doc`. With `#![warn(rustdoc::broken_intra_doc_links)]` (or `deny`) at the top of your crate, you get the diagnostic where you want it. Linking to a non-existent item produces this real warning:

```text
warning: unresolved link to `NonExistent`
 --> src/lib.rs:4:15
  |
4 | //! Broken: [`NonExistent`] is not a real item.
  |               ^^^^^^^^^^^ no item named `NonExistent` in scope
  |
  = help: to escape `[` and `]` characters, add '\' before them like `\[` or `\]`
```

The "escape" hint reveals the other half of this pitfall: prose like `the [key]` or `array[i]` is read as an intra-doc link and warns. Escape literal brackets as `\[key\]`, or wrap them in backticks (`` `array[i]` ``), which is not link-parsed.

### Pitfall 3: doctests with `?` need an explicit return type

JavaScript lets you `await` anywhere in an example. In a Rust doctest the code is wrapped in an implicit `fn main() {}`, which returns `()`, so a bare `?` will not compile; `main` has no `Result` to short-circuit into. Provide a return type by ending the example with a hidden `Ok(...)`:

```rust
/// ```
/// use billing::extra::parse_price_cents;
/// let cents = parse_price_cents("$12.50").ok_or("bad price")?;
/// assert_eq!(cents, 1250);
/// # Ok::<(), &'static str>(())
/// ```
```

The line beginning with `# ` is a **hidden line**: rustdoc compiles and runs it but hides it from the rendered page, so readers see a clean example while `?` still works. (Hidden lines are a rustdoc-only convenience inside doc comments; do not confuse them with anything in normal source files.)

### Pitfall 4: putting examples that touch the network or panic in a plain block

A plain ` ``` ` block is compiled *and executed*. If the example performs real I/O, or is meant to demonstrate a panic, annotate the fence:

- ` ```no_run `: compile it (so it stays type-correct) but do not execute it. Use for network calls, file I/O, or `async` servers.
- ` ```should_panic `: the example is expected to panic; the test passes only if it does. Use to document a precondition.
- ` ```ignore `: neither compile nor run. Use sparingly; prefer `no_run`, which still catches type errors.
- ` ```text `: not Rust at all; never compiled. Use for shell output or pseudo-code.

All three runnable annotations are verified together in the worked example below.

---

## Best Practices

- **Document every public item, and enforce it.** Add `#![warn(missing_docs)]` (or `deny` once the crate is clean) to your crate root. An undocumented `pub` item then produces a warning:

  ```text
  warning: missing documentation for a function
    --> src/lib.rs:91:1
     |
  91 | pub fn undocumented_helper() {}
     | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  ```

- **Lead with a one-sentence summary.** The first paragraph is the search-result blurb; make it stand alone.

- **Always include a `# Examples` section on public functions.** It documents *and* tests in one stroke. Aim for the smallest example that shows realistic usage.

- **Use the conventional headings** (`# Errors`, `# Panics`, `# Safety`) so readers find what they expect. A `Result`-returning function without an `# Errors` section is an incomplete API.

- **Link generously with intra-doc links.** They cost nothing to maintain and the compiler keeps them honest. Promote broken links to errors in CI with `#![deny(rustdoc::broken_intra_doc_links)]`.

- **Reuse your README as the crate's front page** so the page on docs.rs and the README on GitHub never diverge:

  ```rust
  // src/lib.rs
  #![doc = include_str!("../README.md")]
  ```

  `include_str!` embeds the file at compile time; rustdoc renders it as the crate-level documentation. (The README's own ` ```rust ` blocks then become doctests too, a nice way to keep your README's examples honest.)

- **Prefer `no_run` over `ignore`.** `no_run` still type-checks the example against your real API; `ignore` checks nothing and rots like JSDoc.

> **Tip:** Run `cargo doc --no-deps --open` while writing. `--no-deps` skips documenting every dependency (much faster), and `--open` launches the result in your browser so you see exactly what readers will see.

---

## Real-World Example

A production crate documents fallible functions, network calls, and preconditions all at once. The module below shows the `# Errors` convention, a `?`-using doctest, a `no_run` network example, and a `should_panic` precondition demo. Every block here is compiled and run (or compiled-only, for `no_run`) by `cargo test`.

```rust
// src/extra.rs  (declared with `pub mod extra;` in src/lib.rs)

//! Price parsing and charging helpers.

/// Parses a price like `"$12.50"` into integer cents.
///
/// Working in integer cents avoids the floating-point drift you would get
/// from storing money as `f64`.
///
/// # Examples
///
/// ```
/// use billing::extra::parse_price_cents;
/// let cents = parse_price_cents("$12.50").ok_or("bad price")?;
/// assert_eq!(cents, 1250);
/// # Ok::<(), &'static str>(())
/// ```
///
/// A precondition documented with `should_panic`:
///
/// ```should_panic
/// use billing::extra::charge;
/// charge(0); // panics: amount must be positive
/// ```
///
/// A network call documented with `no_run` (compiled, not executed):
///
/// ```no_run
/// use billing::extra::fetch_rate;
/// let rate = fetch_rate("USD");
/// println!("{rate}");
/// ```
pub fn parse_price_cents(s: &str) -> Option<u64> {
    let s = s.strip_prefix('$')?;
    let (dollars, cents) = s.split_once('.')?;
    let dollars: u64 = dollars.parse().ok()?;
    let cents: u64 = cents.parse().ok()?;
    Some(dollars * 100 + cents)
}

/// Charges an amount in cents.
///
/// # Panics
///
/// Panics if `amount` is zero.
pub fn charge(amount: u64) {
    assert!(amount > 0, "amount must be positive");
}

/// Fetches the current exchange rate for `code` from a remote service.
pub fn fetch_rate(_code: &str) -> f64 {
    1.0
}
```

Running `cargo test` exercises all of it (real output, abbreviated):

```text
   Doc-tests billing

running 6 tests
test src/extra.rs - extra::parse_price_cents (line 19) - compile ... ok
test src/extra.rs - extra::parse_price_cents (line 7) ... ok
test src/lib.rs - Invoice::total (line 61) ... ok
test src/lib.rs - tax (line 82) ... ok
test src/lib.rs - LineItem::subtotal_cents (line 33) ... ok
test src/extra.rs - extra::parse_price_cents (line 14) - should panic ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

Notice the test labels: the `no_run` block is reported as `- compile` (built, not executed), the `should_panic` block as `- should panic`, and the plain blocks run fully. One command keeps the prose, the examples, the panic contract, and the public API in lock-step.

### Publishing to docs.rs

You do not run a documentation server. When you publish a crate with `cargo publish`, the [docs.rs](https://docs.rs) service automatically builds your docs and hosts them at `https://docs.rs/<crate>/<version>`, permanently, for every released version. There is nothing to configure for the common case: the same `cargo doc` output you see locally is what readers see online. For crates that need extra features or a non-default target enabled during the docs.rs build, add a small section to `Cargo.toml`:

```toml
# Cargo.toml — only needed if your docs require non-default features.
[package.metadata.docs.rs]
all-features = true
# Surface which items are feature-gated, using a nightly rustdoc flag docs.rs enables:
rustdoc-args = ["--cfg", "docsrs"]
```

This is the Rust answer to "where do I host my TypeDoc site?": you don't — releasing the crate publishes the docs.

---

## Further Reading

- [The `rustdoc` Book](https://doc.rust-lang.org/rustdoc/): the authoritative guide to doc comments, attributes, and doctests.
- [How to write documentation (rustdoc book)](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html): conventions, summary lines, and section headings.
- [Linking to items by name (intra-doc links)](https://doc.rust-lang.org/rustdoc/write-documentation/linking-to-items-by-name.html): the full rules for `` [`Foo`] `` resolution.
- [Documentation tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html): fence attributes (`no_run`, `should_panic`, `ignore`, hidden lines).
- [docs.rs about page](https://docs.rs/about): how automatic doc hosting and the `[package.metadata.docs.rs]` table work.
- Related guide sections: [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/) (the crates you document against), [Logging with the `log` Facade and `env_logger`](/23-ecosystem/03-logging/) and [Structured Logging and Spans with `tracing`](/23-ecosystem/04-tracing/) (observability), and [Tooling](/24-tooling/) (formatting, linting, and CI that should run `cargo test --doc`).
- Foundations: [Introduction](/00-introduction/), [Understanding Cargo](/01-getting-started/03-cargo-basics/), and [Comments and Documentation](/02-basics/03-comments/) for the comment syntax this page builds on.

---

## Exercises

### Exercise 1: Document a method and run the doctest

**Difficulty:** Beginner

**Objective:** Write a doc comment with a runnable `# Examples` block and confirm `cargo test` executes it.

**Instructions:** In a `cargo new --lib geo` project, define a `Point { x: f64, y: f64 }` struct with a `distance(&self, other: &Point) -> f64` method. Document the struct, both fields, and the method. The method's doc comment must contain an `# Examples` block that constructs two points and asserts the distance between `(0,0)` and `(3,4)` is `5.0`. Run `cargo test` and confirm one doctest passes.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
//! Geometry helpers.

/// A point on the 2-D plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// The x coordinate.
    pub x: f64,
    /// The y coordinate.
    pub y: f64,
}

impl Point {
    /// The Euclidean distance from this point to `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use geo::Point;
    /// let a = Point { x: 0.0, y: 0.0 };
    /// let b = Point { x: 3.0, y: 4.0 };
    /// assert_eq!(a.distance(&b), 5.0);
    /// ```
    pub fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}
```

`cargo test` reports the doctest running and passing:

```text
   Doc-tests geo

running 1 test
test src/lib.rs - Point::distance (line 18) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

</details>

### Exercise 2: A doctest that uses `?`

**Difficulty:** Intermediate

**Objective:** Write a fallible function whose doctest uses the `?` operator, handling the implicit-`main`-returns-`()` problem.

**Instructions:** Add a free function `parse_point(s: &str) -> Result<Point, std::num::ParseFloatError>` that parses `"x,y"` (for example `"3,4"`) into a `Point`. Its `# Examples` block must call `parse_point("3,4")?` and assert the parsed `x` and `y`. Make the doctest compile and pass even though it uses `?`.

<details>
<summary>Solution</summary>

The trick is to end the example with a hidden line that gives `main` a `Result` return type. The `# ` line is compiled but hidden from the rendered docs.

```rust
/// Parses a `"x,y"` pair into a [`Point`].
///
/// # Examples
///
/// ```
/// use geo::parse_point;
/// let p = parse_point("3,4")?;
/// assert_eq!(p.x, 3.0);
/// assert_eq!(p.y, 4.0);
/// # Ok::<(), std::num::ParseFloatError>(())
/// ```
pub fn parse_point(s: &str) -> Result<Point, std::num::ParseFloatError> {
    let (x, y) = s.split_once(',').unwrap_or((s, ""));
    Ok(Point { x: x.trim().parse()?, y: y.trim().parse()? })
}
```

Both doctests pass:

```text
running 2 tests
test src/lib.rs - parse_point (line 33) ... ok
test src/lib.rs - Point::distance (line 18) ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

Note `` [`Point`] `` in the summary line: an intra-doc link to the struct, which `rustdoc` resolves and verifies.

</details>

### Exercise 3: Make undocumented public items a hard error

**Difficulty:** Advanced

**Objective:** Turn missing documentation into a build failure, the way a strict CI gate would, and observe the real diagnostic.

**Instructions:** Add `#![deny(missing_docs)]` to the crate root of your `geo` crate, then add a `pub fn` with *no* doc comment. Run `cargo doc --no-deps` and confirm the build fails. Then document the function and confirm the build succeeds again.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
//! Geometry helpers.
#![deny(missing_docs)]

// ... Point and parse_point as before, all documented ...

// This undocumented public function makes the build fail:
pub fn undocumented() {}
```

`cargo doc --no-deps` now errors (real output):

```text
error: missing documentation for a function
   --> src/lib.rs:...
    |
    | pub fn undocumented() {}
    | ^^^^^^^^^^^^^^^^^^^^^
error: could not document `geo`
```

Adding a doc comment fixes it:

```rust
/// A placeholder public function, now documented.
pub fn undocumented() {}
```

With `#![deny(missing_docs)]` in place, every new public item must carry a doc comment or the crate will not build — a far stronger guarantee than any JSDoc linter, because it is enforced by the compiler itself. Many published crates start with `#![warn(missing_docs)]` while filling gaps, then upgrade to `deny` once clean.

</details>
