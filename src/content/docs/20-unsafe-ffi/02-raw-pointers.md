---
title: "Raw Pointers: `*const T` and `*mut T`"
description: "Rust's *const T and *mut T are unchecked C-style addresses with no guarantees. Create and cast them safely; only dereferencing is unsafe."
---

Raw pointers are Rust's unchecked, C-style memory addresses. They are the building block beneath every reference, every smart pointer, and every line of FFI code, and unlike references, the compiler makes you no promises about them. This page explains what they are, how to create and cast them, and exactly how they differ from the `&T` / `&mut T` references you already know.

---

## Quick Overview

A **raw pointer** is just a memory address with a type attached. Rust has two flavors: `*const T` (read-only) and `*mut T` (read-write). You can **create** and **cast** them in completely safe code; they are inert numbers until you actually follow them. The moment you **dereference** one to read or write the pointed-to value, you must do it inside an `unsafe` block, because at that point the compiler can no longer guarantee the address is non-null, aligned, pointing at live memory, or free of aliasing conflicts.

For a TypeScript or JavaScript developer this is a genuinely new concept: JS has no pointers at all. The closest mental model is a numeric index into a giant `ArrayBuffer`: a number that *might* point at valid data, with nothing stopping you from indexing past the end. Raw pointers exist for FFI, for performance-critical data structures, and as the unsafe core that safe abstractions are built on. You will rarely write them in everyday application code; references are almost always the right tool.

> **Note:** Creating and casting raw pointers is safe. Only **dereferencing** them is `unsafe`. The five "unsafe superpowers" and what `unsafe` actually means are covered in [What `unsafe` Really Means](/20-unsafe-ffi/00-unsafe-intro/); this page focuses specifically on the raw-pointer types.

---

## TypeScript/JavaScript Example

JavaScript and TypeScript do not have pointers: the engine manages every reference for you and the garbage collector guarantees you can never hold a reference to freed memory. The lowest-level thing the language exposes is a **typed view over a raw byte buffer**. That comes closest to the spirit of raw pointers: you get a base buffer and you address into it by *offset*, with the runtime doing bounds checks but no type checks beyond the view's element type.

```typescript
// TypeScript — a typed view over a raw byte buffer is the nearest analogue.
// `ArrayBuffer` is a fixed block of bytes; views interpret those bytes.
const buffer = new ArrayBuffer(8); // 8 raw bytes

// Two views over the SAME bytes, reinterpreting them differently.
const asU32 = new Uint32Array(buffer); // 2 x 32-bit unsigned ints
const asU8 = new Uint8Array(buffer); // 8 x 8-bit bytes

asU32[0] = 0x01020304;

// The u8 view sees the individual bytes of that same 32-bit write.
console.log([...asU8.slice(0, 4)]); // [4, 3, 2, 1] on a little-endian machine

// "Pointer arithmetic": index 1 of the u32 view is byte offset 4.
asU32[1] = 0xffffffff;
console.log(asU8[4], asU8[7]); // 255 255

// Out-of-bounds access does NOT crash — JS quietly returns undefined.
console.log(asU32[99]); // undefined (no segfault, no error)
```

**Key points:**

- The base address is hidden; you address memory by **offset/index** into a view.
- Multiple views can **alias** the same bytes and reinterpret them (a `Uint32Array` and a `Uint8Array` over one `ArrayBuffer`). This is the JS equivalent of *casting* a pointer to a different element type.
- The runtime protects you: an out-of-bounds read returns `undefined`, never undefined behavior. There is no way to dereference freed memory, because the GC never frees memory you can still reach.

That last guarantee is exactly what Rust's raw pointers give up.

---

## Rust Equivalent

A raw pointer is a typed address. You can build one from a reference, from an integer, or by casting another pointer. Reading or writing through it happens in `unsafe`.

```rust
fn main() {
    // ── Creating raw pointers is SAFE ──────────────────────────────
    let mut x: i32 = 42;
    let r1: *const i32 = &x; // *const: coerced from a shared reference
    let r2: *mut i32 = &mut x; // *mut: coerced from a mutable reference
    println!("addresses: {r1:p} {r2:p}");

    // Dereferencing is UNSAFE — the compiler can no longer vouch for it.
    unsafe {
        println!("via *const: {}", *r1); // read
        *r2 = 99; // write
        println!("via *mut:   {}", *r2);
    }
    println!("x is now {x}");

    // ── A pointer from an arbitrary integer address ───────────────
    let arbitrary = 0x1234usize as *const u8; // safe to make...
    println!("arbitrary points at {arbitrary:p}"); // ...and to print
    // (dereferencing it would almost certainly be undefined behavior)

    // ── Casting between pointer element types (reinterpret bytes) ──
    // Start from a real u32 so the address is guaranteed 4-byte aligned.
    let n: u32 = 1;
    let p: *const u8 = &n as *const u32 as *const u8;
    let as_u32: *const u32 = p as *const u32; // like the Uint8/Uint32 views
    unsafe {
        println!("reinterpreted: {}", *as_u32); // 1
    }

    // ── Pointer arithmetic into a buffer (disjoint => no aliasing) ─
    let mut data = vec![1, 2, 3, 4];
    let ptr = data.as_mut_ptr();
    unsafe {
        let a = &mut *ptr; // element 0
        let b = &mut *ptr.add(1); // element 1 — a different element
        *a += 10;
        *b += 20;
    }
    println!("data = {data:?}");

    // ── Null pointers ─────────────────────────────────────────────
    let n: *const i32 = std::ptr::null();
    println!("null? {}", n.is_null());
}
```

Real output (`cargo run`; the two hex addresses vary per run):

```text
addresses: 0x16b45197c 0x16b45197c
via *const: 42
via *mut:   99
x is now 99
arbitrary points at 0x1234
reinterpreted: 1
data = [11, 22, 3, 4]
null? true
```

> **Note:** `r1` and `r2` print the *same* address because they point at the same `x`, which is fine, since `*const` and `*mut` are about your *intent* and capabilities, not separate locations. Two simultaneous `&mut` to `x` would be rejected by the borrow checker (see [Mutable References](/05-ownership/03-mutable-references/)); two raw pointers are not. That freedom is precisely why dereferencing them is `unsafe`.

---

## Detailed Explanation

### Creating pointers is safe; dereferencing is not

```rust
let mut x: i32 = 42;
let r1: *const i32 = &x;
let r2: *mut i32 = &mut x;
```

These two lines compile in completely safe code. A raw pointer is "just a number": making one cannot trigger undefined behavior, and neither can passing it around, comparing it, or printing it with the `{:p}` format specifier. The address only becomes dangerous when you **follow** it:

```rust
unsafe {
    println!("via *const: {}", *r1);
    *r2 = 99;
}
```

`*r1` and `*r2` are *dereferences*. They require `unsafe` because the compiler has no way to prove the four invariants every dereference must uphold:

1. The pointer is **non-null**.
2. It is **aligned** for `T` (an `i32` pointer must be 4-byte aligned).
3. It points to a **live, initialized** `T` (not freed, not uninitialized memory).
4. Following it does not **violate aliasing** (e.g., creating a `&mut` while another live reference exists).

You promise all four; the compiler trusts you. Break any one and you have undefined behavior. See [What `unsafe` Really Means](/20-unsafe-ffi/00-unsafe-intro/) for the full list of invariants and what UB is.

### `*const T` vs `*mut T`

The two pointer types differ in *intent* and the operations they permit:

- `*const T`: you intend to **read**. You cannot write through it without first casting to `*mut T`.
- `*mut T`: you intend to **read and write**.

Unlike references, these are **not** enforced exclusivity rules. You can freely have many `*const` and many `*mut` to the same location at the same time, and you can cast between them with `as`. The `const`/`mut` distinction is documentation and a lint barrier, not a guarantee. The guarantee only comes back when you convert a raw pointer into a reference.

### Casting reinterprets the bytes

```rust
let n: u32 = 1;
let p: *const u8 = &n as *const u32 as *const u8;
let as_u32: *const u32 = p as *const u32;
```

`as` casts between pointer types without changing the address. It changes only how the bytes at that address are *interpreted* when dereferenced. This is the direct analogue of layering a `Uint32Array` over a `Uint8Array` on the same `ArrayBuffer`. The danger: after casting `*const u8 → *const u32`, dereferencing now reads **four** bytes and requires **4-byte alignment**. The example dereferences `as_u32` soundly *only because* the original allocation is a `u32`, which is guaranteed 4-byte aligned. Had the bytes come from a `[u8; 4]` instead — whose alignment is just **1** (`align_of::<[u8; 4]>() == 1`) — the address would *not* be guaranteed 4-aligned, and a plain `*as_u32` deref would be undefined behavior even though it might print the right number on your machine. In that case you must read it with `(p as *const u32).read_unaligned()`. This is the classic alignment trap.

### Pointer arithmetic with `.add()` / `.offset()`

```rust
let ptr = data.as_mut_ptr();
let b = &mut *ptr.add(1); // move forward by one element (not one byte)
```

`.add(n)` and `.offset(n)` advance a pointer by `n` **elements** of `T` (so `n * size_of::<T>()` bytes), exactly like `Uint32Array` index `1` being byte offset `4`. These methods are `unsafe`: the result must stay within (or one past the end of) the same allocation, or you get UB even *before* you dereference. There is a safe-but-rarely-what-you-want sibling, `.wrapping_add(n)`, which never claims the result is in-bounds.

### References coerce *to* pointers automatically

`let r1: *const i32 = &x;` works without an `as` cast: a `&T` coerces to `*const T` and a `&mut T` coerces to `*mut T`. Going the other way — pointer back to reference — is the dangerous direction and must be done in `unsafe` via `*` or the helper `as_ref()`/`as_mut()` (shown below).

---

## Key Differences

| Aspect | Reference `&T` / `&mut T` | Raw pointer `*const T` / `*mut T` | TypeScript/JavaScript analogue |
| --- | --- | --- | --- |
| Created in safe code | Yes | Yes | n/a (no pointers) |
| Dereferenced in safe code | Yes | **No** — requires `unsafe` | `view[i]` always allowed |
| Guaranteed non-null | Yes | No (can be `null`) | views are never "null" |
| Guaranteed aligned & valid | Yes (borrow checker + lifetimes) | No — your responsibility | runtime-checked bounds |
| Aliasing rules enforced | Yes (one `&mut` xor many `&`) | No — many `*mut` allowed at once | aliasing views allowed |
| Has a lifetime | Yes | **No** — can dangle freely | GC prevents dangling |
| Can do pointer arithmetic | No | Yes (`.add`, `.offset`) | index arithmetic on views |
| Auto-frees / tracked by compiler | Borrow-checked | No | GC-managed |

The single most important row is **aliasing**. The borrow checker's entire job is to prevent you from having a `&mut T` at the same time as any other reference to the same data. Raw pointers opt out of that check, which is the whole point of using them, but also why every dereference must be justified by hand.

### Thin pointers vs. fat pointers

A pointer to a `Sized` type is one machine word. A pointer to an **unsized** type — a slice `[T]` or a `str` — is a **fat pointer**: it carries the address *plus* the length.

```rust
use std::mem::size_of;

fn main() {
    println!("*const u8     = {} bytes", size_of::<*const u8>());
    println!("*const [u8]   = {} bytes", size_of::<*const [u8]>());
    println!("*const str    = {} bytes", size_of::<*const str>());
    println!("&u8           = {} bytes", size_of::<&u8>());
}
```

Real output on a 64-bit target:

```text
*const u8     = 8 bytes
*const [u8]   = 16 bytes
*const str    = 16 bytes
&u8           = 8 bytes
```

This matters when you reach for `std::slice::from_raw_parts(ptr, len)`: you supply the thin data pointer *and* the length separately, and Rust assembles the fat pointer for you.

---

## Common Pitfalls

### Pitfall 1: Forgetting that dereference needs `unsafe`

```rust
fn main() {
    let x = 5;
    let p: *const i32 = &x;
    println!("{}", *p); // does not compile (error[E0133])
}
```

Real compiler output:

```text
error[E0133]: dereference of raw pointer is unsafe and requires unsafe block
 --> src/main.rs:4:20
  |
4 |     println!("{}", *p);
  |                    ^^ dereference of raw pointer
  |
  = note: raw pointers may be null, dangling or unaligned; they can violate
    aliasing rules and cause data races: all of these are undefined behavior
```

The fix is to wrap the dereference in `unsafe { *p }`, but only after you have convinced yourself the four invariants hold.

### Pitfall 2: Creating a dangling pointer to a dropped value

This one is insidious because **it compiles cleanly with no warning**:

```rust
fn main() {
    let dangling: *const i32 = {
        let temp = 10;
        &temp as *const i32 // `temp` is dropped at the end of this block
    };
    // `dangling` now points at freed stack memory.
    unsafe {
        println!("{}", *dangling); // undefined behavior: use-after-free
    }
}
```

Unlike a reference — where the borrow checker would reject returning `&temp` with a "borrowed value does not live long enough" error — a raw pointer has **no lifetime**, so nothing stops you from outliving the data. The dereference is UB. The program may print `10`, print garbage, or crash; "it printed the right number on my machine" proves nothing. This is the danger references were designed to eliminate.

### Pitfall 3: Casting away `const` to mutate read-only data

Casting `*const T → *mut T` with `as` is allowed and silent, but **writing** through a `*mut` derived from data that was never mutable (for example, a value behind a shared `&T`, or a `static` without interior mutability) is undefined behavior. The `const` in `*const` is more than decoration; it reflects the provenance of the data. Never cast away `const` to obtain write access to something you do not actually own mutably.

### Pitfall 4: Assuming `*mut` re-enables aliasing safely

Raw pointers let you *create* two `&mut` to the same place — the borrow checker that rejects this for references…

```rust
fn main() {
    let mut x = 10;
    let r1 = &mut x;
    let r2 = &mut x; // does not compile (error[E0499])
    *r1 += 1;
    *r2 += 1;
}
```

```text
error[E0499]: cannot borrow `x` as mutable more than once at a time
 --> src/main.rs:4:14
  |
3 |     let r1 = &mut x;
  |              ------ first mutable borrow occurs here
4 |     let r2 = &mut x;
  |              ^^^^^^ second mutable borrow occurs here
5 |     *r1 += 1;
  |     -------- first borrow later used here
```

…is *bypassed* by going through `*mut x`. But the underlying rule still exists at the level of UB: producing two `&mut` that **alias** (point at the same `T` and are both used) is undefined behavior even when the borrow checker did not see it. Raw pointers let you split a buffer into *disjoint* `&mut` regions (which is sound, as in the next section). They do **not** let you legally alias overlapping `&mut`.

### Pitfall 5: Confusing `null()` with `Option::None`

A `*const T` can be `std::ptr::null()`, which is *not* `None`. It is a real pointer value whose address is 0. Always check `ptr.is_null()` (or use `as_ref()`, below) before dereferencing a pointer that might be null, especially one that came from C.

---

## Best Practices

- **Prefer references.** Reach for raw pointers only for FFI, for fundamentally unsafe data structures, or inside a safe abstraction. If a `&T` / `&mut T` works, use it.
- **Keep `unsafe` blocks tiny.** Wrap only the dereference, not the surrounding logic, and write a `// SAFETY:` comment explaining why each invariant holds. Clippy's `undocumented_unsafe_blocks` lint can enforce this.
- **Use `&raw const` / `&raw mut` for fields, not `&x as *const _`.** Stabilized in Rust 1.82, these operators produce a raw pointer *without ever forming a reference*, essential for unaligned fields of a `#[repr(packed)]` struct, where forming a normal reference would itself be UB:

  ```rust
  use std::mem::size_of;

  #[repr(C, packed)]
  struct Packed {
      a: u8,
      b: u32, // unaligned: it sits at byte offset 1
  }

  fn main() {
      let p = Packed { a: 7, b: 0xABCD };
      // `&raw const p.b` makes a *const u32 WITHOUT a temporary &u32.
      let a = unsafe { (&raw const p.a).read_unaligned() };
      let b = unsafe { (&raw const p.b).read_unaligned() };
      println!("a={a}, b={b:#X}");
      let _ = size_of::<Packed>();
  }
  ```

  Real output:

  ```text
  a=7, b=0xABCD
  ```

- **Convert to a reference with `as_ref()` / `as_mut()` for the null check.** These return `Option<&T>`, doing the null check for you (you still owe the validity/aliasing invariants):

  ```rust
  use std::ptr;

  fn main() {
      let value = 7i32;
      let p: *const i32 = &value;
      let null: *const i32 = ptr::null();
      // SAFETY: `p` points to a live, aligned i32; `null` is checked for us.
      unsafe {
          println!("p.as_ref()    = {:?}", p.as_ref()); // Some(7)
          println!("null.as_ref() = {:?}", null.as_ref()); // None
      }
  }
  ```

  Real output:

  ```text
  p.as_ref()    = Some(7)
  null.as_ref() = None
  ```

- **Use the typed helpers** `ptr::read`, `ptr::write`, `read_unaligned`, `write_unaligned`, and `ptr::copy_nonoverlapping` instead of hand-rolling `*p = ...` when you need move/bitwise semantics, an unaligned access, or `memcpy`-style copies.
- **Document and contain.** Bury raw pointers behind a small, safe API and verify the unsafe core under [Miri](/21-performance/) (run with `cargo +nightly miri test`), which detects many UB conditions a normal run silently ignores.

---

## Real-World Example

A common production scenario: a buffer of bytes is handed to you across an FFI boundary (a pointer + length from C, see [Calling C from Rust](/20-unsafe-ffi/04-calling-c/)), and you want to read it as a Rust slice without copying. `std::slice::from_raw_parts` builds a fat slice pointer from a thin data pointer and a length. The pattern is to take the raw pointer in an `unsafe fn` with a documented contract, then expose a safe API.

```rust
use std::slice;

/// A read-only view over a buffer we do NOT own — for example, memory
/// handed to us across an FFI boundary. The caller guarantees the pointer
/// stays valid for the lifetime `'a`.
struct BufferView<'a> {
    data: &'a [u8],
}

impl<'a> BufferView<'a> {
    /// # Safety
    /// `ptr` must be non-null, aligned, and point to `len` initialized
    /// `u8` values that stay valid and immutable for all of `'a`.
    unsafe fn from_raw(ptr: *const u8, len: usize) -> Self {
        // SAFETY: upheld by the caller's contract documented above.
        let data = unsafe { slice::from_raw_parts(ptr, len) };
        BufferView { data }
    }

    fn checksum(&self) -> u32 {
        self.data.iter().map(|&b| b as u32).sum()
    }
}

fn main() {
    // In real code this buffer would come from C. Here we own it, so we
    // can prove the safety contract is satisfied at the call site.
    let owned: Vec<u8> = vec![1, 2, 3, 4, 5];
    let view = unsafe { BufferView::from_raw(owned.as_ptr(), owned.len()) };
    println!("checksum = {}", view.checksum());
    println!("first byte = {}", view.data[0]);
}
```

Real output:

```text
checksum = 15
first byte = 1
```

The raw pointer and the single `unsafe` block are confined to `from_raw`; everything else — `checksum`, indexing, the lifetime tie via `'a` — is ordinary safe Rust. That "unsafe core, safe shell" discipline is the subject of [Building Safe Abstractions](/20-unsafe-ffi/08-safety-abstractions/).

A second classic example is splitting one mutable slice into two non-overlapping halves. The borrow checker cannot prove the halves are disjoint, so the standard library's `split_at_mut` does it with raw pointers, and you can write it yourself:

```rust
/// Split a slice into two disjoint mutable halves with raw pointers.
/// This is essentially how the standard library's `split_at_mut` works.
fn split_at_mut(slice: &mut [i32], mid: usize) -> (&mut [i32], &mut [i32]) {
    let len = slice.len();
    let ptr = slice.as_mut_ptr();
    assert!(mid <= len, "mid out of bounds");

    // SAFETY: `mid <= len`, so both ranges lie inside the original
    // allocation and do not overlap, so the two `&mut` slices never alias.
    unsafe {
        (
            std::slice::from_raw_parts_mut(ptr, mid),
            std::slice::from_raw_parts_mut(ptr.add(mid), len - mid),
        )
    }
}

fn main() {
    let mut v = vec![1, 2, 3, 4, 5, 6];
    let (left, right) = split_at_mut(&mut v, 3);
    left[0] = 100;
    right[0] = 200;
    println!("{v:?}");
}
```

Real output:

```text
[100, 2, 3, 200, 5, 6]
```

---

## Further Reading

- [The Rust Reference — Pointer types](https://doc.rust-lang.org/reference/types/pointer.html): the formal definition of `*const T` and `*mut T`.
- [`std::ptr` module documentation](https://doc.rust-lang.org/std/ptr/index.html): `null`, `read`, `write`, `copy_nonoverlapping`, and the safety contracts.
- [Pointer method docs (`*const T`)](https://doc.rust-lang.org/std/primitive.pointer.html): `add`, `offset`, `as_ref`, `is_null`, `read_unaligned`, and friends.
- [The Rustonomicon — Working with Unsafe](https://doc.rust-lang.org/nomicon/): the deep guide to unsafe code and pointer provenance.
- Within this guide:
  - [What `unsafe` Really Means](/20-unsafe-ffi/00-unsafe-intro/): the five superpowers, invariants, and undefined behavior.
  - [Unsafe Rust](/20-unsafe-ffi/01-unsafe-rust/): `unsafe` blocks, dereferencing operations, calling unsafe functions, `static mut`.
  - [FFI Basics](/20-unsafe-ffi/03-ffi-basics/) and [Calling C from Rust](/20-unsafe-ffi/04-calling-c/): where raw pointers cross language boundaries.
  - [Building Safe Abstractions](/20-unsafe-ffi/08-safety-abstractions/): wrapping unsafe pointer work in a safe API.
  - [When to Use Unsafe and FFI](/20-unsafe-ffi/09-when-to-use/) — and the many times you should not.
  - [Borrowing and References](/05-ownership/02-borrowing/) and [Mutable References](/05-ownership/03-mutable-references/): the checked alternative raw pointers opt out of.
  - [Smart Pointers: `Box<T>`](/10-smart-pointers/00-box/): owning heap pointers built on raw allocation.
  - [Performance](/21-performance/): including verifying unsafe code with Miri.

---

## Exercises

### Exercise 1: Swap with raw pointers

**Difficulty:** Beginner

**Objective:** Understand `*mut T` dereferencing and the move-by-bits helpers.

**Instructions:** Implement a generic `swap<T>(a: &mut T, b: &mut T)` that exchanges the two values using only raw pointers and `std::ptr::read` / `std::ptr::write` (do not call `std::mem::swap`). Test it on two `String`s. Explain in a comment why `ptr::read`/`ptr::write` are needed rather than plain assignment.

<details>
<summary>Solution</summary>

```rust
use std::ptr;

fn swap<T>(a: &mut T, b: &mut T) {
    let pa: *mut T = a;
    let pb: *mut T = b;
    // SAFETY: `a` and `b` are valid, aligned, and (being distinct &mut)
    // do not alias. `read`/`write` move the bits without running Drop on
    // a slot twice — plain `*pa = *pb` would try to move out of `*pb` and
    // drop the old `*pa`, which the type system forbids for non-Copy T.
    unsafe {
        let tmp = ptr::read(pa);
        ptr::write(pa, ptr::read(pb));
        ptr::write(pb, tmp);
    }
}

fn main() {
    let mut a = String::from("hello");
    let mut b = String::from("world");
    swap(&mut a, &mut b);
    println!("a={a}, b={b}"); // a=world, b=hello
}
```

Real output:

```text
a=world, b=hello
```

</details>

### Exercise 2: Sum bytes through a raw pointer + length

**Difficulty:** Intermediate

**Objective:** Build a fat slice from a thin pointer and length, and check for null.

**Instructions:** Write `unsafe fn sum_bytes(ptr: *const u8, len: usize) -> u64` that returns `0` if `ptr` is null, otherwise sums the `len` bytes it points to (use `std::slice::from_raw_parts`). Document the safety contract with a `# Safety` doc comment. Call it on a `[u8; 5]` and confirm the total.

<details>
<summary>Solution</summary>

```rust
use std::slice;

/// # Safety
/// If `ptr` is non-null it must be aligned and point to `len` initialized
/// `u8` values that remain valid for the duration of this call.
unsafe fn sum_bytes(ptr: *const u8, len: usize) -> u64 {
    if ptr.is_null() {
        return 0;
    }
    // SAFETY: non-null was checked; the rest is the caller's contract.
    let bytes = unsafe { slice::from_raw_parts(ptr, len) };
    bytes.iter().map(|&b| b as u64).sum()
}

fn main() {
    let buf = [1u8, 2, 3, 4, 5];
    let total = unsafe { sum_bytes(buf.as_ptr(), buf.len()) };
    println!("sum={total}"); // sum=15

    let empty = unsafe { sum_bytes(std::ptr::null(), 5) };
    println!("null sum={empty}"); // null sum=0
}
```

Real output:

```text
sum=15
null sum=0
```

</details>

### Exercise 3: Two disjoint mutable elements from one slice

**Difficulty:** Advanced

**Objective:** Use raw pointers to produce two `&mut T` the borrow checker would otherwise reject, while keeping the result sound.

**Instructions:** Implement `get_two_mut<T>(s: &mut [T], i: usize, j: usize) -> (&mut T, &mut T)` returning mutable references to two *distinct* elements. Assert that `i != j` and both are in bounds, then use `as_mut_ptr` and `.add()`. Explain in your `// SAFETY` comment why the two references do not alias. Test by incrementing both elements of a `vec![10, 20, 30, 40]`.

<details>
<summary>Solution</summary>

```rust
fn get_two_mut<T>(s: &mut [T], i: usize, j: usize) -> (&mut T, &mut T) {
    assert!(i != j, "indices must differ");
    assert!(i < s.len() && j < s.len(), "index out of bounds");
    let ptr = s.as_mut_ptr();
    // SAFETY: `i != j` and both are in bounds, so the two pointers address
    // distinct elements inside the same allocation. Distinct elements do
    // not overlap, so the two `&mut` we hand out never alias.
    unsafe { (&mut *ptr.add(i), &mut *ptr.add(j)) }
}

fn main() {
    let mut v = vec![10, 20, 30, 40];
    let (x, y) = get_two_mut(&mut v, 0, 3);
    *x += 1;
    *y += 1;
    println!("v={v:?}"); // v=[11, 20, 30, 41]
}
```

Real output:

```text
v=[11, 20, 30, 41]
```

> **Tip:** The standard library already provides this safely as [`<[T]>::get_disjoint_mut`](https://doc.rust-lang.org/std/primitive.slice.html#method.get_disjoint_mut) (stabilized in Rust 1.86). Reaching for it in real code is better than rolling your own unsafe version. This exercise exists to show you what the safe wrapper does underneath.

</details>
