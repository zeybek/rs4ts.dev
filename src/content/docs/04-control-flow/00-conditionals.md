---
title: "Conditionals: `if` / `else`"
description: "Rust has no ternary because if is an expression: let x = if cond { a } else { b }. Conditions must be a real bool, since Rust, unlike JavaScript, has no truthiness."
---

In TypeScript, `if` is a **statement** and the ternary `? :` is the only conditional that produces a value. In Rust, there is no ternary at all, because `if` itself is an **expression** that produces a value. This single shift, plus the fact that Rust conditions must be a real `bool` (no "truthiness"), is most of what you need to know.

---

## Quick Overview

Rust's `if`/`else` looks almost identical to TypeScript's, but two things are different in ways that matter every day:

- **`if` is an expression.** It evaluates to a value, so `let x = if cond { a } else { b };` replaces the ternary `cond ? a : b`. No separate `?:` operator exists.
- **Conditions must be `bool`.** There is no truthiness. `if 0`, `if ""`, `if someObject` — all of those are *compile errors* in Rust. You write the comparison explicitly.

If you internalize "the condition is always a `bool`, and the whole `if` can be a value," you already understand 90% of Rust conditionals.

---

## TypeScript/JavaScript Example

```typescript
// A small shipping-cost calculator, the way you'd write it in TS.
function shippingCost(weightKg: number, isMember: boolean): number {
  let cost: number;

  // if/else statement: assigns into a pre-declared `let`.
  if (weightKg > 20) {
    cost = 25;
  } else if (weightKg > 5) {
    cost = 12;
  } else {
    cost = 5;
  }

  // Ternary expression: the ONLY conditional in JS that yields a value.
  const discount = isMember ? cost * 0.1 : 0;

  return cost - discount;
}

// Truthiness: JS coerces non-booleans in a condition.
function describe(name?: string) {
  if (name) {
    // "" , undefined, null, 0, NaN are all falsy
    console.log(`Hello, ${name}`);
  } else {
    console.log("Hello, stranger");
  }
}

console.log(shippingCost(3, true)); // 4.5
console.log(shippingCost(8, false)); // 12
describe("Ada"); // Hello, Ada
describe(""); // Hello, stranger  (empty string is falsy)
```

Two TypeScript habits to notice: the `let cost` declared-then-assigned pattern, and `if (name)` relying on the empty string being **falsy**.

---

## Rust Equivalent

```rust playground
// The same calculator, written idiomatically in Rust.
fn shipping_cost(weight_kg: f64, is_member: bool) -> f64 {
    // `if` is an EXPRESSION: it evaluates to a value we bind with `let`.
    // This replaces the declared-then-assigned `let cost` pattern.
    let cost = if weight_kg > 20.0 {
        25.0
    } else if weight_kg > 5.0 {
        12.0
    } else {
        5.0
    };

    // This `if`-expression is Rust's ternary. There is no `? :` operator.
    let discount = if is_member { cost * 0.1 } else { 0.0 };

    cost - discount
}

// No truthiness: the condition must be an explicit `bool`.
fn describe(name: &str) {
    if !name.is_empty() {
        println!("Hello, {name}");
    } else {
        println!("Hello, stranger");
    }
}

fn main() {
    println!("{}", shipping_cost(3.0, true)); // 4.5
    println!("{}", shipping_cost(8.0, false)); // 12
    describe("Ada"); // Hello, Ada
    describe(""); // Hello, stranger
}
```

Real output from `cargo run`:

```text
4.5
12
Hello, Ada
Hello, stranger
```

> **Note:** The `cost` binding is computed once by the whole `if`-expression. There is no mutable, uninitialized variable waiting to be filled in. This is more than a style preference: the compiler *knows* `cost` is always initialized to exactly one value, which is why it never needs `let mut`.

---

## Detailed Explanation

### `if` is an expression, not a statement

In TypeScript, an `if` block does not evaluate to anything; you reach a value either by mutating an outer variable or by using the ternary. In Rust, the `if`/`else` construct *is* a value:

```rust playground
fn main() {
    let score = 72;

    // The whole `if`/`else` evaluates to one of the two branch values.
    let grade = if score >= 60 { "pass" } else { "fail" };

    println!("grade = {grade}"); // grade = pass
}
```

The key mechanic: a block `{ ... }` in Rust evaluates to its **last expression** when that expression has **no trailing semicolon**. So `{ "pass" }` is a block whose value is `"pass"`. Each arm of the `if` is such a block, and the `if` takes the value of whichever arm runs.

This is the same block-as-value rule you saw with function bodies in [Section 03 — Functions](/03-functions/02-return-values/), now applied to control flow. (Expressions vs. statements were introduced back in [Section 02 — Variables](/02-basics/00-variables/).)

A block arm can contain multiple lines; only the final unterminated expression is the value:

```rust playground
fn main() {
    let n = 7;

    let label = if n % 2 == 0 {
        let kind = "even"; // intermediate statement (note the semicolon)
        format!("{n} is {kind}") // final expression -> the block's value
    } else {
        format!("{n} is odd")
    };

    println!("{label}"); // 7 is odd
}
```

### Why there is no ternary operator

Because `if` is already an expression, a separate `? :` would be redundant. The Rust form is slightly longer than `cond ? a : b`, but it reads the same and, importantly, it is the *exact same construct* whether you use one branch or ten. You never switch syntaxes between "I want a value" and "I want side effects."

```rust playground
fn main() {
    let is_member = true;
    let cost = 100.0;

    // TS: const discount = isMember ? cost * 0.1 : 0;
    let discount = if is_member { cost * 0.1 } else { 0.0 };

    println!("{discount}"); // 10
}
```

### Both arms must have the same type

Since an `if`-expression produces a single value, every branch that can be reached must produce the **same type**. The compiler unifies the arm types; if they disagree, it is a compile error (see Common Pitfalls). TypeScript's ternary, by contrast, happily produces a *union* type like `string | number`.

### No truthiness — the condition is always `bool`

In JavaScript, `if (x)` runs `Boolean(x)` for you: `0`, `""`, `null`, `undefined`, `NaN` are falsy; everything else is truthy. Rust does **none** of this. The condition between `if` and `{` must already be of type `bool`. You make the comparison explicit:

```rust playground
fn main() {
    let count = 3;
    let name = "Bob";

    // JS: if (count)         -> Rust: compare to get a bool
    if count != 0 {
        println!("count is non-zero");
    }

    // JS: if (name)          -> Rust: ask the value a yes/no question
    if !name.is_empty() {
        println!("name has {} chars", name.len());
    }
}
```

Output:

```text
count is non-zero
name has 3 chars
```

This is verbose at first, but it removes an entire class of bugs (the infamous `if (count)` that silently skips when `count` is legitimately `0`).

### `if let` — a teaser for pattern matching

Rust has a second flavor of `if` that pattern-matches and binds in one step. You will reach for it constantly with `Option<T>` (Rust's `null`-free alternative; see [Section 02 — Types](/02-basics/01-types/)):

```rust playground
fn main() {
    let maybe_user: Option<&str> = Some("ada");

    // "If this Option is a `Some`, bind its inner value to `user`."
    if let Some(user) = maybe_user {
        println!("logged in as {user}");
    } else {
        println!("anonymous");
    }
}
```

Output:

```text
logged in as ada
```

Think of `if let` as the conditional cousin of TypeScript's `if (user !== undefined)` narrowing, except it also **unwraps** the value for you in the same line. This is just a preview; the full treatment (including `while let` and `let ... else`) lives in [if let / while let](/04-control-flow/03-if-let-while-let/), and the general pattern-matching tool is [match](/04-control-flow/02-match/).

---

## Key Differences

| Aspect | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Is `if` a value? | No. `if` is a statement; only `? :` yields a value | Yes. `if` is an expression |
| Ternary operator | `cond ? a : b` | None; use `if cond { a } else { b }` |
| Condition type | Any value (coerced via truthiness) | Must be exactly `bool` |
| `if (0)` / `if ("")` | Runs the else branch (falsy) | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Compile error: `expected bool` |
| Branch result types | May differ — produces a union (`string \| number`) | Must unify to one type |
| `if` without `else` as a value | Ternary requires both sides | `()` only — can't bind to a typed `let` |
| Parentheses around condition | Required: `if (x > 0)` | Omitted: `if x > 0` (braces required) |
| Chained comparison `0 < x < 10` | Allowed (and usually a bug) | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Compile error — must write `0 < x && x < 10` |

### Why Rust makes these choices

- **Expression-orientation** means fewer mutable, half-initialized variables. The value and the decision that produces it live in one place.
- **No truthiness** means the compiler can guarantee a condition is a genuine boolean question, never an accidental coercion. The cost, a slightly longer `x != 0`, is paid once at write time and saves you at debug time.
- **Type unification across arms** is what makes `if` usable as an expression at all: a value has to have *one* type.

---

## Common Pitfalls

### Pitfall 1: Using a number or string as a condition (expecting truthiness)

```rust
fn main() {
    let count = 3;
    if count {
        // does not compile (error[E0308]: mismatched types)
        println!("non-zero");
    }
}
```

Real compiler output:

```text
error[E0308]: mismatched types
 --> src/main.rs:3:8
  |
3 |     if count {
  |        ^^^^^ expected `bool`, found integer
```

The same happens with strings: `if name { ... }` reports `expected bool, found &str`. **Fix:** write the comparison you actually mean: `if count != 0` or `if !name.is_empty()`.

### Pitfall 2: `if` and `else` arms with different types

```rust
fn main() {
    let score = 72;
    // does not compile (error[E0308]: `if` and `else` have incompatible types)
    let result = if score >= 60 { "pass" } else { 0 };
    println!("{result:?}");
}
```

Real compiler output:

```text
error[E0308]: `if` and `else` have incompatible types
 --> src/main.rs:3:51
  |
3 |     let result = if score >= 60 { "pass" } else { 0 };
  |                                   ------          ^ expected `&str`, found integer
  |                                   |
  |                                   expected because of this
```

Unlike a TypeScript ternary (which would infer `string | number`), Rust needs a single type. **Fix:** make both arms the same type, or model the two cases with an `enum` and `match` (see [match](/04-control-flow/02-match/)).

### Pitfall 3: Using a bare `if` (no `else`) as an expression

```rust
fn main() {
    let score = 72;
    // does not compile (error[E0317]: `if` may be missing an `else` clause)
    let grade = if score >= 60 { "pass" };
    println!("{grade:?}");
}
```

Real compiler output:

```text
error[E0317]: `if` may be missing an `else` clause
 --> src/main.rs:3:17
  |
3 |     let grade = if score >= 60 { "pass" };
  |                 ^^^^^^^^^^^^^^^^^------^^
  |                 |                |
  |                 |                found here
  |                 expected `&str`, found `()`
  |
  = note: `if` expressions without `else` evaluate to `()`
  = help: consider adding an `else` block that evaluates to the expected type
```

An `if` with no `else` evaluates to the unit type `()` (Rust's "nothing" — comparable to `void`). You can only bind such an `if` to nothing, not to a `&str`. **Fix:** add an `else` arm that returns the same type, or restructure (often a `match` or a default value).

### Pitfall 4: Trying to chain comparisons like in math

```rust
fn main() {
    let x = 5;
    // does not compile (error: comparison operators cannot be chained)
    let ok = 0 < x < 10;
    println!("{ok}");
}
```

Real compiler output:

```text
error: comparison operators cannot be chained
 --> src/main.rs:3:16
  |
3 |     let ok = 0 < x < 10;
  |                ^   ^
  |
help: split the comparison into two
  |
3 |     let ok = 0 < x && x < 10;
  |                    ++++
```

JavaScript *allows* `0 < x < 10`, but it means `(0 < x) < 10` → `true < 10` → `1 < 10` → `true`, which is almost never what you want. Rust rejects it outright. **Fix:** `0 < x && x < 10`, or the more readable `(0..10).contains(&x)` for ranges. (See [Operators](/02-basics/02-operators/) for the logical operators.)

### Pitfall 5: Putting a semicolon after a branch value

```rust playground
fn main() {
    let n = 4;
    // Adding `;` turns the branch into a statement that yields `()`,
    // so both arms become `()` and `kind` is `()` — not what you wanted.
    let _kind = if n % 2 == 0 {
        "even"; // ← stray semicolon discards the value
    } else {
        "odd";
    };
    // _kind is now (), not "even"
}
```

This *compiles* (both arms are `()`), so it is a silent logic bug rather than a hard error. **Fix:** drop the semicolons inside the arms so each block's final expression becomes its value.

---

## Best Practices

- **Prefer `let x = if ... { } else { };` over a mutable, assigned-later variable.** It keeps the binding immutable and proves to the reader (and compiler) that exactly one value is chosen.
- **Make conditions read as questions.** `if !name.is_empty()`, `if count > 0`, `if user.is_some()`. Method calls like `.is_empty()`, `.is_some()`, `.contains(&x)` are idiomatic and clearer than re-deriving truthiness.
- **Reach for `match` once you have three-plus mutually exclusive branches** on the same value — it gives you exhaustiveness checking that `else if` chains don't. See [match](/04-control-flow/02-match/).
- **Use `if let` instead of an `if` + manual unwrap** when you only care about one variant of an `Option`/`Result`. It's shorter and avoids panics. See [if let / while let](/04-control-flow/03-if-let-while-let/).
- **Keep arm types identical and small.** If arms want to return different shapes, that's a signal to introduce an `enum`.
- **Don't wrap conditions in parentheses.** `if (x > 0)` compiles but `clippy` will nudge you toward `if x > 0`; the braces already delimit the body.

---

## Real-World Example

A retry policy for an HTTP client: classify a response by status code and decide whether to succeed, retry with backoff, or give up. Every decision is driven by `if`/`else` expressions, and every condition is an explicit `bool`.

```rust playground
#[derive(Debug)]
enum RetryDecision {
    Succeed,
    RetryAfter(u64), // milliseconds to wait before retrying
    Fail,
}

fn classify(status: u16, attempt: u32) -> RetryDecision {
    // No truthiness: each condition is a concrete boolean test.
    // `(200..300).contains(&status)` reads better than `status >= 200 && status < 300`.
    if (200..300).contains(&status) {
        RetryDecision::Succeed
    } else if status == 429 || (500..600).contains(&status) {
        // `if` as an expression computes exponential backoff inline,
        // capping it after a few attempts.
        let backoff = if attempt < 5 { 100 * 2u64.pow(attempt) } else { 3200 };
        RetryDecision::RetryAfter(backoff)
    } else {
        RetryDecision::Fail
    }
}

fn main() {
    let responses = [(200, 0), (404, 0), (503, 2), (429, 6)];

    for (status, attempt) in responses {
        match classify(status, attempt) {
            RetryDecision::Succeed => println!("status {status}: ok"),
            RetryDecision::RetryAfter(ms) => println!("status {status}: retry in {ms} ms"),
            RetryDecision::Fail => println!("status {status}: giving up"),
        }
    }
}
```

Real output from `cargo run`:

```text
status 200: ok
status 404: giving up
status 503: retry in 400 ms
status 429: retry in 3200 ms
```

Notice how the inner `let backoff = if ... { ... } else { ... };` and the outer `if`/`else if`/`else` are the same construct used at two scales, and how `(200..300).contains(&status)` replaces a truthiness-laden range check. The `match` at the end is a preview of the next tool in your control-flow toolbox.

---

## Further Reading

### Official Documentation

- [The Rust Book – Control Flow (`if` expressions)](https://doc.rust-lang.org/book/ch03-05-control-flow.html#if-expressions)
- [The Rust Book – `if let` Concise Control Flow](https://doc.rust-lang.org/book/ch06-03-if-let.html)
- [Rust Reference – `if` and `if let` expressions](https://doc.rust-lang.org/reference/expressions/if-expr.html)
- [Rust by Example – `if`/`else`](https://doc.rust-lang.org/rust-by-example/flow_control/if_else.html)

### Related Sections in This Guide

- [Loops](/04-control-flow/01-loops/) — `for`, `while`, and the value-returning `loop`; why there is no C-style `for`.
- [`match`](/04-control-flow/02-match/) — the exhaustive, pattern-matching successor to `switch` and long `else if` chains.
- [`if let` / `while let`](/04-control-flow/03-if-let-while-let/) — the full story behind the `if let` teaser above, plus `let ... else`.
- [Variables and Mutability](/02-basics/00-variables/) — expressions vs. statements, and why immutable bindings pair so well with `if`-expressions.
- [Operators](/02-basics/02-operators/) — the comparison and logical operators that build your conditions.
- [Section 05 — Ownership](/05-ownership/) — how values flow out of `if` branches once types get more complex than `&str`.

---

## Exercises

### Exercise 1: Ternary to `if`-expression

**Difficulty:** Easy

**Objective:** Translate a TypeScript ternary into an idiomatic Rust `if`-expression.

**Instructions:** The TypeScript below returns `"adult"` or `"minor"`. Implement the Rust `category` function so it returns the same `&str`, binding the result of a single `if`-expression. Do not use a mutable variable.

```typescript
// TypeScript original
function category(age: number): string {
  return age >= 18 ? "adult" : "minor";
}
```

```rust
fn category(age: u32) -> &'static str {
    // TODO: return "adult" if age >= 18, otherwise "minor"
    /* ??? */
}

fn main() {
    println!("{}", category(20)); // adult
    println!("{}", category(12)); // minor
}
```

<details>
<summary>Solution</summary>

```rust playground
fn category(age: u32) -> &'static str {
    if age >= 18 { "adult" } else { "minor" }
}

fn main() {
    println!("{}", category(20)); // adult
    println!("{}", category(12)); // minor
}
```

The whole `if`/`else` is the function's tail expression, so it becomes the return value: no `return` keyword and no mutable variable needed. Output:

```text
adult
minor
```

</details>

### Exercise 2: Kill the truthiness

**Difficulty:** Medium

**Objective:** Convert truthiness-based JavaScript conditions into explicit Rust `bool` tests, using nested `if`-expressions for a tiered result.

**Instructions:** Port this JS pricing function to Rust. A `"premium"` role pays no base fee; everyone else pays `5`. On top of that, add a surcharge of `20` for more than 100 monthly orders, `10` for more than 10, and `0` otherwise. Return the total as a `u32`.

```javascript
// JavaScript original (relies on string comparison, not truthiness here,
// but watch the branching)
function feeFor(role, monthlyOrders) {
  const base = role === "premium" ? 0 : 5;
  let surcharge;
  if (monthlyOrders > 100) surcharge = 20;
  else if (monthlyOrders > 10) surcharge = 10;
  else surcharge = 0;
  return base + surcharge;
}
```

```rust
fn fee_for(role: &str, monthly_orders: u32) -> u32 {
    // TODO: compute base and surcharge with if-expressions, then sum them
    /* ??? */
}

fn main() {
    println!("{}", fee_for("premium", 150)); // 20
    println!("{}", fee_for("free", 50));     // 15
    println!("{}", fee_for("free", 3));      // 5
}
```

<details>
<summary>Solution</summary>

```rust playground
fn fee_for(role: &str, monthly_orders: u32) -> u32 {
    let base = if role == "premium" { 0 } else { 5 };
    let surcharge = if monthly_orders > 100 {
        20
    } else if monthly_orders > 10 {
        10
    } else {
        0
    };
    base + surcharge
}

fn main() {
    println!("{}", fee_for("premium", 150)); // 20
    println!("{}", fee_for("free", 50));     // 15
    println!("{}", fee_for("free", 3));      // 5
}
```

Both `base` and `surcharge` are immutable bindings produced by `if`-expressions; no `let mut surcharge;` declared-then-assigned dance. Output:

```text
20
15
5
```

</details>

### Exercise 3: `if let` instead of a default value

**Difficulty:** Medium

**Objective:** Use the `if let` teaser to greet a user by name when one is present, falling back to a guest greeting otherwise.

**Instructions:** Implement `greet` so that, given `Some(name)`, it returns `"Welcome back, {name}!"`, and given `None`, it returns `"Welcome, guest!"`. Use `if let ... else`, and make the whole thing an expression that produces the returned `String`.

```rust
fn greet(config: Option<&str>) -> String {
    // TODO: if there's a name, welcome them back by name; otherwise greet a guest
    /* ??? */
}

fn main() {
    println!("{}", greet(Some("Ada")));
    println!("{}", greet(None));
}
```

<details>
<summary>Solution</summary>

```rust playground
fn greet(config: Option<&str>) -> String {
    if let Some(name) = config {
        format!("Welcome back, {name}!")
    } else {
        "Welcome, guest!".to_string()
    }
}

fn main() {
    println!("{}", greet(Some("Ada")));
    println!("{}", greet(None));
}
```

`if let Some(name) = config` both *tests* that `config` is a `Some` and *binds* its inner value to `name` in one step: the conditional cousin of TypeScript's `if (config !== undefined)` narrowing, but it unwraps for you. Both arms produce a `String`, so the `if let`/`else` is itself the returned expression. Output:

```text
Welcome back, Ada!
Welcome, guest!
```

For the deeper dive on `if let`, `while let`, and `let ... else`, head to [if let / while let](/04-control-flow/03-if-let-while-let/).

</details>
