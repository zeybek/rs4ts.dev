---
title: "Database Migrations: SQLx and Diesel"
description: "Run database migrations in Rust with SQLx and Diesel, writing up/down scripts and applying them at startup, with migrations embedded right into the compiled binary."
---

In a Node project you reach for a migration tool to evolve your schema over time: `knex migrate`, TypeORM's `migration:generate`/`migration:run`, or Prisma Migrate. Rust has the same idea, expressed two ways: **SQLx** ships a `sqlx migrate` CLI plus a `sqlx::migrate!` macro that embeds your SQL into the binary, and **Diesel** ships a `diesel migration` CLI plus a `diesel_migrations::embed_migrations!` macro. This page covers writing reversible **up/down** scripts, applying and reverting them, and the pattern most servers want: **running pending migrations automatically at startup**.

---

## Quick Overview

A **migration** is a small, ordered, versioned change to your database schema ("create the `users` table", "add a `bio` column"), recorded so every environment converges on the same schema. Both SQLx and Diesel store the list of already-applied migrations in a bookkeeping table (`_sqlx_migrations` / `__diesel_schema_migrations`) so re-running is a safe no-op. The big difference from the TypeScript tools you know: Rust migration runners can **embed** every migration file into the compiled binary, so the deployed executable carries its own schema history and needs no migration files alongside it on the server.

> **Note:** Every command and Rust snippet here was run with `cargo` 1.96.0 (current stable; 2024 edition, which `cargo new` selects automatically). The CLI examples use **SQLite** so they reproduce with no database server to install. The current SQLx release is **0.9** (`cargo add sqlx` resolves it on Rust ≥ 1.94; on older toolchains it falls back to **0.8.6**), and the `migrate!`/`sqlx migrate` APIs shown are identical across 0.8 and 0.9. Other crate versions at the time of writing: `sqlx-cli` **0.9**, Diesel **2.3.9**, `diesel_migrations` **2.3.2**.

---

## TypeScript/JavaScript Example

Here is a typical Knex migration workflow. You generate a timestamped file, fill in `up` and `down`, and run the migrator. The migration list lives in a `knex_migrations` table.

```typescript
// migrations/20260601120000_create_users.ts
import type { Knex } from "knex";

export async function up(knex: Knex): Promise<void> {
  await knex.schema.createTable("users", (t) => {
    t.increments("id").primary();
    t.string("name").notNullable();
    t.string("email").notNullable().unique();
  });
}

export async function down(knex: Knex): Promise<void> {
  await knex.schema.dropTable("users");
}
```

```typescript
// run-migrations.ts — typically called on deploy, or at server startup
import knex from "knex";

const db = knex({
  client: "pg",
  connection: process.env.DATABASE_URL,
  migrations: { directory: "./migrations" },
});

async function main() {
  // Applies every pending migration in order; records them in `knex_migrations`.
  const [batch, log] = await db.migrate.latest();
  console.log(`batch ${batch} ran ${log.length} migration(s)`);
  await db.destroy();
}

main();
```

```bash
# CLI equivalents
npx knex migrate:make create_users   # generate a timestamped migration file
npx knex migrate:latest              # apply all pending
npx knex migrate:rollback            # undo the last batch
```

Three things to keep in mind, because Rust does each of them a little differently:

1. The `up`/`down` logic is **TypeScript code** calling a schema builder; the SQL is generated for you.
2. Migration files are **read from disk at runtime** — they must be shipped next to the app.
3. The order is the **filename timestamp**; the `knex_migrations` table prevents double-applying.

---

## Rust Equivalent

In Rust the up/down logic is **plain SQL** in `.sql` files, and the runner can embed those files into the binary. Here is the SQLx version. First the CLI to scaffold and run, then the code that runs migrations at startup.

```bash
# Install the CLI once (built with only the drivers you need; here SQLite + rustls).
cargo install sqlx-cli --no-default-features --features sqlite,rustls

# Point at the database. SQLx reads DATABASE_URL (a .env file works too).
export DATABASE_URL="sqlite://app.db"
sqlx database create

# Scaffold a *reversible* migration: -r generates BOTH an up and a down file.
sqlx migrate add -r create_users
```

That creates two SQL files under `migrations/` (the prefix is a timestamp):

```sql
-- migrations/20260601115557_create_users.up.sql
CREATE TABLE users (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE
);
```

```sql
-- migrations/20260601115557_create_users.down.sql
DROP TABLE users;
```

Apply and inspect them with the CLI:

```bash
sqlx migrate info   # show status of every migration
sqlx migrate run    # apply all pending migrations
sqlx migrate revert # undo the most recent migration (runs its down.sql)
```

And the production pattern — embed the migrations into the binary and run them on boot:

```toml
# Cargo.toml — `migrate` enables the runner, `macros` enables the `migrate!` macro.
[dependencies]
sqlx = { version = "0.9", default-features = false, features = ["runtime-tokio", "sqlite", "migrate", "macros"] }
tokio = { version = "1", features = ["full"] }
anyhow = "1"
```

```rust
use sqlx::sqlite::SqlitePoolOptions;

// `migrate!()` reads the ./migrations directory at COMPILE time and embeds the SQL
// into the binary, so the deployed executable carries its own migrations.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("DATABASE_URL")?;
    let pool = SqlitePoolOptions::new().connect(&url).await?;

    // Apply every pending migration in order, exactly once. Safe to call on every boot.
    MIGRATOR.run(&pool).await?;
    println!("migrations are up to date");

    sqlx::query("INSERT OR IGNORE INTO users (name, email) VALUES (?, ?)")
        .bind("Ada")
        .bind("ada@example.com")
        .execute(&pool)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await?;
    println!("users table has {count} row(s)");
    Ok(())
}
```

Running it on a fresh database applies the migrations; running it again is a no-op:

```text
migrations are up to date
users table has 1 row(s)
```

---

## Detailed Explanation

**`sqlx migrate add -r <name>`** creates the pair of files. The `-r` (`--reversible`) flag is what gives you a `down.sql`; without it you get a single `<timestamp>_<name>.sql` with no rollback. The filename prefix is a version: by default a UTC timestamp, but SQLx switches to sequential numbering (`0001`, `0002`, …) if it detects you started that way. Within a directory you must commit to one style: mixing reversible and non-reversible migrations is rejected.

**The bookkeeping table.** The first time you run migrations, SQLx creates `_sqlx_migrations` and records each applied version, its description, success flag, execution time, and a **checksum** of the SQL. After `sqlx migrate run` it contains:

```text
20260601115557|create users|1
20260601120000|add posts|1
```

That checksum is load-bearing: it is how the runner knows a migration was already applied and detects if you edited an already-applied file (see Pitfalls).

**`sqlx::migrate!()`** is a procedural macro. At **compile time** it walks the `migrations/` directory (next to `Cargo.toml`), reads every `.sql` file, and bakes their contents into a `static Migrator`. This is the opposite of Knex reading files at runtime: once compiled, your binary is self-contained. Unlike the `query!` macro, `migrate!()` does **not** need a live database at compile time — it only reads files — so it works without `DATABASE_URL` set during the build.

**`MIGRATOR.run(&pool)`** opens a transaction per migration, applies each pending one in version order, records it in `_sqlx_migrations`, and skips anything already recorded. Because it consults that table, calling it on every server start is the idiomatic pattern: the first boot migrates, every subsequent boot is a fast no-op.

> **Tip:** The `migrate!` macro reads files at build time, but Cargo does not automatically rebuild when only an `.sql` file changes. Run `sqlx migrate build-script` once to generate a tiny `build.rs` that fixes this:
>
> ```rust
> // build.rs — generated by `sqlx migrate build-script`
> fn main() {
>     // trigger recompilation when a new migration is added
>     println!("cargo:rerun-if-changed=migrations");
> }
> ```

### Diesel migrations

Diesel's model is similar but its CLI scaffolds a **directory per migration** containing `up.sql` and `down.sql`, and its runner lives in the separate `diesel_migrations` crate.

```bash
# The Diesel CLI, built for SQLite (or postgres / mysql).
cargo install diesel_cli --no-default-features --features sqlite

export DATABASE_URL="app.db"          # for SQLite this is just a file path
diesel setup                          # create the db + the migrations/ dir + diesel.toml
diesel migration generate create_users   # makes migrations/<timestamp>_create_users/{up,down}.sql
```

You fill in the two SQL files (Diesel always generates both — every migration is reversible):

```sql
-- migrations/2026-06-01-000001_create_users/up.sql
CREATE TABLE users (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE
);
```

```sql
-- migrations/2026-06-01-000001_create_users/down.sql
DROP TABLE users;
```

```bash
diesel migration run      # apply all pending; also regenerates src/schema.rs
diesel migration revert   # run the latest down.sql
diesel migration redo     # revert + re-apply the latest (verifies down.sql is correct)
```

And the embed-and-run-at-startup version with `diesel_migrations`:

```toml
# Cargo.toml
[dependencies]
diesel = { version = "2", features = ["sqlite"] }
diesel_migrations = "2"
```

```rust
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

// Reads ./migrations at COMPILE time and embeds every up/down pair into the binary.
const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut conn = SqliteConnection::establish(":memory:")?;

    // Apply all pending migrations at startup. Returns the versions it just ran.
    let applied = conn.run_pending_migrations(MIGRATIONS)?;
    println!("applied {} migration(s)", applied.len());
    for v in &applied {
        println!("  - {v}");
    }
    Ok(())
}
```

Real output against a fresh in-memory database:

```text
applied 1 migration(s)
  - 20260601000001
```

The `run_pending_migrations`, `revert_last_migration`, and `has_pending_migration` methods come from the **`MigrationHarness`** trait. You must bring it into scope with `use diesel_migrations::MigrationHarness;` or the methods will not resolve. Note its error type is `Box<dyn std::error::Error + Send + Sync>`, which is why `main` returns exactly that (using a plain `Box<dyn Error>` fails to compile because the boxed error is not `Sized`). The returned versions are the migration timestamps with the separators stripped.

---

## Key Differences

| Concept | Knex / TypeORM / Prisma | SQLx | Diesel |
| --- | --- | --- | --- |
| Up/down content | TypeScript / schema builder | Raw SQL files | Raw SQL files |
| File layout | one file per migration | `<ver>.up.sql` + `<ver>.down.sql` | `<ver>_name/{up,down}.sql` |
| Scaffold command | `knex migrate:make` | `sqlx migrate add -r` | `diesel migration generate` |
| Apply | `knex migrate:latest` | `sqlx migrate run` | `diesel migration run` |
| Roll back | `knex migrate:rollback` | `sqlx migrate revert` | `diesel migration revert` |
| Bookkeeping table | `knex_migrations` | `_sqlx_migrations` | `__diesel_schema_migrations` |
| Files at runtime | read from disk | **embedded in binary** (`migrate!`) | **embedded in binary** (`embed_migrations!`) |
| Generated schema artifact | none | none | regenerates `src/schema.rs` |
| Needs live DB to build | no | no (`migrate!` only reads files) | no |

The headline conceptual difference: **embedding**. With `sqlx::migrate!()` / `embed_migrations!()`, the migration SQL becomes part of the compiled artifact. You deploy a single binary; there is no "did the `migrations/` folder get copied to the server?" failure mode. That is impossible in the Node tools, which always read migration files from disk at runtime.

A second difference is who writes the SQL. Knex and Prisma generate DDL from a builder/schema; Diesel and SQLx have you write the DDL yourself. You trade some convenience for total control over indexes, constraints, and database-specific features, and the SQL is the same SQL you would run by hand.

> **Note:** Diesel uniquely **regenerates `src/schema.rs`** when you run a migration, keeping its compile-time `table!` definitions in sync with the database. SQLx has no such file because it checks queries against a live database (or a cached `.sqlx/` for offline mode) rather than a Rust-side schema. See [Diesel intro](/17-database/03-diesel-intro/) for the `schema.rs` story.

---

## Common Pitfalls

### Forgetting the `macros` feature for `sqlx::migrate!`

`migrate!` is a macro provided by `sqlx-macros`. With only `migrate` enabled (and not `macros`), the call does not resolve. The real compiler error is:

```text
error[E0433]: failed to resolve: could not find `migrate` in `sqlx`
 --> src/main.rs:4:50
  |
4 | static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();
  |                                                  ^^^^^^^ could not find `migrate` in `sqlx`
```

The fix is to enable both: `features = ["runtime-tokio", "sqlite", "migrate", "macros"]`.

### Editing a migration that has already been applied

This is the single most common migration mistake, in any language. Once a migration is recorded in the bookkeeping table, its checksum is fixed. Change the `.sql` file afterward and the next run refuses to proceed. The real error from `MIGRATOR.run` (and identically from `sqlx migrate run`) is:

```text
Error: migration 20260601115557 was previously applied but has been modified
```

The fix is never to edit an applied migration: add a **new** migration that alters the schema forward. (During local development before sharing, you may instead `sqlx migrate revert`, edit, and re-run, but never on a shared or production database.)

### Installing the CLI without your database driver

`cargo install sqlx-cli` defaults to all native-TLS drivers; if you build it `--no-default-features` you must list the drivers you need. A CLI built without the `sqlite` feature fails the moment it touches a `sqlite://` URL:

```text
error: error with configuration: no driver found for URL scheme "sqlite"
```

Reinstall with the right features: `cargo install sqlx-cli --no-default-features --features sqlite,rustls` (add `postgres` and/or `mysql` as needed).

### Forgetting to import Diesel's `MigrationHarness`

`run_pending_migrations` and friends are trait methods. Without `use diesel_migrations::MigrationHarness;` the compiler reports the method does not exist on the connection: the classic Rust "trait not in scope" symptom familiar from the `Read`/`Write` traits. Bring the trait into scope.

### Returning the wrong error type from `main` with Diesel

Diesel's migration methods return `Box<dyn Error + Send + Sync>`. A `fn main() -> Result<(), Box<dyn std::error::Error>>` will not compile against `?` here, because `Box<dyn Error + Send + Sync>` does not coerce into the non-`Send` box through `From` cleanly (the error is `the trait Sized is not implemented for dyn std::error::Error + Send + Sync`). Match the type: `Result<(), Box<dyn std::error::Error + Send + Sync>>`.

---

## Best Practices

- **Run migrations at startup for app-managed schemas.** Calling `MIGRATOR.run(&pool).await?` (SQLx) or `conn.run_pending_migrations(MIGRATIONS)?` (Diesel) early in `main` makes every deploy self-healing: the first instance migrates, the rest see nothing pending. For multi-instance deploys, guard against concurrent runners (a Postgres advisory lock, or a one-off migration job in your pipeline) so two pods do not race on the same migration.
- **Embed, do not ship loose files.** Prefer `sqlx::migrate!()` / `embed_migrations!()` over reading a runtime directory, so the binary is self-contained. Add the `build.rs` `rerun-if-changed=migrations` line so adding a migration triggers a rebuild.
- **Never edit an applied migration.** Roll forward with a new migration. Treat applied migrations as immutable history.
- **Always write a real `down.sql`.** Use `diesel migration redo` (or `sqlx migrate revert` then `run`) locally to prove the rollback actually restores the previous schema before you commit.
- **One logical change per migration**, with a descriptive name (`add_email_index`, not `update2`). Small migrations are easier to review and to revert.
- **Keep DDL idempotent where the database allows it** (`CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`) so a partially-applied migration is recoverable.
- **Commit the migration files (and SQLx's `.sqlx/` offline cache, if you use it) to version control.** Build them into CI so your compile-time-checked queries match the migrated schema. See [SQLx intro](/17-database/00-sqlx-intro/) for offline mode.

---

## Real-World Example

A production web service that connects a pooled database, runs embedded migrations once at startup, then serves requests. This is the shape most Rust API servers use; the migration step is the first thing `main` does after building the pool. The example below is compile-verified end to end (it applies two real migrations and reads back a row).

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.9", default-features = false, features = ["runtime-tokio", "sqlite", "migrate", "macros"] }
tokio = { version = "1", features = ["full"] }
anyhow = "1"
```

```rust
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

// Embedded at compile time from ./migrations (e.g. create_users + add_posts).
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

/// Build the pool and bring the schema up to date before serving traffic.
async fn init_db(url: &str) -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await?;

    // First boot migrates; every later boot finds nothing pending and returns fast.
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("DATABASE_URL")?;
    let pool = init_db(&url).await?;
    println!("schema ready");

    // Pretend this is a request handler using the now-migrated schema.
    sqlx::query("INSERT OR IGNORE INTO users (name, email) VALUES (?, ?)")
        .bind("Grace")
        .bind("grace@example.com")
        .execute(&pool)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await?;
    println!("users table has {count} row(s)");
    Ok(())
}
```

Output on a database that starts empty:

```text
schema ready
users table has 1 row(s)
```

> **Tip:** Wire this into a real HTTP server by handing the `pool` to your router's shared state. See [Connection Pooling](/17-database/08-connection-pooling/) for sizing the pool and [Section 16: Web APIs](/16-web-apis/) for serving requests on top of it.

---

## Further Reading

- [SQLx CLI README (`sqlx-cli`)](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md): `migrate add/run/revert/info`, reversible vs. simple migrations, and the offline cache.
- [`sqlx::migrate!` macro docs](https://docs.rs/sqlx/latest/sqlx/macro.migrate.html): what it embeds and how `Migrator::run` works.
- [Diesel "Getting Started" guide](https://diesel.rs/guides/getting-started) — `diesel setup`, `migration generate/run/redo`, and `schema.rs` regeneration.
- [`diesel_migrations` docs](https://docs.rs/diesel_migrations): `embed_migrations!`, `EmbeddedMigrations`, and the `MigrationHarness` trait.
- Sibling topics in this section:
  - [SQLx intro](/17-database/00-sqlx-intro/): connecting, feature flags, and offline mode (the `.sqlx/` cache).
  - [SQLx queries](/17-database/01-sqlx-queries/) — compile-time-checked queries against your migrated schema.
  - [SQLx transactions](/17-database/02-sqlx-transactions/): how each migration runs inside a transaction.
  - [Diesel intro](/17-database/03-diesel-intro/) — the `table!`/`schema.rs` story migrations keep in sync.
  - [Diesel queries](/17-database/04-diesel-queries/) and [Diesel relations](/17-database/05-diesel-relations/): building on the migrated tables.
  - [Connection pooling](/17-database/08-connection-pooling/) — the pool you run migrations against at startup.
  - [ORM comparison](/17-database/10-orm-comparison/): SQLx vs. Diesel vs. SeaORM, including their migration stories.
- Background from earlier sections:
  - [Section 00: Introduction](/00-introduction/) and [Section 01: Getting Started](/01-getting-started/): `cargo`, `cargo install`, and the toolchain.
  - [Section 02: Basics](/02-basics/) — types like `i64` used when reading rows back.
  - [Section 08: Error Handling](/08-error-handling/): `Result`, `?`, and boxed error types behind these runners.
  - [Section 11: Async](/11-async/) — why the SQLx runner is `async` and Diesel's is synchronous.
- Next section: [Section 18: CLI Tools](/18-cli-tools/) — `sqlx` and `diesel` are themselves Rust CLIs; the same patterns power your own migration tooling.

---

## Exercises

### Exercise 1: Add a reversible migration with the CLI

**Difficulty:** Beginner

**Objective:** Scaffold, fill in, apply, and revert a migration using `sqlx migrate`.

**Instructions:** In an empty directory, set `DATABASE_URL=sqlite://app.db`, run `sqlx database create`, then `sqlx migrate add -r create_products`. Edit the generated up/down files so the `up` creates a `products (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, price_cents INTEGER NOT NULL)` table and the `down` drops it. Apply with `sqlx migrate run`, confirm with `sqlx migrate info`, then `sqlx migrate revert` and confirm the table is gone.

<details>
<summary>Solution</summary>

```bash
export DATABASE_URL="sqlite://app.db"
sqlx database create
sqlx migrate add -r create_products
```

```sql
-- migrations/<timestamp>_create_products.up.sql
CREATE TABLE products (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    price_cents INTEGER NOT NULL
);
```

```sql
-- migrations/<timestamp>_create_products.down.sql
DROP TABLE products;
```

```bash
sqlx migrate run      # Applied <timestamp>/migrate create products (...)
sqlx migrate info     # <timestamp>/installed create products
sqlx migrate revert   # Applied <timestamp>/revert create products (...)
sqlx migrate info     # <timestamp>/pending create products
```

After `revert`, the `products` table is dropped and the migration shows as `pending` again: proof the `down.sql` ran.

</details>

### Exercise 2: Run embedded migrations at startup

**Difficulty:** Intermediate

**Objective:** Embed migrations with `sqlx::migrate!` and apply them on boot, then prove re-running is a no-op.

**Instructions:** Using the `migrations/` directory from Exercise 1, add `sqlx` with the `migrate` and `macros` features. Write a `#[tokio::main]` program that builds a `SqlitePool`, calls `MIGRATOR.run(&pool)`, and prints how many products exist. Run it twice and confirm the second run does not error (migrations already applied).

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.9", default-features = false, features = ["runtime-tokio", "sqlite", "migrate", "macros"] }
tokio = { version = "1", features = ["full"] }
anyhow = "1"
```

```rust
use sqlx::sqlite::SqlitePoolOptions;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = SqlitePoolOptions::new()
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;

    MIGRATOR.run(&pool).await?; // first run applies; later runs are no-ops
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
        .fetch_one(&pool)
        .await?;
    println!("products in catalog: {count}");
    Ok(())
}
```

Run `DATABASE_URL=sqlite://app.db cargo run` twice. Both runs print `products in catalog: 0` (or whatever you inserted) with no migration error on the second run, because `_sqlx_migrations` already records the applied version.

> **Tip:** Generate `build.rs` with `sqlx migrate build-script` so adding a future migration forces a rebuild that re-reads the directory.

</details>

### Exercise 3: Embedded migrations and rollback with Diesel

**Difficulty:** Advanced

**Objective:** Use `diesel_migrations::embed_migrations!`, apply at startup, then revert the latest migration programmatically.

**Instructions:** Create a directory `migrations/2026-06-01-000001_create_widgets/` with `up.sql` creating a `widgets (id INTEGER PRIMARY KEY AUTOINCREMENT, label TEXT NOT NULL)` table and `down.sql` dropping it. Add `diesel` (feature `sqlite`) and `diesel_migrations`. Write a synchronous `main` that opens an in-memory SQLite connection, runs all pending migrations, prints how many were applied, then calls `revert_last_migration` and prints the version it reverted. Remember the `MigrationHarness` import and the `Send + Sync` error type.

<details>
<summary>Solution</summary>

```sql
-- migrations/2026-06-01-000001_create_widgets/up.sql
CREATE TABLE widgets (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    label TEXT NOT NULL
);
```

```sql
-- migrations/2026-06-01-000001_create_widgets/down.sql
DROP TABLE widgets;
```

```toml
# Cargo.toml
[dependencies]
diesel = { version = "2", features = ["sqlite"] }
diesel_migrations = "2"
```

```rust
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut conn = SqliteConnection::establish(":memory:")?;

    let applied = conn.run_pending_migrations(MIGRATIONS)?;
    println!("applied {} migration(s)", applied.len());

    // Roll back the most recent migration by running its down.sql.
    let reverted = conn.revert_last_migration(MIGRATIONS)?;
    println!("reverted: {reverted}");

    // After reverting, the migration is pending again.
    let pending = conn.has_pending_migration(MIGRATIONS)?;
    println!("has pending after revert: {pending}");
    Ok(())
}
```

Real output (the version is the timestamp with separators removed):

```text
applied 1 migration(s)
reverted: 20260601000001
has pending after revert: true
```

</details>
