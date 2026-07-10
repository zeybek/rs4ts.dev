---
title: "Server-Sent Events with Axum"
description: "Push Server-Sent Events from Axum by returning an async Stream, replacing Node's imperative res.write loop with a typed Sse response and throttling."
---

## Quick Overview

**Server-Sent Events (SSE)** is a simple, HTTP-based protocol for pushing a one-way stream of text events from server to browser over a single long-lived connection. Exactly what you reach for to power live dashboards, notification feeds, progress bars, or token-by-token LLM output. In Axum you model an SSE endpoint as an ordinary handler that returns `axum::response::Sse` wrapping an async **`Stream`** of events; this page shows how that `Stream` abstraction replaces the imperative `res.write()` loop you would write in Express.

> **Note:** This page targets the **axum 0.8** API line (recorded with 0.8.9), the repository's [pinned verification toolchain](/00-introduction/05-version-policy/), and the 2024 edition. SSE lives in `axum::response::sse`; you do not need a separate crate for the protocol itself, only a way to build a `Stream` (the `futures-util`, `tokio-stream`, or `async-stream` crates).

---

## TypeScript/JavaScript Example

SSE is built into the browser as the `EventSource` API, and on the server it is just a normal HTTP response with `Content-Type: text/event-stream` that you keep open and write to. Here is a realistic Node endpoint (no framework, to make the wire format obvious) that pushes a `tick` event three times and then closes:

```typescript
// sse.mts — Node v22, plain http server
import http from "node:http";

interface Tick {
  seq: number;
  message: string;
}

const server = http.createServer((req, res) => {
  if (req.url === "/events") {
    // The three headers that make a response an SSE stream.
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
    });

    let seq = 0;
    const timer = setInterval(() => {
      const tick: Tick = { seq, message: `tick ${seq}` };
      // The wire format: optional `event:` line, then `data:`, then a BLANK line.
      res.write(`event: tick\n`);
      res.write(`data: ${JSON.stringify(tick)}\n\n`);
      seq += 1;
      if (seq >= 3) {
        clearInterval(timer);
        res.end();
      }
    }, 150);

    // If the client disconnects, stop the timer so we don't leak it.
    req.on("close", () => clearInterval(timer));
  } else {
    res.writeHead(404).end();
  }
});

server.listen(3011, () => console.log("listening on http://127.0.0.1:3011"));
```

The browser side is tiny:

```typescript
// client.ts — runs in the browser
const source = new EventSource("/events");
source.addEventListener("tick", (e: MessageEvent) => {
  const tick = JSON.parse(e.data) as { seq: number; message: string };
  console.log(tick.seq, tick.message);
});
source.onerror = () => console.log("connection lost; browser will auto-reconnect");
```

Things to notice in the Node version: **you** own the timer, **you** format every `data:` line and remember the trailing blank line, and **you** must clean up on `req.on("close")` or you leak the interval. Running it and connecting with `curl -N` yields:

```text
event: tick
data: {"seq":0,"message":"tick 0"}

event: tick
data: {"seq":1,"message":"tick 1"}

event: tick
data: {"seq":2,"message":"tick 2"}
```

---

## Rust Equivalent

In Axum you do not write to the socket imperatively. You **return a `Stream`**, and Axum's `Sse` response type handles the headers, the `data:`/`event:` framing, the blank-line separators, keep-alive comments, and cleanup-on-disconnect for you.

Add the dependencies in a fresh project (`cargo new sse-demo`):

```toml
# Cargo.toml
[dependencies]
axum = "0.8.9"
tokio = { version = "1.52.3", features = ["full"] }
tokio-stream = "0.1.18"
futures-util = "0.3.32"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
```

```rust
use axum::{
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures_util::stream::{self, Stream};
use serde::Serialize;
use std::{convert::Infallible, time::Duration};
use tokio_stream::StreamExt;

#[derive(Serialize)]
struct Tick {
    seq: u64,
    message: String,
}

// GET /events — a Server-Sent Events endpoint.
// The return type says: "a stream of `Event`s that can never fail".
async fn sse_handler() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::iter(0..3)
        .map(|seq| {
            let tick = Tick { seq, message: format!("tick {seq}") };
            // `json_data` serializes with serde and sets the `data:` line.
            Ok(Event::default().event("tick").json_data(tick).unwrap())
        })
        // Space the events out in time, just like the Node `setInterval`.
        .throttle(Duration::from_millis(200));

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/events", get(sse_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3007").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Run it (`cargo run`) and connect with `curl -N http://127.0.0.1:3007/events`. This is the **real** wire output (`-N` disables curl's buffering so you see events as they arrive):

```text
event: tick
data: {"seq":0,"message":"tick 0"}

event: tick
data: {"seq":1,"message":"tick 1"}

event: tick
data: {"seq":2,"message":"tick 2"}
```

Byte-for-byte the same protocol the Node server produced, but you never wrote a `data:` prefix, a blank-line separator, or a disconnect handler. The response headers are set for you; inspecting them with `curl -D -` shows:

```text
HTTP/1.1 200 OK
content-type: text/event-stream
cache-control: no-cache
transfer-encoding: chunked
```

The same browser `EventSource` client from the TypeScript section talks to this endpoint without changes. SSE is a wire protocol, not a framework feature.

---

## Detailed Explanation

The whole design hinges on one substitution: **Node's imperative `res.write()` loop becomes a declarative `Stream`.** Let's walk the Rust handler line by line and contrast each piece with the Node version.

### The return type is the contract

```rust
async fn sse_handler() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
```

`Sse<S>` is an `IntoResponse` wrapper (see [Request and Response Handling](/16-web-apis/07-request-response/) for how `IntoResponse` turns values into HTTP responses). The type parameter `S` must be a `Stream` whose item is `Result<Event, E>`; note the **`Result`**. SSE streams are allowed to yield errors mid-stream (e.g. a database read fails), so every item is a `Result`. When the stream genuinely cannot fail, the error type is [`std::convert::Infallible`](https://doc.rust-lang.org/std/convert/enum.Infallible.html): the empty type that says "this `Err` variant can never be constructed".

A `Stream` is the async cousin of an `Iterator`: instead of `next() -> Option<T>`, it has `poll_next() -> Poll<Option<T>>`, so producing the next item can `.await`. Streams add the async dimension on top of the synchronous iterator model, and are covered in [11-async](/11-async/02-tokio-intro/).

### Building the stream

```rust
let stream = stream::iter(0..3)
    .map(|seq| { /* ... */ Ok(Event::default().event("tick").json_data(tick).unwrap()) })
    .throttle(Duration::from_millis(200));
```

- `stream::iter(0..3)` lifts an ordinary iterator (`0`, `1`, `2`) into a `Stream`. This is the moral equivalent of the Node `seq` counter.
- `.map(...)` transforms each number into a `Result<Event, Infallible>`. `Event::default()` builds an empty event; `.event("tick")` sets the `event:` field (the SSE "event name" the browser dispatches on); `.json_data(tick)` serializes the struct with serde and writes the `data:` line. We then wrap it in `Ok(...)`.
- `.throttle(Duration::from_millis(200))` (from `tokio_stream::StreamExt`) ensures at least 200ms passes between items: the declarative replacement for `setInterval`. There is no timer handle to clear; when the connection closes, Axum drops the stream and the throttle timer dies with it.

### The `Event` builder

`Event` is a builder for all five SSE fields. Each method returns `Self`, so they chain:

| Method | SSE wire field | Purpose |
| --- | --- | --- |
| `.data(s)` / `.json_data(v)?` | `data:` | The payload. `json_data` serializes with serde. |
| `.event(name)` | `event:` | Named event type the browser dispatches on (`addEventListener("tick", ...)`). |
| `.id(s)` | `id:` | Sets `lastEventId`; the browser echoes it as the `Last-Event-ID` header on reconnect. |
| `.retry(duration)` | `retry:` | Tells the browser how long to wait before reconnecting. |
| `.comment(s)` | `: ...` | A comment line (ignored by clients); used for keep-alive pings. |

> **Note:** `json_data` returns a `Result` because serialization can fail; in handlers prefer `?` over `.unwrap()`, which is why the error-returning patterns below matter.

### Keep-alive

```rust
Sse::new(stream).keep_alive(KeepAlive::default())
```

If your stream is idle for a long time, proxies and load balancers may kill the "dead" connection. `KeepAlive` periodically sends an SSE comment line (`:` plus optional text) that the browser ignores but that keeps the TCP connection and any intermediary timers alive. The default interval is 15 seconds; you can customize it (shown in the real-world example below). The Node version has no equivalent; you would have to schedule your own `res.write(": ping\n\n")`.

### Where the analogy breaks down

> **Unlike** the Node example, there is no `req.on("close")` cleanup to write. Cancellation in Rust is **structured**. When the client disconnects, Axum stops polling the stream and **drops** it. Any resources the stream holds (timers, a `broadcast::Receiver`, a database cursor) are released by their `Drop` impls. You get correct cleanup for free precisely because you described *what* the stream is rather than imperatively driving it.

---

## Key Differences

| Concern | Express / Node | Axum |
| --- | --- | --- |
| Mental model | Imperative: keep `res` open, `res.write(...)` repeatedly | Declarative: return a `Stream<Item = Result<Event, E>>` |
| Wire framing (`data:`, blank lines) | You format every line by hand | `Event` builder + `Sse` do it |
| Headers (`text/event-stream`, `no-cache`) | You set them via `res.writeHead` | Set automatically by the `Sse` response |
| Spacing events over time | `setInterval` + a timer handle | A stream combinator (`.throttle`) or an interval stream |
| Cleanup on disconnect | Manual `req.on("close")` handler | Automatic: the stream is dropped, `Drop` releases resources |
| Errors mid-stream | Throw / `res.destroy()`; ad hoc | First-class: each item is a `Result`, error type in the signature |
| Keep-alive pings | Schedule your own comment writes | `.keep_alive(KeepAlive::default())` |
| Fan-out to many clients | Track a `Set<res>` and loop-write | Each client subscribes to a `broadcast` channel |

The deeper point: SSE in Express is *a pattern you assemble*; in Axum it is *a type you return*. The compiler enforces that you produce `Event`s wrapped in `Result`, and the framework owns the protocol details.

> **Tip:** SSE is **one-way** (server → client) and text-only over plain HTTP, with automatic browser reconnection built into `EventSource`. If you need bidirectional or binary traffic, use WebSockets instead — see [WebSockets with Axum](/16-web-apis/15-websockets/) for the tradeoffs.

---

## Common Pitfalls

### Pitfall 1: forgetting that items must be `Result`, not bare `Event`

A TypeScript developer reasonably writes a stream that yields `Event` values directly:

```rust
use axum::{
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use futures_util::stream::{self, Stream};
use futures_util::StreamExt;

// does not compile (error E0271): items are `Event`, not `Result<Event, E>`.
async fn bad_handler() -> Sse<impl Stream<Item = Event>> {
    let stream = stream::iter(0..3).map(|i| Event::default().data(format!("{i}")));
    Sse::new(stream)
}

#[tokio::main]
async fn main() {
    let app: Router = Router::new().route("/events", get(bad_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3009").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

The real compiler error:

```text
error[E0271]: expected `{closure@main.rs:11:41}` to return `Result<Event, _>`, but it returns `Event`
  --> src/main.rs:11:45
   |
11 |     let stream = stream::iter(0..3).map(|i| Event::default().data(format!("{i}")));
   |                                         --- ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `Result<Event, _>`, found `Event`
   |                                         |
   |                                         this closure
12 |     Sse::new(stream)
   |     ---------------- closure used here
   |
   = note: expected enum `Result<Event, _>`
            found struct `Event`
```

**Fix:** wrap each event in `Ok(...)` and annotate the error type, e.g. `Result<Event, Infallible>` when nothing can fail.

### Pitfall 2: importing the wrong `StreamExt`

Both `futures_util::StreamExt` and `tokio_stream::StreamExt` exist, and they offer *different* combinators. `.throttle(...)` lives on `tokio_stream::StreamExt`; `.enumerate()` lives on `futures_util::StreamExt`. If you call a combinator without its trait in scope, the compiler complains that the trait bounds were not satisfied, but, helpfully, points you at the exact import you need:

```rust
// does not compile (error E0599): StreamExt is not in scope.
fn main() {
    let s = futures_util::stream::iter(0..3);
    let _e = s.enumerate();
}
```

```text
error[E0599]: the method `enumerate` exists for struct `futures_util::stream::Iter<std::ops::Range<{integer}>>`, but its trait bounds were not satisfied
 --> src/main.rs:3:16
  |
3 |     let _e = s.enumerate();
  |                ^^^^^^^^^ method cannot be called due to unsatisfied trait bounds
  |
  = note: the following trait bounds were not satisfied:
          `futures_util::stream::Iter<std::ops::Range<{integer}>>: Iterator`
          which is required by `&mut futures_util::stream::Iter<std::ops::Range<{integer}>>: Iterator`
  = help: items from traits can only be used if the trait is in scope
help: trait `StreamExt` which provides `enumerate` is implemented but not in scope; perhaps you want to import it
  |
1 + use futures_util::StreamExt;
  |
```

**Fix:** read the `help:` line; it names the exact trait to import. Importing *both* `StreamExt` traits causes a method-ambiguity error for the methods they share (like `map`), so pick the one whose combinators you need, or fully-qualify the call.

### Pitfall 3: blocking the async runtime to produce events

Putting a synchronous sleep or a blocking I/O call inside the stream's closure stalls the entire worker thread, freezing every other request the runtime is handling:

```rust
// wrong: std::thread::sleep blocks the async worker thread.
.map(|i| {
    std::thread::sleep(std::time::Duration::from_secs(1)); // never do this in async
    Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(i.to_string()))
})
```

**Fix:** use async timing — `.throttle(...)`, an `IntervalStream`, or `tokio::time::sleep(...).await` inside an `async_stream::stream! { ... }` block (shown next). See [11-async](/11-async/02-tokio-intro/) for why blocking calls poison cooperative runtimes.

### Pitfall 4: buffering proxies hide your events

If you test through nginx (or a similar reverse proxy) and events arrive all at once at the end instead of trickling in, the proxy is buffering the response. SSE needs `proxy_buffering off;` (nginx) on the relevant location. This is not an Axum issue — Axum already sends `Cache-Control: no-cache` — but it surprises people. Locally, remember `curl -N` to disable curl's own buffering. Deployment specifics live in [Deploying Axum Applications](/16-web-apis/19-deployment/).

---

## Best Practices

- **Always type the stream's error explicitly.** Use `Infallible` when nothing can fail (it documents intent and lets the compiler optimize), or a real error type when items come from fallible sources.
- **Prefer `event.json_data(value)?` over hand-formatting `data:`.** It serializes with serde, escapes newlines correctly, and keeps your payload in one typed struct.
- **Set an event `id` when clients should resume.** On reconnect the browser sends `Last-Event-ID`; read it (via the `headers` extractor — see [Extractors](/16-web-apis/04-extractors/)) to replay only what was missed.
- **Add a keep-alive.** `KeepAlive::default()` is cheap insurance against idle-connection timeouts in proxies.
- **Fan out with a `broadcast` channel, not a shared `Vec` of senders.** `tokio::sync::broadcast` gives each subscriber its own receiver and handles slow-consumer lag for you; combine it with shared `State` (see [Shared Application State in Axum](/16-web-apis/06-state-management/)).
- **Use `async_stream::stream!` when emission is inherently sequential/stateful** (a countdown, a job that does work between yields). Use combinators (`stream::iter(...).map(...).throttle(...)`) when the source is already a collection or another stream.
- **Set `Content-Type` via `Sse`, never by hand.** Returning `Sse` already sets it; manually adding the header risks duplicates.

---

## Real-World Example

A live notification feed: clients open `GET /events` to subscribe, and any process can `POST /messages` to broadcast a message to **every** connected client. This is the canonical SSE production pattern: a `tokio::sync::broadcast` channel held in shared state, with each subscriber getting its own receiver wrapped as a stream.

Add `tokio-stream`'s `sync` feature for `BroadcastStream` (`cargo add tokio-stream --features sync`):

```rust
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, time::Duration};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

#[derive(Clone, Serialize)]
struct ChatMessage {
    user: String,
    text: String,
}

#[derive(Clone)]
struct AppState {
    // A broadcast channel: every subscriber gets a clone of every message.
    tx: broadcast::Sender<ChatMessage>,
}

#[derive(Deserialize)]
struct PostMessage {
    user: String,
    text: String,
}

// POST /messages — publish to all connected SSE clients.
async fn publish(State(state): State<AppState>, Json(body): Json<PostMessage>) -> &'static str {
    let msg = ChatMessage { user: body.user, text: body.text };
    // `send` errors only when there are zero receivers; we ignore that.
    let _ = state.tx.send(msg);
    "queued"
}

// GET /events — subscribe; the stream lives as long as the connection.
async fn subscribe(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(msg) => Some(Ok(Event::default().json_data(msg).unwrap())),
        // A slow client that fell behind yields a `Lagged` error; skip it.
        Err(_lagged) => None,
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[tokio::main]
async fn main() {
    let (tx, _rx) = broadcast::channel::<ChatMessage>(64);
    let state = AppState { tx };

    let app = Router::new()
        .route("/messages", post(publish))
        .route("/events", get(subscribe))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3008").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Exercise it: start the server, subscribe in one terminal with `curl -N http://127.0.0.1:3008/events`, then post messages from another:

```bash
curl -X POST http://127.0.0.1:3008/messages \
  -H 'content-type: application/json' -d '{"user":"alice","text":"hello"}'
curl -X POST http://127.0.0.1:3008/messages \
  -H 'content-type: application/json' -d '{"user":"bob","text":"hi there"}'
```

The subscriber receives, in real time (verified output):

```text
data: {"user":"alice","text":"hello"}

data: {"user":"bob","text":"hi there"}
```

When a subscriber's `curl` is killed, its `BroadcastStream` is dropped, the underlying `broadcast::Receiver` is dropped, and the channel automatically stops tracking it: no bookkeeping, no leaked slot. In a real app, `AppState` would also hold a database pool (see [Shared Application State in Axum](/16-web-apis/06-state-management/) and [Databases](/17-database/)), and `publish` would persist the message before broadcasting.

> **Tip:** `broadcast::channel(64)` sets the per-receiver buffer. If a client cannot keep up and falls more than 64 messages behind, it receives a `Lagged` error (which we map to "skip"). Size the buffer to your tolerance for dropped messages on slow clients.

---

## Further Reading

- [`axum::response::sse` module docs](https://docs.rs/axum/latest/axum/response/sse/index.html) — `Sse`, `Event`, `KeepAlive`.
- [`Event` builder API](https://docs.rs/axum/latest/axum/response/sse/struct.Event.html) — every field method.
- [`tokio_stream` wrappers](https://docs.rs/tokio-stream/latest/tokio_stream/wrappers/index.html) — `BroadcastStream`, `IntervalStream`, `ReceiverStream`.
- [`async-stream` crate](https://docs.rs/async-stream/latest/async_stream/) — the `stream!` / `try_stream!` macros for sequential emission.
- [MDN: Using server-sent events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events) — the `EventSource` client API and wire format.
- Cross-links in this section: [Axum Fundamentals](/16-web-apis/01-axum-basics/) (the handler/router/serve loop), [Extractors](/16-web-apis/04-extractors/) (reading `State` and headers), [Shared Application State in Axum](/16-web-apis/06-state-management/) (sharing the broadcast channel), [Request and Response Handling](/16-web-apis/07-request-response/) (`IntoResponse`), [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/) (fallible streams), [JSON REST APIs](/16-web-apis/08-json-apis/) (serde + `Json`), and [WebSockets with Axum](/16-web-apis/15-websockets/) (when you need two-way traffic).
- Foundations: [The Tokio Runtime](/11-async/02-tokio-intro/) (futures, streams, the runtime), and the project [intro](/00-introduction/), [getting started](/01-getting-started/), and [basics](/02-basics/).

---

## Exercises

### Exercise 1: A server clock

**Difficulty:** Beginner

**Objective:** Build the simplest possible time-driven SSE endpoint and watch it tick.

**Instructions:** Create a `GET /clock` handler that emits one event per second forever, where each event's `data:` is the current count (`0`, `1`, `2`, ...). Use `tokio_stream::wrappers::IntervalStream` over a `tokio::time::interval` plus a stream combinator. Add a default keep-alive. Connect with `curl -N` and confirm one event arrives per second.

<details>
<summary>Solution</summary>

```rust
use axum::{
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures_util::stream::{Stream, StreamExt};
use std::{convert::Infallible, time::Duration};

async fn clock() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = tokio_stream::wrappers::IntervalStream::new(
        tokio::time::interval(Duration::from_secs(1)),
    )
    .enumerate()
    .map(|(count, _instant)| Ok(Event::default().data(count.to_string())));

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/clock", get(clock));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3012").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

`curl -N http://127.0.0.1:3012/clock` prints (one line pair per second):

```text
data: 0

data: 1

data: 2
```

> Note `IntervalStream`'s combinators here come from `futures_util::StreamExt` (`enumerate`, `map`). Import only that `StreamExt` to avoid the method-ambiguity error from Pitfall 2.

</details>

### Exercise 2: A resumable metrics stream with event ids

**Difficulty:** Intermediate

**Objective:** Emit structured JSON events with monotonically increasing `id`s so a reconnecting client could resume.

**Instructions:** Create a `GET /metrics` handler that, once per second, emits a named `metrics` event whose `data:` is a JSON object `{ "cpu": ..., "memory": ..., "at": <RFC 3339 timestamp> }`, and whose SSE `id` is the sequence number. Use serde for the struct and the `chrono` crate for the timestamp (`cargo add chrono --features clock`).

<details>
<summary>Solution</summary>

```rust
use axum::{
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use chrono::Utc;
use futures_util::stream::{Stream, StreamExt};
use serde::Serialize;
use std::{convert::Infallible, time::Duration};

#[derive(Serialize)]
struct Reading {
    cpu: f64,
    memory: f64,
    at: String,
}

async fn metrics() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = tokio_stream::wrappers::IntervalStream::new(
        tokio::time::interval(Duration::from_secs(1)),
    )
    .enumerate()
    .map(|(i, _instant)| {
        let reading = Reading {
            cpu: 12.5 + i as f64,
            memory: 40.0,
            at: Utc::now().to_rfc3339(),
        };
        Ok(Event::default()
            .id(i.to_string())
            .event("metrics")
            .json_data(reading)
            .unwrap())
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/metrics", get(metrics));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3012").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

`curl -N http://127.0.0.1:3012/metrics` prints (verified output):

```text
id: 0
event: metrics
data: {"cpu":12.5,"memory":40.0,"at":"2026-06-01T12:07:52.873017+00:00"}

id: 1
event: metrics
data: {"cpu":13.5,"memory":40.0,"at":"2026-06-01T12:07:53.873344+00:00"}

id: 2
event: metrics
data: {"cpu":14.5,"memory":40.0,"at":"2026-06-01T12:07:54.876789+00:00"}
```

To make it truly resumable, add a `headers: HeaderMap` parameter (see [Extractors](/16-web-apis/04-extractors/)) and start the counter from the incoming `Last-Event-ID` header.

</details>

### Exercise 3: A countdown with sequential async logic

**Difficulty:** Intermediate / Advanced

**Objective:** Use `async_stream::stream!` to emit events from imperative, stateful async code — the situation where combinators get awkward.

**Instructions:** Create a `GET /launch` handler that counts down `T-minus 3`, `T-minus 2`, `T-minus 1` (one event every 150ms), then emits a final `liftoff` named event with payload `Liftoff!`, then ends the stream. Use the `async_stream::stream!` macro (`cargo add async-stream`) so you can write a normal `for` loop with `.await` and `yield`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use async_stream::stream;
use futures_util::stream::Stream;
use std::{convert::Infallible, time::Duration};

async fn countdown() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let s = stream! {
        for n in (1..=3).rev() {
            tokio::time::sleep(Duration::from_millis(150)).await;
            yield Ok(Event::default().data(format!("T-minus {n}")));
        }
        yield Ok(Event::default().event("liftoff").data("Liftoff!"));
    };
    Sse::new(s).keep_alive(KeepAlive::default())
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/launch", get(countdown));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3010").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

`curl -N http://127.0.0.1:3010/launch` prints (verified):

```text
data: T-minus 3

data: T-minus 2

data: T-minus 1

event: liftoff
data: Liftoff!
```

The `stream!` macro turns this imperative async block into a `Stream` whose items are exactly what each `yield` produces — far more readable than chaining combinators when each step depends on the last. For fallible logic, use `try_stream!` and `yield` values directly while propagating errors with `?`.

</details>
