---
title: "HashMaps: `HashMap<K, V>` vs JavaScript Objects and `Map`"
description: "Rust's HashMap<K, V> is the typed counterpart to JavaScript objects and Map: get returns Option not undefined, keys and values are owned, and the entry API"
---

In JavaScript and TypeScript you reach for a plain **object** (`{}`) or a **`Map`** whenever you need to look something up by key. Rust's equivalent is `std::collections::HashMap<K, V>`, a hash table that maps keys of type `K` to values of type `V`. The data structure is familiar; what's new is that keys and values are *typed*, *owned*, and the lookup API hands you an `Option` instead of `undefined`.

---

## Quick Overview

A **`HashMap<K, V>`** stores key→value pairs with average O(1) insert and lookup, just like a JavaScript `Map`. The two ideas a TypeScript/JavaScript developer must internalize: (1) a `HashMap` is **homogeneous**: every key is the same type `K` and every value the same type `V`, unlike a JS object whose values can be anything; and (2) lookups return `Option<&V>` (`Some(&v)` or `None`) rather than silently producing `undefined`, so a missing key is something the compiler makes you handle.

In short, a JS object is really two things at once: a record with known fields *and* an ad-hoc dictionary. Rust splits those roles: use a [`struct`](/06-data-structures/00-structs/) when the keys are known at compile time, and a `HashMap` when keys are dynamic data.

---

## TypeScript/JavaScript Example

```typescript
// Counting how many times each word appears — a classic dictionary task.
function wordFrequency(text: string): Map<string, number> {
  const freq = new Map<string, number>();
  for (const word of text.split(/\s+/)) {
    // The "insert or increment" dance: read, default, write back.
    freq.set(word, (freq.get(word) ?? 0) + 1);
  }
  return freq;
}

const counts = wordFrequency("the quick brown fox the lazy dog the");
console.log(counts.get("the")); // 3
console.log(counts.get("cat")); // undefined  ← missing key, no error

// A plain object works too, but values are typed loosely and keys
// are coerced to strings:
const config: Record<string, string> = { host: "localhost", port: "8080" };
console.log(config.host); // "localhost"
console.log(config.scheme); // undefined ← typo? you find out at runtime

// Iteration order: a Map preserves insertion order; a plain object
// mostly does too (with integer-key caveats).
for (const [word, n] of counts) {
  // ...
}
```

**Key points:**

- `Map.get` returns the value or `undefined`; nothing forces you to check.
- `??` supplies a default during the read-modify-write update.
- A `Map` preserves **insertion order**; objects approximately do (with quirks for integer-like keys).
- Object values are whatever you put in; TypeScript's `Record<string, string>` is a *compile-time* promise that the runtime does not enforce.

---

## Rust Equivalent

```rust
use std::collections::HashMap;

/// Counting how many times each word appears.
fn word_frequency(text: &str) -> HashMap<&str, u32> {
    let mut freq: HashMap<&str, u32> = HashMap::new();
    for word in text.split_whitespace() {
        // The entry API does the "insert or increment" in one step.
        *freq.entry(word).or_insert(0) += 1;
    }
    freq
}

fn main() {
    let counts = word_frequency("the quick brown fox the lazy dog the");

    // get returns Option<&V>: Some(&3) or None — never a silent undefined.
    println!("the: {:?}", counts.get("the")); // Some(3)
    println!("cat: {:?}", counts.get("cat")); // None
}
```

Running it prints:

```text
the: Some(3)
cat: None
```

**Key points:**

- `HashMap::new()` needs a type, here inferred from the annotation `HashMap<&str, u32>`.
- The values are `u32` — *every* value is a `u32`, enforced at compile time. There is no "values can be anything" mode.
- `freq.entry(word).or_insert(0)` returns a `&mut u32` you can increment in place; no separate read-then-write.
- `get` returns `Option<&V>`. A missing key is `None`, a real value you must deal with, not `undefined`.

> **Note:** A standard-library `HashMap` does **not** preserve insertion order, and the order is intentionally randomized per program run (a defense against hash-collision DoS attacks). If you need ordering, see [Key Differences](#key-differences) and the sibling [Sorted Collections](/07-collections/05-btreemap-btreeset/).

---

## Detailed Explanation

### Creating and typing a map

```rust
use std::collections::HashMap;

let mut scores: HashMap<String, u32> = HashMap::new();
```

Unlike `Vec` and `String`, `HashMap` is **not** in the [prelude](/01-getting-started/02-hello-world/), so you must `use std::collections::HashMap;` first. The type parameters `<String, u32>` fix the key and value types for the entire map's life. Compare this to TypeScript's `Map<string, number>`: same shape of annotation, but in Rust it is a hard guarantee the compiler enforces, not an erasable hint.

If you immediately insert values, inference can often figure the types out, so the annotation is optional:

```rust
let mut scores = HashMap::new();
scores.insert(String::from("Blue"), 10u32); // now K = String, V = u32
```

### `insert`: returns the *previous* value

```rust
let mut scores: HashMap<String, u32> = HashMap::new();
scores.insert(String::from("Blue"), 10);
let old = scores.insert(String::from("Blue"), 25); // overwrites
println!("old Blue value: {old:?}"); // old Blue value: Some(10)
```

`insert` overwrites an existing key and **returns the old value as `Option<V>`** (`None` if the key was new). JavaScript's `map.set(k, v)` returns the map itself for chaining; Rust returns the displaced value, which is occasionally handy and never a `Map` for chaining.

### `get`, `contains_key`, `remove`

```rust
println!("{:?}", scores.get("Blue"));      // Some(25)  — note: &str key works
println!("{}", scores.contains_key("Red")); // false
let removed = scores.remove("Blue");        // Some(25), and the key is gone
```

A subtle convenience: even though the key type is `String`, you can look up with a `&str` (`"Blue"`) because `String` *borrows as* `str`. This is the `Borrow` trait at work — you don't have to allocate a `String` just to do a lookup. The closest JS analogy is that you never need to "rebuild" a key to read; here the type system makes the cheap path the default.

`get` returns `Option<&V>`, a **reference** into the map, not a copy. To get an owned value out, combine with `Option` methods (covered in [The Option Type](/06-data-structures/03-option-enum/)):

```rust
let config: HashMap<&str, &str> =
    HashMap::from([("host", "localhost"), ("port", "8080")]);

// .copied() turns Option<&&str> into Option<&str>; unwrap_or supplies a default.
let host = config.get("host").copied().unwrap_or("0.0.0.0");
let scheme = config.get("scheme").copied().unwrap_or("http");
println!("host={host} scheme={scheme}"); // host=localhost scheme=http
```

This is the typed, explicit version of JavaScript's `config.host ?? "0.0.0.0"`.

### The `entry` API: Rust's killer feature for maps

The "get the value, or insert a default, then mutate it" pattern is so common that Rust gives it a dedicated, allocation-aware API. `entry(key)` returns an `Entry`, a handle to a slot that may or may not be occupied:

```rust
let mut counts: HashMap<&str, i32> = HashMap::new();

// or_insert: if absent, insert this default; either way, return &mut V.
counts.entry("apple").or_insert(0);
counts.entry("apple").or_insert(99); // already present → 99 is ignored
println!("{}", counts["apple"]); // 0

// The increment idiom: deref the returned &mut and add.
*counts.entry("apple").or_insert(0) += 1;
println!("{}", counts["apple"]); // 1
```

`or_insert` returns a `&mut V` pointing at the slot, so `*entry += 1` mutates in place. In JavaScript you'd write `m.set(k, (m.get(k) ?? 0) + 1)` — two hash lookups (a `get` and a `set`) and a temporary. The Rust entry API does a **single** lookup.

For richer logic, `and_modify` updates an existing value and `or_insert` provides the first one:

```rust
let mut hits: HashMap<&str, u32> = HashMap::new();
hits.entry("/").and_modify(|c| *c += 1).or_insert(1); // first hit → 1
hits.entry("/").and_modify(|c| *c += 1).or_insert(1); // second hit → 2
println!("{}", hits["/"]); // 2
```

When the default itself is expensive to build, use `or_insert_with(|| ...)` so the closure only runs on a miss, or `or_default()` when `V: Default`:

```rust
// Grouping: each new department gets a fresh Vec, then we push into it.
let people = [("eng", "Alice"), ("sales", "Bob"), ("eng", "Carol")];
let mut by_dept: HashMap<&str, Vec<&str>> = HashMap::new();
for (dept, name) in people {
    by_dept.entry(dept).or_default().push(name);
}
// by_dept now: {"eng": ["Alice", "Carol"], "sales": ["Bob"]}
```

### Iteration

```rust
let mut roster: HashMap<String, u32> = HashMap::new();
roster.insert("Alice".into(), 30);
roster.insert("Bob".into(), 25);

for (name, age) in &roster {        // borrow: (&String, &u32)
    println!("{name} is {age}");
}

for age in roster.values_mut() {    // mutable borrow of each value
    *age += 1;                      // everyone has a birthday
}

let names: Vec<&String> = roster.keys().collect();   // just the keys
let ages: Vec<u32> = roster.values().copied().collect(); // just the values
```

`iter()` / `&map` yields `(&K, &V)` tuples; `keys()` and `values()` yield only one side; `values_mut()` yields `&mut V` for in-place updates; and `into_iter()` (i.e. `for (k, v) in map`) **consumes** the map and yields owned `(K, V)` pairs. The big difference from JavaScript: **iteration order is unspecified and randomized per run**, so never rely on it. To produce stable output, collect into a `Vec` and sort:

```rust
let mut pairs: Vec<(&String, &u32)> = roster.iter().collect();
pairs.sort_by_key(|(name, _)| *name); // deterministic, alphabetical
```

### Ownership of keys and values

This is where Rust departs most sharply from JavaScript. `insert(k, v)` **moves** `k` and `v` into the map: the map *owns* them:

```rust
let key = String::from("name");
let value = String::from("Ada");
let mut map = HashMap::new();
map.insert(key, value);
// `key` and `value` are moved; using them now is a compile error (see Pitfalls).
```

For `Copy` types (`i32`, `bool`, `char`, `&str`, …), "move" is a bitwise copy and the original stays usable. For owned types like `String` or `Vec`, the map takes ownership; see [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/). This is exactly the discipline that lets a `HashMap` free all its keys and values automatically when it goes out of scope, with no garbage collector.

> **Tip:** A key type must implement `Eq` and `Hash`. All the obvious types do (`String`, `&str`, integers, `char`, tuples of hashable things, and any `struct`/`enum` you annotate with `#[derive(PartialEq, Eq, Hash)]`). Notably `f64` does **not** implement `Eq` (because `NaN != NaN`), so you can't use a float as a key without extra work.

---

## Key Differences

| Concept | JS object / `Map` | Rust `HashMap<K, V>` |
| --- | --- | --- |
| Key types | Object: string/symbol only. `Map`: any value | Any type that is `Eq + Hash` |
| Value types | Anything (heterogeneous) | Exactly one type `V` (homogeneous) |
| Missing key | `undefined` (silent) | `Option<&V>` → `None` (must handle) |
| Iteration order | `Map` preserves insertion order | **Unspecified & randomized** |
| Read-modify-write | Two lookups (`get` + `set`) | One lookup via the `entry` API |
| Ownership | GC-managed references | Map **owns** its keys and values |
| Lookup by alt type | n/a | `&str` looks up a `String` key (via `Borrow`) |
| Indexing missing key | `obj[k]` → `undefined` | `map[k]` → **panics** |
| Default hasher | engine-defined | SipHash 1-3 (DoS-resistant, not the fastest) |

### Missing keys: `None` vs panic

There are two ways to read a key, and they differ on what "missing" means:

- `map.get(k)` → `Option<&V>`. The safe, idiomatic choice. Missing key is `None`.
- `map[k]` (the `Index` operator) → `&V`, but **panics** if the key is absent.

Use indexing only when you are certain the key exists; otherwise use `get`. This mirrors `Vec` indexing in [Vectors](/07-collections/00-vectors/): the convenient `[]` syntax trades safety for brevity.

### Homogeneous values

A JS object happily holds `{ id: 1, name: "Ada", active: true }`. A Rust `HashMap` cannot — every value is one type. That's a feature: if your keys are *known field names*, you want a `struct`, which is checked at compile time and has zero hashing overhead. Reach for `HashMap` when the **keys are data** (user IDs, words, SKUs) rather than a fixed schema. To store genuinely mixed value types under string keys, you'd use an `enum` as the value (see [Enums and Data-Carrying Variants](/06-data-structures/02-enums/)) — making the "anything" explicit and type-checked.

### The hasher

Rust's default hasher is **SipHash 1-3**, chosen for resistance to hash-flooding denial-of-service attacks, not raw speed. For internal, untrusted-input-free maps where speed matters, you can swap in a faster hasher (e.g. the `ahash` or `rustc-hash` crates) via `HashMap::with_hasher` / a type alias. Most code never needs to; mentioned here so the analogy to JS's opaque hashing is honest.

---

## Common Pitfalls

### Pitfall 1: Using a key or value after inserting it

In JavaScript, `map.set(key, value)` leaves your `key`/`value` variables fully usable afterward. In Rust, inserting an owned value **moves** it into the map:

```rust
use std::collections::HashMap;

fn main() {
    let key = String::from("name");
    let value = String::from("Ada");
    let mut map = HashMap::new();
    map.insert(key, value);
    println!("{key}"); // does not compile (error[E0382]: borrow of moved value: `key`)
}
```

The real compiler error:

```text
error[E0382]: borrow of moved value: `key`
 --> src/main.rs:8:16
  |
4 |     let key = String::from("name");
  |         --- move occurs because `key` has type `String`, which does not implement the `Copy` trait
...
7 |     map.insert(key, value);
  |                --- value moved here
8 |     println!("{key}"); // use after move
  |                ^^^ value borrowed here after move
  |
help: consider cloning the value if the performance cost is acceptable
  |
7 |     map.insert(key.clone(), value);
  |                   ++++++++
```

**Fix:** if you still need the original, `insert(key.clone(), value)`; or store `&str` keys instead of `String` when the strings outlive the map; or simply read the value back out of the map. The compiler's suggestion to `.clone()` is the easy escape hatch.

### Pitfall 2: Indexing a key that might not exist

`obj["missing"]` in JavaScript is just `undefined`. The Rust `Index` operator panics:

```rust
use std::collections::HashMap;

fn main() {
    let scores: HashMap<&str, i32> = HashMap::from([("Blue", 10)]);
    let v = scores["Red"]; // missing key
    println!("{v}");
}
```

This compiles but panics at runtime:

```text
thread 'main' panicked at src/main.rs:5:19:
no entry found for key
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

**Fix:** use `get`, which returns `Option`, and decide what a miss means:

```rust
let v = scores.get("Red").copied().unwrap_or(0);
```

### Pitfall 3: Trying to mutate through `get`

`get` returns an **immutable** reference `&V`, so you cannot write through it. This trips up developers expecting `map.get(k)` to behave like a mutable JS slot:

```rust
use std::collections::HashMap;

fn main() {
    let mut scores: HashMap<&str, i32> = HashMap::from([("Blue", 10)]);
    if let Some(s) = scores.get("Blue") {
        *s += 1; // does not compile (error[E0594]: cannot assign to `*s`, behind a `&` reference)
    }
    println!("{scores:?}");
}
```

The core of the real error:

```text
error[E0594]: cannot assign to `*s`, which is behind a `&` reference
 --> src/main.rs:6:9
  |
5 |     if let Some(s) = scores.get("Blue") {
  |                 - consider changing this binding's type to be: `&mut i32`
6 |         *s += 1; // s is &i32, not &mut i32
  |         ^^^^^^^ `s` is a `&` reference, so the data it refers to cannot be written
```

**Fix:** use `get_mut` (returns `Option<&mut V>`) or, better for "update or insert", the `entry` API:

```rust
if let Some(s) = scores.get_mut("Blue") {
    *s += 1;
}
// or:
*scores.entry("Blue").or_insert(0) += 1;
```

### Pitfall 4: Holding a borrow from the map while modifying it

Rust forbids reading a reference *into* the map while you also mutate the map (which might reallocate and invalidate that reference). This is the borrow checker, the same rule you meet with `Vec`:

```rust
use std::collections::HashMap;

fn main() {
    let mut map: HashMap<&str, i32> = HashMap::from([("a", 1)]);
    let first = map.get("a").unwrap(); // immutable borrow starts
    map.insert("b", 2);                // mutable borrow while `first` is live
    println!("{first}");
}
```

The real error:

```text
error[E0502]: cannot borrow `map` as mutable because it is also borrowed as immutable
 --> src/main.rs:6:5
  |
5 |     let first = map.get("a").unwrap(); // immutable borrow starts
  |                 --- immutable borrow occurs here
6 |     map.insert("b", 2);                // mutable borrow while `first` is live
  |     ^^^^^^^^^^^^^^^^^^ mutable borrow occurs here
7 |     println!("{first}");               // immutable borrow used here
  |                ----- immutable borrow later used here
```

**Fix:** finish using the borrowed value first, or copy it out (`let first = *map.get("a").unwrap();`) before mutating. See [Borrowing and References](/05-ownership/02-borrowing/).

### Pitfall 5: Expecting deterministic iteration order

```rust
// The order of these is NOT stable across runs — never assert on it directly.
for (k, v) in &map { /* ... */ }
```

If a test or output depends on order, collect into a `Vec` and `sort` (as shown earlier), or use a `BTreeMap` ([Sorted Collections](/07-collections/05-btreemap-btreeset/)) which keeps keys sorted.

---

## Best Practices

- **Prefer `get` over indexing.** `map.get(k)` makes "absent" a value you handle; `map[k]` panics. Reserve `[]` for keys you have proven exist.
- **Use the `entry` API for read-modify-write.** `*map.entry(k).or_insert(0) += 1` is one lookup, no temporaries, and reads cleanly. Avoid the `if contains_key { get } else { insert }` dance.
- **Pick `or_default` / `or_insert_with` to avoid building unused defaults.** `or_insert(expensive())` always evaluates `expensive()`; `or_insert_with(|| expensive())` only does so on a miss.
- **Choose key types deliberately.** `&str` keys avoid allocation when the strings live long enough; `String` keys when the map must own them. Look up with `&str` even when the key is `String`.
- **Reach for a `struct` when keys are a fixed schema,** and a `HashMap` when keys are runtime data. Don't model a known record as `HashMap<String, _>`.
- **Pre-size with `HashMap::with_capacity(n)`** when you know roughly how many entries you'll insert, to cut down on rehashing: the same idea as `Vec::with_capacity`, covered in [Collection Performance](/07-collections/09-collection-performance/).
- **Derive `#[derive(PartialEq, Eq, Hash)]`** on any custom type you want to use as a key, and remember floats can't be keys without a wrapper.
- **Sort before printing** if output must be deterministic; never rely on iteration order.

---

## Real-World Example

Aggregating per-key totals in a single pass is a daily task: order line items by SKU, request counts by route, error counts by type. Here we summarize an order into per-SKU units and revenue using the `entry` API, then print a sorted report.

```rust
use std::collections::HashMap;

/// One line of an order: which SKU and how many units at what price.
#[derive(Debug)]
struct LineItem {
    sku: String,
    quantity: u32,
    unit_price_cents: u64,
}

/// Aggregate per-SKU totals across many line items in a single pass.
/// Value is a (units, revenue_cents) tuple.
fn summarize(items: &[LineItem]) -> HashMap<String, (u32, u64)> {
    let mut totals: HashMap<String, (u32, u64)> = HashMap::new();
    for item in items {
        // One lookup per item: get-or-create the slot, then update it.
        let entry = totals.entry(item.sku.clone()).or_insert((0, 0));
        entry.0 += item.quantity;
        entry.1 += item.quantity as u64 * item.unit_price_cents;
    }
    totals
}

fn main() {
    let orders = vec![
        LineItem { sku: "WIDGET".into(), quantity: 3, unit_price_cents: 250 },
        LineItem { sku: "GADGET".into(), quantity: 1, unit_price_cents: 999 },
        LineItem { sku: "WIDGET".into(), quantity: 2, unit_price_cents: 250 },
        LineItem { sku: "GIZMO".into(),  quantity: 5, unit_price_cents: 120 },
    ];

    let totals = summarize(&orders);

    // HashMap order is unspecified, so collect + sort for a stable report.
    let mut rows: Vec<(&String, &(u32, u64))> = totals.iter().collect();
    rows.sort_by(|a, b| b.1.1.cmp(&a.1.1)); // by revenue, descending

    println!("{:<8} {:>5} {:>10}", "SKU", "UNITS", "REVENUE");
    for (sku, (units, revenue_cents)) in rows {
        println!("{sku:<8} {units:>5} {:>9.2}", *revenue_cents as f64 / 100.0);
    }

    let grand: u64 = totals.values().map(|(_, rev)| rev).sum();
    println!("grand total: ${:.2}", grand as f64 / 100.0);
}
```

Output:

```text
SKU      UNITS    REVENUE
WIDGET       5     12.50
GADGET       1      9.99
GIZMO        5      6.00
grand total: $28.49
```

The interesting line is `totals.entry(item.sku.clone()).or_insert((0, 0))`: it does a single hash lookup that either finds the existing `(units, revenue)` tuple or inserts a fresh `(0, 0)`, returning a `&mut (u32, u64)` either way. The JavaScript version of this aggregation typically does two `Map` operations per item (`get` then `set`) plus an object spread; the Rust version does one, with the types guaranteeing every value is a `(u32, u64)`.

---

## Further Reading

- [`std::collections::HashMap`](https://doc.rust-lang.org/std/collections/struct.HashMap.html) — the full API reference
- [`std::collections::hash_map::Entry`](https://doc.rust-lang.org/std/collections/hash_map/enum.Entry.html) — the entry API in detail
- [The Rust Programming Language — Storing Keys with Associated Values in Hash Maps](https://doc.rust-lang.org/book/ch08-03-hash-maps.html)
- [Rust by Example — HashMap](https://doc.rust-lang.org/rust-by-example/std/hash.html)
- Sibling topics: [Vectors](/07-collections/00-vectors/) · [Strings](/07-collections/01-strings/) · [Sets and HashSet](/07-collections/04-hashsets/) · [Sorted Collections](/07-collections/05-btreemap-btreeset/) · [Iterators](/07-collections/06-iterators/) · [Iterator Consumers](/07-collections/07-iterator-consumers/) · [Collection Performance](/07-collections/09-collection-performance/)
- Background: [Section 05 — Ownership & Move/Copy/Clone](/05-ownership/06-move-copy-clone/) · [Section 06 — Structs](/06-data-structures/00-structs/) · [Option](/06-data-structures/03-option-enum/)
- What's next: a missing key as a recoverable error in [Section 08 — Error Handling](/08-error-handling/)

---

## Exercises

### Exercise 1: Tally votes

**Difficulty:** Beginner

**Objective:** Practice the `entry` API for counting.

**Instructions:** Write `fn count_votes(votes: &[&str]) -> HashMap<String, u32>` that counts how many times each candidate name appears in `votes`. Use the entry API so each name is looked up once. In `main`, count `["yes", "no", "yes", "yes", "no"]` and print the totals for `"yes"` and `"no"`.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

fn count_votes(votes: &[&str]) -> HashMap<String, u32> {
    let mut tally: HashMap<String, u32> = HashMap::new();
    for &v in votes {
        *tally.entry(v.to_string()).or_insert(0) += 1;
    }
    tally
}

fn main() {
    let votes = ["yes", "no", "yes", "yes", "no"];
    let tally = count_votes(&votes);
    println!("yes={} no={}", tally["yes"], tally["no"]);
}
```

Output:

```text
yes=3 no=2
```

</details>

### Exercise 2: Invert a map

**Difficulty:** Intermediate

**Objective:** Iterate a map and build a new one, practicing ownership of keys/values via `clone` and `collect`.

**Instructions:** Write `fn invert(map: &HashMap<String, String>) -> HashMap<String, String>` that returns a new map with keys and values swapped (assume values are unique). Build it from an iterator with `.map(...).collect()`. In `main`, invert a small phone book (`name → number`) and look up a name by number.

> **Tip:** `map.iter()` yields `(&String, &String)`. Since the new map must *own* its strings, `.clone()` each side inside the closure.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

fn invert(map: &HashMap<String, String>) -> HashMap<String, String> {
    map.iter().map(|(k, v)| (v.clone(), k.clone())).collect()
}

fn main() {
    let mut phone = HashMap::new();
    phone.insert("Alice".to_string(), "555-1234".to_string());
    phone.insert("Bob".to_string(), "555-9999".to_string());

    let by_number = invert(&phone);
    println!("{:?}", by_number.get("555-1234")); // Some("Alice")
}
```

Output:

```text
Some("Alice")
```

</details>

### Exercise 3: Merge two maps, summing collisions

**Difficulty:** Advanced

**Objective:** Combine cloning, the entry API, and deterministic sorted output.

**Instructions:** Write `fn merge_sum(a: &HashMap<String, i64>, b: &HashMap<String, i64>) -> HashMap<String, i64>` that returns a new map containing every key from both inputs; when a key appears in both, its value is the **sum**. Start from a clone of `a` and fold `b` in with the entry API. In `main`, merge `{a:1, b:2}` with `{b:40, c:100}` and print the result with keys in sorted order.

> **Tip:** Iteration order is unspecified, so collect the keys into a `Vec`, `sort` them, and index the map to print in a stable order.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

fn merge_sum(
    a: &HashMap<String, i64>,
    b: &HashMap<String, i64>,
) -> HashMap<String, i64> {
    let mut out = a.clone();
    for (k, v) in b {
        *out.entry(k.clone()).or_insert(0) += v;
    }
    out
}

fn main() {
    let mut a = HashMap::new();
    a.insert("a".to_string(), 1);
    a.insert("b".to_string(), 2);

    let mut b = HashMap::new();
    b.insert("b".to_string(), 40);
    b.insert("c".to_string(), 100);

    let merged = merge_sum(&a, &b);

    let mut keys: Vec<&String> = merged.keys().collect();
    keys.sort();
    for k in keys {
        println!("{k} => {}", merged[k]);
    }
}
```

Output:

```text
a => 1
b => 42
c => 100
```

</details>
