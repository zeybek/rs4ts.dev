---
title: "Profiling Rust Applications"
description: "Profile the optimized binary users run, not the debug build. samply, perf, and Instruments find Rust hot spots, the way node --prof does for JavaScript."
---

When a Rust program is slower than you expect, the first instinct of many TypeScript developers is to start rewriting code. Resist it. A **profiler** tells you where the time actually goes, so you change the 5% of code that matters instead of guessing. This topic covers sampling profilers (`samply`, `perf`, Instruments), how to read their output to find **hot spots**, and the one build setting that makes Rust profiles readable: a release build that still carries debug info.

---

## Quick Overview

A **profiler** observes a running program and reports which functions consume CPU time. The dominant approach for native code is **sampling**: the profiler interrupts the program hundreds or thousands of times per second and records the call stack each time. Functions that appear in many samples are your **hot spots**.

For a TypeScript/JavaScript developer, the mental model is familiar — it is the same idea as the Chrome DevTools or `node --prof` CPU profiler — but the tooling is different and, importantly, it works on the *optimized* machine code your users actually run. The catch: an optimized Rust binary normally throws away the symbol names a profiler needs, so you must explicitly keep **debug info** in your release build.

> **Note:** Profiling answers "*where* is the time spent?" Benchmarking answers "*how much faster* is version B than version A?" They are complementary. Benchmarking lives in [Benchmarking with Criterion](/21-performance/02-benchmarking/); deciding whether to profile at all lives in [When to Optimize](/21-performance/10-when-to-optimize/).

---

## TypeScript/JavaScript Example

In Node.js you profile by asking the V8 engine to record a CPU profile, then opening it in a viewer. A common workflow:

```typescript
// wordFreq.ts — count word frequencies, then report the top N.
import { readFileSync } from "node:fs";

function normalize(word: string): string {
  // Strip non-alphanumerics, then lowercase.
  return word.replace(/[^a-z0-9]/gi, "").toLowerCase();
}

function wordFrequencies(text: string): Map<string, number> {
  const counts = new Map<string, number>();
  for (const line of text.split("\n")) {
    for (const raw of line.split(/\s+/)) {
      const word = normalize(raw);
      if (word.length === 0) continue;
      counts.set(word, (counts.get(word) ?? 0) + 1);
    }
  }
  return counts;
}

function topN(counts: Map<string, number>, n: number): [string, number][] {
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, n);
}

const corpus = readFileSync("corpus.txt", "utf8");
const counts = wordFrequencies(corpus);
console.log(topN(counts, 3));
```

> **Tip:** To reproduce this you need a `corpus.txt` large enough to give the profiler real work. Generate the same 50,000-line input the Rust version below synthesizes in code with one shell line: `yes 'the quick brown fox jumps over the lazy dog THE Fox, the Dog! the the the brown brown' | head -n 50000 > corpus.txt`. Both sides then print the same top three: `the: 350000`, `brown: 150000`, `dog: 100000`.

You profile it with the built-in V8 profiler:

```bash
# Records an isolate-*.log of every V8 tick, then turns it into a flat report.
node --prof dist/wordFreq.js
node --prof-process isolate-*.log > profile.txt
```

`profile.txt` lists ticks per function. Or you launch with `node --inspect`, open `chrome://inspect`, and read a flame chart in DevTools. Either way you are sampling the JIT-compiled code that V8 is *currently* running, which may change as the JIT re-optimizes.

---

## Rust Equivalent

The Rust workflow has the same shape — run under a sampling profiler, then read where the time went — but you profile a single, statically compiled binary. We will use [`samply`](https://github.com/mstange/samply), a cross-platform sampling profiler that opens results in the Firefox Profiler UI in your browser. It works the same on macOS, Linux, and Windows, which makes it the easiest starting point for a former Node developer.

Here is the equivalent program. Save it as `src/main.rs` in a fresh project (`cargo new word_freq`):

```rust
use std::collections::HashMap;

/// Normalize a word: keep alphanumeric characters, then lowercase.
/// (Written the "obvious" first way — we will profile it and improve it.)
fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Count word frequencies in a body of text.
fn word_frequencies(text: &str) -> HashMap<String, u32> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for line in text.lines() {
        for raw in line.split_whitespace() {
            let word = normalize(raw);
            if word.is_empty() {
                continue;
            }
            *counts.entry(word).or_insert(0) += 1;
        }
    }
    counts
}

/// Return the top-N words by descending frequency, ties broken alphabetically.
fn top_n(counts: &HashMap<String, u32>, n: usize) -> Vec<(String, u32)> {
    let mut pairs: Vec<(String, u32)> =
        counts.iter().map(|(w, c)| (w.clone(), *c)).collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    pairs.truncate(n);
    pairs
}

fn main() {
    // Build a large synthetic corpus so the work is measurable.
    let sample = "the quick brown fox jumps over the lazy dog \
                  THE Fox, the Dog! the the the brown brown";
    let mut corpus = String::new();
    for _ in 0..50_000 {
        corpus.push_str(sample);
        corpus.push('\n');
    }

    let counts = word_frequencies(&corpus);
    let top = top_n(&counts, 3);

    println!("unique words: {}", counts.len());
    for (word, count) in &top {
        println!("{word}: {count}");
    }
}
```

Running it produces, exactly:

```text
unique words: 8
the: 350000
brown: 150000
dog: 100000
```

Before you can profile it usefully, configure the build (next section), then:

```bash
# Install samply once (it is a normal cargo-installed CLI tool):
cargo install --locked samply

# Build optimized + with debug info, then record and open the profile:
cargo build --profile profiling
samply record ./target/profiling/word_freq
```

`samply record` runs the program to completion and opens the Firefox Profiler in your browser with a flame graph and a per-function ("call tree") view of where CPU time was spent.

---

## Detailed Explanation

### Why a release build, and why with debug info

This is the single most important rule of profiling Rust, and the one most former JavaScript developers get wrong.

A **debug build** (plain `cargo build` / `cargo run`) applies essentially no optimization. It is full of redundant copies, un-inlined helper calls, and integer-overflow checks. Profiling it tells you about overhead that *will not exist in production*. You would optimize the wrong things.

The runtime difference is dramatic. The same program above, run with `/usr/bin/time -p`:

```text
# cargo run            (debug build)
real 0.97
user 0.60

# ./target/release/...  (release build)
real 0.25
user 0.06
```

The release build was roughly **4x faster** here, and that gap is entirely build configuration, not a code change. Always profile something that resembles what ships.

But a default **release build** (`cargo build --release`) strips the line-number and function-name tables a profiler needs, so a flame graph becomes a wall of hexadecimal addresses. The fix is to keep **debug info** in the optimized binary. Add this to `Cargo.toml`:

```toml
[profile.release]
debug = true        # keep DWARF debug info in the optimized binary

# A dedicated profile for profiling: optimized like release, never stripped.
[profile.profiling]
inherits = "release"
debug = true
strip = false
```

> **Tip:** `debug = true` does **not** make the binary slower; the optimizer still runs at full strength. It only makes the file larger and keeps the symbol/line tables. You can ship `strip = true` for releases and use the separate `profiling` profile (above) when you need symbols. Splitting it out also means you don't have to remember to flip a flag back.

You can confirm the flag actually reaches the compiler with a verbose rebuild:

```bash
touch src/main.rs
cargo build --release -v | grep -o 'debuginfo=[0-9a-z-]*'
```

```text
debuginfo=2
debuginfo=unpacked
```

`debuginfo=2` is full DWARF (functions *and* line numbers). On macOS you also see `split-debuginfo=unpacked`, which matters for where the data lives. See the macOS note below.

### How sampling profilers find hot spots

`samply record ./binary` starts your program and, many times per second, pauses it just long enough to walk the current call stack. After the run it aggregates: a function that appeared in 6,000 of 10,000 samples was on the CPU about 60% of the time. The Firefox Profiler shows this two ways:

- **Flame graph** — width is proportional to time; the widest boxes are your hot spots. (Reading flame graphs in depth is covered in [Flame Graphs with cargo-flamegraph](/21-performance/01-flamegraph/).)
- **Call tree / inverted stack** — a sortable list of functions by self-time and total-time, closer to the `node --prof-process` text report you may know.

For our `word_freq` program you would expect `normalize` and the allocator (`malloc`/`free`, called via `String` allocation) to dominate, because `normalize` allocates a fresh `String` for *every word* and `to_lowercase` allocates *again*. That is the hot spot to attack, and the fix (reusing a buffer, avoiding repeated allocation) lives in [Optimization Techniques](/21-performance/03-optimization/).

### Two kinds of "self time"

When you read the report, distinguish:

- **Self time:** time spent in a function's *own* instructions.
- **Total time:** self time *plus* everything it called.

`main` always has near-100% total time (it calls everything) but tiny self time. You optimize functions with high *self* time, or a high-total-time function whose children you cannot change.

### samply vs perf vs Instruments

You have three good sampling profilers depending on your platform; they differ in mechanism but the workflow ("run, then read a flame graph") is the same.

| Tool          | Platform               | How you launch it                         | Where results open                   |
| ------------- | ---------------------- | ----------------------------------------- | ------------------------------------ |
| `samply`      | macOS, Linux, Windows  | `samply record ./binary`                  | Firefox Profiler (browser, local)    |
| `perf`        | Linux only             | `perf record ./binary` then `perf report` | Terminal TUI, or feeds flame graphs  |
| Instruments   | macOS only             | Xcode → Instruments → Time Profiler       | Native macOS GUI                     |

- **`samply`** is the recommended default: one `cargo install`, no `sudo`, identical experience across operating systems, and it understands Rust's mangled symbols out of the box. It uses the OS facilities under the hood (`perf_event_open` on Linux, `task_for_pid`/sampling on macOS).
- **`perf`** is the venerable Linux kernel profiler. It is extremely capable (hardware counters, cache misses, branch misses) but Linux-only and sometimes needs `sudo` or a `sysctl kernel.perf_event_paranoid` tweak. To sample an existing running service, use `perf record -p <pid>`.
- **Instruments** ships with Xcode on macOS. Its "Time Profiler" template gives a polished GUI and integrates with the rest of the Apple tooling. Point it at `target/profiling/<binary>`.

> **Note:** Whatever the tool, the build configuration is the same: optimized binary, debug info kept. A profiler cannot invent symbols that the compiler discarded.

### Where debug info lives on macOS (a real gotcha)

On Linux, `debug = true` embeds DWARF sections directly in the executable. On macOS, the default `split-debuginfo=unpacked` leaves the DWARF inside the per-unit `.o` object files in `target/`, and the final binary only carries *references* to them (`N_OSO` stab entries). `samply` and Instruments follow those references automatically, so profiling Just Works, **as long as you do not delete `target/`** between building and profiling.

If you want a self-contained debug bundle (for example, to profile a binary copied to another machine), collect the DWARF into a `.dSYM`:

```bash
dsymutil ./target/profiling/word_freq
# produces ./target/profiling/word_freq.dSYM next to the binary
```

That bundle is exactly what Instruments and `samply` look for beside the executable.

---

## Key Differences

| Concept                  | Node.js / TypeScript                                  | Rust                                                       |
| ------------------------ | ----------------------------------------------------- | ---------------------------------------------------------- |
| What you profile         | JIT-compiled bytecode, re-optimized at runtime        | One statically compiled, ahead-of-time-optimized binary    |
| Getting symbols          | Always present; V8 owns the symbol map                | **Opt in** via `debug = true`; release strips them by default |
| Build to profile         | The same `.js` you run                                | A **release-like** build, never the debug build            |
| Launch mechanism         | `node --prof` / `--inspect` flag                      | External profiler wraps the binary: `samply record ./bin`  |
| Primary viewer           | Chrome DevTools / `--prof-process` text               | Firefox Profiler (samply), `perf report`, or Instruments   |
| GC time in the profile   | Visible and often significant (V8 GC frames)          | No GC; you see `malloc`/`free` only where *you* allocate    |
| Inlining surprises       | JIT inlines opaquely; frames can vanish               | LLVM inlines aggressively; a hot fn may fold into its caller |

The deepest difference is the absence of a garbage collector. In a Node CPU profile you routinely see GC frames stealing time at unpredictable moments. A Rust profile has no GC frames at all; allocation cost shows up only as the `malloc`/`free` you triggered by constructing `String`s, `Vec`s, and `Box`es. That makes Rust profiles easier to reason about: the time is *yours*, attributable to specific lines.

> **Warning:** Aggressive inlining can make a hot function "disappear" from the flame graph because LLVM folded it into its caller. If you can't find a function you *know* is hot, look at the caller, or temporarily add `#[inline(never)]` to that function for the duration of a profiling session to force a distinct frame.

---

## Common Pitfalls

### Profiling the debug build

The number-one mistake. You profile `cargo run`, see `core::ops` and overflow-check machinery everywhere, and "optimize" code that the release optimizer would have erased. Always build with optimizations on. If your profiler shows suspiciously slow arithmetic or tons of tiny un-inlined helper calls, you are almost certainly looking at a debug build.

### Forgetting debug info, then reading hex addresses

A plain `cargo build --release` strips symbols. Your flame graph becomes:

```text
0x0000000100003a1c
0x0000000100003b40
0x0000000100003c08
```

unreadable. The fix is the `debug = true` profile setting shown above. If you only realize this *after* recording, you must rebuild with debug info and record again; the missing names cannot be recovered from the stripped binary.

### Typos in the profile name

Profiles must be defined before you can `--profile` them. Asking for one that doesn't exist is a hard error:

```bash
cargo build --profile prof
```

```text
error: profile `prof` is not defined
```

The fix is to define `[profile.prof]` (or use the correct name, e.g. `profiling`). Built-in profiles are `dev`, `release`, `test`, and `bench`; any other name must be declared and must `inherits = "release"` or `inherits = "dev"`.

### Deleting `target/` between build and profile (macOS)

Because macOS keeps DWARF in `target/.../deps/*.o`, running `cargo clean` after building but before profiling leaves the profiler unable to symbolize. Build, then profile, then clean — in that order. Or run `dsymutil` to bake a standalone `.dSYM`.

### Sample count too low to be meaningful

A program that runs for 30 milliseconds gives the profiler only a handful of samples: pure noise. If your hot path is fast, run it in a loop, feed it a larger input (as the example's 50,000-line corpus does), or use a benchmark tool like `criterion` ([Benchmarking with Criterion](/21-performance/02-benchmarking/)) instead of a one-shot profiler.

### Mistaking total time for self time

Seeing `word_frequencies` at "95%" does not mean *it* is slow; it means it and everything it calls take 95%. Sort by **self time** to find the function whose own instructions are the bottleneck (here, the allocation inside `normalize`).

---

## Best Practices

- **Measure before you change anything.** Profile, identify the top one or two hot spots, change *only* those, then profile again to confirm. This is the loop; everything else is guessing. See [When to Optimize](/21-performance/10-when-to-optimize/).
- **Keep a dedicated `profiling` profile** (`inherits = "release"`, `debug = true`, `strip = false`) so production releases can still strip symbols while you profile a faithful build on demand.
- **Profile a realistic workload.** Use production-sized inputs. A profile of a toy input optimizes for the toy.
- **Use `samply` as your default.** It is cross-platform, needs no `sudo`, and opens the same Firefox Profiler UI everywhere, so your team shares one workflow.
- **Reach for `perf` when you need hardware counters.** On Linux, `perf stat` and `perf record -e cache-misses` answer questions a stack sampler can't ("why is this cache-bound?"). Pair that with [Cache-Friendly Code](/21-performance/05-cache-efficiency/).
- **Save and share profiles.** The Firefox Profiler lets you upload/export a profile so a teammate can open the exact same recording. Treat a profile like a test artifact.
- **Force frames you need with `#[inline(never)]`, temporarily.** If inlining hides a function you want to study, annotate it for the profiling session, then remove the annotation.

---

## Real-World Example

A realistic scenario: a backend service exposes a `/report` endpoint that has gotten slow under load. You suspect the report-building code, but you don't *know*. Here is the disciplined workflow, end to end, with the part that actually finds the answer.

First, the report builder — an HTTP-handler-free, runnable extract you can profile directly:

```rust
use std::collections::HashMap;
use std::time::Instant;

#[derive(Clone)]
struct Event {
    user: String,
    kind: String,
    amount_cents: u64,
}

/// Aggregate events into per-user totals — the suspected hot path.
fn build_report(events: &[Event]) -> HashMap<String, u64> {
    let mut totals: HashMap<String, u64> = HashMap::new();
    for e in events {
        // Allocating a fresh key string per event is the kind of thing
        // a profiler exposes as time inside the allocator.
        let key = format!("{}::{}", e.user, e.kind);
        *totals.entry(key).or_insert(0) += e.amount_cents;
    }
    totals
}

fn main() {
    // Synthesize a production-sized batch.
    let mut events = Vec::with_capacity(500_000);
    for i in 0..500_000u64 {
        events.push(Event {
            user: format!("user{}", i % 1_000),
            kind: if i % 2 == 0 { "purchase" } else { "refund" }.to_string(),
            amount_cents: i % 10_000,
        });
    }

    // A coarse wall-clock timer is enough to know the build is worth profiling.
    let start = Instant::now();
    let report = build_report(&events);
    let elapsed = start.elapsed();

    println!("rows: {}", report.len());
    println!("build_report took {:?}", elapsed);
}
```

A representative run of the release build prints (the exact duration varies by machine):

```text
rows: 1000
build_report took 60.891125ms
```

Now you profile it to *confirm* where those milliseconds go rather than assume:

```bash
# One-time setup in Cargo.toml: the [profile.profiling] block from earlier.
cargo build --profile profiling
samply record ./target/profiling/report_service
```

In the Firefox Profiler that opens, you sort the call tree by self time. `build_report` shows high total time, but its *self* time is modest; the bulk sits in children: `alloc::fmt::format` (the `format!`) and `malloc`/`realloc`/`free`. That is the evidence: the cost is **one heap allocation per event for the composite key**, half a million of them.

Armed with that, the fix is targeted: build the key into a reused buffer, or key the map on `(String, String)` / a borrowed tuple to avoid the `format!` entirely. You then re-profile to confirm the allocator frames shrank. The concrete allocation-avoidance techniques are in [Optimization Techniques](/21-performance/03-optimization/); the point here is that the profiler turned "I think the report code is slow" into "the `format!` on line 18 is the hot spot," which is the difference between fixing it in ten minutes and rewriting the wrong module for a day.

> **Tip:** For a long-running service you don't want to restart, attach to the live process instead of launching it: `samply record -p <pid>` (or `perf record -p <pid>` on Linux). This samples production traffic in place.

---

## Further Reading

- [The `samply` profiler](https://github.com/mstange/samply): installation, `record`, and attaching to a PID.
- [The Rust Performance Book — Profiling](https://nnethercote.github.io/perf-book/profiling.html): a tool-by-tool survey including `perf`, Valgrind/Cachegrind, and `samply`.
- [Cargo profiles reference](https://doc.rust-lang.org/cargo/reference/profiles.html): every key (`debug`, `strip`, `lto`, `inherits`, ...) and the built-in profiles.
- [The Firefox Profiler docs](https://profiler.firefox.com/docs/): reading the call tree, flame graph, and stack chart that `samply` opens.
- [Linux `perf` Examples (Brendan Gregg)](https://www.brendangregg.com/perf.html): the canonical reference for `perf` on Linux.

Related sections of this guide:

- [Flame Graphs with cargo-flamegraph](/21-performance/01-flamegraph/) — read flame graphs and drill into hot stacks.
- [Benchmarking with Criterion](/21-performance/02-benchmarking/) — measure *how much* faster a change is, with `criterion`.
- [Optimization Techniques](/21-performance/03-optimization/) — fix the allocation-heavy hot spots a profiler reveals.
- [Cache-Friendly Code](/21-performance/05-cache-efficiency/) — when `perf` says you're memory-bound, not CPU-bound.
- [When to Optimize](/21-performance/10-when-to-optimize/) — measure first; avoid premature optimization.
- [Section 01: Getting Started](/01-getting-started/) — `cargo` and build basics.
- [Section 02: Basics — Output](/02-basics/04-output/) — `println!` and formatting used in the examples.
- [Common Patterns](/22-common-patterns/) — idioms the optimized versions rely on.

---

## Exercises

### Exercise 1: Set up a profiling build

**Difficulty:** Easy

**Objective:** Produce an optimized binary that a profiler can symbolize.

**Instructions:**

1. Create a new project with `cargo new word_freq` and paste in the `word_freq` program from the **Rust Equivalent** section.
2. Add a `[profile.release]` (or a dedicated `[profile.profiling]`) block so the optimized build keeps debug info.
3. Build it and verify, with a verbose build, that the compiler received `debuginfo=2`.

<details>
<summary>Solution</summary>

Add to `Cargo.toml`:

```toml
[profile.release]
debug = true

[profile.profiling]
inherits = "release"
debug = true
strip = false
```

Then:

```bash
cargo build --profile profiling
# Confirm the flag reached rustc (force a rebuild so the command line prints):
touch src/main.rs
cargo build --release -v | grep -o 'debuginfo=[0-9a-z-]*'
```

Expected (the `split-debuginfo` line appears on macOS):

```text
debuginfo=2
debuginfo=unpacked
```

If you also install `samply` (`cargo install --locked samply`), you can now run `samply record ./target/profiling/word_freq` and see named frames, not hex addresses, in the browser.

</details>

### Exercise 2: Find the hot spot by reasoning, then confirm with self time

**Difficulty:** Medium

**Objective:** Predict which function dominates CPU time, then describe how you would confirm it from a profiler's report.

**Instructions:**

1. Look at the `word_freq` program. Identify the function that allocates the most heap memory per word processed, and explain *why* it allocates more than once per call.
2. State whether you would sort the profiler's call tree by **self time** or **total time** to confirm your prediction, and why.

<details>
<summary>Solution</summary>

`normalize` is the hot spot. It allocates **twice per word**:

```rust
fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()   // allocation #1: a brand-new String
        .to_lowercase()         // allocation #2: another new String
}
```

`collect::<String>()` heap-allocates a `String`, and `to_lowercase()` returns *another* freshly allocated `String`. Called once per word over a 50,000-line corpus, that is millions of allocations, so the allocator (`malloc`/`free`) and `normalize`'s own copying loop dominate.

You confirm by sorting by **self time**. `word_frequencies` and `main` have high *total* time because they call everything, but their *own* instructions are cheap. Self time isolates the function whose own work — here `normalize` and the allocator it invokes — is the real cost. (The fix: lowercase while iterating into a single reused buffer, eliminating both allocations per word; see [Optimization Techniques](/21-performance/03-optimization/).)

</details>

### Exercise 3: Choose the right tool

**Difficulty:** Medium

**Objective:** Match a profiling question to the appropriate tool.

**Instructions:** For each scenario, name the tool you would reach for and one sentence of justification.

1. You're on macOS and want a quick CPU flame graph of a CLI tool, with no `sudo`.
2. Your teammate on Linux reports the same code is "memory-bound" and you need cache-miss counts.
3. You must sample a long-running web service in production without restarting it.

<details>
<summary>Solution</summary>

1. **`samply record ./target/profiling/tool`** — it is cross-platform, needs no elevated privileges, and opens a flame graph in the Firefox Profiler in your browser. (Instruments' Time Profiler is a fine macOS-native alternative.)

2. **`perf` on Linux** — only `perf` exposes hardware performance counters. Use `perf stat -e cache-misses,cache-references ./binary` for counts, or `perf record -e cache-misses ./binary` to see *where* the misses occur. Stack samplers like `samply` can't answer "why is this cache-bound?" Pair this with [Cache-Friendly Code](/21-performance/05-cache-efficiency/).

3. **Attach to the running PID** with `samply record -p <pid>` (or `perf record -p <pid>` on Linux). Both sample an already-running process in place, so you capture real production traffic without a restart.

</details>
