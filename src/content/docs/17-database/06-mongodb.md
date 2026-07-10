---
title: "MongoDB with the Official Rust Driver"
description: "Use the official Rust mongodb crate like Mongoose or the Node driver: an async client, BSON, the doc! macro, and typed Collection<T> serialized through serde."
---

If you reach for Mongoose (or the raw `mongodb` Node driver) in your TypeScript projects, the official Rust `mongodb` crate will feel familiar: an async client, typed collections, and BSON documents. The big upgrade is that a Rust collection is generic over your document type, so serialization between Rust structs and BSON is handled by serde: checked at compile time, deserialized once at the boundary.

---

## Quick Overview

MongoDB's official Rust driver (`mongodb`) is **async** and integrates with **serde**: you parameterize a `Collection<T>` with your own struct, and the driver serializes inserts and deserializes query results through serde automatically. Documents are represented as **BSON** (MongoDB's binary JSON), which you build with the ergonomic `doc!` macro. For a TypeScript developer the surprises are pleasant. There is no separate schema layer like a Mongoose model; your `#[derive(Serialize, Deserialize)]` struct *is* the schema. And one sharp edge: a document whose stored shape disagrees with your struct fails at deserialization with a real error, instead of silently handing you `undefined`.

> **Note:** This file covers the document database side of Section 17: BSON, the `doc!` macro, CRUD, and typed collections via serde. The SQL world lives in [SQLx](/17-database/00-sqlx-intro/) and [Getting Started with Diesel (the TypeORM of Rust)](/17-database/03-diesel-intro/); key/value caching is in [Redis with the `redis` Crate](/17-database/07-redis/). MongoDB has no SQL-style migrations, so there is no migration counterpart here.

---

## TypeScript/JavaScript Example

A typical Node setup using the official `mongodb` driver: connect, get a typed collection, then run create/read/update/delete operations.

```typescript
// TypeScript with the official `mongodb` driver (npm install mongodb)
import { MongoClient, ObjectId, Collection } from "mongodb";

interface User {
  _id?: ObjectId;
  name: string;
  email: string;
  age: number;
  roles: string[];
  createdAt: Date;
}

async function main() {
  const client = new MongoClient("mongodb://localhost:27017");
  await client.connect();
  const db = client.db("ts2rust_demo");
  const users: Collection<User> = db.collection("users");

  // Create a unique index on email
  await users.createIndex({ email: 1 }, { unique: true });

  // Insert
  const { insertedId } = await users.insertOne({
    name: "Ada Lovelace",
    email: "ada@example.com",
    age: 36,
    roles: ["admin"],
    createdAt: new Date(),
  });
  console.log("inserted", insertedId);

  // Read one (returns null when nothing matches)
  const ada = await users.findOne({ email: "ada@example.com" });
  console.log(ada?.name); // "Ada Lovelace"

  // Read many, sorted
  const adults = await users
    .find({ age: { $gte: 18 } })
    .sort({ age: 1 })
    .toArray();
  console.log(adults.length);

  // Update
  const res = await users.updateOne(
    { email: "ada@example.com" },
    { $set: { age: 37 }, $push: { roles: "owner" } },
  );
  console.log(res.modifiedCount);

  // Delete
  await users.deleteOne({ email: "ada@example.com" });

  await client.close();
}

main();
```

**Key points:**

- The `User` interface is a *compile-time-only* hint. At runtime the driver hands back whatever BSON Mongo stored, cast to `User` — TypeScript does no validation, so a document with `age: "old"` would still be typed as `number` and blow up later.
- `findOne` resolves to `null` when there is no match.
- Filters and updates are plain JavaScript objects using Mongo operators (`$gte`, `$set`, `$push`).

---

## Rust Equivalent

In Rust the same flow uses a `Collection<User>` parameterized by your struct. The `doc!` macro builds BSON filters and updates, and serde does the struct↔BSON conversion. The example below is fully runnable against a local MongoDB (`docker run -d -p 27017:27017 mongo:7`).

```rust
// Rust with the official `mongodb` driver
use futures::stream::TryStreamExt; // brings `cursor.try_next()` into scope
use mongodb::bson::{doc, oid::ObjectId, DateTime};
use mongodb::options::{FindOptions, IndexOptions};
use mongodb::{Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    // Mongo's primary key is `_id`. We rename so Rust's `id` maps to it, and
    // skip it when serializing an insert so the server generates the ObjectId.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<ObjectId>,
    name: String,
    email: String,
    age: i32,
    #[serde(default)] // tolerate documents that predate this field
    roles: Vec<String>,
    created_at: DateTime,
}

#[tokio::main]
async fn main() -> mongodb::error::Result<()> {
    let client = Client::with_uri_str("mongodb://localhost:27017").await?;
    let db = client.database("ts2rust_demo");

    // Typed collection: every read deserializes into `User`, every write serializes from it.
    let users: Collection<User> = db.collection("users");

    // Unique index on email.
    let idx = IndexModel::builder()
        .keys(doc! { "email": 1 })
        .options(IndexOptions::builder().unique(true).build())
        .build();
    users.create_index(idx).await?;

    // Create
    let ada = User {
        id: None, // omitted on the wire; the server assigns an ObjectId
        name: "Ada Lovelace".to_string(),
        email: "ada@example.com".to_string(),
        age: 36,
        roles: vec!["admin".to_string()],
        created_at: DateTime::now(),
    };
    let res = users.insert_one(&ada).await?;
    let ada_id = res.inserted_id.as_object_id().unwrap();
    println!("inserted _id is 24 hex chars = {}", ada_id.to_hex().len());

    // Read one: `Option<User>` — `None` (not an error) when nothing matches.
    let found = users.find_one(doc! { "email": "ada@example.com" }).await?;
    if let Some(u) = found {
        println!("found one: {} <{}> age {}", u.name, u.email, u.age);
    }

    // Read many, sorted by age, draining a typed cursor.
    let opts = FindOptions::builder().sort(doc! { "age": 1 }).build();
    let mut cursor = users
        .find(doc! { "age": { "$gte": 18 } })
        .with_options(opts)
        .await?;
    let mut names = Vec::new();
    while let Some(u) = cursor.try_next().await? {
        names.push(format!("{} ({})", u.name, u.age));
    }
    println!("adults sorted by age: {names:?}");

    // Update
    let upd = users
        .update_one(
            doc! { "email": "ada@example.com" },
            doc! { "$set": { "age": 37 }, "$push": { "roles": "owner" } },
        )
        .await?;
    println!("matched={} modified={}", upd.matched_count, upd.modified_count);

    // Delete
    let del = users.delete_one(doc! { "email": "ada@example.com" }).await?;
    println!("deleted = {}", del.deleted_count);

    Ok(())
}
```

This compiles and runs with the following dependencies:

```toml
# Cargo.toml
[dependencies]
mongodb = "3.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
futures = "0.3" # for TryStreamExt, used to drain a cursor
```

> **Tip:** You do not add the `bson` crate separately. The `mongodb` crate re-exports a matching BSON version as `mongodb::bson`. Importing BSON types from `mongodb::bson` (rather than a standalone `bson` dependency) avoids the classic "two BSON versions in the tree" mismatch. The one exception is opting into the `chrono` integration (covered later): there you add `bson` explicitly, but with the version that matches the driver (`bson@2` for `mongodb` 3.7) so the tree stays unified.

Running the full version of this program (with all the print statements wired in) against a local MongoDB prints real values like:

```text
inserted _id is 24 hex chars = 24
found one: Ada Lovelace <ada@example.com> age 36
adults sorted by age: ["Ada Lovelace (36)"]
matched=1 modified=1
deleted = 1
```

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. The driver shown is `mongodb` 3.7.

---

## Detailed Explanation

### The client is cheap to clone and meant to be shared

`Client::with_uri_str(...)` returns a `Client` that owns an internal connection pool. Like a Node `MongoClient`, you create **one** per process and share it — cloning a `Client` is cheap (it is an `Arc` internally) and shares the same pool. There is no `client.close()` you must remember in normal app code; the pool is cleaned up when the last clone is dropped. (Connection-pool sizing and lifecycle across the whole section is covered in [Connection Pooling](/17-database/08-connection-pooling/).)

### `Collection<T>` is generic, and `T` drives serde

This is the heart of the ergonomic story. `db.collection::<User>("users")` produces a `Collection<User>`. From then on:

- `insert_one(&user)` **serializes** the `User` to BSON via serde.
- `find_one(...)` and the cursor from `find(...)` **deserialize** BSON back into `User`.

In TypeScript the `Collection<User>` generic is erased at runtime; it only annotates types and never inspects data. In Rust the generic is real: `User: Deserialize` is required at compile time, and the bytes are actually validated against the struct's shape when a document comes back. (TypeScript generics being erased while Rust monomorphizes is covered in [Section 09](/09-generics-traits/).)

### `_id`, `ObjectId`, and the optional id field

Mongo's primary key field is literally named `_id` and defaults to a 12-byte `ObjectId`. Two serde attributes make a Rust struct play nicely with it:

- `#[serde(rename = "_id")]` maps the Rust field `id` to the BSON key `_id`.
- `#[serde(skip_serializing_if = "Option::is_none")]` on `id: Option<ObjectId>` means inserts omit the field entirely when it is `None`, so the server generates the `ObjectId`. Reads populate it.

`insert_one` returns the generated id inside `res.inserted_id`, which is a `Bson` value; `.as_object_id()` extracts the `ObjectId`. To turn a hex string from a URL path into one, use `ObjectId::parse_str("...")`.

### The `doc!` macro builds BSON, not JSON

`doc! { "age": { "$gte": 18 } }` expands to a `mongodb::bson::Document`, an ordered map of `String` → `Bson`. It looks like the JavaScript object you would pass to the Node driver, but it is strongly typed: keys are strings and values are `Bson` variants (`Bson::Int32`, `Bson::String`, `Bson::Array`, ...). It is closer to `serde_json::json!` (see [Section 15](/15-serialization/)) than to a plain struct literal.

### Cursors are async streams; you need `TryStreamExt`

`find(...)` returns a `Cursor<User>`, which is an async stream. The idiomatic drain loop is `while let Some(item) = cursor.try_next().await? { ... }`. The `try_next` method comes from the `futures::stream::TryStreamExt` trait, which you must bring into scope. Forgetting that import is the single most common beginner error (see Common Pitfalls). The async-stream model here is the same lazy-future story from [Section 11: Async](/11-async/): nothing is fetched until you poll the cursor.

### Builders instead of options objects

The Node driver takes a plain object for options (`{ unique: true }`). The Rust driver uses the **builder pattern**: `IndexOptions::builder().unique(true).build()`, `FindOptions::builder().sort(...).limit(...).build()`. Each call to a fluent method like `.with_options(opts)` attaches them to an operation. This is a recurring Rust idiom for "lots of optional named parameters."

### Untyped access with `Collection<Document>`

When the shape is dynamic (logs, migrations, ad-hoc tooling) you can skip the struct and use `Collection<Document>`. You then read fields with typed getters such as `d.get_str("msg")` and `d.get_i32("n")`, each returning a `Result`. You can also convert between your structs and `Document` without touching the database using `mongodb::bson::to_document` / `from_document`:

```rust
use mongodb::bson::{self, doc, Bson, Document};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Task {
    title: String,
    done: bool,
    priority: i32,
}

fn main() {
    let t = Task { title: "Write docs".into(), done: false, priority: 1 };

    // serde struct -> BSON Document (no DB involved)
    let d: Document = bson::to_document(&t).unwrap();
    println!("to_document = {d}");

    // ...and back
    let back: Task = bson::from_document(d.clone()).unwrap();
    println!("round trip equal = {}", back == t);

    // typed getters on a Document return Result
    println!("title field = {:?}", d.get_str("title"));
    println!("priority    = {:?}", d.get_i32("priority"));

    // a single BSON value is the `Bson` enum
    let b: Bson = Bson::String("hi".into());
    println!("bson variant = {b}");
}
```

Real program output:

```text
to_document = { "title": "Write docs", "done": false, "priority": 1 }
round trip equal = true
title field = Ok("Write docs")
priority    = Ok(1)
bson variant = "hi"
```

---

## Key Differences

| Aspect                     | Node `mongodb` / Mongoose (TypeScript)        | `mongodb` crate (Rust)                                  |
| -------------------------- | --------------------------------------------- | ------------------------------------------------------- |
| Document type              | `interface` (erased) or Mongoose schema       | `struct` with `#[derive(Serialize, Deserialize)]`       |
| Collection typing          | `Collection<User>` is a hint only             | `Collection<User>` actually drives serde (monomorphized)|
| Runtime validation         | None by default; Mongoose adds optional schema| serde **deserialization** validates shape at the boundary|
| Filters / updates          | Plain JavaScript objects                      | `doc! { ... }` building a typed `Document`              |
| `findOne` miss             | resolves to `null`                            | `Ok(None)` (`None`, never an error)                     |
| Cursor iteration           | `await cursor.toArray()` / `for await`        | `while cursor.try_next().await?` (needs `TryStreamExt`) |
| Options                    | options object `{ unique: true }`             | builders: `IndexOptions::builder()...build()`           |
| Shape mismatch (e.g. type) | silent `undefined` → crash later              | a real, immediate deserialization error                 |
| BSON dates                 | JavaScript `Date`                             | `mongodb::bson::DateTime` (millis since epoch, UTC)     |

### Why this design

The driver leans on serde so the same `User` struct that flows through your HTTP layer (see [Section 16](/16-web-apis/)) is also your database model: no second schema definition, no Mongoose-style hydration step. Because the conversion is real code rather than a type annotation, MongoDB's famously flexible documents become *checked* at the moment they enter your program: if the database holds a string where your struct expects an `i32`, you find out at that boundary with a precise error, instead of carrying a wrongly-typed value deep into your logic the way an erased TypeScript interface would. The trade-off is honesty for flexibility: you decide per field how lenient to be (`Option<T>`, `#[serde(default)]`, custom deserializers).

### `bson::DateTime` is not `chrono::DateTime`

Mongo stores dates as 64-bit millisecond timestamps, surfaced as `mongodb::bson::DateTime`. It is deliberately small and Mongo-specific. Out of the box, `DateTime::now()` and the millisecond accessors cover most needs. If you want the richer `chrono` API, this is the one case where you add the `bson` crate explicitly: bring in the version that matches the driver with its `chrono-0_4` feature (`cargo add bson@2 --features chrono-0_4`, since `mongodb` 3.7 re-exports `bson` 2.15), then call `dt.to_chrono()`. Note that `to_chrono()` is gated behind that feature — with only a bare `mongodb = "3.7"` dependency the call does not compile (`error[E0599]: no method named to_chrono found for struct mongodb::bson::DateTime`). Do not reach for `std::time::SystemTime` in your documents; it has no canonical BSON mapping.

---

## Common Pitfalls

### Pitfall 1: Forgetting `use futures::stream::TryStreamExt`

`find(...)` gives you a cursor, but `cursor.try_next()` is a trait method. Without the import the code looks correct yet does not compile:

```rust
// does not compile (error[E0599]): TryStreamExt is not in scope
use mongodb::bson::doc;
use mongodb::bson::Document;
use mongodb::{Client, Collection};
// missing: use futures::stream::TryStreamExt;

#[tokio::main]
async fn main() -> mongodb::error::Result<()> {
    let client = Client::with_uri_str("mongodb://localhost:27017").await?;
    let coll: Collection<Document> = client.database("d").collection("c");
    let mut cursor = coll.find(doc! {}).await?;
    while let Some(_doc) = cursor.try_next().await? {} // method not found
    Ok(())
}
```

The real compiler error names the fix:

```text
error[E0599]: no method named `try_next` found for struct `mongodb::Cursor` in the current scope
   --> src/main.rs:11:35
    |
 11 |     while let Some(_doc) = cursor.try_next().await? {} // method not found
    |                                   ^^^^^^^^
    |
   ::: .../futures-util-0.3.32/src/stream/try_stream/mod.rs:404:8
    |
404 |     fn try_next(&mut self) -> TryNext<'_, Self>
    |        -------- the method is available for `mongodb::Cursor<mongodb::bson::Document>` here
    |
    = help: items from traits can only be used if the trait is in scope
help: trait `TryStreamExt` which provides `try_next` is implemented but not in scope; perhaps you want to import it
    |
  1 + use futures_util::stream::try_stream::TryStreamExt;
    |
help: there is a method `next` with a similar name
    |
 11 -     while let Some(_doc) = cursor.try_next().await? {} // method not found
 11 +     while let Some(_doc) = cursor.next().await? {} // method not found
    |
```

> **Note:** The compiler suggests `futures_util::...`, which works, but the idiomatic import is `use futures::stream::TryStreamExt;` (the `futures` crate re-exports it). Either resolves the error.

### Pitfall 2: Expecting `find_one` to throw on "not found"

Coming from `findOne` returning `null`, it is tempting to treat a missing document as an error. In Rust `find_one` returns `Result<Option<User>>`: the `Err` case is reserved for *actual failures* (network, auth), and "no match" is `Ok(None)`. Use a `match` or `if let Some(...)`, and reserve `?` for genuine errors. This mirrors the `Option`/`Result` split used everywhere in the guide ([Section 08: Error Handling](/08-error-handling/)).

### Pitfall 3: A stored document that disagrees with your struct

This is where MongoDB's flexibility meets Rust's strictness. Suppose a document was written with `age` as a string, but your struct says `age: i32`:

```rust
// The collection holds: { "name": "Bob", "age": "old" }
// but the struct expects an i32 for `age`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Account {
    name: String,
    age: i32,
}
// let typed: Collection<Account> = db.collection("acct");
// typed.find_one(doc! { "name": "Bob" }).await  // -> Err(...)
```

The driver returns a real, descriptive error (not a panic, not a silent `undefined`):

```text
ERROR: Kind: invalid type: string "old", expected i32, labels: {}, ...
```

To tolerate such documents, model the field as `Option<String>`, add a custom serde deserializer, or first read as `Collection<Document>` and coerce manually. The point is you are *told* — unlike the TypeScript version, where the value is typed `number` but is really a string until something downstream breaks.

### Pitfall 4: Inserting with a populated `_id` you did not mean to set

If `id` is `Some(...)` and you do not use `skip_serializing_if`, the insert sends that exact `_id`. Two inserts with the same id collide. Always either keep `id: None` for new documents (with `skip_serializing_if = "Option::is_none"`), or let a dedicated write struct omit the field entirely: the same read-model/write-model split shown for Diesel in [Getting Started with Diesel (the TypeORM of Rust)](/17-database/03-diesel-intro/).

### Pitfall 5: Assuming the driver clusters writes for you

`insert_many(&docs)` is one round trip, but `update_many`/`delete_many` apply a *single* filter to many documents; they are not "loop and write each struct." To update many *different* documents efficiently, build a bulk write or iterate deliberately. There is no Mongoose-style change tracking that flushes dirty objects on `save()`.

---

## Best Practices

- **Reuse one `Client` per process and clone it.** It is `Arc`-backed and owns the pool. Construct it once at startup, store it in your app state, and clone freely into handlers. See [Connection Pooling](/17-database/08-connection-pooling/).
- **Import BSON from `mongodb::bson`.** This guarantees the BSON version matches the driver and avoids a duplicate-`bson` dependency in your tree.
- **Make your structs serde-resilient.** Use `Option<T>` for fields that may be absent, `#[serde(default)]` for fields added later, and `#[serde(rename = "_id")]` for the primary key. Flexible schemas are a feature; model the flexibility explicitly.
- **Prefer typed `Collection<T>` over `Collection<Document>`** for your domain data, and reserve `Document` for genuinely dynamic shapes (logs, tooling, aggregation outputs).
- **Use builders for options and create indexes at startup.** Encode `unique`, TTL, and compound indexes in code (`IndexModel`) so they are versioned with the app rather than applied by hand.
- **Atomic mutations belong in the update document.** Prefer `$inc`, `$set`, `$push`, and `find_one_and_update` with `ReturnDocument::After` over read-modify-write in application code; the latter races under concurrency. (For multi-document atomicity you would use transactions; the SQL analog is in [Transactions with SQLx](/17-database/02-sqlx-transactions/).)
- **Keep the database model and the API model close.** Because both go through serde, the `User` you store can often be the `User` you serialize to JSON for an Axum handler ([Section 16](/16-web-apis/)) — but split them when storage and API shapes legitimately diverge.

---

## Real-World Example

A small inventory repository, the kind of module you would put behind a web handler. It wraps a typed `Collection<Product>`, exposes intention-revealing methods, and uses `find_one_and_update` with `$inc` to reserve stock **atomically** (the filter `in_stock >= qty` plus the decrement happen in one operation, so two concurrent reservations cannot oversell). It finishes with an aggregation that sums inventory value. Fully runnable against a local MongoDB.

```rust
// Cargo.toml:
// [dependencies]
// mongodb = "3.7"
// tokio = { version = "1", features = ["full"] }
// serde = { version = "1", features = ["derive"] }
// futures = "0.3"

use futures::stream::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId, Bson, DateTime};
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};
use mongodb::{Client, Collection, Database};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Product {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<ObjectId>,
    sku: String,
    name: String,
    price_cents: i64,
    in_stock: i32,
    updated_at: DateTime,
}

struct ProductRepo {
    coll: Collection<Product>,
}

impl ProductRepo {
    fn new(db: &Database) -> Self {
        Self { coll: db.collection("products") }
    }

    async fn create(
        &self,
        sku: &str,
        name: &str,
        price_cents: i64,
        in_stock: i32,
    ) -> mongodb::error::Result<ObjectId> {
        let product = Product {
            id: None,
            sku: sku.to_string(),
            name: name.to_string(),
            price_cents,
            in_stock,
            updated_at: DateTime::now(),
        };
        let res = self.coll.insert_one(&product).await?;
        Ok(res.inserted_id.as_object_id().expect("server returns an ObjectId"))
    }

    async fn by_sku(&self, sku: &str) -> mongodb::error::Result<Option<Product>> {
        self.coll.find_one(doc! { "sku": sku }).await
    }

    /// Atomically decrement stock and return the *updated* document.
    /// The `in_stock >= qty` filter is part of the same operation, so an
    /// over-reservation simply matches nothing and returns `None`.
    async fn reserve(&self, sku: &str, qty: i32) -> mongodb::error::Result<Option<Product>> {
        let opts = FindOneAndUpdateOptions::builder()
            .return_document(ReturnDocument::After)
            .build();
        self.coll
            .find_one_and_update(
                doc! { "sku": sku, "in_stock": { "$gte": qty } },
                doc! {
                    "$inc": { "in_stock": -qty },
                    "$currentDate": { "updated_at": true }
                },
            )
            .with_options(opts)
            .await
    }
}

#[tokio::main]
async fn main() -> mongodb::error::Result<()> {
    let client = Client::with_uri_str("mongodb://localhost:27017").await?;
    let db = client.database("ts2rust_shop");

    let repo = ProductRepo::new(&db);
    let id = repo.create("KB-01", "Mechanical Keyboard", 7999, 5).await?;
    println!("created id = {id}");

    let product = repo.by_sku("KB-01").await?.expect("just created it");
    println!(
        "by_sku: {} @ {} cents, stock {}",
        product.name, product.price_cents, product.in_stock
    );

    let after = repo.reserve("KB-01", 2).await?.expect("enough stock");
    println!("after reserving 2: stock {}", after.in_stock);

    // Over-reservation matches no document, so we get None — never overselling.
    let none = repo.reserve("KB-01", 100).await?;
    println!("over-reserve returned None = {}", none.is_none());

    // Aggregation pipeline: total inventory value across all products.
    let mut cursor = repo
        .coll
        .aggregate(vec![doc! {
            "$group": {
                "_id": Bson::Null,
                "total": { "$sum": { "$multiply": ["$price_cents", "$in_stock"] } }
            }
        }])
        .await?;
    if let Some(d) = cursor.try_next().await? {
        println!("inventory value (cents) = {}", d.get_i64("total").unwrap_or(0));
    }

    Ok(())
}
```

Real program output (the `_id` is a fresh `ObjectId`, so its hex value varies per run; `3 × 7999 = 23997`):

```text
created id = 6a1d7433288945ef596f5e94
by_sku: Mechanical Keyboard @ 7999 cents, stock 5
after reserving 2: stock 3
over-reserve returned None = true
inventory value (cents) = 23997
```

Every method returns `mongodb::error::Result<T>`, so callers compose failures with `?`, and "not found" stays an honest `None` instead of a thrown exception: the same error story as the rest of your Rust code, and the opposite of Mongoose where a missing document and a connection failure both surface as a rejected promise you have to disambiguate by hand.

> **Tip:** To configure the connection (pool size, timeouts, app name) parse the URI into `ClientOptions` first:
>
> ```rust
> use mongodb::{Client, options::ClientOptions};
> use std::time::Duration;
>
> async fn connect() -> mongodb::error::Result<Client> {
>     let mut opts = ClientOptions::parse("mongodb://localhost:27017").await?;
>     opts.app_name = Some("ts2rust-demo".to_string());
>     opts.max_pool_size = Some(20);
>     opts.connect_timeout = Some(Duration::from_secs(5));
>     Client::with_options(opts)
> }
> ```

---

## Further Reading

- [`mongodb` crate on docs.rs](https://docs.rs/mongodb): `Client`, `Collection<T>`, `Cursor`, and the operation builders.
- [MongoDB Rust Driver guide](https://www.mongodb.com/docs/drivers/rust/current/) — official tutorials for CRUD, aggregation, and indexes.
- [`bson` crate docs](https://docs.rs/bson): the `doc!` macro, the `Bson` enum, `ObjectId`, `DateTime`, and serde integration.
- Sibling topics in this section:
  - [SQLx intro](/17-database/00-sqlx-intro/) and [SQLx queries](/17-database/01-sqlx-queries/) — async, compile-time-checked SQL when your data is relational.
  - [Diesel intro](/17-database/03-diesel-intro/): read-model/write-model structs and a typed query DSL for SQL.
  - [Redis](/17-database/07-redis/) — key/value caching and counters alongside your document store.
  - [Connection pooling](/17-database/08-connection-pooling/): sharing one `Client`, sizing pools, and lifecycle.
  - [ORM comparison](/17-database/10-orm-comparison/) — where a document driver fits next to SQLx, Diesel, and SeaORM.
- Background from earlier sections:
  - [Section 15: Serialization](/15-serialization/) — serde, `#[serde(rename/default)]`, and `serde_json::json!` (BSON's cousin).
  - [Section 11: Async](/11-async/): why cursors are lazy streams and need a runtime.
  - [Section 09: Generics & Traits](/09-generics-traits/) — why `Collection<T>` is real at runtime, unlike TypeScript generics.
  - [Section 08: Error Handling](/08-error-handling/): `Result`, `Option`, and `?`.
- Next section: [Section 18: CLI Tools](/18-cli-tools/) — wrap these queries in a database-admin CLI.

---

## Exercises

### Exercise 1: A typed `Note` collection

**Difficulty:** Beginner

**Objective:** Define a serde-mapped document model and round-trip it through a typed collection.

**Instructions:** Define a `Note` struct with an optional `_id` (renamed and skipped when `None`), a `title`, a `body`, a `Vec<String>` of `tags`, and a `created_at: DateTime`. Insert five notes, then `find_one` the note titled `"Note 1"` and print its title.

<details>
<summary>Solution</summary>

```rust
use mongodb::bson::{doc, oid::ObjectId, DateTime};
use mongodb::{Client, Collection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Note {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<ObjectId>,
    title: String,
    body: String,
    tags: Vec<String>,
    created_at: DateTime,
}

#[tokio::main]
async fn main() -> mongodb::error::Result<()> {
    let client = Client::with_uri_str("mongodb://localhost:27017").await?;
    let notes: Collection<Note> = client.database("ts2rust_ex").collection("notes");

    for i in 1..=5 {
        notes
            .insert_one(&Note {
                id: None,
                title: format!("Note {i}"),
                body: format!("body {i}"),
                tags: vec![],
                created_at: DateTime::now(),
            })
            .await?;
    }

    let first = notes.find_one(doc! { "title": "Note 1" }).await?.unwrap();
    println!("found: {}", first.title); // found: Note 1
    Ok(())
}
```

</details>

### Exercise 2: Pagination with `skip` and `limit`

**Difficulty:** Intermediate

**Objective:** Build a paginated query and drain the cursor into a `Vec`.

**Instructions:** Write `async fn page(coll: &Collection<Note>, page: u64, per: i64) -> mongodb::error::Result<Vec<String>>` that returns the titles of one page of notes, sorted by `created_at` ascending, using `FindOptions` with `.skip(page * per).limit(per)`. Print page 0 and page 1 with two items each.

<details>
<summary>Solution</summary>

```rust
use futures::stream::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use mongodb::Collection;
// (assumes the `Note` struct and a populated `notes` collection from Exercise 1)

async fn page(
    coll: &Collection<Note>,
    page: u64,
    per: i64,
) -> mongodb::error::Result<Vec<String>> {
    let opts = FindOptions::builder()
        .sort(doc! { "created_at": 1 })
        .skip(page * per as u64)
        .limit(per)
        .build();

    let mut cursor = coll.find(doc! {}).with_options(opts).await?;
    let mut titles = Vec::new();
    while let Some(note) = cursor.try_next().await? {
        titles.push(note.title);
    }
    Ok(titles)
}

// Calling page(&notes, 0, 2) then page(&notes, 1, 2) prints:
//   ex2 page0 = ["Note 1", "Note 2"]
//   ex2 page1 = ["Note 3", "Note 4"]
```

The verified output for a five-note collection is `["Note 1", "Note 2"]` for page 0 and `["Note 3", "Note 4"]` for page 1.

</details>

### Exercise 3: Upsert by a natural key

**Difficulty:** Intermediate

**Objective:** Use `update_one` with `upsert(true)` so a write either updates an existing document or inserts a new one, and detect which happened.

**Instructions:** Write `async fn upsert_note(coll: &Collection<Note>, title: &str, body: &str) -> mongodb::error::Result<bool>` that matches on `title`, `$set`s the `body`, and uses `$setOnInsert` to populate `title`/`tags`/`created_at` only when inserting. Return `true` when a brand-new document was created. Call it once for an existing title and once for a new one.

<details>
<summary>Solution</summary>

```rust
use mongodb::bson::{doc, DateTime};
use mongodb::options::UpdateOptions;
use mongodb::Collection;
// (assumes the `Note` struct and a populated `notes` collection)

async fn upsert_note(
    coll: &Collection<Note>,
    title: &str,
    body: &str,
) -> mongodb::error::Result<bool> {
    let res = coll
        .update_one(
            doc! { "title": title },
            doc! {
                "$set": { "body": body },
                "$setOnInsert": {
                    "title": title,
                    "tags": [],
                    "created_at": DateTime::now()
                }
            },
        )
        .with_options(UpdateOptions::builder().upsert(true).build())
        .await?;

    // `upserted_id` is Some(...) only when a new document was inserted.
    Ok(res.upserted_id.is_some())
}

// upsert_note(&notes, "Note 1", "edited")  -> Ok(false)  (updated existing)
// upsert_note(&notes, "Brand New", "fresh") -> Ok(true)   (inserted new)
```

`$setOnInsert` is the key detail: it applies its fields only on the insert branch, so updating an existing note never resets its `created_at`. The verified output is `false` for the first call and `true` for the second.

</details>
