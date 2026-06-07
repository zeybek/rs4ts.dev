---
title: "The `use` Keyword: Bringing Items Into Scope"
description: "use brings a path into scope as a short alias, like a JavaScript import, but loads nothing and runs no code. Covers pub use re-exports, as renaming, and globs."
---

In TypeScript/JavaScript you reach for another file's exports with `import`. In Rust the equivalent is `use`, but with an important twist: `use` does **not** load or run anything, it only creates a **shorter local name** for a path that is already part of your crate's module tree. This page covers bringing items into scope, re-exporting with `pub use`, renaming with `as`, and glob imports.

---

## Quick Overview

The `use` keyword brings a path into the current scope so you can refer to it by a short name instead of writing the full path every time. Unlike a JavaScript `import`, `use` performs **no file loading and no side effects**. Modules are wired together separately (with `mod`, covered in [Modules](/12-modules-packages/00-modules/)), and `use` is purely a convenience alias. Mastering `use`, `pub use`, and `as` is what makes a Rust crate pleasant to consume.

**In short:** `import` ≈ `use`, but `use` is just an *alias for a path*: it never executes code and never decides what gets compiled.

---

## TypeScript/JavaScript Example

```typescript
// services/users.ts
export interface User {
  id: number;
  name: string;
}

export function findUser(id: number): User {
  return { id, name: "Ada" };
}

export function createUser(name: string): User {
  return { id: Date.now(), name };
}
```

```typescript
// app.ts — pulling those exports into scope
import { User, findUser } from "./services/users";

// Rename on import to avoid a clash with a local `createUser`
import { createUser as createDbUser } from "./services/users";

// Namespace (glob) import: everything under one object
import * as users from "./services/users";

// Re-export so consumers of THIS module can import from here
export { User } from "./services/users";

const a: User = findUser(1);
const b = createDbUser("Grace");
const c = users.findUser(2);

console.log(a); // { id: 1, name: 'Ada' }
console.log(b); // { id: <timestamp>, name: 'Grace' }
console.log(c); // { id: 2, name: 'Ada' }
```

**Key points:**

- `import { x }` brings a named export into scope.
- `import { x as y }` renames on the way in.
- `import * as ns` is a namespace (glob) import.
- `export { x } from "..."` re-exports: it forwards another module's export through yours.
- Each `import` of a module **runs that module's top-level code** the first time it is evaluated.

---

## Rust Equivalent

```rust
// src/services/users.rs (a module — see modules.md for how `mod` wires this in)
#[derive(Debug)]
pub struct User {
    pub id: u32,
    pub name: String,
}

pub fn find_user(id: u32) -> User {
    User { id, name: "Ada".to_string() }
}

pub fn create_user(name: &str) -> User {
    User { id: 999, name: name.to_string() }
}
```

```rust
// src/main.rs
mod services; // declares the module tree; `use` below only makes names shorter

// Bring named items into scope (like `import { User, find_user }`)
use services::users::{User, find_user};

// Rename with `as` (like `import { create_user as create_db_user }`)
use services::users::create_user as create_db_user;

fn main() {
    let a: User = find_user(1);
    let b = create_db_user("Grace");
    println!("{a:?}"); // User { id: 1, name: "Ada" }
    println!("{b:?}"); // User { id: 999, name: "Grace" }
}
```

**Key points:**

- `use path::{A, B}` brings several names into scope from one path (like a named import list).
- `use path::Item as Alias` renames, exactly like `import { Item as Alias }`.
- `use` is just an alias: writing the full path `services::users::find_user(1)` would work without any `use` at all.
- There is **no namespace-object import** like `import * as users`. The closest thing, `use services::users;`, brings the *module name* into scope so you can write `users::find_user(...)`, but `users` is a path segment, not a runtime object you can pass around.

> **Note:** `use` and `mod` are different jobs. `mod services;` is what actually attaches `services/users.rs` to your crate's module tree (and the file is compiled because it is *reachable*, not because something imported it). `use` only shortens the name. See [Modules](/12-modules-packages/00-modules/) and [Paths in the Module Tree](/12-modules-packages/01-module-tree/).

---

## Detailed Explanation

### `use` is an alias, not a loader

This is the single most important mental-model shift. In JavaScript, `import "./logger"` can have **side effects**: the first time the module is evaluated, its top-level code runs (it might register a global, open a connection, etc.). Tree-shaking aside, the import drives what gets included.

In Rust, **what gets compiled is decided by `mod`** (reachability in the module tree), not by `use`. A `use` line is closer to a TypeScript `type` alias or a local `const Foo = SomeNamespace.Foo`; it just gives you a shorter handle. Removing a `use` never changes *whether* code is compiled; it only forces you to spell out the full path.

```rust
// These two are exactly equivalent in behavior:
use std::collections::HashMap;
let m: HashMap<String, u32> = HashMap::new();

// ...vs writing the full path inline, with no `use` at all:
let m: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
```

### Bringing items into scope

A real example using two standard-library types:

```rust
use std::collections::HashMap;
use std::collections::HashSet;

fn main() {
    let mut scores: HashMap<String, u32> = HashMap::new();
    scores.insert("Alice".to_string(), 10);
    scores.insert("Bob".to_string(), 7);

    let mut seen: HashSet<&str> = HashSet::new();
    seen.insert("a");
    seen.insert("a");

    println!("{:?}", scores.get("Alice"));
    println!("unique = {}", seen.len());
}
```

**Output (verified):**

```
Some(10)
unique = 1
```

### Nested paths and `self`

When several `use` lines share a prefix, collapse them with braces. This is the idiomatic equivalent of a multi-name JavaScript import list:

```rust
// Instead of two lines sharing `std::collections::`
use std::collections::{HashMap, BTreeMap};

// `self` inside the braces means "the module itself", so you get BOTH
// the `io` module name AND the `Write` trait in one line:
use std::io::{self, Write};
//          ^^^^ brings `io` into scope (so you can write `io::stdout()`)
//                ^^^^^ brings the `Write` trait into scope
```

### Renaming with `as`

`as` exists for the same reason as JavaScript's `import { x as y }`: avoiding name collisions. The classic case is two different `Result` types:

```rust
use std::fmt::Result as FmtResult;
use std::io::Result as IoResult;

fn make_fmt_result() -> FmtResult { Ok(()) }
fn make_io_result() -> IoResult<()> { Ok(()) }
```

There is also a special form, `as _`, which brings a **trait** into scope so you can call its methods, *without* binding a usable name. This is handy when you only need the trait's methods and want to avoid a name clash entirely:

```rust
use std::fmt::Write as _; // `write!` on a String works; the name `Write` stays unbound
```

### Glob imports (`*`)

The `*` glob brings **everything public** from a path into scope, like `import * as ns` but flattened directly into your namespace (no `ns.` prefix):

```rust
use std::collections::*; // HashMap, BTreeMap, HashSet, VecDeque, ...
```

Globs are discouraged in everyday code because they hide *where* a name came from, but they are idiomatic in exactly two places:

1. **Test modules:** `use super::*;` to pull the module under test into the test scope (see [Testing](/13-testing/)).
2. **Preludes:** `use some_crate::prelude::*;` to grab a crate's curated "starter set" of items.

### Re-exporting with `pub use`

A plain `use` is private to its module. A `pub use` re-exports the name, making it part of *your* module's public surface, exactly like `export { x } from "./other"` in TypeScript. This is the foundation of the **facade pattern**: organize code into deep modules internally, but expose a flat, friendly public path.

```rust
// src/models/mod.rs
pub mod user;
pub mod order;

// Re-export the most-used types one level up, so callers can write
// `models::User` instead of the longer `models::user::User`.
pub use user::User;
pub use order::Order;
```

```rust
// src/main.rs
mod models;

// Thanks to `pub use`, the short, flat path works:
use models::{User, Order};

fn main() {
    let u = User { id: 1, name: "Ada".to_string() };
    let o = Order { id: 42, total_cents: 1999 };
    println!("{u:?}");
    println!("{o:?}");
}
```

**Output (verified):**

```
User { id: 1, name: "Ada" }
Order { id: 42, total_cents: 1999 }
```

> **Tip:** `pub use` is how published crates give you ergonomic imports. When you write `use serde::{Serialize, Deserialize}`, those names are re-exported by `serde` from deeper internal modules. You never have to know where they actually live.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| Bring item into scope | `import { x } from "./m"` | `use m::x;` |
| Rename | `import { x as y }` | `use m::x as y;` |
| Import many | `import { a, b, c }` | `use m::{a, b, c};` |
| Namespace import | `import * as m` (real object) | `use m;` then `m::x` (path, not object) |
| Glob into scope | (no direct equivalent) | `use m::*;` |
| Re-export | `export { x } from "./m"` | `pub use m::x;` |
| Side effects on import | Yes: top-level code runs | None: `use` never executes code |
| Decides what's compiled | Yes (with bundler/tree-shaking) | No: `mod` + reachability does |
| Default visibility of imports | export must be explicit | a plain `use` is private; `pub use` to share |

### Why `use` is "just an alias"

Rust separates two concerns that JavaScript blends into `import`:

1. **"Does this code exist in my program?"** → answered by `mod` and reachability.
2. **"What short name do I type to reach it?"** → answered by `use`.

Because of this split, the standard library is always available *without* any dependency declaration: `use std::collections::HashMap;` works in any project. The `std` crate is implicitly linked; `use` just names a path inside it. (Contrast with Node, where even built-ins like `node:fs` must be imported to be referenced.)

### The prelude: why some things need no `use`

You can call `Vec::new()`, `String::from(...)`, use `Option`, `Result`, `Box`, and `println!` without ever importing them. That is because Rust automatically brings the **standard prelude** into every module; conceptually `use std::prelude::rust_2024::*;` is injected for you. Everything else needs an explicit `use` or a full path.

---

## Common Pitfalls

### Pitfall 1: Calling a trait method without importing the trait

This is the number-one `use`-related surprise for newcomers. In Rust, a method that comes from a **trait** is only callable when that trait is **in scope**. The data type can be right there, the method can be implemented, and it *still* won't resolve.

```rust
fn main() {
    let mut buf: Vec<u8> = Vec::new();
    buf.write_all(b"log line\n").unwrap(); // does not compile (error[E0599])
    println!("wrote {} bytes", buf.len());
}
```

**Compiler error (abridged):**

```
error[E0599]: no method named `write_all` found for struct `Vec<u8>` in the current scope
    --> src/main.rs:3:9
     |
   3 |     buf.write_all(b"log line\n").unwrap(); // does not compile (error[E0599])
     |         ^^^^^^^^^
     |
    ::: .../library/std/src/io/mod.rs:1835:8
     |
1835 |     fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
     |        --------- the method is available for `Vec<u8>` here
     |
     = help: items from traits can only be used if the trait is in scope
help: trait `Write` which provides `write_all` is implemented but not in scope; perhaps you want to import it
     |
   1 + use std::io::Write;
     |
help: there is a method `write` with a similar name
     |
   3 -     buf.write_all(b"log line\n").unwrap(); // does not compile (error[E0599])
   3 +     buf.write(b"log line\n").unwrap(); // does not compile (error[E0599])
     |
```

**Fix:** bring the trait into scope. The compiler even tells you which `use` line to add.

```rust
use std::io::Write; // needed for the `write_all` / `flush` methods

fn main() {
    let mut buf: Vec<u8> = Vec::new();
    buf.write_all(b"log line\n").unwrap();
    buf.flush().unwrap();
    println!("wrote {} bytes", buf.len());
}
```

**Output (verified):** `wrote 9 bytes`

> **Note:** This has no TypeScript analogue. In TS, methods belong to the object's type; in Rust, trait methods are gated on the trait being imported. When a method "should exist but doesn't," the missing `use` for a trait is the usual culprit.

### Pitfall 2: Two imports with the same name

Importing two items that share a final name is a hard error. Rust will not silently shadow one with the other.

```rust
use std::fmt::Result;
use std::io::Result; // does not compile (error[E0252])
```

**Compiler error (abridged — two trailing `unused import` warnings omitted):**

```
error[E0252]: the name `Result` is defined multiple times
 --> src/main.rs:2:5
  |
1 | use std::fmt::Result;
  |     ---------------- previous import of the type `Result` here
2 | use std::io::Result; // does not compile (error[E0252])
  |     ^^^^^^^^^^^^^^^ `Result` reimported here
  |
  = note: `Result` must be defined only once in the type namespace of this module
help: you can use `as` to change the binding name of the import
  |
2 | use std::io::Result as OtherResult; // does not compile (error[E0252])
  |                     ++++++++++++++
```

**Fix:** rename one (or both) with `as`, exactly as the compiler suggests:

```rust
use std::fmt::Result as FmtResult;
use std::io::Result as IoResult;
```

### Pitfall 3: Expecting `use` to "load" or run a file

Coming from JavaScript, it is tempting to think `use crate::logger;` will execute `logger`'s setup code or "include" the file. It does neither. If your module isn't part of the tree (declared with `mod`), no `use` will conjure it into existence: you'll get an "unresolved import" error. Wiring files in is the `mod` keyword's job; see [Modules](/12-modules-packages/00-modules/).

### Pitfall 4: Reaching for glob imports everywhere

`use foo::*;` feels familiar (it looks like a barrel import), but in normal code it makes review harder. A reader can no longer tell where `Widget` came from, and adding an item to `foo` can suddenly collide with a local name. Reserve globs for preludes and test modules (Best Practices below).

---

## Best Practices

### 1. Group imports and let the formatter organize them

`rustfmt` (run via `cargo fmt`, see [Cargo Commands](/12-modules-packages/05-cargo-commands/)) sorts and merges `use` lines for you. The conventional grouping is: standard library, then external crates, then your own crate.

```rust
// Standard library
use std::collections::HashMap;
use std::fmt;

// External crates
use serde::{Deserialize, Serialize};

// This crate
use crate::models::User;
```

### 2. Prefer explicit names over globs

```rust
// Clear where each name comes from
use std::collections::{HashMap, HashSet};

// Hides origins; reserve for preludes/tests
use std::collections::*;
```

### 3. Import the parent module for functions, the item itself for types

Idiomatic Rust often imports the **module** for free functions (so call sites read `io::stdout()`, signaling origin) but imports **types/traits directly** (so you write `HashMap`, not `collections::HashMap`).

```rust
use std::io::{self, Write}; // call `io::stdout()`, but `Write` used bare
```

### 4. Build a facade with `pub use`

Keep your internal module layout free to evolve while presenting a stable, flat public API:

```rust
// src/lib.rs
mod parser;   // internal modules can be reorganized freely
mod renderer;

pub use parser::parse;       // public callers see `your_crate::parse`
pub use renderer::Renderer;  // and `your_crate::Renderer`
```

This is exactly how mature crates expose their API; visibility rules behind it are covered in [Visibility and the `pub` Keyword](/12-modules-packages/03-pub-visibility/).

### 5. Use `as _` to import a trait you only call methods on

When you need a trait's methods but never name the trait, `use Trait as _;` keeps the trait out of the namespace and silences "unused import" pedantry while still enabling the methods.

---

## Real-World Example

A production-flavored pattern: a crate organizes its domain types across modules but offers a **prelude** that callers pull in with one glob. This mirrors how `tokio`, `rayon`, and many others ship a `prelude`.

```rust
mod domain {
    #[derive(Debug, Clone)]
    pub struct Money { pub cents: i64 }

    impl Money {
        pub fn dollars(d: i64) -> Self { Money { cents: d * 100 } }
    }

    #[derive(Debug)]
    pub struct Invoice {
        pub id: u32,
        pub amount: Money,
    }

    pub trait Summarize {
        fn summary(&self) -> String;
    }

    impl Summarize for Invoice {
        fn summary(&self) -> String {
            format!(
                "Invoice #{} for ${}.{:02}",
                self.id,
                self.amount.cents / 100,
                self.amount.cents % 100
            )
        }
    }
}

// A `prelude` gathers the items most callers want behind one path.
mod prelude {
    pub use crate::domain::{Invoice, Money, Summarize};
}

// One glob import pulls in the whole curated prelude.
use prelude::*;

fn main() {
    let invoice = Invoice { id: 7, amount: Money::dollars(42) };
    // `summary()` only resolves because `Summarize` is in scope via the prelude.
    println!("{}", invoice.summary());
}
```

**Output (verified):**

```
Invoice #7 for $42.00
```

**Why this is good design:**

- Internal layout (`domain`) can be split, renamed, or reorganized without breaking callers; only `prelude` is the public contract.
- The `Summarize` trait being re-exported is essential: without it in scope, `invoice.summary()` would hit the E0599 error from Pitfall 1.
- Callers opt into one glob (`use crate::prelude::*;`) instead of memorizing a dozen paths: the legitimate, intentional use of `*`.

A realistic external-crate example, using `serde` (see [Specifying Dependencies](/12-modules-packages/06-dependencies/) for adding it):

```rust
// Bring traits + derive macros into scope from an external crate.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct ServiceConfig {
    name: String,
    port: u16,
    #[serde(default)]
    env: HashMap<String, String>,
}

fn main() {
    let raw = r#"{ "name": "api", "port": 8080, "env": { "LOG": "info" } }"#;
    let cfg: ServiceConfig = serde_json::from_str(raw).expect("valid config");
    println!("{} listening on :{}", cfg.name, cfg.port);
    println!("env = {:?}", cfg.env);

    let back = serde_json::to_string(&cfg).unwrap();
    println!("{back}");
}
```

**Output (verified):**

```
api listening on :8080
env = {"LOG": "info"}
{"name":"api","port":8080,"env":{"LOG":"info"}}
```

Notice that `serde_json::from_str` is called by its full path with no `use`. `use` is optional, and for a one-off call the qualified path can be clearer.

---

## Further Reading

### Official Documentation

- [The Rust Book — Bringing Paths into Scope with `use`](https://doc.rust-lang.org/book/ch07-04-bringing-paths-into-scope-with-the-use-keyword.html)
- [The Rust Book — Re-exporting Names with `pub use`](https://doc.rust-lang.org/book/ch07-04-bringing-paths-into-scope-with-the-use-keyword.html#re-exporting-names-with-pub-use)
- [Rust Reference — Use declarations](https://doc.rust-lang.org/reference/items/use-declarations.html)
- [Rust by Example — `use` declaration](https://doc.rust-lang.org/rust-by-example/mod/use.html)
- [`std::prelude` documentation](https://doc.rust-lang.org/std/prelude/index.html)

### Related Sections in This Guide

- [Modules: ES modules → `mod`](/12-modules-packages/00-modules/) — how files get attached to the module tree (the `mod` half of the story).
- [Module Tree & Paths](/12-modules-packages/01-module-tree/) — `crate::`, `super::`, `self::`, absolute vs relative paths used by `use`.
- [Visibility with `pub`](/12-modules-packages/03-pub-visibility/) — why a plain `use` is private and how `pub use` exposes it; `pub(crate)`/`pub(super)`.
- [Dependencies](/12-modules-packages/06-dependencies/) — adding external crates so you have something to `use`.
- [Cargo commands](/12-modules-packages/05-cargo-commands/) — `cargo fmt` (organizes imports), `cargo clippy`.
- [Section 13: Testing](/13-testing/) — where `use super::*;` is the idiomatic glob.
- [Getting Started: Cargo Basics](/01-getting-started/03-cargo-basics/) and [Basics: Output](/02-basics/04-output/) — foundations referenced above.

---

## Exercises

### Exercise 1: Import the right things

**Difficulty:** Easy

**Objective:** Practice bringing a type and a trait into scope.

**Instructions:** This program counts letter frequencies and builds a small report string using the `write!` macro. It is missing its imports; add the two `use` lines so it compiles. (Hint: `write!` writing into a `String` needs a trait in scope.)

```rust
// TODO: add the necessary `use` lines

fn main() {
    let mut counts: HashMap<char, u32> = HashMap::new();
    for c in "hello".chars() {
        *counts.entry(c).or_insert(0) += 1;
    }

    let mut report = String::new();
    write!(report, "letters: {}", counts.len()).unwrap();
    println!("{report}");
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;
use std::fmt::Write as _; // brings the trait in for `write!`; `as _` keeps the name unbound

fn main() {
    let mut counts: HashMap<char, u32> = HashMap::new();
    for c in "hello".chars() {
        *counts.entry(c).or_insert(0) += 1;
    }

    let mut report = String::new();
    write!(report, "letters: {}", counts.len()).unwrap();
    println!("{report}");
}
```

**Output:** `letters: 4`

`HashMap` lives in `std::collections`. The `write!` macro requires `std::fmt::Write` to be in scope to write into a `String`; using `as _` brings the trait's methods in without binding the name `Write`.

</details>

### Exercise 2: Resolve a name clash with `as`

**Difficulty:** Medium

**Objective:** Use `as` to import two same-named functions.

**Instructions:** The crate has `parser::json::parse` and `parser::csv::parse`. Both are named `parse`, so you cannot import both directly. Add two `use` lines that rename them to `parse_json` and `parse_csv`, then call each in `main`.

```rust
mod parser {
    pub mod json {
        pub fn parse(input: &str) -> usize { input.len() }
    }
    pub mod csv {
        pub fn parse(input: &str) -> usize { input.split(',').count() }
    }
}

// TODO: import both `parse` functions under distinct names

fn main() {
    // TODO: call parse_json("{\"a\":1}") and parse_csv("a,b,c"), printing each result
}
```

<details>
<summary>Solution</summary>

```rust
mod parser {
    pub mod json {
        pub fn parse(input: &str) -> usize { input.len() }
    }
    pub mod csv {
        pub fn parse(input: &str) -> usize { input.split(',').count() }
    }
}

use parser::json::parse as parse_json;
use parser::csv::parse as parse_csv;

fn main() {
    println!("json bytes: {}", parse_json("{\"a\":1}"));
    println!("csv fields: {}", parse_csv("a,b,c"));
}
```

**Output:**

```
json bytes: 7
csv fields: 3
```

Without `as`, the second `use parser::...::parse;` would trigger `error[E0252]: the name 'parse' is defined multiple times`.

</details>

### Exercise 3: Build a facade with `pub use`

**Difficulty:** Medium

**Objective:** Flatten a deep path using `pub use` so the crate root can import items directly.

**Instructions:** The `auth` module nests `issue` inside `auth::tokens`. Add a `pub use` inside `auth` so that `issue` is reachable as `auth::issue`. Then at the crate root, import `Session` and `issue` together in one `use` line and use them in `main`.

```rust
mod auth {
    #[derive(Debug)]
    pub struct Session { pub user: String }

    pub mod tokens {
        pub fn issue(user: &str) -> String { format!("tok-{user}") }
    }

    // TODO: re-export `tokens::issue` so it is reachable as `auth::issue`
}

// TODO: import Session and issue from `auth` in one line

fn main() {
    // TODO: create a Session for "ada", issue a token for its user, and print both
}
```

<details>
<summary>Solution</summary>

```rust
mod auth {
    #[derive(Debug)]
    pub struct Session { pub user: String }

    pub mod tokens {
        pub fn issue(user: &str) -> String { format!("tok-{user}") }
    }

    // Facade: re-export the deeper item so the crate root sees `auth::issue`.
    pub use tokens::issue;
}

use auth::{Session, issue};

fn main() {
    let s = Session { user: "ada".into() };
    let token = issue(&s.user);
    println!("{s:?} -> {token}");
}
```

**Output:**

```
Session { user: "ada" } -> tok-ada
```

The `pub use tokens::issue;` line is the facade: callers no longer need to know `issue` lives in the `tokens` submodule. This is the same mechanism crates like `serde` use to expose deeply-nested items at a friendly top-level path.

</details>

---

## Summary

**What you've learned:**

- `use` brings a path into scope as a shorter alias — it does **not** load files or run code (that's `mod`'s job).
- `use path::{A, B}` groups names; `use path::{self, X}` brings in a module *and* an item.
- `as` renames imports to dodge collisions; `as _` imports a trait for its methods without binding a name.
- Glob `use path::*` flattens everything in; reserve it for preludes and test modules.
- `pub use` re-exports, enabling the facade pattern that gives crates clean public APIs.
- Trait methods require the trait to be in scope (the E0599 trap with no TypeScript equivalent).

**Mental model:** `import` ≈ `use`, but in Rust the questions "is this compiled?" and "what do I call it?" are answered by *different* keywords — `mod` and `use`, respectively.
