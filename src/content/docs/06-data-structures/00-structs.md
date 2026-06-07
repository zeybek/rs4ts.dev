---
title: "Structs: Modeling Data the Rust Way"
description: "Rust structs model data like a TypeScript interface, but with concrete field types, owned fields that move, and opt-in #[derive] for printing and cloning."
---

In TypeScript you reach for an `interface` or an object literal to describe shaped data. In Rust the equivalent tool is the **struct**, a named type with named fields. This page covers defining structs, instantiating them, who *owns* each field, and the `#[derive(...)]` attribute that brings TypeScript-like conveniences such as printing and copying.

---

## Quick Overview

A **struct** is Rust's named record type: a fixed set of named, typed fields grouped under one name, the closest analog to a TypeScript `interface` plus an object literal. The big shifts for a TypeScript developer are that **every field has a concrete type**, the **struct owns its fields** (so moving the struct moves the data inside it), and "free" behaviors like debug-printing or copying are **opt-in via `#[derive(...)]`** rather than automatic.

---

## TypeScript/JavaScript Example

```typescript
// TypeScript - an interface describes the shape; an object literal fills it in
interface User {
  id: number;
  username: string;
  email: string;
  active: boolean;
  loginCount: number;
}

const user: User = {
  id: 1,
  username: "alice",
  email: "alice@example.com",
  active: true,
  loginCount: 0,
};

// Field access with dot notation
console.log(user.username); // "alice"

// Objects are mutable by default
user.loginCount += 1;

// Printing the whole object "just works"
console.log(user);
// Node wraps multi-property objects across lines:
// {
//   id: 1,
//   username: 'alice',
//   email: 'alice@example.com',
//   active: true,
//   loginCount: 1
// }

// "Copying" with spread (shallow!)
const clone = { ...user };
```

**Key points:**

- `interface` describes the shape; it is **erased at runtime** (no trace of `User` exists when the program runs).
- Object fields are **mutable by default**, and any reference can mutate them.
- `console.log(obj)` prints field names and values automatically.
- `{ ...user }` makes a **shallow** copy: nested objects/arrays are still shared by reference.

> **Note:** A TypeScript `interface` and a runtime object are two separate things. The interface is a compile-time contract; the object literal is the actual data. A Rust `struct` fuses both ideas into one item that exists at both compile time *and* runtime.

---

## Rust Equivalent

```rust
// Rust - one `struct` item defines both the type AND the runtime layout
#[derive(Debug, Clone)]
struct User {
    id: u64,
    username: String,
    email: String,
    active: bool,
    login_count: u32,
}

fn main() {
    // Instantiation: every field must be given a value, by name
    let mut user = User {
        id: 1,
        username: String::from("alice"),
        email: String::from("alice@example.com"),
        active: true,
        login_count: 0,
    };

    // Field access with dot notation (same as TS)
    println!("{}", user.username); // alice

    // Mutation requires the binding to be `mut` (the whole `user`, not the field)
    user.login_count += 1;

    // `#[derive(Debug)]` makes the whole struct printable with {:?}
    println!("{:?}", user);

    // `#[derive(Clone)]` gives us an explicit, deep copy
    let snapshot = user.clone();
    println!("{:?}", snapshot);
}
```

Running this prints:

```text
alice
User { id: 1, username: "alice", email: "alice@example.com", active: true, login_count: 1 }
User { id: 1, username: "alice", email: "alice@example.com", active: true, login_count: 1 }
```

**Key points:**

- Each field has a **concrete type** (`u64`, `String`, `bool`...), not a single `number`/`string` catch-all.
- Instantiation must provide **every** field: there is no partial object.
- Mutation is gated by `let mut`, matching Rust's immutable-by-default rule from [Section 02: Variables](/02-basics/00-variables/).
- Printing and copying are **not free**: you opt in with `#[derive(Debug)]` and `#[derive(Clone)]`.

---

## Detailed Explanation

### Defining a struct

```rust
#[derive(Debug, Clone)]
struct User {
    id: u64,
    username: String,
    email: String,
    active: bool,
    login_count: u32,
}
```

Line by line:

- `#[derive(Debug, Clone)]` is an **attribute** that asks the compiler to auto-generate trait implementations (more below). It is the rough analog of a decorator placed on the type — but mechanically very different, since it generates real code at compile time rather than wrapping a runtime value.
- `struct User { ... }` declares a type named `User`. By convention struct names are `UpperCamelCase`.
- Each field is `name: Type`. Field names are `snake_case` by convention, so the TypeScript `loginCount` becomes `login_count`.
- Fields are **comma-separated** (a trailing comma after the last field is idiomatic).
- The fields are stored **inline** in the struct's memory. A `User` is exactly the bytes of its fields packed together (the compiler may reorder them for alignment); there is no per-object hash map or hidden class as in a JavaScript engine.

This `struct` definition is the entire equivalent of the TypeScript `interface` *and* it describes the runtime memory layout. There is no separate "object" concept: the struct *is* the type and the data.

### Instantiation

```rust
let user = User {
    id: 1,
    username: String::from("alice"),
    email: String::from("alice@example.com"),
    active: true,
    login_count: 0,
};
```

- You name the struct, then give a value for **every field** inside braces. Order does not matter (you write fields by name), but **none may be omitted**: there is no `Partial<User>` and no `undefined` to fall back on.
- `String::from("alice")` creates an **owned, heap-allocated** `String`. The string literal `"alice"` on its own is a `&str` (a borrowed string slice), which is a different type — we convert it to an owned `String` so the `User` can own its own copy. (Owned vs borrowed strings are covered in [Section 05: Ownership](/05-ownership/).)
- Numeric literals like `1` and `0` are coerced to the declared field types (`u64`, `u32`).

### Field access and mutation

```rust
let mut user = /* ... */;
println!("{}", user.username); // read a field
user.login_count += 1;         // mutate a field
```

Reading uses `.` exactly like TypeScript. **Mutation** is where Rust differs sharply: you cannot change any field unless the **whole binding** is declared `let mut`. Rust has no notion of marking a single field mutable while the rest are frozen. Mutability is a property of the *binding*, not the individual field.

### `#[derive(Debug, Clone)]` — buying back TypeScript conveniences

In TypeScript, `console.log(obj)` and `{ ...obj }` work on any object for free. In Rust those behaviors are **traits** you opt into:

- **`Debug`** enables the `{:?}` (and pretty `{:#?}`) format specifiers in `println!`/`format!`. Without it, you simply cannot debug-print the struct.
- **`Clone`** provides a `.clone()` method that makes a **deep, explicit** copy. Unlike the implicit, shallow `{ ...obj }`, a Rust `.clone()` recursively clones owned fields (a cloned `User` gets its *own* `String`s), and it never happens silently: you must call it.

`#[derive(...)]` tells the compiler to generate these implementations automatically based on the fields. You can list several traits at once. We will revisit traits and deriving in depth in [Section 09: Generics & Traits](/09-generics-traits/).

### Pretty-printing

`{:#?}` produces an indented, multi-line view, handy for nested data:

```rust
println!("{:#?}", user);
```

```text
User {
    id: 1,
    username: "alice",
    email: "alice@example.com",
    active: true,
    login_count: 1,
}
```

---

## Ownership of Fields

This is the concept with no TypeScript counterpart, and it is the heart of why structs behave the way they do.

**A struct owns its fields.** When you put a `String`, a `Vec`, or another owned value into a struct field, the struct becomes responsible for that data. The struct's lifetime governs the field's lifetime: when the struct is dropped, its owned fields are dropped too.

This has direct consequences:

### Moving the whole struct moves its fields

A `User` is not a reference to data living elsewhere (as a JavaScript object variable is). The `User` *is* the data. Assigning it elsewhere or passing it to a function by value **moves** it, just like any other owned value (see [Section 05: Ownership](/05-ownership/)).

### Moving a single field out (partial move)

You can move one owned field out of a struct. After that, the moved field is gone, but the *other* fields remain usable:

```rust
struct Profile {
    name: String,
    bio: String,
}

fn main() {
    let profile = Profile {
        name: String::from("Ada"),
        bio: String::from("Mathematician"),
    };

    // Move one owned field out of the struct
    let name = profile.name;
    println!("name = {}", name); // name = Ada

    // Fields that were NOT moved are still usable
    println!("bio = {}", profile.bio); // bio = Mathematician

    // But `profile` as a whole, and `profile.name`, can no longer be used.
}
```

This prints:

```text
name = Ada
bio = Mathematician
```

After `let name = profile.name;`, the `name` field has been *moved out*. Reading `profile.name` again is a compile error (shown in [Common Pitfalls](#common-pitfalls)). This **partial move** behavior simply does not exist in TypeScript, where assigning `const n = profile.name` copies a string value and leaves `profile` fully intact.

> **Note:** Fields whose types implement the `Copy` trait — small `Copy` types like `u64`, `bool`, `char` — are *copied* out instead of moved, so reading them never invalidates the struct. Moving only applies to non-`Copy` owned types like `String` and `Vec<T>`. We cover `Copy` vs move in [Section 05: Ownership](/05-ownership/).

### Owned fields vs borrowed fields

The structs on this page hold **owned** fields (`String`, `u64`, `Vec<T>`), which is the common, beginner-friendly default. The struct carries its own data and has no lifetime strings attached. A struct *can* instead hold a **reference** (e.g. `&str`), but that requires a **lifetime annotation** so the compiler can prove the borrowed data outlives the struct. That is an advanced topic; for now, **prefer owned fields**. The pitfalls section shows the exact error you get if you try `&str` without a lifetime.

---

## Key Differences

| Aspect | TypeScript `interface` / object | Rust `struct` |
| --- | --- | --- |
| Exists at runtime? | Interface erased; object is a runtime value | Struct is one item: compile-time type **and** runtime layout |
| Field types | `number`, `string`, ... (broad) | Concrete: `u64`, `f64`, `String`, ... |
| All fields required at creation | No (`?` optional fields, `undefined`) | Yes: every field must be initialized |
| Mutability | Mutable by default | Immutable unless binding is `let mut` |
| Per-field mutability | Not a concept (all fields mutable) | Not a concept (whole binding is `mut` or not) |
| Printing | `console.log` works automatically | Opt in with `#[derive(Debug)]`, use `{:?}` |
| Copying | `{ ...obj }` (shallow, implicit) | `.clone()` (deep, explicit) with `#[derive(Clone)]` |
| Memory | Object on the heap, variable holds a reference | Struct stored inline; owns its fields |
| Identity | Variable is a handle; assignment shares it | Value semantics; assignment **moves** it |

The mental-model shift: a TypeScript object variable is a *handle* pointing at heap data, and assigning the variable just copies the handle (both names see the same object). A Rust struct value *is* the data, and assigning it **moves** ownership: the old binding can no longer be used.

---

## Common Pitfalls

### Pitfall 1: Using a struct (or field) after moving a field out

```rust
struct Profile {
    name: String,
    bio: String,
}

fn main() {
    let profile = Profile {
        name: String::from("Ada"),
        bio: String::from("Mathematician"),
    };

    let name = profile.name; // moves `name` field out
    println!("{}", name);
    println!("{}", profile.name); // does not compile (error[E0382]: borrow of moved value)
}
```

Real compiler output:

```text
error[E0382]: borrow of moved value: `profile.name`
  --> src/main.rs:14:20
   |
12 |     let name = profile.name; // moves `name` field out
   |                ------------ value moved here
13 |     println!("{}", name);
14 |     println!("{}", profile.name); // does not compile (error[E0382]: borrow of moved value)
   |                    ^^^^^^^^^^^^ value borrowed here after move
   |
   = note: move occurs because `profile.name` has type `String`, which does not implement the `Copy` trait
```

**Fix:** Either clone the field (`let name = profile.name.clone();`) if you still need the original, or borrow it (`let name = &profile.name;`) instead of taking ownership.

### Pitfall 2: Forgetting a field at instantiation

Coming from TypeScript, you might expect missing fields to default to `undefined`. Rust requires all of them:

```rust
struct User {
    id: u64,
    username: String,
    email: String,
}

fn main() {
    let user = User {
        id: 1,
        username: String::from("alice"),
    }; // does not compile (error[E0063]: missing field `email`)
    println!("{}", user.username);
}
```

Real compiler output:

```text
error[E0063]: missing field `email` in initializer of `User`
 --> src/main.rs:8:16
  |
8 |     let user = User {
  |                ^^^^ missing `email`
```

**Fix:** Provide every field. If you want optional-ish behavior, make the field an `Option<T>` (see [Option Enum](/06-data-structures/03-option-enum/)) or derive `Default` and use [struct update syntax](/06-data-structures/08-field-init-shorthand/).

### Pitfall 3: Printing a struct that does not derive `Debug`

```rust
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let p = Point { x: 1, y: 2 };
    println!("{:?}", p); // does not compile (error[E0277]: `Point` doesn't implement `Debug`)
}
```

Real compiler output (note the helpful suggestion):

```text
error[E0277]: `Point` doesn't implement `Debug`
 --> src/main.rs:8:22
  |
8 |     println!("{:?}", p); // does not compile (error[E0277]: `Point` doesn't implement `Debug`)
  |               ----   ^ `Point` cannot be formatted using `{:?}` because it doesn't implement `Debug`
  |               |
  |               required by this formatting parameter
  |
  = help: the trait `Debug` is not implemented for `Point`
  = note: add `#[derive(Debug)]` to `Point` or manually `impl Debug for Point`
help: consider annotating `Point` with `#[derive(Debug)]`
  |
1 + #[derive(Debug)]
2 | struct Point {
```

**Fix:** Add `#[derive(Debug)]` above the struct. (`{}` — the `Display` format — is *not* auto-derivable; you would implement `Display` by hand for user-facing output.)

### Pitfall 4: Mutating a field without `mut`

```rust
struct Counter {
    value: u32,
}

fn main() {
    let counter = Counter { value: 0 };
    counter.value += 1; // does not compile (error[E0594]: cannot assign)
    println!("{}", counter.value);
}
```

Real compiler output:

```text
error[E0594]: cannot assign to `counter.value`, as `counter` is not declared as mutable
 --> src/main.rs:7:5
  |
7 |     counter.value += 1; // does not compile (error[E0594]: cannot assign)
  |     ^^^^^^^^^^^^^^^^^^ cannot assign
  |
help: consider changing this to be mutable
  |
6 |     let mut counter = Counter { value: 0 };
  |         +++
```

**Fix:** Declare `let mut counter = ...`. Mutability lives on the binding, not the field.

### Pitfall 5: Trying to mark an individual field `mut`

There is no per-field `mut` keyword. This is a parse error, not a borrow error:

```rust
struct Config {
    mut retries: u32, // does not compile (expected identifier, found keyword `mut`)
}
```

Real compiler output:

```text
error: expected identifier, found keyword `mut`
 --> src/main.rs:2:5
  |
1 | struct Config {
  |        ------ while parsing this struct
2 |     mut retries: u32, // no per-field mut in Rust
  |     ^^^ expected identifier, found keyword
```

**Fix:** Remove `mut` from the field. Control mutability at the binding (`let mut config = ...`). If you need genuinely interior mutability (mutating through a shared reference), that is what `Cell`/`RefCell` are for; see [Section 10: Smart Pointers](/10-smart-pointers/).

### Pitfall 6: A reference field without a lifetime

```rust
struct Borrowed {
    name: &str, // does not compile (error[E0106]: missing lifetime specifier)
}
```

Real compiler output:

```text
error[E0106]: missing lifetime specifier
 --> src/main.rs:2:11
  |
2 |     name: &str, // needs a lifetime annotation
  |           ^ expected named lifetime parameter
  |
help: consider introducing a named lifetime parameter
  |
1 ~ struct Borrowed<'a> {
2 ~     name: &'a str, // needs a lifetime annotation
```

**Fix while you are learning:** use an **owned** field — `name: String` — so the struct owns its data and needs no lifetime. Lifetimes for borrowed fields are an advanced topic.

---

## Best Practices

### 1. Prefer owned fields (`String`, `Vec<T>`) over borrowed ones

Owned fields keep your structs self-contained and free of lifetime parameters. Reach for `&str`/`&[T]` fields only when you have measured a real need and understand lifetimes.

```rust
struct Article {
    title: String,        // owned, simple
    tags: Vec<String>,    // owned collection
}
```

### 2. Derive the traits you actually need — start with `Debug`

`#[derive(Debug)]` should be on virtually every struct: it costs nothing at runtime if unused and makes debugging vastly easier. Add `Clone` when you genuinely need copies, `PartialEq` for `==`, and `Default` for "empty" values:

```rust
#[derive(Debug, Clone, PartialEq, Default)]
struct Settings {
    dark_mode: bool,
    font_size: u32,
    language: String,
}

fn main() {
    let defaults = Settings::default();
    println!("{:?}", defaults);

    let custom = Settings {
        dark_mode: true,
        font_size: 16,
        language: String::from("en"),
    };

    println!("equal? {}", defaults == custom);
    println!("equal to self clone? {}", custom == custom.clone());
}
```

Output:

```text
Settings { dark_mode: false, font_size: 0, language: "" }
equal? false
equal to self clone? true
```

> **Tip:** `#[derive(Default)]` fills each field with its type's default (`false`, `0`, empty `String`). It is the idiomatic alternative to TypeScript's "all-optional with defaults" object, and it is checked at compile time.

### 3. Name things idiomatically

`UpperCamelCase` for the struct, `snake_case` for fields. Clippy will nudge you if you stray. Translate TypeScript's `loginCount` to `login_count`.

### 4. Use descriptive types for fields

Prefer specific integer types (`u64` for an id you will never make negative) over reaching for the largest type by default. The type system documents intent — see [Section 02: Basic Types](/02-basics/01-types/).

### 5. Don't reach for `.clone()` reflexively

A `.clone()` of a struct with `String`/`Vec` fields is a real, deep allocation. Coming from JavaScript where `{ ...obj }` is cheap-feeling, it is tempting to clone away borrow-checker complaints. Often borrowing (`&user`) is what you actually want. Use `.clone()` deliberately.

---

## Real-World Example

A small e-commerce domain model assembled entirely from structs: note nested structs, a `Vec` of structs, owned `String` fields, and an explicit `.clone()` that produces an independent snapshot.

```rust
// A small e-commerce domain model built from structs.

#[derive(Debug, Clone)]
struct LineItem {
    sku: String,
    description: String,
    quantity: u32,
    unit_price_cents: u64,
}

#[derive(Debug, Clone)]
struct Address {
    street: String,
    city: String,
    postal_code: String,
    country: String,
}

#[derive(Debug, Clone)]
struct Order {
    id: u64,
    customer_email: String,
    items: Vec<LineItem>,
    shipping: Address,
    paid: bool,
}

fn order_total_cents(order: &Order) -> u64 {
    order
        .items
        .iter()
        .map(|item| item.unit_price_cents * item.quantity as u64)
        .sum()
}

fn main() {
    let order = Order {
        id: 42,
        customer_email: String::from("dora@example.com"),
        items: vec![
            LineItem {
                sku: String::from("RUST-BOOK"),
                description: String::from("The Rust Programming Language"),
                quantity: 2,
                unit_price_cents: 3_999,
            },
            LineItem {
                sku: String::from("STICKER"),
                description: String::from("Ferris sticker"),
                quantity: 5,
                unit_price_cents: 250,
            },
        ],
        shipping: Address {
            street: String::from("1 Ada Lane"),
            city: String::from("London"),
            postal_code: String::from("EC1A"),
            country: String::from("UK"),
        },
        paid: false,
    };

    // Cloning gives us an independent copy (deep clone of the String/Vec fields).
    let mut snapshot = order.clone();
    snapshot.paid = true;

    let total = order_total_cents(&order);
    println!("Order #{} for {}", order.id, order.customer_email);
    println!("Ships to {}, {}", order.shipping.city, order.shipping.country);
    println!("Items: {}", order.items.len());
    println!("Total: ${}.{:02}", total / 100, total % 100);
    println!("Original paid? {} | Snapshot paid? {}", order.paid, snapshot.paid);
}
```

Output:

```text
Order #42 for dora@example.com
Ships to London, UK
Items: 2
Total: $92.48
Original paid? false | Snapshot paid? true
```

Notice how `order_total_cents` **borrows** the order with `&Order` rather than taking ownership, so `main` keeps using `order` afterward. The `snapshot` is a deep `.clone()`: flipping `snapshot.paid` leaves the original `order.paid` untouched. Value semantics, no shared reference. Behavior (methods like `order_total_cents`) is usually attached with `impl` blocks; see [impl blocks](/06-data-structures/05-impl-blocks/) and [associated functions](/06-data-structures/06-associated-functions/).

---

## Further Reading

### Official Documentation

- [The Rust Book — Defining and Instantiating Structs](https://doc.rust-lang.org/book/ch05-01-defining-structs.html)
- [The Rust Book — An Example Program Using Structs](https://doc.rust-lang.org/book/ch05-02-example-structs.html)
- [Rust by Example — Structures](https://doc.rust-lang.org/rust-by-example/custom_types/structs.html)
- [The Rust Reference — Struct types](https://doc.rust-lang.org/reference/types/struct.html)
- [`std::fmt` — Debug formatting](https://doc.rust-lang.org/std/fmt/trait.Debug.html)

### Related Topics in This Guide

- [Tuple Structs and Unit Structs](/06-data-structures/01-tuple-structs/) — fieldless and positional structs, plus the newtype pattern
- [Field Init Shorthand and Struct Update Syntax](/06-data-structures/08-field-init-shorthand/): `username` shorthand and `..other` spread-like updates
- [Impl Blocks](/06-data-structures/05-impl-blocks/) — attaching methods to your structs
- [Associated Functions](/06-data-structures/06-associated-functions/): `Self::new` constructors
- [Enums](/06-data-structures/02-enums/) — when your data is "one of several shapes" instead of a fixed record
- [Option Enum](/06-data-structures/03-option-enum/): modeling optional fields without `null`/`undefined`
- [Pattern Matching](/06-data-structures/04-pattern-matching/) — destructuring structs in `match` and `let`
- [Section 05: Ownership](/05-ownership/): the rules behind field ownership and moves
- [Section 02: Basic Types](/02-basics/01-types/) — choosing concrete field types
- [Section 07: Collections](/07-collections/): `Vec`, `HashMap`, and other field container types

---

## Exercises

### Exercise 1: Define and print a struct

**Difficulty:** Easy

**Objective:** Practice declaring a struct, deriving `Debug`, instantiating it, and reading fields.

**Instructions:** Define a `Rectangle` struct with `width` and `height` fields (`u32`). In `main`, create a `30 x 50` rectangle, debug-print it, then print its area (`width * height`).

```rust
// Define Rectangle here (don't forget the derive)

fn main() {
    // create a rectangle, print it with {:?}, then print its area
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
struct Rectangle {
    width: u32,
    height: u32,
}

fn main() {
    let rect = Rectangle { width: 30, height: 50 };
    println!("{:?}", rect);
    println!("area = {}", rect.width * rect.height);
}
```

Output:

```text
Rectangle { width: 30, height: 50 }
area = 1500
```

</details>

### Exercise 2: Clone and mutate without touching the original

**Difficulty:** Medium

**Objective:** Show that `.clone()` produces an independent copy — mutating the clone leaves the original unchanged.

**Instructions:** Define an `Account` struct with an `owner: String` and a `balance_cents: i64`, deriving `Debug` and `Clone`. Create an account for "Eve" with `10_000` cents. Clone it into a `mut` copy, subtract `2_500` from the copy's balance, then print both. The original must still read `10_000`.

```rust
// Define Account here

fn main() {
    // create the account, clone it, mutate the clone, print both
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone)]
struct Account {
    owner: String,
    balance_cents: i64,
}

fn main() {
    let account = Account {
        owner: String::from("Eve"),
        balance_cents: 10_000,
    };

    let mut copy = account.clone();
    copy.balance_cents -= 2_500;

    println!("original: {:?}", account);
    println!("copy:     {:?}", copy);
}
```

Output:

```text
original: Account { owner: "Eve", balance_cents: 10000 }
copy:     Account { owner: "Eve", balance_cents: 7500 }
```

</details>

### Exercise 3: Nested structs and a `Vec` of structs

**Difficulty:** Medium-Hard

**Objective:** Compose structs (a struct that owns a `Vec` of another struct) and compute an aggregate by borrowing.

**Instructions:** Define a `Track` struct (`title: String`, `duration_secs: u32`) and an `Album` struct (`title: String`, `artist: String`, `tracks: Vec<Track>`). Write a free function `total_runtime(album: &Album) -> u32` that sums the track durations by **borrowing** the album (so the caller can still use it). In `main`, build an album with at least three tracks, then print the artist/title and the total runtime formatted as `M:SS`.

```rust
// Define Track and Album, and total_runtime here

fn main() {
    // build an album, then print "<title> by <artist>" and the total runtime
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone)]
struct Track {
    title: String,
    duration_secs: u32,
}

#[derive(Debug, Clone)]
struct Album {
    title: String,
    artist: String,
    tracks: Vec<Track>,
}

fn total_runtime(album: &Album) -> u32 {
    album.tracks.iter().map(|t| t.duration_secs).sum()
}

fn main() {
    let album = Album {
        title: String::from("Crate Sounds"),
        artist: String::from("The Borrow Checkers"),
        tracks: vec![
            Track { title: String::from("Move Semantics"), duration_secs: 210 },
            Track { title: String::from("Lifetime"), duration_secs: 185 },
            Track { title: String::from("Drop"), duration_secs: 240 },
        ],
    };

    let secs = total_runtime(&album);
    println!("{} by {}", album.title, album.artist);
    println!("{} tracks, {}:{:02} total", album.tracks.len(), secs / 60, secs % 60);
}
```

Output:

```text
Crate Sounds by The Borrow Checkers
3 tracks, 10:35 total
```

</details>

---

## Summary

**What you've learned:**

- A Rust **struct** is the equivalent of a TypeScript `interface` + object literal, fused into one type that exists at compile time and runtime.
- Every field has a **concrete type**, and **all fields must be initialized** at instantiation: there is no `undefined`.
- **Mutation requires `let mut`** on the whole binding; there is no per-field `mut`.
- A struct **owns its fields**: moving the struct moves the data, and you can even move a single owned field out (a partial move).
- **`#[derive(Debug)]`** enables `{:?}` printing and **`#[derive(Clone)]`** gives you an explicit, deep `.clone()`: conveniences that are automatic in TypeScript but opt-in in Rust.
