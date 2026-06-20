---
title: "Async Synchronization Primitives"
description: "JavaScript never makes you pick a lock; Rust does. Choose std's blocking Mutex/RwLock or Tokio's async ones by one rule: do you hold the guard across an .await?"
---

When several Rust tasks share mutable state, you reach for a lock, but in async code you have a choice JavaScript never forces on you: a *blocking* lock from the standard library, or an *async-aware* lock from Tokio. Picking the wrong one leads to subtle deadlocks, performance cliffs, or a confusing `future cannot be sent between threads safely` error. This page explains the difference and the one rule that drives the whole decision.

---

## Quick Overview

Tokio provides async-aware versions of the standard library's synchronization types: **`tokio::sync::Mutex`**, **`tokio::sync::RwLock`**, and **`tokio::sync::Semaphore`**. Unlike their `std::sync` counterparts, their locking methods are `async` (you write `lock().await`), so a task that cannot acquire the lock *yields* to the runtime instead of blocking the OS thread. The decisive question is whether you need to **hold a lock across an `.await` point**: if you do, you usually need the Tokio version; if you do not, the plain `std::sync` lock is faster and simpler.

> **Note:** Every Rust snippet on this page was compiled and run with `cargo`/`rustc` 1.96.0 (current stable; 2024 edition). Async examples use `tokio = { version = "1.52", features = ["full"] }`. Rust ships **no built-in async runtime**, so the runtime is always explicit. See [Tokio Setup](/11-async/03-tokio-setup/).

---

## TypeScript/JavaScript Example

JavaScript has no concept of a lock, because Node runs your code on a **single thread** with a cooperative event loop. Two `async` functions can interleave at `await` points, but they never *truly* run at the same instant, so there is no data race on a plain object. Developers instead reach for ad-hoc concurrency control like a "mutex" promise chain, or a `p-limit`-style semaphore to cap concurrent I/O.

```typescript
// TypeScript / JavaScript (Node v22)
// There is no real shared-memory race here: the event loop is single-threaded.
// But we DO need to limit concurrency, and to serialize a read-modify-write
// that spans multiple awaits.

// A hand-rolled async mutex: each acquire() waits for the previous release().
class AsyncMutex {
  private tail: Promise<void> = Promise.resolve();

  async runExclusive<T>(fn: () => Promise<T>): Promise<T> {
    const prev = this.tail;
    let release!: () => void;
    this.tail = new Promise((resolve) => (release = resolve));
    await prev; // wait our turn
    try {
      return await fn();
    } finally {
      release(); // let the next waiter proceed
    }
  }
}

// A semaphore: at most `max` operations run concurrently.
class Semaphore {
  private permits: number;
  private waiters: Array<() => void> = [];
  constructor(max: number) {
    this.permits = max;
  }
  async acquire(): Promise<void> {
    if (this.permits > 0) {
      this.permits--;
      return;
    }
    await new Promise<void>((resolve) => this.waiters.push(resolve));
    this.permits--;
  }
  release(): void {
    this.permits++;
    this.waiters.shift()?.();
  }
}

const stats = { completed: 0, bytes: 0 };
const lock = new AsyncMutex();
const limit = new Semaphore(2);

async function download(id: number): Promise<number> {
  await new Promise((r) => setTimeout(r, 20));
  return id * 1000;
}

async function worker(id: number): Promise<void> {
  await limit.acquire();
  try {
    const bytes = await download(id);
    // A read-modify-write that crosses an await must be serialized.
    await lock.runExclusive(async () => {
      stats.completed += 1;
      stats.bytes += bytes;
    });
  } finally {
    limit.release();
  }
}

await Promise.all([1, 2, 3, 4, 5].map(worker));
console.log(stats); // { completed: 5, bytes: 15000 }
```

Two things to notice, because they map directly onto the Rust version:

- The `AsyncMutex` exists to serialize an operation that **spans `await`s**, not to prevent a CPU-level data race (Node's single thread already prevents those).
- The `Semaphore` exists to **cap concurrency**: limit how many downloads are in flight.

---

## Rust Equivalent

Rust gives you these primitives as battle-tested library types instead of hand-rolled classes. Because tasks can run on **different OS threads** under Tokio's multi-thread scheduler, the lock does real memory-safety work here, beyond just ordering.

```rust playground
// Rust — tokio::sync primitives for shared async state
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{sleep, Duration};

/// Shared, mutable counters updated by every worker.
struct Stats {
    completed: u32,
    bytes: u64,
}

async fn download(id: u32) -> u64 {
    sleep(Duration::from_millis(20)).await; // pretend this is a network call
    (id as u64) * 1000
}

#[tokio::main]
async fn main() {
    // At most 2 downloads may run at the same time (like the JS Semaphore).
    let limit = Arc::new(Semaphore::new(2));
    // Shared mutable state guarded by an async-aware Mutex.
    let stats = Arc::new(Mutex::new(Stats { completed: 0, bytes: 0 }));

    let mut handles = Vec::new();
    for id in 1..=5 {
        let limit = Arc::clone(&limit);
        let stats = Arc::clone(&stats);
        handles.push(tokio::spawn(async move {
            // Acquire a permit; this future waits if 2 are already running.
            let _permit = limit.acquire().await.unwrap();

            let bytes = download(id).await;

            // Lock the stats just long enough to update them.
            let mut s = stats.lock().await;
            s.completed += 1;
            s.bytes += bytes;
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let s = stats.lock().await;
    println!("completed = {}, bytes = {}", s.completed, s.bytes);
}
```

Real output (Rust 1.96.0, Tokio 1.52.3):

```text
completed = 5, bytes = 15000
```

The `Arc<...>` wrapping is what lets multiple tasks own a handle to the same value. That pattern has its own page, [Arc + Mutex pattern](/11-async/12-arc-mutex-pattern/). Here we focus on *which lock* goes inside the `Arc`.

---

## Detailed Explanation

### The `std` vs `tokio` lock families

For every common lock, there are two implementations:

| Job | `std::sync` (blocking) | `tokio::sync` (async-aware) |
| --- | --- | --- |
| Exclusive access | `std::sync::Mutex<T>` | `tokio::sync::Mutex<T>` |
| Many readers / one writer | `std::sync::RwLock<T>` | `tokio::sync::RwLock<T>` |
| Cap concurrency | *(none in std)* | `tokio::sync::Semaphore` |

The standard library's `Mutex::lock()` returns a `LockGuard` directly and **blocks the current OS thread** if the lock is taken. Tokio's `Mutex::lock()` is an `async fn` returning a future; awaiting it **suspends only the current task** and lets the runtime schedule other tasks on that thread while it waits.

```rust
// std: synchronous, blocks the thread
let guard = std_mutex.lock().unwrap(); // .unwrap() because std locks can be "poisoned"

// tokio: asynchronous, yields the task
let guard = tokio_mutex.lock().await;  // no Result — tokio locks are not poisoned
```

> **Tip:** A small but real ergonomic difference: `std::sync::Mutex::lock()` returns a `Result` because a `std` lock becomes *poisoned* if a thread panics while holding it. `tokio::sync::Mutex::lock()` returns the guard directly — Tokio locks are not poisonable — so there is no `.unwrap()`.

### The one rule: holding a guard across `.await`

This is the heart of the page. A `std::sync::MutexGuard` is **not `Send`** — it cannot be moved to another thread. A Tokio task may be paused at an `.await` and resumed **on a different worker thread**. So if you hold a `std` guard across an `.await`, the compiler refuses to let that future be spawned, with a `future cannot be sent between threads safely` error (shown in [Common Pitfalls](#common-pitfalls)).

The decision tree is therefore:

- **Do you keep the lock locked while you `.await` something?** → use `tokio::sync::Mutex` / `RwLock`.
- **Do you only lock, touch the data, and unlock, with no `.await` in between?** → use `std::sync::Mutex`. It is faster and avoids dragging async machinery into a tiny critical section.

### Why `std` locks are often the *better* choice

It is a common misconception that "async code must use async locks everywhere." In fact, for a short critical section that does no `.await`, `std::sync::Mutex` is the idiomatic and faster choice; even Tokio's own documentation recommends it. The async `Mutex` carries extra bookkeeping (a wait queue of tasks) and lets the task yield, which is pure overhead if you never actually need to yield while holding the lock.

```rust playground
// std: drop the guard in a tight scope BEFORE any .await
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let data = Arc::new(Mutex::new(0_u32));
    let d = Arc::clone(&data);
    let handle = tokio::spawn(async move {
        // Compute under the lock, then release it before awaiting.
        {
            let mut guard = d.lock().unwrap();
            *guard += 1;
        } // guard dropped here — lock released

        sleep(Duration::from_millis(10)).await; // no lock held across await
    });
    handle.await.unwrap();
    println!("{}", *data.lock().unwrap());
}
```

Real output:

```text
1
```

The explicit `{ ... }` scope is the workhorse pattern: it forces the guard's `Drop` (which releases the lock) to run *before* the `.await`, keeping the future `Send`.

### `RwLock`: many readers, one writer

When reads vastly outnumber writes (a config map, a cache index), an `RwLock` lets many readers proceed concurrently and only serializes writers.

```rust playground
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    let config = Arc::new(RwLock::new(vec!["a".to_string()]));

    // Many readers can hold the read lock at once.
    let r1 = Arc::clone(&config);
    let reader = tokio::spawn(async move {
        let guard = r1.read().await;
        guard.len()
    });

    // A single writer needs exclusive access.
    {
        let mut guard = config.write().await;
        guard.push("b".to_string());
    }

    println!("reader saw len in [1,2]: {}", reader.await.unwrap());
    println!("final len = {}", config.read().await.len());
}
```

Real output:

```text
reader saw len in [1,2]: 2
final len = 2
```

> **Note:** The reader's result is `1` or `2` depending on whether it ran before or after the writer — both are correct. The point is that `read().await` and `write().await` coordinate access; you never observe a torn, half-written `Vec`.

### `Semaphore`: capping concurrency

A `Semaphore` hands out a fixed number of *permits*. A task calls `acquire().await`; if no permit is free, the task waits. When the returned permit is dropped, a permit is returned to the pool. This is the direct analog of the JavaScript `p-limit` / hand-rolled semaphore above, and `std` has no equivalent.

```rust playground
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let sem = Arc::new(Semaphore::new(2)); // 2 concurrent permits
    let mut handles = Vec::new();
    for id in 1..=4 {
        let sem = Arc::clone(&sem);
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            println!("task {id} acquired permit");
            sleep(Duration::from_millis(10)).await;
            println!("task {id} releasing permit");
            // _permit dropped at end of scope -> permit returned
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
}
```

Real output (one possible interleaving):

```text
task 1 acquired permit
task 2 acquired permit
task 2 releasing permit
task 1 releasing permit
task 3 acquired permit
task 4 acquired permit
task 4 releasing permit
task 3 releasing permit
```

Notice that tasks 3 and 4 do not even start their work until 1 and 2 release their permits — exactly two run at a time.

---

## Key Differences

| Concept | TypeScript / JavaScript (Node) | Rust + Tokio |
| --- | --- | --- |
| Need for locks at all | No data races (single-threaded event loop); locks only *order* async ops | Real shared-memory concurrency across worker threads; locks enforce safety |
| Built-in mutex | None (hand-rolled or `async-mutex` / `p-limit` npm packages) | `std::sync::Mutex` and `tokio::sync::Mutex` in the standard toolset |
| Acquire semantics | `await` a promise chain | `.lock().await` (async) or `.lock().unwrap()` (blocking) |
| Holding across `await` | Always fine — same thread | Fine with `tokio` lock; a compile error with `std` lock (`!Send` guard) |
| Releasing the lock | Manual `release()` in a `finally` | Automatic — the guard's `Drop` releases it (RAII), no `finally` needed |
| Read/write split | Not built in | `tokio::sync::RwLock` (and `std::sync::RwLock`) |
| Concurrency cap | `p-limit` / hand-rolled | `tokio::sync::Semaphore` |
| Poisoning | N/A | `std` locks poison on panic (returns `Result`); `tokio` locks do not |

The deepest conceptual difference is **RAII release**: there is no `release()` to forget. The lock is tied to the lifetime of the guard value. When the guard goes out of scope, the lock is freed — which is precisely why "scope the guard" is the fix for the across-`await` problem.

> **Warning:** "Use `tokio::sync::Mutex` because I'm in async code" is a reflex worth resisting. The right reflex is: *do I `.await` while holding it?* If not, `std::sync::Mutex` is the leaner, faster, idiomatic choice. See [async vs sync](/11-async/13-async-vs-sync/) for the broader "when is async even the right tool" discussion.

---

## Common Pitfalls

### Pitfall 1: Holding a `std` guard across `.await`

This is the error nearly every TypeScript/JavaScript developer hits first. It looks innocent:

```rust
// does not compile (future cannot be sent between threads safely)
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let data = Arc::new(Mutex::new(0_u32));
    let d = Arc::clone(&data);
    let handle = tokio::spawn(async move {
        let mut guard = d.lock().unwrap();       // std::sync::MutexGuard
        sleep(Duration::from_millis(10)).await;  // held across .await — !Send
        *guard += 1;
    });
    handle.await.unwrap();
    println!("{}", *data.lock().unwrap());
}
```

The real `cargo build` error:

```text
error: future cannot be sent between threads safely
   --> src/bin/std_across_await.rs:9:18
    |
  9 |       let handle = tokio::spawn(async move {
    |  __________________^
 10 | |         let mut guard = d.lock().unwrap(); // std::sync::MutexGuard
 11 | |         sleep(Duration::from_millis(10)).await; // hold the guard across .await
 12 | |         *guard += 1;
 13 | |     });
    | |______^ future created by async block is not `Send`
    |
    = help: within `{async block@src/bin/std_across_await.rs:9:31: 9:41}`, the trait `Send` is not implemented for `std::sync::MutexGuard<'_, u32>`
note: future is not `Send` as this value is used across an await
   --> src/bin/std_across_await.rs:11:42
    |
 10 |         let mut guard = d.lock().unwrap();
    |             --------- has type `std::sync::MutexGuard<'_, u32>` which is not `Send`
 11 |         sleep(Duration::from_millis(10)).await;
    |                                          ^^^^^ await occurs here, with `mut guard` maybe used later
note: required by a bound in `tokio::spawn`
```

**Two valid fixes:**

1. Drop the guard before the `.await` by scoping it (shown earlier) — keeps the cheap `std` lock.
2. Switch to `tokio::sync::Mutex`, whose guard *is* `Send`, when you genuinely must stay locked across the `.await`:

```rust playground
// tokio::sync::Mutex guard is Send, so holding across .await compiles
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let data = Arc::new(Mutex::new(0_u32));
    let d = Arc::clone(&data);
    let handle = tokio::spawn(async move {
        let mut guard = d.lock().await;          // tokio MutexGuard IS Send
        sleep(Duration::from_millis(10)).await;  // OK to hold across .await
        *guard += 1;
    });
    handle.await.unwrap();
    println!("{}", *data.lock().await);
}
```

Real output: `1`.

### Pitfall 2: Locking the same lock twice in one task (self-deadlock)

Unlike Node, where there is no real lock to deadlock on, a Tokio `Mutex` is **not reentrant**. If a task holds the guard and then tries to lock the same mutex again, it waits forever — the permit it is waiting for is held by itself.

```rust playground
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

#[tokio::main]
async fn main() {
    let m = Mutex::new(0);
    let _g1 = m.lock().await;
    // Second lock on the SAME mutex while _g1 is alive -> blocks forever.
    // The timeout wrapper lets the demo terminate instead of hanging.
    let res = timeout(Duration::from_millis(50), m.lock()).await;
    match res {
        Ok(_) => println!("acquired again (unexpected)"),
        Err(_) => println!("timed out: second lock() never succeeded (self-deadlock)"),
    }
}
```

Real output:

```text
timed out: second lock() never succeeded (self-deadlock)
```

The fix is to restructure so the lock is acquired once, or release the first guard (`drop(_g1)`) before locking again.

### Pitfall 3: Trusting the compiler to catch *every* across-`await` hold

The `!Send` error only fires when the future must be `Send` — for example when you `tokio::spawn` it. On a current-thread runtime, or for a future that never crosses a thread boundary, holding a `std` guard across `.await` **compiles** but is still a latent deadlock risk and almost always a bug. Clippy's `await_holding_lock` lint catches it regardless:

```rust
use std::sync::Mutex;
use tokio::time::{sleep, Duration};

async fn bump(m: &Mutex<u32>) {
    let mut g = m.lock().unwrap();
    sleep(Duration::from_millis(1)).await; // holding std guard across await
    *g += 1;
}
```

Real `cargo clippy` warning:

```text
warning: this `MutexGuard` is held across an await point
 --> src/bin/clippy_hold.rs:5:9
  |
5 |     let mut g = m.lock().unwrap();
  |         ^^^^^
  |
  = help: consider using an async-aware `Mutex` type or ensuring the `MutexGuard` is dropped before calling `await`
note: these are all the await points this lock is held through
 --> src/bin/clippy_hold.rs:6:37
  |
6 |     sleep(Duration::from_millis(1)).await; // holding std guard across await
  |                                     ^^^^^
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#await_holding_lock
  = note: `#[warn(clippy::await_holding_lock)]` on by default
```

> **Tip:** Run `cargo clippy` in CI. The `await_holding_lock` lint is on by default and is your best defense against the latent (non-spawned) version of Pitfall 1.

### Pitfall 4: Reaching for a lock when an atomic or channel is simpler

A `Mutex<u64>` counter incremented under a lock works, but for a single integer an `AtomicU64` (no lock at all) or a `tokio::sync::mpsc` channel (no shared mutable state at all) is often clearer and faster. Locks are not the only tool — see [Channels](/11-async/08-channels/) for the "share by communicating" alternative.

---

## Best Practices

- **Default to `std::sync::Mutex`/`RwLock` for non-`await` critical sections.** Only reach for `tokio::sync` locks when you must hold the guard across an `.await`. This is Tokio's own recommendation.
- **Keep critical sections tiny.** Compute outside the lock; lock only to read or commit the result. A long critical section serializes your tasks and erases the benefit of async.
- **Use an explicit `{ }` scope (or `drop(guard)`)** to release a guard before an `.await` when using a `std` lock.
- **Prefer `RwLock` only when reads dominate and writes are rare.** For balanced or write-heavy access, a plain `Mutex` is simpler and frequently faster (an `RwLock` has more bookkeeping).
- **Use `Semaphore` for backpressure / concurrency limits**, e.g. capping simultaneous outbound HTTP requests or database connections.
- **For spawned tasks, prefer the `owned` variants** — `Mutex::lock_owned`, `Semaphore::acquire_owned` — to sidestep guard-lifetime issues with `'static` futures:

```rust playground
use std::sync::Arc;
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() {
    let sem = Arc::new(Semaphore::new(1));
    let sem2 = Arc::clone(&sem);
    let handle = tokio::spawn(async move {
        // acquire_owned consumes the Arc clone -> an owned permit with no borrow.
        let _permit = sem2.acquire_owned().await.unwrap();
        "did work"
    });
    println!("{}", handle.await.unwrap());
    println!("available permits = {}", sem.available_permits());
}
```

Real output:

```text
did work
available permits = 1
```

- **Run `cargo clippy`** to catch held-across-`await` guards that the type checker lets slip on current-thread runtimes.
- **Never hold two locks in different orders across tasks** — that is the classic lock-ordering deadlock, identical to the threaded-Rust case.

---

## Real-World Example

A small client that fetches user records from a slow "API", **caches** them behind an `RwLock` (many concurrent readers, occasional writers), and **throttles** outbound calls with a `Semaphore` so it never makes more than `MAX_INFLIGHT` requests at once. This is the production-flavored combination of all three primitives.

```rust playground
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{sleep, Duration};

/// Fetches user records, caches them, and rate-limits concurrent requests.
#[derive(Clone)]
struct UserClient {
    cache: Arc<RwLock<HashMap<u32, String>>>,
    limiter: Arc<Semaphore>,
}

const MAX_INFLIGHT: usize = 3;

impl UserClient {
    fn new() -> Self {
        UserClient {
            cache: Arc::new(RwLock::new(HashMap::new())),
            limiter: Arc::new(Semaphore::new(MAX_INFLIGHT)),
        }
    }

    async fn fetch_remote(id: u32) -> String {
        sleep(Duration::from_millis(20)).await; // simulate network latency
        format!("user#{id}")
    }

    async fn get(&self, id: u32) -> String {
        // Fast path: a read lock lets many tasks check the cache concurrently.
        if let Some(name) = self.cache.read().await.get(&id) {
            return name.clone();
        }

        // Slow path: throttle concurrent network calls with the semaphore.
        let _permit = self.limiter.acquire().await.unwrap();
        let name = Self::fetch_remote(id).await;

        // Take the write lock only briefly to insert the result.
        self.cache.write().await.insert(id, name.clone());
        name
    }
}

#[tokio::main]
async fn main() {
    let client = UserClient::new();

    // Ten concurrent lookups over four distinct ids.
    let mut handles = Vec::new();
    for i in 0..10 {
        let client = client.clone();
        let id = (i % 4) + 1;
        handles.push(tokio::spawn(async move { client.get(id).await }));
    }

    let mut results: Vec<String> = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    results.sort();
    results.dedup();
    println!("distinct users fetched: {:?}", results);
    println!("cache size: {}", client.cache.read().await.len());
}
```

Real output:

```text
distinct users fetched: ["user#1", "user#2", "user#3", "user#4"]
cache size: 4
```

Note how the locks are held across `.await` here (the read lock spans the `.get()`, the write lock spans the `.insert()`) — which is exactly why these are `tokio::sync` locks and not `std` ones. The `Semaphore` guarantees at most three `fetch_remote` calls overlap, regardless of how many of the ten tasks miss the cache.

> **Note:** A subtle real-world refinement (omitted for clarity) is the "double-check after acquiring the permit": between the cache miss and getting the permit, another task may have populated the cache. Production code often re-reads the cache after `acquire().await` to avoid duplicate fetches — an instance of the classic check-then-act race.

---

## Further Reading

- [Tokio `sync` module docs](https://docs.rs/tokio/latest/tokio/sync/index.html): the authoritative reference for `Mutex`, `RwLock`, `Semaphore`, and more.
- [`tokio::sync::Mutex` docs](https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html): includes Tokio's own guidance on when to prefer `std::sync::Mutex`.
- [`std::sync::Mutex` docs](https://doc.rust-lang.org/std/sync/struct.Mutex.html): poisoning, `lock`, `try_lock`.
- [Clippy `await_holding_lock` lint](https://rust-lang.github.io/rust-clippy/master/index.html#await_holding_lock): the lint behind Pitfall 3.
- [The Tokio Tutorial: Shared State](https://tokio.rs/tokio/tutorial/shared-state): `Arc<Mutex<T>>` in context.

Related pages in this guide:

- [Arc + Mutex pattern](/11-async/12-arc-mutex-pattern/): how `Arc` lets multiple tasks share one lock.
- [Channels](/11-async/08-channels/): "share by communicating" as an alternative to shared-state locks.
- [Spawning tasks](/11-async/09-spawning-tasks/): the `tokio::spawn` and `Send` requirements that drive the across-`await` rule.
- [select / join](/11-async/07-select-join/) — concurrent awaiting that often replaces manual locking.
- [async vs sync](/11-async/13-async-vs-sync/): when async (and therefore async locks) is the right tool at all.
- [Tokio Intro](/11-async/02-tokio-intro/) and [Tokio Setup](/11-async/03-tokio-setup/) — the runtime these primitives need.
- [Ownership: reference counting](/05-ownership/07-reference-counting/): the `Arc` mechanics underlying shared locks.
- [Smart Pointers section](/10-smart-pointers/): `Arc`, `Rc`, and interior mutability in depth.
- Next up after async: [Modules & Packages](/12-modules-packages/).

---

## Exercises

### Exercise 1

**Difficulty:** Beginner

**Objective:** Use a `std::sync::Mutex` correctly inside spawned async tasks by keeping the lock out of any `.await`.

**Instructions:** Create an `Arc<Mutex<u64>>` total. Spawn four tasks; each first `sleep`s (an `.await`), then adds its own number (1 through 4) to the total. Make sure the program compiles (no across-`await` hold) and prints `total = 10`.

<details>
<summary>Solution</summary>

```rust playground
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let total = Arc::new(Mutex::new(0_u64));
    let mut handles = Vec::new();
    for i in 1..=4 {
        let total = Arc::clone(&total);
        handles.push(tokio::spawn(async move {
            sleep(Duration::from_millis(5)).await; // await BEFORE locking
            let mut g = total.lock().unwrap();      // lock, no await while held
            *g += i;                                 // guard dropped at scope end
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    println!("total = {}", *total.lock().unwrap());
}
```

Real output:

```text
total = 10
```

The `.await` happens *before* the lock is taken, so the `std` `MutexGuard` never lives across an await point — the future stays `Send` and `tokio::spawn` accepts it.

</details>

### Exercise 2

**Difficulty:** Intermediate

**Objective:** Use a `tokio::sync::RwLock` to coordinate concurrent writers and readers over a shared counter.

**Instructions:** Create an `Arc<RwLock<u64>>` hit counter. Spawn five writer tasks that each take the write lock and increment by 1. After they all finish, spawn three reader tasks that take the read lock and return the value; assert each reads `5`. Print the final value.

<details>
<summary>Solution</summary>

```rust playground
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    let hits = Arc::new(RwLock::new(0_u64));

    // Five concurrent writers each increment once.
    let mut writers = Vec::new();
    for _ in 0..5 {
        let hits = Arc::clone(&hits);
        writers.push(tokio::spawn(async move {
            let mut w = hits.write().await;
            *w += 1;
        }));
    }
    for w in writers {
        w.await.unwrap();
    }

    // Three concurrent readers all share the read lock.
    let mut readers = Vec::new();
    for _ in 0..3 {
        let hits = Arc::clone(&hits);
        readers.push(tokio::spawn(async move { *hits.read().await }));
    }
    for r in readers {
        assert_eq!(r.await.unwrap(), 5);
    }
    println!("final hits = {}", *hits.read().await);
}
```

Real output:

```text
final hits = 5
```

The writers serialize (each needs exclusive `write()` access), so all five increments land. The readers run *after* the writers and can share the read lock, all observing the final value.

</details>

### Exercise 3

**Difficulty:** Advanced

**Objective:** Use a `tokio::sync::Semaphore` to cap concurrency, and *prove* the cap holds by tracking the maximum number of simultaneously running tasks.

**Instructions:** With a semaphore of limit 2, spawn six tasks. Each acquires an owned permit (`acquire_owned`), increments a shared `AtomicUsize` "current" counter, updates an `AtomicUsize` "max seen" with `fetch_max`, sleeps briefly, then decrements "current". After all tasks finish, print the maximum concurrency observed and assert it never exceeded 2.

<details>
<summary>Solution</summary>

```rust playground
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    const LIMIT: usize = 2;
    let sem = Arc::new(Semaphore::new(LIMIT));
    let current = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..6 {
        let sem = Arc::clone(&sem);
        let current = Arc::clone(&current);
        let max_seen = Arc::clone(&max_seen);
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire_owned().await.unwrap();
            let now = current.fetch_add(1, Ordering::SeqCst) + 1;
            max_seen.fetch_max(now, Ordering::SeqCst);
            sleep(Duration::from_millis(10)).await;
            current.fetch_sub(1, Ordering::SeqCst);
            // _permit dropped here -> a permit is returned
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    println!("max concurrent = {}", max_seen.load(Ordering::SeqCst));
    println!(
        "never exceeded limit: {}",
        max_seen.load(Ordering::SeqCst) <= LIMIT
    );
}
```

Real output:

```text
max concurrent = 2
never exceeded limit: true
```

`acquire_owned` returns a permit that owns its slice of the semaphore with no borrow lifetime, so it can live inside a `'static` spawned task. The `AtomicUsize` counters (no lock needed for a single integer) record that at most two tasks ever held a permit at once.

</details>
