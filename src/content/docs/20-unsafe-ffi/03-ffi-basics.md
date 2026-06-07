---
title: "FFI Basics: `extern \"C\"`, `#[no_mangle]`, `#[repr(C)]`, and the C ABI"
description: "Export Rust over the stable C ABI with extern \"C\", #[no_mangle], and #[repr(C)], so Node addons, Python, or any C-speaking caller can link to your functions."
---

The **Foreign Function Interface** (**FFI**) is how Rust talks to code written in other languages, and how other languages call into Rust. The lingua franca of that conversation is the **C ABI**: a stable, language-neutral calling convention that almost every toolchain on the planet understands. This page covers the four building blocks for *exporting* Rust to that world: `extern "C"`, `#[no_mangle]`, `#[repr(C)]`, and how the result actually gets linked.

---

## Quick Overview

For a TypeScript/JavaScript developer, the closest thing you already know is a **Node.js native addon**: a compiled `.node` file (originally C++) that `require()` loads and calls like any other module. Under the hood, that addon and Node agree on a binary contract: how arguments are passed, how the function is named in the compiled object file, and how structs are laid out in memory. That contract is the **Application Binary Interface (ABI)**, and the most widely-supported one is the **C ABI**.

Rust does not use the C ABI by default. Its own ABI is **unstable and unspecified** (the compiler is free to reorder struct fields, change calling conventions between releases, and mangle function names). To make a Rust function callable from C (or from Python's `ctypes`, Node's FFI bindings, Go's cgo, a game engine, or the OS dynamic linker) you opt in explicitly with three annotations:

- **`extern "C"`**: use C's *calling convention* (how args/return values move through registers and the stack).
- **`#[unsafe(no_mangle)]`**: keep the function's *symbol name* exactly as written, so the linker can find it.
- **`#[repr(C)]`** — give a struct or enum C's *memory layout*, so both sides agree on where each field lives.

> **Note:** This page is about FFI *fundamentals* and the *export* direction (Rust as the library). Calling C *from* Rust — `build.rs`, the `cc` crate, `CString`/`CStr` — is covered in [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/). Auto-generating bindings from a C header is in [Generating Bindings with bindgen](/20-unsafe-ffi/05-bindgen/). The raw-pointer mechanics these examples rely on are in [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/), and the meaning of the `unsafe` keyword itself is in [What `unsafe` Really Means (and What It Does Not)](/20-unsafe-ffi/00-unsafe-intro/).

---

## TypeScript/JavaScript Example

JavaScript has no native FFI of its own. When a Node.js project needs to reach into compiled native code — say, to call into a fast image codec or a system library — it loads a **native addon** built against Node's **N-API**. The JavaScript you write looks innocent:

```typescript
// app.ts — using a precompiled native addon
// The .node file is a compiled dynamic library that exports C-compatible symbols.
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);

// Node's loader (process.dlopen) opens the shared library and wires up its exports.
const native = require("./build/Release/imageproc.node") as {
  grayscale(buffer: Buffer): Buffer;
};

const input: Buffer = Buffer.from([135, 206, 235, 255]); // one RGBA pixel
const output = native.grayscale(input);
console.log(output); // <Buffer bc bc bc ff>
```

A few things are quietly true here that you may never have had to think about:

- `require("...node")` ultimately calls `process.dlopen`, the same OS facility C programs use to load a `.so`/`.dylib`/`.dll`. Node v22 has **no built-in FFI module**: there is no `require("node:ffi")`; the boundary always goes through a compiled addon.
- The addon and Node agree on a binary layout for every value crossing the boundary. The JS `number` you pass becomes a C `double`; a `Buffer` becomes a pointer plus a length.
- If the addon were compiled with a function named differently than what Node expects, the load would fail with an "undefined symbol" error: a *linker* problem, not a JavaScript one.

That binary contract is exactly what Rust's FFI annotations let you produce, *without* writing any C++.

---

## Rust Equivalent

Here is a small but realistic Rust library that exports functions over the C ABI: a color utility that any C-speaking caller — including a Node addon, a Python script, or a C program — can link against.

```rust
// src/lib.rs
use std::os::raw::c_double;

// `#[repr(C)]` gives this struct the SAME memory layout a C compiler would use:
// fields in declaration order, with C's alignment and padding rules. Without it,
// Rust may reorder the fields, so C code reading this struct would see garbage.
#[repr(C)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

// `extern "C"` = use the C calling convention.
// `#[unsafe(no_mangle)]` = export the symbol under the literal name `color_luminance`,
// so the linker (and the foreign caller) can find it.
#[unsafe(no_mangle)]
pub extern "C" fn color_luminance(c: Color) -> c_double {
    // Rec. 601 luminance weights.
    0.299 * c.r as f64 + 0.587 * c.g as f64 + 0.114 * c.b as f64
}

#[unsafe(no_mangle)]
pub extern "C" fn color_opaque(c: Color) -> bool {
    c.a == 255
}
```

To produce a shared library a C program can link against, declare the crate's output type in `Cargo.toml`:

```toml
# Cargo.toml
[package]
name = "colorlib"
version = "0.1.0"
edition = "2024"

[lib]
# cdylib  -> a C-compatible dynamic library (.so / .dylib / .dll) for runtime loading
# staticlib -> a C-compatible static archive (.a / .lib) for compile-time linking
# rlib    -> the normal Rust library, so other Rust crates can still use this one
crate-type = ["cdylib", "staticlib", "rlib"]
```

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically. Two edition-2024 details matter for FFI: `#[no_mangle]` must now be written `#[unsafe(no_mangle)]`, and `extern` blocks must be written `unsafe extern`. Both are explained below.

Building it produces a real, loadable library, and the C-ABI symbols are visible in it:

```text
$ cargo build --release
   Compiling colorlib v0.1.0 (/tmp/colorlib)
    Finished `release` profile [optimized] target(s) in 0.62s

$ nm -gU target/release/libcolorlib.dylib | grep color_
0000000000000358 T _color_luminance
00000000000003c4 T _color_opaque
```

> **Tip:** `nm` lists the symbols in a compiled object. The `T` means "defined in the text (code) section." On macOS, C symbols carry a leading underscore (`_color_luminance`); on Linux there is no underscore. The point is that the names survive *exactly* as you wrote them; that is what `#[unsafe(no_mangle)]` buys you.

And it really works across the language boundary. Here is a C program calling straight into the Rust library:

```c
/* main.c */
#include <stdio.h>
#include <stdint.h>
#include <stdbool.h>

/* Mirror the #[repr(C)] struct on the C side, field-for-field. */
typedef struct { uint8_t r, g, b, a; } Color;

/* Declare the Rust functions with their C ABI signatures. */
extern double color_luminance(Color c);
extern bool   color_opaque(Color c);

int main(void) {
    Color sky = { 135, 206, 235, 255 };
    printf("luminance = %.3f\n", color_luminance(sky));
    printf("opaque    = %s\n", color_opaque(sky) ? "true" : "false");
    return 0;
}
```

```text
$ cc main.c -L target/release -lcolorlib -o demo
$ DYLD_LIBRARY_PATH=target/release ./demo
luminance = 188.077
opaque    = true
```

The C compiler had no idea Rust was involved. It saw two functions with a C calling convention, a struct with a C layout, and stable symbol names. That is the whole trick.

---

## Detailed Explanation

Three orthogonal things must agree across an FFI boundary, and each annotation fixes exactly one of them.

### 1. The calling convention — `extern "C"`

When you call a function, the compiler decides *how* the arguments travel: which CPU registers, what order, how the return value comes back, who cleans up the stack. That set of rules is the **calling convention**, and it is part of the ABI.

Rust's default `extern "Rust"` convention is **deliberately unstable**: the compiler may change it between releases to enable optimizations. C's convention has been stable for decades and is what every other language's FFI assumes. Writing `extern "C"` on a function (or function pointer) tells Rust: "use C's rules here."

```rust
// Default Rust ABI: not callable from C, may change between compiler versions.
fn rust_add(a: i32, b: i32) -> i32 { a + b }

// C ABI: stable, language-neutral.
extern "C" fn c_add(a: i32, b: i32) -> i32 { a + b }
```

The string is the ABI name. `"C"` is the portable choice. (Others exist — `"system"`, `"stdcall"` on 32-bit Windows, and `"C-unwind"` for boundaries that intentionally propagate panics/exceptions — but `"C"` is the one you reach for first.)

> **Warning:** A plain `extern "C"` boundary must **never** let a Rust panic unwind across it. Unwinding into C is undefined behavior. Catch panics at the boundary (shown in the Real-World Example) or use the `"C-unwind"` ABI when you specifically *want* unwinding to cross.

### 2. The symbol name — `#[unsafe(no_mangle)]`

Rust **mangles** symbol names by default: it encodes the module path, generics, and a hash into the compiled name so that two functions named `parse` in different modules don't collide. Great for Rust; useless for a foreign linker that is looking for a function literally called `color_luminance`.

`#[no_mangle]` (written `#[unsafe(no_mangle)]` in edition 2024) disables mangling for that item, so the symbol name in the object file equals the source name. The contrast is stark: a mangled internal function isn't even exported from a `cdylib`, while the `no_mangle` one is:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn unmangled_add(a: i32, b: i32) -> i32 {
    a + b
}

// No #[no_mangle]: the symbol is mangled (and not a C-ABI export).
pub fn rust_internal_add(a: i32, b: i32) -> i32 {
    a + b
}
```

```text
$ nm -gU target/release/libcolorlib.dylib | grep -i add
0000000000000358 T _unmangled_add
```

Only `_unmangled_add` appears. The mangled `rust_internal_add` is invisible to a C linker.

> **Note:** Why is it spelled `unsafe(no_mangle)`? Because forcing a specific symbol name can cause **name collisions** at link time: if two `no_mangle` symbols share a name, you get undefined behavior, not a friendly error. Edition 2024 makes you acknowledge that risk with the `unsafe(...)` wrapper. This is purely an *attribute*; it does not introduce an `unsafe {}` block. See [What `unsafe` Really Means (and What It Does Not)](/20-unsafe-ffi/00-unsafe-intro/) for what `unsafe` does and does not mean.

### 3. The data layout — `#[repr(C)]`

This is the one that bites people. Rust's default struct layout (`repr(Rust)`) is **unspecified**: the compiler reorders fields to minimize padding and may change the layout at any time. C's layout is fixed: fields in declaration order, padded to each field's alignment. If you pass a default-layout struct to C, the two sides disagree about where each field lives.

The difference is observable. These two structs have identical fields:

```rust
struct DefaultLayout {
    flag: bool, // 1 byte
    id: u64,    // 8 bytes
    code: u16,  // 2 bytes
}

#[repr(C)]
struct CLayout {
    flag: bool,
    id: u64,
    code: u16,
}

fn main() {
    use std::mem::{size_of, align_of, offset_of};
    println!("DefaultLayout: size={}, align={}", size_of::<DefaultLayout>(), align_of::<DefaultLayout>());
    println!("CLayout:       size={}, align={}", size_of::<CLayout>(), align_of::<CLayout>());
    println!("Default offsets: flag={}, id={}, code={}",
        offset_of!(DefaultLayout, flag), offset_of!(DefaultLayout, id), offset_of!(DefaultLayout, code));
    println!("C offsets:       flag={}, id={}, code={}",
        offset_of!(CLayout, flag), offset_of!(CLayout, id), offset_of!(CLayout, code));
}
```

```text
DefaultLayout: size=16, align=8
CLayout:       size=24, align=8
Default offsets: flag=10, id=0, code=8
C offsets:       flag=0, id=8, code=16
```

Read those offsets carefully. In the default layout, Rust put `id` *first* (offset 0) and `flag` *last* (offset 10) to pack the struct into 16 bytes. A C program expecting `flag` at offset 0 would read the wrong bytes. With `#[repr(C)]`, `flag` is at offset 0 and the layout is the (larger, 24-byte) one C produces, exactly what the C side expects. **The C-compatible layout is sometimes *less* efficient; that is the price of a stable, shared layout.**

`#[repr(C)]` applies to enums too. A field-less enum becomes a plain C integer, and you can pin the size with `#[repr(i32)]`, `#[repr(u8)]`, etc.:

```rust
#[repr(C)]
pub enum Status {
    Ok = 0,
    InvalidInput = 1,
    Overflow = 2,
}
```

There is also **`#[repr(transparent)]`** for single-field newtypes: it guarantees the wrapper has the *exact* same ABI as its one field, which is how you build type-safe handles that cross FFI for free:

```rust
#[repr(transparent)]
struct Handle(u64); // ABI-identical to a bare u64, but a distinct Rust type
```

### Linking: how the pieces actually join

`crate-type` in `Cargo.toml` decides what artifact Cargo emits:

| `crate-type`  | Produces (macOS / Linux / Windows) | When to use it                                          |
| ------------- | ---------------------------------- | ------------------------------------------------------- |
| `cdylib`      | `.dylib` / `.so` / `.dll`          | Runtime loading: Node addons, Python `ctypes`, plugins  |
| `staticlib`   | `.a` / `.a` / `.lib`               | Compile-time linking into a larger C/C++ binary         |
| `rlib`        | `.rlib`                            | Normal Rust-to-Rust dependency (the default)            |

A `cdylib` is loaded at runtime by the dynamic linker (`dlopen` / `LoadLibrary`, the same `process.dlopen` Node uses for addons). A `staticlib` is baked into another program at build time. You can request several at once, which is why the example above lists `["cdylib", "staticlib", "rlib"]`.

---

## Key Differences

| Concept                  | TypeScript / JavaScript                                   | Rust                                                                 |
| ------------------------ | --------------------------------------------------------- | -------------------------------------------------------------------- |
| Built-in FFI             | None; goes through a compiled N-API addon                 | First-class: `extern "C"`, `#[repr(C)]`, no separate runtime needed  |
| Default ABI              | N/A (engine-internal)                                     | `extern "Rust"` — **unstable**, must opt into `"C"` for FFI          |
| Symbol names             | Managed by the addon's C++ build                          | Mangled by default; `#[unsafe(no_mangle)]` to keep the literal name  |
| Struct memory layout     | Engine-defined object shapes (hidden classes)             | `repr(Rust)` reorders fields; `#[repr(C)]` for a fixed C layout      |
| Numbers crossing over    | `number` (always f64) marshaled by the addon              | Pick the exact C type: `c_int`, `c_double`, `u8`, etc.               |
| Memory ownership         | GC owns everything; addon must be careful                 | Explicit: you decide who frees what across the boundary              |
| Error signaling          | Throw / reject                                            | Return codes / out-params; **panics must not cross** a `"C"` boundary |

The deepest difference is **ownership and safety**. In Node, the garbage collector and the addon coordinate to keep memory alive. In Rust FFI, *you* are the contract: the borrow checker stops at the boundary, so passing a pointer to C means manually guaranteeing it stays valid and is freed exactly once. The C ABI is fast precisely because it does none of this for you. The discipline of wrapping that rawness in a safe API is covered in [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).

---

## Common Pitfalls

### Pitfall 1: Forgetting `unsafe(...)` on the attribute (edition 2024)

If you copy a pre-2024 example, you'll write the bare attribute and hit a hard error:

```rust
#[no_mangle] // does not compile under edition 2024
pub extern "C" fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

The real compiler output:

```text
error: unsafe attribute used without unsafe
 --> src/main.rs:1:3
  |
1 | #[no_mangle]
  |   ^^^^^^^^^ usage of unsafe attribute
  |
help: wrap the attribute in `unsafe(...)`
  |
1 | #[unsafe(no_mangle)]
  |   +++++++         +
```

The fix is exactly what the compiler suggests: `#[unsafe(no_mangle)]`.

### Pitfall 2: Forgetting `unsafe` on an `extern` block (edition 2024)

The same edition-2024 change applies when you *declare* foreign functions:

```rust
extern "C" { // does not compile under edition 2024
    fn abs(input: i32) -> i32;
}
fn main() {
    let n = unsafe { abs(-42) };
    println!("{n}");
}
```

```text
error: extern blocks must be unsafe
 --> src/main.rs:1:1
  |
1 | / extern "C" {
2 | |     fn abs(input: i32) -> i32;
3 | | }
  | |_^
```

The fix is `unsafe extern "C" { ... }`. (The *consuming* side — declaring and calling C functions — is the subject of [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/); the syntax is shown here only so you recognize the error.)

### Pitfall 3: Passing a default-layout struct to C

Omitting `#[repr(C)]` compiles fine — Rust has no idea the struct will leave the language — but the C side reads scrambled fields, as the offset experiment above showed. There is no compiler error; you get silently wrong data or a crash. **Any type that crosses an FFI boundary needs an explicit `repr`.**

### Pitfall 4: Letting a panic unwind into C

A `panic!` (or an arithmetic overflow in debug, an `unwrap` on `None`, an out-of-bounds index) inside an `extern "C"` function that unwinds across the boundary is undefined behavior. It will not look like a JavaScript exception bubbling up; it can corrupt the C caller's stack. Catch it with `std::panic::catch_unwind` and return an error code instead (next section).

### Pitfall 5: Expecting the borrow checker to follow your pointer into C

Once you hand C a `*const T` or `*mut T`, Rust's lifetime analysis is over. Nothing stops C from holding the pointer after the Rust value is dropped (a use-after-free) or freeing it twice. The C ABI carries no ownership information. Designing the ownership contract is on you; see [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/) and [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).

---

## Best Practices

- **Annotate every exported item with all three:** `#[unsafe(no_mangle)] pub extern "C" fn` for functions, `#[repr(C)]` for any struct/enum that crosses over. Missing one is a silent layout or linkage bug.
- **Use the `c_*` type aliases** from `std::os::raw` (or the re-exports in `std::ffi`) — `c_int`, `c_uint`, `c_double`, `c_char` — instead of guessing that `int` is `i32`. They track the target platform's real C types.
- **Catch panics at the boundary.** Wrap the body of each `extern "C"` function in `std::panic::catch_unwind` and translate failures into return codes, or compile with `panic = "abort"` so a panic terminates cleanly instead of unwinding.
- **Prefer return codes and out-parameters over rich return types.** The C ABI can't carry a Rust `Result` or `Option`; encode success/failure as a `#[repr(C)]` enum or an integer and write results through `*mut T` out-params.
- **Keep the FFI surface tiny and `unsafe`-free for callers.** Expose a minimal set of C-ABI functions, and build the ergonomic, safe Rust API in a separate layer (the unsafe-inside / safe-outside pattern in [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/)).
- **Name exported symbols defensively.** Because `no_mangle` symbols can collide globally, prefix them (`colorlib_luminance`, not `luminance`) the way C libraries do.
- **Verify your exports.** A quick `nm -gU <lib>` (Unix) or `dumpbin /exports` (Windows) confirms the symbols you expect are present and unmangled before you ship.

---

## Real-World Example

A production C-ABI library wraps every entry point so that errors come back as status codes, results travel through out-parameters, and **no panic can ever escape into the caller**. Here is a small numeric utility that does all three:

```rust
// src/lib.rs
use std::os::raw::c_int;
use std::panic;

// A C-compatible status enum, laid out as a plain integer.
#[repr(C)]
pub enum Status {
    Ok = 0,
    Overflow = 1,
    Panicked = 2,
}

// Add two integers with overflow checking. The result is written through `out`;
// the return value reports success or failure. This is the classic C pattern
// (return code + out-param) because the C ABI can't carry a Rust `Result`.
#[unsafe(no_mangle)]
pub extern "C" fn checked_add(a: c_int, b: c_int, out: *mut c_int) -> Status {
    // catch_unwind stops any panic from unwinding into the C caller (which is UB).
    let result = panic::catch_unwind(|| {
        a.checked_add(b).ok_or(Status::Overflow)
    });

    match result {
        Ok(Ok(sum)) => {
            // SAFETY: the caller contract is that `out` is a valid, writable pointer.
            unsafe { *out = sum; }
            Status::Ok
        }
        Ok(Err(status)) => status, // arithmetic overflow
        Err(_) => Status::Panicked, // a panic was caught at the boundary
    }
}
```

```toml
# Cargo.toml
[package]
name = "mathlib"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]
```

The C consumer mirrors the enum and passes the address of a local for the result:

```c
/* main.c */
#include <stdio.h>

typedef enum { OK = 0, OVERFLOW = 1, PANICKED = 2 } Status;
extern Status checked_add(int a, int b, int *out);

int main(void) {
    int out = 0;
    Status s = checked_add(2000000000, 2000000000, &out); /* overflows int */
    printf("status=%d out=%d\n", s, out);

    s = checked_add(20, 22, &out);
    printf("status=%d out=%d\n", s, out);
    return 0;
}
```

Building and running shows the contract honored in both directions:

```text
$ cargo build --release
    Finished `release` profile [optimized] target(s) in 0.18s
$ cc main.c -L target/release -lmathlib -o demo
$ DYLD_LIBRARY_PATH=target/release ./demo
status=1 out=0
status=0 out=42
```

The first call adds `2_000_000_000 + 2_000_000_000`, which overflows a 32-bit `int`, so `checked_add` returns `Status::Overflow` (status `1`) and leaves `out` untouched. The second call succeeds with status `0` and `out = 42`. Neither path can crash the C caller: **overflow and panics alike become ordinary status codes the caller can branch on**, which is the entire reason for the `catch_unwind` wrapper and the return-code design.

> **Note:** This same return-code-plus-out-param shape is what binding generators and higher-level wrappers expect. When you later wrap a C library *from* Rust ([Generating Bindings with bindgen](/20-unsafe-ffi/05-bindgen/)) or expose Rust to Node ([Node.js Native Addons with napi-rs](/20-unsafe-ffi/06-napi/)), you'll see the framework hiding exactly this dance behind ergonomic types.

---

## Further Reading

- [The Rustonomicon — FFI](https://doc.rust-lang.org/nomicon/ffi.html): the canonical guide to Rust's foreign function interface.
- [Rust Reference — Type layout & `repr`](https://doc.rust-lang.org/reference/type-layout.html): exactly what `repr(Rust)`, `repr(C)`, and `repr(transparent)` guarantee.
- [Rust Reference — External blocks & ABIs](https://doc.rust-lang.org/reference/items/external-blocks.html): the list of supported ABI strings, including `"C-unwind"`.
- [`std::os::raw`](https://doc.rust-lang.org/std/os/raw/index.html) and [`std::ffi`](https://doc.rust-lang.org/std/ffi/index.html): the C type aliases and string types.
- Sibling pages: [What `unsafe` Really Means (and What It Does Not)](/20-unsafe-ffi/00-unsafe-intro/) · [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/) · [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/) · [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/) · [Generating Bindings with bindgen](/20-unsafe-ffi/05-bindgen/) · [Node.js Native Addons with napi-rs](/20-unsafe-ffi/06-napi/) · [Node.js Native Addons with Neon](/20-unsafe-ffi/07-neon/) · [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/) · [When `unsafe` and FFI Are Actually Necessary (and the Many Times They Are Not)](/20-unsafe-ffi/09-when-to-use/)
- Foundations: [01 — Getting Started](/01-getting-started/) · [02 — Basics](/02-basics/) (the numeric types you'll map to C types).
- Next steps: when FFI is for *speed*, weigh it against pure-Rust optimization in [21 — Performance](/21-performance/).

---

## Exercises

### Exercise 1 — Export your first C-ABI function

**Difficulty:** Beginner

**Objective:** Get the three annotations under your fingers and confirm the symbol is exported.

**Instructions:** Create a library crate. Write an `extern "C"` function `square(n: i32) -> i32` that returns `n * n`, exported with an unmangled symbol. Build it as a `cdylib` and use `nm` (Unix) or `dumpbin /exports` (Windows) to confirm `square` appears in the output.

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
#[unsafe(no_mangle)]
pub extern "C" fn square(n: i32) -> i32 {
    n * n
}
```

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]
```

```text
$ cargo build --release
    Finished `release` profile [optimized] target(s) in 0.14s
$ nm -gU target/release/libsquarelib.dylib | grep square
0000000000000358 T _square
```

The `T _square` line confirms the function is exported under its literal name. Drop the `#[unsafe(no_mangle)]` and rebuild: `square` disappears from the `nm` output, replaced by a mangled symbol.

</details>

### Exercise 2 — A `#[repr(C)]` struct across the boundary

**Difficulty:** Intermediate

**Objective:** Pass a struct by value to a C-ABI function and prove the layout agrees by calling it from C.

**Instructions:** Define a `#[repr(C)]` struct `Point { x: f64, y: f64 }` and an `extern "C"` function `point_distance(a: Point, b: Point) -> f64` returning the Euclidean distance. Write a short C program that declares a matching `struct`, passes `{0,0}` and `{3,4}`, and prints the result (expect `5.0`).

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
use std::os::raw::c_double;

#[repr(C)]
pub struct Point {
    pub x: c_double,
    pub y: c_double,
}

#[unsafe(no_mangle)]
pub extern "C" fn point_distance(a: Point, b: Point) -> c_double {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}
```

```c
/* main.c */
#include <stdio.h>
typedef struct { double x, y; } Point;
extern double point_distance(Point a, Point b);
int main(void) {
    Point a = {0.0, 0.0}, b = {3.0, 4.0};
    printf("distance = %.1f\n", point_distance(a, b));
    return 0;
}
```

```text
$ cargo build --release && cc main.c -L target/release -lgeolib -o demo
$ DYLD_LIBRARY_PATH=target/release ./demo
distance = 5.0
```

Try removing `#[repr(C)]`: for a two-`f64` struct the layout happens to coincide, so it may still work, proving nothing. The danger appears with mixed-size fields (as in the `DefaultLayout`/`CLayout` experiment), which is exactly why you annotate *every* boundary struct rather than relying on luck.

</details>

### Exercise 3 — A safe-from-panic counter with a status enum

**Difficulty:** Advanced

**Objective:** Combine `#[repr(C)]` enums, raw-pointer out-params, and panic safety into one production-shaped function.

**Instructions:** Write `extern "C" fn count_words(s: *const u8, len: usize, out: *mut c_int) -> ParseStatus`. It receives a UTF-8 byte buffer (pointer + length, the way C passes strings), counts whitespace-separated words, writes the count through `out`, and returns a `#[repr(C)]` `ParseStatus` enum: `Ok`, `Empty` (only whitespace), or `NotANumber` (null pointer or invalid UTF-8). Call it from C with `"the quick brown fox"` (expect 4 words) and `"   "` (expect the empty status).

<details>
<summary>Solution</summary>

```rust
// src/lib.rs
use std::os::raw::c_int;

#[repr(C)]
pub enum ParseStatus {
    Ok = 0,
    Empty = 1,
    NotANumber = 2,
}

#[unsafe(no_mangle)]
pub extern "C" fn count_words(s: *const u8, len: usize, out: *mut c_int) -> ParseStatus {
    if s.is_null() || out.is_null() {
        return ParseStatus::NotANumber;
    }
    // SAFETY: the caller guarantees `s` points to `len` valid, initialized bytes.
    let bytes = unsafe { std::slice::from_raw_parts(s, len) };
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => return ParseStatus::NotANumber,
    };
    if text.trim().is_empty() {
        return ParseStatus::Empty;
    }
    let count = text.split_whitespace().count() as c_int;
    // SAFETY: `out` was null-checked above and is the caller's writable slot.
    unsafe { *out = count; }
    ParseStatus::Ok
}
```

```c
/* main.c */
#include <stdio.h>
#include <string.h>
typedef enum { OK = 0, EMPTY = 1, NOT_A_NUMBER = 2 } ParseStatus;
extern ParseStatus count_words(const char *s, size_t len, int *out);
int main(void) {
    const char *sentence = "the quick brown fox";
    int out = 0;
    ParseStatus st = count_words(sentence, strlen(sentence), &out);
    printf("status=%d words=%d\n", st, out);
    st = count_words("   ", 3, &out);
    printf("status=%d\n", st);
    return 0;
}
```

```text
$ cargo build --release && cc main.c -L target/release -ltextlib -o demo
$ DYLD_LIBRARY_PATH=target/release ./demo
status=0 words=4
status=1
```

This single function shows the whole FFI export toolkit at once: a `#[repr(C)]` enum for status, a pointer-plus-length string (the universal C convention), null checks before every dereference, `std::slice::from_raw_parts` to rebuild a safe slice inside one tight `unsafe` block, and graceful handling of invalid input. Wrapping such functions so Rust callers never touch the raw pointers is the topic of [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).

</details>
