---
title: "Generic Functions"
description: "Write one Rust function for many types, like a TypeScript generic, but monomorphized into specialized, zero-cost code at compile time, with trait bounds on T."
---

Generic functions let you write one function that works for many types, without giving up type safety. If you have ever written `function first<T>(arr: T[]): T` in TypeScript, you already know the shape of the idea. But Rust compiles generics in a fundamentally different way, and that difference has real consequences for performance, error messages, and what you can and cannot do at runtime.

---

## Quick Overview

A **generic function** is parameterized over one or more types written in angle brackets (`fn name<T>(...)`). The same source produces a separate, specialized machine-code copy for each concrete type it is used with: a process called **monomorphization**. This is the opposite of TypeScript, where generics are erased before the code ever runs, so a TS dev's main adjustment is realizing that in Rust generics are a *compile-time* mechanism with *zero runtime cost*, not a runtime feature.

---

## TypeScript/JavaScript Example

```typescript
// TypeScript - generic functions with type parameters
function first<T>(items: T[]): T {
  return items[0];
}

function pair<A, B>(a: A, b: B): [A, B] {
  return [a, b];
}

// The type parameter is inferred from the arguments...
const n = first([10, 20, 30]); // T = number, n: number
const s = first(["a", "b"]); // T = string, s: string
const p = pair(1, "one"); // A = number, B = string

// ...or supplied explicitly with the same angle-bracket syntax:
const x = first<number>([1, 2, 3]);

// At RUNTIME, T is gone. This compiles and runs — and crashes:
function bad<T>(items: T[]): T {
  return items[0];
}
console.log(bad<number>([])); // undefined — no error, types were erased
```

**Key points:**

- `<T>` declares a type parameter; TypeScript infers it from arguments.
- Generics are a **compile-time-only** check. `tsc` erases them and emits plain JavaScript; there is no `T` left at runtime.
- Because the types are gone, you cannot inspect `T`, branch on it, or `new T()`. And mistakes the types *should* have caught (like indexing an empty array) still produce `undefined` at runtime.

---

## Rust Equivalent

```rust playground
// Rust - the same two functions, with type parameters in <>
fn first<T>(items: &[T]) -> &T {
    &items[0]
}

fn pair<A, B>(a: A, b: B) -> (A, B) {
    (a, b)
}

fn main() {
    let nums = [10, 20, 30];
    let n = first(&nums); // T = i32, inferred from the argument
    println!("first: {n}");

    let words = ["a", "b"];
    let s = first(&words); // T = &str, inferred
    println!("first: {s}");

    let p = pair(1, "one"); // A = i32, B = &str
    println!("pair: {p:?}"); // prints: pair: (1, "one")
}
```

Real output from `cargo run`:

```text
first: 10
first: a
pair: (1, "one")
```

**Key points:**

- The syntax is strikingly similar: `fn first<T>(...)`.
- Rust infers `T` from arguments just like TypeScript.
- But unlike TypeScript, `first(&nums)` and `first(&words)` cause the compiler to **generate two distinct copies** of `first`: one specialized for `i32`, one for `&str`. The generic is resolved entirely at compile time, then erased into concrete, optimized code.

> **Note:** Rust borrows the slice (`&[T]`) instead of taking ownership of an array. We will not dwell on borrowing here — see [Section 05: Ownership](/05-ownership/) — but notice `first` returns `&T` (a reference), so it does not have to copy the element out.

---

## Detailed Explanation

### Declaring and inferring type parameters

```rust
fn first<T>(items: &[T]) -> &T {
    &items[0]
}
```

- `<T>` after the function name introduces the type parameter. `T` is just a name; `U`, `Item`, or `Value` work equally well, though single uppercase letters are the convention for "any type."
- Inside the body, `T` stands for whatever concrete type the caller uses. You may use `T` in the parameter list (`&[T]`), the return type (`&T`), and local bindings.
- At the call site `first(&nums)`, Rust unifies `&[T]` with `&[i32; 3]` (coerced to `&[i32]`) and concludes `T = i32`. No annotation needed.

### Monomorphization: the big mental shift from TypeScript

This is the single most important idea for a TS/JS developer. TypeScript **erases** generics; Rust **monomorphizes** them. When you call a generic function with `i32` and with `f64`, the compiler stamps out two separate functions, each with the concrete type baked in, exactly as if you had hand-written `id_i32` and `id_f64`.

We can prove it. Compiling this program to LLVM IR:

```rust playground
use std::hint::black_box;

#[inline(never)]
pub fn id<T>(x: T) -> T {
    x
}

fn main() {
    let a = id::<i32>(black_box(5));
    let b = id::<f64>(black_box(2.5));
    println!("{} {}", black_box(a), black_box(b));
}
```

...produces two distinct, fully concrete definitions of `id` in the emitted IR: one taking an `i32`, one taking a `double` (Rust's `f64`):

```text
define internal i32    @_ZN4mono2id17h73ef7d59100e96ddE(i32 %x) ...
define internal double @_ZN4mono2id17h901c7050e1366328E(double %x) ...
```

> **Note:** Those two `define` lines are real output from `rustc --emit=llvm-ir`. The `i32` copy and the `double` (f64) copy are genuinely separate machine functions generated from one generic source.

The consequences for a TypeScript developer:

| Aspect | TypeScript (erasure) | Rust (monomorphization) |
| --- | --- | --- |
| When is `T` resolved? | Compile time, then erased | Compile time, then specialized into concrete code |
| Runtime representation of `T` | None; it does not exist | None needed, every copy is concrete |
| Runtime cost of generics | The code is just plain JS | **Zero**, as fast as hand-written concrete code |
| Can you inspect `T` at runtime? | No (`typeof` sees the value, not `T`) | No (and you rarely need to) |
| Effect on binary/output size | None (one copy) | Larger binary: one copy **per type used** ("code bloat") |
| Compile time | Fast | Slower; more code to generate and optimize |

The headline win is **zero-cost abstraction**: a generic `first<T>` is exactly as fast as a `first_i32` you wrote by hand, because after monomorphization that is literally what exists. The trade-off is binary size and compile time, since each instantiation is real code.

### Inferred vs explicit type arguments — the turbofish `::<>`

Most of the time Rust infers the type parameter. When it cannot, or when you want to be explicit, you supply the type argument. In an expression, that uses the **turbofish** syntax `::<>`:

```rust playground
fn largest<T: PartialOrd + Copy>(list: &[T]) -> T {
    let mut max = list[0];
    for &item in list {
        if item > max {
            max = item;
        }
    }
    max
}

fn main() {
    // Inferred from the argument:
    let m = largest(&[3, 9, 1]);
    println!("{m}");

    // Explicit, using the turbofish on the function name:
    let m = largest::<i32>(&[3, 9, 1]);
    println!("{m}");
}
```

The name "turbofish" comes from the `::<>` glyph resembling a fish. Why the leading `::`? Because `largest<i32>(...)` would be ambiguous to the parser: `<` could be the less-than operator. The `::` disambiguates "this is a type argument list," not a comparison.

You will see the turbofish most often on standard-library methods whose return type the compiler cannot otherwise pin down:

```rust playground
fn main() {
    // `parse` is generic over its return type; tell it what to produce:
    let n = "42".parse::<i32>().unwrap();
    println!("{n}"); // 42

    // `collect` builds "some collection" — turbofish picks which one:
    let v = (0..3).collect::<Vec<i32>>();
    println!("{v:?}"); // [0, 1, 2]
}
```

Equivalently, you can move the type to the *binding* and let inference flow backward; these two lines are interchangeable:

```rust playground
fn main() {
    let v = (0..3).collect::<Vec<i32>>(); // turbofish on the method
    let v: Vec<i32> = (0..3).collect();   // annotation on the variable
    println!("{v:?}");
}
```

This "type flows backward from the annotation" behavior has no real TypeScript analog: in TS you almost always write `[...]` and the array type is concrete already.

### Generic functions usually need trait bounds

In TypeScript a bare `<T>` is fully usable: you can pass it around, put it in arrays, return it. In Rust a bare `<T>` is deliberately *almost useless*: you can move it, return it, or store it, but you cannot do anything that requires a *capability*, because the compiler must guarantee the operation works for **every** possible `T`. Comparing with `>`, adding with `+`, or printing with `{}` are all capabilities you must request via **trait bounds** (`T: PartialOrd`, `T: Display`, ...). That is why `largest` above is written `largest<T: PartialOrd + Copy>` rather than just `largest<T>`. Trait bounds are a topic of their own — see [Trait Bounds](/09-generics-traits/05-trait-bounds/) — but you cannot write many generic functions without at least one, so they appear here too.

---

## Key Differences

| Concept | TypeScript | Rust |
| --- | --- | --- |
| Declaration | `function f<T>(x: T): T` | `fn f<T>(x: T) -> T` |
| Inference | Yes, from arguments | Yes, from arguments |
| Explicit type arg | `f<number>(x)` | `f::<i32>(x)` (turbofish in expressions) |
| Compilation model | **Erasure**: one runtime copy | **Monomorphization**: one copy per concrete type |
| Runtime cost | None (it is just JS) | Zero (specialized to concrete code) |
| Capabilities on `T` | Anything; unsound casts via `any` slip through | Only what **trait bounds** permit; checked |
| "Empty" bound `<T>` | Fully usable | Can only move/store/return, no operations |
| Inspect `T` at runtime | No | No (use enums/trait objects for runtime polymorphism) |
| Binary size impact | None | Grows with number of instantiations |

The conceptual core: **TypeScript generics are a type-checker feature that disappears; Rust generics are a code-generation feature.** Rust trades larger binaries and slower compiles for code that is as fast as if you had never used generics at all, and for the guarantee that every operation on `T` is provably valid for every `T` you actually use.

> **Tip:** Reach for generics when you want one implementation specialized per type at compile time (the common case). When you instead need *runtime* polymorphism — a heterogeneous list of "things that implement `Draw`," chosen at runtime — that is [trait objects](/09-generics-traits/06-trait-objects/) (`dyn Trait`), Rust's dynamic-dispatch counterpart.

---

## Common Pitfalls

### Pitfall 1: Forgetting a trait bound when you operate on `T`

A TypeScript dev expects `T` to "just work" with `>` or `+`. Rust refuses, because it cannot prove those operations are valid for every `T`.

```rust
// does not compile (error E0369: binary operation `>` cannot be applied to type `T`)
fn largest<T>(list: &[T]) -> T {
    let mut max = list[0];
    for &item in list {
        if item > max {
            max = item;
        }
    }
    max
}
```

Real compiler output:

```text
error[E0369]: binary operation `>` cannot be applied to type `T`
 --> err1.rs:4:17
  |
4 |         if item > max {
  |            ---- ^ --- T
  |            |
  |            T
  |
help: consider restricting type parameter `T` with trait `PartialOrd`
  |
1 | fn largest<T: std::cmp::PartialOrd>(list: &[T]) -> T {
  |             ++++++++++++++++++++++
```

The compiler even writes the fix for you: add `T: PartialOrd`. (For comparing-and-copying scalars like `i32`, you typically also want `+ Copy`, as in the working version above.)

The same thing happens with `+`:

```rust
// does not compile (error E0369: cannot add `T` to `T`)
fn add<T>(a: T, b: T) -> T {
    a + b
}
```

```text
error[E0369]: cannot add `T` to `T`
 --> err4.rs:2:7
  |
2 |     a + b
  |     - ^ - T
  |     |
  |     T
  |
help: consider restricting type parameter `T` with trait `Add`
  |
1 | fn add<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
  |         +++++++++++++++++++++++++++
```

### Pitfall 2: Ambiguous return type — the compiler needs a turbofish or annotation

`parse` and `collect` are generic over what they *produce*. With nothing to pin the result type, inference fails:

```rust
// does not compile (error E0284: type annotations needed)
fn main() {
    let n = "42".parse().unwrap();
    println!("{n}");
}
```

```text
error[E0284]: type annotations needed
 --> err2.rs:2:9
  |
2 |     let n = "42".parse().unwrap();
  |         ^        ----- type must be known at this point
  |
  = note: cannot satisfy `<_ as FromStr>::Err == _`
help: consider giving `n` an explicit type
  |
2 |     let n: /* Type */ = "42".parse().unwrap();
  |          ++++++++++++
```

Fix it with the turbofish (`"42".parse::<i32>()`) or a binding annotation (`let n: i32 = ...`). `collect` gives the analogous `E0283`:

```text
error[E0283]: type annotations needed
    --> err3.rs:2:9
     |
   2 |     let v = (0..5).collect();
     |         ^          ------- type must be known at this point
     |
     = note: cannot satisfy `_: FromIterator<i32>`
note: required by a bound in `collect`
    --> .../library/core/src/iter/traits/iterator.rs:2014:19
     |
2014 |     fn collect<B: FromIterator<Self::Item>>(self) -> B
     |                   ^^^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `Iterator::collect`
help: consider giving `v` an explicit type
     |
   2 |     let v: Vec<_> = (0..5).collect();
     |          ++++++++
```

### Pitfall 3: Expecting to inspect or branch on `T` at runtime

Because generics are monomorphized and then erased into concrete code, there is no runtime "type object" to look at. There is no `typeof T`, no `if (T === Number)`, no `new T()`. If you need to make a runtime decision based on the variant, that is a job for an [`enum`](/09-generics-traits/02-generic-enums/) or a [trait object](/09-generics-traits/06-trait-objects/), not for generics. This mirrors TypeScript's erasure (you cannot do `if (T === ...)` in TS either), but TS devs sometimes *think* they can via `instanceof` on values, which checks the runtime value, not the erased type parameter.

### Pitfall 4: Assuming generics are "free" like in TypeScript

In TypeScript a generic costs nothing at runtime and nothing in output size; it is the same emitted JS. In Rust, every distinct type you instantiate a generic with generates another copy of the code. Calling `largest` with `i32`, `f64`, `char`, and `u8` produces four specialized functions. This is usually a non-issue, but for very large generic functions used with many types it can bloat the binary and slow compiles. When that matters, you can extract the type-independent work into a non-generic inner function (a technique sometimes called "outlining").

---

## Best Practices

- **Let inference do the work.** Write `let n = parse_count(input);` and only add a turbofish or annotation when the compiler asks. Over-annotating reads as noise.
- **Add the minimum bounds you need, no more.** Each bound is a promise the caller must keep. `T: PartialOrd` is weaker (more permissible) than `T: Ord`; pick the loosest one that compiles. See [Trait Bounds](/09-generics-traits/05-trait-bounds/).
- **Name type parameters meaningfully when it aids clarity.** `T` is fine for a one-off, but `fn group_by<T, K, F>(...)` reading "item, key, function" is clearer than `<A, B, C>`.
- **Prefer borrowing generic inputs (`&[T]`, `&T`)** over taking ownership when you only need to read, so callers are not forced to give up their data.
- **Reach for `impl Trait` in argument position** for simple "accept any iterator / any closure" cases; it is a lighter-weight sugar over a generic parameter. See [`impl Trait`](/09-generics-traits/07-impl-trait/).
- **Use generics for compile-time, same-shape-per-type code; use `dyn Trait` for runtime, heterogeneous collections.** Knowing which axis you are on prevents fighting the borrow checker later.

> **Tip:** When you find yourself writing the same function body twice for `i32` and `f64`, that is the signal to make it generic. When you find yourself wanting a `Vec` holding *several different* concrete types at once, that is the signal you want a trait object instead.

---

## Real-World Example

A small data-pipeline module: two reusable, fully generic helpers, `group_by` (bucket items by a derived key) and `max_by_key` (find the "biggest" item by a derived score). Both are parameterized over the element type *and* a closure, so they work for any data the caller throws at them. This is the kind of generic utility you would otherwise reach for Lodash to provide in TypeScript.

```rust playground
use std::collections::HashMap;
use std::hash::Hash;

/// Group items into buckets keyed by a value derived from each item.
fn group_by<T, K, F>(items: Vec<T>, key_of: F) -> HashMap<K, Vec<T>>
where
    K: Eq + Hash,
    F: Fn(&T) -> K,
{
    let mut groups: HashMap<K, Vec<T>> = HashMap::new();
    for item in items {
        let key = key_of(&item);
        groups.entry(key).or_default().push(item);
    }
    groups
}

/// Return the item with the largest derived key, or `None` if empty.
fn max_by_key<T, K, F>(items: &[T], key_of: F) -> Option<&T>
where
    K: PartialOrd,
    F: Fn(&T) -> K,
{
    let mut best: Option<&T> = None;
    let mut best_key: Option<K> = None;
    for item in items {
        let k = key_of(item);
        match &best_key {
            Some(bk) if !(k > *bk) => {}
            _ => {
                best = Some(item);
                best_key = Some(k);
            }
        }
    }
    best
}

#[derive(Debug)]
struct Order {
    id: u32,
    customer: &'static str,
    total: f64,
}

fn main() {
    let orders = vec![
        Order { id: 1, customer: "ada", total: 42.0 },
        Order { id: 2, customer: "linus", total: 99.5 },
        Order { id: 3, customer: "ada", total: 10.0 },
    ];

    // `max_by_key` is inferred: T = Order, K = f64, F = the closure
    let biggest = max_by_key(&orders, |o| o.total);
    println!("biggest order: {:?}", biggest.map(|o| o.id));

    // `group_by` consumes the Vec and buckets by customer name
    let by_customer = group_by(orders, |o| o.customer);
    let mut customers: Vec<_> = by_customer.keys().copied().collect();
    customers.sort();
    for c in customers {
        println!("{c}: {} orders", by_customer[c].len());
    }
}
```

Real output from `cargo run`:

```text
biggest order: Some(2)
ada: 2 orders
linus: 1 orders
```

Notice that all three type parameters of each function are inferred — no turbofish needed at the call sites — because the `Vec<Order>` and the closures fully determine `T`, `K`, and `F`. The standard library's own [`Iterator::max_by_key`](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.max_by_key) is built on exactly this pattern; here we wrote our own to see the mechanics.

> **Note:** `F: Fn(&T) -> K` is a trait bound on a *closure type*: closures in Rust each have a unique, unnameable type, so you accept them via a generic parameter bounded by the `Fn` family. The standard `HashMap` used here is covered in [Section 07: Collections](/07-collections/).

---

## Further Reading

- [The Rust Book — Generic Data Types](https://doc.rust-lang.org/book/ch10-01-syntax.html) — the canonical introduction, including the `largest` example.
- [The Rust Reference — Generic parameters](https://doc.rust-lang.org/reference/items/generics.html) — the precise grammar and rules.
- [Rust by Example — Generics](https://doc.rust-lang.org/rust-by-example/generics.html) — bite-sized runnable examples.
- [The Rustonomicon — Monomorphization](https://doc.rust-lang.org/nomicon/) and the Reference on codegen — for how specialization actually works under the hood.
- Sibling topics in this section:
  - [Generic Structs](/09-generics-traits/01-generic-structs/) — putting type parameters on data structures.
  - [Generic Enums](/09-generics-traits/02-generic-enums/) — `Option<T>` and `Result<T, E>` as the canonical examples.
  - [Trait Bounds](/09-generics-traits/05-trait-bounds/) — `<T: Trait>`, `where` clauses, and bounds on return types.
  - [Traits](/09-generics-traits/03-traits/) — the "interfaces" that bounds refer to.
  - [`impl Trait`](/09-generics-traits/07-impl-trait/) — lighter-weight generics in argument and return position.
  - [Trait Objects](/09-generics-traits/06-trait-objects/) — `dyn Trait`, the *dynamic-dispatch* alternative to monomorphized generics.
- Related earlier sections: [Section 02: Basic Types](/02-basics/01-types/), [Section 05: Ownership](/05-ownership/), [Section 07: Collections](/07-collections/).
- Up next after this section: [Section 10: Smart Pointers](/10-smart-pointers/), where `Box<dyn Trait>` ties generics and trait objects together.

---

## Exercises

### Exercise 1: Swap a pair

**Difficulty:** Beginner

**Objective:** Get comfortable declaring multiple type parameters and returning a generic tuple.

**Instructions:** Write a generic function `swap` that takes a tuple `(A, B)` and returns it with the elements swapped, as `(B, A)`. It should require no trait bounds. Verify that `swap((1, "a"))` returns `("a", 1)`.

```rust
fn swap<A, B>(pair: (A, B)) -> (B, A) {
    // TODO: destructure and return swapped
    todo!()
}

fn main() {
    let result = swap((1, "a"));
    println!("{result:?}"); // ("a", 1)
}
```

<details>
<summary>Solution</summary>

```rust playground
fn swap<A, B>(pair: (A, B)) -> (B, A) {
    let (a, b) = pair;
    (b, a)
}

fn main() {
    let result = swap((1, "a"));
    println!("{result:?}"); // ("a", 1)
    assert_eq!(swap((1, "a")), ("a", 1));
}
```

No trait bounds are needed: we only *move* the values, never operate on them, so the empty `<A, B>` is enough. This is one of the rare cases where a bound-free generic is genuinely useful.

</details>

### Exercise 2: Print any two displayable values

**Difficulty:** Intermediate

**Objective:** Apply trait bounds so the function body can actually *use* the type parameters, and observe the compiler error if you forget them.

**Instructions:** Write a generic function `print_pair<T, U>(a: T, b: U)` that prints `a` and `b` in the form `"<a> and <b>"`. To use `{}` formatting on `a` and `b`, you must bound both parameters with `std::fmt::Display`. First try it *without* the bounds and read the error; then add them.

```rust
fn print_pair<T, U>(a: T, b: U) {
    // TODO: add the necessary bounds in the signature, then:
    println!("{a} and {b}");
}

fn main() {
    print_pair(1, "one");
    print_pair(3.14, 'x');
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::fmt::Display;

fn print_pair<T: Display, U: Display>(a: T, b: U) {
    println!("{a} and {b}");
}

fn main() {
    print_pair(1, "one");   // 1 and one
    print_pair(3.14, 'x');  // 3.14 and x
}
```

Without the `Display` bounds, the compiler rejects the `{}` formatting with `error[E0277]: T doesn't implement std::fmt::Display`. The bound is the promise "every type passed here can be displayed," which the compiler then enforces at each call site.

</details>

### Exercise 3: Count matching elements with a predicate closure

**Difficulty:** Advanced

**Objective:** Combine a generic element type with a generic closure parameter, using a `where` clause.

**Instructions:** Write `count_matching<T, F>(items: &[T], pred: F) -> usize` that returns how many elements satisfy the predicate `pred`. The closure parameter `F` must be bounded by `Fn(&T) -> bool`. Use a `where` clause for readability. Verify that counting the even numbers in `[1, 2, 3, 4]` yields `2`.

```rust
fn count_matching<T, F>(items: &[T], pred: F) -> usize
where
    // TODO: bound F so it can be called as `pred(&item) -> bool`
{
    // TODO: count the matches (an iterator chain works well)
    todo!()
}

fn main() {
    let evens = count_matching(&[1, 2, 3, 4], |&n| n % 2 == 0);
    println!("evens: {evens}"); // 2
}
```

<details>
<summary>Solution</summary>

```rust playground
fn count_matching<T, F>(items: &[T], pred: F) -> usize
where
    F: Fn(&T) -> bool,
{
    items.iter().filter(|x| pred(x)).count()
}

fn main() {
    let evens = count_matching(&[1, 2, 3, 4], |&n| n % 2 == 0);
    println!("evens: {evens}"); // 2
    assert_eq!(count_matching(&[1, 2, 3, 4], |&n| n % 2 == 0), 2);

    // Works for any element type, because T is generic:
    let words = ["hi", "hello", "yo"];
    assert_eq!(count_matching(&words, |s| s.len() > 2), 1); // only "hello"
}
```

`F: Fn(&T) -> bool` lets the function accept any closure (or function pointer) that takes a `&T` and returns a `bool`. Because the closure has a unique, unnameable type, accepting it through a generic parameter is the idiomatic way. See [Section 03: Functions](/03-functions/) for closures and [`impl Trait`](/09-generics-traits/07-impl-trait/) for the lighter-weight `pred: impl Fn(&T) -> bool` spelling of the same thing.

</details>
