---
title: "What Is WebAssembly and Why Compile Rust to It?"
description: "WebAssembly runs compiled Rust in the browser at near-native speed beside JavaScript. Why Rust ships no GC, how the JS boundary works, and when Wasm actually wins."
---

**WebAssembly** (**Wasm**) is a portable, low-level bytecode format that runs in every modern browser at near-native speed, right alongside your JavaScript. Rust is one of the best languages for producing it: small binaries, no garbage collector to ship, and first-class tooling. This page explains *what* Wasm is, *why* you might reach for Rust instead of plain JavaScript, and *what runs where* once you do.

---

## Quick Overview

JavaScript is the only programming language browsers natively understand, until WebAssembly. Wasm is a compact binary instruction format that the same JavaScript engine (V8 in Chrome/Node, SpiderMonkey in Firefox, JavaScriptCore in Safari) can load, validate, and execute. You don't *replace* JavaScript with it; you *add* a fast, sandboxed module that JavaScript calls into for the heavy lifting.

For a TypeScript/JavaScript developer the mental model is: **Wasm is to the browser what a native addon (`.node` C++ binding) is to Node.js** — a compiled, high-performance module loaded from JavaScript — except Wasm is portable, sandboxed, and runs in the browser too. Rust compiles to Wasm exceptionally well because it has no runtime and no garbage collector to bundle, so the modules stay small and predictable.

---

## TypeScript/JavaScript Example

Here is a realistic piece of front-end work: converting an image to grayscale on a `<canvas>`. In JavaScript you pull the pixel buffer out of the canvas and loop over it by hand.

```typescript
// grayscale.ts — runs in the browser's main thread
function grayscale(imageData: ImageData): void {
  const pixels = imageData.data; // Uint8ClampedArray, RGBA RGBA RGBA ...
  for (let i = 0; i < pixels.length; i += 4) {
    // Rec. 601 luminance weights
    const lum = 0.299 * pixels[i] + 0.587 * pixels[i + 1] + 0.114 * pixels[i + 2];
    pixels[i] = lum; // R
    pixels[i + 1] = lum; // G
    pixels[i + 2] = lum; // B
    // pixels[i + 3] (alpha) untouched
  }
}

const canvas = document.querySelector("canvas")!;
const ctx = canvas.getContext("2d")!;
const frame = ctx.getImageData(0, 0, canvas.width, canvas.height);
grayscale(frame); // For a 4K image this is ~33 million array accesses
ctx.putImageData(frame, 0, 0);
```

This *works*, and for a small image it is perfectly fast. But there are characteristics worth noticing:

- The loop runs on the **main thread**, so a 4K image can freeze the UI for tens of milliseconds.
- Every `pixels[i]` is a bounds-checked, dynamically-typed array access that V8 must keep de-optimizing and re-optimizing if the shapes change.
- The numeric work happens in IEEE-754 `f64` (JavaScript's only number type) even though the data is bytes.

For one filter this is fine. For a real-time video pipeline, a physics engine, a Markdown-to-HTML parser running on every keystroke, or a chess engine searching millions of positions, this is exactly the kind of CPU-bound, tight-loop work where a compiled language pulls ahead.

---

## Rust Equivalent

The same algorithm in Rust, exported to JavaScript through the **`wasm-bindgen`** crate. This compiles to a `.wasm` module that JavaScript imports and calls like any other function.

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

/// Convert an RGBA image buffer to grayscale, in place.
/// Exported to JavaScript as `grayscale(pixels: Uint8Array)`.
#[wasm_bindgen]
pub fn grayscale(pixels: &mut [u8]) {
    // `chunks_exact_mut(4)` walks the buffer one RGBA pixel at a time.
    for px in pixels.chunks_exact_mut(4) {
        // Rec. 601 luminance weights, the same formula as the JavaScript version.
        // Note: casting f32 to u8 truncates toward zero, whereas writing a float into
        // a Uint8ClampedArray rounds to nearest; for in-range luminance the two differ
        // by at most one level per channel.
        let lum = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) as u8;
        px[0] = lum; // R
        px[1] = lum; // G
        px[2] = lum; // B
        // px[3] (alpha) is left untouched
    }
}
```

The crate is configured as a dynamic library so the toolchain can emit a Wasm module:

```toml
# Cargo.toml
[package]
name = "imageproc"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
```

Building it produces a real `.wasm` file. Compiling the snippet above to the browser's Wasm target yields:

```text
$ cargo build --target wasm32-unknown-unknown --release
   Compiling imageproc v0.1.0 (/tmp/wasm_probe/imageproc)
    Finished `release` profile [optimized] target(s) in 7.63s

$ ls -la target/wasm32-unknown-unknown/release/imageproc.wasm
-rwxr-xr-x  1 you  staff  38975  imageproc.wasm
```

> **Note:** You normally don't call `cargo build --target wasm32-unknown-unknown` directly; the `wasm-pack` tool wraps this, runs `wasm-bindgen` to generate the JavaScript glue, and shrinks the output. That workflow is covered in [Setting Up wasm-pack](/19-wasm/01-wasm-pack/) and [Your First Rust → WebAssembly Module](/19-wasm/02-first-wasm/). The raw command above is shown only to prove that this Rust really does become a `.wasm` binary.

The roughly 38 KB you see is the raw `cargo` output before `wasm-bindgen` post-processing and `wasm-opt` strip it; once those run, a function this small shrinks to a few kilobytes. Sizing and shrinking are covered in [WebAssembly Performance](/19-wasm/09-performance/).

---

## Detailed Explanation

### What WebAssembly actually is

WebAssembly is a **virtual instruction set architecture**: a CPU-like bytecode that no real chip executes directly. Instead, the browser's engine validates a `.wasm` module (a fast, single linear pass) and then compiles it to the machine code of whatever CPU you're on. The result is a sandboxed module with:

- **A linear memory**: one contiguous, growable `ArrayBuffer` of bytes. This is the *only* memory a Wasm module has. There are no objects, no strings, no garbage collector inside, just numbers and bytes. (A separate "reference types" / GC proposal exists, but the core model is this flat byte array.)
- **A small set of numeric types**: `i32`, `i64`, `f32`, `f64`, plus 128-bit SIMD vectors. That's it. Everything richer — a Rust `String`, a `struct`, a `Vec` — is *encoded into* that linear memory as bytes.
- **Imports and exports**: functions the module exposes to JavaScript (`exports`) and functions it asks JavaScript to provide (`imports`). All cross-boundary calls go through these.

Because the type system is so narrow, you cannot just hand a JavaScript object to a Wasm function. Something has to translate. That "something" is the glue code that `wasm-bindgen` generates for you: it copies your `Uint8Array` into the module's linear memory, calls the export with a pointer and length, and copies results back. The mechanics of that translation are the subject of [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/).

### Why Rust, specifically

JavaScript engines can run Wasm produced by *any* language — C, C++, Go, AssemblyScript, Zig, C#. Rust is a standout choice for the browser for concrete reasons:

| Reason | Why it matters for Wasm |
| --- | --- |
| **No garbage collector** | A Go or C# Wasm binary must ship its own GC runtime, often hundreds of KB. Rust ships none. Its ownership model (see [Section 05: Ownership](/05-ownership/)) frees memory deterministically at compile time, so the binary stays tiny. |
| **No heavy runtime** | Rust's standard library compiles to almost nothing for the parts you use; there's no interpreter or VM to bundle. |
| **Best-in-class tooling** | `wasm-pack` + `wasm-bindgen` are written and maintained by the Rust/Wasm working group and generate TypeScript type definitions for you automatically. |
| **Memory safety** | The same compile-time guarantees that prevent buffer overflows in native Rust apply in the Wasm sandbox: no use-after-free, no data races. |
| **Predictable performance** | No GC pauses, no JIT warm-up, no de-opt cliffs. The same input produces the same timing every run. |

### The boundary is the catch

Here is the single most important thing to internalize before you write any Wasm. Calling a Wasm function from JavaScript is **not free**. Every call crosses a boundary, and any non-numeric data (strings, arrays, objects) must be **copied or marshalled** between JavaScript's heap and the module's linear memory.

This means Wasm wins when you do **a lot of work per boundary crossing** and loses when you cross the boundary constantly to do tiny work. Grayscaling a whole image in one call: great. Calling a Wasm `add(a, b)` a million times in a JavaScript loop: you'll often be *slower* than plain JavaScript because the call overhead dwarfs the addition. The economics of the boundary are explored in [WebAssembly Performance](/19-wasm/09-performance/).

---

## Key Differences

### WebAssembly vs JavaScript

| Aspect | JavaScript | WebAssembly (from Rust) |
| --- | --- | --- |
| Form | Text source, parsed and JIT-compiled at runtime | Pre-compiled binary bytecode, validated then compiled |
| Types | One number type (`f64`), dynamic objects | `i32`/`i64`/`f32`/`f64` + SIMD; rich types encoded in linear memory |
| Memory | Managed heap, garbage collected | One linear `ArrayBuffer`; freed deterministically by Rust |
| Startup | Parse + JIT warm-up | Fast validation, no warm-up; deterministic timing |
| DOM access | Direct | None directly — must call back into JS (see [Using Web APIs from Rust with web-sys](/19-wasm/06-web-apis/), [Manipulating the DOM from Rust with web-sys](/19-wasm/07-dom-manipulation/)) |
| Threads | Web Workers (message-passing) | Threads via Workers + `SharedArrayBuffer` (advanced) |
| Sandbox | Same-origin policy | Cannot touch memory, OS, or DOM outside what JS hands it |

### What Wasm is *not*

- **It is not a replacement for JavaScript.** You cannot build a web page in pure Wasm. Wasm has no DOM access of its own; it cannot read the document, attach event listeners, or make a `fetch` call without going *through* JavaScript APIs. Whole-app Rust frameworks like Yew and Leptos feel like "pure Rust front ends," but under the hood they still call the browser's JavaScript APIs via generated glue.
- **It is not always faster.** For DOM-heavy, I/O-bound, or boundary-chatty code, the marshalling overhead can make it *slower* than JavaScript. It shines on self-contained, compute-heavy work.
- **It is not Node-only or browser-only.** The same `.wasm` runs in browsers, in Node.js, in Deno, and — with the WASI standard — in standalone runtimes like Wasmtime, on edge platforms, and in plugin systems.

> **Tip:** A good rule of thumb: reach for Wasm when you'd otherwise consider a Web Worker doing pure computation, a native addon, or a "this loop is janking the UI" problem. Reach for plain JavaScript when the work is mostly talking to the DOM or the network.

---

## Common Pitfalls

### Pitfall 1: Expecting Wasm to make ordinary web code faster

The most common disappointment is rewriting glue-heavy or DOM-heavy code in Rust and finding it *slower*. The cost is the boundary, not the language. A button click handler that updates three DOM nodes will never benefit from Wasm; the work isn't CPU-bound, and you'd add marshalling on top.

**Fix:** Profile first. Move only the genuinely CPU-bound kernel (the parser, the codec, the solver) into Wasm and keep orchestration in JavaScript.

### Pitfall 2: Assuming you can pass a JavaScript object straight in

A `struct` or a `String` cannot be handed across the boundary as-is. Wasm only speaks numbers. If you forget the `#[wasm_bindgen]` machinery and just try to expose a normal Rust function that takes a custom type without the right derives, the toolchain rejects it. Even at the language level, mixing up the byte representation is easy. For example, this perfectly ordinary-looking Rust does not compile, because `&[u8]` and `&[u32]` are different types and Rust will not silently reinterpret one buffer as the other:

```rust
fn sum_pixels(pixels: &[u8]) -> u32 {
    let mut total: u32 = 0;
    for value in pixels {
        total += value; // does not compile (error[E0308] + error[E0277])
    }
    total
}
```

Iterating over `&[u8]` yields `&u8` (a borrow of each byte), and Rust refuses to add it to a `u32` accumulator. `rustc` reports two real errors:

```text
error[E0308]: mismatched types
 --> src/lib.rs:4:18
  |
4 |         total += value;
  |                  ^^^^^ expected `u32`, found `u8`

error[E0277]: cannot add-assign `&u8` to `u32`
 --> src/lib.rs:4:15
  |
4 |         total += value;
  |               ^^ no implementation for `u32 += &u8`
  |
  = help: the trait `AddAssign<&u8>` is not implemented for `u32`
```

**Fix:** Dereference and widen explicitly (`total += *value as u32;`). The broader lesson: data crossing the boundary is *bytes you choose how to interpret*, and Rust forces that interpretation to be explicit. The full set of crossing strategies (typed arrays, `JsValue`, `serde-wasm-bindgen`) lives in [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/).

### Pitfall 3: Forgetting that Wasm can't reach the DOM by itself

Coming from JavaScript, it's natural to reach for `document.querySelector` from Rust. It isn't there. Rust in the browser must call DOM APIs through the **`web-sys`** crate, which is a typed wrapper over the JavaScript glue. Trying to "just do DOM stuff" without `web-sys` produces a wall of unresolved-name errors.

**Fix:** Use `web-sys` for the DOM and `js-sys` for built-in JavaScript objects (`Array`, `Object`, `Promise`). See [Using Web APIs from Rust with web-sys](/19-wasm/06-web-apis/) and [Manipulating the DOM from Rust with web-sys](/19-wasm/07-dom-manipulation/).

### Pitfall 4: Shipping a giant binary

A naive build can be surprisingly large if you pull in panic-unwinding machinery, formatting, or heavy crates. Unlike a Node bundle where 200 KB is unremarkable, a 1 MB `.wasm` download is a real cost on mobile.

**Fix:** Build in `--release`, run `wasm-opt`, strip with `twiggy`, and set `panic = "abort"`. Bundle-size discipline is the whole of [WebAssembly Performance](/19-wasm/09-performance/).

---

## Best Practices

- **Use Wasm as a coprocessor, not a rewrite.** Keep the application shell, routing, and DOM work in TypeScript; offload the hot, self-contained algorithm to Rust. This is how production teams (Figma's image/vector kernels, Google Earth, the Photoshop web port) actually use it.
- **Design a coarse-grained API.** Cross the boundary as few times as possible with as much data as possible per call. One `process(buffer)` beats ten thousand `processOne(x)` calls.
- **Let the tooling generate your types.** `wasm-pack` emits a `.d.ts` file, so your TypeScript callers get full type checking on the Wasm exports for free. Don't hand-write the bindings.
- **Measure on the boundary you actually use.** Benchmark the *whole* JavaScript-to-Wasm-and-back path, not just the Rust function in isolation, so the marshalling cost is included.
- **Start with `wasm-pack`, graduate to a framework only if you need one.** For a single fast kernel, a plain `wasm-pack` module imported from your existing Vite/webpack app is the simplest path. Full Rust front ends (Yew, Leptos) are a bigger commitment best deferred until you have a reason to write the whole UI in Rust.
- **Pick the right target.** The browser target is `wasm32-unknown-unknown`. For server-side or standalone Wasm, WASI targets (`wasm32-wasip1`) give you a sandboxed filesystem and clock. The targets and `wasm-pack` build modes (`web`, `bundler`, `nodejs`) are covered in [Setting Up wasm-pack](/19-wasm/01-wasm-pack/).

---

## Real-World Example

Consider a Markdown editor with a live preview that re-renders on every keystroke. Parsing Markdown to HTML is a CPU-bound string-crunching task: exactly where a compiled parser pays off, and where doing it on every keystroke in JavaScript can introduce input lag on large documents.

The Rust side wraps the mature `pulldown-cmark` parser and exposes a single coarse-grained function:

```rust
// src/lib.rs
use pulldown_cmark::{html, Options, Parser};
use wasm_bindgen::prelude::*;

/// Render a Markdown string to an HTML string.
/// Exported to JavaScript as `render_markdown(input: string): string`.
#[wasm_bindgen]
pub fn render_markdown(input: &str) -> String {
    // Enable a few common extensions (tables, strikethrough, task lists).
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(input, options);

    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}
```

```toml
# Cargo.toml
[package]
name = "md-render"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
pulldown-cmark = { version = "0.13", default-features = false, features = ["html"] }
```

The TypeScript side stays in charge of the DOM and the debouncing — the orchestration JavaScript is good at — and calls into Wasm only for the parse:

```typescript
// editor.ts
import init, { render_markdown } from "./pkg/md_render.js";

await init(); // load + instantiate the .wasm module once

const input = document.querySelector<HTMLTextAreaElement>("#source")!;
const preview = document.querySelector<HTMLDivElement>("#preview")!;

input.addEventListener("input", () => {
  // One boundary crossing per keystroke, doing the whole parse in Rust.
  preview.innerHTML = render_markdown(input.value);
});
```

> **Warning:** A real editor must sanitize `render_markdown`'s output before assigning it to `innerHTML`, because Markdown can contain raw HTML. Wasm gives you speed, not XSS protection; the same security rules apply. The generated `pkg/` directory and the `init()` flow come from `wasm-pack`; see [Your First Rust → WebAssembly Module](/19-wasm/02-first-wasm/) for the end-to-end build, and [Calling Rust from JavaScript](/19-wasm/04-rust-from-js/) for what the generated glue looks like.

Each keystroke crosses the boundary exactly once (a single string in, a single string out), and the heavy parsing happens in compiled code with no GC pauses. This is the canonical "Wasm as a fast kernel behind a JavaScript UI" shape.

---

## Further Reading

### Official documentation

- [WebAssembly.org](https://webassembly.org/): the standard, its concepts, and the proposal pipeline
- [MDN: WebAssembly](https://developer.mozilla.org/en-US/docs/WebAssembly): how the browser loads and runs Wasm from JavaScript
- [The Rust and WebAssembly Book](https://rustwasm.github.io/docs/book/): the official guide to the Rust → Wasm workflow
- [wasm-bindgen Guide](https://rustwasm.github.io/wasm-bindgen/): the crate that bridges Rust and JavaScript

### Related sections in this guide

- Next: [Setting Up wasm-pack →](/19-wasm/01-wasm-pack/): project structure, `cdylib`, and build targets
- [Your First Rust → WASM Module](/19-wasm/02-first-wasm/) — export, build, and call from a web page
- [The wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/) — how types actually cross the boundary
- [Using Web APIs with web-sys](/19-wasm/06-web-apis/) and [DOM Manipulation](/19-wasm/07-dom-manipulation/)
- [WASM Performance](/19-wasm/09-performance/): bundle size and the boundary cost
- Background: [Why Rust?](/01-getting-started/00-why-rust/) and [Ownership](/05-ownership/) (the reason Rust ships no GC)
- Going lower-level: [Section 20: Unsafe & FFI](/20-unsafe-ffi/) — the same "talk to another world" skills applied to C

---

## Exercises

### Exercise 1: Classify the workload

**Difficulty:** Easy

**Objective:** Build the instinct for when Wasm helps and when it hurts.

**Instructions:** For each task, decide whether moving it from JavaScript to Rust/Wasm is likely to help, hurt, or make no real difference, and say why in one sentence.

1. Validating a form field's format on every keystroke.
2. Decoding a 20 MB JPEG and applying a blur filter.
3. Re-rendering a list of 10 DOM rows when a checkbox toggles.
4. Running a chess engine that searches millions of board positions per move.
5. A function called in a tight loop one million times that adds two integers.

<details>
<summary>Solution</summary>

1. **No real difference (likely hurts).** The work is trivial; the cost is the boundary crossing and DOM access, not arithmetic. Keep it in JavaScript.
2. **Helps.** CPU-bound pixel crunching over a large buffer, one coarse boundary crossing — the ideal Wasm shape.
3. **Hurts / no benefit.** This is DOM work; Wasm has no direct DOM access and would add marshalling on top of calls back into JavaScript.
4. **Helps a lot.** Deeply CPU-bound, almost no boundary crossings, no GC pauses — exactly where compiled code dominates.
5. **Hurts.** A million boundary crossings to do one addition each: the call overhead dwarfs the work, so plain JavaScript (which V8 will JIT into a tight native loop anyway) usually wins. The lesson: do *more per crossing*.

</details>

### Exercise 2: Predict the byte interpretation

**Difficulty:** Medium

**Objective:** Internalize that Wasm only moves bytes, and Rust forces you to interpret them explicitly.

**Instructions:** The grayscale function takes `&mut [u8]`, a flat byte buffer of RGBA pixels. Suppose a teammate wants a function that sums the brightness of every pixel and returns it. They wrote the version below. Explain why it does not compile, then write a version that does. (You can verify your fix in a `cargo new --lib` project with `cargo check`.)

```rust
fn total_brightness(pixels: &[u8]) -> u32 {
    let mut total: u32 = 0;
    for value in pixels {
        total += value; // problem here
    }
    total
}
```

<details>
<summary>Solution</summary>

Iterating over `&[u8]` yields `&u8` (a borrow of each byte), and Rust will not implicitly widen it into the `u32` accumulator. The compiler reports two errors: `error[E0308]: mismatched types` ("expected `u32`, found `u8`") and `error[E0277]: cannot add-assign `&u8` to `u32`". You must dereference and convert explicitly:

```rust
fn total_brightness(pixels: &[u8]) -> u32 {
    let mut total: u32 = 0;
    for value in pixels {
        total += *value as u32; // deref the &u8, then widen to u32
    }
    total
}
```

A more idiomatic Rust version uses an iterator and `map`, which makes the conversion obvious and lets the compiler optimize the loop:

```rust
fn total_brightness(pixels: &[u8]) -> u32 {
    pixels.iter().map(|&b| b as u32).sum()
}
```

The takeaway: a byte buffer crossing the Wasm boundary carries no meaning of its own — *you* decide whether those bytes are pixels, audio samples, or text, and Rust makes that decision explicit. How those buffers are physically shared with JavaScript is covered in [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/).

</details>

### Exercise 3: Design the boundary

**Difficulty:** Hard

**Objective:** Practice designing a coarse-grained Wasm API instead of a chatty one.

**Instructions:** You're adding a Rust/Wasm module to an existing TypeScript photo app. The product wants three operations on a canvas image: grayscale, invert, and adjust brightness by a delta. A junior engineer proposes exporting `getPixel(x, y)`, `setPixel(x, y, r, g, b)`, and looping over them from TypeScript. Explain why that design will perform badly, then sketch (in prose or function signatures) a better boundary. You don't need a full implementation, just the API shape and the reasoning.

<details>
<summary>Solution</summary>

The `getPixel`/`setPixel` design crosses the JavaScript↔Wasm boundary **twice per pixel**. For a 4K image that is roughly 16 million crossings *per filter*, and each crossing has fixed overhead plus argument marshalling. That overhead, summed over millions of calls, will almost certainly make the Wasm version slower than a plain JavaScript loop, defeating the entire purpose.

A good boundary does the whole operation in **one** crossing, passing the entire pixel buffer in and mutating it in place:

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn grayscale(pixels: &mut [u8]) { /* loop over chunks_exact_mut(4) */ }

#[wasm_bindgen]
pub fn invert(pixels: &mut [u8]) { /* px = 255 - px for R,G,B */ }

#[wasm_bindgen]
pub fn adjust_brightness(pixels: &mut [u8], delta: i32) { /* saturating add per channel */ }
```

TypeScript then calls each filter once with the canvas's `Uint8ClampedArray` (which `wasm-bindgen` maps to `&mut [u8]`), reading the buffer back out only after the Rust function returns:

```typescript
const frame = ctx.getImageData(0, 0, w, h);
grayscale(frame.data);          // one crossing
adjust_brightness(frame.data, 20); // one crossing
ctx.putImageData(frame, 0, 0);
```

The principle generalizes: make each boundary crossing carry as much work as possible. If you need to chain operations, prefer one Rust function that does all of them over many small calls. The deeper mechanics — how a `Uint8ClampedArray` becomes `&mut [u8]` and whether it is copied or shared — are in [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/), and the measured cost of crossings is in [WebAssembly Performance](/19-wasm/09-performance/).

</details>
