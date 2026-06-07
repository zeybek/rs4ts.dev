---
title: "Custom Allocators: `GlobalAlloc`, `#[global_allocator]`, and Swapping in jemalloc / mimalloc"
description: "Swap Rust's global allocator for jemalloc or mimalloc with one attribute via GlobalAlloc, no call-site changes. Something JavaScript's V8 heap never lets you do."
---

In Node.js, every `{}`, `[]`, and `new Buffer()` goes through V8's allocator and the platform `malloc` underneath. You never see it, never choose it, and never override it. Rust exposes that machinery as a real, swappable interface. With a single attribute you can replace the program-wide allocator with one tuned for throughput (jemalloc), low fragmentation (mimalloc), or your own bookkeeping logic, without touching a single `Vec` or `Box` in your code.

---

## Quick Overview

Rust routes every heap allocation — `Box`, `Vec`, `String`, `HashMap`, `Rc`, and the rest — through one program-wide **global allocator**. By default that allocator is `std::alloc::System`, a thin wrapper over the platform's `malloc`/`free`. You can replace it by writing a type that implements the `unsafe` **`GlobalAlloc`** trait and tagging a `static` instance of it with the `#[global_allocator]` attribute. The most common reasons to do this are performance (drop in jemalloc or mimalloc) and observability (count or cap allocations).

For a TypeScript/JavaScript developer, the headline is **control with zero call-site churn**: you change *how* memory is obtained, but the rest of your program — including all the `std` collections — keeps working unmodified. This is something the V8 heap simply does not let you do from JavaScript.

---

## TypeScript/JavaScript Example

In JavaScript the allocator is sealed inside the engine. The closest you get is *observing* memory, never *replacing* the allocator:

```typescript
// Node.js v22 — you can MEASURE heap usage, but you cannot replace malloc.
const before = process.memoryUsage().heapUsed;

// Allocate ~8 MB of small objects on the V8 heap.
const big: { id: number }[] = [];
for (let i = 0; i < 100_000; i++) {
  big.push({ id: i });
}

const after = process.memoryUsage().heapUsed;
console.log(`heap grew by ${((after - before) / 1024 / 1024).toFixed(1)} MB`);
// heap grew by ~8.5 MB (exact value varies by GC timing)

// You can NUDGE the allocator with V8 flags at startup:
//   node --max-old-space-size=512 app.js
// ...but you cannot say "use jemalloc instead of V8's allocator for this object",
// and there is no `[object]` hook to intercept every allocation.
```

**Key points:**

- `process.memoryUsage()` *observes* the V8 heap; it cannot *change* the allocator.
- Engine flags (`--max-old-space-size`, `--max-semi-space-size`) tune the GC, not the underlying `malloc`.
- There is no per-program "use this allocator" switch and no allocation interception hook. The garbage collector decides when memory is reclaimed; you do not free anything explicitly.

> **Note:** The Rust comparison here is not garbage-collected. Rust frees memory deterministically (when a value's owner is dropped; see [Ownership](/05-ownership/)), and the *allocator* is the component that hands out and reclaims the underlying bytes. Customizing the allocator changes the bookkeeping, not the ownership rules.

---

## Rust Equivalent

Two lines of setup swap the entire program over to mimalloc, and every `Vec`/`Box`/`String` you already wrote now goes through it:

First add the crate (network access required):

```bash
cargo add mimalloc
```

Then declare the global allocator:

```rust
use mimalloc::MiMalloc;

// This ONE attribute redirects every heap allocation in the whole program.
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    // Nothing else changes. These all allocate through mimalloc now.
    let data: Vec<String> = (0..5).map(|i| format!("item {i}")).collect();
    println!("{data:?}");
}
```

Real output:

```text
["item 0", "item 1", "item 2", "item 3", "item 4"]
```

And here is what the trait you are plugging into actually looks like when you write your *own* allocator, a wrapper around `System` that counts live bytes:

```rust
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

// A counting allocator that forwards to the System allocator and tracks
// how many bytes are currently live.
struct Counting;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Forward to the real system allocator for the actual memory.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
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

fn main() {
    let before = ALLOCATED.load(Ordering::Relaxed);
    let v: Vec<u64> = (0..1000).collect();
    let during = ALLOCATED.load(Ordering::Relaxed);
    println!("before allocating vec: {before} bytes live");
    println!("with a Vec<u64> of 1000 items: {during} bytes live");
    drop(v);
    println!("after drop: {} bytes live", ALLOCATED.load(Ordering::Relaxed));
    println!("total alloc() calls so far: {}", ALLOC_CALLS.load(Ordering::Relaxed));
}
```

Real output (the exact numbers vary by platform and by what `std` allocates at startup, but the shape is stable: the `Vec<u64>` adds 8000 bytes, then frees them on `drop`):

```text
before allocating vec: 524 bytes live
with a Vec<u64> of 1000 items: 8524 bytes live
after drop: 1612 bytes live
total alloc() calls so far: 6
```

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically. `GlobalAlloc`, `#[global_allocator]`, and `std::alloc::System` are all long-stable — none of this needs nightly.

---

## Detailed Explanation

### The `GlobalAlloc` trait

`GlobalAlloc` lives in `std::alloc` and has exactly two required methods:

```rust
// (from the standard library — shown for reference)
pub unsafe trait GlobalAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8;
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout);
    // alloc_zeroed and realloc have default implementations you may override.
}
```

Walking through the pieces a TypeScript developer has never had to think about:

- **`unsafe trait` / `unsafe impl`.** The trait is *unsafe to implement* because the compiler cannot verify that your `alloc` returns a block that is actually `layout.size()` bytes long and correctly aligned, nor that `dealloc` is given back a pointer your `alloc` produced. You promise those invariants by writing `unsafe impl`. (This is the inverse of an `unsafe fn`, which is unsafe to *call*. See [Unsafe Rust](/20-unsafe-ffi/00-unsafe-intro/).)

- **`Layout`.** Every request carries a `Layout`: the `(size, align)` pair the allocation must satisfy. There is no "allocate me an object of unknown size": the size and alignment are always known up front, because Rust types have a fixed, compile-time layout (see [Memory Layout](/21-performance/04-memory-layout/)).

  ```rust
  use std::alloc::Layout;

  fn main() {
      // A Layout is the (size, alignment) pair the allocator must satisfy.
      let l = Layout::new::<[u64; 4]>();
      println!("[u64; 4]: size={} align={}", l.size(), l.align());

      let l2 = Layout::new::<u8>();
      println!("u8:       size={} align={}", l2.size(), l2.align());

      // Layout for a slice whose length you compute at runtime.
      let l3 = Layout::array::<u32>(10).unwrap();
      println!("[u32; 10]: size={} align={}", l3.size(), l3.align());
  }
  ```

  Real output:

  ```text
  [u64; 4]: size=32 align=8
  u8:       size=1 align=1
  [u32; 10]: size=40 align=4
  ```

- **`*mut u8`.** `alloc` returns a raw pointer to the start of the block, or null on failure. Raw pointers are how you talk to allocators (see [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/)). `Vec`/`Box` build their safe abstractions *on top of* this.

- **`&self`, not `&mut self`.** The global allocator is shared across all threads simultaneously, so its methods take `&self`. Any internal state you keep (like the byte counter above) must be thread-safe, which is why the counting example uses `AtomicUsize` rather than a plain `usize`. (Atomics are covered in [Atomic Operations](/26-systems-programming/).)

### What `#[global_allocator]` does

The `#[global_allocator]` attribute marks one `static` as *the* allocator for the entire program (and everything it links, including dependencies). The compiler wires the language's allocation "lang items" — the hidden hooks that `Box::new`, `Vec::push`, `String`, etc. call — to your static's `alloc`/`dealloc`. You write zero changes at any call site; the redirection is global and automatic.

You may declare **at most one** `#[global_allocator]` per program, and it must be a `static` of a type implementing `GlobalAlloc`.

### Forwarding vs. replacing

The counting allocator above is a *forwarding* (or "shim") allocator: it does bookkeeping and then hands the real work to `System`. jemalloc and mimalloc are *replacement* allocators: their `alloc` talks to a completely different memory manager that often outperforms the system `malloc` under multi-threaded, high-churn workloads, the exact profile of a busy web server.

---

## Key Differences

| Aspect | JavaScript (Node/V8) | Rust |
| --- | --- | --- |
| Who allocates? | The V8 engine; you cannot replace it | The global allocator: `System` by default, swappable |
| How to swap | Not possible from JS | One `#[global_allocator]` static |
| Reclamation | Garbage collector, non-deterministic | Deterministic `drop` → allocator's `dealloc` |
| Interception hook | None | Implement `GlobalAlloc` yourself |
| Per-object choice | None | Stable global; per-collection allocators are nightly (`allocator_api`) |
| Tuning knobs | GC flags (`--max-old-space-size`) | Crate features + env vars (e.g. `MALLOC_CONF` for jemalloc) |
| Cost of swapping | N/A | Zero call-site changes; recompile only |

The deepest conceptual difference: in JavaScript the allocator and the garbage collector are one inseparable, hidden subsystem. In Rust, **ownership decides *when* memory is freed** and the **allocator decides *how* the bytes are obtained and returned**: two independent concerns. Customizing the allocator never changes your program's correctness or its `drop` timing; it only changes the byte-management strategy underneath.

> **Tip:** Swapping to jemalloc or mimalloc is one of the highest-impact, lowest-risk performance changes available to a Rust server. It is two lines of code and frequently buys double-digit-percent throughput gains on allocation-heavy, multi-threaded workloads. Measure before and after with the techniques in [Benchmarking](/21-performance/02-benchmarking/).

---

## Common Pitfalls

### Pitfall 1: Forgetting `unsafe` on the `impl`

`GlobalAlloc` is an `unsafe trait`, so the implementation block must be `unsafe impl`, not plain `impl`.

```rust
use std::alloc::{GlobalAlloc, Layout, System};

struct MyAlloc;

// does not compile (error[E0200]): missing the `unsafe` keyword on the impl.
impl GlobalAlloc for MyAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

fn main() {}
```

The real compiler error:

```text
error[E0200]: the trait `GlobalAlloc` requires an `unsafe impl` declaration
 --> src/main.rs:6:1
  |
6 | impl GlobalAlloc for MyAlloc {
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: the trait `GlobalAlloc` enforces invariants that the compiler can't check. Review the trait documentation and make sure this implementation upholds those invariants before adding the `unsafe` keyword
help: add `unsafe` to this trait implementation
```

The fix is exactly what the compiler says: write `unsafe impl GlobalAlloc for MyAlloc`.

### Pitfall 2: Two `#[global_allocator]` declarations

You get exactly one. Declaring two (a classic mistake when you copy a snippet into a crate that already sets one) is a hard error.

```rust
use mimalloc::MiMalloc;
use tikv_jemallocator::Jemalloc;

#[global_allocator]
static A: MiMalloc = MiMalloc;

#[global_allocator] // does not compile: a second global allocator
static B: Jemalloc = Jemalloc;

fn main() {}
```

The real compiler error:

```text
error: cannot define multiple global allocators
 --> src/main.rs:8:1
  |
5 | static A: MiMalloc = MiMalloc;
  | ------------------------------ previous global allocator defined here
6 |
7 | #[global_allocator]
```

This also bites if a *dependency* already sets a global allocator. Only the final binary crate should choose one. Libraries should not declare `#[global_allocator]`; leave that decision to the application.

### Pitfall 3: Allocating *inside* your allocator → infinite recursion → stack overflow

This is the single nastiest custom-allocator trap. Your `alloc`/`dealloc` hooks run on *every* allocation. If they themselves allocate — for example by calling `println!`, `format!`, or building a `String` for a log line — that inner allocation re-enters your hook, which allocates again, forever.

A version of the budget allocator (in the Real-World Example below) that called `eprintln!("...")` inside `alloc` produces, at runtime:

```text
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting
```

> **Warning:** Inside `GlobalAlloc::alloc`/`dealloc`, never do anything that allocates. Set an `Atomic` flag or update an `AtomicUsize` counter, then read/log it *outside* the hook. (`eprintln!` of a bare static `&str` can also trip startup machinery; the safe pattern is to record state in atomics and report it from normal code.)

### Pitfall 4: Expecting a per-`Vec` allocator on stable

You may have seen `Vec::new_in(my_alloc)` and the `Allocator` trait. That per-collection allocator API (`allocator_api`) is **still nightly-only** as of Rust 1.96.0. On stable you choose the allocator *once*, globally, via `#[global_allocator]`. If you need region/arena allocation for a subset of your data on stable, reach for a crate like `bumpalo` (which gives you `bumpalo::Bump` and its own `Vec`/`String` types) rather than the nightly `Allocator` trait.

### Pitfall 5: Forgetting jemalloc's `unprefixed_malloc` / stats features

The `tikv-jemalloc-ctl` crate gates its statistics modules behind a Cargo feature. Importing `stats` without enabling it fails:

```text
error[E0432]: unresolved import `tikv_jemalloc_ctl::stats`
  --> src/main.rs:2:32
   |
 2 | use tikv_jemalloc_ctl::{epoch, stats};
   |                                ^^^^^ no `stats` in the root
   |
note: found an item that was configured out
...
98 | #[cfg(feature = "stats")]
   |       ----------------- the item is gated behind the `stats` feature
```

Fix it with `cargo add tikv-jemalloc-ctl --features stats`.

---

## Best Practices

- **Default to a battle-tested replacement allocator for servers.** For multi-threaded, allocation-heavy services, dropping in **jemalloc** (`tikv-jemallocator`) or **mimalloc** (`mimalloc`) is a cheap, well-understood win. Pick based on measurement, not folklore.
- **Only the binary crate chooses.** Never put `#[global_allocator]` in a library you publish: it would force the choice on every downstream user and collide with theirs.
- **Keep allocator hooks allocation-free and fast.** They are on the hottest path in the program. Use atomics for any bookkeeping; never log, format, or lock a `Mutex` that could allocate inside them.
- **Forward to `System` unless you truly manage memory yourself.** Most custom allocators are *shims* (count, cap, trace) that delegate the real work to `System`. Only write the actual byte management when you have a specific strategy (arena, pool, bump).
- **Measure, do not guess.** Use the stats hooks (jemalloc's `tikv-jemalloc-ctl`) and the profiling tools in [Profiling](/21-performance/00-profiling/) and [Benchmarking](/21-performance/02-benchmarking/) to confirm a swap actually helps *your* workload.
- **Reach for `bumpalo` for arenas on stable.** If you want fast bump allocation for a batch of short-lived values, `bumpalo` is the idiomatic stable choice; reserve a custom `GlobalAlloc` for whole-program policy.

---

## Real-World Example

A production-flavored use case where a custom allocator helps even though raw speed is not the goal: a **memory budget guardrail** for staging/test builds. It forwards every allocation to `System`, tracks the peak and current live bytes, and flips a flag if the program ever exceeds a configured budget. A cheap way to catch a memory regression in CI before it reaches production.

```rust
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// A global allocator that forwards to the System allocator but records the
/// peak live byte count and trips a flag if a budget is exceeded. Useful as a
/// debug/staging guardrail to catch runaway allocation in tests and CI.
struct BudgetAlloc {
    limit: usize,
}

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static OVER_BUDGET: AtomicBool = AtomicBool::new(false);

unsafe impl GlobalAlloc for BudgetAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
            if now > self.limit {
                // CRITICAL: never allocate inside the allocator. Just set a flag;
                // do NOT call println!/format! here (they allocate -> recursion).
                OVER_BUDGET.store(true, Ordering::Relaxed);
            }
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static GLOBAL: BudgetAlloc = BudgetAlloc { limit: 4 * 1024 };

fn main() {
    let _small: Vec<u8> = vec![0; 1024]; // under budget
    let big: Vec<u8> = vec![0; 8 * 1024]; // exceeds the 4 KiB budget
    drop(big);

    // Safe to format/print HERE, outside the allocator hook.
    println!("peak live bytes: {}", PEAK.load(Ordering::Relaxed));
    println!("ever over budget: {}", OVER_BUDGET.load(Ordering::Relaxed));
}
```

Real output:

```text
peak live bytes: 9740
ever over budget: true
```

### Observability with jemalloc's stats

If you ship jemalloc, you get rich, free statistics through `tikv-jemalloc-ctl`. Add both crates:

```bash
cargo add tikv-jemallocator
cargo add tikv-jemalloc-ctl --features stats
```

```rust
use tikv_jemallocator::Jemalloc;
use tikv_jemalloc_ctl::{epoch, stats};

#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() {
    // jemalloc caches its statistics; advancing the "epoch" refreshes them.
    let e = epoch::mib().unwrap();
    let allocated = stats::allocated::mib().unwrap();
    let resident = stats::resident::mib().unwrap();

    let _big: Vec<u8> = vec![0; 10 * 1024 * 1024]; // 10 MiB

    e.advance().unwrap(); // refresh the cached statistics
    println!("allocated: {} bytes", allocated.read().unwrap());
    println!("resident:  {} bytes", resident.read().unwrap());
}
```

Real output (numbers vary by run; `allocated` tracks bytes handed to the program, `resident` tracks bytes jemalloc holds from the OS):

```text
allocated: 10557528 bytes
resident:  15482880 bytes
```

This is the kind of per-process memory telemetry you would normally export to your metrics backend (see [Metrics](/28-production/)), and it comes essentially for free once jemalloc is your allocator.

---

## Further Reading

- [`std::alloc::GlobalAlloc`](https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html) — the trait reference.
- [`std::alloc::Layout`](https://doc.rust-lang.org/std/alloc/struct.Layout.html): size/alignment requests.
- [The `#[global_allocator]` attribute](https://doc.rust-lang.org/std/alloc/index.html) — module-level docs on global allocators.
- [`tikv-jemallocator` on crates.io](https://crates.io/crates/tikv-jemallocator) and [`mimalloc` on crates.io](https://crates.io/crates/mimalloc) — the two most common replacement allocators.
- [`bumpalo` on crates.io](https://crates.io/crates/bumpalo) — stable arena/bump allocation when you want region allocation for a subset of data.
- Related guide sections:
  - [Unsafe Rust](/20-unsafe-ffi/00-unsafe-intro/) and [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/) — the `unsafe` and `*mut u8` foundations.
  - [Memory Layout](/21-performance/04-memory-layout/): where `Layout` (size + alignment) comes from.
  - [Profiling](/21-performance/00-profiling/) and [Benchmarking](/21-performance/02-benchmarking/) — how to prove a swap helped.
  - [PhantomData and Zero-Sized Types](/25-advanced-topics/00-phantom-data/): your allocator type is itself a zero-sized type.
  - [Specialization](/25-advanced-topics/07-specialization/) and [Compiler & Tooling Internals](/25-advanced-topics/08-compiler-plugins/) — more "stable vs. nightly" boundaries, like the nightly `allocator_api`.
  - [Systems Programming](/26-systems-programming/): atomics and threads, which allocator state relies on.
  - Newcomers: [Getting Started](/01-getting-started/) and [Basics](/02-basics/).

---

## Exercises

### Exercise 1: Track peak memory, not just current

**Difficulty:** Beginner

**Objective:** Extend the counting allocator so it also records the *peak* live byte count (the high-water mark), and override `alloc_zeroed` so zeroed allocations are tracked too.

**Instructions:**

Start from the counting allocator. Add a `static PEAK: AtomicUsize`. In `alloc`, after incrementing the live counter, update the peak with `fetch_max`. Add an `alloc_zeroed` override (forwarding to `System.alloc_zeroed`) that does the same bookkeeping. In `main`, allocate two large vectors inside a scope, let them drop, allocate a tiny one, then print both the current live bytes and the peak — the peak should be much larger than the live total.

<details>
<summary>Solution</summary>

```rust
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Tracking;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        ptr
    }
}

#[global_allocator]
static GLOBAL: Tracking = Tracking;

fn main() {
    {
        let _a: Vec<u64> = (0..2000).collect();
        let _b: Vec<u64> = (0..2000).collect();
    } // both dropped here
    let _c: Vec<u8> = vec![0; 10];

    println!("live now: {} bytes", LIVE.load(Ordering::Relaxed));
    println!("peak:     {} bytes", PEAK.load(Ordering::Relaxed));
}
```

Real output (peak greatly exceeds the live total because the two big vectors were alive simultaneously):

```text
live now: 534 bytes
peak:     32524 bytes
```

</details>

### Exercise 2: Swap in mimalloc and confirm it changed nothing else

**Difficulty:** Beginner

**Objective:** Prove the "zero call-site churn" claim by running an allocation-heavy program first with the default allocator, then with mimalloc, with no other code changes.

**Instructions:**

Write a `main` that builds a `Vec<String>` of 100,000 formatted strings and prints its length. Run it as-is. Then `cargo add mimalloc`, add the two-line `#[global_allocator]` declaration at the top, and run again. The output (the length) must be identical; only the allocator underneath changed.

<details>
<summary>Solution</summary>

```rust
// After: cargo add mimalloc
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    let data: Vec<String> = (0..100_000).map(|i| format!("row-{i}")).collect();
    println!("built {} strings, last = {:?}", data.len(), data.last());
}
```

Real output:

```text
built 100000 strings, last = Some("row-99999")
```

Remove the `use` line and the `#[global_allocator]` static, and the program prints the exact same line — the only thing that differs is which allocator served the 100,000 `String`s. That is the whole point: allocator choice is orthogonal to program logic.

</details>

### Exercise 3: A bump allocator with a `System` fallback

**Difficulty:** Advanced

**Objective:** Implement a real (not forwarding) global allocator: a fixed-size **bump allocator** that hands out aligned slices from a static arena by advancing an offset, and falls back to `System` once the arena is exhausted. Never frees individual arena allocations.

**Instructions:**

Create a 64 KiB static arena inside an `UnsafeCell<[u8; N]>` wrapped in a `#[repr(align(16))]` struct, with a manual `unsafe impl Sync` (synchronization is provided by an `AtomicUsize` offset). In `alloc`, round the current offset up to `layout.align()`, reserve `layout.size()` bytes with a `compare_exchange_weak` loop, and return `base + aligned`; if the request would overflow the arena, forward to `System`. In `dealloc`, free only pointers that fall *outside* the arena range (those came from the `System` fallback); arena pointers are never freed. Test it by boxing a value and building a small `Vec`, then print how many arena bytes were used.

<details>
<summary>Solution</summary>

```rust
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

const ARENA_SIZE: usize = 64 * 1024;

// Over-aligned so the arena's base satisfies common alignment requirements.
#[repr(align(16))]
struct Arena(UnsafeCell<[u8; ARENA_SIZE]>);

// SAFETY: all access is coordinated through the atomic `offset` in BumpAlloc.
unsafe impl Sync for Arena {}

struct BumpAlloc {
    arena: Arena,
    offset: AtomicUsize,
}

unsafe impl GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();
        let base = self.arena.0.get() as *mut u8;

        // Reserve an aligned slice with a CAS loop (lock-free, thread-safe).
        let mut old = self.offset.load(Ordering::Relaxed);
        loop {
            let aligned = (old + align - 1) & !(align - 1);
            let new = aligned + size;
            if new > ARENA_SIZE {
                // Arena full: fall back to the System allocator.
                return unsafe { System.alloc(layout) };
            }
            match self.offset.compare_exchange_weak(
                old, new, Ordering::Relaxed, Ordering::Relaxed,
            ) {
                Ok(_) => return unsafe { base.add(aligned) },
                Err(actual) => old = actual,
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let base = self.arena.0.get() as *mut u8;
        let end = unsafe { base.add(ARENA_SIZE) };
        // Only free pointers that came from the System fallback.
        if ptr < base || ptr >= end {
            unsafe { System.dealloc(ptr, layout) };
        }
        // Arena allocations are never individually freed (that's the bump trade-off).
    }
}

#[global_allocator]
static GLOBAL: BumpAlloc = BumpAlloc {
    arena: Arena(UnsafeCell::new([0; ARENA_SIZE])),
    offset: AtomicUsize::new(0),
};

fn main() {
    let a = Box::new(42u64);
    let b: Vec<u8> = vec![7; 100];
    println!("boxed = {a}, vec[0] = {}, len = {}", b[0], b.len());
    println!("arena bytes used so far: {}", GLOBAL.offset.load(Ordering::Relaxed));
}
```

Real output (the exact byte count depends on what `std` allocates before `main`):

```text
boxed = 42, vec[0] = 7, len = 100
arena bytes used so far: 1728
```

This is the core idea behind arena/bump allocation: allocation is just an atomic add, deallocation is free (literally a no-op), and you trade the ability to reclaim individual objects for raw speed. For a production-quality, *scoped* version on stable, use the `bumpalo` crate rather than wiring a bump allocator in globally.

</details>
