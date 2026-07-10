---
title: "SQLx vs Diesel vs SeaORM: Choosing a Database Layer"
description: "Compare SQLx, Diesel, and SeaORM against Knex, TypeORM, and Prisma: compile-checked raw SQL versus typed query DSL versus async ORM, and when to pick each in Rust."
---

By now you have a database in Node and three or four ways to talk to it: a raw driver (`pg`), a query builder (Knex), and an ORM or two (Prisma, TypeORM, Sequelize). Rust's database story has the same shape, but the three leading choices stake out genuinely different positions on a single axis: **how much does the compiler check, and how much does it write for you?** This page compares **SQLx**, **Diesel**, and **SeaORM** side by side so you can pick deliberately instead of by reflex.

---

## Quick Overview

The three crates answer "what is the right abstraction for SQL?" differently:

- **SQLx**: you write **raw SQL strings**, and (with its macros) the compiler checks them against your real database schema at build time. Async. Not an ORM.
- **Diesel**: a **synchronous ORM and typed query builder**. You build queries through a strongly typed DSL that the compiler validates; there is no async by default.
- **SeaORM**: an **async, ActiveRecord-style ORM** built on top of SQLx. It gives you entities, an `ActiveModel` write pattern, relations, and a dynamic query builder, with `async`/`await` throughout. (Deep dive: [SeaORM](/17-database/11-sea-orm/).)

For a TypeScript developer the rough map is: **SQLx is "Knex, but the compiler checks your SQL"**, **Diesel is "TypeORM, but synchronous and statically typed end-to-end"**, and **SeaORM is "TypeORM/Prisma ergonomics that stay async."** This page builds a shared example across all three and gives you a decision framework.

> **Note:** Every Rust snippet here was compiled and run with `cargo` 1.96.0 (the recorded verification toolchain; 2024 edition). Versions used: **SQLx 0.8**, **Diesel 2.3**, **SeaORM 1.1** (current at the time of writing). All examples use **SQLite** (in-memory or bundled) so they reproduce with no database server installed. The deep dives for each crate live in the sibling pages linked throughout; this page is purely the comparison.

---

## TypeScript/JavaScript Example

In Node you already make this same choice. Here are the three positions you are choosing between, in TypeScript:

```typescript
// 1) Raw-ish SQL with a query builder (Knex) — flexible, unchecked.
import knex from "knex";
const db = knex({ client: "pg", connection: process.env.DATABASE_URL });

const activeKnex = await db.raw(
  "SELECT id, name, email, active FROM users WHERE active = ? ORDER BY id",
  [true],
); // rows: any[] — the shape is a promise to yourself

// 2) An ORM with decorated entities (TypeORM) — ergonomic, runtime-checked.
import { DataSource } from "typeorm";
const ds = new DataSource({ type: "postgres", url: process.env.DATABASE_URL, entities: [User] });
await ds.initialize();
const activeTypeOrm = await ds.getRepository(User).findBy({ active: true });

// 3) A schema-first ORM (Prisma) — generated client, partial type-safety.
import { PrismaClient } from "@prisma/client";
const prisma = new PrismaClient();
const activePrisma = await prisma.user.findMany({
  where: { active: true },
  orderBy: { id: "asc" },
});
```

All three are **async** (everything returns a `Promise`), and the safety they give you ranges from *none* (the Knex `raw` cast) to *partial* (Prisma generates types from `schema.prisma`, but a hand-written `$queryRaw` is unchecked again). None of them validates your SQL against the actual database **before the program runs**. That last property is exactly what Rust's options compete on.

---

## Rust Equivalent

The same query (*find active users, ordered by id*) written three ways. Each is a complete, compile-verified program against an in-memory SQLite database.

### SQLx: raw SQL, typed result

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::FromRow;

#[derive(Debug, FromRow)]
struct User {
    id: i64,
    name: String,
    email: String,
    active: bool,
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, \
         email TEXT NOT NULL, active BOOLEAN NOT NULL)",
    )
    .execute(&pool)
    .await?;
    for (n, e, a) in [("Ada", "ada@x.com", true), ("Alan", "alan@x.com", false)] {
        sqlx::query("INSERT INTO users (name, email, active) VALUES (?, ?, ?)")
            .bind(n)
            .bind(e)
            .bind(a)
            .execute(&pool)
            .await?;
    }

    // The query is a SQL STRING. With the query!/query_as! macros it is also
    // checked against the live schema at compile time (see sqlx-queries.md).
    let active: Vec<User> = sqlx::query_as::<_, User>(
        "SELECT id, name, email, active FROM users WHERE active = ? ORDER BY id",
    )
    .bind(true)
    .fetch_all(&pool)
    .await?;

    println!(
        "{} active: {:?}",
        active.len(),
        active.iter().map(|u| &u.name).collect::<Vec<_>>()
    );
    Ok(())
}
```

Real output:

```text
1 active: ["Ada"]
```

### Diesel: synchronous typed DSL

```toml
# Cargo.toml
[dependencies]
diesel = { version = "2.3", features = ["sqlite", "returning_clauses_for_sqlite_3_35"] }
libsqlite3-sys = { version = "0.37", features = ["bundled"] }
```

```rust
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

// Normally generated by `diesel print-schema` into src/schema.rs.
diesel::table! {
    users (id) { id -> Integer, name -> Text, email -> Text, active -> Bool, }
}

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct User {
    id: i32,
    name: String,
    email: String,
    active: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = SqliteConnection::establish(":memory:")?;
    diesel::sql_query(
        "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, \
         email TEXT NOT NULL, active BOOLEAN NOT NULL)",
    )
    .execute(&mut conn)?;
    diesel::sql_query(
        "INSERT INTO users (name, email, active) \
         VALUES ('Ada','ada@x.com',1),('Alan','alan@x.com',0)",
    )
    .execute(&mut conn)?;

    // No SQL string: the query is built from typed column objects. A typo
    // like `users::activ` simply does not compile.
    let active: Vec<User> = users::table
        .filter(users::active.eq(true))
        .order(users::id.asc())
        .select(User::as_select())
        .load(&mut conn)?; // blocking — no .await

    println!(
        "{} active: {:?}",
        active.len(),
        active.iter().map(|u| &u.name).collect::<Vec<_>>()
    );
    Ok(())
}
```

Real output:

```text
1 active: ["Ada"]
```

### SeaORM: async ActiveRecord-style entities

```toml
# Cargo.toml
[dependencies]
sea-orm = { version = "1.1", features = ["sqlx-sqlite", "runtime-tokio-rustls", "macros"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, QueryOrder, Schema, Set};

// An ENTITY — the SeaORM analogue of a TypeORM @Entity class.
// Normally generated by `sea-orm-cli generate entity` from your live schema.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    #[sea_orm(unique)]
    pub email: String,
    pub active: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[tokio::main]
async fn main() -> Result<(), DbErr> {
    let db = Database::connect("sqlite::memory:").await?;

    // Create the schema from the entity (dev convenience, like synchronize: true).
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);
    db.execute(builder.build(&schema.create_table_from_entity(Entity)))
        .await?;

    // INSERT via an ActiveModel: set the fields you want, leave the PK NotSet.
    for (n, e, a) in [("Ada", "ada@x.com", true), ("Alan", "alan@x.com", false)] {
        ActiveModel {
            name: Set(n.into()),
            email: Set(e.into()),
            active: Set(a),
            ..Default::default()
        }
        .insert(&db)
        .await?;
    }

    // A dynamic query builder, like TypeORM's QueryBuilder — but async.
    let active: Vec<Model> = Entity::find()
        .filter(Column::Active.eq(true))
        .order_by_asc(Column::Id)
        .all(&db)
        .await?;

    println!(
        "{} active: {:?}",
        active.len(),
        active.iter().map(|u| &u.name).collect::<Vec<_>>()
    );
    Ok(())
}
```

Real output:

```text
1 active: ["Ada"]
```

Three crates, identical result. The differences are not in *what they can do* (all three run any query you need) but in **where the checking happens**, **whether you write SQL or a DSL**, and **whether the API is async**.

---

## Detailed Explanation

### The core axis: SQL vs DSL, and where validation happens

Read the three programs again with one question in mind: *where is your query, and who checks it?*

- In **SQLx**, the query is a SQL string. The plain `sqlx::query(...)` form is unchecked until it runs (like Knex `raw`). The `sqlx::query!`/`query_as!` macros, covered in [Writing Queries with SQLx](/17-database/01-sqlx-queries/), connect to your real database **during `cargo build`** and validate the SQL, its parameters, and the result types against the actual schema. So a typo is a *compile* error, but the SQL itself is still SQL you wrote.
- In **Diesel**, there is no SQL string in your code at all. `users::active` is a distinct Rust type carrying the column's SQL type, and `.filter(...)`, `.order(...)`, `.select(...)` build a query from those types. The compiler validates the whole thing structurally: comparing a `Text` column to an integer, or loading a column into the wrong field type, has no valid trait implementation and the program does not build. The schema lives in a generated `schema.rs` (see [Getting Started with Diesel (the TypeORM of Rust)](/17-database/03-diesel-intro/)).
- In **SeaORM**, you also avoid raw SQL via a DSL (`Entity::find().filter(Column::Active.eq(true))`), but the building blocks are runtime values (a `Column` enum, dynamic `Condition`s) rather than Diesel's compile-time type machinery. SeaORM checks that you reference a real `Column` variant (a typo is a compile error), but it does **not** validate the full query against a live schema the way SQLx's macros do.

A compact way to hold it: **SQLx checks your SQL against the database; Diesel checks a typed query against the schema's types; SeaORM checks that you used real columns and gives you ORM ergonomics on top.**

### Async vs synchronous

This is the second decisive difference, and it follows the broader Rust async story from [Section 11](/11-async/):

- **SQLx and SeaORM are async.** Every call returns a `Future` you `.await`, and you need a runtime (Tokio) to drive it. This is the natural fit for an async web server like Axum (see [Section 16](/16-web-apis/)), where a handler is already `async`.
- **Diesel is synchronous.** `establish`, `load`, and `get_result` are blocking calls that return a plain `Result`: no `.await`, no runtime needed for a script. Inside an async server, a blocking Diesel call would stall a runtime worker thread, so you run it on a blocking pool (`tokio::task::spawn_blocking` + an `r2d2` pool) or reach for the separate `diesel-async` crate. Details in [Connection Pooling](/17-database/08-connection-pooling/).

> **Tip:** "Async" is not automatically "faster." Diesel's synchronous model is often the simplest and fastest choice for a CLI, a batch job, or a worker that is not already inside an async runtime. The cost of async is real (a runtime, `.await` everywhere); pay it when you are already async, not as a reflex.

### The write side: `INSERT`/`UPDATE` ergonomics

The three crates differ most visibly when writing data:

- **SQLx**: you write the `INSERT`/`UPDATE` SQL yourself and bind parameters. Maximum control, zero magic.
- **Diesel**: you build an `Insertable` struct (a separate write model, no `id`) and call `insert_into(table).values(&new)`. Updates use `AsChangeset`. Covered in [Diesel Query Builder](/17-database/04-diesel-queries/).
- **SeaORM**: the **ActiveModel** pattern. Every entity gets a generated `ActiveModel` whose fields are `ActiveValue<T>`: each field is `Set(value)`, `NotSet`, or `Unchanged`. You build one, set the fields you want, and call `.insert()` or `.update()`. SeaORM tracks which fields changed, so an update only writes the columns you touched — the closest analogue to TypeORM's `repo.save(partialEntity)`.

Here is the SeaORM ActiveModel write pattern in action: load a row, mutate it, and save only the changed column.

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, PaginatorTrait, Schema, Set};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    #[sea_orm(unique)]
    pub email: String,
    pub active: bool,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

#[tokio::main]
async fn main() -> Result<(), DbErr> {
    let db = Database::connect("sqlite::memory:").await?;
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);
    db.execute(builder.build(&schema.create_table_from_entity(Entity)))
        .await?;

    for (n, e, a) in [("Ada", "ada@x.com", true), ("Alan", "alan@x.com", false)] {
        ActiveModel {
            name: Set(n.into()),
            email: Set(e.into()),
            active: Set(a),
            ..Default::default()
        }
        .insert(&db)
        .await?;
    }

    // UPDATE: load the Model, turn it into an ActiveModel, change one field, save.
    let alan = Entity::find_by_id(2).one(&db).await?.unwrap();
    let mut alan: ActiveModel = alan.into();
    alan.active = Set(true); // only this column will be written
    let alan: Model = alan.update(&db).await?;
    println!("updated: {} active={}", alan.name, alan.active);

    let n = Entity::find().filter(Column::Active.eq(true)).count(&db).await?;
    println!("active count = {n}");

    let res = Entity::delete_by_id(1).exec(&db).await?;
    println!("deleted {} row(s)", res.rows_affected);
    Ok(())
}
```

Real output:

```text
updated: Alan active=true
active count = 2
deleted 1 row(s)
```

### Code generation: who writes the boilerplate?

A practical concern: how much do you type by hand?

- **SQLx** generates nothing persistent. (`cargo sqlx prepare` caches *query metadata* for offline builds, see [Database Migrations](/17-database/09-migrations/), but you write all the structs and SQL.)
- **Diesel** generates `schema.rs` (the `table!` macros) from your migrations via the `diesel` CLI. You hand-write the model structs.
- **SeaORM** can generate **entire entity files** (the `Model`, `Column`, `Relation`, and `ActiveModel`) from a live database with `sea-orm-cli generate entity`. This is the most "Prisma-like" experience: point it at a database and get typed code out.

### Escape hatches: every option lets you drop to raw SQL

None of these locks you out of raw SQL when the DSL cannot express something (recursive CTEs, database-specific functions, hand-tuned queries). SQLx is *already* raw SQL. Diesel has `diesel::sql_query`. SeaORM has `Statement::from_sql_and_values` plus `FromQueryResult` to map the result into a custom struct:

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, DbBackend, FromQueryResult,
              Schema, Set, Statement};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub active: bool,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

// A custom projection for the raw-SQL escape hatch.
#[derive(Debug, FromQueryResult)]
struct NameOnly {
    name: String,
}

#[tokio::main]
async fn main() -> Result<(), DbErr> {
    let db = Database::connect("sqlite::memory:").await?;
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);
    db.execute(builder.build(&schema.create_table_from_entity(Entity)))
        .await?;
    for (n, a) in [("Ada", true), ("Alan", false), ("Grace", true)] {
        ActiveModel { name: Set(n.into()), active: Set(a), ..Default::default() }
            .insert(&db)
            .await?;
    }

    // Drop to raw SQL when the DSL is not enough; bound params still prevent injection.
    let names: Vec<NameOnly> = NameOnly::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "SELECT name FROM users WHERE active = ? ORDER BY name DESC",
        [true.into()],
    ))
    .all(&db)
    .await?;

    println!("raw: {:?}", names.iter().map(|n| &n.name).collect::<Vec<_>>());
    Ok(())
}
```

Real output:

```text
raw: ["Grace", "Ada"]
```

The lesson: choosing an ORM does not trap you. You can use SeaORM or Diesel for 95% of queries and drop to raw SQL for the rest, the same way you mix Prisma's typed client with `$queryRaw` in Node.

---

## Key Differences

| Aspect | SQLx | Diesel | SeaORM |
| --- | --- | --- | --- |
| Category | SQL toolkit (not an ORM) | ORM + typed query builder | Async ActiveRecord ORM |
| You write | raw SQL strings | a typed DSL (no SQL) | a typed DSL + ActiveModels |
| Async? | **Yes** (Tokio) | **No** (synchronous) | **Yes** (Tokio; built on SQLx) |
| Query validation | SQL checked vs live DB at **compile time** (macros) | **compile-time** structural type-checking | column names checked at compile time; query not validated vs live schema |
| Needs DB to compile? | **Yes**, for the `query!` macros (or offline cache) | No | No |
| Write pattern | hand-written SQL | `Insertable`/`AsChangeset` structs | `ActiveModel` (`Set`/`NotSet`) |
| Relations / eager loading | none (write the `JOIN`) | `belongs_to`/`has_many` ([Diesel Relations](/17-database/05-diesel-relations/)) | `related`/`find_also_related` |
| Code generation | none (query cache only) | `schema.rs` from migrations | full entity files from a live DB |
| Closest TS analogue | Knex `raw`, but compiler-checked | TypeORM/Prisma, but sync + fully typed | TypeORM/Prisma, staying async |
| Maturity | very mature, widely used | the oldest, most battle-tested | newer, rapidly growing |

### Why these designs differ

SQLx's bet is that **SQL is already the right abstraction** and the compiler should check it rather than replace it; you keep full SQL expressiveness and lose nothing to a DSL. Diesel's bet is that **the type system can model SQL itself**, so an entire class of query bugs becomes unrepresentable — at the cost of a steeper learning curve and a DSL that occasionally fights you on complex queries. SeaORM's bet is that **async ORM ergonomics** (entities, ActiveModels, relations, generated code) matter most to teams coming from Prisma/TypeORM, so it builds those on top of SQLx's solid async foundation, trading Diesel's deepest compile-time guarantees for a gentler, more familiar API.

---

## Common Pitfalls

### Pitfall 1: Expecting SeaORM/SQLx to be synchronous like a quick script, or Diesel to be async

Diesel calls block and return a `Result` directly; writing `.await` on one is a compile error because the value is not a `Future`. Conversely, SQLx and SeaORM calls return `Future`s that do nothing until `.await`ed and driven by a runtime (Rust futures are **lazy**, the opposite of eager JS Promises — see [Promises vs Futures](/11-async/00-promises-vs-futures/)). Mixing the models up is the single most common early mistake. Decide up front whether your program is async.

### Pitfall 2: Misreading what "compile-time checked" means for each crate

All three give *some* compile-time safety, but it is not the same safety:

- **Diesel** and **SeaORM** catch a misspelled *column* at compile time because columns are Rust items. In SeaORM, `Column::Activ` for an `active` field is a real `rustc` error:

```text
error[E0599]: no variant or associated item named `Activ` found for enum `Column` in the current scope
  --> src/main.rs:20:43
   |
 4 | #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
   |                                   ----------------- variant or associated item `Activ` not found for this enum
...
20 |     let _ = Entity::find().filter(Column::Activ.eq(true)).all(&db).await?;
   |                                           ^^^^^ variant or associated item not found in `Column`
   |
help: there is a variant with a similar name
   |
20 |     let _ = Entity::find().filter(Column::Active.eq(true)).all(&db).await?;
   |                                                +
```

- But **only SQLx's `query!` macros** check that the *SQL itself* matches the live database: that a column exists in the table, that a `JOIN` is valid, that a returned column is non-null. SeaORM does **not** phone the database at compile time. So "SeaORM is compile-time checked" is true for the DSL surface but does not give you SQLx's schema-level guarantee.

The plain `sqlx::query("...")` form (without `!`) is **not** checked at all until it runs; it is exactly as unsafe as Knex `raw`. The compile-time guarantee is the macro, not the crate.

### Pitfall 3: The `query!` macro needs a database at build time — surprising in CI/Docker

Because SQLx's macros validate against a live database during `cargo build`, a build with no reachable database fails unless you committed an offline query cache (`cargo sqlx prepare` → `.sqlx/`). This bites CI pipelines and `docker build` constantly. Diesel and SeaORM do **not** have this requirement; they compile without any database. If "must reach the DB to compile" is unacceptable for your build environment and you do not want to manage the offline cache, that argues for Diesel/SeaORM (or for SQLx's non-macro API).

### Pitfall 4: Reaching for an ORM out of habit when SQLx would be simpler

TypeScript developers often default to "I need an ORM." In Rust, if your access patterns are mostly straightforward queries, **SQLx with the `query!` macros frequently beats a full ORM**: you get compile-time-checked SQL, the smallest dependency footprint, and no DSL to learn — just SQL you already know. Reach for Diesel or SeaORM when you genuinely want entity mapping, relations/associations, change-tracking on updates, or generated code, not reflexively.

### Pitfall 5: Assuming relations work the same as in TypeORM/Prisma

Prisma's `include` and TypeORM's `relations` make eager loading feel free. In Rust, **SQLx has no relation concept at all** (you write the `JOIN` and map it yourself), **Diesel** models associations explicitly with `belongs_to`/`has_many` and a `belonging_to` load step ([Diesel Relations](/17-database/05-diesel-relations/)), and **SeaORM** has `related()`/`find_also_related()`. If rich relation loading is central to your app, weigh SeaORM or Diesel over SQLx, and read their relation pages before committing.

---

## Best Practices

- **Pick async vs sync first.** Already inside an async server (Axum, etc.)? Choose SQLx or SeaORM. Writing a CLI, a migration tool, or a batch job? Diesel's synchronous model is often the simplest. This decision narrows the field before any other.
- **Default to SQLx + `query!` for query-shaped workloads.** Compile-time-checked SQL, minimal dependencies, no DSL to learn. It is the most common production choice and the easiest for a SQL-fluent team.
- **Choose Diesel for the strongest static guarantees and a mature ecosystem,** especially in synchronous services and when you want column/type mismatches to be flatly impossible to compile. Accept the DSL learning curve and the blocking model.
- **Choose SeaORM when you want Prisma/TypeORM-style ergonomics while staying async:** entities, `ActiveModel` change-tracking, relations, and `sea-orm-cli generate entity` to scaffold from an existing database.
- **Do not over-pick.** Many teams use SQLx everywhere and reach for raw SQL when needed; that is a perfectly good endpoint. Mixing crates in one binary is possible but rarely worth the cognitive cost — standardize on one.
- **Whatever you choose, share one connection pool, cloned everywhere.** All three expose a cheap clonable handle (SQLx `Pool`, SeaORM `DatabaseConnection`, Diesel via `r2d2`). Build it once at startup. See [Connection Pooling](/17-database/08-connection-pooling/).
- **Keep the escape hatch in mind.** No choice locks you out of raw SQL; use the DSL for the common path and drop down for the hard 5%.

---

## Real-World Example

A `UserRepo` in SeaORM, the kind of repository layer you would put behind a web handler, showing entities with a **typed enum column** (a SeaORM strength that maps a Rust enum to a database value), the `ActiveModel` write path, and typed queries. It is fully self-contained on SQLite.

```toml
# Cargo.toml
[dependencies]
sea-orm = { version = "1.1", features = ["sqlx-sqlite", "runtime-tokio-rustls", "macros"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, DbErr,
              QueryOrder, Schema, Set};

// A typed enum column: SeaORM maps this Rust enum to a SQL string value.
#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum Role {
    #[sea_orm(string_value = "admin")]
    Admin,
    #[sea_orm(string_value = "member")]
    Member,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    #[sea_orm(unique)]
    pub email: String,
    pub role: Role,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

// A repository over a clonable connection handle (DatabaseConnection is an Arc inside).
#[derive(Clone)]
struct UserRepo {
    db: DatabaseConnection,
}

impl UserRepo {
    async fn create(&self, name: &str, email: &str, role: Role) -> Result<Model, DbErr> {
        ActiveModel {
            name: Set(name.to_owned()),
            email: Set(email.to_owned()),
            role: Set(role),
            ..Default::default()
        }
        .insert(&self.db)
        .await
    }

    // Returns Option<Model>: None (not an error) when nothing matches.
    async fn find_by_email(&self, email: &str) -> Result<Option<Model>, DbErr> {
        Entity::find().filter(Column::Email.eq(email)).one(&self.db).await
    }

    async fn admins(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::Role.eq(Role::Admin)) // compares against the typed enum
            .order_by_asc(Column::Id)
            .all(&self.db)
            .await
    }
}

#[tokio::main]
async fn main() -> Result<(), DbErr> {
    let db = Database::connect("sqlite::memory:").await?;
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);
    db.execute(builder.build(&schema.create_table_from_entity(Entity)))
        .await?;

    let repo = UserRepo { db };
    repo.create("Ada", "ada@x.com", Role::Admin).await?;
    repo.create("Alan", "alan@x.com", Role::Member).await?;

    if let Some(u) = repo.find_by_email("ada@x.com").await? {
        println!("found {} with role {:?}", u.name, u.role);
    }
    let admins = repo.admins().await?;
    println!(
        "admins: {:?}",
        admins.iter().map(|u| &u.name).collect::<Vec<_>>()
    );
    Ok(())
}
```

Real output:

```text
found Ada with role Admin
admins: ["Ada"]
```

The repository is `Clone` because `DatabaseConnection` is a cheap `Arc` handle, exactly like an SQLx `Pool`. You would build the connection once in `main`, store the `UserRepo` in your application state, and clone it into every handler, the same shared-state pattern used across the web frameworks in [Section 16](/16-web-apis/). The typed `Role` enum is the kind of ergonomic that pulls teams toward SeaORM: in SQLx you would map that column to a `String` and convert by hand, or write a custom `FromRow`.

---

## Further Reading

- [SQLx documentation (docs.rs)](https://docs.rs/sqlx) and [repository](https://github.com/launchbadge/sqlx) — features, the `query!` macros, offline mode.
- [Diesel guides](https://diesel.rs/guides/getting-started) and [API docs (docs.rs)](https://docs.rs/diesel): the `table!` macro, `Queryable`/`Selectable`, the query DSL.
- [SeaORM documentation (docs.rs)](https://docs.rs/sea-orm) and [the SeaORM book](https://www.sea-ql.org/SeaORM/) — entities, `ActiveModel`, relations, and `sea-orm-cli`.
- Sibling topics in this section:
  - [SQLx intro](/17-database/00-sqlx-intro/) — async, compile-time-checked SQL: setup and connecting.
  - [SQLx queries](/17-database/01-sqlx-queries/) — `query!`/`query_as!`, parameter binding (and how it prevents injection), `FromRow`.
  - [SQLx transactions](/17-database/02-sqlx-transactions/): `begin`/`commit`/`rollback`.
  - [Diesel intro](/17-database/03-diesel-intro/) and [Diesel queries](/17-database/04-diesel-queries/) — the synchronous ORM and its DSL.
  - [Diesel relations](/17-database/05-diesel-relations/): `belongs_to`/`has_many` and eager loading.
  - [Connection pooling](/17-database/08-connection-pooling/) — `sqlx::Pool`, `deadpool`/`bb8`, and running Diesel off an async runtime.
  - [Migrations](/17-database/09-migrations/): `sqlx migrate` and Diesel migrations.
- Background from earlier sections:
  - [Section 11: Async](/11-async/) and [Promises vs Futures](/11-async/00-promises-vs-futures/) — why SQLx/SeaORM are async and Diesel is not.
  - [Section 09: Generics & Traits](/09-generics-traits/) — derives are trait implementations; this is why Diesel's mapping is checked statically.
  - [Section 08: Error Handling](/08-error-handling/) — `Result`, `?`, and `.optional()`.
- Next section: [Section 18: CLI Tools](/18-cli-tools/) — `sqlx-cli`, `diesel_cli`, and `sea-orm-cli` are themselves Rust CLIs, and the same patterns power your own.

---

## Exercises

### Exercise 1: Name the right tool

**Difficulty:** Beginner

**Objective:** Internalize the decision axis by matching scenarios to crates.

**Instructions:** For each scenario, name the crate (SQLx, Diesel, or SeaORM) that fits best and state the deciding factor in one sentence:

1. An Axum web service whose team is fluent in SQL and wants its queries verified against the schema at build time.
2. A synchronous command-line tool that imports a CSV into a database, with no async runtime anywhere.
3. A team migrating from a Prisma/TypeORM codebase that wants generated entities and ActiveRecord-style updates while staying async.

<details>
<summary>Solution</summary>

1. **SQLx.** It is async (fits Axum) and its `query!` macros check the SQL against the live database at compile time — ideal for a SQL-fluent team.
2. **Diesel.** Its synchronous, blocking model needs no runtime, which is the simplest fit for a CLI with no async anywhere. (SQLx without async would force you to add a runtime just for the database.)
3. **SeaORM.** It offers Prisma/TypeORM-like ergonomics — generated entities (`sea-orm-cli generate entity`) and the `ActiveModel` write pattern — while remaining fully async on top of SQLx.

The deciding factors are, in order: async vs sync, then how much you value compile-time-checked raw SQL versus ORM ergonomics and generated code.

</details>

### Exercise 2: Translate one query across two crates

**Difficulty:** Intermediate

**Objective:** Feel the difference between raw-SQL SQLx and the SeaORM DSL by writing the same read both ways.

**Instructions:** Given a `products` table (`id`, `name`, `price`), write a program that loads the **two most expensive products** (ordered by price descending, limit 2) and prints their names. Do it once with SQLx (`query_as` and a `LIMIT` in the SQL string) and once with SeaORM (`Entity::find().order_by_desc(...).limit(2)`). Both should print the same names. Use in-memory SQLite.

<details>
<summary>Solution</summary>

SQLx version:

```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::FromRow;

#[derive(Debug, FromRow)]
struct Product {
    name: String,
    price: f64,
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query("CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL NOT NULL)")
        .execute(&pool)
        .await?;
    for (n, p) in [("Keyboard", 49.99), ("Mouse", 19.5), ("Monitor", 199.0), ("Cable", 5.0)] {
        sqlx::query("INSERT INTO products (name, price) VALUES (?, ?)")
            .bind(n)
            .bind(p)
            .execute(&pool)
            .await?;
    }

    let top: Vec<Product> =
        sqlx::query_as::<_, Product>("SELECT name, price FROM products ORDER BY price DESC LIMIT 2")
            .fetch_all(&pool)
            .await?;
    println!("{:?}", top.iter().map(|p| &p.name).collect::<Vec<_>>());
    Ok(())
}
```

SeaORM version:

```toml
# Cargo.toml
[dependencies]
sea-orm = { version = "1.1", features = ["sqlx-sqlite", "runtime-tokio-rustls", "macros"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, QueryOrder, QuerySelect, Schema, Set};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "products")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    #[sea_orm(column_type = "Double")]
    pub price: f64,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

#[tokio::main]
async fn main() -> Result<(), DbErr> {
    let db = Database::connect("sqlite::memory:").await?;
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);
    db.execute(builder.build(&schema.create_table_from_entity(Entity)))
        .await?;
    for (n, p) in [("Keyboard", 49.99), ("Mouse", 19.5), ("Monitor", 199.0), ("Cable", 5.0)] {
        ActiveModel { name: Set(n.into()), price: Set(p), ..Default::default() }
            .insert(&db)
            .await?;
    }

    let top: Vec<Model> = Entity::find()
        .order_by_desc(Column::Price)
        .limit(2)
        .all(&db)
        .await?;
    println!("{:?}", top.iter().map(|p| &p.name).collect::<Vec<_>>());
    Ok(())
}
```

Both print:

```text
["Monitor", "Keyboard"]
```

The SQLx version carries the ordering and limit in the SQL string; the SeaORM version expresses them through `order_by_desc` and `limit` on the typed query (note `limit` comes from the `QuerySelect` trait). Same result, two philosophies.

</details>

### Exercise 3: Use SeaORM's pagination

**Difficulty:** Advanced

**Objective:** Exercise a feature an ORM gives you for free that you would hand-roll in SQLx — paginating a result set.

**Instructions:** Seed a `products` table with four rows. Using SeaORM's `Paginator` (the `paginate(&db, page_size)` method from the `PaginatorTrait`), page through all products **two per page**, ordered by price descending. Print the total number of pages, then print each page's `(name, price)` pairs.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
sea-orm = { version = "1.1", features = ["sqlx-sqlite", "runtime-tokio-rustls", "macros"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, PaginatorTrait, QueryOrder, Schema, Set};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "products")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    #[sea_orm(column_type = "Double")]
    pub price: f64,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

#[tokio::main]
async fn main() -> Result<(), DbErr> {
    let db = Database::connect("sqlite::memory:").await?;
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);
    db.execute(builder.build(&schema.create_table_from_entity(Entity)))
        .await?;
    for (n, p) in [("Keyboard", 49.99), ("Mouse", 19.50), ("Monitor", 199.0), ("Cable", 5.0)] {
        ActiveModel { name: Set(n.into()), price: Set(p), ..Default::default() }
            .insert(&db)
            .await?;
    }

    // num_pages reports the total; fetch_and_next walks the pages.
    let counter = Entity::find().order_by_desc(Column::Price).paginate(&db, 2);
    println!("total pages = {}", counter.num_pages().await?);

    let mut paginator = Entity::find().order_by_desc(Column::Price).paginate(&db, 2);
    let mut page_no = 0;
    while let Some(rows) = paginator.fetch_and_next().await? {
        println!(
            "page {page_no}: {:?}",
            rows.iter().map(|m| (&m.name, m.price)).collect::<Vec<_>>()
        );
        page_no += 1;
    }
    Ok(())
}
```

Real output:

```text
total pages = 2
page 0: [("Monitor", 199.0), ("Keyboard", 49.99)]
page 1: [("Mouse", 19.5), ("Cable", 5.0)]
```

In SQLx you would compute `LIMIT`/`OFFSET` and a separate `COUNT(*)` yourself; SeaORM's `Paginator` bundles both into one API. This is a concrete example of the ergonomics an ORM buys you over a raw-SQL toolkit.

</details>
