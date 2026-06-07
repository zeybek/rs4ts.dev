---
title: "The Factory Pattern"
description: "Rust has no constructors or new keyword: build values with associated functions, enum factories, or Box<dyn Trait>, not TypeScript static factory methods."
---

In TypeScript the **factory pattern** usually means a `static` method or a free function that decides which class to instantiate and hands you back an object: `User.createAdmin(...)`, `LoggerFactory.create(level)`, a `switch` that `new`s up the right subclass. Rust reaches the same goals with three smaller, sharper tools: **associated functions** (the idiomatic `Self::new`), **enums** (a closed set of products parsed in one place), and **trait-object factories** (`Box<dyn Trait>` chosen at runtime). This file is about picking among them, and about why Rust developers rarely write a class called `Factory` at all.

---

## Quick Overview

A **factory** centralizes object construction so callers ask for *what* they want, not *how* it is built. It matters to a TypeScript/JavaScript developer for a specific reason: Rust has **no constructors, no `new` keyword, no constructor overloading, and no optional/named parameters**. Where TypeScript gives a class one `constructor` plus a few `static` factory methods, Rust gives a type a set of plain **associated functions**. There is nothing privileged about `new`; it is a convention, not a language feature.

That shifts the whole pattern. The three encodings you will use:

- **Associated functions** (`User::new`, `User::admin`, `Config::from_env`): the everyday factory. Named alternative constructors replace overloaded constructors and static factory methods.
- **Enums + a parsing/dispatch function** — when the products form a *closed, known set*, the factory is a `match` that turns input into a variant.
- **Trait-object factories** (`fn make(...) -> Box<dyn Trait>`) — when the product set is *open* or chosen at runtime from config, and the caller should not know the concrete type.

> **Note:** This page is about *building* values. For configuring a complex value field-by-field see [The Builder Pattern](/22-common-patterns/00-builder-pattern/); for the strategies a factory often produces see [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/); for wiring constructed dependencies into a system see [Dependency Injection in Rust](/22-common-patterns/09-dependency-injection/).

---

## TypeScript/JavaScript Example

A realistic notification system. The textbook OO approach: a `Notifier` interface, a concrete class per channel, and a `NotifierFactory` whose `static create` switches on a runtime string to pick the class. Alongside it, a `User` class shows the *other* common factory shape: `static` named constructors.

```typescript
// TypeScript - the classic OO factory: an interface + a static factory method
interface Notifier {
  send(message: string): string;
  channel(): string;
}

class EmailNotifier implements Notifier {
  constructor(private address: string) {}
  send(message: string): string {
    return `email to ${this.address}: ${message}`;
  }
  channel(): string {
    return "email";
  }
}

class SmsNotifier implements Notifier {
  constructor(private number: string) {}
  send(message: string): string {
    return `sms to ${this.number}: ${message}`;
  }
  channel(): string {
    return "sms";
  }
}

class NotifierFactory {
  // Decides which class to instantiate from a runtime string.
  static create(kind: string, dest: string): Notifier | undefined {
    switch (kind) {
      case "email":
        return new EmailNotifier(dest);
      case "sms":
        return new SmsNotifier(dest);
      default:
        return undefined;
    }
  }
}

// The other factory shape: static named constructors on the class itself.
class User {
  private constructor(
    public id: number,
    public name: string,
    public role: "admin" | "member" | "guest",
  ) {}

  static create(id: number, name: string): User {
    return new User(id, name, "member");
  }
  static admin(id: number, name: string): User {
    return new User(id, name, "admin");
  }
  static guest(id: number): User {
    return new User(id, "guest", "guest");
  }
}

const n = NotifierFactory.create("email", "a@b.com");
console.log(n?.send("deploy finished"));
console.log(User.admin(1, "Bob"));
```

Running this under Node v22 prints:

```text
email to a@b.com: deploy finished
User { id: 1, name: 'Bob', role: 'admin' }
```

Two shapes to carry forward. `NotifierFactory.create` returns *some* `Notifier` and the caller never names the concrete class; that is the **trait-object** factory in Rust. `User.admin` / `User.guest` are named alternative constructors that exist only because TypeScript's single `constructor` cannot be overloaded ergonomically, and those become **associated functions** in Rust.

---

## Rust Equivalent

### Version 1: associated functions (the everyday factory)

`User` has no constructor. Instead it has **associated functions**: functions namespaced under the type, called with `Type::function()`. `new` is just the conventional name for the primary one; `admin`, `guest`, and `from_row` are named alternatives that replace TypeScript's overloads and `static` factory methods.

```rust
#[derive(Debug)]
struct User {
    id: u64,
    name: String,
    role: Role,
}

#[derive(Debug, PartialEq)]
enum Role {
    Admin,
    Member,
    Guest,
}

impl User {
    // The conventional primary constructor: `new`.
    fn new(id: u64, name: &str) -> Self {
        Self { id, name: name.to_string(), role: Role::Member }
    }

    // Named alternative constructors — Rust's answer to overloaded constructors.
    fn admin(id: u64, name: &str) -> Self {
        Self { id, name: name.to_string(), role: Role::Admin }
    }

    fn guest(id: u64) -> Self {
        Self { id, name: "guest".to_string(), role: Role::Guest }
    }

    // A fallible factory returns `Result` instead of panicking.
    fn from_row(row: &str) -> Result<Self, String> {
        let (id_str, name) = row.split_once(',').ok_or("expected `id,name`")?;
        let id = id_str.trim().parse::<u64>().map_err(|e| e.to_string())?;
        Ok(Self::new(id, name.trim()))
    }
}

fn main() {
    let member = User::new(1, "Alice");
    let admin = User::admin(2, "Bob");
    let guest = User::guest(3);

    println!("{member:?}");
    println!("{admin:?}");
    println!("{guest:?}");

    println!("{:?}", User::from_row("4, Carol"));
    println!("{:?}", User::from_row("nope"));
}
```

**Real output:**

```text
User { id: 1, name: "Alice", role: Member }
User { id: 2, name: "Bob", role: Admin }
User { id: 3, name: "guest", role: Guest }
Ok(User { id: 4, name: "Carol", role: Member })
Err("expected `id,name`")
```

### Version 2: an enum factory (a closed set of products)

When the products are a *known, fixed set*, model them as an **enum** and make the factory a single `match` that turns input into a variant. There is one concrete type (`Shape`), so the products live happily in a `Vec`, and the compiler forces every consumer to handle every variant.

```rust
#[derive(Debug)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Square { side: f64 },
}

impl Shape {
    // The factory: a config string -> a concrete variant.
    fn from_spec(spec: &str) -> Result<Shape, String> {
        let mut parts = spec.split_whitespace();
        let kind = parts.next().ok_or("empty spec")?;
        let nums: Vec<f64> = parts
            .map(|p| p.parse::<f64>().map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;

        match (kind, nums.as_slice()) {
            ("circle", [r]) => Ok(Shape::Circle { radius: *r }),
            ("rect", [w, h]) => Ok(Shape::Rectangle { width: *w, height: *h }),
            ("square", [s]) => Ok(Shape::Square { side: *s }),
            _ => Err(format!("bad spec: {spec:?}")),
        }
    }

    fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Square { side } => side * side,
        }
    }
}

fn main() {
    for spec in ["circle 2", "rect 3 4", "square 5", "blob 1 2 3"] {
        match Shape::from_spec(spec) {
            Ok(shape) => println!("{spec:>10} -> area {:.2}", shape.area()),
            Err(e) => println!("{spec:>10} -> error: {e}"),
        }
    }
}
```

**Real output:**

```text
  circle 2 -> area 12.57
  rect 3 4 -> area 12.00
  square 5 -> area 25.00
blob 1 2 3 -> error: bad spec: "blob 1 2 3"
```

### Version 3: a trait-object factory (open set, runtime choice)

When callers should not know the concrete type, and the set of products is open or selected at runtime, the factory returns `Box<dyn Trait>`. This is the direct analogue of `NotifierFactory.create` returning `Notifier`.

```rust
trait Notifier {
    fn send(&self, message: &str) -> String;
    fn channel(&self) -> &'static str;
}

struct Email {
    address: String,
}
struct Sms {
    number: String,
}
struct Slack {
    webhook: String,
}

impl Notifier for Email {
    fn send(&self, message: &str) -> String {
        format!("email to {}: {message}", self.address)
    }
    fn channel(&self) -> &'static str {
        "email"
    }
}

impl Notifier for Sms {
    fn send(&self, message: &str) -> String {
        format!("sms to {}: {message}", self.number)
    }
    fn channel(&self) -> &'static str {
        "sms"
    }
}

impl Notifier for Slack {
    fn send(&self, message: &str) -> String {
        format!("slack via {}: {message}", self.webhook)
    }
    fn channel(&self) -> &'static str {
        "slack"
    }
}

// The factory: a runtime kind + destination -> some boxed Notifier.
fn make_notifier(kind: &str, dest: &str) -> Option<Box<dyn Notifier>> {
    match kind {
        "email" => Some(Box::new(Email { address: dest.to_string() })),
        "sms" => Some(Box::new(Sms { number: dest.to_string() })),
        "slack" => Some(Box::new(Slack { webhook: dest.to_string() })),
        _ => None,
    }
}

fn main() {
    // The same Vec holds three different concrete types behind `dyn`.
    let config = [("email", "a@b.com"), ("sms", "+15551234"), ("slack", "https://hook")];
    let notifiers: Vec<Box<dyn Notifier>> = config
        .iter()
        .filter_map(|(kind, dest)| make_notifier(kind, dest))
        .collect();

    for n in &notifiers {
        println!("[{}] {}", n.channel(), n.send("deploy finished"));
    }

    println!("unknown -> none? {}", make_notifier("carrier-pigeon", "x").is_none());
}
```

**Real output:**

```text
[email] email to a@b.com: deploy finished
[sms] sms to +15551234: deploy finished
[slack] slack via https://hook: deploy finished
unknown -> none? true
```

---

## Detailed Explanation

### Associated functions replace constructors

There is no `new` keyword in Rust. `User::new` is an **associated function**: a function defined in an `impl` block that does *not* take `self`, called through the type with the `::` path operator. The compiler treats `new` like any other name: you could call it `create`, `build`, or `default`. Several conventions cluster here:

- `new` — the primary constructor when there is an obvious "default" way to build the value.
- `with_*` / `from_*` — alternative constructors emphasizing what they take (`Vec::with_capacity`, `String::from`, `User::from_row`).
- `default()` from the `Default` trait — the zero-argument factory the rest of the ecosystem looks for.

`Self` inside an `impl User` block is an alias for `User`, so `-> Self` and `Self { .. }` keep the type name out of the body, handy when you rename the type later. This is the same `Self` covered in [Section 06: Associated Functions](/06-data-structures/06-associated-functions/) and [Section 06: impl Blocks](/06-data-structures/05-impl-blocks/).

The key contrast with TypeScript: because Rust has no overloading, `User::new`, `User::admin`, and `User::guest` are *distinct names*, not three signatures of one constructor. That is a feature: each construction path is self-documenting at the call site. And `from_row` shows the idiomatic move for construction that can fail: return `Result<Self, E>` rather than throwing. The caller cannot ignore the failure path. (`?` and `Result` are covered in [Section 08: The Question Mark Operator](/08-error-handling/01-question-mark/).)

### Enums make the product set closed and exhaustive

`Shape::from_spec` is a factory in the GoF sense (input goes in, the right product comes out) but the product is an enum *variant*, not a subclass. This is the most idiomatic Rust factory when you control the full set of products, because the enum gives you two things a class hierarchy cannot:

- **Exhaustiveness.** A `match self` over `Shape` is a compile error if you add a `Triangle` variant and forget to handle it somewhere. A TypeScript `switch` over subclasses silently falls through.
- **One concrete type.** Every `Shape` is the same size and type, so `Vec<Shape>` just works: no boxing, no `dyn`, no heap allocation per element.

The factory's `match (kind, nums.as_slice())` parses *and* validates in one expression: the tuple pattern `("rect", [w, h])` only matches when the keyword is `"rect"` **and** exactly two numbers were supplied, so a malformed spec falls through to the `Err` arm. (Slice and tuple patterns come from [Section 04: match](/04-control-flow/02-match/) and [Section 06: Pattern Matching](/06-data-structures/04-pattern-matching/).)

### Trait objects defer the concrete type to runtime

`make_notifier` returns `Box<dyn Notifier>`: a heap-allocated value plus a vtable, where the concrete type (`Email`, `Sms`, `Slack`) is erased at the boundary. This is the encoding that maps one-to-one onto `NotifierFactory.create` returning an interface: the caller programs against `Notifier`, the factory owns the `match` over runtime strings, and the three products coexist in one `Vec<Box<dyn Notifier>>`.

Use it when the product set is **open** (a plugin could add a fourth channel), when selection is genuinely **runtime** (a config key, a CLI flag, a DB column), or when you need a **heterogeneous collection**. The cost versus the enum factory: a heap allocation per product and a vtable indirection per call. The mechanics of `dyn` live in [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/).

### Enum factory vs trait-object factory — the real decision

Both turn a runtime string into a product. Choose by asking *who owns the set of products*:

- **You own a fixed set** → enum. Exhaustive `match`, no allocation, products in a `Vec<Shape>`. Adding a variant is a deliberate, compiler-checked change everywhere.
- **The set is open or extended elsewhere** → `Box<dyn Trait>`. New implementors can be added without touching the trait, at the cost of allocation and dynamic dispatch.

This is the same closed-vs-open axis as the [The Visitor Pattern](/22-common-patterns/04-visitor-pattern/) (enum + match) versus the [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/) (trait objects) discussions. The factory is just where the product gets *built*.

### The product can be a function, too

A factory does not have to produce a struct. When the product is "a behavior," the factory returns a closure. `make_parser` below is a factory whose product is a `Box<dyn Fn>` configured by the captured `radix`:

```rust
// A factory whose product is a closure (a function value).
type Parser = Box<dyn Fn(&str) -> Result<i64, String>>;

fn make_parser(radix: u32) -> Parser {
    Box::new(move |s: &str| {
        i64::from_str_radix(s.trim(), radix).map_err(|e| format!("radix {radix}: {e}"))
    })
}

fn main() {
    let parsers: Vec<(&str, Parser)> = vec![
        ("decimal", make_parser(10)),
        ("hex", make_parser(16)),
        ("binary", make_parser(2)),
    ];

    for (name, parse) in &parsers {
        println!("{name:>8}: {:?}", parse("FF"));
    }
}
```

**Real output:**

```text
 decimal: Err("radix 10: invalid digit found in string")
     hex: Ok(255)
  binary: Err("radix 2: invalid digit found in string")
```

This blurs the line between factory and [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/) on purpose: a factory that produces strategies *is* the bridge between the two patterns.

---

## Key Differences

| Aspect | TypeScript factory | Rust associated fn | Rust enum factory | Rust trait-object factory |
| --- | --- | --- | --- | --- |
| Construction keyword | `new` / `static` method | `Type::fn()` (no `new`) | `Type::fn()` returns variant | `fn make() -> Box<dyn T>` |
| Multiple constructors | overload or `static` methods | distinct named fns | one fn, many arms | one fn, many arms |
| Product type | a class / interface | the struct itself | one enum, many variants | erased behind `dyn` |
| Product set | open (any subclass) | n/a (single type) | **closed** (exhaustive) | **open** |
| Fits in one `Vec` | yes (all subtype) | yes | yes (no boxing) | yes (boxed) |
| Failure | throws | returns `Result` | returns `Result` | returns `Option`/`Result` |
| Runtime cost | object alloc + dynamic dispatch | none | none (no alloc) | heap alloc + vtable |
| Compiler checks all cases | no | n/a | **yes** | no |

The decision tree most Rust developers use:

1. **Just building one type, maybe several ways?** Associated functions: `new`, `with_*`, `from_*`. Do not write a `Factory` type.
2. **A fixed set of product kinds you own?** An enum plus a `from_*` / `parse` function. Exhaustive and allocation-free.
3. **An open set, or runtime selection, where callers must not know the concrete type?** A function returning `Box<dyn Trait>`.

> **Note:** A TypeScript codebase often grows a dedicated `XxxFactory` class. In Rust that is usually a smell: a free function or an associated function already *is* the factory. Reserve a named factory **value** (a struct you pass around) for the abstract-factory case, where the factory itself is a swappable dependency. See the Real-World Example below.

The deepest mindset shift: in TypeScript, construction and the `Factory` class are separate things because the language forces objects through `new`. In Rust, a function that returns a value is already a factory. The pattern mostly dissolves into ordinary functions.

---

## Common Pitfalls

### Pitfall 1: a factory method that returns `Self` breaks `Box<dyn Trait>`

A natural OO instinct is to put a "clone me / make another" method *on the trait*. But a method that returns `Self` makes the trait **not dyn-compatible** (historically "object-safe"), so you cannot box it:

```rust
trait Widget {
    fn render(&self) -> String;
    // A "factory method" that returns Self breaks dyn-compatibility.
    fn duplicate(&self) -> Self;
}

struct Button;
impl Widget for Button {
    fn render(&self) -> String {
        "<button>".to_string()
    }
    fn duplicate(&self) -> Self {
        Button
    }
}

fn store(w: Box<dyn Widget>) { // does not compile (error[E0038])
    println!("{}", w.render());
}

fn main() {
    store(Box::new(Button));
}
```

The real compiler error names the exact reason (it also repeats at the `Box::new` call site):

```text
error[E0038]: the trait `Widget` is not dyn compatible
  --> src/main.rs:18:17
   |
18 | fn store(w: Box<dyn Widget>) {
   |                 ^^^^^^^^^^ `Widget` is not dyn compatible
   |
note: for a trait to be dyn compatible it needs to allow building a vtable
      for more information, visit <https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>
  --> src/main.rs:5:28
   |
 2 | trait Widget {
   |       ------ this trait is not dyn compatible...
...
 5 |     fn duplicate(&self) -> Self;
   |                            ^^^^ ...because method `duplicate` references the `Self` type in its return type
   = help: consider moving `duplicate` to another trait
   = help: only type `Button` implements `Widget`; consider using it directly instead.
```

Fixes: keep the construction in a *free* factory function (`fn make_widget(...) -> Box<dyn Widget>`) rather than on the trait, or, if you really need "make another from a trait object," return `Box<dyn Widget>` instead of `Self`. The dyn-compatibility rules are detailed in [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/).

### Pitfall 2: `-> impl Trait` cannot return different concrete types per branch

`impl Trait` in return position means "one specific hidden type," not "any type implementing the trait." So a factory that wants to return `Email` from one arm and `Sms` from another will not compile:

```rust
trait Notifier {
    fn send(&self, message: &str) -> String;
}

struct Email;
struct Sms;

impl Notifier for Email {
    fn send(&self, message: &str) -> String {
        format!("email: {message}")
    }
}
impl Notifier for Sms {
    fn send(&self, message: &str) -> String {
        format!("sms: {message}")
    }
}

// Looks like a clean factory, but each branch is a DIFFERENT concrete type.
fn make_notifier(kind: &str) -> impl Notifier { // does not compile (error[E0308])
    match kind {
        "sms" => Sms,
        _ => Email,
    }
}

fn main() {
    let n = make_notifier("sms");
    println!("{}", n.send("hi"));
}
```

The real error even tells you the fix:

```text
error[E0308]: `match` arms have incompatible types
  --> src/main.rs:24:14
   |
22 | /     match kind {
23 | |         "sms" => Sms,
   | |                  --- this is found to be of type `Sms`
24 | |         _ => Email,
   | |              ^^^^^ expected `Sms`, found `Email`
25 | |     }
   | |_____- `match` arms have incompatible types
   |
help: you could change the return type to be a boxed trait object
   |
21 - fn make_notifier(kind: &str) -> impl Notifier {
21 + fn make_notifier(kind: &str) -> Box<dyn Notifier> {
   |
help: if you change the return type to expect trait objects, box the returned expressions
   |
23 ~         "sms" => Box::new(Sms),
24 ~         _ => Box::new(Email),
   |
```

The rule of thumb: **`impl Trait` is for one concrete type known at compile time; `Box<dyn Trait>` is for many, chosen at runtime.** A runtime-dispatching factory almost always wants `Box<dyn Trait>`. See [Section 09: impl Trait](/09-generics-traits/07-impl-trait/).

### Pitfall 3: writing a `Factory` struct when a function would do

Porting a `class NotifierFactory { static create() {} }` literally produces a struct with one associated function and no fields, pure ceremony:

```rust
struct NotifierFactory;
impl NotifierFactory {
    fn create(kind: &str) -> Option<Box<dyn std::fmt::Debug>> {
        // ...
        None
    }
}
```

Unless the factory itself holds state or is injected as a swappable dependency, drop the struct and write a free function `fn make_notifier(...) -> Box<dyn Notifier>`. Modules already give you the namespacing that the TypeScript class provided (see [Section 12: Modules](/12-modules-packages/)). Keep the named factory *value* only for the abstract-factory case.

### Pitfall 4: panicking inside a factory instead of returning `Result`

A factory that parses untrusted input must not `unwrap`/`panic!` on bad data; that turns a recoverable error into a crash. The idiomatic shape is `fn from_x(...) -> Result<Self, Error>` (as in `from_row` and `from_spec` above), so the caller decides what to do. Reserve panics for genuinely unreachable internal states. Error-handling layering is covered in [The Error-Propagation Pattern](/22-common-patterns/03-error-propagation/) and [Section 08](/08-error-handling/).

---

## Best Practices

- **Prefer associated functions over a `Factory` type.** `Type::new`, `Type::with_capacity`, `Type::from_*` are the Rust factory. A standalone factory struct earns its place only when it carries state or is injected.
- **Name the construction path.** Because there is no overloading, use distinct names (`admin`, `guest`, `from_env`, `from_row`) instead of cramming variants into one `new`. The call site reads as documentation.
- **Return `Result` (or `Option`) from fallible factories.** Make "this input cannot become a valid value" a type the caller must handle, not a panic.
- **Reach for an enum when you own the product set.** Exhaustive `match`, no allocation, one concrete type, and the compiler flags every place that must change when you add a variant.
- **Reach for `Box<dyn Trait>` when the set is open or runtime-selected.** Keep the `match` over config strings in *one* factory function so the mapping lives in a single place.
- **Implement `Default` when there is a sensible zero-config value**, and have `new()` delegate to it where appropriate. The rest of the ecosystem (and `#[derive(Default)]`) builds on that trait.
- **Keep factory traits dyn-compatible if you intend to box the *factory* itself**: avoid `-> Self` and generic methods on the factory trait (see the Real-World Example).
- **Use `From`/`TryFrom` for conversions.** A factory that builds `Self` from exactly one other type is better expressed as `impl From<T> for Self` (or `TryFrom` when fallible), which also gives you `.into()` for free.

---

## Real-World Example

A report exporter whose **output format is chosen from configuration**. It combines two factory styles cleanly: a *fallible enum factory* (`Format::parse`) that validates the config string into a closed set, and a *trait-object factory* (`Format::exporter`) that turns the chosen format into a `Box<dyn Exporter>`. Callers touch only `render_report`.

```rust
use std::collections::BTreeMap;

/// One row of the report.
struct Record {
    name: String,
    value: u64,
}

/// The product trait: anything that can serialize records.
trait Exporter {
    fn export(&self, records: &[Record]) -> String;
}

struct CsvExporter;
struct JsonExporter;
struct MarkdownExporter;

impl Exporter for CsvExporter {
    fn export(&self, records: &[Record]) -> String {
        let mut out = String::from("name,value\n");
        for r in records {
            out.push_str(&format!("{},{}\n", r.name, r.value));
        }
        out.trim_end().to_string()
    }
}

impl Exporter for JsonExporter {
    fn export(&self, records: &[Record]) -> String {
        let items: Vec<String> = records
            .iter()
            .map(|r| format!("{{\"name\":\"{}\",\"value\":{}}}", r.name, r.value))
            .collect();
        format!("[{}]", items.join(","))
    }
}

impl Exporter for MarkdownExporter {
    fn export(&self, records: &[Record]) -> String {
        let mut out = String::from("| name | value |\n| --- | --- |\n");
        for r in records {
            out.push_str(&format!("| {} | {} |\n", r.name, r.value));
        }
        out.trim_end().to_string()
    }
}

/// A closed set of supported formats — the parse target of the factory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Csv,
    Json,
    Markdown,
}

impl Format {
    /// Fallible enum factory: a config string -> a known variant.
    fn parse(name: &str) -> Result<Format, String> {
        match name.to_ascii_lowercase().as_str() {
            "csv" => Ok(Format::Csv),
            "json" => Ok(Format::Json),
            "md" | "markdown" => Ok(Format::Markdown),
            other => Err(format!("unknown format {other:?}")),
        }
    }

    /// Trait-object factory: each variant builds its exporter.
    fn exporter(self) -> Box<dyn Exporter> {
        match self {
            Format::Csv => Box::new(CsvExporter),
            Format::Json => Box::new(JsonExporter),
            Format::Markdown => Box::new(MarkdownExporter),
        }
    }
}

/// The one public entry point: parse the format, build the exporter, run it.
fn render_report(format_name: &str, records: &[Record]) -> Result<String, String> {
    let format = Format::parse(format_name)?;
    Ok(format.exporter().export(records))
}

fn main() {
    let records = vec![
        Record { name: "alpha".to_string(), value: 10 },
        Record { name: "beta".to_string(), value: 20 },
    ];

    // Imagine `config` came from a TOML file or a CLI flag.
    let config: BTreeMap<&str, &str> = BTreeMap::from([("output", "markdown")]);

    match render_report(config["output"], &records) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("error: {e}"),
    }

    println!("---");
    println!("{}", render_report("csv", &records).unwrap());
    println!("---");
    println!("{:?}", render_report("yaml", &records));
}
```

**Real output:**

```text
| name | value |
| --- | --- |
| alpha | 10 |
| beta | 20 |
---
name,value
alpha,10
beta,20
---
Err("unknown format \"yaml\"")
```

This compiles cleanly under `cargo clippy` with no warnings. Note the division of labor: `Format::parse` owns *validation* over a closed set (an unknown format is an `Err`, never a panic), while `Format::exporter` owns the *mapping to behavior* behind `dyn`. If you later add a `Yaml` variant, the compiler forces you to update *both* `match`es; the enum keeps the factory honest in a way a `switch` over classes never could.

---

## Further Reading

### Official documentation

- [The Rust Book — Method Syntax (associated functions)](https://doc.rust-lang.org/book/ch05-03-method-syntax.html#associated-functions) — `Self::new` and friends
- [The Rust Book — Defining an Enum](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html): the enum-factory product type
- [The Rust Book — Trait Objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) — returning `Box<dyn Trait>` from a factory
- [The Rust Reference — `dyn` compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility): why a `-> Self` factory method cannot be boxed
- [Rust API Guidelines — Constructors](https://rust-lang.github.io/api-guidelines/predictability.html#constructors-are-static-inherent-methods-c-ctor) — the `new`/`with_*`/`from_*` conventions
- [`std::convert::From`](https://doc.rust-lang.org/std/convert/trait.From.html) and [`TryFrom`](https://doc.rust-lang.org/std/convert/trait.TryFrom.html): the conversion-as-factory traits

### Related topics in this guide

- [Section 22 overview](/22-common-patterns/) — the full map of common patterns
- [The Builder Pattern](/22-common-patterns/00-builder-pattern/) — when one constructor is not enough and you assemble field-by-field
- [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/): the behaviors a factory often produces (closures vs generics vs trait objects)
- [The Newtype Pattern](/22-common-patterns/01-newtype/) — a factory whose whole job is wrapping one field in a meaningful type
- [The Visitor Pattern](/22-common-patterns/04-visitor-pattern/) — the same closed-set enum + `match` idiom, applied to traversal
- [Dependency Injection in Rust](/22-common-patterns/09-dependency-injection/) — injecting the factory itself as a swappable dependency
- [The Error-Propagation Pattern](/22-common-patterns/03-error-propagation/): designing the `Result` error type a fallible factory returns
- [Section 06: Associated Functions](/06-data-structures/06-associated-functions/) and [Section 06: impl Blocks](/06-data-structures/05-impl-blocks/) — the mechanics of `Type::fn()` and `Self`
- [Section 06: Enums](/06-data-structures/02-enums/): the enum-factory product
- [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/) and [Section 09: impl Trait](/09-generics-traits/07-impl-trait/) — `Box<dyn Trait>` vs `impl Trait` return types
- [Section 08: The Question Mark Operator](/08-error-handling/01-question-mark/) — propagating errors out of a fallible factory
- [Section 23: Ecosystem](/23-ecosystem/) — crates whose public API is built from `from_*` constructors and trait-object factories
- Foundations: [Getting Started](/01-getting-started/) and [Basics](/02-basics/)

---

## Exercises

### Exercise 1: named alternative constructors

**Difficulty:** Easy

**Objective:** Replace overloaded constructors with associated functions.

**Instructions:** Define a `Temperature` struct that stores a single `celsius: f64`. Give it three associated functions — `from_celsius`, `from_fahrenheit`, and `from_kelvin` — that each construct a `Temperature` from the named unit, plus a `celsius(&self) -> f64` accessor. In `main`, build one temperature each way and print its Celsius value to two decimal places. (Fahrenheit→Celsius is `(f - 32) * 5 / 9`; Kelvin→Celsius is `k - 273.15`.)

```rust
#[derive(Debug)]
struct Temperature {
    celsius: f64,
}

impl Temperature {
    fn from_celsius(c: f64) -> Self {
        /* ??? */
    }
    // TODO: from_fahrenheit, from_kelvin, celsius
}

fn main() {
    // TODO: build one each way, print to 2 dp
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
struct Temperature {
    celsius: f64,
}

impl Temperature {
    fn from_celsius(c: f64) -> Self {
        Self { celsius: c }
    }
    fn from_fahrenheit(f: f64) -> Self {
        Self { celsius: (f - 32.0) * 5.0 / 9.0 }
    }
    fn from_kelvin(k: f64) -> Self {
        Self { celsius: k - 273.15 }
    }
    fn celsius(&self) -> f64 {
        self.celsius
    }
}

fn main() {
    println!("{:.2}", Temperature::from_celsius(25.0).celsius());
    println!("{:.2}", Temperature::from_fahrenheit(98.6).celsius());
    println!("{:.2}", Temperature::from_kelvin(300.0).celsius());
}
```

**Real output:**

```text
25.00
37.00
26.85
```

</details>

### Exercise 2: a trait-object factory with a registry

**Difficulty:** Medium

**Objective:** Build a runtime factory that looks products up by name in a registry of constructor functions.

**Instructions:** Define a trait `Animal` with `fn speak(&self) -> String` and `fn species(&self) -> &'static str`. Implement it for unit structs `Dog`, `Cat`, and `Cow`. Build an `AnimalFactory` whose field is a `HashMap<&'static str, fn() -> Box<dyn Animal>>` populated in `new()`, plus a `create(&self, kind: &str) -> Option<Box<dyn Animal>>` that looks up the builder and calls it (returning `None` for an unknown kind). Drive it over `["dog", "cat", "cow", "dragon"]`.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

trait Animal {
    fn speak(&self) -> String;
    fn species(&self) -> &'static str;
}

struct Dog;
struct Cat;
struct Cow;

impl Animal for Dog {
    fn speak(&self) -> String {
        "woof".to_string()
    }
    fn species(&self) -> &'static str {
        "dog"
    }
}
impl Animal for Cat {
    fn speak(&self) -> String {
        "meow".to_string()
    }
    fn species(&self) -> &'static str {
        "cat"
    }
}
impl Animal for Cow {
    fn speak(&self) -> String {
        "moo".to_string()
    }
    fn species(&self) -> &'static str {
        "cow"
    }
}

// Registry of constructor functions keyed by name.
struct AnimalFactory {
    builders: HashMap<&'static str, fn() -> Box<dyn Animal>>,
}

impl AnimalFactory {
    fn new() -> Self {
        let mut builders: HashMap<&'static str, fn() -> Box<dyn Animal>> = HashMap::new();
        builders.insert("dog", || Box::new(Dog) as Box<dyn Animal>);
        builders.insert("cat", || Box::new(Cat) as Box<dyn Animal>);
        builders.insert("cow", || Box::new(Cow) as Box<dyn Animal>);
        Self { builders }
    }

    fn create(&self, kind: &str) -> Option<Box<dyn Animal>> {
        self.builders.get(kind).map(|build| build())
    }
}

fn main() {
    let factory = AnimalFactory::new();
    for kind in ["dog", "cat", "cow", "dragon"] {
        match factory.create(kind) {
            Some(a) => println!("{}: {}", a.species(), a.speak()),
            None => println!("{kind}: (unknown)"),
        }
    }
}
```

**Real output:**

```text
dog: woof
cat: meow
cow: moo
dragon: (unknown)
```

> **Note:** A function pointer (`fn() -> Box<dyn Animal>`) can hold a non-capturing closure, so the registry stays cheap and `Copy`. If the builders needed to capture configuration, the value type would become `Box<dyn Fn() -> Box<dyn Animal>>`.

</details>

### Exercise 3: an abstract factory with an associated type

**Difficulty:** Hard

**Objective:** Abstract over *which factory* is used, so a generic consumer never names the concrete product type.

**Instructions:** Define a trait `Factory` with an associated type `Product` and a method `fn create(&self) -> Self::Product`. Implement it for a `ConnectionFactory` that hands out `Connection { id: u64 }` values with monotonically increasing ids (use a `std::cell::Cell<u64>` for the counter so `create` can take `&self`). Then write a `Pool<F: Factory>` holding the factory plus a `Vec<F::Product>`, with `grow(&mut self, n: usize)` to create `n` products and `len(&self)`. In `main`, build a pool from a `ConnectionFactory`, grow it by 3, and print the size and contents.

<details>
<summary>Solution</summary>

```rust
trait Factory {
    type Product;
    fn create(&self) -> Self::Product;
}

#[derive(Debug)]
struct Connection {
    id: u64,
}

// A factory that hands out connections with increasing ids.
struct ConnectionFactory {
    next_id: std::cell::Cell<u64>,
}

impl ConnectionFactory {
    fn new() -> Self {
        Self { next_id: std::cell::Cell::new(1) }
    }
}

impl Factory for ConnectionFactory {
    type Product = Connection;
    fn create(&self) -> Connection {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        Connection { id }
    }
}

// A pool generic over ANY factory; it never names a concrete product type.
struct Pool<F: Factory> {
    factory: F,
    items: Vec<F::Product>,
}

impl<F: Factory> Pool<F> {
    fn new(factory: F) -> Self {
        Self { factory, items: Vec::new() }
    }
    fn grow(&mut self, n: usize) {
        for _ in 0..n {
            self.items.push(self.factory.create());
        }
    }
    fn len(&self) -> usize {
        self.items.len()
    }
}

fn main() {
    let mut pool = Pool::new(ConnectionFactory::new());
    pool.grow(3);
    println!("pool size: {}", pool.len());
    println!("{:?}", pool.items);
}
```

**Real output:**

```text
pool size: 3
[Connection { id: 1 }, Connection { id: 2 }, Connection { id: 3 }]
```

> **Note:** Because `Pool<F>` is generic, the compiler monomorphizes `create` per concrete factory: zero-cost static dispatch, no boxing. The associated type `Product` keeps the consumer fully decoupled from `Connection` while staying statically typed. This is the abstract-factory pattern done the Rust way: the factory is a swappable *dependency*, which is exactly the topic of [Dependency Injection in Rust](/22-common-patterns/09-dependency-injection/).

</details>
