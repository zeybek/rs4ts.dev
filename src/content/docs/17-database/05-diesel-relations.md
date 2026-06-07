---
title: "Diesel Relations: Associations, Joins, and Eager Loading"
description: "Model relations in Diesel: declare joins, derive Associations, and eager-load parent/child trees with belonging_to and grouped_by to dodge N+1, all checked"
---

In TypeORM or Prisma you describe relations with decorators (`@OneToMany`, `@ManyToOne`) or a schema block, then `include`/`relations` pulls children in automatically. Diesel takes a more explicit, compile-checked approach: you declare which tables can be joined, derive `Associations`, and assemble parent/child trees yourself with `belonging_to` and `grouped_by`. This page shows how the **`belongs_to`/`has_many`** model maps onto Diesel and how to do **eager loading without N+1 queries**.

---

## Quick Overview

A relational schema is mostly about foreign keys: a post **belongs to** an author, an author **has many** posts. TypeORM hides the wiring behind decorators and lazy proxies; Diesel keeps it explicit and type-safe. You declare joins once in your schema, derive `Associations` on your models, and load related rows either with SQL joins (`inner_join`/`left_join`) or with the association helpers (`belonging_to` + `grouped_by`) that let you fetch a whole parent/child tree in a fixed number of queries.

> **Note:** Diesel is **synchronous**. The examples here run on a blocking connection, exactly as covered in [Getting Started with Diesel (the TypeORM of Rust)](/17-database/03-diesel-intro/). For async query execution, SQLx is the more natural fit; see [SQLx](/17-database/00-sqlx-intro/) and the trade-offs in [SQLx vs Diesel vs SeaORM](/17-database/10-orm-comparison/).

The examples below use the bundled SQLite backend so they run with no external database. The concepts are identical on PostgreSQL and MySQL.

```bash
cargo add diesel --features sqlite,returning_clauses_for_sqlite_3_35
cargo add libsqlite3-sys@0.37 --features bundled   # ships SQLite so no system lib is needed
```

The current stable toolchain is Rust 1.96.0 on the 2024 edition (`cargo new` selects it automatically); the code here was verified against Diesel 2.3.

---

## TypeScript/JavaScript Example

Here is a typical blog schema with TypeORM. An `Author` has many `Post`s, and each `Post` has many `Comment`s.

```typescript
// entities.ts (TypeORM)
import {
  Entity,
  PrimaryGeneratedColumn,
  Column,
  OneToMany,
  ManyToOne,
} from "typeorm";

@Entity()
export class Author {
  @PrimaryGeneratedColumn() id!: number;
  @Column() name!: string;

  @OneToMany(() => Post, (post) => post.author)
  posts!: Post[];
}

@Entity()
export class Post {
  @PrimaryGeneratedColumn() id!: number;
  @Column() title!: string;

  @ManyToOne(() => Author, (author) => author.posts)
  author!: Author;

  @OneToMany(() => Comment, (comment) => comment.post)
  comments!: Comment[];
}

@Entity()
export class Comment {
  @PrimaryGeneratedColumn() id!: number;
  @Column() body!: string;

  @ManyToOne(() => Post, (post) => post.comments)
  post!: Post;
}
```

Loading related data is one method call. TypeORM generates the joins (or extra queries) for you:

```typescript
// Eager-load every author together with their posts.
const authors = await dataSource.getRepository(Author).find({
  relations: { posts: true },
});

for (const author of authors) {
  console.log(`${author.name} has ${author.posts.length} post(s)`);
}

// A nested tree: authors -> posts -> comments
const tree = await dataSource.getRepository(Author).find({
  relations: { posts: { comments: true } },
});
```

This is convenient, but two costs are hidden:

- **The N+1 problem.** With lazy relations (or a naive loop calling `author.posts`), one query per author is issued. TypeORM can do this silently.
- **No compile-time guarantee** that `posts.author` actually maps to a real column; a typo in a relation name surfaces only at runtime.

---

## Rust Equivalent

Diesel models are plain structs. You declare the foreign-key relationships in the schema (which `diesel print-schema` normally generates) and on the models, then assemble trees explicitly.

```rust
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

// Normally generated into schema.rs by `diesel print-schema`.
mod schema {
    diesel::table! {
        authors (id) { id -> Integer, name -> Text, }
    }
    diesel::table! {
        posts (id) { id -> Integer, author_id -> Integer, title -> Text, }
    }
    diesel::table! {
        comments (id) { id -> Integer, post_id -> Integer, body -> Text, }
    }

    // Declare the foreign-key join paths once.
    diesel::joinable!(posts -> authors (author_id));
    diesel::joinable!(comments -> posts (post_id));
    diesel::allow_tables_to_appear_in_same_query!(authors, posts, comments);
}

use schema::{authors, comments, posts};

#[derive(Queryable, Selectable, Identifiable, Debug, PartialEq)]
#[diesel(table_name = authors)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct Author {
    id: i32,
    name: String,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Debug, PartialEq)]
#[diesel(belongs_to(Author))] // posts.author_id -> authors.id
#[diesel(table_name = posts)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct Post {
    id: i32,
    author_id: i32,
    title: String,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Debug, PartialEq)]
#[diesel(belongs_to(Post))] // comments.post_id -> posts.id
#[diesel(table_name = comments)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct Comment {
    id: i32,
    post_id: i32,
    body: String,
}
```

Eager-loading every author with their posts in exactly **two** queries:

```rust
fn authors_with_posts(conn: &mut SqliteConnection) -> QueryResult<Vec<(Author, Vec<Post>)>> {
    // Query 1: load all authors.
    let all_authors = authors::table
        .select(Author::as_select())
        .load(conn)?;

    // Query 2: load every post belonging to any of those authors,
    // then bucket the posts under their parent author.
    let posts_per_author = Post::belonging_to(&all_authors)
        .select(Post::as_select())
        .load(conn)?
        .grouped_by(&all_authors);

    // Zip authors with their post buckets — no extra query.
    Ok(all_authors.into_iter().zip(posts_per_author).collect())
}
```

Calling it and printing the result produces this **real** output:

```text
== eager-loaded authors with posts ==
  Ada has 2 post(s)
  Grace has 1 post(s)
```

---

## Detailed Explanation

### Declaring the relationship

Two declarations describe a `belongs_to`/`has_many` pair:

1. **`joinable!(posts -> authors (author_id))`** in the schema tells Diesel that `posts.author_id` is the foreign key into `authors.id`. This is what makes SQL joins (`inner_join`, `left_join`) type-check.
2. **`#[diesel(belongs_to(Author))]`** on the `Post` model, paired with `#[derive(Associations)]`, generates a `BelongsTo<Author>` impl. This is what powers `Post::belonging_to(...)`.

By convention Diesel infers the foreign key as `author_id` (the parent's snake-cased struct name + `_id`). If your column is named differently, spell it out: `#[diesel(belongs_to(Author, foreign_key = writer_id))]`.

There is **no `has_many` annotation**. In Diesel, "an author has many posts" is just the inverse of "a post belongs to an author"; you express it by calling `Post::belonging_to(&author)`. This is the opposite direction from TypeORM, where you decorate both sides.

> **Note:** `belonging_to` requires the parent type to derive **`Identifiable`** (so Diesel knows the parent's primary key). Forgetting it is a common error; see Common Pitfalls.

### `belonging_to` for a single parent

```rust
let ada = authors::table
    .find(1)
    .select(Author::as_select())
    .first(conn)?;

let adas_posts = Post::belonging_to(&ada)
    .select(Post::as_select())
    .load(conn)?;
```

`Post::belonging_to(&ada)` builds a query equivalent to `SELECT ... FROM posts WHERE posts.author_id = 1`. It returns a normal query you can further `.filter()`, `.order()`, or `.limit()` just like any other Diesel query (see [Diesel Query Builder](/17-database/04-diesel-queries/)).

### `belonging_to` for many parents + `grouped_by` (the eager-load pattern)

The key insight: `belonging_to` accepts a **slice** of parents, generating `WHERE posts.author_id IN (...)`. You then call `.grouped_by(&parents)` on the loaded children. `grouped_by` returns a `Vec<Vec<Child>>` whose order matches the parent slice: bucket `i` holds the children of parent `i`, including empty buckets for parents with no children. A final `zip` stitches them together:

```rust
let authors = authors::table.select(Author::as_select()).load(conn)?;          // 1 query
let grouped = Post::belonging_to(&authors)                                     // 1 query
    .select(Post::as_select())
    .load(conn)?
    .grouped_by(&authors);
let result: Vec<(Author, Vec<Post>)> = authors.into_iter().zip(grouped).collect();
```

Two queries total, regardless of how many authors exist. This is Diesel's answer to the N+1 problem: instead of one query per parent, you batch all children into a single `IN (...)` query and group them in memory.

### SQL joins for flat rows

When you want flat tuples rather than a nested tree, use `inner_join`:

```rust
let rows: Vec<(Post, Author)> = posts::table
    .inner_join(authors::table)
    .select((Post::as_select(), Author::as_select()))
    .load(conn)?;

for (post, author) in &rows {
    println!("'{}' by {}", post.title, author.name);
}
```

Real output for the sample data:

```text
== inner join (post -> author) ==
  'Rust ownership' by Ada
  'Lifetimes' by Ada
  'Borrow checker' by Grace
```

Diesel infers the `ON posts.author_id = authors.id` clause from the `joinable!` declaration, so you do not write it by hand. For an outer join, use `left_join` and select the right-hand side as `Option<T>`:

```rust
let rows: Vec<(Author, Option<Post>)> = authors::table
    .left_join(posts::table)
    .select((Author::as_select(), Option::<Post>::as_select()))
    .load(conn)?;
```

An author with no posts yields `(author, None)`. This is the type system encoding the SQL fact that the right side of a left join may be null — far harder to forget than in TypeScript, where a missing row is just `undefined`.

### Three-level eager loading

The same pattern nests. To build `authors -> posts -> comments` in **three** queries:

```rust
let authors = authors::table.select(Author::as_select()).load(conn)?;        // query 1
let posts = Post::belonging_to(&authors).select(Post::as_select()).load(conn)?; // query 2
let comments = Comment::belonging_to(&posts)                                  // query 3
    .select(Comment::as_select())
    .load(conn)?;

// Group bottom-up.
let comments_per_post = comments.grouped_by(&posts);
let posts_with_comments: Vec<(Post, Vec<Comment>)> =
    posts.into_iter().zip(comments_per_post).collect();
let grouped = posts_with_comments.grouped_by(&authors);
let tree: Vec<(Author, Vec<(Post, Vec<Comment>)>)> =
    authors.into_iter().zip(grouped).collect();
```

Note `grouped_by` works on `(Post, Vec<Comment>)` tuples too: it groups by the `Post` (the `BelongsTo<Author>` parent), carrying the attached comments along. Real output:

```text
== three-level tree ==
  Ada:
    Rust ownership (2 comments)
    Lifetimes (0 comments)
  Grace:
    Borrow checker (1 comments)
```

Three round-trips for an arbitrarily large, fully nested tree — versus the N+1 explosion a naive loop would cause.

---

## Key Differences

| Concept | TypeScript (TypeORM/Prisma) | Rust (Diesel) |
| --- | --- | --- |
| Declare a relation | `@OneToMany` / `@ManyToOne` decorators on both sides | `joinable!` + `belongs_to(Parent)` on the child only |
| `has_many` | Explicit `@OneToMany` field | Implicit: inverse of `belongs_to`, via `Child::belonging_to(&parent)` |
| Eager load | `find({ relations: { posts: true } })` | `belonging_to(&parents)` + `grouped_by` + `zip` |
| Join | Generated automatically; relation is a field | `inner_join` / `left_join`; result is a tuple `(A, B)` |
| Outer join null | `undefined` on the relation | `Option<Child>` enforced by the type system |
| Avoiding N+1 | Up to you to remember `relations`/`include` | Built into the `belonging_to(&slice)` pattern |
| Wrong relation name | Runtime error | Compile error (`JoinTo` / `Identifiable` not implemented) |
| Lazy loading | Default in some configs (proxies) | Does not exist — loads are always explicit |

The biggest mental shift: Diesel has **no lazy-loaded relation fields**. A `Post` struct has an `author_id: i32`, not an `author: Author`. You never accidentally trigger a query by touching a field; every database access is a visible `.load()`/`.first()` call. That removes a whole class of surprise N+1 queries that lazy ORMs are prone to, at the cost of a little more ceremony.

The second shift: relationships are **directional and one-sided** in the model. You annotate the child (`belongs_to`), and the parent's "has many" is just a query against the child. There is nothing to keep in sync on the parent side.

---

## Common Pitfalls

### Forgetting `joinable!` / `allow_tables_to_appear_in_same_query!`

If you try to join two tables that Diesel does not know are related, you get a compile error — not a runtime surprise. Omitting `joinable!(posts -> authors (author_id))` and writing `posts::table.inner_join(authors::table)` produces this **real** error:

```text
error[E0277]: cannot join `authors::table` to `posts::table` due to missing relation
   --> src/main.rs:27:21
    |
    |          ---------- ^^^^^^^^^^^^^^ the trait `JoinTo<authors::table>` is not implemented for `posts::table`
```

A second error points right at the fix:

```text
error[E0277]: the trait bound `posts::table: TableNotEqual<authors::table>` is not satisfied
   = note: double check that `authors::table` and `posts::table` appear in the same `allow_tables_to_appear_in_same_query!`
```

**Fix:** add both `joinable!(posts -> authors (author_id))` and list the tables in `allow_tables_to_appear_in_same_query!`. When `diesel print-schema` generates your `schema.rs` from a database with real foreign-key constraints, it writes these for you.

### Forgetting `Identifiable` (or `Associations`) for `belonging_to`

`belonging_to` is provided by the association machinery and needs the parent to derive `Identifiable` and the child to derive `Associations`. If the parent `Author` derives only `Queryable, Selectable`, calling `Post::belonging_to(&author)` fails to compile with this **real** error:

```text
error[E0599]: no function or associated item named `belonging_to` found for struct `Post` in the current scope
   --> src/main.rs:28:19
   = help: items from traits can only be used if the trait is implemented and in scope
```

**Fix:** derive `Identifiable` on the parent and `Associations` (plus `Identifiable`) on the child, and add `#[diesel(belongs_to(Parent))]`.

### The N+1 trap: looping with `belonging_to(&single)`

It is easy to fall back into TypeORM habits and write a loop:

```rust
// Anti-pattern: one query per author (N+1).
let mut tree = Vec::new();
for author in authors {
    let posts = Post::belonging_to(&author) // ← runs a SELECT every iteration
        .select(Post::as_select())
        .load(conn)?;
    tree.push((author, posts));
}
```

This compiles and works, but issues one query per author. **Fix:** pass the whole slice once — `Post::belonging_to(&authors)` — and call `grouped_by`, as shown above. The slice form generates a single `WHERE author_id IN (...)`.

### `grouped_by` order matters

`grouped_by(&parents)` assumes the same ordering and membership relationship as the parent slice you pass it. Always `grouped_by` against the **exact** parent `Vec` you then `zip` with, and group **bottom-up** (deepest children first) when nesting more than one level. Grouping against a different or re-sorted slice silently misaligns buckets.

### Counting with a left join: `count_star` over-counts

A natural "posts per author" query using `count_star()` over a `left_join` counts the synthetic null row for authors with zero posts, reporting `1` instead of `0`. Count a nullable child column instead:

```rust
use diesel::dsl::count;

let rows: Vec<(String, i64)> = authors::table
    .left_join(posts::table)
    .group_by(authors::id)
    .select((authors::name, count(posts::id.nullable())))
    .load(conn)?;
```

Verified output (note `Lonely`, who has no posts):

```text
Ada: 2
Grace: 1
Lonely: 0
```

`count(posts::id.nullable())` ignores nulls, so authors with no posts correctly report `0`.

---

## Best Practices

- **Generate `schema.rs` from the database** with `diesel print-schema` (driven by real foreign-key constraints) so `joinable!` and `allow_tables_to_appear_in_same_query!` are written and kept correct for you. See [Database Migrations](/17-database/09-migrations/) for how migrations establish those constraints.
- **Default to the `belonging_to(&slice)` + `grouped_by` pattern** for one-to-many eager loads. Reach for explicit `inner_join`/`left_join` when you want flat tuples or you need to filter/sort across both tables in SQL.
- **Use `Selectable` + `as_select()`** rather than relying on column order in `Queryable`. It ties the struct to named columns and, with `#[diesel(check_for_backend(...))]`, makes Diesel verify field types against the backend at compile time.
- **Select the right side of a `left_join` as `Option<T>`.** Let the type system represent "may be absent"; do not flatten it away.
- **Name foreign keys conventionally** (`author_id`, `post_id`) so the inferred `belongs_to` foreign key just works; otherwise pass `foreign_key = ...` explicitly.
- **Group bottom-up** for multi-level trees, and keep the parent `Vec` around until after the final `zip`.

---

## Real-World Example

A small "blog dashboard" function that loads every author with their posts and each post's comments in a fixed three queries, then renders a summary, the kind of payload an API endpoint would serialize. This is the complete, compile-verified program (run against bundled SQLite).

```rust
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

mod schema {
    diesel::table! { authors (id) { id -> Integer, name -> Text, } }
    diesel::table! { posts (id) { id -> Integer, author_id -> Integer, title -> Text, } }
    diesel::table! { comments (id) { id -> Integer, post_id -> Integer, body -> Text, } }
    diesel::joinable!(posts -> authors (author_id));
    diesel::joinable!(comments -> posts (post_id));
    diesel::allow_tables_to_appear_in_same_query!(authors, posts, comments);
}
use schema::{authors, comments, posts};

#[derive(Queryable, Selectable, Identifiable, Debug)]
#[diesel(table_name = authors)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct Author { id: i32, name: String }

#[derive(Queryable, Selectable, Identifiable, Associations, Debug)]
#[diesel(belongs_to(Author))]
#[diesel(table_name = posts)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct Post { id: i32, author_id: i32, title: String }

#[derive(Queryable, Selectable, Identifiable, Associations, Debug)]
#[diesel(belongs_to(Post))]
#[diesel(table_name = comments)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct Comment { id: i32, post_id: i32, body: String }

/// Load the full author -> posts -> comments tree in three queries.
fn load_dashboard(
    conn: &mut SqliteConnection,
) -> QueryResult<Vec<(Author, Vec<(Post, Vec<Comment>)>)>> {
    let authors = authors::table.select(Author::as_select()).load(conn)?;
    let posts = Post::belonging_to(&authors).select(Post::as_select()).load(conn)?;
    let comments = Comment::belonging_to(&posts).select(Comment::as_select()).load(conn)?;

    let comments_per_post = comments.grouped_by(&posts);
    let posts_with_comments: Vec<(Post, Vec<Comment>)> =
        posts.into_iter().zip(comments_per_post).collect();
    let grouped = posts_with_comments.grouped_by(&authors);
    Ok(authors.into_iter().zip(grouped).collect())
}

fn seed(conn: &mut SqliteConnection) {
    for sql in [
        "CREATE TABLE authors (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        "CREATE TABLE posts (id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL, title TEXT NOT NULL)",
        "CREATE TABLE comments (id INTEGER PRIMARY KEY, post_id INTEGER NOT NULL, body TEXT NOT NULL)",
        "INSERT INTO authors VALUES (1,'Ada'),(2,'Grace')",
        "INSERT INTO posts VALUES (1,1,'Rust ownership'),(2,1,'Lifetimes'),(3,2,'Borrow checker')",
        "INSERT INTO comments VALUES (1,1,'Great!'),(2,1,'Helpful'),(3,3,'Thanks')",
    ] {
        diesel::sql_query(sql).execute(conn).unwrap();
    }
}

fn main() -> QueryResult<()> {
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    seed(&mut conn);

    for (author, posts) in load_dashboard(&mut conn)? {
        println!("{}:", author.name);
        for (post, comments) in posts {
            println!("    {} ({} comments)", post.title, comments.len());
        }
    }
    Ok(())
}
```

Real output:

```text
Ada:
    Rust ownership (2 comments)
    Lifetimes (0 comments)
Grace:
    Borrow checker (1 comments)
```

In a real service you would obtain `conn` from a pool rather than a one-off `:memory:` connection — see [Connection Pooling](/17-database/08-connection-pooling/) — and you would derive `serde::Serialize` on the structs to return the tree as JSON from a web handler (covered in [15-serialization](/15-serialization/) and [16-web-apis](/16-web-apis/)).

---

## Further Reading

- [Diesel: Relations guide](https://diesel.rs/guides/relations.html): the official walkthrough of `belongs_to`, `belonging_to`, and `grouped_by`.
- [Diesel API: `Associations` derive](https://docs.rs/diesel/latest/diesel/associations/derive.Associations.html) and [`BelongingToDsl`](https://docs.rs/diesel/latest/diesel/associations/trait.BelongingToDsl.html).
- [Diesel API: `grouped_by`](https://docs.rs/diesel/latest/diesel/associations/trait.GroupedBy.html): how children are bucketed under parents.
- [Diesel API: `joinable!` and joins](https://docs.rs/diesel/latest/diesel/macro.joinable.html).

Related sections of this guide:

- [Getting Started with Diesel (the TypeORM of Rust)](/17-database/03-diesel-intro/): Diesel setup, `table!`, models, and the synchronous model.
- [Diesel Query Builder](/17-database/04-diesel-queries/) — the query DSL: `filter`, `select`, `order`, insert/update/delete.
- [Database Migrations](/17-database/09-migrations/): Diesel migrations that create the foreign-key constraints behind these relations.
- [Connection Pooling](/17-database/08-connection-pooling/) — getting a connection from a pool instead of a one-off.
- [SQLx vs Diesel vs SeaORM](/17-database/10-orm-comparison/): Diesel vs SQLx vs SeaORM, and when to pick each.
- [Writing Queries with SQLx](/17-database/01-sqlx-queries/) — the SQLx alternative, where you write the join SQL yourself.
- [CLI Tools](/18-cli-tools/) — building the kind of CLI (like `diesel_cli`) that drives schema generation.

---

## Exercises

### Exercise 1: Posts for a single author

**Difficulty:** Easy

**Objective:** Use `belonging_to` to load all posts for one author.

**Instructions:** Given the `Author` and `Post` models from this page, write a function `fn posts_of(conn: &mut SqliteConnection, author: &Author) -> QueryResult<Vec<Post>>` that returns every post belonging to the given author, ordered by `title`.

<details>
<summary>Solution</summary>

```rust
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use crate::schema::posts;

fn posts_of(conn: &mut SqliteConnection, author: &Author) -> QueryResult<Vec<Post>> {
    Post::belonging_to(author)
        .order(posts::title.asc())
        .select(Post::as_select())
        .load(conn)
}
```

`belonging_to(author)` builds `WHERE posts.author_id = <author.id>`; it returns an ordinary query, so you can chain `.order(...)`, `.filter(...)`, etc. before `.load`. This is verified to compile against Diesel 2.3 with the models from this page.

</details>

### Exercise 2: Eager-load authors with their posts (no N+1)

**Difficulty:** Medium

**Objective:** Load every author paired with their posts in exactly two queries.

**Instructions:** Write `fn authors_with_posts(conn: &mut SqliteConnection) -> QueryResult<Vec<(Author, Vec<Post>)>>`. Load all authors, then all their posts in one batched query, group with `grouped_by`, and `zip`. Do **not** issue one query per author.

<details>
<summary>Solution</summary>

```rust
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use crate::schema::authors;

fn authors_with_posts(
    conn: &mut SqliteConnection,
) -> QueryResult<Vec<(Author, Vec<Post>)>> {
    let all_authors = authors::table
        .select(Author::as_select())
        .load(conn)?; // query 1

    let posts_per_author = Post::belonging_to(&all_authors)
        .select(Post::as_select())
        .load(conn)? // query 2: WHERE author_id IN (...)
        .grouped_by(&all_authors);

    Ok(all_authors.into_iter().zip(posts_per_author).collect())
}
```

The slice form of `belonging_to` plus `grouped_by` is the canonical Diesel eager-load pattern: two queries regardless of how many authors there are. Verified output for the sample data is `Ada has 2 post(s)` / `Grace has 1 post(s)`.

</details>

### Exercise 3: Authors with no posts, via a left join

**Difficulty:** Hard

**Objective:** Find every author who has written zero posts, using an outer join.

**Instructions:** Write `fn authors_without_posts(conn: &mut SqliteConnection) -> QueryResult<Vec<Author>>`. Use `left_join` so authors with no posts still appear, select the post side as `Option<Post>`, and keep only the authors whose post side is `None`. (Bonus: how would you instead do this purely in SQL with `filter` + `is_null`?)

<details>
<summary>Solution</summary>

```rust
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use crate::schema::{authors, posts};

fn authors_without_posts(conn: &mut SqliteConnection) -> QueryResult<Vec<Author>> {
    let rows: Vec<(Author, Option<Post>)> = authors::table
        .left_join(posts::table)
        .select((Author::as_select(), Option::<Post>::as_select()))
        .load(conn)?;

    Ok(rows
        .into_iter()
        .filter(|(_, post)| post.is_none())
        .map(|(author, _)| author)
        .collect())
}

// Bonus: push the filter into SQL so the database does the work.
fn authors_without_posts_sql(conn: &mut SqliteConnection) -> QueryResult<Vec<Author>> {
    authors::table
        .left_join(posts::table)
        .filter(posts::id.is_null())
        .select(Author::as_select())
        .load(conn)
}
```

The first version loads `(Author, Option<Post>)` tuples and filters in Rust; an author with no posts yields exactly one `(author, None)` row. The bonus version pushes `WHERE posts.id IS NULL` into SQL, which is more efficient because the database filters before returning rows. Both compile against Diesel 2.3.

</details>
