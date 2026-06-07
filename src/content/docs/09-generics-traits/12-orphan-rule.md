---
title: "The Orphan Rule and Coherence"
description: "Rust's orphan rule lets you implement a trait for a type only if you own one side, turning TypeScript's silent prototype-patching clashes into compile errors."
---

In TypeScript you can bolt a method onto `String.prototype`, declare module augmentations, and generally graft behavior onto anything from anywhere. Rust deliberately forbids the most dangerous version of this. The **orphan rule** — one half of Rust's **coherence** guarantee — decides *who* is allowed to write a given `impl Trait for Type`. This file explains the rule, the real compiler error you hit when you break it, and the **newtype** pattern that gets you what you wanted anyway.

---

## Quick Overview

**Coherence** is the compiler's promise that for any given (trait, type) pair there is **at most one** implementation in the entire program: no ambiguity, ever. The **orphan rule** is the concrete restriction that enforces this: you may write `impl Trait for Type` only if **either the trait or the type is local to your crate**. You cannot implement a *foreign* trait for a *foreign* type. That combination is "orphaned," owned by neither your crate nor anyone who can see it. When the rule blocks you, the standard escape hatch is the **newtype** pattern: wrap the foreign type in a one-field local struct, which makes the type local and re-opens the door.

> **Note:** This matters to a TypeScript developer because the rule has no TypeScript analogue. There is nothing stopping two npm packages from both monkey-patching `Array.prototype.flatten`, and the resulting clash is a runtime mystery. Rust converts that whole class of bug into a compile error.

---

## TypeScript/JavaScript Example

In JavaScript and TypeScript, "implementing behavior on a type you don't own" is routine, and routinely fragile. Here we add a method to the built-in `Array` and to a class from an imaginary third-party library:

```typescript
// TypeScript - monkey-patching foreign types is allowed (and risky)

// 1. Augment the global Array type so TypeScript knows about our method.
declare global {
  interface Array<T> {
    second(): T | undefined;
  }
}

// 2. Actually attach it at runtime.
Array.prototype.second = function <T>(this: T[]): T | undefined {
  return this[1];
};

const items = ["a", "b", "c"];
console.log(items.second()); // "b"

// The catch: NOTHING stops another module from doing the same thing.
// If a dependency also defines `Array.prototype.second` with different
// behavior, the last one loaded silently wins. No error, no warning.
Array.prototype.second = function () {
  return "hijacked";
};

console.log(items.second()); // "hijacked" — your method is gone
```

**Key points:**

- You can attach `second()` to `Array.prototype` even though you defined neither `Array` nor (conceptually) the "has a second element" behavior.
- Two independent definitions silently collide. Whichever module is evaluated last overwrites the other.
- The failure surfaces at **runtime**, far from the code that caused it, and is notoriously hard to debug.

This freedom is exactly what Rust's orphan rule trades away, on purpose.

---

## Rust Equivalent

Rust lets you do the *useful* version of the above (adding a method to `Vec`) but forces you to own at least one side of the relationship. Here we define our **own** trait and implement it for the foreign `String` and `i64` types, which is allowed:

```rust
// Rust - implementing a LOCAL trait for FOREIGN types is fine.
trait Summarize {
    fn summary(&self) -> String;
}

impl Summarize for String {
    fn summary(&self) -> String {
        format!("a string of {} bytes", self.len())
    }
}

impl Summarize for i64 {
    fn summary(&self) -> String {
        format!("the integer {self}")
    }
}

fn main() {
    let s = String::from("hello");
    let n: i64 = 42;
    println!("{}", s.summary());
    println!("{}", n.summary());
}
```

Real output:

```text
a string of 5 bytes
the integer 42
```

The trait `Summarize` is **local** (we defined it in this crate), so we may implement it for any type we like, foreign or not. The orphan rule is satisfied because *one* side — the trait — belongs to us. There is no way for another crate to define a conflicting `impl Summarize for String`, because they cannot see our private trait without importing it, and even then they would have to import *our* trait to clash with *our* impl, which the rule also handles.

---

## Detailed Explanation

### The two ingredients: "local" and "foreign"

A trait or type is **local** if it is defined in the crate you are currently compiling, and **foreign** if it comes from another crate, including the standard library (`std`, `core`, `alloc`).

The orphan rule, stated precisely enough to use: **an `impl Trait for Type` is allowed only if `Trait` is local, or `Type` (more precisely, a type constructor in the impl's "self" position) is local.** If *both* are foreign, the impl is orphaned and rejected.

| `impl Trait for Type` | Trait | Type | Allowed? |
| --- | --- | --- | --- |
| `impl Summarize for String` | local | foreign | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> yes: trait is local |
| `impl Display for Wrapper` | foreign | local | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> yes: type is local |
| `impl MyTrait for MyType` | local | local | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> yes: both local |
| `impl Display for Vec<String>` | foreign | foreign | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> no: orphaned |

### Why coherence needs this

Coherence guarantees there is exactly one impl per (trait, type) pair across the *whole* program. Imagine the rule did not exist. Crate `A` could write `impl Display for Vec<String>` to render a comma-separated list, and crate `B` could write `impl Display for Vec<String>` to render newline-separated lines. Your binary depends on both. Now `some_vec.to_string()` has **two** valid meanings, chosen by... what? Link order? Compilation order? There is no good answer, so Rust forbids the situation from arising. Because both `Display` and `Vec` live in `std`/`alloc`, neither `A` nor `B` is allowed to add that impl — only `std` itself can.

> **Tip:** Read the rule as a question of *ownership and responsibility*. If you own the trait, you are responsible for its impls and can coordinate them. If you own the type, likewise. If you own neither, you have no authority to decide how that pairing behaves, and neither should two unrelated crates fight over it.

### The error you actually get

Trying the orphaned impl directly produces a specific, well-known error:

```rust
use std::fmt;

// does not compile (error[E0117]): both `Display` and `Vec<T>` are foreign.
impl fmt::Display for Vec<String> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[{}]", self.join(", "))
    }
}

fn main() {}
```

Real compiler output:

```text
error[E0117]: only traits defined in the current crate can be implemented for types defined outside of the crate
 --> src/main.rs:4:1
  |
4 | impl fmt::Display for Vec<String> {
  | ^^^^^^^^^^^^^^^^^^^^^^-----------
  |                       |
  |                       `Vec` is not defined in the current crate
  |
  = note: impl doesn't have any local type before any uncovered type parameters
  = note: for more information see https://doc.rust-lang.org/reference/items/implementations.html#orphan-rules
  = note: define and implement a trait or new type instead
```

The final note — "define and implement a trait or new type instead" — is the compiler pointing you straight at the two ways out: own the trait, or own the type (via a newtype).

### The newtype workaround

A **newtype** is a tuple struct with a single field that wraps another type: `struct Wrapper(Vec<String>);`. The wrapper is **local to your crate**, so implementing a foreign trait for it satisfies the orphan rule. It compiles to the same layout as the inner value (no runtime overhead), and it exists purely to give you a *local* type you are allowed to add impls to.

```rust
use std::fmt;

// `Wrapper` is a local newtype: a tuple struct holding one foreign type.
struct Wrapper(Vec<String>);

// We OWN `Wrapper` (a local type), so we may implement the FOREIGN trait
// `Display` for it. The orphan rule is satisfied because the type is local.
impl fmt::Display for Wrapper {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[{}]", self.0.join(", "))
    }
}

fn main() {
    let w = Wrapper(vec![String::from("hello"), String::from("world")]);
    println!("w = {w}");
}
```

Real output:

```text
w = [hello, world]
```

You reach the inner value through the tuple field `self.0`. To convert in and out ergonomically, a `From` impl is idiomatic:

```rust
struct Wrapper(Vec<String>);

impl From<Vec<String>> for Wrapper {
    fn from(v: Vec<String>) -> Self {
        Wrapper(v)
    }
}

fn main() {
    let w: Wrapper = vec![String::from("a"), String::from("b")].into();
    // Access the inner value through the `.0` tuple field.
    println!("first = {}", w.0[0]);
    println!("len = {}", w.0.len());
}
```

Real output:

```text
first = a
len = 2
```

The one inconvenience: a newtype does *not* automatically expose the inner type's methods. The next section's pitfalls and the Real-World Example show how `Deref` recovers them when that is what you want.

---

## Key Differences

| Aspect | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Add a method to a foreign type | `Foo.prototype.bar = ...`, always allowed | Only via a **local trait** or a **local newtype** |
| Two libraries define the same extension | Silent runtime clash; last load wins | **Compile error** if it could be ambiguous |
| When conflicts are detected | At runtime, possibly far from the cause | At compile time, before the program runs |
| Guarantee of "one meaning per method" | None | **Coherence** — at most one impl per (trait, type) |
| Workaround for "I don't own either side" | Not needed (you just patch it) | **Newtype** wrapper makes the type local |
| Runtime cost of the workaround | n/a | Zero: a newtype is the same layout as its inner value |

### Coherence is global; the orphan rule is local

Coherence is the *property* ("at most one impl program-wide"). The orphan rule is the *mechanism* the compiler uses to guarantee that property while only ever looking at one crate at a time. Because the compiler cannot see every other crate that might exist now or in the future, it must reject any impl that *could* clash with one written elsewhere. The orphan rule is a conservative, decidable approximation: "if neither side is yours, someone else might also write this, so no."

### A second coherence check: overlap within a crate

Even when the orphan rule is satisfied, coherence still forbids **overlapping** impls of the same trait for the same type *inside your own crate*:

```rust
trait Greet {
    fn greet(&self) -> String;
}

// does not compile (error[E0119]): two impls of the same trait for `i32`.
impl Greet for i32 {
    fn greet(&self) -> String {
        String::from("hi")
    }
}

impl Greet for i32 {
    fn greet(&self) -> String {
        String::from("hello")
    }
}

fn main() {}
```

Real compiler output:

```text
error[E0119]: conflicting implementations of trait `Greet` for type `i32`
  --> src/main.rs:12:1
   |
 6 | impl Greet for i32 {
   | ------------------ first implementation here
...
12 | impl Greet for i32 {
   | ^^^^^^^^^^^^^^^^^^ conflicting implementation for `i32`
```

`E0117` (orphan rule) and `E0119` (overlap) are the two faces of coherence: the first stops cross-crate ambiguity, the second stops within-crate ambiguity.

---

## Common Pitfalls

### Pitfall 1: Trying to implement a `std` trait for a `std` type

The classic stumble is wanting `Display` (or `Serialize`, `FromStr`, `PartialOrd`, ...) on `Vec`, `HashMap`, `String`, or another standard type. Both sides are foreign, so it is rejected with `error[E0117]` (shown above). The fix is a newtype: wrap the standard type in your own struct and implement the trait on the wrapper.

### Pitfall 2: Assuming the newtype "just works" like the inner type

A newtype starts with **none** of the inner type's methods:

```rust
struct Wrapper(Vec<String>);

fn main() {
    let w = Wrapper(vec![String::from("a")]);
    let n = w.len(); // does not compile (error[E0599]): `Wrapper` has no method `len`
    println!("{n}");
}
```

The real error is `error[E0599]: no method named `len` found for struct `Wrapper``. You either go through the field explicitly (`w.0.len()`) or implement `Deref` to forward calls (see the Real-World Example).

### Pitfall 3: Thinking a generic parameter makes the impl "local enough"

A subtlety the error message hints at ("any uncovered type parameters"): wrapping a foreign generic does not help if the *type constructor* is still foreign. `impl ForeignTrait for Vec<MyLocalType>` is **still rejected** — `Vec` itself is foreign, and `MyLocalType` appears only *inside* it, "uncovered" behind a foreign constructor. The local type must be the outermost one: `impl ForeignTrait for MyWrapper(Vec<...>)` works because `MyWrapper` is the head type and it is yours.

> **Warning:** "I used my own type somewhere in there" is not the test. The orphan rule looks at the *first* (left-to-right, outermost) type constructor that is not a bare type parameter. That constructor must be local.

### Pitfall 4: Expecting structural typing to bypass the rule

Coming from TypeScript's structural typing, you might expect that if your type "looks like" the foreign one, you can share impls. Rust is **nominal**: `Wrapper(Vec<String>)` and `Vec<String>` are entirely distinct types regardless of identical layout. The wrapper does not inherit the inner type's trait impls, and the inner type does not gain the wrapper's. This is the price — and the point — of the rule.

---

## Best Practices

- **Reach for a newtype the moment you need a foreign trait on a foreign type.** It is the idiomatic, zero-cost answer; do not fight the rule with `unsafe` or clever generics.
- **Prefer owning the trait when you can.** If you control the trait, you can implement it for any foreign types directly, no wrapper needed. A local trait that you implement for `String`, `i64`, etc., is clean and conflict-free (the first Rust example).
- **Add `Deref` to a newtype only when you want transparent method forwarding** (for example, a smart-pointer-like wrapper). For a newtype whose whole purpose is to *restrict* or *change* behavior (a validated `Email(String)`), deliberately **omit** `Deref` so callers cannot bypass your invariants. See [Smart Pointers](/10-smart-pointers/) for `Deref` in depth.
- **Use `#[derive(...)]` on the newtype to recover common impls cheaply.** Deriving `Debug`, `Clone`, `PartialEq`, etc. on the wrapper is usually all you need to make it pleasant to use.
- **Expose conversions with `From`/`Into`** so wrapping and unwrapping read naturally (`value.into()`), and add accessor methods or `into_inner(self) -> Inner` rather than making the `.0` field part of your public API.
- **Lean on the rule, do not resent it.** When the compiler rejects an orphan impl, it is preventing a real, hard-to-debug clash. The newtype is a feature, not a tax.

---

## Real-World Example

A common production need: render HTTP headers in a custom format. `Display` is foreign and `HashMap` is foreign, so you cannot implement `Display for HashMap` directly. The orphan-rule-respecting solution is a `Headers` newtype, given a custom `Display` and a `Deref` so callers can still use the underlying `HashMap` API transparently.

```rust
use std::collections::HashMap;
use std::fmt;
use std::ops::Deref;

// Newtype around the foreign type `HashMap<String, String>`.
// This lets us give it a custom `Display` (a foreign trait) for rendering
// HTTP-style headers — something we could never do on the bare HashMap.
struct Headers(HashMap<String, String>);

impl Headers {
    fn new() -> Self {
        Headers(HashMap::new())
    }

    fn insert(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_string(), value.to_string());
    }
}

// `Deref` forwards method calls to the inner map, so callers can use
// `.get`, `.len`, `.contains_key`, etc. directly on a `Headers`.
impl Deref for Headers {
    type Target = HashMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for Headers {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Sort for deterministic output.
        let mut entries: Vec<_> = self.0.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (key, value) in entries {
            writeln!(f, "{key}: {value}")?;
        }
        Ok(())
    }
}

fn main() {
    let mut headers = Headers::new();
    headers.insert("Content-Type", "application/json");
    headers.insert("X-Request-Id", "abc-123");

    // Deref lets us call HashMap methods straight through the newtype.
    println!("header count: {}", headers.len());
    println!("has content-type: {}", headers.contains_key("Content-Type"));

    // Our custom Display does the rendering.
    print!("{headers}");
}
```

Real output:

```text
header count: 2
has content-type: true
Content-Type: application/json
X-Request-Id: abc-123
```

This is the same instinct as the TypeScript monkey-patch — "give this collection a nicer toString" — but Rust routes it through a type *you* own, so two crates can each have their own `Headers`-style wrapper without ever colliding.

> **Note:** This is exactly why serde provides `#[serde(remote = "...")]` and why crates publish "newtype" wrapper helpers: the ecosystem builds *around* the orphan rule rather than against it, because the guarantee it buys — no surprise impl conflicts when you add a dependency — is worth far more than the convenience it costs.

---

## Further Reading

### Official documentation

- [The Rust Book — Implementing a Trait on a Type (and the orphan rule)](https://doc.rust-lang.org/book/ch10-02-traits.html#implementing-a-trait-on-a-type)
- [The Rust Book — Using the Newtype Pattern to Implement External Traits](https://doc.rust-lang.org/book/ch20-02-advanced-traits.html#using-the-newtype-pattern-to-implement-external-traits-on-external-types)
- [The Rust Reference — Implementation coherence & orphan rules](https://doc.rust-lang.org/reference/items/implementations.html#orphan-rules)
- [Rust error index — E0117](https://doc.rust-lang.org/error_codes/E0117.html) and [E0119](https://doc.rust-lang.org/error_codes/E0119.html)

### Related sections in this guide

- [Section 09 overview](/09-generics-traits/): the full map of generics and traits
- [Traits](/09-generics-traits/03-traits/): defining and implementing a trait; the `impl Trait for Type` syntax this rule governs
- [Trait Bounds](/09-generics-traits/05-trait-bounds/): why bounds are nominal, which is the same reason the orphan rule is nominal
- [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/): `Box<dyn Trait>` for heterogeneous collections
- [Operator Overloading](/09-generics-traits/10-operator-overloading/) — implementing `Add`, `Index`, etc.; often combined with newtypes
- [Marker Traits](/09-generics-traits/11-marker-traits/) — `Send`, `Sync`, `Copy`, `Sized`, and the auto-trait variation of these rules
- [Generic Functions](/09-generics-traits/00-generic-functions/) — monomorphization vs TypeScript type erasure
- [Smart Pointers](/10-smart-pointers/) — `Deref`, the trait that makes newtype forwarding ergonomic
- [Section 02: Basics](/02-basics/) and [Section 01: Getting Started](/01-getting-started/) — foundational concepts (tuple structs, `struct` basics)

---

## Exercises

### Exercise 1: A `Display` newtype for a primitive

**Difficulty:** Easy

**Objective:** Use the newtype pattern to attach a foreign trait (`Display`) to a wrapper around a foreign type (`f64`).

**Instructions:** Define a newtype `Celsius(f64)`. Implement `std::fmt::Display` for it so that a temperature prints with one decimal place followed by `°C` (for example, `21.5°C`). In `main`, construct `Celsius(21.5)` and print it with `println!`.

```rust
use std::fmt;

struct Celsius(f64);

// TODO: impl fmt::Display for Celsius, printing "{:.1}°C"

fn main() {
    // TODO: build Celsius(21.5) and print it
}
```

<details>
<summary>Solution</summary>

```rust
use std::fmt;

struct Celsius(f64);

impl fmt::Display for Celsius {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:.1}°C", self.0)
    }
}

fn main() {
    let temp = Celsius(21.5);
    println!("Current temperature: {temp}");
}
```

**Output:**

```text
Current temperature: 21.5°C
```

> Both `Display` and `f64` are foreign, so this only compiles because `Celsius` is a local type. Implementing `Display` for bare `f64` would fail with `error[E0117]`.

</details>

### Exercise 2: A foreign operator trait on a wrapped collection

**Difficulty:** Medium

**Objective:** Implement the foreign `std::ops::Add` trait for a newtype around the foreign `Vec<i32>`, an impl the orphan rule would forbid on `Vec` itself.

**Instructions:** Define a newtype `IntList(Vec<i32>)` and derive `Debug`. Implement `Add` so that adding two `IntList`s concatenates their inner vectors (the elements of the left list followed by the elements of the right). In `main`, add `IntList(vec![1, 2, 3])` and `IntList(vec![4, 5])` and print the result with `{:?}`.

```rust
use std::ops::Add;

#[derive(Debug)]
struct IntList(Vec<i32>);

// TODO: impl Add for IntList, concatenating the inner vectors

fn main() {
    // TODO: add two IntLists and print the result with {:?}
}
```

<details>
<summary>Solution</summary>

```rust
use std::ops::Add;

// Newtype around the foreign type `Vec<i32>`.
#[derive(Debug)]
struct IntList(Vec<i32>);

// `Add` is foreign and `Vec<i32>` is foreign, but `IntList` is local,
// so this impl is allowed by the orphan rule.
impl Add for IntList {
    type Output = IntList;
    fn add(self, other: IntList) -> IntList {
        let mut combined = self.0;
        combined.extend(other.0);
        IntList(combined)
    }
}

fn main() {
    let a = IntList(vec![1, 2, 3]);
    let b = IntList(vec![4, 5]);
    let c = a + b;
    println!("{c:?}");
}
```

**Output:**

```text
IntList([1, 2, 3, 4, 5])
```

> Operator overloading lives in [Operator Overloading](/09-generics-traits/10-operator-overloading/); the orphan rule is what makes wrapping `Vec` necessary to do it here.

</details>

### Exercise 3: Newtype + `Deref` to add a foreign trait to a foreign type

**Difficulty:** Hard

**Objective:** Model the real-world bind (a trait from one crate, a type from another, neither of which you control) and resolve it with a newtype plus `Deref` for transparent field access.

**Instructions:** Treat `Money { cents: u64 }` and the trait `Render { fn render(&self) -> String; }` as if they came from two different external crates you cannot edit (so `impl Render for Money` is impossible by the orphan rule). Define a local newtype `Priced(Money)`. Implement `Deref<Target = Money>` so that `priced.cents` reads through to the inner value, then implement `Render` for `Priced` to format the amount as dollars (for example, `1995` cents becomes `$19.95`; always show two cents digits). In `main`, build `Priced(Money { cents: 1995 })`, print its raw `cents` (via `Deref`), and print `render()`.

```rust
use std::ops::Deref;

#[derive(Clone, Copy)]
struct Money {
    cents: u64,
}

trait Render {
    fn render(&self) -> String;
}

struct Priced(Money);

// TODO: impl Deref for Priced (Target = Money)
// TODO: impl Render for Priced, formatting cents as "$D.CC"

fn main() {
    // TODO: build Priced(Money { cents: 1995 }), print .cents and .render()
}
```

<details>
<summary>Solution</summary>

```rust
use std::ops::Deref;

// Imagine `Money` is a type from an external crate that we cannot edit,
// and `Render` is a trait from ANOTHER external crate. We cannot write
// `impl Render for Money` (orphan rule). The fix: wrap it.

// --- pretend these two come from different foreign crates ---
#[derive(Clone, Copy)]
struct Money {
    cents: u64,
}

trait Render {
    fn render(&self) -> String;
}
// -----------------------------------------------------------

// Local newtype wrapper.
struct Priced(Money);

impl Deref for Priced {
    type Target = Money;
    fn deref(&self) -> &Money {
        &self.0
    }
}

// Allowed: `Priced` is local, even though both `Render` and `Money`
// would be foreign in the real-world scenario.
impl Render for Priced {
    fn render(&self) -> String {
        let dollars = self.cents / 100;
        let cents = self.cents % 100;
        format!("${dollars}.{cents:02}")
    }
}

fn main() {
    let price = Priced(Money { cents: 1995 });
    // Deref lets us read `.cents` straight through the wrapper.
    println!("raw cents: {}", price.cents);
    println!("rendered:  {}", price.render());
}
```

**Output:**

```text
raw cents: 1995
rendered:  $19.95
```

> `self.cents` inside `render` works because `Deref` auto-dereferences `&Priced` to `&Money` when looking up the field. The `Deref` trait and its method-resolution magic are covered in [Smart Pointers](/10-smart-pointers/).

</details>
