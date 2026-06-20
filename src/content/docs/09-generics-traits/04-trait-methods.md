---
title: "Trait Methods: Required vs Provided"
description: "Rust trait methods are required (signature only) or provided (default body), like an abstract class but with no super, so one trait holds contract and defaults."
---

A TypeScript `interface` can only declare method *signatures*; the bodies always live somewhere else (a class, an object literal). A Rust **trait** can do both: it can declare a method that every implementor *must* write (a **required method**), and it can ship a method that already has a body (a **provided**, or **default**, method). This file is about that split: how you write each kind, how you call them, and how (and whether) you can override a default.

---

## Quick Overview

A trait method is either **required** (just a signature followed by `;` — every implementor supplies the body) or **provided** (the trait ships a default body that implementors inherit for free and may optionally override). Provided methods are how a trait like the standard library's `Iterator` gives you ~70 methods (`map`, `filter`, `sum`, ...) after you write a single required one (`next`). The closest TypeScript analogy is an abstract class with some abstract methods and some concrete ones. But, unlike TypeScript classes, there is no `super` to reach back into the default once you override it.

---

## TypeScript/JavaScript Example

In TypeScript you reach for an `abstract class` when you want to mix "you must implement this" with "here's a sensible default". Below, every logger *must* provide `name()`, but `level()` and `log()` come with working defaults that subclasses inherit.

```typescript
// TypeScript - abstract class: mix of abstract and concrete methods
abstract class Logger {
  // "Required": subclasses MUST implement this (no body).
  abstract name(): string;

  // "Provided": a default the subclass inherits unless it overrides.
  level(): string {
    return "info";
  }

  // A provided method built on top of the other two.
  log(message: string): void {
    console.log(`[${this.level()}] ${this.name()}: ${message}`);
  }
}

class ConsoleLogger extends Logger {
  name(): string {
    return "console";
  }
  // Inherits level() and log() unchanged.
}

class AuditLogger extends Logger {
  name(): string {
    return "audit";
  }
  // Override the default, and still reach the parent via super.
  level(): string {
    return "audit-" + super.level(); // -> "audit-info"
  }
}

const c = new ConsoleLogger();
c.log("server started"); // [info] console: server started

const a = new AuditLogger();
a.log("user 42 deleted record 7"); // [audit-info] audit: ...
```

Two TypeScript abilities to keep in mind, because Rust treats them differently:

- A plain `interface` cannot carry method bodies at all — you need an `abstract class` (or a mixin) to get defaults.
- An overriding method can call `super.level()` to reuse the parent implementation.

---

## Rust Equivalent

A single Rust `trait` does the job of that abstract class. A method that ends in `;` is **required**; a method with a `{ ... }` body is **provided**.

```rust playground
trait Logger {
    // REQUIRED: no body, so every implementor must supply it.
    fn name(&self) -> String;

    // PROVIDED (default): has a body. Implementors keep it or override it.
    fn level(&self) -> &str {
        "info"
    }

    // PROVIDED: builds on the required + provided methods above.
    fn log(&self, message: &str) {
        println!("[{}] {}: {}", self.level(), self.name(), message);
    }
}

struct ConsoleLogger;

impl Logger for ConsoleLogger {
    // Only the required method is mandatory.
    fn name(&self) -> String {
        "console".to_string()
    }
    // level() and log() are inherited from the defaults.
}

struct AuditLogger;

impl Logger for AuditLogger {
    fn name(&self) -> String {
        "audit".to_string()
    }

    // Override a provided method. (Note: there is no `super` to reuse
    // the default body — see Common Pitfalls.)
    fn level(&self) -> &str {
        "audit"
    }
}

fn main() {
    let c = ConsoleLogger;
    c.log("server started");

    let a = AuditLogger;
    a.log("user 42 deleted record 7");
    println!("audit level = {}", a.level());
}
```

Real output:

```text
[info] console: server started
[audit] audit: user 42 deleted record 7
audit level = audit
```

> **Note:** This file assumes you have already met the basics of declaring and implementing a trait. If `impl Logger for ConsoleLogger` looks unfamiliar, read [Traits](/09-generics-traits/03-traits/) first.

---

## Detailed Explanation

### Required methods: a signature and a semicolon

```rust
fn name(&self) -> String;
```

The `;` where a body would go is the whole story: this is a **required method**. The trait promises every type implementing `Logger` will have a `name(&self) -> String`, but it refuses to guess what that string is. This is exactly an `abstract` method or a bare `interface` member in TypeScript: a contract with no implementation.

If an `impl` block forgets a required method, the program does not compile. That is the key safety difference from a TypeScript `abstract class`: there is no way to "forget for now and crash later", because there is no runtime later.

### Provided methods: a signature and a body

```rust
fn level(&self) -> &str {
    "info"
}
```

Because `level` has a body, it is a **provided method**. An `impl` block that says nothing about `level` still gets this version. That is why `ConsoleLogger`'s `impl` block contains *only* `name` yet `console_logger.level()` and `console_logger.log(...)` both work.

### Defaults can call other trait methods — including required ones

```rust
fn log(&self, message: &str) {
    println!("[{}] {}: {}", self.level(), self.name(), message);
}
```

This is the pattern that makes provided methods pull their weight. `log` is written once, in the trait, in terms of `self.level()` and `self.name()`. When `log` runs, `self.name()` dispatches to *whatever the concrete type defined*. So the trait author writes the orchestration once, and each implementor only fills in the small required pieces. The standard library's `Iterator` is the canonical example: you implement the one required method `next`, and dozens of provided methods (`map`, `filter`, `sum`, `collect`, ...) are written in terms of it.

```rust playground
// Implement only `next`; inherit `map`, `sum`, `collect`, and friends.
struct CountUp { n: u32, max: u32 }

impl Iterator for CountUp {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {   // the ONE required method
        if self.n < self.max {
            self.n += 1;
            Some(self.n)
        } else {
            None
        }
    }
}

fn main() {
    let total: u32 = CountUp { n: 0, max: 5 }.sum();   // provided
    let doubled: Vec<u32> = CountUp { n: 0, max: 5 }
        .map(|x| x * 2)                                // provided
        .collect();                                    // provided
    println!("sum = {total}, doubled = {doubled:?}");
}
```

Real output:

```text
sum = 15, doubled = [2, 4, 6, 8, 10]
```

### Calling trait methods three ways

Most of the time you call a trait method with ordinary method syntax (`value.method()`), and Rust figures out which trait it belongs to. But there are two more explicit forms, useful when a name is ambiguous or you want to be precise:

```rust playground
trait Summary {
    fn title(&self) -> String;       // required
    fn author(&self) -> String;      // required

    // provided default that calls TWO required methods
    fn summarize(&self) -> String {
        format!("{} (by {})", self.title(), self.author())
    }
}

struct Article {
    headline: String,
    writer: String,
}

impl Summary for Article {
    fn title(&self) -> String {
        self.headline.clone()
    }
    fn author(&self) -> String {
        self.writer.clone()
    }
}

fn main() {
    let a = Article {
        headline: "Rust 1.96 released".to_string(),
        writer: "The Rust Team".to_string(),
    };

    // 1. Method-call syntax (what you'll write 99% of the time)
    println!("{}", a.summarize());

    // 2. Trait-qualified call: name the trait, pass the receiver explicitly
    println!("{}", Summary::summarize(&a));

    // 3. Fully-qualified syntax: name both the type AND the trait
    println!("{}", <Article as Summary>::summarize(&a));
}
```

Real output (all three lines identical):

```text
Rust 1.96 released (by The Rust Team)
Rust 1.96 released (by The Rust Team)
Rust 1.96 released (by The Rust Team)
```

Form 1 is idiomatic. Forms 2 and 3 exist for disambiguation. For instance, when a type implements two traits that both define a method named `summarize`, `<Article as Summary>::summarize(&a)` says exactly which one you mean. (See [Traits](/09-generics-traits/03-traits/) and [Trait Bounds](/09-generics-traits/05-trait-bounds/) for more on dispatch.)

### Overriding a default

An `impl` block overrides a provided method simply by defining it — same name, same signature, a new body. In the `Logger` example, `AuditLogger` defines its own `level()`, so its `log()` (still the default) prints `[audit]` instead of `[info]`. The override is total: once you write your own `level`, the trait's default body is no longer reachable from inside `AuditLogger`. There is no `super`.

---

## Key Differences

| Concept | TypeScript | Rust |
| --- | --- | --- |
| "Must implement" method | `abstract` method, or any `interface` member | **Required** method: signature + `;` |
| "Has a default" method | concrete method on an `abstract class` | **Provided** method: signature + `{ body }` |
| Pure contract (no bodies) | `interface` | A trait with only required methods |
| Mix contract + defaults | `abstract class` | One trait does both |
| Forgetting a required method | runtime error / `any` escape hatch | **compile error** (`error[E0046]`) |
| Reuse parent body after override | `super.method()` | **Not available** — refactor instead |
| Calling a specific implementation | `super`, casts | trait-qualified / fully-qualified syntax |
| Where defaults are dispatched | `this` (dynamic) | `self`, statically by default; dynamic via `dyn` |

The single biggest mental adjustment: **a trait is one declaration that can hold both halves**, and there is **no `super`**. In TypeScript you separate "the contract" (`interface`) from "the partial implementation" (`abstract class`); in Rust those collapse into one trait. And where TypeScript lets an override delegate back up the chain with `super.level()`, Rust gives you no built-in hook into the overridden default. The idiom is to factor the shared work into its own method (shown below) so both the default and any override can call it.

> **Note:** Default trait methods do **not** create an inheritance hierarchy. A trait override replaces the default for that type only; there is no chain of parents to climb. Rust models "is-a-kind-of" relationships through composition and [supertraits](/09-generics-traits/09-supertraits/), not class inheritance.

---

## Common Pitfalls

### Pitfall 1: Forgetting a required method

If your `impl` block leaves out a required method, the compiler stops you. There is no "implement it later" the way a partially-typed TypeScript object might slide by.

```rust
trait Greeter {
    fn name(&self) -> String;          // required, no default
    fn greet(&self) -> String {        // provided
        format!("Hello, {}!", self.name())
    }
}

struct Robot;

impl Greeter for Robot {
    // does not compile (error[E0046]): forgot to implement `name`
}

fn main() {
    let r = Robot;
    println!("{}", r.greet());
}
```

The real error:

```text
error[E0046]: not all trait items implemented, missing: `name`
  --> src/main.rs:10:1
   |
 2 |     fn name(&self) -> String;          // required, no default
   |     ------------------------- `name` from trait
...
10 | impl Greeter for Robot {
   | ^^^^^^^^^^^^^^^^^^^^^^ missing `name` in implementation
```

The fix is to add the missing method. Notice the compiler points at exactly which trait item is missing: `name` is required (no body), but `greet` is provided, so leaving `greet` out is fine.

### Pitfall 2: Reaching for `super` inside an override

Coming from TypeScript, the natural instinct is to override `render` and call the parent's version. There is no `super` in Rust, and `self.render()` calls *the very method you are writing* — infinite recursion.

```rust
trait Renderer {
    fn render(&self) -> String {
        "<default render>".to_string()
    }
}

struct Fancy;

impl Renderer for Fancy {
    fn render(&self) -> String {
        // This calls THIS method again, not the trait's default.
        let base = self.render(); // unconditional recursion!
        format!("** {} **", base)
    }
}

fn main() {
    let f = Fancy;
    println!("{}", f.render());
}
```

The compiler catches the mistake at compile time with a default-on warning (the program would otherwise overflow the stack at runtime):

```text
warning: function cannot return without recursing
  --> src/main.rs:10:5
   |
10 |     fn render(&self) -> String {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^ cannot return without recursing
11 |         // This calls THIS method again, not the trait's default.
12 |         let base = self.render(); // unconditional recursion!
   |                    ------------- recursive call site
   |
   = help: a `loop` may express intention better if this is on purpose
   = note: `#[warn(unconditional_recursion)]` on by default
```

The idiomatic fix is to extract the shared logic into its own method that *both* the default and the override can call:

```rust playground
trait Renderer {
    // The shared logic lives in its own method, so both the default
    // and any override can reuse it without recursion.
    fn render_inner(&self) -> String;

    fn render(&self) -> String {
        self.render_inner()
    }
}

struct Plain;
impl Renderer for Plain {
    fn render_inner(&self) -> String {
        "plain content".to_string()
    }
    // keeps the default render()
}

struct Fancy;
impl Renderer for Fancy {
    fn render_inner(&self) -> String {
        "fancy content".to_string()
    }
    fn render(&self) -> String {
        // reuse the shared method, then decorate
        format!("** {} **", self.render_inner())
    }
}

fn main() {
    println!("{}", Plain.render());
    println!("{}", Fancy.render());
}
```

Real output:

```text
plain content
** fancy content **
```

### Pitfall 3: Expecting a default to "see" a struct field

A default method can only call other trait methods and use its parameters; it has no idea what fields the implementing struct has, because different implementors have different fields. If a default needs a piece of per-type data, expose it through a *required* method (often a small getter) and have the default call that. This is precisely what `log` does with `self.name()` above.

### Pitfall 4: Changing the signature when overriding

An override must match the trait method's signature exactly — same parameters, same return type, same receiver (`&self` vs `&mut self`). Changing it does not "overload"; it fails to compile because it no longer satisfies the trait. If you want a different shape, that is a different method (or a different trait).

---

## Best Practices

- **Make a method *provided* when a sensible default exists; make it *required* when only the implementor can know the answer.** A getter like `name()` is required; an orchestration like `log()` or `summarize()` is provided.
- **Write provided methods in terms of a small set of required ones.** This is the "implement `next`, get everything else" pattern. Keep the required surface area minimal so implementors have little to write.
- **Factor shared logic into its own method instead of reaching for `super`.** When several implementors want to extend a default, give them a helper method to call (Pitfall 2's fix).
- **Don't add a default just because you can.** A default that is wrong for most implementors is worse than a required method, because implementors might silently inherit incorrect behavior. Require it instead, and let the compiler force a decision.
- **Use plain method-call syntax** (`value.method()`) unless a name is genuinely ambiguous, in which case reach for `<Type as Trait>::method(...)`.
- **Document which methods are meant to be overridden.** In `///` doc comments, note when a default is "override me to customize X" versus "you shouldn't need to touch this".

---

## Real-World Example

A small, production-flavored validation framework. Each rule *must* supply its core `check`, but gets a free `is_valid` boolean helper and a `validate` method that decorates errors with the rule's name: both provided, both written once in the trait.

```rust playground
/// A validation rule applied to a string field (e.g. a form input).
trait Validator {
    /// REQUIRED: the core check. Returns `Ok(())` or an error message.
    fn check(&self, input: &str) -> Result<(), String>;

    /// PROVIDED: a human-readable rule name, used in default reporting.
    fn rule_name(&self) -> &str {
        "rule"
    }

    /// PROVIDED: turn `check` into a yes/no answer.
    fn is_valid(&self, input: &str) -> bool {
        self.check(input).is_ok()
    }

    /// PROVIDED: validate and prefix any error with the rule name.
    fn validate(&self, input: &str) -> Result<(), String> {
        self.check(input)
            .map_err(|e| format!("[{}] {}", self.rule_name(), e))
    }
}

struct NonEmpty;
impl Validator for NonEmpty {
    fn check(&self, input: &str) -> Result<(), String> {
        if input.trim().is_empty() {
            Err("must not be empty".to_string())
        } else {
            Ok(())
        }
    }
    fn rule_name(&self) -> &str {
        "non-empty"
    }
}

struct MaxLen(usize);
impl Validator for MaxLen {
    fn check(&self, input: &str) -> Result<(), String> {
        if input.chars().count() > self.0 {
            Err(format!("must be at most {} chars", self.0))
        } else {
            Ok(())
        }
    }
    fn rule_name(&self) -> &str {
        "max-len"
    }
    // Keeps the default `validate` and `is_valid`.
}

/// Run every rule against `input`, stopping at the first failure.
fn run(rules: &[&dyn Validator], input: &str) {
    print!("{input:?} => ");
    for rule in rules {
        if let Err(msg) = rule.validate(input) {
            println!("FAIL {msg}");
            return;
        }
    }
    println!("OK");
}

fn main() {
    let rules: Vec<&dyn Validator> = vec![&NonEmpty, &MaxLen(8)];
    run(&rules, "alice");
    run(&rules, "");
    run(&rules, "this-is-way-too-long");

    // `is_valid` comes for free from the default impl.
    println!("MaxLen(8).is_valid(\"ok\") = {}", MaxLen(8).is_valid("ok"));
}
```

Real output:

```text
"alice" => OK
"" => FAIL [non-empty] must not be empty
"this-is-way-too-long" => FAIL [max-len] must be at most 8 chars
MaxLen(8).is_valid("ok") = true
```

Each new rule only writes `check` (and an optional `rule_name`); `is_valid` and `validate` are inherited. The `&[&dyn Validator]` slice stores different rule types behind a trait object so they can be iterated uniformly. For the details of that mechanism, see [Trait Objects](/09-generics-traits/06-trait-objects/).

---

## Further Reading

### Official Documentation

- [The Rust Book - Default Implementations](https://doc.rust-lang.org/book/ch10-02-traits.html#default-implementations)
- [The Rust Book - Traits: Defining Shared Behavior](https://doc.rust-lang.org/book/ch10-02-traits.html)
- [Rust by Example - Traits](https://doc.rust-lang.org/rust-by-example/trait.html)
- [Rust Reference - Trait items and provided methods](https://doc.rust-lang.org/reference/items/traits.html)
- [`std::iter::Iterator`](https://doc.rust-lang.org/std/iter/trait.Iterator.html): the canonical "one required method, many provided" trait

### Related Sections in This Guide

- [Traits](/09-generics-traits/03-traits/) — interfaces become traits; defining and implementing a trait
- [Default Implementations](/09-generics-traits/08-default-impls/) — a deeper look at how provided methods cut boilerplate
- [Trait Bounds](/09-generics-traits/05-trait-bounds/) — `<T: Trait>` so generic code can call these methods
- [Trait Objects](/09-generics-traits/06-trait-objects/): `&dyn Trait` / `Box<dyn Trait>` and dynamic dispatch (used in the validator example)
- [Supertraits](/09-generics-traits/09-supertraits/) — requiring one trait for another (Rust's answer to "inheritance")
- [`impl` Trait](/09-generics-traits/07-impl-trait/) — returning and accepting "some type that implements this trait"
- [Operator Overloading](/09-generics-traits/10-operator-overloading/): traits like `Add` whose methods you implement
- [Methods and `impl` Blocks](/06-data-structures/05-impl-blocks/) — `&self` / `&mut self` / `self` receivers, the foundation for trait method signatures
- [Error Handling](/08-error-handling/) — the `Result<(), String>` returned by the validator
- [Smart Pointers](/10-smart-pointers/): `Box<dyn Trait>` for owning trait objects

---

## Exercises

### Exercise 1: A provided method that calls required ones

**Difficulty:** Easy

**Objective:** Practice the "required getters + provided orchestration" pattern, and override a default.

**Instructions:** Complete the trait so `describe` (provided) prints `"<name> says <noise>"` using the two required methods. Implement `Animal` for `Dog` (name `"Rex"`, noise `"woof"`) keeping the default `describe`, and for `Cat` (name `"Whiskers"`, noise `"meow"`) **overriding** `describe` to print `"The cat <name> disdainfully says <noise>"`.

```rust
trait Animal {
    fn name(&self) -> String;   // required
    fn noise(&self) -> String;  // required
    fn describe(&self) -> String {
        // TODO: "<name> says <noise>"
        /* ??? */
    }
}

struct Dog;
struct Cat;
// TODO: impl Animal for Dog and Cat

fn main() {
    println!("{}", Dog.describe());
    println!("{}", Cat.describe());
}
```

<details>
<summary>Solution</summary>

```rust playground
trait Animal {
    fn name(&self) -> String;
    fn noise(&self) -> String;
    fn describe(&self) -> String {
        format!("{} says {}", self.name(), self.noise())
    }
}

struct Dog;
impl Animal for Dog {
    fn name(&self) -> String { "Rex".to_string() }
    fn noise(&self) -> String { "woof".to_string() }
    // keeps the default describe()
}

struct Cat;
impl Animal for Cat {
    fn name(&self) -> String { "Whiskers".to_string() }
    fn noise(&self) -> String { "meow".to_string() }
    fn describe(&self) -> String {
        format!("The cat {} disdainfully says {}", self.name(), self.noise())
    }
}

fn main() {
    println!("{}", Dog.describe());
    println!("{}", Cat.describe());
}
```

Output:

```text
Rex says woof
The cat Whiskers disdainfully says meow
```

`Dog` only writes the two required getters and inherits `describe`. `Cat` overrides `describe` with a new body — and notice there is no `super`: it rebuilds the string itself by calling `self.name()` and `self.noise()` directly.

</details>

### Exercise 2: A default value plus an override

**Difficulty:** Medium

**Objective:** Ship a provided method with a useful default constant, then override it in one implementor.

**Instructions:** Build a tiny HTTP-handler trait. `body` is required. `status` is provided and defaults to `200`. `respond` is provided and returns `"HTTP <status> | <body>"`. Implement `Home` (body `"Welcome"`, default status) and `NotFound` (body `"Page not found"`, status `404`).

```rust
trait Handler {
    fn body(&self) -> String;       // required
    fn status(&self) -> u16 {       // provided
        /* ??? */
    }
    fn respond(&self) -> String {   // provided
        // TODO: "HTTP <status> | <body>"
        /* ??? */
    }
}

// TODO: struct Home; struct NotFound; + impls

fn main() {
    // expected:
    // HTTP 200 | Welcome
    // HTTP 404 | Page not found
}
```

<details>
<summary>Solution</summary>

```rust playground
trait Handler {
    fn body(&self) -> String;
    fn status(&self) -> u16 {
        200
    }
    fn respond(&self) -> String {
        format!("HTTP {} | {}", self.status(), self.body())
    }
}

struct Home;
impl Handler for Home {
    fn body(&self) -> String { "Welcome".to_string() }
    // inherits status() == 200 and respond()
}

struct NotFound;
impl Handler for NotFound {
    fn body(&self) -> String { "Page not found".to_string() }
    fn status(&self) -> u16 { 404 } // override the default
}

fn main() {
    println!("{}", Home.respond());
    println!("{}", NotFound.respond());
}
```

Output:

```text
HTTP 200 | Welcome
HTTP 404 | Page not found
```

`respond` is written once in the trait. Because it calls `self.status()`, overriding `status` in `NotFound` automatically changes what `respond` prints — without `NotFound` touching `respond` at all.

</details>

### Exercise 3: One required method powering a provided one (Iterator-style)

**Difficulty:** Medium/Hard

**Objective:** Reproduce the standard library's "implement one method, get more for free" design, and override the provided method for a fast path.

**Instructions:** Define a `Counter` trait whose only required method is `next_value(&mut self) -> u64`. Provide a default `take(&mut self, n) -> Vec<u64>` that calls `next_value` `n` times. Implement `Naturals` (counts `0, 1, 2, ...`) keeping the default `take`. Implement `Evens` (counts `0, 2, 4, ...`) and **override** `take` with a closed-form computation that avoids the loop while producing the same result and advancing the counter correctly.

```rust
trait Counter {
    fn next_value(&mut self) -> u64; // required
    fn take(&mut self, n: usize) -> Vec<u64> {
        // TODO: call next_value() n times, collect into a Vec
        /* ??? */
    }
}

struct Naturals { current: u64 }
struct Evens { current: u64 }
// TODO: impls

fn main() {
    let mut nats = Naturals { current: 0 };
    println!("{:?}", nats.take(5)); // [0, 1, 2, 3, 4]

    let mut evens = Evens { current: 0 };
    println!("{:?}", evens.take(5)); // [0, 2, 4, 6, 8]
}
```

<details>
<summary>Solution</summary>

```rust playground
trait Counter {
    fn next_value(&mut self) -> u64;

    fn take(&mut self, n: usize) -> Vec<u64> {
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.next_value());
        }
        out
    }
}

struct Naturals { current: u64 }
impl Counter for Naturals {
    fn next_value(&mut self) -> u64 {
        let v = self.current;
        self.current += 1;
        v
    }
    // keeps the default take()
}

struct Evens { current: u64 }
impl Counter for Evens {
    fn next_value(&mut self) -> u64 {
        let v = self.current;
        self.current += 2;
        v
    }
    // Override take() with a closed form; still advances `current`.
    fn take(&mut self, n: usize) -> Vec<u64> {
        let start = self.current;
        let out: Vec<u64> = (0..n as u64).map(|i| start + i * 2).collect();
        self.current += (n as u64) * 2;
        out
    }
}

fn main() {
    let mut nats = Naturals { current: 0 };
    println!("{:?}", nats.take(5));

    let mut evens = Evens { current: 0 };
    println!("{:?}", evens.take(5));
}
```

Output:

```text
[0, 1, 2, 3, 4]
[0, 2, 4, 6, 8]
```

`Naturals` writes only the one required method and inherits the loop-based `take`. `Evens` proves overrides are free to use a completely different algorithm, as long as the observable result and the state change match — exactly how the standard library overrides `Iterator` defaults (e.g. `size_hint`, `count`) for performance.

</details>
