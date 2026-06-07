---
title: "Default Method Implementations: Boilerplate Elimination"
description: "Rust default trait methods ship a working body once so every implementor inherits it, like a TypeScript abstract class but extendable to types you don't own"
---

Writing the same helper method on twenty classes is one of the quiet taxes of object-oriented TypeScript. A Rust **default method** lets a trait ship a working body *once*, so every implementor inherits it for free. This file is about that single idea taken seriously: how a well-designed trait turns *one* method you must write into a *whole API* you get for nothing, and where that advantage comes from.

---

## Quick Overview

A **default (provided) method** is a trait method with a body. Implementors inherit it automatically and may override it. The payoff is reuse: a trait can demand one small required method and then build a dozen useful methods on top of it, all written once in the trait. The standard library's `Iterator` is the poster child: you write `next`, and `map`, `filter`, `sum`, `collect`, and ~70 more arrive for free. The closest TypeScript analogy is a concrete method on an abstract class, but a Rust trait can hand the same defaults to types you do not own, and even to *every* type that meets a bound.

> **Note:** This file focuses on **using defaults to cut boilerplate**: the design patterns and economics. For the mechanical rules (required vs provided, the three call syntaxes, why there is no `super`), read the sibling [Trait Methods](/09-generics-traits/04-trait-methods/) first; this file does not repeat them.

---

## TypeScript/JavaScript Example

A common TypeScript pattern: an `abstract class` where subclasses implement one piece and inherit a pile of concrete helpers. Here every responder must supply a `body()`, but `status()`, `contentType()`, `headers()`, and `render()` come pre-built.

```typescript
// TypeScript - abstract class supplying default ("concrete") methods
abstract class HttpResponder {
  // The ONE thing each subclass must implement.
  abstract body(): string;

  // Concrete defaults the subclass inherits unless it overrides them.
  status(): number {
    return 200;
  }
  contentType(): string {
    return "text/plain";
  }
  headers(): [string, string][] {
    return [
      ["Content-Type", this.contentType()],
      ["Content-Length", String(this.body().length)],
    ];
  }
  render(): string {
    const reason = { 200: "OK", 404: "Not Found" }[this.status()] ?? "Unknown";
    let out = `HTTP/1.1 ${this.status()} ${reason}\n`;
    for (const [k, v] of this.headers()) out += `${k}: ${v}\n`;
    out += "\n" + this.body();
    return out;
  }
}

class Welcome extends HttpResponder {
  body(): string {
    return "Welcome!";
  }
  // Inherits status, contentType, headers, render.
}

class NotFound extends HttpResponder {
  body(): string {
    return "<h1>404</h1>";
  }
  status(): number {
    return 404;
  }
  contentType(): string {
    return "text/html";
  }
  // Still inherits headers and render.
}

console.log(new Welcome().render());
console.log("----");
console.log(new NotFound().render());
```

Running this under Node v22 prints:

```text
HTTP/1.1 200 OK
Content-Type: text/plain
Content-Length: 8

Welcome!
----
HTTP/1.1 404 Not Found
Content-Type: text/html
Content-Length: 12

<h1>404</h1>
```

This works, but it leans on **class inheritance**: `Welcome` and `NotFound` *are* `HttpResponder`s. You can only attach these defaults to types you define and `extends`. Keep that constraint in mind: Rust lifts it.

---

## Rust Equivalent

The same design as a trait. The single required method is `body`; everything else is a default that implementors inherit.

```rust
trait HttpResponder {
    // The ONE required method each responder must supply.
    fn body(&self) -> String;

    // Everything below is a provided (default) method: free boilerplate.
    fn status(&self) -> u16 {
        200
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }

    fn headers(&self) -> Vec<(String, String)> {
        vec![
            ("Content-Type".to_string(), self.content_type().to_string()),
            ("Content-Length".to_string(), self.body().len().to_string()),
        ]
    }

    fn render(&self) -> String {
        let mut out = format!("HTTP/1.1 {} {}\n", self.status(), reason(self.status()));
        for (k, v) in self.headers() {
            out.push_str(&format!("{k}: {v}\n"));
        }
        out.push('\n');
        out.push_str(&self.body());
        out
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Unknown",
    }
}

struct Welcome;
impl HttpResponder for Welcome {
    fn body(&self) -> String {
        "Welcome!".to_string()
    }
    // Inherits status (200), content_type, headers, render — all for free.
}

struct NotFound;
impl HttpResponder for NotFound {
    fn body(&self) -> String {
        "<h1>404</h1>".to_string()
    }
    fn status(&self) -> u16 {
        404
    }
    fn content_type(&self) -> &str {
        "text/html"
    }
    // Still inherits headers and render.
}

fn main() {
    println!("{}", Welcome.render());
    println!("----");
    println!("{}", NotFound.render());
}
```

Real output (identical to the TypeScript run above):

```text
HTTP/1.1 200 OK
Content-Type: text/plain
Content-Length: 8

Welcome!
----
HTTP/1.1 404 Not Found
Content-Type: text/html
Content-Length: 12

<h1>404</h1>
```

`Welcome` writes three lines of real code (`body`) and inherits four methods. `NotFound` overrides two and still inherits `headers` and `render`. The boilerplate lives in *one* place: the trait.

---

## Detailed Explanation

### The leverage: defaults call the required method

The whole trick is that a default body may call **other trait methods**, including the required ones. `render` never knows what `body` returns or what `status` a given type uses. It calls `self.body()` and `self.status()` and lets each concrete type fill in the blanks. So the *orchestration* is written once, and each implementor supplies only the *small required pieces*.

This is why a tiny required surface produces a large free API. Consider a reporting trait whose only required method is `rows`:

```rust
trait Report {
    // Required: the raw rows.
    fn rows(&self) -> Vec<String>;

    // Provided defaults derived entirely from `rows`.
    fn count(&self) -> usize {
        self.rows().len()
    }
    fn is_empty(&self) -> bool {
        self.count() == 0
    }
    fn first(&self) -> Option<String> {
        self.rows().into_iter().next()
    }
    fn to_csv(&self) -> String {
        self.rows().join(",")
    }
    fn summary(&self) -> String {
        format!("{} row(s): {}", self.count(), self.to_csv())
    }
}

struct SalesReport {
    entries: Vec<String>,
}
impl Report for SalesReport {
    fn rows(&self) -> Vec<String> {
        self.entries.clone()
    }
    // count, is_empty, first, to_csv, summary: all inherited.
}

fn main() {
    let r = SalesReport {
        entries: vec!["jan=10".into(), "feb=12".into(), "mar=9".into()],
    };
    println!("count   = {}", r.count());
    println!("empty   = {}", r.is_empty());
    println!("first   = {:?}", r.first());
    println!("csv     = {}", r.to_csv());
    println!("summary = {}", r.summary());
}
```

Real output:

```text
count   = 3
empty   = false
first   = Some("jan=10")
csv     = jan=10,feb=12,mar=9
summary = 3 row(s): jan=10,feb=12,mar=9
```

`SalesReport` implements one method. It gets five. A second report type — backed by a database, a file, an HTTP call — would also implement only `rows` and inherit the identical query API. That is the boilerplate elimination this file is named for.

> **Tip:** Notice `summary` calls `count` and `to_csv`, which themselves call `rows`. Defaults can layer on defaults. Keep the layering shallow and the required surface tiny.

### Defaults that build on a single comparison

The standard library uses this everywhere. `Ord` requires `cmp` and provides `max`, `min`, and `clamp`; `Iterator` requires `next` and provides the rest. You can mirror the pattern in your own code: define the one operation that only the implementor can know, then derive a family of conveniences.

```rust
use std::cmp::Ordering;

// Define ONE comparison, get a family of methods free.
trait Ranked {
    // Required: how do two of these compare?
    fn rank_cmp(&self, other: &Self) -> Ordering;

    // Provided helpers derived from the single required method.
    fn is_better_than(&self, other: &Self) -> bool {
        self.rank_cmp(other) == Ordering::Greater
    }
    fn max_of<'a>(&'a self, other: &'a Self) -> &'a Self {
        if self.rank_cmp(other) == Ordering::Less {
            other
        } else {
            self
        }
    }
}

struct Player {
    name: &'static str,
    score: u32,
}
impl Ranked for Player {
    fn rank_cmp(&self, other: &Self) -> Ordering {
        self.score.cmp(&other.score)
    }
}

fn main() {
    let a = Player { name: "Ada", score: 90 };
    let b = Player { name: "Bo", score: 75 };
    println!("Ada better than Bo? {}", a.is_better_than(&b));
    println!("winner = {}", a.max_of(&b).name);
}
```

Real output:

```text
Ada better than Bo? true
winner = Ada
```

### The move TypeScript cannot make: defaults for types you do not own

This is where the analogy with abstract classes breaks down, in Rust's favor. A TypeScript abstract class only gives its concrete methods to subclasses you write with `extends`. A Rust trait with defaults can be implemented for *any* type, including a foreign one (within the [orphan rule](/09-generics-traits/12-orphan-rule/)), and a single **blanket implementation** can hand those defaults to *every* type that meets a bound.

This is the **extension trait** pattern: add methods to types you do not own. Here we bolt new methods onto every iterator of `i64` by giving an empty blanket `impl` that simply inherits all the defaults:

```rust
// An "extension trait" that adds methods to ANY iterator-of-numbers via defaults.
trait NumericExt: Iterator<Item = i64> + Sized {
    fn sum_squares(self) -> i64 {
        self.map(|x| x * x).sum()
    }
    fn average(self) -> Option<f64> {
        let v: Vec<i64> = self.collect();
        if v.is_empty() {
            None
        } else {
            Some(v.iter().sum::<i64>() as f64 / v.len() as f64)
        }
    }
}

// One blanket impl gives every qualifying iterator all the default methods.
impl<I: Iterator<Item = i64>> NumericExt for I {}

fn main() {
    let data = [1i64, 2, 3, 4];
    println!("sum_squares = {}", data.iter().copied().sum_squares());
    println!("average     = {:?}", data.iter().copied().average());
    let empty: [i64; 0] = [];
    println!("empty avg   = {:?}", empty.iter().copied().average());
}
```

Real output:

```text
sum_squares = 30
average     = Some(2.5)
empty avg   = None
```

The `impl<I: Iterator<Item = i64>> NumericExt for I {}` body is *empty*: every method comes from the defaults. That one line of `impl` retrofits `sum_squares` and `average` onto arrays, vectors, ranges, hash-map values, anything that iterates `i64`. There is no equivalent in TypeScript short of monkey-patching `Array.prototype`, which is global, untyped, and discouraged. (The generics behind `impl<I: ...>` are covered in [Generic Functions](/09-generics-traits/00-generic-functions/) and [Trait Bounds](/09-generics-traits/05-trait-bounds/); the supertrait bound `: Iterator<...>` is explained in [Supertraits](/09-generics-traits/09-supertraits/).)

### Where the cost goes: monomorphization, not runtime dispatch

In TypeScript, an inherited method is one function in memory; every subclass instance calls the same code via the prototype chain (dynamic dispatch). In Rust, when you call a default method on a concrete type, the compiler **monomorphizes** it: it stamps out a specialized copy as if you had written it by hand on that type, then inlines and optimizes it. So a default method is not "shared code with a virtual call"; it is a template the compiler expands per type. You get the source-level deduplication of inheritance with the runtime profile of hand-written code. (Dynamic dispatch is still available on demand via [trait objects](/09-generics-traits/06-trait-objects/); contrast the two in [Generic Functions](/09-generics-traits/00-generic-functions/).)

---

## Key Differences

| Concern | TypeScript (abstract class) | Rust (trait default method) |
| --- | --- | --- |
| Where defaults live | concrete methods on an `abstract class` | methods with a body in a `trait` |
| Who can inherit them | only subclasses you write with `extends` | **any** type that `impl`s the trait, including foreign types |
| Apply to many types at once | one base class per hierarchy | a single **blanket `impl`** covers every type meeting a bound |
| Dispatch of an inherited method | dynamic, via the prototype chain | **static**, monomorphized + inlined per type (dynamic only via `dyn`) |
| Accessing instance data in a default | `this.field` works directly | only via trait methods; defaults cannot see struct fields |
| Reusing the default after overriding | `super.method()` | no `super`; factor shared logic into a helper method |
| Multiple sources of defaults | single inheritance (one base class) | a type can `impl` many traits, each bringing defaults |

The headline difference for boilerplate: **inheritance ties defaults to a class hierarchy; traits do not.** A type can pick up default-laden behavior from any number of traits, and one blanket `impl` can distribute defaults across an open-ended set of types. That is strictly more reach than `extends`.

> **Warning:** A default method **cannot read the implementing struct's fields** (`self.some_field`). Different implementors have different fields, so the trait has no idea they exist. Expose any per-type data the default needs through a *required* method (a getter) and have the default call that. The first pitfall below shows the exact compiler error.

---

## Common Pitfalls

### Pitfall 1: A default method reaching for a struct field

Coming from TypeScript, where `this.name` is fair game inside an inherited method, the instinct is to write `self.name` in a default. The trait has no fields, so this does not compile.

```rust
trait Named {
    fn greeting(&self) -> String {
        // does not compile (error[E0609]): a trait default can't see fields
        format!("Hi, {}", self.name)
    }
}

struct User {
    name: String,
}
impl Named for User {}

fn main() {
    let u = User { name: "Sam".into() };
    println!("{}", u.greeting());
}
```

The real error:

```text
error[E0609]: no field `name` on type `&Self`
 --> src/main.rs:4:32
  |
1 | trait Named {
  | ----------- type parameter 'Self' declared here
...
4 |         format!("Hi, {}", self.name)
  |                                ^^^^ unknown field

For more information about this error, try `rustc --explain E0609`.
```

The fix is to add a required getter and let the default call it:

```rust
trait Named {
    fn name(&self) -> &str;            // required getter
    fn greeting(&self) -> String {
        format!("Hi, {}", self.name()) // default calls the getter
    }
}

struct User {
    name: String,
}
impl Named for User {
    fn name(&self) -> &str {
        &self.name
    }
}

fn main() {
    let u = User { name: "Sam".into() };
    println!("{}", u.greeting());
}
```

This is the single most common surprise for TypeScript developers, and it is the reason the required-getters-plus-provided-orchestration pattern shows up so often in Rust traits.

### Pitfall 2: Changing the signature when "overriding" a default

An override must match the trait's signature exactly. Tweaking the return type does not overload the method; it stops satisfying the trait.

```rust
trait Pricing {
    fn base(&self) -> u32;
    fn total(&self) -> u32 {
        self.base()
    }
}

struct Item;
impl Pricing for Item {
    fn base(&self) -> u32 {
        100
    }
    // does not compile (error[E0053]): return type must match the trait
    fn total(&self) -> f64 {
        self.base() as f64 * 1.2
    }
}

fn main() {
    println!("{}", Item.total());
}
```

The real error:

```text
error[E0053]: method `total` has an incompatible type for trait
  --> src/main.rs:14:24
   |
14 |     fn total(&self) -> f64 {
   |                        ^^^ expected `u32`, found `f64`
   |
note: type in trait
  --> src/main.rs:3:24
   |
 3 |     fn total(&self) -> u32 {
   |                        ^^^
   = note: expected signature `fn(&Item) -> u32`
              found signature `fn(&Item) -> f64`
help: change the output type to match the trait
   |
14 -     fn total(&self) -> f64 {
14 +     fn total(&self) -> u32 {
   |
```

If you genuinely want a different return shape, that is a *different* method (or a different trait), not an override.

### Pitfall 3: Adding a default whose behavior is wrong for most implementors

A default is silent: a type that forgets to override it still compiles and runs. That is the danger of a default that is right only sometimes. If `serialize` defaults to JSON but half your types need XML, those types will *silently* emit JSON until someone notices in production. When only the implementor can know the right answer, make the method **required** so the compiler forces a decision at `impl` time. Reserve defaults for behavior that is correct (or a deliberately sensible fallback) for essentially every implementor.

### Pitfall 4: Expecting a default to call back into an overridden version via `super`

There is no `super` in Rust. Once a type overrides a default, the trait's original body is not reachable from inside that override; `self.method()` calls the override itself (infinite recursion). The idiom is to put the shared work in its own method that both the default and any override can call. This is covered with the real Clippy output in [Trait Methods → Common Pitfalls](/09-generics-traits/04-trait-methods/#common-pitfalls); mentioned here only so the boilerplate-reduction story is complete.

---

## Best Practices

- **Minimize the required surface, maximize the provided surface.** Aim for the `Iterator` shape: one (or very few) required methods, with everything convenient built on top as defaults. The smaller the required set, the cheaper each new implementor.
- **Write defaults purely in terms of trait methods.** A default that calls only `self`'s other trait methods (never struct fields) stays valid for every present and future implementor. Expose needed data through required getters.
- **Use a blanket `impl` to distribute defaults widely.** The extension-trait pattern (`impl<T: Bound> MyTrait for T {}`) retrofits a whole default API onto every qualifying type: the boilerplate-elimination move that has no clean TypeScript analogue.
- **Default only what is broadly correct; require the rest.** A wrong-by-default method causes silent bugs. When in doubt, make it required and let the compiler demand an explicit choice.
- **Document the override contract.** In `///` doc comments, mark each default as "override to customize X" or "you should not need to touch this", so implementors know which defaults are extension points.
- **Prefer overriding a default for a faster path, not different behavior.** The standard library overrides defaults like `Iterator::count` or `size_hint` for performance while keeping the observable result identical. Use overrides the same way; if you need *different* semantics, reconsider the design.

---

## Real-World Example

A read-side repository trait. A concrete store implements one method, `all`, and inherits an entire query API: count, lookup, existence check, projection, and a generic filter. Swapping the backing store (in-memory here, but it could be SQL or a cache) requires writing only `all` again.

```rust
use std::collections::HashMap;

#[derive(Clone, Debug)]
struct Record {
    id: u32,
    name: String,
}

// A repository trait where ONE backend method (`all`) powers a whole read API.
trait ReadRepository {
    // Required: the single point a concrete store must implement.
    fn all(&self) -> Vec<Record>;

    // Provided: a full query API built on top of `all`, written once.
    fn count(&self) -> usize {
        self.all().len()
    }
    fn find(&self, id: u32) -> Option<Record> {
        self.all().into_iter().find(|r| r.id == id)
    }
    fn exists(&self, id: u32) -> bool {
        self.find(id).is_some()
    }
    fn names(&self) -> Vec<String> {
        self.all().into_iter().map(|r| r.name).collect()
    }
    fn where_<F: Fn(&Record) -> bool>(&self, pred: F) -> Vec<Record> {
        self.all().into_iter().filter(|r| pred(r)).collect()
    }
}

struct InMemory {
    store: HashMap<u32, Record>,
}
impl ReadRepository for InMemory {
    fn all(&self) -> Vec<Record> {
        let mut v: Vec<Record> = self.store.values().cloned().collect();
        v.sort_by_key(|r| r.id);
        v
    }
    // count, find, exists, names, where_ : all free.
}

fn main() {
    let mut store = HashMap::new();
    store.insert(1, Record { id: 1, name: "alice".into() });
    store.insert(2, Record { id: 2, name: "bob".into() });
    store.insert(3, Record { id: 3, name: "carol".into() });
    let repo = InMemory { store };

    println!("count       = {}", repo.count());
    println!("find(2)     = {:?}", repo.find(2));
    println!("exists(9)   = {}", repo.exists(9));
    println!("names       = {:?}", repo.names());
    let long = repo.where_(|r| r.name.len() > 3);
    println!(
        "names > 3   = {:?}",
        long.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
}
```

Real output:

```text
count       = 3
find(2)     = Some(Record { id: 2, name: "bob" })
exists(9)   = false
names       = ["alice", "bob", "carol"]
names > 3   = ["alice", "carol"]
```

Every new storage backend implements `all` and inherits the rest. In a real service, `find` backed by a naive scan of `all()` would be overridden with an indexed lookup, which is exactly Pitfall 4's "override for a faster path, not different behavior". The shape of the API never changes; only the one expensive method gets specialized per backend.

> **Note:** `where_` takes a generic closure `F: Fn(&Record) -> bool`. A default method may be generic over its own parameters, which is part of why one default can serve so many call sites. See [Trait Bounds](/09-generics-traits/05-trait-bounds/) and [Generic Functions](/09-generics-traits/00-generic-functions/).

---

## Further Reading

### Official Documentation

- [The Rust Book - Default Implementations](https://doc.rust-lang.org/book/ch10-02-traits.html#default-implementations)
- [The Rust Book - Traits: Defining Shared Behavior](https://doc.rust-lang.org/book/ch10-02-traits.html)
- [Rust by Example - Traits](https://doc.rust-lang.org/rust-by-example/trait.html)
- [Rust Reference - Provided methods](https://doc.rust-lang.org/reference/items/traits.html#provided-methods)
- [`std::iter::Iterator`](https://doc.rust-lang.org/std/iter/trait.Iterator.html) — the canonical "one required method, dozens of defaults" trait

### Related Sections in This Guide

- [Trait Methods](/09-generics-traits/04-trait-methods/) — required vs provided methods, calling them, overriding, and the no-`super` rule (read first)
- [Traits](/09-generics-traits/03-traits/) — defining and implementing a trait; `impl Trait for Type`
- [Trait Bounds](/09-generics-traits/05-trait-bounds/) — `<T: Trait>` and the bounds that power blanket `impl`s
- [Generic Functions](/09-generics-traits/00-generic-functions/) — monomorphization vs TypeScript type erasure
- [Supertraits](/09-generics-traits/09-supertraits/) — the `: Iterator<...>` supertrait bound used by the extension trait
- [Trait Objects](/09-generics-traits/06-trait-objects/) — dynamic dispatch with `&dyn Trait` / `Box<dyn Trait>` when you opt out of monomorphization
- [The Orphan Rule](/09-generics-traits/12-orphan-rule/) — what you may and may not implement defaults *for*
- [Operator Overloading](/09-generics-traits/10-operator-overloading/) — traits like `Add` that you implement to enable operators
- [Getting Started](/01-getting-started/) and [Basics](/02-basics/) — toolchain and syntax foundations
- [Smart Pointers](/10-smart-pointers/) — `Box<dyn Trait>` for owning trait objects built from these traits

---

## Exercises

### Exercise 1: One required method, defaults plus an override

**Difficulty:** Easy

**Objective:** Build a trait whose defaults compose, and override one of them.

**Instructions:** Define a `Notifier` trait. `recipient(&self) -> String` is required. `channel(&self) -> &str` is provided and defaults to `"email"`. `notify(&self, msg) -> String` is provided and returns `"[<channel>] -> <recipient>: <msg>"`. Implement `EmailUser` (keeps the default channel) and `SmsUser` (overrides `channel` to `"sms"`).

```rust
trait Notifier {
    fn recipient(&self) -> String;   // required
    fn channel(&self) -> &str {      // provided
        /* ??? */
    }
    fn notify(&self, msg: &str) -> String { // provided
        // TODO: "[<channel>] -> <recipient>: <msg>"
        /* ??? */
    }
}

// TODO: struct EmailUser; struct SmsUser; + impls

fn main() {
    // expected:
    // [email] -> me@zeybek.dev: deploy finished
    // [sms] -> +1-555-0100: 2FA code: 4821
}
```

<details>
<summary>Solution</summary>

```rust
trait Notifier {
    fn recipient(&self) -> String;
    fn channel(&self) -> &str {
        "email"
    }
    fn notify(&self, msg: &str) -> String {
        format!("[{}] -> {}: {}", self.channel(), self.recipient(), msg)
    }
}

struct EmailUser {
    addr: String,
}
impl Notifier for EmailUser {
    fn recipient(&self) -> String {
        self.addr.clone()
    }
}

struct SmsUser {
    phone: String,
}
impl Notifier for SmsUser {
    fn recipient(&self) -> String {
        self.phone.clone()
    }
    fn channel(&self) -> &str {
        "sms"
    }
}

fn main() {
    let e = EmailUser { addr: "me@zeybek.dev".into() };
    let s = SmsUser { phone: "+1-555-0100".into() };
    println!("{}", e.notify("deploy finished"));
    println!("{}", s.notify("2FA code: 4821"));
}
```

Output:

```text
[email] -> me@zeybek.dev: deploy finished
[sms] -> +1-555-0100: 2FA code: 4821
```

`notify` is written once. Because it calls `self.channel()`, overriding `channel` in `SmsUser` automatically changes what `notify` produces; `notify` itself is never touched.

</details>

### Exercise 2: Defaults derived from one required getter

**Difficulty:** Medium

**Objective:** Practice the required-getter-plus-provided-orchestration pattern that sidesteps Pitfall 1.

**Instructions:** Define a `Shape` trait. `area(&self) -> f64` is required. `name(&self) -> &str` is provided and defaults to `"shape"`. `describe(&self) -> String` is provided and returns `"<name> with area <area, 2 decimals>"`. Implement `Circle { r: f64 }` (overrides `name` to `"circle"`) and `UnitSquare` (keeps the default name). Note that neither default reads a field directly; they go through `area()` and `name()`.

```rust
trait Shape {
    fn area(&self) -> f64;          // required
    fn name(&self) -> &str {        // provided
        /* ??? */
    }
    fn describe(&self) -> String {  // provided
        // TODO: "<name> with area <area:.2>"
        /* ??? */
    }
}

// TODO: struct Circle { r: f64 }; struct UnitSquare; + impls

fn main() {
    // expected:
    // circle with area 12.57
    // shape with area 1.00
}
```

<details>
<summary>Solution</summary>

```rust
trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> &str {
        "shape"
    }
    fn describe(&self) -> String {
        format!("{} with area {:.2}", self.name(), self.area())
    }
}

struct Circle {
    r: f64,
}
impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.r * self.r
    }
    fn name(&self) -> &str {
        "circle"
    }
}

struct UnitSquare;
impl Shape for UnitSquare {
    fn area(&self) -> f64 {
        1.0
    }
    // keeps default name() == "shape" and default describe()
}

fn main() {
    println!("{}", Circle { r: 2.0 }.describe());
    println!("{}", UnitSquare.describe());
}
```

Output:

```text
circle with area 12.57
shape with area 1.00
```

`describe` never touches `self.r`; it calls `self.area()`. That is what keeps the default valid for `UnitSquare`, which has no radius at all.

</details>

### Exercise 3: An extension trait via a blanket impl

**Difficulty:** Medium/Hard

**Objective:** Distribute a default API to many types at once with an empty blanket `impl`: the boilerplate-elimination move with no TypeScript equivalent.

**Instructions:** Define a `Loggable` trait. `label(&self) -> String` is required. `log_line(&self) -> String` is provided and returns `"LOG: <label>"`. `log_with_level(&self, level) -> String` is provided and returns `"[<LEVEL uppercased>] <label>"`. Implement it for `Order { id, total }` (keeps the default `log_line`) and `Event { kind }` (overrides `log_line` to start with `"LOG* "`).

```rust
trait Loggable {
    fn label(&self) -> String;             // required
    fn log_line(&self) -> String {         // provided
        /* ??? */
    }
    fn log_with_level(&self, level: &str) -> String { // provided
        /* ??? */
    }
}

// TODO: struct Order { id: u32, total: u32 }; struct Event { kind: String }; + impls

fn main() {
    // expected:
    // LOG: order #7 ($42)
    // [WARN] order #7 ($42)
    // LOG* event: login
    // [INFO] event: login
}
```

<details>
<summary>Solution</summary>

```rust
trait Loggable {
    // Required: the one-line label for this value.
    fn label(&self) -> String;

    // Provided defaults that compose, giving a free logging API.
    fn log_line(&self) -> String {
        format!("LOG: {}", self.label())
    }
    fn log_with_level(&self, level: &str) -> String {
        format!("[{}] {}", level.to_uppercase(), self.label())
    }
}

struct Order {
    id: u32,
    total: u32,
}
impl Loggable for Order {
    fn label(&self) -> String {
        format!("order #{} (${})", self.id, self.total)
    }
}

struct Event {
    kind: String,
}
impl Loggable for Event {
    fn label(&self) -> String {
        format!("event: {}", self.kind)
    }
    // override the default to add a marker
    fn log_line(&self) -> String {
        format!("LOG* {}", self.label())
    }
}

fn main() {
    let o = Order { id: 7, total: 42 };
    let e = Event { kind: "login".into() };
    println!("{}", o.log_line());
    println!("{}", o.log_with_level("warn"));
    println!("{}", e.log_line());
    println!("{}", e.log_with_level("info"));
}
```

Output:

```text
LOG: order #7 ($42)
[WARN] order #7 ($42)
LOG* event: login
[INFO] event: login
```

`Order` writes only `label` and inherits both logging methods; `Event` overrides one. For the *true* extension-trait move — handing these defaults to every type meeting a bound via `impl<T: SomeBound> Loggable for T {}` with an empty body — see the `NumericExt` example in the Detailed Explanation. Stretch goal: rewrite this so `Loggable` is blanket-implemented for every type that already implements `std::fmt::Display`, using `self.to_string()` as the default `label`.

</details>
