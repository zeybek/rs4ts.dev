---
title: "Collection Performance: Big-O, Choosing a Collection, and Capacity"
description: "Choose Rust collections by cost, not feel: Big-O for Vec, HashMap, and BTreeMap, with_capacity pre-allocation, and why iterators compile to tight loops."
---

In JavaScript you reach for `Array`, `Object`, `Map`, and `Set` mostly by feel — the engine hides the cost model, and "it's fast enough" usually holds. Rust gives you the same four shapes of collection (`Vec`, `HashMap`, `BTreeMap`, `HashSet`) but also hands you the cost model directly: you choose the data structure, you choose when memory is allocated, and you choose between a manual loop and a lazy iterator. This page is about making those choices deliberately.

---

## Quick Overview

Every standard collection has documented **Big-O** complexity, and picking the wrong one quietly turns an O(n) job into an O(n²) one. The three decisions that matter most for a TypeScript/JavaScript developer learning Rust are: (1) **which collection** to use: `Vec` for ordered lists, `HashMap` for keyed lookup, `BTreeMap` when you need sorted keys or range queries; (2) **whether to pre-allocate** with `with_capacity`/`reserve` to avoid repeated reallocations; and (3) **iterator vs hand-written loop**, which in Rust produce essentially identical machine code, so you can pick whichever reads better. Unlike a JavaScript engine, the Rust compiler will not silently switch representations behind your back, so the choice is yours and it sticks.

---

## TypeScript/JavaScript Example

A small analytics pass over web request logs. Notice how you reach for each built-in collection almost reflexively, without thinking about its cost.

```typescript
interface LogEntry {
  timestamp: number; // unix seconds
  ip: string;
  path: string;
  status: number;
}

function analyze(logs: LogEntry[]) {
  // Object/Map as a frequency counter — keyed lookup, order irrelevant.
  const hitsPerPath = new Map<string, number>();
  // Set for "have I seen this visitor?" — membership only.
  const uniqueIps = new Set<string>();
  // Another Map, but here we secretly *want* sorted keys for a range query.
  const perMinute = new Map<number, number>();

  for (const entry of logs) {
    hitsPerPath.set(entry.path, (hitsPerPath.get(entry.path) ?? 0) + 1);
    uniqueIps.add(entry.ip);
    const bucket = Math.floor(entry.timestamp / 60);
    perMinute.set(bucket, (perMinute.get(bucket) ?? 0) + 1);
  }

  // A "range query": hits in the first two minutes. With a plain Map you must
  // iterate ALL entries and filter — there is no ordered range lookup.
  let firstTwoMinutes = 0;
  for (const [bucket, count] of perMinute) {
    if (bucket < 2) firstTwoMinutes += count;
  }

  return { uniqueVisitors: uniqueIps.size, hitsPerPath, firstTwoMinutes };
}
```

Two cost questions JavaScript never makes you answer: *Is `perMinute` actually sorted?* (A `Map` preserves **insertion** order, not key order, so the range scan above is O(n) and fragile.) And *how big should these structures be?* (You cannot pre-size a `Map` or `Set` at all.)

---

## Rust Equivalent

The same analysis, but each collection is chosen for its cost characteristics, and the one that needs ordered range queries uses `BTreeMap`.

```rust
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug)]
struct LogEntry {
    timestamp: u64,
    ip: String,
    path: String,
    status: u16,
}

struct Analytics {
    // Count hits per path: keyed lookup, order irrelevant -> HashMap (avg O(1)).
    hits_per_path: HashMap<String, u64>,
    // Distinct visitors: membership only -> HashSet (avg O(1)).
    unique_ips: HashSet<String>,
    // Requests per 60s bucket: we want ORDER and RANGE queries -> BTreeMap (O(log n)).
    per_minute: BTreeMap<u64, u64>,
    errors: u64,
}

impl Analytics {
    fn new(expected_paths: usize) -> Self {
        Analytics {
            // Pre-size the map we expect to fill the most.
            hits_per_path: HashMap::with_capacity(expected_paths),
            unique_ips: HashSet::new(),
            per_minute: BTreeMap::new(),
            errors: 0,
        }
    }

    fn record(&mut self, entry: &LogEntry) {
        *self.hits_per_path.entry(entry.path.clone()).or_insert(0) += 1;
        self.unique_ips.insert(entry.ip.clone());
        let bucket = entry.timestamp / 60;
        *self.per_minute.entry(bucket).or_insert(0) += 1;
        if entry.status >= 500 {
            self.errors += 1;
        }
    }

    // Range query over the SORTED BTreeMap: total hits in [from, to) buckets.
    fn hits_between(&self, from_bucket: u64, to_bucket: u64) -> u64 {
        self.per_minute
            .range(from_bucket..to_bucket)
            .map(|(_, c)| *c)
            .sum()
    }
}

fn main() {
    let logs = vec![
        LogEntry { timestamp: 0,   ip: "1.1.1.1".into(), path: "/home".into(),  status: 200 },
        LogEntry { timestamp: 30,  ip: "2.2.2.2".into(), path: "/home".into(),  status: 200 },
        LogEntry { timestamp: 61,  ip: "1.1.1.1".into(), path: "/login".into(), status: 200 },
        LogEntry { timestamp: 75,  ip: "3.3.3.3".into(), path: "/home".into(),  status: 500 },
        LogEntry { timestamp: 130, ip: "1.1.1.1".into(), path: "/home".into(),  status: 200 },
    ];

    let mut stats = Analytics::new(8);
    for entry in &logs {
        stats.record(entry);
    }

    println!("unique visitors: {}", stats.unique_ips.len());
    println!("5xx errors: {}", stats.errors);
    // hits in the first two minutes — a true O(log n) range query, not a full scan.
    println!("hits in first 2 minutes: {}", stats.hits_between(0, 2));
}
```

Verified output:

```text
unique visitors: 3
5xx errors: 1
hits in first 2 minutes: 4
```

> **Note:** `range(0..2)` walks only the matching slice of the tree, not the whole map. That single capability is the reason to pick `BTreeMap` over `HashMap` even though `HashMap` has faster individual lookups.

---

## Detailed Explanation

### The cost table you should memorize

Here are the average-case complexities for the standard collections. "Amortized" means the cost averaged over many operations (a single one may occasionally be more expensive because of a reallocation).

| Operation                  | `Vec<T>`            | `HashMap<K,V>` / `HashSet<T>` | `BTreeMap<K,V>` / `BTreeSet<T>` |
| -------------------------- | ------------------- | ----------------------------- | ------------------------------- |
| Insert at end / add        | amortized **O(1)**  | average **O(1)**              | **O(log n)**                    |
| Lookup by key              | **O(n)** (scan)     | average **O(1)**              | **O(log n)**                    |
| Random access by index     | **O(1)**            | n/a                           | n/a                             |
| Membership test            | **O(n)** (scan)     | average **O(1)**              | **O(log n)**                    |
| Insert/remove in middle    | **O(n)** (shift)    | n/a                           | **O(log n)**                    |
| Remove from end            | **O(1)**            | average **O(1)**              | **O(log n)**                    |
| Iterate all (n items)      | **O(n)**, in order  | **O(n)**, unordered           | **O(n)**, **sorted** order      |
| Range / sorted query       | **O(n)** (must scan)| **not supported**             | **O(log n + k)**                |
| Min / max                  | **O(n)** unless sorted | **O(n)**                   | **O(log n)** (ends of tree)     |

> **Warning:** The "average O(1)" for `HashMap`/`HashSet` is *average*, not worst case. With adversarial keys a hash collision storm can degrade lookups, which is exactly why Rust's default hasher (SipHash 1-3) is randomized and DoS-resistant; see [HashMaps](/07-collections/03-hashmaps/). For trusted internal keys you can swap in a faster hasher; for untrusted external input the default is the safe choice.

Each operation here can be confirmed against [the standard library docs](https://doc.rust-lang.org/std/collections/index.html#performance), which state the complexity of every method.

### Choosing a collection: the decision flow

The standard-library documentation gives a concise rule of thumb, and it maps cleanly onto what you already do in TypeScript:

- **You have a list and mostly append to the end or iterate it in order → `Vec<T>`.** This is your default, the way `Array` is your default in JavaScript. It is also the most cache-friendly because its elements are contiguous in memory.
- **You look things up by key and don't care about order → `HashMap<K,V>`** (or `HashSet<T>` for keys with no value). This is the `Map`/`Object`/`Set` of JavaScript, and individual lookups are the fastest of the keyed collections.
- **You look things up by key AND need keys kept in sorted order, or you need range queries / min / max → `BTreeMap<K,V>`** (or `BTreeSet<T>`). JavaScript has no built-in equivalent; people fake it with a sorted array plus binary search. See [BTreeMap and BTreeSet](/07-collections/05-btreemap-btreeset/).

A `Vec` of `(key, value)` pairs *can* act like a tiny map — and for a handful of entries it is actually faster than a `HashMap` because there is no hashing and the data is contiguous. But its lookup is a linear O(n) scan, so it collapses as the collection grows. The next section measures exactly that.

### Why "wrong collection" is the most expensive mistake

Using a `Vec` and `contains()` as a stand-in for a `HashSet` is the single most common performance trap. Each `contains()` is an O(n) scan, and doing it inside a loop makes the whole thing O(n²).

```rust
use std::collections::HashSet;

fn main() {
    let input = vec![3, 1, 2, 3, 1, 2, 4];

    // Accidentally O(n^2): contains() scans the whole Vec each time.
    let mut seen: Vec<i32> = Vec::new();
    let mut unique_slow: Vec<i32> = Vec::new();
    for &x in &input {
        if !seen.contains(&x) {     // O(n) linear scan per element
            seen.push(x);
            unique_slow.push(x);
        }
    }
    println!("slow unique = {unique_slow:?}");

    // O(n): HashSet membership is average O(1).
    let mut set: HashSet<i32> = HashSet::new();
    let mut unique_fast: Vec<i32> = Vec::new();
    for &x in &input {
        if set.insert(x) {          // insert returns true if newly added
            unique_fast.push(x);
        }
    }
    println!("fast unique = {unique_fast:?}");
}
```

Verified output:

```text
slow unique = [3, 1, 2, 4]
fast unique = [3, 1, 2, 4]
```

Both produce the same answer, but their cost diverges sharply with size. Timed on 20,000 mostly-unique values (release build):

```text
n = 20000
Vec::contains (O(n^2)) = 10.6615ms
HashSet (O(n))         = 542.333µs
```

> **Note:** Exact timings depend on your machine, and these were a single run; the *ratio* is what matters. At n = 20,000 the `HashSet` version was roughly 20× faster, and the gap widens as n grows because one curve is quadratic and the other linear. This is the same trap as using `Array.includes` inside a loop in JavaScript instead of a `Set`.

### Capacity: the second cost JavaScript hides

A `Vec` (and a `HashMap`/`HashSet`) has both a **length** (how many items it holds) and a **capacity** (how many it can hold before it must allocate a bigger buffer and copy everything over). When you `push` past capacity, the `Vec` reallocates, roughly **doubling** its buffer. Those reallocations are the "amortized" part of "amortized O(1)".

If you know roughly how many items you'll add, tell the collection up front with `with_capacity`:

```rust
fn main() {
    // Count reallocations: Vec::new() vs Vec::with_capacity(n).
    let mut grown: Vec<u64> = Vec::new();
    let mut reallocs = 0;
    let mut cap = grown.capacity();
    for i in 0..1_000u64 {
        grown.push(i);
        if grown.capacity() != cap {
            reallocs += 1;
            cap = grown.capacity();
        }
    }
    println!("Vec::new()       -> {reallocs} reallocations to reach len {}", grown.len());

    let mut sized: Vec<u64> = Vec::with_capacity(1_000);
    let mut reallocs2 = 0;
    let mut cap2 = sized.capacity();
    for i in 0..1_000u64 {
        sized.push(i);
        if sized.capacity() != cap2 {
            reallocs2 += 1;
            cap2 = sized.capacity();
        }
    }
    println!("with_capacity()  -> {reallocs2} reallocations to reach len {}", sized.len());
}
```

Verified output:

```text
Vec::new()       -> 9 reallocations to reach len 1000
with_capacity()  -> 0 reallocations to reach len 1000
```

Nine reallocations (each copying the entire buffer) versus zero. `HashMap` and `HashSet` work the same way: `HashMap::with_capacity(n)` and `reserve(n)` pre-size them. The deep-dive on `Vec` capacity and growth lives in [Vectors](/07-collections/00-vectors/); this page is about *when* the pre-allocation is worth it.

```rust
use std::collections::HashMap;

fn main() {
    // HashMap also supports pre-allocation.
    let mut counts: HashMap<&str, u32> = HashMap::with_capacity(16);
    let words = ["a", "b", "a", "c", "a", "b"];
    for w in words {
        *counts.entry(w).or_insert(0) += 1;
    }
    let mut pairs: Vec<(&str, u32)> = counts.into_iter().collect();
    pairs.sort(); // HashMap order is randomized; sort for stable display
    println!("{pairs:?}");

    // reserve() grows capacity ahead of a known bulk insert.
    let mut m: HashMap<u32, u32> = HashMap::new();
    m.reserve(1_000);
    println!("capacity after reserve(1000): >= {}", m.capacity() >= 1000);
}
```

Verified output:

```text
[("a", 3), ("b", 2), ("c", 1)]
capacity after reserve(1000): >= true
```

When you build a collection from an iterator with `.collect()`, you usually get this for free: the iterator reports a size hint and `collect` pre-allocates accordingly, so an explicit `with_capacity` is rarely needed in that case. Reclaiming over-allocated memory is the job of `shrink_to_fit`:

```rust
fn main() {
    let mut v: Vec<i32> = Vec::with_capacity(100);
    v.extend([1, 2, 3]);
    println!("before shrink: len={}, cap={}", v.len(), v.capacity());
    v.shrink_to_fit(); // hand the unused capacity back to the allocator
    println!("after shrink:  len={}, cap={}", v.len(), v.capacity());
}
```

Verified output:

```text
before shrink: len=3, cap=100
after shrink:  len=3, cap=3
```

### Iterator vs loop: a non-choice for performance

In JavaScript, `arr.map().filter().reduce()` allocates one or more intermediate arrays and runs several passes, so it can be meaningfully slower than a single hand-written `for` loop on a hot path. **In Rust this trade-off essentially does not exist.** Iterator adaptors are lazy and get fused and inlined by the compiler into the same machine code a manual loop would produce. This is what "zero-cost abstraction" means. The iterator chain also drops per-element bounds checks that a manual `v[i]` would incur.

```rust
fn main() {
    let data: Vec<u64> = (0..1_000_000).collect();

    // Manual index loop — every `data[i]` is bounds-checked.
    let mut sum_loop: u64 = 0;
    for i in 0..data.len() {
        sum_loop += data[i];
    }

    // Iterator chain — no manual indexing, no per-element bounds checks.
    let sum_iter: u64 = data.iter().sum();

    // Filtered + mapped sum, the iterator way (still one pass, no intermediate Vec).
    let even_doubled: u64 = data.iter().filter(|&&x| x % 2 == 0).map(|&x| x * 2).sum();

    println!("sum_loop      = {sum_loop}");
    println!("sum_iter      = {sum_iter}");
    println!("even_doubled  = {even_doubled}");
}
```

Verified output:

```text
sum_loop      = 499999500000
sum_iter      = 499999500000
even_doubled  = 499999000000
```

Because the iterator version compiles to equivalent (often better) code, the rule is: **write whichever is clearest, which is almost always the iterator.** The important difference from JavaScript is that `.filter().map().sum()` here makes **no intermediate `Vec`** — adaptors are lazy and only pull one element through the whole chain at a time. The mechanics of laziness are covered in [Iterators](/07-collections/06-iterators/) and [Iterator Consumers](/07-collections/07-iterator-consumers/). Pre-sizing applies to iterators too: `collect()` uses the size hint, as below.

```rust
fn main() {
    // collect into a Vec sizes the allocation from the iterator's size hint.
    let squares: Vec<u64> = (1..=5u64).map(|n| n * n).collect();
    println!("squares = {squares:?}");
}
```

Verified output:

```text
squares = [1, 4, 9, 16, 25]
```

---

## Key Differences

| Concern                        | TypeScript / JavaScript                          | Rust                                                            |
| ------------------------------ | ------------------------------------------------ | --------------------------------------------------------------- |
| Cost model                     | Opaque; engine may reshape `Object`/`Array`      | Explicit and documented per method                              |
| Sorted-key map                 | None built in (sorted array + binary search)     | `BTreeMap` / `BTreeSet`, O(log n) with range queries            |
| Pre-allocation                 | Not exposed for `Map`/`Set`; `Array(n)` is hole-y| `with_capacity(n)` / `reserve(n)` on all growable collections   |
| `Map`/`Object` iteration order | Insertion order (deterministic)                  | `HashMap` order is **randomized**; `BTreeMap` is **sorted**     |
| `map`/`filter` chains          | Allocate intermediate arrays, multiple passes    | Lazy, fused, zero intermediate allocations                      |
| Loop vs higher-order method    | Loop can be measurably faster on hot paths       | Iterator ≈ loop; pick for readability                           |
| Hash collisions / DoS          | Engine-dependent                                 | Default `HashMap` hasher is randomized + DoS-resistant          |
| Memory layout                  | Engine-managed, often pointer-chasing            | `Vec` is contiguous (cache-friendly); maps/sets hash- or tree-backed |

### Why Rust exposes all of this

JavaScript optimizes for "don't make the developer think." Rust optimizes for "the developer is in control and nothing is hidden." That means *you* pick the collection, *you* decide when memory is allocated, and the compiler guarantees the cost model won't change underneath you. The upside is predictable, tunable performance with no surprise deopts; the cost is that choosing badly (a `Vec` scan where a `HashMap` belongs) is on you, not the runtime.

---

## Common Pitfalls

### Pitfall 1: Using `Vec::contains` in a loop (accidental O(n²))

This is the trap shown above, restated as the mistake itself. If you find yourself calling `.contains()`, `.iter().any()`, or `.iter().find()` inside a loop over the same data, you almost certainly want a `HashSet` or `HashMap`. The code compiles and is *correct*. It is just quadratic, and it will not show up until your inputs grow. There is no compiler error here; this is a design pitfall the type checker cannot catch for you.

**Fix:** build a `HashSet`/`HashMap` once (O(n)) and do O(1) lookups against it, as in the `unique_fast` example above.

### Pitfall 2: Expecting `HashMap` to iterate in insertion or sorted order

Coming from JavaScript, where `Map` and modern `Object` preserve insertion order, it is natural to assume iteration order is stable. A Rust `HashMap` iterates in an **unspecified, randomized** order that can differ between runs of the same program.

```rust
use std::collections::HashMap;

fn main() {
    let mut m = HashMap::new();
    m.insert("a", 1);
    m.insert("b", 2);
    m.insert("c", 3);
    // The order of this loop is NOT guaranteed and may change run-to-run.
    for (k, v) in &m {
        println!("{k} = {v}");
    }
}
```

**Fix:** if you need a deterministic order, either collect into a `Vec` and `sort()` it, or use a `BTreeMap` (always sorted by key). Relying on `HashMap` order is a latent bug, not a performance issue.

### Pitfall 3: `Vec::dedup` without sorting first

`dedup` only removes **consecutive** duplicates; it is not a general "make unique" operation.

```rust
fn main() {
    let mut v = vec![1, 2, 1, 1, 3];
    v.dedup();
    println!("{v:?}"); // the non-adjacent 1 survives
}
```

Verified output:

```text
[1, 2, 1, 3]
```

**Fix:** `v.sort(); v.dedup();` to remove all duplicates while keeping it O(n log n) and order-by-value, **or** use a `HashSet` if you don't need sorting and want O(n). Choosing between them is a performance decision: sort+dedup keeps the result sorted; the `HashSet` route is faster but loses order.

### Pitfall 4: Pre-allocating the wrong thing (or everywhere)

`with_capacity` only helps when you would otherwise reallocate repeatedly while growing a single collection to a size you can estimate. Sprinkling it on every `Vec::new()` (including ones you build with `.collect()`, which already pre-sizes, or ones that stay tiny) adds noise without benefit and can even waste memory if your estimate is too high.

**Fix:** pre-allocate when (a) you know the approximate final size and (b) you are growing element-by-element in a hot loop. Otherwise let `collect()` or the default growth strategy handle it, and reach for `shrink_to_fit` only if you over-allocated and the slack matters.

---

## Best Practices

### 1. Default to `Vec`, escalate deliberately

Start with `Vec<T>`. Move to `HashMap`/`HashSet` the moment you do keyed lookups or membership tests more than occasionally. Move to `BTreeMap`/`BTreeSet` only when you genuinely need sorted iteration, range queries, or min/max — they pay O(log n) for that ordering.

### 2. Pre-allocate on hot, size-known paths

When a loop pushes a predictable number of items, call `Vec::with_capacity(n)` / `HashMap::with_capacity(n)` first. It converts O(log n) reallocations (each an O(n) copy) into zero. For collections built by `.collect()`, trust the automatic size hint instead.

### 3. Prefer iterators; they are not slower

Write `.iter().filter(...).map(...).sum()` over manual index loops. It is clearer, it drops bounds checks, it allocates no intermediates, and it compiles to equivalent machine code. Reserve manual loops for cases the iterator genuinely cannot express cleanly.

### 4. Measure before micro-optimizing

For real performance work, use the [Criterion](https://docs.rs/criterion) benchmarking crate rather than ad-hoc `Instant::now()` timing; it handles warm-up, statistical noise, and outliers. The quick timings on this page are illustrative; Criterion is what you'd commit to a repo. See [Section 21 — Performance](/21-performance/) for the full treatment.

### 5. Pick the right hasher for the situation

The default `HashMap` hasher is DoS-resistant, which is right for untrusted keys (user input, network data). For trusted internal keys where speed dominates, a faster non-cryptographic hasher (e.g. the `fxhash` or `ahash` crate) can be a meaningful win. Default first; swap only after measuring. See [HashMaps](/07-collections/03-hashmaps/).

---

## Real-World Example

Deduplicating a large stream of IDs is a textbook "wrong collection" decision. Here both approaches are factored into functions and timed at scale so you can see the curve rather than take it on faith.

```rust
use std::collections::HashSet;
use std::time::Instant;

/// O(n^2): each `contains` linearly scans everything seen so far.
fn dedup_vec(input: &[u32]) -> Vec<u32> {
    let mut seen: Vec<u32> = Vec::new();
    let mut out = Vec::new();
    for &x in input {
        if !seen.contains(&x) {
            seen.push(x);
            out.push(x);
        }
    }
    out
}

/// O(n): HashSet membership is average O(1); `insert` returns whether it was new.
fn dedup_set(input: &[u32]) -> Vec<u32> {
    let mut seen: HashSet<u32> = HashSet::with_capacity(input.len());
    let mut out = Vec::with_capacity(input.len());
    for &x in input {
        if seen.insert(x) {
            out.push(x);
        }
    }
    out
}

fn main() {
    let input: Vec<u32> = (0..20_000).collect();

    let t = Instant::now();
    let a = dedup_vec(&input);
    let vec_time = t.elapsed();

    let t = Instant::now();
    let b = dedup_set(&input);
    let set_time = t.elapsed();

    assert_eq!(a.len(), b.len());
    println!("n = {}", input.len());
    println!("Vec::contains (O(n^2)) = {vec_time:?}");
    println!("HashSet (O(n))         = {set_time:?}");
}
```

Verified output (single run, release build; your numbers will differ, the *ratio* is the point):

```text
n = 20000
Vec::contains (O(n^2)) = 10.6615ms
HashSet (O(n))         = 542.333µs
```

The `HashSet` version is both correct and roughly 20× faster here, and the gap grows with n. Note the deliberate `with_capacity(input.len())` calls in `dedup_set`: we know the upper bound on output size, so we pre-allocate and avoid all reallocation. This is the whole chapter in one example: right collection, right pre-allocation, idiomatic iteration.

> **Tip:** If you don't need to preserve first-seen order, the entire `dedup_set` body collapses to one line — `input.iter().copied().collect::<HashSet<_>>()` — and then `.into_iter().collect::<Vec<_>>()` if you want a `Vec` back. The explicit loop is shown here to make the cost visible.

---

## Further Reading

### Official Documentation

- [`std::collections` — module docs, including "Performance" and "Which collection?"](https://doc.rust-lang.org/std/collections/index.html): the canonical decision guide and complexity tables
- [`Vec::with_capacity`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.with_capacity) and [`Vec::reserve`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.reserve)
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/): collections, allocations, and how to measure them
- [Zero-cost abstractions (Rust Book, Iterators chapter)](https://doc.rust-lang.org/book/ch13-04-performance.html) — why iterators match hand-written loops

### Related Topics in This Guide

- [Vectors](/07-collections/00-vectors/): `Vec` capacity, growth, and `with_capacity` in depth
- [HashMaps](/07-collections/03-hashmaps/): the entry API, key/value ownership, and the default hasher
- [HashSets](/07-collections/04-hashsets/): membership and set algebra at O(1)
- [BTreeMap and BTreeSet](/07-collections/05-btreemap-btreeset/): when sorted keys and range queries earn their O(log n)
- [Iterators](/07-collections/06-iterators/) and [Iterator Consumers](/07-collections/07-iterator-consumers/) — lazy evaluation and why chains don't allocate
- [Section 05 — Ownership](/05-ownership/) — why moving a collection is O(1) but cloning is O(n)
- [Section 21 — Performance](/21-performance/) — Criterion benchmarking and profiling

---

## Exercises

### Exercise 1: Pick the Right Counter

**Difficulty:** Beginner

**Objective:** Choose the correct collection for keyed counting and pre-size it.

**Instructions:** Write `word_frequencies(text: &str) -> HashMap<String, u32>` that counts case-insensitive word frequencies (split on whitespace). Use the entry API so you never look a key up twice, and pre-size the map. Print the pairs sorted for a stable result.

```rust
use std::collections::HashMap;

fn word_frequencies(text: &str) -> HashMap<String, u32> {
    // TODO: pre-size the map, then count each lowercased word with the entry API
    todo!()
}

fn main() {
    let freq = word_frequencies("the cat the dog the bird");
    let mut pairs: Vec<(&String, &u32)> = freq.iter().collect();
    pairs.sort();
    println!("{pairs:?}");
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

fn word_frequencies(text: &str) -> HashMap<String, u32> {
    let mut freq: HashMap<String, u32> = HashMap::with_capacity(text.len() / 5);
    for word in text.split_whitespace() {
        *freq.entry(word.to_lowercase()).or_insert(0) += 1;
    }
    freq
}

fn main() {
    let freq = word_frequencies("the cat the dog the bird");
    let mut pairs: Vec<(&String, &u32)> = freq.iter().collect();
    pairs.sort();
    println!("{pairs:?}");
}
```

Output:

```text
[("bird", 1), ("cat", 1), ("dog", 1), ("the", 3)]
```

The `entry(...).or_insert(0)` pattern does a single hash lookup that both finds-or-creates the slot; `*... += 1` mutates it in place. `with_capacity(text.len() / 5)` is a rough guess at the word count to avoid reallocations.

</details>

### Exercise 2: Eliminate the Reallocations

**Difficulty:** Intermediate

**Objective:** Pre-allocate a `Vec` so that building it triggers zero reallocations.

**Instructions:** Write `build_ids(n: usize) -> Vec<u64>` that returns the squares `0, 1, 4, 9, ...` for `0..n`. Build it with a loop (not `.collect()`) but pre-allocate so the capacity never grows mid-loop. Print the result and its capacity to prove it equals `n`.

```rust
fn build_ids(n: usize) -> Vec<u64> {
    // TODO: pre-allocate, then push n squared values with no reallocation
    todo!()
}

fn main() {
    let ids = build_ids(5);
    println!("{ids:?}, cap={}", ids.capacity());
}
```

<details>
<summary>Solution</summary>

```rust
fn build_ids(n: usize) -> Vec<u64> {
    let mut ids = Vec::with_capacity(n); // exact size known -> zero reallocations
    for i in 0..n as u64 {
        ids.push(i * i);
    }
    ids
}

fn main() {
    let ids = build_ids(5);
    println!("{ids:?}, cap={}", ids.capacity());
}
```

Output:

```text
[0, 1, 4, 9, 16], cap=5
```

Because the capacity (5) exactly matches the number of pushes, the buffer is allocated once and never copied. With `Vec::new()` this would have reallocated a few times as it grew through 4 → 8.

</details>

### Exercise 3: Range Query Needs the Right Map

**Difficulty:** Advanced

**Objective:** Use a `BTreeMap` to answer a sorted range query that a `HashMap` cannot do efficiently.

**Instructions:** Given a list of `(name, score)` pairs, build a `BTreeMap<u32, &str>` keyed by score. Then (a) print everyone whose score falls in the B band `[80, 90)` using a range query, and (b) print the top scorer by reading the last entry of the sorted map — no manual scan over all entries.

```rust
use std::collections::BTreeMap;

fn main() {
    let scores = [("ana", 91), ("ben", 72), ("cy", 85), ("dee", 60), ("ed", 95)];
    // TODO: build a BTreeMap<u32, &str> keyed by score
    // TODO: print the B band [80, 90) via a range query
    // TODO: print the top scorer using the sorted order (no full scan)
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::BTreeMap;

fn main() {
    let scores = [("ana", 91), ("ben", 72), ("cy", 85), ("dee", 60), ("ed", 95)];

    let mut by_score: BTreeMap<u32, &str> = BTreeMap::new();
    for (name, score) in scores {
        by_score.insert(score, name);
    }

    // Range query: the B band, scores in [80, 90). O(log n + k), not a full scan.
    let b_band: Vec<&str> = by_score.range(80..90).map(|(_, &name)| name).collect();
    println!("B band (80..90): {b_band:?}");

    // Top scorer is simply the last (highest-key) entry of the sorted map.
    if let Some((score, name)) = by_score.iter().next_back() {
        println!("top: {name} with {score}");
    }
}
```

Output:

```text
B band (80..90): ["cy"]
top: ed with 95
```

`range(80..90)` walks only the matching slice of the tree, and `iter().next_back()` reads the maximum key in O(log n) — both are impossible to do efficiently on a `HashMap`, which has no order at all. (If two students could share a score you'd key by a tuple or store a `Vec` of names per score; here scores are unique.)

</details>
