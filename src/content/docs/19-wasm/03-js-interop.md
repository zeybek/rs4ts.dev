---
title: "Calling JavaScript from Rust"
description: "Call JavaScript from Rust in Wasm: declare imports with wasm-bindgen extern blocks, reach built-ins via js-sys, and turn JS throws into a Rust Result with catch."
---

When you compile Rust to WebAssembly, the Rust code lands in a sandbox that has no DOM, no `fetch`, no `console`, and no `localStorage` of its own. Everything outside the linear-memory sandbox lives in JavaScript. This page is about the direction most tutorials skip: declaring the JavaScript functions, classes, and built-ins your Rust code wants to **call**, so that Rust can reach back out into the host.

---

## Quick Overview

A WebAssembly module is a pure function machine: it imports a list of functions from its host and exports a list of functions back. **`wasm-bindgen`** is the tool that makes those imports ergonomic in Rust. Instead of hand-writing raw `extern` declarations against untyped numbers, you write a normal-looking `extern "C"` block annotated with `#[wasm_bindgen]`, and the macro generates the glue that marshals strings, objects, and callbacks across the boundary. You point `#[wasm_bindgen(module = "...")]` at a JavaScript file (or an npm package) to import its named exports, and you use the **`js-sys`** crate for the JavaScript built-ins that exist in every runtime (`Array`, `Object`, `Math`, `Date`, `JSON`, `Reflect`, `Promise`). For a TypeScript developer the mental model is a `.d.ts` file: you are writing type declarations for JavaScript that already exists, and the bundler wires the real implementation in at load time.

---

## TypeScript/JavaScript Example

Imagine a small front-end utility module. It formats currency using the platform's `Intl` API and answers whether a given date falls on a weekend. Both things are trivial in JavaScript because the runtime hands you `Intl` and `Date` for free:

```typescript
// js/format.ts
export function formatPrice(cents: number, currency: string): string {
  return (cents / 100).toLocaleString("en-US", {
    style: "currency",
    currency,
  });
}

export function isWeekend(date: Date): boolean {
  const day = date.getDay();
  return day === 0 || day === 6; // Sunday or Saturday
}
```

```typescript
// caller.ts
import { formatPrice, isWeekend } from "./format";

console.log(formatPrice(123456, "USD")); // "$1,234.56"
console.log(formatPrice(99, "USD")); // "$0.99"
console.log(`isWeekend(Sat 2026-06-06): ${isWeekend(new Date("2026-06-06"))}`); // a Saturday
console.log(`isWeekend(Mon 2026-06-01): ${isWeekend(new Date("2026-06-01"))}`); // a Monday
```

Running the JavaScript under Node v22 produces exactly:

```text
$1,234.56
$0.99
isWeekend(Sat 2026-06-06): true
isWeekend(Mon 2026-06-01): false
```

The interesting part is *why* this is hard from WebAssembly. `Intl.NumberFormat` (the engine behind `toLocaleString`) and the `Date` object are JavaScript host facilities. A `.wasm` module cannot reimplement locale-aware currency formatting cheaply, and it has no clock of its own. So rather than port these to Rust, we **import** them: keep the JavaScript implementation, and declare it to Rust.

---

## Rust Equivalent

Create a library crate configured to build a `cdylib` (covered in [wasm-pack](/19-wasm/01-wasm-pack/)), then add the two crates that power JavaScript interop. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition, and `cargo new` selects that edition automatically:

```bash
cargo new --lib js-interop-demo
cd js-interop-demo
cargo add wasm-bindgen js-sys
```

That resolves to the current stable releases and writes them into `Cargo.toml`. Add the `crate-type` so the linker emits a `.wasm`:

```toml
[package]
name = "js-interop-demo"
version = "0.1.0"
edition = "2024"

[dependencies]
js-sys = "0.3.99"
wasm-bindgen = "0.2.122"

[lib]
crate-type = ["cdylib", "rlib"]
```

Keep the JavaScript file next to your Rust source (say `js/format.js`), exporting the same two functions as above:

```javascript
// js/format.js
export function formatPrice(cents, currency) {
  return (Number(cents) / 100).toLocaleString("en-US", {
    style: "currency",
    currency,
  });
}

export function isWeekend(date) {
  const day = date.getDay();
  return day === 0 || day === 6;
}
```

Now declare those exports to Rust and call them. The `module = "..."` path is **relative to the file containing the attribute**:

```rust
use wasm_bindgen::prelude::*;

// Import the named exports of a JS ES module that ships with this crate.
// The path is relative to this source file; wasm-bindgen reads it at build time.
#[wasm_bindgen(module = "/js/format.js")]
extern "C" {
    // JS: export function formatPrice(cents, currency)
    #[wasm_bindgen(js_name = formatPrice)]
    fn format_price(cents: i64, currency: &str) -> String;

    // JS: export function isWeekend(date)
    #[wasm_bindgen(js_name = isWeekend)]
    fn is_weekend(date: &js_sys::Date) -> bool;
}

#[wasm_bindgen]
pub fn price_label(cents: i64) -> String {
    // Calling JS from Rust looks like an ordinary function call.
    format_price(cents, "USD")
}

#[wasm_bindgen]
pub fn weekend_today() -> bool {
    // js_sys::Date::new_0() === `new Date()` in JS: the host's clock.
    let now = js_sys::Date::new_0();
    is_weekend(&now)
}
```

Building this for the WebAssembly target compiles cleanly:

```text
$ cargo build --target wasm32-unknown-unknown
   Compiling js-interop-demo v0.1.0 (/.../js-interop-demo)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.41s
```

You did not have to write any byte-shuffling code. `wasm-bindgen` saw `currency: &str` and generated the glue to copy the Rust string into the JavaScript heap, and it saw `-> String` and generated the glue to copy the JavaScript return value back into Rust's linear memory.

---

## Detailed Explanation

### The `extern "C"` block is a declaration, not a definition

In plain Rust, an `extern "C"` block declares functions whose bodies live in another language (we cover the C side in [Unsafe & FFI](/20-unsafe-ffi/)). `wasm-bindgen` reuses that syntax, but the "other language" is JavaScript and the macro generates the marshalling layer. The body of `format_price` does not exist in Rust; it is a *promise* that, by the time the module runs, the host will have supplied a function under this import name.

This is the exact inverse of a `.d.ts` file in TypeScript. There you write `export function formatPrice(cents: number, currency: string): string;` to *describe* JavaScript that exists. Here the `extern` block is your `.d.ts`, written in Rust, and the `#[wasm_bindgen]` macro turns it into real, type-checked calls.

### `#[wasm_bindgen(module = "...")]` chooses where the import comes from

The attribute on the block decides how the host resolves the import:

- `module = "/js/format.js"`: a **local module** that ships with your crate. `wasm-bindgen` reads this file at build time (so it must exist) and re-exports it from the generated glue. The leading `/` means "relative to the crate root"; a bare name like `"./format.js"` is relative to the current file.
- `module = "lodash"`: a **bare specifier**, resolved by your bundler (Vite, webpack) the same way a normal `import ... from "lodash"` would be. This is how you call functions from npm packages.
- *No `module` attribute at all*: the import is expected in the **global scope**, e.g. `window.alert` or anything attached to `globalThis`.

### `js_name` and namespaces bridge naming conventions

Rust style is `snake_case`; JavaScript style is `camelCase`. `#[wasm_bindgen(js_name = formatPrice)]` lets you keep an idiomatic Rust name (`format_price`) while binding to the real export `formatPrice`. For functions that live under an object, `js_namespace` walks the path:

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    // No `module`: these are global. `alert` is window.alert.
    fn alert(msg: &str);

    // Binds Rust `console_log` to the global `console.log`.
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log(msg: &str);
}

#[wasm_bindgen]
pub fn greet(name: &str) {
    console_log(&format!("Hello from Rust, {name}!"));
    alert(&format!("Hi {name}"));
}
```

### `js-sys`: the JavaScript standard library, typed for Rust

You do not need a `.js` file to reach JavaScript *built-ins* — those exist in every runtime, so `js-sys` ships ready-made bindings for them. `js_sys::Date::new_0()` is `new Date()`; `js_sys::Math::random()` is `Math.random()`; `js_sys::Array`, `js_sys::Object`, `js_sys::JSON`, and `js_sys::Reflect` mirror their JavaScript namesakes. They behave like handles: a `js_sys::Array` is a *reference into the JavaScript heap*, not a Rust `Vec`, so iterating it yields `JsValue`s you then convert:

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn sum_numbers(arr: &js_sys::Array) -> f64 {
    let mut total = 0.0;
    for value in arr.iter() {
        // Each element is a JsValue. as_f64() returns Option<f64>
        // (None if it is not a JS number), mirroring `Number(x)` being NaN.
        total += value.as_f64().unwrap_or(0.0);
    }
    total
}

#[wasm_bindgen]
pub fn random_id() -> String {
    let n = js_sys::Math::random();
    format!("id-{}", (n * 1_000_000.0) as u64)
}
```

> **Note:** `js-sys` covers only the ECMAScript built-ins that exist in *any* JavaScript engine. Browser- and Node-specific APIs — `document`, `fetch`, `localStorage`, `setTimeout` — live in the separate `web-sys` crate, covered in [Web APIs](/19-wasm/06-web-apis/) and [DOM Manipulation](/19-wasm/07-dom-manipulation/).

### Errors cross the boundary with `catch`

JavaScript signals failure by throwing. Rust signals failure with `Result`. To let an imported function *throw*, mark it `catch` and give it a `Result` return type. Without `catch`, a thrown exception aborts the whole module:

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/storage.js")]
extern "C" {
    // JS may throw if the key is missing; `catch` turns that throw into Err.
    #[wasm_bindgen(js_name = readSetting, catch)]
    fn read_setting(key: &str) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen]
pub fn setting_or_default(key: &str, fallback: &str) -> String {
    match read_setting(key) {
        Ok(v) => v.as_string().unwrap_or_else(|| fallback.to_string()),
        Err(_) => fallback.to_string(),
    }
}
```

The `Err` arm carries a `JsValue` (the thrown value, usually an `Error` object), which you can inspect or rethrow. This is the closest WebAssembly gets to a `try`/`catch`, and it makes the JavaScript-throws-vs-Rust-returns mismatch explicit rather than silent.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust + wasm-bindgen |
| --- | --- | --- |
| Declaring foreign functions | `.d.ts` ambient declarations | `#[wasm_bindgen] extern "C"` block |
| Importing a module | `import { f } from "./m"` | `#[wasm_bindgen(module = "/m.js")]` |
| Importing an npm package | `import _ from "lodash"` | `#[wasm_bindgen(module = "lodash")]` |
| Reaching globals | just use `alert(...)` | `extern` block with **no** `module` |
| JS standard library | always present (`Array`, `Math`) | the `js-sys` crate |
| A JS value of unknown type | `any` / `unknown` | `JsValue` |
| Naming convention bridge | n/a (same names) | `js_name`, `js_namespace` |
| A thrown exception | propagates up the stack | `catch` → `Result<_, JsValue>` |
| `new Date()` | built into the runtime | `js_sys::Date::new_0()` |

### A `js_sys::Array` is not a `Vec`

This is the difference that bites hardest. A Rust `Vec<f64>` is a contiguous block in WebAssembly linear memory that Rust owns. A `js_sys::Array` is a *handle* (really an integer index into a table) pointing at an object the JavaScript garbage collector owns. Every `arr.get(i)` or `arr.iter()` is a call across the boundary, not a pointer dereference. So `js-sys` types are right when you are *talking to* JavaScript, but for heavy number crunching you want to receive a `Vec`/`&[f64]` (which `wasm-bindgen` maps to a typed array) and let Rust own the data. The boundary-cost trade-off is the subject of [Performance](/19-wasm/09-performance/).

### `JsValue` is `unknown`, not `any`

`JsValue` is an opaque handle to *some* JavaScript value. Unlike TypeScript's `any`, you cannot call methods on it directly; you must narrow it first (`as_f64`, `as_string`, `dyn_into::<js_sys::Array>()`), which makes it behave like `unknown`. The compiler forces you to acknowledge that you do not know the type yet. The full set of `JsValue` conversions, plus `serde-wasm-bindgen` for turning JavaScript objects into Rust structs, is covered in [wasm-bindgen deep dive](/19-wasm/05-wasm-bindgen/).

---

## Common Pitfalls

### Pitfall 1: the `module` path points at a file that does not exist

Because `wasm-bindgen` reads local module files **at build time**, a wrong path is a hard compile error, and the message cascades confusingly into "function not found", because the macro could not generate the binding:

```rust
// does not compile — js/format.js is missing on disk
#[wasm_bindgen(module = "/js/format.js")]
extern "C" {
    #[wasm_bindgen(js_name = formatPrice)]
    fn format_price(cents: i64, currency: &str) -> String;
}
```

The real `cargo build --target wasm32-unknown-unknown` output:

```text
error: failed to read file `/.../js-interop-demo/js/format.js`: No such file or directory (os error 2)
 --> src/lib.rs:4:25
  |
4 | #[wasm_bindgen(module = "/js/format.js")]
  |                         ^^^^^^^^^^^^^^^
...
error[E0425]: cannot find function `format_price` in this scope
  --> src/lib.rs:35:5
   |
35 |     format_price(cents, "USD")
   |     ^^^^^^^^^^^^ not found in this scope
```

When you see "cannot find function" for something you clearly declared, scroll up: the *first* error (the missing file) is the real cause. The path is relative to the crate root for a leading `/`, and relative to the current source file otherwise.

### Pitfall 2: declaring `Result` without `catch`

If you give an imported function a `Result` return type but forget `catch`, the macro cannot generate the marshalling code, because a bare `Result` has no representation across the WebAssembly boundary:

```rust
// does not compile (E0277) — Result return type requires `catch`
#[wasm_bindgen(module = "/js/storage.js")]
extern "C" {
    #[wasm_bindgen(js_name = readSetting)]
    fn read_setting_bad(key: &str) -> Result<JsValue, JsValue>;
}
```

The real compiler error:

```text
error[E0277]: the trait bound `Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>: FromWasmAbi` is not satisfied
 --> src/lib.rs:7:39
  |
7 |     fn read_setting_bad(key: &str) -> Result<JsValue, JsValue>;
  |                                       ^^^^^^^^^^^^^^^^^^^^^^^^ the trait `FromWasmAbi` is not implemented for `Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>`
```

`FromWasmAbi` is the trait every type that crosses the boundary must implement. Do not be surprised that `wasm-bindgen` emits this same `E0277` more than once (the build ends with `due to 3 previous errors`): the first occurrence points at the `#[wasm_bindgen(module = ...)]` attribute line, and a later one points at the `Result<...>` return type shown above. They are all the same root cause. The fix is to add `catch` to the attribute (as in the Rust example above), which makes `wasm-bindgen` emit the try/catch wrapper.

### Pitfall 3: expecting `js_sys::Array` to behave like `Vec`

```typescript
// JS: indexing is cheap and direct
const arr = [1, 2, 3];
const x = arr[0]; // 1
```

A `js_sys::Array` does not implement `Index`, and `arr.get(0)` returns a `JsValue`, not a number, because the element could be anything. New users try `arr[0]` and get a "cannot index" error, then try to add `JsValue`s and get a type error. The idiom is to iterate and convert each element (`value.as_f64()`), as shown earlier. If you control the JavaScript side, prefer passing a typed array (`Float64Array`) or a `Vec<f64>` so Rust owns the buffer.

### Pitfall 4: forgetting that closures must outlive the call

When you pass a Rust closure to JavaScript as a callback, the closure lives in Rust's memory. If JavaScript keeps the callback (a timer, an event listener) but Rust drops the `Closure`, the callback becomes a dangling reference and invoking it panics. The fix is to either store the `Closure` somewhere that lives long enough, or hand ownership to JavaScript with `.forget()` (a deliberate, one-time memory leak):

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/events.js")]
extern "C" {
    // JS: export function onTick(cb) { setInterval(() => cb(performance.now()), 1000); }
    #[wasm_bindgen(js_name = onTick)]
    fn on_tick(cb: &Closure<dyn FnMut(f64)>);
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn web_log(s: &str);
}

#[wasm_bindgen]
pub fn install_ticker() {
    let cb = Closure::<dyn FnMut(f64)>::new(|t: f64| {
        web_log(&format!("tick {t}"));
    });
    on_tick(&cb);
    // JS will call this forever, so give up Rust's ownership.
    cb.forget();
}
```

> **Warning:** `.forget()` leaks the closure on purpose — appropriate for a callback that genuinely lives for the whole program. For callbacks you will remove later (e.g. a `removeEventListener`), store the `Closure` in a struct field instead and drop it when you tear down. Closures are covered in depth in [wasm-bindgen deep dive](/19-wasm/05-wasm-bindgen/).

---

## Best Practices

- **Import the smallest possible surface.** Each `extern` declaration is one import in your `.wasm`. Declare only the functions you actually call, with the exact types you need, rather than mirroring an entire JavaScript API.
- **Take `&str` in, return `String` out.** For imported functions, `&str` parameters let `wasm-bindgen` copy the string once; returning an owned `String` is the natural shape for values JavaScript hands back. Avoid `String` parameters unless the function consumes the value.
- **Use `catch` for anything that can throw.** Any JavaScript that touches I/O, parsing, or the DOM can throw. Modeling it as `Result<_, JsValue>` keeps failures in Rust's type system instead of crashing the module.
- **Prefer `js-sys` to a hand-rolled `.js` shim.** If a JavaScript built-in already does what you need (`JSON.parse`, `Math.max`, `Array.from`), import it from `js-sys` rather than writing and shipping your own JavaScript file.
- **Keep number-heavy data on the Rust side.** Receive slices/`Vec`s rather than `js_sys::Array` when you will loop over thousands of elements; only reach for `js_sys::Array` when you are genuinely interoperating with a JavaScript array you do not own.
- **Pin nothing by hand; let `cargo add` resolve.** `wasm-bindgen`, `js-sys`, and `wasm-bindgen-futures` are released as a coordinated set. Add them with `cargo add` so their versions line up, and let `wasm-pack` (see [wasm-pack](/19-wasm/01-wasm-pack/)) match the CLI version automatically.

---

## Real-World Example

A common production pattern: the heavy logic is in Rust, but a few platform capabilities — fetching an auth token over the network, formatting money, logging to a structured logger — stay in JavaScript. Here Rust imports an async JavaScript function that returns a `Promise`, awaits it, and combines the result with its own work. Awaiting a JavaScript `Promise` needs the `wasm-bindgen-futures` crate, which bridges JavaScript promises to Rust `async`/`await`:

```bash
cargo add wasm-bindgen js-sys wasm-bindgen-futures
```

```javascript
// js/api.js — stays in JavaScript because it uses the browser's fetch + cookies
export async function fetchToken(scope) {
  const res = await fetch(`/auth/token?scope=${encodeURIComponent(scope)}`);
  if (!res.ok) throw new Error(`token request failed: ${res.status}`);
  return (await res.json()).token;
}
```

```rust
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// Import an async JS function. It returns a Promise; `catch` captures a throw
// (including the `throw new Error(...)` for a non-2xx response).
#[wasm_bindgen(module = "/js/api.js")]
extern "C" {
    #[wasm_bindgen(js_name = fetchToken, catch)]
    fn fetch_token(scope: &str) -> Result<js_sys::Promise, JsValue>;
}

// An exported async fn: from JavaScript this becomes a function returning a Promise.
#[wasm_bindgen]
pub async fn token_for(scope: &str) -> Result<String, JsValue> {
    // `?` propagates a synchronous throw from fetch_token.
    let promise = fetch_token(scope)?;
    // JsFuture::from turns the JS Promise into something Rust can .await.
    // A rejected Promise becomes Err(JsValue), propagated by `?`.
    let value = JsFuture::from(promise).await?;
    Ok(value.as_string().unwrap_or_default())
}
```

Building for WebAssembly succeeds with the current stable crates (`wasm-bindgen 0.2.122`, `js-sys 0.3.99`, `wasm-bindgen-futures 0.4.72`):

```text
$ cargo build --target wasm32-unknown-unknown
   Compiling wasm-bindgen-futures v0.4.72
   Compiling js-interop-demo v0.1.0 (/.../js-interop-demo)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.86s
```

From the JavaScript side, the exported `token_for` is an ordinary async function — `await token_for("read:billing")` resolves to a string or rejects with the captured error. The division of labor is the point: the network call and cookie handling stay in idiomatic JavaScript, while Rust owns the orchestration and can call into far heavier logic between the `await`s. Rust futures are lazy and require a runtime to drive them; here that runtime is the browser's own event loop, which `wasm-bindgen-futures` hooks into, so the `Promise` and the Rust future advance together. (Contrast this with eager JavaScript promises, which start running the moment you create them; see [Async](/11-async/) for the deeper story.)

---

## Further Reading

- [The `wasm-bindgen` Guide — Importing functions from JS](https://rustwasm.github.io/wasm-bindgen/reference/js-snippets.html) and [the `extern "C"` reference](https://rustwasm.github.io/wasm-bindgen/reference/attributes/on-js-imports/index.html)
- [`js-sys` API documentation on docs.rs](https://docs.rs/js-sys/): every JavaScript built-in binding
- [`wasm-bindgen-futures` on docs.rs](https://docs.rs/wasm-bindgen-futures/): bridging `Promise` and `async`/`await`
- [What WASM is and why Rust](/19-wasm/00-wasm-intro/): the big picture before the mechanics
- [Setting up wasm-pack](/19-wasm/01-wasm-pack/): project structure and the `cdylib` crate type
- [Your first Rust → WASM module](/19-wasm/02-first-wasm/): the minimal export/build/call loop
- [Calling Rust from JavaScript](/19-wasm/04-rust-from-js/): the opposite direction and the generated glue
- [wasm-bindgen deep dive](/19-wasm/05-wasm-bindgen/) — `JsValue`, `serde-wasm-bindgen`, and closures in detail
- [Using Web APIs with web-sys](/19-wasm/06-web-apis/) and [DOM manipulation](/19-wasm/07-dom-manipulation/): the browser-specific bindings beyond `js-sys`
- [Unsafe & FFI](/20-unsafe-ffi/) — the `extern "C"` machinery that `wasm-bindgen` builds on, for native code

---

## Exercises

### Exercise 1: Import a global and a built-in

**Difficulty:** Easy

**Objective:** Practice the two no-`module` import styles: a global function and a `js-sys` built-in.

**Instructions:** Write a function `shout_random()` that calls `Math.random()` (via `js-sys`), turns it into a percentage string like `"42%"`, and passes that string to the global `console.log`. Declare the `console.log` binding yourself with `js_namespace`/`js_name`; use `js_sys::Math::random` for the number.

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    // TODO: bind console.log here
}

#[wasm_bindgen]
pub fn shout_random() {
    // TODO: let pct = (js_sys::Math::random() * 100.0) as u32;
    // TODO: log "<pct>%"
}
```

<details>
<summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log(s: &str);
}

#[wasm_bindgen]
pub fn shout_random() {
    let pct = (js_sys::Math::random() * 100.0) as u32;
    log(&format!("{pct}%"));
}
```

`cargo build --target wasm32-unknown-unknown` finishes with `Finished` and no warnings. `Math::random` needs no `.js` file because it is a universal built-in supplied by `js-sys`; `console.log` needs no `module` because it is global.

</details>

### Exercise 2: Import a module function and handle a thrown error

**Difficulty:** Medium

**Objective:** Import a named export from a local JavaScript module and convert a JavaScript `throw` into a Rust `Result`.

**Instructions:** Given a JavaScript module `js/json.js` that exports `parseConfig(text)` which throws on invalid JSON, declare it to Rust with `catch`. Write `is_valid_config(text: &str) -> bool` that returns `true` when parsing succeeds and `false` when it throws. (Create the `.js` file too, since the path is read at build time.)

```javascript
// js/json.js
export function parseConfig(text) {
  return JSON.parse(text); // throws SyntaxError on bad input
}
```

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/json.js")]
extern "C" {
    // TODO: declare parseConfig with `catch`, returning Result<JsValue, JsValue>
}

#[wasm_bindgen]
pub fn is_valid_config(text: &str) -> bool {
    // TODO: return true on Ok, false on Err
    /* ??? */
}
```

<details>
<summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/json.js")]
extern "C" {
    #[wasm_bindgen(js_name = parseConfig, catch)]
    fn parse_config(text: &str) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen]
pub fn is_valid_config(text: &str) -> bool {
    parse_config(text).is_ok()
}
```

This compiles cleanly for `wasm32-unknown-unknown`. Without `catch`, the `Result` return type fails to compile with `the trait FromWasmAbi is not implemented for Result<...>` (Pitfall 2). `.is_ok()` is the idiomatic way to collapse a `Result` into a boolean when you do not care about the error's contents.

</details>

### Exercise 3: Pass a long-lived closure to JavaScript

**Difficulty:** Hard

**Objective:** Hand a Rust closure to a JavaScript event-style API and keep it alive correctly.

**Instructions:** Given `js/timer.js` exporting `everySecond(cb)` (which calls `cb(count)` once per second), write `start_counter()` that installs a Rust closure logging `"count: N"` each tick. Make sure the closure outlives `start_counter` so JavaScript can keep calling it. Use the `Closure<dyn FnMut(...)>` type.

```javascript
// js/timer.js
export function everySecond(cb) {
  let count = 0;
  setInterval(() => cb(++count), 1000);
}
```

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log(s: &str);
}

#[wasm_bindgen(module = "/js/timer.js")]
extern "C" {
    // TODO: declare everySecond(cb: &Closure<dyn FnMut(u32)>)
}

#[wasm_bindgen]
pub fn start_counter() {
    // TODO: build a Closure that logs "count: N", install it, and keep it alive
}
```

<details>
<summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log(s: &str);
}

#[wasm_bindgen(module = "/js/timer.js")]
extern "C" {
    #[wasm_bindgen(js_name = everySecond)]
    fn every_second(cb: &Closure<dyn FnMut(u32)>);
}

#[wasm_bindgen]
pub fn start_counter() {
    let cb = Closure::<dyn FnMut(u32)>::new(|count: u32| {
        log(&format!("count: {count}"));
    });
    every_second(&cb);
    // The timer keeps calling cb forever; hand ownership to JS so it never drops.
    cb.forget();
}
```

This compiles cleanly for `wasm32-unknown-unknown`. The key line is `cb.forget()`: without it, `cb` is dropped at the end of `start_counter`, JavaScript's interval would then call into freed memory, and the module would panic on the first tick. `forget()` deliberately leaks the closure — correct here because the timer runs for the program's lifetime. For a closure you intend to remove later, store it in a struct and drop it when you detach the listener instead (see [wasm-bindgen deep dive](/19-wasm/05-wasm-bindgen/)).

</details>
