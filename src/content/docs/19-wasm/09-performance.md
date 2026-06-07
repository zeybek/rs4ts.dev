---
title: "WebAssembly Performance: Bundle Size and the Boundary Cost"
description: "Compiling Rust to WebAssembly is not automatically faster: two costs decide if it pays off — the bytes downloaded, and every JS-WASM boundary crossing."
---

Compiling Rust to WebAssembly does not automatically make your app faster. Two costs decide whether the rewrite pays off: the **bytes a user downloads** before anything runs, and the **price of every call across the JavaScript↔WASM boundary**. This page measures both with real tools (`wasm-opt`, `twiggy`) and gives you a concrete decision rule for when WebAssembly actually wins over plain JavaScript.

---

## Quick Overview

A `.wasm` module is not free: the browser must download it, parse it, and instantiate it before your first function runs, and every value you pass into or out of it has to be **marshalled** across a boundary that natively speaks only numbers. For a TypeScript/JavaScript developer the mental model is "a second runtime you ship alongside your bundle, reached through a typed FFI." The performance work is therefore two-pronged: **shrink the binary** (with `wasm-opt` and the `twiggy` size profiler) so the download is small, and **design the API to cross the boundary rarely with large payloads** rather than often with tiny ones. WebAssembly wins decisively on CPU-bound, self-contained work (numeric kernels, parsing, compression, image processing) and loses on chatty, DOM-heavy, or trivially small tasks where the boundary and load costs dominate.

> **Note:** This file is about *measuring and tuning* performance. The build pipeline itself (`crate-type`, `--target`) lives in [Setting Up wasm-pack](/19-wasm/01-wasm-pack/); the mechanics of *what* crosses the boundary and *how* it is encoded live in [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/). Read those first if the terms `cdylib` or `JsValue` are new.

---

## TypeScript/JavaScript Example

A real frontend bundle has a download budget. Teams track it with tools like `source-map-explorer`, gzip every asset, and obsess over keeping the initial payload small because every kilobyte delays time-to-interactive. Here is the kind of CPU-bound workload a team might consider moving to WebAssembly (a numeric normalization pass over a large array, plus a tight prime-counting loop) written in idiomatic TypeScript first:

```typescript
// signal.ts — CPU-bound numeric work, the candidate for a WASM rewrite.

// Normalize an array so its largest value becomes 1.0.
export function normalizeAll(values: Float64Array): Float64Array {
  let max = 0;
  for (const v of values) {
    if (v > max) max = v;
  }
  if (max === 0) return values;
  const out = new Float64Array(values.length);
  for (let i = 0; i < values.length; i++) {
    out[i] = values[i] / max;
  }
  return out;
}

// A deliberately tight loop: count primes below `limit`.
export function countPrimes(limit: number): number {
  let count = 0;
  for (let n = 2; n < limit; n++) {
    let isPrime = true;
    for (let d = 2; d * d <= n; d++) {
      if (n % d === 0) {
        isPrime = false;
        break;
      }
    }
    if (isPrime) count++;
  }
  return count;
}
```

A subtle anti-pattern lurks in how such a module gets *called*. A naive integration normalizes element-by-element from JavaScript:

```typescript
// caller.ts — the WRONG shape once `normalizeOne` lives in WASM.
import { normalizeOne } from "./signal-wasm"; // hypothetical per-element WASM fn

const data = new Float64Array(1_000_000);
// ... fill data ...
const max = data.reduce((m, v) => Math.max(m, v), 0);
const out = new Float64Array(data.length);
for (let i = 0; i < data.length; i++) {
  out[i] = normalizeOne(data[i], max); // 1,000,000 boundary crossings!
}
```

In pure JavaScript that loop is fine: a function call inside V8 is cheap. The moment `normalizeOne` lives in WebAssembly, each call pays a boundary-crossing tax a million times over, and the "fast" Rust version can end up *slower* than the JavaScript it replaced. The rest of this page is about avoiding exactly that trap and shrinking what you ship.

---

## Rust Equivalent

Set up a standard `cdylib` library crate. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically.

```bash
cargo new --lib signal
cd signal
cargo add wasm-bindgen
```

```toml
# Cargo.toml
[package]
name = "signal"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2.122"
```

The crate exposes both the *wrong* and the *right* boundary shapes so we can contrast them. The key idea: pass the whole buffer across **once** and loop inside WASM, rather than calling across the boundary per element.

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

/// BAD boundary design: one call per element. Cheap inside WASM, but every
/// call re-crosses the JS↔WASM boundary, so the marshalling cost is paid N times.
#[wasm_bindgen]
pub fn normalize_one(value: f64, max: f64) -> f64 {
    if max == 0.0 {
        0.0
    } else {
        value / max
    }
}

/// GOOD boundary design: hand the whole buffer across once. wasm-bindgen copies
/// the JS `Float64Array` into linear memory a single time; the loop then runs
/// entirely inside WASM with no further boundary crossings, and the result
/// `Vec<f64>` is copied back out once.
#[wasm_bindgen]
pub fn normalize_all(values: &[f64]) -> Vec<f64> {
    let max = values.iter().cloned().fold(0.0_f64, f64::max);
    if max == 0.0 {
        return values.to_vec();
    }
    values.iter().map(|v| v / max).collect()
}

/// CPU-bound work with NO boundary traffic in the hot path: count the primes
/// below `limit`. One small argument in, one number out — the boundary cost is
/// a rounding error next to the compute. This is where WASM wins outright.
#[wasm_bindgen]
pub fn count_primes(limit: u32) -> u32 {
    let mut count = 0;
    for n in 2..limit {
        let mut is_prime = true;
        let mut d = 2;
        while d * d <= n {
            if n % d == 0 {
                is_prime = false;
                break;
            }
            d += 1;
        }
        if is_prime {
            count += 1;
        }
    }
    count
}
```

The logic is verified on the host (where the `rlib` crate type lets these functions run under `cargo test`):

```rust
// add to src/lib.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primes_and_normalize() {
        assert_eq!(count_primes(100), 25);
        assert_eq!(count_primes(1_000_000), 78_498);
        assert_eq!(normalize_all(&[2.0, 4.0, 8.0]), vec![0.25, 0.5, 1.0]);
    }
}
```

Running it on the host prints the real, verified results:

```text
count_primes(100) = 25
count_primes(1_000_000) = 78498
normalize_all([2,4,8]) = [0.25, 0.5, 1.0]
```

Now build for the browser and measure what actually ships. The `wasm-pack build` pipeline (compile → `wasm-bindgen` → `wasm-opt`) is covered in [Setting Up wasm-pack](/19-wasm/01-wasm-pack/); here we care about the **size at each stage**, captured from a real build of the crate above:

```text
build stage                          .wasm size
─────────────────────────────────────────────────
debug build (cargo build)            2,603,447 bytes   (~2.5 MB)
release build (cargo build --release)    43,387 bytes
after wasm-bindgen                       24,682 bytes
after wasm-opt -Os (wasm-pack default)   17,737 bytes
─────────────────────────────────────────────────
gzip on the wire                          8,203 bytes
brotli on the wire                        7,017 bytes
```

> **Note:** Exact byte counts vary by `rustc`/`wasm-bindgen`/`wasm-opt` patch version and platform, so a fresh reproduction will land near (not exactly on) these numbers. What is stable is the *shape*: a multi-megabyte debug build, a tens-of-KB release, a further drop through `wasm-bindgen` and `wasm-opt`, and roughly halving again on the wire.

Two lessons jump out immediately. First, **never ship a debug build**: at 2.5 MB it is ~150× larger than the optimized release. Second, **the bytes a user actually downloads are the compressed size** (8 KB gzip / 7 KB brotli), not the on-disk 17.7 KB; serving WASM with HTTP compression matters as much as `wasm-opt`.

---

## Detailed Explanation

### The bundle-size pipeline, stage by stage

The size table above is a pipeline, and each stage strips something different:

1. **Debug → release (2.5 MB → 43 KB).** The debug `.wasm` is dominated by debug info (DWARF) and zero optimization. `cargo build --release` (which `wasm-pack` runs by default) turns on `opt-level = 3` and drops most of that. This single step is the biggest win and is automatic.
2. **release → after `wasm-bindgen` (43 KB → 24.7 KB).** The `wasm-bindgen` CLI rewrites the module to add the JS-glue-facing exports and **runs the linker's dead-code elimination**, dropping Rust standard-library code your exports never reach.
3. **`wasm-bindgen` → after `wasm-opt` (24.7 KB → 17.7 KB).** `wasm-opt` (from the Binaryen toolkit) is a dedicated WASM-to-WASM optimizer. It does instruction-level shrinking, more aggressive dead-code elimination, and (with `-Os`/`-Oz`) size-focused rewrites. `wasm-pack` runs it for you with `-Os`.
4. **`wasm-opt` → on the wire (17.7 KB → ~7-8 KB).** Gzip/brotli on the HTTP layer roughly halves the binary again. WASM compresses well; this is "free" if your server is configured for it.

> **Note:** Unlike a JavaScript bundle, where minification (terser/esbuild) and tree-shaking happen in *your* bundler, the WASM equivalents (`wasm-bindgen`'s DCE and `wasm-opt`) happen in the *Rust* build pipeline before the bundler ever sees the file. Your Vite/webpack config does not shrink the `.wasm`; `wasm-opt` does.

### `wasm-opt`: the WASM minifier

`wasm-opt` is to a `.wasm` file what terser is to a `.js` file. `wasm-pack` invokes it automatically, but you can run it by hand to compare optimization levels. On the `wasm-bindgen` output of the crate above (24,682 bytes in), the real results were:

```text
wasm-opt level            output size   intent
──────────────────────────────────────────────────────
-Os (wasm-pack default)   17,737 bytes  balance size & speed
-Oz                       17,728 bytes  size at all costs
-O3                       18,334 bytes  speed at all costs
```

For this module the three levels land within ~600 bytes of each other; `-Os` and `-Oz` are nearly identical, and the speed-focused `-O3` is actually the *largest* because it inlines and unrolls. The practical guidance: **`-Os` is the right default** (and is what `wasm-pack` uses); reach for `-Oz` only when every kilobyte counts and you have benchmarked that the speed cost is acceptable, and `-O3` only when you have proven a hot path benefits from it.

> **Note:** Current Rust emits **bulk-memory** instructions (`memory.copy`, `memory.fill`), and `wasm-bindgen` embeds a `target_features` custom section in its output that declares them. A modern `wasm-opt` reads that section and **auto-enables** the matching features, so running `wasm-opt -Os input.wasm -o output.wasm` on real `wasm-bindgen` output just works, no extra flag, even with `--mvp-features`. You only hit a validation error like this real `wasm-opt` 129 message when the module *lacks* the `target_features` section (an older toolchain, or a hand-assembled module):
>
> ```text
> [wasm-validator error in function 3] unexpected false: memory.copy operations
> require bulk memory operations [--enable-bulk-memory-opt], on
> (memory.copy ...)
> Fatal: error validating input
> ```
>
> In that edge case the fix is to pass the feature flag explicitly: `wasm-opt -Os --enable-bulk-memory-opt input.wasm -o output.wasm`. With the documented toolchain (current Rust + `wasm-bindgen`) the section is present, so neither `wasm-pack` nor a manual `wasm-opt` needs the flag.

### `twiggy`: finding what is bloating the binary

When 17 KB is somehow 170 KB, you need to know *which Rust code* is responsible. `twiggy` is a code-size profiler for `.wasm` (think of it as `source-map-explorer` for WebAssembly). Install it once with `cargo install twiggy`, then run `twiggy top` against the **pre-`wasm-opt`** module (which still carries the function-name section `twiggy` reads). Real output from the crate above:

```text
$ twiggy top -n 5 signal_bg.wasm
 Shallow Bytes │ Shallow % │ Item
───────────────┼───────────┼─────────────────────────────────────────────────────
          4516 ┊    22.16% ┊ dlmalloc::dlmalloc::Dlmalloc<A>::malloc::h8212cd1e7...
          4379 ┊    21.49% ┊ "function names" subsection
          1005 ┊     4.93% ┊ data segment ".rodata"
           885 ┊     4.34% ┊ core::str::count::do_count_chars::haa2c4f188ad8cef2
           785 ┊     3.85% ┊ __rustc[de2ca18b4c54d5b8]::__rdl_realloc
          8809 ┊    43.23% ┊ ... and 111 more.
         20379 ┊   100.00% ┊ Σ [116 Total Rows]
```

This is enormously informative. The single biggest code item is **`dlmalloc::malloc`**, the default Rust allocator, baked into the binary at ~4.5 KB. The `"function names" subsection` (another ~4.4 KB) is the debug-name table that profiling needs but shipping does not. The string-handling helpers (`do_count_chars`, `__rdl_realloc`) come from `wasm-bindgen`'s string/array marshalling glue and the default allocator, not from your own logic. Knowing this, you can act: drop the name section for the shipped build, or reconsider whether a function that pulls in formatting/allocation is worth its weight.

`twiggy garbage` finds items that are present but unreachable, pure waste you can strip:

```text
$ twiggy garbage signal_bg.wasm
 Bytes │ Size % │ Garbage Item
───────┼────────┼─────────────────────────────────
   132 ┊  0.65% ┊ custom section 'target_features'
   102 ┊  0.50% ┊ custom section 'producers'
    55 ┊  0.27% ┊ __wbindgen_destroy_closure
    22 ┊  0.11% ┊ __wbindgen_exn_store
```

> **Tip:** Profile the module *with* names, ship the module *without* them. After `wasm-opt` strips the name section, `twiggy top` falls back to opaque labels like `code[0]`, `code[17]`, useful for sizing but not for attribution. Keep an un-stripped copy around for diagnosis.

### The JS↔WASM boundary cost

WebAssembly functions natively accept and return only `i32`, `i64`, `f32`, and `f64`. Anything richer — a string, a typed array, a struct — must be **encoded into those primitives, written into the module's linear memory, and decoded on the other side**. `wasm-bindgen` generates that marshalling glue (see [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/)), and it is fast, but it is *not free*:

- **Passing a number** (`count_primes(u32)`) is essentially free; it is already a WASM-native type.
- **Passing a `&str` or `&[f64]`** copies the bytes from the JS heap into WASM linear memory. The cost is proportional to the length, paid once per call.
- **Returning a `String` or `Vec<T>`** copies the bytes back out of linear memory into a fresh JS value.
- **Touching the DOM or a Web API** from Rust is an *import* call back into JavaScript for every operation, each one a boundary crossing in the other direction.

The disastrous pattern is **many tiny crossings**. Calling `normalize_one(value, max)` a million times pays the call-and-marshal overhead a million times; the per-call compute (one division) is far smaller than the per-call overhead, so JavaScript's in-engine loop wins. Calling `normalize_all(values)` once pays the boundary cost **twice total** (array in, array out) and runs the million divisions inside WASM at native speed. Same math, opposite outcome.

> **Note:** This is the inverse of the usual JavaScript intuition. In JavaScript, "extract a helper function" is a cost-free refactor. Across the WASM boundary, "call a helper a million times" is a performance bug. Batch the work.

### When WebAssembly actually wins

Put the two costs together and a clear rule emerges. WebAssembly pays off when **compute per boundary crossing is high** and the **module is downloaded rarely** (cached, reused across many operations). It loses when crossings are frequent and the work per crossing is trivial.

| Workload | WASM verdict | Why |
| --- | --- | --- |
| Image/video filters, codecs | Wins big | Megabytes processed per call; pure compute |
| Cryptography, hashing, compression | Wins big | CPU-bound, self-contained, one buffer in/out |
| Physics/game simulation, ray tracing | Wins big | Tight numeric loops, predictable memory |
| Parsing/validating large documents | Wins | One big string in, structured result out |
| Spreadsheet/formula engines | Wins | Heavy recompute, batched results |
| DOM manipulation (per element) | Usually loses | Every DOM op is a boundary crossing back to JS |
| Tiny per-event handlers (a click → one add) | Loses | Boundary + load cost dwarfs the work |
| String concatenation, JSON glue | Loses | V8 is already excellent; marshalling overhead added |
| First-paint-critical, tiny logic | Loses | The download/instantiate delay hurts more than it helps |

---

## Key Differences

| Concern | TypeScript / JavaScript | Rust → WebAssembly |
| --- | --- | --- |
| What ships | minified `.js`, tree-shaken by bundler | `.wasm` binary, shrunk by `wasm-bindgen` DCE + `wasm-opt` |
| Minifier | terser / esbuild (in your bundler) | `wasm-opt` (in the Rust pipeline, before the bundler) |
| Size profiler | `source-map-explorer`, bundle analyzers | `twiggy top` / `twiggy garbage` |
| Startup cost | parse JS (incremental, lazy) | fetch + parse + instantiate the whole `.wasm` upfront |
| Function-call cost | cheap (in-engine) | cheap inside WASM; **a boundary crossing has marshalling cost** |
| Passing a 1M-element array | reference, no copy | copied into linear memory (once) |
| "Extract a helper" refactor | free | free *inside* Rust; a footgun *across* the boundary |
| Compute speed (tight loops) | JIT-compiled, fast but variable | AOT-compiled, fast and predictable |

The deepest shift for a JavaScript developer: in JavaScript the runtime is already present and values are shared by reference, so calls are nearly free. In WebAssembly you ship a second compiled artifact, and the line between the two worlds is a real, measurable wall. Performance is won by **shipping few bytes** and **crossing the wall rarely with big payloads**.

> **Warning:** Do not assume "Rust is faster, so rewriting in WASM is faster." A WASM rewrite of chatty, DOM-bound, or trivially small JavaScript frequently ends up *slower* after you add download, instantiate, and per-call boundary costs. Measure the specific workload before committing.

---

## Common Pitfalls

### Shipping the debug build

The default `cargo build` produces a `.wasm` with full debug info: 2.5 MB in our example versus 17.7 KB optimized. `wasm-pack build` uses the release profile by default, so this bites mostly when you wire up your own build with a raw `cargo build` and forget `--release`. Always ship release; use `wasm-pack build --dev` only for fast local iteration.

### Designing a per-element boundary API

The single most common WASM performance mistake is exporting a fine-grained function and calling it in a JavaScript loop. Each call re-crosses the boundary. The fix is to export a batch function that takes the whole array/buffer and loops *inside* Rust: `normalize_all(&[f64])` instead of `normalize_one(f64, f64)` called a million times.

### Trying to pass a slice of non-numeric types by reference

Newcomers reach for `&[String]` or `&[SomeStruct]` to "pass a batch," expecting it to work like `&[f64]`. It does not compile. The real error from the crate above:

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn join_all(words: &[String]) -> String { // does not compile (E0277)
    words.join(" ")
}
```

```text
error[E0277]: the trait bound `[String]: RefFromWasmAbi` is not satisfied
 --> src/lib.rs:4:25
  |
4 | pub fn join_all(words: &[String]) -> String {
  |                         ^^^^^^^^ the trait `RefFromWasmAbi` is not implemented for `[String]`
  |
  = help: the following other types implement trait `RefFromWasmAbi`:
            [MaybeUninit<f32>]
            [MaybeUninit<f64>]
            [MaybeUninit<i16>]
            ...
```

Only slices of the numeric primitives marshal by reference (zero-allocation, straight into linear memory). For a batch of strings or structs you either accept a `Vec<String>` (which `wasm-bindgen` *can* take, at the cost of converting each element), or, for richer/nested data, serialize the whole batch once with `serde-wasm-bindgen` (covered in [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/)). Either way, cross **once**.

### Expecting a `bulk memory operations` error from `wasm-opt` (it usually does not happen)

A long-standing piece of folklore says you must pass `--enable-bulk-memory-opt` when running `wasm-opt` on Rust output, or it rejects the `memory.copy`/`memory.fill` instructions current Rust emits. With the documented toolchain that is no longer true: `wasm-bindgen` embeds a `target_features` section that `wasm-opt` reads to auto-enable the needed features, so `wasm-opt -Os input.wasm -o output.wasm` succeeds with no flag. You only see the `memory.copy ... require bulk memory operations` validation error on a module that lacks that section (an older toolchain, or a hand-assembled `.wat`); the fix there is to pass `--enable-bulk-memory-opt`.

### Profiling a stripped binary with `twiggy`

If `twiggy top` shows only `code[0]`, `code[17]`, `data[0]` instead of demangled Rust names, you ran it against a `wasm-opt`'d binary whose name section was stripped. Profile the *pre-`wasm-opt`* `wasm-bindgen` output (or build a copy that keeps debug names) to get attributable results, then ship the stripped one.

### Assuming `panic = "abort"` and LTO always shrink the binary

It is tempting to bolt on every size knob. For our small module the aggressive profile (`opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`) produced a **larger** final binary (19,327 bytes) than the default release profile plus `wasm-opt -Oz` (17,728 bytes). The knobs interact, and on a small module the gains can invert. **Measure, do not assume**: `twiggy` and a byte count are the arbiters.

---

## Best Practices

- **Ship release, never debug.** `wasm-pack build` does this by default; if you script `cargo build` yourself, always pass `--release`.
- **Let `wasm-pack` run `wasm-opt -Os` for you** as the sane default. It balances size and speed and sets the right Binaryen feature flags automatically.
- **Optimize the wire as well as the disk.** Serve `.wasm` with gzip or brotli (our 17.7 KB binary dropped to ~7-8 KB compressed). Configure your CDN/server for it; see [Deploying WebAssembly Applications](/19-wasm/10-deployment/).
- **Design coarse-grained boundary APIs.** One call that processes a whole buffer beats N calls that process one element. Pass `&[f64]`/`&[u8]` for numeric batches; serialize once for structured batches.
- **Profile with `twiggy` before you guess.** `twiggy top` to find the biggest items, `twiggy garbage` to find unreachable waste. Attack the largest contributors first (often the allocator and the standard-library formatting/panic machinery).
- **Reduce allocation and formatting in hot paths.** `dlmalloc` and `core::fmt` repeatedly show up as the biggest non-trivial code items; fewer `String`/`Vec` allocations and less `format!` shrink both size and runtime.
- **Reach for aggressive profile knobs only with a measurement in hand.** Try `opt-level = "z"` and `lto = true`, then *check the byte count*; they do not always help, especially on small modules.
- **Keep `console_error_panic_hook` to dev builds.** It improves panic messages during development but adds code; gate it behind a feature so it does not bloat production.
- **Cache the `.wasm` aggressively.** The instantiate cost is paid once; a content-hashed, far-future-cached binary means repeat visits skip the download entirely, tilting the cost/benefit toward WASM.

---

## Real-World Example

A production pattern: a Rust crate that does one CPU-heavy job (here, normalizing a large signal buffer), built and tuned for size. The crate uses a coarse-grained boundary API and a dev-only panic hook gated behind a Cargo feature so it never reaches production.

```toml
# Cargo.toml
[package]
name = "signal"
version = "0.1.0"
edition = "2024"
description = "A size-tuned Rust→WASM numeric kernel"
license = "MIT"

[lib]
crate-type = ["cdylib", "rlib"]

[features]
# Enable only in dev builds: wasm-pack build --features debug-panics
debug-panics = ["dep:console_error_panic_hook"]

[dependencies]
wasm-bindgen = "0.2.122"
console_error_panic_hook = { version = "0.1.7", optional = true }
```

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

/// Install a readable panic handler. Compiled in ONLY when the `debug-panics`
/// feature is on, so production builds carry none of its code.
#[wasm_bindgen]
pub fn init() {
    #[cfg(feature = "debug-panics")]
    console_error_panic_hook::set_once();
}

/// Coarse-grained boundary API: the entire buffer crosses ONCE. The browser
/// passes a `Float64Array`; wasm-bindgen copies it into linear memory, the loop
/// runs at native speed inside WASM, and the result crosses back out once.
#[wasm_bindgen]
pub fn normalize_all(values: &[f64]) -> Vec<f64> {
    let max = values.iter().cloned().fold(0.0_f64, f64::max);
    if max == 0.0 {
        return values.to_vec();
    }
    values.iter().map(|v| v / max).collect()
}

/// Pure CPU work, near-zero boundary traffic: count primes below `limit`.
#[wasm_bindgen]
pub fn count_primes(limit: u32) -> u32 {
    let mut count = 0;
    for n in 2..limit {
        let mut is_prime = true;
        let mut d = 2;
        while d * d <= n {
            if n % d == 0 {
                is_prime = false;
                break;
            }
            d += 1;
        }
        if is_prime {
            count += 1;
        }
    }
    count
}

// Pure-logic tests run on the host via the `rlib` crate type — fast, no browser.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correctness() {
        assert_eq!(count_primes(1_000_000), 78_498);
        assert_eq!(normalize_all(&[2.0, 4.0, 8.0]), vec![0.25, 0.5, 1.0]);
    }
}
```

Build the lean production binary and profile it:

```bash
# Production: optimized, no panic hook, wasm-opt'd by wasm-pack.
wasm-pack build --target web

# Inspect the size budget before optimization strips names:
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir names \
  target/wasm32-unknown-unknown/release/signal.wasm
twiggy top -n 10 names/signal_bg.wasm
```

The call site is written to cross the boundary as little as possible: one call per buffer, not one per element:

```javascript
// app.mjs (browser, ES module)
import init, { normalize_all, count_primes } from "./pkg/signal.js";

await init(); // fetch + instantiate the .wasm — paid once, then cached.

// GOOD: one boundary crossing for the whole array.
const data = Float64Array.from({ length: 1_000_000 }, (_, i) => i);
const normalized = normalize_all(data); // single call, single copy each way

// CPU-bound, near-zero boundary traffic — WASM's sweet spot.
console.log(count_primes(1_000_000)); // 78498
```

The result `count_primes(1_000_000) === 78498` is computed entirely inside WebAssembly, and `normalize_all` touches the boundary exactly twice regardless of array length. The shipped binary is ~17.7 KB on disk and ~7-8 KB over a brotli-compressed connection, a payload small enough that the CPU savings on the million-element pass clearly justify it.

---

## Further Reading

### Official Documentation

- [Shrinking `.wasm` size — Rust and WebAssembly book](https://rustwasm.github.io/docs/book/reference/code-size.html) — the canonical size-reduction checklist (`wasm-opt`, allocators, feature gating).
- [`twiggy` documentation](https://rustwasm.github.io/twiggy/) — the code-size profiler: `top`, `dominators`, `garbage`, `diff`.
- [Binaryen / `wasm-opt`](https://github.com/WebAssembly/binaryen) — the WASM-to-WASM optimizer `wasm-pack` runs for you.
- [`wasm-bindgen` Guide: the boundary](https://wasm-bindgen.github.io/wasm-bindgen/) — exactly how values are marshalled across JS↔WASM.
- [WebAssembly performance — MDN](https://developer.mozilla.org/en-US/docs/WebAssembly) — instantiation, streaming compilation, and runtime characteristics.

### Related Sections

- [Section 19 README](/19-wasm/) — the full WebAssembly section index.
- [What Is WebAssembly and Why Compile Rust to It?](/19-wasm/00-wasm-intro/) — what WASM is and the realistic use cases where Rust→WASM beats plain JavaScript.
- [Setting Up wasm-pack](/19-wasm/01-wasm-pack/) — the build pipeline, `crate-type`, and targets that produce the binaries measured here.
- [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/) — the boundary in depth: `JsValue`, `serde-wasm-bindgen`, and what each type costs to cross.
- [Calling JavaScript from Rust](/19-wasm/03-js-interop/) — calling JS from Rust, where import-side boundary crossings add up.
- [Manipulating the DOM from Rust with web-sys](/19-wasm/07-dom-manipulation/) — why per-element DOM work is boundary-heavy and how to batch it.
- [Deploying WebAssembly Applications](/19-wasm/10-deployment/) — serving `.wasm` with the right MIME type and HTTP compression to realize the wire-size wins.
- [Section 01: Cargo basics](/01-getting-started/03-cargo-basics/) — release profiles and `Cargo.toml` profile settings.
- [Section 02: Types](/02-basics/01-types/) — the numeric types (`f64`, `u32`) that cross the boundary for free.
- [Section 20: Unsafe & FFI](/20-unsafe-ffi/) — the C-ABI cousin of the WASM boundary, with similar marshalling-cost trade-offs.

---

## Exercises

### Exercise 1: Measure the size pipeline yourself

**Difficulty:** Beginner

**Objective:** See the debug-vs-release-vs-`wasm-opt` size collapse with your own eyes.

**Instructions:**

1. Create a `cdylib` crate exporting `#[wasm_bindgen] pub fn count_primes(limit: u32) -> u32` (use the body from this page).
2. Build it three ways and record the `.wasm` byte size after each: a debug `cargo build --target wasm32-unknown-unknown`, a `cargo build --release --target wasm32-unknown-unknown`, and a full `wasm-pack build --target web`.
3. Run `gzip -9 -c pkg/<name>_bg.wasm | wc -c` to see the compressed wire size. State which stage saved the most bytes.

<details>
<summary>Solution</summary>

```bash
cargo new --lib primes && cd primes
cargo add wasm-bindgen
# set [lib] crate-type = ["cdylib", "rlib"] in Cargo.toml
```

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn count_primes(limit: u32) -> u32 {
    let mut count = 0;
    for n in 2..limit {
        let mut is_prime = true;
        let mut d = 2;
        while d * d <= n {
            if n % d == 0 {
                is_prime = false;
                break;
            }
            d += 1;
        }
        if is_prime {
            count += 1;
        }
    }
    count
}
```

```bash
cargo build --target wasm32-unknown-unknown
ls -l target/wasm32-unknown-unknown/debug/primes.wasm    # ~MBs (debug info)

cargo build --release --target wasm32-unknown-unknown
ls -l target/wasm32-unknown-unknown/release/primes.wasm  # tens of KB

wasm-pack build --target web
ls -l pkg/primes_bg.wasm                                 # smaller still (wasm-opt)
gzip -9 -c pkg/primes_bg.wasm | wc -c                    # ~half again
```

The **debug → release** step saves by far the most (dropping megabytes of debug info), followed by `wasm-bindgen`'s dead-code elimination and `wasm-opt`. Gzip then roughly halves the final binary on the wire. The headline lesson: the single most important thing is to ship a release build.

</details>

### Exercise 2: Profile and attribute the bloat

**Difficulty:** Intermediate

**Objective:** Use `twiggy` to find what is taking space, and confirm that profiling needs the un-stripped binary.

**Instructions:**

1. Take the crate from Exercise 1 (or add a `reverse_words(&str) -> String` function to pull in more standard-library code).
2. Install `twiggy` (`cargo install twiggy`). Run `twiggy top -n 10` against the **`wasm-bindgen` output** (the `pkg/<name>_bg.wasm` *before* you re-run `wasm-opt` to strip names, or a fresh `wasm-bindgen` run with names kept).
3. Run `twiggy top` again against a `wasm-opt`'d copy and observe that the names become opaque (`code[N]`). Explain in one sentence why you profile the un-stripped binary but ship the stripped one.

<details>
<summary>Solution</summary>

```bash
cargo install twiggy   # one-time

# Build a names-bearing module (the wasm-bindgen output keeps the name section):
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir names \
  target/wasm32-unknown-unknown/release/primes.wasm

twiggy top -n 10 names/primes_bg.wasm
```

Real output is dominated by the allocator and the name section:

```text
 Shallow Bytes │ Shallow % │ Item
───────────────┼───────────┼──────────────────────────────────────────────
          4516 ┊    22.16% ┊ dlmalloc::dlmalloc::Dlmalloc<A>::malloc::h...
          4379 ┊    21.49% ┊ "function names" subsection
          1005 ┊     4.93% ┊ data segment ".rodata"
           885 ┊     4.34% ┊ core::str::count::do_count_chars::h...
```

```bash
# Now strip names the way a shipped build does:
wasm-opt -Os --enable-bulk-memory-opt names/primes_bg.wasm -o shipped.wasm
twiggy top -n 5 shipped.wasm   # items show as code[0], code[17], data[0], ...
```

You profile the **un-stripped** binary because `twiggy` needs the function-name section to attribute bytes to real Rust symbols; you ship the **stripped** binary because the name section is pure download weight the user never needs. The biggest contributor here is the default allocator (`dlmalloc::malloc`): a hint that allocation-heavy code is expensive in both size and speed.

</details>

### Exercise 3: Fix a chatty boundary API

**Difficulty:** Advanced

**Objective:** Refactor a per-element boundary call into a single batched crossing and explain the performance difference.

**Instructions:**

1. Start from a `#[wasm_bindgen] pub fn square_one(x: f64) -> f64` intended to be called in a JavaScript loop over a million-element array.
2. Replace it with a batched `#[wasm_bindgen] pub fn square_all(xs: &[f64]) -> Vec<f64>` that performs the whole pass inside WASM.
3. In prose, explain how many boundary crossings each design incurs for an `N`-element array, and why the batched version is the one that lets WASM beat plain JavaScript.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

// BEFORE — chatty: one crossing PER element.
#[wasm_bindgen]
pub fn square_one(x: f64) -> f64 {
    x * x
}

// AFTER — batched: the whole array crosses ONCE in, once out.
#[wasm_bindgen]
pub fn square_all(xs: &[f64]) -> Vec<f64> {
    xs.iter().map(|x| x * x).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squares_a_batch() {
        assert_eq!(square_all(&[2.0, 3.0, 4.0]), vec![4.0, 9.0, 16.0]);
    }
}
```

Calling `square_one` from a JavaScript loop over an `N`-element array performs **`N` boundary crossings** — each pays the call-and-marshal overhead, while the per-call work (one multiply) is tiny, so the overhead dominates and JavaScript's in-engine loop is faster. Calling `square_all(xs)` performs **exactly 2 crossings total** regardless of `N` (the input slice copied into linear memory once, the result `Vec<f64>` copied out once); the million multiplies then run at native speed entirely inside WASM. The batched design amortizes the fixed boundary cost over the whole array, which is precisely the condition under which WebAssembly outperforms plain JavaScript.

</details>
