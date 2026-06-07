---
title: "Calling Rust from JavaScript"
description: "A wasm-bindgen struct becomes a JS class with a constructor, getters, methods, and a manual free(). How Rust exports and errors look from TypeScript."
---

## Quick Overview

Once you compile a Rust crate to WebAssembly with `#[wasm_bindgen]`, the functions and structs you mark `pub` become real JavaScript values: an exported function turns into a JS function, and an exported struct turns into a JS **class** with methods, getters, and a constructor. This page is about the *consumer* side of that boundary: what your exports look like from JavaScript or TypeScript, the JS "glue" code `wasm-bindgen` generates to make it work, and the rules a TypeScript developer needs to internalize so the experience feels like calling any other module.

> **Note:** The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. The examples here target `wasm-bindgen` 0.2 (resolved to 0.2.122 at the time of writing) on the `wasm32-unknown-unknown` target. This page assumes you already have a project building; see [Setting Up wasm-pack](/19-wasm/01-wasm-pack/) and [Your First Rust to WASM Module](/19-wasm/02-first-wasm/). The *opposite* direction (calling JS from Rust) is covered in [Calling JavaScript from Rust](/19-wasm/03-js-interop/), and the fine details of which types can cross the boundary live in the [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/).

---

## TypeScript/JavaScript Example

Here is the kind of thing you would reach for a library to do: a small analytics module. In a pure-TypeScript world you would publish it as an npm package and import it.

```typescript
// analytics.ts — a hand-written TypeScript module
export function mean(values: number[]): number {
  if (values.length === 0) return NaN;
  return values.reduce((a, b) => a + b, 0) / values.length;
}

export function formatCurrency(amount: number): string {
  return `$${amount.toFixed(2)}`;
}

// A class with state, a constructor, methods, and a read-only property.
export class Histogram {
  private buckets: Uint32Array;
  private readonly bucketWidth: number;

  constructor(bucketCount: number, bucketWidth: number) {
    this.buckets = new Uint32Array(bucketCount);
    this.bucketWidth = bucketWidth;
  }

  record(value: number): void {
    const idx = Math.min(
      Math.floor(value / this.bucketWidth),
      this.buckets.length - 1,
    );
    this.buckets[idx] += 1;
  }

  get total(): number {
    return this.buckets.reduce((a, b) => a + b, 0);
  }

  counts(): Uint32Array {
    return this.buckets.slice();
  }
}
```

A caller imports it and uses it exactly as you would expect:

```typescript
// app.ts
import { mean, formatCurrency, Histogram } from "./analytics";

console.log(mean([10, 20, 30])); // 20
console.log(formatCurrency(19.5)); // "$19.50"

const hist = new Histogram(4, 25);
hist.record(10);
hist.record(60);
console.log(hist.total); // 2 — a property, no parentheses
console.log(hist.counts()); // Uint32Array(4) [ 1, 0, 1, 0 ]
```

Two things to hold onto, because they shape everything that follows: the class manages its own memory through the garbage collector (you never `free` a `Histogram`), and `total` is a *property* you read without calling it.

---

## Rust Equivalent

The same module written in Rust and exported with `#[wasm_bindgen]`. The crate is a library compiled to the `cdylib` crate type (see [Setting Up wasm-pack](/19-wasm/01-wasm-pack/) for the `Cargo.toml`).

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

/// A free function. Becomes an exported JS function `mean(values)`.
#[wasm_bindgen]
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Rename on the JS side: callable as `formatCurrency(...)` in JS,
/// while staying idiomatic snake_case in Rust.
#[wasm_bindgen(js_name = formatCurrency)]
pub fn format_currency(amount: f64) -> String {
    format!("${amount:.2}")
}

/// A struct exported as a JS class.
#[wasm_bindgen]
pub struct Histogram {
    buckets: Vec<u32>,
    bucket_width: f64,
}

#[wasm_bindgen]
impl Histogram {
    /// Exported as the JS constructor: `new Histogram(bucketCount, bucketWidth)`.
    #[wasm_bindgen(constructor)]
    pub fn new(bucket_count: usize, bucket_width: f64) -> Histogram {
        Histogram {
            buckets: vec![0; bucket_count],
            bucket_width,
        }
    }

    /// A method: `histogram.record(value)`.
    pub fn record(&mut self, value: f64) {
        let idx = (value / self.bucket_width) as usize;
        let last = self.buckets.len() - 1;
        let idx = idx.min(last);
        self.buckets[idx] += 1;
    }

    /// A getter: read as `histogram.total` (no parentheses) from JS.
    #[wasm_bindgen(getter)]
    pub fn total(&self) -> u32 {
        self.buckets.iter().sum()
    }

    /// Returns owned data; the bytes are copied into a JS `Uint32Array`.
    pub fn counts(&self) -> Vec<u32> {
        self.buckets.clone()
    }
}
```

Building this with `wasm-pack build --target web` produces a `pkg/` directory whose `.d.ts` declares the exports. This is the **real, generated** TypeScript surface, not something hand-written:

```typescript
// pkg/analytics.d.ts (generated by wasm-bindgen)
/* tslint:disable */
/* eslint-disable */

/**
 * A struct exported as a JS class.
 */
export class Histogram {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Returns owned data; the bytes are copied into a JS `Uint32Array`.
     */
    counts(): Uint32Array;
    /**
     * Exported as the JS constructor: `new Histogram(bucketCount, bucketWidth)`.
     */
    constructor(bucket_count: number, bucket_width: number);
    /**
     * A method: `histogram.record(value)`.
     */
    record(value: number): void;
    /**
     * A getter: read as `histogram.total` (no parentheses) from JS.
     */
    readonly total: number;
}

/**
 * Rename on the JS side: callable as `formatCurrency(...)` in JS,
 * while staying idiomatic snake_case in Rust.
 */
export function formatCurrency(amount: number): string;

/**
 * A free function. Becomes an exported JS function `mean(values)`.
 */
export function mean(values: Float64Array): number;
```

Notice that the generated declarations match the TypeScript module almost exactly: `mean` returns a `number`, `Histogram` is a class with a `constructor`, `record` is a method, and `total` is `readonly` (a getter). Each `///` doc comment is carried through as a JSDoc block on the corresponding declaration. The two extra members — `free()` and `[Symbol.dispose]()` — are the memory-management hook that has no TypeScript equivalent. More on that below.

---

## Detailed Explanation

### Free functions become exported functions

`#[wasm_bindgen]` on a `pub fn` does two jobs. First, it tells the compiler to keep the function in the final `.wasm` binary and expose it as a WebAssembly export. Second — and this is the part that matters for JavaScript callers — it generates a JavaScript wrapper that translates arguments and return values across the boundary. WebAssembly itself only understands integers and floats, so anything richer (a string, a slice, a struct) needs glue.

Here is the actual generated wrapper for `mean`:

```javascript
// pkg/analytics.js (generated) — the wrapper for `mean`
export function mean(values) {
    const ptr0 = passArrayF64ToWasm0(values, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.mean(ptr0, len0);
    return ret;
}
```

The Rust signature `mean(values: &[f64])` became a JS function that takes a `Float64Array`. The glue copies the array's bytes into the WebAssembly linear memory (`passArrayF64ToWasm0`), then calls the *raw* exported function `wasm.mean(ptr, len)` with a pointer and length: exactly the two integers WebAssembly can actually pass. Your Rust code receives a normal `&[f64]` slice and never sees any of this.

`format_currency` shows the same pattern for strings. The wrapper allocates space in WebAssembly memory, calls the function, then reads the resulting `String` back out and frees it:

```javascript
// pkg/analytics.js (generated) — the wrapper for `formatCurrency`
export function formatCurrency(amount) {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.formatCurrency(amount);
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}
```

> **Tip:** The `#[wasm_bindgen(js_name = formatCurrency)]` attribute renames the export. Rust convention is `snake_case` for functions, JavaScript convention is `camelCase`. Without the rename, JS callers would have to write `format_currency`, which looks out of place in a JS/TS codebase. Renaming keeps both sides idiomatic.

### Structs become JS classes

A `pub struct` annotated with `#[wasm_bindgen]` becomes a JavaScript class. The Rust value lives in WebAssembly's linear memory; the JS object is a thin **handle** that holds a pointer to it. Here is the generated class:

```javascript
// pkg/analytics.js (generated) — the Histogram class
export class Histogram {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        HistogramFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_histogram_free(ptr, 0);
    }
    counts() {
        const ret = wasm.histogram_counts(this.__wbg_ptr);
        var v1 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    constructor(bucket_count, bucket_width) {
        const ret = wasm.histogram_new(bucket_count, bucket_width);
        this.__wbg_ptr = ret;
        HistogramFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    record(value) {
        wasm.histogram_record(this.__wbg_ptr, value);
    }
    get total() {
        const ret = wasm.histogram_total(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) Histogram.prototype[Symbol.dispose] = Histogram.prototype.free;
```

Reading this top to bottom tells the whole story:

- `this.__wbg_ptr` is the pointer to the Rust `Histogram` inside WebAssembly memory. Every method passes it as the first argument to the raw export (`wasm.histogram_record(this.__wbg_ptr, value)`). This is exactly how `&self`/`&mut self` is implemented across the boundary.
- The `constructor` calls `wasm.histogram_new(...)`, stores the returned pointer, and registers the object with a `FinalizationRegistry`.
- `get total()` is generated from the `#[wasm_bindgen(getter)]` method, so JS reads `hist.total` with no parentheses. The `>>> 0` coerces the value to an unsigned 32-bit integer (Rust's `u32`).
- `counts()` reads a `Vec<u32>` back out of WebAssembly memory into a fresh JS `Uint32Array` and frees the temporary Rust allocation.

### Memory: the `free()` method and `FinalizationRegistry`

This is the single biggest conceptual difference from a hand-written TypeScript class, so it is worth slowing down. In JavaScript, the garbage collector owns every object; you never deallocate manually. But the actual `Histogram` data lives in WebAssembly's linear memory, which the JS garbage collector does **not** manage. So the Rust value has to be freed explicitly when the JS handle goes away.

`wasm-bindgen` handles this two ways:

1. A **`FinalizationRegistry`** (a standard JS API since 2021) is wired up so that *if* the JS garbage collector eventually collects the handle, a callback runs `__wbg_histogram_free` to release the Rust memory. This is a safety net, not a guarantee; finalizers run "eventually, maybe."
2. An explicit **`free()`** method (and, in modern output, `[Symbol.dispose]`, so it works with the TC39 `using` declaration) lets you release the memory deterministically the moment you are done.

```typescript
// Deterministic cleanup with the `using` declaration (TypeScript 5.2+)
import init, { Histogram } from "./pkg/analytics.js";

await init();

{
  using hist = new Histogram(4, 25); // [Symbol.dispose] === free
  hist.record(10);
  console.log(hist.total);
} // hist.free() runs automatically here
```

> **Warning:** Relying solely on the `FinalizationRegistry` can leak WebAssembly memory in long-running apps, because finalizers are not prompt and not guaranteed to run at all. For short-lived objects this is fine; for objects you create in a hot loop, call `free()` (or use `using`) explicitly.

### How a JavaScript caller wires it up

With the `web` target, the generated module is an ES module with a default export that loads and instantiates the `.wasm` file. You **must** await initialization before calling anything:

```typescript
// app.ts — consuming the `web`-target build
import init, { mean, formatCurrency, Histogram } from "./pkg/analytics.js";

async function main() {
  await init(); // fetch + instantiate analytics_bg.wasm — required first

  console.log(mean(new Float64Array([10, 20, 30]))); // 20
  console.log(formatCurrency(19.5)); // "$19.50"

  const hist = new Histogram(4, 25);
  hist.record(10);
  hist.record(60);
  console.log(hist.total); // 2
  console.log(hist.counts()); // Uint32Array(4) [ 1, 0, 1, 0 ]
  hist.free();
}

main();
```

The `await init()` step is the one piece with no TypeScript-module analogue. JavaScript modules are ready to use the moment they are imported; a WebAssembly module has to be *fetched and instantiated* first, which is asynchronous. Forgetting `await init()` is the most common first-time mistake (see Common Pitfalls).

---

## Key Differences

| Aspect | Hand-written TypeScript module | Rust compiled with `#[wasm_bindgen]` |
| --- | --- | --- |
| Exported function | `export function f(...)` | `#[wasm_bindgen] pub fn f(...)` |
| Exported class | `export class C` | `#[wasm_bindgen] pub struct C` + `impl` block |
| Constructor | `constructor(...)` | method tagged `#[wasm_bindgen(constructor)]` |
| Getter | `get x()` | method tagged `#[wasm_bindgen(getter)]` |
| Naming | `camelCase` everywhere | `snake_case` in Rust, `#[wasm_bindgen(js_name=...)]` to rename |
| Memory | GC-managed, invisible | handle + pointer; needs `free()` or `using` |
| Initialization | ready on `import` | must `await init()` first (web/bundler targets) |
| Generics | erased at runtime | **not allowed** on exports; must be concrete |
| Errors | `throw` anything | `Result<T, JsError>` → thrown JS `Error` |
| `number[]` arg | a real JS array | maps to a typed array (`Float64Array`, etc.) |

The deepest difference is **ownership crossing a language boundary**. In TypeScript, every value is owned by the JS runtime. With WebAssembly, an exported struct is owned by *Rust* and merely referenced by a JS handle. That is why a `Histogram` has a `free()` method and a plain TS class does not, and why methods that return collections (`counts()`) hand you a *copy* rather than a live view: the underlying `Vec<u32>` belongs to Rust.

A second difference worth flagging: TypeScript generics are erased at compile time and have no runtime cost, so `function identity<T>(x: T): T` is perfectly normal. A generic Rust function **cannot** be exported, because there is no single concrete machine-code function to expose — WebAssembly needs a monomorphized, concrete signature. You must pick concrete types at the boundary.

### Error handling crosses as exceptions

Returning a `Result<T, E>` where `E: Into<JsValue>` (the simplest being `JsError`) turns the `Err` arm into a thrown JavaScript exception, and the `Ok` arm into the plain return value. The generated TypeScript type shows only the success type; the throwing is invisible in the type, just like any TS function that can `throw`:

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

/// The `Err` arm becomes a thrown JS exception; the `Ok` arm is the return value.
#[wasm_bindgen(js_name = parsePercentage)]
pub fn parse_percentage(input: &str) -> Result<f64, JsError> {
    let trimmed = input.trim().trim_end_matches('%');
    let value: f64 = trimmed
        .parse()
        .map_err(|_| JsError::new(&format!("not a number: {input:?}")))?;
    if !(0.0..=100.0).contains(&value) {
        return Err(JsError::new("percentage must be between 0 and 100"));
    }
    Ok(value / 100.0)
}
```

The generated declaration is just `export function parsePercentage(input: string): number;`; note the `: number`, with no hint of the error path. From JavaScript you handle it with an ordinary `try`/`catch`:

```typescript
import init, { parsePercentage } from "./pkg/analytics.js";
await init();

console.log(parsePercentage("42%")); // 0.42
try {
  parsePercentage("nope");
} catch (e) {
  console.log((e as Error).message); // 'not a number: "nope"'
}
```

Running this exact module under Node (built with `--target nodejs`) produces real output:

```text
parsePercentage('42%'): 0.42
threw: not a number: "nope"
threw: percentage must be between 0 and 100
```

The Rust error message survives the crossing intact as the JS `Error.message`. For a thorough treatment of richer error values and `JsValue`, see the [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/).

---

## Common Pitfalls

### Pitfall 1: Trying to export a generic function

Coming from TypeScript, writing a generic helper feels natural. It does not compile when exported:

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn largest<T: PartialOrd>(a: T, b: T) -> T { // does not compile
    if a > b { a } else { b }
}
```

The compiler is explicit about why:

```text
error: can't #[wasm_bindgen] functions with lifetime or type parameters
 --> src/lib.rs:4:15
  |
4 | pub fn largest<T: PartialOrd>(a: T, b: T) -> T {
```

**Fix:** pick a concrete type at the boundary (`pub fn largest(a: f64, b: f64) -> f64`). You can keep an internal generic helper and export a concrete wrapper that calls it.

### Pitfall 2: A public, non-`Copy` field on an exported struct

`wasm-bindgen` generates a getter *and* a setter for each `pub` field of an exported struct. By default the getter returns the value, which requires the field to be `Copy`. A `String` is not `Copy`:

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct User {
    pub id: u32,
    pub name: String, // does not compile
}
```

The real error:

```text
error[E0277]: the trait bound `String: std::marker::Copy` is not satisfied
 --> src/lib.rs:6:15
  |
3 | #[wasm_bindgen]
  | --------------- in this procedural macro expansion
...
6 |     pub name: String,  // non-Copy public field
  |               ^^^^^^ the trait `std::marker::Copy` is not implemented for `String`
  |
note: required by a bound in `__wbg_get_user_name::assert_copy`
```

**Fix:** add `getter_with_clone` so the generated getter clones the field instead of copying it. This compiles cleanly:

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen(getter_with_clone)]
pub struct User {
    pub id: u32,
    pub name: String, // now exported via a cloning getter/setter
}
```

### Pitfall 3: Using a handle after `free()` (or after `using` scope ends)

Because the Rust value is freed but the JS handle object still exists, calling a method on a freed handle is a use-after-free. It does not crash silently — `wasm-bindgen` detects the null pointer and throws:

```typescript
const stats = new TextStats("hello world");
stats.free();
console.log(stats.wordCount); // throws
```

The real runtime error (captured running the module in Node):

```text
Error: null pointer passed to rust
    at __wbg___wbindgen_throw_... (analytics.js:39:19)
    at wasm://wasm/a50b6c42:wasm-function[43]:0x3206
```

**Fix:** do not touch a handle after `free()`. With the `using` declaration, watch out for the same trap: the object is freed at the end of the block, so do not stash a reference that outlives the block.

> **Note:** This also bites you when you **pass a struct by value** into another exported function. Moving a value into Rust consumes the JS handle (its pointer is zeroed), so the original JS variable becomes a freed handle. If you need to keep using it afterward, take `&self` instead of `self` on the Rust side.

### Pitfall 4: Forgetting `await init()`

With the `web` and `bundler` targets, the `.wasm` must be instantiated before any export works. Calling an export first throws a `TypeError` about reading a property of `undefined` (the glue's `wasm` binding is still unset). Always `await init()` (web target) or import from a bundler-aware entry that does it for you. The `nodejs` target is the exception: it instantiates synchronously at `require` time using `fs.readFileSync`, so there is no init step.

---

## Best Practices

- **Rename exports to `camelCase`.** Use `#[wasm_bindgen(js_name = doThing)]` on functions and methods, and `js_name`/`js_class` on structs, so the JavaScript API reads like a normal JS API. Keep `snake_case` in your Rust source.
- **Prefer `&self`/`&mut self` methods over consuming `self`** unless you genuinely want to hand ownership to the caller's call. Consuming `self` invalidates the JS handle, which surprises JS callers.
- **Expose deterministic cleanup.** Document that long-lived exported objects should be `free()`d or wrapped in `using`. The `FinalizationRegistry` is a safety net, not a memory-management strategy.
- **Return owned data, not borrowed.** You cannot return a `&str` or `&[T]` that borrows from `self` across the boundary, because the JS side has no lifetime to anchor it to. Return `String`/`Vec<T>` (a copy) or expose a getter.
- **Use `Result<T, JsError>` for fallible exports** so JS callers get idiomatic `try`/`catch` instead of sentinel return values.
- **Let `wasm-pack` generate the `.d.ts`.** The generated TypeScript declarations are accurate and free; commit them or publish them with your npm package so TS consumers get full type-checking. See [Deploying WASM Apps](/19-wasm/10-deployment/) for packaging.
- **Keep the boundary coarse.** Each call across the JS↔WASM boundary has overhead (argument marshalling, memory copies). Prefer one call that does a lot of work over many small calls in a tight loop; see [WASM Performance](/19-wasm/09-performance/).

---

## Real-World Example

A text-analysis engine you might ship as an npm package: a stateful `TextStats` class plus a fallible parser. This is the complete crate, and every snippet here was compiled to `wasm32-unknown-unknown` and exercised from Node.

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

/// A reusable text-analysis engine, exported to JS as a class.
#[wasm_bindgen]
pub struct TextStats {
    text: String,
}

#[wasm_bindgen]
impl TextStats {
    #[wasm_bindgen(constructor)]
    pub fn new(text: String) -> TextStats {
        TextStats { text }
    }

    /// Read-only property: `stats.wordCount`.
    #[wasm_bindgen(getter, js_name = wordCount)]
    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }

    #[wasm_bindgen(getter, js_name = charCount)]
    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    /// Returns the N most frequent words as a JS array of strings.
    #[wasm_bindgen(js_name = topWords)]
    pub fn top_words(&self, n: usize) -> Vec<String> {
        use std::collections::HashMap;
        let mut counts: HashMap<&str, u32> = HashMap::new();
        for word in self.text.split_whitespace() {
            *counts.entry(word).or_insert(0) += 1;
        }
        let mut pairs: Vec<(&str, u32)> = counts.into_iter().collect();
        // Most frequent first; break ties alphabetically for stable output.
        pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        pairs
            .into_iter()
            .take(n)
            .map(|(w, _)| w.to_string())
            .collect()
    }
}

/// A free function that can fail; the `Err` becomes a thrown JS exception.
#[wasm_bindgen(js_name = parsePercentage)]
pub fn parse_percentage(input: &str) -> Result<f64, JsError> {
    let trimmed = input.trim().trim_end_matches('%');
    let value: f64 = trimmed
        .parse()
        .map_err(|_| JsError::new(&format!("not a number: {input:?}")))?;
    if !(0.0..=100.0).contains(&value) {
        return Err(JsError::new("percentage must be between 0 and 100"));
    }
    Ok(value / 100.0)
}
```

The generated `.d.ts` is the public TypeScript contract; note that each `///` doc comment becomes a JSDoc block on the corresponding declaration (members without a doc comment, such as the `constructor` and `charCount` here, simply get none), `usize` becomes `number`, and `Vec<String>` becomes `string[]`:

```typescript
// pkg/analytics.d.ts (generated)
/**
 * A reusable text-analysis engine, exported to JS as a class.
 */
export class TextStats {
    free(): void;
    [Symbol.dispose](): void;
    constructor(text: string);
    /**
     * Returns the N most frequent words as a JS array of strings.
     */
    topWords(n: number): string[];
    readonly charCount: number;
    /**
     * Read-only property: `stats.wordCount`.
     */
    readonly wordCount: number;
}

/**
 * A free function that can fail; the `Err` becomes a thrown JS exception.
 */
export function parsePercentage(input: string): number;
```

Consuming it from a Node program (built with `wasm-pack build --target nodejs`):

```javascript
// test.cjs
const wasm = require("./pkg/analytics.js");

const stats = new wasm.TextStats("the quick brown fox the lazy dog the fox");
console.log("wordCount:", stats.wordCount);
console.log("charCount:", stats.charCount);
console.log("topWords(2):", stats.topWords(2));
stats.free();

console.log("parsePercentage('42%'):", wasm.parsePercentage("42%"));
try {
  wasm.parsePercentage("nope");
} catch (e) {
  console.log("threw:", e.message);
}
```

The **real output** from running it:

```text
wordCount: 9
charCount: 40
topWords(2): [ 'the', 'fox' ]
parsePercentage('42%'): 0.42
threw: not a number: "nope"
```

Everything lines up with how the equivalent hand-written TypeScript class would behave — the only visible seam is the explicit `stats.free()`.

> **Tip:** The `nodejs` target instantiates the module synchronously at `require` time (its glue ends with `require('fs').readFileSync(...)` and a synchronous `new WebAssembly.Instance(...)`), so there is no `await init()`. The `web` and `bundler` targets are asynchronous. Pick the target that matches where the code runs; see [Setting Up wasm-pack](/19-wasm/01-wasm-pack/).

---

## Further Reading

- [The `wasm-bindgen` Guide — Exporting Rust to JS](https://rustwasm.github.io/wasm-bindgen/reference/attributes/on-rust-exports/index.html) — the authoritative reference for `constructor`, `getter`/`setter`, `js_name`, and `getter_with_clone`.
- [The `wasm-bindgen` Guide — Reference Types Crossing the Boundary](https://rustwasm.github.io/wasm-bindgen/reference/types.html) — which Rust types map to which JS types.
- [`FinalizationRegistry` on MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/FinalizationRegistry): the JS API behind automatic cleanup.
- [The TC39 `using` declaration / explicit resource management](https://github.com/tc39/proposal-explicit-resource-management): the basis for `[Symbol.dispose]`.

Related sections in this guide:

- [What is WebAssembly?](/19-wasm/00-wasm-intro/) — why compile Rust to WASM at all.
- [Setting Up wasm-pack](/19-wasm/01-wasm-pack/): `cdylib`, build targets, project layout.
- [Your First Rust to WASM Module](/19-wasm/02-first-wasm/): the minimal end-to-end build.
- [Calling JavaScript from Rust](/19-wasm/03-js-interop/): the opposite direction (`#[wasm_bindgen]` imports, `js_sys`).
- [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/) — `JsValue`, `serde-wasm-bindgen`, closures and callbacks.
- [Using Web APIs from Rust](/19-wasm/06-web-apis/) and [DOM Manipulation](/19-wasm/07-dom-manipulation/): driving the browser from Rust.
- [WASM Performance](/19-wasm/09-performance/) — the cost of the JS↔WASM boundary and bundle size.
- [Deploying WASM Apps](/19-wasm/10-deployment/): bundlers, MIME types, publishing to npm.
- Background concepts: [Ownership](/05-ownership/) (why `free()` exists), [Error Handling](/08-error-handling/) (`Result` → exceptions), and [Getting Started](/01-getting-started/) for toolchain setup.
- For the lower-level story of how Rust talks to other languages without `wasm-bindgen`, see [Unsafe & FFI](/20-unsafe-ffi/).

---

## Exercises

### Exercise 1: Export a counter class

**Difficulty:** Easy

**Objective:** Export a stateful struct as a JS class with a constructor, a mutating method, and a getter.

**Instructions:**

1. Create a `Counter` struct holding a single `i32` `count`.
2. Add a constructor that takes a starting value (`new Counter(start)`).
3. Add an `increment(&mut self, by: i32)` method.
4. Add a `value` getter returning the current count.
5. Make sure the getter is read as `counter.value` (no parentheses) from JS.

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Counter {
    count: i32,
}

#[wasm_bindgen]
impl Counter {
    // TODO: constructor, increment, value getter
}
```

<details>
<summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Counter {
    count: i32,
}

#[wasm_bindgen]
impl Counter {
    #[wasm_bindgen(constructor)]
    pub fn new(start: i32) -> Counter {
        Counter { count: start }
    }

    pub fn increment(&mut self, by: i32) {
        self.count += by;
    }

    #[wasm_bindgen(getter)]
    pub fn value(&self) -> i32 {
        self.count
    }
}
```

From JavaScript:

```javascript
const c = new Counter(10);
c.increment(5);
console.log(c.value); // 15
c.free();
```

This compiles cleanly for `wasm32-unknown-unknown` and the generated `.d.ts` declares `constructor(start: number)`, `increment(by: number): void`, and `readonly value: number`.

</details>

### Exercise 2: A fallible exported function

**Difficulty:** Medium

**Objective:** Export a function whose error path becomes a thrown JS exception.

**Instructions:**

1. Write `divide(a: f64, b: f64) -> Result<f64, JsError>`.
2. Return an `Err(JsError::new(...))` when `b == 0.0`, otherwise `Ok(a / b)`.
3. Confirm that from JS, `divide(10, 2)` returns `5` and `divide(10, 0)` throws an `Error` you can `catch`.
4. Rename it to `safeDivide` on the JS side.

<details>
<summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = safeDivide)]
pub fn safe_divide(a: f64, b: f64) -> Result<f64, JsError> {
    if b == 0.0 {
        return Err(JsError::new("division by zero"));
    }
    Ok(a / b)
}
```

From JavaScript:

```javascript
console.log(safeDivide(10, 2)); // 5
try {
  safeDivide(10, 0);
} catch (e) {
  console.log(e.message); // "division by zero"
}
```

The generated declaration is `export function safeDivide(a: number, b: number): number;`; the throwing behaviour is real at runtime but, as with any throwing TS function, not visible in the return type.

</details>

### Exercise 3: Avoid the use-after-move trap

**Difficulty:** Hard

**Objective:** Design an exported API that passes a struct between functions without invalidating the caller's JS handle.

**Instructions:**

1. Export a `Vector2 { x: f64, y: f64 }` struct (use `getter_with_clone` is not needed here since the fields are `f64`, which is `Copy`, but feel free to expose `x`/`y` as `pub`).
2. Add a method `add(&self, other: &Vector2) -> Vector2` that returns a new vector.
3. Importantly, take `other` by **reference** (`&Vector2`), not by value, so the caller's handle stays usable.
4. From JS, create two vectors `a` and `b`, compute `a.add(b)`, and confirm that `b` is still usable afterward (its `x` getter still works).

<details>
<summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Vector2 {
    pub x: f64,
    pub y: f64,
}

#[wasm_bindgen]
impl Vector2 {
    #[wasm_bindgen(constructor)]
    pub fn new(x: f64, y: f64) -> Vector2 {
        Vector2 { x, y }
    }

    // Borrow `other` so the caller's JS handle is NOT consumed.
    pub fn add(&self, other: &Vector2) -> Vector2 {
        Vector2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}
```

From JavaScript:

```javascript
const a = new Vector2(1, 2);
const b = new Vector2(3, 4);
const c = a.add(b);
console.log(c.x, c.y); // 4 6
console.log(b.x);      // 3 — b is still alive because `add` borrowed it
a.free();
b.free();
c.free();
```

If `add` had taken `other: Vector2` (by value), the call would have *moved* `b` into Rust and zeroed `b.__wbg_ptr`; the next `b.x` access would then throw `Error: null pointer passed to rust`. Taking `&Vector2` keeps `b` owned by JavaScript. The fields `x`/`y` are `f64` (which is `Copy`), so plain `pub` fields work without `getter_with_clone`.

</details>
