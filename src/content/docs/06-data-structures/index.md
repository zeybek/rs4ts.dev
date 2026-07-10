---
title: "Rust Structs & Enums"
sidebar:
  label: "Overview"
description: "Map TypeScript interfaces, unions, and null onto Rust's structs, enums, Option, pattern matching, and impl blocks: nominally typed, exhaustively checked."
---

In TypeScript you model data with object literals, `interface`s, `type` aliases, and union types. Rust gives you a tighter, compiler-enforced toolkit: **structs** for fixed records, **enums** for "one of several shapes," **`Option<T>`** instead of `null`/`undefined`, **pattern matching** to destructure and branch in one step, and **`impl` blocks** to attach behavior to data. This section maps each TypeScript habit onto its idiomatic Rust counterpart so you can model a domain the Rust way — nominally typed, exhaustively checked, and with no hidden `undefined`.

---

## What You'll Learn

- How a TypeScript `interface` + object literal becomes a single `struct` that exists at compile time *and* runtime, and what it means for a struct to **own its fields**
- The leaner struct shapes — **tuple structs** and **unit structs** — and the **newtype pattern** that gives a primitive a distinct, type-checked identity
- How TypeScript union and discriminated-union types map to **enums** with data-carrying variants, with mandatory exhaustiveness instead of an opt-in `never` trick
- Why Rust has no `null`/`undefined`, and how **`Option<T>`** with `Some`/`None`, the combinators (`map`, `and_then`, `unwrap_or`), and the `?` operator replace `?.` and `??`
- How **pattern matching** fuses destructuring, narrowing, and branching into one exhaustive `match` (plus `if let`, `let ... else`, and `while let`)
- How class methods become **`impl` blocks**, and how `&self` / `&mut self` / `self` make a method's borrowing rules part of its signature
- How `static` methods become **associated functions**, including the conventional `Self::new` constructor and a first look at the builder pattern
- A light introduction to **associated types and associated constants** (full treatment arrives in Section 09)
- How object property shorthand becomes **field init shorthand**, and how `..other` **struct update syntax** mimics the spread operator

---

## Topics

| Topic | Description |
| --- | --- |
| [Structs](/06-data-structures/00-structs/) | TypeScript objects/interfaces → structs: field definitions, instantiation, ownership of fields, and `#[derive(Debug, Clone)]`. |
| [Tuple Structs and Unit Structs](/06-data-structures/01-tuple-structs/) | Positional tuple structs and fieldless unit structs, with a teaser for the newtype pattern. |
| [Enums and Data-Carrying Variants](/06-data-structures/02-enums/) | TypeScript union types → enums: data-carrying variants, and how enums compare to discriminated unions. |
| [The Option Type](/06-data-structures/03-option-enum/) | `null`/`undefined` → `Option<T>`: `Some`/`None`, the `map`/`and_then`/`unwrap_or` combinators, and `?` with `Option`. |
| [Pattern Matching](/06-data-structures/04-pattern-matching/) | Destructuring → `match` and `let` patterns: struct/enum/tuple/reference patterns and compile-time exhaustiveness. |
| [Methods and `impl` Blocks](/06-data-structures/05-impl-blocks/) | Class methods → `impl` blocks: `&self` / `&mut self` / `self`, and splitting behavior across multiple `impl` blocks. |
| [Associated Functions and Constructors](/06-data-structures/06-associated-functions/) | `static` methods → associated functions: `Self::new` constructors and a builder-pattern teaser. |
| [Associated Types and Associated Constants](/06-data-structures/07-associated-types/) | A light introduction to associated types and associated consts (full treatment in Section 09). |
| [Field Init Shorthand and Struct Update Syntax](/06-data-structures/08-field-init-shorthand/) | Object property shorthand → field init shorthand, plus `..other` struct update syntax. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Choose the right modeling tool for a given shape — struct, tuple struct, unit struct, or enum — and justify why
- Translate a TypeScript `interface`, `type` alias, or discriminated union into an idiomatic, nominally typed Rust equivalent
- Replace `null`/`undefined` and `?.`/`??` chains with `Option<T>`, its combinators, and the `?` operator
- Destructure structs, enums, tuples, and slices with `match`, `if let`, `let ... else`, and `while let`, and rely on exhaustiveness to catch missed cases at compile time
- Attach behavior to your types with `impl` blocks, choosing `&self`, `&mut self`, or `self` deliberately
- Write `Self::new` and named alternative constructors with associated functions, and recognize when a builder is warranted
- Use field init shorthand and `..other` struct update syntax to write concise, readable instantiation

---

## Prerequisites

- [Section 05: Ownership](/05-ownership/) — structs **own** their fields, methods **borrow** or **consume** `self`, and matching can move or borrow; this section assumes you are comfortable with moves, borrows, and `Clone`
- [Section 02: Basics](/02-basics/) — concrete types (`u64`, `f64`, `bool`, `String`), immutability-by-default, and `#[derive(...)]`-style printing with `{:?}`
- [Section 04: Control Flow](/04-control-flow/) — `if`/`else` and loops, which pattern matching builds on and often replaces

---

## Estimated Time

- **Reading:** 4-5 hours
- **Hands-on Practice:** 3-4 hours
- **Exercises:** 2-3 hours
- **Total:** 10-12 hours

> **Tip:** Read the topics in order. Structs, tuple structs, and enums define the *shapes* of data; `Option<T>` and pattern matching are how you *work with* those shapes; and `impl` blocks plus associated functions attach *behavior*. The biggest mental shift for a TypeScript developer is that Rust is **nominally typed** and `match` is **exhaustive** — two same-shaped types are not interchangeable, and the compiler will not let you forget a case.


---

## Frequently asked questions

### What replaces a TypeScript `interface` or `type`?

A data shape is a `struct`; a union like `A | B` is an `enum`. Shared behaviour and contracts are traits, covered in the next section. A `struct` uses concrete, owned field types rather than structural typing. See [Structs](/06-data-structures/00-structs/) and [Enums](/06-data-structures/02-enums/).

### How do enums carry data like a discriminated union?

Each variant can hold its own fields: `enum Event { Click { x: i32, y: i32 }, Close }`. The variant is the tag, and `match` destructures the data while the compiler enforces that every variant is handled. See [Enums](/06-data-structures/02-enums/) and [Pattern Matching](/06-data-structures/04-pattern-matching/).

### Where do methods go without classes?

In an `impl` block next to the type: `impl User { fn greet(&self) -> String { … } }`. The first parameter (`&self`, `&mut self`, or `self`) spells out the borrow that JavaScript hides behind `this`. See [impl Blocks](/06-data-structures/05-impl-blocks/).

---

**Next:** [Section 07: Collections →](/07-collections/) — `Vec`, `HashMap`, `HashSet`, and strings, the container types your structs and enums will live inside.
