---
title: "Functions"
sidebar:
  label: "Overview"
description: "Map every TypeScript function piece to Rust: signatures, parameters, returns, closures, higher-order functions, fn pointers, and recursion, with the deeper"
---

Functions are the workhorse of any program, and you already write them every day in TypeScript and JavaScript. This section maps each piece you know — declarations, parameters, return values, arrow functions, higher-order functions, function pointers, and recursion — onto its idiomatic Rust counterpart. The syntax is close enough to feel familiar, but a few deeper ideas (typed signatures as hard contracts, the expression-oriented body, the `Fn`/`FnMut`/`FnOnce` closure traits, and the absence of default/rest parameters and guaranteed tail-call optimization) reshape how you read and write Rust code.

---

## What's in This Section

- **[Basic Functions and Signatures](/03-functions/00-basic-functions/)** - `fn` definitions, typed parameters, return types, statements vs expressions
- **[Function Parameters](/03-functions/01-parameters/)** - no default or rest parameters; idiomatic alternatives (`Option<T>`, slices, structs, traits)
- **[Return Values](/03-functions/02-return-values/)** - return types, tail expressions, the unit type `()`, early return, returning tuples
- **[Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/)** - `|args|` syntax, `Fn`/`FnMut`/`FnOnce`, capture by reference vs `move`
- **[Higher-Order Functions](/03-functions/04-higher-order/)** - `map`/`filter`/`reduce` equivalents, taking and returning closures
- **[Function Pointers](/03-functions/05-function-pointers/)** - the `fn` type, passing named functions, function items vs closures
- **[Recursion](/03-functions/06-recursion/)** - recursive functions, stack depth, iterative alternatives, recursive enums with `Box`

---

## What You'll Learn

By the end of this section, you will be able to:

- Write Rust functions with `fn`, `snake_case` names, mandatory typed parameters, and a `-> Type` return arrow
- Use the **tail expression** (last line, no semicolon) as the return value, and reserve `return` for early exits
- Reproduce default, optional, and rest parameters with `Option<T>`, slices `&[T]`, `Default` structs, and the builder pattern
- Return multiple values as a **tuple** (or a named struct) instead of mutating out-parameters
- Translate arrow functions into Rust closures and understand capture by `&`, `&mut`, and `move`
- Distinguish the three closure traits — `Fn`, `FnMut`, `FnOnce` — and accept the weakest one that works
- Take closures as parameters (`impl Fn`) and return them (`impl Fn` or `Box<dyn Fn>`)
- Pass named functions and constructors as function pointers (`fn(T) -> R`)
- Write recursion safely, knowing Rust has no guaranteed tail-call optimization, and reach for iteration or `Box` when appropriate

---

## Topics

| #   | Topic                                                  | What it covers                                                                                             |
| --- | ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| 1   | [Basic Functions and Signatures](/03-functions/00-basic-functions/) | `fn` definitions, typed parameters, the `->` return type, statements vs expressions; vs TS declarations    |
| 2   | [Function Parameters](/03-functions/01-parameters/)                 | No default/rest params; `Option<T>`, slices `&[T]`, `Default` structs, builders, "overloading" via traits  |
| 3   | [Return Values](/03-functions/02-return-values/)                    | Return types, implicit tail-expression return, the unit type `()`, early `return`, returning tuples        |
| 4   | [Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/)  | `(args) =>` becomes `\|args\|`; `Fn`/`FnMut`/`FnOnce`; capture by reference vs move; the `move` keyword     |
| 5   | [Higher-Order Functions](/03-functions/04-higher-order/)            | `map`/`filter`/`reduce` (lazy iterators); taking `impl Fn` and returning `impl Fn` / `Box<dyn Fn>`         |
| 6   | [Function Pointers](/03-functions/05-function-pointers/)            | The `fn` type, passing named functions, function items vs `fn` pointers vs closures                        |
| 7   | [Recursion](/03-functions/06-recursion/)                            | Recursive functions, no guaranteed TCO, stack depth, iterative alternatives, recursive enums with `Box`    |

---

## Learning Objectives

After completing this section, a TypeScript/JavaScript developer should be able to:

1. Read any Rust function signature and explain its parameters and return type as a compiler-enforced contract.
2. Explain why a stray semicolon changes a function's return value to `()`, and fix the resulting `mismatched types` error.
3. Choose the right idiom for "flexible" arguments: `Option<T>`, a slice, a `Default` struct, a builder, or a trait bound.
4. Write closures with inferred types, decide when `move` is required, and pick the correct `Fn`/`FnMut`/`FnOnce` bound.
5. Build lazy iterator pipelines (`iter().filter().map().collect()`) as the replacement for eager `Array.prototype` chains.
6. Decide between a `fn` pointer and a generic `impl Fn` bound for a callback parameter.
7. Recognize when recursion is safe (shallow, recursive data) versus when iteration is the safer Rust default.

---

## Prerequisites

This section assumes you have completed:

- **[Section 02: Basics](/02-basics/)** — variables and mutability (`let mut` is essential for `FnMut`), the basic types you will pass as parameters, and especially the **statement-vs-expression** distinction that underpins tail-expression returns.

If the expression-oriented model (`let x = { ...; a + b };`) feels unfamiliar, re-read the Basics section before starting here.

> **Note:** Several topics in this section _preview_ concepts covered fully later: ownership and `move` ([Section 05](/05-ownership/)), `Option`/`Result` and the `?` operator ([Section 08](/08-error-handling/)), generics and traits ([Section 09](/09-generics-traits/)), and `Box`/`Rc` for recursive types ([Section 10](/10-smart-pointers/)). You do not need those sections first — the links are there for when you want to go deeper.

---

## Estimated Time

- **Reading:** 3-4 hours
- **Hands-on Practice & Exercises:** 3-4 hours
- **Total:** 6-8 hours

A reasonable order is the list order above: basics → parameters → return values → closures → higher-order → function pointers → recursion. Closures (topic 4) are the conceptual heart of the section, so do not skip them.

---

## Frequently asked questions

### How do I write a default or optional parameter?

Rust has neither. Model an optional argument as `Option<T>` and pass `None`, or use the builder pattern when there are many optionals. For overloading-like flexibility, take a generic parameter bounded by a trait. See [Function Parameters](/03-functions/01-parameters/).

### What is the difference between a function and a closure?

A `fn` is a plain named function. A closure (`|x| x + 1`) can capture variables from its surroundings, and the compiler sorts closures into the `Fn`, `FnMut`, and `FnOnce` traits by how they use what they capture. See [Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/).

### Why doesn't my function need a `return`?

The last expression in a block is its value, so `fn add(a: i32, b: i32) -> i32 { a + b }` returns `a + b` with no `return` and no trailing semicolon. An explicit `return` is reserved for early exits. See [Return Values](/03-functions/02-return-values/).

