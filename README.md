# Rust for TS/JS Developers

A comprehensive, hands-on guide to learning Rust for developers coming from TypeScript or JavaScript. It uses side-by-side code comparisons so you can leverage what you already know while learning what makes Rust different.

> 📖 **This guide is published as a website** built with [Astro Starlight](https://starlight.astro.build/) — searchable, with a chapter sidebar, "Edit on GitHub" links, and TypeScript↔Rust syntax highlighting.
>
> **Live site:** **[rs4ts.dev](https://rs4ts.dev)** &nbsp;•&nbsp; **Or read it offline:** run locally with the two commands below, or browse the Markdown under [`src/content/docs/`](./src/content/docs/).

```bash
npm install      # Node 22.12+ (see .nvmrc)
npm run dev      # open http://localhost:4321
```

---

## 🎯 Who This Guide Is For

- **TypeScript/JavaScript developers** with 1+ years of experience
- Developers wanting to learn **systems programming**
- Those curious about **high-performance backends**
- Anyone transitioning from **interpreted to compiled languages**

**Prerequisites:** Solid TypeScript/JavaScript, familiarity with npm/package managers, basic programming concepts. No Rust knowledge required.

---

## 🚀 Why Rust?

```typescript
// TypeScript — safe under a strict configuration after boundary validation
interface Item { value: number }
function processData(data: readonly Item[]): number[] {
  return data.map((item) => item.value * 2);
}
```

```rust
// Rust — compile-time safety, zero-cost abstractions, no GC pauses
struct Item { value: i32 }

fn process_data(data: &[Item]) -> Vec<i32> {
    data.iter().map(|item| item.value * 2).collect()
    // Safe Rust prevents null dereferences, use-after-free, and data races
}
```

**Rust offers:**

- 🔒 **Memory safety without garbage collection**
- ⚡ **Performance comparable to C/C++**
- 🛡️ **Compile-time error prevention**
- 🔄 **Fearless concurrency**
- 📦 **Modern tooling** (Cargo = npm + webpack + more)
- 🌐 **Great for web, CLI, systems, embedded, WASM**

---

## 📑 Chapters

The guide is 31 chapters (00–30). Each is a folder under [`src/content/docs/`](./src/content/docs/); the links below open each chapter's intro. For the full navigable, searchable experience, read the website (or `npm run dev`).

### 📘 Foundation

- [00 — Introduction](./src/content/docs/00-introduction/index.md) — start here
- [01 — Getting Started](./src/content/docs/01-getting-started/index.md) — installation, first program, Cargo
- [02 — Basics](./src/content/docs/02-basics/index.md) — variables, types, operators
- [03 — Functions](./src/content/docs/03-functions/index.md) — functions, closures, higher-order functions
- [04 — Control Flow](./src/content/docs/04-control-flow/index.md) — if/else, loops, match
- [05 — Ownership](./src/content/docs/05-ownership/index.md) ⭐ **CRITICAL** — the ownership system

### 🔧 Core Language

- [06 — Data Structures](./src/content/docs/06-data-structures/index.md) — structs, enums, pattern matching
- [07 — Collections](./src/content/docs/07-collections/index.md) — Vec, String, HashMap, iterators
- [08 — Error Handling](./src/content/docs/08-error-handling/index.md) — Result, Option, `?`
- [09 — Generics & Traits](./src/content/docs/09-generics-traits/index.md) — generics, traits, trait objects
- [10 — Smart Pointers](./src/content/docs/10-smart-pointers/index.md) — Box, Rc, Arc, RefCell

### ⚡ Async & Organization

- [11 — Async](./src/content/docs/11-async/index.md) — async/await, Tokio, Futures
- [12 — Modules & Packages](./src/content/docs/12-modules-packages/index.md) — modules, Cargo, crates
- [13 — Testing](./src/content/docs/13-testing/index.md) — unit, integration, benchmarks
- [14 — Macros](./src/content/docs/14-macros/index.md) — declarative and procedural macros

### 🌐 Practical Skills

- [15 — Serialization](./src/content/docs/15-serialization/index.md) — Serde, JSON, other formats
- [16 — Web APIs](./src/content/docs/16-web-apis/index.md) — REST APIs with Axum
- [17 — Database](./src/content/docs/17-database/index.md) — SQLx, Diesel, MongoDB, Redis
- [18 — CLI Tools](./src/content/docs/18-cli-tools/index.md) — command-line applications
- [19 — WebAssembly](./src/content/docs/19-wasm/index.md) — Rust in the browser

### 🎯 Advanced Topics

- [20 — Unsafe & FFI](./src/content/docs/20-unsafe-ffi/index.md) — unsafe Rust, calling C, Node.js addons
- [21 — Performance](./src/content/docs/21-performance/index.md) — profiling, optimization, benchmarking
- [22 — Common Patterns](./src/content/docs/22-common-patterns/index.md) — design patterns in Rust
- [23 — Ecosystem](./src/content/docs/23-ecosystem/index.md) — popular crates and libraries
- [24 — Tooling](./src/content/docs/24-tooling/index.md) — Cargo, rustfmt, clippy, rust-analyzer
- [25 — Advanced Topics](./src/content/docs/25-advanced-topics/index.md) — PhantomData, Pin, const generics
- [26 — Systems Programming](./src/content/docs/26-systems-programming/index.md) — threads, atomics, low-level

### 🚀 Production Ready

- [27 — Security](./src/content/docs/27-security/index.md) — security best practices
- [28 — Production](./src/content/docs/28-production/index.md) — deployment, monitoring, scaling
- [29 — Migration Guide](./src/content/docs/29-migration-guide/index.md) — migrating from Node.js to Rust
- [30 — Complete Projects](./src/content/docs/30-projects/index.md) — 6 full applications

---

## 📖 How to Use This Guide

Each **concept topic** (chapters 01–29) follows a fixed 10-part format:

1. **Quick Overview** — what you'll learn and why it matters
2. **TypeScript/JavaScript Example** — familiar code you already know
3. **Rust Equivalent** — the Rust way of doing things
4. **Detailed Explanation** — line-by-line breakdown
5. **Key Differences** — important conceptual changes
6. **Common Pitfalls** — mistakes TS/JS devs typically make
7. **Best Practices** — idiomatic Rust approaches
8. **Real-World Example** — integrated code plus production considerations
9. **Further Reading** — additional resources
10. **Exercises** — practice problems with collapsible solutions

> **Note:** The orientation pages in [Chapter 00](./src/content/docs/00-introduction/index.md) and the build-along capstone projects in [Chapter 30](./src/content/docs/30-projects/index.md) use a format tailored to their purpose, so they don't carry every part above.

**Suggested paths:** 🏃 Quick (70–85h): orientation, then 01–05, 08, 11, 16 · 🚶 Standard (180–230h): 00–19, skim 20–26, build two projects · 🎓 Complete (300–400h): all 31 chapters + six projects. These ranges include the reading, practice, and exercise estimates published by the chapters; your pace will vary.

---

## 🗂️ Project Structure

```
rs4ts/
├─ src/content/docs/      # the 31 chapters (Markdown) — the site content
│  ├─ index.mdx           # landing page
│  └─ NN-section/         # each chapter: index.md + NN-topic.md pages
├─ examples/              # 6 runnable Rust capstone crates
│  ├─ rest-api-code/  cli-tool-code/  microservice-code/
│  └─ websocket-chat-code/  wasm-app-code/  full-stack-code/
├─ astro.config.mjs       # Starlight config (sidebar, fonts, theme)
├─ src/content.config.ts  # content collection schema
├─ src/styles/custom.css  # brand (IBM Plex, Rust-orange accent)
└─ public/                # favicon, robots.txt
```

### Running locally

```bash
npm install        # Node 22.12+ (nvm: `nvm use`)
npm run dev        # dev server at http://localhost:4321
npm run build      # production build into dist/
npm run preview    # preview the production build
```

### The example crates

Each crate under [`examples/`](./examples/) is a standalone Rust project:

```bash
cd examples/rest-api-code
cargo run          # (wasm-app-code targets wasm32 — see its README)
```

---

## 💡 Key Differences for TS/JS Developers

| Concept               | TypeScript/JavaScript                          | Rust                                      |
| --------------------- | ---------------------------------------------- | ----------------------------------------- |
| **Memory Management** | Garbage collection                             | Ownership system (compile-time)           |
| **Null Safety**       | Union types checked with `strictNullChecks`    | `Option<T>`; ordinary references non-null |
| **Error Handling**    | Exceptions; result unions/libraries by choice  | `Result<T, E>`; panics still exist         |
| **Mutability**        | Reassignable bindings and mutable objects      | Immutable bindings by default; `mut` explicit |
| **Type System**       | Structural, erased; strictness configurable    | Mostly nominal, compiled and mandatory    |
| **Concurrency**       | Event loop plus Web/Node worker threads        | OS threads plus optional async runtimes   |
| **Runtime**           | JavaScript VM/runtime (V8, Node.js, Deno)      | Native binary; no VM or garbage collector |
| **Package Manager**   | npm, yarn, pnpm                                | Cargo (built-in)                          |
| **Interfaces**        | `interface`, type aliases                      | traits                                    |
| **Async**             | eager `Promise`, async/await                   | lazy `Future`, async/await                 |
| **Runtime Speed**     | JIT; workload-dependent                        | AOT; workload-dependent                   |
| **Learning Curve**    | Moderate                                       | Steep (ownership is new)                  |

### The ownership mental model — **the** idea that makes Rust different

```typescript
// TypeScript — shared references, GC cleans up
let data = { value: 42 };
let data2 = data; // both reference the same object
console.log(data.value); // 42 — still accessible
```

```rust
// Rust — ownership moved, tracked at compile time
let data = String::from("hello");
let data2 = data;          // ownership moved to data2
// println!("{}", data);   // ❌ compile error: data no longer valid
println!("{}", data2);     // ✅ data2 owns it
```

This compile-time tracking prevents use-after-free, double-free, data races, and null-pointer dereferences — **without a garbage collector**.

---

## 🛠️ Installing Rust

You only need Rust to run the example crates; the guide itself reads in the browser.

```bash
# Install Rust (includes Cargo)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustc --version && cargo --version
```

---

## 🎓 Further Learning

- [The Rust Book](https://doc.rust-lang.org/book/) — the official guide
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/) — learn by doing
- [Rustlings](https://github.com/rust-lang/rustlings) — small exercises
- [Rust Playground](https://play.rust-lang.org/) — try Rust in the browser
- [r/rust](https://www.reddit.com/r/rust/) · [Rust Users Forum](https://users.rust-lang.org/) · [This Week in Rust](https://this-week-in-rust.org/)

---

## 🤝 Contributing

Found an error, typo, or have a suggestion? Contributions are welcome — see [CONTRIBUTING.md](./CONTRIBUTING.md). Every page has an **Edit on GitHub** link on the site. Starring the repo ⭐ helps others find it.

---

## 📊 Status

All 31 chapters (00–30) are complete. The six capstone crates are locked and checked in CI with the repository's pinned Rust toolchain: formatting, Clippy, native tests, and `wasm32-unknown-unknown` checks where relevant. A deterministic subset of self-contained standard-library page programs is also compiled by the documentation snippet check; fragments, exercises, external-dependency examples, and intentionally non-compiling snippets are excluded explicitly. See the [version and verification policy](./src/content/docs/00-introduction/05-version-policy.md) for the exact scope.

The guide is **330 Markdown pages** across the 31 chapters, plus **6 runnable example crates**.

---

## 📄 License

Licensed under the [MIT License](./LICENSE).

## 🙏 Acknowledgments

- Inspired by the "X for Y developers" comparison-guide format
- Thanks to the Rust community for excellent documentation
- Built with feedback from TypeScript/JavaScript developers learning Rust

---

<div align="center">

**Ready to start your Rust journey?**

### 👉 [Begin with Chapter 00 — Introduction](./src/content/docs/00-introduction/index.md) 👈

Made with ❤️ for developers, by developers.

</div>
