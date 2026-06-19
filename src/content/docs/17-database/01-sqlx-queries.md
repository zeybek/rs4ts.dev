---
title: "Writing Queries with SQLx"
description: "Write SQL with SQLx in Rust: query! and query_as! verify columns and types against your schema at compile time, and bound params kill injection."
---

Learn how to run SQL with SQLx: the `query!` and `query_as!` macros that check your SQL against a real database **at compile time**, how parameter binding stops SQL injection cold, and how the `FromRow` trait maps result rows into your Rust structs.

---

## Quick Overview

In TypeScript you usually reach for an ORM (Prisma, TypeORM) or a query builder (Knex) that wraps SQL in method chains. **SQLx** takes a different stance: you write **real SQL strings**, but the `query!`/`query_as!` macros connect to your database during `cargo build` and verify that the SQL parses, the columns exist, and the types line up. A typo in a column name becomes a *compile error*, not a 2 a.m. production page. This file covers writing those queries, binding parameters safely, and decoding rows with `FromRow`.

> **Prerequisites:** This builds on [SQLx setup and connecting](/17-database/00-sqlx-intro/). Make sure you have a `Pool` and a `DATABASE_URL` first.

---

## TypeScript/JavaScript Example

Here is a typical data-access layer using the `pg` driver (node-postgres) with parameterized queries, the responsible, injection-safe way to write raw SQL in Node:

```typescript
import { Pool } from "pg";

const pool = new Pool({ connectionString: process.env.DATABASE_URL });

interface User {
  id: number;
  name: string;
  email: string;
  active: boolean;
}

// Parameterized query: values go through $1, $2 — never string concatenation.
async function findByEmail(email: string): Promise<User | null> {
  const result = await pool.query<User>(
    "SELECT id, name, email, active FROM users WHERE email = $1",
    [email],
  );
  return result.rows[0] ?? null;
}

async function createUser(name: string, email: string): Promise<User> {
  const result = await pool.query<User>(
    "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id, name, email, active",
    [name, email],
  );
  return result.rows[0];
}

// DANGER: never do this. String interpolation = SQL injection.
async function unsafeSearch(name: string) {
  // If name is `'; DROP TABLE users; --` you just lost your table.
  return pool.query(`SELECT * FROM users WHERE name = '${name}'`);
}
```

Two things to notice. First, `pool.query<User>(...)` accepts a type parameter, but TypeScript **does not check it**. `<User>` is an unverified promise. If the SQL returns different columns, or the table doesn't have a `name` column, TypeScript stays silent and you get `undefined` at runtime. Second, the unsafe function compiles and runs perfectly; nothing stops you from shipping it.

---

## Rust Equivalent

SQLx closes both gaps. Bound parameters are the *only* way to pass values, and the macro variants verify the SQL against your real schema at build time.

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, FromRow)]
struct User {
    id: i64,
    name: String,
    email: String,
    active: bool,
}

async fn setup(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE,
            active BOOLEAN NOT NULL DEFAULT 1
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    setup(&pool).await?;

    // INSERT with bound params; `?` is the SQLite placeholder.
    let result = sqlx::query("INSERT INTO users (name, email) VALUES (?, ?)")
        .bind("Alice")
        .bind("alice@example.com")
        .execute(&pool)
        .await?;
    println!("rows affected = {}", result.rows_affected());
    println!("new id        = {}", result.last_insert_rowid());

    sqlx::query("INSERT INTO users (name, email) VALUES (?, ?)")
        .bind("Bob")
        .bind("bob@example.com")
        .execute(&pool)
        .await?;

    // query_as maps each row into a User via FromRow.
    let users: Vec<User> =
        sqlx::query_as::<_, User>("SELECT id, name, email, active FROM users ORDER BY id")
            .fetch_all(&pool)
            .await?;
    for u in &users {
        println!("{:?}", u);
    }

    // query_scalar pulls a single column.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await?;
    println!("count = {count}");

    Ok(())
}
```

This is real, compiled-and-run output:

```text
rows affected = 1
new id        = 1
User { id: 1, name: "Alice", email: "alice@example.com", active: true }
User { id: 2, name: "Bob", email: "bob@example.com", active: true }
count = 2
```

> **Note:** The examples here use SQLite (`sqlite::memory:`) so they run with zero external setup. For Postgres the placeholder is `$1, $2, ...` instead of `?`, and you use `result.rows_affected()` plus a `RETURNING` clause instead of `last_insert_rowid()`. Everything else is identical. Add the dependency with `cargo add sqlx --features runtime-tokio,sqlite,macros --no-default-features` (swap `sqlite` for `postgres` as needed) plus `cargo add tokio --features full`. With the current stable toolchain (Rust 1.96.0 on the 2024 edition), `cargo new` already selects the newest edition and `cargo add` is built in.

---

## Detailed Explanation

### Two families of query functions

SQLx gives you **functions** and **macros** that do the same jobs:

| Goal | Runtime function | Compile-checked macro |
| --- | --- | --- |
| Run a statement, ignore rows | `sqlx::query(...)` | `sqlx::query!(...)` |
| Get rows as a named struct | `sqlx::query_as::<_, T>(...)` | `sqlx::query_as!(T, ...)` |
| Get one column | `sqlx::query_scalar(...)` | `sqlx::query_scalar!(...)` |

The **function** form takes the SQL as an ordinary `&str`, so it is never checked against the database. The **macro** form (`query!`, note the `!`) takes the SQL as a *string literal* and, during compilation, sends it to your database to validate it. We will use both; each has its place.

### `query()` + `execute()` for writes

```rust
let result = sqlx::query("INSERT INTO users (name, email) VALUES (?, ?)")
    .bind("Alice")
    .bind("alice@example.com")
    .execute(&pool)
    .await?;
```

- `sqlx::query(sql)` builds a query. Each `?` (SQLite) or `$N` (Postgres) is a **placeholder**.
- `.bind(value)` supplies the value for the next placeholder, in order. Bind once per placeholder.
- `.execute(&pool)` runs the statement and returns a driver result. `.rows_affected()` works everywhere; `.last_insert_rowid()` is SQLite-specific.
- `.await?`, like a JavaScript `await`, but Rust futures are **lazy**: nothing touches the database until you `.await`. (See [Section 11 — Async](/11-async/).)

### `query_as()` + `FromRow` for reads

```rust
let users: Vec<User> =
    sqlx::query_as::<_, User>("SELECT id, name, email, active FROM users ORDER BY id")
        .fetch_all(&pool)
        .await?;
```

`query_as::<_, User>` says "decode each row into a `User`". For that to work, `User` must implement the `FromRow` trait. `#[derive(FromRow)]` generates the implementation: for each struct field it pulls the column of the same name out of the row and converts it to the field's type. The first generic argument is the database row type, which Rust infers, so we write `_`.

### Choosing how many rows you want

The fetch method encodes your cardinality expectation directly in the type:

| Method | Returns | Use when |
| --- | --- | --- |
| `.fetch_one(&pool)` | `T` | Exactly one row expected; error if zero |
| `.fetch_optional(&pool)` | `Option<T>` | Zero or one row (lookups by key) |
| `.fetch_all(&pool)` | `Vec<T>` | A list of rows |
| `.fetch(&pool)` | a `Stream` of `T` | Huge result sets, processed lazily |

`fetch_optional` is the idiomatic equivalent of the TypeScript `result.rows[0] ?? null`: a missing row is `None`, not an error. This is far cleaner than the JS pattern, because the *type* tells callers a row may be absent and the compiler forces them to handle it.

### `query!` and `query_as!`: compile-time-checked SQL

```rust
let row = sqlx::query!("SELECT id, name, email FROM users WHERE id = ?", 1)
    .fetch_one(&pool)
    .await?;
println!("id={} name={} email={}", row.id, row.name, row.email);

let users = sqlx::query_as!(UserRow, "SELECT id, name, email FROM users ORDER BY id")
    .fetch_all(&pool)
    .await?;
```

This is SQLx's headline feature. At `cargo build` time, the macro connects to the database at `DATABASE_URL`, prepares the statement, and reads back the column names and types. It then:

1. Verifies the SQL parses and every column/table exists.
2. Checks that each bound argument's Rust type matches the expected SQL type (`1` here must be a valid `id`).
3. For `query!`, generates an **anonymous struct** whose fields (`row.id`, `row.name`, ...) have the exact Rust types the database reported: fully inferred, no struct to write.
4. For `query_as!`, decodes into the named struct you pass (`UserRow`), still fully checked.

Bound arguments to the macros are passed **inline after the SQL**, comma-separated — not via `.bind()`. The macro counts them and matches them to placeholders.

This is genuinely different from anything in the TypeScript world. Prisma generates types from a schema file, but it cannot tell you that a *hand-written* SQL string has a typo. SQLx talks to the live database and validates the actual query you wrote.

---

## Key Differences

| Aspect | TypeScript (`pg` / Prisma) | Rust (SQLx) |
| --- | --- | --- |
| SQL verification | None for raw SQL; Prisma checks its own DSL only | `query!`/`query_as!` check SQL against the live DB at compile time |
| Result typing | `query<User>()` cast is unchecked | `FromRow` decoding is type-checked; macros infer types from columns |
| Parameters | `$1`, `?` arrays (driver-dependent) | `.bind()` or inline macro args; never string interpolation |
| Missing row | `rows[0]` is `undefined` | `fetch_optional` returns `Option<T>`, `fetch_one` errors |
| Nullable column | `string \| null` if you remember | maps to `Option<String>`; macro infers nullability |
| Injection safety | Possible to misuse (template strings) | Values cannot be concatenated into the prepared statement |

### Why bound parameters prevent SQL injection

A **bound parameter** is sent to the database *separately* from the SQL text. The driver first sends `SELECT * FROM users WHERE name = ?` to be parsed and planned, then sends the value `'; DROP TABLE users; --` as pure data. The database never re-parses that value as SQL, so it can only ever be a string compared against the `name` column. There is no "escaping" to get wrong: the structure of the query is fixed before the value is ever seen.

In SQLx, `.bind(value)` and the inline macro arguments are the *only* ways to get a value into a query. To inject SQL you would have to deliberately build the string yourself with `format!`, which the next section shows is both ugly and obviously wrong.

### `FromRow` field mapping and renaming

By default `#[derive(FromRow)]` matches struct fields to columns by name. You can override the column name and let nullable columns become `Option`:

```rust
use sqlx::FromRow;

#[derive(Debug, FromRow)]
struct UserProfile {
    id: i64,
    #[sqlx(rename = "name")] // struct field is full_name, column is name
    full_name: String,
    bio: Option<String>, // nullable column maps to Option
}
```

Running `SELECT id, name, bio FROM users` into this struct (with one row having a NULL `bio`) produces this real output:

```text
UserProfile { id: 1, full_name: "Alice", bio: Some("Engineer") }
UserProfile { id: 2, full_name: "Bob", bio: None }
```

A NULL column becomes `None`, a present value becomes `Some(...)`. If you map a nullable column to a non-`Option` field and a row is NULL, you get a decode error. The type system makes you acknowledge nullability, unlike the silent `undefined` you get in JavaScript.

---

## Common Pitfalls

### Pitfall 1: Building SQL with `format!` (the injection trap)

The single worst mistake a TypeScript developer brings over is interpolating values into the SQL string, exactly like a JS template literal:

```rust
// NEVER DO THIS — SQL injection, and it defeats every SQLx guarantee.
let name = user_input; // could be: '; DROP TABLE users; --
let sql = format!("SELECT * FROM users WHERE name = '{name}'");
let rows = sqlx::query(&sql).fetch_all(&pool).await?;
```

This compiles and runs, just like the JavaScript version, and it is just as dangerous. The fix is always a placeholder plus `.bind()`:

```rust
// Correct: the value is bound, never concatenated.
let rows = sqlx::query("SELECT * FROM users WHERE name = ?")
    .bind(name)
    .fetch_all(&pool)
    .await?;
```

> **Warning:** If you ever find yourself reaching for `format!` to assemble a query, stop. Values go through `.bind()` / macro arguments. The only legitimate use of dynamic SQL text is the **identifiers** (table/column names) you control yourself (never user data), and even then prefer a fixed `match` over interpolation.

### Pitfall 2: A typo in `query!` is a compile error (this is good)

This is the feature, experienced as a "pitfall" by newcomers. Misspell a column and the build fails:

```rust
// does not compile — column is `name`, not `username`.
let row = sqlx::query!("SELECT id, username FROM users WHERE id = ?", 1)
    .fetch_one(&pool)
    .await?;
```

The real `cargo check` output:

```text
error: error returned from database: (code: 1) no such column: username
 --> src/main.rs:7:15
  |
7 |     let row = sqlx::query!("SELECT id, username FROM users WHERE id = ?", 1)
  |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The same typo in the runtime `sqlx::query(...)` form would compile fine and blow up at runtime instead. Prefer the macros when you can.

### Pitfall 3: Wrong number of bound arguments

The macros count your placeholders. Supply too few:

```rust
// does not compile — two `?`, one argument.
let row = sqlx::query!("SELECT id FROM users WHERE name = ? AND email = ?", "Alice")
    .fetch_one(&pool)
    .await?;
```

Real `cargo check` output:

```text
error: expected 2 parameters, got 1
 --> src/main.rs:7:15
  |
7 |     let row = sqlx::query!("SELECT id FROM users WHERE name = ? AND email = ?", "Alice")
  |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

With the runtime `.bind()` form there is no compile-time count check — a mismatch surfaces as a runtime error from the database driver instead.

### Pitfall 4: Mismatched column type vs struct field type

Map a column to the wrong Rust type and the behavior depends on which API you used. With the **runtime** `query_as` form, you get a runtime decode error:

```rust
#[derive(Debug, sqlx::FromRow)]
struct BadUser {
    id: String, // the column is INTEGER, not TEXT
    name: String,
}
// ...query_as::<_, BadUser>("SELECT id, name FROM users")...
```

Real runtime error:

```text
Error: ColumnDecode { index: "\"id\"", source: "mismatched types; Rust type
`alloc::string::String` (as SQL type `TEXT`) is not compatible with SQL type `INTEGER`" }
```

The **macro** form catches the very same mistake at compile time instead:

```rust
// does not compile — query_as! knows id is i64.
let users = sqlx::query_as!(BadUser, "SELECT id, name FROM users")
    .fetch_all(&pool)
    .await?;
```

Real `cargo check` output:

```text
error[E0277]: the trait bound `String: From<i64>` is not satisfied
  --> src/main.rs:12:17
   |
12 |     let users = sqlx::query_as!(BadUser, "SELECT id, name FROM users")
   |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the trait `From<i64>` is not implemented for `String`
```

Catching it at build time instead of in production is exactly why the macros exist.

### Pitfall 5: Forgetting the database has to exist at build time

Because `query!`/`query_as!` connect during compilation, they need a `DATABASE_URL` (usually set in a `.env` file) pointing at a database that already has your schema. If it is missing, the build fails with a connection error. For CI or builds without a live database, run `cargo sqlx prepare` once to cache the query metadata into a `.sqlx/` directory, commit it, and set `SQLX_OFFLINE=true`. (Setup and the `sqlx-cli` tool are covered in [SQLx setup](/17-database/00-sqlx-intro/) and [migrations](/17-database/09-migrations/).)

---

## Best Practices

- **Prefer the macros (`query!`, `query_as!`) over the functions** whenever the SQL is static. Compile-time verification is the entire reason to choose SQLx over a plain driver.
- **Use the function form (`query`, `query_as`) for dynamic SQL** (pagination clauses, optional filters built at runtime) where a string literal will not do. You trade compile-time checking for flexibility, so test those paths.
- **Match the fetch method to your cardinality:** `fetch_optional` for "by id" lookups, `fetch_one` when a row must exist, `fetch_all` for lists, `fetch` (a `Stream`) for large result sets you do not want to buffer in memory.
- **Always pass values through `.bind()` or macro arguments.** Treat `format!` near a SQL string as a code-review red flag.
- **Map nullable columns to `Option<T>`** and let the type system carry the nullability, rather than hoping a value is present.
- **Use `RETURNING` to avoid a second round trip** when you need the inserted/updated row back (Postgres and modern SQLite support it).
- **Put data access behind a repository struct** that owns the `Pool`, so SQL lives in one place and call sites stay clean (see the next section).
- **Select explicit columns, not `SELECT *`**, so the macro's inferred field set is stable and obvious.

---

## Real-World Example

A production-flavored repository: it owns the pool, exposes typed methods, uses `fetch_optional` for lookups, and `RETURNING` to get the created row back in a single query. This compiles and runs.

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, FromRow)]
struct User {
    id: i64,
    name: String,
    email: String,
}

struct UserRepository {
    pool: SqlitePool,
}

impl UserRepository {
    fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // Returns Option: None when no row matches (fetch_optional).
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>("SELECT id, name, email FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await
    }

    // INSERT ... RETURNING gives us the row back in one round trip.
    async fn create(&self, name: &str, email: &str) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "INSERT INTO users (name, email) VALUES (?, ?) RETURNING id, name, email",
        )
        .bind(name)
        .bind(email)
        .fetch_one(&self.pool)
        .await
    }

    async fn rename(&self, id: i64, new_name: &str) -> Result<u64, sqlx::Error> {
        let res = sqlx::query("UPDATE users SET name = ? WHERE id = ?")
            .bind(new_name)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    async fn delete(&self, id: i64) -> Result<u64, sqlx::Error> {
        let res = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }
}

async fn setup(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    setup(&pool).await?;
    let repo = UserRepository::new(pool);

    let created = repo.create("Carol", "carol@example.com").await?;
    println!("created: {:?}", created);

    match repo.find_by_email("carol@example.com").await? {
        Some(u) => println!("found:   {:?}", u),
        None => println!("not found"),
    }
    match repo.find_by_email("nobody@example.com").await? {
        Some(u) => println!("found:   {:?}", u),
        None => println!("found:   None (no such user)"),
    }

    let updated = repo.rename(created.id, "Caroline").await?;
    println!("rows updated: {updated}");

    let deleted = repo.delete(created.id).await?;
    println!("rows deleted: {deleted}");

    Ok(())
}
```

Real output:

```text
created: User { id: 1, name: "Carol", email: "carol@example.com" }
found:   User { id: 1, name: "Carol", email: "carol@example.com" }
found:   None (no such user)
rows updated: 1
rows deleted: 1
```

> **Tip:** When you want each method's SQL verified at compile time, swap `query_as::<_, User>(...)` for `query_as!(User, ...)` and move the bound values inline after the SQL. The repository shape stays the same. For multi-statement operations that must succeed or fail together (e.g. create-user-then-create-profile), wrap them in a transaction — see [SQLx transactions](/17-database/02-sqlx-transactions/).

---

## Further Reading

- [SQLx — query / query_as docs (docs.rs)](https://docs.rs/sqlx/latest/sqlx/macro.query.html)
- [SQLx `FromRow` derive (docs.rs)](https://docs.rs/sqlx/latest/sqlx/derive.FromRow.html)
- [SQLx README — compile-time verification](https://github.com/launchbadge/sqlx#compile-time-verification)
- [OWASP — SQL Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/SQL_Injection_Prevention_Cheat_Sheet.html)

### Related sections in this guide

- [SQLx setup and connecting](/17-database/00-sqlx-intro/): pools, `DATABASE_URL`, Postgres vs SQLite
- [SQLx transactions](/17-database/02-sqlx-transactions/): `begin`/`commit`/`rollback`, atomicity
- [Connection pooling](/17-database/08-connection-pooling/): sizing and lifecycle of `sqlx::Pool`
- [Migrations](/17-database/09-migrations/): `sqlx migrate`, schema setup for the macros
- [Diesel queries](/17-database/04-diesel-queries/) — the ORM/query-builder alternative
- [SQLx vs Diesel vs SeaORM](/17-database/10-orm-comparison/): when to choose which
- [Section 11 — Async](/11-async/): why `.await` is required and futures are lazy
- [Section 08 — Error Handling](/08-error-handling/): `Result`, `?`, and `sqlx::Error`
- [Section 18 — CLI Tools](/18-cli-tools/) — building command-line apps around a database

---

## Exercises

### Exercise 1: Bind, don't interpolate

**Difficulty:** Beginner

**Objective:** Convert an injection-prone query into a safe, parameterized one.

**Instructions:** The function below builds SQL with `format!`. Rewrite it so the `min_age` value is passed through a bound parameter and the query returns the count of matching users. Assume a `people` table with an integer `age` column.

```rust
use sqlx::SqlitePool;

async fn count_older_than(pool: &SqlitePool, min_age: i64) -> Result<i64, sqlx::Error> {
    // Rewrite this to use a placeholder and .bind(...)
    let sql = format!("SELECT COUNT(*) FROM people WHERE age >= {min_age}");
    sqlx::query_scalar(&sql).fetch_one(pool).await
}
```

<details>
<summary>Solution</summary>

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

async fn count_older_than(pool: &SqlitePool, min_age: i64) -> Result<i64, sqlx::Error> {
    // The value is bound, never concatenated into the SQL text.
    sqlx::query_scalar("SELECT COUNT(*) FROM people WHERE age >= ?")
        .bind(min_age)
        .fetch_one(pool)
        .await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query("CREATE TABLE people (id INTEGER PRIMARY KEY, age INTEGER NOT NULL)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO people (age) VALUES (17), (21), (45)")
        .execute(&pool)
        .await?;

    let n = count_older_than(&pool, 18).await?;
    println!("adults = {n}"); // adults = 2
    Ok(())
}
```

Real output: `adults = 2`.

</details>

### Exercise 2: Map rows with `FromRow`

**Difficulty:** Intermediate

**Objective:** Define a struct and use `query_as` to load rows into it, handling a nullable column.

**Instructions:** Given a `products` table with columns `id INTEGER`, `name TEXT NOT NULL`, and a nullable `description TEXT`, define a `Product` struct deriving `FromRow` (where `description` is optional) and write a function that returns all products ordered by `id`.

<details>
<summary>Solution</summary>

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, FromRow)]
struct Product {
    id: i64,
    name: String,
    description: Option<String>, // nullable column -> Option
}

async fn all_products(pool: &SqlitePool) -> Result<Vec<Product>, sqlx::Error> {
    sqlx::query_as::<_, Product>("SELECT id, name, description FROM products ORDER BY id")
        .fetch_all(pool)
        .await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query(
        "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT NOT NULL, description TEXT)",
    )
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO products (name, description) VALUES (?, ?), (?, ?)")
        .bind("Keyboard")
        .bind("Mechanical, tactile")
        .bind("Mouse")
        .bind(Option::<String>::None) // NULL description
        .execute(&pool)
        .await?;

    for p in all_products(&pool).await? {
        println!("{p:?}");
    }
    Ok(())
}
```

Real output:

```text
Product { id: 1, name: "Keyboard", description: Some("Mechanical, tactile") }
Product { id: 2, name: "Mouse", description: None }
```

</details>

### Exercise 3: Insert and fetch with one query

**Difficulty:** Advanced

**Objective:** Use `RETURNING` plus `query_as` to insert a row and get the full row (including its generated id) back in a single round trip, then look it up with `fetch_optional`.

**Instructions:** Define an `Account` struct (`id: i64`, `username: String`, `balance: i64`). Write `create_account(pool, username, balance)` that inserts and returns the new `Account` via `RETURNING`, and `find(pool, id)` that returns `Option<Account>`. Demonstrate that looking up a non-existent id yields `None`.

<details>
<summary>Solution</summary>

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, FromRow)]
struct Account {
    id: i64,
    username: String,
    balance: i64,
}

async fn create_account(
    pool: &SqlitePool,
    username: &str,
    balance: i64,
) -> Result<Account, sqlx::Error> {
    sqlx::query_as::<_, Account>(
        "INSERT INTO accounts (username, balance) VALUES (?, ?)
         RETURNING id, username, balance",
    )
    .bind(username)
    .bind(balance)
    .fetch_one(pool)
    .await
}

async fn find(pool: &SqlitePool, id: i64) -> Result<Option<Account>, sqlx::Error> {
    sqlx::query_as::<_, Account>("SELECT id, username, balance FROM accounts WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query(
        "CREATE TABLE accounts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            balance INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    let acct = create_account(&pool, "dana", 1000).await?;
    println!("created: {acct:?}");

    println!("lookup hit:  {:?}", find(&pool, acct.id).await?);
    println!("lookup miss: {:?}", find(&pool, 999).await?);
    Ok(())
}
```

Real output:

```text
created: Account { id: 1, username: "dana", balance: 1000 }
lookup hit:  Some(Account { id: 1, username: "dana", balance: 1000 })
lookup miss: None
```

</details>
