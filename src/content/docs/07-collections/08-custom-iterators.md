---
title: "Custom Iterators: Implementing `Iterator` and `IntoIterator`"
description: "Build custom sequences in Rust: implement Iterator's next method and IntoIterator instead of a JavaScript generator, then inherit map, filter, zip, and every"
---

In TypeScript you make a value iterable by writing a **generator** (`function*`) or by implementing the `[Symbol.iterator]()` protocol. In Rust you do the equivalent by implementing the **`Iterator`** trait (define one method, `next`) and, when you want `for` loops to work directly on your type, the **`IntoIterator`** trait. This page shows how to build your own lazy data producers and plug them into Rust's entire adaptor toolbox for free.

---

## Quick Overview

Rust's iteration is built on two **traits**. **`Iterator`** has a single required method, `next(&mut self) -> Option<Self::Item>`, that yields values one at a time and signals "done" with `None`, exactly like a JavaScript generator yielding values and finally `{ done: true }`. **`IntoIterator`** is what `for x in thing` actually calls; implementing it makes your own collection loopable. The huge payoff: once you implement `next`, you automatically get `map`, `filter`, `take`, `zip`, `sum`, `collect`, and every other adaptor. You write five lines and inherit a hundred methods.

---

## TypeScript/JavaScript Example

In TypeScript, the idiomatic way to produce a custom sequence is a generator function. To make a *class* iterable, you implement the `Symbol.iterator` protocol.

```typescript
// A generator that produces Fibonacci numbers lazily, forever.
function* fibonacci(): Generator<number> {
  let [a, b] = [0, 1];
  while (true) {
    yield a;
    [a, b] = [b, a + b];
  }
}

// Generators are lazy: nothing runs until you pull values out.
const firstTen: number[] = [];
for (const n of fibonacci()) {
  if (firstTen.length === 10) break; // we must stop it ourselves
  firstTen.push(n);
}
console.log(firstTen); // [0, 1, 1, 2, 3, 5, 8, 13, 21, 34]

// Making a class iterable via the Symbol.iterator protocol.
class Playlist {
  constructor(private tracks: string[]) {}

  *[Symbol.iterator](): Generator<string> {
    for (const track of this.tracks) {
      yield track;
    }
  }
}

const playlist = new Playlist(["Intro", "Verse", "Chorus"]);
for (const track of playlist) {
  console.log(track);
}
const upper = [...playlist].map((t) => t.toUpperCase());
console.log(upper); // [ 'INTRO', 'VERSE', 'CHORUS' ]
```

Two things to carry into Rust: the generator is **lazy** (no values are computed until consumed), and the `for...of` loop secretly calls the object's `[Symbol.iterator]()` method to obtain an iterator.

---

## Rust Equivalent

Rust splits the same job across `Iterator` (the thing that produces values) and `IntoIterator` (the thing that *hands you* a producer). First, the Fibonacci producer:

```rust
struct Fibonacci {
    a: u64,
    b: u64,
}

impl Iterator for Fibonacci {
    type Item = u64; // what each `next()` yields

    fn next(&mut self) -> Option<u64> {
        let current = self.a;
        self.a = self.b;
        self.b = current + self.b;
        Some(current) // never None -> this is an INFINITE iterator
    }
}

fn fib() -> Fibonacci {
    Fibonacci { a: 0, b: 1 }
}

fn main() {
    // Lazy, like the JS generator. `take(10)` bounds the infinite stream.
    let first_ten: Vec<u64> = fib().take(10).collect();
    println!("{first_ten:?}");

    // The whole adaptor library is available because we implemented `next`.
    let sum: u64 = fib().take_while(|&n| n < 100).filter(|n| n % 2 == 0).sum();
    println!("sum of even fibs < 100: {sum}");
}
```

Verified output:

```text
[0, 1, 1, 2, 3, 5, 8, 13, 21, 34]
sum of even fibs < 100: 44
```

Now the `Playlist` equivalent. Implement `IntoIterator` so `for track in playlist` works:

```rust
struct Playlist {
    tracks: Vec<String>,
}

impl IntoIterator for Playlist {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.tracks.into_iter() // delegate to Vec's iterator
    }
}

fn main() {
    let playlist = Playlist {
        tracks: vec!["Intro".into(), "Verse".into(), "Chorus".into()],
    };
    for track in playlist {
        println!("{track}");
    }
}
```

Verified output:

```text
Intro
Verse
Chorus
```

---

## Detailed Explanation

### The `Iterator` trait is just one method

Here is the trait, reduced to its essence (the standard library defines dozens of additional methods, but they all have default implementations built on top of `next`):

```rust
// From the standard library (simplified).
pub trait Iterator {
    type Item;                                  // associated type: what you yield
    fn next(&mut self) -> Option<Self::Item>;   // the ONE method you must write
    // ...70+ provided methods: map, filter, take, sum, collect, ...
}
```

- **`type Item`** is an *associated type*: the type of each yielded value. It is the rough analogue of the `T` in TypeScript's `Generator<T>`. (Associated types are covered in [Section 06 — Associated Types](/06-data-structures/07-associated-types/).)
- **`next(&mut self)`** takes `&mut self` because pulling a value mutates the iterator's internal state (advancing the cursor). It returns `Option<Self::Item>`: `Some(value)` while values remain, `None` once exhausted. That `None` is Rust's `{ done: true }`.

Because every adaptor is built on `next`, implementing that single method gives your type `map`, `filter`, `enumerate`, `zip`, `fold`, `collect`, and the rest, for free. This is the central reason custom iterators are worth the small boilerplate.

### A minimal example, step by step

```rust
struct Countdown {
    current: u32,
}

impl Iterator for Countdown {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == 0 {
            None // exhausted: signal "done"
        } else {
            self.current -= 1; // mutate state for next time
            Some(self.current + 1) // yield the pre-decrement value
        }
    }
}

fn main() {
    let cd = Countdown { current: 3 };
    for n in cd {
        print!("{n} ");
    }
    println!();

    let cd2 = Countdown { current: 5 };
    let doubled: Vec<u32> = cd2.map(|n| n * 2).collect();
    println!("{doubled:?}");
}
```

Verified output:

```text
3 2 1 
[10, 8, 6, 4, 2]
```

The loop calls `next()` repeatedly until it returns `None`. The `cd2.map(...).collect()` line proves the payoff: we never wrote `map` or `collect`, yet they work because the trait provides them on top of our `next`.

### `IntoIterator`: what `for` actually calls

A `for` loop in Rust is **syntactic sugar**. This:

```rust
fn main() {
    let v = vec![1, 2, 3];

    // This loop...
    for x in &v {
        print!("{x} ");
    }
    println!();

    // ...is exactly this, written out by hand:
    let mut it = IntoIterator::into_iter(&v);
    while let Some(x) = it.next() {
        print!("{x} ");
    }
    println!();
}
```

Verified output:

```text
1 2 3 
1 2 3
```

`for thing in expr` calls `IntoIterator::into_iter(expr)` to get an iterator, then loops on `next()`. This is the direct counterpart of JavaScript's `for...of` calling `expr[Symbol.iterator]()`. `Iterator` and `IntoIterator` are distinct traits, but every `Iterator` also implements `IntoIterator` (its `into_iter` just returns itself), which is why you can write `for x in some_iterator` as well as `for x in some_collection`.

### Implementing `IntoIterator` three ways (by value, `&`, `&mut`)

Standard collections let you write `for x in v`, `for x in &v`, and `for x in &mut v`. You get the same flexibility for your own type by implementing `IntoIterator` once per ownership flavor. The trick is that you implement it for `Grid`, `&Grid`, and `&mut Grid` separately:

```rust
struct Grid {
    cells: Vec<i32>,
}

// by value (consuming): `for c in grid`
impl IntoIterator for Grid {
    type Item = i32;
    type IntoIter = std::vec::IntoIter<i32>;
    fn into_iter(self) -> Self::IntoIter {
        self.cells.into_iter()
    }
}

// by shared reference: `for c in &grid`
impl<'a> IntoIterator for &'a Grid {
    type Item = &'a i32;
    type IntoIter = std::slice::Iter<'a, i32>;
    fn into_iter(self) -> Self::IntoIter {
        self.cells.iter()
    }
}

// by mutable reference: `for c in &mut grid`
impl<'a> IntoIterator for &'a mut Grid {
    type Item = &'a mut i32;
    type IntoIter = std::slice::IterMut<'a, i32>;
    fn into_iter(self) -> Self::IntoIter {
        self.cells.iter_mut()
    }
}

fn main() {
    let mut g = Grid { cells: vec![1, 2, 3] };

    // &mut Grid -> mutate in place
    for c in &mut g {
        *c *= 10;
    }

    // &Grid -> read-only
    for c in &g {
        print!("{c} ");
    }
    println!();

    // Grid (by value) -> consume
    let total: i32 = g.into_iter().sum();
    println!("total = {total}");
}
```

Verified output:

```text
10 20 30 
total = 60
```

> **Tip:** By convention, types that implement `IntoIterator for &T` also offer an inherent `fn iter(&self)` method (and `iter_mut` for `&mut`). That is why you write `v.iter()` on a `Vec`: it is shorthand for `(&v).into_iter()`. Provide an `iter()` method on your own type for the same ergonomic reason. The three borrow flavors map onto the same `iter()`/`iter_mut()`/`into_iter()` distinction you saw for `Vec` in [Vectors](/07-collections/00-vectors/#iteration-three-flavors-of-borrow).

### A borrowing iterator that yields references

The Fibonacci and `Countdown` examples yield *owned* values, so the iterator owns all its state. When you want to iterate over data that lives elsewhere and yield **references** into it, you create a separate iterator struct that *borrows* the source. This is the pattern the standard library uses for `slice::Iter`, and it requires a lifetime parameter:

```rust
struct Ring {
    data: Vec<char>,
}

struct RingIter<'a> {
    ring: &'a Ring, // borrows the Ring for lifetime 'a
    pos: usize,
    yielded: usize,
}

impl Ring {
    fn iter_from(&self, start: usize) -> RingIter<'_> {
        RingIter { ring: self, pos: start, yielded: 0 }
    }
}

impl<'a> Iterator for RingIter<'a> {
    type Item = &'a char; // we yield references, not owned chars

    fn next(&mut self) -> Option<&'a char> {
        if self.yielded == self.ring.data.len() {
            return None;
        }
        let item = &self.ring.data[self.pos % self.ring.data.len()];
        self.pos += 1;
        self.yielded += 1;
        Some(item)
    }

    // Optional but recommended: lets `collect` pre-allocate exactly.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.ring.data.len() - self.yielded;
        (remaining, Some(remaining))
    }
}

fn main() {
    let r = Ring { data: vec!['a', 'b', 'c', 'd'] };
    let it = r.iter_from(2);
    println!("size_hint: {:?}", it.size_hint());

    let collected: String = r.iter_from(2).collect();
    println!("{collected}");

    // `r` is still usable: the iterator only borrowed it.
    println!("ring still has {} cells", r.data.len());
}
```

Verified output:

```text
size_hint: (4, Some(4))
cdab
ring still has 4 cells
```

The lifetime `'a` ties each yielded `&char` to the `Ring` it came from, so the borrow checker guarantees the data outlives the iterator. JavaScript has no such concept: a generator can close over and yield anything, and the garbage collector keeps it alive. (Lifetimes are introduced in [Section 05 — Ownership](/05-ownership/).)

### `size_hint`: a free performance hint

`size_hint` returns `(lower_bound, Option<upper_bound>)`. Consumers like `collect` use it to pre-allocate the right capacity. It is optional — the default returns `(0, None)` — but cheap to provide when you know the count, and it makes `collect`ing into a `Vec` allocate exactly once. It must never *over*-report the lower bound; under-reporting is always safe.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Define a producer | `function*` generator with `yield` | `impl Iterator` with `fn next` |
| Make a type loopable | `[Symbol.iterator]()` method | `impl IntoIterator` |
| "Done" signal | `{ value, done: true }` | `next()` returns `None` |
| Element type | `Generator<T>` (type param) | `type Item = T` (associated type) |
| Pausing/resuming | Compiler rewrites to a state machine implicitly | You hold state in struct fields explicitly |
| Built-in operations | A few (`.map` only on arrays, not generators) | 70+ adaptors, all from one `next` |
| Yielding references | Not a distinct concept (GC keeps things alive) | Lifetime-checked `Item = &'a T` |
| Infinite sequences | `while (true) { yield }` | `next` that never returns `None` |

### Generators are state machines you don't have to write

The biggest ergonomic gap: a JavaScript generator *pauses* at each `yield` and the engine remembers where to resume. Rust has no stable generator syntax for this; instead, **you** turn the logic into an explicit state machine by storing progress in struct fields and computing the next value from them in `next`. The Fibonacci example stores `a` and `b`; the `Countdown` stores `current`. What `yield` did implicitly, you do explicitly. The upside is that the state is plain data you can inspect, clone, and reason about.

### One `next`, the whole library

In TypeScript, generators do *not* have `.map`/`.filter`; those live only on `Array`, so you typically spread (`[...gen()]`) into an array first, losing laziness. In Rust, implementing `next` automatically gives your type every lazy adaptor. `fib().take_while(...).filter(...).sum()` never builds an intermediate array; it pulls values one at a time. See [Iterators](/07-collections/06-iterators/) for the adaptor catalog and [Iterator Consumers](/07-collections/07-iterator-consumers/) for the terminal operations.

---

## Common Pitfalls

### Pitfall 1: Forgetting `type Item`

`Iterator` requires you to specify the associated type. Omitting it is a hard error, not an inference fallback.

```rust
struct Counter {
    n: u32,
}

impl Iterator for Counter {
    // type Item = u32;  // omitted on purpose
    fn next(&mut self) -> Option<u32> {
        self.n += 1;
        Some(self.n)
    }
}
// does not compile (error[E0046]: missing `Item`)
```

Real compiler error:

```text
error[E0046]: not all trait items implemented, missing: `Item`
 --> src/main.rs:6:1
  |
6 | impl Iterator for Counter {
  | ^^^^^^^^^^^^^^^^^^^^^^^^^ missing `Item` in implementation
  |
  = help: implement the missing item: `type Item = /* Type */;`
```

**Fix:** add `type Item = u32;` as the first line inside the `impl` block.

### Pitfall 2: Wrong `self` receiver on `next`

The signature is `fn next(&mut self)`, not `fn next(self)` or `fn next(&self)`. Pulling a value mutates the cursor, so it must borrow mutably.

```rust
struct Counter {
    n: u32,
}

impl Iterator for Counter {
    type Item = u32;
    fn next(self) -> Option<u32> {
        // wrong receiver
        Some(self.n)
    }
}
// does not compile (error[E0053]: incompatible type for trait)
```

Real compiler error:

```text
error[E0053]: method `next` has an incompatible type for trait
 --> src/main.rs:8:13
  |
8 |     fn next(self) -> Option<u32> {
  |             ^^^^ expected `&mut Counter`, found `Counter`
  |
  = note: expected signature `fn(&mut Counter) -> Option<_>`
             found signature `fn(Counter) -> Option<_>`
help: change the self-receiver type to match the trait
  |
8 |     fn next(&mut self) -> Option<u32> {
  |             ++++
```

**Fix:** write `fn next(&mut self) -> Option<Self::Item>` to match the trait exactly.

### Pitfall 3: An accidental infinite iterator

If `next` never returns `None`, your iterator is infinite, which is fine *if* you always bound it with `take`, `take_while`, `find`, etc. But calling a consuming method like `collect`, `sum`, or `count` on an unbounded iterator loops forever and never terminates.

```rust
// `Counter` here never returns None.
let everything: Vec<u32> = counter.collect(); // hangs forever — no panic, no error
```

This is not a compile error and produces no message: the program simply never finishes (the same trap as `[...fibonacci()]` in JavaScript, which also hangs). **Fix:** bound infinite iterators before consuming them — `counter.take(100).collect()` — or make `next` return `None` at a terminal condition. Treat any custom `next` that never yields `None` as infinite by design and document it.

### Pitfall 4: Reaching for generators that Rust doesn't have (yet)

TypeScript developers often look for a `yield` keyword. Stable Rust has **no generator syntax**: you cannot write `gen { yield x; }` in a normal function on stable as of Rust 1.96.0. You must encode the state by hand in a struct, as shown throughout this page. (An experimental `gen` block exists on nightly, but do not rely on it in production.) For the common case of "I just want to transform an existing collection," you usually do *not* need a custom iterator at all. Chain adaptors on the collection's built-in iterator instead (see [Iterators](/07-collections/06-iterators/)).

---

## Best Practices

### 1. Implement `Iterator` for a dedicated struct, then expose `iter()`

For a collection, the idiomatic shape mirrors the standard library: a separate `XIter` struct implements `Iterator`, and your collection offers `iter(&self) -> XIter<'_>` plus `IntoIterator` impls for the three borrow forms. This gives callers `for x in &c`, `for x in c`, and `c.iter().map(...)` exactly as they expect from `Vec` and `HashMap`.

### 2. Return `impl Iterator<Item = T>` from functions

When a function produces a sequence, return `impl Iterator<Item = T>` rather than a concrete type or a `Vec`. This keeps the result lazy and hides the (often unnameable) adaptor type:

```rust
// Returns a lazy iterator; the caller decides whether to collect, sum, etc.
fn evens_up_to(max: u32) -> impl Iterator<Item = u32> {
    (0..max).filter(|n| n % 2 == 0)
}

fn main() {
    let evens: Vec<u32> = evens_up_to(10).collect();
    println!("{evens:?}"); // [0, 2, 4, 6, 8]
}
```

Verified output:

```text
[0, 2, 4, 6, 8]
```

### 3. Provide `size_hint` when the length is known

A correct `size_hint` lets `collect` pre-allocate, avoiding reallocations. It costs a few lines and is a pure win for finite iterators. Never lie about the lower bound.

### 4. Consider the optional refinement traits

Once `Iterator` is implemented, you can opt into stronger guarantees that enable extra methods:

- **`DoubleEndedIterator`** (add `next_back`) enables `.rev()` and `.next_back()`.
- **`ExactSizeIterator`** (when `size_hint` is exact) enables `.len()`.
- **`FusedIterator`** promises that once `None` is returned, it stays `None`.

Implement them only when the semantics genuinely hold for your type.

### 5. Don't write a custom iterator when an adaptor chain will do

The most common mistake is over-engineering. If you only need to transform or filter an existing collection, chain `map`/`filter`/`flat_map` on its built-in iterator. Reserve a hand-written `impl Iterator` for genuinely new sequence *sources* (generators, parsers, pagination, infinite streams).

---

## Real-World Example

A paginating iterator over a (mocked) HTTP API. Each call to `next()` "fetches" the next page until the server reports no more pages: a stateful, lazy producer that is impossible to express as a simple adaptor chain. Because it is a real `Iterator`, the entire adaptor library applies to the result.

```rust
#[derive(Debug)]
struct User {
    id: u32,
    name: String,
}

/// Stands in for an HTTP client. Returns up to `page_size` users per page,
/// and reports whether more pages remain.
struct ApiClient {
    total_users: u32,
}

impl ApiClient {
    fn fetch_page(&self, page: u32, page_size: u32) -> (Vec<User>, bool) {
        let start = page * page_size;
        let end = (start + page_size).min(self.total_users);
        let users = (start..end)
            .map(|id| User { id, name: format!("user-{id}") })
            .collect();
        let has_more = end < self.total_users;
        (users, has_more)
    }

    /// Hand out a lazy iterator over pages.
    fn pages(&self, page_size: u32) -> Pages<'_> {
        Pages { client: self, page: 0, page_size, done: false }
    }
}

/// An iterator that yields one *page* (a `Vec<User>`) at a time.
struct Pages<'a> {
    client: &'a ApiClient,
    page: u32,
    page_size: u32,
    done: bool,
}

impl<'a> Iterator for Pages<'a> {
    type Item = Vec<User>;

    fn next(&mut self) -> Option<Vec<User>> {
        if self.done {
            return None;
        }
        let (users, has_more) = self.client.fetch_page(self.page, self.page_size);
        self.page += 1;
        if !has_more {
            self.done = true;
        }
        if users.is_empty() {
            return None; // an empty final page also terminates iteration
        }
        Some(users)
    }
}

fn main() {
    let client = ApiClient { total_users: 7 };

    // Flatten pages of users into a single lazy stream of users.
    let labels: Vec<String> = client
        .pages(3)
        .flatten() // Vec<User> -> User
        .map(|u| format!("#{}:{}", u.id, u.name))
        .collect();

    println!("fetched {} users across pages", labels.len());
    println!("{labels:?}");

    // Or process page-by-page, as you would when streaming into a database.
    for (i, page) in client.pages(3).enumerate() {
        println!("page {i}: {} users", page.len());
    }
}
```

Verified output:

```text
fetched 7 users across pages
["#0:user-0", "#1:user-1", "#2:user-2", "#3:user-3", "#4:user-4", "#5:user-5", "#6:user-6"]
page 0: 3 users
page 1: 3 users
page 2: 1 users
```

The win is composability: `pages(3).flatten().map(...).collect()` reads as a clean pipeline, yet under the hood each page is fetched only when the consumer asks for it. Swapping the mock `fetch_page` for a real network call (returning a `Result`) would turn this into a production cursor-pagination helper; you would change `Item` to `Result<Vec<User>, ApiError>` and handle failures with the techniques in [Section 08 — Error Handling](/08-error-handling/).

---

## Further Reading

### Official Documentation

- [`std::iter::Iterator`](https://doc.rust-lang.org/std/iter/trait.Iterator.html): the trait and all its provided methods
- [`std::iter::IntoIterator`](https://doc.rust-lang.org/std/iter/trait.IntoIterator.html): what `for` loops call
- [The Rust Book — Creating Our Own Iterators with the `Iterator` Trait](https://doc.rust-lang.org/book/ch13-02-iterators.html#creating-our-own-iterators-with-the-iterator-trait)
- [`std::iter` module docs](https://doc.rust-lang.org/std/iter/index.html): the mental model for laziness and adaptors
- [`DoubleEndedIterator`](https://doc.rust-lang.org/std/iter/trait.DoubleEndedIterator.html) and [`ExactSizeIterator`](https://doc.rust-lang.org/std/iter/trait.ExactSizeIterator.html): the optional refinement traits

### Related Topics in This Guide

- [Iterators](/07-collections/06-iterators/): the lazy adaptors (`map`, `filter`, `take`, `zip`) your `next` enables
- [Iterator Consumers](/07-collections/07-iterator-consumers/): terminal operations (`collect`, `sum`, `fold`, `find`) that drive `next`
- [Vectors](/07-collections/00-vectors/): the three borrow flavors of iteration (`iter`, `iter_mut`, `into_iter`)
- [HashMaps](/07-collections/03-hashmaps/) and [BTreeMap / BTreeSet](/07-collections/05-btreemap-btreeset/): collections whose iterators you'll often wrap or chain
- [Collection Performance](/07-collections/09-collection-performance/): why `size_hint` and laziness matter for speed
- [Section 06 — Associated Types](/06-data-structures/07-associated-types/): the `type Item` mechanism
- [Section 05 — Ownership](/05-ownership/): lifetimes for reference-yielding iterators
- [Section 02 — Basics: Types](/02-basics/01-types/): `Option`, `usize`, and the integer types used above

---

## Exercises

### Exercise 1: A Stepping Range

**Difficulty:** Beginner

**Objective:** Implement `Iterator` for a struct that produces numbers from a start up to (but not including) an end, advancing by a fixed step.

**Instructions:** Define a `Stepper` struct holding `current`, `step`, and `end` (`i64`). Implement `Iterator` so that `stepper(0, 3, 16).collect::<Vec<_>>()` yields `[0, 3, 6, 9, 12, 15]`. Return `None` once `current >= end`.

```rust
struct Stepper {
    current: i64,
    step: i64,
    end: i64,
}

impl Iterator for Stepper {
    type Item = i64;
    fn next(&mut self) -> Option<i64> {
        /* ??? */
        todo!()
    }
}

fn stepper(start: i64, step: i64, end: i64) -> Stepper {
    Stepper { current: start, step, end }
}

fn main() {
    let v: Vec<i64> = stepper(0, 3, 16).collect();
    println!("{v:?}");
}
```

<details>
<summary>Solution</summary>

```rust
struct Stepper {
    current: i64,
    step: i64,
    end: i64,
}

impl Iterator for Stepper {
    type Item = i64;
    fn next(&mut self) -> Option<i64> {
        if self.current >= self.end {
            return None;
        }
        let value = self.current;
        self.current += self.step;
        Some(value)
    }
}

fn stepper(start: i64, step: i64, end: i64) -> Stepper {
    Stepper { current: start, step, end }
}

fn main() {
    let v: Vec<i64> = stepper(0, 3, 16).collect();
    println!("{v:?}");
}
```

Output:

```text
[0, 3, 6, 9, 12, 15]
```

</details>

### Exercise 2: Make a Wrapper Type Loopable

**Difficulty:** Intermediate

**Objective:** Implement `IntoIterator` for both a value and a shared reference so a `Playlist` works in `for` loops by value and by `&`.

**Instructions:** Given `struct Playlist { tracks: Vec<String> }`, implement `IntoIterator for Playlist` (yielding `String`) and `IntoIterator for &Playlist` (yielding `&String`). Then loop over `&playlist` to print each track, and consume `playlist` to build a `Vec<String>` of uppercased tracks. Delegate to `Vec`'s own iterators.

```rust
struct Playlist {
    tracks: Vec<String>,
}

// TODO: impl IntoIterator for Playlist (by value)
// TODO: impl IntoIterator for &Playlist (by shared reference)

fn main() {
    let pl = Playlist {
        tracks: vec!["a".into(), "b".into(), "c".into()],
    };
    // TODO: loop over &pl and print each track
    // TODO: consume pl into a Vec<String> of uppercased tracks and print it
}
```

<details>
<summary>Solution</summary>

```rust
struct Playlist {
    tracks: Vec<String>,
}

impl IntoIterator for Playlist {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;
    fn into_iter(self) -> Self::IntoIter {
        self.tracks.into_iter()
    }
}

impl<'a> IntoIterator for &'a Playlist {
    type Item = &'a String;
    type IntoIter = std::slice::Iter<'a, String>;
    fn into_iter(self) -> Self::IntoIter {
        self.tracks.iter()
    }
}

fn main() {
    let pl = Playlist {
        tracks: vec!["a".into(), "b".into(), "c".into()],
    };

    for t in &pl {
        print!("{t} ");
    }
    println!();

    let upper: Vec<String> = pl.into_iter().map(|t| t.to_uppercase()).collect();
    println!("{upper:?}");
}
```

Output:

```text
a b c 
["A", "B", "C"]
```

</details>

### Exercise 3: A Double-Ended Iterator

**Difficulty:** Advanced

**Objective:** Implement both `Iterator` and `DoubleEndedIterator` so your type supports `.rev()`.

**Instructions:** Define `struct Span { front: u32, back: u32 }` representing the half-open range `[front, back)`. Implement `Iterator` so `next` yields ascending values from the front, and `DoubleEndedIterator` so `next_back` yields descending values from the back. Verify that `Span { front: 0, back: 5 }.rev().collect::<Vec<_>>()` produces `[4, 3, 2, 1, 0]`. (Hint: `.rev()` is provided automatically once `DoubleEndedIterator` is implemented; it calls `next_back`.)

```rust
struct Span {
    front: u32,
    back: u32,
}

impl Iterator for Span {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        /* ??? */
        todo!()
    }
}

// TODO: impl DoubleEndedIterator for Span

fn main() {
    let s = Span { front: 0, back: 5 };
    let reversed: Vec<u32> = s.rev().collect();
    println!("{reversed:?}");
}
```

<details>
<summary>Solution</summary>

```rust
struct Span {
    front: u32,
    back: u32,
}

impl Iterator for Span {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.front >= self.back {
            return None;
        }
        let v = self.front;
        self.front += 1;
        Some(v)
    }
}

impl DoubleEndedIterator for Span {
    fn next_back(&mut self) -> Option<u32> {
        if self.front >= self.back {
            return None;
        }
        self.back -= 1;
        Some(self.back)
    }
}

fn main() {
    let s = Span { front: 0, back: 5 };
    let reversed: Vec<u32> = s.rev().collect();
    println!("{reversed:?}");

    // Forward still works too.
    let forward: Vec<u32> = Span { front: 0, back: 5 }.collect();
    println!("{forward:?}");
}
```

Output:

```text
[4, 3, 2, 1, 0]
[0, 1, 2, 3, 4]
```

</details>
