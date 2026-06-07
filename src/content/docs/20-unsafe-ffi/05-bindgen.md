---
title: "Generating Bindings with bindgen"
description: "bindgen reads a C header and generates Rust extern \"C\" declarations and #[repr(C)] types at build time, like producing .d.ts from a library."
---

When you want to call a C library that has more than a couple of functions, hand-writing `extern "C"` declarations becomes tedious and error-prone. The **bindgen** crate reads a C header and generates the Rust FFI declarations for you at build time, so the C API stays the single source of truth.

---

## Quick Overview

**bindgen** parses a C (or C++) header using libclang and emits a Rust file full of `extern "C"` function declarations, `#[repr(C)]` structs, enums, and type aliases that exactly mirror the header. You run it from a `build.rs` build script, then `include!` the generated file into your crate.

For a TypeScript/JavaScript developer, this is the spiritual cousin of **generating `.d.ts` type declarations from a `.js` library** or running `protoc`/`openapi-generator` to turn a schema into typed client code. You point a tool at an interface definition and get type-safe glue back. The important difference: the bindings bindgen emits are *raw and `unsafe`*. They mirror the C types faithfully but provide none of Rust's safety guarantees. Making them safe is your job, and this page shows the pattern.

> **Note:** This page is about *auto-generating* bindings. If you only have one or two functions, hand-declaring them (see [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/)) is simpler. For the underlying `extern "C"` / `#[repr(C)]` mechanics, see [FFI Basics](/20-unsafe-ffi/03-ffi-basics/).

---

## TypeScript/JavaScript Example

In the Node.js world you rarely write FFI by hand either. A native addon ships a `.node` binary plus a generated `.d.ts` so TypeScript callers get types. A typical "wrap a C library" workflow with a tool such as `node-gyp` plus a typings generator looks conceptually like this:

```typescript
// temperature.d.ts — GENERATED type declarations for a native addon.
// You did not write these by hand; a tool produced them from the C interface.

export const enum TempUnit {
  Celsius = 0,
  Fahrenheit = 1,
  Kelvin = 2,
}

export interface Reading {
  value: number;
  unit: TempUnit;
}

export function tempConvert(reading: Reading, to: TempUnit): number;
export function tempConversionCount(): bigint;
```

```typescript
// app.ts — your code consumes the generated declarations.
import { tempConvert, TempUnit } from "./temperature";

const f = tempConvert({ value: 100, unit: TempUnit.Celsius }, TempUnit.Fahrenheit);
console.log(`100C = ${f}F`); // 100C = 212F
```

The generated `.d.ts` only describes shapes for the TypeScript compiler; it is erased at runtime and enforces nothing once the program runs. bindgen's output is the same idea (machine-generated glue from an interface) but it produces *executable* Rust declarations that the compiler actually links against.

---

## Rust Equivalent

We will wrap a small C temperature-conversion library. The project layout:

```text
temp_conv/
├── Cargo.toml
├── build.rs            # runs cc + bindgen at build time
├── csrc/
│   ├── temperature.h   # the C interface we want to call
│   └── temperature.c   # the C implementation we compile and link
└── src/
    └── main.rs         # raw bindings + our safe wrapper
```

The C header is the interface bindgen reads:

```c
// csrc/temperature.h
#ifndef TEMPERATURE_H
#define TEMPERATURE_H

#include <stdint.h>

/* A unit of temperature. */
typedef enum TempUnit {
    TEMP_CELSIUS = 0,
    TEMP_FAHRENHEIT = 1,
    TEMP_KELVIN = 2,
} TempUnit;

/* A temperature reading: a value paired with its unit. */
typedef struct Reading {
    double value;
    TempUnit unit;
} Reading;

/* Convert a reading to a different unit. Returns the converted value. */
double temp_convert(Reading reading, TempUnit to);

/* Number of conversions performed since process start. */
uint64_t temp_conversion_count(void);

#endif
```

```c
// csrc/temperature.c
#include "temperature.h"

static uint64_t COUNT = 0;

static double to_celsius(Reading r) {
    switch (r.unit) {
        case TEMP_FAHRENHEIT: return (r.value - 32.0) * 5.0 / 9.0;
        case TEMP_KELVIN:     return r.value - 273.15;
        default:              return r.value;
    }
}

double temp_convert(Reading reading, TempUnit to) {
    COUNT += 1;
    double c = to_celsius(reading);
    switch (to) {
        case TEMP_FAHRENHEIT: return c * 9.0 / 5.0 + 32.0;
        case TEMP_KELVIN:     return c + 273.15;
        default:              return c;
    }
}

uint64_t temp_conversion_count(void) {
    return COUNT;
}
```

Add the build-time tooling. Both are **build-dependencies**, not regular dependencies. They run during compilation, not at runtime. `cargo add` resolves the current versions automatically (it has been built into Cargo since 1.62, no `cargo-edit` needed):

```bash
cargo add bindgen --build   # parses the header, emits Rust FFI declarations
cargo add cc --build        # compiles temperature.c into a static lib we link
```

```toml
# Cargo.toml
[package]
name = "temp_conv"
version = "0.1.0"
edition = "2024"

[build-dependencies]
bindgen = "0.72.1"
cc = "1.2.63"
```

The build script compiles the C, generates bindings, and writes them into `OUT_DIR`:

```rust
// build.rs
use std::env;
use std::path::PathBuf;

fn main() {
    // 1. Compile the C source into a static library and link it.
    cc::Build::new()
        .file("csrc/temperature.c")
        .include("csrc")
        .compile("temperature");

    // 2. Re-run this script only if the header or source actually changes.
    println!("cargo:rerun-if-changed=csrc/temperature.h");
    println!("cargo:rerun-if-changed=csrc/temperature.c");

    // 3. Generate Rust bindings from the C header.
    let bindings = bindgen::Builder::default()
        .header("csrc/temperature.h")
        .allowlist_function("temp_.*")   // only emit what we asked for
        .allowlist_type("Reading")
        .allowlist_type("TempUnit")
        .rustified_enum("TempUnit")      // make TempUnit a real Rust enum
        .derive_default(true)            // add #[derive(Default)] where safe
        .generate()
        .expect("unable to generate bindings");

    // 4. Write the generated Rust into OUT_DIR for `include!`.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("couldn't write bindings");
}
```

Finally, pull the generated file into a private `ffi` module and build a **safe wrapper** on top of it:

```rust
// src/main.rs
#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]

mod ffi {
    // The generated file lives in OUT_DIR, not in your source tree.
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

/// A temperature value tagged with its unit (safe Rust mirror of the C enum).
#[derive(Clone, Copy, Debug)]
pub enum Unit {
    Celsius,
    Fahrenheit,
    Kelvin,
}

impl From<Unit> for ffi::TempUnit {
    fn from(u: Unit) -> Self {
        match u {
            Unit::Celsius => ffi::TempUnit::TEMP_CELSIUS,
            Unit::Fahrenheit => ffi::TempUnit::TEMP_FAHRENHEIT,
            Unit::Kelvin => ffi::TempUnit::TEMP_KELVIN,
        }
    }
}

/// Convert `value` (in `from` units) to `to` units. Safe wrapper over the C call.
pub fn convert(value: f64, from: Unit, to: Unit) -> f64 {
    let reading = ffi::Reading { value, unit: from.into() };
    // SAFETY: `Reading` is plain old data with no invariants; any `TempUnit`
    // discriminant is valid, and `temp_convert` has no preconditions.
    unsafe { ffi::temp_convert(reading, to.into()) }
}

/// How many conversions the C library has performed.
pub fn conversion_count() -> u64 {
    // SAFETY: a parameterless C call that only reads a counter.
    unsafe { ffi::temp_conversion_count() }
}

fn main() {
    let f = convert(100.0, Unit::Celsius, Unit::Fahrenheit);
    let k = convert(32.0, Unit::Fahrenheit, Unit::Kelvin);
    println!("100C = {f}F");
    println!("32F  = {k}K");
    println!("conversions so far: {}", conversion_count());
}
```

Running it produces this real output:

```text
$ cargo run --quiet
100C = 212F
32F  = 273.15K
conversions so far: 2
```

> **Note:** bindgen needs **libclang** installed at build time (it is the C parser). On macOS it ships with the Xcode Command Line Tools; on Debian/Ubuntu install `libclang-dev`; on Windows install LLVM. If bindgen cannot find it, set `LIBCLANG_PATH` to the directory containing `libclang.{so,dylib,dll}`.

---

## Detailed Explanation

**What bindgen actually wrote.** The `build.rs` above produced this `bindings.rs` (shown verbatim, trimmed only of blank lines). Reading it teaches you exactly what bindgen does:

```rust
// $OUT_DIR/bindings.rs — generated by bindgen 0.72.1
#[repr(u32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum TempUnit {
    TEMP_CELSIUS = 0,
    TEMP_FAHRENHEIT = 1,
    TEMP_KELVIN = 2,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Reading {
    pub value: f64,
    pub unit: TempUnit,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of Reading"][::std::mem::size_of::<Reading>() - 16usize];
    ["Alignment of Reading"][::std::mem::align_of::<Reading>() - 8usize];
    ["Offset of field: Reading::value"][::std::mem::offset_of!(Reading, value) - 0usize];
    ["Offset of field: Reading::unit"][::std::mem::offset_of!(Reading, unit) - 8usize];
};
impl Default for Reading {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
unsafe extern "C" {
    pub fn temp_convert(reading: Reading, to: TempUnit) -> f64;
}
unsafe extern "C" {
    pub fn temp_conversion_count() -> u64;
}
```

Line by line:

- **`#[repr(C)] struct Reading`** mirrors the C struct field-for-field. The `#[repr(C)]` attribute pins the field layout so it matches what the C compiler produced; Rust's default `repr(Rust)` layout is deliberately unspecified and would not be ABI-compatible. (See [FFI Basics](/20-unsafe-ffi/03-ffi-basics/) for `#[repr(C)]` in depth.)
- **The `const _: () = { ... }` block** is a compile-time *layout assertion*. bindgen bakes in the sizes and field offsets it observed from the C compiler; if your platform lays the struct out differently, this block fails to compile rather than silently corrupting memory. It costs nothing at runtime.
- **`impl Default for Reading`** appears only because we asked with `.derive_default(true)`. bindgen cannot `#[derive(Default)]` here (the value-then-unit layout needs zeroing) so it writes an explicit zero-initializing impl.
- **`unsafe extern "C" { ... }`** are the raw function declarations. The `unsafe extern` block syntax is the latest-stable form (Rust 1.96.0, 2024 edition): every `extern` block is now written `unsafe extern` and each declared function is implicitly `unsafe` to call, because Rust cannot verify a foreign function upholds any of its safety contract.

**Why `OUT_DIR` and `include!`.** The build script writes into `OUT_DIR`, a per-build scratch directory Cargo provides, and `src/main.rs` pulls it in with `include!(concat!(env!("OUT_DIR"), "/bindings.rs"))`. This keeps the multi-thousand-line generated file out of version control and guarantees it is regenerated whenever the header changes.

**Why the `#![allow(...)]` at the top.** bindgen preserves C naming (`TEMP_CELSIUS`, `Reading`), which violates Rust's `non_camel_case_types` / `non_upper_case_globals` style lints. Without the allow attribute, the compiler warns:

```text
warning: variant `TEMP_CELSIUS` should have an upper camel case name
warning: variant `TEMP_FAHRENHEIT` should have an upper camel case name
warning: variant `TEMP_KELVIN` should have an upper camel case name
```

Confining the bindings to a `mod ffi { ... }` and allowing those lints there is the standard way to silence the noise without disabling the lints crate-wide.

**Why every call sits in `unsafe`.** The generated functions are `unsafe`. Our `convert` and `conversion_count` wrap each call in a small `unsafe { }` block with a `// SAFETY:` comment justifying why the call is sound, then expose a safe signature. This is the **unsafe-inside / safe-outside** pattern that the rest of your program (and your tests) can use without writing `unsafe` again. Covered in depth in [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).

---

## Key Differences

| Aspect | TypeScript `.d.ts` generation | Rust bindgen |
| --- | --- | --- |
| Input | A `.js`/schema file | A C/C++ header (`.h`) |
| Output | Type declarations, erased at runtime | Real `extern "C"` declarations the linker uses |
| Runtime effect | None (types are erased) | The bindings *are* the call sites |
| Safety of output | N/A (no runtime enforcement) | Raw and `unsafe`; you wrap them |
| When it runs | Build/tooling step | `build.rs` during `cargo build` |
| Layout correctness | Not applicable | Verified by compile-time `size_of`/offset assertions |

**Rustified vs. constant enums.** `.rustified_enum("TempUnit")` turns the C enum into a real Rust `enum`, which is ergonomic to `match` on. Without it, bindgen's default is *safer for arbitrary C*: it emits a type alias plus constants, because a C `enum` value read from foreign memory could legally hold a discriminant your Rust enum does not list, and constructing an out-of-range Rust enum is undefined behavior. The default output looks like this:

```rust
// Default bindgen output for the same enum (no .rustified_enum call):
pub const TempUnit_TEMP_CELSIUS: TempUnit = 0;
pub const TempUnit_TEMP_FAHRENHEIT: TempUnit = 1;
pub const TempUnit_TEMP_KELVIN: TempUnit = 2;
pub type TempUnit = ::std::os::raw::c_uint;
```

Use `.rustified_enum()` only for enums you fully control and whose values you trust. For enums whose value comes back across the FFI boundary from C, prefer the default (or `.newtype_enum()`) to avoid UB. This is exactly the kind of trade-off the bindgen API forces you to think about, and it has no analog in `.d.ts` generation.

**bindgen is descriptive, not safe.** A generated `.d.ts` lying about a type just produces a TypeScript error you can ignore with `as any`. bindgen producing a wrong declaration (e.g. you hand-edited it, or the C compiler disagrees about layout) produces *undefined behavior* at runtime. The layout assertions catch the most common case; you remain responsible for the rest.

---

## Common Pitfalls

**Pitfall 1: calling a generated function outside `unsafe`.** The bindings are `unsafe extern`, so every call must be inside an `unsafe` block. Forgetting it is a hard error, not a warning:

```rust
// does not compile (error[E0133]): a generated extern fn called outside unsafe
fn main() {
    let n = ffi::temp_conversion_count();
    println!("{n}");
}
```

The real compiler output:

```text
error[E0133]: call to unsafe function `ffi::temp_conversion_count` is unsafe and requires unsafe block
 --> src/main.rs:5:13
  |
5 |     let n = ffi::temp_conversion_count();
  |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
  |
  = note: consult the function's documentation for information on how to avoid undefined behavior
```

The fix is the wrapper pattern shown above: one small `unsafe { }` block with a `SAFETY:` justification, exposed behind a safe `fn`.

**Pitfall 2: forgetting `rerun-if-changed`.** Without the `cargo:rerun-if-changed=...` lines, Cargo's default heuristic re-runs `build.rs` whenever *any* file in the crate changes, but if you edit the C header in a way Cargo does not notice, your bindings can go stale. Always list the header (and source) explicitly so edits trigger regeneration.

**Pitfall 3: generating the entire transitive header.** Pointing bindgen at a header that `#include`s `<stdint.h>`, `<stdio.h>`, and friends will, by default, emit bindings for *everything* those pull in: thousands of lines and slow builds. Use `allowlist_function`, `allowlist_type`, and `allowlist_var` (regex-based) to emit only the API you actually call, as the example does with `allowlist_function("temp_.*")`.

**Pitfall 4: assuming bindgen makes things safe.** It does not. A bindgen'd function that takes a `*const c_char` still expects a valid, NUL-terminated, correctly-aliased pointer. bindgen gives you a faithful but *raw* declaration; upholding the C function's preconditions is entirely on you (see [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/) and [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/)).

**Pitfall 5: missing libclang.** If the build fails with a message about not finding `libclang`, it is an environment problem, not a code problem. Install the platform's clang/LLVM development package and, if needed, set `LIBCLANG_PATH`.

---

## Best Practices

- **Confine bindings to a private `mod ffi`** with `#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]` (or use bindgen's `.raw_line("...")` to inject the allow into the generated file). Never re-export raw bindings as your crate's public API.
- **Always wrap, never expose.** Build a safe Rust API (`convert`, `slugify`, ...) over the raw bindings and keep `unsafe` blocks small with explicit `// SAFETY:` comments. The two-crate convention — a `*-sys` crate holding only raw bindings and a sibling crate holding the safe wrapper — is the ecosystem standard (`openssl-sys` + `openssl`, `libgit2-sys` + `git2`, and so on).
- **Allowlist aggressively** to keep generated output small, readable, and fast to compile.
- **Pin layout with the generated assertions** (they are on by default) and let `derive_default`/`derive_debug`/`derive_partialeq` add the traits you need rather than hand-writing impls.
- **Use `rerun-if-changed`** for every input file so incremental builds stay correct.
- **Prefer the default (constant) enum representation** for values that originate in C; reserve `.rustified_enum()` for enums you fully own.
- **Vendor or feature-gate** the C source. The `cc` crate compiling a checked-in `csrc/` keeps builds reproducible; linking a system library instead means emitting `cargo:rustc-link-lib=...` from `build.rs`.

> **Tip:** A standalone `bindgen` command-line tool exists (`cargo install bindgen-cli`) for quickly previewing what a header generates: `bindgen csrc/temperature.h --allowlist-function 'temp_.*'`. It is handy for exploration, but for real projects keep generation in `build.rs` so the bindings always track the header.

---

## Real-World Example

A production-flavored wrapper: a C `slugify` function (the kind you might pull in from an existing C codebase) that turns arbitrary text into a URL-safe slug. It uses a caller-provided output buffer and a sentinel return value for "buffer too small", a very common C convention that forces us to think about strings and error handling across the boundary.

```c
// csrc/slug.h
#ifndef SLUG_H
#define SLUG_H

#include <stddef.h>

/*
 * Write a URL-safe slug of `input` into the caller-provided `out` buffer
 * (lowercase ASCII letters/digits, runs of other chars collapsed to '-').
 * Returns the number of bytes written (excluding the NUL terminator),
 * or (size_t)-1 if `out` was too small.
 */
size_t slugify(const char *input, char *out, size_t out_len);

#endif
```

```c
// csrc/slug.c
#include "slug.h"
#include <ctype.h>

size_t slugify(const char *input, char *out, size_t out_len) {
    if (out_len == 0) return (size_t)-1;
    size_t w = 0;
    int prev_dash = 1; /* avoid leading dash */
    for (const char *p = input; *p; ++p) {
        unsigned char c = (unsigned char)*p;
        if (isalnum(c)) {
            if (w + 1 >= out_len) return (size_t)-1;
            out[w++] = (char)tolower(c);
            prev_dash = 0;
        } else if (!prev_dash) {
            if (w + 1 >= out_len) return (size_t)-1;
            out[w++] = '-';
            prev_dash = 1;
        }
    }
    while (w > 0 && out[w - 1] == '-') w--; /* trim trailing dash */
    out[w] = '\0';
    return w;
}
```

```rust
// build.rs
use std::env;
use std::path::PathBuf;

fn main() {
    cc::Build::new()
        .file("csrc/slug.c")
        .include("csrc")
        .compile("slug");
    println!("cargo:rerun-if-changed=csrc/slug.h");
    println!("cargo:rerun-if-changed=csrc/slug.c");

    let bindings = bindgen::Builder::default()
        .header("csrc/slug.h")
        .allowlist_function("slugify")
        .generate()
        .expect("unable to generate bindings");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out.join("bindings.rs"))
        .expect("couldn't write bindings");
}
```

bindgen emits this declaration for the function (note the C `char *` becomes `*const`/`*mut ::std::os::raw::c_char`):

```rust
unsafe extern "C" {
    pub fn slugify(
        input: *const ::std::os::raw::c_char,
        out: *mut ::std::os::raw::c_char,
        out_len: usize,
    ) -> usize;
}
```

The safe wrapper marshals a Rust `&str` into a C string, provides an output buffer, decodes the sentinel return value into a `Result`, and reads the result back as UTF-8:

```rust
// src/lib.rs
#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]

mod ffi {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

use std::ffi::{CStr, CString};

/// Errors that the safe `slugify` wrapper can return.
#[derive(Debug, PartialEq, Eq)]
pub enum SlugError {
    /// The input contained an interior NUL byte (not a valid C string).
    InteriorNul,
    /// The output buffer was too small for the result.
    BufferTooSmall,
}

/// Turn arbitrary text into a URL-safe slug, delegating to the C `slugify`.
pub fn slugify(input: &str) -> Result<String, SlugError> {
    // 1. Marshal the &str into a NUL-terminated C string.
    let c_input = CString::new(input).map_err(|_| SlugError::InteriorNul)?;

    // 2. Provide an output buffer the C side will fill. A slug is never longer
    //    than the input, so input length + 1 (for the NUL) is always enough.
    let mut out = vec![0u8; input.len() + 1];

    // SAFETY: `c_input` is a valid NUL-terminated string; `out` points to
    // `out.len()` writable bytes; `slugify` only writes within that span and
    // NUL-terminates. Both borrows outlive the call.
    let written = unsafe {
        ffi::slugify(
            c_input.as_ptr(),
            out.as_mut_ptr() as *mut std::os::raw::c_char,
            out.len(),
        )
    };

    // The C API signals failure with (size_t)-1, which maps to usize::MAX.
    if written == usize::MAX {
        return Err(SlugError::BufferTooSmall);
    }

    // SAFETY: on success the C function wrote `written` bytes plus a NUL,
    // so `out` is a valid C string.
    let slug = unsafe { CStr::from_ptr(out.as_ptr() as *const std::os::raw::c_char) };
    Ok(slug.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_slug() {
        assert_eq!(slugify("Hello, World!").unwrap(), "hello-world");
    }

    #[test]
    fn collapses_and_trims() {
        assert_eq!(slugify("  Rust & TS:  FFI  ").unwrap(), "rust-ts-ffi");
    }

    #[test]
    fn rejects_interior_nul() {
        assert_eq!(slugify("a\0b"), Err(SlugError::InteriorNul));
    }
}
```

Running the tests gives this real output:

```text
$ cargo test --quiet
running 3 tests
...
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

Notice how all the FFI danger — the raw pointers, the NUL handling, the sentinel decoding — is contained inside one function. Callers and tests work entirely with `&str`, `String`, and `Result`. The `CString`/`CStr` types that bridge Rust strings and C strings are covered in [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/).

---

## Further Reading

- [The bindgen User Guide](https://rust-lang.github.io/rust-bindgen/): the canonical reference for every `Builder` option (allowlisting, enum styles, callbacks).
- [bindgen on docs.rs](https://docs.rs/bindgen/): the `Builder` API for the exact version you depend on.
- [The cc crate](https://docs.rs/cc/): compiling and linking C/C++ from `build.rs`.
- [The Cargo Book: build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html): `build.rs`, `OUT_DIR`, and the `cargo:` directives.
- [The Rustonomicon: FFI](https://doc.rust-lang.org/nomicon/ffi.html): the unsafe FFI fundamentals bindgen builds on.

Within this guide:

- [FFI Basics](/20-unsafe-ffi/03-ffi-basics/) — `extern "C"`, `#[repr(C)]`, and the C ABI that bindgen targets.
- [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/) — hand-writing extern declarations, `build.rs` + `cc`, and `CStr`/`CString`.
- [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/) — the `*const T` / `*mut T` types that appear all over generated bindings.
- [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/) — the `unsafe` blocks every binding call needs.
- [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/) — the unsafe-inside / safe-outside pattern in depth.
- [When `unsafe` and FFI Are Actually Necessary (and the Many Times They Are Not)](/20-unsafe-ffi/09-when-to-use/) — deciding whether FFI is the right tool at all.
- [Section 00: Introduction](/00-introduction/) and [Section 01: Getting Started](/01-getting-started/) — Rust and Cargo fundamentals.
- [Section 02: Basics](/02-basics/) — types and syntax used throughout.
- [Section 21: Performance](/21-performance/) — when calling into C is (and is not) worth the FFI overhead.

---

## Exercises

### Exercise 1: Allowlist the noise away

**Difficulty:** Beginner

**Objective:** See the difference allowlisting makes and keep generated output minimal.

**Instructions:** Take the `temperature.h` from this page and write a `build.rs` that generates bindings *without* any `allowlist_*` calls, then build. Observe how much extra code (e.g. items pulled in from `<stdint.h>`) appears in `$OUT_DIR/bindings.rs`. Then add `allowlist_function("temp_.*")` and the two `allowlist_type` calls and compare. Which approach would you ship?

<details>
<summary>Solution</summary>

```rust
// build.rs — the allowlisted version (what you should ship).
use std::env;
use std::path::PathBuf;

fn main() {
    cc::Build::new()
        .file("csrc/temperature.c")
        .include("csrc")
        .compile("temperature");
    println!("cargo:rerun-if-changed=csrc/temperature.h");
    println!("cargo:rerun-if-changed=csrc/temperature.c");

    let bindings = bindgen::Builder::default()
        .header("csrc/temperature.h")
        .allowlist_function("temp_.*")
        .allowlist_type("Reading")
        .allowlist_type("TempUnit")
        .rustified_enum("TempUnit")
        .generate()
        .expect("unable to generate bindings");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out.join("bindings.rs"))
        .expect("couldn't write bindings");
}
```

Without the allowlist calls, bindgen emits declarations for everything reachable from the header, including types dragged in by `#include <stdint.h>`. The allowlisted version emits only `TempUnit`, `Reading`, `temp_convert`, and `temp_conversion_count`. Always ship the allowlisted version: smaller output, faster compiles, and a clearer contract.

</details>

### Exercise 2: A safe rounding wrapper

**Difficulty:** Intermediate

**Objective:** Build a safe API over a generated binding and add a Rust ergonomic feature.

**Instructions:** Using the temperature library, add a safe function `convert_rounded(value: f64, from: Unit, to: Unit, decimals: u32) -> f64` that calls the C `temp_convert` through the bindings and rounds the result to `decimals` decimal places. Keep the `unsafe` call confined to a wrapper and write a test asserting `convert_rounded(98.6, Unit::Fahrenheit, Unit::Celsius, 2) == 37.0`.

<details>
<summary>Solution</summary>

```rust
// src/main.rs (additions)
/// Convert and round to `decimals` decimal places.
pub fn convert_rounded(value: f64, from: Unit, to: Unit, decimals: u32) -> f64 {
    let raw = convert(value, from, to); // convert() already wraps the unsafe call
    let factor = 10f64.powi(decimals as i32);
    (raw * factor).round() / factor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_to_two_places() {
        assert_eq!(convert_rounded(98.6, Unit::Fahrenheit, Unit::Celsius, 2), 37.0);
    }
}
```

```text
$ cargo test --quiet
running 1 test
.
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

The key idea: `convert_rounded` never writes `unsafe` itself; it builds on the already-safe `convert`, so the unsafe surface stays as small as possible.

</details>

### Exercise 3: Choose the right enum representation

**Difficulty:** Advanced

**Objective:** Understand why `.rustified_enum()` can be unsound for values that come *from* C, and pick a safe representation.

**Instructions:** Suppose a C function `TempUnit detect_unit(const char *label);` returns a `TempUnit` parsed from user input, and a buggy or hostile C implementation could return `99` for unknown labels. Explain why binding this enum with `.rustified_enum("TempUnit")` and matching on the result is undefined behavior, and rewrite the bindgen configuration to make it sound. Show how the safe wrapper would validate the returned value.

<details>
<summary>Solution</summary>

A Rust `enum` may only ever hold one of its declared discriminants — holding any other bit pattern is instant undefined behavior. With `.rustified_enum("TempUnit")`, `detect_unit` is typed to return the Rust `enum TempUnit`, so if C returns `99` you have already constructed an invalid enum before you even `match` on it. The cure is to *not* rustify enums whose values originate in C; use the default (or `.newtype_enum()`), receive the value as a plain integer, and validate it:

```rust
// build.rs — drop .rustified_enum() and use newtype_enum instead.
let bindings = bindgen::Builder::default()
    .header("csrc/temperature.h")
    .allowlist_function("temp_.*")
    .allowlist_function("detect_unit")
    .allowlist_type("Reading")
    .allowlist_type("TempUnit")
    .newtype_enum("TempUnit") // a #[repr(transparent)] wrapper, not a Rust enum
    .generate()
    .expect("unable to generate bindings");
```

```rust
// src/main.rs — validate the C-provided value before trusting it.
pub fn detect_unit(label: &str) -> Option<Unit> {
    let c_label = std::ffi::CString::new(label).ok()?;
    // SAFETY: `c_label` is a valid NUL-terminated string for the call's duration.
    let raw = unsafe { ffi::detect_unit(c_label.as_ptr()) };
    match raw {
        ffi::TempUnit::TEMP_CELSIUS => Some(Unit::Celsius),
        ffi::TempUnit::TEMP_FAHRENHEIT => Some(Unit::Fahrenheit),
        ffi::TempUnit::TEMP_KELVIN => Some(Unit::Kelvin),
        _ => None, // an unknown discriminant like 99 is handled, not UB
    }
}
```

With `newtype_enum`, `TempUnit` is a `#[repr(transparent)]` struct wrapping an integer plus the known-variant constants, so receiving `99` is a perfectly valid value that your `match` rejects safely — no undefined behavior, ever.

</details>
