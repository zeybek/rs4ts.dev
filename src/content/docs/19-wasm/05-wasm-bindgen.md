---
title: "wasm-bindgen Deep Dive: Crossing the Rust/JavaScript Boundary"
description: "Which Rust types cross into JavaScript: strings, structs, Option, Result, closures, JsValue, and serde-wasm-bindgen, the core of the wasm-bindgen glue."
---

`wasm-bindgen` is the glue layer that makes Rust-in-the-browser feel like a normal JavaScript module. Raw WebAssembly can only pass a handful of numeric types back and forth; `wasm-bindgen` lets you exchange strings, structs, arrays, `Option`, errors that throw, and even closures you can register as event handlers. This page is the conceptual core of the section: exactly **which types can cross the boundary, how they are represented on each side, what `JsValue` is, when to reach for `serde-wasm-bindgen`, and how to hand a Rust closure to JavaScript without leaking memory or crashing**.

---

## Quick Overview

WebAssembly's own type system is tiny: a function can only take and return `i32`, `i64`, `f32`, and `f64`. Everything richer — a `String`, a `Uint8Array`, a callback — has to be *encoded* into those primitives and decoded on the other side. **`wasm-bindgen` generates that encode/decode glue automatically** from the `#[wasm_bindgen]` attribute, plus a `.js` shim and a `.d.ts` TypeScript declaration file so the module is fully typed when you `import` it. For a TypeScript developer the mental model is "a typed FFI compiler": you annotate Rust, and you get a JavaScript module whose types line up with the Rust ones, but only for the set of types `wasm-bindgen` knows how to translate. Anything outside that set travels as a `JsValue` (its `any`/`unknown` equivalent) or via `serde`.

---

## TypeScript/JavaScript Example

Today, when a TypeScript team wants a hot loop to run faster, they often ship it as a separately compiled module and call it across a boundary, exactly the situation `wasm-bindgen` automates. Here is the *shape* of that interaction written entirely in TypeScript: a typed module that exchanges primitives, objects, and a callback.

```typescript
// geometry.ts — a module we import and call. Note every value that crosses
// the call is either a primitive, a plain object, or a function.

export interface Point {
  x: number;
  y: number;
  label: string;
}

export function distanceFromOrigin(p: Point): number {
  return Math.sqrt(p.x * p.x + p.y * p.y);
}

export function parseAge(input: string): number {
  const n = Number(input.trim());
  if (!Number.isInteger(n) || n < 0) {
    throw new Error(`invalid age: ${input}`);
  }
  return n;
}

// Register a callback that JavaScript will invoke later.
export function startTicker(onTick: (count: number) => void): number {
  let count = 0;
  return setInterval(() => onTick(++count), 1000);
}
```

```typescript
// caller.ts
import { distanceFromOrigin, parseAge, startTicker } from "./geometry";

console.log(distanceFromOrigin({ x: 3, y: 4, label: "p" })); // 5
console.log(parseAge("  42 "));                                // 42
const id = startTicker((n) => console.log(`tick ${n}`));       // tick 1, tick 2, ...
```

Three things matter here, because each maps onto a different `wasm-bindgen` mechanism: a *plain object* crosses by value (`Point`), an *error* propagates by throwing (`parseAge`), and a *callback* is handed over and stored to be called later (`startTicker`). Crossing into Rust, each needs explicit handling.

---

## Rust Equivalent

Create a library crate (WASM modules are libraries, not binaries) and add the boundary crates. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically.

```bash
cargo new --lib geometry
cd geometry
cargo add wasm-bindgen
cargo add serde --features derive
cargo add serde-wasm-bindgen
```

A WASM library must be built as a `cdylib`, so set the crate type in `Cargo.toml` (this is covered in detail in [Setting Up wasm-pack](/19-wasm/01-wasm-pack/)):

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2.122"
serde = { version = "1.0.228", features = ["derive"] }
serde-wasm-bindgen = "0.6.5"
```

Now `src/lib.rs`, the same three interactions, idiomatic and compile-verified against `wasm32-unknown-unknown`:

```rust
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

// A rich struct travels via serde-wasm-bindgen as a plain JS object.
#[derive(Serialize, Deserialize)]
pub struct Point {
    x: f64,
    y: f64,
    label: String,
}

#[wasm_bindgen]
pub fn distance_from_origin(value: JsValue) -> Result<f64, JsValue> {
    // Deserialize the incoming JS object into our Rust struct.
    let p: Point = serde_wasm_bindgen::from_value(value)?;
    Ok((p.x * p.x + p.y * p.y).sqrt())
}

// Result<T, JsValue>: Ok(t) returns t to JS, Err(e) becomes a thrown exception.
#[wasm_bindgen]
pub fn parse_age(s: &str) -> Result<u32, JsValue> {
    s.trim()
        .parse::<u32>()
        .map_err(|e| JsValue::from_str(&format!("invalid age: {e}")))
}

// Import the JS timer and console functions we need.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    fn setInterval(cb: &Closure<dyn FnMut()>, ms: f64) -> f64;
}

// The Closure must outlive the timer, so the Ticker owns it.
#[wasm_bindgen]
pub struct Ticker {
    _closure: Closure<dyn FnMut()>,
    id: f64,
}

#[wasm_bindgen]
impl Ticker {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Ticker {
        let mut count = 0u32;
        let closure = Closure::new(move || {
            count += 1;
            log(&format!("tick {count}"));
        });
        let id = setInterval(&closure, 1000.0);
        Ticker { _closure: closure, id }
    }

    #[wasm_bindgen(getter)]
    pub fn id(&self) -> f64 {
        self.id
    }
}
```

Building this against the WASM target compiles cleanly:

```text
$ cargo build --target wasm32-unknown-unknown
   Compiling geometry v0.1.0 (/.../geometry)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.13s
```

> **Note:** Plain `cargo build` type-checks the WASM code on your host platform too, but only the `wasm32-unknown-unknown` target produces a real `.wasm`. In practice you run `wasm-pack build` (see [Setting Up wasm-pack](/19-wasm/01-wasm-pack/)), which compiles *and* runs the `wasm-bindgen` CLI to emit the `.js` + `.d.ts` glue in one step.

---

## Detailed Explanation

### What `wasm-bindgen` actually does

The `#[wasm_bindgen]` attribute is a procedural macro. For every exported function it generates a second, *ABI-flattened* function whose arguments are only the WASM-native numeric types, plus the encode/decode logic. A `&str` argument, for example, is not passed as a "string"; there is no such WASM type. Instead the generated JavaScript copies the string's UTF-8 bytes into the WASM module's linear memory, passes a `(pointer, length)` pair of `i32`s, and the Rust side reconstructs a `&str` view over those bytes. The `.d.ts` file it emits still *says* `string`, so the boundary is fully typed from TypeScript's perspective; the byte-copying is invisible.

This is the first big conceptual difference from a TypeScript module: **a function call across the boundary is not free**. Primitives (`number`, `boolean`) pass as a single register and are essentially zero-cost, but every string, array, or object involves copying bytes into or out of WASM memory. The performance consequences of that are the subject of [WebAssembly Performance](/19-wasm/09-performance/); here the point is simply that the *type* determines the *cost*.

### The type translation table

`wasm-bindgen` knows how to translate a fixed, well-defined set of Rust types. The most important ones:

| Rust type | JavaScript / TypeScript type | How it crosses |
|---|---|---|
| `i8`..`i32`, `u8`..`u32`, `f32`, `f64` | `number` | by value (one register) |
| `i64` / `u64` / `i128` / `u128` | `bigint` | by value; **not** `number` |
| `bool` | `boolean` | by value |
| `char` | `string` (length 1) | copied |
| `String` / `&str` | `string` | UTF-8 bytes copied |
| `Vec<u8>` / `&[u8]` | `Uint8Array` | bytes copied |
| `Vec<i32>`, `Vec<f64>`, … | `Int32Array`, `Float64Array`, … | copied |
| `Vec<String>` | `string[]` | copied |
| `Option<T>` | `T \| undefined` | sentinel value |
| `Result<T, JsValue>` | `T` or a thrown exception | see below |
| `#[wasm_bindgen] struct` | a class instance (opaque handle) | by reference (pointer) |
| `JsValue` | any JS value | by reference (handle) |

Two rows surprise TypeScript developers the most. First, `u64`/`i64` map to `bigint`, **not** `number`, because a JavaScript `number` is always an IEEE-754 `f64` and silently loses precision above 2^53, whereas Rust's 64-bit integers are exact. `wasm-bindgen` refuses to lie about that and uses `bigint`. Second, a `#[wasm_bindgen]` struct does **not** cross as a plain object the way a TypeScript class instance would when structured-cloned; it stays *inside* WASM memory and JavaScript receives an opaque handle (a small class wrapping a pointer). We unpack that distinction in [Key Differences](#key-differences).

### `Option` and `Result`

`Option<T>` becomes `T | undefined`: `Some(v)` is `v`, `None` is `undefined`. This is the cleanest analogue to TypeScript's optional values, and it is what you usually want instead of inventing a null-object.

`Result<T, JsValue>` is how you raise an exception in JavaScript from Rust. `Ok(t)` returns `t` normally; `Err(e)` is *thrown*. The JavaScript caller sees `try { ... } catch (e) { ... }` semantics, with `e` being whatever `JsValue` you put in the `Err`. In `parse_age`, a parse failure becomes `JsValue::from_str("invalid age: ...")`, which JavaScript receives as a thrown string. (To throw a real `Error` object instead of a bare string, construct a `js_sys::Error`, see [Calling JavaScript from Rust](/19-wasm/03-js-interop/).)

### `JsValue`: the `any`/`unknown` of the boundary

`JsValue` is an opaque handle to *any* JavaScript value living on the JS side of the heap: a number, an object, a function, a DOM node, anything. Rust cannot inspect its fields directly; it can only call the conversion helpers (`JsValue::from_str`, `as_f64()`, `as_bool()`, `is_null()`, …) or hand it to `js-sys` / `web-sys` accessors. Think of it as TypeScript's `unknown`: you hold it safely, but you must narrow it before you can do anything specific. When a function signature says `JsValue`, `wasm-bindgen` performs no copying and no validation — it just passes the handle through, which is exactly what `echo` below does:

```rust
use wasm_bindgen::prelude::*;

// The identity function over arbitrary JS values — no copy, no inspection.
#[wasm_bindgen]
pub fn echo(input: JsValue) -> JsValue {
    input
}
```

### `serde-wasm-bindgen`: rich structs without writing glue

When you want to pass a *structured* value (an object with named fields, nested arrays, enums) you have two options. You can annotate the struct with `#[wasm_bindgen]` and expose getters/setters one field at a time; verbose, and it produces an opaque handle, not a plain object. Or, far more often, you derive `serde::{Serialize, Deserialize}` and use **`serde-wasm-bindgen`** to convert between your Rust type and a real JavaScript object in one call:

- `serde_wasm_bindgen::to_value(&rust_value)` → `JsValue` (a plain JS object/array)
- `serde_wasm_bindgen::from_value::<T>(js_value)` → `Result<T, Error>`

That is exactly the `Point` round-trip above. The incoming `{ x, y, label }` object is deserialized into a Rust `Point`; the result is computed in Rust. This is the WASM-boundary cousin of the Serde JSON workflow you already know from [Section 15](/15-serialization/01-serde-basics/) — same derives, same mental model — except the target is a live JS object graph rather than a JSON string, so no text parsing happens.

> **Tip:** `serde-wasm-bindgen` replaced the old `JsValue::from_serde` / `into_serde` methods that lived behind `wasm-bindgen`'s `serde-serialize` feature. Those are deprecated; reach for the `serde-wasm-bindgen` crate, which is faster (it builds JS objects directly instead of going through a JSON string) and correctly handles maps, `u64`, and byte arrays.

### Closures: the hardest part of the boundary

A Rust closure cannot be handed to JavaScript directly, because JavaScript needs a callable function object and Rust needs to keep the closure's captured environment alive. `wasm_bindgen::closure::Closure` bridges the two: `Closure::new(f)` allocates a JS function that, when called, jumps back into the Rust closure. The catch — and this is where almost every newcomer trips — is **ownership and lifetime**:

- The JS function is valid only while the `Closure` is alive in Rust. If the `Closure` is dropped, the JS function becomes dangling, and calling it throws `closure invoked recursively or after being dropped`.
- A `Closure` that is registered for *repeated* calls (an interval, an event listener) must therefore be *stored* somewhere that outlives those calls. In `Ticker`, the struct field `_closure` holds it for as long as the `Ticker` exists.
- `Closure::new` accepts `FnMut`/`Fn` closures for repeated calls; `Closure::once` is for a callback that fires exactly once and then frees itself.

The deliberately-ugly `_closure` field (leading underscore to silence the "unused" warning) is idiomatic: it documents that the field exists purely to keep the callback alive. The alternative — `closure.forget()` — leaks the closure on purpose so it lives for the whole program; convenient for a one-time global handler, a permanent leak for anything created repeatedly.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust + wasm-bindgen |
|---|---|---|
| Passing an object | Reference to a heap object; same identity on both sides | Either a *copy* into a plain JS object (`serde-wasm-bindgen`) or an *opaque handle* to data inside WASM memory (`#[wasm_bindgen] struct`) |
| 64-bit integers | `number` (f64), loses precision past 2^53 | `i64`/`u64` ↔ `bigint`, always exact |
| Throwing | `throw new Error(...)` anywhere | only via `Err(JsValue)` from a function returning `Result<_, JsValue>` |
| "any" value | `any` / `unknown` | `JsValue` (must narrow with `.as_f64()`, `js-sys`, etc.) |
| Closures / callbacks | First-class functions, GC'd automatically | `Closure<...>`; you must keep it alive and decide `new` vs `once` vs `forget` |
| Cost of a call | Uniform; objects shared by reference | Primitives ~free; strings/arrays/objects copy bytes across the boundary |
| Type checking of inputs | `as Point` is a no-op at runtime | `serde_wasm_bindgen::from_value` *actually validates* the shape and errors on mismatch |

The deepest difference is the **opaque-handle vs plain-object** choice for structs. A `#[wasm_bindgen] struct` keeps its data inside Rust: JavaScript gets a class with methods, the data never leaves WASM memory, and there is no per-access copy: ideal for a long-lived stateful object (a parser, a game world, a database connection). A `serde-wasm-bindgen` object is the opposite: a one-time deep copy into an ordinary JS object that JavaScript fully owns and can mutate freely, ideal for data you compute once and hand off. Choosing the wrong one is the most common design mistake; the rule of thumb is *handle for behavior + long-lived state, serialize for plain data*.

> **Note:** Unlike a TypeScript class, an opaque `#[wasm_bindgen]` struct is **not garbage collected**. JavaScript holds a pointer into WASM memory, and you must call `.free()` on it (the generated class exposes one) when done, or use it via wasm-pack's optional `WeakRef`-based finalization. We return to this in [Common Pitfalls](#common-pitfalls).

---

## Common Pitfalls

### Pitfall 1: Returning a borrowed reference

You cannot return a `&str`, `&[T]`, or any borrow from a `#[wasm_bindgen]` function: the boundary has to *own* what it sends, because the value is copied into JS memory and the Rust stack frame is gone afterward. Return an owned `String` / `Vec<T>` instead.

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn label() -> &'static str { // does not compile
    "hello"
}
```

Real compiler output:

```text
error: cannot return a borrowed ref with #[wasm_bindgen]
 --> src/lib.rs:4:19
  |
4 | pub fn label() -> &'static str {
  |                   ^^^^^^^^^^^^
```

The fix is to return `String`: `pub fn label() -> String { "hello".to_string() }`.

### Pitfall 2: Exposing an unsupported type

`wasm-bindgen` only knows the translation table. Hand it a `HashMap` (or any type that does not implement its `IntoWasmAbi`/`WasmDescribe` traits) directly and it cannot generate glue:

```rust
use wasm_bindgen::prelude::*;
use std::collections::HashMap;

#[wasm_bindgen]
pub fn build_map() -> HashMap<String, i32> { // does not compile
    let mut m = HashMap::new();
    m.insert("a".to_string(), 1);
    m
}
```

The real error names the missing trait:

```text
error[E0277]: the trait bound `HashMap<String, i32>: IntoWasmAbi` is not satisfied
 --> src/lib.rs:4:1
  |
4 | #[wasm_bindgen]
  | ^^^^^^^^^^^^^^^ the trait `IntoWasmAbi` is not implemented for `HashMap<String, i32>`
  |
  = note: required for `HashMap<String, i32>` to implement `ReturnWasmAbi`
```

The fix: serialize it. Return `Result<JsValue, JsValue>` and use `serde_wasm_bindgen::to_value(&m)`. A `HashMap` serializes to an ES2015 `Map` by default (or to a plain JS object if you build it with `serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true)`, or the preset `Serializer::json_compatible()`, and call `m.serialize(&serializer)`).

### Pitfall 3: Passing a bare closure where a `Closure` is required

An imported JS function that takes a callback expects a `&Closure<...>`, not a raw Rust closure. Pass a bare `|| {}` and the types do not line up:

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    fn jsTakesCallback(cb: &Closure<dyn FnMut()>);
}

#[wasm_bindgen]
pub fn try_pass() {
    jsTakesCallback(|| {}); // does not compile
}
```

Real error:

```text
error[E0308]: mismatched types
  --> src/lib.rs:10:21
   |
10 |     jsTakesCallback(|| {});
   |     --------------- ^^^^^ expected `&ScopedClosure<'_, dyn FnMut()>`, found closure
   |     |
   |     arguments to this function are incorrect
   |
   = note: expected reference `&ScopedClosure<'static, (dyn FnMut() + 'static)>`
                found closure `{closure@src/lib.rs:10:21: 10:23}`
```

Wrap it: `let cb = Closure::<dyn FnMut()>::new(|| {}); jsTakesCallback(&cb);`, and remember to keep `cb` alive (Pitfall 4).

### Pitfall 4: A dropped `Closure` (runtime, not compile-time)

This one *compiles* and then fails at runtime, which makes it nasty. If you create a `Closure`, register it with `setInterval`/`addEventListener`, and let it go out of scope, the closure is freed while JavaScript still holds the function. The next invocation throws: the real message wasm-bindgen emits is `closure invoked recursively or after being dropped`. The cure is structural, not a compiler hint: store the `Closure` in a field that outlives the callback (as `Ticker` does), or call `.forget()` to intentionally leak it for the program's lifetime. There is no E-code here because Rust's type system cannot see across the boundary into JavaScript's reference; this is a discipline you adopt, the same way you remember to `clearInterval` in JavaScript.

### Pitfall 5: Forgetting that opaque structs are not garbage-collected

`const c = new Counter(0)` in JavaScript holds a pointer into WASM memory. Dropping the JS variable does **not** free the Rust value. Long-lived apps that create many short-lived handles and never call `.free()` will grow WASM memory until the tab dies. Either call the generated `c.free()` when done, or prefer `serde-wasm-bindgen` plain objects for throwaway data so the JS garbage collector handles them.

---

## Best Practices

- **Prefer the richest *supported* type over `JsValue`.** A signature of `fn(p: JsValue)` throws away all type information; `fn(x: f64, y: f64)` or a serde round-trip keeps the `.d.ts` honest. Use `JsValue` only for genuinely dynamic values.
- **Use `serde-wasm-bindgen` for data, `#[wasm_bindgen] struct` for behavior.** Plain records and DTOs → serialize. Stateful objects with methods and a lifetime → opaque handle.
- **Return `Result<T, JsValue>` for anything fallible** so JavaScript gets idiomatic `try/catch`. Construct a `js_sys::Error` (not a bare string) when you want a real `Error` with a stack trace.
- **Make `Closure` ownership explicit.** Store repeated-call closures in a struct field; use `Closure::once` for fire-once callbacks; reserve `.forget()` for genuinely program-lifetime handlers and comment *why* you are leaking.
- **Let `u64`/`i64` be `bigint`.** Do not cast to `f64` to "make it a `number`": you reintroduce the precision bug `wasm-bindgen` was protecting you from. (See [Section 02 types](/02-basics/01-types/) for why JS `number` cannot hold a 64-bit integer exactly.)
- **Keep boundary crossings coarse.** Because strings and objects copy, design APIs that exchange one big batch rather than many tiny calls in a loop ([WebAssembly Performance](/19-wasm/09-performance/)).
- **Enable `console_error_panic_hook` in debug builds** so a Rust panic shows a readable stack trace in the browser console instead of `unreachable executed`.

---

## Real-World Example

A production-flavored module: parse a batch of CSV rows that arrive from JavaScript as a single string, validate each into a typed record with Serde, and return both the parsed records (as a plain JS array) and a progress callback that JavaScript can drive. This exercises every boundary mechanism: `&str` in, a serde round-trip out, `Result`/throw, and a `Closure`.

```rust
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Sale {
    product: String,
    units: u32,
    revenue_cents: u64, // via serde-wasm-bindgen: small values cross as a JS number; large ones need the bigint serializer option
}

// Parse "product,units,revenue" lines. On a malformed row, throw with context.
#[wasm_bindgen]
pub fn parse_sales(csv: &str) -> Result<JsValue, JsValue> {
    let mut sales: Vec<Sale> = Vec::new();

    for (i, line) in csv.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(',');
        let row = (parts.next(), parts.next(), parts.next());

        let sale = match row {
            (Some(p), Some(u), Some(r)) => Sale {
                product: p.trim().to_string(),
                units: u.trim().parse().map_err(|_| {
                    JsValue::from_str(&format!("row {}: bad units `{u}`", i + 1))
                })?,
                revenue_cents: r.trim().parse().map_err(|_| {
                    JsValue::from_str(&format!("row {}: bad revenue `{r}`", i + 1))
                })?,
            },
            _ => {
                return Err(JsValue::from_str(&format!(
                    "row {}: expected 3 columns",
                    i + 1
                )))
            }
        };
        sales.push(sale);
    }

    // Vec<Sale> -> a real JS array of plain objects.
    serde_wasm_bindgen::to_value(&sales).map_err(|e| e.into())
}

// Sum revenue, reporting progress through a JS callback. The callback is
// borrowed for the duration of the call, so no long-lived storage is needed.
#[wasm_bindgen]
pub fn total_revenue(csv: &str, on_progress: &js_sys::Function) -> Result<f64, JsValue> {
    let sales: Vec<Sale> = serde_wasm_bindgen::from_value(parse_sales(csv)?)?;
    let total = sales.len() as f64;
    let mut sum: u64 = 0;

    for (i, sale) in sales.iter().enumerate() {
        sum += sale.revenue_cents;
        // Invoke the JS callback with a fraction 0.0..=1.0.
        let progress = JsValue::from_f64((i + 1) as f64 / total);
        on_progress.call1(&JsValue::NULL, &progress)?;
    }

    Ok(sum as f64 / 100.0) // cents -> dollars for display
}
```

This whole module compiles cleanly against `wasm32-unknown-unknown`. Note three deliberate choices: `revenue_cents` is `u64` so money is exact in Rust — though, because it goes out through `serde_wasm_bindgen::to_value` (not a direct `#[wasm_bindgen]` signature), it lands in JavaScript as a plain `number` for values within `Number.MAX_SAFE_INTEGER`; to force `bigint` you would serialize with `Serializer::new().serialize_large_number_types_as_bigints(true)`. `parse_sales` returns `Result<JsValue, JsValue>` so a bad row *throws* with a row number; and `total_revenue` takes the callback as a borrowed `&js_sys::Function` rather than a `Closure`, because here JavaScript *owns* the function and Rust only calls it synchronously during the request, no lifetime management required. From TypeScript the generated module is used like any other:

```typescript
import init, { parse_sales, total_revenue } from "./pkg/geometry";

await init(); // load + instantiate the .wasm (see ./wasm-pack.md and ./rust-from-js.md)

const csv = "widget, 3, 1999\ngadget, 1, 4950";
const sales = parse_sales(csv) as { product: string; units: number; revenue_cents: number }[];
console.log(sales); // [{ product: 'widget', units: 3, revenue_cents: 1999 }, ...]

const dollars = total_revenue(csv, (p: number) => console.log(`${Math.round(p * 100)}%`));
console.log(`total: $${dollars}`); // 50%, 100%, total: $69.49
```

---

## Further Reading

- [The `wasm-bindgen` Guide](https://rustwasm.github.io/docs/wasm-bindgen/) — the official, authoritative reference for every attribute and supported type.
- [Supported Rust types reference](https://rustwasm.github.io/docs/wasm-bindgen/reference/types.html) — the full translation table.
- [`Closure` documentation](https://docs.rs/wasm-bindgen/latest/wasm_bindgen/closure/struct.Closure.html) — `new` vs `once` vs `forget`, and lifetime rules.
- [`serde-wasm-bindgen` on docs.rs](https://docs.rs/serde-wasm-bindgen/) — `to_value` / `from_value`, the `Serializer` options.
- [`js-sys` documentation](https://docs.rs/js-sys/) — `js_sys::Function`, `js_sys::Error`, and the rest of the JS standard library from Rust.
- Section cross-links: [What Is WebAssembly and Why Compile Rust to It?](/19-wasm/00-wasm-intro/) · [Setting Up wasm-pack](/19-wasm/01-wasm-pack/) · [Your First Rust → WebAssembly Module](/19-wasm/02-first-wasm/) · [Calling JavaScript from Rust](/19-wasm/03-js-interop/) · [Calling Rust from JavaScript](/19-wasm/04-rust-from-js/) · [Using Web APIs from Rust with web-sys](/19-wasm/06-web-apis/) · [WebAssembly Performance](/19-wasm/09-performance/)
- Foundations: [Section 15 — Serde basics](/15-serialization/01-serde-basics/) (the derive model reused here) · [Section 02 — Types](/02-basics/01-types/) (why `number` is f64) · [the low-level FFI cousin of this boundary](/20-unsafe-ffi/).

---

## Exercises

### Exercise 1: Owned return types

**Difficulty:** Beginner

**Objective:** Internalize why the boundary must own what it returns.

**Instructions:** Write a `#[wasm_bindgen]` function `initials(full_name: &str) -> String` that returns the uppercase first letter of each whitespace-separated word (so `"ada lovelace"` → `"AL"`). Confirm it compiles against `wasm32-unknown-unknown`. Then change the return type to `&str` and observe the compiler error; explain in one sentence why owning the result is mandatory here.

<details><summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn initials(full_name: &str) -> String {
    full_name
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .flat_map(|c| c.to_uppercase())
        .collect()
}
```

This compiles cleanly. Returning `&str` instead fails with `error: cannot return a borrowed ref with #[wasm_bindgen]` because the function builds a *new* string that no longer exists once the call returns — the boundary copies the owned `String`'s bytes into JS memory, and there is nothing for a borrow to point at afterward.

</details>

### Exercise 2: serde round-trip with validation

**Difficulty:** Intermediate

**Objective:** Pass a structured object across the boundary and validate it, contrasting with TypeScript's unchecked `as`.

**Instructions:** Define a `#[derive(Serialize, Deserialize)]` struct `User { name: String, age: u32 }`. Write `fn make_adult(user: JsValue) -> Result<JsValue, JsValue>` that deserializes the incoming object, returns an error (thrown to JS) if `age < 18`, otherwise bumps a new `is_adult: true`-style flag by returning a *new* struct `Profile { name: String, age: u32, adult: bool }` serialized back to a `JsValue`. Compile-verify it.

<details><summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct User {
    name: String,
    age: u32,
}

#[derive(Serialize, Deserialize)]
pub struct Profile {
    name: String,
    age: u32,
    adult: bool,
}

#[wasm_bindgen]
pub fn make_adult(user: JsValue) -> Result<JsValue, JsValue> {
    let user: User = serde_wasm_bindgen::from_value(user)?;
    if user.age < 18 {
        return Err(JsValue::from_str(&format!(
            "{} is under 18 (age {})",
            user.name, user.age
        )));
    }
    let profile = Profile {
        name: user.name,
        age: user.age,
        adult: true,
    };
    serde_wasm_bindgen::to_value(&profile).map_err(|e| e.into())
}
```

`serde_wasm_bindgen::from_value` *actually checks* that the incoming object has a string `name` and a numeric `age`; a missing or wrong-typed field returns `Err` (thrown to JavaScript), unlike TypeScript's `user as User`, which is erased at runtime and would let a malformed object through.

</details>

### Exercise 3: A self-storing event closure

**Difficulty:** Advanced

**Objective:** Hand a long-lived Rust closure to JavaScript without it being dropped, and free it deliberately.

**Instructions:** Import the JS functions `addClick(cb: &Closure<dyn FnMut()>)` and `removeClick(cb: &Closure<dyn FnMut()>)`. Build a `#[wasm_bindgen] struct ClickCounter` whose constructor registers a click handler that increments an internal count (use `Rc<Cell<u32>>` so the closure and the struct can share it), stores the `Closure` in a field, and exposes a `count(&self) -> u32` getter plus a `stop(self)` method that unregisters the handler. Explain why the `Closure` must be stored. Compile-verify.

<details><summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

#[wasm_bindgen]
extern "C" {
    fn addClick(cb: &Closure<dyn FnMut()>);
    fn removeClick(cb: &Closure<dyn FnMut()>);
}

#[wasm_bindgen]
pub struct ClickCounter {
    count: Rc<Cell<u32>>,
    handler: Closure<dyn FnMut()>,
}

#[wasm_bindgen]
impl ClickCounter {
    #[wasm_bindgen(constructor)]
    pub fn new() -> ClickCounter {
        let count = Rc::new(Cell::new(0u32));
        let count_for_cb = Rc::clone(&count);
        let handler = Closure::new(move || {
            count_for_cb.set(count_for_cb.get() + 1);
        });
        addClick(&handler);
        ClickCounter { count, handler }
    }

    #[wasm_bindgen(getter)]
    pub fn count(&self) -> u32 {
        self.count.get()
    }

    pub fn stop(self) {
        removeClick(&self.handler);
        // `self` (and its Closure) is dropped here, freeing the JS function.
    }
}
```

The `Closure` is stored in `handler` because JavaScript keeps calling it on every click. If it were a local variable in `new`, it would be dropped when the constructor returned, and the next click would throw `closure invoked recursively or after being dropped`. `Rc<Cell<u32>>` lets the closure and the struct share one mutable counter (WASM is single-threaded, so `Rc`/`Cell` are appropriate — no `Arc`/`Mutex` needed). `stop(self)` takes ownership, removes the listener, and lets the `Closure` drop cleanly.

</details>
