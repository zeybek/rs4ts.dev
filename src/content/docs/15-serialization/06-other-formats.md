---
title: "Beyond JSON: TOML, YAML, MessagePack, bincode, and CSV with Serde"
description: "Node needs a separate library per format; in Rust you derive Serialize once, then swap a crate to read TOML, YAML, MessagePack, bincode, or CSV from the same struct."
---

In the Node.js world, JSON is the default and everything else needs a separate library: `js-yaml` for YAML, `@iarna/toml` for TOML, `csv-parse` for CSV, `@msgpack/msgpack` for MessagePack. Each ships its own API, its own type story, and its own quirks. Serde flips this: you derive `Serialize` and `Deserialize` **once**, then plug in a different format crate to read or write the same data as TOML, YAML, MessagePack, a binary blob, or CSV rows. This page shows the same struct travelling through all five formats.

---

## Quick Overview

Serde separates your **data types** from the **wire format**. Your structs implement the `Serialize` and `Deserialize` traits via `#[derive]`; a *format crate* (`toml`, `serde_norway`, `rmp-serde`, `bincode`, `csv`) knows how to turn that trait-driven description into bytes and back. Adding a new format to your program means adding a crate and calling its `to_string` / `from_str` (or `to_vec` / `from_slice`). Your type definitions do not change at all. For a TypeScript developer this is the payoff: one set of annotations, every format for free.

> **Note:** This page assumes you already know the JSON basics from [Serde Basics](/15-serialization/01-serde-basics/) and how `#[derive(Serialize, Deserialize)]` works from [Deriving `Serialize` and `Deserialize`](/15-serialization/02-derive-serialize/). The architecture that makes one derive work across all formats is explained in [Serde](/15-serialization/00-serde-intro/).

---

## TypeScript/JavaScript Example

In Node.js, each format is a different package with a different shape. Here is the same configuration object serialized to four text formats and one binary format, the way you would actually do it:

```typescript
// npm install js-yaml @iarna/toml @msgpack/msgpack
import * as YAML from "js-yaml";
import * as TOML from "@iarna/toml";
import { encode, decode } from "@msgpack/msgpack";

interface ServerConfig {
  name: string;
  port: number;
  workers: number;
  tls: boolean;
  allowedOrigins: string[];
}

const config: ServerConfig = {
  name: "api-gateway",
  port: 8443,
  workers: 8,
  tls: true,
  allowedOrigins: ["https://app.example.com", "https://admin.example.com"],
};

// JSON — built in
const json: string = JSON.stringify(config);

// YAML — js-yaml: returns `any` on load, no type checking
const yamlText: string = YAML.dump(config);
const fromYaml = YAML.load(yamlText) as ServerConfig; // cast is unchecked

// TOML — @iarna/toml: again `any`, again an unchecked cast
const tomlText: string = TOML.stringify(config as any);
const fromToml = TOML.parse(tomlText) as unknown as ServerConfig;

// MessagePack — binary, returns Uint8Array; decode() is `unknown`
const packed: Uint8Array = encode(config);
const fromMsgpack = decode(packed) as ServerConfig; // cast is unchecked
```

Notice the recurring pattern: each library has a different method name (`dump`/`load`, `stringify`/`parse`, `encode`/`decode`), and every `load`/`parse`/`decode` hands you `any` or `unknown` that you cast with `as`. That cast is a compile-time fiction — **nothing validates that the parsed bytes actually match `ServerConfig` at runtime**. CSV is worse still: most Node CSV libraries give you arrays of strings and you convert each field by hand.

---

## Rust Equivalent

First, add the format crates. `cargo new` selects the newest stable edition automatically (the current stable toolchain is Rust 1.96.0 on the 2024 edition):

```bash
cargo new config_formats
cd config_formats
cargo add serde --features derive
cargo add serde_json
cargo add toml
cargo add serde_norway        # maintained successor to the deprecated serde_yaml
cargo add rmp-serde           # MessagePack
cargo add bincode@2 --features serde
cargo add csv
```

That produces these dependencies in `Cargo.toml`:

```toml
[dependencies]
bincode = { version = "2", features = ["serde"] }
csv = "1.4.0"
rmp-serde = "1.3.1"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
serde_norway = "0.9.42"
toml = "1.1.2"
```

Now define the struct **once** and run it through every format in `src/main.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct ServerConfig {
    name: String,
    port: u16,
    workers: u32,
    tls: bool,
    allowed_origins: Vec<String>,
}

fn main() {
    let cfg = ServerConfig {
        name: "api-gateway".to_string(),
        port: 8443,
        workers: 8,
        tls: true,
        allowed_origins: vec![
            "https://app.example.com".to_string(),
            "https://admin.example.com".to_string(),
        ],
    };

    // --- TOML (text) ---
    let toml_text = toml::to_string(&cfg).unwrap();
    println!("=== TOML ===");
    print!("{toml_text}");
    let toml_back: ServerConfig = toml::from_str(&toml_text).unwrap();
    println!("toml round-trip equal: {}", toml_back == cfg);

    // --- YAML (text) ---
    let yaml_text = serde_norway::to_string(&cfg).unwrap();
    println!("=== YAML ===");
    print!("{yaml_text}");
    let yaml_back: ServerConfig = serde_norway::from_str(&yaml_text).unwrap();
    println!("yaml round-trip equal: {}", yaml_back == cfg);

    // --- MessagePack (binary) ---
    let mp: Vec<u8> = rmp_serde::to_vec(&cfg).unwrap();
    println!("=== MessagePack ===");
    println!("msgpack bytes: {}", mp.len());
    let mp_back: ServerConfig = rmp_serde::from_slice(&mp).unwrap();
    println!("msgpack round-trip equal: {}", mp_back == cfg);

    // --- bincode (binary) ---
    let config = bincode::config::standard();
    let bin: Vec<u8> = bincode::serde::encode_to_vec(&cfg, config).unwrap();
    println!("=== bincode ===");
    println!("bincode bytes: {}", bin.len());
    let (bin_back, _consumed): (ServerConfig, usize) =
        bincode::serde::decode_from_slice(&bin, config).unwrap();
    println!("bincode round-trip equal: {}", bin_back == cfg);
}
```

Real output from `cargo run`:

```text
=== TOML ===
name = "api-gateway"
port = 8443
workers = 8
tls = true
allowed_origins = ["https://app.example.com", "https://admin.example.com"]
toml round-trip equal: true
=== YAML ===
name: api-gateway
port: 8443
workers: 8
tls: true
allowed_origins:
- https://app.example.com
- https://admin.example.com
yaml round-trip equal: true
=== MessagePack ===
msgpack bytes: 69
msgpack round-trip equal: true
=== bincode ===
bincode bytes: 68
bincode round-trip equal: true
```

The struct never changed. Each format crate exposes the same two-verb vocabulary — text formats use `to_string` / `from_str`, binary formats use `to_vec` / `from_slice` — and every `from_*` returns a fully-typed `ServerConfig`, not an `any` you have to trust. The `round-trip equal: true` lines come from `PartialEq`, proving the data survived the trip intact.

---

## Detailed Explanation

Let's unpack each format and the one or two lines that drive it.

- **TOML, `toml::to_string(&cfg)` / `toml::from_str(&text)`.** TOML ("Tom's Obvious Minimal Language") is the format Rust itself uses for `Cargo.toml`, so it is the natural choice for human-edited config. The API mirrors `serde_json` exactly: `to_string`, `to_string_pretty`, `from_str`. Scalars become `key = value`; a `Vec` becomes an inline array; a nested struct becomes a `[table]` section. One structural rule matters: the **top level of a TOML document must be a table** (a struct or map), never a bare array or scalar; see [Common Pitfalls](#common-pitfalls).

- **YAML, `serde_norway::to_string(&cfg)` / `serde_norway::from_str(&text)`.** YAML is whitespace-significant and common in Kubernetes, CI pipelines, and Docker Compose. The widely-cited `serde_yaml` crate was **deprecated and archived by its author in 2024**; `serde_norway` is a drop-in maintained fork with the identical API, which is why this guide uses it. The output starts directly with `name:` — by default it emits no `---` document-start marker.

- **MessagePack, `rmp_serde::to_vec(&cfg)` / `rmp_serde::from_slice(&bytes)`.** MessagePack is a compact binary format ("like JSON, but fast and small"). Because it is binary, you serialize to `Vec<u8>` (not `String`) and deserialize from `&[u8]`. Here the same data is 69 bytes versus JSON's 131, roughly half. `rmp-serde` is the Serde-integrated implementation; the crate name comes from "**r**ust **m**ess**p**ack".

- **bincode — `bincode::serde::encode_to_vec(&cfg, config)` / `bincode::serde::decode_from_slice(&bin, config)`.** bincode is a Rust-native binary format optimized for Rust-to-Rust communication (caches, IPC, on-disk snapshots). Version 2.x takes an explicit **configuration** value (`bincode::config::standard()`) that fixes the integer encoding and byte order, and its `decode_from_slice` returns a tuple `(value, bytes_consumed)`. The `bincode::serde::` path is what bridges bincode to Serde-derived types; it is gated behind the `serde` feature you enabled with `cargo add bincode@2 --features serde`.

- **`PartialEq` in the derive.** Adding `PartialEq` lets us write `toml_back == cfg`. It has nothing to do with Serde; it is just how we assert each round-trip preserved the value.

Every format above reads the *same* `#[derive(Serialize, Deserialize)]`. That is the whole point of Serde's design: the format crate and your type never know about each other directly: they meet through Serde's data model.

---

## Key Differences

| Format | Crate | Text/Binary | Serialize / Deserialize | Typical use |
| --- | --- | --- | --- | --- |
| JSON | `serde_json` | text | `to_string` / `from_str` | HTTP APIs, config, logs |
| TOML | `toml` | text | `to_string` / `from_str` | Human-edited config (`Cargo.toml`) |
| YAML | `serde_norway` | text | `to_string` / `from_str` | Kubernetes, CI, Compose |
| MessagePack | `rmp-serde` | binary | `to_vec` / `from_slice` | Compact wire transfer, caches |
| bincode | `bincode` (v2) | binary | `encode_to_vec` / `decode_from_slice` | Rust↔Rust IPC, snapshots |
| CSV | `csv` | text | `Writer::serialize` / `Reader::deserialize` | Tabular data, spreadsheets |

A few conceptual points a TypeScript developer should internalize:

- **One derive, every format.** In Node you reach for a different library per format, each with its own type-erased `any`. In Rust the *type* carries the serialization logic, and formats are interchangeable consumers of it. Swapping JSON for MessagePack in a function is often a one-line change.

- **Binary formats use bytes, not strings.** `to_vec`/`encode_to_vec` return `Vec<u8>`; you cannot `println!("{}")` a MessagePack blob as text. This is a hard type-level distinction Rust enforces, unlike JavaScript where a `Uint8Array` and a `string` blur together at the edges.

- **Self-describing vs. schema-coupled.** JSON, TOML, YAML, and MessagePack are *self-describing*: the bytes contain field names (or at least structure), so a slightly different reader can still parse them. bincode's compact form is *positional*: it stores values in field order with no names, which is smaller but means producer and consumer must agree on the exact layout. MessagePack offers both modes (see Pitfalls).

- **CSV is row-shaped, not tree-shaped.** JSON/TOML/YAML/MessagePack represent arbitrary nested trees. CSV represents a flat table: a header row plus data rows. A struct maps cleanly to a row only when all its fields are scalars — nesting a struct inside another and writing it as CSV is an error, not silent flattening.

- **TOML and YAML are not interchangeable for everything.** TOML insists the root is a table and has no concept of a top-level array document; YAML is happy with a top-level sequence. Pick the format to fit the data shape, not the other way around.

---

## Common Pitfalls

### Reaching for the deprecated `serde_yaml`

Most YAML tutorials and older answers tell you to `cargo add serde_yaml`. That crate was deprecated and archived by its maintainer in 2024 and no longer receives fixes. Use **`serde_norway`** instead — it is a maintained fork with the same module-level functions (`to_string`, `from_str`, `to_writer`, `from_reader`), so existing examples work after a rename:

```rust
// Old (deprecated, unmaintained):
// let text = serde_yaml::to_string(&cfg).unwrap();

// Current:
let text = serde_norway::to_string(&cfg).unwrap();
```

### TOML's top level must be a table

TOML documents are key/value tables at the root. Trying to serialize a bare `Vec` (or any non-map) at the top level fails. This compiles but errors at runtime:

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Item {
    id: u32,
    label: String,
}

fn main() {
    let items = vec![
        Item { id: 1, label: "a".into() },
        Item { id: 2, label: "b".into() },
    ];
    // runtime error: TOML's root must be a table, not an array
    match toml::to_string(&items) {
        Ok(s) => println!("{s}"),
        Err(e) => println!("error: {e}"),
    }
}
```

The real output from `cargo run`:

```text
error: unsupported array type
```

The fix is to wrap the array in a struct field, e.g. `struct Catalog { items: Vec<Item> }`, which serializes the array under an `[[items]]` array-of-tables. JSON and YAML have no such restriction — only TOML requires a table at the root.

### CSV cannot represent nested structs

CSV is flat. If a struct field is itself a struct (or a `Vec`/`HashMap`), the `csv` crate cannot lay it out as columns and refuses when it tries to write the header. This compiles but errors at runtime:

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Address {
    city: String,
    zip: String,
}

#[derive(Serialize)]
struct Person {
    name: String,
    address: Address, // nested struct: CSV has no column layout for this
}

fn main() {
    let p = Person {
        name: "Ada".into(),
        address: Address { city: "London".into(), zip: "EC1A".into() },
    };
    let mut wtr = csv::Writer::from_writer(vec![]);
    match wtr.serialize(&p) {
        Ok(()) => println!("ok"),
        Err(e) => println!("error: {e}"),
    }
}
```

The real output from `cargo run`:

```text
error: CSV write error: cannot serialize Address container inside struct when writing headers from structs
```

Keep CSV records flat: use only scalar fields (`String`, numbers, `bool`). If you need nesting, flatten it manually (e.g. `city: String`, `zip: String` directly on `Person`) or choose a tree format like JSON.

### MessagePack: compact (array) form drops field names

`rmp_serde::to_vec` produces the *compact* encoding, which stores struct fields **positionally as an array**. Field names are not written. That is smaller, but it means the decoder must use the exact same field order. `to_vec_named` writes a *map* with field names, which is larger but order-independent and interoperable with other MessagePack libraries that expect named fields. Mixing them silently scrambles data rather than erroring:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct V1 {
    a: u32,
    b: u32,
    c: u32,
}

// Same fields, declared in a DIFFERENT order:
#[derive(Debug, Deserialize)]
struct V2 {
    c: u32,
    b: u32,
    a: u32,
}

fn main() {
    let v1 = V1 { a: 1, b: 2, c: 3 };

    // Compact: positions are matched, names ignored -> WRONG mapping
    let compact = rmp_serde::to_vec(&v1).unwrap();
    let from_compact: V2 = rmp_serde::from_slice(&compact).unwrap();
    println!("compact -> {from_compact:?}");

    // Named: fields matched by name -> correct
    let named = rmp_serde::to_vec_named(&v1).unwrap();
    let from_named: V2 = rmp_serde::from_slice(&named).unwrap();
    println!("named   -> {from_named:?}");
}
```

The real output from `cargo run`:

```text
compact -> V2 { c: 1, b: 2, a: 3 }
named   -> V2 { c: 3, b: 2, a: 1 }
```

The compact decode put `1` into `c` purely by position — no error, just wrong data. Use `to_vec_named` whenever the reader and writer might disagree on field order, or when talking to a non-Rust MessagePack consumer.

### bincode: decode with the same config you encoded with

bincode 2.x makes the configuration explicit, and it is **not** stored in the bytes. If you encode with `standard()` (variable-length integers) and decode with `with_fixed_int_encoding()`, the bytes are misinterpreted. Always thread the same `config` value through both calls:

```rust
let config = bincode::config::standard();
let bytes = bincode::serde::encode_to_vec(&value, config).unwrap();
// Must decode with the SAME config:
let (decoded, _len): (MyType, usize) =
    bincode::serde::decode_from_slice(&bytes, config).unwrap();
```

This is also why bincode is best for Rust-to-Rust links you control end to end, not as a public interchange format.

---

## Best Practices

- **Match the format to the job.** Human-edited config → TOML (or YAML if your platform expects it). Public/browser-facing APIs → JSON. Bandwidth-sensitive service-to-service traffic → MessagePack. Rust-only caches, snapshots, or IPC where you own both ends → bincode. Tabular exports for spreadsheets → CSV.

- **Use `serde_norway`, not `serde_yaml`.** The latter is deprecated; the former is the maintained drop-in replacement with the same API.

- **Prefer `to_vec_named` for MessagePack across boundaries.** The few extra bytes buy field-name interoperability and immunity to field-reordering bugs. Reserve the compact `to_vec` for internal, version-locked links.

- **Pin bincode to 2.x with the `serde` feature.** That is the established, maintained release line for Serde integration (`cargo add bincode@2 --features serde`). Store the `config` value once and reuse it for both encode and decode.

- **Keep CSV records flat.** Use scalar fields only. For optional columns, prefer `Option<T>` plus the `csv` reader's flexible handling, and lean on Serde attributes like `#[serde(rename)]` to match header names — see [Serde Attributes](/15-serialization/05-attributes/).

- **Read from bytes/readers for large inputs.** Text crates offer `from_reader`/`to_writer` and the binary crates work directly on `&[u8]`; streaming avoids buffering a whole file as a `String`. Performance trade-offs (borrowing, zero-copy, buffer reuse) are covered in [Serde Performance](/15-serialization/08-performance/).

- **Let the same struct serve every format.** Define your data types once and switch formats at the call site. Resist duplicating types per format; that defeats Serde's central advantage.

---

## Real-World Example

A common production need is a config loader that accepts the same settings as JSON, TOML, or YAML and picks the parser by file extension, so ops can write `config.yaml` while a test fixture uses `config.json`, all loading into one typed struct. This complete `src/main.rs` writes the config in three formats, loads each back, and returns a typed error for an unsupported extension:

```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct AppConfig {
    service_name: String,
    port: u16,
    log_level: String,
    features: Vec<String>,
}

#[derive(Debug)]
enum ConfigError {
    Io(std::io::Error),
    UnknownFormat(String),
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "io error: {e}"),
            ConfigError::UnknownFormat(ext) => write!(f, "unsupported config extension: .{ext}"),
            ConfigError::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}
impl std::error::Error for ConfigError {}
impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

/// Load a config file, choosing the parser from its extension.
fn load_config(path: &Path) -> Result<AppConfig, ConfigError> {
    let text = fs::read_to_string(path)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "json" => serde_json::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string())),
        "toml" => toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string())),
        "yaml" | "yml" => {
            serde_norway::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))
        }
        other => Err(ConfigError::UnknownFormat(other.to_string())),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir();
    let default = AppConfig {
        service_name: "billing".into(),
        port: 9000,
        log_level: "info".into(),
        features: vec!["metrics".into(), "tracing".into()],
    };

    // Write the SAME config in three formats from one struct.
    let json_path = dir.join("cfg.json");
    let toml_path = dir.join("cfg.toml");
    let yaml_path = dir.join("cfg.yaml");
    fs::write(&json_path, serde_json::to_string_pretty(&default)?)?;
    fs::write(&toml_path, toml::to_string(&default)?)?;
    fs::write(&yaml_path, serde_norway::to_string(&default)?)?;

    // Load each back -- all yield an identical, fully-typed AppConfig.
    for path in [&json_path, &toml_path, &yaml_path] {
        let loaded = load_config(path)?;
        println!(
            "{:<4} -> service={} port={} (matches default: {})",
            path.extension().unwrap().to_str().unwrap(),
            loaded.service_name,
            loaded.port,
            loaded == default
        );
    }

    // An unsupported extension is a clean typed error, not a panic.
    let bad = dir.join("cfg.ini");
    fs::write(&bad, "x=1")?;
    if let Err(e) = load_config(&bad) {
        println!("expected failure: {e}");
    }

    for p in [json_path, toml_path, yaml_path, bad] {
        fs::remove_file(p).ok();
    }
    Ok(())
}
```

Real output from `cargo run`:

```text
json -> service=billing port=9000 (matches default: true)
toml -> service=billing port=9000 (matches default: true)
yaml -> service=billing port=9000 (matches default: true)
expected failure: unsupported config extension: .ini
```

One `AppConfig` type, three text formats, and a single `load_config` that dispatches on the extension. Each parser converts its own error into the unified `ConfigError`, so the `?` operator in `load_config` flows file-I/O and parse failures through one channel — and an unknown extension produces a descriptive, typed error instead of a crash. The struct definition is the only schema; the format is just a detail at the edge.

---

## Further Reading

- [TOML format spec](https://toml.io/) and the [`toml` crate docs](https://docs.rs/toml/): the format Rust itself uses for `Cargo.toml`.
- [`serde_norway` crate docs](https://docs.rs/serde_norway/): maintained YAML support (the successor to the deprecated `serde_yaml`).
- [`rmp-serde` crate docs](https://docs.rs/rmp-serde/): MessagePack via Serde, including `to_vec` vs `to_vec_named`.
- [`bincode` crate docs](https://docs.rs/bincode/): Rust-native binary format; see `config` and the `serde` module.
- [`csv` crate docs](https://docs.rs/csv/) and [the csv crate tutorial](https://docs.rs/csv/latest/csv/tutorial/index.html): reading and writing tabular data with Serde.
- [Serde data formats list](https://serde.rs/#data-formats): every format crate that plugs into Serde.
- [Serde](/15-serialization/00-serde-intro/): the data-model architecture that decouples types from formats.
- [Serde Basics](/15-serialization/01-serde-basics/): the `to_string` / `from_str` round-trip with JSON.
- [Structs and JSON](/15-serialization/03-json/): mapping nested structs, `Vec`, `HashMap`, `Option`, and enums.
- [Serde Attributes](/15-serialization/05-attributes/): `rename`, `default`, `flatten`, and other attributes that also apply to these formats.
- [Serde Performance](/15-serialization/08-performance/): borrowing, zero-copy, streaming, and buffer reuse across formats.
- [Section 01: Getting Started](/01-getting-started/) — `cargo new`, `cargo add`, project layout.
- [Section 02: Basics](/02-basics/) — the scalar and collection types these structs are built from.
- [Section 16: Web APIs](/16-web-apis/) — where these serialized payloads meet HTTP request and response bodies.

---

## Exercises

### Exercise 1: One struct, five formats

**Difficulty:** Easy

**Objective:** Confirm a single type round-trips correctly through four formats.

**Instructions:** In a fresh project (`cargo new`, then add `serde --features derive`, `serde_json`, `toml`, `serde_norway`, and `rmp-serde`), define `Point { x: i32, y: i32, label: String }` deriving `Serialize`, `Deserialize`, `Debug`, and `PartialEq`. Serialize one `Point` to JSON, TOML, YAML, and MessagePack (use `to_vec_named` for MessagePack), deserialize each back, and `assert!` that all four parsed values equal the original.

```rust
use serde::{Deserialize, Serialize};

// TODO: derive Serialize, Deserialize, Debug, PartialEq
struct Point {
    x: i32,
    y: i32,
    label: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let p = Point { x: 3, y: -7, label: "origin-ish".into() };
    // TODO: round-trip through JSON, TOML, YAML, MessagePack and assert equality
    Ok(())
}
```

<details>
<summary>Solution</summary>

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Point {
    x: i32,
    y: i32,
    label: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let p = Point {
        x: 3,
        y: -7,
        label: "origin-ish".into(),
    };

    let from_json: Point = serde_json::from_str(&serde_json::to_string(&p)?)?;
    let from_toml: Point = toml::from_str(&toml::to_string(&p)?)?;
    let from_yaml: Point = serde_norway::from_str(&serde_norway::to_string(&p)?)?;
    let from_mp: Point = rmp_serde::from_slice(&rmp_serde::to_vec_named(&p)?)?;

    assert_eq!(from_json, p);
    assert_eq!(from_toml, p);
    assert_eq!(from_yaml, p);
    assert_eq!(from_mp, p);

    println!("all four formats round-tripped equal");
    Ok(())
}
```

Real output from `cargo run`:

```text
all four formats round-tripped equal
```

The `Box<dyn std::error::Error>` return type lets the different error types from each format crate flow through the same `?`. `PartialEq` is what makes the four `assert_eq!` calls compile and pass.

</details>

### Exercise 2: Parse a TOML config with a nested table

**Difficulty:** Medium

**Objective:** Read a real-world-shaped TOML config that includes a `[table]` section, then re-serialize it.

**Instructions:** Define `Server { host: String, port: u16 }` and `Settings { title: String, server: Server }` (derive `Serialize`, `Deserialize`, `Debug`). Parse the TOML literal below into a `Settings`, print the host and port, then serialize it back to TOML and print the result. Observe how the nested struct becomes a `[server]` table.

```rust
use serde::{Deserialize, Serialize};

// TODO: define Server and Settings with the right derives

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let toml_in = r#"
title = "prod"

[server]
host = "0.0.0.0"
port = 8080
"#;
    // TODO: parse into Settings, print host:port, re-serialize to TOML
    Ok(())
}
```

<details>
<summary>Solution</summary>

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Server {
    host: String,
    port: u16,
}

#[derive(Debug, Serialize, Deserialize)]
struct Settings {
    title: String,
    server: Server,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let toml_in = r#"
title = "prod"

[server]
host = "0.0.0.0"
port = 8080
"#;

    let settings: Settings = toml::from_str(toml_in)?;
    println!(
        "{} -> {}:{}",
        settings.title, settings.server.host, settings.server.port
    );

    let back = toml::to_string(&settings)?;
    println!("reserialized:\n{back}");
    Ok(())
}
```

Real output from `cargo run`:

```text
prod -> 0.0.0.0:8080
reserialized:
title = "prod"

[server]
host = "0.0.0.0"
port = 8080
```

The nested `Server` struct maps to TOML's `[server]` table both ways. Notice that the scalar `title` is emitted before the table — the `toml` crate orders output so that all top-level keys precede any tables, which is required by the TOML grammar.

</details>

### Exercise 3: Summarize a CSV file

**Difficulty:** Medium

**Objective:** Deserialize CSV rows into typed records and compute a summary.

**Instructions:** Define `Sale { product: String, units: u32, revenue: f64 }` deriving `Deserialize` and `Debug`. Using `csv::Reader::from_reader`, deserialize the CSV string below into `Sale` records, then print the total revenue and the single product with the highest revenue. Each row deserializes into a fully-typed `Sale`, no manual string-to-number parsing.

```rust
use serde::Deserialize;

// TODO: define Sale with derive(Deserialize, Debug)

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let csv_in = "\
product,units,revenue
widget,120,2400.0
gadget,80,3200.0
gizmo,45,900.0
";
    // TODO: deserialize rows, print total revenue and the top product
    Ok(())
}
```

<details>
<summary>Solution</summary>

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Sale {
    product: String,
    units: u32,
    revenue: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let csv_in = "\
product,units,revenue
widget,120,2400.0
gadget,80,3200.0
gizmo,45,900.0
";

    let mut rdr = csv::Reader::from_reader(csv_in.as_bytes());
    let mut total = 0.0;
    let mut best: Option<(String, f64)> = None;

    for record in rdr.deserialize() {
        let sale: Sale = record?;
        total += sale.revenue;
        if best.as_ref().map_or(true, |(_, r)| sale.revenue > *r) {
            best = Some((sale.product.clone(), sale.revenue));
        }
        let _ = sale.units; // available if you want a per-unit figure
    }

    let (top_product, top_revenue) = best.expect("at least one row");
    println!("total revenue = {total}");
    println!("top product = {top_product} ({top_revenue})");
    Ok(())
}
```

Real output from `cargo run`:

```text
total revenue = 6500
top product = gadget (3200)
```

`csv::Reader` reads the header row to map columns to the `Sale` fields by name, and each `record?` yields a typed `Sale` with `units` already parsed as `u32` and `revenue` as `f64`. The `map_or(true, ...)` keeps the first row as the initial "best" and then updates it whenever a higher-revenue row appears.

</details>
