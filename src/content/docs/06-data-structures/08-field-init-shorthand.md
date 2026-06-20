---
title: "Field Init Shorthand and Struct Update Syntax"
description: "Rust's field init shorthand mirrors TypeScript's { username } property shorthand, and struct update syntax (..other) echoes the spread, but ..other can move fields."
---

In TypeScript you lean on **object property shorthand** (`{ username }`) and the **spread operator** (`{ ...base, port: 9090 }`) constantly to keep object construction terse. Rust has direct, named equivalents (**field init shorthand** and **struct update syntax**) but with one rule baked in that TypeScript never enforces: ownership. This page maps both conveniences across the two languages and shows exactly where the analogy holds and where it bites.

---

## Quick Overview

When a local variable has the **same name** as a struct field, Rust lets you write just the name instead of `field: field`. This is **field init shorthand**. When you want a new struct value that is "mostly like an existing one but with a few fields changed," **struct update syntax** (`..other`) fills in the remaining fields from another instance. They look almost exactly like TypeScript's `{ username }` shorthand and `{ ...base }` spread. The important difference is that `..other` may **move** non-`Copy` fields out of the source, where the JavaScript spread always makes an independent shallow copy.

---

## TypeScript/JavaScript Example

These two features are so habitual in TypeScript that most developers stop noticing them. Property shorthand drops the `key: key` repetition, and the spread operator builds a new object from an existing one with a few overrides.

```typescript
// TypeScript - property shorthand + spread are everyday tools
interface User {
  id: number;
  username: string;
  email: string;
  active: boolean;
  loginCount: number;
}

function buildUser(username: string, email: string): User {
  // Property shorthand: `username` means `username: username`
  return { id: 1, username, email, active: true, loginCount: 0 };
}

const user = buildUser("alice", "alice@example.com");
console.log(user);
// {
//   id: 1,
//   username: 'alice',
//   email: 'alice@example.com',
//   active: true,
//   loginCount: 0
// }

// Spread: copy `user`, then override two properties
const updated = { ...user, email: "alice@new.example.com", loginCount: 1 };
console.log(updated);
// {
//   id: 1,
//   username: 'alice',
//   email: 'alice@new.example.com',
//   active: true,
//   loginCount: 1
// }

// `user` is still fully usable — spread made an independent (shallow) copy
console.log(user.email); // 'alice@example.com'
```

**Key points:**

- Property shorthand `{ username }` is pure sugar for `{ username: username }`.
- The spread `{ ...user, ... }` reads every own enumerable property of `user` into a **new** object; later keys win over spread keys.
- The original `user` is untouched and still usable: the spread is a **shallow copy**, not a move.
- Order matters only for overrides: `{ ...user, email }` overrides, `{ email, ...user }` would let `user.email` win.

> **Note:** JavaScript's spread is **shallow**: nested objects and arrays are shared by reference between the original and the copy. Keep this in mind when we compare it to Rust's `..`, which has its own (different) rule.

---

## Rust Equivalent

Rust expresses the same two ideas with dedicated syntax. Field init shorthand is the bare field name; struct update syntax is `..other` as the **last** thing inside the braces.

```rust playground
#[derive(Debug, Clone)]
struct User {
    id: u64,
    username: String,
    email: String,
    active: bool,
    login_count: u32,
}

// Field init shorthand: the parameter names match the field names,
// so `username` is shorthand for `username: username`.
fn build_user(username: String, email: String) -> User {
    User {
        id: 1,
        username, // shorthand
        email,    // shorthand
        active: true,
        login_count: 0,
    }
}

fn main() {
    let user = build_user(String::from("alice"), String::from("alice@example.com"));
    println!("{:?}", user);

    // Struct update syntax: take `email` and `login_count` explicitly,
    // fill the rest from `user.clone()`. The `..source` MUST come last.
    let updated = User {
        email: String::from("alice@new.example.com"),
        login_count: 1,
        ..user.clone()
    };
    println!("{:?}", updated);

    // Mix shorthand and update in one expression.
    let id = 99u64;
    let username = String::from("bob");
    let bob = User {
        id,
        username,
        ..updated.clone()
    };
    println!("{:?}", bob);
}
```

Running this prints:

```text
User { id: 1, username: "alice", email: "alice@example.com", active: true, login_count: 0 }
User { id: 1, username: "alice", email: "alice@new.example.com", active: true, login_count: 1 }
User { id: 99, username: "bob", email: "alice@new.example.com", active: true, login_count: 1 }
```

**Key points:**

- `username,` inside the literal is exactly `username: username,` — identical sugar to TypeScript.
- `..source` fills in **every field you did not write explicitly**, and it must be the **last** element (no trailing comma after it).
- Here we wrote `..user.clone()` deliberately: calling `.clone()` first makes an independent copy, so the original `user` stays usable. Without the `.clone()`, the update would **move** the `String` fields out of `user`. That ownership wrinkle is the whole story of the next sections.

---

## Detailed Explanation

### Field init shorthand, line by line

```rust
fn build_user(username: String, email: String) -> User {
    User {
        id: 1,
        username, // (1)
        email,    // (2)
        active: true,
        login_count: 0,
    }
}
```

1. `username` (no colon, no value) desugars to `username: username`: the field named `username` takes the value of the in-scope variable named `username`.
2. Same for `email`. The remaining fields use the normal `field: value` form because there is no matching local variable (and `1`/`true`/`0` are literals, not variables).

The rule is purely about **name matching**: shorthand is available only when an in-scope binding has the **exact** name of the field. If the names differ, you must use the long form `field: some_other_variable`. There is no "rename while shorthanding" form, unlike TypeScript's `{ field: localName }`, which is also how you write the long form there.

> **Tip:** This is why constructor parameters and helper-function parameters in idiomatic Rust are so often named after the fields they populate: it enables the shorthand and keeps the literal clean.

### Struct update syntax, line by line

```rust
let updated = User {
    email: String::from("alice@new.example.com"), // (1)
    login_count: 1,                               // (2)
    ..user.clone()                                // (3)
};
```

1. and 2. are explicit overrides; these win over whatever `..` would supply.
3. `..user.clone()` says: for **every field not listed above** (`id`, `username`, `active`), take the value from this source expression. The source must be a value of the **same struct type**.

Three rules that have no TypeScript equivalent:

- **`..` must be last.** `User { ..base, port: 9090 }` does not compile; the override fields come first, the `..base` comes last, and there is no comma after it.
- **The source supplies the *remaining* fields, not the leading ones.** TypeScript's spread can appear anywhere and later keys override; Rust's `..` is strictly "everything I didn't already name."
- **`..` can move.** This is covered in detail in [Key Differences](#key-differences) and [Common Pitfalls](#common-pitfalls).

### Why `..user.clone()` and not `..user`?

`User` contains `String` fields, and `String` is **not** `Copy` (it owns a heap allocation — see [Section 05: Ownership](/05-ownership/)). When `..user` fills in `username` from `user`, it **moves** that `String` out of `user`. After that, `user` is partially moved and can no longer be used as a whole. Calling `.clone()` first gives `..` an independent copy to consume, leaving the original `user` intact, which is what we wanted here so we could keep printing it.

---

## Key Differences

| Aspect | TypeScript spread `{ ...base }` | Rust struct update `..base` |
| --- | --- | --- |
| Position in literal | Anywhere; later keys override | Must be **last**; explicit fields override |
| Effect on source | Always an independent **shallow copy** | **Moves** non-`Copy` fields out (source partially consumed) |
| Source type | Any object; extra/missing keys tolerated | Must be the **same struct type** |
| Missing fields | Result simply lacks them | Every field must end up set (override or `..`) |
| Nested data | Shared by reference (shallow) | Moved or `Copy`-copied per field; no implicit deep clone |
| Adding new keys | Spread can introduce keys not on `base` | Cannot introduce fields the struct does not declare |

### The move-vs-copy split

This is the single most important difference. Whether `..base` leaves `base` usable depends entirely on the **types of the fields it pulls in**:

- If every field that `..base` supplies is `Copy` (e.g. all `i32`, `u16`, `bool`, `f64`), then `..base` **copies** those fields and `base` remains fully usable afterward.
- If `..base` supplies any non-`Copy` field (e.g. a `String` or a `Vec`), then those fields are **moved** out of `base`, and `base` is partially moved. You cannot use it as a whole anymore.

A fully-`Copy` struct stays usable after an update:

```rust playground
#[derive(Debug, Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
    z: i32,
}

fn main() {
    let origin = Point { x: 0, y: 0, z: 0 };
    let shifted = Point { x: 10, ..origin };
    // `origin` is still usable: every field is `Copy`, so `..origin` copies.
    println!("{:?}", origin);
    println!("{:?}", shifted);
}
```

Output:

```text
Point { x: 0, y: 0, z: 0 }
Point { x: 10, y: 0, z: 0 }
```

> **Warning:** Do not assume `..base` behaves like a JavaScript spread that always leaves the source intact. With non-`Copy` fields it consumes the source. If you need the original afterward, write `..base.clone()`.

### Struct update is total, not partial

In TypeScript, `{ ...base }` produces whatever keys `base` happens to have. In Rust, a struct literal must set **every** declared field; `..base` is just a convenient way to fill the ones you did not write. You cannot use `..base` to *skip* a field; the resulting value is always a fully-initialized struct. (See [Structs](/06-data-structures/00-structs/) for why there is no partially-initialized struct in Rust.)

---

## Common Pitfalls

### Pitfall 1: Using the source after `..source` moved a non-`Copy` field

This is the move-vs-copy split biting in practice:

```rust
#[derive(Debug)]
struct Config {
    host: String,
    port: u16,
    verbose: bool,
}

fn main() {
    let base = Config {
        host: String::from("localhost"),
        port: 8080,
        verbose: false,
    };

    let custom = Config {
        port: 9090,
        ..base // moves `host` (a String) out of `base`
    };

    println!("{:?}", custom);
    println!("{:?}", base); // does not compile (error[E0382]: borrow of partially moved value: `base`)
}
```

The real compiler error:

```text
error[E0382]: borrow of partially moved value: `base`
  --> src/main.rs:21:22
   |
15 |       let custom = Config {
   |  __________________-
16 | |         port: 9090,
17 | |         ..base
18 | |     };
   | |_____- value partially moved here
...
21 |       println!("{:?}", base);
   |                        ^^^^ value borrowed here after partial move
   |
   = note: partial move occurs because `base.host` has type `String`, which does not implement the `Copy` trait
```

**Fix:** if you need `base` afterward, clone the source: `..base.clone()`. If you do not, just delete the later use of `base`.

### Pitfall 2: Putting `..base` first, or adding a comma after it

Coming from TypeScript, the muscle memory is `{ ...base, port: 9090 }`. In Rust, `..base` is special: it goes **last**, with no trailing comma.

```rust
// does not compile: `..base` must be the last field, and no comma may follow it.
let custom = Config { ..base, port: 9090 };
```

The real compiler error:

```text
error: cannot use a comma after the base struct
 --> src/main.rs:6:27
  |
6 |     let custom = Config { ..base, port: 9090 };
  |                           ^^^^^^
  |
  = note: the base struct must always be the last field
```

**Fix:** put the explicit overrides first and `..base` last:

```rust
let custom = Config { port: 9090, ..base };
```

### Pitfall 3: Expecting shorthand to rename a field

Shorthand only works when the variable name **equals** the field name. A mismatched name is not "shorthand with a rename" — it is a reference to a field that does not exist:

```rust
struct Point { x: i32, y: i32 }

fn main() {
    let x = 1;
    let height = 2; // name does NOT match field `y`
    let p = Point { x, height }; // does not compile (error[E0560]: struct `Point` has no field named `height`)
    println!("{} {}", p.x, p.y);
}
```

The real error:

```text
error[E0560]: struct `Point` has no field named `height`
 --> src/main.rs:6:24
  |
6 |     let p = Point { x, height };
  |                        ^^^^^^ `Point` does not have this field
  |
  = note: available fields are: `y`
```

**Fix:** use the long form to map the variable to the right field: `Point { x, y: height }`.

### Pitfall 4: Forgetting that `..base` does not let you skip fields

A struct literal must initialize every field. If you omit one and do not supply `..base`, you get a missing-field error — `..base` is the only way to fill the gaps:

```rust
struct User { id: u64, name: String, active: bool }

fn main() {
    let name = String::from("alice");
    let u = User { id: 1, name }; // does not compile (error[E0063]: missing field `active`)
    println!("{}", u.name);
}
```

The real error:

```text
error[E0063]: missing field `active` in initializer of `User`
 --> src/main.rs:5:13
  |
5 |     let u = User { id: 1, name };
  |             ^^^^ missing `active`
```

**Fix:** provide the field explicitly, or fill it from another instance with `..other`, or give the struct a `Default` and use `..Default::default()` (see [Best Practices](#best-practices)).

---

## Best Practices

### Pair `..Default::default()` with struct update for "config with overrides"

This is the idiomatic Rust answer to the TypeScript `{ ...DEFAULTS, ...overrides }` pattern. Derive or implement `Default`, then override only the fields that differ:

```rust playground
#[derive(Debug, Clone)]
struct ServerConfig {
    host: String,
    port: u16,
    max_connections: u32,
    tls: bool,
    log_level: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            host: String::from("127.0.0.1"),
            port: 8080,
            max_connections: 1024,
            tls: false,
            log_level: String::from("info"),
        }
    }
}

fn main() {
    let dev = ServerConfig {
        log_level: String::from("debug"),
        ..Default::default()
    };
    println!("{:?}", dev);
}
```

Output:

```text
ServerConfig { host: "127.0.0.1", port: 8080, max_connections: 1024, tls: false, log_level: "debug" }
```

> **Tip:** `..Default::default()` is the closest Rust gets to TypeScript's "optional fields with defaults." It never moves anything out of an existing value because it constructs a fresh default on the spot.

### Use field init shorthand in constructors

When you write a `new`-style associated function (covered in [Associated Functions](/06-data-structures/06-associated-functions/)), name the parameters after the fields so shorthand applies:

```rust
struct Point3 { x: f64, y: f64, z: f64 }

impl Point3 {
    fn new(x: f64, y: f64, z: f64) -> Self {
        Point3 { x, y, z } // clean shorthand
    }
}
```

### Reach for `..` only when most fields stay the same

Struct update shines when you change one or two fields out of many. If you are overriding most of the fields anyway, an explicit literal is clearer than `..base` plus a long list of overrides.

### Clone the source deliberately, not reflexively

`..base.clone()` is correct when you truly need `base` afterward. If you do not, let `..base` move — that is cheaper and the borrow checker confirms you are not using the consumed value. Do not sprinkle `.clone()` everywhere just to silence the compiler; let the move happen when it is fine. See [Section 05: Ownership](/05-ownership/) for the reasoning.

---

## Real-World Example

A production configuration layer: a `Default` baseline, a `new` constructor that uses field shorthand, an environment-specific override built with struct update, and a per-deployment tweak built from that. This is the everyday shape of config plumbing in a Rust service.

```rust playground
#[derive(Debug, Clone)]
struct ServerConfig {
    host: String,
    port: u16,
    max_connections: u32,
    tls: bool,
    log_level: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            host: String::from("127.0.0.1"),
            port: 8080,
            max_connections: 1024,
            tls: false,
            log_level: String::from("info"),
        }
    }
}

impl ServerConfig {
    // Field init shorthand shines here: params named like the fields.
    fn new(host: String, port: u16) -> Self {
        ServerConfig {
            host,
            port,
            ..Default::default()
        }
    }
}

fn main() {
    // Start from defaults, override only what differs (struct update).
    let dev = ServerConfig {
        log_level: String::from("debug"),
        ..Default::default()
    };

    // Production: from a base, flip two fields. `.clone()` keeps `dev` usable.
    let prod = ServerConfig {
        host: String::from("0.0.0.0"),
        tls: true,
        ..dev.clone()
    };

    // Constructor uses shorthand + Default for the rest.
    let custom = ServerConfig::new(String::from("localhost"), 3000);

    println!("dev:    {:?}", dev);
    println!("prod:   {:?}", prod);
    println!("custom: {:?}", custom);
}
```

Output:

```text
dev:    ServerConfig { host: "127.0.0.1", port: 8080, max_connections: 1024, tls: false, log_level: "debug" }
prod:   ServerConfig { host: "0.0.0.0", port: 8080, max_connections: 1024, tls: true, log_level: "debug" }
custom: ServerConfig { host: "localhost", port: 3000, max_connections: 1024, tls: false, log_level: "info" }
```

Notice how `prod` inherits `log_level: "debug"` from `dev` (not from `Default`), because its `..dev.clone()` source was the already-overridden `dev`. Struct update chains naturally: each layer overrides the previous one's fields.

---

## Further Reading

- [The Rust Programming Language — Creating Instances From Other Instances With Struct Update Syntax](https://doc.rust-lang.org/book/ch05-01-defining-structs.html#creating-instances-from-other-instances-with-struct-update-syntax): the official walkthrough of `..` and field init shorthand.
- [The Rust Reference — Struct expressions](https://doc.rust-lang.org/reference/expressions/struct-expr.html) — the precise grammar, including the functional-update (`..`) form.
- [`std::default::Default`](https://doc.rust-lang.org/std/default/trait.Default.html): the trait behind `..Default::default()`.
- [Structs](/06-data-structures/00-structs/) — defining structs, instantiation, and field ownership (read this first if `..` field moves are surprising).
- [Tuple Structs and Unit Structs](/06-data-structures/01-tuple-structs/): positional structs (struct update applies to these too, by position).
- [Associated Functions](/06-data-structures/06-associated-functions/) — `Self::new` constructors where field init shorthand is most at home.
- [Pattern Matching](/06-data-structures/04-pattern-matching/): the *destructuring* counterpart; `..` also appears in patterns, with a related-but-different meaning.
- [Section 05: Ownership](/05-ownership/) — why `..base` can move fields and what "partially moved" means.
- [Section 02: Variables](/02-basics/00-variables/) — immutability defaults that interact with struct construction.
- [Section 07: Collections](/07-collections/): building `Vec`s and maps of structs once you can construct them concisely.

---

## Exercises

### Exercise 1: From spread to struct update

**Difficulty:** Easy

**Objective:** Translate a TypeScript "copy with one override" into Rust struct update syntax, keeping the original usable.

**Instructions:** Given the struct below, write a `main` that creates a `base` `Theme`, then a `user_theme` that is identical except `dark_mode: true` and `font_size: 16`. Print both, and make sure `base` is still printable after creating `user_theme`.

```rust playground
#[derive(Debug, Clone)]
struct Theme {
    primary: String,
    secondary: String,
    font_size: u8,
    dark_mode: bool,
}

fn main() {
    // 1. build `base`
    // 2. build `user_theme` from `base` with two overrides
    // 3. print both (base must still work)
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug, Clone)]
struct Theme {
    primary: String,
    secondary: String,
    font_size: u8,
    dark_mode: bool,
}

fn main() {
    let base = Theme {
        primary: String::from("#0066cc"),
        secondary: String::from("#666666"),
        font_size: 14,
        dark_mode: false,
    };

    let user_theme = Theme {
        dark_mode: true,
        font_size: 16,
        ..base.clone() // clone so `base` stays usable below
    };

    println!("{:?}", base);
    println!("{:?}", user_theme);
}
```

Output:

```text
Theme { primary: "#0066cc", secondary: "#666666", font_size: 14, dark_mode: false }
Theme { primary: "#0066cc", secondary: "#666666", font_size: 16, dark_mode: true }
```

> If the solution did not print `base` afterward, you could drop the `.clone()` and let `..base` move the `String` fields. But because we print `base` below, the `.clone()` is required: `..base` alone would move `primary` and `secondary` out and trigger `error[E0382]`.

</details>

### Exercise 2: Defaults plus a constructor with shorthand

**Difficulty:** Medium

**Objective:** Combine `Default`, field init shorthand, and `..Default::default()` in a constructor.

**Instructions:** Implement `Default` for `QueryOptions` (defaults: `limit = 25`, everything else zero/false). Add an associated function `paged(limit: u32, offset: u32) -> Self` that sets those two fields using **field init shorthand** and fills the rest from `Default`. In `main`, build one value with `paged(50, 100)` and print it.

```rust playground
#[derive(Debug, Clone)]
struct QueryOptions {
    limit: u32,
    offset: u32,
    descending: bool,
    include_archived: bool,
}

// impl Default for QueryOptions { /* ??? */ }
// impl QueryOptions { fn paged(/* ??? */) -> Self { /* ??? */ } }

fn main() {
    // let q = QueryOptions::paged(50, 100);
    // println!("{:?}", q);
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug, Clone)]
struct QueryOptions {
    limit: u32,
    offset: u32,
    descending: bool,
    include_archived: bool,
}

impl Default for QueryOptions {
    fn default() -> Self {
        QueryOptions {
            limit: 25,
            offset: 0,
            descending: false,
            include_archived: false,
        }
    }
}

impl QueryOptions {
    fn paged(limit: u32, offset: u32) -> Self {
        QueryOptions {
            limit,  // field init shorthand
            offset, // field init shorthand
            ..Default::default()
        }
    }
}

fn main() {
    let q = QueryOptions::paged(50, 100);
    println!("{:?}", q);
}
```

Output:

```text
QueryOptions { limit: 50, offset: 100, descending: false, include_archived: false }
```

</details>

### Exercise 3: Chained, ownership-taking "with" methods

**Difficulty:** Medium-Hard

**Objective:** Build a fluent override API using struct update where each method **takes `self` by value** and returns a modified copy — a tiny builder powered entirely by `..self`.

**Instructions:** Derive `Default` (and `PartialEq`) for `QueryOptions`. Add two methods that consume `self`: `with_limit(self, limit: u32) -> Self` and `descending(self) -> Self`. Each should return a new `QueryOptions` that changes only its field and keeps the rest with `..self`. In `main`, chain `QueryOptions::default().with_limit(50).descending()` and assert it equals the expected value.

> **Hint:** Because each method takes `self` by value, `..self` is free to move the remaining fields — there is no original to keep usable.

```rust playground
#[derive(Debug, Clone, Default, PartialEq)]
struct QueryOptions {
    limit: u32,
    offset: u32,
    descending: bool,
    include_archived: bool,
}

// impl QueryOptions {
//     fn with_limit(self, limit: u32) -> Self { /* ??? */ }
//     fn descending(self) -> Self { /* ??? */ }
// }

fn main() {
    // chain the methods, then assert_eq! the result
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug, Clone, Default, PartialEq)]
struct QueryOptions {
    limit: u32,
    offset: u32,
    descending: bool,
    include_archived: bool,
}

impl QueryOptions {
    fn with_limit(self, limit: u32) -> Self {
        QueryOptions { limit, ..self }
    }

    fn descending(self) -> Self {
        QueryOptions { descending: true, ..self }
    }
}

fn main() {
    let opts = QueryOptions::default()
        .with_limit(50)
        .descending();

    println!("{:?}", opts);
    assert_eq!(
        opts,
        QueryOptions { limit: 50, offset: 0, descending: true, include_archived: false }
    );
    println!("ok");
}
```

Output:

```text
QueryOptions { limit: 50, offset: 0, descending: true, include_archived: false }
ok
```

> Each method consumes `self`, so `..self` moving the remaining fields is exactly what we want — the old value is gone and a new one takes its place. This consuming-builder shape is previewed further in [Associated Functions](/06-data-structures/06-associated-functions/).

</details>

---

## Summary

**What you've learned:**

- **Field init shorthand** (`username` instead of `username: username`) is the direct analog of TypeScript's property shorthand and works whenever a local binding shares the field's exact name.
- **Struct update syntax** (`..other`) fills every unwritten field from another instance and must appear **last** in the literal, with no trailing comma — unlike TypeScript's spread, which can go anywhere.
- The defining difference from JavaScript's spread: `..other` **moves** non-`Copy` fields out of the source (leaving it partially moved), while a fully-`Copy` source stays usable. Use `..other.clone()` when you need the original afterward.
- `..Default::default()` is the idiomatic Rust equivalent of `{ ...DEFAULTS, ...overrides }`, and field init shorthand keeps constructors clean.
