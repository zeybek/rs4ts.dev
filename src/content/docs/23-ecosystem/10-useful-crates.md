---
title: "Other Essential Crates: itertools, rayon, LazyLock, uuid, indexmap, bytes, dashmap"
description: "Seven Rust workhorses TS devs need: itertools, rayon, LazyLock, uuid, indexmap, bytes, dashmap for parallelism, ordered maps, and shared concurrent state."
---

## Quick Overview

Beyond the headline crates (serde, tokio, reqwest, clap), every working Rust project quickly accumulates a second tier of utilities that fill gaps a Node developer never thinks about, because in JavaScript they are either built into the language or live in one-line npm packages. This page covers seven of those workhorses: **itertools** (lodash-style iterator adapters), **rayon** (one-line data parallelism with no `Worker` boilerplate), **once_cell**/**LazyLock** (lazy global initialization), **uuid** (id generation), **indexmap** (a map that remembers insertion order, like a JS `Map`), **bytes** (cheap, refcounted byte buffers for network code), and **dashmap** (a concurrent `HashMap` you can share across threads). Knowing these saves you from reinventing them or reaching for `unsafe`.

> **Note:** This page is the grab-bag of general-purpose utilities. The big, topic-specific crates live in their own pages: see [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/) for the overview, [Async Runtimes](/23-ecosystem/02-async-runtimes/) for Tokio, [HTTP Clients](/23-ecosystem/06-http-clients/) for reqwest, [Date and Time](/23-ecosystem/07-date-time/) for chrono, [Regular Expressions with the `regex` Crate](/23-ecosystem/08-regex/) for the regex crate, and [Parsing](/23-ecosystem/09-parsing/) for nom/pest.

---

## TypeScript/JavaScript Example

In Node, most of what this page covers is either a language built-in or a tiny dependency. Here is the everyday toolbox a TypeScript developer reaches for:

```typescript
// The Node equivalents of everything on this page.
import { randomUUID } from "node:crypto";   // built-in UUID v4
import { groupBy, uniq, zip } from "lodash"; // iterator helpers
import { Worker } from "node:worker_threads"; // parallelism (heavy boilerplate)

// 1. Iterator helpers — lodash fills the gaps in Array.prototype.
const orders = [
  { customer: "alice", amount: 30 },
  { customer: "bob", amount: 10 },
  { customer: "alice", amount: 12 },
];
const byCustomer = groupBy(orders, (o) => o.customer);
// { alice: [ {..30}, {..12} ], bob: [ {..10} ] }

// 2. A lazily-initialized singleton (computed once, on first use).
let _config: Map<string, number> | undefined;
function settings(): Map<string, number> {
  if (!_config) {
    _config = new Map([["retries", 3], ["timeout", 30]]);
  }
  return _config;
}

// 3. UUIDs.
const id = randomUUID(); // "67e55044-10b1-426f-9247-bb680e5fe0c8"

// 4. A Map keeps insertion order; a plain object mostly does too.
const ordered = new Map<string, number>();
ordered.set("zulu", 1);
ordered.set("alpha", 2);
console.log([...ordered.keys()]); // ['zulu', 'alpha'] — insertion order

// 5. Concurrency on shared state is *not* a problem in Node:
//    one thread, one event loop, so a plain object is "thread-safe".
const counts: Record<string, number> = {};
counts["hits"] = (counts["hits"] ?? 0) + 1;
```

Three things about this code shape what Rust does differently:

- **Parallelism is exotic in Node.** `worker_threads` means serializing data across a thread boundary; almost nobody reaches for it casually. Rust makes CPU parallelism a one-line change.
- **Shared mutable state is "free" in Node** because there is only one thread. In Rust, sharing a `HashMap` across threads does not compile: you need a concurrency-aware type like `dashmap`.
- **`Map` remembers insertion order; `Object` mostly does.** Rust's default `HashMap` is deliberately *unordered* (and randomized), so when you need `Map`-like ordering you reach for `indexmap`.

---

## Rust Equivalent

The same five concerns, the idiomatic Rust way. First the dependencies:

```toml
# Cargo.toml — add these with `cargo add` so versions resolve to current stable.
[dependencies]
itertools = "0.14"
rayon = "1.12"
uuid = { version = "1.23", features = ["v4", "v7"] }
indexmap = "2.14"
bytes = "1.11"
dashmap = "6.2"
# once_cell is optional now — std's LazyLock (stable since Rust 1.80) covers most uses.
```

```rust
use itertools::Itertools;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::LazyLock;
use indexmap::IndexMap;
use dashmap::DashMap;
use uuid::Uuid;

// 2. A lazily-initialized global, computed once on first access. No `if (!_x)` dance.
static SETTINGS: LazyLock<HashMap<&'static str, i32>> = LazyLock::new(|| {
    HashMap::from([("retries", 3), ("timeout", 30)])
});

fn main() {
    // 1. Iterator helpers — itertools is lodash for Rust's iterators.
    let words = ["apple", "banana", "apple", "cherry", "banana", "apple"];
    let counts = words.iter().counts(); // HashMap<&&str, usize>
    let mut counts_sorted: Vec<_> = counts.into_iter().collect();
    counts_sorted.sort();
    println!("counts: {counts_sorted:?}");

    // 3. Parallelism — change `.iter()` to `.par_iter()` and rayon does the rest.
    let total: u64 = (1..=1_000_000u64).into_par_iter().filter(|n| n % 3 == 0).sum();
    println!("rayon sum: {total}");

    // 2. The global is initialized lazily, on first read.
    println!("retries: {}", SETTINGS["retries"]);

    // 4. UUIDs.
    let id = Uuid::new_v4();
    println!("uuid v4 has {} chars", id.to_string().len());

    // 5. A map that preserves insertion order, like a JS Map.
    let mut ordered: IndexMap<&str, i32> = IndexMap::new();
    ordered.insert("zulu", 1);
    ordered.insert("alpha", 2);
    println!("indexmap keys: {:?}", ordered.keys().collect::<Vec<_>>());

    // 6. A concurrent map you can share across threads without a Mutex<HashMap>.
    let hits: DashMap<&str, i32> = DashMap::new();
    *hits.entry("hits").or_insert(0) += 1;
    println!("dashmap hits: {}", *hits.get("hits").unwrap());
}
```

Running it prints:

```text
counts: [("apple", 3), ("banana", 2), ("cherry", 1)]
rayon sum: 166666833333
retries: 3
uuid v4 has 36 chars
indexmap keys: ["zulu", "alpha"]
dashmap hits: 1
```

---

## Detailed Explanation

### itertools — lodash for iterators

Rust's standard `Iterator` trait is already richer than `Array.prototype` (`map`, `filter`, `take`, `skip`, `flat_map`, `zip`, `fold`...), but it deliberately omits anything that would need allocation or buffering. **itertools** is the crate that adds them. You bring its methods into scope with one `use itertools::Itertools;` and they appear on every iterator.

A few you will reach for constantly:

```rust
use itertools::Itertools;

fn main() {
    // join: like Array.prototype.join, but on any iterator of Display values.
    let joined = ["a", "b", "c"].iter().join(", ");
    println!("{joined}"); // a, b, c

    // counts: a frequency map in one call (lodash's countBy).
    let counts = ["x", "y", "x", "x"].iter().counts();
    println!("{:?}", counts.get(&&"x")); // Some(3)

    // cartesian_product: every pair, no nested loops.
    let pairs: Vec<(i32, char)> =
        [1, 2].iter().copied().cartesian_product(['x', 'y']).collect();
    println!("{pairs:?}"); // [(1, 'x'), (1, 'y'), (2, 'x'), (2, 'y')]

    // sorted + dedup: sort then remove consecutive duplicates.
    let unique: Vec<i32> = [3, 1, 2, 3, 1].iter().copied().sorted().dedup().collect();
    println!("{unique:?}"); // [1, 2, 3]

    // chunk_by: group *consecutive* runs by a key (like a streaming groupBy).
    let runs: Vec<(bool, Vec<i32>)> = [1, 2, 4, 3, 5, 6]
        .iter()
        .copied()
        .chunk_by(|n| n % 2 == 0)
        .into_iter()
        .map(|(k, g)| (k, g.collect()))
        .collect();
    println!("{runs:?}"); // [(false, [1]), (true, [2, 4]), (false, [3, 5]), (true, [6])]
}
```

This prints:

```text
a, b, c
Some(3)
[(1, 'x'), (1, 'y'), (2, 'x'), (2, 'y')]
[1, 2, 3]
[(false, [1]), (true, [2, 4]), (false, [3, 5]), (true, [6])]
```

> **Warning:** `chunk_by` groups **consecutive** equal keys, exactly like Unix `uniq` and unlike lodash's `groupBy`, which gathers *all* matching items regardless of position. To get lodash semantics, `.sorted_by(...)` first (so equal keys are adjacent), then `chunk_by`. We do exactly that in the Real-World Example below.

itertools also adds `izip!`, which zips three or more iterators at once. std's `zip` only takes two:

```rust
use itertools::izip;

fn main() {
    let names = ["a", "b", "c"];
    let ages = [30, 25, 40];
    let cities = ["NYC", "LA", "SF"];
    for (n, age, city) in izip!(names, ages, cities) {
        println!("{n} {age} {city}");
    }
}
```

```text
a 30 NYC
b 25 LA
c 40 SF
```

### rayon — data parallelism for the price of one method call

This is the crate with the best effort-to-reward ratio in the ecosystem. To parallelize a sequential iterator chain, you change `.iter()` to `.par_iter()` (or `.into_iter()` to `.into_par_iter()`) and add `use rayon::prelude::*;`. rayon spreads the work across a thread pool sized to your CPU cores using work-stealing, and the compiler still enforces that your closure does not race on shared data.

```rust
use rayon::prelude::*;

fn main() {
    // Sequential: .iter()  →  Parallel: .par_iter()
    let total: u64 = (1..=1_000_000u64)
        .into_par_iter()
        .filter(|n| n % 3 == 0)
        .sum();
    println!("sum of multiples of 3: {total}");

    // par_sort sorts a slice across all cores in place.
    let mut data: Vec<i64> = (0..10).rev().collect();
    data.par_sort();
    println!("{data:?}");

    // reduce needs an identity value and an *associative* combining op.
    let factorial: u64 = (1..=10u64).into_par_iter().reduce(|| 1, |a, b| a * b);
    println!("10! = {factorial}");
}
```

```text
sum of multiples of 3: 166666833333
[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
10! = 3628800
```

Compare this to Node, where the same CPU-bound work would require spawning `Worker` threads, serializing inputs into them with `postMessage`, and reassembling the results: dozens of lines for what rayon expresses in one. The reason Rust can do this safely is the same `Send`/`Sync` machinery that powers the rest of the language: if your closure tried to mutate captured state without synchronization, it would not compile. This is the "fearless concurrency" promise made concrete. (rayon is for **CPU-bound** parallelism; for **I/O-bound** concurrency you want async and Tokio — see [Async Runtimes](/23-ecosystem/02-async-runtimes/).)

### once_cell / LazyLock — lazy globals done right

A global that is expensive to build (a compiled regex, a config table, a connection registry) should be initialized once, on first use, and then shared. In JavaScript you write the `if (!_x) _x = ...` lazy-init pattern by hand. Rust gives you two tools:

- **`std::sync::LazyLock`** — built into the standard library, **stable since Rust 1.80**. Prefer this in new code; it needs no dependency.
- **`once_cell::sync::Lazy`**: the original crate that `LazyLock` was modeled on. You will still see it everywhere in existing code and crates that support older compilers, and its API is nearly identical.

```rust
use once_cell::sync::Lazy;
use std::sync::LazyLock;
use std::collections::HashMap;

// once_cell crate (pre-1.80 idiom, still extremely common in the wild).
static REGISTRY_OLD: Lazy<HashMap<&str, u32>> =
    Lazy::new(|| HashMap::from([("alpha", 1), ("beta", 2)]));

// std LazyLock (prefer this in new code — no dependency needed).
static REGISTRY_NEW: LazyLock<HashMap<&str, u32>> =
    LazyLock::new(|| HashMap::from([("alpha", 1), ("beta", 2)]));

fn main() {
    println!("once_cell: {}", REGISTRY_OLD["beta"]);
    println!("LazyLock:  {}", REGISTRY_NEW["beta"]);
}
```

```text
once_cell: 2
LazyLock:  2
```

The closure runs exactly once, the first time the static is accessed, and the result is cached for the program's lifetime. Both types are thread-safe: if two threads race to first-access, one wins the initialization and the other blocks until it completes. The migration is mechanical: `once_cell::sync::Lazy` becomes `std::sync::LazyLock` with the same closure.

> **Note:** once_cell also offers `OnceCell`/`Lazy` for the non-thread-safe (`unsync`) case and a `get_or_init` API; std mirrors these as `OnceCell`/`OnceLock`. For a plain "initialize once, no closure stored" cell, std's `OnceLock` is the analogue of `once_cell::sync::OnceCell`.

### uuid — id generation

The `uuid` crate generates and parses UUIDs. You opt into the versions you need via Cargo features; the two you want today are **v4** (random) and **v7** (timestamp-ordered, the modern default for database keys because the ids sort by creation time, which is friendlier to B-tree indexes).

```rust
use uuid::Uuid;

fn main() {
    let random = Uuid::new_v4();   // fully random (like Node's randomUUID)
    let ordered = Uuid::now_v7();  // time-sortable; great for DB primary keys

    println!("v4: {random}");
    println!("v7 version number: {}", ordered.get_version_num());

    // Parse a UUID from a string (returns Result, no exceptions).
    let parsed = Uuid::parse_str("67e55044-10b1-426f-9247-bb680e5fe0c8").unwrap();
    println!("parsed: {parsed}");
}
```

A sample run (the random parts differ each time):

```text
v4: 9f1d8c2a-...-...-...-............
v7 version number: 7
parsed: 67e55044-10b1-426f-9247-bb680e5fe0c8
```

Node's built-in `crypto.randomUUID()` only gives you v4. If you want time-ordered ids in Node you need a third-party package; in Rust it is a one-feature flag away.

### indexmap — a map that remembers insertion order

Rust's standard `HashMap` is **unordered**, and its iteration order is even randomized per-run to discourage you from depending on it. That is the opposite of a JavaScript `Map`, which guarantees insertion order. When you need that guarantee — serializing config back out in a stable order, building an ordered cache, preserving the order of HTTP headers — reach for **indexmap**.

```rust
use indexmap::IndexMap;

fn main() {
    let mut map: IndexMap<&str, i32> = IndexMap::new();
    map.insert("zulu", 1);
    map.insert("alpha", 2);
    map.insert("mike", 3);

    // Iterates in insertion order, like a JS Map.
    println!("keys: {:?}", map.keys().collect::<Vec<_>>());

    // Bonus: positional access, which a HashMap cannot do.
    println!("first entry: {:?}", map.get_index(0));
}
```

```text
keys: ["zulu", "alpha", "mike"]
first entry: Some(("zulu", 1))
```

> **Warning:** `IndexMap` has two ways to remove an entry, and they are not interchangeable. `swap_remove(key)` is O(1) but moves the *last* element into the gap, **breaking order**. `shift_remove(key)` is O(n) but **preserves order** by sliding subsequent entries down. If you chose `IndexMap` *for* its ordering, you almost always want `shift_remove`. The default `.remove()` was deliberately removed from the API to force this choice:

```rust
use indexmap::IndexMap;

fn main() {
    let mut a: IndexMap<&str, i32> = ["a", "b", "c", "d"].iter().map(|&k| (k, 0)).collect();
    a.swap_remove("b");  // moves "d" into b's slot — order broken
    println!("swap_remove:  {:?}", a.keys().collect::<Vec<_>>());

    let mut b: IndexMap<&str, i32> = ["a", "b", "c", "d"].iter().map(|&k| (k, 0)).collect();
    b.shift_remove("b"); // shifts "c","d" down — order preserved
    println!("shift_remove: {:?}", b.keys().collect::<Vec<_>>());
}
```

```text
swap_remove:  ["a", "d", "c"]
shift_remove: ["a", "c", "d"]
```

### bytes — cheap, refcounted byte buffers

Network and protocol code constantly slices and shares byte buffers. Copying them every time is wasteful, and in Node you reach for `Buffer.slice` (which shares memory) or `Buffer.subarray`. The **bytes** crate is the Rust equivalent and the foundation that hyper, Tokio, and most of the HTTP stack are built on. Its key types are `Bytes` (immutable, cheaply cloneable) and `BytesMut` (a growable buffer you build up, then `freeze()` into `Bytes`).

The key property: **cloning a `Bytes` is O(1)**. It bumps a reference count and shares the underlying allocation rather than copying the data. Slicing is likewise O(1) and shares memory.

```rust
use bytes::{Bytes, BytesMut, Buf, BufMut};

fn main() {
    // Build up a buffer, then freeze it into an immutable Bytes.
    let mut buf = BytesMut::with_capacity(64);
    buf.put_u8(0xFF);          // write a length/version byte
    buf.put(&b"hello"[..]);    // write a payload
    let frozen: Bytes = buf.freeze();

    // clone() and slice() are O(1): they share the same allocation.
    let header = frozen.slice(0..1);
    let body = frozen.slice(1..);
    println!("header: {header:?}, body: {body:?}");

    // The Buf trait lets you consume bytes like a cursor (advances position).
    let mut reader = frozen.clone();
    let version = reader.get_u8();       // reads 1 byte, advances
    println!("version: {version:#X}, {} bytes left", reader.remaining());
}
```

```text
header: b"\xff", body: b"hello"
version: 0xFF, 5 bytes left
```

`put_u8`/`get_u8` come from the `BufMut`/`Buf` traits, which give you endian-aware, cursor-style reads and writes, exactly what you want when parsing a binary protocol. Unless you are writing networking or codec code you may never need `bytes` directly, but you will see `Bytes` in the signatures of hyper, reqwest, and Tokio, so it pays to recognize it.

### dashmap — a concurrent HashMap

Here is a place where Rust forces work that Node never asks of you. In Node, a plain object *is* a concurrent map because there is only one thread; reads and writes can never interleave. In Rust, sharing a `HashMap` across threads does not compile: you would need to wrap it in `Arc<Mutex<HashMap>>`, which serializes *all* access through one lock, even reads of unrelated keys.

**dashmap** is a drop-in `HashMap` replacement that is safe to share and mutate from many threads at once. Internally it shards the map into many independently-locked segments, so two threads touching different keys rarely contend. Importantly, you can mutate it through a shared `&` reference (no outer `Mutex` needed), which is exactly what `par_iter` and threads require.

```rust
use dashmap::DashMap;
use std::sync::Arc;
use std::thread;

fn main() {
    // Share one map across threads. No Mutex<HashMap>, no &mut.
    let map: Arc<DashMap<&'static str, i32>> = Arc::new(DashMap::new());

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let map = Arc::clone(&map);
            thread::spawn(move || {
                for _ in 0..1000 {
                    *map.entry("hits").or_insert(0) += 1;
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // 4 threads × 1000 increments, no lost updates.
    println!("hits: {}", *map.get("hits").unwrap());
}
```

```text
hits: 4000
```

Each `entry().or_insert()` is atomic with respect to that key, so the increments do not race even though four threads hammer the same entry. This is the structure you would use for a shared cache, a connection pool's bookkeeping, or per-key counters in a server.

---

## Key Differences

| Concern | JavaScript / Node | Rust |
| --- | --- | --- |
| Iterator helpers | `Array.prototype` + lodash | std `Iterator` + **itertools** |
| `groupBy` | lodash gathers all matches | itertools `chunk_by` groups *consecutive*; sort first for lodash semantics |
| CPU parallelism | `worker_threads` (heavy, serialize across boundary) | **rayon**: `.iter()` → `.par_iter()`, compiler-checked |
| Lazy singleton | hand-rolled `if (!_x)` | **LazyLock** (std, ≥1.80) or once_cell `Lazy` |
| UUID | `crypto.randomUUID()` (v4 only) | **uuid** with `v4`/`v7` features |
| Ordered map | `Map` keeps insertion order | default `HashMap` is *unordered*; use **indexmap** |
| Map removal order | always preserved | `swap_remove` (fast, reorders) vs `shift_remove` (ordered) |
| Shared byte buffers | `Buffer.slice` shares memory | **bytes** `Bytes`, O(1) clone/slice, refcounted |
| Concurrent map | plain object (single thread) | **dashmap**, sharded locks, mutate via `&` |

The deepest conceptual gap is the last two rows. In Node, concurrency safety is a non-issue because of the single-threaded event loop, so a plain object doubles as a "thread-safe" map and you never think about it. Rust does not have that luxury — it is genuinely multi-threaded when you ask it to be — so it surfaces the choice in the type system. The upside is that data races are caught at compile time, not in production at 3 a.m. The trade is that "just share a map" becomes "pick `dashmap` (or `Arc<Mutex<HashMap>>`)": a deliberate, visible decision.

---

## Common Pitfalls

### Forgetting to bring the itertools trait into scope

itertools adds its methods through the `Itertools` extension trait. If you call `.join()` or `.counts()` without importing it, the method simply does not exist:

```rust
fn main() {
    // does not compile (error[E0599]: no method named `join`)
    let joined = ["a", "b", "c"].iter().join(", ");
    println!("{joined}");
}
```

The real compiler error is:

```text
error[E0599]: no method named `join` found for struct `std::slice::Iter` in the current scope
 --> src/main.rs:2:41
  |
2 |     let joined = ["a", "b", "c"].iter().join(", ");
  |                  ---------------        ^^^^ method not found in `std::slice::Iter<'_, &str>`
  |                  |
  |                  method `join` is available on `&[&str]`
```

The fix is one line at the top: `use itertools::Itertools;`. (This trait-import requirement is the same pattern as rayon's `use rayon::prelude::*;`: extension traits must be in scope for their methods to appear.)

### Deadlocking dashmap by holding two guards on the same key

`dashmap`'s `get` returns a read guard that holds a lock on that key's shard. If you then try to take a *write* guard for the same key while the read guard is still alive, the second call blocks forever, a classic self-deadlock:

```rust
use dashmap::DashMap;

fn main() {
    let map: DashMap<&str, i32> = DashMap::new();
    map.insert("a", 1);

    // This COMPILES but DEADLOCKS at runtime:
    let one = map.get("a").unwrap();          // read guard, still alive...
    let mut two = map.get_mut("a").unwrap();  // ...so this write guard blocks forever
    *two += *one;
    println!("never reached");
}
```

The borrow checker cannot catch this because both guards borrow the `DashMap` immutably (that is the whole point of mutating through `&`). The lock is a *runtime* construct. The fix is to not hold overlapping guards: read the value into a plain local and drop the guard before taking the write guard, or use `entry(...).and_modify(...)` / `alter(...)` which take a single guard internally. The same hazard exists with `iter()` while inserting.

### Expecting `HashMap` to preserve insertion order

Coming from JavaScript's `Map`, it is tempting to assume any map keeps order. Rust's `HashMap` does not — and its iteration order is randomized per process, so a test that happens to pass locally can fail in CI:

```rust
use std::collections::HashMap;

fn main() {
    let mut m = HashMap::new();
    m.insert("zulu", 1);
    m.insert("alpha", 2);
    m.insert("mike", 3);
    // Order is unspecified and may differ every run — do NOT rely on it.
    println!("{:?}", m.keys().collect::<Vec<_>>());
}
```

If you need ordering, use `IndexMap` (insertion order) or `BTreeMap` (sorted order, see [Sorted Collections](/07-collections/05-btreemap-btreeset/)). Never assert on `HashMap` iteration order in a test.

### Reaching for rayon on I/O-bound work

rayon parallelizes CPU work across a thread pool. If your loop is dominated by network or disk I/O (awaiting HTTP responses, reading files), rayon will tie up its threads blocking on syscalls and you will not get the concurrency you wanted. That is what `async`/Tokio is for. Rule of thumb: **rayon for crunching numbers, Tokio for waiting on the network.** Mixing them needs care: do not call blocking rayon work directly inside an async task without `spawn_blocking`.

### Choosing the wrong `IndexMap` removal

As shown above, `swap_remove` silently reorders the map. If you picked `IndexMap` specifically to keep order and then call `swap_remove`, you have quietly defeated the purpose. Default to `shift_remove` unless you have measured that the O(n) shift matters and you do not care about order at that point.

---

## Best Practices

- **Prefer std `LazyLock` over the once_cell crate in new code.** It is stable (since Rust 1.80), needs no dependency, and has the same ergonomics. Keep `once_cell` only when you must support older compilers or need its `unsync` variants.
- **Parallelize last, measure first.** rayon makes `.par_iter()` trivial, but the thread-pool coordination has overhead. For small inputs the sequential version is faster. Profile (see [Performance](/21-performance/)) before sprinkling `par_` everywhere.
- **Use UUID v7 for database keys, v4 for opaque tokens.** v7's time-ordering keeps B-tree index inserts near the "right" of the tree, reducing page splits; v4's full randomness is what you want when ordering would leak information.
- **Default to `shift_remove` on `IndexMap`** unless you have a measured reason to trade order for speed.
- **Reach for `dashmap` only when you genuinely share a map across threads.** Within a single thread, or behind one short-lived lock, a plain `HashMap` (or `Arc<Mutex<HashMap>>`) is simpler and the standard choice. dashmap shines under concurrent read/write contention.
- **Recognize `bytes` types rather than fight them.** When a crate hands you `Bytes`, clone it freely (it is cheap) and slice it instead of copying. Only build `BytesMut` yourself when authoring a codec or protocol.
- **Keep `use itertools::Itertools;` and `use rayon::prelude::*;` at the top.** Both crates work through extension traits that must be in scope; the imports are the price of admission.

---

## Real-World Example

A concurrent word-frequency counter, the kind of thing you would build for log analysis or search indexing. It uses **rayon** to process documents in parallel, **dashmap** to accumulate counts safely across threads, and **itertools** to produce a sorted top-N report. This single function combines four of the crates on this page.

```rust
// cargo add rayon dashmap itertools
use dashmap::DashMap;
use rayon::prelude::*;
use itertools::Itertools;
use std::sync::Arc;

/// Count word frequencies across many documents in parallel,
/// then return them sorted by descending count (ties broken alphabetically).
fn word_frequencies(docs: &[&str]) -> Vec<(String, usize)> {
    let counts: Arc<DashMap<String, usize>> = Arc::new(DashMap::new());

    // Each document is processed on the rayon thread pool; all threads
    // write into the same DashMap concurrently and safely.
    docs.par_iter().for_each(|doc| {
        for word in doc.split_whitespace() {
            let key = word.to_lowercase();
            *counts.entry(key).or_insert(0) += 1;
        }
    });

    // Reclaim the map (we are the only owner now) and sort with itertools.
    Arc::try_unwrap(counts)
        .expect("all worker threads have finished")
        .into_iter()
        .sorted_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)))
        .collect()
}

fn main() {
    let docs = [
        "the quick brown fox",
        "the lazy dog the fox",
        "quick quick brown the",
    ];

    for (word, count) in word_frequencies(&docs).iter().take(4) {
        println!("{word:>6}: {count}");
    }
}
```

Running it prints:

```text
   the: 4
 quick: 3
 brown: 2
   fox: 2
```

The interesting part is what is *not* there: no `Worker` threads, no `postMessage`, no lock around the map, and no possibility of a lost update — yet four crates cooperate to do genuinely parallel work. `par_iter()` spreads the documents across cores, `DashMap`'s per-key atomic `entry().or_insert()` keeps the counts correct under contention, `Arc::try_unwrap` recovers sole ownership once the parallel section is done, and itertools' `sorted_by` gives a stable, descending report.

---

## Further Reading

- [itertools documentation](https://docs.rs/itertools): the full list of iterator adapters
- [rayon documentation](https://docs.rs/rayon) and the [rayon FAQ](https://github.com/rayon-rs/rayon/blob/main/FAQ.md): parallel iterators and the thread pool
- [`std::sync::LazyLock`](https://doc.rust-lang.org/std/sync/struct.LazyLock.html) and the [once_cell crate](https://docs.rs/once_cell): lazy initialization
- [uuid documentation](https://docs.rs/uuid): versions, features, and parsing
- [indexmap documentation](https://docs.rs/indexmap): the order-preserving map and set
- [bytes documentation](https://docs.rs/bytes): `Bytes`, `BytesMut`, and the `Buf`/`BufMut` traits
- [dashmap documentation](https://docs.rs/dashmap): the concurrent map and its locking model
- Related pages in this section: [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/) (the overview), [Async Runtimes](/23-ecosystem/02-async-runtimes/) (rayon vs Tokio), and [Parsing](/23-ecosystem/09-parsing/) (`bytes` in codecs).
- Background from earlier sections: [HashMaps](/07-collections/03-hashmaps/) (why `HashMap` is unordered), [Smart Pointers](/10-smart-pointers/) (`Arc` and shared ownership), and [Async](/11-async/) (concurrency vs parallelism).
- Next: [Tooling](/24-tooling/) — the tooling that ties these crates together.

---

## Exercises

### Exercise 1: Totals by customer with itertools

**Difficulty:** Beginner

**Objective:** Use itertools' `sorted_by` + `chunk_by` to reproduce lodash `groupBy` semantics and aggregate within each group.

**Instructions:** Given a slice of `Order { customer: String, amount: u32 }`, write `totals_by_customer` that returns a `Vec<(String, u32)>` of each customer's summed amount. Remember that `chunk_by` only groups *consecutive* keys, so you must sort by customer first.

<details>
<summary>Solution</summary>

```rust
// cargo add itertools
use itertools::Itertools;

#[derive(Debug)]
struct Order {
    customer: String,
    amount: u32,
}

fn totals_by_customer(orders: &[Order]) -> Vec<(String, u32)> {
    orders
        .iter()
        .sorted_by(|a, b| a.customer.cmp(&b.customer)) // make equal keys adjacent
        .chunk_by(|o| o.customer.clone())
        .into_iter()
        .map(|(customer, group)| (customer, group.map(|o| o.amount).sum()))
        .collect()
}

fn main() {
    let orders = vec![
        Order { customer: "alice".into(), amount: 30 },
        Order { customer: "bob".into(),   amount: 10 },
        Order { customer: "alice".into(), amount: 12 },
        Order { customer: "bob".into(),   amount: 5 },
        Order { customer: "carol".into(), amount: 100 },
    ];
    for (c, total) in totals_by_customer(&orders) {
        println!("{c}: {total}");
    }
}
```

Output:

```text
alice: 42
bob: 15
carol: 100
```

`sorted_by` clusters each customer's orders together so `chunk_by` can group them; without the sort, the two `alice` orders (separated by a `bob`) would land in two different chunks.

</details>

### Exercise 2: Parallel id assignment

**Difficulty:** Intermediate

**Objective:** Use rayon and uuid together, and confirm the version of the generated ids.

**Instructions:** Write `assign_ids(names: &[&str]) -> Vec<(Uuid, String)>` that, **in parallel**, pairs each name with a fresh time-ordered UUID (v7) and uppercases the name. In `main`, verify that every id reports version number 7.

<details>
<summary>Solution</summary>

```rust
// cargo add rayon
// cargo add uuid --features v7
use rayon::prelude::*;
use uuid::Uuid;

fn assign_ids(names: &[&str]) -> Vec<(Uuid, String)> {
    names
        .par_iter()
        .map(|name| (Uuid::now_v7(), name.to_uppercase()))
        .collect()
}

fn main() {
    let names = ["ada", "linus", "grace", "alan"];
    let assigned = assign_ids(&names);

    println!("assigned {} ids", assigned.len());
    let all_v7 = assigned.iter().all(|(id, _)| id.get_version_num() == 7);
    println!("all v7: {all_v7}");
    for (_, name) in &assigned {
        println!("  {name}");
    }
}
```

Output:

```text
assigned 4 ids
all v7: true
  ADA
  LINUS
  GRACE
  ALAN
```

`par_iter().map(...).collect()` runs the closures across the thread pool while `collect` reassembles the results **in input order**, so the names line up with their positions.

</details>

### Exercise 3: A thread-safe LRU-ish hit counter

**Difficulty:** Advanced

**Objective:** Combine `dashmap`, `LazyLock`, and `std::thread` into a shared, concurrent counter behind a global.

**Instructions:** Declare a global `static HITS: LazyLock<DashMap<String, u64>>`. Write a function `record(path: &str)` that increments the counter for that path. Spawn several threads that each call `record` many times on a shared set of paths, then print the totals sorted by path. Confirm there are no lost updates.

<details>
<summary>Solution</summary>

```rust
// cargo add dashmap
use dashmap::DashMap;
use std::sync::LazyLock;
use std::thread;

// One global, lazily built, safely shared across all threads.
static HITS: LazyLock<DashMap<String, u64>> = LazyLock::new(DashMap::new);

fn record(path: &str) {
    // entry().or_insert() is atomic per key — no lost updates.
    *HITS.entry(path.to_string()).or_insert(0) += 1;
}

fn main() {
    let paths = ["/", "/about", "/contact"];

    let handles: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn(move || {
                for _ in 0..1000 {
                    for p in paths {
                        record(p);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // 8 threads × 1000 iterations = 8000 hits per path.
    let mut totals: Vec<(String, u64)> =
        HITS.iter().map(|e| (e.key().clone(), *e.value())).collect();
    totals.sort();
    for (path, count) in totals {
        println!("{path}: {count}");
    }
}
```

Output:

```text
/: 8000
/about: 8000
/contact: 8000
```

Because `HITS` is a `LazyLock<DashMap>`, every thread sees the same map (no `Arc` needed — a `static` lives for the whole program), and `entry().or_insert()` makes each increment atomic, so all 8000 hits per path are counted with no races. Swapping the `DashMap` for a plain `HashMap` here would not even compile, since a `static HashMap` cannot be mutated through a shared reference.

</details>
