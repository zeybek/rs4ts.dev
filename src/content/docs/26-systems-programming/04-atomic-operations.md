---
title: "Atomic Operations"
description: "Share a counter or flag across Rust threads lock-free with AtomicUsize and fetch_add: like JavaScript's Atomics, but type-safe and on ordinary values."
---

In JavaScript and TypeScript you almost never reach for atomics. Single-threaded event-loop code shares no mutable state between "threads," so a plain `let counter = 0; counter++` is always safe. The moment you spawn real OS threads in Rust, that assumption collapses: two threads incrementing the same integer is a **data race**, and Rust refuses to compile it. Atomic types are the smallest, fastest tool for fixing that: shared mutable numbers and flags that multiple threads can touch at once without a lock and without undefined behavior.

---

## Quick Overview

An **atomic type** is an integer or boolean whose reads and writes happen as a single, indivisible (atomic) hardware operation, so no thread can ever observe a half-written value. Rust's `std::sync::atomic` module gives you `AtomicBool`, `AtomicUsize`, `AtomicI64`, and friends, with methods like `load`, `store`, `fetch_add`, and `compare_exchange`. They let multiple threads share a counter or flag **without a mutex**, which matters because atomics are lock-free and dramatically cheaper than locking for simple numeric updates.

The closest thing a TypeScript developer has seen is the `Atomics` object that works on a `SharedArrayBuffer` across Web Workers: same idea, much narrower API. In Rust, atomics are a first-class, type-safe building block you will use constantly in multithreaded code.

---

## TypeScript/JavaScript Example

In a single-threaded event loop, sharing a counter is trivial, and that is exactly why TS/JS developers rarely think about atomicity:

```typescript
// Single-threaded JavaScript: this is always safe.
let requestCount = 0;

function handleRequest(): void {
  requestCount++; // read, add 1, write back — never interleaved with anything
}

handleRequest();
handleRequest();
console.log(requestCount); // 2
```

The only place JS exposes true cross-thread shared memory is `SharedArrayBuffer` plus the `Atomics` API, used to coordinate Web Workers. It is verbose and works on raw integer slots, not on ordinary variables:

```typescript
// main.ts — shared memory across Web Workers
const sab = new SharedArrayBuffer(8); // 8 bytes
const counter = new Int32Array(sab); // view over the buffer; counter[0] is our slot

// Send `sab` to several workers; each worker runs:
//   Atomics.add(counter, 0, 1);   // atomic counter[0] += 1
// then reads the result back:
const total = Atomics.load(counter, 0);
console.log(total);

// Atomics also offers compareExchange, store, and, or, etc.:
// Atomics.compareExchange(counter, 0, expected, replacement);
```

> **Note:** `Atomics` only works on a `SharedArrayBuffer`, never on a normal `number`. It exists precisely because Web Workers do not share the main thread's heap. Rust's atomics solve the same problem but apply to ordinary stack/heap values shared by OS threads.

---

## Rust Equivalent

Here is the idiomatic Rust version: a counter that eight threads increment in parallel, with no lock.

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

fn main() {
    // Wrap the atomic in `Arc` so multiple threads can share ownership.
    let counter = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                // Atomic read-modify-write: no torn values, no lost updates.
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // 8 threads * 1000 increments = 8000, every single time.
    println!("total = {}", counter.load(Ordering::Relaxed));
}
```

Real output:

```
total = 8000
```

> **Tip:** Notice there is no `mut` anywhere. Atomic methods take `&self`, not `&mut self`, because the synchronization happens *inside* the type. This is **interior mutability** — the same pattern `Cell` and `RefCell` use, but thread-safe. See [Smart Pointers: Cell](/10-smart-pointers/03-cell/) for the broader concept.

---

## Detailed Explanation

### The three building blocks

Every atomic exposes the same core trio:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    let counter = AtomicUsize::new(0);

    // store: write a value atomically.
    counter.store(10, Ordering::Relaxed);

    // load: read the current value atomically.
    let v = counter.load(Ordering::Relaxed);
    println!("value = {v}");

    // fetch_add: read, add, write back — all as one indivisible step.
    // Returns the value *before* the add.
    let previous = counter.fetch_add(5, Ordering::Relaxed);
    println!("was {previous}, now {}", counter.load(Ordering::Relaxed));
}
```

Real output:

```
value = 10
was 10, now 15
```

`load` and `store` are the atomic equivalents of reading and writing a variable. The interesting one is `fetch_add`: in JavaScript `counter++` is three separate machine steps (read, increment, write), and on real threads another thread can sneak in between them and clobber your write, a **lost update**. `fetch_add` fuses all three into a single hardware instruction that no other thread can interrupt.

### Why `Arc`?

A bare `AtomicUsize` lives on one thread's stack. To let several threads reach the *same* atomic, you need shared ownership that outlives every thread. That is exactly what `Arc` (atomically reference-counted pointer) provides. `Arc::clone` is cheap: it bumps a reference count, it does **not** copy the atomic, so all clones point at one shared value.

You will see this `Arc<Atomic…>` pairing constantly. The atomic gives you safe shared *mutation*; the `Arc` gives you safe shared *ownership*. (For values that need general shared mutable state rather than a single number, you reach for `Arc<Mutex<T>>` instead — see [Smart Pointers: RefCell and Mutex](/10-smart-pointers/02-refcell-mutex/).)

### The `Ordering` argument

Every atomic method takes an `Ordering`. It controls how this operation synchronizes with the memory *around* it: what other threads are guaranteed to see. For a standalone counter where you only care about the count itself, `Ordering::Relaxed` is correct and fastest. Once an atomic acts as a *signal* that guards other data (a "ready" flag protecting a buffer), you need stronger orderings like `Acquire`/`Release`. That is a deep topic with its own file: see [Memory Ordering](/26-systems-programming/05-memory-ordering/). For now, internalize one rule:

> **Note:** `Relaxed` guarantees the *atomicity* of the single operation (no torn reads, no lost updates) but makes **no promises** about the visibility ordering of other memory. Use it only when the atomic's value is the *only* thing you care about, such as a plain counter.

### `compare_exchange`: the heart of lock-free programming

`compare_exchange(expected, new, success_ordering, failure_ordering)` is a **compare-and-swap (CAS)**. It atomically checks "is the value still `expected`? If so, replace it with `new`." It returns a `Result`:

- `Ok(previous)` if the swap happened (the value was `expected`).
- `Err(actual)` if it did not (the value was something else, and `actual` tells you what).

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    let value = AtomicUsize::new(100);

    // Swap to 200 only if the current value is still 100.
    let res = value.compare_exchange(100, 200, Ordering::SeqCst, Ordering::SeqCst);
    println!("first attempt: {:?}", res);

    // Now the value is 200, so this CAS fails and reports the real value.
    let res = value.compare_exchange(100, 999, Ordering::SeqCst, Ordering::SeqCst);
    println!("second attempt: {:?}", res);
    println!("final = {}", value.load(Ordering::SeqCst));
}
```

Real output:

```
first attempt: Ok(100)
second attempt: Err(200)
final = 200
```

This is the JS `Atomics.compareExchange(view, index, expected, replacement)` pattern, but Rust's version returns a `Result` you must handle, and it works on a type-safe atomic rather than an integer slot in a buffer.

### The CAS loop pattern

CAS shines when you need a custom read-modify-write that no single `fetch_*` method provides, for example "store the maximum value any thread has seen." You read the current value, compute the new one, and try to swap. If another thread changed it underneath you, you retry with the fresh value:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    let max = AtomicUsize::new(0);
    let samples = [3usize, 1, 7, 2, 5];

    for &s in &samples {
        let mut current = max.load(Ordering::Relaxed);
        while s > current {
            // Try to bump `max` from `current` up to `s`.
            match max.compare_exchange_weak(
                current,
                s,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,                 // we won; done with this sample
                Err(actual) => current = actual, // someone changed it; retry
            }
        }
    }

    println!("max = {}", max.load(Ordering::Relaxed));
}
```

Real output:

```
max = 7
```

`compare_exchange_weak` is used inside loops like this. It is allowed to fail *spuriously* (return `Err` even when the value matched) on some CPU architectures, which lets it compile to a single tighter instruction. Because the loop already retries on failure, a spurious failure costs nothing, so prefer `_weak` in loops and the stronger `compare_exchange` for one-shot, non-looping swaps.

> **Tip:** The standard library already wraps this loop for you in `fetch_max`/`fetch_min` and the general-purpose `fetch_update`. Reach for those first; write a manual CAS loop only when your update logic is more complex than a single closure can express.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Default concurrency model | Single-threaded event loop; no shared mutable memory | Real OS threads sharing the heap |
| Shared atomic integers | `Atomics` on a `SharedArrayBuffer` only | First-class `AtomicUsize`, `AtomicI64`, etc. on ordinary values |
| Atomic boolean | None (use an `Int32Array` slot) | Dedicated `AtomicBool` |
| Increment | `counter++` (safe only single-threaded) | `counter.fetch_add(1, ordering)` |
| Compare-and-swap | `Atomics.compareExchange(...)` returns the old value | `compare_exchange(...)` returns `Result<old, actual>` |
| Memory ordering control | Implicit sequential consistency | Explicit `Ordering` argument on every call |
| Mutability | Mutate freely | `&self` methods; interior mutability, no `mut` needed |
| Sharing across threads | Pass the `SharedArrayBuffer` | Wrap in `Arc` |
| Safety if you get it wrong | Runtime bug, possibly silent | Won't compile, or you opt into well-defined `Relaxed` semantics |

### Why explicit memory ordering at all?

JavaScript's `Atomics` are always sequentially consistent: the strongest, simplest, slowest guarantee. Rust exposes the full spectrum (`Relaxed` through `SeqCst`) because systems code often pays for synchronization it does not need. The trade-off is that *you* choose. For a standalone counter, `Relaxed` is both correct and the fastest; for an atomic that gates access to other data, you need `Acquire`/`Release`. The full reasoning lives in [Memory Ordering](/26-systems-programming/05-memory-ordering/).

### Atomics are not a general-purpose `Mutex`

Atomics only work on machine-word-sized primitives: booleans, integers, and pointers. You cannot atomically update a `String`, a `Vec`, or a struct with one operation. The moment your shared state is more than a single number or flag, you need a `Mutex` or `RwLock` (see [Smart Pointers: RefCell and Mutex](/10-smart-pointers/02-refcell-mutex/)). Atomics are the scalpel; mutexes are the general tool.

---

## Common Pitfalls

### Pitfall 1: Moving an atomic into multiple threads instead of sharing it

A `move` closure takes ownership. Spawning the *second* thread tries to move the same atomic again, which is impossible: `AtomicUsize` is not `Copy`:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

fn main() {
    let counter = AtomicUsize::new(0); // does not compile (error[E0382])
    let mut handles = Vec::new();
    for _ in 0..4 {
        handles.push(thread::spawn(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!("{}", counter.load(Ordering::Relaxed));
}
```

The real compiler error:

```
error[E0382]: borrow of moved value: `counter`
  --> src/main.rs:15:20
   |
 5 |     let counter = AtomicUsize::new(0);
   |         ------- move occurs because `counter` has type `AtomicUsize`, which does not implement the `Copy` trait
...
 8 |         handles.push(thread::spawn(move || {
   |                                    ------- value moved into closure here, in previous iteration of loop
...
15 |     println!("{}", counter.load(Ordering::Relaxed));
   |                    ^^^^^^^ value borrowed here after move
```

**Fix:** wrap the atomic in `Arc` and `Arc::clone` it once per thread, as the [Rust Equivalent](#rust-equivalent) example does. Each thread gets its own handle to the one shared atomic.

### Pitfall 2: Borrowing the atomic across threads without `Arc`

A natural-looking shortcut is to hand each thread a reference `&counter`. It fails because `thread::spawn` requires its closure to be `'static`: the threads might outlive `main`'s stack frame, so a borrow of a local will not do:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

fn main() {
    let counter = AtomicUsize::new(0);
    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = &counter; // does not compile (error[E0597])
        handles.push(thread::spawn(move || {
            c.fetch_add(1, Ordering::Relaxed);
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!("{}", counter.load(Ordering::Relaxed));
}
```

The real compiler error:

```
error[E0597]: `counter` does not live long enough
  --> src/main.rs:8:17
   |
 5 |       let counter = AtomicUsize::new(0);
   |           ------- binding `counter` declared here
...
 8 |           let c = &counter;
   |                   ^^^^^^^^ borrowed value does not live long enough
 9 |           handles.push(thread::spawn(move || {
   |  ______________________-
10 | |             c.fetch_add(1, Ordering::Relaxed);
11 | |         }));
   | |__________- argument requires that `counter` is borrowed for `'static`
...
17 |   }
   |   - `counter` dropped here while still borrowed
```

**Fix:** use `Arc` for `thread::spawn`. (If you genuinely want to *borrow* a stack atomic across threads without `Arc`, that is what **scoped threads** are for — see [Native Threads with `std::thread`](/26-systems-programming/00-threads/).)

### Pitfall 3: Treating two atomic ops as one

Atomicity applies to *each call*, not to a sequence of calls. This `load`-then-`store` is two separate atomic operations, so another thread can slip in between them and you lose updates, exactly the bug atomics were supposed to prevent:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    let counter = AtomicUsize::new(0);

    // NOT atomic as a whole — another thread can change `counter`
    // between the load and the store, silently dropping an increment.
    let current = counter.load(Ordering::Relaxed);
    counter.store(current + 1, Ordering::Relaxed);

    println!("{}", counter.load(Ordering::Relaxed));
}
```

This compiles and prints `1` on a single thread, but under contention it loses updates. **Fix:** use the fused read-modify-write methods (`fetch_add`, `fetch_or`, `fetch_max`, …) or a `compare_exchange` loop, which perform the read and the write as one indivisible step.

### Pitfall 4: Reaching for `SeqCst` everywhere "to be safe"

`SeqCst` (sequential consistency) is the strongest and most expensive ordering. Defaulting to it for a simple counter is a common reflex carried over from JavaScript's always-sequential `Atomics`. It is not *wrong*, but it leaves performance on the table. For a standalone counter, `Relaxed` is correct. Pick the weakest ordering that still gives the guarantee you need; see [Memory Ordering](/26-systems-programming/05-memory-ordering/) for how to reason about that choice.

---

## Best Practices

- **Prefer the highest-level method that fits.** Use `fetch_add`, `fetch_sub`, `fetch_max`, `fetch_min`, `fetch_or`, `fetch_and`, or the general `fetch_update` before writing a manual CAS loop. Each is a single, correct, optimized operation.
- **Use `Relaxed` for standalone counters and statistics.** It is the fastest and is correct whenever the atomic's value is the only thing you care about. Escalate to `Acquire`/`Release` only when the atomic guards *other* data.
- **Use `compare_exchange_weak` inside loops, `compare_exchange` for one-shot swaps.** The weak form may fail spuriously but compiles to tighter code; the loop already handles retries.
- **Pair atomics with `Arc` to share them across threads.** `Arc<AtomicUsize>` is the canonical multithreaded counter. Use scoped threads ([Native Threads with `std::thread`](/26-systems-programming/00-threads/)) if you want to avoid `Arc` for stack-local atomics.
- **Reach for a `Mutex`/`RwLock` when the data is not a single primitive.** Atomics cannot make a `Vec` or `HashMap` thread-safe; do not try to fake it with a flag.
- **When exclusive access is available, skip synchronization entirely.** `get_mut()` returns an `&mut` to the inner value (no atomic operation needed) and `into_inner()` consumes the atomic to return the plain value — both are free because the borrow checker has proven no other thread can touch it:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    let mut owned = AtomicUsize::new(1);
    // Exclusive `&mut` access: no synchronization required.
    *owned.get_mut() += 41;
    println!("get_mut: {}", owned.load(Ordering::Relaxed));

    // Consume the atomic and recover the plain value.
    let n = AtomicUsize::new(7).into_inner();
    println!("into_inner: {n}");
}
```

Real output:

```
get_mut: 42
into_inner: 7
```

---

## Real-World Example

A common production need is a **lock-free unique ID generator** shared across worker threads — every request, job, or span needs a distinct ID, and you do not want a mutex on the hot path. `fetch_add` returns the value *before* the increment, so each caller atomically claims a unique number, even under heavy contention. An `AtomicBool` provides a clean cooperative shutdown signal at the same time.

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// A lock-free, monotonically increasing ID generator shared across threads.
struct IdGenerator {
    next: AtomicU64,
}

impl IdGenerator {
    fn new() -> Self {
        IdGenerator { next: AtomicU64::new(1) }
    }

    /// Returns a unique ID. `fetch_add` returns the value *before* the add,
    /// so each caller gets a distinct number even when threads race.
    fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

fn main() {
    let id_gen = Arc::new(IdGenerator::new());
    let shutdown = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::new();
    for _ in 0..4 {
        let id_gen = Arc::clone(&id_gen);
        let shutdown = Arc::clone(&shutdown);
        handles.push(thread::spawn(move || {
            let mut ids = Vec::new();
            // Run until the main thread flips the shutdown flag.
            while !shutdown.load(Ordering::Relaxed) {
                ids.push(id_gen.next_id());
                if ids.len() >= 250 {
                    break;
                }
            }
            ids
        }));
    }

    thread::sleep(Duration::from_millis(10));
    shutdown.store(true, Ordering::Relaxed); // signal every worker to stop

    let mut all: Vec<u64> = Vec::new();
    for h in handles {
        all.extend(h.join().unwrap());
    }

    let total = all.len();
    all.sort_unstable();
    all.dedup();
    let unique = all.len();
    println!("generated {total} IDs, {unique} unique");
}
```

Real output:

```
generated 1000 IDs, 1000 unique
```

Every ID is unique with zero locking. `fetch_add` guarantees no two threads ever receive the same number. The `AtomicBool` flag gives you a race-free way to ask all workers to wind down. This is the foundation of request counters, span/trace IDs, sequence numbers, and graceful-shutdown switches in real services. For coordinating shutdown on an actual OS signal (Ctrl-C / SIGTERM), see [Signal Handling and Clean Shutdown](/26-systems-programming/08-signals/).

> **Warning:** A `u64` counter is effectively inexhaustible (over 18 quintillion IDs), but it *can* technically wrap on overflow. Unlike the `+` operator (which panics on overflow in debug builds), atomic `fetch_add` always wraps silently in both debug and release builds: it performs no overflow check. If uniqueness is safety-critical, choose a width you will never exhaust (a `u64` gives over 18 quintillion IDs) or detect the wrap explicitly with a CAS loop.

---

## Further Reading

- [`std::sync::atomic` module documentation](https://doc.rust-lang.org/std/sync/atomic/index.html) — the full list of atomic types and methods.
- [`AtomicUsize`](https://doc.rust-lang.org/std/sync/atomic/struct.AtomicUsize.html) and [`AtomicBool`](https://doc.rust-lang.org/std/sync/atomic/struct.AtomicBool.html) — the two you will use most.
- [`Ordering`](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html) — the memory-ordering enum every method takes.
- [The Rustonomicon: Atomics](https://doc.rust-lang.org/nomicon/atomics.html) — the deeper "why" behind the memory model.
- [Memory Ordering](/26-systems-programming/05-memory-ordering/) — the companion topic explaining `Relaxed`/`Acquire`/`Release`/`AcqRel`/`SeqCst`.
- [Native Threads with `std::thread`](/26-systems-programming/00-threads/) — `std::thread`, scoped threads, and how atomics fit into spawning.
- [Channels](/26-systems-programming/03-channels/) — when message passing is a cleaner alternative to shared atomics.
- [Smart Pointers: Rc and Arc](/10-smart-pointers/01-rc-arc/) and [RefCell and Mutex](/10-smart-pointers/02-refcell-mutex/) — `Arc`, `Mutex`, and `RwLock` for non-primitive shared state.
- [Section 27: Security](/27-security/) — why eliminating data races at compile time is a security property as well as a correctness one.

---

## Exercises

### Exercise 1: Parallel counter

**Difficulty:** Beginner

**Objective:** Get comfortable with `Arc<AtomicUsize>` and `fetch_add`.

**Instructions:** Spawn 10 threads. Each thread should increment a shared counter 100 times. After all threads finish, print the total (it must be exactly 1000 every run). Use `fetch_add` with `Ordering::Relaxed`.

<details>
<summary>Solution</summary>

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

fn main() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..10 {
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    println!("final = {}", counter.load(Ordering::Relaxed));
}
```

Real output:

```
final = 1000
```

</details>

### Exercise 2: Run-once guard

**Difficulty:** Intermediate

**Objective:** Use `compare_exchange` to let exactly one thread "win" a race.

**Instructions:** Build a `OnceFlag` type backed by an `AtomicBool`. Its `try_run(&self) -> bool` method should return `true` for the *first* caller and `false` for everyone else, using a single `compare_exchange`. Spawn 16 threads that all call `try_run`, count how many got `true`, and confirm the count is exactly 1.

<details>
<summary>Solution</summary>

```rust
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

struct OnceFlag {
    done: AtomicBool,
}

impl OnceFlag {
    fn new() -> Self {
        OnceFlag { done: AtomicBool::new(false) }
    }

    /// Returns true exactly once, for the first caller.
    fn try_run(&self) -> bool {
        self.done
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

fn main() {
    let flag = Arc::new(OnceFlag::new());
    let winners = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..16 {
        let flag = Arc::clone(&flag);
        let winners = Arc::clone(&winners);
        handles.push(thread::spawn(move || {
            if flag.try_run() {
                winners.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    println!("winners = {}", winners.load(Ordering::Relaxed));
}
```

Real output:

```
winners = 1
```

Only one thread can move the flag from `false` to `true`; every other `compare_exchange` sees `true` and returns `Err`, so `try_run` returns `false`. The `AcqRel`/`Acquire` orderings make this a proper synchronization point — useful when the "winner" goes on to initialize shared data others will read.

</details>

### Exercise 3: A spinlock from scratch

**Difficulty:** Advanced

**Objective:** Build a mutual-exclusion primitive using only `AtomicBool` and `compare_exchange_weak`.

**Instructions:** Implement a `SpinLock` with `lock(&self)` and `unlock(&self)`. `lock` should spin (busy-wait) using `compare_exchange_weak` until it flips the flag from `false` to `true`; `unlock` should `store(false)`. Use `Ordering::Acquire` when locking and `Ordering::Release` when unlocking so the critical section is properly fenced, and call `std::hint::spin_loop()` while spinning. Then have 8 threads each take the lock 1000 times to increment a shared counter, and verify the total is 8000.

> **Note:** A real spinlock is only appropriate for *extremely* short critical sections; for anything else use `std::sync::Mutex`. This exercise is about understanding the mechanism, not replacing the standard library.

<details>
<summary>Solution</summary>

```rust
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

struct SpinLock {
    locked: AtomicBool,
}

impl SpinLock {
    fn new() -> Self {
        SpinLock { locked: AtomicBool::new(false) }
    }

    fn lock(&self) {
        // Spin until we successfully flip `false` -> `true`.
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // Hint to the CPU that we're in a busy-wait loop.
            std::hint::spin_loop();
        }
    }

    fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

fn main() {
    let lock = Arc::new(SpinLock::new());
    let counter = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let lock = Arc::clone(&lock);
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                lock.lock();
                // Critical section: a non-atomic-style read+write is safe
                // here because the spinlock guarantees exclusive access.
                let now = counter.load(Ordering::Relaxed);
                counter.store(now + 1, Ordering::Relaxed);
                lock.unlock();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    println!("count = {}", counter.load(Ordering::Relaxed));
}
```

Real output:

```
count = 8000
```

The `compare_exchange_weak` is ideal here because the surrounding `while` loop already retries on failure, so a spurious failure is harmless and the weak form compiles to tighter code. The `Acquire` on lock and `Release` on unlock ensure that everything done inside the critical section is visible to the next thread that acquires the lock.

</details>
