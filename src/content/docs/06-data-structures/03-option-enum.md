---
title: "The Option Type: Rust's Answer to null and undefined"
description: "Rust has no null or undefined; Option<T> makes absence an explicit Some/None type, so the forgotten check that crashes JavaScript becomes a compile error."
---

Rust has no `null` and no `undefined`. Instead, the possibility of "no value" is encoded in the type system with `Option<T>` — an enum with exactly two variants, `Some(value)` and `None`. The compiler then forces you to handle the "missing" case before you can touch the value, which eliminates an entire category of runtime crashes that plague TypeScript and JavaScript.

---

## Quick Overview

In TypeScript, any value can secretly be `null` or `undefined`, and forgetting to check leads to the infamous `TypeError: Cannot read properties of undefined`. Rust replaces those two bottom values with a single, explicit type, **`Option<T>`**: a value is either `Some(T)` (present) or `None` (absent). Because "absent" is a distinct variant rather than a magic value any reference can hold, you cannot accidentally use a missing value. The code simply will not compile until you account for `None`.

---

## TypeScript/JavaScript Example

```typescript
// TypeScript - absence is expressed with null/undefined and optional fields
interface User {
  id: number;
  name: string;
  email?: string; // optional => string | undefined
}

const users: User[] = [
  { id: 1, name: "Ada", email: "ada@corp.dev" },
  { id: 2, name: "Grace" }, // no email
];

function findUser(id: number): User | undefined {
  return users.find((u) => u.id === id);
}

// The optional chaining + nullish coalescing dance:
function contactLine(id: number): string {
  const user = findUser(id);
  const email = user?.email ?? "no-email-on-file";
  return `${user?.name ?? "unknown"} <${email}>`;
}

console.log(contactLine(1)); // Ada <ada@corp.dev>
console.log(contactLine(2)); // Grace <no-email-on-file>
console.log(contactLine(99)); // unknown <no-email-on-file>

// The classic mistake: forgetting the check entirely
const u = findUser(99);
// console.log(u.name); // compiles only if strictNullChecks is off; crashes at runtime:
//   TypeError: Cannot read properties of undefined (reading 'name')
```

**Key points:**

- `find` returns `User | undefined`; `?.` and `??` are the tools for handling it.
- With `strictNullChecks` **on**, TypeScript flags `u.name`. With it **off** (still common in legacy codebases), the bug ships and crashes at runtime.
- `null` and `undefined` are two separate bottom values, and the distinction between them is a frequent source of subtle bugs.

---

## Rust Equivalent

```rust playground
// Rust - absence is one explicit type: Option<T>
#[derive(Debug)]
struct User {
    id: u64,
    name: String,
    email: Option<String>, // explicitly "maybe a String"
}

fn find_user<'a>(users: &'a [User], id: u64) -> Option<&'a User> {
    users.iter().find(|u| u.id == id) // `find` returns Option, like Array.find
}

fn contact_line(users: &[User], id: u64) -> String {
    find_user(users, id)
        .map(|u| {
            // as_deref turns Option<String> into Option<&str>; unwrap_or supplies a fallback
            let email = u.email.as_deref().unwrap_or("no-email-on-file");
            format!("{} <{}>", u.name, email)
        })
        .unwrap_or_else(|| "unknown user".to_string())
}

fn main() {
    let users = vec![
        User { id: 1, name: "Ada".to_string(), email: Some("ada@corp.dev".to_string()) },
        User { id: 2, name: "Grace".to_string(), email: None },
    ];

    println!("{}", contact_line(&users, 1));  // Ada <ada@corp.dev>
    println!("{}", contact_line(&users, 2));  // Grace <no-email-on-file>
    println!("{}", contact_line(&users, 99)); // unknown user

    // The equivalent of "forgetting the check" does not compile:
    let u = find_user(&users, 99);
    // println!("{}", u.name); // does not compile (error[E0609]: no field `name` on Option<&User>)
    println!("{u:?}"); // None
}
```

**Verified output:**

```text
Ada <ada@corp.dev>
Grace <no-email-on-file>
unknown user
None
```

**Key points:**

- One type, `Option<T>`, replaces both `null` and `undefined`.
- You cannot reach the inner `User` without first dealing with the `None` case. The "forgotten check" is a **compile error**, not a runtime crash.
- `map` / `unwrap_or` / `as_deref` are the idiomatic counterparts to `?.` and `??`.

---

## Detailed Explanation

### `Option<T>` is just an enum

There is nothing magical about `Option`. It is defined in the standard library, roughly, as:

```rust playground
// Conceptual: this is essentially how the standard library defines Option.
enum MyOption<T> {
    Some(T),
    None,
}

fn main() {
    let a: MyOption<i32> = MyOption::Some(3);
    let b: MyOption<i32> = MyOption::None;

    let label = match a {
        MyOption::Some(n) => format!("value {n}"),
        MyOption::None => "nothing".to_string(),
    };
    let label2 = match b {
        MyOption::Some(n) => format!("value {n}"),
        MyOption::None => "nothing".to_string(),
    };
    println!("{label} / {label2}"); // value 3 / nothing
}
```

`Some` and `None` are **variants** of the enum, parameterized over a generic type `T`. (Enums and their data-carrying variants are covered in [Enums and Data-Carrying Variants](/06-data-structures/02-enums/); generics get full treatment in [Section 09](/09-generics-traits/).) Because `Option` is so fundamental, `Some` and `None` are in Rust's **prelude**: you never need to write `Option::Some` or import anything.

> **Note:** Unlike a TypeScript union `T | undefined`, the `Some` wrapper is a real, named variant. To get at the `T` inside, you must explicitly unwrap it; there is no implicit "the value is just there." This is what makes the absence impossible to ignore.

### Creating and inspecting Options

```rust playground
fn main() {
    let present: Option<i32> = Some(42);
    let absent: Option<i32> = None;

    println!("{present:?}"); // Some(42)
    println!("{absent:?}");  // None

    // Quick boolean checks (rarely the best tool, but they exist)
    println!("{} {}", present.is_some(), present.is_none()); // true false
    println!("{} {}", absent.is_some(), absent.is_none());   // false true
}
```

When you write a bare `None`, Rust sometimes cannot infer `T`, so you annotate either the binding (`let absent: Option<i32> = None;`) or the value (`None::<i32>`).

### Getting the value out: pattern matching

The most fundamental way to handle an `Option` is `match`, which forces you to cover both variants:

```rust playground
fn find_user(name: &str) -> Option<&'static str> {
    match name {
        "alice" => Some("Alice Smith"),
        "bob" => Some("Bob Jones"),
        _ => None,
    }
}

fn main() {
    // `match` is exhaustive: the compiler rejects it if you forget `None`.
    match find_user("alice") {
        Some(full_name) => println!("found {full_name}"),
        None => println!("no user"),
    }

    // `if let` when you only care about the Some case
    if let Some(full_name) = find_user("bob") {
        println!("if-let found {full_name}");
    }

    // `let ... else` to bind-or-bail (stabilized in Rust 1.65)
    let Some(first) = find_user("alice") else {
        println!("no user — returning early");
        return;
    };
    println!("let-else got {first}");
}
```

`match`, `if let`, and `let ... else` are all covered in depth in [Pattern Matching](/06-data-structures/04-pattern-matching/). The key idea here: extracting the inner value is a deliberate act, and the compiler checks that you handled `None`.

### Combinators: the idiomatic alternative to unwrapping

Most real code does not `match` on every `Option`. Instead it uses **combinator methods** — small functions on `Option` that transform or unwrap it. These are the direct counterparts to TypeScript's `?.` and `??`.

```rust playground
fn main() {
    // map: transform the inner value if present (like x?.f())
    let len: Option<usize> = Some("hello").map(|s| s.len());
    println!("{len:?}"); // Some(5)
    let none_len: Option<usize> = None::<&str>.map(|s| s.len());
    println!("{none_len:?}"); // None

    // and_then: chain operations that THEMSELVES return Option (a flatMap)
    let parsed: Option<i32> = Some("42").and_then(|s| s.parse::<i32>().ok());
    println!("{parsed:?}"); // Some(42)
    let bad: Option<i32> = Some("abc").and_then(|s| s.parse::<i32>().ok());
    println!("{bad:?}"); // None

    // unwrap_or: supply a fallback value (like ?? )
    let port: u16 = None.unwrap_or(8080);
    println!("{port}"); // 8080

    // unwrap_or_else: compute the fallback lazily (only runs if None)
    let value = None.unwrap_or_else(|| expensive_default());
    println!("{value}"); // 99

    // unwrap_or_default: use the type's Default::default()
    let count: i32 = None.unwrap_or_default();
    println!("{count}"); // 0

    // filter: keep Some only if a predicate holds
    println!("{:?} {:?}", Some(4).filter(|n| n % 2 == 0), Some(3).filter(|n| n % 2 == 0));
    // Some(4) None

    // or: fall back to another Option
    println!("{:?}", None.or(Some("fallback"))); // Some("fallback")
}

fn expensive_default() -> i32 {
    99
}
```

**Verified output:**

```text
Some(5)
None
Some(42)
None
8080
99
0
Some(4) None
Some("fallback")
```

The key pair to internalize:

| You want to...                                       | Method        | TS analogue                       |
| ---------------------------------------------------- | ------------- | --------------------------------- |
| Transform the value but stay inside `Option`         | `map`         | `x?.f()`                          |
| Transform with a function that returns another `Option` | `and_then` | `x?.f()` where `f` may be nullish |
| Provide a fallback value and leave `Option`          | `unwrap_or`   | `x ?? fallback`                   |
| Provide a fallback computed lazily                   | `unwrap_or_else` | `x ?? expensiveFallback()`     |

> **Tip:** Reach for `unwrap_or_else` over `unwrap_or` whenever the default is expensive to build (e.g. allocates a `String` or hits the network). `unwrap_or` evaluates its argument eagerly, even when the value is `Some`.

### Chaining: a realistic pipeline

Combinators shine when chained, replacing a nest of TypeScript `?.`/`??`:

```rust playground
fn main() {
    // Read a "port" setting that may be missing, whitespace-padded, or unparseable.
    let cfg: Option<&str> = Some("  9090 ");
    let port: u16 = cfg
        .map(|s| s.trim())                  // Option<&str> with trimmed value
        .and_then(|s| s.parse::<u16>().ok()) // Option<u16> (parse may fail)
        .unwrap_or(8080);                    // final value with a default
    println!("port: {port}"); // 9090
}
```

`.ok()` converts a `Result<T, E>` into an `Option<T>` (discarding the error). The reverse — `Option` to `Result` — is `.ok_or(err)`, which is how you bridge into the `?`-and-`Result` world covered in [Section 08](/08-error-handling/).

### The `?` operator with `Option`

The `?` operator is short-circuit unwrapping: **if the value is `Some`, it pulls out the inner value; if it is `None`, it immediately returns `None` from the enclosing function.** This lets you write a chain of fallible lookups linearly, with no nesting:

```rust playground
#[derive(Debug)]
struct Config {
    database: Database,
}
#[derive(Debug)]
struct Database {
    url: Option<String>,
}

// `?` on an Option requires the function to ALSO return Option (or Result).
fn first_db_host(cfg: &Config) -> Option<String> {
    let url = cfg.database.url.as_ref()?; // returns None early if url is None
    let host = url.split('/').nth(2)?;    // returns None if there's no 3rd segment
    Some(host.to_string())
}

fn main() {
    let with_url = Config {
        database: Database { url: Some("postgres://db.example.com/app".to_string()) },
    };
    let without_url = Config {
        database: Database { url: None },
    };

    println!("{:?}", first_db_host(&with_url));    // Some("db.example.com")
    println!("{:?}", first_db_host(&without_url)); // None

    // ok_or converts Option -> Result, bridging into the ? + Result world
    let ok: Result<i32, &str> = Some(5).ok_or("missing");
    let err: Result<i32, &str> = None.ok_or("missing");
    println!("{ok:?} / {err:?}"); // Ok(5) / Err("missing")
}
```

**Verified output:**

```text
Some("db.example.com")
None
Ok(5) / Err("missing")
```

This is the closest Rust gets to TypeScript's optional chaining `a?.b?.c`, but it reaches further: beyond member access, `?` works on **any** expression that produces an `Option`, including function calls, parsing, and collection lookups.

> **Warning:** `?` propagates the absence to the **caller**. The function that uses `?` on an `Option` must itself return an `Option` (or a `Result`, if you `?` a `Result`). You cannot use `?` in a function that returns a plain `String` or `i32`. The exact compiler error appears in [Common Pitfalls](#common-pitfalls).

### Borrowing the inner value: `as_ref` and `as_deref`

A subtle but important point unique to Rust: calling a method like `unwrap()` on an `Option<String>` **moves** the `String` out (ownership rules from [Section 05](/05-ownership/)). Often you only want to *borrow* it. `as_ref` converts `&Option<T>` ergonomically into `Option<&T>`, and `as_deref` goes one step further to `Option<&str>` (or `Option<&[T]>`):

```rust playground
fn main() {
    let owned: Option<String> = Some("hi".to_string());

    let borrowed: Option<&String> = owned.as_ref(); // does NOT move `owned`
    println!("{:?}", borrowed); // Some("hi")
    println!("{:?}", owned);    // still usable: Some("hi")

    let s: Option<String> = Some("deref".to_string());
    let d: Option<&str> = s.as_deref(); // Option<String> -> Option<&str>
    println!("{:?}", d); // Some("deref")
}
```

`as_deref().unwrap_or("default")` is the idiomatic way to read an optional `String` field as a `&str` with a fallback, which is exactly what the opening example does.

---

## Key Differences

| Concept                     | TypeScript/JavaScript                          | Rust                                              |
| --------------------------- | ---------------------------------------------- | ------------------------------------------------- |
| Absence value(s)            | `null` **and** `undefined` (two bottoms)       | One type: `Option<T>` with `None`                 |
| Can any reference be absent? | Yes; every object can be `null`/`undefined`  | No; only `Option<T>` can be `None`               |
| Using the inner value without narrowing | Runtime `TypeError` (or compile error in strict TS) | Compile error; extract via match/combinator first |
| Optional chaining           | `a?.b?.c`                                      | `a.and_then(...)` / `?` operator                  |
| Nullish coalescing          | `x ?? fallback`                                | `x.unwrap_or(fallback)` / `unwrap_or_else`        |
| Default for missing         | `x ?? defaultValue`                            | `x.unwrap_or_default()`                           |
| Runtime cost                | A value compared against `null`/`undefined`    | Zero-cost: niche-optimized (often same size as `T`) |

### Why Rust does it this way

The deep reason is the **billion-dollar mistake**. Tony Hoare, who invented null references in 1965, later called them his billion-dollar mistake because of the countless bugs they caused. Rust's design response is to make absence a value you *opt into* per type, rather than a hole in every reference. A `String` in Rust is *always* a valid string; if it might be missing, its type is `Option<String>` and the compiler tracks that fact everywhere.

A pleasant bonus: `Option<T>` is usually **zero-overhead**. For types with a "niche" (an unused bit pattern), the compiler represents `None` using that bit pattern, so `Option<&T>`, `Option<Box<T>>`, and `Option<NonZero<u32>>` are the *same size* as the value they wrap. There is no boxing and no extra tag word in those cases.

> **Note:** This contrasts sharply with TypeScript, where `string | undefined` is a compile-time-only construct. TypeScript's union types are erased at runtime (the JS engine just sees a value that happens to be `undefined`), whereas Rust monomorphizes `Option<T>` into a concrete in-memory layout for each `T`.

---

## Common Pitfalls

### Pitfall 1: Calling a method on the `Option` instead of the inner value

A TypeScript habit is to call methods directly, since `?.` short-circuits for you. In Rust, `Option<&str>` is **not** a `&str`, so its methods are not available:

```rust
fn main() {
    let name: Option<&str> = Some("Bob");
    let shout = name.to_uppercase(); // does not compile
    println!("{shout}");
}
```

The real compiler error:

```text
error[E0599]: no method named `to_uppercase` found for enum `Option` in the current scope
 --> src/main.rs:3:22
  |
3 |     let shout = name.to_uppercase(); // does not compile
  |                      ^^^^^^^^^^^^ method not found in `Option<&str>`
  |
note: the method `to_uppercase` exists on the type `&str`
help: consider using `Option::expect` to unwrap the `&str` value, panicking if the value is an `Option::None`
```

**Fix:** transform *inside* the `Option` with `map`: `name.map(|s| s.to_uppercase())`.

### Pitfall 2: Reaching `unwrap()` for "I know it's there"

`unwrap()` and `expect()` extract the value but **panic** (crash the thread) on `None`. They are the moral equivalent of pretending a value is non-null:

```rust
fn main() {
    let maybe: Option<i32> = None;
    let value = maybe.unwrap(); // panics at runtime
    println!("{value}");
}
```

Running it produces a real panic, not a compile error:

```text
thread 'main' panicked at src/main.rs:3:23:
called `Option::unwrap()` on a `None` value
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

**Fix:** use `unwrap_or`, `unwrap_or_else`, `match`, `if let`, or `?` to handle `None` explicitly. Reserve `unwrap()` for cases that are *provably* impossible, and prefer `expect("reason")` so the panic message documents your assumption.

### Pitfall 3: Using `?` in a function that doesn't return `Option`/`Result`

```rust
fn third_segment(url: &str) -> String {
    let seg = url.split('/').nth(2)?; // does not compile
    seg.to_string()
}

fn main() {
    println!("{}", third_segment("a/b/c"));
}
```

The real compiler error spells out the requirement exactly:

```text
error[E0277]: the `?` operator can only be used in a function that returns `Result` or `Option` (or another type that implements `FromResidual`)
 --> src/main.rs:2:36
  |
1 | fn third_segment(url: &str) -> String {
  | ------------------------------------- this function should return `Result` or `Option` to accept `?`
2 |     let seg = url.split('/').nth(2)?; // does not compile
  |                                    ^ cannot use the `?` operator in a function that returns `String`
```

**Fix:** change the return type to `Option<String>` (and the caller deals with the `None`), or handle the `None` locally with `unwrap_or`.

### Pitfall 4: Assuming `None == null` for interop

When serializing to JSON (see [Section 15](/15-serialization/)), `None` typically maps to `null` or an *omitted* field, while `Some(x)` maps to `x`. There is no JavaScript-style distinction between `null` and `undefined` on the Rust side; that nuance has to be configured at the serialization layer (e.g. with `serde`'s `skip_serializing_if`).

---

## Best Practices

- **Prefer combinators to `match` for simple transforms.** `opt.map(...).unwrap_or(...)` reads more clearly than a three-line `match` when you are just transforming-or-defaulting.
- **Use `?` to flatten nested optional lookups.** A chain of `let x = a()?; let y = b(x)?;` is far clearer than nested `match`/`if let`.
- **Avoid `unwrap()`/`expect()` in library and production code paths.** They are fine in tests, prototypes, and genuinely-unreachable cases, and when you do use `expect`, write a message that explains *why* it cannot be `None`.
- **Borrow with `as_ref`/`as_deref` instead of moving.** This keeps the original `Option` usable and avoids unnecessary clones.
- **Reach for `unwrap_or_default()`** when the natural fallback is the type's zero/empty value (`0`, `""`, `vec![]`).
- **Convert at the boundary.** Use `.ok_or(err)` to turn an `Option` into a `Result` the moment "missing" should become a reportable error, then let `?` carry it upward.

> **Tip:** Clippy will nudge you toward idiomatic choices. For example, it suggests `map_or` over `map(...).unwrap_or(...)` in some cases, and flags `unwrap()` patterns in lints you can opt into. Run `cargo clippy` regularly.

---

## Real-World Example

A small in-memory user directory backed by a `HashMap` (collections are covered in [Section 07](/07-collections/)). It shows `?` chaining through genuinely-optional fields, plus `map` + `as_deref` + `unwrap_or` fallbacks: the kind of code you write constantly in a service layer.

```rust playground
use std::collections::HashMap;

/// A user profile where several fields are genuinely optional.
#[derive(Debug, Clone)]
struct User {
    id: u64,
    name: String,
    email: Option<String>,   // not every account verified an email
    manager_id: Option<u64>, // the CEO has no manager
}

struct Directory {
    users: HashMap<u64, User>,
}

impl Directory {
    fn get(&self, id: u64) -> Option<&User> {
        self.users.get(&id) // HashMap::get already returns Option
    }

    /// The display name of a user's manager — but only if the user exists,
    /// has a manager, and that manager also exists. Any failure -> None.
    fn manager_name(&self, id: u64) -> Option<&str> {
        let user = self.get(id)?; // user must exist
        let manager_id = user.manager_id?; // user must have a manager
        let manager = self.get(manager_id)?; // manager must exist
        Some(&manager.name)
    }

    /// A printable contact line, with placeholders for missing pieces.
    fn contact_line(&self, id: u64) -> String {
        self.get(id)
            .map(|u| {
                let email = u.email.as_deref().unwrap_or("no-email-on-file");
                format!("#{} {} <{}>", u.id, u.name, email)
            })
            .unwrap_or_else(|| "unknown user".to_string())
    }
}

fn main() {
    let mut users = HashMap::new();
    users.insert(1, User { id: 1, name: "Ada".into(),   email: Some("ada@corp.dev".into()), manager_id: None });
    users.insert(2, User { id: 2, name: "Grace".into(), email: None,                        manager_id: Some(1) });
    let dir = Directory { users };

    // The ? chain in manager_name short-circuits cleanly:
    println!("{:?}", dir.manager_name(2));  // Some("Ada")
    println!("{:?}", dir.manager_name(1));  // None  (Ada has no manager)
    println!("{:?}", dir.manager_name(99)); // None  (no such user)

    // map + as_deref + unwrap_or fallbacks:
    println!("{}", dir.contact_line(1));  // #1 Ada <ada@corp.dev>
    println!("{}", dir.contact_line(2));  // #2 Grace <no-email-on-file>
    println!("{}", dir.contact_line(99)); // unknown user
}
```

**Verified output:**

```text
Some("Ada")
None
None
#1 Ada <ada@corp.dev>
#2 Grace <no-email-on-file>
unknown user
```

Notice how `manager_name` reads like a sequence of preconditions, each guarded by `?`. The TypeScript equivalent would be a tangle of `?.` and intermediate `if` checks, and a single missed check would crash at runtime instead of being caught by the compiler.

---

## Further Reading

- [`std::option` module documentation](https://doc.rust-lang.org/std/option/): the full list of `Option` methods.
- [`Option` enum API](https://doc.rust-lang.org/std/option/enum.Option.html): `map`, `and_then`, `unwrap_or`, `as_ref`, and dozens more.
- [The Rust Book, ch. 6.1: Defining an Enum](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html): where `Option` is introduced.
- [The `?` operator (Rust reference)](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator).

**Cross-links within this guide:**

- [Enums and Data-Carrying Variants](/06-data-structures/02-enums/) — `Option` is an enum; this covers data-carrying variants and discriminated-union comparisons in full.
- [Pattern Matching](/06-data-structures/04-pattern-matching/) — `match`, `if let`, and `let ... else` for destructuring `Option` and other types.
- [Structs](/06-data-structures/00-structs/) — modeling records with optional fields (`email: Option<String>`).
- [Section 05: Ownership](/05-ownership/) — why `as_ref`/`as_deref` matter (moving vs. borrowing the inner value).
- [Section 07: Collections](/07-collections/) — `HashMap::get` and many iterator methods return `Option`.
- [Section 08: Error Handling](/08-error-handling/) — `Result`, `?` with errors, and converting `Option` ↔ `Result`.
- [Section 02: Basics](/02-basics/) and [Section 00: Introduction](/00-introduction/) for foundational context.

---

## Exercises

### Exercise 1: Safe division

**Difficulty:** Beginner

**Objective:** Model "this operation might not produce a value" with `Option<T>`.

**Instructions:**

1. Write `safe_div(a: f64, b: f64) -> Option<f64>` that returns `None` when `b` is `0.0` and `Some(a / b)` otherwise.
2. In `main`, print the results of `safe_div(10.0, 2.0)` and `safe_div(1.0, 0.0)` with `{:?}`.
3. Use `unwrap_or` to print the result of `safe_div(7.0, 0.0)` with a fallback of `f64::INFINITY`.

```rust
fn safe_div(a: f64, b: f64) -> Option<f64> {
    // TODO
}

fn main() {
    // TODO: print the three cases
}
```

<details>
<summary>Solution</summary>

```rust playground
fn safe_div(a: f64, b: f64) -> Option<f64> {
    if b == 0.0 {
        None
    } else {
        Some(a / b)
    }
}

fn main() {
    println!("{:?}", safe_div(10.0, 2.0)); // Some(5.0)
    println!("{:?}", safe_div(1.0, 0.0));  // None

    let result = safe_div(7.0, 0.0).unwrap_or(f64::INFINITY);
    println!("{result}"); // inf
}
```

**Verified output:**

```text
Some(5.0)
None
inf
```

Returning `Option` (rather than throwing or returning a sentinel like `NaN`) makes the "no result" case impossible for callers to ignore.

</details>

---

### Exercise 2: A combinator pipeline

**Difficulty:** Intermediate

**Objective:** Replace a `?.`/`??` chain with `Option` combinators.

**Instructions:**

1. Write `parse_timeout(raw: Option<&str>) -> u64` that reads a timeout setting.
2. The setting may be missing (`None`), surrounded by whitespace, empty, unparseable, or zero.
3. Trim it, reject empty strings, parse it as a `u64`, reject `0`, and fall back to `30` for any failure.
4. Use a chain of `map`, `filter`, `and_then`, and `unwrap_or` — no `match` or `if`.
5. Test with `Some("  60 ")`, `Some("abc")`, `Some("0")`, `Some("")`, and `None`.

<details>
<summary>Solution</summary>

```rust playground
fn parse_timeout(raw: Option<&str>) -> u64 {
    raw.map(|s| s.trim())                // trim whitespace
        .filter(|s| !s.is_empty())       // reject empty
        .and_then(|s| s.parse::<u64>().ok()) // parse, discarding the error
        .filter(|&n| n > 0)              // reject zero
        .unwrap_or(30)                   // fall back to the default
}

fn main() {
    println!("{}", parse_timeout(Some("  60 "))); // 60
    println!("{}", parse_timeout(Some("abc")));   // 30 (parse fails)
    println!("{}", parse_timeout(Some("0")));     // 30 (filtered out)
    println!("{}", parse_timeout(Some("")));      // 30 (empty)
    println!("{}", parse_timeout(None));          // 30 (missing)
}
```

**Verified output:**

```text
60
30
30
30
30
```

Each combinator handles exactly one failure mode, and a `None` anywhere in the chain flows straight through to `unwrap_or(30)`.

</details>

---

### Exercise 3: `?` through nested optional data

**Difficulty:** Advanced

**Objective:** Use the `?` operator to traverse deeply-optional data without nesting.

**Instructions:**

1. Define three structs: `Order { customer: Option<Customer> }`, `Customer { address: Option<Address> }`, and `Address { zip: Option<String> }`.
2. Write `shipping_zip(order: &Order) -> Option<&str>` that returns the zip code only if the customer, the address, and the zip are all present.
3. Use `?` together with `as_ref` / `as_deref` so you borrow rather than move.
4. Test with a fully-populated order, an order whose customer has no address, and an order with no customer.

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug)]
struct Order {
    customer: Option<Customer>,
}
#[derive(Debug)]
struct Customer {
    address: Option<Address>,
}
#[derive(Debug)]
struct Address {
    zip: Option<String>,
}

fn shipping_zip(order: &Order) -> Option<&str> {
    let customer = order.customer.as_ref()?; // Option<&Customer>
    let address = customer.address.as_ref()?; // Option<&Address>
    let zip = address.zip.as_deref()?;        // Option<&str>
    Some(zip)
}

fn main() {
    let full = Order {
        customer: Some(Customer {
            address: Some(Address { zip: Some("94107".to_string()) }),
        }),
    };
    let no_addr = Order {
        customer: Some(Customer { address: None }),
    };
    let no_cust = Order { customer: None };

    println!("{:?}", shipping_zip(&full));    // Some("94107")
    println!("{:?}", shipping_zip(&no_addr)); // None
    println!("{:?}", shipping_zip(&no_cust)); // None
}
```

**Verified output:**

```text
Some("94107")
None
None
```

This is the Rust counterpart to `order?.customer?.address?.zip` — but `?` works on owned and borrowed `Option`s alike and explicitly propagates `None` from the enclosing function.

</details>

---

## Summary

**What you've learned:**

- Rust has no `null`/`undefined`; absence is modeled by the `Option<T>` enum (`Some(T)` or `None`).
- Reading the inner `T` requires you to account for `None` with a match, combinator, propagation, fallback, or deliberate panic; discarding the whole `Option` merely warns by default.
- `map` and `and_then` are the `?.` analogues; `unwrap_or`/`unwrap_or_else`/`unwrap_or_default` are the `??` analogues.
- The `?` operator unwraps `Some` or returns `None` early — flattening nested optional lookups — and requires the enclosing function to return `Option` (or `Result`).
- `unwrap()`/`expect()` panic on `None`; prefer explicit handling, and use `as_ref`/`as_deref` to borrow rather than move the inner value.

**The mental model:** `Option<T>` is the type that says, out loud and in the signature, "this might not be here." TypeScript hides that possibility in every reference and hopes you remember to check; Rust makes it a visible, compiler-enforced part of the type.
