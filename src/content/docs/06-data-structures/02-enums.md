---
title: "Enums and Data-Carrying Variants"
description: "Rust enums model \"one of several shapes\" like a TypeScript discriminated union, but the variant is the tag and the compiler forces you to handle every case."
---

Rust **enums** are the tool you reach for when a value is "one of several shapes." If you have ever written a TypeScript **union type** or a **discriminated union**, you already understand the *idea*. But Rust's enums are first-class, exhaustively-checked, and can carry data per variant with zero runtime tagging overhead.

---

## Quick Overview

A Rust `enum` defines a type that can be exactly **one** of a fixed set of **variants**, and each variant can carry its own data. This is Rust's answer to the TypeScript union type `A | B | C`, and especially to the **discriminated union** pattern. The big payoff for a TypeScript/JavaScript developer: the compiler *forces* you to handle every variant (no forgotten `case`), and there is no separate `kind` string to keep in sync by hand. The variant *is* the tag.

**In short:** In TypeScript a discriminated union is a convention you assemble out of object types plus a literal `kind` field. In Rust an enum is a built-in language construct, so the tag and the payload are one inseparable thing.

---

## TypeScript/JavaScript Example

```typescript
// The classic discriminated union: a `kind` field is the discriminant,
// and each member carries its own payload.
type WebEvent =
  | { kind: "pageLoad" }
  | { kind: "pageUnload" }
  | { kind: "keyPress"; key: string }
  | { kind: "paste"; text: string }
  | { kind: "click"; x: number; y: number };

function describe(event: WebEvent): string {
  switch (event.kind) {
    case "pageLoad":
      return "page loaded";
    case "pageUnload":
      return "page unloaded";
    case "keyPress":
      return `pressed '${event.key}'`;
    case "paste":
      return `pasted "${event.text}"`;
    case "click":
      return `clicked at (${event.x}, ${event.y})`;
    // Forget a case and TypeScript only complains if you opted into
    // exhaustiveness (e.g. a `never` default). It is not automatic.
  }
}

const event: WebEvent = { kind: "click", x: 20, y: 80 };
console.log(describe(event)); // clicked at (20, 80)
```

**Key points:**

- The union is built by hand: object types glued together with `|`.
- The `kind` string literal is the discriminant; *you* invent and maintain it.
- Narrowing happens via `switch (event.kind)`; inside each `case`, TypeScript narrows `event` to the matching member.
- Exhaustiveness is **opt-in** (you add a `never` trick). Miss a case and, by default, you just get `undefined` at runtime.

---

## Rust Equivalent

```rust
// One language construct. Each variant is a shape; some carry data.
#[derive(Debug)]
enum WebEvent {
    PageLoad,                  // unit-like: no data
    PageUnload,                // unit-like: no data
    KeyPress(char),            // tuple-like: one positional field
    Paste(String),             // tuple-like: one positional field
    Click { x: i64, y: i64 },  // struct-like: named fields
}

fn describe(event: &WebEvent) -> String {
    match event {
        WebEvent::PageLoad => "page loaded".to_string(),
        WebEvent::PageUnload => "page unloaded".to_string(),
        WebEvent::KeyPress(c) => format!("pressed '{c}'"),
        WebEvent::Paste(s) => format!("pasted \"{s}\""),
        WebEvent::Click { x, y } => format!("clicked at ({x}, {y})"),
    }
}

fn main() {
    let event = WebEvent::Click { x: 20, y: 80 };
    println!("{}", describe(&event)); // clicked at (20, 80)
}
```

Running it prints:

```text
clicked at (20, 80)
```

**Key points:**

- `enum WebEvent { ... }` declares the whole closed set of shapes in one place.
- Variants come in three flavors: **unit-like** (`PageLoad`), **tuple-like** (`KeyPress(char)`), and **struct-like** (`Click { x, y }`).
- The variant name *is* the discriminant — there is no separate `kind` string to maintain.
- `match` narrows and destructures in one step, and (as we'll see) the compiler checks exhaustiveness for free.

---

## Detailed Explanation

### The three variant shapes, line by line

```rust
enum WebEvent {
    PageLoad,                  // (1) unit-like variant
    KeyPress(char),            // (2) tuple-like variant, one field
    Paste(String),             // (3) tuple-like variant, owns a String
    Click { x: i64, y: i64 },  // (4) struct-like variant, named fields
}
```

1. **Unit-like** variants carry no data. They are the closest thing to a plain TypeScript string-literal member like `{ kind: "pageLoad" }`, except there is no object at all, just a tag.
2. **Tuple-like** variants hold positional data, like a [tuple struct](/06-data-structures/01-tuple-structs/). `KeyPress(char)` holds exactly one `char`.
3. `Paste(String)` *owns* its `String`. This matters: constructing `WebEvent::Paste(s)` moves `s` into the enum (see [Section 05 — Ownership](/05-ownership/)).
4. **Struct-like** variants hold named fields, mirroring a discriminated-union member with several properties (`{ kind: "click"; x; y }`).

You can mix all three flavors freely in one enum, which has no clean TypeScript equivalent: there, every union member is an object type.

### Constructing a value

```rust
let a = WebEvent::PageLoad;                       // unit-like
let b = WebEvent::KeyPress('x');                  // tuple-like
let c = WebEvent::Paste(String::from("hello"));   // tuple-like
let d = WebEvent::Click { x: 20, y: 80 };         // struct-like
```

Every value is namespaced under the enum type: `WebEvent::Click`, not bare `Click`. Compare this to TypeScript, where you just write an object literal `{ kind: "click", x: 20, y: 80 }` and rely on structural typing. Rust is **nominal**: the value's type is `WebEvent`, full stop.

### Reading the data back: `match`

You cannot read a variant's payload directly (more on that in [Common Pitfalls](#common-pitfalls)). You **destructure** it, almost always with `match`:

```rust
match event {
    WebEvent::KeyPress(c) => format!("pressed '{c}'"),       // bind the char as `c`
    WebEvent::Click { x, y } => format!("at ({x}, {y})"),    // bind both fields
    // ...
}
```

This is the same instinct as `switch (event.kind)` followed by `event.key`, but fused into one operation: matching the variant **and** pulling out its fields happen together, and the binding (`c`, `x`, `y`) only exists inside that arm where the type is known. Pattern matching gets its own page in [Pattern Matching](/06-data-structures/04-pattern-matching/).

### Memory: a tag plus the largest payload

A TypeScript discriminated-union value is a heap-allocated object with a `kind` property string plus its other fields. A Rust enum value is laid out inline as a small integer **discriminant** (the tag) followed by enough space for the *largest* variant's payload: no heap allocation, no string tag. The whole `WebEvent` lives on the stack. This is why Rust enums are cheap enough to use everywhere, including in hot loops.

> **Note:** The compiler is clever about layout. For example `Option<&T>` (an enum!) needs no extra tag byte at all, because a reference can never be null — `None` reuses the all-zero bit pattern. This "niche optimization" means many Rust enums are as compact as the raw data they wrap.

### `Option` and `Result` are just enums

Two of the most important types in Rust are nothing more than enums defined in the standard library:

```rust
let found: Option<i32> = Some(7);
let missing: Option<i32> = None;
println!("{found:?} {missing:?}"); // Some(7) None
```

`Option<T>` is `enum Option<T> { Some(T), None }` and `Result<T, E>` is `enum Result<T, E> { Ok(T), Err(E) }`. Everything you learn here applies to them. `Option<T>` is Rust's replacement for `null`/`undefined` and gets its own page: [The Option Type](/06-data-structures/03-option-enum/). `Result` powers error handling in [Section 08](/08-error-handling/).

---

## Key Differences

| Concept | TypeScript discriminated union | Rust enum |
| --- | --- | --- |
| Definition | Hand-assembled: `A \| B \| C` of object types | One `enum` declaration |
| Discriminant | A literal field you invent (`kind: "click"`) | The variant name, built into the language |
| Payload | Each member is an object with properties | Per-variant: unit, tuple, or named fields |
| Runtime representation | Heap object with a string tag + fields | Inline tag (small int) + largest payload, often stack-allocated |
| Narrowing | `switch`/`if` on the tag, then read fields | `match` / `if let` destructures variant + fields together |
| Exhaustiveness | Opt-in (`never` trick); otherwise silent | **Mandatory**: compiler error if a variant is missed |
| Adding a variant | Misses are silent until runtime | Compiler points at every `match` you must update |
| Type identity | Structural (shape-compatible counts) | Nominal (the named type is the type) |
| Generics | Erased at runtime | Monomorphized (real specialized code) |

### Exhaustiveness is the headline feature

In TypeScript, adding a new union member compiles fine and your existing `switch` statements silently fall through. In Rust, adding a variant turns every `match` that doesn't handle it into a **compile error**, with the compiler naming the exact uncovered case. This turns "did I update every place?" from a manual audit into something the type checker guarantees.

### Enums can have methods

Like structs, enums can have `impl` blocks. The `matches!` macro is handy for a quick boolean check:

```rust
#[derive(Debug, Clone)]
enum JobState {
    Queued,
    Running { progress: u8 },
    Succeeded { output: String },
    Failed { code: u32, message: String },
}

impl JobState {
    fn is_terminal(&self) -> bool {
        matches!(self, JobState::Succeeded { .. } | JobState::Failed { .. })
    }
}
```

Methods on enums are covered alongside structs in [Methods and `impl` Blocks](/06-data-structures/05-impl-blocks/); here we just note that data and behavior live together.

### C-like enums with explicit discriminants

When variants carry *no* data, an enum behaves like a classic C enum, and you can pin each tag to an integer and cast to it:

```rust
#[derive(Debug, Clone, Copy)]
enum HttpStatus {
    Ok = 200,
    NotFound = 404,
    ServerError = 500,
}

fn main() {
    let all = [HttpStatus::Ok, HttpStatus::NotFound, HttpStatus::ServerError];
    for s in all {
        println!("{s:?} = {}", s as i32);
    }
}
```

This prints:

```text
Ok = 200
NotFound = 404
ServerError = 500
```

> **Note:** Casting an enum to an integer with `as` only works for these data-less, "field-less" enums. Once a variant carries a payload, there is no single integer to cast to. (And `as` only goes one way: there is no built-in `404 as HttpStatus`. Reverse conversion needs a `match` or the [`num_enum`](/23-ecosystem/) crate.)

---

## Common Pitfalls

### Pitfall 1: Forgetting a variant in `match`

A TypeScript `switch` will happily skip a missing case. Rust refuses to compile:

```rust
enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Triangle(f64, f64),
}

fn area(s: &Shape) -> f64 {
    match s { // does not compile (error[E0004]: non-exhaustive patterns)
        Shape::Circle(r) => 3.14159 * r * r,
        Shape::Rectangle(w, h) => w * h,
        // Triangle is missing!
    }
}
```

The real compiler error:

```text
error[E0004]: non-exhaustive patterns: `&Shape::Triangle(_, _)` not covered
 --> src/main.rs:8:11
  |
8 |     match s {
  |           ^ pattern `&Shape::Triangle(_, _)` not covered
  |
note: `Shape` defined here
 --> src/main.rs:1:6
  |
1 | enum Shape {
  |      ^^^^^
...
4 |     Triangle(f64, f64),
  |     -------- not covered
  = note: the matched value is of type `&Shape`
help: ensure that all possible cases are being handled by adding a match arm with a
      wildcard pattern or an explicit pattern as shown
```

**Fix:** add the missing arm. Reach for a catch-all `_ => ...` only when you genuinely want a default; if you use `_` everywhere, you lose the very compiler nudge that makes enums safe when you add variants later.

### Pitfall 2: Trying to read a payload like a TypeScript property

In TypeScript, after narrowing you read `event.key`. In Rust there is no field to read until you destructure; an enum is not a struct:

```rust
enum Msg {
    Text(String),
    Quit,
}

fn main() {
    let m = Msg::Text(String::from("hi"));
    println!("{}", m.0); // does not compile (error[E0609]: no field `0` on type `Msg`)
}
```

The real error:

```text
error[E0609]: no field `0` on type `Msg`
 --> src/main.rs:8:22
  |
8 |     println!("{}", m.0);
  |                      ^ unknown field
```

**Fix:** match or use `if let` to bind the inner value:

```rust
if let Msg::Text(s) = &m {
    println!("{s}");
}
```

### Pitfall 3: Comparing enum values with `==` before deriving `PartialEq`

In JavaScript `a === b` always works. In Rust, `==` is the `PartialEq` trait, and an enum doesn't get it automatically:

```rust
enum Direction {
    North,
    South,
}

fn main() {
    let d = Direction::North;
    if d == Direction::North { // does not compile (error[E0369])
        println!("going north");
    }
}
```

The real error (note the actionable hint):

```text
error[E0369]: binary operation `==` cannot be applied to type `Direction`
 --> src/main.rs:8:10
  |
8 |     if d == Direction::North {
  |        - ^^ ---------------- Direction
  |        |
  |        Direction
  |
note: an implementation of `PartialEq` might be missing for `Direction`
 --> src/main.rs:1:1
  |
1 | enum Direction {
  | ^^^^^^^^^^^^^^ must implement `PartialEq`
help: consider annotating `Direction` with `#[derive(PartialEq)]`
```

**Fix:** add `#[derive(PartialEq)]` (and usually `Debug`) above the enum. Deriving traits is covered for structs in [Structs](/06-data-structures/00-structs/) and in full in [Section 09](/09-generics-traits/).

### Pitfall 4: Expecting structural typing

Two TypeScript unions with the same members are interchangeable. In Rust two enums with identical-looking variants are **different, incompatible types** — Rust is nominal. If function `f` wants a `WebEvent`, no other enum will do, even one defined with the same variants.

---

## Best Practices

- **Reach for an enum whenever you'd write a TypeScript union of object types.** It is the idiomatic, type-safe way to model "one of N shapes."
- **Prefer named fields for variants with 2+ pieces of data.** `Click { x: i64, y: i64 }` documents itself; `Click(i64, i64)` makes the call site guess which number is which.
- **Let `match` enforce exhaustiveness; avoid a blanket `_` arm** unless a default truly is correct. The exhaustiveness error is a feature, not an obstacle.
- **Derive the traits you need** (`#[derive(Debug, Clone, PartialEq)]` is a common starting set). Add `Copy` only for small, data-light enums where copying is trivially cheap.
- **Put behavior on the enum with `impl`.** A `JobState::is_terminal()` method keeps the logic next to the data, the way a class method would in TypeScript.
- **Use `if let` (or `matches!`) when you care about exactly one variant**; use `match` when you must handle them all.
- **Model invalid states out of existence.** If a "loading" request has no response and a "loaded" one always does, encode that in the variants so an impossible combination simply cannot be constructed: something a TypeScript object with optional fields can't guarantee.

---

## Real-World Example

A job/task status is a textbook case for a data-carrying enum: each state carries different information, and several states are *terminal*. In TypeScript you'd model it as a discriminated union; in Rust the enum carries the right payload per state and a method answers questions about it.

```rust
#[derive(Debug, Clone)]
enum JobState {
    Queued,
    Running { progress: u8 },
    Succeeded { output: String },
    Failed { code: u32, message: String },
}

impl JobState {
    /// A job is "terminal" once it can no longer change.
    fn is_terminal(&self) -> bool {
        matches!(self, JobState::Succeeded { .. } | JobState::Failed { .. })
    }

    /// A human-readable one-liner for logs or a UI.
    fn summary(&self) -> String {
        match self {
            JobState::Queued => "waiting to start".to_string(),
            JobState::Running { progress } => format!("running ({progress}%)"),
            JobState::Succeeded { output } => format!("done: {output}"),
            JobState::Failed { code, message } => format!("failed [{code}]: {message}"),
        }
    }
}

fn main() {
    let states = [
        JobState::Queued,
        JobState::Running { progress: 42 },
        JobState::Succeeded { output: "report.pdf".to_string() },
        JobState::Failed { code: 503, message: "upstream timeout".to_string() },
    ];

    for s in &states {
        println!("{:<28} terminal={}", s.summary(), s.is_terminal());
    }
}
```

Output:

```text
waiting to start             terminal=false
running (42%)                terminal=false
done: report.pdf             terminal=true
failed [503]: upstream timeout terminal=true
```

Notice what the type system buys you: a `Failed` value *always* has a `code` and a `message`, and a `Queued` value *cannot* carry a progress number. In the TypeScript equivalent you'd typically end up with optional fields (`progress?: number`) that are technically present in every member, and nothing stops you from constructing a `{ kind: "queued", progress: 99 }`. The Rust enum makes that nonsensical state **unrepresentable**.

---

## Further Reading

- [The Rust Programming Language — Defining an Enum](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html)
- [The Rust Programming Language — The `match` Control Flow Construct](https://doc.rust-lang.org/book/ch06-02-match.html)
- [Rust by Example — Enums](https://doc.rust-lang.org/rust-by-example/custom_types/enum.html)
- [`std::option::Option`](https://doc.rust-lang.org/std/option/enum.Option.html) and [`std::result::Result`](https://doc.rust-lang.org/std/result/enum.Result.html) — the standard library's own enums
- Sibling topics: [Structs](/06-data-structures/00-structs/) · [Tuple Structs and Unit Structs](/06-data-structures/01-tuple-structs/) · [The Option Type](/06-data-structures/03-option-enum/) · [Pattern Matching](/06-data-structures/04-pattern-matching/) · [Methods and `impl` Blocks](/06-data-structures/05-impl-blocks/)
- Background: [Section 02 — Basic Types](/02-basics/01-types/) · [Section 05 — Ownership](/05-ownership/)
- What's next: enums are the natural element type for [Section 07 — Collections](/07-collections/) (e.g. a `Vec<WebEvent>`).

---

## Exercises

### Exercise 1: A `Shape` enum with an `area` method

**Difficulty:** Beginner

**Objective:** Practice declaring struct-like variants and matching on them inside a method.

**Instructions:** Define an enum `Shape` with three variants — `Circle { radius: f64 }`, `Rectangle { width: f64, height: f64 }`, and `Triangle { base: f64, height: f64 }`. Add an `impl Shape` with a method `area(&self) -> f64`. In `main`, put three shapes in an array and print each one's area.

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Triangle { base: f64, height: f64 },
}

impl Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Triangle { base, height } => 0.5 * base * height,
        }
    }
}

fn main() {
    let shapes = [
        Shape::Circle { radius: 2.0 },
        Shape::Rectangle { width: 3.0, height: 4.0 },
        Shape::Triangle { base: 6.0, height: 2.0 },
    ];
    for s in &shapes {
        println!("{:?} -> area {:.2}", s, s.area());
    }
}
```

Output:

```text
Circle { radius: 2.0 } -> area 12.57
Rectangle { width: 3.0, height: 4.0 } -> area 12.00
Triangle { base: 6.0, height: 2.0 } -> area 6.00
```

</details>

### Exercise 2: A recursive `Json` enum

**Difficulty:** Intermediate

**Objective:** Model a value that is "one of several shapes," including ones that contain *more* of itself, the way a TypeScript `JSONValue` union does.

**Instructions:** Define an enum `Json` with variants `Null`, `Bool(bool)`, `Number(f64)`, `Str(String)`, `Array(Vec<Json>)`, and `Object(HashMap<String, Json>)`. Add a method `type_name(&self) -> &'static str` returning `"null"`, `"boolean"`, etc. In `main`, build a small object and print the type name of the whole document and of one of its fields.

> **Tip:** Variants like `Array` and `Object` hold a `Vec`/`HashMap` *of `Json`*, which is fine because those collections heap-allocate their contents. (A directly self-containing variant like `Node(Json)` would need a `Box`; see the hint in the next exercise.)

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

#[derive(Debug)]
#[allow(dead_code)] // payloads are intentionally only matched with `_` in `type_name`
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<Json>),
    Object(HashMap<String, Json>),
}

impl Json {
    fn type_name(&self) -> &'static str {
        match self {
            Json::Null => "null",
            Json::Bool(_) => "boolean",
            Json::Number(_) => "number",
            Json::Str(_) => "string",
            Json::Array(_) => "array",
            Json::Object(_) => "object",
        }
    }
}

fn main() {
    let mut obj = HashMap::new();
    obj.insert("name".to_string(), Json::Str("Ada".to_string()));
    obj.insert("age".to_string(), Json::Number(36.0));
    obj.insert("admin".to_string(), Json::Bool(true));
    obj.insert(
        "tags".to_string(),
        Json::Array(vec![Json::Str("a".into()), Json::Null]),
    );

    let doc = Json::Object(obj);
    println!("top-level type: {}", doc.type_name());

    if let Json::Object(map) = &doc {
        if let Some(name) = map.get("name") {
            println!("name field is a {}", name.type_name());
        }
    }
}
```

Output:

```text
top-level type: object
name field is a string
```

> **Note:** Because `type_name` only inspects each variant's *discriminant* (matching the payloads with `_`), the compiler considers the inner data unread and would emit `dead_code` warnings. The `#[allow(dead_code)]` above silences them; in a real codebase you'd read those payloads (e.g. to serialize the value) and the warnings would disappear on their own.

</details>

### Exercise 3: A `Light` state machine

**Difficulty:** Advanced

**Objective:** Combine a data-less enum, derived traits, `match`-based state transitions, and the `matches!` macro.

**Instructions:** Define `enum Light { Red, Yellow, Green }` deriving `Debug, Clone, Copy, PartialEq`. Add a method `next(self) -> Light` that advances the cycle Red → Green → Yellow → Red, and a method `may_go(self) -> bool` that returns `true` only for `Green` (use `matches!`). In `main`, start at `Red` and print four steps of the cycle with whether you may go.

> **Tip:** A recursive variant that contains the enum *directly* — such as a `Node(Tree, Tree)` — would have infinite size, so it must wrap the inner value in a `Box` (a heap pointer): `Node(Box<Tree>, Box<Tree>)`. You won't need that here, but it's the standard fix and you'll meet `Box` in [Section 10](/10-smart-pointers/).

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum Light {
    Red,
    Yellow,
    Green,
}

impl Light {
    fn next(self) -> Light {
        match self {
            Light::Red => Light::Green,
            Light::Green => Light::Yellow,
            Light::Yellow => Light::Red,
        }
    }

    fn may_go(self) -> bool {
        matches!(self, Light::Green)
    }
}

fn main() {
    let mut light = Light::Red;
    for _ in 0..4 {
        println!("{light:?} (go? {})", light.may_go());
        light = light.next();
    }
}
```

Output:

```text
Red (go? false)
Green (go? true)
Yellow (go? false)
Red (go? false)
```

</details>
