---
title: "Rust Databases: SQLx, Diesel & SeaORM"
sidebar:
  label: "Overview"
description: "Talk to databases from Rust: SQLx checks raw SQL at compile time, Diesel and SeaORM give ORM ergonomics, plus MongoDB, Redis, pooling, and migrations vs TypeScript."
---

In TypeScript you reach for TypeORM, Prisma, or Knex and trust that your queries are correct until they run. Rust's database story stakes out the same positions but moves the safety net earlier: **SQLx** lets you write raw SQL that the compiler type-checks against your real database at build time, **Diesel** is a synchronous ORM with an end-to-end statically typed query DSL, and **SeaORM** offers async ActiveRecord ergonomics on top of SQLx. This section covers all three plus the **MongoDB** and **Redis** drivers, **connection pooling**, and **migrations** — everything you need to talk to a real datastore from a Rust service.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. Crate examples are compile-verified against the versions pinned throughout this section (SQLx 0.8.6, Diesel 2.3, SeaORM 1.1, the `mongodb` driver 3.7, the `redis` crate 1.2, `deadpool` 0.14, `bb8` 0.9) and run against in-memory or bundled SQLite wherever a server is not needed. (SQLx 0.9 shipped in May 2026; the examples deliberately pin the 0.8 line.)

---

## What You'll Learn

- How SQLx checks raw SQL against your live schema **at compile time**, and how to connect to PostgreSQL and SQLite from a single async API
- How to write queries with the `query!`/`query_as!` macros, bind parameters (which is what actually prevents SQL injection), and map rows into structs with `FromRow`
- How to run transactions in SQLx — `begin`/`commit`/`rollback`, the RAII `Transaction` guard, and the atomicity guarantees it gives you
- How Diesel's synchronous ORM works: the generated `schema.rs`, model structs, project setup, and why there is no `async`/`await`
- How to build queries with Diesel's typed DSL — `filter`/`select`/`order`/`insert`/`update`/`delete`
- How to model relationships in Diesel with `belongs_to`/`has_many` associations, joins, and eager loading
- How to use MongoDB from Rust: BSON, documents, CRUD, and typed collections backed by serde
- How to use Redis from Rust: async connections, typed command replies, and the everyday patterns (caching, counters, rate limiting)
- How to manage connection pools: `sqlx::Pool`, the generic `deadpool`/`bb8` poolers, and the sizing and lifecycle knobs that keep a service healthy under load
- How to run database migrations with `sqlx migrate` and Diesel's migration system — up/down scripts and running them at startup
- How to choose between SQLx, Diesel, and SeaORM for a given project
- How SeaORM's async ActiveRecord style works in depth: entities, `ActiveModel` writes, relations without N+1, transactions, and `sea-orm-cli` codegen

---

## Topics

| Topic | Description |
| ----- | ----------- |
| [SQLx Intro](/17-database/00-sqlx-intro/) | SQL databases with SQLx: async, compile-time-checked queries, setup, and connecting to PostgreSQL/SQLite. |
| [SQLx Queries](/17-database/01-sqlx-queries/) | Writing queries with `query!`/`query_as!`; binding parameters (which prevents SQL injection) and mapping rows with `FromRow`. |
| [SQLx Transactions](/17-database/02-sqlx-transactions/) | Transactions in SQLx: `begin`/`commit`/`rollback`, the `Transaction` guard, and atomicity. |
| [Diesel Intro](/17-database/03-diesel-intro/) | The Diesel ORM (like TypeORM): the `table!` schema, models, project setup, and the synchronous model. |
| [Diesel Queries](/17-database/04-diesel-queries/) | Diesel's query builder: `filter`/`select`/`order`/`insert`/`update`/`delete` and the typed DSL. |
| [Diesel Relations](/17-database/05-diesel-relations/) | Relationships and joins in Diesel: `belongs_to`/`has_many` associations and eager loading. |
| [MongoDB](/17-database/06-mongodb/) | MongoDB with the official driver: BSON, documents, CRUD, and typed collections via serde. |
| [Redis](/17-database/07-redis/) | Redis with the `redis` crate: commands, async connections, and common patterns (cache, counters). |
| [Connection Pooling](/17-database/08-connection-pooling/) | Connection pool management: `sqlx::Pool`, `deadpool`/`bb8`, and sizing and lifecycle. |
| [Migrations](/17-database/09-migrations/) | Database migrations: `sqlx migrate` and Diesel migrations; up/down scripts and running at startup. |
| [ORM Comparison](/17-database/10-orm-comparison/) | Comparing SQLx vs Diesel vs SeaORM: compile-checked SQL vs ORM ergonomics, and when to use which. |
| [SeaORM](/17-database/11-sea-orm/) | The async ActiveRecord ORM in depth: entities, ActiveModels, relations, transactions, and `sea-orm-cli` codegen. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Explain how SQLx's compile-time query checking differs from the runtime-only validation of TypeORM, Prisma, and Knex, and connect to PostgreSQL and SQLite
- Write parameterized queries with the `query!`/`query_as!` macros and map results into your own types with `FromRow`
- Wrap multi-statement operations in a transaction and rely on the `Transaction` guard to roll back on early return
- Set up a Diesel project, generate `schema.rs` from migrations, and define model structs for reads and writes
- Build reads and writes through Diesel's typed DSL and reason about the compiler errors when a type or column is wrong
- Model and traverse relationships with `belongs_to`/`has_many` associations and load related rows efficiently
- Perform CRUD against MongoDB with typed, serde-backed documents and against Redis with typed command replies
- Configure a connection pool with appropriate size, timeouts, and lifetimes, and share it across handlers as a cheap clonable handle
- Author and run up/down migrations with both SQLx and Diesel, including at application startup
- Pick the right database layer — compile-checked SQL vs. ORM ergonomics — for a given project and justify the choice
- Model entities, relations, and transactional writes in SeaORM's async ActiveRecord style, and generate entities from a live schema with `sea-orm-cli`

---

## Prerequisites

- [Section 11: Async](/11-async/) — SQLx, MongoDB, Redis, and SeaORM are all `async`; you need `async`/`await`, the Tokio runtime, and the lazy-future model. Diesel is the synchronous exception, which this section contrasts against the async crates.
- [Section 15: Serialization](/15-serialization/) — serde's `Serialize`/`Deserialize` drive typed MongoDB documents and JSON columns, and `FromRow`/`Queryable` follow the same "map a row into a typed struct" idea.

A working knowledge of [error handling](/08-error-handling/) (`Result`, `?`, `anyhow`/`thiserror`) is assumed throughout — every database call returns a `Result` you propagate.

---

## Estimated Time

Approximately **14 hours**, including reading, hands-on practice, and the per-topic exercises.

> **Tip:** If you only have time for one track, read `sqlx-intro` → `sqlx-queries` → `sqlx-transactions` → `connection-pooling` → `migrations`. That is the most common path for a new Rust web service. Treat the Diesel pages, MongoDB, and Redis as a toolbox to reach for when a specific need arises, read `orm-comparison` once to make the SQLx-vs-Diesel-vs-SeaORM decision deliberately, and go deeper with the `sea-orm` page if that decision lands on SeaORM.

---

## Next

Continue to [Section 18: CLI Tools](/18-cli-tools/) to build self-contained command-line tools with clap, ratatui, and indicatif.
