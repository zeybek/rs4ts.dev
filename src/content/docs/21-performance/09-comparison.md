---
title: "Performance: Rust vs Node.js / TypeScript — Where Rust Wins, and the Honest Caveats"
description: "Where Rust beats Node.js and where it doesn't: no GC, contiguous memory, instant startup, with honest caveats for I/O-bound work and warm V8 numeric loops."
---

## Quick Overview

Rust is a compiled, garbage-collector-free systems language, so for CPU-bound and memory-bound work it typically runs several times faster than Node.js while using a fraction of the memory and starting almost instantly. But "faster" is not a license to rewrite everything: a JIT-compiled V8 loop can rival native code on tight numeric kernels, most web services are I/O-bound (so the language barely matters), and Rust buys its speed with stricter code and longer build times. This page gives a TypeScript/JavaScript developer an honest, measured mental model of *where* Rust pulls ahead, by *how much*, and where the gap closes to nothing.

> **Note:** Every timing, memory figure, and program output on this page was produced by running the code on one machine (Apple Silicon, macOS, Rust 1.96 release builds, Node v22). Absolute numbers will differ on your hardware; treat them as *ratios and direction*, not benchmark-grade truth. For rigorous measurement, see [Benchmarking with Criterion](/21-performance/02-benchmarking/), and always [measure before you optimize](/21-performance/10-when-to-optimize/).

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically, and `--release` turns on the optimizations that make these comparisons fair.

---

## TypeScript/JavaScript Example

Here is a workload that shows up constantly in real backends: build a large in-memory dataset of small records and reduce over it. In Node.js, each record is a heap-allocated object, and the V8 garbage collector tracks every one of them.

```typescript
// points.mjs — build 10 million records, then sum two fields.
const n = 10_000_000;
const start = process.hrtime.bigint();

const pts = new Array(n);
for (let i = 0; i < n; i++) {
  pts[i] = { x: i, y: i * 2, label: i % 7 }; // each {} is a separate GC-tracked heap object
}

let sum = 0;
for (let i = 0; i < n; i++) {
  sum += pts[i].x + pts[i].y;
}

const ms = Number(process.hrtime.bigint() - start) / 1e6;
console.log(`n=${n} sum=${sum} elapsed=${ms.toFixed(3)} ms`);

const mem = process.memoryUsage();
console.log(`rss=${(mem.rss / 1048576).toFixed(1)} MiB heapUsed=${(mem.heapUsed / 1048576).toFixed(1)} MiB`);
```

Real output on the test machine (`node points.mjs`):

```text
n=10000000 sum=149999985000000 elapsed=673.854 ms
rss=645.4 MiB heapUsed=538.9 MiB
```

The result is correct, the code is clean, and you never wrote `malloc` or `free`. The cost is invisible but real: ten million distinct heap objects, each carrying a V8 object header and hidden-class pointer, all scanned and managed by the garbage collector. That is where Rust's structural advantages show up.

---

## Rust Equivalent

The same logic in Rust. The records live in one contiguous `Vec` of plain 24-byte structs: no per-element heap object, no header, no GC.

```rust playground
#[derive(Clone)]
struct Point {
    x: f64,
    y: f64,
    label: u32,
}

fn main() {
    let n: usize = 10_000_000;
    let start = std::time::Instant::now();

    // One contiguous allocation of n structs, sized up front.
    let mut pts: Vec<Point> = Vec::with_capacity(n);
    for i in 0..n {
        pts.push(Point {
            x: i as f64,
            y: (i * 2) as f64,
            label: (i % 7) as u32,
        });
    }

    let mut sum = 0.0_f64;
    for p in &pts {
        sum += p.x + p.y;
    }

    let elapsed = start.elapsed();
    println!("n={n} sum={sum} elapsed={:.3?}", elapsed);
    println!("vec bytes ~= {}", pts.len() * std::mem::size_of::<Point>());
}
```

Real output (`cargo run --release`):

```text
n=10000000 sum=149999985000000 elapsed=51.459ms
vec bytes ~= 240000000
```

Same answer (`149999985000000`), about **13x faster** (51 ms vs 674 ms), and the memory tells an even sharper story. Measuring peak resident memory with `/usr/bin/time -l`:

```text
# Rust release build
maximum resident set size: 241500160      # ~230 MiB — dominated by the 240 MB Vec itself
# Node.js
maximum resident set size: 676839424      # ~645 MiB — ~2.8x more for the same logical data
```

The Rust process's footprint is essentially *just the data* (240 MB for ten million 24-byte structs). Node needs roughly 2.8x that because every record is a separate object with engine overhead, plus the GC's working set.

---

## Detailed Explanation

Why does Rust win here, and where does the win actually come from? Four distinct mechanisms, each independent of the others.

### 1. Ahead-of-time compilation to native code

Node runs JavaScript through V8, which interprets bytecode and then JIT-compiles hot functions to machine code at runtime. That is genuinely fast once warmed up (V8's TurboFan is a world-class compiler), but it pays a warm-up cost, can deoptimize when types shift, and must keep the JIT machinery resident. Rust is compiled fully ahead of time by LLVM, with the same optimizer backend Clang uses for C++. There is no interpreter, no warm-up, and no deopt cliff: the code your users run is the optimized machine code, every time, from the first call.

### 2. No garbage collector

This is the deepest difference, and the reason TypeScript developers feel the change most. V8 reclaims memory by periodically tracing live objects; Rust reclaims memory **deterministically** the moment a value's owner goes out of scope (see [Ownership](/05-ownership/) and [Smart Pointers](/10-smart-pointers/)). The consequences:

- **No GC pauses.** A V8 major GC can stop the world for milliseconds; in a latency-sensitive service that becomes tail-latency jitter you cannot fully control. Rust has no such pauses because there is no collector to run.
- **Lower memory.** No object headers, no reachability metadata, no heap headroom kept free so the collector has room to work. The 2.8x memory gap above is mostly this.
- **Predictability.** Freeing happens at known points in the code, not "sometime later when the heap pressure builds." For real-time, embedded, or p99-sensitive workloads, predictable beats fast-on-average.

### 3. Value types and contiguous memory by default

In JavaScript, `{ x, y, label }` is always a heap object accessed through a pointer; an `Array` of them is an array of pointers scattered across the heap. In Rust, `Point` is a flat 24-byte value, and `Vec<Point>` stores those values back-to-back in one allocation. Summing over them is a linear walk that the CPU prefetcher loves: no pointer-chasing, far fewer cache misses. This is the same reason a `Float64Array` (a typed array) is dramatically faster than a plain `Array` of numbers in JS: contiguous primitives win. Rust gives you that layout for *your own structs* by default. (The `label` field forces `Point` to 24 bytes via alignment padding; [Memory Layout](/21-performance/04-memory-layout/) explains why, and [Cache-Friendly Code](/21-performance/05-cache-efficiency/) covers contiguous layout in depth.)

### 4. Zero-cost abstractions

The iterator chain, the closure, the generic — in release mode these compile down to the same machine code as a hand-written loop, with no runtime dispatch and no boxing. You do not pay for the abstraction. JavaScript's high-level constructs (`.map`, `.filter`, spread) usually allocate intermediate arrays and box numbers, so the convenient style is the slow style. In Rust the convenient style *is* the fast style. [Zero-Cost Abstractions](/21-performance/06-zero-cost/) shows the disassembly evidence; here is the behavioral equivalence:

```rust playground
fn sum_even_squares_iter(data: &[u64]) -> u64 {
    data.iter().filter(|&&n| n % 2 == 0).map(|&n| n * n).sum()
}

fn sum_even_squares_loop(data: &[u64]) -> u64 {
    let mut total = 0;
    for &n in data {
        if n % 2 == 0 {
            total += n * n;
        }
    }
    total
}

fn main() {
    let data: Vec<u64> = (0..1000).collect();
    // The lazy pipeline and the hand loop are interchangeable...
    assert_eq!(sum_even_squares_iter(&data), sum_even_squares_loop(&data));
    println!("both = {}", sum_even_squares_iter(&data));
}
```

Real output (`cargo run --release`):

```text
both = 166167000
```

The lazy iterator does **not** build an intermediate filtered array the way `data.filter(...).map(...)` does in JS; it fuses into a single pass and, in a release build, optimizes to essentially the loop.

### Startup time and deployment

Native binaries start almost instantly because there is no runtime to boot. Timing a trivial "hello" program 20 times and taking the median:

```text
rust hello median: 2.77 ms
node hello median: 31.74 ms      # ~11x slower to start
```

Node must initialize V8, the event loop, and core modules before your first line runs. For a long-lived server this is a one-time cost you ignore; for a CLI tool, a serverless function with cold starts, or a per-request subprocess, an 11x faster startup is decisive. And the Rust artifact is a single self-contained binary (the trivial one above is ~400 KB; see [Reducing Binary Size](/21-performance/08-binary-size/) to shrink it further): no `node_modules`, no runtime to install on the target machine.

---

## Key Differences

| Dimension | Node.js / TypeScript | Rust | Practical impact |
| --- | --- | --- | --- |
| Execution | V8 JIT (interpret → optimize at runtime) | AOT-compiled native code (LLVM) | No warm-up, no deopt; consistent from first call |
| Memory management | Tracing garbage collector | Ownership + deterministic drop | No GC pauses; ~2-5x less memory typical |
| Default data layout | Objects on heap, arrays of pointers | Values inline, `Vec<T>` contiguous | Far fewer cache misses on bulk data |
| Abstractions | `.map`/`.filter` allocate intermediates | Iterators/closures are zero-cost | Idiomatic code is also the fast code |
| Concurrency | Single-threaded event loop + workers | Real threads, `Send`/`Sync`, fearless | Rust uses all cores without GIL-style limits |
| Numbers | All `number` is f64 (precision loss > 2^53) | `i32`/`i64`/`u64`/`f64` etc., exact integers | Rust does exact 64-bit integer math |
| Startup | ~30 ms (boot V8) | ~3 ms (none) | Wins for CLIs, cold starts, subprocesses |
| Deployment | Needs Node + `node_modules` | Single static-ish binary | Simpler, smaller containers |
| Build/iteration | Instant (no compile step) | Compile required (can be slow) | Slower dev loop; see [Reducing Compile Time](/21-performance/07-compilation-time/) |

### Where the gap is *small or zero* (the honest part)

Rust does not win everything, and pretending otherwise would mislead you.

- **I/O-bound work.** If a request spends 40 ms waiting on Postgres and 0.2 ms in your code, rewriting that 0.2 ms in Rust changes the response time by half a percent. Most CRUD web services are I/O-bound. The database, the network, and your query plan dominate; the language barely registers. Async Rust ([Section 11](/11-async/)) and Node both handle thousands of concurrent waits efficiently.
- **Warm, monomorphic numeric loops.** On a tight integer kernel that V8 can fully type-specialize and JIT, the gap narrows dramatically; in some of our trial-division prime-counting runs, warmed-up Node landed within the run-to-run noise of the Rust build. V8 is genuinely excellent at this specific shape of code. Rust's reliable, large wins come on **allocation-heavy, memory-bound, multi-threaded, or latency-sensitive** workloads, not on every micro-benchmark.
- **Throwaway scripts and glue.** A 50-line data-munging script that runs once does not care about 13x; it cares about how fast *you* wrote it. Node's instant edit-run loop and vast npm ecosystem win there.

> **Tip:** A good rule of thumb: the more your program is *doing work with data in memory* (parsing, transforming, computing, serving many connections), the more Rust wins. The more it is *waiting* (on the network, a disk, a database), the less the language choice matters.

---

## Common Pitfalls

### Pitfall 1: Benchmarking the debug build

The single most common "Rust isn't faster" mistake. `cargo run` and `cargo build` produce an **unoptimized** debug binary; `cargo run --release` enables optimizations. Debug builds can be 10-50x slower than release and are not a fair comparison to V8's JIT-optimized output. Always measure release.

```text
$ cargo run            # debug — slow, do NOT benchmark this
$ cargo run --release  # optimized — this is the fair comparison
```

### Pitfall 2: Expecting JS-style integer math

In JavaScript every `number` is an IEEE-754 double, so integers above 2^53 silently lose precision (they do **not** wrap — that is a separate myth). Rust's integer types are exact across their full range:

```rust playground
fn main() {
    let big: i64 = 9_007_199_254_740_993; // 2^53 + 1
    println!("i64 stays exact: {}", big);
    let as_f64 = big as f64; // now it behaves like a JS number
    println!("as f64 rounds:   {}", as_f64 as i64);
}
```

Real output:

```text
i64 stays exact: 9007199254740993
as f64 rounds:   9007199254740992
```

The exact same value in Node's default `number` type prints `9007199254740992`; the `+ 1` is gone. (Node's `BigInt`, `9007199254740993n`, is exact, like Rust's `i64`.) Rust gives you exact 64-bit integers without opting into a slower bignum type, which is both a correctness and a performance advantage.

### Pitfall 3: Integer overflow surprises a JS developer

Because JS numbers never overflow into garbage (they go imprecise instead), TypeScript developers do not think about overflow. Rust does. In a **debug** build, arithmetic overflow panics; in a **release** build it wraps (two's complement). Both behaviors are real and intentional:

```rust
use std::hint::black_box;

fn main() {
    let a: u8 = black_box(250);
    let b: u8 = black_box(10);
    let c = a + b; // 260 doesn't fit in u8
    println!("{c}");
}
```

Debug build (`cargo run`), real panic:

```text
thread 'main' panicked at src/main.rs:5:13:
attempt to add with overflow
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

Release build (`cargo run --release`), wraps silently:

```text
4
```

For deliberate wrapping use `wrapping_add`; for checked math use `checked_add` (returns `Option`); for saturating use `saturating_add`. Choosing the right one is part of writing correct *and* fast Rust.

### Pitfall 4: Assuming Rust is multithreaded by default

It is not. Rust gives you *fearless* concurrency — the compiler stops data races at compile time via `Send`/`Sync` — but a plain Rust program runs on one thread until you opt in (with `std::thread`, [Rayon](/22-common-patterns/), or an async runtime). The advantage over Node is that when you *do* go parallel, you genuinely use every core with shared memory and no GIL-style serialization, whereas Node's main loop is single-threaded and `worker_threads` communicate by copying or via `SharedArrayBuffer`. The win is *available*, not *automatic*.

### Pitfall 5: Comparing the wrong thing

Do not benchmark "Rust vs TypeScript" by timing a one-shot script that spends its life starting up, reading a tiny file, and exiting; you will mostly measure process startup, which flatters Rust unfairly, or compile time, which flatters Node unfairly. Benchmark the *steady-state hot path* of a realistic workload, with warm-up where a JIT is involved, using the methodology in [Benchmarking with Criterion](/21-performance/02-benchmarking/).

---

## Best Practices

- **Profile before porting.** Use a profiler ([Profiling Rust Applications](/21-performance/00-profiling/)) on your *existing* Node service to find the true hot path. If it is the database, Rust will not save you; fix the query first.
- **Port the hot 5%, not the whole app.** You rarely need to rewrite a whole service. Extract the CPU-bound kernel — an image transform, a parser, a crypto routine, a search index — into Rust and call it from Node via [N-API / napi-rs](/20-unsafe-ffi/) or compile it to [WebAssembly](/19-wasm/). You keep your TypeScript codebase and get native speed where it counts.
- **Always measure the release build**, and warm up any JIT'd comparison so V8 gets a fair shot.
- **Lean on the idiomatic style.** In Rust, iterators, borrowing `&str`/slices, and `Vec::with_capacity` are both the clean way and the fast way ([Optimization Techniques](/21-performance/03-optimization/)). You do not trade readability for speed the way you sometimes do in JS.
- **Pick the right concurrency model.** I/O-bound and highly concurrent? Use async Rust (Tokio). CPU-bound and parallelizable? Use threads or Rayon. Do not reach for async to speed up a number-crunching loop; that is Node thinking.
- **Be honest in your write-up.** If you report a speedup, say what the workload was, that it was a release build, the hardware, and where the gap is *not* large. Credibility is a performance feature.

---

## Real-World Example

A production-flavored task that genuinely favors Rust: aggregating a stream of request-log lines into per-route latency stats in a single allocation-light pass. This is the kind of in-memory data crunching where Rust's borrowing and contiguous layout pay off, and it is realistic for a metrics pipeline or log processor.

```rust playground
use std::collections::HashMap;

#[derive(Debug)]
struct Stats {
    count: u64,
    total_ms: u64,
}

/// Aggregate request latencies by route in a single pass.
/// The route keys borrow directly from the input lines — no per-row String allocation.
fn aggregate<'a>(lines: impl Iterator<Item = &'a str>) -> HashMap<&'a str, Stats> {
    let mut by_route: HashMap<&str, Stats> = HashMap::new();
    for line in lines {
        // line format: "<route> <latency_ms>"
        let mut parts = line.split_whitespace();
        let (Some(route), Some(ms)) = (parts.next(), parts.next()) else {
            continue; // skip malformed lines
        };
        let Ok(ms) = ms.parse::<u64>() else { continue };

        let entry = by_route.entry(route).or_insert(Stats { count: 0, total_ms: 0 });
        entry.count += 1;
        entry.total_ms += ms;
    }
    by_route
}

fn main() {
    let log = "\
/api/users 12
/api/users 8
/api/login 30
/api/users 10
/api/login 25";

    let stats = aggregate(log.lines());

    let mut routes: Vec<_> = stats.iter().collect();
    routes.sort_by_key(|(route, _)| *route); // sort only for stable display
    for (route, s) in routes {
        let avg = s.total_ms as f64 / s.count as f64;
        println!("{route:<12} count={} avg={:.1}ms", s.count, avg);
    }
}
```

Real output (`cargo run --release`):

```text
/api/login   count=2 avg=27.5ms
/api/users   count=3 avg=10.0ms
```

The TypeScript equivalent would `split` each line into a fresh array, build objects, and let the GC manage every intermediate string and the `Map`. The Rust version borrows the route names straight out of the input (the `<'a>` lifetime ties the map's keys to the input's lifetime; see [Ownership](/05-ownership/)), so the hot loop allocates only when a brand-new route key is first inserted into the map. On a multi-gigabyte log this is the difference between a steady, low-memory pass and a GC working overtime.

> **Note:** This is exactly the kind of kernel to extract into Rust and call from a Node service via WebAssembly or N-API. Your TypeScript orchestration stays; the heavy loop gets native speed and flat memory.

---

## Further Reading

- [The Computer Language Benchmarks Game](https://benchmarksgame-team.pages.debian.net/benchmarksgame/): cross-language micro-benchmarks; read them critically, as a directional signal, not gospel.
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/): the canonical guide to making Rust fast, the right way.
- [V8 — Ignition & TurboFan](https://v8.dev/docs) — how Node's JIT actually optimizes your JavaScript, so the comparison is fair both ways.
- [`std::time::Instant`](https://doc.rust-lang.org/std/time/struct.Instant.html): the monotonic clock used for the timings above.

Related sections of this guide:

- [When to Optimize](/21-performance/10-when-to-optimize/): measure first; avoid premature optimization (read this *before* you act on any number here).
- [Benchmarking with Criterion](/21-performance/02-benchmarking/) — rigorous measurement with `criterion`, `black_box`, and warm-up.
- [Profiling Rust Applications](/21-performance/00-profiling/) and [Flame Graphs with cargo-flamegraph](/21-performance/01-flamegraph/): find the hot path before porting it.
- [Optimization Techniques](/21-performance/03-optimization/) — the idiomatic habits (no needless clones, `&str`, iterators, capacity) that produce these wins.
- [Memory Layout](/21-performance/04-memory-layout/) and [Cache-Friendly Code](/21-performance/05-cache-efficiency/): why contiguous value types beat pointer-chasing.
- [Zero-Cost Abstractions](/21-performance/06-zero-cost/) — the disassembly proof that iterators compile to loops.
- [Reducing Binary Size](/21-performance/08-binary-size/) and [Reducing Compile Time](/21-performance/07-compilation-time/): the deployment-size and dev-loop trade-offs mentioned above.
- [Section 11: Async](/11-async/) — for I/O-bound concurrency where the language gap is small.
- [Section 19: WebAssembly](/19-wasm/) and [Section 20: Unsafe & FFI](/20-unsafe-ffi/): how to call a Rust hot path from your existing Node app.
- [Common Patterns](/22-common-patterns/) — idioms (including Rayon) the fast versions rely on.
- Foundations: [Introduction](/00-introduction/) · [Getting Started](/01-getting-started/) · [Basics](/02-basics/).

---

## Exercises

### Exercise 1: Make the comparison fair

**Difficulty:** Easy

**Objective:** Experience the debug-vs-release gap that causes most "Rust isn't faster" confusion.

**Instructions:** Write a program that sums `0..50_000_000` into a `u64`, timing it with `std::time::Instant`. Run it once with `cargo run` and once with `cargo run --release`. Note both elapsed times and explain why they differ. (Use `std::hint::black_box` on the result so the optimizer cannot delete the whole loop.)

<details>
<summary>Solution</summary>

```rust playground
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let start = Instant::now();
    let mut sum: u64 = 0;
    for i in 0..50_000_000u64 {
        sum += i;
    }
    // Keep the optimizer honest: it must actually compute `sum`.
    let sum = black_box(sum);
    println!("sum = {sum}");
    println!("elapsed: {:.3?}", start.elapsed());
}
```

Run it both ways:

```text
$ cargo run            # debug: optimizations OFF — far slower
$ cargo run --release  # release: optimizations ON — the fair number
```

The debug build keeps overflow checks, does no inlining, and stores everything to the stack between operations, so it can be an order of magnitude slower. The release build optimizes the loop aggressively (it may even reduce it to a closed-form computation). **Only the release build is a fair comparison to Node's JIT-optimized output.** Without `black_box`, an aggressive release build could discard the unused result entirely and report a near-zero time; see [Benchmarking with Criterion](/21-performance/02-benchmarking/) for why `black_box` matters.

</details>

### Exercise 2: Exact integers vs `number`

**Difficulty:** Medium

**Objective:** Demonstrate, with output, where JavaScript's `number` loses precision and Rust's integers do not.

**Instructions:** In Rust, take `u64::MAX / 2 + 5` (a value well above 2^53), print it exactly, then cast it to `f64` and back to `u64` and print that. Explain which line corresponds to JavaScript's default behavior and which to `BigInt`.

<details>
<summary>Solution</summary>

```rust playground
fn main() {
    let exact: u64 = u64::MAX / 2 + 5;
    println!("u64 exact:        {exact}");

    let via_f64 = exact as f64;       // what a JS `number` would store
    let back: u64 = via_f64 as u64;   // round-tripped through the double
    println!("through f64:      {back}");
    println!("changed by f64:   {}", exact != back);
}
```

Real output (`cargo run --release`):

```text
u64 exact:        9223372036854775811
through f64:      9223372036854775808
changed by f64:   true
```

The `u64` line behaves like JavaScript's `BigInt`: exact across the full 64-bit range. The round-through-`f64` line behaves like a default JS `number`: above 2^53 it cannot represent every integer, so the value is rounded to the nearest representable double (`...808` instead of `...811`). Rust gives you exact 64-bit integers by default; in JavaScript you must reach for `BigInt` (which is slower) to get the same guarantee.

</details>

### Exercise 3: Find the workload where the language barely matters

**Difficulty:** Medium-Hard

**Objective:** Build intuition for the honest caveat — that I/O-bound work hides the language gap — by simulating a workload that is mostly waiting.

**Instructions:** Write a function that "handles a request" by sleeping 40 ms (simulating a database round-trip) and then doing a small CPU task (sum `0..10_000`). Time the *total*, then time *just* the CPU part, and print the CPU part as a percentage of the total. Argue, from the numbers, how much a faster language could possibly improve this request.

<details>
<summary>Solution</summary>

```rust playground
use std::hint::black_box;
use std::thread::sleep;
use std::time::{Duration, Instant};

fn handle_request() -> (Duration, Duration) {
    let total_start = Instant::now();

    // Simulated I/O wait (DB/network round-trip) — the language can't speed this up.
    sleep(Duration::from_millis(40));

    // The actual CPU work this request does.
    let cpu_start = Instant::now();
    let mut acc: u64 = 0;
    for i in 0..10_000u64 {
        acc += i;
    }
    black_box(acc);
    let cpu = cpu_start.elapsed();

    (total_start.elapsed(), cpu)
}

fn main() {
    let (total, cpu) = handle_request();
    let pct = cpu.as_secs_f64() / total.as_secs_f64() * 100.0;
    println!("total: {total:.3?}");
    println!("cpu:   {cpu:.3?}");
    println!("cpu is {pct:.4}% of the request");
}
```

Example output (numbers vary; the CPU portion is a tiny fraction of the 40 ms wait):

```text
total: 40.04ms
cpu:   1.20µs
cpu is 0.0030% of the request
```

The CPU work is a vanishingly small slice of the request; almost all the time is spent *waiting*. Even an infinitely fast language could only remove that ~0.003%; the response time would be essentially unchanged. This is the central honest caveat: for I/O-bound services, the database, network, and disk dominate, and switching languages for raw speed is the wrong lever. Reach for Rust when your program is *computing*, not when it is *waiting* — and confirm which one it is with [Profiling Rust Applications](/21-performance/00-profiling/) before you commit to a rewrite.

</details>
