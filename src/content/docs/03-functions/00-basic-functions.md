---
title: "Basic Functions and Signatures"
description: "Rust functions resemble TypeScript's, but every parameter is typed and the last expression returns with no return keyword. Learn fn signatures and unit."
---

Functions are the workhorse of any program, and you already write them every day in TypeScript. The Rust syntax is close enough to feel familiar, but two ideas reshape how you read Rust code: **every parameter and return value is explicitly typed**, and **the function body is an expression** whose final line is the return value.

---

## Quick Overview

A Rust function is declared with the `fn` keyword, takes **fully typed** parameters, and declares its return type after a `->` arrow. Unlike TypeScript, Rust never infers parameter types or return types from usage: the signature is a hard contract. And because Rust is **expression-oriented**, the last expression in the body (with no trailing semicolon) is automatically returned.

> **Note:** This file covers function definitions, signatures, typed parameters, return types, and the statement-vs-expression model. Parameter patterns (no default or rest parameters), return-value details, and closures each get their own file — see [Further Reading](#further-reading).

---

## TypeScript/JavaScript Example

```typescript
// TypeScript - function declarations with optional type annotations
function greet(name: string): string {
  return `Hello, ${name}!`;
}

function add(a: number, b: number): number {
  return a + b;
}

// Return type can be inferred and omitted
function logMessage(level: string, message: string) {
  console.log(`[${level}] ${message}`);
}

const message = greet("Ada");
console.log(message); // "Hello, Ada!"

console.log(`2 + 3 = ${add(2, 3)}`); // "2 + 3 = 5"
logMessage("INFO", "service started"); // "[INFO] service started"
```

**Key points:**

- `function` keyword, `camelCase` names by convention.
- Parameter types are optional in plain JavaScript and only enforced by TypeScript at compile time.
- The return type can be omitted and TypeScript will infer it.
- `return` is always required to produce a value.

---

## Rust Equivalent

```rust
// Rust - every parameter and the return value is explicitly typed
fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

fn add(a: i32, b: i32) -> i32 {
    a + b // tail expression: no semicolon, this is the return value
}

fn log_message(level: &str, message: &str) {
    println!("[{level}] {message}");
}

fn main() {
    let message = greet("Ada");
    println!("{message}");

    let sum = add(2, 3);
    println!("2 + 3 = {sum}");

    log_message("INFO", "service started");
}
```

**Output (verified):**

```text
Hello, Ada!
2 + 3 = 5
[INFO] service started
```

**Key points:**

- `fn` keyword, `snake_case` names by convention (Rust will warn if you use `camelCase`).
- Every parameter **must** have a type; there is no inference for parameters.
- The return type follows `->`. Omitting it means the function returns the **unit type** `()` (Rust's "returns nothing").
- The last expression with **no semicolon** is the return value. No `return` keyword needed.

---

## Detailed Explanation

### The anatomy of a Rust function

```rust
fn add(a: i32, b: i32) -> i32 {
//  ^    ^         ^       ^
//  |    |         |       └─ return type (after the arrow)
//  |    |         └───────── second typed parameter
//  |    └─────────────────── first typed parameter
//  └──────────────────────── the `fn` keyword + function name
    a + b
}
```

Compare this to the TypeScript signature `function add(a: number, b: number): number`. The pieces map almost one-to-one:

| Piece            | TypeScript                    | Rust                        |
| ---------------- | ----------------------------- | --------------------------- |
| Keyword          | `function`                    | `fn`                        |
| Name convention  | `camelCase`                   | `snake_case`                |
| Parameter type   | `a: number` (optional)        | `a: i32` (mandatory)        |
| Return type      | `: number` (optional)         | `-> i32` (defaults to `()`) |
| Produce a value  | `return a + b;`               | `a + b` (tail expression)   |

### Parameter types are never inferred

In TypeScript you can lean on inference for return types, and in plain JavaScript you can skip types entirely. Rust draws a firm line. **Parameter types are always required**, because a function signature is a public contract the compiler checks at every call site. Return types may be omitted only when the function returns `()` (nothing).

```rust
// This is the full, idiomatic form. Nothing here is optional except
// the return type — and only because returning `()` is a real choice.
fn multiply(a: f64, b: f64) -> f64 {
    a * b
}
```

> **Tip:** If you find yourself wishing for "inferred parameter types," you usually want **generics** instead — a single function that works across many types. That is covered in [Section 09: Generics & Traits](/09-generics-traits/).

### Statements vs expressions: the core mental shift

This is the idea that trips up most TypeScript developers. Rust distinguishes between:

- **Statements** perform an action and return *nothing*. `let x = 5;` is a statement.
- **Expressions** evaluate to a *value*. `5 + 3`, a function call, and even an `if`/`match` block are expressions.

A function body is a **block**, and a block is itself an expression: its value is the value of its **final expression**, provided that expression has no trailing semicolon.

```rust
// statements vs expressions
fn classify(score: u32) -> &'static str {
    // `if` is an expression in Rust; it evaluates to a value.
    let grade = if score >= 90 {
        "A"
    } else if score >= 80 {
        "B"
    } else {
        "C or below"
    };

    grade // tail expression
}

fn main() {
    // A block is an expression; its value is the last expression in it.
    let x = {
        let a = 3;
        let b = 4;
        a * a + b * b // no semicolon -> this is the block's value
    };
    println!("x = {x}");

    println!("{}", classify(95));
    println!("{}", classify(83));
    println!("{}", classify(40));
}
```

**Output (verified):**

```text
x = 25
A
B
C or below
```

In TypeScript, an `if`/`else` is a statement; it cannot be assigned to a variable directly. You reach for the ternary operator (`cond ? a : b`) or an immediately-invoked function for anything bigger. In Rust, `if`, `match`, `loop`, and `{ ... }` blocks are all expressions, so you assign them straight to a binding. (Control-flow expressions are explored in depth in [Section 04: Control Flow](/04-control-flow/).)

### Semicolons matter

The trailing semicolon is not optional decoration. It changes meaning:

- `a + b` (no semicolon) → an expression that becomes the block's value.
- `a + b;` (semicolon) → a statement that throws the value away and evaluates to `()`.

So a function whose last line is `a + b;` returns `()`, not the sum. That is one of the most common early mistakes; see [Common Pitfalls](#common-pitfalls).

### Functions are order-independent (no "hoisting" needed)

JavaScript hoists `function` declarations so you can call them before they appear in the file. Rust does not need hoisting because items in a module are resolved regardless of order. You can call a function defined later in the same file (or even later in the same module) freely.

```rust
// Functions can be called before their definition (order-independent).
fn main() {
    println!("{}", double(21));
}

fn double(n: i32) -> i32 {
    n * 2
}
```

**Output (verified):**

```text
42
```

> **Note:** This applies to **items** (functions, structs, constants) at module scope. It does *not* apply to local `let` bindings inside a function body, which must be declared before use, exactly like `let`/`const` in JavaScript.

---

## Key Differences

| Concept                    | TypeScript/JavaScript                          | Rust                                                    |
| -------------------------- | ---------------------------------------------- | ------------------------------------------------------- |
| Keyword                    | `function`                                     | `fn`                                                    |
| Naming convention          | `camelCase`                                    | `snake_case` (lint-enforced)                            |
| Parameter types            | Optional (TS) / absent (JS)                    | **Always required**                                     |
| Return type                | Optional, inferred                             | Declared with `->`; defaults to `()`                    |
| Returning a value          | `return` keyword required                      | Tail expression (no semicolon); `return` is optional    |
| `if` / blocks              | Statements (use ternary to get a value)        | Expressions (assign directly)                           |
| Call before definition     | Hoisted `function` declarations                | Order-independent for module items                      |
| "Returns nothing"          | `void`                                         | `()` (the unit type — a real, zero-size value)          |
| Overloading                | Allowed (multiple signatures)                  | Not allowed; use generics/traits instead                |

### `void` vs the unit type `()`

TypeScript's `void` is a "don't use this value" marker. Rust's `()` is a genuine value (the empty tuple) that every expression-with-no-other-value produces. A function with no `-> Type` returns `()`, and you can even bind it:

```rust
// A function with no `-> Type` returns the unit type `()`.
fn print_banner(title: &str) {
    println!("==== {title} ====");
}

// This is exactly equivalent — explicit unit return type.
fn print_banner_explicit(title: &str) -> () {
    println!("==== {title} ====");
}

fn main() {
    print_banner("Report");
    print_banner_explicit("Report");

    // The value of calling a unit-returning function IS `()`.
    let nothing: () = print_banner("Again");
    println!("{nothing:?}");
}
```

**Output (verified):**

```text
==== Report ====
==== Report ====
==== Again ====
()
```

You will almost never write `-> ()` explicitly; idiomatic Rust omits it. But understanding that "no return type" means "returns `()`" explains the error messages in the next section.

### No function overloading

TypeScript lets you declare multiple signatures for one name. Rust does not. Instead you parameterize over types with **generics and traits**, or accept an `enum`. We point to the idiomatic alternatives in [Function Parameters](/03-functions/01-parameters/) and [Section 09](/09-generics-traits/).

---

## Common Pitfalls

### Pitfall 1: Accidental semicolon on the return expression

This is *the* classic. You write what looks like a return, add a semicolon out of habit, and the function suddenly returns `()`.

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b; // accidental semicolon turns this into a statement
}
```

**Real compiler error:**

```text
error[E0308]: mismatched types
 --> err_semicolon.rs:1:27
  |
1 | fn add(a: i32, b: i32) -> i32 {
  |    ---                    ^^^ expected `i32`, found `()`
  |    |
  |    implicitly returns `()` as its body has no tail or `return` expression
2 |     a + b; // accidental semicolon turns this into a statement
  |          - help: remove this semicolon to return this value
```

Notice the compiler points directly at the stray semicolon and tells you to remove it. **Fix:** drop the `;`:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

### Pitfall 2: Omitting a parameter type

TypeScript tolerates `function add(a, b)` (with implicit `any` in loose mode). Rust does not: every parameter needs a type.

```rust
fn add(a, b) {
    a + b
}
```

**Real compiler error (first message):**

```text
error: expected one of `:`, `@`, or `|`, found `,`
 --> err_noparamtype.rs:1:9
  |
1 | fn add(a, b) {
  |         ^ expected one of `:`, `@`, or `|`
  |
  = note: anonymous parameters are removed in the 2018 edition (see RFC 1685)
help: if this is a `self` type, give it a parameter name
  |
1 | fn add(self: a, b) {
  |        +++++
help: if this is a parameter name, give it a type
  |
1 | fn add(a: TypeName, b) {
  |         ++++++++++
help: if this is a type, explicitly ignore the parameter name
  |
1 | fn add(_: a, b) {
  |        ++
```

**Fix:** annotate both parameters and the return type:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

### Pitfall 3: Returning a value without declaring a return type

If you forget the `-> Type`, the compiler assumes the function returns `()` and then complains that your tail expression has the wrong type.

```rust
fn square(n: i32) {
    n * n
}
```

**Real compiler error:**

```text
error[E0308]: mismatched types
 --> err_noreturntype.rs:2:5
  |
1 | fn square(n: i32) {
  |                  - help: try adding a return type: `-> i32`
2 |     n * n
  |     ^^^^^ expected `()`, found `i32`
```

The compiler even suggests the exact return type to add. **Fix:**

```rust
fn square(n: i32) -> i32 {
    n * n
}
```

### Pitfall 4: Reaching for `return` everywhere

Coming from TypeScript, you will instinctively write `return a + b;`. It compiles and works, but it is not idiomatic for the *final* expression of a function, and Clippy (Rust's linter) will tell you so.

```rust
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
```

**Real `cargo clippy` warning:**

```text
warning: unneeded `return` statement
 --> src/bin/clippy_return.rs:2:5
  |
2 |     return a + b;
  |     ^^^^^^^^^^^^
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_return
  = note: `#[warn(clippy::needless_return)]` on by default
help: remove `return`
  |
2 -     return a + b;
2 +     a + b
  |
```

`return` is still useful for **early returns** in the middle of a function; that is covered in [Return Values](/03-functions/02-return-values/). For the final value, use the tail expression.

---

## Best Practices

### 1. Use tail expressions for the final value

```rust
// Idiomatic: tail expression
fn area(width: f64, height: f64) -> f64 {
    width * height
}

// Works, but Clippy flags the needless `return`
fn area_verbose(width: f64, height: f64) -> f64 {
    return width * height;
}
```

### 2. Name functions in `snake_case`

Rust's compiler warns (`non_snake_case`) if you use `camelCase`. Embrace `snake_case`: `calculate_total`, not `calculateTotal`.

### 3. Borrow instead of taking ownership for read-only parameters

For a parameter you only need to read, take a **borrow** (`&str`, `&[T]`) rather than an owned `String` or `Vec<T>`. This avoids forcing the caller to give up (or clone) their data. This connects directly to [Section 05: Ownership](/05-ownership/).

```rust
// Reads the string without taking ownership
fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}
```

### 4. Keep signatures honest

The signature is documentation the compiler enforces. Prefer precise types (`u32` for a count that can't be negative, `f64` for money-free math) over a single catch-all, just as you would prefer precise TypeScript types over `any`.

### 5. Let `()` be implicit

Do not write `-> ()`. Omitting the return type for side-effecting functions is the convention every Rust reader expects.

---

## Real-World Example

A small order-pricing module that uses several typed functions, tail expressions, and `if`/`match` as expressions. This is the shape of code you would find in a real billing service.

```rust
// Real-world example: a tiny order-pricing module.

/// A line item in a shopping cart.
struct LineItem {
    name: String,
    unit_price_cents: u32,
    quantity: u32,
}

/// Returns the subtotal in cents for a single line item.
fn line_total_cents(item: &LineItem) -> u32 {
    item.unit_price_cents * item.quantity
}

/// Applies a percentage discount (0-100) to an amount in cents,
/// rounding to the nearest cent.
fn apply_discount(amount_cents: u32, percent_off: u8) -> u32 {
    // `if` is an expression, so we can bind its result directly.
    let percent_off = if percent_off > 100 { 100 } else { percent_off };
    let kept = 100 - u32::from(percent_off);
    (amount_cents * kept + 50) / 100 // tail expression: rounded result
}

/// Formats an amount of cents as a dollar string, e.g. 1299 -> "$12.99".
fn format_price(amount_cents: u32) -> String {
    let dollars = amount_cents / 100;
    let cents = amount_cents % 100;
    format!("${dollars}.{cents:02}")
}

fn main() {
    let cart = [
        LineItem { name: "Keyboard".to_string(), unit_price_cents: 7999, quantity: 1 },
        LineItem { name: "Cable".to_string(), unit_price_cents: 599, quantity: 3 },
    ];

    // Sum subtotals across the cart.
    let mut subtotal = 0;
    for item in &cart {
        let total = line_total_cents(item);
        println!("{:<10} x{} = {}", item.name, item.quantity, format_price(total));
        subtotal += total;
    }

    let total = apply_discount(subtotal, 10);
    println!("Subtotal: {}", format_price(subtotal));
    println!("Total (10% off): {}", format_price(total));
}
```

**Output (verified):**

```text
Keyboard   x1 = $79.99
Cable      x3 = $17.97
Subtotal: $97.96
Total (10% off): $88.16
```

**What to notice:**

- Each function has a precise typed signature. The compiler enforces that `quantity` is a `u32`, so a negative quantity is simply unrepresentable.
- `line_total_cents` and `format_price` end in a tail expression; no `return` keyword.
- `apply_discount` clamps the percentage with an `if` **expression** bound to a `let`, then returns the computed value as the tail expression.
- `for item in &cart` *borrows* each item, so `cart` is still usable afterward — a preview of ownership in [Section 05](/05-ownership/).

---

## Further Reading

### Official Documentation

- [The Rust Book – Functions](https://doc.rust-lang.org/book/ch03-03-how-functions-work.html)
- [The Rust Book – Statements and Expressions](https://doc.rust-lang.org/book/ch03-03-how-functions-work.html#statements-and-expressions)
- [Rust by Example – Functions](https://doc.rust-lang.org/rust-by-example/fn.html)
- [Rust Reference – Functions](https://doc.rust-lang.org/reference/items/functions.html)

### Related Sections in This Guide

- [Parameters](/03-functions/01-parameters/) — why Rust has no default or rest parameters, and the idiomatic alternatives.
- [Return Values](/03-functions/02-return-values/) — tail returns, early `return`, the unit type, and returning tuples.
- [Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/) — `|args|` syntax and the `Fn`/`FnMut`/`FnOnce` traits.
- [Higher-Order Functions](/03-functions/04-higher-order/) — passing and returning closures.
- [Function Pointers](/03-functions/05-function-pointers/) — the `fn` type and passing named functions.
- [Section 02: Variables and Mutability](/02-basics/00-variables/) — the expression-oriented model starts here.
- [Section 04: Control Flow](/04-control-flow/) — `if`, `match`, and `loop` as expressions.
- [Section 05: Ownership](/05-ownership/) — why read-only parameters should borrow.
- [Section 09: Generics & Traits](/09-generics-traits/) — the replacement for parameter-type inference and overloading.

---

## Exercises

### Exercise 1: Convert Celsius to Fahrenheit

**Difficulty:** Easy

**Objective:** Write a function with a typed signature that returns a value via a tail expression.

**Instructions:**

1. Write a function `celsius_to_fahrenheit` that takes one `f64` parameter and returns an `f64`.
2. Use the formula `c * 9 / 5 + 32`.
3. Do not use the `return` keyword — use a tail expression.
4. Call it from `main` with `100.0`, `0.0`, and `37.0` and print the results.

<details>
<summary>Solution</summary>

```rust
fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

fn main() {
    println!("{}", celsius_to_fahrenheit(100.0)); // 212
    println!("{}", celsius_to_fahrenheit(0.0)); // 32
    println!("{}", celsius_to_fahrenheit(37.0)); // 98.6
}
```

**Verified output:**

```text
212
32
98.6
```

Note the literals are `9.0`, `5.0`, `32.0` — using integer literals like `9` would be a type mismatch against the `f64` value `c`. Rust does not implicitly convert between numeric types.

</details>

### Exercise 2: Password strength check returning a `bool`

**Difficulty:** Medium

**Objective:** Combine several boolean sub-checks and return the result as a tail expression.

**Instructions:**

1. Write `is_strong_password(password: &str) -> bool`.
2. A password is strong when it is at least 8 characters long, contains at least one digit, and contains at least one uppercase letter.
3. Compute three booleans and combine them with `&&` as the tail expression — no `return`.
4. Test it with `"abc"`, `"password"`, and `"Password1"`.

> **Tip:** `str` has handy iterator methods: `password.chars().count()`, and `password.chars().any(|c| c.is_ascii_digit())`. (Closures like `|c| ...` are detailed in [Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/).)

<details>
<summary>Solution</summary>

```rust
fn is_strong_password(password: &str) -> bool {
    let long_enough = password.chars().count() >= 8;
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());

    // Tail expression returns the combined boolean — no `return` needed.
    long_enough && has_digit && has_upper
}

fn main() {
    println!("{}", is_strong_password("abc")); // false
    println!("{}", is_strong_password("password")); // false
    println!("{}", is_strong_password("Password1")); // true
}
```

**Verified output:**

```text
false
false
true
```

</details>

### Exercise 3: Price tier with a `match` expression

**Difficulty:** Medium

**Objective:** Use a `match` expression as the body of a function and return a `&'static str`.

**Instructions:**

1. Write `price_tier(amount_cents: u32) -> &'static str`.
2. Return `"budget"` for `0`–`999`, `"standard"` for `1000`–`9999`, and `"premium"` for anything higher.
3. Use a `match` expression (a single expression body) rather than a chain of `if`/`else if`.
4. Test it with `500`, `2500`, and `50000`.

<details>
<summary>Solution</summary>

```rust
/// Returns the price tier name for a given amount in cents.
fn price_tier(amount_cents: u32) -> &'static str {
    // `match` is an expression; the matched arm's value becomes the result.
    let tier = match amount_cents {
        0..=999 => "budget",
        1000..=9999 => "standard",
        _ => "premium",
    };

    tier // tail expression
}

fn main() {
    println!("{}", price_tier(500)); // budget
    println!("{}", price_tier(2500)); // standard
    println!("{}", price_tier(50000)); // premium
}
```

**Verified output:**

```text
budget
standard
premium
```

You could shorten this further by making the `match` itself the tail expression (dropping the `let tier` binding). Both are idiomatic; the named binding can aid readability. `match` is covered fully in [Section 04: Control Flow](/04-control-flow/).

</details>

---

## Summary

**What you've learned:**

- Rust functions use `fn`, `snake_case` names, **mandatory** parameter types, and a `-> Type` return arrow.
- Omitting the return type means the function returns the unit type `()`, Rust's "nothing."
- Rust is **expression-oriented**: the final expression (no trailing semicolon) is the return value.
- A stray semicolon turns a return value into `()`, producing a `mismatched types` error.
- Functions are order-independent at module scope; no hoisting needed.
- `return` is reserved for early exits; idiomatic final values use the tail expression.
