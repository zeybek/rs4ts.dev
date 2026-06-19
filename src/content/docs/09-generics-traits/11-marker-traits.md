---
title: "Marker Traits: `Copy`, `Sized`, `Send`, and `Sync`"
description: "Rust marker traits Copy, Sized, Send, and Sync have no methods; they tell the compiler facts about a type, enabling bitwise copies and safe threading."
---

Some Rust traits have no methods at all. They exist purely to **mark** a type with a property the compiler cares about: "this type is safe to copy bit-for-bit," "this type has a known size," "this type may move between threads." TypeScript has no real equivalent: these are compile-time facts the Rust compiler tracks and enforces for you, mostly without you writing a single line.

---

## Quick Overview

A **marker trait** is a trait with no methods or associated items; implementing it simply asserts a fact about the type. The four you will meet first are `Copy` (cheap bitwise duplication), `Sized` (the size is known at compile time), and the **auto traits** `Send` and `Sync` (the type is safe to move to, or share with, another thread). You almost never call methods on these. Instead, the compiler reads them to decide what your code is allowed to do, which is how Rust delivers data-race-free threading without a runtime.

> **Note:** This file focuses on the four foundational marker traits. Bounding generics on traits in general is covered in [Trait Bounds](/09-generics-traits/05-trait-bounds/); the threading machinery (`Arc`, `Mutex`) lives under [smart pointers](/10-smart-pointers/) and the async sections.

---

## TypeScript/JavaScript Example

TypeScript has nothing that directly corresponds to a marker trait, so the closest thing is to contrast the two "facts" a TypeScript developer already reasons about informally: **assignment aliases an object** (it never copies), and **the language has no compiler-enforced thread-safety** because the main thread is single-threaded.

```typescript
// TypeScript / JavaScript — assignment shares a reference; nothing is copied.
const original = { r: 255, g: 128, b: 0 };

const aliased = original; // `aliased` and `original` point at the SAME object
aliased.g = 0;
console.log(original.g); // 0 — the mutation is visible through both names

// To actually duplicate, you opt in explicitly:
const copied = { ...original }; // shallow copy (or structuredClone for deep)
copied.r = 10;
console.log(original.r, copied.r); // 255 10 — now they are independent

console.log(original); // { r: 255, g: 0, b: 0 }  (this is how Node prints it)
```

**Key points for a TypeScript developer:**

- Objects are always passed and assigned **by reference**. There is no concept of "this object is cheap enough to copy automatically."
- Worker threads in Node receive **structured-cloned** copies of data, and the runtime decides what is and is not transferable at runtime (e.g. a `function` cannot be cloned and throws a `DataCloneError`). Nothing is checked at compile time.
- TypeScript's type system is **erased** at runtime, so it can never enforce a property like "this value is safe to share across threads."

Rust turns each of these informal ideas into a trait the compiler enforces.

---

## Rust Equivalent

Here the marker traits do their work. `Copy` makes a small struct duplicate on assignment instead of move; the implicit `Sized` bound lets generics accept normal values; and `Send`/`Sync` are checked when we cross a thread boundary.

```rust
use std::sync::{Arc, Mutex};
use std::thread;

// `Copy` says: duplicating this value is just a bitwise memcpy, so assignment
// copies instead of moving. We can only derive it because every field is Copy.
#[derive(Debug, Clone, Copy)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

fn show(color: Rgb) {
    println!("rgb({}, {}, {})", color.r, color.g, color.b);
}

fn main() {
    let orange = Rgb { r: 255, g: 128, b: 0 };
    show(orange);
    show(orange); // still valid: `Copy` duplicated it instead of moving it
    println!("original still usable: {orange:?}");

    // `Arc<Mutex<T>>` is `Send + Sync`, so the compiler lets it cross threads.
    let counter = Arc::new(Mutex::new(0));
    let mut handles = Vec::new();
    for _ in 0..5 {
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            *counter.lock().unwrap() += 1;
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    println!("final count = {}", *counter.lock().unwrap());
}
```

Running it:

```text
rgb(255, 128, 0)
rgb(255, 128, 0)
original still usable: Rgb { r: 255, g: 128, b: 0 }
final count = 5
```

The `show(orange)` call appears twice and compiles, because `Rgb` is `Copy`. Swap a `String` field into `Rgb` and the second call would fail to compile, because the value would have **moved**. That single trait flips the most fundamental rule of the language for a type.

---

## Detailed Explanation

### `Copy` — duplicate by `memcpy`, no move

By default, assigning or passing a value **moves** it (see [Section 05: Ownership](/05-ownership/)). A type that implements `Copy` opts out of move semantics: the value is duplicated with a trivial bit-for-bit copy, and the original stays valid.

- `Copy` is a **supertrait** relationship away from `Clone`: every `Copy` type must also be `Clone`, which is why we derive `#[derive(Clone, Copy)]` together. `Clone` is the explicit, possibly-expensive `.clone()`; `Copy` is the implicit, always-cheap duplication the compiler inserts for you.
- A type can be `Copy` only if **all of its fields are `Copy`**. Integers, floats, `bool`, `char`, shared references `&T`, and tuples/arrays of `Copy` types qualify. `String`, `Vec<T>`, `Box<T>`, and `&mut T` do **not**, because they own a resource (heap allocation, unique borrow) that cannot be meaningfully duplicated by a `memcpy`.
- You implement `Copy` by deriving it; never write the body, because there is nothing to write. It is a pure marker.

> **Tip:** Reach for `Copy` on small, value-like types: coordinates, IDs, flags, enums of unit variants. Skip it for anything that owns heap data; cloning those should be a visible `.clone()` call so the cost is obvious at the call site.

### `Sized` — the size is known at compile time

`Sized` marks types whose size is known at compile time (`i32` is 4 bytes; `Rgb` is 3 bytes). This is the most invisible marker trait, because **every generic type parameter is implicitly `Sized`**. When you write `fn first_or<T>(...)`, the compiler silently rewrites it as `fn first_or<T: Sized>(...)`.

The unsized (or **dynamically sized**) types you will encounter are `str` and `[T]` (slices), plus `dyn Trait` trait objects. You never hold these by value. You hold them behind a pointer (`&str`, `Box<[T]>`, `&dyn Trait`), and the pointer is `Sized` even though the thing it points to is not.

To accept an unsized type in a generic, relax the bound with `?Sized` ("may or may not be sized"):

```rust
// `?Sized` lets this accept `str` (unsized) behind a reference, not just `String`.
fn print_len<T: AsRef<str> + ?Sized>(s: &T) {
    println!("len = {}", s.as_ref().len());
}

fn main() {
    print_len("hello");                 // &str — `str` is unsized
    print_len(&String::from("world"));  // &String — sized, also fine
}
```

```text
len = 5
len = 5
```

The `?Sized` bound applies to the type behind the reference, so `T` is allowed to be `str`. Note we must take `&T`, never `T`, because an unsized `T` cannot be passed by value.

### `Send` and `Sync` — the auto traits behind fearless concurrency

`Send` and `Sync` are **auto traits**: the compiler implements them for your type *automatically* if all of its fields already implement them. You do not write `impl Send for MyType {}`; composition handles it.

- **`Send`** means a value of the type can be **moved to another thread**. Almost everything is `Send`. The famous exception is `Rc<T>` (the single-threaded reference counter), whose non-atomic count would race if shared across threads.
- **`Sync`** means `&T` is `Send` — i.e. a **shared reference** can be handed to another thread, so the type can be accessed from multiple threads at once. Formally, `T: Sync` if and only if `&T: Send`. `Cell<T>` and `RefCell<T>` are `Send` but **not** `Sync`, because their interior mutability is not synchronized.

These are exactly the traits `std::thread::spawn` requires:

```rust
// std signature (abridged): the closure and its captures must be Send + 'static.
// pub fn spawn<F, T>(f: F) -> JoinHandle<T> where F: Send + 'static, ...
```

Because the bound is `Send`, the compiler statically rejects any attempt to capture a non-`Send` value (like `Rc`) in a thread closure. There is no runtime check and no data race: the program simply does not compile. That is what the Rust community calls **fearless concurrency**.

> **Note:** `Send`/`Sync` are `unsafe` traits to implement *by hand*, precisely because getting them wrong reintroduces data races. You will essentially never implement them manually; you rely on the automatic derivation and, when you need shared mutable state across threads, you reach for `Arc<Mutex<T>>` or `Arc<RwLock<T>>` (see [smart pointers](/10-smart-pointers/)).

### Opting out

Auto traits can be *withheld* by including a non-`Send`/non-`Sync` field. The classic device is `PhantomData<*const ()>` (raw pointers are neither `Send` nor `Sync`), which removes the auto traits from a wrapper without changing its runtime layout. You rarely need this, but it explains why some standard types are deliberately not thread-safe.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Duplicating a value | Always a reference alias; you opt in to copying (`{...obj}`, `structuredClone`) | `Copy` types duplicate automatically; everything else **moves** |
| Knowing a type's size | Irrelevant: everything is a boxed reference at runtime | `Sized` is tracked and implicitly required on every generic |
| Thread safety | Not expressible in the type system; runtime `DataCloneError` at worst | `Send`/`Sync` are compiler-enforced *before* the program runs |
| Who implements the trait | N/A | Auto traits (`Send`/`Sync`) are derived by the compiler; `Copy`/`Sized` you derive or get for free |
| Methods on the trait | N/A | None: these are pure markers read by the compiler |
| Cost of getting it wrong | Runtime exception or silent data race | Compile error (`E0277`, `E0382`, `E0204`) |

The headline mental shift: in TypeScript, "is this safe to share between workers?" is a question you answer at runtime (and often get wrong). In Rust it is a *property of the type*, checked at compile time, and you almost never have to think about it because the compiler tracks it for you.

> **Warning:** Do not equate `Copy` with TypeScript's spread `{...obj}`. The spread is a shallow copy you write explicitly; `Copy` is an *automatic* full duplication the compiler inserts, and it is only legal when the bytes are self-contained (no owned heap data). They solve related problems but live at opposite ends of the explicit/implicit spectrum.

---

## Common Pitfalls

### Pitfall 1: Trying to derive `Copy` on a type with an owning field

A TypeScript developer often assumes any struct can be `Copy`. But a type is `Copy` only if every field is.

```rust
// does not compile (error[E0204])
#[derive(Clone, Copy)]
struct Wrapper {
    label: String, // String owns heap data — not Copy
}

fn main() {
    let _w = Wrapper { label: "x".into() };
}
```

Real compiler output:

```text
error[E0204]: the trait `Copy` cannot be implemented for this type
 --> src/main.rs:2:17
  |
2 | #[derive(Clone, Copy)]
  |                 ^^^^
3 | struct Wrapper {
4 |     label: String, // String is not Copy
  |     ------------- this field does not implement `Copy`

For more information about this error, try `rustc --explain E0204`.
```

**Fix:** drop `Copy` and keep `Clone`. Use an explicit `.clone()` when you need a second owned copy; the cost is then visible.

### Pitfall 2: Assuming a non-`Copy` value survives being passed by value

Without `Copy`, passing a value into a function moves it, and the original binding is dead.

```rust
// does not compile (error[E0382])
#[derive(Debug, Clone)] // no Copy
struct Config {
    name: String,
}

fn consume(c: Config) {
    println!("{c:?}");
}

fn main() {
    let cfg = Config { name: "prod".into() };
    consume(cfg);
    consume(cfg); // second use after move
}
```

Real compiler output (trimmed):

```text
error[E0382]: use of moved value: `cfg`
  --> src/main.rs:13:13
   |
11 |     let cfg = Config { name: "prod".into() };
   |         --- move occurs because `cfg` has type `Config`, which does not implement the `Copy` trait
12 |     consume(cfg);
   |             --- value moved here
13 |     consume(cfg); // second use after move
   |             ^^^ value used here after move
   |
help: consider cloning the value if the performance cost is acceptable
   |
12 |     consume(cfg.clone());
   |                ++++++++
```

**Fix:** pass a borrow (`&Config`) if `consume` only needs to read, or `.clone()` if it needs its own copy. This is the same decision you make everywhere ownership applies; `Copy` is just the special case where the compiler makes it for you.

### Pitfall 3: Capturing an `Rc` (or other non-`Send` value) in a thread

`Rc<T>` is the single-threaded reference counter and is deliberately **not** `Send`. Try to send it to a thread and the compiler stops you cold.

```rust
// does not compile (error[E0277]: `Rc<...>` cannot be sent between threads safely)
use std::rc::Rc;
use std::thread;

fn main() {
    let shared = Rc::new(vec![1, 2, 3]);
    let shared2 = Rc::clone(&shared);
    thread::spawn(move || {
        println!("{:?}", shared2);
    });
    println!("{:?}", shared);
}
```

Real compiler output (trimmed):

```text
error[E0277]: `Rc<Vec<i32>>` cannot be sent between threads safely
   --> src/main.rs:8:19
    |
  8 |       thread::spawn(move || {
    |       ------------- ^------
    |       |             |
    |  _____|_____________within this `{closure@src/main.rs:8:19: 8:26}`
    | |     |
    | |     required by a bound introduced by this call
    | |_____^ `Rc<Vec<i32>>` cannot be sent between threads safely
    |
    = help: within `{closure@...}`, the trait `Send` is not implemented for `Rc<Vec<i32>>`
note: required by a bound in `spawn`
```

**Fix:** use `Arc<T>` (the *atomic* reference counter), which **is** `Send + Sync`. This is the entire reason both types exist: `Rc` is cheaper but single-threaded, `Arc` pays for atomic counters and earns thread-safety.

### Pitfall 4: Forgetting `?Sized` and trying to pass an unsized value

If a generic implicitly requires `Sized`, you cannot pass `str` or `[T]` by value.

```rust
// does not compile (error[E0277]: the size for values of type `str` cannot be known)
fn describe<T: std::fmt::Debug>(_value: T) {}

fn main() {
    let s: &str = "hi";
    describe(*s); // dereferences to `str`, which is unsized
}
```

Real compiler output (trimmed):

```text
error[E0277]: the size for values of type `str` cannot be known at compilation time
 --> src/main.rs:6:14
  |
6 |     describe(*s);
  |     -------- ^^ doesn't have a size known at compile-time
  |
  = help: the trait `Sized` is not implemented for `str`
note: required by an implicit `Sized` bound in `describe`
help: consider relaxing the implicit `Sized` restriction
  |
2 | fn describe<T: std::fmt::Debug + ?Sized>(_value: T) {}
  |                                ++++++++
```

**Fix:** add `+ ?Sized` *and* take the value behind a reference (`_value: &T`). The compiler's own suggestion points the way.

---

## Best Practices

- **Derive `Copy` for small, plain-data types** (coordinates, IDs, flag enums) where bitwise duplication is the natural semantics. Always pair it with `Clone`: `#[derive(Clone, Copy)]`. Skip `Copy` for anything that owns heap data so that duplication stays an explicit `.clone()`.
- **Never implement `Send`/`Sync` by hand.** Let the compiler derive them. If a type is missing `Send`/`Sync`, that is a signal: find the offending field (often an `Rc`, `Cell`, or raw pointer) rather than forcing an `unsafe impl`.
- **Default to `Arc<Mutex<T>>` (or `Arc<RwLock<T>>`) for shared mutable state across threads,** and `Arc<T>` for shared read-only data. Use `Rc<T>`/`RefCell<T>` only when you know the data never crosses a thread boundary.
- **Add `Send + Sync + 'static` bounds when you store callbacks, handlers, or spawn work,** so the type system documents and enforces "this must be thread-safe and self-owned." A `Box<dyn Fn() + Send + Sync + 'static>` is the canonical thread-safe handler type.
- **Use `?Sized` on borrow-only generic parameters** (`fn f<T: ?Sized>(x: &T)`) to accept slices, `str`, and trait objects in addition to sized types. This is how `AsRef<str>`-style flexible APIs are built.
- **Verify a type's marker traits with a one-line compile-time assertion** when you want a guarantee documented in code: `fn assert_send_sync<T: Send + Sync>() {}` then `assert_send_sync::<MyType>();`. If it stops compiling later, you broke thread-safety.

```rust
// Compile-time proof that types have the marker traits you expect.
fn assert_send_sync<T: Send + Sync>() {}
fn assert_send<T: Send>() {}

use std::cell::Cell;

fn main() {
    assert_send_sync::<i32>();
    assert_send_sync::<String>();
    assert_send_sync::<std::sync::Arc<i32>>();
    assert_send::<Cell<i32>>(); // Cell is Send but NOT Sync, so only assert Send
    println!("all the asserted bounds hold");
}
```

```text
all the asserted bounds hold
```

---

## Real-World Example

A production-flavored **thread-safe event bus**: subscribers register handler closures by topic, and `publish` fans an event out to every handler in parallel. The marker traits are the load-bearing part of the design — the handler type bound `Fn(&str) + Send + Sync + 'static` is exactly what lets handlers be stored, cloned across threads, and run concurrently, and the compiler enforces it.

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

/// A handler is any closure that processes an event payload. The bound
/// `Send + Sync + 'static` is what makes it safe to share across worker
/// threads and store for the lifetime of the bus.
type Handler = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// A minimal thread-safe event bus. `Arc<Mutex<...>>` is `Send + Sync`,
/// so the whole bus can be cloned and shared between threads.
#[derive(Clone)]
struct EventBus {
    handlers: Arc<Mutex<HashMap<String, Vec<Handler>>>>,
}

impl EventBus {
    fn new() -> Self {
        EventBus {
            handlers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn subscribe<F>(&self, topic: &str, handler: F)
    where
        F: Fn(&str) + Send + Sync + 'static, // the marker bounds, made explicit
    {
        let mut map = self.handlers.lock().unwrap();
        map.entry(topic.to_string())
            .or_default()
            .push(Arc::new(handler));
    }

    fn publish(&self, topic: &str, payload: &str) {
        // Snapshot the handlers, then drop the lock before running them.
        let handlers = {
            let map = self.handlers.lock().unwrap();
            map.get(topic).cloned().unwrap_or_default()
        };

        let mut threads = Vec::new();
        for handler in handlers {
            let payload = payload.to_string();
            // Both `handler` (an Arc) and `payload` (a String) are Send,
            // so the compiler allows this move into the thread closure.
            threads.push(thread::spawn(move || handler(&payload)));
        }
        for t in threads {
            t.join().unwrap();
        }
    }
}

fn main() {
    let bus = EventBus::new();
    let counter = Arc::new(Mutex::new(0));

    let audit_counter = Arc::clone(&counter);
    bus.subscribe("order.created", move |payload| {
        *audit_counter.lock().unwrap() += 1;
        println!("audit: order event -> {payload}");
    });

    bus.subscribe("order.created", |payload| {
        println!("email: confirming {payload}");
    });

    bus.publish("order.created", "#1042");
    bus.publish("order.created", "#1043");

    println!("audit handler fired {} times", *counter.lock().unwrap());
}
```

One real run (handlers for a single publish run on separate threads, so the relative order of the two lines within a publish is non-deterministic):

```text
audit: order event -> #1042
email: confirming #1042
audit: order event -> #1043
email: confirming #1043
audit handler fired 2 times
```

If you ever change `Handler` to `Rc<dyn Fn(&str)>`, this program stops compiling at the `thread::spawn` call: the `Send` bound fails exactly as in Pitfall 3. The marker traits are doing real work: they are the difference between a compiler-guaranteed-safe concurrent bus and a latent data race.

---

## Further Reading

### Official Documentation

- [The Rust Book — Fearless Concurrency (`Send` and `Sync`)](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html)
- [The Rust Reference — Special types and traits (`Copy`, `Sized`, `Send`, `Sync`)](https://doc.rust-lang.org/reference/special-types-and-traits.html)
- [`std::marker::Copy`](https://doc.rust-lang.org/std/marker/trait.Copy.html) · [`std::marker::Sized`](https://doc.rust-lang.org/std/marker/trait.Sized.html) · [`std::marker::Send`](https://doc.rust-lang.org/std/marker/trait.Send.html) · [`std::marker::Sync`](https://doc.rust-lang.org/std/marker/trait.Sync.html)
- [The Rustonomicon — Send and Sync](https://doc.rust-lang.org/nomicon/send-and-sync.html) (advanced; for when you genuinely need to reason about manual impls)
- [The Rust Reference — Dynamically sized types](https://doc.rust-lang.org/reference/dynamically-sized-types.html)

### Related Topics in This Guide

- [Ownership](/05-ownership/) — why move semantics is the default that `Copy` opts out of
- [Variables](/02-basics/00-variables/): first contact with move vs. copy at the binding level
- [Traits](/09-generics-traits/03-traits/) — what a trait is and how `impl Trait for Type` works
- [Trait Bounds](/09-generics-traits/05-trait-bounds/): how `Send + Sync + 'static` bounds constrain generics
- [Generic Functions](/09-generics-traits/00-generic-functions/) — where the implicit `Sized` bound is added
- [Operator Overloading](/09-generics-traits/10-operator-overloading/): another family of traits the compiler wires up for you
- [The Orphan Rule](/09-generics-traits/12-orphan-rule/) — coherence rules that also govern auto traits
- [Smart Pointers](/10-smart-pointers/): `Rc` vs. `Arc`, `RefCell` vs. `Mutex`, and where the marker traits decide which you need

---

## Exercises

### Exercise 1 — Make a value type `Copy`

**Difficulty:** Easy

**Objective:** Recognize when a type qualifies for `Copy` and observe how it changes pass-by-value behavior.

**Instructions:** Define an `Rgb` color struct with three `u8` fields. Derive the traits that let you (a) duplicate it on assignment without moving, (b) compare two colors with `==`, and (c) print it with `{:?}`. Add a `luminance(self) -> f64` method (use `0.299*r + 0.587*g + 0.114*b`). In `main`, bind a color, assign it to a second variable, call `luminance` on the *first* one afterward, and confirm both names still work.

```rust
// TODO: derive the right traits so this struct is Copy, comparable, and printable
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

fn main() {
    let white = Rgb { r: 255, g: 255, b: 255 };
    let copy = white;
    // TODO: print white.luminance() and whether white == copy
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

impl Rgb {
    fn luminance(self) -> f64 {
        0.299 * self.r as f64 + 0.587 * self.g as f64 + 0.114 * self.b as f64
    }
}

fn main() {
    let white = Rgb { r: 255, g: 255, b: 255 };
    let copy = white; // Copy: a bitwise duplicate; `white` stays valid
    println!("luminance = {:.1}", white.luminance());
    println!("equal? {}", white == copy);
}
```

**Output:**

```text
luminance = 255.0
equal? true
```

Because every field (`u8`) is `Copy`, the struct can derive `Copy`. The `self` receiver on `luminance` takes the value by copy, so `white` remains usable afterward.

</details>

### Exercise 2 — Accept both owned and borrowed strings with `?Sized`

**Difficulty:** Medium

**Objective:** Use `?Sized` to write a function that accepts string literals, `&str`, and `&String` alike.

**Instructions:** Write a generic `log_line` that takes a `prefix: &str` and a message, and prints `[prefix] message`. Bound the message type so it works for `&str`, `&String`, and a `&str` slice without separate overloads. (Hint: `AsRef<str>` plus a relaxed `Sized` bound, taken by reference.)

```rust
// TODO: bound T so this compiles for &str, &String, and string slices
fn log_line<T>(prefix: &str, message: &T) {
    println!("[{prefix}] {}", /* ??? */);
}

fn main() {
    log_line("info", "starting up");
    let owned = String::from("connection lost");
    log_line("warn", &owned);
    log_line("warn", owned.as_str());
}
```

<details>
<summary>Solution</summary>

```rust
// `?Sized` relaxes the implicit `Sized` bound so `T` may be `str`;
// `AsRef<str>` gives a uniform way to view it as a string slice.
fn log_line<T: AsRef<str> + ?Sized>(prefix: &str, message: &T) {
    println!("[{prefix}] {}", message.as_ref());
}

fn main() {
    log_line("info", "starting up");        // &str literal
    let owned = String::from("connection lost");
    log_line("warn", &owned);               // &String
    log_line("warn", owned.as_str());       // &str slice
}
```

**Output:**

```text
[info] starting up
[warn] connection lost
[warn] connection lost
```

Without `?Sized`, the implicit `T: Sized` bound would reject `str`. Taking `&T` and viewing it through `AsRef<str>` makes the function accept every string-shaped input.

</details>

### Exercise 3 — A generic thread-safe cache

**Difficulty:** Hard

**Objective:** Use `Send + Sync` (plus `'static`) bounds to build a generic container that can be shared across threads, and prove it works by mutating it from a spawned thread.

**Instructions:** Build a `Cache<K, V>` backed by `Arc<Mutex<HashMap<K, V>>>`. Implement `new`, `insert(&self, K, V)`, and `get(&self, &K) -> Option<V>`. Add the trait bounds on the `impl` block that the threading and the `HashMap` actually require. Make `Cache` cheap to share by implementing `Clone` so it just clones the inner `Arc`. In `main`, insert one entry on the main thread, hand a clone of the cache to a spawned thread that inserts another, join it, and print both values.

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct Cache<K, V> {
    store: Arc<Mutex<HashMap<K, V>>>,
}

// TODO: impl block with the right bounds: new / insert / get
// TODO: impl Clone so sharing across threads is cheap
// TODO: main that inserts from two threads and prints results
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::thread;

struct Cache<K, V> {
    store: Arc<Mutex<HashMap<K, V>>>,
}

impl<K, V> Cache<K, V>
where
    // HashMap needs Eq + Hash on keys; threading needs Send + Sync + 'static;
    // Clone lets `get` return an owned value out of the lock.
    K: Eq + Hash + Send + Sync + Clone + 'static,
    V: Send + Sync + Clone + 'static,
{
    fn new() -> Self {
        Cache { store: Arc::new(Mutex::new(HashMap::new())) }
    }

    fn insert(&self, key: K, value: V) {
        self.store.lock().unwrap().insert(key, value);
    }

    fn get(&self, key: &K) -> Option<V> {
        self.store.lock().unwrap().get(key).cloned()
    }
}

// Cloning the cache just bumps the Arc's refcount — both handles share state.
impl<K, V> Clone for Cache<K, V> {
    fn clone(&self) -> Self {
        Cache { store: Arc::clone(&self.store) }
    }
}

fn main() {
    let cache: Cache<String, u32> = Cache::new();
    cache.insert("startup".to_string(), 1);

    let worker = cache.clone(); // shares the same underlying map
    let writer = thread::spawn(move || {
        worker.insert("hits".to_string(), 7);
    });
    writer.join().unwrap();

    println!("startup = {:?}", cache.get(&"startup".to_string()));
    println!("hits = {:?}", cache.get(&"hits".to_string()));
}
```

**Output:**

```text
startup = Some(1)
hits = Some(7)
```

The `Send + Sync + 'static` bounds are what let the cloned `Cache` move into the spawned thread; `Arc<Mutex<...>>` provides the `Send + Sync` shared mutable state. Replace `Arc`/`Mutex` with `Rc`/`RefCell` and the `thread::spawn` call would fail to compile with the `Send` error from Pitfall 3.

</details>
