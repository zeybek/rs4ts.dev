//! The persistence layer, behind a trait so the backing store is swappable.
//!
//! The default [`InMemoryStore`] keeps everything in an `Arc<RwLock<HashMap>>`,
//! which makes the service runnable with zero external dependencies. In
//! production you would implement [`Store`] over Redis or Postgres instead —
//! see `../17-database/07_redis.md` and `../17-database/README.md`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use rand::Rng;

use crate::error::AppError;

/// Behaviour every storage backend must provide.
///
/// The methods are synchronous because the in-memory `RwLock` never awaits.
/// A Redis-backed implementation would make these `async` (or wrap a
/// connection pool) — the handlers depend only on this trait, not the concrete
/// type, so swapping backends does not ripple through the codebase.
pub trait Store: Send + Sync + 'static {
    /// Store `target` under `code`. Returns `Ok(true)` if it was stored, or
    /// `Ok(false)` if `code` was already taken — so the caller can retry with a
    /// fresh code instead of silently overwriting the existing link.
    fn insert(&self, code: String, target: String) -> Result<bool, AppError>;

    /// Look up the original URL for a short `code`, bumping the global redirect
    /// counter. Takes only a read lock, so many redirects resolve concurrently.
    fn resolve(&self, code: &str) -> Result<Option<String>, AppError>;

    /// Total number of links currently stored.
    ///
    /// This also doubles as the readiness probe's connectivity check: for the
    /// in-memory store it just acquires the lock, but a Redis-backed
    /// implementation would `PING` here, and an `Err` makes `/ready` answer
    /// `503` so the orchestrator stops routing to a broken instance.
    fn len(&self) -> Result<usize, AppError>;

    /// Whether the store holds no links. (Pairs with [`len`](Store::len).)
    fn is_empty(&self) -> Result<bool, AppError> {
        Ok(self.len()? == 0)
    }
}

/// In-memory [`Store`] backed by an `Arc<RwLock<HashMap>>` mapping each short
/// code to its target URL.
///
/// `Arc` lets every request handler share one store cheaply; `RwLock` allows
/// many concurrent readers (redirects) and exclusive writers (new links). A
/// separate atomic counts total redirects so the read path never needs a write
/// lock.
#[derive(Clone, Default)]
pub struct InMemoryStore {
    inner: Arc<RwLock<HashMap<String, String>>>,
    redirects: Arc<AtomicU64>,
}

impl InMemoryStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Total redirects served since startup (a cheap liveness/traffic signal).
    pub fn redirect_count(&self) -> u64 {
        self.redirects.load(Ordering::Relaxed)
    }

    /// Generate a random base62 short code of `len` characters.
    pub fn generate_code(len: usize) -> String {
        const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = rand::rng();
        (0..len)
            .map(|_| {
                let idx = rng.random_range(0..ALPHABET.len());
                ALPHABET[idx] as char
            })
            .collect()
    }
}

impl Store for InMemoryStore {
    fn insert(&self, code: String, target: String) -> Result<bool, AppError> {
        let mut map = self.inner.write().map_err(|_| AppError::Store)?;
        if map.contains_key(&code) {
            return Ok(false); // collision — let the caller pick a new code
        }
        map.insert(code, target);
        Ok(true)
    }

    fn resolve(&self, code: &str) -> Result<Option<String>, AppError> {
        // A read lock lets concurrent redirects resolve in parallel; the hit
        // counter is a separate atomic, so no exclusive access is needed.
        let map = self.inner.read().map_err(|_| AppError::Store)?;
        match map.get(code) {
            Some(target) => {
                self.redirects.fetch_add(1, Ordering::Relaxed);
                Ok(Some(target.clone()))
            }
            None => Ok(None),
        }
    }

    fn len(&self) -> Result<usize, AppError> {
        let map = self.inner.read().map_err(|_| AppError::Store)?;
        Ok(map.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_then_resolve_roundtrips() {
        let store = InMemoryStore::new();
        store
            .insert("abc123".into(), "https://example.com".into())
            .unwrap();

        assert_eq!(store.len().unwrap(), 1);
        assert_eq!(
            store.resolve("abc123").unwrap().as_deref(),
            Some("https://example.com")
        );
        // resolve incremented the global redirect counter.
        assert_eq!(store.redirect_count(), 1);
    }

    #[test]
    fn missing_code_resolves_to_none() {
        let store = InMemoryStore::new();
        assert_eq!(store.resolve("nope").unwrap(), None);
        assert_eq!(store.len().unwrap(), 0);
    }

    #[test]
    fn generated_codes_have_requested_length() {
        let code = InMemoryStore::generate_code(7);
        assert_eq!(code.len(), 7);
        assert!(code.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
