---
title: "SQL Injection Prevention"
description: "Stop SQL injection in Rust by binding values with SQLx instead of formatting strings, with compile-time query checks Prisma and the pg driver can't match."
---

SQL injection is the oldest trick in the web-security book, and it still tops breach reports decades later. The fix is always the same: **let the database driver bind your values as data, never paste them into the SQL text yourself.** This chapter shows how Rust's SQLx makes the safe path the obvious one, and how to handle the genuinely dynamic cases (sorting, filtering) without opening a hole.

---

## Quick Overview

**SQL injection** happens when user input is concatenated into a SQL string, so an attacker can break out of the value and inject their own SQL. The defense is **parameterized queries** (also called **prepared statements** or **bound parameters**): you send the SQL with placeholders (`?` or `$1`) and the values separately, and the database treats every value as opaque data, never as code.

For a TypeScript developer this should feel familiar: `pg`'s `pool.query(sql, params)` and Prisma's tagged-template `$queryRaw` already parameterize. Rust's [SQLx](https://github.com/launchbadge/sqlx) goes one step further: bound parameters are the *only* ergonomic way to pass a value, and the optional `query!` macro can verify your SQL against a real database **at compile time**. This file focuses narrowly on injection: bind, never format. Validating the *shape* of input before it reaches the database is covered in [Input Validation](/27-security/00-input-validation/).

> **Note:** The examples use SQLx 0.8 with the bundled SQLite driver so they run with zero setup (`sqlite::memory:`). The same `bind`/placeholder API applies to PostgreSQL and MySQL; only the placeholder syntax differs (`$1` for Postgres, `?` for SQLite/MySQL). The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically.

---

## TypeScript/JavaScript Example

Here is a typical Node data-access layer. The first two functions are written the *responsible* way with the `pg` driver; the third is the trap that ships in real codebases.

```typescript
import { Pool } from "pg";

const pool = new Pool({ connectionString: process.env.DATABASE_URL });

interface User {
  id: number;
  name: string;
  email: string;
}

// Parameterized: values go through $1, $2 — pg sends them out-of-band.
async function findByEmail(email: string): Promise<User | null> {
  const result = await pool.query<User>(
    "SELECT id, name, email FROM users WHERE email = $1",
    [email],
  );
  return result.rows[0] ?? null;
}

// String interpolation = SQL injection.
async function unsafeSearch(name: string): Promise<User[]> {
  // If `name` is `' OR '1'='1`, this returns EVERY row.
  // If it's `'; DROP TABLE users; --`, you've lost the table.
  const result = await pool.query<User>(
    `SELECT id, name, email FROM users WHERE name = '${name}'`,
  );
  return result.rows;
}
```

Two things to notice. First, the safe and unsafe versions look almost identical. The only difference is `$1` + a params array versus a template literal. It is *easy* to reach for the template literal, especially when the query "feels" dynamic. Second, `unsafeSearch` compiles and passes a casual code review; nothing in JavaScript or TypeScript flags it. The type parameter `<User>` is unverified: TypeScript never checks that the SQL actually returns those columns.

> **Warning:** Template literals are the single most common source of injection in Node code, because they make concatenation look clean. A backtick around SQL with a `${}` inside is a red flag every time.

---

## Rust Equivalent

SQLx closes both gaps. The value is passed via `.bind(...)`, which the driver sends as a separate prepared-statement parameter; it never becomes part of the SQL text. Add the dependencies first:

```bash
cargo add sqlx --no-default-features --features sqlite,runtime-tokio,macros
cargo add tokio --features full
```

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, FromRow)]
struct User {
    id: i64,
    name: String,
    email: String,
}

async fn setup(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT NOT NULL)",
    )
    .execute(pool)
    .await?;

    for (name, email) in [("Alice", "alice@example.com"), ("Bob", "bob@example.com")] {
        sqlx::query("INSERT INTO users (name, email) VALUES (?, ?)")
            .bind(name)
            .bind(email)
            .execute(pool)
            .await?;
    }
    Ok(())
}

// Safe: the value is BOUND, never spliced into the SQL text.
async fn find_by_name(pool: &SqlitePool, name: &str) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT id, name, email FROM users WHERE name = ?")
        .bind(name)
        .fetch_all(pool)
        .await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    setup(&pool).await?;

    let found = find_by_name(&pool, "Alice").await?;
    println!("lookup 'Alice' -> {} row(s)", found.len());
    for u in &found {
        println!("  {:?}", u);
    }

    // Classic injection payload passed as the *value*. Because it is bound,
    // it is compared as a literal string and matches nothing.
    let payload = "Alice' OR '1'='1";
    let attacked = find_by_name(&pool, payload).await?;
    println!(
        "lookup with payload {:?} -> {} row(s) (no leak)",
        payload,
        attacked.len()
    );

    Ok(())
}
```

Running it produces this real output:

```text
lookup 'Alice' -> 1 row(s)
  User { id: 1, name: "Alice", email: "alice@example.com" }
lookup with payload "Alice' OR '1'='1" -> 0 row(s) (no leak)
```

The injection payload returns **zero rows**, exactly what we want. The driver compared the literal 16-character string `Alice' OR '1'='1` against the `name` column. No row has that name, so nothing matches. The quotes inside the payload are just characters in a value; they never terminated a string literal because there was no string literal in the SQL to terminate.

---

## Detailed Explanation

### How a prepared statement neutralizes the attack

When you write `WHERE name = ?` and call `.bind(name)`, two separate things travel to the database:

1. The **SQL template** — `SELECT ... WHERE name = ?` — which the server parses and plans **once**, with the `?` as a typed slot.
2. The **value** — `Alice' OR '1'='1` — sent afterward as binary protocol data that fills the already-parsed slot.

Because the query is parsed *before* the value arrives, there is no parsing step left for the value to subvert. Contrast that with `format!("WHERE name = '{name}'")`, where the value is part of the text the parser sees, so a stray quote changes the query's structure. This is the entire game: **parse first, fill values after.**

### `query` vs `query_as` vs `query!`

SQLx gives you three layers, all of which bind parameters the same way:

- `sqlx::query("... ?")`: runs SQL, returns generic rows. Bind with `.bind(...)`.
- `sqlx::query_as::<_, User>("... ?")`: same, but decodes each row into a `#[derive(FromRow)]` struct.
- `sqlx::query!("... ?", value)` and `sqlx::query_as!(...)`: **macros** that connect to a real database during `cargo build`, verify the SQL parses, check the columns exist, and confirm the bound-parameter and result types line up. A typo becomes a *compile error*.

The macros take their bind values as extra macro arguments rather than `.bind(...)` calls, but they are still bound parameters — you cannot smuggle an unparameterized value in. This is stricter than anything in the TypeScript world: Prisma's `$queryRaw` tagged template parameterizes, but it does not check your SQL against the live schema at build time.

### Compile-time checking needs a database URL

Because the macros talk to a real database at build time, they need to know where it is:

```rust
#[tokio::main]
async fn main() {
    // query! checks the SQL against a real DB at compile time.
    let _ = sqlx::query!("SELECT id FROM users WHERE id = ?", 1i64);
}
```

Building this without a `DATABASE_URL` set gives the real error:

```text
error: set `DATABASE_URL` to use query macros online, or run `cargo sqlx prepare` to update the query cache
 --> src/main.rs:4:13
  |
4 |     let _ = sqlx::query!("SELECT id FROM users WHERE id = ?", 1i64);
  |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

In CI you either point `DATABASE_URL` at a throwaway database or commit a `.sqlx` query cache produced by `cargo sqlx prepare` so builds work offline. The non-macro `sqlx::query(...)` form needs no database at build time; it just gives up the compile-time checking. Both are equally safe against injection; the macro adds *correctness* checking on top.

---

## Key Differences

| Concern | TypeScript (`pg` / Prisma) | Rust (SQLx) |
| --- | --- | --- |
| Safe binding | `pool.query(sql, [v])`; `$queryRaw\`...\`` | `.bind(v)` or `query!(sql, v)` |
| Placeholder syntax | `$1, $2` (pg) | `?` (SQLite/MySQL), `$1` (Postgres) |
| Easy to inject by accident? | Yes — template literals look clean | Harder — binding is the path of least resistance |
| SQL verified against schema? | No (`$queryRaw` is unchecked) | Optional, at compile time via `query!` |
| Result columns type-checked? | No (`<User>` is unverified) | Yes, with `query!`/`query_as!` |
| Identifiers (table/column) bindable? | No | No (same SQL limitation) |

The conceptual point that surprises TypeScript developers: **Rust does not make injection impossible: SQL is still a string and you can still call `format!`.** What it does is make the safe path (`bind`) the natural, ergonomic one and offer compile-time SQL verification that no TypeScript tool matches. Discipline still matters; the language just tilts the floor toward safety.

> **Note:** Bound parameters protect **values**, not **identifiers**. You can never bind a table name, column name, `ORDER BY` direction, or `LIMIT` keyword. That is a SQL-engine limitation shared by every language. Dynamic identifiers need an allowlist; see Common Pitfalls below.

---

## Common Pitfalls

### Pitfall 1: `format!` into SQL compiles perfectly

The most dangerous thing about injection in Rust is that the borrow checker will *not* save you here — building SQL with `format!` is valid Rust:

```rust
// VULNERABLE — compiles and runs, but is injectable.
async fn find_unsafe(pool: &sqlx::SqlitePool, name: &str) -> Result<(), sqlx::Error> {
    let sql = format!("SELECT id FROM users WHERE name = '{name}'");
    sqlx::query(&sql).fetch_all(pool).await?;
    Ok(())
}
```

There is no compiler error, no Clippy warning by default. If `name` is `' OR '1'='1`, this leaks every row. **Rule:** if a user-controlled value ends up inside `format!`, `+`, or `write!` that builds SQL, you have a bug. Search your codebase for `format!(` near `query(` as a quick audit.

### Pitfall 2: Trying to bind an identifier and being surprised it "doesn't work"

Placeholders only stand in for values. This compiles and runs, but the sort does nothing useful:

```rust
// The `?` cannot be a column name; the DB binds it as a value and ignores it
// for ordering, so the rows come back in their natural order.
async fn order_by(pool: &sqlx::SqlitePool, column: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT id, name FROM users ORDER BY ?")
        .bind(column)
        .fetch_all(pool)
        .await?;
    Ok(())
}
```

Running `order_by(&pool, "name")` does **not** sort by name. The bound `?` is treated as a constant value in the `ORDER BY`, which SQLite ignores. The fix is *not* to switch back to `format!` with the raw input; it is to use an allowlist (Best Practices below).

### Pitfall 3: Forgetting `.await`, then misreading the error

SQLx query builders are **lazy futures**: nothing runs until you `.await`. Drop the `.await` and you get a type error, not a runtime hang:

```rust
async fn count(pool: &sqlx::SqlitePool) -> Result<i64, sqlx::Error> {
    // forgot `.await` on fetch_one
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_one(pool);
    Ok(row.0)
}
```

The real compiler error:

```text
error[E0308]: mismatched types
 --> src/main.rs:6:23
  |
6 |     let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_one(pool);
  |              ------   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `(i64,)`, found future
  |              |
  |              expected due to this
```

`found future` is the tell: add `.await`. This is unlike JavaScript, where a forgotten `await` silently yields a `Promise` that often "works" enough to mask the bug until later. See [Futures are lazy](/11-async/) for why.

### Pitfall 4: Assuming an ORM means you are immune

Even with a query builder or ORM, the moment you drop to a raw-SQL escape hatch (`$queryRawUnsafe` in Prisma, `sqlx::query` with a hand-built string) you are back on the hook. Injection is about *how the value reaches the engine*, not which library you used.

---

## Best Practices

- **Always bind values.** Reach for `.bind(...)` or the `query!`/`query_as!` macros. Treat any `format!`/string-concatenated SQL that includes user input as a defect.
- **Use the compile-time-checked macros when you can.** `query!`/`query_as!` catch typos, missing columns, and type mismatches at build time, a strict improvement over TypeScript's unverified raw queries.
- **For dynamic identifiers, use an allowlist** that maps user input to your own constant strings. Only literals you control ever reach the SQL:

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, FromRow)]
struct User {
    name: String,
}

// Map untrusted input to one of OUR OWN literals. The raw input never
// reaches the SQL — only a `&'static str` we hand-picked does.
fn safe_sort_column(input: &str) -> &'static str {
    match input {
        "name" => "name",
        "id" => "id",
        _ => "id", // default; reject or fall back, never echo user input
    }
}

async fn list_sorted(pool: &SqlitePool, sort: &str) -> Result<Vec<User>, sqlx::Error> {
    let column = safe_sort_column(sort);
    // `column` is one of our literals, so this interpolation is safe.
    let sql = format!("SELECT name FROM users ORDER BY {column}");
    sqlx::query_as::<_, User>(&sql).fetch_all(pool).await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO users (name) VALUES ('Zed'), ('Amy')")
        .execute(&pool)
        .await?;

    let by_name = list_sorted(&pool, "name").await?;
    println!("sorted by name -> {:?}", by_name);

    // Attacker-supplied column is ignored; falls back to a safe default.
    let evil = list_sorted(&pool, "name; DROP TABLE users; --").await?;
    println!("evil sort input -> {:?} (fell back to id)", evil);

    Ok(())
}
```

Real output, the malicious input is discarded and the table survives:

```text
sorted by name -> [User { name: "Amy" }, User { name: "Zed" }]
evil sort input -> [User { name: "Zed" }, User { name: "Amy" }] (fell back to id)
```

- **Apply least-privilege at the database layer too.** The application's DB role should not be able to `DROP TABLE` or read tables it doesn't need. Defense in depth means even a missed bind cannot become a catastrophe.
- **Pair binding with input validation.** Parameterization stops injection, but it does not stop a 10 MB "name" or an email that is not an email. Validate shape and bounds first; see [Input Validation](/27-security/00-input-validation/).

---

## Real-World Example

A search endpoint almost always has *optional* filters: filter by role if one was given, restrict to active users if asked. The temptation is to build the `WHERE` clause with string concatenation. SQLx's `QueryBuilder` lets you assemble dynamic SQL while **every value still goes through `push_bind`**, which appends a placeholder and binds the value, so the query stays injection-proof no matter which filters are present.

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool};

#[derive(Debug, FromRow)]
struct User {
    id: i64,
    name: String,
}

async fn setup(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, role TEXT, active INTEGER)")
        .execute(pool)
        .await?;
    let rows = [
        ("Alice", "admin", 1),
        ("Bob", "user", 1),
        ("Carol", "user", 0),
    ];
    for (name, role, active) in rows {
        sqlx::query("INSERT INTO users (name, role, active) VALUES (?, ?, ?)")
            .bind(name)
            .bind(role)
            .bind(active)
            .execute(pool)
            .await?;
    }
    Ok(())
}

// Dynamic filtering done SAFELY: each user-supplied value goes through
// push_bind, which appends a placeholder AND binds the value.
async fn search(
    pool: &SqlitePool,
    role: Option<&str>,
    only_active: bool,
) -> Result<Vec<User>, sqlx::Error> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT id, name FROM users WHERE 1 = 1");
    if let Some(role) = role {
        qb.push(" AND role = ");
        qb.push_bind(role); // <- placeholder + bound value, not concatenation
    }
    if only_active {
        qb.push(" AND active = ");
        qb.push_bind(1);
    }
    qb.build_query_as::<User>().fetch_all(pool).await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    setup(&pool).await?;

    let admins = search(&pool, Some("admin"), false).await?;
    println!("role=admin -> {:?}", admins);

    let active_users = search(&pool, Some("user"), true).await?;
    println!("role=user & active -> {:?}", active_users);

    // Even a malicious "role" is bound as a value, not SQL.
    let evil = search(&pool, Some("user' OR '1'='1"), false).await?;
    println!("evil role -> {} row(s)", evil.len());

    Ok(())
}
```

Real output:

```text
role=admin -> [User { id: 1, name: "Alice" }]
role=user & active -> [User { id: 2, name: "Bob" }]
evil role -> 0 row(s)
```

The key line is `qb.push_bind(role)`. It writes a placeholder into the SQL and stores the value to bind, exactly like the static `?` case, only assembled at runtime. The structural keywords (`AND role =`) are your own constant strings via `push`; only values ever go through `push_bind`. The malicious `role` returns zero rows because, again, it is compared as a literal string. This is the pattern to reach for whenever you would otherwise be tempted to concatenate a `WHERE` clause.

> **Tip:** Use `push` only for fixed SQL fragments you wrote yourself, and `push_bind` for every value. If you find yourself passing user input to `push`, stop — that is the concatenation bug in a new outfit.

---

## Further Reading

- [SQLx documentation](https://docs.rs/sqlx) — the `query`/`query_as` functions, the `query!` macros, and `QueryBuilder`.
- [SQLx `QueryBuilder`](https://docs.rs/sqlx/latest/sqlx/struct.QueryBuilder.html) — building dynamic queries safely with `push_bind`.
- [OWASP SQL Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/SQL_Injection_Prevention_Cheat_Sheet.html) — the canonical, language-agnostic reference.
- [Input Validation](/27-security/00-input-validation/) — validate the shape of input before it reaches the database.
- [Cryptography done right](/27-security/03-cryptography/) and [Password hashing](/27-security/04-password-hashing/) — sibling security topics.
- [Section 17: Database](/17-database/) — the SQLx and Diesel chapters in depth, including [Writing Queries with SQLx](/17-database/01-sqlx-queries/).
- [Section 11: Async](/11-async/) — why query builders are lazy futures that need `.await`.
- [Section 02: Basics](/02-basics/) — string formatting with `format!`.
- [Section 28: Production](/28-production/) — running services with least-privilege database roles and other hardening.

---

## Exercises

### Exercise 1: Convert a vulnerable query

**Difficulty:** Beginner

**Objective:** Recognize and fix string-concatenated SQL.

**Instructions:** The function below builds SQL with `format!`. Rewrite it to use a bound parameter so the injection payload `' OR '1'='1` returns no rows. Assume a table `products (id INTEGER PRIMARY KEY, sku TEXT)`.

```rust
async fn find_by_sku(pool: &sqlx::SqlitePool, sku: &str) -> Result<Vec<(i64, String)>, sqlx::Error> {
    let sql = format!("SELECT id, sku FROM products WHERE sku = '{sku}'"); // vulnerable
    sqlx::query_as::<_, (i64, String)>(&sql).fetch_all(pool).await
}
```

<details>
<summary>Solution</summary>

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

// Bind the value; the `?` is filled by the driver, not by format!.
async fn find_by_sku(pool: &SqlitePool, sku: &str) -> Result<Vec<(i64, String)>, sqlx::Error> {
    sqlx::query_as::<_, (i64, String)>("SELECT id, sku FROM products WHERE sku = ?")
        .bind(sku)
        .fetch_all(pool)
        .await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query("CREATE TABLE products (id INTEGER PRIMARY KEY, sku TEXT)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO products (sku) VALUES ('A-100'), ('B-200')")
        .execute(&pool)
        .await?;

    let normal = find_by_sku(&pool, "A-100").await?;
    println!("A-100 -> {} row(s)", normal.len());

    let evil = find_by_sku(&pool, "' OR '1'='1").await?;
    println!("injection -> {} row(s)", evil.len());

    Ok(())
}
```

Real output:

```text
A-100 -> 1 row(s)
injection -> 0 row(s)
```

</details>

### Exercise 2: Safe dynamic sorting

**Difficulty:** Intermediate

**Objective:** Handle a user-chosen sort column without concatenating raw input.

**Instructions:** Write `list_products(pool, sort_by)` that orders results by `"sku"` or `"id"` based on `sort_by`. Any other value must fall back to `"id"`. Confirm that passing `"sku; DROP TABLE products; --"` does not run any extra SQL and leaves the table intact.

<details>
<summary>Solution</summary>

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

// Map untrusted input to one of our own literals — an allowlist.
fn sort_column(input: &str) -> &'static str {
    match input {
        "sku" => "sku",
        "id" => "id",
        _ => "id",
    }
}

async fn list_products(pool: &SqlitePool, sort_by: &str) -> Result<Vec<(i64, String)>, sqlx::Error> {
    let column = sort_column(sort_by);
    let sql = format!("SELECT id, sku FROM products ORDER BY {column}");
    sqlx::query_as::<_, (i64, String)>(&sql).fetch_all(pool).await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query("CREATE TABLE products (id INTEGER PRIMARY KEY, sku TEXT)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO products (sku) VALUES ('B-200'), ('A-100')")
        .execute(&pool)
        .await?;

    let by_sku = list_products(&pool, "sku").await?;
    println!("by sku -> {:?}", by_sku);

    let evil = list_products(&pool, "sku; DROP TABLE products; --").await?;
    println!("evil sort -> {:?} (fell back to id, table intact)", evil);

    Ok(())
}
```

Real output:

```text
by sku -> [(2, "A-100"), (1, "B-200")]
evil sort -> [(1, "B-200"), (2, "A-100")] (fell back to id, table intact)
```

The malicious string never reaches the SQL — only the literal `"id"` does — so the table is never dropped.

</details>

### Exercise 3: Optional filters with `QueryBuilder`

**Difficulty:** Advanced

**Objective:** Build a query with zero, one, or both optional filters where every value is bound.

**Instructions:** Write `search_orders(pool, status, min_total)` against a table `orders (id INTEGER PRIMARY KEY, status TEXT, total INTEGER)`. Add `AND status = ?` only when `status` is `Some`, and `AND total >= ?` only when `min_total` is `Some`. Use `QueryBuilder` and `push_bind` so all values are bound. Verify that a malicious `status` returns no rows.

<details>
<summary>Solution</summary>

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

async fn search_orders(
    pool: &SqlitePool,
    status: Option<&str>,
    min_total: Option<i64>,
) -> Result<Vec<(i64, String, i64)>, sqlx::Error> {
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT id, status, total FROM orders WHERE 1 = 1");
    if let Some(status) = status {
        qb.push(" AND status = ");
        qb.push_bind(status);
    }
    if let Some(min_total) = min_total {
        qb.push(" AND total >= ");
        qb.push_bind(min_total);
    }
    qb.build_query_as::<(i64, String, i64)>()
        .fetch_all(pool)
        .await
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query("CREATE TABLE orders (id INTEGER PRIMARY KEY, status TEXT, total INTEGER)")
        .execute(&pool)
        .await?;
    for (status, total) in [("paid", 100), ("paid", 50), ("pending", 200)] {
        sqlx::query("INSERT INTO orders (status, total) VALUES (?, ?)")
            .bind(status)
            .bind(total)
            .execute(&pool)
            .await?;
    }

    let paid_big = search_orders(&pool, Some("paid"), Some(80)).await?;
    println!("paid & total>=80 -> {:?}", paid_big);

    let no_filters = search_orders(&pool, None, None).await?;
    println!("no filters -> {} row(s)", no_filters.len());

    let evil = search_orders(&pool, Some("paid' OR '1'='1"), None).await?;
    println!("evil status -> {} row(s)", evil.len());

    Ok(())
}
```

Real output:

```text
paid & total>=80 -> [(1, "paid", 100)]
no filters -> 3 row(s)
evil status -> 0 row(s)
```

</details>
