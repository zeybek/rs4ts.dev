---
title: "Visibility and the `pub` Keyword"
description: "Rust makes every item private by default, like an un-exported declaration, then adds pub, pub(crate), pub(super), and per-field control JavaScript modules lack."
---

In JavaScript and TypeScript modules, **nothing leaves a file unless you `export` it**, and once exported, it is fully public to anyone who imports it. Rust takes the same "private by default" stance but gives you a far richer dial: `pub`, `pub(crate)`, `pub(super)`, and `pub(in path)` let you say *exactly how far* an item is allowed to travel. This page is about that dial.

---

## Quick Overview

Every item in Rust — functions, structs, enums, modules, constants, and even individual struct fields — is **private by default**, visible only within the module that defines it (and that module's children). You opt items into wider visibility with the **`pub`** keyword and its restricted variants. For a TypeScript/JavaScript developer the mental model is: Rust's `pub` is like `export`, but Rust adds a graduated scale between "totally private" and "totally public" that JavaScript modules simply do not have.

> **Note:** This page focuses on *visibility*: who is allowed to see an item. How items are organized into modules is covered in [Modules](/12-modules-packages/00-modules/), how you name paths to reach them in [Paths in the Module Tree](/12-modules-packages/01-module-tree/), and how you bring them into scope with `use` (including re-exporting with `pub use`) in [The `use` Keyword](/12-modules-packages/02-use-keyword/).

---

## TypeScript/JavaScript Example

In an ES module, a declaration is private to its file until you `export` it. There is no in-between: an exported symbol is reachable by *any* file that imports it, whether that file lives in the same folder, the same package, or a completely different `npm` package that depends on yours.

```typescript
// billing.ts

// Private to this file — no `export`, so nobody outside can see it.
function internalTaxRate(): number {
  return 0.08;
}

// Exported — now PUBLIC to every importer, in this package or another.
export function totalWithTax(amount: number): number {
  return amount + amount * internalTaxRate();
}

// A class whose fields are public unless marked `private`/`#`.
export class User {
  id: number;
  name: string;
  #email: string; // truly private (ES `#` field)

  constructor(id: number, name: string, email: string) {
    this.id = id;
    this.name = name;
    this.#email = email;
  }

  get email(): string {
    return this.#email;
  }
}
```

```typescript
// app.ts
import { totalWithTax, User } from "./billing";

console.log(totalWithTax(100)); // 108
// internalTaxRate();           // not exported, not importable

const user = new User(1, "Ada", "ada@example.com");
user.name = "Ada Lovelace"; // public field, freely writable
// user.#email;             // SyntaxError: private field outside class
```

TypeScript's `private`/`protected` keywords add compile-time field access control, but they are erased at runtime; nothing stops a `(user as any).email` cast. The ES `#` prefix is the only *runtime*-enforced privacy. The key point for the comparison: at the **module** level, JavaScript offers a binary switch — exported or not.

---

## Rust Equivalent

```rust playground
mod billing {
    // Private by default — visible only inside `billing`.
    fn internal_tax_rate() -> f64 {
        0.08
    }

    // `pub` exposes this to the parent module (the crate root, here).
    pub fn total_with_tax(amount: f64) -> f64 {
        amount + amount * internal_tax_rate()
    }
}

mod model {
    #[derive(Debug)]
    pub struct User {
        pub id: u64,      // public field
        pub name: String, // public field
        email: String,    // private field — protected by the type's methods
    }

    impl User {
        pub fn new(id: u64, name: &str, email: &str) -> Self {
            User {
                id,
                name: name.to_string(),
                email: email.to_string(),
            }
        }

        pub fn email(&self) -> &str {
            &self.email
        }
    }
}

fn main() {
    let total = billing::total_with_tax(100.0);
    println!("Total: {total}");

    let mut user = model::User::new(1, "Ada", "ada@example.com");
    user.name = "Ada Lovelace".to_string(); // public field, writable
    println!("id={}, name={}", user.id, user.name);
    println!("email={}", user.email()); // private field, via getter
}
```

**Real output:**

```
Total: 108
id=1, name=Ada Lovelace
email=ada@example.com
```

Notice the new dimension compared to JavaScript: `pub` is applied **per field**, not just per type. A `pub struct` can still hide some of its data, which is the foundation of encapsulation in Rust.

---

## Detailed Explanation

### Private by default, like an un-`export`ed declaration

When you write a plain `fn`, `struct`, `enum`, `const`, `static`, `trait`, or `mod` with no visibility keyword, it is **private**. "Private" in Rust means: visible inside the defining module and any of that module's **descendant** modules, but invisible to the **parent** module and to siblings.

```rust
mod billing {
    fn internal_tax_rate() -> f64 { 0.08 } // private to `billing`
}

fn main() {
    let r = billing::internal_tax_rate(); // does not compile (error[E0603]: private function)
    println!("{r}");
}
```

The real compiler error:

```
error[E0603]: function `internal_tax_rate` is private
 --> src/main.rs:8:22
  |
8 |     let r = billing::internal_tax_rate();
  |                      ^^^^^^^^^^^^^^^^^ private function
  |
note: the function `internal_tax_rate` is defined here
 --> src/main.rs:2:5
  |
2 |     fn internal_tax_rate() -> f64 {
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

> **Note:** A subtle but important asymmetry: child modules can always see *into* their parents' private items, but parents cannot see *into* their children's private items. Privacy in Rust flows *outward*: you publish to ancestors, you never hide from descendants. This is the opposite of how many object systems think about "private."

### `pub` — export to the parent (and beyond)

Adding `pub` makes an item visible to the module that *contains* it. That is the literal meaning of `pub`, and it is why a `pub` item in a module that is itself private is still not reachable from far away: visibility composes along the whole path. The general rule:

> An item is reachable from some location only if **every** segment of the path to it — each enclosing module *and* the item itself — is visible from that location.

So to expose `billing::total_with_tax` to code *outside* your crate, both `total_with_tax` **and** the `billing` module would need to be `pub` (`pub mod billing`).

### Field visibility: the part JavaScript modules don't have

In TypeScript, marking a class public exports the class; field-level privacy is a separate, class-scoped feature (`private`, `#`). In Rust, **fields have their own visibility, independent of the struct's**:

```rust
pub struct User {
    pub id: u64,   // anyone who can see `User` can read/write this
    email: String, // only code in this module can touch it
}
```

A field with no `pub` is private even when the struct is `pub`. Reading it from outside is an `error[E0616]`:

```
error[E0616]: field `email` of struct `User` is private
  --> src/main.rs:16:25
   |
16 |     println!("{}", user.email);
   |                         ^^^^^ private field
```

And — a detail that surprises newcomers — if a struct has *any* private field, you **cannot build it with a struct literal from outside the module at all**, even if you only set the public fields:

```rust
mod model {
    pub struct User {
        pub id: u64,
        email: String, // private
    }
}

fn main() {
    let user = model::User {
        id: 1,
        email: "ada@example.com".to_string(), // does not compile (error[E0451])
    };
    println!("{}", user.id);
}
```

```
error[E0451]: field `email` of struct `User` is private
  --> src/main.rs:12:9
   |
10 |     let user = model::User {
   |                ----------- in this type
11 |         id: 1,
12 |         email: "ada@example.com".to_string(),
   |         ^^^^^ private field
```

This is intentional: by hiding even one field, the type *forces* outside callers through a constructor (`User::new`), which is how Rust guarantees invariants. It is the equivalent of a TypeScript class that hides its backing fields behind a constructor and getters, but enforced at compile time across the whole crate boundary.

### The graduated scale: `pub(crate)`, `pub(super)`, `pub(in path)`

This is where Rust goes beyond `export`. Between "private" and "fully public" sit three restricted forms:

| Form               | Reachable from…                                                          |
| ------------------ | ------------------------------------------------------------------------ |
| `pub`              | Everywhere the path is visible — including **other crates**              |
| `pub(crate)`       | Anywhere in **this crate**, but never from a crate that depends on yours |
| `pub(super)`       | The **parent** module and everything nested under that parent            |
| `pub(in some::path)` | Only within the named ancestor module (and its descendants)            |

```rust playground
mod network {
    // Visible anywhere in THIS crate, but NOT to external crates.
    pub(crate) fn connection_id() -> u64 {
        42
    }

    pub mod tcp {
        // Visible only to the parent module `network` (and its descendants).
        pub(super) fn raw_handshake() -> &'static str {
            "SYN/ACK"
        }

        // Visible only within the `network` module subtree.
        pub(in crate::network) fn buffer_size() -> usize {
            8192
        }

        pub fn open() -> &'static str {
            let _ = buffer_size(); // same module: always fine
            "tcp-open"
        }
    }

    pub fn diagnostics() -> String {
        let hs = tcp::raw_handshake();    // `network` can see `pub(super)` of its child
        let bs = tcp::buffer_size();      // and `pub(in crate::network)` items too
        format!("handshake={hs}, buffer={bs}")
    }
}

fn main() {
    println!("conn id = {}", network::connection_id()); // pub(crate): visible at crate root
    println!("{}", network::tcp::open());               // pub: visible everywhere
    println!("{}", network::diagnostics());
}
```

**Real output:**

```
conn id = 42
tcp-open
handshake=SYN/ACK, buffer=8192
```

> **Tip:** `pub(crate)` is the workhorse. Most "this is public to my own code but I never want to commit to it as part of my library's API" items should be `pub(crate)`. It is the closest thing Rust has to "internal" in C# or "package-private" in Java, a level JavaScript modules cannot express at all.

The mental model for `pub(super)` deserves care. It means "visible to my parent module." Because privacy flows outward and descendants always inherit access, anything *nested under that parent* can also reach the item. So `pub(super)` is not "visible only to the immediate parent and nothing else"; it is "visible to the parent subtree." We will see exactly where this trips people up in [Common Pitfalls](#common-pitfalls).

### Enums: variants inherit the enum's visibility

Unlike struct fields, **enum variants and their fields are automatically as public as the enum itself**. You do not (and cannot) write `pub` on a variant:

```rust playground
mod payments {
    pub enum Status {
        Pending,
        Settled { amount: u64 }, // variant fields are public automatically
        Failed,
    }

    pub fn describe(s: &Status) -> String {
        match s {
            Status::Pending => "pending".to_string(),
            Status::Settled { amount } => format!("settled: {amount}"),
            Status::Failed => "failed".to_string(),
        }
    }
}

fn main() {
    let s = payments::Status::Settled { amount: 500 };
    if let payments::Status::Settled { amount } = &s {
        println!("amount = {amount}");
    }
    println!("{}", payments::describe(&s));
}
```

**Real output:**

```
amount = 500
settled: 500
```

This makes sense: an enum's whole purpose is for callers to match on its variants, so hiding individual variants would defeat the point. If you need a "closed" enum that callers can't exhaustively match or construct, you reach for other patterns (a private struct wrapper, or the `#[non_exhaustive]` attribute), not field visibility.

---

## Key Differences

| Concept                          | TypeScript/JavaScript                                    | Rust                                                            |
| -------------------------------- | -------------------------------------------------------- | -------------------------------------------------------------- |
| Default for a top-level item     | Private to the file until `export`ed                     | Private to the module (visible to descendants)                 |
| Granularity                      | Binary: exported or not                                  | Graduated: `pub`, `pub(crate)`, `pub(super)`, `pub(in path)`   |
| "Internal to my package" level   | None (an `export` is reachable cross-package)            | `pub(crate)`                                                   |
| Field visibility                 | `private`/`#` on class members (class-scoped)            | Per-field `pub` (independent of the struct)                    |
| Enforcement                      | `private`/`protected` erased at runtime; `#` enforced    | Always enforced at compile time, across crate boundaries       |
| Constructing a type with hidden state | Allowed via `as any` casts                          | Impossible from outside if any field is private (`E0451`)      |
| Direction of privacy             | Members hidden from outside the class                    | Items hidden from *ancestors*; descendants always have access  |

**Why the extra knobs?** Rust libraries publish a stable public API to *other crates* via [crates.io](https://crates.io) (see [Publishing to crates.io](/12-modules-packages/11-publishing/)). Everything that is merely `pub` becomes part of that semver contract; changing or removing it is a breaking change. `pub(crate)` lets you write code that is shared freely *inside* your crate without ever leaking into your public API. JavaScript packages have no language-level equivalent: the convention is fragile (underscore prefixes, `internal/` folders, an `exports` map in `package.json`), and none of it is checked by the compiler.

> **Note:** The visibility of an item also interacts with `use`-based re-exports. You can keep an item defined deep inside a private module and surface it at the crate root with `pub use`, decoupling your *file layout* from your *public API*. That technique lives in [The `use` Keyword](/12-modules-packages/02-use-keyword/#re-exporting-with-pub-use).

---

## Common Pitfalls

### Pitfall 1: Assuming a `pub struct` makes its fields public

Coming from JavaScript, where exporting a class exposes its public fields, it is easy to expect `pub struct` to do the same. It does not: fields are private unless individually marked `pub`.

```rust
mod model {
    pub struct User {
        pub id: u64,
        email: String, // forgot `pub`
    }
    impl User {
        pub fn new(id: u64, email: &str) -> Self {
            User { id, email: email.to_string() }
        }
    }
}

fn main() {
    let user = model::User::new(1, "ada@example.com");
    println!("{}", user.email); // does not compile (error[E0616]: private field)
}
```

```
error[E0616]: field `email` of struct `User` is private
  --> src/main.rs:16:25
   |
16 |     println!("{}", user.email);
   |                         ^^^^^ private field
```

**Fix:** add `pub` to the field, or expose a getter method (`pub fn email(&self) -> &str`). Prefer the getter when the field has invariants to protect.

### Pitfall 2: Leaking a private type through a public function

If a `pub` (or `pub(crate)`) function returns or accepts a type that is *less* visible than the function itself, the function's signature would expose a name callers cannot otherwise reach. Rust warns about this (lint `private_interfaces`) and the type is unusable at the call site:

```rust
mod config {
    struct Secret {           // private type
        token: String,
    }
    pub fn load() -> Secret { // public fn returning a private type
        Secret { token: "abc".to_string() }
    }
}

fn main() {
    let _s = config::load(); // does not compile (the returned type is private)
}
```

```
warning: type `Secret` is more private than the item `load`
 --> src/main.rs:8:5
  |
8 |     pub fn load() -> Secret {
  |     ^^^^^^^^^^^^^^^^^^^^^^^ function `load` is reachable at visibility `pub(crate)`
  |
note: but type `Secret` is only usable at visibility `pub(self)`
...
error: type `Secret` is private
  --> src/main.rs:14:14
   |
14 |     let _s = config::load();
   |              ^^^^^^^^^^^^^^ private type
```

**Fix:** make `Secret` at least as visible as `load` (`pub(crate) struct Secret`), or lower `load`'s visibility to match the type. The rule is: an item's public surface may not be more visible than the types it mentions.

### Pitfall 3: Misreading `pub(super)` as "only the immediate parent"

`pub(super)` means "visible to my parent module," and because descendants inherit ancestor access, that includes **everything nested under the parent** — not just the parent's own body. Reaching the item from a *cousin* under the same parent succeeds, which surprises people who expected a tighter restriction:

```rust
mod app {
    pub mod inner {
        pub(super) fn helper() -> u32 { 1 } // visible to `app` and all of `app`'s descendants
    }
    pub mod other {
        pub fn call_it() -> u32 {
            super::inner::helper() // compiles — `other` is inside `app`
        }
    }
}
```

But step **outside** the parent subtree (here, the crate root is not inside `app`) and it is private again:

```rust
mod app {
    pub mod inner {
        pub(super) fn helper() -> u32 { 1 }
    }
}

fn main() {
    println!("{}", app::inner::helper()); // does not compile (error[E0603]: private function)
}
```

```
error[E0603]: function `helper` is private
  --> src/main.rs:10:32
   |
10 |     println!("{}", app::inner::helper());
   |                                ^^^^^^ private function
```

**Fix:** if you truly want only one specific ancestor module to have access (and nothing else), name it explicitly with `pub(in crate::app)`. If you want crate-wide access, use `pub(crate)`.

### Pitfall 4: Reaching for `pub` when `pub(crate)` is what you mean

Marking helpers `pub` "so my other modules can use them" silently enlarges your crate's public API. Once published, those items are part of your semver contract and you cannot remove them without a major version bump. There is no compiler *error* here; that is exactly why it is dangerous.

**Fix:** default to `pub(crate)` for anything that is shared internally but not meant for external consumers. Reserve plain `pub` for the genuine public surface of a library. (See [Publishing to crates.io](/12-modules-packages/11-publishing/) for why this matters at release time.)

---

## Best Practices

- **Start private, widen only when forced.** Write items with no visibility keyword first; add `pub`/`pub(crate)` when a real consumer appears. The compiler error tells you precisely when.
- **Use `pub(crate)` as your default for shared internals.** It is the idiomatic "internal but not public API" level and keeps your published surface small and stable.
- **Hide struct fields and expose behavior.** Make fields private and offer constructors plus getter/setter methods when there are invariants to protect; expose fields with `pub` only for plain data holders (configuration structs, DTOs) that have no invariants.
- **Keep public APIs free of private types.** Heed the `private_interfaces` lint; if a function is `pub`, every type in its signature must be at least as visible.
- **Prefer explicit `pub(in path)` over loose `pub(super)`** when you want a single, named module to be the only privileged caller; it documents intent and survives refactors that move modules around.
- **Decouple file layout from public API with `pub use`.** Organize code into private submodules for your own sanity, then re-export the curated public items at a stable path. (Details in [The `use` Keyword](/12-modules-packages/02-use-keyword/).)
- **Run `cargo doc --no-deps --open`** to see exactly what your crate exposes — only `pub` items reachable from the crate root show up, which is the truest picture of your public API.

---

## Real-World Example

A production-flavored event store that uses field visibility to enforce an invariant: every stored event gets a store-assigned sequence number, and the internal index is kept in sync with the event list. Callers can never corrupt either because the fields are private: the only way in is through `push`.

```rust playground
mod store {
    use std::collections::HashMap;

    /// An append-only event log. The internal storage is hidden so the
    /// `events` vec and the `index` map can never drift out of sync.
    pub struct EventStore {
        events: Vec<Event>,            // private: callers must use `push`
        index: HashMap<String, usize>, // private: maintained internally
    }

    #[derive(Clone, Debug)]
    pub struct Event {
        pub id: String,      // public: callers read this freely
        pub payload: String, // public
        seq: u64,            // private: assigned by the store, never the caller
    }

    impl Event {
        /// Read-only access to the store-assigned sequence number.
        pub fn seq(&self) -> u64 {
            self.seq
        }
    }

    impl EventStore {
        pub fn new() -> Self {
            EventStore {
                events: Vec::new(),
                index: HashMap::new(),
            }
        }

        /// The single, invariant-preserving way to add an event.
        pub fn push(&mut self, id: &str, payload: &str) {
            let seq = self.events.len() as u64;
            self.index.insert(id.to_string(), self.events.len());
            self.events.push(Event {
                id: id.to_string(),
                payload: payload.to_string(),
                seq,
            });
        }

        pub fn get(&self, id: &str) -> Option<&Event> {
            let pos = *self.index.get(id)?;
            self.events.get(pos)
        }

        /// Crate-internal metric: other modules in this crate may read the
        /// count, but external crates cannot rely on it.
        pub(crate) fn len(&self) -> usize {
            self.events.len()
        }
    }
}

// Surface the public types at the crate root, independent of file layout.
pub use store::{Event, EventStore};

fn main() {
    let mut es = EventStore::new();
    es.push("user.created", "{\"id\":1}");
    es.push("user.renamed", "{\"id\":1,\"name\":\"Ada\"}");

    if let Some(ev) = es.get("user.renamed") {
        println!("id={}, seq={}, payload={}", ev.id, ev.seq(), ev.payload);
    }

    println!("total events (crate-internal): {}", es.len());
}
```

**Real output:**

```
id=user.renamed, seq=1, payload={"id":1,"name":"Ada"}
total events (crate-internal): 2
```

Note the layered design: `id` and `payload` are `pub` because they are plain data the caller supplied; `seq` is private with a read-only getter because *the store owns it*; the `EventStore` internals are fully private; and `len` is `pub(crate)` because internal tooling may want it but it is not part of the library's public promise. This is the kind of fine-grained control that JavaScript's binary `export` cannot express.

---

## Further Reading

### Official Documentation

- [The Rust Book — Controlling Visibility with `pub`](https://doc.rust-lang.org/book/ch07-03-paths-for-referring-to-an-item-in-the-module-tree.html#exposing-paths-with-the-pub-keyword)
- [The Rust Reference — Visibility and Privacy](https://doc.rust-lang.org/reference/visibility-and-privacy.html)
- [Rust by Example — Visibility](https://doc.rust-lang.org/rust-by-example/mod/visibility.html)
- [The `non_exhaustive` attribute](https://doc.rust-lang.org/reference/attributes/type_system.html#the-non_exhaustive-attribute) — for evolving enums and structs without breaking callers

### Related Sections in This Guide

- [Modules](/12-modules-packages/00-modules/): how `mod` defines the tree that visibility is measured against
- [The Module Tree and Paths](/12-modules-packages/01-module-tree/) — `crate::`, `super::`, `self::`, and absolute vs relative paths
- [The `use` Keyword](/12-modules-packages/02-use-keyword/): bringing items into scope and re-exporting with `pub use`
- [Cargo and `Cargo.toml`](/12-modules-packages/04-cargo/) — the crate that `pub(crate)` refers to
- [Publishing to crates.io](/12-modules-packages/11-publishing/) — why your `pub` surface is a semver contract
- [Variables and Mutability](/02-basics/00-variables/) — the broader "private/immutable by default" philosophy
- [Writing Tests](/13-testing/): how `#[cfg(test)]` modules access private items via `use super::*`

---

## Exercises

### Exercise 1: Expose the right function

**Difficulty:** Easy

**Objective:** Practice the difference between a private helper and a public entry point.

**Instructions:** Make `area` callable from `main`, while keeping `scale_factor` private to the module. Fill in the visibility keywords so the program compiles and prints `6`.

```rust
mod geometry {
    fn area(width: f64, height: f64) -> f64 {
        scale_factor() * width * height
    }

    fn scale_factor() -> f64 {
        1.0
    }
}

fn main() {
    println!("{}", geometry::area(2.0, 3.0)); // should print 6
}
```

<details>
<summary>Solution</summary>

Only `area` needs to be public; `scale_factor` stays private and remains reachable from inside the module.

```rust playground
mod geometry {
    pub fn area(width: f64, height: f64) -> f64 {
        scale_factor() * width * height
    }

    fn scale_factor() -> f64 {
        1.0
    }
}

fn main() {
    println!("{}", geometry::area(2.0, 3.0));
}
```

**Output:**

```
6
```

</details>

### Exercise 2: Encapsulate an invariant with field visibility

**Difficulty:** Medium

**Objective:** Use a private field to force callers through methods that protect a balance from going negative.

**Instructions:** Complete the `Account` type so that `owner` is a public field, `balance` is private, and the balance can only change through `deposit` and `withdraw`. A withdrawal larger than the balance must return an `Err`.

```rust
mod bank {
    pub struct Account {
        // owner should be a public field
        // balance should be private
    }

    impl Account {
        pub fn open(owner: &str) -> Self {
            /* ??? */
        }

        pub fn deposit(&mut self, amount: u64) {
            /* ??? */
        }

        pub fn withdraw(&mut self, amount: u64) -> Result<(), String> {
            /* ??? */
        }

        pub fn balance(&self) -> u64 {
            /* ??? */
        }
    }
}

fn main() {
    let mut acct = bank::Account::open("Ada");
    acct.deposit(100);
    acct.withdraw(30).unwrap();
    println!("{} has {}", acct.owner, acct.balance());
    println!("{:?}", acct.withdraw(1000));
}
```

<details>
<summary>Solution</summary>

```rust playground
mod bank {
    pub struct Account {
        pub owner: String, // public: freely readable/writable
        balance: u64,      // private: protected by deposit/withdraw
    }

    impl Account {
        pub fn open(owner: &str) -> Self {
            Account { owner: owner.to_string(), balance: 0 }
        }

        pub fn deposit(&mut self, amount: u64) {
            self.balance += amount;
        }

        pub fn withdraw(&mut self, amount: u64) -> Result<(), String> {
            if amount > self.balance {
                return Err("insufficient funds".to_string());
            }
            self.balance -= amount;
            Ok(())
        }

        pub fn balance(&self) -> u64 {
            self.balance
        }
    }
}

fn main() {
    let mut acct = bank::Account::open("Ada");
    acct.deposit(100);
    acct.withdraw(30).unwrap();
    println!("{} has {}", acct.owner, acct.balance());
    println!("{:?}", acct.withdraw(1000));
}
```

**Output:**

```
Ada has 70
Err("insufficient funds")
```

Because `balance` is private and the struct has a private field, no caller outside `bank` can build an `Account` literal or set the balance directly; they must go through the methods.

</details>

### Exercise 3: Pick the tightest restricted visibility

**Difficulty:** Hard

**Objective:** Wire up `pub(crate)`, `pub(super)`, and `pub(in path)` so the program compiles with the narrowest visibility that still works.

**Instructions:** `engine::run` must be callable from `main` (it is internal to this crate, never an external API). `scheduler::tick` should be reachable only from within `engine`. `scheduler::next_id` should be reachable only from within the `engine` subtree. Choose the most restrictive keyword for each.

```rust
mod engine {
    pub mod scheduler {
        fn tick() -> u64 {
            next_id()
        }

        fn next_id() -> u64 {
            7
        }
    }

    fn run() -> u64 {
        scheduler::tick()
    }
}

fn main() {
    println!("{}", engine::run()); // should print 7
}
```

<details>
<summary>Solution</summary>

- `run` is internal-to-the-crate API called from `main` (which is outside `engine`), so `pub(crate)` is the tightest fit.
- `tick` must be visible to `engine` (its grandparent module), so `pub(super)` exposes it to `engine` and everything nested under it.
- `next_id` should be confined to the `engine` subtree, which `pub(in crate::engine)` states explicitly.

```rust playground
mod engine {
    pub mod scheduler {
        // Visible to the parent (`engine`) and its descendants.
        pub(super) fn tick() -> u64 {
            next_id()
        }

        // Visible only within the `engine` module subtree.
        pub(in crate::engine) fn next_id() -> u64 {
            7
        }
    }

    // Visible across the whole crate, but not to external crates.
    pub(crate) fn run() -> u64 {
        scheduler::tick()
    }
}

fn main() {
    println!("{}", engine::run());
}
```

**Output:**

```
7
```

> **Tip:** `engine::run` could have been plain `pub` and the program would still compile, but for a binary crate there is no "external crate" to expose it to, and using `pub` on a genuinely internal item is the habit Pitfall 4 warns against. `pub(crate)` documents the intent precisely.

</details>
