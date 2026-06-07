---
title: "Macros"
sidebar:
  label: "Overview"
description: "Rust macros are compile-time code generation with zero runtime cost, unlike TypeScript's decorators and Babel transforms. Covers macro_rules!, derive"
---

In TypeScript you have no real compile-time metaprogramming: the closest things are runtime functions, decorators, and build-step AST transformers like Babel. Rust takes the opposite approach with **macros** — compile-time code generation that expands *before* type checking and leaves **no runtime cost** behind. This section covers both families: **declarative macros** (`macro_rules!`, pattern-matching over your source tokens) and **procedural macros** (the derive, attribute, and function-like flavors written with `syn` 2 + `quote`), plus the standard-library macros you will use every single day.

---

## What You'll Learn

- Why a Rust macro is **not** a function and **not** a decorator — it operates on *tokens* at compile time, is gone by runtime, and is protected by **hygiene** so its temporaries cannot clash with yours
- How to write `macro_rules!` declarative macros: matchers, transcribers, and how to inspect the generated code with `cargo expand`
- The full set of **fragment specifiers** (`$x:expr`, `:ident`, `:ty`, `:pat`, `:tt`, `:literal`, `:block`, ...) and how multiple rules give you overload-like dispatch on the *shape* of the input
- How **repetition** (`$(...),*`, `$(...);*`, `$(...)+`, `$(...)?`) builds variadic macros, including a faithful `vec!`-style clone
- What `#[derive(...)]` actually generates (`Debug`, `Clone`, `PartialEq`, `Hash`, `Default`, ...) and how a custom derive looks from the user's side
- How **attribute macros** (`#[name]`) inspect and rewrite a whole item — the compile-time, type-safe counterpart to a TypeScript decorator
- How **function-like procedural macros** (`foo!(...)`) run arbitrary Rust at compile time to parse a custom DSL and *validate* input into a compiler error
- How to *write* procedural macros end to end: the `proc-macro` crate type, `TokenStream`, and the `syn` 2 + `quote` + `proc-macro2` toolchain, with a compile-verified custom derive
- The standard-library macros — `vec!`, `println!`, `format!`, `write!`, `matches!`, the `assert*!` family, `todo!`, `dbg!`, `include_str!`, and more

---

## Topics

| Topic | Description |
| --- | --- |
| [Macro Basics](/14-macros/00-macro-basics/) | What macros are and are **not** (not decorators, not functions); compile-time expansion; hygiene; when to reach for a macro. |
| [Declarative Macros](/14-macros/01-declarative-macros/) | `macro_rules!`: basic matchers, a simple example expanded, and inspecting output with `cargo expand`. |
| [Macro Patterns](/14-macros/02-macro-patterns/) | Fragment specifiers (`$x:expr`, `:ident`, `:ty`, `:tt`, `:pat`, ...) and multiple rules for shape-based dispatch. |
| [Repetition](/14-macros/03-repetition/) | Repetition operators `$(...),*` / `$(...);*` / `$(...)+`, and building a `vec!`-like macro. |
| [Derive Macros](/14-macros/04-derive-macros/) | `#[derive(...)]`: what the std derives generate (`Debug`/`Clone`/`PartialEq`...) plus a custom-derive overview. |
| [Attribute Macros](/14-macros/05-attribute-macros/) | Custom attribute macros (`#[name]`): the concept and a minimal logging/route example. |
| [Function-like Macros](/14-macros/06-function-like-macros/) | Function-like procedural macros `foo!(...)` versus declarative ones, and when to use each. |
| [Procedural Macros](/14-macros/07-proc-macros/) | Writing proc macros: the `proc-macro` crate type, `TokenStream`, `syn` 2 + `quote` + `proc-macro2`, and a compile-verified custom derive. |
| [Common Macros](/14-macros/08-common-macros/) | The std macros: `vec!`/`println!`/`format!`/`write!`/`matches!`/`assert*!`/`todo!`/`unimplemented!`/`dbg!`/`include_str!`. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Explain why macros are compile-time, token-based, and hygienic, and correct the common (wrong) mental model that they are decorators or functions
- Write `macro_rules!` macros with the right fragment specifiers, multiple rules ordered specific-to-general, and repetition with an optional trailing comma
- Read a macro's expansion with `cargo expand` and debug why a call fails to match
- Choose the correct standard derives for a type and read the compiler errors when a field doesn't support a derived trait
- Distinguish the three procedural-macro flavors (derive, attribute, function-like) by their entry-point attribute and signature, and set up a `proc-macro = true` crate
- Write a custom derive with `syn` 2 + `quote`, preserve generics with `split_for_impl`, and emit located compile errors instead of panicking
- Decide *declarative-first, procedural-only-when-needed*, and reach for a plain function before either when values (not syntax) are all you need
- Use the standard-library macros idiomatically, including inline format captures and the `assert!`-panics-vs-`Result` distinction

---

## Prerequisites

- [Section 09: Generics & Traits](/09-generics-traits/) — derive macros generate trait `impl`s, custom derives must preserve trait bounds and generics, and the orphan rule explains why you can only `#[derive(...)]` on types you own. The trait vocabulary from Section 09 makes this whole section land.
- [Section 02: Basics](/02-basics/) — you have already been calling `println!` and `format!`; this section explains what those macro invocations actually are.
- [Section 12: Modules & Packages](/12-modules-packages/) — `#[macro_export]`, importing macros across modules, and the separate `proc-macro` crate all build on the module and Cargo model from Section 12.

---

## Estimated Time

- **Reading:** 4-5 hours
- **Hands-on Practice:** 3 hours
- **Exercises:** 1-2 hours
- **Total:** 8-10 hours

> **Tip:** Read in listed order. The first four topics (`macro-basics`, `declarative-macros`, `macro-patterns`, `repetition`) build a complete picture of `macro_rules!` and cover the macros you will *write* most often. The middle three (`derive-macros`, `attribute-macros`, `function-like-macros`) introduce the procedural flavors conceptually; `proc-macros` then shows the full `syn` + `quote` machinery behind all three. Finish with `common-macros` for the standard-library toolbox you will *use* daily. The biggest mental shift for a TypeScript developer is that a macro is **gone by runtime** — it is a program that writes a program, not a decorator that wraps a value.

---

**Next:** [Section 15: Serialization →](/15-serialization/) — `serde`'s `#[derive(Serialize, Deserialize)]` and the `serde_json::json!` DSL put the derive and declarative macros from this section to work.
