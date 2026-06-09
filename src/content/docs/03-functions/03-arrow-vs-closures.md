---
title: "Arrow Functions vs Closures"
description: "Rust closures are JavaScript arrow functions with |args| syntax, but the compiler tracks how they capture state through the Fn, FnMut, and FnOnce traits and"
---

If there is one Rust feature that feels immediately *familiar* to a TypeScript/JavaScript developer, it is the **closure**. Rust closures are the direct counterpart of JavaScript arrow functions: small, anonymous functions that capture variables from the surrounding scope. The syntax is different (`|args|` instead of `(args) =>`), and Rust's ownership system adds rules that JavaScript never enforces, but the mental model transfers almost perfectly.

---

## Quick Overview

A **closure** is an anonymous function that can **capture** values from the scope in which it is defined. In TypeScript/JavaScript you write these as arrow functions (`(x) => x * 2`); in Rust you write them with vertical-bar parameter lists (`|x| x * 2`). The big new idea is *how* a closure captures its environment: Rust tracks whether a closure reads, mutates, or takes ownership of captured values, and encodes that in three traits: `Fn`, `FnMut`, and `FnOnce`.

---

## TypeScript/JavaScript Example

```typescript
// A "counter factory" — a classic use of closures in JavaScript.
// The returned arrow function closes over the `count` variable.
function makeCounter(): () => number {
  let count = 0;
  return () => {
    count += 1; // mutates a captured variable
    return count;
  };
}

const next = makeCounter();
console.log(next()); // 1
console.log(next()); // 2
console.log(next()); // 3

// Arrow functions capturing a value from the enclosing scope:
const factor = 3;
const scale = (n: number) => n * factor; // closes over `factor`
console.log(scale(5)); // 15

// Closures are also the everyday argument to array methods:
const numbers = [1, 2, 3, 4, 5, 6];
const threshold = 3;
const big = numbers.filter((n) => n > threshold); // closes over `threshold`
console.log(big); // [4, 5, 6]
```

**Key points:**

- Arrow functions can read **and** mutate variables from the enclosing scope.
- Capture is implicit and always by reference to the variable.
- There is no notion of "ownership"; the garbage collector keeps captured values alive as long as the closure exists.

---

## Rust Equivalent

```rust
fn main() {
    // The same closures, written the Rust way.
    let double = |n: i32| n * 2;
    println!("{}", double(10)); // 20

    let add = |a: i32, b: i32| a + b;
    println!("{}", add(3, 4)); // 7

    // A multi-line body uses braces, just like a block expression:
    let describe = |n: i32| {
        let parity = if n % 2 == 0 { "even" } else { "odd" };
        format!("{n} is {parity}")
    };
    println!("{}", describe(7)); // 7 is odd

    // A zero-argument closure has empty bars:
    let greet = || println!("Hello from a closure!");
    greet();

    // Capturing a value from the environment (by reference):
    let factor = 3;
    let scale = |n: i32| n * factor; // borrows `factor`
    println!("{}", scale(5)); // 15
    println!("factor still usable: {factor}"); // 3

    // Capturing and MUTATING a value (an FnMut closure):
    let mut count = 0;
    let mut increment = || {
        count += 1;
        count
    };
    println!("{}", increment()); // 1
    println!("{}", increment()); // 2

    // Taking OWNERSHIP of a captured value with `move` (an FnOnce closure here):
    let name = String::from("Ada");
    let consume = move || name.to_uppercase(); // `name` was moved in
    println!("{}", consume()); // ADA
}
```

**Output:**

```
20
7
7 is odd
Hello from a closure!
15
factor still usable: 3
1
2
ADA
```

**Key points:**

- Parameters go between `|bars|`; the body is a single expression or a `{ block }`.
- Parameter and return types are usually *inferred* — you rarely annotate them.
- How a closure captures (`borrow`, `mutable borrow`, or `move`) is decided by the compiler based on what the body does, and you can force a move with the `move` keyword.

---

## Detailed Explanation

### Syntax: `|args|` is the new `(args) =>`

The translation is mechanical:

| TypeScript/JavaScript      | Rust                       |
| -------------------------- | -------------------------- |
| `(n) => n * 2`             | `\|n\| n * 2`              |
| `(a, b) => a + b`          | `\|a, b\| a + b`           |
| `() => doThing()`          | `\|\| do_thing()`          |
| `(n) => { ...; return x; }`| `\|n\| { ...; x }`         |

In Rust the body after `|args|` is an expression. A single expression needs no braces (`|n| n * 2`). For multiple statements you use a `{ }` block, and (as everywhere in Rust) the final expression *without* a semicolon is the return value. (See [Statements vs expressions](/03-functions/00-basic-functions/) and [return values](/03-functions/02-return-values/) for the full story.)

> **Note:** Closure parameter types are usually inferred, unlike a top-level `fn`, which *must* annotate every parameter. When inference needs help you can annotate: `|n: i32| -> i32 { n * 2 }`. The `-> ReturnType` is only allowed when the body is a braced block.

### How capture works: the three traits

This is the part with no JavaScript equivalent. When a closure uses a variable from its surroundings, the compiler picks the *least invasive* way to capture it, and that choice determines which of three **traits** the closure implements:

- **`Fn`**: the closure captures values **by immutable reference** (`&T`). It only reads them, so it can be called many times and even from several places at once.
- **`FnMut`**: the closure captures **by mutable reference** (`&mut T`). It changes captured state, so it can be called many times but needs a `mut` binding.
- **`FnOnce`**: the closure captures **by value** (it takes ownership / moves things in). It may only be guaranteed to be callable **once**, because calling it might consume the captured values.

These form a hierarchy: every `Fn` is also an `FnMut`, and every `FnMut` is also an `FnOnce`. (Anything you can call repeatedly, you can certainly call once.) When you *accept* a closure as a parameter, ask for the **weakest** trait that works: `FnOnce` if you call it once, `FnMut` if you call it repeatedly and it mutates, `Fn` if it only reads.

```rust
// Reads its environment only — call it as many times as you like.
fn apply_twice<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(f(x))
}

// Mutates state across calls — note `mut f`.
fn repeat<F: FnMut()>(mut f: F, times: u32) {
    for _ in 0..times {
        f();
    }
}

// May consume captured values — called exactly once.
fn call_once<F: FnOnce() -> String>(f: F) -> String {
    f()
}

fn main() {
    println!("{}", apply_twice(|n| n + 3, 10)); // 16

    let mut total = 0;
    repeat(|| total += 1, 5);
    println!("{total}"); // 5

    let greeting = String::from("hi");
    let msg = call_once(move || greeting + " there"); // moves `greeting` in
    println!("{msg}"); // hi there
}
```

**Output:**

```
16
5
hi there
```

The compiler chose the trait for each closure automatically: `|n| n + 3` only reads, so it is `Fn`; `|| total += 1` mutates `total`, so it is `FnMut`; `move || greeting + " there"` consumes `greeting` (the `+` operator on `String` takes ownership of the left side), so it is only `FnOnce`.

### Capture by reference is the default

In the earlier `scale` example, `|n: i32| n * factor` only *reads* `factor`, so the closure borrows it immutably. That is why `factor` is still usable on the next line: the closure holds a shared reference, not the value itself. This is exactly like JavaScript closing over a `const`, except Rust's borrow checker enforces that nobody mutates `factor` while the closure's borrow is alive.

### `move`: forcing capture by value

The `move` keyword forces a closure to take **ownership** of everything it captures, instead of borrowing. You need it whenever the closure must outlive the scope that created the captured values, most importantly when spawning threads or returning a closure from a function:

```rust
use std::thread;

fn main() {
    let data = vec![1, 2, 3];

    // `move` transfers ownership of `data` into the new thread, so the
    // closure can safely outlive `main`'s stack frame.
    let handle = thread::spawn(move || {
        let sum: i32 = data.iter().sum();
        println!("sum in thread: {sum}");
    });

    handle.join().unwrap();
}
```

**Output:**

```
sum in thread: 6
```

Without `move`, the closure would try to *borrow* `data`, but the new thread might run after `main` returns and frees `data`: a use-after-free that the borrow checker refuses to allow. The `move` keyword resolves this by handing ownership of `data` to the closure.

> **Tip:** `move` does not automatically make a closure `FnOnce`. A `move` closure that only *reads* its captured values (or captures only `Copy` types) is still `Fn`. `move` controls *how* values are captured (by value vs by reference); the `Fn`/`FnMut`/`FnOnce` traits describe what *calling* the closure does to those captured values.

For `Copy` types (like integers), `move` copies the value in, leaving the original usable:

```rust
fn main() {
    let x = 10; // i32 is Copy
    let add_x = move |n: i32| n + x; // copies x into the closure
    println!("{}", add_x(5)); // 15
    println!("x still usable: {x}"); // 10 — Copy types are duplicated, not moved away
}
```

**Output:**

```
15
10
```

---

## Key Differences

| Concept                  | TypeScript/JavaScript                        | Rust                                                              |
| ------------------------ | -------------------------------------------- | ----------------------------------------------------------------- |
| Syntax                   | `(a, b) => a + b`                            | `\|a, b\| a + b`                                                  |
| Parameter types          | Optional annotations; erased at runtime      | Usually inferred; can annotate `\|n: i32\|`                       |
| How capture is decided   | Always by reference to the variable          | Compiler picks `&`, `&mut`, or by-value (override with `move`)    |
| Mutating captured state  | Always allowed                               | Closure must be `FnMut` and bound with `let mut`                  |
| Lifetime of captured data| GC keeps it alive forever if needed          | Borrow checker enforces captured borrows do not outlive the data  |
| Calling repeatedly       | Always allowed                               | `Fn`/`FnMut` yes; `FnOnce` only guaranteed once                   |
| Each closure's type      | Structural: same signature = same type      | Every closure has a unique, anonymous, compiler-generated type    |

### Every closure has its own unique type

In TypeScript, two arrow functions with the same signature share the type `(n: number) => number`. In Rust, **each closure has its own anonymous type** generated by the compiler — even two closures with identical signatures are different types. This is why functions that take closures use generics (`F: Fn(...)`) or trait objects (`Box<dyn Fn(...)>`), a topic covered in detail in [Higher-Order Functions](/03-functions/04-higher-order/) and [Function Pointers](/03-functions/05-function-pointers/).

### Closures vs. plain functions

A Rust closure can capture its environment; a named `fn` cannot. That captured environment is stored inline in the closure value, so closures are *not* the same as a `fn` pointer. The relationship between the two, and when a closure can coerce to a function pointer, is the subject of [Function Pointers](/03-functions/05-function-pointers/).

---

## Common Pitfalls

### Pitfall 1: A closure's parameter type is locked in at first use

Because closure types are inferred, the compiler fixes the parameter and return types the **first time** the closure is called (or otherwise constrained). Calling it later with a different type is an error; there is no implicit polymorphism like a JavaScript function would have.

```rust
fn main() {
    let identity = |x| x;
    let a = identity(5);
    let b = identity("hello"); // ← different type!
    println!("{a} {b}");
}
```

**Real compiler error:**

```
error[E0308]: mismatched types
 --> src/main.rs:4:22
  |
4 |     let b = identity("hello");
  |             -------- ^^^^^^^ expected integer, found `&str`
  |             |
  |             arguments to this function are incorrect
  |
note: expected because the closure was earlier called with an argument of type `{integer}`
 --> src/main.rs:3:22
  |
3 |     let a = identity(5);
  |             -------- ^ expected because this argument is of type `{integer}`
  |             |
  |             in this closure call
note: closure parameter defined here
 --> src/main.rs:2:21
  |
2 |     let identity = |x| x;
  |                     ^
```

If you genuinely need "one function, many types," that is what *generics* are for — use a generic `fn` (see [Generics & Traits](/09-generics-traits/)), not a closure.

### Pitfall 2: Using a value after a `move` closure took it

A `move` closure that captures a non-`Copy` value (like a `String` or `Vec`) takes ownership. The original variable is then gone:

```rust
fn main() {
    let name = String::from("Ada");
    let consume = move || println!("{name}");
    consume();
    println!("{name}"); // ← `name` was moved into the closure
}
```

**Real compiler error:**

```
error[E0382]: borrow of moved value: `name`
 --> src/main.rs:5:16
  |
2 |     let name = String::from("Ada");
  |         ---- move occurs because `name` has type `String`, which does not implement the `Copy` trait
3 |     let consume = move || println!("{name}");
  |                   -------            ---- variable moved due to use in closure
  |                   |
  |                   value moved into closure here
4 |     consume();
5 |     println!("{name}"); // ← `name` was moved into the closure
  |                ^^^^ value borrowed here after move
  |
  = note: this error originates in the macro `$crate::format_args_nl` which comes from the expansion of the macro `println` (in Nightly builds, run with -Z macro-backtrace for more info)
help: consider cloning the value before moving it into the closure
  |
3 ~     let value = name.clone();
4 ~     let consume = move || println!("{value}");
  |
```

As the compiler suggests, clone the value first if you need both copies, or simply do not use `move` if a borrow is sufficient. This is ownership in action; see [Ownership](/05-ownership/) for the full model.

### Pitfall 3: Forgetting `let mut` for an `FnMut` closure

A closure that mutates captured state must itself be stored in a `mut` binding, because *calling* it mutably borrows the closure:

```rust
fn main() {
    let mut count = 0;
    let increment = || {
        count += 1;
    };
    increment(); // ← needs a mutable binding
    println!("{count}");
}
```

**Real compiler error:**

```
error[E0596]: cannot borrow `increment` as mutable, as it is not declared as mutable
 --> src/main.rs:6:5
  |
4 |         count += 1;
  |         ----- calling `increment` requires mutable binding due to mutable borrow of `count`
5 |     };
6 |     increment(); // ← needs a mutable binding
  |     ^^^^^^^^^ cannot borrow as mutable
  |
help: consider changing this to be mutable
  |
3 |     let mut increment = || {
  |         +++
```

The fix is exactly what the compiler says: `let mut increment = || { ... };`. This trips up TypeScript/JavaScript developers because in JavaScript a mutating closure is indistinguishable from a non-mutating one: there is no `mut` to forget.

---

## Best Practices

### 1. Let types be inferred

Idiomatic Rust closures are terse. Prefer `|n| n * 2` over `|n: i32| -> i32 { n * 2 }` unless the compiler cannot infer the types or an annotation genuinely aids readability.

### 2. Only use `move` when you need it

Reach for `move` when the closure must outlive its defining scope: spawning threads, returning a closure, or storing one in a struct or `async` task. For ordinary inline use with iterator adapters, the default borrowing capture is usually what you want and avoids unnecessary clones.

### 3. Accept the weakest trait that works

When writing a function that takes a closure, choose `FnOnce` if you call it once, `FnMut` if it mutates and you call it repeatedly, and `Fn` if it only reads. Asking for less makes your function callable with more closures. (Details and the `impl Fn` / `Box<dyn Fn>` return patterns live in [Higher-Order Functions](/03-functions/04-higher-order/).)

### 4. Use closures with iterator adapters

The single most common place you will write closures is in iterator chains: `map`, `filter`, `fold`, and friends. This replaces the `Array.prototype` methods you know from JavaScript:

```rust
fn main() {
    let numbers = vec![1, 2, 3, 4, 5, 6];
    let threshold = 3;
    // `into_iter` consumes the vec; the closure captures `threshold` by reference.
    let big: Vec<i32> = numbers.into_iter().filter(|&n| n > threshold).collect();
    println!("{big:?}"); // [4, 5, 6]
}
```

**Output:**

```
[4, 5, 6]
```

> **Tip:** `|&n|` here is *pattern matching* in the parameter position: `filter` hands the closure a `&i32`, and `&n` destructures it so `n` is a plain `i32`. Without it you would write `|n| *n > threshold`.

---

## Real-World Example

A retry helper is a production-flavored use of an `FnMut` closure: real retries often mutate state between attempts (bumping a counter, rotating an endpoint). This mirrors a hand-rolled `retry` utility you might write in TypeScript.

**TypeScript:**

```typescript
function retry<T>(operation: () => T, maxAttempts: number): T {
  let lastErr: unknown;
  for (let i = 0; i < maxAttempts; i++) {
    try {
      return operation();
    } catch (e) {
      lastErr = e;
    }
  }
  throw lastErr;
}

let attempt = 0;
const result = retry(() => {
  attempt += 1; // the closure mutates captured state
  if (attempt < 3) throw new Error(`attempt ${attempt} failed`);
  return `succeeded on attempt ${attempt}`;
}, 5);

console.log(result); // "succeeded on attempt 3"
```

**Rust:**

```rust
/// Runs `operation` up to `max_attempts` times, stopping at the first `Ok`.
/// The operation is `FnMut` because real-world retries often mutate state
/// (bump an attempt counter, rotate an endpoint, etc.).
fn retry<T, E, F>(mut operation: F, max_attempts: u32) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    let mut last_err = None;
    for _ in 0..max_attempts {
        match operation() {
            Ok(value) => return Ok(value),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.expect("max_attempts must be > 0"))
}

fn main() {
    // Simulate a flaky service that fails twice, then succeeds.
    let mut attempt = 0;
    let result = retry(
        || {
            attempt += 1; // the closure mutates captured state -> FnMut
            if attempt < 3 {
                Err(format!("attempt {attempt} failed"))
            } else {
                Ok(format!("succeeded on attempt {attempt}"))
            }
        },
        5,
    );

    match result {
        Ok(msg) => println!("OK: {msg}"),
        Err(e) => println!("gave up: {e}"),
    }
}
```

**Output:**

```
OK: succeeded on attempt 3
```

Notice the contrasts with the TypeScript version. Rust uses `Result<T, E>` instead of exceptions (no `try`/`catch`), the closure parameter is constrained with `F: FnMut() -> Result<T, E>` in a `where` clause, and `operation` is declared `mut` so it can be called repeatedly while mutating `attempt`.

---

## Further Reading

### Official Documentation

- [The Rust Book — Closures: Anonymous Functions that Capture Their Environment](https://doc.rust-lang.org/book/ch13-01-closures.html)
- [Rust by Example — Closures](https://doc.rust-lang.org/rust-by-example/fn/closures.html)
- [Rust Reference — Closure expressions](https://doc.rust-lang.org/reference/expressions/closure-expr.html)
- [`std::ops::Fn`](https://doc.rust-lang.org/std/ops/trait.Fn.html), [`FnMut`](https://doc.rust-lang.org/std/ops/trait.FnMut.html), [`FnOnce`](https://doc.rust-lang.org/std/ops/trait.FnOnce.html)

### Related Topics in This Guide

- [Basic Functions](/03-functions/00-basic-functions/) — `fn` definitions, signatures, statements vs expressions
- [Function Parameters](/03-functions/01-parameters/) — Rust's alternatives to default and rest parameters
- [Return Values](/03-functions/02-return-values/) — tail expressions, the unit type, returning tuples
- [Higher-Order Functions](/03-functions/04-higher-order/) — taking and returning closures (`impl Fn`, `Box<dyn Fn>`); `map`/`filter`/`fold`
- [Function Pointers](/03-functions/05-function-pointers/) — the `fn` type, named functions, closures vs function items
- [Ownership](/05-ownership/) — the model behind `move` and capture
- [Variables and Mutability](/02-basics/00-variables/) — why `let mut` matters for `FnMut`
- [Control Flow](/04-control-flow/) — the iterator chains where closures live

---

## Exercises

### Exercise 1: Translate the arrow functions

**Difficulty:** Easy

**Objective:** Get comfortable with `|args|` syntax and capture-by-reference.

**Instructions:** Rewrite this TypeScript snippet in Rust. Keep `threshold` as a captured variable; do not hardcode it into the closure.

```typescript
const numbers = [1, 2, 3, 4, 5, 6];
const threshold = 3;
const big = numbers.filter((n) => n > threshold);
console.log(big); // [4, 5, 6]
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let numbers = vec![1, 2, 3, 4, 5, 6];
    let threshold = 3;
    let big: Vec<i32> = numbers.into_iter().filter(|&n| n > threshold).collect();
    println!("{big:?}"); // [4, 5, 6]
}
```

The closure `|&n| n > threshold` captures `threshold` by immutable reference (so the closure is `Fn`) and uses a `&n` pattern to destructure the `&i32` that `filter` provides.

</details>

### Exercise 2: A counter factory

**Difficulty:** Medium

**Objective:** Return a closure that owns and mutates its own state — the Rust version of JavaScript's `makeCounter`.

**Instructions:** Implement `make_counter` so that it returns a closure which yields `1`, then `2`, then `3` on successive calls. The closure must own its count (it outlives `make_counter`), so you will need `move` and an `FnMut` return type.

```rust
fn make_counter() -> /* ??? */ {
    // ...
}

fn main() {
    let mut counter = make_counter();
    println!("{}", counter()); // 1
    println!("{}", counter()); // 2
    println!("{}", counter()); // 3
}
```

<details>
<summary>Solution</summary>

```rust
// `impl FnMut() -> u32` means "some concrete type that implements FnMut".
fn make_counter() -> impl FnMut() -> u32 {
    let mut count = 0;
    move || {
        count += 1;
        count
    }
}

fn main() {
    let mut counter = make_counter();
    println!("{}", counter()); // 1
    println!("{}", counter()); // 2
    println!("{}", counter()); // 3
}
```

**Output:**

```
1
2
3
```

`move` is required because `count` is local to `make_counter` and must travel out with the returned closure. The binding in `main` must be `let mut counter`, because calling an `FnMut` closure mutably borrows it. Returning closures with `impl Fn` is explored further in [Higher-Order Functions](/03-functions/04-higher-order/).

</details>

### Exercise 3: Consume a captured value once

**Difficulty:** Medium

**Objective:** Build intuition for `FnOnce` and `move` capturing an owned `String`.

**Instructions:** Write a function `run_once` that accepts a closure returning a `String` and calls it a single time, returning its result. Then call it with a `move` closure that consumes a captured `String` by concatenating to it. Concatenating with `+` on a `String` takes ownership of the left operand, which forces the closure to be `FnOnce`.

```rust
fn run_once</* ??? */>(f: F) -> String {
    // ...
}

fn main() {
    let banner = String::from("DEPLOYING");
    // Build a closure that produces "DEPLOYING... done" and pass it to run_once.
}
```

<details>
<summary>Solution</summary>

```rust
fn run_once<F: FnOnce() -> String>(f: F) -> String {
    f()
}

fn main() {
    let banner = String::from("DEPLOYING");
    let job = move || format!("{banner}... done"); // captures `banner` by value
    let report = run_once(job);
    println!("{report}"); // DEPLOYING... done
}
```

**Output:**

```
DEPLOYING... done
```

Because `run_once` only ever calls `f` once, `FnOnce` is the correct (weakest) bound — it accepts closures that consume their captured values, as well as any `Fn`/`FnMut` closure. This is the best-practice "ask for the weakest trait" rule in action.

</details>

---

## Summary

**What you've learned:**

- Rust closures are arrow functions with `|args|` syntax instead of `(args) =>`.
- Parameter and return types are usually inferred; bodies are expressions or `{ }` blocks.
- Closures capture their environment by `&`, `&mut`, or by value — the compiler picks the least invasive option.
- `Fn` reads, `FnMut` mutates, `FnOnce` consumes; they form a hierarchy, and you accept the weakest one that works.
- `move` forces capture by value, needed when a closure must outlive its scope (threads, returned closures).
- Every closure has a unique anonymous type — unlike TypeScript, where any two functions with the same structural signature share a type.

**The mental model:** A closure is a small struct holding its captured variables plus a call method. JavaScript hides that machinery and lets the garbage collector sort out lifetimes; Rust makes capture explicit and proves at compile time that no captured borrow outlives its data.
