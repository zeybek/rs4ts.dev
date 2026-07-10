---
title: "What `unsafe` Really Means (and What It Does Not)"
description: "Rust's unsafe is not TypeScript's any. It removes zero checks and the borrow checker stays on; it adds exactly five abilities you must uphold yourself."
---

`unsafe` is the most misunderstood keyword in Rust. Coming from TypeScript, it is tempting to read it as "the `any` of Rust", a switch that turns off the compiler's checks. It is not. This page builds the correct mental model: `unsafe` enables exactly **five** extra abilities, it leaves the borrow checker fully on, and it is a *promise you make to the compiler*, not a permission the compiler grants you.

---

## Quick Overview

In safe Rust, the compiler proves your program has no use-after-free, no data races, no out-of-bounds access, and no dangling pointers, before it ever runs. A handful of low-level operations (talking to C, dereferencing raw pointers, certain hardware-level tricks) cannot be proven safe by the compiler, so Rust requires you to wrap them in an `unsafe` block. Inside that block, **you** take on the obligation the compiler normally discharges: you promise that the code upholds memory safety.

For a TypeScript/JavaScript developer the key reframe is this: `unsafe` is **not** TypeScript's `any`, and it is **not** "turn off the type system." It does not silence the borrow checker, the type checker, or lifetimes. It only adds five specific superpowers on top of everything safe Rust already enforces. Misuse those powers and you get **undefined behavior** (UB): the C-style hazard Rust otherwise eliminates, where the program may crash, corrupt data, or appear to work until it does not.

---

## TypeScript/JavaScript Example

JavaScript has no `unsafe` keyword because it has no equivalent danger: the runtime is garbage-collected and memory-safe by construction. There is no way to dereference a dangling pointer in JavaScript. The closest "escape hatch" a TypeScript developer reaches for is `any`, which switches off the **type checker**.

```typescript
// account.ts
interface Account {
  id: number;
  balance: number;
}

function applyBonus(account: Account): void {
  // `any` opts out of TypeScript's type checking for this value.
  const loose: any = account;

  // The compiler now lets us do nonsense it would normally reject:
  loose.blance += 100; // typo: "blance", not "balance" — NO compile error

  console.log(account); // { id: 1, balance: 0, blance: NaN } — the bonus landed in a phantom field

  loose.id.toUpperCase(); // calling a string method on a number — NO compile error
}

applyBonus({ id: 1, balance: 0 });
```

Run with Node v22 (`node --experimental-strip-types account.ts`) and the typo silently creates a `blance` property. Because `undefined += 100` is `NaN`, `console.log(account)` actually prints `{ id: 1, balance: 0, blance: NaN }`: the phantom field is fully visible, holding a `NaN`. (The later `loose.id.toUpperCase()` line then throws a catchable `TypeError`, since `id` is a number.) Note what `any` did **not** do:

- It did **not** corrupt memory. The object is still a valid object. The bonus simply landed in a visible `blance: NaN` property instead of `balance`.
- It did **not** segfault. A wrong method call throws a catchable `TypeError` at runtime, not undefined behavior.
- It only suppressed **compile-time type checking** for that one value.

This is the model many TypeScript developers carry into Rust — "a keyword that makes the compiler stop complaining." Rust's `unsafe` is a fundamentally different beast, and conflating the two is the single most dangerous misconception you can bring with you.

---

## Rust Equivalent

There is no single keyword in Rust that corresponds to `any`. The thing people *think* `unsafe` does — opt out of checking — is closest to nothing in safe Rust at all. What `unsafe` actually does is grant five abilities. Here is one tiny example of each, all in one compile-verified program.

```rust playground
// src/main.rs

// --- Superpower 4: implement an `unsafe` trait ---
// An unsafe trait carries a contract the implementer must uphold by hand.
unsafe trait AllZeroBitsValid {
    // Marker: implementer promises an all-zero byte pattern is a valid value.
}

// SAFETY: the all-zero bit pattern (0) is a perfectly valid `u32`.
unsafe impl AllZeroBitsValid for u32 {}

// --- Superpower 5: declare/call a foreign (C) function ---
unsafe extern "C" {
    fn abs(input: i32) -> i32; // from the C standard library
}

// --- Superpower 2: an `unsafe fn` is one whose caller must uphold a contract ---
/// # Safety
/// `index` must be less than `slice.len()`.
unsafe fn get_unchecked(slice: &[i32], index: usize) -> i32 {
    // In the 2024 edition the *body* of an unsafe fn is safe-by-default,
    // so the unsafe operation still needs its own `unsafe` block.
    unsafe { *slice.get_unchecked(index) }
}

// --- A union, whose field access is unsafe ---
union IntOrFloat {
    i: u32,
    f: f32,
}

fn main() {
    // Superpower 1: dereference a raw pointer.
    let x = 42;
    let p: *const i32 = &x;
    // SAFETY: `p` was just created from a live `i32`, so it is valid to read.
    let read_back = unsafe { *p };
    println!("1. deref raw pointer -> {read_back}");

    // Superpower 2: call an `unsafe fn`.
    let data = [10, 20, 30];
    // SAFETY: index 1 is in bounds for a 3-element array.
    let value = unsafe { get_unchecked(&data, 1) };
    println!("2. call unsafe fn   -> {value}");

    // Superpower 3: access (and mutate) a mutable static.
    static mut COUNTER: u32 = 0;
    // SAFETY: single-threaded here; nothing else touches COUNTER concurrently.
    unsafe {
        let c = &raw mut COUNTER; // 2024-edition idiom: take a raw pointer, not a reference
        *c += 1;
        println!("3. mutable static  -> {}", *c);
    }

    // Superpower 5 (cont.): the FFI call itself.
    // SAFETY: `abs` is a pure C function with no preconditions.
    let a = unsafe { abs(-5) };
    println!("5. call C `abs(-5)` -> {a}");

    // Reading a union field reinterprets the bytes.
    let u = IntOrFloat { i: 0x3f80_0000 };
    // SAFETY: 0x3f80_0000 is the IEEE-754 bit pattern for 1.0_f32.
    let as_float = unsafe { u.f };
    println!("   union as f32     -> {as_float}");

    // Superpower 4 (cont.): the unsafe trait we implemented above.
    fn requires<T: AllZeroBitsValid>() {}
    requires::<u32>();
    println!("4. unsafe trait     -> u32: AllZeroBitsValid");
}
```

Running it produces real output:

```text
$ cargo run
1. deref raw pointer -> 42
2. call unsafe fn   -> 20
3. mutable static  -> 1
5. call C `abs(-5)` -> 5
   union as f32     -> 1
4. unsafe trait     -> u32: AllZeroBitsValid
```

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition, which `cargo new` selects automatically. Two 2024-edition details show up above: `extern "C"` blocks are now written `unsafe extern "C"`, and you reach a `static mut` through a raw pointer with the `&raw mut` operator rather than taking an ordinary `&mut` reference. (The `union as f32 -> 1` line is not truncated: Rust's `Display` for `f32` prints `1`, not `1.0`, for a whole-number float.) The mechanics of each operation are the subject of the sibling pages — see [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/), [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/), and [FFI Basics](/20-unsafe-ffi/03-ffi-basics/). This page is about the *concept*.

---

## Detailed Explanation

### The five superpowers — the complete list

The Rust reference is precise about this: `unsafe` lets you do **five** things you cannot do in safe code, and nothing else. Memorize this list; it is the whole point of the keyword.

| # | Superpower | Why it can't be checked | Covered in depth |
| - | ---------- | ----------------------- | ---------------- |
| 1 | **Dereference a raw pointer** (`*const T` / `*mut T`) | The compiler can't prove the pointer is non-null, aligned, and points to a live value. | [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/) |
| 2 | **Call an `unsafe fn`** (including FFI functions) | The function documents a contract its caller must uphold; the compiler can't verify the caller did. | [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/) |
| 3 | **Access or modify a mutable `static`** | A `static mut` is global shared mutable state, so reads/writes can race with other threads. | [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/) |
| 4 | **Implement an `unsafe trait`** | The trait carries an invariant (e.g. `Send`/`Sync`) the compiler trusts the implementer to uphold. | [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/) |
| 5 | **Access the fields of a `union`** | Reading the "wrong" field reinterprets raw bytes, which the type system can't track. | this page (above) |

That is the entire list. There is no sixth power that "disables the borrow checker," no power that "lets you reassign immutable variables," no power that "skips bounds checks on a normal `Vec`." If you find yourself thinking `unsafe` should let you do something *not* on this list, you have misunderstood it.

### `unsafe` does NOT turn off the borrow checker

This is the most important correction in the entire section, so let's prove it. The borrow checker, the type checker, lifetimes, ownership, move semantics, and `Drop` all run **exactly the same inside an `unsafe` block as outside it.** The only difference is that the five operations above become *available*.

```rust
fn main() {
    let mut s = String::from("hello");
    let r1 = &mut s;
    unsafe {
        let r2 = &mut s; // borrow checker is STILL ON inside `unsafe`
        r1.push_str(" world");
        r2.push_str("!");
    }
}
```

This does **not** compile, and the error is the ordinary borrow-checker error you would get without any `unsafe` at all:

```text
warning: unnecessary `unsafe` block
 --> src/main.rs:4:5
  |
4 |     unsafe {
  |     ^^^^^^ unnecessary `unsafe` block
  |
  = note: `#[warn(unused_unsafe)]` on by default

error[E0499]: cannot borrow `s` as mutable more than once at a time
 --> src/main.rs:5:18
  |
3 |     let r1 = &mut s;
  |              ------ first mutable borrow occurs here
4 |     unsafe {
5 |         let r2 = &mut s; // borrow checker is STILL ON inside `unsafe`
  |                  ^^^^^^ second mutable borrow occurs here
6 |         r1.push_str(" world");
  |         -- first borrow later used here
```

Two things to notice. First, the borrow checker rejected the double mutable borrow; `unsafe` gave it no pass. Second, the compiler *also* warned the `unsafe` block was **unnecessary**: none of the five superpowers were used inside it, so the block bought you nothing. The lesson is exact: `unsafe` adds five abilities and removes zero safety checks.

### `unsafe` is a promise, not a permission

The deepest reframe for a TypeScript developer is about **who is responsible**. With `any`, you tell the TypeScript compiler "stop checking and trust me," and the worst case is a runtime `TypeError`. With `unsafe`, you tell the Rust compiler "I have personally verified that this code upholds memory safety, even though you cannot." You are co-signing the safety guarantee.

When you write `unsafe { *p }`, you are asserting: *this pointer is non-null, properly aligned, points to an initialized value of the right type, and respects Rust's aliasing rules.* If that assertion is false, the result is not a catchable exception — it is **undefined behavior**.

### Undefined behavior is categorically worse than a JavaScript exception

In JavaScript, "something went wrong" means a thrown error you can `try/catch`, or a `NaN` you can detect. **Undefined behavior** is different in kind. When a Rust program triggers UB — say, by dereferencing a pointer to freed memory — the compiler's optimizer has already assumed UB *cannot happen*. So there is no defined outcome. The program might:

- crash with a segfault,
- silently read or corrupt unrelated data,
- "work" today and break after an unrelated code change six months later,
- behave differently in debug vs. release builds.

There is no `try/catch` for UB. This is precisely the class of bug Rust's safe subset exists to eliminate, and it is why `unsafe` is a keyword you write rarely and review carefully. The list of operations that are UB (dangling deref, data races, breaking aliasing, reading uninitialized memory, etc.) is enumerated in the [Rust Reference's "Behavior considered undefined"](https://doc.rust-lang.org/reference/behavior-considered-undefined.html).

### Safety invariants: the contract behind every `unsafe`

Every unsafe operation has a **safety invariant**: a precondition that must hold for the operation to be sound. Dereferencing `*p` requires `p` to be valid. Calling `slice.get_unchecked(i)` requires `i < slice.len()`. Implementing `Send` for a type requires that the type really is safe to move across threads.

The discipline of unsafe Rust is: *state the invariant, then prove (to yourself and your reviewer) that it holds.* The community convention is a `// SAFETY:` comment on every `unsafe` block and a `/// # Safety` doc section on every public `unsafe fn`, explaining *why* the invariant is satisfied. You saw both in the Rust example above. Building a *safe* API that upholds these invariants internally — so callers never have to think about them — is the central pattern of the whole section, covered in [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).

---

## Key Differences

### `unsafe` vs TypeScript's `any` / `as` / `@ts-ignore`

| Aspect | TypeScript `any` / `as` | Rust `unsafe` |
| ------ | ----------------------- | ------------- |
| What it switches off | The **type checker**, for that value | **Nothing** — adds 5 abilities, removes 0 checks |
| Borrow/ownership rules | N/A (GC language) | Fully enforced, unchanged |
| Worst-case failure | Catchable runtime `TypeError`, `NaN`, wrong value | **Undefined behavior**: corruption, segfault, silent miscompile |
| Who is responsible | "Trust me, ignore the types" | "I have *proven* this upholds memory safety" |
| Recoverable? | Yes, with `try/catch` | No — UB has no defined behavior to catch |
| How often used | Sprinkled liberally in many codebases | Rare; isolated, documented, reviewed |
| Tooling reaction | Linters may warn | Compiler *requires* it; Clippy enforces docs; Miri can detect some UB |

### What stays exactly the same inside `unsafe`

It is worth stating the non-events explicitly, because they surprise newcomers:

- Immutable variables stay immutable (`let x = 5; x = 6;` still fails).
- Move semantics still apply (a moved-out value is still unusable).
- Lifetimes are still checked.
- Bounds checks on normal indexing (`v[i]`) still happen; only the *unchecked* methods skip them, and those are `unsafe fn`s you opt into.
- `Drop` still runs; RAII still works.

`unsafe` is a tiny, surgical extension to the language, not a different language.

> **Tip:** A useful one-liner to remember: *safe Rust is a proof that your code is sound; `unsafe` is where **you** supply the part of the proof the compiler can't.* The borrow checker is your co-author the entire time, not a switch you flip off.

---

## Common Pitfalls

### Pitfall 1: Believing `unsafe` lets you skip the borrow checker

This is the headline misconception, and the borrow-checker error in the Detailed Explanation above is the proof. New Rustaceans sometimes hit a borrow error and wrap the offending code in `unsafe`, expecting it to compile. It will not, and you will get an `unnecessary unsafe block` warning on top of the original error. **Fix:** restructure the ownership (use indices, `split_at_mut`, `Cell`/`RefCell`, or a different data layout), or, if you genuinely need raw pointers, use them deliberately as their own technique ([Raw Pointers](/20-unsafe-ffi/02-raw-pointers/)).

### Pitfall 2: Dereferencing a raw pointer outside an `unsafe` block

You can freely *create* a raw pointer in safe code; you just cannot *read through it* without `unsafe`.

```rust
fn main() {
    let x = 42;
    let p: *const i32 = &x; // creating the pointer is safe
    let value = *p; // does not compile (error[E0133])
    println!("{value}");
}
```

The compiler is explicit about both the error and *why*:

```text
error[E0133]: dereference of raw pointer is unsafe and requires unsafe block
 --> src/main.rs:4:17
  |
4 |     let value = *p; // not in unsafe block
  |                 ^^ dereference of raw pointer
  |
  = note: raw pointers may be null, dangling or unaligned; they can violate
          aliasing rules and cause data races: all of these are undefined behavior
```

**Fix:** wrap the dereference in `unsafe { *p }` *and* add a `// SAFETY:` comment justifying why `p` is valid.

### Pitfall 3: Calling an `unsafe fn` without an `unsafe` block

In the 2024 edition, even the body of an `unsafe fn` is "safe by default": you must still mark the unsafe operations inside it, and callers must still wrap the call. Forgetting the block gives an error (and, inside an unsafe fn, a lint):

```rust
unsafe fn read_unchecked(slice: &[i32], index: usize) -> i32 {
    *slice.get_unchecked(index) // unsafe op needs its own block (warn: unsafe_op_in_unsafe_fn)
}

fn main() {
    let data = [10, 20, 30];
    let value = read_unchecked(&data, 1); // does not compile (error[E0133])
    println!("{value}");
}
```

The real diagnostics:

```text
warning[E0133]: call to unsafe function `core::slice::<impl [T]>::get_unchecked` is unsafe and requires unsafe block
 --> src/main.rs:2:6
  |
2 |     *slice.get_unchecked(index) // unsafe op needs its own block (warn: unsafe_op_in_unsafe_fn)
  |      ^^^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
  |
  = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
  = note: consult the function's documentation for information on how to avoid undefined behavior
note: an unsafe function restricts its caller, but its body is safe by default
 --> src/main.rs:1:1
  |
1 | unsafe fn read_unchecked(slice: &[i32], index: usize) -> i32 {
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  = note: `#[warn(unsafe_op_in_unsafe_fn)]` on by default

error[E0133]: call to unsafe function `read_unchecked` is unsafe and requires unsafe block
 --> src/main.rs:7:17
  |
7 |     let value = read_unchecked(&data, 1); // does not compile (error[E0133])
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
  |
  = note: consult the function's documentation for information on how to avoid undefined behavior
```

**Fix:** put an `unsafe { ... }` block around the inner operation *and* around the call site, each with its own `// SAFETY:` justification; see the corrected version in [Best Practices](#best-practices).

### Pitfall 4: Forgetting a public `unsafe fn` needs a documented contract

If you expose a public `unsafe fn`, Clippy insists you document the contract callers must uphold:

```rust
pub unsafe fn get_first(slice: &[i32]) -> i32 {
    unsafe { *slice.get_unchecked(0) }
}
```

Running `cargo clippy` reports:

```text
warning: unsafe function's docs are missing a `# Safety` section
 --> src/lib.rs:1:1
  |
1 | pub unsafe fn get_first(slice: &[i32]) -> i32 {
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: `#[warn(clippy::missing_safety_doc)]` on by default
```

**Fix:** add a `/// # Safety` section stating the precondition (here: "the slice must be non-empty").

### Pitfall 5: Assuming UB will "probably just crash"

Because `unsafe` doesn't always misbehave *immediately*, it is easy to write unsound code that appears to work in testing. UB is allowed to do *anything*, including looking correct until an optimizer or an unrelated change exposes it. **Fix:** treat *soundness* (the absence of UB for all possible inputs), not "passes my tests," as the bar. Run your unsafe code under [Miri](https://github.com/rust-lang/miri) (`cargo +nightly miri test`), the interpreter that detects many forms of UB.

---

## Best Practices

- **Default to safe Rust; reach for `unsafe` only when one of the five superpowers is genuinely required.** Most application code — including the kind you would write in TypeScript — never needs it. When you think you do, read [When `unsafe` and FFI Are Actually Necessary (and the Many Times They Are Not)](/20-unsafe-ffi/09-when-to-use/) first.
- **Keep `unsafe` blocks as small as possible.** Wrap only the operation that needs the power, not the surrounding safe logic. A tight block is easier to audit and keeps the `unnecessary unsafe block` lint honest.
- **Write a `// SAFETY:` comment on every `unsafe` block** stating the invariant and why it holds, and a `/// # Safety` doc on every public `unsafe fn`. This is the corrected version of the Pitfall 3 example:

```rust playground
/// # Safety
/// `index` must be less than `slice.len()`.
unsafe fn read_unchecked(slice: &[i32], index: usize) -> i32 {
    // SAFETY: forwarded to the caller's contract — `index < slice.len()`.
    unsafe { *slice.get_unchecked(index) }
}

fn main() {
    let data = [10, 20, 30];
    // SAFETY: 1 is a valid index into a 3-element array.
    let value = unsafe { read_unchecked(&data, 1) };
    println!("value = {value}");
}
```

```text
$ cargo run
value = 20
```

- **Encapsulate `unsafe` behind a safe API.** The goal is "unsafe inside, safe outside": callers should never have to write `unsafe` themselves, because your module upholds the invariants for them. This is the standard-library pattern (e.g. `Vec`) and the subject of [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).
- **Verify with the tooling.** Use `cargo clippy` for the safety-doc lints, and run `cargo +nightly miri test` to catch UB the compiler can't. Consider `#![forbid(unsafe_code)]` at the crate root for modules that should contain none.
- **Never use `unsafe` as a borrow-checker workaround.** If you are fighting ownership, the answer is almost always a different data structure (indices, `Rc<RefCell<T>>`, an arena), not raw pointers. See [Section 05: Ownership](/05-ownership/) and [Smart Pointers](/10-smart-pointers/).

---

## Real-World Example

A classic place `unsafe` is *justified* is in a data structure where the borrow checker is too conservative, but where the **author** can prove safety. The standard library's `split_at_mut` hands out two mutable slices into the same backing array. The borrow checker rejects the naive version (two `&mut` into one buffer), yet it is perfectly sound because the two slices cover **disjoint** ranges. The implementation uses raw pointers internally and exposes a fully safe signature:

```rust playground
// src/main.rs
use std::slice;

/// Split `values` into two non-overlapping mutable halves at `mid`.
/// This is a simplified version of the standard library's `<[T]>::split_at_mut`.
fn split_at_mut(values: &mut [i32], mid: usize) -> (&mut [i32], &mut [i32]) {
    let len = values.len();
    let ptr = values.as_mut_ptr();

    // This precondition is the safety invariant; we check it up front so the
    // unsafe block below can rely on it.
    assert!(mid <= len);

    unsafe {
        // SAFETY: `mid <= len` was asserted above, so:
        //   * `[0, mid)` and `[mid, len)` are both within the allocation, and
        //   * the two ranges are disjoint, so the two `&mut [i32]` slices we
        //     create never alias. That upholds Rust's aliasing rules even
        //     though the borrow checker can't see it.
        (
            slice::from_raw_parts_mut(ptr, mid),
            slice::from_raw_parts_mut(ptr.add(mid), len - mid),
        )
    }
}

fn main() {
    let mut data = vec![1, 2, 3, 4, 5, 6];
    let (left, right) = split_at_mut(&mut data, 3);

    // Both halves are mutable *at the same time* — impossible in safe Rust
    // without this carefully-justified `unsafe` block underneath.
    for x in left.iter_mut() {
        *x *= 10;
    }
    for y in right.iter_mut() {
        *y += 100;
    }

    println!("{data:?}");
}
```

```text
$ cargo run
[10, 20, 30, 104, 105, 106]
```

Notice the shape: the `unsafe` is **tiny**, **justified by an assertion** that establishes the invariant, and **wrapped in a safe function** whose callers write no `unsafe` at all. The borrow checker still governs everything around it: `split_at_mut` returns two borrows tied to the input's lifetime, so you cannot misuse the result. This is unsafe Rust done right: a small, audited core upholding an invariant the compiler cannot, behind a safe boundary. The reverse — when a problem *looks* like it needs `unsafe` but a safe restructuring is better — is the topic of [When `unsafe` and FFI Are Actually Necessary (and the Many Times They Are Not)](/20-unsafe-ffi/09-when-to-use/).

> **Warning:** "I can prove it's safe" is a high bar. The `assert!(mid <= len)` is load-bearing: remove it, and `ptr.add(mid)` could compute a pointer past the allocation, which is undefined behavior even before any dereference. Every safety invariant in an `unsafe` block must be genuinely guaranteed, not merely likely.

---

## Further Reading

### Official documentation

- [The Rust Book — Unsafe Rust](https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html): the canonical "five superpowers" explanation
- [The Rustonomicon](https://doc.rust-lang.org/nomicon/): the book of unsafe Rust, soundness, and UB
- [Rust Reference — Behavior considered undefined](https://doc.rust-lang.org/reference/behavior-considered-undefined.html): the precise list of what UB is
- [Miri](https://github.com/rust-lang/miri): an interpreter that detects many forms of undefined behavior
- [Clippy `missing_safety_doc`](https://rust-lang.github.io/rust-clippy/master/index.html#missing_safety_doc): the lint that enforces `# Safety` docs

### Related sections in this guide

- Next: [Unsafe Rust in Practice →](/20-unsafe-ffi/01-unsafe-rust/): `unsafe` blocks, calling unsafe fns, and `static mut` dangers
- [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/): `*const T` / `*mut T` vs references
- [FFI Basics](/20-unsafe-ffi/03-ffi-basics/) and [Calling C from Rust](/20-unsafe-ffi/04-calling-c/) — superpower #5 in detail
- [Building Safe Abstractions](/20-unsafe-ffi/08-safety-abstractions/): the unsafe-inside/safe-outside pattern
- [When to Use `unsafe`/FFI](/20-unsafe-ffi/09-when-to-use/) — and the many times you should not
- Foundations: [Why Rust?](/01-getting-started/00-why-rust/), [Basics](/02-basics/), [Ownership](/05-ownership/)
- The intro to the whole series: [Section 00: Introduction](/00-introduction/)
- Going further: [Section 21: Performance](/21-performance/): where a justified `unsafe` sometimes earns its keep

---

## Exercises

### Exercise 1: Spot the misconception

**Difficulty:** Easy

**Objective:** Cement that `unsafe` does not disable the borrow checker or the type system.

**Instructions:** For each statement, decide whether it is **true** or **false** about Rust's `unsafe`, and give a one-sentence reason.

1. Wrapping code in `unsafe` lets you take two `&mut` references to the same value at once.
2. `unsafe` is Rust's version of TypeScript's `any`.
3. You can create a raw pointer (`*const T`) in safe code, but reading through it requires `unsafe`.
4. Triggering undefined behavior in Rust always crashes the program immediately.
5. There are exactly five extra abilities `unsafe` enables.

<details>
<summary>Solution</summary>

1. **False.** The borrow checker is fully active inside `unsafe`; the aliasing rule on `&mut` is enforced exactly as in safe code (you'd get `error[E0499]`).
2. **False.** `any` switches off the *type checker* with a catchable worst case; `unsafe` removes *no* checks, adds five abilities, and its worst case is undefined behavior.
3. **True.** Creating raw pointers is safe; only dereferencing them is an `unsafe` operation (`error[E0133]` otherwise).
4. **False.** UB has *no defined behavior* — it may crash, silently corrupt data, or appear to work; "immediate crash" is not guaranteed.
5. **True.** Dereference a raw pointer, call an `unsafe fn`, access/modify a mutable `static`, implement an `unsafe trait`, and access `union` fields.

</details>

### Exercise 2: Fix the `unsafe` usage

**Difficulty:** Medium

**Objective:** Practice the real edition-2024 rules for `unsafe fn` bodies and call sites, plus the `// SAFETY:` convention.

**Instructions:** The function below is meant to read a slice element without a bounds check. As written it does not compile. Explain *why*, then produce a version that compiles and follows current best practice (a `/// # Safety` doc, `unsafe` blocks where required, and a `// SAFETY:` justification at the call site). Verify with `cargo run` in a fresh `cargo new` project.

```rust
unsafe fn nth(slice: &[u32], index: usize) -> u32 {
    *slice.get_unchecked(index) // problem here
}

fn main() {
    let data = [5, 6, 7, 8];
    let value = nth(&data, 2); // and here
    println!("{value}");
}
```

<details>
<summary>Solution</summary>

Two problems. First, in the 2024 edition the body of an `unsafe fn` is "safe by default," so the call to `get_unchecked` (itself an `unsafe fn`) must be inside its own `unsafe` block; otherwise you get `warning[E0133] ... unsafe_op_in_unsafe_fn`. Second, calling `nth` (an `unsafe fn`) from `main` requires an `unsafe` block at the call site, or you get `error[E0133]: call to unsafe function ... requires unsafe block`. Best practice adds a documented contract and a safety justification:

```rust playground
/// Returns `slice[index]` without a bounds check.
///
/// # Safety
/// `index` must be less than `slice.len()`.
unsafe fn nth(slice: &[u32], index: usize) -> u32 {
    // SAFETY: upheld by this function's caller (`index < slice.len()`).
    unsafe { *slice.get_unchecked(index) }
}

fn main() {
    let data = [5, 6, 7, 8];
    // SAFETY: 2 is a valid index into a 4-element array.
    let value = unsafe { nth(&data, 2) };
    println!("{value}");
}
```

```text
$ cargo run
7
```

</details>

### Exercise 3: Justify or eliminate the `unsafe`

**Difficulty:** Hard

**Objective:** Build the judgment to recognize when `unsafe` is needed at all. Usually it is not.

**Instructions:** A teammate wants a `first()` method on a custom collection that "can never fail," and proposes using `get_unchecked(0)` in an `unsafe` block "for speed." Design a `NonEmptyVec<T>` type whose API makes `first()` infallible **without any `unsafe`**, by upholding the non-empty invariant in the type itself. Then write one or two sentences on when reaching for `unsafe` here *would* actually be justified. Verify your type compiles and runs.

<details>
<summary>Solution</summary>

The non-empty invariant can be enforced entirely by the constructor and the absence of any operation that removes the last element. With the invariant guaranteed at the API boundary, indexing `[0]` is provably in bounds, so `first()` is infallible *and* fully safe — no `unsafe` required:

```rust playground
/// A vector guaranteed to contain at least one element.
pub struct NonEmptyVec<T> {
    inner: Vec<T>,
}

impl<T> NonEmptyVec<T> {
    /// Construct from a guaranteed-present first element.
    pub fn new(first: T) -> Self {
        NonEmptyVec { inner: vec![first] }
    }

    pub fn push(&mut self, item: T) {
        self.inner.push(item);
    }

    /// Infallible: the type's invariant guarantees a first element exists.
    pub fn first(&self) -> &T {
        // Safe indexing; the bounds check is trivially satisfied and the
        // optimizer can elide it, so there's nothing for `unsafe` to win.
        &self.inner[0]
    }
}

fn main() {
    let mut v = NonEmptyVec::new(10);
    v.push(20);
    println!("first = {}", v.first());
}
```

```text
$ cargo run
first = 10
```

When *would* `unsafe` be justified here? Essentially never for correctness — the invariant is enforced by the API, and the safe `[0]` index is already optimal in release builds. You would only consider `get_unchecked(0)` after **profiling proves** the bounds check is a measured bottleneck in a hot loop, and even then you would wrap it in an audited `unsafe` block with a `// SAFETY:` comment citing the non-empty invariant. The default is: uphold invariants in the type, not with raw pointers.

</details>
