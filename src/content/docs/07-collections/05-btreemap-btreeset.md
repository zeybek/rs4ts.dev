---
title: "Sorted Collections: BTreeMap and BTreeSet"
description: "Rust's BTreeMap and BTreeSet keep keys permanently sorted, giving free in-order iteration and O(log n) range queries a JavaScript Map can't."
---

Rust's standard library ships two collections whose superpower is **order**: `BTreeMap<K, V>` and `BTreeSet<T>`. Where a `HashMap` gives you the fastest possible lookup but throws away ordering, a `BTreeMap` keeps its keys permanently sorted, which makes "give me everything between X and Y" and "iterate in order" cheap, built-in operations rather than something you reconstruct by sorting on every read.

---

## Quick Overview

In JavaScript, both plain objects and `Map` iterate in **insertion order**, and there is no built-in sorted collection at all: if you want sorted keys or a range query, you sort an array yourself, every single time. Rust's **`BTreeMap`** stores its entries sorted by key, so iteration is always in ascending key order and you get `O(log n)` range queries for free. **`BTreeSet`** is the same idea without values: a sorted set of unique elements. The trade-off is that `BTreeMap`/`BTreeSet` are slightly slower per lookup than their hashing cousins, and their keys must be **orderable** (`Ord`) rather than merely hashable.

---

## TypeScript/JavaScript Example

```typescript
// TypeScript - a Map preserves INSERTION order, never sorted order.
const scores = new Map<string, number>();
scores.set("charlie", 90);
scores.set("alice", 85);
scores.set("bob", 95);

console.log([...scores.keys()]); // [ 'charlie', 'alice', 'bob' ]  <- insertion order!

// To display in sorted order, you must sort explicitly EVERY time you read.
const sortedByName = [...scores.entries()].sort(([a], [b]) => a.localeCompare(b));
console.log(sortedByName); // [ [ 'alice', 85 ], [ 'bob', 95 ], [ 'charlie', 90 ] ]

// A "range query" — events between 10:00 and 15:00 — is a manual filter + sort.
const events = new Map<number, string>([
  [900, "standup"],
  [1030, "design review"],
  [1200, "lunch"],
  [1400, "1:1"],
  [1630, "retro"],
]);

const between = [...events.entries()]
  .filter(([time]) => time >= 1000 && time <= 1500)
  .sort(([a], [b]) => a - b);
console.log(between);
// [ [ 1030, 'design review' ], [ 1200, 'lunch' ], [ 1400, '1:1' ] ]
```

**Key points:**

- A JavaScript `Map` (and a plain object) iterates in **insertion** order, not key order.
- There is no native sorted map or sorted set; you rebuild a sorted view with `[...map].sort(...)` on every read, which is `O(n log n)` each time.
- A range query is "spread to an array, `filter`, then `sort`": correct, but linear in the whole collection regardless of how small the window is.

---

## Rust Equivalent

```rust
use std::collections::BTreeMap;

fn main() {
    // Insert in scrambled order; BTreeMap keeps the keys sorted automatically.
    let mut scores: BTreeMap<String, u32> = BTreeMap::new();
    scores.insert("charlie".to_string(), 90);
    scores.insert("alice".to_string(), 85);
    scores.insert("bob".to_string(), 95);

    // Iteration is ALWAYS in ascending key order — no sorting step needed.
    for (name, score) in &scores {
        println!("{name}: {score}");
    }

    // The smallest and largest keys are O(log n) lookups.
    println!("first: {:?}", scores.first_key_value());
    println!("last: {:?}", scores.last_key_value());
}
```

**Verified output:**

```text
alice: 85
bob: 95
charlie: 90
first: Some(("alice", 85))
last: Some(("charlie", 90))
```

The range query becomes a single, efficient `.range(..)` call that touches only the entries inside the window:

```rust
use std::collections::BTreeMap;

fn main() {
    let mut events: BTreeMap<u32, &str> = BTreeMap::new();
    events.insert(900, "standup");
    events.insert(1030, "design review");
    events.insert(1200, "lunch");
    events.insert(1400, "1:1");
    events.insert(1630, "retro");

    // Inclusive range 1000..=1500 — returns entries in key order, no filter/sort.
    println!("between 10:00 and 15:00:");
    for (time, name) in events.range(1000..=1500) {
        println!("  {time}: {name}");
    }
}
```

**Verified output:**

```text
between 10:00 and 15:00:
  1030: design review
  1200: lunch
  1400: 1:1
```

**Key points:**

- `BTreeMap` keeps entries sorted by key at all times; iteration order is guaranteed and free.
- `.range(start..=end)` walks only the matching slice of the tree, in order: no full scan, no per-read sort.
- The API mirrors `HashMap` (`insert`, `get`, `remove`, the `entry` API), so most code reads the same; you swap the type and gain ordering.

---

## Detailed Explanation

### What "B-Tree" means and why you don't care (much)

A `BTreeMap` is implemented as a **B-Tree** — a balanced search tree where each node holds many keys, chosen to be cache-friendly on modern CPUs. You never interact with the tree structure directly; what matters is the behavior it buys you:

- Keys are kept in **sorted order** as you insert.
- `get`, `insert`, and `remove` are `O(log n)` (versus `HashMap`'s amortized `O(1)`).
- Iteration visits keys smallest-to-largest.
- Finding a range of keys is `O(log n + k)`, where `k` is the number of items in the range.

> **Note:** The name is "B-Tree", not "Binary Tree". Each node stores a block of keys, which keeps the tree shallow and the memory accesses local. You will not implement or tune it; `std` does that for you.

### Keys must be `Ord`, not `Hash`

The defining requirement is right there in the method signatures: a `BTreeMap` key must implement the **`Ord`** trait (total ordering), because the collection sorts by comparing keys. A `HashMap` instead needs `Hash` + `Eq`. All the obvious primitives — integers, `char`, `bool`, `&str`/`String`, and tuples/`Vec`s of orderable things — implement `Ord` already. (`Ord` and its companions `PartialOrd`/`Eq` are part of the trait system covered in [Section 09](/09-generics-traits/).)

```rust
use std::collections::BTreeSet;

// Deriving Ord (plus the traits it builds on) makes a struct usable as a key.
// Ordering is field-by-field, top to bottom — major, then minor, then patch.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

fn main() {
    let mut releases: BTreeSet<Version> = BTreeSet::new();
    releases.insert(Version { major: 1, minor: 2, patch: 0 });
    releases.insert(Version { major: 1, minor: 0, patch: 5 });
    releases.insert(Version { major: 2, minor: 0, patch: 0 });
    releases.insert(Version { major: 1, minor: 2, patch: 0 }); // duplicate ignored

    for v in &releases {
        println!("{}.{}.{}", v.major, v.minor, v.patch);
    }
    println!("latest: {:?}", releases.last());
}
```

**Verified output:**

```text
1.0.5
1.2.0
2.0.0
latest: Some(Version { major: 2, minor: 0, patch: 0 })
```

The four derives matter: `#[derive(PartialEq, Eq, PartialOrd, Ord)]` generates a lexicographic comparison in field declaration order. Reorder the fields and you reorder the sort. This is more honest than JavaScript's `Array.prototype.sort`, whose default coerces everything to strings (so `[2, 10].sort()` famously yields `[10, 2]`).

### `BTreeSet` is a `BTreeMap` with no values

`BTreeSet<T>` is the sorted analogue of `HashSet<T>` (see [Sets and HashSet](/07-collections/04-hashsets/)): a collection of unique, ordered elements. Duplicate inserts are silently ignored, membership tests are `O(log n)`, and iteration is in sorted order.

```rust
use std::collections::BTreeSet;

fn main() {
    let mut tags: BTreeSet<&str> = BTreeSet::new();
    tags.insert("rust");
    tags.insert("async");
    tags.insert("rust"); // duplicate ignored
    tags.insert("web");

    let listed: Vec<&&str> = tags.iter().collect();
    println!("{listed:?}");                       // sorted, deduped
    println!("contains async: {}", tags.contains("async"));
}
```

**Verified output:**

```text
["async", "rust", "web"]
contains async: true
```

### Range queries in depth

`.range(..)` accepts any of Rust's range syntaxes, and the bounds follow the same inclusive/exclusive rules as slicing:

| Range expression       | Meaning                                  |
| ---------------------- | ---------------------------------------- |
| `start..end`           | `start` inclusive, `end` **exclusive**   |
| `start..=end`          | both ends inclusive                      |
| `start..`              | from `start` (inclusive) to the largest key |
| `..end`                | from the smallest key up to (excluding) `end` |
| `..`                   | the whole map, in order (same as iterating) |

```rust
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

fn main() {
    let mut events: BTreeMap<u32, &str> = BTreeMap::new();
    events.insert(900, "standup");
    events.insert(1030, "design review");
    events.insert(1200, "lunch");
    events.insert(1400, "1:1");
    events.insert(1630, "retro");

    // Everything from 1200 onward.
    println!("from 12:00 onward:");
    for (time, name) in events.range(1200..) {
        println!("  {time}: {name}");
    }

    // For full control, pass an explicit (start, end) pair of Bound values.
    // Here: strictly after 1200, no upper limit.
    println!("strictly after 12:00:");
    for (time, name) in events.range((Excluded(1200), Unbounded)) {
        println!("  {time}: {name}");
    }
}
```

**Verified output:**

```text
from 12:00 onward:
  1200: lunch
  1400: 1:1
  1630: retro
strictly after 12:00:
  1400: 1:1
  1630: retro
```

The `Bound` enum (`Included`, `Excluded`, `Unbounded`) is the escape hatch when the `..` syntaxes are not expressive enough, most commonly when you need an **exclusive lower bound**, which the range operators cannot express on their own.

`BTreeSet` has the same `.range(..)`:

```rust
use std::collections::BTreeSet;

fn main() {
    let primes: BTreeSet<u32> = [2, 3, 5, 7, 11, 13, 17, 19].into_iter().collect();
    let in_range: Vec<&u32> = primes.range(5..15).collect();
    println!("{in_range:?}"); // 5 inclusive, 15 exclusive
}
```

**Verified output:**

```text
[5, 7, 11, 13]
```

### The familiar map operations still apply

Everything you know from [HashMaps](/07-collections/03-hashmaps/) carries over (the `entry` API, `get_mut`, `remove`, set operations on sets) because both maps implement the same conceptual interface. Here is the classic word-count, which now prints alphabetically with no extra sorting:

```rust
use std::collections::BTreeMap;

fn main() {
    let text = "the quick brown fox the lazy dog the end";
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for word in text.split_whitespace() {
        // entry().or_insert() works identically to HashMap.
        *counts.entry(word).or_insert(0) += 1;
    }
    for (word, count) in &counts {
        println!("{word}: {count}");
    }
}
```

**Verified output:**

```text
brown: 1
dog: 1
end: 1
fox: 1
lazy: 1
quick: 1
the: 3
```

`BTreeSet` also supports the same `union`, `intersection`, and `difference` as `HashSet`, but here the results come out **sorted**:

```rust
use std::collections::BTreeSet;

fn main() {
    let a: BTreeSet<i32> = [1, 2, 3, 4].into_iter().collect();
    let b: BTreeSet<i32> = [3, 4, 5, 6].into_iter().collect();

    let union: Vec<&i32> = a.union(&b).collect();
    let inter: Vec<&i32> = a.intersection(&b).collect();
    let diff: Vec<&i32> = a.difference(&b).collect();

    println!("union: {union:?}");
    println!("intersection: {inter:?}");
    println!("difference: {diff:?}");
}
```

**Verified output:**

```text
union: [1, 2, 3, 4, 5, 6]
intersection: [3, 4]
difference: [1, 2]
```

### Ordered-specific extras

Because the data is sorted, `BTreeMap`/`BTreeSet` offer methods that have no `HashMap` equivalent. `pop_first`/`pop_last` remove and return the smallest/largest entry, turning a `BTreeMap` into a serviceable ordered queue or simple priority queue:

```rust
use std::collections::BTreeMap;

fn main() {
    let mut tasks: BTreeMap<u8, &str> = BTreeMap::new();
    tasks.insert(3, "medium");
    tasks.insert(1, "urgent");
    tasks.insert(5, "low");

    // Always pull the smallest key first.
    while let Some((priority, name)) = tasks.pop_first() {
        println!("handling p{priority}: {name}");
    }
}
```

**Verified output:**

```text
handling p1: urgent
handling p3: medium
handling p5: low
```

> **Tip:** For a heavy-duty priority queue, reach for `std::collections::BinaryHeap` instead. It is purpose-built for "always give me the max" with `O(log n)` push/pop and `O(1)` peek. Use `BTreeMap::pop_first`/`pop_last` when you *also* need ordered iteration, range queries, or keyed lookup on the same data.

`split_off` cuts the map in two at a key, keeping the lower part and returning the upper part as a new map:

```rust
use std::collections::BTreeMap;

fn main() {
    let mut map: BTreeMap<i32, &str> = BTreeMap::new();
    for i in 1..=5 {
        map.insert(i, "x");
    }
    // Keys >= 3 move into `high`; `map` retains keys < 3.
    let high = map.split_off(&3);
    let low_keys: Vec<&i32> = map.keys().collect();
    let high_keys: Vec<&i32> = high.keys().collect();
    println!("low: {low_keys:?}");
    println!("high: {high_keys:?}");
}
```

**Verified output:**

```text
low: [1, 2]
high: [3, 4, 5]
```

---

## Key Differences

| Aspect                 | JS `Map` / object          | Rust `HashMap`                 | Rust `BTreeMap`                       |
| ---------------------- | -------------------------- | ------------------------------ | ------------------------------------- |
| Iteration order        | Insertion order            | **Unspecified** (randomized)   | **Sorted by key**                     |
| Lookup / insert / remove | `O(1)` average           | `O(1)` amortized               | `O(log n)`                            |
| Range query            | Manual `filter` + `sort`   | Not supported                  | `O(log n + k)` via `.range(..)`       |
| Min / max key          | Manual scan / sort         | Manual scan (`O(n)`)           | `O(log n)` (`first/last_key_value`)   |
| Key requirement        | Any value (SameValueZero)  | `Hash` + `Eq`                  | **`Ord`**                             |
| Floats as keys         | Allowed                    | Allowed (via `Eq` workarounds) | **Not allowed** (`f64` is not `Ord`)  |

### Why iteration order differs from HashMap

A `HashMap`'s order is not merely "different": it is **deliberately randomized** per execution (its default hasher is seeded from a random value at startup) to defend against hash-flooding denial-of-service attacks. So you must never rely on `HashMap` iteration order. A `BTreeMap` makes the opposite promise: order is part of the contract. If your output must be deterministic — config files, serialized snapshots, anything diffed in tests — `BTreeMap` removes a whole class of flaky behavior.

### When to choose which

- **Reach for `HashMap`/`HashSet`** when you only do point lookups and inserts and never need order. They are faster per operation.
- **Reach for `BTreeMap`/`BTreeSet`** when you need any of: sorted iteration, range queries, smallest/largest by key, or **deterministic output**.

A detailed Big-O comparison across all the collections lives in [Collection Performance](/07-collections/09-collection-performance/).

---

## Common Pitfalls

### Pitfall 1: Trying to use a type that isn't `Ord` as a key

If you forget to derive `Ord` on a struct (or try to derive it on a type containing a non-`Ord` field), the error appears at the `insert` call:

```rust
use std::collections::BTreeMap;

#[derive(Debug)] // only Debug — no Ord
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let mut m: BTreeMap<Point, &str> = BTreeMap::new();
    m.insert(Point { x: 1, y: 2 }, "origin-ish"); // does not compile (error[E0277])
    println!("{m:?}");
}
```

Real compiler output:

```text
error[E0277]: the trait bound `Point: Ord` is not satisfied
  --> src/main.rs:12:7
   |
12 |     m.insert(Point { x: 1, y: 2 }, "origin-ish");
   |       ^^^^^^ the trait `Ord` is not implemented for `Point`
   |
note: required by a bound in `BTreeMap::<K, V, A>::insert`
...
help: consider annotating `Point` with `#[derive(Ord)]`
   |
 5 + #[derive(Ord)]
 6 | struct Point {
   |
```

The fix is exactly what the compiler suggests, except you need the full set: `#[derive(PartialEq, Eq, PartialOrd, Ord)]` (deriving `Ord` requires `PartialOrd`, `Eq`, and `PartialEq` too).

### Pitfall 2: Floating-point keys

Coming from JavaScript, where `new Map().set(1.5, "x")` is fine, you may try a `BTreeMap<f64, _>`. It will not compile, because `f64` deliberately does **not** implement `Ord` (the `NaN != NaN` rule makes a total ordering impossible):

```rust
use std::collections::BTreeMap;

fn main() {
    let mut m: BTreeMap<f64, &str> = BTreeMap::new();
    m.insert(1.5, "one and a half"); // does not compile (error[E0277])
    println!("{m:?}");
}
```

Real compiler output (abridged):

```text
error[E0277]: the trait bound `f64: Ord` is not satisfied
  --> src/main.rs:5:7
   |
5  |     m.insert(1.5, "one and a half");
   |       ^^^^^^ the trait `Ord` is not implemented for `f64`
   |
   = help: the following other types implement trait `Ord`:
             i128
             i16
             i32
             ...
```

Workarounds: scale to an integer key (e.g. store millis or cents as `u64`/`i64`), or wrap floats in a crate like `ordered-float`'s `OrderedFloat`, which provides a total order by defining where `NaN` sorts.

### Pitfall 3: A backwards range panics at runtime

Unlike a JavaScript filter that silently returns an empty array, calling `.range(start..end)` with `start > end` is a programming error and **panics**:

```rust
use std::collections::BTreeMap;

fn main() {
    let mut m: BTreeMap<i32, &str> = BTreeMap::new();
    m.insert(1, "a");
    m.insert(2, "b");
    m.insert(3, "c");
    let r: Vec<_> = m.range(3..1).collect(); // start > end
    println!("{r:?}");
}
```

Real runtime output:

```text
thread 'main' panicked at .../alloc/src/collections/btree/search.rs:121:21:
range start is greater than range end in BTreeMap
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

Validate or `min`/`max`-clamp user-supplied bounds before passing them to `.range(..)`. (An empty *equal* range like `3..3` is fine; it just yields nothing.)

### Pitfall 4: Expecting `HashMap`-level speed

`BTreeMap` operations are `O(log n)`, not `O(1)`. For a million-entry hot path doing nothing but point lookups, a `HashMap` will measurably win. Choose `BTreeMap` for what it *gives* you (order, ranges), not as a drop-in faster map.

---

## Best Practices

- **Use `BTreeMap`/`BTreeSet` for deterministic output.** Anything serialized, logged, snapshot-tested, or diffed benefits from stable key order. Many teams collect into a `BTreeMap` purely so their test fixtures and config dumps stop reordering between runs.

- **Build with a `HashMap`, present with a `BTreeMap`** when ordering only matters at the end. Do the hot inserts in a `HashMap`, then `.collect()` into a `BTreeMap` once for display:

  ```rust
  use std::collections::{BTreeMap, HashMap};

  fn main() {
      let mut counts: HashMap<&str, u32> = HashMap::new();
      for w in "b a c a b a".split_whitespace() {
          *counts.entry(w).or_insert(0) += 1;
      }
      // One conversion at the end yields a sorted view.
      let sorted: BTreeMap<_, _> = counts.into_iter().collect();
      for (word, count) in &sorted {
          println!("{word}: {count}");
      }
  }
  ```

  **Verified output:**

  ```text
  a: 3
  b: 2
  c: 1
  ```

- **Prefer `.range(..)` over `.iter().filter(..)`** for windowed queries. `filter` walks the entire map; `.range(..)` jumps straight to the start of the window and stops at the end.

- **Use `first_key_value`/`last_key_value` (peek) and `pop_first`/`pop_last` (remove)** instead of `.iter().next()` gymnastics when you want the min/max entry.

- **Order keys by deriving in field order.** Put the most significant field first in a key struct so the derived `Ord` sorts the way you want.

> **Note:** Need a custom comparison that the derived field order can't express, for example "sort users by score descending, then name ascending"? Implement `Ord`/`PartialOrd` by hand on the key type, or store a transformed key (e.g. `std::cmp::Reverse(score)`). Hand-written `Ord` is covered alongside the other comparison traits in [Section 09](/09-generics-traits/).

---

## Real-World Example

A time-series metric store, the kind you'd find behind a monitoring dashboard. Samples arrive keyed by timestamp; the consumer asks for windowed averages and "the latest value as of time T". A `BTreeMap` makes every one of those a sorted range query.

```rust
use std::collections::BTreeMap;

/// A time-series store keyed by Unix-millisecond timestamps. Because a
/// `BTreeMap` keeps keys sorted, range queries ("everything in this window")
/// are cheap and the results come back in chronological order.
#[derive(Debug, Default)]
struct MetricSeries {
    samples: BTreeMap<u64, f64>,
}

impl MetricSeries {
    fn record(&mut self, timestamp_ms: u64, value: f64) {
        self.samples.insert(timestamp_ms, value);
    }

    /// Average of all samples in the half-open window `[start, end)`.
    fn average(&self, start: u64, end: u64) -> Option<f64> {
        let window: Vec<f64> = self.samples.range(start..end).map(|(_, &v)| v).collect();
        if window.is_empty() {
            return None;
        }
        Some(window.iter().sum::<f64>() / window.len() as f64)
    }

    /// The most recent sample at or before `timestamp_ms`.
    /// `range(..=t).next_back()` walks to the end of the window and grabs the last entry.
    fn latest_at(&self, timestamp_ms: u64) -> Option<(u64, f64)> {
        self.samples
            .range(..=timestamp_ms)
            .next_back()
            .map(|(&t, &v)| (t, v))
    }
}

fn main() {
    let mut cpu = MetricSeries::default();
    cpu.record(1_000, 12.5);
    cpu.record(2_000, 30.0);
    cpu.record(3_000, 45.5);
    cpu.record(4_000, 22.0);
    cpu.record(5_000, 60.0);

    // Average over [2000, 5000): samples at 2000, 3000, 4000.
    println!("avg 2s..5s: {:?}", cpu.average(2_000, 5_000));

    // What was the reading as of t=3500? -> the sample at 3000.
    println!("latest at 3500: {:?}", cpu.latest_at(3_500));

    // No samples in this window -> None, not a crash.
    println!("avg 6s..9s: {:?}", cpu.average(6_000, 9_000));

    // Iterating yields chronological order for free.
    println!("all samples:");
    for (t, v) in &cpu.samples {
        println!("  t={t}: {v}");
    }
}
```

**Verified output:**

```text
avg 2s..5s: Some(32.5)
latest at 3500: Some((3000, 45.5))
avg 6s..9s: None
all samples:
  t=1000: 12.5
  t=2000: 30
  t=3000: 45.5
  t=4000: 22
  t=5000: 60
```

Note the design choices a JavaScript version would make harder: `latest_at` uses `range(..=t).next_back()` — an `O(log n)` "floor" lookup — instead of scanning; values are floats but the **key** is an integer (`u64` millis), sidestepping the `f64: Ord` pitfall entirely; and the chronological iteration at the end needs no sort.

---

## Further Reading

- [`std::collections::BTreeMap`](https://doc.rust-lang.org/std/collections/struct.BTreeMap.html) — full API reference, including `range`, `split_off`, and the `entry` API.
- [`std::collections::BTreeSet`](https://doc.rust-lang.org/std/collections/struct.BTreeSet.html) — the sorted-set reference.
- [`std::collections` module docs](https://doc.rust-lang.org/std/collections/index.html): the standard library's own "which collection should I use?" guide.
- [`std::ops::Bound`](https://doc.rust-lang.org/std/ops/enum.Bound.html): explicit range bounds for `.range(..)`.
- [The `Ord` trait](https://doc.rust-lang.org/std/cmp/trait.Ord.html): what a key type must satisfy.

**Related sections in this guide:**

- [HashMaps](/07-collections/03-hashmaps/) — the unordered `HashMap` and the shared `entry`/`get`/`insert` API.
- [Sets and HashSet](/07-collections/04-hashsets/) — `HashSet` and the set operations that `BTreeSet` shares.
- [Collection Performance](/07-collections/09-collection-performance/): Big-O across all collections and when to pick each.
- [Iterators](/07-collections/06-iterators/) and [Iterator Consumers](/07-collections/07-iterator-consumers/) — `.range(..)`, `.keys()`, etc. all return iterators.
- [Section 09: Generics & Traits](/09-generics-traits/): implementing `Ord` by hand for custom key ordering.
- [Section 08: Error Handling](/08-error-handling/): `Option` returns like `first_key_value()` and the `latest_at` example above.
- [Section 05: Ownership](/05-ownership/) — why `.range(..)` yields references (`&K`, `&V`) you must dereference.
- [Section 02: Basics](/02-basics/) and [Section 00: Introduction](/00-introduction/) — language fundamentals if any syntax here is unfamiliar.

---

## Exercises

### Exercise 1: First repeated word, alphabetically

**Difficulty:** Easy

**Objective:** Use a `BTreeMap` to find the alphabetically-first word that appears two or more times in a sentence.

**Instructions:**

1. Split the input on whitespace and count occurrences with the `entry` API.
2. Because a `BTreeMap` iterates in sorted key order, iterate the counts and return the first word whose count is `>= 2`.
3. Return `Option<String>` so the "no repeats" case is `None`.

```rust
use std::collections::BTreeMap;

fn first_repeated(text: &str) -> Option<String> {
    // TODO: count words, then find the first (alphabetically) with count >= 2
    todo!()
}

fn main() {
    println!("{:?}", first_repeated("zebra apple mango apple zebra"));
    println!("{:?}", first_repeated("one two three"));
}
```

<details><summary>Solution</summary>

```rust
use std::collections::BTreeMap;

fn first_repeated(text: &str) -> Option<String> {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word).or_insert(0) += 1;
    }
    // Iteration is alphabetical, so `find` returns the first qualifying word.
    counts
        .into_iter()
        .find(|&(_, count)| count >= 2)
        .map(|(word, _)| word.to_string())
}

fn main() {
    println!("{:?}", first_repeated("zebra apple mango apple zebra"));
    println!("{:?}", first_repeated("one two three"));
}
```

**Verified output:**

```text
Some("apple")
None
```

`apple` and `zebra` both repeat, but because the `BTreeMap` is sorted, `find` reaches `apple` first.

</details>

### Exercise 2: Score range report

**Difficulty:** Medium

**Objective:** Given a `BTreeMap<u32, Vec<&str>>` mapping a score to the students who earned it, list everyone whose score falls in an inclusive `[lo, hi]` band, in ascending score order.

**Instructions:**

1. Use `.range(lo..=hi)` to select only the scores in the band; do not iterate the whole map.
2. For each `(score, names)` entry, produce one `"name: score"` line per student.
3. Return a `Vec<String>` of those lines.

```rust
use std::collections::BTreeMap;

fn report(scores: &BTreeMap<u32, Vec<&str>>, lo: u32, hi: u32) -> Vec<String> {
    // TODO: range over [lo, hi] and flatten into "name: score" lines
    todo!()
}

fn main() {
    let mut scores: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    scores.insert(72, vec!["Dana"]);
    scores.insert(85, vec!["Alice", "Eve"]);
    scores.insert(91, vec!["Bob"]);
    scores.insert(60, vec!["Frank"]);
    for line in report(&scores, 70, 90) {
        println!("{line}");
    }
}
```

<details><summary>Solution</summary>

```rust
use std::collections::BTreeMap;

fn report(scores: &BTreeMap<u32, Vec<&str>>, lo: u32, hi: u32) -> Vec<String> {
    scores
        .range(lo..=hi)
        // `move` lets the inner closure capture `score` by copy for each entry.
        .flat_map(|(score, names)| names.iter().map(move |n| format!("{n}: {score}")))
        .collect()
}

fn main() {
    let mut scores: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    scores.insert(72, vec!["Dana"]);
    scores.insert(85, vec!["Alice", "Eve"]);
    scores.insert(91, vec!["Bob"]);
    scores.insert(60, vec!["Frank"]);
    for line in report(&scores, 70, 90) {
        println!("{line}");
    }
}
```

**Verified output:**

```text
Dana: 72
Alice: 85
Eve: 85
```

`60` (Frank) and `91` (Bob) fall outside `[70, 90]`, so `.range(..)` skips them entirely. `flat_map` is covered in [Iterators](/07-collections/06-iterators/).

</details>

### Exercise 3: Merge two leaderboards

**Difficulty:** Medium

**Objective:** Combine two `BTreeMap<String, u32>` leaderboards into one, summing the scores of players who appear in both.

**Instructions:**

1. Start from a clone of the first map.
2. For each `(name, score)` in the second map, add the score into the merged map using the `entry` API (insert `0` if the player is new).
3. Return the merged `BTreeMap`, which is automatically sorted by player name.

```rust
use std::collections::BTreeMap;

fn merge(
    a: &BTreeMap<String, u32>,
    b: &BTreeMap<String, u32>,
) -> BTreeMap<String, u32> {
    // TODO: clone `a`, then fold `b`'s scores in with entry().or_insert(0)
    todo!()
}

fn main() {
    let mut q1: BTreeMap<String, u32> = BTreeMap::new();
    q1.insert("alice".into(), 10);
    q1.insert("bob".into(), 5);
    let mut q2: BTreeMap<String, u32> = BTreeMap::new();
    q2.insert("bob".into(), 7);
    q2.insert("carol".into(), 3);
    for (name, total) in merge(&q1, &q2) {
        println!("{name}: {total}");
    }
}
```

<details><summary>Solution</summary>

```rust
use std::collections::BTreeMap;

fn merge(
    a: &BTreeMap<String, u32>,
    b: &BTreeMap<String, u32>,
) -> BTreeMap<String, u32> {
    let mut merged = a.clone();
    for (name, score) in b {
        // entry() returns a mutable handle; or_insert(0) seeds new players.
        *merged.entry(name.clone()).or_insert(0) += score;
    }
    merged
}

fn main() {
    let mut q1: BTreeMap<String, u32> = BTreeMap::new();
    q1.insert("alice".into(), 10);
    q1.insert("bob".into(), 5);
    let mut q2: BTreeMap<String, u32> = BTreeMap::new();
    q2.insert("bob".into(), 7);
    q2.insert("carol".into(), 3);
    for (name, total) in merge(&q1, &q2) {
        println!("{name}: {total}");
    }
}
```

**Verified output:**

```text
alice: 10
bob: 12
carol: 3
```

`bob` appears in both leaderboards, so his `5 + 7 = 12` is summed; the result iterates alphabetically because it is a `BTreeMap`.

</details>

---

_Previous: [HashSets](/07-collections/04-hashsets/) · Next: [Iterators](/07-collections/06-iterators/) · Up: [Section 07 Overview](/07-collections/)_
