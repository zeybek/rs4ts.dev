---
title: "SeaORM: Async ActiveRecord Entities, Relations, and Transactions"
description: "A deep dive into SeaORM for TypeORM/Prisma users: entities and ActiveModels, relations with find_related, pagination, transactions, and sea-orm-cli codegen."
---

## Quick Overview

[SQLx vs Diesel vs SeaORM](/17-database/10-orm-comparison/) compared the three database layers; this page is the deep dive into the one that feels most like home if you are coming from **TypeORM or Prisma**: **SeaORM**, the async, ActiveRecord-style ORM built on top of SQLx. You define **entities** (like `@Entity` classes), write through **ActiveModels** (partial write models that track which fields changed), traverse **relations** with `find_related`, and everything is `async` — so it drops straight into an Axum handler with no `spawn_blocking` dance.

> **Note:** This page pins **SeaORM 1.1**, the same version compile-verified throughout this section ([see the section overview](/17-database/)). SeaORM **2.0 is in release candidate** as of mid-2026 — the concepts here carry over, but check the [migration notes](https://www.sea-ql.org/SeaORM/docs/index/) before adopting a 2.0 RC. Examples use SQLite in memory so they run with no server installed.

---

## TypeScript/JavaScript Example

The TypeORM workflow you already know: define entities with decorators, get repositories, write with `save`, read with `find`, and traverse relations:

```typescript
// npm install typeorm reflect-metadata sqlite3
import { Entity, PrimaryGeneratedColumn, Column, ManyToOne, OneToMany, DataSource } from "typeorm";

@Entity()
class User {
  @PrimaryGeneratedColumn() id!: number;
  @Column() name!: string;
  @Column({ default: true }) active!: boolean;
  @OneToMany(() => Post, (post) => post.author) posts!: Post[];
}

@Entity()
class Post {
  @PrimaryGeneratedColumn() id!: number;
  @Column() title!: string;
  @ManyToOne(() => User, (user) => user.posts) author!: User;
}

const db = new DataSource({
  type: "sqlite", database: ":memory:",
  entities: [User, Post], synchronize: true, // dev-only schema sync
});
await db.initialize();

const users = db.getRepository(User);
const ada = await users.save({ name: "Ada", active: true }); // INSERT

ada.active = false;
await users.save(ada);                                       // UPDATE

const withPosts = await users.find({ relations: { posts: true } }); // eager load
```

Everything is validated **at runtime**: a typo'd column name, a wrong type in a `where`, or a missing relation only fails when the query executes. Keep that in mind as the contrast point.

---

## Rust Equivalent

The same shape in SeaORM: two entities, a relation between them, an insert, a partial update, and an eager load. Entities live in modules (one per table — exactly how `sea-orm-cli` generates them):

```toml
# Cargo.toml — or run:
#   cargo add sea-orm@1.1 --features sqlx-sqlite,runtime-tokio-rustls,macros
#   cargo add tokio --features full
[dependencies]
sea-orm = { version = "1.1", features = ["sqlx-sqlite", "runtime-tokio-rustls", "macros"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, ModelTrait, QueryOrder, Schema, Set};

mod users {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "users")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub name: String,
        pub active: bool,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        // The `@OneToMany` side: a user has many posts.
        #[sea_orm(has_many = "super::posts::Entity")]
        Posts,
    }

    impl Related<super::posts::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Posts.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

mod posts {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "posts")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub title: String,
        pub user_id: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        // The `@ManyToOne` side: a post belongs to a user, via the FK column.
        #[sea_orm(
            belongs_to = "super::users::Entity",
            from = "Column::UserId",
            to = "super::users::Column::Id"
        )]
        User,
    }

    impl Related<super::users::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::User.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

#[tokio::main]
async fn main() -> Result<(), DbErr> {
    let db = Database::connect("sqlite::memory:").await?;

    // Create both tables from the entities (dev convenience, like synchronize: true).
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);
    db.execute(builder.build(&schema.create_table_from_entity(users::Entity)))
        .await?;
    db.execute(builder.build(&schema.create_table_from_entity(posts::Entity)))
        .await?;

    // INSERT — an ActiveModel with only the fields you set; PK stays NotSet.
    let ada = users::ActiveModel {
        name: Set("Ada".into()),
        active: Set(true),
        ..Default::default()
    }
    .insert(&db)
    .await?;

    for title in ["Analytical Engine notes", "On computable numbers"] {
        posts::ActiveModel {
            title: Set(title.into()),
            user_id: Set(ada.id),
            ..Default::default()
        }
        .insert(&db)
        .await?;
    }

    // UPDATE — load the Model, convert to ActiveModel, change one field, save.
    let mut ada: users::ActiveModel = ada.into();
    ada.active = Set(false); // only this column is written
    let ada: users::Model = ada.update(&db).await?;

    // Relation traversal — the `relations: { posts: true }` equivalent.
    let her_posts = ada
        .find_related(posts::Entity)
        .order_by_asc(posts::Column::Id)
        .all(&db)
        .await?;

    println!(
        "{} (active={}) has {} posts: {:?}",
        ada.name,
        ada.active,
        her_posts.len(),
        her_posts.iter().map(|p| &p.title).collect::<Vec<_>>()
    );
    Ok(())
}
```

Real output:

```text
Ada (active=false) has 2 posts: ["Analytical Engine notes", "On computable numbers"]
```

The pieces map one-to-one: the entity `Model` is your `@Entity` class, the `Relation` enum plus `Related` impl replaces the `@OneToMany`/`@ManyToOne` decorator pair, the `ActiveModel` is the write side, and `find_related` is the relation load.

---

## Detailed Explanation

### One entity, four generated types

`#[derive(DeriveEntityModel)]` on a `Model` struct expands into a small family per table, and learning their roles is most of learning SeaORM:

| Generated type | Role | TypeORM analogue |
| --- | --- | --- |
| `Model` | A **read-only row**: plain data, what queries return | an entity instance you loaded |
| `Entity` | The **table handle** you query through: `Entity::find()` | the repository |
| `Column` | An enum of **typed column references**: `Column::Active.eq(true)` | column names in `where: {...}` |
| `ActiveModel` | The **write model**: every field is `Set`, `NotSet`, or `Unchanged` | the partial object you pass to `save` |

The crucial split for a TypeORM user: **`Model` is immutable data; all writes go through `ActiveModel`.** There is no `user.active = false; repo.save(user)` mutation of a live entity — you convert a `Model` into an `ActiveModel` (`let mut am: ActiveModel = model.into()`), `Set` the fields you want, and `.update(&db)`. Because each field tracks its state, the generated `UPDATE` touches only the columns you actually `Set` — the same minimal-update behavior Prisma's `update({ data: {...} })` gives you.

### Reading: `find`, `filter`, `order`, pagination

`Entity::find()` opens a query builder that reads like a TypeORM `QueryBuilder`, but each method takes typed `Column` values rather than strings:

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{PaginatorTrait, QueryOrder};

// All matching rows
let active: Vec<users::Model> = users::Entity::find()
    .filter(users::Column::Active.eq(true))
    .order_by_asc(users::Column::Name)
    .all(&db)
    .await?;

// One row (Option<Model> — absence is honest, like findOneBy returning null)
let ada = users::Entity::find()
    .filter(users::Column::Name.eq("Ada"))
    .one(&db)
    .await?;

// By primary key
let by_id = users::Entity::find_by_id(1).one(&db).await?;

// Count without loading rows
let n = users::Entity::find().count(&db).await?;

// Pagination: pages of 20, fetch page 0 (the Prisma skip/take pattern)
let pages = users::Entity::find().paginate(&db, 20);
let first_page: Vec<users::Model> = pages.fetch_page(0).await?;
```

A typo'd column is a **compile error** (there is no `Column::Naem` variant), and comparing a column against the wrong Rust type fails to compile too. What SeaORM does *not* do is validate the query against your live schema at build time — that is SQLx's `query!` trick ([SQLx Queries](/17-database/01-sqlx-queries/)). If the entity drifts from the real table, you find out at runtime.

### Relations: the `Relation` enum and `Related` trait

The decorator pair you know becomes data: the FK side declares `belongs_to` (with the exact `from`/`to` columns), the parent side declares `has_many`, and a `Related` impl tells the query layer how to join the two. With that wiring there are two read patterns:

```rust
// Lazy, per-row: SELECT posts WHERE user_id = ? (a second query)
let posts = some_user.find_related(posts::Entity).all(&db).await?;

// Eager, for many rows: one JOIN, grouped into (parent, children) pairs
let users_with_posts: Vec<(users::Model, Vec<posts::Model>)> = users::Entity::find()
    .find_with_related(posts::Entity)
    .all(&db)
    .await?;
```

`find_with_related` is the `relations: { posts: true }` / Prisma `include` equivalent — and because it is one explicit call, the classic TypeORM N+1 trap (lazily touching `user.posts` in a loop) is harder to write by accident. If you *do* loop over parents calling `find_related`, you have reinvented N+1; reach for `find_with_related` instead.

### Transactions

Two APIs, both familiar from [SQLx Transactions](/17-database/02-sqlx-transactions/). The explicit guard:

```rust
use sea_orm::TransactionTrait;

let txn = db.begin().await?;

users::ActiveModel { name: Set("Grace".into()), active: Set(true), ..Default::default() }
    .insert(&txn)
    .await?;
posts::ActiveModel { title: Set("First!".into()), user_id: Set(3), ..Default::default() }
    .insert(&txn)
    .await?;

txn.commit().await?; // drop without commit = rollback
```

Every query method accepts any connection-like value, so `&db` and `&txn` are interchangeable — the same "pass the transaction where you'd pass the pool" discipline as SQLx. There is also a closure API (`db.transaction(|txn| Box::pin(async move { ... }))`) that commits on `Ok` and rolls back on `Err`, mirroring Prisma's `$transaction(async (tx) => ...)`.

### Codegen: `sea-orm-cli` writes the entities

Hand-writing entity modules (as in the example above) is fine for learning, but the intended workflow is **generate them from a live database** — the most Prisma-like experience in Rust:

```bash
cargo install sea-orm-cli
sea-orm-cli generate entity --database-url sqlite://app.db --output-dir src/entities
```

This emits one module per table with the `Model`, `Relation` enum (FKs detected from the schema), and `Related` impls filled in. Migrations live in a companion crate: `sea-orm-migration` gives you Rust-based up/down migrations run via `sea-orm-cli migrate` — the same role as `sqlx migrate` and Diesel's migration system ([Migrations](/17-database/09-migrations/)).

---

## Key Differences

| Concept | TypeORM / Prisma | SeaORM |
| --- | --- | --- |
| Entity definition | Decorated class / `schema.prisma` | `Model` struct + `DeriveEntityModel` (usually generated) |
| Reads return | Entity instances (mutable) | `Model` (plain immutable data) |
| Writes | `repo.save(obj)` / `update({ data })` | `ActiveModel` with `Set`/`NotSet` per field |
| Relations | `@OneToMany` decorators / `include` | `Relation` enum + `Related`; `find_related` / `find_with_related` |
| Wrong column name | Runtime error | Compile error (no such `Column` variant) |
| Schema drift | Runtime error | Runtime error (entities are not checked against live schema at build) |
| Async | Always (Promise) | Always (`Future`, runs on Tokio) |
| Codegen direction | Code → schema (`synchronize`) or schema file → client | **Database → code** (`sea-orm-cli generate entity`) |

The last row is worth internalizing: with SeaORM the *database* is the source of truth and entities are generated from it, where TypeORM's `synchronize: true` pushes your classes onto the database. In production both ecosystems converge on explicit migrations.

---

## Common Pitfalls

### Pitfall 1: Forgetting `..Default::default()` in an ActiveModel literal

An `ActiveModel` literal must say something about *every* field. The idiom is to `Set(...)` what you have and let `..Default::default()` mark the rest `NotSet` (which is what lets the database assign the auto-increment PK). Spelling out the PK as `Set(0)` "to make it compile" inserts a literal `0` — and the second insert collides.

### Pitfall 2: Editing a `Model` and expecting the change to persist

`Model` is plain data; mutating a field on it changes a struct in memory and nothing else. All persistence flows through `ActiveModel::insert`/`update` (or `Entity::update_many`). If you are looking for `repo.save`, the translation is `let mut am: ActiveModel = model.into(); am.field = Set(...); am.update(&db).await?`.

### Pitfall 3: N+1 via `find_related` in a loop

`find_related` issues one query per call. Calling it for each row of a parent list is exactly TypeORM's lazy-relation N+1, just more visible. For lists, use `find_with_related` (one join) and consume the `(parent, children)` pairs.

### Pitfall 4: Trusting entities over the real schema

SeaORM validates your query against the *entity*, not the database. If a migration renamed a column and you did not regenerate entities, everything still compiles and fails at runtime — the one place SQLx's `query!` macros are strictly stronger. Make `sea-orm-cli generate entity` part of your migration routine, the way you re-run `prisma generate` after editing `schema.prisma`.

---

## Best Practices

- **Generate entities; don't hand-maintain them.** `sea-orm-cli generate entity` after every migration keeps the Rust view of the schema honest, exactly like re-running `prisma generate`.
- **Keep `ActiveModel` construction at the edges.** Handlers should validate input into typed values first ([Section 16: Validation](/16-web-apis/)), then build an `ActiveModel` in one place — not pass half-set ActiveModels around.
- **Use `find_with_related` for lists, `find_related` for a single parent.** This single habit eliminates the N+1 class of bugs.
- **Pass the transaction explicitly.** Functions that write should take `&impl ConnectionTrait` so callers can hand them either the pool or an open transaction — same advice as the SQLx pages.
- **Pool through SeaORM's `Database::connect`**, which wraps a `sqlx::Pool` under the hood; the sizing guidance in [Connection Pooling](/17-database/08-connection-pooling/) applies unchanged.
- **Reach for the raw-SQL escape hatch without guilt.** `Statement::from_sql_and_values` + `FromQueryResult` (shown in the [comparison page](/17-database/10-orm-comparison/)) is the supported path for recursive CTEs and hand-tuned queries; bound parameters still prevent injection.

---

## Real-World Example

A compact task-board write path: create a board and its first tasks atomically, then render the board with tasks eagerly loaded — the same flow as the Express + Prisma handler pair you have written a dozen times. (The `boards`/`tasks` entities follow exactly the `users`/`posts` shape from the top of this page: a parent table, and a child table whose `board_id` column is the `belongs_to` FK.)

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, QueryOrder, Set, TransactionTrait};

/// POST /boards — create a board plus its seed tasks in one transaction.
async fn create_board_with_tasks(
    db: &DatabaseConnection,
    name: &str,
    seed_tasks: &[&str],
) -> Result<boards::Model, DbErr> {
    let txn = db.begin().await?;

    let board = boards::ActiveModel {
        name: Set(name.to_owned()),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    for title in seed_tasks {
        tasks::ActiveModel {
            title: Set((*title).to_owned()),
            board_id: Set(board.id),
            done: Set(false),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
    }

    txn.commit().await?; // any earlier `?` rolls everything back
    Ok(board)
}

/// GET /boards — every board with its tasks, one JOIN, no N+1.
async fn list_boards(
    db: &DatabaseConnection,
) -> Result<Vec<(boards::Model, Vec<tasks::Model>)>, DbErr> {
    boards::Entity::find()
        .find_with_related(tasks::Entity)
        .order_by_asc(boards::Column::Id)
        .all(db)
        .await
}
```

Note what the types are doing for you: `create_board_with_tasks` *cannot* forget to set `board_id` to a real id (it comes off the inserted `board`), the early-return `?` inside the transaction gives you automatic rollback, and `list_boards`' return type documents the eager-loaded shape that TypeORM would express as `relations: { tasks: true }`.

---

## Further Reading

- [SeaORM documentation](https://www.sea-ql.org/SeaORM/docs/index/) — the official book-style docs.
- [SeaORM on docs.rs](https://docs.rs/sea-orm/latest/sea_orm/) — API reference.
- [`sea-orm-cli`](https://www.sea-ql.org/SeaORM/docs/generate-entity/sea-orm-cli/) — entity generation and migrations.
- Related guide sections:
  - [SQLx vs Diesel vs SeaORM](/17-database/10-orm-comparison/) — where SeaORM sits among the three, with compiled side-by-side examples.
  - [SQLx Intro](/17-database/00-sqlx-intro/) — the layer SeaORM is built on.
  - [Connection Pooling](/17-database/08-connection-pooling/) — sizing the pool SeaORM wraps.
  - [Migrations](/17-database/09-migrations/) — schema evolution across SQLx, Diesel, and SeaORM.

---

## Exercises

### Exercise 1: A partial update that touches one column

**Difficulty:** Beginner

**Objective:** Internalize the `Model` → `ActiveModel` write path.

**Instructions:** Using the `users` entity from this page, write `async fn deactivate(db: &DatabaseConnection, id: i32) -> Result<Option<users::Model>, DbErr>` that loads a user by primary key, returns `Ok(None)` if it does not exist, and otherwise sets `active` to `false` (touching no other column) and returns the updated model.

<details>
<summary>Solution</summary>

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, Set};

async fn deactivate(
    db: &DatabaseConnection,
    id: i32,
) -> Result<Option<users::Model>, DbErr> {
    let Some(user) = users::Entity::find_by_id(id).one(db).await? else {
        return Ok(None);
    };

    let mut user: users::ActiveModel = user.into();
    user.active = Set(false); // the only field that will appear in the UPDATE
    Ok(Some(user.update(db).await?))
}
```

Because every other field of the `ActiveModel` is `Unchanged`, the generated SQL is `UPDATE users SET active = ? WHERE id = ?` — nothing else is written.

</details>

### Exercise 2: Spot and fix the N+1

**Difficulty:** Intermediate

**Objective:** Recognize the lazy-relation N+1 in SeaORM form.

**Instructions:** The function below issues one query for the users plus one query *per user*. Rewrite it to a single eager-loading call that returns the same data.

```rust
async fn users_with_posts_n_plus_one(
    db: &DatabaseConnection,
) -> Result<Vec<(users::Model, Vec<posts::Model>)>, DbErr> {
    let all_users = users::Entity::find().all(db).await?;
    let mut out = Vec::new();
    for user in all_users {
        let posts = user.find_related(posts::Entity).all(db).await?; // N queries!
        out.push((user, posts));
    }
    Ok(out)
}
```

<details>
<summary>Solution</summary>

```rust
async fn users_with_posts(
    db: &DatabaseConnection,
) -> Result<Vec<(users::Model, Vec<posts::Model>)>, DbErr> {
    users::Entity::find()
        .find_with_related(posts::Entity)
        .all(db)
        .await
}
```

One `JOIN`, grouped into `(user, posts)` pairs by SeaORM. The loop version runs `1 + N` queries for `N` users; this runs exactly one.

</details>

### Exercise 3: All-or-nothing seed

**Difficulty:** Intermediate

**Objective:** Use a transaction so partial failures leave no debris.

**Instructions:** Write `async fn seed(db: &DatabaseConnection) -> Result<(), DbErr>` that inserts three users and, for each, one welcome post — all inside a single transaction, so that if any insert fails nothing is committed. Use the explicit `begin`/`commit` API.

<details>
<summary>Solution</summary>

```rust
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, Set, TransactionTrait};

async fn seed(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;

    for name in ["Ada", "Alan", "Grace"] {
        let user = users::ActiveModel {
            name: Set(name.to_owned()),
            active: Set(true),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        posts::ActiveModel {
            title: Set(format!("Welcome, {name}!")),
            user_id: Set(user.id),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
    }

    txn.commit().await
}
```

Every `?` before `commit` propagates the error out of the function, dropping `txn` — and an uncommitted transaction rolls back on drop. The inserts use `&txn`, not `db`; mixing the two is the classic way to leak writes outside the transaction.

</details>
