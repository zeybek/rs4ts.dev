---
title: "Serde Basics: Setup and the to_string / from_str Round-Trip"
description: "Set up Serde and run the JSON round-trip you know from JavaScript: to_string and from_str replace JSON.stringify and JSON.parse, into a typed struct."
---

In JavaScript, `JSON.stringify` and `JSON.parse` are built into the language and always available. Rust ships no JSON support in the standard library; instead the ecosystem standardized on **Serde**, a small set of crates you add to your project. This page is the hands-on starting point: how to add Serde to a project, and how to perform the two operations you do every day in TypeScript: turn a value into a JSON string, and parse a JSON string back into a value.

---

## Quick Overview

**Serde** ("**ser**ialize / **de**serialize") is the de-facto serialization framework for Rust. To use it for JSON you add two crates, `serde` (with the `derive` feature) and `serde_json`, then call `serde_json::to_string(&value)` to produce JSON and `serde_json::from_str::<T>(text)` to parse it. The closest mental model for a TypeScript developer is `JSON.stringify` / `JSON.parse`, but unlike those, Serde is fully type-directed: you tell it the exact type you expect, and parsing fails loudly with a precise error if the JSON does not match.

---

## TypeScript/JavaScript Example

In a Node.js or browser project, JSON is always there. A typical round-trip looks like this:

```typescript
interface Article {
  id: number;
  title: string;
  tags: string[];
  published: boolean;
}

const article: Article = {
  id: 7,
  title: "Serde in 5 minutes",
  tags: ["rust", "json"],
  published: true,
};

// Value -> JSON string
const json = JSON.stringify(article);
console.log(json);
// {"id":7,"title":"Serde in 5 minutes","tags":["rust","json"],"published":true}

// Pretty-printed (2-space indent)
console.log(JSON.stringify(article, null, 2));

// JSON string -> value
const input = '{"id":42,"title":"Parsed back","tags":["a","b"],"published":false}';
const parsed = JSON.parse(input) as Article;
console.log(parsed); // { id: 42, title: 'Parsed back', tags: [ 'a', 'b' ], published: false }
```

Two things are worth noticing, because they are exactly where Rust diverges. First, `JSON.parse` returns `any`: the `as Article` cast is a compile-time fiction that is **never checked at runtime**. If the JSON is missing `title` or has `id: "oops"`, `JSON.parse` happily hands you a malformed object and the bug surfaces later. Second, there is nothing to install: JSON support is part of the runtime.

---

## Rust Equivalent

First, create a project and add the two crates. `cargo new` selects the newest stable edition automatically (the repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition):

```bash
cargo new article_demo
cd article_demo
cargo add serde --features derive
cargo add serde_json
```

> **Note:** `cargo add` is built into Cargo (since 1.62); you do **not** need the separate `cargo-edit` tool that older tutorials mention.

That writes the following into `Cargo.toml`:

```toml
[dependencies]
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
```

The `features = ["derive"]` part is the piece TypeScript developers most often miss. Without it the `#[derive(Serialize, Deserialize)]` macros do not exist and your code will not compile. We cover that error in [Common Pitfalls](#common-pitfalls).

Now the equivalent round-trip in `src/main.rs`:

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Article {
    id: u32,
    title: String,
    tags: Vec<String>,
    published: bool,
}

fn main() -> Result<(), serde_json::Error> {
    let article = Article {
        id: 7,
        title: String::from("Serde in 5 minutes"),
        tags: vec![String::from("rust"), String::from("json")],
        published: true,
    };

    // Serialize: struct -> compact JSON string
    let json = serde_json::to_string(&article)?;
    println!("{json}");

    // Pretty-printed JSON (two-space indent)
    let pretty = serde_json::to_string_pretty(&article)?;
    println!("{pretty}");

    // Deserialize: JSON string -> struct
    let input = r#"{"id":42,"title":"Parsed back","tags":["a","b"],"published":false}"#;
    let parsed: Article = serde_json::from_str(input)?;
    println!("{parsed:?}");
    println!("title = {}", parsed.title);

    Ok(())
}
```

Running it with `cargo run` produces this real output:

```text
{"id":7,"title":"Serde in 5 minutes","tags":["rust","json"],"published":true}
{
  "id": 7,
  "title": "Serde in 5 minutes",
  "tags": [
    "rust",
    "json"
  ],
  "published": true
}
Article { id: 42, title: "Parsed back", tags: ["a", "b"], published: false }
title = Parsed back
```

The compact output matches `JSON.stringify` byte-for-byte, and `to_string_pretty` matches `JSON.stringify(x, null, 2)`. The key win is the last two lines: `parsed` is a real, fully-typed `Article`, not an `any` you have to trust.

---

## Detailed Explanation

Let's walk through the Rust version line by line and contrast each piece with the TypeScript.

- **`use serde::{Deserialize, Serialize};`** brings the two **traits** into scope. A trait is Rust's version of an interface (a shared contract); `Serialize` means "this type knows how to write itself out", `Deserialize` means "this type knows how to build itself from input". This single `use` line imports *both* the traits and the derive macros of the same name, which is why you write `Serialize` once but it does double duty.

- **`#[derive(Debug, Serialize, Deserialize)]`** is the heart of Serde's ergonomics. `derive` is a macro that **generates code at compile time**. Here it auto-writes the `Serialize` and `Deserialize` implementations for `Article` based on its fields, so you never hand-write the mapping. (`Debug` is unrelated to Serde — it just enables the `{:?}` printing used by `println!`.) This is *not* like a TypeScript decorator: decorators run at runtime and wrap behavior, whereas a Rust derive macro expands into ordinary source code before the program is even compiled. The full mechanics of what gets generated are covered in [Deriving `Serialize` and `Deserialize`](/15-serialization/02-derive-serialize/).

- **The field types matter.** `id: u32` is an unsigned 32-bit integer; `title: String` is an owned, growable UTF-8 string; `tags: Vec<String>` is a growable array (TypeScript's `string[]`); `published: bool` is a boolean. Serde uses these types to *drive* both serialization and parsing. When you ask it to parse into `Article`, it knows `id` must be a number that fits in a `u32`, and it will reject anything else.

- **`fn main() -> Result<(), serde_json::Error>`**: Serde's operations return a `Result`, Rust's type for "this might fail". Returning `Result` from `main` lets us use the `?` operator. See [Section 08: Error Handling](/08-error-handling/) if `Result` and `?` are new to you.

- **`serde_json::to_string(&article)?`** serializes a *borrow* of `article` (the `&`) into a `String`. The `?` says "if this returns an error, return it from `main`". Serialization of well-formed Rust values essentially never fails for JSON, but the API is still fallible because some types (or custom serializers) can error.

- **`serde_json::from_str(input)?`** parses the string. Note the annotation `let parsed: Article = ...`. Serde is **type-directed**: it needs to know the target type to parse into. You can also write it turbofish-style as `serde_json::from_str::<Article>(input)?`. This is the precise opposite of `JSON.parse`, which returns `any` and ignores the type entirely.

- **`r#"..."#`** is a **raw string literal** — like a backtick template literal in JavaScript minus interpolation. It lets the JSON's double quotes appear literally without escaping every `"` as `\"`.

---

## Key Differences

| Aspect | TypeScript / JavaScript | Rust + Serde |
| --- | --- | --- |
| Availability | Built into the runtime | Add `serde` + `serde_json` crates |
| Stringify | `JSON.stringify(x)` | `serde_json::to_string(&x)?` |
| Pretty print | `JSON.stringify(x, null, 2)` | `serde_json::to_string_pretty(&x)?` |
| Parse | `JSON.parse(s)` → `any` | `serde_json::from_str::<T>(s)?` → `T` |
| Runtime type checking | None: the `as T` cast is erased | Full: shape is validated against `T` |
| Failure mode | Returns malformed objects; throws only on syntax errors | Returns `Err` on syntax **and** shape mismatch |
| Numbers | All `number` (IEEE-754 f64) | You choose: `u8`, `i32`, `u64`, `f64`, … |

A few of these deserve emphasis:

- **Parsing is checked.** Because `from_str` knows the target type, a missing required field or a wrong type is a hard error, not a silent `undefined`. This moves a whole class of "the API changed and now `x.title` is undefined three functions later" bugs to the parse boundary.

- **You pick the numeric type.** JavaScript has one numeric type, `number`, which is always an IEEE-754 double. That means it silently **loses precision** on integers beyond 2^53 (it does *not* wrap around — it rounds). In Rust you declare exactly `u32`, `i64`, `f64`, and so on, and Serde enforces the range while parsing. A JSON `99999` will refuse to fit into a `u16` field.

- **Tuple structs and arrays.** Serde maps Rust shapes to natural JSON shapes. A named struct becomes a JSON object; a tuple struct becomes a JSON array. For example, `struct Rgb(u8, u8, u8)` with `Rgb(255, 128, 0)` serializes to `[255,128,0]`, not an object. That mirrors how the data is shaped, and it surprises people who expect every struct to become an object.

> **Note:** This page deals only with JSON via `serde_json`. The same `to_string` / `from_str` muscle memory transfers to TOML, YAML, and binary formats; see [Beyond JSON](/15-serialization/06-other-formats/). The architecture that makes one set of derives work across every format is explained in [Serde](/15-serialization/00-serde-intro/).

---

## Common Pitfalls

### Forgetting the `derive` feature

This is the single most common stumble. If you run `cargo add serde` (without `--features derive`) and then write `#[derive(Serialize, Deserialize)]`, the build fails. Here is the **real** compiler output:

```rust
// does not compile — `serde` was added WITHOUT features = ["derive"]
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Point {
    x: i32,
    y: i32,
}
```

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
  |                          ^^^^^^^^^^^
```

The fix is to enable the feature, either with `cargo add serde --features derive` or by editing `Cargo.toml`:

```toml
serde = { version = "1.0.228", features = ["derive"] }
```

> **Tip:** The note "it is only a trait, without a derive macro" is the compiler's hint that the trait imported fine but the *macro* of the same name is gated behind the feature flag.

### Expecting `from_str` to infer the type

Unlike `JSON.parse`, `from_str` cannot guess what you want. You must give it a target type, either with a `let` annotation or turbofish:

```rust
// Both forms work:
let parsed: Article = serde_json::from_str(input)?;
let parsed = serde_json::from_str::<Article>(input)?;
```

Omit the type and you get a `type annotations needed` error, because Rust has no way to choose `T`.

### Forgetting `Deserialize` when you only added `Serialize`

The two traits are independent. A type that only derives `Serialize` can be turned *into* JSON but cannot be parsed *from* it (and vice versa). If you call `from_str` on a type that only derives `Serialize`, the compiler reports that the trait bound `Article: Deserialize` is not satisfied. Derive both unless you genuinely only need one direction.

### Assuming a bad shape will "just parse" like JavaScript

In JavaScript, `JSON.parse('{"host":"localhost","port":8080}')` succeeds even if your code expects a `retries` field; you find out later when something is `undefined`. Serde fails at the boundary. Given:

```rust
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Config {
    host: String,
    port: u16,
    retries: u8,
}
```

these four inputs each produce a real, descriptive `Err` (printed via the error's `Display`):

```text
missing-field error: missing field `retries` at line 1 column 32
type error: invalid type: string "oops", expected u16 at line 1 column 33
range error: invalid value: integer `99999`, expected u16 at line 1 column 32
syntax error: trailing comma at line 1 column 33
```

The first is a missing field, the second a type mismatch, the third a number that overflows `u16`, the fourth malformed JSON. All four are caught for you, with a line and column, instead of becoming a silent runtime surprise.

---

## Best Practices

- **Always add `serde` with the `derive` feature** and add `serde_json` alongside it. `cargo add serde --features derive && cargo add serde_json` is the standard incantation.

- **Derive both `Serialize` and `Deserialize`** on data-transfer types unless you have a clear reason not to. It costs nothing extra and saves you a recompile when you later need the other direction.

- **Prefer the most specific numeric type.** Use `u16` for a port, `u8` for a percentage, `i64` for a database id. Letting Serde enforce the range at parse time is free validation you would otherwise hand-roll.

- **Use `to_string` for wire/storage and `to_string_pretty` for human-facing output** (config files you expect people to read, debug logs). The compact form is smaller; the pretty form is diff-friendly.

- **Propagate the error with `?` rather than `.unwrap()`** in real code. `serde_json::Error` carries the line and column; throwing it away with `.unwrap()` turns a helpful message into a bare panic. See [Section 08: Error Handling](/08-error-handling/).

- **Know that unknown fields are ignored by default.** Parsing `{"id":1,"name":"Ada","role":"admin"}` into a struct with only `id` and `name` succeeds and drops `role`. This is convenient for forward-compatible APIs but can hide typos. To make unexpected fields a hard error, use `#[serde(deny_unknown_fields)]` — covered in [Serde Attributes](/15-serialization/05-attributes/).

- **For byte input, use `from_slice`.** When you have raw bytes (e.g. an HTTP request body as `&[u8]`), `serde_json::from_slice(&bytes)` avoids an intermediate `String` allocation. There are also `from_reader` / `to_writer` for streaming I/O.

---

## Real-World Example

A common first task is loading and saving a service's configuration as JSON. This `ServerConfig` loader reads a file, parses it into a typed struct, writes a pretty default back out, and also shows parsing directly from a byte buffer (as you would from an HTTP body). It is a complete, runnable `src/main.rs` using only `serde` (with `derive`) and `serde_json`:

```rust playground
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Serialize, Deserialize)]
struct ServerConfig {
    bind_address: String,
    port: u16,
    max_connections: u32,
    log_level: String,
}

fn load_config(path: &str) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let config: ServerConfig = serde_json::from_str(&text)?;
    Ok(config)
}

fn save_config(path: &str, config: &ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let default = ServerConfig {
        bind_address: "0.0.0.0".to_string(),
        port: 8080,
        max_connections: 1024,
        log_level: "info".to_string(),
    };

    let path = "config.json";
    save_config(path, &default)?;

    let loaded = load_config(path)?;
    println!("Loaded config: {loaded:?}");
    println!("Listening on {}:{}", loaded.bind_address, loaded.port);

    // from_slice: parse straight from raw bytes (e.g. an HTTP request body)
    let bytes: &[u8] =
        br#"{"bind_address":"127.0.0.1","port":3000,"max_connections":256,"log_level":"debug"}"#;
    let from_bytes: ServerConfig = serde_json::from_slice(bytes)?;
    println!("From bytes: port={}", from_bytes.port);

    fs::remove_file(path).ok();
    Ok(())
}
```

Real output from `cargo run`:

```text
Loaded config: ServerConfig { bind_address: "0.0.0.0", port: 8080, max_connections: 1024, log_level: "info" }
Listening on 0.0.0.0:8080
From bytes: port=3000
```

The `Box<dyn std::error::Error>` return type lets both the file-I/O errors (from `fs::read_to_string` / `fs::write`) and the JSON errors (from `from_str` / `from_slice`) flow through the same `?` — a single, uniform error channel for two unrelated failure sources. This same struct, with no changes, would also load from TOML or YAML once you add the matching crate; see [Beyond JSON](/15-serialization/06-other-formats/).

---

## Further Reading

- [Serde official site](https://serde.rs/) — the canonical guide; start with "Overview" and "Using derive".
- [`serde_json` API docs](https://docs.rs/serde_json/): every function (`to_string`, `to_string_pretty`, `from_str`, `from_slice`, `from_reader`, `to_writer`).
- [The `serde` crate features](https://docs.rs/serde/latest/serde/#feature-flags) — what `derive` and other features enable.
- [Serde](/15-serialization/00-serde-intro/). The bigger picture: how Serde's data model decouples types from formats.
- [Deriving `Serialize` and `Deserialize`](/15-serialization/02-derive-serialize/) — what `#[derive(Serialize, Deserialize)]` actually generates.
- [Structs and JSON](/15-serialization/03-json/): mapping nested structs, `Vec`, `HashMap`, `Option`, and enums to JSON.
- [Dynamic JSON with `serde_json::Value`](/15-serialization/04-json-manipulation/) — dynamic JSON with `serde_json::Value` and the `json!` macro.
- [Serde Attributes](/15-serialization/05-attributes/): `rename`, `default`, `skip`, `deny_unknown_fields`, and friends.
- [Section 01: Getting Started](/01-getting-started/) — `cargo new`, `cargo add`, and project layout.
- [Section 08: Error Handling](/08-error-handling/): `Result`, `?`, and `Box<dyn Error>`.
- [Section 16: Web APIs](/16-web-apis/) — where serialized structs meet HTTP request and response bodies.

---

## Exercises

### Exercise 1: Round-trip a struct

**Difficulty:** Easy

**Objective:** Confirm that serializing then deserializing yields an equal value.

**Instructions:** In a fresh project (`cargo new`, then `cargo add serde --features derive` and `cargo add serde_json`), define a `Todo` struct with `id: u32`, `text: String`, and `done: bool`. Serialize an instance with `to_string`, parse it back with `from_str`, and `assert_eq!` that the original equals the parsed value. (You will need to derive `PartialEq` so the two values can be compared.)

```rust playground
use serde::{Deserialize, Serialize};

// TODO: derive the traits needed to serialize, deserialize, AND compare with assert_eq!
struct Todo {
    id: u32,
    text: String,
    done: bool,
}

fn main() -> Result<(), serde_json::Error> {
    let todo = Todo { id: 1, text: "Learn Serde".into(), done: false };
    // TODO: serialize -> deserialize -> assert_eq!
    Ok(())
}
```

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Todo {
    id: u32,
    text: String,
    done: bool,
}

fn main() -> Result<(), serde_json::Error> {
    let todo = Todo {
        id: 1,
        text: "Learn Serde".into(),
        done: false,
    };

    let json = serde_json::to_string(&todo)?;
    let back: Todo = serde_json::from_str(&json)?;

    assert_eq!(todo, back);
    println!("round-trip ok: {json}");
    Ok(())
}
```

Real output from `cargo run`:

```text
round-trip ok: {"id":1,"text":"Learn Serde","done":false}
```

`PartialEq` is what makes `assert_eq!` work; without it the compiler would reject the comparison. The `Debug` derive is needed so `assert_eq!` can print the values if they ever differ.

</details>

### Exercise 2: Parse nested JSON with a collection

**Difficulty:** Medium

**Objective:** Deserialize a JSON object that contains an array of nested objects.

**Instructions:** Define `Author { name: String, email: String }` and `Book { title: String, year: u16, authors: Vec<Author> }`. Parse the JSON literal below into a `Book` and print the title, year, and each author. Note how `Vec<Author>` and the nested struct require no extra wiring — Serde recurses automatically.

```rust playground
use serde::{Deserialize, Serialize};

// TODO: define Author and Book with the right derives

fn main() -> Result<(), serde_json::Error> {
    let input = r#"{
        "title": "Programming Rust",
        "year": 2021,
        "authors": [
            {"name": "Jim Blandy", "email": "jim@example.com"},
            {"name": "Jason Orendorff", "email": "jason@example.com"}
        ]
    }"#;
    // TODO: parse into a Book and print its contents
    Ok(())
}
```

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Author {
    name: String,
    email: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Book {
    title: String,
    year: u16,
    authors: Vec<Author>,
}

fn main() -> Result<(), serde_json::Error> {
    let input = r#"{
        "title": "Programming Rust",
        "year": 2021,
        "authors": [
            {"name": "Jim Blandy", "email": "jim@example.com"},
            {"name": "Jason Orendorff", "email": "jason@example.com"}
        ]
    }"#;

    let book: Book = serde_json::from_str(input)?;
    println!(
        "{} ({}) by {} authors",
        book.title,
        book.year,
        book.authors.len()
    );
    for author in &book.authors {
        println!("  - {} <{}>", author.name, author.email);
    }
    Ok(())
}
```

Real output from `cargo run`:

```text
Programming Rust (2021) by 2 authors
  - Jim Blandy <jim@example.com>
  - Jason Orendorff <jason@example.com>
```

`Vec<Author>` deserializes a JSON array, and each element is parsed as an `Author`. Serde handles arbitrarily deep nesting this way — you only describe the shape with types.

</details>

### Exercise 3: Read the error on a shape mismatch

**Difficulty:** Medium

**Objective:** Observe that Serde rejects JSON that does not match your type, and inspect the error message.

**Instructions:** Reuse the `Book` type from Exercise 2. Try to parse JSON where `year` is the *string* `"2021"` instead of a number. Match on the `Result`: print the parsed book on `Ok`, and the error's message on `Err`. Confirm you get a descriptive type-mismatch error rather than a silently wrong value.

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Book {
    title: String,
    year: u16,
}

fn main() {
    let bad = r#"{"title":"Mistyped","year":"2021"}"#;
    // TODO: match on serde_json::from_str::<Book>(bad) and print Ok or the Err message
}
```

<details>
<summary>Solution</summary>

```rust playground
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Book {
    title: String,
    year: u16,
}

fn main() {
    let bad = r#"{"title":"Mistyped","year":"2021"}"#;
    match serde_json::from_str::<Book>(bad) {
        Ok(book) => println!("parsed: {book:?}"),
        Err(e) => println!("could not parse: {e}"),
    }
}
```

Real output from `cargo run`:

```text
could not parse: invalid type: string "2021", expected u16 at line 1 column 33
```

In TypeScript, `JSON.parse` would have returned `{ title: "Mistyped", year: "2021" }` and the wrong type would slip through unnoticed. Serde caught it at the boundary with a line and column. Matching on `Result` instead of calling `.unwrap()` is how you turn that into graceful handling — for example, returning a 400 response from a web handler.

</details>
