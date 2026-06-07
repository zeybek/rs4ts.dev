---
title: "Thread Pools with Rayon"
description: "Rayon gives Rust a global, work-stealing thread pool and a join primitive, so CPU work parallelizes without the worker_threads boilerplate Node needs."
---

Spawning a fresh OS thread for every small unit of work is wasteful: thread creation has real cost, and a thousand threads fighting over eight cores thrash more than they compute. A **thread pool** keeps a fixed set of worker threads alive and feeds them tasks. In Rust the de-facto pool is **[Rayon](https://docs.rs/rayon)**, which gives you a ready-made global pool, a `join` primitive for divide-and-conquer parallelism, and builders for dedicated custom pools.

---

## Quick Overview

A thread pool reuses a small number of long-lived worker threads to run many short tasks, avoiding the per-task cost of `std::thread::spawn`. Rayon provides this out of the box with a **work-stealing** scheduler: idle workers steal tasks from busy ones, so the cores stay saturated without you hand-balancing the load. For a TypeScript/JavaScript developer this is the closest analogue to a `worker_threads` pool (like [Piscina](https://github.com/piscinajs/piscina)). But Rayon's tasks are plain Rust closures sharing memory safely, not separate Workers exchanging serialized messages.

> **Note:** This file is about the **pool machinery**: the global pool, `rayon::join`, and building custom pools. The high-level `par_iter()` API that runs on top of these pools has its own file: see [Parallel Iterators with Rayon](/26-systems-programming/02-parallel-iterators/).

---

## TypeScript/JavaScript Example

JavaScript runs your code on a single thread. To use multiple cores you reach for `worker_threads`, and because spawning a Worker is expensive, real apps pool them. A pool maintained by hand looks like this:

```typescript
// Node.js v22 — a hand-rolled fixed worker pool (the shape Piscina automates)
import { Worker } from "node:worker_threads";
import { availableParallelism } from "node:os";

interface Task<T> {
  payload: unknown;
  resolve: (value: T) => void;
  reject: (err: Error) => void;
}

class WorkerPool {
  private idle: Worker[] = [];
  private queue: Task<number>[] = [];

  constructor(size: number, private readonly script: string) {
    for (let i = 0; i < size; i++) {
      this.idle.push(this.makeWorker());
    }
  }

  private makeWorker(): Worker {
    const worker = new Worker(this.script);
    worker.on("message", (result: number) => {
      const task = (worker as any).currentTask as Task<number>;
      task.resolve(result);
      this.idle.push(worker); // return to the pool
      this.drain();
    });
    return worker;
  }

  private drain(): void {
    if (this.queue.length === 0 || this.idle.length === 0) return;
    const worker = this.idle.pop()!;
    const task = this.queue.shift()!;
    (worker as any).currentTask = task;
    worker.postMessage(task.payload); // payload is *copied/serialized*
  }

  run(payload: unknown): Promise<number> {
    return new Promise((resolve, reject) => {
      this.queue.push({ payload, resolve, reject });
      this.drain();
    });
  }
}

const pool = new WorkerPool(availableParallelism(), "./score-worker.js");
const scores = await Promise.all(
  documents.map((doc) => pool.run(doc)),
);
console.log("total:", scores.reduce((a, b) => a + b, 0));
```

**Key points:**

- You manage the worker count, the idle list, and the queue yourself (or pull in Piscina to do it).
- Every `postMessage` **copies** the payload (structured clone) across the thread boundary — workers do not share your heap.
- Sharing memory at all requires `SharedArrayBuffer` + `Atomics`, which only works for raw numeric buffers, not arbitrary objects.

---

## Rust Equivalent

Rayon ships a global pool that is already sized to your machine, so the "score every document" workload above is one line. For divide-and-conquer you use `rayon::join`, which forks two closures and only actually parallelizes them if a worker is free to pick up the second one.

```rust
// Cargo.toml: run `cargo add rayon`  (this pulls rayon 1.12)
use rayon::prelude::*;

fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    (2..=((n as f64).sqrt() as u64)).all(|d| n % d != 0)
}

// Divide-and-conquer with rayon::join: split the slice until chunks are small.
fn sum_of_primes(slice: &[u64]) -> u64 {
    if slice.len() <= 1000 {
        // Base case: small enough that sequential is faster than forking.
        return slice.iter().filter(|&&n| is_prime(n)).sum();
    }
    let mid = slice.len() / 2;
    let (left, right) = slice.split_at(mid);
    // Run both halves *potentially* in parallel; returns when both finish.
    let (left_sum, right_sum) =
        rayon::join(|| sum_of_primes(left), || sum_of_primes(right));
    left_sum + right_sum
}

fn main() {
    let numbers: Vec<u64> = (0..200_000).collect();

    // The GLOBAL pool: par_iter() borrows it automatically.
    let count = numbers.par_iter().filter(|&&n| is_prime(n)).count();
    println!("primes below 200000: {count}");

    let total = sum_of_primes(&numbers);
    println!("sum of those primes: {total}");

    // How many worker threads back the global pool?
    println!("global pool threads: {}", rayon::current_num_threads());
}
```

Running this on an 8-core machine prints the real output:

```text
primes below 200000: 17984
sum of those primes: 1709600813
global pool threads: 8
```

**Key points:**

- No pool to set up, no queue to manage: `par_iter()` and `rayon::join` both use the global pool.
- Closures **borrow** `numbers` directly — no copying, no serialization. The borrow checker proves the data outlives the parallel work.
- The pool defaults to one worker per logical core (`8` here), discovered the same way Node finds `availableParallelism()`.

---

## Detailed Explanation

### The global pool

The first time you call any Rayon API (`par_iter`, `join`, `spawn`, ...), Rayon lazily creates a **single process-wide pool** with `N` worker threads, where `N` is the number of logical cores. Those threads live for the rest of the program and are shared by every Rayon call, so you never pay thread-creation cost per task. `rayon::current_num_threads()` reports its size.

This is the big structural difference from the Node example: there is no per-task `Worker`, no idle list, no message queue in *your* code. Rayon owns one queue per worker thread and balances them with work-stealing.

### `rayon::join` and work-stealing

`rayon::join(a, b)` is the core primitive. It says "these two closures are independent; run them in parallel if it pays off." The implementation is subtle and worth understanding:

1. The current worker pushes task `b` onto its own local deque and starts running `a` immediately on the current thread.
2. If another worker is idle, it **steals** `b` and runs it concurrently.
3. If no worker is free, the current thread simply runs `b` itself after `a`, sequentially, with almost zero overhead.

That self-tuning behavior is why `sum_of_primes` can recurse all the way down without a manual "is this chunk big enough to be worth a thread?" heuristic on the *thread* side. The only tuning you supply is the **base-case cutoff** (`<= 1000` here) that stops the recursion; below it, the forking overhead would outweigh the parallelism. `join` returns a tuple `(A, B)` of both closures' results, so you compose results by **returning values**, not by mutating shared state.

### Custom pools

Sometimes you do *not* want the global pool: a latency-sensitive request handler should not be starved by a giant background batch job, and a CPU-bound batch should not fight your async runtime's threads. Build a dedicated pool with `ThreadPoolBuilder`:

```rust
// cargo add rayon
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;

fn main() {
    // A custom pool: fixed size, named threads (named threads show up in
    // profilers and panic messages, which is gold when debugging).
    let pool = ThreadPoolBuilder::new()
        .num_threads(4)
        .thread_name(|i| format!("worker-{i}"))
        .build()
        .expect("failed to build thread pool");

    // install() runs the closure ON this pool; par_iter inside uses these
    // 4 threads instead of the global pool.
    let sum: u64 = pool.install(|| (1..=1_000_000u64).into_par_iter().sum());
    println!("sum = {sum}");
    println!("custom pool threads = {}", pool.current_num_threads());

    // join can be scoped to a specific pool by wrapping it in install().
    let (a, b) = pool.install(|| {
        rayon::join(|| (0..500u64).sum::<u64>(), || (500..1000u64).sum::<u64>())
    });
    println!("a + b = {}", a + b);

    // scope(): spawn an arbitrary number of tasks that borrow local data and
    // are all guaranteed to finish before scope() returns.
    let data = vec![1, 2, 3, 4];
    let mut results = vec![0; data.len()];
    pool.scope(|s| {
        for (slot, &value) in results.iter_mut().zip(&data) {
            s.spawn(move |_| {
                *slot = value * value;
            });
        }
    });
    println!("squares = {results:?}");
}
```

Real output:

```text
sum = 500000500000
custom pool threads = 4
a + b = 499500
squares = [1, 4, 9, 16]
```

Three ways to feed a custom pool:

- **`pool.install(closure)`**: runs `closure` on the pool and blocks until it returns its value. Any Rayon call *inside* (`par_iter`, `join`, ...) uses this pool. This is the workhorse.
- **`pool.scope(|s| ...)`**: opens a scope where `s.spawn(...)` launches tasks that may borrow local variables (`slot`, `data`); the scope does not return until every spawned task completes, which is what makes the borrows sound.
- **`pool.spawn(closure)`** — fire-and-forget: the task runs on the pool whenever a worker is free, and `spawn` returns immediately. The closure must be `'static + Send` because nothing waits for it.

> **Note:** A custom pool is a real resource. When the `ThreadPool` value is dropped, Rayon signals its worker threads to finish and joins them. Keep the pool alive for as long as you submit work to it: usually store it in a struct or a `OnceLock`.

### Configuring the global pool

You can also resize the *global* pool, but exactly once and before first use:

```rust
// cargo add rayon
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;

fn main() {
    // Configure the GLOBAL pool — must happen before any Rayon call uses it.
    ThreadPoolBuilder::new()
        .num_threads(2)
        .build_global()
        .expect("global pool already initialized");

    println!("global threads = {}", rayon::current_num_threads());

    // A SECOND build_global fails: the global pool can be set only once.
    let second = ThreadPoolBuilder::new().num_threads(8).build_global();
    println!("second build_global is_err = {}", second.is_err());

    let total: u64 = (1..=100u64).into_par_iter().sum();
    println!("total = {total}");
}
```

Real output:

```text
global threads = 2
second build_global is_err = true
total = 5050
```

`build_global()` returns a `Result` and errors if the pool was already initialized, either by a prior `build_global` or by any earlier Rayon call that triggered lazy creation. Without calling `build_global`, the environment variable `RAYON_NUM_THREADS` also controls the default size, which is handy for ops to tune in production without recompiling.

---

## Key Differences

| Concern | Node.js `worker_threads` pool | Rayon thread pool |
| --- | --- | --- |
| Default pool | None; you build it (or use Piscina) | Global pool, auto-sized to cores |
| Task unit | A separate `Worker` + message | A plain closure |
| Data sharing | Copied via structured clone; `SharedArrayBuffer` for raw bytes only | Closures borrow your heap directly, checked at compile time |
| Load balancing | Your queue/idle-list logic | Built-in work-stealing scheduler |
| Result delivery | `postMessage` + `Promise`/event | Return value of `join` / `install`, or a channel |
| Divide-and-conquer | Manual chunking + many Workers | `rayon::join`, forks only when a core is free |
| Failure mode | Worker `error` event, pool may wedge | A panicking task propagates the panic out of `join`/`install` |

**The mental-model shift:** in Node, a "thread pool" is a fleet of isolated VMs you message; in Rust, it is a set of threads that run your closures against *shared, borrow-checked* memory. You parallelize a computation by splitting a borrow (`split_at_mut`) and handing each half to a worker, something that is simply impossible in JavaScript's isolate model.

> **Tip:** Rayon is for **CPU-bound** work (parsing, hashing, image processing, number crunching). For **I/O-bound** concurrency (HTTP, DB queries), reach for an async runtime like Tokio instead; see [Async](/11-async/). Mixing them is fine: run blocking CPU work on a Rayon pool so it never blocks Tokio's async workers.

---

## Common Pitfalls

### Pitfall 1: Trying to mutate shared state from both `join` closures

A TypeScript developer's instinct is to have both tasks write into a shared accumulator. Rust's borrow checker rejects this at compile time, because two closures holding `&mut` to the same variable is a data race waiting to happen:

```rust
fn main() {
    let mut counter = 0;
    // does not compile (error[E0499]: cannot borrow `counter` as mutable
    //    more than once at a time)
    rayon::join(|| counter += 1, || counter += 1);
    println!("{counter}");
}
```

The real compiler error (the line/column refer to the `rayon::join` call itself):

```text
error[E0499]: cannot borrow `counter` as mutable more than once at a time
 --> src/main.rs:3:34
  |
3 |     rayon::join(|| counter += 1, || counter += 1);
  |     ----------- -- -------       ^^ ------- second borrow occurs due to use of `counter` in closure
  |     |           |  |             |
  |     |           |  |             second mutable borrow occurs here
  |     |           |  first borrow occurs due to use of `counter` in closure
  |     |           first mutable borrow occurs here
  |     first borrow later used by call
```

**Fix:** have each closure *return* its contribution and combine the results. This is the whole point of `join` returning a tuple:

```rust
fn main() {
    // Each half computes its own value; the parent combines them.
    let (a, b) = rayon::join(|| 1, || 1);
    println!("counter = {}", a + b);
}
```

This prints `counter = 2`. (If you genuinely need shared mutation, you would use an atomic or a `Mutex` — see [Atomic Operations](/26-systems-programming/04-atomic-operations/) — but for divide-and-conquer, returning values is faster and simpler.)

### Pitfall 2: Assuming `join` always uses two threads

`rayon::join(a, b)` does **not** guarantee `a` and `b` run on different threads. If every worker is busy, `b` runs sequentially after `a` on the same thread. Never write code whose *correctness* depends on the two closures overlapping in time. `join` is an optimization hint, not a concurrency guarantee. (For guaranteed concurrent execution with explicit threads, see [Native Threads with `std::thread`](/26-systems-programming/00-threads/).)

### Pitfall 3: Forgetting the base-case cutoff in recursive `join`

Recursing with `join` all the way down to single elements drowns the real work in forking overhead. Always stop forking once chunks are small enough to process sequentially (the `slice.len() <= 1000` check in `sum_of_primes`). Without it, a parallel version can be *slower* than the sequential one.

### Pitfall 4: Letting a custom pool be dropped too early

Because dropping a `ThreadPool` shuts down its workers, this is a classic mistake:

```rust
// Conceptually broken: the pool is dropped at the end of the function,
// so the spawned task may be cut short.
use rayon::ThreadPoolBuilder;

fn start_background() {
    let pool = ThreadPoolBuilder::new().num_threads(2).build().unwrap();
    pool.spawn(|| { /* long-running work */ });
    // `pool` drops here → its workers are signaled to finish. Bad for spawn().
}
```

Keep the pool alive (store it in a long-lived struct or a `static OnceLock`) for the lifetime of the work you submit to it. `install` and `scope` block until their work finishes, so they are safe; `spawn` is the one to watch.

---

## Best Practices

- **Default to the global pool.** For ordinary CPU-bound work, just call `par_iter()` or `rayon::join` and let Rayon size and manage the pool. Reach for a custom pool only when you need isolation (background batch vs. request path) or a non-default size.
- **Compose with return values, not shared mutation.** `join` and `reduce` hand you results; prefer them over `Mutex`/atomics inside the hot loop.
- **Tune the cutoff, not the thread count.** The lever for recursive `join` performance is the sequential base-case size, not how many threads you spawn.
- **Name your pool threads.** `thread_name(|i| format!("scorer-{i}"))` makes panics, profilers, and `top` output readable.
- **Propagate errors cleanly.** A parallel pipeline of fallible work can `collect()` into a `Result<Vec<T>, E>` (stops conceptually at the first error) or use `try_reduce`/`try_for_each`:

```rust
// cargo add rayon
use rayon::prelude::*;

fn main() {
    // Collecting Results: any Err short-circuits to a single Err.
    let inputs = vec!["10", "20", "30", "oops", "40"];
    let parsed: Result<Vec<i64>, _> =
        inputs.par_iter().map(|s| s.parse::<i64>()).collect();
    println!("parse result is_err = {}", parsed.is_err());

    // try_reduce: fold in parallel, bailing on the first error.
    let good = vec!["10", "20", "30"];
    let sum: Result<i64, _> = good
        .par_iter()
        .map(|s| s.parse::<i64>())
        .try_reduce(|| 0, |acc, n| Ok(acc + n));
    println!("sum = {sum:?}");
}
```

Real output:

```text
parse result is_err = true
sum = Ok(60)
```

- **Keep Rayon away from blocking I/O.** A worker blocked on a socket cannot steal other tasks. Do I/O on an async runtime and feed only CPU work to Rayon.

---

## Real-World Example

A document-scoring batch job — the Rust counterpart to the Node `WorkerPool` at the top. It runs on a **dedicated** pool (so a huge batch never starves the rest of the service) and combines per-document scores with a parallel `reduce`:

```rust
// cargo add rayon
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::collections::HashMap;

/// A CPU-bound job: score a document by word-frequency (sum of squared counts).
fn score_document(text: &str) -> u64 {
    let mut counts: HashMap<&str, u64> = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word).or_insert(0) += 1;
    }
    counts.values().map(|&c| c * c).sum()
}

fn main() {
    let documents: Vec<String> = (0..10_000)
        .map(|i| format!("doc {i} alpha beta beta gamma gamma gamma"))
        .collect();

    // A dedicated pool isolates this batch from the global pool / request path.
    let pool = ThreadPoolBuilder::new()
        .num_threads(4)
        .thread_name(|i| format!("scorer-{i}"))
        .build()
        .expect("pool");

    // install() runs the whole pipeline on the 4 dedicated workers.
    let (total, max) = pool.install(|| {
        documents
            .par_iter()
            .map(|doc| score_document(doc))
            .map(|score| (score, score))
            // Parallel reduce: an associative combine of per-document results.
            .reduce(
                || (0, 0),
                |(sum_a, max_a), (sum_b, max_b)| {
                    (sum_a + sum_b, max_a.max(max_b))
                },
            )
    });

    println!("documents scored: {}", documents.len());
    println!("total score: {total}");
    println!("max single-doc score: {max}");
}
```

Real output:

```text
documents scored: 10000
total score: 160000
max single-doc score: 16
```

Compared to the Node version, there is no message queue, no idle-worker bookkeeping, and no serialization: `par_iter()` borrows `documents` directly, the 4 named workers steal chunks from each other, and `reduce` folds the results in parallel. The pool is dropped at the end of `main`, cleanly joining its workers.

---

## Further Reading

- [Rayon documentation](https://docs.rs/rayon): the crate's full API.
- [`rayon::join`](https://docs.rs/rayon/latest/rayon/fn.join.html) and [`ThreadPoolBuilder`](https://docs.rs/rayon/latest/rayon/struct.ThreadPoolBuilder.html) — the primitives covered here.
- [The Rayon FAQ](https://github.com/rayon-rs/rayon/blob/main/FAQ.md): work-stealing internals and gotchas.
- [Parallel Iterators with Rayon](/26-systems-programming/02-parallel-iterators/) — the `par_iter()`/`par_bridge()` layer that runs on these pools, and when it actually helps.
- [Native Threads with `std::thread`](/26-systems-programming/00-threads/): raw `std::thread` and scoped threads, the lower-level building blocks.
- [Channels](/26-systems-programming/03-channels/) — moving results off pool workers with `mpsc`/crossbeam channels.
- [Atomic Operations](/26-systems-programming/04-atomic-operations/) and [Memory Ordering](/26-systems-programming/05-memory-ordering/) — when you genuinely need shared mutable state across pool workers.
- [Async](/11-async/) — async runtimes for I/O-bound concurrency (the complement to Rayon).
- [Security](/27-security/) — concurrency correctness as a security concern (data races, denial-of-service via unbounded parallelism).
- [Understanding Cargo](/01-getting-started/03-cargo-basics/) — adding dependencies like `rayon` with `cargo add`.

---

## Exercises

### Exercise 1: Parallel quicksort with `rayon::join`

**Difficulty:** Medium

**Objective:** Use `rayon::join` for genuine divide-and-conquer parallelism over a mutable slice.

**Instructions:** Write `fn parallel_quicksort<T: Send + Ord + Copy>(slice: &mut [T])` that partitions the slice around a pivot, then sorts the two partitions with `rayon::join`. Use `split_at_mut` to hand each half a disjoint mutable borrow. Verify the result is sorted.

<details>
<summary>Solution</summary>

```rust
// cargo add rayon
fn parallel_quicksort<T: Send + Ord + Copy>(slice: &mut [T]) {
    if slice.len() <= 1 {
        return;
    }
    let pivot_index = partition(slice);
    // split_at_mut gives two disjoint &mut halves — no aliasing, so join is sound.
    let (left, right) = slice.split_at_mut(pivot_index);
    rayon::join(
        || parallel_quicksort(left),
        || parallel_quicksort(&mut right[1..]), // skip the pivot at right[0]
    );
}

fn partition<T: Ord + Copy>(slice: &mut [T]) -> usize {
    let len = slice.len();
    let pivot = slice[len / 2];
    slice.swap(len / 2, len - 1);
    let mut store = 0;
    for i in 0..len - 1 {
        if slice[i] < pivot {
            slice.swap(i, store);
            store += 1;
        }
    }
    slice.swap(store, len - 1);
    store
}

fn main() {
    let mut data = vec![9, 3, 7, 1, 8, 2, 6, 5, 4, 0, 11, 10];
    parallel_quicksort(&mut data);
    println!("sorted = {data:?}");
    assert!(data.windows(2).all(|w| w[0] <= w[1]));
}
```

Output:

```text
sorted = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
```

The key insight is that `split_at_mut` proves to the compiler that the two halves never overlap, which is exactly what lets `rayon::join` run them in parallel safely.

</details>

### Exercise 2: A bounded custom pool

**Difficulty:** Medium

**Objective:** Build a custom `ThreadPool` of a chosen size and run a parallel reduction on it.

**Instructions:** Write `fn batch_max(values: &[u64], threads: usize) -> u64` that builds a `ThreadPool` with `threads` workers and uses `install` + `par_iter().reduce` to compute the maximum. Confirm it returns the right answer with `threads = 2`.

<details>
<summary>Solution</summary>

```rust
// cargo add rayon
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;

fn batch_max(values: &[u64], threads: usize) -> u64 {
    let pool = ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
    // reduce needs an identity (u64::MIN) and an associative combiner (u64::max).
    pool.install(|| values.par_iter().copied().reduce(|| u64::MIN, u64::max))
}

fn main() {
    let m = batch_max(&[3, 99, 42, 7, 88], 2);
    println!("max = {m}");
    assert_eq!(m, 99);
}
```

Output:

```text
max = 99
```

`reduce`'s identity element (`u64::MIN`) must be neutral for the operation — `max(x, MIN) == x` — and the combiner must be associative so partial results from different workers compose correctly.

</details>

### Exercise 3: Fire-and-forget tasks with a result channel

**Difficulty:** Hard

**Objective:** Use `pool.spawn` for detached tasks and collect their results over an `mpsc` channel.

**Instructions:** Build a 4-thread pool, `spawn` 8 jobs that each compute `job_id * job_id`, and send `(job_id, result)` down an `mpsc` channel. After spawning, drop the original sender and drain the receiver into a sorted `Vec`. Explain why dropping the sender matters.

<details>
<summary>Solution</summary>

```rust
// cargo add rayon
use rayon::ThreadPoolBuilder;
use std::sync::mpsc::channel;

fn main() {
    let pool = ThreadPoolBuilder::new()
        .num_threads(4)
        .thread_name(|i| format!("task-{i}"))
        .build()
        .unwrap();

    let (tx, rx) = channel();
    for job_id in 0..8u32 {
        let tx = tx.clone();
        // spawn detaches the task; it runs whenever a worker is free.
        pool.spawn(move || {
            let result = job_id * job_id;
            tx.send((job_id, result)).unwrap();
        });
    }
    // Drop the original sender: rx.iter() ends only once EVERY sender is gone.
    drop(tx);

    let mut results: Vec<(u32, u32)> = rx.iter().collect();
    results.sort();
    println!("results = {results:?}");
}
```

Output:

```text
results = [(0, 0), (1, 1), (2, 4), (3, 9), (4, 16), (5, 25), (6, 36), (7, 49)]
```

`rx.iter()` blocks until all senders are dropped. Each spawned task holds a cloned `tx` that drops when the task finishes, but the *original* `tx` would keep the channel open forever, so we `drop(tx)` explicitly after the spawn loop. The pool stays alive through the end of `main`, so every detached task gets to run. See [Channels](/26-systems-programming/03-channels/) for more on this pattern.

</details>
