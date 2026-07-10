//! The WebSocket endpoint: upgrade handshake plus the per-connection task that
//! pumps messages in both directions.
//!
//! Each connected browser gets one of these tasks. Inside it we split the
//! socket into a write half and a read half and run two loops concurrently:
//!
//! * **outbound** — forward every [`ServerMessage`] from the room's broadcast
//!   channel down to this client.
//! * **inbound**  — parse [`ClientMessage`]s from the browser and turn them
//!   into broadcasts / roster changes.
//!
//! This mirrors how a Socket.IO connection handler works in Node, where you
//! register `socket.on("message", ...)` and call `io.to(room).emit(...)`.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::message::{ClientMessage, ServerMessage};
use crate::state::AppState;

/// Upper bound on a single chat line, so one client can't flood the room's
/// 256-slot broadcast ring with an oversized frame.
const MAX_CHAT_CHARS: usize = 2_000;

/// Axum handler for `GET /ws`. Performs the HTTP→WebSocket upgrade and hands
/// the live socket to [`handle_socket`].
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

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
                        handle_text(&state, conn_id, &mut current, &mut rx, &mut sender, &text).await;
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
        let roster = state.leave(&room, conn_id);
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

/// A write half of the split socket, named for readability.
type Sender = futures::stream::SplitSink<WebSocket, Message>;

/// Parse and act on one inbound text frame.
async fn handle_text(
    state: &AppState,
    conn_id: Uuid,
    current: &mut Option<(String, String)>,
    rx: &mut Option<broadcast::Receiver<ServerMessage>>,
    sender: &mut Sender,
    text: &str,
) {
    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(err) => {
            let _ = send(
                sender,
                ServerMessage::Error {
                    message: format!("invalid message: {err}"),
                },
            )
            .await;
            return;
        }
    };

    match msg {
        ClientMessage::Join { room, username } => {
            let room = room.trim();
            let username = username.trim();
            if room.is_empty() || username.is_empty() {
                let _ = send(
                    sender,
                    ServerMessage::Error {
                        message: "room and username must not be empty".to_string(),
                    },
                )
                .await;
                return;
            }

            // Leave any previous room first (a client can switch rooms).
            if let Some((prev_room, prev_user)) = current.take() {
                let roster = state.leave(&prev_room, conn_id);
                state.broadcast(
                    &prev_room,
                    ServerMessage::Left {
                        room: prev_room.clone(),
                        username: prev_user,
                    },
                );
                state.broadcast(
                    &prev_room,
                    ServerMessage::Roster {
                        room: prev_room.clone(),
                        users: roster,
                    },
                );
            }

            let (new_rx, users) = state.join(room, conn_id, username);
            *rx = Some(new_rx);
            *current = Some((room.to_string(), username.to_string()));

            // Tell *this* socket it's in, with the current roster...
            let _ = send(
                sender,
                ServerMessage::Welcome {
                    room: room.to_string(),
                    username: username.to_string(),
                    users: users.clone(),
                },
            )
            .await;
            // ...then announce the join to everyone (including us, via the
            // broadcast channel) and push the refreshed roster.
            state.broadcast(
                room,
                ServerMessage::Joined {
                    room: room.to_string(),
                    username: username.to_string(),
                },
            );
            state.broadcast(
                room,
                ServerMessage::Roster {
                    room: room.to_string(),
                    users,
                },
            );
        }

        ClientMessage::Chat { text } => {
            let Some((room, username)) = current.clone() else {
                let _ = send(
                    sender,
                    ServerMessage::Error {
                        message: "join a room before chatting".to_string(),
                    },
                )
                .await;
                return;
            };
            let text = text.trim();
            if text.is_empty() {
                return;
            }
            if text.chars().count() > MAX_CHAT_CHARS {
                let _ = send(
                    sender,
                    ServerMessage::Error {
                        message: format!("message too long (max {MAX_CHAT_CHARS} characters)"),
                    },
                )
                .await;
                return;
            }
            state.broadcast(
                &room,
                ServerMessage::Chat {
                    room: room.clone(),
                    username,
                    text: text.to_string(),
                    ts: now_millis(),
                },
            );
        }

        ClientMessage::Leave => {
            if let Some((room, username)) = current.take() {
                *rx = None;
                let roster = state.leave(&room, conn_id);
                state.broadcast(
                    &room,
                    ServerMessage::Left {
                        room: room.clone(),
                        username,
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
