---
title: "Associated Types and Associated Constants"
description: "Associated types and constants let a Rust trait name one implementer-chosen type or value, a more constrained tool than the generic parameters TypeScript uses."
---

When a trait needs to refer to a type or a constant that *the implementer gets to choose*, Rust offers two tools that have no direct TypeScript equivalent: **associated types** and **associated constants**. This file is a light, practical introduction aimed at recognizing the syntax (`type Item;`, `Self::Item`, `const NAME: ...`) and knowing why it exists. The full, deep treatment (including where associated types beat generic parameters and how they interact with bounds) lives in [Section 09 — Generics and Traits](/09-generics-traits/).

---

## Quick Overview

An **associated type** is a placeholder type declared inside a trait (`type Item;`) that each implementer fills in (`type Item = i32;`). It lets a trait talk about "some related type" without that type being a generic parameter the caller has to spell out. The most famous example is the standard library's `Iterator`, whose `type Item` is "what each call to `next` produces."

An **associated constant** is the same idea for a value: a constant (`const MAX: Self;` or `const NAME: &'static str;`) attached to a trait or a type, where each implementer supplies the value. Rust's own `u8::MAX`, `i32::MAX`, and friends are associated constants.

**The point for a TypeScript/JavaScript developer:** TypeScript has no built-in concept of "a type the implementer chooses, named once and reused across the trait's methods." You normally fake it with a generic type parameter (`interface Container<Item>`). Associated types are a *different, more constrained* mechanism: there is exactly **one** choice per implementing type, and that single choice ties all of a trait's methods together.

---

## TypeScript/JavaScript Example

In TypeScript, when an interface needs to refer to "the element type," you reach for a **generic type parameter**:

```typescript
// TypeScript: the element type is a generic parameter on the interface.
interface Container<Item> {
  get(index: number): Item | undefined;
  first(): Item | undefined;
}

class IntStack implements Container<number> {
  constructor(private items: number[]) {}

  get(index: number): number | undefined {
    return this.items[index];
  }
  first(): number | undefined {
    return this.get(0);
  }
}

const s = new IntStack([10, 20, 30]);
console.log(s.get(0)); // 10
console.log(s.get(9)); // undefined
```

This works, but notice two things. First, `Item` is *open*: nothing stops you from writing a class that implements `Container<number>` **and** `Container<string>` at the same time. TypeScript is perfectly happy with `class X implements Container<number>, Container<string>`. Second, because TypeScript generics are **erased at runtime**, `Item` is purely a compile-time fiction; there is no `Item` value or type to inspect once the code runs.

For a constant-shared-across-a-contract, TypeScript developers usually fall back to a `static readonly` field or a module-level `const`; there is no first-class "every implementer must provide this constant, and the trait's methods can reference it" feature.

---

## Rust Equivalent

Rust offers the generic-parameter style too, but the *idiomatic* choice for "one element type per implementer" is an **associated type**:

```rust
trait Container {
    type Item; // associated type: the implementer decides what this is

    fn get(&self, index: usize) -> Option<&Self::Item>;

    // A default method can already use Self::Item before it is known.
    fn first(&self) -> Option<&Self::Item> {
        self.get(0)
    }
}

struct IntStack {
    items: Vec<i32>,
}

impl Container for IntStack {
    type Item = i32; // fill in the placeholder, exactly once
    fn get(&self, index: usize) -> Option<&i32> {
        self.items.get(index)
    }
}

struct StringStack {
    items: Vec<String>,
}

impl Container for StringStack {
    type Item = String;
    fn get(&self, index: usize) -> Option<&String> {
        self.items.get(index)
    }
}

fn main() {
    let ints = IntStack { items: vec![10, 20, 30] };
    let names = StringStack {
        items: vec!["ada".to_string(), "linus".to_string()],
    };

    println!("{:?}", ints.first());  // Some(10)
    println!("{:?}", names.first()); // Some("ada")
    println!("{:?}", ints.get(5));   // None
}
```

Real output:

```text
Some(10)
Some("ada")
None
```

For the constant side, here is the associated-const counterpart: both the inherent kind (attached directly to a type via an `impl` block) and the trait kind.

```rust
// (1) An inherent associated const: lives on the type itself.
struct Circle {
    radius: f64,
}

impl Circle {
    const PI: f64 = 3.14159265358979;

    fn area(&self) -> f64 {
        Self::PI * self.radius * self.radius
    }
}

// (2) An associated const declared on a trait, with a default value.
trait Animal {
    const LEGS: u32 = 4; // default
    fn legs(&self) -> u32 {
        Self::LEGS
    }
}

struct Dog;
struct Bird;

impl Animal for Dog {} // takes the default 4
impl Animal for Bird {
    const LEGS: u32 = 2; // overrides the default
}

fn main() {
    let c = Circle { radius: 2.0 };
    println!("area = {:.2}", c.area()); // area = 12.57
    println!("PI = {}", Circle::PI);    // reachable through the type

    println!("dog legs {}", Dog.legs());   // 4
    println!("bird legs {}", Bird.legs()); // 2

    // The standard library uses associated consts heavily:
    println!("u8::MAX = {}", u8::MAX); // 255
}
```

Real output:

```text
area = 12.57
PI = 3.14159265358979
dog legs 4
bird legs 2
u8::MAX = 255
```

---

## Detailed Explanation

### Associated types, line by line

```rust
trait Container {
    type Item;
    fn get(&self, index: usize) -> Option<&Self::Item>;
}
```

- `type Item;` declares an **associated type**, a named, implementer-chosen type. It is a *placeholder*: the trait does not know yet whether `Item` is an `i32`, a `String`, or anything else.
- `Self::Item` is how the trait's methods *refer* to that placeholder. `Self` is "whatever concrete type is implementing this trait," and `Self::Item` is "that type's choice of `Item`." (Read more about `Self` in [Associated Functions](/06-data-structures/06-associated-functions/).)
- In the `impl`, `type Item = i32;` is the moment the placeholder is filled in. From then on, for `IntStack`, `Self::Item` *is* `i32`, so `get` can return `Option<&i32>`.

The payoff is that the method signatures stay clean. `get` returns `Option<&Self::Item>` and never has to repeat `<Item>` anywhere, and the default `first` method can already be written against `Self::Item` even though no implementer exists yet.

> **Note:** You write `Self::Item` inside the trait and inside `impl` blocks. From *outside*, you name it as `<IntStack as Container>::Item` (the fully-qualified form) or, far more often, you never name it at all — you let inference do the work.

### Naming an associated type in a function bound

The everyday place a TypeScript developer meets associated-type syntax is in a generic bound. To say "any `Container` whose `Item` is `i32`," you write `Container<Item = i32>`:

```rust
trait Container {
    type Item;
    fn get(&self, index: usize) -> Option<&Self::Item>;
}

struct IntStack { items: Vec<i32> }
impl Container for IntStack {
    type Item = i32;
    fn get(&self, index: usize) -> Option<&i32> { self.items.get(index) }
}

// Accept any Container whose Item is i32.
fn print_first<C: Container<Item = i32>>(c: &C) {
    match c.get(0) {
        Some(x) => println!("first = {x}"),
        None => println!("empty"),
    }
}

// Fully generic over the Item too.
fn count_present<C: Container>(c: &C, upto: usize) -> usize {
    (0..upto).filter(|&i| c.get(i).is_some()).count()
}

fn main() {
    let s = IntStack { items: vec![7, 8, 9] };
    print_first(&s);                       // first = 7
    println!("{}", count_present(&s, 10)); // 3
}
```

Real output:

```text
first = 7
3
```

That `Container<Item = i32>` syntax looks like passing a generic argument, but the `Item =` part makes it clear you are *constraining the associated type*, not supplying a normal type parameter. You see it constantly with iterators. `impl Iterator<Item = i32>` means "some iterator that yields `i32`s":

```rust
fn sum_doubled(it: impl Iterator<Item = i32>) -> i32 {
    it.map(|x| x * 2).sum()
}

fn main() {
    let v = vec![1, 2, 3];
    // v.iter() yields &i32; .copied() turns it into an i32 iterator.
    println!("{}", sum_doubled(v.iter().copied())); // 12
}
```

Real output:

```text
12
```

### The canonical example: `Iterator`

Nearly every Rust program uses associated types, because `Iterator` is built on one:

```rust
struct Counter {
    count: u32,
    max: u32,
}

impl Iterator for Counter {
    type Item = u32; // each next() yields a u32

    fn next(&mut self) -> Option<Self::Item> {
        if self.count < self.max {
            self.count += 1;
            Some(self.count)
        } else {
            None
        }
    }
}

fn main() {
    let counter = Counter { count: 0, max: 5 };
    let collected: Vec<u32> = counter.collect();
    println!("{collected:?}"); // [1, 2, 3, 4, 5]

    // Because Item is fixed, the whole adapter chain knows the element type.
    let sum: u32 = Counter { count: 0, max: 5 }.map(|n| n * 2).sum();
    println!("{sum}"); // 30
}
```

Real output:

```text
[1, 2, 3, 4, 5]
30
```

Why is `Item` an associated type and not a generic parameter? Because a given iterator yields **exactly one** kind of element. A `Counter` always yields `u32`; it makes no sense for it to *also* be a `u32`-or-`String` iterator. Associated types encode "there is one right answer per type," which is exactly the situation here. (Contrast that with the `From<T>` trait, where one type can sensibly convert *from* many source types, so `From` uses a generic parameter, not an associated type.)

### Associated constants, line by line

```rust
trait Animal {
    const LEGS: u32 = 4; // default value
    fn legs(&self) -> u32 { Self::LEGS }
}
```

- `const LEGS: u32 = 4;` declares an **associated constant** with a default. Implementers may accept the default or override it with `const LEGS: u32 = 2;`.
- `Self::LEGS` references the implementer's value, the same way `Self::Item` references the implementer's type.
- An *inherent* associated const (like `Circle::PI` above) is even simpler: it is just a `const` written inside an `impl` block, namespaced under the type. It is reached as `Circle::PI`.

The standard library leans on associated constants for numeric limits: `i32::MAX`, `u8::MAX`, `f64::EPSILON`, `char::MAX`, and so on are all associated consts. In TypeScript the closest analog is `Number.MAX_SAFE_INTEGER`, a static property on a built-in, but Rust's version is generic-aware: a function bounded by a trait can write `T::MAX` and get the right constant for whatever `T` turns out to be.

---

## Key Differences

| Concept | TypeScript | Rust |
| --- | --- | --- |
| "Type the implementer picks" | Generic parameter: `interface Container<Item>` | Associated type: `type Item;` (idiomatic) **or** a generic parameter |
| How many choices per implementer | Many — a class can implement `Container<number>` and `Container<string>` | Exactly **one** for an associated type; many for a generic parameter |
| Caller has to name it? | Yes — `Container<number>` | No — inferred; named only when constraining, as `Container<Item = number>` |
| Runtime presence | Erased; purely compile-time | Resolved at compile time via monomorphization, but the chosen type is real and concrete |
| "Constant every implementer provides" | No first-class feature; use `static readonly` / module `const` | Associated constant: `const MAX: Self;` on a trait, or `const PI` in an `impl` |
| Famous standard-library use | `Array<T>`, `Map<K, V>` (generic params) | `Iterator::Item`, `i32::MAX` (associated type + const) |

### Associated type vs. generic parameter: the one-paragraph rule

Use an **associated type** when each implementing type has a *single, canonical* answer (an iterator's element type, a parser's output type, a repository's key type). Use a **generic trait parameter** when one type can reasonably implement the trait *many ways* (converting `From<i32>` and `From<&str>` into the same target). The compiler enforces this difference: an associated-type trait can be implemented for a type only once, whereas a generic trait can be implemented many times for the same type. The next two examples make that concrete.

A **generic** trait — implementable many times for one type:

```rust
trait Producer<Output> {
    fn produce(&self) -> Output;
}

struct Factory;

impl Producer<i32> for Factory {
    fn produce(&self) -> i32 { 1 }
}

impl Producer<String> for Factory {
    fn produce(&self) -> String { "x".to_string() }
}

fn main() {
    let f = Factory;
    let n: i32 = f.produce();
    let s: String = f.produce();
    println!("{n} {s}"); // 1 x
}
```

Real output:

```text
1 x
```

The equivalent **associated-type** trait *cannot* be implemented twice for `Factory`. See the first pitfall below for the exact compiler error.

---

## Common Pitfalls

### Pitfall 1: Trying to implement an associated-type trait twice for one type

A TypeScript developer used to `implements A<number>, A<string>` will reach for the same trick in Rust and hit a wall:

```rust
trait Producer {
    type Output;
    fn produce(&self) -> Self::Output;
}

struct Factory;

impl Producer for Factory {
    type Output = i32;
    fn produce(&self) -> i32 { 1 }
}

impl Producer for Factory { // does not compile (error[E0119])
    type Output = String;
    fn produce(&self) -> String { "x".to_string() }
}

fn main() {}
```

The real error:

```text
error[E0119]: conflicting implementations of trait `Producer` for type `Factory`
  --> src/main.rs:13:1
   |
 8 | impl Producer for Factory {
   | ------------------------- first implementation here
...
13 | impl Producer for Factory { // does not compile (error[E0119])
   | ^^^^^^^^^^^^^^^^^^^^^^^^^ conflicting implementation for `Factory`
```

> **Tip:** If you genuinely need many implementations for the same type, you wanted a *generic* trait (`trait Producer<Output>`), not an associated type. If you only ever need one, the associated type is the right, cleaner choice.

### Pitfall 2: Forgetting to fill in the associated type in the `impl`

The `type Item = ...;` line is mandatory; leaving it out is the same kind of error as forgetting to implement a method:

```rust
trait Container {
    type Item;
    fn get(&self) -> Option<&Self::Item>;
}

struct Bag { v: Vec<i32> }

impl Container for Bag { // does not compile (error[E0046])
    // forgot: type Item = i32;
    fn get(&self) -> Option<&i32> { self.v.first() }
}

fn main() {}
```

The real error:

```text
error[E0046]: not all trait items implemented, missing: `Item`
 --> src/main.rs:8:1
  |
2 |     type Item;
  |     --------- `Item` from trait
...
8 | impl Container for Bag { // does not compile (error[E0046])
  | ^^^^^^^^^^^^^^^^^^^^^^ missing `Item` in implementation
```

### Pitfall 3: Using a trait with an associated type as a trait object without naming the type

You can turn many traits into trait objects (`&dyn Trait`), but if the trait has an associated type, the object form is ambiguous until you pin the type down:

```rust
trait Container {
    type Item;
    fn get(&self) -> Option<&Self::Item>;
}

fn take(_c: &dyn Container) {} // does not compile (error[E0191])

fn main() {}
```

The real error tells you exactly how to fix it:

```text
error[E0191]: the value of the associated type `Item` in `Container` must be specified
 --> src/main.rs:6:18
  |
2 |     type Item;
  |     --------- `Item` defined here
...
6 | fn take(_c: &dyn Container) {} // does not compile (error[E0191])
  |                  ^^^^^^^^^ help: specify the associated type: `Container<Item = Type>`
```

The fix is `&dyn Container<Item = i32>` (or whatever concrete `Item` you mean). Trait objects and `dyn` are covered in [Section 09](/09-generics-traits/) and [Section 10 — Smart Pointers](/10-smart-pointers/).

### Pitfall 4: Expecting an associated const to be mutable or computed at runtime

`const` means *compile-time constant*, not "a field with a default." It is the same `const` you met in [Section 02](/02-basics/00-variables/): no heap allocation, no runtime initialization, evaluated at compile time. You cannot store a `String::from("...")` in an associated `const` (use `&'static str`), and you cannot reassign it. If you need per-instance, mutable data, that is a struct *field*, not an associated const. See [Structs](/06-data-structures/00-structs/).

---

## Best Practices

- **Default to an associated type when the answer is unique per implementer.** Iterator element types, parser output types, key/entity types in a storage abstraction all have one obvious answer per type, so an associated type keeps signatures clean and inference smooth.
- **Reach for a generic trait parameter only when multiple implementations make sense** for a single type (the `From<T>` situation). When in doubt, start with an associated type; it is the more constrained, easier-to-read default.
- **Name associated types in bounds with the `Trait<Assoc = T>` form**, e.g. `impl Iterator<Item = u32>`, rather than spelling out `<Concrete as Trait>::Item` unless you really need the fully-qualified syntax.
- **Use associated constants for type-level limits and labels** (`const MAX`, `const NAME`) so generic code can reference `T::MAX` or `T::NAME`. Prefer `&'static str` for string constants.
- **Lean on the standard library's conventions.** Implementing `Iterator` (with `type Item`) for your own types gives you `map`, `filter`, `collect`, `sum`, and the rest for free, exactly as shown above.
- This is an intentionally light tour. For when associated types pull their full weight (generic associated types, where-clauses involving `Self::Item`, and the design trade-offs against generic parameters), see [Section 09 — Generics and Traits](/09-generics-traits/).

---

## Real-World Example

A storage-layer abstraction is a textbook case for associated types and consts: every repository stores **one** kind of entity, keyed **one** way, in **one** named collection. Encoding those as a generic parameter would force every caller to repeat them; as associated items they are decided once, by the implementer, and generic code can stay blissfully unaware of the specifics.

```rust
use std::collections::HashMap;

// Each repository decides what it stores (Entity), how rows are keyed (Key),
// and the collection name (NAME). All three are implementer-chosen, so they
// are associated items rather than generic parameters.
trait Repository {
    type Key;
    type Entity;

    const NAME: &'static str; // associated const: the table/collection name

    fn insert(&mut self, key: Self::Key, entity: Self::Entity);
    fn find(&self, key: &Self::Key) -> Option<&Self::Entity>;
    fn count(&self) -> usize;
}

#[derive(Debug, Clone)]
struct User {
    name: String,
    email: String,
}

struct UserRepo {
    rows: HashMap<u64, User>,
}

impl UserRepo {
    fn new() -> Self {
        UserRepo { rows: HashMap::new() }
    }
}

impl Repository for UserRepo {
    type Key = u64;
    type Entity = User;

    const NAME: &'static str = "users";

    fn insert(&mut self, key: u64, entity: User) {
        self.rows.insert(key, entity);
    }
    fn find(&self, key: &u64) -> Option<&User> {
        self.rows.get(key)
    }
    fn count(&self) -> usize {
        self.rows.len()
    }
}

// Generic over ANY repository — names no concrete type, yet can read the
// associated const through the bound.
fn summarize<R: Repository>(repo: &R) -> String {
    format!("{} rows in '{}'", repo.count(), R::NAME)
}

fn main() {
    let mut users = UserRepo::new();
    users.insert(1, User { name: "Ada".into(), email: "ada@x.dev".into() });
    users.insert(2, User { name: "Linus".into(), email: "linus@x.dev".into() });

    println!("{}", summarize(&users)); // 2 rows in 'users'

    match users.find(&1) {
        Some(u) => println!("found {} <{}>", u.name, u.email),
        None => println!("missing"),
    }
    println!("missing key -> {:?}", users.find(&99));
}
```

Real output:

```text
2 rows in 'users'
found Ada <ada@x.dev>
missing key -> None
```

The `summarize` function is the heart of the example: it works for *every* repository, present and future, without knowing the key type, the entity type, or the collection name. It reads `R::NAME` (an associated const) and calls methods whose signatures use `Self::Key` and `Self::Entity` (associated types). A `ProductRepo` keyed by `String` and storing `Product` would slot in unchanged.

> **Note:** The `HashMap` used here comes from the standard collections. See [Section 07 — Collections](/07-collections/).

---

## Further Reading

### Official Documentation

- [The Rust Book - Advanced Traits: Associated Types](https://doc.rust-lang.org/book/ch20-02-advanced-traits.html#specifying-placeholder-types-in-trait-definitions-with-associated-types)
- [Rust by Example - Associated Types](https://doc.rust-lang.org/rust-by-example/generics/assoc_items/types.html)
- [Rust by Example - Associated Constants](https://doc.rust-lang.org/rust-by-example/custom_types/constants.html)
- [Rust Reference - Associated Items](https://doc.rust-lang.org/reference/items/associated-items.html)
- [`std::iter::Iterator`](https://doc.rust-lang.org/std/iter/trait.Iterator.html): the canonical associated-type trait

### Related Sections in This Guide

- [Methods and `impl` Blocks](/06-data-structures/05-impl-blocks/) — where associated types and consts are written
- [Associated Functions](/06-data-structures/06-associated-functions/) — `Self`, and methods without a receiver
- [Enums](/06-data-structures/02-enums/) and [Option Enum](/06-data-structures/03-option-enum/): the `Option<&Self::Item>` returns used throughout this file
- [Structs](/06-data-structures/00-structs/) — fields (per-instance, mutable data) vs. associated consts (type-level constants)
- [Generics and Traits](/09-generics-traits/) — the full treatment: associated types vs. generic parameters, GATs, and bounds
- [Smart Pointers](/10-smart-pointers/) — `dyn Trait` objects, including specifying associated types on them
- [Collections](/07-collections/) — `HashMap` and the iterators that rely on `type Item`
- [Variables and Mutability](/02-basics/00-variables/) — what `const` means in Rust

---

## Exercises

### Exercise 1: Give a trait an associated constant

**Difficulty:** Easy

**Objective:** Practice declaring and reading an associated constant.

**Instructions:** Add an associated constant `SIDES` to the `Shape` trait so the generic `describe` function below prints the right number of sides. Implement it for both `Triangle` (3) and `Square` (4).

```rust
trait Shape {
    // TODO: declare an associated const SIDES: u32
    fn name(&self) -> &'static str;
}

struct Triangle;
struct Square;

// TODO: impl Shape for Triangle and Square

fn describe<S: Shape>(s: &S) -> String {
    format!("a {} has {} sides", s.name(), /* ??? */)
}

fn main() {
    println!("{}", describe(&Triangle)); // a triangle has 3 sides
    println!("{}", describe(&Square));   // a square has 4 sides
}
```

<details>
<summary>Solution</summary>

```rust
trait Shape {
    const SIDES: u32;
    fn name(&self) -> &'static str;
}

struct Triangle;
struct Square;

impl Shape for Triangle {
    const SIDES: u32 = 3;
    fn name(&self) -> &'static str { "triangle" }
}
impl Shape for Square {
    const SIDES: u32 = 4;
    fn name(&self) -> &'static str { "square" }
}

fn describe<S: Shape>(s: &S) -> String {
    format!("a {} has {} sides", s.name(), S::SIDES)
}

fn main() {
    println!("{}", describe(&Triangle)); // a triangle has 3 sides
    println!("{}", describe(&Square));   // a square has 4 sides
}
```

Output:

```text
a triangle has 3 sides
a square has 4 sides
```

</details>

### Exercise 2: Implement `Iterator` with an associated type

**Difficulty:** Medium

**Objective:** Fill in the associated type and `next` method for a real standard-library trait.

**Instructions:** Make `Fib` an iterator that yields Fibonacci numbers. Set `type Item` correctly and implement `next` so that `fib.take(10).collect::<Vec<_>>()` produces the first ten Fibonacci numbers starting from 0.

```rust
struct Fib {
    a: u64,
    b: u64,
}

impl Iterator for Fib {
    // TODO: type Item = ?
    fn next(&mut self) -> Option<Self::Item> {
        // TODO: yield self.a, then advance the pair
    }
}

fn main() {
    let fib = Fib { a: 0, b: 1 };
    let first10: Vec<u64> = fib.take(10).collect();
    println!("{first10:?}"); // [0, 1, 1, 2, 3, 5, 8, 13, 21, 34]
}
```

<details>
<summary>Solution</summary>

```rust
struct Fib {
    a: u64,
    b: u64,
}

impl Iterator for Fib {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        let current = self.a;
        self.a = self.b;
        self.b = current + self.b;
        Some(current)
    }
}

fn main() {
    let fib = Fib { a: 0, b: 1 };
    let first10: Vec<u64> = fib.take(10).collect();
    println!("{first10:?}"); // [0, 1, 1, 2, 3, 5, 8, 13, 21, 34]
}
```

Output:

```text
[0, 1, 1, 2, 3, 5, 8, 13, 21, 34]
```

> **Note:** `Fib` never returns `None`, so it is an *infinite* iterator. That is fine: `take(10)` stops pulling after ten elements, which is exactly why lazy iterators are useful.

</details>

### Exercise 3: A parser trait with both an associated type and an associated const

**Difficulty:** Medium-Hard

**Objective:** Combine an associated type (the parse result) and an associated constant (a label) in one trait, then write generic code over it.

**Instructions:** Define a `Parser` trait with an associated `Output` type, an associated `LABEL: &'static str`, and a `parse(&self, &str) -> Result<Self::Output, String>` method. Implement it for `IntParser` (output `i64`, label `"integer"`) and `BoolParser` (output `bool`, label `"boolean"`). Then write a generic `run` function that returns `Result<P::Output, String>`. Errors should include the label.

<details>
<summary>Solution</summary>

```rust
trait Parser {
    type Output;
    const LABEL: &'static str;
    fn parse(&self, input: &str) -> Result<Self::Output, String>;
}

struct IntParser;
struct BoolParser;

impl Parser for IntParser {
    type Output = i64;
    const LABEL: &'static str = "integer";
    fn parse(&self, input: &str) -> Result<i64, String> {
        input
            .trim()
            .parse::<i64>()
            .map_err(|e| format!("{}: {e}", Self::LABEL))
    }
}

impl Parser for BoolParser {
    type Output = bool;
    const LABEL: &'static str = "boolean";
    fn parse(&self, input: &str) -> Result<bool, String> {
        match input.trim() {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            other => Err(format!("{}: not a bool: {other:?}", Self::LABEL)),
        }
    }
}

// Works for any Parser; the Output type is decided by the impl.
fn run<P: Parser>(p: &P, input: &str) -> Result<P::Output, String> {
    p.parse(input)
}

fn main() {
    println!("{:?}", run(&IntParser, "  42 ")); // Ok(42)
    println!("{:?}", run(&IntParser, "oops"));  // Err("integer: ...")
    println!("{:?}", run(&BoolParser, "true")); // Ok(true)
    println!("{:?}", run(&BoolParser, "maybe"));// Err("boolean: ...")
}
```

Output:

```text
Ok(42)
Err("integer: invalid digit found in string")
Ok(true)
Err("boolean: not a bool: \"maybe\"")
```

> **Tip:** `run`'s return type is `Result<P::Output, String>` — it names the associated type through the generic parameter `P` without ever mentioning `i64` or `bool`. That is associated types doing their job: the concrete type travels with the implementer. Error handling with `Result` is covered in [Section 08](/08-error-handling/).

</details>
