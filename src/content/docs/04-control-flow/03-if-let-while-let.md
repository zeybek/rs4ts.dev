---
title: "Concise Pattern Matching: `if let`, `while let`, and `let ... else`"
description: "When only one case matters, if let, while let, and let-else fuse the check and binding in one step, replacing TypeScript's if (x !== null) narrowing."
---

A full [`match`](/04-control-flow/02-match/) is wonderful when you genuinely care about *every* variant, but a lot of real code only cares about *one* case: "if this `Option` is `Some`, use it." Rust gives you three lightweight tools for exactly that — `if let`, `while let`, and `let ... else` — and they will quickly become the workhorses you reach for when you would have written `if (x !== null)` in TypeScript.

---

## Quick Overview

`if let` runs a block only when a value matches a single pattern, binding the inner data in one step. Think of it as `if` fused with destructuring. `while let` is the loop version: keep looping as long as the pattern keeps matching. `let ... else` (often called **let-else**) binds a pattern unconditionally and runs a diverging `else` block (like `return` or `break`) when it *doesn't* match, which is the cleanest way to do early-return "guard clauses" in Rust.

> **Note:** These are all *syntactic sugar* over `match`. Learning them after [`match`](/04-control-flow/02-match/) makes the sugar obvious; learning them first makes `match` feel like a natural generalization. Either order is fine.

---

## TypeScript/JavaScript Example

In TypeScript, you constantly narrow a possibly-absent value before using it. There is no destructuring-in-the-condition, so you check first and read the field on the next line.

```typescript
// TypeScript: narrow-then-use, the everyday pattern
interface Session {
  userId: number;
  token: string;
}

function currentSession(): Session | null {
  return Math.random() > 0.5 ? { userId: 7, token: "abc" } : null;
}

const session = currentSession();
if (session !== null) {
  // `session` is narrowed to Session inside this block
  console.log(`User ${session.userId} is logged in`);
} else {
  console.log("No active session");
}

// "Keep going while there's work" — the while-let shape
const stack: number[] = [1, 2, 3, 4, 5];
let top: number | undefined;
while ((top = stack.pop()) !== undefined) {
  console.log(`popped ${top}`);
}

// Guard clause / early return to avoid nesting
function parsePort(input: string): string {
  const port = Number(input);
  if (!Number.isInteger(port)) {
    return `'${input}' is not a valid port`;
  }
  // `port` is usable for the rest of the function
  return `listening on port ${port}`;
}
```

Notice three friction points a Rust developer will recognize: you re-read `session` after narrowing, the `while` loop needs an extra mutable `top` declared *outside* the loop, and the guard clause re-checks a condition rather than binding the success value directly.

---

## Rust Equivalent

Rust folds the check and the binding together. Each block below is **compile-verified**.

```rust playground
#[derive(Debug)]
struct Session {
    user_id: u32,
    token: String,
}

fn current_session() -> Option<Session> {
    Some(Session { user_id: 7, token: "abc".to_string() })
}

fn parse_port(input: &str) -> String {
    // let-else: bind on success, diverge on failure — no re-check, no nesting
    let Ok(port) = input.parse::<u16>() else {
        return format!("'{input}' is not a valid port");
    };
    format!("listening on port {port}")
}

fn main() {
    // if let: check + destructure in one step
    if let Some(session) = current_session() {
        println!("User {} is logged in", session.user_id);
    } else {
        println!("No active session");
    }

    // while let: loop as long as `pop()` keeps returning Some
    let mut stack = vec![1, 2, 3, 4, 5];
    while let Some(top) = stack.pop() {
        println!("popped {top}");
    }

    println!("{}", parse_port("8080"));
    println!("{}", parse_port("not-a-port"));
}
```

Running it prints:

```text
User 7 is logged in
popped 5
popped 4
popped 3
popped 2
popped 1
listening on port 8080
'not-a-port' is not a valid port
```

The mutable `top` variable that TypeScript needed outside the loop is gone. `while let` declares and binds `top` fresh on every iteration, scoped to the loop body.

---

## Detailed Explanation

### `if let`: "match one pattern, ignore the rest"

`if let PATTERN = EXPRESSION { ... }` evaluates `EXPRESSION`, tries to match it against `PATTERN`, and runs the block only on a match. On a match, any variables in the pattern are bound and **scoped to the block**.

```rust playground
fn main() {
    let maybe_user: Option<&str> = Some("Ada");

    if let Some(name) = maybe_user {
        // `name` is the &str inside the Some, only visible here
        println!("Welcome, {name}!");
    } else {
        // optional else, runs on no-match
        println!("No user logged in");
    }
}
```

This is exactly equivalent to the more verbose:

```rust playground
fn main() {
    let maybe_user: Option<&str> = Some("Ada");

    match maybe_user {
        Some(name) => println!("Welcome, {name}!"),
        None => println!("No user logged in"),
    }
}
```

`if let` shines when one of the `match` arms would be a do-nothing `_ => {}`. It works on *any* pattern, not just `Option`: enums, structs, tuples, and ranges all work:

```rust playground
#[derive(Debug)]
enum Event {
    Click { x: i32, y: i32 },
    KeyPress(char),
    Close,
}

fn main() {
    let event = Event::Click { x: 10, y: 20 };
    // Struct-variant destructuring directly in the condition
    if let Event::Click { x, y } = event {
        println!("Clicked at ({x}, {y})");
    }

    let key = Event::KeyPress('q');
    if let Event::KeyPress(c) = key {
        println!("Key pressed: {c}");
    }

    let _ = Event::Close; // (just to use the variant)
}
```

This prints:

```text
Clicked at (10, 20)
Key pressed: q
```

### `else if let` chains

You can chain alternatives with `else if let`, mixing in regular `else if` and a final `else`:

```rust playground
fn main() {
    let setting: Option<i32> = None;
    let fallback: Option<i32> = Some(42);

    if let Some(v) = setting {
        println!("setting = {v}");
    } else if let Some(v) = fallback {
        println!("fallback = {v}");
    } else {
        println!("nothing set");
    }
}
```

This prints `fallback = 42`. Each `if let` tries its own pattern; the first match wins.

### `if let` chains with `&&` (Rust 2024)

Since the latest stable edition (2024, stabilized in Rust 1.88), you can join a pattern match and ordinary boolean conditions with `&&` in a single `if let`. All conditions must hold, and bindings from earlier links are visible to later ones:

```rust playground
fn main() {
    let opt: Option<i32> = Some(7);
    let flag = true;

    // let-chain: pattern match AND boolean tests together
    if let Some(n) = opt && n > 5 && flag {
        println!("matched and n={n} > 5");
    }
}
```

This prints `matched and n=7 > 5`. Before this feature you had to nest an `if let` inside an `if`, or push the extra condition into a `match` guard.

> **Note:** Let-chains require the 2024 edition. A fresh `cargo new` selects it automatically, so you get this for free in new projects.

### `while let`: loop while the pattern keeps matching

`while let PATTERN = EXPRESSION { ... }` re-evaluates `EXPRESSION` before every iteration and stops the first time it does *not* match. It is the idiomatic way to drain anything that yields `Option`:

```rust playground
fn main() {
    // Drain a stack: pop() returns Some(x) until empty, then None
    let mut stack = vec![1, 2, 3];
    while let Some(top) = stack.pop() {
        println!("popped {top}");
    }

    // Pull values straight from an iterator
    let mut numbers = vec![10, 20, 30].into_iter();
    while let Some(n) = numbers.next() {
        println!("got {n}");
    }
}
```

> **Tip:** A `while let ... = iter.next()` loop is almost always better written as a plain `for n in iter { ... }`, which handles the `next()`/`None` dance for you. Reach for `while let` when the source is a mutable container you are *consuming* (like `stack.pop()` or `channel.recv()`), where `for` does not directly apply. See [Loops](/04-control-flow/01-loops/) for the `for`/`while`/`loop` trio.

### `let ... else`: early-return guard clauses

`let PATTERN = EXPRESSION else { ... }` binds `PATTERN` for the **rest of the enclosing scope** when it matches. When it does *not* match, the `else` block runs, and that block **must diverge**, meaning it has to leave the current scope via `return`, `break`, `continue`, or a `panic!`. This is Rust's answer to the guard-clause / early-return style.

```rust playground
fn first_word(text: &str) -> &str {
    let Some(word) = text.split_whitespace().next() else {
        return "<empty>";
    };
    // `word` is in scope here, with no extra indentation
    word
}

fn main() {
    println!("{}", first_word("hello world")); // hello
    println!("{}", first_word("   "));          // <empty>
}
```

The key difference from `if let`: with `if let`, the binding lives *inside* the block. With `let ... else`, the binding lives *after* the statement, in the surrounding scope — which is precisely what you want for "validate, then proceed with the validated value." Compare the shapes:

```rust
// if let: success path is INSIDE, nesting grows with each check
fn describe_if_let(input: &str) -> String {
    if let Ok(n) = input.parse::<i32>() {
        if n > 0 {
            return format!("positive: {n}");
        }
    }
    "invalid".to_string()
}

// let-else: success path is the MAIN body, failures bail early
fn describe_let_else(input: &str) -> String {
    let Ok(n) = input.parse::<i32>() else {
        return "invalid".to_string();
    };
    if n <= 0 {
        return "invalid".to_string();
    }
    format!("positive: {n}")
}
```

`let ... else` also works mid-loop, where the divergence is `continue` or `break`:

```rust playground
fn sum_even_strings(items: &[&str]) -> i32 {
    let mut total = 0;
    for item in items {
        // Skip anything that isn't a number, without nesting the happy path
        let Ok(n) = item.parse::<i32>() else {
            continue;
        };
        if n % 2 == 0 {
            total += n;
        }
    }
    total
}

fn main() {
    println!("{}", sum_even_strings(&["2", "x", "4", "5", "6"])); // 12
}
```

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| Narrow-then-use | `if (x !== null) { use x }`, re-read `x` | `if let Some(v) = x { use v }`, binds in one step |
| Binding scope on success | Whole block after the narrowing check | `if let`: inside block; `let ... else`: rest of scope |
| Loop while value present | `while ((v = next()) !== undefined)` with outer `let` | `while let Some(v) = next()`, fresh binding each pass |
| Early-return guard | `if (!ok) return;` then re-fetch the value | `let Ok(v) = ... else { return; };` binds `v` directly |
| Combine match + boolean | nest `if` inside `if` | `if let P = x && cond` (let-chains, edition 2024) |
| What you can match | runtime `typeof` / property checks | any pattern: enums, structs, tuples, ranges, `@` bindings |

### The conceptual shift

In TypeScript you *test* a value and the compiler *narrows* its static type for the rest of the block. In Rust you *match a pattern* and the compiler *binds* the inner data. The mechanisms differ, but the ergonomics line up: `if let Some(v) = opt` feels like `if (opt != null)`, and `let Some(v) = opt else { return; }` feels like `if (opt == null) return;`.

Unlike TypeScript, the success binding from `if let` does **not** leak past the closing brace. There is no flow-based narrowing that survives the block. When you want the value afterward, that is the signal to use `let ... else` instead.

### `if let` is refutable; `let` is irrefutable

A plain `let x = ...;` requires an **irrefutable** pattern — one that always matches (like a bare name or a tuple of names). `if let`, `while let`, and the pattern in `let ... else` accept **refutable** patterns — ones that might fail (like `Some(x)` or `Ok(x)`). That refutability is exactly what gives them a "didn't match" path to run.

---

## Common Pitfalls

### Pitfall 1: Expecting the `if let` binding to survive the block

A TypeScript developer expects the narrowed value to be usable after the `if`. In Rust the binding is scoped to the block and disappears.

```rust
fn main() {
    let maybe_name: Option<&str> = Some("Grace");
    if let Some(name) = maybe_name {
        println!("inside: {name}");
    }
    // `name` only existed inside the if-let block:
    println!("outside: {name}"); // does not compile (error[E0425])
}
```

The real compiler error:

```text
error[E0425]: cannot find value `name` in this scope
 --> src/main.rs:7:25
  |
7 |     println!("outside: {name}");
  |                         ^^^^ not found in this scope
```

**Fix:** if you need the value afterward, use `let ... else`, which binds for the rest of the scope:

```rust playground
fn main() {
    let maybe_name: Option<&str> = Some("Grace");
    let Some(name) = maybe_name else {
        println!("no name");
        return;
    };
    println!("inside or outside, {name} is in scope now");
}
```

### Pitfall 2: A `let ... else` block that doesn't diverge

The `else` block must leave the scope. If it just prints and falls through, the compiler rejects it, because `name` would otherwise be unbound on that path.

```rust
fn get_count(input: &str) -> i32 {
    let Ok(n) = input.parse::<i32>() else {
        println!("not a number");
        // does not compile (error[E0308]): forgot to diverge
    };
    n
}

fn main() {
    println!("{}", get_count("5"));
}
```

The real compiler error:

```text
error[E0308]: `else` clause of `let...else` does not diverge
 --> src/main.rs:2:43
  |
2 |       let Ok(n) = input.parse::<i32>() else {
  |  ___________________________________________^
3 | |         println!("not a number");
4 | |         // forgot to diverge (no return/break/panic)
5 | |     };
  | |_____^ expected `!`, found `()`
  |
  = note:   expected type `!`
          found unit type `()`
  = help: try adding a diverging expression, such as `return` or `panic!(..)`
  = help: ...or use `match` instead of `let...else`
```

**Fix:** end the `else` with `return`, `break`, `continue`, or `panic!` (or another expression of the never type `!`).

### Pitfall 3: Using `if let` with a pattern that always matches

If the pattern can never fail, `if let` is pointless, and Clippy/rustc will tell you so with a built-in lint.

```rust playground
fn main() {
    let point = (3, 4);
    // A tuple pattern always matches a tuple -> irrefutable
    if let (x, y) = point { // irrefutable_let_patterns warning
        println!("{x}, {y}");
    }
}
```

The real warning:

```text
warning: irrefutable `if let` pattern
 --> src/main.rs:4:8
  |
4 |     if let (x, y) = point {
  |        ^^^^^^^^^^^^^^^^^^
  |
  = note: this pattern will always match, so the `if let` is useless
  = help: consider replacing the `if let` with a `let`
  = note: `#[warn(irrefutable_let_patterns)]` on by default
```

**Fix:** use a plain `let (x, y) = point;`; no conditional needed.

### Pitfall 4: `if let` where every variant matters

`if let` silently ignores the non-matching cases. That is a feature when you truly do not care, but a bug when you do. If you find yourself writing `else if let ... else if let ...` over an enum's variants, you have re-implemented `match` badly and lost its exhaustiveness checking. Prefer [`match`](/04-control-flow/02-match/) when you want the compiler to force you to handle every case.

---

## Best Practices

### Reach for the lightest tool that fits

- **One case matters, nothing needed afterward → `if let`** (optionally with `else`).
- **One case matters, value needed for the rest of the scope → `let ... else`.**
- **Repeatedly consume a source that yields `Option` → `while let`** (but prefer `for` when iterating).
- **Every variant matters → [`match`](/04-control-flow/02-match/).**

### Use `let ... else` to flatten guard clauses

Early returns keep the happy path un-indented. This scales far better than a pyramid of nested `if let`s:

```rust playground
fn extract_user_id(header: &str) -> Result<u64, String> {
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Err("missing Bearer prefix".to_string());
    };
    let Ok(id) = token.parse::<u64>() else {
        return Err(format!("invalid id: {token}"));
    };
    Ok(id)
}

fn main() {
    println!("{:?}", extract_user_id("Bearer 42"));    // Ok(42)
    println!("{:?}", extract_user_id("Basic 42"));     // Err("missing Bearer prefix")
    println!("{:?}", extract_user_id("Bearer notanum")); // Err("invalid id: notanum")
}
```

> **Tip:** Inside a function returning `Result` or `Option`, the [`?` operator](/03-functions/) is often even shorter than `let ... else` for plain "propagate the error" cases. Use `let ... else` when you need a *custom* failure action (a different error, a log line, a `continue`), not just propagation.

### Combine conditions with let-chains instead of nesting

When you need a pattern match *and* an extra check, a let-chain (`if let P = x && cond`) reads better than nesting and keeps both the binding and the condition in one place.

### Don't reach for `if let` over an exhaustive enum

If the compiler could be checking exhaustiveness for you, let it. `match` turns "I forgot the `Disconnected` variant" into a compile error; a chain of `if let`s turns it into a silent runtime no-op.

---

## Real-World Example

A small command interpreter that reads lines, parses each into a typed `Command`, and builds up a profile. It uses `while let` to drain a work queue, `if let` to react only to recognized commands, `let ... else` (via `?`) inside the parser, and an `if let` with a tuple pattern at the end. This compiles and runs as-is.

```rust playground
use std::collections::VecDeque;

/// A command parsed from a line of user input.
#[derive(Debug)]
enum Command {
    SetName(String),
    SetAge(u32),
    Quit,
}

/// Parse one line into a `Command`, or `None` if it is not recognized.
fn parse_command(line: &str) -> Option<Command> {
    let mut parts = line.split_whitespace();
    let verb = parts.next()?; // `?` returns None if the line is empty
    match verb {
        "name" => {
            let rest = parts.next()?;
            Some(Command::SetName(rest.to_string()))
        }
        "age" => {
            // let-else: bail out of THIS function cleanly on a bad number
            let Ok(age) = parts.next()?.parse::<u32>() else {
                return None;
            };
            Some(Command::SetAge(age))
        }
        "quit" => Some(Command::Quit),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct Profile {
    name: Option<String>,
    age: Option<u32>,
}

fn main() {
    let input = [
        "name Ada",
        "age 36",
        "age not-a-number",
        "unknown verb",
        "quit",
    ];

    // Feed lines into a queue and drain it with `while let`.
    let mut queue: VecDeque<&str> = input.iter().copied().collect();
    let mut profile = Profile::default();

    while let Some(line) = queue.pop_front() {
        // `if let` to react only when parsing succeeds.
        if let Some(command) = parse_command(line) {
            match command {
                Command::SetName(name) => profile.name = Some(name),
                Command::SetAge(age) => profile.age = Some(age),
                Command::Quit => {
                    println!("Quit received, stopping.");
                    break;
                }
            }
        } else {
            println!("Ignoring unrecognized line: {line:?}");
        }
    }

    // `if let` with a tuple pattern: only print once BOTH fields are present.
    if let (Some(name), Some(age)) = (&profile.name, profile.age) {
        println!("Profile complete: {name}, age {age}");
    } else {
        println!("Profile incomplete: {profile:?}");
    }
}
```

Output:

```text
Ignoring unrecognized line: "age not-a-number"
Ignoring unrecognized line: "unknown verb"
Quit received, stopping.
Profile complete: Ada, age 36
```

Notice how `"age 36"` parses, `"age not-a-number"` is rejected by the `let ... else` inside `parse_command` (which returns `None`), and the loop stops cleanly at `"quit"` via `break` — three different control-flow idioms cooperating in one tidy loop.

---

## Further Reading

### Official Documentation

- [The Rust Book — Concise Control Flow with `if let` and `let ... else`](https://doc.rust-lang.org/book/ch06-03-if-let.html)
- [The Rust Reference — `if let` expressions](https://doc.rust-lang.org/reference/expressions/if-expr.html#if-let-expressions)
- [The Rust Reference — `let` statements (`let ... else`)](https://doc.rust-lang.org/reference/statements.html#let-statements)
- [Rust by Example — `if let`](https://doc.rust-lang.org/rust-by-example/flow_control/if_let.html) and [`while let`](https://doc.rust-lang.org/rust-by-example/flow_control/while_let.html)

### Related Sections in This Guide

- [Conditionals](/04-control-flow/00-conditionals/): `if`/`else` as an expression and why Rust has no truthiness
- [Loops](/04-control-flow/01-loops/): `for`, `while`, and `loop`, and when to prefer `for` over `while let`
- [`match`](/04-control-flow/02-match/): the full pattern-matching expression these tools are sugar for
- [`break` and `continue`](/04-control-flow/04-break-continue/): the divergence options for a `let ... else` inside a loop
- [Labeled loops](/04-control-flow/05-labeled-loops/) — breaking out of nested `while let` loops
- [Functions and the `?` operator](/03-functions/) — the other tool for early returns
- [Variables and Mutability](/02-basics/00-variables/): how binding and scope work in Rust
- [Ownership](/05-ownership/) — why a binding's scope matters for borrows and moves

---

## Exercises

### Exercise 1: Greet or fall back

**Difficulty:** Easy

**Objective:** Practice `if let` with an `else` branch.

**Instructions:** Implement `greet` so that `greet(Some("Lin"))` prints `Hello, Lin!` and `greet(None)` prints `Hello, guest!`.

```rust playground
fn greet(user: Option<&str>) {
    // TODO: use `if let` with an `else`
}

fn main() {
    greet(Some("Lin"));
    greet(None);
}
```

<details>
<summary>Solution</summary>

```rust playground
fn greet(user: Option<&str>) {
    if let Some(name) = user {
        println!("Hello, {name}!");
    } else {
        println!("Hello, guest!");
    }
}

fn main() {
    greet(Some("Lin")); // Hello, Lin!
    greet(None);        // Hello, guest!
}
```

</details>

### Exercise 2: Drain a job queue

**Difficulty:** Medium

**Objective:** Use `while let` to consume a container until it is empty.

**Instructions:** Implement `drain_report` so it repeatedly pops the last job off `queue`, prints `processing job N` for each, and finally prints `queue empty`. (`Vec::pop` returns `Option`.)

```rust playground
fn drain_report(mut queue: Vec<i32>) {
    // TODO: use `while let` and `queue.pop()`
}

fn main() {
    drain_report(vec![1, 2, 3]);
}
```

<details>
<summary>Solution</summary>

```rust playground
fn drain_report(mut queue: Vec<i32>) {
    while let Some(job) = queue.pop() {
        println!("processing job {job}");
    }
    println!("queue empty");
}

fn main() {
    drain_report(vec![1, 2, 3]);
    // processing job 3
    // processing job 2
    // processing job 1
    // queue empty
}
```

> The `mut` on the parameter lets the function mutate its own copy of the
> `Vec`. For more on this, see [Variables and Mutability](/02-basics/00-variables/).

</details>

### Exercise 3: Parse an auth header with `let ... else`

**Difficulty:** Hard

**Objective:** Use `let ... else` for two early-return guard clauses, keeping the happy path un-indented.

**Instructions:** Implement `extract_user_id` to return `Ok(u64)` when `header` looks like `"Bearer 42"`. If the `"Bearer "` prefix is missing, return `Err("missing Bearer prefix")`. If the remainder is not a valid `u64`, return `Err(format!("invalid id: {token}"))`. Use `str::strip_prefix` (returns `Option`) and `str::parse::<u64>()` (returns `Result`). Do **not** nest `if let`s.

```rust
fn extract_user_id(header: &str) -> Result<u64, String> {
    // TODO: two `let ... else` guards, then `Ok(id)`
    /* ??? */
}

fn main() {
    println!("{:?}", extract_user_id("Bearer 42"));
    println!("{:?}", extract_user_id("Basic 42"));
    println!("{:?}", extract_user_id("Bearer notanum"));
}
```

<details>
<summary>Solution</summary>

```rust playground
fn extract_user_id(header: &str) -> Result<u64, String> {
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Err("missing Bearer prefix".to_string());
    };
    let Ok(id) = token.parse::<u64>() else {
        return Err(format!("invalid id: {token}"));
    };
    Ok(id)
}

fn main() {
    println!("{:?}", extract_user_id("Bearer 42"));      // Ok(42)
    println!("{:?}", extract_user_id("Basic 42"));       // Err("missing Bearer prefix")
    println!("{:?}", extract_user_id("Bearer notanum")); // Err("invalid id: notanum")
}
```

Each guard binds its success value (`token`, then `id`) for the rest of the
function, so the final `Ok(id)` reads as the clean, un-nested happy path. This
is the same shape as the [`?` operator](/03-functions/), but with
custom error messages on each failure.

</details>
