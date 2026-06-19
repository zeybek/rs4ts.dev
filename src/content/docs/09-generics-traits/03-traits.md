---
title: "Rust Traits vs TypeScript Interfaces"
description: "Rust traits are TypeScript interfaces with a twist: impls are written separately, are nominal not structural, and let you add behavior to foreign types."
---

Traits are Rust's answer to TypeScript interfaces: a named set of behaviors a type can promise to provide. If you have ever written `interface Serializable { serialize(): string }` and implemented it on a class, you already know the shape of the idea. The mechanics, though, are deliberately different, and those differences are the whole point of this file.

---

## Quick Overview

A **trait** declares a set of methods a type must provide; an `impl Trait for Type` block provides them. It is the closest Rust feature to a TypeScript `interface`, but with one big twist: in TypeScript a class declares "I implement this interface" at its definition, while in Rust the implementation is written **separately** from both the trait and the type. That separation lets you add behavior to types you did not define, and it powers generics, dynamic dispatch, and operator overloading throughout the language.

> **Note:** In Rust we say **trait**, never "interface." This file deliberately keeps the analogy front and center, but the vocabulary you should adopt is `trait` and `impl`.

---

## TypeScript/JavaScript Example

Here is a small, realistic content-feed scenario in TypeScript. We define an interface describing "things that can summarize themselves," then implement it on two classes.

```typescript
// TypeScript - an interface plus two classes that implement it
interface Summary {
  summarize(): string;
}

class Article implements Summary {
  constructor(
    public title: string,
    public author: string,
    public body: string,
  ) {}

  summarize(): string {
    return `${this.title} by ${this.author}`;
  }
}

class Tweet implements Summary {
  constructor(
    public username: string,
    public content: string,
  ) {}

  summarize(): string {
    return `@${this.username}: ${this.content}`;
  }
}

const article = new Article(
  "Rust 1.96 released",
  "The Rust Team",
  "Today we are happy to announce...",
);
const tweet = new Tweet("rustlang", "We just shipped a new release!");

console.log(`Article: ${article.summarize()}`);
console.log(`Tweet:   ${tweet.summarize()}`);
```

**Key points:**

- The interface (`Summary`) only describes a shape; it has no runtime presence (TypeScript types are erased).
- Each class opts in with `implements Summary` **at its own declaration**, and the method body lives inside the class.
- Structural typing means a class with a matching `summarize()` would satisfy `Summary` even without `implements`.

---

## Rust Equivalent

The same scenario in idiomatic Rust. Notice that the data (`struct`) and the behavior (`impl ... for ...`) are written as separate blocks.

```rust
// Rust - a trait plus two structs that implement it
trait Summary {
    fn summarize(&self) -> String;
}

struct Article {
    title: String,
    author: String,
    body: String,
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

fn main() {
    let article = Article {
        title: String::from("Rust 1.96 released"),
        author: String::from("The Rust Team"),
        body: String::from("Today we are happy to announce..."),
    };

    let tweet = Tweet {
        username: String::from("rustlang"),
        content: String::from("We just shipped a new release!"),
    };

    println!("Article: {}", article.summarize());
    println!("Tweet:   {}", tweet.summarize());
}
```

**Real output:**

```text
Article: Rust 1.96 released by The Rust Team
Tweet:   @rustlang: We just shipped a new release!
```

> **Note:** The current stable toolchain is **Rust 1.96.0** on the latest stable edition (2024). Create new projects with `cargo new` — it auto-selects the newest edition — so you never need to pin an older one. Everything in this file is verified on a 2024-edition project.

---

## Detailed Explanation

Let's walk through the Rust version line by line and contrast each piece with the TypeScript you already know.

### Declaring the trait

```rust
trait Summary {
    fn summarize(&self) -> String;
}
```

- `trait Summary { ... }` declares the contract, exactly like `interface Summary { ... }`.
- Inside, `fn summarize(&self) -> String;` is a **method signature with no body**: the semicolon ends it. This is a **required method**: any type implementing `Summary` must supply it. (Traits can also provide **default** method bodies; that is the focus of the sibling files [Trait Methods](/09-generics-traits/04-trait-methods/) and [Default Method Implementations](/09-generics-traits/08-default-impls/).)
- `&self` is the receiver. It is the rough equivalent of `this` in a TypeScript method, but it is an explicit parameter and it is a **borrow**: an immutable reference to the value the method is called on. Other receiver forms are `&mut self` (a mutable borrow, like a mutating method) and `self` (takes ownership, consuming the value). Borrowing is covered in [Section 05: Ownership](/05-ownership/).

### The data is separate from the behavior

In TypeScript, the method body lives inside the class. In Rust, the `struct` holds only data:

```rust
struct Article {
    title: String,
    author: String,
    body: String,
}
```

and the behavior is bolted on afterward in a dedicated block:

```rust
impl Summary for Article {
    fn summarize(&self) -> String {
        format!("{} by {}", self.title, self.author)
    }
}
```

Read `impl Summary for Article` as "implement the `Summary` trait for the `Article` type." This is the headline syntactic difference from TypeScript: **the implementation is a free-standing item**, not a clause on the type definition. You can put it in the same file, a different module, or even (under certain rules) apply a trait you defined to a type from the standard library.

### `self` access and `format!`

Inside `summarize`, `self.title` and `self.author` read fields through the borrow. `format!` is Rust's string-building macro; it works like a template literal that returns a new `String` (it does not print). We use inline interpolation (`{}` positionally here; you can also write `format!("{title}")` to capture a variable named `title` directly).

### Calling the method

```rust
println!("Article: {}", article.summarize());
```

Method-call syntax (`value.method()`) is identical to TypeScript. The compiler resolves `summarize` to the `Summary` implementation for `Article`. One catch that has no TypeScript analogue: **the trait must be in scope** at the call site (more on this in Common Pitfalls).

### Inherent methods vs trait methods

A type can also have methods that belong to *no* trait; these live in an **inherent `impl`** block. Mixing both is common:

```rust
trait Summary {
    fn summarize(&self) -> String;
}

struct Article {
    title: String,
}

// Inherent impl: methods that belong only to Article (no trait).
impl Article {
    fn new(title: &str) -> Self {
        Article { title: title.to_string() }
    }
    fn word_count(&self) -> usize {
        self.title.split_whitespace().count()
    }
}

// Trait impl: fulfilling the Summary contract.
impl Summary for Article {
    fn summarize(&self) -> String {
        format!("\"{}\" ({} words)", self.title, self.word_count())
    }
}

fn main() {
    let a = Article::new("Hello Rust World");
    println!("{}", a.summarize());
}
```

**Real output:**

```text
"Hello Rust World" (3 words)
```

`impl Article { ... }` (no `for`) defines methods and associated functions like `Article::new`: the rough equivalent of a class's own methods and `static` factory. `impl Summary for Article { ... }` separately fulfills the trait contract. The two coexist, and trait methods can call inherent methods (`self.word_count()`) and vice versa. `Self` (capitalized) inside an `impl` is an alias for the type being implemented.

---

## Key Differences

| Concept | TypeScript `interface` | Rust `trait` |
| --- | --- | --- |
| Where the impl lives | Inside the class (`implements`) | A separate `impl Trait for Type` block |
| Opt-in vs structural | Structural: matching shape is enough | Nominal: you must write an explicit `impl` |
| Runtime presence | Erased at compile time | Real: drives monomorphization and dispatch |
| Add behavior to a foreign type | Not really (you'd subclass/wrap) | Yes, if you own the trait (the orphan rule) |
| Default method bodies | Not in plain interfaces | Yes — provided methods |
| Dispatch model | Always dynamic (every object) | Static by default; dynamic via `dyn` (opt-in) |

### Nominal, not structural

This is the difference TypeScript developers feel first. In TypeScript, a class satisfies an interface if it has the right shape, whether or not it says `implements`. In Rust, a type implements a trait **only if there is an explicit `impl` block** for that exact trait. Two traits with identical method signatures are completely distinct, and a type that satisfies one does not automatically satisfy the other. This is *nominal* typing: the name of the trait matters, not just the shape.

### Traits are real at runtime; interfaces are erased

TypeScript interfaces vanish after compilation; they exist only to type-check your source. Rust traits stay relevant all the way to the binary. When you use a trait as a bound on a generic (`fn f<T: Summary>`), the compiler **monomorphizes**: it stamps out a specialized copy of the function for each concrete type, with no runtime lookup. (Compare this to TypeScript generics, which are erased; see [Generic Functions](/09-generics-traits/00-generic-functions/).) When you use a trait object (`&dyn Summary`), the compiler builds a vtable for dynamic dispatch, covered in [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/).

### Implementation is decoupled — within limits

Because the `impl` is separate, you can implement *your own* trait for a type you did not define (for example, implementing a `Pluralize` trait for `&str`). What you cannot do is implement a trait you do not own for a type you do not own: the **orphan rule**. That coherence rule has its own sibling file, [The Orphan Rule and Coherence](/09-generics-traits/12-orphan-rule/), and it appears as a pitfall below.

---

## Common Pitfalls

### Pitfall 1: Calling a trait method without the trait in scope

The implementation can exist and still be uncallable if the trait name is not imported. TypeScript has no equivalent: once a method exists on a class, it is always callable.

```rust
// src/shapes.rs
pub trait Area {
    fn area(&self) -> f64;
}
pub struct Square { pub side: f64 }
impl Area for Square {
    fn area(&self) -> f64 { self.side * self.side }
}

// src/main.rs
mod shapes;
use shapes::Square; // brought in the type but NOT the `Area` trait

fn main() {
    let s = Square { side: 3.0 };
    println!("{}", s.area()); // does not compile (error[E0599])
}
```

Real compiler error:

```text
error[E0599]: no method named `area` found for struct `Square` in the current scope
 --> src/main.rs:6:22
  |
6 |     println!("{}", s.area());
  |                      ^^^^ method not found in `Square`
  |
 ::: src/shapes.rs:2:8
  |
2 |     fn area(&self) -> f64;
  |        ---- the method is available for `Square` here
3 | }
4 | pub struct Square { pub side: f64 }
  | ----------------- method `area` not found for this struct
  |
  = help: items from traits can only be used if the trait is in scope
help: trait `Area` which provides `area` is implemented but not in scope; perhaps you want to import it
  |
1 + use crate::shapes::Area;
  |
```

**Fix:** bring the trait into scope with `use crate::shapes::Area;`. The compiler even prints the exact line to add. (Modules and `use` are covered in [Section 12](/12-modules-packages/).)

### Pitfall 2: Forgetting a required method

Unlike a TypeScript class — where a missing interface method is flagged at the class — Rust reports it on the `impl` block, and the whole crate fails to compile.

```rust
trait Greet {
    fn hello(&self) -> String;
    fn goodbye(&self) -> String;
}

struct Robot;

impl Greet for Robot {
    fn hello(&self) -> String {
        String::from("BEEP BOOP HELLO")
    }
    // Forgot to implement `goodbye`
} // does not compile (error[E0046]: missing `goodbye` in implementation)

fn main() {}
```

Real compiler error:

```text
error[E0046]: not all trait items implemented, missing: `goodbye`
 --> src/main.rs:8:1
  |
3 |     fn goodbye(&self) -> String;
  |     ---------------------------- `goodbye` from trait
...
8 | impl Greet for Robot {
  | ^^^^^^^^^^^^^^^^^^^^ missing `goodbye` in implementation
```

**Fix:** implement every required method, or give the trait a default body for it (see [Default Method Implementations](/09-generics-traits/08-default-impls/)).

### Pitfall 3: Implementing a foreign trait for a foreign type (orphan rule)

Coming from TypeScript, it is tempting to "just add `Display` to `Vec<i32>`." Rust forbids it because neither the trait nor the type is local to your crate.

```rust
use std::fmt::Display;

// Both Display (std) and Vec (std) are foreign to this crate.
impl Display for Vec<i32> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "a vector")
    }
} // does not compile (error[E0117])

fn main() {}
```

Real compiler error:

```text
error[E0117]: only traits defined in the current crate can be implemented for types defined outside of the crate
 --> src/main.rs:4:1
  |
4 | impl Display for Vec<i32> {
  | ^^^^^^^^^^^^^^^^^--------
  |                  |
  |                  `Vec` is not defined in the current crate
  |
  = note: impl doesn't have any local type before any uncovered type parameters
  = note: for more information see https://doc.rust-lang.org/reference/items/implementations.html#orphan-rules
  = note: define and implement a trait or new type instead
```

**Fix:** either define your own trait (you own the trait, so you may implement it for `Vec<i32>`), or wrap the foreign type in a local **newtype** (`struct MyVec(Vec<i32>)`) and implement the foreign trait for the wrapper. The full treatment is in [The Orphan Rule and Coherence](/09-generics-traits/12-orphan-rule/).

### Pitfall 4: Expecting structural matching

A type that happens to have a `summarize(&self) -> String` method does **not** implement `Summary` unless you write `impl Summary for ThatType`. There is no duck typing here. If you want polymorphism, you must declare the relationship explicitly.

---

## Best Practices

### Keep traits small and focused

Prefer narrow traits with one clear responsibility (`Summary`, `Drawable`, `Notifier`) over a single mega-trait. Small traits compose well as bounds (`T: Summary + Clone`) and are easier to implement. This mirrors the interface-segregation instinct you already have in TypeScript.

### Name traits by capability, often with `-able` or a verb-noun

The standard library sets the tone: `Display`, `Clone`, `Iterator`, `From`, `Default`. Trait names usually describe *what the type can do*. A good smell test: "this type **is** `Summary`-able."

### Provide default method bodies to reduce boilerplate

If most implementors would write the same method, give it a default in the trait so implementors only override when needed. See [Default Method Implementations](/09-generics-traits/08-default-impls/).

### Let generic functions take the weakest bound that works

When you write a function over "anything summarizable," prefer a generic with a trait bound (static dispatch, zero-cost) and reach for `dyn` only when you genuinely need a heterogeneous collection at runtime:

```rust
trait Summary {
    fn summarize(&self) -> String;
}

struct BlogPost {
    title: String,
}

impl Summary for BlogPost {
    fn summarize(&self) -> String {
        format!("Post: {}", self.title)
    }
}

// A generic function constrained to any T that implements Summary.
fn announce<T: Summary>(item: &T) {
    println!("Breaking! {}", item.summarize());
}

fn main() {
    let post = BlogPost {
        title: String::from("Traits in Rust for TS/JS Developers"),
    };
    announce(&post);
}
```

**Real output:**

```text
Breaking! Post: Traits in Rust for TS/JS Developers
```

Trait bounds are the topic of [Trait Bounds](/09-generics-traits/05-trait-bounds/), and the `impl Trait` shorthand for arguments and return types is covered in [`impl Trait`](/09-generics-traits/07-impl-trait/).

### Group inherent methods and trait impls thoughtfully

Put constructors and type-specific helpers in an inherent `impl Type` block; keep each `impl Trait for Type` focused on satisfying that one trait. This keeps the "what it is" separate from "what contracts it fulfills."

---

## Real-World Example

A small notification system. Each delivery channel is its own type implementing a shared `Notifier` trait, and a dispatcher broadcasts a message across a heterogeneous list of channels. This is a classic place where defining a trait and implementing it per type pays off.

```rust
// Each channel implements one trait; the dispatcher works over any of them.
trait Notifier {
    /// Send a message; returns a short delivery receipt string.
    fn send(&self, message: &str) -> String;
}

struct Email {
    address: String,
}

struct Sms {
    phone: String,
}

struct Slack {
    channel: String,
}

impl Notifier for Email {
    fn send(&self, message: &str) -> String {
        format!("email -> {}: {}", self.address, message)
    }
}

impl Notifier for Sms {
    fn send(&self, message: &str) -> String {
        format!("sms -> {}: {}", self.phone, message)
    }
}

impl Notifier for Slack {
    fn send(&self, message: &str) -> String {
        format!("slack -> #{}: {}", self.channel, message)
    }
}

/// Broadcast one message to every configured channel.
fn broadcast(channels: &[Box<dyn Notifier>], message: &str) {
    for channel in channels {
        println!("{}", channel.send(message));
    }
}

fn main() {
    let channels: Vec<Box<dyn Notifier>> = vec![
        Box::new(Email { address: "ops@example.com".into() }),
        Box::new(Sms { phone: "+1-555-0100".into() }),
        Box::new(Slack { channel: "incidents".into() }),
    ];

    broadcast(&channels, "Deploy finished successfully");
}
```

**Real output:**

```text
email -> ops@example.com: Deploy finished successfully
sms -> +1-555-0100: Deploy finished successfully
slack -> #incidents: Deploy finished successfully
```

Two things worth highlighting. First, `Vec<Box<dyn Notifier>>` is a heterogeneous list (different concrete types stored behind a common trait) which requires the dynamic-dispatch trait object (`dyn`); a plain generic `Vec<T>` could only hold one concrete type. `Box` is the heap-allocating smart pointer; see [Section 10: Smart Pointers](/10-smart-pointers/). Second, adding a new channel (say, `Webhook`) is purely additive: define the struct, write one `impl Notifier for Webhook`, and the dispatcher needs no changes.

> **Tip:** If every channel always exists and you do not need a runtime-variable list of mixed types, prefer a generic `fn send_all<T: Notifier>(channel: &T, ...)` for zero-cost static dispatch. Choose `dyn` specifically when you need to mix types in one collection. The static-vs-dynamic trade-off is the subject of [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/).

---

## Further Reading

### Official documentation

- [The Rust Book — Traits: Defining Shared Behavior](https://doc.rust-lang.org/book/ch10-02-traits.html)
- [Rust by Example — Traits](https://doc.rust-lang.org/rust-by-example/trait.html)
- [The Rust Reference — Traits](https://doc.rust-lang.org/reference/items/traits.html)
- [The Rust Reference — Implementations & coherence](https://doc.rust-lang.org/reference/items/implementations.html)

### Related sections in this guide

- [Section 09 overview](/09-generics-traits/): the full map of generics and traits
- [Trait Methods](/09-generics-traits/04-trait-methods/): required vs provided methods, overriding defaults
- [Default Method Implementations](/09-generics-traits/08-default-impls/): default method bodies and how they cut boilerplate
- [Trait Bounds](/09-generics-traits/05-trait-bounds/): constraining generics with `<T: Trait>` and `where`
- [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/) — `&dyn Trait` / `Box<dyn Trait>`, object safety, dispatch trade-offs
- [`impl Trait`](/09-generics-traits/07-impl-trait/): `impl Trait` in argument and return position
- [Supertraits](/09-generics-traits/09-supertraits/): requiring one trait as a prerequisite for another
- [Operator Overloading](/09-generics-traits/10-operator-overloading/) — implementing `Add`, `Index`, and friends for your types
- [The Orphan Rule and Coherence](/09-generics-traits/12-orphan-rule/): coherence, the orphan rule, and the newtype workaround
- [Generic Functions](/09-generics-traits/00-generic-functions/): monomorphization vs TypeScript type erasure
- [Section 05: Ownership](/05-ownership/) — what `&self`, `&mut self`, and `self` receivers mean
- [Section 10: Smart Pointers](/10-smart-pointers/): `Box<dyn Trait>` and heap allocation

---

## Exercises

### Exercise 1: Define and implement a trait

**Difficulty:** Easy

**Objective:** Practice the core `trait` / `impl Trait for Type` workflow on more than one type.

**Instructions:** Define a trait `Describe` with one required method `describe(&self) -> String`. Create two structs, `Dog { name: String }` and `Cat { name: String }`, and implement `Describe` for both so that a dog says "woof" and a cat says "meow." In `main`, construct one of each and print both descriptions.

```rust
trait Describe {
    // TODO: declare describe(&self) -> String
}

struct Dog { name: String }
struct Cat { name: String }

// TODO: impl Describe for Dog
// TODO: impl Describe for Cat

fn main() {
    // TODO: build a Dog and a Cat, print describe() for each
}
```

<details>
<summary>Solution</summary>

```rust
trait Describe {
    fn describe(&self) -> String;
}

struct Dog {
    name: String,
}

struct Cat {
    name: String,
}

impl Describe for Dog {
    fn describe(&self) -> String {
        format!("{} the dog says woof", self.name)
    }
}

impl Describe for Cat {
    fn describe(&self) -> String {
        format!("{} the cat says meow", self.name)
    }
}

fn main() {
    let d = Dog { name: String::from("Rex") };
    let c = Cat { name: String::from("Lily") };
    println!("{}", d.describe());
    println!("{}", c.describe());
}
```

**Output:**

```text
Rex the dog says woof
Lily the cat says meow
```

</details>

### Exercise 2: Implement your own trait for a standard-library type

**Difficulty:** Medium

**Objective:** See how the orphan rule lets you extend a foreign type as long as *you* own the trait.

**Instructions:** Define a trait `Pluralize` with a method `pluralize(&self, count: usize) -> String`. Implement it for `&str` so that `"apple".pluralize(1)` produces `"1 apple"` and `"apple".pluralize(3)` produces `"3 apples"` (append an `s` whenever `count != 1`). In `main`, print both. This is allowed even though `&str` is a foreign type, because the trait is local to your crate.

```rust
trait Pluralize {
    // TODO: pluralize(&self, count: usize) -> String
}

// TODO: impl Pluralize for &str

fn main() {
    // TODO: print "apple".pluralize(1) and "apple".pluralize(3)
}
```

<details>
<summary>Solution</summary>

```rust
// We OWN the trait, so we may implement it for the foreign type &str.
trait Pluralize {
    fn pluralize(&self, count: usize) -> String;
}

impl Pluralize for &str {
    fn pluralize(&self, count: usize) -> String {
        if count == 1 {
            format!("{count} {self}")
        } else {
            format!("{count} {self}s")
        }
    }
}

fn main() {
    println!("{}", "apple".pluralize(1));
    println!("{}", "apple".pluralize(3));
}
```

**Output:**

```text
1 apple
3 apples
```

> If you instead tried to implement a *standard-library* trait (like `Display`) for `&str`, the compiler would reject it with `error[E0117]`; see Pitfall 3.

</details>

### Exercise 3: A trait used through trait objects

**Difficulty:** Hard

**Objective:** Combine multiple required methods, multiple implementors, and a function that operates over a heterogeneous collection of trait objects.

**Instructions:** Define a trait `Shape` with two required methods: `area(&self) -> f64` and `name(&self) -> &str`. Implement it for `Circle { radius: f64 }` (area = pi r squared, name `"circle"`) and `Square { side: f64 }` (area = side squared, name `"square"`). Write a free function `total_area(shapes: &[Box<dyn Shape>]) -> f64` that sums the areas. In `main`, build a `Vec<Box<dyn Shape>>` with one of each, print each shape's name and area to two decimals, and print the total.

```rust
trait Shape {
    // TODO: area(&self) -> f64 and name(&self) -> &str
}

struct Circle { radius: f64 }
struct Square { side: f64 }

// TODO: impl Shape for Circle and Square

fn total_area(shapes: &[Box<dyn Shape>]) -> f64 {
    // TODO: sum the areas
}

fn main() {
    // TODO: build the Vec<Box<dyn Shape>>, print names/areas, print total
}
```

<details>
<summary>Solution</summary>

```rust
trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> &str;
}

struct Circle {
    radius: f64,
}

struct Square {
    side: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.radius * self.radius
    }
    fn name(&self) -> &str {
        "circle"
    }
}

impl Shape for Square {
    fn area(&self) -> f64 {
        self.side * self.side
    }
    fn name(&self) -> &str {
        "square"
    }
}

fn total_area(shapes: &[Box<dyn Shape>]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

fn main() {
    let shapes: Vec<Box<dyn Shape>> = vec![
        Box::new(Circle { radius: 1.0 }),
        Box::new(Square { side: 2.0 }),
    ];
    for s in &shapes {
        println!("{}: {:.2}", s.name(), s.area());
    }
    println!("total: {:.2}", total_area(&shapes));
}
```

**Output:**

```text
circle: 3.14
square: 4.00
total: 7.14
```

> Storing different concrete types in one `Vec` requires the `dyn` trait object: a plain `Vec<T>` holds only one concrete type. The dynamic-dispatch mechanics are covered in detail in [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/).

</details>
