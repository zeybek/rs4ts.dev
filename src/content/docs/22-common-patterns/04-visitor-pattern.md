---
title: "The Visitor Pattern"
description: "In Rust most visitors vanish: model the structure as an enum and match on it for exhaustive, compiler-checked dispatch, no accept/visit double dispatch."
---

In object-oriented code the **Visitor pattern** lets you add a new operation to a fixed set of object types without editing those types, at the cost of a fair amount of double-dispatch ceremony. Rust gives you the same power with far less plumbing, because its `enum` plus `match` already *is* a closed set of shapes with exhaustive, compiler-checked dispatch. This page shows the idiomatic Rust form first, then the OO trait form, and explains exactly when each one earns its keep.

---

## Quick Overview

The Visitor pattern separates an **algorithm** (what you want to do) from the **object structure** it runs over (the data). A TypeScript/JavaScript developer usually meets it as a class hierarchy with `accept(visitor)` methods plus a `Visitor` interface full of `visitCircle`, `visitRect`, … callbacks.

In Rust, the everyday answer is much simpler: model the structure as an `enum`, and write each operation as a free function that `match`es on it. The compiler enforces exhaustiveness for you, so "add a new operation" is just "write another function," and "add a new variant" is "let the compiler list every match you must update." You only reach for the heavier OO-style trait form when you genuinely need an **open** set of node types that third parties can extend.

**In short:** The classic OO Visitor exists to bolt exhaustive dispatch onto languages that lack it. Rust has exhaustive dispatch built in (`match` over an `enum`), so most "visitors" collapse into a plain recursive function.

---

## TypeScript/JavaScript Example

Here is the textbook OO Visitor: a small shape hierarchy where each shape can `accept` a visitor, and an `AreaVisitor` computes a running total. This is the "double dispatch" version that the Gang of Four book describes.

```typescript
// The element interface: every shape can accept a visitor.
interface Shape {
  accept(visitor: ShapeVisitor): void;
}

// The visitor interface: one method per concrete shape type.
interface ShapeVisitor {
  visitCircle(circle: Circle): void;
  visitRect(rect: Rect): void;
}

class Circle implements Shape {
  constructor(public radius: number) {}
  accept(visitor: ShapeVisitor): void {
    visitor.visitCircle(this); // dispatch #2: pick visitCircle
  }
}

class Rect implements Shape {
  constructor(public width: number, public height: number) {}
  accept(visitor: ShapeVisitor): void {
    visitor.visitRect(this);
  }
}

class AreaVisitor implements ShapeVisitor {
  total = 0;
  visitCircle(circle: Circle): void {
    this.total += Math.PI * circle.radius * circle.radius;
  }
  visitRect(rect: Rect): void {
    this.total += rect.width * rect.height;
  }
}

const shapes: Shape[] = [new Circle(1), new Rect(2, 3)];
const area = new AreaVisitor();
for (const shape of shapes) {
  shape.accept(area); // dispatch #1: pick the right `accept`
}
console.log("total area =", area.total.toFixed(4)); // total area = 9.1416
```

Run with Node v22 (`node demo.mjs`), this prints:

```text
total area = 9.1416
```

**Key points:**

- Two interfaces (`Shape`, `ShapeVisitor`) and the `accept`/`visit*` round-trip are the "double dispatch" that picks the right method based on *both* the shape's runtime type and the visitor's.
- Adding a new **operation** (say, a `PerimeterVisitor`) is cheap: write one new class, touch nothing else.
- Adding a new **shape** (say, `Triangle`) is expensive: you must add `visitTriangle` to the interface and to *every* existing visitor, and TypeScript will only flag the visitor classes, not the data, as needing updates.

Many TypeScript codebases skip all that and use a **discriminated union** with a `switch` instead, which, as we'll see, is the form that maps directly onto idiomatic Rust:

```typescript
type Expr =
  | { kind: "number"; value: number }
  | { kind: "add"; left: Expr; right: Expr }
  | { kind: "mul"; left: Expr; right: Expr };

function render(node: Expr): string {
  switch (node.kind) {
    case "number": return String(node.value);
    case "add": return `(${render(node.left)} + ${render(node.right)})`;
    case "mul": return `(${render(node.left)} * ${render(node.right)})`;
    // Exhaustiveness is OPT-IN: only a `never` default catches a missed case.
  }
}
```

---

## Rust Equivalent

The idiomatic Rust form drops both interfaces. The structure is an `enum`; each operation is a function that `match`es on it. Here is an arithmetic expression tree with two operations — evaluate and render — written as plain recursive functions.

```rust
#[derive(Debug, Clone)]
enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
}

// Operation #1: evaluate the tree to a number.
fn eval(expr: &Expr) -> f64 {
    match expr {
        Expr::Number(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Sub(a, b) => eval(a) - eval(b),
        Expr::Mul(a, b) => eval(a) * eval(b),
        Expr::Neg(e) => -eval(e),
    }
}

// Operation #2: render the tree as a parenthesized string.
fn render(expr: &Expr) -> String {
    match expr {
        Expr::Number(n) => n.to_string(),
        Expr::Add(a, b) => format!("({} + {})", render(a), render(b)),
        Expr::Sub(a, b) => format!("({} - {})", render(a), render(b)),
        Expr::Mul(a, b) => format!("({} * {})", render(a), render(b)),
        Expr::Neg(e) => format!("-{}", render(e)),
    }
}

fn main() {
    // -(2 + 3) * 4
    let expr = Expr::Mul(
        Box::new(Expr::Neg(Box::new(Expr::Add(
            Box::new(Expr::Number(2.0)),
            Box::new(Expr::Number(3.0)),
        )))),
        Box::new(Expr::Number(4.0)),
    );

    println!("{}", render(&expr));
    println!("= {}", eval(&expr));
}
```

Running it prints:

```text
(-(2 + 3) * 4)
= -20
```

**Key points:**

- `enum Expr { ... }` declares the entire closed set of node shapes in one place: the equivalent of the whole `Shape` hierarchy.
- Each operation (`eval`, `render`) is a single function. There is no `accept`, no `Visitor` interface, no double dispatch; `match` resolves the variant directly.
- The recursive variants hold `Box<Expr>` because an `enum` must have a known, finite size; the indirection is covered in [Section 10 — Smart Pointers](/10-smart-pointers/).
- Importantly, the compiler *requires* every arm. Add an `Expr::Pow` variant and `eval`/`render` will refuse to compile until you handle it, the safety the OO version can only approximate with a `never` trick.

---

## Detailed Explanation

### Why `enum` + `match` is the real visitor

The Visitor pattern solves one core problem: **dispatch on the concrete type of a node, exhaustively.** In Java, C#, and pre-`never` TypeScript, the language gives you only single dispatch (one `virtual`/method call per object), so the GoF "double dispatch" dance (`node.accept(visitor)` then `visitor.visitX(node)`) is a workaround to recover the second dispatch.

Rust does not need the workaround. A `match` over an `enum` *is* a multi-way dispatch on the variant tag, and the compiler verifies you covered every case:

```rust
fn eval(expr: &Expr) -> f64 {
    match expr {
        Expr::Number(n) => *n,          // dispatch happens HERE, in one place
        Expr::Add(a, b) => eval(a) + eval(b),
        // ...
    }
}
```

The variant name *is* the discriminant. There is no separate `kind: "add"` string to keep in sync (contrast the TypeScript discriminated union, where you invent and maintain that field by hand). See [Section 06 — Enums](/06-data-structures/02-enums/) for the full story on data-carrying variants.

### Multiple operations sharing accumulator state

Many real visitors carry state — a running total, a counter, a list of collected nodes. In OO code that state lives on the visitor object. In Rust you pass a `&mut` accumulator into the function, which is the same idea without the object:

```rust
#[derive(Default, Debug)]
struct Stats {
    numbers: usize,
    operators: usize,
}

fn collect_stats(expr: &Expr, stats: &mut Stats) {
    match expr {
        Expr::Number(_) => stats.numbers += 1,
        Expr::Neg(e) => {
            stats.operators += 1;
            collect_stats(e, stats);
        }
        Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) => {
            stats.operators += 1;
            collect_stats(a, stats);
            collect_stats(b, stats);
        }
    }
}
```

Note the **or-pattern** `Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b)`: when several variants share a shape and you want identical handling, one arm covers them all while still binding `a` and `b`. The `&mut Stats` borrow means the accumulator is threaded through the whole traversal without cloning, enforced by the borrow checker.

### The OO trait form, when you actually need it

The `enum` form has one limitation: the set of variants is **closed**. Only the crate that defines `Expr` can add a variant. If you are writing a library and want *downstream* crates to plug in their own node types — the "open structure" scenario — you need the trait-object form, which trades exhaustiveness for extensibility.

```rust
trait Shape {
    fn accept(&self, visitor: &mut dyn ShapeVisitor);
}

trait ShapeVisitor {
    fn visit_circle(&mut self, circle: &Circle);
    fn visit_rect(&mut self, rect: &Rect);
}

struct Circle {
    radius: f64,
}
struct Rect {
    width: f64,
    height: f64,
}

impl Shape for Circle {
    fn accept(&self, visitor: &mut dyn ShapeVisitor) {
        visitor.visit_circle(self); // dispatch #2: picks visit_circle
    }
}
impl Shape for Rect {
    fn accept(&self, visitor: &mut dyn ShapeVisitor) {
        visitor.visit_rect(self);
    }
}

struct AreaVisitor {
    total: f64,
}
impl ShapeVisitor for AreaVisitor {
    fn visit_circle(&mut self, circle: &Circle) {
        self.total += std::f64::consts::PI * circle.radius * circle.radius;
    }
    fn visit_rect(&mut self, rect: &Rect) {
        self.total += rect.width * rect.height;
    }
}

// A SECOND visitor, added later, WITHOUT touching Circle / Rect / Shape.
struct NameVisitor {
    names: Vec<&'static str>,
}
impl ShapeVisitor for NameVisitor {
    fn visit_circle(&mut self, _: &Circle) {
        self.names.push("circle");
    }
    fn visit_rect(&mut self, _: &Rect) {
        self.names.push("rect");
    }
}

fn main() {
    let shapes: Vec<Box<dyn Shape>> = vec![
        Box::new(Circle { radius: 1.0 }),
        Box::new(Rect { width: 2.0, height: 3.0 }),
    ];

    let mut area = AreaVisitor { total: 0.0 };
    let mut names = NameVisitor { names: Vec::new() };
    for shape in &shapes {
        shape.accept(&mut area);
        shape.accept(&mut names);
    }
    println!("total area = {:.4}", area.total);
    println!("names = {:?}", names.names);
}
```

Running it prints:

```text
total area = 9.1416
names = ["circle", "rect"]
```

This is the literal translation of the TypeScript version: `&dyn ShapeVisitor` is the trait object (covered in [Section 09 — Trait Objects](/09-generics-traits/06-trait-objects/)), `accept` performs the first dispatch, and the `visit_*` call performs the second. `Box<dyn Shape>` lets a heterogeneous `Vec` hold both shapes. Adding `AreaVisitor` and `NameVisitor` requires no change to `Circle`, `Rect`, or `Shape`: the *operations* are open. But adding a new shape (`Triangle`) means adding `visit_triangle` to the trait and to every visitor, exactly as in TypeScript.

### The expression problem, made concrete

The two forms sit on opposite sides of the classic **expression problem**: the tension between adding new *types* and adding new *operations* without recompiling existing code:

- **`enum` + `match`:** adding an **operation** is trivial (a new function); adding a **variant** forces edits to every `match`, but the compiler hands you the exact list. Easy to add operations, "compiler-guided" to add variants.
- **trait objects:** adding a **type** is trivial (a new `impl`); adding an **operation** forces a new method on the visitor trait and every impl. Easy to add types, painful to add operations.

For a tree you own — an AST, a config document, a JSON value — the `enum` form wins almost every time. Reach for the trait form only when the *set of node types* must stay open to other crates.

---

## Key Differences

| Aspect | TypeScript OO Visitor | Rust `enum` + `match` | Rust trait-object Visitor |
| --- | --- | --- | --- |
| Structure definition | Class hierarchy + `Shape` interface | One `enum` | One trait + many `impl`s |
| Dispatch mechanism | Double dispatch (`accept` → `visit*`) | Single `match` on the variant tag | Double dispatch (`accept` → `visit*`) |
| Exhaustiveness | Opt-in (`never` trick) | **Compiler-enforced** (`E0004`) | Not checked (open set) |
| Add a new operation | New visitor class (cheap) | New function (cheap) | New trait method + every impl (costly) |
| Add a new node type | New `visit*` on every visitor (costly) | New variant; compiler lists every `match` to fix | New struct + `impl` (cheap) |
| Node set | Open (subclasses) | **Closed** (one crate owns it) | Open (any crate can `impl`) |
| Runtime cost | Virtual calls + allocation | Tag check; no vtable, no heap | Vtable indirection + `Box` allocation |
| Boilerplate | High (two interfaces, round-trip) | Minimal (just the function) | Moderate (`accept` + `visit*` plumbing) |

> **Note:** "Double dispatch" is not a Rust idiom you should reach for by default. It is a technique to recover exhaustive multi-type dispatch in languages that only have single dispatch. Rust's `match` already gives you that, so the `accept`/`visit*` round-trip is pure overhead unless you specifically need the open node set.

The deepest difference is *who is forced to react to change*. In the OO world, a forgotten case is a silent runtime fall-through. In the `enum` world, a forgotten case is a compile error that names the missing variant, the safety property that makes refactoring large match-heavy codebases tractable.

---

## Common Pitfalls

### Pitfall 1: a wildcard arm silently swallows new variants

The single biggest mistake when porting a `switch` to Rust is reaching for `_ =>` to "handle the rest." It compiles, but it also defeats the exhaustiveness check: when someone later adds a variant, your `match` keeps compiling and quietly mishandles it.

```rust
#[derive(Debug)]
enum Event {
    Click { x: i64, y: i64 },
    KeyPress(char),
    Scroll(i64),
    Paste(String), // added later
}

fn describe(event: &Event) -> String {
    match event {
        Event::Click { x, y } => format!("click at ({x}, {y})"),
        Event::KeyPress(c) => format!("key '{c}'"),
        _ => "other".to_string(), // silently swallows Scroll AND Paste
    }
}
```

This compiles and runs — `Scroll` and `Paste` both print `"other"`, almost certainly a bug. Clippy can catch it; running `cargo clippy -- -W clippy::wildcard_enum_match_arm` reports the real warning:

```text
warning: wildcard match will also match any future added variants
  --> src/main.rs:17:9
   |
17 |         _ => "other".to_string(), // silently swallows Scroll AND Paste
   |         ^ help: try: `Event::Scroll(_) | Event::Paste(_)`
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#wildcard_enum_match_arm
```

**Fix:** spell out the remaining variants (or an or-pattern) instead of `_`, so the next person who adds a variant gets a compile error pointing right at this `match`.

### Pitfall 2: trusting that an added variant "just works"

Conversely, if you *do* write every arm explicitly, the compiler protects you. Add `Expr::Mul` and forget to handle it in `eval`, and the build fails with a precise message:

```rust
enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>), // newly added variant
}

fn eval(expr: &Expr) -> f64 {
    match expr {
        Expr::Number(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
        // forgot Expr::Mul
    }
}
```

The real error from `cargo build`:

```text
error[E0004]: non-exhaustive patterns: `&Expr::Mul(_, _)` not covered
 --> src/main.rs:8:11
  |
8 |     match expr {
  |           ^^^^ pattern `&Expr::Mul(_, _)` not covered
  |
note: `Expr` defined here
 --> src/main.rs:1:6
  |
1 | enum Expr {
  |      ^^^^
...
4 |     Mul(Box<Expr>, Box<Expr>), // newly added variant
  |     --- not covered
  = note: the matched value is of type `&Expr`
```

This `E0004` is the feature, not the obstacle: it is the compiler doing the bookkeeping the OO Visitor pattern was invented to force on you. Embrace it — and resist the urge to silence it with `_`.

### Pitfall 3: over-engineering with a `Visitor` trait when a function would do

Coming from an OO background, it is tempting to build a full `trait ExprVisitor { fn visit_number(...); fn visit_add(...); ... }` for a tree you fully own. That recreates all the boilerplate the `enum` form eliminated, *and* you give up nothing in return because the node set is closed anyway. Start with a free function over the `enum`; only introduce a visitor trait if you find yourself with many operations that share a fixed traversal order and want to factor the walk out (see Exercise 2).

### Pitfall 4: forgetting the box on recursive variants

A self-referential `enum` has no finite size, so a naive `Add(Expr, Expr)` is rejected. The fix is indirection — `Box<Expr>`. The error is `E0072` ("recursive type has infinite size"); the compiler even suggests inserting `Box`. This is a property of value-typed enums, unlike TypeScript objects which are always behind a reference.

---

## Best Practices

- **Default to `enum` + `match`.** For any tree or node set you own (ASTs, documents, protocol messages, state machines), the recursive-function form is the idiomatic, zero-overhead visitor.
- **Write every arm; avoid `_` for enums you control.** Spell out variants so adding one becomes a guided compile error rather than a silent fall-through. Turn on `clippy::wildcard_enum_match_arm` in code where this matters.
- **Thread mutable state through a `&mut` accumulator.** It is the direct analog of visitor instance fields, without an object.
- **Use or-patterns** (`A(x) | B(x) | C(x)`) to share one arm across structurally similar variants.
- **Reach for the trait-object form only for an open node set** — when third-party crates must add node types. Accept that you then lose exhaustiveness and that new operations become expensive.
- **Consider a default-method visitor trait** when you have many operations that all need the *same* recursive traversal: put the walk in a provided method and let each visitor override only the per-node hook (Exercise 2).
- **Return new trees for transformations.** A "transforming visitor" (constant folding, desugaring, optimization) is just a function `fn(&Expr) -> Expr` that pattern-matches and rebuilds — no mutation, no visitor object (Exercise 3).

---

## Real-World Example

A production-flavored use: walking a JSON document with two independent passes — one that gathers statistics, one that collects every string leaf in document order — plus a compact serializer. This is exactly the shape of work a linter, a config validator, or a redaction tool does, and it shows multiple "visitors" over one owned `enum`.

```rust
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

// Operation: serialize to compact JSON (reads every payload).
fn to_compact(value: &Json) -> String {
    match value {
        Json::Null => "null".to_string(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => n.to_string(),
        Json::Str(s) => format!("\"{s}\""),
        Json::Array(items) => {
            let inner: Vec<String> = items.iter().map(to_compact).collect();
            format!("[{}]", inner.join(","))
        }
        Json::Object(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{k}\":{}", to_compact(v)))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

// Operation: count nodes by kind, accumulating into shared state.
#[derive(Default, Debug)]
struct Stats {
    nulls: usize,
    bools: usize,
    numbers: usize,
    strings: usize,
    arrays: usize,
    objects: usize,
}

fn collect_stats(value: &Json, stats: &mut Stats) {
    match value {
        Json::Null => stats.nulls += 1,
        Json::Bool(_) => stats.bools += 1,
        Json::Number(_) => stats.numbers += 1,
        Json::Str(_) => stats.strings += 1,
        Json::Array(items) => {
            stats.arrays += 1;
            for item in items {
                collect_stats(item, stats);
            }
        }
        Json::Object(map) => {
            stats.objects += 1;
            for child in map.values() {
                collect_stats(child, stats);
            }
        }
    }
}

// Operation: gather every string leaf, in document order, borrowing in place.
fn collect_strings<'a>(value: &'a Json, out: &mut Vec<&'a str>) {
    match value {
        Json::Str(s) => out.push(s),
        Json::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
        Json::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
        _ => {} // leaf kinds with no strings: deliberately ignored
    }
}

fn main() {
    let doc = Json::Object(BTreeMap::from([
        ("name".into(), Json::Str("Ada".into())),
        ("active".into(), Json::Bool(true)),
        (
            "tags".into(),
            Json::Array(vec![Json::Str("a".into()), Json::Str("b".into())]),
        ),
        ("score".into(), Json::Number(9.5)),
        ("note".into(), Json::Null),
    ]));

    println!("{}", to_compact(&doc));

    let mut stats = Stats::default();
    collect_stats(&doc, &mut stats);
    println!("{stats:?}");

    let mut strings = Vec::new();
    collect_strings(&doc, &mut strings);
    println!("strings = {strings:?}");
}
```

Running it prints (the `BTreeMap` keeps keys sorted, so output is deterministic):

```text
{"active":true,"name":"Ada","note":null,"score":9.5,"tags":["a","b"]}
Stats { nulls: 1, bools: 1, numbers: 1, strings: 3, arrays: 1, objects: 1 }
strings = ["Ada", "a", "b"]
```

Three operations, one data type, zero `accept`/`visit*` ceremony. Note `collect_strings` borrows the strings (`&'a str`) instead of cloning them. The lifetime `'a` ties the borrowed slices to the document, so the visitor allocates nothing for the strings it gathers. (The deliberate `_ => {}` here is safe because the ignored kinds genuinely have no strings; if `Json` ever gained a string-bearing variant, you'd want to switch to explicit arms per Pitfall 1.)

> **Tip:** This is precisely how the `serde_json::Value` enum and many real parsers are consumed: an owned `enum`, traversed by ordinary functions. Production crates layer the same idea. `syn`'s `Visit`/`VisitMut` traits, for instance, are a default-method visitor over Rust's own AST `enum`s, used by procedural macros (see [Section 23 — The Ecosystem](/23-ecosystem/)).

---

## Further Reading

- [The Rust Programming Language — `match` Control Flow](https://doc.rust-lang.org/book/ch06-02-match.html): exhaustive matching, the engine behind the idiomatic visitor.
- [Rust by Example — Enums and `match`](https://doc.rust-lang.org/rust-by-example/custom_types/enum.html): data-carrying variants and pattern binding.
- [Clippy lint: `wildcard_enum_match_arm`](https://rust-lang.github.io/rust-clippy/master/index.html#wildcard_enum_match_arm): the lint from Pitfall 1.
- [`syn`'s `Visit` trait](https://docs.rs/syn/latest/syn/visit/index.html): a real default-method visitor over an AST `enum`.
- Guide cross-links:
  - [Section 06 — Enums](/06-data-structures/02-enums/) and [Pattern Matching](/06-data-structures/04-pattern-matching/): the foundation of the idiomatic form.
  - [Section 09 — Trait Objects](/09-generics-traits/06-trait-objects/): `dyn Trait` and double dispatch for the OO form.
  - [Section 08 — Error Handling](/08-error-handling/): matching on `Result`/error enums is the same pattern applied to errors.
  - Sibling patterns: [Strategy](/22-common-patterns/05-strategy-pattern/) (selecting one behavior), [Command](/22-common-patterns/07-command-pattern/) (enums of actions with undo/redo), [Factory](/22-common-patterns/08-factory-pattern/) (constructing variants), and the [section overview](/22-common-patterns/).

---

## Exercises

### Exercise 1: Extend the expression tree

**Difficulty:** Beginner

**Objective:** Experience the compiler-guided workflow for adding a variant.

**Instructions:** Starting from the `Expr` enum with `Number`, `Add`, and `Mul`, add a `Pow(Box<Expr>, Box<Expr>)` variant (exponentiation). Extend both `eval` (using `f64::powf`) and `render` (rendering `a ^ b`). Build after adding the variant *before* updating the functions, and observe which `E0004` errors the compiler reports.

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone)]
enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
}

fn eval(expr: &Expr) -> f64 {
    match expr {
        Expr::Number(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Mul(a, b) => eval(a) * eval(b),
        Expr::Pow(base, exp) => eval(base).powf(eval(exp)),
    }
}

fn render(expr: &Expr) -> String {
    match expr {
        Expr::Number(n) => n.to_string(),
        Expr::Add(a, b) => format!("({} + {})", render(a), render(b)),
        Expr::Mul(a, b) => format!("({} * {})", render(a), render(b)),
        Expr::Pow(base, exp) => format!("({} ^ {})", render(base), render(exp)),
    }
}

fn main() {
    // (2 + 3) ^ 2
    let expr = Expr::Pow(
        Box::new(Expr::Add(
            Box::new(Expr::Number(2.0)),
            Box::new(Expr::Number(3.0)),
        )),
        Box::new(Expr::Number(2.0)),
    );
    println!("{} = {}", render(&expr), eval(&expr)); // ((2 + 3) ^ 2) = 25
}
```

Running it prints:

```text
((2 + 3) ^ 2) = 25
```

</details>

### Exercise 2: A default-method visitor trait

**Difficulty:** Intermediate

**Objective:** Factor a shared traversal into a trait so multiple operations reuse it, while still matching on an owned `enum`.

**Instructions:** Define a `trait ExprVisitor` with a provided `visit(&mut self, expr: &Expr)` method that recursively walks children and then calls a required `on_node(&mut self, expr: &Expr)` hook. Implement a `NodeCounter` that counts how many nodes the tree has by overriding only `on_node`. (Use the `Expr` from Exercise 1.)

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone)]
enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
}

trait ExprVisitor {
    // Provided method: the shared post-order walk.
    fn visit(&mut self, expr: &Expr) {
        match expr {
            Expr::Number(_) => {}
            Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
                self.visit(a);
                self.visit(b);
            }
        }
        self.on_node(expr);
    }
    // Required hook: what each concrete visitor does per node.
    fn on_node(&mut self, expr: &Expr);
}

struct NodeCounter {
    count: usize,
}
impl ExprVisitor for NodeCounter {
    fn on_node(&mut self, _expr: &Expr) {
        self.count += 1;
    }
}

fn main() {
    // (2 + 3) ^ 2  -> nodes: Number(2), Number(3), Add, Number(2), Pow = 5
    let expr = Expr::Pow(
        Box::new(Expr::Add(
            Box::new(Expr::Number(2.0)),
            Box::new(Expr::Number(3.0)),
        )),
        Box::new(Expr::Number(2.0)),
    );

    let mut counter = NodeCounter { count: 0 };
    counter.visit(&expr);
    println!("nodes = {}", counter.count); // nodes = 5
}
```

Running it prints:

```text
nodes = 5
```

> The provided `visit` method centralizes the walk; each visitor overrides only `on_node`. This is the pattern `syn`'s `Visit` trait uses: worth it only when several operations share one traversal.

</details>

### Exercise 3: A transforming visitor (constant folding)

**Difficulty:** Advanced

**Objective:** Write a visitor that produces a *new* tree, the way a compiler optimization pass does.

**Instructions:** Write `fn fold_consts(expr: &Expr) -> Expr` that recursively simplifies any subtree of literal numbers into a single `Number`. For example, `(2 + 3) * x` should fold the `2 + 3` into `5`, leaving `5 * x` (model `x` as a `Number` for the demo). Operations on non-literal subtrees must be rebuilt unchanged.

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone)]
enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
}

fn render(expr: &Expr) -> String {
    match expr {
        Expr::Number(n) => n.to_string(),
        Expr::Add(a, b) => format!("({} + {})", render(a), render(b)),
        Expr::Mul(a, b) => format!("({} * {})", render(a), render(b)),
        Expr::Pow(a, b) => format!("({} ^ {})", render(a), render(b)),
    }
}

fn fold_consts(expr: &Expr) -> Expr {
    match expr {
        Expr::Number(n) => Expr::Number(*n),
        Expr::Add(a, b) => match (fold_consts(a), fold_consts(b)) {
            (Expr::Number(x), Expr::Number(y)) => Expr::Number(x + y),
            (l, r) => Expr::Add(Box::new(l), Box::new(r)),
        },
        Expr::Mul(a, b) => match (fold_consts(a), fold_consts(b)) {
            (Expr::Number(x), Expr::Number(y)) => Expr::Number(x * y),
            (l, r) => Expr::Mul(Box::new(l), Box::new(r)),
        },
        Expr::Pow(a, b) => match (fold_consts(a), fold_consts(b)) {
            (Expr::Number(x), Expr::Number(y)) => Expr::Number(x.powf(y)),
            (l, r) => Expr::Pow(Box::new(l), Box::new(r)),
        },
    }
}

fn main() {
    // (2 + 3) * 10  (10 stands in for a variable that can't be folded away)
    let expr = Expr::Mul(
        Box::new(Expr::Add(
            Box::new(Expr::Number(2.0)),
            Box::new(Expr::Number(3.0)),
        )),
        Box::new(Expr::Number(10.0)),
    );

    println!("before = {}", render(&expr));        // ((2 + 3) * 10)
    let folded = fold_consts(&expr);
    println!("after  = {}", render(&folded));       // (5 * 10) -> further folds to 50
}
```

Running it prints:

```text
before = ((2 + 3) * 10)
after  = 50
```

Because `10` is itself a literal here, the whole tree folds to `Number(50)`. If `10` were a non-literal node (a variable), only the `2 + 3` would fold, leaving `(5 * x)` — the rebuild-unchanged branch handles that case.

</details>
