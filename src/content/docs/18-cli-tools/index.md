---
title: "Rust CLI Tools with clap"
sidebar:
  label: "Overview"
description: "Build Rust CLIs that map onto the Node toolbox you know: clap for parsing, ratatui and indicatif for UI, portable paths, env config, and single-binary distribution."
---

Command-line tools are where Rust shines for working TypeScript/JavaScript developers: a single self-contained native binary, sub-millisecond startup, and no Node runtime to install on the target machine. This section maps the Node CLI toolbox you already know (`commander`/`yargs` for parsing, `chalk` for color, `ora`/`cli-progress` for feedback, `blessed`/`ink` for full-screen UIs, `process.env`, and `path`) onto their idiomatic Rust counterparts. You will build real CLIs with **clap**, render terminal UIs with **ratatui**, draw progress with **indicatif**, handle paths and files portably, read environment configuration, navigate cross-platform pitfalls, and ship the result as prebuilt binaries.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. The recorded dependency lines are clap 4.6, ratatui 0.30, indicatif 0.18, owo-colors/anstream/console, and dotenvy 0.15; page-level external-crate examples follow the verification scope documented in the policy.

---

## What You'll Learn

- How to parse arguments, flags, and options with clap — both the builder API and the idiomatic `#[derive(Parser)]` API — with auto-generated `--help` and `--version`
- How to model git-like subcommands and nested commands with `#[derive(Subcommand)]`
- How to build interactive full-screen terminal UIs with ratatui's immediate-mode rendering model
- How to give long-running work a heartbeat with indicatif progress bars, spinners, and multi-progress displays
- How to produce colored output that automatically respects `NO_COLOR` and non-terminal output
- How to read, write, and stream files with `std::fs`, `BufReader`, and `BufWriter`
- How to manipulate paths portably with `Path`/`PathBuf` instead of string concatenation
- How to read and validate environment-variable configuration, including `.env` files
- How to handle cross-platform concerns: line endings, path separators, `cfg!(windows)`, and exit codes
- How to distribute a finished tool via `cargo install`, prebuilt binaries, and automated releases

---

## Topics

| Topic | Description |
| ----- | ----------- |
| [clap Basics (Builder API)](/18-cli-tools/00-clap-basics/) | CLI argument parsing with clap's builder API: args, flags, options, and help generation. |
| [clap Derive API](/18-cli-tools/01-clap-derive/) | The idiomatic `#[derive(Parser)]` approach: arg attributes, type conversion, and default values. |
| [Subcommands](/18-cli-tools/02-subcommands/) | Git-like subcommands with `#[derive(Subcommand)]`, including nested commands. |
| [Terminal UI with ratatui](/18-cli-tools/03-terminal-ui/) | Full-screen terminal UIs with ratatui: the immediate-mode model, widgets, and the event loop. |
| [Progress Bars](/18-cli-tools/04-progress-bars/) | Progress indicators with indicatif: bars, spinners, and multi-progress. |
| [Colored Output](/18-cli-tools/05-colored-output/) | Colored terminal output with owo-colors / console / anstream, and respecting `NO_COLOR`. |
| [File I/O](/18-cli-tools/06-file-io/) | File system operations with `std::fs`: read/write, `BufReader`/`BufWriter`, and reading lines. |
| [Path Handling](/18-cli-tools/07-path-handling/) | `Path`/`PathBuf` manipulation (join, extension, file name) and cross-platform paths vs Node's `path`. |
| [Environment Variables](/18-cli-tools/08-environment-vars/) | Environment variables with `std::env::var`, dotenvy, and config via env. |
| [Cross-Platform Considerations](/18-cli-tools/09-cross-platform/) | Cross-platform concerns: line endings, paths, `cfg!(windows)`, and exit codes. |
| [Distribution](/18-cli-tools/10-distribution/) | Distributing CLI tools: `cargo install`, prebuilt binaries, cargo-dist, and release profiles. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Choose between clap's builder and derive APIs and justify the choice
- Define a typed CLI surface — positional args, flags, options, defaults, and validation — as a single source of truth
- Structure a multi-command tool (`tool add`, `tool remove`, `tool config set ...`) with subcommands
- Build a keyboard-driven full-screen TUI and reason about its render-on-every-frame model
- Add progress feedback that behaves correctly when output is piped or running in CI
- Emit color that degrades gracefully and honors `NO_COLOR`
- Read and write files efficiently with buffered I/O and propagate errors with `?`
- Manipulate filesystem paths portably instead of concatenating strings
- Load configuration from the environment and `.env` files and validate it into typed structs
- Account for line endings, path separators, and exit-code conventions across Linux, macOS, and Windows
- Publish a tool as a single binary your users can install without a Rust toolchain

---

## Prerequisites

- [Section 08: Error Handling](/08-error-handling/): CLIs lean heavily on `Result`, the `?` operator, and `anyhow`/`thiserror` for reporting failures to the user.
- [Section 12: Modules and Packages](/12-modules-packages/): understanding crates, `Cargo.toml`, features, and binary vs library targets is essential for adding clap and shipping a binary.

A working knowledge of the earlier fundamentals ([ownership](/05-ownership/), [collections](/07-collections/), and [structs and enums](/06-data-structures/)) will also help.

---

## Estimated Time

Approximately **12 hours**, including reading, hands-on practice, and the per-topic exercises.

---

## Next

Continue to [Section 19: WebAssembly](/19-wasm/) to take Rust into the browser and beyond.
