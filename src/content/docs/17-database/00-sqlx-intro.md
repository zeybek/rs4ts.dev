---
title: "SQLx: Async, Compile-Time-Checked SQL"
description: "SQLx is an async Rust SQL toolkit, not an ORM: you write raw SQL but its macros connect to your real database at compile time, so a bad column fails the"
---

In the TypeScript world you talk to a SQL database through a query builder or an ORM — Knex, Prisma, TypeORM — and your queries are checked (if at all) only when they actually run against the database. **SQLx** takes a strikingly different position: you write **raw SQL**, but a macro connects to your real database **at compile time** and verifies the query and its result types before your program ever runs. This page is the on-ramp: what SQLx is, how to add it, and how to connect to PostgreSQL and SQLite.

---

## Quick Overview

**SQLx** is an async, pure-Rust SQL toolkit. It is **not** an ORM: there is no entity mapping, no migration-from-decorators magic, no lazy-loaded relations. You write SQL strings, SQLx runs them, and it maps the rows back into Rust types. Its headline feature is the `query!`/`query_as!` macro family, which uses a live database connection **during `cargo build`** to type-check your SQL: a misspelled column or a type mismatch becomes a **compile error**, not a 500 at 3 a.m. SQLx is fully `async` (it runs on the [Tokio runtime](/11-async/02-tokio-intro/)) and supports PostgreSQL, MySQL/MariaDB, and SQLite from a single API.

> **Note:** Every Rust snippet on this page was compiled and run with `cargo` 1.96.0 (current stable; 2024 edition, which `cargo new` selects automatically). These examples pin **SQLx 0.8** deliberately. SQLx 0.9 is what `cargo add sqlx` now resolves to (it requires Rust ≥ 1.94) and it renames the runtime/TLS feature flags. On 0.9 the combined `runtime-tokio-rustls` is split into `runtime-tokio` + `tls-rustls`, so a 0.9 dependency line reads `sqlx = { version = "0.9", features = ["runtime-tokio", "tls-rustls", "sqlite"] }`. The `query`/`query_as`/`Pool`/`FromRow` API shown here is unchanged across both releases. Examples use an in-memory or file-based **SQLite** database so they reproduce with no server to install.

---

## TypeScript/JavaScript Example

Here is a typical Knex setup in TypeScript: a connection pool, a table, an insert with bound parameters, and a typed-ish read. Knex returns `any[]` from a raw query, so the `as User[]` is a *cast you are trusting*, not a check.

```typescript
// db.ts
import knex from "knex";

interface User {
  id: number;
  name: string;
  email: string;
}

// A connection POOL, not a single connection (Knex manages it for you).
const db = knex({
  client: "pg",
  connection: "postgres://user:pass@localhost/app",
  pool: { min: 2, max: 10 },
});

async function main() {
  await db.raw(`
    CREATE TABLE IF NOT EXISTS users (
      id    SERIAL PRIMARY KEY,
      name  TEXT NOT NULL,
      email TEXT NOT NULL UNIQUE
    )
  `);

  // Bound parameters ($1, $2) — Knex escapes them, preventing SQL injection.
  await db.raw("INSERT INTO users (name, email) VALUES (?, ?)", [
    "Alice",
    "alice@example.com",
  ]);

  // The cast `as User[]` is a PROMISE to the compiler, checked by nobody.
  const rows = (await db.raw("SELECT id, name, email FROM users")).rows as User[];
  console.log(rows); // [ { id: 1, name: 'Alice', email: 'alice@example.com' } ]

  // A typo here ('usrname') compiles fine and blows up only at runtime:
  // const bad = await db.raw("SELECT usrname FROM users");

  await db.destroy();
}

main();
```

Three things to hold in mind, because SQLx changes all three:

1. The query string is opaque to TypeScript: `SELECT usrname` type-checks and ships.
2. `db.raw(...).rows as User[]` is an **unchecked cast**; the runtime shape may not match.
3. The pool is configured imperatively and `db` is a shared handle you clone references to everywhere.

---

## Rust Equivalent

The same program in Rust with SQLx. Add the crate first — note the **feature flags** that select the runtime, TLS backend, and which databases you want compiled in:

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite", "postgres"] }
tokio = { version = "1", features = ["full"] }
```

```bash
# Or, equivalently, from a shell:
cargo add sqlx@0.8 --features runtime-tokio-rustls,sqlite,postgres
cargo add tokio --features full
```

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{FromRow, Row};

// The same shape as the TypeScript `interface User`.
#[derive(Debug, FromRow)]
struct User {
    id: i64,
    name: String,
    email: String,
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    // Build a connection POOL (not a single connection). `sqlite::memory:`
    // is an in-memory DB so this runs with nothing installed.
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite::memory:")
        .await?;

    // A statement that returns no rows: `.execute(...)`.
    sqlx::query(
        "CREATE TABLE users (
            id    INTEGER PRIMARY KEY,
            name  TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE
        )",
    )
    .execute(&pool)
    .await?;

    // Bound parameters via `.bind(...)`. Each `?` is filled positionally.
    let result = sqlx::query("INSERT INTO users (name, email) VALUES (?, ?)")
        .bind("Alice")
        .bind("alice@example.com")
        .execute(&pool)
        .await?;
    println!(
        "inserted {} row(s), last id = {}",
        result.rows_affected(),
        result.last_insert_rowid()
    );

    // Read MANY rows into the typed struct via `query_as` + `FromRow`.
    let users: Vec<User> = sqlx::query_as::<_, User>("SELECT id, name, email FROM users")
        .fetch_all(&pool)
        .await?;
    println!("{users:?}");

    // Read ONE row and pull a column out dynamically (no struct).
    let row = sqlx::query("SELECT COUNT(*) AS count FROM users")
        .fetch_one(&pool)
        .await?;
    let count: i64 = row.get("count");
    println!("user count = {count}");

    Ok(())
}
```

Real output:

```text
inserted 1 row(s), last id = 1
[User { id: 1, name: "Alice", email: "alice@example.com" }]
user count = 1
```

The PostgreSQL version differs only in the connection string and the placeholder syntax (`$1`, `$2` instead of `?`):

```rust
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    // The connection string carries everything; the pool dials at `.connect().await`.
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect("postgres://user:pass@localhost/app")
        .await?;

    // Postgres uses $1, $2 placeholders instead of `?`.
    let row = sqlx::query("SELECT id, name FROM users WHERE id = $1")
        .bind(1_i32)
        .fetch_one(&pool)
        .await?;
    println!("loaded user #{}", row.get::<i32, _>("id"));

    Ok(())
}
```

> **Note:** This PostgreSQL snippet type-checks and builds, but it only attempts a TCP connection when `.connect().await` runs, so it needs a real server to *execute*. The SQLite examples above need no server, which is why this page demonstrates live output with SQLite.

---

## Detailed Explanation

### Feature flags decide what gets compiled

Unlike `npm install knex` (which always pulls everything), SQLx uses **Cargo features** to compile only what you ask for. The three categories you almost always set:

- **Runtime + TLS:** `runtime-tokio-rustls` ties SQLx to the Tokio async runtime and the pure-Rust `rustls` TLS stack. (`runtime-tokio-native-tls` uses the system TLS instead.) SQLx does not bundle an executor, exactly like the rest of Rust async. See [Tokio Setup](/11-async/03-tokio-setup/).
- **Drivers:** `postgres`, `mysql`, `sqlite`. Pick the databases you actually use; each is a separately compiled driver.
- **Extras (optional):** `macros` (the `query!` family — on by default), `migrate` (the embedded migration runner), `chrono`/`time`/`uuid`/`json` to map those types to/from SQL columns.

### The pool is a cheap, clonable handle

`SqlitePoolOptions`/`PgPoolOptions` build a `Pool<DB>`. A `Pool` is an `Arc` inside, so cloning it is cheap and every clone shares the same set of connections. This is the idiomatic way to hand a database to many request handlers or background tasks. (The [Arc/Mutex pattern](/11-async/12-arc-mutex-pattern/) explains the shared-ownership machinery; connection-pool sizing and lifecycle get their own page in [Connection Pooling](/17-database/08-connection-pooling/).) You pass `&pool` to every query; SQLx checks out a connection, runs the statement, and returns it to the pool.

### `execute` vs `fetch_*`: pick by what comes back

| Method | Returns | Use for |
| --- | --- | --- |
| `.execute(&pool)` | a results struct (`rows_affected`, `last_insert_rowid`) | `INSERT`/`UPDATE`/`DELETE`/DDL |
| `.fetch_one(&pool)` | exactly one row (errors if 0 rows) | a guaranteed single result |
| `.fetch_optional(&pool)` | `Option<Row>` | a "maybe one" lookup |
| `.fetch_all(&pool)` | `Vec<Row>` | the whole result set |
| `.fetch(&pool)` | a `Stream` of rows | huge result sets, row-by-row |

`fetch_optional` is the SQL analogue of TypeScript's `T | undefined`: SQLx makes "no row" a first-class `Option::None` you must handle, instead of a `rows[0]` that is silently `undefined`.

### Two query styles: dynamic `query()` vs the checked `query!` macro

There are two API layers, and the difference is the whole point of SQLx.

**1. The function `sqlx::query(...)` / `sqlx::query_as::<_, T>(...)`** takes a `&str` at runtime. It is flexible (you can build the SQL string dynamically) but the SQL is **not** validated until execution, much like Knex's `db.raw`. You read columns either via `FromRow` (`query_as`) or by hand with `row.get("col")`.

**2. The macros `sqlx::query!(...)` / `sqlx::query_as!(...)`** connect to your real database **while the compiler runs** and verify the SQL string against the live schema. Consider:

```rust
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    // `std::env::var` reads the process env, NOT a .env file. Export
    // DATABASE_URL (or load .env yourself; see the note below) before running.
    let pool = SqlitePoolOptions::new()
        .connect(&std::env::var("DATABASE_URL").unwrap())
        .await?;

    // The macro returns an anonymous struct whose fields are named after the
    // columns and TYPED FROM THE SCHEMA (id: i64, name: String).
    let rows = sqlx::query!("SELECT id, name FROM users WHERE id > ?", 0)
        .fetch_all(&pool)
        .await?;

    for r in &rows {
        // r.id and r.name are statically typed; a typo is a COMPILE error.
        println!("{}: {}", r.id, r.name);
    }
    Ok(())
}
```

With a `users` table seeded with Alice and Bob, this prints:

```text
1: Alice
2: Bob
```

The macro found the database via the `DATABASE_URL` environment variable. SQLx's `.env` auto-loading is a **compile-time** convenience: the `query!`/`query_as!` macros read a `.env` file during `cargo build` to find the schema. It does **not** apply at runtime: the example's `std::env::var("DATABASE_URL")` reads the process environment only, so you must export `DATABASE_URL` yourself (or load the `.env` in code, e.g. `cargo add dotenvy` and call `dotenvy::dotenv().ok();` at the top of `main`). Otherwise the build succeeds but `.connect(&std::env::var("DATABASE_URL").unwrap())` panics at runtime with a `NotPresent` error. The deep difference from every TypeScript tool is that the schema is consulted by the **compiler**. The mechanics of binding parameters, `FromRow`, and the full macro vocabulary are covered in [SQLx Queries](/17-database/01-sqlx-queries/); this page only introduces the idea.

### `?` propagates database errors like a thrown rejection

Each SQLx call returns a `Result<_, sqlx::Error>`. The `?` operator forwards a failure up to the caller, the same way an `await`ed Promise rejection unwinds an `async function`. Because `main` here returns `Result<(), sqlx::Error>`, a failure prints a `Debug` of the error and exits non-zero. In real services you map `sqlx::Error` into your own error type. See [Error Handling](/08-error-handling/) and the Real-World Example below.

---

## Key Differences

| Aspect | TypeScript (Knex / Prisma / TypeORM) | Rust (SQLx) |
| --- | --- | --- |
| Category | query builder / ORM | SQL toolkit (**not** an ORM) |
| You write | builder calls or decorated entities | **raw SQL strings** |
| Query validation | at runtime (or never, for `raw`) | **at compile time** via `query!` macros |
| Result typing | `any` / a cast you trust | a real struct or a compiler-generated typed struct |
| Async model | Promises (eager, auto-scheduled) | Futures (lazy; need a runtime) — see [Promises vs Futures](/11-async/00-promises-vs-futures/) |
| Migrations | framework-specific CLI | `sqlx migrate` (see [Migrations](/17-database/09-migrations/)) |
| Relations / eager loading | first-class (`include`, `relations`) | none; you write the `JOIN` yourself |
| Connection pool | configured object, often a singleton | `Pool<DB>`, a cheap clonable `Arc` |

The mental model to adopt: **SQLx is closer to a really good driver than to Prisma.** If you want the ORM experience (entities, a query DSL, associations), that is [Diesel](/17-database/03-diesel-intro/) or [SeaORM](/17-database/11-sea-orm/). SQLx's bet is that *SQL is the right abstraction* and the compiler should check it for you.

> **Tip:** "Compile-time-checked" does not mean "compiled into the binary with no database." It means the macro phoned a live database during `cargo build`. For builds in CI or Docker where no database is reachable, SQLx supports **offline mode**: run `cargo sqlx prepare` to cache the query metadata into a `.sqlx/` directory that you commit, and the macros validate against that cache instead. More in [Migrations](/17-database/09-migrations/) and the SQLx docs.

---

## Common Pitfalls

### Forgetting `DATABASE_URL` (the macros need a live database to compile)

The single most common surprise: `query!`/`query_as!` will not compile unless they can reach a database. If `DATABASE_URL` is unset and there is no offline cache, the build fails. This is the **real** error message:

```text
error: set `DATABASE_URL` to use query macros online, or run `cargo sqlx prepare` to update the query cache
  --> src/main.rs:17:17
   |
17 |       let users = sqlx::query_as!(
   |  _________________^
18 | |         User,
19 | |         r#"SELECT id as "id!", name, email FROM users ORDER BY id"#
...
```

> **Warning:** This trips up CI pipelines and `docker build` constantly, because those environments often have no database. The fix is **offline mode** (`cargo sqlx prepare`, then commit `.sqlx/`), or use the non-macro `sqlx::query()` API which never touches the database at build time.

### A typo in your SQL is now a build error (this is the feature, not a bug)

With the macros, a wrong column name fails the build with the database's own message embedded. Misspelling `name` as `usrname` produces this **real** compile error:

```text
error: error returned from database: (code: 1) no such column: usrname
  --> src/main.rs:10:16
   |
10 |     let rows = sqlx::query!("SELECT id, usrname FROM users")
   |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

In TypeScript that same typo would compile and only fail when the query ran. Treat this error as SQLx doing its job.

### SQLite reports primary keys as nullable — the `"col!"` cast

SQLite's type inference is weaker than PostgreSQL's, and it reports `INTEGER PRIMARY KEY` columns as **nullable**. So `sqlx::query!("SELECT id FROM users")` infers `id: Option<i64>`, and assigning it to an `i64` field fails:

```text
error[E0308]: mismatched types
37 |         Ok(rec.map(|r| User { id: r.id, name: r.name, email: r.email }))
   |                                   ^^^^ expected `i64`, found `Option<i64>`
   = note:    found enum `Option<i64>`
```

The fix is SQLx's **forced-non-null override** in the SQL: alias the column with a trailing `!`. (PostgreSQL usually does not need this, because it tracks nullability precisely.)

```rust
// infers id: Option<i64> on SQLite
// let rec = sqlx::query!("SELECT id, name FROM users").fetch_one(&pool).await?;

// `as "id!"` tells the macro the column is non-null -> id: i64
// let rec = sqlx::query!(r#"SELECT id as "id!", name FROM users"#).fetch_one(&pool).await?;
```

### Bind-parameter type mismatches with `query()` are caught at runtime, not compile time

The dynamic `sqlx::query("... = ?").bind(value)` API does **not** check that `value`'s Rust type matches the column. A wrong type surfaces as a `sqlx::Error` when the statement runs. If you want that checked at compile time, use the `query!` macros, which validate every bind against the schema. (This is the trade-off table in reverse: flexibility vs. compile-time safety.)

### Forgetting the runtime/TLS feature flag

If you `cargo add sqlx` without a `runtime-*` feature, you get link/usage errors because SQLx has no executor selected. Always include exactly one runtime feature (e.g. `runtime-tokio-rustls`). This mirrors the broader Rust async rule: futures are lazy and need a runtime to drive them.

---

## Best Practices

- **Pick `query!`/`query_as!` by default.** The compile-time safety is the reason to choose SQLx over a bare driver. Drop to dynamic `sqlx::query()` only when the SQL genuinely must be built at runtime.
- **Commit your offline cache.** Run `cargo sqlx prepare` and check in `.sqlx/` so CI and Docker builds work without a live database. Re-run it whenever a query or the schema changes.
- **One pool per application, cloned everywhere.** Build the `Pool` once at startup, store it in your shared state, and `.clone()` it into handlers — never open a fresh connection per request. See [Connection Pooling](/17-database/08-connection-pooling/).
- **Use a `.env` with `DATABASE_URL`.** The `query!` macros and `sqlx-cli` read it automatically at compile time, but your runtime code does not. Load it yourself with `dotenvy::dotenv().ok();` (or export the variable) so `std::env::var` can see it. Keep `.env` out of version control.
- **Enable type-mapping features you need** (`uuid`, `chrono`/`time`, `json`) so columns map to real Rust types instead of strings.
- **Let `?` carry `sqlx::Error` up to a boundary**, then convert to your domain error there (with `thiserror` or `anyhow`) rather than matching on it at every call site.

---

## Real-World Example

A small `UserRepo` over a connection pool: the SQLx equivalent of a TypeORM repository. It uses the compile-time-checked macros, `RETURNING` to get the inserted row back in one round-trip, and `fetch_optional` for "maybe one" lookups. Errors flow through `anyhow`.

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite"] }
tokio = { version = "1", features = ["full"] }
anyhow = "1"
dotenvy = "0.15"
```

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
struct User {
    id: i64,
    name: String,
    email: String,
}

// A repository holding a clonable pool handle — share one across the app.
#[derive(Clone)]
struct UserRepo {
    pool: SqlitePool,
}

impl UserRepo {
    fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn create(&self, name: &str, email: &str) -> anyhow::Result<User> {
        // RETURNING gives us the inserted row in a single round-trip.
        // `as "id!"` forces the SQLite primary key to non-null (i64, not Option).
        let rec = sqlx::query!(
            r#"INSERT INTO users (name, email)
               VALUES (?, ?)
               RETURNING id as "id!", name, email"#,
            name,
            email
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(User { id: rec.id, name: rec.name, email: rec.email })
    }

    async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<User>> {
        let rec = sqlx::query!(
            r#"SELECT id as "id!", name, email FROM users WHERE email = ?"#,
            email
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(rec.map(|r| User { id: r.id, name: r.name, email: r.email }))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load a .env into the process environment at RUNTIME. (SQLx's macros read
    // .env at compile time; `std::env::var` at runtime does not — so without
    // this line you would need to `export DATABASE_URL=...` first.)
    dotenvy::dotenv().ok();

    // In production DATABASE_URL comes from the environment / .env.
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;

    let repo = UserRepo::new(pool);

    let carol = repo.create("Carol", "carol@example.com").await?;
    println!("created: {carol:?}");

    match repo.find_by_email("carol@example.com").await? {
        Some(u) => println!("found: {} <{}>", u.name, u.email),
        None => println!("not found"),
    }
    Ok(())
}
```

With `DATABASE_URL` pointing at a SQLite file that has the `users` table, this prints (real output):

```text
created: User { id: 3, name: "Carol", email: "carol@example.com" }
found: Carol <carol@example.com>
```

The `UserRepo` is `Clone` because the `Pool` inside it is a cheap `Arc`. In a web service you would build the pool once in `main`, wrap the repo in your application state, and clone it into every handler, exactly how a database handle is shared across routes in the web frameworks covered in [Section 16](/16-web-apis/).

---

## Further Reading

- [SQLx documentation on docs.rs](https://docs.rs/sqlx): the full API reference.
- [SQLx repository and README](https://github.com/launchbadge/sqlx): feature flags, offline mode, and the macro internals.
- [The `query!` macro reference](https://docs.rs/sqlx/latest/sqlx/macro.query.html): compile-time checking, nullability overrides, and the `"col!"` syntax.
- [SQLx Queries](/17-database/01-sqlx-queries/) — `query!`/`query_as!` in depth, binding parameters (and how that prevents SQL injection), and `FromRow`.
- [SQLx Transactions](/17-database/02-sqlx-transactions/) — `begin`/`commit`/`rollback` and the transaction guard.
- [Connection Pooling](/17-database/08-connection-pooling/): sizing, lifecycle, and `deadpool`/`bb8`.
- [Migrations](/17-database/09-migrations/) — `sqlx migrate`, up/down scripts, offline-mode caching, and running migrations at startup.
- [SQLx vs Diesel vs SeaORM](/17-database/10-orm-comparison/) — when to choose compile-checked SQL over an ORM.
- Prerequisites: [Async/Await](/11-async/01-async-await/), [Tokio Intro](/11-async/02-tokio-intro/), and [Error Handling](/08-error-handling/).
- Next steps in tooling: [Section 18: CLI Tools](/18-cli-tools/) covers building the `sqlx`-style command-line utilities you will use alongside a database.

---

## Exercises

### Exercise 1: Connect and count

**Difficulty:** Beginner

**Objective:** Get comfortable building a pool and running a no-row statement plus a single-row read.

**Instructions:** Using an in-memory SQLite database (`sqlite::memory:`), create a `products` table with columns `id INTEGER PRIMARY KEY`, `name TEXT NOT NULL`, and `price REAL NOT NULL`. Insert two products with bound parameters, then run `SELECT COUNT(*)` and print the count. Use the dynamic `sqlx::query(...)` API (no macros, so no `DATABASE_URL` needed).

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
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite::memory:")
        .await?;

    sqlx::query(
        "CREATE TABLE products (
            id    INTEGER PRIMARY KEY,
            name  TEXT NOT NULL,
            price REAL NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    for (name, price) in [("Keyboard", 49.99), ("Mouse", 19.50)] {
        sqlx::query("INSERT INTO products (name, price) VALUES (?, ?)")
            .bind(name)
            .bind(price)
            .execute(&pool)
            .await?;
    }

    let row = sqlx::query("SELECT COUNT(*) AS count FROM products")
        .fetch_one(&pool)
        .await?;
    let count: i64 = row.get("count");
    println!("product count = {count}");
    Ok(())
}
```

Real output:

```text
product count = 2
```

</details>

### Exercise 2: Typed reads with `FromRow`

**Difficulty:** Intermediate

**Objective:** Map result rows into your own struct instead of pulling columns by name.

**Instructions:** Extend Exercise 1. Define a `Product` struct deriving `FromRow` (fields `id: i64`, `name: String`, `price: f64`). Use `sqlx::query_as::<_, Product>(...)` with `fetch_all` to load every product ordered by `price` descending, and print each one. Then use `fetch_optional` to look up a product by name that does **not** exist, and confirm you get `None`.

<details>
<summary>Solution</summary>

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::FromRow;

#[derive(Debug, FromRow)]
struct Product {
    id: i64,
    name: String,
    price: f64,
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await?;

    sqlx::query(
        "CREATE TABLE products (
            id    INTEGER PRIMARY KEY,
            name  TEXT NOT NULL,
            price REAL NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    for (name, price) in [("Keyboard", 49.99), ("Mouse", 19.50)] {
        sqlx::query("INSERT INTO products (name, price) VALUES (?, ?)")
            .bind(name)
            .bind(price)
            .execute(&pool)
            .await?;
    }

    // Many rows into Product via FromRow.
    let products: Vec<Product> =
        sqlx::query_as::<_, Product>("SELECT id, name, price FROM products ORDER BY price DESC")
            .fetch_all(&pool)
            .await?;
    for p in &products {
        println!("{}: {} (${:.2})", p.id, p.name, p.price);
    }

    // A "maybe one" lookup that finds nothing -> None.
    let missing: Option<Product> =
        sqlx::query_as::<_, Product>("SELECT id, name, price FROM products WHERE name = ?")
            .bind("Monitor")
            .fetch_optional(&pool)
            .await?;
    println!("missing lookup = {missing:?}");
    Ok(())
}
```

Real output:

```text
1: Keyboard ($49.99)
2: Mouse ($19.50)
missing lookup = None
```

</details>

### Exercise 3: Compile-time-checked insert with `RETURNING`

**Difficulty:** Advanced

**Objective:** Use the `query!` macro against a real SQLite file database and handle SQLite's nullable-primary-key quirk.

**Instructions:** Create a file database `notes.db` with a table `notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL)`. Set `DATABASE_URL=sqlite://<absolute path>/notes.db` (e.g. in a `.env` file). Write an `async fn add_note(pool, body)` that uses `sqlx::query!` with `INSERT ... RETURNING` to insert a note and return its new `id` as an `i64`. Remember the `as "id!"` cast. Call it twice in `main` and print both ids.

<details>
<summary>Solution</summary>

Setup (run once before building, since the macro needs the schema at compile time):

```bash
sqlite3 notes.db "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL);"
echo "DATABASE_URL=sqlite://$(pwd)/notes.db" > .env
```

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite"] }
tokio = { version = "1", features = ["full"] }
anyhow = "1"
dotenvy = "0.15"
```

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

async fn add_note(pool: &SqlitePool, body: &str) -> anyhow::Result<i64> {
    // `as "id!"` overrides SQLite's nullable inference so `id` is i64, not Option<i64>.
    let rec = sqlx::query!(
        r#"INSERT INTO notes (body) VALUES (?) RETURNING id as "id!""#,
        body
    )
    .fetch_one(pool)
    .await?;
    Ok(rec.id)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load the .env at runtime so `std::env::var` can see DATABASE_URL.
    // (The query! macro already read it at compile time; this is for runtime.)
    dotenvy::dotenv().ok();

    let pool = SqlitePoolOptions::new()
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;

    let first = add_note(&pool, "buy milk").await?;
    let second = add_note(&pool, "call dentist").await?;
    println!("inserted note ids: {first}, {second}");
    Ok(())
}
```

Real output (ids depend on existing rows; on a fresh `notes.db`):

```text
inserted note ids: 1, 2
```

> **Tip:** If you remove the `as "id!"` cast, the build fails with `expected i64, found Option<i64>` — proof that the macro inferred the column type from the live schema at compile time.

</details>
