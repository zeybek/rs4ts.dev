---
title: "Rust Tooling: Cargo, Clippy & rustfmt"
sidebar:
  label: "Overview"
description: "Node scatters tooling across Prettier, ESLint, a test runner, and a bundler. Rust folds most of it into Cargo, plus rustfmt, Clippy, and rust-analyzer."
---

Coming from Node, you assemble a toolchain: a formatter (Prettier), a linter (ESLint), a test runner, a bundler, a debugger, CI config, and a Dockerfile. Rust ships most of that *in one tool* — Cargo — plus first-class formatting (rustfmt), linting (Clippy), and an excellent language server (rust-analyzer). This section maps your Node tooling habits onto the Rust equivalents and covers the workflows that make day-to-day Rust productive: a close look at Cargo, debugging, editor setup, CI/CD with GitHub Actions, Dockerizing, cross-compilation, and the cargo plugins worth installing.

---

## What You'll Learn

- **Cargo beyond the basics**: profiles, aliases, workspace tricks, `[patch]`, and offline builds
- **Prettier → rustfmt** and **ESLint → Clippy**, including how to configure and enforce them
- The most common **Clippy lints** explained with before/after, so the suggestions teach you idiomatic Rust
- **Debugging** Rust with lldb/gdb, the VS Code flow, `dbg!`, and `RUST_BACKTRACE`
- Getting the most out of **rust-analyzer** and a clean **VS Code** setup (using the current `check.command`, not the deprecated `checkOnSave.command`)
- **CI/CD** for Rust and a real **GitHub Actions** workflow (fmt + clippy + test + build, with caching)
- **Dockerizing** Rust with multi-stage builds and small final images
- **Cross-compiling** to other targets, and the **cargo plugins** worth having (nextest, watch, audit, deny, expand)

---

## Topics

| Topic | Description |
| --- | --- |
| [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/) | Profiles, aliases, workspace tricks, `[patch]`, offline builds, and `cargo metadata`. |
| [Formatting](/24-tooling/01-formatting/) | Prettier → rustfmt: `rustfmt.toml`, format-on-save, and CI `fmt --check`. |
| [Linting](/24-tooling/02-linting/) | ESLint → Clippy: running it, lint levels, and `allow`/`warn`/`deny`. |
| [Clippy Lints Explained](/24-tooling/03-clippy-lints/) | Common Clippy lints with before/after: the idioms they teach. |
| [Debugging](/24-tooling/04-debugging/) | lldb/gdb, the VS Code debugging flow, `dbg!`, and `RUST_BACKTRACE`. |
| [rust-analyzer](/24-tooling/05-rust-analyzer/) | What the LSP gives you: inlay hints, code actions, and configuration. |
| [VS Code Setup](/24-tooling/06-vscode-setup/) | Extensions and settings for a productive Rust setup in VS Code. |
| [CI/CD](/24-tooling/07-ci-cd/) | CI/CD concepts for Rust: the fmt + clippy + test + build gates and target caching. |
| [GitHub Actions](/24-tooling/08-github-actions/) | A real GitHub Actions workflow for Rust, with toolchain and cache actions. |
| [Docker](/24-tooling/09-docker/) | Dockerizing Rust: multi-stage builds, `cargo-chef` caching, and small final images. |
| [Cross-Compilation](/24-tooling/10-cross-compilation/) | Cross-compiling with rustup targets, `cross`, and musl static builds. |
| [Cargo Plugins](/24-tooling/11-cargo-plugins/) | Useful plugins: nextest, watch, audit, deny, expand, outdated, bloat. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Drive a Rust project entirely through Cargo, configuring profiles, aliases, and workspaces
- Keep code formatted and lint-clean with rustfmt and Clippy, and enforce both in CI
- Read Clippy's suggestions as idiom lessons rather than mere warnings
- Debug a Rust program with a real debugger and good backtraces
- Set up a fast, comfortable editor experience with rust-analyzer
- Build a CI pipeline and a small Docker image for a Rust service, and cross-compile when needed

---

## Prerequisites

- [Section 12: Modules & Packages](/12-modules-packages/). Tooling is Cargo-centric, so the `Cargo.toml`/workspace model comes first.
- [Section 13: Testing](/13-testing/). Your CI gates run the tests, and plugins like `cargo-nextest` slot into that workflow.

---

## Estimated Time

- **Reading:** 5 hours
- **Hands-on Practice:** 4 hours
- **Exercises:** 3 hours
- **Total:** 12 hours

> **Tip:** Set up rustfmt + Clippy + rust-analyzer on day one — they teach you idiomatic Rust faster than any tutorial. Treat `clippy-lints` as a reference you return to whenever Clippy flags something you do not recognize.

---

**Next:** [Section 25: Advanced Topics →](/25-advanced-topics/): PhantomData, Pin/Unpin, how async works internally, const generics, GATs, and the type-system frontier.
