---
title: "CI/CD Concepts for Rust"
description: "Rust CI runs four gates like a Node pipeline: cargo fmt, clippy, test, and build. The new concern is caching the compiled target/ directory, not just a download."
---

## Quick Overview

Continuous Integration for a Rust project is built from the same idea you already use in Node.js: run a series of fast, deterministic checks on every push and block the merge if any of them fail. The difference is *which* checks and *how you cache*. A typical Rust pipeline has four gates — **format**, **lint**, **test**, **build** — that map almost one-to-one onto `prettier --check`, `eslint`, `jest`, and `tsc`/`vite build`. The single biggest practical concern unique to Rust CI is caching the `target/` directory, because a cold compile of your dependency tree can take minutes where `node_modules` is just a download.

> **Note:** The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. Every command in this topic (`cargo fmt`, `cargo clippy`, `cargo test`, `cargo build`) ships with that toolchain: there is no separate test runner or bundler to install, which keeps the CI config small.

This topic covers the *concepts*: the gates, their exit codes, and caching strategy. The concrete [GitHub Actions](/24-tooling/08-github-actions/) workflow (matrix, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`) and the [Docker](/24-tooling/09-docker/) build (multi-stage, `cargo-chef`) are their own topics.

---

## TypeScript/JavaScript Example

A mature Node.js project wires its quality gates into `package.json` scripts, then a CI provider runs them in order. The scripts are the contract; CI just invokes them.

```jsonc
// package.json (excerpt)
{
  "scripts": {
    "format:check": "prettier --check .",
    "lint": "eslint . --max-warnings 0",
    "test": "vitest run --coverage",
    "build": "tsc --noEmit && vite build"
  },
  "devDependencies": {
    "prettier": "^3.4.2",
    "eslint": "^9.18.0",
    "vitest": "^3.0.5",
    "vite": "^6.0.7",
    "typescript": "^5.7.3"
  }
}
```

A minimal GitHub Actions workflow installs Node, restores the npm cache, installs dependencies, then runs each gate:

```yaml
# .github/workflows/ci.yml (Node.js)
name: ci
on: [push, pull_request]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm # caches ~/.npm based on package-lock.json
      - run: npm ci # reproducible install from package-lock.json
      - run: npm run format:check
      - run: npm run lint
      - run: npm run test
      - run: npm run build
```

Two details matter for the Rust comparison. First, `npm ci` is the *reproducible* install: it installs exactly what `package-lock.json` pins and fails if the lockfile is out of sync. Second, the cache key is derived from `package-lock.json`, so the cache is reused until your dependencies change.

---

## Rust Equivalent

The same four gates in Rust are four `cargo` subcommands. There is no install-dependencies step that downloads a prebuilt `node_modules`: `cargo` fetches sources and *compiles* them as part of the first build, and the compiled artifacts live in `target/`. That is exactly what you cache.

```yaml
# .github/workflows/ci.yml (Rust)
name: ci
on: [push, pull_request]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2 # caches ~/.cargo and target/
      - run: cargo fmt --all --check # gate 1: formatting (≈ prettier --check)
      - run: cargo clippy --all-targets --all-features -- -D warnings # gate 2: lint (≈ eslint)
      - run: cargo test --all-features --workspace # gate 3: tests (≈ vitest run)
      - run: cargo build --release --locked # gate 4: release build (≈ vite build)
```

Each command is a real, self-contained quality gate. Here is what each one prints and the exit code CI keys off, all captured from a real crate with a `slugify` function and two unit tests.

**Gate 1: formatting.** `cargo fmt --all --check` writes nothing; it prints a diff and exits non-zero if any file is unformatted (the formatter itself is covered in [Formatting with rustfmt](/24-tooling/01-formatting/)):

```text
$ cargo fmt --all --check     # on unformatted code
Diff in /tmp/probe/src/lib.rs:1:
-pub fn double(x:i32)->i32{x*2}
+pub fn double(x: i32) -> i32 {
+    x * 2
+}
$ echo $?
1
```

On clean code it prints nothing and exits `0`.

**Gate 2 — lint.** `cargo clippy ... -- -D warnings` turns every Clippy warning into a hard error, so a single lint fails the job (lint levels are covered in [ESLint to Clippy](/24-tooling/02-linting/)):

```text
$ cargo clippy --all-targets -- -D warnings     # on code with a needless `return`
error: unneeded `return` statement
 --> src/lib.rs:2:5
  |
2 |     return x * 2;
  |     ^^^^^^^^^^^^
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_return
  = note: `-D clippy::needless-return` implied by `-D warnings`
  = help: to override `-D warnings` add `#[allow(clippy::needless_return)]`
help: remove `return`
  |
2 -     return x * 2;
2 +     x * 2
  |

error: could not compile `probe2` (lib) due to 1 previous error
$ echo $?
101
```

**Gate 3: tests.** `cargo test` compiles and runs unit tests, integration tests, and doctests, and exits non-zero if any fail:

```text
$ cargo test
running 2 tests
test tests::collapses_internal_whitespace ... ok
test tests::slugifies_basic_title ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

A failing assertion prints the real values and exits non-zero, just like a failing Vitest expectation:

```text
$ cargo test     # with a wrong expected value
running 1 test
test tests::adds ... FAILED

failures:

---- tests::adds stdout ----

thread 'tests::adds' panicked at src/lib.rs:11:9:
assertion `left == right` failed
  left: 4
 right: 5
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

**Gate 4 — build.** `cargo build --release --locked` produces the optimized binary you ship and asserts the lockfile is up to date:

```text
$ cargo build --release
   Compiling probe v0.1.0 (/tmp/probe)
    Finished `release` profile [optimized] target(s) in 0.23s
```

Every gate communicates pass/fail through the process exit code, so the CI runner stops the job on the first failure without any extra wiring, identical to how `npm run lint` failing aborts the Node.js pipeline.

---

## Detailed Explanation

### The four gates, ordered fastest-to-slowest

Order the gates so the cheapest check fails first. Formatting is nearly instant, Clippy and the build share most of their compilation work, and tests run last because they need everything compiled.

| # | Gate | Command | Node.js analogue | Exits non-zero when |
| --- | --- | --- | --- | --- |
| 1 | Format | `cargo fmt --all --check` | `prettier --check .` | Any file is not `rustfmt`-clean |
| 2 | Lint | `cargo clippy --all-targets --all-features -- -D warnings` | `eslint . --max-warnings 0` | Any Clippy lint fires (treated as error) |
| 3 | Test | `cargo test --all-features --workspace` | `vitest run` | Any test, doctest, or compile of a test target fails |
| 4 | Build | `cargo build --release --locked` | `tsc --noEmit && vite build` | Compilation fails or the lockfile is stale |

> **Tip:** Formatting genuinely is the fastest gate: it parses but does not type-check or codegen. Putting it first means a contributor who forgot to run `cargo fmt` gets a failure in seconds instead of after a multi-minute compile.

### Why `-- -D warnings` is the lint contract

By default `cargo clippy` *warns* but still exits `0`, so CI would pass even with lints present. The `-- -D warnings` part forwards `-D warnings` to the compiler driver, promoting every warning (Clippy's and `rustc`'s) to a hard error. That is the exact analogue of ESLint's `--max-warnings 0`: warnings you tolerate locally become blockers in CI. The `--` after the `clippy` flags separates Cargo's arguments from the arguments passed through to the lint driver.

`--all-targets` makes Clippy check your tests, examples, and benchmarks too — not just `src/`. `--all-features` enables every Cargo feature so feature-gated code is linted as well. Both widen coverage the same way a thorough ESLint config globs your whole repo rather than just `src/`.

### Tests cover three things Node.js splits across tools

A single `cargo test` run compiles and executes:

- **unit tests** — `#[test]` functions inside `#[cfg(test)] mod tests`, like Vitest `test()` blocks colocated with code;
- **integration tests** — every file in `tests/`, compiled as a separate crate against your public API;
- **doctests** — runnable code blocks in `///` documentation comments, which have no direct Node.js equivalent and double as compile-checked examples.

So `cargo test` alone covers ground that in Node.js needs Vitest plus a separate "are the README examples still valid?" check. The broader testing story is in [Testing](/13-testing/); here the point is that CI's test gate is one command.

### Reproducible builds: `--locked` is `npm ci`

`Cargo.lock` is the analogue of `package-lock.json` and you commit it for applications (libraries usually do not). Adding `--locked` to your CI commands makes Cargo refuse to modify the lockfile: if `Cargo.toml` and `Cargo.lock` disagree, the build fails instead of silently resolving new versions. That is precisely what `npm ci` guarantees over `npm install`. The real error when the lockfile is missing or stale is explicit:

```text
$ cargo build --locked
error: the lock file Cargo.lock needs to be updated but --locked was passed to prevent this
If you want to try to generate the lock file without accessing the network, remove the --locked flag and use --offline instead.
```

Two related flags appear in `cargo build --help`: `--offline` runs without touching the network (using only already-fetched crates), and `--frozen` is shorthand for both `--locked --offline`. Use `--locked` in CI so a dependency can never drift between the run that opened the PR and the run that merges it.

### Caching `target/`: the one thing that is different from Node.js

In Node.js, "dependencies" are downloaded, prebuilt JavaScript. Caching `~/.npm` (or `node_modules`) saves a download. In Rust, your dependencies are **compiled from source** into `target/`, so a cold CI run can spend minutes building crates you have not touched. Caching is therefore the difference between a 90-second CI run and a 6-minute one.

There are three layers worth caching, and the dedicated [GitHub Actions](/24-tooling/08-github-actions/) topic shows the `Swatinem/rust-cache@v2` action that handles all of this for you. Conceptually:

| What | Path | Why cache it |
| --- | --- | --- |
| Crate source registry & downloads | `~/.cargo/registry/`, `~/.cargo/git/` | Avoids re-downloading every dependency's source |
| Compiled dependency artifacts | `target/` (the dependency `.rlib`s under `target/<profile>/deps/`) | Avoids recompiling unchanged dependencies: the big win |
| Tool binaries | `~/.cargo/bin/` | Avoids reinstalling `cargo-nextest`, `cargo-audit`, etc. |

The correct **cache key** is derived from `Cargo.lock` (plus the Rust version and OS), exactly as the Node.js cache key is derived from `package-lock.json`. When the lockfile is unchanged, the cache restores and `cargo` only recompiles your own crate. When dependencies change, the key changes and the cache is rebuilt.

> **Warning:** Do **not** naively cache your *entire* `target/` directory across unrelated runs without a tool that understands Cargo's fingerprints. Stale, oversized caches can be slower to restore than a clean build, and Cargo may recompile anyway when its fingerprints do not match. `Swatinem/rust-cache` exists precisely because it caches the dependency artifacts intelligently and prunes your own (frequently-changing) crate output. Hand-rolling `actions/cache` over `target/` is the most common way teams get *slower* CI.

### Profiles: why CI builds twice

Your test and build gates use different Cargo profiles. `cargo test` and `cargo clippy` use the `dev` profile (unoptimized, fast to compile, with debug assertions on). `cargo build --release` uses the `release` profile (optimized, slow to compile). These produce **separate artifact directories** (`target/debug/` vs `target/release/`), so the cache holds both. This is why a Rust pipeline that runs both tests and a release build does meaningfully more compilation than a Node.js one, and why caching matters more. Profiles are covered in depth in [Cargo deep dive](/24-tooling/00-cargo-deep-dive/).

---

## Key Differences

| Concept | Node.js CI | Rust CI |
| --- | --- | --- |
| Install step | `npm ci` downloads prebuilt deps | No separate install; `cargo` compiles deps during the first build |
| Format gate | `prettier --check` | `cargo fmt --all --check` |
| Lint gate | `eslint --max-warnings 0` | `cargo clippy ... -- -D warnings` |
| Test gate | `vitest run` / `jest` | `cargo test` (incl. doctests) |
| Type-check | `tsc --noEmit` (separate step) | Folded into compilation — `cargo build`/`clippy` type-check |
| Build gate | `vite build` / `tsc` | `cargo build --release` |
| Reproducible install | `npm ci` (fails on stale lockfile) | `--locked` flag (fails on stale `Cargo.lock`) |
| What you cache | `~/.npm` / `node_modules` (downloads) | `~/.cargo/` **and** `target/` (compiled artifacts) |
| Cache key source | `package-lock.json` | `Cargo.lock` (+ toolchain + OS) |
| Cold-cache cost | Seconds (a download) | Minutes (a full compile) |
| Tooling install in CI | Many dev-dependencies | Toolchain only; `rustfmt`/`clippy` are components |

The two takeaways for a TypeScript developer: (1) there is no "install dependencies" phase distinct from building — fetching and compiling are the same step — and (2) because compilation, not downloading, dominates CI time, caching `target/` is the highest-impact optimization you will make.

---

## Common Pitfalls

### Pitfall 1: Forgetting `-- -D warnings`, so the lint gate never fails

Running plain `cargo clippy` in CI is the most common mistake. Clippy emits warnings but exits `0`, so the job is green even when lints fire: the opposite of what you intended. The lint gate only bites with the deny flag:

```bash
# passes CI even when lints fire — Clippy warns but exits 0
cargo clippy --all-targets --all-features

# a single lint fails the job (exits 101)
cargo clippy --all-targets --all-features -- -D warnings
```

This mirrors `eslint .` (warnings allowed) versus `eslint . --max-warnings 0`. Always use the deny form in CI.

### Pitfall 2: Caching all of `target/` by hand and getting slower CI

A TypeScript developer reasonably reaches for `actions/cache` pointed at `target/`, the way they would cache `node_modules`. But `target/` also contains your *own* crate's output, which changes on every commit, so the cache balloons and frequently misses. Cargo's fingerprinting may then recompile anyway. Use a Rust-aware cache (`Swatinem/rust-cache@v2`) that caches dependency artifacts and discards your fast-changing crate output; see [GitHub Actions](/24-tooling/08-github-actions/).

### Pitfall 3: Not running `--locked`, so CI silently upgrades dependencies

Without `--locked`, `cargo build` will happily update `Cargo.lock` to newer compatible versions when it sees fit, meaning the code that merges might depend on different crate versions than the code that was reviewed. This is the `npm install` vs `npm ci` trap. Commit `Cargo.lock` for applications and pass `--locked` in every CI command.

### Pitfall 4: Tests pass locally but the cache hides a stale build

If your hand-rolled cache restores a `target/` that does not match the current `Cargo.lock`, Cargo may reuse stale artifacts and you can get confusing results. Keying the cache on `Cargo.lock` (and the toolchain version) prevents this. The lock-file-keyed cache is also what makes a green CI run trustworthy: same lockfile, same compiled dependencies.

### Pitfall 5: Stopping tests at the first failure when you wanted the full picture

By default `cargo test` stops the *test binary* on the first failing test target. For CI dashboards you often want every failure listed. `cargo test --no-fail-fast` runs all tests regardless of failures (it appears in `cargo test --help`), so a single report shows everything that is broken rather than just the first thing:

```bash
cargo test --workspace --all-features --no-fail-fast
```

This is the analogue of running your test runner without `--bail`.

### Pitfall 6: Assuming a separate type-check step is needed

There is no `tsc --noEmit` equivalent to add. Type checking in Rust happens *during compilation*, so `cargo clippy`, `cargo test`, and `cargo build` all type-check as a side effect. If you want a fast type-check-only gate without producing a binary, use `cargo check`: it runs the front end (parsing, type-checking, borrow-checking) and skips codegen, making it the closest thing to `tsc --noEmit`:

```bash
cargo check --all-targets --all-features --locked
```

---

## Best Practices

- **Run the four gates in fastest-first order:** `fmt --check`, then `clippy -- -D warnings`, then `test`, then `build --release`. Cheap failures should surface in seconds.
- **Deny warnings in CI, not locally.** Keep `-- -D warnings` (and a `#![deny(...)]` policy from [Linting](/24-tooling/02-linting/)) in the pipeline so contributors are not blocked mid-edit but nothing warning-y ever merges.
- **Always pass `--locked`** in CI commands and commit `Cargo.lock` for applications. This is your `npm ci` guarantee.
- **Cache with a Rust-aware action keyed on `Cargo.lock`.** Reach for `Swatinem/rust-cache@v2` rather than hand-rolling `actions/cache` over `target/`.
- **Scope to the whole workspace.** Use `--workspace` (and `--all-targets`, `--all-features`) so member crates, examples, and feature-gated code are all checked: the analogue of globbing your entire repo.
- **Pin the toolchain explicitly.** Install a known channel (`dtolnay/rust-toolchain@stable`) and consider a `rust-toolchain.toml` so local and CI use the same compiler, like pinning `node-version` and committing `.nvmrc`.
- **Separate concurrency-heavy jobs.** Putting `fmt`/`clippy`/`test`/`build` in parallel matrix jobs (covered in [GitHub Actions](/24-tooling/08-github-actions/)) gives faster feedback than one long serial job, at the cost of more cache restores.
- **Add deeper gates as the project matures:** `cargo audit` for vulnerable dependencies and `cargo deny` for license/duplicate-dependency policy. These are catalogued in [Cargo plugins](/24-tooling/11-cargo-plugins/).

> **Tip:** For a faster, richer test gate, many teams swap `cargo test` for `cargo nextest run`, which parallelizes test execution and prints a cleaner CI-friendly summary. It is covered in [Cargo plugins](/24-tooling/11-cargo-plugins/).

---

## Real-World Example

A production-flavored single-job pipeline that exercises all four gates against a small library crate. The library and its tests below are compile-verified; the test gate output shown earlier (`2 passed`) is the real result of running `cargo test` on exactly this code.

### The crate under test

```rust
// src/lib.rs
/// Returns the slug form of a title: trimmed, lowercased,
/// with runs of whitespace collapsed to single hyphens.
pub fn slugify(title: &str) -> String {
    title
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugifies_basic_title() {
        assert_eq!(slugify("  Hello World  "), "hello-world");
    }

    #[test]
    fn collapses_internal_whitespace() {
        assert_eq!(slugify("a   b\tc"), "a-b-c");
    }
}
```

Running the gates locally, in the same order CI would, produces real, green output:

```text
$ cargo fmt --all --check && echo "fmt OK"
fmt OK

$ cargo clippy --all-targets -- -D warnings 2>&1 | tail -1
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.15s

$ cargo test 2>&1 | grep "test result"
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

$ cargo build --release 2>&1 | tail -1
    Finished `release` profile [optimized] target(s) in 0.23s
```

### The CI workflow

A self-contained GitHub Actions job. The toolchain and cache actions belong to the [GitHub Actions](/24-tooling/08-github-actions/) topic; the *shape* — install toolchain, restore cache, run four gates with `--locked` — is the reusable concept:

```yaml
# .github/workflows/ci.yml
name: ci
on:
  push:
    branches: [main]
  pull_request:

# Cancel superseded runs on the same ref to save CI minutes.
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  quality-gates:
    name: fmt + clippy + test + build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install stable toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache cargo registry and target/
        uses: Swatinem/rust-cache@v2

      - name: Formatting
        run: cargo fmt --all --check

      - name: Clippy (warnings are errors)
        run: cargo clippy --all-targets --all-features --locked -- -D warnings

      - name: Tests
        run: cargo test --workspace --all-features --locked

      - name: Release build
        run: cargo build --release --locked
```

### A local pre-flight script

So contributors hit failures *before* pushing, mirror the gates in a script (the analogue of a `pretest`/`precommit` npm script). Run it before opening a PR:

```bash
#!/usr/bin/env bash
# ci-local.sh — run the same gates CI runs, fail fast.
set -euo pipefail

cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release

echo "All gates passed locally."
```

`set -e` makes the script abort on the first non-zero exit code, so it stops exactly where CI would. Because each gate signals failure through its exit code, no per-command error handling is needed.

---

## Further Reading

- [The Cargo Book: `cargo test`](https://doc.rust-lang.org/cargo/commands/cargo-test.html), [`cargo build`](https://doc.rust-lang.org/cargo/commands/cargo-build.html), and the [`--locked`/`--offline`/`--frozen` flags](https://doc.rust-lang.org/cargo/commands/cargo.html#manifest-options) — the gate commands and reproducible-build flags.
- [The Cargo Book: Continuous Integration](https://doc.rust-lang.org/cargo/guide/continuous-integration.html): official CI guidance and example matrices.
- [`Swatinem/rust-cache`](https://github.com/Swatinem/rust-cache): the Rust-aware caching action referenced throughout.
- [GitHub Actions for Rust](/24-tooling/08-github-actions/), the concrete workflow: matrix, `dtolnay/rust-toolchain`, and caching wired together.
- [Dockerizing Rust](/24-tooling/09-docker/) — caching in container builds with multi-stage and `cargo-chef`.
- [Formatting with rustfmt](/24-tooling/01-formatting/) and [ESLint to Clippy](/24-tooling/02-linting/) — the fmt and lint gates in detail.
- [Common Clippy lints](/24-tooling/03-clippy-lints/) — what the lint gate actually catches, with before/after.
- [Cargo deep dive](/24-tooling/00-cargo-deep-dive/) — profiles (`dev` vs `release`), workspaces, and offline mode behind these gates.
- [Cargo plugins](/24-tooling/11-cargo-plugins/) — `nextest`, `audit`, and `deny` for richer test and security gates.
- [Testing](/13-testing/) — writing the tests the test gate runs.
- Foundational background: [Understanding Cargo](/01-getting-started/03-cargo-basics/), [Getting Started](/01-getting-started/), and [Rust Basics](/02-basics/).
- Continue to [Advanced Topics](/25-advanced-topics/) once your pipeline is in place.

---

## Exercises

### Exercise 1: Build the four-gate local script

**Difficulty:** Easy

**Objective:** Internalize the gate order and verify each command's exit code.

**Instructions:**

1. Create a new library crate: `cargo new --lib gate_practice && cd gate_practice`.
2. Add a small function and a passing unit test.
3. Write a `ci-local.sh` that runs `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --release` in that order, with `set -e`.
4. Confirm it exits `0`. Then introduce a `return` keyword in a one-line function and confirm the script now fails at the Clippy gate (check `echo $?`).

<details>
<summary>Solution</summary>

`src/lib.rs`:

```rust
pub fn double(x: i32) -> i32 {
    x * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles() {
        assert_eq!(double(21), 42);
    }
}
```

`ci-local.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
echo "All gates passed."
```

Running `bash ci-local.sh` prints `All gates passed.` and exits `0`. Now change the body to `return x * 2;`. Re-running stops at the Clippy gate with the real error:

```text
error: unneeded `return` statement
 --> src/lib.rs:2:5
  |
2 |     return x * 2;
  |     ^^^^^^^^^^^^
```

and `echo $?` prints a non-zero status (Clippy exits `101`). The earlier `fmt` gate passed, so the script aborts exactly at the lint gate: fastest-failing gate first in action.

</details>

### Exercise 2: Make the build reproducible with `--locked`

**Difficulty:** Medium

**Objective:** Understand how `--locked` enforces a stable dependency set, the Rust analogue of `npm ci`.

**Instructions:**

1. In a binary crate, `cargo add serde` to create a `Cargo.lock`. Commit (or just keep) the lockfile.
2. Run `cargo build --locked` and confirm it succeeds.
3. Manually delete `Cargo.lock`, then run `cargo build --locked` again. Read the error.
4. Explain in one sentence why CI should pass `--locked`.

<details>
<summary>Solution</summary>

With `Cargo.lock` present and in sync, `cargo build --locked` builds normally. After deleting the lockfile (so Cargo would need to regenerate it), `--locked` refuses with the real message:

```text
error: the lock file Cargo.lock needs to be updated but --locked was passed to prevent this
If you want to try to generate the lock file without accessing the network, remove the --locked flag and use --offline instead.
```

CI should pass `--locked` so that a dependency version can never silently change between the run that reviewed a PR and the run that merges it: the same reproducibility guarantee `npm ci` gives over `npm install`.

</details>

### Exercise 3: Reason about the caching key

**Difficulty:** Medium

**Objective:** Choose a correct CI cache key and explain why caching `target/` differs from caching `node_modules`.

**Instructions:**

1. A teammate proposes caching `target/` with a cache key of `cargo-cache` (a constant string) on every run.
2. Describe two problems with a constant key.
3. Propose a better key and say what should and should not be cached.

<details>
<summary>Solution</summary>

**Problems with a constant key:**

1. *It never invalidates.* When `Cargo.lock` changes (a dependency is added or upgraded), the cache still restores stale dependency artifacts. Cargo's fingerprinting may detect the mismatch and recompile anyway, so the cache provides no benefit and wastes restore time.
2. *It accumulates your own crate's output.* `target/` also holds artifacts for your fast-changing crate, so a constant-key cache grows unbounded and, when restored, can be slower than a clean build.

**A better approach:** Key the cache on a hash of `Cargo.lock` plus the Rust toolchain version and the OS, for example `${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}`. Cache the crate registry/downloads (`~/.cargo/registry`, `~/.cargo/git`) and the *dependency* artifacts in `target/`, but not your own crate's frequently-changing output. In practice, delegate this to `Swatinem/rust-cache@v2`, which derives the key from `Cargo.lock` and the toolchain and prunes your crate's output automatically. Unlike `node_modules` (prebuilt downloads), Rust's `target/` holds compiled artifacts, so the cache exists to avoid *recompilation*, which is the dominant cost in Rust CI.

</details>
