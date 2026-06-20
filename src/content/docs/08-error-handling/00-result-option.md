---
title: "Result and Option: Replacing try/catch and null"
description: "Rust has no exceptions or null: a fallible function returns Result<T, E> and a maybe-empty one returns Option<T>, so the compiler makes you handle both."
---

Rust has no exceptions and no `null`. Instead, a function that can fail returns a **`Result<T, E>`** value, and a function that might have nothing to return uses **`Option<T>`**. Both are ordinary enums, so the possibility of failure or absence is written into the type and the compiler forces you to deal with it.

---

## Quick Overview

In TypeScript/JavaScript, failure travels through a side channel: a function `throw`s and some `catch` far away (or none at all) handles it, and "missing" is represented by `null` or `undefined`. Rust folds both into the return value:

- **`Result<T, E>`**: the operation either succeeded with a `T` (`Ok(T)`) or failed with an error `E` (`Err(E)`). This is the replacement for `throw`/`try`/`catch`.
- **`Option<T>`**: there is either a value `Some(T)` or nothing `None`. This is the replacement for `null`/`undefined`.

Because these are return values, not control-flow jumps, **the type signature tells you a function can fail**, and the compiler will not let you silently ignore it.

---

## TypeScript/JavaScript Example

```typescript
// Failure is signaled by throwing; "not found" is signaled by undefined/null.
interface User {
  id: number;
  name: string;
}

function parsePort(input: string): number {
  const port = Number(input);
  if (!Number.isInteger(port) || port < 0 || port > 65535) {
    throw new Error(`'${input}' is not a valid port number`);
  }
  return port;
}

function findUser(users: User[], id: number): User | undefined {
  return users.find((u) => u.id === id);
}

// The caller must REMEMBER to wrap calls in try/catch...
try {
  const port = parsePort("8080");
  console.log(`Listening on port ${port}`);
} catch (err) {
  // `err` is `unknown` in modern TypeScript — you must narrow it yourself.
  console.error("Configuration error:", (err as Error).message);
}

// ...and must REMEMBER that findUser can return undefined.
const user = findUser([{ id: 1, name: "Ada" }], 99);
console.log(user.name); // TypeError at runtime: Cannot read properties of undefined
```

**The problem:** nothing forces the caller to handle the `throw`, and nothing forces a check for `undefined`. Both mistakes compile and ship; they explode at runtime. (In strict TypeScript, `user.name` above is flagged because the return type is `User | undefined`, but only if strict null checks are on, and a thrown exception is still invisible in the type.)

---

## Rust Equivalent

```rust playground
fn parse_port(input: &str) -> Result<u16, String> {
    match input.parse::<u16>() {
        Ok(port) => Ok(port),
        Err(_) => Err(format!("'{input}' is not a valid port number")),
    }
}

#[derive(Debug, Clone)]
struct User {
    id: u32,
    name: String,
}

fn find_user(users: &[User], id: u32) -> Option<&User> {
    for user in users {
        if user.id == id {
            return Some(user);
        }
    }
    None
}

fn main() {
    // The compiler will NOT let you use the inner value without addressing failure.
    match parse_port("8080") {
        Ok(port) => println!("Listening on port {port}"),
        Err(message) => println!("Configuration error: {message}"),
    }

    let users = vec![
        User { id: 1, name: "Ada".to_string() },
        User { id: 2, name: "Linus".to_string() },
    ];

    // `find_user` returns Option<&User>; you cannot reach `.name` without handling None.
    match find_user(&users, 99) {
        Some(user) => println!("Found user: {}", user.name),
        None => println!("No user with that id"),
    }
}
```

Running this prints:

```text
Listening on port 8080
No user with that id
```

> **Note:** The fallible signature is right there in `-> Result<u16, String>` and `-> Option<&User>`. A TypeScript signature like `(input: string) => number` hides the fact that the function can throw; the Rust signature cannot hide it.

> **Tip:** The explicit `for` loop in `find_user` is written out so the `Some`/`None` flow is obvious. The idiomatic one-liner is `users.iter().find(|u| u.id == id)`, which returns the same `Option<&User>`; Clippy will in fact nudge you toward it. We use that combinator form in the helpers later in this file.

---

## Detailed Explanation

### `Option` and `Result` are just enums

Neither type is built into the language as magic syntax. They are defined in the standard library, roughly like this:

```rust
// Already provided by std — shown here only to demystify them.
enum Option<T> {
    Some(T),
    None,
}

enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

`T` and `E` are generic type parameters (covered in [Section 09: Generics and Traits](/09-generics-traits/)). The compiler monomorphizes them for each concrete type you use, so `Option<i32>` and `Result<u16, String>` are distinct, fully-checked types, unlike TypeScript generics, which are erased at runtime.

Because they are enums, you construct and inspect them with the same tools as any other enum (see [Section 06: Enums](/06-data-structures/)):

```rust playground
fn main() {
    let some_number: Option<i32> = Some(5);
    let no_number: Option<i32> = None;
    println!("{some_number:?} {no_number:?}"); // Some(5) None

    let ok_value: Result<i32, String> = Ok(200);
    let err_value: Result<i32, String> = Err("boom".to_string());
    println!("{ok_value:?} {err_value:?}"); // Ok(200) Err("boom")
}
```

Real output:

```text
Some(5) None
Ok(200) Err("boom")
```

### Why there is no `null`

In JavaScript, *any* reference might secretly be `null` or `undefined`, which is why "Cannot read properties of undefined" is the most common runtime error in the ecosystem. Rust has no `null`. If a value might be absent, its type is `Option<T>`, and `T` and `Option<T>` are different types. You literally cannot pass a "maybe-missing" value where a "definitely-present" one is required without first unwrapping it. Indexing past the end of a slice does not return `undefined`; `Vec::get` returns `None`:

```rust playground
fn main() {
    let names = vec!["Ada", "Linus"];
    let third: Option<&&str> = names.get(2); // out of bounds -> None, never a panic
    println!("{third:?}"); // None
}
```

```text
None
```

### Matching to extract the value

The fundamental way to get at the inner value is `match`, which is exhaustive: you must cover every variant or the program does not compile. The two arms of a `Result` are `Ok` and `Err`; the two arms of an `Option` are `Some` and `None`:

```rust playground
fn parse_port(input: &str) -> Result<u16, String> {
    input.parse::<u16>().map_err(|_| format!("'{input}' is not a valid port number"))
}

fn report_port(input: &str) {
    match parse_port(input) {
        Ok(port) => println!("Listening on port {port}"),
        Err(message) => println!("Configuration error: {message}"),
    }
}

fn main() {
    report_port("8080");
    report_port("oops");
}
```

`match` is where the safety comes from: forgetting the `Err` or `None` case is a compile error, not a latent bug. This is the headline difference from `try`/`catch`, where forgetting to catch is perfectly legal.

### Lighter-weight matching: `if let` and `let ... else`

When you only care about one variant, `match` is verbose. `if let` matches a single pattern:

```rust playground
#[derive(Clone)]
struct User {
    id: u32,
    name: String,
}

fn find_user(users: &[User], id: u32) -> Option<&User> {
    users.iter().find(|u| u.id == id)
}

fn main() {
    let users = vec![User { id: 1, name: "Ada".to_string() }];

    if let Some(user) = find_user(&users, 1) {
        println!("if let found: {}", user.name);
    }
}
```

`let ... else` binds the value when the pattern matches, and runs a diverging block (one that `return`s, `break`s, or panics) when it does not. Perfect for "extract or bail":

```rust playground
#[derive(Clone)]
struct User {
    id: u32,
    name: String,
}

fn find_user(users: &[User], id: u32) -> Option<&User> {
    users.iter().find(|u| u.id == id)
}

fn main() {
    let users = vec![User { id: 1, name: "Ada".to_string() }];

    let Some(found) = find_user(&users, 1) else {
        println!("could not find user");
        return;
    };
    // `found` is a plain &User from here on — no more Option in the way.
    println!("let-else found: {}", found.name);
}
```

### Combinators: handling without `match`

For everyday transformations you rarely write `match`. Both types carry a rich set of methods. The most useful ones:

```rust playground
fn parse_port(input: &str) -> Result<u16, String> {
    input.parse::<u16>().map_err(|_| format!("'{input}' is not a valid port number"))
}

fn first_admin(names: &[&str]) -> Option<usize> {
    names.iter().position(|&n| n == "admin")
}

fn main() {
    let names = ["guest", "admin", "root"];

    // map: transform the inner value, leaving None/Err untouched.
    let label = first_admin(&names)
        .map(|index| format!("admin at position {index}"))
        .unwrap_or_else(|| "no admin found".to_string());
    println!("{label}"); // admin at position 1

    // map on Result transforms the Ok value.
    let doubled: Result<u16, String> = parse_port("4000").map(|p| p * 2);
    println!("{doubled:?}"); // Ok(8000)

    // unwrap_or: supply a fallback value for the failure case.
    let port = parse_port("oops").unwrap_or(3000);
    println!("port fallback = {port}"); // 3000
}
```

These are conceptually close to TypeScript's `?.` (optional chaining) and `??` (nullish coalescing): `opt.map(f)` resembles `obj?.f()`, and `opt.unwrap_or(d)` resembles `value ?? d`. But the analogy is not exact: `?.`/`??` are language operators that short-circuit on `null`/`undefined`, whereas `map`/`unwrap_or` are ordinary methods on a real enum, and they also work on the `Err` side of a `Result`, which has no JavaScript counterpart.

### Converting between `Result` and `Option`

The two types interconvert when you want to discard or supply error context:

```rust playground
#[derive(Debug, Clone)]
struct User {
    id: u32,
    name: String,
}

fn find_user(users: &[User], id: u32) -> Option<&User> {
    users.iter().find(|u| u.id == id)
}

fn parse_port(input: &str) -> Result<u16, String> {
    input.parse::<u16>().map_err(|_| format!("'{input}' is not a valid port number"))
}

fn main() {
    let users = vec![User { id: 1, name: "Ada".to_string() }];

    // .ok() throws away the error and yields an Option.
    let maybe: Option<u16> = parse_port("22").ok();
    println!("{maybe:?}"); // Some(22)

    // .ok_or_else() attaches an error and yields a Result.
    let as_result: Result<&User, String> =
        find_user(&users, 1).ok_or_else(|| "user not found".to_string());
    println!("{:?}", as_result.map(|u| u.name.clone())); // Ok("Ada")
}
```

> **Tip:** Use `ok_or_else` (which takes a closure) rather than `ok_or` when building the error is non-trivial, so the error value is only constructed on the failure path. The same logic applies to `unwrap_or_else` vs `unwrap_or`.

---

## Key Differences

| Concept                 | TypeScript/JavaScript                          | Rust                                                   |
| ----------------------- | ---------------------------------------------- | ------------------------------------------------------ |
| Recoverable failure     | `throw` + `try`/`catch`                        | return `Result<T, E>`                                  |
| Absent value            | `null` / `undefined`                           | `Option<T>` (`Some`/`None`)                            |
| Is failure in the type? | No: `throw` is invisible in the signature      | Yes: `Result`/`Option` are in the return type          |
| Can you ignore it?      | Yes (silently; explodes at runtime)            | No: compiler forces handling; ignoring `Result` warns  |
| Error value type        | Anything (you can `throw 42` or a string)      | A concrete `E` chosen by the function                  |
| "Catch all" handler     | `catch (err: unknown)`                         | match the `Err`/`None` arm; no catch-all needed        |
| Cost                    | Stack unwinding when thrown                     | Just a returned enum value; no unwinding               |

### `Result` vs `Option`: which one?

This is the single most common question. The rule of thumb:

- Use **`Option<T>`** when *absence is normal and carries no explanation*. "This key isn't in the map." "The list is empty." "Index 5 of a 3-element slice." There is nothing to report; the value simply isn't there. `None` says all there is to say.
- Use **`Result<T, E>`** when *failure has a reason worth communicating*. "The port string wasn't a number." "The file couldn't be opened because permission was denied." The `E` carries the *why*, which a caller may log, surface to a user, or branch on.

A quick test: if you find yourself wanting to attach a message or an error code, you want `Result`. If the only sensible response to absence is "try something else" or "skip it," `Option` is enough.

> **Note:** `parse_port` returns `Result` because "not a valid port" is a reason worth reporting. `find_user` returns `Option` because "no user with that id" is just absence, though in a real API you might upgrade it to `Result` if the caller needs to distinguish "not found" from other failures.

### Failure is a value, not a jump

A thrown JavaScript exception unwinds the stack until something catches it, skipping every line in between. A Rust `Result` is an ordinary value returned normally; control flow is completely linear and visible. This is why Rust error handling has essentially zero hidden cost and why you can store, collect, and transform errors like any other data. (Rust *does* have `panic!` for truly unrecoverable situations, and it can unwind — but that is a separate mechanism covered in [Panics](/08-error-handling/02-panic/), not the everyday error path.)

---

## Common Pitfalls

### Pitfall 1: Trying to use an `Option`/`Result` as if it were the inner value

A TypeScript developer expects `findUser(...)` to give back a `User`. In Rust it gives back an `Option<&User>`, and you cannot use it directly:

```rust
fn find(v: &[i32], target: i32) -> Option<usize> {
    v.iter().position(|&x| x == target)
}
fn main() {
    let v = vec![10, 20, 30];
    let idx = find(&v, 20); // idx is Option<usize>, not usize
    let element = v[idx];   // does not compile (error[E0277])
    println!("{element}");
}
```

Real compiler error:

```text
error[E0277]: the type `[i32]` cannot be indexed by `Option<usize>`
 --> src/bin/pitfall_a.rs:7:21
  |
7 |     let element = v[idx];   // does not compile (error[E0277])
  |                     ^^^ slice indices are of type `usize` or ranges of `usize`
  |
  = help: the trait `SliceIndex<[i32]>` is not implemented for `Option<usize>`
  = note: required for `Vec<i32>` to implement `Index<Option<usize>>`
```

**Fix:** match, use `if let`, or use a combinator to get the `usize` out first. The compiler is reminding you that the index might not exist.

### Pitfall 2: Comparing an `Option` to a bare value

Coming from JavaScript, you might write `if (port === 80)`. In Rust, `port` is an `Option<u16>`, not a `u16`:

```rust
fn find_port(name: &str) -> Option<u16> {
    if name == "http" { Some(80) } else { None }
}
fn main() {
    let port = find_port("http");
    if port == 80 { // does not compile (error[E0308])
        println!("standard http");
    }
}
```

Real compiler error (note the helpful suggestion):

```text
error[E0308]: mismatched types
 --> src/bin/pitfall_c.rs:6:16
  |
6 |     if port == 80 { // does not compile (error[E0308])
  |        ----    ^^ expected `Option<u16>`, found integer
  |        |
  |        expected because this is `Option<u16>`
  |
  = note: expected enum `Option<u16>`
             found type `{integer}`
help: try wrapping the expression in `Some`
  |
6 |     if port == Some(80) { // does not compile (error[E0308])
  |                +++++  +
```

**Fix:** compare against `Some(80)`, or pattern-match with `if let Some(80) = port`.

### Pitfall 3: Ignoring a `Result`

In JavaScript you can call a throwing function and never wrap it in `try`/`catch`. In Rust, `Result` is marked `#[must_use]`, so discarding one triggers a warning:

```rust playground
fn parse_port(input: &str) -> Result<u16, String> {
    input.parse::<u16>().map_err(|_| "bad port".to_string())
}
fn main() {
    parse_port("8080"); // warning: unused `Result` that must be used
    println!("done");
}
```

Real compiler warning:

```text
warning: unused `Result` that must be used
 --> src/bin/pitfall_b.rs:5:5
  |
5 |     parse_port("8080"); // warning: unused `Result` that must be used
  |     ^^^^^^^^^^^^^^^^^^
  |
  = note: this `Result` may be an `Err` variant, which should be handled
  = note: `#[warn(unused_must_use)]` on by default
help: use `let _ = ...` to ignore the resulting value
```

**Fix:** handle the `Result`, propagate it with `?` (see [The `?` Operator](/08-error-handling/01-question-mark/)), or — if you genuinely mean to discard it — write `let _ = parse_port("8080");` to say so explicitly.

### Pitfall 4: A non-exhaustive `match`

`match` must cover every variant. Omitting the `Err` (or `None`) arm is a compile error, which is exactly the protection that `try`/`catch` lacks:

```rust
fn parse_port(input: &str) -> Result<u16, String> {
    input.parse::<u16>().map_err(|_| "bad port".to_string())
}
fn main() {
    let result = parse_port("8080");
    match result { // does not compile (error[E0004])
        Ok(port) => println!("port {port}"),
        // forgot the Err arm
    }
}
```

Real compiler error (trimmed):

```text
error[E0004]: non-exhaustive patterns: `Err(_)` not covered
 --> src/bin/pitfall_d.rs:6:11
  |
6 |     match result {
  |           ^^^^^^ pattern `Err(_)` not covered
...
help: ensure that all possible cases are being handled by adding a match arm
      with a wildcard pattern or an explicit pattern as shown
```

**Fix:** add the missing arm. Reach for a catch-all `_ => ...` only when you truly mean "everything else."

---

## Best Practices

### Prefer combinators and `?` over `match` for plumbing

A full `match` is the right tool when each variant needs genuinely different logic. For the common cases — transform the success value, supply a default, propagate the error upward — `map`, `unwrap_or_else`, `and_then`, and the `?` operator are shorter and clearer. Save `match` for branching decisions.

### Reserve `unwrap`/`expect` for provable invariants

`unwrap()` and `expect()` extract the inner value but **panic** on `None`/`Err`. They are fine in tests, quick prototypes, and cases where the failure is genuinely impossible, but in production logic they reintroduce exactly the kind of runtime crash that `Result` was meant to prevent. The dedicated topic [unwrap and expect](/08-error-handling/03-unwrap-expect/) covers when they are acceptable.

### Don't `match` only to re-wrap — use `.map_err`

Beginners often write a `match` that returns `Ok(x)` in the `Ok` arm and a transformed error in the `Err` arm. That entire `match` is just `.map` (to change the `Ok` value) or `.map_err` (to change the `Err` value):

```rust playground
fn raw() -> Result<u16, std::num::ParseIntError> { "x".parse() }

// Verbose:
fn verbose() -> Result<u16, String> {
    match raw() {
        Ok(v) => Ok(v),
        Err(_) => Err("bad number".to_string()),
    }
}

// Idiomatic:
fn idiomatic() -> Result<u16, String> {
    raw().map_err(|_| "bad number".to_string())
}

fn main() {
    println!("{:?}", verbose());
    println!("{:?}", idiomatic());
}
```

### Choose meaningful error types

`Result<T, String>` is fine for examples and small programs, but real libraries define a dedicated error type so callers can match on specific cases. That is the subject of [Custom Errors](/08-error-handling/04-custom-errors/) and the [`thiserror`/`anyhow`](/08-error-handling/06-anyhow-thiserror/) topic. The key idea for *this* file: the moment you write `Result`, you have already made the failure visible and handleable. Refining `E` is the next step.

### Let the type encode optionality once

If a struct field is genuinely optional, make it `Option<T>` in the struct and stop checking for absence everywhere else. This is the Rust counterpart to a `field?: T` in TypeScript, but enforced — you can never accidentally read it as if it were always present.

---

## Real-World Example

A small configuration loader that parses `key = value` lines. It uses `Option` for "this line might not be a valid pair" and `Result` for "this whole config might be invalid, and here's why." Notice how `Option`'s `split_once` + `?`, `Result`'s `map_err` + `?`, and `unwrap_or` for defaults all combine.

```rust playground
#[derive(Debug)]
struct ServerConfig {
    host: String,
    port: u16,
    max_connections: u32,
}

/// Parse "key=value" into a (key, value) pair, or None if malformed.
fn parse_pair(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?; // `?` on Option: bail with None
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key, value))
}

/// Build a ServerConfig from raw lines. Returns the first error encountered.
fn load_config(lines: &[&str]) -> Result<ServerConfig, String> {
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut max_connections: Option<u32> = None;

    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue; // skip blanks and comments
        }

        // `let ... else`: extract the pair or report a malformed line.
        let Some((key, value)) = parse_pair(line) else {
            return Err(format!("malformed line: {line:?}"));
        };

        match key {
            "host" => host = Some(value.to_string()),
            "port" => {
                let parsed = value
                    .parse::<u16>()
                    .map_err(|_| format!("port must be 0-65535, got {value:?}"))?;
                port = Some(parsed);
            }
            "max_connections" => {
                let parsed = value
                    .parse::<u32>()
                    .map_err(|_| format!("max_connections must be a number, got {value:?}"))?;
                max_connections = Some(parsed);
            }
            other => return Err(format!("unknown key: {other:?}")),
        }
    }

    Ok(ServerConfig {
        // host is required: turn None into an Err with `ok_or_else` + `?`.
        host: host.ok_or_else(|| "missing required key: host".to_string())?,
        // port and max_connections are optional: fall back to defaults.
        port: port.unwrap_or(8080),
        max_connections: max_connections.unwrap_or(256),
    })
}

fn main() {
    let good = [
        "# production config",
        "host = api.example.com",
        "port = 443",
        "",
        "max_connections = 1000",
    ];
    println!("{:?}", load_config(&good));

    let missing_host = ["port = 443"];
    println!("{:?}", load_config(&missing_host));

    let bad_port = ["host = localhost", "port = wat"];
    println!("{:?}", load_config(&bad_port));

    let malformed = ["host = localhost", "garbage"];
    println!("{:?}", load_config(&malformed));

    // Defaults apply when optional keys are absent.
    let minimal = ["host = localhost"];
    println!("{:?}", load_config(&minimal));
}
```

Real output:

```text
Ok(ServerConfig { host: "api.example.com", port: 443, max_connections: 1000 })
Err("missing required key: host")
Err("port must be 0-65535, got \"wat\"")
Err("malformed line: \"garbage\"")
Ok(ServerConfig { host: "localhost", port: 8080, max_connections: 256 })
```

Every failure mode — missing required key, unparseable number, malformed line — is a returned `Err` with a message, never a thrown exception, and the happy path supplies defaults through `Option`. The `?` operator doing the propagation here is the subject of the [next topic](/08-error-handling/01-question-mark/).

---

## Further Reading

### Official Documentation

- [The Rust Book — Error Handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [`std::result::Result`](https://doc.rust-lang.org/std/result/enum.Result.html) (full list of combinators)
- [`std::option::Option`](https://doc.rust-lang.org/std/option/enum.Option.html)
- [Rust by Example — `Option`](https://doc.rust-lang.org/rust-by-example/std/option.html) and [`Result`](https://doc.rust-lang.org/rust-by-example/error/result.html)

### Related Sections in This Guide

- [The `?` Operator](/08-error-handling/01-question-mark/): propagating `Result`/`Option` upward without `match`
- [Panics](/08-error-handling/02-panic/) — `panic!`, the closest thing to an unhandled exception, and when it is appropriate
- [unwrap and expect](/08-error-handling/03-unwrap-expect/): extracting values when failure is impossible (and when it isn't)
- [Custom Errors](/08-error-handling/04-custom-errors/) and [`anyhow` / `thiserror`](/08-error-handling/06-anyhow-thiserror/) — designing real error types for the `E` in `Result<T, E>`
- [Section 06: Data Structures](/06-data-structures/): enums and pattern matching, the machinery behind `Option`/`Result`
- [Section 09: Generics and Traits](/09-generics-traits/) — the `<T>` and `<T, E>` that make these types reusable
- [Section 02: Basics](/02-basics/): types and `match` fundamentals

---

## Exercises

### Exercise 1: From `null` to `Option`

**Difficulty:** Easy

**Objective:** Replace a JavaScript-style "return `null` on bad input" function with an `Option`-returning Rust function.

**Instructions:** Implement `safe_divide(a: f64, b: f64) -> Option<f64>` that returns `None` when `b` is `0.0` and `Some(a / b)` otherwise. Print the results of dividing `10.0 / 2.0` and `1.0 / 0.0`.

```rust
fn safe_divide(a: f64, b: f64) -> Option<f64> {
    // TODO: return None when b is 0.0, otherwise Some(a / b)
    /* ??? */
}

fn main() {
    println!("{:?}", safe_divide(10.0, 2.0)); // expected Some(5.0)
    println!("{:?}", safe_divide(1.0, 0.0));  // expected None
}
```

<details>
<summary>Solution</summary>

```rust playground
fn safe_divide(a: f64, b: f64) -> Option<f64> {
    if b == 0.0 {
        None
    } else {
        Some(a / b)
    }
}

fn main() {
    println!("{:?}", safe_divide(10.0, 2.0)); // Some(5.0)
    println!("{:?}", safe_divide(1.0, 0.0));  // None
}
```

Output:

```text
Some(5.0)
None
```

</details>

### Exercise 2: Turning `throw` into `Result` with a real error type

**Difficulty:** Medium

**Objective:** Replace a throwing checkout function with one that returns `Result<T, E>` where `E` is a descriptive enum.

**Instructions:** Define an enum `CheckoutError` with variants `EmptyCart` and `InsufficientStock { item, requested, available }`. Implement `checkout(cart: &[CartItem], stock: u32) -> Result<u32, CheckoutError>` that returns `Err(EmptyCart)` for an empty cart, `Err(InsufficientStock { .. })` if any item's quantity exceeds `stock`, and otherwise `Ok` of the total quantity. Print results for a valid cart, an empty cart, and an over-stock cart.

```rust
#[derive(Debug, PartialEq)]
enum CheckoutError {
    EmptyCart,
    InsufficientStock { item: String, requested: u32, available: u32 },
}

struct CartItem {
    name: String,
    quantity: u32,
}

fn checkout(cart: &[CartItem], stock: u32) -> Result<u32, CheckoutError> {
    // TODO
    /* ??? */
}

fn main() {
    let cart = vec![CartItem { name: "widget".to_string(), quantity: 3 }];
    println!("{:?}", checkout(&cart, 10));
    println!("{:?}", checkout(&[], 10));
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug, PartialEq)]
enum CheckoutError {
    EmptyCart,
    InsufficientStock { item: String, requested: u32, available: u32 },
}

struct CartItem {
    name: String,
    quantity: u32,
}

fn checkout(cart: &[CartItem], stock: u32) -> Result<u32, CheckoutError> {
    if cart.is_empty() {
        return Err(CheckoutError::EmptyCart);
    }
    let mut total = 0;
    for item in cart {
        if item.quantity > stock {
            return Err(CheckoutError::InsufficientStock {
                item: item.name.clone(),
                requested: item.quantity,
                available: stock,
            });
        }
        total += item.quantity;
    }
    Ok(total)
}

fn main() {
    let cart = vec![CartItem { name: "widget".to_string(), quantity: 3 }];
    println!("{:?}", checkout(&cart, 10)); // Ok(3)
    println!("{:?}", checkout(&[], 10));   // Err(EmptyCart)

    let big = vec![CartItem { name: "widget".to_string(), quantity: 99 }];
    println!("{:?}", checkout(&big, 10));  // Err(InsufficientStock { .. })
}
```

Output:

```text
Ok(3)
Err(EmptyCart)
Err(InsufficientStock { item: "widget", requested: 99, available: 10 })
```

> Defining `E` as an enum (rather than a `String`) lets callers `match` on `CheckoutError::EmptyCart` vs `InsufficientStock`. This is the bridge to [Custom Errors](/08-error-handling/04-custom-errors/).

</details>

### Exercise 3: Chaining `Option` and `Result`

**Difficulty:** Medium-Hard

**Objective:** Build a lookup-then-parse pipeline using `ok_or_else`, `map_err`, and the `?` operator together.

**Instructions:** Given `env_lookup(key) -> Option<&'static str>`, implement `config_u16(key: &str) -> Result<u16, String>` that: returns `Err("{key} is not set")` if the key is absent; otherwise parses the value as `u16`, returning `Err("{key} is not a valid number")` on a parse failure; otherwise returns `Ok(value)`. Do it without an explicit `match` — use `ok_or_else`, `?`, and `map_err`.

```rust
fn env_lookup(key: &str) -> Option<&'static str> {
    match key {
        "PORT" => Some("8080"),
        "RETRIES" => Some("notanumber"),
        _ => None,
    }
}

fn config_u16(key: &str) -> Result<u16, String> {
    // TODO: use ok_or_else, ?, and map_err
    /* ??? */
}

fn main() {
    println!("{:?}", config_u16("PORT"));
    println!("{:?}", config_u16("RETRIES"));
    println!("{:?}", config_u16("MISSING"));
}
```

<details>
<summary>Solution</summary>

```rust playground
fn env_lookup(key: &str) -> Option<&'static str> {
    match key {
        "PORT" => Some("8080"),
        "RETRIES" => Some("notanumber"),
        _ => None,
    }
}

fn config_u16(key: &str) -> Result<u16, String> {
    env_lookup(key)
        .ok_or_else(|| format!("{key} is not set"))? // Option -> Result, then unwrap or return Err
        .parse::<u16>()
        .map_err(|_| format!("{key} is not a valid number"))
}

fn main() {
    println!("{:?}", config_u16("PORT"));    // Ok(8080)
    println!("{:?}", config_u16("RETRIES")); // Err("RETRIES is not a valid number")
    println!("{:?}", config_u16("MISSING")); // Err("MISSING is not set")
}
```

Output:

```text
Ok(8080)
Err("RETRIES is not a valid number")
Err("MISSING is not set")
```

> The `?` here turns the `Result<&str, String>` produced by `ok_or_else` into a `&str` (or returns early with the `Err`). Read more in [The `?` Operator](/08-error-handling/01-question-mark/).

</details>
