---
title: "Your First Rust → WebAssembly Module"
description: "Build a real Rust crate, compile it to WebAssembly, and call its functions from a web page: the WASM equivalent of \"Hello, world!\"."
---

Build a real Rust crate, compile it to WebAssembly, and call its functions from a web page: the WASM equivalent of "Hello, world!".

---

## Quick Overview

**WebAssembly (WASM)** is a compact binary instruction format that runs in every modern browser at near-native speed. With the `#[wasm_bindgen]` attribute and the `wasm-pack` tool, you can write a function in Rust, compile it to a `.wasm` file, and import it into JavaScript almost exactly like a normal ES module. For a TypeScript/JavaScript developer, this is the first time you can ship compiled, statically-typed Rust into the browser and call it from your existing front-end code.

This page walks the full round trip: write the Rust, run one build command, and wire the result into an HTML page. The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024), and `cargo new` selects it automatically. The examples here use **wasm-bindgen 0.2** and **wasm-pack 0.13**.

> **Note:** This page assumes the project is already set up. If you have not installed `wasm-pack` or seen the project layout yet, read [wasm-pack setup](/19-wasm/01-wasm-pack/) first, and [what WASM is and why](/19-wasm/00-wasm-intro/) for the motivation. The mechanics of how values cross the boundary live in [wasm-bindgen deep dive](/19-wasm/05-wasm-bindgen/).

---

## TypeScript/JavaScript Example

Suppose you have a CPU-bound helper in your front end: counting prime numbers below a limit (a stand-in for any tight numeric loop: checksums, image filters, physics). In plain TypeScript you write a function and call it directly:

```typescript
// primes.ts — a hot numeric loop, written in TypeScript
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

// And a string helper, to show non-numeric values too:
export function greet(name: string): string {
  return `Hello, ${name}!`;
}
```

```typescript
// app.ts — using it on a page
import { countPrimes, greet } from "./primes.js";

console.log(greet("Ada"));               // Hello, Ada!
console.log(countPrimes(100_000));       // 9592
```

This works, but every call runs in V8's JavaScript engine. For a loop this hot, the same logic compiled to WebAssembly typically runs noticeably faster and — just as importantly — is checked by Rust's type system and ownership rules at compile time. (Whether WASM actually *wins* for a given workload is a real question, covered in [performance](/19-wasm/09-performance/); for tiny loops the boundary crossing can cost more than you save.)

---

## Rust Equivalent

Here is the same functionality as a Rust library crate. The single new ingredient versus a normal crate is the `#[wasm_bindgen]` attribute, which tells the toolchain "expose this item to JavaScript."

The crate must build as a **`cdylib`** (a C-compatible dynamic library, the shape a `.wasm` file needs). Your `Cargo.toml`:

```toml
# Cargo.toml
[package]
name = "greeter"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
```

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

/// Greets a user by name. Exported to JavaScript as `greet`.
#[wasm_bindgen]
pub fn greet(name: &str) -> String {
    format!("Hello, {name}! This greeting came from Rust + WebAssembly.")
}

/// CPU-bound work: count the primes below `limit`. The kind of tight numeric
/// loop where compiled WebAssembly comfortably beats interpreted JavaScript.
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

Build it for the browser with one command:

```bash
wasm-pack build --target web
```

Real output (first build; subsequent builds are a few seconds):

```text
[INFO]:  Checking for the Wasm target...
[INFO]:  Compiling to Wasm...
   Compiling greeter v0.1.0 (/path/to/greeter)
    Finished `release` profile [optimized] target(s) in 2.91s
[INFO]:  Installing wasm-bindgen...
[INFO]: Optimizing wasm binaries with `wasm-opt`...
[INFO]:   Done in 9.07s
[INFO]:   Your wasm pkg is ready to publish at /path/to/greeter/pkg.
```

This produces a `pkg/` directory:

```text
pkg/
├── greeter_bg.wasm        # the compiled WebAssembly binary (~17 KB here)
├── greeter_bg.wasm.d.ts   # TypeScript types for the raw wasm exports
├── greeter.js             # auto-generated JS "glue" you import from
├── greeter.d.ts           # TypeScript types for the friendly JS API
└── package.json           # a real npm package manifest
```

Now use it from a web page. With `--target web` the package is a plain ES module; no bundler required:

```html
<!-- index.html -->
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>First WASM</title>
  </head>
  <body>
    <script type="module">
      // The default export is the async init function; named exports are your fns.
      import init, { greet, count_primes } from "./pkg/greeter.js";

      async function run() {
        await init();                 // fetch + instantiate the .wasm — do this ONCE
        console.log(greet("Ada"));    // Hello, Ada! This greeting came from Rust + WebAssembly.
        console.log(count_primes(100_000)); // 9592
      }

      run();
    </script>
  </body>
</html>
```

Serve the folder over HTTP (browsers will not `fetch` a `.wasm` over `file://`):

```bash
# any static server works; Python's is always handy
python3 -m http.server 8080
# then open http://localhost:8080
```

The console output (verified by running the same compiled module under Node v22):

```text
Hello, Ada! This greeting came from Rust + WebAssembly.
9592
```

---

## Detailed Explanation

### `use wasm_bindgen::prelude::*;`

This brings the `#[wasm_bindgen]` attribute (and supporting types like `JsValue`) into scope. The `wasm-bindgen` crate is the bridge between Rust types and JavaScript types; the `wasm-bindgen` *CLI* (invoked for you by `wasm-pack`) reads your compiled `.wasm`, finds the marked items, and generates the matching JavaScript glue.

### `#[wasm_bindgen]` on a function

This attribute is a **procedural macro** (covered conceptually in [Macros](/14-macros/), and no, macros are *not* TypeScript decorators; see the comparison below). At compile time it generates the extra "shim" code that marshals arguments across the JS↔WASM boundary. Without it, a `pub fn` is still compiled into the `.wasm` but is **not** exported in a JS-callable way, and `wasm-pack` will not put it in `greeter.js`.

### `greet(name: &str) -> String`

Strings cannot live "inside" a number-only WASM module directly; WebAssembly's core types are just integers and floats. So `wasm-bindgen` generates glue that copies the JS string into the wasm module's linear memory, hands Rust a `&str` pointing at it, then copies the returned `String` back out and frees the temporary buffer. You write ordinary Rust (`&str` in, `String` out); the byte-shuffling is generated for you. The generated `greet` wrapper in `greeter.js` looks like this (real, lightly trimmed):

```javascript
// pkg/greeter.js (generated)
export function greet(name) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.greet(ptr0, len0);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}
```

You never write that — but seeing it demystifies what "the glue" actually is. The mechanics of which types cross cheaply versus expensively are detailed in [wasm-bindgen deep dive](/19-wasm/05-wasm-bindgen/).

### `count_primes(limit: u32) -> u32`

Pure numbers are the *fast* case: `u32` maps straight onto a WASM `i32` with no copying. The generated wrapper is almost nothing:

```javascript
// pkg/greeter.js (generated)
export function count_primes(limit) {
    const ret = wasm.count_primes(limit);
    return ret >>> 0;   // reinterpret the i32 as unsigned
}
```

> **Note:** Rust's `u32` wraps at 2³², and the JS side reads it back as an unsigned number (`>>> 0`). This is different from JavaScript's `number`, which is always an IEEE-754 `f64` and *loses precision* past 2⁵³ rather than wrapping. Choose your Rust integer width deliberately.

### `await init()` — the part TS/JS devs forget

In `--target web` mode the package's **default export** is an async initializer. It `fetch`es `greeter_bg.wasm`, compiles it, and instantiates it. **Until that promise resolves, the exported functions are not ready.** This is the single most common first mistake (see Pitfalls). Unlike a normal ES module — whose exports exist the moment the import resolves — a WASM module needs an explicit, asynchronous instantiation step. Rust futures are lazy and need a runtime; similarly, a WASM module is inert bytes until you instantiate it.

The generated `package.json` confirms it is a genuine, publishable npm package:

```json
{
  "name": "greeter",
  "type": "module",
  "version": "0.1.0",
  "files": ["greeter_bg.wasm", "greeter.js", "greeter.d.ts"],
  "main": "greeter.js",
  "types": "greeter.d.ts",
  "sideEffects": ["./snippets/*"]
}
```

### The generated TypeScript types

Because you wrote typed Rust, you get typed TypeScript for free. The generated `greeter.d.ts` (real output):

```typescript
// pkg/greeter.d.ts (generated)
/**
 * CPU-bound work: count the primes below `limit`. ...
 */
export function count_primes(limit: number): number;

/**
 * Greets a user by name. Exported to JavaScript as `greet`.
 */
export function greet(name: string): string;

export default function __wbg_init(/* ... */): Promise<InitOutput>;
```

Your Rust doc comments (`///`) even become JSDoc on the TypeScript side. A consuming TypeScript file gets full autocomplete and type-checking on `greet` and `count_primes`.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust → WASM |
| --- | --- | --- |
| Unit of code | An `.ts`/`.js` module | A `cdylib` crate compiled to `.wasm` |
| "Export" mechanism | `export function` | `#[wasm_bindgen] pub fn` |
| Build step | `tsc`/bundler (or none) | `wasm-pack build` |
| When exports are usable | Immediately on import | Only after `await init()` |
| String passing | Native; strings are values | Copied into/out of linear memory by glue |
| Numbers | All `f64`; lose precision past 2⁵³ | Real `u32`/`i64`/`f64`; wrap, not lose precision |
| Type info | You write the types | Generated `.d.ts` from your Rust types |
| Runtime cost | JIT-compiled in V8 | Compiled ahead of time; near-native |

### `#[wasm_bindgen]` is not a decorator

It is tempting to read `#[wasm_bindgen]` as a TypeScript decorator. It is **not**. A decorator is a runtime function that wraps a class/method *while your program runs*. `#[wasm_bindgen]` is a compile-time procedural macro: it generates additional Rust code (and metadata that the `wasm-bindgen` CLI later turns into JS) *before* anything runs. There is no decorator object, no runtime indirection, and no per-call overhead from the attribute itself.

### One instantiation, many calls

`init()` is asynchronous and should run exactly once. After it resolves, every call to `greet`/`count_primes` is synchronous and cheap. Treat init like opening a database connection: do it at startup, then reuse it.

---

## Common Pitfalls

### Pitfall 1: Calling an export before `await init()`

The most common first error. The functions exist as imports, but the wasm module behind them is not instantiated yet:

```javascript
// Wrong: calling before instantiation
import init, { greet } from "./pkg/greeter.js";

console.log(greet("Ada")); // TypeError: Cannot read properties of undefined ...
init();
```

The `greet` wrapper dereferences the not-yet-assigned `wasm` object, so you get a `TypeError` in the browser console (the exact message varies by browser/version). Always:

```javascript
// Right: await first, then call
import init, { greet } from "./pkg/greeter.js";

await init();
console.log(greet("Ada"));
```

### Pitfall 2: Forgetting `crate-type = ["cdylib"]`

A default library crate produces an `rlib` (Rust's own format), not a WASM-loadable dynamic library. If you omit the `[lib] crate-type` line, `wasm-pack` cannot find a `cdylib` to process and the build aborts. Keep both: `["cdylib", "rlib"]` — `cdylib` for the browser, `rlib` so you can still `cargo test` the crate natively. Project layout details are in [wasm-pack setup](/19-wasm/01-wasm-pack/).

### Pitfall 3: Returning a type WASM cannot marshal

Every value crossing the boundary must implement `wasm-bindgen`'s conversion traits. Plain Rust structs do **not** — unless you also mark them `#[wasm_bindgen]`. This compiles natively but fails for WASM:

```rust
// does not compile (error[E0277]: the trait bound `Point: IntoWasmAbi` is not satisfied)
use wasm_bindgen::prelude::*;

pub struct Point {     // NOT marked #[wasm_bindgen]
    pub x: f64,
    pub y: f64,
}

#[wasm_bindgen]
pub fn origin() -> Point {   // can't hand a plain struct to JS
    Point { x: 0.0, y: 0.0 }
}
```

The real compiler error:

```text
error[E0277]: the trait bound `Point: IntoWasmAbi` is not satisfied
 --> src/lib.rs:8:1
  |
8 | #[wasm_bindgen]
  | ^^^^^^^^^^^^^^^ the trait `IntoWasmAbi` is not implemented for `Point`
  |
  = note: required for `Point` to implement `ReturnWasmAbi`
  = note: this error originates in the attribute macro `wasm_bindgen` ...

error[E0277]: the trait bound `Point: wasm_bindgen::describe::WasmDescribe` is not satisfied
 --> src/lib.rs:9:20
  |
9 | pub fn origin() -> Point {
  |                    ^^^^^ the trait `WasmDescribe` is not implemented for `Point`
```

**Fix:** mark the struct `#[wasm_bindgen]` (it becomes a JS class — see the real-world example below), or return a primitive/`String`/`Vec`, or convert it to a `JsValue` with `serde-wasm-bindgen` ([wasm-bindgen deep dive](/19-wasm/05-wasm-bindgen/)).

### Pitfall 4: Opening `index.html` from `file://`

Double-clicking the HTML file loads it as `file://...`, and browsers refuse to `fetch` the `.wasm` from there (CORS / security). You will see a fetch or instantiation error in the console. **Fix:** serve over HTTP (`python3 -m http.server`, `npx serve`, Vite, etc.). Correct serving and MIME types are covered in [deployment](/19-wasm/10-deployment/).

### Pitfall 5: Silent panics that say "unreachable"

If your Rust panics inside WASM, the default message in the console is the unhelpful `RuntimeError: unreachable executed`. Add the `console_error_panic_hook` crate so panics print a real message and stack trace (see Best Practices).

---

## Best Practices

### Add a panic hook during development

```toml
# Cargo.toml
[dependencies]
wasm-bindgen = "0.2"
console_error_panic_hook = "0.1"
```

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

// Runs once, automatically, when the module is instantiated.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}
```

The `#[wasm_bindgen(start)]` attribute marks a function to run on instantiation (the WASM equivalent of top-level module code). Now a Rust `panic!` shows up as a readable error in the browser console instead of `unreachable executed`. This snippet compiles cleanly against `wasm-bindgen 0.2` and `console_error_panic_hook 0.1`.

### Keep functions coarse-grained

Each JS→WASM call has a small fixed cost, and copying strings/arrays across the boundary is not free. Prefer one call that does meaningful work (e.g. `process_image(pixels)`) over thousands of tiny calls in a JS loop. The boundary-cost analysis is in [performance](/19-wasm/09-performance/).

### Build `--release` for shipping

`wasm-pack build` defaults to a release build already; for the smallest binary also run `wasm-opt` (which `wasm-pack` invokes automatically) and consider `twiggy` to find bloat — both in [performance](/19-wasm/09-performance/). Our tiny `greeter_bg.wasm` is about **17 KB** after optimization.

### Pick the right `--target`

`--target web` gives a no-bundler ES module (used here). Use `--target bundler` for Vite/webpack and `--target nodejs` for Node. The differences are spelled out in [wasm-pack setup](/19-wasm/01-wasm-pack/).

### Test the logic natively

Keep the `rlib` crate-type so the same code can be unit-tested with plain `cargo test` on your host machine (fast, no browser). Add WASM-specific tests later with `wasm-bindgen-test`.

---

## Real-World Example

A production-flavored module: compute summary statistics over a dataset coming from JavaScript. The dataset arrives as a `Float64Array` (which `wasm-bindgen` hands you as `&[f64]`), and we return a **`Summary` struct exported as a JS class** with read-only getters: the idiomatic way to return structured data without serialization.

```toml
# Cargo.toml
[package]
name = "stats"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
```

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

/// A summary of a numeric dataset, exported to JavaScript as a class.
#[wasm_bindgen]
pub struct Summary {
    mean: f64,
    min: f64,
    max: f64,
}

#[wasm_bindgen]
impl Summary {
    // `#[wasm_bindgen(getter)]` exposes the private field as a read-only JS property.
    #[wasm_bindgen(getter)]
    pub fn mean(&self) -> f64 {
        self.mean
    }

    #[wasm_bindgen(getter)]
    pub fn min(&self) -> f64 {
        self.min
    }

    #[wasm_bindgen(getter)]
    pub fn max(&self) -> f64 {
        self.max
    }
}

/// Takes a JS `Float64Array` (arrives as `&[f64]`) and returns a `Summary`.
#[wasm_bindgen]
pub fn summarize(values: &[f64]) -> Summary {
    let n = values.len() as f64;
    let sum: f64 = values.iter().sum();
    let mean = if n > 0.0 { sum / n } else { 0.0 };
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Summary { mean, min, max }
}
```

Build and use it:

```bash
wasm-pack build --target web
```

```javascript
// app.js
import init, { summarize } from "./pkg/stats.js";

await init();

const data = new Float64Array([3, 1, 4, 1, 5, 9, 2, 6]);
const s = summarize(data);

console.log("mean =", s.mean); // mean = 3.875
console.log("min  =", s.min);  // min  = 1
console.log("max  =", s.max);  // max  = 9

// A WASM-backed object owns Rust memory. Free it when done to avoid leaks.
s.free();
```

Verified runtime output (running the compiled module under Node v22):

```text
mean = 3.875
min  = 1
max  = 9
```

> **Warning:** A `#[wasm_bindgen]` struct returned to JS is a handle to memory *inside* the wasm module; JavaScript's garbage collector does **not** automatically reclaim it. Call `.free()` when finished (or, in the latest stable, rely on the generated `[Symbol.dispose]` with a `using` declaration where supported). This explicit-ownership-across-the-boundary model is examined further in [exporting structs to JS](/19-wasm/04-rust-from-js/).

> **Tip:** Returning a `#[wasm_bindgen]` struct avoids JSON serialization entirely; JS gets a thin object whose getters call straight into wasm. For ad-hoc/dynamic shapes, serialize to a plain JS object with `serde-wasm-bindgen` instead; see [wasm-bindgen deep dive](/19-wasm/05-wasm-bindgen/).

---

## Further Reading

### Official documentation

- [The `wasm-bindgen` Guide](https://rustwasm.github.io/wasm-bindgen/): the canonical reference for `#[wasm_bindgen]`
- [`wasm-pack` documentation](https://rustwasm.github.io/wasm-pack/book/) — build tool and commands
- [MDN: WebAssembly](https://developer.mozilla.org/en-US/docs/WebAssembly): the platform feature itself
- [`console_error_panic_hook` on docs.rs](https://docs.rs/console_error_panic_hook) — readable panics in the browser

### Related sections in this guide

- [What WebAssembly is and why use Rust for it](/19-wasm/00-wasm-intro/): start here for the big picture
- [Setting up wasm-pack](/19-wasm/01-wasm-pack/) — project structure, `cdylib`, build targets
- [Calling JavaScript from Rust](/19-wasm/03-js-interop/) and [Calling Rust from JavaScript](/19-wasm/04-rust-from-js/)
- [wasm-bindgen deep dive](/19-wasm/05-wasm-bindgen/) — `JsValue`, `serde-wasm-bindgen`, closures
- [Using Web APIs with web-sys](/19-wasm/06-web-apis/) and [DOM manipulation](/19-wasm/07-dom-manipulation/)
- [WASM performance](/19-wasm/09-performance/) and [deploying WASM apps](/19-wasm/10-deployment/)
- Foundations referenced above: [Hello World](/01-getting-started/02-hello-world/), [Cargo basics](/01-getting-started/03-cargo-basics/), [Basics: types](/02-basics/01-types/), [Macros](/14-macros/)
- For lower-level native interop (no browser), see [Unsafe & FFI](/20-unsafe-ffi/)

---

## Exercises

### Exercise 1: Export your own function

**Difficulty:** Beginner

**Objective:** Get the full Rust → WASM → web-page loop working with a function you wrote.

**Instructions:** Create a crate with `wasm-pack`, add a `#[wasm_bindgen]` function `add(a: i32, b: i32) -> i32`, build with `wasm-pack build --target web`, and call it from an HTML page after `await init()`. Log the result of `add(2, 40)` to the console.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "adder"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
```

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

```bash
wasm-pack build --target web
python3 -m http.server 8080
```

```html
<!-- index.html -->
<!DOCTYPE html>
<html lang="en">
  <head><meta charset="utf-8" /><title>Adder</title></head>
  <body>
    <script type="module">
      import init, { add } from "./pkg/adder.js";
      await init();
      console.log(add(2, 40)); // 42
    </script>
  </body>
</html>
```

The `i32`s map directly onto WASM `i32`s, so the generated glue is a near-passthrough — the fast, copy-free case. Confirmed natively: `add(2, 40)` returns `42`.

</details>

### Exercise 2: Return a String and a Vec

**Difficulty:** Intermediate

**Objective:** See how non-primitive values cross the boundary, and contrast `String` (copied) with `i32` (passed directly).

**Instructions:** Export `repeat_word(word: &str, times: usize) -> String` that returns `word` repeated `times` times joined by spaces, and `squares(n: u32) -> Vec<u32>` that returns `[0, 1, 4, 9, ...]` for the first `n` integers. Build and call both from JS; note that the `Vec<u32>` arrives in JavaScript as a `Uint32Array`.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn repeat_word(word: &str, times: usize) -> String {
    vec![word; times].join(" ")
}

#[wasm_bindgen]
pub fn squares(n: u32) -> Vec<u32> {
    (0..n).map(|i| i * i).collect()
}
```

```javascript
// app.js
import init, { repeat_word, squares } from "./pkg/yourcrate.js";

await init();
console.log(repeat_word("hi", 3)); // "hi hi hi"
console.log(squares(5));           // Uint32Array(5) [ 0, 1, 4, 9, 16 ]
```

`String` is copied out of wasm linear memory and freed for you; `Vec<u32>` is handed to JS as a typed array (`Uint32Array`) backed by a fresh copy. Both compile against `wasm-bindgen 0.2` with no extra dependencies. (Verified natively: `repeat_word("hi", 3)` → `"hi hi hi"`, `squares(5)` → `[0, 1, 4, 9, 16]`.)

</details>

### Exercise 3: A stateful counter as a JS class

**Difficulty:** Advanced

**Objective:** Export a Rust struct with a constructor and a mutating method, and understand why you must `.free()` it.

**Instructions:** Export a `Counter` struct holding an `i32`. Give it a `#[wasm_bindgen(constructor)]` that takes a starting value, and an `increment(&mut self) -> i32` method that adds one and returns the new value. From JS, create `new Counter(10)`, call `increment()` twice, log the results (`11`, then `12`), and `free()` it. Explain in a comment why `free()` is needed.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Counter {
    value: i32,
}

#[wasm_bindgen]
impl Counter {
    #[wasm_bindgen(constructor)]
    pub fn new(start: i32) -> Counter {
        Counter { value: start }
    }

    pub fn increment(&mut self) -> i32 {
        self.value += 1;
        self.value
    }
}
```

```javascript
// app.js
import init, { Counter } from "./pkg/yourcrate.js";

await init();

const c = new Counter(10);
console.log(c.increment()); // 11
console.log(c.increment()); // 12

// The Counter's `value` lives in wasm linear memory, which JavaScript's GC
// does not manage. `free()` runs Rust's deallocation; skipping it leaks memory.
c.free();
```

The `#[wasm_bindgen]` struct becomes a JS `class`; `#[wasm_bindgen(constructor)]` makes `new Counter(10)` call `Counter::new`. The generated `.d.ts` declares `class Counter { constructor(start: number); increment(): number; free(): void; }`, so TypeScript callers get full types. This is the same JS-class-from-Rust-struct pattern as the `Summary` example above, and is explored further in [exporting structs to JS](/19-wasm/04-rust-from-js/).

</details>
