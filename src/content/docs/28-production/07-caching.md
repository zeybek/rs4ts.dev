---
title: "Caching Strategies"
description: "Two-tier caching in Rust: moka for in-process LRU with stampede protection, Redis for shared state. The TTL and invalidation patterns behind Node's lru-cache."
---

A cache trades freshness for speed: you keep a copy of an expensive-to-produce value close to where it is consumed so the next reader does not pay the full cost. In Node you probably reach for an in-process LRU (`lru-cache`) and a shared store (`ioredis` against Redis). Rust's equivalents are [`moka`](https://docs.rs/moka) for fast in-process caching and the [`redis`](https://docs.rs/redis) crate for a shared, cross-instance cache. This chapter covers both tiers, how to set TTLs, and how to invalidate without leaving stale data behind.

---

## Quick Overview

There are two caches you will almost always combine in a production service. An **in-process (L1) cache** lives in the application's own memory: zero network hops, no serialization, but private to one instance and lost on restart. A **shared (L2) cache** like Redis is reachable by every instance and survives restarts, at the cost of a round-trip and (de)serialization. `moka` is a concurrent, bounded cache with size-based eviction and time-based expiry, plus built-in **request coalescing** so a cache stampede cannot fire the same expensive load a hundred times. `redis` gives you the familiar `GET`/`SET key val EX ttl`/`DEL` surface you already know from `ioredis`. The hard part is never the storage. It is **invalidation**: deciding when a cached copy is wrong and removing it everywhere.

---

## TypeScript/JavaScript Example

A typical Node service with a two-tier cache: an in-process `lru-cache` in front of Redis (`ioredis`), with TTLs and explicit invalidation on write.

```typescript
// npm install lru-cache ioredis
import { LRUCache } from "lru-cache";
import Redis from "ioredis";

interface User {
  id: number;
  name: string;
}

const redis = new Redis(process.env.REDIS_URL ?? "redis://127.0.0.1:6379");

// L1: in-process LRU, bounded to 10k entries, each living 60s.
const l1 = new LRUCache<string, User>({
  max: 10_000,
  ttl: 60_000, // milliseconds
});

let dbCalls = 0;
async function loadUserFromDb(id: number): Promise<User> {
  dbCalls++;
  // Pretend this is a slow query.
  await new Promise((r) => setTimeout(r, 10));
  return { id, name: `user-${id}` };
}

async function getUser(id: number): Promise<User> {
  const key = `user:${id}`;

  // 1. L1 lookup.
  const local = l1.get(key);
  if (local) return local;

  // 2. L2 (Redis) lookup.
  const cached = await redis.get(key);
  if (cached) {
    const user = JSON.parse(cached) as User;
    l1.set(key, user);
    return user;
  }

  // 3. Miss in both tiers: load and back-fill, with a TTL on Redis.
  const user = await loadUserFromDb(id);
  await redis.set(key, JSON.stringify(user), "EX", 300);
  l1.set(key, user);
  return user;
}

// On a write, invalidate BOTH tiers so no instance serves stale data.
async function invalidateUser(id: number): Promise<void> {
  l1.delete(`user:${id}`);
  await redis.del(`user:${id}`);
}
```

**Key points:**

- `lru-cache` bounds memory by entry count and supports a per-cache `ttl`.
- `ioredis` exposes `get` / `set ... EX` / `del`: the raw Redis command surface.
- Cache-aside (a.k.a. lazy loading) is the dominant pattern: read cache, fall through to the source, back-fill.
- Two concurrent cache misses for the same key both run `loadUserFromDb` — `lru-cache` does **not** coalesce them.
- Invalidation is manual and easy to get wrong: forget one tier and you serve stale data.

---

## Rust Equivalent

The idiomatic in-process cache is `moka`, which is concurrent (built for multi-threaded async servers), bounded, and TTL-aware. Its main advantage over `lru-cache` is **read-through with stampede protection**: `try_get_with` runs the loader at most once per key even under a thundering herd. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically.

```bash
cargo add moka --features future
cargo add tokio --features full
```

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use moka::future::Cache;

// A "database" that is slow and counts how often it is hit.
#[derive(Clone)]
struct Db {
    calls: Arc<AtomicU64>,
}

impl Db {
    async fn load_user(&self, id: u64) -> String {
        self.calls.fetch_add(1, Ordering::Relaxed);
        // Pretend this is a slow network/database round-trip.
        tokio::time::sleep(Duration::from_millis(10)).await;
        format!("user-{id}")
    }
}

#[tokio::main]
async fn main() {
    let db = Db { calls: Arc::new(AtomicU64::new(0)) };

    // A bounded cache: at most 10_000 entries, each living for 60 seconds.
    let cache: Cache<u64, String> = Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(60))
        .build();

    // `try_get_with` is the read-through pattern: on a miss it runs the
    // closure, stores the result, and — crucially — coalesces concurrent
    // callers for the same key so the closure runs at most once.
    let load = |id: u64| {
        let db = db.clone();
        async move { Ok::<_, std::convert::Infallible>(db.load_user(id).await) }
    };

    // First call for key 42: a miss, so the DB is hit.
    let a = cache.try_get_with(42, load(42)).await.unwrap();
    // Second call: a hit, served from memory, DB untouched.
    let b = cache.try_get_with(42, load(42)).await.unwrap();

    println!("a = {a}");
    println!("b = {b}");
    println!("db calls = {}", db.calls.load(Ordering::Relaxed));

    // Explicit invalidation removes a single key.
    cache.invalidate(&42).await;
    let c = cache.try_get_with(42, load(42)).await.unwrap();
    println!("c = {c}");
    println!("db calls after invalidate = {}", db.calls.load(Ordering::Relaxed));
}
```

Real output:

```text
a = user-42
b = user-42
db calls = 1
c = user-42
db calls after invalidate = 2
```

The second `try_get_with(42, ...)` was a hit, so `db calls` stayed at `1`. After `invalidate(&42)`, the next read missed and the DB was hit again, bumping the count to `2`.

---

## Detailed Explanation

### `Cache::builder()` and bounds

```rust
let cache: Cache<u64, String> = Cache::builder()
    .max_capacity(10_000)
    .time_to_live(Duration::from_secs(60))
    .build();
```

`moka` caches are **bounded by design**. `max_capacity` sets the maximum number of entries (or a weighted size if you supply a `weigher`), and `moka` uses a TinyLFU eviction policy that outperforms a plain LRU on real workloads. This is a deliberate contrast with a naive `Map`-as-cache, which grows without limit until you run out of memory. The cache is internally `Arc`-shared, so `cache.clone()` is cheap: every clone points at the same underlying store, exactly like cloning an `Arc`.

> **Note:** The `future::Cache` is `Send + Sync` and designed to be stored in shared application state (for example an axum `State`) and cloned into every request handler. You do not wrap it in a `Mutex`.

### TTL vs. TTI

```rust
.time_to_live(Duration::from_secs(60))  // evict 60s after WRITE
.time_to_idle(Duration::from_secs(300)) // evict 300s after last READ
```

`time_to_live` (TTL) counts from when an entry was inserted; `time_to_idle` (TTI) counts from the last access. Use TTL to bound staleness ("this data is never more than 60 seconds old"); use TTI to keep hot keys warm while letting cold ones fall out. You can set both; an entry is evicted when **either** limit is reached. `moka` removes expired entries lazily on access and in a background housekeeping pass, so a `get` of an expired key returns `None`:

```rust
use std::time::Duration;

use moka::future::Cache;

#[tokio::main]
async fn main() {
    let cache: Cache<String, i32> = Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_millis(50))
        .build();

    cache.insert("key".to_string(), 1).await;
    println!("right after insert: {:?}", cache.get("key").await);

    // Wait past the TTL.
    tokio::time::sleep(Duration::from_millis(80)).await;
    println!("after TTL:        {:?}", cache.get("key").await);

    cache.run_pending_tasks().await;
    println!("entry_count:      {}", cache.entry_count());
}
```

Real output:

```text
right after insert: Some(1)
after TTL:        None
entry_count:      0
```

### Read-through and stampede protection

The single most important `moka` feature is `try_get_with` (fallible loader) and `get_with` (infallible loader). On a miss they run your loader, store the result, and **coalesce** concurrent callers for the same key: even if a hundred tasks ask for a cold key at once, the loader runs exactly once and the rest await its result. This is the cure for a **cache stampede** (the "thundering herd" that hammers your database the instant a popular key expires).

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use moka::future::Cache;

#[tokio::main]
async fn main() {
    let loads = Arc::new(AtomicU64::new(0));
    let cache: Cache<u64, String> = Cache::builder()
        .max_capacity(1_000)
        .time_to_live(Duration::from_secs(30))
        .build();

    // Fire 50 concurrent requests for the SAME key while the cache is cold.
    let mut handles = Vec::new();
    for _ in 0..50 {
        let cache = cache.clone();
        let loads = loads.clone();
        handles.push(tokio::spawn(async move {
            cache
                .try_get_with(7u64, async move {
                    // Only ONE task should ever run this block per key.
                    loads.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Ok::<_, std::convert::Infallible>("expensive-result".to_string())
                })
                .await
                .unwrap()
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Despite 50 concurrent callers, the loader ran exactly once.
    println!("loader executions = {}", loads.load(Ordering::Relaxed));
}
```

Real output:

```text
loader executions = 1
```

The Node `lru-cache` has no equivalent guarantee out of the box; you must add your own in-flight-promise deduplication. `moka` gives it to you for free.

### The shared (Redis) tier

For a cache that every instance shares and that survives restarts, use Redis through the `redis` crate. The cache-aside flow is identical to the Node version — read cache, fall through to the source, write back with a TTL — but the Redis reply types are statically typed.

```bash
cargo add redis --features tokio-comp,connection-manager
cargo add serde --features derive
cargo add serde_json
cargo add tokio --features full
```

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
}

// Simulated slow data source.
struct Db {
    calls: AtomicU64,
}

impl Db {
    async fn load_user(&self, id: u64) -> User {
        self.calls.fetch_add(1, Ordering::Relaxed);
        User { id, name: format!("user-{id}") }
    }
}

// Cache-aside read-through against Redis with a 60s TTL.
async fn get_user(
    conn: &mut ConnectionManager,
    db: &Db,
    id: u64,
) -> redis::RedisResult<User> {
    let key = format!("user:{id}");

    // 1. Try the cache.
    let cached: Option<String> = conn.get(&key).await?;
    if let Some(json) = cached {
        return Ok(serde_json::from_str(&json).expect("corrupt cache entry"));
    }

    // 2. Miss: load from the source of truth.
    let user = db.load_user(id).await;

    // 3. Populate the cache with a TTL so stale data self-heals.
    let json = serde_json::to_string(&user).expect("serializable");
    let _: () = conn.set_ex(&key, json, 60).await?;

    Ok(user)
}

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut conn = ConnectionManager::new(client).await?;

    // Clean slate for a deterministic demo.
    let _: () = redis::cmd("FLUSHDB").query_async(&mut conn).await?;

    let db = Arc::new(Db { calls: AtomicU64::new(0) });

    let a = get_user(&mut conn, &db, 42).await?; // miss -> DB
    let b = get_user(&mut conn, &db, 42).await?; // hit  -> Redis
    println!("a = {a:?}");
    println!("b = {b:?}");
    println!("db calls = {}", db.calls.load(Ordering::Relaxed));

    // Invalidate on write: delete the key so the next read repopulates.
    let _: () = conn.del("user:42").await?;
    let c = get_user(&mut conn, &db, 42).await?; // miss again -> DB
    println!("c = {c:?}");
    println!("db calls after invalidate = {}", db.calls.load(Ordering::Relaxed));

    Ok(())
}
```

Run against a local Redis (`docker run -p 6379:6379 redis`), the real output is:

```text
a = User { id: 42, name: "user-42" }
b = User { id: 42, name: "user-42" }
db calls = 1
c = User { id: 42, name: "user-42" }
db calls after invalidate = 2
```

A few things to notice:

- **`ConnectionManager`** is a cheap-to-clone, multiplexed, auto-reconnecting connection. Clone it into each handler instead of opening a new socket per request. That is the production-correct counterpart to an `ioredis` client (which is also a long-lived multiplexed connection).
- **`set_ex(key, value, 60)`** maps to Redis `SET key value EX 60`. The TTL is your safety net: even if you forget to invalidate, the entry self-destructs in 60 seconds, so the worst-case staleness is bounded.
- **The turbofish-free `let _: () =`** annotations are load-bearing. Redis replies are polymorphic, so you must tell the compiler what type to decode the reply into (see *Common Pitfalls*).
- Values are serialized with `serde_json`. Redis stores bytes; you choose the encoding (JSON here, but `bincode` or `MessagePack` are faster and smaller for internal-only data).

> **Tip:** For high-throughput services, put a connection **pool** (`bb8` or `deadpool`) in front of Redis rather than a single `ConnectionManager`, the same way you would size an `ioredis` cluster client. See [the database section](/17-database/08-connection-pooling/) for pooling patterns that apply equally to Redis.

---

## Key Differences

| Concern | TypeScript / Node | Rust |
| --- | --- | --- |
| In-process cache | `lru-cache` (LRU) | `moka` (TinyLFU, concurrent) |
| Bounding | `max` entries / `ttl` | `max_capacity` + `time_to_live` / `time_to_idle` |
| Stampede protection | manual in-flight dedup | built into `try_get_with` / `get_with` |
| Concurrency | single-threaded event loop | true multi-threaded; `moka` is lock-light |
| Shared cache | `ioredis` | `redis` crate + `ConnectionManager` |
| Redis reply typing | dynamic (`string \| null`) | static (`Option<String>`, must annotate) |
| Value requirement | any JS value | `K: Hash + Eq`, `V: Clone` (see pitfalls) |
| Eviction visibility | mostly opaque | `entry_count`, `run_pending_tasks`, listeners |

The deepest conceptual difference is **concurrency**. Node's single event loop means an in-process cache never has data races; you just mutate a `Map`. Rust servers are genuinely multi-threaded, so a cache shared across tasks must be thread-safe. `moka` is engineered for exactly this: it is internally `Arc`-shared and uses sharded, mostly lock-free structures, so you clone it freely across tasks without a `Mutex`. A second difference is **type discipline at the Redis boundary**: where `ioredis` hands you `string | null` and you cast, the `redis` crate forces you to name the decode target, which catches "I expected a list but got a string" bugs at compile time.

> **Warning:** A cache is shared mutable state. The one thing it must never do is store something whose validity depends on the request that created it (a per-user token, a request-scoped permission). Cache the *data*, authorize *per request*. This is a classic cache-poisoning vector. See [the security section](/27-security/).

---

## Common Pitfalls

### 1. Forgetting to annotate Redis reply types

Redis commands are generic over the reply type. If the compiler cannot infer it, you get a confusing error mentioning the never type `!`:

```rust
use redis::AsyncCommands;
use redis::aio::ConnectionManager;

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut conn = ConnectionManager::new(client).await?;

    // does not compile (E0277): no type tells redis how to decode the reply.
    conn.set("k", "v").await?;

    Ok(())
}
```

The real compiler error:

```text
error[E0277]: the trait bound `!: FromRedisValue` is not satisfied
    --> src/main.rs:10:10
     |
  10 |     conn.set("k", "v").await?;
     |          ^^^ the trait `FromRedisValue` is not implemented for `!`
...
     = help: did you intend to use the type `()` here instead?
```

The fix is to annotate the discarded reply: `SET` returns `OK`, which you decode as `()`:

```rust
let _: () = conn.set("k", "v").await?;
```

This trips up nearly every newcomer. The compiler even suggests `()`; take its advice.

### 2. Caching a non-`Clone` value

`moka` hands out a fresh value on every `get`, so the value type must be `Clone`. Trying to cache something like a live TCP connection fails to compile:

```rust
use moka::sync::Cache;

// A value type that is NOT Clone.
struct Connection {
    _socket: std::net::TcpStream,
}

fn main() {
    // does not compile (E0277): Connection does not implement Clone.
    let cache: Cache<u64, Connection> = Cache::builder().max_capacity(10).build();
    println!("{:?}", cache.get(&1).is_some());
}
```

The real compiler error:

```text
error[E0277]: the trait bound `Connection: Clone` is not satisfied
   --> src/main.rs:11:41
    |
 11 |     let cache: Cache<u64, Connection> = Cache::builder().max_capacity(10).build();
    |                                         ^^^^^^^^^^^^^^^^ the trait `Clone` is not implemented for `Connection`
...
note: required by a bound in `moka::sync::Cache::<K, V>::builder`
```

For large values, do not pay the deep clone on every hit — wrap the value in `Arc<T>` and cache `Arc<T>`. Cloning an `Arc` is a cheap atomic refcount bump, not a copy of the data. (This is why the real-world example below caches `Arc<Product>`.)

### 3. The unbounded "cache" that is actually a memory leak

A `HashMap<K, V>` you only ever insert into is not a cache — it is a leak. Without an eviction bound it grows until the process is OOM-killed. Always set `max_capacity` (and usually a TTL) on a `moka` cache, and always set an `EX` on Redis keys. An entry with no expiry is a promise to remember it forever.

### 4. Invalidating only one tier

With a two-tier cache, a write that deletes the Redis key but leaves the L1 copy in every instance's memory will serve stale data for up to the L1 TTL. Either keep L1 TTLs short (seconds, not minutes) so staleness self-heals, or publish invalidation events (Redis pub/sub) that each instance subscribes to and uses to clear its L1. Short L1 TTL is simpler and usually good enough.

### 5. Caching errors and `None`s by accident

If your loader can fail, decide deliberately whether to cache the failure. `try_get_with` does **not** cache an `Err` — the next call retries the loader, which is usually what you want for transient errors. But if you cache an `Option` and store `None` on a miss, you have implemented **negative caching**, which protects you from a flood of lookups for keys that do not exist. Make that choice on purpose, and give negative entries a *shorter* TTL than positive ones (a missing record may appear at any moment).

---

## Best Practices

- **Always bound the cache.** `max_capacity` plus a TTL/TTI on `moka`; `EX` on every Redis key. Treat an unbounded cache as a bug.
- **Use `try_get_with` / `get_with` for read-through**, not a manual get-then-insert. You get stampede protection and correct concurrent behavior for free.
- **Cache `Arc<T>` for large values** so a hit is a refcount bump, not a deep clone.
- **Bound staleness with a TTL; treat invalidation as a bonus.** Invalidation is best-effort; the TTL is the guarantee. Pick the longest staleness your product can tolerate and set the TTL to that.
- **Give the cache its own type.** Wrap the L1/L2 logic in a struct with `get` / `invalidate` methods so call sites cannot accidentally read one tier and forget the other.
- **Per-entry TTLs via the `Expiry` trait** when different keys need different lifetimes (hot config vs. rarely-changing reference data):

  ```rust
  use std::time::{Duration, Instant};

  use moka::Expiry;
  use moka::sync::Cache;

  // A cached value that carries its own desired lifetime.
  #[derive(Clone)]
  struct Cached {
      value: String,
      ttl: Duration,
  }

  // Implement per-entry expiration: each entry decides its own TTL.
  struct PerEntryExpiry;

  impl Expiry<String, Cached> for PerEntryExpiry {
      fn expire_after_create(
          &self,
          _key: &String,
          value: &Cached,
          _created_at: Instant,
      ) -> Option<Duration> {
          Some(value.ttl)
      }
  }

  fn main() {
      let cache: Cache<String, Cached> = Cache::builder()
          .max_capacity(1_000)
          .expire_after(PerEntryExpiry)
          .build();

      cache.insert(
          "short".to_string(),
          Cached { value: "a".into(), ttl: Duration::from_millis(30) },
      );
      cache.insert(
          "long".to_string(),
          Cached { value: "b".into(), ttl: Duration::from_secs(60) },
      );

      std::thread::sleep(Duration::from_millis(50));
      cache.run_pending_tasks();

      // Read the values back so the field is actually used.
      println!("short present: {}", cache.get("short").is_some());
      if let Some(c) = cache.get("long") {
          println!("long still holds: {}", c.value);
      }
  }
  ```

  Real output:

  ```text
  short present: false
  long still holds: b
  ```

- **Pick the right `moka` flavor.** Use `moka::future::Cache` inside an async (`tokio`) server; use `moka::sync::Cache` (the `sync` feature) for synchronous or CPU-bound code with no runtime.
- **Choose a compact serialization for the L2 tier.** JSON is debuggable; `bincode`/MessagePack are faster and smaller for internal-only data you never read by hand.

---

## Real-World Example

A production-flavored two-tier cache: a fast per-process `moka` L1 in front of a shared Redis L2, fronting a repository. L1 holds `Arc<Product>` so hits are cheap, Redis holds JSON so every instance can share entries, and `invalidate` clears both tiers on a write. This is the shape you would store in an axum `State` and call from handlers.

```bash
cargo add moka --features future
cargo add redis --features tokio-comp,connection-manager
cargo add serde --features derive
cargo add serde_json
cargo add tokio --features full
```

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use moka::future::Cache;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Product {
    id: u64,
    name: String,
    price_cents: u64,
}

// The source of truth (a database, an upstream API, ...).
struct Repo {
    db_hits: AtomicU64,
}

impl Repo {
    async fn fetch(&self, id: u64) -> Product {
        self.db_hits.fetch_add(1, Ordering::Relaxed);
        Product { id, name: format!("Widget {id}"), price_cents: 999 + id }
    }
}

// A two-tier cache: a fast per-process L1 (moka) backed by a shared L2 (Redis).
#[derive(Clone)]
struct ProductCache {
    l1: Cache<u64, Arc<Product>>,
    redis: ConnectionManager,
    repo: Arc<Repo>,
}

impl ProductCache {
    fn key(id: u64) -> String {
        format!("product:{id}")
    }

    async fn get(&self, id: u64) -> Arc<Product> {
        // L1: in-process, no network, no serialization.
        if let Some(hit) = self.l1.get(&id).await {
            return hit;
        }

        // L2: shared Redis. Many app instances can reuse the same entry.
        let mut redis = self.redis.clone();
        let cached: Option<String> = redis.get(Self::key(id)).await.unwrap_or(None);
        if let Some(json) = cached {
            if let Ok(p) = serde_json::from_str::<Product>(&json) {
                let arc = Arc::new(p);
                self.l1.insert(id, arc.clone()).await;
                return arc;
            }
        }

        // Miss in both tiers: load from the source and back-fill both caches.
        let product = self.repo.fetch(id).await;
        let json = serde_json::to_string(&product).expect("serializable");
        let _: Result<(), _> = redis.set_ex(Self::key(id), json, 300).await;

        let arc = Arc::new(product);
        self.l1.insert(id, arc.clone()).await;
        arc
    }

    // Invalidate both tiers on a write so no instance serves stale data.
    async fn invalidate(&self, id: u64) {
        self.l1.invalidate(&id).await;
        let mut redis = self.redis.clone();
        let _: Result<(), _> = redis.del(Self::key(id)).await;
    }
}

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut conn = ConnectionManager::new(client).await?;
    let _: () = redis::cmd("FLUSHDB").query_async(&mut conn).await?;

    let cache = ProductCache {
        l1: Cache::builder()
            .max_capacity(50_000)
            .time_to_live(Duration::from_secs(60))
            .build(),
        redis: conn,
        repo: Arc::new(Repo { db_hits: AtomicU64::new(0) }),
    };

    let p1 = cache.get(7).await; // miss both -> DB
    let p2 = cache.get(7).await; // L1 hit
    println!("p1 == p2: {}", p1 == p2);
    println!("db hits after two gets: {}", cache.repo.db_hits.load(Ordering::Relaxed));

    // Simulate a second process: clear L1 only, Redis still has the value.
    cache.l1.invalidate(&7).await;
    let p3 = cache.get(7).await; // L1 miss, L2 (Redis) hit -> no DB call
    println!("p3 == p1: {}", p3 == p1);
    println!("db hits after L1 eviction: {}", cache.repo.db_hits.load(Ordering::Relaxed));

    // Write-path invalidation clears both tiers.
    cache.invalidate(7).await;
    let _ = cache.get(7).await; // miss both -> DB again
    println!("db hits after invalidate: {}", cache.repo.db_hits.load(Ordering::Relaxed));

    Ok(())
}
```

Run against a local Redis, the real output is:

```text
p1 == p2: true
db hits after two gets: 1
p3 == p1: true
db hits after L1 eviction: 1
db hits after invalidate: 2
```

This proves the tiers: two reads hit the DB once (L1 absorbs the second). After evicting L1 (simulating a fresh instance or a restart), the read is served from Redis with **no** DB hit: the count stays at `1`. Only after invalidating both tiers does the next read fall through to the DB again, bumping the count to `2`. The whole `ProductCache` is `Clone` and `Send + Sync`, so you store one in axum `State` and clone it into every handler. See [the web APIs section](/16-web-apis/) for wiring shared state.

---

## Further Reading

- [`moka` documentation](https://docs.rs/moka) — builder options, eviction policy, listeners, and the `Expiry` trait.
- [`redis` crate documentation](https://docs.rs/redis) — async commands, `ConnectionManager`, pipelines, and pub/sub.
- [Redis `SET` command reference](https://redis.io/docs/latest/commands/set/): `EX`, `PX`, `NX`, and `XX` options for TTLs and conditional writes.
- [The `Arc` chapter](/05-ownership/07-reference-counting/) — why caching `Arc<T>` makes hits cheap.
- Sibling pages in this section: [rate limiting](/28-production/06-rate-limiting/) (another tower/shared-state concern), [health checks](/28-production/03-health-checks/) (your readiness probe should check Redis), [graceful shutdown](/28-production/02-graceful-shutdown/) (flush or drain caches on stop), and the [production checklist](/28-production/09-production-checklist/).
- [Database connection pooling](/17-database/08-connection-pooling/): the same pooling ideas apply to Redis.
- Moving an existing Node caching layer? See the [migration guide](/29-migration-guide/).

---

## Exercises

### Exercise 1: Memoize an expensive computation

**Difficulty:** Beginner

**Objective:** Use a `moka::future::Cache` to compute a value once and serve repeat requests from memory.

**Instructions:** Build a cache keyed by `u64`. Use `get_with` to compute `fib(n)` (iteratively) on a miss, incrementing a shared counter each time the loader actually runs. Request the same key three times and prove the loader ran exactly once.

<details>
<summary>Solution</summary>

```rust
// cargo add moka --features future
// cargo add tokio --features full
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use moka::future::Cache;

#[tokio::main]
async fn main() {
    let computations = Arc::new(AtomicU64::new(0));

    let cache: Cache<u64, u64> = Cache::builder()
        .max_capacity(1_000)
        .time_to_live(Duration::from_secs(600))
        .build();

    async fn slow_fib(n: u64) -> u64 {
        match n {
            0 => 0,
            1 => 1,
            _ => {
                let (mut a, mut b) = (0u64, 1u64);
                for _ in 2..=n {
                    (a, b) = (b, a + b);
                }
                b
            }
        }
    }

    let mut last = 0;
    // Ask for fib(90) three times; only the first should compute it.
    for _ in 0..3 {
        let computations = computations.clone();
        last = cache
            .get_with(90u64, async move {
                computations.fetch_add(1, Ordering::Relaxed);
                slow_fib(90).await
            })
            .await;
    }

    println!("fib(90) = {last}");
    println!("computations = {}", computations.load(Ordering::Relaxed));
}
```

Real output:

```text
fib(90) = 2880067194370816120
computations = 1
```

</details>

### Exercise 2: Negative caching with per-entry TTLs

**Difficulty:** Intermediate

**Objective:** Cache "not found" results with a shorter TTL than successful results, using the `Expiry` trait.

**Instructions:** Define an enum `Entry { Hit(String), Miss }`. Implement `Expiry` so `Hit` lives 300 seconds and `Miss` lives only 5 seconds. Insert one of each, then read them back and prove a `Miss` is stored (negative caching) so the next lookup of a known-missing key does not hit the backend.

<details>
<summary>Solution</summary>

```rust
// cargo add moka --features sync
use std::time::{Duration, Instant};

use moka::Expiry;
use moka::sync::Cache;

#[derive(Clone, Debug)]
enum Entry {
    Hit(String),
    // A cached "not found" so repeated lookups of a missing key don't
    // hammer the backend (negative caching).
    Miss,
}

struct TieredExpiry;

impl Expiry<u64, Entry> for TieredExpiry {
    fn expire_after_create(
        &self,
        _key: &u64,
        value: &Entry,
        _created_at: Instant,
    ) -> Option<Duration> {
        match value {
            Entry::Hit(_) => Some(Duration::from_secs(300)), // real data: 5 min
            Entry::Miss => Some(Duration::from_secs(5)),      // misses: short TTL
        }
    }
}

fn main() {
    let cache: Cache<u64, Entry> = Cache::builder()
        .max_capacity(10_000)
        .expire_after(TieredExpiry)
        .build();

    cache.insert(1, Entry::Hit("found".to_string()));
    cache.insert(2, Entry::Miss);

    if let Some(Entry::Hit(v)) = cache.get(&1) {
        println!("key 1 -> {v}");
    }
    println!(
        "key 2 is negatively cached: {}",
        matches!(cache.get(&2), Some(Entry::Miss))
    );
}
```

Real output:

```text
key 1 -> found
key 2 is negatively cached: true
```

</details>

### Exercise 3: Conditional Redis write with `SET NX EX`

**Difficulty:** Advanced

**Objective:** Use Redis's atomic `SET ... NX EX` to implement a "set only if absent, with TTL": the building block for a distributed lock or request-dedup key.

**Instructions:** Open a `ConnectionManager` to a local Redis. Use a raw `SET key value NX EX 30` command and decode the reply as `Option<String>` (it is `Some("OK")` on success, `None` when the key already exists). Acquire the key from "worker-a", then attempt to acquire it from "worker-b" and show the second attempt fails while the first holder remains.

<details>
<summary>Solution</summary>

```rust
// cargo add redis --features tokio-comp,connection-manager
// cargo add tokio --features full
use redis::AsyncCommands;
use redis::aio::ConnectionManager;

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut conn = ConnectionManager::new(client).await?;
    let _: () = redis::cmd("FLUSHDB").query_async(&mut conn).await?;

    // SET key value NX EX 30: atomically set only if absent, with a 30s TTL.
    // The reply is the string "OK" on success or nil when the key existed.
    let first: Option<String> = redis::cmd("SET")
        .arg("lock:order:7")
        .arg("worker-a")
        .arg("NX")
        .arg("EX")
        .arg(30)
        .query_async(&mut conn)
        .await?;
    println!("first acquire: {first:?}");

    // A second worker cannot take the lock until it expires or is released.
    let second: Option<String> = redis::cmd("SET")
        .arg("lock:order:7")
        .arg("worker-b")
        .arg("NX")
        .arg("EX")
        .arg(30)
        .query_async(&mut conn)
        .await?;
    println!("second acquire: {second:?}");

    let holder: String = conn.get("lock:order:7").await?;
    println!("lock held by: {holder}");

    Ok(())
}
```

Real output:

```text
first acquire: Some("OK")
second acquire: None
lock held by: worker-a
```

This `SET NX EX` is the kernel of a simple distributed lock and of request deduplication for [background jobs](/28-production/08-background-jobs/) — a job that should run at most once writes a unique key with `NX` before starting.

</details>
