---
title: "Break and Continue"
description: "break and continue work like JavaScript inside a loop, but Rust's loop is an expression: break value hands a result out, replacing the mutate-a-variable pattern."
---

`break` and `continue` are the two keywords you reach for when you want to *interrupt* the normal flow of a loop. If you've written `for`/`while` loops in JavaScript, you already know the basic idea. Rust keeps the familiar parts and adds one genuinely new superpower: a loop can **`break` with a value**, turning the whole loop into an expression that produces a result.

---

## Quick Overview

In both TypeScript/JavaScript and Rust, `break` exits the nearest enclosing loop and `continue` skips to the next iteration. The big difference is that Rust's `loop` is an **expression**, so `break value;` can hand a value back out of the loop, something JavaScript simply cannot do. This single feature replaces a lot of the "declare a `let` outside the loop, mutate it inside" boilerplate you write in JavaScript.

> **Note:** This page focuses narrowly on `break`/`continue` themselves and on `break`-with-a-value. The loop constructs they live in (`for`, `while`, `loop`) are covered in [Loops](/04-control-flow/01-loops/), and breaking/continuing across *nested* loops with labels is covered in [Labeled Loops](/04-control-flow/05-labeled-loops/).

---

## TypeScript/JavaScript Example

```typescript
// Scan log entries: ignore "debug" noise, stop at the first "fatal".
interface LogEntry {
  level: "debug" | "info" | "warn" | "error" | "fatal";
  message: string;
}

function collectUntilFatal(entries: LogEntry[]): string[] {
  const collected: string[] = [];

  for (const entry of entries) {
    if (entry.level === "debug") continue; // skip noisy debug lines
    if (entry.level === "fatal") break; // stop scanning entirely
    collected.push(entry.message);
  }

  return collected;
}

const entries: LogEntry[] = [
  { level: "debug", message: "connecting" },
  { level: "warn", message: "slow response" },
  { level: "error", message: "timeout" },
  { level: "fatal", message: "out of memory" },
  { level: "info", message: "never reached" },
];

console.log(collectUntilFatal(entries));
// [ 'slow response', 'timeout' ]
```

**Key points:**

- `continue` jumps straight to the next iteration, skipping the rest of the loop body.
- `break` leaves the loop entirely; everything after the `fatal` entry is never visited.
- `break`/`continue` are **statements** in JavaScript — they don't produce values.

---

## Rust Equivalent

```rust
#[derive(Debug)]
struct LogEntry {
    level: String,
    message: String,
}

/// Scan log entries, skip "debug" lines, and stop at the first "fatal".
fn collect_until_fatal(entries: &[LogEntry]) -> Vec<String> {
    let mut collected = Vec::new();

    for entry in entries {
        match entry.level.as_str() {
            "debug" => continue, // ignore noisy debug lines
            "fatal" => break,    // stop scanning entirely
            _ => collected.push(entry.message.clone()),
        }
    }

    collected
}

fn main() {
    let entries = vec![
        LogEntry { level: "debug".into(), message: "connecting".into() },
        LogEntry { level: "warn".into(),  message: "slow response".into() },
        LogEntry { level: "error".into(), message: "timeout".into() },
        LogEntry { level: "fatal".into(), message: "out of memory".into() },
        LogEntry { level: "info".into(),  message: "never reached".into() },
    ];

    let important = collect_until_fatal(&entries);
    println!("{:#?}", important);
}
```

**Output:**

```text
[
    "slow response",
    "timeout",
]
```

**Key points:**

- `continue` and `break` work exactly like JavaScript inside a `for` loop.
- Here they appear as arms of a `match`, which reads cleanly, but a plain `if`/`else if` works identically.
- `break`/`continue` with **no value** is the form allowed in `for` and `while` loops (more on values below).

---

## Detailed Explanation

### `continue`: skip the rest of this iteration

`continue` immediately abandons the current iteration and moves to the next one. Anything in the loop body *after* the `continue` is skipped for that pass only.

```rust
fn main() {
    let mut i = 0;
    let mut kept = String::new();
    while i < 10 {
        i += 1;
        if i % 3 == 0 {
            continue; // skip multiples of 3
        }
        kept.push_str(&i.to_string());
        kept.push(' ');
    }
    println!("non-multiples of 3: {}", kept.trim());
}
```

**Output:**

```text
non-multiples of 3: 1 2 4 5 7 8 10
```

> **Warning:** In a `while` loop, `continue` does **not** re-run any "update" step for you; there is no `i++` clause like in a C-style `for`. Notice that `i += 1` is the *first* line of the body above, on purpose. If you put `i += 1` at the *end* and then `continue` before reaching it, the counter never advances and you get an infinite loop. This is a very common JavaScript-to-Rust slip, because JS `for (let i = 0; ...; i++)` runs `i++` even after a `continue`. Rust has no C-style `for` (see [Loops](/04-control-flow/01-loops/)), so you manage the counter yourself.

### `break`: leave the loop entirely

`break` exits the nearest enclosing loop. Execution resumes at the first statement *after* the loop.

```rust
fn main() {
    let mut sum = 0;
    for n in 1..=10 {
        if n % 2 == 0 {
            continue; // skip even numbers
        }
        if n > 7 {
            break; // stop once we pass 7
        }
        sum += n;
    }
    println!("sum of odd numbers <= 7: {}", sum); // 1 + 3 + 5 + 7 = 16
}
```

**Output:**

```text
sum of odd numbers <= 7: 16
```

### `break` with a value — Rust's superpower

This is the part with no JavaScript equivalent. Because `loop { ... }` is an **expression**, you can give `break` a value and that value becomes the value of the whole loop:

```rust
fn main() {
    let mut attempts = 0;
    let result = loop {
        attempts += 1;
        if attempts * attempts > 50 {
            break attempts; // hand this value out of the loop
        }
    };
    println!("first n where n*n > 50: {}", result);
}
```

**Output:**

```text
first n where n*n > 50: 8
```

Read `break attempts;` as "stop looping, and the loop expression evaluates to `attempts`." In JavaScript you'd declare `let result;` *before* the loop, assign to it inside, and `break;` separately. Rust folds those three steps into one and lets the result be immutable (`let result`, no `mut`).

> **Note:** `break value` is only valid in a `loop` (or a labeled block — see [Labeled Loops](/04-control-flow/05-labeled-loops/)). A `for` or `while` loop always evaluates to the unit value `()`, so `break` there must be value-less. The loops page covers *why* `loop` is the construct that returns values.

### Combining both: a retry / first-match pattern

A `loop` that `continue`s on failure and `break`s with a value on success is an extremely common idiom. Think "find the first item that parses, otherwise a sentinel":

```rust
fn main() {
    let inputs = ["abc", "not a number", "42", "99"];
    let mut idx = 0;

    let parsed: i32 = loop {
        if idx >= inputs.len() {
            break -1; // sentinel: nothing parsed
        }
        let candidate = inputs[idx];
        idx += 1;
        match candidate.parse::<i32>() {
            Ok(n) => break n,   // success: hand the value back out of the loop
            Err(_) => continue, // bad input: try the next one
        }
    };

    println!("first parseable value: {}", parsed);
}
```

**Output:**

```text
first parseable value: 42
```

> **Tip:** When the sentinel feels hacky, return `Option<T>` instead: `break Some(n)` / `break None`. You'll see exactly that in the [Real-World Example](#real-world-example) and the exercises below. Often an iterator method like `.find_map(...)` is even cleaner; see [Best Practices](#best-practices).

---

## Key Differences from TypeScript/JavaScript

| Concept                         | TypeScript/JavaScript                      | Rust                                                    |
| ------------------------------- | ------------------------------------------ | ------------------------------------------------------- |
| `break` exits nearest loop | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes |
| `continue` skips iteration | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes |
| `break` returns a value | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Not possible | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> `break value` (in `loop` / labeled block) |
| `continue` returns a value | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> No | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> No (it's value-less everywhere) |
| Loop as an expression | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Statements only | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> `let x = loop { ... break v; };` |
| C-style `for (;;i++)` update | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Runs even after `continue` | n/a — no C-style `for`; you advance counters manually |
| `break`/`continue` to a label   | `break outer;` (label is a statement label)| `break 'outer;` (label is a lifetime-style name)        |

### Why does Rust let `break` carry a value?

Because Rust is **expression-oriented**. Almost everything produces a value: `if`/`else` (see [Conditionals](/04-control-flow/00-conditionals/)), `match` (see [Pattern Matching with `match`](/04-control-flow/02-match/)), and blocks all evaluate to something. A `loop` would be the odd one out if it didn't, so Rust closes the gap: `break value` makes a `loop` produce a result the same way `return value` makes a function produce one. This eliminates the "uninitialized variable mutated inside the loop" pattern that's so common (and so bug-prone) in JavaScript.

---

## Common Pitfalls

### Pitfall 1: `break value` in a `for` loop

A `for` loop can never produce a non-unit value, so attaching a value to its `break` is a compile error. TypeScript developers reach for this because they expect any loop to be assignable.

```rust
fn main() {
    let found = for n in 1..10 {
        if n == 5 {
            break n; // does not compile (error E0571: `break` with value from a `for` loop)
        }
    };
    println!("{:?}", found);
}
```

**Real compiler error:**

```text
error[E0571]: `break` with value from a `for` loop
 --> src/main.rs:4:13
  |
2 |     let found = for n in 1..10 {
  |                 -------------- you can't `break` with a value in a `for` loop
3 |         if n == 5 {
4 |             break n; // trying to return a value from a `for`
  |             ^^^^^^^ can only break with a value inside `loop` or breakable block
  |
help: use `break` on its own without a value inside this `for` loop
  |
4 -             break n; // trying to return a value from a `for`
4 +             break; // trying to return a value from a `for`
  |
```

**Fix:** Use `loop` when you need a value, or restructure with an iterator method like `.find()`:

```rust
fn main() {
    let found = (1..10).find(|&n| n == 5); // Option<i32>
    println!("{:?}", found); // Some(5)
}
```

### Pitfall 2: Mismatched types across multiple `break` values

Every `break value` in the same `loop` must produce the *same* type. The loop has exactly one result type, just like a function has one return type.

```rust
fn main() {
    let x = loop {
        break 5;
        break "done"; // does not compile (error E0308: mismatched types)
    };
    println!("{}", x);
}
```

**Real compiler error** (the unreachable second `break` also triggers a warning):

```text
warning: unreachable statement
 --> src/main.rs:4:9
  |
3 |         break 5;
  |         ------- any code following this expression is unreachable
4 |         break "done"; // type mismatch with earlier break
  |         ^^^^^^^^^^^^^ unreachable statement
  |
  = note: `#[warn(unreachable_code)]` on by default

error[E0308]: mismatched types
 --> src/main.rs:4:15
  |
3 |         break 5;
  |         ------- expected because of this `break`
4 |         break "done"; // type mismatch with earlier break
  |               ^^^^^^ expected integer, found `&str`
```

**Fix:** Make all `break` values share a type (here, both `&str`):

```rust
fn main() {
    let x = loop {
        if true {
            break "five";
        }
        break "done";
    };
    println!("{}", x); // five
}
```

### Pitfall 3: `break`/`continue` outside any loop

In JavaScript these are also illegal outside a loop, but the error fires at parse time. Rust gives you a precise, named diagnostic.

```rust
fn main() {
    let n = 5;
    if n > 3 {
        break; // does not compile (error E0268: `break` outside of a loop)
    }
}
```

**Real compiler error:**

```text
error[E0268]: `break` outside of a loop or labeled block
 --> src/main.rs:4:9
  |
4 |         break; // not inside any loop
  |         ^^^^^ cannot `break` outside of a loop or labeled block
```

> **Note:** The message says "loop **or labeled block**." Rust does let you `break` out of a labeled *block* (`'name: { ... break 'name value; }`) even though it isn't a loop — a niche feature covered in [Labeled Loops](/04-control-flow/05-labeled-loops/). A bare `if` block is not breakable.

### Pitfall 4: Expecting `continue` to carry a value

`continue` never takes a value — not even in a `loop`. There's no "continue with X" concept in Rust (or JavaScript). Trying it is a syntax error:

```rust
fn main() {
    let mut total = 0;
    for n in 1..5 {
        total += continue 0; // does not compile (syntax error: continue takes no value)
    }
    println!("{}", total);
}
```

**Real compiler error:**

```text
error: expected one of `.`, `;`, `?`, `}`, or an operator, found `0`
 --> src/main.rs:4:27
  |
4 |         total += continue 0; // continue does not take a value
  |                           ^ expected one of `.`, `;`, `?`, `}`, or an operator
```

**Fix:** `continue` stands alone. If you wanted to add `0` and move on, just `continue;` (adding `0` is a no-op anyway).

---

## Best Practices

### 1. Reach for `break value` instead of a pre-declared mutable

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> JavaScript-style: declare, mutate, break separately**

```rust
fn main() {
    let mut result = 0; // needs `mut`, starts as a meaningless 0 (rustc warns this initial value is never read — which is exactly the point)
    let mut n = 0;
    loop {
        n += 1;
        if n * n > 50 {
            result = n;
            break;
        }
    }
    println!("{}", result);
}
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Idiomatic Rust: let the loop be the value**

```rust
fn main() {
    let mut n = 0;
    let result = loop {
        n += 1;
        if n * n > 50 {
            break n;
        }
    };
    println!("{}", result); // 8
}
```

The second version makes `result` immutable and removes the meaningless initial value.

### 2. Prefer iterator adapters over manual `break`/`continue` when the intent is "filter" or "stop early"

`continue` to skip and `break` to stop early often map directly onto `filter`, `take_while`, `find`, and friends. The iterator version states the *intent* and is harder to get wrong.

```rust
fn main() {
    // Imperative: `continue` to skip evens, `break` to stop past 19.
    let mut sum_imperative = 0;
    for n in 1..=100 {
        if n % 2 == 0 {
            continue;
        }
        if n > 19 {
            break;
        }
        sum_imperative += n;
    }

    // Iterator-idiomatic equivalent.
    let sum_iter: i32 = (1..=100)
        .take_while(|&n| n <= 19)
        .filter(|&n| n % 2 != 0)
        .sum();

    println!("imperative = {}, iterator = {}", sum_imperative, sum_iter);
    assert_eq!(sum_imperative, sum_iter);
}
```

**Output:**

```text
imperative = 100, iterator = 100
```

> **Tip:** Use `take_while` for the `break` (stop when a condition stops holding) and `filter` for the `continue` (skip items that don't match). For "find the first match and return it," `find` / `find_map` replace the whole `loop { ... break value; }` dance. Reach for explicit `break`/`continue` when the control flow is genuinely irregular (multiple exit conditions, side effects, mutation of outside state).

### 3. Keep the loop body small enough that the exit conditions are obvious

A loop with three `break`s and four `continue`s buried in nested `if`s is hard to follow. If you find yourself there, extract the body into a function that returns an enum describing what to do (`Keep`, `Skip`, `Stop`), or switch to an iterator pipeline.

### 4. Don't forget to advance counters in `while` loops before `continue`

As noted above, a `continue` in a `while` loop bypasses any code below it, including a counter increment. Put the increment first, or use a `for` over a range so the iterator advances for you.

---

## Real-World Example

A small networking utility: given a list of candidate port strings (from a config file, environment variable, CLI args, etc.), find the first one that is both a valid port number and a non-privileged port (>= 1024). This combines `continue` (skip invalid candidates) with `break Some(_)` / `break None` (produce a typed result).

```rust
/// Return the first candidate that parses as a valid, non-privileged port.
fn first_valid_port(candidates: &[&str]) -> Option<u16> {
    let mut idx = 0;
    let port = loop {
        if idx >= candidates.len() {
            break None; // exhausted all candidates
        }
        let raw = candidates[idx];
        idx += 1;
        match raw.parse::<u16>() {
            Ok(p) if p >= 1024 => break Some(p), // valid, non-privileged port
            _ => continue,                        // skip invalid / privileged
        }
    };
    port
}

fn main() {
    let candidates = ["not-a-port", "80", "8080", "3000"];
    println!("{:?}", first_valid_port(&candidates)); // Some(8080)

    let none_valid = ["xyz", "22", "443"];
    println!("{:?}", first_valid_port(&none_valid)); // None
}
```

**Output:**

```text
Some(8080)
None
```

Notice how `break Some(8080)` and `break None` give the `loop` a uniform `Option<u16>` type, and the whole result flows into an immutable `port`. In TypeScript you'd track a `result` variable and a found-flag, or `return` early from a helper. Rust lets the loop itself be the answer.

> **Tip:** This exact "find the first match" shape is what `Iterator::find_map` exists for: `candidates.iter().find_map(|raw| raw.parse::<u16>().ok().filter(|&p| p >= 1024))`. Use the explicit `loop` when the iteration also needs side effects or an index you can't easily express as an adapter.

---

## Further Reading

### Official Documentation

- [The Rust Book — Repetition with Loops](https://doc.rust-lang.org/book/ch03-05-control-flow.html#repetition-with-loops) — covers `break`, `continue`, and returning values from loops.
- [Rust Reference — `break` expressions](https://doc.rust-lang.org/reference/expressions/loop-expr.html#break-expressions)
- [Rust Reference — `continue` expressions](https://doc.rust-lang.org/reference/expressions/loop-expr.html#continue-expressions)
- [Rust by Example — Loops: `break` and `continue`](https://doc.rust-lang.org/rust-by-example/flow_control/loop.html)

### Related Topics in This Guide

- [Loops](/04-control-flow/01-loops/): `for`, `while`, and `loop`; the constructs `break`/`continue` operate on, and how `loop` returns a value.
- [Labeled Loops](/04-control-flow/05-labeled-loops/) — `break`/`continue` to an outer loop with a `'label`, and breaking out of labeled blocks.
- [Conditionals](/04-control-flow/00-conditionals/): `if`/`else` as an expression, which pairs with the conditions that trigger `break`/`continue`.
- [Match](/04-control-flow/02-match/) — using `match` arms (as in the log-scanning example) to decide between `continue`, `break`, and normal work.
- [if let and while let](/04-control-flow/03-if-let-while-let/): concise pattern-matching loop conditions that often remove the need for a manual `break`.
- [Functions: Return Values](/03-functions/02-return-values/) — `return` from a function is the function-level analogue of `break` from a loop.
- [Ownership](/05-ownership/): why immutable bindings (which `break value` enables) are central to Rust.

---

## Exercises

### Exercise 1: Skip and Stop

**Difficulty:** Beginner

**Objective:** Practice using `continue` to skip and `break` to terminate within a single `for` loop.

**Instructions:** Implement `sum_positive_until_zero`. It should iterate over a slice of `i32`, *ignore* any negative numbers (use `continue`), *stop* completely as soon as it sees a `0` (use `break`), and return the sum of the positive numbers seen before the zero.

```rust
fn sum_positive_until_zero(nums: &[i32]) -> i32 {
    let mut sum = 0;
    // TODO: loop with continue (skip negatives) and break (stop at 0)
    sum
}

fn main() {
    let data = [3, -1, 4, -5, 2, 0, 100];
    println!("{}", sum_positive_until_zero(&data)); // expected: 9
}
```

<details>
<summary>Solution</summary>

```rust
fn sum_positive_until_zero(nums: &[i32]) -> i32 {
    let mut sum = 0;
    for &n in nums {
        if n == 0 {
            break; // 0 is a terminator
        }
        if n < 0 {
            continue; // ignore negatives
        }
        sum += n;
    }
    sum
}

fn main() {
    let data = [3, -1, 4, -5, 2, 0, 100];
    println!("{}", sum_positive_until_zero(&data)); // 3 + 4 + 2 = 9
}
```

**Output:**

```text
9
```

The `100` after the `0` is never added because `break` ends the loop at the zero.

</details>

### Exercise 2: Break with a Value

**Difficulty:** Intermediate

**Objective:** Use `loop` plus `break value` to compute and return a result without a pre-declared mutable.

**Instructions:** Implement `first_square_over(limit)`. Starting from `1`, find the smallest positive integer `n` whose square is strictly greater than `limit`, and return `n`. Use a `loop` and `break n;` — do **not** declare a separate `result` variable.

```rust
fn first_square_over(limit: u32) -> u32 {
    // TODO: use `loop` and `break n;`
}

fn main() {
    println!("{}", first_square_over(50));  // expected: 8  (8*8 = 64)
    println!("{}", first_square_over(100)); // expected: 11 (11*11 = 121)
}
```

<details>
<summary>Solution</summary>

```rust
fn first_square_over(limit: u32) -> u32 {
    let mut n = 0;
    loop {
        n += 1;
        if n * n > limit {
            break n; // the loop expression evaluates to n
        }
    }
}

fn main() {
    println!("{}", first_square_over(50));  // 8
    println!("{}", first_square_over(100)); // 11
}
```

**Output:**

```text
8
11
```

Because the `loop` is the last expression in the function, its broken-out value is also the function's return value; no `return` keyword needed.

</details>

### Exercise 3: First Valid Result or `None`

**Difficulty:** Advanced

**Objective:** Combine `continue` (skip failures) with `break Some(_)` / `break None` to make a `loop` produce a typed `Option`.

**Instructions:** Implement `first_even_parse(candidates: &[&str]) -> Option<i32>`. Walk the candidates in order; parse each as `i32`. Skip ones that fail to parse *or* are odd (use `continue`). Return `Some(n)` for the first candidate that parses to an **even** number, or `None` if none qualify. Use a `loop` with an index — not an iterator adapter — so you practice the pattern explicitly.

```rust
fn first_even_parse(candidates: &[&str]) -> Option<i32> {
    // TODO: loop over an index; continue on failures/odds; break Some/None
}

fn main() {
    println!("{:?}", first_even_parse(&["x", "3", "10", "12"])); // Some(10)
    println!("{:?}", first_even_parse(&["1", "abc", "7"]));      // None
}
```

<details>
<summary>Solution</summary>

```rust
fn first_even_parse(candidates: &[&str]) -> Option<i32> {
    let mut idx = 0;
    loop {
        if idx >= candidates.len() {
            break None; // ran out of candidates
        }
        let raw = candidates[idx];
        idx += 1;
        match raw.parse::<i32>() {
            Ok(n) if n % 2 == 0 => break Some(n), // first even wins
            _ => continue,                         // unparseable or odd: skip
        }
    }
}

fn main() {
    println!("{:?}", first_even_parse(&["x", "3", "10", "12"])); // Some(10)
    println!("{:?}", first_even_parse(&["1", "abc", "7"]));      // None
}
```

**Output:**

```text
Some(10)
None
```

> **Tip:** The idiomatic one-liner is `candidates.iter().find_map(|s| s.parse::<i32>().ok().filter(|n| n % 2 == 0))`. Reach for the explicit `loop` form when you need the index or extra side effects; reach for `find_map` when you don't.

</details>

---

## Summary

**What you've learned:**

- `continue` skips to the next iteration; `break` exits the loop, same as JavaScript.
- A `loop` is an **expression**: `break value;` makes the loop evaluate to `value`.
- `for`/`while` loops always evaluate to `()`, so their `break` must be value-less.
- All `break value` expressions in one loop must share a single type.
- `continue` never carries a value.
- The "skip with `continue`, return with `break value`" pattern frequently has a cleaner iterator equivalent (`filter`, `take_while`, `find`, `find_map`).

**Key syntax** (skeleton, not runnable — `skip`/`done`/`work`/`found`/`value` are placeholders):

```text
for x in iter {
    if skip(x)  { continue; } // skip this item
    if done(x)  { break; }    // value-less break (for/while)
    work(x);
}

let answer = loop {
    if found() { break value; } // break WITH a value (loop only)
};
```
