---
title: "Ownership"
sidebar:
  label: "Overview"
description: "Rust ownership replaces garbage collection: stack vs heap, the three rules, borrowing, lifetimes, move/copy/clone, Rc/Arc, and Drop, mapped from TypeScript."
---

**Ownership** is the single biggest idea that separates Rust from garbage-collected TypeScript and JavaScript, and the reason a Rust program can be memory-safe with no garbage collector and no manual `free`. Instead of a runtime that periodically scans for unreachable objects, the Rust compiler tracks, at *compile time*, exactly who owns each value and inserts cleanup at precisely the right moment. This section builds that model from the ground up: the **stack/heap** memory layout it rests on, the three **ownership rules**, **borrowing** with `&` and `&mut`, **lifetimes**, **move/copy/clone** semantics, **reference counting** for shared ownership, and **Drop/RAII** for deterministic cleanup.

> **Note:** This is the steepest part of the climb for a TypeScript/JavaScript developer, and the most rewarding. Almost everything that feels strange about Rust at first (why you can't use a variable after passing it, why the compiler talks about "borrows," why `&` is everywhere) comes from this one system. Once it clicks, the rest of Rust gets dramatically easier.

---

## What You'll Learn

- Where values live — the **stack** vs the **heap** — and why Rust makes this visible when TypeScript/JavaScript hides it behind a GC
- The **three ownership rules** that replace garbage collection: one owner per value, move on assignment, drop at end of scope
- How to **borrow** data with `&` (shared/immutable references) without copying or transferring ownership, and how the borrow checker prevents dangling references
- How `&mut` works and the **one-mutable-XOR-many-shared** rule that prevents data races at compile time
- Why a value is **moved**, **copied**, or **cloned**, and how to control which one happens (`Copy` vs `Clone`)
- What **lifetimes** (`'a`) are, why they exist, and when **elision** lets you omit the annotations entirely
- How to share a single value among several owners with `Rc<T>` (single-threaded) and `Arc<T>` (thread-safe)
- How the **`Drop` trait** and **RAII** give you deterministic, leak-resistant cleanup: the opposite of JavaScript's GC and `FinalizationRegistry`

---

## Topics

Read these in order; each one builds on the last. The numbered order matches the navigation links inside the files.

| # | Topic | What it covers |
| --- | --- | --- |
| 1 | [Stack and Heap](/05-ownership/00-stack-heap/) | The stack vs heap memory model: what lives where, fixed-size handles vs heap buffers, versus JavaScript's GC-managed heap and value/reference split, and why it matters in Rust. |
| 2 | [The Three Ownership Rules](/05-ownership/01-ownership-rules/) | Each value has exactly one **owner**; ownership **moves** on assignment and when passed to a function; the value is **dropped** at the end of the owner's scope. |
| 3 | [Borrowing and References](/05-ownership/02-borrowing/) | Shared/immutable borrows with `&`; how Rust references differ from JavaScript object references; the borrow checker and how dangling references are prevented. |
| 4 | [Mutable References](/05-ownership/03-mutable-references/) | `&mut`; the one-mutable-XOR-many-shared rule; non-lexical lifetimes; how data races are ruled out at compile time. |
| 5 | [Lifetimes](/05-ownership/04-lifetimes/) | Lifetime annotations (`'a`), why they exist, and how they relate input and output lifetimes in function signatures and structs. |
| 6 | [Lifetime Elision](/05-ownership/05-lifetime-elision/) | The three elision rules, when annotations can be omitted, and the common patterns that "just work" without `'a`. |
| 7 | [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/) | Move semantics vs JavaScript's reference copy; the `Copy` trait (cheap stack duplicates); `Clone` (explicit deep copy); exactly when each happens. |
| 8 | [Reference Counting (`Rc`/`Arc`)](/05-ownership/07-reference-counting/) | `Rc<T>` (single-thread) and `Arc<T>` (thread-safe) for shared ownership; strong counts. A light intro; the full treatment is in [Section 10](/10-smart-pointers/). |
| 9 | [The Drop Trait and RAII](/05-ownership/08-drop-trait/) | RAII and deterministic, scope-based cleanup; the `Drop` trait; drop order; `std::mem::drop`; versus JavaScript GC / `FinalizationRegistry`. |

---

## Learning Objectives

By the end of this section, a TypeScript/JavaScript developer should be able to:

1. Explain, in plain terms, where a value lives (stack vs heap) and why that determines whether assignment **moves** or **copies** it.
2. State the three ownership rules and trace, by reading the code, exactly when each value is dropped.
3. Choose deliberately between **borrowing**, **cloning**, and **shared ownership** for a given design, and reach for `.clone()` intentionally rather than reflexively.
4. Read and write `&` and `&mut` references that satisfy the borrow checker, and apply the one-mutable-XOR-many-shared rule.
5. Add lifetime annotations (`'a`) only where the compiler genuinely needs them, and recognize where the three elision rules let you omit them.
6. Use `Rc`/`Arc` to model shared ownership and inspect a strong count.
7. Use the `Drop` trait and RAII (and `std::mem::drop` for early cleanup) to manage resources deterministically.
8. Recognize and fix the common borrow-checker errors: `E0382` (use after move), `E0499` (two `&mut`), `E0502` (`&` and `&mut` together), `E0515`/`E0597` (returning/holding a dangling reference).

---

## Prerequisites

This section assumes you have completed:

- **[Section 03: Functions](/03-functions/)**: typed parameters and return values. Ownership crosses **every** function boundary, so the difference between an owned parameter (`String`) and a borrowed one (`&str`) is central here.
- **[Section 04: Control Flow](/04-control-flow/)**: scopes and blocks. A value is dropped at the closing `}` of its scope, so knowing exactly where scopes begin and end is what makes drop timing predictable.

If immutability-by-default (`let` vs `let mut`) or the stack/heap idea feels unfamiliar, revisit **[Section 02 — Variables and Mutability](/02-basics/00-variables/)** first. Ownership builds directly on it.

> **Tip:** The apostrophe in a lifetime (`'a`) is the same sigil Rust uses for loop **labels** (`'outer:`) in [Section 04 — Labeled Loops](/04-control-flow/05-labeled-loops/). They are unrelated concepts that merely share punctuation; the compiler always knows which one you mean from context.

---

## Estimated Time

This is the most important — and most demanding — section in the guide, so budget extra time and work through every example by hand.

- **Reading:** 5-6 hours
- **Hands-on Practice:** 4-5 hours
- **Exercises:** 3-5 hours
- **Total:** 12-16 hours

> **Tip:** Do not rush this section. The recommended order is the list order above: stack/heap → ownership rules → borrowing → mutable references → lifetimes → lifetime elision → move/copy/clone → reference counting → Drop. Start with [Stack and Heap](/05-ownership/00-stack-heap/): the memory model it builds is the foundation everything else rests on. If a borrow-checker error frustrates you, that frustration *is* the learning — read the compiler's suggestion, it is unusually good at proposing the fix.

---

## Frequently asked questions

### Why does the compiler say a value was "moved"?

Assigning a non-`Copy` value or passing it to a function transfers ownership, leaving the original binding unusable. Borrow it with `&` to lend access instead, or call `.clone()` when you genuinely need a second independent copy. See [The Three Ownership Rules](/05-ownership/01-ownership-rules/).

### What is the difference between `&` and `&mut`?

`&T` is a shared, read-only borrow; `&mut T` is an exclusive, writable borrow. The rule is one `&mut` XOR any number of `&` at the same time, which is how Rust rules out data races at compile time. See [Borrowing](/05-ownership/02-borrowing/).

### When do I need `Rc` or `Arc`?

When one value genuinely needs several owners, such as a node pointed to by multiple parents. `Rc<T>` is for a single thread and `Arc<T>` is thread-safe. For single ownership, plain ownership or `&` references are cheaper. See [Reference Counting](/05-ownership/07-reference-counting/).

