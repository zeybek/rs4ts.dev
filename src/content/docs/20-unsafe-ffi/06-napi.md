---
title: "Node.js Native Addons with napi-rs"
description: "Build Node.js native addons in Rust with napi-rs: annotate functions with #[napi] to move CPU-bound work off JavaScript, with generated TypeScript types and no C++."
---

Sometimes a hot path in your Node.js service is simply too slow in JavaScript, or you need to reuse a battle-tested Rust crate from your existing TypeScript codebase. A **native addon** lets you ship compiled Rust that Node loads and calls like any other module. **napi-rs** is the modern, ergonomic way to build one: you annotate ordinary Rust functions with `#[napi]`, and it generates the C glue, the loader, and even the TypeScript type definitions for you.

---

## Quick Overview

A **native addon** is a compiled binary (`.node` file) that exposes functions, classes, and values to JavaScript through Node's stable **Node-API** (formerly N-API). [napi-rs](https://napi.rs/) is a Rust framework that turns annotated Rust functions into a Node addon: write `#[napi] pub fn greet(name: String) -> String`, run one build command, and `require()` it from Node as if it were a normal package, with generated `.d.ts` types so TypeScript callers get full IntelliSense.

For a TypeScript or JavaScript developer, the value proposition is concrete: keep your Node service and its ecosystem, but move CPU-bound work (parsing, hashing, image processing, compression) into Rust that runs many times faster and never blocks the event loop. Because napi-rs targets the **stable Node-API ABI**, a single compiled `.node` binary keeps working across Node major versions without recompiling, unlike the older, header-bound native-module approach.

> **Note:** This page covers napi-rs specifically. The pure-C-ABI foundation underneath it is in [FFI Basics](/20-unsafe-ffi/03-ffi-basics/) and [Calling C from Rust](/20-unsafe-ffi/04-calling-c/). The alternative addon framework, Neon, and how it compares, is in [Neon](/20-unsafe-ffi/07-neon/). Compiling Rust for the browser instead of Node is [WebAssembly](/19-wasm/) territory.

---

## TypeScript/JavaScript Example

Today, when a Node.js codebase needs a native addon, the realistic options are: write painful C++ against `node-addon-api`, or call out to a separate binary over a child process. Here is the C++ approach, the thing napi-rs replaces. Even a trivial `fibonacci` function is verbose, manual, and easy to get wrong:

```cpp
// addon.cc — node-addon-api (C++). This is the status quo napi-rs improves on.
#include <napi.h>

// Every argument must be hand-unwrapped and type-checked by you.
Napi::Value Fibonacci(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();

  if (info.Length() < 1 || !info[0].IsNumber()) {
    Napi::TypeError::New(env, "expected a number").ThrowAsJavaScriptException();
    return env.Null();
  }

  uint32_t n = info[0].As<Napi::Number>().Uint32Value();
  uint64_t a = 0, b = 1;
  for (uint32_t i = 0; i < n; i++) {
    uint64_t next = a + b;
    a = b;
    b = next;
  }
  // You must also choose the right JS number representation by hand.
  return Napi::BigInt::New(env, a);
}

// And manually register every export into the module object.
Napi::Object Init(Napi::Env env, Napi::Object exports) {
  exports.Set("fibonacci", Napi::Function::New(env, Fibonacci));
  return exports;
}

NODE_API_MODULE(addon, Init)
```

```js
// binding.gyp + node-gyp build, then:
const { fibonacci } = require('./build/Release/addon.node');
console.log(fibonacci(50)); // 12586269025n
```

The argument unwrapping, the type checks, the `binding.gyp` build descriptor, the `node-gyp` toolchain, the manual `exports.Set` registration, and the hand-written `.d.ts` (if you want types at all) are all on you. There is no memory safety and no borrow checker: a stray pointer here is a production segfault.

---

## Rust Equivalent

Here is the entire equivalent in napi-rs. The Rust is plain Rust; the `#[napi]` attribute does the binding generation. Set up a library crate:

```toml
# Cargo.toml
[package]
name = "greet-native"
version = "0.1.0"
edition = "2024"          # cargo new selects the latest stable edition automatically

[dependencies]
napi = { version = "3", features = ["napi9"] }
napi-derive = "3"

[build-dependencies]
napi-build = "2"

[lib]
crate-type = ["cdylib"]    # produce a C-compatible dynamic library Node can load
```

```rust
// build.rs — runs napi's build-time setup (configures the linker for Node-API).
fn main() {
    napi_build::setup();
}
```

```rust
// src/lib.rs
use napi_derive::napi;

#[napi]
pub fn fibonacci(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let next = a + b;
        a = b;
        b = next;
    }
    a
}

#[napi]
pub fn greet(name: String) -> String {
    format!("Hello, {name}!")
}
```

Build it with the napi CLI (installed as a dev dependency: `npm install --save-dev @napi-rs/cli`):

```bash
# binaryName is read from the "napi" block in package.json; see below.
npx napi build --platform --release
```

That single command compiles the crate, copies the resulting `greet-native.<platform>.node` next to your JS, and **auto-generates** `index.js` (a loader that picks the right binary) plus `index.d.ts`. Here is the real generated `index.d.ts` from the code above:

```typescript
/* auto-generated by NAPI-RS */
/* eslint-disable */
export declare function fibonacci(n: number): bigint

export declare function greet(name: string): string
```

And calling it from Node:

```js
// test.mjs
import { fibonacci, greet } from './index.js';

console.log(greet('Ada'));
console.log('fib(50) =', fibonacci(50));
```

Real output from `node test.mjs`:

```text
Hello, Ada!
fib(50) = 12586269025n
```

No `binding.gyp`, no manual argument unwrapping, no hand-written types. The Rust function signature *is* the binding contract.

---

## Detailed Explanation

Let's walk through what each piece does and why, contrasting with the JavaScript/C++ side you know.

### The `#[napi]` attribute is a procedural macro

`#[napi]` is not a decorator (decorators in TypeScript wrap a value at runtime). It is a **procedural macro** that runs at *compile time* and rewrites your function: it emits an `extern "C"` wrapper with the exact signature Node-API expects, generates code to convert each JavaScript argument into the Rust type and the Rust return value back into a JS value, and registers the export in the module's init function. The macro machinery is the same family covered in [Macros](/14-macros/); it just happens to target Node-API. Your `fibonacci` stays a normal, testable, standalone Rust function.

### Type conversions are automatic and bidirectional

When JS calls `fibonacci(50)`, napi-rs converts the JS `number` `50` into a Rust `u32`, runs your loop, then converts the `u64` result back to a JS value. The mapping is deliberate and worth memorizing:

- `u32` parameter ← JS `number`. If the caller passes something that is not a number, napi-rs throws a JS error *before your code runs*: you never see a bad value.
- `u64` return → JS **`bigint`**, which is why the output is `12586269025n` (note the `n` suffix). This is correct, not a quirk: a JS `number` is an IEEE-754 `f64` and silently loses integer precision above 2^53, so napi-rs maps the 64-bit integer types to `bigint` to preserve every bit. Contrast this with WebAssembly, where the boundary is even more restrictive.
- `String` parameter/return ↔ JS `string`, with a UTF-8/UTF-16 conversion at the boundary.

### `crate-type = ["cdylib"]` and `build.rs`

A normal Rust binary or `rlib` is no use to Node. `cdylib` tells the compiler to produce a **C-compatible dynamic library** — a `.dylib`/`.so`/`.dll` exposing a stable C ABI, which is exactly what Node's `process.dlopen` can load. This is the same `cdylib` you would use for any C consumer; see [FFI Basics](/20-unsafe-ffi/03-ffi-basics/). The `build.rs` calling `napi_build::setup()` configures the linker so the symbols Node-API needs resolve correctly on each platform.

### The `napi9` feature selects the Node-API version

`features = ["napi9"]` declares which **Node-API version** your addon targets (Node-API 9 is available in Node 18.17+, 20.3+, and 21+). Node-API is a *stability contract*: a binary built against Node-API 9 keeps loading on every future Node release that supports version 9 or higher, with no recompilation. This is the headline reliability win over the old `node-gyp`/V8-header approach, where a Node upgrade routinely broke prebuilt binaries.

### The `package.json` napi block

The CLI reads a small config block to know what to name the binary:

```json
{
  "name": "greet-native",
  "main": "index.js",
  "napi": {
    "binaryName": "greet-native"
  }
}
```

`binaryName` is the prefix of the generated `.node` file (`greet-native.darwin-arm64.node`, `greet-native.linux-x64-gnu.node`, and so on). The generated `index.js` loader inspects `process.platform`/`process.arch` at runtime and `require`s the matching binary: that is how one published package serves many platforms.

---

## Key Differences

| Concern | node-addon-api (C++) | napi-rs (Rust) |
| --- | --- | --- |
| Argument unwrapping | Manual `info[0].As<...>()` + type checks | Automatic from the Rust signature |
| Type errors | You throw them yourself, or crash | Thrown by the framework before your code runs |
| Memory safety | None — UB and segfaults are on you | Borrow checker applies; `unsafe` is rare |
| TypeScript types | Hand-written `.d.ts`, easily out of date | Generated from the signature, always in sync |
| Build system | `binding.gyp` + `node-gyp` (Python toolchain) | `cargo` + `@napi-rs/cli` |
| 64-bit integers | You pick `Number` vs `BigInt` manually | `i64`/`u64` map to `bigint` automatically |
| ABI stability | Tied to Node-API if you use it carefully | Node-API by construction; cross-version stable |
| Async | Manual `AsyncWorker` boilerplate | `async fn` returns a JS `Promise` |

A few conceptual points a TypeScript developer should internalize:

- **The boundary is a real cost.** Every call from JS into the addon performs argument conversion and a function-pointer call through Node-API. For tiny, frequently-called functions that overhead can dominate; the win comes from doing *meaningful work* per call. This mirrors the WASM boundary cost discussed in [WebAssembly performance](/19-wasm/09-performance/).
- **Naming is converted.** Rust's `snake_case` exports become JavaScript's `camelCase` automatically: `fetch_delayed` becomes `fetchDelayed`. The generated `.d.ts` reflects this.
- **Rust errors become thrown JS errors.** Returning `napi::Result<T>` maps `Ok(v)` to a normal return and `Err(e)` to a thrown JS `Error`. There is no checked-exception equivalent; the TypeScript signature shows only the success type, exactly like a function that may `throw`.
- **`#[napi]` is compile-time codegen, not a runtime wrapper.** Unlike a TypeScript decorator that executes when the class is defined, the macro has fully expanded before the binary exists. There is no runtime reflection cost.

---

## Common Pitfalls

### Pitfall 1: Returning a type napi-rs cannot convert

Not every Rust type can cross the boundary. If you return something without a JS representation — say `std::net::Ipv4Addr` — the macro-generated conversion fails to compile:

```rust
use napi_derive::napi;

#[napi]
pub fn bad() -> std::net::Ipv4Addr {           // does not compile (E0277)
    std::net::Ipv4Addr::new(127, 0, 0, 1)
}
```

The real `cargo build` error:

```text
error[E0277]: the trait bound `Ipv4Addr: ToNapiValue` is not satisfied
 --> src/lib.rs:4:17
  |
4 | pub fn bad() -> std::net::Ipv4Addr {
  |                 ^^^^^^^^^^^^^^^^^^ the trait `JsValue<'_>` is not implemented for `Ipv4Addr`
  |
  = help: the following other types implement trait `JsValue<'env>`:
            Array<'env>
            ArrayBuffer<'env>
            BigInt64ArraySlice<'env>
            ... and 21 others
  = note: required for `Ipv4Addr` to implement `ToNapiValue`
```

The fix is to return a type that does convert: a `String` (`addr.to_string()`), a struct annotated with `#[napi(object)]`, or a numeric/`Vec`/`HashMap` type that napi-rs supports.

### Pitfall 2: Forgetting `crate-type = ["cdylib"]`

Without the `[lib] crate-type = ["cdylib"]` entry, Cargo builds an `rlib` (a Rust-only static library). The compile may succeed, but there is no loadable dynamic library, and Node cannot `require` anything. Always set `cdylib` for an addon crate.

### Pitfall 3: Expecting a JS `number` to hold a 64-bit integer

If you return `u64` and a TypeScript caller writes `const n: number = fibonacci(50)`, the type checker will complain because the generated type is `bigint`, not `number`:

```typescript
import { fibonacci } from './index.js';
const n: number = fibonacci(50); // TS error: Type 'bigint' is not assignable to type 'number'.
```

This is napi-rs protecting you. A JS `number` cannot represent `12586269025` faithfully through arithmetic above 2^53, losing precision, not wrapping. If you genuinely only need values that fit in 2^53, return `i32`/`f64` from Rust so the JS side gets a plain `number`.

### Pitfall 4: Passing the wrong argument type from JS

You do not need to write any type-checking code; napi-rs rejects bad arguments at the boundary. Calling `greet(42)` when `greet` expects a `String` throws at runtime:

```js
greet(42); // throws
```

The real thrown message:

```text
Failed to convert JavaScript value `Number 42 ` into rust type `String`
```

The pitfall is *assuming* you still need manual `typeof` guards inside the addon (you do not), and forgetting that untyped JS callers can still trigger these throws, so wrap addon calls in `try/catch` at the JS boundary just as you would any throwing function.

### Pitfall 5: Doing long synchronous work and blocking the event loop

A `#[napi]` function with a non-`async` signature runs **on the calling JavaScript thread**. A multi-second computation there freezes the whole Node event loop, exactly as a long synchronous JS loop would. The fix is an `async fn` (next section) or napi-rs's `AsyncTask`, which run the work off-thread and resolve a `Promise`.

---

## Best Practices

- **Keep the unit of work large.** Cross the boundary as few times as possible: prefer `process(records: Vec<Record>) -> Vec<Result>` over calling a per-record function in a JS loop a million times. The conversion overhead is per-call.
- **Use `#[napi(object)]` for plain data, `#[napi]` classes for stateful handles.** A struct tagged `#[napi(object)]` becomes a plain JS object (passed by value, with a generated `interface`). A struct tagged `#[napi]` becomes a JS `class` whose instance holds a live Rust value: use it when the object owns resources or state.
- **Return `napi::Result<T>` and build errors with `Error::new(Status::…, msg)`** so failures surface as proper JS exceptions with a `.code`, instead of `unwrap()`/`panic!` (a panic across the boundary is at best an opaque error, at worst undefined behavior).
- **Make blocking work `async`.** If the Rust work is CPU-heavy or does blocking I/O, use an `async fn` (with napi's `async` feature) so it does not stall the event loop.
- **Commit the generated `index.d.ts`** (or regenerate it in CI) so TypeScript consumers always get accurate types, and verify it in code review when signatures change.
- **Pin to a Node-API version (`napi9`) deliberately** and document the minimum Node version it implies. Higher versions give access to more APIs but require newer Node.
- **For publishing, build per-platform binaries in CI** (the CLI's `--target` flag and the generated GitHub Actions workflow handle the matrix) and ship them as `optionalDependencies` so each consumer downloads only their platform's binary.

---

## Real-World Example

A realistic reason to reach for a native addon: a Node service has a hot computation that dominates a request, and you want to move it to Rust without rewriting the service. This example shows the three patterns you will actually use — a data-returning struct, fallible parsing, and asynchronous work — plus a measured speedup.

```rust
// src/lib.rs
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// A plain-data result. `#[napi(object)]` makes it a JS object with a
/// generated TypeScript `interface`.
#[napi(object)]
pub struct Stats {
    pub count: u32,
    pub total: f64,
    pub mean: f64,
}

#[napi]
pub fn summarize(values: Vec<f64>) -> Stats {
    let count = values.len() as u32;
    let total: f64 = values.iter().sum();
    let mean = if count == 0 { 0.0 } else { total / count as f64 };
    Stats { count, total, mean }
}

/// Fallible work. `Result<u16>` maps `Err` to a thrown JS error;
/// the generated `.d.ts` shows only the success type `number`.
#[napi]
pub fn parse_port(s: String) -> Result<u16> {
    s.parse::<u16>()
        .map_err(|e| Error::new(Status::InvalidArg, format!("invalid port {s:?}: {e}")))
}

/// Asynchronous work. An `async fn` returns a JS `Promise<...>` and runs
/// off the JS thread, so it never blocks the event loop. Requires the
/// `async` feature on the `napi` crate (and a Tokio dependency here).
#[napi]
pub async fn fetch_delayed(ms: u32) -> Result<String> {
    tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
    Ok(format!("done after {ms}ms"))
}

/// A stateful handle. `#[napi]` on a struct + impl produces a JS class.
#[napi]
pub struct Counter {
    value: i32,
}

#[napi]
impl Counter {
    #[napi(constructor)]
    pub fn new(start: i32) -> Self {
        Counter { value: start }
    }

    #[napi]
    pub fn increment(&mut self) -> i32 {
        self.value += 1;
        self.value
    }

    #[napi(getter)]
    pub fn value(&self) -> i32 {
        self.value
    }
}
```

The `Cargo.toml` adds the `async` feature and Tokio for the async example:

```toml
# Cargo.toml (additions)
[dependencies]
napi = { version = "3", default-features = false, features = ["napi9", "async"] }
napi-derive = "3"
tokio = { version = "1", features = ["rt", "time", "rt-multi-thread"] }
```

The CLI regenerates the full `index.d.ts` from these signatures. Note the `camelCase` conversion, the `Promise`, the `interface`, and the `class`:

```typescript
/* auto-generated by NAPI-RS */
/* eslint-disable */
export declare class Counter {
  constructor(start: number)
  increment(): number
  get value(): number
}

export declare function fetchDelayed(ms: number): Promise<string>

export declare function fibonacci(n: number): bigint

export declare function greet(name: string): string

export declare function parsePort(s: string): number

export interface Stats {
  count: number
  total: number
  mean: number
}

export declare function summarize(values: Array<number>): Stats
```

Calling all of it from Node, including the error and async paths:

```js
// test.mjs
import { summarize, parsePort, fetchDelayed, Counter } from './index.js';

console.log(summarize([10, 20, 30, 40]));
console.log('parsePort("8080") =', parsePort('8080'));

try {
  parsePort('70000'); // out of u16 range -> thrown error
} catch (err) {
  console.log('threw:', err.message);
  console.log('err.code:', err.code);
}

console.log('await fetchDelayed(50) =>', await fetchDelayed(50));

const c = new Counter(10);
console.log('initial:', c.value, '| after increment:', c.increment());
```

Real output from `node test.mjs`:

```text
{ count: 4, total: 100, mean: 25 }
parsePort("8080") = 8080
threw: invalid port "70000": number too large to fit in target type
err.code: InvalidArg
await fetchDelayed(50) => done after 50ms
initial: 10 | after increment: 11
```

And the payoff — a microbenchmark of the native `fibonacci` against an equivalent pure-JS `BigInt` version, one million calls each (`fib(90)`):

```js
// bench.mjs (excerpt) — fibJs uses BigInt to match the u64 return type.
import { fibonacci } from './index.js';
function fibJs(n) { let a = 0n, b = 1n; for (let i = 0; i < n; i++) { const t = a + b; a = b; b = t; } return a; }
// ...time 1,000,000 calls of each with process.hrtime.bigint()...
```

Real output from `node bench.mjs` on this machine (Node v22, release build, Apple Silicon):

```text
JS  : 1047.8 ms
Rust: 57.6 ms
```

About an 18x speedup, and importantly, that figure *includes* a million boundary crossings, so it is a fair, honest measurement rather than a microbenchmark that hides the FFI cost. The bigger the work per call, the closer you get to raw Rust speed.

> **Tip:** Benchmark *your* workload before committing to a native addon. If a function is cheap and called in a tight JS loop, the per-call boundary overhead can erase the win. See [Performance](/21-performance/) for how to measure rigorously with `criterion`.

---

## Further Reading

- [napi.rs — official documentation](https://napi.rs/): the canonical guide, including the full type-conversion reference and the `@napi-rs/cli` commands.
- [napi-rs GitHub repository](https://github.com/napi-rs/napi-rs) — source, examples, and the supported-types matrix.
- [Node-API documentation](https://nodejs.org/api/n-api.html) — the stable C API that napi-rs targets, and the Node-API version-to-Node-version table.
- [`napi` crate on docs.rs](https://docs.rs/napi/latest/napi/) — the Rust API surface: `Error`, `Status`, `bindgen_prelude`, async tasks, and buffers.
- Within this guide:
  - [Neon](/20-unsafe-ffi/07-neon/): the alternative Node-addon framework, and a head-to-head comparison with napi-rs.
  - [FFI Basics](/20-unsafe-ffi/03-ffi-basics/) and [Calling C from Rust](/20-unsafe-ffi/04-calling-c/): the `extern "C"` / `cdylib` foundation napi-rs is built on.
  - [What `unsafe` Really Means](/20-unsafe-ffi/00-unsafe-intro/) and [Building Safe Abstractions](/20-unsafe-ffi/08-safety-abstractions/): how napi-rs hides `unsafe` boundary code behind a safe API.
  - [When to Use Unsafe and FFI](/20-unsafe-ffi/09-when-to-use/): deciding whether a native addon is actually warranted.
  - [WebAssembly](/19-wasm/) and [WASM performance](/19-wasm/09-performance/): the in-browser / portable alternative when you do not need native Node.
  - [Macros](/14-macros/) — how attribute macros like `#[napi]` generate code at compile time.
  - [Error Handling](/08-error-handling/) — the `Result` type that maps to thrown JS errors.
  - [Async](/11-async/) and [Performance](/21-performance/) — async fundamentals and how to measure the speedup honestly.

---

## Exercises

### Exercise 1: Export a function and call it from Node

**Difficulty:** Beginner

**Objective:** Create a working napi-rs addon from scratch and call it from Node.

**Instructions:** Set up a `cdylib` crate with the `napi`, `napi-derive`, and `napi-build` dependencies, a `build.rs` calling `napi_build::setup()`, and a `package.json` with a `napi.binaryName`. Export a function `word_count(text: String) -> u32` that returns the number of whitespace-separated words. Build it with `npx napi build --platform --release` and call it from a `.mjs` file. Predict what the generated `.d.ts` signature will be before you look.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
use napi_derive::napi;

#[napi]
pub fn word_count(text: String) -> u32 {
    text.split_whitespace().count() as u32
}
```

```rust
// build.rs
fn main() {
    napi_build::setup();
}
```

```toml
# Cargo.toml (key parts)
[dependencies]
napi = { version = "3", features = ["napi9"] }
napi-derive = "3"

[build-dependencies]
napi-build = "2"

[lib]
crate-type = ["cdylib"]
```

The generated `index.d.ts` (a `u32` maps to JS `number`, not `bigint`):

```typescript
export declare function wordCount(text: string): number
```

```js
// test.mjs
import { wordCount } from './index.js';
console.log(wordCount('the quick brown fox')); // 4
```

Real output:

```text
4
```

</details>

### Exercise 2: Return a struct and surface errors

**Difficulty:** Intermediate

**Objective:** Use `#[napi(object)]` for structured output and `napi::Result` for fallible work.

**Instructions:** Export `parse_rgb(hex: String) -> Result<Rgb>` where `Rgb` is a `#[napi(object)]` struct with `r`, `g`, `b` fields (each `u8`). Accept strings like `"#ff8800"`; on a malformed input, return an `Err` built with `Error::new(Status::InvalidArg, ...)` so Node sees a thrown error with a `code`. Call it from Node for both a valid and an invalid input, catching the throw.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi(object)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[napi]
pub fn parse_rgb(hex: String) -> Result<Rgb> {
    let s = hex.strip_prefix('#').unwrap_or(&hex);
    if s.len() != 6 {
        return Err(Error::new(
            Status::InvalidArg,
            format!("expected 6 hex digits, got {:?}", hex),
        ));
    }
    let parse = |slice: &str| {
        u8::from_str_radix(slice, 16)
            .map_err(|e| Error::new(Status::InvalidArg, format!("bad hex {slice:?}: {e}")))
    };
    Ok(Rgb {
        r: parse(&s[0..2])?,
        g: parse(&s[2..4])?,
        b: parse(&s[4..6])?,
    })
}
```

The generated `index.d.ts`:

```typescript
export interface Rgb {
  r: number
  g: number
  b: number
}
export declare function parseRgb(hex: string): Rgb
```

```js
// test.mjs
import { parseRgb } from './index.js';

console.log(parseRgb('#ff8800')); // { r: 255, g: 136, b: 0 }

try {
  parseRgb('nope');
} catch (err) {
  console.log('code:', err.code, '| message:', err.message);
}
```

Real output:

```text
{ r: 255, g: 136, b: 0 }
code: InvalidArg | message: expected 6 hex digits, got "nope"
```

> **Note:** napi-rs validates that each `u8` field is in range when converting back to JS, so you cannot accidentally return a value outside `0..=255`.

</details>

### Exercise 3: Async work that returns a Promise

**Difficulty:** Advanced

**Objective:** Move blocking work off the event loop with an `async fn`, returning a JS `Promise`.

**Instructions:** Enable the `async` feature on the `napi` crate and add `tokio`. Export `async fn hash_rounds(input: String, rounds: u32) -> Result<String>` that, for `rounds` iterations, folds the input into a running `u64` hash (any simple mixing is fine — e.g. multiply-add over the bytes) with a small `tokio::time::sleep` per round to simulate work, then returns the final hash as a hex string. Confirm from Node that the returned value is a `Promise` you can `await`, and that the generated `.d.ts` says `Promise<string>`.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi]
pub async fn hash_rounds(input: String, rounds: u32) -> Result<String> {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a offset basis
    for _ in 0..rounds {
        for &byte in input.as_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3); // FNV prime
        }
        // Yield to the runtime; in real code this would be real I/O or CPU work.
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    Ok(format!("{hash:016x}"))
}
```

```toml
# Cargo.toml (key parts)
[dependencies]
napi = { version = "3", default-features = false, features = ["napi9", "async"] }
napi-derive = "3"
tokio = { version = "1", features = ["rt", "time", "rt-multi-thread"] }
```

The generated `index.d.ts`:

```typescript
export declare function hashRounds(input: string, rounds: number): Promise<string>
```

```js
// test.mjs
import { hashRounds } from './index.js';

const promise = hashRounds('payload', 5);
console.log('is a Promise:', promise instanceof Promise); // true
console.log('hash:', await promise);                      // 16-hex-digit string
```

Real output (the exact hash depends on your mixing function; with the FNV-1a code above):

```text
is a Promise: true
hash: 2227eb666952eee5
```

The key win: while the addon is computing, the Node event loop keeps running other tasks. A non-`async` version of the same loop would freeze the process for the duration. See [Async](/11-async/) for the underlying model, and remember that Rust futures are *lazy* (they do nothing until polled by the runtime), the opposite of an eager JavaScript `Promise`.

</details>
