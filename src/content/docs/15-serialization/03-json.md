---
title: "Structs and JSON"
description: "TypeScript's JSON.parse as User is an unchecked cast; Rust maps JSON onto typed structs and enums with serde_json, validating every field, Option, and tagged union."
---

In TypeScript you reach for `JSON.parse` / `JSON.stringify` and trust that the bytes match your `interface`. In Rust, you map JSON onto strongly typed **structs** and **enums** with Serde, and the compiler — plus the deserializer — guarantees the shape really matches before you ever touch a field.

---

## Quick Overview

This page is about mapping JSON documents onto Rust **structs** and **enums**: nested objects, arrays (`Vec<T>`), dynamically keyed objects (`HashMap<K, V>`), optional/`null` fields (`Option<T>`), and the four ways Serde can represent an enum (tagged unions) in JSON. You will use [`serde_json`](https://docs.rs/serde_json) to turn typed values into JSON and back, with the same `#[derive(Serialize, Deserialize)]` you met in [Serde Basics](/15-serialization/01-serde-basics/) and [Deriving `Serialize` and `Deserialize`](/15-serialization/02-derive-serialize/). The big shift from TypeScript: the structure is checked at deserialization time, not merely asserted at compile time and then ignored at runtime.

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects the newest edition automatically. The examples here use `serde` 1.x (with the `derive` feature) and `serde_json` 1.x.

---

## TypeScript/JavaScript Example

```typescript
// TypeScript: an interface describes the shape, JSON.* moves bytes <-> objects.
interface Address {
  street: string;
  city: string;
  zip: string;
}

interface User {
  id: number;
  name: string;
  email: string;
  address: Address; // nested object
  roles: string[]; // JSON array
  settings: Record<string, boolean>; // object with dynamic keys
  nickname: string | null; // may be absent or null
}

const user: User = {
  id: 7,
  name: "Ada Lovelace",
  email: "ada@example.com",
  address: { street: "12 Analytical Ave", city: "London", zip: "EC1A" },
  roles: ["admin", "author"],
  settings: { dark_mode: true },
  nickname: null,
};

const json = JSON.stringify(user);
const parsed = JSON.parse(json) as User; // <- a cast, NOT a check
console.log(parsed.address.city); // "London"
```

The catch: `JSON.parse(json) as User` is a **lie the compiler believes**. TypeScript types are erased at runtime, so if the server sends `{ "id": "7" }` or omits `address`, `parsed` still has type `User` and you crash later, far from the parse site. The `as` cast performs zero validation.

---

## Rust Equivalent

```rust playground
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct Address {
    street: String,
    city: String,
    zip: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String,
    address: Address,                // nested struct
    roles: Vec<String>,              // JSON array
    settings: HashMap<String, bool>, // JSON object with dynamic keys
    nickname: Option<String>,        // may be absent / null
}

fn main() {
    let mut settings = HashMap::new();
    settings.insert("dark_mode".to_string(), true);

    let user = User {
        id: 7,
        name: "Ada Lovelace".to_string(),
        email: "ada@example.com".to_string(),
        address: Address {
            street: "12 Analytical Ave".to_string(),
            city: "London".to_string(),
            zip: "EC1A".to_string(),
        },
        roles: vec!["admin".to_string(), "author".to_string()],
        settings,
        nickname: None,
    };

    // Serialize to a compact JSON string.
    let json = serde_json::to_string(&user).unwrap();
    println!("compact: {json}");

    // Pretty-printed (two-space indent).
    let pretty = serde_json::to_string_pretty(&user).unwrap();
    println!("pretty:\n{pretty}");

    // Round-trip: parse it back into a typed value. This VALIDATES the shape.
    let parsed: User = serde_json::from_str(&json).unwrap();
    println!("parsed city: {}", parsed.address.city);
    println!("nickname is none: {}", parsed.nickname.is_none());
}
```

Real output:

```text
compact: {"id":7,"name":"Ada Lovelace","email":"ada@example.com","address":{"street":"12 Analytical Ave","city":"London","zip":"EC1A"},"roles":["admin","author"],"settings":{"dark_mode":true},"nickname":null}
pretty:
{
  "id": 7,
  "name": "Ada Lovelace",
  "email": "ada@example.com",
  "address": {
    "street": "12 Analytical Ave",
    "city": "London",
    "zip": "EC1A"
  },
  "roles": [
    "admin",
    "author"
  ],
  "settings": {
    "dark_mode": true
  },
  "nickname": null
}
parsed city: London
nickname is none: true
```

Unlike TypeScript's `as User`, `serde_json::from_str::<User>(...)` actually inspects the JSON. If `id` is a string, or `address` is missing, you get an `Err`, not a time bomb. We'll see those exact error messages in [Common Pitfalls](#common-pitfalls).

---

## Detailed Explanation

### The two-step model: data type, then format

Serde splits the work cleanly. `#[derive(Serialize, Deserialize)]` teaches your **type** how to describe itself as a stream of generic data-model events ("a struct with field `id` of value `7`…"). `serde_json` is the **format** crate that turns those events into JSON bytes and back. The same derived `User` works with TOML, YAML, MessagePack, and more (see [Beyond JSON](/15-serialization/06-other-formats/)) without changing the struct. This architecture is covered in [Serde](/15-serialization/00-serde-intro/).

### Field mapping, one line at a time

- `id: u32` ↔ a JSON number. Rust's many integer types ([Section 02: Basic Types](/02-basics/01-types/)) all map to JSON numbers, but they round-trip with their full range — unlike JavaScript, where every `number` is an IEEE-754 `f64` and integers past 2^53 silently lose precision. A `u64` in Rust keeps every bit.
- `name: String` ↔ a JSON string. (`String` is the owned, growable string; `&str` borrows. See [Serde Performance](/15-serialization/08-performance/) for borrowing JSON without copying.)
- `address: Address` ↔ a nested JSON object. Serde recurses: as long as `Address` also derives the traits, nesting "just works" to any depth.
- `roles: Vec<String>` ↔ a JSON array. Any `Vec<T>` where `T` is (de)serializable becomes an array; a `Vec<Address>` becomes an array of objects.
- `settings: HashMap<String, bool>` ↔ a JSON object with arbitrary keys. Use a `HashMap` (or `BTreeMap` for sorted, deterministic key order) when the keys are data you don't know at compile time. Use a `struct` when the keys are a fixed, known set.
- `nickname: Option<String>` ↔ a value that may be `null` or absent. `Some("x")` serializes to `"x"`, `None` serializes to `null`. On the way in, both `null` **and a missing key** become `None`.

### Serializing: `to_string`, `to_string_pretty`, `to_vec`

`serde_json::to_string(&value)` returns a compact `String`. `to_string_pretty` adds newlines and two-space indentation for human-readable output (logs, config files, debugging). When you're writing bytes to a socket or file, `serde_json::to_vec(&value)` skips the UTF-8 `String` step and hands you a `Vec<u8>` directly. All three return `Result` because a custom `Serialize` impl can fail; for plain derived structs it effectively never does, so `.unwrap()` is fine in examples (use `?` in real code — see [Section 08: The `?` Operator](/08-error-handling/01-question-mark/)).

### Deserializing validates the shape

`serde_json::from_str::<T>(s)` parses the text and builds a `T`, returning `Result<T, serde_json::Error>`. Every field is checked: types must match, non-optional fields must be present. This is the single most important difference from `JSON.parse(...) as T`. The cost of a typo or a schema drift is paid **at the boundary**, as an `Err` you handle, rather than as an `undefined` that detonates three function calls later.

### Top-level arrays and maps

You don't need a wrapper struct for everything. A JSON array of objects deserializes straight into a `Vec<T>`:

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let points: Vec<Point> =
        serde_json::from_str(r#"[{"x":1,"y":2},{"x":3,"y":4}]"#).unwrap();
    println!("points: {points:?}");
}
```

Output: `points: [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]`.

### Enums: tagged unions, four ways

A TypeScript discriminated union like `{ kind: "circle"; radius: number } | { kind: "rect"; w: number; h: number }` maps onto a Rust `enum`. Serde offers four JSON **representations**, chosen with attributes. Given:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Unit,
}
```

the **default (externally tagged)** form wraps each value in an object keyed by the variant name (and a bare string for a unit variant):

```text
[{"Circle":{"radius":1.5}},{"Rectangle":{"width":2.0,"height":3.0}},"Unit"]
```

Add `#[serde(tag = "type")]` for the **internally tagged** form: the variant name lives in a field alongside the data, which is what most web APIs use:

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum Event {
    Click { x: i32, y: i32 },
    KeyPress { key: String },
}
```

```text
[{"type":"Click","x":10,"y":20},{"type":"KeyPress","key":"Enter"}]
```

`#[serde(tag = "kind", content = "data")]` gives the **adjacently tagged** form (tag and payload in two sibling fields):

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum Message {
    Text(String),
    Ping,
}
```

```text
[{"kind":"Text","data":"hi"},{"kind":"Ping"}]
```

Finally, `#[serde(untagged)]` produces **untagged** values that carry no discriminator at all. Serde tries each variant in order and keeps the first that fits:

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Scalar {
    Num(i64),
    Text(String),
}
```

```text
[42,"abc"]
```

Serializing one value of each enum side by side makes the four shapes easy to compare:

```text
external: [{"Circle":{"radius":1.5}},{"Rectangle":{"width":2.0,"height":3.0}},"Unit"]
internal: [{"type":"Click","x":10,"y":20},{"type":"KeyPress","key":"Enter"}]
adjacent: [{"kind":"Text","data":"hi"},{"kind":"Ping"}]
untagged: [42,"abc"]
```

> **Note:** Adjacent and external tagging both accept a newtype variant such as `Message::Text(String)`, because the payload lives in its own slot (the `data` field, or the value behind the variant key). Internal tagging is the exception: a newtype variant works with `#[serde(tag = "...")]` only if it wraps a struct or map, since the tag field has to be merged into the payload object. The `tag`/`content`/`untagged` attributes are part of the broader attribute toolkit covered in [Serde Attributes](/15-serialization/05-attributes/).

### Option fields: missing vs. `null`

This trips up everyone, so it's worth a dedicated demonstration:

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Profile {
    name: String,
    bio: Option<String>,
}

fn main() {
    // Field present with a value.
    let a: Profile = serde_json::from_str(r#"{"name":"A","bio":"hello"}"#).unwrap();
    println!("with value: {:?}", a.bio);

    // Field present but null.
    let b: Profile = serde_json::from_str(r#"{"name":"B","bio":null}"#).unwrap();
    println!("explicit null: {:?}", b.bio);

    // Field entirely missing -> also None.
    let c: Profile = serde_json::from_str(r#"{"name":"C"}"#).unwrap();
    println!("missing field: {:?}", c.bio);

    // Serializing None emits null by default.
    let p = Profile { name: "D".into(), bio: None };
    println!("serialized None: {}", serde_json::to_string(&p).unwrap());
}
```

Output:

```text
with value: Some("hello")
explicit null: None
missing field: None
serialized None: {"name":"D","bio":null}
```

Two takeaways: an `Option<T>` field makes the key optional on the way in (both `null` and "absent" deserialize to `None`), and `None` serializes to `null` on the way out. If you'd rather **omit** the key entirely when it's `None`, add `#[serde(skip_serializing_if = "Option::is_none")]`. See [Serde Attributes](/15-serialization/05-attributes/).

---

## Key Differences

| Concern | TypeScript (`JSON.*`) | Rust (Serde + `serde_json`) |
| --- | --- | --- |
| Validation on parse | None; `as T` is an unchecked cast, types erased at runtime | Full: types and required fields checked; mismatch → `Err` |
| Integer precision | All numbers are `f64`; integers > 2^53 lose precision | Each integer type keeps its full range (`u64` is exact) |
| Optional fields | `field?: T` or `T | undefined`; runtime presence unknown | `Option<T>`; `None` for both missing key and `null` |
| Dynamic-keyed object | `Record<string, V>` | `HashMap<String, V>` / `BTreeMap<String, V>` |
| Discriminated union | `{ kind: "a" } | { kind: "b" }` | `enum` + `#[serde(tag = ...)]` (four representations) |
| Unknown extra fields | Silently kept on the object | Silently **ignored** by default; opt into `deny_unknown_fields` |
| Where errors surface | Late, deep in your code | At the parse boundary, as a value |

The mental shift: in TypeScript, JSON validation is *your* job (or a library like Zod's). In Rust, the deserializer **is** the validator, generated from the type definition. You describe the shape once, in the struct, and get parsing, validation, and serialization for free.

> **Tip:** Serde does not invent a runtime schema validator. It generates exact (de)serialization code per type at compile time via monomorphization, the same way Rust handles generics generally (TypeScript erases generics; Rust specializes them). There is no reflection and no per-call schema lookup.

---

## Common Pitfalls

### Pitfall 1: Assuming a missing field is harmless

A non-`Option` field is **required**. Leave it out and deserialization fails, which is usually what you want, but it surprises developers coming from `JSON.parse`'s permissiveness.

```rust playground
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    host: String,
    port: u16,
}

fn main() {
    // "port" is missing from the JSON.
    let result: Result<Config, _> = serde_json::from_str(r#"{"host":"localhost"}"#);
    match result {
        Ok(c) => println!("ok: {}:{}", c.host, c.port),
        Err(e) => println!("error: {e}"),
    }

    // Wrong type: port is a string, not a number.
    let result2: Result<Config, _> =
        serde_json::from_str(r#"{"host":"localhost","port":"8080"}"#);
    match result2 {
        Ok(c) => println!("ok: {}:{}", c.host, c.port),
        Err(e) => println!("error: {e}"),
    }
}
```

Real output:

```text
error: missing field `port` at line 1 column 20
error: invalid type: string "8080", expected u16 at line 1 column 33
```

The fix is intentional: make the field `Option<u16>`, or add `#[serde(default)]` to fall back to `u16::default()` (which is `0`) when absent. Both are covered in [Serde Attributes](/15-serialization/05-attributes/). The second error is the precision/type guarantee in action: `"8080"` (a string) is *not* accepted where a `u16` is expected, even though JavaScript's `+"8080"` would coerce.

### Pitfall 2: Expecting unknown fields to be rejected

By default, Serde **ignores** JSON keys that don't map to a struct field. This is forgiving (good for evolving APIs) but can hide typos in your data.

```rust playground
use serde::Deserialize;

// Unknown fields are silently ignored by default.
#[derive(Debug, Deserialize)]
struct Lenient {
    name: String,
}

// deny_unknown_fields makes extra keys an error.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Strict {
    name: String,
}

fn main() {
    let json = r#"{"name":"Grace","extra":true,"another":42}"#;

    let lenient: Lenient = serde_json::from_str(json).unwrap();
    println!("lenient ok: {}", lenient.name);

    let strict: Result<Strict, _> = serde_json::from_str(json);
    match strict {
        Ok(s) => println!("strict ok: {}", s.name),
        Err(e) => println!("strict error: {e}"),
    }
}
```

Real output:

```text
lenient ok: Grace
strict error: unknown field `extra`, expected `name` at line 1 column 23
```

### Pitfall 3: Untagged enum variant order

With `#[serde(untagged)]`, Serde tries variants **top to bottom** and accepts the first that parses. Put a broad variant first and it will swallow inputs meant for a stricter one. A JSON `42` is a valid `f64`, so a `Float`-first enum never reaches `Int`:

```rust playground
use serde::{Deserialize, Serialize};

// WRONG order: Float matches integers too, so 42 deserializes as Float(42.0).
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum NumWrong {
    Float(f64),
    Int(i64),
}

// RIGHT order: try the stricter Int variant first.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum NumRight {
    Int(i64),
    Float(f64),
}

fn main() {
    let w: NumWrong = serde_json::from_str("42").unwrap();
    println!("wrong order, input 42 -> {w:?}");

    let r: NumRight = serde_json::from_str("42").unwrap();
    println!("right order, input 42 -> {r:?}");

    let r2: NumRight = serde_json::from_str("3.5").unwrap();
    println!("right order, input 3.5 -> {r2:?}");
}
```

Real output:

```text
wrong order, input 42 -> Float(42.0)
right order, input 42 -> Int(42)
right order, input 3.5 -> Float(3.5)
```

> **Warning:** Untagged enums also produce vaguer error messages ("data did not match any variant…") because Serde can only report that *nothing* matched, not *why* each variant failed. Prefer an internally or adjacently tagged enum whenever the JSON has (or can carry) a discriminator field. Reserve `untagged` for genuinely tagless inputs like "an ID is either a number or a string".

### Pitfall 4: Reaching for `serde_json::Value` too early

Coming from JavaScript, it's tempting to deserialize into `serde_json::Value` (a dynamic JSON tree) and index it like a JS object. That throws away the type checking that is the whole point — and is slower. Use a typed struct whenever the shape is known. `Value` is for genuinely dynamic JSON; that's the topic of [Dynamic JSON with `serde_json::Value`](/15-serialization/04-json-manipulation/).

---

## Best Practices

- **Model the known shape as a `struct`/`enum`; reach for `HashMap`/`Value` only for genuinely dynamic data.** Typed deserialization is your validation layer; lean on it.
- **Always derive `Debug`** alongside `Serialize`/`Deserialize`. It costs nothing and makes `{:?}` printing and test assertions painless.
- **Use `Option<T>` for fields that may be absent**, and combine it with `#[serde(skip_serializing_if = "Option::is_none")]` when you want absent-means-omitted on the way out. Use `#[serde(default)]` for fields that should fall back to a sensible value.
- **Prefer internally tagged enums (`#[serde(tag = "...")]`)** for API payloads: they read naturally as JSON and give precise error messages. Save `untagged` for tagless inputs, and order its variants strictest-first.
- **Match JSON's `camelCase` with `#[serde(rename_all = "camelCase")]`** at the struct level instead of renaming each field; keep your Rust fields idiomatic `snake_case`. (Details in [Serde Attributes](/15-serialization/05-attributes/).)
- **Use `BTreeMap` instead of `HashMap`** when you need deterministic, sorted key ordering in the output (e.g. for stable snapshots or signatures).
- **Propagate errors with `?`** instead of `.unwrap()` outside of examples and tests. `serde_json::Error` implements `std::error::Error`, so it composes with `Box<dyn Error>`, `anyhow`, and `thiserror` (see [Section 08: Error Handling](/08-error-handling/)).

---

## Real-World Example

A paginated API response: a wrapper with pagination metadata, a `Vec` of nested `Order` objects, a `HashMap` of feature flags, an `Option` field that only appears once an order ships, and an internally tagged status enum: exactly the kind of payload a Rust service receives from another service or returns to a client.

```rust playground
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse {
    page: u32,
    per_page: u32,
    total: u64,
    items: Vec<Order>,
    // Server-supplied feature flags keyed by name.
    flags: HashMap<String, bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Order {
    id: u64,
    customer: Customer,
    lines: Vec<LineItem>,
    status: OrderStatus,
    // Present only once the order ships.
    tracking_number: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Customer {
    name: String,
    email: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LineItem {
    sku: String,
    quantity: u32,
    unit_price_cents: u64,
}

// An internally tagged enum models a discriminated union, just like a
// TypeScript `{ status: "shipped"; carrier: string } | ...`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum OrderStatus {
    Pending,
    Shipped { carrier: String },
    Cancelled { reason: String },
}

fn handle_payload(raw: &str) -> Result<ApiResponse, serde_json::Error> {
    let response: ApiResponse = serde_json::from_str(raw)?;
    Ok(response)
}

fn main() {
    let raw = r#"
    {
      "page": 1,
      "per_page": 20,
      "total": 137,
      "flags": { "new_checkout": true, "promo_banner": false },
      "items": [
        {
          "id": 1001,
          "customer": { "name": "Ada", "email": "ada@example.com" },
          "lines": [
            { "sku": "BOOK-1", "quantity": 2, "unit_price_cents": 1599 }
          ],
          "status": { "status": "shipped", "carrier": "DHL" },
          "tracking_number": "1Z999"
        },
        {
          "id": 1002,
          "customer": { "name": "Linus", "email": "linus@example.com" },
          "lines": [
            { "sku": "MUG-7", "quantity": 1, "unit_price_cents": 899 }
          ],
          "status": { "status": "pending" }
        }
      ]
    }"#;

    let response = handle_payload(raw).expect("valid payload");

    println!("page {} of {} total orders", response.page, response.total);
    println!("new_checkout flag = {}", response.flags["new_checkout"]);

    for order in &response.items {
        let total_cents: u64 = order
            .lines
            .iter()
            .map(|l| l.quantity as u64 * l.unit_price_cents)
            .sum();
        let summary = match &order.status {
            OrderStatus::Pending => "pending".to_string(),
            OrderStatus::Shipped { carrier } => {
                let tracking = order.tracking_number.as_deref().unwrap_or("n/a");
                format!("shipped via {carrier} (tracking {tracking})")
            }
            OrderStatus::Cancelled { reason } => format!("cancelled: {reason}"),
        };
        println!(
            "order {} for {} -> {} | total ${:.2}",
            order.id,
            order.customer.name,
            summary,
            total_cents as f64 / 100.0
        );
    }

    // Re-serialize one order to forward to another service.
    let echo = serde_json::to_string(&response.items[0]).unwrap();
    println!("echo: {echo}");
}
```

Real output:

```text
page 1 of 137 total orders
new_checkout flag = true
order 1001 for Ada -> shipped via DHL (tracking 1Z999) | total $31.98
order 1002 for Linus -> pending | total $8.99
echo: {"id":1001,"customer":{"name":"Ada","email":"ada@example.com"},"lines":[{"sku":"BOOK-1","quantity":2,"unit_price_cents":1599}],"status":{"status":"shipped","carrier":"DHL"},"tracking_number":"1Z999"}
```

Notice what the type system bought you: the `match` on `order.status` is **exhaustive** — add a `Refunded` variant later and the compiler forces you to handle it everywhere. Prices are kept in integer cents (`u64`), sidestepping the float-rounding bugs you'd risk in JavaScript, and only formatted as a float for display. And `handle_payload` returns a `Result`, so a malformed payload is a caught error, not a thrown exception that escapes the function. When this struct backs an HTTP handler, the same derives plug straight into a web framework. See [Section 16: Web APIs](/16-web-apis/).

---

## Further Reading

- [Serde — Overview](https://serde.rs/): the official guide to the data-model and derive macros.
- [Serde — Enum representations](https://serde.rs/enum-representations.html): externally/internally/adjacently tagged and untagged, with examples.
- [`serde_json` API docs](https://docs.rs/serde_json): `to_string`, `to_vec`, `from_str`, `from_slice`, and the `Value` type.
- [Serde — Examples](https://serde.rs/examples.html): struct, enum, and collection recipes.

Related sections in this guide:

- [Serde](/15-serialization/00-serde-intro/) — the `Serialize`/`Deserialize` traits and the data-model-vs-format architecture.
- [Serde Basics](/15-serialization/01-serde-basics/) — adding `serde` + `serde_json` and your first `to_string`/`from_str`.
- [Deriving `Serialize` and `Deserialize`](/15-serialization/02-derive-serialize/) — what `#[derive(Serialize, Deserialize)]` generates.
- [Serde Attributes](/15-serialization/05-attributes/) — `rename`, `rename_all`, `skip_serializing_if`, `default`, `flatten`, `tag`, `deny_unknown_fields`, and more.
- [Dynamic JSON with `serde_json::Value`](/15-serialization/04-json-manipulation/) — dynamic JSON with `serde_json::Value` and the `json!` macro.
- [Beyond JSON](/15-serialization/06-other-formats/) — the same structs serialized as TOML, YAML, MessagePack, and CSV.
- [Custom Serialization](/15-serialization/07-custom-serialization/) — hand-written `Serialize`/`Deserialize` and `with`/`serialize_with`.
- [Serde Performance](/15-serialization/08-performance/) — borrowing with `&str`/`#[serde(borrow)]` and zero-copy deserialization.
- Foundations: [Section 02: Basic Types](/02-basics/01-types/) for the numeric types JSON maps onto, and [Section 08: Error Handling](/08-error-handling/) for handling `serde_json::Error` with `?`.

---

## Exercises

### Exercise 1: Round-trip a blog post

**Difficulty:** Easy

**Objective:** Define a struct that mixes a string, an array, and a boolean, then serialize and deserialize it.

**Instructions:** Create a `BlogPost` struct with `title: String`, `tags: Vec<String>`, and `published: bool`. Build one, serialize it to a JSON string with `serde_json::to_string`, print it, then deserialize it back and print the recovered title. Remember the `derive` feature and the `Serialize`/`Deserialize` derives.

```rust playground
use serde::{Deserialize, Serialize};

// TODO: derive Serialize + Deserialize (and Debug)
struct BlogPost {
    title: String,
    tags: Vec<String>,
    published: bool,
}

fn main() {
    let post = BlogPost {
        title: "Hello".into(),
        tags: vec!["rust".into(), "serde".into()],
        published: true,
    };
    // TODO: serialize, print, deserialize, print the title
}
```

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct BlogPost {
    title: String,
    tags: Vec<String>,
    published: bool,
}

fn main() {
    let post = BlogPost {
        title: "Hello".into(),
        tags: vec!["rust".into(), "serde".into()],
        published: true,
    };

    let s = serde_json::to_string(&post).unwrap();
    println!("serialized: {s}");

    let back: BlogPost = serde_json::from_str(&s).unwrap();
    println!("recovered title: {}", back.title);
}
```

Output:

```text
serialized: {"title":"Hello","tags":["rust","serde"],"published":true}
recovered title: Hello
```

</details>

### Exercise 2: Nested structs, a map, and an optional field

**Difficulty:** Medium

**Objective:** Combine nesting, `Vec<Struct>`, `HashMap`, and `Option` in one type.

**Instructions:** Model a `Team` with `name: String`, `members: Vec<Member>`, and `scores: HashMap<String, u32>`. A `Member` has `handle: String` and an optional `captain: Option<bool>`. Construct a team where one member has `captain: Some(true)` and another has `None`, then serialize it. Confirm in the output that the `None` captain becomes `null` while the map and the nested array serialize correctly.

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct Team {
    name: String,
    members: Vec<Member>,
    scores: HashMap<String, u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Member {
    handle: String,
    captain: Option<bool>,
}

fn main() {
    let mut scores = HashMap::new();
    scores.insert("round1".to_string(), 42u32);

    let team = Team {
        name: "Crustaceans".into(),
        members: vec![
            Member { handle: "ferris".into(), captain: Some(true) },
            Member { handle: "gopher".into(), captain: None },
        ],
        scores,
    };

    println!("{}", serde_json::to_string(&team).unwrap());
}
```

Output:

```text
{"name":"Crustaceans","members":[{"handle":"ferris","captain":true},{"handle":"gopher","captain":null}],"scores":{"round1":42}}
```

The `None` captain rendered as `null`. To omit it instead, you'd add `#[serde(skip_serializing_if = "Option::is_none")]`. See [Serde Attributes](/15-serialization/05-attributes/).

</details>

### Exercise 3: A tagged command protocol

**Difficulty:** Hard

**Objective:** Use an internally tagged enum to parse a JSON command stream and re-serialize it.

**Instructions:** Define a `Command` enum with variants `Move { x: i32, y: i32 }`, `Say { text: String }`, and `Quit`. Use `#[serde(tag = "op", rename_all = "lowercase")]` so the discriminator is the `op` field and variant names appear lowercased. Deserialize the array `[{"op":"move","x":1,"y":2},{"op":"say","text":"hi"},{"op":"quit"}]` into a `Vec<Command>`, print it with `{:?}`, then serialize it back and confirm the JSON matches.

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Command {
    Move { x: i32, y: i32 },
    Say { text: String },
    Quit,
}

fn main() {
    let input = r#"[{"op":"move","x":1,"y":2},{"op":"say","text":"hi"},{"op":"quit"}]"#;

    let cmds: Vec<Command> = serde_json::from_str(input).unwrap();
    println!("parsed: {cmds:?}");

    let json = serde_json::to_string(&cmds).unwrap();
    println!("reserialized: {json}");
}
```

Output:

```text
parsed: [Move { x: 1, y: 2 }, Say { text: "hi" }, Quit]
reserialized: [{"op":"move","x":1,"y":2},{"op":"say","text":"hi"},{"op":"quit"}]
```

The round-trip is lossless: `rename_all = "lowercase"` controls the variant names, and `tag = "op"` places the discriminator inline. Internal tagging like this is the idiomatic way to model a JSON discriminated union in Rust.

</details>
