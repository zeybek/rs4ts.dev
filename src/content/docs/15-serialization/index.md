---
title: "Rust Serialization with Serde"
sidebar:
  label: "Overview"
description: "TypeScript ships JSON.parse and JSON.stringify; Rust uses Serde, splitting data types from formats so one annotated struct targets JSON, TOML, YAML, and more."
---

In TypeScript, turning data into JSON and back is built into the language: `JSON.parse` and `JSON.stringify` are always there. Rust takes a different route: serialization lives in a single, dominant framework called **Serde** (a contraction of **ser**ialize/**de**serialize). Instead of one hard-coded JSON object, Serde splits the problem into **data types** (your structs and enums, which implement the `Serialize`/`Deserialize` traits) and **data formats** (JSON, TOML, YAML, MessagePack, …, each a separate crate) that meet through a shared **data model**. The payoff: you annotate a type once and get every format for free. And unlike `JSON.parse`, deserialization validates against a real type and returns a `Result` instead of an unchecked `any`. This section takes you from that mental model through derive macros, attributes, dynamic JSON, alternative formats, fully hand-written (de)serialization, and performance.

---

## What You'll Learn

- How `JSON.parse`/`JSON.stringify` map onto Serde's `Serialize`/`Deserialize` traits, and the **data-model architecture** that lets one type target every format
- How to set up `serde` (with `features = ["derive"]`) and `serde_json`, and round-trip values with `to_string`/`from_str`
- What `#[derive(Serialize, Deserialize)]` actually generates for structs and enums
- How structs map to JSON, including nested types, `Vec`/`HashMap`, `Option` fields, and the four enum representations (externally/internally/adjacently tagged and untagged)
- How to work with dynamic, schema-less JSON via `serde_json::Value` and the `json!` macro — and when to prefer typed structs instead
- The common Serde attributes — `rename`, `rename_all`, `skip`, `skip_serializing_if`, `default`, `flatten`, `tag`, `with` — and what each controls
- How the same derived type serializes to TOML, YAML, MessagePack, bincode, and CSV through the Serde ecosystem
- How to hand-write `Serialize`/`Deserialize`, use `serialize_with`/`deserialize_with`, and apply remote derive for types you do not own
- How to make serialization fast: borrowing (`&str`, `#[serde(borrow)]`), zero-copy parsing, streaming, avoiding `Value`, and reusing buffers

---

## Topics

| Topic | Description |
| --- | --- |
| [Serde Intro](/15-serialization/00-serde-intro/) | `JSON.parse`/`JSON.stringify` → Serde; the `Serialize`/`Deserialize` traits and the data-model architecture (data types ↔ formats). |
| [Serde Basics](/15-serialization/01-serde-basics/) | Setting up `serde` (`features = ["derive"]`) and `serde_json`; the `to_string`/`from_str` round-trip, compile-verified. |
| [Deriving Serialize/Deserialize](/15-serialization/02-derive-serialize/) | `#[derive(Serialize, Deserialize)]` on structs and enums, and what the macros generate. |
| [Structs and JSON](/15-serialization/03-json/) | Structs ↔ JSON: nested types, `Vec`/`HashMap`, `Option` fields, and the four enum representations. |
| [Dynamic JSON](/15-serialization/04-json-manipulation/) | Schema-less JSON with `serde_json::Value`, the `json!` macro, indexing, and when to use `Value` vs typed structs. |
| [Serde Attributes](/15-serialization/05-attributes/) | `#[serde(rename, rename_all, skip, skip_serializing_if, default, flatten, tag, with)]` and the other everyday attributes. |
| [Other Formats](/15-serialization/06-other-formats/) | The same data in other formats: TOML, YAML (`serde_norway`), MessagePack (`rmp-serde`), bincode, and CSV. |
| [Custom Serialization](/15-serialization/07-custom-serialization/) | Hand-written `Serialize`/`Deserialize`, `serialize_with`/`deserialize_with`, and remote derive. |
| [Performance](/15-serialization/08-performance/) | Borrowing (`&str`, `#[serde(borrow)]`), zero-copy, streaming, avoiding `Value`, and buffer reuse. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Explain how Serde's data-type/data-format split differs from TypeScript's single built-in `JSON` object, and why it makes types format-agnostic
- Add and configure `serde` and `serde_json`, and round-trip your own types with `to_string`, `to_string_pretty`, and `from_str`
- Derive `Serialize`/`Deserialize` for structs and enums and reason about the code the macros emit
- Map realistic API payloads — nested objects, collections, optional fields, and discriminated unions — to Rust types and back
- Manipulate untyped JSON with `Value` and the `json!` macro, and decide deliberately when typed structs are the better tool
- Reshape the wire format with Serde attributes without changing your idiomatic Rust field names
- Serialize the same type to TOML, YAML, MessagePack, bincode, and CSV by swapping the format crate
- Write `Serialize`/`Deserialize` by hand when the derive is not enough, including for types you do not own
- Profile and tune serialization with borrowing, zero-copy, streaming, and buffer reuse

---

## Prerequisites

- [Section 08: Error Handling](/08-error-handling/): Serde's `from_str`/`to_string` return `Result`, and deserialization errors are values you handle with `?`, `match`, or anyhow/thiserror. The error-handling vocabulary from Section 08 is assumed throughout.
- Helpful but not required: [Section 09: Generics & Traits](/09-generics-traits/): `Serialize`/`Deserialize` are traits, and the derive vs. hand-written distinction lands better once trait basics and the orphan rule are familiar.

---

## Estimated Time

- **Reading:** 4-5 hours
- **Hands-on Practice:** 3 hours
- **Exercises:** 2 hours
- **Total:** 8-10 hours

> **Tip:** Read `serde-intro` → `serde-basics` → `derive-serialize` → `json` as a single connected track. That covers the 90% case of typed JSON. Pull in `json-manipulation` when you need untyped JSON and `attributes` when the wire format and your Rust names diverge. Treat `other-formats`, `custom-serialization`, and `performance` as a toolbox to reach for when a specific need arises, rather than something to master up front.


---

## Frequently asked questions

### How do I parse and produce JSON in Rust?

With Serde. Derive `Serialize` and `Deserialize` on a struct, then `serde_json::from_str` parses and `serde_json::to_string` produces JSON, type-checked against your struct instead of `any`. See [JSON with Serde](/15-serialization/03-json/) and [Serde Basics](/15-serialization/01-serde-basics/).

### How do I match JavaScript's camelCase keys?

Rust fields are snake_case, so add `#[serde(rename_all = "camelCase")]` to the struct, or `#[serde(rename = "…")]` on a single field, to bridge Rust naming and your JSON contract. See [Serde Attributes](/15-serialization/05-attributes/).

### Can Serde handle formats other than JSON?

Yes. The same derives work with `serde_norway` (the maintained YAML crate, since `serde_yaml` is deprecated), `toml`, `bincode`, and `rmp-serde` (MessagePack) by swapping the format crate; the struct definition stays the same. See [Other Formats](/15-serialization/06-other-formats/).

---

**Next:** [Section 16: Web APIs →](/16-web-apis/) — building HTTP clients and servers, where Serde does the request/response (de)serialization you just learned.
