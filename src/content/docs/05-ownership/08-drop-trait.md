---
title: "The Drop Trait and RAII"
description: "Rust's Drop trait and RAII free resources deterministically at scope exit, replacing JavaScript's garbage collector and manual try/finally cleanup with a guarantee."
---

Ownership Rule 3 says a value is dropped when its owner goes out of scope. The **`Drop` trait** is how you hook into that moment to run custom cleanup, and **RAII** (Resource Acquisition Is Initialization) is the pattern that falls out of it: tie a resource's lifetime to a value's scope and let the compiler free it for you, deterministically, at a point you can read off the source.

---

## Quick Overview

In Rust, cleanup is **deterministic and scope-bound**: the compiler inserts a call to a value's destructor at the exact `}` where its owner goes out of scope, with no runtime tracking and no garbage collector. You implement the `Drop` trait to say *what* should happen at that moment (close a file, release a lock, roll back a transaction), and you can force cleanup *early* with `std::mem::drop`. For a TypeScript/JavaScript developer, this is the opposite of the garbage collector: instead of "freed eventually, at a time the engine chooses," Rust gives you "freed *here*, every time."

> **Note:** This page is about the cleanup *mechanism*: the `Drop` trait, RAII, drop order, and `std::mem::drop`. The ownership Rule 3 that triggers drops lives in [The Three Ownership Rules](/05-ownership/01-ownership-rules/); the stack/heap split that explains *what* gets freed is in [Stack and Heap](/05-ownership/00-stack-heap/).

---

## TypeScript/JavaScript Example

In JavaScript and TypeScript, you never decide *when* an object's memory is reclaimed. The **garbage collector** frees objects at some unspecified time after they become unreachable. For non-memory resources (files, sockets, locks), the convention is a manual `close()`/`dispose()` you must remember to call, usually wrapped in `try/finally`.

```typescript
// TypeScript/JavaScript: cleanup is either manual (try/finally) or non-deterministic (GC).
class DbConnection {
  constructor(public readonly id: number) {
    console.log(`[conn ${id}] opening`);
  }

  close(): void {
    console.log(`[conn ${this.id}] closing`);
  }
}

function handleRequest(): void {
  const conn = new DbConnection(1);
  try {
    console.log("running query...");
    // ... work that might throw ...
  } finally {
    conn.close(); // YOU must remember this, in every exit path
  }
}

handleRequest();

// For pure-memory objects, there is no close() — the GC frees them "eventually".
// FinalizationRegistry can run a callback after collection, but the docs are explicit:
// it offers NO timing guarantee and the callback may never run at all.
const registry = new FinalizationRegistry((label: string) => {
  console.log(`finalized: ${label}`); // may fire late, or never
});
let cache: object | null = { big: "buffer" };
registry.register(cache, "cache");
cache = null; // now unreachable — but when is it collected? Unknown.
```

**Key points:**

- Memory cleanup is **non-deterministic**: the GC runs on its own schedule.
- Non-memory resources need a **manual** `close()`/`dispose()`, and you must call it on every exit path (hence `try/finally`).
- `FinalizationRegistry` is explicitly documented as offering **no timing guarantees** and may never run; it is a last-ditch safety net, not a destructor.
- Forgetting a `close()` leaks the resource (a file descriptor, a held lock) even though the memory is eventually GC'd.

---

## Rust Equivalent

Rust ties cleanup to scope. You implement `Drop::drop` once, and the compiler runs it automatically at the closing `}`, on *every* exit path, including early returns and panics. No `try/finally`, no `close()` to forget.

```rust
struct DbConnection {
    id: u32,
}

impl DbConnection {
    fn open(id: u32) -> DbConnection {
        println!("[conn {id}] opening");
        DbConnection { id }
    }
}

impl Drop for DbConnection {
    fn drop(&mut self) {
        println!("[conn {}] closing (Drop ran)", self.id);
    }
}

fn main() {
    println!("-- start of main --");
    let _primary = DbConnection::open(1);

    {
        let _scratch = DbConnection::open(2);
        println!("-- inside inner block --");
    } // `_scratch` dropped HERE — exactly at this `}`

    println!("-- back in main --");
} // `_primary` dropped HERE — at the end of main
```

**Output:**

```
-- start of main --
[conn 1] opening
[conn 2] opening
-- inside inner block --
[conn 2] closing (Drop ran)
-- back in main --
[conn 1] closing (Drop ran)
```

**Key points:**

- `impl Drop for DbConnection` defines a destructor; its `drop(&mut self)` body is the cleanup logic.
- `_scratch` is cleaned up at the end of its inner block, *before* `-- back in main --` prints. Cleanup is scope-bound, not deferred.
- `_primary` is dropped at the end of `main`. You never call `.close()`; the compiler inserts the destructor call.
- This is **RAII**: acquiring the `DbConnection` in `open` *is* the act that schedules its release.

> **Note:** Most types you use (`String`, `Vec<T>`, `Box<T>`, `File`, `MutexGuard`) already implement `Drop` (or contain things that do). You only write your own `impl Drop` when you have a resource that needs custom teardown. Freeing a `String`'s heap buffer happens automatically; you don't implement `Drop` for that.

---

## Detailed Explanation

### The `Drop` trait

`Drop` is a trait from the standard library with a single method:

```rust
// (from std — shown for reference, not to be re-defined)
pub trait Drop {
    fn drop(&mut self);
}
```

When a value that implements `Drop` goes out of scope, the compiler inserts a call to `drop(&mut self)`. A few things are worth nailing down:

- `drop` takes `&mut self`, not `self`. The value is *already being destroyed*; you get a mutable borrow so you can inspect and tear down its fields, but you cannot move the value out of `self` inside `drop`.
- You **cannot call `value.drop()` yourself** — the compiler forbids the explicit destructor call (more on that under Common Pitfalls). To trigger cleanup early, you use the free function `std::mem::drop`, covered below.
- After your `drop` body runs, the compiler then automatically drops each *field* of the value, recursively. So a `String` field's heap buffer is freed for you even though your `drop` body didn't mention it.

### Cleanup runs on every exit path

The compiler inserts the destructor call wherever the owner's scope ends: at a literal closing brace you can see, but equally at early `return`s and even during a panic unwind. Here a value moved *into* a function is dropped inside that function, not back in the caller:

```rust
struct Loud(&'static str);
impl Drop for Loud {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn consume(item: Loud) {
    println!("consume() received {}", item.0);
} // `item` dropped HERE, inside consume

fn main() {
    let x = Loud("x");
    println!("before consume");
    consume(x); // `x` MOVED into consume; it is dropped there, not in main
    println!("after consume (x already dropped)");
}
```

**Output:**

```
before consume
consume() received x
drop x
after consume (x already dropped)
```

Because ownership moved into `consume`, that function's scope now owns the value, so the drop happens at the end of `consume`, before `after consume` prints. (Moves are the subject of [The Three Ownership Rules](/05-ownership/01-ownership-rules/) and [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/); the takeaway here is that *the current owner's scope* decides when the drop fires.)

### Drop order: bindings are LIFO

Within a single scope, local variables are dropped in **reverse order of declaration**: last declared, first dropped, like popping a stack:

```rust
struct Tracer(&'static str);
impl Drop for Tracer {
    fn drop(&mut self) {
        println!("dropping {}", self.0);
    }
}

fn main() {
    let _a = Tracer("a");
    let _b = Tracer("b");
    let _c = Tracer("c");
    println!("all three created");
} // dropped in reverse: c, then b, then a
```

**Output:**

```
all three created
dropping c
dropping b
dropping a
```

This reverse order matters: if `_b` depends on `_a` (say `_a` is a connection and `_b` a transaction on it), declaring `_a` first guarantees the transaction is torn down *before* the connection it relies on.

### Drop order: a value's own `drop`, then its fields — in declaration order

Nested values follow a different rule from local bindings. When a struct is dropped, **its own `drop` runs first**, and then **its fields are dropped in declaration order** (top to bottom):

```rust
struct Noisy(&'static str);
impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

#[allow(dead_code)]
struct Wrapper {
    first: Noisy,
    second: Noisy,
}
impl Drop for Wrapper {
    fn drop(&mut self) {
        println!("drop Wrapper");
    }
}

fn main() {
    let _w = Wrapper {
        first: Noisy("first"),
        second: Noisy("second"),
    };
    println!("created wrapper");
}
```

**Output:**

```
created wrapper
drop Wrapper
drop first
drop second
```

So: the outer value's `drop` body (`drop Wrapper`) runs first, then the fields drop **in declaration order** (`first` before `second`). Note this is *forward* order, the opposite of the LIFO order for separate `let` bindings. Elements of a `Vec<T>` likewise drop in index order when the `Vec` is dropped.

> **Tip:** Don't memorize edge cases. Remember the principle: a composite is torn down *outermost-first* (its own `drop`, then its parts), and *separate local bindings* unwind like a stack (last-in, first-out). Code that relies on subtle drop-order details is usually fragile; prefer explicit ordering with `std::mem::drop` when order truly matters.

### `std::mem::drop`: releasing early

Sometimes scope is too coarse — you want a resource gone *before* the end of the block (release a lock before doing slow work, free a big buffer before a long phase). The standard library provides a free function, in the prelude, that does exactly this:

```rust
// This is essentially all std::mem::drop is:
fn drop<T>(_value: T) {} // takes ownership by value, then its scope ends immediately
```

It takes the value **by value** (a move), so the value is now owned by `drop`'s parameter, whose scope ends instantly, running the destructor. It's already in the prelude, so you write `drop(x)`, not `std::mem::drop(x)`:

```rust
struct Guard(&'static str);
impl Drop for Guard {
    fn drop(&mut self) {
        println!("releasing {}", self.0);
    }
}

fn main() {
    let lock = Guard("mutex");
    println!("got the lock, doing critical work");

    drop(lock); // explicitly release NOW, before the end of scope
    println!("lock released early; doing non-critical work");
} // nothing left to drop here — `lock` is already gone
```

**Output:**

```
got the lock, doing critical work
releasing mutex
lock released early; doing non-critical work
```

The magic is unremarkable: `drop` does nothing in its body. The cleanup happens because the value was *moved* into a scope that ends immediately. After `drop(lock)`, the binding `lock` is no longer usable; using it is a use-after-move error, which is exactly what you want.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| When memory is freed | Non-deterministically, by the GC | Deterministically, at the owner's scope exit |
| Destructor for resources | Manual `close()` / `dispose()`, often in `try/finally` | `Drop::drop`, run automatically by the compiler |
| Runs on early return / throw | Only if you wrote `finally` | Always, including during panic unwind |
| Forgetting cleanup | Leaks the resource (FD, lock) | Impossible to forget; it's tied to scope |
| Finalizers | `FinalizationRegistry`: no timing guarantee, may never run | `Drop`: precise, guaranteed, source-visible |
| Force cleanup now | Call `close()` and null the reference | `std::mem::drop(value)` (a move) |
| Cleanup order | Whatever order you write `close()` calls | Bindings LIFO; a value's `drop` then its fields in declaration order |
| Runtime cost | GC tracing, pauses, allocation headers | Zero — destructor calls are inserted at compile time |

**The core mental shift:** JavaScript's `FinalizationRegistry` looks like a destructor but is the opposite of one: it is a *best-effort, no-guarantee* callback. Rust's `Drop` is a *hard guarantee* fired at a *known point*. RAII replaces the discipline of "remember to call `close()` on every path" with "the type system frees it for you, always."

> **Warning:** Do not reach for `Drop` as a place to run important *application* logic like flushing a network buffer where errors matter: `drop` cannot return a `Result` or fail gracefully, and during a panic it runs mid-unwind. Use it for *resource release* (free, close, unlock). For fallible teardown, expose an explicit method (e.g. `fn close(self) -> io::Result<()>`) and use `Drop` only as a backstop.

---

## Common Pitfalls

### Pitfall 1: Calling `.drop()` directly

Coming from a `dispose()`/`close()` mindset, the natural instinct is to call the destructor by name. Rust forbids it, because that would let the value be destroyed twice (once by you, once by the automatic end-of-scope drop): a double-free.

```rust
struct Guard(&'static str);
impl Drop for Guard {
    fn drop(&mut self) {
        println!("releasing {}", self.0);
    }
}

fn main() {
    let lock = Guard("mutex");
    lock.drop(); // does not compile (error[E0040]): explicit destructor call
}
```

Real compiler output:

```
error[E0040]: explicit use of destructor method
  --> src/main.rs:10:10
   |
10 |     lock.drop(); // does not compile (error[E0040]): explicit destructor call
   |          ^^^^ explicit destructor calls not allowed
   |
help: consider using `drop` function
   |
10 -     lock.drop(); // does not compile (error[E0040]): explicit destructor call
10 +     drop(lock); // does not compile (error[E0040]): explicit destructor call
   |
```

**Fix:** use the free function `drop(lock)` (the compiler even suggests it). It moves the value in and lets its scope end, running the destructor exactly once.

### Pitfall 2: Using a value after `drop(value)`

`drop` takes ownership, so after `drop(x)` the binding `x` is moved-from and can no longer be used. This trips up developers expecting `x` to merely be "cleared" or "nulled" the way `x = null` works in JavaScript.

```rust
struct FileHandle {
    path: String,
}
impl Drop for FileHandle {
    fn drop(&mut self) {
        println!("closing {}", self.path);
    }
}

fn main() {
    let f = FileHandle { path: String::from("/tmp/log.txt") };
    drop(f); // ownership moved into drop(); `f` is now gone
    println!("path was {}", f.path); // does not compile (error[E0382]): use after move
}
```

Real compiler output (trimmed):

```
error[E0382]: borrow of moved value: `f`
  --> src/main.rs:13:29
   |
11 |     let f = FileHandle { path: String::from("/tmp/log.txt") };
   |         - move occurs because `f` has type `FileHandle`, which does not implement the `Copy` trait
12 |     drop(f); // ownership moved into drop(); `f` is now gone
   |          - value moved here
13 |     println!("path was {}", f.path); // does not compile (error[E0382]): use after move
   |                             ^^^^^^ value borrowed here after move
```

**Fix:** read anything you need from the value *before* dropping it, or simply let it drop naturally at the end of scope. There is no valid "use after explicit drop."

### Pitfall 3: Trying to make a `Drop` type `Copy`

A `Copy` type is duplicated bitwise with no notion of a unique owner, but a destructor exists precisely to clean up a *unique* resource. The two are mutually exclusive, and the compiler says so.

```rust
#[derive(Copy, Clone)] // does not compile (error[E0184])
struct Token {
    value: u32,
}

impl Drop for Token {
    fn drop(&mut self) {
        println!("dropping token {}", self.value);
    }
}

fn main() {
    let _t = Token { value: 1 };
}
```

Real compiler output:

```
error[E0184]: the trait `Copy` cannot be implemented for this type; the type has a destructor
 --> src/main.rs:1:10
  |
1 | #[derive(Copy, Clone)]
  |          ^^^^ `Copy` not allowed on types with destructors
```

**Fix:** pick one model. If the type owns a resource that needs cleanup, it should *move* (not be `Copy`); drop the `Copy` derive. If it's a plain value type with no resource, drop the `Drop` impl. (See [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/) for the `Copy` vs move distinction.)

### Pitfall 4: Expecting `Drop` to fire when a `panic!` aborts

During a normal panic, Rust *unwinds* the stack and runs destructors: `Drop` still fires, which is great for releasing locks safely. But if the project is built with `panic = "abort"` (in `Cargo.toml` or via a target that aborts), or if a panic occurs *while already unwinding from another panic*, the process aborts immediately and **destructors do not run**.

**Fix:** don't rely on `Drop` for correctness-critical cleanup that *must* happen even on abort (e.g. nothing should "leak" in a way that matters across process death). For in-process resources (locks, memory), the OS reclaims everything on process exit anyway. Just be aware that "drop always runs" assumes unwinding, not aborting.

---

## Best Practices

- **Let scope drive cleanup; reach for `drop()` only to release *early*.** The whole point of RAII is that you don't manage lifetimes by hand. Restructure code (an inner `{ }` block, a helper function) so a value's scope matches its useful lifetime, and only call `std::mem::drop` when you genuinely need a resource freed before the natural end of scope.

- **Keep `drop` bodies short, infallible, and side-effect-light.** A destructor cannot return errors and may run during unwinding. Release the resource; don't run business logic. If teardown can fail, expose an explicit `fn close(self) -> Result<...>` and treat `Drop` as the safety net.

- **Use a guard with a "done" flag for commit/rollback patterns.** A value that auto-rolls-back on drop unless a `commit`/`finish` method flips a flag is the idiomatic Rust equivalent of `try/finally` for transactional resources (shown in the Real-World Example).

- **Don't implement `Drop` just to free memory.** `String`, `Vec<T>`, `Box<T>`, and friends already free their heap allocations. Write `impl Drop` only for *external* resources or *observable* teardown (closing a handle, logging a span end).

- **Mind the difference between binding order and field order.** If teardown order is load-bearing, make it explicit (separate `drop()` calls, or deliberate field/declaration ordering) rather than relying on a reader to recall the LIFO-vs-declaration-order rules.

---

## Real-World Example

A classic production use of RAII: a **transaction guard** that automatically rolls back if it is dropped without being explicitly committed. This makes "every early-return path rolls back" a *compile-time guarantee* rather than something a reviewer has to verify by reading every branch — the JavaScript `try/finally` equivalent that you can never forget.

```rust
/// An RAII transaction guard. If it is dropped without `commit()`, it rolls back.
struct Transaction {
    name: String,
    committed: bool,
}

impl Transaction {
    fn begin(name: &str) -> Transaction {
        println!("BEGIN {name}");
        Transaction { name: name.to_string(), committed: false }
    }

    /// Consume the guard on success. Takes `self` by value so it can't be reused.
    fn commit(mut self) {
        println!("COMMIT {}", self.name);
        self.committed = true;
        // `self` is dropped at the end of commit(); the flag suppresses the rollback.
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.committed {
            println!("ROLLBACK {} (guard cleanup)", self.name);
        }
    }
}

fn transfer(ok: bool) {
    let tx = Transaction::begin("transfer-funds");
    println!("  ... debiting account A");
    if !ok {
        println!("  ... validation failed, returning early");
        return; // `tx` dropped here -> automatic ROLLBACK, no manual cleanup needed
    }
    println!("  ... crediting account B");
    tx.commit(); // explicit success -> COMMIT, and the rollback is suppressed
}

fn main() {
    println!("== happy path ==");
    transfer(true);
    println!("== error path ==");
    transfer(false);
}
```

**Output:**

```
== happy path ==
BEGIN transfer-funds
  ... debiting account A
  ... crediting account B
COMMIT transfer-funds
== error path ==
BEGIN transfer-funds
  ... debiting account A
  ... validation failed, returning early
ROLLBACK transfer-funds (guard cleanup)
```

**Why this is idiomatic:**

- The early `return` in `transfer(false)` triggers the rollback *automatically*: the compiler inserts the drop on that exit path. No `try/finally`, no rollback call to forget.
- `commit(self)` takes ownership, so a committed transaction can't be used again, and the `committed` flag stops `Drop` from also rolling back.
- The same pattern underlies the standard library's own RAII guards: `std::sync::MutexGuard` releases the lock on drop, `std::fs::File` closes the descriptor on drop, and `Box<T>` frees its heap allocation on drop. You're using RAII constantly even when you never write `impl Drop` yourself.
- This is *deterministic* and *local*: you can read the source and know exactly when the rollback fires. In JavaScript the equivalent safety requires a correctly-written `finally` on every path, plus the resource itself surviving long enough, neither of which the type system enforces.

---

## Further Reading

### Official Documentation

- [The Rust Book — Running Code on Cleanup with the `Drop` Trait](https://doc.rust-lang.org/book/ch15-03-drop.html) — the canonical introduction, including why you can't call `drop` directly.
- [`std::ops::Drop`](https://doc.rust-lang.org/std/ops/trait.Drop.html) — the trait reference, with the rules on field drop order.
- [`std::mem::drop`](https://doc.rust-lang.org/std/mem/fn.drop.html) — the free function for early, deterministic release.
- [The Rustonomicon — Destructors](https://doc.rust-lang.org/nomicon/destructors.html) — the precise drop-order semantics for composite types.
- [MDN — `FinalizationRegistry`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/FinalizationRegistry) — note its explicit "no guarantee the callback runs" caveat, the contrast with Rust's `Drop`.

### Related Sections in This Guide

- [The Three Ownership Rules](/05-ownership/01-ownership-rules/) — Rule 3 (drop at end of scope) is what `Drop` hooks into.
- [Stack vs Heap](/05-ownership/00-stack-heap/) — *what* gets freed when a value is dropped.
- [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/) — why a `Drop` type can't be `Copy`, and what a move does.
- [Borrowing](/05-ownership/02-borrowing/) — references don't drop the value they point to; only the owner does.
- [Reference Counting (`Rc`/`Arc`)](/05-ownership/07-reference-counting/) — with shared ownership, the value drops when the *last* owner does.
- [Variables and Mutability](/02-basics/00-variables/) — scopes and shadowing, the foundation for drop timing.
- [Output and Formatting](/02-basics/04-output/) — the `println!` used throughout these examples.
- [Data Structures](/06-data-structures/) — how `Drop` composes through struct and enum fields.

---

## Exercises

### Exercise 1

**Difficulty:** Easy

**Objective:** Implement `Drop` and observe deterministic, reverse-order cleanup.

**Instructions:** Define a `TempFile` struct with a `name: String` field and implement `Drop` so that dropping prints `deleting temp file <name>`. In `main`, create two `TempFile`s (`"a.tmp"` then `"b.tmp"`), print `working with temp files`, and let them drop at the end of `main`. Before running, predict the order the two deletion messages print in.

<details>
<summary>Solution</summary>

```rust
struct TempFile {
    name: String,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        println!("deleting temp file {}", self.name);
    }
}

fn main() {
    let _a = TempFile { name: String::from("a.tmp") };
    let _b = TempFile { name: String::from("b.tmp") };
    println!("working with temp files");
} // dropped in reverse declaration order: b.tmp first, then a.tmp
```

Output:

```
working with temp files
deleting temp file b.tmp
deleting temp file a.tmp
```

Local bindings drop in **reverse** order of declaration (LIFO), so `b.tmp` — declared last — is deleted first. The leading underscores (`_a`, `_b`) keep the compiler from warning that the bindings are unused; they exist purely for their drop side effect.

</details>

### Exercise 2

**Difficulty:** Medium

**Objective:** Use `std::mem::drop` to release a resource *before* the end of its scope.

**Instructions:** Define a `Buffer` struct with a `label: &'static str` field whose `Drop` prints `freeing buffer '<label>'`. In `main`, create a `Buffer` labelled `"scratch"`, print `phase 1: using scratch buffer`, then free the buffer *immediately* (do not wait for the end of `main`), and finally print `phase 2: long-running work without the buffer`. The freeing message must appear between the two phase messages.

<details>
<summary>Solution</summary>

```rust
struct Buffer {
    label: &'static str,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        println!("freeing buffer '{}'", self.label);
    }
}

fn main() {
    let scratch = Buffer { label: "scratch" };
    println!("phase 1: using scratch buffer");
    drop(scratch); // free it NOW, before the long phase 2
    println!("phase 2: long-running work without the buffer");
}
```

Output:

```
phase 1: using scratch buffer
freeing buffer 'scratch'
phase 2: long-running work without the buffer
```

`drop(scratch)` moves the buffer into the prelude's `drop` function, whose scope ends instantly, running the destructor right there. After this line, `scratch` is moved-from and can't be used again, which is exactly the guarantee you want when releasing a resource early.

</details>

### Exercise 3

**Difficulty:** Medium/Hard

**Objective:** Build an RAII guard that performs cleanup on drop *unless* it was explicitly finished — the commit/rollback pattern.

**Instructions:** Define a `Span` struct with a `name: String` and a `finished: bool`. Add `Span::start(name: &str)` that prints `-> entering <name>` and returns a `Span` with `finished: false`. Add a method `finish(self)` (taking `self` by value) that prints `<- <name> completed normally` and sets `finished = true`. Implement `Drop` so that, *only if `finished` is false*, it prints `!! <name> aborted (cleanup on drop)`. Then write `fn run(fail: bool)` that starts a span named `"request"`, returns early (without finishing) when `fail` is true, and otherwise calls `finish()`. Call `run(false)` then `run(true)` in `main` and predict the output.

<details>
<summary>Solution</summary>

```rust
struct Span {
    name: String,
    finished: bool,
}

impl Span {
    fn start(name: &str) -> Span {
        println!("-> entering {name}");
        Span { name: name.to_string(), finished: false }
    }

    fn finish(mut self) {
        println!("<- {} completed normally", self.name);
        self.finished = true;
        // `self` is dropped at the end of finish(); the flag suppresses the abort message.
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if !self.finished {
            println!("!! {} aborted (cleanup on drop)", self.name);
        }
    }
}

fn run(fail: bool) {
    let span = Span::start("request");
    if fail {
        return; // dropped without finish -> abort message fires
    }
    span.finish();
}

fn main() {
    run(false);
    run(true);
}
```

Output:

```
-> entering request
<- request completed normally
-> entering request
!! request aborted (cleanup on drop)
```

In the success path, `finish()` consumes the span and sets `finished = true`, so the `Drop` body sees the flag and stays quiet. In the failure path, the early `return` drops the span while `finished` is still `false`, so the abort message fires automatically: no `try/finally`, and impossible to forget. This is exactly how production transaction guards, tracing spans, and lock guards work.

</details>
