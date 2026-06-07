---
title: "Async"
sidebar:
  label: "Overview"
description: "Map every TypeScript async habit to Rust: a Promise is eager, but a Future is lazy and runs nothing until awaited on a runtime like Tokio."
---

In TypeScript you reach for `async`/`await` and `Promise` constantly, and the Node.js event loop runs them for you, invisibly, on a single thread. Rust has the same `async`/`await` *syntax*, but the model underneath differs in one decisive way: a Rust **`Future` is lazy**. Where a JavaScript `Promise` starts running the moment you create it, a Rust future does *nothing* until you `.await` it (or hand it to a runtime), and Rust ships **no built-in runtime**: you choose one, almost always **Tokio**. This section maps every async habit you have — `Promise.all`, `Promise.race`, async iterators, shared mutable state — onto its idiomatic, compile-verified Rust counterpart, and is careful to flag exactly where the analogy with JavaScript breaks down.

---

## What You'll Learn

- The single most important shift: a `Promise` is **eager** (runs on creation) while a `Future` is **lazy** (runs only when awaited/polled), and why that means Rust needs an explicit runtime
- How `async`/`await` syntax maps over: an `async fn` returns `impl Future`, `.await` replaces `await`, and `?` still propagates errors
- Why Node's single-threaded event loop becomes the **Tokio** runtime, and the difference between the multi-thread and current-thread schedulers
- How to set up a Tokio project (`#[tokio::main]`, `features = ["full"]`); all examples compile-verified against current Tokio
- That **`async fn` in traits is native and stable** (since Rust 1.75) — you only need the `async-trait` crate for `dyn Trait` (dynamic dispatch)
- How `AsyncIterator` becomes the **`Stream`** trait, and how to consume one with `StreamExt`/`while let`
- How `Promise.race`/`Promise.all` become `tokio::select!` / `join!` / `try_join!`
- The async channel family (`mpsc`, `oneshot`, `broadcast`, `watch`) for moving data between tasks
- How `tokio::spawn` relates to firing off a `Promise`, what a `JoinHandle` is, and when to use `spawn_blocking`
- Concurrency vs parallelism, tasks vs OS threads, and the **`Arc<Mutex<T>>`** pattern for shared mutable state across tasks
- When async is the wrong tool — CPU-bound work, the "function coloring" problem, and choosing threads instead

---

## Topics

| Topic | Description |
| --- | --- |
| [Promises vs Futures](/11-async/00-promises-vs-futures/) | The key difference: eager JS `Promise` vs **lazy** Rust `Future`, which does nothing until awaited and needs a runtime. |
| [Async/Await](/11-async/01-async-await/) | `async`/`await` syntax: an `async fn` returns `impl Future`, `.await`, and error handling with `?`. |
| [Tokio Intro](/11-async/02-tokio-intro/) | Node's event loop → the Tokio runtime; why Rust needs an explicit runtime; multi-thread vs current-thread schedulers. |
| [Tokio Setup](/11-async/03-tokio-setup/) | Adding Tokio (`features = ["full"]`), `#[tokio::main]`, and the runtime builder. |
| [Async Functions & Blocks](/11-async/04-async-functions/) | `async fn` and `async` blocks, capturing, returning futures, and lifetimes in async code. |
| [Async in Traits](/11-async/05-async-trait/) | Native `async fn` in traits (stable since 1.75) and when you still need the `async-trait` crate (for `dyn`). |
| [Streams](/11-async/06-streams/) | `AsyncIterator` → the `Stream` trait; consuming with `StreamExt` / `while let`. |
| [select! and join!](/11-async/07-select-join/) | `Promise.race`/`Promise.all` → `tokio::select!` / `join!` / `try_join!`. |
| [Channels](/11-async/08-channels/) | Async channels: `mpsc` / `oneshot` / `broadcast` / `watch`, and how they differ from `std::sync::mpsc`. |
| [Spawning Tasks](/11-async/09-spawning-tasks/) | `tokio::spawn`, `JoinHandle`, tasks vs OS threads, and `spawn_blocking` for blocking work. |
| [Concurrency](/11-async/10-concurrency/) | Concurrency vs parallelism, tasks vs threads, structured patterns, and cancellation. |
| [Synchronization Primitives](/11-async/11-sync-primitives/) | Tokio `Mutex`/`RwLock`/`Semaphore` vs their `std` versions, and the danger of holding a lock across `.await`. |
| [The Arc&lt;Mutex&lt;T&gt;&gt; Pattern](/11-async/12-arc-mutex-pattern/) | Sharing mutable state across tasks with `Arc<Mutex<T>>` / `Arc<RwLock<T>>`. |
| [Async vs Sync](/11-async/13-async-vs-sync/) | When to use async vs threads vs blocking: CPU-bound vs IO-bound, and the "function coloring" issue. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Explain why a Rust `Future` does nothing until awaited, and contrast that with an eager JavaScript `Promise`
- Set up and run a Tokio application, choosing the right scheduler and feature flags
- Write `async fn`s, await them, propagate errors with `?`, and put async methods in traits without reaching for a crate unless you need `dyn`
- Run work concurrently with `join!`/`try_join!`, race with `select!`, and move data between tasks over the right kind of channel
- Spawn tasks, await their `JoinHandle`s, and offload blocking/CPU-bound work with `spawn_blocking` or a thread
- Share mutable state safely across tasks with `Arc<Mutex<T>>`, and avoid holding a lock across an `.await`
- Decide deliberately between async, threads, and plain blocking code for a given workload

---

## Prerequisites

- [Section 08: Error Handling](/08-error-handling/). Async code is full of `Result` and `?`; fallible `.await`s propagate errors exactly like synchronous ones.
- [Section 09: Generics & Traits](/09-generics-traits/) — a `Future` is a trait, `async fn` returns `impl Future`, and `Send`/`Sync` bounds decide what can cross task boundaries.
- [Section 10: Smart Pointers](/10-smart-pointers/). Shared async state is built from `Arc` plus a `Mutex`/`RwLock`, so be comfortable with reference counting and interior mutability first.

---

## Estimated Time

- **Reading:** 6-7 hours
- **Hands-on Practice:** 5-6 hours
- **Exercises:** 3 hours
- **Total:** 14-16 hours

> **Tip:** Read in order. Start with `promises-vs-futures` and `async-await` to internalize **laziness** — the one idea that trips up every JavaScript developer — then set up Tokio before moving on to streams, `select!`/`join!`, channels, and shared state. Save `async-vs-sync` for last: once you understand the machinery, you can judge when *not* to use it.


---

## Frequently asked questions

### Is a Rust `Future` the same as a Promise?

Almost. Both represent a value that arrives later, but a `Promise` starts running the moment it is created, while a `Future` is lazy and does nothing until you `.await` it or hand it to a runtime. See [Promises vs Futures](/11-async/00-promises-vs-futures/).

### Why do I need Tokio?

Rust ships no built-in event loop, so a future needs a runtime to poll it to completion. Tokio is the common choice, started with `#[tokio::main]`. This is the main setup difference from Node, where the loop is always running. See [Tokio Intro](/11-async/02-tokio-intro/).

### What replaces `Promise.all` and `Promise.race`?

`tokio::join!(a, b)` runs futures concurrently and waits for all of them, like `Promise.all`. `tokio::select!` returns as soon as the first one finishes, like `Promise.race`. See [select and join](/11-async/07-select-join/).

---

**Next:** [Section 12: Modules & Packages →](/12-modules-packages/). Organizing code with modules and managing dependencies with Cargo, the equivalents of ES modules and npm.
