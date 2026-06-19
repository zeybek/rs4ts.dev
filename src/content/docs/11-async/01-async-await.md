---
title: "Rust async/await vs JavaScript"
description: "Rust's async/await mirrors JavaScript's, but .await is postfix, an async fn returns a lazy Future that runs nothing until awaited, and ?"
---

Rust borrowed the `async`/`await` keywords from the same family of languages that gave them to JavaScript, so the surface syntax will feel familiar. The semantics underneath are different in one decisive way: an `async fn` in Rust does not start running when you call it. It hands you a **lazy value** (a `Future`) that does nothing until you `.await` it.

---

## Quick Overview

In Rust you mark a function `async`, and inside it you write `some_future.await` (a postfix keyword, not a `await some_future` prefix). An `async fn` is not a normal function that returns its result. It is sugar for a function that returns an anonymous type implementing the `Future` trait. The `?` operator works inside `async fn` exactly as it does in synchronous code, propagating errors out of the future. This page is about the **syntax and mechanics** of `async`/`await`; the deeper "why are futures lazy and what is a runtime" story lives in [Promises vs Futures](/11-async/00-promises-vs-futures/) and [Tokio Intro](/11-async/02-tokio-intro/).

> **Note:** Every runnable Rust snippet on this page was compiled and executed with `rustc`/`cargo` 1.96.0 (current stable; 2024 edition). The async examples use the `tokio` runtime (`tokio = { version = "1.52", features = ["full"] }`) because Rust ships **no built-in executor** (see [Tokio Setup](/11-async/03-tokio-setup/)).

---

## TypeScript/JavaScript Example

In TypeScript, `async`/`await` is a thin layer over Promises. An `async function` always returns a `Promise<T>`, and `await` suspends the surrounding async function until that Promise settles. Critically, the work **starts the moment you call the function**, even if you never `await` the returned Promise.

```typescript
// Each async function returns a Promise<T>.
async function fetchUser(id: number): Promise<string> {
  // Simulate a network round-trip.
  await new Promise((resolve) => setTimeout(resolve, 50));
  return `user-${id}`;
}

async function greet(id: number): Promise<string> {
  // `await` unwraps the resolved value of the Promise.
  const name = await fetchUser(id);
  return `Hello, ${name}!`;
}

// Error propagation: a thrown error rejects the Promise; `await` re-throws it.
async function fetchScore(id: number): Promise<number> {
  const body = await fetchUser(id); // "user-3"
  const score = Number.parseInt(body.replace("user-", ""), 10);
  if (Number.isNaN(score)) {
    throw new Error("bad score");
  }
  return score * 2;
}

const msg = await greet(7);
console.log(msg); // Hello, user-7!

// EAGER: calling fetchUser starts the timer immediately, even unawaited.
fetchUser(99); // the setTimeout is already ticking
```

Two things to fix firmly in mind before the Rust version:

- `await` is a **prefix** operator in JavaScript: `await expr`.
- Calling `fetchUser(99)` **eagerly** kicks off the work; the returned Promise is "hot" whether or not you await it.

---

## Rust Equivalent

The same shape in Rust. Note the **postfix** `.await`, the `async fn`, and that the work only happens at the `.await` point.

```rust
use std::time::Duration;
use tokio::time::sleep;

// An `async fn` returns an `impl Future`; calling it does NOTHING yet.
async fn fetch_user(id: u32) -> String {
    // Simulate a network round-trip without an external server.
    sleep(Duration::from_millis(50)).await;
    format!("user-{id}")
}

async fn greet(id: u32) -> String {
    // `.await` drives the future to completion and unwraps its output.
    let name = fetch_user(id).await;
    format!("Hello, {name}!")
}

#[tokio::main]
async fn main() {
    let msg = greet(7).await;
    println!("{msg}");
}
```

Real output:

```text
Hello, user-7!
```

The `#[tokio::main]` attribute sets up the runtime that actually polls these futures (covered in [Tokio Setup](/11-async/03-tokio-setup/)). Without a runtime, none of this code would run: there is nothing in the language itself that drives a future forward.

---

## Detailed Explanation

### `await` is postfix, and that is on purpose

In JavaScript you write `await foo()`. In Rust you write `foo().await`. The postfix form chains cleanly with the `?` operator and method calls:

```rust
// Rust: reads left-to-right, like a pipeline.
// let body = fetch().await?.text().await?;

// The JavaScript prefix form forces awkward parentheses for the same chain:
// const body = await (await fetch()).text();
```

> **Tip:** Read `x.await` as "wait for `x`, then give me its value." It is a real keyword, not a field access — `x.await` is special syntax, even though it looks like reading a field named `await`.

### `async fn` returns `impl Future`, not the value

This is the heart of the matter. When you write:

```rust
async fn double_async(n: u64) -> u64 {
    n * 2
}
```

the compiler rewrites it to roughly:

```rust
use std::future::Future;

fn double_desugared(n: u64) -> impl Future<Output = u64> {
    // An `async` block is itself a value of an anonymous Future type.
    async move { n * 2 }
}
```

Both are equivalent and both compile. Awaiting either gives `42`:

```rust
use std::future::Future;

async fn double_async(n: u64) -> u64 {
    n * 2
}

fn double_desugared(n: u64) -> impl Future<Output = u64> {
    async move { n * 2 }
}

#[tokio::main]
async fn main() {
    println!("{}", double_async(21).await);
    println!("{}", double_desugared(21).await);

    // An async block can be stored in a variable and awaited later.
    let lazy = async { 10 + 5 };
    println!("{}", lazy.await);
}
```

Real output:

```text
42
42
15
```

The declared return type of an `async fn` is its **`Output`**, not its actual return type. `async fn double_async(n: u64) -> u64` really returns `impl Future<Output = u64>`. The `-> u64` describes what you get *after* awaiting. (`async` blocks and returning futures are explored further in [Async Functions](/11-async/04-async-functions/).)

> **Note:** Because the returned type is an anonymous, compiler-generated state machine, you write `impl Future<Output = T>` rather than naming it. There is no `Future<u64>` you can spell directly the way you write `Promise<number>` in TypeScript.

### Futures are lazy — calling does not run

In JavaScript, `fetchUser(99)` starts immediately. In Rust, building a future runs **none** of its body. The body only advances when a runtime polls it, which happens because you `.await` it (or hand it to `tokio::spawn`). If you call an `async fn` and ignore the result, the compiler warns you that nothing happened:

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn fetch_user(id: u32) -> String {
    sleep(Duration::from_millis(10)).await;
    format!("user-{id}")
}

#[tokio::main]
async fn main() {
    // Forgetting `.await`: the future is created but never driven.
    let _fut = fetch_user(1);
    fetch_user(2);
    println!("done");
}
```

Real compiler warning:

```text
warning: unused implementer of `Future` that must be used
  --> src/main.rs:13:5
   |
13 |     fetch_user(2);
   |     ^^^^^^^^^^^^^
   |
   = note: futures do nothing unless you `.await` or poll them
   = note: `#[warn(unused_must_use)]` on by default
```

That phrase, "futures do nothing unless you `.await` or poll them," is the single most important sentence on this page. It is the exact opposite of a JavaScript Promise. The full eager-vs-lazy contrast is in [Promises vs Futures](/11-async/00-promises-vs-futures/).

### Sequential `.await` runs one after another

Two `.await`s in a row behave like `await a; await b` in JavaScript: the second future does not begin until the first resolves. The total time is roughly the **sum**:

```rust
use std::time::{Duration, Instant};
use tokio::time::sleep;

async fn step(label: &str, ms: u64) -> &str {
    sleep(Duration::from_millis(ms)).await;
    label
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    // Two sequential `.await`s: the second does not begin until the first
    // resolves. Total time is roughly the SUM (~150ms), like `await a; await b`.
    let a = step("a", 75).await;
    let b = step("b", 75).await;
    println!("got {a} then {b} in ~{}ms", start.elapsed().as_millis());
}
```

Real output (timing varies slightly):

```text
got a then b in ~150ms
```

To run them **concurrently** (the equivalent of `Promise.all([a, b])`, ~75ms total), you reach for `tokio::join!` or `select!`; see [Select & Join](/11-async/07-select-join/). This page deliberately keeps to sequential awaiting; concurrency is its own topic.

### Error handling with `?` inside `async fn`

The `?` operator works inside an `async fn` the same way it works in any function: if the value is `Err`, the future short-circuits and resolves to that `Err`. You typically `.await` first to get a `Result`, then apply `?`:

```rust
use std::num::ParseIntError;
use std::time::Duration;
use tokio::time::sleep;

// Simulates an async I/O fetch that yields a raw string body.
async fn fetch_body(id: u32) -> String {
    sleep(Duration::from_millis(10)).await;
    if id == 0 { "not-a-number".to_string() } else { (id * 100).to_string() }
}

// `?` works inside `async fn` exactly like in a sync fn: it short-circuits
// on `Err` and returns it from the future's output type.
async fn fetch_score(id: u32) -> Result<u32, ParseIntError> {
    let body = fetch_body(id).await; // .await first ...
    let score: u32 = body.parse()?;  // ... then `?` on the Result
    Ok(score * 2)
}

#[tokio::main]
async fn main() {
    match fetch_score(3).await {
        Ok(score) => println!("score = {score}"),
        Err(e) => println!("error: {e}"),
    }
    match fetch_score(0).await {
        Ok(score) => println!("score = {score}"),
        Err(e) => println!("error: {e}"),
    }
}
```

Real output:

```text
score = 600
error: invalid digit found in string
```

The `Output` of `fetch_score` is `Result<u32, ParseIntError>`. The `?` returns early *from the future*, resolving it to the `Err`. The mental model is: an `async fn -> Result<T, E>` is "a future that resolves to a `Result`." Everything you know about `?`, `From`-based error conversion, and `Result` from [The `?` Operator](/08-error-handling/01-question-mark/) carries over unchanged. The only new wrinkle is remembering to `.await` before you `?` on something that is itself a future.

> **Tip:** The order matters: `fetch().await?` means "await the future, then `?` the `Result` it produced." Writing `fetch()?.await` is usually wrong: a bare future is not a `Result`, so there is nothing for `?` to act on.

### `?` in `async fn main`

Just like a synchronous `fn main`, an async `main` can return a `Result` so `?` works at the top level. `#[tokio::main] async fn main() -> Result<(), E>` is valid:

```rust
async fn load() -> Result<u32, std::num::ParseIntError> {
    "42".parse()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let n = load().await?; // `?` is fine: main returns a Result
    println!("loaded {n}");
    Ok(())
}
```

---

## Key Differences

| Aspect | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| `await` position | Prefix: `await expr` | Postfix: `expr.await` |
| What `async fn` returns | `Promise<T>` (a concrete, awaitable object) | `impl Future<Output = T>` (an anonymous lazy type) |
| When work starts | **Eagerly**, on call | **Lazily**, only when polled/`.await`ed |
| Needs a runtime? | No; the JS engine has a built-in event loop | Yes; you choose one (Tokio), none is built in |
| Error propagation | `throw` / `try`/`catch`; `await` re-throws | `Result<T, E>` + `?`; no exceptions |
| Cancellation | Hard; a started Promise generally runs to completion | Dropping a future cancels it (it just stops being polled) |
| Multiple awaits in series | `await a; await b` runs sequentially | `a.await; b.await` runs sequentially (same) |
| Concurrency primitive | `Promise.all` / `Promise.race` | `tokio::join!` / `tokio::select!` (see [Select & Join](/11-async/07-select-join/)) |

The deepest divergence is the runtime + laziness pair. In Node, the event loop is always there and Promises are hot. In Rust, you opt into a runtime and futures are cold until driven. Treat that as the load-bearing difference; most async bugs that trip up TypeScript developers trace back to it.

> **Warning:** Do not describe Rust futures as "eager Promises with different syntax." They are the **opposite** — lazy, and inert without an executor. Internalizing this prevents a whole class of "why didn't my code run?" surprises.

---

## Common Pitfalls

### Pitfall 1: Forgetting `.await` and using the future as its value

A TypeScript habit is to treat the return of an async call as the value. In Rust that is a future, not the value, so you get a type error:

```rust
async fn fetch_user(id: u32) -> String {
    format!("user-{id}")
}

#[tokio::main]
async fn main() {
    // does not compile (error[E0308]): using the future as if it were a String.
    let name: String = fetch_user(1);
    println!("{}", name.len());
}
```

Real compiler error:

```text
error[E0308]: mismatched types
 --> src/main.rs:8:24
  |
8 |     let name: String = fetch_user(1);
  |               ------   ^^^^^^^^^^^^^ expected `String`, found future
  |               |
  |               expected due to this
  |
note: calling an async function returns a future
 --> src/main.rs:8:24
  |
8 |     let name: String = fetch_user(1);
  |                        ^^^^^^^^^^^^^
help: consider `await`ing on the `Future`
  |
8 |     let name: String = fetch_user(1).await;
  |                                     ++++++
```

The fix is exactly what the compiler suggests: add `.await`. (When you ignore the future entirely instead of binding it, you get the `must_use` warning shown earlier rather than a hard error.)

### Pitfall 2: Trying to `.await` outside an async context

`.await` is only legal inside an `async fn` or `async` block. A plain synchronous function cannot await:

```rust
async fn load() -> Result<u32, std::num::ParseIntError> {
    "42".parse()
}

// does not compile (error[E0728]): a non-async fn cannot use `.await`.
fn helper() {
    let _ = load().await;
}

fn main() {
    helper();
}
```

Real compiler error:

```text
error[E0728]: `await` is only allowed inside `async` functions and blocks
 --> src/main.rs:7:20
  |
6 | fn helper() {
  | ----------- this is not `async`
7 |     let _ = load().await;
  |                    ^^^^^ only allowed inside `async` functions and blocks
```

Fixes: make `helper` itself `async`, or if you must call async code from synchronous code, enter the runtime explicitly with something like `Runtime::block_on` (see [Tokio Setup](/11-async/03-tokio-setup/)). You cannot simply "await from anywhere," which is the **function coloring** problem also present in JavaScript, discussed in [Concurrency vs Parallelism](/11-async/10-concurrency/).

### Pitfall 3: Expecting the body to run on call

```typescript
// JavaScript: this logs "side effect!" immediately, even unawaited.
async function doWork() {
  console.log("side effect!");
}
doWork();
```

```rust
async fn do_work() {
    println!("side effect!"); // does NOT print on call
}

#[tokio::main]
async fn main() {
    do_work();        // builds a future, runs nothing (and warns must_use)
    do_work().await;  // NOW "side effect!" prints
}
```

If you ported the JavaScript expecting the first call to print, you would be surprised by silence. Always `.await` (or `spawn`) the future.

### Pitfall 4: `?` before `.await`

```rust
// Wrong shape: a future is not a Result, so `?` has nothing to operate on.
// let n = load()?.await;   // type error

// Right shape: await first, then propagate the Result.
// let n = load().await?;   //
```

Remember the pipeline order: `.await` turns the future into its `Output`; only then does `?` act on that `Output` (which must be a `Result` or `Option`).

---

## Best Practices

### Await at the edges, pass futures sparingly

Prefer writing straightforward `let x = thing().await?;` sequences. Only return `impl Future` from a synchronous function when you have a concrete reason (e.g., building combinators). For ordinary code, `async fn` reads best.

### Use `?` for propagation, reserve `match` for handling

Inside async functions, lean on `?` the same way you do in sync code. Only `match`/`if let` on a `Result` where you actually handle the error (logging, fallback, returning a different value). This keeps the happy path linear and readable.

### Keep the runtime at the top

Put `#[tokio::main]` on `main` (or construct the runtime once at startup) and let `async fn`s call each other freely below it. Avoid sprinkling `block_on` deep in your code; that re-enters the runtime and is a common source of "Cannot start a runtime from within a runtime" panics.

### Name your error type, then `?` everything

Define one error type for a module (often with `thiserror`) so that `?` can convert each underlying error via `From`. This is identical to the synchronous pattern in [Multiple Error Types](/08-error-handling/07-multiple-errors/); async changes nothing about it.

### Do not block inside async

Calling synchronous blocking APIs (a long CPU loop, `std::thread::sleep`, blocking file I/O) inside an `async fn` stalls the runtime's worker. Use the async equivalents (`tokio::time::sleep`, async I/O) or move the work to `spawn_blocking`; see [Spawning Tasks](/11-async/09-spawning-tasks/) and [Concurrency vs Parallelism](/11-async/10-concurrency/).

---

## Real-World Example

A small production-flavored slice of a service: load a user and their orders, then build a summary. It shows `async fn`, `.await`, `?` propagation through several layers, a real error type, and an async `main` returning `Result`. (It also previews `tokio::join!` for concurrency, fully covered in [Select & Join](/11-async/07-select-join/).)

```rust
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug)]
struct User {
    id: u32,
    name: String,
}

#[derive(Debug)]
struct Order {
    id: u32,
    total_cents: u64,
}

// A simulated repository error.
#[derive(Debug)]
enum RepoError {
    NotFound(u32),
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepoError::NotFound(id) => write!(f, "record {id} not found"),
        }
    }
}
impl std::error::Error for RepoError {}

async fn load_user(id: u32) -> Result<User, RepoError> {
    sleep(Duration::from_millis(40)).await; // simulate DB latency
    if id == 0 {
        return Err(RepoError::NotFound(id));
    }
    Ok(User { id, name: format!("Ada-{id}") })
}

async fn load_orders(user_id: u32) -> Result<Vec<Order>, RepoError> {
    sleep(Duration::from_millis(40)).await;
    Ok(vec![
        Order { id: user_id * 10 + 1, total_cents: 1299 },
        Order { id: user_id * 10 + 2, total_cents: 4900 },
    ])
}

// Build a per-user summary. The `?` propagates the first RepoError it sees.
async fn build_summary(user_id: u32) -> Result<String, RepoError> {
    // `join!` polls both futures on the same task concurrently and waits
    // for both. (Covered in depth in select-join.md.)
    let (user, orders) = tokio::join!(load_user(user_id), load_orders(user_id));
    let user = user?; // `?` on the Result returned by load_user
    let orders = orders?; // `?` on the Result returned by load_orders

    let total: u64 = orders.iter().map(|o| o.total_cents).sum();
    let first_order_id = orders.first().map(|o| o.id).unwrap_or(0);
    Ok(format!(
        "user #{} ({}) has {} orders (first #{}) totaling ${}.{:02}",
        user.id,
        user.name,
        orders.len(),
        first_order_id,
        total / 100,
        total % 100
    ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `?` in `main` works because main returns a Result.
    let summary = build_summary(42).await?;
    println!("{summary}");

    // The error path:
    match build_summary(0).await {
        Ok(s) => println!("{s}"),
        Err(e) => println!("failed: {e}"),
    }
    Ok(())
}
```

Real output:

```text
user #42 (Ada-42) has 2 orders (first #421) totaling $61.99
failed: record 0 not found
```

The equivalent TypeScript would look almost identical structurally (`async function`, `await`, `try/catch` or rejected Promises), but every `load_*` call here is a lazy future that only advances inside the Tokio runtime started by `#[tokio::main]`.

---

## Further Reading

### Official Documentation

- [The Rust Book — Async and Await](https://doc.rust-lang.org/book/ch17-01-futures-and-syntax.html)
- [Asynchronous Programming in Rust (the async book)](https://rust-lang.github.io/async-book/)
- [`std::future::Future`](https://doc.rust-lang.org/std/future/trait.Future.html)
- [Reference — `await` expressions](https://doc.rust-lang.org/reference/expressions/await-expr.html)
- [Tokio tutorial](https://tokio.rs/tokio/tutorial)

### Related Topics in This Guide

- [Promises vs Futures](/11-async/00-promises-vs-futures/) — the eager-vs-lazy difference in depth
- [Tokio Intro](/11-async/02-tokio-intro/) — why Rust needs an explicit runtime
- [Tokio Setup](/11-async/03-tokio-setup/) — `#[tokio::main]`, the runtime builder, `block_on`
- [Async Functions](/11-async/04-async-functions/) — `async` blocks, capturing, returning futures, lifetimes
- [Async Traits](/11-async/05-async-trait/) — native async fn in traits (stable since 1.75)
- [Select & Join](/11-async/07-select-join/) — `tokio::join!` / `select!` for concurrency
- [Spawning Tasks](/11-async/09-spawning-tasks/) — `tokio::spawn`, `spawn_blocking`
- [Concurrency vs Parallelism](/11-async/10-concurrency/) — when to use async at all; the function-coloring issue
- [The `?` Operator](/08-error-handling/01-question-mark/) — the foundation `?` builds on
- [Multiple Error Types](/08-error-handling/07-multiple-errors/) — one error type + `From` conversions
- [Functions](/03-functions/) — function fundamentals
- Next up: [Section 12 — Modules & Packages](/12-modules-packages/)

---

## Exercises

### Exercise 1

**Difficulty:** Beginner

**Objective:** Convert a synchronous fallible function into an `async fn` and propagate the error with `?`.

**Instructions:** Write `async fn parse_and_double(raw: &str) -> Result<i64, std::num::ParseIntError>` that (1) `.await`s a short `tokio::time::sleep` to simulate I/O, (2) parses `raw` into an `i64` using `?`, and (3) returns the value doubled. In `main`, print the results of `parse_and_double("21")` and `parse_and_double("oops")`.

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn parse_and_double(raw: &str) -> Result<i64, std::num::ParseIntError> {
    sleep(Duration::from_millis(5)).await;
    let n: i64 = raw.parse()?;
    Ok(n * 2)
}

#[tokio::main]
async fn main() {
    println!("{:?}", parse_and_double("21").await);
    println!("{:?}", parse_and_double("oops").await);
}
```

Real output:

```text
Ok(42)
Err(ParseIntError { kind: InvalidDigit })
```

</details>

### Exercise 2

**Difficulty:** Intermediate

**Objective:** Chain two dependent async steps where the second depends on the first, propagating errors with `?`.

**Instructions:** Given `async fn fetch_name(id: u32) -> Result<String, String>` (returns `Err` for `id == 0`) and `async fn fetch_bio(name: &str) -> Result<String, String>`, write `async fn build_profile(id: u32) -> Result<Profile, String>` that fetches the name, then uses it to fetch the bio, then returns a `Profile { id, bio }`. The two steps must run sequentially because the second needs the first's result. Print `build_profile(7)` and `build_profile(0)`.

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug)]
struct Profile {
    id: u32,
    bio: String,
}

async fn fetch_name(id: u32) -> Result<String, String> {
    sleep(Duration::from_millis(5)).await;
    if id == 0 {
        Err("no such user".to_string())
    } else {
        Ok(format!("name-{id}"))
    }
}

async fn fetch_bio(name: &str) -> Result<String, String> {
    sleep(Duration::from_millis(5)).await;
    Ok(format!("bio of {name}"))
}

async fn build_profile(id: u32) -> Result<Profile, String> {
    let name = fetch_name(id).await?;  // step 1, may short-circuit
    let bio = fetch_bio(&name).await?; // step 2 depends on step 1
    Ok(Profile { id, bio })
}

#[tokio::main]
async fn main() {
    println!("{:?}", build_profile(7).await);
    println!("{:?}", build_profile(0).await);
}
```

Real output:

```text
Ok(Profile { id: 7, bio: "bio of name-7" })
Err("no such user")
```

</details>

### Exercise 3

**Difficulty:** Intermediate

**Objective:** Demonstrate that a future is lazy by returning one from a *synchronous* function and awaiting it later.

**Instructions:** Write a synchronous `fn make_adder(base: i32) -> impl Future<Output = i32>` that returns an `async` block computing `base + 100`. In `main`, call `make_adder(5)`, print a line proving nothing has run yet, then `.await` it and print the result. Confirm from the output that the "not yet awaited" line prints before the future runs.

<details>
<summary>Solution</summary>

```rust
use std::future::Future;

fn make_adder(base: i32) -> impl Future<Output = i32> {
    async move { base + 100 }
}

#[tokio::main]
async fn main() {
    let fut = make_adder(5); // nothing has run yet
    println!("future built, not yet awaited");
    let result = fut.await; // now it runs
    println!("result = {result}");
}
```

Real output:

```text
future built, not yet awaited
result = 105
```

> **Note:** `make_adder` is not `async`, yet it returns a future — proof that `async fn` is just sugar for "a function returning `impl Future`," and that building the future does not run its body.

</details>
