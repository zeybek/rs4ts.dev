---
title: "Cell&lt;T&gt;: Cheap Interior Mutability for Copy Types"
description: "Cell<T> gives zero-cost interior mutability for Copy types, moving whole values in and out with get and set so a &self method can change a field without panicking."
---

## Quick Overview

`Cell<T>` is Rust's lightest-weight tool for **interior mutability**: it lets you mutate a value through a shared `&` reference, with **zero runtime cost** and no borrow tracking. The trade-off is that you can never get a reference *into* a `Cell`. You only ever **move whole values in and out** with `get` and `set`, which is why it shines for small `Copy` types like counters, flags, and IDs.

For a TypeScript/JavaScript developer, the surprising part is not the mutation (everything mutates in JS) but *why you'd need a special type for it at all*. In Rust, a `&self` method normally cannot change anything; `Cell<T>` is the escape hatch that keeps the method signature `&self` while still letting a field change.

---

## TypeScript/JavaScript Example

In JavaScript, an object method can freely mutate its own fields. There is no concept of a "read-only reference" enforced at the language level, so a `render()` method that bumps a private counter is completely ordinary:

```typescript
// widget.ts — a render counter on an otherwise "read-only" object
class Widget {
  label: string;
  private renderCount: number;

  constructor(label: string) {
    this.label = label;
    this.renderCount = 0;
  }

  // Looks like a read-only accessor, but it quietly mutates state.
  render(): string {
    this.renderCount += 1;
    return `${this.label} (rendered ${this.renderCount} times)`;
  }
}

const w = new Widget("Button");
console.log(w.render());
console.log(w.render());
console.log(w.render());
```

Running it with Node v22 (`node --experimental-strip-types widget.ts`) prints:

```text
Button (rendered 1 times)
Button (rendered 2 times)
Button (rendered 3 times)
```

JavaScript does not care that `render` "reads like" an accessor; there is no compile-time notion of immutability to violate. Even a `const w` only freezes the *binding*, not the object's fields (`w.renderCount` could still change). Rust draws that line much more strictly, and `Cell<T>` is how you cross it on purpose.

---

## Rust Equivalent

The naive Rust translation — a `render(&self)` that does `self.render_count += 1` — does **not** compile, because `&self` is a shared (read-only) reference. Wrapping the field in `Cell<u32>` makes it work while keeping the `&self` signature:

```rust playground
use std::cell::Cell;

struct Widget {
    label: String,
    render_count: Cell<u32>, // interior mutability for a Copy field
}

impl Widget {
    fn render(&self) -> String {
        // get() copies the value out, set() stores a new value back in.
        self.render_count.set(self.render_count.get() + 1);
        format!("{} (rendered {} times)", self.label, self.render_count.get())
    }
}

fn main() {
    let w = Widget {
        label: "Button".to_string(),
        render_count: Cell::new(0),
    };
    println!("{}", w.render());
    println!("{}", w.render());
    println!("{}", w.render());
}
```

This compiles and prints exactly the same output as the TypeScript version:

```text
Button (rendered 1 times)
Button (rendered 2 times)
Button (rendered 3 times)
```

> **Note:** `w` is **not** declared `mut`. That is the whole point: `Cell<T>`
> lets the inner value change even though the binding and the `&self` reference
> are immutable. The mutability is "interior" to the `Cell`.

---

## Detailed Explanation

### The problem `Cell` solves

Rust's borrow rules can be summed up as: at any moment a value may have **either** many shared `&` references **or** exactly one mutable `&mut` reference, never both. Methods that take `&self` therefore promise "I will not mutate." That promise is enforced by the compiler:

```rust
struct Widget {
    render_count: u32,
}
impl Widget {
    fn render(&self) {
        self.render_count += 1; // does not compile (error[E0594])
    }
}
fn main() {
    let w = Widget { render_count: 0 };
    w.render();
}
```

The real compiler error is:

```text
error[E0594]: cannot assign to `self.render_count`, which is behind a `&` reference
 --> src/main.rs:6:9
  |
6 |         self.render_count += 1; // does not compile (error[E0594])
  |         ^^^^^^^^^^^^^^^^^^^^^^ `self` is a `&` reference, so the data it refers to cannot be written
  |
help: consider changing this to be a mutable reference
  |
5 |     fn render(&mut self) {
  |                +++
```

Changing to `&mut self` is one fix, but it forces *every caller* to hold a unique mutable handle, impossible when the widget is shared (behind an `Rc`, inside a `Vec` you're iterating over, or used by multiple callbacks). `Cell<T>` lets you keep `&self` and still mutate. This is called **interior mutability**: from the outside the value looks immutable, but it carries a sanctioned mutable interior.

### `get` and `set`: move values, never borrow

`Cell<T>` deliberately exposes a tiny surface:

```rust playground
use std::cell::Cell;

fn main() {
    let counter = Cell::new(0);
    counter.set(counter.get() + 1); // read out, compute, store back
    counter.set(counter.get() + 1);
    println!("counter = {}", counter.get());

    // A shared &reference is enough to mutate — no `mut` anywhere.
    let c = &counter;
    c.set(42);
    println!("via shared ref = {}", counter.get());
}
```

Output:

```text
counter = 2
via shared ref = 42
```

- **`get(&self) -> T`** returns a **copy** of the inner value. It is only available when `T: Copy`, which is why `Cell` is a fit for numbers, `bool`, `char`, small `enum`s, and other `Copy` data.
- **`set(&self, value: T)`** overwrites the inner value, dropping the old one.

Importantly, neither method ever hands out a `&T` or `&mut T` pointing *inside* the `Cell`. Because no reference into the cell can exist, there is no way to observe a half-written value or to alias it, so no borrow checking is needed, and `Cell` is sound with literally zero bookkeeping. That is the deep reason `get` requires `Copy`: handing you a copy means you walk away with your *own* value, not a borrow of the cell's.

### Other useful methods

`Cell<T>` has a few more move-in/move-out helpers. These all take `&self`:

```rust playground
use std::cell::Cell;

fn main() {
    let a = Cell::new(10);

    let old = a.replace(20);          // set new, return the old value
    println!("replace: old={}, new={}", old, a.get());

    let taken = a.take();             // available when T: Default; leaves T::default()
    println!("take: taken={}, now={}", taken, a.get());

    let x = Cell::new(1);
    let y = Cell::new(2);
    x.swap(&y);                       // swap the contents of two cells
    println!("swap: x={}, y={}", x.get(), y.get());
}
```

Output:

```text
replace: old=10, new=20
take: taken=20, now=0
swap: x=2, y=1
```

There is also `update`, stabilized in Rust 1.88, which reads, applies a closure, and stores the result. Handy for the very common "increment" pattern:

```rust playground
use std::cell::Cell;

fn main() {
    let c = Cell::new(5);
    c.update(|x| x + 1); // equivalent to c.set(c.get() + 1)
    println!("{}", c.get());
}
```

Output:

```text
6
```

### When you *do* have `&mut`, the overhead vanishes

If you happen to hold a `&mut Cell<T>`, the cell stops being necessary, and Rust gives you direct, free access to the inside:

```rust playground
use std::cell::Cell;

fn main() {
    let mut c = Cell::new(5);

    // get_mut requires &mut self and returns a real &mut T into the cell.
    let inner: &mut i32 = c.get_mut();
    *inner += 10;
    println!("after get_mut: {}", c.get());

    // into_inner consumes the Cell and returns the owned value.
    let owned = c.into_inner();
    println!("into_inner: {}", owned);

    // Cell<T> is exactly the same size as T — no tag, no flag, no overhead.
    println!("size_of::<i32>()       = {}", std::mem::size_of::<i32>());
    println!("size_of::<Cell<i32>>() = {}", std::mem::size_of::<Cell<i32>>());
}
```

Output:

```text
after get_mut: 15
into_inner: 15
size_of::<i32>()       = 4
size_of::<Cell<i32>>() = 4
```

`get_mut` is safe precisely because `&mut self` proves no other reference to the cell exists, so a real `&mut T` cannot alias anything. And as the last two lines show, `Cell<i32>` occupies the same 4 bytes as a bare `i32`: wrapping a field in `Cell` costs nothing in memory.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust `Cell<T>` |
| --- | --- | --- |
| Default mutability | Everything mutable; no read-only references | `&self` is read-only; `Cell` is an explicit opt-in to mutate |
| How you read | `obj.field` returns the live value | `cell.get()` returns a **copy** (requires `T: Copy`) |
| How you write | `obj.field = x` | `cell.set(x)` |
| References into it | You always get a live reference to the object | **Never**: you cannot borrow inside a `Cell` |
| Runtime cost | N/A | **Zero**: no flags, locks, or counters |
| Thread safety | Single-threaded event loop by default | `!Sync`, single-thread only (use `Atomic*`/`Mutex` across threads) |
| Failure mode | Runtime `undefined`/exceptions | None — `Cell` cannot panic |

### `Cell` versus its siblings

The smart-pointer family splits the job of "mutate through a shared reference" along two axes: *what you store* and *single- vs multi-threaded*:

| Need | Reach for |
| --- | --- |
| Mutate a small `Copy` value, single thread, no overhead | **`Cell<T>`** (this file) |
| Mutate a non-`Copy` value (e.g. `String`, `Vec`) and hand out borrows, single thread | [`RefCell<T>`](/10-smart-pointers/02-refcell-mutex/) |
| Mutate shared state across threads | [`Mutex<T>` / `RwLock<T>`](/10-smart-pointers/02-refcell-mutex/) or `std::sync::atomic::*` |
| Share ownership of the cell among many holders | [`Rc<Cell<T>>`](/10-smart-pointers/01-rc-arc/) (single thread) |

The mental rule: **if `T: Copy` and you never need a borrow into it, prefer `Cell`**. It is the cheapest tool and it can never panic, unlike `RefCell`, which trades a runtime borrow check (and possible panic) for the ability to hand out references.

> **Tip:** A common idiom is a counter shared across closures or graph nodes:
> `Rc<Cell<u32>>`. The `Rc` shares ownership; the `Cell` provides the cheap
> mutation. You almost never want `Rc<RefCell<u32>>` for a plain integer.

---

## Common Pitfalls

### Pitfall 1: Trying to use `Cell` with a non-`Copy` type and calling `get`

`get` clones the value out by copying, so it is gated on `T: Copy`. A `Cell<String>` compiles, but `.get()` on it does not:

```rust
use std::cell::Cell;

fn main() {
    let c = Cell::new(String::from("hi"));
    let s = c.get(); // does not compile (error[E0599]): String is not Copy
    println!("{}", s);
}
```

Real compiler error:

```text
error[E0599]: the method `get` exists for struct `Cell<String>`, but its trait bounds were not satisfied
 --> src/main.rs:5:15
  |
5 |     let s = c.get(); // does not compile (error[E0599]): String is not Copy
  |               ^^^
  |
 ::: .../library/alloc/src/string.rs:360:1
  |
  | pub struct String {
  | ----------------- doesn't satisfy `String: Copy`
  |
  = note: the following trait bounds were not satisfied:
          `String: Copy`
```

**Fix:** For non-`Copy` data you want to mutate behind `&self`, use [`RefCell<T>`](/10-smart-pointers/02-refcell-mutex/) (which hands out borrows), or use `Cell`'s move-only methods like `take`/`replace` that do not require `Copy`. For example `c.take()` works on `Cell<String>` because it moves the `String` out and leaves an empty `String` (`Default`) behind.

### Pitfall 2: Expecting to borrow the value inside a `Cell`

There is intentionally **no** `&self` method that returns a reference into a `Cell`. If you find yourself wanting `&cell.contents` to, say, iterate over a `Vec` stored inside, `Cell` is the wrong tool. You would have to `get()` a whole copy out (impossible for a `Vec` since it is not `Copy`) or `take()` it, mutate the owned copy, and `set()` it back. That is awkward by design.

**Fix:** Reach for [`RefCell<T>`](/10-smart-pointers/02-refcell-mutex/), whose `borrow()` / `borrow_mut()` give you references into the contents (at the cost of a runtime borrow check that can panic).

### Pitfall 3: Sharing a `Cell` across threads

`Cell<T>` is `!Sync`: a `&Cell<T>` may not be sent to another thread, because two threads writing to the same cell with no synchronization is a data race. The compiler stops you:

```rust
use std::cell::Cell;
use std::thread;

fn main() {
    let counter = Cell::new(0);
    thread::scope(|s| {
        s.spawn(|| {
            counter.set(counter.get() + 1); // does not compile (error[E0277])
        });
    });
    println!("{}", counter.get());
}
```

Real compiler error (abbreviated):

```text
error[E0277]: `Cell<i32>` cannot be shared between threads safely
 --> src/main.rs:7:17
  |
7 |         s.spawn(|| {
  |                 ^^ `Cell<i32>` cannot be shared between threads safely
  |
  = help: the trait `Sync` is not implemented for `Cell<i32>`
  = note: if you want to do aliasing and mutation between multiple threads, use `std::sync::RwLock` or `std::sync::atomic::AtomicI32` instead
  = note: required for `&Cell<i32>` to implement `Send`
```

**Fix:** As the compiler itself suggests, use an atomic such as `std::sync::atomic::AtomicI32` for a shared counter, or a `Mutex`/`RwLock` for larger state. See [RefCell vs Mutex](/10-smart-pointers/02-refcell-mutex/) and the [async section](/11-async/) for shared-state patterns across tasks.

### Pitfall 4: Reaching for `Cell` (or `RefCell`) too early

Coming from JavaScript, interior mutability feels natural and you may sprinkle `Cell` everywhere. In idiomatic Rust it is the *exception*, not the rule. If a plain `&mut self` works — and it usually does — prefer it: the compiler then guarantees no aliasing for free.

**Fix:** Default to ordinary ownership and `&mut`. Reach for `Cell` only when you genuinely need to mutate through a shared reference (shared graph nodes, observer counters, flags inside an `Rc`, caches keyed by `&self`).

---

## Best Practices

- **Use `Cell` for small `Copy` state mutated behind `&self`:** counters, version numbers, generation IDs, dirty/visited flags, cached `Option<T>` of a `Copy` value.
- **Prefer `Cell` over `RefCell` when `T: Copy`:** it is cheaper and *cannot panic*. Only step up to `RefCell` when you need to borrow into a non-`Copy` value.
- **Reach for `update` for read-modify-write:** `c.update(|n| n + 1)` is clearer than `c.set(c.get() + 1)` and avoids repeating the binding.
- **Pair with `Rc` for shared, mutable, single-threaded state:** `Rc<Cell<T>>` is the canonical "shared counter" type; see [Rc/Arc](/10-smart-pointers/01-rc-arc/).
- **Switch to atomics across threads:** `Cell` is single-threaded by design. The thread-safe analogues are `AtomicUsize`, `AtomicBool`, etc., or a `Mutex` for larger data.
- **Keep the `Cell` private:** expose intent-revealing methods (`record`, `next_id`) rather than leaking the `Cell` field, so callers cannot accidentally `set` arbitrary values.

---

## Real-World Example

A request-metrics recorder is a perfect fit: handlers usually receive `&self` (the metrics object is shared), the tracked values are all `Copy`, and you never need to borrow *into* them: just bump counters and read totals.

```rust playground
use std::cell::Cell;

/// Tracks request statistics while exposing only `&self` methods, so it can
/// live behind a shared reference (e.g. inside an `Rc` or a handler struct).
#[derive(Debug)]
struct RequestMetrics {
    total: Cell<u64>,
    errors: Cell<u64>,
    last_status: Cell<u16>,
}

impl RequestMetrics {
    fn new() -> Self {
        RequestMetrics {
            total: Cell::new(0),
            errors: Cell::new(0),
            last_status: Cell::new(0),
        }
    }

    /// Note: `&self`, not `&mut self`. Callers do not need a mutable handle.
    fn record(&self, status: u16) {
        self.total.update(|t| t + 1);
        if status >= 500 {
            self.errors.update(|e| e + 1);
        }
        self.last_status.set(status);
    }

    fn error_rate(&self) -> f64 {
        let total = self.total.get();
        if total == 0 {
            0.0
        } else {
            self.errors.get() as f64 / total as f64
        }
    }
}

fn main() {
    let metrics = RequestMetrics::new();

    for status in [200, 200, 500, 404, 503, 200] {
        metrics.record(status); // shared &self call, no `mut metrics` needed
    }

    println!("total requests: {}", metrics.total.get());
    println!("server errors:  {}", metrics.errors.get());
    println!("last status:    {}", metrics.last_status.get());
    println!("error rate:     {:.1}%", metrics.error_rate() * 100.0);
}
```

Output:

```text
total requests: 6
server errors:  2
last status:    200
error rate:     33.3%
```

Notice that `metrics` is never `mut`, yet every call to `record` mutates three fields. If `RequestMetrics` lived inside an `Rc` shared among several handlers (see [Rc/Arc](/10-smart-pointers/01-rc-arc/)), this exact code would still work. That is the payoff of choosing `Cell` over `&mut self`. If you needed thread-safe metrics, you would swap each `Cell` for an `AtomicU64`/`AtomicU16` and the API would barely change.

---

## Further Reading

### Official Documentation

- [`std::cell::Cell` API docs](https://doc.rust-lang.org/std/cell/struct.Cell.html)
- [`std::cell` module overview](https://doc.rust-lang.org/std/cell/index.html): the canonical explanation of interior mutability
- [The Rust Book — `RefCell<T>` and the Interior Mutability Pattern](https://doc.rust-lang.org/book/ch15-05-interior-mutability.html)
- [`std::sync::atomic`](https://doc.rust-lang.org/std/sync/atomic/index.html): the thread-safe counterparts to `Cell`

### Related Topics

- [RefCell and Mutex](/10-smart-pointers/02-refcell-mutex/): interior mutability for non-`Copy` data and across threads
- [Rc and Arc](/10-smart-pointers/01-rc-arc/) — shared ownership; pairs with `Cell` as `Rc<Cell<T>>`
- [Box](/10-smart-pointers/00-box/): heap allocation, the simplest smart pointer
- [Weak](/10-smart-pointers/05-weak/) — breaking reference cycles in shared graphs
- [Cow](/10-smart-pointers/04-cow/) — clone-on-write, another "avoid needless work" pattern
- [Smart Pointer Comparison](/10-smart-pointers/07-comparison/) — a decision guide for which pointer to use when
- [Variables and Mutability](/02-basics/00-variables/): why `&self` is read-only in the first place
- [Ownership](/05-ownership/) — the borrow rules `Cell` carefully sidesteps
- [Getting Started](/01-getting-started/) and [Introduction](/00-introduction/) — if you are new to the guide
- [Async](/11-async/) — shared-state patterns across tasks (where `Cell` does *not* apply)

---

## Exercises

### Exercise 1

**Difficulty:** Beginner

**Objective:** Use `Cell` to mutate a field through a `&self` method.

**Instructions:** The following code does not compile because `redraw` tries to mutate `redraws` through `&self`. Change the field type so it compiles, without changing the method signatures or adding `mut` to the binding in `main`.

```rust
struct View {
    redraws: u32, // change me
}
impl View {
    fn redraw(&self) {
        self.redraws += 1; // must keep &self
    }
}
fn main() {
    let v = View { redraws: 0 }; // must stay non-mut
    v.redraw();
    v.redraw();
    println!("redraws = {}", v.redraws);
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::cell::Cell;

struct View {
    redraws: Cell<u32>,
}
impl View {
    fn redraw(&self) {
        self.redraws.set(self.redraws.get() + 1);
    }
}
fn main() {
    let v = View { redraws: Cell::new(0) };
    v.redraw();
    v.redraw();
    println!("redraws = {}", v.redraws.get());
}
```

Output:

```text
redraws = 2
```

</details>

### Exercise 2

**Difficulty:** Intermediate

**Objective:** Build a monotonic ID generator that hands out unique IDs through a shared reference.

**Instructions:** Implement `IdGenerator` so that `next_id(&self)` returns `1`, then `2`, then `3`, ... on successive calls. The generator must work through `&self` (no `&mut`). Try to use a single `Cell` method for the read-and-bump.

```rust
struct IdGenerator {
    // TODO
}
impl IdGenerator {
    fn new() -> Self {
        // TODO
    }
    fn next_id(&self) -> u64 {
        // TODO: return current value, then increment
    }
}
fn main() {
    let id_gen = IdGenerator::new();
    println!("{} {} {}", id_gen.next_id(), id_gen.next_id(), id_gen.next_id());
    // expected: 1 2 3
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::cell::Cell;

struct IdGenerator {
    next: Cell<u64>,
}
impl IdGenerator {
    fn new() -> Self {
        IdGenerator { next: Cell::new(1) }
    }
    fn next_id(&self) -> u64 {
        // replace stores `current + 1` and returns the previous value.
        self.next.replace(self.next.get() + 1)
    }
}
fn main() {
    let id_gen = IdGenerator::new();
    println!("{} {} {}", id_gen.next_id(), id_gen.next_id(), id_gen.next_id());
}
```

Output:

```text
1 2 3
```

> Note: `next` is the variable name here, not `gen`; in the latest stable
> edition (2024), `gen` is a reserved keyword (for `gen` blocks) and cannot be
> used as a plain identifier.

</details>

### Exercise 3

**Difficulty:** Advanced

**Objective:** Share a single counter across multiple closures using `Rc<Cell<T>>`.

**Instructions:** Write `make_clicker()` that returns a shared counter handle plus a closure. Each call to the closure increments the counter; reading the returned handle reflects every click. This mirrors a UI event handler holding a shared counter. (Hint: clone the `Rc` so the closure owns its own handle.)

```rust
use std::cell::Cell;
use std::rc::Rc;

fn make_clicker() -> (Rc<Cell<u32>>, impl Fn()) {
    // TODO
}

fn main() {
    let (count, click) = make_clicker();
    click();
    click();
    click();
    println!("clicks = {}", count.get()); // expected: 3
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::cell::Cell;
use std::rc::Rc;

fn make_clicker() -> (Rc<Cell<u32>>, impl Fn()) {
    let count = Rc::new(Cell::new(0));
    let count_for_closure = Rc::clone(&count);
    let on_click = move || {
        count_for_closure.set(count_for_closure.get() + 1);
    };
    (count, on_click)
}

fn main() {
    let (count, click) = make_clicker();
    click();
    click();
    click();
    println!("clicks = {}", count.get());
}
```

Output:

```text
clicks = 3
```

The `Rc` gives both the returned handle and the closure shared ownership of the
same `Cell`; the `Cell` provides the cheap, panic-free mutation. For a non-`Copy`
payload you would instead use `Rc<RefCell<T>>`; see [Rc/Arc](/10-smart-pointers/01-rc-arc/) and
[RefCell/Mutex](/10-smart-pointers/02-refcell-mutex/).

</details>
