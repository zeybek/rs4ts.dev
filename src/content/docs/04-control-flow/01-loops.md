---
title: "Loops"
description: "Rust drops the C-style for entirely: you iterate ranges and collections, not counters. for, while, and loop cover it, and loop can return a value via break value."
---

Rust gives you three loop keywords (`for`, `while`, and `loop`), but the way you actually iterate is different from JavaScript. You loop over **ranges** and **iterators** instead of incrementing a counter by hand, and `loop` is a real expression that can hand a value back.

---

## Quick Overview

In TypeScript/JavaScript you reach for `for (let i = 0; i < n; i++)` constantly. Rust has **no C-style `for`** at all. Instead you iterate over a **range** (`0..n`) or directly over the elements of a collection, which eliminates an entire class of off-by-one and out-of-bounds bugs. Rust also has `while` (same as you know it) and a dedicated infinite `loop` keyword that — uniquely — can produce a value with `break value`.

---

## TypeScript/JavaScript Example

```typescript
// The four loop shapes a TS/JS dev reaches for every day.
const scores = [88, 92, 75];

// 1. Classic C-style index loop
for (let i = 0; i < scores.length; i++) {
  console.log(`index ${i}: ${scores[i]}`);
}

// 2. for...of over the values
for (const score of scores) {
  console.log(`score ${score}`);
}

// 3. while with a manual counter
let countdown = 3;
while (countdown > 0) {
  console.log(`T-minus ${countdown}`);
  countdown--;
}

// 4. "Infinite" loop with break (often hides a return value in a mutable var)
let n = 1;
let firstBig: number;
while (true) {
  n *= 2;
  if (n > 100) {
    firstBig = n;
    break;
  }
}
console.log(`first power of two over 100: ${firstBig}`);
```

**Key characteristic:** the index loop is the default, and "loop until I find something" usually means mutating an outer variable and `break`-ing.

---

## Rust Equivalent

```rust playground
fn main() {
    let scores = [88, 92, 75];

    // 1. There is NO C-style for. To iterate by index, loop over a range:
    for i in 0..scores.len() {
        println!("index {i}: {}", scores[i]);
    }

    // 2. for over the values (the idiomatic default)
    for score in &scores {
        println!("score {score}");
    }

    // 3. while with a manual counter — identical idea to JavaScript
    let mut countdown = 3;
    while countdown > 0 {
        println!("T-minus {countdown}");
        countdown -= 1;
    }

    // 4. `loop` is an expression: `break value` hands a value back out.
    let mut n = 1;
    let first_big = loop {
        n *= 2;
        if n > 100 {
            break n; // <-- the loop *evaluates* to this value
        }
    };
    println!("first power of two over 100: {first_big}");
}
```

```text
index 0: 88
index 1: 92
index 2: 75
score 88
score 92
score 75
T-minus 3
T-minus 2
T-minus 1
first power of two over 100: 128
```

**Key characteristic:** iterate over ranges/collections, not hand-rolled counters; and `loop` can be the right-hand side of a `let`.

> **Note:** `0..scores.len()` is shown here only to make the comparison concrete. In real Rust you almost never index like this; prefer `for score in &scores`. See [Common Pitfalls](#common-pitfalls).

---

## Detailed Explanation

### `for` iterates over an iterator, never a counter

In Rust, `for pattern in expression { ... }` requires `expression` to be something that can turn into an **iterator** (anything implementing `IntoIterator`). Each pass binds the next item to `pattern`. There is no init/condition/increment triple, because there is no counter; the iterator decides when it is exhausted.

The most common iterables:

```rust playground
fn main() {
    // A range: 1, 2, 3 (the end is EXCLUSIVE)
    for i in 1..4 {
        println!("range {i}");
    }

    // An inclusive range with ..=  : 1, 2, 3
    for i in 1..=3 {
        println!("inclusive {i}");
    }

    // An array/slice/Vec, borrowed with &
    let names = ["Alice", "Bob", "Carol"];
    for name in &names {
        println!("name {name}");
    }
}
```

```text
range 1
range 2
range 3
inclusive 1
inclusive 2
inclusive 3
name Alice
name Bob
name Carol
```

A **range** like `1..4` is the closest thing to `for (let i = 1; i < 4; i++)`. Note the boundary: `start..end` **excludes** `end` (like `Array.prototype.slice`), while `start..=end` **includes** it. JavaScript has no range literal at all; the nearest equivalents are `Array.from({length: n}, (_, i) => i)` or a manual counter.

### `.enumerate()` when you genuinely need the index

If you want both the index and the value — the legitimate reason a TS/JS dev uses the C-style loop — call `.enumerate()` on the iterator. It yields `(index, value)` tuples, which you destructure right in the `for` pattern:

```rust playground
fn main() {
    let names = ["Alice", "Bob", "Carol"];
    for (index, name) in names.iter().enumerate() {
        println!("{index}: {name}");
    }
}
```

```text
0: Alice
1: Bob
2: Carol
```

This is the Rust equivalent of JavaScript's `array.forEach((value, index) => ...)` or `for (const [index, value] of array.entries())`.

### Iterator adapters give you ranges JavaScript can't express in a header

Because the thing after `in` is just an iterator, you compose adapters instead of editing a loop header:

```rust playground
fn main() {
    // Count down: rev() reverses any iterator that supports it
    for i in (0..3).rev() {
        println!("rev {i}");
    }

    // Step by 3: 0, 3, 6, 9 — like `for (i = 0; i < 10; i += 3)`
    for i in (0..10).step_by(3) {
        println!("step {i}");
    }
}
```

```text
rev 2
rev 1
rev 0
step 0
step 3
step 6
step 9
```

> **Tip:** `rev()` and `step_by()` replace the `i--` and `i += 3` you would write in a C-style loop header. The whole iterator toolbox (`filter`, `map`, `take`, `zip`, …) is covered in [Section 07 — Collections](/07-collections/).

### `while` is exactly what you expect

`while condition { ... }` runs the body while the condition is `true`. The only catch (covered in [conditionals](/04-control-flow/00-conditionals/)) is that the condition must be a real `bool`. There is no truthiness, so `while queue.length` does not compile; you write `while !queue.is_empty()`.

```rust playground
fn main() {
    let mut count = 3;
    while count > 0 {
        println!("while {count}");
        count -= 1;
    }
}
```

```text
while 3
while 2
while 1
```

### `loop` is an infinite loop — and an expression

`loop { ... }` repeats forever until you `break`. That alone is just `while (true)`. What is genuinely new for a JavaScript developer is that `loop` is an **expression**: `break value` makes the entire `loop` evaluate to `value`, so you can assign it to a binding.

```rust playground
fn main() {
    let mut n = 1;
    let result = loop {
        n *= 2;
        if n > 100 {
            break n; // the loop produces 128
        }
    };
    println!("result {result}"); // result 128
}
```

```text
result 128
```

In JavaScript you simulate this by declaring a `let result;` outside the loop and assigning to it before `break`. In Rust the value flows *out of* the loop, so there is no uninitialized outer variable to forget about. (`while` and `for` cannot do this; they always evaluate to `()`, because they may run zero times, leaving no value to produce.)

> **Note:** This is the same expression-orientation you saw with [`if`](/04-control-flow/00-conditionals/): in Rust, control-flow constructs are values in their own right, going beyond plain statements. Returning a value from `loop` via `break` and from named blocks is detailed in [break & continue](/04-control-flow/04-break-continue/).

---

## Key Differences from TypeScript

| Aspect | TypeScript/JavaScript | Rust |
| ------ | --------------------- | ---- |
| C-style `for (init; cond; step)` | Yes, the default | **Does not exist** |
| Iterate values | `for...of` | `for x in &collection` |
| Iterate with index | `for (let i...)` or `.entries()` | `for (i, x) in coll.iter().enumerate()` |
| Numeric range | `Array.from(...)` / manual | `start..end` (exclusive), `start..=end` (inclusive) |
| Condition type | any value (truthy/falsy) | must be `bool` |
| Infinite loop | `while (true)` | `loop { ... }` |
| Loop produces a value | No (mutate an outer `let`) | `loop` can: `let x = loop { break v; };` |
| Mutate collection while iterating | Allowed (often buggy) | Rejected by the borrow checker at compile time |
| `for...in` over object keys | Yes | No equivalent; iterate a `HashMap` instead |

### Why no C-style `for`?

The C-style header is three independent pieces — initialization, a re-checked condition, and a mutation — that the programmer must keep in sync. Almost every classic loop bug (off-by-one, `<` vs `<=`, forgetting to increment, indexing past the end) lives in that header. Rust deletes the whole category by making you say *what you are iterating over* (a range or a collection) rather than *how to advance a counter*. The compiler then guarantees the index can never go out of bounds.

### Why does `loop` exist when `while true` would do?

`loop` signals intent ("this runs until an explicit `break`") and, importantly, the compiler treats it specially for **type inference and reachability**. Because `loop` has no condition, code after it is reachable only via `break`, so a `loop` with no `break` has type `!` ("never"), and a `loop { break v; }` can produce a value. `while true` does not get this treatment; it always has type `()`.

---

## Common Pitfalls

### Pitfall 1: Writing a C-style `for`

A TS/JS dev's fingers will type this automatically:

```rust
fn main() {
    for (let mut i = 0; i < 5; i += 1) { // does not compile
        println!("{i}");
    }
}
```

The real compiler error is a parse failure. Rust tries to read `(let mut i = 0; ...)` as a **pattern** to bind, not a loop header:

```text
error: expected pattern, found `let`
 --> src/main.rs:2:10
  |
2 |     for (let mut i = 0; i < 5; i += 1) {
  |          ^^^
  |
help: remove the unnecessary `let` keyword
```

**Fix:** loop over a range.

```rust playground
fn main() {
    for i in 0..5 {
        println!("{i}");
    }
}
```

### Pitfall 2: Reaching for `0..len` and indexing

This compiles, but it is not idiomatic and reintroduces bounds checks and off-by-one risk:

```rust playground
fn main() {
    let scores = [88, 92, 75];
    // Works, but un-idiomatic:
    for i in 0..scores.len() {
        println!("idx {}: {}", i, scores[i]);
    }
}
```

**Fix:** iterate the elements directly; use `.enumerate()` only if you actually need the index.

```rust playground
fn main() {
    let scores = [88, 92, 75];
    for score in &scores {
        println!("score {score}");
    }
}
```

### Pitfall 3: Off-by-one with `..` vs `..=`

`1..4` yields `1, 2, 3` — **not** `4`. Coming from `for (i = 1; i <= 4; i++)`, you will reach for the wrong one.

```rust playground
fn main() {
    let exclusive: Vec<i32> = (1..4).collect();  // [1, 2, 3]
    let inclusive: Vec<i32> = (1..=4).collect(); // [1, 2, 3, 4]
    println!("{exclusive:?} vs {inclusive:?}");
}
```

```text
[1, 2, 3] vs [1, 2, 3, 4]
```

> **Tip:** Read `..` as "up to" and `..=` as "up to and including". The exclusive form matches `array.length` indexing perfectly: `0..arr.len()` covers exactly the valid indices.

### Pitfall 4: Mutating a collection while iterating over it

In JavaScript, pushing to an array while looping over it is legal (and a common source of subtle bugs). Rust rejects it at compile time, because the `for` loop holds an **immutable borrow** of the collection while a `push` needs a **mutable** one:

```rust
fn main() {
    let mut numbers = vec![1, 2, 3];
    for n in &numbers {
        if *n == 2 {
            numbers.push(99); // does not compile (error[E0502])
        }
    }
}
```

```text
error[E0502]: cannot borrow `numbers` as mutable because it is also borrowed as immutable
 --> src/main.rs:5:13
  |
3 |     for n in &numbers {
  |              --------
  |              |
  |              immutable borrow occurs here
  |              immutable borrow later used here
4 |         if *n == 2 {
5 |             numbers.push(99); // try to mutate while borrowed
  |             ^^^^^^^^^^^^^^^^ mutable borrow occurs here
```

**Fix:** collect the changes first, or build a new collection, then apply them after the loop ends. The rules behind this error are the subject of [Section 05 — Ownership](/05-ownership/).

```rust playground
fn main() {
    let mut numbers = vec![1, 2, 3];
    let mut to_add = Vec::new();
    for n in &numbers {
        if *n == 2 {
            to_add.push(99);
        }
    }
    numbers.extend(to_add); // mutate after the borrow ends
    println!("{numbers:?}"); // [1, 2, 3, 99]
}
```

### Pitfall 5: Using a value moved by `for`

`for x in collection` (without `&`) **consumes** the collection: each element is moved into `x`. Afterward the original is gone:

```rust playground
fn main() {
    let owned = vec![String::from("a"), String::from("b")];
    for s in owned { // moves each String out of `owned`
        println!("{s}");
    }
    // println!("{}", owned.len()); // would not compile: `owned` was moved
}
```

**Fix:** borrow with `&` if you still need the collection after the loop:

```rust playground
fn main() {
    let owned = vec![String::from("a"), String::from("b")];
    for s in &owned { // borrows; `owned` survives
        println!("{s}");
    }
    println!("still have {} items", owned.len()); //
}
```

---

## Best Practices

### 1. Default to iterating values, not indices

```rust
// index-heavy, easy to get wrong
for i in 0..items.len() {
    process(&items[i]);
}

// clear and bounds-safe
for item in &items {
    process(item);
}
```

### 2. Use `.enumerate()` instead of a side counter

```rust
// manual counter alongside the loop
let mut i = 0;
for line in &lines {
    println!("{i}: {line}");
    i += 1;
}

// index comes from the iterator
for (i, line) in lines.iter().enumerate() {
    println!("{i}: {line}");
}
```

### 3. Use `loop { break value }` instead of a sentinel variable

```rust
// JavaScript-style: outer mutable var + while true
let mut found = -1;
let mut i = 0;
while i < 1000 {
    if is_target(i) { found = i; break; }
    i += 1;
}

// the loop produces the value directly
let found = loop {
    let candidate = next_candidate();
    if is_target(candidate) {
        break candidate;
    }
};
```

### 4. Prefer iterator methods when you are computing a single result

A `for` loop that just accumulates is often clearer as an iterator chain, and the compiler optimizes it just as well:

```rust playground
fn main() {
    let scores = [88, 92, 75];

    // manual accumulation
    let mut total = 0;
    for s in &scores {
        total += s;
    }

    // expresses intent directly
    let total: i32 = scores.iter().sum();

    println!("{total}"); // 255
}
```

> **Note:** Reach for a `for` loop when the body has side effects or early exits; reach for iterator adapters (`map`/`filter`/`sum`/`collect`) when you are transforming data into a value. Both are idiomatic.

---

## Real-World Example

A small slice of a job-runner: poll a backend until a job finishes (using `loop` + `break value`), render a text progress bar with a `for` range, then drain a work queue with `while`.

```rust playground
#[derive(Debug)]
struct Job {
    id: u32,
    status: &'static str,
}

// Pretend this calls an API. The job "completes" on the 3rd poll.
fn poll_job(attempt: u32) -> Job {
    let status = if attempt >= 3 { "done" } else { "running" };
    Job { id: 42, status }
}

fn main() {
    // `loop` + `break value`: poll until done, hand the finished job back out.
    let mut attempt = 0;
    let finished = loop {
        attempt += 1;
        let job = poll_job(attempt);
        println!("attempt {attempt}: job {} is {}", job.id, job.status);

        if job.status == "done" {
            break job; // the loop evaluates to this Job
        }
        if attempt >= 5 {
            break job; // give up after 5 tries
        }
    };
    println!("final: {finished:?}");

    // `for` over a range to render a 10-segment progress bar.
    let percent = 60;
    let filled = percent / 10;
    let mut bar = String::new();
    for i in 0..10 {
        bar.push(if i < filled { '#' } else { '-' });
    }
    println!("[{bar}] {percent}%");

    // `while` to drain a stack of remaining tasks.
    let mut queue = vec!["build", "test", "deploy"];
    while !queue.is_empty() {
        let task = queue.pop().unwrap();
        println!("running {task}");
    }
}
```

```text
attempt 1: job 42 is running
attempt 2: job 42 is running
attempt 3: job 42 is done
final: Job { id: 42, status: "done" }
[######----] 60%
running deploy
running test
running build
```

> **Note:** Draining the queue with `while let Some(task) = queue.pop()` is even more idiomatic than `while !queue.is_empty()` + `unwrap()`. That `while let` pattern is covered in [if let / while let](/04-control-flow/03-if-let-while-let/).

---

## Further Reading

### Official Documentation

- [The Rust Book — Control Flow: Repetition with Loops](https://doc.rust-lang.org/book/ch03-05-control-flow.html#repetition-with-loops)
- [The Rust Book — Processing a Series of Items with Iterators](https://doc.rust-lang.org/book/ch13-02-iterators.html)
- [Rust by Example — Loops](https://doc.rust-lang.org/rust-by-example/flow_control.html)
- [`std::ops::Range` documentation](https://doc.rust-lang.org/std/ops/struct.Range.html)
- [`Iterator` trait (adapters like `enumerate`, `rev`, `step_by`)](https://doc.rust-lang.org/std/iter/trait.Iterator.html)

### Related Sections in This Guide

- [Conditionals](/04-control-flow/00-conditionals/): `if` as an expression; why loop/`while` conditions must be `bool`
- [match](/04-control-flow/02-match/): the other major control-flow expression
- [if let / while let](/04-control-flow/03-if-let-while-let/): concise pattern-driven looping (e.g. `while let Some(x) = ...`)
- [break & continue](/04-control-flow/04-break-continue/): early exit, skipping iterations, and returning values from a `loop`
- [Labeled loops](/04-control-flow/05-labeled-loops/): `break`/`continue` targeting an outer loop by label
- [Section 02 — Basics](/02-basics/): variables, types, and operators used above
- [Section 03 — Functions](/03-functions/): functions are also expression-based
- [Section 05 — Ownership](/05-ownership/): the borrowing rules behind the "mutate while iterating" error
- [Section 07 — Collections](/07-collections/): the full iterator toolbox

---

## Exercises

### Exercise 1: FizzBuzz with a range

**Difficulty:** Easy

**Objective:** Practice `for` over an inclusive range and combine it with `if`/`else if`.

**Instructions:** Print the numbers `1` through `15`, but print `Fizz` for multiples of 3, `Buzz` for multiples of 5, and `FizzBuzz` for multiples of both.

```rust playground
fn main() {
    for n in 1..=15 {
        // TODO: print Fizz / Buzz / FizzBuzz / the number
    }
}
```

<details>
<summary>Solution</summary>

```rust playground
fn main() {
    for n in 1..=15 {
        let line = if n % 15 == 0 {
            "FizzBuzz".to_string()
        } else if n % 3 == 0 {
            "Fizz".to_string()
        } else if n % 5 == 0 {
            "Buzz".to_string()
        } else {
            n.to_string()
        };
        println!("{line}");
    }
}
```

```text
1
2
Fizz
4
Buzz
Fizz
7
8
Fizz
Buzz
11
Fizz
13
14
FizzBuzz
```

</details>

### Exercise 2: Return a value from `loop`

**Difficulty:** Medium

**Objective:** Use `loop` + `break value` to produce a result, instead of mutating an outer variable.

**Instructions:** Write `next_power_of_two_above(target: u32) -> u32` that returns the smallest power of two strictly greater than `target`. Use a `loop` that `break`s with the answer. (Start at `1` and keep doubling.)

```rust
fn next_power_of_two_above(target: u32) -> u32 {
    // TODO: use `loop { ... break p; ... }`
}

fn main() {
    println!("{}", next_power_of_two_above(100)); // 128
    println!("{}", next_power_of_two_above(5));   // 8
}
```

<details>
<summary>Solution</summary>

```rust playground
fn next_power_of_two_above(target: u32) -> u32 {
    let mut p = 1u32;
    loop {
        if p > target {
            break p; // the loop evaluates to p, which the fn returns
        }
        p *= 2;
    }
}

fn main() {
    println!("{}", next_power_of_two_above(100)); // 128
    println!("{}", next_power_of_two_above(5));   // 8
}
```

```text
128
8
```

> The `loop` expression is the function's tail expression, so its `break` value becomes the return value, no `return` keyword needed.

</details>

### Exercise 3: Collatz step counter with `while`

**Difficulty:** Medium

**Objective:** Drive a `while` loop with a changing condition and mutate a counter.

**Instructions:** Write `collatz_steps(n: u64) -> u32` that counts how many steps it takes to reach `1` under the Collatz rule: if `n` is even, halve it; if odd, compute `3 * n + 1`. Count each transformation.

```rust
fn collatz_steps(mut n: u64) -> u32 {
    // TODO: loop with `while n != 1 { ... }`
}

fn main() {
    println!("{}", collatz_steps(27)); // 111
    println!("{}", collatz_steps(6));  // 8
}
```

<details>
<summary>Solution</summary>

```rust playground
fn collatz_steps(mut n: u64) -> u32 {
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
    }
    steps
}

fn main() {
    println!("{}", collatz_steps(27)); // 111
    println!("{}", collatz_steps(6));  // 8
}
```

```text
111
8
```

> Note `mut n` in the parameter list: parameters are immutable by default in Rust, so you opt into mutating the local copy. The `if`/`else` here is used as an **expression** producing the next value of `n`; see [conditionals](/04-control-flow/00-conditionals/).

</details>

---

## Summary

**What you've learned:**

- Rust has **no C-style `for`**: you iterate over ranges and collections
- `for x in start..end` (exclusive) and `start..=end` (inclusive)
- `.enumerate()` gives you `(index, value)` when you need the index
- `.rev()` and `.step_by(n)` replace counting down / stepping
- `while condition` works like JavaScript, but the condition must be `bool`
- `loop { ... }` is the dedicated infinite loop, and `break value` makes it an expression
- Borrowing (`&`) vs moving in `for`, and why you cannot mutate a collection mid-loop

**The big mental shift:** stop thinking "advance a counter" and start thinking "iterate over a sequence." It eliminates off-by-one and out-of-bounds bugs by construction.
