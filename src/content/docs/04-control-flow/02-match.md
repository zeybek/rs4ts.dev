---
title: "Pattern Matching with `match`"
description: "Rust's match is JS's switch with no fall-through and no forgotten case: an exhaustive expression that destructures, binds, and matches ranges."
---

Rust's `match` is the spiritual successor to JavaScript's `switch`, but it does far more: it is an **expression** that produces a value, and the compiler forces you to handle *every* possible case. If you have ever shipped a bug because a `switch` fell through or missed a case, `match` is about to become your favorite control-flow construct.

---

## Quick Overview

A **`match` expression** compares a value against a series of **patterns** and runs the code for the first one that fits. Unlike `switch`, it is **exhaustive** (you must cover all cases or the code will not compile), it **never falls through** (no `break` needed), and it can **destructure** data and **bind** parts of it to variables in the same step. For a TypeScript/JavaScript developer, think "`switch` + destructuring + a type-checker that refuses to let you forget a case."

---

## TypeScript/JavaScript Example

Here is a realistic `switch` you might write to turn a typed status object into a message, plus the kind of fall-through bug that `switch` invites:

```typescript
type Status =
  | { kind: "ok" }
  | { kind: "notFound" }
  | { kind: "serverError"; code: number };

function describeStatus(status: Status): string {
  switch (status.kind) {
    case "ok":
      return "All good";
    case "notFound":
      return "Resource missing";
    case "serverError":
      return `Server error ${status.code}`;
    default:
      return "Unknown";
  }
}

// Grouping cases relies on intentional fall-through:
function classify(n: number): string {
  switch (n) {
    case 1:
    case 2:
    case 3:
      return "small";
    case 0:
      return "zero";
    default:
      return "other";
  }
}

// The classic accidental fall-through bug:
function risky(x: number): string[] {
  const out: string[] = [];
  switch (x) {
    case 1:
      out.push("one"); // forgot `break`!
    case 2:
      out.push("two");
      break;
    default:
      out.push("other");
  }
  return out;
}

console.log(describeStatus({ kind: "serverError", code: 503 })); // "Server error 503"
console.log(risky(1)); // [ 'one', 'two' ]  ← the bug: both ran
```

Running this with Node v22 prints `[ 'one', 'two' ]` for `risky(1)`: case `1` "fell through" into case `2` because the `break` was missing. The `default` branch is also opt-in: drop it and TypeScript will *not* complain that you missed a `kind`.

---

## Rust Equivalent

```rust
#[derive(Debug)]
enum HttpStatus {
    Ok,
    NotFound,
    ServerError(u16),
}

fn describe_status(status: &HttpStatus) -> String {
    // `match` is an expression: this whole thing is the function's return value.
    match status {
        HttpStatus::Ok => "All good".to_string(),
        HttpStatus::NotFound => "Resource missing".to_string(),
        HttpStatus::ServerError(code) => format!("Server error {code}"),
    }
}

fn classify(n: i32) -> &'static str {
    match n {
        0 => "zero",
        1 | 2 | 3 => "small", // `|` groups patterns — no fall-through trickery
        _ => "other",         // `_` is the catch-all, like `default`
    }
}

fn main() {
    println!("{}", describe_status(&HttpStatus::ServerError(503))); // Server error 503
    println!("{}", classify(2)); // small
    println!("{}", classify(0)); // zero
}
```

Output (real):

```text
Server error 503
small
zero
```

There is no `break`, there is no fall-through, and there is no `default` you can forget: if you delete one of the three `HttpStatus` arms, the program **will not compile**.

> **Note:** Each arm is `pattern => expression,`. The `=>` is a "fat arrow," not a closure (Rust closures use `|args| body`). All arms must produce the **same type**, because the `match` itself evaluates to that type.

---

## Detailed Explanation

### `match` is an expression, not a statement

In JavaScript a `switch` is a statement — it *does* things, it does not *evaluate to* a value. That is why `describeStatus` had to `return` inside each `case`. In Rust, `match` is an expression, so you can assign its result directly:

```rust
fn main() {
    let code = 503;
    // The match evaluates to a String, which we bind to `message`.
    let message = match code {
        200 => "OK".to_string(),
        404 => "Not Found".to_string(),
        500..=599 => format!("Server error {code}"),
        _ => "Unhandled".to_string(),
    };
    println!("{message}"); // Server error 503
}
```

This is the same expression-oriented model you saw with `if` in [Conditionals](/04-control-flow/00-conditionals/) and blocks in [Section 02 — Variables](/02-basics/00-variables/). The last expression of each arm (with **no trailing semicolon**) is that arm's value.

### Exhaustiveness: the compiler has your back

The single biggest upgrade over `switch` is **exhaustiveness checking**. The compiler knows every possible value an `enum` can take, so it refuses to compile a `match` that leaves a case unhandled. This turns "I forgot a case" from a runtime bug into a compile error, and it is *especially* valuable when you later add a new variant to an enum: every `match` that does not handle it lights up red.

### Patterns, not just values

A `switch` case can only test for equality against a constant. A `match` arm is a **pattern**, which can:

- match a literal (`0`, `'a'`, `"hi"`),
- match a range (`1..=9`),
- match several alternatives with `|`,
- **destructure** a tuple, struct, or enum and pull its fields into variables,
- **bind** the whole matched value to a name with `@`,
- and add an extra runtime condition with a **guard** (`if ...`).

The arms below combine destructuring (`ServerError(code)`) with formatting in one step; no separate `status.code` access needed:

```rust
#[derive(Debug)]
enum HttpStatus {
    Ok,
    NotFound,
    ServerError(u16),
}

fn main() {
    let status = HttpStatus::ServerError(503);
    match &status {
        HttpStatus::ServerError(code) => println!("got code {code}"), // code is bound to 503
        other => println!("other: {other:?}"),
    }
}
```

### Order matters: top to bottom, first match wins

Like `switch`, arms are tried in order and the **first** matching pattern wins. Unlike `switch`, evaluation stops there; control never falls into the next arm. This means the **catch-all `_` must come last**; put it first and every arm below it becomes dead code (a real warning, shown in Common Pitfalls).

---

## Key Differences

| Aspect | JavaScript `switch` | Rust `match` |
| --- | --- | --- |
| Produces a value? | No (statement) | Yes (expression) |
| Fall-through | Default behavior; needs `break` | Never; no `break` exists |
| Missing a case | Silently allowed | **Compile error** (must be exhaustive) |
| Grouping cases | Stacked empty `case`s | `pattern1 \| pattern2` |
| Ranges | Not supported (`case 1..5` is not a thing) | `1..=9`, `'a'..='z'` |
| Destructuring | Done separately, before/after the switch | Built into the pattern |
| Extra conditions | An `if` inside the `case` body | Guard: `pattern if cond =>` |
| Equality semantics | Uses `===` (strict) | Structural pattern match (no coercion) |
| Catch-all | `default:` (optional, position-free) | `_` (must be last to be reachable) |

The deepest difference is the mindset: a `switch` *executes statements*; a `match` *describes the shape of data and computes a value from it*. Rust leans hard on this everywhere: `Option`, `Result`, and most enums are designed to be consumed by `match`.

---

## Common Pitfalls

### Pitfall 1: Forgetting a case (non-exhaustive match)

Coming from `switch`, it is natural to handle "the cases you care about" and stop. Rust will not let you:

```rust
enum Direction {
    North,
    South,
    East,
    West,
}

fn turn(d: Direction) -> &'static str {
    match d {
        Direction::North => "up",
        Direction::South => "down",
        // does not compile (error[E0004]): forgot East and West
    }
}
```

The real compiler error:

```text
error[E0004]: non-exhaustive patterns: `Direction::East` and `Direction::West` not covered
 --> src/main.rs:9:11
  |
9 |     match d {
  |           ^ patterns `Direction::East` and `Direction::West` not covered
  |
note: `Direction` defined here
...
4 |     East,
  |     ---- not covered
5 |     West,
  |     ---- not covered
  = note: the matched value is of type `Direction`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern, a match arm with multiple or-patterns as shown, or multiple match arms
```

Add the missing arms, or a `_` catch-all if they really should be treated the same.

> **Tip:** Resist the urge to reach for `_` just to silence this error. An explicit `_` means "any new variant added later is also handled here," which can hide bugs. When the variants are a closed set you control, listing them all means the compiler will remind you to update this `match` the day you add a fifth direction.

### Pitfall 2: Guards do not count toward exhaustiveness

A guard (`if ...`) makes an arm *conditional*, so the compiler cannot assume it covers the pattern. This is exhaustive-looking but does not compile:

```rust
fn main() {
    let opt = Some(5);
    let _ = match opt {
        Some(x) if x > 0 => "positive",
        None => "none",
        // does not compile (error[E0004]): `Some(_)` not covered
    };
}
```

Real error:

```text
error[E0004]: non-exhaustive patterns: `Some(_)` not covered
 --> src/main.rs:3:19
  |
3 |     let _ = match opt {
  |                   ^^^ pattern `Some(_)` not covered
```

`Some(x)` where `x <= 0` falls through every arm. Add an unguarded arm (e.g. `Some(_) => "non-positive"`) to cover it.

### Pitfall 3: Putting `_` (or a catch-all) too early

In a `switch`, `default` can sit anywhere. In a `match`, arms are tried top-to-bottom, so a catch-all above a specific pattern makes the specific one **unreachable**:

```rust
fn main() {
    let n = 3;
    let label = match n {
        _ => "anything",
        1 => "one", // warning: unreachable
    };
    println!("{label}");
}
```

This compiles but warns:

```text
warning: unreachable pattern
 --> src/main.rs:5:9
  |
4 |         _ => "anything",
  |         - matches any value
5 |         1 => "one",
  |         ^ no value can reach this
  |
  = note: `#[warn(unreachable_patterns)]` on by default
```

Move specific patterns above the catch-all.

### Pitfall 4: Arms must all return the same type

Every arm feeds the same `match` expression, so they must agree on a type:

```rust
fn main() {
    let n = 1;
    // let x = match n {
    //     1 => "one",   // &str
    //     _ => 0,       // i32
    // };  // does not compile (error[E0308]): `match` arms have incompatible types
    let _ = n;
}
```

The fix is to make the arms produce one consistent type (e.g. both `String`, or both `i32`).

### Pitfall 5: Expecting C-style range *case* syntax

There is no `case 1..5:` in `switch`, and the Rust *expression* range `1..5` you use in a `for` loop ([Loops](/04-control-flow/01-loops/)) is **not** the same as a range *pattern*. In a pattern you write `1..=9` (inclusive) or `1..10` (exclusive); you cannot use a runtime range variable as a pattern.

---

## Best Practices

### Match on enums and let exhaustiveness guide you

Model your states as an `enum` and `match` on it. The compiler then guarantees every state is handled, and reminds you to revisit each `match` whenever you add a variant. This is the Rust replacement for discriminated-union `switch`es in TypeScript.

### Prefer explicit arms over `_` for closed enums

For your own enums, list the variants. Reserve `_` for genuinely open-ended values like integers and chars, where enumerating everything is impossible.

### Reach for `if let` when you only care about one case

If a `match` has one meaningful arm and a do-nothing `_ => {}`, an `if let` is cleaner. That is its own topic; see [Concise Pattern Matching](/04-control-flow/03-if-let-while-let/):

```rust
fn main() {
    let config_value: Option<u16> = Some(8080);

    // Instead of: match config_value { Some(port) => {...}, None => {} }
    if let Some(port) = config_value {
        println!("listening on {port}");
    }
}
```

### Use `|`, ranges, and guards to keep arms readable

Group alternatives with `|`, collapse contiguous values into a range, and use a guard for the one extra condition that does not fit the pattern. Keep the heavy logic in a `{ }` block or a helper function so the arm stays scannable.

### Bind with `@` when you need both the test *and* the value

When you want to check that a value is in a range *and* keep the value, `name @ pattern` does both in one step instead of re-reading the variable.

---

## Real-World Example

A tiny command interpreter — the kind you might build for a REPL, a chat bot, or a debug console. It shows `|` patterns, ranges, the `@` binding, guards, destructuring, and nested `match`es all working together to parse and execute typed commands.

```rust
/// A tiny command interpreter, the kind you'd build for a REPL or a chat bot.
#[derive(Debug)]
enum Command {
    Help,
    Echo(String),
    Add(i64, i64),
    SetVolume(u8),
    Quit,
    Unknown(String),
}

/// Parse a raw line into a typed Command.
fn parse(line: &str) -> Command {
    let mut parts = line.trim().splitn(2, ' ');
    let verb = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim();

    match verb {
        "help" | "?" => Command::Help, // `|` = multiple aliases
        "echo" => Command::Echo(rest.to_string()),
        "add" => {
            let mut nums = rest.split_whitespace();
            // Nested match on a tuple of two parse results.
            match (nums.next(), nums.next()) {
                (Some(a), Some(b)) => match (a.parse(), b.parse()) {
                    (Ok(x), Ok(y)) => Command::Add(x, y),
                    _ => Command::Unknown(line.to_string()),
                },
                _ => Command::Unknown(line.to_string()),
            }
        }
        "volume" => match rest.parse::<u8>() {
            // `v @ 0..=100`: bind the value AND require it to be in range.
            Ok(v @ 0..=100) => Command::SetVolume(v),
            _ => Command::Unknown(line.to_string()),
        },
        "quit" | "exit" => Command::Quit,
        _ => Command::Unknown(line.to_string()),
    }
}

/// Execute a command and return the user-facing response.
fn run(cmd: Command) -> String {
    match cmd {
        Command::Help => "commands: help, echo, add, volume, quit".to_string(),
        // Guard: an empty echo is a special case.
        Command::Echo(text) if text.is_empty() => "(nothing to echo)".to_string(),
        Command::Echo(text) => text,
        Command::Add(a, b) => format!("{a} + {b} = {}", a + b),
        Command::SetVolume(v) => format!("volume set to {v}"),
        Command::Quit => "bye!".to_string(),
        Command::Unknown(raw) => format!("unknown command: {raw:?}"),
    }
}

fn main() {
    let session = [
        "help",
        "echo hello there",
        "add 19 23",
        "volume 80",
        "volume 250",
        "dance now",
        "quit",
    ];

    for line in session {
        let cmd = parse(line);
        println!("> {line}");
        println!("  {}", run(cmd));
    }
}
```

Real output:

```text
> help
  commands: help, echo, add, volume, quit
> echo hello there
  hello there
> add 19 23
  19 + 23 = 42
> volume 80
  volume set to 80
> volume 250
  unknown command: "volume 250"
> dance now
  unknown command: "dance now"
> quit
  bye!
```

Notice `volume 250` falls through to `Unknown`: the `v @ 0..=100` pattern rejects `250` because it is out of range, so `parse` returns `Command::Unknown`. The range check is part of the *pattern*, not a separate `if`.

### Bonus: the patterns toolbox

A compact tour of every pattern feature in this file's scope, all in one program:

```rust
#[derive(Debug)]
struct Point {
    x: i32,
    y: i32,
    z: i32,
}

fn char_kind(c: char) -> &'static str {
    match c {
        'a'..='z' => "lowercase", // char ranges work too
        'A'..='Z' => "uppercase",
        '0'..='9' => "digit",
        _ => "other",
    }
}

fn classify(n: i32) -> &'static str {
    match n {
        0 => "zero",
        1 | 2 | 3 => "small",   // `|` alternatives
        4..=9 => "medium",      // inclusive range
        _ if n < 0 => "negative", // guard on the catch-all
        _ => "large",
    }
}

fn main() {
    for c in ['k', 'Q', '7', '#'] {
        println!("{c}: {}", char_kind(c));
    }

    for n in [-5, 0, 2, 7, 42] {
        println!("{n} is {}", classify(n));
    }

    // Destructure a struct; `..` ignores the rest of the fields.
    let p = Point { x: 1, y: 2, z: 3 };
    match p {
        Point { x: 0, y: 0, z: 0 } => println!("at origin"),
        Point { x, .. } => println!("x is {x}, ignoring y and z"),
    }

    // Tuple destructuring with literal + binding patterns.
    let point = (0, 7);
    let desc = match point {
        (0, 0) => "origin".to_string(),
        (x, 0) => format!("on x-axis at {x}"),
        (0, y) => format!("on y-axis at {y}"),
        (x, y) => format!("at ({x}, {y})"),
    };
    println!("{desc}");

    // `@` binding: capture the value while also range-checking it.
    let id = 5;
    let label = match id {
        n @ 1..=9 => format!("single digit: {n}"),
        n @ 10..=99 => format!("double digit: {n}"),
        n => format!("big: {n}"),
    };
    println!("{label}");

    // Slice patterns: first and last with `..` in the middle.
    let numbers = [1, 2, 3];
    match numbers {
        [first, .., last] => println!("first {first}, last {last}"),
    }
}
```

Real output:

```text
k: lowercase
Q: uppercase
7: digit
#: other
-5 is negative
0 is zero
2 is small
7 is medium
42 is large
x is 1, ignoring y and z
on y-axis at 7
single digit: 5
first 1, last 3
```

---

## Further Reading

### Official documentation

- [The Rust Book — The `match` Control Flow Construct](https://doc.rust-lang.org/book/ch06-02-match.html)
- [The Rust Book — Patterns and Matching (Chapter 19)](https://doc.rust-lang.org/book/ch19-00-patterns.html)
- [The Rust Book — All the Places Patterns Can Be Used](https://doc.rust-lang.org/book/ch19-01-all-the-places-for-patterns.html)
- [Rust Reference — Patterns](https://doc.rust-lang.org/reference/patterns.html)
- [Rust by Example — `match`](https://doc.rust-lang.org/rust-by-example/flow_control/match.html)

### Related sections in this guide

- [Conditionals](/04-control-flow/00-conditionals/) — `if`/`else` as expressions; the `if let` teaser.
- [Loops](/04-control-flow/01-loops/) — where expression ranges like `1..5` come from (different from range *patterns*).
- [`if let` and `while let`](/04-control-flow/03-if-let-while-let/) — concise pattern matching for one or two cases.
- [Break, continue, and labeled loops](/04-control-flow/04-break-continue/) — controlling iteration that wraps a `match`.
- [Section 02 — Types](/02-basics/01-types/) — tuples, chars, and the integer types you match on.
- [Section 02 — Operators](/02-basics/02-operators/) — why `match` patterns are not `==` comparisons.
- [Section 05 — Ownership](/05-ownership/) — how matching on a reference (`match &value`) avoids moving the value out.

---

## Exercises

### Exercise 1: FizzBuzz, the pattern-matching way

**Difficulty:** Easy

**Objective:** Practice matching on a tuple and using the `_` wildcard inside a pattern.

**Instructions:** Implement `fizzbuzz(n)` so that it returns `"Fizz"` when `n` is divisible by 3, `"Buzz"` when divisible by 5, `"FizzBuzz"` when divisible by both, and the number itself otherwise. Do it with a single `match` on the tuple `(n % 3, n % 5)`, no `if`/`else`.

```rust
fn fizzbuzz(n: u32) -> String {
    match (n % 3, n % 5) {
        /* ??? */
    }
}

fn main() {
    for n in [1, 3, 5, 15, 7] {
        println!("{n} -> {}", fizzbuzz(n));
    }
}
```

<details>
<summary>Solution</summary>

```rust
fn fizzbuzz(n: u32) -> String {
    match (n % 3, n % 5) {
        (0, 0) => "FizzBuzz".to_string(),
        (0, _) => "Fizz".to_string(), // divisible by 3 only
        (_, 0) => "Buzz".to_string(), // divisible by 5 only
        (_, _) => n.to_string(),      // neither
    }
}

fn main() {
    for n in [1, 3, 5, 15, 7] {
        println!("{n} -> {}", fizzbuzz(n));
    }
}
```

Output:

```text
1 -> 1
3 -> Fizz
5 -> Buzz
15 -> FizzBuzz
7 -> 7
```

The `(0, 0)` arm must come first: order matters, and a number divisible by both 3 and 5 would otherwise be caught by `(0, _)`.

</details>

### Exercise 2: HTTP status categories with ranges

**Difficulty:** Medium

**Objective:** Use inclusive range patterns and a catch-all to classify values.

**Instructions:** Implement `status_category(code: u16)` that returns `"informational"` for `100..=199`, `"success"` for `200..=299`, `"redirect"` for `300..=399`, `"client error"` for `400..=499`, `"server error"` for `500..=599`, and `"invalid"` for anything else.

```rust
fn status_category(code: u16) -> &'static str {
    match code {
        // TODO: fill in the range arms
    }
}

fn main() {
    for code in [101, 204, 301, 404, 503, 999] {
        println!("{code} -> {}", status_category(code));
    }
}
```

<details>
<summary>Solution</summary>

```rust
fn status_category(code: u16) -> &'static str {
    match code {
        100..=199 => "informational",
        200..=299 => "success",
        300..=399 => "redirect",
        400..=499 => "client error",
        500..=599 => "server error",
        _ => "invalid",
    }
}

fn main() {
    for code in [101, 204, 301, 404, 503, 999] {
        println!("{code} -> {}", status_category(code));
    }
}
```

Output:

```text
101 -> informational
204 -> success
301 -> redirect
404 -> client error
503 -> server error
999 -> invalid
```

The `_ => "invalid"` arm is required: `u16` can hold values like `999` and `0` that no range covers, so without it the match would not be exhaustive.

</details>

### Exercise 3: Evaluate an expression tree

**Difficulty:** Hard

**Objective:** Destructure a recursive enum, bind its fields, and combine `match` with a guard and a nested `match` for error handling.

**Instructions:** Given the `Expr` enum below, implement `eval(expr: &Expr) -> Result<f64, String>` that evaluates the tree. Numbers evaluate to themselves; `Add`/`Sub`/`Mul` combine their operands; `Div` returns `Err("division by zero")` when the divisor evaluates to `0.0`. Use the `?` operator to propagate errors from sub-evaluations. (`Box`, `Result`, and `?` are previewed in [Section 03](/03-functions/) and covered fully in [Section 08 — Error Handling](/08-error-handling/).)

```rust
#[derive(Debug)]
enum Expr {
    Num(f64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

fn eval(expr: &Expr) -> Result<f64, String> {
    // TODO: match on `expr`
}

fn main() {
    // (2 + 3) * 4 - 1
    let expr = Expr::Sub(
        Box::new(Expr::Mul(
            Box::new(Expr::Add(Box::new(Expr::Num(2.0)), Box::new(Expr::Num(3.0)))),
            Box::new(Expr::Num(4.0)),
        )),
        Box::new(Expr::Num(1.0)),
    );
    println!("{:?}", eval(&expr));

    let bad = Expr::Div(Box::new(Expr::Num(10.0)), Box::new(Expr::Num(0.0)));
    println!("{:?}", eval(&bad));
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
enum Expr {
    Num(f64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

fn eval(expr: &Expr) -> Result<f64, String> {
    match expr {
        // `Num(n)` binds n: &f64, so dereference with *n.
        Expr::Num(n) => Ok(*n),
        Expr::Add(a, b) => Ok(eval(a)? + eval(b)?),
        Expr::Sub(a, b) => Ok(eval(a)? - eval(b)?),
        Expr::Mul(a, b) => Ok(eval(a)? * eval(b)?),
        // Nested match on the divisor catches division by zero.
        Expr::Div(a, b) => match eval(b)? {
            0.0 => Err("division by zero".to_string()),
            divisor => Ok(eval(a)? / divisor),
        },
    }
}

fn main() {
    // (2 + 3) * 4 - 1
    let expr = Expr::Sub(
        Box::new(Expr::Mul(
            Box::new(Expr::Add(Box::new(Expr::Num(2.0)), Box::new(Expr::Num(3.0)))),
            Box::new(Expr::Num(4.0)),
        )),
        Box::new(Expr::Num(1.0)),
    );
    println!("{:?}", eval(&expr)); // Ok(19.0)

    let bad = Expr::Div(Box::new(Expr::Num(10.0)), Box::new(Expr::Num(0.0)));
    println!("{:?}", eval(&bad)); // Err("division by zero")
}
```

Output:

```text
Ok(19.0)
Err("division by zero")
```

A few things to notice: `eval(a)?` recursively evaluates the sub-tree and short-circuits the whole function if it returns `Err`. Because we matched on `&Expr`, the bindings `a` and `b` are `&Box<Expr>`, which auto-dereference cleanly when passed back into `eval`. The nested `match eval(b)?` is how you inspect a computed value mid-pattern.

</details>
