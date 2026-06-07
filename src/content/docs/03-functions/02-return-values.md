---
title: "Return Values"
description: "Rust functions return their final expression with no return keyword, use the unit type instead of void, and hand back multiple values as tuples, unlike TypeScript."
---

How a function hands a result back to its caller looks superficially similar in Rust and TypeScript, but the mechanics are different. Rust functions return the value of their final **expression** (usually with no `return` keyword at all), and a function that returns "nothing" actually returns a real value called the **unit type**.

---

## Quick Overview

In Rust, the last expression in a function body is its return value (no `return`, no semicolon). The `return` keyword exists but is reserved for **early returns**. Functions that produce no meaningful value return `()` (the **unit type**), and when you need to hand back several values at once you return a **tuple** rather than mutating out-parameters.

**In short:** A semicolon turns an expression into a statement that evaluates to `()`. Whether your function returns a useful value or `()` often comes down to whether that last semicolon is present.

---

## TypeScript/JavaScript Example

```typescript
// Explicit `return` on every path is the norm in TS/JS.
function square(n: number): number {
  return n * n;
}

// A "void" function still returns something: `undefined`.
function logMessage(msg: string): void {
  console.log(`[log] ${msg}`);
}

// Early return (guard clause) to avoid deep nesting.
function classify(score: number): string {
  if (score < 0) return "invalid";
  if (score >= 90) return "excellent";
  if (score >= 60) return "passing";
  return "failing";
}

// Returning "multiple values" means returning an array or an object,
// then destructuring at the call site.
function minMax(values: number[]): [number, number] {
  return [Math.min(...values), Math.max(...values)];
}

const [lo, hi] = minMax([7, 2, 9, 4, 1]);
console.log(lo, hi); // 1 9

const result = logMessage("hi");
console.log(result); // undefined  <-- void really means `undefined`
```

**Key points:**

- Every value-producing path needs an explicit `return`.
- A `void` function implicitly returns `undefined`.
- "Multiple return values" are an array (positional) or object (named) plus destructuring.

---

## Rust Equivalent

```rust
// The last expression IS the return value: no `return`, no semicolon.
fn square(n: i32) -> i32 {
    n * n
}

// No `-> Type` means the function returns `()`, the unit type.
fn log_message(msg: &str) {
    println!("[log] {msg}");
}

// `return` is kept for EARLY returns (guard clauses).
fn classify(score: i32) -> &'static str {
    if score < 0 {
        return "invalid";
    }
    if score >= 90 {
        return "excellent";
    }
    if score >= 60 {
        return "passing";
    }
    "failing" // final value: tail expression, no semicolon
}

// Multiple values come back as a tuple.
fn min_max(values: &[i32]) -> (i32, i32) {
    let mut min = values[0];
    let mut max = values[0];
    for &v in &values[1..] {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    (min, max)
}

fn main() {
    println!("{}", square(5)); // 25
    log_message("hi");

    let (lo, hi) = min_max(&[7, 2, 9, 4, 1]);
    println!("min={lo}, max={hi}"); // min=1, max=9
}
```

Running the full version of this program prints:

```
25
[log] hi
min=1, max=9
```

**Key points:**

- The **tail expression** (last expression, no semicolon) is the return value.
- Omitting `-> Type` is the same as `-> ()`.
- `return` is for leaving early, not for the normal final result.
- A **tuple** is the idiomatic way to return several values.

---

## Detailed Explanation

### The tail expression is the return value

Rust is an **expression-oriented** language. A function body is a block, and a block evaluates to its final expression, provided that expression has no trailing semicolon.

```rust
fn square(n: i32) -> i32 {
    n * n // tail expression: this is what the function returns
}
```

The `-> i32` after the parameter list is the **return type annotation** (TypeScript writes it after a colon: `(n: number): number`). The body's last expression must produce that type.

You *can* write `return`, and it compiles:

```rust
fn square_explicit(n: i32) -> i32 {
    return n * n; // legal, but not idiomatic
}
```

But in idiomatic Rust the `return` keyword is reserved for leaving a function *before* the end. Using it on the last line is so unidiomatic that Clippy flags it (see [Common Pitfalls](#common-pitfalls)).

> **Tip:** "No semicolon means return" is the single most useful rule to internalize from this page. If you find yourself writing `return x;` as the very last line, delete the `return` and the `;`.

### Statements vs. expressions: the role of the semicolon

This is where TypeScript intuition can mislead you. In Rust:

- An **expression** evaluates to a value (`n * n`, `if c { a } else { b }`, a block `{ ... }`).
- A **statement** performs an action and evaluates to `()` (a `let` binding, or *any expression followed by a semicolon*).

Adding a semicolon to the tail expression discards its value and substitutes `()`:

```rust
fn square(n: i32) -> i32 {
    n * n; // BUG: now this is a statement; the function "falls off the end" with ()
}
```

That mismatch (`()` where `i32` is expected) is a compile error, covered below. The fix is to remove the semicolon.

> **Note:** This statement/expression distinction was introduced in [Section 02 — Basics](/02-basics/). Functions are where it bites hardest, so it is worth re-reading if it feels shaky. See also [Basic Functions and Signatures](/03-functions/00-basic-functions/) for the statement-vs-expression fundamentals.

### `if` (and `match`, and blocks) are expressions

Because `if` is an expression, you can use it directly as the tail expression; no need for a temporary mutable variable:

```rust
fn abs_diff(a: i32, b: i32) -> i32 {
    if a > b {
        a - b
    } else {
        b - a
    }
}
```

Every branch must produce the same type, and there must be an `else` (otherwise the value of the `if` would be `()` when the condition is false). A plain block is also an expression:

```rust
fn block_expr_demo() -> i32 {
    let y = {
        let a = 3;
        let b = 4;
        a * a + b * b // tail expression of the inner block -> 25
    };
    y
}
```

Even `loop` is an expression: `break value` makes the whole `loop` evaluate to `value`.

```rust
fn first_power_over(limit: u32) -> u32 {
    let mut n = 1;
    loop {
        n *= 2;
        if n > limit {
            break n; // the value of the whole `loop` expression
        }
    }
}
```

> Control-flow-as-expression is explored fully in [Section 04 — Control Flow](/04-control-flow/).

### The unit type `()`

When a function has no `-> Type`, its return type is `()`, pronounced "unit". Unit is a real type with exactly one value, also written `()`. These two signatures are identical:

```rust
fn log_message(msg: &str) {
    println!("[log] {msg}");
}

// Exactly the same thing, written explicitly:
fn log_message_explicit(msg: &str) -> () {
    println!("[log] {msg}");
}
```

`()` is the closest Rust analogue to TypeScript's `void`, but they differ in an important way:

- TypeScript's `void` is a *type-system marker*; at runtime such functions return the value `undefined`.
- Rust's `()` is a genuine zero-size value. It has no runtime representation (it takes up zero bytes), yet you can store it, pass it, and pattern-match on it like any other value.

You almost never write `-> ()` explicitly; idiomatic Rust just omits the return type.

### Early return with `return`

Guard clauses translate directly. Use `return value;` (with a semicolon — it is a statement) to leave early, and let the final value be a tail expression:

```rust
fn classify(score: i32) -> &'static str {
    if score < 0 {
        return "invalid"; // early return
    }
    if score >= 90 {
        return "excellent";
    }
    if score >= 60 {
        return "passing";
    }
    "failing" // tail expression for the remaining case
}
```

Note the asymmetry that trips people up: early `return` statements end with a semicolon, but the final tail expression does **not**. They are different syntactic roles, not an inconsistency.

### Returning multiple values: tuples

JavaScript reaches for an array (`[a, b]`) or object (`{ a, b }`); Rust returns a **tuple**, then you destructure it:

```rust
fn min_max(values: &[i32]) -> (i32, i32) {
    // ...
    (min, max)
}

// Destructure at the call site:
let (lo, hi) = min_max(&[7, 2, 9, 4, 1]);

// Or access positionally if you didn't destructure:
let pair = min_max(&[7, 2, 9, 4, 1]);
println!("{} {}", pair.0, pair.1);
```

Tuple fields are accessed by position (`.0`, `.1`, ...), and the type `(i32, i32)` is checked at compile time: there is no risk of reading `pair[2]` and getting `undefined`. Tuples are statically sized and typed, unlike a JS array.

Once you have more than two or three fields, or the positions stop being self-explanatory, prefer a named **struct**, the Rust analogue of returning an object with named keys:

```rust
struct Stats {
    min: i32,
    max: i32,
    sum: i32,
}

fn stats(values: &[i32]) -> Stats {
    let mut s = Stats { min: values[0], max: values[0], sum: 0 };
    for &v in values {
        if v < s.min { s.min = v; }
        if v > s.max { s.max = v; }
        s.sum += v;
    }
    s
}
```

(Structs get full treatment in [Section 06 — Data Structures](/06-data-structures/).)

### Bonus: the never type `!`

A function that *never returns normally* (it loops forever or always panics) has the special return type `!`, the **never type**. `!` coerces to any type, which is why a panicking branch can sit alongside an `i32` branch:

```rust
fn bail(msg: &str) -> ! {
    panic!("fatal: {msg}");
}

fn get_config(present: bool) -> i32 {
    if present {
        42
    } else {
        bail("config missing"); // type `!` coerces to i32
    }
}
```

TypeScript has a direct counterpart: the `never` type, used for functions that always throw or loop forever.

---

## Key Differences

| Concept                       | TypeScript/JavaScript                         | Rust                                                    |
| ----------------------------- | --------------------------------------------- | ------------------------------------------------------- |
| Normal return                 | `return value;` on every path                 | Tail expression (no `return`, no `;`)                   |
| `return` keyword              | The only way to return a value                | Reserved for **early** returns                          |
| "No value" function           | `void` → returns `undefined` at runtime       | `()` (unit), a zero-size real value                     |
| Return type position          | After params: `(x: number): number`           | After params with arrow: `(x: i32) -> i32`              |
| Multiple values               | Array `[a, b]` or object `{ a, b }`           | Tuple `(a, b)` or a named `struct`                      |
| Index access on returned pair | `arr[0]`, may be `undefined` if out of bounds | `tuple.0`, checked at compile time                      |
| Block produces a value        | No (needs IIFE or explicit `return`)          | Yes — a block is an expression                          |
| Never-returns marker          | `never`                                       | `!` (the never type)                                    |

### Why does Rust make the last expression the return value?

It is a consequence of being expression-oriented. Once `if`, `match`, blocks, and `loop` all produce values, "the function returns its body's value" falls out naturally and removes a whole category of mistakes (forgetting `return` on one branch). TypeScript can flag missing returns with `noImplicitReturns`, but it remains an opt-in lint; in Rust the type system enforces it structurally: every path must produce the declared type or it does not compile.

---

## Common Pitfalls

### Pitfall 1: Accidental semicolon on the tail expression

**Problem:**

```rust
fn square(n: i32) -> i32 {
    n * n; // BUG: trailing semicolon makes this a statement, returns ()
}
```

**Real compiler error:**

```
error[E0308]: mismatched types
 --> src/main.rs:1:22
  |
1 | fn square(n: i32) -> i32 {
  |    ------            ^^^ expected `i32`, found `()`
  |    |
  |    implicitly returns `()` as its body has no tail or `return` expression
2 |     n * n; // BUG: trailing semicolon makes this a statement, not the return value
  |          - help: remove this semicolon to return this value

For more information about this error, try `rustc --explain E0308`.
```

**Solution:** Remove the semicolon. The compiler even points at it (`help: remove this semicolon to return this value`).

```rust
fn square(n: i32) -> i32 {
    n * n
}
```

### Pitfall 2: Forgetting the return type annotation

Rust does **not** infer the return type of a top-level function from its body (unlike closures, and unlike TypeScript inferring `: number`). If the body produces a value, you must declare the type.

**Problem:**

```rust
fn add(a: i32, b: i32) {
    a + b
}
```

**Real compiler error:**

```
error[E0308]: mismatched types
 --> src/main.rs:3:5
  |
2 | fn add(a: i32, b: i32) {
  |                       - help: try adding a return type: `-> i32`
3 |     a + b
  |     ^^^^^ expected `()`, found `i32`

For more information about this error, try `rustc --explain E0308`.
```

The compiler assumed `-> ()` because none was given, then found an `i32`. **Solution:** add `-> i32`.

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

### Pitfall 3: Mismatched `if`/`else` branch types

Because `if` is an expression used as the return value, every branch must yield the **same** type. TypeScript would happily widen to a union (`number | string`); Rust will not.

**Problem:**

```rust
fn describe(n: i32) -> i32 {
    if n > 0 {
        n
    } else {
        "negative or zero" // wrong type!
    }
}
```

**Real compiler error:**

```
error[E0308]: mismatched types
 --> src/main.rs:6:9
  |
2 | fn describe(n: i32) -> i32 {
  |                        --- expected `i32` because of return type
...
6 |         "negative or zero" // wrong type!
  |         ^^^^^^^^^^^^^^^^^^ expected `i32`, found `&str`

For more information about this error, try `rustc --explain E0308`.
```

**Solution:** make both arms the same type (e.g. return a `&str` describing the number, or an `enum`). If you genuinely need "either a number or a message," model it with an `enum` or `Result`; see [Section 08 — Error Handling](/08-error-handling/).

### Pitfall 4: Writing `return` as the last line

It compiles and runs, but it is not idiomatic, and Clippy will tell you so.

**Code:**

```rust
fn square(n: i32) -> i32 {
    return n * n;
}
```

**Real `cargo clippy` warning:**

```
warning: unneeded `return` statement
 --> src/main.rs:2:5
  |
2 |     return n * n; // clippy will flag this needless return
  |     ^^^^^^^^^^^^
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_return
  = note: `#[warn(clippy::needless_return)]` on by default
help: remove `return`
```

**Solution:** drop the `return` and the trailing semicolon, leaving the bare tail expression `n * n`.

### Pitfall 5: Expecting `void`/`()` to behave like `undefined`

In TypeScript you might write `const r = doThing();` and then check `if (r === undefined)`. In Rust, a function returning `()` gives you a value you cannot meaningfully test; there is exactly one `()`. Trying to print or compare it is almost always a sign you wanted a real return type (often `Option<T>` or `Result<T, E>`). If a function "might not produce a value," return `Option<T>`, not `()`.

---

## Best Practices

### Prefer tail expressions over `return`

```rust
// Idiomatic
fn fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

// Not idiomatic (Clippy: needless_return)
fn fahrenheit_verbose(c: f64) -> f64 {
    return c * 9.0 / 5.0 + 32.0;
}
```

### Use `return` for genuine early exits / guard clauses

Guard clauses keep the happy path un-nested and are the canonical reason to reach for `return`:

```rust
fn paginate(total_items: u32, page: u32, per_page: u32) -> (u32, u32) {
    if per_page == 0 {
        return (0, 0); // guard clause
    }
    let offset = page.saturating_sub(1) * per_page;
    (offset, per_page)
}
```

### Return tuples for 2-3 unnamed values; use a struct beyond that

A `(width, height)` tuple is clear. A `(u32, u32, u32, bool, String)` tuple is not: give the fields names with a struct. When absence is possible, return `Option<(A, B)>` rather than a sentinel value like `(-1, -1)`.

### Let the type system enforce totality

Don't disable return checking. Every path producing the declared type is a feature: it is impossible to forget a branch, the way you can in JavaScript without `noImplicitReturns`.

### Don't annotate `-> ()`

Omit the return type for unit-returning functions; `fn f() {}` is preferred over `fn f() -> () {}`.

---

## Real-World Example

A small slice of an API layer: computing pagination metadata (early returns + a tuple result) and parsing a `key=value` line (early return + `Option` of a tuple).

```rust
/// Compute pagination metadata for a list endpoint.
///
/// Returns `(offset, limit, total_pages)`. Uses early returns to clamp
/// invalid input instead of nesting the happy path inside `if` blocks.
fn paginate(total_items: u32, page: u32, per_page: u32) -> (u32, u32, u32) {
    // Guard clause: a zero page size has no sensible offset/limit.
    if per_page == 0 {
        return (0, 0, 0);
    }

    // Pages are 1-based for the caller; treat 0 as "first page".
    let page = if page == 0 { 1 } else { page };

    let total_pages = total_items.div_ceil(per_page);
    let offset = (page - 1) * per_page;
    let limit = per_page;

    (offset, limit, total_pages)
}

/// Parse a `key=value` configuration line.
///
/// Returns `None` for malformed input — the absence is modeled with
/// `Option`, not a sentinel string. The `?` propagates the `None` from
/// `split_once` as an early return.
fn parse_kv(input: &str) -> Option<(String, String)> {
    let (key, value) = input.split_once('=')?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() {
        return None; // explicit early return for the empty-key case
    }
    Some((key.to_string(), value.to_string()))
}

fn main() {
    let (offset, limit, pages) = paginate(95, 3, 20);
    println!("offset={offset}, limit={limit}, total_pages={pages}");

    let (offset, limit, pages) = paginate(95, 0, 0);
    println!("offset={offset}, limit={limit}, total_pages={pages}");

    println!("{:?}", parse_kv("  host = localhost  "));
    println!("{:?}", parse_kv("=oops"));
    println!("{:?}", parse_kv("no-equals"));
}
```

**Output:**

```
offset=40, limit=20, total_pages=5
offset=0, limit=0, total_pages=0
Some(("host", "localhost"))
None
None
```

Notice three return-value techniques working together: a **tuple** for `(offset, limit, total_pages)`, **early returns** for guard clauses, and `?` — which is itself an early return that yields `None` from a function returning `Option`. The `?` operator is covered in depth in [Section 08 — Error Handling](/08-error-handling/).

> **Note:** `u32::div_ceil` (ceiling division) is a stable standard-library method, handy for computing page counts without floating point.

---

## Further Reading

### Official Documentation

- [The Rust Book — Functions](https://doc.rust-lang.org/book/ch03-03-how-functions-work.html): statements, expressions, and return values
- [Rust Reference — Functions](https://doc.rust-lang.org/reference/items/functions.html): the formal definition of function items and return types
- [Rust Reference — The `()` type](https://doc.rust-lang.org/std/primitive.unit.html): the unit type
- [Rust Reference — The `!` type](https://doc.rust-lang.org/std/primitive.never.html): the never type
- [Clippy: `needless_return`](https://rust-lang.github.io/rust-clippy/master/index.html#needless_return): why `return` on the last line is flagged

### Related Sections in This Guide

- [Basic Functions](/03-functions/00-basic-functions/): `fn` signatures, statements vs. expressions
- [Parameters](/03-functions/01-parameters/): passing data in (the counterpart to this page)
- [Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/): closures *do* infer their return type
- [Higher-Order Functions](/03-functions/04-higher-order/): returning closures with `impl Fn` / `Box<dyn Fn>`
- [Recursion](/03-functions/06-recursion/): returning from recursive calls
- [Section 02 — Basics](/02-basics/) — the expression/statement distinction
- [Section 04 — Control Flow](/04-control-flow/) — `if`/`match`/`loop` as expressions
- [Section 06 — Data Structures](/06-data-structures/) — structs for named multi-value returns
- [Section 08 — Error Handling](/08-error-handling/) — `Option`/`Result` and the `?` operator

---

## Exercises

### Exercise 1: From explicit `return` to tail expression

**Difficulty:** Beginner

**Objective:** Internalize that the last expression is the return value.

**Instructions:** Rewrite this function in idiomatic Rust style (no `return`, no trailing semicolon on the result). It converts Celsius to Fahrenheit.

```rust
fn celsius_to_fahrenheit(c: f64) -> f64 {
    return c * 9.0 / 5.0 + 32.0;
}

fn main() {
    println!("{}", celsius_to_fahrenheit(100.0)); // 212
}
```

<details>
<summary>Solution</summary>

```rust
fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

fn main() {
    println!("{}", celsius_to_fahrenheit(100.0)); // 212
}
```

Running it prints `212`. Dropping both the `return` keyword and the trailing semicolon leaves a bare tail expression, which is what the function returns.

</details>

### Exercise 2: Return multiple values with a tuple

**Difficulty:** Intermediate

**Objective:** Return more than one value and destructure it at the call site.

**Instructions:** Implement `divmod`, which returns both the integer quotient and the remainder of `a / b` as a tuple `(quotient, remainder)`. Destructure the result in `main` and print both parts.

```rust
fn divmod(a: i32, b: i32) -> (i32, i32) {
    // TODO: return (quotient, remainder)
}

fn main() {
    let (q, r) = divmod(17, 5);
    println!("q={q}, r={r}"); // q=3, r=2
}
```

<details>
<summary>Solution</summary>

```rust
fn divmod(a: i32, b: i32) -> (i32, i32) {
    (a / b, a % b) // tuple tail expression
}

fn main() {
    let (q, r) = divmod(17, 5);
    println!("q={q}, r={r}"); // q=3, r=2
}
```

Running it prints `q=3, r=2`. The tuple `(a / b, a % b)` is the function's value, and `let (q, r) = ...` destructures it.

</details>

### Exercise 3: Early return + `Option` of a tuple

**Difficulty:** Advanced

**Objective:** Combine guard clauses, the `?` operator, and a tuple return modeled with `Option`.

**Instructions:** Implement `parse_kv` so it parses a `key=value` string into `Some((key, value))` with both parts trimmed. Return `None` if there is no `=` **or** if the key is empty after trimming. Use `str::split_once` and the `?` operator.

```rust
fn parse_kv(input: &str) -> Option<(String, String)> {
    // TODO: split on '=', trim both sides, reject empty keys
}

fn main() {
    println!("{:?}", parse_kv("  host = localhost  ")); // Some(("host", "localhost"))
    println!("{:?}", parse_kv("=oops"));                // None
    println!("{:?}", parse_kv("no-equals"));            // None
}
```

<details>
<summary>Solution</summary>

```rust
fn parse_kv(input: &str) -> Option<(String, String)> {
    let (key, value) = input.split_once('=')?; // `?` returns None early if no '='
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() {
        return None; // explicit early return
    }
    Some((key.to_string(), value.to_string()))
}

fn main() {
    println!("{:?}", parse_kv("  host = localhost  "));
    println!("{:?}", parse_kv("=oops"));
    println!("{:?}", parse_kv("no-equals"));
}
```

Output:

```
Some(("host", "localhost"))
None
None
```

The `?` after `split_once` is an early return that yields `None` when there is no `=`; the explicit `return None` handles the empty-key case; and `Some((..., ...))` is the tuple tail expression for the success path.

</details>

---

## Summary

**What you've learned:**

- A function returns its **tail expression**: last line, no semicolon, no `return`.
- A trailing semicolon turns that expression into a statement of type `()`.
- `return` is reserved for **early** exits (guard clauses, `?`).
- Functions with no `-> Type` return `()`, the unit type (Rust's `void`-like value).
- Return several values as a **tuple** (or a named **struct** when the fields deserve names).
- Every code path must produce the declared type, enforced by the compiler.

**Key syntax:**

```rust
fn f(x: i32) -> i32 { x + 1 }      // tail expression
fn g() { /* ... */ }               // returns ()
fn h(n: i32) -> i32 {              // early return + tail expression
    if n < 0 { return 0; }
    n * 2
}
fn pair() -> (i32, i32) { (1, 2) } // tuple of multiple values
```
