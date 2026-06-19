---
title: "Smart Pointers"
sidebar:
  label: "Overview"
description: "TypeScript hides every heap, sharing, and lifetime choice; Rust's smart pointers make them explicit: Box, Rc/Arc, RefCell, Cell, Cow, Weak, and Deref."
---

In TypeScript and JavaScript every object is a garbage-collected, freely-shared, freely-mutable reference; the runtime makes every storage and lifetime decision for you. Rust hands those decisions back, and **smart pointers** are the types that encode them: `Box<T>` for heap allocation with a single owner, `Rc<T>`/`Arc<T>` for shared ownership, `RefCell<T>`/`Mutex<T>` and `Cell<T>` for **interior mutability** (mutating through a shared reference), `Cow<'_, T>` for clone-on-write, and `Weak<T>` for breaking reference cycles. The `Deref` trait is the quiet machinery that makes all of them feel like the value they wrap. This section maps each TypeScript habit — implicit boxing, aliased references, mutate-anywhere objects — onto the explicit Rust type that expresses the same intent at zero or near-zero runtime cost.

---

## What You'll Learn

- How to put a value on the **heap** with `Box<T>`, why **recursive types** (cons lists, trees, ASTs) require it, and how `Box<dyn Trait>` owns a trait object so one container can hold many concrete types
- How **shared ownership** works with `Rc<T>` (single-threaded) and `Arc<T>` (atomic, thread-safe), why `clone` is a cheap reference-count bump rather than a deep copy, and how to read the live owner count with `strong_count`
- What **interior mutability** is, and how `RefCell<T>` (single-thread, runtime borrow checks that **panic** on violation) and `Mutex<T>` (thread-safe, blocking) let you mutate through a shared `&` reference
- When to drop down to `Cell<T>` for `Copy` types: `get`/`set` of whole values with no references handed out and no runtime borrow tracking
- How `Cow<'_, T>` holds **borrowed or owned** data behind one type and allocates only at the moment you must mutate, eliminating needless copies on the hot path
- How `Weak<T>` breaks the reference **cycles** that would otherwise leak `Rc`/`Arc` memory, and how `upgrade()` safely turns a weak handle back into a strong one
- How the **`Deref`/`DerefMut`** traits and **deref coercion** explain why `&String` works where `&str` is expected and why you rarely write `*` on a `Box`
- A repeatable **decision procedure** (single vs. shared owner, mutated through `&` or not, one thread or many) that lands you on exactly the right smart pointer every time

---

## Topics

| Topic | Description |
| --- | --- |
| [`Box<T>`: Heap Allocation](/10-smart-pointers/00-box/) | `Box<T>` for heap allocation; recursive types (cons list, tree, AST); trait objects with `Box<dyn Trait>`. |
| [Shared Ownership with `Rc`/`Arc`](/10-smart-pointers/01-rc-arc/) | `Rc<T>` (single-thread) vs `Arc<T>` (atomic, thread-safe); shared ownership; `strong_count`; why cloning is a cheap reference bump. |
| [Interior Mutability: `RefCell`/`Mutex`](/10-smart-pointers/02-refcell-mutex/) | Interior mutability; `RefCell<T>` (single-thread, runtime borrow checks) vs `Mutex<T>` (thread-safe); `borrow`/`borrow_mut` panics. |
| [`Cell<T>` for Copy Types](/10-smart-pointers/03-cell/) | `Cell<T>` for `Copy` types; `get`/`set` whole values without handing out references. |
| [Clone-on-Write with `Cow`](/10-smart-pointers/04-cow/) | `Cow<'_, T>` clone-on-write; borrowed vs owned variants; avoiding needless allocation. |
| [Weak References with `Weak<T>`](/10-smart-pointers/05-weak/) | `Weak<T>` to break reference cycles; `upgrade()`; a parent/child graph example. |
| [The `Deref` Trait](/10-smart-pointers/06-deref-trait/) | `Deref`/`DerefMut`; deref coercion; why `&String` works where `&str` is expected; `Box` deref. |
| [Choosing a Smart Pointer](/10-smart-pointers/07-comparison/) | Decision guide: which smart pointer when — a table mapping each need to the type that satisfies it. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Reach for `Box<T>` to break the "infinite size" cycle of a recursive type, and use `Box<dyn Trait>` when different branches must return different concrete types
- Choose between `Rc<T>` and `Arc<T>` based on whether the data crosses threads, and explain why `Rc::clone` / `Arc::clone` are cheap count bumps that free deterministically when the last owner drops
- Apply interior mutability deliberately: `Cell<T>` for `Copy` values, `RefCell<T>` for single-threaded shared mutation, `Mutex<T>`/`RwLock<T>` across threads; and anticipate the runtime panic a doubled `borrow_mut()` produces
- Combine a pointer with a cell idiomatically (`Rc<RefCell<T>>` single-threaded, `Arc<Mutex<T>>` across threads) to model the "shared mutable object" that JavaScript gives you for free
- Use `Cow<'_, str>` to design APIs that return their input untouched on the common path and allocate only when they genuinely change it
- Detect a reference cycle on sight and break it with `Weak<T>`, recovering a live value through `upgrade()`
- Explain deref coercion well enough to predict when `&String`, `&Box<T>`, or `&Vec<T>` will be accepted where a borrowed inner type is expected
- Run the three-question decision procedure to pick the minimal correct smart pointer instead of defaulting to `Arc<Mutex<T>>`

---

## Prerequisites

- [Section 05: Ownership](/05-ownership/) — moves, borrows (`&`/`&mut`), the one-writer-or-many-readers rule, `Drop`, and the [reference-counting introduction](/05-ownership/07-reference-counting/); every smart pointer in this section is a deliberate departure from plain single ownership
- [Section 09: Generics & Traits](/09-generics-traits/) — traits and especially [trait objects](/09-generics-traits/06-trait-objects/), which `Box<dyn Trait>` owns, plus the marker traits `Send`/`Sync` that decide `Rc`-vs-`Arc` and `RefCell`-vs-`Mutex`

---

## Estimated Time

- **Reading:** 4-5 hours
- **Hands-on Practice:** 4-5 hours
- **Exercises:** 2-3 hours
- **Total:** 10-12 hours

> **Tip:** Read [`00_box.md`](/10-smart-pointers/00-box/) first (it is the simplest pointer and introduces deref), then `01_rc-arc.md` → `02_refcell-mutex.md` → `03_cell.md` to build up shared ownership and interior mutability, then `04_cow.md` and `05_weak.md` for the two specialized cases. Finish with [`06_deref-trait.md`](/10-smart-pointers/06-deref-trait/) for the trait that unifies them and [`07_comparison.md`](/10-smart-pointers/07-comparison/) for the decision guide that ties everything together. The biggest mental shift for a TypeScript developer is that what JavaScript does invisibly — heap-allocate, share, and mutate every object — Rust makes you spell out, and each smart pointer is the explicit word for one of those previously-hidden choices.

---

**Next:** [Section 11: Async →](/11-async/) — `Future`s, `async`/`await`, and the runtimes that drive them, where `Arc<Mutex<T>>` from this section becomes the everyday shape of shared application state.
