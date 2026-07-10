---
title: "Move, Copy, and Clone"
description: "Rust's move, Copy, and Clone replace JavaScript's invisible reference sharing. See when assignment transfers ownership, duplicates a value, or deep-copies."
---

In TypeScript and JavaScript, assigning one variable to another never duplicates the underlying object; both names point at the same garbage-collected value, and you rarely think about *who owns what*. Rust replaces that quiet sharing with three precise behaviors: a value is **moved** (ownership transfers, the old name becomes unusable), **copied** (a cheap bit-for-bit duplicate of a stack value), or **cloned** (an explicit, possibly expensive deep duplicate). Understanding which one happens, and when, is the difference between fighting the compiler and writing fluent Rust.

---

## Quick Overview

When you write `let b = a;` in Rust, one of two things happens: if `a`'s type implements the **`Copy`** trait (small, stack-only values like `i32` or `bool`), `a` is duplicated and both `a` and `b` stay usable; otherwise the value is **moved**: `b` takes ownership and `a` becomes inaccessible. To get a duplicate of a non-`Copy` value (like a `String` or `Vec`), you call **`.clone()`** explicitly, which performs a deep copy you can *see* in the source. This is the opposite of JavaScript, where `const b = a` for an object simply aliases the same heap value and the GC keeps it alive for as long as anyone references it.

> **Note:** This page builds directly on [Ownership Rules](/05-ownership/01-ownership-rules/) (each value has one owner; assignment moves it) and [Stack vs Heap](/05-ownership/00-stack-heap/) (why some types are cheap to copy and others are not). If those ideas are new, skim them first.

---

## TypeScript/JavaScript Example

In JavaScript, every variable holding an object holds a *reference*. Assignment copies the reference, never the object, and primitives are copied by value. You never explicitly "move" or "clone"; the engine and garbage collector handle lifetime for you.

```typescript
// inventory.ts

interface Product {
  sku: string;
  tags: string[];
}

// Assigning an object copies the REFERENCE, not the object.
const original: Product = { sku: "A-100", tags: ["new"] };
const alias = original;
alias.tags.push("sale");
console.log(original.tags); // [ 'new', 'sale' ] — they are the SAME object

// Primitives are copied by value.
let count = 5;
let backup = count;
backup += 1;
console.log(count, backup); // 5 6 — independent copies

// To get a genuinely separate object you must clone EXPLICITLY,
// and even then a shallow clone shares nested arrays/objects.
const shallow = { ...original };
const deep = structuredClone(original);
shallow.tags.push("shallow"); // mutates original.tags too!
deep.tags.push("deep"); // safe: deep has its own array
console.log(original.tags); // [ 'new', 'sale', 'shallow' ]
console.log(deep.tags); // [ 'new', 'sale', 'deep' ]

// Passing an object to a function passes the reference; the original
// is still usable afterward, and mutations leak back to the caller.
function addTag(p: Product, tag: string): void {
  p.tags.push(tag);
}
addTag(original, "fn"); // original is fine after the call
```

**What the runtime does for you here:**

- `const alias = original` aliases the same heap object. Both names see each other's mutations.
- `let backup = count` copies the primitive `5`. The two numbers are independent.
- After passing `original` to `addTag`, you can keep using `original`. The function only borrowed a reference, and the GC keeps the object alive.

Rust makes every one of these distinctions explicit and enforces them at compile time.

---

## Rust Equivalent

The same three behaviors (alias-by-reference, copy-by-value, and explicit deep copy) become **move**, **`Copy`**, and **`Clone`**:

```rust playground
#[derive(Debug, Clone)]
struct Account {
    id: u64,
    owner: String,
    balance: f64,
}

// Takes ownership of `account` (it is MOVED in), mutates it, and returns it.
fn deposit(mut account: Account, amount: f64) -> Account {
    account.balance += amount;
    account
}

fn main() {
    // --- Copy: i32 is a small stack value, so `=` duplicates it. ---
    let x = 5;
    let y = x; // x is COPIED, not moved
    println!("x = {x}, y = {y}"); // both usable

    // --- Move: String owns heap data, so `=` transfers ownership. ---
    let s1 = String::from("hello");
    let s2 = s1; // s1 is MOVED into s2
    println!("{s2}");
    // println!("{s1}"); // does not compile (error[E0382]): s1 was moved

    // --- Clone: ask explicitly for a deep copy. ---
    let s3 = String::from("world");
    let s4 = s3.clone(); // duplicates the heap buffer
    println!("{s3} {s4}"); // BOTH valid

    // --- Move into a function and get ownership back via the return. ---
    let acct = Account { id: 1, owner: String::from("Ada"), balance: 100.0 };
    let acct = deposit(acct, 50.0); // `acct` moved in, rebound to the result
    println!("{acct:?}");

    // --- Clone a whole struct (deep-copies the String inside). ---
    let original = Account { id: 2, owner: String::from("Bob"), balance: 0.0 };
    let copy = original.clone();
    println!("{}", original.owner); // original still usable
    println!("{}", copy.owner);
}
```

This compiles and prints:

```text
x = 5, y = 5
hello
world world
Account { id: 1, owner: "Ada", balance: 150.0 }
Bob
Bob
```

Notice what is *visible* in the source: every potentially expensive duplication is a `.clone()` call you can grep for, and every place a value becomes unusable is an ordinary `=` or function call. There is no hidden deep-copy and no hidden aliasing.

---

## Detailed Explanation

### Move: the default for owned, heap-backed values

A `String` is a three-word handle on the stack: a pointer to a heap buffer, a length, and a capacity (see [Stack vs Heap](/05-ownership/00-stack-heap/)). When you write `let s2 = s1;`, Rust copies those three stack words into `s2`. But now two handles would point at the *same* heap buffer, and when both went out of scope they would each try to free it — a double-free. Rust's solution is the **move**: after the assignment, `s1` is statically marked invalid, so only `s2` is responsible for the buffer. The copy of the stack handle is cheap; what is forbidden is *continuing to use the old name*.

This is the single biggest mental shift from JavaScript. In JS, `const s2 = s1` gives you a second name for the same live object. In Rust, the second name *takes over* and the first is retired.

### Copy: opting out of move for cheap stack values

Some types are so cheap to duplicate that moving them would be pointless ceremony. Integers, floats, `bool`, `char`, and tuples/arrays of such types implement the **`Copy`** trait. For these, `let y = x;` performs a bitwise copy and leaves `x` fully usable — exactly like a JavaScript primitive:

```rust playground
fn square(n: i32) -> i32 {
    n * n
}

fn main() {
    let n = 7;
    let sq = square(n); // n is COPIED into the function
    println!("{n} squared is {sq}"); // n still usable
}
```

```text
7 squared is 49
```

A type can be `Copy` only if *all* of its parts are `Copy` and it does not own a heap allocation or any other resource. That is why `String` (owns heap memory) and `Vec<T>` are **not** `Copy`, while `(i32, bool)` is.

### Clone: explicit, deep, possibly expensive

`Copy` happens implicitly because it is guaranteed cheap. Anything that might be expensive (duplicating a heap buffer, recursively copying a tree) must be requested with **`.clone()`**. `String::clone` allocates a new buffer and copies the bytes; `Vec::clone` allocates a new array and clones each element. The cost is real, so Rust makes you write it down.

```rust
let words = vec![String::from("a"), String::from("b")];
// `iter()` borrows; `cloned()` clones each &String into an owned String.
let joined: String = words.iter().cloned().collect::<Vec<_>>().join("-");
println!("{joined}");                 // a-b
println!("still have words: {words:?}"); // words was only borrowed, still valid
```

```text
a-b
still have words: ["a", "b"]
```

### Moving into functions (and getting ownership back)

Passing a non-`Copy` value to a function *moves* it, just like assignment. The function now owns it; the caller does not. If the caller still needs the value, the idiomatic options are (in rough order of preference): **borrow** instead of moving (`&T`, covered in [Borrowing](/05-ownership/02-borrowing/)), **return the value back** out of the function (as `deposit` does above), or **clone** before the call. Borrowing is almost always the right answer when the function only needs to *read or temporarily mutate* the data.

### Partial moves out of a struct

You can move a single field out of a struct. After that, the field is invalid but `Copy` fields remain readable:

```rust playground
#[derive(Debug)]
struct Profile {
    username: String,
    age: u32,
}

fn main() {
    let p = Profile { username: String::from("rustacean"), age: 30 };
    let name = p.username; // partial MOVE of the String field out of p
    let age = p.age;       // u32 is Copy, so this is a copy
    println!("{name} is {age}");
    // println!("{:?}", p); // does not compile: p is partially moved
}
```

```text
rustacean is 30
```

---

## Key Differences

| Aspect | TypeScript/JavaScript | Rust |
| ------ | --------------------- | ---- |
| `let b = a` for an object | Copies the **reference**; both names alias one live object | **Moves** ownership; `a` becomes unusable (unless the type is `Copy`) |
| `let b = a` for a number | Copies the value; both independent | **Copies** (`Copy` types); both independent |
| Deep duplicate | `structuredClone(a)` / spread (shallow) | `a.clone()` (depth depends on the type's `Clone` impl) |
| Passing to a function | Passes a reference (objects) or a copy (primitives); original stays usable | **Moves** the value in (non-`Copy`), or copies it (`Copy`); to keep it, borrow or clone |
| Who frees the memory | The garbage collector, eventually | The single current owner, deterministically at end of scope ([Drop](/05-ownership/08-drop-trait/)) |
| Cost of duplication | Invisible; sharing is implicit and free | Explicit: cheap `Copy` is automatic, expensive `Clone` is spelled out |

The deepest difference is *honesty about cost*. JavaScript hides whether you are sharing or copying; the GC papers over the lifetime question entirely. Rust forces the choice into the open: a move is a cheap ownership transfer, a `Copy` is a guaranteed-cheap duplicate, and a `Clone` is the one place where an allocation might happen, and it is always visible in the source.

> **Tip:** A quick rule of thumb: if a type is `Copy`, treating it like a JavaScript primitive (free to duplicate, no aliasing surprises) is exactly right. If it is not `Copy`, assume `=` and function calls *consume* the value unless you borrow with `&`.

---

## Common Pitfalls

### Pitfall 1: Using a value after it was moved

This is the error every TypeScript developer hits first, because in JavaScript the original would still be valid:

```rust
fn main() {
    let s1 = String::from("hello");
    let s2 = s1; // value moved here
    println!("{s1}"); // does not compile (error[E0382])
    println!("{s2}");
}
```

The real compiler output:

```text
error[E0382]: borrow of moved value: `s1`
 --> src/main.rs:4:16
  |
2 |     let s1 = String::from("hello");
  |         -- move occurs because `s1` has type `String`, which does not implement the `Copy` trait
3 |     let s2 = s1;
  |              -- value moved here
4 |     println!("{s1}");
  |                ^^ value borrowed here after move
  |
  = note: this error originates in the macro `$crate::format_args_nl` which comes from the expansion of the macro `println` (in Nightly builds, run with -Z macro-backtrace for more info)
help: consider cloning the value if the performance cost is acceptable
  |
3 |     let s2 = s1.clone();
  |                ++++++++
```

The fix is whichever matches your intent: `let s2 = s1.clone();` if you genuinely need two owners, or `let s2 = &s1;` if you only need a second *view* ([Borrowing](/05-ownership/02-borrowing/)).

### Pitfall 2: Moving a value into a function, then using it again

Same error, but it surprises people because the move is hidden inside a call:

```rust
fn print_len(s: String) {
    println!("{}", s.len());
}

fn main() {
    let name = String::from("Grace Hopper");
    print_len(name);   // value moved here
    println!("{name}"); // does not compile (error[E0382])
}
```

The real output even tells you the better design:

```text
error[E0382]: borrow of moved value: `name`
 --> src/main.rs:8:16
  |
6 |     let name = String::from("Grace Hopper");
  |         ---- move occurs because `name` has type `String`, which does not implement the `Copy` trait
7 |     print_len(name);
  |               ---- value moved here
8 |     println!("{name}");
  |                ^^^^ value borrowed here after move
  |
note: consider changing this parameter type in function `print_len` to borrow instead if owning the value isn't necessary
 --> src/main.rs:1:17
  |
1 | fn print_len(s: String) {
  |    ---------    ^^^^^^ this parameter takes ownership of the value
  |    |
  |    in this function
  = note: this error originates in the macro `$crate::format_args_nl` which comes from the expansion of the macro `println` (in Nightly builds, run with -Z macro-backtrace for more info)
help: consider cloning the value if the performance cost is acceptable
  |
7 |     print_len(name.clone());
  |                   ++++++++
```

The idiomatic fix is to make `print_len` borrow: `fn print_len(s: &str)` and call `print_len(&name)`. The function never needed to own the string.

### Pitfall 3: Deriving `Copy` on a type that owns heap data

You cannot make a `String`-holding struct `Copy`, because copying it bit-for-bit would create two owners of one buffer, exactly what move semantics prevent:

```rust
#[derive(Copy, Clone)] // does not compile (error[E0204])
struct Wrapper {
    name: String,
}
```

```text
error[E0204]: the trait `Copy` cannot be implemented for this type
 --> src/main.rs:1:10
  |
1 | #[derive(Copy, Clone)]
  |          ^^^^
2 | struct Wrapper {
3 |     name: String,
  |     ------------ this field does not implement `Copy`
```

The fix: derive only `Clone` (`#[derive(Clone)]`) and call `.clone()` when you need a duplicate.

### Pitfall 4: Reaching for the whole struct after a partial move

Moving one field out invalidates the *whole* binding for purposes of using it as a unit:

```rust
let _name = p.username; // moves the String field out
println!("{:?}", p);     // does not compile
```

```text
error[E0382]: borrow of partially moved value: `p`
  --> src/main.rs:10:22
   |
 9 |     let _name = p.username;
   |                 ---------- value partially moved here
10 |     println!("{:?}", p);
   |                      ^ value borrowed here after partial move
   |
   = note: partial move occurs because `p.username` has type `String`, which does not implement the `Copy` trait
   = note: this error originates in the macro `$crate::format_args_nl` which comes from the expansion of the macro `println` (in Nightly builds, run with -Z macro-backtrace for more info)
```

If you need both the field and the rest of the struct, clone the field (`p.username.clone()`) or borrow it (`&p.username`).

### Pitfall 5: "Cloning" to silence the borrow checker

When the compiler suggests `.clone()`, it is one valid fix — but reaching for it reflexively can hide a design that should borrow instead, adding needless allocations on a hot path. Clone is a tool, not a panic button. Prefer borrowing first (see [Borrowing](/05-ownership/02-borrowing/)); clone when you genuinely need two independent owners.

---

## Best Practices

- **Borrow before you clone before you move.** If a function only reads, take `&T`. If it temporarily mutates, take `&mut T`. Move (take `T`) only when the function truly needs to *own* and store the value. Reserve `.clone()` for when you genuinely need a second independent owner.
- **Derive `Copy` for small, plain value types**: IDs, coordinates, enums without data, fixed-size numeric wrappers. It makes them ergonomic to pass around exactly like JavaScript primitives. Always derive `Clone` alongside `Copy` (`#[derive(Clone, Copy)]`); `Copy` requires `Clone` as a supertrait.
- **Never derive `Copy` on anything that owns a heap allocation or other resource** (`String`, `Vec`, `Box`, file handles). Those are move-only by design.
- **Make clones visible and intentional.** When you do clone, a brief comment on *why* (e.g. "config is shared across threads") helps reviewers distinguish a deliberate clone from a borrow-checker workaround.
- **Return ownership instead of cloning when a function transforms a value.** The `deposit` pattern (`value in → modified value out`) avoids both a clone and a borrow dance.
- **Reach for `Rc`/`Arc` for shared ownership, not repeated `.clone()` of large data.** When many parts of a program need to *share* one immutable value, reference counting is cheaper than deep cloning. See [Reference Counting](/05-ownership/07-reference-counting/).

---

## Real-World Example

A job-processing loop shows all three behaviors working together: a `Copy` ID type passed freely, a shared `Config` that is only *borrowed* (never cloned), and jobs that are *moved* into the processor and consumed.

```rust playground
use std::collections::HashMap;

/// Shared, read-only configuration loaded once at startup.
#[derive(Debug, Clone)]
struct Config {
    service_name: String,
    max_retries: u32,
    endpoints: Vec<String>,
}

/// A small, all-`Copy` value: cheap to pass and store by value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct JobId(u64);

#[derive(Debug)]
struct Job {
    id: JobId,
    payload: String,
}

/// Consumes the job (it is MOVED in) and only BORROWS the config,
/// so one `Config` serves every job without a single clone.
fn process(job: Job, config: &Config) -> String {
    let _ = &config.endpoints; // (would be used to dispatch in real code)
    format!(
        "[{}] job {} -> {} (retries={})",
        config.service_name, job.id.0, job.payload, config.max_retries
    )
}

fn main() {
    let config = Config {
        service_name: String::from("ingest"),
        max_retries: 3,
        endpoints: vec![String::from("https://a"), String::from("https://b")],
    };

    let jobs = vec![
        Job { id: JobId(1), payload: String::from("resize-image") },
        Job { id: JobId(2), payload: String::from("send-email") },
    ];

    let mut results: HashMap<JobId, String> = HashMap::new();
    for job in jobs {
        let id = job.id; // JobId is Copy: this does NOT consume `job`
        // `job` is moved into `process`; `&config` is only borrowed.
        let summary = process(job, &config);
        results.insert(id, summary);
    }

    // `config` is still fully usable — it was only ever borrowed.
    println!("service still available: {}", config.service_name);
    let mut ids: Vec<_> = results.keys().copied().collect();
    ids.sort_by_key(|j| j.0);
    for id in ids {
        println!("{}", results[&id]);
    }
}
```

This compiles and prints:

```text
service still available: ingest
[ingest] job 1 -> resize-image (retries=3)
[ingest] job 2 -> send-email (retries=3)
```

The key design wins, all enforced at compile time: `JobId` being `Copy` lets us grab `job.id` for the map key *without* consuming `job`; the `&config` borrow means the single config object is reused for every iteration with zero allocations; and each `Job` is moved into `process` and dropped at the end of that call, so memory is reclaimed deterministically with no GC pauses.

---

## Further Reading

### Official Documentation

- [The Rust Book — What Is Ownership? (Move semantics)](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)
- [The Rust Book — References and Borrowing](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html)
- [`std::marker::Copy` (trait docs)](https://doc.rust-lang.org/std/marker/trait.Copy.html)
- [`std::clone::Clone` (trait docs)](https://doc.rust-lang.org/std/clone/trait.Clone.html)
- [Rust by Example — Ownership and moves](https://doc.rust-lang.org/rust-by-example/scope/move.html)
- [Error index — E0382 (use of moved value)](https://doc.rust-lang.org/error_codes/E0382.html), [E0204 (cannot derive `Copy`)](https://doc.rust-lang.org/error_codes/E0204.html)

### Related Sections in This Guide

- [Stack vs Heap](/05-ownership/00-stack-heap/) — why `Copy` types are cheap and `String`/`Vec` are not.
- [Ownership Rules](/05-ownership/01-ownership-rules/): one owner per value; move on assignment; drop at end of scope.
- [Borrowing](/05-ownership/02-borrowing/) — `&T` shared references: the alternative to moving or cloning.
- [Mutable References](/05-ownership/03-mutable-references/): `&mut T` for in-place mutation without moving.
- [Lifetimes](/05-ownership/04-lifetimes/) — relating the validity of borrows you hand out.
- [Reference Counting (`Rc`/`Arc`)](/05-ownership/07-reference-counting/): shared ownership without deep clones.
- [The `Drop` Trait](/05-ownership/08-drop-trait/) — what happens when the final owner goes out of scope.
- [Section 05 overview](/05-ownership/): the full ownership system.
- [Functions — Parameters](/03-functions/01-parameters/) — choosing between by-value, `&`, and `&mut` parameters.
- [Basics — Variables and Mutability](/02-basics/00-variables/) and [Types](/02-basics/01-types/): the building blocks moved and copied here.
- [Data Structures](/06-data-structures/) — structs and enums that own vs. borrow their fields.

---

## Exercises

### Exercise 1: Clone to keep two owners

**Difficulty:** Easy

**Objective:** Recognize a move and fix it by cloning when two independent owners are genuinely required.

**Instructions:** The snippet below does not compile because `a` is moved into `b` and then used again. Make it compile *without* changing the fact that both `a` and `b` must be usable afterward and must hold equal contents.

```rust
fn main() {
    let a = String::from("data");
    let b = a;            // TODO: a gets used below — make this work
    println!("{a} {b}");
    assert_eq!(a, b);
}
```

<details>
<summary>Solution</summary>

```rust playground
fn main() {
    let a = String::from("data");
    let b = a.clone(); // explicit deep copy: now there are two owners
    println!("{a} {b}");
    assert_eq!(a, b);
    println!("ex1 ok");
}
```

`a.clone()` allocates a second heap buffer with the same bytes, so `a` and `b` are independent owners. (If you only needed to *read* `a` through `b`, `let b = &a;` would have avoided the allocation entirely, but the exercise requires two owners.)

</details>

### Exercise 2: Make a type `Copy`

**Difficulty:** Medium

**Objective:** Derive `Copy` on a small value type so it can be used multiple times without moving.

**Instructions:** Define a `Meters(f64)` newtype and an `add(a: Meters, b: Meters) -> Meters` function. Then call `add(d, d)` using the *same* variable `d` twice and also read `d` afterward. This is only possible if `Meters` is `Copy`; add the right derive.

```rust
struct Meters(f64); // TODO: derive what you need

fn add(a: Meters, b: Meters) -> Meters {
    // TODO
    todo!()
}

fn main() {
    let d = Meters(3.0);
    let total = add(d, d); // d used twice
    assert_eq!(total.0, 6.0);
    assert_eq!(d.0, 3.0); // d still usable
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug, Clone, Copy, PartialEq)]
struct Meters(f64);

fn add(a: Meters, b: Meters) -> Meters {
    Meters(a.0 + b.0)
}

fn main() {
    let d = Meters(3.0);
    let total = add(d, d); // `d` is COPIED into each argument, not moved
    assert_eq!(total, Meters(6.0));
    assert_eq!(d, Meters(3.0)); // still usable
    println!("ok");
}
```

Because `f64` is `Copy`, a `Meters` wrapping a single `f64` can derive `Copy` too. `Copy` requires `Clone` as a supertrait, so you must derive both. Now passing `d` twice copies it twice instead of moving it.

</details>

### Exercise 3: Borrow, then own only what you keep

**Difficulty:** Medium–Hard

**Objective:** Take borrowed input, clone only the pieces you decide to keep, and return owned data: the common "build an owned result from borrowed inputs" pattern.

**Instructions:** Implement `collect_unique(items: &[&str]) -> Vec<String>` that returns the input strings in first-seen order with duplicates removed. The parameter is borrowed (the function does not own the strings), so you must turn each kept `&str` into an owned `String`. `collect_unique(&["a", "b", "a", "c", "b"])` must return `vec!["a", "b", "c"]` (as `String`s).

```rust
fn collect_unique(items: &[&str]) -> Vec<String> {
    // TODO: keep first occurrences, return owned Strings
    todo!()
}

fn main() {
    let result = collect_unique(&["a", "b", "a", "c", "b"]);
    assert_eq!(result, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust playground
fn collect_unique(items: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for &item in items {
        let owned = item.to_string(); // clone the borrowed &str into an owned String
        if !out.contains(&owned) {
            out.push(owned); // moved into the Vec; `out` now owns it
        }
    }
    out
}

fn main() {
    let result = collect_unique(&["a", "b", "a", "c", "b"]);
    assert_eq!(result, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    println!("ok");
}
```

The slice `&[&str]` is borrowed, so the original strings stay owned by the caller. `item.to_string()` allocates an owned `String` (a kind of clone from a borrowed `&str`), which is then *moved* into `out`. The function returns the owned `Vec<String>` by value: ownership transfers to the caller, no borrow-checker gymnastics required.

</details>
