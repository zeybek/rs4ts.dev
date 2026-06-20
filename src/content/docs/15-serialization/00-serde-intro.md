---
title: "Serde: From `JSON.parse`/`JSON.stringify` to a Universal Data Model"
description: "Serde is Rust's answer to JSON.parse and JSON.stringify, but type-checked and format-agnostic: one struct serializes to JSON, TOML, YAML, and more."
---

In TypeScript you reach for `JSON.parse` and `JSON.stringify` so often they feel like part of the language. Rust's answer is **Serde**, a serialization framework that does the same job but with two important upgrades: it is **type-checked** (you parse *into* a known type, not into `any`) and it is **format-agnostic** (the same type serializes to JSON, TOML, YAML, MessagePack, and more). This page maps your `JSON.parse`/`JSON.stringify` instincts onto Serde's **`Serialize`** and **`Deserialize`** traits and explains the **data-model architecture** that makes one type work with every format.

---

## Quick Overview

**Serde** (a contraction of **ser**ialize/**de**serialize) is the de-facto serialization framework for Rust. Instead of one built-in `JSON` object, Serde splits the problem into two halves that meet in the middle: your **data types** (structs and enums that implement the `Serialize`/`Deserialize` traits) on one side, and **data formats** (JSON, TOML, YAML, …, each a separate crate) on the other. Because both sides talk through a shared **data model**, you write your types once and get every format for free. And unlike `JSON.parse`, deserialization validates against a real type and returns a `Result` instead of handing you an unchecked `any`.

> **Note:** This page is the conceptual on-ramp. The mechanics (adding the crates, the exact `to_string`/`from_str` calls, the derive macro, attributes, and other formats) are covered in detail by the sibling pages linked throughout and in [Further Reading](#further-reading).

---

## TypeScript/JavaScript Example

In TypeScript, `JSON` is a global object with two methods. You typically `parse` into a value typed as `any` (or, more honestly, `unknown`) and then *assert* its shape:

```typescript
// blog.ts
interface BlogPost {
  id: number;
  title: string;
  tags: string[];
  published: boolean;
}

// Serialize: an object -> a JSON string
const post: BlogPost = {
  id: 42,
  title: "Rust for TS/JS Developers",
  tags: ["rust", "serde"],
  published: true,
};

const json: string = JSON.stringify(post);
console.log(json);
// {"id":42,"title":"Rust for TS/JS Developers","tags":["rust","serde"],"published":true}

const pretty: string = JSON.stringify(post, null, 2); // 2-space indent
console.log(pretty);

// Deserialize: a JSON string -> a value
const input = '{"id":7,"title":"Hello","tags":["intro"],"published":false}';
const parsed = JSON.parse(input) as BlogPost; // <- a *cast*, not a check
console.log(parsed.title); // "Hello"
```

There are three things to notice, because Rust changes all three:

1. **`JSON.parse` returns `any`.** The `as BlogPost` is a compile-time *promise*, not a runtime *check*. If the JSON is the wrong shape, you find out later, somewhere else, with a confusing error.
2. **`JSON.parse` does not validate.** It happily accepts extra fields, the wrong types, or missing fields:

   ```typescript
   const wrong = JSON.parse('{"id":"oops","extra":true}') as BlogPost;
   console.log(wrong, typeof wrong.id); // { id: 'oops', extra: true } 'string'
   ```

   No error — `id` is a string and `title` is missing, but TypeScript's cast hides it.
3. **`JSON.stringify` is JSON-only.** Want TOML for a config file or MessagePack on the wire? That is a different library with a different API.

---

## Rust Equivalent

In Rust, you describe the shape **once** as a struct, derive the two traits, and parse *into that type*. The format lives in a separate crate (`serde_json` here):

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```rust playground
use serde::{Deserialize, Serialize};

// The same shape as the TypeScript `interface BlogPost`.
#[derive(Debug, Serialize, Deserialize)]
struct BlogPost {
    id: u32,
    title: String,
    tags: Vec<String>,
    published: bool,
}

fn main() {
    // Serialize: a value -> a JSON string (like JSON.stringify)
    let post = BlogPost {
        id: 42,
        title: "Rust for TS/JS Developers".to_string(),
        tags: vec!["rust".to_string(), "serde".to_string()],
        published: true,
    };

    let json: String = serde_json::to_string(&post).unwrap();
    println!("{json}");

    let pretty: String = serde_json::to_string_pretty(&post).unwrap();
    println!("{pretty}");

    // Deserialize: a JSON string -> a value (like JSON.parse, but type-checked)
    let input = r#"{"id":7,"title":"Hello","tags":["intro"],"published":false}"#;
    let parsed: BlogPost = serde_json::from_str(input).unwrap();
    println!("{parsed:?}");
    println!("title = {}", parsed.title);
}
```

Real output:

```text
{"id":42,"title":"Rust for TS/JS Developers","tags":["rust","serde"],"published":true}
{
  "id": 42,
  "title": "Rust for TS/JS Developers",
  "tags": [
    "rust",
    "serde"
  ],
  "published": true
}
BlogPost { id: 7, title: "Hello", tags: ["intro"], published: false }
title = Hello
```

The shapes line up almost word-for-word with the TypeScript. The differences are the important part: `serde_json::from_str` returns a **`Result`** (we `.unwrap()`ed it for brevity; see the next section), and the type it produces is a real `BlogPost`, not an `any` you have to trust.

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects the newest edition automatically. `cargo add serde --features derive` and `cargo add serde_json` add the dependencies above. `cargo add` has been built into Cargo since 1.62, so no extra tooling is required.

---

## Detailed Explanation

### The two traits: `Serialize` and `Deserialize`

`JSON.stringify` and `JSON.parse` are two functions. Serde splits the same idea into two **traits** (Rust's version of interfaces) that a *type* implements:

| TypeScript | Serde |
| --- | --- |
| `JSON.stringify(value)` works on any object | a type implements **`Serialize`** to say "I can be turned into data" |
| `JSON.parse(text)` produces an object | a type implements **`Deserialize`** to say "I can be built from data" |

When `BlogPost` derives both, the macro generates the code that walks its fields. (What that generated code looks like is covered in [Deriving `Serialize` and `Deserialize`](/15-serialization/02-derive-serialize/).) You almost never implement these traits by hand; the [`derive` macro](/15-serialization/02-derive-serialize/) does it, the same way you'd let the compiler derive `Debug`. Hand-written impls are reserved for unusual cases; see [Custom Serialization](/15-serialization/07-custom-serialization/).

> **Note:** The `features = ["derive"]` part of the `serde` dependency is what enables `#[derive(Serialize, Deserialize)]`. Forgetting it is the single most common setup mistake; see [Common Pitfalls](#common-pitfalls).

### Why the function call is `serde_json::to_string`, not `BlogPost::to_string`

This is the architectural twist. In JavaScript, `JSON.stringify` *is* the JSON encoder. In Rust, the trait (`Serialize`) describes *what your type can do*, while the **format crate** (`serde_json`) provides the *function that drives it*. The call `serde_json::to_string(&post)` reads as: "JSON crate, please serialize this value." Swap `serde_json` for `toml` and the very same `post` becomes TOML. We'll see that in the [Real-World Example](#real-world-example) and in [Beyond JSON](/15-serialization/06-other-formats/).

### Parsing returns a `Result`, not an `any`

The signature of `serde_json::from_str` is, in spirit:

```text
fn from_str<T: Deserialize>(s: &str) -> Result<T, serde_json::Error>
```

Two things follow. First, the type `T` you want out is part of the call — usually supplied by the binding's type annotation (`let parsed: BlogPost = ...`) or a turbofish (`from_str::<BlogPost>(...)`). Second, the return is a **`Result`** (covered in [Section 08: Error Handling](/08-error-handling/)). Where `JSON.parse` throws, and where its `as BlogPost` cast lies about validation, Serde forces you to acknowledge that parsing can fail:

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    port: u16,
    host: String,
}

// Returns a Result instead of throwing, and the `?` propagates failures.
fn load_config(raw: &str) -> Result<Config, serde_json::Error> {
    let config: Config = serde_json::from_str(raw)?;
    Ok(config)
}

fn main() {
    // A valid document.
    match load_config(r#"{"port":8080,"host":"localhost"}"#) {
        Ok(cfg) => println!("listening on {}:{}", cfg.host, cfg.port),
        Err(e) => println!("config error: {e}"),
    }

    // Malformed JSON (syntax error).
    match load_config(r#"{"port":8080,"host":}"#) {
        Ok(cfg) => println!("listening on {}:{}", cfg.host, cfg.port),
        Err(e) => println!("config error: {e}"),
    }

    // Well-formed JSON, wrong shape: `port` should be a number.
    match load_config(r#"{"port":"oops"}"#) {
        Ok(cfg) => println!("listening on {}:{}", cfg.host, cfg.port),
        Err(e) => println!("config error: {e}"),
    }
}
```

Real output:

```text
listening on localhost:8080
config error: expected value at line 1 column 21
config error: invalid type: string "oops", expected u16 at line 1 column 14
```

Look at the third line: `invalid type: string "oops", expected u16`. This is the validation TypeScript's cast silently skipped. Serde checked the shape *and the types* against `Config` and told you exactly where and why it failed: line, column, and the type mismatch. The `?` operator (see [the `?` operator](/08-error-handling/01-question-mark/)) is Serde's answer to letting an error bubble up the way an uncaught `throw` would in JavaScript, but explicitly and on the function's return type.

### How Serde handles fields you didn't model

By default Serde is forgiving about *extra* fields and strict about *missing* ones, the opposite of `JSON.parse`'s "anything goes":

```rust playground
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct User {
    id: u32,
    name: String,
}

fn main() {
    // An unknown field ("extra") is ignored by default.
    let u: User = serde_json::from_str(r#"{"id":1,"name":"Ada","extra":true}"#).unwrap();
    println!("unknown field ignored: {u:?}");

    // A missing required field ("name") is an error.
    match serde_json::from_str::<User>(r#"{"id":1}"#) {
        Ok(u) => println!("{u:?}"),
        Err(e) => println!("missing field error: {e}"),
    }
}
```

Real output:

```text
unknown field ignored: User { id: 1, name: "Ada" }
missing field error: missing field `name` at line 1 column 8
```

Both behaviors are configurable with attributes (`#[serde(deny_unknown_fields)]`, `#[serde(default)]`, `Option<T>` fields, …), which is the subject of [Serde Attributes](/15-serialization/05-attributes/).

---

## Key Differences

### The data-model architecture

This is the mental model worth internalizing. JavaScript's `JSON` is a monolith: object in, string out, JSON only. Serde is shaped like an **hourglass** with a thin waist in the middle:

```text
   YOUR TYPES                 SERDE DATA MODEL              FORMATS
   (Serialize/Deserialize)    (~29 primitives:             (one crate each)
                               bool, i32, str, seq,
   struct BlogPost  ─┐         map, struct, enum, …)   ┌─►  serde_json   (JSON)
   struct Config    ─┤                                 ├─►  toml         (TOML)
   enum  Event      ─┼──────►  [ the thin waist ]  ────┼─►  serde_norway (YAML)
   Vec<T>, HashMap  ─┤                                 ├─►  rmp-serde    (MessagePack)
   Option<T>, …     ─┘                                 └─►  bincode      (binary)
```

Your type's `Serialize` impl describes itself in terms of the **data model** ("I am a struct with four fields named …"). A format crate's serializer translates those data-model calls into bytes. Because the two halves only ever talk through the waist, **`N` data types and `M` formats need `N + M` pieces of code, not `N × M`.** Add a new struct and every format already supports it; add a new format crate and every struct you've ever written already serializes to it.

`JSON.parse`/`JSON.stringify` give you `1` data model (whatever JavaScript objects are) and `1` format (JSON). Serde generalizes both axes.

### `any` versus a real type

| Aspect | TypeScript `JSON` | Rust Serde |
| --- | --- | --- |
| Parse result | `any` (cast to a type you hope is right) | a concrete `T`, validated against its definition |
| Type checking | none at runtime; the `as` cast is a no-op | full: wrong type/missing field → `Err` |
| Failure mode | `throw` on bad syntax; *silent* on bad shape | `Result` for both bad syntax and bad shape |
| Number precision | every number is IEEE-754 `f64`; large integers lose precision | you choose `u64`, `i32`, `f64`, … and get the exact type |
| Formats | JSON only | JSON, TOML, YAML, MessagePack, bincode, CSV, … |
| Extra fields | kept on the object | ignored by default (configurable) |
| Where the logic lives | the `JSON` global | split: traits on your type + a format crate |

> **Warning:** That number-precision row bites real programs. In JavaScript, `JSON.parse('{"id":12345678901234567890}')` yields `12345678901234567000`: the integer is silently rounded because every JS `number` is a 64-bit float. It does **not** wrap; it loses precision. In Rust you would model that field as `u64` (or `u128`) and Serde would preserve every digit, or return an error if the value doesn't fit.

### Serialization is explicit, not reflective

`JSON.stringify` uses runtime reflection: it walks whatever properties happen to exist on the object, drops `undefined` and functions, and turns `Date` into a string via `toJSON`. Rust has no runtime reflection; instead, the `#[derive(Serialize)]` macro generates the field-walking code **at compile time** based on the struct definition. The upshot: what gets serialized is fixed by the type, knowable by reading the source, and has zero reflection overhead at runtime. The trade-off is that "serialize this arbitrary value" isn't a thing: a value's type must implement `Serialize`, and the compiler enforces it (see the second pitfall below).

---

## Common Pitfalls

### Pitfall 1: Forgetting `features = ["derive"]`

The `#[derive(Serialize, Deserialize)]` macros live behind an opt-in feature of the `serde` crate. If you add plain `serde = "1"` (for example via `cargo add serde` without `--features derive`), the *traits* are in scope but the *derive macros* are not:

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)] // does not compile: derive macros not enabled
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let p = Point { x: 1, y: 2 };
    println!("{}", serde_json::to_string(&p).unwrap());
}
```

The real error is unusually helpful here:

```text
error: cannot find derive macro `Serialize` in this scope
 --> src/main.rs:3:10
  |
3 | #[derive(Serialize, Deserialize)]
  |          ^^^^^^^^^
  |
note: `Serialize` is imported here, but it is only a trait, without a derive macro
 --> src/main.rs:1:26
  |
1 | use serde::{Deserialize, Serialize};
  |                          ^^^^^^^^^
```

The fix is in `Cargo.toml`:

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
```

The full setup walkthrough lives in [Serde Basics](/15-serialization/01-serde-basics/).

### Pitfall 2: A field whose type isn't serializable

Because Serde has no reflection, *every* field of a struct you derive `Serialize` on must itself implement `Serialize`. Put a non-serializable type in a field and the compiler stops you, at compile time, not at runtime like a `JSON.stringify` surprise:

```rust
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Serialize, Deserialize)] // does not compile (error E0277)
struct Session {
    user: String,
    started: Instant, // Instant does not implement Serialize
}

fn main() {
    let s = Session { user: "bob".to_string(), started: Instant::now() };
    println!("{}", serde_json::to_string(&s).unwrap());
}
```

The real error, trimmed:

```text
error[E0277]: the trait bound `Instant: serde::Serialize` is not satisfied
 --> src/main.rs:4:10
  |
4 | #[derive(Serialize, Deserialize)]
  |          ^^^^^^^^^ the trait `Serialize` is not implemented for `Instant`
...
7 |     started: Instant,
  |     ------- required by a bound introduced by this call
  |
  = note: for types from other crates check whether the crate offers a `serde` feature flag
```

The note points at the fix: many crates ship their own `serde` support behind a feature (for time, `chrono` with `features = ["serde"]`), or you serialize a serializable representation instead (e.g. a `u64` of seconds), or you tell Serde to skip the field with `#[serde(skip)]`. Skipping and custom field handling are covered in [Serde Attributes](/15-serialization/05-attributes/) and [Custom Serialization](/15-serialization/07-custom-serialization/).

### Pitfall 3: Treating `from_str` like `JSON.parse` and ignoring the `Result`

Coming from JavaScript, it is tempting to `.unwrap()` every `from_str` because `JSON.parse` "just returns the value." In a real program that turns a malformed network payload into a **panic** that aborts the thread. `serde_json::from_str` returns a `Result` precisely so you can decide what to do; match on it, propagate it with `?`, or convert it with the `Result` combinators from [Section 08](/08-error-handling/00-result-option/). Reserve `.unwrap()` for tests and for data you constructed yourself and *know* is valid.

### Pitfall 4: Expecting `JSON.parse`'s silent shape-mismatch behavior

A subtle one: TypeScript's `JSON.parse(...) as Config` *accepts* `{"port":"oops"}` and only blows up later when something tries to use `port` as a number. Serde rejects it immediately with `invalid type: string "oops", expected u16`. This is a feature, not friction: the failure happens at the boundary, with a precise location, instead of leaking a wrong-typed value deep into your program. If you genuinely want to accept loosely-typed input, that is an explicit choice (e.g. deserialize into `serde_json::Value` first — see [Dynamic JSON with `serde_json::Value`](/15-serialization/04-json-manipulation/)).

---

## Best Practices

- **Model the shape with a struct and derive the traits.** Reach for `#[derive(Serialize, Deserialize)]` on a real type before reaching for the dynamic `serde_json::Value`. A typed model gives you validation, autocompletion, and a single source of truth; see the typed-vs-`Value` discussion in [Dynamic JSON with `serde_json::Value`](/15-serialization/04-json-manipulation/).
- **Derive `Debug` alongside `Serialize`/`Deserialize`.** `#[derive(Debug, Serialize, Deserialize)]` costs nothing and makes `println!("{value:?}")` and test failures readable.
- **Let the binding's type drive deserialization.** Prefer `let cfg: Config = serde_json::from_str(s)?;` over a turbofish; it reads cleanly and the annotation documents intent. Use `from_str::<Config>(s)?` only when there is no binding to annotate.
- **Propagate, don't panic.** Return `Result<_, serde_json::Error>` (or a unified app error via [`anyhow`/`thiserror`](/08-error-handling/06-anyhow-thiserror/)) and use `?`. Save `.unwrap()`/`.expect()` for tests and provable invariants.
- **Choose precise field types.** Use `u32`/`u64`/`i64` for integers you care about and `f64` only for genuine floats. This is where Rust beats JavaScript's one-`number`-fits-all model: model the data accurately and Serde enforces it.
- **Keep types format-neutral.** Don't bake JSON assumptions into your structs. The whole point of the data model is that the same type also serializes to TOML/YAML/MessagePack; attributes like `#[serde(rename_all = "camelCase")]` ([Serde Attributes](/15-serialization/05-attributes/)) adapt naming per need without coupling the type to one format.

---

## Real-World Example

A common task: consume a JSON API response into typed Rust, then re-emit the same value in a *different* format: exactly the scenario where Serde's data model pays off. Here we parse a GitHub-style repository payload into nested structs and serialize the result to both JSON and TOML, with **no per-format code in the types**.

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "1"
```

```rust playground
use serde::{Deserialize, Serialize};

// One set of types. They implement Serialize + Deserialize, so they can travel
// to/from ANY format whose crate plugs into Serde.
#[derive(Debug, Serialize, Deserialize)]
struct Repository {
    name: String,
    full_name: String,
    stargazers_count: u32,
    private: bool,
    owner: Owner, // nested struct — Serde recurses automatically
    topics: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Owner {
    login: String,
    id: u64, // a real 64-bit integer, not a lossy f64
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Pretend this came back from an HTTP GET.
    let body = r#"
    {
        "name": "serde",
        "full_name": "serde-rs/serde",
        "stargazers_count": 9123,
        "private": false,
        "owner": { "login": "serde-rs", "id": 4144980 },
        "topics": ["serialization", "rust"]
    }"#;

    // Deserialize JSON -> typed Rust (validated against the structs above).
    let repo: Repository = serde_json::from_str(body)?;
    println!(
        "{}/{} has {} stars",
        repo.owner.login, repo.name, repo.stargazers_count
    );

    // Serialize the SAME value to two different formats.
    let as_json = serde_json::to_string(&repo)?;
    let as_toml = toml::to_string(&repo)?;
    println!("--- JSON ---\n{as_json}");
    println!("--- TOML ---\n{as_toml}");

    Ok(())
}
```

Real output:

```text
serde-rs/serde has 9123 stars
--- JSON ---
{"name":"serde","full_name":"serde-rs/serde","stargazers_count":9123,"private":false,"owner":{"login":"serde-rs","id":4144980},"topics":["serialization","rust"]}
--- TOML ---
name = "serde"
full_name = "serde-rs/serde"
stargazers_count = 9123
private = false
topics = ["serialization", "rust"]

[owner]
login = "serde-rs"
id = 4144980
```

The `Repository` and `Owner` types never mention JSON or TOML. The nesting (`owner`, the `Vec<String>` of topics) is handled automatically because Serde recurses through fields that are themselves `Serialize`/`Deserialize`. Swapping in YAML or MessagePack would change only the format crate, never the types. In a web service you'd wire this same `Repository` straight into a handler; see [Section 16: Web APIs](/16-web-apis/).

---

## Further Reading

### Official documentation

- [Serde — official site and guide](https://serde.rs/) — the data model, the traits, and the attribute reference.
- [Understanding Serde's data model](https://serde.rs/data-model.html) — the ~29 primitives that form the "thin waist."
- [`serde` crate on docs.rs](https://docs.rs/serde/latest/serde/) — the `Serialize` and `Deserialize` trait docs.
- [`serde_json` crate on docs.rs](https://docs.rs/serde_json/latest/serde_json/) — `to_string`, `from_str`, and friends.
- [MDN: `JSON.parse`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/JSON/parse) and [`JSON.stringify`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/JSON/stringify) — the TypeScript/JavaScript starting points this page maps from.

### Related sections in this guide

- [Serde Basics](/15-serialization/01-serde-basics/) — the exact setup, `to_string`/`from_str`, and your first round-trip.
- [Deriving `Serialize` and `Deserialize`](/15-serialization/02-derive-serialize/) — what `#[derive(Serialize, Deserialize)]` generates and how it works on enums.
- [Structs and JSON](/15-serialization/03-json/) — structs ↔ JSON in depth: nested types, `Vec`/`HashMap`, `Option` fields, enum representations.
- [Dynamic JSON with `serde_json::Value`](/15-serialization/04-json-manipulation/) — dynamic JSON with `serde_json::Value` and the `json!` macro, and when to prefer it over a typed model.
- [Serde Attributes](/15-serialization/05-attributes/) — `rename`, `rename_all`, `skip`, `default`, `flatten`, `tag`, and more.
- [Beyond JSON](/15-serialization/06-other-formats/) — the same data as TOML, YAML, MessagePack, bincode, and CSV.
- [Custom Serialization](/15-serialization/07-custom-serialization/) — hand-written `Serialize`/`Deserialize` and `serialize_with`/`deserialize_with`.
- [Serde Performance](/15-serialization/08-performance/) — borrowing, zero-copy, streaming, and avoiding `Value`.
- [Section 08: Error Handling](/08-error-handling/) — `Result`, the [`?` operator](/08-error-handling/01-question-mark/), and `anyhow`/`thiserror`, all of which appear in real Serde code.
- [Section 09: Generics and Traits](/09-generics-traits/) — `Serialize`/`Deserialize` are traits; this is the background on what that means.
- [Section 02: Basics](/02-basics/) — the concrete number types (`u32`, `u64`, `f64`) you'll model JSON numbers with.

---

## Exercises

### Exercise 1: First round-trip

**Difficulty:** Easy

**Objective:** Confirm you can take a struct to JSON and back.

**Instructions:** Define a `Movie` struct with `title: String`, `year: u16`, and `rating: f64`. Derive `Debug`, `Serialize`, and `Deserialize`. In `main`, build a `Movie`, serialize it with `serde_json::to_string_pretty`, print the JSON, then deserialize that JSON back into a `Movie` and print it with `{:?}`.

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Movie {
    title: String,
    year: u16,
    rating: f64,
}

fn main() {
    let m = Movie {
        title: "Arrival".to_string(),
        year: 2016,
        rating: 8.0,
    };

    let json = serde_json::to_string_pretty(&m).unwrap();
    println!("{json}");

    let back: Movie = serde_json::from_str(&json).unwrap();
    println!("{back:?}");
}
```

Real output:

```text
{
  "title": "Arrival",
  "year": 2016,
  "rating": 8.0
}
Movie { title: "Arrival", year: 2016, rating: 8.0 }
```

</details>

### Exercise 2: Parse without panicking

**Difficulty:** Medium

**Objective:** Replace `JSON.parse`-style "just trust it" with a `Result`-returning function that reports type mismatches.

**Instructions:** Write `fn parse_movie(raw: &str) -> Result<Movie, serde_json::Error>` that deserializes a `Movie` (reuse the struct from Exercise 1). In `main`, call it twice: once with valid JSON and once with `{"title":"Dune","year":"twenty"}` (note `year` is a string). Print the parsed movie on `Ok` and the error on `Err`. Do not use `.unwrap()` on the parse result.

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Movie {
    title: String,
    year: u16,
    rating: f64,
}

fn parse_movie(raw: &str) -> Result<Movie, serde_json::Error> {
    serde_json::from_str(raw)
}

fn main() {
    match parse_movie(r#"{"title":"Dune","year":2021,"rating":8.0}"#) {
        Ok(movie) => println!("parsed {} ({})", movie.title, movie.year),
        Err(e) => eprintln!("bad movie json: {e}"),
    }

    // `year` is a string here — Serde validates and returns an Err.
    match parse_movie(r#"{"title":"Dune","year":"twenty"}"#) {
        Ok(movie) => println!("parsed {} ({})", movie.title, movie.year),
        Err(e) => eprintln!("bad movie json: {e}"),
    }
}
```

Real output (the error line goes to stderr):

```text
parsed Dune (2021)
bad movie json: invalid type: string "twenty", expected u16 at line 1 column 31
```

Note how Serde reported the precise type mismatch: the validation `JSON.parse` would have skipped.

</details>

### Exercise 3: One type, two formats

**Difficulty:** Medium

**Objective:** Experience the data-model architecture directly: serialize a single value to two formats with zero format-specific code in the type.

**Instructions:** Add the `toml` crate (`cargo add toml`). Define an `AppSettings` struct with `theme: String`, `autosave: bool`, and `max_recent_files: u8`. Serialize one `AppSettings` value to **both** JSON (with `serde_json::to_string`) and TOML (with `toml::to_string`) and print each. Then deserialize a TOML string back into `AppSettings` and print it with `{:?}`.

<details>
<summary>Solution</summary>

`Cargo.toml`:

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "1"
```

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct AppSettings {
    theme: String,
    autosave: bool,
    max_recent_files: u8,
}

fn main() {
    let settings = AppSettings {
        theme: "dark".to_string(),
        autosave: true,
        max_recent_files: 10,
    };

    // Same value, two formats — no per-format code in AppSettings.
    println!("{}", serde_json::to_string(&settings).unwrap());
    println!("{}", toml::to_string(&settings).unwrap());

    // And back from a TOML config file.
    let loaded: AppSettings =
        toml::from_str("theme = \"light\"\nautosave = false\nmax_recent_files = 5\n").unwrap();
    println!("{loaded:?}");
}
```

Real output:

```text
{"theme":"dark","autosave":true,"max_recent_files":10}
theme = "dark"
autosave = true
max_recent_files = 10

AppSettings { theme: "light", autosave: false, max_recent_files: 5 }
```

The `AppSettings` type never mentions JSON or TOML; that is the whole point of Serde's data model. Continue to [Beyond JSON](/15-serialization/06-other-formats/) to add YAML and MessagePack the same way.

</details>
