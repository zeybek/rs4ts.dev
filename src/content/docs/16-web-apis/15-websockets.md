---
title: "WebSockets with Axum"
description: "Build WebSocket servers in Axum with WebSocketUpgrade. Instead of Node's ws callbacks, you own an async recv loop, split the socket, and broadcast over a channel."
---

## Quick Overview

WebSockets give you a single long-lived, full-duplex TCP connection where both the server and the client can push messages at any time: perfect for chat, live dashboards, multiplayer games, and collaborative editors. In Node you reach for the `ws` library (or Socket.IO); in Axum you use `axum::extract::ws::WebSocketUpgrade`, which turns an ordinary HTTP handler into a connection upgrade and then hands you an `async` send/receive loop. This page covers the full lifecycle: upgrading the request, the receive loop, sending messages, splitting the socket for concurrent reads and writes, and a working echo server.

> **Note:** This page targets the **axum 0.8** API line (recorded with 0.8.9) and the repository's [pinned verification toolchain](/00-introduction/05-version-policy/) on the 2024 edition. The WebSocket types live behind a Cargo feature: `cargo add axum --features ws`. In 0.8 the `Message::Text` variant wraps a `Utf8Bytes` (a cheap, UTF-8-validated byte buffer), not a `String`; a detail that trips up developers following older tutorials.

---

## TypeScript/JavaScript Example

A realistic echo-plus-broadcast WebSocket server in Node using the `ws` library. It accepts connections, echoes text back, and also broadcasts every message to all connected clients.

```typescript
// server.ts — Node v22, `npm i ws @types/ws`
import { WebSocketServer, WebSocket } from "ws";

const wss = new WebSocketServer({ port: 3000 });
console.log("listening on ws://127.0.0.1:3000");

wss.on("connection", (socket: WebSocket, req) => {
  console.log(`client connected from ${req.socket.remoteAddress}`);

  // Receive: the event loop calls this every time a frame arrives.
  socket.on("message", (data: Buffer, isBinary: boolean) => {
    const text = data.toString();
    console.log(`received: ${text}`);

    // Echo back to the sender.
    socket.send(`echo: ${text}`);

    // Broadcast to everyone else.
    for (const client of wss.clients) {
      if (client !== socket && client.readyState === WebSocket.OPEN) {
        client.send(text);
      }
    }
  });

  // Lifecycle callbacks.
  socket.on("close", () => console.log("client disconnected"));
  socket.on("error", (err) => console.error("socket error:", err));

  // The server can push at any time — here, a greeting.
  socket.send("welcome");
});
```

Things a TypeScript developer relies on here: the server is **callback-driven**. You register `on("message")`, `on("close")`, and `on("error")` handlers, and Node's event loop invokes them whenever a frame arrives. The connection's `clients` set lives on the server object, and "broadcast" is a `for` loop over it. There is no explicit loop you write — the runtime owns the loop and calls back into your code.

---

## Rust Equivalent

The same echo server in Axum. Instead of registering callbacks, you write one `async fn` that **owns the connection** and loops over incoming messages explicitly.

```rust
// Cargo.toml dependencies:
//   axum = { version = "0.8", features = ["ws"] }
//   tokio = { version = "1", features = ["full"] }
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::any,
    Router,
};

#[tokio::main]
async fn main() {
    let app = Router::new().route("/ws", any(ws_handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on ws://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

// 1. An ordinary handler that takes the `WebSocketUpgrade` extractor.
//    Returning `ws.on_upgrade(..)` responds with HTTP 101 Switching Protocols.
async fn ws_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_socket)
}

// 2. This runs AFTER the handshake completes, once per connection.
//    `socket` is owned here — this function IS the connection's lifecycle.
async fn handle_socket(mut socket: WebSocket) {
    // 3. The receive loop. `recv()` returns `Some(Ok(msg))` per frame,
    //    `None` when the client hangs up.
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                // 4. Echo back. `Message::Text` wraps `Utf8Bytes`;
                //    `format!` derefs it to `&str`, then `.into()` converts back.
                if socket
                    .send(Message::Text(format!("echo: {text}").into()))
                    .await
                    .is_err()
                {
                    break; // client gone; stop the loop
                }
            }
            Message::Binary(bin) => {
                if socket.send(Message::Binary(bin)).await.is_err() {
                    break;
                }
            }
            // 5. The protocol's control frames. axum auto-replies to Ping
            //    with Pong, so you usually just observe these.
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) => {}
        }
    }
    // Falling out of the loop drops `socket`, which closes the connection.
}
```

This compiles and serves. Here is a real round trip captured by connecting a client, sending `hello` and `world`, then closing (server-side logs added to the loop for the trace):

```text
[client] connected to ws://127.0.0.1:65159/ws
[server] received text: hello
[client] got: echo: hello
[server] received text: world
[client] got: echo: world
[server] client closed: None
[server] socket loop ended
[client] done
```

> **Tip:** The route uses `any(ws_handler)`, not `get(...)`. A WebSocket handshake is technically a `GET`, so `get(ws_handler)` also works, but `any` is the conventional choice because the upgrade is method-agnostic from your code's point of view and it avoids surprises if a proxy rewrites the method.

---

## Detailed Explanation

### The two-phase handshake

A WebSocket connection starts life as a normal HTTP request with `Upgrade: websocket` headers. Axum models this in two phases, and the split is the key mental shift from Node:

1. **`ws_handler`** is a regular Axum handler. The `WebSocketUpgrade` extractor reads the upgrade headers and validates the handshake. Calling `ws.on_upgrade(callback)` returns a `Response` (HTTP `101 Switching Protocols`). At this point you are still in normal request/response land: you can run other extractors first (auth, query params), reject the request with a different status, and so on.

2. **`handle_socket`** runs only *after* the `101` response has been sent and the TCP connection has been "upgraded". It receives an owned `WebSocket` and is responsible for the entire conversation. When this `async fn` returns, the socket is dropped and the connection closes.

In Node, both phases are collapsed into the `wss.on("connection", ...)` callback, and the library owns the read loop. In Axum **you own the loop**, which is why you write `while let Some(...) = socket.recv().await`.

### `recv()` returns `Option<Result<Message, Error>>`

- `Some(Ok(msg))`: a frame arrived.
- `Some(Err(e))`: a protocol or I/O error (malformed frame, broken pipe).
- `None`: the stream ended cleanly; the client is gone.

The pattern `while let Some(Ok(msg)) = socket.recv().await` stops the loop on *either* an error or a clean close, which is the right default for an echo server. If you need to log errors, match all three arms instead of using `while let`.

### The `Message` enum

In axum 0.8 the variants are:

| Variant | Payload type | Meaning |
| --- | --- | --- |
| `Message::Text` | `Utf8Bytes` | A UTF-8 text frame |
| `Message::Binary` | `Bytes` | A binary frame |
| `Message::Ping` | `Bytes` | Keep-alive ping (axum auto-replies with Pong) |
| `Message::Pong` | `Bytes` | Reply to a ping |
| `Message::Close` | `Option<CloseFrame>` | Graceful close, optionally with code + reason |

`Utf8Bytes` and `Bytes` are reference-counted, cheaply cloneable byte buffers (from the `bytes` crate). `Utf8Bytes` derefs to `&str`, so `format!("echo: {text}")` and `text.to_string()` both work directly. To build a text message you can write `Message::Text(my_string.into())` or the shorthand constructor `Message::text(my_string)`.

> **Note:** Axum (via the underlying `tungstenite` library) **automatically responds to Ping frames with Pong**. You generally do not need to handle `Message::Ping` yourself unless you want to observe liveness. You *can* send your own `Message::Ping` to detect dead connections.

### Sending and the back-pressure check

`socket.send(msg).await` returns a `Result`. An `Err` means the connection is broken (the client vanished mid-write). The idiomatic pattern is `if socket.send(...).await.is_err() { break; }`; there is no point continuing to read a socket you can no longer write to.

---

## Key Differences

| Concept | Node (`ws`) | Axum |
| --- | --- | --- |
| Programming model | Callback-driven (`on("message")`) | You own an explicit `async` loop |
| Who owns the read loop | The library / event loop | Your `handle_socket` function |
| Connection lifetime | Until you call `socket.close()` or it errors | Until `handle_socket` returns (socket dropped) |
| Concurrent send + receive | Implicit (both are just callbacks/method calls) | Must `split()` the socket into a sender + receiver |
| Text payload type | `Buffer` / `string` | `Utf8Bytes` (derefs to `&str`) |
| Ping/Pong | Manual or via library options | Auto-Pong by axum; ping is opt-in |
| Tracking all clients | `wss.clients` set on the server | Shared state + a `broadcast` channel (you build it) |
| Backpressure | Mostly hidden; `socket.bufferedAmount` | Explicit: `send().await` can fail / suspend |

The deepest difference is **ownership of the loop**. In Node the runtime calls your callbacks; a single connection's logic is scattered across `on("message")`, `on("close")`, etc., sharing state through closures. In Axum, one function reads top-to-bottom and owns everything for that connection, so the control flow (and where the connection ends) is explicit. This is the same async ownership story covered in [the async section](/11-async/) — Rust futures are lazy and driven by the [Tokio](/11-async/02-tokio-intro/) runtime, the opposite of eager JavaScript promises.

---

## Common Pitfalls

### Pitfall 1: `Message::Text(String)` — wrong payload type

Following an older tutorial, you might write:

```rust
use axum::extract::ws::Message;
fn main() {
    // does not compile (error[E0308]: mismatched types)
    let _m = Message::Text(String::from("hi"));
}
```

In axum 0.8 the `Text` variant holds `Utf8Bytes`, not `String`. The real compiler error:

```text
error[E0308]: mismatched types
   --> src/bin/err1.rs:4:28
    |
  4 |     let _m = Message::Text(String::from("hi"));
    |              ------------- ^^^^^^^^^^^^^^^^^^ expected `Utf8Bytes`, found `String`
    |              |
    |              arguments to this enum variant are incorrect
    |
help: call `Into::into` on this expression to convert `String` into `Utf8Bytes`
```

The compiler tells you the fix: `Message::Text(String::from("hi").into())`, or simply `Message::text("hi")`.

### Pitfall 2: trying to read and write from two tasks without `split()`

A `WebSocket` is a single owned value. If you want to read in one task and write in another (e.g. a heartbeat that pushes while the read loop runs), you cannot move the whole socket into both:

```rust
use axum::extract::ws::{Message, WebSocket};

async fn handle(mut socket: WebSocket) {
    // does not compile (error[E0382]: use of moved value: `socket`)
    tokio::spawn(async move {
        let _ = socket.send(Message::text("hi")).await;
    });
    tokio::spawn(async move {
        let _ = socket.recv().await;
    });
}
```

The real error:

```text
error[E0382]: use of moved value: `socket`
 --> src/bin/err2.rs:8:18
  |
3 | async fn handle(mut socket: WebSocket) {
  |                 ---------- move occurs because `socket` has type `WebSocket`, which does not implement the `Copy` trait
...
5 |     tokio::spawn(async move {
  |                  ---------- value moved here
...
8 |     tokio::spawn(async move {
  |                  ^^^^^^^^^^ value used here after move
```

The fix is `socket.split()`, which gives you an owned `SplitSink` (the writer) and an owned `SplitStream` (the reader). Each half can move into its own task — see the chat-room example below.

### Pitfall 3: blocking the read loop with slow work

Because *you* own the loop, any slow synchronous work inside it (a tight CPU loop, a blocking file read) stalls that connection, and if you use a single-threaded runtime, every connection. Offload heavy work to `tokio::task::spawn_blocking` or a worker task and keep the read loop responsive, exactly as covered in [the async section](/11-async/).

### Pitfall 4: forgetting the `ws` feature

`use axum::extract::ws::WebSocketUpgrade;` will not resolve unless you enabled the feature. Run `cargo add axum --features ws`. Without it you'll get an unresolved-import error pointing at `ws`.

---

## Best Practices

- **Split the socket for any non-echo workload.** As soon as the server needs to *push* (chat broadcast, live updates, heartbeats), call `socket.split()` and run a sender task and a receiver task, joined with `tokio::select!` so that when one ends you `abort()` the other.
- **Combine the upgrade with other extractors freely.** `WebSocketUpgrade` is a `FromRequestParts` extractor: it inspects only the handshake *headers*, never the request body, so it can sit in any argument position. Put auth/query/path extractors before *or* after it to authenticate or parameterize the connection (see [authentication](/16-web-apis/12-authentication/) and [extractors](/16-web-apis/04-extractors/)). The "must come last" rule applies only to body-consuming `FromRequest` extractors like `Json`, `Bytes`, and `String` — `WebSocketUpgrade` is not one of them.
- **Use `tokio::sync::broadcast` for fan-out.** It is the natural analogue of Node's `wss.clients` loop: every connection `subscribe()`s to a shared sender stored in [application state](/16-web-apis/06-state-management/).
- **Send a `Close` frame on graceful shutdown.** It lets the client distinguish a clean close from a dropped connection and carries a status code and reason.
- **Cap message size.** `ws.max_message_size(64 * 1024)` and `ws.max_frame_size(...)` guard against memory-exhaustion from hostile clients.
- **Send periodic pings** if you need to detect half-open connections (a client that vanished without a TCP FIN); time out if no Pong returns.

---

## Real-World Example

A small **chat room** that fans messages out to every connected client, structured the way a production service would be: shared state holds a `broadcast` channel, and each connection splits its socket into a sender task (forwarding broadcasts to this client) and a receiver task (publishing this client's messages). This is the Axum equivalent of the Node `wss.clients` broadcast loop.

```rust
// Cargo.toml dependencies:
//   axum = { version = "0.8", features = ["ws"] }
//   tokio = { version = "1", features = ["full"] }
//   futures-util = "0.3"
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    routing::any,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;

// Shared application state: one broadcast sender for the whole room.
#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<String>,
}

#[tokio::main]
async fn main() {
    // Capacity 64: how many messages may lag behind a slow client.
    let (tx, _rx) = broadcast::channel(64);
    let state = Arc::new(AppState { tx });

    let app = Router::new()
        .route("/ws", any(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("chat on ws://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(|socket| chat(socket, state))
}

async fn chat(socket: WebSocket, state: Arc<AppState>) {
    // Split into an owned writer and an owned reader.
    let (mut sender, mut receiver) = socket.split();

    // Each connection gets its own subscription to the shared channel.
    let mut rx = state.tx.subscribe();

    // Announce the join to everyone (including future-joined readers).
    let _ = state.tx.send("* a user joined".to_string());

    // Task A: forward every broadcast message to THIS client.
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::text(msg)).await.is_err() {
                break; // this client disconnected
            }
        }
    });

    // Task B: read THIS client's messages and publish them to the room.
    let tx = state.tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let _ = tx.send(format!("user: {text}"));
        }
    });

    // When either side ends, abort the other and clean up.
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    let _ = state.tx.send("* a user left".to_string());
}
```

Running this with two clients — client A is already connected, then client B connects (and sees the join announcement), after which A sends `hi from A` — produces this real output on B's side:

```text
[B] * a user joined
[B] user: hi from A
```

> **Note:** `broadcast::channel` drops the oldest message for receivers that fall too far behind, and `rx.recv()` then returns a `RecvError::Lagged(n)`. The `while let Ok(msg) = ...` loop above stops on that error; in production you'd match it explicitly and either skip-and-continue or close the slow client. The integer you pass to `broadcast::channel` is that backlog capacity.

For wiring the `broadcast` sender into a larger app (alongside a database pool and config), see [state management](/16-web-apis/06-state-management/). For pushing *server-to-client only* updates without the client ever sending (simpler than WebSockets), reach for [Server-Sent Events](/16-web-apis/16-sse/) instead.

---

## Further Reading

- [`axum::extract::ws` documentation](https://docs.rs/axum/latest/axum/extract/ws/index.html) — the authoritative reference for `WebSocketUpgrade`, `WebSocket`, and `Message`.
- [Axum WebSocket example](https://github.com/tokio-rs/axum/tree/main/examples/websockets) — the official, maintained chat example this page is modeled on.
- [The WebSocket Protocol (RFC 6455)](https://datatracker.ietf.org/doc/html/rfc6455): frames, close codes, and the handshake.
- [`tokio::sync::broadcast`](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) — the fan-out channel used for the chat room.
- Sibling pages in this section: [Axum basics](/16-web-apis/01-axum-basics/) · [routing](/16-web-apis/03-routing/) · [extractors](/16-web-apis/04-extractors/) · [state management](/16-web-apis/06-state-management/) · [authentication](/16-web-apis/12-authentication/) · [Server-Sent Events](/16-web-apis/16-sse/) · [deployment](/16-web-apis/19-deployment/).
- Foundations: [async & futures](/11-async/) · [the Tokio runtime](/11-async/02-tokio-intro/) · [getting started](/01-getting-started/) · [language basics](/02-basics/). Persisting chat history? See [the database section](/17-database/).

---

## Exercises

### Exercise 1: Reverse-echo server

**Difficulty:** Beginner

**Objective:** Get comfortable with the upgrade handler and the receive loop.

**Instructions:** Starting from the echo server in the "Rust Equivalent" section, change the handler so that for every text frame it replies with the message **reversed** (e.g. `hello` → `olleh`). Leave binary frames echoed unchanged and break the loop on a close frame.

<details>
<summary>Solution</summary>

```rust
// Cargo.toml: axum = { version = "0.8", features = ["ws"] }
//             tokio = { version = "1", features = ["full"] }
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::any,
    Router,
};

#[tokio::main]
async fn main() {
    let app = Router::new().route("/ws", any(ws_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on ws://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                // `Utf8Bytes` derefs to `&str`; reverse by chars.
                let reversed: String = text.chars().rev().collect();
                if socket.send(Message::text(reversed)).await.is_err() {
                    break;
                }
            }
            Message::Binary(bin) => {
                if socket.send(Message::Binary(bin)).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}
```

</details>

### Exercise 2: A broadcast chat room

**Difficulty:** Intermediate

**Objective:** Practice splitting the socket and sharing state across connections with a `broadcast` channel.

**Instructions:** Build a server with a single `/ws` route. Store a `tokio::sync::broadcast::Sender<String>` in `State`. On each connection, split the socket; spawn a task that forwards broadcast messages to the client, and a task that reads the client's text frames and publishes them (prefixed with `"user: "`) to the channel. Use `tokio::select!` so that when one task ends you abort the other. Announce `"* a user joined"` on connect and `"* a user left"` on disconnect.

<details>
<summary>Solution</summary>

```rust
// Cargo.toml: axum = { version = "0.8", features = ["ws"] }
//             tokio = { version = "1", features = ["full"] }
//             futures-util = "0.3"
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    routing::any,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<String>,
}

#[tokio::main]
async fn main() {
    let (tx, _rx) = broadcast::channel(64);
    let state = Arc::new(AppState { tx });
    let app = Router::new()
        .route("/ws", any(handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("chat on ws://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(|socket| chat(socket, state))
}

async fn chat(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let _ = state.tx.send("* a user joined".to_string());

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::text(msg)).await.is_err() {
                break;
            }
        }
    });

    let tx = state.tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let _ = tx.send(format!("user: {text}"));
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    let _ = state.tx.send("* a user left".to_string());
}
```

</details>

### Exercise 3: Typed JSON protocol with a graceful close

**Difficulty:** Advanced

**Objective:** Combine `serde` with the socket: parse a tagged-enum protocol off the wire, respond with typed messages, and close gracefully with a status code.

**Instructions:** Define a `ClientMsg` enum (`Chat { text }` and `Ping`) deserialized with `#[serde(tag = "type")]`, and a `ServerMsg` enum (`Echo { text }`, `Pong`, `Error { message }`). In the receive loop, parse each text frame as JSON: `Chat` replies with `Echo`, `Ping` replies with `Pong`, and a parse failure replies with `Error`. On close, send a `Message::Close` with code `1000` (normal) and a reason.

<details>
<summary>Solution</summary>

```rust
// Cargo.toml: axum = { version = "0.8", features = ["ws"] }
//             tokio = { version = "1", features = ["full"] }
//             serde = { version = "1", features = ["derive"] }
//             serde_json = "1"
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::{response::Response, routing::any, Router};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMsg {
    Chat { text: String },
    Ping,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMsg {
    Echo { text: String },
    Pong,
    Error { message: String },
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/ws", any(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on ws://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(socket_loop)
}

async fn socket_loop(mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        // Only act on text frames; break on a close frame.
        let Message::Text(raw) = msg else {
            if matches!(msg, Message::Close(_)) {
                break;
            }
            continue;
        };

        let reply = match serde_json::from_str::<ClientMsg>(&raw) {
            Ok(ClientMsg::Chat { text }) => ServerMsg::Echo { text },
            Ok(ClientMsg::Ping) => ServerMsg::Pong,
            Err(e) => ServerMsg::Error { message: e.to_string() },
        };

        let json = serde_json::to_string(&reply).unwrap();
        if socket.send(Message::text(json)).await.is_err() {
            return;
        }
    }

    // Graceful close with a status code and reason.
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: axum::extract::ws::close_code::NORMAL,
            reason: "bye".into(),
        })))
        .await;
}
```

</details>
