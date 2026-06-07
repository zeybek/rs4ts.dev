---
title: "Flame Graphs with cargo-flamegraph"
description: "cargo-flamegraph turns sampled call stacks into a single picture of where your Rust program spends CPU time, like Chrome DevTools or 0x for Node."
---

A flame graph turns thousands of stack samples into a single picture that answers one question fast: *where is my program actually spending its time?* `cargo-flamegraph` makes producing one a single command, and learning to read it gives you the most payoff for the least effort when optimizing Rust.

---

## Quick Overview

A **flame graph** is a visualization of *sampled* call stacks. A profiler interrupts your program many times per second, records the call stack each time, and `cargo-flamegraph` aggregates those samples into stacked, colored bars: each box is a function, its width is the share of samples that function (and everything it called) was on the stack, and boxes stack vertically to show caller → callee. The wider a box, the more time the program spent there, so you scan for the **widest boxes near the top** to find your hot path.

For a TypeScript/JavaScript developer this is the same idea as the **flame chart in Chrome DevTools' Performance panel** or the output of the [`0x`](https://github.com/davidmarkclements/0x) tool over a Node process. The mental model transfers directly. What changes in Rust is the workflow (one Cargo subcommand instead of DevTools), and the fact that you are profiling *native machine code*. There is no garbage collector, no JIT warm-up, and no event-loop layer between you and the CPU, so the stacks you see map cleanly onto the functions you wrote.

> **Note:** This page is about *reading and drilling into* flame graphs. For choosing a profiler (samply, `perf`, Instruments) and setting up release builds with debug symbols, see [Profiling Rust Applications](/21-performance/00-profiling/). For turning a hot path into a precise before/after measurement, see [Benchmarking with Criterion](/21-performance/02-benchmarking/). For the actual fixes once you have found the hot spot, see [Optimization Techniques](/21-performance/03-optimization/).

---

## TypeScript/JavaScript Example

In Node you reach for a sampling profiler when something is slow and you do not yet know *why*. Node ships one in the box — the V8 sampling profiler behind `--prof` — and the `0x` package renders its output as an interactive flame graph in your browser.

Here is a small CPU-bound script: it counts word frequencies across a synthetic corpus. The `normalize` function is doing more work than it looks like.

```typescript
// wordstats.ts — count word frequencies across many documents.
function normalize(word: string): string {
  // Strip non-alphanumeric characters, then lowercase.
  return word
    .split("")
    .filter((c) => /[a-z0-9]/i.test(c))
    .join("")
    .toLowerCase();
}

function wordCounts(docs: string[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const doc of docs) {
    for (const raw of doc.split(/\s+/)) {
      const word = normalize(raw);
      if (word.length === 0) continue;
      counts.set(word, (counts.get(word) ?? 0) + 1);
    }
  }
  return counts;
}

function makeCorpus(nDocs: number, wordsPerDoc: number): string[] {
  const vocab = ["the", "quick", "brown", "fox", "jumps", "over",
    "lazy", "dog", "rust", "typescript", "performance", "flamegraph", "profiler"];
  const docs: string[] = [];
  for (let d = 0; d < nDocs; d++) {
    const parts: string[] = [];
    for (let w = 0; w < wordsPerDoc; w++) {
      parts.push(vocab[(d * 31 + w * 7) % vocab.length]);
    }
    docs.push(parts.join(" "));
  }
  return docs;
}

const docs = makeCorpus(50_000, 200);
const counts = wordCounts(docs);
console.log(`distinct words: ${counts.size}`);
```

To profile it, you produce a V8 log and turn it into a flame graph:

```bash
# Built into Node — no install required. Produces an isolate-*.log file.
node --prof wordstats.js
node --prof-process isolate-*.log > processed.txt   # human-readable summary

# Or, for an interactive browser flame graph:
npx 0x wordstats.js                                  # opens flamegraph.html
```

You then open the flame graph and look for the widest tower. You would discover that `normalize` — and inside it the per-character regex test and the array `split`/`join` — dominates. That is the insight a flame graph gives you: not a number, but a *shape* that points at the guilty function.

---

## Rust Equivalent

The same program in idiomatic Rust, with the same deliberately-wasteful `normalize`:

```rust
// src/main.rs
use std::collections::HashMap;

/// Normalize a word: keep only alphanumeric characters, then lowercase.
/// (Deliberately allocation-heavy so it shows up clearly in a flame graph.)
fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Count how many times each word appears across all documents.
fn word_counts(docs: &[String]) -> HashMap<String, u64> {
    let mut counts = HashMap::new();
    for doc in docs {
        for raw in doc.split_whitespace() {
            let word = normalize(raw);
            if word.is_empty() {
                continue;
            }
            *counts.entry(word).or_insert(0) += 1;
        }
    }
    counts
}

/// Build a corpus of synthetic documents so the program does real work.
fn make_corpus(n_docs: usize, words_per_doc: usize) -> Vec<String> {
    let vocab = [
        "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog",
        "rust", "typescript", "performance", "flamegraph", "profiler",
    ];
    let mut docs = Vec::with_capacity(n_docs);
    for d in 0..n_docs {
        let mut doc = String::new();
        for w in 0..words_per_doc {
            doc.push_str(vocab[(d * 31 + w * 7) % vocab.len()]);
            doc.push(' ');
        }
        docs.push(doc);
    }
    docs
}

fn main() {
    let docs = make_corpus(50_000, 200);
    let counts = word_counts(&docs);

    let mut pairs: Vec<(&String, &u64)> = counts.iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1));

    println!("distinct words: {}", counts.len());
    for (word, count) in pairs.iter().take(3) {
        println!("{word}: {count}");
    }
}
```

Running it confirms it does real work:

```text
distinct words: 13
the: 769232
dog: 769232
over: 769231
```

Now produce a flame graph. First install the tool (it is a normal crate; the current version is `flamegraph` 0.6.12):

```bash
cargo install flamegraph
```

> **Note:** `cargo install flamegraph` gives you the `cargo flamegraph` subcommand. The toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects that edition automatically. On Linux `cargo-flamegraph` drives `perf`; on macOS it drives `dtrace` (which ships with the OS, but the profiler needs elevated privileges, so you run it under `sudo`). See [Profiling Rust Applications](/21-performance/00-profiling/) for the platform setup.

You want optimized code with debug symbols so function names survive. The cleanest way is to keep symbols in release builds. Add this to `Cargo.toml`:

```toml
# Cargo.toml — keep line-table debuginfo in release so flame graph frames are named.
[profile.release]
debug = true
```

Then generate the graph:

```bash
# Build in release, profile the run, write an interactive flamegraph.svg.
cargo flamegraph --bin textstats

# On macOS, dtrace needs privileges:
sudo cargo flamegraph --bin textstats

# Open it (it is a self-contained SVG — clickable and searchable in a browser):
open flamegraph.svg      # macOS;  use `xdg-open` on Linux
```

`cargo-flamegraph` builds the binary in release, runs it under the system profiler, collapses the samples, and writes a `flamegraph.svg` in the current directory. Open that SVG in a browser; it is interactive: click any box to zoom into that subtree, click the "Reset Zoom" text to go back, and use "Search" (top-right) to highlight every frame matching a pattern.

---

## Detailed Explanation

### What the picture means, axis by axis

- **The x-axis is NOT time.** It is the *fraction of collected samples*. Boxes are sorted alphabetically within a level, not chronologically. A box that is 40% of the width means the function was on the stack in ~40% of samples — i.e. the program spent ~40% of its CPU time at or below that frame. You cannot read "this happened, then that happened" left-to-right; a flame graph is a *statistical summary*, not a timeline. (Chrome DevTools' "flame **chart**" *is* time-ordered; cargo-flamegraph's "flame **graph**" is aggregated. Same flames, different x-axis — this trips up DevTools veterans.)
- **The y-axis is stack depth.** The bottom is the entry point (`main`, often wrapped in runtime startup frames); each box above sits on its caller. A tall narrow spire is deep recursion or a long call chain that is *not* itself expensive. A short wide plateau is where the work happens.
- **Width = self + children.** A wide box does not mean *that* function is slow; it means that function plus everything it called is where the time went. To find the function that is itself expensive (its own instructions, not its callees), look for a box that is wide but has little or nothing stacked on top of it. That flat "ceiling" is **self time**.
- **Color is usually meaningless.** The default palette is random warm hues purely so adjacent boxes are distinguishable. Do not read significance into red vs. orange.

### Reading our example

In our flame graph the bottom is `main`, with `word_counts` taking nearly the entire width above it (`make_corpus` runs once and is comparatively thin). Stacked on `word_counts` you would see a wide `normalize` tower, and on top of *that*, frames for `core::str::...to_lowercase`, the iterator/`filter` chain, and, most tellingly, `alloc`/`__rust_alloc` and `String` growth frames. Those allocation frames near the top are the smoking gun: `normalize` allocates a fresh `String` for the `collect`, then `to_lowercase` allocates *another* `String`, for every word in every document (10 million words here). The flame graph shows allocation as a real, measurable plateau rather than something you have to guess at.

### From picture to fix

Once the graph fingers `normalize`, the fix is to stop allocating per word. Lowercase the characters *as you filter*, and reuse one scratch buffer across the whole loop instead of returning a new `String` each call:

```rust
// src/bin/opt.rs — optimized: one reusable buffer, no per-word allocation.
use std::collections::HashMap;

/// Normalize into a caller-owned buffer: lowercase while filtering, no temp String.
fn normalize_into(word: &str, buf: &mut String) {
    buf.clear();
    for c in word.chars().filter(|c| c.is_alphanumeric()) {
        buf.extend(c.to_lowercase()); // to_lowercase yields chars; extend appends them
    }
}

fn word_counts(docs: &[String]) -> HashMap<String, u64> {
    let mut counts = HashMap::new();
    let mut buf = String::new(); // allocated ONCE, reused for every word
    for doc in docs {
        for raw in doc.split_whitespace() {
            normalize_into(raw, &mut buf);
            if buf.is_empty() {
                continue;
            }
            // Only allocate a new key String on the FIRST sight of a word.
            if let Some(n) = counts.get_mut(buf.as_str()) {
                *n += 1;
            } else {
                counts.insert(buf.clone(), 1u64);
            }
        }
    }
    counts
}

fn make_corpus(n_docs: usize, words_per_doc: usize) -> Vec<String> {
    let vocab = [
        "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog",
        "rust", "typescript", "performance", "flamegraph", "profiler",
    ];
    let mut docs = Vec::with_capacity(n_docs);
    for d in 0..n_docs {
        let mut doc = String::new();
        for w in 0..words_per_doc {
            doc.push_str(vocab[(d * 31 + w * 7) % vocab.len()]);
            doc.push(' ');
        }
        docs.push(doc);
    }
    docs
}

fn main() {
    let docs = make_corpus(50_000, 200);
    let counts = word_counts(&docs);
    println!("distinct words: {}", counts.len());
}
```

This keeps the same output (`distinct words: 13`). The honest measurement on this machine (best of five runs, user CPU time) went from about **0.82 s to 0.75 s**: a real but modest win, because the corpus has only 13 distinct words so the `get_mut` fast path already avoids most key allocations. A flame graph of the optimized binary shows the `normalize_into` plateau shrink and the `__rust_alloc` frames atop it nearly vanish. That visual confirmation — *the box you targeted got narrower* — is exactly what you want from a profiler-guided change, and it is why you re-profile after every fix.

> **Tip:** A flame graph tells you *where* time goes; it does not give you a trustworthy *number* for a small change. Confirm the actual speedup with a microbenchmark — see [Benchmarking with Criterion](/21-performance/02-benchmarking/) and its `black_box` discussion so the optimizer does not delete the work you are measuring.

---

## Key Differences

| Concept | Chrome DevTools / `0x` (Node) | cargo-flamegraph (Rust) |
|---|---|---|
| What you profile | JS bundle through V8 + JIT + event loop | Native machine code, no GC/JIT layer |
| How to capture | DevTools "Record"; `node --prof`; `npx 0x app.js` | `cargo flamegraph --bin app` |
| Backend sampler | V8 sampling profiler | `perf` (Linux) / `dtrace` (macOS) / DTrace-likes |
| x-axis | DevTools flame **chart** is time-ordered; `0x` is aggregated | Aggregated samples (NOT time) |
| Symbol quality | Function names from source maps; inlined/JIT frames can be fuzzy | Real symbols if `debug = true`; inlining can merge frames |
| GC noise | Garbage-collection stacks show up as their own towers | No GC; allocation shows as `malloc`/`__rust_alloc` frames |
| Build step | None (interpreted/JIT) | Must build **release + debuginfo** or names are stripped |

The deepest difference is what the wide boxes *mean*. In Node, a fat tower might be V8's garbage collector reclaiming objects you allocated: work you cannot see in your own source. In Rust there is no GC, so a fat allocation tower (`__rust_alloc`, `realloc`, `String`/`Vec` growth) points at *your* `clone()`, `collect()`, or `format!` calls directly. The flame graph attributes the cost to the exact line you wrote, which makes the fix far more obvious.

> **Note:** Unlike a DevTools recording, a cargo-flamegraph graph has no zoom-to-time-range and no "bottom-up" table by default — it is one static (but clickable) SVG. If you want an aggregated *bottom-up* "which function has the most self time" table, a profiler like [samply](/21-performance/00-profiling/) gives you that view alongside the flame graph.

---

## Common Pitfalls

### 1. Profiling a debug build (or release without symbols)

If you run `cargo flamegraph` on a default debug build, the numbers are meaningless: debug code is 10–100× slower than release and the bottleneck shifts to things that vanish under optimization. Conversely, a default *release* build strips debug info, so every box reads as a bare address or `[unknown]` and the graph is unreadable. The correct combination is **optimized code with line-table debug info**, which is why you add `[profile.release] debug = true` to `Cargo.toml`. `cargo flamegraph` builds release by default; the `debug = true` line is what keeps the names.

### 2. Reading the x-axis as a timeline

The boxes are sorted alphabetically, not by when they ran. A function that appears "to the left" did not run "first." If you need ordering — "the slow request comes *after* the cache miss" — a flame graph is the wrong tool; reach for a tracing/timeline view instead. Treat the flame graph purely as "where does the aggregate time live?"

### 3. Blaming a wide box for time its children spent

A wide `word_counts` box does not mean `word_counts`'s own code is slow; almost all of that width is `normalize` underneath it. The function whose *own instructions* are hot is the one with a flat top (little stacked above it). Always drill *up* the tower to find the real self-time plateau before you start editing code.

### 4. Inlining makes your hot function disappear

The release optimizer inlines small functions into their callers, so the frame you expected to see (`normalize`) can be fused into `word_counts` and never appear as its own box. If a function you suspect is missing from the graph, mark it `#[inline(never)]` *temporarily* so it shows up as a distinct frame:

```rust
// Force a distinct frame so the profiler attributes time to THIS function,
// instead of inlining it into its caller. Remove once you are done profiling.
#[inline(never)]
fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

fn main() {
    for w in ["Rust!", "FLAMEGRAPH", "  fox,"] {
        println!("{w} -> {}", normalize(w));
    }
}
```

```text
Rust! -> rust
FLAMEGRAPH -> flamegraph
  fox, -> fox
```

This is a *diagnostic* aid, not an optimization: `#[inline(never)]` usually makes code slower, so delete it once you have your picture.

### 5. The program finishes too fast to sample

A sampler that fires ~1000 times/second collects almost nothing from a 5 ms program, and the graph is pure noise. Make the workload long enough to gather thousands of samples — loop the operation, feed a bigger input (as our 50,000-document corpus does), or profile a representative workload rather than a toy. On macOS you also need to run under `sudo` because `dtrace` requires elevated privileges; forgetting that yields an empty or permission-denied graph rather than a real one.

---

## Best Practices

- **Measure before you touch code.** Generate a flame graph *first* and let the widest box pick the target. Optimizing a box that is 2% of the width can never beat optimizing the one that is 50%. (See [When to Optimize](/21-performance/10-when-to-optimize/).)
- **Always release + `debug = true`.** Keep a `[profile.release] debug = true` (or a dedicated profiling profile) so any release binary is ready to flame-graph without a rebuild dance.
- **Profile a realistic workload.** Feed production-like input sizes. A flame graph of a 10-element input lies about what matters at 10 million.
- **Drill into self time.** Find the flat ceilings (wide box, little on top) — those are the functions whose own code is hot and where edits pay off.
- **Use Search to total a pattern.** In the SVG, search for `alloc` (or `clone`, `memcpy`, `Drop`) to highlight every matching frame and read the combined percentage at the bottom-right. This is how you quantify "how much am I spending on allocation/cloning?" across the whole program at once.
- **Re-profile after each change** and confirm the targeted box shrank. Pair the graph with a [criterion](/21-performance/02-benchmarking/) benchmark for a trustworthy number.
- **One change at a time.** If you fix three things and re-profile, you cannot attribute the win. Flame-graph-driven optimization is a loop: graph → one hypothesis → one fix → graph again.

---

## Real-World Example

A common production hot spot is a request handler that serializes a large response. Here a service builds a JSON report from rows; a naive version re-formats and re-allocates per row, and a flame graph immediately shows the `format!`/allocation tower. This is a self-contained program you can flame-graph end to end.

```rust
// src/main.rs — build a CSV-style report from many records.
// Run: cargo flamegraph --bin report   (with [profile.release] debug = true)

/// A row of telemetry, as a service might pull from a database.
struct Record {
    id: u64,
    region: String,
    latency_ms: f64,
    ok: bool,
}

fn make_records(n: usize) -> Vec<Record> {
    let regions = ["us-east", "us-west", "eu-central", "ap-south"];
    (0..n)
        .map(|i| Record {
            id: i as u64,
            region: regions[i % regions.len()].to_string(),
            latency_ms: (i % 250) as f64 + 0.5,
            ok: i % 17 != 0,
        })
        .collect()
}

/// Naive: a fresh String per row plus `format!` allocations everywhere.
/// In a flame graph this is a wide tower of `__rust_alloc` / `format!`.
fn render_naive(records: &[Record]) -> String {
    let mut out = String::new();
    for r in records {
        let line = format!(
            "{},{},{:.1},{}\n",
            r.id,
            r.region,
            r.latency_ms,
            if r.ok { "ok" } else { "err" }
        );
        out.push_str(&line); // the temporary `line` String is allocated then dropped
    }
    out
}

/// Optimized: preallocate the output, write in place, no per-row temporary.
fn render_fast(records: &[Record]) -> String {
    use std::fmt::Write; // brings `write!` for String into scope
    // Reserve a generous estimate so the buffer rarely reallocates.
    let mut out = String::with_capacity(records.len() * 32);
    for r in records {
        let _ = write!(
            out,
            "{},{},{:.1},{}\n",
            r.id,
            r.region,
            r.latency_ms,
            if r.ok { "ok" } else { "err" }
        );
    }
    out
}

fn main() {
    let records = make_records(2_000_000);

    let a = render_naive(&records);
    let b = render_fast(&records);

    // Same output — the optimization is purely about how we build the string.
    assert_eq!(a.len(), b.len());
    println!("rendered {} bytes from {} records", b.len(), records.len());
}
```

Running it confirms both renderers agree:

```text
rendered 50126538 bytes from 2000000 records
```

Flame-graph this binary and you will see `render_naive` dominated by a `format!` + `__rust_alloc` tower (one allocation per row, two million times) plus the `Drop` of each temporary `String`. Search the SVG for `alloc` to see the combined percentage. After switching the call to `render_fast`, re-profile: the per-row allocation plateau collapses because the output `String` is allocated once (`with_capacity`) and `write!` formats directly into it. The flame graph makes the difference between "allocate per row" and "allocate once" visible as a shrinking box: the whole point of profiling before *and* after.

> **Tip:** `write!(out, ...)` into an existing `String` is the standard Rust idiom for "format without a temporary." It returns a `Result` (the `fmt::Write` impl for `String` never actually fails), which is why we discard it with `let _ =`. See [Optimization Techniques](/21-performance/03-optimization/) for more allocation-avoidance patterns.

---

## Further Reading

- [The Rust Performance Book — Profiling](https://nnethercote.github.io/perf-book/profiling.html): flame graphs in the broader profiling workflow.
- [cargo-flamegraph (flamegraph crate) README](https://github.com/flamegraph-rs/flamegraph): install, platform setup, and every CLI flag (`--bin`, `--bench`, `--root`, `--`, `-o`, `--dev`).
- [Brendan Gregg — Flame Graphs](https://www.brendangregg.com/flamegraphs.html): the original technique and how to read it, from its inventor.
- [`0x` for Node.js](https://github.com/davidmarkclements/0x): the closest TypeScript/JavaScript equivalent, for comparison.
- Related sections in this guide:
  - [Profiling Rust Applications](/21-performance/00-profiling/): picking a profiler and configuring release-with-debuginfo builds.
  - [Benchmarking with Criterion](/21-performance/02-benchmarking/): turning a flame-graph finding into a trustworthy measurement with criterion.
  - [Optimization Techniques](/21-performance/03-optimization/): the actual fixes: clones, allocations, `&str` over `String`, capacity.
  - [When to Optimize](/21-performance/10-when-to-optimize/): measure first; avoid premature optimization.
  - [Performance](/21-performance/09-comparison/): honest Rust-vs-Node.js performance picture.
  - [Getting Started](/01-getting-started/) and [Basics](/02-basics/): if `cargo` and the toolchain are new to you.
  - [Common Patterns](/22-common-patterns/): idioms that tend to keep your flame graphs flat.

---

## Exercises

### Exercise 1: Read a graph before you build one

**Difficulty:** Beginner

**Objective:** Practice the core skill — interpreting the shape — without any tooling.

**Instructions:** Suppose a flame graph shows, from bottom to top: `main` (100% width) → `process_orders` (95%) → and stacked on `process_orders` two boxes side by side, `validate` (15%) and `serialize` (78%). On top of `serialize` sits `__rust_alloc` (70%) with nothing above it. Answer in prose: (a) Which function should you investigate first, and why? (b) Is `process_orders`'s *own* code likely the problem? (c) What does the `__rust_alloc` ceiling at 70% strongly suggest the fix is?

<details>
<summary>Solution</summary>

(a) **`serialize`** (and what it calls). It is 78% of the width, far more than `validate`'s 15%, so the overwhelming majority of CPU time lives in the `serialize` subtree. Optimizing `validate` could at best save 15%; `serialize` is where the payoff is.

(b) **No.** `process_orders` is 95% wide only because its children (`validate` + `serialize`) are wide. It has almost nothing stacked *as its own ceiling*, so its self time is tiny; it is just a router that calls the expensive work.

(c) An `__rust_alloc` box that is 70% wide *with nothing above it* means the program is spending ~70% of its time in the allocator itself (self time in `malloc`/grow). That points squarely at **excessive heap allocation inside `serialize`** — repeated `String`/`Vec` allocation, `clone()`, `collect()`, or `format!` per item. The fix is to allocate once (e.g. `String::with_capacity` + `write!`) and reuse buffers, as in the Real-World Example above. After the fix, re-profile: that ceiling should shrink dramatically.

</details>

### Exercise 2: Generate a flame graph and find the hot spot

**Difficulty:** Intermediate

**Objective:** Run the full cargo-flamegraph loop on a real program and identify the guilty function from the SVG.

**Instructions:** Create a binary that, given a `Vec<String>` of `200_000` lines, counts how many lines contain a given substring by building the lowercased version of *each line* with `to_lowercase()` and calling `.contains()`. Add `[profile.release] debug = true` to `Cargo.toml`, run `cargo flamegraph --bin <name>` (use `sudo` on macOS), open the SVG, and identify which standard-library function forms the widest plateau. Write down what you observe.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
/// Count lines that contain `needle`, case-insensitively.
/// The per-line `to_lowercase()` allocation is the hot spot.
fn count_matches(lines: &[String], needle: &str) -> usize {
    let needle = needle.to_lowercase();
    let mut hits = 0;
    for line in lines {
        // Allocates a brand-new lowercased String for EVERY line — the bottleneck.
        if line.to_lowercase().contains(&needle) {
            hits += 1;
        }
    }
    hits
}

fn make_lines(n: usize) -> Vec<String> {
    let words = ["RUST", "Performance", "FlameGraph", "tokio", "Profiler", "node"];
    (0..n)
        .map(|i| {
            let mut s = String::new();
            for k in 0..8 {
                s.push_str(words[(i + k) % words.len()]);
                s.push(' ');
            }
            s
        })
        .collect()
}

fn main() {
    let lines = make_lines(200_000);
    let hits = count_matches(&lines, "flamegraph");
    println!("matching lines: {hits}");
}
```

```toml
# Cargo.toml
[profile.release]
debug = true
```

```bash
cargo flamegraph --bin matches      # add `sudo` on macOS
open flamegraph.svg
```

**What you observe:** The widest plateau (its own flat ceiling) is inside `str::to_lowercase` and the `__rust_alloc` frames beneath the per-line allocation. `count_matches`'s own box is wide but only because of what it calls. The fix (an optimization, not part of this exercise) is to lowercase into a reused buffer or, better, do a case-insensitive comparison without allocating per line. The output is `matching lines: 200000` because every generated line contains the word.

> Verified to compile and run (the binary prints `matching lines: 200000`). Your exact SVG layout depends on the OS sampler, but `to_lowercase` + allocation will be the dominant tower.

</details>

### Exercise 3: Confirm the fix on the flame graph and the clock

**Difficulty:** Advanced

**Objective:** Close the optimization loop — fix the Exercise 2 hot spot, re-profile, and verify both the box shrank *and* the program got faster.

**Instructions:** Rewrite `count_matches` to avoid allocating a new `String` per line. Reuse one scratch buffer, lowercasing each line into it with the `to_lowercase` char iterator, then call `.contains()`. Keep the result identical. Re-run `cargo flamegraph` and compare; also time both versions (`/usr/bin/time` or a quick loop) to confirm a real speedup.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
/// Reuse one buffer across all lines: lowercase into `buf`, then search it.
fn count_matches(lines: &[String], needle: &str) -> usize {
    let needle = needle.to_lowercase();
    let mut buf = String::new(); // allocated once, reused every iteration
    let mut hits = 0;
    for line in lines {
        buf.clear();
        // `to_lowercase()` on a char yields chars; `extend` appends without a temp String.
        buf.extend(line.chars().flat_map(|c| c.to_lowercase()));
        if buf.contains(&needle) {
            hits += 1;
        }
    }
    hits
}

fn make_lines(n: usize) -> Vec<String> {
    let words = ["RUST", "Performance", "FlameGraph", "tokio", "Profiler", "node"];
    (0..n)
        .map(|i| {
            let mut s = String::new();
            for k in 0..8 {
                s.push_str(words[(i + k) % words.len()]);
                s.push(' ');
            }
            s
        })
        .collect()
}

fn main() {
    let lines = make_lines(200_000);
    let hits = count_matches(&lines, "flamegraph");
    println!("matching lines: {hits}");
}
```

```bash
cargo build --release
# Time the optimized binary (best of a few runs):
for i in 1 2 3; do /usr/bin/time ./target/release/matches; done
```

**What you should see:** The output is still `matching lines: 200000`. On the new flame graph the per-line `__rust_alloc` plateau under `count_matches` shrinks sharply because the buffer is allocated once instead of 200,000 times; the remaining cost is the unavoidable `char::to_lowercase` work. The wall-clock/user time drops correspondingly. For a *trustworthy* speedup number rather than an eyeballed one, wrap both versions in a [criterion](/21-performance/02-benchmarking/) benchmark with `black_box` so the optimizer cannot elide the search.

> This solution compiles and runs (prints `matching lines: 200000`). The exact timing depends on your machine, but reusing the buffer measurably beats allocating per line.

</details>
