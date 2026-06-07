---
title: "Data Migration Strategies"
description: "Move a live database from Node.js to Rust with no downtime: shared tables, dual-write, and chunked, resumable, idempotent backfill, plus reconciliation."
---

When you rewrite a Node.js service in Rust, the code is only half the job. The other half is the data: a live database holding millions of rows that customers depend on right now. This page covers the three strategies that let you move from a Node-owned database to a Rust-owned one without downtime, lost writes, or a terrifying big-bang cutover.

---

## Quick Overview

A rewrite rarely fails on the new code; it fails on the data underneath it. You cannot stop the world, dump a database, and re-import it into a Rust service while customers are writing to it. Instead you use one of three patterns, often in sequence: **shared database** (Rust and Node read/write the same tables), **dual-write** (writes go to both an old and a new store), and **backfill** (a batch job copies historical rows into the new store). This page shows how each looks from a TypeScript/JavaScript developer's perspective and how to express the critical bits idiomatically in Rust, with the type-safety and error-handling guarantees that make Rust a good fit for migration tooling.

> **Note:** This page is about moving *data* during a rewrite. For moving *traffic* incrementally see [Incremental Migration](/29-migration-guide/00-incremental/); for keeping HTTP responses byte-compatible see [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/); for the end-to-end service port see [Porting a Node.js Service to Rust](/29-migration-guide/01-node-to-rust/). For the general database APIs (`sqlx`, Diesel, connection pooling, schema migrations) used in the snippets here, see Section 17, starting with [Transactions with SQLx](/17-database/02-sqlx-transactions/) and [Database Migrations](/17-database/09-migrations/).

---

## TypeScript/JavaScript Example

A typical Node service owns its database. Writes flow straight through an ORM or query builder, and the schema is whatever the Node code has accreted over the years. Here is a dual-write helper written the way a Node team usually first reaches for it. The legacy store is the source of truth, and a new store is shadowed alongside it so the request never fails just because the new store hiccups.

```typescript
// dual-write.ts — Node 22, run with `node --experimental-strip-types`
class WriteError extends Error {}

const legacy = new Map<number, string>();
const next = new Map<number, string>();
let failNext = false;

async function legacyUpsert(id: number, email: string): Promise<void> {
  legacy.set(id, email);
}

async function newUpsert(id: number, email: string): Promise<void> {
  if (failNext) throw new WriteError("new store unavailable");
  next.set(id, email);
}

// Source of truth = legacy. The new store is best-effort.
async function dualWrite(id: number, email: string): Promise<void> {
  await legacyUpsert(id, email); // hard requirement
  try {
    await newUpsert(id, email); // shadow write, best-effort
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    console.error(`shadow write failed for id=${id}: ${msg} (request still succeeds)`);
  }
}

await dualWrite(1, "ada@example.com");
console.log(`after write 1: legacy=${legacy.size} new=${next.size}`);

failNext = true; // simulate the new store going down
await dualWrite(2, "alan@example.com");
console.log(`after write 2: legacy=${legacy.size} new=${next.size}`);
```

Running it under Node v22 prints:

```text
after write 1: legacy=1 new=1
shadow write failed for id=2: new store unavailable (request still succeeds)
after write 2: legacy=2 new=1
```

This works, but notice what TypeScript does *not* protect you from: nothing forces `legacyUpsert` and `newUpsert` to agree on the shape of a row, nothing catches a `number` that has silently lost integer precision, and the `e instanceof Error` dance is needed because a thrown value in JavaScript can be anything. These are exactly the gaps Rust closes.

---

## Rust Equivalent

The same dual-write control flow in Rust. The legacy store returns `Result`, so its failure propagates with `?`; the new (shadow) store's failure is *deliberately* swallowed and logged, so it can never turn into a request failure.

```rust
// Dual-write pattern, distilled to the essential control flow.
// The OLD store is the source of truth; the NEW store is shadowed.
// A failure writing the new store must NEVER fail the request.

use std::collections::HashMap;

#[derive(Debug)]
struct WriteError(String);

#[derive(Default)]
struct LegacyStore {
    rows: HashMap<i64, String>,
}
impl LegacyStore {
    fn upsert(&mut self, id: i64, email: &str) -> Result<(), WriteError> {
        self.rows.insert(id, email.to_string());
        Ok(())
    }
}

#[derive(Default)]
struct NewStore {
    rows: HashMap<i64, String>,
    fail_next: bool,
}
impl NewStore {
    fn upsert(&mut self, id: i64, email: &str) -> Result<(), WriteError> {
        if self.fail_next {
            return Err(WriteError("new store unavailable".into()));
        }
        self.rows.insert(id, email.to_string());
        Ok(())
    }
}

/// Returns Ok as long as the SOURCE OF TRUTH (legacy) succeeded.
/// The shadow write is best-effort: log on failure, alert on drift, but don't 500.
fn dual_write(
    legacy: &mut LegacyStore,
    new: &mut NewStore,
    id: i64,
    email: &str,
) -> Result<(), WriteError> {
    legacy.upsert(id, email)?; // hard requirement

    if let Err(e) = new.upsert(id, email) {
        // In production: increment a `dual_write_shadow_failures` counter + structured log.
        eprintln!("shadow write failed for id={id}: {e:?} (request still succeeds)");
    }
    Ok(())
}

fn main() {
    let mut legacy = LegacyStore::default();
    let mut new = NewStore::default();

    dual_write(&mut legacy, &mut new, 1, "ada@example.com").unwrap();
    println!("after write 1: legacy={} new={}", legacy.rows.len(), new.rows.len());

    // New store hiccups — request must still succeed.
    new.fail_next = true;
    dual_write(&mut legacy, &mut new, 2, "alan@example.com").unwrap();
    println!("after write 2: legacy={} new={}", legacy.rows.len(), new.rows.len());
}
```

Real output:

```text
after write 1: legacy=1 new=1
shadow write failed for id=2: WriteError("new store unavailable") (request still succeeds)
after write 2: legacy=2 new=1
```

The behavior is identical to the Node version by design. During a migration, *identical behavior is the goal*. The difference is that the legacy write's `Result` cannot be accidentally ignored (an unused `Result` is a compiler warning), and the only failure path that can reach the caller is the legacy store's, which is exactly the contract we want.

> **Note:** These snippets use `HashMap` as a stand-in for a real database so they compile and run with no external services. In production each `upsert` would be a `sqlx::query!` against PostgreSQL inside a transaction — see [Transactions with SQLx](/17-database/02-sqlx-transactions/).

---

## Detailed Explanation

There are three building blocks. You usually apply them in this order over weeks or months.

### 1. Shared database

The simplest start: the Rust service connects to the **same** database the Node service already owns. No data moves. Both services read and write the same tables. This is the lowest-risk way to ship the first Rust endpoint, because the data layer does not change at all — only which process answers a given request.

The catch: the Rust types must faithfully mirror what Node persists. Node wrote the schema, so Rust is a *guest* in that schema and must not break it. The discipline is to make every Rust schema change **additive and backward-compatible** — add nullable columns, never rename or retype a column Node still reads.

```rust
use serde::{Deserialize, Serialize};

// A row as it exists in the LEGACY (Node-written) schema and the NEW (Rust) schema.
// During a shared-DB migration both services read/write the SAME table, so the
// Rust types must be a faithful mirror of what Node persists.

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct User {
    id: i64,
    email: String,
    // Node stored this as a Unix-millis number; we mirror it exactly.
    #[serde(rename = "createdAt")]
    created_at: i64,
    // Added by the Rust service. Old rows won't have it, so default it.
    #[serde(default)]
    verified: bool,
}

fn main() {
    // Simulate a row that the legacy Node service wrote (no `verified` column yet).
    let legacy_json = r#"{"id":1,"email":"ada@example.com","createdAt":1700000000000}"#;
    let user: User = serde_json::from_str(legacy_json).expect("parse legacy row");
    assert_eq!(user.verified, false); // defaulted, not crashed
    println!("parsed legacy: {user:?}");

    // What Rust now writes back — additive, so Node keeps working.
    let json = serde_json::to_string(&user).unwrap();
    println!("rust writes:   {json}");
}
```

This needs `serde` (`cargo add serde --features derive` and `cargo add serde_json`). Real output:

```text
parsed legacy: User { id: 1, email: "ada@example.com", created_at: 1700000000000, verified: false }
rust writes:   {"id":1,"email":"ada@example.com","createdAt":1700000000000,"verified":false}
```

Three things to notice:

- `#[serde(rename = "createdAt")]` maps Node's camelCase field to Rust's snake_case without changing the wire/column name.
- `created_at: i64` mirrors Node's millis-as-`number`. In JavaScript that value is an IEEE-754 `f64`; as long as it stays under 2^53 it round-trips exactly, but if it is a large 64-bit identifier you must treat precision carefully (see Pitfalls below).
- `#[serde(default)]` on `verified` means old rows that predate the column deserialize cleanly to `false` instead of erroring. That is what makes the change *backward-compatible*.

### 2. Dual-write

Once the Rust service has its *own* schema (or its own database), you can no longer share one table. The transition pattern is to write to both stores for a period: the legacy store stays the source of truth, the new store is shadowed. The `dual_write` function shown earlier is the heart of it. The rules:

1. The legacy (source-of-truth) write must succeed or the request fails.
2. The new-store write is best-effort: a failure is logged and counted, never propagated.
3. You alert on the shadow-failure rate. A steady stream of shadow failures means the two stores are diverging, and you must not cut over until that is near zero.

Dual-write only covers rows written *after* you turned it on. Everything older still lives only in the legacy store. That is what backfill is for.

### 3. Backfill

A batch job copies historical rows from the legacy store into the new one. A correct backfill is **chunked**, **resumable**, and **idempotent**:

- **Chunked**: process a page at a time (here, keyset pagination on the primary key) so you never load the whole table into memory or hold a giant transaction.
- **Resumable**: persist a checkpoint (the last processed id) after each chunk commits, so a crash resumes instead of restarting.
- **Idempotent**: each row is written with an UPSERT, so re-running over already-migrated rows is a safe no-op. This matters because backfill and live dual-write run concurrently, and you will re-run after fixing bugs.

```rust
// Idempotent, chunked, resumable backfill — the shape that matters in production.
// Key ideas: process by ascending primary key, remember the last id (checkpoint),
// and make each row's transform a no-op if already done.

#[derive(Debug, Clone)]
struct LegacyUser {
    id: i64,
    email: String,
    // legacy stored display name as "First Last" in one column
    full_name: String,
}

#[derive(Debug, PartialEq)]
struct NewUser {
    id: i64,
    email: String,
    first_name: String,
    last_name: String,
}

/// Pure transform: trivially testable, no I/O. Splits the legacy name.
fn transform(row: &LegacyUser) -> NewUser {
    let (first, last) = row.full_name.split_once(' ').unwrap_or((&row.full_name, ""));
    NewUser {
        id: row.id,
        email: row.email.clone(),
        first_name: first.to_string(),
        last_name: last.to_string(),
    }
}

/// Fetch one page of rows whose id > `after`, ordered by id (keyset pagination).
fn fetch_chunk(all: &[LegacyUser], after: i64, limit: usize) -> Vec<LegacyUser> {
    all.iter().filter(|u| u.id > after).take(limit).cloned().collect()
}

fn main() {
    let legacy = vec![
        LegacyUser { id: 1, email: "ada@x.io".into(), full_name: "Ada Lovelace".into() },
        LegacyUser { id: 2, email: "alan@x.io".into(), full_name: "Alan Turing".into() },
        LegacyUser { id: 3, email: "grace@x.io".into(), full_name: "Grace Hopper".into() },
    ];

    let mut checkpoint: i64 = 0; // resume from here if the job crashes
    let mut migrated = 0usize;
    const CHUNK: usize = 2;

    loop {
        let chunk = fetch_chunk(&legacy, checkpoint, CHUNK);
        if chunk.is_empty() {
            break;
        }
        for row in &chunk {
            let new = transform(row);
            // UPSERT into the new table here; idempotent so re-runs are safe.
            println!("backfilled id={} -> {} / {}", new.id, new.first_name, new.last_name);
            migrated += 1;
            checkpoint = row.id; // persist this after each chunk commits
        }
    }
    println!("done: {migrated} rows, checkpoint={checkpoint}");

    // Sanity: the transform is a pure function we can unit-test.
    assert_eq!(
        transform(&legacy[0]),
        NewUser { id: 1, email: "ada@x.io".into(), first_name: "Ada".into(), last_name: "Lovelace".into() }
    );
}
```

Real output:

```text
backfilled id=1 -> Ada / Lovelace
backfilled id=2 -> Alan / Turing
backfilled id=3 -> Grace / Hopper
done: 3 rows, checkpoint=3
```

The single most important design choice here is separating the **pure `transform`** from the **I/O** (`fetch_chunk` and the UPSERT). The transform has no database, no async, no error handling, so it is trivially unit-testable, and the trickiest part of any migration (the field-by-field mapping) gets full test coverage without a database. This is the same separation Rust pushes you toward everywhere; see [Common Patterns](/22-common-patterns/).

> **Tip:** Use **keyset pagination** (`WHERE id > $checkpoint ORDER BY id LIMIT n`), never `OFFSET`. With `OFFSET` the database still scans and discards all skipped rows, so a backfill gets quadratically slower as it progresses. Keyset pagination stays flat.

---

## Key Differences

| Concern | TypeScript / Node | Rust |
| --- | --- | --- |
| Row shape vs. DB schema | ORM types are often hand-maintained; drift is silent | `sqlx::query!` checks queries against the live schema at **compile time** |
| Large 64-bit ids | `number` is f64 — loses precision past 2^53 | native `i64` / `u64`, exact |
| Ignoring a failed write | easy — a forgotten `await` or unhandled rejection | unused `Result` warns; `?` makes propagation explicit |
| Transform testability | possible, but I/O often tangled in | borrow checker pushes you to isolate the pure transform |
| Migration job crashing | unhandled rejection may abort silently | `Result` + `panic = "abort"` make failure loud and deliberate |
| Concurrency in backfill | event loop; CPU-bound transforms block it | `rayon` / async tasks parallelize the transform safely |

The deepest difference is *when* you find out the new store's schema disagrees with reality. In Node, a column rename usually surfaces as a runtime error (or worse, a silently `undefined` field) in production. With `sqlx`'s compile-time-checked queries, the same mistake fails `cargo build` on your laptop. During a migration — when two schemas are evolving in parallel — that shift from runtime to compile time is worth a great deal.

> **Note:** Rust is *not* multi-threaded by default; concurrency is opt-in and the compiler enforces data-race freedom ("fearless concurrency"). For a CPU-heavy backfill transform, `rayon`'s `par_iter` parallelizes across cores safely, which a single Node event loop cannot do without worker threads.

---

## Common Pitfalls

### Pitfall 1: mirroring a numeric column as `String`

A JavaScript developer used to everything being loosely typed may declare a timestamp field as `String` because "it's just data." But Node wrote `createdAt` as a JSON **number**. Serde is strict and will refuse it at deserialize time:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct User {
    id: i64,
    email: String,
    created_at: String, // BUG: Node wrote a numeric millis timestamp, not a string
}

fn main() {
    let legacy = r#"{"id":1,"email":"ada@x.io","created_at":1700000000000}"#;
    let user: User = serde_json::from_str(legacy).unwrap(); // panics here
    println!("{user:?}");
}
```

Running it produces this **real** runtime error:

```text
thread 'main' panicked at src/main.rs:12:51:
called `Result::unwrap()` on an `Err` value: Error("invalid type: integer `1700000000000`, expected a string", line: 1, column: 53)
```

The fix is to type the field as `i64` to match what Node actually stored. Unlike TypeScript, where a wrong type annotation is erased at runtime and the mismatch slips through, serde checks the value against the declared type and tells you exactly which field is wrong.

### Pitfall 2: defaulting an auto-increment id to `i32`

JavaScript has one number type, so there is no "pick the integer width" decision. In Rust there is, and reaching for `i32` (a common default in many languages) silently caps your ids at about 2.1 billion. A mature table can exceed that. This one is caught at **compile time**:

```rust
fn main() {
    // Node stored an auto-increment id that has grown past 2^31.
    // Mirroring it as i32 (the "default int" reflex) overflows.
    let id: i32 = 3_000_000_000; // does not compile
    println!("{id}");
}
```

The real `rustc` error:

```text
error: literal out of range for `i32`
 --> src/main.rs:4:19
  |
4 |     let id: i32 = 3_000_000_000;
  |                   ^^^^^^^^^^^^^
  |
  = note: the literal `3_000_000_000` does not fit into the type `i32` whose range is `-2147483648..=2147483647`
  = help: consider using the type `u32` instead
  = note: `#[deny(overflowing_literals)]` on by default
```

Mirror PostgreSQL `BIGSERIAL`/`BIGINT` ids as `i64`. Note also that if such an id had passed through a Node service as a JavaScript `number`, any value above 2^53 would already have **lost precision** in transit — a real cross-language migration hazard, not a wrapping bug.

### Pitfall 3: treating the shadow write as load-bearing

The whole point of dual-write is that the new store is *not* yet trusted. If you propagate its errors (`new.upsert(...)?` instead of logging), a flaky new store now causes customer-facing 500s — you have made the migration *reduce* reliability. Keep the shadow write best-effort and gate the cutover on the measured drift rate instead.

### Pitfall 4: forgetting backfill and dual-write race on the same row

Backfill reads an old row, transforms it, and writes it. Meanwhile a live dual-write may have *already* written a newer version of that same row. If backfill blindly overwrites, it clobbers fresh data with stale data. Defend against this by making the new-store write a conditional UPSERT (`ON CONFLICT ... DO UPDATE WHERE excluded.updated_at > target.updated_at`) so newer data always wins. This is impossible to test by eyeballing it; make the transform pure and unit-test the conflict resolution.

---

## Best Practices

- **Make schema changes additive during shared-DB.** Add nullable columns; never rename or retype a column the other service still reads. Use `#[serde(default)]` so old rows deserialize cleanly.
- **Mirror integer widths exactly.** PostgreSQL `BIGINT` → `i64`, `INT` → `i32`, and check whether a column is signed before choosing `u*`. When in doubt, prefer `i64`.
- **Separate the pure transform from I/O** so the row-mapping logic gets full unit-test coverage without a database, as in the backfill example.
- **Use keyset pagination and a persisted checkpoint** for backfills. Resumability is not optional at scale.
- **Make every write idempotent (UPSERT).** Backfill will be re-run, and it overlaps live dual-write.
- **Measure drift continuously** with a reconciliation job (next section) and alert on it. Cut over only when drift is near zero.
- **Let `sqlx`'s compile-time query checks guard the new schema.** Run `cargo sqlx prepare` in CI so a schema/query mismatch fails the build, not production. See [SQLx](/17-database/00-sqlx-intro/).
- **Wrap multi-table writes in a transaction** so a partial dual-write to the new store cannot leave it half-updated. See [Transactions with SQLx](/17-database/02-sqlx-transactions/).
- **Keep a rollback path.** Until the legacy store is retired, you can always fall back to it. Do not delete legacy data until the new store has been the source of truth, verified, for a full business cycle.

---

## Real-World Example

Before flipping the read path to the new store, you run a **reconciliation pass**: sample (or fully scan) both stores, fingerprint each row's canonical form, and report rows that are missing or divergent. The cutover is *gated* on this report being clean: you cut over on evidence, not optimism.

Fingerprinting via a hash lets you compare rows cheaply and detect drift even when the two stores format a value differently (trailing whitespace, casing). This uses `sha2` (`cargo add sha2`):

```rust
// A reconciliation pass run AFTER backfill, BEFORE flipping the read path.
// It scans both stores and reports drift, so you gate the cutover on real
// evidence instead of hope.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone)]
struct Row {
    id: i64,
    email: String,
    verified: bool,
}

/// Hash the CANONICAL form so the same logical row hashes identically across
/// stores even if they format the value differently (case, whitespace).
fn fingerprint(r: &Row) -> u64 {
    let canonical = format!("{}|{}|{}", r.id, r.email.trim().to_lowercase(), r.verified);
    let digest = Sha256::digest(canonical.as_bytes());
    u64::from_be_bytes(digest[..8].try_into().unwrap())
}

struct ReconcileReport {
    checked: usize,
    missing_in_new: Vec<i64>,
    mismatched: Vec<i64>,
}

fn reconcile(legacy: &BTreeMap<i64, Row>, new: &BTreeMap<i64, Row>) -> ReconcileReport {
    let mut report = ReconcileReport { checked: 0, missing_in_new: vec![], mismatched: vec![] };
    for (id, lrow) in legacy {
        report.checked += 1;
        match new.get(id) {
            None => report.missing_in_new.push(*id),
            Some(nrow) if fingerprint(lrow) != fingerprint(nrow) => report.mismatched.push(*id),
            Some(_) => {}
        }
    }
    report
}

fn main() {
    let mut legacy = BTreeMap::new();
    let mut new = BTreeMap::new();
    for (id, email, v) in [(1, "ada@x.io", true), (2, "alan@x.io", false), (3, "grace@x.io", true)] {
        legacy.insert(id, Row { id, email: email.into(), verified: v });
    }
    // New store is missing id=3 and has stale `verified` for id=2.
    new.insert(1, Row { id: 1, email: "Ada@x.io ".into(), verified: true }); // formatting differs, same logical value
    new.insert(2, Row { id: 2, email: "alan@x.io".into(), verified: true });  // drifted

    let report = reconcile(&legacy, &new);
    println!("checked: {}", report.checked);
    println!("missing in new: {:?}", report.missing_in_new);
    println!("mismatched:     {:?}", report.mismatched);
    let healthy = report.missing_in_new.is_empty() && report.mismatched.is_empty();
    println!("safe to cut over? {healthy}");
}
```

Real output:

```text
checked: 3
missing in new: [3]
mismatched:     [2]
safe to cut over? false
```

Row 1 differs only in casing and whitespace, and the canonical fingerprint correctly treats it as in-sync. Row 2 has genuinely drifted (the new store has stale `verified`), and row 3 was never backfilled; both are flagged, and the gate refuses the cutover. In production this job runs on a schedule, exports the mismatched ids for repair, and emits a metric so the cutover decision is data-driven. For honestly measuring the *performance* payoff once you have cut over, see [Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/).

---

## Further Reading

- [The Rust `serde` data model](https://serde.rs/data-model.html): how types map to and from the wire/columns.
- [`sqlx` compile-time checked queries](https://docs.rs/sqlx): catching schema drift at build time.
- [PostgreSQL `INSERT ... ON CONFLICT`](https://www.postgresql.org/docs/current/sql-insert.html#SQL-ON-CONFLICT): idempotent UPSERTs for backfill and dual-write.
- Section guides used here: [SQLx](/17-database/00-sqlx-intro/), [Transactions with SQLx](/17-database/02-sqlx-transactions/), [Database Migrations](/17-database/09-migrations/), [Connection Pooling](/17-database/08-connection-pooling/).
- Sibling pages in this section: [Incremental Migration](/29-migration-guide/00-incremental/), [Porting a Node.js Service to Rust](/29-migration-guide/01-node-to-rust/), [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/), [Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/), [Common Migration Challenges](/29-migration-guide/05-common-challenges/).
- Foundations if any syntax here is unfamiliar: [Introduction](/00-introduction/), [Getting Started](/01-getting-started/), [Basics](/02-basics/).
- Apply this end to end in the capstones: [Projects](/30-projects/).

---

## Exercises

### Exercise 1: read-path migration with fallback

**Difficulty:** Beginner

**Objective:** Implement the read side of a migration. Reads should prefer the new store, and on a miss fall back to the legacy store while lazily copying the row forward (a "read-through" cache pattern applied to migration).

**Instructions:** Write a function `read_through(new, legacy, id)` that returns the row from `new` if present; otherwise reads from `legacy`, writes it into `new`, and returns it; otherwise returns `None`. Use `Option` combinators, not nested `if let` where avoidable.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

#[derive(Default)]
struct Store(HashMap<i64, String>);
impl Store {
    fn get(&self, id: i64) -> Option<&String> { self.0.get(&id) }
    fn put(&mut self, id: i64, v: String) { self.0.insert(id, v); }
}

/// Read-through: new store first; on miss, read legacy and write it forward.
fn read_through(new: &mut Store, legacy: &Store, id: i64) -> Option<String> {
    if let Some(v) = new.get(id) {
        return Some(v.clone());
    }
    let v = legacy.get(id)?.clone(); // `?` short-circuits to None if absent everywhere
    new.put(id, v.clone()); // lazy backfill on read
    Some(v)
}

fn main() {
    let mut new = Store::default();
    let mut legacy = Store::default();
    legacy.put(1, "ada@x.io".into());

    // First read: miss in new, hit in legacy, backfilled.
    let first = read_through(&mut new, &legacy, 1);
    println!("first read: {first:?}, new now has it: {}", new.get(1).is_some());

    // Missing everywhere.
    println!("unknown: {:?}", read_through(&mut new, &legacy, 99));
}
```

Real output:

```text
first read: Some("ada@x.io"), new now has it: true
unknown: None
```

The `?` operator on `legacy.get(id)?` returns `None` from the whole function if the row is absent everywhere — the same short-circuit you would write with `??` and early-return in TypeScript, but type-checked end to end.

</details>

### Exercise 2: model the cutover as a state machine

**Difficulty:** Intermediate

**Objective:** Encode the migration phases as an `enum` so an illegal phase transition is unrepresentable, and expose a helper that reports whether the legacy store is still being written in a given phase.

**Instructions:** Define a `Phase` enum with variants `LegacyOnly`, `DualWrite`, `Backfilling`, `NewPrimary`, `NewOnly`. Add `next(self) -> Option<Phase>` that only advances forward (and returns `None` at the end), and `writes_legacy(self) -> bool` that is `true` for every phase except `NewOnly`. Drive it through all phases in `main`.

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    LegacyOnly,   // only Node writes
    DualWrite,    // both written, legacy is source of truth
    Backfilling,  // historical rows being copied
    NewPrimary,   // Rust is source of truth, legacy shadowed
    NewOnly,      // legacy retired
}

impl Phase {
    /// Only forward transitions are legal; you never skip dual-write.
    fn next(self) -> Option<Phase> {
        use Phase::*;
        match self {
            LegacyOnly => Some(DualWrite),
            DualWrite => Some(Backfilling),
            Backfilling => Some(NewPrimary),
            NewPrimary => Some(NewOnly),
            NewOnly => None,
        }
    }
    fn writes_legacy(self) -> bool {
        !matches!(self, Phase::NewOnly)
    }
}

fn main() {
    let mut phase = Phase::LegacyOnly;
    print!("{phase:?}");
    while let Some(next) = phase.next() {
        print!(" -> {next:?}");
        phase = next;
    }
    println!();
    assert!(Phase::DualWrite.writes_legacy());
    assert!(!Phase::NewOnly.writes_legacy());
    println!("final phase still writes legacy? {}", phase.writes_legacy());
}
```

Real output:

```text
LegacyOnly -> DualWrite -> Backfilling -> NewPrimary -> NewOnly
final phase still writes legacy? false
```

Because `next` is the only way to advance and `match` is exhaustive, the compiler guarantees you handle every phase and that the sequence can only move forward. A TypeScript union of string literals gives you the *shape* but not the exhaustiveness guarantee without extra discipline.

</details>

### Exercise 3: idempotent UPSERT with conflict resolution

**Difficulty:** Advanced

**Objective:** Implement the "newer data wins" rule that prevents a backfill from clobbering a fresher dual-write (Pitfall 4).

**Instructions:** Model a row as `{ id: i64, email: String, updated_at: i64 }`. Write `upsert(store, incoming)` that inserts the row only if there is no existing row for that id, or if `incoming.updated_at` is strictly greater than the stored row's. Return a `bool` indicating whether the write was applied. Demonstrate that a stale backfill row does **not** overwrite a fresher live row.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Row {
    id: i64,
    email: String,
    updated_at: i64,
}

/// Idempotent UPSERT with "newer wins" conflict resolution.
/// Returns true if the store was changed.
fn upsert(store: &mut HashMap<i64, Row>, incoming: Row) -> bool {
    match store.get(&incoming.id) {
        Some(existing) if existing.updated_at >= incoming.updated_at => false, // keep fresher data
        _ => {
            store.insert(incoming.id, incoming);
            true
        }
    }
}

fn main() {
    let mut store: HashMap<i64, Row> = HashMap::new();

    // Live dual-write lands a fresh row.
    let applied = upsert(&mut store, Row { id: 1, email: "new@x.io".into(), updated_at: 200 });
    println!("fresh write applied: {applied}");

    // Backfill replays an OLDER snapshot of the same row — must be rejected.
    let applied = upsert(&mut store, Row { id: 1, email: "stale@x.io".into(), updated_at: 100 });
    println!("stale backfill applied: {applied}");
    println!("stored email: {}", store[&1].email);

    // Re-running the exact same fresh write is a no-op (idempotent).
    let applied = upsert(&mut store, Row { id: 1, email: "new@x.io".into(), updated_at: 200 });
    println!("replay applied: {applied}");
}
```

Real output:

```text
fresh write applied: true
stale backfill applied: false
stored email: new@x.io
replay applied: false
```

The stale backfill (`updated_at: 100`) is rejected even though it arrived later in wall-clock time, and a replay of the same fresh row is a no-op — exactly the idempotency and conflict-resolution guarantees a concurrent backfill needs. In SQL this is `INSERT ... ON CONFLICT (id) DO UPDATE SET ... WHERE excluded.updated_at > <table>.updated_at`.

</details>
