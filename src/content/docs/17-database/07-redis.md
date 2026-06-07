---
title: "Redis with the `redis` Crate"
description: "Use the Rust redis crate like ioredis for caching, counters, and rate limits, but with typed command replies and cheap multiplexed connections instead of a pool."
---

If you have reached for `ioredis` or `node-redis` in Node to cache database results, count rate-limited requests, or store sessions, the Rust `redis` crate is the direct equivalent. It speaks the same RESP protocol, exposes the same commands (`GET`, `SET`, `INCR`, `EXPIRE`, `HSET`, …), and integrates with `tokio` for async access. But it adds Rust's signature twist: every value you read back is **typed**, and the type you ask for is part of the call.

---

## Quick Overview

The `redis` crate is an async Redis client that maps each Redis command to a Rust method through the `AsyncCommands` trait. The defining difference from `ioredis` is that Redis values are untyped bytes on the wire, so **you tell Rust what Rust type to decode each reply into** (`let n: i64 = con.incr("hits", 1).await?`), and a mismatch is caught when the value is converted, not silently coerced like JavaScript would. For a TypeScript developer the two surprises are this mandatory return-type annotation and the fact that Redis async connections are *multiplexed and cheaply cloneable*, so you usually do not need a separate connection pool at all.

> **Note:** This file covers the `redis` crate itself: connecting asynchronously, running commands, and the everyday patterns (caching, counters, rate limiting, locks). For pooling SQL connections see [Connection Pooling](/17-database/08-connection-pooling/); for the SQL databases you are usually caching *in front of* see [SQLx](/17-database/00-sqlx-intro/) and [Getting Started with Diesel (the TypeORM of Rust)](/17-database/03-diesel-intro/). The async model here builds directly on [Section 11: Async](/11-async/).

---

## TypeScript/JavaScript Example

A common Node setup: a shared client, a cache-aside helper, an atomic counter, and a fixed-window rate limiter, all with `ioredis`.

```typescript
// TypeScript with ioredis (npm i ioredis)
import Redis from "ioredis";

const redis = new Redis(process.env.REDIS_URL ?? "redis://127.0.0.1:6379");

interface User {
  id: number;
  name: string;
  email: string;
}

// Cache-aside: check Redis, fall back to the database, then populate the cache.
async function getUser(id: number): Promise<User> {
  const key = `user:${id}`;
  const cached = await redis.get(key); // string | null
  if (cached !== null) {
    console.log(`cache HIT for ${key}`);
    return JSON.parse(cached) as User;
  }

  console.log(`cache MISS for ${key}`);
  const user = await loadUserFromDb(id); // imagine a SQL query here
  await redis.set(key, JSON.stringify(user), "EX", 300); // expire after 5 min
  return user;
}

// Atomic counter — INCR is server-side, so it is safe under concurrency.
async function recordVisit(): Promise<number> {
  return await redis.incr("visits"); // number
}

// Fixed-window rate limit: 3 requests per 60 seconds per user.
async function allowRequest(user: string): Promise<boolean> {
  const key = `rate:${user}`;
  const count = await redis.incr(key);
  if (count === 1) await redis.expire(key, 60);
  return count <= 3;
}

async function loadUserFromDb(id: number): Promise<User> {
  return { id, name: "Ada Lovelace", email: "ada@example.com" };
}
```

**Key points:**

- One shared `redis` client is reused everywhere; `ioredis` multiplexes commands over a single socket.
- Every reply is loosely typed — `redis.get` returns `string | null`, and you cast (`as User`) after `JSON.parse`.
- `INCR` returns a `number` and is atomic on the server, so two concurrent callers never lose an increment.
- A typo like `redis.incrr("visits")` is a runtime error; the type of the returned value is whatever you assert it to be.

---

## Rust Equivalent

The same four operations with the `redis` crate. Bring the commands into scope with `use redis::AsyncCommands`, then call methods on a connection. The one new habit is the **type annotation on the result**: `let val: String`, `let n: i64`, `let cached: Option<String>`.

Add the dependencies in a fresh project (`cargo new` selects the latest stable edition, 2024):

```toml
# Cargo.toml
[dependencies]
redis = { version = "1.2", features = ["tokio-comp", "connection-manager"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

> **Tip:** Equivalently, `cargo add redis --features tokio-comp,connection-manager` then `cargo add tokio --features full`. The `tokio-comp` feature wires Redis I/O into the Tokio runtime; `connection-manager` adds the auto-reconnecting `ConnectionManager` used later.

```rust
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct User {
    id: u64,
    name: String,
    email: String,
}

// Cache-aside: check Redis, fall back to the "database", then populate the cache.
async fn get_user(con: &mut ConnectionManager, id: u64) -> redis::RedisResult<User> {
    let key = format!("user:{id}");

    if let Some(json) = con.get::<_, Option<String>>(&key).await? {
        println!("cache HIT for {key}");
        return Ok(serde_json::from_str(&json).expect("valid cached JSON"));
    }

    println!("cache MISS for {key}");
    let user = User { id, name: "Ada Lovelace".into(), email: "ada@example.com".into() };
    let payload = serde_json::to_string(&user).expect("serializable");
    let _: () = con.set_ex(&key, payload, 300).await?; // EX 300 seconds
    Ok(user)
}

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut con = ConnectionManager::new(client).await?;

    let _: () = redis::cmd("FLUSHDB").query_async(&mut con).await?; // clean slate for the demo

    let u1 = get_user(&mut con, 42).await?; // miss
    println!("{u1:?}");
    let u2 = get_user(&mut con, 42).await?; // hit
    println!("{u2:?}");

    // Pipeline: batch several commands into one round-trip.
    let (a, b): (i64, i64) = redis::pipe()
        .atomic()
        .incr("hits", 1)
        .incr("hits", 10)
        .query_async(&mut con)
        .await?;
    println!("pipeline hits => {a}, {b}");

    Ok(())
}
```

Running this against a local Redis prints the real output:

```text
cache MISS for user:42
User { id: 42, name: "Ada Lovelace", email: "ada@example.com" }
cache HIT for user:42
User { id: 42, name: "Ada Lovelace", email: "ada@example.com" }
pipeline hits => 1, 11
```

---

## Detailed Explanation

Going line by line, contrasting with the `ioredis` version:

- **`redis::Client::open("redis://127.0.0.1:6379/")`** parses a connection URL and returns a `Client`. Like `new Redis(url)`, this does *not* open a socket yet — it is a lazy handle. (The URL form `redis://:password@host:port/db` carries auth and database index, exactly like `ioredis`.)

- **`ConnectionManager::new(client).await?`** establishes the actual async connection. A `ConnectionManager` multiplexes many concurrent commands over one socket *and* transparently reconnects if the connection drops: the closest match to how `ioredis` behaves out of the box. It is `Clone`, and each clone shares the same underlying connection, so you store one in your application state and `.clone()` it per task. This is why Rust async Redis rarely needs a pool (contrast with [Connection Pooling](/17-database/08-connection-pooling/), which is essential for *synchronous* SQL).

- **`use redis::AsyncCommands`** is the trait that adds `.get`, `.set`, `.incr`, `.expire`, and dozens more as methods on a connection. Without the `use`, those methods are invisible: a frequent first stumble (see Common Pitfalls). This is the trait-method pattern from [Section 09: Generics & Traits](/09-generics-traits/): the methods live on a trait, and the trait must be in scope.

- **`con.get::<_, Option<String>>(&key)`** asks Redis for the value and decodes the reply into `Option<String>`. Redis returns *bytes*; the `FromRedisValue` trait converts them into the Rust type you name. Decoding into `Option<String>` means "a missing key is `None`, not an error," the type-system equivalent of `ioredis` returning `string | null`. The turbofish `::<_, Option<String>>` names the return type inline; alternatively, annotate the binding: `let cached: Option<String> = con.get(&key).await?`.

- **`con.set_ex(&key, payload, 300)`** is `SET key value EX 300` in one call. The `let _: () = ...` annotation says "this command returns nothing useful; decode the reply as the unit type `()`." You *must* state this — Rust will not guess (see Common Pitfalls for the exact error).

- **`redis::pipe().atomic().incr(...).incr(...).query_async(&mut con)`** builds a pipeline (multiple commands sent in one network round-trip) and `.atomic()` wraps it in `MULTI`/`EXEC`. The replies come back as a tuple whose type you annotate — here `(i64, i64)`. This mirrors `redis.pipeline().incr(...).incr(...).exec()` in `ioredis`, but the tuple is statically typed.

- **`#[tokio::main]` and `async fn main`** put you inside the Tokio runtime, just as Node's event loop is always running. Unlike JavaScript Promises, **Rust futures are lazy**: nothing happens until `.await`. Forgetting `.await` does not "fire and forget"; it produces an unused-`Future` warning and the command never runs. See [Section 11: Promises vs Futures](/11-async/00-promises-vs-futures/).

---

## Key Differences

| Aspect | `ioredis` (TypeScript) | `redis` crate (Rust) |
| --- | --- | --- |
| Reply typing | Loosely typed (`string \| null`), cast after parse | You name the Rust type per call; `FromRedisValue` decodes |
| Missing key | `null` | `Option<T>` decoded as `None` |
| Errors | Rejected `Promise` (try/catch) | `RedisResult<T>` = `Result<T, RedisError>`, handled with `?` |
| Connection reuse | One client, multiplexed | `MultiplexedConnection` / `ConnectionManager`, cheaply `Clone` |
| Pooling | Not needed | Usually not needed for async (unlike sync SQL) |
| Reconnect | Automatic | `ConnectionManager` reconnects; raw connections do not |
| Pipelines | `redis.pipeline()` | `redis::pipe()`, replies as a typed tuple |
| JSON values | `JSON.stringify` / `JSON.parse` | `serde_json::to_string` / `from_str` (see [Section 15](/15-serialization/)) |
| Eagerness | Promises run immediately | Futures are **lazy**: nothing runs without `.await` |

The deepest difference is the typed reply. In `ioredis`, `await redis.get(key)` is always `string | null` and you decide afterward what it "really" is. In Rust, the conversion happens *as part of the call*, so `con.get::<_, i64>(key)` will error at decode time if the stored value is not a valid integer, rather than handing you a string you might misuse.

> **Note:** The `redis` crate (1.x) ships **two** command traits. `AsyncCommands` (used above) is generic: *you* choose the return type. `AsyncTypedCommands` is the newer, opinionated variant where each method has a fixed, sensible return type, so you often skip the annotation. Pick one per file for consistency; `AsyncTypedCommands` is shown under Best Practices.

---

## Common Pitfalls

### Forgetting the return-type annotation

A TypeScript developer expects `con.set("k", "v").await?` to "just work" because in `ioredis` the reply is discarded. In Rust the compiler cannot infer what type to decode the reply into:

```rust
use redis::AsyncCommands;

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut con = client.get_multiplexed_async_connection().await?;

    con.set("k", "v").await?; // does not compile (error[E0277]: the trait bound `!: FromRedisValue` is not satisfied)
    Ok(())
}
```

The real `cargo check` output:

```text
error[E0277]: the trait bound `!: FromRedisValue` is not satisfied
    --> src/main.rs:8:9
     |
   8 |     con.set("k", "v").await?;
     |         ^^^ the trait `FromRedisValue` is not implemented for `!`
     |
     = help: the following other types implement trait `FromRedisValue`:
               ()
               (T,)
               (T1, T2)
               ...
     = help: did you intend to use the type `()` here instead?
note: required by a bound in `redis::AsyncCommands::set`
```

The compiler even suggests the fix: annotate the discarded reply as `()`.

```rust
let _: () = con.set("k", "v").await?; // "I do not care about the reply"
```

### Forgetting to import the trait

The command methods live on `AsyncCommands`. Without the `use`, `con.get(...)` does not exist and you get `no method named 'get' found`. The fix is one line: `use redis::AsyncCommands;` (or `use redis::AsyncTypedCommands;`). This is the same "method comes from a trait that must be in scope" rule as the rest of Rust.

### Decoding into the wrong Rust type at runtime

Redis is dynamically typed *on the server*. Calling a string command on a key that holds a list is a server-side `WRONGTYPE` error, surfaced as a `RedisError`:

```rust
use redis::AsyncCommands;

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut con = client.get_multiplexed_async_connection().await?;

    let _: () = con.rpush("mylist", "x").await?;
    let bad: String = con.get("mylist").await?; // runtime error: key holds a list, not a string
    println!("{bad}");
    Ok(())
}
```

Real runtime output:

```text
Error: "WRONGTYPE": Operation against a key holding the wrong kind of value
```

This is not a compiler error (the compiler cannot know what a key holds), but unlike JavaScript it surfaces as a typed `Err` you must handle, not a silently coerced value.

### Assuming you need a connection pool

Coming from sync SQL, the instinct is to build a pool. For async Redis, a `MultiplexedConnection`/`ConnectionManager` already multiplexes concurrent commands over one socket and is cheap to `.clone()`. Reach for a pool only when you specifically need blocking commands (e.g. `BLPOP`) that hold a connection, or features that require a dedicated connection.

---

## Best Practices

- **Store one `ConnectionManager` in your app state and clone it.** It auto-reconnects and multiplexes; cloning is cheap. This is the idiomatic shared-handle pattern, the same as sharing a `sqlx::Pool` (see [Connection Pooling](/17-database/08-connection-pooling/)).

- **Prefer `AsyncTypedCommands` when you want fixed return types.** It removes most annotations. The same program with the typed trait (note no turbofish or `let _: ()`):

```rust
use redis::AsyncTypedCommands;

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut con = client.get_multiplexed_async_connection().await?;

    con.set("greeting", "hi").await?;                 // returns ()
    let val: Option<String> = con.get("greeting").await?; // GET is naturally nullable
    println!("typed get => {val:?}");

    let n = con.incr("visits", 1).await?; // returns i64 — no annotation needed
    println!("typed incr => {n}");

    let existed = con.exists("greeting").await?; // returns bool
    println!("typed exists => {existed}");
    Ok(())
}
```

Real output:

```text
typed get => Some("hi")
typed incr => 1
typed exists => true
```

- **Always set a TTL on cache and ephemeral keys.** Use `set_ex` (or `set_options` with `SetExpiry`) so keys self-expire; an unbounded cache is a memory leak.

- **Use atomic server-side operations instead of read-modify-write.** `INCR`, `INCRBY`, `EXPIRE`, and `SET ... NX` execute atomically on the server, so concurrent callers never race. Building a counter with `GET` then `SET` is a classic concurrency bug.

- **Decode missing keys as `Option<T>`.** It makes "absent" a value you handle with `?.`-style combinators rather than an error: the spiritual cousin of TypeScript's `?? ` and `?.`. See [Section 08: Result & Option](/08-error-handling/00-result-option/).

- **Keep serialization in one place.** Serialize values with `serde_json` (or a compact format like MessagePack) on the way in and out, so a key always holds the same shape. See [Section 15: Serialization](/15-serialization/).

---

## Real-World Example

A small, production-flavored cache/session service: a `Cache` struct wrapping a `ConnectionManager`, with generic `put`/`get` helpers that serialize any `serde` type to JSON with a TTL. This is the shape you would store in your `axum` application state (see [Section 16: Web APIs](/16-web-apis/)) and hand to request handlers.

```rust
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
struct Cache {
    con: ConnectionManager,
}

impl Cache {
    async fn connect(url: &str) -> redis::RedisResult<Self> {
        let client = redis::Client::open(url)?;
        let con = ConnectionManager::new(client).await?;
        Ok(Self { con })
    }

    async fn put<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl: Duration,
    ) -> redis::RedisResult<()> {
        let json = serde_json::to_string(value).expect("serializable");
        let mut con = self.con.clone();
        con.set_ex(key, json, ttl.as_secs()).await
    }

    async fn get<T: DeserializeOwned>(&self, key: &str) -> redis::RedisResult<Option<T>> {
        let mut con = self.con.clone();
        let raw: Option<String> = con.get(key).await?;
        Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Session {
    user_id: u64,
    role: String,
}

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let cache = Cache::connect("redis://127.0.0.1:6379/").await?;

    let session = Session { user_id: 7, role: "editor".into() };
    cache.put("session:abc", &session, Duration::from_secs(900)).await?;

    let loaded: Option<Session> = cache.get("session:abc").await?;
    println!("loaded => {loaded:?}");

    let missing: Option<Session> = cache.get("session:xyz").await?;
    println!("missing => {missing:?}");
    Ok(())
}
```

Real output:

```text
loaded => Some(Session { user_id: 7, role: "editor" })
missing => None
```

The `Cache` struct is `Clone` (because `ConnectionManager` is), so every handler gets its own cheap handle to the same multiplexed connection. The generic `put<T: Serialize>` / `get<T: DeserializeOwned>` pair means any `serde`-derived type can be cached without bespoke code per type. Rust monomorphizes a specialized version for each `T` at compile time, whereas the TypeScript equivalent erases the generic and trusts a runtime `as` cast.

---

## Further Reading

- [`redis` crate documentation (docs.rs)](https://docs.rs/redis/) — the full command list, `AsyncCommands`/`AsyncTypedCommands`, and `ConnectionManager`.
- [`redis` crate on crates.io](https://crates.io/crates/redis) — current version and feature flags (`tokio-comp`, `connection-manager`, `json`, `cluster-async`, …).
- [Redis command reference](https://redis.io/commands/) — the canonical semantics of every command, identical whichever client you use.
- Related guide sections:
  - [SQLx](/17-database/00-sqlx-intro/) and [Getting Started with Diesel (the TypeORM of Rust)](/17-database/03-diesel-intro/) — the SQL stores Redis usually caches in front of.
  - [Connection Pooling](/17-database/08-connection-pooling/) — why async Redis rarely needs a pool, and how sync SQL does.
  - [MongoDB with the Official Rust Driver](/17-database/06-mongodb/) — another async, document-oriented client with serde-typed values.
  - [Section 11: Async](/11-async/) and [Promises vs Futures](/11-async/00-promises-vs-futures/) — the runtime model behind `.await`.
  - [Section 15: Serialization](/15-serialization/) — `serde_json` for cache values.
  - [Section 16: Web APIs](/16-web-apis/) — wiring a `Cache` into `axum` state.
  - [Section 18: CLI Tools](/18-cli-tools/) — building a small Redis admin CLI around these calls.

---

## Exercises

### Exercise 1: A typed cache-aside helper

**Difficulty:** Beginner

**Objective:** Practise the mandatory return-type annotation and the `Option<T>` "missing key" pattern.

**Instructions:** Write `async fn cached_greeting(con: &mut ConnectionManager, name: &str) -> redis::RedisResult<String>` that returns the value of the key `greet:{name}` if present, and otherwise stores `format!("Hello, {name}!")` with a 60-second TTL and returns it. Call it twice for the same name and observe the second call reading from the cache.

<details>
<summary>Solution</summary>

```rust
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

async fn cached_greeting(
    con: &mut ConnectionManager,
    name: &str,
) -> redis::RedisResult<String> {
    let key = format!("greet:{name}");

    if let Some(cached) = con.get::<_, Option<String>>(&key).await? {
        return Ok(cached);
    }

    let greeting = format!("Hello, {name}!");
    let _: () = con.set_ex(&key, &greeting, 60).await?;
    Ok(greeting)
}

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut con = ConnectionManager::new(client).await?;
    let _: () = redis::cmd("FLUSHDB").query_async(&mut con).await?;

    println!("{}", cached_greeting(&mut con, "Ada").await?); // computes + stores
    println!("{}", cached_greeting(&mut con, "Ada").await?); // reads from cache
    Ok(())
}
```

Both calls print `Hello, Ada!`; the second served it from Redis. The key detail is `con.get::<_, Option<String>>(&key)`, where the `Option` turns "absent" into `None` instead of an error.

</details>

### Exercise 2: A fixed-window rate limiter

**Difficulty:** Intermediate

**Objective:** Use the atomic `INCR` + `EXPIRE` pattern that backs most API rate limiters.

**Instructions:** Write `async fn allow_request(con, user: &str, limit: i64, window_secs: i64) -> redis::RedisResult<bool>` that increments `rate:{user}`, sets the TTL to `window_secs` only on the first hit of a window (when the counter equals 1), and returns whether the count is within `limit`. Call it five times for one user with `limit = 3` and print which requests are allowed.

<details>
<summary>Solution</summary>

```rust
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;

async fn allow_request(
    con: &mut MultiplexedConnection,
    user: &str,
    limit: i64,
    window_secs: i64,
) -> redis::RedisResult<bool> {
    let key = format!("rate:{user}");
    let count: i64 = con.incr(&key, 1).await?;
    if count == 1 {
        // Start the window only on the first request that created the key.
        let _: () = con.expire(&key, window_secs).await?;
    }
    Ok(count <= limit)
}

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut con = client.get_multiplexed_async_connection().await?;
    let _: () = redis::cmd("FLUSHDB").query_async(&mut con).await?;

    for i in 1..=5 {
        let ok = allow_request(&mut con, "alice", 3, 60).await?;
        println!("request {i}: allowed = {ok}");
    }
    Ok(())
}
```

Real output:

```text
request 1: allowed = true
request 2: allowed = true
request 3: allowed = true
request 4: allowed = false
request 5: allowed = false
```

Because `INCR` runs atomically on the server, this counter is correct even under thousands of concurrent requests; a `GET`-then-`SET` version would not be.

</details>

### Exercise 3: A `SET NX EX` distributed lock

**Difficulty:** Advanced

**Objective:** Use `set_options` to build the canonical "acquire-once" lock and prove the second acquirer is rejected.

**Instructions:** Using `SetOptions` with `ExistenceCheck::NX` and `SetExpiry::EX(30)`, write code that tries to acquire `lock:job` twice. The first attempt should succeed (`true`) and the second should fail (`false`) because the key already exists. The `EX(30)` ensures the lock auto-releases if the holder crashes.

<details>
<summary>Solution</summary>

```rust
use redis::{AsyncCommands, ExistenceCheck, SetExpiry, SetOptions};

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    let client = redis::Client::open("redis://127.0.0.1:6379/")?;
    let mut con = client.get_multiplexed_async_connection().await?;
    let _: () = redis::cmd("FLUSHDB").query_async(&mut con).await?;

    // SET lock:job owner-1 NX EX 30
    let opts = SetOptions::default()
        .conditional_set(ExistenceCheck::NX)
        .with_expiration(SetExpiry::EX(30));
    let acquired: bool = con.set_options("lock:job", "owner-1", opts).await?;
    println!("first acquire => {acquired}");

    // A different worker tries while the lock is held.
    let opts2 = SetOptions::default()
        .conditional_set(ExistenceCheck::NX)
        .with_expiration(SetExpiry::EX(30));
    let acquired2: bool = con.set_options("lock:job", "owner-2", opts2).await?;
    println!("second acquire => {acquired2}");

    Ok(())
}
```

Real output:

```text
first acquire => true
second acquire => false
```

The `NX` flag means "set only if the key does not exist," so the second worker is cleanly turned away. Pairing it with `EX(30)` guarantees the lock cannot be held forever if the first worker dies before releasing it: the foundation of safe distributed locking. (For multi-node correctness, study the Redlock algorithm before relying on this in production.)

</details>
