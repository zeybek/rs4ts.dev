---
title: "WebAssembly"
sidebar:
  label: "Overview"
description: "Compile a Rust crate to a compact .wasm binary that runs beside your JavaScript at near-native speed. Map your front-end toolbox onto wasm-pack and Rust."
---

WebAssembly is where Rust meets the browser: you compile a Rust crate to a compact `.wasm` binary that runs alongside your JavaScript at near-native speed, with no garbage collector to ship and full type-checking across the boundary. This section maps the front-end toolbox you already know (`tsc` + a bundler, `lib.dom.d.ts`, `fetch`/`localStorage`/`setTimeout`, React/Svelte components) onto their idiomatic Rust counterparts. You will set up **wasm-pack** and **wasm-bindgen**, call JavaScript from Rust and Rust from JavaScript, reach Web APIs and the DOM through **web-sys**, build whole UIs with **Yew** and **Leptos**, and tune bundle size, the JS↔WASM boundary cost, and deployment.

The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. Crate examples are pinned to current releases (wasm-bindgen 0.2.122, web-sys/js-sys 0.3.99, wasm-bindgen-futures 0.4.72, serde-wasm-bindgen 0.6, wasm-pack 0.13) and are compile-verified against `wasm32-unknown-unknown`.

---

## What You'll Learn

- What WebAssembly is, what runs where, and when Rust→WASM genuinely beats plain JavaScript (and when it does not)
- How to set up a `cdylib` crate and drive `wasm-pack` to emit a browser-ready, npm-installable package
- How to export a Rust function with `#[wasm_bindgen]`, build it, and call it from a web page
- How to call JavaScript from Rust with `#[wasm_bindgen(module = ...)]` imports and the `js-sys` built-ins
- How exported functions and structs appear from JavaScript, and what the generated JS/`.d.ts` glue actually does
- Which types can cross the boundary, what `JsValue` is, when to reach for `serde-wasm-bindgen`, and how to hand a closure to JavaScript without leaking or crashing
- How to use Web APIs (`fetch`, timers, `localStorage`) from Rust through `web-sys` and its feature-flag system
- How to query, create, and wire up DOM elements and event listeners from Rust
- How the Yew (component/Elm-like) and Leptos (fine-grained reactivity) frameworks let you write a whole UI in Rust
- How to measure and shrink bundle size (`wasm-opt`, `twiggy`), reason about the boundary cost, and decide when WASM wins
- How to deploy a WASM app: bundlers (Vite/webpack), serving `.wasm` with the right MIME type, and CDN caching

---

## Topics

| Topic | Description |
| ----- | ----------- |
| [What Is WebAssembly?](/19-wasm/00-wasm-intro/) | What WASM is and why use Rust for it; realistic use cases vs plain JavaScript, and what runs where. |
| [Setting Up wasm-pack](/19-wasm/01-wasm-pack/) | The build toolchain: project structure, the `cdylib` crate type, and the `web`/`bundler`/`nodejs` build targets. |
| [Your First Rust → WASM Module](/19-wasm/02-first-wasm/) | A `#[wasm_bindgen]` export, one build command, and calling the result from a web page. |
| [Calling JavaScript from Rust](/19-wasm/03-js-interop/) | Importing JS into Rust via `#[wasm_bindgen(module = ...)]` and the `js-sys` standard-library bindings. |
| [Calling Rust from JavaScript](/19-wasm/04-rust-from-js/) | Exporting functions and structs, and what the generated JS glue and `.d.ts` look like to the consumer. |
| [The wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/) | Types crossing the boundary, `JsValue`, `serde-wasm-bindgen`, and closures/callbacks. |
| [Using Web APIs from Rust](/19-wasm/06-web-apis/) | `fetch`, timers, and `localStorage` from Rust with `web-sys`, and the Cargo feature-flag system. |
| [DOM Manipulation from Rust](/19-wasm/07-dom-manipulation/) | Reading the `document`, creating elements, and attaching event listeners with `web-sys`. |
| [Frontend Frameworks: Yew & Leptos](/19-wasm/08-yew-leptos/) | Yew (component/Elm-like) and Leptos (fine-grained reactivity): overview plus a tiny example of each. |
| [WASM Performance](/19-wasm/09-performance/) | Bundle size (`wasm-opt`, `twiggy`), the JS↔WASM boundary cost, and when WASM actually wins. |
| [Deploying WASM Apps](/19-wasm/10-deployment/) | Bundlers (Vite/webpack), serving `.wasm` with the correct MIME type, and CDN caching. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Decide whether a given workload belongs in WebAssembly or stays in JavaScript, and justify the choice
- Scaffold a `cdylib` crate and produce a `pkg/` directory with `wasm-pack` for the right target
- Export Rust functions and structs to JavaScript and consume them, including the `await init()` step and explicit `free()`
- Import JavaScript functions, globals, and npm packages into Rust, and bridge a JS `Promise` with `JsFuture`
- Reason about which Rust types cross the boundary cheaply, which copy, and which need `serde-wasm-bindgen`
- Hand a Rust closure to JavaScript and manage its lifetime so it neither dangles nor leaks
- Drive Web APIs and the DOM from Rust with `web-sys`, enabling only the feature flags you need
- Sketch a UI in Yew or Leptos and explain the difference between a virtual-DOM and a fine-grained-reactive model
- Profile and shrink a `.wasm` binary, and design a coarse-grained boundary that crosses rarely with large payloads
- Wire the build into a bundler and serve and cache the `.wasm` artifact correctly in production

---

## Prerequisites

- [Section 12: Modules and Packages](/12-modules-packages/) — a WASM project is a library crate with a `[lib]` `crate-type`, and you will lean on crates, `Cargo.toml`, and feature flags throughout.
- [Section 15: Serialization](/15-serialization/): `serde-wasm-bindgen` reuses the Serde derive model to move structured data across the JS↔WASM boundary.

A working knowledge of the earlier fundamentals — [ownership](/05-ownership/) (why exported structs need `free()`), [error handling](/08-error-handling/) (`Result` becomes a thrown JS exception), and [async](/11-async/) (Rust futures are lazy, unlike eager JS Promises) — will also help.

---

## Estimated Time

Approximately **14 hours**, including reading, hands-on practice, and the per-topic exercises.

---

## Next

Continue to [Section 20: Unsafe & FFI](/20-unsafe-ffi/) to apply the same "talk to another world" skills to native C code: the `extern "C"` and `cdylib` machinery that `wasm-bindgen` builds on.

---

## Frequently asked questions

### How do I run Rust in the browser?

Compile a crate to WebAssembly with `wasm-pack`, then `import` the generated module from JavaScript. `wasm-bindgen` produces the JS glue and the TypeScript types for your exported functions. See [Your First WASM Module](/19-wasm/02-first-wasm/).

### When is Rust and WASM actually faster than JavaScript?

For CPU-bound work such as parsing, image processing, or crypto, once the module is loaded. Every JS↔WASM boundary crossing has a cost, so the win comes from doing substantial work per call rather than many tiny calls. See [WASM Performance](/19-wasm/09-performance/).

### Can Rust call browser APIs like `fetch` or the DOM?

Yes, through `web-sys`, the typed bindings to browser APIs (the WASM equivalent of `lib.dom.d.ts`). You enable the features you need and call `document`, `fetch`, and friends from Rust. See [Web APIs from Rust](/19-wasm/06-web-apis/).

