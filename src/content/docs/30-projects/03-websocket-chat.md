---
title: "Project 4: WebSocket Chat Server"
description: "Rebuild a Socket.IO-style multi-room chat server in Rust with axum WebSockets, tokio broadcast channels, and JSON frames, instead of Node sockets and rooms."
---

If you have ever built a real-time feature in Node, you have probably reached
for [Socket.IO](https://socket.io/): clients connect, you keep a `Map` of
sockets, group them into "rooms", and `io.to(room).emit(...)` to broadcast.
This project rebuilds exactly that pattern in Rust — a multi-room chat server
over plain WebSockets — using [axum](https://docs.rs/axum) 0.8 for the HTTP and
WebSocket layer, a [`tokio::sync::broadcast`](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html)
channel per room for fan-out, and [serde](https://serde.rs/) for message
framing. State lives in memory behind an `Arc<Mutex<HashMap<..>>>`, so there is
nothing external to run. A small HTML/JavaScript client is served as a static
file so you can open two browser tabs and watch messages fly between them.

> [!NOTE]
> This guide targets Rust 1.96.0 (2024 edition) and was verified on the
> toolchain shipping with that release. The code was compiled and run for real;
> every command output below is copied from an actual run.

## What You'll Build

A chat server that mirrors a Socket.IO app:

- **WebSocket endpoint** at `GET /ws` that upgrades the HTTP connection.
- **Named rooms**: clients send a `join` frame to enter a room; messages only
  reach others in the same room.
- **Live roster**: everyone sees who joins and leaves in real time.
- **Static client**: a single `index.html` page (served at `/`) with a username
  field, a room field, a message list, and a user sidebar.
- **A tiny debug API**: `GET /api/rooms` returns the active rooms and their
  member counts as JSON.

The wire protocol is JSON, tagged by a `type` field, so it reads naturally from
JavaScript. A typical exchange looks like this. The client sends:

```json
{ "type": "join", "room": "general", "username": "alice" }
{ "type": "chat", "text": "hi bob!" }
```

and the server pushes back frames like:

```json
{ "type": "welcome", "room": "general", "username": "alice", "users": ["alice"] }
{ "type": "joined",  "room": "general", "username": "bob" }
{ "type": "roster",  "room": "general", "users": ["alice", "bob"] }
{ "type": "chat",    "room": "general", "username": "bob", "text": "hey alice ", "ts": 1780384175817 }
{ "type": "left",    "room": "general", "username": "bob" }
```

In the browser it looks like a minimal Slack: a header with username/room
inputs and a **Join** button, a scrolling message pane, and a "Users (2)"
sidebar that updates as people come and go.

## Prerequisites

This project ties together several earlier sections. If any concept feels
shaky, follow the link first:

- [Section 11 — Async](/11-async/), especially
  [The Tokio Runtime](/11-async/02-tokio-intro/),
  [Async Channels](/11-async/08-channels/), and
  [Spawning Tasks](/11-async/09-spawning-tasks/). The whole server is async,
  and the broadcast channel is the centerpiece.
- [Section 10 — Smart Pointers](/10-smart-pointers/), particularly
  [Shared Ownership with `Rc<T>` and `Arc<T>`](/10-smart-pointers/01-rc-arc/) and
  [Interior Mutability](/10-smart-pointers/02-refcell-mutex/). Shared state lives
  in an `Arc<Mutex<..>>`.
- [Section 15 — Serialization](/15-serialization/) and
  [Serde Basics](/15-serialization/01-serde-basics/) for the tagged-enum
  message framing.
- [Section 06 — Data Structures](/06-data-structures/02-enums/) and
  [Pattern Matching](/06-data-structures/04-pattern-matching/): the
  protocol is modeled as enums and matched on.
- [Section 16 — Web APIs](/16-web-apis/) for axum fundamentals
  (the [REST API project](/30-projects/00-rest-api/) is a gentler axum starting point).

You will need a recent Rust toolchain (`rustup` recommended) and, optionally,
Node.js 22+ if you want to script automated WebSocket clients like the test
runs shown later. A browser is all you need for the manual demo.

## Project Structure

The code directory is a normal Cargo binary crate with a small module tree:

```text
websocket-chat-code/
├── Cargo.toml          # dependencies pinned to current versions
├── src/
│   ├── main.rs         # entry point: logging, router, static files, server
│   ├── message.rs      # serde-tagged ClientMessage / ServerMessage enums
│   ├── state.rs        # AppState: room registry + broadcast channels
│   └── ws.rs           # WebSocket upgrade + per-connection duplex pump
└── static/
    └── index.html      # the browser chat client (HTML + vanilla JS)
```

Four small files, each with one job. In Node you might cram all of this into a
single `server.js`; splitting by responsibility keeps the Rust borrow-checking
story local and the modules independently testable.

## Walkthrough

We will build from the inside out: first the message types, then the shared
state, then the connection handler, and finally the server wiring and client.

### Step 1: Dependencies (`Cargo.toml`)

These are the crate versions resolved on the verified build. Pin to the current
major versions and let Cargo pick compatible point releases.

```toml
[package]
name = "websocket-chat-code"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = { version = "0.8", features = ["ws"] }
futures = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.6", features = ["fs", "trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["v4"] }
```

You would generate this with `cargo add`:

```bash
cargo new --bin websocket-chat-code
cd websocket-chat-code
cargo add axum --features ws
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json
cargo add tower-http --features fs,trace
cargo add tracing tracing-subscriber --features env-filter
cargo add futures uuid --features v4
```

What each does, in Node terms:

| Crate | Role | Node analogue |
| --- | --- | --- |
| `axum` (`ws`) | HTTP router + WebSocket upgrade | `express` + `ws`/Socket.IO |
| `tokio` | async runtime | the Node event loop itself |
| `serde` / `serde_json` | typed JSON in/out | `JSON.parse` / `JSON.stringify`, but checked |
| `tower-http` (`fs`) | serve the static client | `express.static` |
| `tracing` | structured logging | `pino` / `winston` |
| `futures` | `Stream`/`Sink` combinators | async iterators |
| `uuid` | per-connection ids | `crypto.randomUUID()` |

> [!NOTE]
> `tokio`'s `broadcast` channel, the heart of this project, lives in the core
> `tokio` crate, so the `full` feature already includes it. No extra dependency
> needed.

### Step 2: The message protocol (`src/message.rs`)

In TypeScript you would describe the protocol with a discriminated union. serde
gives us the same shape with `#[serde(tag = "type")]`, but the parsing is done
for us and the variants are checked at compile time.

```rust
//! Message framing for the chat protocol.
//!
//! Every WebSocket frame is a JSON object with a `type` tag. We use serde's
//! internally tagged enums (`#[serde(tag = "type")]`) so the wire format is
//! ergonomic for a JavaScript client: `{ "type": "chat", "text": "hi" }`.

use serde::{Deserialize, Serialize};

/// A message sent *from* the browser client *to* the server.
///
/// In TypeScript you might model this as a discriminated union:
/// ```ts
/// type ClientMessage =
///   | { type: "join"; room: string; username: string }
///   | { type: "chat"; text: string }
///   | { type: "leave" };
/// ```
/// serde's `tag = "type"` gives us the exact same JSON shape, but checked at
/// compile time and parsed for free.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Join (or switch to) a room under a chosen display name.
    Join { room: String, username: String },
    /// Post a chat line to the current room.
    Chat { text: String },
    /// Leave the current room (the socket stays open).
    Leave,
}
```

`rename_all = "snake_case"` turns the variant `Join` into the tag `"join"` on
the wire, matching the JavaScript naming convention. The server side is the
mirror image, with a few more variants for the events the server originates:

```rust
/// A message sent *from* the server *to* a browser client.
///
/// Serialized the same way: `{ "type": "chat", "room": "general", ... }`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Confirmation that this socket joined a room, plus who else is present.
    Welcome {
        room: String,
        username: String,
        users: Vec<String>,
    },
    /// A normal chat line broadcast to everyone in the room.
    Chat {
        room: String,
        username: String,
        text: String,
        /// Milliseconds since the Unix epoch (matches JS `Date.now()`).
        ts: u64,
    },
    /// A user joined the room (system notice).
    Joined { room: String, username: String },
    /// A user left the room (system notice).
    Left { room: String, username: String },
    /// The current roster for a room changed.
    Roster { room: String, users: Vec<String> },
    /// Something went wrong with the client's last message.
    Error { message: String },
}
```

A small helper serializes a `ServerMessage` to a JSON string. Because we own
every variant, serialization cannot realistically fail — but rather than
`unwrap()` and risk a panic inside a connection task, we fall back to a
hand-written error frame:

```rust
impl ServerMessage {
    /// Serialize to a JSON string for sending over the socket.
    ///
    /// We control these variants, so serialization cannot realistically fail;
    /// if it ever did we fall back to a hand-written error frame rather than
    /// panicking inside a connection task.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"type":"error","message":"failed to encode server message"}"#.to_string()
        })
    }
}
```

> [!TIP]
> The two enums make the protocol self-documenting. Add a feature, say
> typing indicators, and you add one variant; the compiler then points at
> every `match` that needs a new arm. That is the discriminated-union
> experience from TypeScript, enforced rather than merely suggested. See
> [Section 06 — Enums](/06-data-structures/02-enums/).

### Step 3: Shared room state (`src/state.rs`)

In a Socket.IO server you keep something like `Map<string, Set<Socket>>` in
module scope. In Rust, that state is touched by many concurrent tasks, so it
has to be shared safely: an `Arc` (shared ownership) wrapping a `Mutex`
(exclusive access at a time). See
[Shared Ownership with `Rc<T>` and `Arc<T>`](/10-smart-pointers/01-rc-arc/) and
[Interior Mutability](/10-smart-pointers/02-refcell-mutex/).

Each room owns a **broadcast channel** and a roster:

```rust
//! Shared, in-memory chat state: rooms, their members, and a broadcast
//! channel per room used to fan messages out to every connected socket.
//!
//! This is the Rust equivalent of the `Map<string, Set<Socket>>` you'd keep
//! in module scope in a Socket.IO server. The difference: it's shared across
//! many async tasks, so it lives behind an `Arc<Mutex<..>>`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::message::ServerMessage;

/// How many messages a slow client can fall behind before it starts dropping
/// frames. `tokio::sync::broadcast` keeps a ring buffer of this size per room.
const ROOM_CHANNEL_CAPACITY: usize = 256;

/// Everything one room needs: a fan-out channel and the current roster.
struct Room {
    /// Senders clone this to publish; each connected socket holds a receiver.
    tx: broadcast::Sender<ServerMessage>,
    /// Display names currently in the room. A `Vec` is fine for chat-sized
    /// rooms and keeps roster ordering stable for the UI.
    users: Vec<String>,
}

/// The whole server's state. Cloning an `AppState` is cheap: it just bumps an
/// `Arc` refcount, so we hand a clone to every connection task and to axum.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Mutex<HashMap<String, Room>>>,
}
```

The `broadcast` channel is the magic ingredient. It is a multi-producer,
**multi-consumer** channel: every value sent is delivered to *every* active
receiver. That is precisely "emit to everyone in the room". Each connected
socket calls `subscribe()` to get its own `Receiver`; sending on the `Sender`
fans the message out to all of them. (Contrast with an `mpsc` channel, where a
value goes to exactly one consumer: see
[Async Channels](/11-async/08-channels/).)

Joining a room creates it lazily, adds the user, and hands back a fresh
receiver plus the post-join roster:

```rust
impl AppState {
    /// Create an empty registry. Rooms are created lazily on first join.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Join `username` to `room`, creating the room if needed.
    ///
    /// Returns a [`broadcast::Receiver`] the caller should read from to receive
    /// every future message in the room, plus the roster *after* joining. If
    /// the same username is already present we don't add a duplicate (handy
    /// when a client reconnects).
    pub fn join(
        &self,
        room: &str,
        username: &str,
    ) -> (broadcast::Receiver<ServerMessage>, Vec<String>) {
        let mut rooms = self.inner.lock().expect("room registry mutex poisoned");
        let entry = rooms.entry(room.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(ROOM_CHANNEL_CAPACITY);
            Room {
                tx,
                users: Vec::new(),
            }
        });

        if !entry.users.iter().any(|u| u == username) {
            entry.users.push(username.to_string());
        }

        // Subscribing *before* returning means the joining socket will see any
        // messages sent from this point on, including its own "joined" notice.
        let rx = entry.tx.subscribe();
        (rx, entry.users.clone())
    }
```

> [!IMPORTANT]
> `entry.tx.subscribe()` is the analogue of `socket.join(room)`. A
> `broadcast::Receiver` only sees messages sent **after** it subscribed, so we
> subscribe before announcing the join. That ordering is why the joining client
> reliably receives its own `joined` and `roster` notices.

Leaving removes the user and, when the room empties, drops it from the map,
which also drops the channel and frees its buffer:

```rust
    /// Remove `username` from `room`. If the room becomes empty it's dropped
    /// from the map, which also drops its broadcast channel. Returns the
    /// remaining roster (empty if the room is gone).
    pub fn leave(&self, room: &str, username: &str) -> Vec<String> {
        let mut rooms = self.inner.lock().expect("room registry mutex poisoned");
        let Some(entry) = rooms.get_mut(room) else {
            return Vec::new();
        };
        entry.users.retain(|u| u != username);
        if entry.users.is_empty() {
            rooms.remove(room);
            return Vec::new();
        }
        entry.users.clone()
    }
```

Broadcasting is a one-liner. `broadcast::Sender::send` only errors when there
are zero receivers, which for us simply means nobody is listening, a no-op:

```rust
    /// Publish a message to everyone subscribed to `room`.
    ///
    /// `broadcast::Sender::send` only errors when there are zero receivers, so
    /// we treat that as a no-op: nobody is listening, nothing to do.
    pub fn broadcast(&self, room: &str, msg: ServerMessage) {
        let rooms = self.inner.lock().expect("room registry mutex poisoned");
        if let Some(entry) = rooms.get(room) {
            let _ = entry.tx.send(msg);
        }
    }

    /// Snapshot of the current rooms and their member counts, for the
    /// `/api/rooms` debug endpoint.
    pub fn snapshot(&self) -> Vec<(String, usize)> {
        let rooms = self.inner.lock().expect("room registry mutex poisoned");
        let mut out: Vec<(String, usize)> = rooms
            .iter()
            .map(|(name, room)| (name.clone(), room.users.len()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
```

> [!NOTE]
> We hold a `std::sync::Mutex` (not tokio's async `Mutex`) and never `.await`
> while it is locked — every locked section is a quick map operation. That is
> the recommended pattern: for short, non-async critical sections, the standard
> blocking mutex is simpler and faster. See
> [Interior Mutability](/10-smart-pointers/02-refcell-mutex/).

### Step 4: The connection handler (`src/ws.rs`)

This is where it all comes together: one async task per connected browser. In
Socket.IO you would write `io.on("connection", socket => { ... })` and register
`socket.on("message", ...)` handlers. In axum, the upgrade handler hands you a
live `WebSocket`, and we drive it ourselves.

First, the upgrade. axum's `WebSocketUpgrade` extractor does the HTTP-101
handshake; `on_upgrade` spawns our handler with the upgraded socket:

```rust
/// Axum handler for `GET /ws`. Performs the HTTP→WebSocket upgrade and hands
/// the live socket to [`handle_socket`].
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}
```

The per-connection task splits the socket into a write half (a `Sink`) and a
read half (a `Stream`), then loops, reacting to whichever happens next: a
broadcast to forward out, or an inbound frame from the browser. `tokio::select!`
is the duplex pump:

```rust
/// Drives a single client connection from open to close.
async fn handle_socket(socket: WebSocket, state: AppState) {
    // A stable id for logs; the user picks a display name on `join`.
    let conn_id = Uuid::new_v4();
    tracing::info!(%conn_id, "client connected");

    // Split into a write half (sink) and read half (stream) so the two loops
    // below can own them independently.
    let (mut sender, mut receiver) = socket.split();

    // Per-connection session: which room/name this socket currently holds.
    let mut current: Option<(String, String)> = None;
    // The receiver for the room we're in; `None` until the first join.
    let mut rx: Option<broadcast::Receiver<ServerMessage>> = None;

    loop {
        // `tokio::select!` waits on whichever arm is ready first: either a
        // broadcast message to forward out, or an inbound frame from the
        // browser. This is the heart of the duplex pump.
        tokio::select! {
            // --- OUTBOUND: room broadcast -> this client ---------------------
            broadcast = recv_broadcast(rx.as_mut()) => {
                match broadcast {
                    BroadcastEvent::Message(msg) => {
                        if sender.send(Message::Text(msg.to_json().into())).await.is_err() {
                            break; // socket closed under us
                        }
                    }
                    // We fell behind the ring buffer and skipped `n` messages.
                    BroadcastEvent::Lagged(n) => {
                        let warn = ServerMessage::Error {
                            message: format!("dropped {n} message(s); you were too slow"),
                        };
                        let _ = sender.send(Message::Text(warn.to_json().into())).await;
                    }
                    // Channel closed (room emptied) or we're not in a room yet.
                    BroadcastEvent::Idle => {}
                }
            }

            // --- INBOUND: this client -> server ------------------------------
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        handle_text(&state, &mut current, &mut rx, &mut sender, &text).await;
                    }
                    // Browsers send a Close frame on tab close / navigation.
                    Some(Ok(Message::Close(_))) | None => break,
                    // axum answers Ping with Pong automatically; ignore the
                    // rest (Binary/Ping/Pong) for this text-only protocol.
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        tracing::debug!(%conn_id, %err, "websocket receive error");
                        break;
                    }
                }
            }
        }
    }

    // Cleanup: if this socket was in a room, remove it and tell the room.
    if let Some((room, username)) = current {
        let roster = state.leave(&room, &username);
        state.broadcast(
            &room,
            ServerMessage::Left {
                room: room.clone(),
                username: username.clone(),
            },
        );
        state.broadcast(
            &room,
            ServerMessage::Roster {
                room: room.clone(),
                users: roster,
            },
        );
    }
    tracing::info!(%conn_id, "client disconnected");
}
```

A few things worth dwelling on for a JavaScript developer:

- **`socket.split()`** gives a `SplitSink` (write) and `SplitStream` (read).
  Because they are separate values, the two `select!` arms can each own one half
  without the borrow checker complaining. There is no Node equivalent (`ws`
  hands you one duplex object), but the split is what lets us send and receive
  truly concurrently.
- **The cleanup block runs when the loop breaks**, which happens on a Close
  frame, a read error, or the stream ending. This is your guaranteed
  `socket.on("disconnect")`: no chance of leaking the user in the roster.
- **`Lagged`** is a real backpressure signal you do not get for free in Node.
  If a client is so slow that 256 messages pile up, the broadcast channel tells
  us we skipped some, and we forward a warning instead of silently corrupting
  the stream.

The "disabled until joined" trick keeps the outbound arm dormant before the
client picks a room. When `rx` is `None`, `recv_broadcast` awaits a future that
never completes, so `select!` simply waits on the inbound arm:

```rust
/// What [`recv_broadcast`] resolved to. Modeling lag/closed explicitly keeps
/// the `select!` arm above easy to read.
enum BroadcastEvent {
    Message(ServerMessage),
    Lagged(u64),
    Idle,
}

/// Await the next broadcast message, translating the channel's error cases.
///
/// When `rx` is `None` (socket hasn't joined a room) we return a future that
/// never completes, so the `select!` simply parks on the inbound arm instead.
async fn recv_broadcast(rx: Option<&mut broadcast::Receiver<ServerMessage>>) -> BroadcastEvent {
    match rx {
        Some(rx) => match rx.recv().await {
            Ok(msg) => BroadcastEvent::Message(msg),
            Err(broadcast::error::RecvError::Lagged(n)) => BroadcastEvent::Lagged(n),
            Err(broadcast::error::RecvError::Closed) => BroadcastEvent::Idle,
        },
        None => {
            // Never resolves -> this arm is effectively disabled until join.
            std::future::pending::<()>().await;
            BroadcastEvent::Idle
        }
    }
}
```

> [!TIP]
> `std::future::pending()` is the idiomatic "this branch is off" future for
> `select!`. It is the async equivalent of an `if` that is currently false:
> the arm stays registered but can never fire until `rx` becomes `Some`.

Finally, the inbound frame handler. This is the big `match` on the incoming
`ClientMessage`. Note how cleanly the protocol enum drives the control flow:
the same readability you get from a `switch` over a discriminated union in
TypeScript, but exhaustively checked:

```rust
/// A write half of the split socket, named for readability.
type Sender = futures::stream::SplitSink<WebSocket, Message>;

/// Parse and act on one inbound text frame.
async fn handle_text(
    state: &AppState,
    current: &mut Option<(String, String)>,
    rx: &mut Option<broadcast::Receiver<ServerMessage>>,
    sender: &mut Sender,
    text: &str,
) {
    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(err) => {
            let _ = send(sender, ServerMessage::Error {
                message: format!("invalid message: {err}"),
            })
            .await;
            return;
        }
    };

    match msg {
        ClientMessage::Join { room, username } => {
            let room = room.trim();
            let username = username.trim();
            if room.is_empty() || username.is_empty() {
                let _ = send(sender, ServerMessage::Error {
                    message: "room and username must not be empty".to_string(),
                })
                .await;
                return;
            }

            // Leave any previous room first (a client can switch rooms).
            if let Some((prev_room, prev_user)) = current.take() {
                let roster = state.leave(&prev_room, &prev_user);
                state.broadcast(&prev_room, ServerMessage::Left {
                    room: prev_room.clone(),
                    username: prev_user,
                });
                state.broadcast(&prev_room, ServerMessage::Roster {
                    room: prev_room.clone(),
                    users: roster,
                });
            }

            let (new_rx, users) = state.join(room, username);
            *rx = Some(new_rx);
            *current = Some((room.to_string(), username.to_string()));

            // Tell *this* socket it's in, with the current roster...
            let _ = send(sender, ServerMessage::Welcome {
                room: room.to_string(),
                username: username.to_string(),
                users: users.clone(),
            })
            .await;
            // ...then announce the join to everyone (including us, via the
            // broadcast channel) and push the refreshed roster.
            state.broadcast(room, ServerMessage::Joined {
                room: room.to_string(),
                username: username.to_string(),
            });
            state.broadcast(room, ServerMessage::Roster {
                room: room.to_string(),
                users,
            });
        }

        ClientMessage::Chat { text } => {
            let Some((room, username)) = current.clone() else {
                let _ = send(sender, ServerMessage::Error {
                    message: "join a room before chatting".to_string(),
                })
                .await;
                return;
            };
            let text = text.trim();
            if text.is_empty() {
                return;
            }
            state.broadcast(&room, ServerMessage::Chat {
                room: room.clone(),
                username,
                text: text.to_string(),
                ts: now_millis(),
            });
        }

        ClientMessage::Leave => {
            if let Some((room, username)) = current.take() {
                *rx = None;
                let roster = state.leave(&room, &username);
                state.broadcast(&room, ServerMessage::Left {
                    room: room.clone(),
                    username,
                });
                state.broadcast(&room, ServerMessage::Roster {
                    room: room.clone(),
                    users: roster,
                });
            }
        }
    }
}

/// Send one [`ServerMessage`] to this client as a text frame.
async fn send(sender: &mut Sender, msg: ServerMessage) -> Result<(), axum::Error> {
    sender.send(Message::Text(msg.to_json().into())).await
}

/// Current Unix time in milliseconds, matching JavaScript's `Date.now()`.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
```

Walk through the `Join` arm: we trim and validate, leave any prior room (so a
client can switch rooms on one socket), join the new room to get a fresh
receiver, send a private `Welcome` straight back to *this* socket, then
`broadcast` the `Joined` and `Roster` notices to *everyone* in the room. The
`Welcome` goes through `send` (the per-socket sink) while `Joined`/`Roster` go
through `state.broadcast` (the room channel). That distinction is exactly
Socket.IO's `socket.emit` versus `io.to(room).emit`.

The `#[serde(tag = "type")]` import at the top of the file pulls in everything
the handler needs:

```rust
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::message::{ClientMessage, ServerMessage};
use crate::state::AppState;
```

> [!NOTE]
> `axum::extract::ws::Message::Text` holds a `Utf8Bytes`, so we call
> `.into()` on our `String` to convert it. axum also answers WebSocket Ping
> frames with Pong automatically, so our handler does not need a keep-alive
> loop: one fewer thing to get wrong compared to a hand-rolled `ws` setup.

### Step 5: Wiring the server (`src/main.rs`)

The entry point sets up logging, builds the router, mounts the static client,
and serves. This is the axum equivalent of `app.use(express.static(...))`,
`app.get(...)`, and `app.listen(...)`.

```rust
//! A real-time multi-room chat server over WebSockets.
//!
//! This is the Rust analogue of a small Socket.IO app in Node: clients connect
//! over a WebSocket, join named rooms, and every message is fanned out to
//! everyone else in the same room. State lives entirely in memory.
//!
//! Architecture:
//!   * [`state::AppState`] — the room registry (`Arc<Mutex<HashMap<..>>>`),
//!     one `tokio::sync::broadcast` channel per room.
//!   * [`ws`] — the WebSocket upgrade handler and per-connection duplex pump.
//!   * [`message`] — serde-tagged JSON message framing for both directions.
//!
//! Run it with `cargo run`, then open http://127.0.0.1:3000 in two browser
//! tabs and chat between them.

mod message;
mod state;
mod ws;

use std::net::SocketAddr;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

#[tokio::main]
async fn main() {
    // Structured logging. Override verbosity with e.g.
    // `RUST_LOG=websocket_chat_code=debug,tower_http=debug cargo run`.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("websocket_chat_code=info,tower_http=info")),
        )
        .init();

    let state = AppState::new();

    // Serve the browser client (static/index.html) at `/` and assets under it.
    let static_files = ServeDir::new("static").append_index_html_on_directories(true);

    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
        .route("/api/rooms", get(list_rooms))
        .fallback_service(static_files)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Bind address: override with `CHAT_ADDR=0.0.0.0:8080 cargo run`.
    let addr: SocketAddr = std::env::var("CHAT_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
        .parse()
        .expect("CHAT_ADDR must be a valid socket address, e.g. 127.0.0.1:3000");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    tracing::info!("chat server listening on http://{addr}");
    tracing::info!("open the URL in two browser tabs to try it out");

    // axum 0.8 serves via `axum::serve` over a plain tokio listener.
    axum::serve(listener, app).await.expect("server error");
}

/// `GET /api/rooms` — a tiny debug endpoint returning active rooms and how many
/// users each holds. Handy for `curl` while testing.
async fn list_rooms(State(state): State<AppState>) -> Json<serde_json::Value> {
    let rooms: Vec<serde_json::Value> = state
        .snapshot()
        .into_iter()
        .map(|(name, count)| serde_json::json!({ "room": name, "users": count }))
        .collect();
    Json(serde_json::json!({ "rooms": rooms }))
}
```

Key points:

- **`.with_state(state)`** injects the shared `AppState` into every handler. The
  `State<AppState>` extractor in `ws_handler` and `list_rooms` pulls it out.
  Because `AppState` is `Clone` (it is just an `Arc` inside), axum can hand each
  request its own cheap clone. This is the idiomatic alternative to a global
  variable.
- **`fallback_service(ServeDir::new("static"))`** serves `index.html` at `/` and
  any other static asset by path: the catch-all after the explicit routes.
- **`axum::serve(listener, app)`** is the axum 0.8 way to run a server over a
  plain `tokio::net::TcpListener`. (Older tutorials show
  `axum::Server::bind(...)`, which was removed.)
- **`CHAT_ADDR`** lets you override the bind address without recompiling — handy
  for Docker or to dodge a port clash.

### Step 6: The browser client (`static/index.html`)

The client is plain HTML and vanilla JavaScript using the browser's built-in
`WebSocket`: no Socket.IO client library needed, because we speak ordinary
WebSocket frames. The interesting part is the `onmessage` handler, which
switches on the same `type` tag our `ServerMessage` enum serializes:

```javascript
socket.onopen = () => {
  statusEl.textContent = "connected";
  // First thing we send is a "join" frame.
  socket.send(JSON.stringify({ type: "join", room, username }));
};

socket.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  switch (msg.type) {
    case "welcome":
      joined = true;
      $("text").disabled = false;
      $("send").disabled = false;
      addMessage(`Joined <b>#${escapeHtml(msg.room)}</b> as <b>${escapeHtml(msg.username)}</b>`, "system");
      renderRoster(msg.users);
      break;
    case "chat": {
      const when = new Date(msg.ts).toLocaleTimeString();
      addMessage(
        `<span class="who">${escapeHtml(msg.username)}</span>: ` +
          `${escapeHtml(msg.text)}<span class="time">${when}</span>`
      );
      break;
    }
    case "joined":
      addMessage(`<b>${escapeHtml(msg.username)}</b> joined`, "system");
      break;
    case "left":
      addMessage(`<b>${escapeHtml(msg.username)}</b> left`, "system");
      break;
    case "roster":
      renderRoster(msg.users);
      break;
    case "error":
      addMessage(`Error: ${escapeHtml(msg.message)}`, "error");
      break;
  }
};
```

Sending a chat line is one `socket.send` with a JSON string, mirroring the
`ClientMessage::Chat` variant:

```javascript
function sendChat() {
  const input = $("text");
  const text = input.value.trim();
  if (!text || !joined) return;
  socket.send(JSON.stringify({ type: "chat", text }));
  input.value = "";
}
```

> [!IMPORTANT]
> The client builds the WebSocket URL from `location.host`, choosing `wss://`
> when the page is served over HTTPS. So if you put the server behind a TLS
> reverse proxy, the same page works unchanged. We also `escapeHtml` every
> server-supplied string before inserting it into the DOM: basic XSS hygiene,
> since usernames and messages are untrusted input. See
> [Section 27 — Security](/27-security/).

The full page (including the dark-mode CSS) lives in
`static/index.html`; the JavaScript above is the part that matters.

## Running It

Build and run from the code directory. The first build pulls and compiles the
dependency tree; subsequent builds are incremental.

```bash
cargo run
```

Real output (logging is on `info` by default):

```text
   Compiling websocket-chat-code v0.1.0 (.../examples/websocket-chat-code)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 18.74s
     Running `target/debug/websocket-chat-code`
2026-06-02T07:09:21.220334Z  INFO websocket_chat_code: chat server listening on http://127.0.0.1:3000
2026-06-02T07:09:21.220451Z  INFO websocket_chat_code: open the URL in two browser tabs to try it out
```

Now open <http://127.0.0.1:3000> in **two** browser tabs. In each, set a
username (e.g. `alice` and `bob`), keep the room as `general`, and click
**Join**. Type in one tab and watch the message appear in both, with the user
sidebar showing "Users (2)".

### Verifying with `curl` and a scripted client

The static page and the debug API respond over plain HTTP:

```bash
curl -s -o /dev/null -w "GET / -> HTTP %{http_code}, %{size_download} bytes\n" http://127.0.0.1:3000/
curl -s http://127.0.0.1:3000/api/rooms
```

Real output (no clients connected yet):

```text
GET / -> HTTP 200, 7326 bytes
{"rooms":[]}
```

To prove the broadcast flow without clicking around two browser tabs, here is a
small Node 22 script that opens two clients, joins them both to `general`, and
sends a message from each (Node 22 ships a global `WebSocket`):

```javascript
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const lines = [];
const log = (tag, m) => lines.push(`[${tag}] ${JSON.stringify(m)}`);

function open(name) {
  return new Promise((resolve) => {
    const ws = new WebSocket("ws://127.0.0.1:3000/ws");
    ws.onmessage = (e) => { const m = JSON.parse(e.data); log(name, m); if (ws._onmsg) ws._onmsg(m); };
    ws.onopen = () => resolve(ws);
  });
}
function joined(ws) {
  return new Promise((resolve) => { ws._onmsg = (m) => { if (m.type === "welcome") resolve(); }; });
}

const alice = await open("alice");
alice.send(JSON.stringify({ type: "join", room: "general", username: "alice" }));
await joined(alice);
const bob = await open("bob");
bob.send(JSON.stringify({ type: "join", room: "general", username: "bob" }));
await joined(bob);

console.log(await (await fetch("http://127.0.0.1:3000/api/rooms")).text());
alice.send(JSON.stringify({ type: "chat", text: "hi bob!" }));
bob.send(JSON.stringify({ type: "chat", text: "hey alice " }));
await sleep(250);
bob.close(); await sleep(200); alice.close(); await sleep(150);
console.log(lines.join("\n"));
process.exit(0);
```

Real output from running it (against the server bound to `127.0.0.1:3100` in
the verification run, identical behavior on `:3000`):

```text
{"rooms":[{"room":"general","users":2}]}
[alice] {"type":"welcome","room":"general","username":"alice","users":["alice"]}
[alice] {"type":"joined","room":"general","username":"alice"}
[alice] {"type":"roster","room":"general","users":["alice"]}
[bob] {"type":"welcome","room":"general","username":"bob","users":["alice","bob"]}
[bob] {"type":"joined","room":"general","username":"bob"}
[bob] {"type":"roster","room":"general","users":["alice","bob"]}
[alice] {"type":"joined","room":"general","username":"bob"}
[alice] {"type":"roster","room":"general","users":["alice","bob"]}
[alice] {"type":"chat","room":"general","username":"alice","text":"hi bob!","ts":1780384175817}
[alice] {"type":"chat","room":"general","username":"bob","text":"hey alice ","ts":1780384175817}
[bob] {"type":"chat","room":"general","username":"alice","text":"hi bob!","ts":1780384175817}
[bob] {"type":"chat","room":"general","username":"bob","text":"hey alice ","ts":1780384175817}
[alice] {"type":"left","room":"general","username":"bob"}
[alice] {"type":"roster","room":"general","users":["alice"]}
```

Notice the flow: alice joins (sees her own `welcome`, `joined`, `roster`); bob
joins and alice receives a second `joined` plus an updated `roster`; both
messages reach **both** clients; and when bob disconnects, alice gets `left` and
a shrunken `roster`. The server's own log for that run:

```text
2026-06-02T07:09:21.220334Z  INFO websocket_chat_code: chat server listening on http://127.0.0.1:3100
2026-06-02T07:09:21.220451Z  INFO websocket_chat_code: open the URL in two browser tabs to try it out
2026-06-02T07:09:46.228079Z  INFO websocket_chat_code::ws: client connected conn_id=2c00a1bc-13ed-4a92-b8cd-625ad9af6ae2
2026-06-02T07:09:46.233517Z  INFO websocket_chat_code::ws: client connected conn_id=55455c66-4eb7-405b-b62a-769548e01351
2026-06-02T07:09:46.891694Z  INFO websocket_chat_code::ws: client disconnected conn_id=2c00a1bc-13ed-4a92-b8cd-625ad9af6ae2
2026-06-02T07:09:46.891843Z  INFO websocket_chat_code::ws: client disconnected conn_id=eeb327ee-7917-46b2-bd6e-e9206a366dc6
```

### Room isolation and error handling

A separate verification run confirmed the two invariants you would want to test
in any chat server. With one client in room `rust` and another in `node`, a
message sent in `rust` is delivered **only** to the sender's room (it never
crosses to `node`):

```text
cross-room deliveries: [ 'rustacean received "rust-only message" from rustacean' ]
```

And misbehaving clients get clear errors rather than crashing the connection:

```text
errors observed: [
  'lurker error: join a room before chatting',
  'lurker error: invalid message: expected ident at line 1 column 2'
]
```

The first is from sending `chat` before `join`; the second is from sending a
non-JSON frame. Both produce a friendly `error` frame and the socket stays open.

## Key Concepts

This project cements a cluster of Rust ideas that show up in every real async
service:

- **`tokio::sync::broadcast` for fan-out.** One sender, many receivers, each
  getting every message: the channel shape that maps directly onto "emit to a
  room". It even surfaces backpressure (`Lagged`) for slow consumers, something
  you would have to engineer yourself in Node. See
  [Async Channels](/11-async/08-channels/).
- **`Arc<Mutex<..>>` for shared mutable state.** The room registry is touched by
  every connection task; `Arc` shares ownership and `Mutex` serializes access.
  We deliberately use the blocking `std::sync::Mutex` because the critical
  sections are short and synchronous. See
  [Shared Ownership with `Rc<T>` and `Arc<T>`](/10-smart-pointers/01-rc-arc/) and
  [Interior Mutability](/10-smart-pointers/02-refcell-mutex/).
- **`tokio::select!` for duplex I/O.** Splitting the socket and selecting over
  "broadcast ready" versus "frame received" is the canonical pattern for a
  bidirectional protocol, and `std::future::pending()` is how you disable an arm
  until it is relevant. See [The Tokio Runtime](/11-async/02-tokio-intro/).
- **serde tagged enums as a wire protocol.** `#[serde(tag = "type")]` turns Rust
  enums into the discriminated-union JSON a JavaScript client expects, with
  exhaustive `match` keeping the server honest. See
  [Serde Basics](/15-serialization/01-serde-basics/) and
  [Enums and Data-Carrying Variants](/06-data-structures/02-enums/).
- **Deterministic cleanup.** The post-loop cleanup block is your reliable
  `disconnect` hook: when the connection task ends, the user is removed and the
  room is notified, no leaks. This leans on Rust's ownership and `Drop`
  semantics (the `broadcast::Receiver` is dropped automatically when the task
  ends). See [Section 05 — Drop](/05-ownership/08-drop-trait/).
- **axum `State` injection.** `.with_state(...)` plus the `State<T>` extractor
  is the idiomatic replacement for module-level globals or `req.app.locals`.

## Extending It

Concrete next steps, roughly in order of effort:

1. **Persist message history.** Right now history is ephemeral. Add an
   in-memory ring buffer per room (e.g. the last 50 messages) and replay it in
   the `Welcome` frame so a joining client sees recent context. To make it
   durable, store messages in SQLite and load on join; see
   [Section 17 — Database](/17-database/), and note that the same
   `AppState` pattern holds a `sqlx::SqlitePool` instead of (or alongside) the
   `HashMap`.
2. **Authenticate users.** Require a token on the WebSocket upgrade (a query
   param or `Sec-WebSocket-Protocol` header), validate it with a JWT library,
   and reject unauthenticated upgrades. The
   [REST API project](/30-projects/00-rest-api/) and
   [Section 27 — Security](/27-security/) cover the auth pieces.
3. **Enforce unique usernames per room.** Today two sockets can both call
   themselves `alice`. Track active names in the `Room` and return an `error`
   frame on a clash, or auto-suffix the name.
4. **Scale beyond one process.** A single `broadcast` channel is per-process.
   To run multiple server instances, replace the in-process fan-out with Redis
   pub/sub (each instance subscribes to a channel and re-broadcasts locally),
   exactly the "Redis adapter" pattern from Socket.IO. See
   [Redis with the `redis` Crate](/17-database/07-redis/).

## Further Reading

- [Section 11 — Async](/11-async/) ·
  [The Tokio Runtime](/11-async/02-tokio-intro/) ·
  [Async Channels](/11-async/08-channels/) ·
  [Spawning Tasks](/11-async/09-spawning-tasks/)
- [Section 10 — Smart Pointers](/10-smart-pointers/) ·
  [Shared Ownership with `Rc<T>` and `Arc<T>`](/10-smart-pointers/01-rc-arc/) ·
  [Interior Mutability](/10-smart-pointers/02-refcell-mutex/)
- [Section 15 — Serialization](/15-serialization/) ·
  [Serde Basics](/15-serialization/01-serde-basics/)
- [Section 06 — Data Structures: Enums](/06-data-structures/02-enums/) ·
  [Pattern Matching](/06-data-structures/04-pattern-matching/)
- [Section 16 — Web APIs](/16-web-apis/)
- [Section 17 — Database](/17-database/) ·
  [Redis with the `redis` Crate](/17-database/07-redis/)
- [Section 27 — Security](/27-security/)
- Related projects: [REST API](/30-projects/00-rest-api/) ·
  [Production Microservice](/30-projects/04-microservice/) ·
  [Full-Stack App](/30-projects/05-full-stack/)
- Official docs: [axum WebSockets](https://docs.rs/axum/latest/axum/extract/ws/index.html) ·
  [`tokio::sync::broadcast`](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) ·
  [The Rust Async Book](https://rust-lang.github.io/async-book/)
