---
title: "Iterators: From Array Methods to Lazy Iterator Adaptors"
description: "Map your JavaScript .map/.filter/.slice habits onto Rust's lazy Iterator adaptors: map, filter, take, skip, zip, enumerate build a pipeline that runs no work"
---

In TypeScript you reach for `.map()`, `.filter()`, `.slice()`, and friends without a second thought. Rust has all of these too, but they live on the **`Iterator`** trait and behave one important way differently: they are **lazy**. This page maps your array-method muscle memory onto Rust's iterator adaptors (`map`, `filter`, `take`, `skip`, `zip`, `enumerate`) and explains the laziness that makes them both fast and, at first, slightly surprising.

---

## Quick Overview

An **iterator adaptor** is a method that takes one iterator and returns a new, transformed iterator. `map`, `filter`, `take`, `skip`, `zip`, and `enumerate` are all adaptors. The big difference from JavaScript's array methods is that adaptors are **lazy**: building the chain does *no work*. Nothing happens until a **consumer** (like `.collect()` or a `for` loop) pulls values through it. This is the opposite of JavaScript, where `arr.map(...)` allocates a brand-new array immediately, even if you throw the result away.

> **Note:** This page covers the adaptors that *transform* an iterator. The methods that *finish* a chain and produce a value (`collect`, `sum`, `fold`, `find`, `any`, `count`, and so on) are **consumers**, covered in [Iterator Consumers](/07-collections/07-iterator-consumers/). Writing your own iterator type is covered in [Custom Iterators](/07-collections/08-custom-iterators/).

---

## TypeScript/JavaScript Example

```typescript
// Processing a price list in a typical e-commerce backend.
const prices = [19.99, 4.5, 120.0, 9.99, 250.0];

// Chained array methods — each call allocates a NEW array right away.
const discounted = prices
  .filter((p) => p > 10.0) // [19.99, 120.0, 250.0]
  .map((p) => p * 0.9); // [17.991, 108, 225]
console.log("discounted:", discounted);

// `.map` runs its callback IMMEDIATELY, even if you ignore the result:
[1, 2, 3].map((n) => {
  console.log("mapping", n);
  return n * 2;
});
console.log("after the orphan map");

// Index-aware iteration uses the second callback parameter.
const names = ["Ada", "Alan", "Grace"];
names.forEach((name, i) => console.log(`${i}: ${name}`));

// "take 5 starting at offset 20" is a slice.
const page = Array.from({ length: 100 }, (_, i) => i + 1).slice(20, 25);
console.log("page:", page);

// JavaScript has no built-in `zip`; you hand-roll it with map + index.
const labels = ["cpu", "mem", "disk"];
const values = [80, 55, 40];
const pairs = labels.map((l, i) => [l, values[i]]);
console.log("pairs:", pairs);
```

Running this with Node v22 prints:

```text
discounted: [ 17.991, 108, 225 ]
mapping 1
mapping 2
mapping 3
after the orphan map
0: Ada
1: Alan
2: Grace
page: [ 21, 22, 23, 24, 25 ]
pairs: [ [ 'cpu', 80 ], [ 'mem', 55 ], [ 'disk', 40 ] ]
```

The key thing to notice: `"mapping 1/2/3"` prints **even though we never use the result of that `.map`**. JavaScript array methods are *eager*: they do the work the instant you call them. Keep that in mind; Rust does the exact opposite.

---

## Rust Equivalent

```rust
fn main() {
    let prices = vec![19.99, 4.50, 120.0, 9.99, 250.0];

    // Build a chain of ADAPTORS. This line allocates nothing and runs
    // no closures — it just describes the pipeline. `.collect()` is the
    // consumer that actually drives it.
    let discounted: Vec<f64> = prices
        .iter() // Iterator<Item = &f64>
        .filter(|&&p| p > 10.0) // keep p > 10.0
        .map(|&p| p * 0.9) // apply the 10% discount
        .collect(); // RUN it, gather into a Vec
    println!("discounted: {discounted:?}");

    // An orphan adaptor that is never consumed does NOTHING — and the
    // compiler warns you about it (see Common Pitfalls).

    // Index-aware iteration uses the `enumerate` adaptor, not a callback arg.
    let names = vec!["Ada", "Alan", "Grace"];
    for (i, name) in names.iter().enumerate() {
        println!("{i}: {name}");
    }

    // "take 5 starting at offset 20": skip then take.
    let page: Vec<i32> = (1..=100).skip(20).take(5).collect();
    println!("page: {page:?}");

    // `zip` is built in — it pairs two iterators element by element.
    let labels = vec!["cpu", "mem", "disk"];
    let values = vec![80, 55, 40];
    let pairs: Vec<(&str, i32)> = labels
        .iter()
        .copied()
        .zip(values.iter().copied())
        .collect();
    println!("pairs: {pairs:?}");
}
```

Verified output:

```text
discounted: [17.991, 108.0, 225.0]
0: Ada
1: Alan
2: Grace
page: [21, 22, 23, 24, 25]
pairs: [("cpu", 80), ("mem", 55), ("disk", 40)]
```

> **Note:** Rust prints `108.0` where Node prints `108`. Both are the same `f64` value; the difference is purely how each language's default formatter renders a whole-number float. Rust's `{:?}` always shows the decimal point so you can see the value is a float.

---

## Detailed Explanation

### Where iterators come from

An **iterator** is any type that implements the `Iterator` trait, which boils down to one method: `next(&mut self) -> Option<Self::Item>`. Each call to `next` hands back `Some(item)` or `None` when the sequence is exhausted. You rarely call `next` by hand; the adaptors and `for` loops do it for you.

You get an iterator from a collection in one of three ways, and the choice decides whether you borrow or consume, exactly like `Vec` iteration in [Vectors](/07-collections/00-vectors/):

| You write     | Method        | Item type | Source afterwards  |
| ------------- | ------------- | --------- | ------------------ |
| `v.iter()`    | `iter()`      | `&T`      | still usable       |
| `v.iter_mut()`| `iter_mut()`  | `&mut T`  | still usable       |
| `v.into_iter()` / `for x in v` | `into_iter()` | `T` | **consumed/moved** |

Ranges like `1..=100` and `0..` are *also* iterators, with no backing collection at all. That is why `(1..=100).skip(20).take(5)` works directly.

### `map` — transform every element

```rust
fn main() {
    let cents = vec![100, 250, 75];
    let dollars: Vec<f64> = cents.iter().map(|&c| c as f64 / 100.0).collect();
    println!("{dollars:?}");
}
```

Verified output:

```text
[1.0, 2.5, 0.75]
```

`map` takes a closure (Rust's arrow-function equivalent — see [Section 03 — Functions](/03-functions/)) and returns a new iterator that yields the transformed values. It is exactly `Array.prototype.map`, except it produces an *iterator*, not an array; you choose the output container with `.collect()`.

### `filter` — keep elements matching a predicate

```rust
fn main() {
    let nums = vec![1, 2, 3, 4, 5, 6];
    let evens: Vec<&i32> = nums.iter().filter(|&&n| n % 2 == 0).collect();
    println!("{evens:?}");
}
```

Verified output:

```text
[2, 4, 6]
```

Note the `|&&n|` pattern. `nums.iter()` yields `&i32`. `filter`'s closure receives its item *by reference* (so it can decide without consuming it), so the closure gets `&&i32`. The double-`&` pattern `&&n` destructures both layers, giving you a plain `i32` named `n`. This double-reference quirk trips up newcomers constantly. See [Common Pitfalls](#common-pitfalls).

### `enumerate` — pair each element with its index

```rust
fn main() {
    let tasks = vec!["build", "test", "deploy"];
    for (i, task) in tasks.iter().enumerate() {
        println!("step {}: {task}", i + 1);
    }
}
```

Verified output:

```text
step 1: build
step 2: test
step 3: deploy
```

In JavaScript, the index is the *second argument* to `map`/`forEach`/`filter` callbacks. In Rust, indexing is a separate adaptor that wraps each item into a `(usize, item)` tuple. This is cleaner: the index is opt-in, always a `usize`, and composes with every other adaptor.

### `take` and `skip` — slicing without allocating

```rust
fn main() {
    let feed = vec![10, 20, 30, 40, 50, 60, 70];

    let first_three: Vec<i32> = feed.iter().copied().take(3).collect();
    let after_two: Vec<i32> = feed.iter().copied().skip(2).collect();
    // Pagination: page 2, 3 items per page -> skip(3).take(3).
    let page_two: Vec<i32> = feed.iter().copied().skip(3).take(3).collect();

    println!("take 3:   {first_three:?}");
    println!("skip 2:   {after_two:?}");
    println!("page two: {page_two:?}");
}
```

Verified output:

```text
take 3:   [10, 20, 30]
skip 2:   [30, 40, 50, 60, 70]
page two: [40, 50, 60]
```

`skip(n).take(m)` is the lazy equivalent of `arr.slice(n, n + m)`, but unlike `slice`, it never allocates an intermediate array and works on *any* iterator, including infinite ranges. (`.copied()` turns the `&i32` items into owned `i32`; see the note under `zip`.)

### `zip` — walk two iterators in lockstep

```rust
fn main() {
    let metrics = vec!["cpu", "mem", "disk"];
    let percentages = vec![80, 55, 40, 99]; // one extra — ignored

    let report: Vec<(&str, i32)> = metrics
        .iter()
        .copied()
        .zip(percentages.iter().copied())
        .collect();
    println!("{report:?}");
}
```

Verified output:

```text
[("cpu", 80), ("mem", 55), ("disk", 40)]
```

`zip` stops at the **shorter** of the two iterators; the extra `99` is dropped silently. JavaScript has no built-in `zip`; you simulate it with `a.map((x, i) => [x, b[i]])`, which breaks if `b` is shorter (you get `undefined`). Rust's `zip` is total and type-safe.

> **Tip:** `.copied()` (and its cousin `.cloned()`) convert an iterator of `&T` into an iterator of `T`. Here `metrics.iter()` yields `&&str`; `.copied()` makes it `&str` so the tuple is `(&str, i32)` rather than `(&&str, &i32)`. Use `copied` for `Copy` types (numbers, `&str`, `char`) and `cloned` for owned types like `String`.

### The heart of it: **laziness**

This is the single most important difference from JavaScript. Building an adaptor chain runs **no** code. The closures fire only when a consumer pulls values through.

```rust
fn main() {
    let nums = vec![1, 2, 3];

    // Building the chain runs nothing.
    let lazy = nums.iter().map(|n| {
        println!("mapping {n}");
        n * 2
    });
    println!("created the iterator, nothing printed yet");

    // .collect() is the consumer — NOW the closure runs.
    let doubled: Vec<i32> = lazy.collect();
    println!("doubled: {doubled:?}");
}
```

Verified output:

```text
created the iterator, nothing printed yet
mapping 1
mapping 2
mapping 3
doubled: [2, 4, 6]
```

Compare this to the JavaScript example earlier, where `"mapping 1/2/3"` printed *before* `"after the orphan map"`, because JS ran the callback immediately. In Rust, the prints come *after* `"created the iterator..."`, because the work waits for `.collect()`.

### Laziness is also efficient: only as much work as needed

Because values are *pulled* one at a time, an adaptor chain only does the work the consumer actually demands. This lets Rust iterate over **infinite** sequences and stop early:

```rust
fn main() {
    let result: Vec<i32> = (1..) // 1, 2, 3, ... infinite!
        .map(|n| {
            println!("squaring {n}");
            n * n
        })
        .filter(|sq| sq % 2 == 1) // keep odd squares
        .take(3) // stop after 3
        .collect();
    println!("first 3 odd squares: {result:?}");
}
```

Verified output:

```text
squaring 1
squaring 2
squaring 3
squaring 4
squaring 5
first 3 odd squares: [1, 9, 25]
```

Notice the interleaving: the pipeline squares `1`, checks it (odd, keep), squares `2` (even, drop), and so on, pulling *one element at a time* until `take(3)` is satisfied at `5`. It never tries to materialize the infinite range. In JavaScript, `Array.from({length: Infinity})` would hang or crash; eager methods *cannot* express this. (Lazy JS iteration exists via generators, but the array methods aren't lazy.)

---

## Key Differences

| Concept                     | TypeScript/JavaScript array methods            | Rust iterator adaptors                                  |
| --------------------------- | ---------------------------------------------- | ------------------------------------------------------- |
| Evaluation                  | **Eager**: runs immediately on the call        | **Lazy**: runs only when consumed                       |
| Intermediate results        | Each step allocates a new array                | No allocation between adaptors; one pass at the end     |
| Index access                | Callback's second parameter `(x, i) => ...`    | Separate `enumerate()` adaptor yielding `(usize, item)` |
| `zip`                       | Not built in; hand-rolled, breaks on mismatch  | Built in; stops at the shorter iterator                 |
| Infinite sequences          | Not expressible with array methods             | Natural: `(0..)`, `std::iter::repeat`, etc.             |
| `slice(n, m)`               | Allocates a new array                          | `skip(n).take(m)` — lazy, no allocation                 |
| Ignoring the result         | Side effects still run                         | Nothing runs; compiler warns about the unused iterator  |
| What you iterate            | Always the values (copies of references)       | `&T`, `&mut T`, or `T` depending on `iter`/`into_iter`  |
| Output type                 | Always an `Array`                              | You choose via `collect::<Vec<_>>()`, `String`, etc.    |

### Why lazy?

Laziness lets the compiler **fuse** the whole chain into a single loop with no intermediate collections. A four-step chain like `iter().filter(...).map(...).take(3)` becomes, after optimization, roughly one `for` loop that breaks after three matches: typically as fast as the hand-written loop you'd write in C, and often faster than the equivalent JavaScript because there are no per-step array allocations. You get the readability of method chaining with the performance of an imperative loop. See [Collection Performance](/07-collections/09-collection-performance/) for the iterator-vs-loop comparison.

---

## Common Pitfalls

### Pitfall 1: Forgetting to consume the iterator

This is the number-one surprise for JavaScript developers. You write a `map` for its side effect, and nothing happens.

```rust
fn main() {
    let nums = vec![1, 2, 3];
    nums.iter().map(|n| println!("{n}")); // adaptor created but never consumed
    println!("done");
}
```

The program prints only `done`; the `println!` inside `map` never runs. And the compiler warns you (trimmed):

```text
warning: unused `Map` that must be used
 --> src/main.rs:3:5
  |
3 |     nums.iter().map(|n| println!("{n}")); // adaptor created but never consumed
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: iterators are lazy and do nothing unless consumed
  = note: `#[warn(unused_must_use)]` on by default
```

**Fix:** if you want a side effect per element, use a `for` loop (the idiomatic choice) or the `for_each` consumer: `nums.iter().for_each(|n| println!("{n}"));`. Use `map` only when you want the *transformed values*.

### Pitfall 2: The double-reference in `filter`/`map` closures

`iter()` yields `&T`, and `filter` passes its item *by reference*, so the closure sees `&&T`. Treating it like a plain number fails to compile.

```rust
fn main() {
    let nums = vec![1, 2, 3, 4];
    let evens: Vec<&i32> = nums.iter().filter(|n| n % 2 == 0).collect(); // does not compile (error[E0369])
    println!("{evens:?}");
}
```

Real compiler error (trimmed):

```text
error[E0369]: cannot calculate the remainder of `&&{integer}` divided by `{integer}`
 --> src/main.rs:3:53
  |
3 |     let evens: Vec<&i32> = nums.iter().filter(|n| n % 2 == 0).collect();
  |                                                   - ^ - {integer}
  |                                                   |
  |                                                   &&{integer}
  |
help: `%` can be used on `&{integer}` if you dereference the left-hand side
  |
3 |     let evens: Vec<&i32> = nums.iter().filter(|n| *n % 2 == 0).collect();
  |                                                   +
```

**Fix:** dereference (`*n % 2`) or, more cleanly, destructure the references in the parameter pattern (`|&&n| n % 2 == 0`). For tuple items from `enumerate`, the same idea applies — `.filter(|(_, v)| **v % 2 == 0)`.

### Pitfall 3: Using a collection after `into_iter()` consumed it

`into_iter()` takes ownership. Once you call it, the original is gone. This is the ownership story from [Section 05 — Ownership](/05-ownership/), surfacing in iterator form.

```rust
fn main() {
    let names = vec![String::from("Ada"), String::from("Alan")];
    let upper: Vec<String> = names.into_iter().map(|n| n.to_uppercase()).collect();
    println!("{upper:?}");
    println!("{}", names.len()); // does not compile (error[E0382])
}
```

Real compiler error (trimmed):

```text
error[E0382]: borrow of moved value: `names`
   --> src/main.rs:5:20
    |
  2 |     let names = vec![String::from("Ada"), String::from("Alan")];
    |         ----- move occurs because `names` has type `Vec<String>`, which does not implement the `Copy` trait
  3 |     let upper: Vec<String> = names.into_iter().map(|n| n.to_uppercase()).collect();
    |                                    ----------- `names` moved due to this method call
...
  5 |     println!("{}", names.len()); // use after into_iter consumed it
    |                    ^^^^^ value borrowed here after move
    |
note: `into_iter` takes ownership of the receiver `self`, which moves `names`
```

**Fix:** use `names.iter()` (borrows, yields `&String`) if you still need `names` afterward. Use `into_iter()` only when you genuinely want to *consume* the collection and take ownership of its elements.

### Pitfall 4: Expecting `zip` to fail or pad on length mismatch

A JavaScript developer used to `b[i]` returning `undefined` past the end may expect `zip` to surface mismatched lengths. It does not; it silently stops at the shorter one.

```rust
fn main() {
    let keys = vec!["a", "b", "c"];
    let vals = vec![1, 2]; // shorter!
    let z: Vec<(&str, i32)> = keys.iter().copied().zip(vals.iter().copied()).collect();
    println!("{z:?}"); // ("c", ...) is silently dropped
}
```

Verified output:

```text
[("a", 1), ("b", 2)]
```

**Fix:** if equal length is a real invariant, assert `keys.len() == vals.len()` before zipping, or look at the `itertools` crate's `zip_eq` (panics on mismatch). Otherwise, the truncating behavior is exactly what you want.

---

## Best Practices

### 1. Reach for a chain, fall back to a `for` loop for side effects

Use adaptor chains to *transform data into a new value*. Use a plain `for` loop when the point is a **side effect** (printing, mutating external state, I/O). A chain that ends in `for_each` purely for side effects is usually less readable than the loop.

```rust
// Transforming -> use a chain ending in a consumer.
fn main() {
    let prices = vec![100, 250, 75];
    let with_tax: Vec<i32> = prices.iter().map(|p| p * 110 / 100).collect();
    println!("{with_tax:?}"); // [110, 275, 82]

    // Side effect -> use a for loop.
    for p in &with_tax {
        println!("charge {p} cents");
    }
}
```

### 2. Let the adaptors carry the index — don't track it manually

Instead of a mutable counter, use `enumerate`. It is clearer and the index type (`usize`) is always correct.

```rust
// idiomatic
fn main() {
    let items = vec!["x", "y", "z"];
    for (i, item) in items.iter().enumerate() {
        println!("{i}: {item}");
    }
}
```

### 3. Prefer `skip(n).take(m)` over manual index math for paging

It reads as "skip a page, take a page," handles short inputs gracefully (no panic), and allocates nothing until you `collect`.

### 4. Annotate `collect`'s target type

`collect` is generic over its output, so the compiler needs to know what you want, either via a `let` annotation or the turbofish `::<>`:

```rust
fn main() {
    let evens: Vec<i32> = (1..=10).filter(|n| n % 2 == 0).collect(); // annotate the binding
    let odds = (1..=10).filter(|n| n % 2 == 1).collect::<Vec<i32>>(); // or turbofish
    println!("{evens:?} {odds:?}");
}
```

### 5. Use `.copied()` / `.cloned()` to drop a layer of reference

When chaining off `iter()` produces awkward `&&T` items or you want owned values out the other end, insert `.copied()` (for `Copy` types) or `.cloned()` (for owned types) early in the chain.

---

## Real-World Example

A log-processing pipeline: number raw log lines, parse them into structs, keep only warnings and errors, and cap the output at the first three problems, all in one lazy chain. This is the kind of code you'd write in a command-line tool or a server's log scanner.

```rust
#[derive(Debug)]
struct LogLine {
    line_no: usize,
    level: String,
    message: String,
}

fn main() {
    // Raw log text, as you might read from a file.
    let raw = "\
INFO  server started
DEBUG cache warmed
WARN  high memory usage
ERROR db connection lost
INFO  request handled
ERROR timeout on /api/users
DEBUG gc pause 12ms";

    // A single lazy pipeline:
    //   number the lines (enumerate) -> parse (map) ->
    //   keep WARN/ERROR (filter) -> cap at 3 (take) -> gather (collect).
    let problems: Vec<LogLine> = raw
        .lines() // an iterator over &str lines
        .enumerate() // (0-based index, &str)
        .map(|(i, line)| {
            let (level, message) = line.split_once(' ').unwrap_or((line, ""));
            LogLine {
                line_no: i + 1, // humans count from 1
                level: level.trim().to_string(),
                message: message.trim().to_string(),
            }
        })
        .filter(|entry| entry.level == "WARN" || entry.level == "ERROR")
        .take(3)
        .collect();

    for p in &problems {
        println!("line {:>2} [{}] {}", p.line_no, p.level, p.message);
    }

    // Total a severity score across the problems: map each one to a
    // number, then let the `sum` consumer drive the chain.
    let total: i32 = problems
        .iter()
        .map(|p| if p.level == "ERROR" { 10 } else { 3 })
        .sum();
    println!("total severity score: {total}");
}
```

Verified output:

```text
line  3 [WARN] high memory usage
line  4 [ERROR] db connection lost
line  6 [ERROR] timeout on /api/users
total severity score: 23
```

The pipeline never builds an intermediate `Vec` of all parsed lines; laziness fuses `enumerate -> map -> filter -> take` so the parser closure runs at most until three problems are found. `str::lines()` and `str::split_once()` are themselves iterator-friendly (covered in [String Manipulation](/07-collections/02-string-manipulation/)), and `.sum()` is a consumer from [Iterator Consumers](/07-collections/07-iterator-consumers/). The `unwrap_or` handles a malformed line without panicking, a taste of the error-handling discipline in [Section 08 — Error Handling](/08-error-handling/).

---

## Further Reading

### Official Documentation

- [`std::iter::Iterator` API docs](https://doc.rust-lang.org/std/iter/trait.Iterator.html) — every adaptor and consumer, with examples
- [The Rust Book — Processing a Series of Items with Iterators](https://doc.rust-lang.org/book/ch13-02-iterators.html)
- [Rust by Example — Iterators](https://doc.rust-lang.org/rust-by-example/trait/iter.html)
- [`std::iter` module docs](https://doc.rust-lang.org/std/iter/index.html) — laziness, `IntoIterator`, and free functions like `repeat`/`once`

### Related Topics in This Guide

- [Iterator Consumers](/07-collections/07-iterator-consumers/) — `collect`, `fold`, `sum`, `find`, `any`/`all`, `min`/`max`: how chains *end*
- [Custom Iterators](/07-collections/08-custom-iterators/) — implementing the `Iterator` trait on your own types
- [Vectors](/07-collections/00-vectors/) — `iter` / `iter_mut` / `into_iter` and the three borrow flavors
- [String Manipulation](/07-collections/02-string-manipulation/) — `chars()`, `lines()`, `split()` return iterators too
- [Collection Performance](/07-collections/09-collection-performance/) — iterator chains vs hand-written loops; when laziness pays off
- [Section 03 — Functions](/03-functions/) — closures, the arrow-function analog passed to `map`/`filter`
- [Section 05 — Ownership](/05-ownership/) — why `into_iter()` moves the collection
- [Section 02 — Basics: Types](/02-basics/01-types/) — `usize`, `Option`, and integer types you'll meet here

---

## Exercises

### Exercise 1: Celsius to Fahrenheit

**Difficulty:** Beginner

**Objective:** Practice a basic `map` chain ending in `collect`.

**Instructions:** Given `let temps_c = vec![0.0, 25.0, 37.0, 100.0];`, build a new `Vec<f64>` of the same temperatures converted to Fahrenheit using the formula `f = c * 9/5 + 32`. Print the result.

```rust
fn main() {
    let temps_c = vec![0.0, 25.0, 37.0, 100.0];
    // TODO: map each Celsius value to Fahrenheit and collect into a Vec<f64>

    // TODO: print it
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let temps_c = vec![0.0, 25.0, 37.0, 100.0];
    let temps_f: Vec<f64> = temps_c.iter().map(|c| c * 9.0 / 5.0 + 32.0).collect();
    println!("{temps_f:?}");
}
```

Output:

```text
[32.0, 77.0, 98.6, 212.0]
```

</details>

### Exercise 2: Paginated Passing Scores

**Difficulty:** Intermediate

**Objective:** Compose `enumerate`, `filter`, `skip`, and `take` in one lazy chain.

**Instructions:** Given `let scores = vec![88, 42, 95, 60, 73, 31];`, produce a `Vec<(usize, i32)>` of `(original_index, score)` pairs where the score is a passing grade (`>= 60`), then skip the first passing result and take the next two. (Hint: `enumerate` *before* `filter` so the index reflects the original position.)

```rust
fn main() {
    let scores = vec![88, 42, 95, 60, 73, 31];
    // TODO: enumerate -> filter passing -> skip(1) -> take(2) -> collect
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let scores = vec![88, 42, 95, 60, 73, 31];
    let top_passing: Vec<(usize, i32)> = scores
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, s)| *s >= 60)
        .skip(1)
        .take(2)
        .collect();
    println!("{top_passing:?}");
}
```

Output:

```text
[(2, 95), (3, 60)]
```

The passing scores are at indices 0, 2, 3, 4. `skip(1)` drops index 0; `take(2)` keeps indices 2 and 3.

</details>

### Exercise 3: Lazy Report Builder with `zip`

**Difficulty:** Advanced

**Objective:** Combine `zip`, `enumerate`, and `map` to merge two parallel iterators into formatted lines — and observe that `zip` truncates safely.

**Instructions:** Given `let labels = vec!["jan", "feb", "mar"];` and `let revenue = vec![100, 150, 90];`, build a `Vec<String>` where each entry looks like `"1. jan: $100"` (a 1-based row number, the label, and the revenue). Use `zip` to pair labels with revenue and `enumerate` for the row number. Print each line.

```rust
fn main() {
    let labels = vec!["jan", "feb", "mar"];
    let revenue = vec![100, 150, 90];
    // TODO: zip labels with revenue, enumerate, map to "N. label: $amount" strings
    // TODO: print each line
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let labels = vec!["jan", "feb", "mar"];
    let revenue = vec![100, 150, 90];
    let report: Vec<String> = labels
        .iter()
        .zip(revenue.iter())
        .enumerate()
        .map(|(i, (label, amount))| format!("{}. {label}: ${amount}", i + 1))
        .collect();
    for line in &report {
        println!("{line}");
    }
}
```

Output:

```text
1. jan: $100
2. feb: $150
3. mar: $90
```

Note the destructuring `(i, (label, amount))`: `enumerate` wraps each `zip`-produced `(&&str, &i32)` tuple into `(usize, (&&str, &i32))`, and the pattern peels both layers apart. If `revenue` had only two entries, `zip` would simply produce two lines — no panic, no `undefined`.

</details>
