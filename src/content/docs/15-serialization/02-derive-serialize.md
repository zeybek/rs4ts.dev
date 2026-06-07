---
title: "Deriving `Serialize` and `Deserialize`"
description: "JSON.stringify serializes any object via runtime reflection; Rust has none, so #[derive(Serialize, Deserialize)] generates type-checked, compile-time impls instead."
---

In TypeScript, any object is serializable to JSON for free: `JSON.stringify` walks its enumerable properties at runtime. Rust has no runtime reflection, so a type can only be serialized if it explicitly implements the **`Serialize`** trait (and deserialized only if it implements **`Deserialize`**). The `#[derive(Serialize, Deserialize)]` attribute makes the Serde derive macro write those implementations for you at compile time, so your `struct`s and `enum`s round-trip to JSON, TOML, YAML, and dozens of other formats with one line.

---

## Quick Overview

- **What it is:** A derive macro from the [`serde`](https://serde.rs) crate that generates `Serialize` and/or `Deserialize` trait implementations for your `struct`s and `enum`s.
- **Why it matters:** It is the Rust equivalent of "this object is JSON-ready." Without it, `serde_json::to_string(&value)` will not even compile, because the compiler cannot find a `Serialize` impl for your type.
- **The mental shift:** Unlike `JSON.stringify`, which works on *any* value at runtime, Rust decides *at compile time* exactly which types can be serialized. The derive macro is how you opt a type in.

> **Note:** This page focuses on the derive macro itself: where you put it, what it generates, and how it behaves on every kind of `struct` and `enum`. Setting up the dependency is covered in [Serde Basics](/15-serialization/01-serde-basics/), the trait architecture in [Serde](/15-serialization/00-serde-intro/), and the many `#[serde(...)]` field attributes in [Serde Attributes](/15-serialization/05-attributes/).

---

## TypeScript/JavaScript Example

In TypeScript you never *declare* a type serializable. Every plain object is fair game for `JSON.stringify`, and `JSON.parse` hands you back an untyped `any` that you assert into a shape:

```typescript
// TypeScript - serialization is implicit and works on any object
interface User {
  id: number;
  username: string;
  email: string;
  isActive: boolean;
}

const user: User = {
  id: 42,
  username: "ada",
  email: "ada@example.com",
  isActive: true,
};

// Object -> JSON string. No declaration needed; structural and runtime-driven.
const json = JSON.stringify(user);
console.log(json);
// {"id":42,"username":"ada","email":"ada@example.com","isActive":true}

// JSON string -> object. The cast is a lie the compiler trusts blindly.
const parsed = JSON.parse(json) as User;
console.log(parsed.username); // "ada"
```

There are two hidden costs that Rust will make explicit:

```typescript
// 1. Methods and prototype are silently dropped — only data survives.
class Account {
  constructor(public id: number, public name: string) {}
  greet() {
    return `Hi ${this.name}`;
  }
}

const acct = new Account(7, "Grace");
const restored = JSON.parse(JSON.stringify(acct));
console.log(restored instanceof Account); // false
console.log(typeof restored.greet); // "undefined"

// 2. `as User` is unchecked. If the JSON is missing `email`, you get
//    `undefined` at runtime with zero warning at the boundary.
```

---

## Rust Equivalent

In Rust you annotate the type once. The derive macro generates the trait implementations the serializer needs:

```rust
// Cargo.toml:
//   [dependencies]
//   serde = { version = "1", features = ["derive"] }
//   serde_json = "1"
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    id: u64,
    username: String,
    email: String,
    is_active: bool,
}

fn main() {
    let user = User {
        id: 42,
        username: String::from("ada"),
        email: String::from("ada@example.com"),
        is_active: true,
    };

    // Struct -> JSON string (serialize).
    let json = serde_json::to_string(&user).unwrap();
    println!("{json}");

    // Pretty-printed JSON.
    let pretty = serde_json::to_string_pretty(&user).unwrap();
    println!("{pretty}");

    // JSON string -> struct (deserialize). The type annotation tells Serde
    // exactly which shape to build, and it is *checked*.
    let parsed: User = serde_json::from_str(&json).unwrap();
    println!("{parsed:?}");
}
```

Real output:

```text
{"id":42,"username":"ada","email":"ada@example.com","is_active":true}
{
  "id": 42,
  "username": "ada",
  "email": "ada@example.com",
  "is_active": true
}
User { id: 42, username: "ada", email: "ada@example.com", is_active: true }
```

The single `#[derive(Serialize, Deserialize)]` line replaces both `JSON.stringify` *capability* and the unchecked `as User` cast. Unlike the cast, deserialization actually validates that every required field is present and correctly typed.

---

## Detailed Explanation

### What `derive` is, and why it is needed

`Serialize` and `Deserialize` are **traits** (Rust's version of interfaces — see [section 09](/09-generics-traits/)). A function like `serde_json::to_string` is generic over `T: Serialize`, so you can only pass it a value whose type implements `Serialize`. Rust has no runtime reflection, so there is no way to "just walk the fields" the way `JSON.stringify` does. Someone has to write the field-by-field code, and `#[derive(...)]` is that someone.

`#[derive(Trait)]` runs a **procedural macro** at compile time that reads your type's definition and emits an `impl Trait for YourType { ... }` block. (Macros are covered in depth in [section 14](/14-macros/).) It is *not* a runtime annotation or decorator; by the time your program runs, the generated code is indistinguishable from code you typed by hand.

### What the derive actually generates

For a struct, the generated `Serialize` impl visits each field in declaration order. You rarely look at it, but it helps to see that it is plain, ordinary Rust. Here is a hand-written `Serialize` that is effectively what the derive produces:

```rust
use serde::ser::{Serialize, SerializeStruct, Serializer};

struct User {
    id: u64,
    username: String,
    is_active: bool,
}

// This is roughly what `#[derive(Serialize)]` writes for you.
impl Serialize for User {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Announce "I am a struct named User with 3 fields".
        let mut state = serializer.serialize_struct("User", 3)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("username", &self.username)?;
        state.serialize_field("is_active", &self.is_active)?;
        state.end()
    }
}

fn main() {
    let user = User { id: 42, username: "ada".into(), is_active: true };
    println!("{}", serde_json::to_string(&user).unwrap());
    // {"id":42,"username":"ada","is_active":true}
}
```

Real output:

```text
{"id":42,"username":"ada","is_active":true}
```

Two things to notice:

- The impl is **format-agnostic**. It talks to an abstract `Serializer` (`serialize_struct`, `serialize_field`), not to JSON specifically. That is why the *same* derived type can also produce TOML or MessagePack — see [Beyond JSON](/15-serialization/06-other-formats/).
- It is **recursive by composition**. `serialize_field("id", &self.id)` only works because `u64` itself implements `Serialize`. Serde ships impls for all the standard types (`String`, `Vec<T>`, `Option<T>`, `HashMap<K, V>`, etc.), and your derived impl reuses them.

`Deserialize` is generated the same way but is considerably more code: it builds a "visitor" state machine that can accept fields in any order, report missing fields, and ignore unknown ones. It is verbose enough that you almost never want to write it by hand; that is the whole point of the derive. ([Custom Serialization](/15-serialization/07-custom-serialization/) covers the rare cases where you do.)

### `Serialize` and `Deserialize` are independent

They are two separate traits, and you derive only what you need:

- `#[derive(Serialize)]`: the type can be turned *into* a format (write-only). Common for response/output types.
- `#[derive(Deserialize)]`: the type can be built *from* a format (read-only). Common for request/config types.
- `#[derive(Serialize, Deserialize)]` — both directions, for types that round-trip.

This is finer-grained than TypeScript, where every shape is implicitly both directions.

### Every field's type must also implement the trait

This is the rule that trips up everyone coming from TypeScript. Because the generated code calls `serialize_field("address", &self.address)`, the type of `address` must *itself* implement `Serialize`. There is no "deep stringify everything" fallback. If a nested type lacks the derive, your code will not compile (see Common Pitfalls).

### It works on enums too

Enums are where Serde shines compared to TypeScript's hand-rolled discriminated unions. Each variant kind serializes to a distinct, predictable JSON shape:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
enum Event {
    // Unit variant: no data.
    Logout,
    // Newtype variant: one unnamed field.
    PageView(String),
    // Tuple variant: multiple unnamed fields.
    Click(u32, u32),
    // Struct variant: named fields.
    Purchase { item_id: u64, quantity: u32 },
}

fn main() {
    let events = vec![
        Event::Logout,
        Event::PageView(String::from("/home")),
        Event::Click(120, 45),
        Event::Purchase { item_id: 7, quantity: 2 },
    ];

    for event in &events {
        println!("{}", serde_json::to_string(event).unwrap());
    }
}
```

Real output:

```text
"Logout"
{"PageView":"/home"}
{"Click":[120,45]}
{"Purchase":{"item_id":7,"quantity":2}}
```

This is the **externally tagged** representation, the default. The variant name becomes a JSON key (or, for a unit variant, a bare string). In TypeScript you would model this as a tagged union (`{ type: "purchase"; itemId: number }`) and check `e.type` by hand; Serde derives both the encode *and* the validated decode for you. You can switch to internally tagged, adjacently tagged, or untagged representations with `#[serde(tag = "...")]` and friends; those are covered in [Serde Attributes](/15-serialization/05-attributes/) and [Structs and JSON](/15-serialization/03-json/).

### Struct kinds and their JSON shapes

The derive handles every shape of struct Rust allows, and each maps to a sensible JSON value:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Meters(f64); // newtype struct (one field)

#[derive(Debug, Serialize, Deserialize)]
struct Point(i32, i32); // tuple struct

#[derive(Debug, Serialize, Deserialize)]
struct Marker; // unit struct (no fields)

#[derive(Debug, Serialize, Deserialize)]
struct Wrapper<T> {
    value: T,
    label: String,
}

fn main() {
    println!("{}", serde_json::to_string(&Meters(3.5)).unwrap());
    println!("{}", serde_json::to_string(&Point(1, 2)).unwrap());
    println!("{}", serde_json::to_string(&Marker).unwrap());

    let w = Wrapper { value: vec![1, 2, 3], label: "nums".into() };
    println!("{}", serde_json::to_string(&w).unwrap());
}
```

Real output:

```text
3.5
[1,2]
null
{"value":[1,2,3],"label":"nums"}
```

- A **newtype struct** is transparent: it serializes as its single inner value (`3.5`, not `{"0":3.5}`).
- A **tuple struct** serializes as a JSON array.
- A **unit struct** serializes as `null`.
- A **generic struct** works too: the derive automatically adds a `T: Serialize` (or `T: Deserialize`) bound to the generated impl, so `Wrapper<T>` is serializable exactly when its `T` is. This is one place the macro is smarter than a copy-paste impl would be.

---

## Key Differences

| Aspect | TypeScript / JavaScript | Rust + Serde derive |
| --- | --- | --- |
| Who can be serialized | Any value, implicitly | Only types that implement `Serialize` |
| When that is decided | At runtime, by walking properties | At compile time, by the derive macro |
| Direction control | Always both ways | `Serialize` and `Deserialize` are separate, opt-in |
| Nested types | Recursively walked automatically | Each field type must *also* implement the trait |
| Deserialization safety | `JSON.parse(...) as T` is unchecked | Missing/mismatched fields are real, recoverable errors |
| Methods/behavior | Silently dropped (only data survives) | N/A: only the data fields exist; no methods to lose |
| Generics | Erased at runtime | Monomorphized; derive adds bounds like `T: Serialize` |
| Cost | Reflection on every call | Zero runtime reflection; code is generated once |

> **Tip:** The most useful one-sentence summary for a TypeScript developer: *`#[derive(Serialize, Deserialize)]` is how you make a type "JSON-ready," and the compiler enforces that everything it contains is ready too.*

### Why Rust does it this way

Rust has no runtime type information by design — generics are monomorphized away (the opposite of TypeScript's type erasure), and there is no prototype chain to walk. Pushing serialization into compile-time generated code means it is **fast** (no reflection), **type-checked** (you cannot serialize something half-defined), and **format-agnostic** (the same impl drives JSON, YAML, binary formats, and more). The trade-off is the one line of `#[derive(...)]` you must remember to write.

---

## Common Pitfalls

### Pitfall 1: A nested field type forgot the derive

This is the number-one error. You derive `Serialize` on the outer struct but forget it on a type it contains:

```rust
use serde::{Deserialize, Serialize};

struct Address {
    city: String,
} // no derive here

#[derive(Serialize, Deserialize)]
struct Person {
    name: String,
    address: Address, // requires Address: Serialize + Deserialize
}

fn main() {
    let p = Person { name: "Ada".into(), address: Address { city: "London".into() } };
    let _ = serde_json::to_string(&p);
}
```

The real compiler error (`cargo build`):

```text
error[E0277]: the trait bound `Address: serde::Serialize` is not satisfied
    --> src/main.rs:7:10
     |
   7 | #[derive(Serialize, Deserialize)]
     |          ^^^^^^^^^ the trait `Serialize` is not implemented for `Address`
...
  10 |     address: Address,
     |     ------- required by a bound introduced by this call
     |
     = note: for local types consider adding `#[derive(serde::Serialize)]` to your `Address` type
     = note: for types from other crates check whether the crate offers a `serde` feature flag
```

**Fix:** add `#[derive(Serialize, Deserialize)]` to `Address` too. The compiler note spells out both cases: derive it yourself for your own types, or enable the dependency's `serde` feature for third-party types (for example `uuid = { version = "1", features = ["serde"] }`).

### Pitfall 2: Borrowing `&str` fields that outlive the source bytes

It is tempting to deserialize into a struct that *borrows* from the input (`&str` instead of `String`) for speed. That works, but the borrowed data cannot outlive the buffer it points into:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config<'a> {
    name: &'a str,
}

fn parse_config() -> Config<'static> {
    let data = String::from(r#"{"name":"prod"}"#);
    let cfg: Config = serde_json::from_str(&data).unwrap();
    cfg // does not compile (error0515): `data` is dropped at end of fn
}

fn main() {
    println!("{:?}", parse_config());
}
```

The real compiler error:

```text
error[E0515]: cannot return value referencing local variable `data`
  --> src/main.rs:11:5
   |
10 |     let cfg: Config = serde_json::from_str(&data).unwrap();
   |                                            ----- `data` is borrowed here
11 |     cfg
   |     ^^^ returns a value referencing data owned by the current function
```

**Fix:** if you need the data to live independently, use owned fields (`name: String`). Use borrowed fields only when the input buffer clearly outlives the struct. Zero-copy deserialization and `#[serde(borrow)]` are explored in [Serde Performance](/15-serialization/08-performance/).

### Pitfall 3: Importing the wrong `Serialize`/`Deserialize`

The names you derive must be in scope and must be Serde's. A common mistake is to write `#[derive(Serialize)]` without `use serde::Serialize;`, or to confuse the *derive macro* (used in `#[derive(...)]`) with the *trait* (used in `T: Serialize` bounds). With the `derive` feature enabled, a single `use serde::{Deserialize, Serialize};` brings both the traits and the same-named derive macros into scope, which is why that import line appears at the top of nearly every Serde example.

### Pitfall 4: Expecting `Debug` and `Serialize` to be the same thing

`#[derive(Debug)]` gives you `{:?}` formatting for logs; it is *not* serialization. They are separate derives. You will frequently see them together (`#[derive(Debug, Serialize, Deserialize)]`) but each does its own job: `Debug` output is for humans and is not guaranteed stable, while `Serialize` output is a real data format you can parse back.

### Pitfall 5: Assuming unknown JSON fields cause an error

By default, deserialization **ignores** fields in the input that your struct does not declare. This is usually what you want for forward-compatible APIs, but it can hide typos:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Temperature {
    celsius: f64,
}

fn main() {
    // The extra "humidity" key is silently ignored.
    let json = r#"{"celsius":21.5,"humidity":40}"#;
    let parsed: Temperature = serde_json::from_str(json).unwrap();
    println!("{parsed:?}"); // Temperature { celsius: 21.5 }
}
```

Real output:

```text
Temperature { celsius: 21.5 }
```

**Fix:** if you want unknown fields to be a hard error, add `#[serde(deny_unknown_fields)]` to the struct (see [Serde Attributes](/15-serialization/05-attributes/)). Conversely, a *missing required* field is always an error — that asymmetry is what makes deserialization safer than a bare `as T` cast.

---

## Best Practices

- **Put the derive on the data type, once.** Keep `#[derive(Serialize, Deserialize)]` on the plain data struct/enum and let it propagate through composition. Do not scatter manual impls unless you genuinely need custom behavior.
- **Derive only the direction you use.** Output-only DTOs need just `Serialize`; config/request types that you only read need just `Deserialize`. It keeps intent clear and compile times marginally lower.
- **Pair with `Debug`.** `#[derive(Debug, Serialize, Deserialize)]` is the workhorse combination: `Debug` for logging, the Serde pair for the wire.
- **Prefer owned fields (`String`, `Vec<T>`) by default.** Reach for borrowed `&str` / `#[serde(borrow)]` only when profiling shows it matters; the ownership headaches (Pitfall 2) are rarely worth it up front.
- **Keep field names matching the wire format, or use attributes — not a second struct.** If your API uses `camelCase`, add `#[serde(rename_all = "camelCase")]` rather than maintaining a parallel type. See [Serde Attributes](/15-serialization/05-attributes/).
- **Enable the `serde` feature on third-party crates** (e.g. `chrono`, `uuid`, `rust_decimal`) instead of writing wrapper types, so their inner types implement the traits directly.
- **Let the derive add generic bounds for you.** For generic types, write the derive normally; Serde inserts `T: Serialize`/`T: Deserialize` automatically. Override with `#[serde(bound = "...")]` only in advanced cases.

---

## Real-World Example

A typical domain model for an e-commerce order: nested structs, an enum with data-carrying variants, a `Vec`, and a `HashMap`, all serializable from a single derive on each type, and verified to round-trip.

```rust
// Cargo.toml:
//   [dependencies]
//   serde = { version = "1", features = ["derive"] }
//   serde_json = "1"
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct Money {
    amount_cents: u64,
    currency: String,
}

#[derive(Debug, Serialize, Deserialize)]
enum OrderStatus {
    Pending,
    Shipped { tracking_number: String },
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize)]
struct LineItem {
    sku: String,
    quantity: u32,
    unit_price: Money, // nested type — also derives the traits
}

#[derive(Debug, Serialize, Deserialize)]
struct Order {
    order_id: String,
    status: OrderStatus,         // enum field
    items: Vec<LineItem>,        // collection of nested structs
    metadata: HashMap<String, String>,
}

fn main() {
    let mut metadata = HashMap::new();
    metadata.insert("source".to_string(), "web".to_string());

    let order = Order {
        order_id: "ord_1001".into(),
        status: OrderStatus::Shipped { tracking_number: "TRK-9".into() },
        items: vec![LineItem {
            sku: "BOOK-42".into(),
            quantity: 2,
            unit_price: Money { amount_cents: 1599, currency: "USD".into() },
        }],
        metadata,
    };

    // Serialize the whole graph in one call.
    let json = serde_json::to_string_pretty(&order).unwrap();
    println!("{json}");

    // Deserialize it straight back into the typed model.
    let round_tripped: Order = serde_json::from_str(&json).unwrap();
    assert_eq!(round_tripped.order_id, order.order_id);
    println!("round-trip OK");
}
```

Real output:

```text
{
  "order_id": "ord_1001",
  "status": {
    "Shipped": {
      "tracking_number": "TRK-9"
    }
  },
  "items": [
    {
      "sku": "BOOK-42",
      "quantity": 2,
      "unit_price": {
        "amount_cents": 1599,
        "currency": "USD"
      }
    }
  ],
  "metadata": {
    "source": "web"
  }
}
round-trip OK
```

One derive per type and the entire nested graph — struct inside struct inside `Vec`, plus an enum and a map — serializes and deserializes with full type checking. This same `Order` type is exactly what you would return from a web handler; see how it plugs into HTTP responses in [section 16](/16-web-apis/).

---

## Further Reading

- [Serde derive — official documentation](https://serde.rs/derive.html): the canonical reference for what the derive generates.
- [Serde data model](https://serde.rs/data-model.html) — the 29 types that sit between your structs and every format.
- [Enum representations](https://serde.rs/enum-representations.html) — externally/internally/adjacently tagged and untagged.
- [`serde_json` crate docs](https://docs.rs/serde_json/) — `to_string`, `from_str`, and friends.
- This guide:
  - [Serde](/15-serialization/00-serde-intro/) — the `Serialize`/`Deserialize` traits and the data-model architecture.
  - [Serde Basics](/15-serialization/01-serde-basics/) — adding the dependency with `features = ["derive"]` and the basic API.
  - [Structs and JSON](/15-serialization/03-json/): structs and enums to JSON in depth, including `Option` and collection fields.
  - [Serde Attributes](/15-serialization/05-attributes/) — `rename`, `rename_all`, `skip`, `default`, `flatten`, `tag`, and more.
  - [Custom Serialization](/15-serialization/07-custom-serialization/): hand-writing `Serialize`/`Deserialize` when the derive is not enough.
  - [Beyond JSON](/15-serialization/06-other-formats/) — the *same* derived types as TOML, YAML, MessagePack, and binary.
  - [Serde Performance](/15-serialization/08-performance/): borrowing, zero-copy, and avoiding `Value`.
  - [Section 09: Generics & Traits](/09-generics-traits/) — what traits are and how trait bounds work.
  - [Section 14: Macros](/14-macros/): how derive macros generate code at compile time.

---

## Exercises

### Exercise 1: Make a type round-trip

**Difficulty:** Easy

**Objective:** Get comfortable adding the derive and round-tripping a value through JSON.

**Instructions:**

1. Define a `BlogPost` struct with fields `title: String`, `author: String`, `tags: Vec<String>`, and `published: bool`.
2. Make it both serializable and deserializable.
3. In `main`, construct a value, serialize it to a JSON string, print it, then deserialize it back and print the resulting struct with `{:?}`.

```rust
use serde::{Deserialize, Serialize};

// TODO: add the right derive(s)
struct BlogPost {
    title: String,
    author: String,
    tags: Vec<String>,
    published: bool,
}

fn main() {
    // TODO: build, serialize, print, deserialize, print
}
```

<details>
<summary>Solution</summary>

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct BlogPost {
    title: String,
    author: String,
    tags: Vec<String>,
    published: bool,
}

fn main() {
    let post = BlogPost {
        title: "Rust for TS devs".into(),
        author: "ada".into(),
        tags: vec!["rust".into(), "serde".into()],
        published: true,
    };

    let json = serde_json::to_string(&post).unwrap();
    println!("{json}");
    // {"title":"Rust for TS devs","author":"ada","tags":["rust","serde"],"published":true}

    let back: BlogPost = serde_json::from_str(&json).unwrap();
    println!("{back:?}");
    // BlogPost { title: "Rust for TS devs", author: "ada", tags: ["rust", "serde"], published: true }
}
```

</details>

### Exercise 2: A data-carrying enum and a deserialization error

**Difficulty:** Medium

**Objective:** Observe how struct-variant enums serialize, and see a *real* error when a required field is missing.

**Instructions:**

1. Define an enum `Shape` with three struct variants: `Circle { radius: f64 }`, `Rectangle { width: f64, height: f64 }`, and `Triangle { base: f64, height: f64 }`. Derive both Serde traits.
2. Write a function `area(&Shape) -> f64` that `match`es each variant.
3. Serialize one of each variant and print the JSON alongside its area.
4. Then attempt to deserialize the *invalid* JSON `{"Circle":{}}` (missing `radius`) into a `Shape`, and print the error message instead of unwrapping.

<details>
<summary>Solution</summary>

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Triangle { base: f64, height: f64 },
}

fn area(shape: &Shape) -> f64 {
    match shape {
        Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
        Shape::Rectangle { width, height } => width * height,
        Shape::Triangle { base, height } => 0.5 * base * height,
    }
}

fn main() {
    let shapes = vec![
        Shape::Circle { radius: 2.0 },
        Shape::Rectangle { width: 3.0, height: 4.0 },
        Shape::Triangle { base: 6.0, height: 2.0 },
    ];

    for s in &shapes {
        println!("{} -> area {:.2}", serde_json::to_string(s).unwrap(), area(s));
    }

    // A missing required field is a real, recoverable error.
    let bad = r#"{"Circle":{}}"#;
    let result: Result<Shape, _> = serde_json::from_str(bad);
    println!("{:?}", result.err().map(|e| e.to_string()));
}
```

Real output:

```text
{"Circle":{"radius":2.0}} -> area 12.57
{"Rectangle":{"width":3.0,"height":4.0}} -> area 12.00
{"Triangle":{"base":6.0,"height":2.0}} -> area 6.00
Some("missing field `radius` at line 1 column 12")
```

</details>

### Exercise 3: Hand-write what the derive generates

**Difficulty:** Hard

**Objective:** Prove to yourself that `#[derive(Serialize)]` produces ordinary code by writing an equivalent `impl` and confirming it yields byte-identical JSON.

**Instructions:**

1. Define a `Temperature` struct with one field `celsius: f64`. Derive only `Deserialize` on it.
2. Hand-write `impl Serialize for Temperature` using `serialize_struct` / `serialize_field` / `end`, matching the field name `"celsius"` exactly.
3. Define a second struct `TemperatureDerived` with the same field but `#[derive(Serialize)]`.
4. Serialize one value of each and `assert_eq!` that the two JSON strings are identical.

<details>
<summary>Solution</summary>

```rust
use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Temperature {
    celsius: f64,
}

// Hand-written Serialize that mirrors the derive output exactly.
impl Serialize for Temperature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Temperature", 1)?;
        state.serialize_field("celsius", &self.celsius)?;
        state.end()
    }
}

#[derive(Debug, serde::Serialize)]
struct TemperatureDerived {
    celsius: f64,
}

fn main() {
    let manual = Temperature { celsius: 21.5 };
    let derived = TemperatureDerived { celsius: 21.5 };

    let a = serde_json::to_string(&manual).unwrap();
    let b = serde_json::to_string(&derived).unwrap();
    println!("manual:  {a}");
    println!("derived: {b}");

    assert_eq!(a, b);
    println!("identical: {}", a == b);
}
```

Real output:

```text
manual:  {"celsius":21.5}
derived: {"celsius":21.5}
identical: true
```

</details>
