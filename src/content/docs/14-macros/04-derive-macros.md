---
title: "Derive Macros"
description: "#[derive(...)] generates trait impls at compile time: Debug, Clone, PartialEq, Hash, Default and serde, replacing the per-type helpers you hand-write in TypeScript."
---

The `#[derive(...)]` attribute tells the compiler to **generate trait implementations for you** at compile time. Instead of hand-writing equality, cloning, debug-printing, or hashing for every struct and enum, you list the traits you want and Rust writes the boilerplate. This is the single most common macro a TypeScript/JavaScript developer will meet on day one of writing Rust.

---

## Quick Overview

A **derive macro** is a procedural macro that reads your `struct` or `enum` definition and emits a complete `impl` block for a trait. The result is real, monomorphized code with **zero runtime cost**: the same as if you had typed the implementation yourself. For a TypeScript/JavaScript developer, `#[derive(Clone)]` is roughly "the compiler generates `structuredClone` for this exact shape," and `#[derive(PartialEq)]` is "the compiler generates a correct deep-equality function" — except both are checked and specialized at compile time, not improvised at runtime.

> **Note:** `#[derive(...)]` is **not** a decorator. It does not wrap or modify your type at runtime; it generates *additional* code (trait impls) alongside your definition. See [Macro Basics](/14-macros/00-macro-basics/) for why "macro ≈ decorator" is a misleading analogy.

---

## TypeScript/JavaScript Example

In TypeScript/JavaScript, the behaviors that Rust derives correspond to things you either get for free, write by hand, or reach for a helper to do.

```typescript
// TypeScript/JavaScript: behaviors you implement or improvise per type

interface User {
  id: number;
  name: string;
  active: boolean;
}

const alice: User = { id: 1, name: "Alice", active: true };

// "Debug printing" — console.log understands object shapes already:
console.log(alice); // { id: 1, name: 'Alice', active: true }

// "Cloning" — you must pick a strategy; structuredClone is the modern deep copy:
const bob = structuredClone(alice);

// "Equality" — === is reference identity for objects, so you hand-roll it:
function usersEqual(a: User, b: User): boolean {
  return a.id === b.id && a.name === b.name && a.active === b.active;
}
console.log(alice === bob); // false  (different references!)
console.log(usersEqual(alice, bob)); // true (manual structural compare)

// "Default value" — there is no language-level default; you write a factory:
function defaultUser(): User {
  return { id: 0, name: "", active: false };
}

// "Hashing for use as a key" — objects can't be Map/Set keys by value,
// so people serialize to a string key:
const seen = new Set<string>();
seen.add(JSON.stringify(alice));
```

Notice the pattern: each capability (printing, copying, comparing, defaulting, keying) is either built into the runtime in a loose, dynamic way, or you write a one-off function per type. Nothing ties these helpers to the `User` shape — if you add a field, `usersEqual` silently goes stale.

---

## Rust Equivalent

In Rust you declare the capabilities you want in one line, and the compiler generates correct, shape-aware implementations that **cannot go stale**: add a field and the generated code updates automatically.

```rust playground
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
struct User {
    id: u32,
    name: String,
    active: bool,
}

fn main() {
    let alice = User {
        id: 1,
        name: String::from("Alice"),
        active: true,
    };

    // Debug: {:?} compact, {:#?} pretty-printed
    println!("{alice:?}");
    println!("{alice:#?}");

    // Clone: an explicit, independent deep copy
    let bob = alice.clone();
    println!("clone equal? {}", alice == bob);

    // PartialEq / Eq: structural comparison, field by field
    let carol = User { id: 2, name: String::from("Carol"), active: false };
    println!("alice == carol? {}", alice == carol);

    // Default: every field takes its own type's default
    let blank = User::default();
    println!("{blank:?}");

    // Hash + Eq: the type can be a HashSet / HashMap key by value
    let mut set = HashSet::new();
    set.insert(alice.clone());
    set.insert(bob);   // equal to alice -> not stored twice
    set.insert(carol);
    println!("unique users: {}", set.len());
}
```

Real output from `cargo run`:

```text
User { id: 1, name: "Alice", active: true }
User {
    id: 1,
    name: "Alice",
    active: true,
}
clone equal? true
alice == carol? false
User { id: 0, name: "", active: false }
unique users: 2
```

One `#[derive(...)]` line replaced the debug formatter, the deep-clone, the equality function, the default factory, and the hashing-for-keys logic from the TypeScript version, and the compiler keeps them all in sync with the struct's fields.

---

## Detailed Explanation

### What "derive" literally does

When the compiler sees `#[derive(Clone)]` above a type, it runs the `Clone` derive macro. That macro receives the **tokens** of your type definition and produces an `impl` block. Conceptually, `#[derive(Clone)]` on `User` expands to something like this, which you could also write by hand:

```rust playground
// This is (approximately) what #[derive(Clone)] generates for you.
struct User {
    id: u32,
    name: String,
    active: bool,
}

impl Clone for User {
    fn clone(&self) -> Self {
        User {
            id: self.id.clone(),
            name: self.name.clone(),
            active: self.active.clone(),
        }
    }
}

fn main() {
    let a = User { id: 1, name: "A".into(), active: true };
    let b = a.clone();
    println!("{}", b.id);
}
```

The derive simply clones each field in turn. To see the *exact* expansion for any derive, install the `cargo-expand` tool (`cargo install cargo-expand`) and run `cargo expand`; it prints your crate with all macros — including derives — fully expanded. [Declarative Macros with `macro_rules!`](/14-macros/01-declarative-macros/) covers `cargo expand` in more depth.

> **Tip:** Because the generated code is ordinary `impl` blocks, there is no hidden runtime machinery, no reflection, and no per-call overhead. Rust monomorphizes generic code, so a derived `Clone` for `User` is as fast as one you wrote by hand. This contrasts with TypeScript generics, which are **erased** at runtime.

### The most common standard-library derives

| Trait | What the generated impl gives you | Closest TypeScript/JavaScript analogy |
| ----- | ---------------------------------- | ------------------------------------- |
| `Debug` | `{:?}` / `{:#?}` formatting for logs and tests | `console.log(obj)` printing the shape |
| `Clone` | An explicit deep `.clone()` | `structuredClone(obj)` |
| `Copy` | Implicit bitwise copy on assignment (no `.clone()`) | primitive value-copy semantics |
| `PartialEq` / `Eq` | `==` and `!=` by structural comparison | a hand-written `deepEqual(a, b)` |
| `PartialOrd` / `Ord` | `<`, `>`, `.sort()`, `.max()` | a hand-written comparator for `Array.sort` |
| `Hash` | Use as a `HashMap` / `HashSet` key | a stable key for a `Map` / `Set` |
| `Default` | A `T::default()` constructor | a `defaultX()` factory function |

These are detailed below. (`Serialize` / `Deserialize` are *not* in this list because they come from the `serde` crate, not std — see [Custom derive overview](#custom-derive-overview).)

### `Debug`

`Debug` powers the `{:?}` and `{:#?}` format specifiers, which is what you use in `println!`, `dbg!`, and test assertions. The plain `{:?}` is compact; `{:#?}` is multi-line and indented. Almost every type you define should derive `Debug`; it costs nothing and makes debugging and test failures readable.

### `Clone` and `Copy`

`Clone` gives you an explicit `.clone()` method that produces an independent copy. It is opt-in and visible precisely because copying can be expensive (cloning a `String` allocates new heap memory).

`Copy` is a *marker* for types that are cheap to duplicate by copying their bytes — like `i32` or a small `struct` of integers. When a type is `Copy`, assigning or passing it makes a copy automatically instead of moving it (ownership and moves are covered in [05-ownership](/05-ownership/)). A type can only be `Copy` if **all of its fields are `Copy`**, and `Copy` always requires `Clone`.

```rust playground
// Copy: every field is Copy, so the whole struct can be Copy.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

// Ordering derives compare fields in declaration order (lexicographic).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

// Derives work on enums too. Unit-variant order = discriminant order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    Low,    // discriminant 0 -> smallest
    Medium, // 1
    High,   // 2 -> largest
}

fn main() {
    let a = Point { x: 1, y: 2 };
    let b = a;            // Copy: `a` is NOT moved, still usable below
    println!("{a:?} {b:?}");

    let mut versions = vec![
        Version { major: 1, minor: 2, patch: 0 },
        Version { major: 1, minor: 0, patch: 5 },
        Version { major: 2, minor: 0, patch: 0 },
    ];
    versions.sort(); // uses the derived Ord
    println!("{versions:?}");

    println!("Low < High? {}", Priority::Low < Priority::High);
    let mut ps = vec![Priority::High, Priority::Low, Priority::Medium];
    ps.sort();
    println!("{ps:?}");
}
```

Real output:

```text
Point { x: 1, y: 2 } Point { x: 1, y: 2 }
[Version { major: 1, minor: 0, patch: 5 }, Version { major: 1, minor: 2, patch: 0 }, Version { major: 2, minor: 0, patch: 0 }]
Low < High? true
[Low, Medium, High]
```

### `PartialEq`, `Eq`, `PartialOrd`, `Ord`

`PartialEq` generates `==`/`!=` as a field-by-field structural comparison. `Eq` is a marker that adds the promise "equality is reflexive" (every value equals itself). Floats are deliberately **not** `Eq` because `NaN != NaN`, which is why a struct containing an `f64` can derive `PartialEq` but not `Eq`.

`PartialOrd` and `Ord` generate comparison operators and enable `.sort()`, `.min()`, `.max()`, and ordered collections like `BTreeMap`. As shown above, the derived ordering is **lexicographic by field declaration order** for structs, and by **variant declaration order** for enums.

### `Hash`

`Hash` lets a value be used as a key in a `HashMap` or `HashSet`. There is a contract: if `a == b` then `a` and `b` must hash to the same value. Because of that contract, a type used as a hash key should derive both `Hash` and `Eq` together; the standard library bounds `HashMap`/`HashSet` keys on both.

### `Default`

`Default` generates a `T::default()` constructor. For a struct, each field is set to its own type's default (`0` for integers, `false` for `bool`, `""` for `String`, an empty `Vec`, and so on). Combined with **struct update syntax** (`..Default::default()`), this gives you concise "set a few fields, default the rest" construction: the idiomatic Rust answer to optional fields in a TypeScript object literal.

```rust playground
// `Default` on an enum needs one unit variant marked #[default].
#[derive(Debug, Default, PartialEq)]
enum Status {
    #[default]
    Pending,
    Active,
    Closed,
}

#[derive(Debug, Default)]
struct Settings {
    retries: u32,
    verbose: bool,
    label: String,
    status: Status,
}

fn main() {
    println!("{:?}", Status::default());
    // Set one field, default the rest via struct update syntax:
    let s = Settings { retries: 3, ..Default::default() };
    println!("{s:?}");
    println!("settings status is Pending? {}", s.status == Status::Pending);
}
```

Real output:

```text
Pending
Settings { retries: 3, verbose: false, label: "", status: Pending }
settings status is Pending? true
```

### Custom derive overview

`#[derive(...)]` is not limited to standard-library traits. Library authors can write their own **custom derive macros** (also called *procedural derive macros*), and you import and use them exactly like the built-in ones. The most famous example is `serde`: deriving `Serialize` and `Deserialize` generates all the code needed to turn your type into JSON (or many other formats) and back.

```toml
# Cargo.toml — enable serde's derive feature plus a JSON backend
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

You can add both with `cargo add serde --features derive` and `cargo add serde_json` (no extra plugin needed — `cargo add` is built into Cargo).

```rust playground
use serde::{Serialize, Deserialize};

// Serialize / Deserialize are CUSTOM derive macros from the serde crate,
// not std derives. They generate (de)serialization code for this exact shape.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Config {
    host: String,
    port: u16,
    #[serde(default)] // a "helper attribute" the derive understands
    verbose: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config { host: "localhost".into(), port: 8080, verbose: true };

    let json = serde_json::to_string(&cfg)?;
    println!("{json}");

    // `verbose` is absent in the input -> #[serde(default)] fills it in.
    let parsed: Config = serde_json::from_str(r#"{"host":"example.com","port":443}"#)?;
    println!("{parsed:?}");
    Ok(())
}
```

Real output (verified with serde 1.0.228 and serde_json 1.0.150):

```text
{"host":"localhost","port":8080,"verbose":true}
Config { host: "example.com", port: 443, verbose: false }
```

Two things to notice. First, a custom derive can define **helper attributes** like `#[serde(default)]` that further configure the generated code; these are only valid inside a type that derives the matching macro. Second, from the *user's* side a custom derive looks identical to a std one: list it in `#[derive(...)]` and the code appears. *Writing* such a macro is a separate skill, covered in [Procedural Macros](/14-macros/07-proc-macros/) (the `syn` 2 + `quote` toolchain). Serialization itself is the subject of [15-serialization](/15-serialization/).

---

## Key Differences

| Concern | TypeScript/JavaScript | Rust `#[derive(...)]` |
| ------- | --------------------- | --------------------- |
| Printing a value | `console.log` reads the live object shape at runtime | `#[derive(Debug)]` generates a formatter at compile time |
| Copying | `structuredClone` (deep) or spread (shallow), chosen per call | `#[derive(Clone)]` generates `.clone()`; `Copy` for cheap implicit copies |
| Equality | `===` is reference identity; structural equality is hand-rolled | `#[derive(PartialEq)]` generates correct structural `==` |
| Sorting | pass a comparator to `Array.prototype.sort` | `#[derive(Ord)]` enables `.sort()` directly |
| Using as a map key | objects coerce to keys oddly; people `JSON.stringify` | `#[derive(Hash, Eq)]` makes the type a real key by value |
| Defaults | a hand-written factory function | `#[derive(Default)]` generates `T::default()` |
| When it runs | at runtime, dynamically, per value | at compile time, once, generating specialized code |
| Staleness | helpers drift out of sync as fields change | generated code always matches the current fields |

The deepest conceptual difference: in TypeScript/JavaScript these behaviors are **runtime conveniences** that operate on whatever object you hand them. In Rust they are **compile-time code generation** tied to a specific type. If your type can't legitimately support a behavior — say, comparing a field that has no notion of equality — the program **does not compile**, rather than misbehaving at runtime.

> **Note:** Derive only works on `struct` and `enum` definitions you control. You cannot `#[derive(...)]` for a type defined in another crate — that runs into Rust's coherence ("orphan") rules, discussed in [09-generics-traits](/09-generics-traits/). For external types you write a manual `impl` or use the newtype pattern.

---

## Common Pitfalls

### Pitfall 1: Deriving `Copy` on a type with a heap field

`Copy` requires every field to be `Copy`. `String` owns a heap allocation and is not `Copy`, so this fails.

```rust
#[derive(Clone, Copy)] // does not compile (error[E0204])
struct Wrapper {
    name: String,
}

fn main() {
    let _ = Wrapper { name: String::from("x") };
}
```

Real compiler error:

```text
error[E0204]: the trait `Copy` cannot be implemented for this type
 --> src/main.rs:1:17
  |
1 | #[derive(Clone, Copy)]
  |                 ^^^^
2 | struct Wrapper {
3 |     name: String,
  |     ------------ this field does not implement `Copy`

For more information about this error, try `rustc --explain E0204`.
```

**Fix:** drop `Copy` and keep `Clone`. Use explicit `.clone()` when you need a copy. Reserve `Copy` for small, all-`Copy` types like `Point { x: i32, y: i32 }`.

### Pitfall 2: Deriving a trait whose requirement a field doesn't meet

A derived trait is only valid if every field also implements it. Deriving `PartialEq` on a struct whose field type isn't comparable fails, and the error points at the field, not the derive name.

```rust
struct NotComparable;

#[derive(PartialEq)] // does not compile (error[E0369])
struct Holder {
    inner: NotComparable,
}

fn main() {}
```

Real compiler error (abridged):

```text
error[E0369]: binary operation `==` cannot be applied to type `NotComparable`
 --> src/main.rs:5:5
  |
3 | #[derive(PartialEq)]
  |          --------- in this derive macro expansion
4 | struct Holder {
5 |     inner: NotComparable,
  |     ^^^^^^^^^^^^^^^^^^^^
...
help: consider annotating `NotComparable` with `#[derive(PartialEq)]`
```

**Fix:** derive (or implement) the trait on the inner type too, as the compiler suggests.

### Pitfall 3: Deriving a trait *and* implementing it by hand

If you both `#[derive(PartialEq)]` and write `impl PartialEq`, you have declared the trait twice: a conflicting implementation.

```rust
#[derive(PartialEq)] // does not compile (error[E0119]): conflicts with the manual impl below
struct Celsius(f64);

impl PartialEq for Celsius {
    fn eq(&self, other: &Self) -> bool {
        (self.0 - other.0).abs() < 0.001
    }
}

fn main() {}
```

Real compiler error (abridged):

```text
error[E0119]: conflicting implementations of trait `PartialEq` for type `Celsius`
 --> src/main.rs:1:10
  |
1 | #[derive(PartialEq)]
  |          ^^^^^^^^^ conflicting implementation for `Celsius`
...
4 | impl PartialEq for Celsius {
  | -------------------------- first implementation here
```

**Fix:** pick one. If you need custom logic (like the tolerance-based equality above), remove that trait from `#[derive(...)]` and keep only your manual `impl`.

### Pitfall 4: Forgetting to derive `Debug` before logging or asserting

`{:?}`, `dbg!`, and `assert_eq!` all require `Debug`. A type without it can't be printed that way.

```rust
struct User {
    id: u32,
}

fn main() {
    let u = User { id: 1 };
    println!("{u:?}"); // does not compile (error[E0277]): User doesn't implement Debug
}
```

Real compiler error (abridged):

```text
error[E0277]: `User` doesn't implement `Debug`
 --> src/main.rs:7:16
  |
7 |     println!("{u:?}");
  |               -^---
  |               ||
  |               |`User` cannot be formatted using `{:?}` because it doesn't implement `Debug`
...
help: consider annotating `User` with `#[derive(Debug)]`
```

**Fix:** add `#[derive(Debug)]`. Note `assert_eq!` needs both `Debug` (to print the mismatch) and `PartialEq` (to compare) — see [13-testing](/13-testing/).

### Pitfall 5: The "derive adds a `T:` bound" surprise on generics

For a generic type, a derive generates a *conditional* impl: `#[derive(Clone)] struct Pair<T>` produces `impl<T: Clone> Clone for Pair<T>`. So `Pair<T>` is `Clone` **only when `T` is `Clone`**, which is usually what you want, but the error appears at the call site, not the definition.

```rust
#[derive(Clone)]
struct Pair<T> {
    first: T,
    second: T,
}

struct NotClone;

fn main() {
    let ints = Pair { first: 1, second: 2 };
    let _copy = ints.clone(); // i32 is Clone -> fine

    let stuck = Pair { first: NotClone, second: NotClone };
    let _bad = stuck.clone(); // does not compile (error[E0599]): NotClone is not Clone
}
```

Real compiler error (abridged):

```text
error[E0599]: the method `clone` exists for struct `Pair<NotClone>`, but its trait bounds were not satisfied
  --> src/main.rs:16:22
   |
 4 | struct Pair<T> {
   | -------------- method `clone` not found for this struct because it doesn't satisfy `Pair<NotClone>: Clone`
...
note: trait bound `NotClone: Clone` was not satisfied
  --> src/main.rs:3:10
   |
 3 | #[derive(Clone)]
   |          ^^^^^ unsatisfied trait bound introduced in this `derive` macro
```

**Fix:** make the inner type `Clone` too, or, if the type parameter shouldn't need the bound (a `PhantomData<T>` marker is the classic case), write a manual `impl` without it.

---

## Best Practices

- **Default every type to `#[derive(Debug)]`.** It is free, aids debugging, and is required by `assert_eq!`. Many teams treat a missing `Debug` as a code-smell.
- **Derive the common quartet when it makes sense:** `#[derive(Debug, Clone, PartialEq, Eq)]` is a sensible baseline for plain data types. Add `Hash` when the value will be a map/set key, `PartialOrd, Ord` when it needs sorting, and `Default` when "empty/zero" is meaningful.
- **Prefer derive over hand-written impls** for these traits. The generated code is correct by construction and stays in sync as you add fields. Reserve manual impls for genuinely custom semantics (like tolerance-based float equality).
- **Don't derive `Copy` reflexively.** Add it only to small, all-`Copy` value types. If a struct grows a `String` or `Vec` later, a previously-`Copy` type stops being `Copy` and downstream code may break, so add `Copy` deliberately.
- **Keep `Eq` and `Hash` together** for keys, and remember floats can't be `Eq`. If a struct contains an `f64`, it can derive `PartialEq` but not `Eq`/`Hash`.
- **Use `Default` + struct update syntax** (`Foo { a, ..Default::default() }`) instead of writing constructors that just fill in zeros. It is the idiomatic substitute for optional object fields.
- The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the **2024 edition**; `cargo new` selects the newest edition automatically. All derives shown here are stable across recent editions.

---

## Real-World Example

A small banking/domain model that leans on derives the way production code does: an `enum` used as a `HashMap` key, a money type that sorts, and an account record that has a meaningful default and is cloned for snapshots.

```rust playground
use std::collections::HashMap;

// Eq + Hash -> usable as a HashMap key by value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Currency {
    Usd,
    Eur,
    Gbp,
}

// Ord -> sortable amounts (store money as integer cents, never floats).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Money {
    cents: u64,
}

// Default -> an "empty" account; Clone -> cheap snapshots; PartialEq -> change detection.
#[derive(Debug, Clone, PartialEq, Default)]
struct Account {
    id: u32,
    owner: String,
    balance_cents: u64,
}

fn main() {
    // Currency as a HashMap key:
    let mut rates: HashMap<Currency, f64> = HashMap::new();
    rates.insert(Currency::Usd, 1.0);
    rates.insert(Currency::Eur, 1.08);
    rates.insert(Currency::Gbp, 1.27);
    println!("EUR rate: {}", rates[&Currency::Eur]);

    // Sorting money via the derived Ord:
    let mut amounts = vec![
        Money { cents: 999 },
        Money { cents: 50 },
        Money { cents: 1200 },
    ];
    amounts.sort();
    println!("{amounts:?}");

    // Default + struct update syntax for construction:
    let base = Account::default();
    let acct = Account { id: 7, owner: "Dana".into(), ..base.clone() };
    println!("{acct:?}");
    println!("base unchanged: {base:?}");

    // Clone for an independent snapshot, PartialEq to detect changes:
    let mut snapshot = acct.clone();
    snapshot.balance_cents = 5000;
    println!("changed snapshot? {}", snapshot != acct);
}
```

Real output:

```text
EUR rate: 1.08
[Money { cents: 50 }, Money { cents: 999 }, Money { cents: 1200 }]
Account { id: 7, owner: "Dana", balance_cents: 0 }
base unchanged: Account { id: 0, owner: "", balance_cents: 0 }
changed snapshot? true
```

Every capability here — keying a map, sorting, defaulting, cloning, comparing — came from one-line `#[derive(...)]` declarations, and all of it is checked and specialized at compile time.

---

## Further Reading

### Official documentation

- [The Rust Book — Derivable Traits (Appendix C)](https://doc.rust-lang.org/book/appendix-03-derivable-traits.html)
- [Rust Reference — Derive macros](https://doc.rust-lang.org/reference/attributes/derive.html)
- [`std::default::Default`](https://doc.rust-lang.org/std/default/trait.Default.html), [`std::cmp::PartialEq`](https://doc.rust-lang.org/std/cmp/trait.PartialEq.html), [`std::fmt::Debug`](https://doc.rust-lang.org/std/fmt/trait.Debug.html)
- [`cargo-expand`](https://github.com/dtolnay/cargo-expand) — see what a derive generates
- [serde — derive macros](https://serde.rs/derive.html)

### Related sections in this guide

- [Macro Basics](/14-macros/00-macro-basics/): why a macro is **not** a decorator or a function
- [Declarative Macros with `macro_rules!`](/14-macros/01-declarative-macros/): `macro_rules!` and inspecting expansions with `cargo expand`
- [Attribute Macros](/14-macros/05-attribute-macros/): the other procedural-macro form
- [Function-Like Procedural Macros](/14-macros/06-function-like-macros/): `foo!(...)` procedural macros
- [Procedural Macros](/14-macros/07-proc-macros/): **writing** a custom derive with `syn` 2 + `quote`
- [Common Standard-Library Macros](/14-macros/08-common-macros/): std macros like `assert_eq!` (which needs `Debug` + `PartialEq`)
- [09-generics-traits](/09-generics-traits/): traits, bounds, and the orphan rule
- [05-ownership](/05-ownership/): moves vs. `Clone` vs. `Copy`
- [13-testing](/13-testing/): `assert_eq!` and the `Debug` requirement
- [15-serialization](/15-serialization/): `serde`'s `Serialize` / `Deserialize` derives in depth

---

## Exercises

### Exercise 1

**Difficulty:** Beginner

**Objective:** Pick the right derives so a type can be printed, copied, and compared.

**Instructions:** Add the correct `#[derive(...)]` line to `Temperature` so that the `main` below compiles and prints whether two clones are equal. The struct holds an `f64`, so think about which equality trait is and isn't available.

```rust
struct Temperature {
    celsius: f64,
}

fn main() {
    let a = Temperature { celsius: 20.0 };
    let b = a.clone();
    println!("{a:?} == {b:?}? {}", a == b);
}
```

<details>
<summary>Solution</summary>

`f64` is `PartialEq` but not `Eq` (because of `NaN`), so derive `PartialEq` — not `Eq`. You also need `Debug` for `{:?}` and `Clone` for `.clone()`.

```rust playground
#[derive(Debug, Clone, PartialEq)]
struct Temperature {
    celsius: f64,
}

fn main() {
    let a = Temperature { celsius: 20.0 };
    let b = a.clone();
    println!("{a:?} == {b:?}? {}", a == b);
}
```

Output:

```text
Temperature { celsius: 20.0 } == Temperature { celsius: 20.0 }? true
```

</details>

### Exercise 2

**Difficulty:** Intermediate

**Objective:** Understand what a derive generates by writing the equivalent impl by hand.

**Instructions:** `PointManual` below deliberately does **not** derive `PartialEq`. Write the `impl PartialEq for PointManual` block by hand so that `p == q` works and matches what `#[derive(PartialEq)]` would have produced (field-by-field comparison). Keep `#[derive(Debug)]`.

```rust
#[derive(Debug)]
struct PointManual {
    x: i32,
    y: i32,
}

// TODO: impl PartialEq for PointManual { ... }

fn main() {
    let p = PointManual { x: 1, y: 2 };
    let q = PointManual { x: 1, y: 2 };
    println!("{}", p == q); // should print true
}
```

<details>
<summary>Solution</summary>

The derived `PartialEq` compares every field with `==` and combines them with `&&`. Writing it by hand makes the generated code concrete:

```rust playground
#[derive(Debug)]
struct PointManual {
    x: i32,
    y: i32,
}

impl PartialEq for PointManual {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }
}

fn main() {
    let p = PointManual { x: 1, y: 2 };
    let q = PointManual { x: 1, y: 2 };
    println!("{}", p == q); // true
}
```

Output:

```text
true
```

In real code you would simply add `PartialEq` to the derive list — this exercise just shows there is no magic: the derive writes exactly this.

</details>

### Exercise 3

**Difficulty:** Intermediate

**Objective:** Combine `Default` with struct update syntax to build a "set a few fields, default the rest" configuration.

**Instructions:** Add derives to `ServerConfig` so that `Default::default()` works, then construct a value that sets only `host` and `port` and defaults the remaining fields. Print the result with `{:?}`.

```rust
struct ServerConfig {
    host: String,
    port: u16,
    max_conns: u32,
    tls: bool,
}

fn main() {
    let cfg = /* TODO: host "0.0.0.0", port 8080, defaults for the rest */;
    println!("{cfg:?}");
}
```

<details>
<summary>Solution</summary>

Derive `Default` (and `Debug` to print it). Then use `..Default::default()` to fill in `max_conns` and `tls`:

```rust playground
#[derive(Debug, Default)]
struct ServerConfig {
    host: String,
    port: u16,
    max_conns: u32,
    tls: bool,
}

fn main() {
    let cfg = ServerConfig {
        host: "0.0.0.0".into(),
        port: 8080,
        ..Default::default()
    };
    println!("{cfg:?}");
}
```

Output:

```text
ServerConfig { host: "0.0.0.0", port: 8080, max_conns: 0, tls: false }
```

This is the idiomatic Rust counterpart to a partially-specified object literal in TypeScript — concise, and the compiler guarantees every field has a value.

</details>
