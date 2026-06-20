---
title: "Memory Layout: Size, Alignment, and How Rust Packs Your Data"
description: "Rust gives every type a knowable size and alignment. See how field order, padding, repr(C), and niche optimization make Option<&T> cost zero bytes."
---

In TypeScript you never think about how many bytes a `{ price: number, venue: number }` object occupies. V8 owns that decision and hides it behind pointers and hidden classes. In Rust, every type has a precise, knowable **size** and **alignment**, and the way you order struct fields can change how much memory a million of them consume. This page shows you how Rust lays out structs and enums, how field ordering interacts with padding, what `#[repr(...)]` controls, and the "free" niche optimization that makes `Option<&T>` cost nothing.

---

## Quick Overview

Every Rust type has a **size** (`std::mem::size_of`) and an **alignment** (`std::mem::align_of`) that are fixed at compile time. Because the compiler must place each field at an address that respects its alignment, a struct can contain invisible **padding** bytes, and the order you declare fields in can make a struct larger or smaller. Unlike a TypeScript object (whose layout is V8's private business), a Rust struct's layout is something you can measure, reason about, and — when you need to — control with the `#[repr(...)]` attribute.

For a TypeScript/JavaScript developer this matters in two situations: when you hold **huge arrays** of small structs (shaving 16 bytes off a 40-byte struct saves 160 MB across 10 million elements), and when you exchange bytes with C, the network, or the GPU (where the exact layout is part of the contract).

---

## TypeScript/JavaScript Example

```typescript
// TypeScript - you describe the *shape*, never the byte layout.
interface Tick {
  isBuy: boolean;
  price: number; // IEEE-754 f64
  venue: number;
  quantity: number;
}

const tick: Tick = { isBuy: true, price: 101.5, venue: 7, quantity: 100 };
console.log(tick);
// { isBuy: true, price: 101.5, venue: 7, quantity: 100 }

// How big is this object in memory? You cannot say.
// V8 stores it as a "hidden class" + a pointer-laden object on the heap.
// Every `number` is a 64-bit float (or a tagged 31-bit "SMI" if it fits),
// `boolean` is a tagged pointer-sized slot, and the JIT may change the
// representation as the program runs. The developer has zero control.

// The ONLY place JavaScript exposes byte layout is typed arrays / ArrayBuffer:
const buf = new ArrayBuffer(16);
const view = new DataView(buf);
view.setFloat64(0, 101.5, true); // write price at offset 0, little-endian
view.setUint32(8, 100, true); // write quantity at offset 8
console.log("byteLength =", buf.byteLength); // 16
console.log("price back =", view.getFloat64(0, true)); // 101.5
```

**Key points:**

- A plain object's memory layout is **decided by V8** and can change at runtime; you cannot query its byte size or field offsets.
- Every JavaScript `number` is an IEEE-754 `f64` (8 bytes); there is no `u8`, `i32`, or `f32`. Big integers lose precision; they do **not** wrap.
- The only way to control exact bytes is `ArrayBuffer` + `DataView`/typed arrays, and even there *you* compute every offset by hand.

> **Note:** The Node output above is real: `console.log(tick)` prints the object's fields, not `[object Object]`. That string only appears from implicit coercion like `"" + tick`.

---

## Rust Equivalent

```rust playground
use std::mem::{align_of, size_of};

// Rust - the struct *is* the layout. Each field has a concrete, sized type.
struct Tick {
    is_buy: bool, // 1 byte
    price: f64,   // 8 bytes
    venue: u8,    // 1 byte
    quantity: u32, // 4 bytes
}

fn main() {
    // size_of and align_of are const fns evaluated at compile time.
    println!("size  = {}", size_of::<Tick>());
    println!("align = {}", align_of::<Tick>());

    let tick = Tick {
        is_buy: true,
        price: 101.5,
        venue: 7,
        quantity: 100,
    };
    println!("price = {}", tick.price);
}
```

Running it:

```text
size  = 16
align = 8
price = 101.5
```

The fields you wrote add up to `1 + 8 + 1 + 4 = 14` bytes, yet the struct reports **16**. The extra two bytes are **padding**, and to understand where they come from (and why Rust gets away with only two bytes of waste where C would need ten) you need to understand alignment and Rust's freedom to reorder fields.

---

## Detailed Explanation

### Size and alignment, the two numbers every type carries

Every Rust type `T` has:

- **`size_of::<T>()`**: how many bytes one value occupies, including any internal padding. Arrays and `Vec`s stride by exactly this many bytes per element.
- **`align_of::<T>()`**: the byte boundary an address must be a multiple of. A `u32` (align 4) can only live at addresses `0, 4, 8, …`; a `u64` (align 8) at `0, 8, 16, …`.

Here are the primitives a TypeScript developer should memorize. All output below is real, from a probe program:

```rust playground
use std::mem::{align_of, size_of};

fn main() {
    macro_rules! show {
        ($t:ty) => {
            println!("{:<10} size={} align={}", stringify!($t), size_of::<$t>(), align_of::<$t>());
        };
    }
    show!(bool);
    show!(u8);
    show!(u16);
    show!(u32);
    show!(u64);
    show!(char);
    show!(f64);
    show!(usize);
    show!(&u8);
    show!(String);
    show!(Vec<u8>);
    show!(());
}
```

```text
bool       size=1 align=1
u8         size=1 align=1
u16        size=2 align=2
u32        size=4 align=4
u64        size=8 align=8
char       size=4 align=4
f64        size=8 align=8
usize      size=8 align=8
&u8        size=8 align=8
String     size=24 align=8
Vec<u8>    size=24 align=8
()         size=0 align=1
```

Two things jump out for a TypeScript developer:

- A `char` is **4 bytes**, not 1: it holds a full Unicode scalar value, not a byte.
- `String` and `Vec<u8>` are **24 bytes** regardless of content. That is three machine words: a heap pointer, a length, and a capacity. The actual text lives on the heap, exactly like a JavaScript string's backing store lives off to the side.
- The unit type `()` has **size 0**: Rust has genuine zero-sized types, something JavaScript has no equivalent for.

### Why padding exists

Hardware reads memory most efficiently when a value sits at an address that is a multiple of its size. To guarantee that, the compiler inserts **padding** bytes so each field lands on a properly aligned offset, and pads the whole struct up to a multiple of its largest field's alignment (so that in an array, element *N+1* is just as aligned as element 0).

A struct's alignment is the **maximum** alignment of its fields. For `Tick`, the `f64` forces alignment 8, so `size_of::<Tick>()` must be a multiple of 8: hence 16, not 14.

### Rust reorders fields for you (the big difference from C and from TypeScript)

Here is the surprise. Naively, you'd expect `is_buy, price, venue, quantity` to lay out like this with padding:

```text
[is_buy:1][pad:7][price:8][venue:1][pad:3][quantity:4]  = 24 bytes
```

But Rust reported **16**. That is because the default representation, called **`repr(Rust)`**, gives the compiler permission to **reorder fields** to minimize padding. It silently sorts the fields into something like `price(8), quantity(4), venue(1), is_buy(1)` plus two trailing pad bytes, reaching a tight 16. C never does this (declaration order is part of C's ABI), which is exactly why Rust can be more compact than the equivalent C struct without you lifting a finger.

You can see the difference by *forbidding* reordering with `#[repr(C)]`, which pins fields to declaration order:

```rust playground
use std::mem::{align_of, size_of};

#[repr(C)] // C layout: fields stay in declaration order
struct CBad {
    flag: bool, // offset 0, then 7 bytes of padding
    id: u64,    // offset 8
    code: u16,  // offset 16, then 6 bytes of trailing padding
}

#[repr(C)] // same fields, ordered largest-to-smallest by hand
struct CGood {
    id: u64,    // offset 0
    code: u16,  // offset 8
    flag: bool, // offset 10, then 5 bytes of trailing padding
}

struct RustOrder {
    // default repr(Rust): compiler reorders for us
    flag: bool,
    id: u64,
    code: u16,
}

fn main() {
    println!("repr(C)  CBad     size={} align={}", size_of::<CBad>(), align_of::<CBad>());
    println!("repr(C)  CGood    size={} align={}", size_of::<CGood>(), align_of::<CGood>());
    println!("repr(Rust) Order  size={} align={}", size_of::<RustOrder>(), align_of::<RustOrder>());
}
```

```text
repr(C)  CBad     size=24 align=8
repr(C)  CGood    size=16 align=8
repr(Rust) Order  size=16 align=8
```

The lesson: **field order only affects size when you opt out of `repr(Rust)`'s reordering.** Under default `repr(Rust)`, declaration order is purely a readability choice: the compiler will pack it optimally either way. Under `#[repr(C)]`, *you* are responsible for ordering largest-aligned fields first.

### Inspecting offsets with `offset_of!`

Since Rust 1.77 the standard library has `std::mem::offset_of!`, which tells you exactly where a field sits, invaluable for `repr(C)` structs that mirror a C header or a wire format:

```rust playground
use std::mem::{align_of, offset_of, size_of};

#[repr(C)]
struct Record {
    a: u8,
    b: u32,
    c: u16,
}

fn main() {
    println!("offset a = {}", offset_of!(Record, a));
    println!("offset b = {}", offset_of!(Record, b));
    println!("offset c = {}", offset_of!(Record, c));
    println!("size  = {}", size_of::<Record>());
    println!("align = {}", align_of::<Record>());
}
```

```text
offset a = 0
offset b = 4
offset c = 8
size  = 12
align = 4
```

`a` is one byte at offset 0; `b` (align 4) cannot start at offset 1, so it jumps to offset 4 (three padding bytes between); `c` follows at offset 8; the whole struct rounds up to 12 to keep align 4.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Who decides layout | V8 (hidden classes, may change at runtime) | The compiler, deterministically at compile time |
| Can you measure a value's byte size? | No (only `ArrayBuffer.byteLength` for raw buffers) | Yes — `std::mem::size_of::<T>()` |
| Numeric widths | One type: `number` = `f64` | `u8/u16/u32/u64`, `i*`, `f32/f64`, `usize`, `bool` (1 byte), `char` (4 bytes) |
| Field ordering | Irrelevant (no observable layout) | Irrelevant under `repr(Rust)`; **load-bearing** under `repr(C)` |
| Padding | Hidden, not your concern | Inserted for alignment; visible in `size_of` |
| Controlling exact bytes | `ArrayBuffer` + manual offsets | `#[repr(C)]`, `#[repr(packed)]`, `#[repr(align(N))]` |
| `Option<T>` overhead | `T | null` is just a tagged slot | Often **zero** extra bytes (niche optimization) |
| Enum size | N/A (unions are compile-time only) | Discriminant + largest variant, aligned |

### Niche optimization: why `Option<&T>` is free

This is the single most delightful layout trick in Rust. A reference (`&T`), a `Box<T>`, and an `NonZero*` integer can never be all-zeroes/null. The compiler treats that forbidden bit pattern as a **niche** and reuses it to represent `None`, so wrapping such a type in `Option` costs **zero extra bytes**:

```rust playground
use std::mem::size_of;
use std::num::NonZeroU32;

fn main() {
    println!("&u8                 {}", size_of::<&u8>());
    println!("Option<&u8>         {}", size_of::<Option<&u8>>());
    println!("Box<u8>             {}", size_of::<Box<u8>>());
    println!("Option<Box<u8>>     {}", size_of::<Option<Box<u8>>>());
    println!("u32                 {}", size_of::<u32>());
    println!("Option<u32>         {}", size_of::<Option<u32>>());
    println!("NonZeroU32          {}", size_of::<NonZeroU32>());
    println!("Option<NonZeroU32>  {}", size_of::<Option<NonZeroU32>>());
    println!("bool                {}", size_of::<bool>());
    println!("Option<bool>        {}", size_of::<Option<bool>>());
    println!("char                {}", size_of::<char>());
    println!("Option<char>        {}", size_of::<Option<char>>());
    println!("String              {}", size_of::<String>());
    println!("Option<String>      {}", size_of::<Option<String>>());
}
```

```text
&u8                 8
Option<&u8>         8
Box<u8>             8
Option<Box<u8>>     8
u32                 4
Option<u32>         8
NonZeroU32          4
Option<NonZeroU32>  4
bool                1
Option<bool>        1
char                4
Option<char>        4
String              24
Option<String>      24
```

Read those pairs carefully:

- `Option<&u8>`, `Option<Box<u8>>`, `Option<NonZeroU32>`, `Option<String>` are the **same size** as the thing inside. `None` reuses the null/zero bit pattern: no separate flag byte.
- `Option<u32>` jumps from 4 to **8 bytes**: a plain `u32` has no spare bit pattern (all 4 billion values are valid), so the compiler must add a discriminant, and alignment rounds it up to 8.
- `Option<bool>` stays at 1 byte: `bool` only uses values 0 and 1, leaving 254 niche values, so `None` slots into one of them.

This is the layout-level reason Rust's `Option<&T>` is the right way to express "a nullable pointer": it is exactly as cheap as the `T | null` you reach for in TypeScript, but checked by the compiler. The niche even nests: `Option<Option<bool>>` is still 1 byte.

### Enum sizes: tag plus the biggest variant

A Rust `enum` is a **tagged union**: it stores a discriminant (which variant) plus enough room for the largest variant's payload, all rounded to the enum's alignment.

```rust playground
use std::mem::size_of;

enum Direction {
    North,
    South,
    East,
    West,
}

enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Point,
}

enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
    Color(u8, u8, u8),
}

fn main() {
    println!("Direction (4 unit variants) {}", size_of::<Direction>());
    println!("Shape (mixed payloads)      {}", size_of::<Shape>());
    println!("Message (one big variant)   {}", size_of::<Message>());
}
```

```text
Direction (4 unit variants) 1
Shape (mixed payloads)      24
Message (one big variant)   24
```

- `Direction` carries no data, so it is just a 1-byte discriminant, like a TypeScript string-literal union compiled down to a single byte.
- `Shape`'s largest variant is `Rectangle(f64, f64)` = 16 bytes; the discriminant plus alignment round the whole enum to 24.
- `Message` is dominated by `Write(String)` (24 bytes); the discriminant is folded into the `String`'s niche, so the enum is still 24.

> **Warning:** An enum is as big as its *largest* variant, even when most values are small. If one variant is huge (say `Payload([u8; 256])`), every value of that enum reserves 256+ bytes. The fix is to box the fat variant; see Best Practices below.

---

## Common Pitfalls

### Pitfall 1: Taking a reference into a `#[repr(packed)]` struct

`#[repr(packed)]` removes all padding (alignment 1), which is handy for parsing wire formats, but a field is then potentially misaligned, and Rust forbids creating a reference to it because a misaligned reference is undefined behavior:

```rust
#[repr(packed)]
struct Packed {
    flag: bool,
    id: u64,
}

fn main() {
    let p = Packed { flag: true, id: 42 };
    let r = &p.id; // does not compile (error[E0793])
    println!("{}", r);
}
```

The real `rustc` error:

```text
error[E0793]: reference to packed field is unaligned
 --> src/main.rs:9:13
  |
9 |     let r = &p.id; // does not compile (error[E0793])
  |             ^^^^^
  |
  = note: packed structs are only aligned by one byte, and many modern architectures penalize unaligned field accesses
  = note: creating a misaligned reference is undefined behavior (even if that reference is never dereferenced)
  = help: copy the field contents to a local variable, or replace the reference with a raw pointer and use `read_unaligned`/`write_unaligned` (loads and stores via `*p` must be properly aligned even when using raw pointers)
```

The fix the compiler suggests is to **copy the field out** first. `let id = p.id;` reads it by value (a `Copy` field), and then `&id` is a normal aligned reference. Reach for `#[repr(packed)]` only when an external format truly demands it.

### Pitfall 2: Assuming declaration order controls size

Coming from C (or from over-thinking it), TypeScript developers sometimes obsess over field order in plain Rust structs. Under the default `repr(Rust)`, **order does not change the size** — the compiler reorders for you. Order only matters once you add `#[repr(C)]`. Don't contort your struct's readability for layout you aren't actually controlling.

### Pitfall 3: Expecting a stable layout from `repr(Rust)`

Because the compiler is free to reorder, you must **never** assume a particular field offset, transmute between two `repr(Rust)` structs with "the same fields," or send a `repr(Rust)` struct's raw bytes across an FFI or network boundary. The layout is unspecified and may differ between compiler versions. The moment bytes matter, add `#[repr(C)]`. This is covered in depth alongside FFI in [Section 20](/20-unsafe-ffi/03-ffi-basics/).

### Pitfall 4: A giant enum variant bloating a hot `Vec`

```rust playground
use std::mem::size_of;

enum Cmd {
    Ping,
    Payload([u8; 256]),
}

fn main() {
    // Even a `Cmd::Ping` value reserves room for the 256-byte array.
    println!("Cmd = {} bytes", size_of::<Cmd>());
}
```

```text
Cmd = 257 bytes
```

A `Vec<Cmd>` of mostly `Ping`s wastes 256 bytes per element. The fix is in Best Practices.

---

## Best Practices

### Let `repr(Rust)` do the packing; only override when bytes leave your program

For ordinary in-memory types, do nothing — the default representation already minimizes padding. Add a `#[repr(...)]` only for a concrete reason:

| Attribute | What it does | When to use it |
| --- | --- | --- |
| (none) `repr(Rust)` | Compiler reorders fields, minimal padding, unspecified layout | Almost always — normal application types |
| `#[repr(C)]` | Declaration-order layout, C-compatible | FFI, memory-mapped files, GPU buffers, anything `transmute`d or sent over the wire |
| `#[repr(packed)]` | Removes all padding, alignment 1 | Tight binary/wire formats; pairs with `read_unaligned` |
| `#[repr(align(N))]` | **Raises** alignment to N | Avoiding false sharing (one value per cache line) |
| `#[repr(u8)]` / `#[repr(u16)]`… on an enum | Fixes discriminant size and makes `as` casts well-defined | Protocol opcodes, C enums |

### When you *must* use `#[repr(C)]`, order fields largest-aligned first

Since C layout honors declaration order, put 8-byte fields, then 4-byte, then 2-byte, then 1-byte, then zero-sized. This minimizes interior padding — the difference between `CBad` (24 bytes) and `CGood` (16 bytes) above.

### Box the fat variant to shrink an enum

```rust playground
use std::mem::size_of;

enum Cmd {
    Ping,
    Payload(Box<[u8; 256]>), // the 256 bytes now live on the heap
}

fn main() {
    println!("Cmd = {} bytes", size_of::<Cmd>());
}
```

```text
Cmd = 8 bytes
```

The enum shrinks from 257 bytes to 8 (a single pointer), so a `Vec<Cmd>` of mostly `Ping`s no longer wastes 256 bytes per element. You pay one heap allocation *only* when you actually build a `Payload`. Clippy's `large_enum_variant` lint flags this pattern for you. `Box` is covered in [Section 10](/10-smart-pointers/00-box/).

### Fix integer discriminants for protocol enums

```rust playground
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
enum Opcode {
    Get = 0x01,
    Set = 0x02,
    Delete = 0x03,
}

fn main() {
    println!("Opcode is {} byte(s)", std::mem::size_of::<Opcode>());
    println!("Set on the wire = 0x{:02X}", Opcode::Set as u8);
}
```

```text
Opcode is 1 byte(s)
Set on the wire = 0x02
```

`#[repr(u8)]` guarantees the enum is exactly one byte and that `Opcode::Set as u8` yields the explicit `0x02`, which is what you want when serializing a packet header.

### Measure, don't guess

`size_of` and `align_of` are `const fn`s; drop them into a `#[test]` to assert a layout never regresses (`assert_eq!(size_of::<Tick>(), 16)`), or use the `cargo bloat`-style tooling and the `-Zprint-type-sizes` nightly flag to audit large types. Profiling and measurement come first; see [When to Optimize](/21-performance/10-when-to-optimize/).

---

## Real-World Example

A market-data feed handler holds tens of millions of ticks in memory. The struct is tiny, but multiplied across the buffer, layout decides whether the working set fits in RAM (and in cache). Here we contrast a naive ordering that mirrors how you'd write the TypeScript interface against a hand-packed `#[repr(C)]` layout — both pinned to C order so the difference is visible:

```rust playground
use std::mem::size_of;

// Naive order, mirroring a TypeScript interface field-by-field.
#[allow(dead_code)]
#[repr(C)]
struct TickNaive {
    is_buy: bool,      // 1
    price: f64,        // 8  (forces 7 bytes of padding before it)
    venue: u8,         // 1
    quantity: u32,     // 4  (3 bytes of padding before it)
    timestamp_ns: u64, // 8
    flags: u16,        // 2  (6 bytes of trailing padding)
}

// Optimized: largest-aligned fields first.
#[allow(dead_code)]
#[repr(C)]
struct TickPacked {
    price: f64,        // 8
    timestamp_ns: u64, // 8
    quantity: u32,     // 4
    flags: u16,        // 2
    venue: u8,         // 1
    is_buy: bool,      // 1
}

fn main() {
    let n = 10_000_000usize;
    println!(
        "TickNaive  = {} bytes -> {} MB for {} ticks",
        size_of::<TickNaive>(),
        size_of::<TickNaive>() * n / 1_000_000,
        n
    );
    println!(
        "TickPacked = {} bytes -> {} MB for {} ticks",
        size_of::<TickPacked>(),
        size_of::<TickPacked>() * n / 1_000_000,
        n
    );
}
```

```text
TickNaive  = 40 bytes -> 400 MB for 10000000 ticks
TickPacked = 24 bytes -> 240 MB for 10000000 ticks
```

Reordering fields shaved 16 bytes off each tick — a **40% memory reduction**, 160 MB saved across ten million elements, with zero change to behavior. Smaller elements also mean more of them per cache line, which is the bridge to [Cache-Friendly Code](/21-performance/05-cache-efficiency/). In JavaScript this optimization is simply unavailable: an array of plain objects is an array of pointers to heap objects whose layout V8 controls, and the only escape hatch — a single packed `Float64Array`/`DataView` — forces you to abandon named fields and compute every offset by hand.

> **Tip:** If you control the source order and use the default `repr(Rust)`, you get the 24-byte layout *automatically* — you only need to hand-order fields when `#[repr(C)]` is in play (here, so the struct matches an external feed format).

---

## Further Reading

- [The Rustonomicon — Data Layout](https://doc.rust-lang.org/nomicon/data.html) — the authoritative reference on `repr`, niches, and exotic layouts.
- [Type Layout — Rust Reference](https://doc.rust-lang.org/reference/type-layout.html) — the precise rules for size, alignment, and every `#[repr(...)]`.
- [`std::mem::size_of`](https://doc.rust-lang.org/std/mem/fn.size_of.html), [`align_of`](https://doc.rust-lang.org/std/mem/fn.align_of.html), and [`offset_of!`](https://doc.rust-lang.org/std/mem/macro.offset_of.html) — the standard-library tools used throughout this page.
- [Structs](/06-data-structures/00-structs/) and [Enums](/06-data-structures/02-enums/) — the data-modeling fundamentals these layout rules apply to.
- [Box](/10-smart-pointers/00-box/) — heap allocation, used to shrink fat enum variants.
- [FFI Basics](/20-unsafe-ffi/03-ffi-basics/) — where `#[repr(C)]` becomes mandatory.
- Sibling performance topics: [Optimization Techniques](/21-performance/03-optimization/) · [Cache Efficiency](/21-performance/05-cache-efficiency/) · [Zero-Cost Abstractions](/21-performance/06-zero-cost/) · [Benchmarking](/21-performance/02-benchmarking/) · [When to Optimize](/21-performance/10-when-to-optimize/).
- Next section: [Common Patterns](/22-common-patterns/).
- Foundations: [Introduction](/00-introduction/) · [Getting Started](/01-getting-started/) · [Basics](/02-basics/).

---

## Exercises

### Exercise 1: Shrink a `repr(C)` struct by reordering

**Difficulty:** Easy

**Objective:** See first-hand how field order changes the size of a `#[repr(C)]` struct.

**Instructions:** Define a `#[repr(C)]` struct `Bad` with fields in this order: `active: bool`, `score: f64`, `level: u8`, `xp: u32`. Print its size. Then define a second `#[repr(C)]` struct `Good` with the *same four fields reordered* so the struct is as small as possible, and print its size. Explain the difference. (Add `#[allow(dead_code)]` to silence unused-field warnings.)

<details>
<summary>Solution</summary>

```rust playground
use std::mem::size_of;

#[allow(dead_code)]
#[repr(C)]
struct Bad {
    active: bool, // 1 + 7 padding
    score: f64,   // 8
    level: u8,    // 1 + 3 padding
    xp: u32,      // 4
}

#[allow(dead_code)]
#[repr(C)]
struct Good {
    score: f64,   // 8
    xp: u32,      // 4
    level: u8,    // 1
    active: bool, // 1  (+2 trailing padding)
}

fn main() {
    println!("Bad  = {} bytes", size_of::<Bad>());
    println!("Good = {} bytes", size_of::<Good>());
}
```

Output:

```text
Bad  = 24 bytes
Good = 16 bytes
```

In `Bad`, the `bool` and `u8` sit before larger-aligned fields, forcing the compiler to insert padding so `score` and `xp` land on aligned offsets. In `Good`, ordering largest-aligned first (`f64`, then `u32`, then the two single bytes) leaves only two trailing padding bytes, shrinking the struct from 24 to 16. Note that with the default `repr(Rust)` *both* orderings would already be 16 — the reordering matters only because we opted into C layout.

</details>

### Exercise 2: Predict niche-optimized sizes

**Difficulty:** Medium

**Objective:** Build intuition for when `Option` is free and when it costs an extra word.

**Instructions:** Before running anything, predict the `size_of` for each of these, then write a program that prints them and check yourself: `Option<&u32>`, `Option<u32>`, `Option<bool>`, a three-variant unit enum `enum Tri { A, B, C }`, `Option<Tri>`, and `Option<Box<[u8; 256]>>`. For each, say in a comment whether the niche optimization applied and why.

<details>
<summary>Solution</summary>

```rust playground
use std::mem::size_of;

#[allow(dead_code)]
enum Tri {
    A,
    B,
    C,
}

fn main() {
    // Niche applies: references are never null, so None reuses the 0 pattern.
    println!("Option<&u32>          {}", size_of::<Option<&u32>>()); // 8

    // No niche: every u32 bit pattern is valid, so a discriminant is added
    // and alignment rounds the total to 8.
    println!("Option<u32>           {}", size_of::<Option<u32>>()); // 8

    // Niche applies: bool only uses 0 and 1, leaving 254 spare values.
    println!("Option<bool>          {}", size_of::<Option<bool>>()); // 1

    // A 3-variant unit enum needs only a 1-byte discriminant...
    println!("Tri                   {}", size_of::<Tri>()); // 1

    // ...and it has spare discriminant values, so Option reuses one: still 1.
    println!("Option<Tri>           {}", size_of::<Option<Tri>>()); // 1

    // Niche applies: Box is never null, so the Option is just a pointer.
    println!("Option<Box<[u8;256]>> {}", size_of::<Option<Box<[u8; 256]>>>()); // 8
}
```

Output:

```text
Option<&u32>          8
Option<u32>           8
Option<bool>          1
Tri                   1
Option<Tri>           1
Option<Box<[u8;256]>> 8
```

The pattern: if a type has a forbidden bit pattern (null pointer, the unused range of a `bool`, the spare discriminants of a small enum), `Option` is free. Only types that use *every* bit pattern — like `u32` — pay for a separate discriminant.

</details>

### Exercise 3: A byte-exact protocol opcode

**Difficulty:** Advanced

**Objective:** Combine `#[repr(u8)]`, explicit discriminants, `as` casts, and a fallible decoder to model a one-byte wire opcode.

**Instructions:** Define `#[repr(u8)]` enum `Opcode { Hello = 0x10, Data = 0x20, Bye = 0x30 }` deriving `Debug, Clone, Copy, PartialEq`. Add an associated function `from_byte(b: u8) -> Option<Opcode>` that maps known bytes to variants and returns `None` otherwise. In `main`, assert the opcode is exactly one byte, encode `Opcode::Data` to its byte with an `as` cast and print it in hex, then decode both a valid byte (`0x30`) and an invalid one (`0x99`).

<details>
<summary>Solution</summary>

```rust playground
use std::mem::size_of;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
enum Opcode {
    Hello = 0x10,
    Data = 0x20,
    Bye = 0x30,
}

impl Opcode {
    fn from_byte(b: u8) -> Option<Opcode> {
        match b {
            0x10 => Some(Opcode::Hello),
            0x20 => Some(Opcode::Data),
            0x30 => Some(Opcode::Bye),
            _ => None,
        }
    }
}

fn main() {
    assert_eq!(size_of::<Opcode>(), 1);

    let wire: u8 = Opcode::Data as u8;
    println!("Data on the wire = 0x{:02X}", wire);

    println!("decode 0x30 = {:?}", Opcode::from_byte(0x30));
    println!("decode 0x99 = {:?}", Opcode::from_byte(0x99));
}
```

Output:

```text
Data on the wire = 0x20
decode 0x30 = Some(Bye)
decode 0x99 = None
```

`#[repr(u8)]` guarantees the single-byte size and gives the `as u8` cast a well-defined result (the explicit discriminant). Because a raw byte off the network can hold *any* of 256 values, well beyond the three you defined, the decoder must be fallible: `from_byte` returns `Option<Opcode>`, never an invalid `Opcode`. This is the type-safe alternative to casting a stray byte straight into an enum, which would be undefined behavior.

</details>
