---
title: "Modules: From ES Modules to `mod`"
description: "In TypeScript every file is a module; in Rust a file does nothing until you attach it with mod. Build one explicit module tree per crate, private by default."
---

Coming from TypeScript, you organize code with files and `import`/`export`. Rust has files too, but its code-organization unit is the **module**, declared with the `mod` keyword. The mental shift is real: in Rust, **a file is not automatically a module**, and `mod` is not an import.

---

## Quick Overview

A Rust **module** is a named container for functions, types, constants, and other modules. You build a single tree of modules per **crate** (compilation unit), starting at a **crate root** (`src/main.rs` or `src/lib.rs`). Where ES modules treat every file as an independent module that you wire together with `import`, Rust requires you to *explicitly attach* each file into the tree with a `mod` declaration before any of its code is compiled or reachable.

> **Tip:** Read `mod foo;` as "the module `foo` lives in another file — go load it," and `mod foo { ... }` as "here is the module `foo`, inline." Neither one *imports* anything into scope; that is the job of `use` (covered in [The `use` Keyword](/12-modules-packages/02-use-keyword/)).

---

## TypeScript/JavaScript Example

In ES modules (the system Node v22 and modern TypeScript use), **the file *is* the module**. There is no separate declaration step: every file you create is automatically a module, and you connect them by importing the file path.

```typescript
// src/auth/session.ts
export function create(user: string): string {
  return `session-for-${user}`;
}

// src/auth.ts
import { create } from "./auth/session.js";

function verify(user: string): boolean {
  // Not exported -> private to this file/module.
  return user.length > 0;
}

export function login(user: string): boolean {
  console.log(`Logging in ${user}`);
  return verify(user);
}
export { create as createSession };

// src/main.ts
import { login, createSession } from "./auth.js";

const ok = login("alice");
const token = createSession("alice");
console.log(`ok=${ok}, token=${token}`);
```

**Key points about the ES module model:**

- Each file is a module the moment it exists; no declaration needed.
- `export` makes a binding visible; anything not exported is module-private.
- `import` pulls bindings into scope, using the *file path* as the module identity.
- The dependency graph is discovered by *following imports* from the entry file.

---

## Rust Equivalent

Here is the same `auth` / `session` structure in Rust, written **inline** (everything in one file) so you can see the whole module tree at a glance.

```rust playground
// src/main.rs — the crate root.

// `mod auth { ... }` defines a module named `auth` right here, inline.
mod auth {
    // `pub` exports this item out of `auth` (like TS `export`).
    pub fn login(user: &str) -> bool {
        println!("Logging in {user}");
        verify(user) // siblings call each other directly, no path needed
    }

    // No `pub` -> private to `auth`. Callers outside cannot reach it.
    fn verify(user: &str) -> bool {
        !user.is_empty()
    }

    // A child module, also inline. `pub mod` exports the module itself.
    pub mod session {
        pub fn create(user: &str) -> String {
            format!("session-for-{user}")
        }
    }
}

fn main() {
    // Reach into the tree with `::` paths. No `import`/`use` required —
    // a fully qualified path works on its own.
    let ok = auth::login("alice");
    let token = auth::session::create("alice");
    println!("ok={ok}, token={token}");
}
```

**Real output** (verified with `cargo run`):

```text
Logging in alice
ok=true, token=session-for-alice
```

When the tree grows, you split it across files. The next section shows exactly how.

---

## Detailed Explanation

### A crate has one module tree, rooted at the crate root

Every Rust crate is compiled as a single unit and has exactly one **crate root** file:

- A **binary** crate roots at `src/main.rs`.
- A **library** crate roots at `src/lib.rs`.

That root file is the implicit module called `crate`. Everything else hangs beneath it. So in the inline example above, the full paths are `crate::auth`, `crate::auth::login`, and `crate::auth::session::create`. (Path syntax — `crate::`, `super::`, `self::` — is the subject of [Paths in the Module Tree](/12-modules-packages/01-module-tree/).)

> **Note:** This is a big contrast with ES modules, where there is no single tree. Each file is its own module root and the "shape" of your code is just whatever graph the `import` statements happen to form. Rust always has one explicit, hierarchical tree per crate.

### `mod` declares, it does not import

This is the single most important sentence in this whole section:

> `mod foo;` tells the compiler **where the module `foo`'s code lives** and attaches it to the tree. It does **not** bring any of `foo`'s names into the current scope.

There are two forms:

```rust
mod auth { /* ... body of the module ... */ }   // inline: body is right here
mod auth;                                        // out-of-line: body is in another file
```

In TypeScript there is no equivalent of `mod`. A TS file becomes part of your program simply by being imported. In Rust, a `.rs` file that no one declares with `mod` is **completely ignored**: it is not even compiled.

### File-based modules: the modern layout

When you write `mod auth;` (no body), the compiler looks for the module's code in one of two places:

1. `src/auth.rs` (preferred, modern layout), or
2. `src/auth/mod.rs` (older layout, still valid).

A module's **child** modules go in a directory named after the parent. Here is the same `auth`/`session` example, split into files:

```rust
// src/main.rs — crate root.
// Attach the `auth` module; its code lives in src/auth.rs.
mod auth;

fn main() {
    let ok = auth::login("alice");
    let token = auth::session::create("alice");
    println!("ok={ok}, token={token}");
}
```

```rust
// src/auth.rs — the body of module `auth`.
// `auth` declares its own child module `session`,
// whose code lives in src/auth/session.rs.
pub mod session;

pub fn login(user: &str) -> bool {
    println!("Logging in {user}");
    verify(user)
}

fn verify(user: &str) -> bool {
    !user.is_empty()
}
```

```rust
// src/auth/session.rs — the body of module `auth::session`.
pub fn create(user: &str) -> String {
    format!("session-for-{user}")
}
```

The resulting file tree:

```text
src
├── auth
│   └── session.rs   // module crate::auth::session
├── auth.rs          // module crate::auth
└── main.rs          // crate root
```

**Real output** (verified with `cargo run` on exactly this layout):

```text
Logging in alice
ok=true, token=session-for-alice
```

Notice what the file paths do **not** dictate: the *module path* is `crate::auth::session`, which mirrors the declarations (`mod auth;` then `pub mod session;`), and it just so happens to line up with the directory structure. The directory layout follows the module tree, not the other way around.

> **Warning:** Older tutorials (pre-2018-edition) tell you to name every module file `mod.rs` and put it in a folder, e.g. `src/auth/mod.rs`. That still works, but the flat `src/auth.rs` + `src/auth/` layout is the current idiom and avoids a directory full of identical `mod.rs` files in your editor tabs. Use `cargo new` (which selects the latest stable edition, 2024) and prefer `auth.rs`.

### Inline vs file-based: same tree, different physical storage

The inline and file-based versions above produce an **identical** module tree. They are interchangeable; you choose based on size:

- **Inline** (`mod foo { ... }`) keeps small, closely related code together. It is also the standard place for unit tests: `#[cfg(test)] mod tests { ... }` (see [Testing](/13-testing/)).
- **File-based** (`mod foo;` + `foo.rs`) keeps large modules in their own files.

You can freely nest the two: an inline module can declare a file-based child and vice versa.

### Privacy is per-module and defaults to private

Every item (function, struct, module, etc.) is **private by default** and visible only within its own module and that module's descendants. `pub` opens it up one boundary at a time. In the example, `verify` has no `pub`, so `auth::login` can call it but `main` cannot. This is the inverse of TypeScript, where a top-level binding is module-private until you `export` it. Same idea, but Rust applies it at *every* nesting level, the file boundary being just one of them. Visibility levels (`pub`, `pub(crate)`, `pub(super)`) get full treatment in [Visibility and the `pub` Keyword](/12-modules-packages/03-pub-visibility/).

---

## Key Differences

| Concept | TypeScript / ES Modules | Rust |
| --- | --- | --- |
| Unit of organization | The file | The **module** (`mod`) |
| How a file joins the program | Automatically, when imported | Only when declared with `mod` |
| What `mod` does | (no equivalent) | Declares a module + attaches it to the tree |
| Bringing names into scope | `import { x } from "..."` | `use crate::path::x;` (see [The `use` Keyword](/12-modules-packages/02-use-keyword/)) |
| Module identity | The file path | The position in the crate's module tree |
| Number of roots | One module per file (many roots) | One tree per crate, single root |
| Default visibility | Private until `export` | Private until `pub`, at *every* level |
| Nested modules | Folders + re-export files | First-class: `mod a { mod b { ... } }` |
| Cross-package boundary | npm package name | crate name (see [Cargo.toml](/12-modules-packages/04-cargo/)) |

### The mental model in one picture

```text
ES modules (TypeScript):              Rust:
                                      crate (= src/main.rs or src/lib.rs)
file ─import→ file ─import→ file       └── mod auth
   (a graph discovered by imports)         ├── fn login (pub)
                                           ├── fn verify (private)
                                           └── mod session
                                               └── fn create (pub)
                                      (one explicit tree, built by `mod`)
```

> **Note:** A useful analogy: Rust's `mod` is closer to a TypeScript **namespace** (`namespace Foo { export function bar() {} }`) than to a file: both create an explicit named scope and use a path (`Foo.bar` / `auth::session::create`) to reach inside. But TS namespaces are discouraged in modern code, while Rust modules are *the* idiom. Don't lean on the analogy too hard.

---

## Common Pitfalls

### Pitfall 1: Expecting a `.rs` file to load itself (the ES-module reflex)

You create `src/utils.rs` and try to use it directly, the way you'd just `import` a TS file:

```rust
// src/main.rs
fn main() {
    // does not compile (error[E0433]): `utils` was never declared.
    let s = utils::shout("hi");
    println!("{s}");
}
```

```rust
// src/utils.rs
pub fn shout(s: &str) -> String {
    s.to_uppercase()
}
```

**Real compiler error:**

```text
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `utils`
 --> src/main.rs:3:13
  |
3 |     let s = utils::shout("hi");
  |             ^^^^^ use of unresolved module or unlinked crate `utils`
  |
help: to make use of source file src/utils.rs, use `mod utils` in this file to declare the module
  |
2 + mod utils;
  |
```

**Fix:** add `mod utils;` to `src/main.rs`. The file exists on disk, but until it is *declared* it is invisible to the compiler.

### Pitfall 2: Declaring a module whose file does not exist

The mirror image of Pitfall 1: you write `mod widgets;` but never create `src/widgets.rs`.

```rust
// src/main.rs
mod widgets; // does not compile (error[E0583]): no src/widgets.rs

fn main() {
    println!("{}", widgets::render());
}
```

**Real compiler error:**

```text
error[E0583]: file not found for module `widgets`
 --> src/main.rs:2:1
  |
2 | mod widgets;
  | ^^^^^^^^^^^^
  |
  = help: to create the module `widgets`, create file "src/widgets.rs" or "src/widgets/mod.rs"
  = note: if there is a `mod widgets` elsewhere in the crate already, import it with `use crate::...` instead
```

**Fix:** create `src/widgets.rs` (or `src/widgets/mod.rs`), or remove the declaration.

### Pitfall 3: Forgetting that `pub` is per-boundary, not "public to the whole program"

A `pub` item is only reachable if **every** module on the path to it is also reachable. Here `compute` is private to `bank`, so even though `bank` is reachable from `main`, the function is not:

```rust
mod bank {
    pub fn balance() -> u64 {
        compute() // fine: same module
    }
    fn compute() -> u64 {
        100
    }
}

fn main() {
    // does not compile (error[E0603]): compute() is private to `bank`.
    let raw = bank::compute();
    println!("{raw}");
}
```

**Real compiler error:**

```text
error[E0603]: function `compute` is private
  --> src/main.rs:12:21
   |
12 |     let raw = bank::compute();
   |                     ^^^^^^^ private function
   |
note: the function `compute` is defined here
  --> src/main.rs:5:5
   |
 5 |     fn compute() -> u64 {
   |     ^^^^^^^^^^^^^^^^^^^
```

**Fix:** call the public API (`bank::balance()`), or add `pub` to `compute` if it genuinely belongs in the public surface. See [Visibility and the `pub` Keyword](/12-modules-packages/03-pub-visibility/) for the finer-grained `pub(crate)` and `pub(super)`.

### Pitfall 4: Declaring the same module twice or expecting "barrel files"

In TypeScript you often write a barrel `index.ts` that re-exports a folder. In Rust, declaring `mod foo;` more than once in the same parent is an error, and there is no implicit `index.rs`. To create a curated public surface, declare the modules once and **re-export** their items with `pub use` (covered in [The `use` Keyword](/12-modules-packages/02-use-keyword/)). That is the idiomatic "barrel."

---

## Best Practices

### 1. Let the module tree mirror your domain, not your file count

Group by concept (`auth`, `orders`, `catalog`), not by mechanical type buckets (`functions`, `structs`). Start inline; promote a module to its own file only when it grows.

```rust
// Good: small helper stays inline next to what it serves.
mod metrics {
    pub fn record(name: &str) { /* ... */ }
}
```

### 2. Keep the crate root thin

A `main.rs` or `lib.rs` that is mostly `mod` declarations plus a small `main`/public API is easy to scan: it is your table of contents.

```rust
// src/lib.rs
pub mod catalog;
pub mod orders;
mod pricing; // private implementation detail, not part of the public API
```

### 3. Prefer `foo.rs` over `foo/mod.rs`

The flat layout is the current idiom and avoids many same-named tabs. Reserve `mod.rs` only when working in a codebase that already uses it.

### 4. Make modules private by default; widen deliberately

Start every module and item private. Add `pub` (or the narrower `pub(crate)`) only when something genuinely crosses the boundary. This keeps your public API small and your refactors local: exactly the discipline that the [Visibility and the `pub` Keyword](/12-modules-packages/03-pub-visibility/) section formalizes.

### 5. Use inline `#[cfg(test)] mod tests` for unit tests

Co-locating tests with the code they exercise is the Rust convention; the `#[cfg(test)]` attribute means the module is compiled only during `cargo test`. See [Testing](/13-testing/).

---

## Real-World Example

A small order-processing **library crate**, organized into a realistic module tree: a public `catalog` and `orders` API, plus a *private* `pricing` module that is an implementation detail and never leaves the crate.

```rust
// src/lib.rs — the crate root (library crate).
//! A tiny order-processing library, organized into modules.

pub mod catalog;
pub mod orders;

// Private module: an internal detail, NOT exported from the crate.
mod pricing;

#[cfg(test)]
mod tests {
    use super::*; // bring the crate root's items into the test module

    #[test]
    fn totals_apply_tax() {
        let cart = vec![
            orders::LineItem { sku: "BOOK".into(), qty: 2 },
            orders::LineItem { sku: "PEN".into(), qty: 5 },
        ];
        let total = orders::total_cents(&cart);
        // 2 * 1200 + 5 * 150 = 3150, +8% tax = 3402
        assert_eq!(total, 3402);
    }
}
```

```rust
// src/catalog.rs — module crate::catalog (public).
//! Product catalog lookups.

/// Returns the unit price in cents for a known SKU.
pub fn unit_price_cents(sku: &str) -> u32 {
    match sku {
        "BOOK" => 1200,
        "PEN" => 150,
        _ => 0,
    }
}
```

```rust
// src/orders.rs — module crate::orders (public).
//! Order modeling and totals.

use crate::catalog;
use crate::pricing; // reach a *private* sibling via an absolute path

pub struct LineItem {
    pub sku: String,
    pub qty: u32,
}

/// Sum the cart, then apply tax. Uses the private `pricing` module.
pub fn total_cents(items: &[LineItem]) -> u32 {
    let subtotal: u32 = items
        .iter()
        .map(|item| catalog::unit_price_cents(&item.sku) * item.qty)
        .sum();
    pricing::with_tax(subtotal)
}
```

```rust
// src/pricing.rs — module crate::pricing (private to the crate).
//! Internal pricing rules — not part of the public API.

const TAX_PERCENT: u32 = 8;

// `pub(crate)` = visible everywhere in THIS crate, but not to consumers.
pub(crate) fn with_tax(subtotal_cents: u32) -> u32 {
    subtotal_cents + subtotal_cents * TAX_PERCENT / 100
}
```

File tree:

```text
src
├── catalog.rs   // pub mod catalog
├── lib.rs       // crate root
├── orders.rs    // pub mod orders
└── pricing.rs   // mod pricing (private)
```

**Real output** (verified with `cargo test`):

```text
running 1 test
test tests::totals_apply_tax ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The takeaway: `orders` depends on `catalog` and `pricing`, but a *consumer* of this library can only see `catalog` and `orders`. The `pricing` module — even though `orders` calls into it — stays sealed inside the crate because it was declared `mod pricing;` without `pub`, and its function is `pub(crate)`. That is the module system doing exactly the API-shaping job that TypeScript leaves to convention and barrel files.

---

## Further Reading

### Official Documentation

- [The Rust Book — Defining Modules to Control Scope and Privacy](https://doc.rust-lang.org/book/ch07-02-defining-modules-to-control-scope-and-privacy.html)
- [The Rust Book — Separating Modules into Different Files](https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html)
- [The Rust Reference — Modules](https://doc.rust-lang.org/reference/items/modules.html)
- [Rust by Example — Modules](https://doc.rust-lang.org/rust-by-example/mod.html)

### Related Topics in This Guide

- [Paths: `crate::`, `super::`, `self::`](/12-modules-packages/01-module-tree/): how to address items in the tree.
- [The `use` keyword](/12-modules-packages/02-use-keyword/) — bringing names into scope, re-exports, glob.
- [Visibility with `pub`](/12-modules-packages/03-pub-visibility/): `pub`, `pub(crate)`, `pub(super)`, field visibility.
- [Cargo and `Cargo.toml`](/12-modules-packages/04-cargo/) — how crates (the next boundary up from modules) are configured.
- [Cargo commands](/12-modules-packages/05-cargo-commands/): `cargo new`, `build`, `run`, `test`.
- [Testing](/13-testing/) — `#[cfg(test)] mod tests` in depth.

### Foundations

- [Getting Started](/01-getting-started/): crates, Cargo, and the `main.rs` entry point.
- [Basics](/02-basics/) — the syntax used in these examples.

---

## Exercises

### Exercise 1: Build an inline module tree

**Difficulty:** Beginner

**Objective:** Practice declaring nested inline modules and reaching into them with `::` paths.

**Instructions:** Create a `geometry` module with a child module `shapes`. `shapes` should expose a public `describe()` that returns `"circle, square, triangle"`. `geometry` should expose a public `summary()` that calls `shapes::describe()` and returns `"Shapes: circle, square, triangle"`. From `main`, print both `geometry::summary()` and `geometry::shapes::describe()`.

```rust playground
fn main() {
    // TODO: build the geometry / shapes module tree above this,
    // then print summary() and shapes::describe().
}
```

<details>
<summary>Solution</summary>

```rust playground
mod geometry {
    pub mod shapes {
        pub fn describe() -> &'static str {
            "circle, square, triangle"
        }
    }

    pub fn summary() -> String {
        format!("Shapes: {}", shapes::describe())
    }
}

fn main() {
    println!("{}", geometry::summary());
    println!("{}", geometry::shapes::describe());
}
```

**Output:**

```text
Shapes: circle, square, triangle
circle, square, triangle
```

</details>

### Exercise 2: Split a struct module into its own file

**Difficulty:** Intermediate

**Objective:** Convert an idea into a file-based module that owns a type with a private field and a public API.

**Instructions:** In a fresh `cargo new` project, declare `mod inventory;` from `main.rs`. In `src/inventory.rs`, define a `pub struct Store` that holds a *private* `HashMap<String, u32>` field. Give it `pub fn new()`, `pub fn add(&mut self, name: &str, qty: u32)` (which accumulates), and `pub fn count(&self, name: &str) -> u32` (which returns 0 for missing items). From `main`, add a few items and print the counts.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
mod inventory;

fn main() {
    let mut store = inventory::Store::new();
    store.add("apple", 3);
    store.add("apple", 2);
    store.add("pear", 1);
    println!("apples: {}", store.count("apple"));
    println!("pears: {}", store.count("pear"));
    println!("missing: {}", store.count("kiwi"));
}
```

```rust
// src/inventory.rs
use std::collections::HashMap;

pub struct Store {
    items: HashMap<String, u32>, // private field — only `Store`'s methods touch it
}

impl Store {
    pub fn new() -> Self {
        Store { items: HashMap::new() }
    }

    pub fn add(&mut self, name: &str, qty: u32) {
        let entry = self.items.entry(name.to_string()).or_insert(0);
        *entry += qty;
    }

    pub fn count(&self, name: &str) -> u32 {
        self.items.get(name).copied().unwrap_or(0)
    }
}
```

**Output:**

```text
apples: 5
pears: 1
missing: 0
```

</details>

### Exercise 3: A two-level file-based module tree

**Difficulty:** Intermediate

**Objective:** Lay out a parent module file plus a child module in a subdirectory, with the child as a shared private-ish helper.

**Instructions:** Declare `mod temperature;` from `main.rs`. Module `temperature` (in `src/temperature.rs`) exposes `pub fn celsius_to_fahrenheit(c: f64) -> f64` and `pub fn fahrenheit_to_celsius(f: f64) -> f64`. It has a child module `conversions` (in `src/temperature/conversions.rs`) with a `pub fn scale_and_shift(value, factor, offset)` helper that both public functions call. From `main`, convert 100 C and 32 F and print the results.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
mod temperature;

fn main() {
    let f = temperature::celsius_to_fahrenheit(100.0);
    println!("100C = {f}F");
    let c = temperature::fahrenheit_to_celsius(32.0);
    println!("32F = {c}C");
}
```

```rust
// src/temperature.rs
pub mod conversions;

pub fn celsius_to_fahrenheit(c: f64) -> f64 {
    conversions::scale_and_shift(c, 9.0 / 5.0, 32.0)
}

pub fn fahrenheit_to_celsius(f: f64) -> f64 {
    conversions::scale_and_shift(f - 32.0, 5.0 / 9.0, 0.0)
}
```

```rust
// src/temperature/conversions.rs
pub fn scale_and_shift(value: f64, factor: f64, offset: f64) -> f64 {
    value * factor + offset
}
```

File tree:

```text
src
├── main.rs
├── temperature
│   └── conversions.rs
└── temperature.rs
```

**Output:**

```text
100C = 212F
32F = 0C
```

</details>
