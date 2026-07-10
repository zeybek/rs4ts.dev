---
title: "`PhantomData` and Zero-Sized Types"
description: "PhantomData is Rust's branded type, but it tracks ownership, lifetimes, and thread-safety, not just nominal tags, while costing zero bytes at runtime."
---

Sometimes you need a type to *behave* as if it owned, borrowed, or were parameterized over some `T` (for ownership, variance, lifetime, or thread-safety purposes) without actually storing a `T`. Rust's answer is `PhantomData<T>`: a **zero-sized marker** that you place in a struct field to tell the compiler "pretend this type relates to `T`," while costing exactly zero bytes at runtime. There is no TypeScript equivalent, because TypeScript types are erased at runtime and carry no ownership, lifetime, or thread-safety meaning.

---

## Quick Overview

A **zero-sized type** (ZST) is a type whose values occupy **0 bytes**: `()`, a fieldless struct, or `PhantomData<T>`. `PhantomData<T>` is the standard library's purpose-built ZST: putting a `PhantomData<T>` field in your struct makes the type *act* as though it contains a `T` for the compiler's analyses (drop-check, variance, auto-trait inference, lifetime tracking) without adding any storage. The two payoffs for a TypeScript/JavaScript developer are: (1) you can encode extra facts in the type system — "this ID belongs to a `User`," "this connection is open," "this value cannot leave its thread" — and (2) those facts are checked at compile time and then **completely erased**, so the abstraction is genuinely free.

> **Note:** This file covers `PhantomData` and ZSTs specifically. The marker *traits* `Send`/`Sync`/`Sized`/`Copy` are introduced in [marker traits](/09-generics-traits/11-marker-traits/); raw pointers and `unsafe` ownership live in [Section 20](/20-unsafe-ffi/) and [Section 26](/26-systems-programming/). `PhantomData` shows up there because it is how you encode ownership over a raw pointer.

---

## TypeScript/JavaScript Example

TypeScript developers reach for **branded (nominal) types** to make two structurally identical types distinct: for example, to stop a `UserId` being passed where an `OrderId` is expected. Since TypeScript is structurally typed, the trick is to intersect with a phantom property that exists only in the type system:

```typescript
// TypeScript: "branded" types simulate nominal typing.
// The `__brand` field never exists at runtime — it is purely a type-level tag.
type Brand<T, B> = T & { readonly __brand: B };

type UserId = Brand<number, "UserId">;
type OrderId = Brand<number, "OrderId">;

function makeUserId(n: number): UserId {
  return n as UserId; // a cast; nothing is stored
}
function makeOrderId(n: number): OrderId {
  return n as OrderId;
}

function fetchUser(id: UserId): void {
  console.log(`fetching user ${id}`);
}

const userId = makeUserId(42);
const orderId = makeOrderId(42);

fetchUser(userId); // ok
// fetchUser(orderId);
// ^ Argument of type 'OrderId' is not assignable to parameter of type 'UserId'.
//   Type '"OrderId"' is not assignable to type '"UserId"'.
```

**Key points for a TypeScript developer:**

- The `__brand` property is a **lie**: it is never written at runtime. `userId` is just the `number` `42`. The brand exists only so the *checker* keeps the two types apart.
- This is a workaround for TypeScript's **structural** typing. Without the brand, `UserId` and `OrderId` are both just `number` and freely interchangeable.
- The brand carries **no ownership, lifetime, or thread-safety meaning**. TypeScript has no notion of any of those, because the type system is erased before the code ever runs.

`PhantomData` is Rust's analogue of the brand — but it does far more than nominal tagging, because Rust's type system tracks ownership, lifetimes, variance, and thread-safety, and `PhantomData` lets you participate in all of them.

---

## Rust Equivalent

Here is the same "typed IDs" idea in Rust. We make `Id<T>` generic over a marker type, but the only data we store is a `u64`; the `T` lives in a `PhantomData<T>`:

```rust playground
use std::marker::PhantomData;

// Same u64 representation, but the compiler keeps the two ID kinds distinct.
struct Id<T> {
    raw: u64,
    _marker: PhantomData<T>,
}

impl<T> Id<T> {
    fn new(raw: u64) -> Self {
        Id { raw, _marker: PhantomData }
    }
}

// The marker types. They are never constructed — they only exist as tags.
struct User;
struct Order;

fn fetch_user(id: Id<User>) {
    println!("fetching user {}", id.raw);
}

fn main() {
    let user_id: Id<User> = Id::new(42);
    let order_id: Id<Order> = Id::new(42);

    fetch_user(user_id);
    // fetch_user(order_id); // does not compile: expected `Id<User>`, found `Id<Order>`
    println!("order id raw = {}", order_id.raw);

    // The marker costs nothing: Id<User> is the same size as a bare u64.
    println!("size_of Id<User> = {}", std::mem::size_of::<Id<User>>());
    println!("size_of u64      = {}", std::mem::size_of::<u64>());
}
```

Running it:

```text
fetching user 42
order id raw = 42
size_of Id<User> = 8
size_of u64      = 8
```

`Id<User>` and `Id<Order>` are different types the compiler refuses to mix, yet `size_of::<Id<User>>()` is `8` — identical to a raw `u64`. The `PhantomData<T>` field adds **zero bytes**. That is the headline: the safety is real, the cost is nothing.

> **Why not just `struct Id<T> { raw: u64 }` with no marker?** Because Rust forbids unused type parameters. Without `PhantomData`, the compiler emits `error[E0392]: type parameter 'T' is never used`; see [Common Pitfalls](#common-pitfalls).

---

## Detailed Explanation

### What `PhantomData<T>` *is*

`PhantomData<T>` is defined in the standard library, roughly as `pub struct PhantomData<T: ?Sized>;`, a struct with **no fields**. Therefore:

- Its size is `0`. `std::mem::size_of::<PhantomData<T>>()` is always `0`, no matter what `T` is.
- You construct it by writing the bare value `PhantomData` (the type is inferred from the field's declared type, e.g. `_marker: PhantomData<T>`).
- It holds no `T`. It never runs `T`'s constructor or destructor on its own.

What it *does* is feed information to four of the compiler's static analyses. When you write `_marker: PhantomData<T>`, the compiler treats your struct **as if** it contained a `T` for the purposes of:

1. **Drop check**: whether the compiler believes your type owns a `T` that will be dropped.
2. **Variance**: how subtyping of `T` (or a lifetime `'a`) relates to subtyping of your struct.
3. **Auto traits** (`Send`/`Sync`): whether your struct can move to / be shared with another thread.
4. **Lifetime tracking**: keeping a `'a` "in use" so the borrow checker enforces it, even though you only store a pointer or offset.

The marker type itself (`User`, `Order` above) is usually a fieldless struct — itself a ZST — that you never instantiate. It is just a name the type system can distinguish.

### Encoding ownership over a raw pointer

The most important *correctness* use of `PhantomData` is telling the compiler that your struct **owns** a `T` it only points at. Consider a hand-rolled `Box`-like container built over a raw pointer:

```rust playground
use std::marker::PhantomData;
use std::ptr::NonNull;

// A toy owning container built over a raw pointer. `PhantomData<T>` tells the
// compiler "this struct OWNS a T", which drives drop-check and variance.
struct MyBox<T> {
    ptr: NonNull<T>,
    _owns: PhantomData<T>,
}

impl<T> MyBox<T> {
    fn new(value: T) -> Self {
        let boxed = Box::new(value);
        let ptr = NonNull::new(Box::into_raw(boxed)).unwrap();
        MyBox { ptr, _owns: PhantomData }
    }
    fn get(&self) -> &T {
        // Safe: we created this pointer from a valid Box and still own it.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> Drop for MyBox<T> {
    fn drop(&mut self) {
        // Reconstruct the Box so T's destructor runs and the memory is freed.
        unsafe { drop(Box::from_raw(self.ptr.as_ptr())); }
    }
}

fn main() {
    let b = MyBox::new(String::from("owned heap data"));
    println!("value = {}", b.get());
    // `b` drops here: `Box::from_raw` frees the String, no leak.
}
```

```text
value = owned heap data
```

The important detail: `NonNull<T>` is a raw pointer, and **raw pointers do not express ownership**. Without the `_owns: PhantomData<T>` field, the compiler would not know that `MyBox<T>` is responsible for a `T`, which can lead the drop-checker to permit unsound code in the presence of borrowed data with the same lifetime as the box. Adding `PhantomData<T>` makes `MyBox<T>` behave like the real `Box<T>` for the "does this own a `T`?" question. This is the canonical pattern documented in the Rustonomicon for any collection or smart pointer built over `*mut T` / `NonNull<T>`.

### Encoding a lifetime without storing a reference

Sometimes you store an integer offset or a raw pointer that *logically* borrows from some buffer, and you want the borrow checker to keep that buffer alive. `PhantomData<&'a T>` does exactly that:

```rust
use std::marker::PhantomData;

// `Token` borrows from a `&'src str`, but stores only offsets — no reference.
// `PhantomData<&'src str>` keeps the borrow alive in the type system.
struct Token<'src> {
    start: usize,
    len: usize,
    _src: PhantomData<&'src str>,
}
```

Now a `Token<'src>` cannot outlive the `&'src str` it conceptually points into, even though at runtime it is just two `usize`s. The full lexer that produces these tokens is in [Exercises](#exercises).

### Controlling variance and thread-safety with the "right" `PhantomData`

The *type you put inside* `PhantomData` matters, because different forms convey different ownership/variance/auto-trait facts. The four canonical forms:

| You write | "Owns a `T`"? (drop-check) | Variance in `T` | `Send`/`Sync` | Typical use |
| --- | --- | --- | --- | --- |
| `PhantomData<T>` | **Yes** | covariant | inherits from `T` | container/smart pointer that owns a `T` |
| `PhantomData<&'a T>` | No | covariant | inherits (`Sync` if `T: Sync`) | a shared borrow you only model |
| `PhantomData<*const T>` | No | covariant | **neither** (`!Send`, `!Sync`) | a tag that must stay on one thread |
| `PhantomData<fn() -> T>` | No | covariant | **both** (`Send + Sync`) | a pure type tag with no ownership |

For a "nominal tag" like `Id<T>` or units of measure, `PhantomData<fn() -> T>` is often the most conservative choice: it claims no ownership and stays `Send + Sync` regardless of `T`. This is verifiable: `PhantomData<fn() -> T>` is `Send` even when `T` is the `!Send` type `Rc<i32>`:

```rust playground
use std::marker::PhantomData;
use std::rc::Rc;

fn assert_send<T: Send>() {}

struct UsesT<T> {
    _marker: PhantomData<fn() -> T>,
}

fn main() {
    // Rc<i32> is !Send, yet PhantomData<fn() -> Rc<i32>> is still Send.
    assert_send::<UsesT<Rc<i32>>>();
    println!("PhantomData<fn() -> T> is Send even when T is !Send");
}
```

```text
PhantomData<fn() -> T> is Send even when T is !Send
```

Conversely, `PhantomData<*const ()>` is the standard way to make a struct **`!Send` and `!Sync`**; see the thread-safety pitfall below.

### Zero-sized types more broadly

`PhantomData` is one ZST, but ZSTs are a general concept. The unit type `()`, an empty struct, and an empty enum's inhabited unit variants all have size `0`. The compiler and standard library exploit this:

```rust playground
use std::collections::HashMap;
use std::mem::{size_of, size_of_val};

struct Marker; // a zero-sized type (ZST)

fn main() {
    println!("size_of::<()>()      = {}", size_of::<()>());
    println!("size_of::<Marker>()  = {}", size_of::<Marker>());

    // A Vec of 1000 ZSTs allocates no heap memory for its elements.
    let zeros: Vec<()> = vec![(); 1000];
    println!("len = {}, but elements occupy 0 bytes", zeros.len());

    // HashSet<K> is literally HashMap<K, ()> under the hood — value is a ZST.
    let mut set: HashMap<&str, ()> = HashMap::new();
    set.insert("a", ());
    set.insert("b", ());
    println!("set has {} keys; each value is {} bytes", set.len(), size_of::<()>());

    let m = Marker;
    println!("size_of_val(&m) = {}", size_of_val(&m));
}
```

```text
size_of::<()>()      = 0
size_of::<Marker>()  = 0
len = 1000, but elements occupy 0 bytes
set has 2 keys; each value is 0 bytes
size_of_val(&m) = 0
```

`std::collections::HashSet<K>` really is a thin wrapper over `HashMap<K, ()>`: the `()` value is a ZST, so a set costs the same as the map's keys alone. Likewise a `Vec<()>` of a million elements allocates nothing for the elements; it just tracks the length.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Purpose of phantom field | Nominal tagging only (brands) | Ownership, variance, lifetimes, thread-safety, *and* tagging |
| Runtime presence | Erased; the value is the underlying primitive | Erased too — `PhantomData` is genuinely 0 bytes |
| Checked when? | At type-check (then erased before runtime) | At compile time (then monomorphized + erased) |
| Distinguishing two identical shapes | `& { __brand }` intersection workaround | Generic param + `PhantomData<T>`, fully nominal |
| Lifetime / ownership meaning | None — no such concepts exist | `PhantomData<&'a T>`, `PhantomData<T>` model exactly these |
| Thread-safety meaning | None — no compile-time threading model | `PhantomData<*const T>` opts out of `Send`/`Sync` |

The conceptual leap for a TypeScript developer: a brand is *only* a tag, but `PhantomData` is a participant in Rust's ownership and borrow analyses. Rust needs `PhantomData` precisely *because* its type system tracks things TypeScript's does not. When you wrote `as UserId` in TypeScript, nothing was being protected at runtime; when you put `PhantomData<T>` in a Rust struct, you may be the difference between sound and unsound memory management.

> **Note:** Unlike a TypeScript brand, `PhantomData<T>` can change whether your type compiles at all (drop-check, `Send`/`Sync`); it is not a no-op annotation you can sprinkle freely. Pick the form that matches the *real* ownership relationship.

---

## Common Pitfalls

### Pitfall 1: Unused type parameter without `PhantomData`

The first thing every TypeScript developer tries is a generic struct that "remembers" `T` without storing it:

```rust
struct Id<T> {
    raw: u64,
}

fn main() {
    let _id: Id<String> = Id { raw: 1 };
}
```

This does not compile. The real error:

```text
error[E0392]: type parameter `T` is never used
 --> src/main.rs:1:11
  |
1 | struct Id<T> {
  |           ^ unused type parameter
  |
  = help: consider removing `T`, referring to it in a field, or using a marker such as `PhantomData`
  = help: if you intended `T` to be a const parameter, use `const T: /* Type */` instead
```

The compiler itself suggests the fix: add a `PhantomData<T>` field. Rust forbids unused generic parameters because the parameter affects variance and drop-check, and the compiler needs you to state *how* `T` relates to the struct.

### Pitfall 2: Forgetting to actually construct the field

Once you add `_marker: PhantomData<T>`, you must initialize it in every constructor, but it is just the literal `PhantomData`:

```rust playground
use std::marker::PhantomData;

struct Wrapper<T> {
    value: i32,
    _marker: PhantomData<T>,
}

impl<T> Wrapper<T> {
    fn new(value: i32) -> Self {
        // Correct: the field is initialized with the bare `PhantomData` value.
        Wrapper { value, _marker: PhantomData }
    }
}

fn main() {
    let _w: Wrapper<String> = Wrapper::new(7);
    println!("ok");
}
```

If you omit the field you get `error[E0063]: missing field '_marker' in initializer of 'Wrapper<T>'`. There is no runtime work; `PhantomData` is the unit-like value of a zero-sized type.

### Pitfall 3: Assuming a tagged type is `Send` — or that it is not

If you use `PhantomData<*const T>` (or `*mut T`) to model a pointer, you silently make the whole struct `!Send` and `!Sync`. That is usually *desirable* for thread-bound handles, but surprising if you only wanted a tag. Here is the intended use: a handle that must never leave its creating thread:

```rust
use std::marker::PhantomData;
use std::thread;

struct ThreadBound {
    handle: usize,
    _not_send: PhantomData<*const ()>,
}

fn main() {
    let bound = ThreadBound { handle: 1, _not_send: PhantomData };
    let join = thread::spawn(move || {
        let b = bound; // capture the whole struct
        println!("{}", b.handle);
    });
    join.join().unwrap();
}
```

This is `// does not compile`. The real error:

```text
error[E0277]: `*const ()` cannot be sent between threads safely
   --> src/main.rs:11:30
    |
 11 |       let join = thread::spawn(move || {
    |                  ------------- ^------
    |                  |             |
    |  ________________|_____________within this `{closure@src/main.rs:11:30: 11:37}`
    | |                |
    | |                required by a bound introduced by this call
 12 | |         let b = bound; // capture the whole struct
 13 | |         println!("{}", b.handle);
 14 | |     });
    | |_____^ `*const ()` cannot be sent between threads safely
    |
    = help: within `{closure@src/main.rs:11:30: 11:37}`, the trait `Send` is not implemented for `*const ()`
note: required because it appears within the type `PhantomData<*const ()>`
note: required because it appears within the type `ThreadBound`
note: required because it's used within this closure
note: required by a bound in `spawn`
```

If you wanted the struct to remain `Send`, use `PhantomData<fn() -> T>` (or `PhantomData<T>` when `T: Send`) instead of `PhantomData<*const T>`.

> **Warning:** Disjoint closure captures (stable since the 2021 edition) mean a `move` closure that only touches `bound.handle` would capture *just* the `usize` and compile fine. Capturing the whole struct (as above) is what surfaces the `!Send` constraint. Do not rely on accidental field-level capture to dodge thread-safety; it is a footgun.

### Pitfall 4: Using `PhantomData<T>` when you do *not* own a `T`

`PhantomData<T>` claims ownership of a `T` for drop-check. If your struct merely *borrows* a `T` (e.g. holds a `&T` you reconstruct manually), use `PhantomData<&'a T>` instead. Over-claiming ownership can make otherwise-valid programs fail to compile (the drop-checker becomes stricter than necessary). Match the marker to the real relationship.

---

## Best Practices

- **Name the field with a leading underscore** (`_marker`, `_owns`, `_state`) to signal "this is intentionally unused storage" and silence dead-code lints.
- **Choose the marker form deliberately** using the variance/`Send`/`Sync` table above. For a pure nominal tag with no ownership, `PhantomData<fn() -> T>` is the safest default; for an owning raw-pointer container, use `PhantomData<T>`; for a borrowed view, `PhantomData<&'a T>`.
- **Prefer the typestate pattern** (a generic state parameter held in `PhantomData`) over runtime boolean flags when an invalid state should be *unrepresentable*. The compiler then rejects misuse instead of you writing runtime checks. See the Real-World example.
- **Keep marker types fieldless and never construct them** — `struct Open;` not `struct Open {}` with data. They exist only as type-level names.
- **Reach for ZSTs to express "no data, only meaning"**: an empty struct implementing a trait, a unit value in a map (`HashMap<K, ()>`), or a strategy/dispatch tag. They compile away entirely.
- **Do not over-reach.** If a plain newtype (`struct UserId(u64);`) already gives you the distinctness you need without generics, use that; it is simpler. Reach for `PhantomData<T>` when you need to be generic over the tag, model a lifetime/ownership relationship, or build typestate.

---

## Real-World Example

A production-grade use of `PhantomData` is the **typestate pattern**: encode an object's state in its type so that methods only valid in one state are *unavailable* in others, enforced at compile time. Here, a database/socket connection cannot be sent on before it is opened, and cannot be opened twice, and there is zero runtime cost, because the state lives entirely in a `PhantomData`:

```rust playground
use std::marker::PhantomData;

// State markers — fieldless ZSTs that are never constructed.
struct Open;
struct Closed;

struct Connection<State> {
    socket_fd: i32,
    _state: PhantomData<State>,
}

// Methods available only on a *closed* connection.
impl Connection<Closed> {
    fn new(fd: i32) -> Self {
        Connection { socket_fd: fd, _state: PhantomData }
    }
    fn open(self) -> Connection<Open> {
        println!("opening fd {}", self.socket_fd);
        Connection { socket_fd: self.socket_fd, _state: PhantomData }
    }
}

// Methods available only on an *open* connection.
impl Connection<Open> {
    fn send(&self, msg: &str) {
        println!("send on fd {}: {msg}", self.socket_fd);
    }
    fn close(self) -> Connection<Closed> {
        println!("closing fd {}", self.socket_fd);
        Connection { socket_fd: self.socket_fd, _state: PhantomData }
    }
}

fn main() {
    let conn = Connection::<Closed>::new(7);
    let conn = conn.open();    // Closed -> Open
    conn.send("hello");        // only valid because `conn` is Open
    let _conn = conn.close();  // Open -> Closed

    // conn.send("again");     // does not compile: `conn` was moved into close()
    // Connection::<Closed>::new(7).send("x"); // does not compile: no `send` on Closed

    // The state tag is free: Connection<Open> is the same size as its only real field.
    println!(
        "size_of Connection<Open> = {}",
        std::mem::size_of::<Connection<Open>>()
    );
    println!("size_of i32             = {}", std::mem::size_of::<i32>());
}
```

```text
opening fd 7
send on fd 7: hello
closing fd 7
size_of Connection<Open> = 4
size_of i32             = 4
```

A few things to notice. `send` exists *only* in `impl Connection<Open>`, so calling it on a `Connection<Closed>` is not a runtime error — it does not type-check at all. The state transitions consume `self` and return a new type (`open(self) -> Connection<Open>`), so you cannot accidentally keep using the old state. And `size_of::<Connection<Open>>()` is `4`, identical to the lone `i32` field. The entire state machine is enforced by the compiler and then erased. Real libraries use this pattern extensively: HTTP request builders that require a URL before `send()`, embedded HAL crates that model GPIO pins as input/output at the type level, and parser combinators that track whether input remains.

A second realistic use is **units of measure**, where the unit is a phantom tag that prevents mixing dimensions:

```rust playground
use std::marker::PhantomData;
use std::ops::Add;

#[derive(Debug, Clone, Copy)]
struct Quantity<Unit> {
    value: f64,
    _unit: PhantomData<Unit>,
}

impl<Unit> Quantity<Unit> {
    const fn new(value: f64) -> Self {
        Quantity { value, _unit: PhantomData }
    }
}

// Addition is allowed only within the SAME unit.
impl<Unit> Add for Quantity<Unit> {
    type Output = Quantity<Unit>;
    fn add(self, rhs: Self) -> Self::Output {
        Quantity::new(self.value + rhs.value)
    }
}

struct Meters;
struct Seconds;

fn main() {
    let distance = Quantity::<Meters>::new(100.0) + Quantity::<Meters>::new(50.0);
    let time = Quantity::<Seconds>::new(9.58);

    // let bad = distance + time; // does not compile: mismatched units
    //   error[E0308]: mismatched types
    //   expected `Quantity<Meters>`, found `Quantity<Seconds>`

    println!("distance = {} m, time = {} s", distance.value, time.value);
    println!(
        "size_of Quantity<Meters> = {} (a bare f64 is {})",
        std::mem::size_of::<Quantity<Meters>>(),
        std::mem::size_of::<f64>()
    );
}
```

```text
distance = 150 m, time = 9.58 s
size_of Quantity<Meters> = 8 (a bare f64 is 8)
```

Adding meters to seconds is a compile error (`error[E0308]: mismatched types ... expected 'Quantity<Meters>', found 'Quantity<Seconds>'`), while `Quantity<Meters>` is byte-for-byte an `f64`. The `uom` crate generalizes this to the full SI system using exactly this technique.

---

## Further Reading

- [`std::marker::PhantomData`](https://doc.rust-lang.org/std/marker/struct.PhantomData.html): the official API docs, including the variance table.
- [The Rustonomicon: `PhantomData`](https://doc.rust-lang.org/nomicon/phantom-data.html) — the authoritative treatment of drop-check and variance interactions.
- [The Rustonomicon: Subtyping and Variance](https://doc.rust-lang.org/nomicon/subtyping.html): background for why the marker form changes variance.
- [The Reference: Zero-sized types](https://doc.rust-lang.org/reference/type-layout.html) — layout guarantees for ZSTs.
- [Marker traits: `Copy`, `Sized`, `Send`, `Sync`](/09-generics-traits/11-marker-traits/): the traits `PhantomData` interacts with.
- [Generic structs](/09-generics-traits/01-generic-structs/) and [Trait bounds](/09-generics-traits/05-trait-bounds/) — the generics machinery underpinning `PhantomData<T>`.
- Sibling advanced topics: [Pin and Unpin](/25-advanced-topics/01-pin-unpin/), [Const generics](/25-advanced-topics/05-const-generics/), [Custom allocators](/25-advanced-topics/03-allocators/), [Async internals](/25-advanced-topics/02-async-internals/), [Inline assembly](/25-advanced-topics/04-inline-assembly/).
- [Section 20: Unsafe & FFI](/20-unsafe-ffi/) and [Section 26: Systems Programming](/26-systems-programming/): where `PhantomData` over raw pointers earns its keep.
- Foundations: [Section 00: Introduction](/00-introduction/), [Section 01: Getting Started](/01-getting-started/), [Section 02: Basics](/02-basics/).

---

## Exercises

### Exercise 1: Tagged sanitized strings

**Difficulty:** Beginner

**Objective:** Use a phantom state parameter to make "unsanitized" and "sanitized" user input distinct types, so only sanitized input can be rendered.

**Instructions:** Define marker types `Raw` and `Sanitized`, and a `UserInput<State>` struct holding a `String` plus a `PhantomData<State>`. Provide `UserInput::<Raw>::new(...)` and a `sanitize(self) -> UserInput<Sanitized>` method that escapes `<` and `>`. Add a `render(&self) -> &str` method available *only* on `UserInput<Sanitized>`. Prove that you cannot call `render` on raw input.

<details>
<summary>Solution</summary>

```rust playground
use std::marker::PhantomData;

struct Raw;
struct Sanitized;

struct UserInput<State> {
    text: String,
    _state: PhantomData<State>,
}

impl UserInput<Raw> {
    fn new(text: impl Into<String>) -> Self {
        UserInput { text: text.into(), _state: PhantomData }
    }
    fn sanitize(self) -> UserInput<Sanitized> {
        let cleaned = self.text.replace('<', "&lt;").replace('>', "&gt;");
        UserInput { text: cleaned, _state: PhantomData }
    }
}

impl UserInput<Sanitized> {
    // `render` exists only for Sanitized input, so `raw.render()` won't compile.
    fn render(&self) -> &str {
        &self.text
    }
}

fn main() {
    let raw = UserInput::<Raw>::new("<script>alert(1)</script>");
    // raw.render(); // does not compile: no method `render` on UserInput<Raw>
    let safe = raw.sanitize();
    println!("rendered: {}", safe.render());
}
```

```text
rendered: &lt;script&gt;alert(1)&lt;/script&gt;
```

</details>

### Exercise 2: A lifetime-tied lexer token

**Difficulty:** Intermediate

**Objective:** Build a `Token<'src>` that stores only byte offsets but is tied via `PhantomData<&'src str>` to the source string it came from, so it cannot outlive that source.

**Instructions:** Write a `Lexer<'src>` over a `&'src str` with a `next_word(&mut self) -> Option<Token<'src>>` method that skips spaces and returns the start offset and length of each word. The returned `Token<'src>` must carry a `PhantomData<&'src str>` so the borrow checker keeps the source alive. In `main`, iterate the tokens and resolve each back to a `&str` slice of the source.

<details>
<summary>Solution</summary>

```rust playground
use std::marker::PhantomData;

struct Token<'src> {
    start: usize,
    len: usize,
    _src: PhantomData<&'src str>,
}

struct Lexer<'src> {
    source: &'src str,
    pos: usize,
}

impl<'src> Lexer<'src> {
    fn new(source: &'src str) -> Self {
        Lexer { source, pos: 0 }
    }
    fn next_word(&mut self) -> Option<Token<'src>> {
        let bytes = self.source.as_bytes();
        while self.pos < bytes.len() && bytes[self.pos] == b' ' {
            self.pos += 1;
        }
        if self.pos >= bytes.len() {
            return None;
        }
        let start = self.pos;
        while self.pos < bytes.len() && bytes[self.pos] != b' ' {
            self.pos += 1;
        }
        Some(Token { start, len: self.pos - start, _src: PhantomData })
    }
}

fn main() {
    let source = String::from("phantom data is free");
    let mut lexer = Lexer::new(&source);
    while let Some(tok) = lexer.next_word() {
        println!("token: {:?}", &source[tok.start..tok.start + tok.len]);
    }
    println!("size_of Token = {}", std::mem::size_of::<Token>());
}
```

```text
token: "phantom"
token: "data"
token: "is"
token: "free"
size_of Token = 16
```

The `Token` is just two `usize`s (16 bytes on a 64-bit target); the `PhantomData<&'src str>` adds nothing but ties the token's lifetime to the source.

</details>

### Exercise 3: A thread-bound handle

**Difficulty:** Advanced

**Objective:** Use `PhantomData<*const ()>` to build a handle that the compiler refuses to move to another thread, while keeping it usable on the thread that created it.

**Instructions:** Define a `GlHandle` struct holding a `u32` id and a `PhantomData<*const ()>` field. Add `new` and `bind(&self)` methods. Confirm it works on the current thread, then (in prose or a commented-out block) explain what happens if you `thread::spawn` a closure that moves the handle.

<details>
<summary>Solution</summary>

```rust playground
use std::marker::PhantomData;

struct GlHandle {
    id: u32,
    // `*const ()` is neither Send nor Sync, so GlHandle inherits !Send + !Sync.
    _not_send: PhantomData<*const ()>,
}

impl GlHandle {
    fn new(id: u32) -> Self {
        GlHandle { id, _not_send: PhantomData }
    }
    fn bind(&self) {
        println!("binding GL handle {}", self.id);
    }
}

fn main() {
    let h = GlHandle::new(1);
    h.bind(); // fine on the current thread

    // std::thread::spawn(move || { h.bind(); });
    // ^ does not compile: error[E0277] `*const ()` cannot be sent between
    //   threads safely — `GlHandle` is !Send because of the PhantomData marker.

    println!("size_of GlHandle = {} (just the u32)", std::mem::size_of::<GlHandle>());
}
```

```text
binding GL handle 1
size_of GlHandle = 4 (just the u32)
```

The handle behaves exactly like a non-thread-safe resource (think OpenGL contexts, FFI handles, or anything `!Send`), yet costs only the bytes of its real `u32` field. The `PhantomData<*const ()>` is the idiomatic way to opt a type out of `Send`/`Sync` without `unsafe` negative impls.

</details>
