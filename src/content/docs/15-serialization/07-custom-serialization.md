---
title: "Custom Serialization: Hand-Written `Serialize` / `Deserialize`"
description: "When the wire format and your Rust type diverge, hand-write Serialize/Deserialize, retarget one field with serialize_with, or mirror foreign types via"
---

Most of the time `#[derive(Serialize, Deserialize)]` is all you need: the field names and shapes you write in Rust become the JSON you get out. But sometimes the wire format and your in-memory type are *deliberately different*: a color stored as three bytes but exchanged as `"#1a2b3c"`, money kept as integer cents but sent as `"49.99"`, or a type from a third-party crate that does not implement Serde at all. This page covers the three escape hatches for those cases: hand-writing the `Serialize`/`Deserialize` traits, retargeting a single field with `serialize_with`/`deserialize_with`, and `#[serde(remote = "...")]` for foreign types.

---

## Quick Overview

- **What it is:** Three levels of control over how a value maps to its serialized form, from "swap the codec for one field" up to "write the whole trait by hand."
- **Why it matters to a TypeScript/JavaScript developer:** This is the disciplined, compile-checked version of JavaScript's `toJSON()` method and the `JSON.parse` *reviver* callback: the tools you reach for when the JSON shape and the object shape diverge.
- **The mental shift:** In JavaScript, custom (de)serialization is a runtime callback that can return anything. In Rust it is a *trait implementation* the compiler verifies, and it stays **format-agnostic**: the same custom code drives JSON, YAML, MessagePack, and every other Serde format.

> **Note:** Reach for these tools only when the derive (covered in [Deriving `Serialize` and `Deserialize`](/15-serialization/02-derive-serialize/)) and the field attributes in [Serde Attributes](/15-serialization/05-attributes/) genuinely cannot express what you need. The `#[serde(with = ...)]`, `rename`, `default`, and `skip_serializing_if` attributes solve the common cases without any hand-written code.

---

## TypeScript/JavaScript Example

JavaScript gives you two runtime hooks for custom (de)serialization. On the way *out*, an object's `toJSON()` method controls what `JSON.stringify` emits. On the way *in*, a **reviver** callback passed to `JSON.parse` can rebuild rich objects from primitive JSON:

```typescript
// Custom serialization in TypeScript/JavaScript: toJSON + a reviver
class Color {
  constructor(public r: number, public g: number, public b: number) {}

  // Called automatically by JSON.stringify. We emit a hex string,
  // not the { r, g, b } object.
  toJSON(): string {
    const h = (n: number) => n.toString(16).padStart(2, "0");
    return `#${h(this.r)}${h(this.g)}${h(this.b)}`;
  }
}

const c = new Color(26, 43, 60);
console.log(JSON.stringify({ accent: c }));
// {"accent":"#1a2b3c"}

// On the way back, a reviver reconstructs the Color from the hex string.
const parsed = JSON.parse('{"accent":"#1a2b3c"}', (key, value) => {
  if (key === "accent" && typeof value === "string" && value.startsWith("#")) {
    const r = parseInt(value.slice(1, 3), 16);
    const g = parseInt(value.slice(3, 5), 16);
    const b = parseInt(value.slice(5, 7), 16);
    return new Color(r, g, b);
  }
  return value;
});

console.log(parsed.accent instanceof Color, parsed.accent);
// true Color { r: 26, g: 43, b: 60 }
```

This works, but notice the weaknesses Rust will close. `toJSON` is invoked by reflection on whatever object happens to be there — nothing checks its return type. The reviver is a stringly-typed `(key, value) => any` callback: you match keys by hand, the matching is easy to get subtly wrong, and a malformed `"#zz"` silently produces `NaN` channels rather than an error. There is also no symmetry: the encode logic lives in a class method and the decode logic lives in a separate callback, and nothing guarantees they agree.

---

## Rust Equivalent

In Rust you implement the `Serialize` and `Deserialize` traits directly. The two impls sit next to each other, both are type-checked, and a bad input becomes a real, recoverable error instead of a silent `NaN`:

```rust
// Cargo.toml:
//   [dependencies]
//   serde = { version = "1", features = ["derive"] }
//   serde_json = "1"
use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer};
use std::fmt;

struct Color {
    r: u8,
    g: u8,
    b: u8,
}

// Encode: RGB -> "#rrggbb". We talk to an abstract `Serializer`, not to JSON.
impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex = format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b);
        serializer.serialize_str(&hex)
    }
}

// Decode: "#rrggbb" -> RGB, via a Visitor (Serde's pull-based callback).
impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ColorVisitor;

        impl<'de> Visitor<'de> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a hex color string like \"#1a2b3c\"")
            }

            fn visit_str<E>(self, value: &str) -> Result<Color, E>
            where
                E: de::Error,
            {
                let s = value
                    .strip_prefix('#')
                    .ok_or_else(|| E::custom(format!("missing leading '#' in {value:?}")))?;
                if s.len() != 6 {
                    return Err(E::custom(format!("expected 6 hex digits, got {}", s.len())));
                }
                let parse = |range: std::ops::Range<usize>| {
                    u8::from_str_radix(&s[range], 16)
                        .map_err(|e| E::custom(format!("invalid hex: {e}")))
                };
                Ok(Color {
                    r: parse(0..2)?,
                    g: parse(2..4)?,
                    b: parse(4..6)?,
                })
            }
        }

        deserializer.deserialize_str(ColorVisitor)
    }
}

fn main() {
    let c = Color { r: 26, g: 43, b: 60 };
    let json = serde_json::to_string(&c).unwrap();
    println!("serialized: {json}");

    let back: Color = serde_json::from_str(&json).unwrap();
    println!("deserialized: r={} g={} b={}", back.r, back.g, back.b);

    // Bad inputs are real errors, not silent NaN.
    let bad: Result<Color, _> = serde_json::from_str(r#""1a2b3c""#);
    println!("bad input -> {:?}", bad.err().map(|e| e.to_string()));

    let bad2: Result<Color, _> = serde_json::from_str(r##""#zzzzzz""##);
    println!("bad hex   -> {:?}", bad2.err().map(|e| e.to_string()));
}
```

Real output:

```text
serialized: "#1a2b3c"
deserialized: r=26 g=43 b=60
bad input -> Some("missing leading '#' in \"1a2b3c\" at line 1 column 8")
bad hex   -> Some("invalid hex: invalid digit found in string at line 1 column 9")
```

The hex shape, the `#` prefix, and the digit-count check are all enforced on the way in. The closest JavaScript reviver would have happily returned a `Color` with `NaN` channels.

---

## Detailed Explanation

### The `Serialize` trait

`Serialize` has exactly one method:

```rust
// From the serde crate (shown for reference).
pub trait Serialize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer;
}
```

`serializer` is a value implementing the `Serializer` trait: `serde_json` provides one, `serde_yaml` another, and so on. Your job is to call methods on it describing your data: `serialize_str`, `serialize_u64`, `serialize_struct`, `serialize_seq`, and roughly two dozen more (one per type in the [Serde data model](https://serde.rs/data-model.html)). You never mention JSON; the `Serializer` decides what bytes those calls produce. That is why one hand-written `Serialize` impl works for *every* format — the same indirection the derive relies on (see [Serde](/15-serialization/00-serde-intro/) for the data-model architecture).

In the `Color` example, the entire encode is a single `serialize_str` call, because the *external* form is just a string. Internally `Color` is three `u8`s; externally it is one string. That asymmetry is the whole reason to hand-write it.

### The `Deserialize` trait and the Visitor pattern

Deserialization is the harder direction, and the reason is structural. A `Serializer` is *push-based*: you hand it values. A `Deserializer` is *pull-based and format-driven*: it reads tokens from the input and calls **you** back. That callback target is the `Visitor` trait.

The flow in the `Color` example:

1. `deserialize` calls `deserializer.deserialize_str(ColorVisitor)`. This is a **hint**: "I expect a string; here is who to call when you have one."
2. The deserializer (for JSON) reads a string token and calls `visitor.visit_str(self, "#1a2b3c")`.
3. `visit_str` does the parsing and either returns a `Color` or returns `E::custom(...)` for a bad input.

A `Visitor` can implement many `visit_*` methods (`visit_str`, `visit_u64`, `visit_map`, `visit_seq`, …). You only implement the ones you accept; the default for the rest is "produce a type error." The `expecting` method supplies the human-readable description that appears in those error messages.

> **Tip:** The lifetime `'de` is the lifetime of the data being borrowed *from* the input. You almost always just write `impl<'de> Deserialize<'de>` and `impl<'de> Visitor<'de>` verbatim and move on; it only becomes interesting for zero-copy borrowing, which [Serde Performance](/15-serialization/08-performance/) covers.

### Why a string `Visitor` is simpler than a struct one

The `Color` decode only handles `visit_str`. When your external form is a JSON *object* with several keys, you instead implement `visit_map`, walk the keys with `map.next_key()` / `map.next_value()`, and assemble the fields yourself. Here is that fuller pattern — a `Rectangle` that emits a *computed* `area` field on the way out (which the derive cannot do) and ignores it on the way in:

```rust
// Cargo.toml:
//   [dependencies]
//   serde = { version = "1", features = ["derive"] }
//   serde_json = "1"
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde::Deserialize;
use std::fmt;

#[derive(Debug)]
struct Rectangle {
    width: f64,
    height: f64,
}

impl Serialize for Rectangle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Announce a struct with THREE fields, including a computed one.
        let mut s = serializer.serialize_struct("Rectangle", 3)?;
        s.serialize_field("w", &self.width)?;
        s.serialize_field("h", &self.height)?;
        s.serialize_field("area", &(self.width * self.height))?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Rectangle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // A small derived enum gives us validated, fast key matching.
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            W,
            H,
            Area, // accepted on input but ignored (it is computed)
        }

        struct RectVisitor;

        impl<'de> Visitor<'de> for RectVisitor {
            type Value = Rectangle;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a rectangle object with `w` and `h`")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Rectangle, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut width = None;
                let mut height = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::W => {
                            if width.is_some() {
                                return Err(de::Error::duplicate_field("w"));
                            }
                            width = Some(map.next_value()?);
                        }
                        Field::H => {
                            if height.is_some() {
                                return Err(de::Error::duplicate_field("h"));
                            }
                            height = Some(map.next_value()?);
                        }
                        Field::Area => {
                            let _: f64 = map.next_value()?; // read and discard
                        }
                    }
                }
                let width = width.ok_or_else(|| de::Error::missing_field("w"))?;
                let height = height.ok_or_else(|| de::Error::missing_field("h"))?;
                Ok(Rectangle { width, height })
            }
        }

        const FIELDS: &[&str] = &["w", "h", "area"];
        deserializer.deserialize_struct("Rectangle", FIELDS, RectVisitor)
    }
}

fn main() {
    let r = Rectangle { width: 3.0, height: 4.0 };
    let json = serde_json::to_string(&r).unwrap();
    println!("{json}");

    let back: Rectangle = serde_json::from_str(&json).unwrap();
    println!("{back:?}");

    let bad: Result<Rectangle, _> = serde_json::from_str(r#"{"w":3.0}"#);
    println!("{:?}", bad.err().map(|e| e.to_string()));
}
```

Real output:

```text
{"w":3.0,"h":4.0,"area":12.0}
Rectangle { width: 3.0, height: 4.0 }
Some("missing field `h` at line 1 column 9")
```

This is exactly what `#[derive(Deserialize)]` generates internally; the derive writes this visitor for you. Seeing it once explains *why* deserialization "just works" with fields in any order and reports precise missing-field errors: there is a state machine doing it.

### `serialize_with` / `deserialize_with`: customize one field, keep the derive

Hand-writing the full traits for a whole struct just to special-case one field is overkill. The `serialize_with` and `deserialize_with` attributes let you keep `#[derive(Serialize, Deserialize)]` on the struct and point a single field at standalone functions. Those functions have the *same signatures* as the trait methods, but they are free functions, not impls:

```rust
// Cargo.toml:
//   [dependencies]
//   serde = { version = "1", features = ["derive"] }
//   serde_json = "1"
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// Stored as seconds; exchanged as milliseconds.
fn serialize_epoch_millis<S>(secs: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_u64(secs * 1000)
}

fn deserialize_epoch_millis<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let millis = u64::deserialize(deserializer)?;
    Ok(millis / 1000)
}

// Stored as Vec<String>; exchanged as one comma-separated string.
fn serialize_csv<S>(tags: &[String], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&tags.join(","))
}

fn deserialize_csv<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        return Ok(Vec::new());
    }
    Ok(s.split(',').map(|t| t.trim().to_string()).collect())
}

#[derive(Debug, Serialize, Deserialize)]
struct Event {
    name: String,
    #[serde(
        serialize_with = "serialize_epoch_millis",
        deserialize_with = "deserialize_epoch_millis"
    )]
    timestamp_secs: u64,
    #[serde(serialize_with = "serialize_csv", deserialize_with = "deserialize_csv")]
    tags: Vec<String>,
}

fn main() {
    let ev = Event {
        name: "login".into(),
        timestamp_secs: 1_700_000_000,
        tags: vec!["auth".into(), "web".into()],
    };

    let json = serde_json::to_string_pretty(&ev).unwrap();
    println!("{json}");

    let wire = r#"{"name":"logout","timestamp_secs":1700000123000,"tags":"auth,mobile"}"#;
    let back: Event = serde_json::from_str(wire).unwrap();
    println!("{back:?}");
}
```

Real output:

```text
{
  "name": "login",
  "timestamp_secs": 1700000000000,
  "tags": "auth,web"
}
Event { name: "logout", timestamp_secs: 1700000123, tags: ["auth", "mobile"] }
```

A few mechanics worth internalizing:

- The `serialize_with` function takes `&T` (a reference to the field), the `deserialize_with` function returns `Result<T, _>` (the field's type). The compiler enforces both.
- The argument is a string *path* to a function, e.g. `"serialize_epoch_millis"` or `"crate::codecs::serialize_csv"`. It is resolved at macro-expansion time, so a typo is a real "cannot find function" error, not a runtime miss.
- If you have *both* directions and they belong together, group them in a module with `serialize`/`deserialize` functions and use the single `#[serde(with = "module_path")]` attribute instead. That is the form the Real-World Example below uses, and the same form documented in [Serde Attributes](/15-serialization/05-attributes/).

### `#[serde(remote = "...")]`: serializing a foreign type

The hardest case is a type you do not own and cannot annotate: Rust's **orphan rule** (see [section 09](/09-generics-traits/12-orphan-rule/)) forbids implementing `Serialize` for a type from another crate. Serde's answer is **remote derive**: you declare a *local mirror* struct with identical fields, tie it to the foreign type with `#[serde(remote = "...")]`, and Serde generates the impls, but emits them as associated functions on your mirror, which you then attach to fields with `#[serde(with = "...")]`:

```rust
// Cargo.toml:
//   [dependencies]
//   serde = { version = "1", features = ["derive"] }
//   serde_json = "1"
use serde::{Deserialize, Serialize};

// Pretend this comes from a third-party crate we cannot edit; it has NO
// Serde derives. (In real code this would be `other_crate::Duration`.)
mod other_crate {
    #[derive(Debug)]
    pub struct Duration {
        pub secs: u64,
        pub nanos: u32,
    }

    impl Duration {
        pub fn new(secs: u64, nanos: u32) -> Self {
            Duration { secs, nanos }
        }
    }
}

use other_crate::Duration;

// A local mirror with the SAME fields, tied to the remote type.
// Serde generates Serialize/Deserialize *for Duration* from this definition.
#[derive(Serialize, Deserialize)]
#[serde(remote = "Duration")]
struct DurationDef {
    secs: u64,
    nanos: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct Timer {
    label: String,
    // Route this field through the generated remote impl.
    #[serde(with = "DurationDef")]
    interval: Duration,
}

fn main() {
    let timer = Timer {
        label: "heartbeat".into(),
        interval: Duration::new(30, 500_000),
    };

    let json = serde_json::to_string(&timer).unwrap();
    println!("{json}");

    let back: Timer = serde_json::from_str(&json).unwrap();
    println!("{back:?}");
}
```

Real output:

```text
{"label":"heartbeat","interval":{"secs":30,"nanos":500000}}
Timer { label: "heartbeat", interval: Duration { secs: 30, nanos: 500000 } }
```

The mirror's fields must match the remote type's fields (name and type) so the generated code can construct it. If the remote type has private fields, you supply a `#[serde(getter = "...")]` for serialization and the remote type must expose a constructor or `From` for deserialization. This is the mechanism crates like `chrono` use under the hood, though in practice you should prefer a crate's own `serde` feature (e.g. `chrono = { version = "0.4", features = ["serde"] }`) over rolling your own mirror; see [Beyond JSON](/15-serialization/06-other-formats/) and the ecosystem note in Best Practices.

---

## Key Differences

| Aspect | TypeScript / JavaScript | Rust + Serde |
| --- | --- | --- |
| Encode hook | `toJSON()` method, invoked by reflection | `Serialize` trait impl, resolved at compile time |
| Decode hook | `JSON.parse` reviver, a `(key, value) => any` callback | `Deserialize` trait + a `Visitor` state machine |
| Type checking | None — `toJSON` may return anything | Return types enforced by the compiler |
| Bad input | Often silent (`NaN`, `undefined`) | A real `Result::Err` with a precise message |
| Format coupling | Tied to JSON specifically | Format-agnostic; same impl drives every format |
| One-field tweak | Re-implement `toJSON` for the whole object | `#[serde(serialize_with/deserialize_with/with)]` on one field |
| Foreign types | Patch the prototype, or wrap | `#[serde(remote = "...")]` mirror struct |
| Encode/decode symmetry | Two unrelated places (method vs. callback) | Two traits, conventionally written side by side |

### Why Rust splits it into traits and a Visitor

JavaScript can afford a single `(key, value) => any` reviver because everything is dynamically typed and the JSON parser already produced generic objects. Rust has no runtime reflection and monomorphizes generics away, so the parser cannot "just build an object": it does not know the target shape. The `Visitor` is how the *type* tells the *format* what tokens it can accept, decoupling the two so that any `Deserializer` (JSON, YAML, binary) can drive any `Deserialize` type. It is more ceremony than a reviver, but it is the ceremony that makes the result fast and type-safe.

---

## Common Pitfalls

### Pitfall 1: Using a remote type on a field without `#[serde(with = ...)]`

Declaring the `#[serde(remote = "...")]` mirror is only half the job: it generates functions, it does **not** implement the trait *for* the remote type. You must still route each field through it. Forgetting that yields the standard "trait not implemented" error:

```rust
use serde::{Deserialize, Serialize};

mod other_crate {
    #[derive(Debug)]
    pub struct Duration {
        pub secs: u64,
        pub nanos: u32,
    }
}
use other_crate::Duration;

#[derive(Serialize, Deserialize)]
#[serde(remote = "Duration")]
struct DurationDef {
    secs: u64,
    nanos: u32,
}

#[derive(Serialize, Deserialize, Debug)] // does not compile (error0277)
struct Timer {
    label: String,
    interval: Duration, // forgot #[serde(with = "DurationDef")]
}

fn main() {
    let _ = Timer { label: "x".into(), interval: Duration { secs: 1, nanos: 0 } };
}
```

The real compiler error (`cargo build`):

```text
error[E0277]: the trait bound `other_crate::Duration: serde::Serialize` is not satisfied
    --> src/main.rs:19:10
     |
  19 | #[derive(Serialize, Deserialize, Debug)]
     |          ^^^^^^^^^ the trait `Serialize` is not implemented for `other_crate::Duration`
...
  22 |     interval: Duration, // forgot #[serde(with = "DurationDef")]
     |     -------- required by a bound introduced by this call
     |
     = note: for local types consider adding `#[derive(serde::Serialize)]` to your `other_crate::Duration` type
     = note: for types from other crates check whether the crate offers a `serde` feature flag
```

**Fix:** add `#[serde(with = "DurationDef")]` to the `interval` field. The compiler note's second line is also the better long-term advice: check whether the crate has a `serde` feature before writing a mirror at all.

### Pitfall 2: A `serialize_with` / `deserialize_with` function with the wrong signature

The attribute points at a string path, so a wrong *signature* is not caught until the macro-generated call site tries to use it. The two signatures are easy to confuse — `serialize_with` takes `&T`, `deserialize_with` returns `Result<T, _>`. Passing the field by value to `serialize_with`, returning the wrong type, or mixing up `Serializer`/`Deserializer` all produce mismatch errors at compile time. Copy a known-good pair (like the ones above) and edit the body; do not hand-type the generic bounds from memory.

> **Warning:** The function path is a string, so a *misspelled* name like `"serialize_epoc_millis"` produces a "cannot find function" error pointing at the `#[derive]` line, not the field; read past the derive to find the real cause.

### Pitfall 3: Mismatched field count in `serialize_struct`

`serializer.serialize_struct("Name", N)?` declares that exactly `N` fields follow. The count is an optimization hint that some formats (notably binary ones and `serde_json`'s map machinery) rely on. If you pass `2` but call `serialize_field` three times — or vice versa — formats that pre-allocate by the count can misbehave or panic. Always make the number match the number of `serialize_field` calls. The same applies to `serialize_seq(Some(len))` and `serialize_map(Some(len))`; pass `None` if you genuinely do not know the length up front.

### Pitfall 4: Forgetting `visit_str` (or implementing the wrong `visit_*`)

A `Visitor` only handles the `visit_*` methods you implement; every other input type falls through to a default that returns a type error. If your `deserialize` method hints `deserialize_str` but you implemented `visit_string` instead of `visit_str` (or omitted it entirely), valid input is rejected with an "invalid type" error. Match the `visit_*` method to the token the deserializer will actually produce: JSON strings arrive at `visit_str`, JSON numbers at `visit_u64`/`visit_i64`/`visit_f64`, JSON objects at `visit_map`, arrays at `visit_seq`.

### Pitfall 5: Trying to implement Serde traits on a foreign type directly

The instinct from other languages is to write `impl Serialize for chrono::DateTime<Utc>`. Rust's orphan rule rejects this: you may implement a trait for a type only if you own the trait *or* the type. The error is `E0117` ("only traits defined in the current crate can be implemented for types defined outside of the crate"). Remote derive exists precisely to work around this without violating the rule: the impl is generated for *your* mirror, and the `with` attribute wires it in. See [section 09's orphan rule page](/09-generics-traits/12-orphan-rule/) for the underlying reasoning.

---

## Best Practices

- **Prefer attributes over hand-written traits.** `#[serde(with = ...)]`, `rename`, `default`, and `skip_serializing_if` (in [Serde Attributes](/15-serialization/05-attributes/)) cover most divergence between wire and memory. Hand-write the full trait only when no attribute fits.
- **Prefer a crate's `serde` feature over remote derive.** Before mirroring a foreign type, check for a feature flag: `uuid = { version = "1", features = ["serde"] }`, `chrono = { version = "0.4", features = ["serde"] }`, `rust_decimal = { version = "1", features = ["serde"] }`. Remote derive is the fallback for crates that offer none.
- **Group a `with` pair into a module.** When a field needs both custom directions, put `serialize` and `deserialize` functions in a module and use `#[serde(with = "module")]` once, rather than two separate attributes. It keeps the encode/decode logic provably adjacent.
- **Always return `E::custom(...)` for bad input, never `panic!` or `unwrap`.** Deserialization is a fallible boundary; a hand-written `Deserialize` that panics turns malformed input into a crash. Return a descriptive error so the caller can handle it.
- **Write a round-trip test.** The single most valuable test for custom (de)serialization is `assert_eq!(value, from_str(&to_string(&value)))`. It catches encode/decode drift immediately. See [section 13](/13-testing/).
- **Keep `serialize_with`/`deserialize_with` functions pure and small.** They run inside the serializer; doing I/O or allocating heavily there is a smell. Transform data only.
- **Make `expecting` describe the *accepted* form, not the type name.** "a hex color string like `#1a2b3c`" produces far better error messages than "a Color".

---

## Real-World Example

A payment record exchanged with an external API. Three divergences from a naive derive: money is stored as integer **cents** (never floats — JavaScript's IEEE-754 `number` famously loses precision on money) but exchanged as a `"49.99"` decimal string; the upstream service sends statuses in `SCREAMING_SNAKE_CASE`; and the money codec lives in a reusable module wired in with a single `#[serde(with = "money")]`.

```rust
// Cargo.toml:
//   [dependencies]
//   serde = { version = "1", features = ["derive"] }
//   serde_json = "1"
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// A reusable codec module: cents <-> "NN.cc" decimal string.
mod money {
    use super::*;

    pub fn serialize<S>(cents: &i64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let sign = if *cents < 0 { "-" } else { "" };
        let abs = cents.unsigned_abs();
        let formatted = format!("{sign}{}.{:02}", abs / 100, abs % 100);
        serializer.serialize_str(&formatted)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let (sign, digits) = match s.strip_prefix('-') {
            Some(rest) => (-1, rest),
            None => (1, s.as_str()),
        };
        let (whole, frac) = digits.split_once('.').unwrap_or((digits, "0"));
        let whole: i64 = whole.parse().map_err(D::Error::custom)?;
        let frac_padded = format!("{frac:0<2}");
        let cents: i64 = frac_padded[..2].parse().map_err(D::Error::custom)?;
        Ok(sign * (whole * 100 + cents))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum PaymentStatus {
    Pending,
    Settled,
    Refunded,
}

#[derive(Debug, Serialize, Deserialize)]
struct Payment {
    id: String,
    #[serde(with = "money")]
    amount_cents: i64,
    currency: String,
    status: PaymentStatus,
}

fn main() {
    let payment = Payment {
        id: "pay_88".into(),
        amount_cents: 4999,
        currency: "USD".into(),
        status: PaymentStatus::Settled,
    };

    let json = serde_json::to_string_pretty(&payment).unwrap();
    println!("{json}");

    // Parse an inbound payload from the upstream service.
    let wire = r#"{"id":"pay_99","amount_cents":"12.05","currency":"EUR","status":"REFUNDED"}"#;
    let parsed: Payment = serde_json::from_str(wire).unwrap();
    println!("{parsed:?}");
    println!("stored cents = {}", parsed.amount_cents);
}
```

Real output:

```text
{
  "id": "pay_88",
  "amount_cents": "49.99",
  "currency": "USD",
  "status": "SETTLED"
}
Payment { id: "pay_99", amount_cents: 1205, currency: "EUR", status: Refunded }
stored cents = 1205
```

Internally `amount_cents` is always an exact integer — arithmetic on it cannot lose a penny — while the wire stays in the decimal string the external API expects. The `money` module is reusable across every type that carries money, and the `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` attribute handles the status casing without any custom code. This `Payment` is the kind of type a web handler would accept and return; see how typed payloads flow through HTTP in [section 16](/16-web-apis/).

---

## Further Reading

- [Implementing `Serialize`](https://serde.rs/impl-serialize.html): the official walkthrough of hand-writing the trait.
- [Implementing `Deserialize`](https://serde.rs/impl-deserialize.html): visitors, `MapAccess`/`SeqAccess`, and the field-identifier pattern.
- [Custom date format with `serialize_with` / `deserialize_with`](https://serde.rs/custom-date-format.html): the canonical `with`-module example.
- [Deriving `De/Serialize` for remote crate types](https://serde.rs/remote-derive.html): the full rules for `#[serde(remote = "...")]`, including `getter`.
- [`serde::de::Visitor`](https://docs.rs/serde/latest/serde/de/trait.Visitor.html) and [`serde::Serializer`](https://docs.rs/serde/latest/serde/trait.Serializer.html): the method-by-method API reference.
- This guide:
  - [Serde](/15-serialization/00-serde-intro/) — the trait architecture and the data model these impls target.
  - [Deriving `Serialize` and `Deserialize`](/15-serialization/02-derive-serialize/) — what `#[derive(...)]` generates (this page is the manual version).
  - [Serde Attributes](/15-serialization/05-attributes/) — `with`, `rename`, `default`, and the attributes that avoid hand-written code.
  - [Structs and JSON](/15-serialization/03-json/) — struct/enum-to-JSON shapes you may need to reproduce by hand.
  - [Beyond JSON](/15-serialization/06-other-formats/) — why a format-agnostic impl works for TOML, YAML, and MessagePack.
  - [Serde Performance](/15-serialization/08-performance/) — zero-copy `Visitor`s and `#[serde(borrow)]`.
  - [Section 09: Generics & Traits](/09-generics-traits/) and the [orphan rule](/09-generics-traits/12-orphan-rule/) — why remote derive exists.
  - [Section 13: Testing](/13-testing/) — round-trip tests for custom codecs.
  - [Section 16: Web APIs](/16-web-apis/) — typed payloads across HTTP.

---

## Exercises

### Exercise 1: A custom `Serialize` for a newtype

**Difficulty:** Easy

**Objective:** Hand-write the `Serialize` trait for a type whose external form differs from its internal one.

**Instructions:**

1. Define a tuple struct `Percentage(u8)`.
2. Implement `Serialize` so a `Percentage(75)` serializes to the JSON string `"75%"` (with a literal percent sign), not the number `75`.
3. In `main`, serialize `Percentage(75)` and print the result.

```rust
use serde::ser::{Serialize, Serializer};

struct Percentage(u8);

impl Serialize for Percentage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // TODO: serialize as the string "75%"
        todo!()
    }
}

fn main() {
    // TODO: serialize Percentage(75) and print it
}
```

<details>
<summary>Solution</summary>

```rust
use serde::ser::{Serialize, Serializer};

struct Percentage(u8);

impl Serialize for Percentage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}%", self.0))
    }
}

fn main() {
    let p = Percentage(75);
    println!("{}", serde_json::to_string(&p).unwrap());
    // "75%"
}
```

Real output:

```text
"75%"
```

</details>

### Exercise 2: A lenient `deserialize_with`

**Difficulty:** Medium

**Objective:** Use `deserialize_with` to accept input in more than one shape: the kind of leniency real-world APIs demand.

**Instructions:**

1. Write a free function `flexible_bool` suitable for `#[serde(deserialize_with = "...")]` that accepts either a real JSON boolean *or* a string. The strings `"yes"`, `"true"`, `"1"` mean `true`; `"no"`, `"false"`, `"0"` mean `false` (case-insensitive). Anything else is an error.
2. Put it on a `Settings { dark_mode: bool }` struct.
3. In `main`, deserialize `{"dark_mode":true}`, `{"dark_mode":"yes"}`, and `{"dark_mode":"NO"}`, then show that `{"dark_mode":"maybe"}` produces an error.

> **Tip:** A small `#[serde(untagged)]` enum is the cleanest way to accept "a bool *or* a string."

<details>
<summary>Solution</summary>

```rust
use serde::de::Error;
use serde::{Deserialize, Deserializer};

fn flexible_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrString {
        Bool(bool),
        Str(String),
    }

    match BoolOrString::deserialize(deserializer)? {
        BoolOrString::Bool(b) => Ok(b),
        BoolOrString::Str(s) => match s.to_lowercase().as_str() {
            "yes" | "true" | "1" => Ok(true),
            "no" | "false" | "0" => Ok(false),
            other => Err(D::Error::custom(format!("not a boolean: {other:?}"))),
        },
    }
}

#[derive(Debug, Deserialize)]
struct Settings {
    #[serde(deserialize_with = "flexible_bool")]
    dark_mode: bool,
}

fn main() {
    for input in [
        r#"{"dark_mode":true}"#,
        r#"{"dark_mode":"yes"}"#,
        r#"{"dark_mode":"NO"}"#,
    ] {
        let s: Settings = serde_json::from_str(input).unwrap();
        println!("{input} -> {s:?}");
    }
    let bad: Result<Settings, _> = serde_json::from_str(r#"{"dark_mode":"maybe"}"#);
    println!("{:?}", bad.err().map(|e| e.to_string()));
}
```

Real output:

```text
{"dark_mode":true} -> Settings { dark_mode: true }
{"dark_mode":"yes"} -> Settings { dark_mode: true }
{"dark_mode":"NO"} -> Settings { dark_mode: false }
Some("not a boolean: \"maybe\" at line 1 column 21")
```

</details>

### Exercise 3: A full round-tripping `Serialize` + `Deserialize`

**Difficulty:** Hard

**Objective:** Implement *both* traits by hand for a type that serializes as a single string and validates strictly on the way back.

**Instructions:**

1. Define `struct Version { major: u16, minor: u16, patch: u16 }` deriving `Debug` and `PartialEq`.
2. Implement `Serialize` so it produces the string `"MAJOR.MINOR.PATCH"` (e.g. `"1.4.2"`).
3. Implement `Deserialize` with a `Visitor` whose `visit_str` parses exactly three dot-separated `u16`s. Reject too-few components, non-numeric components, *and* too-many components, each with a descriptive `E::custom` error.
4. In `main`, round-trip a `Version`, `assert_eq!` it survives, and show that `"1.x"` fails with a useful message.

<details>
<summary>Solution</summary>

```rust
use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer};
use std::fmt;

#[derive(Debug, PartialEq)]
struct Version {
    major: u16,
    minor: u16,
    patch: u16,
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}.{}.{}", self.major, self.minor, self.patch))
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VersionVisitor;

        impl<'de> Visitor<'de> for VersionVisitor {
            type Value = Version;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str(r#"a version string like "1.4.2""#)
            }

            fn visit_str<E>(self, v: &str) -> Result<Version, E>
            where
                E: de::Error,
            {
                let mut parts = v.split('.');
                let mut next = |name: &str| -> Result<u16, E> {
                    parts
                        .next()
                        .ok_or_else(|| E::custom(format!("missing {name}")))?
                        .parse()
                        .map_err(|e| E::custom(format!("bad {name}: {e}")))
                };
                let major = next("major")?;
                let minor = next("minor")?;
                let patch = next("patch")?;
                if parts.next().is_some() {
                    return Err(E::custom("too many version components"));
                }
                Ok(Version { major, minor, patch })
            }
        }

        deserializer.deserialize_str(VersionVisitor)
    }
}

fn main() {
    let v = Version { major: 1, minor: 4, patch: 2 };
    let json = serde_json::to_string(&v).unwrap();
    println!("{json}");

    let back: Version = serde_json::from_str(&json).unwrap();
    println!("{back:?}");
    assert_eq!(v, back);

    let bad: Result<Version, _> = serde_json::from_str(r#""1.x""#);
    println!("{:?}", bad.err().map(|e| e.to_string()));
}
```

Real output:

```text
"1.4.2"
Version { major: 1, minor: 4, patch: 2 }
Some("bad minor: invalid digit found in string at line 1 column 5")
```

</details>
