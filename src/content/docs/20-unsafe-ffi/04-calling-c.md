---
title: "Calling C Code from Rust"
description: "Call battle-tested C from Rust: declare it in unsafe extern \"C\", compile and link with build.rs and the cc crate, and move strings across with CString and CStr."
---

Decades of battle-tested C exists for things you rarely want to rewrite: codecs, cryptography primitives, compression, database engines, hardware drivers. Rust can call any of it directly, with zero runtime marshaling, because it speaks the same calling convention C does. This page is the practical end-to-end recipe: declare the C functions, compile and link the C source with a `build.rs`, and move strings safely across the boundary with `CStr` and `CString`.

---

## Quick Overview

To call a C function from Rust you do three things: **declare** its signature inside an `unsafe extern "C"` block (Rust trusts your declaration; it cannot check it against the real C code), **build and link** the C object code (either compile C sources at build time with the [`cc`](https://crates.io/crates/cc) crate from a `build.rs` script, or link a system library), and **convert data** at the edges, most importantly turning Rust's UTF-8, length-prefixed `&str` into the NUL-terminated `char*` that C expects using `CString` and `CStr`. For a TypeScript developer the closest analogy is writing a `.d.ts` declaration for a native Node addon and then wiring up the build with `node-gyp`: you describe a function that exists elsewhere, a build step produces the machine code, and a thin layer converts JavaScript values into the shapes the native side understands. The important difference is that in Rust the conversion is explicit and the danger is visible: every call into C is wrapped in `unsafe`, because a wrong signature or a bad pointer is undefined behavior, not a thrown exception.

---

## TypeScript/JavaScript Example

In Node, reaching into C means writing a native addon (or using a foreign-function-interface package). The build is driven by `node-gyp` via a `binding.gyp` file, and you write a `.d.ts` so TypeScript knows the shape of what the addon exports. Conceptually, the moving parts look like this:

```jsonc
// binding.gyp — node-gyp compiles text.c into a .node addon
{
  "targets": [
    {
      "target_name": "text",
      "sources": ["src/text.c", "src/addon.c"]
    }
  ]
}
```

```typescript
// text.d.ts — the type declaration for the compiled native addon
declare module "text-addon" {
  export function countChar(s: string, needle: string): number;
  export function shout(s: string): string;
}
```

```typescript
// caller.ts — uses the addon as if it were a normal module
import { countChar, shout } from "text-addon";

console.log("count l =", countChar("hello, world", "l"));
console.log("shout =", shout("Ferris the crab"));
```

The native side has to convert the V8 `string` into a C `char*`, call the C function, and convert the result back: boilerplate the addon author writes by hand against the N-API headers. A pure-JavaScript stand-in for the same two utilities behaves like this under Node v22:

```javascript
// shout-and-count.mjs — pure JS, no native code
function countChar(s, needle) {
  let n = 0;
  for (const ch of s) if (ch === needle) n++;
  return n;
}
const shout = (s) => s.toUpperCase();

console.log("count l =", countChar("hello, world", "l"));
console.log("shout =", shout("Ferris the crab"));
```

```text
count l = 3
shout = FERRIS THE CRAB
```

The interesting part is what the native addon hides: the `string` → `char*` conversion, who owns the returned buffer, and the fact that a mistake there is a segfault, not a stack trace. Rust makes every one of those steps visible. Building native Node addons in Rust specifically is covered in [napi-rs](/20-unsafe-ffi/06-napi/) and [Neon](/20-unsafe-ffi/07-neon/); here we focus on the lower layer: Rust calling plain C.

---

## Rust Equivalent

We will write a tiny C "text" library and call it from Rust. The current stable toolchain is Rust 1.96.0 on the 2024 edition, and `cargo new` selects that edition automatically. Start a binary crate and add `cc` as a **build dependency**:

```bash
cargo new text-ffi
cd text-ffi
cargo add cc --build   # build-dependency: used by build.rs, not by your program
```

The project has three new pieces: the C source, a `build.rs` that compiles it, and the Rust that declares and calls it:

```text
text-ffi/
├── Cargo.toml
├── build.rs          # compiles + links the C at build time
├── csrc/
│   ├── text.c        # the C implementation
│   └── text.h        # (optional) the C header
└── src/
    └── main.rs       # declares extern fns, calls them safely
```

The C library, two functions, one that reads a string and one that mutates it in place:

```c
// csrc/text.c
#include <stddef.h>
#include <ctype.h>

// Count occurrences of byte `needle` in a NUL-terminated C string.
int count_char(const char *s, char needle) {
    int n = 0;
    for (; *s; s++) {
        if (*s == needle) n++;
    }
    return n;
}

// Uppercase the string in place (ASCII only).
void shout(char *s) {
    for (; *s; s++) {
        *s = (char)toupper((unsigned char)*s);
    }
}
```

The build script. Cargo runs `build.rs` before compiling your crate; the `cc` crate finds the system C compiler (`clang`, `gcc`, or MSVC), compiles the sources into a static library, and tells the linker to bundle it in:

```rust
// build.rs
fn main() {
    cc::Build::new()
        .file("csrc/text.c")
        .compile("text"); // produces libtext.a and links it automatically

    // Re-run this script only when the C sources change.
    println!("cargo:rerun-if-changed=csrc/text.c");
    println!("cargo:rerun-if-changed=csrc/text.h");
}
```

And the Rust that declares and calls the C functions, wrapping each call in a safe function:

```rust
// src/main.rs
use std::ffi::{c_char, c_int, CStr, CString};

// The functions our build.rs compiled and linked from csrc/text.c.
// `extern "C"` says "use the C ABI"; the block is `unsafe` because the
// compiler cannot verify these signatures match the real C definitions.
unsafe extern "C" {
    fn count_char(s: *const c_char, needle: c_char) -> c_int;
    fn shout(s: *mut c_char);
}

/// Safe wrapper: count how many times `needle` appears in `text`.
fn count(text: &str, needle: u8) -> Result<i32, std::ffi::NulError> {
    let c_text = CString::new(text)?; // fails if `text` contains an interior NUL
    // SAFETY: c_text is a valid NUL-terminated buffer that outlives the call,
    // and count_char only reads from it.
    let n = unsafe { count_char(c_text.as_ptr(), needle as c_char) };
    Ok(n)
}

/// Safe wrapper: uppercase `text` via the C `shout` function.
fn shout_safe(text: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Move the bytes into a heap buffer C can mutate in place.
    let mut buf = CString::new(text)?.into_bytes_with_nul(); // owned Vec<u8>, NUL-terminated
    // SAFETY: buf is NUL-terminated and writable; shout only touches bytes
    // up to (not including) the NUL, so the terminator is preserved.
    unsafe { shout(buf.as_mut_ptr() as *mut c_char); }
    // Re-read the (now modified) C string and drop the trailing NUL.
    let cstr = CStr::from_bytes_with_nul(&buf)?;
    Ok(cstr.to_str()?.to_owned())
}

fn main() {
    println!("count l = {}", count("hello, world", b'l').unwrap());
    println!("shout = {}", shout_safe("Ferris the crab").unwrap());

    // An interior NUL is rejected before we ever reach C.
    match count("a\0b", b'a') {
        Ok(n) => println!("unexpected ok: {n}"),
        Err(e) => println!("rejected interior NUL: {e}"),
    }
}
```

Running it with `cargo run` compiles the C, links it, and produces:

```text
count l = 3
shout = FERRIS THE CRAB
rejected interior NUL: nul byte found in provided data at position: 1
```

Same numbers as the JavaScript, but now you can see exactly where the `&str` becomes a `char*`, where ownership of the buffer lives, and which lines the compiler refused to trust without an `unsafe` block.

---

## Detailed Explanation

**The `extern "C"` block declares, it does not define.** `unsafe extern "C" { fn count_char(...); }` is a promise to the compiler: "a symbol named `count_char` with this signature exists and follows the C ABI; trust me." Rust never checks that the declaration matches the real C function; that is the whole reason the block is `unsafe`. If you write `-> c_int` but the C function returns `void`, or you list the arguments in the wrong order, it compiles and then misbehaves at runtime. This is the FFI equivalent of a hand-written `.d.ts`: TypeScript trusts your declaration of a JavaScript module, and if you lie, the failure shows up far from the declaration. The `"C"` is the **ABI string**: it controls argument passing, struct layout, and name mangling. (`extern "C"` and `#[repr(C)]` are the foundation of FFI; they are covered from the ground up in [FFI basics](/20-unsafe-ffi/03-ffi-basics/).)

> **Note:** Since the 2024 edition, `extern` blocks must be written `unsafe extern "C"`. In older editions you wrote a bare `extern "C"`; the `unsafe` keyword now makes the trust boundary explicit at the declaration site. `cargo new` selects the 2024 edition, so use `unsafe extern`.

**`build.rs` is a real Rust program that runs at build time.** Cargo compiles and executes it before your crate, and it communicates back by printing `cargo:` directives to stdout. `cc::Build::new().file(...).compile("text")` shells out to the platform C compiler, produces `libtext.a` in the build output directory, and emits the `cargo:rustc-link-lib` / `cargo:rustc-link-search` directives that tell the final link step to pull the archive in. You do not write any of that linking glue yourself. The `cargo:rerun-if-changed` lines are a caching hint: without them Cargo re-runs `build.rs` whenever *any* file changes; with them it only rebuilds the C when the C actually changed. This is the same role `binding.gyp` plays for a Node addon, except it is ordinary Rust code, so you can branch on the target OS, probe for features, or generate sources.

**The C ABI has no concept of a Rust string.** A Rust `&str` is a pointer plus a length, with no terminating byte and a UTF-8 guarantee. A C string is a bare `char*` that runs until the first `0` byte. Bridging that gap is the job of two types:

- **`CString`** is an *owned*, heap-allocated, guaranteed-NUL-terminated buffer you build from Rust data to *hand to* C. `CString::new(text)?` copies the bytes and appends a NUL; it returns `Err(NulError)` if `text` already contains a `0` byte, because a C string cannot represent one. `c_text.as_ptr()` hands C a `*const c_char` that is valid as long as `c_text` is alive, which is why `c_text` is a named local, not a temporary.
- **`CStr`** is a *borrowed* view of an existing NUL-terminated buffer, the type you use to *receive* a `char*` from C. `CStr::from_ptr(ptr)` (used later) or `CStr::from_bytes_with_nul(&buf)` wraps the bytes without copying; `.to_str()` then validates UTF-8 and gives you a `&str`.

**`shout` mutates in place, so we need a writable buffer.** `count_char` only reads, so a `CString` (which exposes `*const c_char`) is enough. But `shout` takes `char *s` and writes through it, so we need a `*mut c_char` backed by memory C is allowed to modify. `into_bytes_with_nul()` consumes the `CString` and gives back the owned `Vec<u8>` (including the trailing NUL), and `buf.as_mut_ptr()` yields a writable pointer into it. After the call we re-wrap the bytes with `CStr::from_bytes_with_nul` to read the result back as a Rust string.

**Every call into C sits in an `unsafe` block with a `// SAFETY:` comment.** The block is not decoration; it is you taking responsibility for the invariants the compiler cannot verify: the pointer is non-null and properly aligned, the buffer is NUL-terminated, it lives at least as long as the call, and C does not read or write out of bounds. The `// SAFETY:` comment is the idiomatic convention for writing down *why* those invariants hold, so the next reader (and Clippy's `undocumented_unsafe_blocks` lint) can audit it. The unsafe is confined to the smallest possible region; everything outside the wrapper functions is ordinary safe Rust. That confinement — `unsafe` inside, safe API outside — is the heart of the pattern detailed in [building safe abstractions](/20-unsafe-ffi/08-safety-abstractions/).

**The error path never reaches C.** `count("a\0b", ...)` fails inside `CString::new` because of the interior NUL, returning an `Err` *before* any pointer is constructed. This is the payoff of doing conversion explicitly: the dangerous boundary is guarded by the type system, so a malformed input becomes a recoverable `Result`, not a buffer over-read in C.

---

## Key Differences

| Concept | TypeScript / Node native addon | Rust calling C |
| --- | --- | --- |
| Declaring the foreign function | `.d.ts` declaration; addon glue against N-API | `unsafe extern "C" { fn ... }` block |
| Build integration | `binding.gyp` + `node-gyp` | `build.rs` + the `cc` crate (or `#[link]`) |
| String representation | V8 string ↔ `char*` done in the addon | `&str` ↔ `CString` / `CStr` done explicitly |
| String terminator | length-prefixed; addon adds NUL | `CString` appends NUL; `CStr` reads to NUL |
| Who guards the boundary | the addon author, by hand | the type system: `unsafe` + `Result` |
| Cost of a wrong signature | crash, far from the declaration | undefined behavior; same risk, made visible |
| Runtime marshaling | V8 ↔ C value conversion per call | none; it is a direct, ABI-level call |

The deepest difference is **honesty about risk**. A Node addon makes FFI *look* like a normal function call; the danger is real but invisible. Rust takes the opposite stance: the `unsafe` keyword, the explicit `CString`/`CStr` conversions, and the `*const`/`*mut` pointer types put the entire hazardous surface on screen. Nothing about calling C is *safer* in Rust at the machine level — a wrong signature segfaults either way — but Rust forces you to draw a clear line between the small audited unsafe region and the safe API around it.

A second difference is **zero marshaling**. A Node addon converts every JavaScript value to and from a C representation on each call, because V8 values and C values are different things. Rust's `extern "C"` call is a plain function call at the ABI level. Passing a `*const c_char` to C is just passing a pointer. The only "conversion" is the one-time `CString::new` copy you write yourself, and for already-`Vec<u8>` data you can often skip even that.

---

## Common Pitfalls

### Forgetting `unsafe` on the `extern` block (2024 edition)

In the 2024 edition an `extern` block must be `unsafe extern`. Writing the old bare form:

```rust playground edition="2021"
use std::ffi::c_int;

extern "C" {                       // does not compile (edition 2024)
    fn abs(input: c_int) -> c_int;
}

fn main() {
    let _ = unsafe { abs(-3) };
}
```

produces the real error:

```text
error: extern blocks must be unsafe
 --> src/main.rs:3:1
  |
3 | / extern "C" {
4 | |     fn abs(input: c_int) -> c_int;
5 | | }
  | |_^
```

The fix is to add `unsafe` before `extern`.

### Calling the function without an `unsafe` block

A function declared in an `extern` block is an unsafe function, so you must call it inside `unsafe { ... }`:

```rust
use std::ffi::c_int;

unsafe extern "C" {
    fn abs(input: c_int) -> c_int;
}

fn main() {
    let n = abs(-3); // does not compile (error[E0133])
    println!("{n}");
}
```

The compiler reports exactly:

```text
error[E0133]: call to unsafe function `abs` is unsafe and requires unsafe block
 --> src/main.rs:8:13
  |
8 |     let n = abs(-3); // no unsafe block
  |             ^^^^^^^ call to unsafe function
  |
  = note: consult the function's documentation for information on how to avoid undefined behavior
```

### Letting the `CString` drop before C is done with it

This is the most dangerous FFI mistake and the compiler **cannot** catch it. The temptation is to inline the conversion:

```rust
// use-after-free at runtime — compiles, but the pointer dangles
let n = unsafe { count_char(CString::new("hi").unwrap().as_ptr(), b'i' as _) };
```

The `CString` is a temporary: it is created, `as_ptr()` borrows a pointer into it, and then it is *dropped at the end of the statement* — potentially before, or while, C reads through the pointer. Always bind the `CString` to a named local that lives across the call, as the working example does with `let c_text = CString::new(text)?;`. This is the same lifetime hazard as handing C a pointer to a stack buffer that goes out of scope.

### Assuming `char` is signed (or 8 bits) everywhere

C's `char` signedness is implementation-defined: it is signed on x86 but unsigned on ARM. Do not declare a C `char*` parameter as `*const i8` or `*const u8` by hand — use `std::ffi::c_char`, which resolves to the correct type for the target. The same goes for `c_int`, `c_long`, `c_double`, and friends: always use the `c_*` aliases from `std::ffi` (or `std::os::raw`) rather than hardcoding `i32`/`i64`, so your declarations stay correct across platforms.

### Interior NUL bytes silently truncating

A C string ends at the first `0` byte, so a Rust string containing an interior NUL cannot round-trip. `CString::new` is strict about this and returns `Err(NulError)`; handle it (as the example does) rather than `.unwrap()`-ing in code that processes untrusted input. The reverse direction is just as important: a `char*` from C might contain non-UTF-8 bytes, so prefer `CStr::to_str()` (which returns a `Result`) when correctness matters, or `to_string_lossy()` when you want a best-effort `String` with replacement characters.

---

## Best Practices

- **Wrap every `extern` call in a safe function.** Expose a normal Rust API (`fn count(text: &str, needle: u8) -> Result<i32, _>`) and keep the `unsafe`, the raw pointers, and the `CString`/`CStr` dance hidden inside. Callers should never see a `*const c_char`. See [building safe abstractions](/20-unsafe-ffi/08-safety-abstractions/).
- **Write a `// SAFETY:` comment for every `unsafe` block**, stating which invariants you are upholding (non-null, aligned, NUL-terminated, lives long enough, no out-of-bounds access). Enable `#![warn(clippy::undocumented_unsafe_blocks)]` to enforce it.
- **Use the `c_*` type aliases**, never hand-rolled integer widths, so signatures stay portable.
- **Add `cargo:rerun-if-changed`** lines to `build.rs` for every C file (and header) you depend on, so Cargo's incremental build stays correct and fast.
- **Bind owned C buffers to named locals** that clearly outlive the call; never pass `CString::new(...).as_ptr()` as a temporary.
- **Decide ownership of returned pointers up front.** If C `malloc`s a buffer and returns it, you must call the matching C `free` function — never Rust's allocator. Copy the data into an owned Rust type and free the C buffer inside the wrapper so no C-owned memory escapes.
- **For anything beyond a handful of functions, generate the declarations with [bindgen](/20-unsafe-ffi/05-bindgen/)** instead of writing `extern` blocks by hand — it reads the C header and produces correct, `c_*`-typed signatures automatically.

> **Tip:** Run `cargo clippy` on FFI code. Clippy flags undocumented `unsafe` blocks, redundant casts, and several pointer-handling mistakes that are easy to make at the boundary.

---

## Real-World Example

A common production scenario: a vendor ships a C function that **allocates** its result and asks you to **free** it with a paired function. Here a `redact` function returns a freshly `malloc`'d copy of its input with `@` masked to `*` (imagine log-scrubbing before shipping lines off-host). The Rust wrapper's whole job is to ensure that C-allocated buffer is freed exactly once and never escapes into safe code.

```c
// csrc/redact.c
#include <stdlib.h>
#include <string.h>

// Returns a newly malloc'd copy of `input` with every '@' replaced by '*'.
// The caller MUST free the result with redact_free().
char *redact(const char *input) {
    size_t len = strlen(input);
    char *out = (char *)malloc(len + 1);
    if (!out) return NULL;
    for (size_t i = 0; i <= len; i++) {
        out[i] = (input[i] == '@') ? '*' : input[i];
    }
    return out;
}

void redact_free(char *p) { free(p); }
```

```rust
// build.rs
fn main() {
    cc::Build::new()
        .file("csrc/redact.c")
        .compile("redact");
    println!("cargo:rerun-if-changed=csrc/redact.c");
}
```

```rust
// src/main.rs
use std::ffi::{c_char, CStr, CString};

unsafe extern "C" {
    fn redact(input: *const c_char) -> *mut c_char;
    fn redact_free(p: *mut c_char);
}

/// Safe wrapper around the C `redact` function.
///
/// The C side allocates the result with `malloc`, so we are responsible for
/// calling `redact_free` exactly once. We copy the bytes into an owned Rust
/// `String` and free the C buffer before returning, so no C-owned memory
/// ever escapes this function.
fn redact_emails(input: &str) -> Result<String, Box<dyn std::error::Error>> {
    let c_input = CString::new(input)?;
    // SAFETY: c_input outlives the call and redact only reads from it.
    let raw = unsafe { redact(c_input.as_ptr()) };
    if raw.is_null() {
        return Err("redact() returned NULL (allocation failed)".into());
    }
    // Copy the result out first; this runs even if UTF-8 validation fails.
    let result = (|| {
        // SAFETY: raw is non-null and points to a NUL-terminated C string.
        let s = unsafe { CStr::from_ptr(raw) };
        Ok(s.to_str()?.to_owned())
    })();
    // SAFETY: raw came from redact()/malloc and is freed exactly once here.
    unsafe { redact_free(raw); }
    result
}

fn main() {
    let masked = redact_emails("contact alice@corp.io or bob@corp.io").unwrap();
    println!("{masked}");
}
```

Running it prints:

```text
contact alice*corp.io or bob*corp.io
```

Three production-grade habits are on display. First, the NULL return from `malloc` failure becomes a recoverable `Err`, not a crash. Second, the data is copied into an owned `String` *before* `redact_free`, so the returned value is fully owned by Rust and the C buffer's lifetime ends inside the function; there is no way for a dangling pointer to leak out. Third, `redact_free` is called on every success path (the closure isolates the fallible conversion), pairing each `malloc` with exactly one `free`. The mismatched-allocator trap — freeing C memory with Rust's allocator, or vice versa — is one of the nastiest FFI bugs, and the wrapper structure makes it impossible for callers to fall into it.

---

## Further Reading

- [The Rust Reference: External blocks](https://doc.rust-lang.org/reference/items/external-blocks.html): the precise rules for `extern` blocks and ABI strings.
- [`std::ffi` module docs](https://doc.rust-lang.org/std/ffi/): `CString`, `CStr`, `c_char`, `NulError`, and the `c_*` type aliases.
- [The `cc` crate docs](https://docs.rs/cc): compiling C/C++ from `build.rs`, including flags, defines, and cross-compilation.
- [The Cargo Book: build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html) — every `cargo:` directive a `build.rs` can emit.
- [The Rustonomicon: FFI](https://doc.rust-lang.org/nomicon/ffi.html): the full treatment of calling, and being called by, C.

Within this guide:

- [Unsafe, explained](/20-unsafe-ffi/00-unsafe-intro/): what `unsafe` actually grants (and what it does not).
- [Unsafe Rust](/20-unsafe-ffi/01-unsafe-rust/): `unsafe` blocks and the operations they enable.
- [Raw pointers](/20-unsafe-ffi/02-raw-pointers/) — `*const T` / `*mut T`, the pointer types every FFI call uses.
- [FFI basics](/20-unsafe-ffi/03-ffi-basics/): `extern "C"`, `#[no_mangle]`, `#[repr(C)]`, and the C ABI.
- [Auto-generating bindings with bindgen](/20-unsafe-ffi/05-bindgen/): skip hand-written `extern` blocks for real C headers.
- [Building safe abstractions](/20-unsafe-ffi/08-safety-abstractions/) — the unsafe-inside / safe-outside pattern used here.
- [When to use unsafe and FFI](/20-unsafe-ffi/09-when-to-use/) — and the many times you should not.
- Node-specific native addons: [napi-rs](/20-unsafe-ffi/06-napi/) and [Neon](/20-unsafe-ffi/07-neon/).
- Earlier foundations: [Getting started](/01-getting-started/), [Rust basics](/02-basics/).
- The flip side of dropping to C for speed: [Performance](/21-performance/).

---

## Exercises

### Exercise 1: Call a C function with no string conversion

**Difficulty:** Beginner

**Objective:** Declare and call the simplest possible C function — `int square(int)` — to get the `extern` block, `build.rs`, and `unsafe` call mechanics into your fingers without any string handling.

**Instructions:**

1. Create a binary crate and `cargo add cc --build`.
2. Write `csrc/math.c` containing `int square(int x) { return x * x; }`.
3. Compile it in `build.rs` with the `cc` crate.
4. Declare `square` in an `unsafe extern "C"` block using `c_int`, and call it from a safe wrapper `fn square(x: i32) -> i32`. Print `square(7)`.

<details>
<summary>Solution</summary>

```c
// csrc/math.c
int square(int x) { return x * x; }
```

```rust
// build.rs
fn main() {
    cc::Build::new()
        .file("csrc/math.c")
        .compile("math");
    println!("cargo:rerun-if-changed=csrc/math.c");
}
```

```rust
// src/main.rs
use std::ffi::c_int;

unsafe extern "C" {
    fn square(x: c_int) -> c_int;
}

fn square_safe(x: i32) -> i32 {
    // SAFETY: square is a pure function with no preconditions on its input.
    unsafe { square(x as c_int) }
}

fn main() {
    println!("square(7) = {}", square_safe(7));
}
```

`cargo run` prints:

```text
square(7) = 49
```

No `CString` is needed because there are no strings, only an `i32` passed by value, which the C ABI handles directly. Note the use of `c_int` rather than `i32` in the declaration, so the signature stays correct on platforms where `int` is not 32 bits.

</details>

### Exercise 2: Pass a raw byte slice (length + pointer) to C

**Difficulty:** Intermediate

**Objective:** Bridge a Rust `&[u8]` to a C function that takes a pointer *and* a length: the common shape for binary data, where there is no NUL terminator to rely on.

**Instructions:**

1. Write a C function `unsigned char checksum(const unsigned char *data, size_t len)` that returns the sum of all bytes mod 256.
2. Declare it in Rust using `c_uchar` for the byte type and `usize` for the length.
3. Write a safe wrapper `fn checksum_of(bytes: &[u8]) -> u8` that passes `bytes.as_ptr()` and `bytes.len()`. Test it on `b"abc"`, an empty slice, and `&[0xFF, 0x02]`.

<details>
<summary>Solution</summary>

```c
// csrc/util.c
#include <stddef.h>

unsigned char checksum(const unsigned char *data, size_t len) {
    unsigned int acc = 0;
    for (size_t i = 0; i < len; i++) acc += data[i];
    return (unsigned char)(acc & 0xFF);
}
```

```rust
// build.rs
fn main() {
    cc::Build::new()
        .file("csrc/util.c")
        .compile("util");
    println!("cargo:rerun-if-changed=csrc/util.c");
}
```

```rust
// src/main.rs
use std::ffi::c_uchar;

unsafe extern "C" {
    fn checksum(data: *const c_uchar, len: usize) -> c_uchar;
}

fn checksum_of(bytes: &[u8]) -> u8 {
    // SAFETY: we pass the slice's pointer and its real length, so C reads
    // exactly `bytes.len()` valid bytes. An empty slice yields a dangling but
    // unused pointer with len 0, which C never dereferences.
    unsafe { checksum(bytes.as_ptr(), bytes.len()) }
}

fn main() {
    println!("{}", checksum_of(b"abc"));        // 97+98+99 = 294 -> 38
    println!("{}", checksum_of(&[]));           // 0
    println!("{}", checksum_of(&[0xFF, 0x02])); // 257 -> 1
}
```

`cargo run` prints:

```text
38
0
1
```

The key insight versus the string examples: there is **no NUL terminator**, so the length is passed explicitly. Passing the *real* `bytes.len()` is the safety invariant — if you passed a larger number, C would read past the slice into undefined memory. `c_uchar` (an `unsigned char`) maps cleanly to Rust's `u8`.

</details>

### Exercise 3: An out-parameter buffer that C writes into

**Difficulty:** Advanced

**Objective:** Handle the C idiom where the caller provides a buffer and the C function *fills it in*: the safe-wrapper discipline for `*mut` pointers that point into Rust-owned memory.

**Instructions:**

1. Write `void byte_to_hex(unsigned char byte, char *out)` that writes the two lowercase hex digits of `byte` into `out[0]` and `out[1]`.
2. Declare it in Rust (`byte: u8`, `out: *mut c_char`).
3. Write a safe `fn hex_byte(b: u8) -> String` that allocates a 2-byte buffer, passes a `*mut c_char` to it, and turns the result into a `String`. Then build `fn hex_string(bytes: &[u8]) -> String` on top. Test on `b"Rust"` and `&[0, 255, 16]`.

<details>
<summary>Solution</summary>

```c
// csrc/hex.c
void byte_to_hex(unsigned char byte, char *out) {
    const char *digits = "0123456789abcdef";
    out[0] = digits[byte >> 4];
    out[1] = digits[byte & 0x0F];
}
```

```rust
// build.rs
fn main() {
    cc::Build::new()
        .file("csrc/hex.c")
        .compile("hex");
    println!("cargo:rerun-if-changed=csrc/hex.c");
}
```

```rust
// src/main.rs
use std::ffi::c_char;

unsafe extern "C" {
    fn byte_to_hex(byte: u8, out: *mut c_char);
}

fn hex_byte(b: u8) -> String {
    let mut buf = [0u8; 2];
    // SAFETY: byte_to_hex writes exactly 2 bytes; our buffer is 2 bytes long.
    unsafe { byte_to_hex(b, buf.as_mut_ptr() as *mut c_char); }
    // The two bytes are guaranteed ASCII hex digits, so this is valid UTF-8.
    String::from_utf8(buf.to_vec()).unwrap()
}

fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| hex_byte(b)).collect()
}

fn main() {
    println!("{}", hex_string(b"Rust"));      // 52757374
    println!("{}", hex_string(&[0, 255, 16])); // 00ff10
}
```

`cargo run` prints:

```text
52757374
00ff10
```

The safety obligation flips compared to the read-only examples: now C **writes** through the pointer, so the wrapper must guarantee the buffer is *at least* as large as everything C will write. The C contract says "exactly 2 bytes," and we hand it a 2-byte array — make the buffer too small and C would corrupt adjacent stack memory, a bug the compiler cannot catch. Passing a `*mut` into Rust-owned stack memory (rather than a heap allocation) is fine precisely because the buffer outlives the synchronous call.

</details>
