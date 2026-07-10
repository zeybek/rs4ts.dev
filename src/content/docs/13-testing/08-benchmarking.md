---
title: "Benchmarking"
description: "Benchmark Rust with criterion: confidence intervals, baselines, and black_box to defeat the optimizer. Compared to Vitest bench; #[bench] needs nightly."
---

## Quick Overview

A **benchmark** measures how *fast* code runs, as opposed to a [unit test](/13-testing/00-unit-tests/), which checks whether it is *correct*. Coming from JavaScript you have probably reached for `console.time`, `tinybench`, or Vitest's experimental `bench` API; in Rust the community-standard tool is the **criterion** crate, which runs your code thousands of times, applies statistics, detects regressions against a saved baseline, and (optionally) renders HTML plots. This page covers writing and interpreting criterion 0.8 benchmarks on stable Rust, and why the built-in `#[bench]` attribute is *not* an option unless you switch to nightly.

---

## TypeScript/JavaScript Example

Vitest ships an experimental `bench` API (built on `tinybench`) that mirrors its `describe`/`it` structure. Here are two implementations of Fibonacci — a naive recursive one and a linear iterative one — with a benchmark suite.

```typescript
// fib.ts
export function fibRecursive(n: number): number {
  return n < 2 ? n : fibRecursive(n - 1) + fibRecursive(n - 2);
}

export function fibIterative(n: number): number {
  let a = 0,
    b = 1;
  for (let i = 0; i < n; i++) [a, b] = [b, a + b];
  return a;
}
```

```typescript
// fib.bench.ts
import { bench, describe } from "vitest";
import { fibRecursive, fibIterative } from "./fib";

describe("fib", () => {
  bench("recursive", () => {
    fibRecursive(20);
  });
  bench("iterative", () => {
    fibIterative(20);
  });
});
```

Run it with `npx vitest bench --run`:

```text
 fib.bench.ts > fib 5186ms
     name                  hz     min      max    mean     p75     p99    p995     p999      rme  samples
   · recursive       9,615.15  0.0611  30.5918  0.1040  0.0654  0.3849  1.2592  10.5596  ±18.66%     4808
   · iterative  10,822,983.33  0.0000  49.2998  0.0001  0.0001  0.0002  0.0002   0.0003  ±22.48%  5411492

 BENCH  Summary

  iterative - fib.bench.ts > fib
    1125.62x faster than recursive
```

Vitest reports throughput (`hz`, operations per second), percentiles, and a relative-margin-of-error (`rme`). Notice the **±18.66%** and **±22.48%** error bars: micro-benchmarks on a multitasking OS are inherently noisy, and a good tool tells you *how* noisy. Criterion takes the same idea much further.

> **Note:** Vitest prints `Benchmarking is an experimental feature`. The JavaScript ecosystem still treats benchmarking as a bolt-on. Criterion has been the de-facto Rust standard for years and is stable.

---

## Rust Equivalent

Criterion benchmarks do **not** live in `src/` next to unit tests. They go in a top-level `benches/` directory (a sibling of `src/` and `tests/`), each file is a separate benchmark target, and you must register it in `Cargo.toml` so cargo knows to use criterion's harness instead of the built-in one.

First, the library code under test. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically.

```rust
// src/lib.rs

/// Naive recursive Fibonacci — exponential time.
pub fn fib_recursive(n: u64) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fib_recursive(n - 1) + fib_recursive(n - 2),
    }
}

/// Iterative Fibonacci — linear time.
pub fn fib_iterative(n: u64) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        (a, b) = (b, a + b);
    }
    a
}
```

Add criterion as a development dependency:

```bash
cargo add --dev criterion --features html_reports
```

That writes the following to `Cargo.toml`. The `[[bench]]` table and `harness = false` are **mandatory** (more on why in Common Pitfalls):

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.8.2", features = ["html_reports"] }

[[bench]]
name = "fib"        # matches benches/fib.rs
harness = false     # disable libtest's harness; criterion provides its own main()
```

Now the benchmark itself:

```rust
// benches/fib.rs
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use perf::{fib_iterative, fib_recursive};

fn bench_fib(c: &mut Criterion) {
    c.bench_function("fib_recursive 20", |b| {
        b.iter(|| fib_recursive(black_box(20)))
    });
    c.bench_function("fib_iterative 20", |b| {
        b.iter(|| fib_iterative(black_box(20)))
    });
}

criterion_group!(benches, bench_fib);
criterion_main!(benches);
```

Run it with `cargo bench`:

```text
fib_recursive 20        time:   [27.921 µs 33.765 µs 42.311 µs]
Found 17 outliers among 100 measurements (17.00%)
  7 (7.00%) high mild
  10 (10.00%) high severe

fib_iterative 20        time:   [7.3826 ns 7.9508 ns 8.8231 ns]
Found 13 outliers among 100 measurements (13.00%)
  1 (1.00%) high mild
  12 (12.00%) high severe
```

The iterative version runs in *nanoseconds*; the recursive one in *microseconds* — roughly a 4000x gap for `n = 20`, far more dramatic than the JavaScript numbers because both are running as optimized native code with no interpreter overhead to mask the algorithmic difference.

> **Note:** `cargo bench` always compiles in **release mode** (optimizations on). This is the opposite of `cargo test`, which defaults to debug. Benchmarking a debug build measures the wrong thing entirely.

---

## Detailed Explanation

Let's walk through the moving parts and contrast each with the Vitest version.

### The `benches/` directory and the `[[bench]]` target

Vitest discovers `*.bench.ts` files by glob. Cargo is explicit: each file in `benches/` is a separate compiled binary, and the `[[bench]]` table in `Cargo.toml` declares it. The `name = "fib"` must match the filename `benches/fib.rs` (without the extension).

`harness = false` tells cargo: *do not* link the built-in libtest test harness into this target. Criterion supplies its own `main()` (via the `criterion_main!` macro), and it would clash with libtest's. Forgetting this line is the single most common criterion mistake; see Common Pitfalls.

### `use perf::{...}` — benchmarks are external consumers

The benchmark binary depends on your crate the same way an [integration test](/13-testing/04-integration-tests/) does: it is a separate crate that `use`s your library's **public** API by crate name (here `perf`). It cannot see private items. If you need to benchmark a private function, either make it `pub` or write the benchmark inside `src/` behind `#[cfg(test)]`. But the `benches/` convention is overwhelmingly the norm.

### `c.bench_function("name", |b| b.iter(closure))`

This is the core of criterion:

- `c: &mut Criterion` is the benchmark context, handed to you by the harness.
- `bench_function` registers one named benchmark, the analogue of Vitest's `bench("name", ...)`.
- The closure receives a `Bencher` (`b`), and `b.iter(|| ...)` is the part criterion times. **Criterion decides how many times to call your closure** — it warms up for 3 seconds, then collects 100 samples over ~5 seconds, automatically scaling the iteration count to the speed of the code. You do not write a loop; that is the harness's job.

This is an important difference from a hand-rolled `console.time` / `Date.now()` loop in JavaScript, where *you* pick an iteration count and hope it's enough. Criterion picks it for you based on the measured speed, which is why a nanosecond function gets billions of iterations and a microsecond function gets fewer.

### `std::hint::black_box` — defeating the optimizer

```rust
b.iter(|| fib_recursive(black_box(20)))
```

`black_box` is the most important — and most Rust-specific — piece here. Rust's optimizer is aggressive: if it can see that `fib_recursive(20)` is called with a constant and its result is thrown away, it may compute the answer *at compile time* (constant-folding) or delete the call entirely (dead-code elimination). Your benchmark would then measure *nothing*.

`std::hint::black_box(x)` is a compiler hint that means "pretend this value is used in an opaque way you can't reason about." Wrapping the input prevents constant-folding the argument; the value `b.iter` returns from the closure is itself black-boxed by criterion so the result isn't eliminated. There is no JavaScript equivalent because V8 doesn't constant-fold across a function boundary the way an ahead-of-time native optimizer does.

> **Warning:** `black_box` lives in `std::hint`. Criterion 0.8 still re-exports `criterion::black_box`, but it is **deprecated**: using it produces `warning: use of deprecated function 'criterion::black_box': use 'std::hint::black_box()' instead`. Always import `std::hint::black_box`.

### `criterion_group!` and `criterion_main!`

```rust
criterion_group!(benches, bench_fib);
criterion_main!(benches);
```

`criterion_group!` bundles one or more benchmark functions under a group name. `criterion_main!` generates the `fn main()` for the binary, runs the listed groups, parses CLI arguments, and writes results to `target/criterion/`. Together they replace the `main` that `harness = false` removed. (These are macros, like much of Rust's testing machinery; see [Macros](/14-macros/).)

---

## Key Differences

| Concept                | JavaScript (Vitest `bench` / tinybench)         | Rust (criterion)                                            |
| ---------------------- | ----------------------------------------------- | ----------------------------------------------------------- |
| Maturity               | Experimental, may break SemVer                  | Stable, de-facto standard for years                         |
| Where benchmarks live  | `*.bench.ts`, discovered by glob                | `benches/*.rs`, declared in `Cargo.toml` as `[[bench]]`     |
| Build profile          | Whatever your dev build is (JIT-warmed)         | Always **release** (optimized)                              |
| Iteration count        | Tool picks, or you set `time`/`iterations`      | Criterion auto-scales to hit ~5s of samples                 |
| Defeating the compiler | Rarely needed (JIT won't fold across calls)     | Essential: wrap inputs in `std::hint::black_box`            |
| Statistics             | hz, percentiles, rme                            | confidence interval `[lower estimate upper]`, outliers      |
| Regression detection   | Manual / external                               | Built in: compares to last run or a named **baseline**      |
| Output artifacts       | Terminal table                                  | Terminal + `target/criterion/` HTML plots                   |

Two differences deserve emphasis:

**Criterion is statistical, not a single number.** Each line like `time: [27.921 µs 33.765 µs 42.311 µs]` is a **confidence interval**: lower bound, best estimate (the middle), upper bound. A single `Date.now()` delta is one noisy sample; criterion reports the *distribution* and warns you when it's too wide to trust.

**Regression detection is automatic.** Run a benchmark twice and the second run compares itself against the first, printing `change: [...]` and a verdict like `Performance has improved.` or `Performance has regressed.` with a p-value. This is how teams catch accidental slowdowns in CI. There is no built-in JavaScript equivalent.

---

## Interpreting the Results

A criterion line has three numbers and they are *not* min/mean/max — they are the bounds of a 95% confidence interval for the **mean** time per iteration:

```text
fib_recursive 20        time:   [27.921 µs 33.765 µs 42.311 µs]
                                  └ lower    └ estimate └ upper
```

Read it as: "criterion is 95% confident the true average time is between 27.9 µs and 42.3 µs, with 33.8 µs as the best single guess." The **wider** that interval, the **noisier** the measurement and the less you should trust a small difference. A tight interval like `[7.3826 ns 7.9508 ns 8.8231 ns]` is more believable than a 1.5x-wide one.

The outlier report tells you how stable the run was:

```text
Found 17 outliers among 100 measurements (17.00%)
  7 (7.00%) high mild
  10 (10.00%) high severe
```

"High" outliers are samples that took *longer* than expected: usually the OS scheduler, another process, or a CPU frequency change interrupting your benchmark. A handful of mild outliers is normal; a large fraction of severe ones means your machine was busy and you should re-run on an idle system before drawing conclusions.

### Comparing against a previous run

On the *second* `cargo bench`, criterion compares against what it saved last time:

```text
fib_recursive 20        time:   [21.305 µs 21.890 µs 22.751 µs]
                        change: [−49.998% −42.701% −34.154%] (p = 0.00 < 0.05)
                        Performance has improved.

fib_iterative 20        time:   [7.1465 ns 7.2095 ns 7.2894 ns]
                        change: [−67.420% −58.014% −44.336%] (p = 0.00 < 0.05)
                        Performance has improved.
```

(The "improvement" here is largely the first run having been measured on a busier machine, a reminder that the absolute numbers are environment-dependent.) The `change:` line is itself a confidence interval, and the `p = 0.00 < 0.05` means the difference is statistically significant. When `p` is *above* the threshold criterion prints `No change in performance detected.` instead — a noise-induced wobble it deliberately refuses to call a regression.

### Named baselines for CI

For reproducible comparisons (e.g., "this PR vs `main`"), save a named baseline and compare against it later:

```bash
# On the main branch:
cargo bench -- --save-baseline main

# After switching to your feature branch:
cargo bench -- --baseline main
```

Other useful arguments (everything after `--` goes to criterion, not cargo):

- `cargo bench -- --sample-size 50`: fewer samples for a quicker, rougher run.
- `cargo bench -- --measurement-time 10`: collect samples over 10 seconds for tighter intervals.
- `cargo bench -- --noplot`: skip plot generation.
- `cargo bench -- fib_iterative`: run only benchmarks whose name contains the filter string.

### The HTML report

With the `html_reports` feature enabled, criterion writes `target/criterion/report/index.html` plus per-benchmark plots (PDF of the timing distribution, regression line, comparison with the previous run). Open it in a browser for a visual view; install `gnuplot` for nicer charts, otherwise criterion falls back to the `plotters` crate automatically.

---

## Common Pitfalls

### Pitfall 1: Forgetting `harness = false`

If you omit `harness = false` from the `[[bench]]` table, cargo links the **built-in libtest harness** into your benchmark binary. That harness ignores criterion's `main()`, looks for `#[bench]`/`#[test]` functions (there are none), and reports:

```text
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

No error, no benchmark: it just silently does nothing. If `cargo bench` finishes instantly and says `running 0 tests`, this is almost always the cause. The fix is the one line:

```toml
[[bench]]
name = "fib"
harness = false   # <- do not omit
```

### Pitfall 2: Not using `black_box`, so the optimizer deletes your code

Writing `b.iter(|| fib_iterative(20))` without `black_box` lets the optimizer constant-fold `fib_iterative(20)` to a literal and time an empty loop. You'd see a suspiciously flat result like `time: [0.0000 ns ...]` or a number far smaller than physically plausible. Wrap inputs (and trust criterion to black-box the return value):

```rust
use std::hint::black_box;
// ...
b.iter(|| fib_iterative(black_box(20)))
```

If a benchmark reports a function taking essentially zero time, suspect a missing `black_box` before believing your code is infinitely fast.

### Pitfall 3: Reaching for `#[bench]` (it's nightly-only)

Rust *does* have a built-in `#[bench]` attribute, and old blog posts use it:

```rust
// does not compile on stable (error[E0554])
#![feature(test)]
extern crate test;
use test::Bencher;

#[bench]
fn bench_add(b: &mut Bencher) {
    b.iter(|| 1 + 1);
}
```

On stable `rustc` this fails immediately:

```text
error[E0554]: `#![feature]` may not be used on the stable release channel
 --> src/lib.rs:1:1
  |
1 | #![feature(test)]
  | ^^^^^^^^^^^^^^^^^
```

The `test` crate and `#[bench]` have been "about to stabilize" for a decade and remain **nightly-only**. Do not use them on stable. Criterion is the stable answer and is more capable anyway (statistics, baselines, plots). The other actively maintained alternative is the **divan** crate, which offers a lighter, attribute-style API — also stable — if you prefer it.

### Pitfall 4: Including setup inside the timed closure

Anything inside the `b.iter(|| ...)` closure is measured *every iteration*. If you build a large input there, you benchmark the setup, not the function. Build inputs **outside** the closure, or use the input-aware forms (`iter_batched`, `bench_with_input`) shown below.

```rust
// measures String allocation on every iteration
b.iter(|| {
    let data = "1,2,3,4,5".to_string();
    sum_csv_line(black_box(&data))
});

// build once, measure only the call
let data = "1,2,3,4,5".to_string();
b.iter(|| sum_csv_line(black_box(&data)));
```

### Pitfall 5: Trusting numbers from a busy machine

A large fraction of "high severe" outliers, or wildly different numbers between runs, means the environment is noisy. Close other programs, disable CPU turbo/throttling if you can, run on an idle machine, and prefer the `change:` verdict (which accounts for variance) over eyeballing raw nanoseconds.

---

## Best Practices

### Benchmark realistic inputs across sizes

A single input size hides how an algorithm scales. Use a `BenchmarkGroup` with `bench_with_input` to sweep sizes, and `Throughput` so criterion reports bytes/elements per second:

```rust
// benches/csv.rs
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

use perf::sum_csv_line;

fn make_line(n: usize) -> String {
    (1..=n).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

fn bench_sum_csv(c: &mut Criterion) {
    let mut group = c.benchmark_group("sum_csv_line");
    for size in [10usize, 100, 1_000] {
        let line = make_line(size);
        group.throughput(Throughput::Bytes(line.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &line, |b, line| {
            b.iter(|| sum_csv_line(black_box(line)))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_sum_csv);
criterion_main!(benches);
```

Real `cargo bench` output (the `thrpt:` line is the throughput interval):

```text
sum_csv_line/10         time:   [106.39 ns 129.57 ns 157.18 ns]
                        thrpt:  [121.35 MiB/s 147.20 MiB/s 179.28 MiB/s]
sum_csv_line/100        time:   [1.0096 µs 1.1391 µs 1.3095 µs]
                        thrpt:  [211.92 MiB/s 243.64 MiB/s 274.88 MiB/s]
sum_csv_line/1000       time:   [9.8442 µs 9.9936 µs 10.168 µs]
                        thrpt:  [365.03 MiB/s 371.41 MiB/s 377.04 MiB/s]
```

Throughput *rising* with input size (here ~121 → ~371 MiB/s) tells you the per-call fixed overhead is being amortized over more work, a signal you couldn't read off a single timing number.

### Benchmark correct code

A fast wrong answer is worthless. Keep [unit tests](/13-testing/00-unit-tests/) green first; benchmark only code you trust. Criterion and `cargo test` are independent; benchmarks do not assert correctness.

### Keep benchmarks in `benches/`, commit baselines deliberately

Treat `benches/` like `tests/`: version-controlled, reviewed. The `target/criterion/` data is a build artifact; leave it out of git (the default `.gitignore` for `target/` handles this). For CI regression-gating, save a baseline from your main branch and compare PRs against it.

### Use `iter_batched` when each iteration needs fresh, consumed input

If the code under test *consumes* or *mutates* its input (e.g., sorting a `Vec` in place), build a fresh copy per batch so you don't measure an already-sorted vector on the second iteration:

```rust
use criterion::{BatchSize, Criterion};

fn bench_sort(c: &mut Criterion) {
    c.bench_function("sort 1000", |b| {
        b.iter_batched(
            || (0..1000u32).rev().collect::<Vec<_>>(), // setup: NOT timed
            |mut data| data.sort(),                    // routine: timed
            BatchSize::SmallInput,
        )
    });
}
```

The setup closure runs outside the timer; only the routine is measured.

---

## Real-World Example

A production-flavored scenario: you maintain a parser that sums integers from a CSV-style line, and you want to compare your current iterator-based implementation against a hand-written byte-scanning version to decide whether the rewrite is worth it. This is exactly the kind of decision criterion is built for.

```rust
// src/lib.rs

/// Iterator-based: split on commas, trim, parse, sum.
pub fn sum_csv_iter(line: &str) -> i64 {
    line.split(',')
        .filter_map(|tok| tok.trim().parse::<i64>().ok())
        .sum()
}

/// Hand-written byte scanner: one pass, no intermediate &str allocations.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_implementations_agree() {
        let line = " 1, 22 ,-3,,44 ";
        assert_eq!(sum_csv_iter(line), 64);
        assert_eq!(sum_csv_scan(line), 64);
    }
}
```

The benchmark compares both on the same input inside one group, so the report puts them side by side:

```rust
// benches/csv_compare.rs
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;

use perf::{sum_csv_iter, sum_csv_scan};

fn make_line(n: usize) -> String {
    (1..=n).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

fn bench_compare(c: &mut Criterion) {
    let line = make_line(1_000);
    let mut group = c.benchmark_group("sum_csv 1000 fields");
    group.throughput(Throughput::Bytes(line.len() as u64));

    group.bench_function("iter", |b| {
        b.iter(|| sum_csv_iter(black_box(&line)))
    });
    group.bench_function("scan", |b| {
        b.iter(|| sum_csv_scan(black_box(&line)))
    });

    group.finish();
}

criterion_group!(benches, bench_compare);
criterion_main!(benches);
```

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.8.2", features = ["html_reports"] }

[[bench]]
name = "csv_compare"
harness = false
```

Running `cargo bench` first confirms correctness via `cargo test` (run separately), then produces a side-by-side timing for the two strategies under the `sum_csv 1000 fields/iter` and `sum_csv 1000 fields/scan` names. You read the two confidence intervals, check that they don't overlap, glance at the `change:` line if you've benchmarked before, and only then decide whether the byte-scanner's extra complexity earns its keep. That data-driven loop — measure, change, measure again, let criterion judge the difference — is the entire point.

> **Tip:** Pair benchmarking with [coverage](/13-testing/10-coverage/) and a fast runner like `cargo nextest` for everyday testing; reach for criterion only when a *performance* question (not a correctness one) is on the table. Premature micro-benchmarking is as wasteful as premature optimization.

---

## Further Reading

- [Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/index.html) — the authoritative manual: groups, throughput, baselines, plotting.
- [`criterion` on crates.io](https://crates.io/crates/criterion) and [docs.rs](https://docs.rs/criterion/latest/criterion/): current API reference.
- [`std::hint::black_box`](https://doc.rust-lang.org/std/hint/fn.black_box.html): why and how it defeats the optimizer.
- [`cargo bench` reference](https://doc.rust-lang.org/cargo/commands/cargo-bench.html): the cargo side of running benchmarks.
- [The unstable `test` crate / `#[bench]`](https://doc.rust-lang.org/unstable-book/library-features/test.html): the nightly-only built-in, for context.
- [`divan`](https://crates.io/crates/divan): a stable, attribute-style alternative to criterion.
- Sibling topics in this section:
  - [Unit Tests](/13-testing/00-unit-tests/) — correctness with `#[test]`; benchmarks measure speed, not correctness.
  - [Integration Tests](/13-testing/04-integration-tests/) — like `benches/`, these are external crates that see only your public API.
  - [Property Testing](/13-testing/07-property-testing/): for exploring input *spaces* rather than timing.
  - [Coverage](/13-testing/10-coverage/): `cargo-llvm-cov` and the faster `cargo nextest` runner.
  - [Mocking](/13-testing/06-mocking/), [Test Fixtures](/13-testing/05-test-fixtures/), [Doc Tests](/13-testing/09-doc-tests/), [TDD Workflow](/13-testing/11-tdd-workflow/).
- Foundations used above:
  - [Cargo Basics](/01-getting-started/03-cargo-basics/) — `cargo bench`, profiles, and `dev-dependencies`.
  - [Basics: Types](/02-basics/01-types/) — the `u64`/`i64` integer types these benchmarks use.
  - [Macros](/14-macros/) — `criterion_group!`/`criterion_main!` and `#[bench]` are macros/attributes.

---

## Exercises

### Exercise 1: Your first criterion benchmark

**Difficulty:** Easy

**Objective:** Set up criterion from scratch and benchmark a single function.

**Instructions:** Given a `pub fn factorial(n: u64) -> u64` (iterative), create a `benches/factorial.rs` that benchmarks `factorial(10)`. Add the `[dev-dependencies]` and `[[bench]]` entries to `Cargo.toml` (don't forget `harness = false`), wrap the input in `black_box`, and run `cargo bench`.

```rust
// src/lib.rs
pub fn factorial(n: u64) -> u64 {
    (1..=n).product()
}

// TODO: write benches/factorial.rs and the Cargo.toml entries
```

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.8.2", features = ["html_reports"] }

[[bench]]
name = "factorial"
harness = false
```

```rust
// benches/factorial.rs
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use my_crate::factorial; // replace `my_crate` with your crate name

fn bench_factorial(c: &mut Criterion) {
    c.bench_function("factorial 10", |b| {
        b.iter(|| factorial(black_box(10)))
    });
}

criterion_group!(benches, bench_factorial);
criterion_main!(benches);
```

`cargo bench` prints a line like:

```text
factorial 10            time:   [4.1 ns 4.3 ns 4.6 ns]
```

(The exact nanoseconds depend on your machine; what matters is that you get a three-number confidence interval, not `running 0 tests`.)

</details>

### Exercise 2: Compare two implementations across input sizes

**Difficulty:** Medium

**Objective:** Use a `BenchmarkGroup` with `bench_with_input` to compare two functions over several sizes.

**Instructions:** Write two functions that compute the sum `1 + 2 + ... + n`: a `sum_loop(n: u64) -> u64` using a `for` loop, and a `sum_formula(n: u64) -> u64` using `n * (n + 1) / 2`. Benchmark both for `n` in `[100, 10_000, 1_000_000]` inside a single group, so the report shows them side by side. Remember to `black_box` the input.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
pub fn sum_loop(n: u64) -> u64 {
    let mut total = 0u64;
    for i in 1..=n {
        total += i;
    }
    total
}

pub fn sum_formula(n: u64) -> u64 {
    n * (n + 1) / 2
}
```

```rust
// benches/sums.rs
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;

use my_crate::{sum_formula, sum_loop};

fn bench_sums(c: &mut Criterion) {
    let mut group = c.benchmark_group("sum 1..=n");
    for n in [100u64, 10_000, 1_000_000] {
        group.bench_with_input(BenchmarkId::new("loop", n), &n, |b, &n| {
            b.iter(|| sum_loop(black_box(n)))
        });
        group.bench_with_input(BenchmarkId::new("formula", n), &n, |b, &n| {
            b.iter(|| sum_formula(black_box(n)))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_sums);
criterion_main!(benches);
```

`cargo bench` produces six benchmarks (`sum 1..=n/loop/100`, `sum 1..=n/formula/100`, and so on). The formula stays in single-digit nanoseconds for *every* `n` because it's O(1), while the loop's time grows linearly with `n` — the constant-time line is flat while the loop line climbs, which is the whole point of sweeping sizes.

</details>

### Exercise 3: Save a baseline and detect a regression

**Difficulty:** Medium

**Objective:** Use criterion's baseline machinery to prove a change made code slower.

**Instructions:** Start with an efficient `pub fn contains(haystack: &[i32], needle: i32) -> bool` that uses the standard library's `.contains()`. Benchmark it, save the result as a baseline named `fast`. Then rewrite `contains` to be deliberately slower (e.g., sort a clone on every call before scanning), and run the benchmark again comparing against the `fast` baseline. Observe criterion report `Performance has regressed.`

<details>
<summary>Solution</summary>

Step 1 — the fast version:

```rust
// src/lib.rs
pub fn contains(haystack: &[i32], needle: i32) -> bool {
    haystack.contains(&needle)
}
```

```rust
// benches/search.rs
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use my_crate::contains;

fn bench_contains(c: &mut Criterion) {
    let data: Vec<i32> = (0..1_000).collect();
    c.bench_function("contains worst case", |b| {
        // 1_001 is absent: forces a full scan.
        b.iter(|| contains(black_box(&data), black_box(1_001)))
    });
}

criterion_group!(benches, bench_contains);
criterion_main!(benches);
```

Save the baseline:

```bash
cargo bench -- --save-baseline fast
```

Step 2 — make it slower (pointless extra work each call):

```rust
// src/lib.rs
pub fn contains(haystack: &[i32], needle: i32) -> bool {
    let mut sorted = haystack.to_vec(); // clone on every call
    sorted.sort();                       // O(n log n) work we don't need
    sorted.binary_search(&needle).is_ok()
}
```

Step 3 — compare against the saved baseline:

```bash
cargo bench -- --baseline fast
```

Criterion prints something like:

```text
contains worst case     time:   [12.4 µs 12.6 µs 12.9 µs]
                        change: [+900% +1200% +1500%] (p = 0.00 < 0.05)
                        Performance has regressed.
```

The `change:` interval is positive and `p < 0.05`, so criterion flags a statistically significant regression. (Exact percentages depend on your machine.) This is the workflow CI uses to fail a build that accidentally slows a hot path.

</details>
