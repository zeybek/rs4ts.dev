---
title: "Borrowing & References in Rust"
description: "A &T reference lends read-only access without moving a value, so the owner keeps it. Rust's borrow checker proves at compile time no reference ever dangles."
---

In TypeScript and JavaScript, you pass objects and arrays around by reference all day without thinking about it, and you accept the consequence that any function might quietly mutate your data. Rust gives you references too, but with a twist: the compiler tracks every borrow and guarantees, at compile time, that a reference can never outlive the data it points to.

---

## Quick Overview

A **reference** (written `&value`) lets a function read or use a value **without taking ownership of it**: the original owner keeps the value and can use it again afterward. Rust's **borrow checker** verifies, while compiling, that every reference points to live data, which is how Rust eliminates an entire category of bugs (dangling pointers, use-after-free) that you would otherwise debug at runtime.

This page covers **shared (immutable) references**: the `&T` borrow that lets you read. Mutable references (`&mut T`) and their exclusivity rule have their own page: see [Mutable References](/05-ownership/03-mutable-references/).

---

## TypeScript/JavaScript Example

In JavaScript, objects and arrays are always handled through references. When you pass one to a function, the function receives a pointer to the same underlying object, so it can mutate your data, and the change is visible to you afterward.

```typescript
// TypeScript/JavaScript - objects are passed by reference
interface User {
  name: string;
  email: string;
  loginCount: number;
}

function describe(user: User): string {
  return `${user.name} <${user.email}> logged in ${user.loginCount} times`;
}

// A function can mutate the object you handed it...
function recordLogin(user: User): void {
  user.loginCount += 1; // mutates the caller's object!
}

const user: User = { name: "Ada", email: "ada@example.com", loginCount: 7 };

console.log(describe(user)); // read it
recordLogin(user); // and silently change it
console.log(user.loginCount); // 8 — the original object changed

// Aliasing: two names, one object
const alias = user;
alias.loginCount = 0;
console.log(user.loginCount); // 0 — `user` and `alias` ARE the same object
```

**Key points:**

- Objects/arrays are passed and assigned **by reference**.
- Any holder of the reference can mutate the shared object.
- There is no language-level distinction between "I want to read this" and "I want to change this." You rely on convention, documentation, or defensive copies (`structuredClone`, spreads) to stay safe.

---

## Rust Equivalent

Rust makes the distinction explicit. A plain `&` reference is a **read-only loan**: the borrower may look at the value but not change it. The owner keeps the value the whole time.

```rust
#[derive(Debug)]
struct User {
    name: String,
    email: String,
    login_count: u32,
}

// `&User` is a SHARED reference: read-only access, no ownership taken.
fn describe(user: &User) -> String {
    format!(
        "{} <{}> logged in {} times",
        user.name, user.email, user.login_count
    )
}

fn is_active(user: &User) -> bool {
    user.login_count > 0
}

fn main() {
    let user = User {
        name: String::from("Ada"),
        email: String::from("ada@example.com"),
        login_count: 7,
    };

    // Borrow the same value as many times as we like.
    println!("{}", describe(&user));
    println!("active? {}", is_active(&user));

    // `user` is still fully owned here — nothing was moved or consumed.
    println!("{:?}", user);
}
```

**Output (compile-verified):**

```text
Ada <ada@example.com> logged in 7 times
active? true
User { name: "Ada", email: "ada@example.com", login_count: 7 }
```

**Key points:**

- `&user` creates a **shared borrow**; `&User` is the type "shared reference to a `User`."
- The function borrows the value, uses it, and gives it back; `user` is usable afterward.
- A shared reference cannot be used to mutate. To change the value, you would need a mutable reference (`&mut`), covered on the next page.

> **Note:** Without borrowing, passing `user` into `describe` would **move** it (transfer ownership) and you could no longer use `user` afterward. Borrowing is how you avoid that. See [Ownership Rules](/05-ownership/01-ownership-rules/) and [Move, Copy, Clone](/05-ownership/06-move-copy-clone/) for the full story on moves.

---

## Detailed Explanation

### What a reference actually is

A reference is a pointer: a small value that holds the **memory address** of something else. The `&` operator creates a reference; the `*` operator **dereferences** it (follows the pointer to read the value behind it).

```rust
fn main() {
    let x = 10;
    let r = &x; // r is a reference to x (a pointer under the hood)

    println!("x  = {}", x);
    println!("*r = {}", *r); // explicitly follow the pointer
    println!("r  = {}", r);  // most operations auto-deref, so *r is often optional

    // A reference holds the address of what it points at.
    println!("address of x   : {:p}", &x);
    println!("value held by r : {:p}", r);
    println!("equal? {}", std::ptr::eq(&x, r));
}
```

**Output (compile-verified; the exact hex addresses vary by run and platform, but the two are always equal):**

```text
x  = 10
*r = 10
r  = 10
address of x   : 0x16d025c44
value held by r : 0x16d025c44
equal? true
```

The two printed addresses are identical because `r` literally points at `x`. In most everyday code you rarely write `*` yourself; Rust auto-dereferences for method calls and field access (`user.name` works whether `user` is a `User` or a `&User`). You reach for `*` mainly when you need the value itself, such as in arithmetic (`*r + 1`).

> **Note:** Unlike a JavaScript reference, a Rust reference is **guaranteed non-null and always valid** for as long as it exists. There is no `null`, no `undefined`, and no dangling pointer; the compiler proves this before your program ever runs.

### Borrowing instead of moving

Here is the canonical example. We borrow a `String` so the function can measure it without consuming it.

```rust
fn main() {
    let message = String::from("hello, borrow");

    // Borrow `message` immutably — `len` only needs to read it.
    let length = calculate_length(&message);

    // `message` is still valid here because we only lent it out.
    println!("'{}' has length {}", message, length);
}

fn calculate_length(s: &String) -> usize {
    s.len()
} // `s` (the reference) goes out of scope here, but because it is only a
  // borrow, the underlying String is NOT dropped — the caller still owns it.
```

**Output (compile-verified):**

```text
'hello, borrow' has length 13
```

The action of creating a reference is called **borrowing**. The function `calculate_length` borrows `message`, reads its length, and when `s` goes out of scope only the *reference* is dropped, never the `String` it pointed at. The owner (`main`) gets its value back intact.

> **Note:** `&String` is used here purely to mirror the owned-vs-borrowed contrast against the moving version. In production you would write `s: &str` (see [Best Practice #2](#best-practices)), and `cargo clippy` will in fact suggest exactly that via the `clippy::ptr_arg` lint, since `&str` accepts both `&String` and string slices.

### Many shared borrows at once are fine

A shared reference promises **read-only** access. Because no one can mutate, it is perfectly safe to hand out any number of shared references simultaneously:

```rust
fn main() {
    let data = vec![1, 2, 3, 4, 5];

    let first = &data;
    let second = &data;
    let third = &data;

    // Many simultaneous shared borrows are allowed: nobody can mutate.
    println!("sum via first  = {}", first.iter().sum::<i32>());
    println!("len via second = {}", second.len());
    println!("max via third  = {:?}", third.iter().max());

    // The original owner is still usable too.
    println!("data = {:?}", data);
}
```

**Output (compile-verified):**

```text
sum via first  = 15
len via second = 5
max via third  = Some(5)
data = [1, 2, 3, 4, 5]
```

This is the "shared" half of Rust's central borrowing rule: **either many shared (`&`) references, or exactly one mutable (`&mut`) reference, but never both at the same time.** The exclusivity of `&mut` is the subject of the [Mutable References](/05-ownership/03-mutable-references/) page; here it is enough to know that as long as you only borrow with `&`, you can have as many borrows as you want.

### The borrow checker

The **borrow checker** is the part of the Rust compiler that tracks the lifetime of every reference and rejects any program where a reference could point at data that is gone or is being mutated out from under it. It runs at compile time, so the checks cost nothing at runtime. The two guarantees most relevant to shared references are:

1. A reference may never outlive the value it borrows (no dangling references).
2. While any shared reference exists, the borrowed value cannot be mutated or moved. (The exception is *interior mutability* — types like `Cell`, `RefCell`, and `Mutex` that allow controlled mutation behind `&T`; they are the subject of [Smart Pointers](/10-smart-pointers/02-refcell-mutex/).)

You do not call the borrow checker; it is simply part of `cargo build` / `cargo check`. When it rejects your code you get an error like `E0597` ("does not live long enough") or `E0502` ("cannot borrow as mutable because it is also borrowed as immutable"). Reading those errors is a core Rust skill — they almost always point at exactly the right fix.

### Dangling references are impossible

In C you can return a pointer to a local variable and get undefined behavior. In JavaScript you can't even express the problem because the GC keeps anything reachable alive. Rust takes a third path: it makes the dangling case a **compile error**.

```rust
// does not compile (error[E0106]: missing lifetime specifier)
fn dangle() -> &String {
    let s = String::from("temporary");
    &s // returning a reference to `s`...
} // ...but `s` is dropped here, so the reference would dangle
```

The compiler refuses this outright (see [Common Pitfalls](#common-pitfalls) for the full message). The fix is to return the **owned** `String` instead of a reference to it. Then ownership moves out to the caller and nothing is left dangling:

```rust
fn no_dangle() -> String {
    let s = String::from("temporary");
    s // ownership moves out to the caller — perfectly safe
}

fn main() {
    let owned = no_dangle();
    println!("{}", owned);
}
```

---

## Key Differences

| Aspect | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| How objects are passed | Always by reference, implicitly | You choose: by value (move/copy) or by reference (`&` / `&mut`) |
| Read vs. write intent | Not expressed in the language | `&T` = read-only, `&mut T` = read/write |
| Can the callee mutate my data? | Yes, silently, unless you copy defensively | Only if you explicitly lend a `&mut` |
| Dangling references | Impossible (GC keeps things alive) | Impossible (compiler rejects them), but **without** a garbage collector |
| Multiple holders mutating at once | Allowed (a common bug source) | Forbidden at compile time for `&mut`; unlimited for `&` |
| Cost | GC bookkeeping, allocation pressure | Zero runtime cost; checks happen at compile time |
| `null` / `undefined` references | Possible (`obj?.field`) | A `&T` is never null; absence is modeled with `Option<&T>` |

### The big mental shift

In JavaScript, "passing a reference" is the *only* option and it always grants full mutation rights to the callee. In Rust, references are **explicit, typed, and read-only by default**. A function signature like `fn save(user: &User)` is a machine-checked promise: "I will look at your `User`, I will not change it, and I will not keep it after I return." That promise is enforced by the compiler, not by code review.

> **Tip:** When migrating a mental model, read `&T` as "a temporary read-only loan of a `T`." The owner is guaranteed to still have the value when the loan ends.

### Borrowing vs. cloning

A JavaScript habit is to defensively copy (`{ ...obj }`, `structuredClone(obj)`) to avoid accidental shared mutation. In Rust, borrowing usually replaces that copy entirely: you hand out a cheap `&` reference instead of duplicating data. Reach for `.clone()` only when you genuinely need a second independent owner — see [Move, Copy, Clone](/05-ownership/06-move-copy-clone/).

---

## Common Pitfalls

### Pitfall 1: Returning a reference to a local value

This is the dangling-reference attempt. The borrowed value would be destroyed at the end of the function.

```rust
// does not compile
fn dangle() -> &String {
    let s = String::from("temporary");
    &s
}

fn main() {
    let r = dangle();
    println!("{}", r);
}
```

**Real compiler error:**

```text
error[E0106]: missing lifetime specifier
 --> src/main.rs:2:16
  |
2 | fn dangle() -> &String {
  |                ^ expected named lifetime parameter
  |
  = help: this function's return type contains a borrowed value, but there is no value for it to be borrowed from
help: consider using the `'static` lifetime, but this is uncommon unless you're returning a borrowed value from a `const` or a `static`
  |
2 | fn dangle() -> &'static String {
  |                 +++++++
help: instead, you are more likely to want to return an owned value
  |
2 - fn dangle() -> &String {
2 + fn dangle() -> String {
  |
```

**Fix:** Return the owned `String` (drop the `&`). The compiler's last suggestion is exactly right. When you genuinely need to return a borrow, it must borrow from one of the function's *inputs*, which is what [Lifetimes](/05-ownership/04-lifetimes/) are about.

### Pitfall 2: Using a value after its referent has been dropped

A subtler version: the reference outlives the value's scope.

```rust
// does not compile
fn main() {
    let reference;
    {
        let value = String::from("short-lived");
        reference = &value;
    } // `value` is dropped here
    println!("{}", reference); // tries to use the freed value
}
```

**Real compiler error:**

```text
error[E0597]: `value` does not live long enough
 --> src/main.rs:6:21
  |
5 |         let value = String::from("short-lived");
  |             ----- binding `value` declared here
6 |         reference = &value;
  |                     ^^^^^^ borrowed value does not live long enough
7 |     } // `value` is dropped here
  |     - `value` dropped here while still borrowed
8 |     println!("{}", reference); // tries to use the freed value
  |                    --------- borrow later used here
```

**Fix:** Make the borrowed value live at least as long as the reference, usually by declaring `value` in the outer scope. In a GC language this code would "work" by keeping `value` alive; Rust instead tells you the lifetimes don't line up.

### Pitfall 3: Passing by value when you meant to borrow

A very common beginner mistake: forgetting the `&`, which **moves** the value into the function.

```rust
// does not compile
fn print_it(s: String) {
    println!("{}", s);
}

fn main() {
    let owned = String::from("data");
    print_it(owned); // moves `owned` into the function
    println!("{}", owned); // try to use it again
}
```

**Real compiler error (abridged):**

```text
error[E0382]: borrow of moved value: `owned`
 --> src/main.rs:9:20
  |
7 |     let owned = String::from("data");
  |         ----- move occurs because `owned` has type `String`, which does not implement the `Copy` trait
8 |     print_it(owned); // moves `owned` into the function
  |              ----- value moved here
9 |     println!("{}", owned); // try to use it again
  |                    ^^^^^ value borrowed here after move
  |
note: consider changing this parameter type in function `print_it` to borrow instead if owning the value isn't necessary
help: consider cloning the value if the performance cost is acceptable
  |
8 |     print_it(owned.clone()); // moves `owned` into the function
  |                   ++++++++
```

**Fix:** Borrow instead of moving: change the signature to `fn print_it(s: &String)` (or better, `s: &str`) and call it as `print_it(&owned)`. Cloning also compiles, but it needlessly duplicates the data; prefer borrowing.

### Pitfall 4: Trying to mutate through a shared reference

A shared `&` reference is read-only. Attempting to write through it is a compile error.

```rust
// does not compile
fn main() {
    let mut count = 0;
    let r = &count; // shared (immutable) reference
    *r += 1; // try to mutate through it
    println!("{}", count);
}
```

**Real compiler error (abridged):**

```text
error[E0594]: cannot assign to `*r`, which is behind a `&` reference
 --> src/main.rs:4:5
  |
4 |     *r += 1; // try to mutate through it
  |     ^^^^^^^ `r` is a `&` reference, so the data it refers to cannot be written
  |
help: consider changing this to be a mutable reference
  |
3 |     let r = &mut count;
  |              +++
```

**Fix:** Use `&mut count` to get a mutable reference (and note that `count` is already `let mut`). Mutable borrows have their own exclusivity rule — read [Mutable References](/05-ownership/03-mutable-references/) next.

---

## Best Practices

### 1. Borrow by default; take ownership only when you must

If a function only needs to *read* a value, take it by shared reference. This is the most flexible and cheapest signature — callers keep their data and pay nothing for the call.

```rust
// Idiomatic: read-only access via a shared borrow
fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}
```

### 2. Prefer `&str` over `&String`, and `&[T]` over `&Vec<T>`

A `&str` accepts both string literals and borrowed `String`s (via automatic deref coercion), so it is strictly more general. Likewise, `&[T]` accepts arrays, `Vec`s, and other slices.

```rust
fn shout(text: &str) -> String {
    text.to_uppercase()
}

fn main() {
    let owned = String::from("hello");
    println!("{}", shout(&owned)); // &String coerces to &str
    println!("{}", shout("world")); // &str literal works too
}
```

> **Tip:** Clippy will actively suggest changing `&String` parameters to `&str` and `&Vec<T>` to `&[T]`. Following this makes your APIs accept more callers for free.

### 3. Let the compiler guide you

The borrow-checker errors are unusually good. When you hit one, read the `help:` lines — they frequently contain the literal fix (add `&`, add `mut`, return an owned value). Treat the borrow checker as a pair programmer, not an adversary.

### 4. Reach for references before `.clone()`

If you find yourself cloning to "make the error go away," pause: a borrow is usually the right answer and avoids the allocation. Clone only when you truly need a second independent owner.

---

## Real-World Example

A small order-processing module. Three functions all **borrow** a slice of orders; none takes ownership, so the caller can run every report in sequence on the same data without any copying.

```rust
use std::collections::HashMap;

#[derive(Debug)]
struct Order {
    id: u32,
    customer: String,
    total_cents: u64,
    status: String,
}

// Borrow the orders read-only; sum the paid ones.
fn total_revenue(orders: &[Order]) -> u64 {
    orders
        .iter()
        .filter(|o| o.status == "paid")
        .map(|o| o.total_cents)
        .sum()
}

// The returned map borrows customer names FROM `orders`, so its keys
// (`&str`) live exactly as long as the borrowed slice. No strings copied.
fn revenue_by_customer(orders: &[Order]) -> HashMap<&str, u64> {
    let mut totals: HashMap<&str, u64> = HashMap::new();
    for order in orders {
        if order.status == "paid" {
            *totals.entry(order.customer.as_str()).or_insert(0) += order.total_cents;
        }
    }
    totals
}

// Return a borrow of one order that lives as long as the input slice.
fn find_order<'a>(orders: &'a [Order], id: u32) -> Option<&'a Order> {
    orders.iter().find(|o| o.id == id)
}

fn main() {
    let orders = vec![
        Order { id: 1, customer: String::from("Ada"),   total_cents: 4_500, status: String::from("paid") },
        Order { id: 2, customer: String::from("Linus"), total_cents: 9_900, status: String::from("refunded") },
        Order { id: 3, customer: String::from("Ada"),   total_cents: 1_200, status: String::from("paid") },
    ];

    println!("Total revenue: ${:.2}", total_revenue(&orders) as f64 / 100.0);

    let by_customer = revenue_by_customer(&orders);
    let mut rows: Vec<_> = by_customer.iter().collect();
    rows.sort();
    for (customer, cents) in rows {
        println!("  {customer}: ${:.2}", *cents as f64 / 100.0);
    }

    match find_order(&orders, 3) {
        Some(order) => println!("Found order {}: {} cents", order.id, order.total_cents),
        None => println!("No such order"),
    }

    // `orders` is still fully owned and usable here.
    println!("Processed {} orders.", orders.len());
}
```

**Output (compile-verified):**

```text
Total revenue: $57.00
  Ada: $57.00
Found order 3: 1200 cents
Processed 3 orders.
```

Notice that `revenue_by_customer` returns a `HashMap<&str, u64>` whose keys are *borrowed* from the orders: the borrow checker guarantees the map cannot outlive the data it points into. And `find_order` uses an explicit lifetime (`'a`) to say "the returned reference lives as long as the `orders` slice you gave me." That annotation is your bridge to the next topics: [Lifetimes](/05-ownership/04-lifetimes/) and [Lifetime Elision](/05-ownership/05-lifetime-elision/).

> **Note:** `cargo clippy` will flag `find_order`'s `'a` with `clippy::needless_lifetimes`, because the elision rules let you write `fn find_order(orders: &[Order], id: u32) -> Option<&Order>` and the compiler infers the same lifetime. The explicit `'a` is shown here on purpose, to make the "returned reference borrows from the input" relationship visible before we cover elision.

---

## Further Reading

### Official Documentation

- [The Rust Book — References and Borrowing](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html)
- [The Rust Book — The Slice Type](https://doc.rust-lang.org/book/ch04-03-slices.html)
- [Rust by Example — Borrowing](https://doc.rust-lang.org/rust-by-example/scope/borrow.html)
- [Rust Reference — References (`&` and `&mut`)](https://doc.rust-lang.org/reference/types/pointer.html)

> **Note:** This guide targets the latest stable Rust (1.96.0) and the latest stable edition (2024). Everything here is verified against current-stable tooling; `cargo new` selects the newest edition automatically.

### Related Sections in This Guide

- [Section 05 — Ownership (overview)](/05-ownership/)
- [Stack vs. Heap](/05-ownership/00-stack-heap/): where the value a reference points at actually lives
- [Ownership Rules](/05-ownership/01-ownership-rules/): the rules that make borrowing necessary
- [Mutable References](/05-ownership/03-mutable-references/): `&mut` and the one-writer-XOR-many-readers rule
- [Lifetimes](/05-ownership/04-lifetimes/): naming how long references stay valid
- [Lifetime Elision](/05-ownership/05-lifetime-elision/): when you can omit lifetime annotations
- [Move, Copy, Clone](/05-ownership/06-move-copy-clone/): what happens when you *don't* borrow
- [Reference Counting (`Rc` / `Arc`)](/05-ownership/07-reference-counting/): shared ownership when a single owner isn't enough
- [Variables and Mutability](/02-basics/00-variables/): `let` vs `let mut`, the foundation for `&` vs `&mut`
- [Introduction](/00-introduction/) and [Getting Started](/01-getting-started/): if you need a refresher on setup
- [Section 06 — Data Structures](/06-data-structures/): structs and enums you'll be borrowing next

---

## Exercises

### Exercise 1: Borrow instead of move

**Difficulty:** Beginner

**Objective:** Convert a function that consumes its argument into one that borrows it.

**Instructions:** The function below takes ownership of the `Vec`, so the call site can't use `names` afterward. Rewrite `longest_name` to **borrow** the data so that `main` can still print `names` at the end. Implement the body so it returns a reference to the longest name.

```rust
fn longest_name(names: Vec<String>) -> String {
    // TODO: borrow instead of consuming; return a reference to the longest name
    /* ??? */
}

fn main() {
    let names = vec![
        String::from("Bo"),
        String::from("Alexander"),
        String::from("Kai"),
    ];
    println!("Longest: {}", longest_name(/* ??? */));
    println!("All names still here: {:?}", names); // must still compile
}
```

<details>
<summary>Solution</summary>

```rust
fn longest_name(names: &[String]) -> &String {
    let mut longest = &names[0];
    for name in names {
        if name.len() > longest.len() {
            longest = name;
        }
    }
    longest
}

fn main() {
    let names = vec![
        String::from("Bo"),
        String::from("Alexander"),
        String::from("Kai"),
    ];
    println!("Longest: {}", longest_name(&names));
    println!("All names still here: {:?}", names);
}
```

**Output:**

```text
Longest: Alexander
All names still here: ["Bo", "Alexander", "Kai"]
```

Taking `&[String]` (a slice) instead of `Vec<String>` borrows the data, so `names` remains owned by `main`. Returning `&String` hands back a reference into that borrowed slice.

</details>

### Exercise 2: Return a borrowed slice

**Difficulty:** Intermediate

**Objective:** Write a function that returns a `&str` slice borrowed from its input.

**Instructions:** Implement `first_word` so it returns the first whitespace-delimited word of the input as a borrowed slice (no allocation, no `String`). It should accept both `&String` and `&str` literals.

```rust
fn first_word(s: &str) -> &str {
    // TODO: return everything up to the first space, or the whole string
    /* ??? */
}

fn main() {
    let sentence = String::from("borrow checker rules");
    println!("{}", first_word(&sentence)); // "borrow"
    println!("{}", first_word("hello world")); // "hello"
    println!("{}", sentence); // sentence still usable
}
```

<details>
<summary>Solution</summary>

```rust
fn first_word(s: &str) -> &str {
    match s.find(' ') {
        Some(i) => &s[..i],
        None => s,
    }
}

fn main() {
    let sentence = String::from("borrow checker rules");
    println!("{}", first_word(&sentence));
    println!("{}", first_word("hello world"));
    println!("{}", sentence);
}
```

**Output:**

```text
borrow
hello
borrow checker rules
```

The returned `&str` borrows from `s`, so it can never outlive the string it slices into; the borrow checker enforces that automatically. Accepting `&str` (rather than `&String`) lets the same function serve both a borrowed `String` and a string literal.

</details>

### Exercise 3: Prove that shared borrows alias safely

**Difficulty:** Intermediate

**Objective:** Show that multiple shared references to one value can coexist, and that the value remains usable afterward.

**Instructions:** Given the `Counter` struct, write a function `read_twice` that takes **two** shared references to a `Counter` and returns the sum of their `value` fields. In `main`, call it with two borrows of the *same* counter, then print the counter to confirm it was never consumed.

```rust
#[derive(Debug)]
struct Counter {
    value: i32,
}

fn read_twice(/* ??? */) -> i32 {
    // TODO
    /* ??? */
}

fn main() {
    let c = Counter { value: 21 };
    println!("Doubled: {}", read_twice(/* ??? */));
    println!("{:?}", c);
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
struct Counter {
    value: i32,
}

fn read_twice(a: &Counter, b: &Counter) -> i32 {
    a.value + b.value
}

fn main() {
    let c = Counter { value: 21 };
    // Two shared borrows of the same value, at once — perfectly legal.
    println!("Doubled: {}", read_twice(&c, &c));
    println!("{:?}", c);
}
```

**Output:**

```text
Doubled: 42
Counter { value: 21 }
```

Passing `&c` twice creates two simultaneous shared borrows. Because neither can mutate, this is allowed, and `c` is still owned by `main` afterward. (Try changing one parameter to `&mut Counter` and calling `read_twice(&mut c, &c)`: the borrow checker will reject it, which is the exclusivity rule covered in [Mutable References](/05-ownership/03-mutable-references/).)

</details>
