---
title: "Error Handling"
sidebar:
  label: "Overview"
description: "No exceptions and no null in Rust: a tour mapping TypeScript throw/try/catch to Result, null/undefined to Option, and the ? operator that propagates failures."
---

In TypeScript you signal failure by `throw`ing and recover with `try`/`catch`, and you represent "nothing" with `null`/`undefined`. Rust has **no exceptions and no `null`**: a fallible function returns a **`Result<T, E>`**, an optional value is an **`Option<T>`**, and the **`?` operator** propagates failures upward in one character. This section maps every TypeScript error-handling habit onto its idiomatic Rust counterpart, then goes deeper into panics, custom error types, the `std::error::Error` trait, and the two crates that make real-world error handling ergonomic: **`anyhow`** for applications and **`thiserror`** for libraries (current 1.x/2.x APIs, compile-verified).

---

## What You'll Learn

- Why Rust replaces `throw`/`try`/`catch` with the **`Result<T, E>`** return value and `null`/`undefined` with **`Option<T>`**. Failure and absence become part of the type, and the compiler refuses to let you ignore them
- How to choose between `Result` and `Option`, and how to extract their inner values with `match`, `if let`, `let ... else`, and combinators (`map`, `unwrap_or_else`, `ok_or_else`)
- How the **`?` operator** propagates an `Err`/`None` to the caller, and how its built-in **`From`-based conversion** lets one function unify several underlying error types
- The difference between a **recoverable** error (a `Result`) and an **unrecoverable** one (a `panic!`), how unwinding differs from aborting, and when reaching for `panic!` is actually the right call
- When `unwrap`/`expect` are acceptable (**tests, prototypes, provable invariants**) and when they reintroduce exactly the runtime crashes `Result` was meant to prevent, plus how to write a good `expect` message
- How to design **custom error types** as enums or structs, and implement **`Display`** and **`std::error::Error`** so they behave like first-class errors
- What the **`Error` trait** requires (`Display` + `Debug`), how the **`source()`** cause chain works, and how **`Box<dyn Error>`** type-erases any error behind one return type
- How to use **`anyhow` 1.x** (`Context`, `anyhow!`, `bail!`) in applications and **`thiserror` 2.x** (`#[derive(Error)]`, `#[from]`, `#[error("...")]`) in libraries: the current, idiomatic APIs
- How to handle **multiple error types** in one function: by erasing to `Box<dyn Error>` or aggregating into an enum with `#[from]` conversions
- The **design decisions** that tie it all together: libraries vs. applications, error granularity, message quality, and where recoverable handling ends and a programmer bug begins

---

## Topics

| Topic | Description |
| --- | --- |
| [Result and Option](/08-error-handling/00-result-option/) | `try`/`catch` & `throw` → `Result<T, E>` and `null`/`undefined` → `Option<T>`: the difference between them and how to match on each. |
| [The `?` Operator](/08-error-handling/01-question-mark/) | The `?` operator for propagation, `From`-based error conversion, and using `?` in a function (or `main`) that returns `Result`/`Option`. |
| [Panicking](/08-error-handling/02-panic/) | `throw` → `panic!`: unwinding vs. aborting, when panics are appropriate (unrecoverable failures and bugs), and `panic!` vs. `Result`. |
| [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/) | `unwrap`/`expect` and when they are acceptable (tests, prototypes, provable invariants), plus how to write a useful `expect` message. |
| [Custom Error Types](/08-error-handling/04-custom-errors/) | Defining custom error types as enums or structs and implementing the `Display` and `Error` traits by hand. |
| [The `Error` Trait](/08-error-handling/05-error-trait/) | `std::error::Error`: the `Display` + `Debug` requirements, the `source()` cause chain, and `Box<dyn Error>`. |
| [`anyhow` & `thiserror`](/08-error-handling/06-anyhow-thiserror/) | `anyhow` 1.x for applications (`Context`, `anyhow!`) and `thiserror` 2.x for libraries (`#[derive(Error)]`, `#[from]`) — current APIs, compile-verified. |
| [Handling Multiple Error Types](/08-error-handling/07-multiple-errors/) | Combining several error types in one function: `Box<dyn Error>`, enum aggregation, and `#[from]` conversions. |
| [Error-Handling Best Practices](/08-error-handling/08-best-practices/) | Error design: libraries vs. applications, when to use which tool, granularity, message quality, and recoverable vs. unrecoverable. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Translate a TypeScript function that `throw`s into one that returns `Result<T, E>`, and a `User | undefined` return into `Option<T>`, then handle both with `match`, `if let`, `let ... else`, or combinators
- Decide between `Result` and `Option` for a given failure, and between a `String` error, a custom enum, `Box<dyn Error>`, and `anyhow::Error` for the `E`
- Use the `?` operator to write straight-line happy-path code, and add the `From` impls (by hand or via `#[from]`) that let `?` convert error types automatically
- Explain why a `panic!` is not a TypeScript `throw`, and reserve panics for unrecoverable bugs while returning `Result` for anything a caller could handle
- Justify each `unwrap`/`expect` in your code, replacing the rest with `?`, `match`, or the `unwrap_or_*` family, and turn on Clippy's `unwrap_used`/`expect_used` lints where appropriate
- Define a custom error type, implement `Display` and `std::error::Error` (including `source()`), and box it behind `Box<dyn Error>` when type erasure is the right trade-off
- Pick `thiserror` for a library's precise, matchable error enum and `anyhow` for an application's "propagate-and-report" flow, and add `.context(...)` to make failures diagnosable

---

## Prerequisites

- [Section 06: Data Structures](/06-data-structures/) — `Result` and `Option` are ordinary **enums**, and you handle them with **pattern matching** (`match`, `if let`, `let ... else`). Be comfortable with [enums](/06-data-structures/02-enums/), [`Option<T>`](/06-data-structures/03-option-enum/), and [`impl` blocks](/06-data-structures/05-impl-blocks/) before starting here.
- [Section 05: Ownership](/05-ownership/): error values are owned and moved like any other data; `?` returns (and therefore moves) an error out of a function, and `Box<dyn Error>` is a heap-owned trait object.
- [Section 02: Basics](/02-basics/): concrete types, `#[derive(Debug)]`, and `{:?}`/`{}` formatting (`Display` vs `Debug`) show up on every page.
- Helpful but not required: [Section 04: Control Flow](/04-control-flow/) for `match` and early `return`. A few topics preview [Section 09: Generics and Traits](/09-generics-traits/) — the `<T, E>` and the `From`/`Error` traits behind `Result` and `?` — but each page explains what it needs as it goes.

---

## Estimated Time

- **Reading:** 4-5 hours
- **Hands-on Practice:** 3-4 hours
- **Exercises:** 3 hours
- **Total:** 10-12 hours

> **Tip:** Read the topics in order. Start with the *mechanics* — `result-option` → `question-mark` — because everything else builds on `Result`, `Option`, and `?`. Then learn the *failure modes* (`panic` → `unwrap-expect`), then how to *design error types* (`custom-errors` → `error-trait` → `anyhow-thiserror` → `multiple-errors`), and finish with `best-practices` to tie the choices together. The single biggest mental shift for a TypeScript developer is that **failure is a value, not a jump**: a thrown exception unwinds the stack invisibly, whereas a Rust `Result` is returned normally and the `?` operator makes each propagation point explicit in the source.


---

## Frequently asked questions

### What is the Rust equivalent of `try`/`catch`?

There is none, because there are no exceptions. A fallible function returns `Result<T, E>`, and `match` or the `?` operator handles or propagates the error as an ordinary value. See [Result and Option](/08-error-handling/00-result-option/).

### What does the `?` operator do?

On a `Result` or `Option`, `?` unwraps the value on success or returns early with the error on failure, in one character. It replaces repetitive `if (err) return err` plumbing and the `try`/`catch` dance. See [The `?` Operator](/08-error-handling/01-question-mark/).

### When should I use `unwrap()` or `panic!`?

Only when failure is a genuine bug, or in prototypes and tests, or after you have proven the value is present. In real code, propagate with `?` or handle with `match`; an unexpected `unwrap()` on `None`/`Err` aborts the program. See [unwrap and expect](/08-error-handling/03-unwrap-expect/).

---

**Next:** [Section 09: Generics and Traits →](/09-generics-traits/) — the `<T, E>` type parameters and the `From`/`Error` traits that power `Result` and the `?` operator, generalized into Rust's full generics-and-traits system.
