---
title: "Vectors: From `Array` to `Vec<T>`"
description: "Rust's Vec<T> is the JS Array's counterpart: push, pop, and iterate, but with one element type, bounds-checked indexing that panics, and ownership moves."
---

The JavaScript `Array` is the workhorse of every TypeScript codebase. Its direct Rust counterpart is `Vec<T>`: a growable, heap-allocated, contiguous list. This page maps everything you already do with arrays (`push`, `pop`, indexing, iteration) onto `Vec<T>`, and introduces two ideas JavaScript hides from you: a single **element type** and explicit **capacity**.

---

## Quick Overview

A **`Vec<T>`** (pronounced "vector") is Rust's growable array: a contiguous, heap-allocated sequence of values that all share one type `T`. Unlike a JavaScript `Array`, it cannot hold mixed types, its length is a `usize`, and it exposes its memory **capacity** so you can pre-allocate and avoid reallocations. If you reach for `Array` in TypeScript, you almost always reach for `Vec<T>` in Rust.

---

## TypeScript/JavaScript Example

```typescript
// A small "recently viewed products" list in a web app.
const recentlyViewed: string[] = [];

recentlyViewed.push("keyboard");
recentlyViewed.push("monitor");
recentlyViewed.push("mouse");

// Indexing — no bounds checking, returns `undefined` past the end.
const first = recentlyViewed[0]; // "keyboard"
const missing = recentlyViewed[99]; // undefined (no error!)

// Remove the most recent item.
const last = recentlyViewed.pop(); // "mouse"

// Iterate.
for (const item of recentlyViewed) {
  console.log(item);
}

// Iterate with an index.
recentlyViewed.forEach((item, i) => {
  console.log(`${i}: ${item}`);
});

console.log(recentlyViewed.length); // 2

// JavaScript arrays can hold mixed types — TypeScript discourages it,
// but the runtime allows it.
const mixed: unknown[] = [1, "two", true];
```

Things to notice that Rust will handle differently: indexing out of bounds returns `undefined` instead of erroring, `pop()` on an empty array returns `undefined`, and `length` is just a `number`.

---

## Rust Equivalent

```rust
fn main() {
    // A growable list of `String`s. The `T` here is `String`.
    let mut recently_viewed: Vec<String> = Vec::new();

    recently_viewed.push("keyboard".to_string());
    recently_viewed.push("monitor".to_string());
    recently_viewed.push("mouse".to_string());

    // Indexing — bounds-checked. `[99]` would PANIC, so prefer `.get()`.
    // Read the values we want to keep BEFORE mutating with `pop`, and bind
    // them as OWNED (not references) so no borrow stays live across `pop`.
    let first = recently_viewed[0].clone(); // owned String -> "keyboard"
    let missing = recently_viewed.get(99).cloned(); // Option<String> -> None

    // Remove the most recent item. `pop` returns `Option<String>`.
    let last = recently_viewed.pop(); // Some("mouse")

    // Iterate by shared reference (`&` borrows, does not consume).
    for item in &recently_viewed {
        println!("{item}");
    }

    // Iterate with an index using the `enumerate` adaptor.
    for (i, item) in recently_viewed.iter().enumerate() {
        println!("{i}: {item}");
    }

    println!("{}", recently_viewed.len()); // 2 (a usize)
    println!("{first}, {last:?}, {missing:?}");

    // You CANNOT mix types — this would not compile:
    // let mixed = vec![1, "two", true]; // mismatched types
}
```

> **Note:** `[T]` (a slice) and `Vec<T>` are different things. `Vec<T>` *owns* its heap buffer; a slice `&[T]` is a borrowed *view* into one. Most read-only functions should take `&[T]`, not `&Vec<T>`. See [Best Practices](#best-practices).

---

## Detailed Explanation

### Creating a `Vec`

There are three idiomatic ways to make one, and the choice mirrors how much you know up front.

```rust
fn main() {
    // 1. Empty, type annotated. Use when you'll push later.
    let mut empty: Vec<i32> = Vec::new();
    empty.push(10);

    // 2. The `vec!` macro — like an array literal.
    let nums = vec![1, 2, 3, 4, 5];

    // 3. `vec![value; count]` — repeat `value`, `count` times.
    let zeros = vec![0; 5]; // [0, 0, 0, 0, 0]

    println!("{empty:?}");
    println!("{nums:?}");
    println!("{zeros:?}");
}
```

Verified output:

```text
[10]
[1, 2, 3, 4, 5]
[0, 0, 0, 0, 0]
```

`vec!` is a **macro** (note the `!`), not a function. The `vec![value; count]` form is the one TypeScript has no clean equivalent for; `new Array(5).fill(0)` is the closest, but `vec![0; 5]` is a single allocation with no surprises (`new Array(5)` famously creates "holes").

> **Tip:** When you build a `Vec` element-by-element from another sequence, prefer `.collect()` over a manual `push` loop: `let r: Vec<i32> = (1..=5).collect();` produces `[1, 2, 3, 4, 5]`. See [Iterator Consumers](/07-collections/07-iterator-consumers/).

### `push` and `pop`

```rust
fn main() {
    let mut stack = vec![1, 2, 3];
    stack.push(4); // append to the end (amortized O(1))
    let last = stack.pop(); // remove from the end -> Option<i32>
    println!("popped {last:?}, now {stack:?}");
}
```

Verified output:

```text
popped Some(4), now [1, 2, 3]
```

The critical difference from JavaScript: **`pop` returns `Option<T>`, not `T | undefined`.** When the vector is empty, you get `None` rather than a value you might forget to check. The compiler forces you to handle the empty case. (`Option` is covered in [Section 02 — Basics: Types](/02-basics/01-types/) and used heavily throughout [Section 08 — Error Handling](/08-error-handling/).)

### Indexing vs `.get()`

This is the single biggest behavioral difference from JavaScript arrays.

```rust
fn main() {
    let v = vec![10, 20, 30];

    let a = v[0]; // 10 — direct indexing, PANICS if out of bounds
    let b = v.get(10); // None — safe, returns Option<&T>

    println!("a = {a}, b = {b:?}");
}
```

Verified output:

```text
a = 10, b = None
```

- `v[i]` returns the element directly (a copy here, since `i32` is `Copy`), but **panics** if `i >= v.len()`.
- `v.get(i)` returns `Option<&T>`: `Some(&value)` or `None`. No panic, ever.

In JavaScript, `arr[99]` silently gives `undefined`; in Rust, `v[99]` aborts the program. Use `.get()` whenever the index might be out of range, and reserve `v[i]` for indices you have already proven valid.

### Iteration: three flavors of borrow

JavaScript has one iteration model. Rust has three, and the difference is *ownership*:

```rust
fn main() {
    let mut scores = vec![1, 2, 3];

    // `&v` -> iterate by shared reference (read-only). Vec stays usable.
    for s in &scores {
        print!("{s} ");
    }
    println!();

    // `&mut v` -> iterate by mutable reference. `s` is `&mut i32`; deref to write.
    for s in &mut scores {
        *s *= 10;
    }
    println!("{scores:?}");

    // `v` (by value) -> `into_iter()`, CONSUMES the Vec, yields owned values.
    let owned = vec![String::from("x"), String::from("y")];
    for s in owned.into_iter() {
        print!("{s} ");
    }
    println!();
    // `owned` is gone here — it was moved into the loop.
}
```

Verified output:

```text
1 2 3 
[10, 20, 30]
x y 
```

| You write           | Method called  | Item type | Vec afterwards     | JS analogy                |
| ------------------- | -------------- | --------- | ------------------ | ------------------------- |
| `for x in &v`       | `iter()`       | `&T`      | still usable       | `for...of` (read)         |
| `for x in &mut v`   | `iter_mut()`   | `&mut T`  | still usable       | `for...of` + mutate       |
| `for x in v`        | `into_iter()`  | `T`       | **consumed/moved** | (no direct equivalent)    |

> **Note:** `*s *= 10` dereferences the mutable reference to write through it. In the `&mut` loop, `s` has type `&mut i32`, so `*s` is the `i32` itself. JavaScript has no concept of "iterate but you may only read."

### Capacity and growth

A `Vec` tracks two numbers: **length** (how many elements it holds) and **capacity** (how many it *could* hold before it must allocate a bigger buffer). JavaScript hides this entirely; Rust exposes it because it directly affects performance.

```rust
fn main() {
    let mut v: Vec<i32> = Vec::new();
    let mut last_cap = v.capacity();
    println!("start cap = {last_cap}");
    for i in 0..20 {
        v.push(i);
        if v.capacity() != last_cap {
            println!("len {} triggered growth: cap {} -> {}", v.len(), last_cap, v.capacity());
            last_cap = v.capacity();
        }
    }
}
```

Verified output:

```text
start cap = 0
len 1 triggered growth: cap 0 -> 4
len 5 triggered growth: cap 4 -> 8
len 9 triggered growth: cap 8 -> 16
len 17 triggered growth: cap 16 -> 32
```

When `push` runs out of capacity, the `Vec` allocates a larger buffer (currently it roughly **doubles**) and copies the old elements over. That copy is why an individual `push` is *amortized* O(1) rather than strictly O(1). An empty `Vec::new()` starts with **zero** capacity and allocates lazily on the first push.

If you know roughly how many elements you'll store, pre-allocate with `Vec::with_capacity(n)` to skip the intermediate reallocations:

```rust
fn main() {
    let mut c: Vec<i32> = Vec::with_capacity(10);
    println!("len={}, cap={}", c.len(), c.capacity());
    for i in 0..10 {
        c.push(i);
    }
    println!("len={}, cap={}", c.len(), c.capacity());
    c.push(99); // exceeds capacity 10 -> reallocates
    println!("after one more: len={}, cap={}", c.len(), c.capacity());
}
```

Verified output:

```text
len=0, cap=10
len=10, cap=10
after one more: len=11, cap=20
```

Note that `with_capacity(10)` sets capacity to 10 while length stays 0: capacity is *room*, not *contents*.

### Other everyday methods

```rust
fn main() {
    let mut v = vec![1, 2, 3, 4, 5, 6];

    println!("contains 3? {}", v.contains(&3)); // membership (takes &T)
    v.retain(|&x| x % 2 == 0); // keep elements matching predicate (in place)
    println!("after retain: {v:?}");
    v.extend([8, 10]); // append all items from another iterable
    println!("after extend: {v:?}");

    let mut letters = vec!['a', 'c', 'd'];
    letters.insert(1, 'b'); // insert at index — shifts the rest, O(n)
    println!("{letters:?}");
    let removed = letters.remove(0); // remove at index — shifts the rest, O(n)
    println!("removed {removed}, now {letters:?}");

    let mut t = vec![1, 2, 3, 4];
    let s = t.swap_remove(0); // O(1) remove, but does NOT preserve order
    println!("swap_remove gave {s}, now {t:?}");

    let scores = vec![10, 20, 30];
    let total: i32 = scores.iter().sum();
    println!("total={total}, first={:?}, last={:?}", scores.first(), scores.last());

    let data = vec![1, 2, 3, 4, 5];
    let middle = &data[1..4]; // a slice &[i32] — a borrowed view, no copy
    println!("slice: {middle:?}");
}
```

Verified output:

```text
contains 3? true
after retain: [2, 4, 6]
after extend: [2, 4, 6, 8, 10]
['a', 'b', 'c', 'd']
removed a, now ['b', 'c', 'd']
swap_remove gave 1, now [4, 2, 3]
total=60, first=Some(10), last=Some(30)
slice: [2, 3, 4]
```

`first()` and `last()` return `Option<&T>` (safe), unlike `arr[0]` / `arr[arr.length - 1]` in JavaScript. `&data[1..4]` produces a **slice**, covered in depth in [Strings](/07-collections/01-strings/) (for `&str`) and [Section 05 — Ownership](/05-ownership/).

---

## Key Differences

| Concept                  | TypeScript `Array<T>`              | Rust `Vec<T>`                                        |
| ------------------------ | ---------------------------------- | ---------------------------------------------------- |
| Element types            | Can be heterogeneous at runtime    | Strictly homogeneous: one `T`                        |
| Out-of-bounds index      | Returns `undefined`                | `v[i]` **panics**; `v.get(i)` returns `None`         |
| `pop()` when empty       | `undefined`                        | `None` (an `Option<T>`)                              |
| Length type              | `number` (f64)                     | `usize`                                              |
| Memory model             | Engine-managed, opaque             | Explicit `len` + `capacity`, heap-allocated          |
| Pre-allocation           | Not really exposed                 | `Vec::with_capacity(n)`                              |
| Copy on assignment       | Reference copied (shared)          | Value **moved** (ownership transfers)                |
| Removing from the middle | `splice` (O(n))                    | `remove` (O(n), ordered) or `swap_remove` (O(1))     |
| Negative indices         | `arr.at(-1)`                       | No negative indices; use `.last()` or `v[v.len()-1]` |

### The ownership difference that bites first

```typescript
const a = [1, 2, 3];
const b = a; // b and a point at the SAME array
b.push(4);
console.log(a); // [1, 2, 3, 4] — both see the change
```

```rust
let a = vec![1, 2, 3];
let b = a; // ownership MOVED to b; `a` is no longer usable
// println!("{a:?}"); // would not compile: value borrowed after move
```

In JavaScript, `b = a` aliases the same array. In Rust, it **moves** ownership, so `a` becomes invalid. To get two independent vectors, call `a.clone()`. To share read access, borrow with `&a`. This is the core ownership story from [Section 05 — Ownership](/05-ownership/), and it is the most common surprise for TypeScript developers.

---

## Common Pitfalls

### Pitfall 1: Mutating a `Vec` while iterating over it

In JavaScript you can (dangerously) `push` inside a `for...of`. Rust's borrow checker forbids it outright, at compile time.

```rust
fn main() {
    let mut v = vec![1, 2, 3];
    for x in &v {
        if *x == 2 {
            v.push(10); // does not compile (error[E0502])
        }
    }
}
```

Real compiler error:

```text
error[E0502]: cannot borrow `v` as mutable because it is also borrowed as immutable
 --> src/main.rs:5:13
  |
3 |     for x in &v {
  |              --
  |              |
  |              immutable borrow occurs here
  |              immutable borrow later used here
4 |         if *x == 2 {
5 |             v.push(10); // does not compile (error[E0502])
  |             ^^^^^^^^^^ mutable borrow occurs here
```

**Fix:** collect the changes first, or use `retain`/`extend`, or iterate over indices. For example, decide what to add, then push after the loop:

```rust
fn main() {
    let mut v = vec![1, 2, 3];
    let mut to_add = Vec::new();
    for x in &v {
        if *x == 2 {
            to_add.push(10);
        }
    }
    v.extend(to_add);
    println!("{v:?}"); // [1, 2, 3, 10]
}
```

### Pitfall 2: Indexing with the wrong integer type

`Vec` is indexed by `usize`, not `i32`. A plain `let i = 1;` infers `i32` by default and won't work as an index.

```rust
fn main() {
    let i: i32 = 1;
    let v = vec![10, 20, 30];
    let x = v[i]; // does not compile (error[E0277])
    println!("{x}");
}
```

Real compiler error (trimmed):

```text
error[E0277]: the type `[{integer}]` cannot be indexed by `i32`
 --> src/main.rs:4:15
  |
4 |     let x = v[i]; // does not compile (error[E0277])
  |               ^ slice indices are of type `usize` or ranges of `usize`
```

**Fix:** use `usize` for indices (`let i: usize = 1;`) or cast with `i as usize`. Loop counters from `0..v.len()` are already `usize`.

### Pitfall 3: Out-of-bounds indexing panics at runtime

Because `v[i]` is bounds-checked but not type-checked against the length, an out-of-range index compiles fine and then panics when it runs.

```rust
fn main() {
    let v = vec![1, 2, 3];
    let x = v[10]; // compiles, but PANICS at runtime
    println!("{x}");
}
```

Real runtime output:

```text
thread 'main' panicked at src/main.rs:3:14:
index out of bounds: the len is 3 but the index is 10
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

**Fix:** when the index might be invalid, use `.get(i)` and handle the `Option`:

```rust
fn main() {
    let v = vec![1, 2, 3];
    match v.get(10) {
        Some(x) => println!("got {x}"),
        None => println!("no element at index 10"),
    }
}
```

### Pitfall 4: Using a `Vec` after moving it into a function

Passing a `Vec` by value transfers ownership; the caller can no longer use it.

```rust
fn total(v: Vec<i32>) -> i32 {
    v.iter().sum()
}

fn main() {
    let nums = vec![1, 2, 3];
    let t = total(nums); // `nums` moved into `total`
    println!("{t}");
    println!("{}", nums.len()); // does not compile (error[E0382])
}
```

Real compiler error (trimmed):

```text
error[E0382]: borrow of moved value: `nums`
 --> src/main.rs:8:20
  |
5 |     let nums = vec![1, 2, 3];
  |         ---- move occurs because `nums` has type `Vec<i32>`, which does not implement the `Copy` trait
6 |     let t = total(nums);   // nums moved here
  |                   ---- value moved here
...
8 |     println!("{}", nums.len()); // use after move
  |                    ^^^^ value borrowed here after move
```

**Fix:** take a slice instead (`fn total(v: &[i32]) -> i32`) and call `total(&nums)`. The function borrows, so `nums` stays usable. This is the idiomatic signature; see Best Practices below.

---

## Best Practices

### 1. Accept `&[T]`, return `Vec<T>`

A read-only function should take a **slice** `&[T]`, not `&Vec<T>`. Slices accept vectors, arrays, and sub-ranges alike, so your function is more reusable and avoids forcing the caller to own a `Vec`.

```rust
// Idiomatic: works for `&Vec<T>`, `&[T; N]`, and slices.
fn average(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn main() {
    let v = vec![20.5, 22.0, 19.5];
    println!("{}", average(&v)); // a Vec coerces to &[f64] automatically
    let arr = [1.0, 2.0, 3.0];
    println!("{}", average(&arr)); // so does an array
}
```

### 2. Pre-allocate when you know the size

If you're about to push `n` items, call `Vec::with_capacity(n)` first to avoid repeated reallocations. Capacity tuning matters at scale; see [Collection Performance](/07-collections/09-collection-performance/).

### 3. Prefer iterators and `collect` over manual index loops

Building a new `Vec` by transforming another is clearer (and often faster) with iterator adaptors than with a manual `for` + `push`:

```rust
fn main() {
    let prices = vec![100, 250, 75];
    // idiomatic
    let with_tax: Vec<i32> = prices.iter().map(|p| p * 110 / 100).collect();
    println!("{with_tax:?}"); // [110, 275, 82]
}
```

See [Iterators](/07-collections/06-iterators/) and [Iterator Consumers](/07-collections/07-iterator-consumers/).

### 4. Choose the right removal method

`remove(i)` preserves order but is O(n) (it shifts every later element). If order doesn't matter, `swap_remove(i)` is O(1). Don't pay for ordering you don't need.

### 5. Use `.get()` / `.first()` / `.last()` at boundaries

When an index could be out of range (user input, parsed data), use the `Option`-returning accessors so a bad index is a handled `None` rather than a panic.

---

## Real-World Example

A shopping cart: a `Vec` of line items with pre-allocation, predicate-based removal, and an aggregate computed with an iterator.

```rust
#[derive(Debug, Clone)]
struct CartItem {
    name: String,
    price_cents: u64,
    quantity: u32,
}

struct Cart {
    items: Vec<CartItem>,
}

impl Cart {
    fn new() -> Self {
        // Pre-size: most carts hold a handful of items.
        Cart {
            items: Vec::with_capacity(8),
        }
    }

    fn add(&mut self, name: &str, price_cents: u64, quantity: u32) {
        self.items.push(CartItem {
            name: name.to_string(),
            price_cents,
            quantity,
        });
    }

    fn remove_out_of_stock(&mut self, out_of_stock: &[&str]) {
        // retain keeps only items NOT in the out-of-stock list.
        self.items
            .retain(|item| !out_of_stock.contains(&item.name.as_str()));
    }

    fn subtotal_cents(&self) -> u64 {
        self.items
            .iter()
            .map(|item| item.price_cents * item.quantity as u64)
            .sum()
    }

    fn most_expensive(&self) -> Option<&CartItem> {
        self.items.iter().max_by_key(|item| item.price_cents)
    }
}

fn main() {
    let mut cart = Cart::new();
    cart.add("Mechanical Keyboard", 12_900, 1);
    cart.add("USB-C Cable", 1_200, 3);
    cart.add("Discontinued Mouse", 4_500, 1);

    cart.remove_out_of_stock(&["Discontinued Mouse"]);

    for (i, item) in cart.items.iter().enumerate() {
        println!(
            "{}. {} x{} @ ${:.2}",
            i + 1,
            item.name,
            item.quantity,
            item.price_cents as f64 / 100.0
        );
    }

    println!("Subtotal: ${:.2}", cart.subtotal_cents() as f64 / 100.0);

    if let Some(top) = cart.most_expensive() {
        println!("Priciest line: {}", top.name);
    }
}
```

Verified output:

```text
1. Mechanical Keyboard x1 @ $129.00
2. USB-C Cable x3 @ $12.00
Subtotal: $165.00
Priciest line: Mechanical Keyboard
```

Notice the patterns: `with_capacity` to avoid early reallocations, `retain` for in-place filtering (the equivalent of reassigning `cart = cart.filter(...)` in TypeScript, but without a second allocation), `iter().map(...).sum()` for the total, and `max_by_key` returning an `Option<&CartItem>` so an empty cart is handled safely. Money is stored as integer `u64` cents — never `f64` — to avoid floating-point rounding, exactly as you would in a careful TypeScript backend.

---

## Further Reading

### Official Documentation

- [`std::vec::Vec` API docs](https://doc.rust-lang.org/std/vec/struct.Vec.html): every method, with examples
- [The Rust Book — Storing Lists with Vectors](https://doc.rust-lang.org/book/ch08-01-vectors.html)
- [Rust by Example — Vectors](https://doc.rust-lang.org/rust-by-example/std/vec.html)
- [The `vec!` macro](https://doc.rust-lang.org/std/macro.vec.html)

### Related Topics in This Guide

- [Strings](/07-collections/01-strings/): `String` is, internally, a `Vec<u8>` of UTF-8 bytes
- [HashMaps](/07-collections/03-hashmaps/): when you need key/value lookup instead of a list
- [Iterators](/07-collections/06-iterators/) and [Iterator Consumers](/07-collections/07-iterator-consumers/): the lazy adaptors that replace JS array methods
- [Collection Performance](/07-collections/09-collection-performance/): Big-O, capacity tuning, `Vec` vs other collections
- [Section 05 — Ownership](/05-ownership/): why `let b = a` moves a `Vec`
- [Section 02 — Basics: Types](/02-basics/01-types/): `usize`, `Option`, and integer types
- [Section 08 — Error Handling](/08-error-handling/): handling the `Option` that `pop`/`get` return

---

## Exercises

### Exercise 1: Running Average

**Difficulty:** Beginner

**Objective:** Practice creating a `Vec`, pushing onto it, and computing an aggregate with an iterator.

**Instructions:** Start with an empty `Vec<f64>`. Push the temperatures `20.5`, `22.0`, and `19.5`. Then compute and print their average. Watch the integer-vs-float cast for the length.

```rust
fn main() {
    let mut temps: Vec<f64> = Vec::new();
    // TODO: push the three temperatures

    // TODO: compute the average and print it
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let mut temps: Vec<f64> = Vec::new();
    temps.push(20.5);
    temps.push(22.0);
    temps.push(19.5);

    let sum: f64 = temps.iter().sum();
    let avg = sum / temps.len() as f64; // len() is usize, cast to f64
    println!("avg = {avg}");
}
```

Output:

```text
avg = 20.666666666666668
```

</details>

### Exercise 2: Safe Element Access

**Difficulty:** Intermediate

**Objective:** Replace panicking index access with the `Option`-returning `.get()`.

**Instructions:** Given `let v = vec![10, 20, 30];`, write code that prints the element at index 5 if it exists, or `"no element at index 5"` if it does not, without ever panicking. Then read index 1 with a fallback default of `0`.

```rust
fn main() {
    let v = vec![10, 20, 30];
    // TODO: print element at index 5, or a "not found" message
    // TODO: read index 1, defaulting to 0 if missing
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let v = vec![10, 20, 30];

    match v.get(5) {
        Some(x) => println!("got {x}"),
        None => println!("no element at index 5"),
    }

    // .copied() turns Option<&i32> into Option<i32>; unwrap_or supplies a default.
    let val = v.get(1).copied().unwrap_or(0);
    println!("val = {val}");
}
```

Output:

```text
no element at index 5
val = 20
```

</details>

### Exercise 3: Deduplicate and Transform

**Difficulty:** Advanced

**Objective:** Combine in-place mutation (`sort`, `dedup`) with an iterator transformation that builds a new `Vec`.

**Instructions:** Given `vec![3, 1, 2, 3, 1, 2]`, produce a sorted list of the **unique** values, then build a *second* `Vec` containing each unique value doubled. Print both. (Hint: `dedup` only removes *consecutive* duplicates, so you must sort first.)

```rust
fn main() {
    let mut nums = vec![3, 1, 2, 3, 1, 2];
    // TODO: sort, then remove consecutive duplicates

    // TODO: build a new Vec of each value doubled, then print both
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let mut nums = vec![3, 1, 2, 3, 1, 2];
    nums.sort(); // [1, 1, 2, 2, 3, 3]
    nums.dedup(); // [1, 2, 3] — removes only CONSECUTIVE duplicates

    println!("unique sorted: {nums:?}");

    let doubled: Vec<i32> = nums.iter().map(|n| n * 2).collect();
    println!("doubled: {doubled:?}");
}
```

Output:

```text
unique sorted: [1, 2, 3]
doubled: [2, 4, 6]
```

</details>
