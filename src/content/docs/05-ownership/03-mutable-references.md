---
title: "Mutable References"
description: "&mut T borrows a value to mutate it under one rule: one writer XOR many readers, catching at compile time the aliasing bugs JS ships to production."
---

A **mutable reference** (`&mut T`) lets you temporarily borrow a value so you can *change* it without taking ownership. It is the counterpart to the shared, read-only borrows you saw in [Borrowing](/05-ownership/02-borrowing/), but it comes with one strict, far-reaching rule that does not exist in TypeScript or JavaScript.

---

## Quick Overview

A mutable reference is a borrow that grants write access. Rust enforces a single rule around them: at any given moment a value may have **either one mutable reference, or any number of shared (read-only) references, never both at once**. This "one writer XOR many readers" rule is checked entirely at compile time, and it is how Rust eliminates whole categories of bugs (iterator invalidation, aliasing surprises, and data races) that a TypeScript/JavaScript developer normally only catches at runtime, if at all.

---

## TypeScript/JavaScript Example

In JavaScript, objects and arrays are passed by reference, and *any* number of references can read **and** write the same value at the same time. Nothing stops you.

```typescript
// TypeScript - any number of aliases can mutate freely
interface Account {
  balance: number;
}

function deposit(account: Account, amount: number): void {
  account.balance += amount;
}

const account: Account = { balance: 100 };

// Two names for the SAME object - both can mutate it:
const alias = account;
deposit(account, 50);
alias.balance -= 10;

console.log(account.balance); // 140
console.log(alias === account); // true - same reference

// A classic runtime bug: mutating an array while iterating it
const nums = [1, 2, 3];
for (const n of nums) {
  if (n === 1) nums.push(n + 1); // appends a 2 the first (and only) time n is 1
}
console.log(nums); // [1, 2, 3, 2] - the loop also visited the pushed 2
```

Two things are worth noticing:

- `alias` and `account` are two bindings to **one** object; mutating through either is visible through the other, and there is no compiler check coordinating them.
- Mutating `nums` while a `for...of` loop is iterating it is perfectly legal JavaScript. The `1` matches once, so a single `2` is appended; the loop then visits that newly pushed element before ending, because `for...of` re-reads the live array on each step. It does not throw. With a condition that always matched the new last element (e.g. `if (n < 3) nums.push(n + 1)`), this same pattern becomes an accidental infinite loop, and the compiler would never warn you.

> **Note:** The output above is real. In Node v22, `for...of` over an array re-reads the live array on each step, so pushed elements *are* visited. `Array.prototype.forEach`, by contrast, silently *ignores* elements added during iteration. Either way, JavaScript hands you a foot-gun and trusts you not to pull the trigger.

---

## Rust Equivalent

Rust models "I want to change this value through a borrow" explicitly with `&mut`. The compiler then guarantees no other live reference exists while you hold it.

```rust playground
fn deposit(balance: &mut f64, amount: f64) {
    *balance += amount; // `*` dereferences to reach the value behind the reference
}

fn main() {
    let mut balance = 100.0;

    deposit(&mut balance, 50.0); // hand out a mutable borrow, then take it back
    deposit(&mut balance, 25.0); // and again - each borrow is short-lived

    println!("Balance: {}", balance);
}
```

```text
Balance: 175
```

And the JavaScript loop bug? Rust will not even compile it:

```rust
fn main() {
    let mut nums = vec![1, 2, 3];
    for n in &nums {          // shared borrow held for the whole loop
        if *n == 2 {
            nums.push(99);    // does not compile (error[E0502]): mutate while borrowed
        }
    }
}
```

```text
error[E0502]: cannot borrow `nums` as mutable because it is also borrowed as immutable
 --> src/main.rs:5:13
  |
3 |     for n in &nums {           // shared borrow for the whole loop
  |              -----
  |              |
  |              immutable borrow occurs here
  |              immutable borrow later used here
4 |         if *n == 2 {
5 |             nums.push(99);     // mutate while iterating
  |             ^^^^^^^^^^^^^ mutable borrow occurs here
```

The very bug that JavaScript ships to production is a compile error in Rust.

---

## Detailed Explanation

### Creating and using a `&mut`

There are two pieces of syntax to learn:

- `&mut value` — *create* a mutable reference (you must own a `mut` binding to do this).
- `*reference` — *dereference*: follow the reference to read or write the underlying value.

```rust playground
fn main() {
    let mut count = 0;       // the binding must be `mut` to be mutably borrowed
    let r = &mut count;      // r has type &mut i32

    *r += 1;                 // write through the reference
    *r += 1;

    println!("{}", count);   // 2 - the change is visible through the owner
}
```

When you call a method on a reference, Rust inserts the `*` for you (this is *automatic dereferencing*), which is why `r.push(...)` works without writing `(*r).push(...)`.

### `&mut self` methods

The most common place you will write `&mut` is method receivers. A method that takes `&mut self` can mutate the struct it is called on:

```rust playground
struct Counter {
    value: i32,
}

impl Counter {
    fn increment(&mut self) {
        self.value += 1;
    }
}

fn bump_twice(c: &mut Counter) {
    c.increment(); // reborrow of *c happens automatically
    c.increment();
}

fn main() {
    let mut counter = Counter { value: 0 };
    bump_twice(&mut counter);
    println!("value = {}", counter.value); // value = 2
}
```

```text
value = 2
```

> **Tip:** `&mut self`, `&self`, and `self` are the Rust equivalents of asking "does this method mutate the object, just read it, or consume it entirely?" In a TypeScript class every method silently has full mutable access to `this`; in Rust the receiver type makes the answer part of the signature.

### The one rule: mutable XOR shared

This is the heart of the topic. While a `&mut T` to a value is alive, **nothing else** may reference that value: not another `&mut`, and not even a read-only `&`. Conversely, while one or more shared `&T` borrows are alive, **no** `&mut` may exist. You get one of two states:

| State            | Mutable refs | Shared refs | Analogy                                  |
| ---------------- | ------------ | ----------- | ---------------------------------------- |
| Exclusive write  | exactly 1    | 0           | A `RwLock` write lock                     |
| Shared read      | 0            | many        | A `RwLock` read lock                      |

This is sometimes called **"Aliasing XOR Mutability"**: you can have aliasing (many references) *or* mutability (the ability to write), but never both for the same data at the same time.

```rust
fn main() {
    let mut data = vec![1, 2, 3];

    let a = &mut data;
    let b = &mut data; // does not compile (error[E0499]): second &mut while first is live

    a.push(4);
    b.push(5);
}
```

```text
error[E0499]: cannot borrow `data` as mutable more than once at a time
 --> src/main.rs:5:13
  |
4 |     let a = &mut data;
  |             --------- first mutable borrow occurs here
5 |     let b = &mut data; // does not compile (error[E0499]): second &mut while first is live
  |             ^^^^^^^^^ second mutable borrow occurs here
6 |
7 |     a.push(4);
  |     - first borrow later used here
```

### Non-lexical lifetimes (NLL): a borrow ends at its last use

Earlier (pre-2018) the rule above would have been painfully strict, because a borrow used to last until the end of its enclosing `{}` block. Modern Rust uses **non-lexical lifetimes**: a borrow ends at its **last use**, not at the closing brace. That makes the rule far more pleasant in practice: borrows you are "done with" stop counting immediately.

```rust playground
fn main() {
    let mut scores = vec![10, 20, 30];

    let first = &scores[0];              // shared borrow starts
    println!("First score: {}", first);  // ...and ends here (last use of `first`)

    scores.push(40); // mutable borrow is fine now - the shared borrow already ended
    println!("Scores: {:?}", scores);
}
```

```text
First score: 10
Scores: [10, 20, 30, 40]
```

Both borrows touch `scores`, yet this compiles: the shared borrow's lifetime ends after the first `println!`, so the later mutable borrow does not overlap it. If you reordered the code so `first` were used *after* `scores.push(40)`, the borrows would overlap and you would get the `E0502` error from the Quick Overview.

> **Note:** "Lifetime" here means the span of code over which a reference is actually used, not a wall-clock duration and not the lexical scope. Annotated lifetimes (`'a`) are a related but separate topic; see [Lifetimes](/05-ownership/04-lifetimes/).

### Why this prevents data races at compile time

A **data race** is when two or more threads access the same memory at the same time, at least one access is a write, and there is no synchronization. The mutable-XOR-shared rule makes data races *structurally impossible*: if writing requires exclusive access, two threads can never both hold a writer to the same data.

The borrow checker enforces this even across threads. Here two scoped threads each try to mutate the same counter:

```rust
use std::thread;

fn main() {
    let mut total = 0u64;

    thread::scope(|s| {
        s.spawn(|| {
            total += 1; // does not compile (error[E0499]): both closures want &mut total
        });
        s.spawn(|| {
            total += 1;
        });
    });

    println!("{}", total);
}
```

```text
error[E0499]: cannot borrow `total` as mutable more than once at a time
  --> src/main.rs:10:17
   |
 6 |       thread::scope(|s| {
   |                      - has type `&'1 Scope<'1, '_>`
 7 |           s.spawn(|| {
   |           -       -- first mutable borrow occurs here
   |  _________|
   | |
 8 | |             total += 1; // both closures want &mut total
   | |             ----- first borrow occurs due to use of `total` in closure
 9 | |         });
   | |__________- argument requires that `total` is borrowed for `'1`
10 |           s.spawn(|| {
   |                   ^^ second mutable borrow occurs here
```

The exact code that would be a *runtime* heisenbug in JavaScript (if JavaScript had shared-memory threads) is a *compile-time* error in Rust. To actually share writable state across threads you must reach for a synchronization type such as `Mutex<T>` or `Arc<Mutex<T>>`, which re-establish the "one writer at a time" guarantee at runtime, covered in [Reference Counting](/05-ownership/07-reference-counting/) and Section 10.

---

## Key Differences

| Concept                         | TypeScript/JavaScript                                  | Rust                                                              |
| ------------------------------- | ------------------------------------------------------ | ----------------------------------------------------------------- |
| Mutating through an alias       | Always allowed; unlimited aliases can write            | At most one `&mut` at a time; checked at compile time             |
| Read + write at once            | Allowed (reader sees writer's changes mid-flight)      | Forbidden: `&mut` excludes all `&` and vice versa                 |
| Mutating a collection mid-loop  | Allowed; silent bugs / accidental infinite loops       | Compile error (`E0502`)                                           |
| Data races                      | N/A (single-threaded) / possible with SharedArrayBuffer | Impossible by construction; the type system forbids them          |
| When a "borrow" ends            | No concept; GC reclaims when unreachable               | At the reference's last use (non-lexical lifetimes)               |
| Declaring intent to mutate      | Implicit: every method can mutate `this`              | Explicit: `&mut` in the type, `mut` on the binding               |

### Mutability is a property of the borrow, not just the value

In JavaScript, a `const` binding to an object still lets you mutate the object. Rust separates three independent questions:

1. Is the **binding** mutable? (`let` vs `let mut`)
2. Is this particular **borrow** allowed to write? (`&` vs `&mut`)
3. Does anyone *else* hold a borrow right now? (the XOR rule)

You need a `mut` binding to create a `&mut`, the reference type must be `&mut`, and the borrow checker must confirm exclusivity. All three line up to make "who can change this, and when" explicit.

### `&mut` is not a "pointer you copy around"

A `&mut T` cannot be freely copied the way a JavaScript reference can. If you pass it along, the original is **reborrowed** (temporarily lent out) and you cannot use it until the reborrow ends. This is what keeps "exactly one writer" true even as the reference travels through function calls.

---

## Common Pitfalls

### Pitfall 1: Forgetting `mut` on the binding

You cannot take a `&mut` to a value bound with plain `let`.

```rust
fn main() {
    let config = String::from("debug=false");
    let r = &mut config; // does not compile (error[E0596])
    r.push_str(";verbose=true");
    println!("{}", config);
}
```

Real compiler output:

```text
error[E0596]: cannot borrow `config` as mutable, as it is not declared as mutable
 --> src/main.rs:3:13
  |
3 |     let r = &mut config; // cannot borrow immutable binding as mutable
  |             ^^^^^^^^^^^ cannot borrow as mutable
  |
help: consider changing this to be mutable
  |
2 |     let mut config = String::from("debug=false");
  |         +++
```

**Fix:** change `let config` to `let mut config`. The compiler even tells you exactly that.

### Pitfall 2: Holding a shared borrow, then trying to mutate

This is the iterator-invalidation bug from the intro, but it also bites in simpler code:

```rust
fn main() {
    let mut items = vec![1, 2, 3];

    let shared = &items;       // shared (immutable) borrow
    items.push(4);             // does not compile (error[E0502])
    println!("{:?}", shared);  // shared borrow used here, so it is still live
}
```

Real compiler output:

```text
error[E0502]: cannot borrow `items` as mutable because it is also borrowed as immutable
 --> src/main.rs:5:5
  |
4 |     let shared = &items;       // shared (immutable) borrow
  |                  ------ immutable borrow occurs here
5 |     items.push(4);             // mutable borrow while shared borrow is live
  |     ^^^^^^^^^^^^^ mutable borrow occurs here
6 |     println!("{:?}", shared);  // shared borrow used here
  |                      ------ immutable borrow later used here
```

**Fix:** finish using `shared` *before* the mutation (NLL will then let the borrow end), or take a fresh `&items` after the `push`.

### Pitfall 3: Two `&mut` to the same value

Expecting JavaScript-style aliasing, a newcomer might hand a function the same mutable reference twice, or split out two mutable borrows. The borrow checker stops the second one (`E0499`, shown in the Detailed Explanation). **Fix:** restructure so only one `&mut` is live at a time, or — for genuinely disjoint parts of a collection — use `split_at_mut` (see Best Practices).

### Pitfall 4: Assuming `*` is optional everywhere

Method calls auto-dereference, so `r.push(4)` works. But plain operators do not: writing `r += 1` when `r: &mut i32` fails because you are trying to add to the reference, not the value. You must write `*r += 1`. The rule of thumb: **use `*` whenever you read or assign the value itself** rather than calling a method on it.

---

## Best Practices

### Keep mutable borrows as short as possible

Because a `&mut` blocks all other access, the idiom is to take it, do the mutation, and let it end immediately. NLL rewards this: the sooner the reference's last use, the sooner the value is free again.

```rust playground
fn main() {
    let mut log = Vec::new();
    log.push("started");          // implicit short-lived &mut log
    let len = log.len();          // read access after the &mut already ended
    println!("{len} entries: {log:?}");
}
```

### Use `iter_mut()` to mutate every element of a collection

Do not index in a loop with manual bookkeeping; ask the collection for mutable references to its elements.

```rust playground
fn restock_all(inventory: &mut [u32], extra: u32) {
    for stock in inventory.iter_mut() { // each `stock` is &mut u32
        *stock += extra;
    }
}

fn main() {
    let mut levels = [10, 20, 30];
    restock_all(&mut levels, 5);
    println!("{levels:?}"); // [15, 25, 35]
}
```

### Reach for `split_at_mut` when you truly need two `&mut` into one collection

The XOR rule forbids two `&mut` to the *same* value, but two `&mut` to *disjoint* parts are perfectly safe. The standard library exposes this with `split_at_mut`, which hands you two non-overlapping mutable slices:

```rust playground
fn main() {
    let mut data = vec![1, 2, 3, 4, 5, 6];

    // Split into two non-overlapping mutable slices.
    let (left, right) = data.split_at_mut(3);

    left[0] += 100;
    right[0] += 200;

    println!("{:?}", data); // [101, 2, 3, 204, 5, 6]
}
```

```text
[101, 2, 3, 204, 5, 6]
```

### Prefer `std::mem::swap` / `take` / `replace` over fighting the borrow checker

When you need to move a value *out* through a `&mut` (e.g. to reset a field), these helpers do it without a second borrow:

```rust playground
fn main() {
    let mut a = String::from("first");
    let mut b = String::from("second");
    std::mem::swap(&mut a, &mut b);
    println!("a={a}, b={b}"); // a=second, b=first

    // `take` leaves Default::default() behind and returns the old value.
    let mut owned = vec![1, 2, 3];
    let stolen = std::mem::take(&mut owned);
    println!("stolen={stolen:?}, owned={owned:?}"); // stolen=[1, 2, 3], owned=[]
}
```

```text
a=second, b=first
stolen=[1, 2, 3], owned=[]
```

### Expose mutation through `&mut self` methods, not public fields

Returning a `&mut` to internal state ties that reference's lifetime to the borrow of `self`, so the XOR rule still protects your invariants:

```rust playground
struct Config {
    retries: u32,
}

impl Config {
    fn retries_mut(&mut self) -> &mut u32 {
        &mut self.retries
    }
}

fn main() {
    let mut config = Config { retries: 3 };
    *config.retries_mut() += 2;
    println!("retries = {}", config.retries); // retries = 5
}
```

---

## Real-World Example

A small inventory service. Notice how each mutation goes through a clearly scoped mutable borrow: `apply_sale` borrows one product, `restock_all` borrows the whole slice, and the final read-only loop only runs once all mutable borrows have ended.

```rust playground
#[derive(Debug)]
struct Product {
    name: String,
    stock: u32,
    price_cents: u64,
}

/// Applies a sale to a single product: drops the price by `percent`
/// and reserves `qty` units. Mutates in place through `&mut Product`.
fn apply_sale(product: &mut Product, percent: u64, qty: u32) {
    product.price_cents -= product.price_cents * percent / 100;
    product.stock = product.stock.saturating_sub(qty);
}

/// Bulk update: one mutable borrow of the whole slice, mutating each element.
fn restock_all(inventory: &mut [Product], extra: u32) {
    for product in inventory.iter_mut() {
        product.stock += extra;
    }
}

fn main() {
    let mut inventory = vec![
        Product { name: "Keyboard".into(), stock: 12, price_cents: 7999 },
        Product { name: "Mouse".into(),    stock: 30, price_cents: 2999 },
    ];

    // One mutable borrow at a time, scoped to the call:
    apply_sale(&mut inventory[0], 25, 2);

    // A mutable borrow of the whole slice for a bulk update:
    restock_all(&mut inventory, 5);

    // Read-only pass: no &mut is alive here, so a shared borrow is fine.
    for product in &inventory {
        println!(
            "{:<10} stock={:>3} price=${:.2}",
            product.name,
            product.stock,
            product.price_cents as f64 / 100.0
        );
    }
}
```

```text
Keyboard   stock= 15 price=$60.00
Mouse      stock= 35 price=$29.99
```

In TypeScript you would write the equivalent freely, with several aliases all able to mutate `inventory` concurrently. The Rust version reads almost the same, but the compiler has verified that no two parts of the program can mutate the same product at the same time, and it cost you nothing at runtime.

---

## Further Reading

### Official Documentation

- [The Rust Book — References and Borrowing](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html)
- [The Rust Book — Fearless Concurrency](https://doc.rust-lang.org/book/ch16-00-concurrency.html)
- [The Rust Reference — Lifetime and Borrow Rules](https://doc.rust-lang.org/reference/expressions.html#mutability)
- [`slice::split_at_mut`](https://doc.rust-lang.org/std/primitive.slice.html#method.split_at_mut)
- [`std::mem::swap`](https://doc.rust-lang.org/std/mem/fn.swap.html), [`std::mem::take`](https://doc.rust-lang.org/std/mem/fn.take.html), [`std::mem::replace`](https://doc.rust-lang.org/std/mem/fn.replace.html)

### Related Topics in This Guide

- [Borrowing](/05-ownership/02-borrowing/): shared `&` references, where mutable references build from
- [Ownership Rules](/05-ownership/01-ownership-rules/): the rules that mutable references operate within
- [Stack and Heap](/05-ownership/00-stack-heap/): what `&mut` actually points at
- [Lifetimes](/05-ownership/04-lifetimes/) and [Lifetime Elision](/05-ownership/05-lifetime-elision/) — how long a reference is valid
- [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/): moving values vs borrowing them
- [Reference Counting](/05-ownership/07-reference-counting/) — shared *ownership* (`Rc`/`Arc`) when one owner is not enough
- [Variables and Mutability](/02-basics/00-variables/): the `mut` keyword on bindings
- [Data Structures](/06-data-structures/) — `&mut self` methods on structs and enums

---

## Exercises

### Exercise 1: Increment in Place

**Difficulty:** Easy

**Objective:** Write a function that adds 1 to every element of a slice through a mutable reference.

**Instructions:** Complete `increment_all` so that it mutates the caller's data in place (no allocation, no return value).

```rust playground
fn increment_all(values: &mut [i32]) {
    // TODO: add 1 to every element
}

fn main() {
    let mut nums = vec![1, 2, 3];
    increment_all(&mut nums);
    println!("{nums:?}"); // expected: [2, 3, 4]
}
```

<details>
<summary>Solution</summary>

```rust playground
fn increment_all(values: &mut [i32]) {
    for v in values.iter_mut() {
        *v += 1; // `v` is &mut i32, so dereference to write
    }
}

fn main() {
    let mut nums = vec![1, 2, 3];
    increment_all(&mut nums);
    println!("{nums:?}"); // [2, 3, 4]
}
```

Output:

```text
[2, 3, 4]
```

</details>

### Exercise 2: Normalize a Vector

**Difficulty:** Medium

**Objective:** Mutate a vector of `f64` in place so every value is divided by the maximum, without holding two conflicting borrows.

**Instructions:** Implement `normalize`. First compute the maximum (a read-only pass), then scale every element (a mutable pass). The trick is to make sure the read borrow has fully ended before the write borrow begins; non-lexical lifetimes will allow this if you compute `max` into its own variable first.

```rust playground
fn normalize(values: &mut Vec<f64>) {
    // TODO: divide every element by the maximum value
}

fn main() {
    let mut samples = vec![2.0, 4.0, 8.0];
    normalize(&mut samples);
    println!("{samples:?}"); // expected: [0.25, 0.5, 1.0]
}
```

<details>
<summary>Solution</summary>

```rust playground
fn normalize(values: &mut Vec<f64>) {
    // Read pass: the shared borrow inside `iter()` ends when `max` is computed.
    let max = values.iter().cloned().fold(f64::MIN, f64::max);
    if max == 0.0 {
        return; // avoid dividing by zero
    }
    // Write pass: now the &mut is exclusive.
    for v in values.iter_mut() {
        *v /= max;
    }
}

fn main() {
    let mut samples = vec![2.0, 4.0, 8.0];
    normalize(&mut samples);
    println!("{samples:?}"); // [0.25, 0.5, 1.0]
}
```

Output:

```text
[0.25, 0.5, 1.0]
```

</details>

### Exercise 3: A Counter with `&mut self`

**Difficulty:** Medium

**Objective:** Build a `Counter` struct whose `increment` method mutates the counter through `&mut self` and returns the new value.

**Instructions:** Implement `Counter::new` and `Counter::increment`. Calling `increment` twice on a fresh counter should print `1` then `2`. Think about why the binding in `main` must be `mut`.

```rust
struct Counter {
    count: u32,
}

impl Counter {
    fn new() -> Self {
        // TODO
    }

    fn increment(&mut self) -> u32 {
        // TODO: bump the count and return the new value
    }
}

fn main() {
    let mut c = Counter::new();
    println!("{}", c.increment()); // 1
    println!("{}", c.increment()); // 2
}
```

<details>
<summary>Solution</summary>

```rust playground
struct Counter {
    count: u32,
}

impl Counter {
    fn new() -> Self {
        Counter { count: 0 }
    }

    fn increment(&mut self) -> u32 {
        self.count += 1;
        self.count
    }
}

fn main() {
    let mut c = Counter::new(); // `mut` is required to call a &mut self method
    println!("{}", c.increment()); // 1
    println!("{}", c.increment()); // 2
}
```

Output:

```text
1
2
```

`main`'s binding must be `let mut c` because `increment` takes `&mut self`: calling it implicitly creates a mutable borrow of `c`, which is only allowed on a mutable binding.

</details>
