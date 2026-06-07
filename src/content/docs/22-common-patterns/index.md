---
title: "Common Patterns"
sidebar:
  label: "Overview"
description: "TypeScript's Gang-of-Four patterns in idiomatic Rust: builder, strategy, visitor, factory and RAII collapse into enums, generics, or compile-time checks."
---

In TypeScript and other object-oriented languages you reach for the classic Gang-of-Four patterns — builder, strategy, decorator, visitor, command, factory — to work around what the language *lacks*: real sum types, exhaustive dispatch, deterministic destruction, and compile-time invariants. Rust has most of those things built in, so the same patterns either collapse into something far simpler (a visitor becomes a `match` over an `enum`) or move their guarantees from runtime into the type system (a builder's "required field" check becomes a *compile error*). This section walks the patterns a working TypeScript developer already knows and shows the **idiomatic Rust shape** of each: when an `enum` beats a trait hierarchy, when generics beat trait objects, and when a pattern you relied on simply disappears because the borrow checker already covers it.

---

## What You'll Learn

- How the **builder pattern** fills Rust's gap of no overloading and no optional/named parameters: owned vs `&mut self` builders, optional fields, and pushing "you forgot a required field" into a compile error
- How the **newtype pattern** gives you real (not erased) type safety, works around the orphan rule, and gains ergonomics through `Deref` and `From` impls, replacing TypeScript branded types and module augmentation
- How the **type-state pattern** encodes a value's state in its type with generics and `PhantomData`, so calling a method in the wrong state won't compile and costs nothing at runtime
- The **error-propagation** architecture: layered error types, `?` plus `From` to move between layers, and the `thiserror`-in-libraries / `anyhow`-at-the-edge convention
- Why the idiomatic **visitor** in Rust is an `enum` plus exhaustive `match` (the compiler enforces completeness), and when you still want the heavier OO trait form for an open set of node types
- The three encodings of the **strategy pattern** — plain closures, generics (static dispatch), and trait objects (dynamic dispatch) — and how to choose between them
- How the **decorator pattern** becomes a wrapper type implementing the same trait, and how `tower`'s `Layer`/`Service` generalizes "a service wrapped by another service" into reusable middleware
- How the **command pattern** reifies an action as an `enum` variant or a `Box<dyn Fn>`, enabling queuing, logging, and undo/redo
- How **factories** are just associated functions (`Self::new`, named alternative constructors), trait-object factories, or an `enum` + dispatch, since Rust has no `new` keyword and no constructor overloading
- How **dependency injection** is expressed with a trait plus generics (compile-time) or trait objects (runtime), giving you constructor injection and testability without a DI container
- How **RAII and Drop guards** replace `try/finally`, the `using` declaration, and manual `.close()` discipline with one deterministic rule — drop at end of scope — including a hand-rolled `defer`
- How the **extension-trait pattern** adds methods to foreign types (an `IteratorExt`, `StrExt`, …) in a scoped, collision-resistant way, unlike monkey-patching `Array.prototype`

---

## Topics

| Topic | Description |
| --- | --- |
| [The Builder Pattern](/22-common-patterns/00-builder-pattern/) | Owned vs `&mut self` builders, optional fields, and compile-checked required fields. |
| [The Newtype Pattern](/22-common-patterns/01-newtype/) | Type safety, the orphan-rule workaround, and `Deref` / `From` impls. |
| [The Type-State Pattern](/22-common-patterns/02-type-state/) | Encoding state in the type with generics / `PhantomData` so misuse won't compile. |
| [Error-Propagation Patterns](/22-common-patterns/03-error-propagation/) | Layered errors, `?` + `From`, and `anyhow` at the edges vs `thiserror` in libraries. |
| [The Visitor Pattern](/22-common-patterns/04-visitor-pattern/) | Enums + `match` (the idiomatic form) versus the OO trait form. |
| [The Strategy Pattern](/22-common-patterns/05-strategy-pattern/) | Trait objects vs generics vs plain closures. |
| [The Decorator Pattern](/22-common-patterns/06-decorator-pattern/) | Wrapping types, and how `tower`'s `Layer`/`Service` generalizes it. |
| [The Command Pattern](/22-common-patterns/07-command-pattern/) | Enums of commands or `Box<dyn Fn>`, plus undo/redo. |
| [The Factory Pattern](/22-common-patterns/08-factory-pattern/) | Associated functions (`Self::new`), trait-object factories, and enums. |
| [Dependency Injection](/22-common-patterns/09-dependency-injection/) | Generics vs trait objects, constructor injection, and testability. |
| [RAII and Drop Guards](/22-common-patterns/10-raii-pattern/) | Scope guards, releasing resources/locks, and the `defer` pattern. |
| [Extension Traits](/22-common-patterns/11-extension-traits/) | Adding methods to foreign types (e.g. an `IteratorExt`). |

---

## Learning Objectives

By the end of this section, you will be able to:

- Build a fluent builder and decide between an owned and a `&mut self` form, and push required-field checks into compile errors with the type-state technique or a derive crate like `bon`
- Wrap a value in a newtype for real type safety, use it to sidestep the orphan rule, and add ergonomics with `Deref` and `From` without leaking the inner type
- Encode a workflow's states as distinct types so the compiler rejects out-of-order operations, and explain why zero-sized `PhantomData` markers cost nothing at runtime
- Design a layered error architecture: typed `thiserror` enums inside a library, an opaque `anyhow::Error` with context at the application edge, and `?` + `From` to bridge them
- Recognize when a visitor should be a plain `match` over an `enum` (the common case) and when an open node set justifies the OO trait form
- Pick the right strategy encoding — closure, generic, or trait object — based on whether the algorithm is chosen at compile time or run time
- Layer behavior with decorator wrapper types, choosing generic (zero-cost) over boxed (runtime) composition, and read `tower` middleware as the same idea generalized
- Reify actions as command enums or boxed closures and implement undo/redo by recording how each command reverses itself
- Replace constructor overloading with named associated functions, and build trait-object or enum factories when the product set is open or closed respectively
- Inject dependencies through traits using generics or trait objects, and write tests against fakes without a runtime DI framework
- Tie resources to value lifetimes with `Drop`, build scope guards, and reach for a `defer`-style guard instead of `try/finally`
- Add methods to types you do not own with a scoped extension trait, often via a blanket impl, without the global-mutation hazards of monkey-patching

---

## Prerequisites

- [Section 09: Generics & Traits](/09-generics-traits/). Nearly every pattern here is built on traits, trait bounds, trait objects (`dyn`), generics, and the orphan rule. The static-vs-dynamic-dispatch trade-off introduced there is the recurring decision in the strategy, decorator, factory, and dependency-injection patterns, and the newtype pattern is the canonical orphan-rule workaround.
- [Section 14: Macros](/14-macros/). `#[derive(...)]` and crates like `bon` (in the builder pattern) and `thiserror` (in error propagation) are procedural macros; understanding what a derive generates makes those patterns far less magical.
- [Section 05: Ownership](/05-ownership/) — RAII, Drop guards, owned-vs-borrowed builders, and command undo/redo all depend on moves, borrows, and the scope-based `Drop` rule.
- [Section 08: Error Handling](/08-error-handling/). The error-propagation pattern assumes you already know `Result`, the `?` operator, and custom error enums; this section covers the *architecture* on top of those mechanics.

---

## Estimated Time

- **Reading:** 6-7 hours
- **Hands-on Practice:** 4-5 hours
- **Exercises:** 2-3 hours
- **Total:** 14 hours

> **Tip:** Read in listed order, but treat the section as a decision toolkit rather than a checklist. The first three topics (builder, newtype, type-state) are about *encoding correctness in types*; the next four (error propagation, visitor, strategy, decorator) are about *structuring behavior*; the last five (command, factory, dependency injection, RAII, extension traits) are the patterns you will reach for most in day-to-day application code. The recurring mental shift for a TypeScript developer is that many "patterns" exist only to compensate for a missing language feature, and Rust often has that feature, so the pattern either vanishes (the visitor becomes a `match`) or hardens into a compile-time guarantee (the builder's required-field check, the type-state's invalid-method error).

---

**Next:** [Section 23: Ecosystem →](/23-ecosystem/). The crates you will lean on in production: async runtimes, HTTP clients, web frameworks, serialization, logging and tracing, regex, and date/time, where the patterns from this section show up in real library APIs.
