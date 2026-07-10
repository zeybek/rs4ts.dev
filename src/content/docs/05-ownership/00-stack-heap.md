---
title: "Stack and Heap: Where Your Data Lives"
description: "Where Rust values live: fixed-size data on the stack, growable data like String and Vec on the heap — what JS hides behind its GC, Rust makes explicit."
---

Before you can understand Rust's ownership system, you need a mental model of **where** values live in memory: the **stack** or the **heap**. In TypeScript/JavaScript a garbage collector hides this distinction from you completely. In Rust the distinction is front and center: it is the reason ownership exists at all.

---

## Quick Overview

The **stack** is a fast, fixed-size region where values of a known, fixed size are stored and freed automatically as functions enter and exit. The **heap** is a larger, flexible region for values whose size can grow or is not known at compile time (like a growable string). In TypeScript/JavaScript every object lives on a garbage-collected heap and you never think about it; in Rust, knowing what lives where explains *moves*, *borrows*, and why there is no garbage collector.

> **Note:** This file covers the **memory model**: what the stack and heap are, what lives where, and why it matters. The *rules* that govern this memory (one owner, move-on-assign, drop-at-scope-end) are covered in [The Three Ownership Rules](/05-ownership/01-ownership-rules/), and the mechanics of moving versus copying live in [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/).

---

## TypeScript/JavaScript Example

In JavaScript, you never allocate or free memory yourself. Primitives feel like values; objects, arrays, and functions are handled by reference. A garbage collector reclaims anything you stop using.

```typescript
// TypeScript/JavaScript — the engine decides where everything goes
const age = 30; // a number (always IEEE-754 f64 under the hood)
const name = "Grace Hopper"; // a string
const scores = [90, 85, 100]; // an array object

// Assignment of an object copies the *reference*, not the object itself.
const a = { count: 1 };
const b = a; // b points at the SAME object as a
b.count = 99;
console.log(a.count); // 99  — they are the same object

// Primitives behave as if copied by value.
let x = 5;
let y = x; // y is an independent copy
y = 6;
console.log(x, y); // 5 6
```

**Key points:**

- You never write `malloc`/`free`; the V8 garbage collector (GC) reclaims memory when nothing references it anymore.
- Primitives (`number`, `boolean`, `string` handles, `null`, `undefined`) are passed and assigned **by value**.
- Objects, arrays, and functions are passed and assigned **by reference**: `b = a` makes `a` and `b` aliases of one heap object.
- The cost: GC pauses, extra memory headroom, and non-deterministic cleanup timing.

---

## Rust Equivalent

Rust makes the same program work without a garbage collector. The size and location of every value is decided at compile time, and cleanup happens deterministically when a value goes out of scope.

```rust playground
fn main() {
    // --- Lives entirely on the stack: fixed, known size ---
    let age: i32 = 30; // 4 bytes
    let price: f64 = 19.99; // 8 bytes
    let is_active: bool = true; // 1 byte
    let point: (i32, i32) = (3, 4); // 8 bytes (two i32s, inline)
    let scores: [i32; 3] = [90, 85, 100]; // 12 bytes (three i32s, inline)

    // --- Uses the heap: size can grow / is not known at compile time ---
    let name: String = String::from("Grace Hopper"); // text bytes on the heap
    let numbers: Vec<i32> = vec![1, 2, 3, 4, 5]; // elements on the heap

    println!("age = {age}");
    println!("point = {point:?}");
    println!("name = {name}");
    println!("numbers = {numbers:?}");
}
```

**Output (verified):**

```text
age = 30
point = (3, 4)
name = Grace Hopper
numbers = [1, 2, 3, 4, 5]
```

**Key points:**

- Numbers, `bool`, `char`, fixed-size tuples, and fixed-size arrays live **on the stack**.
- `String` and `Vec<T>` store a small **handle** on the stack that points to a buffer **on the heap**.
- There is no garbage collector. When a value's owner goes out of scope, its heap memory is freed immediately and deterministically (see [The Drop Trait and RAII](/05-ownership/08-drop-trait/)).

---

## Detailed Explanation

### What the stack is

The **stack** is a region of memory that works exactly like the data-structure of the same name: last-in, first-out. Every time a function is called, a **stack frame** is pushed holding that function's local variables. When the function returns, its entire frame is popped in one cheap operation. The CPU tracks the top of the stack in a register, so "allocating" a stack value is just moving a pointer, essentially free.

The catch: the stack only works for data whose size is **known at compile time**. The compiler must know how many bytes each local needs in order to lay out the frame. The stack is also small (commonly a few megabytes for the main thread), so it is for short-lived, fixed-size values.

We can observe stack values sitting next to each other in memory:

```rust playground
fn main() {
    let a: i32 = 1;
    let b: i32 = 2;
    let c: i32 = 3;

    // `{:p}` formats a reference as a pointer (hex address).
    println!("a is at {:p}", &a);
    println!("b is at {:p}", &b);
    println!("c is at {:p}", &c);
}
```

**Output (one real run, your addresses will differ):**

```text
a is at 0x16d4e1b44
b is at 0x16d4e1b48
c is at 0x16d4e1b4c
```

Each `i32` sits 4 bytes from the next; they are packed tightly into the current stack frame.

> **Warning:** Do not read meaning into the exact addresses or whether they ascend or descend. The actual values depend on the platform, the optimization level, and the OS, and the compiler is free to reorder or even eliminate variables. The point is only that stack locals are cheap, contiguous, fixed-size slots.

### What the heap is

The **heap** is a larger, general-purpose region for data whose size can change at runtime or is not known up front. Asking the allocator for heap memory ("allocating") and giving it back ("freeing") is more expensive than a stack push/pop because the allocator has to find a suitable free block and track it.

A `String` is the textbook example. Its growable text cannot live on the stack because you might `push_str` more characters later. So a `String` is really **two parts**:

1. A fixed-size **handle on the stack**: a pointer to the heap buffer, a length, and a capacity.
2. The **actual bytes on the heap**, which the pointer points to.

```rust playground
fn main() {
    let greeting = String::from("hi");

    // The String *handle* is a stack local.
    println!("the String handle (stack) is at {:p}", &greeting);
    // Its text bytes live somewhere else entirely — on the heap.
    println!("its heap buffer starts at       {:p}", greeting.as_ptr());
}
```

**Output (one real run):**

```text
the String handle (stack) is at 0x16d4e1c68
its heap buffer starts at       0x600000d9c060
```

Notice the two addresses are in completely different regions of memory: the handle is on the stack (`0x16d4...`), the buffer is on the heap (`0x6000...`). This split is the single most important picture to keep in your head for the rest of this section.

```text
        STACK                         HEAP
   ┌──────────────┐            ┌───────────────────┐
   │ greeting     │            │  'h' 'i'          │
   │  ptr  ───────┼──────────▶ │  (the text bytes) │
   │  len    = 2  │            └───────────────────┘
   │  cap    = 2  │
   └──────────────┘
```

### The size of a handle is fixed

Even though the *contents* of a `String` or `Vec<T>` can grow, the *handle* on the stack is always the same fixed size. We can prove it:

```rust playground
fn main() {
    println!("size_of i32       = {}", std::mem::size_of::<i32>());
    println!("size_of f64       = {}", std::mem::size_of::<f64>());
    println!("size_of bool      = {}", std::mem::size_of::<bool>());
    println!("size_of char      = {}", std::mem::size_of::<char>());
    println!("size_of (i32,i32) = {}", std::mem::size_of::<(i32, i32)>());
    println!("size_of [i32; 3]  = {}", std::mem::size_of::<[i32; 3]>());
    println!("size_of String    = {}", std::mem::size_of::<String>());
    println!("size_of Vec<i32>  = {}", std::mem::size_of::<Vec<i32>>());
    println!("size_of &str      = {}", std::mem::size_of::<&str>());
    println!("size_of Box<i32>  = {}", std::mem::size_of::<Box<i32>>());
}
```

**Output (verified, 64-bit platform):**

```text
size_of i32       = 4
size_of f64       = 8
size_of bool      = 1
size_of char      = 4
size_of (i32,i32) = 8
size_of [i32; 3]  = 12
size_of String    = 24
size_of Vec<i32>  = 24
size_of &str      = 16
size_of Box<i32>  = 8
```

A `String` is 24 bytes on the stack regardless of whether it holds `"hi"` or a megabyte of text; those 24 bytes are the pointer (8) + length (8) + capacity (8). A `Vec<i32>` is the same shape. A `&str` is 16 bytes: a pointer plus a length (no capacity, because you cannot grow through a shared reference). A `Box<i32>` is just 8 bytes: a single pointer to a heap-allocated `i32`.

> **Note:** A `char` is 4 bytes, not 1, because Rust's `char` is a full Unicode scalar value (any code point from `'a'` to `'\u{1F600}'`), unlike C's 1-byte `char`. Text encoding is covered more in [Section 02 — Basic Types](/02-basics/01-types/).

### Putting a value on the heap on purpose: `Box<T>`

Most of the time the standard library decides heap usage for you (`String`, `Vec`). When you want to *explicitly* move a single value to the heap, you use `Box<T>`. The box itself is one pointer on the stack; the value it owns lives on the heap.

```rust playground
fn main() {
    let boxed: Box<i32> = Box::new(42);
    println!("boxed value = {}", *boxed); // deref to read the heap value

    // The handle is tiny even when the boxed data is large.
    let on_stack: [u64; 4] = [1, 2, 3, 4];
    let on_heap: Box<[u64; 4]> = Box::new([1, 2, 3, 4]);
    println!("[u64; 4] on the stack = {} bytes", std::mem::size_of_val(&on_stack));
    println!("Box<[u64; 4]> handle  = {} bytes", std::mem::size_of_val(&on_heap));
    println!("on_heap[0] = {}", on_heap[0]);
}
```

**Output (verified):**

```text
boxed value = 42
[u64; 4] on the stack = 32 bytes
Box<[u64; 4]> handle  = 8 bytes
```

`Box<T>` is the simplest of Rust's smart pointers; the others (`Rc`, `Arc`, and friends) get a light intro in [Reference Counting with `Rc<T>` and `Arc<T>`](/05-ownership/07-reference-counting/) and full coverage in [Section 10 — Smart Pointers](/10-smart-pointers/).

### Why Rust cares (and JavaScript doesn't)

In JavaScript, the engine puts almost everything on a managed heap and a garbage collector decides when to free it. You trade control for convenience. In Rust there is no GC, so the compiler needs a deterministic rule for *when* heap memory is freed. That rule is **ownership**: each heap value has exactly one owner, and when the owner goes out of scope the value is freed. Because copying a 24-byte handle is cheap but duplicating its heap buffer is not, Rust *moves* the handle by default instead of deep-copying it, which is exactly what you saw in the `let b = a` examples above and what the next files explore in depth.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| Who decides stack vs heap | The engine; hidden from you | Determined by type, visible and meaningful |
| Freeing memory | Garbage collector, non-deterministic | At end of scope, deterministic (no GC) |
| Primitive assignment | Copy by value | Copy by value (for `Copy` types) |
| Object/array assignment | Copies the **reference** (aliasing) | **Moves** ownership of the handle |
| `number` | Always IEEE-754 f64 | Pick a size: `i8`..`i128`, `u8`..`u128`, `f32`, `f64` |
| A growable string | A heap object, GC-managed | `String`: stack handle + heap buffer |
| Cost model | Opaque; GC pauses possible | Explicit; you can predict allocations |

### "Reference" means two different things

This is the trap that confuses every JavaScript developer. In JavaScript, "reference" describes how *assignment* works: `b = a` makes two names for one heap object, and mutating through one is visible through the other. In Rust, that automatic-aliasing-on-assignment does **not** happen: `let b = a` *moves* ownership (for heap types) or *copies* (for stack `Copy` types). Rust does have references, written `&a`, but they are an explicit, separate concept called **borrowing**, covered in [Borrowing and References](/05-ownership/02-borrowing/). Do not assume Rust assignment behaves like JavaScript object assignment; it does not.

### No "boxing" surprises

In JavaScript even a humble `number` can end up boxed and the engine may move data around behind your back. In Rust the type *is* the layout. An `i32` is four bytes wherever it appears; a `[u8; 1024]` is exactly 1024 contiguous bytes on the stack. There is no hidden indirection unless you write it (`Box`, `Vec`, `String`, a reference, etc.).

---

## Common Pitfalls

### Pitfall 1: Expecting JavaScript "shared reference" semantics from assignment

Coming from JavaScript, you might expect `let b = a` to make `a` and `b` aliases. For a heap type, Rust *moves* instead, and using the old name afterward is a compile error.

```rust
fn main() {
    let s1 = String::from("hello");
    let s2 = s1; // ownership of the heap buffer moves to s2
    println!("{s1}"); // does not compile (error[E0382]: borrow of moved value: `s1`)
    println!("{s2}");
}
```

**Real compiler error:**

```text
error[E0382]: borrow of moved value: `s1`
 --> src/main.rs:4:16
  |
2 |     let s1 = String::from("hello");
  |         -- move occurs because `s1` has type `String`, which does not implement the `Copy` trait
3 |     let s2 = s1; // ownership of the heap buffer moves to s2
  |              -- value moved here
4 |     println!("{s1}"); // does not compile (error[E0382]: borrow of moved value: `s1`)
  |                ^^ value borrowed here after move
  |
help: consider cloning the value if the performance cost is acceptable
  |
3 |     let s2 = s1.clone(); // ownership of the heap buffer moves to s2
  |                ++++++++
```

The fix depends on intent: borrow with `&s1` if you only need to read it, or `.clone()` if you genuinely want a second independent heap buffer. Why the move happens (and why `i32` would have been fine here) is the subject of [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/).

### Pitfall 2: Trying to put a recursive type entirely on the stack

A type that contains itself has no finite size, so the compiler cannot lay it out on the stack. This is a real (and famous) error:

```rust
// does not compile (error[E0072]: recursive type `List` has infinite size)
enum List {
    Cons(i32, List),
    Nil,
}

fn main() {
    let _list = List::Cons(1, List::Cons(2, List::Nil));
}
```

**Real compiler error (key part):**

```text
error[E0072]: recursive type `List` has infinite size
 --> src/main.rs:2:1
  |
2 | enum List {
  | ^^^^^^^^^
3 |     Cons(i32, List),
  |               ---- recursive without indirection
  |
help: insert some indirection (e.g., a `Box`, `Rc`, or `&`) to break the cycle
  |
3 |     Cons(i32, Box<List>),
  |               ++++    +
```

The fix is exactly what the compiler suggests: put the recursive part behind a pointer (`Box<List>`) so each node is a fixed size. We use this in Exercise 3.

### Pitfall 3: Overflowing the stack with deep recursion or huge arrays

The stack is small. Two ways to blow past it:

- **Unbounded recursion**: every call pushes a frame and they are never popped.
- **A giant fixed-size array as a local** — `let big = [0u8; 50_000_000];` tries to put 50 MB on the stack.

Here is real unbounded recursion:

```rust
// Each call pushes another stack frame; this never returns.
fn count_up(n: u64) -> u64 {
    println!("{n}");
    count_up(n + 1) // no base case: recurses forever
}

fn main() {
    count_up(0);
}
```

This compiles (with an `unconditional_recursion` warning: *function cannot return without recursing*), but at runtime it aborts once the stack is exhausted:

```text
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting
```

(The process exits with code 134.) The fix for huge data is to put it on the heap — `Box`, `Vec`, etc. — where there is room to grow. The fix for recursion is a base case, or rewriting as a loop.

### Pitfall 4: Assuming `number`-style behavior from Rust integers

In JavaScript `number` is always an f64, so very large integers silently **lose precision** (they do not wrap):

```typescript
console.log(9007199254740993); // 9007199254740992  — precision lost, not wrapped
```

In Rust you choose a concrete integer type, and overflow is checked in debug builds (it panics) rather than silently losing precision. This is not strictly a stack/heap issue, but it is the same theme: **Rust makes the representation explicit** instead of hiding it. Integer types and overflow are covered in [Section 02 — Basic Types](/02-basics/01-types/).

---

## Best Practices

### Prefer the stack; reach for the heap only when you need it

Stack values are faster to allocate, faster to access (better cache locality), and freed for free when the frame pops. Use plain values, tuples, and fixed-size arrays for small, fixed-size data. Reach for `String`, `Vec<T>`, or `Box<T>` when the data is growable, large, or its size is unknown at compile time.

### Borrow instead of moving or cloning when you only need to read

If a function just reads a `String`, take `&str` (a borrow), not an owned `String`. You avoid both a move (which would strand the caller's value) and a clone (which would duplicate the heap buffer). Borrowing is the subject of [Borrowing and References](/05-ownership/02-borrowing/), but the memory reason is here: a borrow is a cheap 16-byte pointer-plus-length, never a heap copy.

```rust playground
// reads without taking ownership or copying the heap buffer
fn char_count(text: &str) -> usize {
    text.chars().count()
}

fn main() {
    let name = String::from("Grace Hopper");
    println!("{}", char_count(&name)); // name is still usable afterward
    println!("{name}");
}
```

**Output (verified):**

```text
12
Grace Hopper
```

### Don't reach for `Box` reflexively

A JavaScript developer's instinct is "everything is a reference, so box everything." In Rust, most values should be plain stack values; the compiler and `Vec`/`String` handle heap allocation where it is actually needed. Use `Box<T>` specifically for recursive types, very large values you want to move cheaply, or trait objects (`Box<dyn Trait>`, covered later). Boxing a small `i32` for no reason just adds an indirection.

### Let scope-based cleanup do its job

You never call `free`. Design your code so values live exactly as long as they are needed and let them drop at the end of their scope. Deterministic, scope-based cleanup (RAII) is one of Rust's quiet superpowers and is detailed in [The Drop Trait and RAII](/05-ownership/08-drop-trait/).

---

## Real-World Example

A common back-end task: ingesting telemetry events. A single event mixes small fixed-size fields (IDs, timestamps, flags) with variable-length data (an endpoint string, a list of tags). This is exactly where the stack/heap split shows up in everyday code.

```rust playground
/// A telemetry event ingested from a client. Mixes stack-only fields
/// (fixed-size numbers, a bool) with heap-backed fields (String, Vec).
#[derive(Debug)]
struct TelemetryEvent {
    user_id: u64,      // 8 bytes, stored inline in the struct
    timestamp_ms: u64, // 8 bytes, inline
    is_error: bool,    // 1 byte, inline
    endpoint: String,  // 24-byte handle inline; the text bytes live on the heap
    tags: Vec<String>, // 24-byte handle inline; the elements live on the heap
}

impl TelemetryEvent {
    fn new(user_id: u64, timestamp_ms: u64, endpoint: &str) -> Self {
        TelemetryEvent {
            user_id,
            timestamp_ms,
            is_error: false,
            endpoint: endpoint.to_string(),
            tags: Vec::new(),
        }
    }

    /// Render a one-line summary, reading each field.
    fn summary(&self) -> String {
        let status = if self.is_error { "ERROR" } else { "ok" };
        format!(
            "user={} ts={} {} {} tags={:?}",
            self.user_id, self.timestamp_ms, status, self.endpoint, self.tags
        )
    }
}

fn main() {
    let mut event = TelemetryEvent::new(42, 1_717_000_000_000, "/api/v1/checkout");
    event.is_error = true;
    event.tags.push("payment".to_string());
    event.tags.push("timeout".to_string());

    // The struct value itself is a fixed-size block: the inline fields plus
    // the String/Vec handles. The variable-length text lives on the heap.
    println!(
        "size of TelemetryEvent struct = {} bytes",
        std::mem::size_of::<TelemetryEvent>()
    );
    println!("{}", event.summary());
}
```

**Output (verified):**

```text
size of TelemetryEvent struct = 72 bytes
user=42 ts=1717000000000 ERROR /api/v1/checkout tags=["payment", "timeout"]
```

The struct is a fixed **72 bytes** no matter how long the endpoint or how many tags it carries: 8 + 8 + 1 (rounded up for alignment) + 24 (the `String` handle) + 24 (the `Vec` handle). The actual endpoint text and tag strings live on the heap, reached through those handles. When `event` goes out of scope at the end of `main`, Rust frees the struct *and* every heap buffer it owns, automatically, with no garbage collector and no `free` call.

> **Tip:** Struct field layout (and how `#[derive(Debug)]` lets you print a value with `{:?}`) is covered in [Section 06 — Data Structures](/06-data-structures/). The exact 72 is also affected by alignment and field ordering, which the compiler may optimize.

---

## Further Reading

### Official documentation

- [The Rust Book — What Is Ownership? (the stack and the heap)](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html#the-stack-and-the-heap)
- [The Rust Book — Using `Box<T>` to Point to Data on the Heap](https://doc.rust-lang.org/book/ch15-01-box.html)
- [`std::boxed::Box` API documentation](https://doc.rust-lang.org/std/boxed/struct.Box.html)
- [`std::mem::size_of` API documentation](https://doc.rust-lang.org/std/mem/fn.size_of.html)

### Related sections in this guide

- [Section 05 — Ownership (overview)](/05-ownership/): how this fits into the whole ownership story.
- [The Three Ownership Rules](/05-ownership/01-ownership-rules/): the three rules that govern this memory.
- [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/): why heap handles move and stack values copy.
- [Borrowing and References](/05-ownership/02-borrowing/): references (`&T`), the cheap way to read without owning.
- [The Drop Trait and RAII](/05-ownership/08-drop-trait/): deterministic, scope-based cleanup instead of a garbage collector.
- [Reference Counting with `Rc<T>` and `Arc<T>`](/05-ownership/07-reference-counting/): `Rc`/`Arc` for shared heap ownership.
- [Section 02 — Basic Types](/02-basics/01-types/) — sizes of integers, floats, `char`, and arrays.
- [Section 06 — Data Structures](/06-data-structures/) — how struct fields are laid out.
- [Section 01 — Why Rust?](/01-getting-started/00-why-rust/) — the case for no garbage collector.

---

## Exercises

### Exercise 1: Classify each value's home

**Difficulty:** Easy

**Objective:** Build the mental model of which values live on the stack and which need the heap.

**Instructions:**

1. Create these four bindings: a `u32`, a `String`, a tuple `(f64, f64)`, and a `Vec<f64>`.
2. For each, decide whether it lives entirely on the stack or has a stack handle plus heap data.
3. Use `std::mem::size_of_val(&value)` to print the size of each *stack* representation, and add a comment stating where the rest (if any) lives.

<details>
<summary>Solution</summary>

```rust playground
fn main() {
    let count: u32 = 100; // stack only
    let label = String::from("temp"); // handle on stack, bytes on heap
    let coords = (1.0_f64, 2.0_f64); // stack only (fixed-size tuple)
    let readings = vec![9.8, 10.1]; // handle on stack, elements on heap

    println!("count    -> stack, {} bytes", std::mem::size_of_val(&count));
    println!(
        "label    -> handle on stack ({} bytes) + bytes on heap",
        std::mem::size_of_val(&label)
    );
    println!("coords   -> stack, {} bytes", std::mem::size_of_val(&coords));
    println!(
        "readings -> handle on stack ({} bytes) + elements on heap",
        std::mem::size_of_val(&readings)
    );
}
```

**Verified output:**

```text
count    -> stack, 4 bytes
label    -> handle on stack (24 bytes) + bytes on heap
coords   -> stack, 16 bytes
readings -> handle on stack (24 bytes) + elements on heap
```

The `u32` and the `(f64, f64)` tuple are pure stack values. The `String` and `Vec<f64>` are 24-byte handles on the stack (pointer + length + capacity) that point to buffers on the heap.

</details>

### Exercise 2: Return heap-owned data without dangling

**Difficulty:** Medium

**Objective:** See that returning an owned `String` *moves* its heap buffer out to the caller, so nothing dangles: the opposite of the classic "returning a pointer to a local" bug in C.

**Instructions:**

1. Write `build_greeting(name: &str) -> String` that creates a new `String`, appends `name` and a `'!'`, and returns it.
2. Use `String::from`, `push_str`, and `push`.
3. Return the `String` as a tail expression (no `return` keyword needed) and print the result from `main`.

> **Tip:** Returning the value *moves* ownership of the heap buffer to the caller. Rust statically guarantees the buffer is still alive — you cannot accidentally return a reference to freed memory.

<details>
<summary>Solution</summary>

```rust playground
fn build_greeting(name: &str) -> String {
    let mut greeting = String::from("Hello, ");
    greeting.push_str(name);
    greeting.push('!');
    greeting // ownership of the heap buffer moves out to the caller
}

fn main() {
    let msg = build_greeting("Ada");
    println!("{msg}");
}
```

**Verified output:**

```text
Hello, Ada!
```

The `greeting` buffer was allocated inside the function, but returning it *moves* ownership to `main`. There is no copy of the bytes and no dangling pointer; the heap allocation simply has a new owner.

</details>

### Exercise 3: Use `Box` to give a recursive type a finite size

**Difficulty:** Medium

**Objective:** Fix the "infinite size" error by adding heap indirection, and observe the resulting node size.

**Instructions:**

1. Start from this broken definition (it does not compile; ``error[E0072]: recursive type `List` has infinite size``):

   ```rust
   enum List {
       Cons(i32, List), // recursive without indirection
       Nil,
   }
   ```

2. Wrap the recursive field in a `Box` so each node has a fixed size.
3. Build the list `1 -> 2 -> 3 -> Nil`, then print `size_of::<List>()` and the list itself (derive `Debug`).

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
enum List {
    Cons(i32, Box<List>), // the recursive part now lives behind a pointer
    Nil,
}

use List::{Cons, Nil};

fn main() {
    let list = Cons(1, Box::new(Cons(2, Box::new(Cons(3, Box::new(Nil))))));
    println!("size of List node = {} bytes", std::mem::size_of::<List>());
    println!("{list:?}");
}
```

**Verified output:**

```text
size of List node = 16 bytes
Cons(1, Cons(2, Cons(3, Nil)))
```

`Box<List>` is a single pointer of known size, so each `List` node is a fixed 16 bytes (a discriminant for the enum variant plus the `i32` and the pointer, with alignment padding). The nodes themselves live on the heap, chained by their boxes. `Box` is the gateway to the other smart pointers in [Reference Counting with `Rc<T>` and `Arc<T>`](/05-ownership/07-reference-counting/) and [Section 10 — Smart Pointers](/10-smart-pointers/).

> **Note:** This compiles and runs; the compiler may emit a harmless `dead_code` warning for the enum fields because it analyzes derived `Debug` impls conservatively. The program output above is unaffected.

</details>

---

## Summary

**What you've learned:**

- The **stack** holds fixed-size, short-lived values and is freed automatically as functions return; the **heap** holds growable or unknown-size data.
- `String`, `Vec<T>`, and `Box<T>` keep a small fixed-size **handle on the stack** that points to a buffer on the **heap**.
- TypeScript/JavaScript hides this behind a garbage collector and aliases objects on assignment; Rust makes it explicit and **moves** heap handles instead.
- There is **no garbage collector** in Rust; deterministic, scope-based cleanup is possible precisely because every heap value has a single owner.
- Knowing what lives where is the foundation for understanding moves, borrows, and lifetimes in the rest of this section.

**Mental model:**

- Small + fixed size → stack, copied cheaply.
- Growable / large / unknown size → heap, reached through a handle, moved by default.
- No `malloc`, no `free`, no GC: the compiler frees heap memory when the owner's scope ends.
