---
title: "Advanced Rust: GATs, Pin, Const Generics"
sidebar:
  label: "Overview"
description: "The type-system and runtime frontier: GATs, specialization, Pin, async internals, const generics, allocators, and asm, where stable ends and nightly begins."
---

This section is the type-system and runtime frontier: the features you reach for when ordinary generics and traits run out of room. Most of it you will rarely *write*, but understanding it changes how you read advanced crates and error messages. We cover the markers that encode invariants without storing data (PhantomData), the pinning machinery that makes `async` possible (Pin/Unpin), how `async`/`await` actually desugars into a state machine, custom allocators, inline assembly, const generics, Generic Associated Types, and specialization, with an honest line drawn between what is stable today and what still needs nightly.

---

## What You'll Learn

- **PhantomData** and zero-sized types: how to mark ownership, variance, and lifetimes without storing a value
- **Pin/Unpin**: why self-referential futures must not move, and the guarantees `Pin` provides
- How **async/await actually works**: the `Future` trait, `poll`, the generated state machine, and the `Waker`
- **Custom allocators**: the `GlobalAlloc` trait and swapping in jemalloc/mimalloc
- **Inline assembly** with `asm!`: when it is justified and how its safety contract works
- **Const generics**: types parameterized by constant values, like arrays `[T; N]`
- **Generic Associated Types (GATs)**: lending iterators and the problems they solve
- **Specialization**: what it would enable, why it is still unstable, and safe approximations today

---

## Topics

| Topic | Description |
| --- | --- |
| [PhantomData](/25-advanced-topics/00-phantom-data/) | Zero-sized markers for ownership, variance, and lifetimes without storing data. |
| [Pin and Unpin](/25-advanced-topics/01-pin-unpin/) | Why self-referential futures need pinning, and the guarantees `Pin` provides. |
| [Async Internals](/25-advanced-topics/02-async-internals/) | How `async`/`await` desugars: the `Future` trait, `poll`, the state machine, and `Waker`. |
| [Custom Allocators](/25-advanced-topics/03-allocators/) | The `GlobalAlloc` trait, `#[global_allocator]`, and swapping in jemalloc/mimalloc. |
| [Inline Assembly](/25-advanced-topics/04-inline-assembly/) | `asm!`: when it is justified, register constraints, and safety. |
| [Const Generics](/25-advanced-topics/05-const-generics/) | Types generic over constant values; arrays generic over their length. |
| [Generic Associated Types](/25-advanced-topics/06-gat/) | GATs (stable since 1.65): lending iterators and why they were hard. |
| [Specialization](/25-advanced-topics/07-specialization/) | What specialization would enable, why it is still unstable, and safe alternatives. |
| [Compiler Internals & Tooling](/25-advanced-topics/08-compiler-plugins/) | Proc-macro-driven codegen, build scripts, and what still requires nightly. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Use `PhantomData` to make a type carry the right ownership/variance/lifetime semantics
- Explain why `async` needs `Pin`, and read code that works with pinned values
- Describe how an `async fn` becomes a polled state machine, demystifying the runtime
- Recognize when const generics or GATs are the right tool, and use them where they are stable
- Tell the difference between stable and nightly features, and reach for safe approximations of unstable ones

---

## Prerequisites

- [Section 09: Generics & Traits](/09-generics-traits/) — this section pushes generics, associated types, and trait bounds to their limits.
- [Section 11: Async Programming](/11-async/) — Pin, Unpin, and the async internals only make sense once you have written async code.
- [Section 20: Unsafe & FFI](/20-unsafe-ffi/) — custom allocators, inline assembly, and several of these features involve `unsafe` and its invariants.

---

## Estimated Time

- **Reading:** 6 hours
- **Hands-on Practice:** 5 hours
- **Exercises:** 3 hours
- **Total:** 14 hours

> **Tip:** Treat this section as "read to understand," not "memorize to use." You will write `PhantomData` and const generics occasionally, but Pin, GATs, and specialization mostly matter so you can *read* advanced library code and decode its error messages. Don't let it block your progress. The practical sections do not depend on mastering it.

---

**Next:** [Section 26: Systems Programming →](/26-systems-programming/) — threads, rayon, channels, atomics and memory ordering, and low-level OS interaction.
