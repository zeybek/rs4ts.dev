---
title: "Extension Traits"
description: "Add methods to types you don't own with a scoped, collision-free local trait and blanket impl — Rust's safe answer to patching Array.prototype in JS."
---

The **extension trait** is Rust's disciplined, conflict-free answer to a question TypeScript developers usually solve by reaching for `Array.prototype` or module augmentation: *"How do I add a method to a type I don't own?"* You cannot write `impl str { ... }` outside the standard library, and monkey-patching does not exist. Instead you declare a **local trait** with the methods you want and implement it for the foreign type, often with a blanket impl that covers an entire category of types at once, exactly how `Iterator`'s 70-plus adapter methods are layered on top of `next`.

---

## Quick Overview

An **extension trait** is a trait you define solely to attach new methods to types that already exist: primitives like `str` and `u32`, standard-library types like `Vec<T>` and `Result<T, E>`, or types from another crate. The pattern matters to a TypeScript/JavaScript developer because it replaces two fragile habits at once. In JavaScript you augment `Array.prototype` (a global, last-writer-wins mutation that any module can clobber and that pollutes every array everywhere); in TypeScript you pair that with `declare global { interface Array<T> { ... } }`. Rust's version is **scoped, explicit, and collision-resistant**: the new method only exists where the trait is `use`d, so two crates can both add a `.tally()` method without stepping on each other, and the compiler tells you precisely when you forgot to import it.

> **Note:** This file is about adding *methods* to a foreign type. The sibling [Newtype Pattern](/22-common-patterns/01-newtype/) is the *other* answer to "work with a foreign type": it wraps the type in a local struct so you can implement *foreign traits* on it. Reach for an extension trait when you only need new methods; reach for a newtype when you need a foreign trait impl or a new identity. The orphan rule that motivates both is covered in [The Orphan Rule and Coherence](/09-generics-traits/12-orphan-rule/).

---

## TypeScript/JavaScript Example

Say you want a `.tally()` method on every array that counts how often each element appears. In JavaScript the canonical move is to add it to `Array.prototype`; in TypeScript you also declare the augmentation so the type checker knows about it.

```typescript
// TypeScript: monkey-patching a built-in via prototype + global augmentation.
declare global {
  interface Array<T> {
    tally(): Map<T, number>;
  }
}

Array.prototype.tally = function <T>(this: T[]): Map<T, number> {
  const counts = new Map<T, number>();
  for (const item of this) {
    counts.set(item, (counts.get(item) ?? 0) + 1);
  }
  return counts;
};

const words = ["a", "b", "a", "c", "b", "a"];
console.log(words.tally());

export {};
```

Running it under Node v22 (via `tsx`) prints:

```text
Map(3) { 'a' => 3, 'b' => 2, 'c' => 1 }
```

**Key points — and the problems hiding in them:**

- The method works on *every* array in the *entire* program, including arrays inside dependencies you never meant to touch. There is no scoping.
- It is **last-writer-wins**: if another module also defines `Array.prototype.tally`, one silently overwrites the other, and load order decides which.
- Modifying built-in prototypes is widely considered an anti-pattern precisely because it is global and invisible. MDN and most style guides warn against it.
- The `declare global` block makes the type checker happy but does nothing at runtime; the two pieces can drift out of sync.

---

## Rust Equivalent

Rust does not let you reopen a foreign type to add a method. The idiomatic move is to declare a **local trait** carrying the method and write a **blanket implementation** for every iterator. The capability appears only where the trait is in scope.

```rust playground
use std::collections::HashMap;
use std::hash::Hash;

// 1. Declare a LOCAL trait carrying the method(s) we want to "attach".
//    `: Iterator` makes it a subtrait, so we get `Self::Item` and `self` is an iterator.
trait IteratorExt: Iterator {
    // Count how many times each item appears, consuming the iterator.
    fn tally(self) -> HashMap<Self::Item, usize>
    where
        Self: Sized,
        Self::Item: Eq + Hash,
    {
        let mut counts = HashMap::new();
        for item in self {
            *counts.entry(item).or_insert(0) += 1;
        }
        counts
    }
}

// 2. One blanket impl: EVERY iterator now has `.tally()`.
impl<I: Iterator> IteratorExt for I {}

fn main() {
    let words = ["a", "b", "a", "c", "b", "a"];
    let counts = words.iter().copied().tally();

    // Sort for deterministic output (HashMap iteration order is unspecified).
    let mut pairs: Vec<_> = counts.into_iter().collect();
    pairs.sort();
    println!("{pairs:?}");
}
```

Output:

```text
[("a", 3), ("b", 2), ("c", 1)]
```

**Key points:**

- `IteratorExt` is **your** trait, so implementing it is legal even though `Iterator` and every concrete iterator type belong to the standard library: the orphan rule is satisfied because the *trait* is local.
- The single line `impl<I: Iterator> IteratorExt for I {}` is a **blanket impl**: it adds `.tally()` to all 100-plus iterator types in std and any iterators in your dependencies, with no per-type boilerplate.
- The method body lives as a **default method** on the trait, so the blanket impl can be empty `{}` — implementors inherit the behavior. This is exactly how `Iterator::map`, `filter`, `collect`, and friends are built on top of the single required method `next`.
- Importantly, `.tally()` only exists in modules that `use` the trait. Nothing leaks globally; another crate's `tally` cannot collide with yours.

---

## Detailed Explanation

### Why you cannot just reopen the type

The first thing a TypeScript developer tries is the direct analog of editing a prototype: add an inherent method to the foreign type.

```rust
// does not compile (error[E0390]): you may not add inherent methods to a foreign/primitive type.
impl str {
    fn shout(&self) -> String {
        self.to_uppercase()
    }
}

fn main() {}
```

The real compiler error even names the cure:

```text
error[E0390]: cannot define inherent `impl` for primitive types
 --> src/main.rs:2:1
  |
2 | impl str {
  | ^^^^^^^^
  |
  = help: consider using an extension trait instead
```

> **Note:** "consider using an extension trait instead" is the compiler's own phrasing. The pattern in this file is the blessed, named solution, not a workaround. The same restriction applies to non-primitive foreign types: you may add inherent `impl` blocks only to types your crate defines.

### The recipe, generalized

Every extension trait follows the same three-step shape:

1. **Declare a local trait** with the new method signatures (and usually default bodies).
2. **Implement it** for the foreign type: one concrete impl, or a blanket impl over a bound.
3. **`use` the trait** wherever you want the methods. The methods are inert until the trait is in scope.

Here it is on the primitive `str`, adding two text-formatting helpers:

```rust playground
// 1. Local trait with the methods we want on string slices.
trait StrExt {
    fn truncate_with_ellipsis(&self, max: usize) -> String;
    fn is_blank(&self) -> bool;
}

// 2. Implement it directly for the foreign primitive `str`.
impl StrExt for str {
    fn truncate_with_ellipsis(&self, max: usize) -> String {
        if self.chars().count() <= max {
            self.to_string()
        } else {
            let kept: String = self.chars().take(max.saturating_sub(1)).collect();
            format!("{kept}…")
        }
    }
    fn is_blank(&self) -> bool {
        self.trim().is_empty()
    }
}

fn main() {
    // 3. Because StrExt is in scope here, the methods exist on every &str.
    let title = "The Rust Programming Language";
    println!("{}", title.truncate_with_ellipsis(10));
    println!("{}", "   ".is_blank());
    println!("{}", "x".is_blank());
}
```

Output:

```text
The Rust …
true
false
```

Implementing for `str` (the unsized slice) rather than `String` means the methods work on string literals, `&String` (via deref coercion), and substrings alike: implement on the most general type that makes sense.

### Default methods vs. required methods

A subtrait like `trait IteratorExt: Iterator` can lean on the supertrait's API to provide *default* method bodies, which is why the blanket impl is empty. But an extension trait does not have to be a subtrait; it can declare *required* methods that each implementor must supply. The `StrExt` above does exactly that: no default bodies, so `impl StrExt for str` must implement both methods.

The pattern shines when you combine the two: a few small required methods plus many default methods built on top of them. That is the standard library's `Iterator` design and the reason `itertools` (`cargo add itertools`) can bolt dozens of extra adapters onto every iterator with one blanket impl.

### Adding a *lazy* adapter, the way `map`/`filter` do

Extension methods are not limited to "consume and return a value." You can return a brand-new lazy iterator type, exactly like `map` returns `Map<...>`. Define the adapter struct, implement `Iterator` for it, then add a method on an extension trait that wraps `self`:

```rust playground
// A lazy iterator adapter that yields cumulative sums.
struct RunningTotal<I> {
    iter: I,
    acc: i64,
}

impl<I: Iterator<Item = i64>> Iterator for RunningTotal<I> {
    type Item = i64;
    fn next(&mut self) -> Option<i64> {
        let x = self.iter.next()?;
        self.acc += x;
        Some(self.acc)
    }
}

// The extension trait that introduces `.running_total()`.
trait IteratorMathExt: Iterator<Item = i64> + Sized {
    fn running_total(self) -> RunningTotal<Self> {
        RunningTotal { iter: self, acc: 0 }
    }
}
impl<I: Iterator<Item = i64>> IteratorMathExt for I {}

fn main() {
    let totals: Vec<i64> = vec![10, 20, 30, 40].into_iter().running_total().collect();
    println!("{totals:?}");

    // It is genuinely lazy, so it composes with other adapters and even infinite ranges.
    let first_four: Vec<i64> =
        (1..).map(|x| x as i64).running_total().take(4).collect();
    println!("{first_four:?}");
}
```

Output:

```text
[10, 30, 60, 100]
[1, 3, 6, 10]
```

The second example chains `.map().running_total().take(4)` over the infinite range `1..` and terminates: proof the adapter pulls items on demand rather than eagerly, the same laziness Rust's built-in adapters have. (Laziness of iterators is contrasted with eager JavaScript array methods in [Section 07: Collections](/07-collections/).)

### Scope is the whole point

Because an extension method is reachable only when its trait is imported, two libraries can each define a `.tally()` on iterators and a consumer can use whichever (or both, by `use`ing one at a time). Compare this to `Array.prototype.tally`, where a second definition silently wins globally. Extension methods are opt-in per module, which is why the standard library can expose `Iterator` and `Itertools` (from the `itertools` crate) side by side without conflict.

---

## Key Differences

| Concern | TypeScript / JavaScript | Rust extension trait |
| --- | --- | --- |
| Mechanism | mutate `Array.prototype` + `declare global` | local trait + `impl` for the foreign type |
| Scope of the new method | global; affects every value of that type everywhere | only where the trait is `use`d |
| Two libraries add the same method name | last-writer-wins; one silently clobbers the other | both coexist; consumer imports the one it wants |
| Runtime cost | a prototype lookup; a real heap-mutated object graph | zero: monomorphized, statically dispatched calls |
| Discoverability of failure | a `TypeError: x.tally is not a function` at runtime | a compile error naming the missing `use` |
| Type-checker vs. runtime sync | two separate pieces that can drift | one declaration; impossible to desync |
| Adding to a primitive (`number`, `string`) | augment `Number.prototype` / `String.prototype` | `impl MyExt for i64 / str` |

The headline: **JavaScript extension is global and mutable; Rust extension is scoped and static.** Monkey-patching changes the shared world; an extension trait grants a capability locally, the compiler enforces the import, and the call compiles down to an ordinary direct function call.

> **Note:** If you have used C#'s extension methods or Kotlin's extension functions, the *intent* is identical. The Rust twist is that the capability rides on a *trait* you must bring into scope, so it is even more explicit about where the method is available, and it can be generic and blanket-implemented in ways prototype-patching cannot match.

---

## Common Pitfalls

### Pitfall 1: Forgetting to bring the trait into scope

This is the single most common stumble, and it is the price of the pattern's scoping. The method exists, but it is invisible until you `use` the trait:

```rust
mod ext {
    pub trait IteratorExt: Iterator {
        fn second(mut self) -> Option<Self::Item>
        where
            Self: Sized,
        {
            self.next();
            self.next()
        }
    }
    impl<I: Iterator> IteratorExt for I {}
}

// does not compile (error[E0599]): we forgot `use ext::IteratorExt;`
fn main() {
    let v = vec![10, 20, 30];
    let _ = v.into_iter().second();
}
```

The real compiler error spells out the fix:

```text
error[E0599]: no method named `second` found for struct `std::vec::IntoIter` in the current scope
  --> src/main.rs:17:27
   |
 3 |         fn second(mut self) -> Option<Self::Item>
   |            ------ the method is available for `std::vec::IntoIter<{integer}>` here
...
17 |     let _ = v.into_iter().second();
   |                           ^^^^^^
   |
   = help: items from traits can only be used if the trait is in scope
...
help: trait `IteratorExt` which provides `second` is implemented but not in scope; perhaps you want to import it
   |
 1 + use crate::ext::IteratorExt;
   |
```

**Fix:** add `use crate::ext::IteratorExt;` (or whatever path the trait lives at). Library authors customarily re-export their extension traits from a `prelude` module — `use my_crate::prelude::*;` — so consumers get the methods with one import. (Module paths and re-exports are covered in [Section 12: Modules and Packages](/12-modules-packages/).)

### Pitfall 2: Expecting it to defeat the orphan rule

An extension trait lets you add *methods* (via a local trait), but it does **not** let you implement a *foreign trait* for a *foreign type*. You still cannot write `impl std::fmt::Display for Vec<String>`: there is no local trait there at all. If you need a foreign trait on a foreign type, you need a [newtype](/22-common-patterns/01-newtype/), not an extension trait. The two patterns answer different halves of "I don't own this type."

### Pitfall 3: A blanket impl that is broader than you intended

`impl<I: Iterator> IteratorExt for I {}` adds your method to *every* iterator, which is usually the goal, but if your method only makes sense for, say, iterators of `i64`, constrain the bound (`impl<I: Iterator<Item = i64>> ...`) or you will offer a method that fails to compile when called on the wrong element type. Make the bound say exactly what the method requires.

### Pitfall 4: Colliding with an inherent method (inherent methods win)

If the foreign type already has an inherent method with the same name, the inherent method takes priority in method resolution and your extension method is shadowed — calling it just invokes the original. Name extension methods distinctly (or call them with fully-qualified syntax, `Trait::method(value)`, when you truly need yours). This is the same precedence rule that lets std add new inherent methods without breaking your extension traits.

### Pitfall 5: Putting heavy logic in a default method without the right bounds

Default methods can only use what the trait's supertraits and `where` clauses guarantee. If `tally` needs `Self::Item: Eq + Hash`, that bound must be on the method (as a `where` clause) or the trait — otherwise the body will not compile. Adding bounds at the *method* level (as we did) keeps the trait usable for items that are not hashable while still offering `tally` to those that are.

---

## Best Practices

### 1. Name the trait `XxxExt` and keep it focused

The community convention is `IteratorExt`, `StrExt`, `SliceRandom` (rand), `Itertools` (itertools): a noun describing the extended type plus `Ext`. Keep each extension trait small and cohesive; a grab-bag trait is harder to import selectively and reason about.

### 2. Re-export extension traits from a `prelude`

Because the methods are useless until imported, give consumers a one-line on-ramp:

```rust
// In your library's lib.rs
pub mod prelude {
    pub use crate::ext::IteratorExt;
    // ...other extension traits
}
```

Now downstream code writes `use my_crate::prelude::*;` and gets every extension method. This is exactly how `rayon::prelude::*` delivers `.par_iter()` and how `itertools` is meant to be used.

### 3. Prefer a blanket impl over the most general type

Implement on `str` rather than `String`, on `[T]` rather than `Vec<T>`, and use `impl<I: Iterator> ... for I` rather than enumerating concrete iterators. The broader (but correctly bounded) the impl, the more places your method works — including types you have never heard of.

### 4. Use a sealed supertrait when the set of implementors must stay closed

Sometimes you want an extension trait that *only you* can implement, so you can add methods later without it being a breaking change and so downstream code cannot implement it for surprising types. The **sealed trait** idiom enforces this with a private supertrait:

```rust playground
mod sealed {
    pub trait Sealed {}
    impl Sealed for i32 {}
    impl Sealed for i64 {}
}

// `Doubler` requires the private `Sealed`, which only this crate can implement,
// so no downstream crate can add `impl Doubler for ...`.
pub trait Doubler: sealed::Sealed {
    fn doubled(&self) -> Self;
}

impl Doubler for i32 {
    fn doubled(&self) -> i32 { self * 2 }
}
impl Doubler for i64 {
    fn doubled(&self) -> i64 { self * 2 }
}

fn main() {
    println!("{}", 21_i32.doubled());
    println!("{}", 100_i64.doubled());
}
```

Output:

```text
42
200
```

The `Sealed` trait is `pub` in name only; its module gates who can implement it. Downstream crates can *call* `.doubled()` but cannot `impl Doubler` for their own types. (More on visibility in [Section 12](/12-modules-packages/); the orphan rule that makes sealing meaningful is in [Section 09](/09-generics-traits/12-orphan-rule/).)

### 5. Reach for an existing crate before hand-rolling

For iterators specifically, `itertools` already provides a huge `Itertools` extension trait (`.unique()`, `.chunk_by()`, `.dedup()`, `.sorted()`, and more) via the same blanket-impl mechanism. Adding your own is fine, but check whether the method you want already exists in `itertools`, `tap`, or another well-known crate. See [Section 23: The Ecosystem](/23-ecosystem/).

---

## Real-World Example

A production codebase frequently wants ergonomic helpers on `Result`: log the error branch without consuming the `Result`, or attach a higher-level message. These belong on an extension trait so they read like native combinators (`?`-friendly, chainable). This mirrors the shape of helpers found across the `anyhow`/`tracing` ecosystem.

```rust playground
use std::fmt::Display;

// A local extension trait that adds ergonomic helpers to ANY Result.
trait ResultExt<T, E> {
    /// Run `f` on the error (e.g. log it) and pass the Result through unchanged.
    fn inspect_err_with<F: FnOnce(&E)>(self, f: F) -> Self;

    /// Replace the error with a higher-level message, keeping the original via Display.
    fn context(self, msg: &str) -> Result<T, String>
    where
        E: Display;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn inspect_err_with<F: FnOnce(&E)>(self, f: F) -> Self {
        if let Err(ref e) = self {
            f(e);
        }
        self
    }

    fn context(self, msg: &str) -> Result<T, String>
    where
        E: Display,
    {
        self.map_err(|e| format!("{msg}: {e}"))
    }
}

fn parse_port(s: &str) -> Result<u16, std::num::ParseIntError> {
    s.parse::<u16>()
}

fn main() {
    // Success path: the error inspector never runs; context is a no-op.
    let ok = parse_port("8080")
        .inspect_err_with(|e| eprintln!("(won't print) {e}"))
        .context("invalid PORT");
    println!("{ok:?}");

    // Failure path: we log the raw cause, then attach a higher-level message.
    let bad = parse_port("not-a-port")
        .inspect_err_with(|e| eprintln!("logged: {e}"))
        .context("invalid PORT");
    println!("{bad:?}");
}
```

Standard output:

```text
Ok(8080)
Err("invalid PORT: invalid digit found in string")
```

Standard error:

```text
logged: invalid digit found in string
```

Because `inspect_err_with` returns `Self` unchanged, it slots into a chain without breaking the value flow: perfect for "log on the way past." The `context` method demonstrates a default-style ergonomic that the standard `anyhow::Context` trait provides for real (`cargo add anyhow`); here we build a teaching-sized version from scratch so the mechanism is visible. Both methods are reachable only where `ResultExt` is imported, so they never collide with std's growing inventory of `Result` methods. (Error-handling layering is the focus of the sibling [Error-Handling Patterns](/22-common-patterns/03-error-propagation/) and [Section 08](/08-error-handling/).)

> **Tip:** When your extension method would shadow or duplicate a real std method (std grew `Result::inspect_err` in 1.76), pick a distinct name like `inspect_err_with`. This both avoids the inherent-vs-extension precedence surprise from Pitfall 4 and keeps your intent clear.

---

## Further Reading

### Official Documentation

- [The Rust Book — Defining a Trait and Implementing It on a Type](https://doc.rust-lang.org/book/ch10-02-traits.html) — the trait mechanics that underpin extension traits.
- [Rust API Guidelines — "Sealed traits protect against downstream implementations"](https://rust-lang.github.io/api-guidelines/future-proofing.html#sealed-traits-protect-against-downstream-implementations-c-sealed) — the sealed-trait idiom from Best Practice 4.
- [`std::iter::Iterator`](https://doc.rust-lang.org/std/iter/trait.Iterator.html) — the canonical extension-trait-style design: one required method, dozens of provided ones.
- [`itertools::Itertools`](https://docs.rs/itertools/latest/itertools/trait.Itertools.html) — a production extension trait you can `cargo add` today.

### Related Topics in This Guide

- [The Newtype Pattern](/22-common-patterns/01-newtype/) — the *other* answer to "work with a foreign type": wrap it to implement foreign *traits*.
- [The Orphan Rule and Coherence](/09-generics-traits/12-orphan-rule/) — why a *local* trait is the key that makes implementing on a foreign type possible.
- [Traits and Generics](/09-generics-traits/): blanket impls, supertraits, and default methods in depth.
- [Section 07: Collections](/07-collections/) — iterators and their lazy adapters, which extension traits extend.
- [Error-Handling Patterns](/22-common-patterns/03-error-propagation/) — where the `ResultExt` real-world example leads.
- [The Decorator Pattern](/22-common-patterns/06-decorator-pattern/) — a sibling pattern for wrapping behavior rather than attaching methods.
- [Section 23: The Ecosystem](/23-ecosystem/): crates like `itertools`, `tap`, and `anyhow` that ship extension traits.

---

## Exercises

### Exercise 1: A `second` accessor for slices

**Difficulty:** Beginner

**Objective:** Add a method to a foreign type via a local trait and a blanket impl.

**Instructions:** Define a trait `SliceExt<T>` with a method `second(&self) -> Option<&T>` and implement it for `[T]` (so it works on arrays, `Vec`, and slices). Print the second element of `[10, 20, 30]`.

```rust
trait SliceExt<T> {
    fn second(&self) -> Option<&T>;
}

// TODO: impl SliceExt for [T]

fn main() {
    let v = [10, 20, 30];
    println!("{:?}", v.second());
}
```

<details>
<summary>Solution</summary>

```rust playground
trait SliceExt<T> {
    fn second(&self) -> Option<&T>;
}

impl<T> SliceExt<T> for [T] {
    fn second(&self) -> Option<&T> {
        self.get(1) // returns None if there is no index 1
    }
}

fn main() {
    let v = [10, 20, 30];
    println!("{:?}", v.second()); // Some(20)
}
```

Output:

```text
Some(20)
```

> Implementing on the unsized slice `[T]` (rather than `Vec<T>`) means the method works on arrays, `Vec<T>` (via deref coercion), and `&[T]` alike — the most general home for a slice helper.

</details>

### Exercise 2: A `to_title_case` for `str`

**Difficulty:** Intermediate

**Objective:** Add a method to a foreign primitive and exercise the iterator/`String` APIs inside it.

**Instructions:** Define `StrCaseExt` with `to_title_case(&self) -> String`, implement it for `str`, and make it uppercase the first letter of each whitespace-separated word and lowercase the rest. Verify with `"the RUST language"`.

<details>
<summary>Solution</summary>

```rust playground
trait StrCaseExt {
    fn to_title_case(&self) -> String;
}

impl StrCaseExt for str {
    fn to_title_case(&self) -> String {
        self.split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_uppercase().collect::<String>()
                            + &chars.as_str().to_lowercase()
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn main() {
    println!("{}", "the RUST language".to_title_case());
}
```

Output:

```text
The Rust Language
```

> `chars.next()` peels the first character (which may uppercase to *more* than one `char`, hence `to_uppercase().collect::<String>()`), and `chars.as_str()` gives the cheap remainder of the word to lowercase. Implementing on `str` lets the method run on every string literal and `String`.

</details>

### Exercise 3: A lazy `every_other` iterator adapter

**Difficulty:** Advanced

**Objective:** Build a *lazy* extension method that returns a custom iterator, the way `map`/`filter` do.

**Instructions:** Define an adapter struct `EveryOther<I>` that yields the 1st, 3rd, 5th, ... item of an iterator, implement `Iterator` for it, then add an extension trait `IterEveryOtherExt` with `every_other(self) -> EveryOther<Self>`. Collect `(1..=8).every_other()` into a `Vec`.

<details>
<summary>Solution</summary>

```rust playground
// The lazy adapter: keeps every other item, starting with the first.
struct EveryOther<I> {
    iter: I,
    take_it: bool,
}

impl<I: Iterator> Iterator for EveryOther<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        loop {
            let item = self.iter.next()?;
            let keep = self.take_it;
            self.take_it = !self.take_it; // flip for next time
            if keep {
                return Some(item);
            }
        }
    }
}

// The extension trait that introduces `.every_other()` on every iterator.
trait IterEveryOtherExt: Iterator + Sized {
    fn every_other(self) -> EveryOther<Self> {
        EveryOther { iter: self, take_it: true }
    }
}
impl<I: Iterator> IterEveryOtherExt for I {}

fn main() {
    let kept: Vec<i32> = (1..=8).every_other().collect();
    println!("{kept:?}");
}
```

Output:

```text
[1, 3, 5, 7]
```

> Because `next` only pulls from the inner iterator on demand, `EveryOther` is fully lazy and composes with `take`, `map`, and even infinite ranges — exactly like the built-in adapters. The blanket `impl<I: Iterator> IterEveryOtherExt for I {}` makes `.every_other()` available on every iterator the moment the trait is in scope.

</details>
