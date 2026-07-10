---
title: "RAII and Drop Guards"
description: "Rust's Drop trait runs cleanup automatically at scope exit, replacing TypeScript's try/finally and using with scope guards, lock guards, and defer."
---

In Rust, cleanup is not something you remember to do — it is something the compiler does for you. When a value goes out of scope, Rust runs its destructor automatically, and that single mechanism powers closing files, unlocking mutexes, returning pooled connections, and rolling back transactions. This is **RAII** (Resource Acquisition Is Initialization), and once it clicks, a whole category of "I forgot to release that" bugs simply disappears.

---

## Quick Overview

RAII means that **owning a value is owning a resource, and the resource is released exactly when the value is destroyed**. You tie a resource (a lock, a file descriptor, a network connection) to the lifetime of a Rust value; when that value drops at the end of its scope, the `Drop` trait runs and frees the resource. A value used purely for its `Drop` side effect is called a **scope guard** (or **drop guard**).

For a TypeScript/JavaScript developer this replaces three different habits at once: the `try { ... } finally { release() }` block, the `using` declaration (TC39 explicit resource management), and the manual "remember to call `.close()`" discipline. Rust folds all of them into one deterministic rule, *drop at end of scope*, that the borrow checker enforces so thoroughly that forgetting is not an option.

> **Note:** This page covers RAII and scope guards specifically. Several sibling patterns lean on `Drop` too — the [type-state pattern](/22-common-patterns/02-type-state/) and the [decorator pattern](/22-common-patterns/06-decorator-pattern/) both produce wrapper types — but here the wrapper exists *for its destructor*. For the underlying ownership and scope rules that make all of this work, see [Section 05: Ownership](/05-ownership/).

---

## TypeScript/JavaScript Example

A connection that must be released no matter how the function exits. In TypeScript the only portable guarantee is a `try/finally`, and the burden of writing it correctly is entirely on the author.

```typescript
// TypeScript - manual cleanup with try/finally
interface Connection {
  id: number;
  query(sql: string): void;
  close(): void;
}

function openConnection(id: number): Connection {
  console.log(`opening connection ${id}`);
  return {
    id,
    query(sql) {
      console.log(`conn ${id}: ${sql}`);
    },
    close() {
      console.log(`closing connection ${id}`);
    },
  };
}

function runReport(): void {
  const conn = openConnection(42);
  try {
    conn.query("SELECT * FROM events");
    // ... if this throws, or we `return` early, or we forget the finally,
    //     the connection leaks unless `close()` is guaranteed to run.
  } finally {
    conn.close();
  }
}

runReport();
```

Modern JavaScript improves on this with the `using` declaration (the TC39 *explicit resource management* proposal, available in TypeScript 5.2+; the `using` keyword needs that compiler or a runtime that supports it, though the underlying `[Symbol.dispose]()` method is a plain method):

```typescript
// TypeScript 5.2+ - `using` calls [Symbol.dispose] at end of block
function openDisposable(id: number) {
  console.log(`opening connection ${id}`);
  return {
    id,
    query(sql: string) {
      console.log(`conn ${id}: ${sql}`);
    },
    [Symbol.dispose]() {
      console.log(`closing connection ${id}`);
    },
  };
}

function runReport(): void {
  using conn = openDisposable(42);
  conn.query("SELECT * FROM events");
} // [Symbol.dispose]() runs here, even on an early return or throw
```

This is genuinely close to Rust's model, but it is **opt-in per variable** (you must write `using`), it is **recent** (not available everywhere), and nothing forces a resource type to be disposable or stops you from holding a disposable past its safe window. Rust makes the same guarantee the *default* for every value, and the type system polices the rest.

---

## Rust Equivalent

In Rust you implement the **`Drop`** trait. There is no `close()` to call and no `finally` to write: the destructor runs automatically when `conn` leaves scope.

```rust playground
struct Connection {
    id: u32,
}

impl Connection {
    fn open(id: u32) -> Self {
        println!("opening connection {id}");
        Connection { id }
    }
    fn query(&self, sql: &str) {
        println!("conn {}: {sql}", self.id);
    }
}

// The destructor. Rust calls this automatically when a `Connection` is dropped.
impl Drop for Connection {
    fn drop(&mut self) {
        println!("closing connection {}", self.id);
    }
}

fn run_report() {
    let conn = Connection::open(42);
    conn.query("SELECT * FROM events");
    // No `close()`, no `finally`: `conn` drops here and the destructor runs,
    // even on an early `return` or a panic.
}

fn main() {
    run_report();
}
```

Running this prints:

```text
opening connection 42
conn 42: SELECT * FROM events
closing connection 42
```

The cleanup is *structural*. You cannot forget it because there is nothing to remember, and you cannot accidentally run it twice because ownership guarantees a value is dropped exactly once.

### Standard-library guards you already use

You have been relying on RAII guards since your first Rust program, perhaps without naming them:

- `Box<T>`, `Vec<T>`, `String`: their `Drop` frees heap memory.
- `File`: its `Drop` closes the file descriptor (no `file.close()` exists).
- `MutexGuard` returned by `Mutex::lock()`: its `Drop` releases the lock.
- `RwLockReadGuard` / `RwLockWriteGuard`, `Ref` / `RefMut` from `RefCell`.

Here is the lock guard in action, the canonical "release a lock" case:

```rust playground
use std::sync::Mutex;

fn main() {
    let counter = Mutex::new(0);

    {
        // lock() returns a MutexGuard — an RAII guard. The lock is held
        // for exactly as long as the guard is alive.
        let mut guard = counter.lock().unwrap();
        *guard += 1;
        println!("counter while locked: {}", *guard);
    } // guard dropped here -> mutex unlocked automatically

    // We can lock again because the previous guard was already dropped.
    let guard = counter.lock().unwrap();
    println!("counter after scope: {}", *guard);
}
```

Output:

```text
counter while locked: 1
counter after scope: 1
```

There is no `unlock()` method on `Mutex` *at all* — releasing the lock is not an operation you perform, it is a consequence of the guard dropping. That is RAII in its purest form.

---

## Detailed Explanation

### The `Drop` trait

`Drop` has exactly one method:

```rust
trait Drop {
    fn drop(&mut self);
}
```

A few rules make it behave the way it does, and each one differs from a TypeScript finalizer:

- **It runs deterministically, at scope exit** — not "eventually" like a JavaScript `FinalizationRegistry` callback, which is tied to garbage collection and may never fire. Rust knows statically where every value dies.
- **It takes `&mut self`, never `self`.** Your destructor borrows the value; it cannot move fields out of it (the value is about to be destroyed, so moving out would leave a half-valid thing for the *automatic* field drops that follow). If you need to consume a field, wrap it in `Option` and `.take()` it (we use this trick in the connection-pool example below).
- **You never call `drop()` yourself.** Calling `value.drop()` is a compile error (more on that in Pitfalls). To destroy a value *early*, call the free function `std::mem::drop(value)`, which simply takes the value by value and lets it fall out of scope inside the function.
- **After your `drop` body runs, Rust automatically drops each field**, recursively. You only write cleanup for the resource *this* type owns directly.

### Drop order is LIFO

Within a scope, values are dropped in **reverse order of declaration** — last declared, first dropped — which mirrors how nested resources should unwind. Nested blocks drop their values at the inner `}`.

```rust playground
struct Guard(&'static str);

impl Drop for Guard {
    fn drop(&mut self) {
        println!("dropping {}", self.0);
    }
}

fn main() {
    let _a = Guard("a (first declared)");
    let _b = Guard("b (second declared)");

    {
        let _inner = Guard("inner (nested scope)");
        println!("inside nested scope");
    } // _inner dropped here

    let early = Guard("early");
    drop(early); // std::mem::drop runs Drop now, not at end of main
    println!("after explicit drop(early)");

    println!("end of main reached");
    // _b drops, then _a — reverse declaration order (LIFO)
}
```

Output:

```text
inside nested scope
dropping inner (nested scope)
dropping early
after explicit drop(early)
end of main reached
dropping b (second declared)
dropping a (first declared)
```

Notice three things: the nested `_inner` drops at its closing brace; `drop(early)` runs the destructor *immediately* (not at the end of `main`); and the two outer guards drop in reverse declaration order.

### Drop runs during a panic, too

This is the property that makes RAII trustworthy. When a thread panics, Rust **unwinds** the stack, dropping every value along the way — exactly like a `finally` block, but for *every* value, automatically.

```rust playground
struct Connection {
    id: u32,
}
impl Drop for Connection {
    fn drop(&mut self) {
        println!("closing connection {}", self.id);
    }
}

fn risky() {
    let _conn = Connection { id: 42 };
    println!("connection open, about to panic");
    panic!("boom");
    // unreachable, but _conn STILL gets dropped during unwinding
}

fn main() {
    let result = std::panic::catch_unwind(|| {
        risky();
    });
    println!("caught panic? {}", result.is_err());
}
```

Output:

```text
connection open, about to panic

thread 'main' panicked at src/main.rs:13:5:
boom
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
closing connection 42
caught panic? true
```

The connection closes even though the function never reached its end. (The one exception: if a panic happens *during* unwinding, or you compile with `panic = "abort"`, destructors are skipped because the process is going down regardless.)

### A scope guard: the `defer` pattern

Go has `defer`, Swift has `defer`, and TypeScript has `try/finally`. Rust does not need a `defer` keyword because *any* type with a `Drop` impl is a deferral mechanism. To defer arbitrary code, wrap a closure in a guard:

```rust playground
// A hand-rolled "defer" via a generic scope guard holding a closure.
struct ScopeGuard<F: FnMut()> {
    cleanup: F,
}

impl<F: FnMut()> Drop for ScopeGuard<F> {
    fn drop(&mut self) {
        (self.cleanup)();
    }
}

// `defer`-style helper: run the closure when the returned guard drops.
fn defer<F: FnMut()>(cleanup: F) -> ScopeGuard<F> {
    ScopeGuard { cleanup }
}

fn process() {
    println!("acquiring temp resource");
    let _cleanup = defer(|| println!("releasing temp resource (defer)"));

    println!("doing work...");
    // Even if we returned early or panicked here, _cleanup still runs.
}

fn main() {
    process();
    println!("back in main");
}
```

Output:

```text
acquiring temp resource
doing work...
releasing temp resource (defer)
back in main
```

The closure runs when `_cleanup` drops at the end of `process` — including on an early return or panic. The binding name starts with an underscore so the compiler does not warn about it being unused; the name still matters, as the next pitfall shows.

---

## Key Differences

| Concern | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| When cleanup runs | `finally` block, or `using` + `[Symbol.dispose]()` at block end; GC finalizers are non-deterministic | Deterministically at scope exit, via `Drop`, for **every** value |
| Who writes it | The caller, every time (`try/finally`), or opt in with `using` | The type author once, in `impl Drop`; callers get it for free |
| Can you forget it | Yes — a missing `finally` or `using` silently leaks | No — the compiler always drops owned values |
| Runs on early return | Only if inside `finally` / a `using` block | Always |
| Runs on exception / panic | `finally` does; `using` does | Yes, during unwinding |
| Run cleanup early | Just call `dispose()` / `close()` yourself | `std::mem::drop(value)` |
| Cancel the cleanup | Set a flag and branch inside `finally` | Defuse the guard (e.g. `ScopeGuard::into_inner`, or an internal flag) |
| Double cleanup | Possible (call `close()` twice) | Impossible: a value drops exactly once |

The headline difference is **ownership-driven**. JavaScript ties cleanup to syntax (`finally`) or an opt-in keyword (`using`); Rust ties it to *value lifetime*. Because the borrow checker already tracks where every value lives and dies, attaching cleanup to that lifetime is free and unforgettable.

> **Tip:** The mental model "the guard *is* the resource" pays off. A `MutexGuard` is not a handle to a held lock — it *is* the held lock. Dropping it is releasing it. Keeping it alive longer (storing it in a struct, returning it) holds the lock longer, and the compiler reasons about that for you.

---

## Common Pitfalls

### `let _ = guard()` drops the guard immediately

This is the single most common RAII bug, and it does **not** produce a compiler error — only wrong behavior. A bare wildcard pattern `_` binds *nothing*, so the value is a temporary that drops at the end of the statement. A named binding (even `_named`) lives to the end of the scope.

```rust playground
struct Guard(&'static str);
impl Drop for Guard {
    fn drop(&mut self) { println!("dropping {}", self.0); }
}

fn make(name: &'static str) -> Guard {
    println!("creating {name}");
    Guard(name)
}

fn main() {
    // `let _ = ...` drops the value IMMEDIATELY (the wildcard binds nothing).
    let _ = make("A (let _)");
    println!("  ... work after `let _`");

    // `let _named = ...` keeps it alive until end of scope.
    let _keep = make("B (let _keep)");
    println!("  ... work after `let _keep`");

    // Bare `make(..)` as a statement is also dropped immediately (temporary).
    make("C (statement)");
    println!("  ... work after bare statement");

    println!("end of main");
}
```

Output:

```text
creating A (let _)
dropping A (let _)
  ... work after `let _`
creating B (let _keep)
  ... work after `let _keep`
creating C (statement)
dropping C (statement)
  ... work after bare statement
end of main
dropping B (let _keep)
```

`A` drops *before* the work that was supposed to be protected. If `Guard` were a `MutexGuard`, you would have released the lock before touching the data it guards — a real data-race-shaped bug with no compiler complaint. **Always bind a guard to a real name** (`let _guard = ...`), never to `_`.

### Trying to call `drop()` from inside `Drop`

You cannot invoke a destructor by hand, and you especially cannot call it from within itself. This *is* a compile error:

```rust
struct Thing;
impl Drop for Thing {
    fn drop(&mut self) {
        // does not compile (error[E0040]: explicit use of destructor method)
        self.drop();
    }
}
fn main() {
    let _t = Thing;
}
```

The real message from `rustc`:

```text
error[E0040]: explicit use of destructor method
 --> src/main.rs:5:14
  |
5 |         self.drop();
  |              ^^^^ explicit destructor calls not allowed
  |
help: consider using `drop` function
  |
5 -         self.drop();
5 +         drop(self);
  |
```

To destroy a value early, use the free function `std::mem::drop(value)` from *outside* the destructor. Inside a `Drop` impl you should never need to: Rust drops the fields for you after your body returns.

### Holding a `MutexGuard` longer than you meant to

Because the lock lives as long as the guard, an over-long binding holds the lock over unrelated work — or, worse, locks the same mutex twice on one thread and **deadlocks**:

```rust
// logic bug (deadlock, not a compile error)
// let g1 = m.lock().unwrap();
// let g2 = m.lock().unwrap(); // g1 still alive -> this blocks forever
```

This compiles fine and then hangs. The fix is to *scope the guard*: put the first lock in its own `{ ... }` block so it drops before the second lock, or call `drop(g1)` before re-locking. The same trap appears with `RefCell`'s `borrow()`/`borrow_mut()`, except there a second borrow panics at run time instead of blocking.

> **Warning:** In async code the rule is sharper still: never hold a `std::sync::MutexGuard` across an `.await`. The guard is not `Send`, so the future cannot move between worker threads, and you risk deadlocking the runtime. Use `tokio::sync::Mutex` (whose guard *is* held across awaits) or release the `std::sync` guard before awaiting. See [Section 11: Async](/11-async/).

### Putting important side effects only in `Drop`

`Drop` is for *releasing*, not for *doing the main work*. Flushing a buffered writer is a classic example: `Drop` will flush, but it cannot return a `Result`, so a write error during drop is silently swallowed (or, in some std types, turns into a panic). For operations that can fail in a way the caller must see, expose an explicit `commit()` / `flush()` / `finish()` method that returns `Result`, and treat the `Drop` impl as a best-effort fallback.

---

## Best Practices

- **Bind every guard to a named variable** (`let _guard = ...`), never to `_`. Reserve `let _ =` for values you genuinely want dropped right now.
- **Keep the guard's scope as tight as the resource needs.** Open a `{ }` block to bound a lock or borrow; this both releases earlier and documents intent.
- **Wrap consumable fields in `Option`** so `Drop` can `.take()` them. `Drop` only gets `&mut self`, so this is the standard way to move a value out during cleanup.
- **For fallible cleanup, provide an explicit method** that returns `Result` (e.g. `commit`, `close`, `finish`) and let `Drop` be the safety net for the path where the caller forgot.
- **Reach for the `scopeguard` crate** instead of hand-rolling closure guards. It gives you `defer!`, `guard(value, on_drop)`, and the ability to *cancel* a guard, all battle-tested. Add it with `cargo add scopeguard` (current version `1.2`).
- **Do not abuse `Drop` for control flow.** It cannot be `async`, cannot return a value, and cannot reliably propagate errors. It is for resource release.

### The `scopeguard` crate

`scopeguard` packages the patterns above. `defer!` runs a block at scope end; `guard(value, closure)` attaches cleanup to a value and derefs to it transparently:

```rust playground
use scopeguard::{defer, guard};

fn main() {
    // 1. `defer!` — run a block at end of scope, no value attached.
    defer! {
        println!("3. deferred cleanup runs last");
    }
    println!("1. start");

    // 2. `guard(value, closure)` — wrap a value; closure gets it on drop.
    let mut file = guard(Vec::new(), |buf| {
        println!("flushing {} bytes on drop", buf.len());
    });
    file.push(b'h');
    file.push(b'i');
    println!("2. wrote {} bytes", file.len());

    // ScopeGuard derefs to the wrapped value, so `.len()` / `.push()` just work.
}
```

Output:

```text
1. start
2. wrote 2 bytes
flushing 2 bytes on drop
3. deferred cleanup runs last
```

The most useful feature is **defusing** a guard with `ScopeGuard::into_inner`, which recovers the wrapped value *and cancels* the cleanup. It is the foundation of the commit/rollback pattern below.

---

## Real-World Example

A **connection pool** is RAII at its most idiomatic. Connections are checked out as a `Lease` guard; while the lease is alive it owns a connection; when the lease drops, the connection returns to the pool automatically. The caller cannot leak a connection because there is no way to hold one *except* through a lease, and the lease cleans up on drop.

```rust playground
use std::cell::RefCell;
use std::rc::Rc;

// A tiny connection pool. Connections are checked out as a `Lease`,
// and the RAII `Lease` guard returns them automatically on drop.
struct Pool {
    idle: RefCell<Vec<u32>>, // connection ids waiting to be used
}

impl Pool {
    fn new(conns: impl IntoIterator<Item = u32>) -> Rc<Self> {
        Rc::new(Pool { idle: RefCell::new(conns.into_iter().collect()) })
    }

    fn acquire(self: &Rc<Self>) -> Option<Lease> {
        let id = self.idle.borrow_mut().pop()?;
        println!("checked out connection {id}");
        Some(Lease { pool: Rc::clone(self), conn: Some(id) })
    }

    fn idle_count(&self) -> usize {
        self.idle.borrow().len()
    }
}

// The RAII guard. While alive, it owns a connection; on drop it returns it.
struct Lease {
    pool: Rc<Pool>,
    conn: Option<u32>, // Option so Drop can `.take()` the id out
}

impl Lease {
    fn id(&self) -> u32 {
        self.conn.expect("lease always holds a connection until dropped")
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(id) = self.conn.take() {
            println!("returning connection {id} to pool");
            self.pool.idle.borrow_mut().push(id);
        }
    }
}

fn main() {
    let pool = Pool::new([1, 2]);
    println!("idle at start: {}", pool.idle_count());

    {
        let a = pool.acquire().unwrap();
        let b = pool.acquire().unwrap();
        println!("using {} and {}; idle now: {}", a.id(), b.id(), pool.idle_count());
        assert!(pool.acquire().is_none(), "pool is exhausted");
    } // a and b drop here -> both connections returned

    println!("idle after scope: {}", pool.idle_count());
    let c = pool.acquire().unwrap();
    println!("reacquired connection {}", c.id());
}
```

Output:

```text
idle at start: 2
checked out connection 2
checked out connection 1
using 2 and 1; idle now: 0
returning connection 1 to pool
returning connection 2 to pool
idle after scope: 2
checked out connection 2
reacquired connection 2
returning connection 2 to pool
```

This is precisely how real pools such as `r2d2` and `bb8` work: their `PooledConnection` is a `Deref` guard whose `Drop` returns the connection. Note the `Option<u32>` field: `Drop` only sees `&mut self`, so `self.conn.take()` is how we move the id out during cleanup. (For pooling backed by an actual database, see [Section 17: Database](/17-database/); for the broader pooling crates, see the [ecosystem overview](/23-ecosystem/).)

### Commit-or-rollback with a defusable guard

The other production staple is a transaction that **rolls back by default** and commits only on the explicit success path. `scopeguard`'s `into_inner` defuses the rollback when we are ready to commit:

```rust playground
use scopeguard::{guard, ScopeGuard};

struct Transaction { id: u32 }
impl Transaction {
    fn begin(id: u32) -> Self { println!("BEGIN tx {id}"); Transaction { id } }
    fn execute(&self, sql: &str) -> Result<(), String> {
        println!("tx {}: {sql}", self.id);
        Ok(())
    }
    fn commit(self) { println!("COMMIT tx {}", self.id); }
    fn rollback(&mut self) { println!("ROLLBACK tx {}", self.id); }
}

fn transfer(commit_ok: bool) -> Result<(), String> {
    let tx = Transaction::begin(7);

    // Default behavior: if we leave this scope without committing, roll back.
    let tx = guard(tx, |mut t| t.rollback());

    tx.execute("UPDATE accounts SET balance = balance - 100 WHERE id = 1")?;
    tx.execute("UPDATE accounts SET balance = balance + 100 WHERE id = 2")?;

    if !commit_ok {
        return Err("validation failed".into()); // guard fires -> ROLLBACK
    }

    // Success path: defuse the guard, recover the inner Transaction, commit it.
    let tx = ScopeGuard::into_inner(tx); // cancels the rollback closure
    tx.commit();
    Ok(())
}

fn main() {
    println!("--- happy path ---");
    let _ = transfer(true);
    println!("--- failure path ---");
    let r = transfer(false);
    println!("result: {r:?}");
}
```

Output:

```text
--- happy path ---
BEGIN tx 7
tx 7: UPDATE accounts SET balance = balance - 100 WHERE id = 1
tx 7: UPDATE accounts SET balance = balance + 100 WHERE id = 2
COMMIT tx 7
--- failure path ---
BEGIN tx 7
tx 7: UPDATE accounts SET balance = balance - 100 WHERE id = 1
tx 7: UPDATE accounts SET balance = balance + 100 WHERE id = 2
ROLLBACK tx 7
result: Err("validation failed")
```

The `?` operator on the failure path returns early, and the rollback guard fires automatically as it unwinds the scope. On the success path, `into_inner` cancels the rollback and hands back the owned `Transaction` so we can `commit` it. The "default to safe, opt into commit" structure makes a forgotten rollback impossible.

---

## Further Reading

- [The Rust Programming Language — Running Code on Cleanup with the `Drop` Trait](https://doc.rust-lang.org/book/ch15-03-drop.html)
- [`std::ops::Drop` — standard library docs](https://doc.rust-lang.org/std/ops/trait.Drop.html)
- [`std::mem::drop` — dropping a value early](https://doc.rust-lang.org/std/mem/fn.drop.html)
- [The Rustonomicon — Drop and destructors](https://doc.rust-lang.org/nomicon/destructors.html)
- [`scopeguard` crate documentation](https://docs.rs/scopeguard/)
- Related guide sections: [Section 05: Ownership](/05-ownership/) · [Section 11: Async](/11-async/) (guards across `.await`) · [Section 17: Database](/17-database/) (pooled connections) · [Section 23: Ecosystem](/23-ecosystem/)
- Sibling patterns: [The Type-State Pattern](/22-common-patterns/02-type-state/) · [The Decorator Pattern in Rust](/22-common-patterns/06-decorator-pattern/) · [The Newtype Pattern](/22-common-patterns/01-newtype/) · [The Error-Propagation Pattern](/22-common-patterns/03-error-propagation/)

---

## Exercises

### Exercise 1 — A timing guard

**Difficulty:** Beginner

**Objective:** Use `Drop` to measure how long a scope takes, with no manual stop call.

**Instructions:** Write a `Timer` struct that captures a `std::time::Instant` and a `&'static str` label when constructed via `Timer::new(label)`. Implement `Drop` so it prints `[label] took <duration>` using the elapsed time. Create a `do_work` function that builds a `Timer` and then does a busy loop summing `0..1_000_000`. Confirm the timing line prints automatically when `do_work` returns.

<details>
<summary>Solution</summary>

```rust playground
use std::time::Instant;

struct Timer {
    label: &'static str,
    start: Instant,
}

impl Timer {
    fn new(label: &'static str) -> Self {
        Timer { label, start: Instant::now() }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        println!("[{}] took {:?}", self.label, elapsed);
    }
}

fn do_work() {
    let _timer = Timer::new("do_work");
    let mut sum: u64 = 0;
    for i in 0..1_000_000u64 {
        sum = sum.wrapping_add(i);
    }
    println!("sum = {sum}");
}

fn main() {
    do_work();
}
```

Output (the duration varies per run):

```text
sum = 499999500000
[do_work] took 312.5µs
```

The `_timer` binding has a name (not `_`), so it lives until the end of `do_work` and reports on the way out.

</details>

### Exercise 2 — A balanced indentation guard

**Difficulty:** Intermediate

**Objective:** Use an RAII guard to keep paired state (here, log indentation) always balanced, even if you forget to decrement.

**Instructions:** Build a `Logger` holding an `Rc<Cell<usize>>` depth. `log(&self, msg)` prints the message indented by two spaces per depth level. Add `indent(&self) -> Indent` that increments the depth and returns an `Indent` guard whose `Drop` decrements it. Use nested `{ }` blocks so the indentation rises and falls automatically, and confirm the output is perfectly balanced.

<details>
<summary>Solution</summary>

```rust playground
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
struct Logger {
    depth: Rc<Cell<usize>>,
}

impl Logger {
    fn new() -> Self {
        Logger { depth: Rc::new(Cell::new(0)) }
    }

    fn log(&self, msg: &str) {
        let pad = "  ".repeat(self.depth.get());
        println!("{pad}{msg}");
    }

    // Returns an RAII guard: indentation is restored when it drops.
    fn indent(&self) -> Indent {
        self.depth.set(self.depth.get() + 1);
        Indent { depth: Rc::clone(&self.depth) }
    }
}

struct Indent {
    depth: Rc<Cell<usize>>,
}

impl Drop for Indent {
    fn drop(&mut self) {
        self.depth.set(self.depth.get() - 1);
    }
}

fn main() {
    let log = Logger::new();
    log.log("start request");
    {
        let _g = log.indent();
        log.log("validate input");
        {
            let _g = log.indent();
            log.log("check auth token");
        }
        log.log("run handler");
    }
    log.log("send response");
}
```

Output:

```text
start request
  validate input
    check auth token
  run handler
send response
```

Because the decrement lives in `Drop`, the indentation is impossible to leave unbalanced: there is no `dedent()` to forget.

</details>

### Exercise 3 — Your own `defer!` macro

**Difficulty:** Advanced

**Objective:** Reproduce Go's `defer` using a one-shot closure guard and a small declarative macro, and observe LIFO ordering.

**Instructions:** Define a `Defer<F: FnOnce()>(Option<F>)` whose `Drop` calls the closure via `.take()` (so the `FnOnce` can be invoked by value). Write a `macro_rules! defer { ($($body:tt)*) => { ... } }` that expands to a `let`-bound `Defer` holding a closure of the body. Then call `defer!` twice in `main` and verify the two cleanups run in reverse (LIFO) order. (The `scopeguard` crate's `defer!` works the same way — this exercise is about understanding it.)

<details>
<summary>Solution</summary>

```rust playground
struct Defer<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Drop for Defer<F> {
    fn drop(&mut self) {
        // take() turns the FnOnce into something we can call by value.
        if let Some(f) = self.0.take() {
            f();
        }
    }
}

macro_rules! defer {
    ($($body:tt)*) => {
        // Underscored so it lives to scope end without an "unused" warning.
        let _defer_guard = Defer(Some(|| { $($body)* }));
    };
}

fn main() {
    defer! {
        println!("cleanup B (runs first: LIFO)");
    }
    defer! {
        println!("cleanup A (runs last)");
    }
    println!("body running");
}
```

Output:

```text
body running
cleanup A (runs last)
cleanup B (runs first: LIFO)
```

The two guards drop in reverse declaration order, so the *second* `defer!` runs first — exactly Go's LIFO semantics. `Option<F>` plus `.take()` is the standard idiom for calling an `FnOnce` from a `Drop`, since `Drop` only borrows `self`. For declarative macros in general, see [Section 14: Macros](/14-macros/).

</details>
