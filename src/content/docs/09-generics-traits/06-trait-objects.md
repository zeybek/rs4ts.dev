---
title: "Trait Objects and Dynamic Dispatch"
description: "Trait objects bring runtime polymorphism to Rust via dyn Trait, like a TS Renderer[]. Learn Box<dyn>, vtables, dyn compatibility, and dynamic vs static."
---

In TypeScript, when you store a `Renderer[]` and call `.render()` on each item, the JavaScript engine looks up the right method at runtime; every method call is **dynamic dispatch**. Rust gives you that same capability, but it makes it *opt-in*: you ask for it explicitly with the `dyn` keyword. A value typed as `dyn Trait` is a **trait object**, and this file is about how trait objects work, when to reach for them, and what they cost.

---

## Quick Overview

A **trait object** lets you store and call values of *different concrete types* through a single shared trait, deciding which method to run at runtime: Rust's **dynamic dispatch**. You write it as `&dyn Trait` (a borrowed trait object) or `Box<dyn Trait>` (an owned, heap-allocated one). This is the escape hatch from Rust's default **static dispatch** (monomorphized generics, covered in [Generic Functions](/09-generics-traits/00-generic-functions/)): generics give you one specialized copy per type and zero runtime lookup, while `dyn` gives you one shared code path and a runtime vtable lookup. Not every trait can become a trait object: it must be **dyn-compatible** (historically called "object-safe"), and we will see exactly why.

> **Note:** This page is about *dynamic dispatch* and `dyn Trait`. Defining traits is in [Traits](/09-generics-traits/03-traits/); constraining generics for static dispatch is in [Trait Bounds](/09-generics-traits/05-trait-bounds/); the `impl Trait` shorthand (a different feature entirely) is in [`impl Trait`](/09-generics-traits/07-impl-trait/).

---

## TypeScript/JavaScript Example

A common UI scenario: you have several widget types, each knows how to render itself, and you keep them in one array and render them in a loop. Every TypeScript developer has written something like this.

```typescript
// TypeScript - a shared interface, several classes, one heterogeneous array
interface Renderer {
  render(): string;
}

class Button implements Renderer {
  constructor(public label: string) {}
  render(): string {
    return `<button>${this.label}</button>`;
  }
}

class Checkbox implements Renderer {
  constructor(public checked: boolean) {}
  render(): string {
    const mark = this.checked ? "x" : " ";
    return `[${mark}] checkbox`;
  }
}

// One array holds DIFFERENT concrete types behind the Renderer interface.
function renderAll(widgets: Renderer[]): void {
  for (const widget of widgets) {
    console.log(widget.render());
  }
}

const widgets: Renderer[] = [
  new Button("Save"),
  new Checkbox(true),
  new Button("Cancel"),
];

renderAll(widgets);
```

**Output (Node v22):**

```text
<button>Save</button>
[x] checkbox
<button>Cancel</button>
```

**Key points:**

- `Renderer[]` holds a mix of `Button` and `Checkbox`. JavaScript objects carry their own method table, so `widget.render()` resolves at runtime to whichever class the object actually is.
- In JavaScript this is the *only* dispatch model — every method call is a runtime property lookup. There is no compile-time specialization to opt into or out of.

---

## Rust Equivalent

The same scenario in Rust. The heterogeneous array becomes `Vec<Box<dyn Renderer>>`, and `widget.render()` performs dynamic dispatch through the trait object.

```rust
// Rust - a shared trait, several structs, one heterogeneous Vec of trait objects
trait Renderer {
    fn render(&self) -> String;
}

struct Button {
    label: String,
}

struct Checkbox {
    checked: bool,
}

impl Renderer for Button {
    fn render(&self) -> String {
        format!("<button>{}</button>", self.label)
    }
}

impl Renderer for Checkbox {
    fn render(&self) -> String {
        let mark = if self.checked { "x" } else { " " };
        format!("[{mark}] checkbox")
    }
}

// Takes a slice of trait objects: different concrete types, one interface.
fn render_all(widgets: &[Box<dyn Renderer>]) {
    for widget in widgets {
        println!("{}", widget.render());
    }
}

fn main() {
    let widgets: Vec<Box<dyn Renderer>> = vec![
        Box::new(Button { label: "Save".to_string() }),
        Box::new(Checkbox { checked: true }),
        Box::new(Button { label: "Cancel".to_string() }),
    ];

    render_all(&widgets);
}
```

**Real output:**

```text
<button>Save</button>
[x] checkbox
<button>Cancel</button>
```

Same result as the TypeScript version. The difference is conceptual: `Vec<Box<dyn Renderer>>` is the one place we *asked* for runtime dispatch. A plain generic `Vec<T>` could only hold one concrete type; `Box<dyn Renderer>` is what lets `Button` and `Checkbox` live in the same vector.

> **Note:** All examples here target current stable **Rust 1.96.0** on the latest stable edition (2024); `cargo new` selects the newest edition automatically. `Box` is the heap-allocating smart pointer covered in [Section 10: Smart Pointers](/10-smart-pointers/).

---

## Detailed Explanation

### What `dyn Trait` actually is

`dyn Renderer` is an **unsized type**: on its own the compiler does not know how big it is, because at compile time it could be a `Button` (one field) or a `Checkbox` (a different field). You can never hold a bare `dyn Renderer` by value; you always access it **behind a pointer**: `&dyn Renderer`, `&mut dyn Renderer`, `Box<dyn Renderer>`, `Rc<dyn Renderer>`, and so on.

That pointer is special: it is a **fat pointer**, two machine words wide instead of one. The first word points at the data (the actual `Button`); the second word points at a **vtable**, a small table of function pointers for that type's trait methods. When you call `widget.render()`, Rust reads the function pointer out of the vtable and calls it. That indirection *is* dynamic dispatch.

We can see the fat pointer with `std::mem::size_of`:

```rust
use std::mem::size_of;

trait Shape {
    fn area(&self) -> f64;
}

struct Circle {
    radius: f64,
}

struct Rect {
    w: f64,
    h: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.radius * self.radius
    }
}

impl Shape for Rect {
    fn area(&self) -> f64 {
        self.w * self.h
    }
}

fn main() {
    // A normal reference is one word (8 bytes on a 64-bit target).
    println!("size of &Circle:        {}", size_of::<&Circle>());
    // A trait-object reference is TWO words: data ptr + vtable ptr.
    println!("size of &dyn Shape:     {}", size_of::<&dyn Shape>());
    println!("size of Box<Circle>:    {}", size_of::<Box<Circle>>());
    println!("size of Box<dyn Shape>: {}", size_of::<Box<dyn Shape>>());
}
```

**Real output:**

```text
size of &Circle:        8
size of &dyn Shape:     16
size of Box<Circle>:    8
size of Box<dyn Shape>: 16
```

The trait-object pointers are 16 bytes (two 8-byte words); the concrete pointers are 8. That extra word is the vtable pointer.

> **Tip:** TypeScript has no "fat pointer" because every JavaScript object already carries its prototype chain (its method table) with it. Rust separates data from behavior, so when it needs runtime dispatch it bolts on a vtable pointer just for the duration the value is viewed as `dyn Trait`.

### `&dyn Trait` versus `Box<dyn Trait>`

These are the two forms you will use constantly. The difference is **ownership**, exactly the same distinction as anywhere else in Rust (see [Section 05: Ownership](/05-ownership/)):

```rust
trait Speak {
    fn speak(&self) -> String;
}

struct Dog;
struct Cat;

impl Speak for Dog {
    fn speak(&self) -> String {
        "woof".to_string()
    }
}

impl Speak for Cat {
    fn speak(&self) -> String {
        "meow".to_string()
    }
}

// Borrows a trait object: caller keeps ownership, no allocation.
fn announce(s: &dyn Speak) {
    println!("{}", s.speak());
}

fn main() {
    let d = Dog;
    let boxed: Box<dyn Speak> = Box::new(Cat);

    // A `&Dog` coerces automatically to `&dyn Speak` (an "unsized coercion").
    announce(&d);

    // A `Box<dyn Speak>` derefs to `&dyn Speak` for the borrow.
    announce(&*boxed);
    announce(boxed.as_ref());
}
```

**Real output:**

```text
woof
meow
meow
```

- **`&dyn Trait`** borrows an existing value. No heap allocation, no ownership transfer. Reach for this when a function just needs to *use* a trait object passed in. A concrete `&Dog` coerces into `&dyn Speak` automatically.
- **`Box<dyn Trait>`** owns the value on the heap. Reach for this when you need to *store* trait objects (in a `Vec`, in a struct field) or *return* one from a function: anywhere the value must outlive the current scope and you cannot tie it to a borrow.

### Calling a method dispatches through the vtable

When `s.speak()` runs on a `&dyn Speak`, the compiler does not know (and does not need to know) whether `s` is a `Dog` or a `Cat`. It loads the `speak` function pointer from the vtable attached to `s` and calls it. This is the same runtime model as a JavaScript method call, and the opposite of a generic `fn announce<T: Speak>(s: &T)`, where the compiler monomorphizes a separate `announce` for `Dog` and for `Cat`, each calling `speak` directly with no lookup.

### Static dispatch vs dynamic dispatch, side by side

Here is the same capability expressed both ways:

```rust
trait Shape {
    fn area(&self) -> f64;
}

struct Circle {
    radius: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.radius * self.radius
    }
}

// STATIC dispatch: generic + trait bound. Monomorphized per type, inlinable,
// zero runtime lookup. (See trait-bounds.md and generic-functions.md.)
fn print_area_static<T: Shape>(shape: &T) {
    println!("{:.2}", shape.area());
}

// DYNAMIC dispatch: trait object. One code path, vtable lookup at call time.
fn print_area_dynamic(shape: &dyn Shape) {
    println!("{:.2}", shape.area());
}

fn main() {
    let c = Circle { radius: 2.0 };
    print_area_static(&c);
    print_area_dynamic(&c);
}
```

**Real output:**

```text
12.57
12.57
```

Both print the same thing; the machine code differs. `print_area_static` gets a dedicated copy stamped out for `Circle` with `area` likely inlined. `print_area_dynamic` is compiled once and reads the function pointer from the vtable at runtime. The trade-off (code size and speed vs flexibility and binary footprint) is the heart of choosing between them, summarized in the next section.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Default dispatch | Always dynamic (runtime property lookup) | **Static** (monomorphized generics) |
| Opt into dynamic dispatch | N/A (it is the only model) | Explicit: `dyn Trait` |
| Heterogeneous collection | `Renderer[]`, trivial | `Vec<Box<dyn Renderer>>`, requires `dyn` |
| Method-table storage | On every object (prototype chain) | Separate vtable, referenced by a fat pointer |
| Pointer width for the object | N/A (objects are references) | `&dyn`/`Box<dyn>` are **two** words (fat) |
| Can every type be used this way? | Yes | Only **dyn-compatible** traits |
| Runtime cost | Always a lookup | Lookup only when you choose `dyn` |
| Inlining the method | Never (engine may JIT-optimize) | Yes for static dispatch; not across `dyn` |

### When to choose which

- **Reach for static dispatch (generics) by default.** It is zero-cost: no vtable, fully inlinable, the fastest option. Use it whenever the set of types is known at compile time and a function works with *one* type per call.
- **Reach for `dyn Trait` when you genuinely need runtime variety in one place:** a heterogeneous collection (`Vec<Box<dyn Trait>>`), a plugin/handler list assembled at runtime, returning different concrete types from different branches of a function, or wanting to *avoid* monomorphization bloat: one shared function instead of dozens of stamped-out copies in the binary.

> **Note:** The performance gap is usually small for typical code: a vtable call is one extra pointer indirection. Do not contort your design to avoid `dyn`; reach for it when it expresses your intent. But also do not default to it out of TypeScript habit, because Rust's static dispatch is genuinely free.

### Dyn compatibility (formerly "object safety")

Not every trait can become a `dyn Trait`. A trait is **dyn-compatible** only if the compiler can build a single vtable for it. The two rules you will hit most often:

1. **No method may return `Self`.** The whole point of a trait object is to erase the concrete type, so a method that promises to return "the same type as the receiver" cannot be expressed.
2. **No method may have its own generic type parameters.** A vtable has one slot per method, but a generic method like `fn store<T>(&mut self, item: T)` would need a different implementation per `T` — there is no single function pointer to store.

(Other rules exist, for example the trait cannot require `Sized`, but these two cause the vast majority of real-world errors.) The terminology recently changed: the compiler and docs now say **"dyn compatible"** where older material says **"object safe."** They mean the same thing. We will see the exact compiler errors in Common Pitfalls.

---

## Common Pitfalls

### Pitfall 1: Trying to put different concrete types in one collection without `dyn`

Coming from TypeScript, `[new Button(), new Checkbox()]` "just works," so the instinct is to write the equivalent `Vec` directly. Rust infers the element type from the first element and then rejects the second.

```rust
trait Renderer { fn render(&self) -> String; }
struct Button { label: String }
struct Checkbox { checked: bool }
impl Renderer for Button { fn render(&self) -> String { self.label.clone() } }
impl Renderer for Checkbox { fn render(&self) -> String { format!("{}", self.checked) } }

fn main() {
    // does not compile (error[E0308]: mismatched types)
    let widgets = vec![
        Button { label: "Save".to_string() },
        Checkbox { checked: true },
    ];
    let _ = widgets;
}
```

Real compiler error:

```text
error[E0308]: mismatched types
  --> src/main.rs:11:9
   |
11 |         Checkbox { checked: true },
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `Button`, found `Checkbox`
```

**Fix:** make the element type a trait object and box each element, annotating the `Vec`:

```rust
trait Renderer {
    fn render(&self) -> String;
}

struct Button {
    label: String,
}

struct Checkbox {
    checked: bool,
}

impl Renderer for Button {
    fn render(&self) -> String {
        self.label.clone()
    }
}

impl Renderer for Checkbox {
    fn render(&self) -> String {
        format!("{}", self.checked)
    }
}

fn main() {
    let widgets: Vec<Box<dyn Renderer>> = vec![
        Box::new(Button { label: "Save".to_string() }),
        Box::new(Checkbox { checked: true }),
    ];
    let _ = widgets;
}
```

### Pitfall 2: Returning a bare `dyn Trait` by value

A `dyn Trait` is unsized, so you cannot return it directly: there is no known size to put on the stack. This trips up developers who want a "factory" returning different types from different branches.

```rust
trait Animal { fn speak(&self) -> String; }
struct Dog;
struct Cat;
impl Animal for Dog { fn speak(&self) -> String { "woof".into() } }
impl Animal for Cat { fn speak(&self) -> String { "meow".into() } }

// does not compile (error[E0746]: return type cannot be a trait object without pointer indirection)
fn make(loud: bool) -> dyn Animal {
    if loud { Dog } else { Cat }
}

fn main() {
    let _ = make(true);
}
```

Real compiler error (abridged):

```text
error[E0746]: return type cannot be a trait object without pointer indirection
 --> src/main.rs:7:24
  |
7 | fn make(loud: bool) -> dyn Animal {
  |                        ^^^^^^^^^^ doesn't have a size known at compile-time
  |
help: consider returning an `impl Trait` instead of a `dyn Trait`
  |
7 - fn make(loud: bool) -> dyn Animal {
7 + fn make(loud: bool) -> impl Animal {
  |
help: alternatively, box the return type, and wrap all of the returned values in `Box::new`
  |
7 ~ fn make(loud: bool) -> Box<dyn Animal> {
8 ~     if loud { Box::new(Dog) } else { Box::new(Cat) }
  |
```

**Fix:** box it. The compiler's second suggestion is exactly right when the branches return *different* types:

```rust
trait Animal {
    fn speak(&self) -> String;
}

struct Dog;
struct Cat;

impl Animal for Dog {
    fn speak(&self) -> String {
        "woof".into()
    }
}

impl Animal for Cat {
    fn speak(&self) -> String {
        "meow".into()
    }
}

fn make(loud: bool) -> Box<dyn Animal> {
    if loud { Box::new(Dog) } else { Box::new(Cat) }
}

fn main() {
    println!("{}", make(true).speak());
    println!("{}", make(false).speak());
}
```

This prints `woof` then `meow`. (The compiler's *first* suggestion, `impl Animal`, only works when *every* branch returns the **same** concrete type; that is the static-dispatch alternative covered in [`impl Trait`](/09-generics-traits/07-impl-trait/). Here the branches differ, so `Box<dyn Animal>` is the right call.)

### Pitfall 3: A method that returns `Self` breaks dyn compatibility

You define a sensible-looking trait, then try to use it as `dyn` and the compiler refuses, not at the trait definition, but at the point you ask for the trait object.

```rust
// does not compile (error[E0038]: the trait `Cloneable` is not dyn compatible)
trait Cloneable {
    fn duplicate(&self) -> Self;
}

fn main() {
    let _x: &dyn Cloneable = todo!();
}
```

Real compiler error (abridged):

```text
error[E0038]: the trait `Cloneable` is not dyn compatible
 --> src/main.rs:7:18
  |
7 |     let _x: &dyn Cloneable = todo!();
  |                  ^^^^^^^^^ `Cloneable` is not dyn compatible
  |
note: for a trait to be dyn compatible it needs to allow building a vtable
...
3 |     fn duplicate(&self) -> Self;
  |                            ^^^^ ...because method `duplicate` references the `Self` type in its return type
  = help: consider moving `duplicate` to another trait
```

**Why:** a trait object has erased the concrete type, so "return `Self`" has no meaning. **Fix:** if you only need `duplicate` on the concrete type, gate it behind `where Self: Sized` so it is excluded from the vtable but still callable on the real type (shown in Best Practices). If you need cloning *through* the trait object, the standard pattern is a `fn clone_box(&self) -> Box<dyn Trait>` method that boxes a copy.

### Pitfall 4: A generic method breaks dyn compatibility

The other common dyn-compatibility violation: a trait method with its own type parameter.

```rust
// does not compile (error[E0038]: the trait `Container` is not dyn compatible)
trait Container {
    fn store<T>(&mut self, item: T);
}

fn main() {
    let _x: Box<dyn Container> = todo!();
}
```

Real compiler error (abridged):

```text
error[E0038]: the trait `Container` is not dyn compatible
 --> src/main.rs:7:21
  |
7 |     let _x: Box<dyn Container> = todo!();
  |                     ^^^^^^^^^ `Container` is not dyn compatible
...
3 |     fn store<T>(&mut self, item: T);
  |        ^^^^^ ...because method `store` has generic type parameters
  = help: consider moving `store` to another trait
```

**Why:** the vtable has exactly one slot per method, but a generic method would need a distinct compiled function for every `T` it is ever called with: there is no single pointer to put in the slot. **Fix:** if the type set is small and known, replace the generic parameter with a concrete type or an enum; otherwise keep that method out of the trait object (again, `where Self: Sized`).

### Pitfall 5: Reaching for `dyn` when a generic would do

The mirror image of the TypeScript instinct: because dynamic dispatch is *all* you ever had in JavaScript, it is tempting to make every trait-using function take `&dyn Trait`. If a function only ever handles one concrete type per call and you do not need a heterogeneous collection, a generic is simpler *and* faster:

```rust
trait Shape {
    fn area(&self) -> f64;
}

// Prefer this (static dispatch, zero-cost) ...
fn describe_generic<T: Shape>(shape: &T) -> f64 {
    shape.area()
}

// ... over this, unless you specifically need runtime variety.
fn describe_dyn(shape: &dyn Shape) -> f64 {
    shape.area()
}

struct Sq {
    s: f64,
}

impl Shape for Sq {
    fn area(&self) -> f64 {
        self.s * self.s
    }
}

fn main() {
    let q = Sq { s: 2.0 };
    println!("{} {}", describe_generic(&q), describe_dyn(&q));
}
```

Both compile and behave identically; the generic version is the idiomatic default. Save `dyn` for where its flexibility earns its keep.

---

## Best Practices

### Default to generics; reach for `dyn` deliberately

Use a generic with a trait bound (`fn f<T: Trait>(x: &T)`) unless you have a concrete reason for dynamic dispatch: a mixed-type collection, a runtime-chosen return type, plugin registration, or a desire to shrink the binary by sharing one code path. This is the single most important habit for a TypeScript developer to build, because the JavaScript world only ever offered dynamic dispatch.

### Box trait objects to own them; borrow them to use them

Store and return owned trait objects as `Box<dyn Trait>` (or `Rc<dyn Trait>` / `Arc<dyn Trait>` when you need shared ownership — see [Section 10: Smart Pointers](/10-smart-pointers/)). Accept `&dyn Trait` in functions that only *use* the value, so callers are not forced to allocate.

### Keep traits dyn-compatible when you intend to use them as objects

If a trait is meant to be used behind `dyn`, avoid `Self`-returning methods and generic methods in its core surface. When you *do* need such a method only on concrete types, fence it off with `where Self: Sized` so the rest of the trait stays usable as an object:

```rust
trait Greeter {
    fn greet(&self) -> String;

    // Excluded from the vtable, so the trait remains dyn-compatible.
    fn duplicate(&self) -> Self
    where
        Self: Sized;
}

struct Hello;

impl Greeter for Hello {
    fn greet(&self) -> String {
        "hello".to_string()
    }
    fn duplicate(&self) -> Self {
        Hello
    }
}

fn main() {
    // Usable as a trait object because `duplicate` is gated behind `Self: Sized`.
    let g: Box<dyn Greeter> = Box::new(Hello);
    println!("{}", g.greet());

    // `duplicate` is callable only on the concrete type, never through `dyn`.
    let concrete = Hello;
    let _copy = concrete.duplicate();
    println!("duplicated ok");
}
```

**Real output:**

```text
hello
duplicated ok
```

### Name the type explicitly when collecting trait objects

Write `let v: Vec<Box<dyn Trait>> = vec![...]`. The annotation tells the compiler to coerce each `Box::new(Concrete)` into the trait-object type rather than inferring a single concrete element type (which causes Pitfall 1).

### Prefer enums over trait objects for a small, closed set of types

If the variety is *closed* — you know all the variants up front and they will not be extended by other crates — a plain `enum` with a `match` is often clearer and faster than `dyn`, with no allocation and exhaustiveness checking. Use trait objects when the set is *open* (third parties can add types) or genuinely large. Enums are covered in [Section 06: Data Structures](/06-data-structures/) and [Generic Enums](/09-generics-traits/02-generic-enums/).

---

## Real-World Example

A middleware pipeline, the kind you would find behind an HTTP server or a CLI request handler. Each middleware is its own type implementing a shared `Middleware` trait; the pipeline stores them as a heterogeneous, runtime-assembled list of owned trait objects and runs them in order. This is a textbook use of dynamic dispatch: the set of stages is decided at runtime and they are different types living in one `Vec`.

```rust
#[derive(Debug)]
struct Request {
    path: String,
    user: Option<String>,
    log: Vec<String>,
}

// The shared contract. `&self` and `&mut` receivers are fine for dyn compatibility;
// there are no `Self` returns or generic methods, so this trait is dyn-compatible.
trait Middleware {
    fn name(&self) -> &str;
    fn handle(&self, req: &mut Request);
}

struct Logger;
struct Auth {
    required: bool,
}
struct RateLimit {
    max_per_min: u32,
}

impl Middleware for Logger {
    fn name(&self) -> &str {
        "logger"
    }
    fn handle(&self, req: &mut Request) {
        req.log.push(format!("[logger] {}", req.path));
    }
}

impl Middleware for Auth {
    fn name(&self) -> &str {
        "auth"
    }
    fn handle(&self, req: &mut Request) {
        if self.required && req.user.is_none() {
            req.log.push("[auth] anonymous request flagged".to_string());
        } else {
            req.log.push("[auth] ok".to_string());
        }
    }
}

impl Middleware for RateLimit {
    fn name(&self) -> &str {
        "rate-limit"
    }
    fn handle(&self, req: &mut Request) {
        req.log
            .push(format!("[rate-limit] budget {}/min", self.max_per_min));
    }
}

/// Holds a heterogeneous list of middleware as owned trait objects.
struct Pipeline {
    stages: Vec<Box<dyn Middleware>>,
}

impl Pipeline {
    fn new() -> Self {
        Pipeline { stages: Vec::new() }
    }

    // Accepts ANY concrete middleware type, boxed as a trait object.
    fn add(mut self, stage: Box<dyn Middleware>) -> Self {
        self.stages.push(stage);
        self
    }

    fn run(&self, req: &mut Request) {
        for stage in &self.stages {
            println!("running {}", stage.name()); // dynamic dispatch
            stage.handle(req); // dynamic dispatch
        }
    }
}

fn main() {
    let pipeline = Pipeline::new()
        .add(Box::new(Logger))
        .add(Box::new(Auth { required: true }))
        .add(Box::new(RateLimit { max_per_min: 60 }));

    let mut req = Request {
        path: "/dashboard".to_string(),
        user: None,
        log: Vec::new(),
    };

    pipeline.run(&mut req);

    println!("--- request log ---");
    for line in &req.log {
        println!("{line}");
    }
}
```

**Real output:**

```text
running logger
running auth
running rate-limit
--- request log ---
[logger] /dashboard
[auth] anonymous request flagged
[rate-limit] budget 60/min
```

Two things make this a natural fit for trait objects. First, the pipeline is *assembled at runtime* from different concrete types, exactly what `Vec<Box<dyn Middleware>>` is for; a generic `Vec<T>` could hold only one stage type. Second, the design is open: adding a new middleware (say, `Compression`) is purely additive. Define the struct, write one `impl Middleware for Compression`, `.add(Box::new(Compression))`, and `Pipeline` needs no changes. The cost is one vtable lookup per `name()`/`handle()` call, which is negligible next to the work a real middleware does.

> **Tip:** If your pipeline stages were a fixed, closed set known at compile time, an `enum Stage { Logger, Auth(Auth), RateLimit(RateLimit) }` with a `match` would avoid the allocation and the dispatch. The `dyn` approach earns its place precisely because new stage types can come from anywhere.

---

## Further Reading

### Official documentation

- [The Rust Book — Using Trait Objects That Allow for Values of Different Types](https://doc.rust-lang.org/book/ch18-02-trait-objects.html)
- [The Rust Reference — Trait objects](https://doc.rust-lang.org/reference/types/trait-object.html)
- [The Rust Reference — `dyn` compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility)
- [Rust by Example — Returning Traits with `dyn`](https://doc.rust-lang.org/rust-by-example/trait/dyn.html)

### Related topics in this guide

- [Section 09 overview](/09-generics-traits/): the full map of generics and traits
- [Traits](/09-generics-traits/03-traits/): defining and implementing the traits you turn into objects
- [Trait Bounds](/09-generics-traits/05-trait-bounds/): `<T: Trait>` static dispatch, the default alternative to `dyn`
- [Generic Functions](/09-generics-traits/00-generic-functions/): monomorphization vs TypeScript's type erasure
- [`impl Trait`](/09-generics-traits/07-impl-trait/) — `impl Trait` returns: static dispatch when every branch shares one type
- [Trait Methods](/09-generics-traits/04-trait-methods/): required vs provided methods (provided methods appear in vtables too)
- [Marker Traits](/09-generics-traits/11-marker-traits/): `Sized` and why `dyn Trait` is unsized
- [Generic Enums](/09-generics-traits/02-generic-enums/): enums as the closed-set alternative to trait objects
- [Section 05: Ownership](/05-ownership/): what `&dyn` (borrow) vs `Box<dyn>` (owned) means
- [Section 06: Data Structures](/06-data-structures/): enums, the closed-set alternative
- [Section 10: Smart Pointers](/10-smart-pointers/): `Box<dyn Trait>`, plus `Rc`/`Arc<dyn Trait>` for shared ownership
- Foundations: [Getting Started](/01-getting-started/) and [Basics](/02-basics/)

---

## Exercises

### Exercise 1: A heterogeneous drawing list

**Difficulty:** Easy

**Objective:** Build and iterate a `Vec` of trait objects.

**Instructions:** Define a trait `Drawable` with one required method `draw(&self) -> String`. Implement it for two unit structs, `Line` (returns `"---"`) and `Dot` (returns `"."`). Write a function `draw_all(items: &[Box<dyn Drawable>])` that prints each item's `draw()` output. In `main`, build a `Vec<Box<dyn Drawable>>` containing a `Line`, a `Dot`, and another `Line`, then call `draw_all`.

```rust
trait Drawable {
    // TODO: draw(&self) -> String
}

struct Line;
struct Dot;

// TODO: impl Drawable for Line and Dot

fn draw_all(items: &[Box<dyn Drawable>]) {
    // TODO: print each item's draw() output
}

fn main() {
    // TODO: build a Vec<Box<dyn Drawable>> and call draw_all
}
```

<details>
<summary>Solution</summary>

```rust
trait Drawable {
    fn draw(&self) -> String;
}

struct Line;
struct Dot;

impl Drawable for Line {
    fn draw(&self) -> String {
        "---".to_string()
    }
}

impl Drawable for Dot {
    fn draw(&self) -> String {
        ".".to_string()
    }
}

fn draw_all(items: &[Box<dyn Drawable>]) {
    for item in items {
        println!("{}", item.draw());
    }
}

fn main() {
    let items: Vec<Box<dyn Drawable>> = vec![Box::new(Line), Box::new(Dot), Box::new(Line)];
    draw_all(&items);
}
```

**Output:**

```text
---
.
---
```

The `Vec<Box<dyn Drawable>>` annotation is what lets `Line` and `Dot`, two different types, share one vector; each `Box::new(...)` coerces into the trait-object element type.

</details>

### Exercise 2: A runtime-chosen factory returning `Box<dyn Trait>`

**Difficulty:** Medium

**Objective:** Return different concrete types from one function via a boxed trait object, and accept a trait object by borrow.

**Instructions:** Define a trait `Formatter` with `format(&self, value: f64) -> String`. Implement it for `Currency { symbol: String }` (e.g. `$19.50`, two decimals) and a unit struct `Percent` (e.g. `7.5%`, value times 100, one decimal). Write `formatter_for(kind: &str) -> Box<dyn Formatter>` returning a `Currency { symbol: "$" }` for `"usd"`, `Currency { symbol: "EUR " }` for `"eur"`, and `Percent` for anything else. Write `show(formatter: &dyn Formatter, value: f64)` that prints `formatter.format(value)`. In `main`, build a USD formatter and a fallback formatter and show a value with each.

```rust
trait Formatter {
    // TODO: format(&self, value: f64) -> String
}

struct Currency { symbol: String }
struct Percent;

// TODO: impl Formatter for Currency and Percent

fn formatter_for(kind: &str) -> Box<dyn Formatter> {
    // TODO: return the right boxed formatter
}

fn show(formatter: &dyn Formatter, value: f64) {
    // TODO: print formatter.format(value)
}

fn main() {
    // TODO: build formatters via formatter_for and call show
}
```

<details>
<summary>Solution</summary>

```rust
trait Formatter {
    fn format(&self, value: f64) -> String;
}

struct Currency {
    symbol: String,
}

struct Percent;

impl Formatter for Currency {
    fn format(&self, value: f64) -> String {
        format!("{}{:.2}", self.symbol, value)
    }
}

impl Formatter for Percent {
    fn format(&self, value: f64) -> String {
        format!("{:.1}%", value * 100.0)
    }
}

fn formatter_for(kind: &str) -> Box<dyn Formatter> {
    match kind {
        "usd" => Box::new(Currency { symbol: "$".to_string() }),
        "eur" => Box::new(Currency { symbol: "EUR ".to_string() }),
        _ => Box::new(Percent),
    }
}

fn show(formatter: &dyn Formatter, value: f64) {
    println!("{}", formatter.format(value));
}

fn main() {
    let usd = formatter_for("usd");
    let pct = formatter_for("rate"); // falls through to Percent

    // `&Box<dyn Formatter>` derefs to `&dyn Formatter`; `as_ref()` is explicit.
    show(usd.as_ref(), 19.5);
    show(pct.as_ref(), 0.075);
}
```

**Output:**

```text
$19.50
7.5%
```

`formatter_for` returns `Box<dyn Formatter>` because the branches produce different concrete types; `impl Formatter` would not work here. `show` borrows the trait object as `&dyn Formatter`, so it never takes ownership or allocates.

</details>

### Exercise 3: A mutable event bus

**Difficulty:** Hard

**Objective:** Store stateful trait objects, call them through `&mut`, and confirm a `&mut self` method keeps the trait dyn-compatible.

**Instructions:** Define a trait `Handler` with `on_event(&mut self, event: &str) -> String`. Implement it for `Counter { count: u32 }` — it increments `count` and returns `"counter saw '<event>' (total <n>)"` — and a unit struct `Echo` returning `"echo: <event>"`. Build a `Bus` struct holding `Vec<Box<dyn Handler>>` with `new()`, `register(&mut self, handler: Box<dyn Handler>)`, and `dispatch(&mut self, event: &str)` (which calls `on_event` on every handler and prints the result; you will need `iter_mut()` because `on_event` takes `&mut self`). In `main`, register a `Counter` and an `Echo`, then dispatch `"login"` and `"logout"`.

```rust
trait Handler {
    // TODO: on_event(&mut self, event: &str) -> String
}

struct Counter { count: u32 }
struct Echo;

// TODO: impl Handler for Counter and Echo

struct Bus {
    handlers: Vec<Box<dyn Handler>>,
}

impl Bus {
    // TODO: new, register, dispatch
}

fn main() {
    // TODO: register a Counter and an Echo, dispatch two events
}
```

<details>
<summary>Solution</summary>

```rust
trait Handler {
    fn on_event(&mut self, event: &str) -> String;
}

struct Counter {
    count: u32,
}

struct Echo;

impl Handler for Counter {
    fn on_event(&mut self, event: &str) -> String {
        self.count += 1;
        format!("counter saw '{event}' (total {})", self.count)
    }
}

impl Handler for Echo {
    fn on_event(&mut self, event: &str) -> String {
        format!("echo: {event}")
    }
}

struct Bus {
    handlers: Vec<Box<dyn Handler>>,
}

impl Bus {
    fn new() -> Self {
        Bus { handlers: Vec::new() }
    }

    fn register(&mut self, handler: Box<dyn Handler>) {
        self.handlers.push(handler);
    }

    fn dispatch(&mut self, event: &str) {
        // iter_mut() yields `&mut Box<dyn Handler>`, so `on_event(&mut self)` is callable.
        for handler in self.handlers.iter_mut() {
            println!("{}", handler.on_event(event));
        }
    }
}

fn main() {
    let mut bus = Bus::new();
    bus.register(Box::new(Counter { count: 0 }));
    bus.register(Box::new(Echo));

    bus.dispatch("login");
    bus.dispatch("logout");
}
```

**Output:**

```text
counter saw 'login' (total 1)
echo: login
counter saw 'logout' (total 2)
echo: logout
```

The key detail: `on_event` takes `&mut self`, which is perfectly dyn-compatible; only `Self`-returning and generic methods break dyn compatibility. Iterating with `iter_mut()` (not `iter()`) gives the mutable access each `on_event` call needs, and the `Counter`'s state persists across dispatches because the `Box<dyn Handler>` owns it inside the `Vec`.

</details>
