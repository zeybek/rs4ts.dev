---
title: "Choosing a Smart Pointer: A Decision Guide"
description: "A decision table mapping each need to one Rust smart pointer. Three questions (single or shared owner, mutated through &, one thread or many) pick Box, Rc"
---

This page is the map for the rest of Section 10. The other files each go deep on one smart pointer; this one steps back and answers the question you will actually ask on the job: **"I have this situation — which type do I reach for?"** It is built around a single decision table that maps a *need* to a *type*, plus the reasoning that gets you there.

---

## Quick Overview

Rust's standard library ships a handful of **smart pointers**: types that wrap a value, own (or borrow) the heap allocation behind it, and add a behavior such as shared ownership, interior mutability, or clone-on-write. In TypeScript and JavaScript you never choose any of this: every object is a garbage-collected, freely-shared, freely-mutable reference, and the runtime sorts out lifetime and aliasing for you. Rust hands those decisions back to you, so the skill this page teaches is **diagnosis**: given "shared or single owner?", "do I need to mutate it?", and "across threads or not?", you land on exactly one correct type.

> **Note:** This is the decision-guide companion to the per-type deep dives. Once the table points you at a type, follow the link to its dedicated page: [`00_box.md`](/10-smart-pointers/00-box/), [`01_rc-arc.md`](/10-smart-pointers/01-rc-arc/), [`02_refcell-mutex.md`](/10-smart-pointers/02-refcell-mutex/), [`03_cell.md`](/10-smart-pointers/03-cell/), [`04_cow.md`](/10-smart-pointers/04-cow/), [`05_weak.md`](/10-smart-pointers/05-weak/), and [`06_deref-trait.md`](/10-smart-pointers/06-deref-trait/) (the trait that makes all of them feel like the value they wrap). The whole section rests on the [ownership rules](/05-ownership/01-ownership-rules/) from Section 05.

---

## TypeScript/JavaScript Example

In TypeScript, there is only one storage model and you never pick it. An object is a heap value reached through a reference; assignment copies the reference, not the object; any holder can mutate it; and the garbage collector decides when it dies. Sharing, mutation, and lifetime are all implicit and all free at the type level.

```typescript
// One model for everything. No decisions to make.
interface Account {
  id: string;
  balance: number;
}

const account: Account = { id: "acc-1", balance: 100 };

// "Shared ownership": just alias the reference. Both see the same object.
const ledgerView = account;
const auditView = account;

// "Interior mutability": mutate through any alias, any time.
ledgerView.balance += 50;

// "Cross-thread sharing": Web Workers get a *structured clone* (a copy),
// not the same object — so true shared mutable state is the exception, not
// the default, and you reach for SharedArrayBuffer + Atomics for it.

console.log(auditView.balance); // 150 — the mutation is visible everywhere
console.log(account === ledgerView); // true — same underlying object
```

**Key points:**

- There is exactly **one** way objects are stored and shared, so there is nothing to choose.
- Sharing (`ledgerView = account`), mutation (`ledgerView.balance += 50`), and cleanup (the GC) are all implicit.
- The cost is invisibility: you cannot tell from a type whether a value is uniquely owned, shared, or being mutated behind your back: exactly the information Rust forces into the open.

---

## Rust Equivalent

Rust makes you state your intent, and each intent maps to a specific wrapper. The same scenario (a uniquely owned value, a shared read-only value, a shared *mutable* single-threaded value, and a shared mutable *cross-thread* value) uses four different types, and the type *is* the documentation.

```rust
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug)]
struct Account {
    id: String,
    balance: u64,
}

fn main() {
    // 1. Single owner, no sharing — just own the value (no smart pointer needed).
    let owned = Account { id: "acc-1".into(), balance: 100 };
    println!("owned: {} = {}", owned.id, owned.balance);

    // 2. Shared, read-only, single-threaded -> Rc<T>.
    let shared = Rc::new(Account { id: "acc-2".into(), balance: 100 });
    let ledger_view = Rc::clone(&shared); // cheap: bump the count
    println!("rc: owners={} balance={}", Rc::strong_count(&shared), ledger_view.balance);

    // 3. Shared AND mutable, single-threaded -> Rc<RefCell<T>>.
    let mutable = Rc::new(RefCell::new(Account { id: "acc-3".into(), balance: 100 }));
    let audit_view = Rc::clone(&mutable);
    mutable.borrow_mut().balance += 50; // interior mutability through a shared handle
    println!("rc<refcell>: balance={}", audit_view.borrow().balance);

    // 4. Shared AND mutable AND cross-thread -> Arc<Mutex<T>>.
    let across = Arc::new(Mutex::new(Account { id: "acc-4".into(), balance: 100 }));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let across = Arc::clone(&across);
        handles.push(thread::spawn(move || {
            across.lock().unwrap().balance += 25;
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!("arc<mutex>: balance={}", across.lock().unwrap().balance);
}
```

```text
owned: acc-1 = 100
rc: owners=2 balance=100
rc<refcell>: balance=150
arc<mutex>: balance=200
```

Each line of intent picked a different type. That is the discipline this page systematizes.

---

## Detailed Explanation

The four cases above are not arbitrary: they fall out of **three yes/no questions** you ask in order. Answering them mechanically lands you on the right type every time.

### Question 1: Do I need more than one owner?

- **No (single owner).** You usually need *nothing* — a plain owned value (`Account`, `String`, `Vec<T>`) lives on the stack or owns its own heap data and is freed when it goes out of scope. You only reach for [`Box<T>`](/10-smart-pointers/00-box/) when you need the value specifically *on the heap*: a recursive type that would otherwise be infinitely sized, a large value you want to move cheaply, or a [trait object](/09-generics-traits/06-trait-objects/) (`Box<dyn Trait>`) to store different concrete types behind one pointer.
- **Yes (multiple owners).** You need a reference-counted pointer — [`Rc<T>`](/10-smart-pointers/01-rc-arc/) single-threaded, [`Arc<T>`](/10-smart-pointers/01-rc-arc/) cross-thread. Go to Question 3.

### Question 2: Do I need to mutate it through a shared/`&` handle?

Rust's borrow checker normally forbids mutation through a shared reference. **Interior mutability** types move that check from compile time to run time so you *can* mutate behind a `&`:

- **[`Cell<T>`](/10-smart-pointers/03-cell/)** — for `Copy` types (numbers, `bool`, small enums). No references handed out; you `get()` a copy and `set()` a new value. Zero runtime borrow tracking, so it cannot panic.
- **[`RefCell<T>`](/10-smart-pointers/02-refcell-mutex/)** — for non-`Copy` types. Hands out `Ref`/`RefMut` guards and enforces the borrow rules *at run time* (`borrow()` / `borrow_mut()`), **panicking** if you break them. Single-threaded only.
- **[`Mutex<T>`](/10-smart-pointers/02-refcell-mutex/)** / `RwLock<T>` — the thread-safe equivalents. `lock()` blocks until exclusive access is available and returns a guard.

### Question 3: Does the sharing cross threads?

This is the single most important branch, because the compiler enforces it for you via the `Send`/`Sync` marker traits:

- **Single-threaded** → `Rc<T>` and `RefCell<T>`/`Cell<T>`. Their counters and borrow flags are *not* synchronized, which makes them fast, and the compiler will refuse to send them between threads (you will see the real error in [Common Pitfalls](#common-pitfalls)).
- **Cross-thread** → `Arc<T>` and `Mutex<T>`/`RwLock<T>` (or an `Atomic*` type for a single `Copy` value). `Arc` uses *atomic* count updates; `Mutex` synchronizes access. They cost a little more, so you do not pay for them unless you actually share across threads.

### Putting it together

The combinations you will write in real code are predictable:

- `Rc<RefCell<T>>`: the single-threaded "shared mutable object", the closest thing to a plain JavaScript object reference.
- `Arc<Mutex<T>>`: the cross-thread version, ubiquitous in async servers (see [Section 11](/11-async/)).
- `Arc<T>` alone: shared *immutable* data across threads (config, lookup tables), no lock needed.
- `Weak<T>`: a non-owning handle that breaks the reference *cycles* `Rc`/`Arc` would otherwise leak (a child pointing back at its parent). Covered in [`05_weak.md`](/10-smart-pointers/05-weak/).

---

## Key Differences

### The master decision table

This is the heart of the page. Read left to right: pin down your three answers, then read off the type.

| Your need                                                        | Reach for                          | Why                                                                   |
| ---------------------------------------------------------------- | ---------------------------------- | --------------------------------------------------------------------- |
| One owner; value on the **stack** is fine                        | plain `T` (no wrapper)             | Ownership already gives you this for free                             |
| One owner; need it on the **heap**                               | [`Box<T>`](/10-smart-pointers/00-box/)               | Heap allocation, single owner, fixed pointer size                     |
| A **recursive** type (tree, linked list, AST)                    | [`Box<T>`](/10-smart-pointers/00-box/)               | Breaks the "infinite size" cycle so the type has a known size         |
| Store **different concrete types** behind one type               | `Box<dyn Trait>`                   | An owned [trait object](/09-generics-traits/06-trait-objects/)       |
| **Multiple owners**, read-only, single thread                    | [`Rc<T>`](/10-smart-pointers/01-rc-arc/)             | Reference counting; `clone` is a cheap count bump                     |
| **Multiple owners**, read-only, across threads                   | [`Arc<T>`](/10-smart-pointers/01-rc-arc/)            | Atomic reference counting; `Send + Sync`                              |
| Mutate a **`Copy`** value through `&self`                        | [`Cell<T>`](/10-smart-pointers/03-cell/)             | `get`/`set` with no references, no runtime borrow tracking            |
| Mutate a **non-`Copy`** value through `&self`, single thread     | [`RefCell<T>`](/10-smart-pointers/02-refcell-mutex/) | Runtime-checked `borrow`/`borrow_mut` (panics on violation)           |
| Mutate through `&self`, **across threads**                       | [`Mutex<T>`](/10-smart-pointers/02-refcell-mutex/)   | Locking grants exclusive access; `Sync` when `T: Send`                |
| Across threads, **many readers / few writers**                   | `RwLock<T>`                        | Concurrent shared reads, exclusive writes                            |
| Across threads, a **single `Copy` counter/flag**                 | `AtomicUsize`, `AtomicBool`, …     | Lock-free; cheaper than `Mutex<u64>` for one number                   |
| Shared **mutable object**, single thread                         | `Rc<RefCell<T>>`                   | The combo: shared ownership + interior mutability                     |
| Shared **mutable object**, across threads                        | `Arc<Mutex<T>>`                    | The thread-safe combo; the default for async shared state             |
| Break a reference **cycle** (parent ↔ child)                     | [`Weak<T>`](/10-smart-pointers/05-weak/)             | Non-owning; does not keep the target alive                            |
| "Borrowed *or* owned", avoid copying on the hot path             | [`Cow<'_, T>`](/10-smart-pointers/04-cow/)           | Clone-on-write: allocate only when you must mutate/keep               |

> **Tip:** Start at the top and stop at the first row that matches. The rows are ordered cheapest-and-simplest first, which mirrors the idiomatic instinct: do not reach for `Arc<Mutex<T>>` when a plain `Vec<T>` would do.

### Decision flow in one breath

> 1. **More than one owner?** No → plain value or `Box`. Yes → reference counting.
> 2. **Across threads?** No → `Rc` / `RefCell` / `Cell`. Yes → `Arc` / `Mutex` / `RwLock` / atomics.
> 3. **Mutate through a shared handle?** No → the bare pointer. Yes → wrap the inner type in an interior-mutability cell.

### How this maps to your TypeScript mental model

| TypeScript/JavaScript reality                       | Rust equivalent and what changed                                                        |
| --------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `const b = a` aliases the same object               | `Rc::clone(&a)` / `Arc::clone(&a)`: explicit, and you can read the count               |
| Mutate any object through any alias, any time       | `RefCell`/`Mutex`: allowed but explicit; `RefCell` panics if you break the rules       |
| GC frees objects "eventually"                       | `Rc`/`Arc` free **deterministically** when the last owner drops (count hits zero)       |
| Web Worker `postMessage` deep-copies                | `Arc` shares the *same* allocation across threads; the compiler proves it is safe       |
| Circular references are fine (the GC handles them)  | `Rc` cycles **leak**; you break them with [`Weak<T>`](/10-smart-pointers/05-weak/)                         |
| `string` is immutable; copies are invisible         | [`Cow<'_, str>`](/10-smart-pointers/04-cow/) makes "borrowed vs freshly allocated" an explicit type        |

---

## Common Pitfalls

### Pitfall 1: Reaching for `Arc<Mutex<T>>` by reflex

Coming from a world where everything is shared and mutable, it is tempting to wrap *everything* in `Arc<Mutex<T>>` "to be safe." This is the most common over-engineering mistake. If a value has one owner, use it plainly. If it is shared but never mutated across threads, `Arc<T>` alone is enough, no lock. If it never leaves one thread, `Rc`/`RefCell` are faster. Every layer you add costs allocation, indirection, and (for `Mutex`) lock contention.

> **Warning:** `Arc<Mutex<T>>` is the *last* row of the table for a reason. Earn it by answering the three questions. Do not start there.

### Pitfall 2: Using `Rc` where the data crosses threads

`Rc` is single-threaded on purpose, and the compiler enforces it. If you try to move an `Rc` into a spawned thread, you get a compile error, not a crash later:

```rust
use std::rc::Rc;
use std::thread;

fn main() {
    let shared = Rc::new(5);
    let s2 = Rc::clone(&shared);
    thread::spawn(move || {       // does not compile (error[E0277]: `Rc<i32>` cannot be sent between threads safely)
        println!("{}", s2);
    });
}
```

The real error from `cargo build`:

```text
error[E0277]: `Rc<i32>` cannot be sent between threads safely
   --> src/main.rs:7:19
    |
  7 |       thread::spawn(move || {
    |       ------------- ^------
    |       |             |
    |  _____|_____________within this `{closure@src/main.rs:7:19: 7:26}`
    | |     |
    | |     required by a bound introduced by this call
  8 | |         println!("{}", s2);
  9 | |     });
    | |_____^ `Rc<i32>` cannot be sent between threads safely
    |
    = help: within `{closure@src/main.rs:7:19: 7:26}`, the trait `Send` is not implemented for `Rc<i32>`
```

The fix is exactly what the table says: swap `Rc` for `Arc`. The compiler turned a whole class of data-race bugs into a single, named, before-it-runs error.

### Pitfall 3: Forgetting that `RefCell` moves the check to *run time*

Choosing `RefCell` (or its multi-threaded sibling `Mutex`) does not turn off the borrow rules; it defers them. Two simultaneous `borrow_mut()`s compile fine but **panic** at run time:

```rust
use std::cell::RefCell;

fn main() {
    let cell = RefCell::new(vec![1, 2, 3]);
    let a = cell.borrow_mut();
    let b = cell.borrow_mut(); // second active mutable borrow
    println!("{:?} {:?}", a, b);
}
```

```text
thread 'main' panicked at src/main.rs:6:18:
RefCell already borrowed
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

If you want the rule enforced at *compile* time instead, you did not need interior mutability; you needed a plain `&mut`. Reach for `RefCell` only when shared structure genuinely forces the check to run time.

### Pitfall 4: Building an `Rc` cycle and leaking memory

`Rc`/`Arc` count *strong* owners. If two values own each other strongly (a parent owning its children, each child owning its parent), the count never reaches zero and the memory leaks, even in Rust. The fix is to make one direction a non-owning [`Weak<T>`](/10-smart-pointers/05-weak/). This is the one place where Rust's deterministic cleanup can still let memory escape, so the decision table routes you to `Weak` the moment you see a back-pointer.

### Pitfall 5: Cloning the inner value instead of the pointer

`Rc::clone(&x)` and `x.clone()` look interchangeable, and for an `Rc` they do the same cheap count bump. But idiomatic code writes `Rc::clone(&x)` (fully-qualified) precisely so a reader can tell at a glance "this is a cheap pointer clone," not a deep copy of the wrapped value. Clippy will not force this, but the convention keeps the cost obvious.

---

## Best Practices

- **Default to the simplest thing that compiles.** Plain ownership first; add a wrapper only when a real requirement (sharing, heap, interior mutation, threads) forces it. Walk *down* the table, not up.
- **Let the three questions drive the type, not habit.** "Multiple owners? Across threads? Mutated through `&`?" — answer those and the type is determined.
- **Pair the pointer with the right inner cell.** Shared mutation is always *pointer + cell*: `Rc<RefCell<T>>` single-threaded, `Arc<Mutex<T>>` (or `Arc<RwLock<T>>`) cross-thread. You rarely use `RefCell`/`Mutex` without an `Rc`/`Arc` around them, because a value you can reach from only one place can just use `&mut`.
- **Prefer an atomic to `Mutex<one number>`.** For a single shared `Copy` counter or flag across threads, `AtomicUsize`/`AtomicBool` are lock-free and clearer than `Arc<Mutex<u64>>`.
- **Use `Cow` for "usually unchanged" APIs.** Functions that normally return their input untouched — escaping, normalizing, defaulting — return [`Cow<'_, str>`](/10-smart-pointers/04-cow/) to skip the allocation on the common path.
- **Write `Rc::clone(&x)` / `Arc::clone(&x)` explicitly** so cheap pointer clones never read like deep copies.
- **Reach for `Weak` the instant you see a cycle.** Any "child knows its parent" or graph back-edge is a `Weak`, by default.

---

## Real-World Example

A small in-memory job registry that several worker threads share and mutate. Walking the three questions: multiple owners (every worker holds the store) → reference counting; across threads → `Arc`; mutated through a shared handle → `Mutex`. The table lands us squarely on `Arc<Mutex<HashMap<...>>>`.

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone)]
struct Job {
    id: u64,
    title: String,
    done: bool,
}

// Shared, mutable, cross-thread state -> Arc<Mutex<...>>.
type JobStore = Arc<Mutex<HashMap<u64, Job>>>;

fn add_job(store: &JobStore, id: u64, title: &str) {
    let mut map = store.lock().unwrap();
    map.insert(id, Job { id, title: title.to_string(), done: false });
}

fn complete_job(store: &JobStore, id: u64) {
    let mut map = store.lock().unwrap();
    if let Some(job) = map.get_mut(&id) {
        job.done = true;
    }
}

fn main() {
    let store: JobStore = Arc::new(Mutex::new(HashMap::new()));

    add_job(&store, 1, "index documents");
    add_job(&store, 2, "send emails");

    // Workers share the same store via Arc clones.
    let mut handles = Vec::new();
    for id in [1u64, 2] {
        let store = Arc::clone(&store);
        handles.push(thread::spawn(move || complete_job(&store, id)));
    }
    for h in handles {
        h.join().unwrap();
    }

    let map = store.lock().unwrap();
    let mut ids: Vec<&u64> = map.keys().collect();
    ids.sort();
    for id in ids {
        let job = &map[id];
        println!("job {} \"{}\" done={}", job.id, job.title, job.done);
    }
}
```

```text
job 1 "index documents" done=true
job 2 "send emails" done=true
```

Note what we did **not** reach for: the `Job` values inside are plain owned structs (single owner, the map), the `id` keys are plain `u64`, and there is exactly one lock guarding the whole map. The decision guide kept the design as small as the requirements allow. This is the same shape you will see holding application state in an async web server in [Section 11](/11-async/).

---

## Further Reading

### Official Documentation

- [The Rust Book — Smart Pointers (Chapter 15)](https://doc.rust-lang.org/book/ch15-00-smart-pointers.html): the canonical overview
- [`std::boxed::Box`](https://doc.rust-lang.org/std/boxed/struct.Box.html), [`std::rc::Rc`](https://doc.rust-lang.org/std/rc/struct.Rc.html), [`std::sync::Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html)
- [`std::cell`](https://doc.rust-lang.org/std/cell/index.html) — `Cell` and `RefCell`, with an excellent module-level discussion of interior mutability
- [`std::sync::Mutex`](https://doc.rust-lang.org/std/sync/struct.Mutex.html), [`std::sync::RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html), [`std::sync::atomic`](https://doc.rust-lang.org/std/sync/atomic/index.html)
- [`std::borrow::Cow`](https://doc.rust-lang.org/std/borrow/enum.Cow.html)

### Related Topics

- [`00_box.md`](/10-smart-pointers/00-box/) — heap allocation, recursive types, and `Box<dyn Trait>`
- [`01_rc-arc.md`](/10-smart-pointers/01-rc-arc/) — shared ownership and reference counting in depth
- [`02_refcell-mutex.md`](/10-smart-pointers/02-refcell-mutex/) — interior mutability, runtime borrow checks, lock guards
- [`03_cell.md`](/10-smart-pointers/03-cell/) — the lightweight `Cell<T>` for `Copy` types
- [`04_cow.md`](/10-smart-pointers/04-cow/) — clone-on-write and avoiding needless allocation
- [`05_weak.md`](/10-smart-pointers/05-weak/) — breaking reference cycles
- [`06_deref-trait.md`](/10-smart-pointers/06-deref-trait/) — why every smart pointer feels like the value it wraps
- [Section 05: Ownership](/05-ownership/) — the model that makes these choices necessary
- [Section 09: Generics & Traits](/09-generics-traits/) — trait objects, the partner of `Box<dyn Trait>`
- [Section 11: Async](/11-async/) — where `Arc<Mutex<T>>` becomes everyday shared state

---

## Exercises

### Exercise 1: Borrowed or owned?

**Difficulty:** Beginner

**Objective:** Pick the right wrapper for a "usually unchanged" function and observe when it allocates.

**Instructions:** Write `strip_comment(line: &str)` that removes a trailing `# ...` comment (and the whitespace before it) from a config line. If there is no `#`, the input is returned untouched with **no allocation**; otherwise a new trimmed string is produced. Choose the type the decision table recommends for "borrowed *or* owned", then verify which branch each call took.

```rust
use std::borrow::Cow;

fn strip_comment(line: &str) -> Cow<'_, str> {
    // TODO: borrow when there's no '#', allocate only when trimming a comment.
    /* ??? */
}

fn main() {
    // TODO: call it with a clean line and a commented line; print which variant you got.
}
```

<details>
<summary>Solution</summary>

```rust
use std::borrow::Cow;

fn strip_comment(line: &str) -> Cow<'_, str> {
    match line.find('#') {
        Some(idx) => Cow::Owned(line[..idx].trim_end().to_string()),
        None => Cow::Borrowed(line),
    }
}

fn main() {
    let a = strip_comment("value = 42");
    let b = strip_comment("value = 42  # the answer");
    println!("a={a:?} ({})", if matches!(a, Cow::Borrowed(_)) { "borrowed" } else { "owned" });
    println!("b={b:?} ({})", if matches!(b, Cow::Borrowed(_)) { "borrowed" } else { "owned" });
}
```

Real output:

```text
a="value = 42" (borrowed)
b="value = 42" (owned)
```

The clean line is handed straight back (no allocation); only the commented line produces an owned `String`. This is the [`Cow`](/10-smart-pointers/04-cow/) row of the table in action.

</details>

### Exercise 2: A shared, mutable, single-threaded cache

**Difficulty:** Intermediate

**Objective:** Combine the pointer and the cell the table prescribes for "shared mutable object, single thread."

**Instructions:** Build a `Cache` holding a `HashMap<String, u64>` of counts. It must be `Clone`-able so several parts of one (single-threaded) program can hold a handle to the **same** underlying map, and `bump(&self, key)` must increment and return the count *through a shared `&self`*. Answer the three questions to choose the type, then prove the clones share state by reading the owner count.

<details>
<summary>Solution</summary>

```rust
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone)]
struct Cache {
    // Multiple owners (Rc) + mutation through &self (RefCell), single-threaded.
    inner: Rc<RefCell<HashMap<String, u64>>>,
}

impl Cache {
    fn new() -> Self {
        Cache { inner: Rc::new(RefCell::new(HashMap::new())) }
    }

    fn bump(&self, key: &str) -> u64 {
        let mut map = self.inner.borrow_mut();
        let entry = map.entry(key.to_string()).or_insert(0);
        *entry += 1;
        *entry
    }
}

fn main() {
    let cache = Cache::new();
    let alias = cache.clone(); // cheap Rc bump: same underlying map
    cache.bump("a");
    alias.bump("a");
    let n = cache.bump("b");
    println!("a={}", cache.bump("a")); // 3
    println!("b={n}"); // 1
    println!("owners={}", Rc::strong_count(&cache.inner));
}
```

Real output:

```text
a=3
b=1
owners=2
```

`cache` and `alias` share one map (three bumps of `"a"` across two handles → 3), and the strong count of `2` confirms they own the same allocation. This is the `Rc<RefCell<T>>` row, the closest Rust gets to a plain JavaScript object reference. See [`02_refcell-mutex.md`](/10-smart-pointers/02-refcell-mutex/) and [`01_rc-arc.md`](/10-smart-pointers/01-rc-arc/).

</details>

### Exercise 3: Promote it to threads — and skip the lock

**Difficulty:** Advanced

**Objective:** Move a shared counter across threads and recognize when the table's last rows (`Arc` + atomic) beat `Arc<Mutex<T>>`.

**Instructions:** Eight threads each increment a shared counter 1000 times (expected total `8000`). The shared value is a single `Copy` integer, so instead of `Arc<Mutex<u64>>`, use the cheaper lock-free option the table recommends for "a single `Copy` counter across threads." Share it with `Arc`, increment with the atomic's own method, and print the final total.

> **Hint:** `Arc<AtomicU64>`, then `fetch_add(1, Ordering::Relaxed)` inside each thread; read the result with `load(Ordering::Relaxed)`.

<details>
<summary>Solution</summary>

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

fn main() {
    // A single Copy counter shared across threads: an atomic beats Mutex<u64>.
    let hits = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let hits = Arc::clone(&hits);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                hits.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!("hits = {}", hits.load(Ordering::Relaxed));
}
```

Real output:

```text
hits = 8000
```

`Arc` provides the shared ownership across threads; the `AtomicU64` provides synchronized mutation with **no lock** to acquire or release. For a single number this is both faster and clearer than `Arc<Mutex<u64>>`, which is exactly why the decision table lists atomics as their own row. See [`02_refcell-mutex.md`](/10-smart-pointers/02-refcell-mutex/) for the locking alternatives and [`01_rc-arc.md`](/10-smart-pointers/01-rc-arc/) for `Arc` itself.

</details>
