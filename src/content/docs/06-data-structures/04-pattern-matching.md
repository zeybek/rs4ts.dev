---
title: "Pattern Matching"
description: "Rust's match fuses destructuring, switch, and type narrowing into one exhaustive expression, catching the missing case that TypeScript's switch lets slip."
---

Pattern matching is the feature most TypeScript/JavaScript developers come to miss the moment they go back. Rust's `match` is array/object destructuring, a `switch` statement, type narrowing, and an exhaustiveness checker all rolled into one expression, and the compiler refuses to let you forget a case.

---

## Quick Overview

A **pattern** describes the *shape* of a value: a literal, a range, a tuple, a struct, an enum variant, or a combination of these. Rust lets you match a value against patterns in many places: most prominently the `match` expression, but also `let`, `if let`, `let ... else`, `while let`, and function parameters. The defining feature is **exhaustiveness**: a `match` must cover every possible value, so the compiler turns "I forgot the `null` case" from a 3 a.m. production incident into a build error.

> **Note:** This file focuses on the *patterns* themselves and the *places they appear*: `match`, `let`, `if let`, `let else`, `while let`, and the pattern syntax for tuples, structs, enums, ranges, guards, and bindings. The data types you match on each have their own file: [structs](/06-data-structures/00-structs/), [enums](/06-data-structures/02-enums/), and [`Option<T>`](/06-data-structures/03-option-enum/). Method syntax lives in [impl blocks](/06-data-structures/05-impl-blocks/).

---

## TypeScript/JavaScript Example

TypeScript has no real pattern matching. You reach for a combination of destructuring, `switch`, and manual `if`/`else` chains on a discriminant field, and nothing forces you to handle every case.

```typescript
// A discriminated union — the closest TS analogue to a Rust enum.
type Shape =
  | { kind: "circle"; radius: number }
  | { kind: "rectangle"; width: number; height: number }
  | { kind: "triangle"; base: number; height: number };

function area(shape: Shape): number {
  switch (shape.kind) {
    case "circle":
      return Math.PI * shape.radius ** 2;
    case "rectangle":
      return shape.width * shape.height;
    case "triangle":
      return 0.5 * shape.base * shape.height;
    // With this explicit `: number` return type, TS DOES flag a missing case
    // here (error TS2366: function lacks ending return statement) — no `never`
    // trick required. The `never`-assignment exhaustiveness trick is only needed
    // for side-effect (void) switches, where TS will NOT catch a missing case.
  }
}

// Destructuring is separate from "matching":
const point = { x: 3, y: 7 };
const { x, y } = point; // object destructuring
const [first, , third] = [1, 2, 3]; // array destructuring, skip middle

// Narrowing a value still needs hand-written conditionals:
function classifyStatus(code: number): string {
  if (code === 200) return "OK";
  if (code === 301 || code === 302) return "Redirect";
  if (code >= 400 && code <= 499) return "Client error";
  if (code >= 500 && code <= 599) return "Server error";
  return "Other";
}
```

In TypeScript, destructuring (pulling values *out*) and matching (deciding *which* branch) are two unrelated tools. Exhaustiveness checking is *partial*: for a value-returning `switch` with a declared or inferred return type, TypeScript flags a missing case automatically: either at the function (`error TS2366`, the explicit-return-type case shown above) or at the call site (the inferred-`number | undefined` case, `error TS2322`). But it relies on return-type flow analysis, so a side-effect (`void`) `switch` will silently *miss* a case unless you add the `never`-assignment trick by hand. Rust, as we will see, enforces exhaustiveness structurally for *every* match, no return type or assert needed.

---

## Rust Equivalent

In Rust, destructuring and matching are **the same mechanism**. A pattern simultaneously *tests* a value's shape and *binds* its parts to names. And `match` is exhaustive by construction.

```rust
#[derive(Debug)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Triangle { base: f64, height: f64 },
}

fn area(shape: &Shape) -> f64 {
    // Every variant must be handled, or this will not compile.
    match shape {
        Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
        Shape::Rectangle { width, height } => width * height,
        Shape::Triangle { base, height } => 0.5 * base * height,
    }
}

fn main() {
    // `let` itself takes a pattern — this is destructuring.
    let (name, age) = ("Alice", 30);
    println!("{name} is {age}");

    // Struct destructuring with a `let` pattern.
    #[derive(Debug)]
    struct Point { x: i32, y: i32 }
    let point = Point { x: 3, y: 7 };
    let Point { x, y } = point;
    println!("x={x}, y={y}");

    let shapes = [
        Shape::Circle { radius: 2.0 },
        Shape::Rectangle { width: 3.0, height: 4.0 },
        Shape::Triangle { base: 6.0, height: 2.0 },
    ];
    for s in &shapes {
        println!("{s:?} -> area {:.2}", area(s));
    }
}
```

Output (real, from `cargo run`):

```text
Alice is 30
x=3, y=7
Circle { radius: 2.0 } -> area 12.57
Rectangle { width: 3.0, height: 4.0 } -> area 12.00
Triangle { base: 6.0, height: 2.0 } -> area 6.00
```

> **Note:** The `f64` literals print as `2.0`, not `2`, because Rust's `Debug` formatting for floats always shows the decimal point. This is unlike JavaScript, where `console.log(2.0)` prints `2`.

---

## Detailed Explanation

### `match` is an expression, not a statement

A JavaScript `switch` is a statement: it runs side effects, and you `return` or `break` out. A Rust `match` is an **expression**: it evaluates to a value you can assign or return directly. Each arm is `pattern => expression`.

```rust
fn main() {
    let score = 84;
    let grade = match score {
        90..=100 => 'A',
        80..=89 => 'B',
        70..=79 => 'C',
        _ => 'F',
    };
    println!("grade = {grade}"); // grade = B
}
```

Because `match` is an expression, there is no fall-through and no `break`. Exactly one arm runs, and its value becomes the value of the whole `match`. (Compare to `switch`, where a forgotten `break` causes accidental fall-through, a class of bug that simply cannot exist here.)

### Patterns test *and* bind at once

Each arm's left side is a **pattern**. When the pattern matches, any names inside it are bound to the corresponding pieces of the value. In `Shape::Rectangle { width, height }`, the arm matches only `Rectangle` values, and on a match it binds `width` and `height` to that rectangle's fields. There is no separate "destructure" step.

### Literal, range, and or-patterns

Patterns can be concrete values, inclusive ranges (`..=`), or alternatives joined with `|`:

```rust
fn describe_status(code: u16) -> &'static str {
    match code {
        200 => "OK",
        301 | 302 | 307 | 308 => "Redirect", // or-pattern
        400..=499 => "Client error",         // inclusive range
        500..=599 => "Server error",
        _ => "Other",                         // wildcard catch-all
    }
}

fn main() {
    for code in [200u16, 302, 404, 503, 100] {
        println!("{code} -> {}", describe_status(code));
    }
}
```

Output (real):

```text
200 -> OK
302 -> Redirect
404 -> Client error
503 -> Server error
100 -> Other
```

The `_` wildcard matches anything and binds nothing. It is the equivalent of a `default:` case, and it is how you satisfy exhaustiveness for the "everything else" bucket.

### Match guards: a pattern plus a condition

A pattern can be followed by `if <condition>`, called a **match guard**. The arm matches only if the pattern fits *and* the guard is true.

```rust
#[derive(Debug)]
struct Point { x: i32, y: i32 }

fn classify(point: Point) -> &'static str {
    match point {
        Point { x: 0, y: 0 } => "origin",
        Point { x: 0, y } if y > 0 => "north axis",
        Point { x: 0, y: _ } => "south axis",
        Point { x, y: 0 } if x != 0 => "horizontal axis",
        Point { .. } => "somewhere else",
    }
}
```

Here `Point { x: 0, y }` matches any point on the y-axis and binds the y-coordinate; the guard `if y > 0` then narrows it further. The `Point { .. }` pattern uses `..` to ignore all remaining fields.

### Binding with `@`

Sometimes you want to *test* a value against a range and also *keep* the value. The `@` operator binds a name to a value while also pattern-matching it:

```rust
fn main() {
    let id = 42;
    match id {
        n @ 1..=50 => println!("{n} is in the low range"),
        n @ 51..=100 => println!("{n} is in the high range"),
        n => println!("{n} is out of range"),
    }
    // 42 is in the low range
}
```

Without `@`, the range pattern `1..=50` would match but give you no name for the matched value; with `n @ 1..=50` you get both.

### Reference patterns: matching without moving

This is the part that trips up TypeScript developers, because JavaScript has no concept of ownership. When you `match` a value by reference (`match &thing`), the bindings inside the pattern are also references, so the original value is not consumed:

```rust
fn main() {
    let owned = String::from("config.toml");
    let maybe = Some(owned);

    // Matching `&maybe` borrows; `path` is a `&String`, not a moved String.
    match &maybe {
        Some(path) => println!("path is {path}"),
        None => println!("no path"),
    }

    // We can still use `maybe` afterwards because nothing was moved out.
    println!("still own it: {maybe:?}");
}
```

Output (real):

```text
path is config.toml
still own it: Some("config.toml")
```

Modern Rust applies **match ergonomics**: when you match a reference (`&maybe`) against a non-reference pattern (`Some(path)`), the compiler automatically makes the inner bindings references for you. You rarely need to write the old `ref` keyword anymore. (See [Ownership](/05-ownership/) for the underlying move/borrow rules.)

### Tuple, slice, and nested patterns

Patterns nest arbitrarily, mirroring the structure of the data.

```rust
fn main() {
    // Tuple destructuring, including nested tuples.
    let record = ("Bob", (98.5, 87.0), true);
    let (name, (math, science), active) = record;
    println!("{name}: math={math}, science={science}, active={active}");

    // `..` ignores the middle of a tuple.
    let numbers = (1, 2, 3, 4, 5);
    let (first, .., last) = numbers;
    println!("first={first}, last={last}");

    // Slice patterns match on the *shape* of a slice.
    let parts: &[&str] = &["GET", "/index.html", "HTTP/1.1"];
    match parts {
        [method, path, version] => println!("{method} {path} ({version})"),
        [method, path] => println!("{method} {path}"),
        [] => println!("empty request line"),
        _ => println!("unexpected request line"),
    }

    // Bind the head and capture the rest with `tail @ ..`.
    let words = vec!["a", "b", "c", "d"];
    if let [head, tail @ ..] = words.as_slice() {
        println!("head={head}, tail={tail:?}");
    }
}
```

Output (real):

```text
Bob: math=98.5, science=87, active=true
first=1, last=5
GET /index.html (HTTP/1.1)
head=a, tail=["b", "c", "d"]
```

Slice patterns (`[head, tail @ ..]`) are the closest Rust equivalent to JavaScript's array destructuring with rest (`const [head, ...tail] = arr`), but they also let you match on *length*, something JavaScript destructuring cannot express.

### The other `let`-based pattern forms

`match` is the heavyweight, but four lighter forms cover the common cases:

```rust
fn main() {
    // 1. `if let` — match one pattern, ignore the rest. Like a `match` with
    //    one interesting arm. Equivalent in spirit to TS `if (x !== undefined)`.
    let config: Option<u16> = Some(8080);
    if let Some(port) = config {
        println!("server on port {port}");
    } else {
        println!("using default port");
    }

    // 2. `let ... else` — bind on the happy path, or diverge (return/break/panic).
    //    Great for "early return on failure" without nesting.
    let raw = "127";
    let Ok(n) = raw.parse::<i32>() else {
        println!("not a number");
        return;
    };
    println!("parsed {n}"); // n is in scope for the rest of the function

    // 3. `while let` — loop as long as the pattern keeps matching.
    let mut stack = vec![1, 2, 3];
    while let Some(top) = stack.pop() {
        println!("popped {top}");
    }
}
```

Output (real):

```text
server on port 8080
parsed 127
popped 3
popped 2
popped 1
```

The fourth form is the [`matches!`](https://doc.rust-lang.org/std/macro.matches.html) macro, which returns a `bool` for "does this value match this pattern?", handy in conditions and `.filter()` closures:

```rust
#[allow(dead_code)] // `Click` is never constructed in this isolated snippet
#[derive(Debug)]
enum Event { Click { x: i64, y: i64 }, Close }

fn main() {
    let e = Event::Close;
    println!("is close? {}", matches!(e, Event::Close)); // is close? true
}
```

### Exhaustiveness: the headline feature

A `match` must cover every possible value of the type. This is enforced **structurally**, at compile time, for *every* match — independent of whether the match returns a value or what that value's type is. If you add a variant to an enum later, every `match` that does not handle it stops compiling, and the compiler hands you a to-do list of every place you need to update. TypeScript does catch missing cases in a value-returning `switch` (via return-type flow analysis, as shown earlier), but that safety net disappears for side-effect (`void`) switches unless you hand-write a `never`-assignment assert. Rust needs no such opt-in: exhaustiveness is part of what `match` *is*.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| Branching construct | `switch` statement (with fall-through) | `match` expression (no fall-through) |
| Returns a value | No (statement) | Yes (expression) |
| Destructuring | Separate from branching | Same mechanism as branching |
| Exhaustiveness | Automatic for a value-returning `switch` (return-type flow analysis); needs a `never` assert for `void` switches | Mandatory and structural for every `match` |
| Match on ranges | Manual `if (x >= a && x <= b)` | `a..=b` pattern |
| Match on multiple values | `case a: case b:` fall-through | `a \| b` or-pattern |
| Conditions in a branch | `if` inside the case body | Match guard `if cond` on the arm |
| Bind while testing | Not available | `name @ pattern` |
| Slice/array length match | Not expressible by destructuring | `[a, b, c]` slice patterns |
| Moves a matched value? | N/A (no ownership) | Depends: match by `&value` to borrow |

### Why exhaustiveness instead of a silent default?

Rust's reasoning: a missing case is almost always a bug, and a bug the compiler can see is a bug it should refuse to ship. The `_` wildcard is always available when "everything else" is genuinely intended, but you have to *write it on purpose*, making the choice explicit rather than accidental. TypeScript gets you partway there: a value-returning `switch` with a known return type is checked, but a `void` switch is not unless you opt in with the `never` trick. Rust closes that gap — exhaustiveness applies to every `match` regardless of context, so you opt *out* (with `_`) rather than opt *in* to safety.

### Patterns are refutable or irrefutable

A pattern that can fail to match is **refutable** (`Some(x)`); one that always matches is **irrefutable** (`(a, b)`, `Point { x, y }`). `let` and function parameters require *irrefutable* patterns: `let Some(x) = opt;` is a compile error because it might fail. That is exactly why `if let`, `let else`, and `while let` exist: they are the constructs that *allow* refutable patterns by giving a fallback path.

---

## Common Pitfalls

### Pitfall 1: Forgetting a case (which is the point)

```rust
enum Direction { North, South, East, West }

fn label(d: Direction) -> &'static str {
    // does not compile (error[E0004]: non-exhaustive patterns: `Direction::West` not covered)
    match d {
        Direction::North => "up",
        Direction::South => "down",
        Direction::East => "right",
        // forgot West!
    }
}
```

The real compiler error:

```text
error[E0004]: non-exhaustive patterns: `Direction::West` not covered
 --> src/main.rs:4:11
  |
4 |     match d {
  |           ^ pattern `Direction::West` not covered
  |
note: `Direction` defined here
 --> src/main.rs:1:6
  |
1 | enum Direction { North, South, East, West }
  |      ^^^^^^^^^                       ---- not covered
  = note: the matched value is of type `Direction`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown
  |
7 ~         Direction::East => "right",
8 ~         Direction::West => todo!(),
  |
```

This is not a problem to "fix and forget" — it is the feature working. Add the missing arm. Reach for `_ => ...` only when you truly want a catch-all, and prefer naming variants explicitly so that *adding* a variant later forces you to revisit the match.

### Pitfall 2: A catch-all arm placed too early

Arms are tried top to bottom, so a `_` (or any irrefutable pattern) before more specific arms makes those later arms dead code.

```rust
fn describe(n: i32) -> &'static str {
    match n {
        _ => "anything",
        0 => "zero", // unreachable
    }
}
```

The real compiler warning (this compiles, but warns):

```text
warning: unreachable pattern
 --> src/main.rs:4:9
  |
3 |         _ => "anything",
  |         - matches any value
4 |         0 => "zero",
  |         ^ no value can reach this
  |
  = note: `#[warn(unreachable_patterns)]` on by default
```

Put specific patterns first and the wildcard last.

### Pitfall 3: Matching against a variable instead of comparing to it

This is the trap that surprises every TypeScript developer. In a `switch`, `case expected:` compares against the *value* of `expected`. In a Rust `match`, a bare lowercase identifier is a **new binding** that matches anything — it does *not* compare against an existing variable.

```rust
fn main() {
    let expected = 3;
    let value = 7;
    match value {
        expected => println!("matched expected!"), // binds a NEW `expected`, always matches
        _ => println!("did not match"),
    }
}
```

The compiler warns loudly, and even explains the underlying rule:

```text
warning: unreachable pattern
 --> src/main.rs:6:9
  |
5 |         expected => println!("matched expected!"), // BUG: binds, doesn't compare
  |         -------- matches any value
6 |         _ => println!("did not match"),
  |         ^ no value can reach this
  |
note: there is a binding of the same name; if you meant to pattern match against the value of that binding, that is a feature of constants that is not available for `let` bindings
 --> src/main.rs:2:9
  |
2 |     let expected = 3;
  |         ^^^^^^^^
  = note: `#[warn(unreachable_patterns)]` on by default
```

**Fix** — use a match guard, or match against a `const` (which *does* compare, because `const` names in patterns are treated as values):

```rust
const EXPECTED: i32 = 3;

fn main() {
    let expected = 3;
    let value = 3;

    // Option A: a guard compares against the variable.
    match value {
        v if v == expected => println!("guard matched"),
        _ => println!("no match"),
    }

    // Option B: match against a const (UPPER_SNAKE_CASE) — this DOES compare.
    match value {
        EXPECTED => println!("const matched"),
        _ => println!("no match"),
    }
}
```

Output (real):

```text
guard matched
const matched
```

> **Warning:** This is why Rust's naming convention matters in patterns. `lowercase` = a fresh binding (matches anything); `UPPER_SNAKE_CASE` (a `const`) = a value comparison. The compiler relies on this casing distinction.

### Pitfall 4: Using a refutable pattern in `let`

```rust
fn main() {
    let opt: Option<i32> = Some(5);
    // does not compile (error[E0005]: refutable pattern in local binding)
    let Some(x) = opt;
    println!("{x}");
}
```

A plain `let` requires a pattern that *always* matches. `Some(x)` might be `None`, so it is refutable. Use `if let`, `let ... else`, or a full `match` instead:

```rust
fn main() {
    let opt: Option<i32> = Some(5);
    let Some(x) = opt else {
        println!("was None");
        return;
    };
    println!("{x}");
}
```

---

## Best Practices

- **Prefer explicit variant arms over `_` for enums you own.** A wildcard silences the exhaustiveness check, so adding a variant later will *not* flag the match. Use `_` for open-ended primitive matches (status codes, characters) where listing everything is impossible.
- **Order arms specific-to-general.** Put literals and tight ranges first, the wildcard last, to avoid unreachable-pattern warnings.
- **Use `if let` / `let else` for single-pattern checks**, and reserve full `match` for genuine multi-way branching. `let else` in particular flattens "parse-or-bail" logic that would otherwise nest deeply.
- **Match by reference (`match &value`) when you do not need ownership**, so the matched value remains usable afterward. Let match ergonomics infer the reference bindings rather than sprinkling `ref`.
- **Reach for `matches!(value, Pattern)`** instead of a `match` that just returns `true`/`false`.
- **Use match guards for conditions that patterns cannot express** (relationships between bindings, `x == some_var`), but keep them simple — a guard is not a place for heavy logic.
- **Let the compiler be your refactoring tool.** When you add an enum variant, do *not* add a `_` arm to silence errors; let each broken `match` guide you to every place that needs updating.

---

## Real-World Example

A small HTTP-style request router driven entirely by pattern matching. It exercises tuple matching, struct and enum destructuring, reference bindings, guards, and an exhaustive response mapping: the kind of dispatch logic you would otherwise write as a tangle of `if`/`else` in Express.

```rust
// A tiny HTTP-style request router driven entirely by pattern matching.
// Demonstrates: enum + struct destructuring, tuple matching, ref bindings,
// guards, and exhaustiveness.

#[derive(Debug)]
enum Method {
    Get,
    Post,
    Delete,
}

#[derive(Debug)]
struct Request {
    method: Method,
    path: String,
    body: Option<String>, // Some(body) for POST, None otherwise.
}

#[derive(Debug)]
enum Response {
    Ok(String),
    Created(String),
    NotFound,
    BadRequest(String),
}

fn route(req: &Request) -> Response {
    // Match on a tuple of (&method, path-as-&str, &body). `.as_str()` lets us
    // write string-literal patterns; matching by reference avoids moving `req`.
    match (&req.method, req.path.as_str(), &req.body) {
        (Method::Get, "/health", _) => Response::Ok("healthy".into()),

        // Path parameter extracted by hand, validated with a guard.
        (Method::Get, path, _) if path.starts_with("/users/") => {
            match path.trim_start_matches("/users/").parse::<u32>() {
                Ok(id) => Response::Ok(format!("user #{id}")),
                Err(_) => Response::BadRequest("invalid user id".into()),
            }
        }

        // Create requires a body; bind it with `Some(b)`.
        (Method::Post, "/users", Some(b)) => Response::Created(format!("created: {b}")),
        (Method::Post, "/users", None) => Response::BadRequest("missing body".into()),

        (Method::Delete, path, _) if path.starts_with("/users/") => {
            Response::Ok(format!("deleted {path}"))
        }

        // Catch-all: every other (method, path) combination.
        _ => Response::NotFound,
    }
}

fn status_line(res: &Response) -> String {
    // Exhaustive: every Response variant is mapped to a status line.
    match res {
        Response::Ok(msg) => format!("200 OK: {msg}"),
        Response::Created(msg) => format!("201 Created: {msg}"),
        Response::NotFound => "404 Not Found".to_string(),
        Response::BadRequest(why) => format!("400 Bad Request: {why}"),
    }
}

fn main() {
    let requests = [
        Request { method: Method::Get, path: "/health".into(), body: None },
        Request { method: Method::Get, path: "/users/42".into(), body: None },
        Request { method: Method::Get, path: "/users/abc".into(), body: None },
        Request { method: Method::Post, path: "/users".into(), body: Some("Ada".into()) },
        Request { method: Method::Post, path: "/users".into(), body: None },
        Request { method: Method::Delete, path: "/users/7".into(), body: None },
        Request { method: Method::Get, path: "/unknown".into(), body: None },
    ];

    for req in &requests {
        let res = route(req);
        println!(
            "{:<6} {:<12} -> {}",
            format!("{:?}", req.method),
            req.path,
            status_line(&res)
        );
    }
}
```

Output (real, from `cargo run`):

```text
Get    /health      -> 200 OK: healthy
Get    /users/42    -> 200 OK: user #42
Get    /users/abc   -> 400 Bad Request: invalid user id
Post   /users       -> 201 Created: created: Ada
Post   /users       -> 400 Bad Request: missing body
Delete /users/7     -> 200 OK: deleted /users/7
Get    /unknown     -> 404 Not Found
```

The router compiles cleanly under `cargo clippy` with no warnings. Notice how matching the `(method, path, body)` tuple collapses what would be a nested `if`/`switch` mess into one flat, exhaustive decision table, and the `status_line` match guarantees you can never add a `Response` variant without giving it a status line.

---

## Further Reading

### Official Documentation

- [The Rust Book — The `match` Control Flow Construct](https://doc.rust-lang.org/book/ch06-02-match.html)
- [The Rust Book — Concise Control Flow with `if let` and `let ... else`](https://doc.rust-lang.org/book/ch06-03-if-let.html)
- [The Rust Book — Patterns and Matching (full chapter)](https://doc.rust-lang.org/book/ch19-00-patterns.html)
- [Rust Reference — Patterns](https://doc.rust-lang.org/reference/patterns.html)
- [Rust by Example — Flow of Control: `match`](https://doc.rust-lang.org/rust-by-example/flow_control/match.html)
- [`std::matches!` macro](https://doc.rust-lang.org/std/macro.matches.html)

### Related Topics in This Guide

- [Enums](/06-data-structures/02-enums/) — the data-carrying types you match on most often.
- [Option Enum](/06-data-structures/03-option-enum/) — `Some`/`None` patterns and combinators that often replace a `match`.
- [Structs](/06-data-structures/00-structs/) — defining the struct shapes you destructure here.
- [Tuple Structs](/06-data-structures/01-tuple-structs/) — matching tuple-struct and newtype patterns.
- [impl Blocks](/06-data-structures/05-impl-blocks/) — where `&self` matching commonly lives.
- [Basic Types](/02-basics/01-types/): tuples and the unit type, introduced earlier.
- [Ownership](/05-ownership/): why matching by reference vs. by value matters.
- [Collections](/07-collections/): `while let` with iterators and slice patterns over `Vec`.

---

## Exercises

### Exercise 1: Traffic Light Action

**Difficulty:** Easy

**Objective:** Write an exhaustive `match` over an enum.

**Instructions:** Given the `TrafficLight` enum below, implement `action` so that `Red` returns `"stop"`, `Yellow` returns `"slow down"`, and `Green` returns `"go"`. Do *not* use a `_` wildcard — handle each variant explicitly so a future variant would force you to revisit the match.

```rust
#[derive(Debug)]
enum TrafficLight {
    Red,
    Yellow,
    Green,
}

fn action(light: &TrafficLight) -> &'static str {
    // TODO: match on every variant
    /* ??? */
}

fn main() {
    for l in [TrafficLight::Red, TrafficLight::Yellow, TrafficLight::Green] {
        println!("{l:?} -> {}", action(&l));
    }
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
enum TrafficLight {
    Red,
    Yellow,
    Green,
}

fn action(light: &TrafficLight) -> &'static str {
    match light {
        TrafficLight::Red => "stop",
        TrafficLight::Yellow => "slow down",
        TrafficLight::Green => "go",
    }
}

fn main() {
    for l in [TrafficLight::Red, TrafficLight::Yellow, TrafficLight::Green] {
        println!("{l:?} -> {}", action(&l));
    }
}
```

Output:

```text
Red -> stop
Yellow -> slow down
Green -> go
```

</details>

### Exercise 2: Classify a Coordinate

**Difficulty:** Medium

**Objective:** Combine struct patterns, literal patterns, and match guards.

**Instructions:** Implement `quadrant` for the `Coord` struct. Return `"origin"` for `(0, 0)`, `"on y-axis"` when `x == 0`, `"on x-axis"` when `y == 0`, and `"quadrant I"`/`"II"`/`"III"`/`"IV"` for the four quadrants. Match by reference and use guards for the quadrant comparisons.

```rust
#[derive(Debug)]
struct Coord {
    x: i32,
    y: i32,
}

fn quadrant(c: &Coord) -> &'static str {
    // TODO
    /* ??? */
}

fn main() {
    for c in [
        Coord { x: 0, y: 0 },
        Coord { x: 0, y: 5 },
        Coord { x: 5, y: 0 },
        Coord { x: 2, y: 3 },
        Coord { x: -2, y: 3 },
        Coord { x: -2, y: -3 },
        Coord { x: 2, y: -3 },
    ] {
        println!("{c:?} -> {}", quadrant(&c));
    }
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
struct Coord {
    x: i32,
    y: i32,
}

fn quadrant(c: &Coord) -> &'static str {
    match c {
        Coord { x: 0, y: 0 } => "origin",
        Coord { x: 0, .. } => "on y-axis",
        Coord { y: 0, .. } => "on x-axis",
        // When matching `&Coord`, the bindings are `&i32`, so deref in guards.
        Coord { x, y } if *x > 0 && *y > 0 => "quadrant I",
        Coord { x, y } if *x < 0 && *y > 0 => "quadrant II",
        Coord { x, y } if *x < 0 && *y < 0 => "quadrant III",
        _ => "quadrant IV",
    }
}

fn main() {
    for c in [
        Coord { x: 0, y: 0 },
        Coord { x: 0, y: 5 },
        Coord { x: 5, y: 0 },
        Coord { x: 2, y: 3 },
        Coord { x: -2, y: 3 },
        Coord { x: -2, y: -3 },
        Coord { x: 2, y: -3 },
    ] {
        println!("{c:?} -> {}", quadrant(&c));
    }
}
```

Output:

```text
Coord { x: 0, y: 0 } -> origin
Coord { x: 0, y: 5 } -> on y-axis
Coord { x: 5, y: 0 } -> on x-axis
Coord { x: 2, y: 3 } -> quadrant I
Coord { x: -2, y: 3 } -> quadrant II
Coord { x: -2, y: -3 } -> quadrant III
Coord { x: 2, y: -3 } -> quadrant IV
```

</details>

### Exercise 3: Summarize a JSON-like Value

**Difficulty:** Hard

**Objective:** Match a recursive enum with data-carrying variants, including a guard that distinguishes an empty array.

**Instructions:** Given the `Json` enum, implement `summarize` so that `Null` -> `"null"`, `Bool(b)` -> `"bool(true)"`/`"bool(false)"`, `Number(n)` -> `"number(3.5)"`, `Text(s)` -> `"text(N chars)"` using the string's length, an empty `Array` -> `"empty array"`, and a non-empty `Array` -> `"array of N"`. Match the values by reference.

```rust
#[derive(Debug)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Json>),
}

fn summarize(value: &Json) -> String {
    // TODO
    /* ??? */
}

fn main() {
    let doc = Json::Array(vec![
        Json::Null,
        Json::Bool(true),
        Json::Number(3.5),
        Json::Text("hi".into()),
        Json::Array(vec![]),
    ]);
    if let Json::Array(items) = &doc {
        for item in items {
            println!("{}", summarize(item));
        }
    }
    println!("top: {}", summarize(&doc));
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Json>),
}

fn summarize(value: &Json) -> String {
    match value {
        Json::Null => "null".to_string(),
        Json::Bool(b) => format!("bool({b})"),
        Json::Number(n) => format!("number({n})"),
        Json::Text(s) => format!("text({} chars)", s.len()),
        // A guard distinguishes the empty case before the general one.
        Json::Array(items) if items.is_empty() => "empty array".to_string(),
        Json::Array(items) => format!("array of {}", items.len()),
    }
}

fn main() {
    let doc = Json::Array(vec![
        Json::Null,
        Json::Bool(true),
        Json::Number(3.5),
        Json::Text("hi".into()),
        Json::Array(vec![]),
    ]);
    if let Json::Array(items) = &doc {
        for item in items {
            println!("{}", summarize(item));
        }
    }
    println!("top: {}", summarize(&doc));
}
```

Output:

```text
null
bool(true)
number(3.5)
text(2 chars)
empty array
top: array of 5
```

> **Tip:** The ordering of the two `Json::Array` arms matters: the guarded `is_empty()` arm must come first, because the unguarded `Json::Array(items)` arm would otherwise match every array and make the empty case unreachable.

</details>
