---
title: "A Real GitHub Actions Workflow for Rust"
description: "A complete Rust GitHub Actions workflow: install with dtolnay/rust-toolchain, cache target/ via Swatinem/rust-cache, and matrix-test across OSes and channels."
---

## Quick Overview

GitHub Actions is the same CI platform you already use for Node.js projects: the YAML lives in `.github/workflows/`, the triggers are the same `push`/`pull_request` events, and jobs still run on `ubuntu-latest`. What changes for Rust is the building blocks: instead of `actions/setup-node` + `npm ci` you reach for `dtolnay/rust-toolchain` to install the compiler and `Swatinem/rust-cache` to cache the `target/` directory and the registry. This topic walks through one complete, copy-pasteable workflow: a build/test matrix across operating systems and Rust channels, the right way to install a toolchain, and caching that actually works.

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition. This guide pins action versions that are current as of mid-2026: `actions/checkout@v6`, `dtolnay/rust-toolchain` (referenced by channel, e.g. `@stable`), and `Swatinem/rust-cache@v2`. Always check each action's releases page before copying, because action major versions move independently of the Rust release cycle.

---

## TypeScript/JavaScript Example

A typical Node.js CI workflow installs Node via `actions/setup-node`, restores the npm cache it provides, runs `npm ci`, and then runs lint/test/build across a small matrix of Node versions:

```yaml
# .github/workflows/ci.yml  (Node.js)
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        node: [20, 22]
    steps:
      - uses: actions/checkout@v6

      - uses: actions/setup-node@v4
        with:
          node-version: ${{ matrix.node }}
          cache: npm # built-in npm cache, keyed on package-lock.json

      - run: npm ci # clean, lockfile-faithful install
      - run: npm run lint
      - run: npm test
      - run: npm run build
```

The mental model here: `setup-node` puts a Node binary on `PATH`, its `cache: npm` option transparently restores and saves `~/.npm` keyed on `package-lock.json`, and `npm ci` refuses to touch the lockfile. Everything is dependency-graph driven and re-run on every push.

---

## Rust Equivalent

The Rust version of the same pipeline is structurally identical, but each piece has a Rust-flavored replacement. The toolchain comes from `dtolnay/rust-toolchain`, caching comes from `Swatinem/rust-cache` (there is no built-in Cargo cache the way there is a built-in npm cache), and the lint/test/build commands are `cargo fmt`, `cargo clippy`, and `cargo test`:

```yaml
# .github/workflows/ci.yml  (Rust)
name: CI

on:
  push:
    branches: [main]
  pull_request:

# Cancel superseded runs on the same branch/PR to save runner minutes.
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always # keep colored output in the Actions log

jobs:
  test:
    name: test ${{ matrix.toolchain }} on ${{ matrix.os }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        toolchain: [stable, beta]
    steps:
      - uses: actions/checkout@v6

      - name: Install Rust (${{ matrix.toolchain }})
        uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.toolchain }}
          components: clippy, rustfmt

      - name: Cache cargo registry and target dir
        uses: Swatinem/rust-cache@v2

      - name: Format check
        run: cargo fmt --all --check

      - name: Clippy (warnings are errors)
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Test
        run: cargo test --all-features --locked

      - name: Build (release)
        run: cargo build --release --locked
```

This compiles, lints, and tests your crate on three operating systems and two Rust channels — six jobs total — and the second run on an unchanged dependency set restores `target/` from the cache instead of rebuilding the world.

---

## Detailed Explanation

Reading the workflow top to bottom and contrasting with the Node version:

**`on:` triggers** are identical to Node CI: run on pushes to `main` and on every pull request. Nothing Rust-specific.

**`concurrency:`** cancels an in-flight run when a newer commit lands on the same ref. This matters more for Rust than for Node because a cold Rust compile is genuinely expensive (minutes, not seconds), so killing superseded runs directly saves runner minutes. The `${{ github.ref }}` group key means each branch/PR cancels only its own older runs.

**`env: CARGO_TERM_COLOR: always`** forces Cargo to emit ANSI colors even though it is not attached to a TTY, so the Actions log is readable. There is no Node equivalent because npm output is already plain.

**`matrix:`** works exactly like the Node matrix. The two axes are `os` and `toolchain`. The substitution is conceptual: where Node iterates over *runtime versions* (Node 20, 22), Rust iterates over *release channels* (`stable`, `beta`). Because Rust is a compiled language, the compiler version is what varies, not a separate runtime. `fail-fast: false` lets every cell finish even after one fails, so you see all failures in one run instead of just the first.

**`actions/checkout@v6`** is the same action you already use; nothing changes for Rust.

**`dtolnay/rust-toolchain`** replaces `actions/setup-node`. It is a thin, fast wrapper over `rustup` maintained by David Tolnay (author of serde, anyhow, thiserror, syn). It installs the requested toolchain, sets it as the default, and — importantly — adds the requested `components` (here `clippy` and `rustfmt`) in the same step. The reference you pin selects behavior:

- `dtolnay/rust-toolchain@stable` installs the stable channel with no `with:` block needed.
- `dtolnay/rust-toolchain@1.96.0` pins an exact compiler version (your MSRV, for example).
- `dtolnay/rust-toolchain@master` is used when you want to pass the channel dynamically via `with: { toolchain: ... }`, which is exactly what the matrix needs because `${{ matrix.toolchain }}` is not known until runtime.

This is unlike Node, where `setup-node` always takes the version as an input. With `rust-toolchain`, the *git ref of the action itself* is one valid way to choose the channel, which feels surprising the first time you see it.

> **Note:** Unlike the old `actions-rs/toolchain` action (now unmaintained and archived), `dtolnay/rust-toolchain` does not generate deprecation warnings and is the de-facto community standard. If you inherit a repo using `actions-rs/*`, migrating to `dtolnay/rust-toolchain` + `Swatinem/rust-cache` is the standard fix.

**`Swatinem/rust-cache@v2`** is the single most important line for CI speed, and it has no Node analog because npm's cache is built into `setup-node`. Cargo has no equivalent built-in, so this dedicated action does three things a naive `actions/cache` setup gets wrong:

1. It caches `~/.cargo/registry` and `~/.cargo/git` (downloaded crate sources) **and** the workspace `target/` directory.
2. It computes its cache key from your `Cargo.lock`, the compiler version, and the job, so a toolchain bump or a dependency change correctly invalidates the cache.
3. It automatically cleans stale artifacts from `target/` before saving, so the cache does not grow without bound across runs.

Because it runs *after* the toolchain step, it can key on the exact `rustc` version, which is why step order matters: checkout, then toolchain, then cache, then build.

**The command steps** map cleanly onto the Node lint/test/build trio:

| Node step | Rust step | What it does |
| --- | --- | --- |
| `npm run lint` (ESLint) | `cargo clippy --all-targets --all-features -- -D warnings` | Lints; `-D warnings` turns every warning into a hard error so CI fails |
| (Prettier in lint) | `cargo fmt --all --check` | Verifies formatting without rewriting files |
| `npm test` | `cargo test --all-features --locked` | Runs unit, integration, and doc tests |
| `npm run build` | `cargo build --release --locked` | Compiles the optimized binary |

The `--locked` flag is the Rust counterpart to `npm ci`: it makes Cargo refuse to run if `Cargo.lock` would need to change, guaranteeing CI builds the exact dependency versions the lockfile records. The `--all-targets` flag on Clippy ensures it lints tests, examples, and benches too — not just the library and binary.

Here is the kind of small library this pipeline would be guarding. It is an ordinary lib+binary crate with unit tests and a doc test:

```rust
// src/lib.rs
use std::collections::HashMap;

/// Tallies how many times each word appears in `text`, lowercased.
///
/// # Examples
///
/// ```
/// let counts = taskcli::word_counts("Go go GO");
/// assert_eq!(counts.get("go"), Some(&3));
/// ```
pub fn word_counts(text: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word.to_lowercase()).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_case_insensitive() {
        let counts = word_counts("Rust rust RUST");
        assert_eq!(counts.get("rust"), Some(&3));
    }

    #[test]
    fn empty_input_yields_no_counts() {
        assert!(word_counts("   ").is_empty());
    }
}
```

```rust
// src/main.rs
use taskcli::word_counts;

fn main() {
    let counts = word_counts("the quick brown fox the lazy dog the");
    let mut pairs: Vec<_> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (word, n) in pairs.iter().take(3) {
        println!("{word}: {n}");
    }
}
```

Running `cargo test` locally produces the exact output the CI `Test` step will produce. Note that the doc test runs as a third test binary, something Node has no equivalent for:

```text
running 2 tests
test tests::empty_input_yields_no_counts ... ok
test tests::counts_are_case_insensitive ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests taskcli
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

> **Tip:** This topic focuses on the *workflow mechanics*. The reasoning behind which gates to run (fmt + clippy + test + build) and how to cache `target/` lives in [CI/CD concepts](/24-tooling/07-ci-cd/); the `fmt --check` gate details are in [Formatting with rustfmt](/24-tooling/01-formatting/), and the `-D warnings` lint policy is in [ESLint to Clippy](/24-tooling/02-linting/).

---

## Key Differences

| Concept | Node.js (GitHub Actions) | Rust (GitHub Actions) |
| --- | --- | --- |
| Install runtime/compiler | `actions/setup-node@v4` | `dtolnay/rust-toolchain@stable` (or `@master` + `toolchain:`) |
| Pin version source | `node-version:` input | the action's git ref **or** the `toolchain:` input |
| Matrix axis | runtime versions (20, 22) | release channels (`stable`, `beta`, `nightly`) or MSRV pins |
| Dependency cache | built into `setup-node` (`cache: npm`) | separate `Swatinem/rust-cache@v2` action |
| What is cached | `~/.npm` | `~/.cargo/registry`, `~/.cargo/git`, **and** `target/` |
| Lockfile-faithful install | `npm ci` | `--locked` flag on cargo commands |
| Lint command | `npm run lint` (ESLint) | `cargo clippy ... -- -D warnings` |
| Format check | Prettier (often in lint) | `cargo fmt --all --check` |
| Test runner | `npm test` (Jest/Vitest) | `cargo test` (also runs doc tests) |
| Cold-cache cost | seconds | minutes — caching matters far more |

The single biggest practical difference is caching. With Node, you can omit the cache and CI is merely a little slower. With Rust, omitting `Swatinem/rust-cache` means recompiling every transitive dependency from source on every run, which can turn a 90-second CI into a 10-minute one. Treat the cache step as mandatory, not optional.

The second difference is that Rust's matrix is about *compiler versions*, not runtimes. A common pattern is one cell pinned to your MSRV (minimum supported Rust version) to catch accidental use of newer language features, plus `stable` and optionally `beta` to get early warning of upcoming regressions.

---

## Common Pitfalls

### Forgetting `-D warnings`, so Clippy never fails CI

`cargo clippy` by itself exits `0` even when it emits warnings — warnings are advisory by default. A CI step that runs plain `cargo clippy` shows the lints in the log but passes the job anyway, so nothing is actually enforced. You must add `-- -D warnings` to promote warnings to errors. With it, a single unused variable fails the build:

```rust playground
fn main() {
    let unused = compute(); // fails `cargo clippy -- -D warnings`
    println!("done");
}

fn compute() -> i32 {
    42
}
```

Running `cargo clippy -- -D warnings` on that code produces a real failure with a non-zero exit code (the CI job goes red):

```text
error: unused variable: `unused`
 --> src/main.rs:2:9
  |
2 |     let unused = compute();
  |         ^^^^^^ help: if this is intentional, prefix it with an underscore: `_unused`
  |
  = note: `-D unused-variables` implied by `-D warnings`
  = help: to override `-D warnings` add `#[allow(unused_variables)]`

error: could not compile `lintfail` (bin "lintfail") due to 1 previous error
```

The process exits with status `101`, which is what makes GitHub Actions mark the step failed. Without the `-- -D warnings`, the same code compiles, prints only a warning, and the job stays green: a silent gap that lets lints rot.

### Putting the cache step before the toolchain step

`Swatinem/rust-cache` keys its cache partly on the compiler version, which it can only read *after* the toolchain is installed. If you place the cache step before `dtolnay/rust-toolchain`, the key is computed against whatever Rust the runner image happened to ship, so a toolchain change will silently reuse a stale cache. Always order steps: `checkout` -> `rust-toolchain` -> `rust-cache` -> build/test.

### Using the archived `actions-rs/*` actions

Older Rust CI tutorials use `actions-rs/toolchain` and `actions-rs/cargo`. Those repositories are archived and unmaintained; they rely on deprecated Node 12/16 runners and emit GitHub deprecation warnings on every run. Replace `actions-rs/toolchain` with `dtolnay/rust-toolchain` and drop `actions-rs/cargo` entirely; just call `cargo` in a `run:` step.

### Pinning the channel via `@stable` but also passing `toolchain:`

`dtolnay/rust-toolchain@stable` already *means* stable; adding `with: { toolchain: ... }` to that ref is ignored or conflicting. Use one mechanism: either pin the channel in the ref (`@stable`, `@1.96.0`) with no `toolchain:` input, or use `@master` and pass `toolchain:`, which is mandatory when the value comes from a matrix variable like `${{ matrix.toolchain }}`.

### Expecting Windows paths and line endings to behave like Linux

When your matrix includes `windows-latest`, `git` may convert line endings on checkout and your tests may assume `/`-style paths. If a test passes on Linux/macOS but fails only on Windows, suspect `\r\n` versus `\n` or `std::path` separators before suspecting your logic. This is the Rust echo of the same cross-platform footguns you already know from Node CI.

---

## Best Practices

- **Split the matrix from single-run gates.** Run the cross-platform build/test matrix in one job, but run `cargo fmt --check` and `cargo clippy` only once on `ubuntu-latest`; formatting and lints are platform-independent, so checking them on three OSes wastes minutes. See the multi-job layout in the Real-World Example below.

- **Always cache with `Swatinem/rust-cache@v2`, after the toolchain step.** It is the single most impactful line in any Rust CI file.

- **Use `--locked` everywhere in CI.** It is the `npm ci` guarantee: CI builds exactly what `Cargo.lock` records and fails loudly if the lockfile is out of date. Commit `Cargo.lock` for binaries and applications.

- **Pin your MSRV as an explicit matrix cell** (e.g. `dtolnay/rust-toolchain@1.81.0`) if your crate advertises a minimum supported Rust version, so a newer-language-feature slips through CI instead of breaking downstream users.

- **Set `fail-fast: false`** so one failing cell does not hide the status of the others — you want the whole truth in one run.

- **Add `concurrency` with `cancel-in-progress: true`** to stop paying for superseded runs; Rust compiles are expensive enough that this saves real money on busy repos.

- **Pin action major versions, not floating tags.** `@v6` and `@v2` are acceptable for trusted first-party and well-known actions; for stricter supply-chain hygiene, pin to a full commit SHA. Either way, let Dependabot keep them current.

- **Reach for `dtolnay/rust-toolchain@master` only when the channel is dynamic** (matrix-driven); otherwise prefer the self-documenting `@stable` / `@1.96.0` refs.

---

## Real-World Example

A production-grade workflow usually separates fast feedback (fmt + clippy, single OS) from the slower cross-platform test matrix, and adds a security-audit job. This is a complete, current `.github/workflows/ci.yml` that you can drop into a real crate:

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -D warnings # fail the build on any compiler warning, everywhere

jobs:
  # Fast, single-platform gate: formatting + lints.
  lint:
    name: fmt + clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6

      - name: Install stable Rust with clippy + rustfmt
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - uses: Swatinem/rust-cache@v2

      - name: rustfmt
        run: cargo fmt --all --check

      - name: clippy
        run: cargo clippy --all-targets --all-features --locked -- -D warnings

  # Cross-platform, multi-channel build + test.
  test:
    name: test ${{ matrix.toolchain }} / ${{ matrix.os }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        toolchain: [stable, beta]
        include:
          # One extra cell pinning the crate's MSRV, Linux only.
          - os: ubuntu-latest
            toolchain: "1.81.0"
    steps:
      - uses: actions/checkout@v6

      - name: Install Rust (${{ matrix.toolchain }})
        uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.toolchain }}

      - uses: Swatinem/rust-cache@v2
        with:
          # Separate cache buckets per matrix cell so they don't clobber each other.
          key: ${{ matrix.os }}-${{ matrix.toolchain }}

      - name: Build
        run: cargo build --release --locked

      - name: Test
        run: cargo test --all-features --locked

  # Dependency vulnerability audit (advisory DB).
  audit:
    name: cargo audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install and run cargo-audit
        run: |
          cargo install cargo-audit --locked
          cargo audit
```

What this gives you:

- The `lint` job returns fast feedback on style and lints, running on one OS only.
- The `test` job exercises three operating systems and three toolchains (`stable`, `beta`, and a pinned `1.81.0` MSRV cell added via `include:`), each with its own cache bucket via the `key:` input so a Windows cache never overwrites the Linux one.
- The `RUSTFLAGS: -D warnings` env var applies the "warnings are errors" policy to the compiler itself across every job, complementing Clippy's `-- -D warnings`.
- The `audit` job installs and runs `cargo-audit` to flag dependencies with known CVEs. (See [useful cargo plugins](/24-tooling/11-cargo-plugins/) for `cargo-audit`, `cargo-deny`, and `cargo-nextest`, which slots in as a faster `cargo test` replacement.)

Because each job declares its own `Swatinem/rust-cache@v2` step, the second run on an unchanged `Cargo.lock` restores compiled artifacts instead of rebuilding, typically cutting a multi-minute cold build down to seconds of cache restore plus an incremental compile.

> **Tip:** For release automation — building and uploading platform binaries when you push a tag — you combine this CI with cross-compilation. The targets and `musl` static-build mechanics are covered in [Cross-compilation](/24-tooling/10-cross-compilation/), and shipping the result in a tiny container is in [Dockerizing Rust](/24-tooling/09-docker/).

---

## Further Reading

- [GitHub Actions documentation](https://docs.github.com/en/actions): workflow syntax, triggers, matrices, and contexts.
- [`dtolnay/rust-toolchain`](https://github.com/dtolnay/rust-toolchain): the toolchain-install action; the README lists every input and the channel-by-ref convention.
- [`Swatinem/rust-cache`](https://github.com/Swatinem/rust-cache) — the caching action; read the options for `key`, `shared-key`, and `workspaces`.
- [`actions/checkout`](https://github.com/actions/checkout) — check the releases page for the current major version before copying.
- [CI/CD concepts for Rust](/24-tooling/07-ci-cd/): *why* these gates exist and how to think about caching the `target` directory.
- [Formatting with rustfmt](/24-tooling/01-formatting/) and [ESLint to Clippy](/24-tooling/02-linting/) — the `cargo fmt --check` and `cargo clippy -- -D warnings` gates in depth.
- [Useful cargo plugins](/24-tooling/11-cargo-plugins/): `cargo-nextest`, `cargo-audit`, and `cargo-deny` for richer CI jobs.
- [Cross-compilation](/24-tooling/10-cross-compilation/) and [Dockerizing Rust](/24-tooling/09-docker/) — extending CI into release builds and container images.
- Foundational background: [Understanding Cargo](/01-getting-started/03-cargo-basics/), [Getting Started](/01-getting-started/), and [Rust Basics](/02-basics/).
- Continue to [Advanced Topics](/25-advanced-topics/) once your pipeline is green.

---

## Exercises

### Exercise 1: Add a format-and-lint gate to a workflow

**Difficulty:** Easy

**Objective:** Build the habit of installing a toolchain with components and running the fmt + clippy gates.

**Instructions:**

1. In a new or existing crate, create `.github/workflows/ci.yml`.
2. Add a single job on `ubuntu-latest` that checks out the code, installs stable Rust with the `clippy` and `rustfmt` components, restores the cache, then runs `cargo fmt --all --check` and `cargo clippy -- -D warnings`.
3. Locally reproduce the clippy gate: write a `main` with an unused variable and confirm `cargo clippy -- -D warnings` exits non-zero (`echo $?`).

<details>
<summary>Solution</summary>

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - uses: Swatinem/rust-cache@v2

      - run: cargo fmt --all --check
      - run: cargo clippy --all-targets -- -D warnings
```

To reproduce the lint failure locally, put this in `src/main.rs`:

```rust playground
fn main() {
    let total = 1 + 2; // unused, so clippy -D warnings fails
    println!("hello");
}
```

`cargo clippy -- -D warnings` prints an `unused variable: total` error and `echo $?` shows `101`. Either use `total` or prefix it with `_` to make the gate pass.

</details>

### Exercise 2: Build a cross-platform, multi-channel matrix

**Difficulty:** Medium

**Objective:** Use `dtolnay/rust-toolchain@master` with a matrix variable and give each cell its own cache bucket.

**Instructions:**

1. Add a `test` job that runs on `ubuntu-latest`, `macos-latest`, and `windows-latest`.
2. Add a `toolchain` matrix axis with `stable` and `beta`.
3. Install the toolchain with `dtolnay/rust-toolchain@master` driven by `${{ matrix.toolchain }}`, set `fail-fast: false`, give `Swatinem/rust-cache` a per-cell `key`, and run `cargo test --locked`.

<details>
<summary>Solution</summary>

```yaml
# .github/workflows/ci.yml (test job)
jobs:
  test:
    name: test ${{ matrix.toolchain }} / ${{ matrix.os }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        toolchain: [stable, beta]
    steps:
      - uses: actions/checkout@v6

      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.toolchain }}

      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.os }}-${{ matrix.toolchain }}

      - run: cargo test --all-features --locked
```

The key insight: because `${{ matrix.toolchain }}` is only known at runtime, you must use `@master` plus the `toolchain:` input; a fixed ref like `@stable` cannot read a matrix value. The per-cell `key:` prevents the six caches from overwriting one another.

</details>

### Exercise 3: Pin an MSRV cell and add a dependency-audit job

**Difficulty:** Medium-Hard

**Objective:** Combine `include:` for a one-off matrix cell with a separate security-audit job, the way a production repo is laid out.

**Instructions:**

1. Take the matrix from Exercise 2 and add, via `include:`, a single Linux-only cell that pins `dtolnay/rust-toolchain` to a specific old version (your crate's MSRV, e.g. `1.81.0`).
2. Add a second job `audit` that installs and runs `cargo-audit`.
3. Explain why the MSRV cell uses `include:` rather than adding `1.81.0` to the `toolchain` list directly.

<details>
<summary>Solution</summary>

```yaml
jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        toolchain: [stable, beta]
        include:
          - os: ubuntu-latest
            toolchain: "1.81.0" # MSRV, Linux only
    steps:
      - uses: actions/checkout@v6
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.toolchain }}
      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.os }}-${{ matrix.toolchain }}
      - run: cargo test --all-features --locked

  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo install cargo-audit --locked
      - run: cargo audit
```

Why `include:` instead of adding `1.81.0` to the `toolchain` list? Adding it to the list would multiply across *all three* operating systems, producing nine cells and testing the MSRV on macOS and Windows too — usually wasted minutes. `include:` appends exactly one extra cell with the precise `os`/`toolchain` combination you want, so the MSRV is checked once, on Linux only. Quoting `"1.81.0"` keeps YAML from parsing it as a float and dropping the trailing zero.

</details>
