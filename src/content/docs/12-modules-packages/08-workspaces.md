---
title: "Cargo Workspaces: Monorepos in Rust"
description: "A Cargo workspace is Rust's built-in monorepo: many crates share one Cargo.lock and target/, depend by path, and inherit versions, no Turborepo or Nx required."
---

A **Cargo workspace** lets several related crates live in one repository, share a single `Cargo.lock` and `target/` directory, and be built/tested together. It is Rust's answer to the npm workspaces / Lerna / Nx / Turborepo monorepo you have probably wrestled with in the JavaScript world, but it is built into the toolchain, with no extra dependency.

---

## Quick Overview

When a project outgrows a single crate (say a shared core library, a command-line interface, and a web server that all reuse the same domain types), you reach for a **workspace**. You declare it with a `[workspace]` table in a top-level `Cargo.toml` that lists its **members**. Every member is a normal crate with its own `Cargo.toml`, but they share one resolved dependency graph (`Cargo.lock`), one build cache (`target/`), and can depend on each other by **path**. For a TypeScript/JavaScript developer this is exactly the monorepo pattern (`"workspaces": ["packages/*"]` in a root `package.json`), and the payoff is the same: atomic cross-package changes, one install/build, and no version-skew between internal packages.

---

## TypeScript/JavaScript Example

A typical npm-workspaces monorepo for a task tracker has a private root `package.json` that lists the packages, plus one `package.json` per package. Local packages depend on each other by name:

```json
// package.json  (the repo root — private, not published)
{
  "name": "taskhub",
  "private": true,
  "workspaces": ["packages/*"]
}
```

```json
// packages/core/package.json  — the shared library
{
  "name": "@taskhub/core",
  "version": "0.3.0",
  "main": "index.js"
}
```

```json
// packages/cli/package.json  — depends on the local @taskhub/core
{
  "name": "@taskhub/cli",
  "version": "0.3.0",
  "dependencies": { "@taskhub/core": "0.3.0" }
}
```

A single `npm install` at the root resolves everything into **one** root `package-lock.json` and symlinks the local packages into the root `node_modules/`:

```text
$ npm install
$ find . -name package-lock.json
./package-lock.json                 # one lockfile at the root
$ ls -la node_modules/@taskhub
core -> ../../packages/core         # local packages symlinked, not copied
cli  -> ../../packages/cli
```

That single shared lockfile and the symlinked local packages are the two defining traits of a JavaScript monorepo. Cargo workspaces give you both, by design, not by convention.

---

## Rust Equivalent

The same project as a Cargo workspace. The repo root holds a `Cargo.toml` with **no `[package]`**, only a `[workspace]` table (this is called a **virtual manifest**). Each member crate lives in its own directory with its own manifest.

The layout:

```text
taskhub/
├── Cargo.toml          # the workspace root (virtual manifest — no [package])
├── Cargo.lock          # ONE shared lockfile for the whole workspace
├── target/             # ONE shared build cache for the whole workspace
└── crates/
    ├── core/           # taskhub-core: the shared library crate
    │   ├── Cargo.toml
    │   └── src/lib.rs
    └── cli/            # taskhub-cli: a binary that depends on core
        ├── Cargo.toml
        └── src/main.rs
```

The root manifest:

```toml
# taskhub/Cargo.toml  — the workspace root
[workspace]
resolver = "3"
members = ["crates/core", "crates/cli"]

# Fields every member can inherit, declared once.
[workspace.package]
version = "0.3.0"
edition = "2024"
license = "MIT OR Apache-2.0"
authors = ["Ada Zeybek <me@zeybek.dev>"]

# Dependency versions declared once, used by every member.
[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1"
taskhub-core = { path = "crates/core" }
```

The shared library member:

```toml
# crates/core/Cargo.toml
[package]
name = "taskhub-core"
version.workspace = true        # inherit from [workspace.package]
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
serde = { workspace = true }    # inherit the version + features from the root
```

```rust
// crates/core/src/lib.rs
use serde::{Deserialize, Serialize};

/// A single task tracked by the hub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u32,
    pub title: String,
    pub done: bool,
}

impl Task {
    pub fn new(id: u32, title: impl Into<String>) -> Self {
        Task { id, title: title.into(), done: false }
    }
}

/// Count how many tasks are still open.
pub fn open_count(tasks: &[Task]) -> usize {
    tasks.iter().filter(|t| !t.done).count()
}
```

The binary member that depends on the library:

```toml
# crates/cli/Cargo.toml
[package]
name = "taskhub-cli"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
taskhub-core = { workspace = true }   # the local path dep, declared at the root
serde_json = { workspace = true }
anyhow = { workspace = true }
```

```rust
// crates/cli/src/main.rs
use anyhow::{Context, Result};
use taskhub_core::{open_count, Task};

fn main() -> Result<()> {
    let mut tasks = vec![
        Task::new(1, "Write workspaces guide"),
        Task::new(2, "Verify every snippet"),
    ];
    tasks[1].done = true;

    let json = serde_json::to_string_pretty(&tasks)
        .context("failed to serialize tasks")?;
    println!("{json}");
    println!("{} task(s) still open", open_count(&tasks));
    Ok(())
}
```

Running the CLI member from the workspace root produces real output:

```text
$ cargo run -p taskhub-cli --quiet
[
  {
    "id": 1,
    "title": "Write workspaces guide",
    "done": false
  },
  {
    "id": 2,
    "title": "Verify every snippet",
    "done": true
  }
]
1 task(s) still open
```

> **Note:** The package is named `taskhub-core` (with a hyphen) but the crate you `use` in code is `taskhub_core` (with an underscore). Cargo maps hyphens in package names to underscores in the language. This mirrors how `serde_json` the crate corresponds to the `serde_json` import name.

---

## Detailed Explanation

### The `[workspace]` table and members

```toml
[workspace]
resolver = "3"
members = ["crates/core", "crates/cli"]
```

- **`members`** is a list of directory paths (relative to the root). Each must contain a crate with its own `Cargo.toml`. Glob patterns work too: `members = ["crates/*"]` picks up every crate under `crates/`.
- **`resolver = "3"`** opts into the version-3 feature resolver, which is the default for edition-2024 crates. In a workspace you set it once at the root because the resolver is workspace-wide; you cannot let individual members disagree. (Resolver 2 fixed long-standing issues where a build-time feature would leak into your runtime build; resolver 3 is its successor and the current default.)

A root `Cargo.toml` that contains **only** `[workspace]` (no `[package]`) is a **virtual manifest**. The root is not itself a crate; it is just the coordinator. You can also make the root *both* a package and a workspace by including a `[package]` table alongside `[workspace]` (a "root package"), but for monorepos the virtual-manifest style keeps responsibilities clean.

### One `Cargo.lock`, one `target/`

This is the heart of why workspaces exist. Run any build command anywhere in the tree and Cargo produces exactly one lockfile and one build directory at the root:

```text
$ cargo build --quiet
$ find . -name Cargo.lock -not -path './target/*'
./Cargo.lock          # exactly one, at the workspace root
```

Because every member resolves against the *same* `Cargo.lock`, two members can never accidentally pull in two different versions of `serde`. This is the version-skew problem that plagues JavaScript monorepos (where each package can hoist or nest a different copy in `node_modules/`); a Cargo workspace makes it structurally impossible. The shared `target/` also means a dependency compiled for one member is reused by every other member: no rebuild per package.

### Path dependencies between members

`crates/cli` depends on `crates/core` through a **path dependency**:

```toml
# declared once at the root:
[workspace.dependencies]
taskhub-core = { path = "crates/core" }
```

```toml
# used by the cli member:
[dependencies]
taskhub-core = { workspace = true }
```

A path dependency points at a sibling crate on disk rather than a registry. This is the direct analogue of a local package symlinked into `node_modules/` by npm workspaces. Because it is a path (not a version range), edits to `taskhub-core` are picked up immediately by `taskhub-cli`: no publish, no `npm link`, no rebuild step you have to remember.

### Workspace inheritance: `[workspace.package]` and `[workspace.dependencies]`

Two tables let you declare shared settings once and inherit them with `.workspace = true`:

- **`[workspace.package]`** holds package metadata (`version`, `edition`, `license`, `authors`, `repository`, `rust-version`, …) that members opt into with `version.workspace = true`, `edition.workspace = true`, and so on. Bump the version once at the root and every member moves together.
- **`[workspace.dependencies]`** centralizes dependency *versions and features*. A member writes `serde = { workspace = true }` and inherits the `version = "1.0"` and `features = ["derive"]` from the root. A member can still add *extra* features on top: `serde = { workspace = true, features = ["rc"] }`.

This solves a real JavaScript-monorepo pain point: keeping the version of a third-party dependency identical across every package. In npm you reach for tooling (syncpack, Nx constraints) to enforce it; in Cargo it is a first-class manifest feature.

### Workspace-wide lints

You can also share lint configuration. Declare lints once at the root and have members opt in:

```toml
# root Cargo.toml
[workspace.lints.clippy]
unwrap_used = "warn"
```

```toml
# each member Cargo.toml
[lints]
workspace = true
```

With that in place, running Clippy on any member that calls `.unwrap()` surfaces the shared rule (real output, trimmed to the lint note):

```text
warning: used `unwrap()` on an `Option` value
  = note: if this value is `None`, it will panic
  = help: consider using `expect()` to provide a better panic message
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unwrap_used
```

This is the Rust equivalent of a single root `.eslintrc` that every package extends.

---

## Key Differences

| Concept                       | npm workspaces (JavaScript)                         | Cargo workspaces (Rust)                                  |
| ----------------------------- | --------------------------------------------------- | -------------------------------------------------------- |
| Declared in                   | `"workspaces"` array in root `package.json`         | `[workspace]` table in root `Cargo.toml`                 |
| Root is a package?            | Yes (usually `"private": true`)                     | Often a **virtual manifest** (no `[package]` at all)     |
| Local package linkage         | Symlink into `node_modules/`                        | **Path dependency** (`{ path = "..." }`)                 |
| Lockfile                      | One root `package-lock.json`                         | One root `Cargo.lock` (enforced, not optional)           |
| Build/output cache            | Per-package, plus hoisting heuristics               | One shared `target/` for the whole workspace             |
| Version skew of shared deps   | Possible (different copies can be nested)           | Impossible — single resolved graph                       |
| Shared dependency versions    | External tooling (syncpack, Nx)                     | Built-in `[workspace.dependencies]`                      |
| Shared metadata               | Manual or tooling                                   | Built-in `[workspace.package]` inheritance               |
| Run a task in one package     | `npm run build -w @taskhub/cli`                     | `cargo build -p taskhub-cli`                             |
| Run a task everywhere         | `npm run build --workspaces`                        | `cargo build --workspace`                                |
| Task orchestration            | Turborepo / Nx / Lerna for caching + ordering       | Cargo computes the build DAG itself; no extra tool       |

The biggest conceptual shift: **a JavaScript monorepo is an opt-in pattern layered on top of npm with help from extra tooling, whereas a Cargo workspace is a native concept the compiler and build system understand directly.** There is no Rust equivalent of Turborepo because Cargo already knows the dependency graph between your crates and builds them in the right order with shared caching out of the box.

> **Tip:** Selecting members on the command line uses `-p` / `--package` (one crate) or `--workspace` / `--all` (every member). The flags mirror npm's `-w <name>` and `--workspaces`.

---

## Common Pitfalls

### Pitfall 1: Putting `[profile.*]` in a member crate

Profiles (`[profile.release]`, etc.) are **workspace-wide** and may only live in the root manifest. Add one to a member and Cargo ignores it with a warning. Adding `[profile.release]` to `crates/core/Cargo.toml` and building produces this real warning:

```text
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /path/to/taskhub/crates/core/Cargo.toml
workspace: /path/to/taskhub/Cargo.toml
```

**Fix:** move every `[profile.*]` table to the root `Cargo.toml`. The same rule applies to the `[patch]` and `[replace]` tables; they are workspace-global.

### Pitfall 2: Inheriting a dependency that the root never declared

If a member writes `regex = { workspace = true }` but the root `[workspace.dependencies]` has no `regex` entry, the workspace fails to load. The real error:

```text
error: failed to load manifest for workspace member `/path/to/taskhub/crates/cli`
referenced by workspace at `/path/to/taskhub/Cargo.toml`

Caused by:
  failed to parse manifest at `/path/to/taskhub/crates/cli/Cargo.toml`

Caused by:
  error inheriting `regex` from workspace root manifest's `workspace.dependencies.regex`

Caused by:
  `dependency.regex` was not found in `workspace.dependencies`
```

**Fix:** add `regex = "1"` to `[workspace.dependencies]` at the root first, then inherit it in the member with `regex = { workspace = true }`.

### Pitfall 3: Listing a member that does not exist yet

The root manifest validates *every* listed member before any command runs. If `members = ["crates/core", "crates/cli"]` but `crates/cli/` has no `Cargo.toml` yet, **every** cargo command in the workspace fails:

```text
failed to load manifest for workspace member `/path/to/taskhub/crates/cli`
referenced by workspace at `/path/to/taskhub/Cargo.toml`

Caused by:
  failed to read `/path/to/taskhub/crates/cli/Cargo.toml`

Caused by:
  No such file or directory (os error 2)
```

**Fix:** create the member crate before adding it to `members`, or use a glob (`members = ["crates/*"]`) so only existing directories are picked up. When scaffolding with `cargo new`, create the crate first; Cargo writes the new package without a `[workspace]` table because it detects it sits inside an existing workspace.

### Pitfall 4: `cargo run` in a workspace with several binaries

In a single-crate project `cargo run` "just works." In a workspace with more than one runnable binary it cannot guess which one you mean:

```text
$ cargo run
error: `cargo run` could not determine which binary to run. Use the `--bin` option to specify a binary, or the `default-run` manifest key.
available binaries: taskhub-cli, taskhub-web
```

**Fix:** disambiguate with `cargo run -p taskhub-cli`, or set `default-members = ["crates/cli"]` in the root `[workspace]` so bare `cargo run`/`cargo test` target your usual crate.

### Pitfall 5: Expecting `cargo new mypkg` to "add to the workspace" loudly

Coming from `npm init -w packages/foo`, you might expect explicit confirmation. `cargo new crates/foo` inside a workspace simply creates the crate and relies on your `members` list (or a glob) to include it — it does not print an "added to workspace" notice, and it deliberately omits a `[workspace]` table from the new child. If you used an explicit `members = [...]` list, remember to append the new path yourself.

---

## Best Practices

- **Use a virtual manifest for monorepos.** A root `Cargo.toml` with only `[workspace]` keeps the root from accidentally becoming a publishable crate and makes the "this directory coordinates crates" intent obvious.
- **Centralize versions in `[workspace.dependencies]`.** Declare every third-party crate's version once at the root and inherit it everywhere with `{ workspace = true }`. This is the single most valuable workspace feature for avoiding version drift.
- **Inherit shared metadata via `[workspace.package]`.** Put `version`, `edition`, `license`, and `rust-version` at the root and use `.workspace = true` in members so a single edit moves the whole workspace.
- **Prefer glob members (`crates/*`) for fast-growing repos**, and an explicit list when you want tight control over what builds.
- **Set `default-members`** to the crate(s) you run most so bare `cargo run`/`cargo test` do the obvious thing.
- **Keep `[profile.*]`, `[patch]`, and `[replace]` at the root only.** They are workspace-global by definition.
- **Commit the single root `Cargo.lock`.** For a workspace that ships applications/binaries this is the right call (reproducible builds); a workspace that publishes only libraries follows the usual library lockfile convention. See [Cargo.toml](/12-modules-packages/04-cargo/).
- **Reach for a workspace when crates share code or ship together:** a CLI plus its library, a server plus shared domain types, a proc-macro crate plus the crate that uses it. Do not split a single cohesive crate into a workspace prematurely.

> **Warning:** All members compile against one feature-unified dependency graph. If member A enables a heavy feature of a shared dependency, member B is built with that feature too (resolver 3 mitigates *build-vs-normal* leakage, not enabling-across-members). Keep heavyweight optional features behind crates that not every member depends on.

---

## Real-World Example

A production-flavored three-crate workspace: a shared `taskhub-core` library, a `taskhub-cli` binary, and a `taskhub-web` binary stub. Because there are two binaries, `default-members` points bare commands at the CLI. Everything below was compiled and run as-is.

```toml
# taskhub/Cargo.toml  — workspace root (virtual manifest)
[workspace]
resolver = "3"
members = ["crates/core", "crates/cli", "crates/web"]
default-members = ["crates/cli"]

[workspace.package]
version = "0.3.0"
edition = "2024"
license = "MIT OR Apache-2.0"
authors = ["Ada Zeybek <me@zeybek.dev>"]

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1"
taskhub-core = { path = "crates/core" }
```

Build the entire workspace at once:

```text
$ cargo build --workspace
   Compiling taskhub-cli v0.3.0 (/path/to/taskhub/crates/cli)
   Compiling taskhub-web v0.3.0 (/path/to/taskhub/crates/web)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.92s
```

Test every member in one shot:

```text
$ cargo test --workspace
   Compiling taskhub-core v0.3.0 (/path/to/taskhub/crates/core)
   Compiling taskhub-cli v0.3.0 (/path/to/taskhub/crates/cli)
   Compiling taskhub-web v0.3.0 (/path/to/taskhub/crates/web)
     Running unittests src/lib.rs (target/debug/deps/taskhub_core-...)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running unittests src/main.rs (target/debug/deps/taskhub_cli-...)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running unittests src/main.rs (target/debug/deps/taskhub_web-...)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Inspect the unified dependency graph with `cargo tree`. Note that `taskhub-core` appears as a path dependency of the CLI, exactly like a symlinked local package would in `node_modules/` (real output, trimmed):

```text
$ cargo tree -p taskhub-cli
taskhub-cli v0.3.0 (/path/to/taskhub/crates/cli)
├── anyhow v1.0.102
├── serde_json v1.0.150
│   ├── itoa v1.0.18
│   ├── memchr v2.8.1
│   ├── serde_core v1.0.228
│   └── zmij v1.0.21
└── taskhub-core v0.3.0 (/path/to/taskhub/crates/core)
    └── serde v1.0.228
        ├── serde_core v1.0.228
        └── serde_derive v1.0.228 (proc-macro)
```

Because the entire graph resolves once into the root `Cargo.lock`, `taskhub-cli` and `taskhub-web` are guaranteed to use the *same* `serde` build: the version-skew problem simply cannot occur.

> **Tip:** `cargo metadata --format-version 1` reports `workspace_root` and `workspace_members` as machine-readable JSON: the analogue of parsing a root `package.json`'s `workspaces` field with `jq`, but with the full resolved graph included.

---

## Further Reading

### Official documentation

- [The Cargo Book — Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html) — the authoritative reference for `[workspace]`, members, default-members, and inheritance.
- [The Cargo Book — Dependency inheritance (`workspace = true`)](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#inheriting-a-dependency-from-a-workspace)
- [The Cargo Book — The `[lints]` and `[workspace.lints]` tables](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section)
- [The Cargo Book — Feature resolver versions](https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2) — background on `resolver = "3"`.

### Cross-links in this guide

- [Cargo.toml: the manifest, lockfile & profiles](/12-modules-packages/04-cargo/) — `[package]`, `Cargo.lock`, and the `[profile.*]` tables a workspace shares.
- [Specifying dependencies](/12-modules-packages/06-dependencies/) — path/git deps and SemVer grammar that `[workspace.dependencies]` builds on.
- [Dev & build dependencies](/12-modules-packages/07-dev-dependencies/) — `[dev-dependencies]` can also be inherited from the workspace.
- [Feature flags](/12-modules-packages/09-feature-flags/) — how features unify across workspace members and why `resolver = "3"` matters.
- [Cargo commands](/12-modules-packages/05-cargo-commands/) — `-p`/`--package`, `--workspace`, `cargo new`, `cargo tree`.
- [Publishing to crates.io](/12-modules-packages/11-publishing/) — publishing individual members of a workspace.
- The in-language module system that lives *inside* each crate: [Modules](/12-modules-packages/00-modules/) · [The module tree & paths](/12-modules-packages/01-module-tree/) · [`use` & re-exports](/12-modules-packages/02-use-keyword/) · [`pub` visibility](/12-modules-packages/03-pub-visibility/).
- Foundations: [Why Rust](/00-introduction/) · [Understanding Cargo (intro)](/01-getting-started/03-cargo-basics/) · [Basics](/02-basics/).
- Next up after modules: [Testing](/13-testing/) — `cargo test --workspace` runs every member's tests together.

---

## Exercises

### Exercise 1: Turn two crates into a workspace

**Difficulty:** Beginner

**Objective:** Create a virtual-manifest workspace whose binary depends on its sibling library by path.

**Instructions:** Make a `taskhub/` directory with a root `Cargo.toml` (no `[package]`) listing two members, `crates/core` (a library) and `crates/cli` (a binary). Have `cli` call a function exported from `core`. Run `cargo run -p taskhub-cli` and confirm there is exactly one `Cargo.lock` at the root.

<details>
<summary>Solution</summary>

```toml
# taskhub/Cargo.toml
[workspace]
resolver = "3"
members = ["crates/core", "crates/cli"]
```

```toml
# crates/core/Cargo.toml
[package]
name = "taskhub-core"
version = "0.1.0"
edition = "2024"
```

```rust
// crates/core/src/lib.rs
pub fn greeting(who: &str) -> String {
    format!("hello from {who}")
}
```

```toml
# crates/cli/Cargo.toml
[package]
name = "taskhub-cli"
version = "0.1.0"
edition = "2024"

[dependencies]
taskhub-core = { path = "../core" }
```

```rust
// crates/cli/src/main.rs
fn main() {
    println!("{}", taskhub_core::greeting("workspace"));
}
```

Build and verify the single lockfile:

```text
$ cargo run -p taskhub-cli --quiet
hello from workspace
$ find . -name Cargo.lock -not -path './target/*'
./Cargo.lock
```

One `Cargo.lock` at the root is the proof that both members share a single resolved dependency graph.

</details>

### Exercise 2: Centralize versions with `[workspace.dependencies]`

**Difficulty:** Intermediate

**Objective:** Declare a third-party crate's version once at the root and inherit it in two members, plus inherit shared package metadata.

**Instructions:** Extend the workspace so the root declares `serde` (with the `derive` feature) and the shared `version`/`edition` in `[workspace.package]`. Both members inherit them with `{ workspace = true }` / `.workspace = true`. Make `core` derive `Serialize` on a struct and `cli` serialize it. Fill in the `/* ??? */` placeholders.

```toml
# taskhub/Cargo.toml
[workspace]
resolver = "3"
members = ["crates/core", "crates/cli"]

[workspace.package]
version = "0.2.0"
edition = "2024"

[workspace.dependencies]
serde = /* ??? */                 # version "1.0" with the "derive" feature
serde_json = "1.0"
taskhub-core = { path = "crates/core" }
```

```toml
# crates/core/Cargo.toml
[package]
name = "taskhub-core"
version.workspace = true
edition.workspace = true

[dependencies]
serde = /* ??? */                 # inherit from the workspace
```

```rust
// crates/core/src/lib.rs
use serde::Serialize;

#[derive(/* ??? */)]
pub struct Task {
    pub id: u32,
    pub title: String,
}
```

<details>
<summary>Solution</summary>

```toml
# taskhub/Cargo.toml
[workspace]
resolver = "3"
members = ["crates/core", "crates/cli"]

[workspace.package]
version = "0.2.0"
edition = "2024"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
taskhub-core = { path = "crates/core" }
```

```toml
# crates/core/Cargo.toml
[package]
name = "taskhub-core"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { workspace = true }
```

```rust
// crates/core/src/lib.rs
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Task {
    pub id: u32,
    pub title: String,
}
```

```toml
# crates/cli/Cargo.toml
[package]
name = "taskhub-cli"
version.workspace = true
edition.workspace = true

[dependencies]
taskhub-core = { workspace = true }
serde_json = { workspace = true }
```

```rust
// crates/cli/src/main.rs
use taskhub_core::Task;

fn main() {
    let task = Task { id: 1, title: "Centralize versions".into() };
    println!("{}", serde_json::to_string(&task).unwrap());
}
```

Real output:

```text
$ cargo run -p taskhub-cli --quiet
{"id":1,"title":"Centralize versions"}
```

The win: `serde`'s version and the `derive` feature are declared in exactly one place. Add a third member that needs `serde` and it inherits the same line: no chance of one crate using `serde` 1.0 and another a different copy.

</details>

### Exercise 3: Share lints and run the whole workspace's tests

**Difficulty:** Advanced

**Objective:** Configure a workspace-wide Clippy lint that every member opts into, add a unit test to the library, and run all members' tests together.

**Instructions:** Add a `[workspace.lints.clippy]` table at the root that sets `unwrap_used = "warn"`, and have each member opt in with `[lints] workspace = true`. Add a `#[test]` to the library crate. Run `cargo test --workspace`, then run `cargo clippy --workspace` and observe the shared lint fire on a deliberate `.unwrap()`.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml (root)
[workspace]
resolver = "3"
members = ["crates/mathlib", "crates/calc"]

[workspace.package]
version = "0.1.0"
edition = "2024"

[workspace.dependencies]
mathlib = { path = "crates/mathlib" }

[workspace.lints.clippy]
unwrap_used = "warn"
```

```toml
# crates/mathlib/Cargo.toml
[package]
name = "mathlib"
version.workspace = true
edition.workspace = true

[lints]
workspace = true
```

```rust
// crates/mathlib/src/lib.rs
pub fn add(a: i64, b: i64) -> i64 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds() {
        assert_eq!(add(2, 3), 5);
    }
}
```

```toml
# crates/calc/Cargo.toml
[package]
name = "calc"
version.workspace = true
edition.workspace = true

[dependencies]
mathlib = { workspace = true }

[lints]
workspace = true
```

```rust
// crates/calc/src/main.rs — deliberately uses unwrap() to trip the shared lint
fn first_word(s: &str) -> Option<&str> {
    s.split_whitespace().next()
}

fn main() {
    let _ = mathlib::add(2, 3);
    let w = first_word("hello world");
    println!("{}", w.unwrap());
}
```

Run the whole workspace's tests (real output, compile lines trimmed):

```text
$ cargo test --workspace
     Running unittests src/main.rs (target/debug/deps/calc-...)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (target/debug/deps/mathlib-...)

running 1 test
test tests::adds ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests mathlib

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Run Clippy and watch the shared lint fire on the `.unwrap()` in `calc` (real output, trimmed):

```text
$ cargo clippy --workspace
warning: used `unwrap()` on an `Option` value
 --> crates/calc/src/main.rs:8:20
  |
8 |     println!("{}", w.unwrap());
  |                    ^^^^^^^^^^
  |
  = note: if this value is `None`, it will panic
  = help: consider using `expect()` to provide a better panic message
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unwrap_used
  = note: requested on the command line with `-W clippy::unwrap-used`
```

The single `[workspace.lints.clippy]` table is the Rust analogue of one root `.eslintrc` that every package extends: change the rule once and it applies across the monorepo.

</details>
