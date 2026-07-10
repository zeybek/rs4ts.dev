---
title: "Linting with Clippy"
description: "Clippy is Rust's ESLint, but built in and type-aware: 750+ lints, the allow/warn/deny levels you know, lint groups, the [lints] table, and gating it in CI."
---

If ESLint is the tool that catches your bugs and bad habits in JavaScript and TypeScript, **Clippy** is its Rust counterpart. It ships with the toolchain, knows hundreds of idiomatic-Rust rules out of the box, and uses the exact same lint-level vocabulary (`allow` / `warn` / `deny`) you already know from ESLint and the compiler.

---

## Quick Overview

**Clippy** is Rust's official linter. It runs the same front-end as the compiler but adds 750+ extra lints that catch correctness bugs, performance traps, and non-idiomatic code. Unlike ESLint (which you install, configure, and wire into your build), Clippy comes with `rustup`, needs zero config to be useful, and its suggestions are frequently auto-applicable. For a TypeScript developer, the mental model is "ESLint, but built in, type-aware by default, and with a much higher signal-to-noise ratio."

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition. Clippy is versioned in lockstep with the compiler, so `rustc 1.96.0` ships `clippy 0.1.96`. `cargo new` selects the newest edition automatically.

---

## TypeScript/JavaScript Example

In the Node ecosystem, linting is a separate tool you opt into. A typical setup installs ESLint, writes a flat config (`eslint.config.js`), assigns each rule a level, and runs it through an npm script:

```javascript
// eslint.config.js  (ESLint 9 flat config)
module.exports = [
  {
    rules: {
      "no-var": "error", // hard failure
      eqeqeq: "warn", // advisory
      "no-unused-vars": "off", // disabled
    },
  },
];
```

```javascript
// demo.js
var x = 1;
if (x == "1") {
  console.log("loose equality");
}
```

Running `npx eslint demo.js` produces:

```text
/private/tmp/eslint_probe/demo.js
  1:1  error    Unexpected var, use let or const instead  no-var
  2:7  warning  Expected '===' and instead saw '=='       eqeqeq

2 problems (1 error, 1 warning)
  1 error and 0 warnings potentially fixable with the `--fix` option.
```

Note the three rule levels (`"error"`, `"warn"`, `"off"`), the `--fix` flag for auto-fixable rules, and the fact that **none of this works until you install ESLint and write a config**. Those three concepts map almost one-to-one onto Clippy.

---

## Rust Equivalent

Clippy needs no installation in a standard `rustup` setup and no config file to start producing value. Write some non-idiomatic Rust:

```rust playground
fn main() {
    let numbers = vec![1, 2, 3];
    if numbers.len() == 0 {
        println!("empty");
    }

    let opt: Option<i32> = Some(5);
    if opt.is_some() {
        let value = opt.unwrap();
        println!("{value}");
    }
}
```

Then run `cargo clippy`:

```text
warning: length comparison to zero
 --> src/main.rs:3:8
  |
3 |     if numbers.len() == 0 {
  |        ^^^^^^^^^^^^^^^^^^ help: using `is_empty` is clearer and more explicit: `numbers.is_empty()`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#len_zero
  = note: `#[warn(clippy::len_zero)]` on by default

warning: called `unwrap` on `opt` after checking its variant with `is_some`
  --> src/main.rs:8:21
   |
7  |     if opt.is_some() {
   |     ---------------- help: try: `if let Some(<item>) = opt`
8  |         let value = opt.unwrap();
   |                     ^^^^^^^^^^^^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unnecessary_unwrap
   = note: `#[warn(clippy::unnecessary_unwrap)]` on by default

warning: useless use of `vec!`
 --> src/main.rs:2:19
  |
2 |     let numbers = vec![1, 2, 3];
  |                   ^^^^^^^^^^^^^ help: you can use an array directly: `[1, 2, 3]`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#useless_vec
  = note: `#[warn(clippy::useless_vec)]` on by default
```

Three lints fired, each pointing at the exact span, naming the rule (`clippy::len_zero`, `clippy::unnecessary_unwrap`, `clippy::useless_vec`), suggesting the fix, and linking to its documentation, all with zero configuration.

> **Tip:** Every Clippy lint is documented and searchable in [The Clippy Lints index](https://rust-lang.github.io/rust-clippy/master/index.html). The `#name` fragment in each `help:` URL jumps straight to that lint.

---

## Detailed Explanation

### Installing and running

If you installed Rust via `rustup` (the recommended path), Clippy is already there as the `clippy` component. If it is somehow missing:

```bash
rustup component add clippy
```

You run it through Cargo, exactly like `cargo build` or `cargo test`:

```bash
cargo clippy
```

Under the hood, `cargo clippy` invokes a special Clippy driver in place of `rustc`. Because it reuses the compiler's own analysis (full type information, borrow checking, the works), Clippy is **type-aware by default**. ESLint only becomes type-aware when you add the `typescript-eslint` typed-linting rules and point it at your `tsconfig.json`; Clippy gets that for free because it *is* the compiler front-end.

A subtle but important consequence: `cargo build` and `cargo check` do **not** run Clippy lints. The clean-build output for the same code above is just:

```text
   Compiling clippy_probe v0.1.0 (/path/to/clippy_probe)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
```

No lint warnings at all. Clippy is a separate pass you (or your CI) must invoke explicitly. This is different from ESLint, which most teams wire into a pre-commit hook or `npm run build`, but it parallels how you have to run `eslint` itself: `tsc` alone never runs ESLint rules either.

### The three lint levels

Every Rust lint, both the compiler's own and Clippy's, sits at one of four levels. Three of them line up directly with ESLint:

| ESLint level | Rust/Clippy level | Effect                                                       |
| ------------ | ----------------- | ------------------------------------------------------------ |
| `"off"`      | `allow`           | Lint is silenced; no diagnostic.                             |
| `"warn"`     | `warn`            | Prints a warning; compilation still succeeds.                |
| `"error"`    | `deny`            | Prints an error; **compilation fails** (non-zero exit code). |
| (none)       | `forbid`          | Like `deny`, but can no longer be overridden by an `allow`.  |

`forbid` has no ESLint analogue. It is `deny` that downstream code is *forbidden* from re-allowing. Use it sparingly for lints you consider non-negotiable.

### Where Clippy lints come from: lint groups

ESLint rules are individually named (`no-var`, `eqeqeq`). Clippy lints are individually named too (`clippy::len_zero`), but they are also organized into **groups** you can target wholesale, much like an ESLint *config preset* such as `eslint:recommended`:

| Clippy group         | Default level | What it contains                                                       |
| -------------------- | ------------- | ---------------------------------------------------------------------- |
| `clippy::all`        | `warn`        | The sensible default set: `correctness`, `suspicious`, `style`, `complexity`, `perf`. This is what runs out of the box. |
| `clippy::correctness`| `deny`        | Almost-certainly-wrong code. On by default and already denied.         |
| `clippy::pedantic`   | `allow`       | Stricter, sometimes opinionated lints. Off by default; opt in.         |
| `clippy::nursery`    | `allow`       | New lints still being refined. May have false positives.               |
| `clippy::cargo`      | `allow`       | Lints about your `Cargo.toml` metadata.                                |
| `clippy::restriction`| `allow`       | A grab-bag of lints that are *not* recommended wholesale — enable individual ones only. |

The key insight: `clippy::all` is the curated, high-signal default. `clippy::pedantic` is the "I want the strict opinions too" preset, comparable to enabling a strict ESLint shared config like `airbnb`. You opt into it explicitly:

```bash
cargo clippy -- -W clippy::pedantic
```

Anything after `--` is passed to the Clippy/`rustc` driver. With `pedantic` on, the same code can surface extra advice:

```text
warning: this argument is passed by value, but not consumed in the function body
 --> src/main.rs:1:18
  |
1 | fn process(data: String) -> usize {
  |                  ^^^^^^ help: consider changing the type to: `&str`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_pass_by_value
  = note: `-W clippy::needless-pass-by-value` implied by `-W clippy::pedantic`
```

> **Warning:** Do not blanket-`deny` `clippy::pedantic`, `clippy::nursery`, or `clippy::restriction` in a real project. They contain opinionated and occasionally noisy lints; a hard deny on them turns every Clippy upgrade into a potential build breakage. Set them to `warn` if you want the advice, and `deny` only the curated `clippy::all`.

### Controlling levels: three layers

You can set lint levels in three places, in increasing order of precedence (the innermost wins):

1. **Cargo.toml `[lints]` table**: project-wide, version-controlled, the modern default.
2. **Command-line / crate-root attributes**, e.g. `-W clippy::pedantic` or `#![deny(clippy::all)]`.
3. **Item-level attributes**, e.g. `#[allow(clippy::useless_vec)]` on a single statement, like an ESLint `// eslint-disable-next-line` comment.

The next two sections cover each. They are exactly the ESLint pattern of "config file → CLI flag → inline disable comment," just expressed in Rust syntax.

---

## Key Differences

### `#![deny(clippy::all)]` — promoting warnings to hard errors

By default Clippy *warns*. To make lints fail the build (the Rust equivalent of an ESLint rule set to `"error"`), promote them to `deny`. The most common gate is a crate-root inner attribute:

```rust playground
#![deny(clippy::all)]

fn main() {
    let numbers = vec![1, 2, 3];
    if numbers.len() == 0 {
        println!("empty");
    }
}
```

Now `cargo clippy` reports **errors**, not warnings, and exits non-zero:

```text
error: length comparison to zero
 --> src/main.rs:5:8
  |
5 |     if numbers.len() == 0 {
  |        ^^^^^^^^^^^^^^^^^^ help: using `is_empty` is clearer and more explicit: `numbers.is_empty()`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#len_zero
note: the lint level is defined here
 --> src/main.rs:1:9
  |
1 | #![deny(clippy::all)]
  |         ^^^^^^^^^^^
  = note: `#[deny(clippy::len_zero)]` implied by `#[deny(clippy::all)]`

error: useless use of `vec!`
 --> src/main.rs:4:19
  |
4 |     let numbers = vec![1, 2, 3];
  |                   ^^^^^^^^^^^^^ help: you can use an array directly: `[1, 2, 3]`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#useless_vec
  = note: `#[deny(clippy::useless_vec)]` implied by `#[deny(clippy::all)]`

error: could not compile `clippy_probe` (bin "clippy_probe") due to 2 previous errors
```

The process exits with code `101`. That non-zero exit is what makes Clippy a real gate in CI: the job fails, the PR is blocked.

> **Note:** The `#![...]` form (with the bang) is an *inner* attribute: it applies to the whole crate from the crate root (`main.rs` or `lib.rs`). The `#[...]` form (no bang) is an *outer* attribute that applies to the next item. Both are real Rust attributes and render correctly in plain markdown; they are unrelated to the rustdoc hidden-line `# ` syntax.

### `-D warnings` — the CI one-liner

The crate attribute lives in source. For CI, the common pattern is to leave the code clean and flip *every* warning to deny from the command line:

```bash
cargo clippy -- -D warnings
```

`-D` is the short form of `deny`, and `warnings` is a special lint group meaning "all warnings." This is the single most common Clippy invocation in CI pipelines: any lint at `warn` becomes a build-breaking error.

```text
error: useless use of `vec!`
 --> src/main.rs:2:19
  |
2 |     let numbers = vec![1, 2, 3];
  |                   ^^^^^^^^^^^^^ help: you can use an array directly: `[1, 2, 3]`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#useless_vec
  = note: `-D clippy::useless-vec` implied by `-D warnings`
  = help: to override `-D warnings` add `#[allow(clippy::useless_vec)]`

error: could not compile `clippy_probe` (bin "clippy_probe") due to 1 previous error
```

The difference from `#![deny(clippy::all)]`: `-D warnings` denies *everything* that is currently a warning (including plain `rustc` warnings and any `pedantic` lints you enabled), while `#![deny(clippy::all)]` denies only the curated Clippy group. Most teams use *both*: a `[lints]` table for the project's baseline and `-D warnings` in CI as a belt-and-suspenders gate.

### The modern config home: the `[lints]` table

The current-stable way to configure lints project-wide is the `[lints]` table in `Cargo.toml` (stable since Cargo 1.74). This is the closest thing to an `eslint.config.js`: version-controlled, applies to every `cargo clippy` invocation, and works across a workspace.

```toml
[package]
name = "clippy_probe"
version = "0.1.0"
edition = "2024"

[lints.clippy]
all = { level = "deny", priority = -1 }
unwrap_used = "warn"
```

```rust playground
fn main() {
    let numbers = vec![1, 2, 3];
    println!("{}", numbers.len());
}
```

A plain `cargo clippy` now fails because the table denied `clippy::all`:

```text
error: useless use of `vec!`
 --> src/main.rs:2:19
  |
2 |     let numbers = vec![1, 2, 3];
  |                   ^^^^^^^^^^^^^ help: you can use an array directly: `[1, 2, 3]`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#useless_vec
  = note: `-D clippy::useless-vec` implied by `-D clippy::all`
  = help: to override `-D clippy::all` add `#[allow(clippy::useless_vec)]`

error: could not compile `clippy_probe` (bin "clippy_probe") due to 1 previous error
```

> **Tip:** The `priority = -1` on a *group* is essential. Cargo applies lints in priority order (lower numbers first), so a group at `priority = -1` is applied *before* individual lints at the default `priority = 0`. That lets a specific lint override the group, e.g. `all` denied at `-1`, then a single lint relaxed to `warn` at `0`. Forget the negative priority and Cargo will reject the manifest with an ambiguous-ordering error.

For a multi-crate workspace, declare the lints once at the workspace root and inherit them per-crate. See [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/) for the `[workspace.lints]` + `[lints] workspace = true` pattern.

---

## Common Pitfalls

### Pitfall 1: Expecting `cargo build` to run Clippy

A TypeScript developer used to ESLint running inside `npm run build` may assume `cargo build` lints. It does not. `cargo build` and `cargo check` only run the compiler's own (much smaller) lint set. Clippy's extra lints appear **only** when you run `cargo clippy`. Always add a dedicated `cargo clippy` step to your scripts and CI.

### Pitfall 2: Suppressing a lint by deleting it instead of `allow`-ing it

Sometimes a lint is wrong for your situation. The fix is not to disable Clippy globally; it is to `allow` that one lint at the narrowest scope, the way you would write `// eslint-disable-next-line`:

```rust playground
fn main() {
    #[allow(clippy::useless_vec)]
    let numbers = vec![1, 2, 3];
    println!("{}", numbers.len());
}
```

That produces a clean run (exit `0`), and the suppression is documented in place. Prefer item-level `#[allow]` over crate-level `#![allow]`: the broad version silences the lint everywhere and hides future real hits.

### Pitfall 3: `unwrap` survives Clippy by default

New Rustaceans often expect Clippy to flag every `.unwrap()`. It does not by default; `.unwrap()` is fine in tests, prototypes, and provably-safe spots. The `clippy::unwrap_used` lint that bans it lives in the `restriction` group and is `allow` by default. If your team wants to forbid `unwrap` in production code, opt in explicitly (e.g. `unwrap_used = "warn"` in the `[lints]` table, as shown earlier). Clippy *will* warn when an `.unwrap()` is provably pointless (e.g. right after an `is_some()` check, via `clippy::unnecessary_unwrap`), but it will not police all of them. See [Error Handling](/08-error-handling/) for when `unwrap` is appropriate.

### Pitfall 4: Blanket-denying `pedantic` and getting surprised by upgrades

`#![deny(clippy::pedantic)]` feels rigorous, but `pedantic` is opinionated and grows with every release. A toolchain bump can introduce a new pedantic lint that suddenly fails your build on code that did not change. Keep `pedantic`/`nursery` at `warn`, and reserve `deny` for `clippy::all` (or specific lints you have deliberately chosen).

### Pitfall 5: Forgetting `--all-targets` / `--all-features`

`cargo clippy` by default lints the default target set and default features only. Code behind `#[cfg(test)]`, in benches, in examples, or behind non-default feature flags goes unlinted. For a thorough check (and the standard CI invocation), use:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

---

## Best Practices

- **Run `cargo clippy` locally before every push**, and gate it in CI with `cargo clippy --all-targets --all-features -- -D warnings`. A green Clippy run should be a merge requirement, just like a passing test suite.
- **Put the project baseline in the `[lints]` table** of `Cargo.toml` (or `[workspace.lints]` for a workspace) so the policy is version-controlled and applies to every contributor and editor. [rust-analyzer](/24-tooling/05-rust-analyzer/) reads it and surfaces the same diagnostics inline.
- **Deny the curated group, warn the opinionated ones:**

  ```toml
  [lints.clippy]
  all = { level = "deny", priority = -1 }
  pedantic = { level = "warn", priority = -1 }
  ```

- **Use `cargo clippy --fix` for the mechanical ones.** Many lints are auto-applicable. Given this code:

  ```rust playground
  fn main() {
      let numbers = vec![1, 2, 3];
      if numbers.len() == 0 {
          println!("empty");
      } else {
          println!("{} items", numbers.len());
      }
  }
  ```

  Running `cargo clippy --fix` (add `--allow-no-vcs` outside a git repo) rewrites it in place to the idiomatic form:

  ```rust playground
  fn main() {
      let numbers = [1, 2, 3];
      if numbers.is_empty() {
          println!("empty");
      } else {
          println!("{} items", numbers.len());
      }
  }
  ```

  This is Clippy's `--fix`, the direct analogue of `eslint --fix`. It only applies suggestions Clippy marks as machine-applicable, so it is safe to run and review as a diff.

- **Prefer `#[expect(...)]` over `#[allow(...)]` for intentional suppressions** (stable since Rust 1.81). `expect` behaves like `allow`, but if the lint *stops* firing, Clippy warns that the expectation is now unfulfilled, so a suppression you no longer need does not silently rot:

  ```rust playground
  fn main() {
      // If the `vec!` were later changed to an array, `useless_vec` would no
      // longer fire, and Clippy would flag this attribute as unnecessary.
      #[expect(clippy::useless_vec)]
      let numbers = vec![1, 2, 3];
      println!("{}", numbers.len());
  }
  ```

  When the lint is no longer triggered, you get a helpful nudge instead of dead config:

  ```text
  warning: this lint expectation is unfulfilled
   --> src/main.rs:4:14
    |
  4 |     #[expect(clippy::useless_vec)]
    |              ^^^^^^^^^^^^^^^^^^^
    |
    = note: `#[warn(unfulfilled_lint_expectations)]` on by default
  ```

- **Document *why* a lint is suppressed** with the `reason` field (stable since 1.81), the equivalent of a comment after an ESLint disable directive:

  ```rust
  #[allow(clippy::useless_vec, reason = "grows at runtime")]
  let mut numbers = vec![1, 2, 3];
  ```

---

## Real-World Example

A production crate typically sets a strict-but-sane lint policy in `Cargo.toml`, then keeps the code clean enough to pass it. Here is a small order-total module configured to **deny `clippy::all` and warn on `pedantic`**, and written to satisfy both.

```toml
[package]
name = "clippy_probe"
version = "0.1.0"
edition = "2024"

[lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }

[dependencies]
```

```rust playground
//! A small order-total calculator that passes a strict Clippy config.

/// A line item in a shopping cart.
#[derive(Debug, Clone)]
pub struct LineItem {
    pub name: String,
    pub unit_price_cents: u32,
    pub quantity: u32,
}

/// Returns the total price in cents, applying a percentage discount.
///
/// # Panics
/// Never panics: arithmetic uses `u64` accumulators to avoid overflow.
#[must_use]
pub fn cart_total_cents(items: &[LineItem], discount_percent: u8) -> u64 {
    let subtotal: u64 = items
        .iter()
        .map(|item| u64::from(item.unit_price_cents) * u64::from(item.quantity))
        .sum();

    let discount = subtotal * u64::from(discount_percent) / 100;
    subtotal - discount
}

fn main() {
    let cart = [
        LineItem { name: "Keyboard".to_string(), unit_price_cents: 4_999, quantity: 1 },
        LineItem { name: "Mouse".to_string(), unit_price_cents: 2_500, quantity: 2 },
    ];

    let total = cart_total_cents(&cart, 10);
    println!("Total: ${}.{:02}", total / 100, total % 100);
}
```

`cargo clippy` passes with no diagnostics, and the program runs:

```text
    Checking clippy_probe v0.1.0 (/path/to/clippy_probe)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.05s
```

```text
Total: $90.00
```

Notice the idioms that keep `pedantic` quiet: `#[must_use]` on a pure function, `u64::from(...)` instead of an `as` cast (the pedantic `cast_lossless` lint prefers `From`), taking `&[LineItem]` rather than `Vec<LineItem>` by value, and a `# Panics` doc section. This is what "Clippy-clean production Rust" looks like — the linter nudges you toward these conventions, and once internalized they become automatic. For a guided tour of the specific lints and their before/after rewrites, see [Common Clippy Lints](/24-tooling/03-clippy-lints/).

---

## Further Reading

- [The Clippy Book](https://doc.rust-lang.org/clippy/): official guide to configuring and running Clippy.
- [Clippy Lints index](https://rust-lang.github.io/rust-clippy/master/index.html): searchable list of every lint, its group, and its default level.
- [The `rustc` lint levels reference](https://doc.rust-lang.org/rustc/lints/levels.html) — how `allow`/`warn`/`deny`/`forbid` and `-D warnings` work at the compiler level.
- [Cargo `[lints]` table reference](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section): the modern, version-controlled lint config home.
- [Common Clippy Lints](/24-tooling/03-clippy-lints/): the most-encountered lints (`needless_clone`, `uninlined_format_args`, and friends) explained with before/after code.
- [Prettier to rustfmt](/24-tooling/01-formatting/) — the formatting half of the toolchain; pairs with Clippy in CI.
- [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/): workspace-wide lint inheritance via `[workspace.lints]`.
- [rust-analyzer](/24-tooling/05-rust-analyzer/): surfaces Clippy diagnostics live in your editor.
- [CI/CD for Rust](/24-tooling/07-ci-cd/) and [GitHub Actions](/24-tooling/08-github-actions/) — wiring `cargo clippy -- -D warnings` into a pipeline.
- [Understanding Cargo](/01-getting-started/03-cargo-basics/): the one-tool philosophy that bundles Clippy.
- [Error Handling](/08-error-handling/): context for the `unwrap_used` lint and when `unwrap` is fine.
- [Advanced Topics](/25-advanced-topics/): deeper compiler and lint internals.

---

## Exercises

### Exercise 1: Run and read Clippy

**Difficulty:** Beginner

**Objective:** Get comfortable invoking Clippy and interpreting its output.

**Instructions:** Create a new project with `cargo new lint_practice`. Paste the following into `src/main.rs`, run `cargo clippy`, and identify which lint name fires and what fix it suggests. Then apply the fix and confirm a clean run.

```rust playground
fn main() {
    let words = vec!["alpha", "beta", "gamma"];
    let count = words.iter().count();
    println!("{} words", count);
}
```

<details>
<summary>Solution</summary>

`cargo clippy` reports `clippy::needless_collect`-adjacent advice, specifically the `clippy::iter_count` lint, which notes that calling `.iter().count()` is needlessly indirect when `.len()` exists. The real output:

```text
warning: called `.iter().count()` on a `Vec`
 --> src/main.rs:3:17
  |
3 |     let count = words.iter().count();
  |                 ^^^^^^^^^^^^^^^^^^^^ help: try: `words.len()`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#iter_count
  = note: `#[warn(clippy::iter_count)]` on by default
```

The fixed, Clippy-clean version:

```rust playground
fn main() {
    let words = ["alpha", "beta", "gamma"];
    let count = words.len();
    println!("{count} words");
}
```

(Clippy also flags the `vec!` as a `useless_vec` since the collection is never mutated; switching to an array `[...]` clears that too.)

</details>

### Exercise 2: Gate the build with a lint policy

**Difficulty:** Intermediate

**Objective:** Configure a project so a specific anti-pattern fails the build, while documenting one intentional exception.

**Instructions:** In a `Cargo.toml`, add a `[lints]` table that **denies** `clippy::all` and **warns** on `clippy::unwrap_used`. Then write `src/main.rs` that contains one `.unwrap()` you genuinely intend to keep (e.g. in a clearly-safe constant context), suppressing just that one lint with `#[expect(...)]` and a `reason`. Confirm `cargo clippy` exits `0`.

<details>
<summary>Solution</summary>

```toml
[package]
name = "lint_practice"
version = "0.1.0"
edition = "2024"

[lints.clippy]
all = { level = "deny", priority = -1 }
unwrap_used = "warn"

[dependencies]
```

```rust playground
fn main() {
    // A parse that cannot fail on a hard-coded literal. We expect the
    // `unwrap_used` warning here and document why it is acceptable.
    #[expect(clippy::unwrap_used, reason = "literal is a valid u32")]
    let port: u32 = "8080".parse().unwrap();

    println!("Listening on port {port}");
}
```

`cargo clippy` exits `0` with no diagnostics: `clippy::all` is satisfied (no useless `vec!`, idiomatic formatting), and the single `unwrap_used` warning is silenced by the `#[expect]`, whose expectation is fulfilled because the lint *does* fire there. If you later remove the `.unwrap()`, Clippy will warn that the expectation is unfulfilled, prompting you to drop the now-stale attribute.

</details>

### Exercise 3: Opt into pedantic and resolve a real finding

**Difficulty:** Advanced

**Objective:** Experience the stricter `pedantic` preset and refactor toward idiomatic Rust.

**Instructions:** Take the function below, run `cargo clippy -- -W clippy::pedantic`, and resolve the pedantic finding by changing the signature — without changing what the function does. (Hint: the lint is about taking ownership you do not need.)

```rust playground
fn shout(message: String) -> String {
    message.to_uppercase()
}

fn main() {
    let msg = String::from("ship it");
    println!("{}", shout(msg));
}
```

<details>
<summary>Solution</summary>

With `pedantic` enabled, Clippy fires `clippy::needless_pass_by_value`: `shout` takes a `String` by value but only reads it, so it forces every caller to give up ownership for no reason. The fix is to accept a borrow, `&str`, which also makes the function callable with `&String`, `&str`, and string literals alike:

```rust playground
fn shout(message: &str) -> String {
    message.to_uppercase()
}

fn main() {
    let msg = String::from("ship it");
    println!("{}", shout(&msg));
    // `msg` is still usable here because we only borrowed it.
    println!("original still owned: {msg}");
}
```

`cargo clippy -- -W clippy::pedantic` now reports no warnings. This `&str`-over-`String` parameter convention is one of the most common pieces of advice the pedantic group teaches; see [Ownership](/05-ownership/) for the deeper "borrow what you only read" principle behind it.

</details>
