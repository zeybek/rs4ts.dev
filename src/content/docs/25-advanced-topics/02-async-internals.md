---
title: "How Async/Await Works Under the Hood"
description: "A Rust async fn compiles to a Future state machine the runtime polls, suspending and waking via Waker. Compared to JavaScript generators and the event loop."
---

In Section 11 you learned to *use* `async`/`await`. This page opens the hood: an `async fn` is not magic and there is no hidden event loop in the language. It compiles down to an ordinary value (a **state machine**) that implements one small trait called `Future`, and a separate library called a **runtime** repeatedly calls one method on it until it produces a result.

---

## Quick Overview

A Rust `Future` is a plain data structure with a single core method, `poll`, that the runtime calls to ask "are you done yet?". The compiler rewrites every `async fn` and `async` block into an `enum`-shaped state machine where each `.await` point becomes a state; polling resumes from the last suspended state. When a future cannot make progress (waiting on I/O, a timer, a lock), it returns `Poll::Pending` and registers a **`Waker`** so that whatever it is waiting on can later tell the runtime "poll this future again." Understanding this loop (poll, suspend, wake, re-poll) explains nearly every async behavior and performance characteristic in Rust.

> **Note:** Every runnable Rust snippet on this page was compiled and executed with `cargo`/`rustc` 1.96.0 (current stable; 2024 edition). Examples that need a runtime use Tokio (`tokio = { version = "1.52", features = ["full"] }`), one uses the `futures` crate (`futures = "0.3"`), and the real-world example uses `pin-project-lite = "0.2"`. Add them with `cargo add tokio --features full`, `cargo add futures`, and `cargo add pin-project-lite`.

---

## TypeScript/JavaScript Example

In JavaScript you never see the machinery: the **engine** (V8) and the **event loop** (libuv in Node) are built in. When you write an `async function`, the engine internally turns it into a resumable coroutine. A close, hand-visible analogy is a **generator**: each `yield` is a suspension point, and calling `.next()` resumes execution from where it left off. That is almost exactly what the Rust compiler does for you, except in Rust the suspension points are `.await` and *you* (well, the runtime) drive the resumption.

```javascript
// JavaScript (Node v22) — a generator makes the hidden "state machine" visible.
// An async function is conceptually this, with the event loop calling .next() for you.
function makeStateMachine() {
  function* gen() {
    console.log("state A");
    yield; // suspension point #1
    console.log("state B");
    yield; // suspension point #2
    console.log("state C (done)");
  }
  const it = gen();
  return () => it.next(); // each call advances one "poll"
}

const step = makeStateMachine();
console.log("poll 1:", step().done);
console.log("poll 2:", step().done);
console.log("poll 3:", step().done);
```

Running this with Node v22 prints:

```text
state A
poll 1: false
state B
poll 2: false
state C (done)
poll 3: true
```

The function runs in pieces: `.next()` advances it to the next `yield`, returns `{ done: false }`, and the local variables survive across calls. In JavaScript the event loop is the thing calling `.next()` (and `done` is hidden inside the Promise machinery). Rust exposes this same pattern as the `Future` trait, and the thing calling the equivalent of `.next()` is the async **runtime**.

> **Note:** A JavaScript generator is the closest *built-in* analogy, but it is not identical: generators are not `Future`s, JS Promises are eager, and the engine resumes them automatically. The Rust equivalents are lazy and require an explicit runtime — see [Promises vs Futures](/11-async/00-promises-vs-futures/).

---

## Rust Equivalent

Here is the trait at the center of everything, exactly as it appears in `std`:

```rust
// std::future::Future (shown for reference — you do not write this).
pub trait Future {
    type Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}

// std::task::Poll — the result of one poll.
pub enum Poll<T> {
    Ready(T),  // the future finished; here is the value
    Pending,   // not done yet; I've registered to be woken later
}
```

You rarely implement `Future` by hand — you write an `async fn` and the compiler generates the state machine. But writing one *by hand* makes the generated code concrete. Below is the state machine the compiler would produce for this `async fn`:

```rust
// The async fn we are de-sugaring by hand:
//   async fn two_steps() -> u32 {
//       let a = step_one().await; // yields once
//       let b = step_two().await; // yields once
//       a + b
//   }
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

// A leaf future: returns Pending the first time it is polled, Ready the second.
// This stands in for "real" leaf futures like a socket read or a timer.
struct YieldOnce {
    polled: bool,
    value: u32,
}

impl Future for YieldOnce {
    type Output = u32;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        if self.polled {
            Poll::Ready(self.value)
        } else {
            self.polled = true;
            cx.waker().wake_by_ref(); // "I made progress, poll me again soon"
            Poll::Pending
        }
    }
}

// The compiler turns the async fn into an enum of states. Here it is by hand.
// Each variant captures exactly the locals that must survive across an `.await`.
enum TwoSteps {
    Start,
    AwaitingFirst { first: YieldOnce },
    AwaitingSecond { a: u32, second: YieldOnce },
    Done,
}

impl Future for TwoSteps {
    type Output = u32;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        loop {
            match &mut *self {
                TwoSteps::Start => {
                    // Begin: move into the first await's state.
                    *self = TwoSteps::AwaitingFirst {
                        first: YieldOnce { polled: false, value: 10 },
                    };
                }
                TwoSteps::AwaitingFirst { first } => {
                    // `YieldOnce` is Unpin, so Pin::new is sound here.
                    match Pin::new(first).poll(cx) {
                        Poll::Pending => return Poll::Pending, // suspend the whole fn
                        Poll::Ready(a) => {
                            *self = TwoSteps::AwaitingSecond {
                                a,
                                second: YieldOnce { polled: false, value: 32 },
                            };
                        }
                    }
                }
                TwoSteps::AwaitingSecond { a, second } => {
                    let a = *a;
                    match Pin::new(second).poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(b) => {
                            *self = TwoSteps::Done;
                            return Poll::Ready(a + b);
                        }
                    }
                }
                TwoSteps::Done => panic!("polled after completion"),
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let result = TwoSteps::Start.await;
    println!("hand-written state machine produced: {result}");
}
```

Real output:

```text
hand-written state machine produced: 42
```

That enum *is* what `async fn two_steps()` becomes, minus the readable names. Each `.await` is a place where the function can return `Poll::Pending` and later resume. The `match`/`loop` is the resume logic. Notice there is no thread blocking and no callback: just a value that remembers where it left off.

---

## Detailed Explanation

### The `Future` trait has exactly one job

`poll` answers one question: "given a chance to run, can you produce your `Output`?" It returns `Poll::Ready(value)` if yes, or `Poll::Pending` if it had to stop and wait. That is the whole protocol. Everything else (runtimes, executors, `tokio::spawn`, `join!`) is built on top of this one method.

This is the deepest contrast with JavaScript. A `Promise` is *push*-based: it holds callbacks and the engine *pushes* the result into them when ready. A `Future` is *pull*-based: it holds no callbacks; something must *pull* on it by calling `poll`. Nothing happens until someone polls, which is why **Rust futures are lazy** (the opposite of eager JS Promises).

### `Output` is the resolved type

`type Output` is the associated type the future eventually produces. For `async fn foo() -> User`, the generated future has `Output = User`. This is the analog of the `T` in `Promise<T>`.

### `Context` carries the `Waker`

The `cx: &mut Context<'_>` parameter currently carries exactly one thing: a reference to a `Waker`, retrieved with `cx.waker()`. When a future returns `Poll::Pending`, it is responsible for arranging that the waker gets called once progress is possible. In the `YieldOnce` example we call `cx.waker().wake_by_ref()` immediately (so it is re-polled right away); a real socket future would instead hand its waker to the OS event subsystem (epoll/kqueue/IOCP) and return without waking, so the future sleeps until the kernel reports readiness.

### The `.await` de-sugar is a poll loop

When you write `let a = some_future.await;`, the compiler does not block. It generates, in effect:

```rust playground
// What `let value = inner().await;` becomes inside the generated state machine
// (driven here manually to show the loop; normally the runtime drives it).
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

async fn inner() -> u32 { 99 }

fn main() {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(&waker);
    let mut fut = pin!(inner());
    let value = loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => break v,
            Poll::Pending => {
                // In a real state machine, THIS is the point where the
                // enclosing function returns Poll::Pending to ITS caller,
                // saving the current state so it can resume here later.
                continue;
            }
        }
    };
    println!("desugared await produced: {value}");
}
```

Real output:

```text
desugared await produced: 99
```

`Waker::noop()` is a real no-op waker in `std` (stabilized in Rust 1.85) handy for driving a future manually in tests. In production you never write this loop. The runtime does, and instead of `continue` it returns `Pending` all the way up, then re-enters when the waker fires.

### Who calls `poll`? The runtime.

Rust ships *no* executor in the standard library, so a `Future` left alone does nothing. A **runtime** (Tokio, async-std, smol, or `futures::executor`) owns a queue of tasks and a `poll` loop. Here is a complete, minimal single-threaded executor — the same shape Tokio uses, just tiny — so you can see the moving parts: a run queue, a `Waker` that re-enqueues a task, and the poll loop.

```rust playground
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

// A real leaf future: completes after a timer fires on a background thread.
struct TimerFuture {
    shared: Arc<Mutex<SharedState>>,
}

struct SharedState {
    completed: bool,
    waker: Option<Waker>, // who to wake when the timer fires
}

impl Future for TimerFuture {
    type Output = &'static str;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.shared.lock().unwrap();
        if state.completed {
            Poll::Ready("timer done")
        } else {
            // Store the LATEST waker so the timer thread can wake us.
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl TimerFuture {
    fn new(dur: Duration) -> Self {
        let shared = Arc::new(Mutex::new(SharedState { completed: false, waker: None }));
        let thread_shared = Arc::clone(&shared);
        thread::spawn(move || {
            thread::sleep(dur);
            let mut state = thread_shared.lock().unwrap();
            state.completed = true;
            if let Some(waker) = state.waker.take() {
                waker.wake(); // tell the executor: poll this task again
            }
        });
        TimerFuture { shared }
    }
}

// A task is a top-level future plus a handle to re-enqueue itself.
struct Task {
    future: Mutex<Option<Pin<Box<dyn Future<Output = ()> + Send>>>>,
    task_sender: SyncSender<Arc<Task>>,
}

// Implementing std::task::Wake gives us a Waker for free via Waker::from(Arc<Task>).
impl std::task::Wake for Task {
    fn wake(self: Arc<Self>) {
        let _ = self.task_sender.send(self.clone()); // reschedule
    }
}

struct MiniExecutor {
    ready_queue: Receiver<Arc<Task>>,
}

#[derive(Clone)]
struct Spawner {
    task_sender: SyncSender<Arc<Task>>,
}

impl Spawner {
    fn spawn(&self, fut: impl Future<Output = ()> + Send + 'static) {
        let task = Arc::new(Task {
            future: Mutex::new(Some(Box::pin(fut))),
            task_sender: self.task_sender.clone(),
        });
        self.task_sender.send(task).unwrap();
    }
}

impl MiniExecutor {
    fn run(&self) {
        // The executor's heartbeat: take a ready task, poll it once.
        while let Ok(task) = self.ready_queue.recv() {
            let mut slot = task.future.lock().unwrap();
            if let Some(mut fut) = slot.take() {
                let waker = Waker::from(task.clone());
                let mut cx = Context::from_waker(&waker);
                match fut.as_mut().poll(&mut cx) {
                    Poll::Pending => *slot = Some(fut), // keep it; it'll be re-sent on wake
                    Poll::Ready(()) => {}               // done; drop the future
                }
            }
        }
    }
}

fn new_executor_and_spawner() -> (MiniExecutor, Spawner) {
    let (task_sender, ready_queue) = sync_channel(1024);
    (MiniExecutor { ready_queue }, Spawner { task_sender })
}

fn main() {
    let (executor, spawner) = new_executor_and_spawner();
    spawner.spawn(async {
        println!("task: start, awaiting timer...");
        let msg = TimerFuture::new(Duration::from_millis(50)).await;
        println!("task: timer returned {msg:?}");
    });
    drop(spawner); // close the channel so run() exits after tasks finish
    executor.run();
    println!("executor: all tasks complete");
}
```

Real output:

```text
task: start, awaiting timer...
task: timer returned "timer done"
executor: all tasks complete
```

Trace the flow: the executor polls the task → the `TimerFuture` is not done, so it stores the waker and returns `Pending` → the executor parks (its `recv()` blocks) → 50 ms later the timer thread sets `completed` and calls `waker.wake()`, which sends the task back onto the queue → the executor wakes, polls again → `Ready`. **No CPU was spinning during the wait.** That single wake-and-re-poll cycle is the essence of every async runtime.

### Why a state machine instead of a thread?

Each suspended `async fn` is a value just big enough to hold its live locals at its widest `.await` point: typically tens to a few hundred bytes. A thread needs a full OS stack (often megabytes) and a context switch to suspend. Millions of pending futures cost a few hundred megabytes of heap; millions of threads are impossible. This is why async scales to huge connection counts. The trade-off is the topic of [Async vs Sync](/11-async/13-async-vs-sync/).

---

## Key Differences

| Concept | JavaScript `Promise` | Rust `Future` |
| --- | --- | --- |
| Model | Push (callbacks invoked for you) | Pull (`poll` called on it) |
| When work starts | Eagerly, at creation | Lazily, only when polled |
| Built-in driver | Yes — the engine's event loop | No — bring a runtime (Tokio, etc.) |
| Suspension mechanism | Engine resumes the coroutine | `Poll::Pending` + a `Waker` |
| Cancellation | Hard (Promises can't be cancelled) | Drop the future — it stops being polled |
| Cost per pending op | Heap object + microtask queue entry | A small state-machine value (no thread) |
| Compiled form | Engine-internal coroutine | A concrete `enum`/struct generated at compile time |

> **Tip:** Two consequences fall straight out of "pull, not push." First, **dropping a future cancels it**: no one polls it again, so its remaining work never runs (a clean, structured cancellation JavaScript lacks). Second, an `async fn` you never `.await` and never hand to a runtime simply never executes — the compiler even warns you about the unused future.

### `Waker`: the "call me back" handle

A `Waker` is a type-erased, cheaply-cloneable handle (an `Arc`-like fat pointer to a vtable). Its only meaningful method is `wake()` (consuming) / `wake_by_ref()` (borrowing). Whoever a future is waiting on — a timer thread, an epoll reactor, another task — keeps the waker and calls it when progress is possible. The waker's job is *not* to run the future; it is to put the future's task back on the runtime's ready queue. This indirection is what lets the same future run unchanged on Tokio, smol, or your 80-line `MiniExecutor` above.

---

## Common Pitfalls

### Pitfall 1: Trying to call `poll` on a future you own

`poll` takes `self: Pin<&mut Self>`, not `&mut self`. You cannot call it on a plain owned or borrowed future; you must pin it first. TypeScript developers expecting `future.poll(cx)` to "just work" hit this:

```rust
use std::future::Future;
use std::task::{Context, Waker};

async fn work() -> i32 { 42 }

fn main() {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(&waker);
    let mut fut = work();
    // does not compile (E0599): poll() needs Pin<&mut Self>, not a bare &mut.
    let _ = fut.poll(&mut cx);
}
```

The real compiler error is informative — it even points you at pinning:

```text
error[E0599]: no method named `poll` found for opaque type `impl Future<Output = i32>` in the current scope
  --> src/main.rs:11:17
   |
11 |     let _ = fut.poll(&mut cx);
   |                 ^^^^ method not found in `impl Future<Output = i32>`
   |
   = help: method `poll` found on `Pin<&mut impl Future<Output = i32>>`, see documentation for `std::pin::Pin`
   = help: self type must be pinned to call `Future::poll`, see https://rust-lang.github.io/async-book/04_pinning/01_chapter.html#pinning-in-practice
help: consider pinning the expression
   |
11 ~     let mut pinned = std::pin::pin!(fut);
12 ~     let _ = pinned.as_mut().poll(&mut cx);
```

The fix is to pin it with the `std::pin::pin!` macro (stack pinning) or `Box::pin` (heap pinning), then call `poll` on `Pin<&mut _>`. *Why* `poll` requires `Pin` (because the generated state machine can be self-referential) is the entire subject of [Pin and Unpin](/25-advanced-topics/01-pin-unpin/).

### Pitfall 2: Returning `Pending` without registering a waker

If your `poll` returns `Poll::Pending` but never stores `cx.waker()` anywhere that will call it, the future **hangs forever**: the runtime parks the task and nothing ever re-enqueues it. There is no compiler error — it is a runtime deadlock. The rule is: *every `Pending` path must ensure the current waker will eventually be woken.* In our `TimerFuture` we store the waker on every poll; doing it only on the first poll would be a subtle bug, because the runtime may poll with a *new* waker after a task is moved between threads.

### Pitfall 3: Blocking the executor inside `poll` / an `async fn`

`poll` must return quickly. If you do CPU-heavy work or call a *blocking* API (`std::fs::read`, `std::thread::sleep`, a synchronous DB driver) inside an `async fn`, you stall the executor thread and starve every other task scheduled on it: the async analog of blocking the JavaScript event loop, but worse, because one Tokio worker may be driving thousands of tasks. Use the async equivalents (`tokio::fs`, `tokio::time::sleep`) or move blocking work to `tokio::task::spawn_blocking`.

### Pitfall 4: Assuming `.await` yields to other tasks

`.await` only yields control if the awaited future returns `Pending`. A future that is always immediately `Ready` (a tight loop of `async { 1 }.await`) never hands the executor a chance to run anything else. If you need to cooperatively yield in a long-running async loop, call `tokio::task::yield_now().await` explicitly.

---

## Best Practices

- **Let the compiler write your state machines.** Write `async fn`; hand-implementing `Future` is for library authors building leaf futures (timers, I/O sources) or combinators. Reach for it only when you genuinely need custom `poll` behavior.
- **When you must implement `Future`, prefer `pin-project` / `pin-project-lite`** to safely access pinned fields, instead of writing `unsafe` `Pin` projections by hand. See the real-world example below.
- **Always re-register the current waker on every `Pending`.** Store `cx.waker().clone()` (or compare-and-update); never assume the waker from a previous poll is still valid.
- **Use combinators and `poll_fn` for one-off futures.** `std::future::poll_fn(|cx| ...)` builds a `Future` from a closure that returns `Poll`, avoiding a whole boilerplate struct.
- **Keep `poll` non-blocking and fast.** Offload CPU work with `spawn_blocking` or `rayon`; use async I/O for I/O.
- **Reach for the right runtime.** Tokio for almost everything; `futures::executor::block_on` for tests, examples, and synchronous edges where you just need to drive one future to completion.

```rust
// `poll_fn` builds a Future from a closure — no custom struct needed.
use std::future::poll_fn;
use std::task::Poll;

fn main() {
    let mut count = 0;
    let fut = poll_fn(move |cx| {
        count += 1;
        if count < 3 {
            cx.waker().wake_by_ref(); // re-poll soon
            println!("poll #{count}: Pending");
            Poll::Pending
        } else {
            println!("poll #{count}: Ready");
            Poll::Ready(count)
        }
    });
    // block_on drives the future on the current thread until it completes.
    let total = futures::executor::block_on(fut);
    println!("future completed after {total} polls");
}
```

Real output:

```text
poll #1: Pending
poll #2: Pending
poll #3: Ready
future completed after 3 polls
```

---

## Real-World Example

A common production need: instrument a future to see how often it is polled. A future polled hundreds of times before completing is a red flag — it may be waking spuriously (re-enqueuing itself without real progress) and burning CPU. The idiomatic way to wrap another future and add behavior around its `poll` is a combinator future. Because it stores the inner future by value, we use `pin-project-lite` to safely get a `Pin<&mut Inner>` for the inner field (the safe alternative to hand-written `unsafe` pin projection, covered in [Pin and Unpin](/25-advanced-topics/01-pin-unpin/)).

```rust playground
// Cargo.toml:
//   tokio = { version = "1.52", features = ["full"] }
//   pin-project-lite = "0.2"
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use pin_project_lite::pin_project;

pin_project! {
    /// Wraps any future and counts how many times it is polled before completing.
    /// Drop this around a suspicious future to diagnose spurious wakeups.
    struct PollCounter<F> {
        #[pin]
        inner: F,
        polls: u32,
        label: &'static str,
    }
}

impl<F: Future> PollCounter<F> {
    fn new(label: &'static str, inner: F) -> Self {
        PollCounter { inner, polls: 0, label }
    }
}

impl<F: Future> Future for PollCounter<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = self.project();        // safe pinned access to fields
        *this.polls += 1;
        let n = *this.polls;
        match this.inner.poll(cx) {        // delegate to the wrapped future
            Poll::Pending => {
                println!("[{}] poll #{n} -> Pending", this.label);
                Poll::Pending
            }
            Poll::Ready(v) => {
                println!("[{}] poll #{n} -> Ready (took {n} polls)", this.label);
                Poll::Ready(v)
            }
        }
    }
}

async fn fetch_user(id: u32) -> String {
    // Two awaits => the inner future suspends (Pending) more than once.
    tokio::time::sleep(Duration::from_millis(10)).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    format!("user#{id}")
}

#[tokio::main]
async fn main() {
    let name = PollCounter::new("fetch_user", fetch_user(7)).await;
    println!("result: {name}");
}
```

Real output:

```text
[fetch_user] poll #1 -> Pending
[fetch_user] poll #2 -> Pending
[fetch_user] poll #3 -> Ready (took 3 polls)
result: user#7
```

The wrapper is transparent (`fetch_user` runs unchanged) yet you can now see it suspends twice (once per `sleep`) and completes on the third poll. This same delegation pattern is exactly how Tokio's `Timeout`, the `futures` crate's `Map`/`Then`, and tracing instrumentation like `tracing-futures` are built: a struct that holds an inner future, projects to it with `Pin`, and adds logic around the `poll` call.

---

## Further Reading

### Official Documentation

- [`std::future::Future`](https://doc.rust-lang.org/std/future/trait.Future.html): the trait, `poll`, and `Output`
- [`std::task::Poll`](https://doc.rust-lang.org/std/task/enum.Poll.html), [`Context`](https://doc.rust-lang.org/std/task/struct.Context.html), [`Waker`](https://doc.rust-lang.org/std/task/struct.Waker.html), and [`Wake`](https://doc.rust-lang.org/std/task/trait.Wake.html)
- [`std::future::poll_fn`](https://doc.rust-lang.org/std/future/fn.poll_fn.html): build a future from a closure
- [The Rust Book — Futures, Tasks, and the `Future` trait](https://doc.rust-lang.org/book/ch17-00-async-await.html)
- [Asynchronous Programming in Rust ("async book") — Under the Hood: Executing Futures and Tasks](https://rust-lang.github.io/async-book/02_execution/01_chapter.html) — the original `TimerFuture`/`MiniExecutor` walkthrough this page builds on
- [Tokio internals — the runtime and scheduler](https://tokio.rs/tokio/tutorial)

### Related Sections in This Guide

- [Pin and Unpin](/25-advanced-topics/01-pin-unpin/) — *why* `poll` takes `Pin<&mut Self>`: self-referential state machines
- [`PhantomData` and Zero-Sized Types](/25-advanced-topics/00-phantom-data/): zero-sized markers; relevant to building `Send`/`Sync`-correct futures
- [Generic Associated Types (GATs)](/25-advanced-topics/06-gat/): Generic Associated Types, which power lending streams and async traits
- [Section 11: Async — async/await syntax](/11-async/01-async-await/) — using `async`/`await` (start here if internals feel deep)
- [Section 11: Async — Promises vs Futures](/11-async/00-promises-vs-futures/): eager vs lazy, the runtime requirement
- [Section 11: Async — Tokio intro](/11-async/02-tokio-intro/): the runtime that calls `poll` for you
- [Section 11: Async — async vs sync](/11-async/13-async-vs-sync/): when this machinery is worth it
- [Section 00: Introduction](/00-introduction/), [Section 01: Getting Started](/01-getting-started/), [Section 02: Basics](/02-basics/) — toolchain and fundamentals these examples assume
- [Section 26: Systems Programming](/26-systems-programming/): where bring-your-own-runtime and `no_std` async live

---

## Exercises

### Exercise 1

**Difficulty:** Beginner

**Objective:** Implement `Future` by hand to feel the poll loop.

**Instructions:** Write a struct `Countdown { remaining: u32 }` that implements `Future<Output = &'static str>`. On each poll, if `remaining == 0` return `Poll::Ready("liftoff")`; otherwise print `T-minus {remaining}`, decrement it, wake the current waker so it is polled again, and return `Poll::Pending`. Drive it with `#[tokio::main]` and `.await`, starting from `remaining: 3`.

<details>
<summary>Solution</summary>

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

struct Countdown {
    remaining: u32,
}

impl Future for Countdown {
    type Output = &'static str;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
        if self.remaining == 0 {
            Poll::Ready("liftoff")
        } else {
            println!("T-minus {}", self.remaining);
            self.remaining -= 1;
            cx.waker().wake_by_ref(); // ask to be polled again
            Poll::Pending
        }
    }
}

#[tokio::main]
async fn main() {
    let msg = Countdown { remaining: 3 }.await;
    println!("{msg}");
}
```

Real output:

```text
T-minus 3
T-minus 2
T-minus 1
liftoff
```

</details>

### Exercise 2

**Difficulty:** Intermediate

**Objective:** Build a combinator future that polls two futures and returns whichever finishes first: a hand-rolled `select`/`Promise.race`.

**Instructions:** Write `struct Race<A, B> { a: A, b: B }` implementing `Future<Output = T>` where `A: Future<Output = T> + Unpin` and `B: Future<Output = T> + Unpin`. On each poll, poll `a`; if it is `Ready`, return its value; otherwise poll `b` and return its value if `Ready`; if both are `Pending`, return `Pending`. Test it with a small `ReadyAfter { polls_left, value }` leaf future and confirm the one that becomes ready sooner wins. (The `Unpin` bound lets you use `Pin::new(&mut self.a)` safely.)

<details>
<summary>Solution</summary>

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Polls two futures of the same output type; returns whichever is Ready first.
struct Race<A, B> {
    a: A,
    b: B,
}

impl<A, B, T> Future for Race<A, B>
where
    A: Future<Output = T> + Unpin,
    B: Future<Output = T> + Unpin,
{
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        if let Poll::Ready(v) = Pin::new(&mut self.a).poll(cx) {
            return Poll::Ready(v);
        }
        if let Poll::Ready(v) = Pin::new(&mut self.b).poll(cx) {
            return Poll::Ready(v);
        }
        Poll::Pending
    }
}

struct ReadyAfter {
    polls_left: u32,
    value: u32,
}

impl Future for ReadyAfter {
    type Output = u32;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        if self.polls_left == 0 {
            Poll::Ready(self.value)
        } else {
            self.polls_left -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[tokio::main]
async fn main() {
    let winner = Race {
        a: ReadyAfter { polls_left: 3, value: 1 }, // ready later
        b: ReadyAfter { polls_left: 1, value: 2 }, // ready sooner -> wins
    }
    .await;
    println!("winner: {winner}");
}
```

Real output:

```text
winner: 2
```

> **Note:** A production `select!` is fairer (it does not always poll `a` first) and handles non-`Unpin` futures via pinning; this exercise version is deliberately simplified to focus on the poll mechanics.

</details>

### Exercise 3

**Difficulty:** Advanced

**Objective:** Wire up a `Waker` across tasks: build a one-shot event that one task waits on and another task fires, exactly as a real synchronization primitive does.

**Instructions:** Build an `Event` backed by `Arc<Mutex<{ ready: bool, waker: Option<Waker> }>>`. Give it `fire(&self)` (set `ready = true` and call any stored waker) and `wait(&self) -> impl Future<Output = ()>`. The wait-future's `poll` returns `Ready(())` when `ready`, otherwise stores `cx.waker().clone()` and returns `Pending`. Spawn a task that `.await`s `event.wait()`, sleep briefly in `main`, then `event.fire()` and confirm the waiter resumes.

<details>
<summary>Solution</summary>

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Default)]
struct EventInner {
    ready: bool,
    waker: Option<Waker>,
}

#[derive(Clone, Default)]
struct Event {
    inner: Arc<Mutex<EventInner>>,
}

impl Event {
    fn fire(&self) {
        let mut g = self.inner.lock().unwrap();
        g.ready = true;
        if let Some(w) = g.waker.take() {
            w.wake(); // re-enqueue the waiting task
        }
    }

    fn wait(&self) -> EventWait {
        EventWait { ev: self.clone() }
    }
}

struct EventWait {
    ev: Event,
}

impl Future for EventWait {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut g = self.ev.inner.lock().unwrap();
        if g.ready {
            Poll::Ready(())
        } else {
            // Always store the LATEST waker; the task may have moved threads.
            g.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

#[tokio::main]
async fn main() {
    let event = Event::default();
    let waiter = {
        let event = event.clone();
        tokio::spawn(async move {
            println!("waiter: blocking on event");
            event.wait().await;
            println!("waiter: event fired, resuming");
        })
    };

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    println!("main: firing event");
    event.fire();
    waiter.await.unwrap();
}
```

Real output:

```text
waiter: blocking on event
main: firing event
waiter: event fired, resuming
```

This is a minimal version of `tokio::sync::Notify` / a one-shot channel: the waiter parks (no busy-waiting), and `fire()` wakes it through the stored `Waker`. Re-storing the waker on every `Pending` poll is what makes it correct under Tokio's multi-threaded scheduler.

</details>
