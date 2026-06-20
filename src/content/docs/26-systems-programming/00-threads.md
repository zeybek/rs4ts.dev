---
title: "Native Threads with `std::thread`"
description: "Rust OS threads are lightweight and race-free by construction, unlike Node's heavyweight Workers. Spawn, join, move data in, and borrow with thread::scope."
---

In Node.js, "a thread" is an exotic, heavyweight thing: a `Worker` with its own V8 isolate, its own heap, and a serialization boundary you must cross with `postMessage`. In Rust, an OS thread is a first-class, lightweight tool, and the compiler statically prevents the data races that make threads terrifying in C++. This page covers spawning threads, joining them, moving data into them, and the modern `std::thread::scope` API that lets threads safely *borrow* from their parent.

---

## Quick Overview

`std::thread` gives you real, OS-backed threads that run on multiple cores simultaneously: true parallelism, not the single-threaded concurrency of Node's event loop. You `spawn` a thread with a closure, get back a `JoinHandle`, and call `join()` to wait for its result. The headline feature for a TypeScript developer: Rust's ownership system makes threads **memory-safe by construction**. Code that would race in JavaScript-with-SharedArrayBuffer (or segfault in C++) simply does not compile.

> **Note:** This page is about raw OS threads. For CPU-bound *data parallelism* you will usually reach for the higher-level [rayon thread pool](/26-systems-programming/01-thread-pools/) and [parallel iterators](/26-systems-programming/02-parallel-iterators/) instead of spawning threads by hand. To pass messages between threads, see [channels](/26-systems-programming/03-channels/). To share mutable counters without locks, see [atomic operations](/26-systems-programming/04-atomic-operations/). For async tasks (which are *not* threads), see [Section 11: async/concurrency](/11-async/10-concurrency/).

---

## TypeScript/JavaScript Example

JavaScript is single-threaded. To get real parallelism (to use a second CPU core) you must spin up a **Worker Thread**, which is a separate V8 isolate with its own memory. You cannot share ordinary objects with it; you communicate by *copying* messages across a serialization boundary (structured clone), or by using a `SharedArrayBuffer` for a narrow slice of raw bytes.

```typescript
// main.ts — Node v22
import { Worker } from "node:worker_threads";

// Each worker is a heavyweight thread with its OWN heap. We send it a number,
// it sends back the sum 1..=n. The data is COPIED across the boundary.
function sumInWorker(n: number): Promise<number> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(
      `
      const { parentPort, workerData } = require('node:worker_threads');
      let total = 0;
      for (let i = 1; i <= workerData; i++) total += i;
      parentPort.postMessage(total);
      `,
      { eval: true, workerData: n },
    );
    worker.on("message", resolve);
    worker.on("error", reject);
    worker.on("exit", (code) => {
      if (code !== 0) reject(new Error(`worker exited with code ${code}`));
    });
  });
}

async function main() {
  // Run several workers "in parallel" on real cores.
  const results = await Promise.all([
    sumInWorker(1000),
    sumInWorker(2000),
    sumInWorker(3000),
  ]);
  console.log(results); // [ 500500, 2001000, 4501500 ]
}

main();
```

Key facts about the JavaScript model:

- A `Worker` is **expensive**: it boots a whole V8 isolate. You pool them, you do not create thousands.
- Data is **not shared**. `workerData` and `postMessage` payloads are deep-copied (structured clone). The closure body cannot capture variables from `main` — note we had to inline the worker source as a string.
- There is **no compile-time protection** against races on a `SharedArrayBuffer`; you reach for `Atomics` and hope you got it right.

---

## Rust Equivalent

In Rust, a thread is just a function (closure) you hand to `thread::spawn`. It runs on a real OS thread, in parallel, on the *same* heap as the rest of your program, and the borrow checker guarantees you do not corrupt that shared heap.

```rust playground
use std::thread;

fn main() {
    // Spawn a thread. spawn() returns a JoinHandle<T> immediately; the closure
    // runs concurrently on another core. T is the closure's return type.
    let handle = thread::spawn(|| {
        let mut total = 0u64;
        for i in 1..=1_000 {
            total += i;
        }
        total // the closure's return value becomes the thread's result
    });

    // The main thread keeps running while the worker computes.
    println!("main thread keeps running");

    // join() blocks until the worker finishes and hands back its return value,
    // wrapped in a Result (Err if the thread panicked).
    let sum = handle.join().expect("worker thread panicked");
    println!("worker computed sum = {sum}");
}
```

Running it:

```text
main thread keeps running
worker computed sum = 500500
```

No isolate to boot, no serialization, no message channel for a simple result: the value flows straight back through `join()`. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically, and everything here is in the standard library (no `cargo add` needed).

---

## Detailed Explanation

### `thread::spawn` and `JoinHandle`

```rust playground
use std::thread;

fn main() {
    let handle = thread::spawn(|| 21 * 2);
    let answer = handle.join().unwrap();
    println!("{answer}"); // 42
}
```

- `thread::spawn(f)` takes a closure `f` and starts a new OS thread that runs it. It returns **immediately**; the thread runs concurrently.
- The return type is `JoinHandle<T>`, where `T` is whatever the closure returns. Here `T = i32`.
- `handle.join()` blocks the calling thread until the spawned thread finishes. It returns `thread::Result<T>`: an `Ok(value)` with the closure's return value, or an `Err` if the thread **panicked**.

Compare to JavaScript: a `JoinHandle<T>` plays a role similar to a `Promise<T>`, but it is **not** lazy and **not** async — it is a handle to a thread that is *already running on another core right now*. And `join()` is a *blocking* wait, not an `await` that yields to an event loop.

### Move closures: `move`

A spawned thread can outlive the function that created it, so by default Rust will not let the closure borrow local variables: those locals might be gone by the time the thread reads them. The `move` keyword transfers **ownership** of captured variables into the closure:

```rust playground
use std::thread;

fn main() {
    let data = vec![10, 20, 30, 40];

    // `move` transfers ownership of `data` INTO the thread's closure.
    let handle = thread::spawn(move || {
        let sum: i32 = data.iter().sum();
        println!("worker sees data with sum {sum}");
        sum
    });

    // `data` is no longer usable here — it was moved into the thread.
    let result = handle.join().unwrap();
    println!("main got {result}");
}
```

Output:

```text
worker sees data with sum 100
main got 100
```

This is the big contrast with JavaScript's `Worker`: there, `data` would be *deep-copied* across the boundary. In Rust, `move` transfers the *same* heap allocation: zero copy, zero serialization. After the move, the compiler statically forbids `main` from touching `data`, so there is no race: exactly one owner at a time.

### Scoped threads: `thread::scope` (borrow instead of move)

What if you do not want to *give away* your data, you just want a few threads to read it (or write to disjoint parts) and then get control back? Moving works for one thread, but moving into many is impossible (you only have one value to give). Historically you wrapped everything in `Arc` and cloned the pointer. Since Rust 1.63, `std::thread::scope` offers a cleaner answer: **scoped threads can borrow non-`'static` data** because the scope guarantees they *all finish* before it returns.

```rust playground
use std::thread;

fn main() {
    let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8];

    // thread::scope guarantees all spawned threads finish before it returns,
    // which lets them BORROW `numbers` instead of taking ownership.
    let total = thread::scope(|s| {
        let (left, right) = numbers.split_at(numbers.len() / 2);

        // Each handle borrows a slice of the SAME vector — no clone, no Arc.
        let h_left = s.spawn(|| left.iter().sum::<i32>());
        let h_right = s.spawn(|| right.iter().sum::<i32>());

        h_left.join().unwrap() + h_right.join().unwrap()
    });

    // `numbers` is still fully owned and usable here.
    println!("sum of {numbers:?} = {total}");
}
```

Output:

```text
sum of [1, 2, 3, 4, 5, 6, 7, 8] = 36
```

The closures here capture `left` and `right` by *shared reference*: no `move`, no `Arc`, no clone. The borrow checker accepts this because `scope` will not return until every thread it spawned has been joined, so the borrows cannot outlive `numbers`. This is the idiomatic way to fan out work over data you own on the stack.

> **Tip:** Reach for `thread::scope` first when you want a fixed set of threads to chew on borrowed data and you want to wait for all of them. It eliminates an entire class of `Arc`/clone boilerplate. Use a plain `thread::spawn` + `move` when the thread must **outlive** the current function or be detached.

### `available_parallelism` and collecting handles

To size your work to the machine, ask how many cores you can actually use, then spawn and join a batch:

```rust playground
use std::thread;

fn main() {
    let n = thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    println!("this machine reports {n} usable cores");

    // Spawn one thread per id, then join them all and collect the results.
    let handles: Vec<_> = (0..4)
        .map(|id| thread::spawn(move || id * id))
        .collect();

    let squares: Vec<i32> = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect();

    println!("squares = {squares:?}");
}
```

Output on an 8-core machine:

```text
this machine reports 8 usable cores
squares = [0, 1, 4, 9]
```

`available_parallelism()` is the rough equivalent of Node's `os.availableParallelism()`. Note the `move` on the inner closure: each thread captures its own `id` by value, so there is no shared mutable state to race over.

### The `Builder` API: names and stack size

`thread::spawn` uses sensible defaults. For control over the thread name (shown in panics and debuggers) and stack size, use `thread::Builder`:

```rust playground
use std::thread;

fn main() {
    let handle = thread::Builder::new()
        .name("crunch-worker".to_string())
        .stack_size(4 * 1024 * 1024) // 4 MiB stack
        .spawn(|| {
            let me = thread::current();
            format!("hello from {:?}", me.name().unwrap_or("<unnamed>"))
        })
        .expect("failed to spawn thread");

    println!("{}", handle.join().unwrap());
}
```

Output:

```text
hello from "crunch-worker"
```

Unlike bare `spawn`, `Builder::spawn` returns an `io::Result<JoinHandle<T>>`: spawning an OS thread can genuinely fail (e.g., the OS refuses more threads), and the `Builder` API surfaces that instead of aborting.

---

## Key Differences

| Aspect | Node.js `Worker` | Rust `std::thread` |
| --- | --- | --- |
| Weight | Heavy (full V8 isolate) | Light (one OS thread) |
| Memory | Separate heap per worker | Shared heap, compiler-checked |
| Passing data in | Copied (structured clone) | Moved (`move`) or borrowed (`scope`), zero-copy |
| Getting a result out | `postMessage` + event listener | Return value via `handle.join()` |
| Capturing locals | Not possible (string source / `workerData`) | Closure captures directly |
| Race protection | None at compile time (`Atomics` by hand) | `Send`/`Sync` enforced by the compiler |
| True parallelism | Yes | Yes |
| Cancellation | `worker.terminate()` | Cooperative (no forced kill) |

### Why threads are *safe* in Rust: `Send` and `Sync`

The compiler enforces two auto-traits at the thread boundary:

- **`Send`**: a type is safe to *transfer* ownership of to another thread. Most types are `Send`; notable exceptions are `Rc<T>` (non-atomic reference count) and raw pointers.
- **`Sync`**: a type is safe to *share by reference* (`&T`) across threads. `T` is `Sync` iff `&T` is `Send`.

`thread::spawn` requires the closure (and everything it captures) to be `Send + 'static`. `thread::scope`'s `spawn` relaxes the `'static` requirement to a scoped lifetime but still requires `Send`/`Sync`. This is *the* mechanism that turns "did I introduce a data race?" from a runtime gamble into a compile error. In JavaScript there is no equivalent: sharing a `SharedArrayBuffer` incorrectly is simply a bug you find in production.

### Threads are not async tasks

A common point of confusion for Node developers: Rust threads are **not** the same as async `tokio::spawn` tasks. A thread is a real OS thread that the kernel schedules and that can block. An async task is a lightweight state machine multiplexed onto a small pool of threads by a runtime, and it must never block. Use threads for CPU-bound work and for blocking calls; use async for high-concurrency I/O. See [async vs sync](/11-async/13-async-vs-sync/).

---

## Common Pitfalls

### Pitfall 1: Borrowing a local without `move`

Coming from JavaScript, you expect the closure to just "see" the surrounding variable. With `thread::spawn`, it cannot, because the thread might outlive the function:

```rust
use std::thread;

fn main() {
    let data = vec![1, 2, 3];

    // does not compile (error[E0373]: closure may outlive the current function)
    let handle = thread::spawn(|| {
        println!("{:?}", data); // borrows `data`
    });

    handle.join().unwrap();
}
```

The real compiler error:

```text
error[E0373]: closure may outlive the current function, but it borrows `data`, which is owned by the current function
 --> src/bin/err_borrow.rs:6:32
  |
6 |     let handle = thread::spawn(|| {
  |                                ^^ may outlive borrowed value `data`
7 |         println!("{:?}", data); // borrows `data`
  |                          ---- `data` is borrowed here
  |
help: to force the closure to take ownership of `data` (and any other referenced variables), use the `move` keyword
  |
6 |     let handle = thread::spawn(move || {
  |                                ++++
```

**Fix:** add `move` (transfer ownership), or — if you only need to borrow and will join before the function returns — use `thread::scope` so the borrow is allowed.

### Pitfall 2: Using a value after moving it into a thread

`move` is permanent. After the move, the original binding is gone:

```rust
use std::thread;

fn main() {
    let data = vec![1, 2, 3];

    let handle = thread::spawn(move || {
        println!("{:?}", data);
    });

    // does not compile (error[E0382]: borrow of moved value: `data`)
    println!("{:?}", data);
    handle.join().unwrap();
}
```

The real error:

```text
error[E0382]: borrow of moved value: `data`
  --> src/bin/err_use_after_move.rs:10:22
   |
 4 |     let data = vec![1, 2, 3];
   |         ---- move occurs because `data` has type `Vec<i32>`, which does not implement the `Copy` trait
 6 |     let handle = thread::spawn(move || {
   |                                ------- value moved into closure here
 7 |         println!("{:?}", data);
   |                          ---- variable moved due to use in closure
...
10 |     println!("{:?}", data);
   |                      ^^^^ value borrowed here after move
```

**Fix:** if both the thread and `main` genuinely need the data, share it with `Arc` (read-only) or `Arc<Mutex<T>>` (mutable), and `clone` the `Arc` for the thread. If `main` only needs the *result*, retrieve it via `join()` instead of touching the moved value. Unlike JavaScript's copy-on-`postMessage`, Rust forces you to be explicit about which strategy you want.

### Pitfall 3: Sharing `Rc<T>` across threads

`Rc<T>` is the cheap, *non-atomic* reference-counted pointer. Its refcount updates are not thread-safe, so `Rc` is `!Send` and the compiler rejects it at the boundary:

```rust
use std::rc::Rc;
use std::thread;

fn main() {
    let shared = Rc::new(42);

    // does not compile (error[E0277]: `Rc<i32>` cannot be sent between threads safely)
    let handle = thread::spawn(move || {
        println!("{}", shared);
    });

    handle.join().unwrap();
}
```

The real error (abbreviated):

```text
error[E0277]: `Rc<i32>` cannot be sent between threads safely
   --> src/bin/err_rc.rs:7:32
    |
  7 |     let handle = thread::spawn(move || {
    |                  ------------- ^------ ... `Rc<i32>` cannot be sent between threads safely
    |
    = help: within `{closure@...}`, the trait `Send` is not implemented for `Rc<i32>`
note: required by a bound in `spawn`
```

**Fix:** use `Arc` (Atomic Reference Counted), which *is* `Send + Sync`:

```rust playground
use std::sync::Arc;
use std::thread;

fn main() {
    let shared = Arc::new(vec![1, 2, 3]);

    let handles: Vec<_> = (0..3)
        .map(|i| {
            let shared = Arc::clone(&shared); // bump the atomic refcount, share the same data
            thread::spawn(move || {
                println!("thread {i} sees {:?}", shared);
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}
```

Output (the line order varies run to run because the threads are genuinely concurrent):

```text
thread 0 sees [1, 2, 3]
thread 1 sees [1, 2, 3]
thread 2 sees [1, 2, 3]
```

See [reference counting](/05-ownership/07-reference-counting/) for `Rc` vs `Arc` in depth.

### Pitfall 4: Forgetting to `join` — the process exits and kills the thread

When `main` returns, the whole process exits, taking any still-running threads with it. There is no "wait for background threads" at exit:

```rust playground
use std::thread;
use std::time::Duration;

fn main() {
    thread::spawn(|| {
        thread::sleep(Duration::from_millis(500));
        println!("worker: this may NEVER print");
    });
    // No join() — main returns, the whole process exits, killing the worker.
    println!("main: exiting immediately");
}
```

Output:

```text
main: exiting immediately
```

The worker's `println!` never runs: the process was already gone. **Fix:** hold the `JoinHandle` and `join()` it before `main` ends (or use `thread::scope`, which joins for you).

### Pitfall 5: Expecting a panic in one thread to crash the program

A panic *unwinds only its own thread*. The default panic behavior is `unwind`, so a panicking worker does not take down `main`; instead, `join()` returns `Err`:

```rust playground
use std::thread;

fn main() {
    let handle = thread::spawn(|| {
        panic!("worker exploded");
    });

    match handle.join() {
        Ok(()) => println!("worker finished cleanly"),
        Err(_) => println!("main: detected that the worker panicked, carrying on"),
    }

    println!("main is still alive");
}
```

Output:

```text
thread '<unnamed>' panicked at src/bin/panic_thread.rs:5:9:
worker exploded
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
main: detected that the worker panicked, carrying on
main is still alive
```

The panic message is printed to stderr by the default hook, but the program keeps running. Always check the `Result` from `join()` if a thread might panic. (If your crate sets `panic = "abort"`, the whole process aborts instead; there is no `Err` to observe.)

---

## Best Practices

- **Prefer `thread::scope` for borrowing.** If you have a fixed set of workers that read or write disjoint parts of stack-owned data and you will wait for all of them, `scope` avoids `Arc`/clone ceremony and keeps borrows checked.
- **Prefer rayon for data parallelism.** Spawning one thread per item is almost always wrong. For "apply this to every element of a big collection," use [`par_iter()`](/26-systems-programming/02-parallel-iterators/); for divide-and-conquer, use [rayon's pool and `join`](/26-systems-programming/01-thread-pools/). They handle work-stealing and core sizing for you.
- **Use `move` deliberately.** Reach for `move` when a thread must own its captures or outlive the spawning function. Do not sprinkle it reflexively — if `scope` lets you borrow, that is clearer.
- **Share with the right pointer.** `Arc<T>` for read-only sharing, `Arc<Mutex<T>>` (or `Arc<RwLock<T>>`) for shared mutation, atomics for simple counters/flags. Never reach for `unsafe` to "just share a `&mut`."
- **Always join (or scope).** Detached threads that you never join are a resource and correctness hazard; at minimum keep the handle and join on shutdown.
- **Handle panics at the boundary.** Inspect `join()`'s `Result` for any thread that can panic, and convert it into a clean error or a controlled shutdown.
- **Match thread count to cores.** Use `thread::available_parallelism()` to size a pool; do not spawn thousands of OS threads for CPU-bound work — they will thrash.

---

## Real-World Example

A production-flavored task: compute a content hash (checksum) for several files concurrently and collect the results into a shared map. This pattern — fan out over inputs, each worker computes independently, results aggregate under a lock — is the bread and butter of native threading. It uses `thread::scope` to *borrow* the inputs and the result map (no `Arc` needed) and a `Mutex` to serialize the inserts.

```rust playground
use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;

/// A tiny FNV-1a hash so the example needs no external crate.
/// In real code you would use a crate like `sha2` or `blake3`.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn main() {
    // Simulated "files": (name, contents). In production these would be paths
    // you read with std::fs — see ./file-system.md.
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("config.toml", b"[server]\nport = 8080\n".to_vec()),
        ("index.html", b"<!doctype html><h1>hi</h1>".to_vec()),
        ("data.csv", b"id,name\n1,alice\n2,bob\n".to_vec()),
        ("notes.md", b"# TODO\n- ship it\n".to_vec()),
    ];

    // Shared map guarded by a Mutex; each thread inserts its own result.
    let checksums: Mutex<HashMap<&str, u64>> = Mutex::new(HashMap::new());

    thread::scope(|s| {
        for (name, contents) in &files {
            // Borrow `name`, `contents`, and `checksums`. The scope guarantees
            // these threads end before `files`/`checksums` are dropped, so no Arc.
            s.spawn(|| {
                let digest = fnv1a(contents);
                checksums.lock().unwrap().insert(name, digest);
            });
        }
    });

    // Back on the main thread, the scope has joined every worker.
    let map = checksums.into_inner().unwrap();
    let mut sorted: Vec<_> = map.into_iter().collect();
    sorted.sort_by_key(|(name, _)| *name);
    for (name, digest) in sorted {
        println!("{name:<12} {digest:016x}");
    }
}
```

Output:

```text
config.toml  5f4a2791ebf9f924
data.csv     a976785c72644ad1
index.html   3df08d3c2aac493f
notes.md     5e8adba7f295886a
```

Notice what the borrow checker did for us: the worker closures hold a `&Mutex<HashMap<..>>` *and* references into `files`, with no `Arc::clone` and no lifetime annotations. The `scope` is the proof that none of those borrows escape. The equivalent in Node would require a worker pool, copying each file's bytes across the `postMessage` boundary, and reassembling the results from messages: far more moving parts.

> **Note:** A `Mutex` serializes the *inserts*, not the *hashing*. The expensive `fnv1a` work runs fully in parallel; the lock is held only for the brief `insert`. Keep critical sections small. For a lock-free counter instead of a map, atomics are a better fit; see [atomic operations](/26-systems-programming/04-atomic-operations/).

---

## Further Reading

- [`std::thread` module documentation](https://doc.rust-lang.org/std/thread/index.html) — the canonical reference for `spawn`, `Builder`, and `current`.
- [`std::thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html) — scoped threads and the borrowing guarantees.
- [`std::thread::available_parallelism`](https://doc.rust-lang.org/std/thread/fn.available_parallelism.html) — sizing work to the machine.
- [The Rust Book, Ch. 16 "Fearless Concurrency"](https://doc.rust-lang.org/book/ch16-00-concurrency.html) — `Send`/`Sync`, `Arc<Mutex<T>>`, and threads end to end.
- Sibling pages in this section: [thread pools (rayon)](/26-systems-programming/01-thread-pools/), [parallel iterators](/26-systems-programming/02-parallel-iterators/), [channels](/26-systems-programming/03-channels/), [atomic operations](/26-systems-programming/04-atomic-operations/), [memory ordering](/26-systems-programming/05-memory-ordering/).
- Related: [reference counting (`Rc` vs `Arc`)](/05-ownership/07-reference-counting/), [ownership rules](/05-ownership/01-ownership-rules/), [async concurrency](/11-async/10-concurrency/), and [the section overview](/26-systems-programming/).
- For thread-safety implications of shared secrets and locks under contention, see [Section 27: Security](/27-security/).

---

## Exercises

### Exercise 1: Parallel sum over chunks

**Difficulty:** Beginner

**Objective:** Use `thread::scope` to split a slice into chunks and sum each chunk on its own thread.

**Instructions:** Write `fn parallel_sum(data: &[u64], chunks: usize) -> u64` that divides `data` into roughly `chunks` contiguous pieces, spawns one scoped thread per piece to sum it, and returns the grand total. Verify it against the closed-form sum of `1..=1_000_000`. You should not need `Arc` or `clone`.

<details>
<summary>Solution</summary>

```rust playground
use std::thread;

fn parallel_sum(data: &[u64], chunks: usize) -> u64 {
    // Round up so we never produce more than `chunks` pieces.
    let chunk_size = data.len().div_ceil(chunks.max(1));
    thread::scope(|s| {
        let handles: Vec<_> = data
            .chunks(chunk_size.max(1))
            .map(|chunk| s.spawn(move || chunk.iter().sum::<u64>()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).sum()
    })
}

fn main() {
    let data: Vec<u64> = (1..=1_000_000).collect();
    let total = parallel_sum(&data, 8);
    let expected = 1_000_000u64 * 1_000_001 / 2;
    println!("parallel sum = {total}, expected = {expected}");
    assert_eq!(total, expected);
}
```

Output:

```text
parallel sum = 500000500000, expected = 500000500000
```

Each thread borrows its `chunk` (a `&[u64]`) directly from `data`; `scope` guarantees they finish before `parallel_sum` returns. `div_ceil` (stable since Rust 1.73) rounds the chunk size up so the last chunk is not orphaned.

</details>

### Exercise 2: A hand-rolled worker pool over a shared queue

**Difficulty:** Intermediate

**Objective:** Build a fixed pool of worker threads that pull jobs from a shared queue and push results to a shared vector, using `Arc<Mutex<...>>`.

**Instructions:** Start with `jobs: Vec<u32>` of `1..=10`. Spawn 4 worker threads. Each worker loops: lock the queue, `pop()` one job (release the lock immediately), compute `n * n`, then lock the results vector and push `(n, n*n)`. Stop when the queue is empty. Join all workers, sort the results, and print them. (This is exactly the kind of boilerplate that rayon eliminates — see [thread pools](/26-systems-programming/01-thread-pools/) — but doing it by hand once builds intuition.)

<details>
<summary>Solution</summary>

```rust playground
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    let jobs: Vec<u32> = (1..=10).collect();
    let queue = Arc::new(Mutex::new(jobs));
    let results = Arc::new(Mutex::new(Vec::<(u32, u32)>::new()));

    let mut handles = Vec::new();
    for _worker in 0..4 {
        let queue = Arc::clone(&queue);
        let results = Arc::clone(&results);
        handles.push(thread::spawn(move || loop {
            // Pop one job UNDER the lock, then release it before working,
            // so workers do not serialize on the compute step.
            let job = queue.lock().unwrap().pop();
            match job {
                Some(n) => {
                    let squared = n * n;
                    results.lock().unwrap().push((n, squared));
                }
                None => break, // queue drained
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Sole owner now that all workers are joined; unwrap the Arc and the Mutex.
    let mut out = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
    out.sort();
    println!("{out:?}");
}
```

Output:

```text
[(1, 1), (2, 4), (3, 9), (4, 16), (5, 25), (6, 36), (7, 49), (8, 64), (9, 81), (10, 100)]
```

The key discipline is the size of the critical sections: each worker holds the queue lock only long enough to `pop`, and the results lock only long enough to `push`. The `n * n` work happens with no locks held, so it parallelizes.

</details>

### Exercise 3: Per-row maxima via disjoint `&mut` borrows

**Difficulty:** Advanced

**Objective:** Use `thread::scope` to write results into a pre-sized output `Vec` *in parallel* by handing each thread a disjoint `&mut` slot — no `Mutex`, no atomics.

**Instructions:** Write `fn row_maxima(matrix: &[Vec<i32>]) -> Vec<i32>` that returns, for each row, the maximum element. Pre-allocate the output `Vec`, then use `iter_mut().zip(...)` to pair each output slot with its input row and hand each `(&mut i32, &Vec<i32>)` pair to its own scoped thread. The trick: because each thread gets a *disjoint* mutable reference, the borrow checker allows concurrent writes with no synchronization at all.

<details>
<summary>Solution</summary>

```rust playground
use std::thread;

/// Compute per-row maxima of a matrix in parallel using scoped threads.
fn row_maxima(matrix: &[Vec<i32>]) -> Vec<i32> {
    let mut maxima = vec![i32::MIN; matrix.len()];

    thread::scope(|s| {
        // Pair each output slot with its input row, then hand each pair to its
        // own thread. `iter_mut` yields DISJOINT &mut, so no lock is needed.
        for (out, row) in maxima.iter_mut().zip(matrix.iter()) {
            s.spawn(move || {
                *out = row.iter().copied().max().unwrap_or(i32::MIN);
            });
        }
    });

    maxima
}

fn main() {
    let matrix = vec![
        vec![3, 7, 2],
        vec![9, 1, 4],
        vec![5, 5, 8],
    ];
    println!("{:?}", row_maxima(&matrix)); // [7, 9, 8]
}
```

Output:

```text
[7, 9, 8]
```

This is the payoff of Rust's aliasing rules: `iter_mut()` produces non-overlapping `&mut i32` handles, so the compiler *knows* the threads cannot conflict and lets them write concurrently with zero runtime synchronization. There is no equivalent guarantee in JavaScript — writing into a `SharedArrayBuffer` from multiple workers is unchecked and easy to get wrong. (In practice, for this kind of slice-parallel write you would reach for rayon's `par_iter_mut()`; see [parallel iterators](/26-systems-programming/02-parallel-iterators/).)

</details>
