---
title: "Interior Mutability: `RefCell<T>` and `Mutex<T>`"
description: "RefCell<T> and Mutex<T> let Rust mutate through a shared reference by moving the borrow check to runtime: RefCell panics, Mutex blocks across threads."
---

In TypeScript every object is freely mutable through any reference you hold — aliasing and mutation coexist without comment. Rust forbids that at compile time: you may have *many* shared `&` references **or** *one* exclusive `&mut`, never both. **Interior mutability** is the escape hatch that lets you mutate data through a shared reference anyway, by moving the "one writer, many readers" check from compile time to runtime. `RefCell<T>` does this on a single thread; `Mutex<T>` does it across threads.

---

## Quick Overview

`RefCell<T>` and `Mutex<T>` both let you mutate data you only hold a shared reference to (`&self` rather than `&mut self`). They enforce the same rule the borrow checker enforces statically — no aliased mutation — but they enforce it **dynamically**. `RefCell` checks on the current thread and **panics** if you break the rule; `Mutex` is thread-safe and **blocks** until the data is free. For a TypeScript developer the mental model is: these types give you back the casual "mutate through any handle" feeling of JavaScript objects, but with a guardrail that fails loudly instead of silently corrupting data.

> **Note:** This file covers the two *checked* interior-mutability containers. Its single-threaded sibling for `Copy` values, [`Cell<T>`](/10-smart-pointers/03-cell/), needs no runtime flag at all. For *sharing* the same data between owners (the thing you almost always combine `RefCell`/`Mutex` with) see [`Rc<T>` / `Arc<T>`](/10-smart-pointers/01-rc-arc/). When you cannot decide which container to reach for, jump to the [decision guide](/10-smart-pointers/07-comparison/).

---

## TypeScript/JavaScript Example

In TypeScript, a shared mutable object is the default. Here a `MetricsCollector` is handed to several subsystems; each holds the *same* object and mutates it freely. Nothing in the type system stops two code paths from writing at "the same time." JavaScript's single-threaded event loop makes that mostly safe, and TypeScript never asks you to think about it.

```typescript
// TypeScript/JavaScript - shared mutable state is the default
class MetricsCollector {
  private counts = new Map<string, number>();

  // `increment` mutates internal state, but callers never see a difference
  // between a "read" method and a "write" method — both just take `this`.
  increment(event: string): void {
    this.counts.set(event, (this.counts.get(event) ?? 0) + 1);
  }

  total(): number {
    let sum = 0;
    for (const n of this.counts.values()) sum += n;
    return sum;
  }
}

const metrics = new MetricsCollector();

// Two unrelated subsystems share the SAME collector via aliasing.
const auth = metrics;
const api = metrics;

auth.increment("login");
api.increment("request");
api.increment("request");

console.log(metrics.total()); // 3
```

Under Node v22 this prints `3`. Note three things a Rust developer will care about: the two aliases (`auth`, `api`) point at one object, `increment` mutates through a plain shared reference, and nobody had to declare the object "mutable."

---

## Rust Equivalent

The naive port fails: if `record` takes `&self`, you cannot assign to `self.hits`. Wrapping the field in `RefCell<T>` restores the ability to mutate through a shared reference: `borrow_mut()` hands you a temporary exclusive handle, checked at runtime.

```rust playground
use std::cell::RefCell;

#[derive(Debug)]
struct Counter {
    hits: RefCell<u32>,
}

impl Counter {
    fn new() -> Self {
        Counter { hits: RefCell::new(0) }
    }

    // Note: &self, NOT &mut self — the mutation happens "inside" the cell.
    fn record(&self) {
        *self.hits.borrow_mut() += 1;
    }

    fn total(&self) -> u32 {
        *self.hits.borrow()
    }
}

fn main() {
    let counter = Counter::new();
    counter.record();
    counter.record();
    counter.record();
    println!("total hits = {}", counter.total());
}
```

Running it prints:

```text
total hits = 3
```

The signature `fn record(&self)` is the whole point. The method takes a *shared* reference, yet mutates, exactly the TypeScript ergonomics, but the mutation is funneled through `RefCell`'s runtime check rather than being a free-for-all.

---

## Detailed Explanation

### Why the compiler refuses plain mutation through `&self`

Rust's borrowing rules are not a style guide; they are how the language guarantees memory safety without a garbage collector (see [Section 05 — Ownership](/05-ownership/)). The rule: at any instant a value has either **one** `&mut` reference or **any number** of `&` references. A method taking `&self` only has a shared reference, so writing to a field is rejected:

```rust
struct Logger {
    count: u32, // plain field, no interior mutability
}

impl Logger {
    fn log(&self, msg: &str) {
        println!("{msg}");
        self.count += 1; // does not compile (error[E0594]): cannot assign through &self
    }
}

fn main() {
    let logger = Logger { count: 0 };
    logger.log("hi");
}
```

The real compiler output:

```text
error[E0594]: cannot assign to `self.count`, which is behind a `&` reference
 --> src/main.rs:8:9
  |
8 |         self.count += 1; // does not compile (error[E0594]): cannot assign through &self
  |         ^^^^^^^^^^^^^^^ `self` is a `&` reference, so the data it refers to cannot be written
  |
help: consider changing this to be a mutable reference
  |
6 |     fn log(&mut self, msg: &str) {
  |             +++
```

Sometimes `&mut self` *is* the right fix. But it is contagious: every caller now needs exclusive access, which is impossible the moment the value is shared (behind an `Rc`, stored in a `Vec` you are iterating, captured by two closures). `RefCell` is the answer when you need the mutation but cannot get exclusive access at compile time.

### What `borrow` and `borrow_mut` actually return

`RefCell<T>` is a struct holding your `T` plus a small integer "borrow flag." Two methods read and update that flag:

- `borrow() -> Ref<'_, T>`: registers a *shared* borrow and returns a smart-pointer guard that derefs to `&T`.
- `borrow_mut() -> RefMut<'_, T>`: registers an *exclusive* borrow and returns a guard that derefs to `&mut T`.

The guard's `Drop` releases the borrow. So the flag tracks "how many `Ref`s and whether a `RefMut` is alive" exactly the way the compiler's static analysis would, just at runtime. The `*self.hits.borrow_mut() += 1` line creates a `RefMut`, dereferences it to a `&mut u32`, increments, and drops the guard at the end of the statement (the temporary's lifetime ends at the `;`).

### Runtime checking means runtime *panics*

If you violate the one-writer rule, `RefCell` does not return an error by default; it panics, the same way an out-of-bounds array index does. Holding a read borrow and then asking for a write borrow:

```rust
use std::cell::RefCell;

fn main() {
    let cell = RefCell::new(vec![1, 2, 3]);

    let reader = cell.borrow();          // Ref<Vec<i32>>, a shared read borrow
    println!("len is {}", reader.len());
    cell.borrow_mut().push(4);           // panics: reader is still alive
}
```

The real output:

```text
len is 3

thread 'main' panicked at src/main.rs:8:10:
RefCell already borrowed
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

`reader` is still in scope when `borrow_mut()` runs, so the borrow flag says "a shared borrow is active" and the write request panics. The fix is to end the read borrow first: drop `reader` or scope it in a `{ }` block.

### `Mutex<T>`: the same idea, made thread-safe

`RefCell` is **not** thread-safe (it is `!Sync`), so the compiler will not let you share one across threads. When you need interior mutability *across* threads, reach for `std::sync::Mutex<T>`. It enforces the identical rule — one writer at a time — but instead of panicking when the data is busy, `lock()` **blocks** the calling thread until the lock is free.

```rust playground
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    let counter = Arc::new(Mutex::new(0u64));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let counter = Arc::clone(&counter);
        let handle = thread::spawn(move || {
            for _ in 0..1000 {
                // lock() blocks until the mutex is free, returns a guard.
                let mut n = counter.lock().unwrap();
                *n += 1;
            } // guard dropped here -> lock released each iteration
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    println!("final count = {}", *counter.lock().unwrap());
}
```

Running it prints:

```text
final count = 8000
```

Eight threads each increment 1000 times; the `Mutex` serializes every `*n += 1`, so the result is exactly `8000` with no lost updates. The `Arc` (atomic reference count) is what lets all eight threads *own* the same mutex. `Mutex` provides the mutation, `Arc` provides the sharing. This `Arc<Mutex<T>>` pair is to multithreaded Rust what a plain shared object is to JavaScript.

> **Tip:** `lock()` returns a `Result` because the mutex can be *poisoned* (see Pitfalls). In application code, `.unwrap()` (or `.expect("lock poisoned")`) is the common, accepted choice: a poisoned lock usually means another thread already crashed.

### Holding the guard for as little time as possible

A `MutexGuard` (and a `RefMut`) keeps the lock until it is dropped. Wrapping the locked work in a block releases it early, before any expensive non-locked work runs:

```rust playground
use std::sync::Mutex;

fn main() {
    let data = Mutex::new(vec![10, 20, 30]);

    let sum: i32 = {
        let guard = data.lock().unwrap();
        guard.iter().sum()
    }; // guard dropped here, before the println formatting below

    println!("sum = {}", sum);
}
```

Output:

```text
sum = 60
```

### When you *do* have exclusive access, skip the lock

If you own a `Mutex<T>` by `&mut` (or by value), the compiler has already proven no other thread can touch it, so locking is pure overhead. `get_mut()` (borrow) and `into_inner()` (by value) reach the data without any locking:

```rust playground
use std::sync::Mutex;

fn main() {
    let mut m = Mutex::new(0);
    // We own `m` mutably, so the compiler proves no other thread can race.
    *m.get_mut().unwrap() += 100;
    println!("value = {}", m.into_inner().unwrap());
}
```

Output:

```text
value = 100
```

This mirrors `RefCell::get_mut`, and it is a good signal of intent: it tells the reader "no contention is possible here."

---

## Key Differences

| Concept | TypeScript / JavaScript | `RefCell<T>` (Rust) | `Mutex<T>` (Rust) |
| --- | --- | --- | --- |
| Mutate via shared handle | Always allowed | Allowed, runtime-checked | Allowed, runtime-checked |
| Aliased mutation rule | None enforced | One writer XOR many readers | One writer at a time |
| When the rule is broken | Possible silent bug | **Panics** immediately | (Cannot — it blocks) |
| Thread-safe? | N/A (single event loop) | No (`!Sync`) | Yes (`Send + Sync`) |
| Cost when uncontended | n/a | A few integer ops | An atomic compare-and-swap |
| Get exclusive access cheaply | n/a | `get_mut()` | `get_mut()`, `into_inner()` |
| Typical pairing for sharing | just alias the object | `Rc<RefCell<T>>` | `Arc<Mutex<T>>` |

A few conceptual contrasts worth internalizing:

- **Compile-time vs runtime.** Rust's default borrow checking is static and free. `RefCell` and `Mutex` *opt into* a runtime check, paying a little performance and (for `RefCell`) the risk of a panic, in exchange for flexibility the static checker cannot express.
- **Panic vs block.** This is the headline difference between the two types. A second `borrow_mut` on a busy `RefCell` is a *bug* and panics. A second `lock` on a busy `Mutex` is *normal contention* and waits. They feel similar but model opposite situations.
- **`Mutex<T>` wraps the data, it is not a separate object.** Unlike a Java `synchronized` block or a free-standing lock variable, a Rust `Mutex` *contains* the data it protects. You literally cannot read the value without locking: the type system makes "forgot to take the lock" impossible.
- **Unlike TypeScript,** sharing and mutating are separate concerns with separate tools. JavaScript gives you both in one move (alias an object). Rust makes you pick a *sharer* (`Rc`/`Arc`) and a *mutator* (`RefCell`/`Mutex`) and compose them.

---

## Common Pitfalls

### Pitfall 1: Two live `borrow_mut`s on the same thread

The single most common `RefCell` mistake, taking a second mutable borrow while the first is alive:

```rust
use std::cell::RefCell;

fn main() {
    let cell = RefCell::new(5);

    let mut first = cell.borrow_mut();
    let mut second = cell.borrow_mut(); // panics: already mutably borrowed
    *first += 1;
    *second += 1;
}
```

Real output:

```text
thread 'main' panicked at src/main.rs:7:27:
RefCell already borrowed
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

The opposite ordering (a read while a write is held) panics with a slightly different message — `RefCell already mutably borrowed` — but it is the same class of bug. **Fix:** never hold two borrows that overlap. Finish with the first guard (let it drop, or scope it) before taking the second, and prefer one short `borrow_mut()` per statement over long-lived guard variables.

### Pitfall 2: Trying to send a `RefCell` across threads

A `RefCell` looks like it should work in a thread, but it is `!Sync` and the compiler rejects sharing it: exactly the protection that pushes you toward `Mutex`:

```rust
use std::cell::RefCell;
use std::sync::Arc;
use std::thread;

fn main() {
    let shared = Arc::new(RefCell::new(0));

    let s = Arc::clone(&shared);
    let handle = thread::spawn(move || {
        *s.borrow_mut() += 1; // does not compile: RefCell is not Sync
    });

    handle.join().unwrap();
    println!("{}", shared.borrow());
}
```

The real error (abridged):

```text
error[E0277]: `RefCell<i32>` cannot be shared between threads safely
   --> src/main.rs:9:32
    |
  9 |       let handle = thread::spawn(move || {
    |  __________________-------------_^
...
    | |_____^ `RefCell<i32>` cannot be shared between threads safely
    |
    = help: the trait `Sync` is not implemented for `RefCell<i32>`
    = note: if you want to do aliasing and mutation between multiple threads, use `std::sync::RwLock` instead
    = note: required for `Arc<RefCell<i32>>` to implement `Send`
```

**Fix:** swap `RefCell` for `Mutex` (or `RwLock` if reads vastly outnumber writes) and `Rc` for `Arc`: `Arc<Mutex<i32>>`. The compiler error is doing you a favor: it caught a data race before it could exist.

### Pitfall 3: Locking the same `Mutex` twice on one thread (deadlock)

Because `lock()` *blocks* rather than panicking, locking a non-reentrant `Mutex` you already hold on the same thread deadlocks the thread forever:

```rust
// Conceptual — do NOT run; this hangs:
// let m = Mutex::new(0);
// let g1 = m.lock().unwrap();
// let g2 = m.lock().unwrap(); // thread blocks forever waiting for itself
```

There is no compiler error and no panic: the program simply hangs. **Fix:** never call `lock()` while you still hold a guard from the same mutex. Keep critical sections small, and if a helper needs the data, pass it `&mut T` from the already-acquired guard rather than re-locking.

> **Warning:** Rust's `Mutex` is **not reentrant**. Unlike some languages' recursive locks, taking the same lock twice on one thread is undefined-time hang, not a no-op.

### Pitfall 4: Forgetting that a long-lived guard blocks others

A `MutexGuard` held across an `.await` or a slow I/O call keeps every other thread (or task) waiting. In synchronous code this just hurts throughput; in async code it can stall an entire runtime (`std::sync::Mutex` should generally not be held across `.await`; see [Section 11 — Async](/11-async/)). **Fix:** copy or move what you need out of the guard, drop it, then do the slow work.

---

## Best Practices

- **Reach for the borrow checker first.** Interior mutability is a deliberate exception, not a default. If `&mut self` or restructuring the data threads cleanly, prefer it: you keep compile-time guarantees and zero runtime cost.
- **Keep borrows and locks short.** One `borrow_mut()`/`lock()` per statement when you can. Scope guards in `{ }` blocks so they drop early. Long-lived guards are how both panics (`RefCell`) and deadlocks (`Mutex`) happen.
- **Pick the container by thread-boundary, not by habit.** Single thread → `RefCell` (cheaper, no atomics). Crossing threads → `Mutex` (or `RwLock` for read-heavy workloads). The compiler enforces this for you via `Send`/`Sync`.
- **Compose intentionally:** `Rc<RefCell<T>>` for a shared, mutable, single-threaded graph; `Arc<Mutex<T>>` for the multithreaded version. Document *why* the shared mutability is needed.
- **Use `try_borrow_mut()` / `try_lock()` when contention is expected and recoverable.** They return a `Result` instead of panicking/blocking, so you can fall back gracefully:

  ```rust playground
  use std::cell::RefCell;

  fn main() {
      let cell = RefCell::new(String::from("hello"));
      let _read = cell.borrow();
      match cell.try_borrow_mut() {
          Ok(mut w) => w.push_str(" world"),
          Err(_) => println!("already borrowed; skipping mutation"),
      }
  }
  ```

  Output: `already borrowed; skipping mutation`.

- **Prefer the unlocked accessors when you own the data.** `get_mut()` and `into_inner()` skip the runtime check entirely and signal "no contention here" to the next reader of your code.

---

## Real-World Example

A **memoizing cache** is interior mutability's sweet spot: the *interface* is a read-only "look up a value," but the *implementation* must mutate a cache on a miss. With `RefCell` the public `get` method takes `&self`, so the memoizer composes like any other read-only service. Callers never need `&mut`.

```rust playground
use std::cell::RefCell;
use std::collections::HashMap;

/// A memoizing wrapper around an expensive pure function.
/// The cache mutates on a miss, yet `get` takes `&self` — callers
/// never need a `&mut Memoizer`, so the type composes like a read-only service.
struct Memoizer<F> {
    compute: F,
    cache: RefCell<HashMap<u64, u64>>,
    calls: RefCell<u64>, // how many times we actually ran `compute`
}

impl<F: Fn(u64) -> u64> Memoizer<F> {
    fn new(compute: F) -> Self {
        Memoizer {
            compute,
            cache: RefCell::new(HashMap::new()),
            calls: RefCell::new(0),
        }
    }

    fn get(&self, key: u64) -> u64 {
        // Fast path: check the cache, and release the borrow before recomputing.
        if let Some(&hit) = self.cache.borrow().get(&key) {
            return hit;
        }
        // Miss: no borrow of `cache` is held while we run the real computation.
        *self.calls.borrow_mut() += 1;
        let value = (self.compute)(key);
        self.cache.borrow_mut().insert(key, value);
        value
    }

    fn compute_calls(&self) -> u64 {
        *self.calls.borrow()
    }
}

fn main() {
    // Pretend this is an expensive call (DB hit, hashing, etc.).
    let memo = Memoizer::new(|n: u64| n * n);

    let inputs = [4, 4, 7, 4, 7, 9];
    let results: Vec<u64> = inputs.iter().map(|&n| memo.get(n)).collect();

    println!("results       = {results:?}");
    println!("inputs        = {inputs:?}");
    println!(
        "compute calls = {} (vs {} lookups)",
        memo.compute_calls(),
        inputs.len()
    );
}
```

Running it prints:

```text
results       = [16, 16, 49, 16, 49, 81]
inputs        = [4, 4, 7, 4, 7, 9]
compute calls = 3 (vs 6 lookups)
```

Six lookups, only three real computations: the three distinct inputs `4`, `7`, `9`. The important detail is in `get`: the `if let Some(&hit) = self.cache.borrow()...` borrow ends at the end of that `if`'s condition expression, so when we later call `self.cache.borrow_mut().insert(...)` there is no overlapping borrow to panic on. Sequencing borrows like this — read, drop, then write — is the discipline that keeps `RefCell` panic-free.

> **Note:** To make this cache thread-safe, change both `RefCell` to `Mutex`, wrap the whole `Memoizer` in an [`Arc`](/10-smart-pointers/01-rc-arc/), and lock instead of borrow. The structure is identical; only the runtime-check strategy changes.

---

## Further Reading

- [`std::cell::RefCell` — standard library docs](https://doc.rust-lang.org/std/cell/struct.RefCell.html)
- [`std::sync::Mutex` — standard library docs](https://doc.rust-lang.org/std/sync/struct.Mutex.html)
- [The Rust Book — `RefCell<T>` and the Interior Mutability Pattern](https://doc.rust-lang.org/book/ch15-05-interior-mutability.html)
- [The Rust Book — Shared-State Concurrency (`Mutex`/`Arc`)](https://doc.rust-lang.org/book/ch16-03-shared-state.html)
- Sibling topics: [`Cell<T>`](/10-smart-pointers/03-cell/) (unchecked, `Copy`-only interior mutability) · [`Rc<T>` / `Arc<T>`](/10-smart-pointers/01-rc-arc/) (shared ownership you compose these with) · [`Weak<T>`](/10-smart-pointers/05-weak/) (breaking cycles in `Rc<RefCell<…>>` graphs) · [Smart-pointer decision guide](/10-smart-pointers/07-comparison/)
- Foundations: [Section 05 — Ownership](/05-ownership/) (the borrow rules these types relax) · [Section 11 — Async](/11-async/) (why `std::sync::Mutex` should not be held across `.await`)

---

## Exercises

### Exercise 1: A rate limiter that counts down through `&self`

**Difficulty:** Easy

**Objective:** Get comfortable mutating state behind a shared reference with `RefCell`.

**Instructions:** Implement a `RateLimiter` with a `try_acquire(&self) -> bool` method. It starts with a quota and returns `true` (consuming one unit) while units remain, and `false` once exhausted. Note that `try_acquire` must take `&self`, not `&mut self`.

```rust
use std::cell::RefCell;

struct RateLimiter {
    remaining: RefCell<u32>,
}

impl RateLimiter {
    fn new(quota: u32) -> Self {
        /* ??? */
    }
    fn try_acquire(&self) -> bool {
        /* ??? */
    }
}

fn main() {
    let limiter = RateLimiter::new(2);
    // Expected: true true false
    println!("{} {} {}", limiter.try_acquire(), limiter.try_acquire(), limiter.try_acquire());
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::cell::RefCell;

struct RateLimiter {
    remaining: RefCell<u32>,
}

impl RateLimiter {
    fn new(quota: u32) -> Self {
        RateLimiter { remaining: RefCell::new(quota) }
    }

    fn try_acquire(&self) -> bool {
        let mut r = self.remaining.borrow_mut();
        if *r == 0 {
            false
        } else {
            *r -= 1;
            true
        }
    }
}

fn main() {
    let limiter = RateLimiter::new(2);
    println!(
        "{} {} {}",
        limiter.try_acquire(),
        limiter.try_acquire(),
        limiter.try_acquire()
    );
}
```

Output:

```text
true true false
```

The single `borrow_mut()` at the top of `try_acquire` is dropped when the method returns, so consecutive calls never overlap and never panic.

</details>

### Exercise 2: A shared event log written by two closures

**Difficulty:** Medium

**Objective:** Combine `Rc` (sharing) with `RefCell` (mutation): the single-threaded shared-mutable-state idiom.

**Instructions:** Build a `Vec<String>` event log that two independent logging closures both append to. Each closure should tag its messages with a name (e.g. `"auth"`, `"db"`). After running a few events, print the whole log in order. Hint: the log's type is `Rc<RefCell<Vec<String>>>`, and each closure should hold its own `Rc::clone`.

<details>
<summary>Solution</summary>

```rust playground
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<String>>>;

fn make_logger(log: &Log, name: &'static str) -> impl Fn(&str) {
    let log = Rc::clone(log);
    move |msg: &str| log.borrow_mut().push(format!("{name}: {msg}"))
}

fn main() {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let auth = make_logger(&log, "auth");
    let db = make_logger(&log, "db");

    auth("login ok");
    db("query 1");
    auth("logout");

    for line in log.borrow().iter() {
        println!("{line}");
    }
}
```

Output:

```text
auth: login ok
db: query 1
auth: logout
```

Each `make_logger` call takes its own `Rc::clone`, so both closures own the same heap-allocated `RefCell<Vec<String>>`. The `borrow_mut()` inside each closure is short-lived (one `push` then dropped), so the appends never collide.

</details>

### Exercise 3: Make a registry thread-safe

**Difficulty:** Hard

**Objective:** Convert a single-threaded interior-mutability pattern to a thread-safe one — `RefCell` → `Mutex`, `Rc` → `Arc` — and have several threads write to a shared collection.

**Instructions:** Spawn four threads, each of which pushes a value (`id * 10`) into a shared `Vec<u64>`. Join all threads, then print the collected values sorted ascending. You should reach for `Arc<Mutex<Vec<u64>>>`. (Why not `Rc<RefCell<…>>`? Try it and read the compiler error — it is Pitfall 2.)

<details>
<summary>Solution</summary>

```rust playground
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    let registry: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for id in 0..4u64 {
        let registry = Arc::clone(&registry);
        handles.push(thread::spawn(move || {
            registry.lock().unwrap().push(id * 10);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let mut final_ids = registry.lock().unwrap().clone();
    final_ids.sort_unstable();
    println!("{final_ids:?}");
}
```

Output:

```text
[0, 10, 20, 30]
```

Each thread takes its own `Arc::clone` (cheap reference-count bump, see [`Rc<T>`/`Arc<T>`](/10-smart-pointers/01-rc-arc/)) so all four own the same `Mutex`. The `lock()` serializes the four `push` calls, so no update is lost. We sort at the end because thread completion order — and therefore insertion order — is nondeterministic.

</details>
