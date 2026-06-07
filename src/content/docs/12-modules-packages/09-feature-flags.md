---
title: "Feature Flags and Conditional Compilation"
description: "Cargo features and #[cfg(...)] decide at compile time which code and optional dependencies ship, replacing the runtime process.env and tree-shaking tricks JS uses."
---

Cargo **features** let one crate ship multiple optional capabilities behind named switches, and `#[cfg(...)]` lets the compiler include or exclude code *before it is ever compiled*. Together they replace the runtime `if (process.env.FEATURE)` and `package.json` `optionalDependencies` juggling that TypeScript/JavaScript developers reach for. But the decision happens at **compile time**, not at runtime.

---

## Quick Overview

A **feature** is a named flag declared in `Cargo.toml` under `[features]`. Enabling a feature can pull in **optional dependencies** and turn on `#[cfg(feature = "...")]`-gated code. Because the gating happens at compile time, code behind a disabled feature is **not compiled at all**: it adds zero bytes to your binary and zero startup cost.

For a TypeScript/JavaScript developer, the closest mental model is *bundler dead-code elimination driven by build flags* (think `process.env.NODE_ENV === "production"` checks that a bundler strips) combined with *optional peer dependencies*. The key differences: Rust does this in the language and the compiler (no bundler plugin needed), features are **additive** (turning one on never turns another off), and feature unification across your whole dependency graph is handled by Cargo.

> **Note:** This page covers the language and manifest mechanics of features and `#[cfg]`. The dependency-spec syntax for enabling a *dependency's* features (`features = [...]`, `default-features = false`) lives in [Specifying Dependencies](/12-modules-packages/06-dependencies/), and how features interact with `[dev-dependencies]`/`[build-dependencies]` and `optional = true` is in [Dev-Dependencies, Build-Dependencies, and Optional Dependencies](/12-modules-packages/07-dev-dependencies/).

---

## TypeScript/JavaScript Example

In TypeScript/JavaScript there is no built-in compile-time feature system, so teams improvise. The two common approaches are **runtime environment checks** and **optional dependencies loaded dynamically**:

```typescript
// reporter.ts — runtime feature flags via environment variables

// Optional dependency: may or may not be installed (declared in
// package.json "optionalDependencies"). We must guard the require.
type YamlModule = { dump(value: unknown): string };

interface Report {
  name: string;
  passed: number;
  failed: number;
}

const FEATURES = {
  json: process.env.FEATURE_JSON !== "0", // on by default
  yaml: process.env.FEATURE_YAML === "1", // off by default
  metrics: process.env.FEATURE_METRICS === "1",
};

async function render(report: Report): Promise<string> {
  if (FEATURES.json) {
    return JSON.stringify(report, null, 2);
  }

  if (FEATURES.yaml) {
    // The yaml package might not be installed; this throws at RUNTIME if missing.
    const yaml = (await import("js-yaml")) as unknown as YamlModule;
    return yaml.dump(report);
  }

  return `${report.name} - ${report.passed} passed, ${report.failed} failed`;
}

if (FEATURES.metrics) {
  // Dead code is still shipped in the bundle even when this is false.
  console.error("[metrics] reporter initialized");
}

export { render, FEATURES };
```

The problems a TypeScript/JavaScript developer lives with here:

- **Everything ships anyway.** The `yaml` branch and the `metrics` code are in the bundle even when the flags are off. Tree-shaking helps only if the bundler can statically prove the branch is dead.
- **Failures are at runtime.** A missing optional dependency or a typo in a flag name explodes when the code runs, possibly in production.
- **No type-level guarantee** that you only call code that is actually enabled.

---

## Rust Equivalent

In Rust, features are declared in the manifest and the compiler removes disabled code entirely. Here is the same reporter as a Cargo crate.

```toml
# Cargo.toml
[package]
name = "reporter"
version = "0.1.0"
edition = "2024"

[features]
# `default` lists the features that are on unless the user opts out.
default = ["json"]
# A feature can pull in OPTIONAL dependencies via the `dep:` syntax.
json = ["dep:serde", "dep:serde_json"]
yaml = ["dep:serde", "dep:serde_norway"]
# A pure code flag with no dependencies of its own.
metrics = []

[dependencies]
serde = { version = "1", features = ["derive"], optional = true }
serde_json = { version = "1", optional = true }
serde_norway = { version = "0.9", optional = true }
```

```rust
// src/main.rs

// Add the `Serialize` derive ONLY when a serializing format is enabled.
#[cfg_attr(any(feature = "json", feature = "yaml"), derive(serde::Serialize))]
struct Report {
    name: String,
    passed: u32,
    failed: u32,
}

impl Report {
    /// Render in whatever format the build was compiled with.
    fn render(&self) -> String {
        #[cfg(feature = "json")]
        {
            return serde_json::to_string_pretty(self).expect("serialize json");
        }
        #[cfg(all(feature = "yaml", not(feature = "json")))]
        {
            return serde_norway::to_string(self).expect("serialize yaml");
        }
        #[cfg(not(any(feature = "json", feature = "yaml")))]
        {
            format!("{} - {} passed, {} failed", self.name, self.passed, self.failed)
        }
    }
}

fn main() {
    let report = Report { name: "build-42".into(), passed: 128, failed: 3 };
    println!("{}", report.render());
}
```

Building with the default features (`json`) produces:

```
{
  "name": "build-42",
  "passed": 128,
  "failed": 3
}
```

Building with `cargo run --no-default-features --features yaml`:

```
name: build-42
passed: 128
failed: 3
```

Building with `cargo run --no-default-features` (no format at all):

```
build-42 - 128 passed, 3 failed
```

The `serde_norway` crate (the maintained successor to the deprecated `serde_yaml`; see [Beyond JSON](/15-serialization/06-other-formats/)) is **never downloaded, compiled, or linked** unless someone turns on the `yaml` feature. Disabled branches contribute nothing to the binary.

---

## Detailed Explanation

### The `[features]` table

Each entry maps a **feature name** to a list of things it enables. Those "things" come in three flavors:

```toml
[features]
default = ["json"]              # 1. other features (the default set)
json = ["dep:serde_json"]       # 2. an optional dependency, via `dep:`
full = ["json", "yaml", "metrics"] # 3. a bundle that enables several features
```

- **Enabling other features** lets you build convenience bundles. `full = ["json", "yaml", "metrics"]` turns on three features at once.
- **`dep:<name>`** turns on an optional dependency declared with `optional = true`. The `dep:` prefix (stable since Rust 1.60) keeps the dependency out of the implicit feature namespace, so adding an optional dependency does not silently create a same-named public feature.
- The special **`default`** feature is the set Cargo enables when nobody says otherwise.

### `#[cfg(feature = "...")]` — the attribute form

`#[cfg(...)]` is an **attribute** that conditionally includes the item it is attached to. If the predicate is false, the compiler behaves as if the item *does not exist in the source*:

```rust
#[cfg(feature = "metrics")]
fn report_metrics() {
    println!("metrics: 1 export performed");
}

// A fallback with the SAME name for when the feature is off, so callers
// always have a `report_metrics` to call.
#[cfg(not(feature = "metrics"))]
fn report_metrics() {
    // No-op when the `metrics` feature is off.
}
```

You can attach `#[cfg]` to almost anything: functions, structs, enum variants, `impl` blocks, modules, `use` statements, even individual struct fields. A common pattern is gating a whole module:

```rust
#[cfg(feature = "compression")]
pub mod compress {
    pub fn level() -> u32 {
        6
    }
}
```

### `cfg!(...)` — the expression form

`cfg!(...)` is a **macro** that evaluates to a `bool` at compile time. The key difference from `#[cfg]`: **both branches of the surrounding `if` must still type-check and compile**, because `cfg!` only chooses a value, it does not delete code:

```rust
fn main() {
    // Both arms must compile; only the VALUE is decided at compile time.
    let level = if cfg!(feature = "verbose") { "verbose" } else { "quiet" };
    println!("log level: {level}");
}
```

Use `cfg!` when both branches are valid regardless of the flag (e.g. choosing a string). Use `#[cfg]` when the disabled branch references items that only exist under the feature; those would fail to compile if you used `cfg!`.

### `#[cfg_attr(...)]` — conditional attributes

`#[cfg_attr(predicate, attribute)]` applies *another attribute* only when the predicate holds. This is how you conditionally derive a trait:

```rust
// Adds `#[derive(serde::Serialize)]` only when `serde` is enabled.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug)]
struct Config {
    retries: u32,
    endpoint: String,
}
```

When the `serde` feature is off, the type still derives `Debug` but not `Serialize`, and the `serde` crate is never pulled in.

### `cfg` predicates beyond features

`#[cfg]` is not limited to features. It is the same mechanism Rust uses for platform-specific code, which needs **no `[features]` entry at all**:

```rust
fn main() {
    if cfg!(target_os = "macos") {
        println!("running on macOS");
    } else {
        println!("running on a non-macOS platform");
    }
}
```

Common predicates include `target_os`, `target_arch`, `target_pointer_width`, `unix`, `windows`, `debug_assertions` (true in debug builds), and `test` (true under `cargo test`). They combine with `all(...)`, `any(...)`, and `not(...)`:

```rust
#[cfg(all(feature = "yaml", not(feature = "json")))]
```

### Where features come from at build time

Cargo passes the enabled features to `rustc` as `--cfg feature="json"` flags. The relevant command-line knobs:

| Flag | Effect |
| --- | --- |
| `--features "a b"` | Enable features `a` and `b` (space- or comma-separated). |
| `--no-default-features` | Do **not** enable the `default` set. |
| `--all-features` | Enable every feature the crate declares. |

These work on `cargo build`, `cargo run`, `cargo test`, `cargo check`, and friends.

---

## Key Differences

| Aspect | TypeScript/JavaScript | Rust features + `#[cfg]` |
| --- | --- | --- |
| When the decision happens | Runtime (`process.env`) or bundler build step | **Compile time**, by the compiler itself |
| Disabled code | Usually still shipped; tree-shaking is best-effort | **Not compiled at all**: zero bytes, zero cost |
| Optional dependency | `optionalDependencies` + guarded dynamic `import()` | `optional = true` dep pulled in by a feature via `dep:` |
| Failure mode for a missing optional dep | Throws at runtime when reached | Won't compile if you call code whose feature is off |
| Combining flags | Ad-hoc booleans | `all(...)` / `any(...)` / `not(...)` predicates |
| Cross-package coordination | None built in | **Feature unification** across the whole dependency graph |
| Intended semantics | Anything goes | **Additive** — features only add, never remove |

### Feature unification

This has no TypeScript/JavaScript equivalent and trips up newcomers. If crate `A` depends on `serde` with `features = ["derive"]` and crate `B` (in the same build) depends on `serde` with `features = ["rc"]`, Cargo compiles **one** copy of `serde` with the **union** `["derive", "rc"]`. Features are unified across the entire graph so that a single build of each crate satisfies everyone.

The direct consequence: **enabling a feature anywhere can enable it everywhere**, which is precisely why features must be *additive*. You can inspect the resolved feature graph with:

```bash
cargo tree -e features
```

> **Note:** Unlike npm, which can install multiple versions of the same package in nested `node_modules`, Cargo compiles one build of a crate per *semver-compatible* version with all requested features merged. There is no per-consumer feature isolation.

---

## Common Pitfalls

### Pitfall 1: Calling an item that is gated behind a disabled feature

If you reference a function that only exists under a feature, and that feature is off, the item is "configured out" and the call fails to compile:

```rust
#[cfg(feature = "advanced")]
fn turbo() -> u32 {
    9000
}

fn main() {
    // does not compile (error[E0425]) when `advanced` is OFF.
    println!("{}", turbo());
}
```

Building with the feature off produces a real, helpful error:

```
error[E0425]: cannot find function `turbo` in this scope
 --> src/main.rs:8:20
  |
8 |     println!("{}", turbo());
  |                    ^^^^^ not found in this scope
  |
note: found an item that was configured out
 --> src/main.rs:2:4
  |
1 | #[cfg(feature = "advanced")]
  |       -------------------- the item is gated behind the `advanced` feature
2 | fn turbo() -> u32 {
  |    ^^^^^

For more information about this error, try `rustc --explain E0425`.
```

The fix: either provide a `#[cfg(not(feature = "advanced"))]` fallback with the same signature, or wrap the **call site** in `#[cfg(feature = "advanced")]` too.

### Pitfall 2: Using `cfg!` where you needed `#[cfg]`

`cfg!` does not remove code; both arms must compile. This bites when the disabled arm uses a feature-gated item:

```rust
// If `serde_json` only exists under a feature, this still tries to compile
// the call in BOTH arms, so it fails when the feature is off.
let out = if cfg!(feature = "json") {
    serde_json::to_string(&value).unwrap() // referenced even when off!
} else {
    String::new()
};
```

Use the attribute form so the disabled branch is genuinely deleted:

```rust
#[cfg(feature = "json")]
let out = serde_json::to_string(&value).unwrap();
#[cfg(not(feature = "json"))]
let out = String::new();
```

### Pitfall 3: Designing non-additive (mutually exclusive) features

Because of feature unification, a feature that *removes* or *changes* behavior is dangerous: some other crate in the graph might turn it on, breaking your build in surprising ways. Anti-pattern:

```toml
[features]
fast = []
small = []   # "fast" and "small" are meant to be mutually exclusive — BAD design
```

If you truly cannot avoid a conflict, fail **loudly** at compile time with `compile_error!` rather than silently miscompiling:

```rust
#[cfg(all(feature = "fast", feature = "small"))]
compile_error!("features `fast` and `small` are mutually exclusive; enable only one");
```

Enabling both then yields a clear error instead of a subtle bug:

```
error: features `fast` and `small` are mutually exclusive; enable only one
 --> src/main.rs:4:1
  |
4 | compile_error!("features `fast` and `small` are mutually exclusive; enable only one");
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The better fix is almost always to redesign so the two paths are additive (e.g. pick at runtime, or split into two crates).

### Pitfall 4: Forgetting your crate compiles with `--no-default-features`

Downstream users routinely disable default features to trim builds. If your default-on code is the only thing that makes the crate usable, a `--no-default-features` build can break or do nothing useful. Test it in CI:

```bash
cargo check --no-default-features
cargo check --all-features
```

### Pitfall 5: Dead-code warnings when a field is only used by a gated derive

A struct field read *only* by a `#[cfg_attr(..., derive(Serialize))]` will warn as dead code when the feature is off, because the serialize impl that reads it no longer exists. Either read the field in some always-compiled path, or annotate it (`#[allow(dead_code)]`) deliberately. The compiler message is the standard `field is never read` / `#[warn(dead_code)]`.

---

## Best Practices

- **Keep features additive.** Enabling a feature should only *add* capability. Never make one feature turn another off or change a function's meaning.
- **Make `default` a sensible, minimal "it just works" set.** Put heavy or niche capabilities behind opt-in features so cost-conscious users can strip them with `--no-default-features`.
- **Use `dep:` to gate optional dependencies** instead of relying on the old implicit "dependency name becomes a feature" behavior. It keeps your public feature surface intentional.
- **Document every feature** in your crate docs and `README`, and consider a doc-comment table. Hidden features are a support burden.
- **Test the feature matrix in CI.** At minimum run `cargo check --no-default-features` and `cargo check --all-features`. The [cargo-hack](https://github.com/taiki-e/cargo-hack) tool can test each feature in isolation with `cargo hack check --feature-powerset`.
- **Prefer `#[cfg]` (attribute) for code that only exists under a feature**, and `cfg!` (macro) only when both branches genuinely compile.
- **Name features after capabilities, not implementations** (`compression`, not `flate2`), so you can swap the implementation later without a breaking change.
- **Re-enable docs for hidden features.** When publishing, [docs.rs](https://docs.rs) can be told which features to document via `[package.metadata.docs.rs]` in `Cargo.toml`; see [Publishing to crates.io](/12-modules-packages/11-publishing/).

> **Tip:** Use `cargo tree -e features` to see exactly which features the resolver turned on and *why*. It is the single most useful command for debugging "why is this dependency being built?".

---

## Real-World Example

A small library crate that gates an entire module and an optional dependency behind features: the shape of a typical published crate.

```toml
# Cargo.toml
[package]
name = "archive"
version = "0.1.0"
edition = "2024"

[features]
default = ["compression"]
compression = ["dep:flate2"]
logging = []

[dependencies]
flate2 = { version = "1", optional = true }
```

```rust
// src/lib.rs
//! A tiny library that gates an entire module behind a feature.

pub fn version() -> &'static str {
    "0.1.0"
}

/// Compiled only when the `compression` feature is enabled (it is, by default).
#[cfg(feature = "compression")]
pub mod compress {
    /// The default zlib compression level.
    pub fn level() -> u32 {
        6
    }
}

/// An optional, zero-dependency logging hook.
#[cfg(feature = "logging")]
pub fn log(msg: &str) {
    eprintln!("[log] {msg}");
}
```

A downstream consumer that depends on this crate writes, in *its* `Cargo.toml`:

```toml
[dependencies]
# Take only what we need: no compression, but turn on logging.
archive = { version = "0.1", default-features = false, features = ["logging"] }
```

With the default features on, `cargo tree` shows `flate2` and its transitive dependencies being built:

```
archive v0.1.0 (/path/to/archive)
└── flate2 v1.1.9
    ├── crc32fast v1.5.0
    │   └── cfg-if v1.0.4
    └── miniz_oxide v0.8.9
        ├── adler2 v2.0.1
        └── simd-adler32 v0.3.9
```

Building the same crate with `cargo build --no-default-features` drops `flate2` and everything under it from the build graph entirely. The `compress` module simply does not exist in that build, so nothing references the compression dependency.

> **Note:** This is a real strength over the TypeScript/JavaScript optional-dependency story: the dependency is not merely *unused at runtime*, it is **never fetched or compiled**, which shrinks build times, binary size, and the audited dependency surface.

---

## Further Reading

- [The Cargo Book — Features](https://doc.rust-lang.org/cargo/reference/features.html) — the authoritative reference for `[features]`, `dep:`, and unification.
- [The Cargo Book — Features Examples](https://doc.rust-lang.org/cargo/reference/features-examples.html) — patterns for default features, optional deps, and feature bundles.
- [The Rust Reference — Conditional compilation](https://doc.rust-lang.org/reference/conditional-compilation.html) — the full grammar of `cfg`, `cfg_attr`, and every built-in predicate.
- [`std::cfg!` macro docs](https://doc.rust-lang.org/std/macro.cfg.html) — the expression form.
- Related pages in this section: [Specifying Dependencies](/12-modules-packages/06-dependencies/) (enabling a dependency's features), [Dev-Dependencies, Build-Dependencies, and Optional Dependencies](/12-modules-packages/07-dev-dependencies/) (`optional = true` and build/dev deps), [Cargo.toml](/12-modules-packages/04-cargo/) (the manifest as a whole), [Cargo Workspaces](/12-modules-packages/08-workspaces/) (features across a workspace), and [Publishing to crates.io](/12-modules-packages/11-publishing/) (documenting features on docs.rs).
- Foundations: [Section 00 — Introduction](/00-introduction/), [Section 01 — Getting Started](/01-getting-started/), and [Section 02 — Basics](/02-basics/).
- Features pair naturally with conditional tests; see [Section 13 — Testing](/13-testing/) for `#[cfg(test)]` and feature-gated test suites.

---

## Exercises

### Exercise 1: Add a feature-gated function

**Difficulty:** Easy

**Objective:** Declare a feature and gate a function behind it so the crate builds both with and without the feature.

**Instructions:** Starting from a fresh `cargo new feat-ex1`, add a `pretty` feature to `Cargo.toml`. Write a `greeting()` function that returns `"=== Hello ==="` when `pretty` is on and `"Hello"` when it is off. Print it from `main`. Verify both `cargo run` and `cargo run --features pretty`.

```toml
# Cargo.toml
[package]
name = "feat-ex1"
version = "0.1.0"
edition = "2024"

[features]
# TODO: declare the `pretty` feature
```

```rust
// src/main.rs
// TODO: write two cfg-gated `greeting` functions and call one from main.
fn main() {
    println!("{}", greeting());
}
```

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "feat-ex1"
version = "0.1.0"
edition = "2024"

[features]
default = []
pretty = []
```

```rust
// src/main.rs
#[cfg(feature = "pretty")]
fn greeting() -> &'static str {
    "=== Hello ==="
}

#[cfg(not(feature = "pretty"))]
fn greeting() -> &'static str {
    "Hello"
}

fn main() {
    println!("{}", greeting());
}
```

**Output of `cargo run`:**

```
Hello
```

**Output of `cargo run --features pretty`:**

```
=== Hello ===
```

Providing both a `#[cfg(feature = "pretty")]` and a `#[cfg(not(feature = "pretty"))]` definition means `greeting` always exists, so the call site never needs its own `#[cfg]`.

</details>

---

### Exercise 2: A feature that enables other features

**Difficulty:** Medium

**Objective:** Build a `full` "bundle" feature that turns on two other features, and report which are active at runtime.

**Instructions:** Declare features `std` (in the default set), `extras`, and `full` such that enabling `full` enables both `std` and `extras`. In `main`, use `cfg!` to collect the active feature names into a `Vec<&str>` and print them joined by `", "`. Verify the output for the default build, `--features full`, and `--no-default-features`.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "feat-ex2"
version = "0.1.0"
edition = "2024"

[features]
default = ["std"]
std = []
# `full` turns on everything; a feature can enable other features.
full = ["std", "extras"]
extras = []
```

```rust
// src/main.rs
fn main() {
    let mut enabled: Vec<&str> = Vec::new();
    if cfg!(feature = "std") {
        enabled.push("std");
    }
    if cfg!(feature = "extras") {
        enabled.push("extras");
    }

    if enabled.is_empty() {
        println!("features: <none>");
    } else {
        println!("features: {}", enabled.join(", "));
    }
}
```

**Outputs:**

```
# cargo run                                  (default)
features: std

# cargo run --features full
features: std, extras

# cargo run --no-default-features --features full
features: std, extras

# cargo run --no-default-features
features: <none>
```

Note that `cfg!` is the right tool here because both arms of each `if` compile no matter which features are on: we are only branching on a `bool`, not deleting code.

</details>

---

### Exercise 3: Mutually exclusive features done safely

**Difficulty:** Hard

**Objective:** Model two conflicting build modes and make the *conflict* a compile error instead of a silent bug, while keeping a sensible default.

**Instructions:** Declare features `fast` and `small`. The program should print `"optimized for speed"` under `fast`, `"optimized for size"` under `small`, and `"balanced defaults"` under neither. Enabling **both** at once must fail to compile with a clear message. Verify: default build prints the balanced message, `--features fast` prints the speed message, and `--features "fast small"` fails with your message.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "feat-ex3"
version = "0.1.0"
edition = "2024"

[features]
default = []
fast = []
small = []
```

```rust
// src/main.rs
// Non-additive features are an anti-pattern; if a conflict is truly
// unavoidable, fail LOUDLY at compile time instead of miscompiling.
#[cfg(all(feature = "fast", feature = "small"))]
compile_error!("features `fast` and `small` are mutually exclusive; enable only one");

fn main() {
    if cfg!(feature = "fast") {
        println!("optimized for speed");
    } else if cfg!(feature = "small") {
        println!("optimized for size");
    } else {
        println!("balanced defaults");
    }
}
```

**Outputs:**

```
# cargo run                       (default)
balanced defaults

# cargo run --features fast
optimized for speed

# cargo build --features "fast small"
error: features `fast` and `small` are mutually exclusive; enable only one
 --> src/main.rs:4:1
  |
4 | compile_error!("features `fast` and `small` are mutually exclusive; enable only one");
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

> **Tip:** The "correct" production answer is usually to redesign so the two modes are additive (decide at runtime, or split into two crates), because feature unification means *some other crate in the graph* could enable both: the `compile_error!` guard is your safety net, not your design.

</details>
