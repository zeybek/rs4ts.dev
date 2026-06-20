---
title: "Channels: Message Passing Between Threads"
description: "Pass typed values between Rust threads with mpsc and crossbeam channels, like a Worker's postMessage, but send moves ownership so data races won't compile."
---

In JavaScript you almost never share memory between concurrent units of work. A `Worker` cannot touch the main thread's variables; you `postMessage` data across a boundary instead. Rust lets you share memory across native threads, but it also gives you that same message-passing model, and for many problems it is the cleanest, safest tool. Channels let one thread hand values to another over a typed, thread-safe queue.

---

## Quick Overview

A **channel** is a one-way, typed pipe between threads: producers `send` values into one end, a consumer `recv`s them out the other. The standard library ships `std::sync::mpsc` — **m**ulti-**p**roducer, **s**ingle-**c**onsumer — and the popular `crossbeam-channel` crate adds multi-consumer support, a `select!` macro, and faster internals. If you have ever used `worker.postMessage()` or an `EventEmitter`/async queue in Node, channels will feel familiar, except the queue is statically typed and the compiler guarantees you cannot data-race on what you send.

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically. Every Rust snippet below was compiled and run on stable.

---

## TypeScript/JavaScript Example

JavaScript's true concurrency lives in **Worker threads**, and they communicate exclusively by message passing; there is no shared mutable memory by default. A typical "fan out work, collect results" pipeline looks like this:

```typescript
// main.ts — Node v22, using worker_threads
import { Worker } from "node:worker_threads";

interface Job {
  id: number;
  payload: string;
}
interface JobResult {
  id: number;
  length: number;
}

function runWorker(job: Job): Promise<JobResult> {
  return new Promise((resolve, reject) => {
    const worker = new Worker("./worker.mjs", { workerData: job });
    // The ONLY way data crosses the thread boundary is postMessage/onmessage.
    worker.once("message", (result: JobResult) => resolve(result));
    worker.once("error", reject);
  });
}

async function main() {
  const jobs: Job[] = Array.from({ length: 10 }, (_, id) => ({
    id,
    payload: `payload-${id}`,
  }));

  // Collect results as the workers report back.
  const results = await Promise.all(jobs.map(runWorker));
  const total = results.reduce((sum, r) => sum + r.length, 0);
  console.log(`processed ${results.length} jobs, total bytes: ${total}`);
}

main();
```

```javascript
// worker.mjs — ESM module, matching main.ts's import style.
import { parentPort, workerData } from "node:worker_threads";
parentPort.postMessage({ id: workerData.id, length: workerData.payload.length });
```

Notice the shape: each unit of work is a self-contained message, results come back as messages, and nothing is shared by reference. That is exactly the model Rust channels give you, but with one process, real OS threads, and compile-time type checking on every message.

---

## Rust Equivalent

The minimal channel: one producer thread sends, the main thread consumes.

```rust playground
use std::sync::mpsc;
use std::thread;

fn main() {
    // Create a channel: `tx` is the sending half, `rx` the receiving half.
    let (tx, rx) = mpsc::channel();

    // Spawn a producer thread. `move` transfers ownership of `tx` into it.
    let producer = thread::spawn(move || {
        for i in 1..=5 {
            println!("producer: sending {i}");
            tx.send(i).expect("receiver dropped");
        }
        // `tx` is dropped here when the closure ends, which closes the channel.
    });

    // The main thread is the consumer. Iterating over `rx` blocks until a value
    // arrives, and the loop ends cleanly once every sender has been dropped.
    for value in rx {
        println!("consumer: got {value}");
    }

    producer.join().unwrap();
    println!("done");
}
```

**Output:**

```
producer: sending 1
producer: sending 2
producer: sending 3
producer: sending 4
producer: sending 5
consumer: got 1
consumer: got 2
consumer: got 3
consumer: got 4
consumer: got 5
done
```

> **Note:** The producer ran to completion before the consumer printed anything here only because the values are tiny and the channel buffers them instantly. With slower work the two threads interleave. Channels never guarantee that send and receive happen in lockstep — only that order is preserved per sender.

---

## Detailed Explanation

### Creating a channel

```rust
use std::sync::mpsc;
let (tx, rx) = mpsc::channel(); // returns (Sender<T>, Receiver<T>)
```

`mpsc::channel()` returns a tuple of two linked halves. The element type `T` is inferred from the first `tx.send(value)` call (here `i32`), so you rarely annotate it. The channel is **unbounded**: `send` never blocks because the internal queue grows as needed. Naming the variables `tx`/`rx` (transmit/receive) is the universal Rust convention.

### Sending and ownership transfer

```rust
tx.send(i).expect("receiver dropped");
```

`send` *moves* the value into the channel. This is the heart of why channels are safe: once you send a `String`, the sending thread no longer owns it and cannot mutate it, so there is nothing to race on. `send` returns `Result<(), SendError<T>>`. It only fails if the receiver has been dropped, and when it does, it hands your value *back* inside the error so nothing is lost.

### Receiving

You have three ways to pull values out:

```rust
let v = rx.recv();            // blocks; Ok(T) or Err(RecvError) when channel closed
let v = rx.try_recv();        // never blocks; Err(Empty) if nothing waiting
let v = rx.recv_timeout(dur); // blocks up to `dur`
for v in rx { /* ... */ }     // iterator: blocks per item, ends when channel closes
```

The `for value in rx` loop is the idiomatic consumer. It calls `recv` repeatedly and stops automatically when the channel is closed, which happens when **all** senders have been dropped. That is the single most important rule to internalize: *the loop only ends when every `Sender` is gone.* Forget to drop a sender and your consumer blocks forever.

### Why `move`?

```rust
let producer = thread::spawn(move || { /* uses tx */ });
```

The spawned thread may outlive `main`'s current stack frame, so the closure must *own* everything it touches. `move` transfers `tx` into the closure. See [Native Threads with `std::thread`](/26-systems-programming/00-threads/) for the full story on `spawn`, `move` closures, and scoped threads.

### Multiple producers

`mpsc` is *multi*-producer: clone the sender, one clone per producer thread.

```rust playground
use std::sync::mpsc;
use std::thread;

fn main() {
    let (tx, rx) = mpsc::channel();

    let mut handles = Vec::new();
    for worker_id in 0..3 {
        // Each worker gets its own clone of the sender.
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            for job in 0..2 {
                tx.send(format!("worker {worker_id} -> job {job}")).unwrap();
            }
        }));
    }

    // Drop the ORIGINAL sender so the channel can close once the clones finish.
    drop(tx);

    // Collect everything; the loop ends when all senders are gone.
    let mut received: Vec<String> = rx.iter().collect();
    received.sort();
    for msg in &received {
        println!("{msg}");
    }
    println!("total: {}", received.len());

    for h in handles {
        h.join().unwrap();
    }
}
```

**Output:**

```
worker 0 -> job 0
worker 0 -> job 1
worker 1 -> job 0
worker 1 -> job 1
worker 2 -> job 0
worker 2 -> job 1
total: 6
```

Each `tx.clone()` is a new handle to the *same* channel. The `drop(tx)` on the original is essential: as long as one un-dropped sender exists anywhere, the receiver keeps waiting. (We sort the output only to make it deterministic for printing; the actual arrival order across threads is non-deterministic.)

### Bounded channels and backpressure

`mpsc::sync_channel(capacity)` creates a **bounded** channel. Once the buffer is full, `send` *blocks* until the consumer makes room. This is **backpressure**: your fast producer is forced to slow down to match a slow consumer, instead of building an unbounded backlog in memory.

```rust playground
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn main() {
    // sync_channel(0) is a "rendezvous" channel: send blocks until a receiver
    // takes the value. A positive capacity gives a bounded buffer.
    let (tx, rx) = mpsc::sync_channel::<i32>(2);

    let producer = thread::spawn(move || {
        for i in 0..5 {
            println!("producer: trying to send {i}");
            tx.send(i).unwrap(); // blocks when the buffer (cap 2) is full
            println!("producer: sent {i}");
        }
    });

    // Consume slowly so the bounded buffer applies backpressure.
    thread::sleep(Duration::from_millis(50));
    for value in rx {
        println!("consumer: got {value}");
        thread::sleep(Duration::from_millis(20));
    }

    producer.join().unwrap();
}
```

**Output:**

```
producer: trying to send 0
producer: sent 0
producer: trying to send 1
producer: sent 1
producer: trying to send 2
consumer: got 0
producer: sent 2
producer: trying to send 3
consumer: got 1
producer: sent 3
producer: trying to send 4
consumer: got 2
producer: sent 4
consumer: got 3
consumer: got 4
```

Look at the third message: `trying to send 2` prints, but `sent 2` does **not** appear until `got 0` has freed a buffer slot. The producer is blocked on a full buffer — exactly the rate-limiting JavaScript devs usually hand-roll with a semaphore or a `p-limit` library.

### Non-blocking and timed receives

```rust playground
use std::sync::mpsc;
use std::time::Duration;

fn main() {
    let (tx, rx) = mpsc::channel::<i32>();

    // try_recv never blocks; it reports Empty if nothing is waiting.
    match rx.try_recv() {
        Ok(v) => println!("got {v}"),
        Err(mpsc::TryRecvError::Empty) => println!("try_recv: nothing yet"),
        Err(mpsc::TryRecvError::Disconnected) => println!("try_recv: closed"),
    }

    // recv_timeout blocks, but gives up after the deadline.
    match rx.recv_timeout(Duration::from_millis(30)) {
        Ok(v) => println!("got {v}"),
        Err(mpsc::RecvTimeoutError::Timeout) => println!("recv_timeout: timed out"),
        Err(mpsc::RecvTimeoutError::Disconnected) => println!("recv_timeout: closed"),
    }

    // Drop every sender, then recv() returns RecvError instead of blocking forever.
    drop(tx);
    match rx.recv() {
        Ok(v) => println!("got {v}"),
        Err(e) => println!("recv after drop: {e}"),
    }
}
```

**Output:**

```
try_recv: nothing yet
recv_timeout: timed out
recv after drop: receiving on a closed channel
```

`try_recv` is what you reach for in a polling loop where you also have other work to do; `recv_timeout` lets you wake up periodically to check a shutdown flag. The `RecvError` on a closed channel is your clean "no more data ever" signal.

---

## Key Differences

| Concept | JavaScript (Worker threads) | Rust channels |
| --- | --- | --- |
| Message typing | Untyped; structured-clone at runtime | Statically typed `Sender<T>`/`Receiver<T>` |
| What crosses | A *copy* (structured clone) or transfer | A *moved* value — ownership transfers, no copy |
| Shared memory | Only `SharedArrayBuffer`, manually | Allowed and safe; channels are one option among many |
| Consumers per channel | One `onmessage` per port | `mpsc`: exactly one. `crossbeam`: many |
| Backpressure | Hand-rolled (semaphore, `p-limit`) | Built in via `sync_channel(n)` |
| Closing signal | `port.close()` / worker exit | Drop all senders → receiver loop ends |
| Select over many sources | `Promise.race` (async only) | `crossbeam_channel::select!` (blocking, real threads) |

The deepest difference is **ownership transfer**. In JS, `postMessage` *copies* your object (or detaches a transferable). In Rust, `send` *moves* it: the value is gone from the sender, which is precisely why no lock is needed and no data race is possible. The compiler enforces that `T` is `Send` (safe to move across threads) before it will let you build the channel at all.

The second difference is the **single-consumer** restriction. The name `mpsc` is literal: there is exactly one `Receiver`, and it cannot be cloned. That is not a limitation of channels in general, just of the std implementation; reach for `crossbeam-channel` when you need many consumers.

---

## crossbeam-channel: multi-consumer and `select!`

When you want a *pool* of worker threads all pulling from one queue, std `mpsc` cannot help directly because its `Receiver` is neither `Clone` nor `Sync`. The `crossbeam-channel` crate provides true MPMC (multi-producer, multi-consumer) channels whose receiver *is* clonable.

```bash
cargo add crossbeam-channel
```

> **Note:** `cargo add` is built into Cargo (since 1.62); no `cargo-edit` needed. This resolves to `crossbeam-channel = "0.5"` on current stable.

```rust playground
use crossbeam_channel::unbounded;
use std::thread;

fn main() {
    // crossbeam-channel is MULTI-producer, MULTI-consumer. The receiver is
    // Clone + Send + Sync, so several workers can pull from the same queue.
    let (tx, rx) = unbounded::<u32>();

    // Three worker threads share one receiver (a work-stealing pool).
    let mut workers = Vec::new();
    for id in 0..3 {
        let rx = rx.clone();
        workers.push(thread::spawn(move || {
            let mut count = 0;
            for job in rx.iter() {
                count += 1;
                let _ = job; // pretend to process it
            }
            (id, count)
        }));
    }

    // Feed 12 jobs, then drop the sender to signal "no more work".
    for job in 0..12 {
        tx.send(job).unwrap();
    }
    drop(tx);

    let mut total = 0;
    for w in workers {
        let (id, count) = w.join().unwrap();
        println!("worker {id} handled {count} jobs");
        total += count;
    }
    println!("total handled: {total}");
}
```

**Output (one run — the per-worker split is non-deterministic):**

```
worker 0 handled 12 jobs
worker 1 handled 0 jobs
worker 2 handled 0 jobs
total handled: 12
```

The *total* is always 12 — every job is handled exactly once — but which worker grabs which job depends on timing. Here one worker happened to drain the queue before the others woke up; under real load the work spreads out. For CPU-bound data parallelism you usually want [rayon](/26-systems-programming/02-parallel-iterators/) instead of hand-built channel pools, but channels shine when work arrives over time rather than as one fixed batch.

The other crossbeam strength is `select!`, which waits on several channels at once and runs whichever is ready first; there is no `mpsc` equivalent:

```rust playground
use crossbeam_channel::{select, unbounded};
use std::thread;
use std::time::Duration;

fn main() {
    let (work_tx, work_rx) = unbounded::<String>();
    let (shutdown_tx, shutdown_rx) = unbounded::<()>();

    let worker = thread::spawn(move || {
        loop {
            // select! waits on several channels at once and runs the arm of
            // whichever becomes ready first.
            select! {
                recv(work_rx) -> msg => match msg {
                    Ok(job) => println!("processing {job}"),
                    Err(_) => break, // work channel closed
                },
                recv(shutdown_rx) -> _ => {
                    println!("shutdown signal received, draining and exiting");
                    break;
                }
            }
        }
    });

    work_tx.send("job-1".into()).unwrap();
    work_tx.send("job-2".into()).unwrap();
    thread::sleep(Duration::from_millis(20));
    shutdown_tx.send(()).unwrap();

    worker.join().unwrap();
    println!("worker stopped");
}
```

**Output:**

```
processing job-1
processing job-2
shutdown signal received, draining and exiting
worker stopped
```

This "work channel plus shutdown channel" pattern is the standard way to give a worker a clean exit. It is the thread-based cousin of the `select!` you may have seen in async Rust ([Async](/11-async/)). For wiring `select!` up to real OS signals like `SIGINT`/`SIGTERM`, see [Signal Handling and Clean Shutdown](/26-systems-programming/08-signals/).

> **Tip:** crossbeam also offers `bounded(n)` (for backpressure, like `sync_channel`) and an `after(duration)` channel that fires once after a delay, perfect for adding timeouts to a `select!`.

---

## Common Pitfalls

### Pitfall 1: forgetting to drop the original sender — the consumer hangs forever

This is the number-one channel bug. The receiver loop only ends when **all** senders are dropped. If you clone senders into workers but keep the original alive in `main`, the loop never terminates.

```rust
use std::sync::mpsc;
use std::thread;

fn main() {
    let (tx, rx) = mpsc::channel::<i32>();
    for _ in 0..2 {
        let tx = tx.clone();
        thread::spawn(move || { tx.send(1).unwrap(); });
    }
    // BUG: original `tx` is still alive here, so `rx` never closes.
    for v in rx {              // runs the two values, then BLOCKS FOREVER
        println!("{v}");
    }
}
```

The program prints two `1`s and then hangs. The fix is one line: `drop(tx);` before the loop, or arrange the scope so the original `tx` is consumed/dropped. There is no compiler error for this — it is a logic deadlock — so make dropping senders a deliberate habit.

### Pitfall 2: trying to share a single `mpsc::Receiver` across threads

`mpsc` means *single*-consumer. The `Receiver` cannot be cloned, and it cannot be moved into two threads:

```rust
use std::sync::mpsc;
use std::thread;

fn main() {
    let (tx, rx) = mpsc::channel::<i32>();
    tx.send(1).unwrap();

    let h1 = thread::spawn(move || {
        for v in rx.iter() { println!("a: {v}"); }
    });
    let h2 = thread::spawn(move || {       // does not compile (error[E0382])
        for v in rx.iter() { println!("b: {v}"); }
    });

    h1.join().unwrap();
    h2.join().unwrap();
}
```

The real compiler error:

```
error[E0382]: use of moved value: `rx`
  --> src/main.rs:11:28
   |
 5 |     let (tx, rx) = mpsc::channel::<i32>();
   |              -- move occurs because `rx` has type `std::sync::mpsc::Receiver<i32>`, which does not implement the `Copy` trait
...
 8 |     let h1 = thread::spawn(move || {
   |                            ------- value moved into closure here
 9 |         for v in rx.iter() { println!("a: {v}"); }
   |                  -- variable moved due to use in closure
10 |     });
11 |     let h2 = thread::spawn(move || {       // does not compile (error[E0382])
   |                            ^^^^^^^ value used here after move
12 |         for v in rx.iter() { println!("b: {v}"); }
   |                  -- use occurs due to use in closure
```

**Fix:** use `crossbeam-channel` (its receiver *is* clonable), or wrap the std receiver in `Arc<Mutex<Receiver<T>>>` and lock briefly to pull one item, the technique shown in the Real-World Example below.

### Pitfall 3: assuming `send` blocks like it does in Go or with `sync_channel`

`mpsc::channel()` is **unbounded**: `send` never blocks and never applies backpressure. A fast producer feeding a slow consumer will balloon memory until you OOM. If you need flow control, use `mpsc::sync_channel(capacity)` (or crossbeam's `bounded`). Choosing unbounded "to be safe" is usually the *unsafe* choice for a long-running service.

### Pitfall 4: ignoring the `Result` from `send`

`send` returns `Result`. If you `.unwrap()` it and the consumer has already exited (say, after an error), your producer thread *panics*. Often the right move after the receiver is gone is to stop gracefully, not crash:

```rust
// Instead of tx.send(x).unwrap();
if tx.send(x).is_err() {
    // Receiver hung up — nothing left to do, so stop producing.
    break;
}
```

### Pitfall 5: expecting channel sends to be a copy (it is a move)

Coming from `postMessage`, you might expect the value to still be usable after sending. It is not — `send` *moves* ownership. After `tx.send(my_string)`, `my_string` is gone. If both threads genuinely need the data, `clone()` before sending or send an `Arc<T>` (a cheap shared-ownership pointer; see [Reference Counting with `Rc<T>` and `Arc<T>`](/05-ownership/07-reference-counting/)).

---

## Best Practices

- **Prefer message passing over shared `Mutex` state when you can.** "Do not communicate by sharing memory; share memory by communicating." Channels make ownership flow obvious and sidestep most locking bugs.
- **Always reason about who holds the last sender.** Make dropping senders explicit (`drop(tx)`) right after you finish handing them out, so the consumer's loop terminates deterministically.
- **Use `sync_channel`/`bounded` for any unbounded producer in a long-lived process** to get backpressure for free and bound your memory.
- **Send rich, self-contained messages.** Define an `enum` of message variants (e.g. `enum Msg { Job(Task), Flush, Shutdown }`) so one channel can carry several kinds of command, matched with `match`. This is far cleaner than several parallel channels and parallels a discriminated union in TypeScript.
- **Reach for `crossbeam-channel` when you need** multiple consumers, `select!` over several channels, or a measurable speedup; it is faster than std `mpsc` and a near drop-in replacement.
- **Match the tool to the workload.** Channels excel at *streaming* work that arrives over time. For chewing through one fixed collection in parallel, [rayon's parallel iterators](/26-systems-programming/02-parallel-iterators/) are simpler and faster.
- **In async code, do not use these blocking channels.** Use `tokio::sync::mpsc` instead; a blocking `recv` will stall the async runtime. See [Async](/11-async/).

---

## Real-World Example

A bounded worker pool: the main thread feeds jobs into a channel, a fixed set of worker threads process them concurrently, and results stream back on a second channel where `main` aggregates them. This is the Rust analogue of the worker-pool TypeScript code at the top: one process, real threads, fully type-checked.

Because std `mpsc` has a single consumer, we share the *job* receiver across workers with `Arc<Mutex<Receiver<_>>>` (lock briefly, pull one job, unlock). The *results* channel is plain `mpsc` — many producers, one consumer — which is exactly what `mpsc` is built for.

```rust playground
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// A unit of work handed to a worker.
struct Job {
    id: u32,
    payload: String,
}

/// What a worker produces.
struct JobResult {
    id: u32,
    worker: usize,
    length: usize,
}

fn main() {
    const WORKERS: usize = 4;

    // jobs: main -> workers. std mpsc receivers cannot be cloned, so we wrap the
    // receiver in Arc<Mutex<_>> to share it across the worker pool.
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let job_rx = Arc::new(Mutex::new(job_rx));

    // results: workers -> main (multi-producer, single-consumer: a perfect fit
    // for plain mpsc).
    let (result_tx, result_rx) = mpsc::channel::<JobResult>();

    let mut workers = Vec::new();
    for worker_id in 0..WORKERS {
        let job_rx = Arc::clone(&job_rx);
        let result_tx = result_tx.clone();
        workers.push(thread::spawn(move || loop {
            // Lock only long enough to pull one job, then release immediately.
            let job = {
                let guard = job_rx.lock().unwrap();
                guard.recv()
            };
            match job {
                Ok(job) => {
                    thread::sleep(Duration::from_millis(5)); // simulate work
                    let _ = result_tx.send(JobResult {
                        id: job.id,
                        worker: worker_id,
                        length: job.payload.len(),
                    });
                }
                Err(_) => break, // job channel closed: no more work
            }
        }));
    }

    // Submit work, then close the job channel so workers exit their loops.
    for id in 0..10 {
        job_tx
            .send(Job { id, payload: format!("payload-{id}") })
            .unwrap();
    }
    drop(job_tx);

    // Drop main's spare result sender so result_rx ends once the workers finish.
    drop(result_tx);

    // Aggregate results as they stream in.
    let mut total_len = 0usize;
    let mut count = 0;
    for r in result_rx {
        total_len += r.length;
        count += 1;
        println!("job {} done by worker {} (len {})", r.id, r.worker, r.length);
    }

    for w in workers {
        w.join().unwrap();
    }
    println!("processed {count} jobs, total payload bytes: {total_len}");
}
```

**Output (tail — worker assignment varies between runs):**

```
job 6 done by worker 1 (len 9)
job 7 done by worker 3 (len 9)
job 8 done by worker 2 (len 9)
job 9 done by worker 0 (len 9)
processed 10 jobs, total payload bytes: 90
```

Two `drop` calls do the load-bearing work. `drop(job_tx)` closes the job channel so each worker's `recv()` eventually returns `Err` and the loop breaks. `drop(result_tx)` releases `main`'s spare result sender so that, once the workers (which hold the only other clones) finish and drop theirs, the `for r in result_rx` loop ends. Miss either one and the program deadlocks, which is why "account for every sender" is the discipline that makes channels reliable.

> **Tip:** With `crossbeam-channel` you could delete the `Arc<Mutex<_>>` entirely and just `rx.clone()` the job receiver into each worker. The std version above is worth understanding because you will meet it in code that avoids extra dependencies.

For handling untrusted input that arrives over such a pipeline — sizing bounded channels to resist memory-exhaustion attacks, validating messages before processing — see [Security](/27-security/).

---

## Further Reading

- [`std::sync::mpsc` documentation](https://doc.rust-lang.org/std/sync/mpsc/): the standard-library channel API.
- [`std::sync::mpsc::sync_channel`](https://doc.rust-lang.org/std/sync/mpsc/fn.sync_channel.html): bounded channels and backpressure.
- [The Rust Book, ch. 16.2: "Using Message Passing to Transfer Data Between Threads"](https://doc.rust-lang.org/book/ch16-02-message-passing.html).
- [`crossbeam-channel` documentation](https://docs.rs/crossbeam-channel): MPMC channels, `select!`, `bounded`/`after`.
- [Native Threads with `std::thread`](/26-systems-programming/00-threads/) — spawning and joining the threads you connect with channels.
- [Thread Pools with Rayon](/26-systems-programming/01-thread-pools/) and [Parallel Iterators with Rayon](/26-systems-programming/02-parallel-iterators/) — when rayon is a better fit than hand-built channel pools.
- [Atomic Operations](/26-systems-programming/04-atomic-operations/) and [Memory Ordering](/26-systems-programming/05-memory-ordering/) — the lower-level shared-state primitives channels are built on top of.
- [Reference Counting with `Rc<T>` and `Arc<T>`](/05-ownership/07-reference-counting/) — `Arc` for sharing data you cannot move.
- [Async](/11-async/) — `tokio::sync::mpsc` for the async world.

---

## Exercises

### Exercise 1: Parallel sum with fan-in

**Difficulty:** Beginner

**Objective:** Use a multi-producer channel to split work across threads and combine the partial results.

**Instructions:** Write a function `parallel_sum(data: Vec<u64>, chunks: usize) -> u64` that splits `data` into roughly `chunks` slices, spawns a thread per slice to sum it, sends each partial sum over an `mpsc` channel, and returns the grand total collected from the receiver. Verify it returns `5050` for `1..=100`.

<details>
<summary>Solution</summary>

```rust playground
use std::sync::mpsc;
use std::thread;

fn parallel_sum(data: Vec<u64>, chunks: usize) -> u64 {
    let (tx, rx) = mpsc::channel();
    let chunk_size = data.len().div_ceil(chunks); // round up so we cover all items

    for chunk in data.chunks(chunk_size) {
        let tx = tx.clone();
        let owned: Vec<u64> = chunk.to_vec(); // own the data the thread will read
        thread::spawn(move || {
            let partial: u64 = owned.iter().sum();
            tx.send(partial).unwrap();
        });
    }
    // Drop the original sender so the receiver iterator terminates.
    drop(tx);

    rx.iter().sum()
}

fn main() {
    let data: Vec<u64> = (1..=100).collect();
    let total = parallel_sum(data, 4);
    println!("sum = {total}"); // 5050
    assert_eq!(total, 5050);
}
```

**Output:**

```
sum = 5050
```

Key points: clone `tx` once per thread, `drop(tx)` before collecting so the `rx.iter()` loop ends, and `chunk.to_vec()` so each thread *owns* its data (a borrow could not outlive the function). `div_ceil` rounds the chunk size up so no items are missed.

</details>

### Exercise 2: Bounded pipeline with backpressure

**Difficulty:** Intermediate

**Objective:** Use a bounded `sync_channel` so several producers cannot overwhelm a single collector.

**Instructions:** Spawn 3 producer threads. Each sends 4 messages of the form `(producer_id, n*n)` for `n` in `0..4` into a `sync_channel` with capacity 4. In `main`, collect all messages into a `Vec`, sort them, and print the count (should be 12). Make sure the original sender is dropped so the collector loop terminates.

<details>
<summary>Solution</summary>

```rust playground
use std::sync::mpsc;
use std::thread;

fn main() {
    // Bounded channel: at most 4 items buffered in flight => backpressure.
    let (tx, rx) = mpsc::sync_channel::<(usize, u64)>(4);

    let mut producers = Vec::new();
    for id in 0..3 {
        let tx = tx.clone();
        producers.push(thread::spawn(move || {
            for n in 0..4u64 {
                tx.send((id, n * n)).unwrap(); // blocks if the buffer is full
            }
        }));
    }
    // Drop the original sender; only the clones in the threads remain.
    drop(tx);

    let mut results: Vec<(usize, u64)> = rx.iter().collect();
    results.sort();
    println!("{results:?}");
    println!("count = {}", results.len());

    for p in producers {
        p.join().unwrap();
    }
}
```

**Output:**

```
[(0, 0), (0, 1), (0, 4), (0, 9), (1, 0), (1, 1), (1, 4), (1, 9), (2, 0), (2, 1), (2, 4), (2, 9)]
count = 12
```

With capacity 4 and three producers, sends block whenever four items are already buffered, so memory stays bounded no matter how fast the producers run.

</details>

### Exercise 3: Idle-timeout consumer with `crossbeam-channel`

**Difficulty:** Advanced

**Objective:** Use `crossbeam_channel::select!` with an `after` timer to stop consuming when a producer goes quiet.

**Instructions:** Add `crossbeam-channel`. Spawn a producer that sends `0`, `1`, `2` (with a 30 ms gap between each) and then goes silent. In `main`, loop on `select!` over the data channel and an `after(100ms)` timer: collect each value, but if no value arrives within 100 ms, print an idle-timeout message and break. Print the values you collected (should be `[0, 1, 2]`).

<details>
<summary>Solution</summary>

```bash
cargo add crossbeam-channel
```

```rust playground
use crossbeam_channel::{after, select, unbounded};
use std::thread;
use std::time::Duration;

fn main() {
    let (tx, rx) = unbounded::<u32>();

    thread::spawn(move || {
        for i in 0..3 {
            thread::sleep(Duration::from_millis(30));
            if tx.send(i).is_err() {
                return;
            }
        }
        // Then go quiet, simulating a stalled producer.
        thread::sleep(Duration::from_secs(10));
    });

    let mut received = Vec::new();
    loop {
        select! {
            recv(rx) -> msg => match msg {
                Ok(v) => received.push(v),
                Err(_) => break, // channel closed
            },
            // If no message arrives within 100ms, assume the producer stalled.
            recv(after(Duration::from_millis(100))) -> _ => {
                println!("idle timeout: giving up");
                break;
            }
        }
    }
    println!("received: {received:?}");
}
```

**Output:**

```
idle timeout: giving up
received: [0, 1, 2]
```

`after(d)` returns a receiver that fires a single value once `d` elapses. Because `select!` re-evaluates every arm on each loop iteration, the timer effectively restarts at 100 ms after each message, so the loop survives the three 30 ms gaps but bails out once the producer falls silent. This is the canonical building block for liveness checks and graceful shutdown of long-running consumers.

</details>
