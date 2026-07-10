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
