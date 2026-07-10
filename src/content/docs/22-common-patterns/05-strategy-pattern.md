---
title: "The Strategy Pattern in Rust"
description: "The strategy pattern in Rust three ways: plain closures, generics for static dispatch, or Box<dyn Trait> for runtime choice, versus a TypeScript interface."
---

In TypeScript you implement the **strategy pattern** with an interface and a family of classes: define `interface DiscountStrategy`, write a class per algorithm, and inject the chosen one into a context object. Rust supports that exact shape with **trait objects**, but it also gives you two other tools that a TypeScript developer usually reaches for last: **generics** (the strategy is fixed at compile time) and **plain closures** (the strategy is just a function value). This file is about choosing among those three, and why the closure version is often the most idiomatic answer in Rust.

---

## Quick Overview

The **strategy pattern** lets you swap an algorithm at runtime without changing the code that uses it: a pricing engine that can apply different discount rules, a load balancer that can route requests by different policies, a serializer that can emit JSON or YAML. In TypeScript this is almost always "an interface plus a class per algorithm." Rust offers three idiomatic encodings, and picking the right one is the whole skill:

- **Plain closures** (`impl Fn`, `Box<dyn Fn>`): the lightest option, ideal when the strategy is "just a function" with no extra state or methods.
- **Generics** (`<S: Strategy>`): the strategy is chosen at compile time, monomorphized to zero-cost static dispatch, like a TypeScript generic but with no type erasure.
- **Trait objects** (`Box<dyn Strategy>`, `&dyn Strategy`): the closest match to the OO version, chosen at runtime via a vtable, used when the set of strategies is open or selected from config.

> **Note:** This page covers strategy specifically. The mechanics of `dyn Trait` and static-vs-dynamic dispatch are in [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/) and [Section 09: Trait Bounds](/09-generics-traits/05-trait-bounds/). For building the concrete strategy values, see the sibling [The Factory Pattern](/22-common-patterns/08-factory-pattern/); for wiring strategies into a larger system, see [Dependency Injection in Rust](/22-common-patterns/09-dependency-injection/).

---

## TypeScript/JavaScript Example

A realistic checkout: the cart total is the same, but the discount applied depends on a promotion chosen at runtime. The textbook approach is an interface and a class per strategy, injected into a `Checkout` context.

```typescript
// TypeScript - the classic OO strategy pattern
interface DiscountStrategy {
  apply(subtotal: number): number;
  name(): string;
}

class NoDiscount implements DiscountStrategy {
  apply(subtotal: number): number {
    return subtotal;
  }
  name(): string {
    return "none";
  }
}

class PercentOff implements DiscountStrategy {
  constructor(private fraction: number) {}
  apply(subtotal: number): number {
    return subtotal * (1 - this.fraction);
  }
  name(): string {
    return `${this.fraction * 100}% off`;
  }
}

class FlatOff implements DiscountStrategy {
  constructor(private amount: number) {}
  apply(subtotal: number): number {
    return Math.max(0, subtotal - this.amount);
  }
  name(): string {
    return `$${this.amount} off`;
  }
}

// The context holds whichever strategy was injected.
class Checkout {
  constructor(private discount: DiscountStrategy) {}
  total(subtotal: number): number {
    return Math.round(this.discount.apply(subtotal) * 100) / 100;
  }
}

const chosen = new PercentOff(0.1); // picked from config at runtime
const checkout = new Checkout(chosen);
console.log(checkout.total(100)); // 90
```

**Output (Node v22):**

```text
90
```

Two things worth noticing for the comparison ahead. First, every `DiscountStrategy` is dispatched dynamically; `this.discount.apply(...)` is a runtime property lookup, the only dispatch JavaScript has. Second, TypeScript developers often write a whole class (`PercentOff`, `FlatOff`) where the "strategy" is really just one function. In JavaScript you could already pass a bare `(subtotal: number) => number`, and Rust leans hard into that instinct.

---

## Rust Equivalent

Here is the same checkout three ways. Start with the most idiomatic Rust answer — a **closure** — then build up to generics and trait objects so you can see exactly what each buys you.

### Version 1: plain closures (the idiomatic default)

When a strategy is "just a function," make it a function. Any `Fn(f64) -> f64` is a discount strategy; no trait, no struct, no class.

```rust playground
// Rust - the strategy is a closure. `impl Fn` means "any function-like value".
fn checkout(subtotal: f64, discount: impl Fn(f64) -> f64) -> f64 {
    let after = discount(subtotal);
    (after * 100.0).round() / 100.0
}

fn main() {
    let none = |s: f64| s;
    // A closure that *builds* a closure — strategies parameterized by data.
    let percent_off = |pct: f64| move |s: f64| s * (1.0 - pct);
    let flat_off = |amount: f64| move |s: f64| (s - amount).max(0.0);

    println!("none:    {:.2}", checkout(100.0, none));
    println!("10% off: {:.2}", checkout(100.0, percent_off(0.10)));
    println!("$15 off: {:.2}", checkout(100.0, flat_off(15.0)));

    // Choosing a strategy at runtime: box it, because each closure is a distinct type.
    let strategy: Box<dyn Fn(f64) -> f64> = if true {
        Box::new(percent_off(0.25))
    } else {
        Box::new(flat_off(15.0))
    };
    println!("boxed:   {:.2}", checkout(80.0, &strategy));
}
```

**Real output:**

```text
none:    100.00
10% off: 90.00
$15 off: 85.00
boxed:   60.00
```

### Version 2: generics (compile-time strategy, zero-cost)

When a strategy has more than one method or carries state worth naming, define a **trait** and make the context **generic** over it. The compiler generates a specialized copy per concrete strategy (monomorphization), so calls are inlined with no vtable.

```rust playground
// Rust - the strategy is a trait; the context is generic over it (static dispatch).
trait Validator {
    fn validate(&self, input: &str) -> Result<(), String>;
}

struct NonEmpty;
struct MaxLen(usize);

impl Validator for NonEmpty {
    fn validate(&self, input: &str) -> Result<(), String> {
        if input.is_empty() {
            Err("must not be empty".to_string())
        } else {
            Ok(())
        }
    }
}

impl Validator for MaxLen {
    fn validate(&self, input: &str) -> Result<(), String> {
        if input.len() > self.0 {
            Err(format!("max length is {}", self.0))
        } else {
            Ok(())
        }
    }
}

// `Field<V>` is monomorphized: one specialized type per V used.
struct Field<V: Validator> {
    name: String,
    validator: V,
}

impl<V: Validator> Field<V> {
    fn new(name: &str, validator: V) -> Self {
        Field { name: name.to_string(), validator }
    }
    fn check(&self, input: &str) -> Result<(), String> {
        self.validator
            .validate(input)
            .map_err(|e| format!("{}: {}", self.name, e))
    }
}

fn main() {
    let username = Field::new("username", NonEmpty);
    let bio = Field::new("bio", MaxLen(10));

    println!("{:?}", username.check(""));
    println!("{:?}", username.check("alice"));
    println!("{:?}", bio.check("this bio is way too long"));
    println!("{:?}", bio.check("short"));
}
```

**Real output:**

```text
Err("username: must not be empty")
Ok(())
Err("bio: max length is 10")
Ok(())
```

### Version 3: trait objects (runtime strategy, the OO shape)

When the strategy is selected at runtime — from a config file, a CLI flag, a database column — and you want to store different strategies in the same field or collection, use a **trait object**: `Box<dyn Strategy>`. This is the encoding that maps one-to-one onto the TypeScript class hierarchy.

```rust playground
// Rust - the strategy is a `Box<dyn Trait>` chosen at runtime (dynamic dispatch).
trait CompressionStrategy {
    fn compress(&self, data: &str) -> String;
    fn name(&self) -> &'static str;
}

struct NoCompression;
struct RunLength;

impl CompressionStrategy for NoCompression {
    fn compress(&self, data: &str) -> String {
        data.to_string()
    }
    fn name(&self) -> &'static str {
        "none"
    }
}

impl CompressionStrategy for RunLength {
    fn compress(&self, data: &str) -> String {
        let mut out = String::new();
        let mut chars = data.chars().peekable();
        while let Some(c) = chars.next() {
            let mut count = 1;
            while chars.peek() == Some(&c) {
                chars.next();
                count += 1;
            }
            out.push_str(&format!("{c}{count}"));
        }
        out
    }
    fn name(&self) -> &'static str {
        "rle"
    }
}

// The context stores the strategy behind `dyn`, so its concrete type can vary.
struct Archiver {
    strategy: Box<dyn CompressionStrategy>,
}

impl Archiver {
    fn new(strategy: Box<dyn CompressionStrategy>) -> Self {
        Archiver { strategy }
    }
    fn store(&self, data: &str) -> String {
        format!("[{}] {}", self.strategy.name(), self.strategy.compress(data))
    }
}

// A factory maps a runtime string to a concrete strategy (see factory-pattern.md).
fn make_strategy(name: &str) -> Box<dyn CompressionStrategy> {
    match name {
        "rle" => Box::new(RunLength),
        _ => Box::new(NoCompression),
    }
}

fn main() {
    let chosen = "rle"; // imagine this comes from config at runtime
    let archiver = Archiver::new(make_strategy(chosen));
    println!("{}", archiver.store("aaabbbbc"));
    println!("{}", Archiver::new(make_strategy("none")).store("aaabbbbc"));
}
```

**Real output:**

```text
[rle] a3b4c1
[none] aaabbbbc
```

---

## Detailed Explanation

### Closures are the lightest strategy

In `fn checkout(subtotal: f64, discount: impl Fn(f64) -> f64)`, the parameter type `impl Fn(f64) -> f64` reads as "any value I can call with one `f64` that gives back an `f64`." That covers closures, function pointers, and `fn` items. There is no interface to declare and no class to write; the function signature *is* the strategy contract.

`impl Fn` in argument position is sugar for a generic: it is the same as `fn checkout<F: Fn(f64) -> f64>(subtotal: f64, discount: F)`. So a closure argument is still **static dispatch** and gets inlined. The three closure traits express how the strategy uses its captured environment:

- `Fn`: callable through a shared reference; can be called many times, captures by reference or copy. This is the common case for a stateless strategy.
- `FnMut`: needs `&mut self` to run because it mutates captured state (e.g. a counter, an accumulator).
- `FnOnce`: consumes captured values, so it can be called only once (e.g. a strategy that moves a `String` out of itself).

```rust playground
trait Renderer {
    fn render(&self) -> String;
}

struct Json;
impl Renderer for Json {
    fn render(&self) -> String {
        "{}".to_string()
    }
}

// A borrowed trait object: no heap allocation, no ownership transfer.
fn print_with(r: &dyn Renderer) {
    println!("{}", r.render());
}

fn main() {
    let json = Json;
    print_with(&json); // borrow as &dyn Renderer; no Box required

    // An FnMut strategy: it mutates captured state on each call.
    let mut count = 0;
    let mut tick = || {
        count += 1;
        count
    };
    println!("{} {} {}", tick(), tick(), tick());
}
```

**Real output:**

```text
{}
1 2 3
```

The `percent_off`/`flat_off` "closure that returns a closure" trick is how you parameterize a strategy by data: the equivalent of `new PercentOff(0.1)` in the TypeScript version, but the result is a function value rather than an object.

### Generics fix the strategy at compile time

`Field<V: Validator>` is generic over the strategy. When you write `Field::new("username", NonEmpty)`, the compiler creates a distinct `Field<NonEmpty>` type with the `validate` call hard-wired and inlinable. This is **monomorphization**, the same machinery behind `impl Fn`. There is no vtable, no indirection, and the optimizer can see through the call.

The trade-off: a `Field<NonEmpty>` and a `Field<MaxLen>` are *different types*. You cannot put both in one `Vec<Field<_>>` without erasing the difference, and the generic parameter spreads to everything that names the type. Generics are the right call when each context uses one strategy chosen at build time, and you want maximum speed.

> **Tip:** This is exactly the opposite of TypeScript generics, which are **erased** at runtime. A TypeScript `Field<V>` is one runtime shape with `V` thrown away; a Rust `Field<V>` is many runtime shapes, one per `V`. See [Section 09: Generic Functions](/09-generics-traits/00-generic-functions/).

### Trait objects defer the choice to runtime

`Box<dyn CompressionStrategy>` stores *some* type implementing the trait, behind a pointer plus a vtable. The `Archiver` field has one concrete type (`Box<dyn CompressionStrategy>`) no matter which strategy is inside, so a single `Archiver` can hold `NoCompression` today and `RunLength` after a config reload. Calls like `self.strategy.compress(...)` look up the function in the vtable at runtime: **dynamic dispatch**, the same model JavaScript always uses.

You reach for `dyn` when the strategy is genuinely runtime-selected, when you need a heterogeneous collection (`Vec<Box<dyn CompressionStrategy>>`), or when the generic type parameter would otherwise leak across a large API surface. The cost is a pointer indirection per call and a missed inlining opportunity, usually negligible, occasionally measurable in hot loops.

### You already use this pattern in std

The standard library is full of closure-as-strategy APIs. `Vec::sort_by` takes the comparison *strategy* as a closure; swapping the closure swaps the algorithm without touching the sort.

```rust playground
fn main() {
    let mut words = vec!["pear", "fig", "banana", "kiwi"];

    // Strategy 1: order by length.
    words.sort_by(|a, b| a.len().cmp(&b.len()));
    println!("{words:?}");

    // Strategy 2: reverse-alphabetical — just pass a different closure.
    words.sort_by(|a, b| b.cmp(a));
    println!("{words:?}");

    // sort_by_key takes a "key extraction" strategy.
    words.sort_by_key(|w| w.len());
    println!("{words:?}");
}
```

**Real output:**

```text
["fig", "pear", "kiwi", "banana"]
["pear", "kiwi", "fig", "banana"]
["fig", "pear", "kiwi", "banana"]
```

---

## Key Differences

| Aspect | TypeScript (interface + classes) | Rust closures | Rust generics | Rust trait objects |
| --- | --- | --- | --- | --- |
| Strategy is… | a class instance | a function value | a concrete type param | `Box<dyn Trait>` / `&dyn Trait` |
| Dispatch | dynamic (always) | static (inlined) | static (monomorphized) | dynamic (vtable) |
| Chosen at | runtime | compile or runtime | compile time | runtime |
| Heterogeneous collection | trivial (`Strategy[]`) | needs `Box<dyn Fn>` | not directly | trivial (`Vec<Box<dyn _>>`) |
| Extra methods / state | yes (class) | awkward (one fn) | yes (trait) | yes (trait) |
| Runtime cost | property lookup | none | none | pointer + vtable lookup |
| Boilerplate | high (a class each) | lowest | medium | medium |

The decision tree most Rust developers use:

1. **Is the strategy just one function?** Use a closure (`impl Fn` / `Box<dyn Fn>`). This is the default; do not write a trait you do not need.
2. **Does it have several methods or named state, and is it fixed per call site?** Use a trait + generics (`<S: Strategy>`) for zero-cost static dispatch.
3. **Is it selected at runtime, stored in a field, or mixed in a collection?** Use a trait object (`Box<dyn Strategy>`).

> **Note:** Rust having three encodings is not redundancy. It is the cost model made explicit. TypeScript hides every strategy behind one dynamic-dispatch mechanism. Rust makes you say whether the indirection is worth it.

The biggest mindset shift from TypeScript: do not start by writing an interface. In TypeScript an interface is the entry fee for any abstraction. In Rust the entry fee is a function type, and you only graduate to a trait when one function genuinely is not enough.

---

## Common Pitfalls

### Pitfall 1: putting two different closures in one `Vec` without boxing

Every closure has its own unique, unnameable type. Even two closures with identical signatures are different types. So this does not compile:

```rust
fn main() {
    let pct = 0.10_f64;
    let amount = 10.0_f64;
    let percent = move |s: f64| s * (1.0 - pct);    // captures pct
    let flat = move |s: f64| (s - amount).max(0.0); // captures amount
    let strategies = vec![percent, flat]; // does not compile (error[E0308])
    for s in &strategies {
        println!("{}", s(100.0));
    }
}
```

The real compiler error is explicit about why:

```text
error[E0308]: mismatched types
 --> src/main.rs:6:36
  |
4 |     let percent = move |s: f64| s * (1.0 - pct);   // captures pct
  |                   ------------- the expected closure
5 |     let flat = move |s: f64| (s - amount).max(0.0); // captures amount
  |                ------------- the found closure
6 |     let strategies = vec![percent, flat]; // two DISTINCT closure types
  |                                    ^^^^ expected closure, found a different closure
  |
  = note: expected closure `{closure@src/main.rs:4:19: 4:32}`
             found closure `{closure@src/main.rs:5:16: 5:29}`
  = note: no two closures, even if identical, have the same type
  = help: consider boxing your closure and/or using it as a trait object
```

The fix is exactly what the compiler suggests: box them into a uniform trait-object type:

```rust playground
fn main() {
    let pct = 0.10_f64;
    let amount = 10.0_f64;
    let strategies: Vec<Box<dyn Fn(f64) -> f64>> = vec![
        Box::new(move |s: f64| s * (1.0 - pct)),
        Box::new(move |s: f64| (s - amount).max(0.0)),
    ];
    for s in &strategies {
        println!("{:.2}", s(100.0));
    }
}
```

**Real output:**

```text
90.00
90.00
```

> **Note:** If neither closure captures anything, they can each coerce to the same `fn(f64) -> f64` function-pointer type and the `Vec` compiles without boxing. The conflict appears the moment they capture different environments.

### Pitfall 2: a strategy trait that cannot become a trait object

If you plan to use `Box<dyn Strategy>`, the trait must be **dyn-compatible** (historically "object-safe"). A generic method breaks that, because the compiler cannot put an infinite family of methods in one vtable:

```rust
trait Transformer {
    // A generic method makes the trait NOT dyn-compatible.
    fn transform<T: std::fmt::Display>(&self, value: T) -> String;
}

fn use_it(t: &dyn Transformer) { // does not compile (error[E0038])
    println!("{}", t.transform(42));
}
```

The real error names the exact reason:

```text
error[E0038]: the trait `Transformer` is not dyn compatible
 --> src/main.rs:5:15
  |
5 | fn use_it(t: &dyn Transformer) {
  |               ^^^^^^^^^^^^^^^ `Transformer` is not dyn compatible
  |
note: for a trait to be dyn compatible it needs to allow building a vtable
      for more information, visit <https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>
 --> src/main.rs:2:8
  |
1 | trait Transformer {
  |       ----------- this trait is not dyn compatible...
2 |     fn transform<T: std::fmt::Display>(&self, value: T) -> String;
  |        ^^^^^^^^^ ...because method `transform` has generic type parameters
  = help: consider moving `transform` to another trait
```

Fixes: drop the generic (take `&dyn Display` instead of `T`), or keep the generic and use the strategy through generics rather than `dyn`. The detailed rules are in [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/).

### Pitfall 3: reaching for a trait when a closure would do

The most common over-engineering mistake for a TypeScript developer is writing this in Rust:

```rust
trait DiscountStrategy {
    fn apply(&self, subtotal: f64) -> f64;
}
struct PercentOff(f64);
impl DiscountStrategy for PercentOff {
    fn apply(&self, subtotal: f64) -> f64 {
        subtotal * (1.0 - self.0)
    }
}
// ...and a struct + impl for every other rule.
```

If `apply` is the only method and the strategy carries no behavior beyond it, all of that collapses to `impl Fn(f64) -> f64`. Writing a one-method trait that exists only to be a strategy is usually a sign you wanted a closure. Reserve the trait for when there are multiple methods (`apply` *and* `name` *and* `is_combinable`) or when you need a named type to implement other traits.

### Pitfall 4: assuming `dyn` is free like in TypeScript

In TypeScript every method call is already a dynamic lookup, so `dyn`-style dispatch feels like the natural baseline. In Rust the baseline is static dispatch, and `Box<dyn Strategy>` adds a heap allocation plus a per-call vtable indirection that prevents inlining. It is the right tool for runtime selection, but do not default to it for a strategy that is fixed at compile time. That throws away performance Rust gives you for free.

---

## Best Practices

- **Default to closures.** If the strategy is a single function, use `impl Fn` for arguments and `Box<dyn Fn>` only when you must store or collect heterogeneous functions.
- **Promote to a trait when behavior is richer than one call.** Multiple methods, associated constants, or a strategy that must also be `Debug`/`Clone` all justify a trait.
- **Use generics for compile-time choice, `dyn` for runtime choice.** Phrase it as a question: "Does the caller know the concrete strategy at the call site?" Yes → generic. No → trait object.
- **Accept `&dyn Trait` over `Box<dyn Trait>` in function arguments** when you only need to borrow the strategy; it avoids forcing the caller to allocate.
- **Keep strategy traits dyn-compatible if you might box them**: avoid generic methods and `Self`-returning methods unless you commit to generics only.
- **Pair the strategy with a factory** for runtime selection: a `fn make_strategy(name: &str) -> Box<dyn Strategy>` keeps the `match` over config strings in one place (see [The Factory Pattern](/22-common-patterns/08-factory-pattern/)).
- **Let a blanket impl bridge closures and traits.** `impl<F: Fn(&str) -> String> Transform for F {}` lets callers pass either a struct strategy or a bare closure to the same generic API: the best of both worlds.

---

## Real-World Example

A request router whose **load-balancing policy** is chosen at startup from configuration. The policy genuinely varies at runtime and has more than one method, so this is the case where a trait object is the right call, and it shows the strategy, context, and factory working together.

```rust playground
use std::collections::HashMap;

/// The strategy: how to pick a backend for a request.
trait LoadBalancer {
    fn pick(&self, backends: usize, request_no: u64) -> usize;
    fn name(&self) -> &'static str;
}

struct RoundRobin;
struct Sticky {
    /// Pin every request to one backend (e.g. for a canary rollout).
    index: usize,
}

impl LoadBalancer for RoundRobin {
    fn pick(&self, backends: usize, request_no: u64) -> usize {
        (request_no as usize) % backends
    }
    fn name(&self) -> &'static str {
        "round-robin"
    }
}

impl LoadBalancer for Sticky {
    fn pick(&self, _backends: usize, _request_no: u64) -> usize {
        self.index
    }
    fn name(&self) -> &'static str {
        "sticky"
    }
}

/// The context holds a boxed strategy chosen at startup.
struct Router {
    backends: Vec<String>,
    strategy: Box<dyn LoadBalancer>,
}

impl Router {
    fn new(backends: Vec<String>, strategy: Box<dyn LoadBalancer>) -> Self {
        Router { backends, strategy }
    }

    fn route(&self, request_no: u64) -> &str {
        let i = self.strategy.pick(self.backends.len(), request_no);
        &self.backends[i]
    }
}

/// A factory mapping a config string to a concrete strategy.
fn strategy_from_config(cfg: &str) -> Box<dyn LoadBalancer> {
    match cfg {
        "sticky" => Box::new(Sticky { index: 0 }),
        _ => Box::new(RoundRobin),
    }
}

fn main() {
    let backends = vec![
        "10.0.0.1".to_string(),
        "10.0.0.2".to_string(),
        "10.0.0.3".to_string(),
    ];

    let config: HashMap<&str, &str> = HashMap::from([("lb", "round-robin")]);
    let router = Router::new(backends.clone(), strategy_from_config(config["lb"]));

    println!("policy: {}", router.strategy.name());
    for req in 0..5 {
        println!("request {req} -> {}", router.route(req));
    }

    // Swapping policies is just swapping the boxed strategy — the classic pattern.
    let canary = Router::new(backends, strategy_from_config("sticky"));
    println!("\npolicy: {}", canary.strategy.name());
    for req in 0..3 {
        println!("request {req} -> {}", canary.route(req));
    }
}
```

**Real output:**

```text
policy: round-robin
request 0 -> 10.0.0.1
request 1 -> 10.0.0.2
request 2 -> 10.0.0.3
request 3 -> 10.0.0.1
request 4 -> 10.0.0.2

policy: sticky
request 0 -> 10.0.0.1
request 1 -> 10.0.0.1
request 2 -> 10.0.0.1
```

This compiles cleanly under `cargo clippy` with no warnings. If the policy were instead fixed per deployment and you cared about inlining `pick` in a hot path, you would make `Router` generic — `Router<L: LoadBalancer>` — and drop the `Box`.

---

## Further Reading

### Official documentation

- [The Rust Book — Closures](https://doc.rust-lang.org/book/ch13-01-closures.html) — `Fn`, `FnMut`, `FnOnce` and capturing the environment
- [The Rust Book — Trait Objects for Values of Different Types](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) — the OO-style strategy encoding
- [The Rust Reference — `dyn` compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility) — when a strategy trait can be boxed
- [Rust by Example — Closures as input parameters](https://doc.rust-lang.org/rust-by-example/fn/closures/input_parameters.html)
- [`std::vec::Vec::sort_by`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.sort_by): strategy-as-closure in the standard library

### Related topics in this guide

- [Section 22 overview](/22-common-patterns/): the full map of common patterns
- [The Factory Pattern](/22-common-patterns/08-factory-pattern/): building the concrete strategy values (`make_strategy`)
- [Dependency Injection in Rust](/22-common-patterns/09-dependency-injection/) — injecting strategies via generics vs trait objects, and testing
- [The Command Pattern in Rust](/22-common-patterns/07-command-pattern/) — a close cousin: enums of commands or `Box<dyn Fn>` with undo/redo
- [The Decorator Pattern in Rust](/22-common-patterns/06-decorator-pattern/) — wrapping a strategy to add behavior; how tower `Layer`/`Service` generalizes it
- [The Visitor Pattern](/22-common-patterns/04-visitor-pattern/) — when a closed set of variants beats an open set of strategies
- [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/) — the mechanics of `dyn` and dynamic dispatch
- [Section 09: Trait Bounds](/09-generics-traits/05-trait-bounds/) — `<S: Strategy>` static dispatch in depth
- [Section 09: Generic Functions](/09-generics-traits/00-generic-functions/) — monomorphization vs TypeScript's type erasure
- [Section 23: Ecosystem](/23-ecosystem/) — crates like `tower` that build whole architectures on swappable strategies
- Foundations: [Getting Started](/01-getting-started/) and [Basics](/02-basics/)

---

## Exercises

### Exercise 1: a discount engine with closures

**Difficulty:** Easy

**Objective:** Implement the strategy pattern with closures: no trait, no struct.

**Instructions:** Write a function `total_price(items: &[f64], discount: impl Fn(f64) -> f64) -> f64` that sums the items, applies the `discount` strategy to the subtotal, and rounds to two decimal places. In `main`, define three closure strategies — no discount, ten percent off, and five dollars off (never below zero) — and print the total for the cart `[19.99, 5.00, 49.99]` under each.

```rust
fn total_price(items: &[f64], discount: impl Fn(f64) -> f64) -> f64 {
    // TODO: sum the items, apply discount, round to 2 dp
    /* ??? */
}

fn main() {
    let cart = [19.99, 5.00, 49.99];
    // TODO: three closure strategies and a println! for each
}
```

<details>
<summary>Solution</summary>

```rust playground
fn total_price(items: &[f64], discount: impl Fn(f64) -> f64) -> f64 {
    let subtotal: f64 = items.iter().sum();
    let discounted = discount(subtotal);
    (discounted * 100.0).round() / 100.0
}

fn main() {
    let cart = [19.99, 5.00, 49.99];

    let no_discount = |s: f64| s;
    let ten_percent = |s: f64| s * 0.90;
    let five_off = |s: f64| (s - 5.0).max(0.0);

    println!("{:.2}", total_price(&cart, no_discount));
    println!("{:.2}", total_price(&cart, ten_percent));
    println!("{:.2}", total_price(&cart, five_off));
}
```

**Real output:**

```text
74.98
67.48
69.98
```

</details>

### Exercise 2: a runtime-selected hash strategy

**Difficulty:** Medium

**Objective:** Use trait objects to select a strategy at runtime from a registry.

**Instructions:** Define a trait `HashStrategy` with `fn hash(&self, input: &str) -> u64`. Implement it for `SumBytes` (sum of byte values) and `Fnv1a` (the FNV-1a algorithm). Build a `Hasher` whose field is a `HashMap<String, Box<dyn HashStrategy>>` populated with `"sum"` and `"fnv"`. Add `fn hash_with(&self, algo: &str, input: &str) -> Option<u64>` that looks up the strategy by name and applies it, returning `None` for an unknown algorithm.

<details>
<summary>Solution</summary>

```rust playground
use std::collections::HashMap;

trait HashStrategy {
    fn hash(&self, input: &str) -> u64;
}

struct SumBytes;
struct Fnv1a;

impl HashStrategy for SumBytes {
    fn hash(&self, input: &str) -> u64 {
        input.bytes().map(u64::from).sum()
    }
}

impl HashStrategy for Fnv1a {
    fn hash(&self, input: &str) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in input.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

struct Hasher {
    registry: HashMap<String, Box<dyn HashStrategy>>,
}

impl Hasher {
    fn new() -> Self {
        let mut registry: HashMap<String, Box<dyn HashStrategy>> = HashMap::new();
        registry.insert("sum".to_string(), Box::new(SumBytes));
        registry.insert("fnv".to_string(), Box::new(Fnv1a));
        Hasher { registry }
    }

    fn hash_with(&self, algo: &str, input: &str) -> Option<u64> {
        self.registry.get(algo).map(|s| s.hash(input))
    }
}

fn main() {
    let hasher = Hasher::new();
    println!("{:?}", hasher.hash_with("sum", "abc"));
    println!("{:?}", hasher.hash_with("fnv", "abc"));
    println!("{:?}", hasher.hash_with("missing", "abc"));
}
```

**Real output:**

```text
Some(294)
Some(16654208175385433931)
None
```

</details>

### Exercise 3: a generic pipeline that also accepts closures

**Difficulty:** Hard

**Objective:** Combine the generic and closure encodings with a blanket impl so one API accepts both struct strategies and bare closures.

**Instructions:** Define a trait `Transform` with `fn apply(&self, input: &str) -> String`. Implement it for unit structs `Upper` (uppercase the input) and `Reverse` (reverse the characters). Then add a blanket impl `impl<F: Fn(&str) -> String> Transform for F` so any matching closure is also a `Transform`. Make a generic `Pipeline<T: Transform>` with a `run(&self, input: &str) -> String` method, and drive it with `Upper`, `Reverse`, and a closure `|s: &str| format!("{s}!")`.

<details>
<summary>Solution</summary>

```rust playground
trait Transform {
    fn apply(&self, input: &str) -> String;
}

struct Upper;
struct Reverse;

impl Transform for Upper {
    fn apply(&self, input: &str) -> String {
        input.to_uppercase()
    }
}

impl Transform for Reverse {
    fn apply(&self, input: &str) -> String {
        input.chars().rev().collect()
    }
}

// Any matching closure is ALSO a Transform, so callers can pass either.
impl<F: Fn(&str) -> String> Transform for F {
    fn apply(&self, input: &str) -> String {
        self(input)
    }
}

struct Pipeline<T: Transform> {
    transform: T,
}

impl<T: Transform> Pipeline<T> {
    fn run(&self, input: &str) -> String {
        self.transform.apply(input)
    }
}

fn main() {
    let upper = Pipeline { transform: Upper };
    let reverse = Pipeline { transform: Reverse };
    let exclaim = Pipeline { transform: |s: &str| format!("{s}!") };

    println!("{}", upper.run("hello"));
    println!("{}", reverse.run("hello"));
    println!("{}", exclaim.run("hello"));
}
```

**Real output:**

```text
HELLO
olleh
hello!
```

> **Note:** A blanket impl over `F: Fn(...)` is convenient but can conflict if you later add another blanket impl that could also match the same types. Keep one such bridge per trait.

</details>
