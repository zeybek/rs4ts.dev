---
title: "Using Web APIs from Rust with web-sys"
description: "Call fetch, localStorage, and setTimeout from Rust with web-sys, the typed lib.dom.d.ts of WebAssembly. Covers JsFuture, Closures, and Cargo feature flags."
---

The browser is full of APIs you reach for every day — `fetch`, `localStorage`, `setTimeout`, `window`. From Rust compiled to WebAssembly, none of them exist by default. The **`web-sys`** crate gives you typed Rust bindings to every one of them, gated behind Cargo feature flags so your binary only carries the ones you ask for. This page shows how to call `fetch`, timers, and `localStorage` from Rust, and how the feature-flag system keeps your `.wasm` small.

---

## Quick Overview

A WebAssembly module has no built-in knowledge of the browser. It cannot see `window`, the network, or storage on its own. Every one of those is a JavaScript API that Rust must call *through* generated glue. The `web-sys` crate is that glue: a giant, auto-generated set of Rust bindings to the Web platform (the same surface area as the TypeScript `lib.dom.d.ts` you already rely on), with each API hidden behind a feature flag so you pay only for what you use.

For a TypeScript/JavaScript developer, the mental model is: **`web-sys` is to Rust what `lib.dom.d.ts` is to TypeScript** — the type definitions for the browser. The big difference is that `web-sys`'s surface is *opt-in*: instead of the whole DOM being available, you enable `Window`, `Storage`, `Request`, and so on one feature at a time, and forgetting one is a compile error rather than a runtime `undefined`.

---

## TypeScript/JavaScript Example

Here is an ordinary browser utility: a search box that caches each query's results in `localStorage`, fetches over the network on a cache miss, and debounces input with a timer so it doesn't fire on every keystroke. It touches three Web APIs — `localStorage`, `fetch`, and `setTimeout` — and they are all simply *there*.

```typescript
// search.ts — runs in the browser, no imports needed for Web APIs

let debounce: number | undefined;

async function search(query: string): Promise<string> {
  const cacheKey = `search:${query}`;

  // localStorage is synchronous and string-only.
  const cached = localStorage.getItem(cacheKey);
  if (cached !== null) {
    console.log("cache hit");
    return cached;
  }

  const res = await fetch(`https://api.example.com/search?q=${query}`, {
    method: "GET",
    mode: "cors",
    headers: { Accept: "application/json" },
  });
  if (!res.ok) {
    throw new Error(`search failed: HTTP ${res.status}`);
  }

  const body = await res.text();
  localStorage.setItem(cacheKey, body); // cache for next time
  return body;
}

const input = document.querySelector<HTMLInputElement>("#q")!;
input.addEventListener("input", () => {
  clearTimeout(debounce);
  // setTimeout returns a numeric handle you can clear later.
  debounce = window.setTimeout(() => {
    search(input.value).then((json) => console.log(json));
  }, 300);
});
```

Nothing here is exotic. `window`, `localStorage`, `fetch`, and `setTimeout` are global ambient APIs that TypeScript knows about because `lib.dom.d.ts` ships with the compiler. You never declare a dependency on them.

---

## Rust Equivalent

The same three APIs from Rust through `web-sys`. The shape is recognizable, but every API is reached explicitly through `web_sys::window()`, and every Web type (`Storage`, `Request`, `Response`) must be enabled as a Cargo feature.

```rust
// src/lib.rs
// `prelude::*` also brings the `JsCast` trait into scope, which provides
// the `dyn_into` / `dyn_ref` downcasts used below.
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

/// Save a value to localStorage. Exported to JS as `save_theme(theme)`.
#[wasm_bindgen]
pub fn save_theme(theme: &str) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let storage = window
        .local_storage()? // Result<Option<Storage>, JsValue>
        .expect("localStorage is not available");
    storage.set_item("theme", theme)?;
    Ok(())
}

/// Read a value, defaulting to "light" when the key is absent.
#[wasm_bindgen]
pub fn load_theme() -> Result<String, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let storage = window
        .local_storage()?
        .expect("localStorage is not available");
    let theme = storage.get_item("theme")?.unwrap_or_else(|| "light".into());
    Ok(theme)
}

/// Fetch a user as JSON. Exported as an async function returning a Promise.
#[wasm_bindgen]
pub async fn fetch_user(id: u32) -> Result<JsValue, JsValue> {
    let url = format!("https://api.example.com/users/{id}");

    // RequestInit is configured through setters (not struct fields).
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(&url, &opts)?;
    request.headers().set("Accept", "application/json")?;

    let window = web_sys::window().expect("no global `window` exists");
    // window.fetch(...) returns a JS Promise; await it via JsFuture.
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;

    // fetch resolves to a generic JsValue; narrow it to Response.
    let resp: Response = resp_value.dyn_into()?;
    if !resp.ok() {
        return Err(JsValue::from_str(&format!(
            "request failed with status {}",
            resp.status()
        )));
    }

    // resp.json() is itself a Promise<any>; await it too.
    let json = JsFuture::from(resp.json()?).await?;
    Ok(json)
}
```

The dependency on `web-sys` is where the feature flags live. You list every Web API type you use, and *only* those, so the binary stays small:

```toml
# Cargo.toml
[package]
name = "search-client"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"  # for awaiting JS Promises
js-sys = "0.3"                # built-in JS objects (Array, Object, Promise)

[dependencies.web-sys]
version = "0.3"
features = [
  "Window",       # web_sys::window(), fetch, set_timeout, local_storage
  "Storage",      # the Storage type behind local_storage()
  "Request",
  "RequestInit",
  "RequestMode",
  "Response",
  "Headers",
  "console",      # web_sys::console::log_1, etc.
]
```

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition, and `cargo new` selects it automatically. The verified crate versions used throughout this page are `web-sys` 0.3.99, `wasm-bindgen` 0.2.122, `js-sys` 0.3.99, and `wasm-bindgen-futures` 0.4.72. The caret ranges above (`"0.3"`, `"0.2"`) resolve to those automatically.

All of the Rust above compiles cleanly for the browser target with `cargo check --target wasm32-unknown-unknown` on Rust 1.96.0.

---

## Detailed Explanation

### Everything starts at `web_sys::window()`

In JavaScript `window` is an ambient global. In Rust it is a function call that returns an `Option<Window>`:

```rust
let window = web_sys::window().expect("no global `window` exists");
```

It returns `Option` because your code might run somewhere there is no `window` — for example inside a Web Worker (which has a `WorkerGlobalScope` instead) or in Node.js. Rust makes that possibility explicit instead of letting you hit a runtime `ReferenceError`. From the `Window` you reach everything else: `window.local_storage()`, `window.fetch_with_request(...)`, `window.set_timeout_with_callback_and_timeout_and_arguments_0(...)`, `window.document()`, and so on.

### `localStorage` is synchronous and fallible

```rust
let storage = window.local_storage()?.expect("localStorage is not available");
storage.set_item("theme", theme)?;
let theme = storage.get_item("theme")?.unwrap_or_else(|| "light".into());
```

`local_storage()` returns `Result<Option<Storage>, JsValue>`. The two layers encode two real failure modes from the JavaScript world:

- The **`Result`** (the `?`) handles the case where reading the property *throws* — accessing `localStorage` raises a `SecurityError` when storage is blocked (for example, third-party iframes with cookies disabled). In JavaScript that exception would propagate as an uncaught error; in Rust it becomes a `JsValue` error you must handle.
- The **`Option`** handles `localStorage` being `null`/absent.

Once you have a `Storage`, the methods mirror the JavaScript API one-to-one: `set_item`, `get_item` (returns `Option<String>`, mapping JavaScript's "value or `null`"), `remove_item`, and `clear`. Like its JavaScript counterpart, it stores **only strings**. To persist a struct you serialize to JSON first (see [Section 15: Serialization](/15-serialization/)).

### `fetch` is a Promise, so you bridge it with `JsFuture`

This is the part that feels most different. `window.fetch_with_request(...)` returns a JavaScript `Promise`, represented in Rust as a `js_sys::Promise`. Rust's `async`/`await` doesn't natively understand a JS Promise, so the `wasm-bindgen-futures` crate provides `JsFuture::from(promise)`, which wraps the Promise in a Rust `Future` you can `.await`:

```rust
let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
let resp: Response = resp_value.dyn_into()?;
```

`JsFuture` resolves to a `JsValue` — the dynamically-typed "any JS value" handle. Because `fetch` promises a `Response` but the type system only sees `JsValue`, you narrow it with `dyn_into::<Response>()`, the checked downcast from the `JsCast` trait. This is the Rust equivalent of a TypeScript `as Response` assertion, except it is *checked at runtime*: if the value isn't actually a `Response`, you get an `Err`, not a silent lie.

> **Tip:** Rust futures are **lazy** — unlike a JavaScript `Promise`, which starts running the moment it is created, a Rust `Future` does nothing until it is `.await`ed or spawned. When you mark an exported function `pub async fn`, `wasm-bindgen` returns a real JavaScript `Promise` to the caller and drives the future for you. From the JavaScript side, `await fetch_user(7)` works exactly as you'd expect. The laziness only matters if you build futures and forget to await them. See [Section 11: Async](/11-async/).

### Timers take a callback, so they take a closure

`setTimeout(fn, ms)` needs a function. Rust passes a function to JavaScript by boxing a closure into a `Closure` and handing the engine a reference to it:

```rust
use wasm_bindgen::closure::Closure;

#[wasm_bindgen]
pub fn delayed_log(message: String, delay_ms: i32) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");

    // Box a Rust closure so JavaScript can call it later.
    let closure = Closure::<dyn FnMut()>::new(move || {
        web_sys::console::log_1(&JsValue::from_str(&message));
    });

    window.set_timeout_with_callback_and_timeout_and_arguments_0(
        closure.as_ref().unchecked_ref(), // &Function the API expects
        delay_ms,
    )?;

    // Hand ownership to the JS engine so the closure isn't dropped on return.
    closure.forget();
    Ok(())
}
```

The long method name is not a typo. `web-sys` generates one Rust method per *overload* of a Web API, because Rust has no function overloading. `setTimeout(cb, ms)` becomes `set_timeout_with_callback_and_timeout_and_arguments_0` — "with a callback, with a timeout, with zero extra arguments." It is verbose but unambiguous, and your editor autocompletes it.

The `closure.forget()` call is the load-bearing detail. A `Closure` owns heap memory; when it drops, the underlying JavaScript function becomes invalid. If you let `closure` drop at the end of `delayed_log`, the timer would later fire into freed memory. `forget()` deliberately leaks the closure so it lives as long as the engine might call it. For a one-shot `setTimeout` that fires once, this is a small, bounded leak. The deeper mechanics of closures crossing the boundary are in [The wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/).

### Feature flags: the part with no TypeScript analogue

The `web-sys` crate is *enormous*: it covers essentially the entire Web platform. If every API compiled into every project, build times and binary sizes would be unbearable. So each type, method, and free function is gated behind a Cargo **feature flag**, and `web_sys::window()` only exists if you enable the `"Window"` feature. You enable exactly the slice of the Web platform your code touches, and the rest is compiled out entirely. There is no TypeScript equivalent — `lib.dom.d.ts` gives you the whole DOM at once. We'll see what happens when you forget a flag in the Common Pitfalls section.

---

## Key Differences

| Aspect | TypeScript/JavaScript | Rust + `web-sys` |
| --- | --- | --- |
| Where APIs come from | Ambient globals (`window`, `fetch`) via `lib.dom.d.ts` | The `web-sys` crate, reached via `web_sys::window()` |
| Availability | Whole DOM available at once | Opt-in per **feature flag**; unused APIs compiled out |
| `localStorage` access | `localStorage.getItem(k)` → `string \| null` | `storage.get_item(k)?` → `Result<Option<String>, JsValue>` |
| Awaiting `fetch` | `await fetch(...)` (Promise is native) | `JsFuture::from(promise).await?` bridges JS Promise → Rust Future |
| Future eagerness | Promise starts immediately | Future is **lazy**; runs only when awaited/spawned |
| Narrowing a value | `value as Response` (unchecked) | `value.dyn_into::<Response>()?` (**checked** at runtime) |
| Passing a callback | Pass a function directly | Box a `Closure` and manage its lifetime (`forget`) |
| Method names | One name, many overloads | One Rust method **per overload** (verbose, explicit) |
| Missing API | Runtime `undefined`/`ReferenceError` | **Compile error** (feature not enabled) |

The throughline: `web-sys` trades JavaScript's "everything is always there, fail at runtime" for Rust's "declare what you need, fail at compile time." A forgotten feature flag is caught before your code ever ships.

---

## Common Pitfalls

### Pitfall 1: Forgetting to enable the feature flag

This is *the* `web-sys` rite of passage. You write code that calls `web_sys::window()`, and it doesn't compile because you never added `"Window"` to your features. The error is unusually helpful — it tells you exactly which feature is missing:

```rust
// Cargo.toml has `features = []` — no "Window" enabled
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn current_origin() -> Result<String, JsValue> {
    let window = web_sys::window().expect("no window"); // does not compile (error[E0425])
    let storage = window.local_storage()?.expect("no localStorage");
    storage.set_item("k", "v")?;
    Ok("ok".into())
}
```

The real `cargo check --target wasm32-unknown-unknown` output:

```text
error[E0425]: cannot find function `window` in crate `web_sys`
  --> src/lib.rs:5:27
   |
 5 |     let window = web_sys::window().expect("no window");
   |                           ^^^^^^ not found in `web_sys`
   |
note: found an item that was configured out
  --> /Users/you/.cargo/registry/src/.../web-sys-0.3.99/src/lib.rs:35:8
   |
34 | #[cfg(feature = "Window")]
   |       ------------------ the item is gated behind the `Window` feature
35 | pub fn window() -> Option<Window> {
   |        ^^^^^^

For more information about this error, try `rustc --explain E0425`.
error: could not compile `websys_probe` (lib) due to 1 previous error
```

**Fix:** Read the `note: ... gated behind the `Window` feature` line and add that feature to `Cargo.toml`. The [docs.rs page for `web-sys`](https://docs.rs/web-sys) lists the required feature(s) at the top of every type and method's documentation, so when you reach for a new API, check there first.

### Pitfall 2: Trying to `.await` a JS Promise directly

Coming from JavaScript, it is natural to write `let resp = window.fetch_with_request(&request).await?;`. That does not compile: `window.fetch_with_request(...)` returns a `js_sys::Promise`, and a `Promise` is not a Rust `Future` — it has no `.await`. You must wrap it.

**Fix:** `let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;`, and make sure `wasm-bindgen-futures` is a dependency. The same applies to `resp.json()` and `resp.text()`, which each return a Promise.

### Pitfall 3: Letting a timer/event closure drop

If you create a `Closure` for `setTimeout` or `setInterval` and let it fall out of scope without `forget()` (or without storing it somewhere that outlives the timer), the closure's memory is freed while the JavaScript engine still holds a reference to it. The timer then fires into invalid memory. There is no compiler error for this (the code compiles fine), so it is a genuine runtime trap.

**Fix:** For fire-and-forget timers, call `closure.forget()` to intentionally leak it. For closures you need to clean up (like a self-clearing interval), store the `Closure` in a structure that lives as long as the timer, and drop it when you clear the timer. Exercise 2 below walks through the self-clearing pattern.

### Pitfall 4: Calling `dyn_into` without `JsCast` in scope

The `dyn_into()` and `dyn_ref()` methods live on the `JsCast` trait, and Rust trait methods are only callable when the trait is imported (see [Section 09: Generics & Traits](/09-generics-traits/)). If `JsCast` is in scope by *neither* route, `resp_value.dyn_into::<Response>()` fails to compile with `error[E0599]: no method named `dyn_into` found`, and the compiler even suggests `use wasm_bindgen::JsCast;`.

There are two ways to bring it into scope, and you only need one:

- An explicit `use wasm_bindgen::JsCast;`, or
- `use wasm_bindgen::prelude::*;`: the prelude **re-exports** `JsCast`, so the glob import that every example in this file already uses brings `dyn_into`/`dyn_ref` along with it.

**Fix:** Make sure `JsCast` is in scope. If you already have `use wasm_bindgen::prelude::*;`, you are covered; adding a separate `use wasm_bindgen::JsCast;` is redundant (harmless, but unnecessary).

---

## Best Practices

- **Enable the minimum set of features.** Every feature you add pulls more of `web-sys` into the build. Start empty, let the compiler tell you what's missing (Pitfall 1's error names the feature for you), and add only those. Smaller feature sets mean smaller `.wasm` and faster builds. Bundle-size discipline is covered in [WASM Performance](/19-wasm/09-performance/).
- **Check docs.rs for the required feature *and* the method name.** Because `web-sys` is auto-generated from the Web IDL, the docs are the source of truth for both the verbose method name (`set_timeout_with_callback_and_timeout_and_arguments_0`) and the feature gate. Don't guess from memory.
- **Return `Result<T, JsValue>` from exported functions that can fail.** `wasm-bindgen` turns an `Err(JsValue)` into a thrown JavaScript exception, so your TypeScript callers get a normal `try`/`catch` or a rejected Promise. This keeps the error model familiar on the JavaScript side.
- **Prefer `?` over `.unwrap()` for fallible Web calls.** Storage can be blocked, network calls fail, and downcasts can mismatch. Propagating with `?` turns these into clean rejected Promises instead of panics that abort the whole module.
- **Use `js-sys` for built-in JS objects and `web-sys` for the browser.** `js-sys` covers `Array`, `Object`, `Date`, `Promise`, `JSON` — the ECMAScript globals. `web-sys` covers the Web platform: `window`, `fetch`, `Storage`, the DOM. For richer struct↔JS conversions, reach for `serde-wasm-bindgen` (see [The wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/)).
- **Keep network/JSON shapes typed where it matters.** `fetch_user` above returns a raw `JsValue` for brevity. In production you'll often deserialize into a Rust struct with `serde-wasm-bindgen::from_value`, getting the same compile-time guarantees you'd want from a typed `fetch` wrapper in TypeScript.

---

## Real-World Example

A reusable search client that combines all three APIs: it caches each query's JSON in `localStorage`, fetches on a miss, and reports cache hits via the console. It's exported as a `struct` so JavaScript can construct it once and call `search` repeatedly. The whole thing is compile-verified for the browser target.

```rust
// src/lib.rs
// `prelude::*` re-exports `JsCast`, so `dyn_into` is in scope without a separate import.
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response, Storage, Window};

/// A search client that caches each query's raw JSON in `localStorage`,
/// so repeat searches skip the network entirely.
#[wasm_bindgen]
pub struct SearchClient {
    window: Window,
    storage: Storage,
}

#[wasm_bindgen]
impl SearchClient {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<SearchClient, JsValue> {
        let window = web_sys::window().ok_or("no global `window`")?;
        let storage = window
            .local_storage()?
            .ok_or("localStorage is unavailable")?;
        Ok(SearchClient { window, storage })
    }

    /// Returns the result JSON as a string, using the cache when possible.
    pub async fn search(&self, query: String) -> Result<String, JsValue> {
        let cache_key = format!("search:{query}");

        // 1. Check the cache first (a synchronous Web API call).
        if let Some(cached) = self.storage.get_item(&cache_key)? {
            web_sys::console::log_1(&JsValue::from_str("cache hit"));
            return Ok(cached);
        }

        // 2. Miss: build and send a real request.
        let opts = RequestInit::new();
        opts.set_method("GET");
        opts.set_mode(RequestMode::Cors);

        let url = format!("https://api.example.com/search?q={query}");
        let request = Request::new_with_str_and_init(&url, &opts)?;

        let resp_value =
            JsFuture::from(self.window.fetch_with_request(&request)).await?;
        let resp: Response = resp_value.dyn_into()?;

        if !resp.ok() {
            return Err(JsValue::from_str(&format!(
                "search failed: HTTP {}",
                resp.status()
            )));
        }

        let body = JsFuture::from(resp.text()?).await?;
        let body: String = body.as_string().unwrap_or_default();

        // 3. Cache for next time, then return.
        self.storage.set_item(&cache_key, &body)?;
        Ok(body)
    }
}
```

```toml
# Cargo.toml
[dependencies]
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
js-sys = "0.3"

[dependencies.web-sys]
version = "0.3"
features = ["Window", "Storage", "Request", "RequestInit", "RequestMode", "Response", "console"]
```

From TypeScript, the generated glue (produced by `wasm-pack`, see [Setting Up wasm-pack](/19-wasm/01-wasm-pack/)) makes this feel like any other class, with a typed `.d.ts` and a real `Promise` return:

```typescript
// app.ts
import init, { SearchClient } from "./pkg/search_client.js";

await init(); // load + instantiate the .wasm module once
const client = new SearchClient();

const input = document.querySelector<HTMLInputElement>("#q")!;
let debounce: number | undefined;

input.addEventListener("input", () => {
  clearTimeout(debounce);
  debounce = window.setTimeout(async () => {
    try {
      const json = await client.search(input.value); // returns a Promise<string>
      console.log(json);
    } catch (err) {
      console.error("search error:", err); // Err(JsValue) became a thrown error
    }
  }, 300);
});
```

This module compiles cleanly for the browser target with `cargo check --target wasm32-unknown-unknown` on Rust 1.96.0.

> **Warning:** Building a URL with `format!("...q={query}")` does **not** URL-encode the query. Just like in JavaScript you'd reach for `encodeURIComponent` (or build a `URLSearchParams`), in Rust enable the `Url` / `UrlSearchParams` features of `web-sys` (or use the `url` crate) to encode user input safely. Wasm gives you speed, not input sanitization.

---

## Further Reading

### Official documentation

- [`web-sys` on docs.rs](https://docs.rs/web-sys): every type, method, and the feature flag each requires
- [The `web-sys` chapter of the Rust and WebAssembly Book](https://rustwasm.github.io/docs/wasm-bindgen/web-sys/index.html): features, the `fetch` and DOM examples
- [`wasm-bindgen-futures` on docs.rs](https://docs.rs/wasm-bindgen-futures): `JsFuture` and `spawn_local`
- [MDN: Web Storage API](https://developer.mozilla.org/en-US/docs/Web/API/Web_Storage_API) and [MDN: Fetch API](https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API): the JavaScript APIs `web-sys` mirrors

### Related sections in this guide

- Previous concepts: [The wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/): `JsValue`, closures, and how data crosses the boundary
- [Calling JavaScript from Rust](/19-wasm/03-js-interop/) and [Calling Rust from JavaScript](/19-wasm/04-rust-from-js/): the two directions of interop
- Next: [DOM Manipulation with web-sys](/19-wasm/07-dom-manipulation/) — `document`, elements, and event listeners (same crate, same feature-flag system)
- [Frontend Frameworks: Yew & Leptos](/19-wasm/08-yew-leptos/): frameworks that wrap `web-sys` for you
- [WASM Performance](/19-wasm/09-performance/): why minimal feature sets keep your `.wasm` small
- Background: [Section 11: Async](/11-async/) (Rust futures are lazy), [Section 15: Serialization](/15-serialization/) (turning JSON into structs), and [Section 09: Generics & Traits](/09-generics-traits/) (why `JsCast` must be imported)
- Going lower-level: [Section 20: Unsafe & FFI](/20-unsafe-ffi/): the same "call into another world" skills applied to C

---

## Exercises

### Exercise 1: A logout helper with localStorage

**Difficulty:** Easy

**Objective:** Practice the synchronous `Storage` API: removing one key and clearing all of it.

**Instructions:** Write an exported function `forget_session()` that removes the `"auth_token"` key from `localStorage` and then clears all remaining storage. Return `Result<(), JsValue>` and propagate failures with `?`. Verify it compiles with `cargo check --target wasm32-unknown-unknown` (enable the `Window` and `Storage` features).

<details>
<summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn forget_session() -> Result<(), JsValue> {
    let storage = web_sys::window()
        .ok_or("no window")?
        .local_storage()?
        .ok_or("no localStorage")?;
    storage.remove_item("auth_token")?;
    storage.clear()?;
    Ok(())
}
```

```toml
# Cargo.toml
[dependencies]
wasm-bindgen = "0.2"

[dependencies.web-sys]
version = "0.3"
features = ["Window", "Storage"]
```

`remove_item` deletes a single key; `clear` empties the whole store. Both mirror the JavaScript `localStorage` methods exactly. Each call returns `Result<(), JsValue>` because storage access can throw a `SecurityError`, so `?` propagates that cleanly. This compiles for `wasm32-unknown-unknown`.

</details>

### Exercise 2: A self-clearing interval

**Difficulty:** Medium

**Objective:** Manage a closure's lifetime across repeated timer invocations, the trickiest part of using callbacks from Rust.

**Instructions:** Write an exported function `start_ticker(max: u32)` that uses `setInterval` to log an incrementing counter to the console once per second, and stops itself (clears the interval) after `max` ticks. The challenge: the interval callback needs to reach both the interval *handle* (to clear it) and *itself* (to drop the closure). Enable the `Window`, `Storage`, and `console` features. Verify with `cargo check --target wasm32-unknown-unknown`.

<details>
<summary>Solution</summary>

```rust
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;

#[wasm_bindgen]
pub fn start_ticker(max: u32) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;

    // Shared state: the running count and the interval handle, both behind
    // Rc<RefCell<_>> so the closure can mutate them across invocations.
    // (Wasm is single-threaded, so Rc/RefCell — not Arc/Mutex — is correct.)
    let count = Rc::new(RefCell::new(0u32));
    let handle = Rc::new(RefCell::new(None::<i32>));

    // The closure needs to reach itself (to drop it) and the handle (to clear
    // the interval), so we stash it in an Option first, then fill it in.
    let cb: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));

    let cb_inner = cb.clone();
    let handle_inner = handle.clone();
    let window_inner = window.clone();

    *cb.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || {
        let mut n = count.borrow_mut();
        *n += 1;
        web_sys::console::log_1(&JsValue::from_f64(*n as f64));
        if *n >= max {
            if let Some(id) = *handle_inner.borrow() {
                window_inner.clear_interval_with_handle(id);
            }
            // We're done: drop the closure so its memory is reclaimed.
            let _ = cb_inner.borrow_mut().take();
        }
    }));

    let id = window.set_interval_with_callback_and_timeout_and_arguments_0(
        cb.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
        1000,
    )?;
    *handle.borrow_mut() = Some(id);

    // Keep `cb` alive past this function. The closure removes itself from
    // `cb_inner` once it's finished, so this leak is bounded — it lives
    // exactly as long as the interval.
    std::mem::forget(cb);
    Ok(())
}
```

```toml
# Cargo.toml
[dependencies]
wasm-bindgen = "0.2"
js-sys = "0.3"

[dependencies.web-sys]
version = "0.3"
features = ["Window", "Storage", "console"]
```

The key insight is the chicken-and-egg problem: the closure must reference the interval handle and itself, but neither exists until *after* the closure is created. The fix is `Rc<RefCell<Option<...>>>` holders that you create empty, then fill in once you have the real values. `Rc` (reference-counted) and `RefCell` (interior mutability) are the single-threaded equivalents of `Arc<Mutex<...>>` — appropriate because Wasm in the browser runs on one thread. This compiles cleanly (no warnings) for `wasm32-unknown-unknown`.

</details>

### Exercise 3: A cached GET with a typed result

**Difficulty:** Hard

**Objective:** Combine `fetch`, `localStorage`, and error handling into one coarse-grained function, returning a meaningful error on failure.

**Instructions:** Write an async exported function `get_cached(url: String) -> Result<String, JsValue>` that: (1) checks `localStorage` for a cached body keyed by the URL and returns it on a hit; (2) on a miss, performs a CORS `GET`, and if the response is not OK, returns an `Err` whose message includes the HTTP status; (3) on success, caches the body text under the URL key and returns it. Enable the features you need and verify with `cargo check --target wasm32-unknown-unknown`.

<details>
<summary>Solution</summary>

```rust
// `prelude::*` re-exports `JsCast`, which provides `dyn_into` below.
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

#[wasm_bindgen]
pub async fn get_cached(url: String) -> Result<String, JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let storage = window.local_storage()?.ok_or("no localStorage")?;

    // 1. Cache hit?
    if let Some(cached) = storage.get_item(&url)? {
        return Ok(cached);
    }

    // 2. Miss: do a CORS GET.
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(&url, &opts)?;

    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    if !resp.ok() {
        return Err(JsValue::from_str(&format!("HTTP {}", resp.status())));
    }

    // 3. Cache the body text, then return it.
    let text = JsFuture::from(resp.text()?).await?;
    let body = text.as_string().unwrap_or_default();
    storage.set_item(&url, &body)?;
    Ok(body)
}
```

```toml
# Cargo.toml
[dependencies]
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
js-sys = "0.3"

[dependencies.web-sys]
version = "0.3"
features = ["Window", "Storage", "Request", "RequestInit", "RequestMode", "Response"]
```

This is the canonical "Web APIs from Rust" composition: a synchronous `Storage` check, an async `fetch` bridged through `JsFuture`, a checked `dyn_into::<Response>()`, a status check that returns a descriptive `Err`, and a write back to the cache. Because the function returns `Result<_, JsValue>`, every failure path becomes a rejected `Promise` on the JavaScript side, so callers handle it with ordinary `try`/`catch`. It compiles for `wasm32-unknown-unknown`. To return a *typed* value instead of a string, deserialize the body with `serde-wasm-bindgen` — see [The wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/).

</details>
