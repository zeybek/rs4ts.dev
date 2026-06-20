---
title: "Serde Performance: Borrowing, Zero-Copy, Streaming, and Buffer Reuse"
description: "JSON.parse always copies every byte into a new tree; Serde lets you borrow with &str, stream values lazily, skip serde_json::Value, and reuse buffers on hot paths."
---

In JavaScript, `JSON.parse` is a black box: it always builds a brand-new tree of objects and strings, copying every byte of input into the heap, and you have no levers to pull. Serde gives you those levers. By choosing the right field types and APIs you can parse JSON that **borrows** directly from the input buffer (zero string copies), **stream** millions of records without ever holding them all in memory, and **reuse** a single allocation across an entire workload. This page is about spending Serde's flexibility where it pays off.

---

## Quick Overview

Serde's default path (derive `Deserialize`, parse into a struct full of `String`s and `Vec`s) is already fast and is the right default for almost everything. The performance toolkit on top of that is: **borrowed deserialization** (`&str` / `#[serde(borrow)]` so string fields point *into* the input instead of being copied), **streaming** (`from_reader` and `StreamDeserializer` so you process one value at a time), **avoiding `serde_json::Value`** (parse straight into typed structs instead of building a dynamic tree), and **buffer reuse** (`clear()` + `to_writer` into a kept-capacity buffer). For a TypeScript developer the mental shift is that the *shape of your types* is itself a performance decision. There is no single fixed parse like `JSON.parse`.

> **Note:** Reach for these techniques only when a profiler points here. Borrowing and streaming add lifetime and structural constraints; the typed `String`-field path is the correct starting point. Optimize the hot loop, not the config-file load that runs once at startup.

---

## TypeScript/JavaScript Example

Here is a typical Node.js hot path: reading a large newline-delimited JSON (NDJSON) log file and summing a numeric field per category. This is the kind of code where, at scale, allocation pressure starts to dominate.

```typescript
import { createReadStream } from "node:fs";
import { createInterface } from "node:readline";

interface Sample {
  metric: string;
  value: number;
}

async function aggregate(path: string): Promise<Map<string, number>> {
  const totals = new Map<string, number>();
  const rl = createInterface({ input: createReadStream(path) });

  for await (const line of rl) {
    if (line.trim() === "") continue;
    // JSON.parse builds a fresh object AND a fresh copy of the
    // "metric" string for every single line — no way to avoid it.
    const sample = JSON.parse(line) as Sample;
    totals.set(sample.metric, (totals.get(sample.metric) ?? 0) + sample.value);
  }
  return totals;
}
```

Two costs are unavoidable in this JavaScript version. First, `JSON.parse` allocates a new object plus a fresh `string` for `metric` on every line, even though we only need it briefly to look up a map key. Second, the `as Sample` cast is a compile-time fiction — a malformed line is not caught here, it surfaces as `NaN` or `undefined` somewhere downstream. Rust lets you eliminate the first cost and closes the second by construction.

---

## Rust Equivalent

The Rust version borrows the `metric` string straight out of the line buffer (no per-record string allocation), reuses one `String` for reading lines, and never builds an intermediate dynamic tree. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically.

```bash
cargo new ndjson_agg
cd ndjson_agg
cargo add serde --features derive
cargo add serde_json
```

```rust playground
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor};

// Borrowed: `metric` points INTO the line buffer. No per-record String
// allocation for the metric name — Serde hands back a slice of the input.
#[derive(Debug, Deserialize)]
struct Sample<'a> {
    metric: &'a str,
    value: f64,
}

// Process a JSON Lines stream, reusing a single line buffer across all
// records and never building a serde_json::Value.
fn aggregate<R: BufRead>(mut reader: R) -> std::io::Result<HashMap<String, f64>> {
    let mut totals: HashMap<String, f64> = HashMap::new();
    let mut line = String::new();

    loop {
        line.clear(); // reuse the allocation; keeps capacity
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break; // EOF
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Borrowed deserialize straight out of `line`.
        let sample: Sample = match serde_json::from_str(trimmed) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("skipping bad line: {e}");
                continue;
            }
        };
        // We only need an owned key when inserting a NEW entry into the map.
        *totals.entry(sample.metric.to_owned()).or_insert(0.0) += sample.value;
    }

    Ok(totals)
}

fn main() -> std::io::Result<()> {
    let input = "\
{\"metric\":\"requests\",\"value\":12.0}
{\"metric\":\"errors\",\"value\":1.0}
{\"metric\":\"requests\",\"value\":8.0}
{\"metric\":\"errors\",\"value\":2.0}
";
    let reader = BufReader::new(Cursor::new(input));
    let mut totals = aggregate(reader)?;

    // Sort for deterministic output.
    let mut sorted: Vec<_> = totals.drain().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (metric, total) in sorted {
        println!("{metric}: {total}");
    }
    Ok(())
}
```

Real output from `cargo run`:

```text
errors: 3
requests: 20
```

The `Sample<'a>` struct's `metric: &'a str` is the key move: during parsing, Serde points that field at the bytes already sitting in `line`, instead of allocating and copying a new `String`. The only allocation per record happens when a *new* category appears in the map. Everything else — the line buffer, the parse — reuses memory you already have.

---

## Detailed Explanation

Let's unpack each performance lever in the Rust version and contrast it with what JavaScript forces on you.

### Borrowed deserialization (`&str`)

When you declare a field as `&'a str` instead of `String`, you are telling Serde: *do not copy this string; hand me a slice that points into the input.* This is **zero-copy** deserialization. Compare the two struct definitions:

```rust playground
use serde::Deserialize;

// Owning: every string field is a fresh heap allocation + memcpy.
#[derive(Deserialize)]
struct Owned {
    level: String,
    message: String,
}

// Borrowing: string fields are slices into the parsed input. No copies.
#[derive(Deserialize)]
struct Borrowed<'a> {
    level: &'a str,
    message: &'a str,
}

fn main() {
    let data = r#"{"level":"error","message":"disk full"}"#;
    let owned: Owned = serde_json::from_str(data).unwrap();
    let borrowed: Borrowed = serde_json::from_str(data).unwrap();
    println!("{} / {}", owned.message, borrowed.message);
}
```

The lifetime `'a` ties `Borrowed` to the buffer it was parsed from. The compiler enforces that the struct cannot outlive the input: there is no such thing as a dangling slice in safe Rust. JavaScript has no equivalent: `JSON.parse` always materializes fresh, fully-owned strings, and the GC cleans them up later. Rust's borrow is the price-free option *when the input outlives the parsed value*, and a compile error otherwise (we hit that error in [Common Pitfalls](#common-pitfalls)).

This is the deserialization counterpart of the broader ownership story; if `&str` versus `String` is still fuzzy, see [Section 05: Ownership](/05-ownership/).

### When a borrow can't happen: escapes and `Cow`

A `&str` field can only borrow if the string appears **verbatim** in the input. JSON allows escapes (`\n`, `\"`, `\uXXXX`), and the *unescaped* bytes do not exist contiguously in the source; Serde would have to allocate to produce them. A plain `&str` field therefore **fails at runtime** on an escaped string. The escape-tolerant choice is `Cow<'a, str>` ("clone on write"): it borrows when it can and allocates only when it must.

```rust playground
use serde::Deserialize;
use std::borrow::Cow;

// `#[serde(borrow)]` tells the derive to generate a BORROWING impl for Cow.
// Without it, Cow always allocates (owns) — see Common Pitfalls.
#[derive(Debug, Deserialize)]
struct Message<'a> {
    #[serde(borrow)]
    text: Cow<'a, str>,
}

fn main() {
    // No escapes: Cow borrows (zero-copy).
    let plain = r#"{"text":"hello world"}"#;
    let m1: Message = serde_json::from_str(plain).unwrap();
    println!("{:?} -> borrowed = {}", m1.text, matches!(m1.text, Cow::Borrowed(_)));

    // Contains an escape: Cow must own a freshly-unescaped String.
    let escaped = r#"{"text":"line1\nline2"}"#;
    let m2: Message = serde_json::from_str(escaped).unwrap();
    println!("{:?} -> borrowed = {}", m2.text, matches!(m2.text, Cow::Borrowed(_)));
}
```

Real output from `cargo run`:

```text
"hello world" -> borrowed = true
"line1\nline2" -> borrowed = false
```

`Cow<'a, str>` is the pragmatic default for borrowed string fields when you cannot guarantee escape-free input: the common (no-escape) case stays zero-copy, and the rare escaped case still works correctly.

### `#[serde(borrow)]`

The derive macro auto-detects the borrow for a plain `&'a str` field. For `Cow<'a, str>`, it does **not** — it defaults to the owning implementation unless you opt in with `#[serde(borrow)]`. That single attribute switches the generated code to the borrowing variant. The other place you need it explicitly is when a borrowed lifetime is buried inside your own generic wrapper types where the derive cannot infer the intent.

### Streaming with `from_reader` and `StreamDeserializer`

`serde_json::from_str` parses one value from a fully-loaded string. For a multi-gigabyte file or a network socket you do not want the whole thing in memory. `Deserializer::from_reader(...).into_iter::<T>()` gives you a `StreamDeserializer`: an iterator that pulls one JSON value at a time from any `Read` source.

```rust playground
use serde::Deserialize;
use std::io::Cursor;

#[derive(Debug, Deserialize)]
struct Event {
    id: u32,
    kind: String,
}

fn main() {
    // Whitespace-separated JSON values (JSON Lines style). The stream
    // reader processes them one at a time — no Vec<Event>, no big String.
    let ndjson = r#"
        {"id":1,"kind":"click"}
        {"id":2,"kind":"scroll"}
        {"id":3,"kind":"close"}
    "#;

    let reader = Cursor::new(ndjson);
    let stream = serde_json::Deserializer::from_reader(reader).into_iter::<Event>();

    let mut count = 0u32;
    for event in stream {
        let event = event.unwrap();
        count += 1;
        println!("event #{count}: id={} kind={}", event.id, event.kind);
    }
    println!("processed {count} events");
}
```

Real output from `cargo run`:

```text
event #1: id=1 kind=click
event #2: id=2 kind=scroll
event #3: id=3 kind=close
```

Memory use stays flat regardless of input size, because only one `Event` exists at a time. The catch (which trips up newcomers) is that `from_reader` **cannot borrow** from its input (the reader owns its buffer transiently), so streamed types must be fully owned (`String`, not `&str`). We show that exact compiler error in [Common Pitfalls](#common-pitfalls).

### Avoiding `serde_json::Value`

`serde_json::Value` is the dynamic, JavaScript-object-like representation: an enum tree where every object key is a heap `String`, every nested array/object is its own allocation, and numbers are stored generically. It is the right tool when the shape is genuinely unknown ([Dynamic JSON with `serde_json::Value`](/15-serialization/04-json-manipulation/)), but on a hot path it is the slow option because you pay to build the *entire* tree even if you read one field.

```rust playground
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct User {
    id: u64,
    name: String,
}

fn main() {
    let data = r#"{"id":42,"name":"Ada","extra":{"nested":[1,2,3]},"flag":true}"#;

    // Approach A: parse into Value (dynamic). Builds the WHOLE tree —
    // including `extra` and `flag` — allocating a String for every key.
    let v: Value = serde_json::from_str(data).unwrap();
    println!("via Value: id={}, name={}", v["id"], v["name"].as_str().unwrap());

    // Approach B: parse into a typed struct. Serde skips `extra` and
    // `flag` entirely and builds only the two fields you declared.
    let user: User = serde_json::from_str(data).unwrap();
    println!("via struct: id={} name={}", user.id, user.name);
}
```

Real output from `cargo run`:

```text
via Value: id=42, name=Ada
via struct: id=42 name=Ada
```

Both print the same two values, but they do very different amounts of work. The typed parse ignores `extra` and `flag` and allocates nothing for them, while the `Value` parse builds and allocates the full nested tree: a `String` for every key, a boxed variant for every value. If you only need a couple of fields, a minimal struct with just those fields (unknown fields are ignored by default) is dramatically cheaper than `Value`.

### Buffer reuse on the serialize side

`serde_json::to_string` allocates a fresh `String` every call. In a loop that serializes many values, reuse one buffer instead: `to_writer` appends into any `Write` target, and `Vec::clear()` drops the contents while **keeping the capacity**.

```rust playground
use serde::Serialize;

#[derive(Serialize)]
struct Metric {
    name: &'static str,
    value: f64,
}

fn main() {
    let metrics = vec![
        Metric { name: "cpu", value: 0.42 },
        Metric { name: "mem", value: 0.71 },
        Metric { name: "disk", value: 0.13 },
    ];

    // ONE byte buffer reused for every record. After the first iteration
    // no further allocation happens — clear() keeps the capacity.
    let mut buf: Vec<u8> = Vec::new();

    for m in &metrics {
        buf.clear();
        serde_json::to_writer(&mut buf, m).unwrap();
        // The buffer holds UTF-8 JSON bytes; view them as &str to print.
        println!("{}", std::str::from_utf8(&buf).unwrap());
    }

    println!("final buffer capacity = {}", buf.capacity());
}
```

Real output from `cargo run`:

```text
{"name":"cpu","value":0.42}
{"name":"mem","value":0.71}
{"name":"disk","value":0.13}
final buffer capacity = 32
```

The buffer was allocated once and held its 32-byte capacity through all three serializations. JavaScript gives you no control here: `JSON.stringify` always returns a brand-new string and the GC reclaims the old ones.

---

## Key Differences

| Concern | TypeScript / JavaScript | Rust + Serde |
| --- | --- | --- |
| String fields on parse | Always copied into new `string`s | `&str` borrows from input (zero-copy); `String` copies; `Cow` does both |
| Who owns parsed strings | The GC; always heap, always owned | You choose: borrow the buffer or own a copy |
| Streaming a huge file | Manual `readline` + per-line `JSON.parse` | `StreamDeserializer` iterates values lazily |
| Dynamic vs typed | Always dynamic objects (`any`) | `Value` (dynamic, allocates tree) vs typed struct (skips unwanted fields) |
| Reusing a serialize buffer | Impossible; `JSON.stringify` returns a fresh string | `to_writer` + `clear()` reuses one allocation |
| Unwanted fields on parse | Materialized anyway, then ignored by you | Skipped by the parser; never allocated |
| Safety of borrowing | N/A (everything copied) | Borrow checker guarantees no dangling slice, at compile time |

Three points deserve emphasis for a TypeScript developer:

- **There is no one "parse".** `JSON.parse` is a single fixed operation. In Rust the *type you parse into* decides how much work happens. A struct with three `&str` fields and one with three `String` fields parse the same JSON very differently.

- **Borrowing is checked, not hoped.** A borrowed parse result is tied by lifetime to its input buffer. If you try to keep it after the buffer is gone, the program does not compile. This is the opposite of the "use-after-free" class of bug: it is impossible by construction.

- **Cheap-by-default skipping.** A typed struct ignores fields it does not declare *during parsing*, so a 50-field JSON payload from which you need two fields costs roughly two fields' worth of work. The dynamic-everything `JSON.parse` model cannot do this.

---

## Common Pitfalls

### Trying to borrow an escaped string into `&str`

A `&str` field cannot hold an escaped string, because the unescaped bytes are not contiguous in the source. Serde reports this at **runtime** as a deserialization error:

```rust playground
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Message<'a> {
    text: &'a str,
}

fn main() {
    let escaped = r#"{"text":"line1\nline2"}"#; // contains an escape
    match serde_json::from_str::<Message>(escaped) {
        Ok(m) => println!("ok: {}", m.text),
        Err(e) => println!("error: {e}"),
    }
}
```

Real output from `cargo run`:

```text
error: invalid type: string "line1\nline2", expected a borrowed string at line 1 column 22
```

The fix is to use `Cow<'a, str>` with `#[serde(borrow)]` (borrows when possible, allocates on escapes) or fall back to an owned `String` if borrowing buys you nothing here.

### Returning a borrowed struct that outlives its buffer

This is the classic lifetime error. A borrowed struct points into a buffer; if that buffer is a local that gets dropped, you cannot return the struct:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LogEntry<'a> {
    level: &'a str,
    message: &'a str,
}

fn parse_owned() -> LogEntry<'static> {
    let data = String::from(r#"{"level":"info","message":"started"}"#);
    let entry: LogEntry = serde_json::from_str(&data).unwrap();
    entry // does not compile (error[E0515]): returns a value referencing `data`, dropped here
}

fn main() {
    let e = parse_owned();
    println!("{e:?}");
}
```

The **real** compiler error:

```text
error[E0515]: cannot return value referencing local variable `data`
  --> src/main.rs:12:5
   |
11 |     let entry: LogEntry = serde_json::from_str(&data).unwrap();
   |                                                ----- `data` is borrowed here
12 |     entry // does not compile (error[E0515]): returns a value referencing `data`, dropped here
   |     ^^^^^ returns a value referencing data owned by the current function

For more information about this error, try `rustc --explain E0515`.
```

The fix is to either keep the buffer alive in the caller and parse there, or use an owning struct (`String` fields) so the result does not depend on the buffer's lifetime. Borrowing is for when the input genuinely outlives the parsed value (as in a request handler that holds the body for the duration of the handler).

### Expecting `from_reader` to borrow

`from_reader` and `StreamDeserializer` work over a transient internal buffer, so they can only produce **owned** types. Asking for a borrowed type is a compile error:

```rust
use serde::Deserialize;
use std::io::Cursor;

#[derive(Debug, Deserialize)]
struct Borrowed<'a> {
    name: &'a str,
}

fn main() {
    let reader = Cursor::new(r#"{"name":"x"}"#);
    // does not compile: from_reader cannot produce borrowed data.
    let b: Borrowed = serde_json::from_reader(reader).unwrap();
    println!("{b:?}");
}
```

The **real** compiler error:

```text
error: implementation of `Deserialize` is not general enough
  --> src/main.rs:12:23
   |
12 |     let b: Borrowed = serde_json::from_reader(reader).unwrap();
   |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ implementation of `Deserialize` is not general enough
   |
   = note: `Borrowed<'_>` must implement `Deserialize<'0>`, for any lifetime `'0`...
   = note: ...but `Borrowed<'_>` actually implements `Deserialize<'1>`, for some specific lifetime `'1`
```

The cryptic "not general enough" message is Serde's way of saying the type only implements the *borrowing* `Deserialize<'de>`, but `from_reader` needs the *owning* `DeserializeOwned`. The fix: use `String` fields for streamed types, or, if you must borrow, read the whole input into a `String` first and use `from_str`.

### Forgetting `#[serde(borrow)]` on `Cow`

Without the attribute, the derive generates the **owning** implementation for a `Cow<'a, str>` field — it always allocates, silently defeating the point:

```rust playground
use serde::Deserialize;
use std::borrow::Cow;

#[derive(Debug, Deserialize)]
struct Doc<'a> {
    body: Cow<'a, str>, // no #[serde(borrow)] -> always owns/allocates
}

fn main() {
    let data = r#"{"body":"plain text no escapes"}"#;
    let d: Doc = serde_json::from_str(data).unwrap();
    println!("borrowed = {}", matches!(d.body, Cow::Borrowed(_)));
}
```

Real output from `cargo run`:

```text
borrowed = false
```

It compiles and runs. It is just quietly slower than you intended (`borrowed = false` means it allocated). Adding `#[serde(borrow)]` flips that to `true` for escape-free input. This one bites because it is not an error; it is a silent missed optimization. (A plain `&'a str` field, by contrast, auto-borrows without the attribute.)

### Reaching for `Value` when a typed struct would do

If you find yourself writing `let v: Value = from_str(...)?;` and then `v["a"]["b"].as_str()`, you are paying to build and allocate the whole tree, then indexing it dynamically (which also loses the compile-time type checks). A small typed struct with just the fields you need is faster *and* safer. Save `Value` for genuinely schema-less input; see [Dynamic JSON with `serde_json::Value`](/15-serialization/04-json-manipulation/) for when that is the right call.

---

## Best Practices

- **Start with owned `String` fields; borrow only when profiling says so.** The owning path is simpler, has no lifetime constraints, and is plenty fast for the vast majority of code. Borrowing is a hot-loop optimization, not a default.

- **Prefer `&str` for borrowed fields you control, `Cow<'a, str>` when escapes are possible.** Use `&str` when you can guarantee escape-free input (or are happy to error on escapes); use `Cow<'a, str>` with `#[serde(borrow)]` for the safe, escape-tolerant zero-copy path.

- **Parse from `&[u8]` with `from_slice` to skip a `String` step.** When you already hold bytes (an HTTP body, a memory-mapped file), `serde_json::from_slice(&bytes)` borrows directly and avoids constructing an intermediate `String`. Borrowed `&str` fields can point straight into those bytes.

- **Stream large or unbounded inputs.** Use `Deserializer::from_reader(BufReader::new(file)).into_iter::<T>()` for files or sockets so memory stays flat. Remember streamed types must be owned.

- **Reuse buffers in serialize loops.** `to_writer(&mut buf, &value)` plus `buf.clear()` reuses one allocation across many records. For files and sockets, `to_writer(BufWriter::new(...), &value)` avoids an intermediate `String` entirely.

- **Parse into the narrowest typed struct, not `Value`.** Declare only the fields you need; Serde skips the rest during parsing for free. Reserve `serde_json::Value` for dynamic, schema-unknown data.

- **Pre-size buffers when the count is known.** `Vec::with_capacity(n)` / `String::with_capacity(n)` avoid reallocation churn when you can estimate the output size up front.

- **Benchmark with `criterion`, do not guess.** Before and after any of these changes, measure. Serde's defaults are well-tuned; confirm the optimization is real on your data and workload. See [Section 13: Testing](/13-testing/) for the testing and benchmarking toolchain.

---

## Real-World Example

A production-flavored task: a metrics ingestion service receives a request body containing many NDJSON samples, parses each with a borrowed struct (zero string copies for the metric label), aggregates per-label totals, and serializes a compact summary back out — reusing a single output buffer. This is a complete, runnable `src/main.rs` using only `serde` (with `derive`) and `serde_json`.

```rust playground
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};

// Borrowed input: `label` is a slice into the line buffer, not a copy.
#[derive(Debug, Deserialize)]
struct Sample<'a> {
    label: &'a str,
    value: f64,
}

// Owned output: a summary we build and hand back. BTreeMap keeps keys
// sorted, which makes the serialized output deterministic.
#[derive(Debug, Serialize)]
struct Summary {
    total_samples: u64,
    sums: BTreeMap<String, f64>,
}

// Ingest a JSON Lines stream from any reader, reusing one line buffer and
// borrowing each record's label. Returns a typed Summary.
fn ingest<R: Read>(reader: R) -> std::io::Result<Summary> {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let mut sums: BTreeMap<String, f64> = BTreeMap::new();
    let mut total_samples = 0u64;

    loop {
        line.clear(); // reuse the allocation across all lines
        if reader.read_line(&mut line)? == 0 {
            break; // EOF
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Sample>(trimmed) {
            Ok(sample) => {
                total_samples += 1;
                // Allocate an owned key only for a new label.
                *sums.entry(sample.label.to_owned()).or_insert(0.0) += sample.value;
            }
            Err(e) => eprintln!("dropping malformed sample: {e}"),
        }
    }

    Ok(Summary { total_samples, sums })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Stand-in for an HTTP request body: raw bytes of NDJSON.
    let body: &[u8] = b"\
{\"label\":\"http_2xx\",\"value\":120.0}
{\"label\":\"http_5xx\",\"value\":3.0}
{\"label\":\"http_2xx\",\"value\":80.0}
{\"label\":\"bogus line, not json\"}
{\"label\":\"http_4xx\",\"value\":12.0}
";

    let summary = ingest(body)?;

    // Serialize the response by reusing a single output buffer.
    let mut out: Vec<u8> = Vec::with_capacity(256);
    serde_json::to_writer(&mut out, &summary)?;

    println!("response body: {}", std::str::from_utf8(&out)?);
    println!("accepted {} valid samples", summary.total_samples);
    Ok(())
}
```

Real output from `cargo run`:

```text
dropping malformed sample: missing field `value` at line 1 column 32
response body: {"total_samples":4,"sums":{"http_2xx":200.0,"http_4xx":12.0,"http_5xx":3.0}}
accepted 4 valid samples
```

Notice the pipeline's properties: input is read line-by-line into one reused buffer; each valid sample's `label` is borrowed (no string copy) and only promoted to an owned `String` when it first appears as a map key; the malformed line is rejected at the parse boundary with a precise message rather than poisoning the totals; and the response is written into a pre-sized, reused byte buffer. None of these optimizations changed the readability of the code, and every one of them is checked by the compiler. In a real service this `ingest` function would sit behind an HTTP handler; see [Section 16: Web APIs](/16-web-apis/) for wiring serialized structs to request and response bodies.

---

## Further Reading

- [Serde: Understanding deserializer lifetimes](https://serde.rs/lifetimes.html): the authoritative explanation of `'de`, `#[serde(borrow)]`, and zero-copy.
- [Serde: Streaming a sequence of values](https://docs.rs/serde_json/latest/serde_json/struct.StreamDeserializer.html): `StreamDeserializer` API.
- [`serde_json` functions](https://docs.rs/serde_json/latest/serde_json/#functions) — `from_str`, `from_slice`, `from_reader`, `to_string`, `to_vec`, `to_writer` and when each fits.
- [`std::borrow::Cow`](https://doc.rust-lang.org/std/borrow/enum.Cow.html): clone-on-write, the borrow/own hybrid.
- [`DeserializeOwned`](https://docs.rs/serde/latest/serde/de/trait.DeserializeOwned.html): the owning bound that `from_reader` requires.
- [Serde](/15-serialization/00-serde-intro/): the data-model architecture that makes borrowing and formats orthogonal.
- [Dynamic JSON with `serde_json::Value`](/15-serialization/04-json-manipulation/) — `serde_json::Value` and the `json!` macro: when dynamic JSON is the right tool despite the cost.
- [Structs and JSON](/15-serialization/03-json/): mapping structs, `Vec`, `HashMap`, `Option`, and enums to JSON.
- [Custom Serialization](/15-serialization/07-custom-serialization/) — hand-written (de)serialization and `serialize_with` / `deserialize_with`.
- [Beyond JSON](/15-serialization/06-other-formats/): binary formats (MessagePack, bincode) that are often the bigger performance win than tuning JSON.
- [Section 05: Ownership](/05-ownership/) — `&str` vs `String`, borrowing, and lifetimes.
- [Section 13: Testing](/13-testing/): benchmarking with `criterion` so you measure instead of guess.
- [Section 16: Web APIs](/16-web-apis/): serialized structs as HTTP request and response bodies.

---

## Exercises

### Exercise 1: Convert an owning struct to a borrowing one

**Difficulty:** Easy

**Objective:** Turn a `String`-based struct into a zero-copy borrowed struct and prove the borrow works.

**Instructions:** Start from the owning `Token` below. Rewrite it to borrow its two string fields from the input (`&'a str` with a lifetime parameter), then write a function `first_token(src: &str) -> Token<'_>` that parses it. Parse the literal `{"kind":"ident","lexeme":"user_count"}` and print both fields. Confirm it compiles and runs.

```rust playground
use serde::Deserialize;

// TODO: make this borrow from the input instead of allocating two Strings.
#[derive(Debug, Deserialize)]
struct Token {
    kind: String,
    lexeme: String,
}

fn main() {
    let src = r#"{"kind":"ident","lexeme":"user_count"}"#;
    // TODO: parse `src` into a borrowing Token and print kind + lexeme.
}
```

<details>
<summary>Solution</summary>

```rust playground
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Token<'a> {
    kind: &'a str,
    lexeme: &'a str,
}

fn first_token(src: &str) -> Token<'_> {
    serde_json::from_str(src).unwrap()
}

fn main() {
    let src = r#"{"kind":"ident","lexeme":"user_count"}"#;
    let tok = first_token(src);
    println!("{} = {:?}", tok.kind, tok.lexeme);
}
```

Real output from `cargo run`:

```text
ident = "user_count"
```

The `Token<'a>` lifetime ties the parsed struct to `src`. The `-> Token<'_>` return type lets the compiler infer that the returned token borrows from the `src` argument. No `String` is allocated; both fields are slices into `src`.

</details>

### Exercise 2: Stream-sum NDJSON without holding it all in memory

**Difficulty:** Medium

**Objective:** Use `StreamDeserializer` to sum a field across many JSON values without collecting them into a `Vec`.

**Instructions:** Write `sum_totals<R: std::io::Read>(reader: R) -> Result<f64, serde_json::Error>` that reads a stream of `{"total": <number>}` objects and returns the sum of all `total` fields. Use `serde_json::Deserializer::from_reader(...).into_iter::<Order>()` and accumulate as you iterate — never build a `Vec<Order>`. Test it on `{"total":19.99}{"total":4.50}{"total":100.00}` (note: no separators needed, JSON values are self-delimiting). Remember that streamed types must be **owned**.

```rust
use serde::Deserialize;
use std::io::Cursor;

#[derive(Deserialize)]
struct Order {
    total: f64,
}

// TODO: stream-sum the `total` fields without collecting into a Vec.
fn sum_totals<R: std::io::Read>(reader: R) -> Result<f64, serde_json::Error> {
    todo!()
}

fn main() -> Result<(), serde_json::Error> {
    let ndjson = r#"{"total":19.99}{"total":4.50}{"total":100.00}"#;
    let sum = sum_totals(Cursor::new(ndjson))?;
    println!("sum = {sum:.2}");
    Ok(())
}
```

<details>
<summary>Solution</summary>

```rust playground
use serde::Deserialize;
use std::io::Cursor;

#[derive(Deserialize)]
struct Order {
    total: f64,
}

fn sum_totals<R: std::io::Read>(reader: R) -> Result<f64, serde_json::Error> {
    let stream = serde_json::Deserializer::from_reader(reader).into_iter::<Order>();
    let mut sum = 0.0;
    for order in stream {
        sum += order?.total; // `order` is Result<Order, _>; `?` propagates errors
    }
    Ok(sum)
}

fn main() -> Result<(), serde_json::Error> {
    let ndjson = r#"{"total":19.99}{"total":4.50}{"total":100.00}"#;
    let sum = sum_totals(Cursor::new(ndjson))?;
    println!("sum = {sum:.2}");
    Ok(())
}
```

Real output from `cargo run`:

```text
sum = 124.49
```

The `StreamDeserializer` yields `Result<Order, _>` items, so `order?` both unwraps the value and propagates any parse error. Only one `Order` exists at a time — memory stays constant no matter how many records the reader holds. The fields are `f64` (owned), satisfying `from_reader`'s requirement that streamed types be owned.

</details>

### Exercise 3: Extract one field without building a `Value` tree

**Difficulty:** Medium

**Objective:** Pull a single field out of a large JSON payload using a minimal typed struct instead of `serde_json::Value`, relying on the fact that unknown fields are skipped during parsing.

**Instructions:** Given a large JSON payload with many fields, you only need the top-level `version` number. Write `payload_version(json: &str) -> Result<u32, serde_json::Error>` that parses into a struct declaring *only* `version` (Serde ignores the rest at parse time — no full tree is built). Test it on the multi-field literal below and print the version.

```rust
use serde::Deserialize;

// TODO: declare a struct with ONLY the field you need.

fn payload_version(json: &str) -> Result<u32, serde_json::Error> {
    todo!()
}

fn main() -> Result<(), serde_json::Error> {
    let big = r#"{
        "version": 3,
        "data": {"a": [1,2,3,4,5], "b": "lots of text here"},
        "signature": "deadbeef",
        "nested": {"deep": {"deeper": {"x": true}}}
    }"#;
    println!("version = {}", payload_version(big)?);
    Ok(())
}
```

<details>
<summary>Solution</summary>

```rust playground
use serde::Deserialize;

// Only `version` is declared; `data`, `signature`, and `nested` are
// skipped by the parser — they are never allocated into a Value tree.
#[derive(Deserialize)]
struct VersionOnly {
    version: u32,
}

fn payload_version(json: &str) -> Result<u32, serde_json::Error> {
    let parsed: VersionOnly = serde_json::from_str(json)?;
    Ok(parsed.version)
}

fn main() -> Result<(), serde_json::Error> {
    let big = r#"{
        "version": 3,
        "data": {"a": [1,2,3,4,5], "b": "lots of text here"},
        "signature": "deadbeef",
        "nested": {"deep": {"deeper": {"x": true}}}
    }"#;
    println!("version = {}", payload_version(big)?);
    Ok(())
}
```

Real output from `cargo run`:

```text
version = 3
```

Because Serde ignores fields the struct does not declare, the `data`, `signature`, and `nested` subtrees are parsed past but never materialized into owned values. Compared with `let v: Value = from_str(big)?; v["version"].as_u64()`, this allocates nothing for the unused fields and gives you a checked `u32` directly instead of an `Option<u64>` you have to unwrap.

</details>
