---
title: "Panicking: `panic!` vs Recoverable Errors"
description: "Rust splits failure into recoverable Result and unrecoverable panic!, unlike JavaScript's single throw. Learn unwinding vs abort, catch_unwind, and when to crash."
---

In TypeScript and JavaScript, `throw` is your one tool for "something went wrong," whether it's a missing file, bad user input, or a genuine bug. Rust deliberately splits these into two categories: **recoverable** errors (modeled with `Result<T, E>`) and **unrecoverable** errors (signalled with `panic!`). Knowing which is which, and why Rust draws the line, is the heart of writing idiomatic, solid Rust.

---

## Quick Overview

A **panic** is Rust's way of saying "this should never happen; the program is in a state I cannot safely continue from." Reaching one unwinds the current thread (running destructors as it goes) and, by default, terminates the program. Unlike a JavaScript `throw`, a panic is **not** the normal way to report expected failures; those belong in a `Result`. Use `panic!` for **bugs and broken invariants**, and use `Result` for **anything a caller could reasonably handle**.

> **Note:** This page focuses on `panic!`, unwinding vs. aborting, and when panicking is appropriate. The recoverable side — `Result<T, E>` and `Option<T>` — is covered in [Result and Option](/08-error-handling/00-result-option/), and the `unwrap`/`expect` shortcuts that *deliberately* turn an `Err` into a panic live in [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/).

---

## TypeScript/JavaScript Example

In JavaScript there is exactly one mechanism for signalling failure: `throw`. The same keyword reports user-input problems *and* programmer bugs, and a single `try/catch` can swallow both.

```typescript
// JavaScript/TypeScript: `throw` is used for EVERYTHING.

// (1) Expected, recoverable failure — bad user input.
function parsePort(raw: string): number {
  const port = Number(raw);
  if (!Number.isInteger(port) || port < 0 || port > 65535) {
    throw new Error(`invalid port: ${raw}`);
  }
  return port;
}

// (2) A genuine bug / broken invariant — should "never" happen.
function applyDiscount(price: number, percent: number): number {
  if (percent < 0 || percent > 100) {
    throw new Error(`percent must be 0..100, got ${percent}`);
  }
  return price - (price * percent) / 100;
}

// Both kinds of failure are caught the same way:
try {
  const port = parsePort(process.argv[2] ?? "");
  console.log(`listening on ${port}`);
} catch (err) {
  // A bad CLI argument and an internal bug land in the same handler.
  console.error("startup failed:", (err as Error).message);
  process.exit(1);
}
```

Because `throw` and `catch` are untyped and uniform, nothing in the signature of `parsePort` or `applyDiscount` tells you whether a call *can* fail, what it fails *with*, or whether the failure represents recoverable user error or an internal bug.

---

## Rust Equivalent

Rust splits the two cases apart. Recoverable failure is a value (`Result`); an unrecoverable bug is a `panic!`.

```rust playground
// Rust: two distinct mechanisms.

// (1) Recoverable failure -> the type system forces the caller to deal with it.
fn parse_port(raw: &str) -> Result<u16, std::num::ParseIntError> {
    raw.trim().parse::<u16>()
}

// (2) Broken invariant / bug -> panic. `assert!` panics with a message
//     when its condition is false.
fn apply_discount(price: u32, percent: u32) -> u32 {
    assert!(percent <= 100, "percent must be 0..=100, got {percent}");
    price - (price * percent / 100)
}

fn main() {
    // Propagate the Result; discarding it would trigger an unused_must_use warning.
    match parse_port("8080") {
        Ok(p) => println!("port = {p}"),
        Err(e) => println!("bad port: {e}"),
    }
    match parse_port("not-a-number") {
        Ok(p) => println!("port = {p}"),
        Err(e) => println!("bad port: {e}"),
    }

    println!("discounted = {}", apply_discount(200, 25));
}
```

**Real output:**

```text
port = 8080
bad port: invalid digit found in string
discounted = 150
```

The signature `-> Result<u16, ParseIntError>` is a *promise in the type*: this call can fail, and here is the error type. By contrast, `apply_discount` returns a plain `u32` (its signature says "this always succeeds") and the only way it *doesn't* is if a caller violates the documented contract, which is a bug worth crashing on.

---

## Detailed Explanation

### What `panic!` actually does

`panic!("message")` is a macro (note the `!`, like `println!`). When executed it:

1. Prints the panic message and source location to **stderr**.
2. Optionally captures a backtrace (controlled by the `RUST_BACKTRACE` environment variable).
3. **Unwinds** the current thread by default, walking back up the call stack and running each value's destructor (`Drop`) so resources like files, locks, and heap allocations are released.
4. If the panicking thread is the main thread (and nothing catches the unwind), the process exits with a non-zero status.

```rust
fn main() {
    println!("about to panic");
    panic!("something went terribly wrong");
}
```

**Real output (default debug build):**

```text
about to panic

thread 'main' panicked at src/main.rs:3:5:
something went terribly wrong
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

The process exits with status code **101** (Rust's conventional exit code for an unwinding panic). Compare this with a JavaScript uncaught exception, which prints a stack trace and exits with code `1`.

### Backtraces are opt-in

Notice the line `note: run with RUST_BACKTRACE=1 ...`. Unlike a Node.js exception, which always prints a full stack trace, Rust **omits** the backtrace by default for speed and noise reduction. Set the environment variable to see it:

```rust
fn main() {
    let v = vec![1, 2, 3];
    println!("trying to access index 10");
    let _x = v[10]; // out-of-bounds index -> panic
    println!("unreachable");
}
```

Run with `RUST_BACKTRACE=1 cargo run`:

```text
trying to access index 10

thread 'main' panicked at src/main.rs:4:15:
index out of bounds: the len is 3 but the index is 10
stack backtrace:
   0: __rustc::rust_begin_unwind
             at /rustc/.../library/std/src/panicking.rs:697:5
   1: core::panicking::panic_fmt
             at /rustc/.../library/core/src/panicking.rs:75:14
   2: core::panicking::panic_bounds_check
             at /rustc/.../library/core/src/panicking.rs:280:5
   ...
   6: probe::main
             at ./src/main.rs:4:15
   7: core::ops::function::FnOnce::call_once
             at /rustc/.../library/core/src/ops/function.rs:253:5
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
```

Indexing out of bounds is one of several library operations that panic on a broken precondition. `RUST_BACKTRACE=full` shows every frame including std internals.

> **Tip:** Many bugs in Rust manifest as a panic from a library function (`index out of bounds`, `called Option::unwrap() on a None value`, `attempt to divide by zero`). When you hit one, set `RUST_BACKTRACE=1` to find the exact line in *your* code that triggered it.

### Unwinding runs your destructors

By default a panic unwinds, and unwinding is *not* a silent crash: every value on the stack is dropped in reverse order, so cleanup code runs. Rust has no `finally`; the `Drop` trait fills that role.

```rust playground
struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        println!("Guard::drop ran during unwinding (cleanup happened)");
    }
}

fn risky() {
    let _g = Guard;
    panic!("boom in risky()");
}

fn main() {
    let result = std::panic::catch_unwind(|| {
        risky();
    });

    match result {
        Ok(_) => println!("no panic"),
        Err(_) => println!("caught a panic, main continues"),
    }

    println!("main finished normally");
}
```

**Real output:**

```text

thread 'main' panicked at src/main.rs:11:5:
boom in risky()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
Guard::drop ran during unwinding (cleanup happened)
caught a panic, main continues
main finished normally
```

Two things to take away. First, `Guard::drop` ran *during* the unwind: your cleanup is honored. Second, `std::panic::catch_unwind` can intercept an unwinding panic and turn it into a `Result`. That looks like `try/catch`, but it is emphatically **not** how you handle ordinary errors (see Pitfalls). It exists for narrow cases: stopping a panic from crossing a thread or a foreign-function-interface (FFI) boundary, and in test harnesses.

### A panic kills the thread, not necessarily the program

If a *spawned* thread panics, only that thread dies. The panic is stored in the thread's `JoinHandle` and surfaces as an `Err` when you `join` it.

```rust playground
use std::thread;

fn main() {
    let handle = thread::spawn(|| {
        panic!("worker thread blew up");
    });

    let result = handle.join();
    match result {
        Ok(()) => println!("worker finished cleanly"),
        Err(_) => println!("worker panicked, but main is still alive"),
    }

    println!("main thread keeps running");
}
```

**Real output:**

```text

thread '<unnamed>' panicked at src/main.rs:5:9:
worker thread blew up
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
worker panicked, but main is still alive
main thread keeps running
```

This is closer to how an unhandled rejection in one async task behaves than to a process-wide crash, but it only applies to *additional* threads. A panic on the main thread that nobody catches ends the program.

---

## Key Differences

### Two failure mechanisms, not one

| Concept | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| Expected/recoverable failure | `throw` + `try/catch` | `Result<T, E>` value (see [Result and Option](/08-error-handling/00-result-option/)) |
| Bug / impossible state | `throw` (same as above) | `panic!` / `assert!` / `unreachable!` |
| Shows up in the signature? | No (untyped) | Yes: `Result` is in the return type; panics are not |
| Default cleanup | `finally` blocks | `Drop` destructors run during unwind |
| Catching | `try/catch` (idiomatic, everywhere) | `catch_unwind` (rare, boundary-only) |
| Process exit code | `1` for uncaught exception | `101` for unwinding panic, `134` (`SIGABRT`) for abort |

### Unwinding vs. aborting

A panic can be handled by the runtime in one of two **panic strategies**:

- **`unwind`** (the default): walk the stack, run destructors, optionally let `catch_unwind` stop the unwind. Slightly larger binaries (they carry unwinding tables) and a little runtime cost, but cleanup is guaranteed.
- **`abort`**: immediately terminate the process via the platform's abort (`SIGABRT`). No destructors run, no `catch_unwind`, smaller binaries. Common for embedded targets, some release builds, and anywhere you want a panic to be a hard, fast stop.

You select `abort` per build profile in `Cargo.toml`:

```toml
# Cargo.toml — abort instead of unwinding on panic
[profile.release]
panic = "abort"
```

Here is the difference made concrete. Under `panic = "abort"`, the destructor does **not** run:

```rust
struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        println!("this should NOT print under panic=abort");
    }
}

fn main() {
    let _g = Guard;
    println!("about to panic with abort strategy");
    panic!("aborting now");
}
```

**Real output with `panic = "abort"`:**

```text
about to panic with abort strategy

thread 'main' panicked at src/main.rs:12:5:
aborting now
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

The `Guard::drop` line never prints, and the process exits with code **134** (128 + signal 6, `SIGABRT`) instead of the `101` you get when unwinding. JavaScript has no equivalent toggle: an uncaught exception always unwinds the JS call stack and runs `finally` blocks.

| | `unwind` (default) | `abort` |
| --- | --- | --- |
| Runs `Drop` destructors | Yes | No |
| `catch_unwind` can intercept | Yes | No |
| Binary size | Larger (unwind tables) | Smaller |
| Exit on uncaught main-thread panic | code `101` | `SIGABRT` (code `134` on Unix) |
| Typical use | apps, libraries, tests | embedded, size-/speed-critical release builds |

> **Note:** Even with the default `unwind` strategy, a *second* panic that occurs *while already unwinding* (for example, a `Drop` impl that itself panics) escalates straight to an abort. Keep destructors panic-free.

---

## Common Pitfalls

### Pitfall 1: Treating `panic!` like JavaScript `throw`

The single most common mistake is reaching for `panic!` (or `unwrap`/`expect`) to report an expected failure such as bad input, a missing file, or a failed network call. That is what `Result` is for. Panicking on recoverable conditions makes your library unusable as a dependency: callers cannot recover, and the panic crashes *their* program.

```rust
// Anti-pattern: panicking on ordinary, recoverable input failure.
fn parse_port(raw: &str) -> u16 {
    raw.parse().expect("invalid port") // crashes the whole program on bad input
}

// Idiomatic: return a Result and let the caller decide.
fn parse_port_ok(raw: &str) -> Result<u16, std::num::ParseIntError> {
    raw.parse()
}
```

Both compile. The first is a *design* bug: a parsing routine should never decide to terminate the process. See [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/) for the (narrow) cases where `expect` is justified.

### Pitfall 2: Using `catch_unwind` as a general `try/catch`

`std::panic::catch_unwind` superficially resembles `try/catch`, so newcomers try to use it to handle ordinary errors. Resist. It is meant for *boundaries* (don't let a panic unwind across an FFI call or take down a thread pool), not for control flow. It also requires the captured data to be `UnwindSafe`, and many ordinary types are not, so it often won't even compile:

```rust
use std::cell::RefCell;
use std::panic;

fn main() {
    let shared = RefCell::new(0);
    // does not compile (error E0277: RefCell is not UnwindSafe)
    let result = panic::catch_unwind(|| {
        *shared.borrow_mut() += 1;
    });
    println!("{result:?}");
}
```

The real compiler error:

```text
error[E0277]: the type `UnsafeCell<i32>` may contain interior mutability and a
reference may not be safely transferrable across a catch_unwind boundary
   --> src/main.rs:7:38
    |
  7 |       let result = panic::catch_unwind(|| {
    |  __________________-------------------_^
    | |                  required by a bound introduced by this call
  8 | |         *shared.borrow_mut() += 1;
  9 | |     });
    | |_____^ `UnsafeCell<i32>` may contain interior mutability and a reference may
    |         not be safely transferrable across a catch_unwind boundary
    = help: within `RefCell<i32>`, the trait `RefUnwindSafe` is not implemented
```

The `UnwindSafe` bound is the compiler steering you away from using panics for flow control: catching a panic mid-mutation could leave a value in a logically broken state. If you find yourself reaching for `catch_unwind` in business logic, you almost certainly want `Result` instead.

### Pitfall 3: Assuming `abort` still runs cleanup

If you set `panic = "abort"` for the performance and binary-size benefits, remember that destructors **do not run** on panic. Code that relies on `Drop` for critical cleanup on the panic path (flushing a buffer, releasing an OS resource) will silently skip it. With `unwind`, that same cleanup runs.

### Pitfall 4: Expecting a panic to print a stack trace by default

Coming from Node.js, you may expect every crash to dump a stack trace. Rust does not, for performance reasons; you only get the message and source location unless `RUST_BACKTRACE=1` (or `full`) is set. In production, set it via the environment of your service so crash logs are actionable.

### Pitfall 5: Forgetting that a panicked thread does not stop `main`

A panic in a spawned thread is captured in its `JoinHandle` and does not propagate automatically. If you `spawn` work and never `join` (or never check the result), a worker can die silently while the rest of the program carries on as though nothing happened. Always inspect `handle.join()` when a thread's success matters.

---

## Best Practices

- **Default to `Result`.** If a caller could plausibly want to recover — bad input, a missing file, a timeout, a failed parse — return a `Result`. Reserve panics for situations that genuinely indicate a bug. The companion pages [Result and Option](/08-error-handling/00-result-option/) and [The `?` Operator](/08-error-handling/01-question-mark/) show how to make `Result` ergonomic with the `?` operator.
- **Panic on broken invariants, not on user input.** A function that receives an argument violating its documented contract (an index past the end of an array, a probability outside `0.0..=1.0`) is entitled to panic: the *caller* has a bug. Use `assert!`, `assert_eq!`, and `debug_assert!` to express these checks. Document them under a `# Panics` heading in the doc comment.
- **Write good panic messages.** A panic should explain *what* invariant was violated and ideally *what value* broke it. `assert!(percent <= 100, "percent must be 0..=100, got {percent}")` is far more useful in a crash log than a bare `assert!(percent <= 100)`.
- **Use the right "this should never happen" macro.** Rust ships several:

  | Macro | Meaning | Panics when reached? |
  | --- | --- | --- |
  | `panic!("msg")` | Explicit, unconditional crash | Always |
  | `assert!(cond, "msg")` | Crash if `cond` is false | Conditionally |
  | `unreachable!()` | "Control flow can never get here" | Always (if it does) |
  | `todo!()` | Placeholder for unwritten code | Always |
  | `unimplemented!()` | Intentionally not implemented | Always |

  `todo!` and `unimplemented!` satisfy the type checker (they return `!`, the never type) so you can stub a function and keep compiling:

  ```rust
  fn serialize(_d: &Direction) -> String {
      todo!("serialization not implemented yet")
  }
  ```

  Reaching it panics with `not yet implemented: serialization not implemented yet`. We cover the never type and the type-level reasoning in [Generics & Traits](/09-generics-traits/).

- **Choose your panic strategy deliberately.** Keep the default `unwind` for servers and libraries where graceful teardown matters; consider `panic = "abort"` for embedded targets or when you want the smallest, fastest binary and a panic is always fatal anyway.
- **Test the panic paths.** Use `#[should_panic(expected = "...")]` to assert that a function panics on a contract violation, so the behavior is locked in:

  ```rust
  #[test]
  #[should_panic(expected = "whole must be non-zero")]
  fn panics_on_zero_whole() {
      percentage(1, 0);
  }
  ```

  See [Testing](/13-testing/) for the full testing story.

---

## Real-World Example

A production service typically faces both kinds of failure during startup. User-supplied configuration is **recoverable**: report it and let the operator fix it. An internal helper called with a logically impossible argument represents a **bug** and should panic loudly. Here is a config loader that draws that line clearly.

```rust playground
use std::collections::HashMap;

/// A recoverable error: the operator can fix bad config and retry.
#[derive(Debug)]
enum ConfigError {
    MissingKey(String),
    InvalidNumber { key: String, value: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingKey(k) => write!(f, "missing required key: {k}"),
            ConfigError::InvalidNumber { key, value } => {
                write!(f, "key `{key}` has invalid number: {value:?}")
            }
        }
    }
}

struct Config {
    port: u16,
    workers: u32,
}

/// Parsing operator-supplied config is RECOVERABLE -> return a Result.
fn load_config(raw: &HashMap<&str, &str>) -> Result<Config, ConfigError> {
    let port_str = raw
        .get("port")
        .ok_or_else(|| ConfigError::MissingKey("port".into()))?;
    let port = port_str
        .parse::<u16>()
        .map_err(|_| ConfigError::InvalidNumber {
            key: "port".into(),
            value: (*port_str).to_string(),
        })?;

    let workers_str = raw.get("workers").copied().unwrap_or("4");
    let workers = workers_str
        .parse::<u32>()
        .map_err(|_| ConfigError::InvalidNumber {
            key: "workers".into(),
            value: workers_str.to_string(),
        })?;

    Ok(Config { port, workers })
}

/// Splits `total_jobs` evenly across `workers`. A zero worker count is a
/// PROGRAMMER bug at this layer (the config layer guarantees >= 1), so we
/// treat it as unrecoverable.
///
/// # Panics
/// Panics if `workers` is zero.
fn chunk_size(total_jobs: u32, workers: u32) -> u32 {
    assert!(workers > 0, "worker pool must have at least one worker");
    total_jobs.div_ceil(workers)
}

fn main() {
    let mut raw = HashMap::new();
    raw.insert("port", "8080");
    raw.insert("workers", "3");

    match load_config(&raw) {
        Ok(cfg) => {
            println!("listening on port {}", cfg.port);
            println!("chunk size = {}", chunk_size(100, cfg.workers));
        }
        Err(e) => eprintln!("config error: {e}"),
    }

    // Recoverable path: a bad value is reported and the program keeps running.
    let mut bad = HashMap::new();
    bad.insert("port", "70000"); // > u16::MAX
    match load_config(&bad) {
        Ok(_) => println!("unexpected ok"),
        Err(e) => println!("rejected bad config: {e}"),
    }
}
```

**Real output:**

```text
listening on port 8080
chunk size = 34
rejected bad config: key `port` has invalid number: "70000"
```

The two flavors of failure are visible in the *signatures*. `load_config` returns `Result<Config, ConfigError>`: bad config is data the caller must handle, and the program survives it. `chunk_size` returns a plain `u32` with a documented `# Panics` clause: it trusts its caller, and a zero worker count would be a contradiction the config layer already ruled out, so if it ever happens, crashing is the correct, loud response. For richer error types and the `anyhow`/`thiserror` crates that production code reaches for, see [Custom Error Types](/08-error-handling/04-custom-errors/) and [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/).

---

## Further Reading

- [The Rust Programming Language — "To `panic!` or Not to `panic!`"](https://doc.rust-lang.org/book/ch09-03-to-panic-or-not-to-panic.html)
- [The Rust Programming Language — "Unrecoverable Errors with `panic!`"](https://doc.rust-lang.org/book/ch09-01-unrecoverable-errors-with-panic.html)
- [`std::panic`](https://doc.rust-lang.org/std/panic/index.html) and [`std::panic::catch_unwind`](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html)
- [`panic!` macro](https://doc.rust-lang.org/std/macro.panic.html), [`todo!`](https://doc.rust-lang.org/std/macro.todo.html), [`unreachable!`](https://doc.rust-lang.org/std/macro.unreachable.html)
- [The Cargo Book — profile `panic` setting](https://doc.rust-lang.org/cargo/reference/profiles.html#panic)
- Related sections in this guide:
  - [Result and Option](/08-error-handling/00-result-option/) — the recoverable counterpart to panicking
  - [The `?` Operator](/08-error-handling/01-question-mark/) — propagating recoverable errors with `?`
  - [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/) — when turning an error into a panic is acceptable
  - [Error-Handling Best Practices](/08-error-handling/08-best-practices/) — recoverable vs. unrecoverable error design
  - [Section 08 overview](/08-error-handling/) · [Section 00 — Introduction](/00-introduction/) · [Section 01 — Getting Started](/01-getting-started/) · [Section 02 — Basics](/02-basics/) · [Section 09 — Generics & Traits](/09-generics-traits/)

---

## Exercises

### Exercise 1: From panic to `Result`

**Difficulty:** Easy

**Objective:** Recognize when a panic is the wrong tool and convert it into a recoverable error.

**Instructions:** The function below panics on division by zero, but division by zero is something a caller could reasonably want to handle, not a bug. Rewrite it to return a `Result<i64, MathError>` where `MathError` is your own error type. Demonstrate both the `Ok` and `Err` paths from `main`.

```rust
// Starting point — refactor this to return a Result.
fn divide(numerator: i64, denominator: i64) -> i64 {
    if denominator == 0 {
        panic!("division by zero"); // recoverable; shouldn't panic
    }
    numerator / denominator
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug, PartialEq)]
enum MathError {
    DivideByZero,
}

fn safe_divide(numerator: i64, denominator: i64) -> Result<i64, MathError> {
    if denominator == 0 {
        return Err(MathError::DivideByZero);
    }
    Ok(numerator / denominator)
}

fn main() {
    println!("{:?}", safe_divide(10, 2));  // Ok(5)
    println!("{:?}", safe_divide(10, 0));  // Err(DivideByZero)
}
```

**Output:**

```text
Ok(5)
Err(DivideByZero)
```

The caller now decides what to do with a zero denominator instead of having the program ripped out from under it.

</details>

### Exercise 2: A justified panic with a documented invariant

**Difficulty:** Medium

**Objective:** Identify a case where panicking *is* correct and express the contract clearly.

**Instructions:** Write `pick_wrapping<T>(slice: &[T], index: usize) -> &T` that returns the element at `index`, wrapping around with the modulo operator so any index is valid — *except* when the slice is empty, which is a programmer error (there is nothing to return). Use `assert!` with a clear message, document the panic in a `# Panics` doc comment, and show it returning a value for an out-of-range index on a non-empty slice.

<details>
<summary>Solution</summary>

```rust playground
/// Picks element `index` from `slice`, wrapping around with modulo.
///
/// # Panics
/// Panics if `slice` is empty (there is nothing to pick).
fn pick_wrapping<T>(slice: &[T], index: usize) -> &T {
    assert!(!slice.is_empty(), "cannot pick from an empty slice");
    &slice[index % slice.len()]
}

fn main() {
    let colors = ["red", "green", "blue"];
    println!("{}", pick_wrapping(&colors, 7)); // 7 % 3 == 1 -> "green"
}
```

**Output:**

```text
green
```

The empty-slice case is genuinely impossible to serve, so panicking is the right call — and the `# Panics` clause makes the contract explicit. Pass `&[] as &[&str]` and it would panic with `cannot pick from an empty slice`.

</details>

### Exercise 3: A panic boundary with `catch_unwind`

**Difficulty:** Hard

**Objective:** Use `catch_unwind` for its legitimate purpose: isolating untrusted code so a panic in it does not take down the host.

**Instructions:** Imagine a plugin host that runs third-party closures. A misbehaving plugin should not crash the host. Write `run_plugin<F: FnOnce() + UnwindSafe>(name: &str, plugin: F)` that runs the closure inside `std::panic::catch_unwind`, prints whether it succeeded or panicked, and lets the host continue. Run one well-behaved plugin and one that panics.

<details>
<summary>Solution</summary>

```rust playground
fn run_plugin<F: FnOnce() + std::panic::UnwindSafe>(name: &str, plugin: F) {
    match std::panic::catch_unwind(plugin) {
        Ok(()) => println!("plugin `{name}` ran successfully"),
        Err(_) => println!("plugin `{name}` panicked; host stays up"),
    }
}

fn main() {
    run_plugin("good", || println!("  doing safe work"));
    run_plugin("bad", || panic!("plugin exploded"));
    println!("host still running");
}
```

**Output:**

```text
  doing safe work
plugin `good` ran successfully

thread 'main' panicked at src/main.rs:10:26:
plugin exploded
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
plugin `bad` panicked; host stays up
host still running
```

The panic message still prints (via the panic hook), but the unwind is *caught* at the host boundary, so the program survives and the next plugin runs. This is the legitimate use of `catch_unwind` — note it requires the closure to be `UnwindSafe`, and under `panic = "abort"` it can never actually catch anything: the process is terminated at the panic point before the catch can return `Err`.

</details>
