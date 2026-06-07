---
title: "Dev-Dependencies, Build-Dependencies, and Optional Dependencies"
description: "Cargo splits runtime, dev, and build-time dependencies, plus optional feature-gated ones, enforcing each boundary at compile time, unlike npm's devDependencies."
---

## Quick Overview

In `package.json` you split packages into `dependencies` and `devDependencies` so test runners and bundlers don't ship to production. Cargo makes the same split — and adds a third bucket for code that runs *at build time* — through three manifest tables: **`[dependencies]`**, **`[dev-dependencies]`**, and **`[build-dependencies]`**. On top of that, any normal dependency can be marked **`optional`** so it is only pulled in when a [feature flag](/12-modules-packages/09-feature-flags/) turns it on. This page is about *which bucket a crate belongs in* and *when each one is compiled*, a distinction Cargo enforces far more strictly than npm.

---

## TypeScript/JavaScript Example

A typical `package.json` separates runtime code from tooling. The `dependencies` ship; the `devDependencies` are for building and testing only:

```json
// package.json
{
  "name": "id-gen",
  "version": "0.1.0",
  "type": "module",
  "dependencies": {
    "nanoid": "^5.0.0"
  },
  "devDependencies": {
    "vitest": "^2.0.0",
    "fast-check": "^3.0.0",
    "typescript": "^5.5.0"
  },
  "optionalDependencies": {
    "fsevents": "^2.3.0"
  }
}
```

How Node and npm treat these:

```typescript
// src/index.ts — runtime code can import a `dependency`
import { nanoid } from "nanoid";

export function makeId(prefix: string): string {
  return `${prefix}-${nanoid(6)}`;
}
```

```typescript
// test/id.test.ts — test code reaches for devDependencies
import { test, expect } from "vitest";
import * as fc from "fast-check";
import { makeId } from "../src/index.js";

test("ids start with the prefix", () => {
  fc.assert(fc.property(fc.string(), (p) => makeId(p).startsWith(p)));
});
```

Three things to notice, because Rust will mirror two of them and tighten the third:

- `npm install --production` (or `npm ci --omit=dev`) skips `devDependencies`, but **nothing stops** your `src/index.ts` from importing `vitest`. The boundary is a *convention* enforced only by what you choose to import.
- `optionalDependencies` may fail to install without failing the whole install (e.g. `fsevents` is macOS-only).
- There is no separate bucket for "code that runs during the build": bundler plugins just live in `devDependencies`.

---

## Rust Equivalent

The same project as a Rust crate. Each kind of dependency lives in its own table, and Cargo *enforces* the boundaries:

```toml
# Cargo.toml
[package]
name = "idgen"
version = "0.1.0"
edition = "2024"

# Ships to production. Available to src/ and everything else.
[dependencies]
serde_json = { version = "1", optional = true }   # only compiled when a feature turns it on

# Compiled ONLY for `cargo test`/`cargo bench`/`cargo run --example`.
# Never compiled into your library or binary, never seen by downstream crates.
[dev-dependencies]
proptest = "1.11.0"
criterion = "0.8.2"

# Compiled and run on the BUILD machine, for build.rs only. Not linked into your program.
[build-dependencies]
chrono = "0.4"

[features]
# Turn the optional dependency on with `--features json`.
json = ["dep:serde_json"]
```

The library and its property test, where the dev-dependency is only reachable from the `tests/` directory:

```rust
// src/lib.rs — only `[dependencies]` are in scope here
/// Generate a zero-padded id, e.g. `"user-000042"`.
pub fn make_id(prefix: &str, n: u64) -> String {
    format!("{prefix}-{n:06}")
}
```

```rust
// tests/properties.rs — an integration test; `[dev-dependencies]` ARE in scope here
use idgen::make_id;
use proptest::prelude::*;

proptest! {
    #[test]
    fn id_always_starts_with_prefix(prefix in "[a-z]{1,8}", n in 0u64..1_000_000) {
        let id = make_id(&prefix, n);
        prop_assert!(id.starts_with(&prefix));
        prop_assert!(id.contains('-'));
    }
}
```

Running the suite compiles `proptest` *only now*, not during a normal build:

```text
     Running tests/properties.rs (target/debug/deps/properties-8252496bc40fbcfe)

running 1 test
test id_always_starts_with_prefix ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

> **Note:** Add dependencies to the right table without hand-editing TOML: `cargo add proptest --dev`, `cargo add cc --build`, and `cargo add serde_json --optional`. The `--dev`, `--build`, and `--optional` flags have been built into `cargo add` since Cargo 1.62. No `cargo-edit` install needed. See [Cargo Commands](/12-modules-packages/05-cargo-commands/) and [Specifying Dependencies](/12-modules-packages/06-dependencies/).

---

## Detailed Explanation

### `[dev-dependencies]`: the direct analog of `devDependencies`

A **dev-dependency** is compiled only when Cargo builds your tests, benchmarks, examples, or doc-tests. It is *not* compiled when someone runs `cargo build`, `cargo build --release`, or depends on your crate from their own project.

The important difference from npm is **scope enforcement** rather than installation alone. In Node, `import "vitest"` from `src/` works fine on your machine (it only breaks in production where dev-deps weren't installed). In Rust, a dev-dependency simply does not exist as far as `src/` is concerned — referencing it from library or binary code is a *compile error*, caught immediately on your own machine. We hit that error on purpose in [Common Pitfalls](#common-pitfalls).

Where dev-dependencies *are* in scope:

| Location | Purpose | Dev-deps in scope? |
| --- | --- | --- |
| `src/lib.rs`, `src/main.rs` | Your shipped code | No |
| `tests/*.rs` | Integration tests | Yes |
| `benches/*.rs` | Benchmarks | Yes |
| `examples/*.rs` | Runnable examples | Yes |
| `#[cfg(test)] mod tests` inside `src/` | Unit tests | Yes (only under `cfg(test)`) |
| Doc-tests in `///` comments | Documentation examples | Yes |

> **Tip:** A unit test module guarded by `#[cfg(test)]` inside `src/lib.rs` *can* use dev-dependencies, because that module is compiled only in the test build. But the surrounding non-test code in the same file still cannot. The `#[cfg(test)]` attribute is the switch. See [Testing](/13-testing/) for the full testing story.

The payoff is visible in what ships. A normal release build compiles only your crate, no `criterion`, no `proptest`:

```text
$ cargo build --release
   Compiling idgen v0.1.0 (/private/tmp/idgen_probe/idgen)
    Finished `release` profile [optimized] target(s) in 0.22s
```

And a downstream consumer's dependency graph (`cargo tree -e no-dev`, the edges they actually pull) contains none of your dev-dependencies:

```text
$ cargo tree -e no-dev
idgen v0.1.0 (/private/tmp/idgen_probe/idgen)
```

### Benchmarks: the most common reason to reach for a dev-dependency

`criterion` is the de-facto statistical benchmarking crate, and it is *always* a dev-dependency. A benchmark lives in `benches/` and is declared in the manifest with `harness = false` so Cargo hands control to Criterion's runner instead of the built-in test harness:

```toml
# Cargo.toml
[[bench]]
name = "id_bench"
harness = false
```

```rust
// benches/id_bench.rs
use criterion::{criterion_group, criterion_main, Criterion};
use idgen::{checksum, make_id};
use std::hint::black_box;

fn bench_make_id(c: &mut Criterion) {
    c.bench_function("make_id", |b| {
        b.iter(|| make_id(black_box("user"), black_box(42)))
    });
}

criterion_group!(benches, bench_make_id);
criterion_main!(benches);
```

`cargo bench` compiles Criterion (a dev-dependency) and runs the measurement:

```text
make_id                 time:   [73.714 ns 76.545 ns 81.134 ns]
Found 11 outliers among 100 measurements (11.00%)
  5 (5.00%) high mild
  6 (6.00%) high severe
```

> **Note:** `std::hint::black_box` (stabilized in Rust 1.66) stops the optimizer from deleting code whose result you ignore, the Rust equivalent of the tricks JS micro-benchmark libraries use to defeat dead-code elimination.

### `[build-dependencies]`: a bucket npm doesn't have

There is no `package.json` equivalent of `[build-dependencies]`. These are crates used by **`build.rs`**, a build script that compiles and runs *on the build machine before your crate compiles*. Build-dependencies are linked into the build script, never into your program, and — like dev-dependencies — are invisible to `src/`.

A common, dependency-light use is **code generation**: the build script writes a `.rs` file into the `OUT_DIR` Cargo provides, and `src/` pulls it in with `include!`.

```rust
// build.rs — runs at build time; `[build-dependencies]` are in scope here
use std::{env, fs, path::Path};

fn main() {
    // `chrono` is a BUILD-dependency: usable here, NOT in src/.
    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("build_info.rs");
    fs::write(&dest, format!("pub const BUILD_DATE: &str = \"{now}\";\n")).unwrap();

    // Re-run this script only when build.rs itself changes (not on every build).
    println!("cargo:rerun-if-changed=build.rs");
}
```

```rust
// src/main.rs — splices in the file build.rs generated
include!(concat!(env!("OUT_DIR"), "/build_info.rs"));

fn main() {
    println!("greeter built on {BUILD_DATE}");
}
```

```text
$ cargo run
   Compiling greeter v0.1.0 (/private/tmp/build_probe/greeter)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.25s
     Running `target/debug/greeter`
greeter built on 2026-05-31
```

This page stays in the manifest lane: *what* `[build-dependencies]` is and *when* it compiles. For the build-script API itself — `cargo::rerun-if-changed`, linking native C libraries, more code-generation patterns — see [Build Scripts](/12-modules-packages/10-build-scripts/).

### Optional dependencies: `dependencies` that only appear behind a feature

Marking a dependency `optional = true` means it is **not compiled by default**; a [feature](/12-modules-packages/09-feature-flags/) must request it. This is how crates offer "pay for what you use" integrations (a `serde` impl, a `tokio` runtime, JSON support) without forcing every user to compile them.

```toml
# Cargo.toml
[dependencies]
serde_json = { version = "1", optional = true }

[features]
# `dep:serde_json` activates the optional dependency without leaking its name as a feature.
json = ["dep:serde_json"]
```

```rust
// src/main.rs
fn plain_report(name: &str, count: u64) -> String {
    format!("{name}: {count}")
}

// Compiled only when the `json` feature (and thus serde_json) is enabled.
#[cfg(feature = "json")]
fn json_report(name: &str, count: u64) -> String {
    serde_json::json!({ "name": name, "count": count }).to_string()
}

fn main() {
    println!("{}", plain_report("requests", 7));

    #[cfg(feature = "json")]
    println!("{}", json_report("requests", 7));

    #[cfg(not(feature = "json"))]
    println!("(build with --features json for JSON output)");
}
```

The default build never touches `serde_json`; opting in pulls it in and compiles the gated code:

```text
$ cargo run
requests: 7
(build with --features json for JSON output)

$ cargo run --features json
requests: 7
{"count":7,"name":"requests"}
```

This is a genuinely sharper tool than npm's `optionalDependencies`, which is about *install failures being non-fatal* (platform-specific native modules). Rust's `optional` is about *compile-time presence controlled by features*, a different mechanism for a different goal. There is no `optionalDependencies`-style "try to install, shrug if it fails" in Cargo, because Cargo resolves a complete, reproducible graph up front (see `Cargo.lock` in [Cargo.toml](/12-modules-packages/04-cargo/)).

---

## Key Differences

| Concept | TypeScript / npm | Rust / Cargo |
| --- | --- | --- |
| Runtime deps | `dependencies` | `[dependencies]` |
| Test/tooling deps | `devDependencies` | `[dev-dependencies]` |
| Build-time-only deps | (none — live in `devDependencies`) | `[build-dependencies]` (for `build.rs`) |
| Conditional/optional deps | `optionalDependencies` (install may fail silently) | `optional = true` + a feature (compile-time gate) |
| Boundary enforcement | Convention; `src/` *can* import a devDependency | Compiler error if `src/` uses a dev/build dependency |
| Omitting dev deps | `npm ci --omit=dev` | Automatic: `cargo build` never compiles them |
| What downstream users get | Your `dependencies` (transitively) | Your `[dependencies]` only; dev/build deps never propagate |

Three mental-model shifts for a TypeScript developer:

1. **The buckets are enforced by the compiler, not by which files you happen to import.** You cannot "accidentally ship a test dependency" by importing it from production code. It won't compile.
2. **`[build-dependencies]` is a third, distinct kind.** Build-time tooling that *runs* (code generators, native-lib probing) is separated from test tooling. In npm both would be `devDependencies`.
3. **Optional means "feature-gated at compile time," not "best-effort install."** It is the dependency arm of the feature system, covered in [Feature Flags and Conditional Compilation](/12-modules-packages/09-feature-flags/).

> **Note:** The same crate can appear in more than one table: e.g. `serde` as a normal dependency *and* `serde_json` as a dev-dependency. Cargo de-duplicates by version in the lockfile, so listing a crate in both `[dependencies]` and `[dev-dependencies]` is fine and sometimes necessary (a normal-dep feature you only need in tests).

---

## Common Pitfalls

### Pitfall 1: Using a dev-dependency from `src/`

The single most common mistake. You add `proptest` as a dev-dependency, then reach for it in `src/lib.rs`:

```rust
// src/lib.rs
use proptest::prelude::*; // does not compile (error[E0433]): proptest is a dev-dependency

pub fn make_id(prefix: &str, n: u64) -> String {
    format!("{prefix}-{n:06}")
}
```

The real compiler output:

```text
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `proptest`
 --> src/lib.rs:1:5
  |
1 | use proptest::prelude::*; // does not compile (error[E0433]): proptest is a dev-dependency
  |     ^^^^^^^^ use of unresolved module or unlinked crate `proptest`
  |
  = help: if you wanted to use a crate named `proptest`, use `cargo add proptest` to add it to your `Cargo.toml`

error: could not compile `idgen` (lib) due to 1 previous error
```

**Fix:** if the crate is genuinely needed by shipping code, move it to `[dependencies]` (`cargo add proptest`). If it is only for tests, keep the usage in `tests/`, `benches/`, or a `#[cfg(test)]` module.

### Pitfall 2: Using a build-dependency from `src/`

`build.rs` and `src/` have *separate* dependency graphs. A crate in `[build-dependencies]` is not available to your program:

```rust
// src/main.rs
fn main() {
    let now = chrono::Utc::now(); // does not compile (error[E0433]): chrono is a build-dependency
    println!("now is {now}");
}
```

```text
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `chrono`
 --> src/main.rs:4:15
  |
4 |     let now = chrono::Utc::now(); // does not compile (error[E0433]): chrono is a build-dependency
  |               ^^^^^^ use of unresolved module or unlinked crate `chrono`
  |
  = help: if you wanted to use a crate named `chrono`, use `cargo add chrono` to add it to your `Cargo.toml`
```

**Fix:** if `build.rs` needs it, keep it in `[build-dependencies]`; if your program needs it at runtime, add it to `[dependencies]` as well (the build script's copy and the runtime copy are independent).

### Pitfall 3: An optional dependency silently becomes a feature name

If you mark a dependency `optional = true` but never reference it via `dep:` in any feature, Cargo creates an **implicit feature with the same name as the dependency**. That sometimes surprises people:

```toml
# Cargo.toml — no [features] table at all
[dependencies]
serde_json = { version = "1", optional = true }
```

```text
$ cargo run                          # default: implicit feature off
json feature off

$ cargo run --features serde_json    # an implicit feature named after the optional dep
json feature on: {"ok":true}
```

This compiles and works, but it leaks the dependency's name into your public feature surface, a breaking change if you later rename or drop the dep. **Best practice:** give the dependency a deliberate feature using the `dep:` prefix (`json = ["dep:serde_json"]`), which *suppresses* the implicit feature so only your chosen name is public. The `dep:` syntax has been available since Cargo 1.60.

### Pitfall 4: Expecting `cargo tree` to hide dev-dependencies

Unlike a release *build*, the default `cargo tree` view *does* list your own crate's dev-dependencies (under a `[dev-dependencies]` heading) because it shows the whole local graph:

```text
$ cargo tree
idgen v0.1.0 (/private/tmp/idgen_probe/idgen)
[dev-dependencies]
├── criterion v0.8.2
│   ├── alloca v0.4.0
...
```

That is expected — it does **not** mean those crates ship. To see exactly what propagates to consumers (the deps that actually build into your artifact), use `cargo tree -e no-dev`. Don't conflate "appears in `cargo tree`" with "compiled into the binary."

---

## Best Practices

- **Put benchmarking and property/test crates in `[dev-dependencies]`.** `criterion`, `proptest`, `mockall`, `assert_cmd`, `tempfile`-for-tests — all dev-dependencies. They never bloat your release artifact.
- **Reserve `[build-dependencies]` for crates `build.rs` actually calls.** If you don't have a `build.rs`, you don't need this table. Keep build scripts (and their dependency trees) small. They sit on every clean build's critical path.
- **Make optional dependencies explicit features with `dep:`.** Write `json = ["dep:serde_json"]` rather than relying on the implicit feature, so your public feature names are intentional. See [Feature Flags and Conditional Compilation](/12-modules-packages/09-feature-flags/) for additive feature design.
- **Use `cargo add` with the right flag** (`--dev`, `--build`, `--optional`) instead of editing TOML by hand. It places the entry in the correct table and resolves the latest compatible version.
- **Declare benchmarks with `harness = false`** when using Criterion, and keep each bench file focused. Run them with `cargo bench`, never as part of `cargo test`.
- **Don't fear duplicating a crate across tables.** Listing `serde` in `[dependencies]` and again (perhaps with extra features) in `[dev-dependencies]` is idiomatic when tests need capabilities production doesn't.

---

## Real-World Example

A small library crate that exposes an optional JSON-export feature, is property-tested, and is benchmarked — using all three dependency tables at once.

```toml
# Cargo.toml
[package]
name = "idgen"
version = "0.1.0"
edition = "2024"

[dependencies]
serde_json = { version = "1", optional = true }

[dev-dependencies]
proptest = "1.11.0"
criterion = "0.8.2"

[features]
json = ["dep:serde_json"]

[[bench]]
name = "id_bench"
harness = false
```

```rust
// src/lib.rs — production code: only [dependencies] (and feature-gated optional ones)
/// Generate a zero-padded id, e.g. `"user-000042"`.
pub fn make_id(prefix: &str, n: u64) -> String {
    format!("{prefix}-{n:06}")
}

/// Sum the numeric suffixes of a slice of ids.
pub fn checksum(ids: &[String]) -> u64 {
    ids.iter()
        .filter_map(|s| s.rsplit('-').next())
        .filter_map(|num| num.parse::<u64>().ok())
        .sum()
}

/// JSON export — compiled only with `--features json`.
#[cfg(feature = "json")]
pub fn ids_to_json(ids: &[String]) -> String {
    serde_json::to_string(ids).expect("ids serialize")
}
```

```rust
// benches/id_bench.rs — uses the criterion dev-dependency
use criterion::{criterion_group, criterion_main, Criterion};
use idgen::{checksum, make_id};
use std::hint::black_box;

fn bench_checksum(c: &mut Criterion) {
    let ids: Vec<String> = (0..1000).map(|n| make_id("u", n)).collect();
    c.bench_function("checksum_1000", |b| b.iter(|| checksum(black_box(&ids))));
}

criterion_group!(benches, bench_checksum);
criterion_main!(benches);
```

```rust
// tests/properties.rs — uses the proptest dev-dependency
use idgen::make_id;
use proptest::prelude::*;

proptest! {
    #[test]
    fn checksum_components_are_numeric(prefix in "[a-z]{1,8}", n in 0u64..1_000_000) {
        let id = make_id(&prefix, n);
        let suffix = id.rsplit('-').next().unwrap();
        prop_assert!(suffix.parse::<u64>().is_ok());
    }
}
```

Each command compiles only the relevant tables:

```text
$ cargo build                 # [dependencies] only; serde_json off, no criterion/proptest
$ cargo test                  # + [dev-dependencies]; runs the proptest integration test
$ cargo bench                 # + criterion; runs benchmarks
$ cargo build --features json # pulls in the optional serde_json
```

Verified benchmark output for `checksum_1000`:

```text
checksum_1000           time:   [15.281 µs 17.581 µs 20.611 µs]
Found 18 outliers among 100 measurements (18.00%)
  6 (6.00%) high mild
  12 (12.00%) high severe
```

The release artifact a consumer receives contains only `idgen` (plus `serde_json` *if* they enable `json`), never `proptest` or `criterion`.

---

## Further Reading

- [The Cargo Book: Specifying Dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html) — dev/build/optional dependency syntax.
- [The Cargo Book: Build Scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html) — `build.rs` and `[build-dependencies]`.
- [The Cargo Book: Features](https://doc.rust-lang.org/cargo/reference/features.html) — optional dependencies and the `dep:` syntax.
- [The Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/) — statistical benchmarking.
- Sibling pages: [Cargo.toml](/12-modules-packages/04-cargo/) (the manifest and `Cargo.lock`) · [Specifying Dependencies](/12-modules-packages/06-dependencies/) (version requirements, `cargo add`, git/path deps) · [Feature Flags and Conditional Compilation](/12-modules-packages/09-feature-flags/) (conditional compilation) · [Build Scripts](/12-modules-packages/10-build-scripts/) (the `build.rs` API) · [Cargo Commands](/12-modules-packages/05-cargo-commands/) · [Cargo Workspaces](/12-modules-packages/08-workspaces/) (sharing dev-deps across a monorepo).
- Foundations: [Introduction](/00-introduction/) · [Understanding Cargo](/01-getting-started/03-cargo-basics/) · [Basics](/02-basics/) · testing details in [Testing](/13-testing/).

---

## Exercises

### Exercise 1: Sort a crate into the right table

**Difficulty:** Beginner

**Objective:** Build the correct mental model for which dependency goes where.

**Instructions:** You are writing a library crate that (a) parses JSON config at runtime, (b) generates a Rust constants file from a `.txt` data file during the build, and (c) is fuzz-tested. For each of `serde_json`, a hypothetical `txt-to-rs` code generator used by `build.rs`, and `proptest`, write the `cargo add` command that places it in the correct table. Then state which table each lands in.

<details>
<summary>Solution</summary>

```bash
cargo add serde_json        # runtime parsing -> [dependencies]
cargo add txt-to-rs --build # used by build.rs -> [build-dependencies]
cargo add proptest --dev    # testing only    -> [dev-dependencies]
```

Resulting manifest tables:

```toml
[dependencies]
serde_json = "1"

[build-dependencies]
txt-to-rs = "..."   # only build.rs may use this

[dev-dependencies]
proptest = "1.11.0" # only tests/benches/examples may use this
```

`serde_json` ships and is reachable from `src/`. `txt-to-rs` runs on the build machine and is reachable only from `build.rs`. `proptest` is compiled only for `cargo test`/`cargo bench` and is reachable only from test code.

</details>

### Exercise 2: Add a property test that uses a dev-dependency

**Difficulty:** Intermediate

**Objective:** Wire up a dev-dependency and prove it is *not* visible to `src/`.

**Instructions:** Start from a library with `pub fn double(n: u32) -> u32 { n * 2 }` in `src/lib.rs`. Add `proptest` as a dev-dependency and write an integration test in `tests/` asserting that `double(n)` is always even for `n` in `0..1000`. Then try adding `use proptest::prelude::*;` to the top of `src/lib.rs` and run `cargo build`. Explain the error.

<details>
<summary>Solution</summary>

```bash
cargo add proptest --dev
```

```rust
// src/lib.rs
pub fn double(n: u32) -> u32 {
    n * 2
}
```

```rust
// tests/even.rs
use mylib::double; // replace `mylib` with your crate name
use proptest::prelude::*;

proptest! {
    #[test]
    fn double_is_even(n in 0u32..1000) {
        prop_assert_eq!(double(n) % 2, 0);
    }
}
```

`cargo test` compiles `proptest` and passes. But adding `use proptest::prelude::*;` to `src/lib.rs` fails to build with:

```text
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `proptest`
```

because `src/` only sees `[dependencies]`, never `[dev-dependencies]`. The boundary is enforced by the compiler, not by convention. That is the central difference from npm's `devDependencies`.

</details>

### Exercise 3: Gate an optional dependency behind a feature

**Difficulty:** Advanced

**Objective:** Turn a normal dependency into an opt-in one and verify both build paths, avoiding the implicit-feature pitfall.

**Instructions:** Take a binary crate that prints a report. Make `serde_json` an optional dependency, expose a `json` feature using the `dep:` syntax, and add a `json_report` function compiled only under that feature. Verify `cargo run` (no JSON) and `cargo run --features json` (JSON) both work. Explain why `json = ["dep:serde_json"]` is preferable to leaving the feature implicit.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
serde_json = { version = "1", optional = true }

[features]
json = ["dep:serde_json"]
```

```rust
// src/main.rs
fn plain_report(name: &str, count: u64) -> String {
    format!("{name}: {count}")
}

#[cfg(feature = "json")]
fn json_report(name: &str, count: u64) -> String {
    serde_json::json!({ "name": name, "count": count }).to_string()
}

fn main() {
    println!("{}", plain_report("requests", 7));

    #[cfg(feature = "json")]
    println!("{}", json_report("requests", 7));

    #[cfg(not(feature = "json"))]
    println!("(build with --features json for JSON output)");
}
```

Verified output:

```text
$ cargo run
requests: 7
(build with --features json for JSON output)

$ cargo run --features json
requests: 7
{"count":7,"name":"requests"}
```

Using `dep:serde_json` is preferable because it *suppresses* the implicit feature that Cargo would otherwise create from the optional dependency's name. Without it, both `--features json` *and* `--features serde_json` would work, leaking the dependency's name into your public feature surface — a future breaking change if you rename or drop the dependency. The `dep:` prefix keeps `json` as the only public knob.

</details>
