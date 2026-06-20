---
title: "Unsafe Blocks and Operations"
description: "The mechanics of unsafe in Rust: writing an unsafe block, dereferencing raw pointers, calling unsafe functions, and why static mut is now a compile error."
---

The `unsafe` keyword marks the small regions of code where you, the programmer, take over a few safety guarantees the compiler can no longer check for you. This page covers the mechanics: writing an `unsafe` block, the operations that *require* one, dereferencing raw pointers, calling `unsafe` functions, and why the once-common `static mut` is now a compile error by default.

---

## Quick Overview

Most Rust is *safe* Rust: the borrow checker and type system prove your program has no data races, no use-after-free, and no out-of-bounds access. A handful of operations cannot be proven safe by the compiler (dereferencing a raw pointer, calling into C, touching mutable global state), so Rust makes you wrap them in an `unsafe { ... }` block. That block is a promise: *"I have personally verified the safety rules the compiler can't check here."*

For a TypeScript/JavaScript developer the closest mental analogy is the `any` type or a `// @ts-expect-error` comment: a deliberate, localized opt-out of the type checker. But the analogy breaks down quickly: `any` silently spreads through your codebase and the consequences are runtime `TypeError`s; `unsafe` is a sharply-scoped block whose consequences, if you get them wrong, are *undefined behavior* — memory corruption, not a catchable exception. This page is about using that opt-out correctly and as rarely as possible.

> **Note:** This file covers `unsafe` blocks and the operations inside them. *What* the five "unsafe superpowers" are and what undefined behavior means conceptually live in [What `unsafe` Really Means (and What It Does Not)](/20-unsafe-ffi/00-unsafe-intro/); raw pointer types in depth are in [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/); and the discipline of wrapping `unsafe` in safe APIs is in [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).

---

## TypeScript/JavaScript Example

JavaScript has no concept of memory unsafety — the engine and garbage collector own all of that. The nearest thing a TypeScript developer does *deliberately* is reach past the type checker when they "know better" than the compiler. Here is a realistic case: pulling a typed value out of a buffer and casting away the checks.

```typescript
// parse.ts — reading a little-endian u32 out of a byte buffer
function readU32LE(buf: Uint8Array, offset: number): number {
  // We "know" offset is in range... but nothing enforces it.
  const view = new DataView(buf.buffer);
  return view.getUint32(offset, /* littleEndian */ true);
}

// Casting away the type checker — the TS equivalent of "trust me":
function asUser(value: unknown): { name: string } {
  return value as { name: string }; // no runtime check happens
}

const bytes = new Uint8Array([0xde, 0xad, 0xbe, 0xef]);
console.log(readU32LE(bytes, 0)); // 4022250974
console.log(readU32LE(bytes, 100)); // RangeError thrown at RUNTIME
const u = asUser({ name: "Bob" });
console.log(u.name); // 'Bob'
```

Two things are worth noticing, because they are exactly what Rust changes:

- `readU32LE(bytes, 100)` throws a `RangeError` at runtime. JavaScript's `DataView` still bounds-checks for you: the worst case is an *exception*, never silent memory corruption. The VM is your safety net.
- `value as { name: string }` is a pure compile-time lie. At runtime the cast does nothing; if `value` is actually a number, the `TypeError` surfaces later, somewhere else, when you touch `.name`.

In Rust there is no VM and no garbage collector underneath you. The "trust me" cases still exist, but the consequences are different, and Rust forces you to *name* the danger with `unsafe`.

---

## Rust Equivalent

Here is the same little-endian read in Rust. Notice that the bounds check lives in **safe** code, and only the actual pointer read sits inside an `unsafe` block.

```rust playground
use std::ptr;

/// Read a little-endian `u32` out of a byte slice at `offset`.
/// Returns `None` if the read would run off the end — checked in SAFE code.
fn read_u32_le(buf: &[u8], offset: usize) -> Option<u32> {
    // Overflow-safe bounds check: `offset + 4` could itself overflow for a
    // pathological `offset`, so we subtract instead of add.
    if offset > buf.len() || buf.len() - offset < 4 {
        return None; // bounds checked here, in safe Rust
    }
    // SAFETY: the overflow-safe check above proved `offset..offset + 4` is in
    // bounds, and `read_unaligned` tolerates any alignment, so this read is sound.
    let value = unsafe {
        let p = buf.as_ptr().add(offset) as *const u32;
        ptr::read_unaligned(p)
    };
    Some(u32::from_le(value))
}

fn main() {
    let bytes = [0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x00, 0x00, 0x00];
    println!("read_u32_le(0) -> {:#010X}", read_u32_le(&bytes, 0).unwrap());
    println!("read_u32_le(4) -> {}", read_u32_le(&bytes, 4).unwrap());
    println!("read_u32_le(6) -> {:?}", read_u32_le(&bytes, 6)); // None, not a panic
}
```

Real output:

```text
read_u32_le(0) -> 0xEFBEADDE
read_u32_le(4) -> 1
read_u32_le(6) -> None
```

The shape to internalize: **the `unsafe` block is tiny**, it is surrounded by ordinary safe code that establishes the conditions the block relies on, and every `unsafe` block carries a `// SAFETY:` comment explaining *why* it is sound. That convention, a `SAFETY` comment for every `unsafe` block, is the single most important habit in this entire section, and it is enforced by `clippy::undocumented_unsafe_blocks` in many real codebases.

> **Note:** The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. Several behaviors on this page (`static mut` references being a hard error, `unsafe extern` blocks, and `unsafe fn` bodies no longer being implicitly unsafe) are 2024-edition defaults. Where they differ from older Rust, this page says so.

---

## Detailed Explanation

### What `unsafe` actually does (and does *not*) do

Writing `unsafe { ... }` does exactly **one** thing: it allows the five "unsafe operations" inside that block. It does **not** turn off the borrow checker, it does **not** disable lifetimes, and it does **not** let you mutate a value through a `&T` shared reference. Everything that is checked in safe Rust is still checked inside an `unsafe` block. (This is the single most common misconception, so it gets its own dedicated treatment in [What `unsafe` Really Means (and What It Does Not)](/20-unsafe-ffi/00-unsafe-intro/).)

The five operations `unsafe` enables are:

1. Dereference a raw pointer (`*const T` / `*mut T`).
2. Call an `unsafe` function or method (including foreign/FFI functions).
3. Access or modify a mutable `static`.
4. Implement an `unsafe` trait.
5. Access fields of a `union`.

This page focuses on the first three, which are the ones you hit while writing day-to-day systems code.

### Operation 1: dereferencing a raw pointer

A raw pointer is created in safe code, but reading or writing through it requires `unsafe`:

```rust playground
fn main() {
    let mut num = 5;

    // Creating raw pointers is SAFE — nothing is read or written yet.
    let r1 = &num as *const i32; // a *const i32 (read-only raw pointer)
    let r2 = &mut num as *mut i32; // a *mut i32 (writable raw pointer)

    // Dereferencing them is UNSAFE and must live in an unsafe block.
    unsafe {
        println!("r1 reads: {}", *r1);
        *r2 = 10;
        println!("r2 wrote, num is now: {}", *r2);
    }
}
```

Real output:

```text
r1 reads: 5
r2 wrote, num is now: 10
```

Notice the asymmetry: *making* a raw pointer is harmless, because a pointer is just a number. *Using* one is where the danger lives — the pointer might be null, dangling, unaligned, or aliasing another mutable reference, and the compiler can no longer prove otherwise. That is why the deref, not the cast, requires `unsafe`. The full anatomy of `*const T` vs `*mut T` and how they differ from references is in [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/).

### Operation 2: calling an `unsafe` function

A function declared `unsafe fn` has *preconditions* its caller must satisfy: invariants the compiler can't verify. Calling it therefore requires `unsafe`:

```rust playground
/// SAFETY: `p` must be non-null, properly aligned, and point to an
/// initialized `i32` valid for reads for the duration of the call.
unsafe fn read_i32(p: *const i32) -> i32 {
    // Edition 2024: the body of an `unsafe fn` is NOT automatically an unsafe
    // block, so the deref still needs its own `unsafe { }`.
    unsafe { *p }
}

fn main() {
    let x = 99;
    // The caller takes responsibility by wrapping the call in `unsafe`.
    let v = unsafe { read_i32(&x as *const i32) };
    println!("read_i32 -> {}", v);
}
```

Real output:

```text
read_i32 -> 99
```

The `unsafe` keyword on `fn read_i32` and the `unsafe` keyword on the *call site* mean two different things. On the function it means "I have preconditions; read my `SAFETY` docs before calling." At the call site it means "I have read them and I am upholding them." Standard-library functions like `slice::get_unchecked`, `Vec::set_len`, `str::from_utf8_unchecked`, and every FFI function are `unsafe fn`s with documented preconditions.

> **Note:** In edition 2024 the body of an `unsafe fn` is *safe by default* — you must still write inner `unsafe` blocks for the unsafe operations it performs. This is the `unsafe_op_in_unsafe_fn` lint, warned-on by default. In older editions the entire body was implicitly unsafe, which made it too easy to do something dangerous without noticing.

### Operation 3: accessing a mutable `static`

A `static mut` is a mutable global. Reading or writing it requires `unsafe`, because any other thread could be touching it at the same time and the compiler can't rule out a data race:

```rust
static mut COUNTER: u32 = 0;

fn add_to_count(inc: u32) {
    unsafe {
        COUNTER += inc; // unsafe: writing a mutable global
    }
}
```

This compiles, but as you'll see below, *reading* it in the modern way is now a hard error, and you almost never want a `static mut` at all. We get to the safe replacement (`Atomic*` / `OnceLock`) in [Best Practices](#best-practices).

### Why scope matters

`unsafe` is a *block*, deliberately. The smaller the block, the smaller the surface area a human reviewer has to audit. A 400-line function with `unsafe` sprinkled throughout is unreviewable; a function that is 95% safe code with three carefully-commented two-line `unsafe` blocks is auditable. The compiler's job ends at the `unsafe` boundary. From there it is on you and your reviewers, so you make the boundary as small as you can.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| Opt out of checks | `any`, `value as T`, `// @ts-expect-error` | `unsafe { ... }` block |
| Granularity | Spreads silently through inference | Sharply scoped to one block |
| Worst-case failure | Runtime `TypeError` / `RangeError` (catchable) | Undefined behavior (memory corruption, not catchable) |
| Safety net underneath | VM + garbage collector always present | None: you are the safety net |
| Out-of-bounds read | `DataView` throws; arrays return `undefined` | `get_unchecked` is UB; `[i]` panics; `.get(i)` returns `Option` |
| Mutable globals | Any `let` at module scope; fully allowed | `static mut`: `unsafe` to touch, and references to it are an error |
| Marking the danger | Optional comment, easy to forget | `unsafe` keyword is mandatory and greppable |

The deepest difference is **what failure looks like**. In JavaScript, the worst thing a bad cast does is throw later. In Rust, a bad `unsafe` block can corrupt memory that some unrelated part of the program reads minutes later, producing a bug with no stack trace pointing back to the cause. That is precisely why Rust makes you write the word `unsafe`: it is a flag for reviewers and a marker for `grep`, narrowing the place to look when memory corruption ever does occur.

A second difference TypeScript developers find surprising: **`unsafe` does not turn off the borrow checker.** You cannot use `unsafe` to mutate through a `&T`, to keep two `&mut` to the same data, or to outlive a lifetime. Those rules hold everywhere. `unsafe` only enables the five specific operations listed above; nothing more.

---

## Common Pitfalls

### Pitfall 1: forgetting the `unsafe` block entirely

A TypeScript developer's instinct is to just dereference the pointer. Rust refuses:

```rust
fn main() {
    let num = 5;
    let r = &num as *const i32;
    println!("{}", *r); // does not compile (error[E0133])
}
```

Real compiler error:

```text
error[E0133]: dereference of raw pointer is unsafe and requires unsafe block
 --> src/main.rs:4:20
  |
4 |     println!("{}", *r);
  |                    ^^ dereference of raw pointer
  |
  = note: raw pointers may be null, dangling or unaligned; they can violate aliasing rules and cause data races: all of these are undefined behavior
```

The fix is to wrap the deref: `let value = unsafe { *r };`. The error is genuinely helpful: it lists exactly why the operation is unsafe.

### Pitfall 2: calling an `unsafe fn` like a normal one

```rust
unsafe fn dangerous() {}

fn main() {
    dangerous(); // does not compile (error[E0133])
}
```

Real compiler error:

```text
error[E0133]: call to unsafe function `dangerous` is unsafe and requires unsafe block
 --> src/main.rs:4:5
  |
4 |     dangerous();
  |     ^^^^^^^^^^^ call to unsafe function
  |
  = note: consult the function's documentation for information on how to avoid undefined behavior
```

### Pitfall 3: assuming `unsafe` "just works" everywhere — the `unused_unsafe` warning

The opposite mistake is wrapping perfectly safe code in `unsafe` "to be safe", which does nothing useful and the compiler flags it:

```rust playground
fn main() {
    let x = 5;
    let y = unsafe { x + 1 }; // there is nothing unsafe here
    println!("{}", y);
}
```

Real compiler warning:

```text
warning: unnecessary `unsafe` block
 --> src/main.rs:3:13
  |
3 |     let y = unsafe { x + 1 };
  |             ^^^^^^ unnecessary `unsafe` block
  |
  = note: `#[warn(unused_unsafe)]` on by default
```

Treat this as an instruction to shrink your `unsafe` block until it contains *only* the operation that actually needs it.

### Pitfall 4: a raw pointer to a temporary — compiles, then corrupts

This is the trap that catches everyone coming from a garbage-collected language. The pointer's pointee can be dropped while the pointer lives on, and **the code still compiles**:

```rust playground
fn main() {
    // `5 + 5` is a temporary that is dropped at the end of THIS statement.
    let dangling: *const i32 = &(5 + 5) as *const i32;
    // Dereferencing `dangling` later would be undefined behavior:
    // the storage it points at is gone. We deliberately do NOT deref it.
    println!("got a raw pointer: {:p}", dangling);
}
```

Real output (the address is non-deterministic; the *point* is that it compiled at all):

```text
got a raw pointer: 0x16d4cba98
```

There is no compiler error here, and no exception at runtime: dereferencing `dangling` would simply be undefined behavior. In JavaScript the garbage collector keeps the value alive as long as a reference exists; raw pointers in Rust carry *no* lifetime, so the borrow checker cannot save you. This is the core reason to prefer references (`&T`/`&mut T`), which *do* carry lifetimes, and to reach for raw pointers only when you truly cannot use a reference. See [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/).

### Pitfall 5: `static mut` references are now a hard error

This used to be the "obvious" way to read a mutable global. In edition 2024 it does not compile:

```rust playground edition="2021"
static mut COUNTER: u32 = 0;

fn main() {
    unsafe {
        COUNTER += 3;
        println!("COUNTER: {}", COUNTER); // does not compile (error: static_mut_refs)
    }
}
```

Real compiler error:

```text
error: creating a shared reference to mutable static
 --> src/main.rs:6:33
  |
6 |         println!("COUNTER: {}", COUNTER);
  |                                 ^^^^^^^ shared reference to mutable static
  |
  = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/static-mut-references.html>
  = note: shared references to mutable statics are dangerous; it's undefined behavior if the static is mutated or if a mutable reference is created for it while the shared reference lives
  = note: `#[deny(static_mut_refs)]` on by default
```

The `println!` implicitly takes `&COUNTER`, and a `&` to a `static mut` is now denied because nothing prevents another thread from mutating `COUNTER` while that reference is alive — a textbook data race. (The bare `COUNTER += 3` compiles because a compound assignment operates on the place directly without forming a borrow; it is the *reference* that is rejected.) The lesson is not "how do I silence this." It is "stop using `static mut`." The safe replacement is below.

---

## Best Practices

### Replace `static mut` with `Atomic*` or `OnceLock`

For a mutable global counter, the safe, race-free, no-`unsafe`-needed answer is an atomic:

```rust playground
use std::sync::atomic::{AtomicU32, Ordering};

// A `static` (not `static mut`) holding an atomic. Safe to share across threads.
static COUNTER: AtomicU32 = AtomicU32::new(0);

fn add_to_count(inc: u32) {
    COUNTER.fetch_add(inc, Ordering::Relaxed);
}

fn main() {
    add_to_count(3);
    add_to_count(4);
    println!("COUNTER: {}", COUNTER.load(Ordering::Relaxed));
}
```

Real output:

```text
COUNTER: 7
```

No `unsafe` anywhere, and it is correct even if `add_to_count` is called from many threads at once. For one-time global initialization (a config loaded once, a regex compiled once) use `std::sync::OnceLock` instead; for interior-mutable globals behind a lock, a `static` `Mutex` works. Reach for `static mut` essentially never in application code.

### If you genuinely must touch a mutable static, use `&raw`

In the rare low-level case where a real mutable static is unavoidable (some FFI scenarios), form a raw pointer *without* going through a reference, using the `&raw const` / `&raw mut` operators. They sidestep the `static_mut_refs` error because no reference is ever created:

```rust playground
static mut COUNTER: u32 = 0;

fn add_to_count(inc: u32) {
    unsafe {
        COUNTER += inc;
    }
}

fn main() {
    add_to_count(3);
    add_to_count(4);
    // `&raw const COUNTER` is a *const u32 created without an intermediate &.
    // SAFETY: single-threaded here, so no concurrent mutation can occur.
    let value = unsafe { *(&raw const COUNTER) };
    println!("COUNTER: {}", value);
}
```

Real output:

```text
COUNTER: 7
```

This compiles and runs, but the `SAFETY` comment is doing real work: it is only sound because this program is single-threaded. In a multi-threaded program you would have a data race, which is undefined behavior. This is why the atomic above is the right default.

### Keep `unsafe` blocks minimal and always comment them

- One `unsafe` block per logical unsafe operation, as small as possible.
- Every `unsafe` block gets a `// SAFETY:` comment stating *which invariants make it sound*. Enable `#![warn(clippy::undocumented_unsafe_blocks)]` to enforce it.
- Validate preconditions (bounds, non-null, alignment) in *safe* code right before the block, so the block's `SAFETY` comment can point at that validation.

### Prefer the safe API; drop to `unsafe` only when measured

Almost every `unsafe` operation has a safe counterpart: `slice[i]` (panics on OOB) vs `slice.get_unchecked(i)` (UB on OOB); `String::from_utf8` (validates) vs `from_utf8_unchecked` (trusts you). Use the safe one until a profiler proves the check is a real bottleneck; see [Performance](/21-performance/). The unchecked variants buy you the *elimination of a bounds check*, which matters only in genuinely hot loops.

### Confine `unsafe` behind a safe boundary

The idiomatic pattern is *unsafe inside, safe outside*: a module performs the unsafe operation internally but exposes only a safe API whose preconditions are guaranteed by its own logic. The `read_u32_le` function at the top of this page is a tiny example: its `unsafe` is invisible to callers. This pattern is important enough to have its own page: [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).

---

## Real-World Example

A classic, genuinely-useful `unsafe` abstraction is splitting one mutable slice into two non-overlapping halves. The borrow checker cannot prove the halves don't overlap (it sees two `&mut` derived from the same slice and refuses), so the standard library's own `split_at_mut` uses `unsafe` internally, and exposes a completely safe API. Here is the same technique, written out so you can see the `unsafe` and the `SAFETY` reasoning:

```rust playground
/// Split `values` into two non-overlapping mutable slices at index `mid`.
/// Fully safe to call: the function upholds all invariants internally.
fn split_at_mut(values: &mut [i32], mid: usize) -> (&mut [i32], &mut [i32]) {
    let len = values.len();
    let ptr = values.as_mut_ptr();
    assert!(mid <= len); // checked in SAFE code; panics rather than corrupts

    // SAFETY: `mid <= len` was asserted, so `0..mid` and `mid..len` are both
    // in bounds, and they do not overlap. Therefore the two `&mut [i32]` we
    // hand out never alias, which is exactly what the borrow checker requires.
    unsafe {
        (
            std::slice::from_raw_parts_mut(ptr, mid),
            std::slice::from_raw_parts_mut(ptr.add(mid), len - mid),
        )
    }
}

fn main() {
    let mut data = [1, 2, 3, 4, 5, 6];
    let (left, right) = split_at_mut(&mut data, 3);

    // Both halves are independently mutable, simultaneously — safe to callers.
    left[0] = 100;
    right[0] = 200;

    println!("left = {:?}, right = {:?}", left, right);
    println!("data = {:?}", data);
}
```

Real output:

```text
left = [100, 2, 3], right = [200, 5, 6]
data = [100, 2, 3, 200, 5, 6]
```

Why this is the textbook example:

- The `assert!(mid <= len)` lives in safe code and turns a would-be undefined-behavior bug (an out-of-bounds split) into a clean, deterministic panic.
- The `unsafe` block is three lines and carries a `SAFETY` comment that points at exactly the fact (`mid <= len`, non-overlapping ranges) that makes it sound.
- Callers never write `unsafe`. They get two simultaneously-mutable halves that safe Rust alone cannot express. In production you would simply call the standard library's `values.split_at_mut(mid)`, which is implemented with precisely this technique.

This is the entire philosophy of `unsafe` in one function: a small, audited, well-commented core of `unsafe`, wrapped in a safe API that makes misuse impossible. Building such wrappers deliberately is the subject of [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).

---

## Further Reading

- [The Rustonomicon — the dark arts of unsafe Rust](https://doc.rust-lang.org/nomicon/) — the canonical reference for everything `unsafe`.
- [The Rust Book, ch. 20: Unsafe Rust](https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html) — the five superpowers, with examples.
- [Rust 2024 edition — `static mut` references](https://doc.rust-lang.org/edition-guide/rust-2024/static-mut-references.html) — why the lint became a hard error.
- [Rust 2024 edition — `unsafe_op_in_unsafe_fn`](https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html) — why `unsafe fn` bodies are now safe by default.
- [`std::sync::atomic`](https://doc.rust-lang.org/std/sync/atomic/) and [`std::sync::OnceLock`](https://doc.rust-lang.org/std/sync/struct.OnceLock.html) — the safe replacements for `static mut`.
- Cross-links within this section: [What `unsafe` Really Means (and What It Does Not)](/20-unsafe-ffi/00-unsafe-intro/) (what `unsafe` is and is NOT), [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/) (`*const T` / `*mut T` in depth), [FFI Basics](/20-unsafe-ffi/03-ffi-basics/) and [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/) (FFI, the most common reason to call `unsafe` functions), [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/) (wrapping `unsafe` safely), [When `unsafe` and FFI Are Actually Necessary (and the Many Times They Are Not)](/20-unsafe-ffi/09-when-to-use/) (when you actually need any of this).
- Foundations: [05-ownership](/05-ownership/) (borrowing and lifetimes, the rules `unsafe` does *not* turn off), [10-smart-pointers](/10-smart-pointers/) (safe alternatives built on `unsafe` internally), and the basics in [Variables and Mutability](/02-basics/00-variables/) (`mut`, statics, and constants).
- When you reach for `unchecked` variants for speed: [Performance](/21-performance/).

---

## Exercises

### Exercise 1: Spot the missing `unsafe`

**Difficulty:** Easy

**Objective:** Recognize which operations require an `unsafe` block and fix the resulting compiler error.

**Instructions:** The function below tries to write a value through a raw pointer. Predict whether it compiles, then fix it so it does. Verify in a `cargo new` project with `cargo check`.

```rust
fn set_to_42(p: *mut i32) {
    *p = 42; // problem here
}

fn main() {
    let mut x = 0;
    set_to_42(&mut x as *mut i32);
    println!("{}", x);
}
```

<details>
<summary>Solution</summary>

It does **not** compile: dereferencing (and writing through) a raw pointer is an unsafe operation, so the compiler rejects it with `error[E0133]: dereference of raw pointer is unsafe and requires unsafe block`. Wrap the write in an `unsafe` block — and document the precondition the caller must uphold:

```rust playground
/// SAFETY: `p` must be non-null, properly aligned, and valid for writes.
fn set_to_42(p: *mut i32) {
    // SAFETY: callers promise `p` points to a writable, aligned i32.
    unsafe {
        *p = 42;
    }
}

fn main() {
    let mut x = 0;
    // SAFETY: `&mut x` is non-null, aligned, and valid for the call's duration.
    set_to_42(&mut x as *mut i32);
    println!("{}", x); // prints 42
}
```

Because this function has a real precondition the compiler cannot check, a stricter design would mark the *function itself* `unsafe fn` and push the responsibility to the caller. For an internal helper with a guaranteed-valid pointer, the safe wrapper above is fine.

</details>

### Exercise 2: Kill the `static mut`

**Difficulty:** Medium

**Objective:** Replace a `static mut` global with a thread-safe, `unsafe`-free alternative.

**Instructions:** A teammate wrote a request counter using `static mut`. It increments from multiple worker threads. It does not compile cleanly on the 2024 edition (and even if it did, it would be a data race). Rewrite it with no `unsafe` so it is correct under concurrency.

```rust
static mut REQUESTS: usize = 0;

fn record_request() {
    unsafe {
        REQUESTS += 1;
    }
}

fn current() -> usize {
    unsafe { REQUESTS } // reference to a static mut: rejected on edition 2024
}
```

<details>
<summary>Solution</summary>

Use an `AtomicUsize` in a plain `static`. No `unsafe`, no `static mut`, and correct across threads:

```rust playground
use std::sync::atomic::{AtomicUsize, Ordering};

static REQUESTS: AtomicUsize = AtomicUsize::new(0);

fn record_request() -> usize {
    // fetch_add returns the PREVIOUS value; +1 gives the count including this call.
    REQUESTS.fetch_add(1, Ordering::Relaxed) + 1
}

fn current() -> usize {
    REQUESTS.load(Ordering::Relaxed)
}

fn main() {
    println!("req #{}", record_request());
    println!("req #{}", record_request());
    println!("total seen: {}", current());
}
```

Real output:

```text
req #1
req #2
total seen: 2
```

`Ordering::Relaxed` is correct here because the counter has no happens-before relationship with other data; if the count gated access to *other* memory you would reach for `Acquire`/`Release`. Either way, this is race-free by construction and requires no `unsafe` block at all — which is exactly why `static mut` should be your last resort.

</details>

### Exercise 3: A safe "first and rest" splitter

**Difficulty:** Hard

**Objective:** Write a safe API over an `unsafe` core, upholding the no-aliasing invariant yourself.

**Instructions:** Implement `first_and_rest(buf: &mut [u8]) -> Option<(&mut u8, &mut [u8])>` that returns a mutable reference to the first byte and a mutable slice of everything after it — two non-overlapping mutable borrows from the same buffer. The borrow checker will reject the naive safe version, so use a single, well-commented `unsafe` block. Return `None` for an empty slice. Verify it compiles and runs, and that `cargo clippy` is clean.

<details>
<summary>Solution</summary>

```rust playground
fn first_and_rest(buf: &mut [u8]) -> Option<(&mut u8, &mut [u8])> {
    if buf.is_empty() {
        return None; // checked in safe code
    }
    let ptr = buf.as_mut_ptr();
    let len = buf.len();
    // SAFETY: `buf` is non-empty, so index 0 is valid and `ptr.add(1)` with
    // length `len - 1` covers the remaining region. The head (one byte at 0)
    // and the tail (bytes 1..len) do not overlap, so the two mutable borrows
    // we return never alias — satisfying the borrow checker's core rule.
    unsafe {
        let head = &mut *ptr;
        let tail = std::slice::from_raw_parts_mut(ptr.add(1), len - 1);
        Some((head, tail))
    }
}

fn main() {
    let mut data = [1u8, 2, 3, 4];
    if let Some((first, rest)) = first_and_rest(&mut data) {
        *first = 100; // mutate the head
        rest[0] = 200; // mutate the tail, simultaneously
    }
    println!("data = {:?}", data);

    let mut empty: [u8; 0] = [];
    println!("empty -> {}", first_and_rest(&mut empty).is_some());
}
```

Real output:

```text
data = [100, 200, 3, 4]
empty -> false
```

The key insight is the same as the standard library's `split_at_mut`: the *function* guarantees the two regions are disjoint (one byte at index 0, the rest from index 1), so handing out two `&mut` is genuinely sound: the borrow checker simply can't see that derivation through a raw pointer. You took responsibility for the invariant, documented it in the `SAFETY` comment, and exposed a fully safe signature. That is the unsafe-inside/safe-outside pattern explored further in [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).

</details>
