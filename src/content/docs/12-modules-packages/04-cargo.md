---
title: "Cargo.toml: The Manifest, Dependencies, Lockfile, and Profiles"
description: "Cargo.toml is Rust's package.json, but one TOML file holds metadata, dependencies, and build tuning. Covers the manifest, the Cargo.lock twin, and [profile.*]."
---

`Cargo.toml` is Rust's answer to `package.json`. It declares **who your crate is** (`[package]`), **what it depends on** (`[dependencies]`), and **how it should be built** (`[profile.*]`). This page covers the manifest itself, the `Cargo.lock` file it generates, and build profiles.

---

## Quick Overview

Every Rust **crate** has a `Cargo.toml` manifest at its root, just as every Node project has a `package.json`. The big mental shift for a TypeScript/JavaScript developer is that Cargo folds build configuration, dependency declarations, and release-tuning knobs into **one TOML file** with no `scripts` section and no separate `tsconfig.json`; the standard `cargo` subcommands replace your npm scripts, and **build profiles** replace your bundler config. This page focuses on the manifest's `[package]` and `[dependencies]` tables, the auto-generated `Cargo.lock`, and `[profile.*]`; sibling pages cover the commands, the dependency-spec syntax, workspaces, and features in detail.

---

## TypeScript/JavaScript Example

A realistic `package.json` for a small CLI tool, plus its companion `tsconfig.json`:

```json
// package.json
{
  "name": "task-cli",
  "version": "0.2.1",
  "description": "A tiny task tracker CLI",
  "license": "MIT",
  "type": "module",
  "main": "dist/index.js",
  "bin": { "task-cli": "dist/index.js" },
  "engines": { "node": ">=22" },
  "scripts": {
    "build": "tsc",
    "start": "node dist/index.js",
    "dev": "tsx src/index.ts",
    "test": "vitest"
  },
  "dependencies": {
    "zod": "^3.23.0"
  },
  "devDependencies": {
    "typescript": "^5.5.0",
    "tsx": "^4.0.0",
    "vitest": "^2.0.0"
  }
}
```

```jsonc
// tsconfig.json — build/output configuration lives in a SEPARATE file
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "outDir": "dist",
    "strict": true
  }
}
```

The dependency tree is pinned in `package-lock.json` (generated, committed), and `node_modules/` is downloaded per-project.

---

## Rust Equivalent

The same project as a Rust crate. **One** file holds metadata, dependencies, *and* build tuning:

```toml
# Cargo.toml
[package]
name = "taskcli"
version = "0.2.1"
edition = "2024"
rust-version = "1.85"
authors = ["Ada Zeybek <me@zeybek.dev>"]
description = "A tiny task tracker CLI"
license = "MIT OR Apache-2.0"
repository = "https://github.com/ada/taskcli"
keywords = ["cli", "tasks", "productivity"]
categories = ["command-line-utilities"]

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1"

[dev-dependencies]
tempfile = "3"

# Build tuning lives RIGHT HERE — no separate tsconfig/bundler file.
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

A matching `src/main.rs` that uses those dependencies:

```rust
// src/main.rs
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Task {
    id: u32,
    title: String,
    done: bool,
}

fn load_tasks(json: &str) -> Result<Vec<Task>> {
    let tasks: Vec<Task> =
        serde_json::from_str(json).context("failed to parse tasks JSON")?;
    Ok(tasks)
}

fn main() -> Result<()> {
    let data = r#"[
        { "id": 1, "title": "Write Cargo guide", "done": false },
        { "id": 2, "title": "Verify snippets", "done": true }
    ]"#;

    let tasks = load_tasks(data)?;
    for task in &tasks {
        let mark = if task.done { "x" } else { " " };
        println!("[{mark}] #{} {}", task.id, task.title);
    }
    println!("{} task(s) loaded", tasks.len());
    Ok(())
}
```

Running it produces real output:

```text
$ cargo run --quiet
[ ] #1 Write Cargo guide
[x] #2 Verify snippets
2 task(s) loaded
```

> **Note:** The package `name` is `taskcli`, not `task-cli`. Cargo accepts hyphens in names but the *crate* identifier in code becomes `taskcli` (hyphens map to underscores). Many crates use hyphenated names like `serde_json` vs `serde-json`; the underscore form is what you `use` in code. We use a hyphen-free name here to keep things simple.

---

## Detailed Explanation

### `[package]` — your crate's identity card

This table is the direct analogue of the top-level fields in `package.json`. Field by field:

```toml
[package]
name = "taskcli"          # like package.json "name"
version = "0.2.1"         # like "version"; SemVer, and Cargo ENFORCES it
edition = "2024"          # NO package.json equivalent — see below
rust-version = "1.85"     # like "engines.node": minimum compiler (MSRV)
authors = ["Ada Zeybek <me@zeybek.dev>"]
description = "A tiny task tracker CLI"
license = "MIT OR Apache-2.0"     # an SPDX expression, not free text
repository = "https://github.com/ada/taskcli"
keywords = ["cli", "tasks", "productivity"]   # max 5, for crates.io search
categories = ["command-line-utilities"]        # from a fixed crates.io list
```

- **`name` + `version`** are the only strictly-required fields. `cargo new` fills them in.
- **`edition`** has no `package.json` counterpart. An **edition** (`2015`, `2018`, `2021`, `2024`) is an opt-in revision of the *language surface* — new keywords, lints, defaults — without breaking older code. It is **not** a compiler version. `cargo new` writes the newest edition your toolchain supports; on current stable that is `"2024"`, the latest stable edition. Crates of different editions link together fine.
- **`rust-version`** (the **MSRV**, Minimum Supported Rust Version) is the closest thing to `engines.node`. Unlike `engines`, which Node only *warns* about, Cargo will *refuse* to build if your toolchain is older than the stated `rust-version`.
- **`license`** is an SPDX expression. `"MIT OR Apache-2.0"` (the conventional Rust dual-license) means downstream users may pick either. This is metadata for crates.io, like `package.json`'s `"license"`.
- **`description`, `keywords`, `categories`, `repository`** are crates.io discoverability metadata, relevant only when you publish. See [Publishing to crates.io](/12-modules-packages/11-publishing/).

> **Note:** Notice there is **no `main` field**. Cargo finds your entry point by convention: `src/main.rs` for a binary, `src/lib.rs` for a library. There is also **no `scripts` table**: the standard `cargo build`/`run`/`test`/`fmt`/`clippy` subcommands replace npm scripts. See [Cargo Commands](/12-modules-packages/05-cargo-commands/).

### `[dependencies]` — what you pull in

The simplest form maps one-to-one onto a `package.json` `dependencies` entry:

```toml
[dependencies]
anyhow = "1"          # like "anyhow": "^1"  in package.json
serde_json = "1.0"    # like "serde_json": "^1.0"
```

A bare version string is a **caret requirement** by default, exactly like npm's leading `^`. `"1.0"` means `>=1.0.0, <2.0.0`. This trips up TypeScript developers who read `"1.0"` as "pinned to 1.0". It is *not* pinned. (For the full grammar — caret, tilde, `=` exact, wildcards, git/path sources — see [Specifying Dependencies](/12-modules-packages/06-dependencies/).)

When you need to enable optional **features** or other knobs, the value becomes an inline table:

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
```

This is roughly like installing a package *and* flipping on an opt-in capability. npm has no real equivalent; the closest analogy is a peer/optional dependency you choose to wire up. The `features` array turns on conditionally-compiled parts of the crate; `serde`'s `"derive"` feature is what makes `#[derive(Serialize)]` work. Forgetting it is the single most common Cargo mistake for newcomers (see Common Pitfalls). Features are covered fully in [Feature Flags and Conditional Compilation](/12-modules-packages/09-feature-flags/).

### `[dev-dependencies]` — test-and-build-only deps

```toml
[dev-dependencies]
tempfile = "3"
```

This is the exact analogue of `devDependencies`: crates used by tests, examples, and benchmarks but **not** compiled into your published library or shipped binary. Details and the related `[build-dependencies]` live in [Dev-Dependencies, Build-Dependencies, and Optional Dependencies](/12-modules-packages/07-dev-dependencies/).

### `Cargo.lock` — the generated lockfile

The first time you build, Cargo resolves every version requirement to a concrete version and records the full graph (with checksums) in `Cargo.lock`. This is `package-lock.json`'s twin. The top of a real one:

```toml
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 4

[[package]]
name = "anyhow"
version = "1.0.102"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "7f202df86484c868dbad7eaa557ef785d5c66295e41b460ef922eca0723b842c"

[[package]]
name = "serde"
version = "1.0.228"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "..."
```

Key facts:

- It is **TOML**, not JSON, and far more readable than `package-lock.json`.
- It is **auto-generated**; never hand-edit it. To change a locked version, run `cargo update` (see [Cargo Commands](/12-modules-packages/05-cargo-commands/)).
- **Commit it for applications/binaries; do not commit it for libraries.** Same rule as npm in spirit: an app wants byte-reproducible builds, while a library wants to be tested against the latest compatible versions of its dependents. `cargo new` writes the **same** `.gitignore` (just `/target`) for both binaries and libraries; neither variant ignores `Cargo.lock`. The convention is a decision **you** make: commit `Cargo.lock` for applications/binaries; for a library you publish, add `Cargo.lock` to `.gitignore` yourself.

> **Note:** Unlike `node_modules/`, resolved crates are **downloaded once** to a shared cache in `~/.cargo/registry/` and **compiled** into your project's `target/` directory. There is no per-project copy of source. That is why Rust projects do not carry a giant `node_modules/`-style folder, only the `target/` build cache, which `cargo clean` can wipe.

### `[profile.*]` — build tuning (the part with no `package.json` analogue)

This is where Cargo's "one file" philosophy diverges most sharply from the Node world. In TypeScript, compile/output settings live in `tsconfig.json` and bundling/minification in a bundler config (webpack/esbuild/vite). In Rust, all of that is **build profiles** right in `Cargo.toml`:

```toml
[profile.release]
opt-level = 3        # optimization level 0–3 (3 = max); "s"/"z" optimize for size
lto = true           # link-time optimization across crates (smaller, faster, slower to build)
codegen-units = 1    # 1 = best optimization, slowest compile; default 16 trades speed for parallelism
strip = true         # strip debug symbols from the binary (smaller file)
```

Cargo ships **four built-in profiles**, and you can override any field:

| Profile   | Used by                          | Default `opt-level` | Default `debug` |
| --------- | -------------------------------- | ------------------- | --------------- |
| `dev`     | `cargo build`, `cargo run`       | `0`                 | full            |
| `release` | `cargo build --release`          | `3`                 | none            |
| `test`    | `cargo test`                     | `0`                 | full            |
| `bench`   | `cargo bench`                    | `3`                 | none            |

You rarely set a `[profile.dev]`; the defaults (fast compile, no optimization) are what you want while iterating. You *do* tune `[profile.release]` for shipping. The effect is real and reproducible in *direction*: building the example above with the tuned `release` profile produces a noticeably **smaller** binary than the default `dev` profile, while incremental `dev` rebuilds finish **dramatically faster**:

```text
$ cargo build --release
    Finished `release` profile [optimized] target(s) in ...
# target/release/taskcli  ->  smaller binary (stripped + LTO)

$ cargo build
    Finished `dev` profile [unoptimized + debuginfo] target(s) in ...
# target/debug/taskcli    ->  larger binary (with debug info)
```

> **Note:** Exact byte counts and build times vary heavily by platform and toolchain, so they are omitted here. On one macOS arm64 / Rust 1.96 run the debug binary was roughly 2-3x the size of the stripped release binary. Build *times* are even more variable: a clean build is dominated by compiling dependencies (tens of seconds either way), while a cached rebuild of just this crate finishes in a fraction of a second for `dev` and a few seconds for `release` (where `lto` and `codegen-units = 1` deliberately trade compile speed for a tighter artifact).

The takeaway holds regardless of the exact numbers: the `dev` profile optimizes for fast iteration, the `release` profile for a smaller, faster output. That trade-off is exactly the dev-vs-prod distinction you manage with bundler modes in Node, but it is first-class and built in.

---

## Key Differences

| Concept                   | TypeScript/Node (`package.json`)        | Rust (`Cargo.toml`)                              |
| ------------------------- | --------------------------------------- | ------------------------------------------------ |
| Format                    | JSON                                    | TOML                                             |
| Build config location     | Separate `tsconfig.json` + bundler      | Same file (`[profile.*]`)                        |
| Task runner               | `scripts` table (`npm run x`)           | No scripts; standard `cargo` subcommands         |
| Entry point               | `"main"` / `"bin"` fields               | Convention: `src/main.rs` / `src/lib.rs`         |
| Default version semantics | `^` must be written explicitly          | Bare `"1.0"` is already caret (`^`)              |
| Language revisions        | None (TS version is the compiler)       | `edition` (opt-in, decoupled from compiler)      |
| Min runtime/compiler      | `engines` (advisory warning)            | `rust-version` / MSRV (build-time error)         |
| Lockfile                  | `package-lock.json` (JSON)              | `Cargo.lock` (TOML)                              |
| Dependency storage        | `node_modules/` per project             | Shared `~/.cargo/registry/` + compiled `target/` |
| Optional capabilities     | (no real equivalent)                    | `features` per dependency                        |

The deepest difference: **`Cargo.toml` is declarative and converges on conventions, while `package.json` is imperative and configurable.** You will write far less configuration in Rust because the toolchain assumes sensible defaults (entry points, test discovery, formatting, linting) that you would have to wire up manually in a Node project.

---

## Common Pitfalls

### Pitfall 1: Forgetting a dependency's feature flag

This is *the* classic Cargo mistake for newcomers. You add `serde`, write `#[derive(Serialize)]`, and it does not compile, because the derive macro lives behind the `derive` feature, which is off by default.

```toml
# Cargo.toml — missing the "derive" feature
[dependencies]
serde = "1.0"
serde_json = "1.0"
```

```rust
use serde::Serialize; // imports the TRAIT, not the derive macro

#[derive(Serialize)] // does not compile
struct Task {
    id: u32,
}

fn main() {
    let t = Task { id: 1 };
    println!("{}", serde_json::to_string(&t).unwrap());
}
```

The real error from `cargo build`:

```text
error: cannot find derive macro `Serialize` in this scope
 --> src/main.rs:3:10
  |
3 | #[derive(Serialize)]
  |          ^^^^^^^^^
  |
note: `Serialize` is imported here, but it is only a trait, without a derive macro
 --> src/main.rs:1:5
  |
1 | use serde::Serialize;
  |     ^^^^^^^^^^^^^^^^

error[E0277]: the trait bound `Task: serde::Serialize` is not satisfied
```

**Fix:** enable the feature — `serde = { version = "1.0", features = ["derive"] }` — or run `cargo add serde --features derive`. See [Feature Flags and Conditional Compilation](/12-modules-packages/09-feature-flags/).

### Pitfall 2: Reading a bare version as "pinned"

```toml
[dependencies]
tokio = "1.45.0"   # NOT pinned — this is ^1.45.0 == >=1.45.0, <2.0.0
```

Coming from `package.json`, where `"1.45.0"` (no `^`) means *exactly* `1.45.0`, you will assume Cargo pins it too. It does not: a bare string is a caret range. To pin exactly, prefix with `=`:

```toml
tokio = "=1.45.0"  # exactly 1.45.0
```

The `Cargo.lock` still pins the *resolved* version for reproducibility, but the *requirement* in `Cargo.toml` stays a range. Full grammar in [Specifying Dependencies](/12-modules-packages/06-dependencies/).

### Pitfall 3: Hand-editing `Cargo.lock`

`Cargo.lock` starts with `# It is not intended for manual editing.` for a reason. Bumping a version there by hand will be overwritten or cause a checksum mismatch. To change a locked dependency, edit the *requirement* in `Cargo.toml` and/or run `cargo update`.

### Pitfall 4: Expecting an `engines`-style warning, getting a hard error

If you set `rust-version = "1.85"` and build on an older toolchain, Cargo does not warn-and-continue the way `npm` does for `engines.node`. It errors out before compiling. That is usually what you want, but it surprises people who treat MSRV as advisory.

### Pitfall 5: Putting build settings in the wrong place

There is no `tsconfig.json` to reach for. If you want optimizations, debug symbols, or smaller binaries, those go in `[profile.*]` inside `Cargo.toml` — not in a separate config file, and not as command-line flags you have to remember every time.

---

## Best Practices

- **Let `cargo new` / `cargo add` write the manifest.** `cargo add serde --features derive` edits `[dependencies]` correctly and picks the latest version; you rarely need to type version strings by hand. See [Cargo Commands](/12-modules-packages/05-cargo-commands/) and [Specifying Dependencies](/12-modules-packages/06-dependencies/).
- **Set `rust-version` (MSRV) on anything you publish or share.** It documents the minimum compiler and turns "works on my machine" into a checkable contract.
- **Commit `Cargo.lock` for binaries/applications; omit it for libraries.** This mirrors the npm convention and is the single most-asked lockfile question.
- **Tune `[profile.release]` deliberately, not by reflex.** `opt-level = 3` is already the release default. `lto = true` + `codegen-units = 1` give smaller, faster binaries at the cost of compile time: great for CI release builds, painful for local iteration. `strip = true` shrinks the binary by dropping symbols.
- **Keep build tuning in the manifest, not in shell aliases.** A profile is reproducible and shared; a forgotten `--flag` is not.
- **Use a real SPDX `license` expression** (`"MIT OR Apache-2.0"` is the Rust default convention), not prose, so tooling and crates.io can parse it.
- **Don't over-specify versions.** Bare caret requirements (`"1"`, `"1.0"`) are idiomatic; reach for `=` exact pins only when you genuinely need them.

---

## Real-World Example

A production-flavored manifest for a CLI that reads/writes JSON, with separated dev dependencies and a tuned release profile. Both the manifest and the program below were compiled and run as-is.

```toml
# Cargo.toml
[package]
name = "taskcli"
version = "0.2.1"
edition = "2024"
rust-version = "1.85"
description = "A tiny task tracker CLI"
license = "MIT OR Apache-2.0"
repository = "https://github.com/ada/taskcli"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1"

[dev-dependencies]
tempfile = "3"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

```rust
// src/main.rs
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Task {
    id: u32,
    title: String,
    done: bool,
}

/// Parse a JSON array of tasks, attaching context on failure.
fn load_tasks(json: &str) -> Result<Vec<Task>> {
    let tasks: Vec<Task> =
        serde_json::from_str(json).context("failed to parse tasks JSON")?;
    Ok(tasks)
}

fn main() -> Result<()> {
    let data = r#"[
        { "id": 1, "title": "Write Cargo guide", "done": false },
        { "id": 2, "title": "Verify snippets", "done": true }
    ]"#;

    let tasks = load_tasks(data)?;
    for task in &tasks {
        let mark = if task.done { "x" } else { " " };
        println!("[{mark}] #{} {}", task.id, task.title);
    }
    println!("{} task(s) loaded", tasks.len());
    Ok(())
}
```

Real `cargo run` output:

```text
[ ] #1 Write Cargo guide
[x] #2 Verify snippets
2 task(s) loaded
```

You can inspect the resolved dependency graph that Cargo recorded in `Cargo.lock` with the built-in `cargo tree` (real output, trimmed):

```text
$ cargo tree
taskcli v0.2.1 (/path/to/taskcli)
├── anyhow v1.0.102
├── serde v1.0.228
│   ├── serde_core v1.0.228
│   └── serde_derive v1.0.228 (proc-macro)
│       ├── proc-macro2 v1.0.106
│       ├── quote v1.0.45
│       └── syn v2.0.117
└── serde_json v1.0.150
    ├── itoa v1.0.18
    ├── memchr v2.8.1
    ├── serde_core v1.0.228
    └── zmij v1.0.21
```

That tree — three direct dependencies fanning out to a handful of transitive ones — is the Rust counterpart to running `npm ls`. The `(proc-macro)` tag marks crates that run at compile time to generate code (here, the machinery behind `#[derive(Serialize)]`).

> **Tip:** Run `cargo metadata --format-version 1` to get the manifest and resolved graph as machine-readable JSON, handy for scripts, the way you might parse `package.json` with `jq`.

---

## Further Reading

### Official documentation

- [The Cargo Book — The Manifest Format](https://doc.rust-lang.org/cargo/reference/manifest.html): every `[package]` field.
- [The Cargo Book — Specifying Dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
- [The Cargo Book — Cargo.lock vs Cargo.toml](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html)
- [The Cargo Book — Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)
- [The Rust Edition Guide](https://doc.rust-lang.org/edition-guide/): what editions are and why `2024`.

### Cross-links in this guide

- [Modules: ES modules → `mod`](/12-modules-packages/00-modules/): the in-language module system that lives *inside* a crate.
- [The module tree & paths](/12-modules-packages/01-module-tree/)
- [`use` & re-exports](/12-modules-packages/02-use-keyword/) and [`pub` visibility](/12-modules-packages/03-pub-visibility/)
- [Cargo commands](/12-modules-packages/05-cargo-commands/) — `build`, `run`, `add`, `update`, and friends.
- [Specifying dependencies](/12-modules-packages/06-dependencies/): SemVer grammar, git/path deps, features-on-deps.
- [Dev & build dependencies](/12-modules-packages/07-dev-dependencies/)
- [Workspaces](/12-modules-packages/08-workspaces/) — multi-crate monorepos and a shared `Cargo.lock`.
- [Feature flags](/12-modules-packages/09-feature-flags/): `[features]` and `#[cfg(feature = "...")]`.
- [Build scripts](/12-modules-packages/10-build-scripts/) and [Publishing to crates.io](/12-modules-packages/11-publishing/).
- Foundations: [Why Rust](/00-introduction/) · [Understanding Cargo (intro)](/01-getting-started/03-cargo-basics/) · [Basics](/02-basics/).
- Next up after modules: [Testing](/13-testing/) — where `[dev-dependencies]` and the `test` profile come into play.

---

## Exercises

### Exercise 1: From `package.json` to `Cargo.toml`

**Difficulty:** Beginner

**Objective:** Translate a Node manifest into a Cargo manifest.

**Instructions:** Given this `package.json`, write the equivalent `[package]` table. Pick the appropriate fields and a valid SPDX `license`. Ignore `scripts` (Cargo has no equivalent).

```json
{
  "name": "url-shortener",
  "version": "1.4.0",
  "description": "Shorten and expand URLs",
  "license": "MIT",
  "engines": { "node": ">=22" }
}
```

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "url-shortener"   # hyphens are allowed in the package name
version = "1.4.0"
edition = "2024"          # cargo new fills in the newest edition
rust-version = "1.85"     # the analogue of engines.node — a MINIMUM compiler
description = "Shorten and expand URLs"
license = "MIT"           # a valid SPDX expression

[dependencies]
```

Notes: there is no `main`/`scripts`/`type` to translate; those are handled by convention and by the `cargo` subcommands. `engines.node` maps conceptually to `rust-version` (MSRV), though one is advisory in Node and the other is enforced by Cargo. This manifest compiles with an empty `src/main.rs` containing `fn main() {}`.

</details>

### Exercise 2: Add a dependency with a feature and use it

**Difficulty:** Intermediate

**Objective:** Practice the `[dependencies]` inline-table form and the most common feature flag.

**Instructions:** Starting from a fresh `cargo new`, add `serde` (with the `derive` feature) and `serde_json`, then make this program compile and print the JSON. Fill in the `/* ??? */` placeholders.

```rust
use serde::Serialize;

#[derive(/* ??? */)]
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let p = Point { x: 3, y: 4 };
    let json = /* ??? */; // serialize p to a JSON string
    println!("{json}");
}
```

<details>
<summary>Solution</summary>

`Cargo.toml`:

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

(Equivalently: `cargo add serde --features derive` then `cargo add serde_json`.)

`src/main.rs`:

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let p = Point { x: 3, y: 4 };
    let json = serde_json::to_string(&p).unwrap();
    println!("{json}");
}
```

Real output:

```text
{"x":3,"y":4}
```

Without `features = ["derive"]`, this fails with `error: cannot find derive macro \`Serialize\` in this scope`, exactly Pitfall 1.

</details>

### Exercise 3: Tune a release profile for a small, fast binary

**Difficulty:** Advanced

**Objective:** Use `[profile.release]` to trade compile time for a smaller, optimized binary, and observe the difference.

**Instructions:** Take any binary crate (e.g. the `taskcli` example) and add a `[profile.release]` that maximizes optimization, enables link-time optimization, uses a single codegen unit, and strips symbols. Build with `cargo build` and `cargo build --release` and compare the file sizes of `target/debug/<name>` and `target/release/<name>`. Explain *why* you would not put these settings in `[profile.dev]`.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[profile.release]
opt-level = 3        # maximum optimization (the release default already)
lto = true           # link-time optimization across all crates
codegen-units = 1    # one unit -> best optimization, slowest/least-parallel compile
strip = true         # remove debug symbols from the final binary
```

Compare the builds:

```bash
cargo build
cargo build --release
ls -l target/debug/taskcli target/release/taskcli
```

With the `taskcli` example, the stripped+LTO release binary comes out **noticeably smaller** than the debug binary (on one macOS arm64 / Rust 1.96 run, roughly 2-3x smaller; your exact byte counts will differ). Build times are platform- and cache-dependent: a clean build of either profile is dominated by compiling dependencies, and a cached rebuild of just this crate finishes far faster for `dev` than for the optimization-heavy `release`.

Why not in `[profile.dev]`? `dev` is your edit-compile-run loop. `lto = true` and `codegen-units = 1` disable parallel codegen and cross-crate optimization passes, which can multiply local compile times for no runtime benefit you care about while debugging. You want fast feedback in `dev` and a tuned artifact only in `release` (typically built once in CI). Stripping symbols in `dev` would also hurt debugging. Keep optimization aggression in `release`, keep `dev` fast.

</details>
