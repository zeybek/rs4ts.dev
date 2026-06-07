---
title: "Reducing Binary Size"
description: "A default Rust binary is ~450 KB. Cut it a third with opt-level, lto, strip, panic=abort, and codegen-units, and find the bloat using cargo-bloat."
---

## Quick Overview

A release Rust binary is **self-contained**: it bundles the standard library, your code, and every dependency into one executable with no separate runtime to install. That is wonderful for deployment, but the default release build optimizes for *speed*, not *size*, so even a trivial program lands around 400-500 KB. This topic shows the handful of `Cargo.toml` profile settings — `opt-level = "z"`, `lto`, `strip`, `panic = "abort"`, `codegen-units = 1` — and the `cargo-bloat` tool that, together, can cut a typical binary by a third or more, and explains the trade-offs so you know which knobs to turn for your deployment.

---

## Quick Overview for a TypeScript Developer

In Node.js you almost never think about binary size, because there is no binary. You ship `.js` source plus a `node_modules` tree, and the user supplies the runtime. The "binary" is the Node executable itself, which is roughly 110 MB and installed once. A Rust executable inverts this: a 19-byte `console.log("hi")` needs that 110 MB Node runtime to run, whereas an equivalent compiled Rust program is a single ~320 KB file that runs anywhere with no runtime at all. So when Rust binary size matters — Docker images, embedded devices, serverless cold-start, WebAssembly download size — you are optimizing the *one artifact that contains everything*, and the levers are build-time settings rather than bundler config.

> **Note:** This page is about the size of a native executable. Shrinking a **WebAssembly** `.wasm` module shares the same ideas (`opt-level = "z"`, LTO) plus `wasm-opt`; see [WebAssembly](/19-wasm/). Reducing *compile time* is a different goal with different (sometimes opposite) settings — see [Reducing Compile Time](/21-performance/07-compilation-time/).

---

## TypeScript/JavaScript Example

The closest Node equivalent to "shrink the artifact you ship" is bundling and minifying. You take an app plus its dependencies and produce one compact file:

```typescript
// build.ts — produce a single minified bundle with esbuild.
import { build } from "esbuild";

await build({
  entryPoints: ["src/index.ts"],
  bundle: true, // pull every import into one file
  minify: true, // shorten names, drop whitespace
  platform: "node", // target the Node runtime (not the browser)
  target: "node22",
  treeShaking: true, // drop unused exports
  outfile: "dist/app.js",
});
```

```bash
# Knobs you reach for in the JS world:
npx esbuild src/index.ts --bundle --minify --tree-shaking=true --outfile=dist/app.js
```

This shrinks the *source you distribute*, but two things are still true: the output is JavaScript text that the V8 engine parses and JIT-compiles at startup, and it still requires that separate ~110 MB Node runtime to execute. Bundling does not produce a standalone executable. (Tools like Node's `--experimental-sea` or `pkg` can embed your code *into a copy of Node*, which is why those "single-file" outputs are tens of megabytes — they include the whole runtime.)

---

## Rust Equivalent

In Rust the equivalent of "minify and tree-shake" lives in the **release profile** in `Cargo.toml`. There is no separate bundler step: `cargo build --release` already inlines, dead-code-eliminates, and links everything into one binary. You tune *how* it does that.

Start from a small but realistic program. Create it with `cargo new word_freq` (which selects the current stable 2024 edition automatically), then put this in `src/main.rs`:

```rust
use std::collections::HashMap;

/// Count word frequencies in the arguments and print the top 3.
fn main() {
    let text = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let mut counts: HashMap<String, u32> = HashMap::new();
    for word in text.split_whitespace() {
        let w = word.to_lowercase();
        *counts.entry(w).or_insert(0) += 1;
    }
    let mut pairs: Vec<(String, u32)> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (word, n) in pairs.iter().take(3) {
        println!("{word}: {n}");
    }
}
```

A plain `cargo build --release` produces a binary of **462,992 bytes** (~452 KB on this machine, Rust 1.96.0, the current stable, on macOS). Now add a size-tuned release profile:

```toml
# Cargo.toml
[package]
name = "word_freq"
version = "0.1.0"
edition = "2024"

[dependencies]

# Optimize the release build for the smallest possible binary.
[profile.release]
opt-level = "z"     # optimize for size, not speed
lto = true          # link-time optimization across the whole program
codegen-units = 1   # one codegen unit = more cross-function optimization
panic = "abort"     # drop the stack-unwinding machinery
strip = true        # remove symbol/debug info from the final binary
```

Rebuild and the same program is **319,296 bytes** (~312 KB), a 31% reduction with zero code changes:

```bash
cargo build --release
ls -l target/release/word_freq   # 319296 bytes, and it still works:
./target/release/word_freq "the cat sat on the mat the cat ran"
# the: 3
# cat: 2
# mat: 1
```

Those byte counts are real outputs from building this exact program; your numbers will differ by platform and toolchain version, but the *shape* of the reduction holds.

---

## Detailed Explanation

Each setting in `[profile.release]` does something specific. The numbers below are measured by adding one setting at a time to the `word_freq` program above, in order:

| Profile (cumulative)                 | Size (bytes) | Delta vs. default |
| ------------------------------------ | -----------: | ----------------: |
| `cargo build --release` (default)    |      462,992 |                 — |
| `+ opt-level = "z"`                  |      451,872 |             -2.4% |
| `+ lto = true`                       |      380,112 |            -17.9% |
| `+ codegen-units = 1`                |      379,824 |            -18.0% |
| `+ panic = "abort"`                  |      378,064 |            -18.3% |
| `+ strip = true`                     |      319,296 |            -31.0% |

### `opt-level = "z"` — optimize for size

The default release `opt-level` is `3`, which optimizes for *speed* and will happily make the binary bigger (more inlining, loop unrolling, vectorization). Two size-oriented levels exist:

- `opt-level = "s"` — optimize for size.
- `opt-level = "z"` — optimize for size *and* turn off loop vectorization, usually the smallest.

The name `"z"` is a quoted string, not the number `3`; mixing them up is a common typo. On its own `"z"` often gives a modest win (here, ~11 KB), but it pairs well with LTO.

> **Tip:** `"z"` is not *always* smaller than `"s"`, and neither is always smaller than `3`. For this tiny program with everything else enabled, `"s"` produced 319,248 bytes, `"z"` produced 319,296 bytes, and `opt-level = 3` produced 335,744 bytes. The differences between `s`/`z` are noise here; the real loser is `3`. Measure both `s` and `z` for *your* program rather than assuming.

### `lto = true` — link-time optimization

By default Rust optimizes each crate (and each codegen unit) in isolation, then links. **LTO** runs an extra optimization pass across the *entire* linked program, so the optimizer can inline across crate boundaries and delete code that nothing reaches. This was the single biggest win above (-72 KB). Options:

- `lto = false` (default for release) — no cross-crate LTO.
- `lto = "thin"` — a faster, lighter LTO; small size win, much less compile-time cost.
- `lto = true` (a.k.a. `"fat"`) — full LTO; best size and runtime, slowest to compile.

The cost is build time: LTO and `codegen-units = 1` make release builds noticeably slower to compile (see [Reducing Compile Time](/21-performance/07-compilation-time/)). That is fine for a release artifact you build occasionally, not for your inner dev loop.

### `codegen-units = 1` — one unit, maximum optimization

To parallelize compilation, Rust splits a crate into multiple **codegen units** (16 by default in release). More units compile faster but optimize each piece separately, which can leave duplicated or un-inlined code. Setting `codegen-units = 1` tells the compiler to treat the whole crate as one unit so it can optimize everything together. With LTO already on, the extra win is often tiny (here, under 300 bytes), but it is essentially free at runtime: you pay only in compile time.

### `panic = "abort"` — drop the unwinding machinery

By default a Rust `panic!` **unwinds** the stack: it walks back up through every frame, running destructors, so a `catch_unwind` at the top can recover. That unwinding requires landing-pad tables and personality routines embedded in the binary. Setting `panic = "abort"` makes a panic immediately abort the process instead, which lets the compiler omit all of that.

The behavioral consequence is real and worth understanding. With the default (unwind), `std::panic::catch_unwind` can intercept a panic:

```rust
fn main() {
    let result = std::panic::catch_unwind(|| {
        panic!("boom");
    });
    match result {
        Ok(_) => println!("caught nothing (no panic)"),
        Err(_) => println!("caught the panic via catch_unwind"),
    }
}
```

Default build output (the panic message goes to stderr, then control returns):

```text
thread 'main' panicked at src/main.rs:3:9:
boom
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
caught the panic via catch_unwind
```

With `panic = "abort"` in the profile, the same program prints the panic message and then the **process terminates** before reaching the match — the OS kills it with SIGABRT (exit code 134 on macOS/Linux):

```text
thread 'main' panicked at src/main.rs:3:9:
boom
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

So `panic = "abort"` is appropriate for CLIs, servers, and most binaries where a panic should crash the process anyway. Avoid it if you rely on `catch_unwind` (some plugin hosts and FFI boundaries do — see [Unsafe & FFI](/20-unsafe-ffi/)) or need destructors to run during a panic. The size win is usually small (-2 KB here) but it can be larger in programs with many functions, and it speeds up compilation too.

### `strip = true` — remove symbols

A compiled binary carries **symbol tables** and (with debuginfo) DWARF debugging data that map machine addresses back to function names and source lines. Profilers and debuggers need them — see [Profiling Rust Applications](/21-performance/00-profiling/) — but production users do not. `strip = true` removes them at link time. This was the second biggest win above (-59 KB), because the default release binary still includes name/symbol metadata.

`strip` accepts:

- `strip = "debuginfo"`: remove debug info, keep symbol names.
- `strip = "symbols"` (same as `strip = true`): remove both.

> **Note:** Stripping a release binary is built into Cargo since 1.59; you no longer need to run the external `strip` command in a post-build script. Doing it via the profile is reproducible and applies to every `cargo build --release`. For comparison, the external CLI `strip` on this program's default binary shrank it from 507,456 to 423,104 bytes; Cargo's `strip = true` reaches the same end result as part of the normal build.

---

## Finding what is big: `cargo-bloat`

Before you optimize, find out *what* fills the binary. [`cargo-bloat`](https://github.com/RazrFalcon/cargo-bloat) breaks down the `.text` section by function or by crate: the binary-size analogue of a bundle analyzer like `source-map-explorer` or `webpack-bundle-analyzer` in the JS world.

```bash
cargo install cargo-bloat
```

Run it against a release build of a program that uses `serde` and `serde_json` (`cargo add serde --features derive serde_json`):

```bash
cargo bloat --release -n 10
```

Real output (`cargo-bloat` 0.12.1):

```text
 File  .text     Size       Crate Name
 1.9%   3.8%   9.9KiB         std std::backtrace_rs::symbolize::gimli::resolve
 1.5%   3.1%   8.1KiB         std std::backtrace_rs::symbolize::gimli::Context::new
 1.3%   2.7%   7.1KiB         std gimli::read::dwarf::Unit<R>::new
 1.2%   2.6%   6.7KiB         std core::cell::once::OnceCell<T>::try_init
 0.8%   1.7%   4.5KiB serde_json? <&mut serde_json::de::Deserializer<R> as serde_core::de::Deserializer>::deserialize_struct
 0.7%   1.5%   3.9KiB         std gimli::read::unit::parse_attribute
 0.7%   1.4%   3.6KiB         std addr2line::function::Function<R>::parse_children
 0.6%   1.2%   3.1KiB         std core::cell::once::OnceCell<T>::try_init
 0.6%   1.2%   3.1KiB         std gimli::read::rnglists::RngListIter<R>::next
 0.6%   1.2%   3.0KiB         std std::backtrace_rs::symbolize::gimli::macho::Object::parse
39.7%  81.1% 211.2KiB             And 690 smaller methods. Use -n N to show more.
48.9% 100.0% 260.4KiB             .text section size, the file size is 532.2KiB
```

Two things jump out. First, much of the weight is the standard library's **backtrace/symbolization** machinery (`gimli`, `addr2line`) — exactly the kind of thing `strip` and `panic = "abort"` help remove. Second, you can ask for a per-crate summary:

```bash
cargo bloat --release --crates
```

```text
 File  .text     Size Crate
43.7%  89.3% 232.6KiB std
 4.5%   9.2%  23.8KiB serde_json
 0.5%   1.1%   2.7KiB [Unknown]
 0.4%   0.7%   1.9KiB serde_core
 0.3%   0.5%   1.4KiB zmij
 0.3%   0.5%   1.4KiB bloatdemo
 0.1%   0.1%     316B itoa
 0.0%   0.0%      32B __rustc
48.9% 100.0% 260.4KiB .text section size, the file size is 532.2KiB

Note: numbers above are a result of guesswork. They are not 100% correct and never will be.
```

The per-crate view is the high-value one: it tells you which *dependency* is paying rent. If one crate dominates and you only use a sliver of it, that is a signal to look for a lighter alternative or to disable default features. `cargo-bloat` prints its own honesty disclaimer because it attributes inlined and monomorphized code heuristically; treat the numbers as a guide, not gospel.

---

## Key Differences

| Concern               | TypeScript / Node.js                              | Rust                                                        |
| --------------------- | ------------------------------------------------- | ----------------------------------------------------------- |
| What you ship         | `.js` source + `node_modules`, or a bundle        | one self-contained native binary                            |
| Runtime requirement   | a separate ~110 MB Node install                   | none — the binary *is* everything                           |
| "Make it smaller"     | bundler flags: `--minify`, `--tree-shaking`       | `Cargo.toml` profile: `opt-level`, `lto`, `strip`, ...      |
| Dead-code elimination | tree-shaking (ESM static analysis)                | LTO + monomorphization-aware linking, automatic in release  |
| Symbol stripping      | n/a (source is text)                              | `strip = true` removes symbol/debug tables                  |
| Cost of optimizing    | longer build, sometimes harder stack traces       | longer compile time; abort changes panic semantics          |
| Typical "hello" size  | 19-byte source, but needs the 110 MB runtime      | ~320 KB standalone after size tuning                        |

The deepest difference: in Node, optimizing artifact size is about the *source* you distribute, and the runtime is fixed and huge. In Rust, the standard library is statically linked into *your* binary, so the floor for a `std` program is a few hundred KB, but that floor buys you a program with no external runtime dependency at all. For most server and CLI deployments a few hundred KB is irrelevant; binary size matters most for container images, embedded targets, serverless cold starts, and `.wasm` payloads.

---

## Common Pitfalls

### Putting size settings under `[profile.dev]` (or misspelling a key)

`cargo build` (debug) reads `[profile.dev]`; `cargo build --release` reads `[profile.release]`. If you put your size knobs in the wrong table they silently do nothing for release. Worse, Cargo does **not** error on a misspelled key. It warns and ignores it. Misspell `opt-level` as `opt-levl`:

```toml
[profile.release]
opt-levl = "z"   # typo: silently ignored
```

```text
warning: unused manifest key: profile.release.opt-levl
```

That is a *warning*, not an error, so it is easy to miss in CI logs. Read your build output, or double-check the binary actually shrank.

### Expecting `opt-level = "z"` to always be smallest

It is tempting to assume `z` < `s` < `3` for size. As measured above, `s` (319,248 bytes) edged out `z` (319,296 bytes) for this program, and only `opt-level = 3` was clearly larger. The right answer is workload-dependent. Measure all three; do not cargo-cult `"z"`.

### Forgetting `panic = "abort"` breaks `catch_unwind`

If your program (or a library you embed into, like a plugin host or an FFI callback boundary) depends on recovering from panics with `std::panic::catch_unwind`, turning on `panic = "abort"` will make those panics terminate the process (exit code 134 / SIGABRT) instead of being caught. The compiler will not warn you; it is a runtime behavior change. Know your panic strategy before flipping this.

### Optimizing size before knowing it matters

A 320 KB binary is not a problem for most deployments. LTO + `codegen-units = 1` can multiply your release compile time, and `opt-level = "z"` can make CPU-bound code measurably *slower* than `opt-level = 3`. If your bottleneck is throughput, you may be trading speed for bytes you do not need to save. Decide what you are optimizing for first — see [When to Optimize](/21-performance/10-when-to-optimize/).

### Stripping a binary you still need to profile

`strip = true` removes the symbols that profilers and debuggers rely on. If you strip your release artifact and then try to profile it, the flame graph will be a wall of unnamed addresses. Profile a build that *keeps* debug info ([Profiling Rust Applications](/21-performance/00-profiling/)), and ship a separate stripped build, or use a dedicated profile so the two never collide.

---

## Best Practices

- **Start with the four-line win.** `opt-level = "z"` (or `"s"`), `lto = true`, `strip = true`, and `panic = "abort"` give most of the reduction for almost no effort. Add `codegen-units = 1` if you can afford the compile time.
- **Measure both `s` and `z`** for your specific program; keep whichever is smaller (and not unacceptably slow).
- **Use a custom profile to keep speed and size builds separate.** Inherit from `release` so you only override what differs:

  ```toml
  # Cargo.toml — `cargo build --profile small` gives a size-tuned artifact
  # while plain `cargo build --release` stays speed-tuned.
  [profile.small]
  inherits = "release"
  opt-level = "z"
  lto = true
  codegen-units = 1
  panic = "abort"
  strip = true
  ```

- **Run `cargo bloat --release --crates` before reaching for exotic tricks.** Often one dependency dominates, and the cheapest fix is disabling its default features (`cargo add foo --no-default-features`) or swapping it for a lighter crate.
- **Trim dependency features.** Many crates pull in optional functionality by default; auditing features with `cargo bloat` (and `cargo tree`) often beats compiler flags.
- **For containers, combine a stripped static binary with a minimal base image** (`scratch` or `distroless`). The binary size and the image size are different numbers; both matter for pull times.
- **Reach for `#![no_std]` / `panic_immediate_abort` only for embedded or extreme `.wasm` targets.** They drop the standard library entirely and require nightly or special targets, far beyond the 90% of size savings the profile settings already deliver.

> **Warning:** Do not enable `lto = true` and `codegen-units = 1` on your default `dev` profile to "save space." They have no benefit for debug builds and will make every incremental compile painfully slow. Keep them on release/size profiles only.

---

## Real-World Example

A common production goal is a small Docker image for a JSON-handling CLI. Here is the full project, then the build settings that shrink it.

```toml
# Cargo.toml
[package]
name = "config_tool"
version = "0.1.0"
edition = "2024"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# A dedicated, size-tuned profile. Build with: cargo build --profile dist
[profile.dist]
inherits = "release"
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    name: String,
    retries: u32,
    verbose: bool,
}

fn main() {
    let cfg = Config {
        name: "service".to_string(),
        retries: 3,
        verbose: true,
    };
    let json = serde_json::to_string_pretty(&cfg).unwrap();
    println!("{json}");

    let parsed: Config = serde_json::from_str(&json).unwrap();
    println!("{parsed:?}");
}
```

Building this program three ways gives real, measured sizes (Rust 1.96.0, macOS):

| Build                                  | Size (bytes) |
| -------------------------------------- | -----------: |
| `cargo build --release` (default)      |      507,456 |
| external `strip` on the default binary |      423,104 |
| the size-tuned `dist` profile          |      335,744 |

The program still works after every change:

```bash
cargo build --profile dist
./target/dist/config_tool
# {
#   "name": "service",
#   "retries": 3,
#   "verbose": true
# }
# Config { name: "service", retries: 3, verbose: true }
```

A `Dockerfile` that ships only that stripped binary on a tiny base looks like this:

```dockerfile
# Stage 1: build the size-tuned binary.
FROM rust:1.96 AS build
WORKDIR /app
COPY . .
RUN cargo build --profile dist

# Stage 2: copy just the binary onto a minimal base image.
FROM gcr.io/distroless/cc-debian12
COPY --from=build /app/target/dist/config_tool /config_tool
ENTRYPOINT ["/config_tool"]
```

The final image contains a ~328 KB executable plus a minimal C runtime, orders of magnitude smaller than shipping a Node app, which must include the ~110 MB Node runtime layer.

---

## Further Reading

- [The Cargo Book — Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html): every profile key (`opt-level`, `lto`, `codegen-units`, `panic`, `strip`) and their defaults.
- [`min-sized-rust`](https://github.com/johnthagen/min-sized-rust): a thorough, regularly-updated checklist of every binary-size technique, from profile flags down to `#![no_std]`.
- [`cargo-bloat`](https://github.com/RazrFalcon/cargo-bloat): the size analyzer used above.
- [The Rustonomicon — Unwinding](https://doc.rust-lang.org/nomicon/unwinding.html): what `panic = "abort"` turns off, in depth.

Related sections of this guide:

- [Profiling Rust Applications](/21-performance/00-profiling/): keep debug info when you need to *profile*; the opposite of `strip`.
- [Optimization Techniques](/21-performance/03-optimization/): making code faster, which can affect size in either direction.
- [Reducing Compile Time](/21-performance/07-compilation-time/): LTO and `codegen-units = 1` are size wins but compile-time costs.
- [Performance](/21-performance/09-comparison/): how the standalone-binary model compares to Node overall.
- [When to Optimize](/21-performance/10-when-to-optimize/): decide whether size is even your problem before tuning.
- [WebAssembly](/19-wasm/): the same size ideas applied to `.wasm` modules.
- [Common Patterns](/22-common-patterns/): patterns that keep dependency trees (and therefore binaries) lean.
- New to Cargo profiles? Revisit [Understanding Cargo](/01-getting-started/03-cargo-basics/) and [Basics](/02-basics/).

---

## Exercises

### Exercise 1: Apply the four-line size profile

**Difficulty:** Beginner

**Objective:** Build the `word_freq` program from this page and measure the size reduction yourself.

**Instructions:**

1. Run `cargo new word_freq` and paste the word-frequency program from the **Rust Equivalent** section into `src/main.rs`.
2. Run `cargo build --release` and record the size of `target/release/word_freq` (use `ls -l` or `stat`).
3. Add a `[profile.release]` block with `opt-level = "z"`, `lto = true`, `strip = true`, and `panic = "abort"`.
4. Rebuild and record the new size. Confirm the program still produces correct output.

<details><summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "word_freq"
version = "0.1.0"
edition = "2024"

[dependencies]

[profile.release]
opt-level = "z"
lto = true
strip = true
panic = "abort"
```

```bash
cargo build --release
ls -l target/release/word_freq          # noticeably smaller than the first build
./target/release/word_freq "the cat sat on the mat the cat ran"
# the: 3
# cat: 2
# mat: 1
```

On the reference machine the default build was 462,992 bytes and the tuned build was ~319 KB, roughly a 31% reduction. Your exact numbers will differ by platform, but the program output is unchanged. The point: a substantial size win required *no* code edits, only profile settings.

</details>

### Exercise 2: Find the biggest contributor with `cargo-bloat`

**Difficulty:** Intermediate

**Objective:** Use `cargo-bloat` to identify which crate dominates a binary, then act on it.

**Instructions:**

1. `cargo install cargo-bloat`.
2. In a project that depends on `serde` and `serde_json`, run `cargo bloat --release --crates`.
3. Identify the crate (other than `std`) that contributes the most `.text`.
4. Run `cargo bloat --release -n 15` and note which *kinds* of functions dominate (hint: look for the standard library's backtrace/symbolization code).
5. Explain in one sentence which profile setting from this page would most directly shrink that backtrace code, and why.

<details><summary>Solution</summary>

```bash
cargo bloat --release --crates
#  File  .text     Size Crate
# 43.7%  89.3% 232.6KiB std
#  4.5%   9.2%  23.8KiB serde_json
#  ...
```

`serde_json` is the largest *dependency* (after `std`). The function-level view shows much of the weight is `std::backtrace_rs` / `gimli` / `addr2line`, the panic backtrace and symbolization machinery. The settings that most directly shrink it are **`strip = true`** (removes the symbol/debug tables those routines reference and shrinks the binary) and **`panic = "abort"`** (lets the compiler omit unwinding/landing-pad code). Stripping alone took the default `config_tool` binary from 507,456 to 423,104 bytes in the measurements on this page; the full size profile reached 335,744 bytes.

</details>

### Exercise 3: Separate "fast" and "small" builds

**Difficulty:** Advanced

**Objective:** Configure a project so the same `Cargo.toml` can produce a speed-tuned binary *and* a size-tuned binary on demand, then reason about the trade-off.

**Instructions:**

1. Keep `[profile.release]` at its speed-oriented defaults (or set `opt-level = 3`).
2. Add a second profile, `dist`, that `inherits = "release"` but overrides the settings for minimum size.
3. Build both: `cargo build --release` and `cargo build --profile dist`. Compare the two binary sizes.
4. Explain when you would ship each one. Bonus: build with `opt-level = "s"` vs `"z"` in the `dist` profile and report which is smaller for your program.

<details><summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "config_tool"
version = "0.1.0"
edition = "2024"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
opt-level = 3        # speed-tuned: the default, stated explicitly

[profile.dist]
inherits = "release"
opt-level = "z"      # size-tuned (try "s" too and compare)
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

```bash
cargo build --release            # target/release/config_tool  — fast
cargo build --profile dist       # target/dist/config_tool     — small
ls -l target/release/config_tool target/dist/config_tool
```

For this program, measured sizes were 507,456 bytes (default release) versus 335,744 bytes (`dist`). Ship `release` when CPU throughput dominates (the size win from `"z"` can come with a runtime cost). Ship `dist` when the artifact size is the constraint: container images, serverless cold starts, embedded targets, or anything you download over a network. The `s` vs `z` comparison is genuinely workload-dependent: in the measurements on this page `"s"` (319,248 bytes) was a hair smaller than `"z"` (319,296 bytes) for the `word_freq` program, so always test both rather than assuming `"z"` wins.

</details>
