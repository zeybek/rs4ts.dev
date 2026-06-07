//! Shared, in-memory chat state: rooms, their members, and a broadcast
//! channel per room used to fan messages out to every connected socket.
//!
//! This is the Rust equivalent of the `Map<string, Set<Socket>>` you'd keep
//! in module scope in a Socket.IO server. The difference: it's shared across
//! many async tasks, so it lives behind an `Arc<Mutex<..>>`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::message::ServerMessage;

/// How many messages a slow client can fall behind before it starts dropping
/// frames. `tokio::sync::broadcast` keeps a ring buffer of this size per room.
const ROOM_CHANNEL_CAPACITY: usize = 256;

/// Everything one room needs: a fan-out channel and the current roster.
struct Room {
    /// Senders clone this to publish; each connected socket holds a receiver.
    tx: broadcast::Sender<ServerMessage>,
    /// Connected members as `(connection id, display name)` pairs. Keying on a
    /// unique per-connection id — not the display name — means two clients can
    /// share a name without one's disconnect evicting the other. A `Vec` is
    /// fine for chat-sized rooms and keeps roster ordering stable for the UI.
    members: Vec<(Uuid, String)>,
}

/// Display names of a room's members, in join order, for the roster frame.
fn roster(room: &Room) -> Vec<String> {
    room.members.iter().map(|(_, name)| name.clone()).collect()
}

/// The whole server's state. Cloning an `AppState` is cheap: it just bumps an
/// `Arc` refcount, so we hand a clone to every connection task and to axum.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Mutex<HashMap<String, Room>>>,
}

impl AppState {
    /// Create an empty registry. Rooms are created lazily on first join.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Join `conn_id` (a unique per-connection id) to `room` under the display
    /// name `username`, creating the room if needed.
    ///
    /// Returns a [`broadcast::Receiver`] the caller should read from to receive
    /// every future message in the room, plus the roster *after* joining. A
    /// socket that reconnects with the same `conn_id` just refreshes its name.
    pub fn join(
        &self,
        room: &str,
        conn_id: Uuid,
        username: &str,
    ) -> (broadcast::Receiver<ServerMessage>, Vec<String>) {
        let mut rooms = self.inner.lock().expect("room registry mutex poisoned");
        let entry = rooms.entry(room.to_string()).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(ROOM_CHANNEL_CAPACITY);
            Room {
                tx,
                members: Vec::new(),
            }
        });

        // One slot per connection. Reusing a known id refreshes the name;
        // otherwise the connection is appended (duplicate names are allowed).
        if let Some(slot) = entry.members.iter_mut().find(|(id, _)| *id == conn_id) {
            slot.1 = username.to_string();
        } else {
            entry.members.push((conn_id, username.to_string()));
        }

        // Subscribing *before* returning means the joining socket will see any
        // messages sent from this point on, including its own "joined" notice.
        let rx = entry.tx.subscribe();
        (rx, roster(entry))
    }

    /// Remove the connection `conn_id` from `room`. If the room becomes empty
    /// it's dropped from the map, which also drops its broadcast channel.
    /// Returns the remaining roster (empty if the room is gone).
    pub fn leave(&self, room: &str, conn_id: Uuid) -> Vec<String> {
        let mut rooms = self.inner.lock().expect("room registry mutex poisoned");
        let Some(entry) = rooms.get_mut(room) else {
            return Vec::new();
        };
        entry.members.retain(|(id, _)| *id != conn_id);
        if entry.members.is_empty() {
            rooms.remove(room);
            return Vec::new();
        }
        roster(entry)
    }

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
            .map(|(name, room)| (name.clone(), room.members.len()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_connections_sharing_a_name_are_tracked_independently() {
        let state = AppState::new();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());

        let (_rx_a, roster_a) = state.join("general", a, "alice");
        assert_eq!(roster_a, vec!["alice"]);

        // A second, distinct connection picks the SAME display name.
        let (_rx_b, roster_b) = state.join("general", b, "alice");
        assert_eq!(roster_b, vec!["alice", "alice"], "both connections must appear");

        // When one leaves, the other (same-named) connection must remain.
        let after = state.leave("general", a);
        assert_eq!(after, vec!["alice"], "the still-connected alice must survive");
    }

    #[test]
    fn last_member_leaving_drops_the_room() {
        let state = AppState::new();
        let conn = Uuid::new_v4();
        let (_rx, _) = state.join("room", conn, "bob");
        assert!(state.leave("room", conn).is_empty());
        assert!(state.snapshot().is_empty(), "empty room should be removed");
    }
}
