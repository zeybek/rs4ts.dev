---
title: "Version and Verification Policy"
description: "How rs4ts.dev pins Rust, records crate versions, and verifies capstone projects and runnable documentation snippets without calling an old pin the latest stable release."
---

Rust and its crate ecosystem release frequently. This guide therefore separates three ideas that are easy to conflate:

- **The Rust edition** is the language compatibility boundary used by a crate. New projects in this guide use the **2024 edition**.
- **The repository toolchain** is the exact compiler used for reproducible local and CI checks. Its source of truth is [`rust-toolchain.toml`](https://github.com/zeybek/rs4ts.dev/blob/main/rust-toolchain.toml).
- **The latest stable Rust release** changes on Rust's release cadence. The guide does not hard-code that moving value across chapter prose.

At the time of this repository snapshot, the verification toolchain is pinned to **Rust 1.96.0**. That sentence describes a tested baseline, not a claim that 1.96.0 is still the newest stable release. Run `rustup update stable` in a separate project when you specifically want to try the newest compiler; use the repository pin when reproducing this guide's CI results.

## What CI verifies

The repository applies different checks to different kinds of examples:

1. **Six capstone crates** are checked with the pinned toolchain. CI runs rustfmt, Clippy with warnings denied, native tests, and the relevant `wasm32-unknown-unknown` checks.
2. **A deterministic subset of self-contained standard-library `rust playground` programs** is extracted from normal lesson sections and compiled as Rust 2024 code. Fragments, Common Pitfalls/Exercises sections, deliberately failing teaching examples, and snippets requiring external crates or multiple files are outside that automatic subset. CI enforces a minimum selected count so the gate cannot silently shrink to zero.
3. **Internal links and anchors** are checked by the production site build.
4. **External-crate snippets** state their required dependencies beside the example. They still require a focused Cargo project when edited; a green standard-library snippet job does not prove those integrations.

This scope is intentionally explicit. “Checked on the pinned toolchain” is a reproducible claim only for the named gate; “every example compiles on current stable” would be broader than the automated evidence.

## Crate versions

Chapter prose uses a major or compatible line when that is the meaningful API contract (`axum 0.8`, `serde 1`, `tokio 1`). Exact versions belong in the capstone `Cargo.lock` files or in a page only when an API changed across releases and the distinction matters to the lesson.

When updating the repository toolchain or a documented dependency:

1. change the single source of truth (`rust-toolchain.toml`, a capstone manifest/lockfile, or the page's dependency block);
2. run the focused examples affected by the API change;
3. run the documentation snippet checker and full site build;
4. describe historical measurements as “measured with” or “verified with,” never as permanently “current.”

That wording keeps old benchmark output reproducible without turning yesterday's tested version into today's release claim.
