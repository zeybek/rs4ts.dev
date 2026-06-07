---
title: "Performance"
sidebar:
  label: "Overview"
description: "Measure before you optimize: profiling, flame graphs, criterion benchmarks, allocation and cache wins, plus an honest Rust-vs-Node.js performance comparison."
---

Rust gives you C-class performance by default, but "by default" is not the same as "automatically optimal." This section is about *measuring* before you optimize, then applying the techniques that matter: profiling and flame graphs to find the real hot spots, criterion for statistically honest benchmarks, and concrete optimizations around allocation, memory layout, and cache behavior. It closes with a clear-eyed comparison against Node.js: where Rust wins, by how much, and the caveats an experienced engineer should keep in mind.

---

## What You'll Learn

- How to **profile** a Rust program (samply/perf/Instruments) and read a **flame graph** to find where time actually goes
- How to write **statistically rigorous benchmarks** with criterion, and why `black_box` matters
- The highest-impact **optimizations**: avoiding needless `clone`/allocation, taking `&str`/`&[T]`, and letting iterators fuse
- How **memory layout** (field ordering, `#[repr]`, niche optimization, enum size) affects both size and speed
- Writing **cache-friendly** code: data-oriented design, struct-of-arrays vs array-of-structs, and contiguity
- Why Rust's **zero-cost abstractions** really are zero-cost: iterators and closures compiling to the same code as hand-written loops
- How to cut **compile time** and **binary size** when they matter
- An **honest performance comparison** with Node.js/TypeScript, and when *not* to optimize at all

---

## Topics

| Topic | Description |
| --- | --- |
| [Profiling](/21-performance/00-profiling/) | Profiling Rust apps with samply/perf/Instruments; finding hot spots; release builds with debug info. |
| [Flame Graphs](/21-performance/01-flamegraph/) | Generating and reading flame graphs with `cargo-flamegraph`. |
| [Benchmarking](/21-performance/02-benchmarking/) | Statistically driven micro-benchmarks with criterion; groups, parameters, and `black_box`. |
| [Optimization Techniques](/21-performance/03-optimization/) | Avoiding clones/allocations, borrowing over owning, and letting the iterator chain do less work. |
| [Memory Layout](/21-performance/04-memory-layout/) | Struct field ordering, size/align, `#[repr]`, niche optimization, and enum sizes. |
| [Cache Efficiency](/21-performance/05-cache-efficiency/) | Cache-friendly, data-oriented code: SoA vs AoS and why contiguity wins. |
| [Zero-Cost Abstractions](/21-performance/06-zero-cost/) | How iterators and closures compile down to the same machine code as manual loops. |
| [Compilation Time](/21-performance/07-compilation-time/) | Reducing compile time: workspaces, generics, codegen-units, and caching. |
| [Binary Size](/21-performance/08-binary-size/) | Shrinking binaries: `opt-level = "z"`, LTO, `strip`, `panic = "abort"`, and `cargo-bloat`. |
| [Performance vs Node.js](/21-performance/09-comparison/) | Where Rust beats Node.js (CPU, memory, no GC pauses) — and the honest caveats. |
| [When to Optimize](/21-performance/10-when-to-optimize/) | Measure first: premature optimization, and choosing readable-then-fast. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Profile a Rust program and use a flame graph to locate the real bottleneck instead of guessing
- Write criterion benchmarks that produce trustworthy numbers and resist compiler elision
- Remove avoidable allocations and clones, and reason about when borrowing beats owning
- Lay out data for size and cache-friendliness, and explain why an iterator chain is not slower than a loop
- Tune compile time and binary size when a project needs it
- Make a fair, defensible performance comparison with a Node.js implementation, and decide when optimization is not worth it

---

## Prerequisites

- [Section 13: Testing](/13-testing/) — benchmarking with criterion builds directly on the testing/Cargo workflow, and you will run benches alongside tests.
- [Section 07: Collections](/07-collections/) — most optimization work is about iterators, `Vec`/`String` allocation, and capacity, so be comfortable with those first.
- [Section 05: Ownership](/05-ownership/) — "avoid the clone" only makes sense once moves, borrows, and `Clone` are second nature.

---

## Estimated Time

- **Reading:** 6 hours
- **Hands-on Practice:** 5 hours
- **Exercises:** 3 hours
- **Total:** 14 hours

> **Tip:** Resist optimizing until you have a profile. Read `when-to-optimize` and `profiling` *first*, get a flame graph of real code, and only then reach for the techniques in `optimization`, `memory-layout`, and `cache-efficiency`. The single most common mistake a fast-language newcomer makes is optimizing the wrong 90%.

---

**Next:** [Section 22: Common Patterns →](/22-common-patterns/) — idiomatic Rust design patterns and how they differ from their object-oriented TypeScript counterparts.
