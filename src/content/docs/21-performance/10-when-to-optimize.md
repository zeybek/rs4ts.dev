---
title: "When to Optimize: Measure First, Premature Optimization, Readable-Then-Fast"
description: "Premature optimization wastes effort in Rust as in TypeScript. Write readable code, measure in release builds, fix the proven hot spot, then re-check it still works."
---

## Quick Overview

The single most valuable performance skill is knowing *when not to* optimize. The discipline is always the same: write the clearest correct version first, **measure** it against a real workload, and only then spend effort on the parts the data proves are slow. This page is about that decision process: why guessing is almost always wrong, why **premature optimization** quietly costs you more than it saves, and how Rust changes the calculus compared to a garbage-collected runtime like Node.js.

> **Note:** This file is the *judgment* page for the section. The mechanics live in its siblings: measure with [Profiling](/21-performance/00-profiling/), [Flame Graphs](/21-performance/01-flamegraph/), and [Benchmarking](/21-performance/02-benchmarking/); then apply the techniques in [Optimization](/21-performance/03-optimization/), [Memory Layout](/21-performance/04-memory-layout/), and [Cache Efficiency](/21-performance/05-cache-efficiency/). Read this one before any of those.

---

## The Core Loop

Every credible optimization follows the same four steps, in order:

1. **Write it readably and correctly.** Idiomatic, boring code. Ship that first.
2. **Measure with a representative workload.** A benchmark, a profiler, or even a timed run, but real numbers, in a **release build**.
3. **Find the proven hot spot.** Optimize the thing that actually dominates, not the thing that *looks* expensive.
4. **Re-measure to confirm the win — and that you didn't break correctness.** If the number didn't move, revert and keep the simpler code.

Steps 1 and 4 are the ones people skip. The rest of this page is about why skipping them is a bad trade.

---

## TypeScript/JavaScript Example

Here is a realistic aggregation: summing revenue per customer across a million orders. In TypeScript you would write the obvious version and move on, and you'd be right to, because the runtime hides allocation and copying behind the garbage collector.

```typescript
interface Order {
  id: number;
  customer: string;
  totalCents: number;
}

function revenueByCustomer(orders: Order[]): Map<string, number> {
  const totals = new Map<string, number>();
  for (const o of orders) {
    totals.set(o.customer, (totals.get(o.customer) ?? 0) + o.totalCents);
  }
  return totals;
}

const orders: Order[] = [];
for (let i = 0; i < 1_000_000; i++) {
  orders.push({ id: i, customer: `customer-${i % 1000}`, totalCents: (i % 500) * 100 });
}

const t0 = performance.now();
const totals = revenueByCustomer(orders);
const t1 = performance.now();
console.log("distinct customers:", totals.size); // distinct customers: 1000
console.log("took ms:", (t1 - t0).toFixed(1));    // took ms: ~80 (machine-dependent)
```

Running this on Node v22 prints `distinct customers: 1000`. Note what you *didn't* do: you didn't think about whether `o.customer` is copied into the map, whether the `Map` resizes, or how `??` is compiled. The JIT and the GC absorb those decisions. That convenience is exactly why premature micro-optimization in JavaScript is usually pointless — you cannot see the costs, and the engine often rewrites your code anyway.

---

## Rust Equivalent

The direct Rust translation looks almost identical, and that is the point: **write the readable version first.** Rust makes one cost visible that JavaScript hid (the owned `String` key), but you do *not* eliminate it preemptively. You ship this, then measure.

```rust
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug)]
struct Order {
    id: u64,
    customer: String,
    total_cents: u64,
}

/// Readable first: clear, obviously correct. Optimize later — if measurement says so.
fn revenue_by_customer(orders: &[Order]) -> HashMap<String, u64> {
    let mut totals: HashMap<String, u64> = HashMap::new();
    for order in orders {
        *totals.entry(order.customer.clone()).or_insert(0) += order.total_cents;
    }
    totals
}

fn main() {
    let orders: Vec<Order> = (0..1_000_000u64)
        .map(|i| Order {
            id: i,
            customer: format!("customer-{}", i % 1000),
            total_cents: (i % 500) * 100,
        })
        .collect();

    let start = Instant::now();
    let totals = revenue_by_customer(&orders);
    let elapsed = start.elapsed();

    println!("distinct customers: {}", totals.len());
    println!("revenue_by_customer took {elapsed:?}");
}
```

Running this with `cargo run --release` prints:

```text
distinct customers: 1000
revenue_by_customer took 41.557209ms
```

> **Tip:** That number is a single-run illustration on one machine, not a benchmark; wall-clock timings vary run to run. For numbers you can trust and compare across changes, use [criterion](/21-performance/02-benchmarking/). The point here is the *workflow*: you measured the readable version before touching anything.

The `order.customer.clone()` allocates a fresh `String` for the lookup key on **every** iteration, even when that customer is already in the map. That is the one line a Rust developer's eye is drawn to. But "looks expensive" is a hypothesis, not a verdict. The next sections show how to decide whether it is worth fixing.

---

## Detailed Explanation

### Why "measure first" matters more in compiled languages

In JavaScript, your mental model of cost is approximate by necessity: a hidden-class deopt, an inline-cache miss, or a GC pause can dominate, and none of them are visible in the source. You learn to *not* guess because guessing is futile.

In Rust the costs are far more visible — `clone()` is a copy, `Vec` is contiguous, an `Arc` is a refcount — so it is tempting to think you can reason your way to the fast version from the source alone. **You usually can't.** The optimizer (LLVM, via `rustc`) aggressively inlines, vectorizes, and constant-folds release builds. Code that looks expensive may compile to nothing; code that looks trivial may be the bottleneck. The only reliable signal is a measurement of the optimized binary.

### The release-build trap

This is the most common first mistake, and it makes every other measurement meaningless. Consider a numeric kernel:

```rust
fn sum_of_squares(n: u64) -> u64 {
    // wrapping_* keeps the arithmetic well-defined in both debug and release.
    (0..n).fold(0u64, |acc, x| acc.wrapping_add(x.wrapping_mul(x)))
}

fn main() {
    let n = 100_000_000u64;
    let start = std::time::Instant::now();
    let result = sum_of_squares(n);
    println!("result = {result}");
    println!("elapsed = {:?}", start.elapsed());
}
```

The same binary, built two ways:

```text
$ cargo run            # debug
result = 662921401752298880
elapsed = 735.919125ms

$ cargo run --release  # release
result = 662921401752298880
elapsed = 42.625µs
```

That is not a typo: the release build is roughly **17,000× faster** here, because the optimizer recognizes the loop and collapses it into a handful of arithmetic operations, while the debug build executes 100 million iterations with overflow-check instrumentation. A timing taken in debug mode tells you nothing about production. Always benchmark `--release`. (Why the same numbers in both? `wrapping_add`/`wrapping_mul` are defined to wrap; if you used plain `+`/`*` instead, the debug build would *panic* with "attempt to add with overflow" while release would silently wrap, see [Common Pitfalls](#common-pitfalls).)

### "Looks expensive" vs. "is expensive": a worked decision

Suppose you have a reporting job that sorts events by score and renders each to a text line. Your instinct says the sort is the cost. Measure both candidates instead of trusting the instinct:

```rust
use std::fmt::Write as _;
use std::time::Instant;

#[derive(Clone)]
struct Event {
    user_id: u64,
    score: f64,
    label: String,
}

/// Tiny reusable timing helper: run `f`, print how long it took, return its result.
fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    eprintln!("{label}: {:?}", start.elapsed());
    out
}

fn build_events(n: usize) -> Vec<Event> {
    (0..n)
        .map(|i| Event {
            user_id: (i as u64).wrapping_mul(2654435761) % 1_000_000,
            score: ((i * 7 + 3) % 10_000) as f64 / 100.0,
            label: format!("event-{}", i % 64),
        })
        .collect()
}

/// Readable rendering: one fresh String per row.
fn render_naive(events: &[Event]) -> usize {
    let mut total = 0;
    for e in events {
        let line = format!("{}\t{:.2}\t{}", e.user_id, e.score, e.label);
        total += line.len();
    }
    total
}

/// Optimized rendering: one reused buffer, no per-row allocation.
fn render_buffered(events: &[Event]) -> usize {
    let mut buf = String::with_capacity(64);
    let mut total = 0;
    for e in events {
        buf.clear();
        write!(buf, "{}\t{:.2}\t{}", e.user_id, e.score, e.label).unwrap();
        total += buf.len();
    }
    total
}

fn main() {
    let events = build_events(500_000);

    // Candidate hot spots. MEASURE which dominates instead of guessing.
    let mut sortable = events.clone();
    timed("sort_by_score", || {
        sortable.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    });
    let naive_bytes = timed("render_naive", || render_naive(&events));
    let buffered_bytes = timed("render_buffered", || render_buffered(&events));

    // Correctness guard: the optimization must not change the output.
    assert_eq!(naive_bytes, buffered_bytes);
    println!(
        "rendered {naive_bytes} bytes either way; top score = {:.2}",
        sortable[0].score
    );
}
```

A representative `--release` run:

```text
sort_by_score: 31.613542ms
render_naive: 87.121792ms
render_buffered: 44.394333ms
rendered 10316314 bytes either way; top score = 99.99
```

The measurement overturns the instinct. The **sort is not the bottleneck** (~32 ms); the per-row `format!` — which allocates and frees half a million tiny `String`s — is, at ~87 ms. The targeted fix (reuse one buffer with `write!`) nearly halves rendering to ~44 ms, and the `assert_eq!` proves it produces byte-for-byte the same output. Had you "optimized" the sort first, you'd have spent effort on the smaller cost and possibly traded away the readable `sort_by`. The buffer technique itself belongs to [Optimization](/21-performance/03-optimization/); the *decision to apply it here* came from measuring.

---

## Key Differences

| Question | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Can I reason about cost from the source? | Rarely; JIT/GC hide it | Better, but the optimizer still surprises you; **measure** |
| Does build mode change my timings? | Minor (always JIT-warmed) | **Enormous**: debug vs. release can differ by 1000×+ |
| Cost of a "readable first" default | GC absorbs allocations for you | You see allocations, but the compiler often elides them |
| What does premature optimization cost? | Wasted effort; engine may undo it | Wasted effort **plus** lost safety/readability (`unsafe`, hand-rolled loops) |
| Where is the *real* baseline win? | Algorithm + avoiding the event-loop stall | Often free: no GC pauses, contiguous data, *before* you tune anything |
| Tool for trustworthy numbers | `performance.now()`, `--prof`, clinic.js | [criterion](/21-performance/02-benchmarking/), [samply/perf](/21-performance/00-profiling/), [flame graphs](/21-performance/01-flamegraph/) |

The deeper point: Rust's idiomatic, readable default is *already* fast relative to a GC'd runtime: no garbage collector, no boxed numbers, cache-friendly `Vec`s. You usually start from a much higher floor, which means there is even less reason to micro-optimize before measuring. The honest, full comparison lives in [Performance vs. Node.js](/21-performance/09-comparison/).

> **Note:** "Premature optimization is the root of all evil" (Donald Knuth, 1974) is almost always quoted without its qualifier. The full sentence is: *"We should forget about small efficiencies, say about 97% of the time: premature optimization is the root of all evil. **Yet we should not pass up our opportunities in that critical 3%.**"* The skill is identifying the 3%, which requires measurement, not instinct.

---

## Common Pitfalls

### Pitfall 1: Benchmarking a debug build

`cargo run` and `cargo test` build **without** optimizations by default. A timing taken there is dominated by un-inlined function calls and overflow checks and bears no relation to production. Always measure with `--release` (or `cargo bench`, which is release by default). This is the same overflow-check behavior that makes a plain `a + b` *panic* in debug while wrapping in release:

```rust
fn sum_of_squares(n: u64) -> u64 {
    (0..n).map(|x| x * x).sum() // plain `*` and `sum`: overflow-checked in debug
}

fn main() {
    println!("{}", sum_of_squares(50_000_000));
}
```

In a debug build this aborts with a real panic, proof you were never measuring steady-state behavior:

```text
thread 'main' panicked at .../library/core/src/iter/traits/accum.rs:149:1:
attempt to add with overflow
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### Pitfall 2: Optimizing the thing that *looks* slow

As the worked example showed, the obvious-looking line (the sort) was not the hot spot. Profile before you touch anything: see [Profiling](/21-performance/00-profiling/) and [Flame Graphs](/21-performance/01-flamegraph/). A change that doesn't move a profiled number is not an optimization — it is just risk.

### Pitfall 3: Reaching for cleverness that doesn't even compile

A classic premature micro-optimization is mutating a collection "in place" while iterating it, to "avoid a second pass." Rust's borrow checker rejects it outright:

```rust
fn main() {
    let mut data = vec![1, 2, 3, 4, 5, 6];
    // does not compile (error[E0502]): "optimize" by removing while iterating
    for (i, &x) in data.iter().enumerate() {
        if x % 2 == 0 {
            data.remove(i);
        }
    }
    println!("{data:?}");
}
```

The real compiler error:

```text
error[E0502]: cannot borrow `data` as mutable because it is also borrowed as immutable
 --> src/main.rs:6:13
  |
4 |     for (i, &x) in data.iter().enumerate() {
  |                    -----------------------
  |                    |
  |                    immutable borrow occurs here
  |                    immutable borrow later used here
5 |         if x % 2 == 0 {
6 |             data.remove(i);
  |             ^^^^^^^^^^^^^^ mutable borrow occurs here
```

The clean version is also the fast one — a single in-place pass via the standard library:

```rust
fn main() {
    let mut data = vec![1, 2, 3, 4, 5, 6];
    data.retain(|&x| x % 2 != 0); // readable AND fast
    println!("{data:?}"); // [1, 3, 5]
}
```

This is the recurring lesson: in Rust the idiomatic, readable form is frequently the fastest one too, so "clever" rewrites cost readability (and sometimes `unsafe`) for nothing.

### Pitfall 4: Trusting one wall-clock number

A single `Instant::now()` reading is noisy: CPU frequency scaling, the OS scheduler, and cold caches all move it. Use it for rough direction only. For a number you can put in a PR description or compare across commits, use [criterion](/21-performance/02-benchmarking/), which warms up, takes many samples, and reports a confidence interval plus regression detection.

### Pitfall 5: Optimizing before you have a representative workload

Tuning against a 10-element array tells you about constant factors that vanish at scale, and nothing about the algorithm. Measure with input that resembles production in **size and shape** (key distribution, string lengths, cardinality). The wrong workload produces confident, wrong conclusions.

---

## Best Practices

- **Write the boring version first.** Idiomatic iterators, `String`/`Vec`, `clone()` where it keeps the code clear. Ship it. (See [Functions](/03-functions/) and [Collections](/07-collections/) for what "idiomatic" looks like.)
- **Set a target.** "Fast enough" needs a number: a p99 latency, a throughput floor, a memory ceiling. Without a target you will optimize forever and ship never.
- **Always measure in `--release`,** and prefer [criterion](/21-performance/02-benchmarking/) over hand-rolled timers for anything you'll act on.
- **Profile to find the hot spot, then optimize only that.** [Profiling](/21-performance/00-profiling/) and [flame graphs](/21-performance/01-flamegraph/) point at the 3% that matters.
- **Re-measure after every change,** and keep a correctness check (an `assert_eq!`, a snapshot test) so a faster-but-wrong version can't sneak through.
- **Prefer algorithmic wins over micro-tuning.** Going O(n²) → O(n log n) beats any amount of constant-factor fiddling; a `HashMap` or `HashSet` lookup beats a hand-tuned linear scan.
- **Treat allocation as the default suspect, but confirm it.** Needless allocation is the most common real Rust hot spot, which is why [Optimization](/21-performance/03-optimization/) leads with it. Even so, confirm with a profiler before rewriting.
- **Keep the simpler version if the win is marginal.** If a change is within noise, revert it. Maintainability is a performance feature: code you can change quickly is code you can fix and speed up later.
- **Lean on the free baseline.** No GC pauses and cache-friendly data give Rust a high starting floor; often "readable Rust" already meets the target and you optimize nothing. See [Performance vs. Node.js](/21-performance/09-comparison/).

---

## Real-World Example

A small, reusable measurement setup is worth more than any single optimization, because it turns "I think this is faster" into "this is 1.9× faster on our workload." Here is a self-contained one that compares two implementations of a word-frequency counter against a realistic corpus and **proves they agree** before reporting timings: exactly the loop you'd run before deciding whether the "optimized" version is worth keeping.

```rust
use std::collections::HashMap;
use std::time::Instant;

/// Reusable timing helper: run `f`, print elapsed, return the result.
fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    println!("{label}: {:?}", start.elapsed());
    out
}

/// Naive: allocate a fresh owned, lowercased key for EVERY word — even repeats.
fn count_naive(text: &str) -> HashMap<String, u64> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        let key = word.to_lowercase(); // always allocates
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

/// Fast: look up by borrowed &str first; allocate the owned key only on first insert.
fn count_fast(text: &str) -> HashMap<String, u64> {
    let mut counts: HashMap<String, u64> = HashMap::new();
    for word in text.split_whitespace() {
        if let Some(v) = counts.get_mut(word) {
            *v += 1;
        } else {
            counts.insert(word.to_string(), 1);
        }
    }
    counts
}

fn main() {
    // Representative workload: a large, already-lowercase corpus.
    let base = "the quick brown fox jumps over the lazy dog the fox runs ";
    let text = base.repeat(200_000);

    let a = timed("count_naive", || count_naive(&text));
    let b = timed("count_fast", || count_fast(&text));

    // Correctness guard BEFORE we trust the speedup.
    let mut ka: Vec<_> = a.iter().collect();
    let mut kb: Vec<_> = b.iter().collect();
    ka.sort();
    kb.sort();
    assert_eq!(ka, kb, "the two implementations must agree");

    println!("distinct words: {}, 'the' = {}", a.len(), a["the"]);
}
```

A representative `--release` run:

```text
count_naive: 103.63775ms
count_fast: 56.74ms
distinct words: 9, 'the' = 600000
```

The measurement justifies the change: avoiding a per-word allocation when the word is already lowercase and already counted roughly **halves** the time (~104 ms → ~57 ms), and the `assert_eq!` proves the result is identical. This is the complete loop in miniature: readable baseline, representative workload, measured hot spot (the `to_lowercase()` allocation), targeted fix, confirmed win, preserved correctness. The borrowing technique that made it faster is covered in depth in [Optimization](/21-performance/03-optimization/); the *decision to apply it* is what this page is about.

> **Note:** In production you would graduate from `timed` to [criterion](/21-performance/02-benchmarking/) for statistically sound numbers, and from "two functions in `main`" to a profiler ([Profiling](/21-performance/00-profiling/)) to find which function to look at in the first place. `timed` is the gateway drug, not the destination.

---

## Further Reading

- [The Rust Performance Book](https://nnethercote.github.io/perf-book/): the standard reference; its first chapter is, fittingly, "Benchmarking."
- [`std::time::Instant`](https://doc.rust-lang.org/std/time/struct.Instant.html) — the monotonic clock behind quick timing helpers.
- [The Cargo Book: Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) — why `dev` and `release` differ, and how to configure them.
- "Structured Programming with `go to` Statements," Donald Knuth, *ACM Computing Surveys*, 1974 — the origin of the "premature optimization" quote, in full context.

### Cross-links within this guide

- [Benchmarking with criterion](/21-performance/02-benchmarking/): trustworthy, statistically sound numbers (the upgrade from `timed`).
- [Profiling](/21-performance/00-profiling/) and [Flame Graphs](/21-performance/01-flamegraph/) — finding the hot spot *before* you optimize.
- [Optimization Techniques](/21-performance/03-optimization/) — what to do once the data names the culprit (clones, allocations, `&str`, iterators, capacity).
- [Performance vs. Node.js](/21-performance/09-comparison/) — the honest baseline: where Rust wins for free, and where it doesn't.
- [Memory Layout](/21-performance/04-memory-layout/) and [Cache Efficiency](/21-performance/05-cache-efficiency/) — deeper, measurement-driven tuning.
- [Common Patterns](/22-common-patterns/): idiomatic, readable defaults to write *first*.
- Foundations: [Getting Started](/01-getting-started/) (release vs. debug builds with Cargo) and [Basics](/02-basics/) (the types whose costs you'll be reasoning about).

---

## Exercises

### Exercise 1: Build a Timing Harness and Compare Two Approaches

**Difficulty:** Beginner

**Objective:** Practice the measure-first loop with a reusable `timed` helper, and confirm both approaches produce the same answer.

**Instructions:** Write a `timed<T>(label, f)` helper that prints how long `f` took and returns its result. Use it to compare two ways of summing the multiples of 3 below `n`: one that `collect`s into a `Vec` first, and one that sums lazily in a single pass. Run with `cargo run --release` on `n = 20_000_000` and `assert_eq!` that both agree.

```rust
use std::time::Instant;

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    // TODO: time `f`, print "{label}: {elapsed:?}", return its result
    todo!()
}

fn sum_collect(n: u64) -> u64 {
    /* ??? collect multiples of 3 into a Vec, then sum */
    todo!()
}

fn sum_lazy(n: u64) -> u64 {
    /* ??? sum multiples of 3 in one lazy pass */
    todo!()
}

fn main() {
    let n = 20_000_000u64;
    let a = timed("sum_collect", || sum_collect(n));
    let b = timed("sum_lazy", || sum_lazy(n));
    assert_eq!(a, b);
    println!("both agree: {a}");
}
```

<details>
<summary>Solution</summary>

```rust
use std::time::Instant;

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    println!("{label}: {:?}", start.elapsed());
    out
}

// Approach A: collect into a Vec, then sum (one intermediate allocation).
fn sum_collect(n: u64) -> u64 {
    let v: Vec<u64> = (0..n).filter(|x| x % 3 == 0).collect();
    v.iter().sum()
}

// Approach B: one lazy pass, no intermediate Vec.
fn sum_lazy(n: u64) -> u64 {
    (0..n).filter(|x| x % 3 == 0).sum()
}

fn main() {
    let n = 20_000_000u64;
    let a = timed("sum_collect", || sum_collect(n));
    let b = timed("sum_lazy", || sum_lazy(n));
    assert_eq!(a, b);
    println!("both agree: {a}");
}
```

A representative `--release` run:

```text
sum_collect: 25.619667ms
sum_lazy: 13.340375ms
both agree: 66666663333333
```

The lazy version is roughly twice as fast because it never materializes the multiples into a `Vec`: there is no intermediate allocation to fill and walk. Both produce `66666663333333`, so the faster version is also correct. (Numbers are illustrative and machine-dependent; the lesson is the workflow, and that "readable lazy iterators" won without any cleverness.)

</details>

### Exercise 2: Find and Fix the Proven Hot Spot

**Difficulty:** Intermediate

**Objective:** Use measurement to locate a hot spot, apply a targeted fix, and prove correctness was preserved.

**Instructions:** You are given `count_naive`, which counts word frequencies but allocates a brand-new owned key for **every** word via `to_lowercase()`, even for repeats. The corpus is already lowercase. Write `count_fast` that looks up by borrowed `&str` first and only allocates the owned key on first insert. Time both with the `timed` helper from Exercise 1 against `base.repeat(200_000)`, and `assert_eq!` their results (sort the entries first, since `HashMap` order is arbitrary).

```rust
use std::collections::HashMap;
use std::time::Instant;

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    println!("{label}: {:?}", start.elapsed());
    out
}

fn count_naive(text: &str) -> HashMap<String, u64> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        let key = word.to_lowercase();
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn count_fast(text: &str) -> HashMap<String, u64> {
    // TODO: look up by &str first; only allocate the owned key on first insert
    todo!()
}

fn main() {
    let base = "the quick brown fox jumps over the lazy dog the fox runs ";
    let text = base.repeat(200_000);
    let a = timed("count_naive", || count_naive(&text));
    let b = timed("count_fast", || count_fast(&text));
    let mut ka: Vec<_> = a.iter().collect();
    let mut kb: Vec<_> = b.iter().collect();
    ka.sort();
    kb.sort();
    assert_eq!(ka, kb);
    println!("distinct words: {}, 'the' = {}", a.len(), a["the"]);
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;
use std::time::Instant;

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    println!("{label}: {:?}", start.elapsed());
    out
}

fn count_naive(text: &str) -> HashMap<String, u64> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        let key = word.to_lowercase(); // allocates on every word
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn count_fast(text: &str) -> HashMap<String, u64> {
    let mut counts: HashMap<String, u64> = HashMap::new();
    for word in text.split_whitespace() {
        // Borrowed lookup: no allocation when the word is already present.
        if let Some(v) = counts.get_mut(word) {
            *v += 1;
        } else {
            counts.insert(word.to_string(), 1); // allocate only on first insert
        }
    }
    counts
}

fn main() {
    let base = "the quick brown fox jumps over the lazy dog the fox runs ";
    let text = base.repeat(200_000);

    let a = timed("count_naive", || count_naive(&text));
    let b = timed("count_fast", || count_fast(&text));

    let mut ka: Vec<_> = a.iter().collect();
    let mut kb: Vec<_> = b.iter().collect();
    ka.sort();
    kb.sort();
    assert_eq!(ka, kb, "the two implementations must agree");

    println!("distinct words: {}, 'the' = {}", a.len(), a["the"]);
}
```

A representative `--release` run:

```text
count_naive: 103.63775ms
count_fast: 56.74ms
distinct words: 9, 'the' = 600000
```

The naive version allocates a `String` for every one of the 2.4 million word occurrences; the fast version allocates only nine times (once per distinct word). Roughly halving the time, with `assert_eq!` confirming identical counts, is the textbook outcome of measure → fix the proven hot spot → re-measure. (If the corpus contained mixed case, you would need case-insensitive keys and the trade-off would be different, another reason to measure against a *representative* workload.)

</details>

### Exercise 3: Decide With a Benchmark — and Honor the Result

**Difficulty:** Advanced

**Objective:** Use criterion to compare two correct implementations and practice the hardest part of the loop: keeping the simpler code when the "optimization" doesn't clearly win.

**Instructions:** Create a `--dev`-dependency on criterion (`cargo add criterion --dev`) and a `benches/dedup.rs` registered with `harness = false`. Benchmark two ways to count distinct values in a `&[u32]` with many duplicates: `distinct_sort` (clone, `sort_unstable`, `dedup`, `len`) versus `distinct_set` (collect into a `HashSet`, take `len`). Wrap inputs in `black_box`. Run `cargo bench` and write down which you would ship — and why.

```toml
# Cargo.toml
[dev-dependencies]
criterion = "0.8"

[[bench]]
name = "dedup"
harness = false
```

```rust
// benches/dedup.rs
use std::collections::HashSet;
use criterion::{criterion_group, criterion_main, Criterion, black_box};

fn distinct_sort(input: &[u32]) -> usize {
    // TODO: clone, sort_unstable, dedup, return len
    todo!()
}

fn distinct_set(input: &[u32]) -> usize {
    // TODO: collect into a HashSet, return len
    todo!()
}

fn bench(c: &mut Criterion) {
    // TODO: build a duplicate-heavy dataset and bench both, using black_box
    let _ = c;
}

criterion_group!(benches, bench);
criterion_main!(benches);
```

<details>
<summary>Solution</summary>

```rust
// benches/dedup.rs
use std::collections::HashSet;
use criterion::{criterion_group, criterion_main, Criterion, black_box};

// Approach A: sort then dedup (allocates one sorted copy).
fn distinct_sort(input: &[u32]) -> usize {
    let mut v = input.to_vec();
    v.sort_unstable();
    v.dedup();
    v.len()
}

// Approach B: HashSet.
fn distinct_set(input: &[u32]) -> usize {
    let set: HashSet<u32> = input.iter().copied().collect();
    set.len()
}

fn bench(c: &mut Criterion) {
    // Duplicate-heavy: 100k values drawn from only 5k distinct keys.
    let data: Vec<u32> = (0..100_000u32).map(|i| (i.wrapping_mul(2654435761)) % 5000).collect();

    let mut group = c.benchmark_group("distinct");
    group.bench_function("sort", |b| b.iter(|| distinct_sort(black_box(&data))));
    group.bench_function("set", |b| b.iter(|| distinct_set(black_box(&data))));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
```

A representative `cargo bench` run:

```text
distinct/sort           time:   [879.07 µs 904.35 µs 946.40 µs]
distinct/set            time:   [956.96 µs 985.16 µs 1.0266 ms]
```

On this workload the two are **within roughly 10% of each other**, not a decisive win for either. That is the lesson: the benchmark didn't crown a clear champion, so you ship whichever is clearer for your codebase (often the `HashSet`, which expresses intent directly and doesn't mutate a copy), and you do **not** invent a third "clever" variant chasing a margin this thin. The honest answer to "which is faster?" is sometimes "it doesn't matter — pick the readable one." (The relationship would flip with a different distribution; for instance, very few duplicates makes the `HashSet` relatively worse, which is precisely why you benchmark your *real* data, per [Benchmarking](/21-performance/02-benchmarking/).)

</details>
