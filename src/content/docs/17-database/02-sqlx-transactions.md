---
title: "Transactions with SQLx"
description: "SQLx makes a Rust transaction a value: begin, run through &mut *tx, then commit. Drop rolls back automatically, so the leaks Node's manual try/finally"
---

Database transactions let you group several statements into one atomic unit: either all of them take effect, or none of them do. SQLx models a transaction as a Rust value — a guard — whose lifetime *is* the transaction, so the borrow checker and `Drop` enforce correctness that you have to remember by hand in TypeScript.

---

## Quick Overview

A **transaction** wraps a set of SQL statements so they commit together or roll back together. The database stays consistent even if your program panics, the connection drops, or a constraint is violated halfway through. In SQLx you call `pool.begin().await` to get a `Transaction` value, run queries through `&mut *tx`, and finish with `tx.commit()` or `tx.rollback()`. The key difference from a Node.js client: if you simply *drop* the `Transaction` without committing, SQLx rolls it back for you. There is no way to leak an open transaction by forgetting a `finally` block.

> **Note:** This file assumes you have read [SQLx Intro](/17-database/00-sqlx-intro/) (setup, connecting, the pool) and [Writing Queries](/17-database/01-sqlx-queries/) (`query`, `query_as`, parameter binding). Here we focus only on transactions: `begin`/`commit`/`rollback`, the `Transaction` guard, atomicity, and savepoints.

---

## TypeScript/JavaScript Example

With `node-postgres` (`pg`), a transaction means manually pulling a client out of the pool, issuing `BEGIN`/`COMMIT`/`ROLLBACK` as raw statements, and — critically — remembering to `release()` the client and to roll back on every error path:

```typescript
import { Pool } from "pg";

const pool = new Pool({ connectionString: process.env.DATABASE_URL });

async function transfer(from: number, to: number, amount: number): Promise<void> {
  // You must check out a single client; the whole transaction lives on it.
  const client = await pool.connect();
  try {
    await client.query("BEGIN");

    await client.query(
      "UPDATE accounts SET balance = balance - $1 WHERE id = $2",
      [amount, from],
    );
    await client.query(
      "UPDATE accounts SET balance = balance + $1 WHERE id = $2",
      [amount, to],
    );

    await client.query("COMMIT");
  } catch (err) {
    // If you forget this, the transaction stays open and holds a connection.
    await client.query("ROLLBACK");
    throw err;
  } finally {
    // If you forget this, the client never returns to the pool — a leak.
    client.release();
  }
}
```

The footguns are well known to anyone who has run Postgres in production:

- Forget `ROLLBACK` in the `catch` and the connection sits in an aborted-transaction state.
- Forget `client.release()` in the `finally` and you slowly exhaust the pool until every request hangs.
- Accidentally run a query on `pool` instead of `client` and it executes *outside* the transaction — silently, with no error.

Higher-level tools paper over this. Prisma's `$transaction([...])` takes an array of operations; TypeORM has `dataSource.transaction(async (manager) => { ... })`. Both rely on you routing every call through the right object and on a callback that, if it throws, triggers a rollback. Nothing in the type system stops you from using the wrong handle.

---

## Rust Equivalent

SQLx turns the transaction itself into a value. `pool.begin()` hands you a `Transaction<'_, DB>`; you run queries through `&mut *tx`; you finish with `commit()` or `rollback()`. There is no separate "check out the client / release the client" dance — the `Transaction` owns its connection and returns it to the pool when dropped.

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Pool, Sqlite};

async fn transfer(
    pool: &Pool<Sqlite>,
    from: i64,
    to: i64,
    amount: i64,
) -> Result<(), sqlx::Error> {
    // `begin` checks out a connection and issues BEGIN. `tx` is the guard.
    let mut tx = pool.begin().await?;

    sqlx::query("UPDATE accounts SET balance = balance - ? WHERE id = ?")
        .bind(amount)
        .bind(from)
        .execute(&mut *tx) // run *through the transaction*, not the pool
        .await?;

    sqlx::query("UPDATE accounts SET balance = balance + ? WHERE id = ?")
        .bind(amount)
        .bind(to)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?; // both UPDATEs become permanent together
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await?;

    // (table setup omitted — see the Real-World Example for the full program)
    transfer(&pool, 1, 2, 30).await?;
    Ok(())
}
```

The dependencies for the examples in this file (SQLite keeps them runnable with no external server; the same code works against Postgres or MySQL by swapping the pool type and connection string):

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

```bash
cargo add sqlx --features runtime-tokio,sqlite
cargo add tokio --features rt-multi-thread,macros
```

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically. The examples were compiled and run with SQLx 0.8 and Tokio 1.

The two missing error paths from the TypeScript version — "forgot to roll back" and "forgot to release" — simply cannot happen here. We will see why next.

---

## Detailed Explanation

### `begin()` returns a guard, not a string command

In `pg` you *send* the word `BEGIN`. In SQLx you *receive* a value:

```rust
let mut tx = pool.begin().await?;
```

`tx` has type `sqlx::Transaction<'_, Sqlite>`. Three things happened in that one line:

1. A connection was checked out of the pool (or a new one opened if the pool had spare capacity).
2. `BEGIN` was sent on that connection.
3. Ownership of the connection-plus-open-transaction was moved into `tx`.

Because `tx` *owns* the connection, there is no separate handle to release. The connection's fate is tied to `tx`'s fate.

### `&mut *tx` — running a query "through" the transaction

Every executor method (`execute`, `fetch_one`, `fetch_all`, `fetch_optional`) accepts something that implements the `Executor` trait. A `&Pool` implements it (each call grabs a connection from the pool). A `&mut Transaction` does *not* directly, but a `&mut <Connection>` does, and `*tx` derefs the transaction to its underlying connection, so `&mut *tx` is "a mutable reference to this transaction's connection":

```rust
sqlx::query("UPDATE accounts SET balance = balance - ? WHERE id = ?")
    .bind(amount)
    .bind(from)
    .execute(&mut *tx) // <- the transaction's connection
    .await?;
```

This is the single most important habit to build: **inside a transaction, pass `&mut *tx`, never `&pool`.** If you pass `&pool` you check out a *different* connection and run the statement outside your transaction. (See [Common Pitfalls](#common-pitfalls).) The `&mut` is what forces statements to run one at a time, in order — the borrow checker won't let two queries borrow the same transaction concurrently, which mirrors how SQL transactions are inherently sequential on a single connection.

### `commit` and `rollback` consume the guard

```rust
tx.commit().await?;
```

`commit` takes `self` by value (it consumes `tx`), sends `COMMIT`, and returns the connection to the pool. After this line `tx` no longer exists; you cannot accidentally run another statement on a committed transaction, because the value is gone. `rollback` works the same way but sends `ROLLBACK`. Both are `async` and return `Result`, because the round-trip to the database can itself fail.

### The guard: implicit rollback on drop

Here is the behavior that has no TypeScript equivalent. If a `Transaction` is dropped — because you returned early, an error propagated via `?`, or a panic unwound the stack — **SQLx rolls it back automatically**:

```rust
async fn guard_rollback(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    {
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE accounts SET balance = 12345 WHERE id = 2")
            .execute(&mut *tx)
            .await?;
        // No commit, no rollback: `tx` is dropped here -> rolled back.
    }
    Ok(())
}
```

Running this against a fresh database, then reading account 2 back, prints:

```text
bob after guard rollback: balance=80
```

The balance is unchanged — the `UPDATE` to `12345` was discarded. This is the RAII (Resource Acquisition Is Initialization) pattern: the resource (the open transaction) is released by `Drop`, deterministically, the instant the value goes out of scope. It is the same mechanism that closes a Rust `File` or releases a `MutexGuard`. The TypeScript `finally` block is a *runtime convention you must write*; the Rust rollback-on-drop is a *language guarantee you cannot forget*.

> **Note:** Because SQLx is async, the drop-rollback is "best effort" in one narrow sense: the rollback statement is queued on the connection and sent when the connection is next used (or cleaned up by the pool). The transaction's effects are never visible to other connections, so atomicity holds; you just shouldn't rely on the `ROLLBACK` having flushed at the exact instant of the drop. When you care, call `rollback().await` explicitly.

### Explicit rollback when there is no error

Sometimes you want to abandon a transaction even though nothing failed: for example, a "dry run" or a business rule that says "don't proceed". Call `rollback()` explicitly:

```rust
async fn explicit_rollback(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE accounts SET balance = 999 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    // Changed our mind: undo everything.
    tx.rollback().await?;
    Ok(())
}
```

### Atomicity in action

Atomicity is the "A" in ACID: the transaction is all-or-nothing. Consider a transaction whose *second* statement violates a `CHECK (balance >= 0)` constraint:

```rust
async fn overdraw(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    // This one would succeed on its own.
    sqlx::query("UPDATE accounts SET balance = balance + 1000 WHERE id = 2")
        .execute(&mut *tx)
        .await?;

    // This one fails the CHECK constraint.
    sqlx::query("UPDATE accounts SET balance = balance - 1000 WHERE id = 1")
        .execute(&mut *tx)
        .await?; // <-- returns Err here; we never reach commit

    tx.commit().await?;
    Ok(())
}
```

The second `execute().await?` returns `Err`, so the function returns early via `?`, `tx` is dropped, and the transaction rolls back — including the *first* `UPDATE`, which had already executed successfully. The real output, starting from `alice=100, bob=50`:

```text
overdraw failed: error returned from database: (code: 275) CHECK constraint failed: balance >= 0
balances after failed tx: [(1, 100), (2, 50)]
```

Both balances are exactly as they started. The `+1000` to bob never persisted, because it was part of the same atomic unit as the failing statement. That is the entire point of a transaction, and SQLx gave it to you for free: you wrote no rollback handler.

---

## Key Differences

| Concern | TypeScript (`pg` / Prisma / TypeORM) | Rust (SQLx) |
| --- | --- | --- |
| What a transaction *is* | Raw `BEGIN`/`COMMIT` strings on a checked-out client, or a callback | A `Transaction<'_, DB>` value (a guard) |
| Getting a transaction | `const c = await pool.connect(); await c.query("BEGIN")` | `let mut tx = pool.begin().await?;` |
| Running a statement in it | `await client.query(...)` (must use `client`, not `pool`) | `.execute(&mut *tx).await?` (must use `tx`, not `pool`) |
| Committing | `await client.query("COMMIT")` | `tx.commit().await?` (consumes `tx`) |
| Rollback on error | Manual `catch { ROLLBACK }` you must write | Automatic on drop; or explicit `tx.rollback().await?` |
| Returning the connection | Manual `client.release()` in `finally` | Automatic on drop; there is no `release` |
| Use-after-commit | Allowed; runtime error or silent no-op | Compile error; `commit` moved the value |
| Wrong-handle bug | Easy: `pool.query` instead of `client.query` runs outside the tx | Easy to *write* `&pool`, but it is a clearly different argument; linters and review catch it, and it never leaks the tx |
| Nested transactions | Library-specific; often unsupported | `tx.begin()` opens a real SQL `SAVEPOINT` |

The mental shift: in TypeScript a transaction is a *protocol you follow* (begin, then remember to commit-or-rollback, then remember to release). In Rust a transaction is a *type you hold*, and the compiler plus `Drop` enforce the protocol. Forgetting cleanup is not a bug you can write.

### Savepoints (nested transactions)

Calling `begin()` on a `Transaction` (rather than on a `Pool`) opens a **savepoint** — a nested, partially-rollback-able sub-transaction. Rolling back the inner one undoes only its work; the outer transaction continues:

```rust
use sqlx::Acquire; // brings the nesting `begin` (a SAVEPOINT) into scope on Transaction

let mut tx = pool.begin().await?;
sqlx::query("UPDATE t SET n = 10 WHERE id = 1")
    .execute(&mut *tx)
    .await?;

// Nested transaction == SAVEPOINT.
let mut sp = tx.begin().await?;
sqlx::query("UPDATE t SET n = 999 WHERE id = 1")
    .execute(&mut *sp)
    .await?;
sp.rollback().await?; // undo just the savepoint

tx.commit().await?; // keeps n = 10
```

Final value after running this:

```text
final n after savepoint rollback + outer commit: 10
```

The inner `n = 999` was discarded; the outer `n = 10` committed. This is how you implement "try this sub-step, and if it fails, undo only it and carry on" — something most JavaScript ORMs handle poorly or not at all.

---

## Common Pitfalls

### Pitfall 1: Running queries on the pool instead of the transaction

This is the direct analog of calling `pool.query` instead of `client.query` in `pg`. It compiles, runs, and silently executes *outside* your transaction:

```rust
// Here `pool` is an OWNED `Pool<Sqlite>`, so `&pool` is `&Pool` — a valid `Executor`.
let mut tx = pool.begin().await?;

sqlx::query("UPDATE accounts SET balance = balance - 10 WHERE id = 1")
    .execute(&pool) // logic bug: runs on a DIFFERENT connection, outside `tx`
    .await?;

tx.commit().await?; // commits an EMPTY transaction; the UPDATE already happened
```

There is no compiler error here because `&pool` (where `pool` is an owned `Pool<Sqlite>`) is a perfectly valid `Executor`; it just isn't *your* transaction. The fix is mechanical: inside a transaction, always pass `&mut *tx`. A good habit is to write a helper that takes `&mut Transaction` so the type system stops you from passing a pool by mistake (see [Best Practices](#best-practices)).

> **Note:** This silent bug only slips past the compiler when `pool` is owned. Most functions in this file take `pool: &Pool<Sqlite>` by reference; there, `&pool` is `&&Pool<Sqlite>`, which does *not* implement `Executor`, so the same typo becomes a compile error (`error[E0277]: the trait bound `&&Pool<Sqlite>: Executor<'_>` is not satisfied`). Either way, pass `&mut *tx` and the ambiguity disappears.

### Pitfall 2: Using a transaction after committing it

`commit` (and `rollback`) take ownership of the transaction. Touch it afterward and you get a *compile* error, not a runtime surprise:

```rust
let mut tx = pool.begin().await?;
sqlx::query("INSERT INTO t (id) VALUES (1)")
    .execute(&mut *tx)
    .await?;
tx.commit().await?;

// does not compile (error[E0382]: borrow of moved value: `tx`)
sqlx::query("INSERT INTO t (id) VALUES (2)")
    .execute(&mut *tx)
    .await?;
```

The real error from `cargo build`:

```text
error[E0382]: borrow of moved value: `tx`
   --> src/main.rs:18:24
    |
 10 |     let mut tx = pool.begin().await?;
    |         ------ move occurs because `tx` has type `Transaction<'_, Sqlite>`, which does not implement the `Copy` trait
...
 14 |     tx.commit().await?;
    |        -------- `tx` moved due to this method call
...
 18 |         .execute(&mut *tx)
    |                        ^^ value borrowed here after move
    |
note: `Transaction::<'c, DB>::commit` takes ownership of the receiver `self`, which moves `tx`
   --> .../sqlx-core-0.8.6/src/transaction.rs:116:29
    |
116 |     pub async fn commit(mut self) -> Result<(), Error> {
    |                             ^^^^
```

In TypeScript, calling `client.query("INSERT ...")` after `COMMIT` would either start a brand-new implicit transaction or error at runtime depending on the driver. Rust catches the mistake before the program ever runs. If you genuinely need a *second* transaction, call `pool.begin()` again to get a fresh one.

### Pitfall 3: Forgetting `commit` and expecting the writes to stick

Because of the rollback-on-drop guard, the *opposite* of the TypeScript leak happens: if you forget to commit, your writes silently vanish rather than leaking a connection.

```rust
async fn save(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO logs (msg) VALUES ('hello')")
        .execute(&mut *tx)
        .await?;
    Ok(()) // logic bug: no commit -> tx dropped -> INSERT rolled back
}
```

This compiles and the function returns `Ok(())`, but nothing was saved. The cure is to make `commit` the last meaningful line before the success path returns, and to lean on `?` so any earlier failure rolls back as intended.

### Pitfall 4: Holding a transaction open across slow work

A `Transaction` holds a connection out of the pool and (on most databases) holds row locks. If you `begin()`, then `.await` a slow HTTP call, then run your SQL, you are starving the pool and blocking other writers for the whole network round-trip. Do the slow work *before* `begin()`, keep the transaction as short as possible, and commit promptly. This is the same advice as "keep `BEGIN`...`COMMIT` short" in any database, but it matters more in an async server where a single stuck transaction can back up an entire request queue. See also [Connection Pooling](/17-database/08-connection-pooling/).

---

## Best Practices

### Write helpers that borrow the transaction

Make functions take `&mut Transaction<'_, DB>` so callers can compose multi-step work, and so the type system prevents anyone from accidentally passing a pool. Note the double-deref `&mut **tx`: `*tx` derefs the `&mut Transaction` reference, and `**tx` derefs the `Transaction` to its connection.

```rust
use sqlx::{Sqlite, Transaction};

async fn reserve_stock(
    tx: &mut Transaction<'_, Sqlite>,
    product_id: i64,
    qty: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE products SET stock = stock - ? WHERE id = ?")
        .bind(qty)
        .bind(product_id)
        .execute(&mut **tx) // &mut Transaction -> &mut Connection
        .await?;
    Ok(())
}
```

### Or be generic over `Acquire` so a helper works with a pool *or* a transaction

If a helper should be usable both standalone (against the pool) and inside a transaction, bound it on `Acquire`:

```rust
use sqlx::{Acquire, Sqlite};

async fn debit<'c, A>(conn: A, id: i64, amount: i64) -> Result<(), sqlx::Error>
where
    A: Acquire<'c, Database = Sqlite>,
{
    let mut conn = conn.acquire().await?;
    sqlx::query("UPDATE accounts SET balance = balance - ? WHERE id = ?")
        .bind(amount)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

// Call it on a transaction:  debit(&mut *tx, 1, 10).await?;
// or directly on the pool:   debit(&pool, 1, 10).await?;
```

### Let `?` drive rollback

Don't write `match` arms that call `rollback()` on every error — that is the manual TypeScript style. Use `?` and let the guard roll back on the early return. Only call `rollback()` explicitly when you are abandoning the transaction *without* an error (a business-rule "no", a dry run).

### Commit exactly once, at the end of the happy path

Structure functions so every fallible step uses `?` and the single `tx.commit().await?` is the last thing before returning success. This makes the all-or-nothing boundary obvious to readers.

### Prefer compile-checked macros for the queries inside

Transactions compose with the `query!` / `query_as!` macros exactly like `query` / `query_as` do — pass `&mut *tx` as the executor. The macros verify your SQL against the live schema at compile time. That is the subject of [Writing Queries](/17-database/01-sqlx-queries/); everything in this file works identically with the macro forms.

---

## Real-World Example

A production-flavored "place an order" service: insert an order, decrement stock for each line item, and record the line items — all atomically. If any product is out of stock (a `CHECK (stock >= 0)` constraint), the *entire* order is rolled back, including the order row itself. This is the canonical use case for transactions.

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Pool, Sqlite, Transaction};

#[derive(Debug)]
struct OrderLine {
    product_id: i64,
    quantity: i64,
}

async fn setup(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE products (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            stock INTEGER NOT NULL CHECK (stock >= 0)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE TABLE orders (id INTEGER PRIMARY KEY, total INTEGER NOT NULL)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TABLE order_items (
            order_id INTEGER NOT NULL,
            product_id INTEGER NOT NULL,
            quantity INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO products (id, name, stock) VALUES (1, 'Keyboard', 5), (2, 'Mouse', 3)")
        .execute(pool)
        .await?;
    Ok(())
}

// Reusable helper that operates on a borrowed transaction.
async fn reserve_stock(
    tx: &mut Transaction<'_, Sqlite>,
    product_id: i64,
    qty: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE products SET stock = stock - ? WHERE id = ?")
        .bind(qty)
        .bind(product_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn place_order(pool: &Pool<Sqlite>, lines: &[OrderLine]) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let order_id: i64 = sqlx::query_scalar("INSERT INTO orders (total) VALUES (0) RETURNING id")
        .fetch_one(&mut *tx)
        .await?;

    for line in lines {
        reserve_stock(&mut tx, line.product_id, line.quantity).await?;
        sqlx::query("INSERT INTO order_items (order_id, product_id, quantity) VALUES (?, ?, ?)")
            .bind(order_id)
            .bind(line.product_id)
            .bind(line.quantity)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(order_id)
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    setup(&pool).await?;

    // Succeeds: enough stock.
    let id = place_order(
        &pool,
        &[
            OrderLine { product_id: 1, quantity: 2 },
            OrderLine { product_id: 2, quantity: 1 },
        ],
    )
    .await?;
    println!("placed order {id}");

    // Fails: not enough Mouse stock. The WHOLE order rolls back.
    let result = place_order(
        &pool,
        &[
            OrderLine { product_id: 1, quantity: 1 },
            OrderLine { product_id: 2, quantity: 99 },
        ],
    )
    .await;
    println!("second order: {result:?}");

    // Atomicity check: the Keyboard from the failed order was NOT decremented,
    // and no second order row was persisted.
    let stocks: Vec<(String, i64)> =
        sqlx::query_as("SELECT name, stock FROM products ORDER BY id")
            .fetch_all(&pool)
            .await?;
    println!("final stock: {stocks:?}");

    let order_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM orders")
        .fetch_one(&pool)
        .await?;
    println!("orders persisted: {}", order_count.0);

    Ok(())
}
```

Real output:

```text
placed order 1
second order: Err(Database(SqliteError { code: 275, message: "CHECK constraint failed: stock >= 0" }))
final stock: [("Keyboard", 3), ("Mouse", 2)]
orders persisted: 1
```

Read that output carefully; it is the whole lesson:

- The first order placed (`id 1`) and decremented stock to Keyboard 3, Mouse 2.
- The second order tried Keyboard 1 (which would have succeeded) and Mouse 99 (which violated the constraint). The Mouse statement failed, `?` propagated the error, `tx` dropped, and **everything in that transaction rolled back**.
- Final stock is `Keyboard 3` — *not* 2. The Keyboard decrement from the failed order was undone, even though that statement had already executed successfully.
- `orders persisted: 1`: the `INSERT INTO orders` row from the failed attempt was rolled back too, so no orphan order exists.

You wrote zero rollback handling. The `?` operator and the `Drop` guard did it.

---

## Further Reading

- [SQLx `Transaction` API (docs.rs)](https://docs.rs/sqlx/latest/sqlx/struct.Transaction.html): `commit`, `rollback`, and the `Drop` behavior.
- [SQLx `Pool::begin` (docs.rs)](https://docs.rs/sqlx/latest/sqlx/struct.Pool.html#method.begin): how a transaction is acquired from the pool.
- [SQLx `Acquire` trait (docs.rs)](https://docs.rs/sqlx/latest/sqlx/trait.Acquire.html): for writing helpers generic over pool vs. transaction.
- [PostgreSQL: Transactions tutorial](https://www.postgresql.org/docs/current/tutorial-transactions.html): the database-side semantics (ACID, isolation) behind all of this.

Within this guide:

- [SQLx Intro](/17-database/00-sqlx-intro/): setup, connecting, and the pool you call `begin()` on.
- [Writing Queries](/17-database/01-sqlx-queries/): `query!`/`query_as!`, parameter binding, and `FromRow`, all of which work through `&mut *tx`.
- [Connection Pooling](/17-database/08-connection-pooling/): why a long-held transaction starves the pool, and how to size it.
- [Migrations](/17-database/09-migrations/): schema changes (which themselves run inside transactions where the engine supports it).
- [ORM Comparison](/17-database/10-orm-comparison/): how Diesel and SeaORM model transactions differently.
- [Section 08: Error Handling](/08-error-handling/): the `Result` and `?` that drive automatic rollback.
- [Section 11: Async — promises vs. futures](/11-async/00-promises-vs-futures/) — why every `.await` here is lazy and needs the Tokio runtime, unlike an eager JavaScript `Promise`.
- [Section 10: Smart Pointers](/10-smart-pointers/) — the RAII/`Drop` pattern that powers rollback-on-drop.
- Building a CLI that runs these transactions? See [Section 18: CLI Tools](/18-cli-tools/).

---

## Exercises

### Exercise 1: Guard the balance yourself

**Difficulty:** Beginner

**Objective:** Practice `begin`, an explicit `rollback`, and conditional commit without relying on a database `CHECK` constraint.

**Instructions:** Write `async fn safe_transfer(pool, from, to, amount) -> Result<bool, sqlx::Error>` that begins a transaction, reads the sender's balance, and if it is less than `amount`, rolls back and returns `Ok(false)`. Otherwise it performs both UPDATEs, commits, and returns `Ok(true)`. Use a table `accounts(id INTEGER PRIMARY KEY, balance INTEGER NOT NULL)` seeded with `(1, 100), (2, 50)`.

<details>
<summary>Solution</summary>

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Pool, Sqlite};

async fn safe_transfer(
    pool: &Pool<Sqlite>,
    from: i64,
    to: i64,
    amount: i64,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let from_balance: i64 = sqlx::query_scalar("SELECT balance FROM accounts WHERE id = ?")
        .bind(from)
        .fetch_one(&mut *tx)
        .await?;

    if from_balance < amount {
        tx.rollback().await?; // explicit: no error, we just decline
        return Ok(false);
    }

    sqlx::query("UPDATE accounts SET balance = balance - ? WHERE id = ?")
        .bind(amount)
        .bind(from)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE accounts SET balance = balance + ? WHERE id = ?")
        .bind(amount)
        .bind(to)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(true)
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query("CREATE TABLE accounts (id INTEGER PRIMARY KEY, balance INTEGER NOT NULL)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO accounts (id, balance) VALUES (1, 100), (2, 50)")
        .execute(&pool)
        .await?;

    println!("transfer 30:   {}", safe_transfer(&pool, 1, 2, 30).await?);
    println!("transfer 1000: {}", safe_transfer(&pool, 1, 2, 1000).await?);

    let balances: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id, balance FROM accounts ORDER BY id")
            .fetch_all(&pool)
            .await?;
    println!("balances: {balances:?}");
    Ok(())
}
```

Output:

```text
transfer 30:   true
transfer 1000: false
balances: [(1, 70), (2, 80)]
```

The 1000 transfer was declined and left balances untouched; only the 30 transfer applied.

</details>

### Exercise 2: Prove rollback-on-drop yourself

**Difficulty:** Intermediate

**Objective:** Observe the `Transaction` guard's automatic rollback by deliberately dropping a transaction without committing, then confirming the write did not persist.

**Instructions:** Create a table `events(id INTEGER PRIMARY KEY, name TEXT)`. Write a function that begins a transaction, inserts a row, and returns *without* committing (let `tx` drop). After calling it, query `SELECT COUNT(*) FROM events` and assert it is 0. Then do the same insert in a transaction you *do* commit, and confirm the count is 1.

<details>
<summary>Solution</summary>

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Pool, Sqlite};

async fn insert_then_drop(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO events (name) VALUES ('dropped')")
        .execute(&mut *tx)
        .await?;
    // No commit. `tx` drops here -> ROLLBACK.
    Ok(())
}

async fn insert_then_commit(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO events (name) VALUES ('kept')")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query("CREATE TABLE events (id INTEGER PRIMARY KEY, name TEXT)")
        .execute(&pool)
        .await?;

    insert_then_drop(&pool).await?;
    let after_drop: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events")
        .fetch_one(&pool)
        .await?;
    assert_eq!(after_drop.0, 0);
    println!("after drop, count = {}", after_drop.0);

    insert_then_commit(&pool).await?;
    let after_commit: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events")
        .fetch_one(&pool)
        .await?;
    assert_eq!(after_commit.0, 1);
    println!("after commit, count = {}", after_commit.0);
    Ok(())
}
```

Output:

```text
after drop, count = 0
after commit, count = 1
```

The dropped transaction left nothing behind; only the committed one persisted.

</details>

### Exercise 3: Savepoints for partial rollback

**Difficulty:** Advanced

**Objective:** Use a nested transaction (savepoint) to undo one sub-step while keeping the rest of the outer transaction.

**Instructions:** Create `audit(id INTEGER PRIMARY KEY, action TEXT NOT NULL)`. In one outer transaction: (1) insert `"start"`; (2) open a savepoint with `tx.begin()`, insert `"risky"` through it, then `rollback()` the savepoint; (3) insert `"end"` through the outer transaction; (4) commit the outer transaction. Afterward, the table should contain exactly `start` and `end` — never `risky`. (Remember to bring the nesting `begin` into scope with `use sqlx::Acquire;`.)

<details>
<summary>Solution</summary>

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Acquire; // enables the nesting `begin()` on a Transaction (savepoint)

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query("CREATE TABLE audit (id INTEGER PRIMARY KEY, action TEXT NOT NULL)")
        .execute(&pool)
        .await?;

    let mut tx = pool.begin().await?;

    sqlx::query("INSERT INTO audit (action) VALUES ('start')")
        .execute(&mut *tx)
        .await?;

    // Savepoint: try a risky step, then undo only it.
    let mut sp = tx.begin().await?;
    sqlx::query("INSERT INTO audit (action) VALUES ('risky')")
        .execute(&mut *sp)
        .await?;
    sp.rollback().await?; // discards 'risky' only

    sqlx::query("INSERT INTO audit (action) VALUES ('end')")
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let actions: Vec<(String,)> = sqlx::query_as("SELECT action FROM audit ORDER BY id")
        .fetch_all(&pool)
        .await?;
    println!("{:?}", actions.iter().map(|r| r.0.as_str()).collect::<Vec<_>>());
    Ok(())
}
```

Output:

```text
["start", "end"]
```

The `risky` insert was rolled back at the savepoint, while `start` and `end` survived to the outer commit.

</details>
