---
title: "Dynamic JSON with `serde_json::Value`"
description: "serde_json::Value is Rust's answer to JavaScript's any JSON blob: a typed enum you build with json!, index dynamically, and convert to structs, keeping big"
---

In TypeScript, `JSON.parse` hands you an `any`-typed blob you can poke at freely: `data.users[0].name`. Rust's equivalent is `serde_json::Value`, a tree-shaped enum you build with the `json!` macro, navigate with indexing, and convert to and from typed structs when you're ready to lock things down.

---

## Quick Overview

`serde_json::Value` is Rust's representation of "arbitrary JSON whose shape I don't know (or don't care about) at compile time." It is an enum with one variant per JSON type (null, bool, number, string, array, object) and it gives you the dynamic, `data["key"][0]` style of access that TypeScript developers reach for by default. This matters because not all JSON is worth a dedicated struct: webhook payloads, partial config overrides, and pass-through proxies are often easier to handle as a `Value` than as a fully typed model.

> **Tip:** Reach for typed structs (`#[derive(Deserialize)]`) when you know and rely on the shape; reach for `Value` when the shape is genuinely unknown, partial, or irrelevant. This page is about the second case. The first case is covered in [Structs and JSON](/15-serialization/03-json/).

---

## TypeScript/JavaScript Example

In JavaScript, `JSON.parse` returns an untyped value, and you navigate it directly. This is ergonomic but unsafe: a typo or a missing field surfaces only at runtime, often as `undefined` propagating somewhere far away.

```typescript
// A webhook payload whose exact shape we don't model.
const raw = `{
  "event": "order.created",
  "data": {
    "id": 42,
    "customer": { "name": "Ada Lovelace" },
    "items": [
      { "sku": "A1", "qty": 2 },
      { "sku": "B2", "qty": 1 }
    ]
  }
}`;

const payload = JSON.parse(raw); // type is `any`

// Dynamic navigation — no compile-time checks
console.log(payload.event); // "order.created"
console.log(payload.data.customer.name); // "Ada Lovelace"
console.log(payload.data.items[0].sku); // "A1"

// Missing keys are `undefined`, not errors
console.log(payload.data.shipping); // undefined

// Building JSON dynamically is just object literals
const response = {
  ok: true,
  receivedAt: Date.now(),
  echo: payload.event,
};
console.log(JSON.stringify(response));
```

> **Warning:** JavaScript's `number` is always an IEEE-754 double. `JSON.parse('{"id": 9007199254740993}')` silently rounds the id to `9007199254740992`. The integer is *quietly corrupted* during parsing. It does not throw and it does not wrap; it loses precision. Keep this in mind as we compare with Rust below.

---

## Rust Equivalent

`serde_json::Value` gives you the same dynamic feel, but it is a real, typed enum value, and it preserves large integers exactly.

First, the dependencies. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically. Add `serde_json` (and `serde` if you also want typed conversion):

```bash
cargo add serde_json
cargo add serde --features derive
```

```rust
use serde_json::{json, Value};

fn main() {
    let raw = r#"{
        "event": "order.created",
        "data": {
            "id": 42,
            "customer": { "name": "Ada Lovelace" },
            "items": [
                { "sku": "A1", "qty": 2 },
                { "sku": "B2", "qty": 1 }
            ]
        }
    }"#;

    // Parse into a dynamic Value tree.
    let payload: Value = serde_json::from_str(raw).expect("valid JSON");

    // Dynamic navigation via indexing.
    println!("{}", payload["event"]);                    // "order.created"
    println!("{}", payload["data"]["customer"]["name"]); // "Ada Lovelace"
    println!("{}", payload["data"]["items"][0]["sku"]);  // "A1"

    // Missing keys are Value::Null, never a panic.
    println!("{}", payload["data"]["shipping"]);         // null

    // Building JSON dynamically with the json! macro.
    let response = json!({
        "ok": true,
        "received_at": 1_717_000_000,
        "echo": payload["event"],
    });
    println!("{}", response);
}
```

Running this prints (note the quotes around string values — explained below):

```text
"order.created"
"Ada Lovelace"
"A1"
null
{"echo":"order.created","ok":true,"received_at":1717000000}
```

---

## Detailed Explanation

### What `Value` actually is

`serde_json::Value` is a plain Rust enum. Knowing its definition demystifies everything else:

```rust
// (from the serde_json crate — shown for understanding, you don't write this)
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),       // holds u64, i64, or f64 internally
    String(String),
    Array(Vec<Value>),
    Object(Map<String, Value>), // sorted (BTreeMap) by default; insertion-ordered with the preserve_order feature
}
```

This is the same idea as TypeScript's structural `JSONValue` union:

```typescript
type JSONValue =
  | null
  | boolean
  | number
  | string
  | JSONValue[]
  | { [key: string]: JSONValue };
```

The difference: TypeScript's version is erased at runtime, so the compiler can't actually stop you from doing `payload.data.nope.boom`. Rust's `Value` is a concrete value you pattern-match on, so navigation is checked operations on a real data structure, not blind property access.

### Why printed strings have quotes

When you write `println!("{}", payload["event"])`, you are printing a `Value` through its `Display` implementation, and a `Value::String` displays itself as valid JSON, *including the surrounding quotes*. That is why the output was `"order.created"` and not `order.created`. To get the bare Rust `&str`, extract it:

```rust
use serde_json::json;

fn main() {
    let payload = json!({ "event": "order.created" });

    println!("{}", payload["event"]);              // "order.created"  (JSON form)

    let event: &str = payload["event"].as_str().unwrap();
    println!("{event}");                           // order.created    (bare string)
}
```

This is a frequent surprise. `Display` on a `Value` always yields JSON text; the `as_*` accessors give you native Rust types.

### Indexing with `[]`

`Value` implements `Index` for both `&str` (object keys) and `usize` (array positions), which is what enables the `payload["data"]["items"][0]` chain. Two properties make this pleasant *and* dangerous:

- **Reading a missing key or out-of-range index returns `Value::Null`** instead of panicking. So `payload["nope"]["deeper"]` is `Null`, not a crash, exactly like JavaScript returning `undefined`, except Rust gives you a real `Null` value you can match on.
- **Mutably indexing the wrong *type* panics.** `value["key"] = ...` on a `Value` that is a string or number panics, because there is nowhere to put the key. (See [Common Pitfalls](#common-pitfalls).)

### Extracting typed data: the `as_*` accessors

To pull a Rust value out of a `Value`, use the `as_*` family. Each returns an `Option` because the variant might not match; there is **no coercion**:

```rust
use serde_json::json;

fn main() {
    let v = json!({ "id": 42, "name": "Ada", "active": true });

    let id: Option<i64> = v["id"].as_i64();       // Some(42)
    let name: Option<&str> = v["name"].as_str();  // Some("Ada")
    let active: Option<bool> = v["active"].as_bool(); // Some(true)

    // No coercion: a number is NOT a string.
    let id_as_str: Option<&str> = v["id"].as_str(); // None

    println!("{id:?} {name:?} {active:?} {id_as_str:?}");
    // Some(42) Some("Ada") Some(true) None
}
```

Unlike JavaScript, where `String(payload.id)` or `payload.id + ""` happily coerces, `as_str()` on a number returns `None`. This is the `??`/`?.` mental model from TypeScript made explicit: every access that might fail hands you an `Option` you must deal with.

The common accessors:

| Accessor      | Returns            | Matches `Value` variant |
| ------------- | ------------------ | ----------------------- |
| `as_str()`    | `Option<&str>`     | `String`                |
| `as_i64()`    | `Option<i64>`      | `Number` (integer)      |
| `as_u64()`    | `Option<u64>`      | `Number` (non-negative) |
| `as_f64()`    | `Option<f64>`      | `Number` (any)          |
| `as_bool()`   | `Option<bool>`     | `Bool`                  |
| `as_array()`  | `Option<&Vec<Value>>` | `Array`              |
| `as_object()` | `Option<&Map<String, Value>>` | `Object`     |
| `as_null()`   | `Option<()>`       | `Null`                  |

### `get` vs `[]`

`[]` is concise but yields `Null` for misses. `.get("key")` / `.get(index)` returns `Option<&Value>`, which composes with `?` for clean early-returns:

```rust
use serde_json::{json, Value};

fn city_of(doc: &Value) -> Option<&str> {
    // Each ? bails out to None on the first missing/wrong-typed step.
    doc.get("address")?.get("city")?.as_str()
}

fn main() {
    let a = json!({ "address": { "city": "Berlin" } });
    let b = json!({ "address": {} });
    println!("{:?}", city_of(&a)); // Some("Berlin")
    println!("{:?}", city_of(&b)); // None
}
```

Use `[]` for quick reads where `Null` is an acceptable "miss", and `get(...)?` when you want to distinguish "absent" cleanly and short-circuit.

### The `json!` macro

`json!` lets you write JSON-shaped literals directly in Rust, and, importantly, interpolate Rust variables and expressions:

```rust
use serde_json::json;

fn main() {
    let name = "Ada";
    let scores = vec![90, 85, 100];

    let doc = json!({
        "name": name,                       // a &str variable
        "scores": scores,                   // a Vec<i32> -> JSON array
        "total": scores.iter().sum::<i32>(),// any expression
        "meta": { "active": true, "tags": ["a", "b"] },
    });

    println!("{doc}"); // {"meta":{"active":true,"tags":["a","b"]},"name":"Ada","scores":[90,85,100],"total":275}
}
```

Any type that implements `Serialize` can be interpolated into `json!`, so `Vec`, `HashMap`, and your own derived structs all drop straight in. This is far closer to a JS object literal than the manual `Map::insert` approach, though that exists too when you need to build objects programmatically.

### Numbers stay exact

This is where Rust beats JavaScript outright. `serde_json` stores integers as `i64`/`u64` internally, so a 53-bit-plus integer survives a parse round trip unharmed:

```rust
use serde_json::{json, Value};

fn main() {
    let big = json!({ "n": 9_007_199_254_740_993_i64 });
    println!("{}", big["n"]);           // 9007199254740993  (exact)
    println!("{:?}", big["n"].as_i64()); // Some(9007199254740993)
}
```

The same value parsed by Node's `JSON.parse` becomes `9007199254740992`, silently off by one. If you proxy or transform JSON that carries large integer IDs (Twitter/X snowflake IDs, database bigints), `serde_json::Value` preserves them; JavaScript does not.

### Mutation

A `Value` is just data, so a `mut` binding lets you edit the tree in place:

```rust
use serde_json::{json, Value};

fn main() {
    let mut config = json!({ "debug": false, "level": 1 });

    // Overwrite an existing key.
    config["debug"] = json!(true);

    // Read-modify-write.
    let next = config["level"].as_i64().unwrap() + 1;
    config["level"] = json!(next);

    // Insert a brand-new key via the underlying Map.
    if let Some(obj) = config.as_object_mut() {
        obj.insert("name".to_string(), json!("server"));
    }

    println!("{config}"); // {"debug":true,"level":2,"name":"server"}
}
```

`as_object_mut()` (and `as_array_mut()`) hand you `&mut` access to the underlying `Map`/`Vec` for inserts, removals, and entry-API tricks.

---

## Key Differences

| Concept | TypeScript (`JSON.parse`) | Rust (`serde_json::Value`) |
| ------- | ------------------------- | -------------------------- |
| Result type | `any` (effectively untyped) | A concrete `Value` enum |
| Missing key | `undefined` (and `.deeper` then throws) | `Value::Null` (chaining stays `Null`) |
| Type coercion | Implicit (`+ ""`, `String(x)`) | None; `as_str()` on a number is `None` |
| Large integers | Lossy (IEEE-754 f64) | Exact (`i64`/`u64`) |
| Building literals | Object literal `{ a: 1 }` | `json!({ "a": 1 })` macro |
| Object key order | Insertion order | Insertion order (with `preserve_order` feature) or sorted |
| Failure mode | Runtime `undefined`/`TypeError` | Compile-time `Option` you must handle |

### Object key ordering

By default, `serde_json`'s `Map` is backed by a `BTreeMap`, so object keys serialize in **sorted** order (that is why our examples printed `debug`, `level`, `name` alphabetically). If you need to preserve **insertion order** like a JavaScript object, enable the feature:

```bash
cargo add serde_json --features preserve_order
```

This swaps the backing store to `indexmap`, matching JavaScript's order-preserving semantics. It's worth turning on for round-tripping config files where humans care about key order.

### `Value` is owned data, not a view

A `Value` owns its strings and nested values. There is no concept of "live" navigation against the original text buffer (that is what zero-copy deserialization with `&str` is for; see [Serde Performance](/15-serialization/08-performance/)). When you clone a sub-tree out of a `Value`, you get an independent copy.

---

## Common Pitfalls

### Pitfall 1: Forgetting that `Display` adds quotes

```rust
use serde_json::json;

fn main() {
    let v = json!({ "name": "Ada" });
    let greeting = format!("Hello, {}!", v["name"]);
    println!("{greeting}"); // Hello, "Ada"!  <- note the stray quotes
}
```

The quotes are not a bug: `v["name"]` is printed as JSON. Extract the bare string with `.as_str()`:

```rust
use serde_json::json;

fn main() {
    let v = json!({ "name": "Ada" });
    let greeting = format!("Hello, {}!", v["name"].as_str().unwrap_or("guest"));
    println!("{greeting}"); // Hello, Ada!
}
```

### Pitfall 2: Mutably indexing the wrong type panics

Reading a missing key is safe (`Null`), but *writing* a key into something that isn't an object cannot work, so it panics:

```rust
use serde_json::{json, Value};

fn main() {
    let mut v: Value = json!("just a string");
    v["key"] = json!(1); // panics at runtime
    println!("{v}");
}
```

The real runtime panic:

```text
thread 'main' panicked at .../serde_json-1.0.150/src/value/index.rs:102:18:
cannot access key "key" in JSON string
```

Guard the type first (`if let Some(obj) = v.as_object_mut()`) or ensure `v` is an object before assigning into it.

### Pitfall 3: Assuming `as_i64()` coerces

```rust
use serde_json::json;

fn main() {
    // The JSON number was written as a float...
    let v = json!({ "qty": 2.0 });
    println!("{:?}", v["qty"].as_i64()); // None! 2.0 is not an integer to serde_json
    println!("{:?}", v["qty"].as_f64()); // Some(2.0)
}
```

`as_i64()` only succeeds for values serde_json classifies as integers. If the source might write `2.0`, read with `as_f64()` (then convert), or accept that `as_i64()` returns `None`. There is no silent float-to-int rounding.

### Pitfall 4: Comparing a `Value` and expecting deep numeric equality

`serde_json` implements `PartialEq` between `Value` and many Rust primitives, which is convenient, but integer and float JSON numbers are **distinct**:

```rust
use serde_json::json;

fn main() {
    let v = json!({ "event": "click", "x": 10 });

    // Comparing Value to a primitive works (no .as_* needed):
    println!("{}", v["event"] == "click"); // true
    println!("{}", v["x"] == 10);          // true (i32)
    println!("{}", v["x"] == 10.0);        // true (f64 compares equal here)

    // But two Values of different numeric kinds are NOT equal:
    println!("{}", json!(10) == json!(10.0)); // false — int vs float
}
```

If you need numeric equality regardless of integer/float representation, compare via `as_f64()` rather than comparing `Value`s directly.

### Pitfall 5: Treating a parse error like JavaScript's silent `undefined`

`serde_json::from_str::<Value>` returns a `Result`, and malformed JSON is a real error you must handle; there is no `undefined`:

```rust
use serde_json::Value;

fn main() {
    let bad = r#"{ "name": "Ada", }"#; // trailing comma is invalid JSON
    match serde_json::from_str::<Value>(bad) {
        Ok(v) => println!("ok: {v}"),
        Err(e) => println!("error: {e}"),
    }
}
```

Real output (serde_json reports line and column):

```text
error: trailing comma at line 1 column 18
```

> **Note:** Unlike `JSON.parse` (which throws), Rust forces you to acknowledge the `Result`. Use `?` to propagate, `match` to branch, or `.unwrap_or_default()` if an empty/`Null` fallback is acceptable.

---

## Best Practices

### Prefer typed structs at the boundary, `Value` for the unknown middle

`Value` is the right tool for genuinely dynamic JSON. But if you find yourself writing `payload["data"]["id"].as_i64().unwrap()` repeatedly, that field's shape *is* known — model it. You can even mix the two: derive a struct for the part you understand and keep a `Value` for the free-form remainder.

```rust
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct Webhook {
    event: String,
    // Keep the unmodeled payload as dynamic JSON.
    data: Value,
}

fn main() {
    let raw = r#"{ "event": "ping", "data": { "anything": [1, 2, 3] } }"#;
    let hook: Webhook = serde_json::from_str(raw).unwrap();
    println!("event = {}", hook.event);
    println!("first = {}", hook.data["anything"][0]); // dynamic from here
}
```

### Convert between `Value` and typed with `from_value` / `to_value`

When you do know the shape, `serde_json::from_value` turns a `Value` into a struct, and `to_value` goes the other way; no string round trip needed:

```rust
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
}

fn main() {
    let v = json!({ "id": 7, "name": "Grace" });

    let user: User = serde_json::from_value(v).unwrap(); // Value -> struct
    println!("{user:?}");                                // User { id: 7, name: "Grace" }

    let back: Value = serde_json::to_value(&user).unwrap(); // struct -> Value
    println!("{back}");                                  // {"id":7,"name":"Grace"}
}
```

### Use `pointer()` for deep, configurable paths

When the path to a value is itself data (e.g. comes from config), JSON Pointer syntax (RFC 6901) beats hand-written indexing:

```rust
use serde_json::{json, Value};

fn main() {
    let doc = json!({
        "data": { "items": [ { "sku": "A1" }, { "sku": "B2" } ] }
    });

    let sku = doc.pointer("/data/items/1/sku").and_then(Value::as_str);
    println!("{sku:?}"); // Some("B2")
}
```

### Don't pay for `Value` when you don't need dynamism

Parsing into `Value` allocates a node for every element and box every string into the tree. For hot paths and known shapes, deserializing straight into a struct is faster and lighter. The trade-off (and how to avoid `Value` for performance) is covered in [Serde Performance](/15-serialization/08-performance/).

---

## Real-World Example

A common production task: layering configuration. You have a base config and a set of overrides (from a file, environment, or CLI), and you want a deep merge where nested objects combine key-by-key but scalars and arrays are replaced. With `Value`, this is a short recursive function:

```rust
use serde_json::{json, Map, Value};

/// Recursively merge `patch` into `base`.
/// Two objects merge key-by-key; anything else overwrites.
fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            for (key, patch_val) in patch_map {
                // `entry` inserts Null if the key is new, then we recurse into it.
                merge(base_map.entry(key.clone()).or_insert(Value::Null), patch_val);
            }
        }
        // Non-object on either side: the patch wins.
        (base_slot, patch_val) => {
            *base_slot = patch_val.clone();
        }
    }
}

fn main() {
    let mut config = json!({
        "server": { "host": "localhost", "port": 8080 },
        "logging": { "level": "info" },
        "features": ["a", "b"]
    });

    let overrides = json!({
        "server": { "port": 9090 },
        "logging": { "level": "debug", "json": true },
        "features": ["a", "b", "c"]
    });

    merge(&mut config, &overrides);

    // Pull a final, typed value out once merging is done.
    let port = config["server"]["port"].as_u64().unwrap_or(8080);
    println!("effective port: {port}");

    println!("{}", serde_json::to_string_pretty(&config).unwrap());

    // You can also assemble objects from scratch with Map when json! is awkward.
    let mut summary = Map::new();
    summary.insert("merged".into(), Value::Bool(true));
    summary.insert("port".into(), json!(port));
    println!("{}", Value::Object(summary));
}
```

Output:

```text
effective port: 9090
{
  "features": [
    "a",
    "b",
    "c"
  ],
  "logging": {
    "json": true,
    "level": "debug"
  },
  "server": {
    "host": "localhost",
    "port": 9090
  }
}
{"merged":true,"port":9090}
```

Notice the host (`localhost`) survived from the base because `overrides` only touched `port`, while `features` was wholesale replaced because arrays aren't merged. This is exactly the kind of shape-agnostic transformation where `Value` shines and a fixed struct would get in the way.

---

## Further Reading

### Official Documentation

- [`serde_json::Value` API docs](https://docs.rs/serde_json/latest/serde_json/enum.Value.html) — every variant and `as_*` accessor
- [`json!` macro](https://docs.rs/serde_json/latest/serde_json/macro.json.html) — building values inline
- [`serde_json::Map`](https://docs.rs/serde_json/latest/serde_json/struct.Map.html) — the object type, including the `preserve_order` feature
- [`Value::pointer`](https://docs.rs/serde_json/latest/serde_json/enum.Value.html#method.pointer) — JSON Pointer (RFC 6901) access
- [serde_json README — "Operating on untyped JSON values"](https://github.com/serde-rs/json#operating-on-untyped-json-values)

### Related Sections in This Guide

- [Serde: The Big Picture](/15-serialization/00-serde-intro/) — how `JSON.parse`/`stringify` map onto Serde's data model
- [Serde Basics](/15-serialization/01-serde-basics/) — project setup, `to_string`/`from_str`
- [Deriving Serialize and Deserialize](/15-serialization/02-derive-serialize/) — what `#[derive(Serialize, Deserialize)]` generates
- [Structs and JSON](/15-serialization/03-json/) — the typed counterpart to this page (when you *do* know the shape)
- [Serde Attributes](/15-serialization/05-attributes/) — `rename`, `flatten`, `default`, and friends for typed models
- [Other Formats](/15-serialization/06-other-formats/) — TOML, YAML, MessagePack: the same dynamic value idea in other encodings
- [Custom Serialization](/15-serialization/07-custom-serialization/) — hand-written `Serialize`/`Deserialize`
- [Serde Performance](/15-serialization/08-performance/) — why to *avoid* `Value` on hot paths, and zero-copy alternatives
- [Web APIs and HTTP](/16-web-apis/) — handling dynamic JSON request/response bodies
- [Enums and Pattern Matching](/06-data-structures/) — `Value` is an enum; matching on it builds on these fundamentals
- [Basic Types](/02-basics/01-types/) — why Rust's exact `i64`/`u64` beats JavaScript's lossy `number`

---

## Exercises

### Exercise 1: Safe nested extraction

**Difficulty:** Easy

**Objective:** Navigate a dynamic `Value` without panicking.

**Instructions:** Write `fn first_tag(doc: &Value) -> Option<&str>` that returns the first element of the `"tags"` array as a string, or `None` if `"tags"` is missing, isn't an array, is empty, or its first element isn't a string. Use `get`, `as_array`/indexing, and `as_str` with the `?` operator — do not call `.unwrap()`.

```rust
use serde_json::{json, Value};

fn first_tag(doc: &Value) -> Option<&str> {
    // TODO: return the first tag as &str, or None
    todo!()
}

fn main() {
    let a = json!({ "tags": ["rust", "json"] });
    let b = json!({ "tags": [] });
    let c = json!({ "other": 1 });
    println!("{:?}", first_tag(&a)); // Some("rust")
    println!("{:?}", first_tag(&b)); // None
    println!("{:?}", first_tag(&c)); // None
}
```

<details>
<summary>Solution</summary>

```rust
use serde_json::{json, Value};

fn first_tag(doc: &Value) -> Option<&str> {
    // `get(0)` on a Value indexes the array; each ? bails to None on failure.
    doc.get("tags")?.get(0)?.as_str()
}

fn main() {
    let a = json!({ "tags": ["rust", "json"] });
    let b = json!({ "tags": [] });
    let c = json!({ "other": 1 });
    println!("{:?}", first_tag(&a)); // Some("rust")
    println!("{:?}", first_tag(&b)); // None
    println!("{:?}", first_tag(&c)); // None
}
```

`Value::get` accepts both `&str` keys and `usize` indices, so the whole path is one `?`-chained expression. Because every step returns `Option<&Value>`, a missing key, an empty array, or a non-string first element all collapse cleanly to `None`.

</details>

### Exercise 2: Sum numeric fields across an array

**Difficulty:** Medium

**Objective:** Iterate a dynamic JSON array and aggregate, tolerating missing/odd entries.

**Instructions:** Write `fn total_amount(orders: &Value) -> f64` that sums the `"amount"` field of every object in an `orders` array. Skip any element that lacks `"amount"` or whose `"amount"` isn't numeric. Return `0.0` if `orders` isn't an array. Use `as_array`, an iterator, `filter_map`, and `as_f64`.

```rust
use serde_json::{json, Value};

fn total_amount(orders: &Value) -> f64 {
    // TODO
    todo!()
}

fn main() {
    let orders = json!([
        { "id": 1, "amount": 9.99 },
        { "id": 2, "amount": 5.01 },
        { "id": 3 },                  // no amount -> skipped
        { "id": 4, "amount": "free" } // non-numeric -> skipped
    ]);
    println!("{}", total_amount(&orders)); // 15
    println!("{}", total_amount(&json!({}))); // 0
}
```

<details>
<summary>Solution</summary>

```rust
use serde_json::{json, Value};

fn total_amount(orders: &Value) -> f64 {
    orders
        .as_array()
        .map(|arr| {
            arr.iter()
                // filter_map keeps only entries with a numeric "amount".
                .filter_map(|o| o.get("amount").and_then(Value::as_f64))
                .sum()
        })
        .unwrap_or(0.0)
}

fn main() {
    let orders = json!([
        { "id": 1, "amount": 9.99 },
        { "id": 2, "amount": 5.01 },
        { "id": 3 },
        { "id": 4, "amount": "free" }
    ]);
    println!("{}", total_amount(&orders)); // 15
    println!("{}", total_amount(&json!({}))); // 0
}
```

`as_array()` yields `None` for non-arrays, so `unwrap_or(0.0)` covers the "not an array" case. Inside, `filter_map` discards every element where `get("amount")` is absent or `as_f64` returns `None`, so missing and non-numeric amounts are both skipped silently. (`15.0` prints as `15` because `f64`'s `Display` omits a trailing `.0`.)

</details>

### Exercise 3: Recursive redaction

**Difficulty:** Hard

**Objective:** Mutate a `Value` tree in place, recursing into nested objects and arrays.

**Instructions:** Write `fn redact(value: &mut Value, keys: &[&str])` that walks the entire tree and replaces the value of any object key listed in `keys` with the JSON string `"***"`. It must descend into nested objects and into arrays of objects. Keys not in the list keep their value but are still recursed into. Use `as_object_mut` / `as_array_mut` or a `match` over `&mut Value`.

```rust
use serde_json::{json, Value};

fn redact(value: &mut Value, keys: &[&str]) {
    // TODO: replace matching keys' values with json!("***"), recursively
    todo!()
}

fn main() {
    let mut payload = json!({
        "user": "ada",
        "password": "hunter2",
        "nested": { "token": "abc", "ok": true },
        "list": [ { "token": "xyz" }, { "safe": 1 } ]
    });
    redact(&mut payload, &["password", "token"]);
    println!("{}", serde_json::to_string(&payload).unwrap());
    // {"list":[{"token":"***"},{"safe":1}],"nested":{"ok":true,"token":"***"},
    //  "password":"***","user":"ada"}
}
```

<details>
<summary>Solution</summary>

```rust
use serde_json::{json, Value};

fn redact(value: &mut Value, keys: &[&str]) {
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if keys.contains(&k.as_str()) {
                    *v = json!("***"); // redact this key's value
                } else {
                    redact(v, keys);   // otherwise keep descending
                }
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                redact(v, keys);
            }
        }
        // Scalars (null/bool/number/string) are leaves: nothing to do.
        _ => {}
    }
}

fn main() {
    let mut payload = json!({
        "user": "ada",
        "password": "hunter2",
        "nested": { "token": "abc", "ok": true },
        "list": [ { "token": "xyz" }, { "safe": 1 } ]
    });
    redact(&mut payload, &["password", "token"]);
    println!("{}", serde_json::to_string(&payload).unwrap());
}
```

Matching on `&mut Value` gives mutable access to the inner `Map`/`Vec`, and `iter_mut()` lets us either overwrite `*v` for a matched key or recurse into it. Because matched keys are overwritten rather than recursed into, you avoid descending into data you're about to discard. Real output:

```text
{"list":[{"token":"***"},{"safe":1}],"nested":{"ok":true,"token":"***"},"password":"***","user":"ada"}
```

(Keys print sorted because the default `Map` is a `BTreeMap`; enable `serde_json`'s `preserve_order` feature for insertion order.)

</details>
