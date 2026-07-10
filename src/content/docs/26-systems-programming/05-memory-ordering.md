---
title: "Memory Ordering"
description: "Choose an Ordering on every Rust atomic: Relaxed, Acquire, Release, AcqRel, SeqCst. Unlike JavaScript's fixed Atomics, you control what may be reordered."
---

When two threads touch the same atomic, the question is no longer *"is the operation indivisible?"* but *"what else is the compiler and CPU allowed to reorder around it?"*. The `Ordering` argument every atomic method takes answers exactly that. This file covers the five variants (`Relaxed`, `Acquire`, `Release`, `AcqRel`, and `SeqCst`): what each one guarantees, and how to pick the right one.

---

## Quick Overview

A modern CPU and an optimizing compiler both **reorder memory accesses** to go faster: a store you wrote first might become visible to another core *after* a store you wrote second. **Memory ordering** is the set of rules you opt into so that one thread's writes become visible to another thread in a predictable way. In Rust you choose an ordering on every atomic operation via [`std::sync::atomic::Ordering`](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html); choosing too weak an ordering is a real (if rare) bug, and choosing too strong a one is just slower.

> **Note:** This is the conceptual companion to [Atomic Operations](/26-systems-programming/04-atomic-operations/). That file covers *which* atomic types and methods exist (`load`, `store`, `fetch_add`, `compare_exchange`). This file covers the `Ordering` argument those methods all take.

---

## TypeScript/JavaScript Example

JavaScript hides almost all of this from you. A normal program is single-threaded, so within one thread the language guarantees your code *appears* to run in source order. The only place ordering becomes visible is the `Atomics` API used with a `SharedArrayBuffer` shared between the main thread and a Worker.

```typescript
// worker-shared.ts — the shared-memory surface JS actually exposes.
const sab = new SharedArrayBuffer(8);
const view = new Int32Array(sab);

// Every Atomics operation is *sequentially consistent*. There is no
// "relaxed" or "acquire/release" knob in JavaScript at all.
Atomics.store(view, 0, 42); // publish
const seen = Atomics.load(view, 0); // observe -> always 42
console.log("load:", seen);

// Read-modify-write, also fully sequentially consistent:
const old = Atomics.add(view, 0, 1); // returns the previous value
console.log("add returned old value:", old); // 42
console.log("after add:", Atomics.load(view, 0)); // 43

// compareExchange(view, index, expected, replacement) -> previous value
console.log("cas:", Atomics.compareExchange(view, 0, 43, 100)); // 43
console.log("final:", Atomics.load(view, 0)); // 100
```

Running this under Node v22 prints:

```text
load: 42
add returned old value: 42
after add: 43
cas: 43
final: 100
```

**Key point:** JavaScript gives you exactly **one** memory ordering — the strongest, sequentially-consistent one — and no way to weaken it. That is simpler, but it leaves performance on the table and gives you no vocabulary for the distinctions Rust makes explicit.

---

## Rust Equivalent

Rust forces you to name an ordering on every atomic operation. The most common and most useful pattern is **release/acquire publication**: one thread writes some data and then "publishes" a flag with `Release`; another thread reads the flag with `Acquire` and is then guaranteed to see the data.

```rust playground
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

fn main() {
    // Shared data plus a "ready" flag.
    let data = Arc::new(AtomicU64::new(0));
    let ready = Arc::new(AtomicBool::new(false));

    let data_producer = Arc::clone(&data);
    let ready_producer = Arc::clone(&ready);

    let producer = thread::spawn(move || {
        data_producer.store(42, Ordering::Relaxed); // (1) write the data
        ready_producer.store(true, Ordering::Release); // (2) publish: "data is ready"
    });

    let consumer = thread::spawn(move || {
        // Spin until we observe the Release store.
        while !ready.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        // Acquire guarantees (1) is visible here, so this is always 42.
        let value = data.load(Ordering::Relaxed);
        println!("consumer saw data = {value}");
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}
```

Output:

```text
consumer saw data = 42
```

The `Release` store at (2) and the `Acquire` load that observes it form a **happens-before** edge: everything the producer did *before* the `Release` store (including the data write at (1)) is guaranteed visible to the consumer *after* its `Acquire` load returns `true`. That guarantee is the entire point of memory ordering.

> **Tip:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. Atomics and `Ordering` have been stable since Rust 1.0 and follow the C++20 memory model.

---

## Detailed Explanation

There are five `Ordering` values. Think of them as a ladder from "no synchronization, just atomicity" up to "global total order".

### `Relaxed` — atomicity only, no ordering

`Relaxed` guarantees the single operation is indivisible (no torn reads/writes) and nothing else. It creates **no happens-before relationship** with any other memory. It is perfect for a counter whose value is the only thing you care about:

```rust playground
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

fn main() {
    let counter = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                for _ in 0..100_000 {
                    // Relaxed is enough: we only need the count to be correct,
                    // not to synchronize any *other* memory with it.
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    println!("final count = {}", counter.load(Ordering::Relaxed));
}
```

Output:

```text
final count = 800000
```

The total is always exactly `800000` because each `fetch_add` is atomic. `Relaxed` is correct here precisely because the count does not *guard* any other data: no thread reads the counter and then assumes some other memory is in a particular state.

### `Release` — a one-way "publish" barrier (stores only)

A `Release` store says: *"every memory write I did before this store must not be reordered after it, and becomes visible to any thread that later acquires this value."* It is the write half of publication. `Release` is only valid on operations that write (`store`, the success path of `compare_exchange`, `fetch_*`).

### `Acquire` — a one-way "observe" barrier (loads only)

An `Acquire` load says: *"every memory read I do after this load must not be reordered before it; and if I observe a value written by a `Release`, I see everything that thread did before that `Release`."* It is the read half of publication. `Acquire` is only valid on operations that read.

`Release` and `Acquire` only do anything **as a pair**, on the **same** atomic variable. A lone `Release` with no matching `Acquire` synchronizes with nobody.

### `AcqRel` — both halves at once (read-modify-write only)

A read-modify-write operation (`fetch_add`, `swap`, `compare_exchange`) both reads and writes. `AcqRel` makes the read half an `Acquire` and the write half a `Release`. Use it when a single RMW both observes a previous publication *and* publishes its own result:

```rust playground
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;

// A counter built from compare_exchange (a read-modify-write), so AcqRel
// is the natural fit: it both reads and writes the location.
fn main() {
    let value = Arc::new(AtomicU32::new(0));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let value = Arc::clone(&value);
            thread::spawn(move || {
                for _ in 0..50_000 {
                    let mut current = value.load(Ordering::Relaxed);
                    loop {
                        let next = current + 1;
                        match value.compare_exchange_weak(
                            current,
                            next,
                            Ordering::AcqRel,  // success: acquire+release the RMW
                            Ordering::Relaxed, // failure: we just retry, no sync needed
                        ) {
                            Ok(_) => break,
                            Err(observed) => current = observed,
                        }
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    println!("CAS counter = {}", value.load(Ordering::Relaxed));
}
```

Output:

```text
CAS counter = 200000
```

`compare_exchange` and `compare_exchange_weak` take **two** orderings: one for success and one for failure. The failure ordering describes only the load that happened (the CAS did not write), so it may be `Acquire`, `Relaxed`, or `SeqCst`, never `Release`/`AcqRel`. See [Common Pitfalls](#common-pitfalls).

### `SeqCst` — sequential consistency (the strongest)

`SeqCst` does everything `AcqRel` does **and** adds a single global total order that *all* `SeqCst` operations across all threads agree on. This extra guarantee matters only in subtle cases involving more than two variables, such as the store-buffer / Dekker pattern below: each thread writes its own flag and reads the other's.

```rust playground
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

// Dekker-style store-buffer test, run many times.
fn run_once() -> usize {
    let x = Arc::new(AtomicBool::new(false));
    let y = Arc::new(AtomicBool::new(false));
    let z = Arc::new(AtomicUsize::new(0)); // how many threads saw the *other* flag as false

    let (x1, y1, z1) = (Arc::clone(&x), Arc::clone(&y), Arc::clone(&z));
    let t1 = thread::spawn(move || {
        x1.store(true, Ordering::SeqCst);
        if !y1.load(Ordering::SeqCst) {
            z1.fetch_add(1, Ordering::SeqCst);
        }
    });

    let (x2, y2, z2) = (Arc::clone(&x), Arc::clone(&y), Arc::clone(&z));
    let t2 = thread::spawn(move || {
        y2.store(true, Ordering::SeqCst);
        if !x2.load(Ordering::SeqCst) {
            z2.fetch_add(1, Ordering::SeqCst);
        }
    });

    t1.join().unwrap();
    t2.join().unwrap();
    z.load(Ordering::SeqCst)
}

fn main() {
    // Under SeqCst, z is never 2: a single global order forbids *both* threads
    // reading the other's flag as false.
    let mut max_seen = 0;
    for _ in 0..100_000 {
        max_seen = max_seen.max(run_once());
    }
    println!("max z observed over 100k runs = {max_seen} (SeqCst forbids 2)");
}
```

Output:

```text
max z observed over 100k runs = 1 (SeqCst forbids 2)
```

If both stores and both loads used only `Release`/`Acquire`, the outcome `z == 2` would be permitted by the memory model (each thread's store could sit in a store buffer while it reads a stale `false` for the other flag). `SeqCst` rules that out because the four operations must fit into one consistent global sequence.

> **Note:** `z` is `0` or `1` here, never `2`. You will rarely *need* this multi-variable guarantee, but when you are not sure, `SeqCst` is the safe default and the one that matches JavaScript's `Atomics`.

---

## Key Differences

| Concept | JavaScript `Atomics` | Rust `Ordering` |
| --- | --- | --- |
| Available orderings | One (sequentially consistent) | Five (`Relaxed`/`Acquire`/`Release`/`AcqRel`/`SeqCst`) |
| Default / only choice | `SeqCst`-equivalent, forced | You must pick explicitly every call |
| Non-atomic shared writes | Not directly observable across threads | Reorderable; ordering only constrains relative to the atomic |
| Cost model | Hidden, always strongest | You pay only for what you ask for |
| Memory model basis | ECMAScript shared-memory model | C++20 (C11) memory model |

A few rules worth internalizing:

- **`Release` is for stores, `Acquire` is for loads, `AcqRel` is for read-modify-writes.** Asking for the wrong category is a compile error (shown below), not a silent footgun.
- **Synchronization is per-variable and pairwise.** A `Release` on `flag` only synchronizes with an `Acquire` on `flag` that *observes that store*. It does nothing for some other atomic.
- **Ordering constrains *surrounding* memory, not just the atomic itself.** That is the whole reason it exists: the `Acquire`/`Release` pair is what makes your *non-atomic* `Vec`, `String`, etc. safe to hand between threads once a flag flips.
- **Strength ladder:** `Relaxed` < `Release`/`Acquire` < `AcqRel` < `SeqCst`. Pick the weakest one that is provably correct; default to `SeqCst` only when reasoning gets hard.

> **Warning:** Unlike TypeScript, where `number` precision and single-threading insulate you, getting Rust's ordering *too weak* produces a bug that often passes every test on x86 (which is strongly ordered) and only fails on ARM or under heavy contention. Memory-ordering bugs do not show up as compiler errors; they show up as flaky production incidents. When in doubt, go stronger.

---

## Common Pitfalls

### Pitfall 1: Using a `Release` failure ordering on `compare_exchange`

A failed `compare_exchange` performs only a load, so a `Release`/`AcqRel` failure ordering is meaningless, and rejected at compile time by a built-in lint.

```rust
use std::sync::atomic::{AtomicU32, Ordering};

fn main() {
    let a = AtomicU32::new(0);
    // does not compile: failure ordering may not be Release or AcqRel
    let _ = a.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Release);
}
```

Real compiler output:

```text
error: `compare_exchange`'s failure ordering may not be `Release` or `AcqRel`, since a failed `compare_exchange` does not result in a write
 --> src/main.rs:6:56
  |
6 |     let _ = a.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Release);
  |                                                        ^^^^^^^^^^^^^^^^^ invalid failure ordering
  |
  = help: consider using `Acquire` or `Relaxed` failure ordering instead
  = note: `#[deny(invalid_atomic_ordering)]` on by default
```

Fix: use `Ordering::Acquire` (if you need to observe the publication on failure) or `Ordering::Relaxed` (if you will just retry).

### Pitfall 2: Asking a `store` for an `Acquire` ordering (or a `load` for `Release`)

The categories are enforced. A store cannot be `Acquire`/`AcqRel`; a load cannot be `Release`/`AcqRel`.

```rust
use std::sync::atomic::{AtomicBool, Ordering};

fn main() {
    let flag = AtomicBool::new(false);
    // does not compile: store cannot have Acquire/AcqRel ordering
    flag.store(true, Ordering::Acquire);
    let _ = flag.load(Ordering::Release);
}
```

Real compiler output:

```text
error: atomic stores cannot have `Acquire` or `AcqRel` ordering
 --> src/main.rs:6:22
  |
6 |     flag.store(true, Ordering::Acquire);
  |                      ^^^^^^^^^^^^^^^^^
  |
  = help: consider using ordering modes `Release`, `SeqCst` or `Relaxed`
  = note: `#[deny(invalid_atomic_ordering)]` on by default

error: atomic loads cannot have `Release` or `AcqRel` ordering
 --> src/main.rs:7:23
  |
7 |     let _ = flag.load(Ordering::Release);
  |                       ^^^^^^^^^^^^^^^^^
  |
  = help: consider using ordering modes `Acquire`, `SeqCst` or `Relaxed`
```

### Pitfall 3: Publishing with `Relaxed` (the silent one)

This is the dangerous one because it **compiles cleanly**: there is no diagnostic. It is a logic bug, not a type error.

```rust playground
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

fn main() {
    let data = Arc::new(AtomicU64::new(0));
    let ready = Arc::new(AtomicBool::new(false));

    let (d, r) = (Arc::clone(&data), Arc::clone(&ready));
    let producer = thread::spawn(move || {
        d.store(42, Ordering::Relaxed);
        // BUG: Relaxed gives no ordering relative to the data store above.
        r.store(true, Ordering::Relaxed);
    });

    let consumer = thread::spawn(move || {
        while !ready.load(Ordering::Relaxed) {
            std::hint::spin_loop();
        }
        // With Relaxed there is NO happens-before edge. The CPU/compiler may let
        // this read run before the data store is visible. It usually prints 42 on
        // x86, but that is luck, not a guarantee — on a weakly-ordered CPU (ARM)
        // it can legitimately read 0.
        println!("consumer saw {}", data.load(Ordering::Relaxed));
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}
```

This builds and, on a typical x86 desktop, will print `consumer saw 42` every time — which is exactly why the bug is so insidious. The fix is the [Rust Equivalent](#rust-equivalent) at the top: publish with `Release`, observe with `Acquire`. To *catch* this class of bug, run your tests under [`cargo +nightly miri test`](https://github.com/rust-lang/miri) (Miri's randomized weak-memory emulation can surface the missing edge) or [ThreadSanitizer](https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html).

### Pitfall 4: Expecting two unrelated `Release`/`Acquire` pairs to give a total order

`Release`/`Acquire` only orders memory relative to the *one* variable in the pair. If your algorithm needs all threads to agree on the interleaving of operations on *several* variables (the store-buffer case), you need `SeqCst`. Reaching for `Acquire`/`Release` there is a classic over-optimization that introduces a rare reordering bug.

---

## Best Practices

- **Default to `SeqCst` while prototyping.** It matches JavaScript's `Atomics`, it is the easiest to reason about, and it is correct everywhere. Weaken to `Acquire`/`Release`/`Relaxed` only after you can articulate *why* the weaker ordering is sufficient.
- **Use `Relaxed` for standalone counters and statistics**: values that are not used to guard other memory (request counts, metrics, ID generators).
- **Use `Release` to publish and `Acquire` to consume** whenever a flag or pointer "hands off" other data between threads. This is the workhorse pattern.
- **Use `AcqRel` on read-modify-writes** (`fetch_*`, `swap`, `compare_exchange` success) that both observe and publish.
- **Prefer a higher-level abstraction first.** `Mutex`, `RwLock`, `OnceLock`, channels ([Channels](/26-systems-programming/03-channels/)), and the standard collections already encapsulate correct orderings. Hand-rolled atomics with custom orderings belong in lock-free data structures and hot paths, not everyday code.
- **Document the pairing.** Write a comment naming which `Release` pairs with which `Acquire`; future readers cannot infer it from types alone.
- **Verify with tooling.** Run concurrency-sensitive code under Miri (`cargo +nightly miri test`) and/or `loom` for exhaustive interleaving checks before trusting a weak ordering.

---

## Real-World Example

A background metrics worker that increments a `Relaxed` counter and watches a `Release`/`Acquire` shutdown flag: the two most common orderings, each used where it is exactly right.

```rust playground
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

/// Shared state for a background metrics worker.
struct Metrics {
    requests: AtomicU64,
    shutdown: AtomicBool,
}

fn main() {
    let metrics = Arc::new(Metrics {
        requests: AtomicU64::new(0),
        shutdown: AtomicBool::new(false),
    });

    // Worker: increments a counter until asked to stop.
    let worker = {
        let metrics = Arc::clone(&metrics);
        thread::spawn(move || {
            // Acquire pairs with the Release store in the shutdown signal below.
            while !metrics.shutdown.load(Ordering::Acquire) {
                // Relaxed: the count guards no other memory, so atomicity is enough.
                metrics.requests.fetch_add(1, Ordering::Relaxed);
                thread::sleep(Duration::from_micros(50));
            }
            // Final count after we observed the shutdown signal.
            metrics.requests.load(Ordering::Relaxed)
        })
    };

    thread::sleep(Duration::from_millis(5));

    // Signal shutdown. Release ensures every write we did before this store is
    // visible to the worker once it Acquire-loads `true`.
    metrics.shutdown.store(true, Ordering::Release);

    let processed = worker.join().unwrap();
    println!("worker processed {processed} requests before shutdown");
}
```

This compiles and runs; the exact count varies run to run (it depends on timing), but the worker always stops promptly after the `Release` store, and the printed total is always the value it actually reached, never a torn or stale number. The pattern generalizes directly to graceful shutdown of HTTP servers and background jobs; for OS-signal-driven shutdown see [Signal Handling and Clean Shutdown](/26-systems-programming/08-signals/).

> **Note:** For a *single* boolean stop flag like this, even `Relaxed` on both ends would technically suffice because the worker reads no other shared state through the flag. The `Acquire`/`Release` pair is the right habit to build, and it is what you must use the moment the flag also guards other data (a results buffer, a config pointer, etc.).

---

## Further Reading

- [`std::sync::atomic::Ordering`](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html) — the official enum docs with the formal guarantees.
- [The Rustonomicon: Atomics](https://doc.rust-lang.org/nomicon/atomics.html) — the canonical prose explanation of the memory model in Rust terms.
- *Rust Atomics and Locks* by Mara Bos — free online at <https://marabos.nl/atomics/>, the definitive treatment (Chapters 2-3 cover ordering).
- [`std::sync::atomic::fence`](https://doc.rust-lang.org/std/sync/atomic/fn.fence.html) — standalone memory fences, used in Exercise 3.
- Sibling topics: [Atomic Operations](/26-systems-programming/04-atomic-operations/) (the atomic types and methods that take these orderings), [Native Threads with `std::thread`](/26-systems-programming/00-threads/) (spawning the threads that share atomics), [Channels](/26-systems-programming/03-channels/) (a higher-level alternative to hand-rolled sharing), [Thread Pools with Rayon](/26-systems-programming/01-thread-pools/), and [Parallel Iterators with Rayon](/26-systems-programming/02-parallel-iterators/).
- Foundations: [Reference Counting with `Rc<T>` and `Arc<T>`](/05-ownership/07-reference-counting/) (`Arc`, which wraps the shared atomics here) and [Basic Types](/02-basics/01-types/) (the integer types behind `AtomicU64` and friends).
- Related: [Security](/27-security/), on why data-race freedom is part of Rust's safety story.

---

## Exercises

### Exercise 1: Fix the broken publish

**Difficulty:** Beginner

**Objective:** Recognize a missing happens-before edge and repair it with the correct orderings.

**Instructions:** The program below publishes a value through a flag using `Relaxed` on both ends, so the consumer's read of `data` is not guaranteed to see `1234`. Change *only* the orderings so the data is correctly published, and have the consumer `assert_eq!` it.

```rust playground
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

fn main() {
    let data = Arc::new(AtomicU64::new(0));
    let ready = Arc::new(AtomicBool::new(false));

    let (d, r) = (Arc::clone(&data), Arc::clone(&ready));
    let producer = thread::spawn(move || {
        d.store(1234, Ordering::Relaxed);
        r.store(true, /* ??? */ Ordering::Relaxed); // publish
    });

    let consumer = thread::spawn(move || {
        while !ready.load(/* ??? */ Ordering::Relaxed) {
            std::hint::spin_loop();
        }
        // TODO: assert the data is 1234
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}
```

<details><summary>Solution</summary>

```rust playground
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

fn main() {
    let data = Arc::new(AtomicU64::new(0));
    let ready = Arc::new(AtomicBool::new(false));

    let (d, r) = (Arc::clone(&data), Arc::clone(&ready));
    let producer = thread::spawn(move || {
        d.store(1234, Ordering::Relaxed);
        r.store(true, Ordering::Release); // publish
    });

    let consumer = thread::spawn(move || {
        while !ready.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        assert_eq!(data.load(Ordering::Relaxed), 1234);
        println!("ok: data correctly published");
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}
```

Output:

```text
ok: data correctly published
```

The `Release` store and the `Acquire` load that observes `true` create the happens-before edge that makes the `Relaxed` read of `data` see `1234`.

</details>

### Exercise 2: Run-once initialization with `compare_exchange`

**Difficulty:** Intermediate

**Objective:** Use a read-modify-write with `AcqRel` to ensure a block of work runs exactly once, even when many threads race to run it.

**Instructions:** Spawn eight threads that all try to perform a one-time initialization. Use an `AtomicU8` state machine (`UNINIT` → `RUNNING` → `DONE`) and `compare_exchange` so that exactly one thread wins the `UNINIT → RUNNING` transition and does the init. Prove only one ran by counting.

<details><summary>Solution</summary>

```rust playground
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread;

const UNINIT: u8 = 0;
const RUNNING: u8 = 1;
const DONE: u8 = 2;

fn main() {
    let state = Arc::new(AtomicU8::new(UNINIT));
    let init_count = Arc::new(AtomicU8::new(0));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let state = Arc::clone(&state);
            let init_count = Arc::clone(&init_count);
            thread::spawn(move || {
                // Only one thread wins the UNINIT -> RUNNING transition.
                // AcqRel on success: acquire any prior publication, release our
                // init writes. Acquire on failure: observe the winner's state.
                if state
                    .compare_exchange(UNINIT, RUNNING, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    init_count.fetch_add(1, Ordering::Relaxed); // "do the init"
                    state.store(DONE, Ordering::Release);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
    println!("init ran {} time(s)", init_count.load(Ordering::Relaxed));
}
```

Output:

```text
init ran 1 time(s)
```

> In real code you would not hand-roll this: `std::sync::OnceLock` / `OnceCell` and `std::sync::Once` encapsulate exactly this pattern with the correct orderings already chosen.

</details>

### Exercise 3: Publish with explicit fences

**Difficulty:** Advanced

**Objective:** Reproduce the release/acquire publication using a pair of standalone `fence`s plus `Relaxed` atomics, to understand that fences and tagged operations are two ways to express the same edge.

**Instructions:** Rewrite Exercise 1's publication so the flag store and the flag load are both `Relaxed`, but the producer issues a `fence(Ordering::Release)` *before* the flag store and the consumer issues a `fence(Ordering::Acquire)` *after* observing the flag. The data read must still be guaranteed to see the published value.

<details><summary>Solution</summary>

```rust playground
use std::sync::Arc;
use std::sync::atomic::{fence, AtomicBool, AtomicU64, Ordering};
use std::thread;

fn main() {
    let data = Arc::new(AtomicU64::new(0));
    let ready = Arc::new(AtomicBool::new(false));

    let (d, r) = (Arc::clone(&data), Arc::clone(&ready));
    let producer = thread::spawn(move || {
        d.store(7, Ordering::Relaxed);
        fence(Ordering::Release);         // release fence
        r.store(true, Ordering::Relaxed); // the Relaxed store is "covered" by the fence
    });

    let consumer = thread::spawn(move || {
        while !ready.load(Ordering::Relaxed) {
            std::hint::spin_loop();
        }
        fence(Ordering::Acquire);         // acquire fence pairs with the release fence
        assert_eq!(data.load(Ordering::Relaxed), 7);
        println!("ok: fence-paired publish works");
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}
```

Output:

```text
ok: fence-paired publish works
```

A `fence(Release)` before a `Relaxed` store, paired with a `fence(Acquire)` after the `Relaxed` load that observes it, builds the same happens-before edge as tagging the store/load themselves with `Release`/`Acquire`. Fences are useful when one fence needs to cover several nearby relaxed operations; for a single store/load, the tagged form in Exercise 1 is more idiomatic.

</details>
