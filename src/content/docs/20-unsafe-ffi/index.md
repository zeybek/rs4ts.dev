---
title: "Unsafe Rust & FFI"
sidebar:
  label: "Overview"
description: "Safe Rust proves memory safety before your program runs. This section covers the unsafe keyword, the C FFI toolchain, and shipping Rust as a Node.js addon."
---

Safe Rust proves your program free of use-after-free, data races, and out-of-bounds access before it ever runs. A handful of low-level operations — talking to C, dereferencing raw pointers, certain hardware-level tricks — cannot be proven safe by the compiler, so Rust gates them behind the `unsafe` keyword. This section builds the correct mental model (`unsafe` is a *promise you make*, not the `any` of Rust), then walks the full **Foreign Function Interface** toolchain: calling C from Rust, auto-generating bindings, and shipping Rust as a Node.js native addon to replace the kind of native modules you would otherwise write in C++.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. The 2024 edition changes a few things you will see throughout: `extern "C"` blocks are written `unsafe extern "C"`, an `unsafe fn` body is "safe by default" (`unsafe_op_in_unsafe_fn`), and you reach a `static mut` through `&raw mut` rather than `&mut`. The recorded ecosystem lines are bindgen 0.72, napi/napi-derive 3, and neon 1.

---

## What You'll Learn

- What `unsafe` actually does — the five superpowers it enables — and the important ways it differs from TypeScript's `any`
- Why the borrow checker, type checker, and lifetimes stay fully on inside an `unsafe` block, and what *undefined behavior* is
- How to write `unsafe` blocks and `unsafe fn`s, the `// SAFETY:` / `/// # Safety` conventions, and the dangers of `static mut`
- How raw pointers (`*const T` / `*mut T`) differ from references, and the rules for creating and dereferencing them
- How the C ABI works in Rust: `extern "C"`, `#[no_mangle]`, `#[repr(C)]`, and what crosses the boundary safely
- How to call real C code from Rust, link a C library, and marshal strings and structs across the FFI boundary
- How to auto-generate bindings from C headers with **bindgen** instead of hand-writing `extern` blocks
- How to ship Rust as a Node.js native addon with **napi-rs** and the alternative **Neon**, replacing C++ addons
- How to wrap a small, audited `unsafe` core behind a fully safe API: the "unsafe inside, safe outside" pattern
- How to judge when `unsafe`/FFI is genuinely necessary, and the many times a safe restructuring is the better answer

---

## Topics

| Topic | Description |
| ----- | ----------- |
| [What `unsafe` Really Means (and What It Does Not)](/20-unsafe-ffi/00-unsafe-intro/) | The five superpowers, why `unsafe` is not TypeScript's `any`, and what undefined behavior is. |
| [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/) | Writing `unsafe` blocks, calling `unsafe fn`s, `static mut`, and the 2024-edition `unsafe_op_in_unsafe_fn` rule. |
| [Raw Pointers: `*const T` and `*mut T`](/20-unsafe-ffi/02-raw-pointers/) | How raw pointers differ from references, creating them in safe code, and dereferencing them under `unsafe`. |
| [FFI Basics: `extern "C"`, `#[no_mangle]`, `#[repr(C)]`, and the C ABI](/20-unsafe-ffi/03-ffi-basics/) | The machinery that lets Rust speak the C ABI and present a stable, predictable memory layout. |
| [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/) | Linking a C library with `build.rs` + `cc`, declaring its functions, and marshaling strings (`CStr`/`CString`) and structs across the boundary. |
| [Generating Bindings with bindgen](/20-unsafe-ffi/05-bindgen/) | Auto-generating `extern` declarations from C headers with a `build.rs` instead of writing them by hand. |
| [Node.js Native Addons with napi-rs](/20-unsafe-ffi/06-napi/) | Shipping Rust as an npm-installable native addon with `#[napi]` exports, building it, and calling it from Node. |
| [Node.js Native Addons with Neon](/20-unsafe-ffi/07-neon/) | The Neon alternative for native Node.js addons, and how it compares to napi-rs. |
| [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/) | The "unsafe inside, safe outside" pattern: upholding invariants so callers never write `unsafe`. |
| [When `unsafe` and FFI Are Actually Necessary](/20-unsafe-ffi/09-when-to-use/) | A decision framework, and the many times a safe restructuring beats reaching for `unsafe`. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Explain precisely what `unsafe` enables and what it leaves unchanged, and refute the "`unsafe` is Rust's `any`" misconception
- Write and review `unsafe` blocks and `unsafe fn`s with the `// SAFETY:` and `/// # Safety` conventions the ecosystem expects
- Create and dereference raw pointers correctly, and articulate the validity invariant every dereference depends on
- Declare and call C functions across the FFI boundary using `extern "C"`, `#[no_mangle]`, and `#[repr(C)]`
- Link a native C library into a Rust crate and pass strings and structs back and forth without leaking or corrupting memory
- Generate bindings from a C header with bindgen and a `build.rs`, instead of hand-maintaining `extern` declarations
- Build a Node.js native addon in Rust with napi-rs (or Neon) and consume it from JavaScript
- Encapsulate an `unsafe` core behind a safe API, and run your code under Miri to catch undefined behavior
- Decide whether a given problem truly needs `unsafe`/FFI, and justify reaching for a safe alternative when it does not

---

## Prerequisites

- [Section 05: Ownership](/05-ownership/): `unsafe` adds five abilities but keeps ownership, borrowing, and lifetimes fully enforced; you must be fluent in them to reason about soundness.
- [Section 10: Smart Pointers](/10-smart-pointers/): many safe abstractions over `unsafe` (and the alternatives to it) are built from `Box`, `Rc`/`RefCell`, and friends.
- [Section 19: WebAssembly](/19-wasm/) — the "talk to another world" mindset and the `cdylib`/`extern` machinery there carry directly into native FFI.

A working knowledge of [error handling](/08-error-handling/) (`Result` at the FFI boundary) and [modules and packages](/12-modules-packages/) (`build.rs`, linking, crate types) will also help.

---

## Estimated Time

Approximately **12 hours**, including reading, hands-on practice, and the per-topic exercises.

---

## Next

Continue to [Section 21: Performance](/21-performance/), where profiling, benchmarking, and memory-layout work occasionally make a small, justified `unsafe` block earn its keep, once you have the discipline this section teaches.
