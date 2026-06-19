---
title: "Trait Bounds"
description: "Rust trait bounds limit a generic to types with the abilities you need, like TypeScript's extends: <T: Trait>, multiple bounds with +, and where clauses."
---

In TypeScript, a generic like `<T extends Comparable>` says "`T` can be any type, as long as it has these capabilities." Rust calls that constraint a **trait bound**, and it is not optional sugar: a generic function can only call a method on `T` if a bound *proves* `T` has that method. The bound is the contract that turns a type parameter from an opaque blob into something you can actually use.

---

## Quick Overview

A **trait bound** restricts a generic type parameter to types that implement a particular **trait** (Rust's version of an interface). You write it as `<T: Trait>`, combine several with `+`, and, when the list gets long, move them into a `where` clause for readability. Bounds also govern what a generic function is allowed to *return*. Unlike TypeScript's `extends` constraints, which vanish at runtime, Rust checks every bound at compile time and then generates specialized machine code for each concrete type ([monomorphization](/09-generics-traits/00-generic-functions/)).

> **Note:** This page focuses on the *bounds* themselves: the `<T: Trait>` syntax, multiple bounds, `where` clauses, and bounds on return types. Defining and implementing traits is covered in [Traits](/09-generics-traits/03-traits/); writing the generic functions that carry these bounds is in [Generic Functions](/09-generics-traits/00-generic-functions/).

---

## TypeScript/JavaScript Example

In TypeScript you constrain a generic with `extends`. The constraint tells the compiler which properties and methods are guaranteed to exist on `T`, so the body can use them.

```typescript
// TypeScript - a generic constrained to "things that can be compared"
interface Ordered {
  compareTo(other: this): number; // negative / 0 / positive
}

// `T extends Ordered` is the constraint: T must have compareTo.
function largest<T extends Ordered>(list: T[]): T {
  let biggest = list[0];
  for (const item of list) {
    if (item.compareTo(biggest) > 0) {
      biggest = item;
    }
  }
  return biggest;
}

class Version implements Ordered {
  constructor(public major: number, public minor: number) {}
  compareTo(other: Version): number {
    return this.major - other.major || this.minor - other.minor;
  }
}

const versions = [new Version(1, 2), new Version(2, 0), new Version(1, 9)];
console.log(largest(versions)); // Version { major: 2, minor: 0 }
```

**Key points:**

- `extends Ordered` is the constraint; without it, `item.compareTo` would be a type error.
- Multiple constraints use an intersection type: `<T extends A & B>`.
- At runtime the constraint is *erased*. `largest` is one function; TypeScript checked the types and then threw the type information away. There is no per-type specialization.

---

## Rust Equivalent

Rust expresses the same idea with a trait bound. Here we lean on the standard library's `PartialOrd` trait (which gives us the `>` operator) instead of inventing a `compareTo`.

```rust
use std::fmt::Display;

// `T: PartialOrd` is the bound: T must support `<`, `>`, etc.
fn largest<T: PartialOrd>(list: &[T]) -> &T {
    let mut biggest = &list[0];
    for item in list {
        if item > biggest {
            biggest = item;
        }
    }
    biggest
}

// Multiple bounds with `+`: T must be BOTH Display (printable) AND PartialOrd.
fn announce_largest<T: Display + PartialOrd>(list: &[T]) {
    let winner = largest(list);
    println!("The largest value is {winner}");
}

fn main() {
    let numbers = [34, 50, 25, 100, 65];
    let words = ["pear", "apple", "fig", "banana"];

    announce_largest(&numbers);
    announce_largest(&words);
}
```

**Output (compile-verified):**

```text
The largest value is 100
The largest value is pear
```

**Key points:**

- `<T: PartialOrd>` is the bound. Drop it and `item > biggest` will not compile — Rust refuses to assume `T` is comparable.
- `Display + PartialOrd` requires *both* traits; the `+` reads as "and."
- Unlike TypeScript, this is **monomorphized**: the compiler stamps out a separate, optimized `largest` for `i32` and another for `&str`. The bound is checked once, at compile time, then erased into concrete code.

---

## Detailed Explanation

### Why the bound is mandatory, not optional

In TypeScript an *unconstrained* `<T>` still lets you do a lot: you can pass `T` around, put it in arrays, return it. But the moment you call a method, you need a constraint. Rust takes this further: with a bare `<T>`, the **only** things you can do with a `T` value are move it, store it, and pass it on. You cannot print it, compare it, clone it, or add it, because nothing has promised those operations exist.

```rust
use std::fmt::Display;

trait Summary {
    fn summarize(&self) -> String;
}

struct Article {
    headline: String,
    word_count: u32,
}

impl Summary for Article {
    fn summarize(&self) -> String {
        format!("{} ({} words)", self.headline, self.word_count)
    }
}

// The bound `T: Summary` is precisely what makes `.summarize()` callable below.
fn print_summary<T: Summary>(item: &T) {
    println!("Summary: {}", item.summarize());
}

fn main() {
    let a = Article {
        headline: "Rust 1.96 released".into(),
        word_count: 1200,
    };
    print_summary(&a);
}
```

**Output (compile-verified):**

```text
Summary: Rust 1.96 released (1200 words)
```

Without `T: Summary`, the call `item.summarize()` would fail to compile, because the compiler does not know whether an arbitrary `T` has a `summarize` method. The bound is the proof.

> **Tip:** Think of a bound as a *capability passport*. Inside the function body you may use exactly the methods the bounds grant — no more. This is why generic code in Rust feels stricter than in TypeScript: every capability must be declared up front.

### Single bound: `<T: Trait>`

The simplest form. `T` may be any type that implements `Trait`:

```rust
// Accepts any type that knows how to clone itself.
fn duplicate<T: Clone>(value: T) -> (T, T) {
    (value.clone(), value)
}
```

### Multiple bounds: `<T: A + B>`

Use `+` to require several traits at once. The order does not matter:

```rust
use std::fmt::Debug;

// T must be clonable AND debug-printable.
fn clone_and_log<T: Clone + Debug>(value: &T) -> T {
    println!("cloning {value:?}");
    value.clone()
}
```

This mirrors TypeScript's `<T extends A & B>`, but `+` is a bound combinator, not a runtime type intersection.

### `where` clauses: bounds without the clutter

When you have several type parameters each with several bounds, the angle-bracket form becomes a wall of text. A `where` clause moves the bounds below the signature, where they read top-to-bottom:

```rust
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::hash::Hash;

// `where` clause version — identical meaning, far more readable.
fn summarize<K, V>(map: &HashMap<K, V>) -> String
where
    K: Display + Eq + Hash,
    V: Debug,
{
    let mut parts: Vec<String> = map
        .iter()
        .map(|(k, v)| format!("{k} = {v:?}"))
        .collect();
    parts.sort(); // deterministic output for the example
    parts.join(", ")
}

fn main() {
    let mut scores = HashMap::new();
    scores.insert("alice", vec![90, 85]);
    scores.insert("bob", vec![70]);

    println!("{}", summarize(&scores));
}
```

**Output (compile-verified):**

```text
alice = [90, 85], bob = [70]
```

The inline equivalent, `fn summarize<K: Display + Eq + Hash, V: Debug>(...)`, compiles to the same thing. Pick whichever is more readable. The community convention is: one short bound inline, anything longer in a `where` clause. `where` clauses also enable bounds you *cannot* write inline, such as bounds on associated types or on `&T` rather than `T`.

### Bounds on the return type

Bounds gate what you can call and what you can *produce*. Two common patterns:

**1. A bound that lets you synthesize a return value.** Here `T: Default` is what makes `T::default()` legal:

```rust
// `T: Default` lets us conjure a value when the Option is None.
fn or_default<T: Default>(opt: Option<T>) -> T {
    match opt {
        Some(v) => v,
        None => T::default(),
    }
}

fn main() {
    let a: i32 = or_default(Some(42));
    let b: i32 = or_default(None);
    let c: String = or_default(None);
    println!("{a} {b} {c:?}");
}
```

**Output (compile-verified):**

```text
42 0 ""
```

**2. Returning a *bounded* anonymous type with `impl Trait`.** Sometimes the concrete return type is unspeakable (a closure, a chained iterator). You return "some type that satisfies this bound" and let the compiler fill in the real type:

```rust
// The caller only learns: "this returns something that yields u32s."
fn evens(upto: u32) -> impl Iterator<Item = u32> {
    (0..upto).filter(|n| n % 2 == 0)
}

fn main() {
    let collected: Vec<u32> = evens(10).collect();
    println!("{collected:?}");
}
```

**Output (compile-verified):**

```text
[0, 2, 4, 6, 8]
```

`impl Iterator<Item = u32>` in return position is a *return-position bound*: it promises the caller the value implements `Iterator` without naming the messy concrete type (`Filter<Range<u32>, {closure}>`). This is a distinct feature with its own subtleties; see [impl Trait](/09-generics-traits/07-impl-trait/) for the full treatment. The key idea for *this* page is that a bound can describe a return value, not only an argument.

> **Warning:** `impl Trait` in return position names **one** hidden concrete type for the whole function. You cannot return a `Filter` from one branch and a `Vec`'s iterator from another, even though both implement `Iterator`. We will see the exact compiler error in [Common Pitfalls](#common-pitfalls).

### `impl Trait` in argument position is just a bound in disguise

This signature:

```rust
trait Summary {
    fn summarize(&self) -> String;
}

struct Article {
    headline: String,
}

impl Summary for Article {
    fn summarize(&self) -> String {
        self.headline.clone()
    }
}

fn print_summary(item: &impl Summary) {
    println!("{}", item.summarize());
}

fn main() {
    let a = Article {
        headline: "Rust 1.96 released".into(),
    };
    print_summary(&a);
}
```

is sugar for the generic form `fn print_summary<T: Summary>(item: &T)`. They generate the same code. The `&impl Summary` form is shorter when you have a single parameter and no need to name `T`; the explicit `<T: Summary>` form is required when two parameters must be the *same* type. (More on this trade-off in [impl Trait](/09-generics-traits/07-impl-trait/).)

---

## Key Differences

| Aspect | TypeScript (`extends`) | Rust (trait bound) |
| --- | --- | --- |
| Syntax | `<T extends Constraint>` | `<T: Trait>` |
| Multiple constraints | `<T extends A & B>` | `<T: A + B>` |
| Long constraint lists | inline only | `where` clause available |
| Runtime presence | **erased**: one function for all types | **monomorphized**: one specialized copy per concrete type |
| Unconstrained `<T>` | can still access `Object` methods, structural shape | can *only* move/store/pass the value |
| Constraint on a method's existence | structural ("has this shape") | nominal ("implements this trait") |
| Return-type constraint | `: SomeInterface` return annotation | `-> impl Trait` (one hidden type) |
| Numeric/operator constraints | not really expressible | `T: Add`, `T: PartialOrd`, etc. |

### Structural vs nominal

TypeScript constraints are **structural**: `T extends { len(): number }` is satisfied by *any* object with a `len` method, whether or not it ever heard of your interface. Rust bounds are **nominal**: `T: HasLen` is satisfied only by types that *explicitly* wrote `impl HasLen for ThatType`. This is stricter, but it means a bound is a deliberate contract, not an accident of shape. (The flip side, that you cannot retroactively bolt a foreign trait onto a foreign type, is the [orphan rule](/09-generics-traits/12-orphan-rule/).)

### Bounds enable operators

In TypeScript you cannot write a generic "add anything addable" function, because `+` is hardcoded to `number`/`string`. In Rust, operators *are* traits, so `T: std::ops::Add<Output = T>` is a perfectly ordinary bound. This is why generic numeric code is expressible in Rust but awkward in TypeScript. See [Operator Overloading](/09-generics-traits/10-operator-overloading/).

---

## Common Pitfalls

### Pitfall 1: Using an operator or method without the bound that grants it

The single most common beginner error. You write a generic function, use `>` (or `.clone()`, or `{}` formatting), and forget to declare the bound.

```rust
// does not compile (error[E0369]: binary operation `>` cannot be applied to type `&T`)
fn largest<T>(list: &[T]) -> &T {
    let mut biggest = &list[0];
    for item in list {
        if item > biggest {
            biggest = item;
        }
    }
    biggest
}
```

The real compiler output:

```text
error[E0369]: binary operation `>` cannot be applied to type `&T`
 --> src/main.rs:5:17
  |
5 |         if item > biggest {
  |            ---- ^ ------- &T
  |            |
  |            &T
  |
help: consider restricting type parameter `T` with trait `PartialOrd`
  |
2 | fn largest<T: std::cmp::PartialOrd>(list: &[T]) -> &T {
  |             ++++++++++++++++++++++
```

The fix is exactly what the compiler suggests: add `<T: PartialOrd>`. Rust's diagnostics almost always tell you which bound is missing.

The same thing happens with methods. Calling `.clone()` without `T: Clone`:

```rust
// does not compile (error[E0599]: no method named `clone` found for type parameter `T`)
fn duplicate<T>(value: T) -> (T, T) {
    (value.clone(), value)
}
```

```text
error[E0599]: no method named `clone` found for type parameter `T` in the current scope
 --> src/main.rs:3:12
  |
2 | fn duplicate<T>(value: T) -> (T, T) {
  |              - method `clone` not found for this type parameter
3 |     (value.clone(), value)
  |            ^^^^^ method not found in `T`
  |
  = help: items from traits can only be used if the type parameter is bounded by the trait
help: the following trait defines an item `clone`, perhaps you need to restrict type parameter `T` with it:
  |
2 | fn duplicate<T: Clone>(value: T) -> (T, T) {
  |               +++++++
```

Add `<T: Clone>` and it compiles.

### Pitfall 2: Returning two different types from one `impl Trait`

Coming from TypeScript, you expect that since both `Map` and `Vec`'s iterator "are iterators," you can return either from an `-> impl Iterator` function. You cannot: `impl Trait` resolves to a *single* concrete type.

```rust
// does not compile (error[E0308]: `if` and `else` have incompatible types)
fn make_iter(flag: bool) -> impl Iterator<Item = i32> {
    if flag {
        (0..3).map(|n| n * 2)
    } else {
        vec![1, 2, 3].into_iter()
    }
}
```

The real error, abridged:

```text
error[E0308]: `if` and `else` have incompatible types
 --> src/main.rs:6:9
  |
4 |         (0..3).map(|n| n * 2)
  |         --------------------- expected because of this
6 |         vec![1, 2, 3].into_iter()
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^ expected `Map<Range<{integer}>, {closure@...}>`, found `IntoIter<{integer}>`
  |
help: you could change the return type to be a boxed trait object
  |
2 - fn make_iter(flag: bool) -> impl Iterator<Item = i32> {
2 + fn make_iter(flag: bool) -> Box<dyn Iterator<Item = i32>> {
```

The compiler even suggests the fix: when you genuinely need to return different concrete types from different branches, use a **trait object** (`Box<dyn Iterator<Item = i32>>`) and `Box::new(...)` each branch. That trades static dispatch for dynamic dispatch. See [Trait Objects](/09-generics-traits/06-trait-objects/).

### Pitfall 3: Over-constraining "just in case"

Bounds are part of your function's public contract. A bound you do not actually need makes the function harder to call for no benefit. Require `T: Clone + Debug + Display + PartialOrd` when the body only ever clones, and every caller is forced to satisfy three irrelevant traits.

```rust
// Over-constrained: only Clone is used.
fn make_pair<T: Clone + std::fmt::Debug + std::fmt::Display>(x: T) -> (T, T) {
    (x.clone(), x)
}

// Right-sized: ask only for what the body needs.
fn make_pair_better<T: Clone>(x: T) -> (T, T) {
    (x.clone(), x)
}
```

> **Tip:** Add a bound only when the compiler asks for it. If a method call fails to compile, *then* add the bound it names. Start minimal.

### Pitfall 4: Forgetting `Sized` is implicit (and when to relax it)

Every type parameter `<T>` carries an invisible `T: Sized` bound: the value has a known size at compile time. This is usually invisible and correct. But if you want to accept *unsized* types like `str` or `[T]` or `dyn Trait` behind a reference, you must opt out with `?Sized`:

```rust
use std::fmt::Display;

// `T: ?Sized` lets this accept `&str`, `&dyn Display`, etc. — not just sized types.
fn show<T: Display + ?Sized>(value: &T) {
    println!("{value}");
}

fn main() {
    show("a string slice"); // str is unsized — works thanks to ?Sized
    show(&42);
}
```

**Output (compile-verified):**

```text
a string slice
42
```

`?Sized` is a *relaxation*, not an additional bound. See [Marker Traits](/09-generics-traits/11-marker-traits/) for `Sized` and friends.

---

## Best Practices

- **Ask for the least.** Constrain a type parameter only by the traits the body actually uses. Minimal bounds = maximal callers.
- **Inline one short bound; use `where` for the rest.** `fn f<T: Clone>(...)` reads fine inline. The moment you have two parameters or three-plus traits, switch to a `where` clause.
- **Prefer `&str` / `&[T]` parameters over generic bounds when you only read.** A function that just reads a string slice should take `&str`, not `<S: AsRef<str>>`: simpler signature, no monomorphization bloat, identical ergonomics for the caller.
- **Reach for standard-library traits.** `Clone`, `Debug`, `Display`, `Default`, `PartialOrd`/`Ord`, `From`/`Into`, `Iterator`, `Hash`, `Eq` cover the vast majority of bounds. Custom traits are for genuinely domain-specific capabilities.
- **Let the compiler drive your bounds.** Write the body first; when a call fails, the error message names the exact trait to add. This avoids both under- and over-constraining.
- **Use `-> impl Trait` to hide gnarly return types**, but remember it is one concrete type. If branches return different types, switch to `Box<dyn Trait>`.

---

## Real-World Example

A generic "save this record" helper for a persistence layer. Any type that can be serialized (via [`serde`](/15-serialization/)'s `Serialize` trait) and logged (via `Debug`) can flow through one function, with no per-type save code. The bounds `Serialize + Debug` are the whole contract.

```rust
use serde::Serialize;
use std::fmt::Debug;

/// Persists any serializable, debug-printable record and returns its JSON.
/// The `where` clause is the contract: callers may pass anything that is
/// both `Serialize` (so we can turn it into JSON) and `Debug` (so we can log it).
fn save_record<T>(label: &str, record: &T) -> String
where
    T: Serialize + Debug,
{
    let json = serde_json::to_string(record).expect("serialize");
    println!("[{label}] saving {record:?}");
    json
}

#[derive(Debug, Serialize)]
struct User {
    id: u32,
    name: String,
}

#[derive(Debug, Serialize)]
struct Product {
    sku: String,
    price_cents: u64,
}

fn main() {
    let u = User { id: 1, name: "Ada".into() };
    let p = Product { sku: "RS-01".into(), price_cents: 4999 };

    // One generic function, two unrelated record types.
    let user_json = save_record("users", &u);
    let product_json = save_record("products", &p);

    println!("{user_json}");
    println!("{product_json}");
}
```

Add the dependencies with:

```bash
cargo add serde --features derive
cargo add serde_json
```

**Output (compile-verified):**

```text
[users] saving User { id: 1, name: "Ada" }
[products] saving Product { sku: "RS-01", price_cents: 4999 }
{"id":1,"name":"Ada"}
{"sku":"RS-01","price_cents":4999}
```

Because of monomorphization, the compiler generates a specialized `save_record` for `User` and another for `Product`. There is no runtime dispatch and no reflection; the `Serialize` and `Debug` bounds were resolved entirely at compile time. The TypeScript analogue, `function saveRecord<T>(label: string, record: T): string`, would lean on `JSON.stringify` and reflection at runtime; Rust bakes the serialization code in ahead of time, per type.

---

## Further Reading

### Official Documentation

- [The Rust Book — Traits: Defining Shared Behavior (Trait Bound Syntax)](https://doc.rust-lang.org/book/ch10-02-traits.html#traits-as-parameters)
- [The Rust Book — `where` clauses for clearer code](https://doc.rust-lang.org/book/ch10-02-traits.html#clearer-trait-bounds-with-where-clauses)
- [The Rust Reference — Trait and lifetime bounds](https://doc.rust-lang.org/reference/trait-bounds.html)
- [Rust by Example — Bounds](https://doc.rust-lang.org/rust-by-example/generics/bounds.html)
- [Rust by Example — `where` clauses](https://doc.rust-lang.org/rust-by-example/generics/where.html)

### Related Topics in This Guide

- [Generic Functions](/09-generics-traits/00-generic-functions/): declaring `<T>`, monomorphization, and the turbofish
- [Traits](/09-generics-traits/03-traits/): defining and implementing the traits you bound against
- [Trait Methods](/09-generics-traits/04-trait-methods/): what methods a bound actually grants you
- [impl Trait](/09-generics-traits/07-impl-trait/) — argument- and return-position `impl Trait` in depth
- [Trait Objects](/09-generics-traits/06-trait-objects/) — `Box<dyn Trait>` for when one return type is not enough
- [Supertraits](/09-generics-traits/09-supertraits/): bounds *on a trait itself*
- [Marker Traits](/09-generics-traits/11-marker-traits/): `Sized`, `Send`, `Sync`, and `?Sized`
- [Operator Overloading](/09-generics-traits/10-operator-overloading/) — bounding on `Add`, `Mul`, etc.
- [The Orphan Rule](/09-generics-traits/12-orphan-rule/): why bounds are nominal, not structural
- Foundations: [Getting Started](/01-getting-started/) and [Basics](/02-basics/)
- Next up after this section: [Smart Pointers](/10-smart-pointers/), where `Box<dyn Trait>` returns

---

## Exercises

### Exercise 1: A bounded `smallest`

**Difficulty:** Easy

**Objective:** Write a generic function with the right trait bounds to find the minimum of a slice.

**Instructions:** Implement `smallest` so it returns the smallest element of a slice *by value*. You will need two bounds: one to compare elements, and one to copy small values out of the slice. Make it work for both integers and floats.

```rust
fn smallest<T: /* ??? */>(list: &[T]) -> T {
    // TODO
    /* ??? */
}

fn main() {
    println!("{}", smallest(&[5, 2, 9, 1, 7]));   // 1
    println!("{}", smallest(&[3.5, 1.1, 2.2]));   // 1.1
}
```

<details>
<summary>Solution</summary>

```rust
fn smallest<T: PartialOrd + Copy>(list: &[T]) -> T {
    let mut min = list[0];
    for &item in list {
        if item < min {
            min = item;
        }
    }
    min
}

fn main() {
    println!("{}", smallest(&[5, 2, 9, 1, 7]));   // 1
    println!("{}", smallest(&[3.5, 1.1, 2.2]));   // 1.1
}
```

**Output:**

```text
1
1.1
```

`PartialOrd` grants `<`; `Copy` lets us pull each element out of the slice by value (`for &item in list`) and return one without borrowing. Floats only implement `PartialOrd` (not `Ord`), so `PartialOrd` is the correct, more general bound here.

</details>

### Exercise 2: Convert inline bounds to a `where` clause

**Difficulty:** Medium

**Objective:** Practice multiple bounds and the `where` syntax.

**Instructions:** Write `describe_extremes` that takes a slice and returns a `String` like `"min = 3, max = 22"`. The element type must be comparable, printable with `{}`, and copyable. Put all the bounds in a `where` clause rather than inline.

```rust
use std::fmt::Display;

fn describe_extremes<T>(list: &[T]) -> String
where
    /* ??? */
{
    // TODO
    /* ??? */
}

fn main() {
    println!("{}", describe_extremes(&[10, 4, 7, 22, 3]));
}
```

<details>
<summary>Solution</summary>

```rust
use std::fmt::Display;

fn describe_extremes<T>(list: &[T]) -> String
where
    T: PartialOrd + Display + Copy,
{
    let mut lo = list[0];
    let mut hi = list[0];
    for &item in list {
        if item < lo {
            lo = item;
        }
        if item > hi {
            hi = item;
        }
    }
    format!("min = {lo}, max = {hi}")
}

fn main() {
    println!("{}", describe_extremes(&[10, 4, 7, 22, 3]));
}
```

**Output:**

```text
min = 3, max = 22
```

`PartialOrd` powers the `<`/`>` comparisons, `Display` powers the `{lo}`/`{hi}` formatting, and `Copy` lets us read elements out by value. The `where` clause keeps the signature line clean; the inline form `<T: PartialOrd + Display + Copy>` compiles identically.

</details>

### Exercise 3: A bounded return type

**Difficulty:** Medium

**Objective:** Return a value described only by a trait bound, using `impl Trait`.

**Instructions:** Write `repeated(value, times)` that returns an iterator yielding `value` exactly `times` times. Do not name the concrete iterator type; return `impl Iterator<Item = i32>`. (Hint: `std::iter::repeat(value).take(times)`.)

```rust
fn repeated(value: i32, times: usize) -> /* ??? */ {
    // TODO
    /* ??? */
}

fn main() {
    let v: Vec<i32> = repeated(8, 3).collect();
    println!("{v:?}"); // [8, 8, 8]
}
```

<details>
<summary>Solution</summary>

```rust
fn repeated(value: i32, times: usize) -> impl Iterator<Item = i32> {
    std::iter::repeat(value).take(times)
}

fn main() {
    let v: Vec<i32> = repeated(8, 3).collect();
    println!("{v:?}"); // [8, 8, 8]
}
```

**Output:**

```text
[8, 8, 8]
```

The real return type is `std::iter::Take<std::iter::Repeat<i32>>`, which is verbose and an implementation detail. `impl Iterator<Item = i32>` is a return-position bound: it tells callers exactly what they can do with the value (iterate `i32`s) while hiding the concrete type. See [impl Trait](/09-generics-traits/07-impl-trait/) to go deeper.

</details>
