---
title: "Modules & Packages"
sidebar:
  label: "Overview"
description: "Map TypeScript's import/export and npm onto Rust: mod, use and pub for code inside a crate, and Cargo for dependencies, builds, tests, features, and publishing."
---

In TypeScript you split code across files and pull pieces in with `import`/`export`, and you manage third-party code with npm and a `package.json`. Rust covers the same ground with two distinct systems: **modules** (`mod`, `use`, `pub`) organize code *within* a crate, and **Cargo** (`Cargo.toml`, `cargo add`, workspaces) manages packages, dependencies, and builds. The biggest surprises for a TypeScript developer are that modules are **not** one-file-equals-one-module by default, that *everything is private until you say `pub`*, and that Cargo is far more than npm: it is also your build tool, test runner, formatter, linter, and doc generator rolled into one.

---

## What You'll Learn

- How ES modules map onto Rust's `mod` system — inline modules, file-based modules, and the mental model that a crate is a *tree* of modules rooted at `lib.rs`/`main.rs`
- How to navigate that tree with paths: `crate::`, `super::`, `self::`, and absolute vs relative paths
- How `import` becomes `use`, including re-exporting with `pub use` and renaming with `as`
- Why Rust items are **private by default** and how `pub`, `pub(crate)`, and `pub(super)` give you precise visibility control that `export` never offered
- How `package.json` becomes `Cargo.toml`, what `Cargo.lock` is for, and how build profiles work
- The everyday `cargo` commands (`build`/`run`/`test`/`check`/`fmt`/`clippy`/`doc`/`add`) and how they replace your npm-script muscle memory
- How `npm install` becomes `cargo add`, how semver requirements (caret/tilde/exact) work, and how to enable optional crate **features**
- The difference between `[dependencies]`, `[dev-dependencies]`, and `[build-dependencies]`
- How a monorepo becomes a **Cargo workspace** with a shared lockfile and `workspace` dependency inheritance
- How **feature flags** drive conditional compilation with `#[cfg(...)]`, and why features must be *additive*
- What a `build.rs` **build script** does, and how to publish a crate to **crates.io**

---

## Topics

| Topic | Description |
| --- | --- |
| [Modules](/12-modules-packages/00-modules/) | ES modules → `mod`: inline modules and file-based modules, and the module-system mental model. |
| [The Module Tree](/12-modules-packages/01-module-tree/) | Paths through the tree: `crate::` / `super::` / `self::`, and absolute vs relative paths. |
| [The `use` Keyword](/12-modules-packages/02-use-keyword/) | `import` → `use`: bringing items into scope, re-exporting with `pub use`, and renaming with `as`. |
| [Visibility with `pub`](/12-modules-packages/03-pub-visibility/) | `export` → `pub`: private-by-default, and `pub(crate)`/`pub(super)`/`pub(in path)` for precise control. |
| [Cargo & Cargo.toml](/12-modules-packages/04-cargo/) | `package.json` → `Cargo.toml`: `[package]`/`[dependencies]`, `Cargo.lock`, and build profiles. |
| [Cargo Commands](/12-modules-packages/05-cargo-commands/) | The common `cargo` commands (`build`/`run`/`test`/`check`/`fmt`/`clippy`/`doc`/`add`) and `new` vs `init`. |
| [Dependencies](/12-modules-packages/06-dependencies/) | `npm install` → `cargo add`: semver requirements, features, and git/path dependencies. |
| [Dev & Build Dependencies](/12-modules-packages/07-dev-dependencies/) | `devDependencies` → `[dev-dependencies]`, plus `[build-dependencies]` and optional deps. |
| [Workspaces](/12-modules-packages/08-workspaces/) | Monorepos → Cargo workspaces: `[workspace]`, members, the shared lockfile, and workspace deps. |
| [Feature Flags](/12-modules-packages/09-feature-flags/) | Conditional compilation: `[features]`, `#[cfg(feature = "...")]`, default features, and additive design. |
| [Build Scripts](/12-modules-packages/10-build-scripts/) | `build.rs`: code generation, linking native libraries, and `cargo::rerun-if-*` directives. |
| [Publishing](/12-modules-packages/11-publishing/) | Publishing to crates.io: `cargo publish`, metadata, versioning, and yanking. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Split a crate into modules across files and navigate it with `crate::`/`super::`/`self::` paths
- Control exactly what is public with `pub` and its scoped variants, and design a clean public API with `pub use` re-exports
- Read and write a `Cargo.toml`, understand `Cargo.lock`, and pick the right command for build/test/lint/doc tasks
- Add, rename, and feature-gate dependencies, and place them in the correct dependency table
- Structure a multi-crate project as a workspace with shared dependencies and a single lockfile
- Use feature flags for conditional compilation in an additive way, write a basic `build.rs`, and publish a crate to crates.io

---

## Prerequisites

- [Section 01: Getting Started](/01-getting-started/). You should already be comfortable running `cargo new`, `cargo build`, and `cargo run`; this section explains what those commands and the generated files actually do.
- Helpful but not required: [Section 09: Generics & Traits](/09-generics-traits/) for understanding what you are exporting, and any earlier section whose code you would like to reorganize into modules.

> **Note:** Some examples in this section deliberately span **multiple files** (e.g. `mod auth;` plus a sibling `auth.rs`) or a `Cargo.toml`, so a single snippet won't compile on its own — the surrounding prose shows the full file layout.

---

## Estimated Time

- **Reading:** 4-5 hours
- **Hands-on Practice:** 3 hours
- **Exercises:** 2 hours
- **Total:** 8-10 hours

> **Tip:** Read the *modules* half (`modules` → `module-tree` → `use-keyword` → `pub-visibility`) and the *Cargo* half (`cargo` → `cargo-commands` → `dependencies` → ...) as two related but separable tracks. If you just want to ship code, the Cargo half is the higher-leverage place to start.


---

## Frequently asked questions

### What is Cargo, and how does it map to npm?

Cargo is Rust's build tool and package manager. `cargo add` is `npm install`, `cargo build --release` is `npm run build`, `cargo test` is `npm test`, and `Cargo.toml` is `package.json`. It also bundles the linter, formatter, and docs generator. See [Cargo](/12-modules-packages/04-cargo/).

### How do `mod` and `use` relate to `import`/`export`?

`mod` declares a module (a file or an inline block), `pub` exports an item, and `use` brings a path into scope like `import`. Paths are rooted at `crate`, so you write `use crate::auth::login;`. See [The Module Tree](/12-modules-packages/01-module-tree/) and [The `use` Keyword](/12-modules-packages/02-use-keyword/).

### Why is an item "private" by default?

Everything is private to its module unless you mark it `pub`. This is stricter than JavaScript's file-level exports and makes a crate's public API an explicit, deliberate surface rather than whatever you happened to export. See [Visibility](/12-modules-packages/03-pub-visibility/).

---

**Next:** [Section 13: Testing →](/13-testing/) — Rust's built-in test framework and the ecosystem around it, replacing Jest/Vitest.
