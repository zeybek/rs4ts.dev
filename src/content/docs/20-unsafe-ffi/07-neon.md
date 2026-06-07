---
title: "Node.js Native Addons with Neon"
description: "Build a Node.js native addon in Rust with Neon: high-level #[neon::export] macros plus a hands-on FunctionContext API, and how it compares to napi-rs."
---

**Neon** is the other major way to write a Node.js native addon in Rust. Where [napi-rs](/20-unsafe-ffi/06-napi/) leans on macros to hide almost everything, Neon gives you a hands-on, explicitly-typed door into Node's engine, and, since version 1.0, a high-level macro layer that closes most of the gap. This page shows the current Neon API, builds and runs a real addon, and gives you a clear-eyed comparison so you can pick the right tool.

---

## Quick Overview

A **native addon** is a compiled `.node` file that Node loads with `require()` just like any other module, except the code inside is Rust (or C/C++), not JavaScript. Neon is a Rust crate plus a small npm toolchain that compiles your crate into such an addon and marshals values across the JavaScript/Rust boundary.

For a TypeScript/JavaScript developer the value proposition is the same as wasm or any FFI: push hot, CPU-bound work (parsing, hashing, image processing, number crunching) into Rust, keep the rest of the app in Node, and call across the line as if it were a normal module. Neon and napi-rs both do this; the difference is in *style* and *ergonomics*, which is what this page is really about.

> **Note:** Native addons are different from WebAssembly. An addon is a platform-specific binary that runs in-process with full OS access (files, threads, sockets); wasm is a sandboxed, portable module. If you want portability and the browser, see [Section 19: WebAssembly](/19-wasm/). If you want maximum throughput and native OS access inside a Node server, an addon is the tool.

---

## TypeScript/JavaScript Example

Here is the kind of workload people reach for a native addon to accelerate: a synchronous Fibonacci-style hot loop, a string transform, and an async task. In pure TypeScript on Node v22 it looks like this.

```typescript
// math.ts — the pure-TypeScript version we want to speed up
export function fibonacci(n: number): number {
  let [a, b] = [0n, 1n];
  for (let i = 0; i < n; i++) {
    [a, b] = [b, a + b];
  }
  return Number(a);
}

export function shout(text: string): string {
  return text.toUpperCase();
}

export async function slowDouble(x: number): Promise<number> {
  return x * 2;
}

// Usage:
console.log(fibonacci(10)); // 55
console.log(shout("hello")); // HELLO
slowDouble(21).then((r) => console.log(r)); // 42
```

> **Warning:** Notice the `BigInt` (`0n`, `1n`) in `fibonacci`. A JavaScript `number` is always an IEEE-754 `f64`, so it cannot hold large integers exactly: `fibonacci(80)` computed with plain `number` arithmetic silently loses precision. This is *not* wrapping like a fixed-width integer; it is rounding. We will hit the same `f64`-at-the-boundary reality in the Rust version, because that is what JavaScript hands across the FFI line.

The goal: replace this module with a Rust-backed `.node` addon that exposes the **same three functions** with the same call signatures, so the rest of the app does not change.

---

## Rust Equivalent

Neon 1.x offers two coexisting styles, and idiomatic code uses both. The high-level `#[neon::export]` attribute auto-converts plain Rust types to and from JavaScript; the low-level `FunctionContext` style hands you raw JavaScript handles to parse yourself. The crate is on the latest stable Rust toolchain (Rust 1.96.0, 2024 edition; `cargo new` selects it automatically).

Add Neon with the features this example needs:

```bash
cargo add neon --features napi-6,futures,tokio,tokio-rt-multi-thread
cargo add tokio --features rt-multi-thread
```

```toml
# Cargo.toml
[package]
name = "fast-math"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"] # produce a dynamic library Node can load

[dependencies]
neon = { version = "1.1.1", features = ["napi-6", "futures", "tokio", "tokio-rt-multi-thread"] }
tokio = { version = "1", features = ["rt-multi-thread"] }
```

```rust
// src/lib.rs
use neon::prelude::*;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

// --- High-level style: #[neon::export] with plain Rust types. ---
// Neon serializes JS <-> Rust automatically (anything that implements
// TryFromJs as an argument, TryIntoJs as a return value).
#[neon::export]
fn fibonacci(n: f64) -> f64 {
    let n = n as u64;
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        (a, b) = (b, a + b);
    }
    a as f64
}

// snake_case is exported as camelCase by default; `name` overrides it.
#[neon::export(name = "shout")]
fn shout(text: String) -> String {
    text.to_uppercase()
}

// An `async fn` automatically becomes a JavaScript Promise.
#[neon::export]
async fn slow_double(x: f64) -> f64 {
    x * 2.0
}

// --- Low-level style: parse handles out of the FunctionContext yourself. ---
fn add(mut cx: FunctionContext) -> JsResult<JsNumber> {
    let a = cx.argument::<JsNumber>(0)?.value(&mut cx);
    let b = cx.argument::<JsNumber>(1)?.value(&mut cx);
    Ok(cx.number(a + b))
}

// Async exports need a global executor (here a Tokio runtime) installed once.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

// #[neon::main] is the module's entry point, run when Node loads the addon.
#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    let runtime = RUNTIME.get_or_init(|| Runtime::new().expect("build tokio runtime"));
    let _ = neon::set_global_executor(&mut cx, runtime);

    // Wire up the low-level function by hand...
    cx.export_function("add", add)?;
    // ...and register everything marked with #[neon::export] in one call.
    neon::registered().export(&mut cx)?;
    Ok(())
}
```

Build it, copy the produced dynamic library to `index.node`, and call it from Node exactly like the TypeScript module:

```bash
cargo build --release
# macOS dylib -> .node; on Linux it is lib<name>.so, on Windows <name>.dll
cp target/release/libfast_math.dylib index.node
```

```javascript
// test.cjs
const addon = require("./index.node");
console.log("fibonacci(10) =", addon.fibonacci(10));
console.log("shout('hello') =", addon.shout("hello"));
console.log("add(2, 3) =", addon.add(2, 3));
addon.slowDouble(21).then((r) => console.log("await slowDouble(21) =", r));
```

Real output from `node test.cjs` on Node v22.18.0:

```text
fibonacci(10) = 55
shout('hello') = HELLO
add(2, 3) = 5
await slowDouble(21) = 42
```

The addon is a drop-in for the TypeScript module: same names, same arguments, same results, including the Promise from the `async` function.

> **Tip:** In a real project you do not hand-copy the dylib. The official scaffold, `npm init neon@latest my-addon`, generates a crate plus a `package.json` whose `build` script invokes Neon's tooling to produce `index.node` and place it where `require("./")` finds it. We do the copy by hand here only to keep the example self-contained and reproducible.

---

## Detailed Explanation

**`crate-type = ["cdylib"]`.** A native addon is a C-compatible dynamic library. This line tells Cargo to emit a `.dylib`/`.so`/`.dll` instead of a Rust `.rlib`. It is identical to what you would set for any C FFI library (see [FFI Basics](/20-unsafe-ffi/03-ffi-basics/)). An addon is "just" a dynamic library that happens to register itself with Node's Node-API runtime.

**`#[neon::main]`.** This attribute marks the function Node calls once, at load time. `ModuleContext` is your handle to the freshly-created `module.exports` object. Whatever you attach here becomes the addon's public surface. There is no implicit registration: if a name is not exported in `main`, JavaScript cannot see it.

**`neon::registered().export(&mut cx)?`.** This is the bridge between the two styles. Each `#[neon::export]` item registers itself in a global table at link time; this one call copies them all onto `module.exports`. The catch that bites everyone: if you write your *own* `#[neon::main]`, you **must** make this call, because your hand-written `main` replaces the one Neon would otherwise generate for you. Forget it and your `#[neon::export]` functions silently vanish (see Common Pitfalls). If you do not need a custom `main`, omit it entirely and Neon generates one that registers everything automatically.

**`#[neon::export] fn fibonacci(n: f64) -> f64`.** This is Neon's high-level mode, and it reads like ordinary Rust. The macro generates the glue that converts the incoming JavaScript value into an `f64` (via the `TryFromJs` trait) and converts the `f64` result back into a JavaScript number (via `TryIntoJs`). You never touch a `FunctionContext`. Note the type: it is `f64`, not `u64` or `i64`, because a plain JavaScript `number` *is* an `f64`. The `as u64` inside is where we accept JavaScript's numeric model and convert deliberately, the same precision caveat as the TypeScript version.

**`#[neon::export(name = "shout")]`.** By default Neon renames `snake_case` Rust to `camelCase` JavaScript (`slow_double` becomes `slowDouble`, which is why the JS call site uses `slowDouble`). The `name` argument pins an explicit JavaScript name when you want one. Here `shout` happens to already be lowercase, shown to illustrate the override.

**`async fn slow_double`.** Marking an exported function `async` makes Neon return a JavaScript `Promise` and drive the future on a background executor, so the Node event loop is never blocked. With the `tokio` feature enabled and no custom `#[neon::main]`, Neon auto-registers a multithreaded Tokio runtime for you, so the Promise resolves with no extra work. The moment you write your *own* `#[neon::main]`, that automatic runtime is suppressed and you must install one yourself: `neon::set_global_executor(&mut cx, runtime)` registers a Tokio runtime (Neon ships a blanket `Runtime` impl behind the `tokio` feature). If a custom `main` omits it, calling the async export throws `Error: must initialize with neon::set_global_executor`: a real runtime error, not a compile error, because the missing piece is discovered only when the future needs an executor.

**`fn add(mut cx: FunctionContext) -> JsResult<JsNumber>`.** This is the low-level mode and Neon's historical heart. `cx.argument::<JsNumber>(0)?` pulls argument 0 and checks it is a number; the `?` propagates a thrown JavaScript exception if it is not. `.value(&mut cx)` extracts the Rust `f64`. `cx.number(...)` allocates a JavaScript number to return. `JsResult<JsNumber>` is `Result<Handle<JsNumber>, Throw>`: the function can *throw into JavaScript*, which is how Rust surfaces errors to Node. Every value is a `Handle<'_, JsT>`: a GC-managed reference into V8's heap, valid only while the context lives. Compared to the one-liner `#[napi]` equivalent, you can see the manual marshaling Neon's low-level mode asks of you. That explicitness is the trade-off and, to some, the appeal.

---

## Key Differences

Both Neon and napi-rs produce a Node-API addon and both work on current Node (v22). The differences are about *how much the library does for you* and *which engine APIs you can reach*.

| Dimension | Neon (1.1.x) | napi-rs (3.x) |
| --- | --- | --- |
| High-level export | `#[neon::export]` (auto type conversion) | `#[napi]` (auto type conversion) |
| Low-level access | First-class `FunctionContext` / `Handle` API to V8 + Node-API | Lower-level `Env`/`JsValue` API exists but less emphasized |
| Mental model | "Here is the engine; talk to it" (with macros on top) | "Write Rust; I'll hide the engine" |
| Generating `.d.ts` | Not built in | **Yes**: `napi build` emits TypeScript types |
| Async | `async fn` -> Promise; you install the executor | `async fn` -> Promise; Tokio integrated via a feature |
| Threads -> JS callbacks | `Channel` (send work back to the JS thread) | `ThreadsafeFunction` |
| Cross-compilation / CI | npm toolchain; supported | Strong prebuilt-binary + multi-platform CI tooling |
| Backend | Node-API (with some historical V8 surface) | Node-API throughout |
| Scaffold | `npm init neon@latest` | `npm create napi@latest` (or `@napi-rs/cli`) |

The single biggest practical difference for a TypeScript team: **napi-rs generates a `.d.ts`** for your addon, so your editor and `tsc` understand the native module's signatures automatically. With Neon you write the type declarations yourself. If end-to-end type safety from Rust to TypeScript is a priority, that tilts toward napi-rs, which is exactly what [Node.js Native Addons with napi-rs](/20-unsafe-ffi/06-napi/) covers in depth.

Where Neon shines is its explicit `FunctionContext` API. When you need to do something unusual with the engine — inspect an arbitrary JavaScript value's type at runtime, build a complex object graph by hand, or interleave native and JS calls precisely — Neon's "here is the context, talk to the engine" model is direct and discoverable. napi-rs can do these things too, but its design optimizes for the common case of "convert types and get out of the way."

> **Note:** This is not a wasm-style portability decision. Both produce platform-specific binaries that must be compiled (or prebuilt) per OS/architecture and per Node-API version. Neither runs in a browser. The choice is purely Neon-vs-napi ergonomics, not a capability gap.

---

## Common Pitfalls

### Pitfall 1: Custom `#[neon::main]` silently drops `#[neon::export]` functions

This is the most common Neon surprise. If you hand-write `#[neon::main]`, your function *replaces* the auto-generated one, and the `#[neon::export]` registrations are no longer applied unless you add `neon::registered().export(&mut cx)?` yourself.

```rust
// Wrong: a custom main that forgets to export the registered items.
use neon::prelude::*;

#[neon::export]
fn fibonacci(n: f64) -> f64 { n } // never reaches JavaScript!

fn add(mut cx: FunctionContext) -> JsResult<JsNumber> {
    let a = cx.argument::<JsNumber>(0)?.value(&mut cx);
    let b = cx.argument::<JsNumber>(1)?.value(&mut cx);
    Ok(cx.number(a + b))
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("add", add)?;
    // BUG: missing `neon::registered().export(&mut cx)?;`
    Ok(())
}
```

This compiles fine. The breakage shows up only at runtime in Node:

```text
keys: [ 'add' ]
TypeError: addon.fibonacci is not a function
```

`Object.keys(addon)` shows only `add`; `fibonacci` was registered in the global table but never copied onto `module.exports`. The fix is one line — add `neon::registered().export(&mut cx)?;` to `main` — or delete the custom `main` and let Neon generate it.

### Pitfall 2: Awaiting an async export without a global executor

An `#[neon::export] async fn` always compiles, but whether it *runs* depends on a runtime being installed. The good news: with the `tokio` feature enabled and **no** custom `#[neon::main]`, Neon auto-registers a multithreaded Tokio runtime for you, so the Promise simply resolves. The trap appears when you write your *own* `#[neon::main]` (which suppresses that automatic runtime) and forget to install one. Then the first call into the async export throws:

```text
Error: must initialize with neon::set_global_executor
```

So the rule is: if you hand-write `main`, install a runtime once with `neon::set_global_executor(&mut cx, runtime)` inside it (as in the full example above), or you have traded away the auto-runtime for nothing. This is the inverse of a common JavaScript intuition: a Rust future is **lazy** and needs a runtime to drive it, the opposite of an eager JavaScript Promise that starts running the moment it is created.

### Pitfall 3: Wrong argument type or arity in the low-level style

In `FunctionContext` code, `cx.argument::<T>(i)?` enforces both the count and the type, and the `?` turns a mismatch into a thrown JavaScript exception. Calling `add(2)` (missing the second argument) or `shout(123)` (a number where a string is expected) throws, caught here for display:

```text
ERROR: not enough arguments
ERROR: failed to downcast any to string
```

These are clean, catchable JavaScript `Error`s, not crashes. That is the whole point of the boundary: bad input from JavaScript becomes a normal exception, never undefined behavior in Rust.

### Pitfall 4: Expecting integer precision from a `number`

`fibonacci` takes and returns `f64` because JavaScript hands you an `f64`. Large results lose precision exactly as they would in pure JavaScript. If you need exact 64-bit integers across the boundary, accept and return JavaScript `BigInt` instead (Neon exposes `JsBigInt`), or pass values as strings. Do not assume `f64` will silently behave like a Rust `u64`; it will round, not wrap.

### Pitfall 5: Treating the addon as portable

The compiled `index.node` is tied to the OS, CPU architecture, and Node-API version it was built against. Copying a macOS `.dylib`-derived `index.node` to a Linux server will fail to load. Build per target (CI matrices and prebuilt binaries handle this), or reach for wasm if you genuinely need one portable artifact; see [Section 19](/19-wasm/).

---

## Best Practices

- **Skip the custom `main` unless you need it.** If every export uses `#[neon::export]`, omit `#[neon::main]` and let Neon generate the registration. You only write `main` when you must run setup code (like installing an async executor) or mix in low-level `cx.export_function` calls — and then you must remember `neon::registered().export(&mut cx)?`.
- **Prefer `#[neon::export]` for the common case.** Reach for the low-level `FunctionContext` API only where you genuinely need to manipulate engine handles directly. The high-level macro is shorter, harder to get wrong, and reads like normal Rust.
- **Keep the Rust hot and the boundary cold.** Crossing the JavaScript/Rust line has a cost (argument conversion, GC handle setup). Pass a batch of work across once and do the loop in Rust, rather than calling a tiny Rust function millions of times from a JavaScript loop.
- **Use `async fn` + an executor for blocking or long work** so you never stall Node's event loop. For pushing results back to JavaScript from a spawned thread, use Neon's `Channel`.
- **Wrap any genuinely `unsafe` core behind safe Rust before it reaches the addon surface.** The export functions should take and return safe types; keep raw pointers and FFI calls inside audited helpers, as described in [Building Safe Abstractions](/20-unsafe-ffi/08-safety-abstractions/).
- **Build a release binary for benchmarks and production** (`--release`). A debug addon can be an order of magnitude slower and will mislead any comparison against your TypeScript baseline.
- **Decide Neon vs napi-rs on type-generation and team taste.** If you want auto-generated `.d.ts` and the most "just write Rust" experience, [napi-rs](/20-unsafe-ffi/06-napi/) is likely the better fit. If you value direct, explicit access to the engine, Neon is excellent. And before building either, sanity-check whether you need a native addon at all — see [When to Use `unsafe`/FFI](/20-unsafe-ffi/09-when-to-use/).

---

## Real-World Example

A common production use is offloading a CPU-bound transform that would block Node's single thread. Here is a small but realistic addon: a synchronous word-frequency counter (the kind of text crunching that is slow in JavaScript) plus an async SHA-256 hash that runs off the event loop and resolves a Promise. Both are compile-verified and run on Node v22.

```toml
# Cargo.toml
[package]
name = "text-tools"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
neon = { version = "1.1.1", features = ["napi-6", "futures", "tokio", "tokio-rt-multi-thread", "serde"] }
tokio = { version = "1", features = ["rt-multi-thread"] }
sha2 = "0.11"
```

```rust
// src/lib.rs
use neon::prelude::*;
use neon::types::extract::Json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// Count word frequencies and return the single most common word and its count.
/// The `Json` wrapper (the `serde` feature) lets us return any serde-serializable
/// value; here a tuple, which arrives in JavaScript as a two-element array.
#[neon::export]
fn top_word(text: String) -> Json<Option<(String, u64)>> {
    let mut counts: HashMap<String, u64> = HashMap::new();
    for word in text.split_whitespace() {
        let key = word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        if !key.is_empty() {
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    let best = counts.into_iter().max_by_key(|(_, count)| *count);
    Json(best)
}

/// Hash a string with SHA-256, off the event loop, resolving a JS Promise.
#[neon::export]
async fn sha256_hex(input: String) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    let runtime = RUNTIME.get_or_init(|| Runtime::new().expect("build tokio runtime"));
    let _ = neon::set_global_executor(&mut cx, runtime);
    neon::registered().export(&mut cx)?;
    Ok(())
}
```

```javascript
// test.cjs
const addon = require("./index.node");

const [word, count] = addon.topWord("the cat sat on the mat the cat ran");
console.log(`topWord -> "${word}" x${count}`);

addon.sha256Hex("hello").then((hex) => {
  console.log("sha256Hex('hello') =", hex);
});
```

Build and run:

```bash
cargo build --release
cp target/release/libtext_tools.dylib index.node
node test.cjs
```

Real output on Node v22.18.0:

```text
topWord -> "the" x3
sha256Hex('hello') = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
```

`top_word` returns `Json<Option<(String, u64)>>`: the `Json` wrapper serializes through `serde`, so `None` arrives as JavaScript `null` and `Some(("the", 3))` arrives as the two-element array `[ 'the', 3 ]`, which the JavaScript side destructures directly. `sha256_hex` is `async`, so the hashing happens on the Tokio executor and JavaScript receives a Promise; the event loop stays free for other work. The only `FunctionContext` glue is none at all; the high-level macro handled `String`, the `Json`-wrapped tuple, and the Promise. This is Neon at its most ergonomic, and it is genuinely close to the napi-rs experience for the common case. The hash value matches the well-known SHA-256 of the string `hello`, confirming the addon really ran the Rust code.

> **Note:** Neon maps `Vec<T>`/tuples of *numbers* to JavaScript **typed arrays** (`Float64Array`, etc.), not ordinary arrays, and a bare tuple like `(String, u64)` does not implement the return-conversion trait at all. When you want plain JavaScript arrays, objects, or `null`, wrap the value in `Json<T>` (the `serde` feature) as shown here, or build a `JsArray`/`JsObject` by hand with the low-level API. This is a place where Neon is more explicit than napi-rs, which maps `Vec<String>` to a JavaScript array out of the box.

---

## Further Reading

### Official documentation

- [Neon project site](https://neon-rs.dev/): guides, the `npm init neon` scaffold, and the build toolchain
- [`neon` on docs.rs](https://docs.rs/neon/latest/neon/) — the crate API, including `FunctionContext`, `ModuleContext`, and `Handle`
- [`#[neon::export]` attribute](https://docs.rs/neon/latest/neon/attr.export.html): the high-level export macro and its options
- [`neon::set_global_executor`](https://docs.rs/neon/latest/neon/fn.set_global_executor.html) — installing the async runtime
- [Node-API documentation](https://nodejs.org/api/n-api.html): the stable C ABI both Neon and napi-rs build on

### Related sections in this guide

- Sibling: [Node.js Native Addons with napi-rs](/20-unsafe-ffi/06-napi/) — the macro-first alternative, with auto-generated `.d.ts`
- [FFI Basics](/20-unsafe-ffi/03-ffi-basics/) and [Calling C from Rust](/20-unsafe-ffi/04-calling-c/): the `cdylib`/C-ABI foundation an addon sits on
- [Building Safe Abstractions](/20-unsafe-ffi/08-safety-abstractions/) — keep `unsafe` cores behind safe export functions
- [When to Use `unsafe`/FFI](/20-unsafe-ffi/09-when-to-use/): decide whether you need a native addon at all
- Section home: [Section 20: Unsafe & FFI](/20-unsafe-ffi/)
- [Section 19: WebAssembly](/19-wasm/) — the portable, sandboxed alternative to native addons; see [JS Interop](/19-wasm/03-js-interop/)
- Foundations: [Section 00: Introduction](/00-introduction/), [Why Rust?](/01-getting-started/00-why-rust/), [Basics](/02-basics/)
- [Section 11: Async](/11-async/) — why Rust futures are lazy and need a runtime (the basis for async exports)
- Going further: [Section 21: Performance](/21-performance/): measuring whether the addon actually beat your TypeScript baseline

---

## Exercises

### Exercise 1: Add a function two ways

**Difficulty:** Easy

**Objective:** Internalize the difference between Neon's high-level and low-level export styles by writing the same function in each.

**Instructions:** Starting from the full `Rust Equivalent` example, add a function `multiply(a, b)` that returns `a * b`. Write it **once** with `#[neon::export]` (plain `f64` arguments) and **once** with the low-level `FunctionContext` API and `cx.export_function`. Build, load in Node, and confirm both produce the same result. Which version is shorter? Which gives you direct access to the engine?

<details>
<summary>Solution</summary>

```rust
// src/lib.rs (additions)
use neon::prelude::*;

// High-level: plain types, no context.
#[neon::export]
fn multiply(a: f64, b: f64) -> f64 {
    a * b
}

// Low-level: parse handles out of the context yourself.
fn multiply_manual(mut cx: FunctionContext) -> JsResult<JsNumber> {
    let a = cx.argument::<JsNumber>(0)?.value(&mut cx);
    let b = cx.argument::<JsNumber>(1)?.value(&mut cx);
    Ok(cx.number(a * b))
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("multiplyManual", multiply_manual)?;
    neon::registered().export(&mut cx)?; // registers `multiply`
    Ok(())
}
```

```javascript
// test.cjs
const addon = require("./index.node");
console.log(addon.multiply(6, 7)); // 42
console.log(addon.multiplyManual(6, 7)); // 42
```

Real output:

```text
42
42
```

The `#[neon::export]` version is dramatically shorter: three lines of ordinary Rust with no engine types. The manual version is longer but hands you the `FunctionContext`, which is what you would reach for when you need to do something the macro cannot express. For a plain arithmetic function, the high-level style wins easily.

</details>

### Exercise 2: Reproduce the silent-drop bug, then fix it

**Difficulty:** Medium

**Objective:** Cement why a custom `#[neon::main]` must call `neon::registered().export(...)`.

**Instructions:** Write an addon with one `#[neon::export] fn ping() -> String` returning `"pong"` and a custom `#[neon::main]` that exports a *different* low-level function `version()` but **omits** `neon::registered().export(&mut cx)?`. Load it in Node and print `Object.keys(addon)` and the result of calling `addon.ping()`. Explain what you see, then fix it so both functions are visible.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs — the buggy version
use neon::prelude::*;

#[neon::export]
fn ping() -> String {
    "pong".to_string()
}

fn version(mut cx: FunctionContext) -> JsResult<JsString> {
    Ok(cx.string("1.0.0"))
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("version", version)?;
    // BUG: no `neon::registered().export(&mut cx)?;`
    Ok(())
}
```

```javascript
const addon = require("./index.node");
console.log("keys:", Object.keys(addon));
try {
  console.log(addon.ping());
} catch (e) {
  console.log("ERROR:", e.message);
}
```

Buggy run (real output):

```text
keys: [ 'version' ]
ERROR: addon.ping is not a function
```

`ping` was registered in Neon's global table but never copied onto `module.exports`, because the hand-written `main` replaced the generated one. The fix is a single line:

```rust
#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("version", version)?;
    neon::registered().export(&mut cx)?; // <-- the fix
    Ok(())
}
```

Fixed run (real output):

```text
keys: [ 'version', 'ping' ]
pong
```

</details>

### Exercise 3: An async export with a real runtime

**Difficulty:** Hard

**Objective:** Build an `async` Neon export end to end, including installing the executor, and handle the boundary correctly.

**Instructions:** Add `cargo add sha2`. Write an `#[neon::export] async fn hash_lines(text: String) -> Json<Vec<String>>` that splits `text` on newlines and returns the SHA-256 hex digest of each non-empty line, in order. (Return it wrapped in `Json` so a `Vec<String>` becomes an ordinary JavaScript array rather than a typed array — enable the `serde` feature.) Install a Tokio runtime in `#[neon::main]` so the Promise resolves. From Node, `await` the function on a two-line string and print the array. Confirm it works, then describe what would happen if you forgot `set_global_executor`.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml dependencies
[dependencies]
neon = { version = "1.1.1", features = ["napi-6", "futures", "tokio", "tokio-rt-multi-thread", "serde"] }
tokio = { version = "1", features = ["rt-multi-thread"] }
sha2 = "0.11"
```

```rust
// src/lib.rs
use neon::prelude::*;
use neon::types::extract::Json;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use tokio::runtime::Runtime;

#[neon::export]
async fn hash_lines(text: String) -> Json<Vec<String>> {
    let hashes = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut hasher = Sha256::new();
            hasher.update(line.as_bytes());
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        })
        .collect();
    Json(hashes)
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    let runtime = RUNTIME.get_or_init(|| Runtime::new().expect("build tokio runtime"));
    let _ = neon::set_global_executor(&mut cx, runtime);
    neon::registered().export(&mut cx)?;
    Ok(())
}
```

```javascript
// test.cjs
const addon = require("./index.node");
addon.hashLines("hello\nworld").then((hashes) => {
  console.log(hashes);
});
```

Real output:

```text
[
  '2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824',
  '486ea46224d1bb4fb680f34f7c9ad96a8f24ec88be73ea8e5a6c65260e9cb8a7'
]
```

(`Vec<String>` becomes a JavaScript array; the two hashes are the SHA-256 of `hello` and `world`.) If you forgot `neon::set_global_executor`, the code would still compile and the addon would still load — but the first time JavaScript awaited the Promise it would throw `Error: must initialize with neon::set_global_executor`. The lesson: async Neon exports need a runtime installed at module init, because a Rust future does nothing until something drives it.

</details>
