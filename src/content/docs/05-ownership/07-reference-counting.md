---
title: "Reference Counting with `Rc<T>` and `Arc<T>`"
description: "Rust's Rc and Arc give a value multiple owners with deterministic cleanup, the explicit counterpart to JavaScript's shared object references and garbage collector."
---

So far every value in Rust has had exactly **one owner**. But some data is genuinely shared — a configuration object read by many parts of a program, a node referenced by several edges in a graph. Reference counting is Rust's safe, opt-in way to give one value **multiple owners**, with cleanup that still happens automatically and deterministically.

---

## Quick Overview

`Rc<T>` (**reference counted**) and `Arc<T>` (**atomically reference counted**) are smart pointers that let a value have more than one owner. Each holds a count of how many owners currently exist; when the last one goes away, the value is dropped. For a TypeScript/JavaScript developer this feels like normal shared object references, except the "garbage collector" here is a tiny integer counter the compiler manages, with `Rc` for single-threaded code and `Arc` for sharing across threads.

> **Note:** This page is a focused, practical introduction. `Rc`/`Arc` are *smart pointers*, and the full family (`Box<T>`, `RefCell<T>`, `Weak<T>`, reference cycles, and the `Deref`/`Drop` machinery behind them) is covered in [Section 10: Smart Pointers](/10-smart-pointers/). Here we cover what you need to share ownership safely and read a strong count.

---

## TypeScript/JavaScript Example

In JavaScript and TypeScript, sharing is the default and invisible. Assigning an object to another variable, pushing it into an array, or storing it on another object all create additional references to the **same** object. The garbage collector (GC) frees it only once nothing can reach it anymore.

```typescript
// TypeScript/JavaScript: object references are shared implicitly
interface Config {
  baseUrl: string;
  timeoutMs: number;
}

const config: Config = {
  baseUrl: "https://api.example.com",
  timeoutMs: 5000,
};

// Every one of these is the SAME object, not a copy:
const forLogger = config;
const forRetry = config;
const sinks = [config, forLogger]; // array also holds the same reference

forRetry.timeoutMs = 10000; // mutating through one handle...
console.log(forLogger.timeoutMs); // 10000 — ...is visible through all of them

// You never count references and never free anything. When `config`,
// `forLogger`, `forRetry`, and `sinks` all become unreachable, the GC
// reclaims the object at some unspecified later time.
```

**Key points:**

- Assignment shares a reference; there is no count you can see and no `free` to call.
- Any handle can mutate the shared object, and the change is visible everywhere.
- Cleanup is **non-deterministic** — the GC decides when.

---

## Rust Equivalent

Rust will not let you alias an owned value freely (that is the whole point of [the ownership rules](/05-ownership/01-ownership-rules/)). To opt into shared ownership, you wrap the value in `Rc<T>` and create additional owners with `Rc::clone`. Each clone bumps a strong count; each drop lowers it; the value is freed when the count hits zero.

```rust
use std::rc::Rc;

#[derive(Debug)]
struct Config {
    base_url: String,
    timeout_ms: u32,
}

fn main() {
    // ---- Rc basics: shared ownership, strong count ----
    let config = Rc::new(Config {
        base_url: String::from("https://api.example.com"),
        timeout_ms: 5000,
    });
    println!("after create:  count = {}", Rc::strong_count(&config));

    let for_logger = Rc::clone(&config);
    println!("after 1 clone: count = {}", Rc::strong_count(&config));

    {
        let for_retry = Rc::clone(&config);
        println!("inside block:  count = {}", Rc::strong_count(&config));
        println!("retry sees timeout = {}", for_retry.timeout_ms);
    } // `for_retry` dropped here -> count goes back down

    println!("after block:   count = {}", Rc::strong_count(&config));
    println!("logger sees url = {}", for_logger.base_url);
}
```

**Output:**

```text
after create:  count = 1
after 1 clone: count = 2
inside block:  count = 3
retry sees timeout = 5000
after block:   count = 2
logger sees url = https://api.example.com
```

**Key points:**

- `Rc::new(value)` puts `value` on the heap with a strong count of `1`.
- `Rc::clone(&config)` creates another owner and increments the count. It does **not** deep-copy the `Config`.
- When `for_retry` goes out of scope at the inner `}`, the count drops back from `3` to `2`, deterministically.
- The `Config` is freed exactly when the last `Rc` is dropped — no GC, no leak (caveat: reference cycles, covered below).

---

## Detailed Explanation

### What `Rc<T>` actually is

`Rc<T>` is a smart pointer that owns a heap allocation containing two things: your value of type `T`, and a small bookkeeping header with the counts. Conceptually:

```text
   stack                         heap
 ┌─────────┐        ┌───────────────────────────────┐
 │ config  │ ─────▶ │ strong: 2 | weak: 0 |  T value │
 └─────────┘        └───────────────────────────────┘
 ┌─────────┐           ▲
 │for_logger│ ─────────┘   (both Rc handles point at the same allocation)
 └─────────┘
```

Each `Rc` handle is a pointer; cloning one copies the pointer and bumps `strong` by one. The value `T` is allocated **once** and shared. This is the same layout idea as a JavaScript object reference (multiple variables pointing at one heap object), but the count is explicit and the deallocation is driven by it.

### `Rc::clone` is cheap and explicit

```rust
let for_logger = Rc::clone(&config);
```

Read this as "make another owner of the same allocation." It copies a pointer and increments an integer — it is **not** the expensive deep `clone()` you saw in [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/). `Rc<T>` does implement the `Clone` trait, so `config.clone()` works and does exactly the same thing. The community convention is to write `Rc::clone(&config)` (the *associated-function* form) rather than `config.clone()`, because it makes clear at the call site that you are cheaply bumping a refcount, not deep-copying the data.

You can confirm all clones point at the same heap value: `Rc::as_ptr` returns the address of the inner `T`, and it is identical across clones (the exact address varies per run):

```rust
use std::rc::Rc;

fn main() {
    let original = Rc::new(vec![10, 20, 30]);
    let via_method = original.clone();    // calls Rc's Clone impl
    let via_assoc = Rc::clone(&original); // identical behavior

    println!("original ptr: {:p}", Rc::as_ptr(&original));
    println!("method   ptr: {:p}", Rc::as_ptr(&via_method));
    println!("assoc    ptr: {:p}", Rc::as_ptr(&via_assoc));
    println!("strong_count = {}", Rc::strong_count(&original));
}
```

All three printed pointers are the same address, and `strong_count` is `3`. No `Vec` was copied.

### The strong count and automatic drop

`Rc::strong_count(&rc)` reports how many owners exist right now. The count goes up on every clone and down on every drop, including the implicit drop at the end of a scope and the explicit `std::mem::drop` (the same `drop` from [the Drop trait page](/05-ownership/08-drop-trait/)):

```rust
use std::rc::Rc;

fn main() {
    let data = Rc::new(vec![1, 2, 3]);   // count = 1
    let a = Rc::clone(&data);            // count = 2
    let b = Rc::clone(&data);            // count = 3
    println!("after two clones: {}", Rc::strong_count(&data));

    drop(a);                             // count = 2
    println!("after drop(a):    {}", Rc::strong_count(&data));

    drop(b);                             // count = 1
    println!("after drop(b):    {}", Rc::strong_count(&data));

    println!("data still usable: {:?}", data);
}
```

**Output:**

```text
after two clones: 3
after drop(a):    2
after drop(b):    1
data still usable: [1, 2, 3]
```

When the count would reach `0`, the inner `Vec` is dropped and its heap memory freed. This is still **deterministic** RAII (it happens at a point you can identify by reading the code); it is just that "the owner" is now "the last surviving handle" instead of a single binding.

### Shared means read-only

`Rc<T>` gives you shared ownership, and shared access in Rust means **immutable** access (the same one-mutable-XOR-many-shared rule from [Mutable References](/05-ownership/03-mutable-references/), now enforced at the type level). You cannot get a `&mut T` out of an `Rc<T>` while it might be shared, so you cannot mutate the inner value directly. To have shared *and* mutable data, you combine `Rc<T>` with a cell type that provides **interior mutability**, almost always `RefCell<T>` (single-threaded):

```rust
use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    // Shared, mutable counter: many handles, interior mutability via RefCell.
    let hits = Rc::new(RefCell::new(0u32));

    let a = Rc::clone(&hits);
    let b = Rc::clone(&hits);

    *a.borrow_mut() += 1; // mutate through one handle
    *b.borrow_mut() += 1; // mutate through another

    println!("hits = {}", hits.borrow());
    println!("strong_count = {}", Rc::strong_count(&hits));
}
```

**Output:**

```text
hits = 2
strong_count = 3
```

`RefCell` moves the borrow-checking from compile time to *runtime* (it panics if you break the borrowing rules), and it is the standard partner for `Rc`. The full `RefCell` story lives in [Section 10](/10-smart-pointers/); the takeaway here is the pattern `Rc<RefCell<T>>` for shared mutable state on one thread.

### `Arc<T>`: the same idea, safe across threads

`Rc<T>` deliberately uses a **non-atomic** counter, which is fast but not safe to touch from multiple threads. The compiler enforces this: an `Rc` is not `Send`, so you cannot move one into another thread. When you need to share across threads, switch to `Arc<T>` — identical API, but the count is updated with **atomic** operations:

```rust
use std::sync::Arc;
use std::thread;

#[derive(Debug)]
struct Settings {
    workers: u32,
    region: String,
}

fn main() {
    let settings = Arc::new(Settings {
        workers: 4,
        region: String::from("eu-west-1"),
    });

    let mut handles = Vec::new();
    for id in 0..3 {
        let settings = Arc::clone(&settings); // each thread gets its own handle
        let handle = thread::spawn(move || {
            println!(
                "worker {id} reading region={} workers={}",
                settings.region, settings.workers
            );
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    println!("final strong_count = {}", Arc::strong_count(&settings));
}
```

**Output** (the three worker lines may appear in any order because they run concurrently; the final count is always `1`):

```text
worker 0 reading region=eu-west-1 workers=4
worker 1 reading region=eu-west-1 workers=4
worker 2 reading region=eu-west-1 workers=4
final strong_count = 1
```

Each `Arc::clone` gives a thread its own owner; the `move` closure takes that clone with it. After all threads `join`, their clones have been dropped, so the count is back to `1` (just the original binding). For shared *mutable* state across threads you pair `Arc` with `Mutex` or `RwLock` (`Arc<Mutex<T>>`), the thread-safe analog of `Rc<RefCell<T>>`. Threads and these locks are the subject of [Section 26: Systems Programming](/26-systems-programming/); for now, just remember the swap: **`Rc` + `RefCell`** on one thread, **`Arc` + `Mutex`** across threads.

> **Note:** Rust's compiler will *stop you* from using `Rc` across threads — you don't have to remember the rule. The error (`Rc<...> cannot be sent between threads safely`) is shown in the Pitfalls section below.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust `Rc<T>` / `Arc<T>` |
| --- | --- | --- |
| Shared ownership | Default and implicit (object references) | Opt-in: wrap in `Rc`/`Arc` and `clone` |
| How sharing happens | Assignment / passing aliases the object | `Rc::clone(&x)` bumps a strong count |
| Cleanup trigger | GC decides when nothing can reach it | Strong count reaches `0` → value dropped |
| Cleanup timing | Non-deterministic | Deterministic (at the drop of the last owner) |
| Mutation of shared data | Any handle can mutate freely | Shared = immutable; need `RefCell`/`Mutex` for mutation |
| Cost of a "reference" | Hidden GC bookkeeping + tracing | One pointer + one integer increment/decrement |
| Thread safety | Single-threaded model (event loop) | `Rc` single-thread only; `Arc` for threads (atomic count) |
| Cycles | Collected by the GC's reachability tracing | **Leak** unless broken with `Weak<T>` |

**The core mental shift:** in JavaScript, *every* object reference is traced for you (the engine periodically walks reachability and frees what it can no longer reach), invisibly, by the runtime. In Rust, the default is single ownership with zero runtime cost; `Rc`/`Arc` is how you *deliberately* buy shared ownership, and you pay a small, visible price (a counter, an allocation header, and, for `Arc`, atomic operations).

> **Tip:** Reach for `Rc`/`Arc` only when ownership is *genuinely* shared and you cannot express the relationship with a plain borrow (`&T`). Many designs that look like they need shared ownership are actually fine with borrowing plus [lifetimes](/05-ownership/04-lifetimes/). Shared ownership is a tool, not a default.

---

## Common Pitfalls

### Pitfall 1: Trying to mutate the value inside an `Rc`

Coming from JavaScript, you expect `rc.field = x` or `*rc += 1` to just work, because shared objects are mutable there. In Rust, `Rc<T>` only gives shared (immutable) access:

```rust
use std::rc::Rc;

fn main() {
    let count = Rc::new(0u32);
    let _other = Rc::clone(&count);
    *count += 1; // does not compile (error[E0594]): can't mutate through Rc
    println!("{count}");
}
```

Real compiler output:

```text
error[E0594]: cannot assign to data in an `Rc`
 --> src/main.rs:6:5
  |
6 |     *count += 1; // does not compile (error[E0594]): can't mutate through Rc
  |     ^^^^^^^^^^^ cannot assign
  |
  = help: trait `DerefMut` is required to modify through a dereference, but it is not implemented for `Rc<u32>`
```

**Fix:** wrap the inner value in a cell type for interior mutability, `Rc<RefCell<u32>>` (single-threaded) or `Arc<Mutex<u32>>` (across threads), then mutate through `.borrow_mut()` / `.lock()`. See the `Rc<RefCell<T>>` example above.

### Pitfall 2: Sending an `Rc` to another thread

`Rc` uses a non-atomic counter, so the compiler refuses to move it into a thread:

```rust
use std::rc::Rc;
use std::thread;

fn main() {
    let shared = Rc::new(String::from("config"));
    let handle = Rc::clone(&shared);
    thread::spawn(move || {
        println!("{handle}"); // does not compile (error[E0277]): Rc is not Send
    });
}
```

Real compiler output (trimmed):

```text
error[E0277]: `Rc<String>` cannot be sent between threads safely
   --> src/main.rs:7:19
    |
  7 |       thread::spawn(move || {
    |       ------------- ^------
    |       |             |
    |  _____|_____________within this `{closure@src/main.rs:7:19: 7:26}`
    | |     |
    | |     required by a bound introduced by this call
... |
    | |_____^ `Rc<String>` cannot be sent between threads safely
    |
    = help: within `{closure@src/main.rs:7:19: 7:26}`, the trait `Send` is not implemented for `Rc<String>`
```

**Fix:** use `Arc` instead of `Rc`. The API is identical; just swap the type and `use std::sync::Arc;`.

### Pitfall 3: Reference cycles leak memory

This is the one case where reference counting fails to clean up. If two `Rc`s point at each other (directly or through a chain), their counts never reach zero, so neither is ever dropped — a real memory **leak**, even in safe Rust:

```rust
use std::cell::RefCell;
use std::rc::Rc;

struct Node {
    name: String,
    next: RefCell<Option<Rc<Node>>>,
}

impl Drop for Node {
    fn drop(&mut self) {
        println!("dropping node {}", self.name);
    }
}

fn main() {
    let a = Rc::new(Node { name: String::from("A"), next: RefCell::new(None) });
    let b = Rc::new(Node { name: String::from("B"), next: RefCell::new(None) });

    // Create a cycle: a -> b -> a
    *a.next.borrow_mut() = Some(Rc::clone(&b));
    *b.next.borrow_mut() = Some(Rc::clone(&a));

    println!("a strong_count = {}", Rc::strong_count(&a));
    println!("b strong_count = {}", Rc::strong_count(&b));
    println!("end of main — watch for drop messages...");
} // a and b go out of scope, but each is still held by the other -> NO drops -> leak
```

**Output** (note: the `dropping node` messages **never print**; the nodes leak):

```text
a strong_count = 2
b strong_count = 2
end of main — watch for drop messages...
```

This compiles and runs fine; it is not a crash, just a leak. A JavaScript GC would collect this cycle via reachability tracing; reference counting alone cannot.

**Fix:** make one direction of the cycle a **non-owning** reference using `Weak<T>` (a weak handle that does not contribute to the strong count). `Weak`, cycles, and the parent/child pattern are covered in depth in [Section 10: Smart Pointers](/10-smart-pointers/).

### Pitfall 4: Using `.clone()` and not realizing it is cheap (or expecting it to be cheap when it isn't)

Two confusions, opposite directions:

- On an `Rc<BigThing>`, calling `.clone()` is cheap — it bumps a count, it does not copy `BigThing`. New Rust developers sometimes avoid it thinking it is expensive.
- On a plain `BigThing` (no `Rc`), `.clone()` *is* a deep copy. Wrapping in `Rc` is precisely how you turn an expensive clone into a cheap one when the data is shared and read-only.

**Fix:** prefer the explicit `Rc::clone(&x)` / `Arc::clone(&x)` form so reviewers can see at a glance that a clone is a refcount bump, not a deep copy.

---

## Best Practices

- **Borrow first; reach for `Rc`/`Arc` only for genuine shared ownership.** If a single owner with borrowed access (`&T`) models your data, use that; it is faster and has no runtime bookkeeping. Use `Rc`/`Arc` when *multiple parts of the program must independently keep the value alive* and you cannot tie them to a single owner's lifetime.

- **Use the associated-function clone form.** Write `Rc::clone(&x)` / `Arc::clone(&x)`, not `x.clone()`, so the intent ("share, don't deep-copy") is obvious at the call site. This is the idiom recommended by the Rust Book.

- **Pick `Rc` by default, upgrade to `Arc` only when crossing threads.** `Arc`'s atomic counter is slightly slower; don't pay for thread safety you don't use. And you won't accidentally use the wrong one — the compiler enforces it via `Send`/`Sync`.

- **Combine with the right cell for mutation.** `Rc<RefCell<T>>` for single-threaded shared-mutable; `Arc<Mutex<T>>` (or `Arc<RwLock<T>>`) across threads. Keep the locked/borrowed region small.

- **Watch for cycles in graph-shaped data.** Parent→child links can own (`Rc`/`Arc`); child→parent back-links should be `Weak` to avoid leaks. If you find yourself building doubly-linked structures, design the ownership direction up front.

- **Reading the count is for diagnostics, not control flow.** `strong_count` is great in tests and logging, but in concurrent code it can change the instant after you read it. Don't branch on it to decide whether you are the "last" owner; use the value's own logic instead.

---

## Real-World Example

A common production scenario: a parsed form/schema where many fields reuse the **same** validated field type. Instead of cloning the `FieldType` into every `Field` (wasteful) or threading borrows and lifetimes through the whole structure (awkward when fields are built dynamically), you share one canonical `FieldType` via `Rc`. Every field is an independent owner of the shared type; the type is freed only when the last field referencing it is gone.

```rust
use std::rc::Rc;

/// A reusable field definition that several form sections share.
#[derive(Debug)]
struct FieldType {
    name: String,
    max_len: usize,
}

#[derive(Debug)]
struct Field {
    label: String,
    ty: Rc<FieldType>, // shared, not owned exclusively
}

fn main() {
    // One canonical "email" type, shared by every field that is an email.
    let email_type = Rc::new(FieldType {
        name: String::from("email"),
        max_len: 254,
    });

    let fields = vec![
        Field { label: String::from("Primary email"), ty: Rc::clone(&email_type) },
        Field { label: String::from("Billing email"), ty: Rc::clone(&email_type) },
        Field { label: String::from("Backup email"),  ty: Rc::clone(&email_type) },
    ];

    for field in &fields {
        println!(
            "{:<14} -> type '{}' (max {} chars)",
            field.label, field.ty.name, field.ty.max_len
        );
    }

    // 3 fields each hold a handle + the original `email_type` binding = 4.
    println!("shared FieldType strong_count = {}", Rc::strong_count(&email_type));
}
```

**Output:**

```text
Primary email  -> type 'email' (max 254 chars)
Billing email  -> type 'email' (max 254 chars)
Backup email   -> type 'email' (max 254 chars)
shared FieldType strong_count = 4
```

**Why this is idiomatic:**

- The `FieldType` is allocated **once**. Three `Field`s and the original binding share it; no string is deep-copied per field.
- Each `Field` is a real, independent owner — a field can be moved into another collection, stored, or dropped without invalidating the others, because the `Rc` keeps the shared type alive until the last owner is gone.
- `strong_count` is `4` because there are four live owners: the original `email_type` plus one inside each of the three fields. When `fields` and `email_type` go out of scope, the count walks down to `0` and the `FieldType` is freed — deterministically, no GC.
- If this were a read-heavy server reused across requests/threads, you would change `Rc` to `Arc` and the rest of the code would stay the same.

---

## Further Reading

### Official Documentation

- [The Rust Book — `Rc<T>`, the Reference Counted Smart Pointer](https://doc.rust-lang.org/book/ch15-04-rc.html)
- [The Rust Book — Reference Cycles Can Leak Memory](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html) (`Weak<T>`)
- [The Rust Book — Shared-State Concurrency](https://doc.rust-lang.org/book/ch16-03-shared-state.html) (`Arc<Mutex<T>>`)
- [`std::rc::Rc`](https://doc.rust-lang.org/std/rc/struct.Rc.html) and [`std::sync::Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html) — API reference.

### Related Sections in This Guide

- [The Three Ownership Rules](/05-ownership/01-ownership-rules/): single ownership, the rule `Rc`/`Arc` deliberately relaxes.
- [Borrowing](/05-ownership/02-borrowing/) — try a borrow before reaching for shared ownership.
- [Mutable References](/05-ownership/03-mutable-references/): the shared-XOR-mutable rule that makes `Rc` read-only.
- [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/) — why a *deep* clone differs from an `Rc::clone` count bump.
- [Lifetimes](/05-ownership/04-lifetimes/): the borrow-based alternative to shared ownership.
- [The Drop Trait and RAII](/05-ownership/08-drop-trait/) — what runs when the last `Rc` count hits zero.
- [Section 10: Smart Pointers](/10-smart-pointers/). The full treatment: `Box`, `RefCell`, `Weak`, breaking cycles, `Deref`.
- [Section 06: Data Structures](/06-data-structures/) — building structs and enums that hold shared data.
- [Variables and Mutability](/02-basics/00-variables/) — immutability by default, the backdrop to "shared = immutable".

---

## Exercises

### Exercise 1

**Difficulty:** Easy

**Objective:** Create multiple owners of one value and observe the strong count.

**Instructions:** Wrap a `Palette` (with a `name: String` and `colors: Vec<String>`) in an `Rc`. Make two additional owners called `header` and `footer` with `Rc::clone`. Print the palette name from each handle, print the colors once, and finally print the strong count (it should be `3`).

```rust
use std::rc::Rc;

#[derive(Debug)]
struct Palette {
    name: String,
    colors: Vec<String>,
}

fn main() {
    let palette = Rc::new(Palette {
        name: String::from("ocean"),
        colors: vec![String::from("#012"), String::from("#089")],
    });
    // TODO: make `header` and `footer` owners, print names, colors, and the count
}
```

<details>
<summary>Solution</summary>

```rust
use std::rc::Rc;

#[derive(Debug)]
struct Palette {
    name: String,
    colors: Vec<String>,
}

fn main() {
    let palette = Rc::new(Palette {
        name: String::from("ocean"),
        colors: vec![String::from("#012"), String::from("#089")],
    });

    let header = Rc::clone(&palette);
    let footer = Rc::clone(&palette);

    println!("header uses palette '{}'", header.name);
    println!("footer uses palette '{}'", footer.name);
    println!("colors: {:?}", palette.colors);
    println!("strong_count = {}", Rc::strong_count(&palette));
}
```

Output:

```text
header uses palette 'ocean'
footer uses palette 'ocean'
colors: ["#012", "#089"]
strong_count = 3
```

There are three owners: the original `palette` plus the two clones. None of them deep-copied the `Palette`; they all point at the same heap allocation.

</details>

### Exercise 2

**Difficulty:** Medium

**Objective:** Predict how the strong count changes as owners are dropped early with `drop`.

**Instructions:** Create an `Rc<Vec<i32>>`, clone it twice into `a` and `b`, then `drop(a)` and `drop(b)` one at a time. **Before running it, write down** the count after each step. Then print the count at each step to confirm, and show the data is still usable after both clones are dropped.

```rust
use std::rc::Rc;

fn main() {
    let data = Rc::new(vec![1, 2, 3]);
    let a = Rc::clone(&data);
    let b = Rc::clone(&data);
    // TODO: print the count, drop(a), print, drop(b), print, then use `data`
}
```

<details>
<summary>Solution</summary>

Predicted counts: `3` after two clones, `2` after `drop(a)`, `1` after `drop(b)`.

```rust
use std::rc::Rc;

fn main() {
    let data = Rc::new(vec![1, 2, 3]);   // count = 1
    let a = Rc::clone(&data);            // count = 2
    let b = Rc::clone(&data);            // count = 3
    println!("after two clones: {}", Rc::strong_count(&data));

    drop(a);                             // count = 2
    println!("after drop(a):    {}", Rc::strong_count(&data));

    drop(b);                             // count = 1
    println!("after drop(b):    {}", Rc::strong_count(&data));

    println!("data still usable: {:?}", data);
}
```

Output:

```text
after two clones: 3
after drop(a):    2
after drop(b):    1
data still usable: [1, 2, 3]
```

`drop` (from [the Drop trait page](/05-ownership/08-drop-trait/)) decrements the strong count. Because `data` itself is still in scope, the count never reaches `0` and the `Vec` stays alive and usable.

</details>

### Exercise 3

**Difficulty:** Medium/Hard

**Objective:** Share a *mutable* counter across threads with `Arc<Mutex<T>>`.

**Instructions:** Spawn 5 threads that each increment a shared `u32` counter by `1`. Use `Arc` to share ownership across threads and `Mutex` to make the mutation safe. `join` all threads, then print the final count (it must be `5`) and the strong count after the threads finish (it should be `1`).

> **Hint:** Clone the `Arc` once per thread *before* the `move` closure captures it. Lock with `counter.lock().unwrap()` to get a mutable guard, then `*guard += 1`.

<details>
<summary>Solution</summary>

```rust
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    let counter = Arc::new(Mutex::new(0u32));
    let mut handles = Vec::new();

    for _ in 0..5 {
        let counter = Arc::clone(&counter); // each thread gets its own owner
        handles.push(thread::spawn(move || {
            let mut guard = counter.lock().unwrap(); // lock for safe mutation
            *guard += 1;
        })); // guard dropped here -> lock released
    }

    for h in handles {
        h.join().unwrap();
    }

    println!("final count = {}", *counter.lock().unwrap());
    println!("strong_count = {}", Arc::strong_count(&counter));
}
```

Output:

```text
final count = 5
strong_count = 1
```

`Arc` provides shared *ownership* across threads; `Mutex` provides shared *mutability* by handing out one exclusive guard at a time. After all threads `join`, every per-thread `Arc` clone has been dropped, so the count is back to `1`. Swapping `Arc` for `Rc` here would not compile: `Rc` is not `Send`. The `Mutex`/`Arc` combination is explored further in [Section 26: Systems Programming](/26-systems-programming/).

</details>
