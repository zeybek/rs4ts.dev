---
title: "Paths in the Module Tree"
description: "TypeScript imports point at files on disk; Rust paths point at nodes in the module tree with ::. Learn crate::, super::, self::, and absolute vs relative paths."
---

In TypeScript you reach another file's exports with a string path like `import { x } from "../utils/math"`. Rust has no such file-path strings in its source: instead, everything you can name lives in a single **module tree**, and you address items with `::`-separated **paths** rooted in that tree. This file is about how to write those paths — `crate::`, `super::`, `self::`, and the absolute-versus-relative choice.

---

## Quick Overview

A Rust crate is one big tree of modules, with the crate root (`src/main.rs` or `src/lib.rs`) as the trunk. Every function, struct, constant, and submodule has a **path** through that tree, written with `::` between segments — the way `/` separates folders in a TypeScript import. The key skill is choosing between an **absolute path** (starts at `crate::`) and a **relative path** (starts at `self::`, `super::`, or a name in the current module), and knowing what each `::` segment means.

> **Note:** This file covers *how to write paths*. The mechanics of declaring modules (`mod`, inline vs file-based) live in [Modules](/12-modules-packages/00-modules/), bringing paths into scope with `use` lives in [The `use` Keyword](/12-modules-packages/02-use-keyword/), and making items reachable with `pub` lives in [Visibility and the `pub` Keyword](/12-modules-packages/03-pub-visibility/). You will see all three here in passing because paths only resolve to *visible* items.

---

## TypeScript/JavaScript Example

In a TypeScript project, you locate code in *other files* with a string path, and code in the *same file* just by name. The string is relative to the current file's location on disk.

```typescript
// src/store/checkout.ts

// Relative path: "../" walks up one directory on disk, "./" stays here.
import { TAX_RATE } from "../store/config";
import { findItem } from "./inventory";

// Bare specifier (no "./" or "../"): resolved from node_modules — an external package.
import { randomUUID } from "crypto";

export function total(itemName: string): number {
  const item = findItem(itemName);
  return item.price * (1 + TAX_RATE);
}

export function describe(itemName: string): string {
  // Something defined in THIS file is referenced by bare name.
  const cost = total(itemName);
  return `${itemName} costs $${cost.toFixed(2)} (receipt ${randomUUID()})`;
}
```

**Key points:**

- `./` and `../` are **relative to the file on disk**.
- A bare specifier (`"crypto"`, `"lodash"`) means "look in `node_modules`": an external package.
- Same-file references need no path at all, just the identifier.

---

## Rust Equivalent

Rust expresses the same relationships, but the "coordinates" are positions in the **module tree**, not directories on disk. The separator is `::`, not `/`.

```rust playground
mod store {
    // A constant living at `crate::store::TAX_RATE`.
    pub const TAX_RATE: f64 = 0.08;

    pub mod inventory {
        #[derive(Debug)]
        pub struct Item {
            pub name: String,
            pub price: f64,
        }

        pub fn find(name: &str) -> Item {
            Item { name: name.to_string(), price: 9.99 }
        }
    }

    pub mod checkout {
        // RELATIVE path: `inventory` is a sibling of `checkout` under `store`,
        // so we go up one level with `super::`.
        use super::inventory::{self, Item};

        // ABSOLUTE path: starts at the crate root with `crate::`.
        use crate::store::TAX_RATE;

        pub fn describe(item_name: &str) -> String {
            let item: Item = inventory::find(item_name);
            let cost = item.price * (1.0 + TAX_RATE);
            format!("{} costs ${:.2} with tax", item.name, cost)
        }
    }
}

// At the crate root we reach into the tree with an absolute path.
use crate::store::checkout;

fn main() {
    println!("{}", checkout::describe("Coffee Mug"));
    // A fully-qualified absolute path also works inline:
    println!("Tax rate: {}", crate::store::TAX_RATE);
}
```

**Real output** (compiled and run with `cargo run`):

```text
Coffee Mug costs $10.79 with tax
Tax rate: 0.08
```

**Key points:**

- `crate::` is the absolute root, like an import path anchored at the project root.
- `super::` is "go up one parent module," the rough analog of `../`, but it climbs the *module* tree, not the *directory* tree.
- `self::` is "the current module" — the rough analog of `./`.
- Items in the same module are reachable by bare name, just like same-file references in TypeScript.

---

## Detailed Explanation

### The module tree, and what a "path" addresses

Every crate has exactly one **crate root**: `src/main.rs` for a binary, `src/lib.rs` for a library. That root *is* the module named `crate`. Everything declared inside it, and inside its submodules, forms a tree:

```text
crate
└── store
    ├── TAX_RATE          (a constant)
    ├── inventory
    │   ├── Item          (a struct)
    │   └── find          (a function)
    └── checkout
        └── describe      (a function)
```

A **path** is the sequence of names you walk to reach a node, joined by `::`. The path to `find` from the crate root is `crate::store::inventory::find`. This is the direct counterpart of `store/inventory#find` — except the tree is built from `mod` declarations and `pub` visibility, **not** from the filesystem layout. (Files *do* map onto the tree, but that mapping is a separate topic; see [Modules](/12-modules-packages/00-modules/).)

> **Tip:** Compared with TypeScript, the load-bearing difference is this: a TypeScript import path points at a *file*; a Rust path points at a *node in the module tree*. Renaming a file in TypeScript breaks the import string. In Rust, paths are stated in terms of module names, so moving code between files changes nothing as long as the module structure is preserved.

### Absolute paths: `crate::`

An **absolute path** starts at the crate root with the keyword `crate`, then walks down. It reads the same no matter which module you write it in:

```rust playground
mod store {
    pub const TAX_RATE: f64 = 0.08;
    pub mod checkout {
        // Identical text would work from anywhere in this crate.
        use crate::store::TAX_RATE;
        pub fn rate() -> f64 { TAX_RATE }
    }
}

fn main() {
    println!("{}", store::checkout::rate()); // prints 0.08
}
```

`crate` is *this crate's* root specifically. To name an **external** crate, you use its name as the first segment instead (`std::collections::HashMap`, `serde::Serialize`). You can force "this is an external crate, not a local module" by prefixing a leading `::`:

```rust
fn main() {
    // Leading `::` forces the path to start at the EXTERNAL crate root —
    // the crate named `rand`, never a local module that happens to be `rand`.
    let n: u8 = ::rand::random();
    println!("got a byte: {n}");
}
```

This compiles and runs against `rand` 0.9 (`rand::random()` is the current idiom; the old `rand::thread_rng().gen()` from 0.8 is gone). The leading `::` is rarely needed in everyday code, but it disambiguates when a local module shadows a crate name.

### Relative paths: `self::` and `super::`

A **relative path** starts from where you are:

- `self::` means "starting in the current module." `self::total` is the `total` function declared alongside the caller.
- `super::` means "starting in the parent module." Each `super::` climbs one level toward the root. This is the closest thing Rust has to `../`.

```rust playground
fn deliver_order() {}

mod back_of_house {
    fn fix_incorrect_order() {
        cook_order();
        // `super::` goes up to the crate root (the parent of `back_of_house`),
        // where `deliver_order` lives.
        super::deliver_order();
    }

    fn cook_order() {}

    pub mod prep {
        pub fn season() {
            // Two levels up: prep -> back_of_house -> crate root.
            super::super::deliver_order();
        }
    }
}

fn main() {
    back_of_house::prep::season();
    println!("seasoned and delivered");
}
```

**Real output:**

```text
seasoned and delivered
```

Note `super::super::` to climb two levels. There is no `super::super::super::...` shorthand — you repeat `super::` once per level, which is exactly why deep trees usually read more clearly with an absolute `crate::` path.

### Bare names: items in the current module

If an item lives in the *same* module (or has been brought into scope with `use`), you name it directly with no prefix, exactly like referencing a same-file function in TypeScript. `self::` is optional in front of a same-module item; you only *need* it to disambiguate when a name has been imported that would otherwise win:

```rust playground
mod metrics {
    pub fn record(event: &str) {
        println!("recorded: {event}");
    }
}

mod handler {
    // Bring the crate-level `metrics` into scope.
    use crate::metrics;

    pub fn handle() {
        self::log("request received"); // `self::` makes "local" explicit
        metrics::record("request");    // the imported `crate::metrics`
    }

    fn log(msg: &str) {
        println!("[handler] {msg}");
    }
}

fn main() {
    handler::handle();
}
```

**Real output:**

```text
[handler] request received
recorded: request
```

### Paths span files unchanged

Because paths are module-tree coordinates, splitting a crate across files does not change a single path. Here is the same idea in a four-file crate (`mod foo;` tells the compiler "the module `foo` lives in another file" — covered in [Modules](/12-modules-packages/00-modules/)):

```rust
// src/main.rs
mod config;
mod auth;

fn main() {
    let token = auth::login("alice", "hunter2");
    println!("issued token: {token}");
    println!("base url: {}", config::base_url());
}
```

```rust
// src/config.rs
pub fn base_url() -> &'static str {
    "https://api.example.com"
}

pub const SESSION_SECONDS: u64 = 3600;
```

```rust
// src/auth.rs
mod session;

pub fn login(user: &str, _password: &str) -> String {
    // `session` is a child module of `auth` (lives in src/auth/session.rs).
    session::issue(user)
}
```

```rust
// src/auth/session.rs
// ABSOLUTE path from the crate root, crossing back up the tree:
use crate::config::SESSION_SECONDS;

pub fn issue(user: &str) -> String {
    format!("{user}:valid-for-{SESSION_SECONDS}s")
}
```

**Real output:**

```text
issued token: alice:valid-for-3600s
base url: https://api.example.com
```

`crate::config::SESSION_SECONDS` is written from deep inside `src/auth/session.rs`, yet the path knows nothing about directories — it is purely the tree position of `config`.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| What a path points at | A **file** on disk | A **node in the module tree** |
| Separator | `/` in the import string | `::` in source |
| Absolute root | Anchored at project/`node_modules` config | `crate::` (this crate) or a crate name |
| Go up a level | `../` (one directory) | `super::` (one parent module) |
| Stay here | `./` (same directory) | `self::` (same module) |
| Same scope | Bare identifier (same file) | Bare identifier (same module) |
| External package | Bare specifier (`"lodash"`) → `node_modules` | Crate name first segment (`rand::`), or `::rand::` |
| Survives file moves? | No: string breaks | Yes: path is tree-based, not file-based |

### Why a module tree instead of file paths?

TypeScript resolves imports against the filesystem (plus `tsconfig` `paths` and bundler config). Rust deliberately decouples the *name* of an item from the *file* it sits in. Benefits a TypeScript developer feels immediately:

1. **No fragile relative strings.** You never write `../../../utils`; you write `crate::utils` or `super::utils`, which says *what you mean* in the tree.
2. **Visibility is part of addressing.** A path only resolves if every segment is visible from the caller (see [Visibility and the `pub` Keyword](/12-modules-packages/03-pub-visibility/)). There is no "it imported fine but the symbol is internal" surprise.
3. **Refactor-friendly.** Moving a function to a different file does not touch any path, as long as its module position is unchanged.

### `super::` is not exactly `../`

`../` walks the *directory* containing the current file. `super::` walks the *parent module*. These usually coincide, but not always — a parent module can be inline (`mod foo { ... }`) with no directory of its own, and a single file can hold several nested modules. Always think "parent in the tree," not "parent folder."

---

## Common Pitfalls

### Pitfall 1: A path segment is private

A path only works if **every** segment along it is visible. Forgetting `pub` on an intermediate module is the most common path failure for newcomers.

```rust
mod front_of_house {
    mod hosting {                 // not `pub` — invisible from outside
        fn add_to_waitlist() {}   // also not `pub`
    }
}

fn main() {
    // does not compile (error[E0603]: module `hosting` is private)
    crate::front_of_house::hosting::add_to_waitlist();
    front_of_house::hosting::add_to_waitlist();
}
```

**Real compiler error** (`cargo build`):

```text
error[E0603]: module `hosting` is private
 --> src/main.rs:9:28
  |
9 |     crate::front_of_house::hosting::add_to_waitlist();
  |                            ^^^^^^^  --------------- function `add_to_waitlist` is not publicly re-exported
  |                            |
  |                            private module
  |
note: the module `hosting` is defined here
 --> src/main.rs:2:5
  |
2 |     mod hosting {
  |     ^^^^^^^^^^^
```

The fix is to mark each segment you need to traverse with `pub` (here, `pub mod hosting` and `pub fn add_to_waitlist`). Note the error fires on the path itself, not the declaration: Rust checks visibility at the *use site*. Visibility rules in full are in [Visibility and the `pub` Keyword](/12-modules-packages/03-pub-visibility/).

### Pitfall 2: Writing a sibling name as if it were a child

Because TypeScript imports are filesystem-relative, it is tempting to assume a bare name refers to "anything in the crate." It does not: a bare leading segment is resolved **relative to the current module**, so a top-level sibling module is *not* in scope by its bare name from inside another module.

```rust
mod network {
    pub fn connect() {}
}

mod client {
    pub fn run() {
        // does not compile (error[E0433]): tries to find `network`
        // as a child of `client`, but it is a sibling at the crate root.
        network::connect();
    }
}

fn main() {
    client::run();
}
```

**Real compiler error:**

```text
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `network`
 --> src/main.rs:9:9
  |
9 |         network::connect();
  |         ^^^^^^^ use of unresolved module or unlinked crate `network`
  |
  = help: if you wanted to use a crate named `network`, use `cargo add network` to add it to your `Cargo.toml`
help: consider importing this module
  |
6 +     use crate::network;
  |
```

Fix with an absolute path (`crate::network::connect()`), a relative one (`super::network::connect()` since `network` is a sibling), or the `use crate::network;` the compiler suggests.

### Pitfall 3: Confusing `super::` levels with directory levels

```rust
mod a {
    pub mod b {
        pub mod c {
            pub fn deep() {
                // c -> b -> a -> crate root is THREE supers,
                // regardless of how the files are arranged on disk.
                super::super::super::top();
            }
        }
    }
}

fn top() {}
```

Count parents in the **tree**, not folders. When the count climbs past two, prefer an absolute `crate::` path; it is both shorter and immune to later re-nesting.

### Pitfall 4: Expecting `self` to mean "this crate"

In TypeScript there is no single keyword for "the package root"; you reach for `./` or a configured alias. In Rust, `self::` means **this module**, and `crate::` means **this crate's root**; they are not interchangeable. Using `self::` where you meant `crate::` will look for the item one level too shallow and fail to resolve.

---

## Best Practices

### Prefer absolute `crate::` paths for distant items

The Rust community leans toward absolute paths for anything not in the immediate neighborhood, because the path stays correct when the *caller* later moves. Use relative `super::`/`self::` for tightly coupled items that you expect to move together.

```rust
// Good: distant item, absolute path is stable.
use crate::store::TAX_RATE;

// Good: closely-related sibling, relative path documents the coupling.
use super::inventory;
```

### Resolve a path once with `use`, then use the short name

A long path repeated inline is noise. Bring it into scope once with `use` (see [The `use` Keyword](/12-modules-packages/02-use-keyword/)) and call it by the short name afterward. The path-resolution rules in this file are exactly what `use` is built on.

```rust
use crate::store::inventory::Item; // resolve the path once
// ... then write `Item` everywhere instead of the full path.
```

### Reach for `self::` only when you must disambiguate

A bare same-module name is the idiomatic default. Spell out `self::name` only when a `use` import would otherwise shadow it (as in the `metrics`/`handler` example above). Gratuitous `self::` prefixes add clutter without changing meaning.

### Let the compiler write the path for you

When you get `E0433` or `E0603`, read the `help:` lines; Rust frequently prints the exact `use crate::...;` line to paste in. Running `cargo check` (see [Cargo Commands](/12-modules-packages/05-cargo-commands/)) on every save turns path mistakes into instant, well-explained feedback.

---

## Real-World Example

A small order-processing pipeline laid out as a module tree, exercising absolute and relative paths together the way a real service would. `money` is a shared utility reached absolutely from anywhere; `catalog` and `orders` are siblings that talk to each other relatively; `receipt` is a child module of `orders`.

```rust playground
//! A tiny order-processing pipeline organized as a module tree.

mod money {
    pub const CURRENCY: &str = "USD";

    pub fn format(cents: u64) -> String {
        format!("{}.{:02} {}", cents / 100, cents % 100, CURRENCY)
    }
}

mod catalog {
    pub struct Product {
        pub sku: &'static str,
        pub price_cents: u64,
    }

    pub fn lookup(sku: &str) -> Option<Product> {
        match sku {
            "BOOK-01" => Some(Product { sku: "BOOK-01", price_cents: 1999 }),
            "MUG-07" => Some(Product { sku: "MUG-07", price_cents: 1250 }),
            _ => None,
        }
    }
}

mod orders {
    // RELATIVE: `super::catalog` is a sibling of `orders`.
    use super::catalog::{self, Product};

    pub mod receipt {
        // ABSOLUTE: jump straight to `crate::money` regardless of nesting.
        use crate::money;

        pub fn line(sku: &str, price_cents: u64) -> String {
            format!("  {sku:<8} {}", money::format(price_cents))
        }
    }

    pub fn checkout(skus: &[&str]) -> String {
        let mut lines = String::from("RECEIPT\n");
        let mut total = 0;

        for sku in skus {
            if let Some(Product { sku, price_cents }) = catalog::lookup(sku) {
                // `self::receipt` is a child of THIS module.
                lines.push_str(&self::receipt::line(sku, price_cents));
                lines.push('\n');
                total += price_cents;
            }
        }

        // Absolute path again, used inline.
        lines.push_str(&format!("  TOTAL    {}", crate::money::format(total)));
        lines
    }
}

fn main() {
    let receipt = orders::checkout(&["BOOK-01", "MUG-07", "MISSING"]);
    println!("{receipt}");
}
```

**Real output:**

```text
RECEIPT
  BOOK-01  19.99 USD
  MUG-07   12.50 USD
  TOTAL    32.49 USD
```

Notice how each path choice tells the reader something: `crate::money` signals "shared utility, lives far away and stays put," while `super::catalog` and `self::receipt` document the local relationships inside `orders`.

---

## Further Reading

### Official documentation

- [The Rust Book — Paths for Referring to an Item in the Module Tree](https://doc.rust-lang.org/book/ch07-03-paths-for-referring-to-an-item-in-the-module-tree.html)
- [The Rust Reference — Paths](https://doc.rust-lang.org/reference/paths.html)
- [Rust by Example — Modules: `super` and `self`](https://doc.rust-lang.org/rust-by-example/mod/super.html)

### Related sections in this guide

- [Modules: ES modules → `mod`](/12-modules-packages/00-modules/): how the tree is *built* (this file is about navigating it)
- [The `use` keyword](/12-modules-packages/02-use-keyword/): resolve a path once and use the short name
- [Visibility with `pub`](/12-modules-packages/03-pub-visibility/) — why a path segment may be unreachable
- [Cargo and `Cargo.toml`](/12-modules-packages/04-cargo/): where the crate (the root of `crate::`) is defined
- [Cargo commands](/12-modules-packages/05-cargo-commands/): `cargo check` for instant path feedback
- [Variables and Mutability](/02-basics/00-variables/): `const` items (like `TAX_RATE`) live on the tree too
- [Functions](/03-functions/) — the items you most often address by path
- [Testing](/13-testing/): `#[cfg(test)] mod tests` is itself a child module reached with `super::*`

---

## Exercises

### Exercise 1: Make the paths resolve

**Difficulty:** Easy

**Objective:** Practice marking every segment of a path visible so both an absolute and a relative call compile.

**Instructions:** This program does not compile because the module and function are private. Add the minimum `pub` keywords so both calls in `main` work. Do not change the call sites.

```rust
mod garden {
    mod vegetables {
        fn plant() -> &'static str {
            "planted a carrot"
        }
    }
}

fn main() {
    // Absolute path
    println!("{}", crate::garden::vegetables::plant());
    // Relative path
    println!("{}", garden::vegetables::plant());
}
```

<details>
<summary>Solution</summary>

Each traversed segment needs `pub`: the inner module and the function (the outer `garden` is reachable from the crate root because `main` is also at the root, so it does not strictly need `pub` for these two calls, but `vegetables` and `plant` do).

```rust playground
mod garden {
    pub mod vegetables {
        pub fn plant() -> &'static str {
            "planted a carrot"
        }
    }
}

fn main() {
    // Absolute path
    println!("{}", crate::garden::vegetables::plant());
    // Relative path
    println!("{}", garden::vegetables::plant());
}
```

**Output:**

```text
planted a carrot
planted a carrot
```

</details>

### Exercise 2: Reach a parent's sibling with `super::`

**Difficulty:** Medium

**Objective:** Use a relative path to call a function that lives in the parent module.

**Instructions:** Implement `ui::header` so it returns `"MyApp v1.0.0"` by calling `version()`, which lives in the *parent* module `app`. Use a relative path (`super::`), not an absolute one.

```rust
mod app {
    pub fn version() -> &'static str {
        "1.0.0"
    }

    pub mod ui {
        pub fn header() -> String {
            // TODO: build "MyApp v1.0.0" by calling the parent's `version()`
            todo!()
        }
    }
}

fn main() {
    println!("{}", app::ui::header());
}
```

<details>
<summary>Solution</summary>

```rust playground
mod app {
    pub fn version() -> &'static str {
        "1.0.0"
    }

    pub mod ui {
        pub fn header() -> String {
            // Reach the sibling-of-parent `version` with `super::`.
            format!("MyApp v{}", super::version())
        }
    }
}

fn main() {
    println!("{}", app::ui::header());
}
```

**Output:**

```text
MyApp v1.0.0
```

</details>

### Exercise 3: Call across a deep tree two ways

**Difficulty:** Hard

**Objective:** Reach a top-level utility from deep inside the tree, first with an absolute path and then with the equivalent relative path, and decide which reads better.

**Instructions:** Inside `service::worker::task::run`, log two lines using `crate::logging::write`. Make the **first** call with an absolute `crate::` path and the **second** call with the fully relative `super::super::super::` path so they target the same function. The program should print both log lines.

```rust playground
mod logging {
    pub fn write(line: &str) {
        println!("LOG: {line}");
    }
}

mod service {
    pub mod worker {
        pub mod task {
            pub fn run() {
                // TODO: log "task started" via an absolute path
                // TODO: log "task finished" via the relative super::super::super:: path
            }
        }
    }
}

fn main() {
    service::worker::task::run();
}
```

<details>
<summary>Solution</summary>

```rust playground
mod logging {
    pub fn write(line: &str) {
        println!("LOG: {line}");
    }
}

mod service {
    pub mod worker {
        pub mod task {
            // Absolute path brought into scope once — clearest from deep inside.
            use crate::logging;

            pub fn run() {
                logging::write("task started");
                // The relative equivalent: task -> worker -> service -> crate root.
                super::super::super::logging::write("task finished");
            }
        }
    }
}

fn main() {
    service::worker::task::run();
}
```

**Output:**

```text
LOG: task started
LOG: task finished
```

Both reach the same `crate::logging::write`. At this depth the absolute `crate::` form (or a `use`) is far more readable than three stacked `super::`, and it survives later re-nesting of `task` — which is why absolute paths are the idiomatic default for distant items.

</details>
