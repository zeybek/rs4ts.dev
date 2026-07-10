---
title: "Benchmarking with Criterion"
description: "Use the Criterion crate for statistically sound Rust benchmarks: groups, input-size sweeps, and std::hint::black_box to stop the optimizer eliding work."
---

## Quick Overview

A **benchmark** answers a performance question — *which implementation is faster, and by how much?* — with statistics instead of a single stopwatch reading. In Rust the community-standard tool is the **criterion** crate, which runs your code thousands of times, models the timing as a distribution, and reports a confidence interval rather than one noisy number. This page covers the three pieces of criterion you reach for once you have written your first benchmark: **benchmark groups** (comparing related implementations side by side), **parameterized benchmarks** (sweeping input sizes to reveal scaling), and **defeating optimizer elision** with `std::hint::black_box` so you measure real work and not an empty loop.

> **Note:** This page assumes you already know how to wire criterion into a project (`benches/`, the `[[bench]]` table, `harness = false`, `criterion_group!`/`criterion_main!`). If any of that is unfamiliar, read [Section 13: Benchmarking](/13-testing/08-benchmarking/) first. It covers the setup and the basics of reading a single result. Here we build on that foundation and concentrate on the *advanced* mechanics in this page's title.

---

## TypeScript/JavaScript Example

The closest thing to criterion in the JavaScript world is **tinybench**, which also powers Vitest's experimental `bench` API. It mirrors criterion's idea (run many iterations, report a distribution) but stops well short of criterion's statistics and regression tracking. Here is a Vitest suite comparing two ways to sum the integers in a comma-separated line.

```typescript
// sum-csv.ts
export function sumCsvSplit(line: string): number {
  return line
    .split(",")
    .map((s) => parseInt(s.trim(), 10))
    .filter((n) => !Number.isNaN(n))
    .reduce((acc, n) => acc + n, 0);
}

export function sumCsvScan(line: string): number {
  let total = 0;
  let current = 0;
  let inNumber = false;
  let negative = false;
  for (let i = 0; i < line.length; i++) {
    const code = line.charCodeAt(i);
    if (code >= 48 && code <= 57) {
      current = current * 10 + (code - 48);
      inNumber = true;
    } else if (code === 45 && !inNumber) {
      negative = true;
    } else if (code === 44) {
      total += negative ? -current : current;
      current = 0;
      inNumber = false;
      negative = false;
    }
  }
  return total + (negative ? -current : current);
}
```

```typescript
// sum-csv.bench.ts
import { bench, describe } from "vitest";
import { sumCsvSplit, sumCsvScan } from "./sum-csv";

const line = Array.from({ length: 1000 }, (_, i) => i + 1).join(",");

describe("sum-csv", () => {
  bench("split", () => {
    sumCsvSplit(line);
  });
  bench("scan", () => {
    sumCsvScan(line);
  });
});
```

A real `tinybench` run of the `split` variant on Node v22 reports throughput and a relative margin of error:

```text
┌─────────┬──────────────┬───────────────────┬──────────────────┬────────────────────────┬─────────┐
│ (index) │ Task name    │ Latency avg (ns)  │ Latency med (ns) │ Throughput avg (ops/s) │ Samples │
├─────────┼──────────────┼───────────────────┼──────────────────┼────────────────────────┼─────────┤
│ 0       │ 'sumCsvIter' │ '110213 ± 13.97%' │ '77125 ± 2875.0' │ '12316 ± 0.58%'        │ 4537    │
└─────────┴──────────────┴───────────────────┴──────────────────┴────────────────────────┴─────────┘
```

Two things to take from this. First, the **±13.97%** margin: micro-benchmarks on a multitasking OS are inherently noisy, and a good tool admits it. Second, and this is the key contrast with Rust, in JavaScript you almost never have to fight the optimizer to keep your code from being deleted. V8's JIT does not constant-fold `sumCsvSplit(line)` away just because the result is discarded, so a discarded return value is harmless here. In Rust it is not, as you will see.

> **Note:** Vitest prints `Benchmarking is an experimental feature` on every run. The JavaScript ecosystem still treats benchmarking as a bolt-on; criterion has been the stable Rust default for years.

---

## Rust Equivalent

The same two implementations in Rust, with the three advanced criterion features layered on. The library code:

```rust
// src/lib.rs

/// Sum integers from a comma-separated line, iterator style.
pub fn sum_csv_iter(line: &str) -> i64 {
    line.split(',')
        .filter_map(|tok| tok.trim().parse::<i64>().ok())
        .sum()
}

/// Sum integers, byte-scanning style: one pass, no intermediate `&str`s.
pub fn sum_csv_scan(line: &str) -> i64 {
    let mut total: i64 = 0;
    let mut current: i64 = 0;
    let mut in_number = false;
    let mut negative = false;
    for &byte in line.as_bytes() {
        match byte {
            b'0'..=b'9' => {
                current = current * 10 + (byte - b'0') as i64;
                in_number = true;
            }
            b'-' if !in_number => negative = true,
            b',' => {
                total += if negative { -current } else { current };
                current = 0;
                in_number = false;
                negative = false;
            }
            _ => {} // skip whitespace and anything else
        }
    }
    total + if negative { -current } else { current }
}
```

`Cargo.toml`. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition, and `cargo new` selects that edition automatically. Add criterion with `cargo add --dev criterion --features html_reports`:

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.8.2", features = ["html_reports"] }

[[bench]]
name = "csv"
harness = false   # mandatory: criterion supplies its own main()
```

The benchmark file uses a **group** to compare the two strategies and `Throughput` so criterion reports bytes per second:

```rust
// benches/csv.rs
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;

use perf::{sum_csv_iter, sum_csv_scan};

fn make_line(n: usize) -> String {
    (1..=n).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

fn bench_strategies(c: &mut Criterion) {
    let line = make_line(1_000);
    let mut group = c.benchmark_group("sum_csv");
    group.throughput(Throughput::Bytes(line.len() as u64));

    group.bench_function("iter", |b| b.iter(|| sum_csv_iter(black_box(&line))));
    group.bench_function("scan", |b| b.iter(|| sum_csv_scan(black_box(&line))));

    group.finish();
}

criterion_group!(benches, bench_strategies);
criterion_main!(benches);
```

A real `cargo bench` run:

```text
sum_csv/iter            time:   [9.6848 µs 9.9924 µs 10.378 µs]
                        thrpt:  [357.65 MiB/s 371.45 MiB/s 383.25 MiB/s]
Found 8 outliers among 50 measurements (16.00%)
  2 (4.00%) high mild
  6 (12.00%) high severe

sum_csv/scan            time:   [10.188 µs 11.653 µs 13.121 µs]
                        thrpt:  [282.89 MiB/s 318.53 MiB/s 364.31 MiB/s]
Found 3 outliers among 50 measurements (6.00%)
  1 (2.00%) high mild
  2 (4.00%) high severe
```

The Rust version runs in ~10 microseconds versus JavaScript's ~110 microseconds for the same 1000-field line, roughly a 10x gap, because both are running as optimized native code rather than through an interpreter/JIT. Notice these two intervals *overlap*: on a busy machine, at this input size, criterion is honestly telling you it cannot distinguish the two implementations. The parameterized benchmark below pulls them apart.

> **Note:** `cargo bench` always builds in **release** mode (optimizations on); `cargo test` defaults to debug. Benchmarking a debug build measures the wrong thing entirely.

---

## Detailed Explanation

### Benchmark groups: `c.benchmark_group(...)`

`c.bench_function("name", ...)` registers one standalone benchmark. A **group**, created with `c.benchmark_group("group_name")`, bundles several related benchmarks under a shared umbrella so criterion can:

- name them hierarchically (`sum_csv/iter`, `sum_csv/scan`) and place them next to each other in the report and HTML plots;
- share group-level settings — `throughput`, `sample_size`, `measurement_time`, `sampling_mode` — across every benchmark in the group;
- generate a **comparison violin plot** that overlays all members so you can eyeball which is faster.

You must call `group.finish()` (or let the `BenchmarkGroup` drop) before the group's results are written. Forgetting `finish()` is a common cause of "my group never showed up."

This is the structural analogue of Vitest's `describe("sum-csv", () => { bench(...); bench(...); })`, but criterion does more with the grouping. It knows the members are comparable and produces relative plots, whereas Vitest's `describe` is purely organizational.

### `Throughput`: reporting work per second

```rust
group.throughput(Throughput::Bytes(line.len() as u64));
```

By default criterion reports time *per iteration*. `throughput` adds a second line — `thrpt:` — that divides the work done by the time taken. Use `Throughput::Bytes(n)` for byte-oriented work (parsers, compression, hashing) and `Throughput::Elements(n)` for item-oriented work (sorting, mapping over a collection). The throughput line is often more intuitive than raw nanoseconds: "371 MiB/s" tells a parser story that "9.99 µs" does not.

### `bench_function` vs `bench_with_input`

Inside a group you have two ways to register a benchmark:

- `group.bench_function(id, |b| b.iter(closure))`: when the input is captured by the closure (as above; `line` is captured by reference).
- `group.bench_with_input(id, &input, |b, input| b.iter(...))`: when you want criterion to *own* the relationship between the benchmark ID and its input, which is exactly what you need to sweep sizes (next section).

The `id` is either a string (for `bench_function`) or a `BenchmarkId` (for parameterized benches), which we cover next.

### The `Bencher` and `b.iter`

In every form, the innermost piece is `b.iter(|| ...)`. The `Bencher` (`b`) is handed to you by the harness; `b.iter` is the part criterion *times*. **Criterion decides how many times to call your closure.** It warms up, then collects ~100 samples, automatically scaling the iteration count to the code's speed. You never write the loop. This is the deep difference from a hand-rolled `Date.now()` loop in JavaScript, where you pick an iteration count and hope it is enough; criterion derives it from the measured speed.

---

## Parameterized benchmarks: revealing how code scales

A single input size hides scaling behavior. The two CSV functions looked indistinguishable at 1000 fields, but is that true at 16 fields? At 4096? A parameterized benchmark sweeps a range of sizes and reports each, so you can see the *shape* of the curve, not one point on it.

```rust
// benches/csv_scaling.rs
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

use perf::{sum_csv_iter, sum_csv_scan};

fn make_line(n: usize) -> String {
    (1..=n).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

fn bench_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("sum_csv_scaling");
    for size in [16usize, 256, 4_096] {
        let line = make_line(size);
        group.throughput(Throughput::Bytes(line.len() as u64));

        group.bench_with_input(BenchmarkId::new("iter", size), &line, |b, line| {
            b.iter(|| sum_csv_iter(black_box(line)))
        });
        group.bench_with_input(BenchmarkId::new("scan", size), &line, |b, line| {
            b.iter(|| sum_csv_scan(black_box(line)))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_sizes);
criterion_main!(benches);
```

`BenchmarkId::new("iter", size)` builds an identifier of the form `iter/16`, `iter/256`, `iter/4096`. (If a single implementation is being swept, `BenchmarkId::from_parameter(size)` produces just `16`, `256`, ... without a function-name prefix.) Because the size is part of the ID, criterion groups all the `iter/*` points into one **line on a log-log plot** in the HTML report, so you can literally see O(n) vs O(n²).

Trimmed real output (`cargo bench --bench csv_scaling`, longer measurement time for stability):

```text
sum_csv_scaling/iter/256
                        time:   [5.0857 µs 6.7211 µs 8.7131 µs]
                        thrpt:  [100.15 MiB/s 129.83 MiB/s 171.58 MiB/s]
sum_csv_scaling/scan/256
                        time:   [1.9667 µs 2.3616 µs 2.9064 µs]
                        thrpt:  [300.24 MiB/s 369.49 MiB/s 443.70 MiB/s]

sum_csv_scaling/iter/4096
                        time:   [43.306 µs 46.076 µs 49.798 µs]
                        thrpt:  [370.99 MiB/s 400.96 MiB/s 426.61 MiB/s]
sum_csv_scaling/scan/4096
                        time:   [37.487 µs 38.014 µs 38.717 µs]
                        thrpt:  [477.17 MiB/s 485.99 MiB/s 492.83 MiB/s]
```

Now the picture is clear that was invisible at a single size: the byte scanner's throughput climbs from ~370 MiB/s to ~486 MiB/s as the input grows (per-call fixed overhead amortizes), and it consistently beats the iterator version, most decisively at 256 fields, where their intervals do not overlap at all (`1.97–2.91 µs` vs `5.09–8.71 µs`). Sweeping sizes turned an ambiguous tie into a defensible conclusion.

> **Tip:** Choose sizes that span the regimes you care about, ideally geometrically (16, 256, 4096, each ~16x the last). A geometric sweep makes the log-log plot reveal the asymptotic complexity at a glance.

---

## Defeating optimizer elision with `black_box`

This is the most Rust-specific part of benchmarking and has no real JavaScript counterpart. Rust compiles ahead-of-time through LLVM, whose optimizer is aggressive. If it can prove that a computation's result is unused, or that an input is a compile-time constant, it may **delete the computation entirely** (dead-code elimination) or **compute the answer at compile time** (constant folding). Your benchmark would then time *nothing*.

Consider a function whose result the optimizer can derive in closed form. Summing `0..n` is just Gauss's formula `n*(n-1)/2`:

```rust
// benches/blackbox.rs
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

// Sum 0..n with an explicit loop. The optimizer knows the closed form,
// so with a *constant* bound it can replace the whole loop with one multiply.
fn sum_to(n: u64) -> u64 {
    let mut total = 0u64;
    for i in 0..n {
        total += i;
    }
    total
}

fn bench_blackbox(c: &mut Criterion) {
    // BROKEN: `1_000_000` is a visible constant, so the optimizer folds the
    // entire million-iteration loop into a single multiply at compile time.
    c.bench_function("sum_to 1e6 (no black_box)", |b| {
        b.iter(|| sum_to(1_000_000))
    });
    // CORRECT: black_box hides the bound from the optimizer, so the loop runs.
    c.bench_function("sum_to 1e6 (black_box)", |b| {
        b.iter(|| sum_to(black_box(1_000_000)))
    });
}

criterion_group!(benches, bench_blackbox);
criterion_main!(benches);
```

Real `cargo bench --bench blackbox` output:

```text
sum_to 1e6 (no black_box)
                        time:   [330.82 ps 335.64 ps 341.99 ps]
sum_to 1e6 (black_box)  time:   [657.68 ps 860.67 ps 1.1648 ns]
```

Read that first line again: a **one-million-iteration loop apparently completing in 335 picoseconds**. A single CPU cycle on a ~3 GHz core is roughly 330 picoseconds, so this claims the loop ran a million additions in about *one clock cycle*, which is physically impossible. That is the unmistakable signature of constant folding: the optimizer replaced the loop with `1_000_000 * 999_999 / 2` at compile time, and the benchmark is timing a literal. The `black_box` version is ~2x slower because the bound is now opaque, but even it is sub-nanosecond: LLVM still auto-vectorizes the real loop into SIMD additions. The takeaway is not the absolute numbers; it is that **without `black_box` you were not benchmarking the loop at all.**

### How `black_box` works

`std::hint::black_box(x)` is a compiler hint meaning "treat this value as if it were used in some opaque way you cannot reason about." It returns its argument unchanged at runtime (zero cost) but acts as an optimization barrier:

- **Wrap the input** to stop the optimizer from treating it as a constant: `sum_to(black_box(1_000_000))`.
- **The result** of the closure passed to `b.iter` is automatically black-boxed by criterion, so a returned value is protected. The danger is when you *discard* the result inside the closure — then you must black-box it yourself, or the whole call may be eliminated.

A rule of thumb: black-box every input that would otherwise be a literal, and make sure the result either escapes the closure (criterion handles it) or is wrapped in `black_box` if you keep it local.

> **Warning:** `black_box` lives in `std::hint`. Criterion 0.8 still re-exports `criterion::black_box`, but it is **deprecated**. Importing it produces a real compiler warning: `use of deprecated function 'criterion::black_box': use 'std::hint::black_box()' instead`. Always `use std::hint::black_box;`.

---

## Key Differences

| Concept                  | JavaScript (Vitest `bench` / tinybench)       | Rust (criterion)                                                |
| ------------------------ | --------------------------------------------- | --------------------------------------------------------------- |
| Grouping                 | `describe(...)` — organizational only         | `benchmark_group(...)` — shared settings + comparison plots     |
| Parameter sweeps         | Manual (loop and add benches yourself)        | `bench_with_input` + `BenchmarkId` → one line on a log-log plot |
| Throughput               | `ops/s` reported by default                   | Opt-in `Throughput::Bytes`/`Elements` → `thrpt:` line           |
| Iteration count          | Tool picks, or you set `time`/`iterations`    | Criterion auto-scales to hit a target sample window             |
| Defeating the compiler   | Rarely needed (JIT won't fold across calls)   | **Essential**: wrap inputs/results in `std::hint::black_box`    |
| Result reporting         | latency + throughput + `± rme`                | confidence interval `[lower estimate upper]` + outlier report   |
| Discarded return value   | Harmless                                      | May get the whole call **deleted** by dead-code elimination     |

Two points deserve emphasis.

**Criterion reports a distribution, not a number.** Each `time: [9.6848 µs 9.9924 µs 10.378 µs]` is the 95% confidence interval for the *mean* per-iteration time: lower bound, best estimate, upper bound. The wider the interval, the noisier the run and the less you should trust a small difference. Overlapping intervals (as in the first `iter`/`scan` example) mean "no detectable difference," which is itself a useful, honest answer.

**Optimizer elision is a Rust-only trap.** In JavaScript, V8 will not constant-fold a function call whose result you ignore, so benchmarks rarely lie about doing nothing. In Rust, an ahead-of-time optimizer absolutely will, and a benchmark reporting picoseconds for obviously expensive work is the tell. `black_box` is the standard defense.

---

## Common Pitfalls

### Pitfall 1: A million-iteration loop "runs" in picoseconds (missing `black_box`)

The flagship trap, shown above. If a benchmark reports a time that is physically impossible for the amount of work (sub-nanosecond for a loop, or `0.0000 ns` flat), the optimizer almost certainly folded or deleted the code. Wrap the inputs in `black_box` and ensure the result is not silently discarded. Suspect a missing `black_box` *before* believing your code is infinitely fast.

### Pitfall 2: Discarding the result inside the closure

```rust
use std::hint::black_box;
use criterion::Criterion;

fn expensive(n: u64) -> u64 { (0..n).map(|i| i * i).sum() }

fn demo(c: &mut Criterion) {
    // result discarded: the optimizer may delete the call.
    c.bench_function("bad", |b| b.iter(|| {
        expensive(black_box(1000));   // semicolon throws the value away
    }));

    // result returned from the closure → criterion black-boxes it.
    c.bench_function("good", |b| b.iter(|| expensive(black_box(1000))));
}
```

Black-boxing the *input* is not always enough; if you also throw the *output* away, dead-code elimination can still strike. Prefer returning the value from the closure (no trailing semicolon) so criterion protects it, or wrap it: `black_box(expensive(black_box(1000)));`.

### Pitfall 3: Forgetting `harness = false`

If you omit `harness = false` from the `[[bench]]` table, cargo links the built-in libtest harness, which ignores criterion's `main()`, finds no `#[bench]` functions, and reports:

```text
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

No error, no benchmark — it silently does nothing. If `cargo bench` finishes instantly with `running 0 tests`, this one missing line is almost always the cause.

### Pitfall 4: Doing setup inside the timed closure

Everything inside `b.iter(|| ...)` is measured on *every* iteration. Build inputs **outside** the closure:

```rust
// measures String allocation every iteration, not the function
b.iter(|| {
    let line = make_line(1_000);     // allocation timed millions of times!
    sum_csv_scan(black_box(&line))
});

// build once, measure only the call
let line = make_line(1_000);
b.iter(|| sum_csv_scan(black_box(&line)));
```

For code that *consumes or mutates* its input each iteration, use `iter_batched` (see Best Practices) so the fresh-input setup is excluded from the timer.

### Pitfall 5: Calling `group.finish()` is not optional

A `BenchmarkGroup`'s results are flushed when you call `group.finish()` (or when it drops). Returning early from the function, or shadowing the group, can leave results unwritten. Always end a group with `group.finish();`.

### Pitfall 6: Comparing intervals that overlap

When two confidence intervals overlap, as `sum_csv/iter` and `sum_csv/scan` did at 1000 fields, you do **not** have evidence that one is faster. Resist the urge to read the middle estimates as a verdict. Either gather more samples / a longer measurement time, sweep input sizes to find a regime where they separate, or accept "no measurable difference" as the result.

---

## Best Practices

### Use a group to compare; use `bench_with_input` to scale

Reach for a `benchmark_group` whenever you have two or more comparable implementations, and add `Throughput` so the report speaks in MiB/s or elements/s. Reach for `bench_with_input` + `BenchmarkId` whenever the question is "how does this scale?" A single size is rarely enough to choose an algorithm.

### Use `iter_batched` when each iteration needs fresh, consumed input

If the code under test sorts in place, drains a queue, or otherwise mutates its argument, build a fresh copy per batch so you do not measure an already-sorted vector on the second iteration. The setup closure runs *outside* the timer:

```rust
// benches/sort.rs
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;

fn bench_sort(c: &mut Criterion) {
    let mut group = c.benchmark_group("sort");
    group.bench_function("sort_unstable 10k descending", |b| {
        b.iter_batched(
            || (0..10_000u32).rev().collect::<Vec<_>>(), // setup: NOT timed
            |mut data| {
                data.sort_unstable();
                black_box(data); // keep the sorted result alive
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(benches, bench_sort);
criterion_main!(benches);
```

Real output: `sort/sort_unstable 10k descending  time: [6.4599 µs 7.2063 µs 8.2107 µs]`. The `BatchSize` hint lets criterion decide how many fresh inputs to prepare per measurement; `SmallInput` is the right default for cheap-to-build inputs, `LargeInput` for expensive ones.

### Tune the run for the signal you need

Everything after `--` goes to criterion, not cargo:

- `cargo bench -- --sample-size 50` — fewer samples for a quick, rough pass.
- `cargo bench -- --measurement-time 10`: collect over 10 s for tighter intervals on noisy machines.
- `cargo bench -- sum_csv_scaling/scan`: run only benchmarks whose ID contains the filter.
- `cargo bench -- --save-baseline main` then `--baseline main`: compare a PR against a saved baseline (the CI regression-gating workflow; covered in [Section 13: Benchmarking](/13-testing/08-benchmarking/)).

### Benchmark correct code, on an idle machine

A fast wrong answer is worthless: keep [unit tests](/13-testing/00-unit-tests/) green first; criterion does not check correctness. And a large fraction of "high severe" outliers means the environment was busy: close other programs, disable CPU turbo if you can, and prefer the statistically-aware `change:` verdict over eyeballing raw nanoseconds. Measuring *which function is hot* before micro-benchmarking is the job of [profiling](/21-performance/00-profiling/) and [flame graphs](/21-performance/01-flamegraph/) — benchmark the hot spot they find, not a function you guessed at.

---

## Real-World Example

A production-flavored decision: you maintain a hashing utility and must choose between the standard library's default `SipHash`-based hasher (DoS-resistant, the default for `HashMap`) and the faster, non-cryptographic `fxhash` for an internal cache where untrusted input is not a concern. The right tool for this decision is a parameterized criterion group over realistic key sizes.

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.8.2", features = ["html_reports"] }
fxhash = "0.2"

[[bench]]
name = "hashing"
harness = false
```

```rust
// benches/hashing.rs
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hash::{BuildHasher, Hasher};
use std::hint::black_box;

use std::collections::hash_map::RandomState; // std's default (SipHash)
use fxhash::FxBuildHasher; // fast, non-cryptographic

fn make_key(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// Hash one key with the given BuildHasher and return the digest so the
/// optimizer cannot delete the work.
fn hash_one<S: BuildHasher>(state: &S, key: &[u8]) -> u64 {
    let mut hasher = state.build_hasher();
    hasher.write(key);
    hasher.finish()
}

fn bench_hashing(c: &mut Criterion) {
    let sip = RandomState::new();
    let fx = FxBuildHasher::default();

    let mut group = c.benchmark_group("hash_key");
    for len in [8usize, 64, 1_024] {
        let key = make_key(len);
        group.throughput(Throughput::Bytes(len as u64));

        group.bench_with_input(BenchmarkId::new("siphash", len), &key, |b, key| {
            b.iter(|| hash_one(&sip, black_box(key)))
        });
        group.bench_with_input(BenchmarkId::new("fxhash", len), &key, |b, key| {
            b.iter(|| hash_one(&fx, black_box(key)))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_hashing);
criterion_main!(benches);
```

This single file exercises every advanced feature from this page: a **group** (`hash_key`) so SipHash and FxHash appear side by side; **parameterized** benches sweeping 8, 64, and 1024-byte keys to see how the gap changes with key length; **`Throughput::Bytes`** so the report reads in MiB/s; and **`black_box`** on every key plus a returned digest so neither hash is optimized away. Run it with `cargo bench --bench hashing`, read the per-size confidence intervals, and only adopt FxHash where its intervals clearly beat SipHash *and* the security trade-off is acceptable for that cache. That measure-then-decide loop, never "FxHash is faster, obviously," is the entire discipline.

> **Tip:** This pattern (criterion group + size sweep + `black_box`) generalizes to any "which implementation should I ship?" question: serializers, allocators, string-search algorithms, compression levels. See [Optimization Techniques](/21-performance/03-optimization/) for the changes you might benchmark, and [When to Optimize](/21-performance/10-when-to-optimize/) for deciding whether the question is even worth asking.

---

## Further Reading

- [Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/index.html): the authoritative manual: groups, throughput, `BenchmarkId`, baselines, and plotting.
- [`criterion` on docs.rs](https://docs.rs/criterion/latest/criterion/): current API reference for `Criterion`, `BenchmarkGroup`, `Throughput`, and `BatchSize`.
- [`std::hint::black_box`](https://doc.rust-lang.org/std/hint/fn.black_box.html): the optimization barrier and why it defeats constant folding and dead-code elimination.
- [`cargo bench` reference](https://doc.rust-lang.org/cargo/commands/cargo-bench.html): the cargo side of running benchmarks and passing arguments after `--`.
- [`divan`](https://crates.io/crates/divan) — a stable, attribute-style alternative to criterion if you prefer a lighter API.
- Sibling topics in this section:
  - [Profiling](/21-performance/00-profiling/) and [Flame Graphs](/21-performance/01-flamegraph/): find the hot spot *first*, then benchmark it.
  - [Optimization Techniques](/21-performance/03-optimization/): the kinds of changes worth benchmarking (clones, allocations, `&str` vs `String`).
  - [Zero-Cost Abstractions](/21-performance/06-zero-cost/) — evidence that iterators compile to the same code as hand loops.
  - [Memory Layout](/21-performance/04-memory-layout/) and [Cache Efficiency](/21-performance/05-cache-efficiency/): performance properties a benchmark will surface.
  - [When to Optimize](/21-performance/10-when-to-optimize/): measure first, and avoid premature micro-benchmarking.
  - [Performance vs Node.js](/21-performance/09-comparison/) — the honest CPU/memory comparison this page's 10x number hints at.
- Foundations and related sections:
  - [Section 13: Benchmarking](/13-testing/08-benchmarking/): criterion setup and reading a single result (read this first).
  - [Cargo Basics](/01-getting-started/03-cargo-basics/): `cargo bench`, profiles, and `dev-dependencies`.
  - [Basics: Types](/02-basics/01-types/): the `i64`/`u64`/`u8` integer types these benchmarks use.
  - [Common Patterns](/22-common-patterns/) — idioms (builders, iterators) you will frequently benchmark.

---

## Exercises

### Exercise 1: Group two implementations with throughput

**Difficulty:** Easy

**Objective:** Build a benchmark group that compares two functions and reports throughput.

**Instructions:** Given `pub fn reverse_collect(s: &str) -> String` (which does `s.chars().rev().collect()`) and `pub fn reverse_bytes(s: &str) -> String` (which reverses `s.bytes()` into a `Vec<u8>` then `String::from_utf8` — valid only for ASCII), write `benches/reverse.rs` that benchmarks both on a 1000-character ASCII string inside a single `benchmark_group("reverse")`, with `Throughput::Bytes`. Remember `harness = false`, `black_box` the input, and `group.finish()`.

```rust
// src/lib.rs
pub fn reverse_collect(s: &str) -> String {
    s.chars().rev().collect()
}

pub fn reverse_bytes(s: &str) -> String {
    let bytes: Vec<u8> = s.bytes().rev().collect();
    String::from_utf8(bytes).unwrap()
}

// TODO: write benches/reverse.rs and the Cargo.toml entries
```

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.8.2", features = ["html_reports"] }

[[bench]]
name = "reverse"
harness = false
```

```rust
// benches/reverse.rs
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;

use my_crate::{reverse_bytes, reverse_collect}; // replace `my_crate`

fn bench_reverse(c: &mut Criterion) {
    let s = "a".repeat(1_000);
    let mut group = c.benchmark_group("reverse");
    group.throughput(Throughput::Bytes(s.len() as u64));

    group.bench_function("collect", |b| b.iter(|| reverse_collect(black_box(&s))));
    group.bench_function("bytes", |b| b.iter(|| reverse_bytes(black_box(&s))));

    group.finish();
}

criterion_group!(benches, bench_reverse);
criterion_main!(benches);
```

`cargo bench` prints two intervals under `reverse/collect` and `reverse/bytes`, each with a `thrpt:` line. The byte version is usually faster on pure ASCII because it skips UTF-8 decoding — but only the measured intervals (do they overlap?) justify saying so.

</details>

### Exercise 2: Sweep input sizes to find where two algorithms diverge

**Difficulty:** Medium

**Objective:** Use `bench_with_input` and `BenchmarkId` to compare linear search against the standard library's binary search across sizes.

**Instructions:** Write `pub fn find_linear(data: &[i32], needle: i32) -> bool` (using `.contains`) and `pub fn find_binary(data: &[i32], needle: i32) -> bool` (using `.binary_search(..).is_ok()`, valid because the data is sorted). Benchmark both searching for an *absent* value (worst case) over sizes `[64, 4_096, 262_144]` in one group. `black_box` both the slice and the needle.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
pub fn find_linear(data: &[i32], needle: i32) -> bool {
    data.contains(&needle)
}

pub fn find_binary(data: &[i32], needle: i32) -> bool {
    data.binary_search(&needle).is_ok()
}
```

```rust
// benches/search.rs
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

use my_crate::{find_binary, find_linear};

fn bench_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_absent");
    for size in [64i32, 4_096, 262_144] {
        let data: Vec<i32> = (0..size).collect(); // already sorted
        let needle = size; // absent: forces worst case
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(BenchmarkId::new("linear", size), &data, |b, data| {
            b.iter(|| find_linear(black_box(data), black_box(needle)))
        });
        group.bench_with_input(BenchmarkId::new("binary", size), &data, |b, data| {
            b.iter(|| find_binary(black_box(data), black_box(needle)))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
```

At `size = 64` linear search may *win* (no branch mispredictions, cache-friendly, tiny constant factor); by `size = 262_144` binary search's O(log n) crushes linear's O(n). The size sweep makes the crossover visible — which is exactly why a single size would mislead you into a wrong default.

</details>

### Exercise 3: Prove the optimizer is eliding your benchmark

**Difficulty:** Medium

**Objective:** Reproduce optimizer elision, recognize its tell, and fix it with `black_box`.

**Instructions:** Write `pub fn sum_squares(n: u64) -> u64` that returns `(1..=n).map(|i| i * i).sum()`. Add two benchmarks to one file: a "broken" one that calls `sum_squares(10_000)` and discards the result with a semicolon, and a "fixed" one that `black_box`es the input and returns the result. Run them and explain why the broken one reports an impossibly small time.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
pub fn sum_squares(n: u64) -> u64 {
    (1..=n).map(|i| i * i).sum()
}
```

```rust
// benches/elision.rs
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use my_crate::sum_squares;

fn bench_elision(c: &mut Criterion) {
    // BROKEN: constant input + discarded result → folded/eliminated.
    c.bench_function("sum_squares (elided)", |b| {
        b.iter(|| {
            sum_squares(10_000); // semicolon discards the value
        })
    });
    // FIXED: opaque input, result returned so criterion black-boxes it.
    c.bench_function("sum_squares (black_box)", |b| {
        b.iter(|| sum_squares(black_box(10_000)))
    });
}

criterion_group!(benches, bench_elision);
criterion_main!(benches);
```

The "elided" benchmark reports a time independent of `n` (sub-nanosecond, near the empty-loop floor), because the optimizer computed `sum_squares(10_000)` at compile time and then deleted the unused result — you are timing nothing. The "black_box" benchmark reports a time that *grows with `n`*, because the opaque input forces the loop to actually execute. The tell: when a benchmark's time does not change as you scale the input, suspect elision and add `black_box`.

</details>
