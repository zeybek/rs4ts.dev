---
title: "Supertraits: One Trait Requiring Another"
description: "A Rust supertrait (trait Sub: Super) requires implementors to also implement another trait, echoing TypeScript's interface extends as a constraint."
---

In TypeScript you can write `interface Admin extends User`, declaring that every `Admin` is also a `User`. Rust has a parallel idea — a **supertrait** — written `trait Admin: User`. It looks like inheritance, and the colon even reads like `extends`, but underneath it is something more precise: a *requirement*. Saying "`Plugin` has supertrait `Component`" means "you cannot implement `Plugin` for a type unless that type also implements `Component`."

---

## Quick Overview

A **supertrait** is a trait that another trait depends on. When you declare `trait Sub: Super`, you are telling the compiler two things: any type that implements `Sub` *must* also implement `Super`, and the methods of `Super` are available to call from inside `Sub` (including from `Sub`'s default method bodies). It is Rust's closest analogue to interface inheritance in TypeScript, but it expresses a **constraint on implementors**, not the copying-down of behavior you get from class `extends`.

> **Note:** This page covers the supertrait relationship: the `trait A: B` syntax, why it is a requirement rather than inheritance, and how it interacts with generics and trait objects. Defining and implementing traits is in [Traits](/09-generics-traits/03-traits/); the `<T: Trait>` bounds it resembles are in [Trait Bounds](/09-generics-traits/05-trait-bounds/); default method bodies are in [Default Method Implementations](/09-generics-traits/08-default-impls/).

---

## TypeScript/JavaScript Example

In TypeScript, one interface can extend another. The extending interface gains the parent's members, and any implementor must satisfy both.

```typescript
// TypeScript - an interface that extends another interface
interface Component {
  name(): string;
  describe(): string;
}

// `Plugin` extends `Component`: a Plugin is also a Component.
interface Plugin extends Component {
  start(): string;
  bootLog(): string;
}

class Logger implements Plugin {
  constructor(private level: string) {}

  name(): string {
    return "logger";
  }

  describe(): string {
    return `component '${this.name()}'`;
  }

  start(): string {
    return `logging at level ${this.level}`;
  }

  // A Plugin method that calls a Component method (`describe`).
  bootLog(): string {
    return `booting ${this.describe()} -> ${this.start()}`;
  }
}

const logger = new Logger("info");
console.log(logger.bootLog());
// booting component 'logger' -> logging at level info
```

**Key points:**

- `Plugin extends Component` means `Logger` must satisfy *both* interfaces' members.
- `bootLog` (a `Plugin` member) freely calls `describe` (a `Component` member) — the compiler knows the receiver has both.
- Structural typing applies: any object with all four members (`name`, `describe`, `start`, `bootLog`) is a `Plugin`, whether or not it says `implements`.

---

## Rust Equivalent

The same plugin shape in idiomatic Rust. The `Component` behavior and the `Plugin` behavior are two separate traits, and `Plugin` declares `Component` as its supertrait with a colon.

```rust playground
// A small plugin system. Every Plugin is first a Component (it has a name and
// can describe itself); Plugin adds lifecycle behavior on top.

trait Component {
    fn name(&self) -> &str;

    // Provided (default) method using only this trait's required methods.
    fn describe(&self) -> String {
        format!("component '{}'", self.name())
    }
}

// `Plugin` REQUIRES `Component`: you cannot be a Plugin without being a Component.
trait Plugin: Component {
    fn start(&self) -> String;

    // Provided method that calls a SUPERTRAIT method (`describe`).
    fn boot_log(&self) -> String {
        format!("booting {} -> {}", self.describe(), self.start())
    }
}

struct Logger {
    level: String,
}

// We must satisfy the supertrait with its own impl block.
impl Component for Logger {
    fn name(&self) -> &str {
        "logger"
    }
}

impl Plugin for Logger {
    fn start(&self) -> String {
        format!("logging at level {}", self.level)
    }
}

fn main() {
    let logger = Logger { level: String::from("info") };
    println!("{}", logger.boot_log());
}
```

**Real output:**

```text
booting component 'logger' -> logging at level info
```

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the **2024 edition**. Create new projects with `cargo new` — it auto-selects the newest edition — so you never need to pin an older one. Everything in this file is verified on a 2024-edition project.

---

## Detailed Explanation

Let's walk through the Rust version and contrast each piece with the TypeScript you already know.

### The supertrait declaration

```rust
trait Plugin: Component {
    fn start(&self) -> String;
    fn boot_log(&self) -> String { /* ... */ }
}
```

The `: Component` after the trait name is the whole feature. Read `trait Plugin: Component` as **"`Plugin` requires `Component`"**, or, if you want the TypeScript mnemonic, "`Plugin` extends `Component`." `Component` is the **supertrait**; `Plugin` is the **subtrait**.

This is the same syntax you would use for a trait bound on a generic (`<T: Component>`). That is not a coincidence: a supertrait *is* a bound, applied to the implementing type itself. The compiler reads `trait Plugin: Component` as "for any `Self` implementing `Plugin`, `Self: Component` must also hold."

### Two separate `impl` blocks

In TypeScript, `class Logger implements Plugin` provides every member of `Plugin` *and* `Component` in a single class body. In Rust the two traits are filled in **separately**:

```rust
impl Component for Logger { /* name() ... */ }
impl Plugin for Logger    { /* start() ... */ }
```

This is the headline difference. A supertrait does **not** fold the parent's methods into the child trait; each trait keeps its own identity and gets its own `impl` block. You implement `Component for Logger` to satisfy the requirement, then `Plugin for Logger` to add the plugin behavior. Forgetting the first one is a compile error (see Common Pitfalls).

### Calling supertrait methods from a subtrait

Inside `Plugin`'s default method, this line works:

```rust
fn boot_log(&self) -> String {
    format!("booting {} -> {}", self.describe(), self.start())
}
```

`self.describe()` is a `Component` method, yet we call it from a `Plugin` method. That is the *payoff* of the supertrait relationship: because every `Self: Plugin` is guaranteed to also be `Component`, the compiler lets `Plugin` rely on `Component`'s API. Without the `: Component` bound, `self.describe()` would not type-check — there would be no proof that `Self` has a `describe` method.

### Overriding a default through the chain

Default methods still behave the way [Default Method Implementations](/09-generics-traits/08-default-impls/) describes. A type can override `Component::describe`, and `Plugin::boot_log` will pick up the override because method dispatch resolves on the concrete type:

```rust playground
trait Component {
    fn name(&self) -> &str;
    fn describe(&self) -> String {
        format!("component '{}'", self.name())
    }
}

trait Plugin: Component {
    fn start(&self) -> String;
    fn boot_log(&self) -> String {
        format!("booting {} -> {}", self.describe(), self.start())
    }
}

struct Metrics {
    endpoint: String,
}

impl Component for Metrics {
    fn name(&self) -> &str {
        "metrics"
    }
    // Override the default describe for this type.
    fn describe(&self) -> String {
        format!("metrics exporter -> {}", self.endpoint)
    }
}

impl Plugin for Metrics {
    fn start(&self) -> String {
        String::from("scraping every 15s")
    }
}

fn main() {
    let m = Metrics { endpoint: String::from("/metrics") };
    println!("{}", m.boot_log());
}
```

**Real output:**

```text
booting metrics exporter -> /metrics -> scraping every 15s
```

`boot_log` never changed, but it now reports the overridden description, because `self.describe()` dispatches to the `Metrics` implementation.

### Supertraits from the standard library

The supertrait does not have to be one you wrote. A very common pattern is requiring a `std` trait like `Display` so your trait's default methods can format `self`:

```rust playground
use std::fmt::Display;

// Every Greet type must also be Display.
trait Greet: Display {
    fn greet(&self) -> String {
        // `self` is guaranteed to be Display, so `{self}` works.
        format!("Hi from {self}")
    }
}

struct Bot(u32);

impl Display for Bot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bot#{}", self.0)
    }
}

impl Greet for Bot {}

fn main() {
    println!("{}", Bot(7).greet());
}
```

**Real output:**

```text
Hi from bot#7
```

Note the empty `impl Greet for Bot {}` block: `greet` has a default body that relies entirely on the `Display` supertrait, so there is nothing left to write.

---

## Key Differences

| Concept | TypeScript `interface B extends A` | Rust `trait B: A` |
| --- | --- | --- |
| What it means | `B` includes all members of `A` | A type implementing `B` *must also* implement `A` |
| Where members are supplied | One class body implements all of `B` and `A` | Separate `impl A for T` and `impl B for T` blocks |
| Is behavior inherited? | Members are merged into `B` | No merge — each trait stays distinct; `B` may *call* `A`'s methods |
| Multiple parents | `extends A, C` (intersection) | `trait B: A + C` |
| Runtime presence | Erased | Real: a `B` bound transitively proves the `A` bound |
| Opt-in style | Structural (matching shape is enough) | Nominal (explicit `impl` for each trait) |

### A supertrait is a requirement, not subclassing

This is the single most important mental adjustment. `trait Plugin: Component` does **not** say "`Plugin` contains `Component`'s methods." It says "any implementor of `Plugin` is obligated to also implement `Component`." There is no shared state, no constructor chain, and no method-body copying. Rust has no class inheritance at all. Supertraits express the *only* trait-to-trait relationship there is, and it is a pure constraint plus the permission to call the supertrait's API.

### The `Self` bound, spelled out

These two declarations are equivalent in meaning:

```rust
trait Plugin: Component { /* ... */ }

// Same constraint, written as an explicit `where` clause on Self:
trait Plugin where Self: Component { /* ... */ }
```

Both say "`Self: Component`." The colon form is the idiom you will read everywhere; the `where Self:` form occasionally appears when the bound is long or conditional. Seeing them as the same thing demystifies why a supertrait behaves exactly like a trait bound (see [Trait Bounds](/09-generics-traits/05-trait-bounds/)).

### Bounds compose transitively

If a generic function is bounded by the subtrait, it automatically gets the supertrait's API too. You do not repeat the bound:

```rust playground
use std::fmt::Display;

trait Greet: Display {
    fn greet(&self) -> String {
        format!("Hi from {self}")
    }
}

struct Bot(u32);

impl Display for Bot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bot#{}", self.0)
    }
}

impl Greet for Bot {}

// One bound (`Greet`) transitively gives us Display too.
fn announce<T: Greet>(item: &T) {
    println!("display: {item}");        // uses the Display supertrait
    println!("greet:   {}", item.greet()); // uses Greet itself
}

fn main() {
    announce(&Bot(7));
}
```

**Real output:**

```text
display: bot#7
greet:   Hi from bot#7
```

---

## Common Pitfalls

### Pitfall 1: Implementing the subtrait but forgetting the supertrait

Coming from TypeScript, you might write a single `impl` for the "child" trait and expect the parent to come along. Rust requires an `impl` for the supertrait too, and the error lands on the subtrait `impl`.

```rust
use std::fmt;

trait Person: fmt::Display {
    fn full_name(&self) -> String;
}

struct Employee {
    first: String,
    last: String,
}

// We implement Person but NOT Display, the supertrait.
impl Person for Employee {
    fn full_name(&self) -> String {
        format!("{} {}", self.first, self.last)
    }
} // does not compile (error[E0277]: `Employee` doesn't implement `Display`)

fn main() {
    let e = Employee { first: "Ada".into(), last: "Lovelace".into() };
    println!("{}", e.full_name());
}
```

Real compiler error (trimmed to the key lines):

```text
error[E0277]: `Employee` doesn't implement `std::fmt::Display`
  --> src/main.rs:13:17
   |
13 | impl Person for Employee {
   |                 ^^^^^^^^ the trait `std::fmt::Display` is not implemented for `Employee`
   |
note: required by a bound in `Person`
  --> src/main.rs:3:15
   |
 3 | trait Person: fmt::Display {
   |               ^^^^^^^^^^^^ required by this bound in `Person`
```

**Fix:** add `impl fmt::Display for Employee { ... }`. The supertrait bound is not satisfied until that separate `impl` exists.

### Pitfall 2: Defining a supertrait method inside the subtrait's `impl`

The biggest "this is not subclassing" trap: trying to supply the supertrait's method from within the subtrait's `impl` block, as if the methods had merged.

```rust
trait Component {
    fn name(&self) -> &str;
}
trait Plugin: Component {
    fn start(&self) -> String;
}

struct Logger;

// Only impl Plugin, putting `name` here as if Plugin "contained" it.
impl Plugin for Logger {
    fn name(&self) -> &str { "logger" }      // not a Plugin method!
    fn start(&self) -> String { "go".into() }
} // does not compile (error[E0407] and error[E0277])

fn main() {}
```

Real compiler error (two errors, key lines shown):

```text
error[E0407]: method `name` is not a member of trait `Plugin`
  --> src/main.rs:12:5
   |
12 |     fn name(&self) -> &str { "logger" }      // not a Plugin method!
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not a member of trait `Plugin`

error[E0277]: the trait bound `Logger: Component` is not satisfied
  --> src/main.rs:11:17
   |
11 | impl Plugin for Logger {
   |                 ^^^^^^ the trait `Component` is not implemented for `Logger`
```

**Fix:** put `name` in its own `impl Component for Logger { ... }` block, then keep only `start` in `impl Plugin for Logger`. Each trait gets its own `impl`.

### Pitfall 3: Assuming the supertrait must be in scope to call its methods through the subtrait

As with any trait, calling a method requires the trait that *declares* it to be in scope at the call site ([Traits](/09-generics-traits/03-traits/) Pitfall 1). With supertraits this surprises people: if you call a supertrait method (`item.describe()`) on a value you only know to be the subtrait, the **supertrait** must be imported too. The supertrait relationship guarantees the method *exists*, but `use` is still about which trait names are visible. When in doubt, import both traits, or let the compiler's `help:` line tell you exactly which `use` to add.

### Pitfall 4: Reaching for supertraits when a plain bound would do

A supertrait says "*every* implementor of `Sub` must also be `Super`, forever." That is a strong, permanent coupling baked into the trait definition. If you only need `Super`'s capabilities in one function, prefer a local trait bound there (`fn f<T: Sub + Super>(...)`) instead of welding `Super` onto `Sub` for all time. Use a supertrait only when the relationship is genuinely intrinsic: when a `Sub` makes no sense without being a `Super`.

---

## Best Practices

### Use a supertrait only for a real "is-a-prerequisite" relationship

Add `: Super` when every conceivable implementor of your trait truly must also be the supertrait. For example, a `Widget` that cannot render without first being `Drawable`, or a trait whose default methods need `Display`/`Debug` to format `self`. If the dependency is incidental, keep it as a function-level bound instead.

### Require `std` formatting traits to power default methods

Requiring `Display` or `Debug` as a supertrait is a clean, idiomatic pattern: it lets your trait's default methods format `self` without forcing every implementor to reimplement formatting logic. Many real APIs do this (for instance, `std::error::Error: Debug + Display`).

### Combine multiple supertraits with `+`

When a trait needs more than one prerequisite, list them with `+`, exactly like multiple bounds:

```rust playground
use std::fmt::{Debug, Display};

// Multiple supertraits joined with `+`, just like multiple bounds.
trait Loggable: Debug + Display {
    fn log_line(&self) -> String {
        // Both Debug ({:?}) and Display ({}) are guaranteed available.
        format!("[{self}] (debug: {self:?})")
    }
}

#[derive(Debug)]
struct Order {
    id: u32,
    total_cents: u64,
}

impl Display for Order {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Order #{} = ${:.2}", self.id, self.total_cents as f64 / 100.0)
    }
}

impl Loggable for Order {}

fn main() {
    let o = Order { id: 42, total_cents: 1599 };
    println!("{}", o.log_line());
}
```

**Real output:**

```text
[Order #42 = $15.99] (debug: Order { id: 42, total_cents: 1599 })
```

### Keep the supertrait small

A supertrait is a tax every implementor pays. Requiring a tiny, focused supertrait (`Named`, `Display`) is cheap; requiring a sprawling one forces a lot of boilerplate on implementors. This is the interface-segregation instinct from TypeScript, applied to the prerequisite trait.

### Prefer upcasting when you only need the supertrait view

Since Rust 1.86 (2025), you can **upcast** a trait object from a subtrait to its supertrait: a `&dyn Plugin` can be passed where a `&dyn Component` is expected. Lean on that to write functions against the narrowest trait they actually use.

```rust playground
trait Component {
    fn name(&self) -> &str;
}
trait Plugin: Component {
    fn start(&self) -> String;
}

struct Logger;
impl Component for Logger {
    fn name(&self) -> &str { "logger" }
}
impl Plugin for Logger {
    fn start(&self) -> String { "started".into() }
}

// This function only needs the Component view.
fn print_name(c: &dyn Component) {
    println!("component: {}", c.name());
}

fn main() {
    let logger = Logger;
    let p: &dyn Plugin = &logger;
    print_name(p); // upcast &dyn Plugin -> &dyn Component
}
```

**Real output:**

```text
component: logger
```

> **Tip:** Before Rust 1.86 this upcast required a manual helper method (often named `as_component`). On current stable it just works. Trait objects and object safety are covered in [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/).

---

## Real-World Example

A configurable plugin registry, the shape you would find in a build tool or a server's middleware system. `Component` is the base capability (identity and a self-describing line); `Plugin` builds lifecycle behavior on top. The registry stores a heterogeneous list of plugins as trait objects and boots them all.

```rust playground
// Every Plugin is first a Component; Plugin adds lifecycle on top.
trait Component {
    fn name(&self) -> &str;

    fn describe(&self) -> String {
        format!("component '{}'", self.name())
    }
}

trait Plugin: Component {
    fn start(&self) -> String;

    // Default method that uses BOTH a supertrait method (`describe`)
    // and this trait's own method (`start`).
    fn boot_log(&self) -> String {
        format!("booting {} -> {}", self.describe(), self.start())
    }
}

struct Logger {
    level: String,
}

struct Metrics {
    endpoint: String,
}

impl Component for Logger {
    fn name(&self) -> &str {
        "logger"
    }
}

impl Plugin for Logger {
    fn start(&self) -> String {
        format!("logging at level {}", self.level)
    }
}

impl Component for Metrics {
    fn name(&self) -> &str {
        "metrics"
    }
    // Override the default describe just for this type.
    fn describe(&self) -> String {
        format!("metrics exporter -> {}", self.endpoint)
    }
}

impl Plugin for Metrics {
    fn start(&self) -> String {
        String::from("scraping every 15s")
    }
}

// Works over any Plugin. The supertrait bound transitively guarantees
// Component, so `p.name()` (a supertrait method) is callable here.
fn boot_all(plugins: &[Box<dyn Plugin>]) {
    for p in plugins {
        println!("{:<10} | {}", p.name(), p.boot_log());
    }
}

fn main() {
    let plugins: Vec<Box<dyn Plugin>> = vec![
        Box::new(Logger { level: "info".into() }),
        Box::new(Metrics { endpoint: "/metrics".into() }),
    ];
    boot_all(&plugins);
}
```

**Real output:**

```text
logger     | booting component 'logger' -> logging at level info
metrics    | booting metrics exporter -> /metrics -> scraping every 15s
```

Three things to highlight. First, `boot_all` is bounded only by `Plugin`, yet it calls `p.name()` — a `Component` method — because the supertrait relationship transitively supplies it. Second, `Metrics` overrides `describe`, and `boot_log` automatically reflects that override through dynamic dispatch. Third, `Vec<Box<dyn Plugin>>` is a heterogeneous list; `Box` is the heap-allocating smart pointer covered in [Section 10: Smart Pointers](/10-smart-pointers/), and the dynamic-dispatch trade-offs are in [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/).

---

## Further Reading

### Official documentation

- [The Rust Book — Using Supertraits to Require One Trait's Functionality Within Another Trait](https://doc.rust-lang.org/book/ch20-02-advanced-traits.html#using-supertraits-to-require-one-traits-functionality-within-another-trait)
- [The Rust Reference — Supertraits](https://doc.rust-lang.org/reference/items/traits.html#supertraits)
- [Rust by Example — Supertraits](https://doc.rust-lang.org/rust-by-example/trait/supertraits.html)
- [Rust 1.86 release notes — trait upcasting](https://blog.rust-lang.org/2025/04/03/Rust-1.86.0.html)

### Related sections in this guide

- [Section 09 overview](/09-generics-traits/): the full map of generics and traits
- [Traits](/09-generics-traits/03-traits/): defining and implementing a trait; the `impl Trait for Type` syntax
- [Trait Bounds](/09-generics-traits/05-trait-bounds/): `<T: Trait>` bounds, multiple bounds, and `where` clauses (a supertrait *is* a bound on `Self`)
- [Trait Methods](/09-generics-traits/04-trait-methods/): required vs provided methods and overriding defaults
- [Default Method Implementations](/09-generics-traits/08-default-impls/): default method bodies, which supertraits frequently power
- [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/): `&dyn Trait`, object safety, and trait upcasting
- [Marker Traits](/09-generics-traits/11-marker-traits/): `Send`/`Sync`/`Copy`, which often appear as supertrait bounds
- [Generic Functions](/09-generics-traits/00-generic-functions/): monomorphization vs TypeScript type erasure
- [Section 01: Getting Started](/01-getting-started/): `cargo new` and the toolchain
- [Section 02: Basics](/02-basics/): types, output, and `format!`
- [Section 05: Ownership](/05-ownership/): what `&self` borrows mean
- [Section 10: Smart Pointers](/10-smart-pointers/): `Box<dyn Trait>` and heap allocation

---

## Exercises

### Exercise 1: A subtrait that requires a supertrait

**Difficulty:** Easy

**Objective:** Practice the `trait Sub: Super` syntax and the two-`impl` workflow.

**Instructions:** Define a trait `Named` with one required method `name(&self) -> String`. Define a second trait `Animal: Named` with a required method `sound(&self) -> String` and a **provided** method `speak(&self) -> String` that returns `"{name} says {sound}"` by calling both. Implement both traits for a `Dog` struct (name `"Rex"`, sound `"woof"`). In `main`, build a `Dog` and print `speak()`.

```rust playground
trait Named {
    // TODO: name(&self) -> String
}

trait Animal: Named {
    // TODO: required sound(&self) -> String
    // TODO: provided speak(&self) -> String calling self.name() and self.sound()
}

struct Dog;

// TODO: impl Named for Dog
// TODO: impl Animal for Dog

fn main() {
    // TODO: build a Dog, print speak()
}
```

<details>
<summary>Solution</summary>

```rust playground
trait Named {
    fn name(&self) -> String;
}

trait Animal: Named {
    fn sound(&self) -> String;

    fn speak(&self) -> String {
        format!("{} says {}", self.name(), self.sound())
    }
}

struct Dog;

impl Named for Dog {
    fn name(&self) -> String {
        String::from("Rex")
    }
}

impl Animal for Dog {
    fn sound(&self) -> String {
        String::from("woof")
    }
}

fn main() {
    let d = Dog;
    println!("{}", d.speak());
}
```

**Output:**

```text
Rex says woof
```

Note that `Named` needs its own `impl` block; `Animal`'s `impl` cannot supply `name`. That separation is the heart of supertraits.

</details>

### Exercise 2: A supertrait from the standard library

**Difficulty:** Medium

**Objective:** Require `Display` as a supertrait so a default method can format `self`.

**Instructions:** Define a trait `Priced: Display` with a required method `price_cents(&self) -> u64` and a provided method `receipt(&self) -> String` that prints `"{self}: $X.XX"` (dollars to two decimals). Implement `Display` and `Priced` for a `Coffee { size: String }` where `"small"` costs `350`, `"large"` costs `525`, and anything else costs `450`. In `main`, print the receipt for a large coffee.

```rust playground
use std::fmt::Display;

trait Priced: Display {
    // TODO: required price_cents(&self) -> u64
    // TODO: provided receipt(&self) -> String using "{self}" and price_cents()
}

struct Coffee {
    size: String,
}

// TODO: impl Display for Coffee  (e.g. "large coffee")
// TODO: impl Priced for Coffee

fn main() {
    // TODO: print the receipt for a "large" coffee
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::fmt::Display;

trait Priced: Display {
    fn price_cents(&self) -> u64;

    fn receipt(&self) -> String {
        format!("{self}: ${:.2}", self.price_cents() as f64 / 100.0)
    }
}

struct Coffee {
    size: String,
}

impl Display for Coffee {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} coffee", self.size)
    }
}

impl Priced for Coffee {
    fn price_cents(&self) -> u64 {
        match self.size.as_str() {
            "small" => 350,
            "large" => 525,
            _ => 450,
        }
    }
}

fn main() {
    let c = Coffee { size: String::from("large") };
    println!("{}", c.receipt());
}
```

**Output:**

```text
large coffee: $5.25
```

The `{self}` in `receipt` only works because `Priced: Display` guarantees every implementor is also `Display`.

</details>

### Exercise 3: Multiple supertraits behind a generic bound

**Difficulty:** Hard

**Objective:** Combine multiple supertraits (`Debug + Display`) and consume the subtrait through a generic function, relying on the bound transitively supplying both supertraits.

**Instructions:** Define a trait `Serialize: Debug + Display` with a provided method `to_record(&self) -> String` that returns `"{self} | {self:?}"` (Display then Debug). Implement `Display` for a `User { id: u32, handle: String }` (format `"@handle (#id)"`), derive `Debug`, and give it an empty `impl Serialize for User {}`. Write a generic function `dump<T: Serialize>(item: &T)` that prints `item.to_record()`. In `main`, dump a user.

```rust playground
use std::fmt::{Debug, Display};

trait Serialize: Debug + Display {
    // TODO: provided to_record(&self) -> String returning "{self} | {self:?}"
}

// TODO: derive Debug, define User { id: u32, handle: String }
// TODO: impl Display for User -> "@handle (#id)"
// TODO: impl Serialize for User

fn dump<T: Serialize>(item: &T) {
    // TODO: print item.to_record()
}

fn main() {
    // TODO: build a User and dump it
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::fmt::{Debug, Display};

trait Serialize: Debug + Display {
    fn to_record(&self) -> String {
        format!("{self} | {self:?}")
    }
}

#[derive(Debug)]
struct User {
    id: u32,
    handle: String,
}

impl Display for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "@{} (#{})", self.handle, self.id)
    }
}

impl Serialize for User {}

// Generic over any Serialize; the bound transitively guarantees Display + Debug.
fn dump<T: Serialize>(item: &T) {
    println!("{}", item.to_record());
}

fn main() {
    let u = User { id: 1, handle: String::from("ada") };
    dump(&u);
}
```

**Output:**

```text
@ada (#1) | User { id: 1, handle: "ada" }
```

`dump` is bounded only by `Serialize`, yet `to_record` uses both `{self}` (Display) and `{self:?}` (Debug). The single `T: Serialize` bound carries both supertraits with it — no need to repeat `+ Display + Debug` on the function.

</details>
