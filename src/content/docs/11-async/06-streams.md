---
title: "Streams: Async Iterators in Rust"
description: "A Stream is Rust's async iterator, the cousin of TypeScript's for await...of. Build streams, use StreamExt combinators, pin them, and consume with while let."
---

A **stream** is the asynchronous cousin of an iterator: a sequence of values produced over time, where pulling the next value is an `.await` rather than a blocking call. If you have reached for `for await...of` over an `AsyncIterable` in TypeScript — paging through an API, reading lines from a socket, draining a queue — a Rust `Stream` is the tool you want.

---

## Quick Overview

A **`Stream`** yields items one at a time, and obtaining each item may require waiting (for I/O, a timer, a channel message). It is exactly `Iterator`, but every `next` is asynchronous. JavaScript spells this `AsyncIterator`/`AsyncIterable` and consumes it with `for await...of`; Rust has the `Stream` trait, the `.next().await` method, and the `while let Some(x) = s.next().await` loop. This page covers the `Stream` trait, how to get a stream (`tokio_stream::iter`, the `async-stream` crate, channel wrappers), the iterator-style combinators on `StreamExt`, and the consumption patterns.

> **Note:** `Stream` is **not** in the standard library's prelude the way `Iterator` is. As of the latest stable edition (2024) on Rust 1.96.0, `std::async_iter::AsyncIterator` exists only on nightly and is unstable; real code uses the `Stream` trait from the [`futures`](https://docs.rs/futures) crate, re-exported by [`tokio-stream`](https://docs.rs/tokio-stream). Every runnable snippet here was compiled and run with `rustc`/`cargo` 1.96.0 against the 2024 edition using `tokio = { version = "1.52", features = ["full"] }`, `tokio-stream = "0.1"`, `futures = "0.3"`, and `async-stream = "0.3"`.

---

## TypeScript/JavaScript Example

In TypeScript, an `async function*` (async generator) produces an `AsyncIterableIterator`. Each `yield` hands back a value; `await` points inside the generator suspend it until work completes. You consume it with `for await...of`, which calls the iterator's `next()` and awaits each promised result.

```typescript
// An async generator: each `yield` is a value that arrives over time.
async function* fetchPages(total: number): AsyncGenerator<string[]> {
  for (let page = 1; page <= total; page++) {
    // Simulate a network round-trip per page.
    await new Promise((resolve) => setTimeout(resolve, 10));
    yield [`p${page}-item0`, `p${page}-item1`, `p${page}-item2`];
  }
}

async function main(): Promise<void> {
  const allItems: string[] = [];

  // `for await...of` pulls one page at a time, awaiting each.
  for await (const items of fetchPages(3)) {
    console.log(`fetched a page with ${items.length} items`);
    allItems.push(...items);
  }

  console.log(`total items: ${allItems.length}`);
}

await main();
```

Key properties of the JavaScript model, to contrast against Rust:

- The generator is **lazy** in the sense that it only advances when the consumer asks for the next value; already similar to Rust here.
- But the *consumer* drives it via the built-in event loop; there is no separate runtime to install.
- `for await...of` is the one canonical consumption form.

---

## Rust Equivalent

The same paginated fetch as a Rust `Stream`, consumed with `while let`:

```rust playground
use std::time::Duration;
use tokio::time::sleep;
use tokio_stream::StreamExt;

#[derive(Debug)]
struct Page {
    number: u32,
    items: Vec<String>,
}

// Fetching one page costs an async round-trip.
async fn fetch_page(number: u32) -> Page {
    sleep(Duration::from_millis(10)).await; // simulate network latency
    let items = (0..3)
        .map(|i| format!("p{number}-item{i}"))
        .collect::<Vec<_>>();
    Page { number, items }
}

// Expose pages 1..=total as a `Stream<Item = Page>`, fetched lazily.
fn page_stream(total: u32) -> impl tokio_stream::Stream<Item = Page> {
    // `then` maps each page number to a future and awaits it in order.
    tokio_stream::iter(1..=total).then(fetch_page)
}

#[tokio::main]
async fn main() {
    let mut all_items = Vec::new();

    // A `then`-based stream holds a future and is not `Unpin`, so pin it.
    let pages = page_stream(3);
    tokio::pin!(pages);

    // `while let Some(x) = s.next().await` is Rust's `for await...of`.
    while let Some(page) = pages.next().await {
        println!("fetched page {} with {} items", page.number, page.items.len());
        all_items.extend(page.items);
    }

    println!("total items: {}", all_items.len());
}
```

Real output:

```text
fetched page 1 with 3 items
fetched page 2 with 3 items
fetched page 3 with 3 items
total items: 9
```

The shape mirrors the TypeScript: produce a sequence asynchronously, consume one item at a time. The differences are the explicit runtime (`#[tokio::main]`), the `StreamExt` import that enables `.next()`/`.then()`, and the `tokio::pin!`; all explained below.

---

## Detailed Explanation

### The `Stream` trait is `Iterator` with `poll_next`

A synchronous iterator has one required method:

```rust
// From the standard library (simplified).
// trait Iterator {
//     type Item;
//     fn next(&mut self) -> Option<Self::Item>;
// }
```

A `Stream` has the async analogue. Instead of returning `Option<Item>` immediately, it returns a `Poll` so the runtime can be told "not ready yet, suspend me":

```rust
// From the `futures`/`tokio-stream` crates (simplified).
// trait Stream {
//     type Item;
//     fn poll_next(
//         self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//     ) -> Poll<Option<Self::Item>>;
// }
```

The `Poll<Option<Item>>` return type encodes three states:

- `Poll::Ready(Some(item))`: here is the next value.
- `Poll::Ready(None)` — the stream is finished, forever.
- `Poll::Pending`: no value yet; the runtime will poll again when progress is possible.

You rarely write `poll_next` by hand, just as you rarely write a manual `Iterator`. But seeing it makes the model concrete: a stream is a thing the runtime polls repeatedly until it yields `None`.

```rust playground
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::{Stream, StreamExt};

// A hand-written Stream that counts up to `max`, one value per poll.
struct Counter {
    current: u32,
    max: u32,
}

impl Stream for Counter {
    type Item = u32;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<u32>> {
        if self.current < self.max {
            self.current += 1;
            Poll::Ready(Some(self.current))
        } else {
            Poll::Ready(None)
        }
    }
}

#[tokio::main]
async fn main() {
    let counter = Counter { current: 0, max: 3 };
    // `collect()` drains the stream into a collection.
    let collected: Vec<u32> = counter.collect().await;
    println!("{collected:?}");
}
```

Real output:

```text
[1, 2, 3]
```

> **Note:** Implementing `Stream` by hand is the rare case. This `Counter` never returns `Pending` because it always has a value ready. It is effectively a synchronous iterator wearing a stream's clothes. Real `poll_next` implementations that wait on I/O are subtle (they must register the `Context`'s waker), which is exactly why you normally use a generator macro or a channel wrapper instead. See [Custom Iterators](/07-collections/08-custom-iterators/) for the synchronous counterpart.

### `StreamExt` provides `.next()` and the combinators

The `Stream` trait deliberately has *only* `poll_next`. All the ergonomic methods — `next`, `map`, `filter`, `take`, `collect`, `fold` — live on an **extension trait** called `StreamExt`, mirroring how `Iterator`'s adapters are inherent but its async equivalents are bolted on separately. You must bring `StreamExt` into scope to use them:

```rust playground
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    // Iterator-style adapters are lazy: nothing runs until consumed.
    let mut stream = tokio_stream::iter(1..=10)
        .filter(|n| n % 2 == 0) // keep evens
        .map(|n| n * n) // square them
        .take(3); // stop after 3 items

    while let Some(x) = stream.next().await {
        println!("{x}");
    }

    // `fold` consumes the whole stream into one accumulated value.
    let total = tokio_stream::iter(1..=5).fold(0, |acc, n| acc + n).await;
    println!("sum = {total}");
}
```

Real output:

```text
4
16
36
sum = 15
```

> **Tip:** `tokio_stream::iter(...)` is the bridge from any `IntoIterator` to a `Stream`. It is handy for tests and for feeding fixed data through stream-shaped code, just like `Promise.resolve` wraps a plain value in a promise.

There are **two** `StreamExt` traits in the ecosystem: `futures::StreamExt` and `tokio_stream::StreamExt`. They overlap heavily but are not identical. For instance, `enumerate` lives on `futures::StreamExt`, not the tokio one:

```rust playground
// `enumerate` is provided by the `futures` crate's StreamExt.
use futures::StreamExt;

#[tokio::main]
async fn main() {
    let labels = futures::stream::iter(["a", "b", "c"]);
    let mut numbered = labels.enumerate();
    while let Some((i, label)) = numbered.next().await {
        println!("{i} => {label}");
    }
}
```

Real output:

```text
0 => a
1 => b
2 => c
```

> **Warning:** Importing both `futures::StreamExt` and `tokio_stream::StreamExt` in the same module makes calls like `.next()` ambiguous and fails to compile. Pick one per module. A common convention: use `tokio_stream` when you want its tokio-specific extras (`timeout`, `throttle`, `chunks_timeout`, the channel wrappers), and `futures` otherwise.

### `while let` is the `for await...of` of Rust

There is no `for await` loop in Rust. The idiomatic consumption form is:

```rust
// while let Some(item) = stream.next().await {
//     // use item
// }
```

This reads as "repeatedly: await the next item; if it is `Some`, run the body; when it is `None`, stop." A plain `for item in stream` does **not** work — `for` requires `Iterator`, and a `Stream` is not one. (`tokio_stream::iter` returns a stream, not an iterator, precisely so you can `.next().await` it.)

### Generators: the `async-stream` crate replaces `async function*`

Writing `poll_next` by hand is painful, and the standard library has no stable equivalent of JavaScript's `async function*` with `yield`. The [`async-stream`](https://docs.rs/async-stream) crate fills that gap with a `stream!` macro where you can `yield` and `.await` freely:

```rust
use std::time::Duration;
use tokio::time::sleep;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    // Like a JavaScript `async function*`: `yield x` emits, `.await` suspends.
    let stream = async_stream::stream! {
        for i in 1..=3 {
            sleep(Duration::from_millis(10)).await; // async work between yields
            yield i * 10;
        }
    };

    // A `stream!` value is not `Unpin`; pin it on the stack to iterate.
    tokio::pin!(stream);
    while let Some(item) = stream.next().await {
        println!("yielded {item}");
    }
}
```

Real output:

```text
yielded 10
yielded 20
yielded 30
```

This is the closest analogue to the TypeScript `async function*` at the top of the page, and for most "produce a sequence with some async work in between" cases it is the right tool.

### Pinning: the one genuinely new concept

You saw `tokio::pin!` twice already. Here is why. To poll a stream, the runtime needs a `Pin<&mut Stream>`, a guarantee that the stream will not be moved in memory while it is being polled. Streams produced by `async-stream` or by the `.then()` adapter are *self-referential state machines* (they hold a suspended future that may point into their own data), so they are **not `Unpin`** and cannot be polled until they are pinned to a fixed location.

`tokio::pin!(name)` pins the value on the stack and shadows the binding so subsequent `.next()` calls work. Streams that are *not* self-referential — like `tokio_stream::iter(...)` or a `ReceiverStream` — are `Unpin` and need no pinning. There is no JavaScript equivalent of this; the JS engine manages all of it for you. Pinning is covered more broadly in [Async Functions](/11-async/04-async-functions/).

### Channels are streams: `ReceiverStream`

A frequent real-world source of a stream is "values arriving on a channel." `tokio-stream` wraps an mpsc receiver into a `Stream` so you can apply combinators and `while let`:

```rust playground
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<u32>(16);

    // A producer task pushes values over time, then drops `tx`.
    tokio::spawn(async move {
        for i in 1..=3 {
            sleep(Duration::from_millis(10)).await;
            let _ = tx.send(i * 100).await;
        }
        // Dropping `tx` ends the stream (next() yields None).
    });

    // Wrap the Receiver as a Stream we can iterate.
    let mut stream = ReceiverStream::new(rx);
    while let Some(value) = stream.next().await {
        println!("received {value}");
    }
    println!("channel closed, stream ended");
}
```

Real output:

```text
received 100
received 200
received 300
channel closed, stream ended
```

The stream ends exactly when the last sender is dropped: the natural "the producer is done" signal. Channels themselves are covered in [Channels](/11-async/08-channels/).

---

## Key Differences

| Aspect | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| The abstraction | `AsyncIterator` / `AsyncIterable` | `Stream` trait (from `futures` / `tokio-stream`) |
| Producer syntax | `async function*` with `yield` | `async_stream::stream! { ... yield ... }` |
| Consumption loop | `for await (const x of s)` | `while let Some(x) = s.next().await` |
| Get next item | `await s.next()` → `{ value, done }` | `s.next().await` → `Option<Item>` |
| In the standard library? | Yes (`Symbol.asyncIterator`) | No. `AsyncIterator` is nightly-only; use a crate |
| Adapters (`map`, `filter`, ...) | Not built in (use a library or manual loop) | On `StreamExt` (must be imported) |
| Needs a runtime? | No: built-in event loop | Yes: Tokio (or another executor) |
| Self-referential producers | Engine-managed | Must be **pinned** (`tokio::pin!`) before polling |
| Backpressure | Manual (the consumer's pace) | Built into bounded channels and `poll_next` |

> **Note:** The deepest conceptual gap is the same one that defines all of Rust async: a stream does nothing until a runtime polls it, and you supply the runtime. A JavaScript async generator advances on the engine's event loop with no setup. See [Promises vs Futures](/11-async/00-promises-vs-futures/) for the full eager-vs-lazy story; a stream is just "a future that resolves many times."

### Streams compose like iterators, lazily

Just like `Iterator` adapters, `Stream` adapters (`map`, `filter`, `take`, `then`, `merge`, ...) build a new lazy stream and run nothing until consumed. `merge` interleaves two same-typed streams, yielding from whichever is ready:

```rust playground
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let a = tokio_stream::iter(vec![1, 2, 3]);
    let b = tokio_stream::iter(vec![10, 20, 30]);
    // `merge` yields from whichever stream is ready next.
    let mut merged = a.merge(b);
    let mut out = Vec::new();
    while let Some(x) = merged.next().await {
        out.push(x);
    }
    println!("{out:?}");
}
```

Real output:

```text
[1, 10, 2, 20, 3, 30]
```

If you already think in `Array.prototype.map`/`filter` and Rust's [iterator adapters](/07-collections/06-iterators/), stream adapters are the async version of the same mental model.

---

## Common Pitfalls

### Pitfall 1: Forgetting to import `StreamExt`

The `Stream` trait gives you `poll_next`, but `.next()` lives on `StreamExt`. Without the import, `.next()` does not exist:

```rust
// Note: no `use tokio_stream::StreamExt;` — so `.next()` is not in scope.

#[tokio::main]
async fn main() {
    let mut stream = tokio_stream::iter(vec![1, 2, 3]);
    // does not compile (error[E0599]): `next` not found without StreamExt.
    while let Some(value) = stream.next().await {
        println!("{value}");
    }
}
```

Real compiler error (trimmed):

```text
error[E0599]: no method named `next` found for struct `tokio_stream::Iter` in the current scope
  --> src/main.rs:6:36
   |
 6 |     while let Some(value) = stream.next().await {
   |                                    ^^^^
   |
   = help: items from traits can only be used if the trait is in scope
help: trait `StreamExt` which provides `next` is implemented but not in scope; perhaps you want to import it
   |
 3 + use tokio_stream::StreamExt;
   |
```

The compiler tells you the exact fix: `use tokio_stream::StreamExt;`. This is the single most common stream error for newcomers.

### Pitfall 2: Polling an unpinned self-referential stream

A `stream!` block or a `.then()` stream is not `Unpin`. Calling `.next()` on it without pinning fails:

```rust
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let stream = async_stream::stream! {
        for i in 1..=3 {
            yield i;
        }
    };

    // does not compile (error[E0277]): the stream is not `Unpin`.
    let mut stream = stream;
    while let Some(item) = stream.next().await {
        println!("{item}");
    }
}
```

The real `rustc` error is `error[E0277]: ... cannot be unpinned`, with the note `consider using the pin! macro`, and points at `StreamExt::next`'s `Self: Unpin` bound. The fix is to pin before iterating:

```rust
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let stream = async_stream::stream! {
        for i in 1..=3 {
            yield i;
        }
    };

    tokio::pin!(stream); // pin on the stack; `stream` is now pollable
    while let Some(item) = stream.next().await {
        println!("{item}");
    }
}
```

Real output:

```text
1
2
3
```

> **Tip:** If you need to store the stream in a struct field or return it as `Box<dyn Stream<...>>`, use `Box::pin(stream)` instead of `tokio::pin!`, which pins on the heap and can be moved around.

### Pitfall 3: Reaching for `for` instead of `while let`

A `Stream` is not an `Iterator`, so `for item in stream` does not compile — `for` desugars to `Iterator::next`, which a stream does not have. There is no `for await` in Rust. Always use `while let Some(item) = stream.next().await`. If you genuinely have an `Iterator` (not a stream), use a normal `for`; the two are different traits with different loops.

### Pitfall 4: Mixing the two `StreamExt` traits

Importing both `futures::StreamExt` and `tokio_stream::StreamExt` makes shared methods like `.next()` and `.map()` ambiguous, producing an `error[E0034]: multiple applicable items in scope`. Choose one `StreamExt` per module. If you need a method that only one of them has (for example, `enumerate` from `futures`, or `timeout` from `tokio_stream`), import just that one in that module.

### Pitfall 5: Expecting `collect` on an infinite stream to terminate

Adapters do not magically bound an infinite stream. Calling `.collect().await` on a stream that never returns `None` (like the `Fib` example below without `.take(...)`) will run forever and exhaust memory. Bound it with `.take(n)`, `.take_while(...)`, or `.timeout(...)` first: the same discipline as a synchronous infinite iterator.

---

## Best Practices

### Prefer `async-stream` over hand-written `poll_next`

Unless you are building a low-level library primitive, write producers with the `stream!` macro. It is readable, supports `.await` and `yield` naturally, and sidesteps the subtle waker-registration logic that manual `poll_next` requires. Reserve a hand-written `Stream` impl for cases where you need zero dependencies or maximal control.

### Wrap channels with `ReceiverStream`/`BroadcastStream`

When the data source is "messages on a channel," use the `tokio_stream::wrappers` types rather than writing a loop that calls `recv().await`. You gain the full combinator vocabulary (`filter`, `map`, `timeout`, `chunks_timeout`) for free.

### Bound and rate-limit explicitly

For network or queue-backed streams, lean on adapters that bound behavior: `take`, `take_while`, `timeout`, and `tokio_stream`'s `throttle` (minimum delay between items) and `chunks_timeout` (batch up to N items or until a deadline). These make backpressure and resource limits visible in the code.

### Pick one `StreamExt` per module and stick to it

Decide up front whether a module uses `futures::StreamExt` or `tokio_stream::StreamExt`, and keep it consistent to avoid ambiguity errors. In a Tokio-centric codebase, `tokio_stream::StreamExt` plus the wrapper types is the path of least resistance.

### Don't block inside a stream's producer

The same rule as any async code: do not call blocking APIs (`std::thread::sleep`, blocking file reads, long CPU loops) inside a `stream!` block or `poll_next` — it stalls the runtime worker. Use async equivalents or move the work to `spawn_blocking`. See [Async vs Sync](/11-async/13-async-vs-sync/) and [Spawning Tasks](/11-async/09-spawning-tasks/).

---

## Real-World Example

A telemetry ingester: a producer task emits sensor readings onto a bounded channel, and a consumer treats the receiver as a stream: filtering out invalid readings, throttling is unnecessary here because the channel already provides backpressure, and accumulating a running summary. This is a common production shape: one side produces, the other consumes a `Stream` with combinators.

```rust playground
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

#[derive(Debug, Clone)]
struct Reading {
    sensor: u32,
    celsius: f64,
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<Reading>(64);

    // Producer task: emit readings over time, including one bogus value,
    // then drop `tx` to signal completion.
    tokio::spawn(async move {
        let samples = [
            Reading { sensor: 1, celsius: 21.5 },
            Reading { sensor: 2, celsius: -999.0 }, // sentinel: hardware fault
            Reading { sensor: 1, celsius: 22.1 },
            Reading { sensor: 3, celsius: 19.8 },
        ];
        for r in samples {
            sleep(Duration::from_millis(5)).await;
            if tx.send(r).await.is_err() {
                break; // consumer gone
            }
        }
    });

    // Consumer: receiver-as-stream + combinators.
    let mut readings = ReceiverStream::new(rx)
        // Drop physically impossible readings.
        .filter(|r| r.celsius > -100.0 && r.celsius < 150.0)
        // Convert to Fahrenheit, keeping the sensor id.
        .map(|r| (r.sensor, r.celsius * 9.0 / 5.0 + 32.0));

    let mut count = 0u32;
    let mut sum_f = 0.0;
    while let Some((sensor, fahrenheit)) = readings.next().await {
        count += 1;
        sum_f += fahrenheit;
        println!("sensor {sensor}: {fahrenheit:.1}F");
    }

    if count > 0 {
        println!("accepted {count} readings, avg {:.1}F", sum_f / count as f64);
    } else {
        println!("no valid readings");
    }
}
```

Real output:

```text
sensor 1: 70.7F
sensor 1: 71.8F
sensor 3: 67.6F
accepted 3 readings, avg 70.0F
```

The bogus `-999.0` reading is filtered out before it reaches the summary, the valid ones are converted and averaged, and the loop ends cleanly when the producer drops its sender. The equivalent TypeScript would be an `async function*` feeding a `for await...of` loop with manual `.filter`/`.map` inside it; the Rust version expresses the pipeline declaratively and runs on the Tokio runtime you opted into.

---

## Further Reading

### Official Documentation

- [`futures::Stream` trait](https://docs.rs/futures/latest/futures/stream/trait.Stream.html)
- [`tokio-stream` crate docs](https://docs.rs/tokio-stream/latest/tokio_stream/)
- [Tokio tutorial — Streams](https://tokio.rs/tokio/tutorial/streams)
- [`async-stream` crate docs](https://docs.rs/async-stream/latest/async_stream/)
- [The async book — Streams](https://rust-lang.github.io/async-book/05_streams/01_chapter.html)
- [`std::pin` — pinning](https://doc.rust-lang.org/std/pin/index.html)

### Related Topics in This Guide

- [Promises vs Futures](/11-async/00-promises-vs-futures/): why a stream, like a future, is lazy and needs a runtime
- [Async/Await Syntax](/11-async/01-async-await/) — `async`/`await`, `.await`, `?` propagation
- [Async Functions](/11-async/04-async-functions/): `async` blocks, returning futures, pinning in depth
- [Tokio Intro](/11-async/02-tokio-intro/): the runtime that polls your streams
- [Channels](/11-async/08-channels/) — mpsc/broadcast/watch, the most common stream sources
- [Select & Join](/11-async/07-select-join/): racing and combining futures and streams
- [Spawning Tasks](/11-async/09-spawning-tasks/): `tokio::spawn`, `spawn_blocking`
- [Async vs Sync](/11-async/13-async-vs-sync/) — when to use async at all
- [Iterators](/07-collections/06-iterators/): the synchronous mental model streams build on
- [Iterator Consumers](/07-collections/07-iterator-consumers/) — `collect`, `fold`, and friends
- [Custom Iterators](/07-collections/08-custom-iterators/): implementing `Iterator` by hand, the sync sibling of a manual `Stream`
- Next up: [Section 12 — Modules & Packages](/12-modules-packages/)

---

## Exercises

### Exercise 1

**Difficulty:** Beginner

**Objective:** Build a stream from a range and process it with adapters.

**Instructions:** Using `tokio_stream::iter(1..=20)` and `StreamExt`, build a stream that keeps only multiples of 3, doubles each kept value, and sums the result with `.fold(...)`. Print the sum. (Expected: the multiples of 3 in 1..=20 are 3, 6, 9, 12, 15, 18; doubled they are 6, 12, 18, 24, 30, 36.)

<details>
<summary>Solution</summary>

```rust playground
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let sum: u32 = tokio_stream::iter(1..=20)
        .filter(|n| n % 3 == 0)
        .map(|n| n * 2)
        .fold(0, |acc, n| acc + n)
        .await;
    println!("sum = {sum}");
}
```

Real output:

```text
sum = 126
```

</details>

### Exercise 2

**Difficulty:** Intermediate

**Objective:** Implement the `Stream` trait by hand for an infinite sequence, then bound it.

**Instructions:** Implement `Stream for Fib` where `Fib { a, b }` yields the Fibonacci sequence (`type Item = u64`). Each `poll_next` should return the current value and advance the state. In `main`, build `Fib { a: 0, b: 1 }`, take the first 10 values with `.take(10)`, `collect` them into a `Vec<u64>`, and print it. Remember the stream is infinite, so the `.take(10)` is what makes it terminate.

<details>
<summary>Solution</summary>

```rust playground
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::{Stream, StreamExt};

struct Fib {
    a: u64,
    b: u64,
}

impl Stream for Fib {
    type Item = u64;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<u64>> {
        let next = self.a;
        self.a = self.b;
        self.b = next + self.b;
        Poll::Ready(Some(next))
    }
}

#[tokio::main]
async fn main() {
    let fib = Fib { a: 0, b: 1 };
    // This stream is infinite; `take` bounds it.
    let first_ten: Vec<u64> = fib.take(10).collect().await;
    println!("{first_ten:?}");
}
```

Real output:

```text
[0, 1, 1, 2, 3, 5, 8, 13, 21, 34]
```

> **Note:** `Fib` is `Unpin` (it has no self-referential async state), so no pinning is needed even though it implements `Stream` directly.

</details>

### Exercise 3

**Difficulty:** Intermediate

**Objective:** Bridge a producer task and a consumer using a channel-as-stream.

**Instructions:** Create a bounded `mpsc::channel::<i64>(32)`. Spawn a producer task that sends `1..=10` over the channel (with a tiny `sleep` between sends) and then drops the sender. In `main`, wrap the receiver with `ReceiverStream`, keep only multiples of 3 with `.filter(...)`, take the first 2 with `.take(2)`, and print each kept value with a `while let` loop.

<details>
<summary>Solution</summary>

```rust playground
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<i64>(32);

    // Producer: emit 1..=10, then drop the sender.
    tokio::spawn(async move {
        for n in 1..=10 {
            sleep(Duration::from_millis(5)).await;
            if tx.send(n).await.is_err() {
                break; // receiver dropped
            }
        }
    });

    // Consumer: receiver-as-stream + combinators.
    let mut stream = ReceiverStream::new(rx)
        .filter(|n| n % 3 == 0)
        .take(2);

    while let Some(n) = stream.next().await {
        println!("kept {n}");
    }
    println!("done");
}
```

Real output:

```text
kept 3
kept 6
done
```

> **Note:** Because of `.take(2)`, the consumer stops after two matching values and drops the `ReceiverStream`; the producer's next `send` then fails and its `break` ends the task cleanly.

</details>
