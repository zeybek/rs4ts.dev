---
title: "Rust Testing with Cargo"
sidebar:
  label: "Overview"
description: "Rust bakes testing into Cargo: annotate #[test], run cargo test, no Jest or Vitest setup. Covers assertions, integration and doc tests, mocking, proptest, coverage."
---

In TypeScript, testing means reaching for a third-party framework — Jest, Vitest, Mocha — plus config, a runner, and assertion libraries. Rust bakes testing into the language and Cargo: you annotate a function with `#[test]`, run `cargo test`, and you are done. This section maps your Jest/Vitest habits onto Rust's built-in `#[test]`, integration tests in `tests/`, and **documentation tests** (your doc examples are compiled and run!), then covers the ecosystem you reach for when the built-ins aren't enough: **mocking** with mockall, **property-based testing** with proptest, **benchmarking** with criterion, and **coverage** with cargo-llvm-cov.

---

## What You'll Learn

- How a Jest/Vitest test becomes a `#[test]` function inside a `#[cfg(test)] mod tests`, run with `cargo test`
- The conventions for *where* tests live — co-located unit tests vs the top-level `tests/` directory — and why Rust lets unit tests reach private items
- The assertion macros `assert!`, `assert_eq!`, `assert_ne!`, and how to attach custom failure messages
- How to test that code panics with `#[should_panic(expected = "...")]`, and how to write tests that return `Result` so you can use `?`
- How black-box **integration tests** in `tests/` exercise only your public API, and how to share helper code between them
- Patterns for setup/teardown without a `beforeEach` — helper constructors, `LazyLock`/`once_cell` for shared state, and RAII guards
- How to mock dependencies by programming to traits and generating test doubles with **mockall** (`#[automock]`)
- How **property-based testing** with **proptest** checks invariants across hundreds of generated inputs and shrinks failures to a minimal case
- How to write statistically rigorous **benchmarks** with **criterion** (stable), and why the built-in `#[bench]` is still nightly-only
- How **doc tests** turn the examples in your `///` comments into tests that must keep compiling
- How to measure **coverage** with cargo-llvm-cov and speed up runs with cargo-nextest, plus a practical TDD loop in Rust

---

## Topics

| Topic | Description |
| --- | --- |
| [Unit Tests](/13-testing/00-unit-tests/) | Jest/Vitest → `#[test]` + `#[cfg(test)] mod tests`, run with `cargo test`. |
| [Test Organization](/13-testing/01-test-organization/) | Where tests live, the `tests` submodule convention, and testing private vs public items. |
| [Assertions](/13-testing/02-assertions/) | `assert!`/`assert_eq!`/`assert_ne!`, custom messages, and comparing with `Debug`. |
| [Testing Panics](/13-testing/03-should-panic/) | `#[should_panic(expected = "...")]` and tests that return `Result<(), E>` so you can use `?`. |
| [Integration Tests](/13-testing/04-integration-tests/) | The `tests/` directory: black-box testing of the public API and shared helper modules. |
| [Test Fixtures](/13-testing/05-test-fixtures/) | Setup/teardown patterns: helper constructors, `LazyLock`/`once_cell` shared state, and RAII guards. |
| [Mocking](/13-testing/06-mocking/) | Mocking strategies, trait-based test doubles, and mockall's `#[automock]`. |
| [Property Testing](/13-testing/07-property-testing/) | Property-based testing with proptest: the `proptest!` macro, shrinking, and vs example-based tests. |
| [Benchmarking](/13-testing/08-benchmarking/) | Statistically driven benchmarks with criterion, and why `#[bench]` is nightly-only. |
| [Doc Tests](/13-testing/09-doc-tests/) | Documentation tests: how `///` code blocks run as tests, `pub` items in scope, and `ignore`/`no_run`/`should_panic`. |
| [Coverage](/13-testing/10-coverage/) | Code coverage with cargo-llvm-cov and faster runs with cargo-nextest. |
| [TDD Workflow](/13-testing/11-tdd-workflow/) | A red-green-refactor loop in Rust, with `cargo watch` for fast feedback. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Write and run unit tests with `#[test]`/`cargo test`, and decide whether a test belongs co-located or in `tests/`
- Assert with the right macro, write clear failure messages, and test both panics and `Result`-returning code
- Structure integration tests against your public API and share setup code between them
- Replace `beforeEach`/`afterEach` with idiomatic fixtures, lazy statics, and RAII cleanup guards
- Mock collaborators by depending on traits and generating doubles with mockall
- Cover whole input ranges with proptest, benchmark hot paths with criterion, and keep your documentation honest with doc tests
- Measure coverage, run tests faster with nextest, and drive a feature with a TDD loop

---

## Prerequisites

- [Section 12: Modules & Packages](/12-modules-packages/): tests live inside the module tree (`#[cfg(test)] mod tests`) and in the `tests/` directory, and the testing crates are added through Cargo, so the module/Cargo model from Section 12 comes first.
- [Section 08: Error Handling](/08-error-handling/): many tests return `Result<(), E>` and use `?`, and you will assert on `Ok`/`Err` values.
- [Section 09: Generics & Traits](/09-generics-traits/): mocking works by programming to traits, so trait basics make the mocking topic land.

---

## Estimated Time

- **Reading:** 5 hours
- **Hands-on Practice:** 4 hours
- **Exercises:** 2-3 hours
- **Total:** 10-12 hours

> **Tip:** Start with `unit-tests`, `assertions`, and `should-panic`. That trio covers 90% of day-to-day testing. Add `integration-tests` and `doc-tests` next. Treat `mocking`, `property-testing`, `benchmarking`, and `coverage` as a toolbox to pull from when a specific need arises, rather than something to master up front.

---

**Next:** [Section 14: Macros →](/14-macros/) — declarative and procedural macros, Rust's compile-time code generation.
