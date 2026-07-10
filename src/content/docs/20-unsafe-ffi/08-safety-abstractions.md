---
title: "Building Safe Abstractions Over `unsafe`"
description: "Confine unsafe to a tiny audited core and wrap it in a safe API. Unlike a TypeScript private field, Rust makes the invariant genuinely unbreakable."
---

The whole point of `unsafe` is to confine it to a tiny, audited core and then build a **fully safe API on top**, not to *use* it everywhere. This is the discipline that lets Rust's standard library be written largely in unsafe code while every program you build on it stays sound. This page teaches the "unsafe inside, safe outside" pattern: how to state an **invariant**, uphold it in every method, and expose an API surface whose callers never type the word `unsafe`.

---

## Quick Overview

A **safe abstraction** is a module that uses `unsafe` internally but presents an API that cannot trigger undefined behavior no matter how a caller (mis)uses it. The trick is an **invariant**: a property your code establishes (e.g. "these bytes are always valid ASCII") and that every `unsafe` block is allowed to assume. As long as every path that could break the invariant is guarded, the `unsafe` is *sound*: provably free of undefined behavior for all inputs.

For a TypeScript/JavaScript developer this is the same instinct as a well-designed class with private fields and validated constructors — but with one enormous difference. In TypeScript an invariant is a *convention*: nothing in the language stops external code from reaching in and breaking it, and the worst case is wrong output or a thrown error. In Rust the invariant is load-bearing for *memory safety*. Break it and you get undefined behavior, not a `TypeError`. So Rust gives you the tools — privacy, the borrow checker, lifetimes, `Drop` — to make the invariant genuinely unbreakable from the outside.

---

## TypeScript/JavaScript Example

Here is the kind of "fast path backed by an invariant" you might write in TypeScript: a class that validates its input is ASCII once, then decodes without re-checking. It looks disciplined, and it compiles cleanly — but watch what the language does *not* protect.

```typescript
// ascii.ts
class AsciiString {
  private bytes: Uint8Array;

  // The "invariant": every byte < 128. Established here, at construction.
  private constructor(bytes: Uint8Array) {
    this.bytes = bytes;
  }

  static from(text: string): AsciiString | null {
    for (const ch of text) {
      if (ch.charCodeAt(0) > 127) return null; // validate up front
    }
    return new AsciiString(new TextEncoder().encode(text));
  }

  // "Fast path" decode that ASSUMES the invariant holds (no re-validation).
  toStringFast(): string {
    return String.fromCharCode(...this.bytes);
  }

  // Nothing in the language stops this from breaking the invariant later:
  corrupt(): void {
    this.bytes[0] = 0xff; // now the "all ASCII" promise is a lie
  }
}

const s = AsciiString.from("hello")!;
console.log("fast:", s.toStringFast());
s.corrupt();
console.log("after corrupt:", JSON.stringify(s.toStringFast()));
console.log("rejected:", AsciiString.from("café"));
```

Running it under Node v22 (`node --experimental-strip-types ascii.ts`) prints:

```text
fast: hello
after corrupt: "ÿello"
rejected: null
```

Three things to notice. The constructor *did* validate (`café` is correctly rejected). But `private` is a TypeScript-compile-time fiction: `bytes` is a plain mutable `Uint8Array`, and `corrupt()` quietly writes `0xff`, after which `toStringFast` happily produces the wrong string `"ÿello"`. The "invariant" was never enforced after construction; it was a comment and a hope. In JavaScript that is merely a correctness bug. The same shape in Rust, if `toStringFast` were backed by an `unsafe` decode, would be **undefined behavior**, which is why Rust must enforce the invariant for real.

---

## Rust Equivalent

The idiomatic Rust version makes the same promise — "every byte is ASCII" — but the invariant is genuinely unbreakable from outside the module, so the `unsafe` fast-path decode is *sound*. The `bytes` field is private, there is no method that lets external code write arbitrary bytes, and `push` re-checks every character.

```rust playground
// src/main.rs

/// A string guaranteed to contain only ASCII bytes.
pub struct Ascii {
    // INVARIANT: every byte in `bytes` is a valid ASCII byte (0..=127).
    // Because all-ASCII bytes are by definition valid UTF-8, the unsafe
    // decode in `as_str` is sound as long as this invariant holds.
    bytes: Vec<u8>,
}

impl Ascii {
    /// Build an `Ascii`, validating the invariant up front.
    /// Returns `None` if any byte is non-ASCII.
    pub fn new(bytes: Vec<u8>) -> Option<Ascii> {
        if bytes.iter().all(|&b| b.is_ascii()) {
            Some(Ascii { bytes })
        } else {
            None
        }
    }

    /// View the contents as `&str` with NO runtime UTF-8 validation.
    pub fn as_str(&self) -> &str {
        // SAFETY: the constructor and `push` guarantee every byte is ASCII
        // (< 128), and all-ASCII bytes are by definition valid UTF-8, so
        // skipping the check in `from_utf8_unchecked` is sound.
        unsafe { std::str::from_utf8_unchecked(&self.bytes) }
    }

    /// The only way to mutate the buffer — and it re-checks the invariant.
    pub fn push(&mut self, ch: char) -> Result<(), char> {
        if ch.is_ascii() {
            self.bytes.push(ch as u8);
            Ok(())
        } else {
            Err(ch) // refuse: pushing a non-ASCII char would break the invariant
        }
    }
}

fn main() {
    let mut greeting = Ascii::new(b"hello".to_vec()).expect("all ASCII");
    greeting.push(' ').unwrap();
    greeting.push('w').unwrap();
    println!("as_str -> {:?}", greeting.as_str());

    // Non-ASCII input is rejected at construction, so the invariant always holds.
    let rejected = Ascii::new("café".as_bytes().to_vec());
    println!("rejected non-ASCII -> {}", rejected.is_none());

    // Pushing a non-ASCII char is refused, preserving the invariant.
    let mut buf = Ascii::new(Vec::new()).unwrap();
    println!("push('é') -> {:?}", buf.push('é'));
}
```

Real output:

```text
$ cargo run
as_str -> "hello w"
rejected non-ASCII -> true
push('é') -> Err('é')
```

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition, which `cargo new` selects automatically. The `unsafe` here is a single expression with a `// SAFETY:` comment naming exactly which invariant it relies on. Everything a caller can touch — `new`, `as_str`, `push` — is safe. There is no `corrupt()`-shaped hole, because the only mutating method validates its input.

---

## Detailed Explanation

### The pattern: an invariant + a wall around it

A safe abstraction has three moving parts:

1. **A private representation that can hold both valid and invalid states.** `Vec<u8>` can hold any bytes; only *our discipline* keeps it all-ASCII. The field is `bytes`, not `pub bytes`, so only code in this module can construct or mutate it.
2. **An invariant**, written as a comment on the field and treated as gospel by every `unsafe` block. Here: "every byte is `0..=127`."
3. **A wall of safe methods** that *establish* the invariant (constructors validate) and *preserve* it (mutators re-check), so it is true at every observable moment. Each `unsafe` block then gets to *assume* the invariant, which is what makes it sound.

The slogan is **"unsafe inside, safe outside."** The unsafe core is small and audited; the public surface is ordinary safe Rust. A caller can hammer the API with any input and never cause UB, because every door that could let an invalid state in is guarded.

### Why `from_utf8_unchecked` needs an invariant at all

`std::str::from_utf8_unchecked(bytes)` is an `unsafe fn` because constructing a `&str` over bytes that are *not* valid UTF-8 is instant undefined behavior: `str` is defined to always be valid UTF-8, and the optimizer relies on that. The *safe* alternative, `std::str::from_utf8`, scans the bytes and returns a `Result`. Our `Ascii` type pays that scan cost **once** at construction and then earns the right to skip it forever after. That is the entire economic argument for the `unsafe`: we moved a per-read check to a one-time per-write check, and we *proved* (via the invariant) that the per-read check is always redundant.

### Privacy is the wall — and it is real, unlike TypeScript `private`

In the TypeScript version, `private` is erased at runtime; `corrupt()` could scribble on `bytes` and `toStringFast` had no defense. In Rust, a field with no `pub` is genuinely inaccessible outside its module: there is no reflection escape hatch, no `as any`. Combined with not offering a `bytes_mut(&mut self) -> &mut Vec<u8>` method, this makes the invariant *unbreakable* from outside the module. The unsafe code's correctness depends only on code you wrote and can audit, never on what a caller does.

### Lifetimes extend the guarantee for free

Look again at `as_str(&self) -> &str`. Through Rust's [lifetime elision](/05-ownership/05-lifetime-elision/), the returned `&str` borrows `self`, so the borrow checker forbids using it after the `Ascii` is gone. The abstraction inherits use-after-free protection without any extra work:

```rust
pub struct Ascii { bytes: Vec<u8> }
impl Ascii {
    pub fn new(bytes: Vec<u8>) -> Option<Ascii> {
        bytes.iter().all(u8::is_ascii).then_some(Ascii { bytes })
    }
    pub fn as_str(&self) -> &str {
        // SAFETY: invariant guarantees all bytes are ASCII, hence valid UTF-8.
        unsafe { std::str::from_utf8_unchecked(&self.bytes) }
    }
}
fn main() {
    let s;
    {
        let a = Ascii::new(b"hi".to_vec()).unwrap();
        s = a.as_str();          // borrows `a`
    }                            // `a` dropped here
    println!("{s}");             // does not compile (error[E0597])
}
```

The compiler rejects this dangling use:

```text
error[E0597]: `a` does not live long enough
  --> src/main.rs:15:13
   |
14 |         let a = Ascii::new(b"hi".to_vec()).unwrap();
   |             - binding `a` declared here
15 |         s = a.as_str();          // borrows `a`
   |             ^ borrowed value does not live long enough
16 |     }                            // `a` dropped here
   |     - `a` dropped here while still borrowed
17 |     println!("{s}");             // does not compile (error[E0597])
   |                - borrow later used here
```

The same code in C (return a `char*` into a buffer, free the buffer, print the pointer) is a textbook use-after-free that *might* "work" in testing. Rust's elision rules turn that class of bug into a compile error, and the safe abstraction gets it automatically.

### Owning a raw allocation: `unsafe` plus `Drop`

Some abstractions own a resource that the type system cannot track: a heap allocation made through the global allocator, an OS handle, a C pointer. The pattern scales: hold the resource in a private field, uphold the invariant in every method, and release it exactly once in [`Drop`](/05-ownership/08-drop-trait/). Here is a teaching-sized version of what `Vec<T>` does internally:

```rust playground
// src/main.rs
use std::alloc::{self, Layout};
use std::ptr::NonNull;

pub struct ByteBuffer {
    // INVARIANTS (upheld by every method, relied on by the unsafe code):
    //   * `ptr` points to an allocation of exactly `cap` bytes from the global
    //     allocator, made with the layout reconstructed in `push`/`Drop`.
    //   * `len <= cap`.
    //   * the first `len` bytes are initialized.
    ptr: NonNull<u8>,
    len: usize,
    cap: usize,
}

impl ByteBuffer {
    pub fn with_capacity(cap: usize) -> ByteBuffer {
        assert!(cap > 0, "capacity must be non-zero");
        let layout = Layout::array::<u8>(cap).expect("capacity overflow");
        // SAFETY: `layout` has non-zero size because `cap > 0` is asserted.
        let raw = unsafe { alloc::alloc(layout) };
        let ptr = match NonNull::new(raw) {
            Some(p) => p,
            None => alloc::handle_alloc_error(layout),
        };
        ByteBuffer { ptr, len: 0, cap }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn push(&mut self, byte: u8) -> Result<(), u8> {
        if self.len == self.cap {
            return Err(byte); // full: refuse rather than write out of bounds
        }
        // SAFETY: `len < cap`, so `ptr + len` is within the allocation and the
        // slot is owned by us and currently uninitialized — a valid write.
        unsafe {
            self.ptr.as_ptr().add(self.len).write(byte);
        }
        self.len += 1;
        Ok(())
    }

    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: by the invariants, the first `len` bytes are initialized and
        // live for as long as `&self`, so this shared slice is valid.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for ByteBuffer {
    fn drop(&mut self) {
        let layout = Layout::array::<u8>(self.cap).expect("layout was valid at alloc time");
        // SAFETY: `ptr` came from `alloc::alloc` with exactly this `layout`,
        // and `Drop` runs at most once, so this frees a live allocation once.
        unsafe {
            alloc::dealloc(self.ptr.as_ptr(), layout);
        }
    }
}

fn main() {
    let mut buf = ByteBuffer::with_capacity(4);
    for b in [b'R', b'u', b's', b't'] {
        buf.push(b).unwrap();
    }
    println!("len = {}", buf.len());
    println!("as_slice = {:?}", buf.as_slice());
    println!("as text  = {:?}", std::str::from_utf8(buf.as_slice()).unwrap());
    println!("push when full -> {:?}", buf.push(b'!'));
    // `buf` is dropped here; the allocation is freed exactly once.
}
```

```text
$ cargo run
len = 4
as_slice = [82, 117, 115, 116]
as text  = "Rust"
push when full -> Err(33)
```

Every `unsafe` block names the invariant it leans on; `push` refuses to write past `cap`; `Drop` frees the allocation exactly once with the matching `Layout`. A caller writes no `unsafe`, cannot leak the allocation, and cannot double-free it. (This intentionally omits growth and `Send`/`Sync` for brevity; see the warnings below.) For the mechanics of raw pointers and allocation used here, see [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/) and [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/).

### `unsafe trait`: a different kind of invariant

The fourth of the five "unsafe superpowers" (see [What `unsafe` Really Means (and What It Does Not)](/20-unsafe-ffi/00-unsafe-intro/)) is *implementing* an `unsafe trait`. The two you will meet first are `Send` (safe to move to another thread) and `Sync` (safe to share `&T` across threads). The compiler auto-implements them for types built from `Send`/`Sync` parts, but a type containing a **raw pointer** is `!Send` and `!Sync` by default: the compiler cannot know whether sharing it is safe, so it conservatively says no. When you *can* prove it is safe, you promise so with `unsafe impl`:

```rust playground
// src/main.rs
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

/// A counter accessed through a raw pointer to an atomic.
struct AtomicCounter {
    // INVARIANT: `ptr` points to a live `AtomicU64` owned by the `Arc` below,
    // kept alive for as long as this struct exists.
    ptr: *const AtomicU64,
    _owner: Arc<AtomicU64>,
}

// SAFETY: the only thing reachable through `ptr` is an `AtomicU64`, whose
// operations are atomic and therefore safe to call concurrently from multiple
// threads. The `Arc` keeps it alive. So sending/sharing this handle is sound.
unsafe impl Send for AtomicCounter {}
unsafe impl Sync for AtomicCounter {}

impl AtomicCounter {
    fn new() -> AtomicCounter {
        let owner = Arc::new(AtomicU64::new(0));
        let ptr = Arc::as_ptr(&owner);
        AtomicCounter { ptr, _owner: owner }
    }

    fn increment(&self) {
        // SAFETY: `ptr` is valid for the lifetime of `self` (the `Arc` owns it),
        // and atomic ops are safe to call through a shared reference concurrently.
        unsafe { (*self.ptr).fetch_add(1, Ordering::Relaxed) };
    }

    fn get(&self) -> u64 {
        // SAFETY: same invariant as `increment`.
        unsafe { (*self.ptr).load(Ordering::Relaxed) }
    }
}

fn main() {
    let counter = Arc::new(AtomicCounter::new());
    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                c.increment();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!("final count = {}", counter.get());
}
```

```text
$ cargo run
final count = 4000
```

Implementing `Send`/`Sync` with `unsafe impl` is a *promise about thread-safety* you must actually deliver: here, the only shared operations are atomic, so concurrent access is data-race-free. Get this wrong (claim `Send` for a type that mutates non-atomic shared state) and you reintroduce data races, the exact UB safe Rust eliminates. (In real code you would usually just store the `Arc<AtomicU64>` directly, which is already `Send + Sync`; the raw pointer here exists only to demonstrate the `unsafe impl`.)

---

## Key Differences

### A TypeScript class invariant vs. a Rust safe abstraction

| Aspect | TypeScript class with `private` field | Rust safe abstraction |
| ------ | ------------------------------------- | --------------------- |
| Is the invariant enforced after construction? | No: `private` is compile-time only; runtime mutation is possible | Yes: privacy is real; only your module's methods can mutate |
| What happens if the invariant breaks? | Wrong output or a thrown `Error` (recoverable) | Undefined behavior in the `unsafe` code (not recoverable) |
| Escape hatches that bypass it | `as any`, bracket access, reflection, `Object.assign` | None from outside the module |
| Who must uphold it | "Everyone, please" (convention) | The module's authors only (a closed, auditable set) |
| Caller-facing risk | Caller can break it accidentally | Caller cannot break it at all |
| Tooling | Linter, types (erased at runtime) | Compiler privacy + borrow checker + `Miri` for the unsafe core |

### The soundness contract

A safe abstraction is **sound** if there is *no* sequence of safe calls a user can make that triggers undefined behavior. This is a stronger property than "passes my tests." It must hold for every input, every ordering, every combination, which is why you reason about it via the invariant rather than by example. The standard library's `Vec`, `String`, `Rc`, `RefCell`, and `Mutex` are all "unsafe inside, safe outside" abstractions whose soundness was argued this way and (increasingly) checked with `Miri`.

> **Tip:** A practical test for "is my abstraction sound?": *can a caller, using only safe methods and any inputs, ever make one of my `unsafe` blocks rely on a false assumption?* If yes — for example a `bytes_mut()` that hands out raw mutable access — the abstraction is unsound even though it compiles. Close the hole or mark the leaky method `unsafe`.

---

## Common Pitfalls

### Pitfall 1: Leaking mutable access that breaks the invariant

The most common way to accidentally make a safe wrapper *unsound* is to expose unrestricted mutation. Adding a `bytes_mut()` method to `Ascii` lets safe code write non-ASCII bytes, after which `as_str()` constructs a `&str` over invalid UTF-8: undefined behavior, reached without the caller ever typing `unsafe`:

```rust playground
pub struct Ascii {
    bytes: Vec<u8>, // INVARIANT: all bytes < 128
}

impl Ascii {
    pub fn new(bytes: Vec<u8>) -> Option<Ascii> {
        bytes.iter().all(u8::is_ascii).then_some(Ascii { bytes })
    }
    pub fn as_str(&self) -> &str {
        // SAFETY: relies on the all-ASCII invariant.
        unsafe { std::str::from_utf8_unchecked(&self.bytes) }
    }
    // DANGER: hands out unrestricted mutable access, so safe code can write
    // non-ASCII bytes and break the invariant `as_str` depends on.
    pub fn bytes_mut(&mut self) -> &mut Vec<u8> {
        &mut self.bytes
    }
}

fn main() {
    let mut a = Ascii::new(b"hi".to_vec()).unwrap();
    a.bytes_mut()[0] = 0xff; // invariant broken from SAFE code — and it compiles!
    println!("invariant now broken: {:?}", a.bytes_mut());
}
```

This **compiles and runs without warning**, which is exactly what makes it dangerous. The compiler cannot know that `bytes_mut` undermines `as_str`'s invariant; only you can. **Fix:** never expose a way to set invalid state. Offer narrow, invariant-preserving mutators (like `push`, which re-checks), or if raw access is genuinely needed, make the accessor an `unsafe fn` with a `# Safety` contract so the caller takes responsibility.

### Pitfall 2: Forgetting a `# Safety` doc on a public `unsafe fn`

If you do expose an `unsafe fn`, Clippy insists you document the contract callers must uphold:

```rust
pub struct Buffer {
    data: Vec<u8>,
}

impl Buffer {
    /// Returns the byte at `index` without a bounds check.
    pub unsafe fn get_unchecked(&self, index: usize) -> u8 {
        // SAFETY: forwarded to the caller.
        unsafe { *self.data.get_unchecked(index) }
    }
}
```

`cargo clippy` reports:

```text
warning: unsafe function's docs are missing a `# Safety` section
 --> src/lib.rs:7:5
  |
7 |     pub unsafe fn get_unchecked(&self, index: usize) -> u8 {
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#missing_safety_doc
  = note: `#[warn(clippy::missing_safety_doc)]` on by default
```

**Fix:** add a `/// # Safety` section stating the precondition (here, "`index` must be less than `self.len()`"). Better still, ask whether the function needs to be `unsafe` at all — a checked `get(&self, index) -> Option<u8>` keeps the whole API safe.

### Pitfall 3: Calling an internal `unsafe` operation without a block

In the 2024 edition the body of an `unsafe fn` is *safe by default*, and any unsafe operation anywhere needs its own `unsafe` block. Forgetting it is a hard error:

```rust
fn main() {
    let bytes = vec![0xff, 0xfe];
    let s = std::str::from_utf8_unchecked(&bytes); // does not compile (error[E0133])
    println!("{s}");
}
```

```text
error[E0133]: call to unsafe function `from_utf8_unchecked` is unsafe and requires unsafe block
 --> src/main.rs:4:13
  |
4 |     let s = std::str::from_utf8_unchecked(&bytes);
  |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
  |
  = note: consult the function's documentation for information on how to avoid undefined behavior
```

**Fix:** wrap the call in `unsafe { ... }` *with a `// SAFETY:` comment*, and note that this particular snippet would also be *unsound* (the bytes are not valid UTF-8), which is the whole reason the abstraction must validate first.

### Pitfall 4: `unsafe impl Send` for a type that is not actually thread-safe

Putting a raw pointer in a struct makes it `!Send`/`!Sync`, and `thread::spawn` requires `Send`:

```rust
use std::thread;

struct Handle {
    ptr: *const u64,
}

fn spawn_with(h: Handle) {
    thread::spawn(move || {  // does not compile (error[E0277])
        let _p = h.ptr;
    });
}

fn main() {
    let boxed: Box<u64> = Box::new(42);
    let h = Handle { ptr: Box::into_raw(boxed) };
    spawn_with(h);
}
```

```text
error[E0277]: `*const u64` cannot be sent between threads safely
   --> src/main.rs:8:19
    |
  8 |       thread::spawn(move || {  // does not compile (error[E0277])
    |       ------------- ^------
    |       |             |
    |  _____|_____________within this `{closure@src/main.rs:8:19: 8:26}`
    | |     |
    | |     required by a bound introduced by this call
  9 | |         let _p = h.ptr;
 10 | |     });
    | |_____^ `*const u64` cannot be sent between threads safely
    |
    = help: within `{closure@src/main.rs:8:19: 8:26}`, the trait `Send` is not implemented for `*const u64`
```

The *wrong* fix is a reflexive `unsafe impl Send for Handle {}` just to make the error go away; that is a lie to the compiler unless the type is genuinely thread-safe, and it reopens the door to data races. **Fix:** only write `unsafe impl Send` when you can justify it (as in the `AtomicCounter` example, where everything shared is atomic), and write the `// SAFETY:` argument. Otherwise restructure to use already-`Send` building blocks (`Arc<Mutex<T>>`, `Arc<AtomicU64>`).

### Pitfall 5: Calling "it passed my tests" the same as "it's sound"

Undefined behavior can lie dormant: code with a latent UB bug may produce correct output until an optimizer, a different platform, or an unrelated change exposes it. A safe abstraction is only as good as the *argument* that every `unsafe` block's invariant truly holds for all inputs. **Fix:** treat soundness as a proof obligation, write the `// SAFETY:` comments as that proof, and run the unsafe core under [Miri](https://github.com/rust-lang/miri) (`cargo +nightly miri test`), which interprets your code and flags many forms of UB — out-of-bounds, use-after-free, invalid `str`, data races — that ordinary tests sail past.

---

## Best Practices

- **Keep `unsafe` blocks tiny and name the invariant.** Wrap only the single operation that needs a superpower, and put a `// SAFETY:` comment on it stating *which* invariant makes it sound. If the comment is hard to write, the code is probably unsound.
- **Document the invariant on the field, not just in your head.** A `// INVARIANT:` comment on the private field is the contract every `unsafe` block in the type depends on. Reviewers (and future you) check the methods against it.
- **Make invalid states unrepresentable from outside.** Keep fields private, validate in constructors, re-validate in mutators, and *never* hand out a mutator that can set an invalid state. If a caller could break the invariant with safe code, the abstraction is unsound.
- **Expose `Result`/`Option` instead of `unsafe fn` wherever feasible.** A checked `get(i) -> Option<T>` is almost always the right public API; reserve `unsafe fn` for the rare case where the caller really can prove a precondition the type cannot.
- **Document and justify every `unsafe impl`.** `Send`/`Sync` are promises about concurrency; only assert them when true, with a `// SAFETY:` argument. When in doubt, build from already-`Send`/`Sync` parts (`Arc`, `Mutex`, atomics) and let the compiler derive them.
- **Verify with tooling.** Run `cargo clippy` for the safety-doc lints and `cargo +nightly miri test` to catch UB the compiler can't see. Put `#![forbid(unsafe_code)]` at the crate root of modules that should contain none, so an accidental `unsafe` becomes a compile error.
- **Lean on `Drop` for resources.** If your abstraction owns an allocation, file handle, or C pointer, release it in `Drop` exactly once; that is how you give callers leak-free, double-free-free RAII for free (see [Drop trait](/05-ownership/08-drop-trait/)).

---

## Real-World Example

The canonical safe abstraction in the standard library is `<[T]>::split_at_mut`, which hands out *two* mutable slices into one buffer, something the borrow checker rejects on its face, yet is perfectly sound because the two slices cover disjoint ranges. Below is a production-flavored, fully-safe reimplementation. The `unsafe` is confined to two `from_raw_parts_mut` calls, the invariant (`mid <= len`, ranges disjoint) is established by an `assert!`, and the returned lifetimes are tied to the input so misuse is a compile error.

```rust playground
// src/main.rs
use std::slice;

/// Split `values` into two non-overlapping mutable halves at `mid`.
/// A simplified, fully-safe-to-call version of `<[T]>::split_at_mut`.
fn split_at_mut(values: &mut [i32], mid: usize) -> (&mut [i32], &mut [i32]) {
    let len = values.len();
    let ptr = values.as_mut_ptr();

    // Establish the invariant the unsafe blocks rely on, up front.
    assert!(mid <= len, "split index {mid} out of bounds for len {len}");

    // SAFETY: `mid <= len` was asserted, so both `[0, mid)` and `[mid, len)`
    // lie within the original allocation, and the two ranges are disjoint, so
    // the two `&mut [i32]` slices never alias — upholding Rust's aliasing rules
    // even though the borrow checker cannot see the disjointness itself.
    unsafe {
        (
            slice::from_raw_parts_mut(ptr, mid),
            slice::from_raw_parts_mut(ptr.add(mid), len - mid),
        )
    }
}

fn main() {
    let mut data = vec![1, 2, 3, 4, 5, 6];

    // Two mutable views into one buffer, simultaneously — impossible in safe
    // Rust without this carefully-justified `unsafe` underneath.
    let (left, right) = split_at_mut(&mut data, 3);
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

Notice the shape of a well-built safe abstraction: a *small* `unsafe` core, an `assert!` that turns the safety precondition into a checked invariant, a public signature whose lifetimes prevent misuse, and zero `unsafe` at the call site. The borrow checker still governs everything around it: the two returned slices borrow `data`, so you cannot, for instance, also index `data` directly while they are alive. This is the same discipline the standard library uses to give you `Vec`, `String`, and `split_at_mut` as safe building blocks. When you are tempted to reach for raw pointers yourself, first read [When `unsafe` and FFI Are Actually Necessary (and the Many Times They Are Not)](/20-unsafe-ffi/09-when-to-use/). Most of the time a safe data structure already exists.

> **Warning:** The `assert!(mid <= len)` is load-bearing. Remove it and `ptr.add(mid)` can compute a pointer one-past-or-beyond the allocation, which is undefined behavior *before any dereference*. Every invariant an `unsafe` block relies on must be genuinely guaranteed — by a check, by privacy, or by the type system — not merely "usually true."

---

## Further Reading

### Official documentation

- [The Rustonomicon — Working with Unsafe](https://doc.rust-lang.org/nomicon/working-with-unsafe.html): building sound abstractions over unsafe code
- [The Rustonomicon — Implementing Vec](https://doc.rust-lang.org/nomicon/vec/vec.html) — the canonical "unsafe inside, safe outside" walkthrough
- [The Rust Book — Unsafe Rust](https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html) — the five superpowers, including `unsafe trait`
- [`std::str::from_utf8_unchecked`](https://doc.rust-lang.org/std/str/fn.from_utf8_unchecked.html) and [`std::slice::from_raw_parts_mut`](https://doc.rust-lang.org/std/slice/fn.from_raw_parts_mut.html) — their documented safety contracts
- [Miri](https://github.com/rust-lang/miri) — the interpreter that checks unsafe code for undefined behavior
- [Clippy `missing_safety_doc`](https://rust-lang.github.io/rust-clippy/master/index.html#missing_safety_doc) — the lint enforcing `# Safety` docs

### Related sections in this guide

- [What `unsafe` Really Means](/20-unsafe-ffi/00-unsafe-intro/): the five superpowers and safety invariants (start here)
- [Unsafe Rust in Practice](/20-unsafe-ffi/01-unsafe-rust/) and [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/) — the operations used inside these abstractions
- [FFI Basics](/20-unsafe-ffi/03-ffi-basics/), [Calling C from Rust](/20-unsafe-ffi/04-calling-c/), and [bindgen](/20-unsafe-ffi/05-bindgen/) — wrapping C is the most common reason to build a safe abstraction
- [Node.js Addons with napi-rs](/20-unsafe-ffi/06-napi/) and [Neon](/20-unsafe-ffi/07-neon/) — exposing a safe Rust core to JavaScript
- [When to Use `unsafe`/FFI](/20-unsafe-ffi/09-when-to-use/): and the many times a safe data structure already exists
- Foundations: [Ownership](/05-ownership/), [the `Drop` trait](/05-ownership/08-drop-trait/), [lifetimes](/05-ownership/04-lifetimes/), [Smart Pointers](/10-smart-pointers/)
- The intro to the whole series: [Section 00: Introduction](/00-introduction/), [Section 02: Basics](/02-basics/)
- Going further: [Section 21: Performance](/21-performance/): where a justified, well-encapsulated `unsafe` sometimes earns its keep

---

## Exercises

### Exercise 1: Close the soundness hole

**Difficulty:** Easy

**Objective:** Recognize that exposing arbitrary mutation makes a safe abstraction unsound, and fix it.

**Instructions:** The `Ascii` type below has a `bytes_mut` method that lets safe code write non-ASCII bytes, after which `as_str` is undefined behavior. Without changing `as_str`, modify the API so the all-ASCII invariant is unbreakable from outside the module, while still allowing callers to append ASCII characters. Verify it compiles and runs.

```rust
pub struct Ascii {
    bytes: Vec<u8>, // INVARIANT: all bytes < 128
}

impl Ascii {
    pub fn new(bytes: Vec<u8>) -> Option<Ascii> {
        bytes.iter().all(u8::is_ascii).then_some(Ascii { bytes })
    }
    pub fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(&self.bytes) }
    }
    pub fn bytes_mut(&mut self) -> &mut Vec<u8> {
        &mut self.bytes // ← the hole
    }
}
```

<details>
<summary>Solution</summary>

Replace the leaky `bytes_mut` with an invariant-preserving `push` that re-checks every character. There is now no safe path to an invalid state, so `as_str`'s `unsafe` block is sound.

```rust playground
pub struct Ascii {
    bytes: Vec<u8>, // INVARIANT: all bytes < 128
}

impl Ascii {
    pub fn new(bytes: Vec<u8>) -> Option<Ascii> {
        bytes.iter().all(u8::is_ascii).then_some(Ascii { bytes })
    }

    pub fn as_str(&self) -> &str {
        // SAFETY: every byte is ASCII (constructor + `push` enforce it), so the
        // buffer is valid UTF-8 and skipping the check is sound.
        unsafe { std::str::from_utf8_unchecked(&self.bytes) }
    }

    /// Append an ASCII character, preserving the invariant.
    /// Non-ASCII input is refused, so an invalid state is unreachable.
    pub fn push(&mut self, ch: char) -> Result<(), char> {
        if ch.is_ascii() {
            self.bytes.push(ch as u8);
            Ok(())
        } else {
            Err(ch)
        }
    }
}

fn main() {
    let mut a = Ascii::new(b"hi".to_vec()).unwrap();
    a.push('!').unwrap();
    println!("{:?}", a.as_str());
    println!("push('€') -> {:?}", a.push('€'));
}
```

```text
$ cargo run
"hi!"
push('€') -> Err('€')
```

</details>

### Exercise 2: A safe, write-once cell

**Difficulty:** Medium

**Objective:** Build a safe abstraction over `MaybeUninit<T>`, confining `unsafe` to two blocks justified by a boolean invariant, and freeing the value correctly in `Drop`.

**Instructions:** Implement `OnceCell<T>` with `empty()`, `set(&mut self, value: T) -> Result<(), T>` (returns `Err` if already set), and `get(&self) -> Option<&T>`. Store the value in `MaybeUninit<T>` with a `bool` flag, uphold the invariant "if the flag is true, the value is initialized," and drop the value exactly once. Verify it compiles and runs with a `String` payload (so the `Drop` actually frees something).

<details>
<summary>Solution</summary>

```rust playground
use std::mem::MaybeUninit;

/// A slot written at most once, then read many times.
/// `unsafe` is confined to `get` and `Drop`, justified by `initialized`.
pub struct OnceCell<T> {
    // INVARIANT: if `initialized` is true, `value` holds a valid `T`.
    value: MaybeUninit<T>,
    initialized: bool,
}

impl<T> OnceCell<T> {
    pub fn empty() -> OnceCell<T> {
        OnceCell { value: MaybeUninit::uninit(), initialized: false }
    }

    /// Initialize the cell. Returns `Err(value)` if already set.
    pub fn set(&mut self, value: T) -> Result<(), T> {
        if self.initialized {
            return Err(value);
        }
        self.value.write(value); // safe: writes into the MaybeUninit
        self.initialized = true;
        Ok(())
    }

    pub fn get(&self) -> Option<&T> {
        if self.initialized {
            // SAFETY: `initialized` is true, so by the invariant `value` holds a
            // valid `T`; the returned ref borrows `self`, so it cannot dangle.
            Some(unsafe { self.value.assume_init_ref() })
        } else {
            None
        }
    }
}

impl<T> Drop for OnceCell<T> {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: the value was initialized and is dropped exactly once.
            unsafe { self.value.assume_init_drop() };
        }
    }
}

fn main() {
    let mut cell: OnceCell<String> = OnceCell::empty();
    println!("before: {:?}", cell.get());
    cell.set(String::from("ready")).unwrap();
    println!("after:  {:?}", cell.get());
    println!("second set: {:?}", cell.set(String::from("again")));
}
```

```text
$ cargo run
before: None
after:  Some("ready")
second set: Err("again")
```

The `Drop` impl is what makes this sound for non-`Copy` payloads like `String`: without it, the `String`'s heap buffer would leak. The `if self.initialized` guard ensures we never call `assume_init_drop` on uninitialized memory. (The standard library's `std::cell::OnceCell` and `std::sync::OnceLock` are the production versions of this idea.)

</details>

### Exercise 3: Audit an `unsafe impl Send`

**Difficulty:** Hard

**Objective:** Build the judgment to tell a *sound* `unsafe impl Send` from a *lie*: the difference between encapsulating unsafe and reintroducing data races.

**Instructions:** A teammate has a type `SharedBox<T>` holding a `*mut T` to a heap value, and wants to share `&SharedBox<T>` across threads so multiple threads can read *and write* `*ptr` directly. They propose `unsafe impl Sync for SharedBox<T> {}` to silence the `!Sync` error. Explain in two or three sentences why this `unsafe impl` is unsound as described, then sketch a version that *is* sound to share, and say what makes it so. (A compiling sketch is welcome but the reasoning is the point.)

<details>
<summary>Solution</summary>

The proposed `unsafe impl Sync` is **unsound**: `Sync` promises that `&SharedBox<T>` can be shared across threads, but the design has multiple threads performing *non-atomic, unsynchronized writes* to `*ptr` through shared references. That is a data race — undefined behavior — and no `unsafe impl` makes it true; it only suppresses the compiler's correct refusal. `unsafe impl Send`/`Sync` is a *promise you must actually deliver*, not a cast.

A version that *is* sound to share puts synchronization between the threads and the data, so concurrent access is no longer a race:

```rust playground
use std::sync::atomic::{AtomicI64, Ordering};

/// Sound to share: all access through `ptr` is atomic.
pub struct SharedCounter {
    // INVARIANT: `ptr` points to a live `AtomicI64`.
    ptr: *mut AtomicI64,
    _owner: Box<AtomicI64>,
}

// SAFETY: the only operations performed through `ptr` are atomic, which are
// defined to be data-race-free across threads, and `_owner` keeps the target
// alive for the lifetime of `self`. Therefore sharing `&SharedCounter` is sound.
unsafe impl Sync for SharedCounter {}
unsafe impl Send for SharedCounter {}

impl SharedCounter {
    pub fn new() -> SharedCounter {
        let mut owner = Box::new(AtomicI64::new(0));
        let ptr: *mut AtomicI64 = &mut *owner;
        SharedCounter { ptr, _owner: owner }
    }
    pub fn add(&self, n: i64) {
        // SAFETY: `ptr` is valid (kept alive by `_owner`) and the op is atomic.
        unsafe { (*self.ptr).fetch_add(n, Ordering::Relaxed) };
    }
    pub fn get(&self) -> i64 {
        // SAFETY: same invariant as `add`.
        unsafe { (*self.ptr).load(Ordering::Relaxed) }
    }
}

fn main() {
    let c = SharedCounter::new();
    c.add(5);
    c.add(37);
    println!("count = {}", c.get());
}
```

```text
$ cargo run
count = 42
```

What makes it sound: every access through the raw pointer is an *atomic* operation, which the language guarantees is free of data races even under concurrent reads and writes; the owning `Box` keeps the target alive. (As in the chapter's example, real code would simply use `Arc<AtomicI64>` and let the compiler derive `Send`/`Sync` — the raw pointer is only here to make the `unsafe impl` concrete.)

</details>
