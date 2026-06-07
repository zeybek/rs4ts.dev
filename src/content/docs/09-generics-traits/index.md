---
title: "Generics & Traits"
sidebar:
  label: "Overview"
description: "Map TypeScript generics and interfaces onto Rust's monomorphized generics and nominal traits, with trait bounds, trait objects, and coherence rules explained."
---

In TypeScript you reach for `<T>` generics when you want one function or class to work over many types, and `interface` when you want to describe a shared shape. Rust offers the same two ideas — **generics** and **traits** — but builds them on a different foundation. Generics are **monomorphized** into specialized machine code at compile time (not erased like TypeScript's), traits are **nominal** contracts you opt into with explicit `impl` blocks (not structural), and the whole system is governed by **trait bounds**, **trait objects** for opt-in dynamic dispatch, and **coherence** rules that prevent two crates from clashing. This section maps each TypeScript habit onto its idiomatic Rust counterpart, so you can write polymorphic, zero-cost-abstraction code the Rust way.

---

## What You'll Learn

- How TypeScript `<T>` generic functions become Rust `fn f<T>(...)`, and why **monomorphization** (one specialized copy per concrete type) differs from TypeScript's **type erasure**; plus the **turbofish** `::<>` for when inference needs a hint
- How to put type parameters on **structs** and **enums**, including multiple type parameters and constraints attached to `impl` blocks, with `Option<T>` and `Result<T, E>` as the canonical generic enums
- How a TypeScript `interface` becomes a **trait**, written as a separate `impl Trait for Type` block, and the difference between **required** and **provided (default)** methods
- How to constrain generics with **trait bounds** (`<T: Trait>`, multiple bounds with `+`, and `where` clauses), including bounds on return types
- When to use **trait objects** (`&dyn Trait`, `Box<dyn Trait>`) for runtime dynamic dispatch, what **dyn compatibility** (object safety) requires, and the static-vs-dynamic dispatch trade-off
- How **`impl Trait`** works in argument and return position (RPIT), with a note on return-position `impl Trait` in traits (RPITIT)
- How **default method implementations** and **supertraits** (one trait requiring another) reduce boilerplate and express prerequisites
- How to overload operators (`Add`, `Sub`, `Mul`, `Index`, …) by implementing the corresponding traits, and what the **marker traits** `Send`, `Sync`, `Copy`, and `Sized` signal to the compiler
- Why the **orphan rule** forbids implementing a foreign trait for a foreign type, and the **newtype** pattern that works around it

---

## Topics

| Topic | Description |
| --- | --- |
| [Generic Functions](/09-generics-traits/00-generic-functions/) | Generic functions `<T>`; monomorphization vs TypeScript type erasure; the turbofish `::<>`. |
| [Generic Structs](/09-generics-traits/01-generic-structs/) | Generic data structures; multiple type parameters; constraints attached to `impl` blocks. |
| [Generic Enums](/09-generics-traits/02-generic-enums/) | Generic enums, with `Option<T>` and `Result<T, E>` as the canonical examples. |
| [Traits](/09-generics-traits/03-traits/) | Interfaces → traits: defining and implementing a trait via the `impl Trait for Type` syntax. |
| [Trait Methods](/09-generics-traits/04-trait-methods/) | Required vs provided (default) methods; calling them and overriding defaults. |
| [Trait Bounds](/09-generics-traits/05-trait-bounds/) | Trait bounds `<T: Trait>`, multiple bounds, `where` clauses, and bounds on return types. |
| [Trait Objects](/09-generics-traits/06-trait-objects/) | Dynamic dispatch: `&dyn Trait` / `Box<dyn Trait>`, dyn compatibility, and static-vs-dynamic trade-offs. |
| [`impl Trait`](/09-generics-traits/07-impl-trait/) | `impl Trait` in argument and return position (RPIT), with a brief note on RPITIT. |
| [Default Implementations](/09-generics-traits/08-default-impls/) | Default method bodies and how they cut implementation boilerplate. |
| [Supertraits](/09-generics-traits/09-supertraits/) | Supertraits (trait inheritance): requiring one trait as a prerequisite for another. |
| [Operator Overloading](/09-generics-traits/10-operator-overloading/) | Operator traits `Add`, `Sub`, `Mul`, `Index`, and friends; implementing `+` for your own type. |
| [Marker Traits](/09-generics-traits/11-marker-traits/) | Marker traits `Send`, `Sync`, `Copy`, `Sized`: what they signal, and auto traits. |
| [The Orphan Rule](/09-generics-traits/12-orphan-rule/) | Coherence and the orphan rule; why you cannot impl a foreign trait for a foreign type; the newtype workaround. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Write generic functions, structs, and enums, and explain how monomorphization turns one generic source into specialized, zero-cost concrete code
- Reach for the turbofish `::<>` or a binding annotation exactly when inference needs help, and not before
- Define traits, implement them with `impl Trait for Type`, and distinguish required methods from provided (default) ones
- Constrain a generic with the loosest trait bounds that compile, using `+` and `where` clauses for readability
- Choose deliberately between static dispatch (generics / `impl Trait`) and dynamic dispatch (`dyn Trait`), and recognize when a trait is not dyn compatible
- Use `impl Trait` in argument and return position, and know when a trait method that returns an `impl Trait` (RPITIT) is appropriate
- Eliminate boilerplate with default method implementations and express prerequisites with supertraits
- Overload operators by implementing the relevant `std::ops` traits, and read what `Send`, `Sync`, `Copy`, and `Sized` tell the compiler
- Diagnose an orphan-rule error (`E0117`) and resolve it by owning the trait or applying the newtype pattern

---

## Prerequisites

- [Section 06: Data Structures](/06-data-structures/) — structs, enums, `Option<T>`, pattern matching, and `impl` blocks; this section generalizes all of them with type parameters and trait contracts (and completes the light introduction to associated types started there)
- [Section 05: Ownership](/05-ownership/): what `&self`, `&mut self`, and `self` receivers mean, plus moves, borrows, and `Clone`, all of which trait method signatures depend on
- [Section 02: Basics](/02-basics/): concrete types (`i32`, `f64`, `String`), immutability by default, and `#[derive(...)]`-style trait derivation

---

## Estimated Time

- **Reading:** 5-6 hours
- **Hands-on Practice:** 4-5 hours
- **Exercises:** 3 hours
- **Total:** 12-14 hours

> **Tip:** Read the topics roughly in listed order. Generics (functions, structs, enums) teach you to abstract over *types*; traits, their methods, and bounds teach you to abstract over *behavior*; trait objects and `impl Trait` are the two dispatch strategies that tie them together; and operator overloading, marker traits, and the orphan rule are the practical edges you will hit in real code. The biggest mental shift for a TypeScript developer is that Rust generics are **monomorphized** (real specialized code, not erased) and traits are **nominal** (you must write an explicit `impl`, not just match a shape).


---

## Frequently asked questions

### What is a trait?

A set of methods a type promises to provide, like a TypeScript interface. You write `impl Trait for Type` separately from both the trait and the type, which lets you add behaviour to types you did not define and powers generics and dynamic dispatch. See [Traits](/09-generics-traits/03-traits/).

### What is the difference between `impl Trait` and `dyn Trait`?

`impl Trait` and generics pick the concrete type at compile time and inline it (static dispatch, zero cost). `dyn Trait` is a trait object that chooses the method at runtime through a vtable, like a virtual method call. See [impl Trait](/09-generics-traits/07-impl-trait/) and [Trait Objects](/09-generics-traits/06-trait-objects/).

### How are Rust generics different from TypeScript's?

TypeScript generics are erased and only constrain shape. Rust generics are monomorphized into specialized code per type and constrained by trait bounds like `T: Display`, so they also guarantee which operations are available. See [Generic Functions](/09-generics-traits/00-generic-functions/) and [Trait Bounds](/09-generics-traits/05-trait-bounds/).

---

**Next:** [Section 10: Smart Pointers →](/10-smart-pointers/): `Box`, `Rc`/`Arc`, `RefCell`/`Mutex`, `Cow`, and `Weak`, where `Box<dyn Trait>` from this section ties generics and trait objects to heap allocation.
