---
title: "Pin and Unpin"
description: "Why self-referential Rust futures must never move in memory, what guarantee Pin<P> encodes, and how Unpin lets almost every type ignore it. With JS contrasts."
---

`Pin` is the type that makes `async`/`await` sound. It is one of the most mystifying corners of Rust for newcomers, yet the idea is small: some values must never move in memory once they have been observed, and `Pin<P>` is the wrapper that encodes that promise in the type system. This page explains *why* self-referential futures need pinning, what guarantee `Pin` actually provides, and how `Unpin` lets the vast majority of types ignore the whole thing.

---

## Quick Overview

In Rust a value can normally be **moved** (relocated to a new memory address) at any time, and the language does this constantly (returning from a function, pushing into a `Vec`, swapping two variables). `Pin<P>` is a pointer wrapper that revokes that permission for the value behind the pointer: once pinned, the value is guaranteed to stay at the same address until it is dropped. This matters because a compiled `async` block can be **self-referential** (one field holds a pointer into another field), and moving such a value would leave that internal pointer dangling. The `Unpin` marker trait is the escape hatch: almost every ordinary type is `Unpin`, meaning "I have no self-references, so pinning me changes nothing."

> **Note:** If you write application-level async code with Tokio, you rarely *create* `Pin` by hand; the runtime and `.await` do it for you. You meet `Pin` directly when you implement `Future` or `Stream` manually, or when you hold a future across a `select!`/`loop`. Understanding it removes the fear of the `Pin<&mut Self>` in those signatures.

---

## TypeScript/JavaScript Example

JavaScript has no concept of an object's memory address, and no concept of "moving" a value. A reference is a stable handle to a heap object that the garbage collector keeps alive; the object never relocates from your program's point of view, and you can freely build self-referential structures:

```typescript
// TypeScript — a "self-referential" object is completely ordinary.
// `view` points at the same backing buffer as `data`.
class Parsed {
  data: Uint8Array;
  view: Uint8Array; // a sub-view that aliases `data`'s memory

  constructor(text: string) {
    this.data = new TextEncoder().encode(text);
    // view aliases bytes 1..4 of the SAME ArrayBuffer
    this.view = this.data.subarray(1, 4);
  }
}

const p = new Parsed("hello");

// We can pass `p` around, store it in arrays, capture it in closures —
// the engine never "moves" the object, so `view` stays valid forever.
const holder = [p];
const fn = () => p.view[0];
console.log(p.view[0]); // 101  (the byte 'e')
console.log(fn());      // 101  — still valid after being captured
```

`p.view` keeps working no matter how many places hold `p`, because in JavaScript every object lives at a fixed (logical) location managed by the GC. There is no operation that copies the object's bytes to a new address and invalidates internal pointers. Async is the same story: an `async function`'s suspended state is a closure on the GC heap, and a variable that "borrows" another local across an `await` is just two references to GC-managed objects — nothing can dangle.

```typescript
// JS async: locals that reference each other across `await` are fine.
async function process(): Promise<number> {
  const buf = new Uint8Array([1, 2, 3, 4, 5]);
  const borrowed = buf.subarray(1, 4); // references INTO buf
  await new Promise((r) => setTimeout(r, 0)); // suspend & resume
  return borrowed.reduce((a, b) => a + b, 0); // borrowed still valid
}
process().then((n) => console.log(n)); // 9
```

This freedom is exactly what Rust cannot offer for free, because Rust values are *not* GC-managed boxes. They are plain bytes that the compiler is allowed to memcpy elsewhere.

---

## Rust Equivalent

In Rust, "move" is a real, byte-level operation, and an internal pointer that survives a move becomes dangling. To hold the kind of self-referential structure that JavaScript hands you for free, you must pin the value so it can never move, and you must opt out of `Unpin` with `PhantomPinned`:

```rust
use std::marker::PhantomPinned;
use std::pin::Pin;

// A self-referential struct: `slice` is a raw pointer INTO `data`'s buffer.
struct SelfRef {
    data: String,
    slice: *const u8,    // points into `data` — invalidated by any move
    _pin: PhantomPinned, // opts the type OUT of Unpin (makes it !Unpin)
}

impl SelfRef {
    fn new(text: &str) -> Pin<Box<SelfRef>> {
        let value = SelfRef {
            data: String::from(text),
            slice: std::ptr::null(),
            _pin: PhantomPinned,
        };
        // `Box::pin` heap-allocates and pins in one step: from now on the
        // SelfRef has a fixed address that we are promising never to move.
        let mut boxed = Box::pin(value);

        // Only NOW — after the address is stable — do we wire up the pointer.
        let self_ptr: *const u8 = boxed.data.as_ptr();
        // SAFETY: we never move the data; we only fill in the pointer field.
        unsafe {
            let mut_ref: Pin<&mut SelfRef> = Pin::as_mut(&mut boxed);
            Pin::get_unchecked_mut(mut_ref).slice = self_ptr;
        }
        boxed
    }

    fn first_byte(self: Pin<&Self>) -> u8 {
        // SAFETY: `slice` points into `data`, which has not moved since `new`.
        unsafe { *self.slice }
    }
}

fn main() {
    let s = SelfRef::new("hello");
    println!("first byte = {}", s.as_ref().first_byte());
    println!("as char     = {}", s.as_ref().first_byte() as char);
}
```

Real output:

```text
first byte = 104
as char     = h
```

The same self-reference your TypeScript wrote in one line requires `unsafe`, a `*const u8`, `PhantomPinned`, and a `Pin<Box<…>>` here. That is the cost of *not* having a garbage collector, and it is exactly the cost the compiler pays automatically when it lowers your `async fn` into a state machine. You almost never write `SelfRef` by hand; you write `async fn process()` and the compiler generates the moral equivalent.

> **Note:** Raw pointers (`*const u8`) and `unsafe` are covered in detail in [Section 20: Unsafe & FFI](/20-unsafe-ffi/02-raw-pointers/). `PhantomPinned` is a zero-sized marker, a cousin of the `PhantomData` discussed in [`PhantomData` and Zero-Sized Types](/25-advanced-topics/00-phantom-data/).

---

## Detailed Explanation

### Why moving is the problem

A Rust value is just bytes at some address. When you write `let b = a;` (for a non-`Copy` type), or return a value, or push into a `Vec` that reallocates, the compiler is free to *memcpy those bytes to a different address* and treat the old location as invalid. This is the "move" you learned in [Section 05: Ownership](/05-ownership/). It is cheap and pervasive, and for 99% of types it is harmless.

It is **not** harmless if the value contains a pointer into itself. After a move, the bytes live at a new address but the internal pointer still holds the old one: instant dangling pointer, and reading through it is undefined behavior. JavaScript never hits this because its objects do not get relocated; Rust hits it the moment a generated future borrows one local across an `.await` of another.

### What an `async` block compiles to

`async fn`/`async {}` does not run anything when called — it returns a lazy `Future` (the opposite of an eager JS `Promise`; see [Promises vs Futures](/11-async/00-promises-vs-futures/)). The compiler turns the body into an **enum state machine**, with one variant per `.await` suspension point. Locals that are live across an `.await` become fields of that enum. If one such local *borrows* another, the generated state holds a reference into itself: self-referential. Verify the borrow-across-await pattern compiles and runs:

```rust
// An async fn whose generated state machine is self-referential:
// `borrowed` references INTO `buf`, and both live across the `.await`.
async fn process() -> usize {
    let buf = vec![1u8, 2, 3, 4, 5];
    let borrowed: &[u8] = &buf[1..4]; // reference INTO buf
    tokio::task::yield_now().await;   // suspension point: state is parked here
    // After resume, `borrowed` must still point at the same `buf`.
    borrowed.iter().map(|&b| b as usize).sum()
}

#[tokio::main]
async fn main() {
    let total = process().await;
    println!("sum of borrowed slice = {}", total);
}
```

Real output:

```text
sum of borrowed slice = 9
```

(`cargo add tokio --features full` provides the runtime.) For this to be sound, the future must not move between the moment `borrowed` is set up and the moment it is read after resuming. That is precisely the guarantee `Pin` exists to provide.

### The `Future::poll` signature is the whole reason

The `Future` trait's method is:

```rust
// from the standard library — for illustration
fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
```

The receiver is `Pin<&mut Self>`, **not** `&mut self`. That single design choice is what enforces the no-move guarantee: an executor can only call `poll` if it first commits to pinning the future. Once pinned, the executor *cannot* get a plain `&mut Future` back out (for a non-`Unpin` future), so it can never `mem::swap` or move the future between polls. The internal self-references therefore stay valid. (The `Context`/`Waker` machinery is covered in [How Async/Await Works Under the Hood](/25-advanced-topics/02-async-internals/).)

### What `Pin<P>` actually is

`Pin<P>` is a thin wrapper around a pointer type `P` (e.g. `Pin<Box<T>>`, `Pin<&mut T>`). It does not change the layout or the runtime representation: `Pin<&mut T>` is just a `&mut T` at the machine level. Its power is entirely in its **API**: it refuses to hand you a `&mut T` (which you could move out of) unless `T: Unpin`. The guarantee is a *contract about the pointee*: "the `T` behind this pointer will not be moved until it is dropped."

### `Unpin`: the universal opt-out

`Unpin` is an auto-trait (like `Send`/`Sync`): the compiler implements it automatically for almost every type. A type is `Unpin` when moving it even while pinned is perfectly safe, which is true for any type with no self-references. `i32`, `String`, `Vec<T>`, your structs and enums: all `Unpin` by default. For an `Unpin` type, `Pin` is a no-op wrapper and you get full mutable access right back:

```rust
use std::pin::Pin;
use std::mem;

fn main() {
    // i32 is Unpin, so Pin grants NO extra restriction.
    let mut a = Box::pin(10_i32);
    let mut b = Box::pin(20_i32);
    // For Unpin types you can pull a plain &mut back out and even swap them.
    mem::swap(a.as_mut().get_mut(), b.as_mut().get_mut());
    println!("a = {}, b = {}", *a, *b);

    // `Pin::new` works for any Unpin type with NO unsafe required.
    let mut value = 5_i32;
    let mut pinned: Pin<&mut i32> = Pin::new(&mut value);
    *pinned = 99;
    println!("value = {}", value);
}
```

Real output:

```text
a = 20, b = 10
value = 99
```

The only types that are **not** `Unpin` are those that contain a `PhantomPinned`, and the compiler-generated futures of `async` blocks. For everything else, you can think of `Pin` as a label you sometimes have to satisfy but that never gets in your way.

### Pinning without the heap: the `pin!` macro

`Box::pin` allocates. Since Rust 1.68 the standard `std::pin::pin!` macro pins a value to the current **stack frame**: no allocation, no `unsafe`. This is how you poll a future by hand, or feed one to `select!`:

```rust
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

struct NoopWaker;
impl Wake for NoopWaker {
    fn wake(self: Arc<Self>) {}
}

async fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    // `pin!` pins the future to this stack frame — no heap, no unsafe.
    let mut fut = pin!(add(2, 3));

    let waker = Waker::from(Arc::new(NoopWaker));
    let mut cx = Context::from_waker(&waker);

    // `poll` requires `Pin<&mut Self>`; without pinning we could not call it.
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(v) => println!("ready: {v}"),
        Poll::Pending => println!("pending"),
    }
}
```

Real output:

```text
ready: 5
```

> **Tip:** The macro borrows for the rest of the enclosing scope, so the pinned value cannot escape — exactly the constraint that makes stack pinning sound.

---

## Key Differences

| Aspect | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Can a value's address change? | Conceptually no — the GC manages objects in place | Yes — "move" is a real byte-copy the compiler inserts |
| Self-referential objects | Trivial; references stay valid forever | Unsafe by default; need `Pin` + `PhantomPinned` |
| Async suspended state | A closure on the GC heap; locals are GC references | An enum state machine; cross-`await` borrows are self-references |
| What enforces the guarantee | The runtime / garbage collector | The type system, via `Pin<&mut Self>` on `poll` |
| Cost of the safety | Hidden GC overhead | Zero runtime cost; `Pin` is a compile-time-only wrapper |
| Most values | N/A | `Unpin`: pinning does nothing, full access stays |

The deepest difference: Rust gives you **zero-cost** safety here. `Pin` adds no bytes and no runtime checks. It is purely a set of compile-time API restrictions that prevent you from moving something you promised not to move. JavaScript's equivalent guarantee is paid for at runtime by the garbage collector keeping every object at a stable logical location.

> **Note:** `Pin` is about *not moving*, which is unrelated to aliasing rules. It does not give you shared mutability; that is what `Cell`/`RefCell`/`Mutex` are for (see [Section 10: Smart Pointers](/10-smart-pointers/)).

---

## Common Pitfalls

### Pitfall 1: Trying to call `poll` on an unpinned future

A future cannot be polled until it is pinned. That is the entire point. New implementers often try to forward `poll` straight through an owned or `&mut` future:

```rust
use std::future::Future;
use std::task::{Context, Poll};

async fn work() -> i32 { 42 }

fn poll_once(mut fut: impl Future<Output = i32>, cx: &mut Context<'_>) -> Poll<i32> {
    // does not compile (error[E0599]): poll() needs Pin<&mut Self>,
    // but `fut` here is owned and unpinned.
    fut.poll(cx)
}

fn main() {
    let _ = poll_once(work(), todo!());
}
```

The real compiler error is unusually friendly. It tells you exactly what to do:

```text
error[E0599]: no method named `poll` found for type parameter `impl Future<Output = i32>` in the current scope
 --> src/main.rs:8:9
  |
6 | fn poll_once(mut fut: impl Future<Output = i32>, cx: &mut Context<'_>) -> Poll<i32> {
  |                       ------------------------- method `poll` not found for this type parameter
7 |     // ...
8 |     fut.poll(cx)
  |         ^^^^ method not found in `impl Future<Output = i32>`
  |
help: consider pinning the expression
  |
8 ~     let mut pinned = std::pin::pin!(fut);
9 ~     pinned.as_mut().poll(cx)
  |
```

The fix is exactly the compiler's suggestion: `let mut pinned = std::pin::pin!(fut); pinned.as_mut().poll(cx)`.

### Pitfall 2: `Pin::new` on a `!Unpin` type

`Pin::new` is the safe constructor, but it is only available when `T: Unpin`, because for an `Unpin` type pinning cannot be violated. Trying it on a self-referential type fails:

```rust
use std::marker::PhantomPinned;
use std::pin::Pin;

struct NotUnpin {
    data: String,
    _pin: PhantomPinned,
}

fn main() {
    let mut value = NotUnpin { data: String::from("hi"), _pin: PhantomPinned };
    // does not compile (error[E0277]): NotUnpin is !Unpin, so the safe
    // `Pin::new` is unavailable.
    let pinned: Pin<&mut NotUnpin> = Pin::new(&mut value);
    let _r: &mut NotUnpin = pinned.get_mut();
    println!("{}", _r.data);
}
```

Real compiler error (truncated):

```text
error[E0277]: `PhantomPinned` cannot be unpinned
    --> src/main.rs:13:38
     |
13   |     let pinned: Pin<&mut NotUnpin> = Pin::new(&mut value);
     |                                      -------- ^^^^^^^^^^ within `NotUnpin`, the trait `Unpin` is not implemented for `PhantomPinned`
     |                                      |
     |                                      required by a bound introduced by this call
     |
     = note: consider using the `pin!` macro
             consider using `Box::pin` if you need to access the pinned value outside of the current scope
note: required by a bound in `Pin::<Ptr>::new`
```

As the note says, reach for `Box::pin` (heap, escapes the scope) or `std::pin::pin!` (stack, scoped) and accept that you can no longer get a plain `&mut` back.

### Pitfall 3: Forgetting to pin a future held across a loop or `select!`

`tokio::select!` and manual poll loops take `&mut future`, which means the future must already be pinned in a binding that outlives the loop. Beginners write `select! { x = some_future() => ... }` inside a loop and recreate the future every iteration (restarting the work), or get a type error. Pin it once, before the loop:

```rust
// pseudo-pattern (full version in Real-World Example below)
let fut = some_async_op();
tokio::pin!(fut);              // pin once, on the stack
loop {
    tokio::select! {
        result = &mut fut => { /* completes the SAME future */ }
        _ = tick() => { /* do periodic work without restarting fut */ }
    }
}
```

### Pitfall 4: Thinking `Pin` stops mutation

`Pin` only stops *moving*. You can still mutate a pinned value through `Pin<&mut T>`: via `get_mut` for `Unpin` types, or projection / `get_unchecked_mut` (unsafe) for `!Unpin` ones. Conflating "pinned" with "immutable" leads to confusion; the two concepts are orthogonal.

---

## Best Practices

- **Let the runtime do it.** In ordinary Tokio code you `.await` futures and never touch `Pin`. Reach for it only when implementing `Future`/`Stream` by hand or holding a future across `select!`/a loop.
- **Prefer `std::pin::pin!` over `Box::pin` when the future need not escape its scope** — it avoids a heap allocation. Use `Box::pin` when you must store the pinned future somewhere with a longer lifetime (e.g. in a struct field or return a `Pin<Box<dyn Future>>`).
- **Keep your own types `Unpin`.** Do not add `PhantomPinned` unless you genuinely build self-references. Almost all code should never opt out of `Unpin`.
- **For manual `poll` implementations, use `pin-project` (or `pin-project-lite`) instead of hand-written `unsafe`.** These crates generate sound *structural pinning* projections so you never call `get_unchecked_mut` yourself. Add with `cargo add pin-project-lite`.
- **Treat `unsafe` pinning code as a last resort and document the invariant.** Every `get_unchecked_mut`/`map_unchecked_mut` needs a `// SAFETY:` comment explaining why the pointee never moves.
- **Remember the guarantee is about the *pointee*, not the pointer.** You may freely move a `Pin<Box<T>>` around (it is just a pointer); what stays put is the `T` it points at.

> **Tip:** If you find yourself fighting `Pin` in business logic, step back — you have probably reached for a manual `Future` impl where an `async fn` or a `BoxFuture`/`futures::stream` combinator would do.

---

## Real-World Example

A common production need: run one long-running async operation while doing periodic work (a heartbeat, a progress tick, a timeout) without canceling and restarting the operation. The operation's future must be **pinned once** and re-borrowed each loop iteration. `tokio::pin!` pins it on the stack:

```rust
use std::time::Duration;
use tokio::time::sleep;

// Simulates a slow request whose future we must poll repeatedly.
async fn fetch_user(id: u32) -> String {
    sleep(Duration::from_millis(50)).await;
    format!("user#{id}")
}

#[tokio::main]
async fn main() {
    // We want to poll `work` across many loop iterations alongside a ticker.
    // `select!` polls `&mut future`, and a future can only be polled via Pin,
    // so we pin it ONCE on the stack and re-borrow it each iteration.
    let work = fetch_user(7);
    tokio::pin!(work); // pins `work` in place for the rest of this scope

    let mut ticks = 0u32;
    loop {
        tokio::select! {
            user = &mut work => {
                println!("got {user} after {ticks} ticks");
                break;
            }
            _ = sleep(Duration::from_millis(20)) => {
                ticks += 1;
                println!("tick {ticks}: still waiting...");
            }
        }
    }
}
```

Real output (timing-dependent tick count, but the shape is stable):

```text
tick 1: still waiting...
tick 2: still waiting...
got user#7 after 2 ticks
```

Without `tokio::pin!`, `&mut work` would not type-check (the future is not pinned), and reconstructing `fetch_user(7)` inside the loop would restart the 50 ms request on every tick, never finishing. Pinning makes "poll the *same* future to completion across iterations" both correct and ergonomic. `cargo add tokio --features full` provides `select!`, `sleep`, and `pin!`.

---

## Further Reading

- [`std::pin` module documentation](https://doc.rust-lang.org/std/pin/index.html): the authoritative explanation of the pinning guarantee and its invariants.
- [`std::marker::Unpin`](https://doc.rust-lang.org/std/marker/trait.Unpin.html) and [`std::marker::PhantomPinned`](https://doc.rust-lang.org/std/marker/struct.PhantomPinned.html).
- [`std::pin::pin!` macro](https://doc.rust-lang.org/std/pin/macro.pin.html) and [`Future`](https://doc.rust-lang.org/std/future/trait.Future.html).
- [The Async Book — "Pinning"](https://rust-lang.github.io/async-book/04_pinning/01_chapter.html).
- [`pin-project-lite` on docs.rs](https://docs.rs/pin-project-lite/) — safe structural pinning for your own types.
- Guide cross-links: [How Async/Await Works Under the Hood](/25-advanced-topics/02-async-internals/) (how `poll` and the state machine fit together) · [`PhantomData` and Zero-Sized Types](/25-advanced-topics/00-phantom-data/) (the `PhantomData`/zero-sized-marker family that `PhantomPinned` belongs to) · [Section 11: Async](/11-async/) and [Promises vs Futures](/11-async/00-promises-vs-futures/) (lazy futures vs eager promises) · [Section 10: Smart Pointers](/10-smart-pointers/00-box/) (`Box`, used by `Box::pin`) · [Section 05: Ownership](/05-ownership/) (what "move" means) · [Section 20: Unsafe & FFI](/20-unsafe-ffi/02-raw-pointers/) (raw pointers and `unsafe`) · [Section 26: Systems Programming](/26-systems-programming/).

---

## Exercises

### Exercise 1: Spot the self-reference

**Difficulty:** Beginner

**Objective:** Build the mental model of which async locals become self-referential.

**Instructions:** Look at the following `async fn`. Identify which local is borrowed across an `.await`, and explain in one sentence why the generated future is self-referential and therefore must be pinned before polling.

```rust
async fn render(input: String) -> usize {
    let trimmed: &str = input.trim();      // borrows `input`
    tokio::task::yield_now().await;        // suspension point
    trimmed.len()                          // uses the borrow after resume
}
```

<details>
<summary>Solution</summary>

`trimmed` is a `&str` that borrows `input`, and both `input` and `trimmed` are live across the `yield_now().await`. The compiler therefore stores *both* in the suspended state, where `trimmed` is a reference pointing into `input`'s buffer within the same future: a self-reference. If that future were moved between polls, `trimmed` would dangle, so the `Future::poll` signature (`Pin<&mut Self>`) forces the future to be pinned first, guaranteeing it never moves. You can confirm it compiles and runs:

```rust
async fn render(input: String) -> usize {
    let trimmed: &str = input.trim();
    tokio::task::yield_now().await;
    trimmed.len()
}

#[tokio::main]
async fn main() {
    let n = render(String::from("  hi there  ")).await;
    println!("{n}"); // prints: 8
}
```

(`cargo add tokio --features full`.) Output: `8`.

</details>

### Exercise 2: Pin and poll a future by hand

**Difficulty:** Intermediate

**Objective:** Use `std::pin::pin!` and a real `Waker` to drive a future to completion without a runtime.

**Instructions:** Write a `block_on` function that takes any `Future` and polls it in a loop until it returns `Poll::Ready`, returning the output. Use `std::pin::pin!` to pin the future and a no-op `Waker` (busy-poll; do not worry about real wakeups). Test it with an `async` block that adds two numbers.

```rust
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

fn block_on<F: Future>(future: F) -> F::Output {
    // TODO: pin the future, build a Context, loop on poll until Ready
    todo!()
}

fn main() {
    let out = block_on(async { 2 + 3 });
    println!("{out}");
}
```

<details>
<summary>Solution</summary>

```rust
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

struct NoopWaker;
impl Wake for NoopWaker {
    fn wake(self: Arc<Self>) {}
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future); // pin to this stack frame
    let waker = Waker::from(Arc::new(NoopWaker));
    let mut cx = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => continue, // busy-poll (toy executor)
        }
    }
}

fn main() {
    let out = block_on(async { 2 + 3 });
    println!("{out}"); // prints: 5
}
```

This is a (deliberately naive) executor: `pin!` gives us the `Pin<&mut F>` that `poll` demands, and `as_mut()` re-borrows it each iteration so we can poll repeatedly. A real executor would park the thread instead of busy-looping and use a `Waker` that actually re-schedules. Output: `5`.

</details>

### Exercise 3: A `!Unpin` type and the API it loses

**Difficulty:** Advanced

**Objective:** Demonstrate that adding `PhantomPinned` removes access to the safe `Pin` API, and confirm it with the real compiler error.

**Instructions:** Define a struct `Marker { id: u32, _pin: PhantomPinned }`. First, write an `assert_unpin::<T: Unpin>()` helper and show that `u32`, `String`, and a plain `struct Plain { id: u32 }` all pass it. Then attempt `Pin::new(&mut marker)` for your `!Unpin` `Marker` and record the compiler error. Finally, fix the pinning by using `Box::pin` instead, and read the `id` field back.

<details>
<summary>Solution</summary>

```rust
use std::marker::PhantomPinned;
use std::pin::Pin;

fn assert_unpin<T: Unpin>() {}

struct Plain {
    id: u32,
}

struct Marker {
    id: u32,
    _pin: PhantomPinned,
}

fn main() {
    // These all compile: every field is Unpin, so the type is Unpin.
    assert_unpin::<u32>();
    assert_unpin::<String>();
    assert_unpin::<Plain>();
    // assert_unpin::<Marker>(); // would NOT compile: Marker is !Unpin

    // `Pin::new` is unavailable for !Unpin types — this line, if uncommented,
    // fails with error[E0277]: `PhantomPinned` cannot be unpinned:
    // let mut m = Marker { id: 1, _pin: PhantomPinned };
    // let _p: Pin<&mut Marker> = Pin::new(&mut m); // does not compile

    // The fix: pin on the heap with Box::pin (no Unpin bound required).
    let pinned: Pin<Box<Marker>> = Box::pin(Marker { id: 42, _pin: PhantomPinned });
    println!("id = {}", pinned.id); // field access through Pin's Deref
}
```

Real output:

```text
id = 42
```

The exact error for the commented-out `Pin::new` line is the `error[E0277]: PhantomPinned cannot be unpinned` shown in *Common Pitfalls — Pitfall 2*, whose note recommends `Box::pin`. `Box::pin` is the safe way to pin a `!Unpin` value, and field reads still work through `Pin`'s `Deref` impl because reading does not move the value.

</details>
