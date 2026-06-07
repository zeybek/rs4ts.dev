---
title: "Async Functions and Async Blocks"
description: "Calling a TypeScript async function starts work immediately; Rust's async fn just builds a lazy future. Learn capturing, async move blocks, and lifetimes."
---

In TypeScript, `async function` is the workhorse of asynchronous code: call it and the body starts running immediately. Rust's `async fn` looks almost identical, but it behaves differently in one important way: calling it runs *nothing*. This page covers how `async fn` and `async {}` blocks work, how they capture data, what they actually return, and the lifetime rules that trip up every TypeScript/JavaScript developer.

---

## Quick Overview

An **`async fn`** in Rust is a function whose body is compiled into a state machine that implements the `Future` trait. Calling an `async fn` does **not** run the body. It builds a `Future` value that does nothing until you `.await` it (or hand it to a runtime). An **`async {}` block** is the same idea inline: an expression that evaluates to an anonymous `Future`. Understanding *capturing* (what data the future holds) and *lifetimes* (how long that data must live) is the key to writing async Rust that compiles.

> **Note:** This page focuses on the *shape* of async functions and blocks. For *why* futures are lazy, see [Promises vs Futures](/11-async/00-promises-vs-futures/); for the `.await` syntax and `?` error handling, see [async/await](/11-async/01-async-await/).

---

## TypeScript/JavaScript Example

Here is a typical async data layer in TypeScript: small async functions, one that composes others, and an inline async arrow function.

```typescript
// data-layer.ts
interface User {
  id: number;
  name: string;
}

interface Profile {
  user: User;
  postCount: number;
}

// A basic async function. Calling it STARTS the work immediately.
async function fetchUser(id: number): Promise<User> {
  const res = await fetch(`/api/users/${id}`);
  return res.json();
}

async function fetchPostCount(id: number): Promise<number> {
  const res = await fetch(`/api/users/${id}/posts/count`);
  const { count } = await res.json();
  return count;
}

// An async function that COMPOSES other async functions.
async function buildProfile(id: number): Promise<Profile> {
  // Promise.all runs both concurrently.
  const [user, postCount] = await Promise.all([
    fetchUser(id),
    fetchPostCount(id),
  ]);
  return { user, postCount };
}

// An inline async arrow function that CAPTURES `name` from its scope.
const makeGreeter = (name: string) => async (): Promise<string> => {
  return `Hello, ${name}!`;
};

const greet = makeGreeter("Ada");
greet().then(console.log); // "Hello, Ada!"
```

Two things to internalize before we cross over to Rust:

1. **Calling `fetchUser(id)` starts the network request right away.** The returned `Promise` is already "hot": it is running in the background.
2. **The arrow function captures `name` by reference** to the closure environment. JavaScript's garbage collector keeps `name` alive as long as the closure exists. You never think about it.

Rust changes *both* of these facts.

---

## Rust Equivalent

```rust
use std::future::Future;

// 1. A basic async fn. Calling it builds a Future; the body does NOT run yet.
async fn fetch_count() -> u32 {
    // pretend this hits the network
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    42
}

// 2. An async fn with parameters and a Result return type.
async fn parse_port(s: &str) -> Result<u16, std::num::ParseIntError> {
    let n: u16 = s.parse()?;
    Ok(n)
}

// 3. Returning a future explicitly via `-> impl Future`.
//    `async fn double_async` desugars to almost exactly this.
fn double_async(x: u32) -> impl Future<Output = u32> {
    async move { x * 2 }
}

// 4. Capturing: an `async move` block takes OWNERSHIP of `name`.
async fn build_greeting(name: String) -> String {
    let block = async move {
        // `name` is moved INTO the async block and lives inside the future.
        format!("Hello, {name}!")
    };
    block.await
}

// 5. Lifetimes: the returned future borrows `data`, so it cannot outlive it.
async fn first_word(data: &str) -> &str {
    data.split(' ').next().unwrap_or("")
}

#[tokio::main]
async fn main() {
    let count = fetch_count().await;
    println!("count = {count}");

    match parse_port("8080").await {
        Ok(p) => println!("port = {p}"),
        Err(e) => println!("bad port: {e}"),
    }

    let d = double_async(21).await;
    println!("double = {d}");

    let greeting = build_greeting(String::from("Ada")).await;
    println!("{greeting}");

    let sentence = String::from("rust is fun");
    let w = first_word(&sentence).await;
    println!("first word = {w}");
}
```

Real output (`cargo run`, Rust 1.96.0):

```text
count = 42
port = 8080
double = 42
Hello, Ada!
first word = rust
```

> **Note:** The examples here use [Tokio](/11-async/02-tokio-intro/) only to *run* the futures (the `#[tokio::main]` attribute and `tokio::time::sleep`). The `async fn` / `async {}` mechanics themselves are part of the language and need no crate. See [Tokio Setup](/11-async/03-tokio-setup/) for the `Cargo.toml`.

---

## Detailed Explanation

### `async fn` is sugar for "a function that returns a future"

When you write:

```rust
async fn double_async(x: u32) -> u32 {
    x * 2
}
```

the compiler rewrites it to roughly:

```rust
use std::future::Future;

fn double_async(x: u32) -> impl Future<Output = u32> {
    async move { x * 2 }
}
```

The declared return type (`u32`) becomes the future's **`Output`** type. The `-> impl Future<Output = u32>` form means "this returns *some* concrete type that implements `Future` and yields a `u32`" — the compiler generates an anonymous state-machine type for you. These two definitions are interchangeable; both compile and both print `2`:

```rust
use std::future::Future;

async fn add_one_sugar(x: i32) -> i32 { x + 1 }

fn add_one_desugar(x: i32) -> impl Future<Output = i32> {
    async move { x + 1 }
}

#[tokio::main]
async fn main() {
    println!("{}", add_one_sugar(1).await);   // 2
    println!("{}", add_one_desugar(1).await); // 2
}
```

Real output:

```text
2
2
```

> **Tip:** Use the plain `async fn` form 99% of the time. Reach for the explicit `-> impl Future` form only when you need to add bounds (like `+ Send` or a lifetime) that the sugar cannot express, or when the function body is *not itself* async but returns a future built elsewhere.

### Calling an `async fn` runs nothing

This is the single most important difference from TypeScript. In JavaScript, `fetchUser(1)` *starts* the request. In Rust, `fetch_count()` just *constructs a value*:

```rust
async fn do_work() -> u32 {
    println!("working...");
    42
}

#[tokio::main]
async fn main() {
    // Creating the future does NOT run the body.
    let fut = do_work();
    println!("future created, body has not run yet");
    let result = fut.await; // NOW the body runs.
    println!("result = {result}");
}
```

Real output:

```text
future created, body has not run yet
working...
result = 42
```

Notice `working...` prints *after* `future created...`, even though `do_work()` was called first. The body of an `async fn` only executes when the future is `.await`ed (or spawned onto a runtime, which polls it). This is covered in depth in [Promises vs Futures](/11-async/00-promises-vs-futures/).

### `async {}` blocks: inline futures

An `async {}` block is an expression that evaluates to an anonymous future, just like an `async fn` but without a name. The block's final expression is the future's `Output`:

```rust
let fut = async { 1 + 2 }; // type: impl Future<Output = i32>
let three = fut.await;     // 3
```

You will use async blocks constantly: to spawn ad-hoc work, to build a future inside a `match` arm, or to wrap a sequence of `.await`s you want to pass to `tokio::select!` or `join!` (see [select/join](/11-async/07-select-join/)).

### Capturing: `async {}` vs `async move {}`

A bare `async {}` block captures variables from the enclosing scope **by reference** (the least it needs), exactly like a closure does. An `async move {}` block captures **by value** (moves ownership in):

```rust
#[tokio::main]
async fn main() {
    let name = String::from("Grace");

    // Borrows `name` by default — fine because we await before `name` is dropped.
    let borrows = async {
        println!("borrowed: {}", name.len());
    };
    borrows.await;
    println!("still usable here: {name}");

    // `async move` takes ownership of `name`.
    let owns = async move {
        format!("owned: {name}")
    };
    println!("{}", owns.await);
    // `name` is no longer accessible here — it was moved into `owns`.
}
```

Real output:

```text
borrowed: 5
still usable here: Grace
owned: Grace
```

The rule of thumb mirrors closures (see [Closures](/03-functions/) if you have read Section 03): use `async move` whenever the future may outlive the current scope, which is *always* the case when you hand it to `tokio::spawn` or store it somewhere.

### Lifetimes in async functions

When an `async fn` takes a reference parameter and the future holds onto it, the future is implicitly bounded by that reference's lifetime. Consider:

```rust
async fn first_word(data: &str) -> &str {
    data.split(' ').next().unwrap_or("")
}
```

This desugars (conceptually) to:

```rust
use std::future::Future;

fn first_word<'a>(data: &'a str) -> impl Future<Output = &'a str> + 'a {
    async move { data.split(' ').next().unwrap_or("") }
}
```

The returned future borrows `data`, so the future **cannot outlive** `data`. You must `.await` it (and finish using the result) before `data` is dropped. This is why holding a future around longer than its borrowed inputs is a compile error, and why moving owned data into the future (the `String` version) is the fix when a task needs to live independently.

---

## Key Differences

| Concept | TypeScript `async function` | Rust `async fn` |
| --- | --- | --- |
| What calling it does | **Starts** the work immediately (eager) | **Builds a future**; body runs on `.await` (lazy) |
| Return type | Always `Promise<T>` | `impl Future<Output = T>` (anonymous type) |
| Needs a runtime? | No: the JS event loop is built in | **Yes**: futures need an executor like Tokio |
| Capturing variables | By reference, GC keeps them alive | By borrow (`async {}`) or by move (`async move {}`) |
| Lifetime of captured refs | Irrelevant (garbage collected) | Future must not outlive borrowed data |
| Concurrency primitive | `Promise.all` / `Promise.race` | `join!` / `tokio::select!` (see [select/join](/11-async/07-select-join/)) |
| Cancellation | Generally not cancellable once started | Drop the future to cancel it (it never ran past the last `.await`) |

The deepest conceptual gap is **laziness plus runtime**. A TypeScript developer is used to "call it and forget it — the event loop does the rest." In Rust there is no ambient event loop; a future is an inert value until something polls it. Forgetting to `.await` is a common bug, and the compiler warns you about it (see Pitfalls).

> **Note:** Because Rust futures are lazy and droppable, **cancellation is built in for free**: dropping a future before it completes simply stops it. In JavaScript you need `AbortController` plumbing to approximate this. See [Concurrency](/11-async/10-concurrency/) for cancellation patterns.

---

## Common Pitfalls

### Pitfall 1: Forgetting to `.await` (the future never runs)

Calling an `async fn` and discarding the result silently does nothing, but the compiler catches it with a `must_use` warning:

```rust
async fn do_work() -> u32 {
    42
}

#[tokio::main]
async fn main() {
    do_work(); // future created but never awaited — body never runs
    println!("done");
}
```

Real `cargo build` warning:

```text
warning: unused implementer of `Future` that must be used
 --> src/main.rs:7:5
  |
7 |     do_work(); // future created but never awaited — body never runs
  |     ^^^^^^^^^
  |
  = note: futures do nothing unless you `.await` or poll them
  = note: `#[warn(unused_must_use)]` on by default
```

The fix is to `.await` it (or `tokio::spawn` it). The note — *"futures do nothing unless you `.await` or poll them"* — is the laziness rule stated by the compiler itself.

### Pitfall 2: A spawned task borrowing local data (`'static` required)

`tokio::spawn` requires its future to be `'static`: it may run after the current function returns, so it cannot borrow local variables. This is the most common lifetime error TypeScript developers hit, because in JavaScript closures keep data alive automatically.

```rust
async fn print_len(data: &str) {
    println!("len = {}", data.len());
}

#[tokio::main]
async fn main() {
    let owned = String::from("hello");
    // does not compile (error[E0597]): `owned` does not live long enough
    tokio::spawn(print_len(&owned));
}
```

Real `cargo build` error:

```text
error[E0597]: `owned` does not live long enough
  --> src/main.rs:9:28
   |
 7 |     let owned = String::from("hello");
   |         ----- binding `owned` declared here
 8 |     // does not compile (error[E0597]): `owned` does not live long enough
 9 |     tokio::spawn(print_len(&owned));
   |     -----------------------^^^^^^--
   |     |                      |
   |     |                      borrowed value does not live long enough
   |     argument requires that `owned` is borrowed for `'static`
10 | }
   | - `owned` dropped here while still borrowed
```

The fix is to give the task **owned** data so its future is `'static`:

```rust
async fn print_len(data: String) {
    println!("len = {}", data.len());
}

#[tokio::main]
async fn main() {
    let owned = String::from("hello");
    let handle = tokio::spawn(print_len(owned)); // ownership moved in
    handle.await.unwrap();
}
```

Real output:

```text
len = 5
```

For sharing the *same* data across multiple tasks instead of moving it, see [Arc + Mutex](/11-async/12-arc-mutex-pattern/).

### Pitfall 3: Using `?` inside an async block whose `Output` is not a `Result`

The `?` operator needs the surrounding async block to return `Result` (or `Option`). If the block's value is a bare `u16`, you get a type error:

```rust
#[tokio::main]
async fn main() {
    let fut = async {
        let n: u16 = "8080".parse()?; // does not compile (error[E0277])
        n
    };
    let _ = fut.await;
}
```

Real `cargo build` error:

```text
error[E0277]: the `?` operator can only be used in an async block that returns `Result` or `Option` (or another type that implements `FromResidual`)
 --> src/main.rs:4:36
  |
3 |     let fut = async {
  |               ----- this function should return `Result` or `Option` to accept `?`
4 |         let n: u16 = "8080".parse()?; // does not compile (error[E0277])
  |                                    ^ cannot use the `?` operator in an async block that returns `u16`
```

The fix is to make the block yield a `Result`, annotating the type so inference knows the error variant:

```rust
#[tokio::main]
async fn main() {
    let fut = async {
        let n: u16 = "8080".parse()?;
        Ok::<u16, std::num::ParseIntError>(n)
    };
    match fut.await {
        Ok(n) => println!("port = {n}"),
        Err(e) => println!("bad: {e}"),
    }
}
```

Real output:

```text
port = 8080
```

> **Tip:** Inside an `async fn` you rarely hit this, because the function's declared return type (e.g. `-> Result<u16, ParseIntError>`) gives `?` a target. The annotation trick is only needed for *standalone async blocks*. More on `?` in [async/await](/11-async/01-async-await/).

---

## Best Practices

- **Prefer `async fn` over `-> impl Future`.** The sugar is clearer and produces the same machine code. Drop to the explicit form only to add bounds (`+ Send`, lifetimes) the sugar cannot express.
- **Use `async move` when handing a future to `spawn` or storing it.** If the future may outlive the current scope, it must own its data. Moving owned values in is almost always what you want for spawned tasks.
- **Pass owned data (`String`, `Vec<T>`) to `'static` tasks; share with `Arc` when many tasks need the same value.** See [Arc + Mutex](/11-async/12-arc-mutex-pattern/).
- **Keep `async fn` bodies focused on `.await`-driven I/O.** CPU-heavy loops inside an async fn block the executor thread; use `spawn_blocking` (see [Spawning Tasks](/11-async/09-spawning-tasks/)) or rethink with threads (see [Async vs Sync](/11-async/13-async-vs-sync/)).
- **Let lifetimes guide you.** If the compiler says a future does not live long enough, the question is "does this task need to outlive the borrowed data?" If yes, switch to owned data or `Arc`; if no, ensure you `.await` before the data is dropped.
- **Do not annotate the return type as `Future` by hand unless you must.** `async fn name() -> T` is the idiom; spelling out `Box<dyn Future<...>>` is only needed for `dyn` dispatch (see [Async Traits](/11-async/05-async-trait/)).

---

## Real-World Example

A small profile service: independent `async fn`s for each I/O step, composed by a higher-level `async fn` that drives them **concurrently** with `join!`. This is the Rust analogue of the `buildProfile` + `Promise.all` pattern from the TypeScript example. (The network calls are simulated with `sleep` so the snippet runs without a server.)

```rust
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug)]
struct User {
    id: u32,
    name: String,
}

#[derive(Debug)]
struct Profile {
    user: User,
    post_count: usize,
}

// Each async fn is an independent I/O step (here simulated with sleep).
async fn fetch_user(id: u32) -> User {
    sleep(Duration::from_millis(50)).await; // pretend: HTTP GET /users/{id}
    User { id, name: format!("user{id}") }
}

async fn fetch_post_count(id: u32) -> usize {
    sleep(Duration::from_millis(50)).await; // pretend: HTTP GET /users/{id}/posts
    (id as usize) * 3
}

// Compose async fns. `build_profile` is itself an async fn — its returned
// future drives the two inner futures CONCURRENTLY via join!.
async fn build_profile(id: u32) -> Profile {
    let (user, post_count) = tokio::join!(fetch_user(id), fetch_post_count(id));
    Profile { user, post_count }
}

#[tokio::main]
async fn main() {
    let start = std::time::Instant::now();
    let profile = build_profile(7).await;
    println!("{profile:?}");
    // Two 50ms steps ran concurrently, so total is ~50ms, not ~100ms.
    println!("elapsed: {} ms (concurrent)", start.elapsed().as_millis());
}
```

Real output (timing will vary slightly):

```text
Profile { user: User { id: 7, name: "user7" }, post_count: 21 }
elapsed: 51 ms (concurrent)
```

The two `fetch_*` futures are constructed *lazily* and only start making progress when `join!` polls them. Yet because `join!` polls both, they overlap, finishing in ~50ms rather than ~100ms. This is the same concurrency `Promise.all` gives you, but achieved by *one* task polling two futures, not by two background promises. See [select/join](/11-async/07-select-join/) for the full toolkit.

---

## Further Reading

- [The Async Book — `async`/`.await`](https://rust-lang.github.io/async-book/03_async_await/01_chapter.html): official explanation of the desugaring and state machine.
- [`std::future::Future`](https://doc.rust-lang.org/std/future/trait.Future.html): the trait every `async fn` returns.
- [Rust Reference — Async blocks and functions](https://doc.rust-lang.org/reference/expressions/block-expr.html#async-blocks): the precise capturing and lifetime rules.
- [Tokio Tutorial — Spawning](https://tokio.rs/tokio/tutorial/spawning): why spawned tasks need `'static`.
- Sibling pages in this section:
  - [Promises vs Futures](/11-async/00-promises-vs-futures/): eager vs lazy, the core mental shift.
  - [async/await](/11-async/01-async-await/): the `.await` operator and `?` error handling.
  - [Tokio Intro](/11-async/02-tokio-intro/) and [Tokio Setup](/11-async/03-tokio-setup/): getting a runtime.
  - [Async Traits](/11-async/05-async-trait/): `async fn` in traits and `dyn` dispatch.
  - [Spawning Tasks](/11-async/09-spawning-tasks/): `tokio::spawn`, `JoinHandle`, `spawn_blocking`.
  - [select/join](/11-async/07-select-join/): concurrent awaiting.
  - [Arc + Mutex](/11-async/12-arc-mutex-pattern/): sharing data across `'static` tasks.
- Related earlier sections: [Functions](/03-functions/), [Ownership](/05-ownership/), [Error Handling](/08-error-handling/).
- Next up after async: [Modules and Packages](/12-modules-packages/).

---

## Exercises

### Exercise 1: Convert a sync function to async

**Difficulty:** Easy

**Objective:** Get comfortable writing an `async fn` and `.await`ing it.

**Instructions:** Write an `async fn fetch_temperature(city: &str) -> f64` that "looks up" a temperature (simulate the I/O with `tokio::time::sleep` for 20ms, then return a hard-coded value per city). Call it from `#[tokio::main] async fn main` and print the result for `"Cairo"`.

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn fetch_temperature(city: &str) -> f64 {
    sleep(Duration::from_millis(20)).await; // simulated network call
    match city {
        "Berlin" => 18.5,
        "Cairo" => 31.0,
        _ => 20.0,
    }
}

#[tokio::main]
async fn main() {
    let t = fetch_temperature("Cairo").await;
    println!("Cairo: {t} C");
}
```

Real output:

```text
Cairo: 31 C
```

</details>

### Exercise 2: An async function that returns a borrow

**Difficulty:** Medium

**Objective:** See how a borrowed return value ties the future's lifetime to its input.

**Instructions:** Write an `async fn longest_line(text: &str) -> &str` that returns the longest line of `text` (split on `\n`). The returned `&str` must borrow from `text`. Call it on a multi-line `String` and print the result. Observe that you must keep the `String` alive until after the `.await`.

<details>
<summary>Solution</summary>

```rust
// Returns a slice borrowed from the input; the future's lifetime is tied to `text`.
async fn longest_line(text: &str) -> &str {
    text.lines().max_by_key(|line| line.len()).unwrap_or("")
}

#[tokio::main]
async fn main() {
    let doc = String::from("short\na much longer line\nmid");
    let line = longest_line(&doc).await;
    println!("longest: {line:?}");
}
```

Real output:

```text
longest: "a much longer line"
```

> The future returned by `longest_line(&doc)` borrows `doc`. Because we `.await` it (and finish using `line`) before `doc` goes out of scope, the borrow checker is satisfied.

</details>

### Exercise 3: Fix a spawn lifetime error and run two tasks concurrently

**Difficulty:** Hard

**Objective:** Resolve the `'static` requirement of `tokio::spawn` and combine results with `join!`.

**Instructions:** You have an `async fn process(label: String, ms: u64) -> String` that sleeps for `ms` milliseconds and returns `"<label> done in <ms>ms"`. Spawn two tasks (labels `"A"` at 30ms and `"B"` at 10ms), then await both with `tokio::join!` and print each result. Make sure the futures are `'static` (hint: move owned `String`s in — do not pass borrows).

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn process(label: String, ms: u64) -> String {
    sleep(Duration::from_millis(ms)).await;
    format!("{label} done in {ms}ms")
}

#[tokio::main]
async fn main() {
    // Move owned Strings into each task so the futures are 'static.
    let h1 = tokio::spawn(process(String::from("A"), 30));
    let h2 = tokio::spawn(process(String::from("B"), 10));

    // join! awaits both JoinHandles concurrently.
    let (r1, r2) = tokio::join!(h1, h2);
    println!("{}", r1.unwrap());
    println!("{}", r2.unwrap());
}
```

Real output:

```text
A done in 30ms
B done in 10ms
```

> Passing owned `String`s (not `&str`) makes each future `'static`, satisfying `tokio::spawn`. The two tasks run concurrently; `join!` waits for both `JoinHandle`s and yields their results. See [Spawning Tasks](/11-async/09-spawning-tasks/) and [select/join](/11-async/07-select-join/) for more.

</details>
