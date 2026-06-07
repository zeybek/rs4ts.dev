---
title: "Recursion"
description: "Recursion looks identical to TypeScript, but Rust guarantees no tail-call optimization, aborts on stack overflow, and needs Box for recursive enums like a cons list."
---

A function that calls itself is **recursion**, and it works the same way in Rust as it does in TypeScript: you split a problem into a smaller version of itself plus a base case. The surprises for a TypeScript/JavaScript developer are not about syntax — they are about *limits*. Rust does **not** guarantee tail-call optimization, so deep recursion overflows the stack just like it does in V8, and Rust's strict type system forces you to reach for `Box` the moment you write a **recursive data type**.

---

## Quick Overview

**Recursion** is a function defined in terms of itself: a **base case** that stops the recursion and a **recursive case** that calls the function with a smaller input. Rust supports recursion fully, but, like JavaScript engines, it makes **no guarantee** of tail-call optimization, so each call consumes a real stack frame and very deep recursion will abort with a stack overflow. This file shows recursive functions, why deep recursion is risky, how to convert recursion to iteration, and a first look at recursive data types that need `Box`.

> **Note:** This file focuses on *recursive functions*, *stack depth*, and *iterative alternatives*, with a teaser of *recursive enums*. The function-definition basics (typed parameters, return types, tail expressions) live in [Basic Functions and Signatures](/03-functions/00-basic-functions/); recursive data structures get full treatment in [Section 10: Smart Pointers](/10-smart-pointers/).

---

## TypeScript/JavaScript Example

```typescript
// TypeScript - classic recursion: factorial and a recursive list sum.
function factorial(n: number): number {
  if (n <= 1) {
    return 1; // base case
  }
  return n * factorial(n - 1); // recursive case
}

function sumArray(numbers: number[]): number {
  if (numbers.length === 0) {
    return 0; // base case
  }
  const [first, ...rest] = numbers; // head + tail
  return first + sumArray(rest);
}

console.log(factorial(5)); // 120
console.log(sumArray([3, 1, 4, 1, 5, 9, 2, 6])); // 31

// Deep recursion blows the JS call stack, too:
function sumTo(n: number): number {
  return n === 0 ? 0 : n + sumTo(n - 1);
}
// sumTo(1_000_000); // RangeError: Maximum call stack size exceeded
```

**Key points:**

- A recursive function needs a **base case** or it loops forever (or until the stack runs out).
- JavaScript engines impose a call-stack limit; exceeding it throws `RangeError: Maximum call stack size exceeded`.
- The ES2015 spec defined tail-call optimization, but in practice **only Safari/JavaScriptCore ships it** — V8 (Node, Chrome) does not. So you cannot rely on it either.

---

## Rust Equivalent

```rust
// Recursive factorial with a base case.
fn factorial(n: u64) -> u64 {
    if n <= 1 {
        1 // base case
    } else {
        n * factorial(n - 1) // recursive case
    }
}

// Recursive sum over a slice using "head + recurse on the tail".
fn sum_slice(numbers: &[i64]) -> i64 {
    match numbers {
        [] => 0,                                       // base case: empty slice
        [first, rest @ ..] => first + sum_slice(rest), // head + tail
    }
}

fn main() {
    println!("5! = {}", factorial(5));
    println!("10! = {}", factorial(10));

    let data = [3, 1, 4, 1, 5, 9, 2, 6];
    println!("sum = {}", sum_slice(&data));
}
```

**Output (verified):**

```text
5! = 120
10! = 3628800
sum = 31
```

**Key points:**

- A recursive `fn` in Rust looks just like any other function — it simply calls itself.
- The slice pattern `[first, rest @ ..]` binds the first element to `first` and the **remaining slice** to `rest`, the idiomatic Rust analog of JavaScript's `const [first, ...rest] = numbers`. It does **not** allocate a new array; `rest` is a borrowed view into the same data.
- Just like V8, Rust has a finite stack and **no guaranteed tail-call optimization**. The next sections show exactly what that means.

---

## Detailed Explanation

### Anatomy of a recursive function

Every correct recursion has two parts:

1. A **base case** that returns without recursing. In `factorial`, that is `n <= 1 => 1`.
2. A **recursive case** that calls the function on a *smaller* input so the recursion makes progress toward the base case. In `factorial`, that is `n * factorial(n - 1)`.

If you forget the base case, or the recursive case never shrinks the input, the function recurses forever, which in Rust means it runs until the stack is exhausted (covered below), not an infinite hang.

### Slice patterns replace `[first, ...rest]`

In TypeScript, `const [first, ...rest] = numbers` copies the tail into a brand-new array on every call, so a recursive `sumArray` is quietly O(n²) in allocations. Rust's `[first, rest @ ..]` pattern binds `rest` to a **sub-slice** (a pointer + length) that borrows the original data with **zero copying**. The recursion is still linear in stack depth, but it allocates nothing.

> **Note:** Slice patterns and `match` are part of Rust's pattern-matching system, covered in depth in [Section 04: Control Flow](/04-control-flow/). Here we use them only to express "head and tail."

### The call stack is finite — and Rust does not hide it

Each function call pushes a **stack frame** holding its parameters and locals. When the function returns, the frame is popped. Recursion stacks up one frame per pending call, and the call stack has a fixed size — the main thread defaults to about **8 MiB** on most platforms. Pure tail-recursive functions *could* in principle reuse a single frame, but **Rust makes no guarantee that it will do so**, so you must assume every recursive call costs a frame.

```rust
// Deeply recursive sum 1..=n. Each call adds a frame to the call stack.
// There is no guaranteed tail-call optimization, so this overflows the
// stack for large n.
fn sum_to(n: u64) -> u64 {
    if n == 0 {
        0
    } else {
        n + sum_to(n - 1)
    }
}

fn main() {
    // A few hundred thousand frames is enough to blow the default 8 MiB stack.
    println!("{}", sum_to(1_000_000));
}
```

This compiles cleanly, but running it aborts the process. **Real output (verified):**

```text
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting
```

The process exits with a failure (code 134 / `SIGABRT` on this machine). This is the direct counterpart to V8's `RangeError: Maximum call stack size exceeded` — the difference is that JavaScript throws a *catchable error*, whereas a Rust stack overflow **aborts the whole process** and cannot be caught. That makes uncontrolled recursion depth more dangerous in Rust than in Node, and it is why production Rust code prefers iteration for anything that scales with input size.

### "Tail position" does not save you (no guaranteed TCO)

A call is in **tail position** if it is the very last thing the function does — its result is returned directly without any further work. The factorial above is *not* tail-recursive (it still has to multiply by `n` after the call returns), but an accumulator-passing version is:

```rust
// Accumulator style: still recursive, but the recursive call is in
// "tail position". Rust does NOT guarantee this is optimized away,
// so it can still overflow - shown here only to contrast the shape.
fn sum_to_acc(n: u64, acc: u64) -> u64 {
    if n == 0 {
        acc
    } else {
        sum_to_acc(n - 1, acc + n) // tail call: nothing happens after it returns
    }
}

fn main() {
    println!("sum_to_acc(100, 0) = {}", sum_to_acc(100, 0));
}
```

**Output (verified):**

```text
sum_to_acc(100, 0) = 5050
```

In a language with **guaranteed** tail-call optimization (such as Scheme, or Safari's JavaScript engine for proper tail calls), the compiler would rewrite this into a loop and it would handle any depth. Rust gives **no such guarantee**. The optimizer (LLVM) *may* turn a tail call into a jump at higher optimization levels, but this is an implementation detail you must never depend on. There is no `become` keyword in stable Rust to *require* it. Treat tail-position recursion exactly like any other recursion: bounded by stack size.

### Mutual recursion works too

Two functions can call each other. Because Rust resolves module items regardless of order (no hoisting needed — see [Basic Functions and Signatures](/03-functions/00-basic-functions/)), neither has to be declared "first."

```rust
// Mutual recursion: two functions that call each other.
fn is_even(n: u64) -> bool {
    if n == 0 {
        true
    } else {
        is_odd(n - 1)
    }
}

fn is_odd(n: u64) -> bool {
    if n == 0 {
        false
    } else {
        is_even(n - 1)
    }
}

fn main() {
    println!("is_even(10) = {}", is_even(10));
    println!("is_odd(7) = {}", is_odd(7));
}
```

**Output (verified):**

```text
is_even(10) = true
is_odd(7) = true
```

The same stack-depth caveat applies: mutual recursion consumes one frame per call, alternating between the two functions.

---

## Key Differences

| Concept                       | TypeScript/JavaScript (Node/V8)                          | Rust                                                              |
| ----------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------- |
| Recursion syntax              | Function calls itself                                    | Identical: function calls itself                                |
| Head/tail destructuring       | `const [first, ...rest]` (copies the tail)               | `[first, rest @ ..]` slice pattern (borrows, no copy)            |
| Stack-overflow behavior       | Throws `RangeError` (catchable with `try/catch`)         | **Aborts the process**; not catchable on the main thread        |
| Tail-call optimization        | Spec'd in ES2015, but V8/Node does **not** implement it  | **No guarantee**; no `become` keyword in stable Rust             |
| Recursive data types          | Just nest objects (`{ value, next }`)                    | Need indirection (`Box`, `Rc`) or the type has "infinite size"   |
| Default main-thread stack     | ~984 KiB in Node (V8), tunable via `--stack-size`        | ~8 MiB on most platforms; spawned threads are configurable       |
| Idiomatic preference          | Recursion or iteration, both common                      | Iteration strongly preferred for input-scaled depth             |

### Why iteration is the default in Rust

In JavaScript you often reach for recursion because it reads nicely and the engine's limit feels far away. In Rust, iterators (`for`, `while`, and the `Iterator` methods like `.fold()`, `.sum()`) are **zero-cost abstractions** that compile down to tight loops with no per-element stack growth. So the idiomatic Rust instinct is: *use an iterator unless the data itself is recursive (a tree) and shallow.* This is the same code, rewritten iteratively:

```rust
// Iterative factorial: a loop instead of recursion - constant stack usage.
fn factorial_iter(n: u64) -> u64 {
    let mut result = 1u64;
    for i in 2..=n {
        result *= i;
    }
    result
}

// Iterative sum 1..=n - no stack growth, handles huge n with one line.
fn sum_to_iter(n: u64) -> u64 {
    (1..=n).sum()
}

fn main() {
    println!("10! = {}", factorial_iter(10));
    println!("sum 1..=1_000_000 = {}", sum_to_iter(1_000_000));
}
```

**Output (verified):**

```text
10! = 3628800
sum 1..=1_000_000 = 500000500000
```

Note that `sum_to_iter(1_000_000)` runs fine, while the recursive `sum_to(1_000_000)` aborted with a stack overflow. Same result, radically different runtime safety. (Iterators and their combinators are covered in [Higher-Order Functions](/03-functions/04-higher-order/) and [Section 07: Collections](/07-collections/).)

---

## Common Pitfalls

### Pitfall 1: A recursive `enum` without indirection won't compile

This is unique to Rust and surprises every TypeScript developer. In TypeScript, a linked-list node is just `{ value: number; next: Node | null }`; objects are heap references, so the size is always "one pointer." In Rust, a value's size must be known at compile time, and a type that *contains itself by value* would be infinitely large.

```rust
// A recursive enum WITHOUT a Box - this does NOT compile.
enum List {
    Cons(i32, List),
    Nil,
}

fn main() {
    let _ = List::Nil;
}
```

**Real compiler error:**

```text
error[E0072]: recursive type `List` has infinite size
 --> src/main.rs:2:1
  |
2 | enum List {
  | ^^^^^^^^^
3 |     Cons(i32, List),
  |               ---- recursive without indirection
  |
help: insert some indirection (e.g., a `Box`, `Rc`, or `&`) to break the cycle
  |
3 |     Cons(i32, Box<List>),
  |               ++++    +
```

The compiler tells you exactly how to fix it: wrap the recursive field in a `Box` (a heap pointer of known size). See the [recursive enum teaser](#a-recursive-enum-teaser-box) below.

### Pitfall 2: A missing or unreachable base case → stack overflow at runtime

This compiles fine — the bug only shows up when you run it.

```rust
// BUG: the recursive call never shrinks toward a base case.
fn count_down_forever(n: u32) {
    println!("{n}");
    count_down_forever(n - 1); // u32 wraps below 0 in release, panics in debug
}
```

In **debug builds** the `n - 1` panics with `attempt to subtract with overflow` once `n` hits `0`. In **release builds** the subtraction wraps `0` to `u32::MAX` and the recursion runs effectively forever until it aborts with `thread 'main' has overflowed its stack`. Either way, the fix is a real base case: `if n == 0 { return; }`.

> **Warning:** A Rust stack overflow **aborts the process** and cannot be recovered with error handling. Unlike a Node `RangeError` you can `catch`, there is no safety net — design recursion so its depth is bounded by something small (like tree height), not by raw input size.

### Pitfall 3: Assuming tail recursion is free

Coming from a functional background (or Safari's engine), you might write a tail-recursive loop expecting it to run in constant stack space. Rust does not promise this:

```rust
// Looks tail-recursive, but Rust gives NO guarantee it becomes a loop.
fn sum_to_acc(n: u64, acc: u64) -> u64 {
    if n == 0 { acc } else { sum_to_acc(n - 1, acc + n) }
}
// sum_to_acc(10_000_000, 0) may still overflow the stack.
```

There is no stable `become` keyword to force a tail call. If you need guaranteed constant stack usage, **write a loop** — do not rely on the optimizer.

### Pitfall 4: Exponential recursion (the naive Fibonacci trap)

This is not Rust-specific, but it bites hard because Rust is fast enough that you might not notice the algorithmic blowup until the input grows:

```rust
// O(2^n): recomputes the same values exponentially many times.
fn fib_rec(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fib_rec(n - 1) + fib_rec(n - 2),
    }
}
```

`fib_rec(50)` recomputes lower Fibonacci numbers billions of times. The recursion *depth* is fine (only ~50 frames), but the *number of calls* explodes. The fix is the same as in TypeScript: memoize, or rewrite iteratively (see [Best Practices](#best-practices) and Exercise 3).

---

## Best Practices

### 1. Prefer iteration when depth scales with input

If your recursion depth grows with the size of a list, a number, or any unbounded input, write a loop or use an iterator. Reserve recursion for cases where the **data structure itself** is recursive and **shallow** (a parse tree, a small directory tree, JSON).

```rust
// Iterative: O(1) stack, handles any n.
fn fib_iter(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let next = a + b;
        a = b;
        b = next;
    }
    a
}
```

### 2. Recurse on bounded structures, iterate over sequences

Walking a binary tree recursively is idiomatic and safe — the depth is the tree's height (often `log n`). Summing a million-element list recursively is not — its depth is the list length. Match the technique to the shape of the data.

### 3. Reach for `Box` (or `Rc`) for recursive types

A self-referential `enum` or `struct` needs heap indirection so its size is finite. `Box<T>` is the simplest choice; `Rc<T>` when you need shared ownership. This is introduced below and detailed in [Section 10: Smart Pointers](/10-smart-pointers/).

### 4. Bound recursion explicitly when accepting untrusted input

If recursion depth can be influenced by external input (parsing user-supplied JSON, deserializing nested data), add an explicit `depth` parameter and return an error past a limit, rather than risking an unrecoverable stack overflow. This is a security-relevant pattern; see [Section 27: Security](/27-security/).

### 5. Memoize or tabulate exponential recursions

For overlapping-subproblem recursions like Fibonacci, cache results in a `HashMap` (memoization) or build a table bottom-up (tabulation / iteration). The iterative `fib_iter` above is tabulation taken to its limit.

---

## Real-World Example

Recursion shines on genuinely recursive data. Here is a directory-tree size calculator, the kind of traversal a build tool, bundler, or `du`-style utility performs. The tree is a recursive `enum` (a directory owns its children), and both the traversal functions are naturally recursive. Importantly, the recursion depth equals the **tree's depth**, which is small and bounded, so this is exactly where recursion belongs.

```rust
// Real-world example: compute the total size of a directory tree.
// The tree is a recursive data structure (a node owns its children),
// and the traversal is naturally recursive.

/// A node in a filesystem tree.
enum Node {
    /// A file with a size in bytes.
    File { name: String, size: u64 },
    /// A directory containing child nodes.
    Dir { name: String, children: Vec<Node> },
}

/// Recursively sums the byte size of a node and everything under it.
fn total_size(node: &Node) -> u64 {
    match node {
        Node::File { size, .. } => *size,
        Node::Dir { children, .. } => children.iter().map(total_size).sum(),
    }
}

/// Recursively prints the tree with indentation showing depth.
fn print_tree(node: &Node, depth: usize) {
    let indent = "  ".repeat(depth);
    match node {
        Node::File { name, size } => println!("{indent}{name} ({size} bytes)"),
        Node::Dir { name, children } => {
            println!("{indent}{name}/");
            for child in children {
                print_tree(child, depth + 1);
            }
        }
    }
}

fn main() {
    let tree = Node::Dir {
        name: "src".to_string(),
        children: vec![
            Node::File { name: "main.rs".to_string(), size: 1200 },
            Node::Dir {
                name: "utils".to_string(),
                children: vec![
                    Node::File { name: "parse.rs".to_string(), size: 3400 },
                    Node::File { name: "format.rs".to_string(), size: 900 },
                ],
            },
        ],
    };

    print_tree(&tree, 0);
    println!("total size: {} bytes", total_size(&tree));
}
```

**Output (verified):**

```text
src/
  main.rs (1200 bytes)
  utils/
    parse.rs (3400 bytes)
    format.rs (900 bytes)
total size: 5500 bytes
```

**What to notice:**

- `Node` is a recursive type, but `children: Vec<Node>` already provides the heap indirection (`Vec` stores its elements behind a pointer), so no explicit `Box` is needed here. (You only need `Box` when a variant contains the type *directly*, as in the `Cons` list below.)
- `total_size` recurses on each child; `children.iter().map(total_size).sum()` is the idiomatic combination of iteration *over siblings* and recursion *into depth*.
- `print_tree` passes a `depth` parameter, a common pattern for tracking how deep you are, useful both for formatting and for enforcing a depth limit on untrusted input.
- Because the depth is the directory nesting level (typically a handful), there is no stack-overflow risk. This is the right shape for recursion.

### A recursive `enum` teaser (`Box`)

When a type contains *itself directly* (a classic cons list) you need explicit indirection. `Box<T>` is a heap pointer with a fixed size (one machine word), which breaks the "infinite size" cycle from [Pitfall 1](#pitfall-1-a-recursive-enum-without-indirection-wont-compile).

```rust
// A recursive enum: a cons-style linked list.
// `Box<List>` gives the variant a known, finite size (a pointer).
#[derive(Debug)]
enum List {
    Cons(i32, Box<List>),
    Nil,
}

use List::{Cons, Nil};

fn sum_list(list: &List) -> i32 {
    match list {
        Cons(value, rest) => value + sum_list(rest),
        Nil => 0,
    }
}

fn main() {
    // 1 -> 2 -> 3 -> Nil
    let list = Cons(1, Box::new(Cons(2, Box::new(Cons(3, Box::new(Nil))))));
    println!("list = {list:?}");
    println!("sum = {}", sum_list(&list));
}
```

**Output (verified):**

```text
list = Cons(1, Cons(2, Cons(3, Nil)))
sum = 6
```

This is only a teaser. `Box`, `Rc`, and recursive data structures get full coverage in [Section 10: Smart Pointers](/10-smart-pointers/). The takeaway for now: **recursive *functions* need a base case; recursive *types* need indirection.**

---

## Further Reading

### Official Documentation

- [The Rust Book – Recursive Types with Box](https://doc.rust-lang.org/book/ch15-01-box.html#enabling-recursive-types-with-boxes)
- [The Rust Book – Defining an Enum](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html)
- [Rust by Example – Functions](https://doc.rust-lang.org/rust-by-example/fn.html)
- [Rust Reference – Recursive type errors (E0072)](https://doc.rust-lang.org/error_codes/E0072.html)
- [Rust Reference – Slice patterns](https://doc.rust-lang.org/reference/patterns.html#slice-patterns)

### Related Sections in This Guide

- [Basic Functions](/03-functions/00-basic-functions/) — function signatures, tail expressions, and why items need no hoisting.
- [Parameters](/03-functions/01-parameters/) — accumulator-style parameters and slices `&[T]` as inputs.
- [Return Values](/03-functions/02-return-values/): early `return` for base cases, and returning the unit type.
- [Higher-Order Functions](/03-functions/04-higher-order/) — iterator combinators (`map`, `fold`, `sum`) that often replace recursion.
- [Function Pointers](/03-functions/05-function-pointers/) — passing a function (such as a recursive one) by name.
- [Section 04: Control Flow](/04-control-flow/) — `match`, slice patterns, and `loop`/`while` for the iterative alternatives.
- [Section 07: Collections](/07-collections/): `Vec`, `HashMap` (for memoization), and iterators over sequences.
- [Section 10: Smart Pointers](/10-smart-pointers/) — `Box`, `Rc`, and recursive data structures in full.
- [Section 27: Security](/27-security/): bounding recursion depth on untrusted input.

---

## Exercises

### Exercise 1: Recursive countdown

**Difficulty:** Easy

**Objective:** Write a recursive function with a clear base case that performs a side effect.

**Instructions:**

1. Write `fn countdown(n: u32)` that prints each number from `n` down to `1`, then prints `"liftoff!"`.
2. The base case is `n == 0` (print `"liftoff!"` and stop).
3. The recursive case prints `n` and calls `countdown(n - 1)`.
4. Call `countdown(3)` from `main`.

<details>
<summary>Solution</summary>

```rust
fn countdown(n: u32) {
    if n == 0 {
        println!("liftoff!"); // base case
    } else {
        println!("{n}");
        countdown(n - 1); // recursive case
    }
}

fn main() {
    countdown(3);
}
```

**Verified output:**

```text
3
2
1
liftoff!
```

This is safe because the depth (`n`) is tiny. If `n` could be in the millions, you would write a `for` loop instead to avoid a stack overflow.

</details>

### Exercise 2: Greatest common divisor (Euclid's algorithm)

**Difficulty:** Medium

**Objective:** Implement a naturally recursive numeric algorithm and return a value via a tail expression.

**Instructions:**

1. Write `fn gcd(a: u64, b: u64) -> u64`.
2. The base case: when `b == 0`, return `a`.
3. The recursive case: return `gcd(b, a % b)`.
4. Test it with `gcd(48, 18)` (should be `6`).

> **Tip:** This recursion is shallow (its depth grows only logarithmically) so recursion is perfectly safe here. Note that the recursive call is in tail position, but remember Rust does not *guarantee* that helps with stack usage; it simply does not matter at this depth.

<details>
<summary>Solution</summary>

```rust
fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a // base case
    } else {
        gcd(b, a % b) // recursive case
    }
}

fn main() {
    println!("gcd(48, 18) = {}", gcd(48, 18));
}
```

**Verified output:**

```text
gcd(48, 18) = 6
```

</details>

### Exercise 3: Fibonacci, recursive vs. iterative

**Difficulty:** Medium

**Objective:** Feel the difference between an exponential recursion and an efficient loop, and understand why Rust developers reach for iteration.

**Instructions:**

1. Write `fn fib_rec(n: u32) -> u64` using the naive recursive definition (`fib(0) = 0`, `fib(1) = 1`, otherwise `fib(n-1) + fib(n-2)`).
2. Write `fn fib_iter(n: u32) -> u64` that computes the same value with a single loop and two accumulators — O(n) time, O(1) stack.
3. Print `fib_rec(10)`, `fib_iter(10)` (both `55`), and `fib_iter(90)`.
4. Reflect: why would calling `fib_rec(90)` be a terrible idea even though `fib_iter(90)` is instant? (The recursive version makes an exponential number of calls.)

<details>
<summary>Solution</summary>

```rust
// O(2^n): recomputes the same subproblems exponentially many times.
fn fib_rec(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fib_rec(n - 1) + fib_rec(n - 2),
    }
}

// O(n) time, O(1) stack: tabulate with two rolling accumulators.
fn fib_iter(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let next = a + b;
        a = b;
        b = next;
    }
    a
}

fn main() {
    println!("fib_rec(10) = {}", fib_rec(10));
    println!("fib_iter(10) = {}", fib_iter(10));
    println!("fib_iter(90) = {}", fib_iter(90));
}
```

**Verified output:**

```text
fib_rec(10) = 55
fib_iter(10) = 55
fib_iter(90) = 2880067194370816120
```

`fib_rec`'s recursion *depth* is only about `n`, so it will not overflow the stack at `n = 90`, but its *number of calls* roughly doubles with each step, so `fib_rec(90)` would take longer than the age of the universe. The iterative version computes it instantly. This is the classic illustration of why, in Rust, iteration (or memoization) is the default for anything with overlapping subproblems.

</details>

---

## Summary

**What you've learned:**

- A recursive function needs a **base case** and a **recursive case** that shrinks toward it — identical in concept to TypeScript.
- Rust has a finite call stack and makes **no guarantee of tail-call optimization**; deep recursion **aborts the process** with a stack overflow that you cannot catch.
- Slice patterns (`[first, rest @ ..]`) express "head and tail" by **borrowing**, not copying like JavaScript's `[first, ...rest]`.
- The idiomatic Rust default is **iteration** (loops and iterators) whenever recursion depth scales with input; reserve recursion for shallow, naturally recursive data like trees.
- Recursive *data types* (a cons list) need heap indirection (`Box<T>`) or the compiler rejects them with `error[E0072]: recursive type has infinite size`.
