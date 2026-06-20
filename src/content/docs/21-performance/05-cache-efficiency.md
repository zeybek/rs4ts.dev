---
title: "Cache-Friendly Code: Data-Oriented Design"
description: "Data-oriented design in Rust: why contiguous Vecs and Struct-of-Arrays beat pointer-chasing, and when layout matters more than which algorithm you run."
---

How you *lay out* data in memory often matters more than which algorithm you run over it. This topic covers data-oriented design, the Struct-of-Arrays (SoA) versus Array-of-Structs (AoS) trade-off, and why a flat, contiguous `Vec` beats pointer-chasing data structures on modern hardware.

---

## Quick Overview

A modern CPU core can do billions of arithmetic operations per second, but a single fetch from main memory costs roughly a couple hundred cycles. To hide that gap, the CPU has small, fast caches (L1/L2/L3) and loads memory in fixed-size **cache lines** (64 bytes on x86-64 and Apple Silicon). Code that reads memory in straight, predictable, contiguous runs keeps those lines full of useful data and lets the hardware prefetcher stay ahead of you; code that hops between scattered heap allocations stalls waiting on memory.

For a TypeScript/JavaScript developer this is mostly invisible: the V8 engine boxes objects, manages a garbage-collected heap, and hides layout behind hidden classes. In Rust *you* decide the layout, so you can deliberately make data **cache-friendly**: store the fields you iterate over together and contiguously, and avoid chasing pointers.

> **Note:** This is the *layout* side of performance. Its sibling [Memory Layout](/21-performance/04-memory-layout/) covers the size and alignment of a *single* struct (field ordering, `#[repr]`, niche optimization). This file is about the layout of *collections* of data.

---

## TypeScript/JavaScript Example

In JavaScript, an array of objects is an **Array of Structs (AoS)**, but with an extra level of indirection you cannot remove. Each object is a separately allocated, garbage-collected heap cell, and the array holds *references* (pointers) to them.

```typescript
// A particle simulation, the way you'd naturally write it in TypeScript.
interface Particle {
  x: number;
  y: number;
  z: number;
  vx: number;
  vy: number;
  vz: number;
  hp: number;
  name: string;
}

const particles: Particle[] = [];
for (let i = 0; i < 1_000_000; i++) {
  particles.push({ x: i, y: 0, z: 0, vx: 1, vy: 0.5, vz: 0.25, hp: 100, name: "p" });
}

// A common query: average the x coordinate. We touch ONE field of eight,
// but each `p` is a separate heap object reached through a pointer.
function averageX(ps: Particle[]): number {
  let sum = 0;
  for (const p of ps) sum += p.x;
  return sum / ps.length;
}

console.log(particles[0]); // { x: 0, y: 0, z: 0, vx: 1, ... name: 'p' }
console.log(averageX(particles));
```

When you want true contiguity in JavaScript, you reach for **typed arrays**, a manual Struct-of-Arrays:

```typescript
// Struct-of-Arrays via typed arrays: each "column" is a flat, contiguous buffer.
const N = 1_000_000;
const xs = new Float64Array(N);
const ys = new Float64Array(N);
const vxs = new Float64Array(N);
// ...one TypedArray per field...

for (let i = 0; i < N; i++) {
  xs[i] = i;
  vxs[i] = 1;
}

// Now `averageX` reads one tightly-packed buffer — no per-element pointer hop.
function averageX(xs: Float64Array): number {
  let sum = 0;
  for (let i = 0; i < xs.length; i++) sum += xs[i];
  return sum / xs.length;
}
```

> A `Float64Array` is genuinely contiguous: `new Float64Array(2).byteLength` is `16`, exactly two 8-byte slots, no boxing. This is the closest JavaScript gets to manual memory layout, and it is exactly the SoA idea we will make idiomatic in Rust. The catch in JS: typed arrays only hold numbers, so anything richer (a `name` string) has to live in a parallel plain array, and you lose the ergonomics of a real object.

---

## Rust Equivalent

In Rust, `Vec<Particle>` is **already contiguous**: the structs are stored inline, back-to-back, with no per-element pointer. That is a big head start over JavaScript's array-of-references. But it is still *Array of Structs*: to read one field you stride over every other field too. The data-oriented alternative is **Struct of Arrays**, where each field is its own `Vec`.

```rust playground
// Array of Structs (AoS): the natural, OOP-flavored layout.
#[derive(Clone)]
struct Particle {
    x: f32,
    y: f32,
    z: f32,
    vx: f32,
    vy: f32,
    vz: f32,
    hp: i32,
    name: String,
}

// Struct of Arrays (SoA): one contiguous Vec per field ("column").
#[derive(Default)]
struct Particles {
    x: Vec<f32>,
    y: Vec<f32>,
    z: Vec<f32>,
    vx: Vec<f32>,
    vy: Vec<f32>,
    vz: Vec<f32>,
    hp: Vec<i32>,
    name: Vec<String>,
}

impl Particles {
    fn push(&mut self, x: f32, y: f32, vx: f32, vy: f32) {
        self.x.push(x);
        self.y.push(y);
        self.z.push(0.0);
        self.vx.push(vx);
        self.vy.push(vy);
        self.vz.push(0.0);
        self.hp.push(100);
        self.name.push(String::from("p"));
    }

    fn len(&self) -> usize {
        self.x.len()
    }

    // Reading the average x now streams one tight Vec<f32> — every byte
    // pulled into cache is a value we actually use.
    fn average_x(&self) -> f32 {
        self.x.iter().sum::<f32>() / self.len() as f32
    }
}

fn main() {
    let mut ps = Particles::default();
    for i in 0..1_000_000 {
        ps.push(i as f32, 0.0, 1.0, 0.5);
    }
    println!("count: {}", ps.len());
    println!("average x: {}", ps.average_x());
}
```

The two layouts hold the same data; they differ only in *where the bytes sit*. For a query that touches one field, SoA loads only that field's bytes, while AoS drags the whole struct (including the `name` heap pointer and the unused stats) through cache.

---

## Detailed Explanation

### Why a cache line is the unit that matters

The CPU never loads one `f32`. It loads the whole 64-byte cache line containing it. So the real question for any loop is: *of the 64 bytes I just paid to fetch, how many will I actually use?*

Consider summing the `x` field of a "fat" entity. Here is the struct from the benchmark below; `std::mem::size_of` reports its real size:

```rust playground
#[derive(Clone)]
struct Entity {
    x: f32, y: f32, z: f32,
    vx: f32, vy: f32, vz: f32,
    hp: i32, mana: i32, level: i32, xp: u64,
    name: String,
    inventory: [u32; 32], // 128 bytes of cold data
    flags: u64,
    cooldowns: [f32; 8],  // 32 bytes
}

fn main() {
    println!("size_of::<Entity>() = {} bytes", std::mem::size_of::<Entity>());
}
```

Real output:

```text
size_of::<Entity>() = 240 bytes
```

- **AoS (`Vec<Entity>`):** each element is 240 bytes. To read the 4-byte `x`, the CPU loads the cache line(s) holding that element, and the prefetcher streams in the neighbors, but those neighbors are mostly `inventory`, `cooldowns`, and a `String` pointer you never touch in this loop. You use about 4 of every ~240 bytes you bring in: under 2% of the bandwidth is doing useful work.
- **SoA (`Vec<f32>` for `x`):** the `x` values are packed 16 per 64-byte line. Every loaded byte is a value you sum, and the compiler can autovectorize the loop into SIMD adds because the data is a flat `f32` stream.

### The benchmark

Measured with [criterion](/21-performance/02-benchmarking/) (which handles warm-up and statistics so the numbers are trustworthy) on the `Entity` above, summing only the `x` field across 1,000,000 elements:

```text
sum_x/aos               time:   [4.1552 ms 4.2436 ms 4.3890 ms]
sum_x/soa               time:   [1.0204 ms 1.1019 ms 1.2093 ms]
```

That is roughly a **4x** speedup for SoA on this machine, purely from layout, with identical arithmetic. The exact ratio is hardware- and load-dependent (re-runs on the same laptop landed between about 3x and 5x), so reproduce it on your own target rather than quoting a fixed figure. The *direction* is the reliable part: the wider the struct relative to the field you touch, the bigger the SoA win.

> **Warning:** Do not take a single `Instant::now()` micro-measurement as gospel — first-touch page faults, allocator warmth, and background load can swing a naive timing by 5x or more. Always confirm a layout change with a real benchmark harness. See [Benchmarking with Criterion](/21-performance/02-benchmarking/) and [When to Optimize](/21-performance/10-when-to-optimize/).

### When AoS is actually fine (honesty check)

SoA is **not** a free win. When a loop touches *most* of a struct's fields, AoS keeps that struct's bytes together on one cache line, so reading `vx` and writing `x` for the same particle is already local. In that situation a benchmark of a full position-integration loop (touching `x, y, z, vx, vy, vz`) showed AoS *tying or beating* SoA, because SoA then juggles several separate memory streams and bounds checks. SoA pays off specifically when:

1. you frequently process a **subset** of fields ("give me every `x`"), and/or
2. the struct is **large** with cold fields you rarely read, and/or
3. you want **SIMD** — a flat `Vec<f32>` autovectorizes; an `Vec<Struct>` usually does not.

This honest "it depends" is the whole point of data-oriented design: organize data around *how it is accessed*, not around real-world taxonomy.

### Pointer-chasing is the real villain

The opposite of contiguous data is a structure where each element lives in its own heap allocation and you reach the next one by dereferencing a pointer: a linked list, a tree of `Box`es, a graph of `Rc`s. Each hop is a potential cache miss the prefetcher cannot predict, because the address of the next node is only known *after* you have loaded the current one (a data dependency).

```rust
use std::time::Instant;
use std::hint::black_box;

const N: usize = 5_000_000;

struct Node {
    value: u64,
    next: Option<Box<Node>>,
}

fn main() {
    // Contiguous: a flat Vec.
    let contiguous: Vec<u64> = (0..N as u64).collect();

    // Pointer-chasing: a singly linked list of separate heap allocations.
    let mut head: Option<Box<Node>> = None;
    for v in (0..N as u64).rev() {
        head = Some(Box::new(Node { value: v, next: head }));
    }

    let _: u64 = contiguous.iter().sum(); // warm up

    let t = Instant::now();
    let mut sum1 = 0u64;
    for &v in &contiguous {
        sum1 = sum1.wrapping_add(v);
    }
    let vec_time = t.elapsed();

    let t = Instant::now();
    let mut sum2 = 0u64;
    let mut cur = head.as_deref();
    while let Some(node) = cur {
        sum2 = sum2.wrapping_add(node.value);
        cur = node.next.as_deref();
    }
    let list_time = t.elapsed();

    println!("Vec  sum: {:?}  (= {})", vec_time, black_box(sum1));
    println!("List sum: {:?}  (= {})", list_time, black_box(sum2));
    println!("Vec is {:.1}x faster", list_time.as_secs_f64() / vec_time.as_secs_f64());
}
```

Representative real output (two runs, release build):

```text
Vec  sum: 3.611167ms  (= 12499997500000)
List sum: 14.188292ms  (= 12499997500000)
Vec is 3.9x faster
```

```text
Vec  sum: 3.892125ms  (= 12499997500000)
List sum: 22.985875ms  (= 12499997500000)
Vec is 5.9x faster
```

Same data, same sum, same `O(n)` algorithm. The `Vec` is several times faster and far more consistent, because the linked list spends most of its time stalled on cache misses. This is why Rust's standard `std::collections::LinkedList` carries a documentation note steering you to `Vec` or `VecDeque` for almost everything. The cure for pointer-chasing is to put the data in a contiguous container and use indices (`usize`) instead of pointers when you need to refer between elements.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| `obj[]` of records | Array of *references* to GC'd heap objects (double indirection) | `Vec<Struct>` stores structs *inline*, contiguously (single indirection) |
| Contiguous numeric data | `Float64Array` / `Int32Array` (numbers only) | Any `Vec<T>` of a `Copy`/POD type; works for structs too |
| Choosing memory layout | Mostly out of your hands; V8 hidden classes | Fully under your control (AoS vs SoA, `Box`, `#[repr]`) |
| SoA ergonomics | Parallel typed arrays, manual index bookkeeping | A `struct` of `Vec`s with methods; the type system tracks length |
| Pointer-chasing structures | Idiomatic (linked lists, object graphs) and GC-managed | Possible (`Box`, `Rc`) but discouraged on hot paths; prefer indices into a `Vec` |
| SIMD / autovectorization | JIT may vectorize typed-array loops opportunistically | A flat `Vec<f32>` loop reliably autovectorizes at `--release` |

The deeper difference: in JavaScript the engine *owns* your layout and optimizes heuristically; in Rust the layout is part of your design, chosen at compile time, with zero runtime metadata. That is what makes "data-oriented design" a Rust idiom rather than a fight against the runtime.

---

## Common Pitfalls

### Pitfall 1: Borrow-checker conflicts when updating SoA columns through `self`

A natural-looking column update fails to compile, because iterating one field mutably while calling a method that borrows `self` again is a double borrow:

```rust
struct Particles {
    x: Vec<f32>,
    vx: Vec<f32>,
}

impl Particles {
    fn vx_at(&self, i: usize) -> f32 { self.vx[i] }

    fn integrate(&mut self) {
        // does not compile (error[E0502]): self is mutably borrowed by x.iter_mut()
        for (i, xi) in self.x.iter_mut().enumerate() {
            *xi += self.vx_at(i);
        }
    }
}

fn main() {}
```

The real compiler error:

```text
error[E0502]: cannot borrow `*self` as immutable because it is also borrowed as mutable
  --> src/main.rs:12:20
   |
11 |         for (i, xi) in self.x.iter_mut().enumerate() {
   |                        -----------------------------
   |                        |
   |                        mutable borrow occurs here
   |                        mutable borrow later used here
12 |             *xi += self.vx_at(i);
   |                    ^^^^ immutable borrow occurs here
```

**Fix:** iterate the *fields* directly with `zip`, not through a helper that re-borrows `self`. The borrow checker can see that `self.x` and `self.vx` are disjoint fields and allows one mutable and one immutable borrow simultaneously:

```rust playground
struct Particles {
    x: Vec<f32>,
    vx: Vec<f32>,
}

impl Particles {
    fn integrate(&mut self) {
        // x is borrowed mutably, vx immutably — disjoint fields, no conflict.
        for (xi, &vxi) in self.x.iter_mut().zip(self.vx.iter()) {
            *xi += vxi;
        }
    }
}

fn main() {
    let mut p = Particles { x: vec![0.0, 10.0], vx: vec![1.0, 2.0] };
    p.integrate();
    println!("{:?}", p.x); // [1.0, 12.0]
}
```

Output: `[1.0, 12.0]`. (See [Ownership](/05-ownership/) for why disjoint-field borrows are allowed.)

### Pitfall 2: Columns drifting out of sync

SoA's biggest correctness hazard is silent: nothing forces `x.len() == vx.len()`. If you `push` to some columns but not others, your invariant breaks with no error. Encapsulate every insertion behind a single `push`/`spawn` method that updates **all** columns together, never expose the `Vec`s as `pub`, and index with the same `i` everywhere. (Crates like `soa_derive` generate this boilerplate for you.)

### Pitfall 3: Reaching for SoA before measuring

SoA complicates your code and only helps specific access patterns (Pitfall from the "honesty check" above). Converting a struct to SoA when your hot loop reads most fields can make things *slower* and harder to read. Profile first ([Profiling Rust Applications](/21-performance/00-profiling/)), confirm the loop is memory-bound, then restructure. Premature data-oriented design is still premature optimization; see [When to Optimize](/21-performance/10-when-to-optimize/).

### Pitfall 4: Assuming `Vec<Box<T>>` is contiguous

`Vec<Box<Widget>>` stores the *pointers* contiguously, but each `Widget` is a separate allocation scattered across the heap. You reintroduced pointer-chasing. Prefer `Vec<Widget>` (values inline). Reach for `Box` inside a `Vec` only when you genuinely need stable addresses, trait objects (`Vec<Box<dyn Trait>>`), or recursive types.

---

## Best Practices

- **Default to `Vec<T>` with `T` stored by value.** Contiguous-by-value is the cache-friendly baseline and usually the right answer.
- **Use indices, not pointers, to link elements.** Replace `next: Option<Box<Node>>` with `next: Option<u32>` indexing into a `Vec<Node>` (an "arena" or "slot map"). You keep relationships without the cache misses, and you sidestep `Rc`/lifetime gymnastics. See [Smart Pointers](/10-smart-pointers/) and [Common Patterns](/22-common-patterns/).
- **Hot/cold split fat structs.** Keep frequently-touched fields inline and box the rarely-used remainder behind one pointer:

  ```rust playground
  struct MonsterInline {        // everything inline
      x: f32, y: f32, hp: i32,
      name: String, lore: String, loot_table: Vec<u32>,
  }

  struct MonsterSplit {         // hot fields inline, cold data behind a Box
      x: f32, y: f32, hp: i32,
      cold: Box<MonsterColdData>,
  }
  struct MonsterColdData { name: String, lore: String, loot_table: Vec<u32> }

  fn main() {
      println!("inline = {} bytes", std::mem::size_of::<MonsterInline>());
      println!("split  = {} bytes", std::mem::size_of::<MonsterSplit>());
  }
  ```

  Real output:

  ```text
  inline = 88 bytes
  split  = 24 bytes
  ```

  A `Vec<MonsterSplit>` packs ~2.6x more entities per cache line for the hot loop; the cold data is fetched only when you actually need it.
- **`Vec::with_capacity` when you know the count.** Pre-sizing every column avoids reallocations mid-build and keeps each column in one allocation. (Capacity management is covered in depth in [Optimization Techniques](/21-performance/03-optimization/).)
- **Iterate with `iter()`/`zip()`, not indexed `[i]`, on the hottest loops.** Iterators elide bounds checks and autovectorize cleanly; see [Zero-Cost Abstractions](/21-performance/06-zero-cost/) for the generated-assembly evidence.
- **Measure, don't guess.** Confirm any layout change with [criterion](/21-performance/02-benchmarking/) and a [profiler](/21-performance/00-profiling/).

---

## Real-World Example

A data-oriented particle system in SoA layout, the shape you would find in a game engine or simulation. Each per-frame update is a set of tight, contiguous passes, and a read-only query touches a single column.

```rust playground
/// Data-oriented particle system, Struct-of-Arrays layout.
/// Each "column" is a contiguous Vec, so the per-step update streams
/// linearly through memory and the autovectorizer can use SIMD.
#[derive(Default)]
struct ParticleSystem {
    px: Vec<f32>,
    py: Vec<f32>,
    vx: Vec<f32>,
    vy: Vec<f32>,
}

impl ParticleSystem {
    fn with_capacity(n: usize) -> Self {
        ParticleSystem {
            px: Vec::with_capacity(n),
            py: Vec::with_capacity(n),
            vx: Vec::with_capacity(n),
            vy: Vec::with_capacity(n),
        }
    }

    fn spawn(&mut self, px: f32, py: f32, vx: f32, vy: f32) {
        self.px.push(px);
        self.py.push(py);
        self.vx.push(vx);
        self.vy.push(vy);
    }

    fn len(&self) -> usize {
        self.px.len()
    }

    /// One simulation step: apply gravity, then integrate position.
    fn step(&mut self, dt: f32, gravity: f32) {
        // Pass 1: gravity touches only vy.
        for vy in self.vy.iter_mut() {
            *vy += gravity * dt;
        }
        // Pass 2: integrate x. Disjoint columns -> the borrow checker is happy.
        for (x, &vx) in self.px.iter_mut().zip(self.vx.iter()) {
            *x += vx * dt;
        }
        // Pass 3: integrate y.
        for (y, &vy) in self.py.iter_mut().zip(self.vy.iter()) {
            *y += vy * dt;
        }
    }

    /// A read-only query that touches a single column — the SoA payoff.
    fn average_height(&self) -> f32 {
        if self.py.is_empty() {
            return 0.0;
        }
        self.py.iter().sum::<f32>() / self.py.len() as f32
    }
}

fn main() {
    let mut sim = ParticleSystem::with_capacity(1000);
    for i in 0..1000 {
        sim.spawn(i as f32, 100.0, 1.0, 0.0);
    }

    // Simulate one second at 60 FPS.
    for _ in 0..60 {
        sim.step(1.0 / 60.0, -9.81);
    }

    println!("particles: {}", sim.len());
    println!("average height after 1s: {:.3}", sim.average_height());
    println!("particle 0 position: ({:.3}, {:.3})", sim.px[0], sim.py[0]);
}
```

Real output:

```text
particles: 1000
average height after 1s: 95.014
particle 0 position: (1.000, 95.013)
```

Each pass is a straight walk through one or two contiguous buffers — the ideal access pattern for the prefetcher and the autovectorizer. This is the core idea behind ECS (Entity-Component-System) game architectures and columnar analytics engines: store data in columns, process it in bulk.

---

## Further Reading

- [The Rust Performance Book — Type Sizes & Data Structures](https://nnethercote.github.io/perf-book/type-sizes.html) — practical layout advice from the Rust performance experts.
- [`std::collections` documentation](https://doc.rust-lang.org/std/collections/index.html) — the official guidance on choosing `Vec`/`VecDeque` over `LinkedList`, and why.
- [`Vec` documentation](https://doc.rust-lang.org/std/vec/struct.Vec.html) — the contiguous growable array at the heart of cache-friendly Rust.
- [Mike Acton, "Data-Oriented Design and C++"](https://www.youtube.com/watch?v=rX0ItVEVjHc) — the classic talk that popularized DOD; the principles are language-agnostic.
- Sibling topics: [Memory Layout](/21-performance/04-memory-layout/) (single-struct size/align), [Optimization Techniques](/21-performance/03-optimization/) (clones, allocations, capacity), [Zero-Cost Abstractions](/21-performance/06-zero-cost/) (iterators compile to tight loops), [Benchmarking with Criterion](/21-performance/02-benchmarking/) (criterion), [Profiling Rust Applications](/21-performance/00-profiling/) (finding the hot loop), [When to Optimize](/21-performance/10-when-to-optimize/) (measure first), [Performance](/21-performance/09-comparison/) (vs Node.js).
- Foundations: [Ownership](/05-ownership/), [Collections](/07-collections/), [Smart Pointers](/10-smart-pointers/), and arena/index patterns in [Common Patterns](/22-common-patterns/).

---

## Exercises

### Exercise 1: Convert an Array of Structs to a Struct of Arrays

**Difficulty:** Beginner

**Objective:** Practice the mechanical AoS→SoA transformation and see why a single-column query becomes cheap.

**Instructions:** Given the `Order` struct below, write an `Orders` SoA type with one `Vec` per field and a `from_aos(orders: Vec<Order>) -> Orders` constructor. Add a `revenue_cents(&self) -> u64` method that sums the `total_cents` column. Verify it on three orders.

```rust
#[derive(Clone)]
struct Order {
    id: u64,
    customer: String,
    total_cents: u64,
    shipped: bool,
}

// TODO: define `struct Orders { ... }`, `impl Orders { fn from_aos(...) ...; fn revenue_cents(...) ... }`
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Clone)]
struct Order {
    id: u64,
    customer: String,
    total_cents: u64,
    shipped: bool,
}

struct Orders {
    id: Vec<u64>,
    customer: Vec<String>,
    total_cents: Vec<u64>,
    shipped: Vec<bool>,
}

impl Orders {
    fn from_aos(orders: Vec<Order>) -> Self {
        let mut out = Orders {
            id: Vec::with_capacity(orders.len()),
            customer: Vec::with_capacity(orders.len()),
            total_cents: Vec::with_capacity(orders.len()),
            shipped: Vec::with_capacity(orders.len()),
        };
        for o in orders {
            out.id.push(o.id);
            out.customer.push(o.customer);
            out.total_cents.push(o.total_cents);
            out.shipped.push(o.shipped);
        }
        out
    }

    // Touches exactly one contiguous column.
    fn revenue_cents(&self) -> u64 {
        self.total_cents.iter().sum()
    }
}

fn main() {
    let aos = vec![
        Order { id: 1, customer: "Ada".into(), total_cents: 1500, shipped: true },
        Order { id: 2, customer: "Bob".into(), total_cents: 2500, shipped: false },
        Order { id: 3, customer: "Cy".into(),  total_cents: 1000, shipped: true },
    ];
    let orders = Orders::from_aos(aos);
    println!("revenue: {} cents", orders.revenue_cents()); // 5000
}
```

Output: `revenue: 5000 cents`.

</details>

### Exercise 2: A single-column predicate query

**Difficulty:** Intermediate

**Objective:** See how a filtered count over one column avoids loading the rest of each record.

**Instructions:** Using the `Orders` SoA type from Exercise 1, write a free function `count_shipped(orders: &Orders) -> usize` that returns how many orders are shipped, reading only the `shipped` column. Explain in a comment why this is cache-friendlier than iterating an `Vec<Order>` and checking `o.shipped`.

<details>
<summary>Solution</summary>

```rust playground
struct Orders {
    id: Vec<u64>,
    customer: Vec<String>,
    total_cents: Vec<u64>,
    shipped: Vec<bool>,
}

// Reads only the `shipped` column: a contiguous Vec<bool> (1 byte each).
// An AoS version would load each whole Order — including the `customer`
// String pointer and the 8-byte id/total — just to check one bool, wasting
// most of every cache line it fetched.
fn count_shipped(orders: &Orders) -> usize {
    orders.shipped.iter().filter(|&&s| s).count()
}

fn main() {
    let orders = Orders {
        id: vec![1, 2, 3],
        customer: vec!["Ada".into(), "Bob".into(), "Cy".into()],
        total_cents: vec![1500, 2500, 1000],
        shipped: vec![true, false, true],
    };
    println!("shipped: {}", count_shipped(&orders)); // 2
}
```

Output: `shipped: 2`.

</details>

### Exercise 3: A synchronized multi-column update without tripping the borrow checker

**Difficulty:** Advanced

**Objective:** Update several columns together in one pass, using `zip` so the borrow checker permits the simultaneous borrows.

**Instructions:** Given `struct Bodies { x: Vec<f64>, y: Vec<f64>, mass: Vec<f64> }`, write `scale_positions_by_mass(&mut self)` that multiplies each body's `x` and `y` by its `mass`, in a single pass. Chain `zip` so both `x` and `y` are borrowed mutably and `mass` immutably at once. (Hint: `self.x.iter_mut().zip(self.y.iter_mut()).zip(self.mass.iter())` yields `((&mut x, &mut y), &mass)`.)

<details>
<summary>Solution</summary>

```rust playground
struct Bodies {
    x: Vec<f64>,
    y: Vec<f64>,
    mass: Vec<f64>,
}

impl Bodies {
    fn scale_positions_by_mass(&mut self) {
        // x and y are disjoint fields, so both can be borrowed mutably while
        // mass is borrowed immutably — all in one contiguous pass.
        for ((x, y), &m) in self
            .x
            .iter_mut()
            .zip(self.y.iter_mut())
            .zip(self.mass.iter())
        {
            *x *= m;
            *y *= m;
        }
    }
}

fn main() {
    let mut bodies = Bodies {
        x: vec![1.0, 2.0],
        y: vec![3.0, 4.0],
        mass: vec![10.0, 0.5],
    };
    bodies.scale_positions_by_mass();
    println!("bodies.x = {:?}", bodies.x); // [10.0, 1.0]
    println!("bodies.y = {:?}", bodies.y); // [30.0, 2.0]
}
```

Output:

```text
bodies.x = [10.0, 1.0]
bodies.y = [30.0, 2.0]
```

</details>
