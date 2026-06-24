---
title: "Rust Systems Programming"
sidebar:
  label: "Overview"
description: "Threads, Rayon, channels, atomics, files, sockets, signals, and subprocesses in Rust, mapped for a TypeScript developer used to one event loop."
---

This section descends from high-level application code to the systems layer, where Rust replaces C and C++ for a TypeScript/JavaScript developer who has only ever had a single-threaded event loop. You will spawn real OS threads, fan work across every CPU core with Rayon, pass messages over channels, reach for lock-free atomics with explicit memory ordering, and work directly with the file system, raw sockets, Unix signals, and child processes. Throughout, the recurring theme is **fearless concurrency**: the same ownership rules that protect a single thread also turn data races into compile errors, so code that would segfault in C++ or silently corrupt a `SharedArrayBuffer` in JavaScript simply does not build.

---

## What You'll Learn

- Spawning, joining, and scoping native OS threads with `std::thread`, and how they differ from JavaScript Web Workers.
- Reusing a fixed pool of worker threads with Rayon, including the global pool, `rayon::join`, and custom pools.
- Turning sequential iterators into parallel ones with `par_iter()`/`par_bridge()` — and judging when parallelism actually pays off.
- Passing values between threads safely over `std::sync::mpsc` and `crossbeam-channel`, with backpressure and `select!`.
- Sharing counters and flags without a lock using atomic types (`AtomicUsize`, `AtomicBool`) and compare-and-swap.
- Choosing the right memory ordering (`Relaxed`/`Acquire`/`Release`/`AcqRel`/`SeqCst`) and understanding what each one guarantees.
- Going beyond `read_to_string`: metadata, Unix permission bits, symlinks, directory walking, and memory-mapping.
- Building blocking TCP and UDP servers and clients with `std::net`.
- Catching `SIGINT`/`SIGTERM` for clean shutdown with `ctrlc` and `signal-hook`.
- Spawning and managing subprocesses with `std::process::Command` — arguments, environment, pipes, and exit status.

---

## Topics

| Topic | Description |
| --- | --- |
| [Native Threads](/26-systems-programming/00-threads/) | `std::thread`: spawn/join, `move` closures, and scoped threads (`std::thread::scope`), versus JavaScript Web Workers. |
| [Thread Pools with Rayon](/26-systems-programming/01-thread-pools/) | Reusing worker threads with Rayon: the global pool, `rayon::join`, and custom `ThreadPoolBuilder` pools. |
| [Parallel Iterators](/26-systems-programming/02-parallel-iterators/) | Data parallelism with `par_iter()`/`par_bridge()`; when it helps and when it does not. |
| [Channels](/26-systems-programming/03-channels/) | `std::sync::mpsc` (and `crossbeam-channel`): producer/consumer message passing across threads. |
| [Atomic Operations](/26-systems-programming/04-atomic-operations/) | Atomic types (`AtomicUsize`/`AtomicBool`): `load`/`store`, `fetch_add`, and `compare_exchange`. |
| [Memory Ordering](/26-systems-programming/05-memory-ordering/) | `Relaxed`/`Acquire`/`Release`/`AcqRel`/`SeqCst` — what each guarantees, with runnable examples. |
| [Advanced File-System Operations](/26-systems-programming/06-file-system/) | Metadata, permissions, symlinks, directory walking with `walkdir`, and memory-mapping with `memmap2`. |
| [Low-Level Networking](/26-systems-programming/07-networking/) | `std::net`: `TcpListener`/`TcpStream`/`UdpSocket` and a tiny echo server. |
| [Signal Handling](/26-systems-programming/08-signals/) | Clean shutdown on `SIGINT`/`SIGTERM` with `ctrlc` and `signal-hook`. |
| [Process Management](/26-systems-programming/09-process-management/) | Spawning and managing subprocesses with `std::process::Command`: args, env, pipes, and status. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Decide when to use raw threads, a Rayon pool, channels, or atomics for a given concurrency problem.
- Spawn threads that either own (`move`) or borrow (`thread::scope`) their data, and join them to collect results or detect panics.
- Parallelize a CPU-bound pipeline by changing `iter()` to `par_iter()`, and benchmark it honestly in `--release`.
- Build a producer/consumer pipeline with bounded channels and reason about who holds the last sender.
- Implement a lock-free counter, flag, or CAS loop and pick the weakest correct memory ordering.
- Inspect file metadata and permissions, follow (or refuse to follow) symlinks, walk a tree while pruning subdirectories, and memory-map a file safely.
- Write a blocking TCP/UDP server that frames messages correctly and handles errors by `ErrorKind`.
- Turn `SIGINT`/`SIGTERM` into an orderly shutdown by flipping an `AtomicBool` and polling it.
- Launch external programs, capture or stream their output, and distinguish "could not start" from "ran but failed."

---

## Prerequisites

This is an advanced section. Before starting, you should be comfortable with:

- [Section 05: Ownership](/05-ownership/) — moves, borrows, and lifetimes are the foundation of `Send`/`Sync` and scoped threads.
- [Section 10: Smart Pointers](/10-smart-pointers/) — `Arc`, `Mutex`, and interior mutability underpin shared state across threads.
- [Section 11: Async Programming](/11-async/) — async tasks are *not* threads; knowing the difference clarifies when to use each.

---

## Estimated Time

**14 hours** — roughly 8 hours of reading and worked examples plus 6 hours on the exercises. Threads, channels, and atomics reward hands-on experimentation, so budget time to run the code and watch the non-deterministic interleavings yourself.

---

**Next:** [Section 27: Security →](/27-security/)
