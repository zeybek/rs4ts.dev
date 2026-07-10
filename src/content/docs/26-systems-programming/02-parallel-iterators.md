---
title: "Parallel Iterators with Rayon"
description: "Swap iter() for par_iter() and Rust's Rayon fans the work across every core, where JavaScript and TypeScript array methods stay stuck on a single thread."
---

In JavaScript and TypeScript, `array.map()` and `array.filter()` are always single-threaded. Even on a machine with eight cores, a plain `Array.prototype.filter` runs on exactly one of them. Rust's [Rayon](https://docs.rs/rayon) crate lets you turn a sequential iterator into a parallel one by changing a single method call (`.iter()` becomes `.par_iter()`), and the work fans out across every CPU core, with the borrow checker guaranteeing there are no data races.

---

## Quick Overview

A **parallel iterator** processes the elements of a collection across multiple threads at once, then combines the results. Rayon provides drop-in parallel versions of the iterator adapters you already know (`map`, `filter`, `sum`, `collect`, `reduce`), so converting a sequential pipeline to a parallel one is usually a one-word change. The catch — and the focus of this chapter — is that parallelism only pays off when there is enough independent, CPU-bound work to overcome the cost of coordinating threads.

> **Note:** The recorded verification run used the repository's [pinned Rust toolchain](/00-introduction/05-version-policy/), the 2024 edition, and rayon 1.12.0. Run `cargo add rayon` in a fresh project to resolve a compatible release, then check its changelog if the API differs.

---

## TypeScript/JavaScript Example

In Node.js (here, v22), the built-in array methods are synchronous and single-threaded. Counting primes below two million keeps one core busy while the other seven sit idle:

```typescript
// prime-count.ts — single-threaded, no matter how many cores you have
function isPrime(n: number): boolean {
  if (n < 2) return false;
  for (let d = 2; d * d <= n; d++) {
    if (n % d === 0) return false;
  }
  return true;
}

const numbers: number[] = Array.from({ length: 2_000_000 - 2 }, (_, i) => i + 2);

const start = performance.now();
const count = numbers.filter(isPrime).length;
const ms = (performance.now() - start).toFixed(1);

console.log(`primes: ${count} in ${ms} ms (single-threaded)`);
```

Running it with `node --experimental-strip-types prime-count.ts` on an 8-core machine:

```text
primes: 148933 in 1254.7 ms (single-threaded)
```

To actually use the other cores in Node you reach for [`worker_threads`](https://nodejs.org/api/worker_threads.html): spawn workers, split the range yourself, send each chunk over a `MessageChannel`, run the computation in each worker, and merge the partial results back in the main thread. That is a lot of boilerplate (manual chunking, message serialization, lifecycle management) for what is conceptually still "filter this array." There is no `array.parallelFilter()`.

> **Note:** A `Worker` runs real OS-thread-backed code, but data sent across the channel is **copied** (structured clone) unless you use a `SharedArrayBuffer`. JavaScript has no shared-memory data parallelism for ordinary objects, which is exactly the boilerplate Rayon removes.

---

## Rust Equivalent

In Rust, the parallel version is the sequential version with `iter()` swapped for `par_iter()` (after importing the Rayon prelude):

```rust playground
// Cargo.toml: run `cargo add rayon`
use rayon::prelude::*;
use std::time::Instant;

fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    let mut d = 2;
    while d * d <= n {
        if n % d == 0 {
            return false;
        }
        d += 1;
    }
    true
}

fn main() {
    let numbers: Vec<u64> = (2..2_000_000).collect();
    println!("threads: {}", rayon::current_num_threads());

    // Sequential: one core.
    let t = Instant::now();
    let seq = numbers.iter().filter(|&&n| is_prime(n)).count();
    println!("sequential: {seq} primes in {:?}", t.elapsed());

    // Parallel: every core. The ONLY change is `iter` -> `par_iter`.
    let t = Instant::now();
    let par = numbers.par_iter().filter(|&&n| is_prime(n)).count();
    println!("parallel:   {par} primes in {:?}", t.elapsed());
}
```

Real output from `cargo run --release` on the same 8-core machine:

```text
threads: 8
sequential: 148933 primes in 188.238375ms
parallel:   148933 primes in 40.672ms
```

Two things stand out. First, Rust's *sequential* version (188 ms) is already ~6.7x faster than Node's (1255 ms) because the work is compiled and monomorphized rather than interpreted. Second, `par_iter` takes that 188 ms down to ~41 ms (roughly a 4.6x speedup on 8 cores) for the price of changing one word. The prime count, `148933`, is identical to Node's, so the parallel result is correct.

> **Note:** Always benchmark parallel code with `cargo run --release` (or `cargo bench`). A debug build leaves the per-element work unoptimized, which inflates the apparent speedup and tells you nothing about production performance.

---

## Detailed Explanation

### The prelude brings the parallel methods into scope

```rust
use rayon::prelude::*;
```

This single import adds the `par_iter`, `par_iter_mut`, and `into_par_iter` methods to standard collections (`Vec`, slices, `HashMap`, `BTreeMap`, ranges, and more) through the `IntoParallelIterator` and `IntoParallelRefIterator` traits. Without it, `par_iter` simply does not exist as a method (see [Common Pitfalls](#common-pitfalls)). This mirrors how `use std::io::Write;` is required before `write!` works on a file: the trait must be in scope.

### Three ways in: `par_iter`, `par_iter_mut`, `into_par_iter`

These map directly onto the three sequential forms a TypeScript developer already reasons about as "borrow shared / borrow mutable / take ownership":

| Sequential | Parallel | Yields | Use when |
| --- | --- | --- | --- |
| `v.iter()` | `v.par_iter()` | `&T` | You only need to read each element |
| `v.iter_mut()` | `v.par_iter_mut()` | `&mut T` | You want to mutate each element in place |
| `v.into_iter()` | `v.into_par_iter()` | `T` | You can consume the collection |

`par_iter_mut` is the parallel sweet spot for in-place transforms: because each thread gets a disjoint, non-overlapping `&mut T`, there is no aliasing and no synchronization needed. The borrow checker proves the slices don't overlap.

### What a `ParallelIterator` is (and is not)

A Rayon parallel iterator is **lazy**, just like a `std::iter::Iterator`. Nothing runs until a *consuming* operation — `collect`, `sum`, `reduce`, `count`, `for_each`, `find_any` — drives it. The adapters in between (`map`, `filter`, `filter_map`, `flat_map`) only build up a description of the work. This is the same laziness model as the standard library iterators covered in [Section 02](/02-basics/), not the eager evaluation of a JavaScript array method (which materializes a new array at every `.map`).

Internally, Rayon uses **work stealing** over a divide-and-conquer split: it recursively halves the index range, hands the halves to a global thread pool, and idle threads "steal" pending halves from busy ones. You don't manage any of this. The pool and its `join` primitive are covered in [Thread Pools with Rayon](/26-systems-programming/01-thread-pools/); this chapter stays at the iterator level.

### Reductions: `sum`, `reduce`, and order independence

```rust playground
use rayon::prelude::*;

fn main() {
    // map + collect — order is PRESERVED for indexed sources like ranges and Vec.
    let squares: Vec<u64> = (1..=8).into_par_iter().map(|n| n * n).collect();
    println!("squares: {squares:?}");

    // sum — a built-in parallel reduction.
    let total: u64 = (1..=1_000_000u64).into_par_iter().sum();
    println!("sum 1..=1_000_000: {total}");

    // reduce — explicit identity + associative combiner (here, factorial of 10).
    let product: u64 = (1..=10u64).into_par_iter().reduce(|| 1, |a, b| a * b);
    println!("10! = {product}");

    // find_any short-circuits across threads.
    let found = (1..1_000_000u64).into_par_iter().find_any(|&n| n * n == 1_000_000);
    println!("found: {found:?}");
}
```

Output:

```text
squares: [1, 4, 9, 16, 25, 36, 49, 64]
sum 1..=1_000_000: 500000500000
10! = 3628800
found: Some(1000)
```

The central concept in any parallel reduction is **associativity**. Rayon splits the data into chunks, reduces each chunk on a separate thread, and then combines the per-chunk results in an unspecified order. For `sum` and `product` that is fine because `(a + b) + c == a + (b + c)`. But your combiner must not depend on order:

- `reduce(|| 1, |a, b| a * b)` is safe: multiplication is associative.
- A combiner that subtracts, or that appends to a string expecting left-to-right order, would produce a different result on every run. Rayon's `reduce` gives you `find_any` semantics, not `find_first`: it returns *some* matching element, not necessarily the first by index.

If you need the *first* match by position, use `find_first` instead of `find_any`; it pays a small coordination cost to honor ordering.

### Order preservation in `collect`

Note that `collect` *does* preserve order when the source is indexed (a range, `Vec`, or slice): `squares` above comes back `[1, 4, 9, ...]`, not shuffled. Rayon tracks each element's position and reassembles the output `Vec` in source order, even though the work ran out of order. The shuffle risk is specific to `reduce`/`fold` combiners and to `par_bridge` (next section), not to `collect`.

---

## Key Differences

### `par_iter` vs Node `worker_threads`

| Aspect | Node.js `worker_threads` | Rayon `par_iter` |
| --- | --- | --- |
| Code change to parallelize | Spawn workers, chunk data, post/receive messages, merge | Change `iter()` to `par_iter()` |
| Memory model | Data **copied** across channel (or `SharedArrayBuffer` by hand) | Shared memory; threads borrow disjoint slices |
| Data-race safety | Your responsibility | Guaranteed by the borrow checker at compile time |
| Thread pool | You create and manage workers | Global pool created lazily, reused |
| Scheduling | Manual chunking | Automatic work-stealing, load-balanced |
| Result ordering | Whatever your merge logic does | `collect` preserves order; reductions need associativity |

### `par_iter` vs `par_bridge`

Not every iterator can be split into halves cheaply. A `Vec` knows its length and can be indexed, so Rayon splits it directly. A *sequential* iterator like `str::lines()` or a `File`'s line reader can only be advanced one item at a time; Rayon can't jump to the middle. For those, `par_bridge` adapts any `Iterator` into a `ParallelIterator` by pulling items one at a time (under a lock) and feeding them to worker threads:

```rust playground
use rayon::prelude::*;
use std::collections::HashMap;

fn expensive_hash(s: &str) -> u64 {
    // Stand-in for genuinely CPU-heavy per-item work.
    let mut h = 0u64;
    for _ in 0..50_000 {
        h = 1469598103934665603;
        for b in s.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(1099511628211);
        }
    }
    h
}

fn main() {
    let text = "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta";

    // `lines()` is a sequential iterator: bridge it into a parallel one.
    let results: HashMap<String, u64> = text
        .lines()
        .par_bridge()
        .map(|line| (line.to_string(), expensive_hash(line)))
        .collect();

    let mut keys: Vec<_> = results.keys().cloned().collect();
    keys.sort();
    for k in keys {
        println!("{k} -> {}", results[&k]);
    }
}
```

Output (sorted for stable display):

```text
alpha -> 6542418319912364133
beta -> 17583068548789615225
delta -> 14161400069455568611
epsilon -> 11109341111963135187
gamma -> 4439282355344678600
zeta -> 5298269982014079025
```

Two caveats with `par_bridge`:

1. **It does not preserve order.** Items are consumed sequentially but processed in whatever order threads finish. Collecting into a `Vec` would give you an unspecified order: collect into a `HashMap` (as above), sort afterward, or use `par_iter` on an indexed collection if order matters.
2. **The producer is a bottleneck.** Pulling items happens under a mutex, so if producing each item is itself slow (e.g. blocking I/O), `par_bridge` only parallelizes the *processing*, not the production. The win comes entirely from the per-item work being expensive relative to the cost of pulling it.

> **Tip:** When you can, read all the data into a `Vec` first and use `par_iter`: it splits more efficiently than `par_bridge` and preserves order. Reach for `par_bridge` only when the source genuinely cannot be collected up front, or when each item is so expensive that the pull cost is negligible.

### Rust is not parallel by default

A `Vec::iter()` chain runs on one thread. Parallelism in Rust is **explicit and opt-in** — you ask for it by writing `par_iter`. This is the same philosophy as the rest of the language: zero cost you didn't request. Contrast this with the common misconception that "Rust is multi-threaded by default": it is not. What Rust gives you is *fearless* concurrency: when you do opt in, the `Send`/`Sync` trait bounds and the borrow checker prevent data races at compile time. The standard threading model this builds on is covered in [Native Threads with `std::thread`](/26-systems-programming/00-threads/).

---

## Common Pitfalls

### Forgetting the prelude import

The single most common first error. Without `use rayon::prelude::*;`, the parallel methods are not in scope:

```rust
// does not compile (error[E0599]): missing `use rayon::prelude::*;`
fn main() {
    let v: Vec<i32> = (1..=10).collect();
    let sum: i32 = v.par_iter().sum();
    println!("{sum}");
}
```

The real compiler error:

```text
error[E0599]: no method named `par_iter` found for struct `Vec<i32>` in the current scope
 --> src/main.rs:3:22
  |
3 |     let sum: i32 = v.par_iter().sum();
  |                      ^^^^^^^^
  |
help: there is a method `iter` with a similar name
  |
3 -     let sum: i32 = v.par_iter().sum();
3 +     let sum: i32 = v.iter().sum();
  |
```

The fix is the import; the compiler's suggestion to use `iter` would silently make the code sequential, which is *not* what you want here.

### Mutating shared state inside `for_each`

A TypeScript developer's instinct is to push into an outer array from inside the loop. Rayon's closures are `Fn` (callable from many threads at once), so they cannot capture an outer variable by mutable reference:

```rust
// does not compile (error[E0596]): cannot mutate captured `results` from a parallel closure
use rayon::prelude::*;

fn main() {
    let mut results = Vec::new();
    (0..100).into_par_iter().for_each(|n| {
        results.push(n * n); // many threads, one Vec -> data race, rejected at compile time
    });
    println!("{}", results.len());
}
```

The real compiler error:

```text
error[E0596]: cannot borrow `results` as mutable, as it is a captured variable in a `Fn` closure
 --> src/main.rs:6:9
  |
6 |         results.push(n * n); // many threads, one Vec -> data race, rejected at compile time
  |         ^^^^^^^ cannot borrow as mutable
```

This is the borrow checker stopping a data race before it can exist. The idiomatic fix is not a lock; it's to `map` and `collect`, letting Rayon assemble the result for you:

```rust playground
use rayon::prelude::*;

fn main() {
    // Idiomatic: no shared mutable Vec, no lock. collect() reassembles in order.
    let results: Vec<u64> = (0..10).into_par_iter().map(|n| n * n).collect();
    println!("squares: {results:?}");
}
```

Output:

```text
squares: [0, 1, 4, 9, 16, 25, 36, 49, 64, 81]
```

If you genuinely need to accumulate into shared state (rare), wrap it in a `Mutex` or — better — use a `fold` + `reduce` pair so each thread accumulates locally and you merge at the end (see the [Real-World Example](#real-world-example)). Atomics are an option for simple counters; see [Atomic Operations](/26-systems-programming/04-atomic-operations/).

### Parallelizing cheap work on small inputs

Parallelism is not free: splitting, dispatching to the pool, and joining all cost time. When the per-element work is trivial and the collection is small, that overhead dwarfs the actual computation and parallel is *slower*:

```rust playground
use rayon::prelude::*;
use std::time::Instant;

fn main() {
    let data: Vec<u64> = (0..1_000).collect(); // small input, trivial work

    let _: u64 = data.par_iter().sum(); // warm up the pool

    let runs = 1000;
    let (mut seq_total, mut par_total) = (0u128, 0u128);
    for _ in 0..runs {
        let t = Instant::now();
        let s1: u64 = data.iter().map(|&x| x + 1).sum();
        seq_total += t.elapsed().as_nanos();
        std::hint::black_box(s1);

        let t = Instant::now();
        let s2: u64 = data.par_iter().map(|&x| x + 1).sum();
        par_total += t.elapsed().as_nanos();
        std::hint::black_box(s2);
    }
    println!("sequential avg: {} ns", seq_total / runs);
    println!("parallel avg:   {} ns", par_total / runs);
}
```

Output:

```text
sequential avg: 44005 ns
parallel avg:   2216750 ns
```

Here the parallel version is **~50x slower**. Adding `1` to a thousand numbers takes microseconds; the thread coordination takes milliseconds. The rule of thumb: parallelize when you have *both* a large number of elements *and* meaningful work per element. When in doubt, measure with `criterion` ([benchmarking is covered in Section 21](/21-performance/)).

### Result reordering surprises

`par_bridge` and `reduce`/`fold` combiners do **not** preserve input order. If your code assumes the output is in the same order as the input, use `par_iter` on an indexed collection with `collect` (which *does* preserve order), or use `find_first`/`collect_into_vec` rather than `find_any`. Never assume order from a parallel reduction.

---

## Best Practices

- **Prefer `map` + `collect` (or `sum`/`reduce`) over `for_each` with shared state.** Expressing the computation as a pure transformation lets Rayon handle accumulation safely and lock-free.
- **Use `fold` + `reduce` for per-thread accumulation.** When building a map or histogram, `fold` gives each thread a local accumulator and `reduce` merges them, far better than contending on a single `Mutex`.
- **Benchmark in `--release`, on representative input sizes.** A speedup in debug mode is meaningless; an input that's small in your test may be large in production (or vice versa).
- **Keep closures pure and side-effect-free.** Parallel closures should compute from their inputs, not reach out to mutate the world. This is also what makes them trivially correct.
- **Reach for `par_bridge` only when you can't collect up front.** Prefer reading into a `Vec` and using `par_iter`, which splits efficiently and preserves order.
- **Tune granularity only if profiling demands it.** Methods like `.with_min_len(n)` let you batch small items so each task does at least `n` of them, amortizing dispatch cost. Start without it; add it only if benchmarks show task overhead dominating.
- **Don't parallelize I/O-bound work with Rayon.** Rayon's pool is sized for CPU cores. For waiting on the network or disk, use async (`tokio`) or dedicated threads, not `par_iter`. See [Channels](/26-systems-programming/03-channels/) for producer/consumer pipelines across threads.

> **Warning:** Rayon's closures run on a shared global pool. If a closure blocks (sleeps, waits on I/O, or calls another blocking `par_iter`), it ties up a pool thread and can starve other parallel work or even deadlock. Keep parallel closures CPU-bound and non-blocking.

---

## Real-World Example

A common production task: aggregate word frequencies across a large corpus of documents. Each document is processed independently (embarrassingly parallel), then the per-document counts are merged. The `fold` + `reduce` pattern lets each thread build a local `HashMap` and merge them at the end, with no lock contention on a shared map:

```rust playground
// Cargo.toml: run `cargo add rayon`
use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Instant;

/// Process one document: normalize and count words.
fn word_counts(doc: &str) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    for word in doc.split_whitespace() {
        let normalized: String = word
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect();
        if !normalized.is_empty() {
            *counts.entry(normalized).or_insert(0) += 1;
        }
    }
    counts
}

/// Merge two partial frequency maps into one.
fn merge(mut a: HashMap<String, u32>, b: HashMap<String, u32>) -> HashMap<String, u32> {
    for (k, v) in b {
        *a.entry(k).or_insert(0) += v;
    }
    a
}

fn main() {
    // Synthetic corpus of 10,000 documents.
    let base = "the quick brown fox jumps over the lazy dog the fox runs fast";
    let docs: Vec<String> = (0..10_000).map(|i| format!("{base} doc{i}")).collect();

    let t = Instant::now();
    let totals: HashMap<String, u32> = docs
        .par_iter()
        .map(|doc| word_counts(doc))   // each doc -> its own map, in parallel
        .reduce(HashMap::new, merge);  // merge all the partial maps
    let elapsed = t.elapsed();

    let mut top: Vec<(&String, &u32)> = totals.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));

    println!("processed {} docs in {elapsed:?}", docs.len());
    println!("top 5 words:");
    for (word, count) in top.iter().take(5) {
        println!("  {word}: {count}");
    }
}
```

Real output from `cargo run --release`:

```text
processed 10000 docs in 48.260709ms
top 5 words:
  the: 30000
  fox: 20000
  brown: 10000
  dog: 10000
  fast: 10000
```

The shape of this code is the same map-reduce a TypeScript developer would write — `docs.map(wordCounts).reduce(merge)` — but it runs across all cores with no manual chunking, no worker spawning, and no message passing. Because `merge` is associative (`merge(merge(a, b), c) == merge(a, merge(b, c))`), Rayon is free to combine the partial maps in any order, and the borrow checker guarantees no two threads ever touch the same map at once.

> **Tip:** `reduce(HashMap::new, merge)` takes an identity *constructor* (`HashMap::new`, a function returning the empty value) and an associative combiner. This is the parallel analog of `Array.prototype.reduce(merge, {})`. The key difference is that the identity is created *per chunk*, so it must be a fresh value each time, hence a function rather than a single shared object.

When the corpus is too large to hold in memory, combine this with [Advanced File-System Operations](/26-systems-programming/06-file-system/) for directory walking, processing each file's path with `par_iter`. For workloads where the *security* of processing untrusted input matters, see [Section 27](/27-security/).

---

## Further Reading

- [Rayon documentation](https://docs.rs/rayon): the crate's API reference, including all parallel adapters.
- [`ParallelIterator` trait](https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html): the full list of parallel operations.
- [Rayon FAQ](https://github.com/rayon-rs/rayon/blob/main/FAQ.md): when parallelism helps, oversubscription, and blocking pitfalls.
- [The Rust Book — Fearless Concurrency](https://doc.rust-lang.org/book/ch16-00-concurrency.html) — the `Send`/`Sync` foundations Rayon relies on.
- [Native Threads with `std::thread`](/26-systems-programming/00-threads/): `std::thread`, the manual threading Rayon builds on.
- [Thread Pools with Rayon](/26-systems-programming/01-thread-pools/): the global pool, `join`, and custom Rayon pools.
- [Channels](/26-systems-programming/03-channels/): producer/consumer pipelines when work isn't a pure transform.
- [Atomic Operations](/26-systems-programming/04-atomic-operations/) — lock-free counters for shared state across parallel work.
- [Section 21: Performance](/21-performance/) — benchmarking with `criterion` before and after parallelizing.
- [Section 02: Basics](/02-basics/) — the sequential iterator model parallel iterators mirror.

---

## Exercises

### Exercise 1: One-word parallelization

**Difficulty:** Beginner

**Objective:** Confirm that the `iter()` → `par_iter()` swap composes with a chain of adapters.

**Instructions:** Compute the sum of the squares of all *even* numbers from 1 to 1,000,000, using a parallel iterator. Start from this sequential stub and parallelize it:

```rust playground
fn main() {
    let total: u64 = (1..=1_000_000u64)
        .into_iter()
        .filter(|n| n % 2 == 0)
        .map(|n| n * n)
        .sum();
    println!("{total}");
}
```

> **Note:** The bare range already implements both `IntoIterator` and (with Rayon imported) `IntoParallelIterator`, so the explicit `.into_iter()` here is redundant; `clippy` will flag it as `useless_conversion`. It is shown only to make the one-word swap below visually obvious: replace `.into_iter()` with `.into_par_iter()` and nothing else changes.

<details>
<summary>Solution</summary>

```rust playground
// Cargo.toml: run `cargo add rayon`
use rayon::prelude::*;

fn main() {
    let total: u64 = (1..=1_000_000u64)
        .into_par_iter()           // the only change
        .filter(|n| n % 2 == 0)
        .map(|n| n * n)
        .sum();
    println!("{total}");
}
```

Output:

```text
166667166667000000
```

`sum` is an associative reduction over `u64`, so the parallel result is identical to the sequential one. The `filter` and `map` adapters compose with the parallel iterator exactly as they do with a sequential one.

</details>

### Exercise 2: Parallel argmax

**Difficulty:** Intermediate

**Objective:** Use a parallel reduction that returns more than a single number.

**Instructions:** For every starting value `n` from 1 to 1,000,000, compute the number of steps the [Collatz sequence](https://en.wikipedia.org/wiki/Collatz_conjecture) takes to reach 1. Find the `n` (in that range) that takes the *most* steps, and print both the `n` and the step count. Do the search in parallel.

<details>
<summary>Solution</summary>

```rust playground
// Cargo.toml: run `cargo add rayon`
use rayon::prelude::*;

fn collatz_steps(mut n: u64) -> u32 {
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
    }
    steps
}

fn main() {
    let (best_n, best_steps) = (1..=1_000_000u64)
        .into_par_iter()
        .map(|n| (n, collatz_steps(n)))
        .max_by_key(|&(_, steps)| steps)
        .unwrap();
    println!("{best_n} -> {best_steps} steps");
}
```

Output:

```text
837799 -> 524 steps
```

`max_by_key` is a parallel reduction: each thread finds the max in its chunk, then the per-chunk maxima are combined. Because "maximum" is associative, the order in which chunks finish does not affect the answer. The `.unwrap()` is safe because the range is non-empty.

</details>

### Exercise 3: Parallel histogram with fold + reduce

**Difficulty:** Advanced

**Objective:** Build a shared map from parallel work *without* a lock, using per-thread accumulation.

**Instructions:** Given a block of text, build a histogram of word lengths: a map from each word length to how many words have that length. Process the words in parallel. Each thread should accumulate into its own `HashMap` (with `fold`), and the per-thread maps should be merged at the end (with `reduce`). Print the lengths in ascending order.

> **Hint:** Rayon provides `par_split_whitespace()` on `&str`, and `fold` takes an identity *constructor* plus a folding closure.

<details>
<summary>Solution</summary>

```rust playground
// Cargo.toml: run `cargo add rayon`
use rayon::prelude::*;
use std::collections::HashMap;

fn main() {
    let text = "the quick brown fox jumps over the lazy dog \
                a parallel iterator splits work across cores";

    let histogram: HashMap<usize, u32> = text
        .par_split_whitespace()
        .fold(HashMap::new, |mut acc, word| {
            *acc.entry(word.len()).or_insert(0) += 1;
            acc
        })
        .reduce(HashMap::new, |mut a, b| {
            for (k, v) in b {
                *a.entry(k).or_insert(0) += v;
            }
            a
        });

    let mut lengths: Vec<_> = histogram.into_iter().collect();
    lengths.sort();
    for (len, count) in lengths {
        println!("length {len}: {count} word(s)");
    }
}
```

Output:

```text
length 1: 1 word(s)
length 3: 4 word(s)
length 4: 3 word(s)
length 5: 4 word(s)
length 6: 2 word(s)
length 8: 2 word(s)
```

The `fold` step gives each worker thread its own `HashMap` accumulator, so threads never contend for a shared lock. The `reduce` step merges those partial maps; merging maps by summing counts is associative, so the result is deterministic even though the work runs out of order. This `fold` + `reduce` pattern is the lock-free way to build shared aggregates in parallel — far better than wrapping a single `HashMap` in a `Mutex`.

</details>
