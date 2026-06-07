---
title: "`impl Trait`: Anonymous Types in Arguments and Returns"
description: "impl Trait names a type by capability in Rust function arguments and returns, the static-dispatch counterpart to a TypeScript interface."
---

`impl Trait` is Rust's shorthand for "some type that satisfies this trait, but I am not going to name it." You meet it in two places, function arguments and function return types, and although the syntax looks identical, the two positions mean genuinely different things. This file untangles both, plus the newer return-position `impl Trait` in traits (RPITIT).

---

## Quick Overview

`impl Trait` lets you write a function signature in terms of a **capability** instead of a concrete type. In **argument position** it is concise sugar for a generic with a trait bound; in **return position** (often abbreviated **RPIT**, "return-position `impl Trait`") it hands the caller a value of a single hidden concrete type, perfect for returning closures and iterator chains whose real type names are unwriteable. For a TypeScript developer this feels a little like annotating a parameter or return as an `interface`, but the runtime story is the opposite: there is no boxing, no vtable, and no type erasure.

> **Note:** This page assumes you have met traits ([Traits](/09-generics-traits/03-traits/)) and trait bounds ([Trait Bounds](/09-generics-traits/05-trait-bounds/)). The bound `T: Trait` and the shorthand `impl Trait` are two spellings of overlapping ideas; here we focus specifically on the `impl Trait` spelling and when each position is the right tool.

---

## TypeScript/JavaScript Example

Here is a familiar pattern: a function constrained to "anything that can summarize itself," and a factory that returns such a thing. In TypeScript the natural way to express both is with an `interface`.

```typescript
// TypeScript - constrain a parameter and a return by an interface
interface Summary {
  summarize(): string;
}

class Article implements Summary {
  constructor(
    public title: string,
    public author: string,
  ) {}

  summarize(): string {
    return `${this.title} by ${this.author}`;
  }
}

// Parameter typed by the interface: accepts any conforming object.
function announce(item: Summary): void {
  console.log(`Breaking! ${item.summarize()}`);
}

// Return typed by the interface: the caller sees only `Summary`,
// even though we hand back a concrete `Article`.
function defaultSummary(): Summary {
  return new Article("Rust 1.96 released", "The Rust Team");
}

announce(new Article("Rust 1.96 released", "The Rust Team"));
const s = defaultSummary();
console.log(s.summarize());
```

**Real output (Node v22, run with `tsx`):**

```text
Breaking! Rust 1.96 released by The Rust Team
Rust 1.96 released by The Rust Team
```

**Key points to carry into the Rust version:**

- The parameter `item: Summary` accepts _any_ object whose shape matches (TypeScript is structural). At runtime the object is the same heap object it always was; interface annotations are erased.
- The return type `Summary` **widens** the value: callers may only call interface methods, even though an `Article` came back. TypeScript happily returns different concrete classes from different branches of one such function, because at runtime they are all just objects.

Both of those behaviors differ from Rust in instructive ways, as we will see.

---

## Rust Equivalent

```rust
trait Summary {
    fn summarize(&self) -> String;
}

struct Article {
    title: String,
    author: String,
}

struct Tweet {
    username: String,
    content: String,
}

impl Summary for Article {
    fn summarize(&self) -> String {
        format!("{} by {}", self.title, self.author)
    }
}

impl Summary for Tweet {
    fn summarize(&self) -> String {
        format!("@{}: {}", self.username, self.content)
    }
}

// `impl Trait` in ARGUMENT position: accept any single type that is `Summary`.
fn announce(item: &impl Summary) {
    println!("Breaking! {}", item.summarize());
}

// `impl Trait` in RETURN position (RPIT): return *some* `Summary`,
// without naming the concrete type at the call site.
fn default_tweet() -> impl Summary {
    Tweet {
        username: String::from("rustlang"),
        content: String::from("We just shipped a new release!"),
    }
}

fn main() {
    let article = Article {
        title: String::from("Rust 1.96 released"),
        author: String::from("The Rust Team"),
    };
    announce(&article);

    let t = default_tweet();
    announce(&t);
    println!("{}", t.summarize());
}
```

**Real output:**

```text
Breaking! Rust 1.96 released by The Rust Team
Breaking! @rustlang: We just shipped a new release!
@rustlang: We just shipped a new release!
```

> **Note:** The current stable toolchain is **Rust 1.96.0** on the latest stable edition (2024). Create projects with `cargo new` (it auto-selects the newest edition) and never pin an older one. Everything in this file is verified on a 2024-edition project.

---

## Detailed Explanation

### Argument position: `&impl Summary` is sugar for a generic

```rust
fn announce(item: &impl Summary) {
    println!("Breaking! {}", item.summarize());
}
```

Read `&impl Summary` as "a borrow of some type that implements `Summary`." This is **exactly equivalent** to the generic form:

```rust
fn announce<T: Summary>(item: &T) {
    println!("Breaking! {}", item.summarize());
}
```

The compiler treats both identically: each call site is **monomorphized** into a specialized copy of `announce` for the concrete type passed in. (Monomorphization — Rust stamping out one machine-code copy per concrete type — is the topic of [Generic Functions](/09-generics-traits/00-generic-functions/), and it is the deep contrast with TypeScript's erased generics.) The `impl Trait` form is purely a convenience: it saves you from inventing a name like `T` when you only mention the type once. It is sometimes called "anonymous generic" for that reason.

There is one capability you give up by using the anonymous form, which becomes the source of a common pitfall: you cannot **name** the type, so two parameters written `impl Summary` are two _independent_ types, and you cannot turbofish them. More on that in [Common Pitfalls](#common-pitfalls).

### Return position (RPIT): one hidden concrete type

```rust
fn default_tweet() -> impl Summary {
    Tweet { /* ... */ }
}
```

Return position is where `impl Trait` earns its keep, and it is **not** the same as the argument case. Here `impl Summary` means: "this function returns a value of _one specific, compiler-determined_ concrete type, and all you, the caller, may assume is that it implements `Summary`." The concrete type (`Tweet`) is fixed at compile time and inferred from the function body; it is just hidden from the signature.

This is the central contrast with the TypeScript return-by-interface version:

- In TypeScript, `function defaultSummary(): Summary` may return an `Article` from one branch and a `Tweet` from another; at runtime they are interchangeable objects.
- In Rust, `-> impl Summary` locks in **a single** concrete type for the whole function. Every `return` path must produce that same type, or the program does not compile (see Pitfall 1).

Why hide the type at all? Because some real return types are effectively impossible to write by hand: closures have unnameable anonymous types, and iterator adapters produce deeply nested generic types like `Map<Filter<Range<u32>, {closure}>, {closure}>`. `impl Trait` lets you describe such a value by what it _does_ rather than what it _is_:

```rust
fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n
}

fn even_squares(limit: u32) -> impl Iterator<Item = u32> {
    (0..limit).filter(|x| x % 2 == 0).map(|x| x * x)
}

fn main() {
    let add5 = make_adder(5);
    println!("{}", add5(10));

    let squares: Vec<u32> = even_squares(6).collect();
    println!("{:?}", squares);
}
```

**Real output:**

```text
15
[0, 4, 16]
```

A closure has a unique, compiler-generated type with no name you could type into a signature; `impl Fn(i32) -> i32` is how you return one. The iterator chain's true type is `Map<Filter<Range<u32>, ...>, ...>`, and you would not want to maintain that by hand. `impl Iterator<Item = u32>` says everything the caller needs.

### Static dispatch, no boxing

Both positions produce **static dispatch**: the call goes directly to the right machine code with no runtime lookup, and the returned value lives inline (on the stack, or moved into wherever the caller puts it) with **no heap allocation and no vtable**. This is the opposite of the TypeScript story, where every interface-typed value is a heap object accessed through dynamic property lookup. When you _do_ want runtime polymorphism (a heterogeneous collection, or different return types per branch), you reach for `Box<dyn Trait>` instead, which is covered in [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/) and previewed in the next section.

### A brief note on RPITIT (`impl Trait` in trait methods)

Until recently you could not write `-> impl Trait` as the return type of a method _inside a trait definition_. Since **Rust 1.75** you can; the feature is called **return-position `impl Trait` in traits**, or **RPITIT**:

```rust
trait Container {
    // The method returns "some iterator of i32" without naming the type.
    fn items(&self) -> impl Iterator<Item = i32>;
}

struct Evens {
    upto: i32,
}

impl Container for Evens {
    fn items(&self) -> impl Iterator<Item = i32> {
        (0..self.upto).filter(|n| n % 2 == 0)
    }
}

fn main() {
    let e = Evens { upto: 8 };
    let collected: Vec<i32> = e.items().collect();
    println!("{:?}", collected);
}
```

**Real output:**

```text
[0, 2, 4, 6]
```

Before RPITIT, this pattern required either an associated type (`type Iter: Iterator<Item = i32>;`, which forces every implementor to name a concrete iterator type) or the `async-trait`-style boxing dance. RPITIT removes that boilerplate. The one big caveat: a trait that uses RPITIT is **not `dyn`-compatible** (you cannot make a `Box<dyn Container>` from it), because the hidden return type would vary per implementor and so the compiler cannot build a single vtable. We capture that exact error in the pitfalls.

> **Note:** RPITIT is closely related to native `async fn` in traits (also stable since 1.75): an `async fn` in a trait desugars to a method returning `impl Future`. That is why the modern advice is "you do not need the `async-trait` crate just to write an async method in a trait — only reach for it when you need `dyn`." Async traits are covered in [Section 11: Async](/11-async/).

---

## Key Differences

| Concept | TypeScript | Rust `impl Trait` |
| --- | --- | --- |
| Parameter by capability | `item: SomeInterface` | `item: impl Trait` (sugar for `<T: Trait>`) |
| Return by capability | `(): SomeInterface` (widening) | `-> impl Trait` (one hidden concrete type) |
| Different concrete types per branch in a return | Allowed (all are objects) | **Not** allowed for one `-> impl Trait` |
| Dispatch | Always dynamic (property lookup) | Static — direct call, no vtable |
| Allocation | Heap object | Inline value, no heap, no boxing |
| Naming the type | The interface _is_ the type | The type is anonymous/hidden |
| In a struct field | Fine (`field: SomeInterface`) | **Not allowed** (use a generic param or `Box<dyn>`) |

### "Some type" (argument) versus "one hidden type" (return)

The single most important mental model: in **argument position**, `impl Trait` means the _caller_ chooses the type, so the function must work for _any_ implementor. In **return position**, the _function_ chooses the type, so there is _exactly one_, fixed by the body and merely hidden from the signature. The same three words flip who is in control. The TypeScript intuition — "interface in, interface out, both just shapes" — does not capture this asymmetry.

### `impl Trait` is not a trait object

A TypeScript developer often reads `-> impl Summary` as "returns a `Summary` interface value," analogous to a polymorphic object. It is not. `impl Trait` is resolved entirely at compile time to one concrete type; it never erases the type or introduces indirection. The runtime polymorphism analog of a TypeScript interface-typed value is `Box<dyn Summary>` (heap-allocated, vtable-dispatched), not `impl Summary`. Keep the two firmly apart: confusing them is the root of most early mistakes.

### Edition 2024 captures lifetimes for you

On the 2024 edition, a return-position `impl Trait` automatically captures any in-scope lifetimes, so returning an iterator that borrows an argument "just works" without the older `+ '_` annotation:

```rust
// On edition 2024, the returned iterator may borrow from `data` with no
// extra lifetime annotation needed.
fn first_words(data: &[String]) -> impl Iterator<Item = &str> {
    data.iter().map(|s| s.split(' ').next().unwrap_or(""))
}

fn main() {
    let lines = vec!["hello world".to_string(), "foo bar".to_string()];
    let firsts: Vec<&str> = first_words(&lines).collect();
    println!("{:?}", firsts);
}
```

**Real output:**

```text
["hello", "foo"]
```

> **Tip:** If you read older tutorials that add `+ '_` or `+ 'a` to `impl Trait` return types to make borrowing compile, that ceremony is usually unnecessary on the 2024 edition. Use `cargo new` so you are always on the newest edition and get this behavior by default.

---

## Common Pitfalls

### Pitfall 1: Returning two different concrete types from one `-> impl Trait`

This is the trap that TypeScript habits walk straight into. Because both `Rev<IntoIter<…>>` and `IntoIter<…>` "are iterators," it feels like you should be able to return either branch, exactly as a TypeScript function returning `Summary` could return an `Article` or a `Tweet`. Rust refuses: `-> impl Trait` is _one_ hidden type, not a union.

```rust
// does not compile (error[E0308]: `if` and `else` have incompatible types)
fn make_iter(reverse: bool) -> impl Iterator<Item = i32> {
    let v = vec![1, 2, 3];
    if reverse {
        v.into_iter().rev()   // type: Rev<IntoIter<i32>>
    } else {
        v.into_iter()         // type: IntoIter<i32> -- a DIFFERENT type
    }
}

fn main() {
    for x in make_iter(true) {
        println!("{x}");
    }
}
```

Real compiler error:

```text
error[E0308]: `if` and `else` have incompatible types
 --> src/main.rs:7:9
  |
4 | /     if reverse {
5 | |         v.into_iter().rev()
  | |         ------------------- expected because of this
6 | |     } else {
7 | |         v.into_iter()
  | |         ^^^^^^^^^^^^^ expected `Rev<IntoIter<{integer}>>`, found `IntoIter<{integer}>`
8 | |     }
  | |_____- `if` and `else` have incompatible types
  |
  = note: expected struct `Rev<std::vec::IntoIter<_>>`
             found struct `std::vec::IntoIter<_>`
help: you could change the return type to be a boxed trait object
  |
2 - fn make_iter(reverse: bool) -> impl Iterator<Item = i32> {
2 + fn make_iter(reverse: bool) -> Box<dyn Iterator<Item = i32>> {
  |
help: if you change the return type to expect trait objects, box the returned expressions
  |
5 ~         Box::new(v.into_iter().rev())
6 |     } else {
7 ~         Box::new(v.into_iter())
  |
```

**Fix:** when branches genuinely return different concrete types, switch to a trait object, `Box<dyn Trait>`, which _does_ allow runtime variation (at the cost of one heap allocation and dynamic dispatch). The compiler even spells out the edit:

```rust
fn make_iter(reverse: bool) -> Box<dyn Iterator<Item = i32>> {
    let v = vec![1, 2, 3];
    if reverse {
        Box::new(v.into_iter().rev())
    } else {
        Box::new(v.into_iter())
    }
}

fn main() {
    let collected: Vec<i32> = make_iter(true).collect();
    println!("{:?}", collected);
}
```

**Real output:**

```text
[3, 2, 1]
```

See [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/) for the full static-vs-dynamic trade-off.

### Pitfall 2: Trying to turbofish or unify two `impl Trait` arguments

Two parameters both written `impl Trait` are two _independent_ anonymous type parameters (they need not be the same type), and because the type has no name, you cannot pin it with the turbofish (`::<>`) the way you can a named generic.

```rust
trait Shape { fn area(&self) -> f64; }
struct Sq(f64);
impl Shape for Sq { fn area(&self) -> f64 { self.0 * self.0 } }

fn bigger(a: impl Shape, b: impl Shape) -> f64 {
    a.area().max(b.area())
}

fn main() {
    // does not compile (error[E0107]: function takes 0 generic arguments)
    println!("{}", bigger::<Sq>(Sq(2.0), Sq(3.0)));
}
```

Real compiler error:

```text
error[E0107]: function takes 0 generic arguments but 1 generic argument was supplied
  --> src/main.rs:13:20
   |
13 |     println!("{}", bigger::<Sq>(Sq(2.0), Sq(3.0)));
   |                    ^^^^^^------ help: remove the unnecessary generics
   |                    |
   |                    expected 0 generic arguments
   |
note: function defined here, with 0 generic parameters
  --> src/main.rs:6:4
   |
6  | fn bigger(a: impl Shape, b: impl Shape) -> f64 {
   |    ^^^^^^
   = note: `impl Trait` cannot be explicitly specified as a generic argument
```

**Fix:** when you need to _name_ the type (to turbofish it, or to require two parameters to be the **same** type), use an explicit generic parameter instead:

```rust
fn bigger<T: Shape>(a: T, b: T) -> f64 {
    a.area().max(b.area())
}
```

Now `a` and `b` must be the same `T`, and you can write `bigger::<Sq>(...)`. The rule of thumb: `impl Trait` arguments for one-off, name-free convenience; named generics when the type relationship matters.

### Pitfall 3: Using `impl Trait` where it is not allowed (struct fields)

`impl Trait` is only permitted in the argument and return positions of functions and methods. It is _not_ a general type you can drop into a struct field, a `let` binding's type annotation, or a type alias.

```rust
struct Pipeline {
    // does not compile (error[E0562]: `impl Trait` is not allowed in field types)
    source: impl Iterator<Item = i32>,
}

fn main() {
    let _ = Pipeline { source: (0..3) };
}
```

Real compiler error:

```text
error[E0562]: `impl Trait` is not allowed in field types
 --> src/main.rs:3:13
  |
3 |     source: impl Iterator<Item = i32>,
  |             ^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: `impl Trait` is only allowed in arguments and return types of functions and methods
```

**Fix:** make the struct generic over the field type (`struct Pipeline<I: Iterator<Item = i32>> { source: I }`; see [Generic Structs](/09-generics-traits/01-generic-structs/)), or store a trait object (`source: Box<dyn Iterator<Item = i32>>`).

### Pitfall 4: A trait method using `impl Trait` is not `dyn`-compatible

RPITIT (`-> impl Trait` inside a trait) is convenient, but it makes the trait unusable as a `dyn` trait object, because each implementor's hidden return type could differ, so no single vtable can describe it.

```rust
trait Container {
    fn items(&self) -> impl Iterator<Item = i32>;
}

struct Evens { upto: i32 }

impl Container for Evens {
    fn items(&self) -> impl Iterator<Item = i32> {
        (0..self.upto).filter(|n| n % 2 == 0)
    }
}

fn main() {
    // does not compile (error[E0038]: the trait `Container` is not dyn compatible)
    let boxed: Box<dyn Container> = Box::new(Evens { upto: 8 });
    let _ = boxed;
}
```

Real compiler error (excerpt):

```text
error[E0038]: the trait `Container` is not dyn compatible
  --> src/main.rs:15:24
   |
15 |     let boxed: Box<dyn Container> = Box::new(Evens { upto: 8 });
   |                        ^^^^^^^^^ `Container` is not dyn compatible
   |
note: for a trait to be dyn compatible it needs to allow building a vtable
...
 2 |     fn items(&self) -> impl Iterator<Item = i32>;
   |                        ^^^^^^^^^^^^^^^^^^^^^^^^^ ...because method `items` references an `impl Trait` type in its return type
```

**Fix:** if you need `dyn` dispatch, return a boxed trait object from the trait method instead (`fn items(&self) -> Box<dyn Iterator<Item = i32>>`), accepting the heap allocation. If you only ever use the trait through generics (static dispatch), RPITIT is fine as-is. "dyn compatibility" (formerly called "object safety") is detailed in [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/).

---

## Best Practices

### Prefer `impl Trait` in arguments for single-use, name-free parameters

When a parameter's type appears once and you do not need to refer to it elsewhere, `fn f(x: impl Trait)` reads more cleanly than `fn f<T: Trait>(x: T)`. Reach for the named generic the moment you need to: name the type, repeat it across parameters, turbofish it, or add a `where` clause that mentions it. Both compile to the same code, so the choice is purely about readability and capability.

### Use return-position `impl Trait` to hide unnameable or unstable types

Returning closures and iterator adapters is the canonical, idiomatic use of RPIT. It also gives you encapsulation: callers depend on the _capability_ (`Iterator<Item = T>`), not the exact adapter chain, so you can refactor the body (swap `filter` for `take_while`, add a `map`) without changing the public signature. This is a real API-stability win that has no direct TypeScript analog.

### Reach for `Box<dyn Trait>` when you need runtime variation

`impl Trait` return = one type known at compile time. The moment you need different concrete types depending on runtime conditions (branches, a config flag, a plugin registry), switch to `Box<dyn Trait>`. The cost is one allocation and dynamic dispatch; the benefit is genuine runtime polymorphism. Choosing between them is the central decision covered in [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/).

### Add explicit bounds to `impl Trait` returns when callers need them

`-> impl Iterator<Item = u32>` only promises `Iterator`. If callers also need to, say, clone or debug-print the returned value, add the bound: `-> impl Iterator<Item = u32> + Clone`. The hidden type must then satisfy all listed traits, and callers may rely on every one of them. Be deliberate: the bounds you publish are the contract.

### Treat RPITIT as the default for "method returns an iterator/future"

Since Rust 1.75, returning `impl Trait` from a trait method (including `async fn`, which desugars to `-> impl Future`) is idiomatic and avoids the old associated-type or `async-trait`-boxing boilerplate. Only fall back to associated types or boxing when you specifically need `dyn` dispatch.

---

## Real-World Example

A small, production-flavored log-processing pipeline. `impl Trait` in **argument** position keeps the parser ergonomic (it accepts any iterator of lines), and `impl Trait` in **return** position hides both the parser's iterator-adapter type and a generated filter closure. This is precisely the kind of code where naming the real types would be miserable.

```rust
#[derive(Debug)]
struct LogLine {
    level: String,
    message: String,
}

// Argument position: accept anything iterable yielding `&str`.
// Return position: hide the `FilterMap<...>` adapter type behind the capability.
fn parse_lines<'a>(raw: impl Iterator<Item = &'a str>) -> impl Iterator<Item = LogLine> {
    raw.filter_map(|line| {
        let (level, message) = line.split_once(' ')?;
        Some(LogLine {
            level: level.to_string(),
            message: message.to_string(),
        })
    })
}

// Return position: build and return a reusable predicate closure.
// The closure's type is unnameable, so `impl Fn(...) -> bool` is the only way.
fn at_least(threshold: u8) -> impl Fn(&LogLine) -> bool {
    let rank = |lvl: &str| match lvl {
        "ERROR" => 3u8,
        "WARN" => 2,
        "INFO" => 1,
        _ => 0,
    };
    move |line| rank(&line.level) >= threshold
}

fn main() {
    let raw = "INFO server started\nWARN low disk\nERROR disk full\nDEBUG noise";
    let important = at_least(2);

    for entry in parse_lines(raw.lines()).filter(|l| important(l)) {
        println!("[{}] {}", entry.level, entry.message);
    }
}
```

**Real output:**

```text
[WARN] low disk
[ERROR] disk full
```

Things worth highlighting:

- `parse_lines` takes `impl Iterator<Item = &'a str>`: it works with `raw.lines()`, a `Vec`'s iterator, or anything else yielding `&str`, with zero allocation and full inlining. That single-use argument is a textbook case for the argument-position form rather than a named generic.
- Its return type, `impl Iterator<Item = LogLine>`, hides a `FilterMap<Lines, {closure}>`. We can later add a `.map(...)` or change the parsing logic without touching the signature.
- `at_least` returns `impl Fn(&LogLine) -> bool`. A closure that captures `threshold` has a unique anonymous type with no name you could write down; `impl Fn(...)` is the idiomatic — and only practical — way to return it.

> **Tip:** If you ever needed to choose the parser at runtime (say, JSON lines vs. plain text, selected by a flag), the two parsers would have different concrete iterator types, so you would store the chosen pipeline as `Box<dyn Iterator<Item = LogLine>>` rather than `impl Iterator<...>`. That is Pitfall 1's lesson applied as a design decision.

---

## Further Reading

### Official documentation

- [The Rust Book — Traits as Parameters (`impl Trait`)](https://doc.rust-lang.org/book/ch10-02-traits.html#traits-as-parameters)
- [The Rust Book — Returning Types that Implement Traits](https://doc.rust-lang.org/book/ch10-02-traits.html#returning-types-that-implement-traits)
- [The Rust Reference — `impl Trait`](https://doc.rust-lang.org/reference/types/impl-trait.html)
- [Rust 1.75 release notes — `async fn` and RPIT in traits](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)
- [Edition Guide — RPIT lifetime capture rules (2024)](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html)

### Related sections in this guide

- [Section 09 overview](/09-generics-traits/) — the full map of generics and traits
- [Traits](/09-generics-traits/03-traits/) — defining traits and `impl Trait for Type`
- [Trait Bounds](/09-generics-traits/05-trait-bounds/) — `<T: Trait>`, multiple bounds, `where`, and bounds on returns
- [Generic Functions](/09-generics-traits/00-generic-functions/) — monomorphization vs. TypeScript type erasure; the turbofish `::<>`
- [Generic Structs](/09-generics-traits/01-generic-structs/) — making a struct generic over a field type
- [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/) — `Box<dyn Trait>`, dyn compatibility, and static vs. dynamic dispatch
- [Section 01: Getting Started](/01-getting-started/) — Cargo and editions
- [Section 02: Basics](/02-basics/) — closures and iterator basics
- [Section 10: Smart Pointers](/10-smart-pointers/) — `Box<T>` and heap allocation
- [Section 11: Async](/11-async/) — `async fn` in traits (RPITIT for futures)

---

## Exercises

### Exercise 1: An argument-position `impl Trait` formatter

**Difficulty:** Easy

**Objective:** Use `impl Trait` in argument position to write a function over "anything printable."

**Instructions:** Write a function `describe_all` that takes a slice of items, each of which implements `std::fmt::Display`, and returns a single `String` with the items joined by `", "`. Use `&[impl std::fmt::Display]` as the parameter type. In `main`, call it once with `&[1, 2, 3]` and once with `&["a", "b", "c"]`, printing each result.

```rust
fn describe_all(items: &[impl std::fmt::Display]) -> String {
    // TODO: map each item to its string form and join with ", "
}

fn main() {
    // TODO: call with &[1, 2, 3] and with &["a", "b", "c"]
}
```

<details>
<summary>Solution</summary>

```rust
fn describe_all(items: &[impl std::fmt::Display]) -> String {
    items
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn main() {
    println!("{}", describe_all(&[1, 2, 3]));
    println!("{}", describe_all(&["a", "b", "c"]));
}
```

**Output:**

```text
1, 2, 3
a, b, c
```

`&[impl Display]` is the anonymous-generic form of `<T: Display>(items: &[T])`. Because each call uses a different concrete element type, the compiler monomorphizes `describe_all` once per type.

</details>

### Exercise 2: Return a stateful closure with RPIT

**Difficulty:** Medium

**Objective:** Return a closure whose type is unnameable, using return-position `impl Trait`, and observe that an `FnMut` closure can carry mutable state.

**Instructions:** Write a function `make_counter(start: i32) -> impl FnMut() -> i32` that returns a closure. Each time the closure is called it should return the current value and then increment it (so the first call returns `start`, the next `start + 1`, and so on). In `main`, create a counter starting at `100` and call it three times, printing each result.

```rust
fn make_counter(start: i32) -> impl FnMut() -> i32 {
    // TODO: capture mutable state and return it, incrementing each call
}

fn main() {
    // TODO: make a counter at 100 and call it three times
}
```

<details>
<summary>Solution</summary>

```rust
fn make_counter(start: i32) -> impl FnMut() -> i32 {
    let mut current = start;
    move || {
        let value = current;
        current += 1;
        value
    }
}

fn main() {
    let mut next_id = make_counter(100);
    println!("{}", next_id());
    println!("{}", next_id());
    println!("{}", next_id());
}
```

**Output:**

```text
100
101
102
```

The closure captures `current` by value (`move`) and mutates it, so it implements `FnMut` rather than only `Fn`. Returning `impl FnMut() -> i32` is the only practical way to hand back a closure, since its type has no name. Note that the binding must be `let mut next_id` because calling an `FnMut` needs a mutable borrow of the closure.

</details>

### Exercise 3: An iterator-returning trait method (RPITIT) and a generic consumer

**Difficulty:** Hard

**Objective:** Use return-position `impl Trait` inside a trait, then consume it through an argument-position `impl Trait`, combining both forms in one program.

**Instructions:** Define a trait `DataSource` with one method `records(&self) -> impl Iterator<Item = String>`. Implement it for a struct `StaticList { data: Vec<String> }` so that `records` yields clones of the stored strings. Write a free function `print_records(source: &impl DataSource)` that iterates the source's records and prints each on its own line. In `main`, build a `StaticList` containing `"alpha"` and `"beta"` and pass it to `print_records`.

```rust
trait DataSource {
    // TODO: records(&self) -> impl Iterator<Item = String>
}

struct StaticList {
    data: Vec<String>,
}

// TODO: impl DataSource for StaticList

fn print_records(source: &impl DataSource) {
    // TODO: iterate source.records() and print each
}

fn main() {
    // TODO: build a StaticList and call print_records
}
```

<details>
<summary>Solution</summary>

```rust
trait DataSource {
    fn records(&self) -> impl Iterator<Item = String>;
}

struct StaticList {
    data: Vec<String>,
}

impl DataSource for StaticList {
    fn records(&self) -> impl Iterator<Item = String> {
        self.data.iter().cloned()
    }
}

fn print_records(source: &impl DataSource) {
    for r in source.records() {
        println!("{r}");
    }
}

fn main() {
    let src = StaticList {
        data: vec!["alpha".to_string(), "beta".to_string()],
    };
    print_records(&src);
}
```

**Output:**

```text
alpha
beta
```

This program uses RPITIT (the `impl Iterator` return inside the trait) and an argument-position `impl Trait` (in `print_records`) together. Because `DataSource` uses RPITIT, it is not `dyn`-compatible (you could not write `Box<dyn DataSource>`), but static dispatch through `&impl DataSource` works perfectly and allocates nothing. To support `Box<dyn DataSource>`, you would change the method to return `Box<dyn Iterator<Item = String>>` instead.

</details>
