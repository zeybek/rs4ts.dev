---
title: "Generic Associated Types (GATs)"
description: "GATs let a trait's associated type take a lifetime parameter, enabling lending iterators that borrow from self. The borrow problem TypeScript never has."
---

**Generic associated types** let an `associated type` inside a trait take its own generic parameters, most importantly a *lifetime*. Stabilized in Rust 1.65 (November 2022) after roughly six years of design work, they enable patterns that the trait system simply could not express before, the canonical one being the **lending iterator**: an iterator whose items borrow from the iterator itself.

---

## Quick Overview

A normal associated type is a single fixed type per implementation: `Iterator` has `type Item;`, and once you pick `Item = String`, every call to `next` yields the same `String` type. A **generic associated type** adds parameters to that associated type, so the *concrete* type can depend on, for example, a lifetime: `type Item<'a>;`. Now `next` can hand back something that borrows from `&mut self` and is only valid until the next call.

For a TypeScript/JavaScript developer the closest mental hook is a generic *type member* inside an interface — but the analogy is loose. TypeScript has no lifetimes at all, so the entire problem GATs solve (returning a borrow tied to `self`) does not exist in TS; the language sidesteps it by garbage-collecting and copying. The value of GATs is precisely in the place TypeScript has nothing to say: expressing, in the type system, that a returned reference may not outlive the call that produced it.

---

## TypeScript/JavaScript Example

Consider an iterator that yields overlapping **windows** of an array. In TypeScript you would reach for a generator, and each window is a brand-new array produced by `.slice()`:

```typescript
// A reusable "windows" iterator over an array that yields views.
// In JS/TS there is no borrowing: every yielded slice is a fresh array (a copy).
function* windows<T>(arr: readonly T[], size: number): Generator<T[]> {
  for (let i = 0; i + size <= arr.length; i++) {
    yield arr.slice(i, i + size); // .slice() ALLOCATES a new array each time
  }
}

const data = [1, 2, 3, 4, 5];
for (const w of windows(data, 3)) {
  const sum = w.reduce((a, b) => a + b, 0);
  console.log(`window ${JSON.stringify(w)} sum = ${sum}`);
}
```

Running it with Node v22 (`node --experimental-strip-types`):

```text
window [1,2,3] sum = 6
window [2,3,4] sum = 9
window [3,4,5] sum = 12
```

This works fine, but notice what it costs and what it hides:

- **It allocates.** Every `yield arr.slice(...)` builds a new heap array. For three windows over five elements that is three throwaway arrays.
- **There is no concept of "valid only until the next step".** Each yielded array is an independent, garbage-collected object that lives as long as anything references it. The runtime never tracks that a window is a *view* into shared data, because in JavaScript it is not; it is a copy.

In Rust we want the opposite: each window should be a **borrowed slice** into the underlying data, with **zero allocation**, and the type system should enforce that you do not keep one window alive while advancing to the next. Expressing "the item borrows from the iterator" is exactly what requires a GAT.

---

## Rust Equivalent

Here is the **lending iterator** trait and a `Windows` implementation. The associated type `Item<'a>` carries a lifetime; that is the generic associated type:

```rust playground
// A "lending" iterator: each item may borrow from the iterator itself.
// The associated type `Item<'a>` is generic over a lifetime — that is the GAT.
trait LendingIterator {
    type Item<'a>
    where
        Self: 'a;

    fn next(&mut self) -> Option<Self::Item<'_>>;
}

// Yields overlapping windows of a slice, reusing one internal cursor.
struct Windows<'data, T> {
    slice: &'data [T],
    size: usize,
    pos: usize,
}

impl<'data, T> Windows<'data, T> {
    fn new(slice: &'data [T], size: usize) -> Self {
        Windows { slice, size, pos: 0 }
    }
}

impl<'data, T> LendingIterator for Windows<'data, T> {
    // Each yielded window borrows from `self` for the duration `'a`.
    type Item<'a>
        = &'a [T]
    where
        Self: 'a;

    fn next(&mut self) -> Option<Self::Item<'_>> {
        if self.pos + self.size > self.slice.len() {
            return None;
        }
        let window = &self.slice[self.pos..self.pos + self.size];
        self.pos += 1;
        Some(window)
    }
}

fn main() {
    let data = [1, 2, 3, 4, 5];
    let mut windows = Windows::new(&data, 3);

    // We must use a `while let` loop, not `for`: each borrow ends before the
    // next `next()` call, which is exactly the constraint a GAT encodes.
    while let Some(window) = windows.next() {
        let sum: i32 = window.iter().sum();
        println!("window {window:?} sum = {sum}");
    }
}
```

Real output:

```text
window [1, 2, 3] sum = 6
window [2, 3, 4] sum = 9
window [3, 4, 5] sum = 12
```

Same results as TypeScript, but every window is a `&[i32]` pointing straight into `data`: no allocation, no copy. The `'a` on `Item<'a>` is what lets `next` return a reference whose lifetime is tied to the `&mut self` borrow, and the borrow checker enforces that you finish with one window before asking for the next.

---

## Detailed Explanation

### The signature `fn next(&mut self) -> Option<Self::Item<'_>>`

The `'_` here is the **elided lifetime of the `&mut self` borrow**. Written in full it is:

```rust
// The fully-spelled-out signature; '_ above is shorthand for this 'a.
fn next<'a>(&'a mut self) -> Option<Self::Item<'a>> { /* ... */ }
```

So each call says: "I am borrowing `self` for `'a`, and the item I return is valid for exactly that `'a`." When the returned window goes out of scope, the borrow of `self` ends, and only then can you call `next` again. This is the entire trick. The item's lifetime is *plumbed through* the associated type, which is impossible unless the associated type itself can take a lifetime parameter.

### Why the standard `Iterator` cannot do this

The standard library's `Iterator` looks like this (simplified):

```rust
trait Iterator {
    type Item;                                  // NO lifetime parameter
    fn next(&mut self) -> Option<Self::Item>;
}
```

`Item` is a single fixed type with **no** way to mention the lifetime of the `&mut self` borrow. So if you try to make `Item` a borrow that points into `self`, there is nowhere to attach the lifetime, and the compiler rejects it. This is the historical reason a "streaming"/lending iterator was impossible before GATs, and the compiler's own diagnostic now says so out loud:

```rust
// does not compile: std Iterator's Item cannot borrow from self.
struct WindowsStd<'data, T> {
    slice: &'data [T],
    size: usize,
    pos: usize,
}

impl<'data, T> Iterator for WindowsStd<'data, T> {
    type Item = &[T]; // error: missing lifetime — Item can't borrow from self
    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}
```

The real error:

```text
error: associated type `Iterator::Item` is declared without lifetime parameters, so using a borrowed type for them requires that lifetime to come from the implemented type
  --> src/main.rs:11:17
   |
11 |     type Item = &[T]; // missing lifetime; std Item can't borrow from self
   |                 ^
   |
note: you can't create an `Iterator` that borrows each `Item` from itself, but you can instead create a new type that borrows your existing type and implement `Iterator` for that new type
```

The diagnostic literally explains the limitation: *"you can't create an `Iterator` that borrows each `Item` from itself."* GATs are the feature that lifts this restriction, by moving the lifetime onto the associated type.

> **Note:** `std::slice::windows` *does* exist and yields `&[T]`, but it is **not** a lending iterator. Each `&[T]` it returns borrows from the original slice, not from the iterator's internal state, so all the windows can coexist. A lending iterator is the harder case where items borrow from `self`, which is why it needs a GAT.

### The mandatory `where Self: 'a` bound

Every GAT in these examples carries `where Self: 'a`:

```rust
type Item<'a>
where
    Self: 'a;
```

This bound reads: "`Item<'a>` is only meaningful for lifetimes `'a` during which `Self` itself is valid." It prevents you from constructing an `Item<'a>` that outlives the very value it borrows from. The compiler currently *requires* this bound (it is part of how GATs were stabilized soundly), and it tells you exactly what to add if you forget (see [Common Pitfalls](#common-pitfalls)).

### `while let`, not `for`

The loop is `while let Some(window) = windows.next()`, not `for window in windows`. The `for` loop desugars to the standard `Iterator`, which a lending iterator cannot implement (its items borrow from `self`). With a lending iterator you drive it manually, and the `while let` shape is ideal: each `window` lives only inside one loop iteration, so its borrow of `windows` ends before the next `next()` call. Far from a wart, it is the borrow rule made visible.

### Why this was "hard": six years of design

GATs were first proposed in [RFC 1598](https://github.com/rust-lang/rfcs/blob/master/text/1598-generic_associated_types.md) in 2016 and did not stabilize until Rust 1.65 in late 2022. The difficulty was never the surface syntax; it was making the feature *sound* and the type checker *able to reason about it*. Among the hard problems: deciding which `where`-clause bounds must be required (hence the forced `where Self: 'a`), how GATs interact with higher-ranked trait bounds, lifetime variance, and how implied bounds flow through. Several of those interactions are still being smoothed out (the [closure friction](#key-differences) below is one), which is why the stabilization announcement called it a *minimum viable* version.

### GATs over types, not just lifetimes

A GAT can be generic over a **type** too, not only a lifetime. This expresses *type families* — a single trait that maps an input type to an output container type:

```rust playground
use std::rc::Rc;
use std::sync::Arc;

// A GAT can be generic over a TYPE too. Here, a "smart pointer family":
// given a family, `Of<T>` is the concrete pointer type wrapping a T.
trait PointerFamily {
    type Of<T>;
    fn new<T>(value: T) -> Self::Of<T>;
}

struct RcFamily;
struct ArcFamily;

impl PointerFamily for RcFamily {
    type Of<T> = Rc<T>;
    fn new<T>(value: T) -> Rc<T> {
        Rc::new(value)
    }
}

impl PointerFamily for ArcFamily {
    type Of<T> = Arc<T>;
    fn new<T>(value: T) -> Arc<T> {
        Arc::new(value)
    }
}

// Generic over the family: this code does not know whether it is single- or
// multi-threaded refcounting; it just asks the family for the pointer type.
fn boxed_pair<F: PointerFamily>(a: i32, b: i32) -> (F::Of<i32>, F::Of<i32>) {
    (F::new(a), F::new(b))
}

fn main() {
    let (x, y) = boxed_pair::<RcFamily>(1, 2);
    println!("Rc:  {x} {y}");
    let (p, q) = boxed_pair::<ArcFamily>(3, 4);
    println!("Arc: {p} {q}");
}
```

Real output:

```text
Rc:  1 2
Arc: 3 4
```

`boxed_pair` is written once and is generic over the *pointer family*: pass `RcFamily` for single-threaded reference counting ([`Rc`](/10-smart-pointers/01-rc-arc/)) or `ArcFamily` for the atomic, thread-safe variant — without the function ever naming `Rc` or `Arc`. This "higher-kinded-types-lite" pattern is the second major thing GATs enable.

---

## Key Differences

| Aspect | TypeScript / JavaScript | Rust GATs |
| --- | --- | --- |
| Concept exists? | No lifetimes, so "item borrows from iterator" is a non-problem | The core problem GATs solve |
| How "windows" are yielded | Fresh `.slice()` copy each step (allocates, GC-managed) | Borrowed `&[T]` view into the data (zero allocation) |
| Two items alive at once | Always allowed (independent copies) | Rejected by the borrow checker for a lending iterator |
| Iteration syntax | `for (const x of it)` | `while let Some(x) = it.next()` (no `for`) |
| Associated type members | An interface can declare a generic member type | An associated type can take lifetime *and* type parameters |
| Runtime cost | Per-item allocation | None; items are references |

### The standard `Iterator` did not gain GATs

A reasonable assumption is that `Iterator` was *rewritten* to use a GAT once they stabilized. It was not; doing so would break the entire ecosystem and the `for`-loop desugaring. The lending iterator lives as a separate concept (the `lending-iterator` crate on crates.io packages it for reuse). GATs are additive: they let *new* traits express borrowing items; they did not retrofit the old one.

### A still-rough edge: GATs and closures

GATs are stable and sound, but a few ergonomic interactions remain unsolved. The sharpest one a TS/JS developer is likely to hit: writing a generic combinator that passes each borrowed item to a **closure** often fails to type-check when the iterator borrows from a *local* (non-`'static`) value. The compiler over-constrains the closure's input lifetime to `'static`:

```rust
// A generic `for_each` over a lending iterator, taking a closure.
fn for_each<L, F>(mut iter: L, mut f: F)
where
    L: LendingIterator,
    F: FnMut(L::Item<'_>),   // <- the trouble spot
{
    while let Some(item) = iter.next() {
        f(item);
    }
}
```

Used with an iterator that borrows a local array, the real error is:

```text
error[E0597]: `data` does not live long enough
  --> src/main.rs:45:36
   |
44 |     let data = [1, 2, 3, 4];
   |         ---- binding `data` declared here
45 |     let windows = Windows { slice: &data, size: 2, pos: 0 };
   |                                    ^^^^^ borrowed value does not live long enough
...
   = argument requires that `data` is borrowed for `'static`
   |
note: due to current limitations in the borrow checker, this implies a `'static` lifetime
```

The phrase *"due to current limitations in the borrow checker"* is the compiler admitting this is a known incompleteness, not your mistake. The practical takeaways: prefer a plain `while let` loop at the call site, or keep combinators as **default methods on the trait** (where `Self::Item<'_>` resolves against the concrete `Self`) and have them *consume* items rather than route them through a returning closure. The [Best Practices](#best-practices) section shows the patterns that do work.

---

## Common Pitfalls

### Pitfall 1: Forgetting `where Self: 'a`

The most common first error. Declare the GAT without its required bound:

```rust
trait LendingIterator {
    type Item<'a>; // does not compile: missing `where Self: 'a`
    fn next(&mut self) -> Option<Self::Item<'_>>;
}
```

The real compiler error tells you precisely what to add:

```text
error: missing required bound on `Item`
 --> src/main.rs:2:5
  |
2 |     type Item<'a>; // missing `where Self: 'a`
  |     ^^^^^^^^^^^^^-
  |                  |
  |                  help: add the required where clause: `where Self: 'a`
  |
  = note: this bound is currently required to ensure that impls have maximum flexibility
  = note: we are soliciting feedback, see issue #87479 <https://github.com/rust-lang/rust/issues/87479> for more information
```

The fix is a literal copy-paste of the `help:` line. The bound guarantees an `Item<'a>` can never outlive the `Self` it borrows from.

### Pitfall 2: Trying to hold two lent items at once

Because each item borrows `&mut self`, you cannot keep one alive while taking the next, and that surprises developers used to JavaScript's independent array copies:

```rust
fn main() {
    let data = [1, 2, 3, 4, 5];
    let mut windows = Windows { slice: &data, size: 2, pos: 0 };

    let first = windows.next().unwrap();  // borrows windows mutably
    let second = windows.next().unwrap(); // does not compile
    println!("{first:?} {second:?}");
}
```

The real error:

```text
error[E0499]: cannot borrow `windows` as mutable more than once at a time
  --> src/main.rs:31:18
   |
30 |     let first = windows.next().unwrap();  // borrows windows mutably
   |                 ------- first mutable borrow occurs here
31 |     let second = windows.next().unwrap(); // second mutable borrow while `first` is live
   |                  ^^^^^^^ second mutable borrow occurs here
32 |     println!("{first:?} {second:?}");
   |                ----- first borrow later used here
```

This is not a GAT-specific error — it is the ordinary borrow rule from [Section 05: Borrowing](/05-ownership/02-borrowing/). But it is the *defining feature* of a lending iterator: the type signature deliberately makes "collect all the items into a `Vec` and keep them" impossible, because the items are transient views. If you need them all at once, map each to an owned value first.

### Pitfall 3: Expecting `for` to work

```rust
for window in windows { /* ... */ } // does not compile
```

`for` requires `std::iter::Iterator`, which a lending iterator deliberately does not implement (it cannot; see the Detailed Explanation). Use `while let Some(item) = it.next()`. There is no way around this for genuinely lending iterators, and that is by design.

### Pitfall 4: Trying to make a `dyn LendingIterator`

A trait containing a GAT is **not dyn-compatible** (formerly "object-safe"), so you cannot build a trait object out of it:

```rust
// does not compile: a trait with a GAT is not dyn-compatible.
fn take(_it: &dyn LendingIterator) {}
```

The real error:

```text
error[E0038]: the trait `LendingIterator` is not dyn compatible
 --> src/main.rs:7:19
  |
7 | fn take(_it: &dyn LendingIterator) {}
  |                   ^^^^^^^^^^^^^^^ `LendingIterator` is not dyn compatible
  |
note: for a trait to be dyn compatible it needs to allow building a vtable
 --> src/main.rs:2:10
  |
1 | trait LendingIterator {
  |       --------------- this trait is not dyn compatible...
2 |     type Item<'a> where Self: 'a;
  |          ^^^^ ...because it contains the generic associated type `Item`
  = help: consider moving `Item` to another trait
```

A vtable entry would need a single concrete type for `Item`, but a GAT is a *family* of types indexed by a lifetime, so there is nothing to put in the vtable. Use **static dispatch** (generics: `fn take<L: LendingIterator>(it: L)`) instead. For background on the static-vs-dynamic dispatch trade-off, see [Trait Objects](/09-generics-traits/06-trait-objects/).

---

## Best Practices

- **Reach for a GAT only when you genuinely need an associated type to borrow from `self`** (lending/streaming iterators, cursor APIs, parsers that hand back views into a reused buffer) **or to express a type family** (the pointer-family pattern). For everything else, a plain associated type or a generic method is simpler. GATs add real cognitive cost; do not use them to look clever.

- **Always add the `where Self: 'a` bound** on a lifetime-GAT from the start. It is required, and the compiler will demand it anyway.

- **Drive lending iterators with `while let`**, and offer ergonomic helpers as **default trait methods that consume items** rather than free functions taking returning closures (which hit the borrow-checker limitation shown above). A consuming `count` is a clean example:

  ```rust playground
  trait LendingIterator {
      type Item<'a>
      where
          Self: 'a;

      fn next(&mut self) -> Option<Self::Item<'_>>;

      // A default method that consumes the iterator. It never holds two items
      // at once, so it works even though items borrow from `self`.
      fn count(mut self) -> usize
      where
          Self: Sized,
      {
          let mut n = 0;
          while self.next().is_some() {
              n += 1;
          }
          n
      }
  }

  struct Windows<'data, T> {
      slice: &'data [T],
      size: usize,
      pos: usize,
  }

  impl<'data, T> Windows<'data, T> {
      fn new(slice: &'data [T], size: usize) -> Self {
          Windows { slice, size, pos: 0 }
      }
  }

  impl<'data, T> LendingIterator for Windows<'data, T> {
      type Item<'a>
          = &'a [T]
      where
          Self: 'a;

      fn next(&mut self) -> Option<Self::Item<'_>> {
          if self.pos + self.size > self.slice.len() {
              return None;
          }
          let window = &self.slice[self.pos..self.pos + self.size];
          self.pos += 1;
          Some(window)
      }
  }

  fn main() {
      let data = [1, 2, 3, 4, 5];
      println!("count = {}", Windows::new(&data, 2).count()); // 4
  }
  ```

  Real output:

  ```text
  count = 4
  ```

- **Prefer the `lending-iterator` crate over hand-rolling** when you want the full set of adapters (`map`, `filter`, etc.) on streaming iterators. It works through the GAT friction so you do not have to:

  ```toml
  # cargo add lending-iterator
  [dependencies]
  lending-iterator = "0.1.7"
  ```

- **Use static dispatch (generics), not `dyn`,** for GAT traits, since they are not dyn-compatible. If you truly need dynamic dispatch, factor the GAT into a separate, non-`dyn` trait as the compiler's `help:` suggests, or erase to owned values at the boundary.

- **Document the "valid until next call" contract** on your `next`-style methods. The lifetime enforces it, but a one-line doc comment spares readers from puzzling over why they cannot stash an item.

---

## Real-World Example

A **zero-copy log scanner**. We read newline-delimited records out of one in-memory buffer and hand each line back as a borrowed `&str` view, no per-line `String` allocation, unlike `buffer.split('\n').collect::<Vec<_>>()` or the JavaScript version that copies every slice. This is the production-grade payoff of lending iterators: high-throughput parsing with zero allocation per item.

```rust playground
// A zero-copy line reader over an in-memory buffer. Each call to `next`
// lends a `&str` view into the buffer; no per-line String is allocated.
trait LendingIterator {
    type Item<'a>
    where
        Self: 'a;

    fn next(&mut self) -> Option<Self::Item<'_>>;
}

struct Lines<'buf> {
    rest: &'buf str,
}

impl<'buf> Lines<'buf> {
    fn new(buf: &'buf str) -> Self {
        Lines { rest: buf }
    }
}

impl<'buf> LendingIterator for Lines<'buf> {
    type Item<'a>
        = &'a str
    where
        Self: 'a;

    fn next(&mut self) -> Option<Self::Item<'_>> {
        if self.rest.is_empty() {
            return None;
        }
        match self.rest.find('\n') {
            Some(idx) => {
                let line = &self.rest[..idx];
                self.rest = &self.rest[idx + 1..];
                Some(line)
            }
            None => {
                let line = self.rest;
                self.rest = "";
                Some(line)
            }
        }
    }
}

fn main() {
    let log = "GET /  200\nPOST /login  401\nGET /health  200";

    // Count 2xx responses without allocating a String for any line.
    let mut count_ok = 0;
    let mut lines = Lines::new(log);
    while let Some(line) = lines.next() {
        if line.ends_with("200") {
            count_ok += 1;
        }
        println!("line: {line:?}");
    }
    println!("2xx responses: {count_ok}");
}
```

Real output:

```text
line: "GET /  200"
line: "POST /login  401"
line: "GET /health  200"
2xx responses: 2
```

Each `line` is a `&str` slice pointing into `log`; the iterator owns nothing but a cursor (`rest`). In a real service you would back the buffer with `&mut [u8]` refilled from a socket and lend `&[u8]` frames, processing gigabytes with a single reusable buffer, the kind of allocation discipline that matters in the contexts of [Section 21: Performance](/21-performance/) and [Section 26: Systems Programming](/26-systems-programming/).

> **Tip:** When a borrowed `&str` is *almost always* a view but *occasionally* needs to be owned (say you must mutate or store it past the borrow), `Cow<str>` is the idiomatic bridge — see [Clone-on-Write (Cow)](/10-smart-pointers/04-cow/).

---

## Further Reading

- [The Rust Reference: Generic associated types](https://doc.rust-lang.org/reference/items/associated-items.html#associated-types) — the precise rules, including the required `where` bounds.
- [Stabilizing GATs (1.65 announcement)](https://blog.rust-lang.org/2022/10/28/gats-stabilization.html): the official write-up of what shipped, why it took years, and the remaining limitations.
- [RFC 1598: Generic associated types](https://github.com/rust-lang/rfcs/blob/master/text/1598-generic_associated_types.md): the original 2016 design.
- [`lending-iterator` on docs.rs](https://docs.rs/lending-iterator) — a production crate that packages lending iterators with adapters.

Cross-links within this guide:

- [Traits](/09-generics-traits/03-traits/) and [Generic Functions](/09-generics-traits/00-generic-functions/) — the associated-type and monomorphization machinery GATs extend.
- [Trait Objects](/09-generics-traits/06-trait-objects/) — why a GAT trait must use static dispatch, not `dyn`.
- [Lifetimes](/05-ownership/04-lifetimes/) and [Borrowing](/05-ownership/02-borrowing/) — the borrow rules the lending-iterator pattern makes visible.
- [Const Generics](/25-advanced-topics/05-const-generics/) — a sibling type-system feature, generic over *values* instead of types/lifetimes.
- [Specialization](/25-advanced-topics/07-specialization/): a neighboring trait feature that, unlike GATs, is still nightly-only.
- [PhantomData & zero-sized types](/25-advanced-topics/00-phantom-data/) — another type-level tool, for encoding ownership and variance without storing data.
- [Section 02: Basics](/02-basics/) and [Section 01: Getting Started](/01-getting-started/) — foundational material if any of the syntax here is unfamiliar.

---

## Exercises

### Exercise 1: A mutable lending iterator

**Difficulty:** Beginner

**Objective:** Implement a lending iterator that yields **mutable** borrows, so the caller can edit elements in place.

**Instructions:** Using the `LendingIterator` trait from this chapter, implement `IterMut<'data, T>` over a `&'data mut [T]` whose `Item<'a>` is `&'a mut T`. Each `next` should yield the next element by mutable reference. Drive it with a `while let` loop to add `1` to every element of `[10, 20, 30]`, then print the array (expect `[11, 21, 31]`).

<details>
<summary>Solution</summary>

```rust playground
trait LendingIterator {
    type Item<'a>
    where
        Self: 'a;
    fn next(&mut self) -> Option<Self::Item<'_>>;
}

struct IterMut<'data, T> {
    slice: &'data mut [T],
    pos: usize,
}

impl<'data, T> LendingIterator for IterMut<'data, T> {
    type Item<'a>
        = &'a mut T
    where
        Self: 'a;

    fn next(&mut self) -> Option<Self::Item<'_>> {
        let item = self.slice.get_mut(self.pos);
        if item.is_some() {
            self.pos += 1;
        }
        item
    }
}

fn main() {
    let mut data = [10, 20, 30];
    let mut it = IterMut { slice: &mut data, pos: 0 };
    while let Some(x) = it.next() {
        *x += 1;
    }
    println!("{data:?}"); // [11, 21, 31]
}
```

</details>

### Exercise 2: A consuming `count` default method

**Difficulty:** Intermediate

**Objective:** Add a generic default method to the trait and exercise it on a chunking iterator.

**Instructions:** Add a `count(self) -> usize` default method to `LendingIterator` (consuming `self`, requiring `Self: Sized`). Implement `Chunks<'data, T>` over a `&'data [T]` whose `Item<'a>` is `&'a [T]`, yielding non-overlapping chunks of a given size (use `slice::split_at`). Verify that chunking `[1, 2, 3, 4, 5, 6, 7]` by 3 yields a count of `3`.

<details>
<summary>Solution</summary>

```rust playground
trait LendingIterator {
    type Item<'a>
    where
        Self: 'a;
    fn next(&mut self) -> Option<Self::Item<'_>>;

    fn count(mut self) -> usize
    where
        Self: Sized,
    {
        let mut n = 0;
        while self.next().is_some() {
            n += 1;
        }
        n
    }
}

struct Chunks<'data, T> {
    slice: &'data [T],
    size: usize,
}

impl<'data, T> LendingIterator for Chunks<'data, T> {
    type Item<'a>
        = &'a [T]
    where
        Self: 'a;

    fn next(&mut self) -> Option<Self::Item<'_>> {
        if self.slice.is_empty() {
            return None;
        }
        let take = self.size.min(self.slice.len());
        let (head, tail) = self.slice.split_at(take);
        self.slice = tail;
        Some(head)
    }
}

fn main() {
    let data = [1, 2, 3, 4, 5, 6, 7];
    let chunks = Chunks { slice: &data, size: 3 };
    println!("chunks = {}", chunks.count()); // 3
}
```

</details>

### Exercise 3: A pointer-family GAT

**Difficulty:** Intermediate

**Objective:** Use a GAT generic over a **type** to abstract over single- vs multi-threaded reference counting.

**Instructions:** Define a `PointerFamily` trait with `type Of<T>;` and `fn new<T>(value: T) -> Self::Of<T>;`. Implement it for `RcFamily` (`Of<T> = Rc<T>`) and `ArcFamily` (`Of<T> = Arc<T>`). Then write a generic `fn wrap<F: PointerFamily>(value: &str) -> F::Of<String>` that wraps an owned `String`. Call it with both families and print the results (e.g. `Rc("hi")` and `Arc("bye")`).

<details>
<summary>Solution</summary>

```rust playground
use std::rc::Rc;
use std::sync::Arc;

trait PointerFamily {
    type Of<T>;
    fn new<T>(value: T) -> Self::Of<T>;
}

struct RcFamily;
struct ArcFamily;

impl PointerFamily for RcFamily {
    type Of<T> = Rc<T>;
    fn new<T>(value: T) -> Rc<T> {
        Rc::new(value)
    }
}

impl PointerFamily for ArcFamily {
    type Of<T> = Arc<T>;
    fn new<T>(value: T) -> Arc<T> {
        Arc::new(value)
    }
}

fn wrap<F: PointerFamily>(value: &str) -> F::Of<String> {
    F::new(value.to_string())
}

fn main() {
    let single = wrap::<RcFamily>("hi");
    let shared = wrap::<ArcFamily>("bye");
    println!("Rc  -> {single}");
    println!("Arc -> {shared}");
}
```

Real output:

```text
Rc  -> hi
Arc -> bye
```

</details>
