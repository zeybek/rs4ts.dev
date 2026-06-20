---
title: "Serde Attributes"
description: "Serde's #[serde(...)] attributes rename keys, set casing, skip secrets, supply defaults, flatten, and tag enums, replacing TypeScript's ad-hoc toJSON and ??"
---

Serde's `#[serde(...)]` attributes are how you bend a Rust type to match a real-world JSON contract without hand-writing any (de)serialization code. They are the bridge between idiomatic Rust naming (`snake_case`, `Option`, enums) and the messy keys, casing, and shapes that external APIs actually use.

---

## Quick Overview

When you `#[derive(Serialize, Deserialize)]`, the generated code maps each field one-to-one to a key with the same name. **Serde attributes** let you override that mapping: rename keys, change casing, hide fields, supply defaults, inline nested structs, and control how enums are tagged. If you have ever reached for a custom `toJSON()` method, a `class-transformer` decorator, or a Zod `.transform()` in TypeScript, attributes are Serde's far more powerful equivalent: applied declaratively, checked at compile time, and shared by both directions (serialize and deserialize).

---

## TypeScript/JavaScript Example

In TypeScript there is no single built-in mechanism for this. You typically combine several ad-hoc techniques: a custom `toJSON()`, manual `??` defaults on parse, and hand-written discriminated-union narrowing.

```typescript
// TypeScript: matching a camelCase JSON API with hand-written glue.

interface ApiUser {
  userId: number;
  firstName: string;
  emailAddress: string; // wire name differs from our internal "email"
}

class Account {
  constructor(
    public username: string,
    private passwordHash: string, // must NEVER be serialized
    public nickname: string | null,
  ) {}

  // The closest JS analogue to Serde attributes: a custom toJSON().
  toJSON() {
    const out: Record<string, unknown> = { username: this.username };
    // skip_serializing_if: omit nickname when absent
    if (this.nickname != null) out.nickname = this.nickname;
    // passwordHash is simply never added -> "skip"
    return out;
  }
}

// "default" on the way in is manual with ?? :
function parseConfig(raw: any) {
  return {
    host: raw.host as string,
    port: (raw.port ?? 8080) as number, // default = 8080
    tlsEnabled: (raw.tlsEnabled ?? false) as boolean,
  };
}

// Discriminated union ("internally tagged" enum) checked by hand:
type Shape =
  | { type: "circle"; radius: number }
  | { type: "rectangle"; width: number; height: number };

console.log(JSON.stringify(new Account("ada", "secret", null)));
// {"username":"ada"}
console.log(parseConfig({ host: "localhost" }));
// { host: 'localhost', port: 8080, tlsEnabled: false }
```

Every concern — renaming, casing, skipping, defaults, tagging — is solved with a *different* mechanism, and none of them are type-checked against the actual JSON.

---

## Rust Equivalent

In Rust, every one of those concerns is a declarative attribute on the type, and the same declaration governs both directions.

```rust playground
use serde::{Deserialize, Serialize};

// rename_all renames EVERY field; rename overrides a single one.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    user_id: u64,                  // -> "userId"
    first_name: String,            // -> "firstName"
    last_name: String,             // -> "lastName"
    #[serde(rename = "emailAddress")]
    email: String,                 // -> "emailAddress" (overrides camelCase)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    username: String,
    #[serde(skip)]                 // never serialized, never deserialized
    password_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    nickname: Option<String>,      // omitted from output when None
    #[serde(default)]              // missing input -> Default::default()
    is_active: bool,
    #[serde(default = "default_role")]
    role: String,                  // missing input -> default_role()
}

fn default_role() -> String {
    "member".to_string()
}

// Internally tagged enum == a TypeScript discriminated union.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Shape {
    Circle { radius: f64 },        // {"type":"circle","radius":...}
    Rectangle { width: f64, height: f64 },
}

fn main() {
    let account = Account {
        username: "ada".into(),
        password_hash: "secret-hash".into(),
        nickname: None,
        is_active: true,
        role: "admin".into(),
    };
    println!("{}", serde_json::to_string(&account).unwrap());
    // {"username":"ada","isActive":true,"role":"admin"}

    let shape = Shape::Circle { radius: 2.5 };
    println!("{}", serde_json::to_string(&shape).unwrap());
    // {"type":"circle","radius":2.5}
}
```

> **Note:** This assumes a project set up per [Serde Basics](/15-serialization/01-serde-basics/): `serde = { version = "1", features = ["derive"] }` and `serde_json = "1"` in `Cargo.toml`. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically.

---

## Detailed Explanation

Attributes come in two scopes. A **container attribute** sits on the `struct`/`enum` itself and affects the whole type (`#[serde(rename_all = ...)]`, `#[serde(tag = ...)]`). A **field attribute** sits on a single field and affects only that field (`#[serde(rename = ...)]`, `#[serde(skip)]`, `#[serde(default)]`). Variant attributes sit on enum variants. The same attributes drive both `Serialize` and `Deserialize` unless you scope them with `_serializing`/`_deserializing` suffixes.

### `rename` and `rename_all`

`rename` changes the key for one field. `rename_all` applies a casing convention to *every* field (or every variant) in the container. The accepted casings are `"lowercase"`, `"UPPERCASE"`, `"PascalCase"`, `"camelCase"`, `"snake_case"`, `"SCREAMING_SNAKE_CASE"`, `"kebab-case"`, and `"SCREAMING-KEBAB-CASE"`. A field-level `rename` always wins over the container-level `rename_all`, which is exactly what the `email -> "emailAddress"` case above demonstrates.

This is the single most common attribute, because Rust style is `snake_case` while most JSON APIs are `camelCase`. Without `rename_all` you would otherwise repeat `#[serde(rename = "...")]` on every field.

> **Tip:** You can split serialize and deserialize names with `#[serde(rename(serialize = "out", deserialize = "in"))]` when an API reads one key but writes another.

### `skip`, `skip_serializing`, `skip_deserializing`, and `skip_serializing_if`

- `#[serde(skip)]` removes the field from *both* directions. On deserialize the field is filled with `Default::default()`, so the field's type must implement `Default` (or you must also provide `default = "..."`).
- `#[serde(skip_serializing)]` / `#[serde(skip_deserializing)]` skip only one direction.
- `#[serde(skip_serializing_if = "path")]` skips the field on output only when the named predicate returns `true`. The value is a **string path to a function** taking `&FieldType -> bool`, most commonly `"Option::is_none"`, `"Vec::is_empty"`, `"str::is_empty"`, or `"<[_]>::is_empty"`.

In the `Account` example, `password_hash` vanishes entirely (a secret that must never hit the wire), while `nickname` only disappears when it is `None`. That is why the serialized output is `{"username":"ada","isActive":true,"role":"admin"}`: both `password_hash` and the `None` nickname are gone.

### `default` and `default = "path"`

When the input JSON is missing a field, deserialization normally fails. `#[serde(default)]` instead fills it with `Default::default()` for that type. `#[serde(default = "path")]` calls the named function (with signature `fn() -> FieldType`) to produce the value. This is the declarative equivalent of `raw.port ?? 8080`. Deserializing `{ "username": "grace" }` into the `Account` above succeeds and produces:

```text
Account { username: "grace", password_hash: "", nickname: None, is_active: false, role: "member" }
```

`password_hash` is `""` (skip's `Default`), `is_active` is `false` (`bool::default()`), and `role` is `"member"` (our `default_role()`).

### `flatten`

`#[serde(flatten)]` inlines the keys of a nested struct (or a map) into the parent object instead of nesting them. It serves two distinct purposes:

1. **Composition**: share a common block (pagination, metadata) across many response types without nesting it.
2. **Capture** — flatten a `HashMap<String, Value>` to absorb any extra, unknown keys.

```rust playground
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct Pagination {
    page: u32,
    per_page: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserListResponse {
    users: Vec<String>,
    #[serde(flatten)]
    pagination: Pagination,
}

#[derive(Debug, Serialize, Deserialize)]
struct Event {
    name: String,
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

fn main() {
    let resp = UserListResponse {
        users: vec!["ada".into(), "grace".into()],
        pagination: Pagination { page: 1, per_page: 20 },
    };
    println!("{}", serde_json::to_string(&resp).unwrap());
    // {"users":["ada","grace"],"page":1,"per_page":20}

    let event: Event =
        serde_json::from_str(r#"{ "name": "click", "x": 10, "y": 20, "button": "left" }"#).unwrap();
    println!("{event:?}");
    // Event { name: "click", extra: {"button": String("left"), "y": Number(20), "x": Number(10)} }
}
```

Notice `page` and `per_page` appear at the top level alongside `users`, not nested under a `"pagination"` key. And `Event.extra` swept up `x`, `y`, and `button` even though they were not declared fields. (Map ordering in the `extra` output is not stable — `HashMap` is unordered.)

### `tag`, `content`, and `untagged` (enum representations)

By default, an externally tagged enum serializes as `{"VariantName": <data>}`. The container attributes change that representation:

| Attribute | Representation | JSON for `Circle { radius: 2.5 }` |
| --- | --- | --- |
| *(none)* | externally tagged | `{"Circle":{"radius":2.5}}` |
| `#[serde(tag = "type")]` | internally tagged | `{"type":"Circle","radius":2.5}` |
| `#[serde(tag = "k", content = "c")]` | adjacently tagged | `{"k":"Circle","c":{"radius":2.5}}` |
| `#[serde(untagged)]` | untagged | `{"radius":2.5}` |

**Internally tagged** (`tag = "type"`) is the direct analogue of a TypeScript discriminated union and the most common choice for APIs. **Adjacently tagged** keeps the tag and payload in separate, named keys. **Untagged** has no discriminant at all: Serde tries each variant in declaration order and keeps the first that deserializes successfully.

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
enum Message {
    Text(String),
    Move { x: i32, y: i32 },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum StringOrNumber {
    Number(i64),
    Text(String),
}

fn main() {
    println!("{}", serde_json::to_string(&Message::Move { x: 1, y: 2 }).unwrap());
    // {"kind":"move","data":{"x":1,"y":2}}

    let a: StringOrNumber = serde_json::from_str("42").unwrap();
    let b: StringOrNumber = serde_json::from_str(r#""hello""#).unwrap();
    println!("{a:?} {b:?}");
    // Number(42) Text("hello")
}
```

> **Note:** Enum representations are covered more deeply in [Structs and JSON](/15-serialization/03-json/). This file focuses on the `tag`/`content`/`untagged` *attributes* that select them.

### `with`, `serialize_with`, and `deserialize_with`

When a field's natural Rust type does not match its wire format — a `u64` timestamp stored as a JSON string, a date in a custom format, bytes as base64 — `#[serde(with = "module")]` delegates that one field to a module that provides `serialize` and `deserialize` functions. `serialize_with`/`deserialize_with` do the same with a single function each, when you only need one direction or prefer not to write a module.

```rust playground
use serde::{Deserialize, Serialize};

// A module exposing `serialize` and `deserialize` for a u64 stored as a
// JSON string (some APIs send 64-bit integers as strings to dodge the
// JavaScript number-precision problem).
mod epoch_seconds {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(secs: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&secs.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<u64>().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LogEntry {
    message: String,
    #[serde(with = "epoch_seconds")]
    timestamp: u64,
}

fn main() {
    let entry = LogEntry { message: "started".into(), timestamp: 1717200000 };
    println!("{}", serde_json::to_string(&entry).unwrap());
    // {"message":"started","timestamp":"1717200000"}

    let back: LogEntry =
        serde_json::from_str(r#"{"message":"ok","timestamp":"1717200001"}"#).unwrap();
    println!("{back:?}");
    // LogEntry { message: "ok", timestamp: 1717200001 }
}
```

The `timestamp` is a real `u64` in Rust but a quoted string on the wire. The `with` module is the seam. The hand-written-trait version of this technique, plus `remote` derive, lives in [Custom Serialization](/15-serialization/07-custom-serialization/).

> **Tip:** This precision issue is a real cross-language trap. JavaScript's `number` is always an IEEE-754 `f64`, so any integer above `2^53 - 1` (`Number.MAX_SAFE_INTEGER`) silently loses precision. It does *not* wrap. Sending big 64-bit IDs as JSON strings, then mapping them with `with`, sidesteps that entirely.

---

## Key Differences

| Concern | TypeScript/JavaScript | Rust + Serde |
| --- | --- | --- |
| Rename a key | Manual in `toJSON()` / `class-transformer` `@Expose({name})` | `#[serde(rename = "...")]` |
| Bulk casing | Library config (e.g. `camelcase-keys`) | `#[serde(rename_all = "camelCase")]` |
| Hide a field | `toJSON()` omits it; nothing enforces it | `#[serde(skip)]`, compile-checked |
| Omit if empty | `if (x != null) out.x = x` | `#[serde(skip_serializing_if = "Option::is_none")]` |
| Default on parse | `value ?? fallback` | `#[serde(default)]` / `default = "fn"` |
| Inline nested object | Spread `{...base, ...extra}` | `#[serde(flatten)]` |
| Discriminated union | `type` literal property, narrowed by hand | `#[serde(tag = "type")]` enum |
| Custom field codec | Custom getter/setter or transformer | `#[serde(with = "module")]` |
| Symmetry | Serialize and parse are separate code paths | One attribute drives both directions |

The deepest conceptual difference is **symmetry and verification**. In TypeScript, `JSON.stringify`/`JSON.parse` are unaware of your types; a `toJSON()` method shapes output but nothing checks that `JSON.parse` produces the inverse, and the casts (`as User`) are erased at runtime. In Rust, a single set of attributes generates *both* directions of compile-checked code at build time (monomorphized, not reflective), so an output you produce can be parsed back, and a missing or mistyped attribute is a build error, not a 2 a.m. runtime surprise.

A second difference: TypeScript generics and interfaces are erased at runtime, so a "type" is only a compile-time fiction during (de)serialization. Serde generates concrete code per type, which is why attributes like `skip` can enforce that a secret never serializes — there is real code, not a hopeful annotation.

---

## Common Pitfalls

### Pitfall 1: `skip` on a type that is not `Default`

`#[serde(skip)]` needs *some* value to put in the field when deserializing. By default it calls `Default::default()`, so the field's type must implement `Default`.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Connection {
    host: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Server {
    name: String,
    #[serde(skip)] // does not compile (error[E0277]: Connection: Default not satisfied)
    connection: Connection,
}

fn main() {
    let _s: Server = serde_json::from_str(r#"{ "name": "web-1" }"#).unwrap();
}
```

The real compiler error is:

```text
error[E0277]: the trait bound `Connection: Default` is not satisfied
 --> src/main.rs:8:28
  |
8 | #[derive(Debug, Serialize, Deserialize)]
  |                            ^^^^^^^^^^^ the trait `Default` is not implemented for `Connection`
  |
  = note: this error originates in the derive macro `Deserialize` (in Nightly builds, run with -Z macro-backtrace for more info)
help: consider annotating `Connection` with `#[derive(Default)]`
  |
4 + #[derive(Default)]
5 | struct Connection {
```

**Fix:** add `#[derive(Default)]` to `Connection`, or pair the skip with an explicit factory: `#[serde(skip, default = "make_connection")]`.

### Pitfall 2: `skip_serializing_if` value must be a string path

The predicate is a string literal naming a function, not a bare expression or closure. Writing it without quotes is a hard error.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Item {
    name: String,
    #[serde(skip_serializing_if = Vec::is_empty)] // does not compile (must be a string)
    tags: Vec<String>,
}

fn main() {
    let item = Item { name: "x".into(), tags: vec![] };
    println!("{}", serde_json::to_string(&item).unwrap());
}
```

The real error is:

```text
error: expected serde skip_serializing_if attribute to be a string: `skip_serializing_if = "..."`
 --> src/main.rs:6:35
  |
6 |     #[serde(skip_serializing_if = Vec::is_empty)]
  |                                   ^^^^^^^^^^^^^
```

**Fix:** quote it — `#[serde(skip_serializing_if = "Vec::is_empty")]`.

### Pitfall 3: Internally tagged enums and newtype primitives

An internally tagged enum (`#[serde(tag = "...")]`) injects the tag key *into the variant's object*. A newtype variant wrapping a primitive (like `i64`) has no object to inject into, so it compiles but fails at **runtime**.

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum Token {
    Number(i64),        // a primitive newtype variant
    Word { text: String },
}

fn main() {
    let t = Token::Number(42);
    match serde_json::to_string(&t) {
        Ok(s) => println!("ok: {s}"),
        Err(e) => println!("error: {e}"),
    }
}
```

This prints the genuine runtime error message:

```text
error: cannot serialize tagged newtype variant Token::Number containing an integer
```

**Fix:** internally tagged enums require struct-like or unit variants (or newtype variants wrapping a struct/map). Use a struct variant — `Number { value: i64 }` — or switch to an adjacently tagged (`tag` + `content`) or untagged representation.

### Pitfall 4: `default` does not rescue an explicit `null`

`#[serde(default)]` fills a value only when the key is **absent**. A present `null` is a real value and is deserialized as such. For a non-`Option` field, `"role": null` will error rather than fall back to the default. `default` is about missing keys, not null ones. To accept either, use `Option<T>` (or combine `default` with a `deserialize_with` that maps null to the default).

### Pitfall 5: Two flattened fields claiming the same key

If you flatten two structs that share a key (e.g. both expose `id`), or flatten a map alongside an explicit field of the same name, behavior is ambiguous and the round trip silently misbehaves. Keep flattened key namespaces disjoint, and reserve a flattened catch-all `HashMap` strictly for *unknown* extras.

---

## Best Practices

- **Set `rename_all` once at the container level** rather than renaming each field. Reach for field-level `rename` only for the genuine exceptions.
- **Use `skip` for true secrets** (password hashes, internal tokens). Unlike a TypeScript `toJSON()` omission, this is enforced by the compiler: the field has no serialization code at all.
- **Prefer `skip_serializing_if = "Option::is_none"` over emitting `null`.** Most APIs treat "absent" and "null" differently; omitting keeps payloads small and intentions clear.
- **Pair `default` with `Option`/`Vec`/`bool` for forgiving deserialization** of optional config: it makes adding new fields backward-compatible.
- **Use internally tagged enums (`tag = "type"`) to mirror TypeScript discriminated unions.** It is the most ergonomic and the most familiar to API consumers.
- **Add `#[serde(deny_unknown_fields)]` on strict inputs** (config files, internal RPC) to catch typos; leave it off and use a flattened `HashMap` when you must tolerate forward-compatible extras.
- **Reach for `with` only when the wire type genuinely differs** from the Rust type (timestamps-as-strings, base64 bytes). For mechanical casing/renaming, the simpler attributes are enough.
- **Keep the same attribute set on both sides of a round trip.** Because one declaration drives serialize and deserialize, asymmetric `_serializing`/`_deserializing` attributes should be a deliberate, documented choice.

---

## Real-World Example

A typical API resource: a Rust-idiomatic struct mapped to a `camelCase` JSON contract, hiding internal fields, omitting empty optionals, filling defaults for forward compatibility, and carrying a polymorphic status as an internally tagged enum.

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiUser {
    id: u64,
    display_name: String,

    // The wire contract uses "email_address" regardless of camelCase.
    #[serde(rename = "email_address")]
    email: String,

    // Internal-only; never crosses the wire.
    #[serde(skip)]
    internal_notes: String,

    // Omit entirely when there is no avatar, rather than sending null.
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,

    // Missing in the payload? Fall back to a sensible value.
    #[serde(default = "default_locale")]
    locale: String,

    // Missing array? Treat it as empty.
    #[serde(default)]
    roles: Vec<String>,

    // Polymorphic, self-describing status.
    status: AccountStatus,
}

fn default_locale() -> String {
    "en-US".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum AccountStatus {
    Active,
    Suspended { reason: String },
    PendingReview { since: String },
}

fn main() -> Result<(), serde_json::Error> {
    let user = ApiUser {
        id: 1001,
        display_name: "Grace Hopper".into(),
        email: "grace@example.com".into(),
        internal_notes: "VIP - do not expose".into(),
        avatar_url: None,
        locale: "en-US".into(),
        roles: vec!["admin".into(), "auditor".into()],
        status: AccountStatus::Suspended { reason: "policy violation".into() },
    };
    println!("--- outgoing response ---\n{}", serde_json::to_string_pretty(&user)?);

    // A minimal request: missing locale, roles, and avatar.
    let incoming = r#"{
        "id": 2002,
        "displayName": "Alan Turing",
        "email_address": "alan@example.com",
        "status": { "state": "active" }
    }"#;
    let parsed: ApiUser = serde_json::from_str(incoming)?;
    println!("\n--- parsed request ---\n{parsed:?}");

    Ok(())
}
```

Running it prints the real output below. Note that `internalNotes` is absent (skipped), `avatarUrl` is absent (`None`), `email_address` keeps its overridden name, and the parsed request received `locale: "en-US"`, `roles: []`, and `internal_notes: ""` from defaults:

```text
--- outgoing response ---
{
  "id": 1001,
  "displayName": "Grace Hopper",
  "email_address": "grace@example.com",
  "locale": "en-US",
  "roles": [
    "admin",
    "auditor"
  ],
  "status": {
    "state": "suspended",
    "reason": "policy violation"
  }
}

--- parsed request ---
ApiUser { id: 2002, display_name: "Alan Turing", email: "alan@example.com", internal_notes: "", avatar_url: None, locale: "en-US", roles: [], status: Active }
```

This is the kind of struct you would hand to a web framework like Axum to (de)serialize request and response bodies automatically — see [Web APIs](/16-web-apis/).

---

## Further Reading

### Official Documentation

- [Serde attributes overview](https://serde.rs/attributes.html)
- [Container attributes](https://serde.rs/container-attrs.html) (including `rename_all`, `tag`, `content`, `untagged`, `deny_unknown_fields`)
- [Field attributes](https://serde.rs/field-attrs.html) (`rename`, `skip`, `skip_serializing_if`, `default`, `flatten`, `with`)
- [Variant attributes](https://serde.rs/variant-attrs.html)
- [Enum representations](https://serde.rs/enum-representations.html)

### Related Sections in This Guide

- [Serde Introduction](/15-serialization/00-serde-intro/): the data model and the `Serialize`/`Deserialize` traits
- [Serde Basics](/15-serialization/01-serde-basics/): project setup and `to_string`/`from_str`
- [Derive Serialize/Deserialize](/15-serialization/02-derive-serialize/) — what the derive macro generates
- [Structs and JSON](/15-serialization/03-json/) — nested types, `Option`, and enum representations in depth
- [Dynamic JSON](/15-serialization/04-json-manipulation/) — `serde_json::Value` and the `json!` macro (the type behind a flattened catch-all map)
- [Custom Serialization](/15-serialization/07-custom-serialization/) — hand-written impls, `serialize_with`/`deserialize_with`, and remote derive
- [Other Formats](/15-serialization/06-other-formats/) — the same attributes applied to TOML, YAML, and MessagePack
- [Performance](/15-serialization/08-performance/) — borrowing and zero-copy considerations
- Foundations: [enums and structs](/06-data-structures/), [`Option`/`Result`](/08-error-handling/), and [basic types](/02-basics/)

---

## Exercises

### Exercise 1: Match a camelCase product API

**Difficulty:** Easy

**Objective:** Use `rename_all` and `skip_serializing_if` to match a JSON contract.

**Instructions:** Define a `Product` struct with `product_id: u32`, `display_name: String`, and `discount_code: Option<String>`. The JSON API uses `camelCase` keys, and the discount code must be omitted entirely when there is none. Serialize a product with no discount code and confirm `discountCode` is absent.

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Product {
    product_id: u32,
    display_name: String,
    discount_code: Option<String>, // TODO: add attributes
}

fn main() {
    let p = Product {
        product_id: 9,
        display_name: "Widget".into(),
        discount_code: None,
    };
    println!("{}", serde_json::to_string(&p).unwrap());
    // target: {"productId":9,"displayName":"Widget"}
}
```

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Product {
    product_id: u32,
    display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    discount_code: Option<String>,
}

fn main() {
    let p = Product {
        product_id: 9,
        display_name: "Widget".into(),
        discount_code: None,
    };
    println!("{}", serde_json::to_string(&p).unwrap());
    // {"productId":9,"displayName":"Widget"}
}
```

</details>

### Exercise 2: Forgiving configuration with defaults

**Difficulty:** Medium

**Objective:** Use `default` and `default = "fn"` so a minimal config still parses.

**Instructions:** Define `ServerConfig` with `host: String`, `port: u16` (default `8080`), `tls_enabled: bool` (default `false`), and `allowed_origins: Vec<String>` (default empty). Deserialize the JSON `{ "host": "localhost" }` and confirm the missing fields receive their defaults.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,                 // TODO: default 8080
    tls_enabled: bool,         // TODO: default false
    allowed_origins: Vec<String>, // TODO: default empty
}

fn main() {
    let cfg: ServerConfig =
        serde_json::from_str(r#"{ "host": "localhost" }"#).unwrap();
    println!("{cfg:?}");
}
```

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ServerConfig {
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    tls_enabled: bool,
    #[serde(default)]
    allowed_origins: Vec<String>,
}

fn default_port() -> u16 {
    8080
}

fn main() {
    let cfg: ServerConfig =
        serde_json::from_str(r#"{ "host": "localhost" }"#).unwrap();
    println!("{cfg:?}");
    // ServerConfig { host: "localhost", port: 8080, tls_enabled: false, allowed_origins: [] }
}
```

`#[serde(default)]` uses each type's `Default` (`false`, empty `Vec`), while `default = "default_port"` calls a function for the non-trivial `8080`.

</details>

### Exercise 3: Comma-separated list via `with`

**Difficulty:** Hard

**Objective:** Write a `with` module so a `Vec<String>` field is stored on the wire as a single comma-separated string.

**Instructions:** Define `CsvRow { id: u32, labels: Vec<String> }`. Build a module `comma_list` exposing `serialize` and `deserialize` so that `labels` serializes as `"red,urgent"` and parses `"a, b, c"` back into a trimmed `Vec`. Wire it with `#[serde(with = "comma_list")]`.

```rust
use serde::{Deserialize, Serialize};

mod comma_list {
    // TODO: serialize<S>(items: &[String], s: S) -> Result<S::Ok, S::Error>
    // TODO: deserialize<'de, D>(d: D) -> Result<Vec<String>, D::Error>
}

#[derive(Debug, Serialize, Deserialize)]
struct CsvRow {
    id: u32,
    labels: Vec<String>, // TODO: #[serde(with = "comma_list")]
}

fn main() {
    let row = CsvRow { id: 1, labels: vec!["red".into(), "urgent".into()] };
    println!("{}", serde_json::to_string(&row).unwrap());
    let back: CsvRow =
        serde_json::from_str(r#"{"id":2,"labels":"a, b, c"}"#).unwrap();
    println!("{back:?}");
}
```

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};

mod comma_list {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(items: &[String], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&items.join(","))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.is_empty() {
            return Ok(Vec::new());
        }
        Ok(s.split(',').map(|p| p.trim().to_string()).collect())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CsvRow {
    id: u32,
    #[serde(with = "comma_list")]
    labels: Vec<String>,
}

fn main() {
    let row = CsvRow { id: 1, labels: vec!["red".into(), "urgent".into()] };
    println!("{}", serde_json::to_string(&row).unwrap());
    // {"id":1,"labels":"red,urgent"}

    let back: CsvRow =
        serde_json::from_str(r#"{"id":2,"labels":"a, b, c"}"#).unwrap();
    println!("{back:?}");
    // CsvRow { id: 2, labels: ["a", "b", "c"] }
}
```

The `with` module names two functions Serde wires up automatically. This is the gateway to fully hand-written impls covered in [Custom Serialization](/15-serialization/07-custom-serialization/).

</details>
