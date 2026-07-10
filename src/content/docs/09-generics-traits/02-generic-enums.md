---
title: "Generic Enums"
description: "Generic enums power Rust's Option and Result, the typed answer to TypeScript discriminated unions, with enforced exhaustiveness, ? support, and niche layout."
---

Enums that are parameterized over a type, like TypeScript's discriminated unions with generics. This is where Rust's two most important types — `Option<T>` and `Result<T, E>` — come from, so understanding generic enums is understanding the heart of the standard library.

---

## Quick Overview

A **generic enum** is an enum with one or more **type parameters**, so the same shape (`Some`/`None`, `Ok`/`Err`, a tree node, a cache slot) can hold any type you choose. If you have ever written a TypeScript discriminated union like `type Result<T, E> = { ok: true; value: T } | { ok: false; error: E }`, you already understand the idea. The difference is that Rust bakes this pattern into the language, makes the compiler force you to handle every case, and generates a separate, fully-optimized copy of the enum for each concrete type you use (**monomorphization**).

---

## TypeScript/JavaScript Example

In TypeScript you reach for a **discriminated union** (also called a tagged union) when a value can be in one of several shapes. Generics let you reuse that shape across types. A common one is a hand-rolled `Result` to avoid throwing exceptions:

```typescript
// A generic "result" union: success carries a T, failure carries an E.
type Result<T, E> =
  | { kind: "ok"; value: T }
  | { kind: "err"; error: E };

interface User {
  id: number;
  name: string;
}

interface ApiError {
  status: number;
  message: string;
}

function parseUser(json: string): Result<User, ApiError> {
  try {
    const data = JSON.parse(json) as User;
    return { kind: "ok", value: data };
  } catch {
    return { kind: "err", error: { status: 400, message: "invalid JSON" } };
  }
}

// The consumer must check the discriminant before touching the payload.
const result = parseUser('{"id":1,"name":"Ada"}');
if (result.kind === "ok") {
  console.log(`Loaded ${result.value.name}`); // Loaded Ada
} else {
  console.log(`Failed: ${result.error.message}`);
}
```

TypeScript's union is structural and runtime-erased: the `kind` field is a real property you check at runtime, and the `<T, E>` type parameters exist only at compile time: they vanish in the emitted JavaScript.

> **Note:** TypeScript and JavaScript also have a built-in optional pattern: `T | undefined` (or `T | null`). That is the closest analogue to Rust's `Option<T>`, but as you will see, the Rust version forces you to handle the absent case.

---

## Rust Equivalent

Rust spells the same idea with `enum` plus angle-bracket type parameters. Here is a generic `Either<L, R>` and a generic `Cache<T>` with methods:

```rust playground
// A generic enum with two type parameters.
#[derive(Debug)]
enum Either<L, R> {
    Left(L),
    Right(R),
}

// A generic enum with one type parameter, plus an inherent impl.
#[derive(Debug)]
enum Cache<T> {
    Empty,
    Loaded(T),
}

impl<T> Cache<T> {
    fn get(&self) -> Option<&T> {
        match self {
            Cache::Empty => None,
            Cache::Loaded(value) => Some(value),
        }
    }

    fn is_loaded(&self) -> bool {
        matches!(self, Cache::Loaded(_))
    }
}

fn main() {
    let a: Either<i32, String> = Either::Left(42);
    let b: Either<i32, String> = Either::Right(String::from("oops"));
    println!("{:?} {:?}", a, b); // Left(42) Right("oops")

    let c: Cache<String> = Cache::Loaded(String::from("data"));
    println!("loaded? {} value = {:?}", c.is_loaded(), c.get());
    // loaded? true value = Some("data")

    let empty: Cache<String> = Cache::Empty;
    println!("loaded? {} value = {:?}", empty.is_loaded(), empty.get());
    // loaded? false value = None
}
```

Real output:

```text
Left(42) Right("oops")
loaded? true value = Some("data")
loaded? false value = None
```

The standard library's `Option<T>` and `Result<T, E>` are *exactly* this pattern: generic enums that you will use constantly:

```rust
// These are (essentially) how the standard library defines them:
enum Option<T> {
    None,
    Some(T),
}

enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

You do not need to define `Option` or `Result` yourself. They are in the **prelude**, so `Some`, `None`, `Ok`, and `Err` are always in scope.

---

## Detailed Explanation

### Declaring the type parameter

```rust
enum Either<L, R> {
    Left(L),
    Right(R),
}
```

The `<L, R>` after the enum name introduces two type parameters. Each variant can use them: `Left` holds an `L`, `Right` holds an `R`. The names are conventional — single uppercase letters like `T`, `E`, `K`, `V`, `L`, `R` — but you can use a descriptive name like `Payload` if it reads better. Unlike TypeScript, where you write `type Either<L, R> = ...`, Rust uses the `enum` keyword and each variant is a real constructor (`Either::Left`, `Either::Right`).

### Methods need their own `<T>`

```rust
impl<T> Cache<T> {
    fn is_loaded(&self) -> bool { /* ... */ }
}
```

The `impl<T>` reads as: "for every type `T`, here are methods on `Cache<T>`." The `<T>` right after `impl` *declares* the parameter; the `<T>` in `Cache<T>` *uses* it. Forgetting the first one is a classic beginner error (see Common Pitfalls). This is covered in depth in [Generic Structs](/09-generics-traits/01-generic-structs/), and the same rule applies to enums.

### Pattern matching destructures the payload

```rust
match self {
    Cache::Empty => None,
    Cache::Loaded(value) => Some(value),
}
```

`match` is how you safely get the inner value out. In the `Cache::Loaded(value)` arm, `value` is bound to the `&T` inside (because we matched on `&self`). This is the same idea as checking `result.kind === "ok"` in TypeScript and *then* reading `result.value`, except the compiler **guarantees** you handled every variant. If you add a third variant later, every `match` that does not handle it stops compiling, a refactoring superpower TypeScript's `switch` cannot match unless you opt in with `never` checks. See [Control Flow](/04-control-flow/) for more on `match` and `if let`.

### Monomorphization: one copy per concrete type

This is the deepest difference from TypeScript. When you write `Cache<String>` and `Cache<i32>`, the Rust compiler generates two *separate*, fully-specialized enums — as if you had hand-written `CacheString` and `CacheI32`. There is no boxing, no tag-dispatch overhead, and the layout is optimal for each type. You can observe the distinct layouts:

```rust playground
use std::mem::size_of;

#[derive(Debug)]
enum Slot<T> {
    Empty,
    Full(T),
}

fn main() {
    println!("Slot<u8>  = {} bytes", size_of::<Slot<u8>>());
    println!("Slot<i64> = {} bytes", size_of::<Slot<i64>>());

    let a: Slot<u8> = Slot::Full(1);
    let b: Slot<i64> = Slot::Empty;
    println!("{:?} {:?}", a, b);
}
```

Real output:

```text
Slot<u8>  = 2 bytes
Slot<i64> = 16 bytes
Full(1) Empty
```

The two sizes are the point: `Slot<u8>` is 2 bytes (1 byte payload + 1 byte tag), while `Slot<i64>` is 16 bytes (8 bytes payload + padding + tag). TypeScript erases generics entirely, so there is only ever *one* runtime representation and the `<T>` is gone. Rust trades a little compile time and binary size for zero-cost, type-specialized code. Monomorphization is explained fully in [Generic Functions](/09-generics-traits/00-generic-functions/).

### The `?` operator works on generic enums

Because `Option<T>` and `Result<T, E>` are generic enums with a known shape, the `?` operator can short-circuit on them:

```rust playground
fn first_two(s: &str) -> Option<(char, char)> {
    let mut it = s.chars();
    let a = it.next()?; // if None, return None from first_two
    let b = it.next()?;
    Some((a, b))
}

fn parse_and_double(s: &str) -> Result<i32, std::num::ParseIntError> {
    let n: i32 = s.parse()?; // if Err, return that Err
    Ok(n * 2)
}

fn main() {
    println!("{:?}", first_two("hi"));         // Some(('h', 'i'))
    println!("{:?}", first_two("x"));          // None
    println!("{:?}", parse_and_double("21"));  // Ok(42)
    println!("{:?}", parse_and_double("no"));  // Err(ParseIntError { kind: InvalidDigit })
}
```

Real output:

```text
Some(('h', 'i'))
None
Ok(42)
Err(ParseIntError { kind: InvalidDigit })
```

`?` is roughly TypeScript's optional chaining `?.` combined with early-return-on-error, but it is built on these generic enums. Error handling with `Result` and `?` is the subject of [Error Handling](/08-error-handling/).

---

## Key Differences

| Aspect | TypeScript discriminated union | Rust generic enum |
| ------ | ------------------------------ | ----------------- |
| Syntax | `type U<T> = { kind: "a"; ... } \| ...` | `enum U<T> { A(T), ... }` |
| Discriminant | A real runtime field you choose (`kind`) | A hidden, compiler-managed tag |
| Generics at runtime | Erased — one shared representation | Monomorphized — one specialized copy per type |
| Exhaustiveness | Opt-in (`never` trick / `switch` default) | Enforced — non-exhaustive `match` is a compile error |
| Optional value | `T \| undefined` | `Option<T>` (`Some`/`None`) |
| Fallible value | hand-rolled union or `throw` | `Result<T, E>` (`Ok`/`Err`) |
| Memory layout | uniform (boxed object) | sized exactly for `T`, with niche optimization |

### Null safety is a generic enum, not a special case

TypeScript bolts optionality onto every type with `T | undefined`, and `strictNullChecks` is what makes you check it. Rust has **no** `null`. Absence is modeled with the ordinary generic enum `Option<T>`, and the type system makes the absent case impossible to ignore: you cannot read the `T` out of an `Option<T>` without first dealing with `None`.

### Niche optimization

Because the compiler controls the tag, it can be clever. For types that have an impossible bit pattern (a "niche"), `Option` reuses it instead of adding a separate tag. A reference or `Box` can never be null, so `Option<&T>` and `Option<Box<T>>` use the all-zero pointer as `None` and are the **same size** as the bare pointer:

```rust playground
use std::mem::size_of;

fn main() {
    println!("&i32         = {} bytes", size_of::<&i32>());
    println!("Option<&i32> = {} bytes", size_of::<Option<&i32>>());
    println!("Box<i32>         = {} bytes", size_of::<Box<i32>>());
    println!("Option<Box<i32>> = {} bytes", size_of::<Option<Box<i32>>>());
    println!("i32          = {} bytes", size_of::<i32>());
    println!("Option<i32>  = {} bytes", size_of::<Option<i32>>());
}
```

Real output:

```text
&i32         = 8 bytes
Option<&i32> = 8 bytes
Box<i32>         = 8 bytes
Option<Box<i32>> = 8 bytes
i32          = 4 bytes
Option<i32>  = 8 bytes
```

`Option<&i32>` costs nothing extra, while `Option<i32>` needs a separate discriminant byte (rounded up to 8 for alignment) because every `i32` bit pattern is a valid value. TypeScript's `number | undefined` has no such trick; `undefined` is just another runtime value. `Box<T>` and other smart pointers are covered in [Smart Pointers](/10-smart-pointers/).

---

## Common Pitfalls

### Pitfall 1: Forgetting the type annotation on a bare variant

A variant with no payload (`None`, `Cache::Empty`) gives the compiler nothing to infer `T` from:

```rust
fn main() {
    let cache = None; // does not compile (error[E0282]: type annotations needed)
    println!("{:?}", cache);
}
```

Real compiler error:

```text
error[E0282]: type annotations needed for `Option<_>`
 --> src/main.rs:3:9
  |
3 |     let cache = None;       // what is T?
  |         ^^^^^   ---- type must be known at this point
  |
help: consider giving `cache` an explicit type, where the type for type parameter `T` is specified
  |
3 |     let cache: Option<T> = None;       // what is T?
  |              +++++++++++
```

**Fix:** annotate the binding (`let cache: Option<i32> = None;`) or use the **turbofish** on a function that returns it. Coming from TypeScript this surprises people, because `const x = undefined` is always fine there, but TypeScript widens it to `undefined`/`any`, whereas Rust refuses to guess `T`.

### Pitfall 2: Non-exhaustive `match`

Forgetting a variant is a hard error, not a warning:

```rust
enum Either<L, R> {
    Left(L),
    Right(R),
}

fn describe(e: Either<i32, String>) -> String {
    match e {
        Either::Left(n) => format!("number {}", n),
        // does not compile (error[E0004]): forgot Either::Right
    }
}
```

Real compiler error:

```text
error[E0004]: non-exhaustive patterns: `Either::Right(_)` not covered
 --> src/main.rs:7:11
  |
7 |     match e {
  |           ^ pattern `Either::Right(_)` not covered
  |
note: `Either<i32, String>` defined here
 --> src/main.rs:1:6
  |
1 | enum Either<L, R> {
  |      ^^^^^^
2 |     Left(L),
3 |     Right(R),
  |     ----- not covered
  = note: the matched value is of type `Either<i32, String>`
```

This is a feature: add a variant later and the compiler lists every place that needs updating. To intentionally ignore the rest, add `_ => ...`.

### Pitfall 3: Forgetting `<T>` after `impl`

```rust
enum Maybe<T> {
    Just(T),
    Nothing,
}

impl Maybe<T> { // does not compile (error[E0412]: cannot find type `T`)
    fn is_just(&self) -> bool {
        matches!(self, Maybe::Just(_))
    }
}
```

Real compiler error:

```text
error[E0412]: cannot find type `T` in this scope
 --> src/main.rs:6:12
  |
6 | impl Maybe<T> {            // forgot the <T> after impl
  |            ^ not found in this scope
  |
help: you might be missing a type parameter
  |
6 | impl<T> Maybe<T> {            // forgot the <T> after impl
  |     +++
```

**Fix:** write `impl<T> Maybe<T>`. The first `<T>` declares the parameter, the second uses it.

### Pitfall 4: Reaching for `null`/`undefined` habits

TypeScript developers often look for "the empty value." There is no `null` in safe Rust. Model absence with `Option<T>` and a missing-but-recoverable failure with `Result<T, E>`. Trying to leave a field "unset" by some other means usually means you actually wanted `Option<T>`.

---

## Best Practices

- **Reach for `Option<T>` and `Result<T, E>` first.** Do not invent your own two-state enums for "maybe a value" or "value or error": the standard ones come with dozens of combinators (`map`, `and_then`, `unwrap_or`, `ok_or`, `?`) and integrate with the whole ecosystem.
- **Derive the traits you need.** `#[derive(Debug)]` is almost always worth adding so you can `println!("{:?}", value)`. Add `Clone`, `PartialEq`, etc. as needed.
- **Name your own domain enums, even when `Either` would work.** `Either<L, R>` is generic and meaningless to a reader; a `Payment { Card(CardInfo), Cash(Money) }` enum documents intent. Use generics on enums when the *container* logic is reusable, not to avoid naming a domain concept.
- **Provide constructors and combinators in an `impl<T>` block** rather than forcing callers to `match` everywhere. A `map`/`get`/`unwrap_or` method centralizes the pattern.
- **Bound type parameters where the method needs it, not on the enum.** Put `where T: Clone` on the specific method's `impl`, so that constructing the enum stays unconstrained. See [Trait Bounds](/09-generics-traits/05-trait-bounds/).
- **Box recursive variants.** A self-referential enum like a tree or linked list must put the recursive part behind a pointer (`Box<Tree<T>>`); otherwise it would have infinite size. See the Real-World Example below and [Smart Pointers](/10-smart-pointers/).

---

## Real-World Example

A production-flavored use of a generic enum: a `Fetch<T, E>` state machine, the kind you would model a data-fetching hook around (idle → loading → success/failure). It is generic so the same machine works for any payload and error type.

```rust playground
use std::fmt;

/// The lifecycle of an asynchronous request, generic over its payload `T`
/// and error `E`. Mirrors the states a frontend data hook moves through.
#[derive(Debug, Clone, PartialEq)]
enum Fetch<T, E> {
    Idle,
    Loading,
    Success(T),
    Failure(E),
}

impl<T, E> Fetch<T, E> {
    /// A request is "terminal" once it has succeeded or failed.
    fn is_terminal(&self) -> bool {
        matches!(self, Fetch::Success(_) | Fetch::Failure(_))
    }

    /// Transform the success payload, leaving the other states untouched.
    fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Fetch<U, E> {
        match self {
            Fetch::Idle => Fetch::Idle,
            Fetch::Loading => Fetch::Loading,
            Fetch::Success(value) => Fetch::Success(f(value)),
            Fetch::Failure(err) => Fetch::Failure(err),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct User {
    id: u32,
    name: String,
}

#[derive(Debug, Clone, PartialEq)]
struct ApiError {
    status: u16,
    message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP {}: {}", self.status, self.message)
    }
}

fn render(state: &Fetch<User, ApiError>) -> String {
    match state {
        Fetch::Idle => "Click to load".to_string(),
        Fetch::Loading => "Loading...".to_string(),
        Fetch::Success(user) => format!("Welcome, {} (#{})", user.name, user.id),
        Fetch::Failure(err) => format!("Error: {}", err),
    }
}

fn main() {
    let mut state: Fetch<User, ApiError> = Fetch::Idle;
    println!("{}", render(&state)); // Click to load

    state = Fetch::Loading;
    println!("{}", render(&state)); // Loading...
    println!("terminal? {}", state.is_terminal()); // false

    state = Fetch::Success(User { id: 7, name: "Ada".to_string() });
    println!("{}", render(&state)); // Welcome, Ada (#7)
    println!("terminal? {}", state.is_terminal()); // true

    // Derive a display-name view without losing the state machine's shape.
    let display: Fetch<String, ApiError> = state.clone().map(|u| u.name.to_uppercase());
    println!("{:?}", display); // Success("ADA")

    let failed: Fetch<User, ApiError> =
        Fetch::Failure(ApiError { status: 404, message: "not found".to_string() });
    println!("{}", render(&failed)); // Error: HTTP 404: not found
}
```

Real output:

```text
Click to load
Loading...
terminal? false
Welcome, Ada (#7)
terminal? true
Success("ADA")
Error: HTTP 404: not found
```

Note the `map` method's own type parameters `<U, F: FnOnce(T) -> U>`: it converts a `Fetch<T, E>` into a `Fetch<U, E>` while preserving the error type, the same signature shape as `Option::map` and `Result::map`.

---

## Further Reading

### Official documentation

- [The Rust Book — Generic Data Types (enums)](https://doc.rust-lang.org/book/ch10-01-syntax.html#in-enum-definitions)
- [The Rust Book — Defining an Enum (`Option`)](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html)
- [`std::option::Option` API docs](https://doc.rust-lang.org/std/option/enum.Option.html)
- [`std::result::Result` API docs](https://doc.rust-lang.org/std/result/enum.Result.html)
- [Rust Reference — Type layout & niche optimization](https://doc.rust-lang.org/reference/type-layout.html)

### Related sections in this guide

- [Generic Functions](/09-generics-traits/00-generic-functions/): type parameters on functions, monomorphization, the turbofish `::<>`
- [Generic Structs](/09-generics-traits/01-generic-structs/): the same `<T>` machinery on structs, with multiple parameters and constrained impls
- [Trait Bounds](/09-generics-traits/05-trait-bounds/): restricting a type parameter with `T: Trait` and `where` clauses
- [Traits](/09-generics-traits/03-traits/): TypeScript interfaces become Rust traits
- [Control Flow](/04-control-flow/) — `match`, `if let`, and exhaustiveness
- [Error Handling](/08-error-handling/) — `Result`, the `?` operator, and error types in depth
- [Smart Pointers](/10-smart-pointers/) — `Box<T>` for recursive enums and heap allocation
- [Section 09 overview](/09-generics-traits/)

---

## Exercises

### Exercise 1: A generic singly linked list

**Difficulty:** Easy

**Objective:** Define a recursive generic enum and write methods over it.

**Instructions:**

1. Define `enum List<T> { Cons(T, Box<List<T>>), Nil }`.
2. Add `impl<T> List<T>` with `new()` (returns `Nil`), `push(self, value: T) -> Self` (prepends a value), and `len(&self) -> usize` (counts elements recursively).
3. In `main`, build a list of three numbers and print its length, then build a list of `&str` to prove it is generic.

```rust
#[derive(Debug)]
enum List<T> {
    Cons(T, Box<List<T>>),
    Nil,
}

impl<T> List<T> {
    fn new() -> Self {
        /* ??? */
    }
    fn push(self, value: T) -> Self {
        /* ??? */
    }
    fn len(&self) -> usize {
        // TODO: match on self
    }
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug)]
enum List<T> {
    Cons(T, Box<List<T>>),
    Nil,
}

impl<T> List<T> {
    fn new() -> Self {
        List::Nil
    }

    fn push(self, value: T) -> Self {
        // Prepend by wrapping the existing list as the tail.
        List::Cons(value, Box::new(self))
    }

    fn len(&self) -> usize {
        match self {
            List::Nil => 0,
            List::Cons(_, rest) => 1 + rest.len(),
        }
    }
}

fn main() {
    let list = List::new().push(1).push(2).push(3);
    println!("len = {}", list.len()); // len = 3
    println!("{:?}", list); // Cons(3, Cons(2, Cons(1, Nil)))

    let words: List<&str> = List::new().push("a").push("b");
    println!("len = {}", words.len()); // len = 2
}
```

Real output:

```text
len = 3
Cons(3, Cons(2, Cons(1, Nil)))
len = 2
```

The recursive variant must be wrapped in `Box` so the enum has a known, finite size.

</details>

### Exercise 2: `Either` combinators and conversion to `Result`

**Difficulty:** Medium

**Objective:** Add `map`-style combinators to a generic two-parameter enum and convert it into a standard `Result`.

**Instructions:**

1. Define `enum Either<L, R> { Left(L), Right(R) }` (derive `Debug` and `PartialEq`).
2. Add `map_right<R2, F: FnOnce(R) -> R2>(self, f: F) -> Either<L, R2>` that transforms only the `Right` payload.
3. Add `into_result(self) -> Result<R, L>` treating `Left` as the error and `Right` as success.
4. Demonstrate both in `main`.

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug, PartialEq)]
enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    fn map_right<R2, F: FnOnce(R) -> R2>(self, f: F) -> Either<L, R2> {
        match self {
            Either::Left(l) => Either::Left(l),
            Either::Right(r) => Either::Right(f(r)),
        }
    }

    fn into_result(self) -> Result<R, L> {
        match self {
            Either::Left(l) => Err(l),
            Either::Right(r) => Ok(r),
        }
    }
}

fn main() {
    let ok: Either<String, i32> = Either::Right(21);
    let mapped = ok.map_right(|n| n * 2);
    assert_eq!(mapped, Either::Right(42));
    println!("{:?}", mapped); // Right(42)

    let err: Either<String, i32> = Either::Left("boom".to_string());
    println!("{:?}", err.into_result()); // Err("boom")

    let good: Either<String, i32> = Either::Right(7);
    println!("{:?}", good.into_result()); // Ok(7)
}
```

Real output:

```text
Right(42)
Err("boom")
Ok(7)
```

Note how `map_right` changes only the second type parameter (`R` to `R2`) while leaving `L` alone, the same trick `Result::map` uses.

</details>

### Exercise 3: A mappable, summable binary tree

**Difficulty:** Hard

**Objective:** Write a generic recursive enum with a structure-preserving `map` and a `sum` that requires a trait bound on the method.

**Instructions:**

1. Define `enum Tree<T> { Leaf(T), Node(Box<Tree<T>>, Box<Tree<T>>) }`.
2. Add `map<U, F: Fn(&T) -> U>(&self, f: &F) -> Tree<U>` producing a new tree of the same shape with every leaf transformed.
3. Add `sum(&self) -> T` that only compiles when `T: Add<Output = T> + Copy` (use a `where` clause on the method, not the enum).
4. Build a tree of `i32`, print its sum, map it to doubled values and to `String` labels.

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug)]
enum Tree<T> {
    Leaf(T),
    Node(Box<Tree<T>>, Box<Tree<T>>),
}

impl<T> Tree<T> {
    // Produce a new tree of the same shape with every leaf transformed.
    fn map<U, F: Fn(&T) -> U>(&self, f: &F) -> Tree<U> {
        match self {
            Tree::Leaf(value) => Tree::Leaf(f(value)),
            Tree::Node(left, right) => {
                Tree::Node(Box::new(left.map(f)), Box::new(right.map(f)))
            }
        }
    }

    // The bound lives on the method, so building a Tree<String> stays unconstrained.
    fn sum(&self) -> T
    where
        T: std::ops::Add<Output = T> + Copy,
    {
        match self {
            Tree::Leaf(value) => *value,
            Tree::Node(left, right) => left.sum() + right.sum(),
        }
    }
}

fn main() {
    let tree: Tree<i32> = Tree::Node(
        Box::new(Tree::Leaf(1)),
        Box::new(Tree::Node(Box::new(Tree::Leaf(2)), Box::new(Tree::Leaf(3)))),
    );
    println!("sum = {}", tree.sum()); // sum = 6

    let doubled = tree.map(&|n| n * 2);
    println!("{:?}", doubled); // Node(Leaf(2), Node(Leaf(4), Leaf(6)))
    println!("doubled sum = {}", doubled.sum()); // doubled sum = 12

    let labels: Tree<String> = tree.map(&|n| format!("leaf-{}", n));
    println!("{:?}", labels); // Node(Leaf("leaf-1"), Node(Leaf("leaf-2"), Leaf("leaf-3")))
}
```

Real output:

```text
sum = 6
Node(Leaf(2), Node(Leaf(4), Leaf(6)))
doubled sum = 12
Node(Leaf("leaf-1"), Node(Leaf("leaf-2"), Leaf("leaf-3")))
```

Putting `T: Add<Output = T> + Copy` on `sum` rather than on the enum keeps `Tree<String>` valid (strings are not summable, but we never call `sum` on them). Trait bounds are covered in [Trait Bounds](/09-generics-traits/05-trait-bounds/), and the `Add` trait in [Operator Overloading](/09-generics-traits/10-operator-overloading/).

</details>
