---
title: "Setting Up wasm-pack"
description: "Set up wasm-pack to turn a Rust crate into a browser-ready npm package: the cdylib crate type, web/bundler/nodejs targets, and the generated .wasm plus .js"
---

The toolchain that turns a Rust crate into an npm-installable, browser-ready WebAssembly package. This is the build step that sits between `cargo` and your bundler.

---

## Quick Overview

`wasm-pack` is the build orchestrator for Rust-to-**WebAssembly** (**WASM**) projects: it compiles your crate to the `wasm32-unknown-unknown` target, runs **`wasm-bindgen`** to generate the JavaScript/TypeScript glue, optimizes the binary with `wasm-opt`, and emits a ready-to-publish package directory. For a TypeScript/JavaScript developer, think of it as the equivalent of `tsc` + a bundler plugin + `npm pack`, except the input is Rust and the output is a `.wasm` file with `.js` and `.d.ts` files wrapped around it. The single most important configuration decision is the crate's **`crate-type`** and the **build target** (`web`, `bundler`, or `nodejs`), which together determine the shape of the generated JavaScript glue.

> **Note:** This file covers project setup, the `cdylib` crate type, and choosing a build target. Actually writing and calling exported functions is covered in [Your First Rust → WebAssembly Module](/19-wasm/02-first-wasm/) and [Calling Rust from JavaScript](/19-wasm/04-rust-from-js/). For what WebAssembly is and when it beats plain JavaScript, start with [What Is WebAssembly and Why Compile Rust to It?](/19-wasm/00-wasm-intro/).

---

## TypeScript/JavaScript Example

In the JavaScript world, shipping a library to a browser or to npm is a familiar pipeline: write TypeScript, compile it with `tsc` (which emits `.js` plus `.d.ts` type declarations), and let a bundler or `npm publish` package it. The runtime (V8) is already present, and the toolchain is implicit in your `package.json` scripts.

```bash
# Scaffold a TypeScript library
mkdir greeter && cd greeter
npm init -y
npm install --save-dev typescript
npx tsc --init
```

```json
{
  "name": "greeter",
  "version": "0.1.0",
  "type": "module",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "files": ["dist"],
  "scripts": {
    "build": "tsc"
  },
  "devDependencies": {
    "typescript": "^5.6.0"
  }
}
```

```typescript
// src/index.ts
export function greet(name: string): string {
  return `Hello, ${name}! This greeting came from TypeScript.`;
}

// A CPU-bound function: count primes below `limit`.
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

```bash
npm run build
# tsc emits dist/index.js and dist/index.d.ts
```

Two things matter for the comparison ahead. First, `tsc` produces a `.js` file and a matching `.d.ts` declaration file: the artifact split that lets consumers get both runnable code and types. Second, the output target (CommonJS vs ES modules, which browsers/bundlers it suits) is controlled by `tsconfig.json`. `wasm-pack` mirrors both of these ideas, but the runnable artifact is a `.wasm` binary with a generated `.js` loader wrapped around it, and the "output target" is chosen with a `--target` flag instead of a config file.

---

## Rust Equivalent

First, install the tooling. You need the Rust toolchain (which you already have from [Section 01](/01-getting-started/01-installation/)), the `wasm32-unknown-unknown` compilation target, and the `wasm-pack` CLI:

```bash
# Add the WebAssembly compilation target to your toolchain
rustup target add wasm32-unknown-unknown

# Install the wasm-pack CLI (one-time, global)
cargo install wasm-pack
```

> **Note:** `wasm-pack` will also auto-download a matching `wasm-bindgen` CLI and `wasm-opt` on first build, so you do not install those separately. The version used while writing this guide was `wasm-pack 0.13.1` with `wasm-bindgen 0.2.122`; run `wasm-pack --version` and check the [releases page](https://github.com/drager/wasm-pack/releases) for newer versions, since the CLI evolves.

Now scaffold the crate. A WASM project is a **library** crate, not a binary, so use `--lib`:

```bash
cargo new --lib greeter
cd greeter
cargo add wasm-bindgen
```

The important edit is to `Cargo.toml`. You must declare the crate's `crate-type` as `cdylib` so the compiler emits a self-contained dynamic library, the format `wasm-bindgen` needs:

```toml
# Cargo.toml
[package]
name = "greeter"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2.122"
```

The library source exports functions with `#[wasm_bindgen]`:

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

Build it for the browser:

```bash
wasm-pack build --target web
```

The real output (versions and timings will differ on your machine):

```text
[INFO]:  Checking for the Wasm target...
[INFO]:  Compiling to Wasm...
    Finished `release` profile [optimized] target(s) in 1.74s
[INFO]:  Installing wasm-bindgen...
[INFO]: Optimizing wasm binaries with `wasm-opt`...
[INFO]: Optional fields missing from Cargo.toml: 'description', 'repository', and 'license'. These are not necessary, but recommended
[INFO]:   Done in 5.89s
[INFO]:   Your wasm pkg is ready to publish at /path/to/greeter/pkg.
```

This produces a `pkg/` directory, the analog of the `dist/` that `tsc` produced above:

```text
pkg/
├── greeter_bg.wasm        # the compiled WebAssembly binary (wasm-opt'd)
├── greeter_bg.wasm.d.ts   # TypeScript types for the raw wasm exports
├── greeter.js             # generated JS loader/glue (the part you import)
├── greeter.d.ts           # TypeScript declarations for the JS glue
├── package.json           # generated; makes pkg/ npm-installable
└── .gitignore
```

The `greeter.d.ts` is generated by `wasm-bindgen` from your Rust signatures. Your `greet(name: &str) -> String` becomes a TypeScript declaration automatically:

```typescript
// pkg/greeter.d.ts (excerpt, generated)
/**
 * Greets a user by name. Exported to JavaScript as `greet`.
 */
export function greet(name: string): string;

/**
 * CPU-bound work: count the primes below `limit`. ...
 */
export function count_primes(limit: number): number;
```

> **Tip:** Your Rust doc comments (`///`) flow straight into the generated `.d.ts` as JSDoc. Documenting the Rust function documents the TypeScript API for free.

---

## Detailed Explanation

### Why a `cdylib` crate type?

By default, a Rust library crate compiles to an **`rlib`**: Rust's own static library format, only understood by the Rust compiler and only useful when linked into another Rust crate. WebAssembly modules are loaded by a JavaScript host (the browser or Node), not by `rustc`, so you need a different output format.

`cdylib` stands for "C-compatible dynamic library." It tells the compiler to produce a standalone, self-contained module with a stable, language-agnostic export surface — exactly what a `.wasm` module is. On the `wasm32-unknown-unknown` target, `cdylib` produces the `.wasm` file that `wasm-bindgen` then post-processes.

The list `["cdylib", "rlib"]` asks for **both** outputs:

- `cdylib`: the WASM artifact that ships to JavaScript.
- `rlib`: the normal Rust library output, which lets you keep running `cargo test` on the host and depend on the crate from other Rust crates.

You can write just `crate-type = ["cdylib"]`, but then `cargo test` (which needs an `rlib`) will not work for this crate. Including `rlib` costs nothing and keeps your tests runnable on the host, so it is the standard recommendation.

> **Note:** This is conceptually the same idea as choosing `"module": "ESNext"` vs `"CommonJS"` in `tsconfig.json`: you are selecting an output format for a different consumer. Here the consumer is a WASM host instead of a JavaScript runtime.

### What `wasm-pack build` actually does

The `wasm-pack build` command is a pipeline, and each `[INFO]` line above is one stage:

1. **Checking for the Wasm target**: confirms `wasm32-unknown-unknown` is installed (the `rustup target add` step).
2. **Compiling to Wasm**: runs `cargo build --release --target wasm32-unknown-unknown` under the hood. The "release" profile is the default for `wasm-pack build`; that is why the binary is optimized.
3. **Installing wasm-bindgen**: downloads (once) the `wasm-bindgen` CLI matching the `wasm-bindgen` crate version in your `Cargo.toml`, then runs it on the raw `.wasm` to generate the JS/TS glue.
4. **Optimizing wasm binaries with `wasm-opt`**: shrinks and speeds up the binary using the Binaryen optimizer.
5. **Writing the package**: emits the `pkg/` directory with a generated `package.json`.

The raw `cargo build` step on its own would give you only `target/wasm32-unknown-unknown/release/greeter.wasm`: a binary with no JavaScript wrapper and a hard-to-call ABI. `wasm-pack` exists to wrap that into something a bundler or npm can consume.

### The two-file artifact: `.wasm` + `.js` glue

Unlike a `tsc` build where `dist/index.js` *is* your runnable code, a WASM package always has two layers:

- `greeter_bg.wasm`: the compiled machine-ish bytecode. It cannot be `import`ed directly with full ergonomics: strings, structs, and `Vec`s do not cross the JS↔WASM boundary natively (WASM only speaks numbers and linear memory).
- `greeter.js`: the **glue** that `wasm-bindgen` generates. It allocates memory, copies your JS string into the WASM module's linear memory, calls the raw export, reads the result back out, and decodes it into a JS string. The `_bg` suffix is short for `bindgen`; it marks the lower-level, wasm-facing module that the higher-level `greeter.js` wraps.

This is why even a one-line `greet` function needs the glue: the boundary marshalling is non-trivial, and `wasm-bindgen` writes it for you. The mechanics of that boundary are the subject of [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/).

### The generated `package.json`

`wasm-pack` writes a `package.json` into `pkg/` so the directory is immediately `npm install`-able (locally via a path, or publishable to a registry). For the `web` target it looks like this:

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

`name` and `version` are copied from your `Cargo.toml`. `main` and `types` point at the generated JS and declarations, exactly like the hand-written `package.json` in the TypeScript example, but produced for you.

---

## Key Differences

| Concern | TypeScript (`tsc` + bundler) | Rust (`wasm-pack`) |
| --- | --- | --- |
| Source | `.ts` files | a Rust library crate (`--lib`) |
| Output unit | `.js` (+ `.d.ts`) | `.wasm` + generated `.js` glue (+ `.d.ts`) |
| Crate/module config | `tsconfig.json` `module`/`target` | `Cargo.toml` `crate-type` + `wasm-pack --target` |
| Runtime present? | yes (V8 ships with Node/browser) | the WASM module must be fetched and instantiated |
| Type declarations | emitted by `tsc` | emitted by `wasm-bindgen` from Rust signatures |
| Package metadata | hand-written `package.json` | generated into `pkg/package.json` |
| Optimization | bundler minifier (terser/esbuild) | `wasm-opt` (built into the build) |

The biggest conceptual shift: in TypeScript the *output format* (ESM vs CJS) is a compiler setting; in Rust the *crate type* (`cdylib`) is a Cargo setting, while the *JavaScript flavor* (ESM for the web, CommonJS for Node, bundler-friendly imports) is the separate `wasm-pack --target` flag described next.

> **Note:** Unlike TypeScript, where types are erased and the runtime values are plain JavaScript, the `.wasm` binary is genuinely separate compiled code with its own linear memory. The `.d.ts` describes a *foreign* module, not the same code with annotations stripped.

---

## Choosing a Build Target

The `--target` flag decides what kind of JavaScript glue `wasm-bindgen` writes and therefore *where the package can be loaded*. The three you will use in practice are `web`, `bundler`, and `nodejs`. The crate, the Rust code, and the `.wasm` binary are identical across all three; only the glue and `package.json` change.

### `--target web`

```bash
wasm-pack build --target web
```

Generates ES-module glue that loads the `.wasm` with the browser's native `fetch` + `WebAssembly.instantiateStreaming`. The default export is an async `init()` function you call before using any exports. There is **no bundler required**, so you can `<script type="module">` it straight from a static page. This is the right choice for a plain HTML page or a framework that does not pre-process `.wasm` imports.

The generated `package.json` sets `"type": "module"` and lists `greeter.js` as `main`:

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

### `--target bundler`

```bash
wasm-pack build --target bundler
```

This is the **default** target if you omit `--target`. It generates ESM glue that uses an `import` of the `.wasm` file directly, relying on a bundler (Vite, webpack, Rollup) to resolve and serve the binary. It produces an extra `greeter_bg.js` file (the wasm-facing glue) and re-exports through `greeter.js`. There is no `init()` call to make; the bundler wires up instantiation. Use this when the package is consumed inside a webpack/Vite app.

Notice the differences in the generated `package.json`: an added `greeter_bg.js` in `files`, and `./greeter.js` listed in `sideEffects`:

```json
{
  "name": "greeter",
  "type": "module",
  "version": "0.1.0",
  "files": ["greeter_bg.wasm", "greeter.js", "greeter_bg.js", "greeter.d.ts"],
  "main": "greeter.js",
  "types": "greeter.d.ts",
  "sideEffects": ["./greeter.js", "./snippets/*"]
}
```

### `--target nodejs`

```bash
wasm-pack build --target nodejs
```

Generates CommonJS glue. The loader reads the `.wasm` file synchronously from disk with `require('fs').readFileSync` and instantiates it eagerly, so there is **no async `init()`** — you `require()` the package and call functions immediately. Use this for a Node.js script, a CLI, or a server-side dependency. The `package.json` notably does **not** set `"type": "module"`:

```json
{
  "name": "greeter",
  "version": "0.1.0",
  "files": ["greeter_bg.wasm", "greeter.js", "greeter.d.ts"],
  "main": "greeter.js",
  "types": "greeter.d.ts"
}
```

### Targets at a glance

| `--target` | Module system | How `.wasm` loads | `init()` needed? | Use when |
| --- | --- | --- | --- | --- |
| `web` | ESM | `fetch` + streaming instantiate | yes (async default export) | static HTML page, no bundler |
| `bundler` (default) | ESM | bundler resolves the `import` | no (bundler wires it) | inside Vite/webpack/Rollup |
| `nodejs` | CommonJS | `fs.readFileSync` (sync) | no (eager) | Node script, CLI, server |
| `no-modules` | global `wasm_bindgen` | manual | yes | legacy `<script>` (no `type="module"`) |
| `deno` | ESM | URL fetch | yes | Deno runtime |

> **Tip:** Build into different `--out-dir` directories when you need more than one target from the same crate, e.g. `wasm-pack build --target web --out-dir pkg-web` and `wasm-pack build --target nodejs --out-dir pkg-node`. The default `--out-dir` is `pkg`.

---

## Common Pitfalls

### Forgetting `crate-type = ["cdylib"]`

This is the number-one setup mistake. If your `Cargo.toml` has no `[lib]` `crate-type`, `wasm-pack` stops immediately with a precise error. Running the build on a crate that lacks it produces exactly this:

```text
Error: crate-type must be cdylib to compile to wasm32-unknown-unknown. Add the following to your Cargo.toml file:

[lib]
crate-type = ["cdylib", "rlib"]
```

The fix is in the message: add the `[lib]` section. (`wasm-pack` is unusually helpful here; it tells you the exact lines to paste.)

### Scaffolding a binary instead of a library

`cargo new greeter` (without `--lib`) creates a binary crate with `src/main.rs` and a `fn main()`. WASM modules built with `wasm-pack` are libraries: there is no `main` entry point, you export functions. Use `cargo new --lib greeter`, and put your code in `src/lib.rs`.

### Missing the `wasm32-unknown-unknown` target

If you skipped `rustup target add wasm32-unknown-unknown`, `wasm-pack` will detect it and offer to install it, but a raw `cargo build --target wasm32-unknown-unknown` would fail with `error[E0463]: can't find crate for 'std'` / a note that the target may not be installed. Install the target once per toolchain.

### Reaching for the wrong target

A package built with `--target nodejs` (CommonJS, synchronous `fs` load) will not work in a browser, and a `--target web` package's async `init()` flow will confuse a `require()`-based Node script. The Rust code is identical; the glue is not interchangeable. Decide where the package runs *before* you build, and rebuild with the right `--target` if you change your mind.

### Expecting to `import` the `.wasm` directly

You cannot meaningfully `import greet from './greeter_bg.wasm'` and get a working `String`-returning function; the raw module only exports numeric, memory-pointer-based functions. Always import from the generated `greeter.js`, never from the bare `.wasm`. The glue is the public API.

### Assuming `cargo install wasm-pack` needs extra tooling

Some older guides tell you to install `wasm-bindgen-cli` and `wasm-opt` separately. You do not; modern `wasm-pack` downloads matching versions of both on first build and caches them. Just install `wasm-pack` itself.

---

## Best Practices

- **Always include `rlib` alongside `cdylib`** (`crate-type = ["cdylib", "rlib"]`) so `cargo test` and host-side use keep working. Test your pure logic on the host where it is fast; reserve browser testing for the boundary.
- **Pick the target by destination, not by habit.** `web` for a static page, `bundler` (the default) inside Vite/webpack, `nodejs` for a server or CLI. Document the chosen target in your README so consumers know how to import it.
- **Build into named `--out-dir`s when you publish multiple targets** (`pkg-web`, `pkg-node`) instead of overwriting `pkg`.
- **Use `wasm-pack build --release` for shipping** (it is the default) and `--dev` only for fast local iteration; the dev profile skips optimizations and produces a much larger, slower binary.
- **Add the optional `description`, `repository`, and `license` fields** to `Cargo.toml` if you intend to publish — `wasm-pack` reminds you, and they flow into the generated `package.json`.
- **Keep `wasm-pack`, the `wasm-bindgen` crate, and the `wasm-bindgen` CLI in sync.** `wasm-pack` handles this automatically, but if you ever invoke `wasm-bindgen` by hand, a version mismatch between the crate and the CLI is a classic source of cryptic glue errors. Let `wasm-pack` own that for you.

---

## Real-World Example

A common production setup is a small Rust crate that does one CPU-heavy job, built for the browser via Vite. Here is the full, build-verified crate plus the loader page for the `web` target.

```toml
# Cargo.toml
[package]
name = "greeter"
version = "0.1.0"
edition = "2024"
description = "A tiny Rust→WASM greeting and prime-counting demo"
license = "MIT"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2.122"
```

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

/// Greets a user by name. Exported to JavaScript as `greet`.
#[wasm_bindgen]
pub fn greet(name: &str) -> String {
    format!("Hello, {name}! This greeting came from Rust + WebAssembly.")
}

/// CPU-bound work: count the primes below `limit`.
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

// Pure-logic tests still run on the host, thanks to the `rlib` crate type.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_primes_below_100() {
        assert_eq!(count_primes(100), 25);
    }
}
```

Build it for a static page:

```bash
wasm-pack build --target web --out-dir pkg
```

Then load and use it from an HTML page, no bundler required for the `web` target:

```html
<!-- index.html -->
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Rust + WASM</title>
  </head>
  <body>
    <pre id="out"></pre>
    <script type="module">
      import init, { greet, count_primes } from "./pkg/greeter.js";

      // The `web` target's default export is an async init() that
      // fetches and instantiates the .wasm before any export is callable.
      await init();

      const out = document.getElementById("out");
      out.textContent =
        greet("Ada") + "\n" + "primes below 100: " + count_primes(100);
    </script>
  </body>
</html>
```

> **Note:** Because the `web` target loads the `.wasm` with `fetch`, the page must be served over HTTP, not opened as a `file://` URL — browsers block `fetch` of local files. Any static server works; serving is covered in [Deploying WebAssembly Applications](/19-wasm/10-deployment/).

For a server-side consumer, the same crate built with `--target nodejs` is usable directly from Node. Building with `wasm-pack build --target nodejs --out-dir pkg-node` and running this script prints real output:

```javascript
// run.mjs
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const greeter = require("./pkg-node/greeter.js"); // CommonJS glue, eager-loaded

console.log(greeter.greet("Ada"));
console.log("primes below 100:", greeter.count_primes(100));
```

```text
Hello, Ada! This greeting came from Rust + WebAssembly.
primes below 100: 25
```

The same Rust source, two different targets, two different ways to load it, and the `count_primes(100) === 25` result is identical, computed inside the WASM module both times.

---

## Further Reading

### Official Documentation

- [The `wasm-pack` book](https://drager.github.io/wasm-pack/book/) — the canonical guide to `wasm-pack build`, targets, and publishing.
- [The `wasm-bindgen` Guide](https://wasm-bindgen.github.io/wasm-bindgen/) — what the generated glue does and how the boundary works.
- [Rust and WebAssembly book](https://rustwasm.github.io/docs/book/) — end-to-end project walkthrough.
- [`cdylib` and crate types — Cargo reference](https://doc.rust-lang.org/reference/linkage.html) — the linkage formats Rust can emit.
- [`wasm32-unknown-unknown` — rustc platform support](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html) — the WASM compilation target.

### Related Sections

- [Section 19 README](/19-wasm/) — the full WebAssembly section index.
- [What Is WebAssembly and Why Compile Rust to It?](/19-wasm/00-wasm-intro/) — what WASM is and when Rust→WASM beats plain JavaScript.
- [Your First Rust → WebAssembly Module](/19-wasm/02-first-wasm/) — write and call your first exported function.
- [Calling Rust from JavaScript](/19-wasm/04-rust-from-js/) — exporting functions/structs and the generated glue in detail.
- [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/) — the JS↔WASM boundary, `JsValue`, and types crossing it.
- [WebAssembly Performance](/19-wasm/09-performance/) — shrinking the `.wasm` with `wasm-opt`/`twiggy` and boundary costs.
- [Deploying WebAssembly Applications](/19-wasm/10-deployment/) — bundlers, serving `.wasm` with the right MIME type, CDNs.
- [Section 01: Installing Rust](/01-getting-started/01-installation/) — getting the toolchain and `rustup`.
- [Section 01: Cargo basics](/01-getting-started/03-cargo-basics/) — `Cargo.toml`, `cargo new`, and dependencies.
- [Section 02: Basics](/02-basics/) — types, variables, and output, used throughout the examples.
- [Section 20: Unsafe & FFI](/20-unsafe-ffi/) — `cdylib` also underpins C-ABI FFI, the non-WASM cousin of this setup.

---

## Exercises

### Exercise 1: Scaffold and build for the web

**Difficulty:** Beginner

**Objective:** Produce a working `pkg/` directory from scratch.

**Instructions:**

1. Install the WASM target and `wasm-pack` if you have not (`rustup target add wasm32-unknown-unknown`, `cargo install wasm-pack`).
2. Run `cargo new --lib texttools` and `cd` into it.
3. Add `wasm-bindgen`, set the `[lib]` `crate-type` to `["cdylib", "rlib"]`, and export a function `shout(text: &str) -> String` that returns the text uppercased.
4. Run `wasm-pack build --target web` and confirm `pkg/texttools.js`, `pkg/texttools.d.ts`, and `pkg/texttools_bg.wasm` all exist.

<details>
<summary>Solution</summary>

```bash
rustup target add wasm32-unknown-unknown   # one-time
cargo install wasm-pack                    # one-time
cargo new --lib texttools
cd texttools
cargo add wasm-bindgen
```

```toml
# Cargo.toml
[package]
name = "texttools"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2.122"
```

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

/// Returns `text` in uppercase. Exported to JS as `shout`.
#[wasm_bindgen]
pub fn shout(text: &str) -> String {
    text.to_uppercase()
}
```

```bash
wasm-pack build --target web
ls pkg/
# texttools.js  texttools.d.ts  texttools_bg.wasm  texttools_bg.wasm.d.ts  package.json
```

The generated `pkg/texttools.d.ts` will contain `export function shout(text: string): string;`, your Rust signature, translated.

</details>

### Exercise 2: Compare two targets

**Difficulty:** Intermediate

**Objective:** See how the `--target` flag changes the generated package without changing the Rust.

**Instructions:**

1. Using the `texttools` crate from Exercise 1, build it twice into separate directories: once for `web` (`--out-dir pkg-web`) and once for `nodejs` (`--out-dir pkg-node`).
2. Open both generated `package.json` files and identify two concrete differences.
3. Explain in one sentence why the `nodejs` build does not need an async `init()` call.

<details>
<summary>Solution</summary>

```bash
wasm-pack build --target web    --out-dir pkg-web
wasm-pack build --target nodejs --out-dir pkg-node
```

Two differences in the generated `package.json`:

- The `web` build sets `"type": "module"` (ES modules); the `nodejs` build omits it, so it is treated as CommonJS.
- The `web` build lists `"sideEffects": ["./snippets/*"]`; the `nodejs` build has no `sideEffects` field.

The `nodejs` glue loads the `.wasm` synchronously from disk with `require('fs').readFileSync` and instantiates it eagerly at `require()` time, so the exports are ready immediately — there is no asynchronous `fetch` to await, hence no `init()`. The `web` glue must `fetch` the binary over the network, which is inherently asynchronous, so it exposes an async `init()` you await first.

</details>

### Exercise 3: Diagnose a missing crate type

**Difficulty:** Intermediate

**Objective:** Recognize and fix the most common `wasm-pack` setup error.

**Instructions:**

1. Create `cargo new --lib brokenpkg`, add `wasm-bindgen`, and export a trivial `#[wasm_bindgen] pub fn ping() -> i32 { 1 }`. Do **not** add a `[lib]` `crate-type`.
2. Run `wasm-pack build --target web` and read the error.
3. Apply the fix the error suggests and rebuild successfully.

<details>
<summary>Solution</summary>

Step 2 fails with this exact message (real output from `wasm-pack 0.13.1`):

```text
Error: crate-type must be cdylib to compile to wasm32-unknown-unknown. Add the following to your Cargo.toml file:

[lib]
crate-type = ["cdylib", "rlib"]
```

The fix is to add the `[lib]` section the error prints, so `Cargo.toml` becomes:

```toml
[package]
name = "brokenpkg"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2.122"
```

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn ping() -> i32 {
    1
}
```

Rebuilding now succeeds and writes `pkg/`:

```bash
wasm-pack build --target web
# [INFO]:   Done in N.NNs
# [INFO]:   Your wasm pkg is ready to publish at .../brokenpkg/pkg.
```

</details>
