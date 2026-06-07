---
title: "Connection Pooling: Sizing and Lifecycle"
description: "Manage database connection pools in Rust with sqlx::Pool, deadpool, and bb8: the sizing, timeout, and lifecycle knobs Node's pg.Pool or Prisma hide from you."
---

Opening a fresh database connection for every query is one of the most expensive mistakes a service can make: a TCP handshake, TLS negotiation, and an authentication round-trip on the hot path of every request. A **connection pool** keeps a small, reusable set of live connections so each query borrows one, uses it, and returns it. This page is about managing that pool in Rust: `sqlx::Pool`, the `deadpool` and `bb8` generic poolers, and the sizing and lifecycle knobs that decide whether your service stays up under load.

---

## Quick Overview

A **connection pool** is a fixed-capacity set of open database connections that requests check out and return, instead of dialing the database anew each time. In Node you rarely touch this directly: `pg.Pool`, Knex's `pool`, or Prisma's internal pool hide it behind a config object. In Rust you build the pool explicitly and own its full configuration: maximum size, minimum warm connections, acquire timeout, idle timeout, and maximum connection lifetime. SQLx ships its own `Pool<DB>`; for drivers that do not (the raw `tokio-postgres`, `redis`, MongoDB drivers), the ecosystem standardizes on two **generic** poolers, **`deadpool`** and **`bb8`**. The pool handle is a cheap clonable `Arc` you build once at startup and share across every handler.

> **Note:** Every Rust snippet here was compiled and run with `cargo` 1.96.0 (current stable; 2024 edition). The SQLx examples were verified against SQLx **0.8.6**; the `Pool`/`PoolOptions` API shown is identical in 0.8 and the newer 0.9 line that `cargo add sqlx` now resolves to on Rust ≥ 1.94. The generic-pooler examples use **`deadpool` 0.14** and **`bb8` 0.9**. SQLx examples run against in-memory SQLite, so they reproduce with no server installed.

---

## TypeScript/JavaScript Example

Here is a realistic `node-postgres` (`pg`) pool, the kind you would build once in a module and import everywhere. The pool config is the heart of it: how many connections, how long to wait, when to recycle.

```typescript
// db.ts
import { Pool, type PoolConfig } from "pg";

const config: PoolConfig = {
  connectionString: process.env.DATABASE_URL,
  max: 20,                       // hard cap on connections
  min: 2,                        // keep a couple warm (pg keeps them, doesn't pre-open)
  connectionTimeoutMillis: 5_000, // wait up to 5s for a free connection
  idleTimeoutMillis: 30_000,      // close a connection idle for 30s
  maxLifetimeSeconds: 1_800,      // recycle a connection after 30 min
};

// ONE shared pool for the whole process. Importing this module everywhere
// gives every route the same pool — never `new Pool()` per request.
export const pool = new Pool(config);

// Surface background errors (an idle connection dropped by the server, etc.).
pool.on("error", (err) => {
  console.error("idle client error", err.message);
});

export async function countUsers(): Promise<number> {
  // `pool.query` checks a connection out and returns it automatically.
  const { rows } = await pool.query<{ count: string }>(
    "SELECT COUNT(*) AS count FROM users",
  );
  return Number(rows[0].count);
}

// For a multi-statement unit of work you check a client out explicitly
// and MUST release it in a finally — forgetting `release()` leaks a
// connection and eventually exhausts the pool.
export async function transfer(from: number, to: number, amount: number) {
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    await client.query("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [amount, from]);
    await client.query("UPDATE accounts SET balance = balance + $1 WHERE id = $2", [amount, to]);
    await client.query("COMMIT");
  } catch (e) {
    await client.query("ROLLBACK");
    throw e;
  } finally {
    client.release(); // <- the classic bug is forgetting this
  }
}

// On shutdown, drain the pool so in-flight queries finish.
export async function shutdown() {
  await pool.end();
}
```

Three things to keep in mind, because Rust changes the ergonomics of all three:

1. The config is a plain object whose fields are easy to forget or mistype (`connectionTimeoutMillis` vs `connectTimeout`).
2. `client.release()` is a manual obligation. The leak when you forget it is a real, common production incident.
3. The pool is a module-level singleton you import; sharing is implicit.

---

## Rust Equivalent

SQLx builds the same pool through `PoolOptions`. Every setting is a typed method, the connection is returned to the pool **automatically when the guard is dropped** (no `finally`, no leak), and the pool handle is a value you clone into your application state.

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite", "postgres"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::time::Duration;

// Shared application state — the SQLx analogue of importing the `pool` singleton.
#[derive(Debug, Clone)]
struct AppState {
    pool: SqlitePool,
}

// Build ONE pool at startup. Every knob is a typed builder method, so a typo
// is a compile error, not a silently-ignored object key.
async fn build_pool(url: &str) -> Result<SqlitePool, sqlx::Error> {
    SqlitePoolOptions::new()
        .max_connections(10)              // hard cap
        .min_connections(2)               // keep this many warm
        .acquire_timeout(Duration::from_secs(5))  // wait up to 5s for a free conn
        .idle_timeout(Duration::from_secs(600))   // close a conn idle for 10 min
        .max_lifetime(Duration::from_secs(1800))  // recycle after 30 min
        .connect(url)
        .await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    // `sqlite::memory:` runs with nothing installed.
    let pool = build_pool("sqlite::memory:").await?;

    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO users (name) VALUES (?)")
        .bind("Alice")
        .execute(&pool)
        .await?;

    // Cloning is a cheap Arc bump; both handles share the SAME connections.
    let state = AppState { pool: pool.clone() };
    let state2 = state.clone();

    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state2.pool)
        .await?;
    println!("user count = {}", row.0);

    // Pool statistics, the way a /metrics or /health endpoint would read them.
    println!("pool size = {}", pool.size());
    println!("idle connections = {}", pool.num_idle());

    // Graceful shutdown: stop handing out connections and drain in-flight ones.
    pool.close().await;
    println!("pool closed = {}", pool.is_closed());

    Ok(())
}
```

Real output:

```text
user count = 1
pool size = 2
idle connections = 1
pool closed = true
```

> **Note:** `pool size = 2` because `min_connections(2)` warmed two connections at startup, even though only one query ran. `idle connections = 1` because the `COUNT(*)` query had checked one out at the moment we measured (the measurement races the connection's return; the point is that `size` ≥ `num_idle`).

For PostgreSQL the only change is the builder type and the connection string:

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

async fn build_pg_pool() -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(20)
        .min_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .max_lifetime(Duration::from_secs(1800))
        // Reads `DATABASE_URL` from the environment / .env in real apps.
        .connect("postgres://postgres:secret@localhost/app")
        .await
}
```

> **Note:** This PostgreSQL builder type-checks and compiles, but `.connect().await` only dials a TCP socket at runtime, so it needs a live server to actually run. The SQLite examples need no server, which is why this page shows live output with SQLite.

---

## Detailed Explanation

### The pool is an `Arc` — clone it, do not rebuild it

`SqlitePool` (an alias for `Pool<Sqlite>`) is internally an `Arc` around the shared connection set, so `pool.clone()` just bumps a reference count. It does **not** open new connections or copy the configuration. This is the whole sharing strategy: build the pool once in `main`, store it in your application state, and `.clone()` it into every request handler and background task. The earlier example spawns eight tasks that all share a four-connection pool:

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

async fn worker(id: u32, pool: SqlitePool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT ?1 * 10")
        .bind(id as i64)
        .fetch_one(&pool)
        .await?;
    Ok(row.0)
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await?;

    // Eight tasks, four connections: the pool queues the extra demand.
    let mut handles = Vec::new();
    for id in 1..=8u32 {
        let pool = pool.clone(); // cheap Arc clone
        handles.push(tokio::spawn(async move { worker(id, pool).await }));
    }

    let mut total = 0;
    for h in handles {
        total += h.await.unwrap()?;
    }
    println!("sum of results = {total}");
    println!("pool max connections = {}", pool.options().get_max_connections());
    Ok(())
}
```

Real output:

```text
sum of results = 360
pool max connections = 4
```

Eight tasks needed connections but only four existed; SQLx queued the surplus and serviced them as connections freed up. This is exactly what you want: the pool is the **back-pressure** mechanism that protects your database from an unbounded fan-out. The shared-ownership machinery behind `Arc` is covered in [Rc and Arc](/10-smart-pointers/01-rc-arc/) and the [Arc/Mutex async pattern](/11-async/12-arc-mutex-pattern/).

### The lifecycle knobs, and what each one protects against

| Setting (SQLx) | Node `pg` equivalent | What it controls | What it protects against |
| --- | --- | --- | --- |
| `max_connections(n)` | `max` | Hard ceiling on open connections | Overwhelming the database's own `max_connections` limit |
| `min_connections(n)` | `min` | Warm connections kept open | Cold-start latency on the first requests |
| `acquire_timeout(d)` | `connectionTimeoutMillis` | How long a checkout waits | A stuck request hanging forever when the pool is saturated |
| `idle_timeout(d)` | `idleTimeoutMillis` | Close a connection idle this long | Holding connections you no longer need |
| `max_lifetime(d)` | `maxLifetimeSeconds` | Recycle a connection after this age | Stale connections, server-side timeouts, load-balancer drift |
| `test_before_acquire(bool)` | (manual) | Ping a connection before lending it | Handing out a connection the server silently closed |

> **Tip:** `min_connections` in SQLx actually **pre-opens** connections in the background, unlike `node-postgres` where `min` only means "don't close below this many once they exist." If cold-start latency matters, SQLx warms the pool for you.

A small program shows the warm-up and a health-check query in action:

```rust
use sqlx::sqlite::SqlitePoolOptions;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .min_connections(3)                // eagerly open 3 at startup
        .acquire_timeout(Duration::from_secs(3))
        .test_before_acquire(true)         // validate before lending
        .connect("sqlite::memory:")
        .await?;

    // Let the background warm-up open the minimum connections.
    tokio::time::sleep(Duration::from_millis(50)).await;
    println!("after warm-up: size = {}, idle = {}", pool.size(), pool.num_idle());

    // The query a /health endpoint runs to prove the DB is reachable.
    sqlx::query("SELECT 1").execute(&pool).await?;
    println!("health check ok");

    Ok(())
}
```

Real output:

```text
after warm-up: size = 3, idle = 3
health check ok
```

### Connections return themselves — no `finally`, no leak

The most important ergonomic difference from `node-postgres`: when you call `pool.acquire().await?` in SQLx you get a `PoolConnection` **guard**, and the connection is returned to the pool automatically when that guard goes out of scope (its `Drop` implementation runs). There is no `client.release()` to forget, because Rust's ownership model runs the cleanup for you. The classic `node-postgres` leak — an early `return` or a thrown error that skips `release()` — simply cannot happen, since `Drop` runs on every exit path including a panic or a `?` early-return. This is the same RAII discipline that makes the SQLx [`Transaction` guard](/17-database/02-sqlx-transactions/) roll back automatically if you drop it without committing.

### When SQLx is not enough: `deadpool` and `bb8`

SQLx's `Pool` only pools SQLx connections. The moment you use a driver that has no built-in pool — raw `tokio-postgres`, the `redis` crate, a custom client — you reach for a **generic** async pooler. The two standards are:

- **`deadpool`**: a lightweight pooler built around a `Manager` trait, with ready-made adapters like `deadpool-postgres` and `deadpool-redis`. It is the more popular choice today: simple, fast, and `async`/`await`-native.
- **`bb8`**: an older but still maintained generic pooler with the same `ManageConnection` concept and adapters like `bb8-postgres` and `bb8-redis`.

Here is a `deadpool-postgres` pool with the same sizing knobs, plus a handler-style checkout:

```toml
# Cargo.toml
[dependencies]
deadpool-postgres = "0.14"
tokio-postgres = "0.7"
tokio = { version = "1", features = ["full"] }
```

```rust
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;

fn build_pool() -> Pool {
    let mut cfg = Config::new();
    cfg.host = Some("localhost".to_string());
    cfg.dbname = Some("app".to_string());
    cfg.user = Some("postgres".to_string());
    cfg.password = Some("secret".to_string());
    // RecyclingMethod::Fast returns a connection without a round-trip check;
    // RecyclingMethod::Verified pings it first (slower, safer).
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    // Pool sizing lives in `cfg.pool`.
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(16));
    cfg.create_pool(Some(Runtime::Tokio1), NoTls).unwrap()
}

// A handler: check a client out, run a query, return it automatically on drop.
async fn count_users(pool: &Pool) -> Result<i64, Box<dyn std::error::Error>> {
    let client = pool.get().await?; // checkout; returned to the pool when dropped
    let row = client.query_one("SELECT COUNT(*) FROM users", &[]).await?;
    Ok(row.get(0))
}

#[tokio::main]
async fn main() {
    let pool = build_pool();
    println!("max size = {}", pool.status().max_size);
    // count_users(&pool) would need a live Postgres server to run.
    let _f = count_users;
}
```

Real output:

```text
max size = 16
```

The `bb8` equivalent uses a fluent builder that should feel familiar after `PgPoolOptions`:

```toml
# Cargo.toml
[dependencies]
bb8 = "0.9"
bb8-postgres = "0.9"
tokio-postgres = "0.7"
tokio = { version = "1", features = ["full"] }
```

```rust
use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use std::time::Duration;
use tokio_postgres::NoTls;

async fn build_pool() -> Pool<PostgresConnectionManager<NoTls>> {
    let manager = PostgresConnectionManager::new_from_stringlike(
        "host=localhost user=postgres dbname=app",
        NoTls,
    )
    .unwrap();

    Pool::builder()
        .max_size(16)
        .min_idle(Some(2))
        .connection_timeout(Duration::from_secs(5))
        .idle_timeout(Some(Duration::from_secs(600)))
        .max_lifetime(Some(Duration::from_secs(1800)))
        // `build_unchecked` returns immediately without opening connections;
        // `build` would eagerly verify one connection and is fallible.
        .build_unchecked(manager)
}

#[tokio::main]
async fn main() {
    let pool = build_pool().await;
    let state = pool.state();
    println!("connections = {}", state.connections);
    println!("idle = {}", state.idle_connections);
    // pool.get().await would need a live Postgres server to run.
    let _ = pool;
}
```

Real output (the pool was built without contacting a server, so both counts start at zero):

```text
connections = 0
idle = 0
```

> **Tip:** If you are using SQLx, you do **not** need `deadpool` or `bb8`. SQLx's own `Pool` covers PostgreSQL, MySQL, and SQLite. Reach for the generic poolers only for drivers without a built-in pool, such as `redis` ([Redis](/17-database/07-redis/)) or a raw `tokio-postgres` client.

---

## Key Differences

| Aspect | TypeScript (`pg.Pool` / Knex / Prisma) | Rust (SQLx / deadpool / bb8) |
| --- | --- | --- |
| Pool construction | a config object, fields easy to mistype | typed builder methods; a typo is a compile error |
| Sharing | module-level singleton you import | a clonable `Arc` handle you pass into state |
| Returning a connection | manual `client.release()` in a `finally` | automatic on `Drop`; cannot be leaked |
| "No free connection" | rejected Promise after `connectionTimeoutMillis` | `Err(PoolTimedOut)` after `acquire_timeout` |
| Connection recycling | `maxLifetimeSeconds`, `idleTimeoutMillis` | `max_lifetime`, `idle_timeout` |
| Pre-warming | `min` keeps existing ones; does not pre-open | `min_connections` actively pre-opens in the background |
| Generic pooling | one pool type fits all `pg` clients | SQLx pool for SQLx; `deadpool`/`bb8` for other drivers |
| Sizing guidance | same rule of thumb | same rule of thumb (see below) |

### Sizing: the rule of thumb is the same as everywhere else

Pool size is a database-side concern, not a Rust one. The widely cited HikariCP guidance applies regardless of language: a small, fixed pool almost always beats a large one. A useful starting point for a CPU-bound workload is `connections = ((core_count * 2) + effective_spindle_count)`, then load-test from there. The key insight that surprises people: a pool of, say, 10 connections frequently out-throughputs a pool of 100, because the database spends less time context-switching and contending on locks. Critically, **the sum of `max_connections` across all your service instances must stay under the database server's own connection limit** (PostgreSQL's `max_connections`, default 100); otherwise new instances fail to connect. If you run many instances or serverless functions, put a server-side pooler like **PgBouncer** in front of PostgreSQL and keep each app pool small.

> **Warning:** The default SQLx `max_connections` is **10**. That is fine for a single instance but will exhaust a default PostgreSQL server (`max_connections = 100`) at just ten app instances. Always set `max_connections` deliberately and divide your database's limit across your fleet.

---

## Common Pitfalls

### Building a new pool per request

The cardinal sin. A pool that you build inside a handler opens fresh connections, serves one request, and is dropped, throwing away every benefit of pooling and hammering the database with connection churn. Build the pool **once** in `main`, store it in shared state, and clone the handle into handlers. This is the Rust equivalent of `new Pool()` on every Express request, and it is just as wrong.

### Forgetting that the connection is held until the guard drops

`pool.acquire().await?` hands you a guard that holds a connection for as long as it is alive. If you bind it to a variable that stays in scope across a long `.await` (a slow HTTP call, a `sleep`), you keep a pooled connection checked out the whole time, starving everyone else. Keep checkouts short: acquire, query, drop. Prefer passing `&pool` directly to `query.execute(&pool)` (which checks out and returns within the single call) over holding a `PoolConnection` across unrelated awaits.

### Pool exhaustion surfaces as a timeout, not a hang

When every connection is busy and a new acquire waits longer than `acquire_timeout`, SQLx returns an error rather than blocking forever. This program forces it by capping the pool at one connection, holding that connection, and asking for a second:

```rust
use sqlx::sqlite::SqlitePoolOptions;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(500))
        .connect("sqlite::memory:")
        .await?;

    // Check out the only connection and keep the guard alive.
    let _held = pool.acquire().await?;

    // Asking for a second one with none free -> times out.
    match pool.acquire().await {
        Ok(_) => println!("acquired a second connection (unexpected)"),
        Err(e) => println!("second acquire failed: {e}"),
    }

    Ok(())
}
```

Real output:

```text
second acquire failed: pool timed out while waiting for an open connection
```

> **Warning:** A flood of these `pool timed out` errors is the unmistakable signature of pool exhaustion: either your pool is too small for the load, or (more often) some code path is holding connections too long. Setting a sane `acquire_timeout` turns a silent hang into a fast, loud failure you can alert on, which is exactly what you want.

### Leaving `max_lifetime` unset behind a load balancer or PgBouncer

Without a `max_lifetime`, a pooled connection can live indefinitely. Behind a network load balancer or PgBouncer that silently drops idle TCP connections, your pool will hand out a dead connection and the next query fails. Set a `max_lifetime` (often 30 minutes) so connections are proactively recycled before any intermediary kills them, and enable `test_before_acquire` if you have seen this in production.

### Assuming `deadpool`/`bb8` settings map one-to-one to SQLx

They cover the same concepts but spell them differently: `deadpool` puts sizing in a `PoolConfig` and uses a `RecyclingMethod` enum; `bb8` uses `.max_size`/`.min_idle`/`.connection_timeout`. Do not copy SQLx method names across; check each crate's builder. The verified examples above show the exact current spelling for `deadpool` 0.14 and `bb8` 0.9.

---

## Best Practices

- **One pool, built once, shared everywhere.** Construct it in `main`, put it in your application state, and clone the `Arc` handle into handlers and tasks. Never build a pool per request.
- **Set `max_connections` deliberately, and keep it small.** Start near `(cores * 2)` and load-test. Make sure the total across all instances fits under the database's own `max_connections`; front PostgreSQL with PgBouncer when you scale out.
- **Always set an `acquire_timeout`.** A bounded wait converts pool exhaustion from a hang into an observable error you can alert on and shed load against.
- **Set `max_lifetime` (and consider `idle_timeout`)** so connections are recycled before a load balancer, firewall, or PgBouncer drops them out from under you.
- **Pre-warm with `min_connections`** if cold-start latency matters; SQLx opens them in the background so the first requests do not pay the dial cost.
- **Keep checkouts short.** Pass `&pool` to queries so the connection is borrowed and returned inside one call; avoid holding a `PoolConnection` guard across unrelated `.await` points.
- **Expose pool health.** `pool.size()` and `pool.num_idle()` make great `/metrics` gauges; a `SELECT 1` against the pool is a solid `/health` readiness check.
- **Use SQLx's own pool for SQLx.** Reach for `deadpool`/`bb8` only for drivers (Redis, raw `tokio-postgres`) that lack a built-in pool.

---

## Real-World Example

A production-shaped startup: build one pool with sensible lifecycle settings, share it through an `AppState` (the same value you would hand to [axum's `State`](/16-web-apis/)), serve concurrent "requests" off the shared pool, then shut down gracefully. This is the skeleton of essentially every Rust web service that talks to a database.

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{FromRow, SqlitePool};
use std::time::Duration;

#[derive(Debug, FromRow)]
struct User {
    id: i64,
    name: String,
}

// Shared application state — exactly what goes behind axum's `State`.
#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
}

// Build ONE pool at startup with production-shaped lifecycle settings.
async fn init_pool() -> Result<SqlitePool, sqlx::Error> {
    SqlitePoolOptions::new()
        .max_connections(20)
        .min_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .max_lifetime(Duration::from_secs(1800))
        .connect("sqlite::memory:") // DATABASE_URL in real apps
        .await
}

// A handler-like function borrows the SHARED pool through state.
async fn list_users(state: &AppState) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT id, name FROM users ORDER BY id")
        .fetch_all(&state.pool)
        .await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = init_pool().await?;
    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&pool)
        .await?;
    for name in ["Alice", "Bob"] {
        sqlx::query("INSERT INTO users (name) VALUES (?)")
            .bind(name)
            .execute(&pool)
            .await?;
    }

    let state = AppState { pool: pool.clone() };

    // Two concurrent "requests" sharing the one pool.
    let s1 = state.clone();
    let s2 = state.clone();
    let (a, b) = tokio::join!(
        tokio::spawn(async move { list_users(&s1).await }),
        tokio::spawn(async move { list_users(&s2).await }),
    );
    let users1 = a.unwrap()?;
    let users2 = b.unwrap()?;
    println!("request 1 saw {} users (first: {})", users1.len(), users1[0].name);
    println!("request 2 saw {} users (first: {})", users2.len(), users2[0].name);

    // Graceful shutdown: stop accepting checkouts and drain in-flight ones.
    pool.close().await;
    println!("pool closed cleanly: {}", pool.is_closed());
    Ok(())
}
```

Real output:

```text
request 1 saw 2 users (first: Alice)
request 2 saw 2 users (first: Alice)
pool closed cleanly: true
```

In a real axum service the `AppState` goes into `Router::with_state`, each handler receives `State(state): State<AppState>`, and you call `pool.close().await` in your shutdown signal handler so in-flight queries finish before the process exits. The pattern is identical to how a database handle is shared across routes in [Section 16: Web APIs](/16-web-apis/).

---

## Further Reading

- [SQLx `Pool` documentation on docs.rs](https://docs.rs/sqlx/latest/sqlx/struct.Pool.html) — the full pool API, including `acquire`, `close`, `size`, and `num_idle`.
- [SQLx `PoolOptions` documentation](https://docs.rs/sqlx/latest/sqlx/pool/struct.PoolOptions.html) — every sizing and lifecycle knob with its defaults.
- [`deadpool` on docs.rs](https://docs.rs/deadpool) and [`bb8` on docs.rs](https://docs.rs/bb8) — the generic async poolers and their `Manager`/`ManageConnection` traits.
- [About Pool Sizing (HikariCP wiki)](https://github.com/brettwooldridge/HikariCP/wiki/About-Pool-Sizing) — the canonical, language-agnostic argument for small pools.
- [SQLx Intro](/17-database/00-sqlx-intro/) — building a pool, feature flags, and connecting to PostgreSQL/SQLite.
- [SQLx Queries](/17-database/01-sqlx-queries/) and [SQLx Transactions](/17-database/02-sqlx-transactions/) — running statements against the pool and the RAII transaction guard.
- [Redis](/17-database/07-redis/) and [MongoDB](/17-database/06-mongodb/) — drivers where you may add `deadpool`/`bb8` for pooling.
- [Migrations](/17-database/09-migrations/) — running migrations against the pool at startup.
- [SQLx vs Diesel vs SeaORM](/17-database/10-orm-comparison/) — how each library handles pooling.
- Prerequisites: [Async/Await](/11-async/01-async-await/), [Tokio Intro](/11-async/02-tokio-intro/), [Rc and Arc](/10-smart-pointers/01-rc-arc/), and [Ownership Basics](/05-ownership/).
- Next steps in tooling: [Section 18: CLI Tools](/18-cli-tools/) covers the command-line utilities you run alongside a pooled database.

---

## Exercises

### Exercise 1: Build and inspect a pool

**Difficulty:** Beginner

**Objective:** Build a SQLite pool with explicit sizing and read back its configuration and live stats.

**Instructions:** Using an in-memory SQLite database, build a pool with `max_connections(8)`, `min_connections(2)`, and a 2-second `acquire_timeout`. Run a `SELECT 1` to prove it connects, then print the pool's max and min connection settings via `pool.options()`.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sqlx::sqlite::SqlitePoolOptions;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(2))
        .connect("sqlite::memory:")
        .await?;

    sqlx::query("SELECT 1").execute(&pool).await?;
    println!("connected; max = {}", pool.options().get_max_connections());
    println!("min = {}", pool.options().get_min_connections());
    Ok(())
}
```

Real output:

```text
connected; max = 8
min = 2
```

</details>

### Exercise 2: Trigger pool exhaustion

**Difficulty:** Intermediate

**Objective:** See firsthand how a saturated pool turns into a timeout error rather than a hang.

**Instructions:** Build a pool capped at a single connection with a 500 ms `acquire_timeout`. Check out the only connection into a variable and keep it alive. Then call `pool.acquire().await` again and `match` on the result, printing the error message when the second acquire fails.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sqlx::sqlite::SqlitePoolOptions;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(500))
        .connect("sqlite::memory:")
        .await?;

    let _held = pool.acquire().await?; // hold the only connection

    match pool.acquire().await {
        Ok(_) => println!("acquired a second connection (unexpected)"),
        Err(e) => println!("second acquire failed: {e}"),
    }
    Ok(())
}
```

Real output:

```text
second acquire failed: pool timed out while waiting for an open connection
```

> **Tip:** Dropping `_held` (for example by ending its scope before the second acquire) would let the second checkout succeed — proof that the connection returns itself on `Drop`, with no `release()` call.

</details>

### Exercise 3: Build the pool from explicit `ConnectOptions`

**Difficulty:** Advanced

**Objective:** Configure connection-level options (not just sizing) by building the pool from a `ConnectOptions` value, and turn off SQLx's statement logging.

**Instructions:** Instead of passing a URL string to `.connect(...)`, parse a `SqliteConnectOptions` from `"sqlite::memory:"`, disable statement logging with `.log_statements(log::LevelFilter::Off)`, and build the pool with `.connect_with(...)`. Set `max_connections(8)` and `min_connections(2)`, run a `SELECT 1`, and print the effective max and min. (Add the `log` crate for the `LevelFilter`.)

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite"] }
tokio = { version = "1", features = ["full"] }
log = "0.4"
```

```rust
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::ConnectOptions;
use std::str::FromStr;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    // Connection-LEVEL options (logging, pragmas, etc.) live on ConnectOptions.
    let connect_opts = SqliteConnectOptions::from_str("sqlite::memory:")?
        .log_statements(log::LevelFilter::Off);

    // Pool-LEVEL options (sizing, timeouts) live on PoolOptions.
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(2))
        .connect_with(connect_opts)
        .await?;

    sqlx::query("SELECT 1").execute(&pool).await?;
    println!("connected; max = {}", pool.options().get_max_connections());
    println!("min = {}", pool.options().get_min_connections());
    Ok(())
}
```

Real output:

```text
connected; max = 8
min = 2
```

> **Tip:** The split is deliberate: `ConnectOptions` configures *each connection* (logging, TLS, SQLite pragmas), while `PoolOptions` configures *the pool around them* (sizing, timeouts, lifecycle). `connect_with` is how you combine the two.

</details>
