---
title: "The Newtype Pattern"
description: "Wrap one value in a tuple struct for real, zero-cost type safety Rust enforces where a TypeScript brand can't, plus the orphan-rule fix and validated construction."
---

The **newtype** is the smallest, cheapest, and most-used design pattern in idiomatic Rust: wrap a single existing value in a one-field tuple struct to give it a brand-new identity. That tiny move buys three things a TypeScript developer cannot get for free: nominal type safety with no runtime cost, a legal way to implement foreign traits on foreign types (the orphan-rule workaround), and a home for invariants enforced at construction.

---

## Quick Overview

A **newtype** is a struct with exactly one field that wraps another type — `struct UserId(u64);`. Unlike a type alias or a subtype, it is a distinct, **nominal** type the compiler treats as unrelated to its inner value. For a working TypeScript developer, the newtype replaces three separate workarounds you reach for today: *branded types* (`number & { __brand }`) for type safety, *monkey-patching / module augmentation* for adding behavior to types you do not own, and *validation classes* for "parse, don't validate." Rust folds all three into one zero-cost construct, and unlike a TypeScript brand, the distinction is real at runtime: there is no `as` cast that defeats it.

> **Note:** This file focuses on the newtype as a *pattern* — when and why to reach for it. The mechanics of tuple structs themselves are introduced in [Tuple Structs and Unit Structs](/06-data-structures/01-tuple-structs/), the orphan rule in [The Orphan Rule and Coherence](/09-generics-traits/12-orphan-rule/), and `Deref` in [The `Deref` Trait](/10-smart-pointers/06-deref-trait/). We lean on those rather than repeat them.

---

## TypeScript/JavaScript Example

TypeScript is **structurally typed**, so two aliases of the same shape are interchangeable. To fake distinct primitives you use *branded types*, and to "add a method to a type you don't own" you monkey-patch a prototype. Both are fragile.

```typescript
// Structural typing: aliases are interchangeable, so units get mixed up silently.
type Cents = number;
type Millimeters = number;

function priceTag(amount: Cents): string {
  return `$${(amount / 100).toFixed(2)}`;
}

const width: Millimeters = 1599;
console.log(priceTag(width)); // "$15.99" — compiles & runs, but it's a WIDTH, not money

// Branded types buy compile-time distinction... that vanishes at runtime.
type UserId = number & { readonly __brand: "UserId" };
type OrderId = number & { readonly __brand: "OrderId" };

const userId = (n: number): UserId => n as UserId;

function lookupUser(id: UserId): string {
  return `user-${id}`;
}

console.log(lookupUser(userId(42))); // "user-42"
console.log(lookupUser(7 as UserId)); // "user-7" — the `as` cast defeats the brand
```

Running it under Node v22 prints:

```text
$15.99
user-42
user-7
```

**Key points:**

- `Cents` and `Millimeters` are the *same type* at compile time and at runtime; the mix-up is undetectable.
- The brand on `UserId` exists only in the type checker. It is erased at runtime, and any `as` cast (`7 as UserId`) reintroduces the bug it was meant to prevent.
- To attach behavior to a built-in like `Array`, you would augment `Array.prototype`, a global, last-writer-wins mutation that another module can clobber.

---

## Rust Equivalent

A newtype gives you a genuinely separate type. The compiler enforces the distinction everywhere, with no escape hatch and no runtime overhead.

```rust playground
// A newtype: a one-field tuple struct wrapping an existing type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct UserId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OrderId(u64);

fn lookup_user(id: UserId) -> String {
    format!("user-{}", id.0)
}

fn main() {
    let uid = UserId(42);
    let oid = OrderId(42);

    println!("{}", lookup_user(uid));

    // The two values are equal-shaped but NOT interchangeable.
    // lookup_user(oid); // would not compile: expected `UserId`, found `OrderId`
    let _ = oid;
}
```

Output:

```text
user-42
```

**Key points:**

- `UserId` and `OrderId` both wrap `u64`, have identical layout, yet are distinct types. The compiler will never silently convert one to the other.
- There is **no cast** equivalent to TypeScript's `7 as UserId`. To get a `UserId` you must write `UserId(...)` deliberately, and if the field is private (more below), only the type's own module can do that.
- The wrapper is **zero-cost**: `UserId` is the same size as `u64` and is compiled away to a bare integer. The safety is paid for entirely at compile time.

---

## Detailed Explanation

### Why a wrapper instead of an alias?

Rust *does* have type aliases — `type UserId = u64;` — but an alias is purely a second name for the same type. `fn lookup_user(id: UserId)` written with an alias would happily accept any `u64`, and an `OrderId` alias would too. An alias documents intent; it does not enforce it. A newtype enforces it, because `struct UserId(u64)` mints a brand-new nominal type. This is the central contrast with TypeScript:

| | TypeScript brand | Rust newtype |
| --- | --- | --- |
| Distinct at compile time | yes | yes |
| Distinct at runtime | no (erased) | yes (real type) |
| Defeated by a cast | yes (`x as UserId`) | no |
| Runtime cost | none (and no real safety) | none (full safety) |

### Construction ergonomics: `From` and `Into`

Writing `UserId(42)` everywhere is fine, but for conversions from a single canonical source type, implementing `From` makes the newtype feel native. `From` gives you `Into` for free (the standard library has a blanket `impl<T, U: From<T>> Into<U> for T`).

```rust playground
#[derive(Debug, Clone, PartialEq)]
struct Email(String);

impl From<String> for Email {
    fn from(s: String) -> Self {
        Email(s.to_lowercase()) // normalize on the way in
    }
}

fn main() {
    let e: Email = "Alice@Example.COM".to_string().into();
    println!("{e:?}");
}
```

Output:

```text
Email("alice@example.com")
```

`From` is for conversions that **cannot fail**. When construction can fail — the whole point of a validating newtype — use `TryFrom`, which returns a `Result` (see [Error Handling](/08-error-handling/)):

```rust playground
#[derive(Debug)]
struct Age(u8);

impl TryFrom<i64> for Age {
    type Error = String;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if (0..=130).contains(&value) {
            Ok(Age(value as u8))
        } else {
            Err(format!("age out of range: {value}"))
        }
    }
}

fn main() {
    println!("{:?}", Age::try_from(30));  // Ok(Age(30))
    println!("{:?}", Age::try_from(200)); // Err("age out of range: 200")
}
```

Output:

```text
Ok(Age(30))
Err("age out of range: 200")
```

> **Tip:** `From`/`TryFrom` are the idiomatic conversion traits. A `lossy` or `expensive` conversion (like `String` → a parsed domain type) belongs on `TryFrom` or a named method such as `parse`, not `From`. Reserve `From` for cheap, infallible, obvious conversions.

### The orphan-rule workaround

Rust's **orphan rule** (covered fully in [The Orphan Rule and Coherence](/09-generics-traits/12-orphan-rule/)) forbids implementing a *foreign* trait for a *foreign* type. You may implement `Trait for Type` only if your crate defines the trait **or** the type. So you cannot write `impl Display for Vec<String>`: you own neither `Display` nor `Vec`. The compiler stops you:

```rust
use std::fmt;

// does not compile (error[E0117]): foreign trait on a foreign type.
impl fmt::Display for Vec<String> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.join(", "))
    }
}

fn main() {}
```

Real compiler error:

```text
error[E0117]: only traits defined in the current crate can be implemented for types defined outside of the crate
 --> src/main.rs:4:1
  |
4 | impl fmt::Display for Vec<String> {
  | ^^^^^^^^^^^^^^^^^^^^^^-----------
  |                       |
  |                       `Vec` is not defined in the current crate
  |
  = note: impl doesn't have any local type before any uncovered type parameters
  = note: for more information see https://doc.rust-lang.org/reference/items/implementations.html#orphan-rules
  = note: define and implement a trait or new type instead
```

The compiler's own suggestion — "define and implement a trait or new type instead" — *is* the newtype pattern. Wrap the foreign type in a local newtype, and now the type is local, so the impl is allowed:

```rust playground
use std::fmt;

struct Tags(Vec<String>); // a LOCAL type wrapping the foreign Vec<String>

impl fmt::Display for Tags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.0.join(", "))
    }
}

fn main() {
    let tags = Tags(vec!["rust".into(), "newtype".into(), "patterns".into()]);
    println!("{tags}");
}
```

Output:

```text
[rust, newtype, patterns]
```

This is one of the most common reasons to reach for a newtype in real code: you want `Serialize`, `Display`, `FromStr`, or some other foreign trait on a type from `std` or another crate.

### Forwarding the inner API with `Deref`

By default a newtype exposes *none* of the inner type's methods; you have to reach in via `.0` or write your own accessors. When the wrapper genuinely "is a kind of" the inner type, you can implement `Deref` so the inner methods come through automatically (full coverage in [The `Deref` Trait](/10-smart-pointers/06-deref-trait/)):

```rust playground
use std::ops::Deref;

struct Username(String);

impl Deref for Username {
    type Target = str;            // deref straight to str, not String
    fn deref(&self) -> &str {
        &self.0
    }
}

fn main() {
    let u = Username("ahmet".to_string());
    // Deref coercion lets &str methods work directly on a Username:
    println!("len={}, upper={}", u.len(), u.to_uppercase());
}
```

Output:

```text
len=5, upper=AHMET
```

Only `&self` methods leak through `Deref` (it hands back a shared `&Target`), which is often exactly what you want: read-only access without exposing mutators that could break an invariant:

```rust playground
use std::ops::Deref;

struct SortedVec(Vec<i32>); // invariant: always sorted ascending

impl SortedVec {
    fn new(mut v: Vec<i32>) -> Self {
        v.sort();
        SortedVec(v)
    }
}

impl Deref for SortedVec {
    type Target = Vec<i32>;
    fn deref(&self) -> &Vec<i32> {
        &self.0
    }
}

fn main() {
    let sv = SortedVec::new(vec![3, 1, 2]);
    println!("{:?}", sv.first()); // Some(1) — read-only inner method, leaks safely
    println!("len = {}", sv.len()); // 3
    // sv.push(0) is NOT reachable: push() needs &mut self, and Deref only
    // grants &Vec<i32>, so the sorted invariant cannot be violated this way.
}
```

Output:

```text
Some(1)
len = 3
```

> **Warning:** `Deref` is a double-edged sword. It is meant for **smart pointers** (`Box`, `Rc`, `String` → `str`), not as a general "inherit the inner API" mechanism. Implementing `Deref` on a domain newtype dilutes the encapsulation you bought: *every* `&self` method of the inner type leaks out, and that surprises readers who expect a `UserId` not to behave like a `u64`. For most domain newtypes, prefer a couple of named accessors (`as_str`, `value`) or an `AsRef` impl over a blanket `Deref`.

A lighter-weight alternative to `Deref` is `AsRef`, which forwards by an explicit `.as_ref()` call rather than implicit coercion; it advertises a "view as" without pretending the newtype *is* the inner type:

```rust playground
struct Slug(String);

impl AsRef<str> for Slug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn print_len(s: impl AsRef<str>) {
    println!("{}", s.as_ref().len());
}

fn main() {
    let slug = Slug("hello-world".to_string());
    print_len(&slug);   // 11
    print_len("plain"); // 5 — the same fn also accepts &str
}
```

Output:

```text
11
5
```

### Newtypes can be generic

Wrapping is not limited to concrete types. A generic newtype can mark a *property* of any collection — here, "non-empty" — and bake the invariant into the API:

```rust playground
#[derive(Debug)]
struct NonEmpty<T>(Vec<T>);

impl<T> NonEmpty<T> {
    fn new(v: Vec<T>) -> Option<NonEmpty<T>> {
        if v.is_empty() { None } else { Some(NonEmpty(v)) }
    }
    fn first(&self) -> &T {
        &self.0[0] // safe: the constructor guarantees at least one element
    }
}

fn main() {
    let ne = NonEmpty::new(vec![10, 20, 30]).unwrap();
    println!("first = {}", ne.first());
    println!("empty? {:?}", NonEmpty::<i32>::new(vec![]).is_none());
}
```

Output:

```text
first = 10
empty? true
```

---

## Key Differences

| Concern | TypeScript / JavaScript | Rust newtype |
| --- | --- | --- |
| Distinct primitive type | branded type `number & { __brand }` (compile-time only) | `struct UserId(u64);` (real, enforced everywhere) |
| Runtime representation | identical to the inner value; brand erased | identical to the inner value; type is real |
| Defeating the distinction | trivial: `x as UserId` | impossible without explicitly constructing the type |
| Foreign behavior | monkey-patch the prototype (global, clobberable) | newtype + `impl ForeignTrait for Local` (scoped, checked) |
| Validation | a class with a private field + factory | newtype with a private field + `TryFrom`/`parse` |
| Forwarding inner methods | re-export each one by hand | `Deref` (coercion) or `AsRef` (explicit) |
| Cost | none (and no real safety) | none (and full safety) |

The headline: **TypeScript is structural, Rust is nominal.** A branded `UserId` is a `number` wearing a hat that falls off at runtime; a Rust `UserId` is a different type that merely happens to be laid out like a `u64`. That is why the newtype delivers safety a TypeScript brand only approximates.

> **Note:** Coming from TypeScript, the closest mental model is "a branded type that is also real at runtime and cannot be cast away." If you have used `io-ts`, `zod`'s `.brand()`, or `newtype-ts`, the *intent* is the same, but Rust enforces it in the type system itself rather than via a library and a discipline of never casting.

---

## Common Pitfalls

### Pitfall 1: Treating a newtype as its inner type for arithmetic/operators

Wrapping a `f64` in `Meters` does **not** give you `+`, `*`, comparisons, etc. — those traits are not inherited.

```rust
#[derive(Clone, Copy)]
struct Meters(f64);

fn main() {
    let a = Meters(5.0);
    let b = Meters(2.0);
    let _c = a + b; // does not compile
}
```

Real compiler error:

```text
error[E0369]: cannot add `Meters` to `Meters`
  --> src/main.rs:7:16
   |
 7 |     let _c = a + b; // does not compile
   |              - ^ - Meters
   |              |
   |              Meters
   |
note: an implementation of `Add` might be missing for `Meters`
  --> src/main.rs:2:1
   |
 2 | struct Meters(f64);
   | ^^^^^^^^^^^^^ must implement `Add`
```

**Fix:** either operate on `.0` and re-wrap (`Meters(a.0 + b.0)`), or implement `Add` (and friends) when the arithmetic is meaningful for the domain (see [Operator Overloading](/09-generics-traits/10-operator-overloading/)). For the common case of "I want all the inner type's traits without writing them," the `derive_more` crate (`cargo add derive_more`) can derive `Add`, `Display`, `From`, and more for newtypes.

### Pitfall 2: Reaching for a newtype but exposing the inner field publicly

A newtype with a `pub` inner field gives away the keys: callers can construct invalid values and read raw internals, defeating the encapsulation.

```rust
pub struct Email(pub String); // pub field: anyone can build an unvalidated Email
```

Anyone can now write `Email("not-an-email".to_string())`, bypassing any validation. Keep the field private (the default) and validate in a constructor so that *holding* the type is a proof the invariant holds. (Module visibility is covered in [Section 12](/12-modules-packages/).)

### Pitfall 3: Slapping `Deref` on every newtype

It is tempting to implement `Deref` so the wrapper "just works" like the inner type. But `Deref` is for smart pointers, and overusing it leaks the entire inner API, undermines the type's identity, and can produce confusing method-resolution. Clippy flags the classic mistake of deref-ing only to immediately re-borrow. Prefer explicit accessors or `AsRef`; reserve `Deref` for wrappers that truly are pointer-like.

### Pitfall 4: Confusing a newtype with a type alias

```rust
type UserId = u64;            // alias: just another name for u64 — no safety
struct StrongUserId(u64);     // newtype: a distinct type — real safety
```

A function taking `UserId` (the alias) accepts any `u64`; a function taking `StrongUserId` accepts only deliberately-constructed values. If you wanted the swap-protection, the alias silently gives you nothing.

### Pitfall 5: Forgetting to re-derive traits you lost by wrapping

`u64` is `Copy`, `Eq`, `Hash`, `Ord`, `Debug`; the moment you wrap it in `struct UserId(u64);` you have **none** of those unless you ask. Using a `UserId` as a `HashMap` key, comparing two, or `{:?}`-printing one will fail to compile until you add the derives:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct UserId(u64);
```

---

## Best Practices

### 1. Use newtypes to make illegal states unrepresentable

This is the flagship use. If two parameters share a primitive type, callers can swap them; wrap each and the swap becomes a compile error.

```rust playground
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AccountId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Amount(u64); // in cents

fn transfer(from: AccountId, to: AccountId, amount: Amount) {
    println!("transfer {} cents: {} -> {}", amount.0, from.0, to.0);
}

fn main() {
    transfer(AccountId(1), AccountId(2), Amount(500));
    // transfer(Amount(500), AccountId(1), AccountId(2)); // would not compile
}
```

Output:

```text
transfer 500 cents: 1 -> 2
```

### 2. "Parse, don't validate": a private field + a fallible constructor

Make the inner field private and the only constructor a validator. Then the type itself is a *certificate* that validation passed; downstream code never re-checks.

```rust playground
#[derive(Debug, Clone, PartialEq)]
pub struct Email(String); // private field

impl Email {
    pub fn parse(raw: &str) -> Result<Email, String> {
        if raw.contains('@') {
            Ok(Email(raw.trim().to_lowercase()))
        } else {
            Err(format!("invalid email: {raw}"))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn main() {
    println!("{:?}", Email::parse("Alice@Example.com").map(|e| e.as_str().to_string()));
    println!("{:?}", Email::parse("nope").map(|e| e.as_str().to_string()));
}
```

Output:

```text
Ok("alice@example.com")
Err("invalid email: nope")
```

### 3. Derive (or forward) only the traits that make domain sense

Two `UserId`s should be comparable and hashable, so derive `PartialEq`, `Eq`, `Hash`. But two `Email`s probably should *not* support `Add`, and a `Password` newtype should deliberately **not** derive `Debug`/`Display` so it cannot leak into logs. Choose derives intentionally.

### 4. For mechanical forwarding, reach for `derive_more`

When you want a newtype to transparently support arithmetic, `Display`, `From`, etc., hand-writing each impl is noise. `cargo add derive_more` lets you `#[derive(Add, Display, From, Into)]` on the newtype. This keeps the safety of a distinct type while removing boilerplate — a much better answer than blanket `Deref`.

### 5. Make serialization transparent and validating with serde

A newtype can (de)serialize as its inner value while still enforcing its invariant on the way in; see the real-world example next.

---

## Real-World Example

A production payload often carries fields that are "just strings" in JSON but must satisfy invariants in your domain. Here `Email` deserializes from a plain JSON string, normalizes and validates during parsing, and serializes back to a plain string: the validation lives in exactly one place and runs automatically. This combines the *parse-don't-validate* newtype with serde's `try_from`/`into` attributes.

```rust playground
// Cargo.toml:
//   serde = { version = "1", features = ["derive"] }
//   serde_json = "1"
use serde::{Deserialize, Serialize};

// A validated newtype that (de)serializes as its inner string, transparently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
struct Email(String);

impl Email {
    fn parse(raw: &str) -> Result<Email, String> {
        if raw.contains('@') {
            Ok(Email(raw.trim().to_lowercase()))
        } else {
            Err(format!("invalid email: {raw}"))
        }
    }
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Email {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Email::parse(&s)
    }
}

impl From<Email> for String {
    fn from(e: Email) -> String {
        e.0
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct User {
    id: u64,
    email: Email,
}

fn main() {
    // Valid input round-trips and gets normalized (trimmed + lowercased).
    let json = r#"{ "id": 1, "email": "Alice@Example.COM" }"#;
    let user: User = serde_json::from_str(json).unwrap();
    println!("parsed: {} -> {}", user.id, user.email.as_str());
    println!("re-encoded: {}", serde_json::to_string(&user).unwrap());

    // Invalid input is rejected DURING deserialization — no extra validation pass.
    let bad = r#"{ "id": 2, "email": "not-an-email" }"#;
    match serde_json::from_str::<User>(bad) {
        Ok(u) => println!("ok: {u:?}"),
        Err(e) => println!("rejected: {e}"),
    }
}
```

Output:

```text
parsed: 1 -> alice@example.com
re-encoded: {"id":1,"email":"alice@example.com"}
rejected: invalid email: not-an-email at line 1 column 36
```

Because `#[serde(try_from = "String")]` routes deserialization through `Email::parse`, an invalid email cannot even *exist* as a `User` — the boundary of your program is also the boundary of validity. Once you hold a `User`, its `email` is guaranteed well-formed, and the rest of the codebase is free of defensive re-checks. (Serde is covered in [Section 15](/15-serialization/).)

> **Note:** `#[serde(try_from = "String", into = "String")]` keeps the wire format identical to a plain string while inserting your validation. This is the serde idiom for a "smart" newtype: structurally invisible in JSON, semantically enforced in Rust.

---

## Further Reading

### Official Documentation

- [The Rust Book — Using the Newtype Pattern to Implement External Traits](https://doc.rust-lang.org/book/ch20-02-advanced-traits.html#using-the-newtype-pattern-to-implement-external-traits-on-external-types)
- [The Rust Book — Using the Newtype Pattern for Type Safety and Abstraction](https://doc.rust-lang.org/book/ch20-03-advanced-types.html#using-the-newtype-pattern-for-type-safety-and-abstraction)
- [Rust API Guidelines — Newtypes encapsulate implementation details](https://rust-lang.github.io/api-guidelines/future-proofing.html)
- [`std::convert::From`](https://doc.rust-lang.org/std/convert/trait.From.html) · [`TryFrom`](https://doc.rust-lang.org/std/convert/trait.TryFrom.html) · [`AsRef`](https://doc.rust-lang.org/std/convert/trait.AsRef.html) · [`std::ops::Deref`](https://doc.rust-lang.org/std/ops/trait.Deref.html)

### Related Topics in This Guide

- [Tuple Structs and Unit Structs](/06-data-structures/01-tuple-structs/): the mechanics of the one-field tuple struct a newtype is built from
- [The Orphan Rule and Coherence](/09-generics-traits/12-orphan-rule/): why the newtype workaround is necessary and exactly what it enables
- [Operator Overloading](/09-generics-traits/10-operator-overloading/): implementing `Add`/`Mul`/etc. when a newtype should support arithmetic
- [The `Deref` Trait and Deref Coercion](/10-smart-pointers/06-deref-trait/): the forwarding mechanism, used judiciously
- [The Builder Pattern](/22-common-patterns/00-builder-pattern/) — constructing more complex types step by step
- [The Type-State Pattern](/22-common-patterns/02-type-state/) — encoding *state* (not just identity) in the type, the newtype's bigger sibling
- [Error-Handling Patterns](/22-common-patterns/03-error-propagation/) — where the `Result`/`TryFrom` validation in this file leads
- [Extension Traits](/22-common-patterns/11-extension-traits/) — the *other* answer to "add behavior to a foreign type" (methods, not trait impls)
- [Section 23: The Ecosystem](/23-ecosystem/) — crates like `derive_more` and `nutype` that automate newtype boilerplate

---

## Exercises

### Exercise 1: Type-safe composite keys

**Difficulty:** Beginner

**Objective:** Use newtypes to prevent argument-swap bugs and double as `HashMap` keys.

**Instructions:** Define `ProductId(u32)` and `WarehouseId(u32)`, each deriving the traits needed to be a `HashMap` key. Write `stock_for(inventory, wh, product) -> u32` that looks up a `(WarehouseId, ProductId)` key and returns `0` when absent. Insert one entry and print a present and an absent lookup.

```rust
use std::collections::HashMap;

// TODO: define ProductId and WarehouseId with the right derives

fn stock_for(
    inventory: &HashMap<(WarehouseId, ProductId), u32>,
    wh: WarehouseId,
    product: ProductId,
) -> u32 {
    /* ??? */
}

fn main() {
    let mut inventory = HashMap::new();
    // TODO: insert (WarehouseId(1), ProductId(100)) -> 42, then look it up
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ProductId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct WarehouseId(u32);

fn stock_for(
    inventory: &HashMap<(WarehouseId, ProductId), u32>,
    wh: WarehouseId,
    product: ProductId,
) -> u32 {
    inventory.get(&(wh, product)).copied().unwrap_or(0)
}

fn main() {
    let mut inventory = HashMap::new();
    inventory.insert((WarehouseId(1), ProductId(100)), 42);

    println!("{}", stock_for(&inventory, WarehouseId(1), ProductId(100))); // 42
    println!("{}", stock_for(&inventory, WarehouseId(1), ProductId(999))); // 0
    // Swapping the IDs is a compile error, not a silent bug:
    // stock_for(&inventory, ProductId(100), WarehouseId(1)); // would not compile
}
```

Output:

```text
42
0
```

> The two `u32` IDs are distinct types, so the compiler rejects `stock_for(&inventory, ProductId(100), WarehouseId(1))`, the kind of swap a raw `(u32, u32)` would never catch.

</details>

### Exercise 2: A validating newtype with `TryFrom` and `Display`

**Difficulty:** Intermediate

**Objective:** Combine "parse, don't validate" with the standard conversion and formatting traits.

**Instructions:** Build a `Percentage(u8)` whose invariant is `0..=100`. Implement `TryFrom<u8>` returning `Err(String)` when out of range, and `Display` so it prints like `75%`. Show a valid and an invalid construction.

<details>
<summary>Solution</summary>

```rust playground
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
struct Percentage(u8); // invariant: 0..=100

impl TryFrom<u8> for Percentage {
    type Error = String;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value <= 100 {
            Ok(Percentage(value))
        } else {
            Err(format!("{value} is not a valid percentage (0..=100)"))
        }
    }
}

impl fmt::Display for Percentage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

fn main() {
    match Percentage::try_from(75) {
        Ok(p) => println!("ok: {p}"),
        Err(e) => println!("err: {e}"),
    }
    match Percentage::try_from(150) {
        Ok(p) => println!("ok: {p}"),
        Err(e) => println!("err: {e}"),
    }
}
```

Output:

```text
ok: 75%
err: 150 is not a valid percentage (0..=100)
```

> Implementing `TryFrom` (rather than `From`) is the signal that construction can fail; implementing `Display` gives the newtype a domain-appropriate printed form independent of its inner type.

</details>

### Exercise 3: The orphan-rule workaround

**Difficulty:** Intermediate

**Objective:** Implement a foreign trait on a foreign type via a newtype.

**Instructions:** You cannot `impl Display for std::time::Duration` (you own neither). Wrap it in `PrettyDuration(Duration)` and implement `Display` to print `MmSSs` form (e.g. 125 seconds → `2m05s`). Print a 125-second duration.

<details>
<summary>Solution</summary>

```rust playground
use std::fmt;

// Foreign trait (Display) on a foreign type (Duration) is forbidden directly,
// so we wrap Duration in a LOCAL newtype.
struct PrettyDuration(std::time::Duration);

impl fmt::Display for PrettyDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secs = self.0.as_secs();
        let (m, s) = (secs / 60, secs % 60);
        write!(f, "{m}m{s:02}s")
    }
}

fn main() {
    let d = PrettyDuration(std::time::Duration::from_secs(125));
    println!("{d}"); // 2m05s
}
```

Output:

```text
2m05s
```

> `PrettyDuration` is local to your crate, so `impl Display for PrettyDuration` satisfies the orphan rule. The `{s:02}` format pads the seconds to two digits (see [Output and Formatting](/02-basics/04-output/)).

</details>
