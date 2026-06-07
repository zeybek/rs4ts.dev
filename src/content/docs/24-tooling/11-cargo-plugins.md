---
title: "Useful Cargo Plugins: nextest, watch, audit, deny, expand, and More"
description: "nextest, watch, audit, deny, and expand are your vitest, nodemon, and npm audit in Rust, installed as cargo subcommands instead of scattered npm packages."
---

## Quick Overview

In the Node world you assemble a working developer toolbox out of dozens of small packages: `vitest`/`jest` for tests, `nodemon`/`tsx --watch` for the dev loop, `npm audit` for vulnerabilities, `license-checker` for legal hygiene, `depcheck` for unused dependencies, a bundle analyzer for "why is my build so big." Some you install globally, some you run once with `npx`, some live in `devDependencies`.

Cargo has the same culture, but the mechanism is cleaner: a **cargo plugin** is just a binary on your `PATH` named `cargo-<thing>`, which Cargo then exposes as the subcommand `cargo <thing>`. You install one with `cargo install <crate>` (the rough equivalent of `npm install -g`), and from then on `cargo nextest`, `cargo audit`, `cargo deny`, etc. behave as if they were built in. There is no plugin registry, no manifest entry, no config dance. Cargo discovers them by name.

This page tours the plugins worth installing for almost any serious project: **cargo-nextest** (a faster, nicer test runner), **cargo-watch** (re-run on file change), **cargo-audit** (RUSTSEC vulnerability scan), **cargo-deny** (license / advisory / ban policy), **cargo-expand** (see what macros expand to), **cargo-outdated** (find stale dependencies), and **cargo-bloat** (what is taking up binary space), plus quick notes on **cargo-llvm-cov** (coverage), **cargo-machete** (unused deps), and **cargo-edit** (now mostly built in).

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition. Every plugin here installs as a normal stable binary except `cargo-expand`, which needs a *nightly* toolchain at runtime (explained below). Versions cited are the latest at the time of writing; always let `cargo install` resolve the current release rather than pinning from memory.

---

## TypeScript/JavaScript Example

A typical Node project wires its developer tooling through `devDependencies` and `package.json` scripts, mixing in a few `npx` one-offs:

```jsonc
// package.json — the JS "tooling" surface, scattered across dev deps + scripts
{
  "name": "billing-api",
  "scripts": {
    "test": "vitest run",
    "test:watch": "vitest",
    "dev": "tsx watch src/index.ts",
    "audit": "npm audit --audit-level=high",
    "licenses": "license-checker --production --onlyAllow 'MIT;Apache-2.0;BSD-3-Clause'",
    "deadcode": "depcheck",
    "outdated": "npm outdated",
    "analyze": "esbuild src/index.ts --bundle --analyze --metafile=meta.json"
  },
  "devDependencies": {
    "vitest": "^2.1.0",
    "tsx": "^4.19.0",
    "depcheck": "^1.4.7",
    "license-checker": "^25.0.1",
    "typescript": "^5.7.0"
  }
}
```

And the one-offs you reach for occasionally without installing anything:

```bash
# Run a tool once without adding it as a dependency:
npx depcheck                 # find unused dependencies
npx license-checker --summary
npm outdated                 # list dependencies behind their latest version
npm audit                    # scan the lockfile against the advisory DB
```

Notice the split: the *dev loop* tools (`vitest`, `tsx watch`) live in `devDependencies` so every contributor gets them on `npm install`, while *occasional audits* (`depcheck`, `license-checker`) are often `npx`-only. Cargo collapses all of this into one model — `cargo install` a binary once, run it as a `cargo` subcommand — and every plugin below maps onto one of these JS tools.

---

## Rust Equivalent

Install the toolbox once. Each `cargo install` drops a `cargo-<name>` binary into `~/.cargo/bin` (already on your `PATH` after `rustup`), and Cargo immediately exposes it as a subcommand:

```bash
# Test + dev loop (your vitest / tsx watch):
cargo install cargo-nextest --locked
cargo install cargo-watch --locked

# Supply-chain + policy (your npm audit / license-checker):
cargo install cargo-audit --locked
cargo install cargo-deny  --locked

# Inspection + maintenance (your depcheck / npm outdated / bundle analyzer):
cargo install cargo-expand   --locked
cargo install cargo-outdated --locked
cargo install cargo-bloat    --locked
cargo install cargo-machete  --locked
cargo install cargo-llvm-cov --locked
```

> **Tip:** Pass `--locked` to `cargo install` so the tool builds against its own committed `Cargo.lock`: reproducible installs, fewer surprises. It is the `npm ci` of `cargo install`.

You can confirm what is installed at any time, the closest thing to `npm ls -g`:

```bash
cargo install --list
```

```text
cargo-audit v0.22.1:
    cargo-audit
cargo-bloat v0.12.1:
    cargo-bloat
cargo-deny v0.19.8:
    cargo-deny
cargo-expand v1.0.122:
    cargo-expand
cargo-llvm-cov v0.6.21:
    cargo-llvm-cov
cargo-nextest v0.9.128:
    cargo-nextest
cargo-outdated v0.19.0:
    cargo-outdated
cargo-watch v8.5.3:
    cargo-watch
```

From here, each tool is a `cargo` subcommand. Running the faster test runner in a small project looks like this (real captured output):

```text
$ cargo nextest run
    Starting 3 tests across 2 binaries
        PASS [   0.008s] (1/3) demo tests::adds
        PASS [   0.011s] (2/3) demo tests::doubles
        PASS [   0.011s] (3/3) demo tests::negatives
────────────
     Summary [   0.012s] 3 tests run: 3 passed, 0 skipped
```

---

## Detailed Explanation

### cargo-nextest — the `vitest`/`jest` upgrade

The built-in `cargo test` runs all the tests inside *one process per test binary*, sharing state and printing results serially. **cargo-nextest** is a drop-in replacement that runs each test in its **own process**, in parallel, with a much clearer per-test report, flaky-test retries, and machine-readable output for CI.

```bash
cargo nextest run                       # run everything (your `vitest run`)
cargo nextest run billing::             # filter by path substring
cargo nextest run -E 'test(parse)'      # filter with the nextest filter DSL
cargo nextest run --retries 2           # auto-retry flaky tests (CI lifesaver)
cargo nextest run --no-fail-fast        # don't stop at first failure
```

Why teams switch:

- **Faster** on suites with many small tests, because process-level parallelism scales better than the default in-binary threads.
- **Clearer output:** one line per test with timing, and failures grouped at the end instead of interleaved.
- **CI-friendly** — `--message-format libtest-json` and JUnit output (`cargo nextest run --profile ci`) integrate with test reporters.

The one gap: nextest does **not** run doctests (the `///` examples in your docs), because those have no separate binary. The standard pattern is `cargo nextest run && cargo test --doc`.

### cargo-watch — the `nodemon`/`tsx watch` of Rust

**cargo-watch** watches your source tree and re-runs a Cargo command on every change: the tight feedback loop Node developers expect from `nodemon` or `tsx watch`:

```bash
cargo watch -x check                    # re-run `cargo check` on save (fastest loop)
cargo watch -x test                     # re-run tests on save
cargo watch -x 'nextest run'            # combine with nextest
cargo watch -x clippy -x test           # chain: clippy, then test
cargo watch -x run                      # rebuild + restart your binary (like nodemon)
cargo watch -s 'cargo run -- --port 8080'   # arbitrary shell command via -s
```

`-x` takes a Cargo subcommand; `-s` takes an arbitrary shell command (use it for anything that is not a bare `cargo` call). For the fastest edit loop, watch `cargo check` rather than `cargo build`: `check` does type-checking without codegen, so it returns in a fraction of the time.

> **Note:** Cargo is gaining a built-in `cargo watch`-style mode, but the standalone `cargo-watch` plugin remains the ubiquitous, battle-tested choice today. The flags above are stable.

### cargo-audit — `npm audit` for Rust

**cargo-audit** scans your `Cargo.lock` against the [RUSTSEC advisory database](https://rustsec.org) and reports any dependency with a known vulnerability. Exactly what `npm audit` does against npm's advisory feed:

```bash
cargo audit                             # scan Cargo.lock against RUSTSEC
cargo audit --deny warnings             # treat unmaintained/yanked warnings as errors (CI)
cargo audit fix                         # attempt to bump vulnerable deps (like `npm audit fix`)
```

On a clean project it walks the lockfile and reports nothing wrong (real captured output):

```text
$ cargo audit
    Fetching advisory database from `https://github.com/RustSec/advisory-db.git`
      Loaded 1100 security advisories (from ~/.cargo/advisory-db)
    Updating crates.io index
    Scanning Cargo.lock for vulnerabilities (2 crate dependencies)
```

When it *does* find something, it prints the advisory ID (`RUSTSEC-YYYY-NNNN`), the affected crate and version, the dependency path that pulls it in, and the patched version range to upgrade to. RUSTSEC also tracks **unmaintained** and **yanked** crates, which surface as warnings.

### cargo-deny — policy enforcement (`license-checker` + audit + bans)

Where `cargo-audit` answers "any known vulnerabilities?", **cargo-deny** enforces *policy*: which licenses are allowed, which crates are banned, whether duplicate versions are tolerated, and which advisories block the build. It is configured with a `deny.toml` and run as one gate:

```bash
cargo deny init                         # generate a starter deny.toml
cargo deny check                        # run all checks
cargo deny check licenses               # just the license policy (your license-checker)
cargo deny check advisories             # RUSTSEC, overlapping with cargo-audit
cargo deny check bans                   # banned crates + duplicate-version policy
```

```toml
# deny.toml — license allowlist, banned crate, and advisory policy
[licenses]
# Only these SPDX licenses are permitted anywhere in the graph:
allow = ["MIT", "Apache-2.0", "BSD-3-Clause", "Unicode-3.0"]

[bans]
# Fail if a copyleft or known-problem crate sneaks in transitively:
deny = [{ crate = "openssl-sys", reason = "prefer rustls; avoid system OpenSSL" }]
# Warn when two versions of the same crate end up in the tree:
multiple-versions = "warn"

[advisories]
# Block the build on any RUSTSEC advisory (and on yanked crates):
yanked = "deny"
```

This is the tool you put in CI as a single quality gate; it subsumes both the license check and the advisory scan a Node project would run as separate steps.

### cargo-expand — see what the macros actually generated

Rust's `#[derive(...)]`, `println!`, `#[tokio::main]`, and other macros generate code you never see. **cargo-expand** runs the compiler's macro-expansion pass and prints the resulting source; invaluable for understanding or debugging a macro. It is the one plugin here that needs **nightly**, because macro expansion is exposed only through an unstable compiler flag:

```bash
rustup toolchain install nightly        # one-time: cargo-expand drives nightly rustc
cargo install cargo-expand --locked
cargo expand                            # expand the whole crate
cargo expand --bin greeting             # expand one target
cargo expand path::to::module           # expand a single module
```

For a tiny program that derives `Debug` and uses `println!`, the expansion makes the generated `impl` visible (real captured output, trimmed):

```rust
struct Point {
    x: i32,
    y: i32,
}
#[automatically_derived]
impl ::core::fmt::Debug for Point {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        ::core::fmt::Formatter::debug_struct_field2_finish(
            f, "Point", "x", &self.x, "y", &&self.y,
        )
    }
}
fn main() {
    let p = Point { x: 1, y: 2 };
    { ::std::io::_print(format_args!("{0:?}\n", p)); };
}
```

You can see the `Debug` impl the derive wrote and that `println!` lowered to a `format_args!` + `_print` call. There is no JS equivalent that is this clean; the closest is reading Babel/SWC transform output.

### cargo-outdated — `npm outdated` for Cargo

`cargo update` bumps dependencies *within* their declared semver ranges, but it will not tell you when a *newer major* exists beyond your range. **cargo-outdated** does, listing each dependency's current, latest-compatible, and latest-overall versions:

```bash
cargo outdated                          # full table (your `npm outdated`)
cargo outdated --root-deps-only         # only your direct deps, ignore transitive
cargo outdated --workspace              # across every workspace member
```

On a fully up-to-date project it simply says so (real captured output):

```text
$ cargo outdated
All dependencies are up to date, yay!
```

When something is behind, it prints a table with `Name`, `Project` (your pinned version), `Compat` (latest in-range), and `Latest` (newest published) so you can see at a glance whether a bump is a safe patch or a breaking major.

### cargo-bloat — "why is my binary this big?"

A Node bundle analyzer tells you which packages dominate your JS bundle. **cargo-bloat** does the same for a compiled Rust binary, attributing `.text` (code) size to functions and crates:

```bash
cargo bloat --release                   # biggest functions in the release binary
cargo bloat --release --crates          # roll the sizes up per crate
cargo bloat --release -n 20             # top 20 entries
```

Per-function and per-crate views (real captured output, trimmed):

```text
$ cargo bloat --release --crates
 File  .text     Size Crate
51.6% 104.0% 222.2KiB std
 0.6%   1.3%   2.7KiB [Unknown]
 0.0%   0.0%      60B demo
49.7% 100.0% 213.7KiB .text section size, the file size is 430.2KiB

Note: numbers above are a result of guesswork. They are not 100% correct and never will be.
```

Read it as a *guide*, not gospel (the tool says so itself). It pairs naturally with the release-profile tuning (`strip`, `lto`, `opt-level = "z"`) from [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/): measure with `cargo bloat`, then shrink with profile settings, then re-measure.

### Honorable mentions

- **cargo-llvm-cov:** code coverage built on LLVM source-based instrumentation; `cargo llvm-cov` (text summary) or `cargo llvm-cov --html` / `--lcov` for reports and CI upload. It also drives nextest: `cargo llvm-cov nextest`. This is the recommended modern coverage path; see [Code Coverage and Faster Test Runs](/13-testing/10-coverage/).
- **cargo-machete:** finds dependencies declared in `Cargo.toml` but never actually used (your `depcheck`). Fast, scans source for `use` paths: `cargo machete` to report, `cargo machete --fix` to remove them. (The more thorough but slower `cargo-udeps` does the same via real compilation, but needs nightly.)
- **cargo-edit** — historically provided `cargo add` / `cargo rm` / `cargo upgrade`. **`cargo add` and `cargo remove` have been built into Cargo since 1.62**, so you rarely install `cargo-edit` anymore; its one remaining draw is `cargo upgrade`, which bumps the *version requirements* in `Cargo.toml` (not just the lockfile).

---

## Key Differences

| Concern | Node / npm | Cargo plugin |
| --- | --- | --- |
| Install mechanism | `npm i -g <pkg>` or `npx <pkg>` | `cargo install <crate>` → `cargo-<name>` binary on `PATH` |
| Discovery | explicit binary name / `npx` | Cargo auto-exposes any `cargo-*` on `PATH` as a subcommand |
| Test runner | `vitest` / `jest` | `cargo-nextest` (process-per-test, retries, JUnit) |
| Dev watch loop | `nodemon`, `tsx watch` | `cargo-watch -x check` / `-x 'nextest run'` |
| Vulnerability scan | `npm audit` (npm advisory DB) | `cargo-audit` (RUSTSEC advisory DB) |
| License / policy gate | `license-checker`, manual | `cargo-deny` (licenses + advisories + bans in one `deny.toml`) |
| Unused deps | `depcheck` | `cargo-machete` (`cargo-udeps` for the thorough check) |
| Outdated deps | `npm outdated` | `cargo-outdated` |
| Bundle/binary analysis | esbuild `--analyze`, `source-map-explorer` | `cargo-bloat` (per-function/per-crate `.text` size) |
| Coverage | `c8`, `nyc`, vitest `--coverage` | `cargo-llvm-cov` |
| Macro/transform output | Babel/SWC transform dump | `cargo-expand` (needs nightly) |

Three points where the model genuinely differs from npm:

1. **Plugins are global binaries, not project dependencies.** Unlike `devDependencies`, a cargo plugin is not recorded in `Cargo.toml`; it lives in `~/.cargo/bin`. To make a plugin reproducible for the whole team you pin it in **CI** (install at a known version) rather than in the manifest. Some teams record desired tool versions in a `rust-toolchain.toml` comment or an `xtask`/`Makefile`.
2. **Discovery is convention, not configuration.** Any executable named `cargo-foo` on your `PATH` becomes `cargo foo`. There is no plugin registry or opt-in list, which is why you can write your own (`cargo-xtask` is exactly this trick).
3. **One tool, nightly; the rest, stable.** `cargo-expand` is the lone outlier that needs a nightly toolchain *at run time* (it installs on stable but invokes nightly `rustc`). Everything else here is pure stable.

---

## Common Pitfalls

### Forgetting that `cargo install` compiles from source

Unlike `npm i -g`, which downloads prebuilt JS, `cargo install` **compiles the tool from source**, which can take a minute or two per plugin. On CI this is wasted time on every run. Use prebuilt-binary installers instead: `cargo-binstall` (`cargo binstall cargo-nextest`) downloads a release binary when one exists, and most plugins ship GitHub Actions (e.g. `taiki-e/install-action`) that fetch a binary in seconds. Reserve `cargo install --locked` for local dev machines.

### Assuming `cargo-expand` works on stable

`cargo expand` on a stable-only machine fails because it needs the nightly compiler's expansion flag:

```text
error: the option `Z` is only accepted on the nightly compiler
```

**Fix:** `rustup toolchain install nightly`. You do not have to switch your *project* to nightly; `cargo-expand` invokes nightly `rustc` for the expansion pass only, while your normal builds stay on stable.

### Expecting `cargo nextest` to run doctests

Nextest deliberately does not run doctests (they have no standalone test binary). If your `///` examples contain `assert!`s you care about, a green `cargo nextest run` is not enough:

```bash
cargo nextest run && cargo test --doc   # nextest for unit/integration, cargo for doctests
```

### Confusing `cargo audit` with `cargo deny check advisories`

They overlap — both read RUSTSEC — but they are not interchangeable. `cargo-audit` is laser-focused on vulnerabilities and offers `cargo audit fix`. `cargo-deny` rolls advisories into a broader *policy* gate (licenses, bans, duplicate versions) configured by `deny.toml`. Most teams run `cargo-deny` in CI as the single gate and keep `cargo-audit` for quick interactive checks and its `fix` subcommand.

### Reading `cargo bloat` numbers as exact

`cargo-bloat` itself prints "numbers above are a result of guesswork." Symbol attribution after inlining and dead-code elimination is approximate. Use it to find the *relatively* largest contributors and to compare before/after a change, not to report a precise byte budget.

### Treating `cargo outdated`'s "Latest" as "safe to upgrade"

A newer *major* version (the `Latest` column) is by definition outside your semver range and may be a breaking change. `cargo update` only moves within range (the `Compat` column). Bumping to `Latest` means editing the requirement in `Cargo.toml` (or `cargo upgrade` from `cargo-edit`) and dealing with any API breakage. Treat it like an `npm install pkg@latest` across a major.

---

## Best Practices

- **Standardize the team's toolbox in CI, not in `Cargo.toml`.** Plugins are global binaries, so pin their versions in your workflow (and prefer prebuilt-binary installers like `cargo-binstall` / `taiki-e/install-action` over compiling from source on every run).
- **Make nextest the default test runner.** Add an alias so `cargo t` runs it, and remember the doctest companion:

  ```toml
  # .cargo/config.toml
  [alias]
  t = "nextest run"
  ```

  Then run `cargo t && cargo test --doc` locally and in CI.
- **Use `cargo watch -x check` as your inner loop.** `check` is dramatically faster than `build`, so save-to-feedback stays sub-second. Switch to `cargo watch -x 'nextest run'` when you want tests on every save.
- **Gate every PR with `cargo deny check` and `cargo audit`.** Licenses and known vulnerabilities should fail the build, not get discovered in production. Commit a reviewed `deny.toml`.
- **Run `cargo machete` periodically** (and `cargo-outdated` before dependency-bump PRs) to keep the graph lean — fewer deps means faster builds, smaller binaries, and less audit surface.
- **Profile binary size with `cargo bloat` *before* reaching for exotic tricks**, then shrink with release-profile settings (`strip`, `lto`, `opt-level = "z"`, `panic = "abort"`) from [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/).
- **Don't switch your whole project to nightly just for `cargo-expand`.** Install nightly alongside stable and let the plugin reach for it on demand.

---

## Real-World Example

Here is a realistic two-part setup: a fast local dev loop, and a CI quality gate built from these plugins. Both mirror what a well-run Node project does with `npm run dev` and a CI workflow, but consolidated through Cargo subcommands.

**Local: a save-triggered loop.** Aliases in `.cargo/config.toml` keep commands short, and `cargo-watch` re-runs them on every change:

```toml
# .cargo/config.toml — short aliases for the daily loop
[alias]
t   = "nextest run"
lint = "clippy --all-targets --all-features -- -D warnings"
```

```bash
# One terminal: type-check on every save (fastest feedback)
cargo watch -x check

# Another terminal: lint, then run the full (fast) test suite on every save
cargo watch -x lint -x 'nextest run'
```

**CI: a single quality gate.** A `Makefile`-style sequence (or `xtask`) that any contributor and the CI runner execute identically — formatting, linting, the audit/policy gate, the fast test run, and doctests:

```bash
# scripts/ci.sh — the gate, runnable locally and in CI
set -euo pipefail

cargo fmt --all -- --check          # formatting (see ./formatting.md)
cargo clippy --all-targets --all-features -- -D warnings   # lints (./linting.md)
cargo deny check                    # licenses + advisories + bans (deny.toml)
cargo audit --deny warnings         # RUSTSEC vulnerabilities, warnings fatal
cargo nextest run --profile ci      # fast parallel tests, JUnit output for the reporter
cargo test --doc                    # doctests (nextest skips these)
cargo llvm-cov nextest --lcov --output-path lcov.info   # coverage for upload
```

In a GitHub Actions workflow you would install the plugins with a prebuilt-binary action (so the gate stays fast) and then run that script. The full workflow YAML — caching, the install step, matrix, and artifact upload — lives in [A Real GitHub Actions Workflow for Rust](/24-tooling/08-github-actions/); the broader CI design (when to fail vs. warn, gating strategy) is in [CI/CD Concepts for Rust](/24-tooling/07-ci-cd/).

The payoff is the same as the Node version of this story, but tighter: every tool is a `cargo` subcommand, the audit and license checks are one binary (`cargo deny`), and the test runner doubles as the coverage driver (`cargo llvm-cov nextest`). One install step, one script, identical locally and in CI.

---

## Further Reading

- [The Cargo Book — Third-party subcommands](https://doc.rust-lang.org/cargo/reference/external-tools.html#custom-subcommands): how Cargo discovers `cargo-*` binaries on your `PATH`.
- [cargo-nextest documentation](https://nexte.st/): filter DSL, CI profiles, JUnit output, retries.
- [cargo-watch](https://github.com/watchexec/cargo-watch) — flags, `-x` vs `-s`, and watch behavior.
- [RustSec / cargo-audit](https://rustsec.org/) and the [advisory database](https://github.com/RustSec/advisory-db).
- [cargo-deny book](https://embarkstudios.github.io/cargo-deny/) — `deny.toml` reference for licenses, bans, advisories, and sources.
- [cargo-expand](https://github.com/dtolnay/cargo-expand), [cargo-outdated](https://github.com/kbknapp/cargo-outdated), [cargo-bloat](https://github.com/RazrFalcon/cargo-bloat), [cargo-machete](https://github.com/bnjbvr/cargo-machete), and [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov).
- Sibling pages in this section:
  - [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/) — profiles, aliases, workspaces, and `cargo metadata` these tools build on.
  - [CI/CD Concepts for Rust](/24-tooling/07-ci-cd/) and [A Real GitHub Actions Workflow for Rust](/24-tooling/08-github-actions/) — wiring these plugins into a CI gate.
  - [Linting with Clippy](/24-tooling/02-linting/) and [Formatting with rustfmt](/24-tooling/01-formatting/) — Clippy and rustfmt, the other half of the gate.
- Related sections: [Code Coverage and Faster Test Runs](/13-testing/10-coverage/) (coverage with `cargo-llvm-cov`) and [Auditing Dependencies and Supply-Chain Hygiene](/27-security/08-security-audit/) (the full supply-chain story behind `cargo-audit`/`cargo-deny`).

---

## Exercises

### Exercise 1: Swap in cargo-nextest

**Difficulty:** Beginner

**Objective:** Install `cargo-nextest`, run a small test suite with it, and add an alias so it becomes your default test command.

**Instructions:**

1. Create a project with a library that has two or three `#[test]` functions.
2. Install nextest (`cargo install cargo-nextest --locked`) and run the suite with `cargo nextest run`.
3. Add an `[alias] t = "nextest run"` to `.cargo/config.toml` and confirm `cargo t` runs the same suite.
4. Explain in one sentence why a green `cargo nextest run` does *not* guarantee your doctests pass.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
pub fn add(a: i64, b: i64) -> i64 { a + b }
pub fn double(x: i64) -> i64 { x * 2 }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn adds() { assert_eq!(add(2, 3), 5); }
    #[test] fn doubles() { assert_eq!(double(21), 42); }
    #[test] fn negatives() { assert_eq!(add(-1, -1), -2); }
}
```

```bash
cargo install cargo-nextest --locked
cargo nextest run
```

```text
    Starting 3 tests across 2 binaries
        PASS [   0.008s] (1/3) demo tests::adds
        PASS [   0.011s] (2/3) demo tests::doubles
        PASS [   0.011s] (3/3) demo tests::negatives
────────────
     Summary [   0.012s] 3 tests run: 3 passed, 0 skipped
```

```toml
# .cargo/config.toml
[alias]
t = "nextest run"
```

```bash
cargo t   # now runs `cargo nextest run`
```

**Why doctests aren't covered:** nextest runs compiled test *binaries* in separate processes, but doctests (the `///` examples) are compiled and executed by `rustc`/`cargo test` directly and have no standalone binary, so nextest skips them entirely. Run `cargo test --doc` alongside nextest.

</details>

### Exercise 2: A supply-chain gate with cargo-deny

**Difficulty:** Intermediate

**Objective:** Configure `cargo-deny` to enforce a license allowlist and a banned crate, and observe it pass on a clean project.

**Instructions:**

1. In a project with a couple of dependencies, install `cargo-deny` and run `cargo deny init` to generate a starter `deny.toml`.
2. Edit `deny.toml` to allow only `MIT`, `Apache-2.0`, and `BSD-3-Clause` licenses, and to `deny` the crate `openssl-sys` under `[bans]`.
3. Run `cargo deny check` and confirm it passes (or reports exactly which crate violates the policy).
4. Explain how this single command replaces two separate Node tools.

<details>
<summary>Solution</summary>

```bash
cargo install cargo-deny --locked
cargo deny init        # writes a starter deny.toml
```

```toml
# deny.toml — trimmed to the parts this exercise changes
[licenses]
allow = ["MIT", "Apache-2.0", "BSD-3-Clause"]

[bans]
deny = [{ crate = "openssl-sys", reason = "prefer rustls" }]
multiple-versions = "warn"

[advisories]
yanked = "deny"
```

```bash
cargo deny check
# `licenses ... ok`, `bans ... ok`, `advisories ... ok`  (or a clear error naming
# the crate whose license isn't on the allowlist, or that hit the ban)
```

**What it replaces:** `cargo deny check` rolls together what a Node project would run as *two* separate tools — `license-checker` (the `[licenses]` allowlist) and `npm audit` (the `[advisories]` RUSTSEC scan) — plus a dependency-ban policy npm has no direct equivalent for. One binary, one `deny.toml`, one CI gate.

</details>

### Exercise 3: Find and shrink binary bloat

**Difficulty:** Advanced

**Objective:** Use `cargo-bloat` to identify what dominates a release binary, then apply release-profile settings and measure the difference.

**Instructions:**

1. Build a small binary in release mode and run `cargo bloat --release --crates` to see the per-crate `.text` breakdown.
2. Add a size-tuned `[profile.release]` to `Cargo.toml` (`strip`, `lto`, `opt-level = "z"`).
3. Rebuild and re-run `cargo bloat --release --crates`; note how the totals change.
4. Explain why `cargo-bloat`'s numbers should be read as relative, not exact.

<details>
<summary>Solution</summary>

```bash
cargo install cargo-bloat --locked
cargo bloat --release --crates
```

```text
 File  .text     Size Crate
51.6% 104.0% 222.2KiB std
 0.6%   1.3%   2.7KiB [Unknown]
 0.0%   0.0%      60B demo
49.7% 100.0% 213.7KiB .text section size, the file size is 430.2KiB

Note: numbers above are a result of guesswork. They are not 100% correct and never will be.
```

```toml
# Cargo.toml — size-tuned release profile
[profile.release]
strip = true        # drop symbols
lto = true          # link-time optimization
opt-level = "z"     # optimize aggressively for size
codegen-units = 1   # better cross-function optimization
panic = "abort"     # no unwinding tables
```

```bash
cargo bloat --release --crates   # rebuild + re-measure; file size shrinks noticeably
```

**Why numbers are relative:** after inlining, monomorphization, and dead-code elimination, the linker no longer maps cleanly back to source symbols, so `cargo-bloat` *estimates* attribution (it says so itself). Trust it to rank the largest contributors and to compare before/after a change, not to assert an exact byte count.

</details>
