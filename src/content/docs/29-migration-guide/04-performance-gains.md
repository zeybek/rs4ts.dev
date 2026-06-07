---
title: "Measuring Performance Gains Honestly"
description: "Benchmark a Node.js-to-Rust port honestly: report p99 latency and memory, not averages, with Criterion and HdrHistogram, and dodge coordinated omission."
---

When you port a Node.js service to Rust, someone will eventually ask: "So how much faster is it?" Answering that question honestly -- with the right metric, the right workload, and a clear-eyed view of where Rust actually helps -- is what separates a credible migration from a hype-driven one.

---

## Quick Overview

The headline "Rust is 10x faster" is almost always misleading, because it measures the wrong thing. For a typical web service the bottleneck is rarely raw CPU; it is the **tail latency** (p99/p99.9), memory footprint, and predictability under load. This page shows you how to benchmark the right thing, report **latency percentiles** instead of averages, measure memory honestly, and avoid the traps that make a migration look better (or worse) than it really is.

> **Note:** "Honestly" is the operative word. A migration that genuinely cuts p99 latency from 800 ms to 40 ms is a huge win even if median latency barely moved. Selling it as "20x faster on average" is both wrong and unnecessary; the real, defensible number is impressive enough.

---

## TypeScript/JavaScript Example

Here is the kind of measurement most teams start with: wrap the handler in `console.time`, hit it a few times, eyeball the average.

```typescript
// bench-naive.ts -- the way most teams "measure" first (and get fooled)
import { performance } from "node:perf_hooks";

function dayOfYear(date: string): number | null {
  const parts = date.split("-");
  if (parts.length !== 3) return null;
  const [y, m, d] = parts.map(Number);
  if (!Number.isInteger(y) || m < 1 || m > 12 || d < 1) return null;
  const leap = (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0;
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  if (d > days[m - 1]) return null;
  let total = d;
  for (let i = 0; i < m - 1; i++) total += days[i];
  return total;
}

const iters = 1_000_000;
const start = performance.now();
for (let i = 0; i < iters; i++) dayOfYear("2026-06-02");
const ms = performance.now() - start;
console.log(`${iters} iters in ${ms.toFixed(1)} ms (~${((ms * 1e6) / iters).toFixed(1)} ns/call)`);
```

This has three problems that recur in *every* naive benchmark, in any language:

- **It measures the wrong unit of work.** A microsecond-level pure function is almost never what limits a real service; the JSON parsing, the database round-trip, and the event loop are.
- **It reports a single average.** One number hides the distribution. Users feel the slow requests, not the mean.
- **The JIT and the optimizer can cheat.** V8 may hoist or eliminate a pure call whose result is unused; the loop can warm into a wildly different code path than production ever runs. You are timing the benchmark, not the workload.

To measure a *service*, you need percentiles of end-to-end request latency under realistic concurrency, not a tight loop over a pure function.

```typescript
// percentiles.ts -- the metric that actually matters: the distribution, not the mean
function percentile(sorted: number[], pct: number): number {
  if (sorted.length === 0) return 0;
  const rank = Math.ceil((pct / 100) * sorted.length);
  const idx = Math.min(Math.max(rank - 1, 0), sorted.length - 1);
  return sorted[idx];
}

// Latencies (ms) collected from a load test: mostly fast, a few slow.
const samples = [
  120, 130, 118, 125, 122, 119, 121, 117, 200, 9800,
  124, 126, 123, 128, 131, 115, 116, 127, 129, 4200,
].sort((a, b) => a - b);

const mean = samples.reduce((a, b) => a + b, 0) / samples.length;
console.log("mean", mean.toFixed(1)); // 814.5  <- dominated by two outliers
console.log("p50 ", percentile(samples, 50)); // 124
console.log("p90 ", percentile(samples, 90)); // 200
console.log("p99 ", percentile(samples, 99)); // 9800 <- what 1% of users actually feel
```

Running it under Node v22:

```text
mean 814.5
p50  124
p90  200
p99  9800
```

The **mean of 814.5 ms is a lie**: no single request took that long. Half the users saw 124 ms; the unlucky 1% saw nearly 10 seconds. This is exactly why you report percentiles.

---

## Rust Equivalent

The same percentile logic in Rust, with the same data, produces the same conclusion (the average hides the tail):

```rust
/// Value at a given percentile from a sorted slice, using the nearest-rank method.
fn percentile(sorted: &[u64], pct: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    // nearest-rank: rank = ceil(p/100 * N), 1-based
    let rank = ((pct / 100.0) * sorted.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

fn main() {
    // Simulated request latencies in milliseconds (the same data as the TS example).
    let mut samples: Vec<u64> = vec![
        120, 130, 118, 125, 122, 119, 121, 117, 200, 9800,
        124, 126, 123, 128, 131, 115, 116, 127, 129, 4200,
    ];
    samples.sort_unstable();

    let n = samples.len();
    let mean = samples.iter().sum::<u64>() as f64 / n as f64;

    println!("samples   : {n}");
    println!("mean      : {mean:.1} ms");
    println!("p50       : {} ms", percentile(&samples, 50.0));
    println!("p90       : {} ms", percentile(&samples, 90.0));
    println!("p99       : {} ms", percentile(&samples, 99.0));
    println!("max       : {} ms", samples[n - 1]);
}
```

Real output from `cargo run`:

```text
samples   : 20
mean      : 814.5 ms
p50       : 124 ms
p90       : 200 ms
p99       : 9800 ms
max       : 9800 ms
```

For real workloads, do not hand-roll percentiles over a giant `Vec`: it costs memory proportional to sample count and a full sort. Use a histogram. The `hdrhistogram` crate (a port of the widely used HdrHistogram) records values into fixed-precision buckets in O(1) per sample and answers any quantile cheaply:

```toml
# Cargo.toml
[dependencies]
hdrhistogram = "7.5.4"
```

```rust
use hdrhistogram::Histogram;

fn main() {
    // Record latencies in milliseconds with 3 significant digits of precision.
    let mut hist = Histogram::<u64>::new(3).expect("create histogram");

    let samples: [u64; 20] = [
        120, 130, 118, 125, 122, 119, 121, 117, 200, 9800,
        124, 126, 123, 128, 131, 115, 116, 127, 129, 4200,
    ];
    for &v in &samples {
        hist.record(v).expect("value in range");
    }

    println!("count : {}", hist.len());
    println!("mean  : {:.1} ms", hist.mean());
    println!("p50   : {} ms", hist.value_at_quantile(0.50));
    println!("p90   : {} ms", hist.value_at_quantile(0.90));
    println!("p99   : {} ms", hist.value_at_quantile(0.99));
    println!("max   : {} ms", hist.max());
}
```

Real output:

```text
count : 20
mean  : 814.8 ms
p50   : 124 ms
p90   : 200 ms
p99   : 9807 ms
max   : 9807 ms
```

> **Note:** The p99 reads `9807` instead of `9800` and the mean is `814.8` instead of `814.5`. That is not a bug. A histogram trades a tiny, bounded error (here, 3 significant digits) for constant memory and O(1) recording, so a billion samples cost the same as twenty. For latency dashboards this is exactly the trade you want.

---

## Detailed Explanation

### Why percentiles, not averages

A web service's latency distribution is **right-skewed**: most requests are fast, a long tail is slow (GC pauses, a cold cache, a lock contended under load, a slow database query). The mean is dragged toward the tail by a handful of outliers, so it describes no real request. Percentiles describe the actual experience:

- **p50 (median):** the typical request. Half are faster, half slower.
- **p90 / p95:** what your "normal but busy" users feel.
- **p99 / p99.9:** the tail. On a page that fans out to 100 backend calls, a p99 of 1% means roughly *every* page render hits at least one slow call. Tail latency compounds.

This is the single most important reporting change a migration should make, and it is **independent of language**. But it matters *more* when comparing Node.js to Rust, because the two have very different tail behavior.

### Where Rust actually moves the needle

Rust does not make your business logic algorithmically faster; an O(n²) loop is O(n²) in both languages. The wins come from a few specific places:

| Source of gain | Why Rust helps | Where it shows up |
| --- | --- | --- |
| No GC pauses | No stop-the-world collector; memory freed deterministically at scope end | p99/p99.9 tail latency, jitter |
| No JIT warm-up | Ahead-of-time compiled native code from the first request | cold start, autoscaling, serverless |
| Compact memory layout | `Vec<i64>` is a packed array; no per-element boxing | RSS, cache locality, throughput |
| True parallelism | No single event loop; CPU-bound work spreads across cores | CPU-bound endpoints, batch jobs |
| Predictable cost | No hidden megamorphic deopts or hidden-class churn | latency variance |

Notice what is **not** on that list: a 10x drop in median latency for an I/O-bound CRUD endpoint. If your handler spends 95% of its time waiting on Postgres, rewriting the other 5% in Rust changes p50 by almost nothing. The honest pitch is: *Rust flattens the distribution.* The median may improve modestly; the tail and the memory often improve dramatically.

### The benchmark that fools you

Here is the naive Rust micro-benchmark equivalent to the TypeScript one, and it is just as misleading:

```rust
use std::hint::black_box;
use std::time::Instant;

/// Parse an ISO-8601 date "YYYY-MM-DD" and return the day of the year.
fn day_of_year(date: &str) -> Option<u32> {
    let mut parts = date.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if day > days[(month - 1) as usize] {
        return None;
    }
    let mut total = day;
    for m in 0..(month - 1) as usize {
        total += days[m];
    }
    Some(total)
}

fn main() {
    let iters = 1_000_000;
    let start = Instant::now();
    for _ in 0..iters {
        // black_box stops the optimizer from deleting a pure call whose
        // result is unused -- without it, this loop could compile to nothing.
        black_box(day_of_year(black_box("2026-06-02")));
    }
    let elapsed = start.elapsed();
    println!("{iters} iters in {elapsed:?}");
    println!("~{:.1} ns/call", elapsed.as_nanos() as f64 / iters as f64);
}
```

Real output from `cargo run --release`:

```text
1000000 iters in 28.742084ms
~28.7 ns/call
```

`std::hint::black_box` is the one thing this version gets right: it forces the compiler to treat the value as opaque, so it cannot constant-fold the input or eliminate the unused result. (The TypeScript version has no equivalent guard at all -- V8 may quietly delete the work.) But it is *still* a bad service benchmark: one sample, no warm-up control, and a workload (a pure function on a fixed string) that no production request actually runs. It tells you the function is fast. It tells you nothing about your service.

For a defensible per-function number you want a statistical harness. See the Real-World Example below.

---

## Key Differences

| Aspect | Node.js / TypeScript | Rust |
| --- | --- | --- |
| Headline metric people quote | "X requests/sec" or mean latency | should be p99 latency + RSS |
| Memory model | GC heap; per-object overhead; periodic pauses | deterministic free at scope; packed layouts |
| Latency tail driver | GC, JIT deopt, single event loop | mostly lock contention / I/O, no GC pauses |
| Cold start | JIT must warm up | native code, fast from request one |
| Built-in micro-bench guard | none (`performance.now()` is manual) | `std::hint::black_box`, plus Criterion |
| Standard bench tool | `tinybench`, `mitata`, autocannon (HTTP) | Criterion (functions), wrk/oha/k6 (HTTP) |
| Parallel CPU work | worker threads (heavy) | threads / `rayon` (cheap, safe) |

The biggest conceptual shift for a TypeScript developer is that **the average stops being the metric and the tail becomes the metric** -- and Rust's advantage is most visible precisely in that tail, where Node.js pays for garbage collection and JIT.

> **Tip:** When you publish migration numbers, always report the *measurement conditions* alongside them: hardware, concurrency level, payload size, dataset size, build profile (`--release`!), and number of samples. A number without its conditions is not reproducible and not credible.

---

## Common Pitfalls

### Benchmarking a debug build

The single most common mistake. A debug build (`cargo run`, no flags) has **no optimizations** and can be 10-100x slower than release. If you compare a Node.js service against a debug-mode Rust binary, Rust may look *slower*. Always benchmark `--release`.

```rust
fn main() {
    // Run this with `cargo run` and again with `cargo run --release`.
    let start = std::time::Instant::now();
    let mut acc: u64 = 0;
    for i in 0..50_000_000u64 {
        acc = acc.wrapping_add(i);
    }
    println!("{acc} in {:?}", start.elapsed());
}
```

The debug build runs the bounds-checked, unoptimized loop; the release build often optimizes the whole thing into a closed-form computation. The gap is enormous, and it is *entirely* an artifact of the build profile, not the language.

### Comparing apples to oranges

Make the two services do the *same work* under the *same conditions*. A mismatch quietly invalidates the comparison:

- Same connection pool size, same database, same dataset, same indexes.
- Same payload shapes and sizes (see [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/)).
- Same concurrency in the load generator. A Rust service that uses all cores will trivially beat a single Node.js process, but the fair comparison is against a Node.js cluster across the same cores.
- Warm both services before measuring (JIT warm-up, connection pools, OS page cache).

### Reporting the mean

We have hammered this, but it bears repeating because it is so tempting: the mean is dominated by outliers and describes no real request. Report p50/p90/p99 and the max. If you must give one number, give p99.

### Coordinated omission

A subtle but devastating measurement bug. If your load generator sends a request, *waits* for the response, and only then sends the next one, then a slow response *delays* the requests behind it -- which are never recorded as slow. Your tool silently omits exactly the measurements that matter, and your p99 looks far better than reality. Use a load generator that sends at a fixed rate (an open-model tool like `wrk2`, `oha`, or `k6` configured with a constant arrival rate), or correct for it, so the queue delay shows up in the numbers.

### Letting the optimizer delete your benchmark

In Rust, a pure function whose result is discarded can be optimized away entirely, making it look infinitely fast. Wrap inputs and outputs in `std::hint::black_box` (as shown above), or use Criterion, which does this for you. In Node.js, V8 can do the same thing; assign the result somewhere observable so the JIT cannot eliminate the call.

---

## Best Practices

- **Measure the service, not the function.** End-to-end request latency under realistic concurrency is the number leadership and users care about. Per-function micro-benchmarks are for *finding* a regression, not for the migration headline.
- **Always report percentiles.** p50, p90, p99, p99.9, and max. Pair each with the measurement conditions.
- **Use the right tool for each layer.** Criterion for pure functions; an HTTP load generator with a fixed arrival rate (`oha`, `k6`, `wrk2`) for the service; `hdrhistogram` to aggregate latencies in your own load harness.
- **Always benchmark `--release`** (and ideally with `lto = true` for the production profile, matching what you actually deploy).
- **Measure memory honestly** (RSS, not just heap) and report the steady-state footprint under load, not at idle.
- **Gate regressions in CI.** Once you have a baseline, fail the build when p99 or memory regresses past a budget. See the Real-World Example.
- **Be honest about where Rust does not help.** If a CRUD endpoint is 95% database wait, say so. Overclaiming erodes trust in the genuinely large wins. See [Common Migration Challenges](/29-migration-guide/05-common-challenges/) for when *not* to migrate at all.

---

## Real-World Example

Here is a credible, reproducible benchmarking setup for a ported function, using **Criterion** -- the standard statistical benchmarking harness for Rust. It runs many iterations, controls warm-up, applies `black_box`, and reports a confidence interval instead of a single noisy number.

```toml
# Cargo.toml
[package]
name = "probe"
version = "0.1.0"
edition = "2024"

[dev-dependencies]
criterion = "0.8.2"

[[bench]]
name = "parsing"
harness = false
```

```rust
// src/lib.rs -- the hot, pure function we ported from the Node service.
/// Parse an ISO-8601 date "YYYY-MM-DD" and return the day of the year.
/// Returns None on malformed input.
pub fn day_of_year(date: &str) -> Option<u32> {
    let mut parts = date.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if day > days[(month - 1) as usize] {
        return None;
    }
    let mut total = day;
    for m in 0..(month - 1) as usize {
        total += days[m];
    }
    Some(total)
}
```

```rust
// benches/parsing.rs
use criterion::{criterion_group, criterion_main, Criterion};
use probe::day_of_year;
use std::hint::black_box;

fn bench_day_of_year(c: &mut Criterion) {
    c.bench_function("day_of_year 2026-06-02", |b| {
        b.iter(|| day_of_year(black_box("2026-06-02")));
    });
}

criterion_group!(benches, bench_day_of_year);
criterion_main!(benches);
```

Real output from `cargo bench`:

```text
day_of_year 2026-06-02  time:   [228.07 ns 287.79 ns 349.46 ns]
Found 5 outliers among 100 measurements (5.00%)
  5 (5.00%) high mild
```

The three numbers are the lower bound, the estimate, and the upper bound of a 95% confidence interval -- a *range*, not a single point, which is exactly the honesty a migration report needs. Criterion also detects outliers and, on a second run, will tell you whether performance changed since the last run.

### Measuring memory honestly

Throughput is half the story; the other half is **memory footprint**, which is often where a Rust migration delivers its quietest, biggest win. A `Vec<i64>` of 100,000 user IDs in Rust is a single packed allocation of exactly `100_000 * 8` bytes. The same array in JavaScript carries boxing and per-element overhead that can be several times larger.

You can prove the Rust side with a global allocator that counts live bytes:

```rust
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A wrapper allocator that tracks bytes currently allocated.
struct Counting;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn live_bytes() -> usize {
    ALLOCATED.load(Ordering::Relaxed)
}

fn main() {
    let before = live_bytes();
    let ids: Vec<i64> = (0..100_000).collect();
    let after = live_bytes();
    println!("live heap before : {before} bytes");
    println!("live heap after  : {after} bytes");
    println!(
        "delta            : {} bytes (~{} KiB)",
        after - before,
        (after - before) / 1024
    );
    println!("len              : {}", ids.len());
}
```

Real output:

```text
live heap before : 524 bytes
live heap after  : 800524 bytes
delta            : 800000 bytes (~781 KiB)
len              : 100000
```

Exactly 800,000 bytes -- `100_000 × 8` -- with no per-element overhead. That precision is the point: in Rust you can predict memory from the types. For the *whole-process* number that operations teams track, measure **RSS** (resident set size) under steady-state load: on Linux, `/proc/self/status` (`VmRSS`) or `ps -o rss`, on macOS `ps -o rss`. Report RSS at idle *and* under load, because Node.js's footprint grows with the GC heap while a Rust service's tends to stay flat. A migration that drops steady-state RSS from 512 MB to 40 MB per instance can cut your cloud bill more than any latency number, and it is a number you can defend with `ps`.

### Gating regressions in CI

Once you have a baseline p99, turn it into a budget the build enforces, so a future change cannot silently regress the tail:

```rust
/// Fail CI if the new p99 regressed by more than `tolerance` (e.g. 0.10 = 10%).
fn check_regression(baseline_p99: f64, new_p99: f64, tolerance: f64) -> Result<(), String> {
    let allowed = baseline_p99 * (1.0 + tolerance);
    if new_p99 > allowed {
        Err(format!(
            "p99 regression: {new_p99:.1} ms exceeds budget {allowed:.1} ms \
             (baseline {baseline_p99:.1} ms + {:.0}%)",
            tolerance * 100.0
        ))
    } else {
        Ok(())
    }
}

fn main() {
    match check_regression(120.0, 138.0, 0.10) {
        Ok(()) => println!("within budget"),
        Err(e) => println!("FAIL: {e}"),
    }
    match check_regression(120.0, 125.0, 0.10) {
        Ok(()) => println!("within budget"),
        Err(e) => println!("FAIL: {e}"),
    }
}
```

Real output:

```text
FAIL: p99 regression: 138.0 ms exceeds budget 132.0 ms (baseline 120.0 ms + 10%)
within budget
```

---

## Further Reading

- [HdrHistogram](https://hdrhistogram.org/) -- the canonical reference on latency measurement and coordinated omission.
- [Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/) -- statistical benchmarking for Rust.
- [`std::hint::black_box`](https://doc.rust-lang.org/std/hint/fn.black_box.html) -- preventing the optimizer from deleting your benchmark.
- [`oha`](https://github.com/hatoo/oha) and [`k6`](https://k6.io/) -- HTTP load generators with fixed-arrival-rate modes (avoid coordinated omission).
- Gil Tene, ["How NOT to Measure Latency"](https://www.youtube.com/watch?v=lJ8ydIuPFeU) -- the talk that popularized coordinated omission.

### Related sections in this guide

- [Incremental Migration](/29-migration-guide/00-incremental/) -- pick the hottest paths to port first; this page tells you how to *prove* a path is hot and how much it improved.
- [Porting a Node.js Service to Rust](/29-migration-guide/01-node-to-rust/) -- the worked Express-to-Axum port whose performance you are now measuring.
- [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/) -- keep payloads identical so your before/after comparison is apples-to-apples.
- [Data Migration Strategies](/29-migration-guide/03-data-migration/) -- dataset and index parity matters for a fair benchmark.
- [Common Migration Challenges](/29-migration-guide/05-common-challenges/) -- when the honest numbers say *don't* migrate.
- Section 13: [Benchmarking](/13-testing/08-benchmarking/) -- Criterion in depth.
- Section 21: [Performance](/21-performance/), especially [Benchmarking](/21-performance/02-benchmarking/), [Profiling](/21-performance/00-profiling/), and [When to Optimize](/21-performance/10-when-to-optimize/).
- Section 01: [Why Rust](/01-getting-started/00-why-rust/) -- the honest case for the performance characteristics measured here.
- Capstones: [Projects](/30-projects/) -- full projects you can benchmark end-to-end.

---

## Exercises

### Exercise 1: Expose the tail

**Difficulty:** Beginner

**Objective:** Internalize why the median can look healthy while the tail is on fire.

**Instructions:** Given a vector of request latencies in milliseconds, compute p50 and p99 using the nearest-rank method, then print the "tail amplification" ratio `p99 / p50`. Use this data:
`[10, 11, 9, 12, 10, 11, 10, 9, 13, 250, 10, 11, 10, 12, 11, 9, 10, 11, 10, 180]`.

<details>
<summary>Solution</summary>

```rust
fn percentile(sorted: &[u64], pct: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((pct / 100.0) * sorted.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

fn main() {
    let mut latencies: Vec<u64> = vec![
        10, 11, 9, 12, 10, 11, 10, 9, 13, 250,
        10, 11, 10, 12, 11, 9, 10, 11, 10, 180,
    ];
    latencies.sort_unstable();
    let p50 = percentile(&latencies, 50.0);
    let p99 = percentile(&latencies, 99.0);
    println!("p50 = {p50} ms, p99 = {p99} ms");
    println!("tail amplification (p99/p50) = {:.1}x", p99 as f64 / p50 as f64);
}
```

Real output:

```text
p50 = 10 ms, p99 = 250 ms
tail amplification (p99/p50) = 25.0x
```

A median of 10 ms looks great on a dashboard, but 1% of users wait 250 ms, 25x longer. This is the gap a Rust migration most often closes.

</details>

### Exercise 2: Aggregate without storing everything

**Difficulty:** Intermediate

**Objective:** Use a histogram to compute percentiles in bounded memory, the way a real load harness does.

**Instructions:** Add the `hdrhistogram` crate. Record 1,000 latencies where most are around 50 us but every 100th sample is a 5,000 us spike. Print p50, p99, and the max. Verify the median is unaffected by the spikes while the tail captures them.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
hdrhistogram = "7.5.4"
```

```rust
use hdrhistogram::Histogram;

fn main() {
    let mut hist = Histogram::<u64>::new(3).expect("create histogram");

    for i in 0..1_000u64 {
        let latency = if i % 100 == 99 { 5_000 } else { 50 };
        hist.record(latency).expect("value in range");
    }

    println!("count : {}", hist.len());
    println!("p50   : {} us", hist.value_at_quantile(0.50));
    println!("p99   : {} us", hist.value_at_quantile(0.99));
    println!("max   : {} us", hist.max());
}
```

Real output:

```text
count : 1000
p50   : 50 us
p99   : 50 us
max   : 5003 us
```

With spikes at only 1% frequency, even p99 sits at the fast value; you would need p99.9 (`value_at_quantile(0.999)`) to see them. That is the lesson: pick the percentile that matches how rare your bad events are, and always look at the max. The histogram used a fixed amount of memory regardless of the 1,000 samples (and would for a billion).

</details>

### Exercise 3: A regression gate with a memory budget

**Difficulty:** Advanced

**Objective:** Build a CI check that fails on *either* a latency regression or a memory regression, the way a production migration should guard its gains.

**Instructions:** Write a function `check_budget` that takes a baseline and a candidate measurement (each a struct with `p99_ms: f64` and `rss_mb: f64`) plus a tolerance fraction, and returns `Result<(), Vec<String>>` listing every metric that regressed past `baseline * (1 + tolerance)`. Test it with a candidate that improves p99 but regresses RSS.

<details>
<summary>Solution</summary>

```rust
#[derive(Clone, Copy)]
struct Measurement {
    p99_ms: f64,
    rss_mb: f64,
}

/// Returns Ok(()) if every metric is within budget, otherwise the list of
/// regressions. A metric regresses if it exceeds baseline * (1 + tolerance).
fn check_budget(
    baseline: Measurement,
    candidate: Measurement,
    tolerance: f64,
) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();

    let p99_budget = baseline.p99_ms * (1.0 + tolerance);
    if candidate.p99_ms > p99_budget {
        failures.push(format!(
            "p99 {:.1} ms exceeds budget {:.1} ms",
            candidate.p99_ms, p99_budget
        ));
    }

    let rss_budget = baseline.rss_mb * (1.0 + tolerance);
    if candidate.rss_mb > rss_budget {
        failures.push(format!(
            "rss {:.1} MB exceeds budget {:.1} MB",
            candidate.rss_mb, rss_budget
        ));
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

fn main() {
    let baseline = Measurement { p99_ms: 120.0, rss_mb: 64.0 };
    // Latency got better, but a leak pushed memory up 30%.
    let candidate = Measurement { p99_ms: 95.0, rss_mb: 84.0 };

    match check_budget(baseline, candidate, 0.10) {
        Ok(()) => println!("PASS: all metrics within budget"),
        Err(regressions) => {
            println!("FAIL: {} regression(s):", regressions.len());
            for r in regressions {
                println!("  - {r}");
            }
        }
    }
}
```

Real output:

```text
FAIL: 1 regression(s):
  - rss 84.0 MB exceeds budget 70.4 MB
```

The lesson: a migration can win on latency and *lose* on memory at the same time. A budget that watches both stops you from shipping a regression you were not looking for. Wire this into your CI step after the load test, reading the baseline from a committed file.

</details>

---

### Next: [Common Migration Challenges](/29-migration-guide/05-common-challenges/)
