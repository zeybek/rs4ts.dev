---
title: "Reducing Compile Time"
description: "Rust builds slower than tsc. Speed up the edit loop with Cargo workspaces, cargo check, taming monomorphization, codegen-units, and sccache caching."
---

Rust trades a slower compiler for a faster program. For a TypeScript/JavaScript developer used to instant `node script.js` and near-instant incremental `tsc`, the first painful surprise of Rust is often not the borrow checker. It is the wait for a release build. The good news: most of that wait is avoidable. This topic covers the levers that matter most for everyday iteration speed: splitting a project into a **workspace** of small crates, keeping **monomorphization** under control, tuning **`codegen-units`**, and caching compiler output with **`sccache`**.

---

## Quick Overview

Rust compilation is slow for principled reasons: it monomorphizes generics, runs LLVM optimizations, and proves memory safety: work that JavaScript's runtime JIT and TypeScript's erase-and-go type checker never do. You cannot make `rustc` as fast as `tsc`, but you can stop paying for work you do not need on each edit.

The four biggest, most controllable levers are:

1. **Crate boundaries.** The crate is Rust's unit of recompilation. Splitting a big binary into a **workspace** of smaller library crates lets Cargo rebuild only the crate you touched.
2. **Generics discipline.** Every distinct type a generic function is called with produces a *separate compiled copy* (**monomorphization**). Fewer instantiations means less code for LLVM to chew on.
3. **`codegen-units`.** How many parallel pieces `rustc` splits a crate into. More units compile faster (more parallelism) but optimize slightly worse: a dev-vs-release trade-off.
4. **`sccache`.** A shared compiler cache that reuses build artifacts across `cargo clean`, branches, and machines, like a content-addressed cache in front of `rustc`.

> **Note:** This file is about *build* time: how long you wait at your desk. *Run* time (how fast the program executes) is the subject of the rest of this section, starting with [Profiling Rust Applications](/21-performance/00-profiling/). The two sometimes conflict: the settings that make a build fast (high `codegen-units`, no LTO) make the program slightly slower, and vice versa.

---

## TypeScript/JavaScript Example

A growing TypeScript monorepo hits its own compile-time wall, and the fixes rhyme with Rust's. A typical package splits the codebase into independently built **project references** and turns on incremental builds:

```jsonc
// tsconfig.json at the repo root — a "solution" file referencing sub-projects.
{
  "files": [],
  "references": [
    { "path": "./packages/core" },
    { "path": "./packages/api" },
    { "path": "./packages/app" }
  ]
}
```

```jsonc
// packages/api/tsconfig.json — one project, depending on core.
{
  "compilerOptions": {
    "composite": true,        // required for a project reference
    "incremental": true,      // write a .tsbuildinfo cache
    "outDir": "./dist"
  },
  "references": [{ "path": "../core" }]
}
```

```bash
# Build only what changed, in dependency order, using the .tsbuildinfo caches.
tsc --build

# A CI cache of the .tsbuildinfo files makes a "clean" checkout build incrementally.
```

The lessons that carry over to Rust are exactly the ones above: **split the code into independently buildable units**, **only rebuild what changed**, and **cache build state so a fresh checkout is not a cold build**. What does *not* carry over is type erasure: TypeScript types vanish at runtime, so there is no per-type code duplication to worry about. Rust's generics are the opposite, and that difference is the heart of this topic.

---

## Rust Equivalent

The Rust analogue of a TypeScript monorepo with project references is a **Cargo workspace**: one repository, one shared `Cargo.lock`, one shared `target/` directory, and several member crates that depend on one another. Cargo rebuilds only the crates whose inputs changed.

```bash
# Lay out a workspace: a root manifest plus member crates.
mkdir -p ws/crates/core/src ws/crates/api/src ws/app/src
```

```toml
# ws/Cargo.toml — the workspace root. It has no [package]; it lists members.
[workspace]
resolver = "3"
members = ["crates/core", "crates/api", "app"]

# Shared dependency versions live here so every member agrees on one version.
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
```

```toml
# ws/crates/core/Cargo.toml — a leaf library crate (no dependencies).
[package]
name = "core-lib"
version = "0.1.0"
edition = "2024"
```

```rust
// ws/crates/core/src/lib.rs
pub fn add(a: i64, b: i64) -> i64 {
    a + b
}
```

```toml
# ws/crates/api/Cargo.toml — depends on core-lib via a path dependency.
[package]
name = "api-lib"
version = "0.1.0"
edition = "2024"

[dependencies]
core-lib = { path = "../core" }
```

```rust
// ws/crates/api/src/lib.rs
use core_lib::add;

pub fn sum3(a: i64, b: i64, c: i64) -> i64 {
    add(add(a, b), c)
}
```

```toml
# ws/app/Cargo.toml — the binary crate at the top of the dependency graph.
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
api-lib = { path = "../crates/api" }
```

```rust
// ws/app/src/main.rs
use api_lib::sum3;

fn main() {
    println!("{}", sum3(1, 2, 3));
}
```

The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically, and `resolver = "3"` is the edition-2024 feature resolver. Running `cargo run -p app` from the workspace root prints:

```text
6
```

The payoff shows up on the *next* edit. Touch only the leaf binary and Cargo rebuilds only that crate:

```text
$ touch app/src/main.rs
$ cargo build
   Compiling app v0.1.0 (.../ws/app)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s
```

`core-lib` and `api-lib` were not recompiled — their object code was reused. In a single mega-crate, the same edit would recompile *everything*.

---

## Detailed Explanation

### The crate is the unit of recompilation

`rustc` compiles one **crate** at a time and largely treats each as an atomic unit. Within a crate, *incremental compilation* (on by default for dev builds) reuses unchanged functions, but a change anywhere in a crate can still trigger a meaningful rebuild of that crate. Across crates, the boundary is firm: if crate `A` does not change, its compiled artifact is reused verbatim.

That makes the crate graph your most effective compile-time tool. A workspace splits one logical program into many crates so that:

- An edit to a leaf crate (the `app` binary above) recompiles only that crate.
- An edit to a shared library crate recompiles it *and* its dependents, but not unrelated siblings.
- Crates with no dependency relationship build **in parallel**.

Here is the cascade in the other direction. Touch the bottom of the graph and everything above it rebuilds, in dependency order:

```text
$ touch crates/core/src/lib.rs
$ cargo build
   Compiling core-lib v0.1.0 (.../ws/crates/core)
   Compiling api-lib v0.1.0 (.../ws/crates/api)
   Compiling app v0.1.0 (.../ws/app)
```

So the practical guidance is: **put the code you edit most often in a leaf crate, and the stable foundations underneath it.** A common shape is a thin `bin` crate (your `main.rs`, argument parsing, wiring) on top of a `lib` crate that holds the logic, so a one-line change to startup wiring never recompiles the library.

> **Tip:** Even a single-package project benefits from the `lib.rs` + `main.rs` split. Put logic in `src/lib.rs` and keep `src/main.rs` a tiny shim that calls into it. Integration tests and benches then compile against the library, not a recompiled copy of `main`.

### One `target/`, one `Cargo.lock`

Unlike a Node monorepo where each package may carry its own `node_modules`, a Cargo workspace shares a single `target/` directory and a single `Cargo.lock` at the root. A dependency used by three member crates is compiled **once** and linked into all three. There is exactly one `target/` directory for the whole workspace, not one per crate, which is also why CI caching is simpler than in a JavaScript monorepo.

```toml
# Declare shared dependency versions once in the root...
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
```

```toml
# ...and members opt in without repeating the version, keeping the graph deduplicated.
[dependencies]
serde = { workspace = true }
```

Deduplication matters for compile time: if two crates pull *different* versions of the same dependency, Cargo compiles both versions. Pinning versions through `[workspace.dependencies]` keeps the graph slim.

### Monomorphization: the hidden multiplier

This is the concept with no TypeScript analogue, and the one most likely to silently balloon your build. When you call a generic function, Rust does not compile one polymorphic version that inspects types at runtime (that is the TypeScript/JavaScript model, where types are erased). Instead it **monomorphizes**: it stamps out a separate, specialized copy of the function for every concrete type it is called with.

Consider a deliberately generic helper:

```rust
use std::fmt::Write;

// Generic: the compiler produces a fresh copy of `process` for EVERY
// distinct type T it is called with.
fn process<T: AsRef<str>>(input: T) -> usize {
    let s = input.as_ref();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        let _ = write!(out, "{i}:{c} ");
    }
    out.len()
}

fn main() {
    let mut total = 0;
    total += process("a literal");            // T = &str
    total += process(String::from("owned"));  // T = String
    total += process(&String::from("ref"));   // T = &String
    total += process(Box::<str>::from("b"));   // T = Box<str>
    let cow: std::borrow::Cow<str> = "c".into();
    total += process(cow);                     // T = Cow<str>
    println!("{total}");
}
```

Five call sites with five distinct `T` produce five distinct compiled functions. You can see them directly in the debug binary's symbol table — five separately-mangled copies of `process`:

```text
$ nm --demangle target/debug/mono | grep 'mono::process' | sort -u
mono::process::h4f5ed4eee67be37f
mono::process::h646e748f222b3523
mono::process::h7cc840d50059e702
mono::process::hdb2ee9e487dbd25a
mono::process::hdb79a394ab8c5de6
```

The compiler must type-check, optimize, and emit machine code for each one. The standard tool for measuring this is [`cargo-llvm-lines`](https://github.com/dtolnay/cargo-llvm-lines), which counts the LLVM IR lines generated per function (`cargo install cargo-llvm-lines`, then `cargo llvm-lines`). For the version above it reports:

```text
  Lines               Copies            Function name
   446 (18.9%, 18.9%)  5 (7.1%,  7.1%)  mono::process
```

446 lines of IR, 5 copies — the whole loop body was duplicated five times.

### Taming monomorphization with a thin generic shim

The fix is an old C++ trick adapted to Rust: keep the *generic* surface tiny, and delegate to a single *non-generic* inner function that holds the real work. The generic shim only does the cheap conversion; the expensive body is compiled exactly once.

```rust
use std::fmt::Write;

// The real work lives in ONE non-generic function, compiled exactly once.
fn process_inner(s: &str) -> usize {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        let _ = write!(out, "{i}:{c} ");
    }
    out.len()
}

// A tiny generic shim: convenient API, but its body is only `.as_ref()` + a call.
// The big loop is NOT duplicated per type.
fn process<T: AsRef<str>>(input: T) -> usize {
    process_inner(input.as_ref())
}

fn main() {
    let mut total = 0;
    total += process("a literal");
    total += process(String::from("owned"));
    total += process(&String::from("ref"));
    total += process(Box::<str>::from("b"));
    let cow: std::borrow::Cow<str> = "c".into();
    total += process(cow);
    println!("{total}");
}
```

The API is identical — callers still pass anything `AsRef<str>` — but the IR shrinks. `cargo llvm-lines` now reports:

```text
  Lines               Copies            Function name
   120 (5.7%, 20.4%)   5 (7.0%,  9.9%)  mono::process
    76 (3.6%, 45.0%)   1 (1.4%, 18.3%)  mono::process_inner
```

The five copies of `process` collapse to 120 lines total (just the conversion shim, repeated), plus one 76-line `process_inner`. The duplicated work fell from 446 lines to about 196. On a real codebase with deeply generic stacks (think builder APIs or serializers instantiated across dozens of types), this difference compounds into real seconds and a smaller binary.

The dynamic-dispatch alternative — taking `&dyn` or `&str` directly — removes monomorphization entirely at the cost of a runtime indirection. Whether to pay that runtime cost is exactly the kind of trade-off [Optimization Techniques](/21-performance/03-optimization/) and [Zero-Cost Abstractions](/21-performance/06-zero-cost/) explore. For *compile time*, fewer instantiations is almost always a win.

### `codegen-units`: parallelism versus optimization

`rustc` can split a single crate into N **codegen units** and hand them to LLVM in parallel. More units = more parallelism = faster compile, but the optimizer sees less of the whole crate at once, so the resulting code is slightly slower. Cargo's defaults already reflect this trade-off:

| Profile | Default `codegen-units` | Rationale |
| --- | --- | --- |
| `dev` (debug) | 256 | Maximize parallelism; you are iterating, not shipping. |
| `release` | 16 | Balance: parallel enough, but lets LLVM optimize across units. |

You can push the release build toward *faster compiles* (at a small runtime cost) by raising the count. This is accepted and passed straight through to `rustc`:

```toml
# Cargo.toml — trade a little runtime speed for a faster release compile.
[profile.release]
codegen-units = 256
```

Verifying that the flag reaches the compiler with a verbose build (`cargo build --release -v`) shows it on the `rustc` command line:

```text
codegen-units=256
```

The opposite tuning — `codegen-units = 1` plus `lto = true` — gives the fastest *program* but the slowest *build*; that combination belongs in [Reducing Binary Size](/21-performance/08-binary-size/) and your final release pipeline, not your edit loop.

### `sccache`: a shared compiler cache

`sccache` wraps `rustc` and caches its output keyed by a hash of the inputs (source, flags, compiler version, dependency fingerprints). On a cache hit it returns the cached object file instead of compiling. The cache survives `cargo clean`, branch switches, and — with a shared backend like S3 or Redis — your whole team and CI.

Install it once with `cargo install sccache`, then point Cargo at it as the `rustc` wrapper. The cleanest way is a project (or global) `~/.cargo/config.toml`:

```toml
# .cargo/config.toml — route every rustc invocation through sccache.
[build]
rustc-wrapper = "sccache"
```

(Equivalently, set `RUSTC_WRAPPER=sccache` in the environment for a one-off.) The first build of a fresh project is all misses:

```text
$ sccache --show-stats          # after a cold build
Compile requests                      7
Compile requests executed             1
Cache hits                            0
Cache misses                          1
Cache misses (Rust)                   1
Cache hits rate                    0.00 %
```

Now the part that `cargo`'s own incremental cache cannot do: wipe `target/` entirely and rebuild:

```text
$ cargo clean
     Removed 18 files, 949.9KiB total
$ cargo build --release          # warm sccache, empty target/
    Finished `release` profile [optimized] target(s) in 0.23s
$ sccache --show-stats
Cache hits rate                   100.00 %
Cache hits rate (Rust)            100.00 %
```

100% hit rate after a full `cargo clean`: the compiler did no real work, it copied cached objects. That is the scenario where `sccache` shines: CI runners and freshly-checked-out branches that would otherwise be cold builds.

> **Note:** Versions used here: `sccache` 0.15.0, `cargo-llvm-lines` 0.4.46. `sccache` does not cache *everything* — proc-macro and build-script outputs and some incremental artifacts are skipped, so do not expect 100% on a real workspace. It is most valuable for clean builds; for warm local edits, Cargo's incremental compilation already does the heavy lifting.

---

## Key Differences

| Aspect | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| What the "compiler" does | `tsc` erases types and emits JS; no machine code | `rustc` type-checks, borrow-checks, monomorphizes, runs LLVM to machine code |
| Generics at build time | Erased: one implementation, no duplication | Monomorphized: a separate copy per concrete type |
| Unit of incremental rebuild | File / project reference | **Crate** (with intra-crate incremental on top) |
| Caching across clean checkouts | `.tsbuildinfo`, cached in CI | `sccache` (and the shared `target/`) |
| Parallel build knob | Limited (`tsc` is largely single-threaded per project) | `codegen-units`, plus parallel crate builds |
| Cost model | Cheap compile, runtime cost paid by the JIT | Expensive compile, cheap runtime |
| Dev vs. prod build | Same `tsc`; bundlers add a separate step | `dev` and `release` are different Cargo profiles with different defaults |

The unifying idea: TypeScript pays at runtime for flexibility (the JIT specializes hot code as it runs); Rust pays at compile time (the compiler specializes *all* code ahead of time). Reducing Rust compile time is largely about reducing how much code the compiler must specialize and how often it must redo that work.

---

## Common Pitfalls

### Pitfall 1: One giant crate

The most common cause of slow Rust builds is a single crate with tens of thousands of lines. Any edit recompiles the whole thing, and nothing builds in parallel. **Symptom:** every save triggers a multi-second `Compiling my_app` line. **Fix:** carve out stable subsystems into library crates in a workspace, as shown above.

### Pitfall 2: Iterating with `cargo build` instead of `cargo check`

`cargo build` runs the full pipeline including LLVM codegen and linking. While you are fixing type and borrow errors, you do not need a runnable binary. `cargo check` stops after analysis and is substantially faster. It emits no executable:

```text
$ cargo check
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s
$ ls target/debug/checkdemo
ls: target/debug/checkdemo: No such file or directory   # no binary produced

$ cargo build
$ ls target/debug/checkdemo
target/debug/checkdemo                                  # build DID emit one
```

Wire `cargo check` (or `cargo clippy`) into your editor's on-save action and reserve `cargo build`/`cargo run` for when you actually need to run the program. (Profiling and benchmarking, by contrast, *require* a real optimized binary — see [Profiling Rust Applications](/21-performance/00-profiling/).)

### Pitfall 3: Invalid profile values that fail the build

`codegen-units` is an integer. A typo or a string value is rejected by Cargo before anything compiles. This snippet is intentionally broken:

```toml
# does not compile (Cargo rejects it: invalid type: string "max", expected u32)
[profile.release]
codegen-units = "max"
```

```text
$ cargo build --release
error: invalid type: string "max", expected u32
 --> Cargo.toml:7:17
  |
7 | codegen-units = "max"
  |                 ^^^^^
  |
```

There is no `"max"` sentinel; pass an actual number such as `256`.

### Pitfall 4: Over-generic public APIs

Sprinkling `<T: ...>` everywhere feels idiomatic, but each generic function multiplies across its instantiations. A library whose every function is generic forces *its consumers* to recompile a fresh copy per type. **Symptom:** `cargo llvm-lines` shows one of your functions with a high `Copies` count. **Fix:** apply the thin-shim pattern (generic wrapper, non-generic core), or accept a concrete type like `&str`/`&[u8]` where the flexibility is not actually used.

### Pitfall 5: Assuming `sccache` speeds up incremental edits

`sccache` and Cargo's incremental compilation overlap. For a normal warm edit-rebuild loop, Cargo's incremental cache is already doing the work, and `sccache` adds little (and is actually *incompatible* with incremental compilation; `sccache` disables `CARGO_INCREMENTAL` for the crates it caches). Its real value is **cold** builds: CI, clean checkouts, and switching branches. Do not expect it to accelerate the inner loop you run a hundred times a day.

### Pitfall 6: Heavy dependencies you barely use

A crate's compile cost is paid in full even if you use one function from it. Enabling a crate's default features (or a kitchen-sink feature like tokio's `"full"`) compiles code you may not need. **Fix:** turn off `default-features` and enable only the features you use, e.g. `tokio = { version = "1", default-features = false, features = ["rt", "macros"] }`. Auditing the dependency tree with `cargo tree` and the build timeline with `cargo build --timings` (below) shows where the time goes.

---

## Best Practices

- **Adopt a workspace early.** Even a two-crate split (a `lib` plus a thin `bin`) pays off. Keep frequently-edited code in leaf crates and stable foundations underneath.
- **Default to `cargo check` while coding.** Let your editor run it on save; build only to run.
- **Measure before tuning with `cargo build --timings`.** It writes an HTML report showing per-crate compile durations and the parallelism timeline:

  ```text
  $ cargo build --timings
     Compiling core-lib v0.1.0 (.../ws/crates/core)
     Compiling api-lib v0.1.0 (.../ws/crates/api)
     Compiling app v0.1.0 (.../ws/app)
        Timing report saved to .../target/cargo-timings/cargo-timing-<timestamp>.html
      Finished `dev` profile ... target(s) in 0.17s
  ```

  Open the HTML file to see which crate dominates; optimize *that* one.
- **Keep generics shallow at the boundary.** Use the generic-shim-over-concrete-core pattern for any generic function with a substantial body. Reach for `&dyn Trait` when you genuinely want one shared copy and can accept dynamic dispatch.
- **Trim features.** `default-features = false` plus an explicit feature list keeps dependency compile cost down. Run `cargo tree -d` to find duplicate versions to unify.
- **Profile-tune deliberately.** For the *edit loop*, you can even optimize only dependencies while keeping your own crate unoptimized:

  ```toml
  # Cargo.toml — your crate builds fast (unoptimized); deps build optimized once.
  [profile.dev.package."*"]
  opt-level = 3
  ```

  This is handy when a hot dependency (say, a parser or crypto crate) is unbearably slow at `opt-level = 0` but you do not want to optimize your own rapidly-changing code.
- **Use `sccache` where builds are cold.** Configure it for CI and shared dev machines via `[build] rustc-wrapper = "sccache"`. A shared S3/Redis backend turns teammates' and CI's prior builds into your cache hits.
- **Consider a faster linker for large binaries.** Linking can dominate the back end of a build. On a real project you can point Cargo at `lld` or `mold` via a target-specific `rustflags` block; the config shape is:

  ```toml
  # .cargo/config.toml — switch the linker for one host target.
  [target.aarch64-apple-darwin]
  rustflags = ["-C", "link-arg=-fuse-ld=lld"]
  ```

  (Install the linker first; the exact flag and triple depend on your platform.)

---

## Real-World Example

A web service grows from a prototype `main.rs` into a slow-to-build monolith. The refactor that fixes iteration speed is the workspace split below: a stable **domain** crate, an **api** crate with the request handlers, and a thin **server** binary that just wires them together. Editing a route handler now recompiles only the `api` crate and the `server` binary; the `domain` crate (where the slow-to-compile generic and serde code lives) is reused.

```toml
# Cargo.toml (workspace root)
[workspace]
resolver = "3"
members = ["crates/domain", "crates/api", "crates/server"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
# A release profile tuned for the final ship build, not the edit loop.
# (codegen-units = 1 + lto = "thin" is slow to build but fast at runtime.)

[profile.release]
codegen-units = 1
lto = "thin"
```

```rust
// crates/domain/src/lib.rs — stable types + the ONE non-generic core.
use serde::Serialize;

#[derive(Serialize)]
pub struct Money {
    pub cents: i64,
    pub currency: String,
}

/// The real formatting work: non-generic, compiled exactly once even though
/// callers pass many string-ish types through the shim below.
fn format_label_inner(name: &str, money: &Money) -> String {
    format!("{name}: {:.2} {}", money.cents as f64 / 100.0, money.currency)
}

/// Thin generic shim: ergonomic for callers, no per-type body duplication.
pub fn format_label<S: AsRef<str>>(name: S, money: &Money) -> String {
    format_label_inner(name.as_ref(), money)
}
```

```rust
// crates/api/src/lib.rs — request-shaped logic, depends on domain.
use domain::{format_label, Money};

pub fn line_item(name: &str, cents: i64) -> String {
    let money = Money { cents, currency: "USD".to_string() };
    format_label(name, &money)
}
```

```rust
// crates/server/src/main.rs — the thin binary you edit most often.
use api::line_item;

fn main() {
    // Pretend this is a route handler. Editing it recompiles only this crate.
    println!("{}", line_item("Pro plan", 4999));
    println!("{}", line_item("Add-on", 250));
}
```

With member manifests wiring `domain -> api -> server` by `path` dependency, `cargo run -p server` from the workspace root prints:

```text
Pro plan: 49.99 USD
Add-on: 2.50 USD
```

The compile-time wins are structural and verified by the recompile behavior shown earlier: a handler edit rebuilds `api` + `server` only; a `Money` change rebuilds all three; the generic-shim keeps `format_label`'s body from duplicating across every string type the handlers throw at it. The aggressive `[profile.release]` settings only bite when you actually run `cargo build --release` to ship — your daily `cargo check`/`cargo run` uses the fast `dev` profile.

---

## Further Reading

- [The Rust Performance Book — Compile Times](https://nnethercote.github.io/perf-book/compile-times.html) — the canonical survey: linkers, `codegen-units`, generics, workspaces, `sccache`, and more.
- [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html): the authoritative reference for the manifest format used above.
- [Cargo profiles reference](https://doc.rust-lang.org/cargo/reference/profiles.html): every profile key including `codegen-units`, `lto`, and per-package `[profile.dev.package."*"]` overrides.
- [`cargo build --timings` documentation](https://doc.rust-lang.org/cargo/reference/timings.html): how to read the HTML build-timeline report.
- [`cargo-llvm-lines`](https://github.com/dtolnay/cargo-llvm-lines): measure monomorphization bloat per function.
- [`sccache`](https://github.com/mozilla/sccache): installation, the `rustc-wrapper` setup, and shared cache backends (S3, Redis, GCS).

Related sections of this guide:

- [Profiling Rust Applications](/21-performance/00-profiling/): once builds are fast, find *runtime* hot spots.
- [Optimization Techniques](/21-performance/03-optimization/): the generic-vs-concrete trade-off from the runtime side.
- [Zero-Cost Abstractions](/21-performance/06-zero-cost/): why monomorphized generics produce code as fast as hand-written specializations.
- [Reducing Binary Size](/21-performance/08-binary-size/): the `codegen-units = 1` + LTO + `strip` end of the dial.
- [When to Optimize](/21-performance/10-when-to-optimize/) — measure first; the same discipline applies to build time.
- [Section 01: Getting Started](/01-getting-started/) — `cargo`, profiles, and project layout basics.
- [Section 02: Basics — Output](/02-basics/04-output/) — `println!` and formatting used throughout.
- [Common Patterns](/22-common-patterns/) — the lib/bin split and other structural idioms.

---

## Exercises

### Exercise 1: Split a binary into a workspace

**Difficulty:** Beginner

**Objective:** Experience the leaf-crate recompile benefit firsthand.

**Instructions:** Create a workspace with two crates: a library `calc-core` exposing `pub fn add(a: i64, b: i64) -> i64`, and a binary `calc` that calls it and prints the result. Build once. Then edit only `calc`'s `main.rs` (change the printed numbers) and rebuild. Confirm from the `Compiling ...` lines that `calc-core` is *not* recompiled.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml (workspace root)
[workspace]
resolver = "3"
members = ["calc-core", "calc"]
```

```toml
# calc-core/Cargo.toml
[package]
name = "calc-core"
version = "0.1.0"
edition = "2024"
```

```rust
// calc-core/src/lib.rs
pub fn add(a: i64, b: i64) -> i64 {
    a + b
}
```

```toml
# calc/Cargo.toml
[package]
name = "calc"
version = "0.1.0"
edition = "2024"

[dependencies]
calc-core = { path = "../calc-core" }
```

```rust
// calc/src/main.rs
use calc_core::add;

fn main() {
    println!("{}", add(2, 3));
}
```

First `cargo run -p calc` prints `5`. Now change the literals to `add(10, 20)` and rebuild — only `calc` recompiles:

```text
$ cargo build
   Compiling calc v0.1.0 (.../calc)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s
```

`calc-core` is absent from the output because its artifact was reused. Editing `calc-core/src/lib.rs` instead would recompile both crates, in dependency order.

</details>

### Exercise 2: Shrink monomorphization with a thin shim

**Difficulty:** Intermediate

**Objective:** Cut the generated code of an over-generic function using the wrapper-over-core pattern, and measure the result.

**Instructions:** Start from this fully-generic function and a `main` that calls it with several distinct types. Install `cargo-llvm-lines` (`cargo install cargo-llvm-lines`) and record the `Lines`/`Copies` for `count_words`. Then refactor so the real work lives in a single non-generic function and `count_words` is only a thin generic shim. Re-measure and confirm the duplicated work shrank.

```rust
// Starting point — refactor this.
fn count_words<T: AsRef<str>>(text: T) -> usize {
    let s = text.as_ref();
    let mut n = 0;
    for word in s.split_whitespace() {
        if !word.is_empty() {
            n += 1;
        }
    }
    n
}

fn main() {
    let owned = String::from("the quick brown fox");
    let total = count_words("a b c")            // T = &str
        + count_words(owned.clone())            // T = String
        + count_words(&owned)                   // T = &String
        + count_words(Box::<str>::from("x y")); // T = Box<str>
    println!("{total}");
}
```

<details>
<summary>Solution</summary>

```rust
// The work is non-generic now: compiled once regardless of how many
// string-ish types the shim is called with.
fn count_words_inner(s: &str) -> usize {
    let mut n = 0;
    for word in s.split_whitespace() {
        if !word.is_empty() {
            n += 1;
        }
    }
    n
}

// Thin generic shim: just the cheap `.as_ref()` conversion + a call.
fn count_words<T: AsRef<str>>(text: T) -> usize {
    count_words_inner(text.as_ref())
}

fn main() {
    let owned = String::from("the quick brown fox");
    let total = count_words("a b c")
        + count_words(owned.clone())
        + count_words(&owned)
        + count_words(Box::<str>::from("x y"));
    println!("{total}");
}
```

Running the program prints `13` (3 + 4 + 4 + 2 words). Running `cargo llvm-lines` before the refactor shows several copies of `count_words` each carrying the full loop body; after the refactor `count_words` collapses to a handful of lines per copy, plus a single `count_words_inner`. The duplicated loop is gone. The public signature is unchanged, so callers are unaffected — exactly the structural win this topic is about.

</details>

### Exercise 3: Tune profiles for the edit loop

**Difficulty:** Advanced

**Objective:** Configure a project so your own crate stays cheap to compile while heavy dependencies are still optimized, and confirm Cargo accepts the settings.

**Instructions:** In a project that has at least one dependency, add a `Cargo.toml` profile section that (a) keeps your crate at the default unoptimized `dev` `opt-level`, but (b) compiles *all dependencies* at `opt-level = 3`, and (c) raises the `release` profile's `codegen-units` to favor compile speed. Then deliberately introduce an invalid value to observe Cargo's error, and fix it.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "tuned"
version = "0.1.0"
edition = "2024"

[dependencies]
itoa = "1"   # any small dependency

# (a)+(b): your crate stays unoptimized for fast edits, but every dependency
# is built at opt-level 3 (once) so the program is not crippled at runtime.
[profile.dev.package."*"]
opt-level = 3

# (c): a release profile biased toward a fast compile.
[profile.release]
codegen-units = 256
```

`cargo build` and `cargo build --release` both succeed. Now introduce the deliberate error to see Cargo's validation:

```toml
# does not compile (Cargo rejects it: invalid type: string "max", expected u32)
[profile.release]
codegen-units = "max"
```

```text
$ cargo build --release
error: invalid type: string "max", expected u32
 --> Cargo.toml:...
  |
  | codegen-units = "max"
  |                 ^^^^^
```

Restoring the integer (`codegen-units = 256`) makes it build again. The takeaway: profile keys are validated up front, `codegen-units` is an integer with no `"max"` sentinel, and per-package profile overrides let you decouple *your* crate's compile cost from your dependencies' runtime quality.

</details>
