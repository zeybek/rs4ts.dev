---
title: "Cargo Commands"
description: "One cargo CLI replaces npm, tsc, eslint, prettier, and jest: build, run, check, test, fmt, clippy, doc, add, plus new vs init, identical in every Rust project."
---

Cargo is the command you will type hundreds of times a day. It is Rust's build tool, test runner, formatter front-end, linter front-end, documentation generator, and package manager, all behind one consistent CLI. This file is your reference for the everyday commands and the difference between `cargo new` and `cargo init`.

---

## Quick Overview

Where a Node.js project juggles `npm`, `tsc`, `jest`, `prettier`, `eslint`, and `typedoc`, Rust uses a single tool: **Cargo**. The same handful of subcommands work in every Rust project with zero configuration, because the conventions (where source lives, how tests are written, how the binary is named) are baked into the language and toolchain. This page focuses on the commands themselves; the [Cargo.toml manifest](/12-modules-packages/04-cargo/) and [dependency management](/12-modules-packages/06-dependencies/) live in sibling files.

---

## TypeScript/JavaScript Example

A typical Node.js/TypeScript project wires up its workflow through `package.json` scripts, each delegating to a different tool you installed separately:

```json
{
  "name": "user-service",
  "version": "1.0.0",
  "scripts": {
    "build": "tsc",
    "start": "node dist/index.js",
    "dev": "tsx src/index.ts",
    "test": "vitest run",
    "lint": "eslint .",
    "format": "prettier --write .",
    "docs": "typedoc"
  },
  "devDependencies": {
    "typescript": "^5.6.0",
    "tsx": "^4.19.0",
    "vitest": "^2.1.0",
    "eslint": "^9.0.0",
    "prettier": "^3.3.0",
    "typedoc": "^0.26.0"
  }
}
```

A new contributor has to read `package.json` to learn that "run the app" is `npm run dev`, "test" is `npm test`, and "format" is `npm run format`. Every project invents its own script names.

```bash
npm install            # Install dependencies
npm run dev            # Run in watch mode
npm test               # Run tests
npm run lint           # Lint
npm run format         # Format
npm run build          # Compile to dist/
```

---

## Rust Equivalent

In Rust, the same workflow uses standard Cargo subcommands that are identical across every project. There is no `scripts` table to define or memorize:

```bash
# Create a new binary project (the equivalent of `npm init` + scaffolding)
cargo new user-service
cd user-service

# Add dependencies (writes to Cargo.toml, like `npm install <pkg>`)
cargo add serde --features derive
cargo add --dev assert_cmd        # a dev-only dependency

cargo check       # Type-check without producing a binary (fastest feedback loop)
cargo run         # Build + run the binary
cargo test        # Run all tests (unit, integration, and doctests)
cargo clippy      # Lint (the eslint equivalent)
cargo fmt         # Format (the prettier equivalent)
cargo doc --open  # Generate HTML docs and open them (the typedoc equivalent)
cargo build --release  # Optimized production build
```

> **Tip:** Because these commands are universal, you can clone any Rust project and immediately know that `cargo run` runs it, `cargo test` tests it, and `cargo build --release` produces the optimized artifact. No per-project script archaeology required.

---

## Detailed Explanation

### `cargo new` — scaffold a brand-new project

`cargo new <name>` creates a new directory, lays out the standard structure, and initializes a Git repository. Here is the real output and resulting tree:

```bash
$ cargo new hello
    Creating binary (application) `hello` package
note: see more `Cargo.toml` keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html
```

```text
hello
├── .git/
├── .gitignore        # contains just `/target`
├── Cargo.toml        # the manifest (like package.json)
└── src/
    └── main.rs       # entry point with a `fn main` that prints "Hello, world!"
```

By default you get a **binary** (executable) crate. Pass `--lib` for a **library** crate, which produces `src/lib.rs` instead of `src/main.rs`:

```bash
cargo new my_app          # binary  -> src/main.rs
cargo new my_lib --lib    # library -> src/lib.rs
```

The `edition` field in the generated `Cargo.toml` is filled in for you with the newest edition your toolchain supports. On a current stable toolchain that is `"2024"` (the latest stable edition). You never pick an edition by hand for a new project.

> **Note:** The crate name in `Cargo.toml` follows Rust's `snake_case` convention. If you write `cargo new my-service`, the directory is `my-service` but the crate name becomes `my_service` (hyphens are not valid in Rust identifiers). This is unlike npm, where `@scope/my-service` is a perfectly normal package name.

### `cargo init` — adopt an existing directory

`cargo init` does the same scaffolding as `cargo new`, but **in the current (already-existing) directory** instead of creating a new one. This is the analog of running `npm init` inside a folder you already have:

```bash
$ mkdir existing_app && cd existing_app
$ cargo init
    Creating binary (application) package
note: see more `Cargo.toml` keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html
```

```text
existing_app/
├── .git/
├── .gitignore
├── Cargo.toml
└── src/
    └── main.rs
```

`cargo init` is what you reach for when you have a directory with, say, a `README.md` and a `.git` already (perhaps from `git clone`) and you want to turn it into a Cargo project without nesting another folder inside it. Like `cargo new`, it accepts `--lib`.

> **Note:** `cargo init` will **not** clobber an existing `src/main.rs` or `Cargo.toml`; it only adds what is missing. If the directory is already a Git repo, it skips Git initialization rather than re-running it.

### `cargo check` — fastest feedback

`cargo check` runs the full compiler front-end — parsing, type checking, borrow checking — but stops **before** code generation and linking. It catches every type error and borrow error without spending time producing a runnable binary, so it is dramatically faster than a full build. Use it as your inner loop; it is the closest thing to `tsc --noEmit`.

```bash
$ cargo check
    Checking greeter v0.1.0 (/path/to/greeter)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
```

### `cargo build` — compile to an artifact

`cargo build` compiles your crate and all its dependencies into an actual binary (or `.rlib` for a library). Debug builds land in `target/debug/`; release builds in `target/release/`.

```bash
$ cargo build
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s

$ cargo build --release
   Compiling args_demo v0.1.0 (/path/to/args_demo)
    Finished `release` profile [optimized] target(s) in 0.17s
```

The default **debug** profile compiles fast and includes debug info but does little optimization. The `--release` profile turns on optimizations (`opt-level = 3` by default), producing a much faster binary at the cost of a slower compile. Use debug while developing, `--release` for benchmarks and production. Profile configuration is covered in [Cargo.toml](/12-modules-packages/04-cargo/).

### `cargo run` — build and execute

`cargo run` builds the binary (if needed) and then runs it. If nothing changed since the last build, it skips straight to running:

```bash
$ cargo run
   Compiling args_demo v0.1.0 (/path/to/args_demo)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s
     Running `target/debug/args_demo`
No arguments given.
```

To pass arguments **to your program** (rather than to Cargo), put them after a `--` separator. Everything before `--` is a Cargo flag; everything after goes to your binary's `std::env::args()`:

```bash
$ cargo run -- alice bob
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.00s
     Running `target/debug/args_demo alice bob`
You passed 2 argument(s): ["alice", "bob"]
```

This `--` convention matches `npm run <script> -- <args>` in spirit. The `--release` flag goes *before* `--`, because it is a Cargo flag:

```bash
cargo run --release -- --config prod.toml
```

If your project has more than one binary, Cargo cannot guess which to run and tells you so:

```bash
$ cargo run
error: `cargo run` could not determine which binary to run. Use the `--bin` option to specify a binary, or the `default-run` manifest key.
available binaries: seed, toolbox
```

Pick one with `--bin`:

```bash
$ cargo run --bin seed
   Compiling toolbox v0.1.0 (/path/to/toolbox)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.14s
     Running `target/debug/seed`
seeding database...
```

### `cargo test` — run every kind of test

`cargo test` compiles your code in test mode and runs all `#[test]` functions, integration tests in `tests/`, and **doctests** (the runnable examples in your `///` doc comments). No test runner to install or configure. For a library crate with three unit tests:

```bash
$ cargo test
   Compiling calc v0.1.0 (/path/to/calc)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.16s
     Running unittests src/lib.rs (target/debug/deps/calc-9612d1c07b9ca2bd)

running 3 tests
test tests::test_add ... ok
test tests::test_multiply ... ok
test tests::test_add_negative ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests calc

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

You can run a subset by passing a substring filter; every test whose full path contains it runs:

```bash
$ cargo test add
     Running unittests src/lib.rs (target/debug/deps/calc-9612d1c07b9ca2bd)

running 2 tests
test tests::test_add_negative ... ok
test tests::test_add ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s
```

Notice the `1 filtered out`: `test_multiply` did not match the `add` filter. To see `println!` output from passing tests (Cargo captures it by default), pass `-- --nocapture` (everything after `--` goes to the test harness, not Cargo). Testing gets a whole section of its own; see [Section 13: Testing](/13-testing/).

### `cargo fmt` — format code

`cargo fmt` runs `rustfmt` over your whole crate, rewriting files in place to the canonical Rust style. It is the `prettier --write` equivalent, but the style is community-standard and essentially non-configurable, which ends bikeshedding. To check formatting without modifying files (for CI), use `--check`:

```bash
$ cargo fmt --check
Diff in /path/to/fmt_demo/src/main.rs:1:
 fn main() {
-let name="world";
+    let name = "world";
     println!("Hello, {name}!");
 }
```

`cargo fmt --check` exits with a non-zero status (`1`) when reformatting is needed, which is exactly what you want in a CI gate.

### `cargo clippy` — lint

`cargo clippy` is Rust's linter, the `eslint` equivalent, with hundreds of built-in lints that catch bugs, non-idiomatic code, and performance footguns. Given this code:

```rust playground
fn main() {
    let ready = true;
    if ready == true {
        println!("Starting up...");
    }
}
```

Clippy produces a real, actionable warning:

```bash
$ cargo clippy
    Checking lint_demo v0.1.0 (/path/to/lint_demo)
warning: equality checks against true are unnecessary
 --> src/main.rs:3:8
  |
3 |     if ready == true {
  |        ^^^^^^^^^^^^^ help: try simplifying it as shown: `ready`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#bool_comparison
  = note: `#[warn(clippy::bool_comparison)]` on by default

warning: `lint_demo` (bin "lint_demo") generated 1 warning (run `cargo clippy --fix --bin "lint_demo"` to apply 1 suggestion)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s
```

In CI you typically promote warnings to errors with `-- -D warnings`, which makes any lint fail the build:

```bash
$ cargo clippy -- -D warnings
...
  = note: `-D clippy::bool-comparison` implied by `-D warnings`
  = help: to override `-D warnings` add `#[allow(clippy::bool_comparison)]`

error: could not compile `lint_demo` (bin "lint_demo") due to 1 previous error
```

Many lints carry machine-applicable fixes; `cargo clippy --fix` rewrites your code to apply them automatically.

### `cargo doc` — generate documentation

`cargo doc` builds HTML documentation from your `///` doc comments. `--open` launches it in your browser; `--no-deps` skips documenting your dependencies (faster, and usually what you want locally):

```bash
$ cargo doc --no-deps
 Documenting docgen v0.1.0 (/path/to/docgen)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.40s
   Generated /path/to/docgen/target/doc/docgen/index.html
```

The generated docs include cross-linked types, source links, and rendered examples: the same machinery that produces [docs.rs](https://docs.rs). Importantly, the examples in your doc comments are *tested* by `cargo test`, so your documentation cannot silently rot.

### `cargo add` — manage dependencies from the CLI

`cargo add <crate>` resolves the latest compatible version, writes it into `Cargo.toml`, and updates `Cargo.lock`. It is the `npm install <pkg>` equivalent and has been **built into Cargo since 1.62**: you do *not* need to install `cargo-edit` first, despite what older tutorials claim.

```bash
$ cargo add serde --features derive
      Adding serde v1.0.228 to dependencies
             Features:
             + derive
             + serde_derive
             + std
             - alloc
             - rc
             - unstable
     Locking 7 packages to latest Rust 1.96.0 compatible versions
```

The resulting `Cargo.toml`:

```toml
[dependencies]
serde = { version = "1.0.228", features = ["derive"] }
```

Add to `[dev-dependencies]` with `--dev`:

```bash
$ cargo add --dev criterion
      Adding criterion v0.8.2 to dev-dependencies
```

The full story of version requirements, feature flags, and git/path dependencies lives in [Dependencies](/12-modules-packages/06-dependencies/) and [Dev Dependencies](/12-modules-packages/07-dev-dependencies/).

### Discovering commands

`cargo --list` prints every available subcommand, including aliases (`b` = `build`, `c` = `check`, `r` = `run`, `d` = `doc`) and any third-party subcommands you have installed:

```bash
$ cargo --list
Installed Commands:
    add                  Add dependencies to a Cargo.toml manifest file
    b                    alias: build
    bench                Execute all benchmarks of a local package
    build                Compile a local package and all of its dependencies
    c                    alias: check
    check                Check a local package and all of its dependencies for errors
    clean                Remove artifacts that cargo has generated in the past
    clippy               Checks a package to catch common mistakes and improve your Rust code.
    ...
    new                  Create a new cargo package at <path>
    ...
    run                  Run a binary or example of the local package
```

---

## Key Differences

| Task | Node.js / TypeScript | Rust (Cargo) |
| --- | --- | --- |
| Scaffold new project | `npm init -y` + manual `src/`, `tsconfig.json` | `cargo new <name>` (one command) |
| Initialize in current dir | `npm init` | `cargo init` |
| Install a dependency | `npm install <pkg>` | `cargo add <crate>` |
| Install a dev dependency | `npm install -D <pkg>` | `cargo add --dev <crate>` |
| Type-check only | `tsc --noEmit` | `cargo check` |
| Build | `tsc` / `webpack` / `vite build` | `cargo build` (debug) / `cargo build --release` |
| Run | `node dist/index.js` / `tsx src/index.ts` | `cargo run` |
| Test | `jest` / `vitest` (separate install) | `cargo test` (built in) |
| Format | `prettier --write .` (separate install) | `cargo fmt` |
| Lint | `eslint .` (separate install) | `cargo clippy` |
| Docs | `typedoc` (separate install) | `cargo doc` |
| Pass args to program | `npm start -- <args>` | `cargo run -- <args>` |

### Conventions over configuration

The biggest mental shift is that Cargo commands are **universal**. In Node.js, "test" might be `jest`, `vitest`, `mocha`, `npm test`, or `npm run test:unit`. It depends entirely on what each project's `package.json` declares. In Rust, `cargo test` always means the same thing because the entry point (`src/main.rs` or `src/lib.rs`), the test attribute (`#[test]`), and the build profiles are part of the toolchain, not per-project config.

### `check` vs `build` — a tool TypeScript doesn't separate

`cargo check` has no everyday TypeScript equivalent that most developers use. It skips code generation entirely, so it is the right command for "does my code compile?" while you iterate. Reach for `cargo build` only when you actually need the artifact, and `cargo run` when you want to execute it. This three-way split (`check` / `build` / `run`) is finer-grained than the typical `tsc` → `node` two-step.

### One lockfile philosophy difference

`cargo add` and `cargo build` write `Cargo.lock`, analogous to `package-lock.json`. The convention differs from npm's "always commit it": you commit `Cargo.lock` for **binaries** (reproducible deployments) but conventionally omit it for **libraries** (so downstream users resolve their own compatible versions). See [Cargo.toml](/12-modules-packages/04-cargo/) for details.

---

## Common Pitfalls

### Pitfall 1: Forgetting the `--` separator

A TypeScript developer used to `node dist/index.js --port 8080` may write:

```bash
cargo run --port 8080
```

Cargo interprets `--port` as one of *its own* flags and errors out (`cargo run` has no `--port` option). The fix is to separate your program's arguments with `--`:

```bash
cargo run -- --port 8080
```

Everything after `--` is handed to your binary untouched.

### Pitfall 2: Running `cargo run` in a library crate

If you create a project with `cargo new my_lib --lib`, there is no binary to run, and `cargo run` fails:

```bash
$ cargo run
error: a bin target must be available for `cargo run`
```

Libraries are *consumed* by other crates and tested with `cargo test`; they are not executed directly. If you want a runnable entry point, add a `src/main.rs` (or a `src/bin/*.rs`) so the crate also produces a binary.

### Pitfall 3: Assuming `cargo new` runs in the current directory

`cargo new my_app` *creates a new subdirectory* called `my_app`. If you have already `cd`'d into the folder you want to use, `cargo new my_app` nests `./my_app/my_app/`. Use `cargo init` to scaffold *in place*:

```bash
mkdir my_app && cd my_app
cargo init          # initializes the current directory
# cargo new my_app  # would create my_app/my_app
```

### Pitfall 4: Expecting `cargo build` to be the inner loop

Coming from `tsc`, you might run `cargo build` constantly. It works, but it spends time on code generation and linking you do not need while merely checking for errors. `cargo check` is several times faster for that purpose. Save `cargo build`/`cargo run` for when you actually need to execute the result.

### Pitfall 5: Thinking `cargo add` needs `cargo-edit`

Older blog posts and Stack Overflow answers say "first `cargo install cargo-edit`, then `cargo add`." That has not been necessary since Cargo 1.62 (June 2022). `cargo add` and `cargo remove` ship with Cargo. (`cargo-edit` still provides extras like `cargo upgrade`, but not the basic add/remove.)

---

## Best Practices

- **Use `cargo check` as your default inner loop.** Switch to `cargo run` only when you need to see the program execute. Pair it with an editor running rust-analyzer for instant feedback.
- **Gate CI on formatting and lints.** Run `cargo fmt --check` and `cargo clippy -- -D warnings` in CI so that unformatted or lint-dirty code cannot merge. Both exit non-zero on failure, which CI systems treat as a failed step.
- **Build `--release` only when it matters.** Release builds are slow to compile. Use them for production artifacts, benchmarks, and performance testing, not for routine development.
- **Prefer `cargo add` over hand-editing `Cargo.toml`** for adding dependencies: it picks a current version, sorts features, and updates `Cargo.lock` in one step. Hand-edit only when you need something unusual.
- **Learn the one-letter aliases** (`cargo c`, `cargo b`, `cargo r`, `cargo t` is *not* built in but `cargo test` is short anyway) to shave keystrokes, but write the full names in scripts and docs for clarity.
- **Reach for `cargo new --lib`** when starting reusable code, and `cargo new` (binary) for applications. You can always add a `src/lib.rs` later to make a binary crate also expose a library.

> **Tip:** A clean pre-commit/CI sequence for a Rust project is: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`. It mirrors the `prettier --check && eslint && vitest run` you'd run in a TypeScript repo, but with zero tooling to install.

---

## Real-World Example

Here is the complete lifecycle of a small library crate, `slugify`, from scaffolding through the full quality-gate sequence you'd run locally and in CI. Every command below produces the real output shown.

**1. Scaffold a library crate:**

```bash
$ cargo new slugify --lib
    Creating library `slugify` package
note: see more `Cargo.toml` keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html
```

**2. Write the library (`src/lib.rs`):**

```rust
//! Turn arbitrary titles into URL-friendly slugs.

/// Converts a title into a lowercase, hyphen-separated slug.
///
/// # Examples
///
/// ```
/// use slugify::slugify;
/// assert_eq!(slugify("Hello, World!"), "hello-world");
/// ```
pub fn slugify(title: &str) -> String {
    title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace_and_punctuation() {
        assert_eq!(slugify("  Rust   for  TS devs! "), "rust-for-ts-devs");
    }

    #[test]
    fn lowercases() {
        assert_eq!(slugify("CamelCase"), "camelcase");
    }
}
```

**3. Run the quality gate** (`fmt`, `clippy`, `test`), the same commands you'd put in CI:

```bash
$ cargo fmt --check          # exits 0: already formatted
$ cargo clippy -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.08s
$ cargo test
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Note the **two** test groups: the first `2 passed` is the unit tests in the `tests` module; the second `1 passed` is the **doctest** from the `# Examples` block in the doc comment. The example in your docs is verified to be correct, for free.

**4. Generate the docs:**

```bash
$ cargo doc --no-deps --open   # builds HTML and opens it in the browser
```

For a production-flavored CI configuration, this maps directly onto a GitHub Actions job:

```yaml
# .github/workflows/ci.yml
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo fmt --check
      - run: cargo clippy -- -D warnings
      - run: cargo test
```

This is the same set of standard commands working unchanged, which is exactly the point: no per-project script names to invent.

---

## Further Reading

### Official Documentation

- [The Cargo Book](https://doc.rust-lang.org/cargo/) — the complete reference
- [Cargo Commands Index](https://doc.rust-lang.org/cargo/commands/index.html) — every subcommand documented
- [`cargo new` / `cargo init`](https://doc.rust-lang.org/cargo/commands/cargo-init.html)
- [Clippy lint list](https://rust-lang.github.io/rust-clippy/master/index.html) — searchable catalog of every lint
- [rustfmt configuration](https://rust-lang.github.io/rustfmt/) — the formatter's (rarely needed) options

### Related Sections in This Guide

- [Understanding Cargo](/01-getting-started/03-cargo-basics/) — the gentle introduction in Section 01
- [Hello World](/01-getting-started/02-hello-world/) — your first `cargo run`
- [Cargo.toml](/12-modules-packages/04-cargo/) — the manifest, profiles, and `Cargo.lock`
- [Dependencies](/12-modules-packages/06-dependencies/) — semver requirements, features, git/path deps
- [Dev Dependencies](/12-modules-packages/07-dev-dependencies/) — `[dev-dependencies]` and `[build-dependencies]`
- [Workspaces](/12-modules-packages/08-workspaces/) — running Cargo commands across a monorepo
- [Feature Flags](/12-modules-packages/09-feature-flags/) — building with `--features`
- [Publishing](/12-modules-packages/11-publishing/) — `cargo publish` and crates.io
- [Section 13: Testing](/13-testing/) — everything `cargo test` can do

---

## Exercises

### Exercise 1: Scaffold and run

**Difficulty:** Easy

**Objective:** Get comfortable with `cargo new` and the `--` argument separator.

**Instructions:**

1. Create a new binary project named `greeter` with one command.
2. Edit `src/main.rs` so it reads command-line arguments and prints `Hello, <name>!` for each name passed, or `Hello, world!` if none are given.
3. Run it with no arguments, then run it with `cargo run -- Ada Grace` and confirm both names print.

<details>
<summary>Solution</summary>

```bash
cargo new greeter
cd greeter
```

```rust playground
// src/main.rs
use std::env;

fn main() {
    let names: Vec<String> = env::args().skip(1).collect();
    if names.is_empty() {
        println!("Hello, world!");
    } else {
        for name in names {
            println!("Hello, {name}!");
        }
    }
}
```

```bash
$ cargo run
     Running `target/debug/greeter`
Hello, world!

$ cargo run -- Ada Grace
     Running `target/debug/greeter Ada Grace`
Hello, Ada!
Hello, Grace!
```

`env::args()` yields the program name as the first element, so `.skip(1)` drops it. The `--` separator is what makes `Ada` and `Grace` reach your program instead of being parsed as Cargo flags.

</details>

### Exercise 2: `init` an existing directory and add a dependency

**Difficulty:** Medium

**Objective:** Practice `cargo init` (vs `cargo new`) and `cargo add`.

**Instructions:**

1. Create an empty directory `dice` and `cd` into it.
2. Turn it into a Cargo binary project *in place* (do not create a nested folder).
3. Add the `rand` crate as a dependency from the command line.
4. Make `main` print a random dice roll between 1 and 6 inclusive, then run it.

<details>
<summary>Solution</summary>

```bash
mkdir dice
cd dice
cargo init                # initializes the CURRENT directory
cargo add rand            # writes rand to [dependencies] and updates Cargo.lock
```

```rust playground
// src/main.rs
use rand::RngExt;

fn main() {
    let roll = rand::rng().random_range(1..=6);
    println!("You rolled a {roll}");
}
```

```bash
$ cargo run
   Compiling dice v0.1.0 (/path/to/dice)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.9s
     Running `target/debug/dice`
You rolled a 4
```

> **Note:** This uses current rand 0.10 idioms: `rand::rng()` (not the old 0.8 `thread_rng()`) and `random_range()` via the `RngExt` trait (not the old `gen_range()`). In rand 0.10 the range-sampling method lives on `RngExt`, so you import `rand::RngExt` rather than `rand::Rng`. Using `cargo init` instead of `cargo new dice` keeps everything in the directory you already created rather than nesting `dice/dice/`.

</details>

### Exercise 3: A full quality gate on a library

**Difficulty:** Medium

**Objective:** Run the same `fmt` / `clippy` / `test` sequence used in CI, and use the `cargo test` filter.

**Instructions:**

1. Create a *library* crate named `mathkit`.
2. Add two public functions, `is_even(n: i64) -> bool` and `factorial(n: u64) -> u64`, each with a unit test, plus one doctest example on `factorial`.
3. Run `cargo fmt --check`, then `cargo clippy -- -D warnings`, then `cargo test`. Fix anything the first two report.
4. Run only the factorial-related tests using a `cargo test` filter, and confirm the even-number test is reported as filtered out.

<details>
<summary>Solution</summary>

```bash
cargo new mathkit --lib
cd mathkit
```

```rust
// src/lib.rs

/// Returns `true` if `n` is even.
pub fn is_even(n: i64) -> bool {
    n % 2 == 0
}

/// Computes `n!` (factorial).
///
/// # Examples
///
/// ```
/// use mathkit::factorial;
/// assert_eq!(factorial(5), 120);
/// ```
pub fn factorial(n: u64) -> u64 {
    (1..=n).product()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_even() {
        assert!(is_even(4));
        assert!(!is_even(7));
    }

    #[test]
    fn test_factorial() {
        assert_eq!(factorial(0), 1);
        assert_eq!(factorial(4), 24);
    }
}
```

Run the gate:

```bash
$ cargo fmt --check                 # exits 0 if formatted
$ cargo clippy -- -D warnings       # exits 0 if no lints
$ cargo test
running 2 tests
test tests::test_is_even ... ok
test tests::test_factorial ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
...
   Doc-tests mathkit
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Filter to just the factorial tests:

```bash
$ cargo test factorial
running 1 test
test tests::test_factorial ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s
```

The `1 filtered out` confirms `test_is_even` was skipped because its name does not contain `factorial`. The doctest on `factorial` runs as part of `cargo test`, so the example in the documentation is guaranteed correct.

</details>
