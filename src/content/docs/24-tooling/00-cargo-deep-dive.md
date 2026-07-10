---
title: "Cargo Deep Dive: Profiles, Aliases, Workspaces, and Metadata"
description: "A deeper Cargo tour for Node devs: build profiles, command aliases, workspaces, the [patch] table, offline builds, and cargo metadata, beyond package.json."
---

## Quick Overview

You already know Cargo as the `npm` of Rust: `cargo new`, `cargo add`, `cargo build`, `cargo test`. This page goes one layer deeper into the features that shape real projects: **build profiles** (your `tsconfig` + bundler tuning, but per-build-mode), **aliases** (your `package.json` `scripts`), **workspace tricks** (your monorepo / pnpm-workspace), the **`[patch]`** table (a sturdier `npm link` / `resolutions`), **offline** builds (`npm ci --offline`), and **`cargo metadata`** (a machine-readable `package-lock.json` you can actually query). Mastering these is the difference between fighting Cargo and having it disappear into the background.

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects the newest edition automatically. Everything here works on stable Cargo: no nightly flags, no `cargo-edit` install (`cargo add`/`cargo remove` have been built in since Cargo 1.62).

---

## TypeScript/JavaScript Example

In a Node monorepo you cobble together several files and tools to get the equivalent behavior. A realistic setup looks like this:

```jsonc
// package.json (root of a pnpm workspace) — scripts, overrides, and a "build mode"
{
  "name": "billing-monorepo",
  "private": true,
  "scripts": {
    "build": "tsc -b",
    "build:prod": "NODE_ENV=production tsc -b --sourceMap false",
    "lint": "eslint . --max-warnings 0",
    "ci": "npm run lint && npm test && npm run build"
  },
  // Force every transitive copy of `semver` to a forked/pinned version:
  "pnpm": {
    "overrides": {
      "semver": "npm:my-fork-of-semver@1.0.0"
    }
  }
}
```

```yaml
# pnpm-workspace.yaml — declares the monorepo members
packages:
  - "packages/*"
```

```jsonc
// tsconfig.json — "dev" vs "prod" toggles live here, split across files
{
  "compilerOptions": { "sourceMap": true, "incremental": true }
}
```

To inspect the resolved dependency graph programmatically you reach for `npm ls --json` or read `package-lock.json` by hand. To build without touching the network you run `npm ci --offline`. Notice how this knowledge is scattered: scripts in one file, overrides in another, workspace members in a third, build modes split across `tsconfig` variants and environment variables.

Cargo folds all of this into **one file format** (`Cargo.toml`) plus a small **`.cargo/config.toml`**, and exposes the resolved graph through a single stable command.

---

## Rust Equivalent

Here is the same set of concerns expressed in Cargo, in a workspace root `Cargo.toml`:

```toml
# Cargo.toml — workspace root (a "virtual manifest": no [package] of its own)
[workspace]
resolver = "3"                       # the 2024-edition default feature resolver
members = ["crates/*"]               # globs, like pnpm-workspace.yaml
default-members = ["crates/app"]     # what bare `cargo build`/`run` targets

# Fields shared by every member crate (DRY versioning):
[workspace.package]
version = "0.2.0"
edition = "2024"
license = "MIT"

# Dependencies declared once, referenced by members as `serde.workspace = true`:
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
anyhow = "1"
core-lib = { path = "crates/core-lib" }

# Build "modes" — far more granular than NODE_ENV:
[profile.release]
opt-level = 3
lto = "thin"          # link-time optimization across crate boundaries
codegen-units = 1     # fewer parallel codegen units = better optimization
strip = "symbols"     # drop symbols from the binary (smaller artifact)

# A custom profile: a release build that compiles faster (for staging/profiling):
[profile.release-fast]
inherits = "release"
lto = false
codegen-units = 16

# The `[patch]` table: override `semver` everywhere with a local fork
[patch.crates-io]
semver = { path = "../semver-fork" }
```

A member crate then stays tiny; it inherits versions, edition, and dependency specs from the workspace:

```toml
# crates/app/Cargo.toml
[package]
name = "app"
version.workspace = true       # inherit "0.2.0" from [workspace.package]
edition.workspace = true
license.workspace = true

[dependencies]
core-lib.workspace = true      # inherit the path dep
anyhow.workspace = true
```

And the per-project script aliases live in `.cargo/config.toml`:

```toml
# .cargo/config.toml — your package.json "scripts", but for Cargo subcommands
[alias]
b = "build"
c = "check"
t = "test"
rr = "run --release"
lint = "clippy --all-targets --all-features -- -D warnings"
ci = "test --workspace"
```

Building and running the default member of this workspace:

```text
$ cargo run
   Compiling core-lib v0.2.0 (/private/tmp/ws_probe/crates/core-lib)
   Compiling app v0.2.0 (/private/tmp/ws_probe/crates/app)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.03s
     Running `target/debug/app`
order 7 = $129.99
```

> **Tip:** A *virtual manifest* (a `Cargo.toml` with `[workspace]` but no `[package]`) is the cleanest monorepo root: it owns shared config and the member list, and holds no code of its own.

---

## Detailed Explanation

### Build profiles: the part with no Node analogue

A **profile** is a named set of compiler/linker knobs. Cargo ships four built-in profiles:

| Profile | Triggered by | Optimized | Debug info | Overflow checks |
| --- | --- | --- | --- | --- |
| `dev` | `cargo build`, `cargo run` | no (`opt-level = 0`) | yes | yes |
| `release` | `cargo build --release` | yes (`opt-level = 3`) | no | no |
| `test` | `cargo test` | inherits `dev` | yes | yes |
| `bench` | `cargo bench` | inherits `release` | no | no |

The `dev`/`release` split is the reason Rust feels "slow to compile but fast to run": by default `cargo run` skips optimization so the edit-compile loop stays quick, and only `--release` turns the optimizer all the way up. There is no single Node flag for this; the closest is hand-tuning `tsconfig` plus a separate minifier config.

The most impactful release knobs:

- **`opt-level`** — `0`–`3` (or `"s"`/`"z"` to optimize for size). `3` is the release default.
- **`lto`** — link-time optimization. `"thin"` is a great default (most of the speedup, a fraction of the link-time cost); `true`/`"fat"` is the most aggressive.
- **`codegen-units`** — how many parallel chunks the compiler splits a crate into. Fewer (down to `1`) optimizes harder but compiles slower.
- **`strip`** — `"symbols"` or `"debuginfo"` to shrink the final binary.
- **`panic`** — `"unwind"` (default) or `"abort"` (smaller, faster, but no `catch_unwind` and `#[should_panic]` tests can't run under it).

You can also optimize **just your dependencies** while keeping *your* code unoptimized for fast rebuilds, invaluable when a dependency (say, an image or crypto library) is painfully slow in debug:

```toml
# Build all dependencies with full optimization, but keep our own crate at dev speed.
[profile.dev.package."*"]
opt-level = 3
```

### Custom profiles with `inherits`

Beyond the four built-ins you can define your own. A custom profile **must** specify `inherits` to say which built-in it extends:

```toml
[profile.release-fast]
inherits = "release"
lto = false
codegen-units = 16
```

Select it with `--profile`:

```text
$ cargo build --profile release-fast
   Compiling myapp v0.1.0 (/private/tmp/cargo_probe/myapp)
    Finished `release-fast` profile [optimized] target(s) in 0.08s
```

Each profile gets its own output directory, so artifacts never clobber each other:

```text
$ ls target/
CACHEDIR.TAG  debug  release  release-fast
```

### Aliases: Cargo's `package.json` "scripts"

The `[alias]` table in `.cargo/config.toml` maps a short word to a Cargo subcommand invocation. Unlike npm scripts (which are arbitrary shell), aliases expand to **Cargo subcommands**, so `cargo lint` below literally runs `cargo clippy ...`:

```text
$ cargo c
    Checking myapp v0.1.0 (/private/tmp/cargo_probe/myapp)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.03s

$ cargo lint
    Checking myapp v0.1.0 (/private/tmp/cargo_probe/myapp)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.06s
```

Aliases can carry arguments and flags (`rr = "run --release"`). Anything you don't want hard-coded — arbitrary shell pipelines, multi-step orchestration — goes into a small `xtask` crate (a member of the workspace that you invoke via `cargo run -p xtask -- ...`), which is the community-standard replacement for a `Makefile`.

### Workspaces: a monorepo that shares one `target/` and one lockfile

A workspace ties multiple crates together so they:

- share a **single `Cargo.lock`** (consistent dependency versions across the repo);
- share a **single `target/` directory** (a dependency compiled once is reused by every member: huge build-time savings);
- can be built/tested together (`cargo build --workspace`) or individually (`cargo build -p core-lib`).

`[workspace.dependencies]` plus `dep.workspace = true` is the killer feature: declare a version range *once* at the root and every member inherits it, so you can't accidentally end up with two crates pinning different `serde` versions. `[workspace.package]` does the same for shared metadata like `version`, `edition`, and `license`.

`default-members` controls what a bare `cargo build`/`cargo run` (with no `-p`) operates on, handy when one member is "the app" and the rest are libraries.

### `[patch]`: override a dependency everywhere, transitively

`[patch]` replaces a crate **throughout the entire dependency graph**, including transitive uses you don't control. This is the right tool for: testing a bug fix against an upstream crate before it's released, pointing at your own fork, or pinning to a specific git commit. It's like pnpm `overrides` / Yarn `resolutions`, but it preserves Cargo's version resolution rather than blindly forcing a string.

Here we patch `semver` (a real crates.io dependency) to a local fork that adds a marker function. Because the patch takes effect, code that calls the fork-only function compiles and runs:

```text
$ cargo run
   Compiling patchapp v0.1.0 (/private/tmp/patch_probe/patchapp)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.47s
     Running `target/debug/patchapp`
patched-locally

$ cargo tree
patchapp v0.1.0 (/private/tmp/patch_probe/patchapp)
└── semver v1.0.28 (/private/tmp/patch_probe/semver-fork)
```

Note how `cargo tree` reports the dependency's source as the local path, not crates.io: proof the patch is wired in. For a git fork you'd write `semver = { git = "https://github.com/you/semver", branch = "fix" }` under `[patch.crates-io]` instead.

### Offline builds

`cargo build --offline` (or the persistent `[net] offline = true` in `.cargo/config.toml`) forbids any network access; Cargo resolves and builds purely from the local registry cache (`~/.cargo/registry`). This mirrors `npm ci --offline` and is what reproducible CI/Docker builds rely on. If everything you need is cached it just works:

```text
$ cargo build --offline
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.01s
```

### `cargo metadata`: the queryable lockfile

`cargo metadata --format-version 1` prints the **fully resolved** project state as JSON: every package, its version and source, the dependency graph, feature resolution, the target directory, and the workspace members. Tools like rust-analyzer, `cargo-deny`, and `cargo-nextest` all build on it. Piping through `jq`:

```text
$ cargo metadata --format-version 1 | jq '{
    workspace_root,
    target_directory,
    default_members: .workspace_default_members,
    members: [.packages[] | select(.source==null) | {name, version}]
  }'
{
  "workspace_root": "/private/tmp/ws_probe",
  "target_directory": "/private/tmp/ws_probe/target",
  "default_members": [
    "path+file:///private/tmp/ws_probe/crates/app#0.2.0"
  ],
  "members": [
    { "name": "app", "version": "0.2.0" },
    { "name": "core-lib", "version": "0.2.0" }
  ]
}
```

> **Note:** `source == null` distinguishes *your* workspace crates from downloaded dependencies (whose `source` is `"registry+..."` or `"git+..."`). The top-level `workspace_default_members` field holds the resolved package IDs of `default-members`.

---

## Key Differences

| Concept | Node / npm | Cargo |
| --- | --- | --- |
| Build "mode" | `NODE_ENV`, separate minifier/`tsconfig` | First-class **profiles** (`dev`/`release`/custom) in one file |
| Per-dependency build tuning | not really possible | `[profile.dev.package."*"]` |
| Project scripts | `package.json` `"scripts"` (arbitrary shell) | `[alias]` (expands to Cargo subcommands) |
| Monorepo | `pnpm-workspace.yaml` + per-package versions | `[workspace]` + `[workspace.dependencies]`/`[workspace.package]` |
| Shared install/output | per-package `node_modules` (hoisted) | one shared `target/` + one `Cargo.lock` |
| Force a transitive version | `overrides` / `resolutions` | `[patch]` (preserves resolution; supports path/git) |
| Offline install/build | `npm ci --offline` | `cargo build --offline` |
| Inspect resolved graph | `npm ls --json`, read lockfile | `cargo metadata` (stable, typed, designed for tooling) |
| Caret semantics | `^1.2.3` (explicit caret) | `"1.2.3"` **is** a caret range (not exact!) |

Three points where the analogy genuinely breaks down:

1. **`"1.2.3"` in Cargo is a caret range**, equivalent to npm's `^1.2.3`; it allows `1.x` updates. To pin exactly you write `"=1.2.3"`. This trips up developers who read a bare version string as "exactly this".
2. **Profiles have no real Node equivalent.** Node's optimization story is "transpile, then maybe minify"; Cargo bakes a rich optimization matrix into the build tool itself, switchable per command.
3. **Workspaces share one `target/` and one `Cargo.lock` by design**, not as an opt-in hoisting heuristic. There is no per-crate "node_modules"; a dependency compiled for one member is the *same* artifact every member links.

---

## Common Pitfalls

### Defining a profile in a non-root workspace member

Profiles are a **whole-graph** setting; they only take effect in the workspace root (or in a single-crate package). Put `[profile.release]` in a member crate and Cargo ignores it with a warning:

```text
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /private/tmp/ws_probe/crates/app/Cargo.toml
workspace: /private/tmp/ws_probe/Cargo.toml
```

**Fix:** move every `[profile.*]` table to the workspace-root `Cargo.toml`.

### Forgetting `inherits` on a custom profile

A custom profile that doesn't extend a built-in is rejected. Running `cargo build --profile release-fast` against a profile missing `inherits` produces (real `cargo` output):

```text
error: profile `release-fast` is missing an `inherits` directive (`inherits` is required for all profiles except `dev` or `release`)
```

**Fix:** add `inherits = "release"` (or `"dev"`).

### Expecting `--offline` to work with an empty cache

`--offline` cannot conjure a crate it has never downloaded. With an empty registry cache, even `serde` fails:

```text
error: no matching package named `serde` found
location searched: crates.io index
required by package `myapp v0.1.0 (/private/tmp/cargo_probe/myapp)`
As a reminder, you're using offline mode (--offline) which can sometimes cause surprising resolution failures, if this error is too confusing you may wish to retry without `--offline`.
```

**Fix:** populate the cache first with `cargo fetch` (or `cargo vendor` for fully air-gapped builds), then go offline.

### Misreading version strings as exact pins

`serde = "1"` does **not** lock you to some old 1.x; it's a caret range that happily resolves to the latest `1.x`. `cargo update` will bump it within that range. If you truly need an exact version (rare, usually a workaround), write `serde = "=1.0.228"`. Coming from npm, remember the caret is *implicit* in Cargo.

### Parsing `cargo metadata` text instead of JSON

Don't scrape human-readable output (`cargo build`, `cargo tree` without flags) in scripts; that text is not a stable interface. `cargo metadata --format-version 1` is the contract designed for machines; pin the format version so a future Cargo doesn't surprise you.

---

## Best Practices

- **Keep all `[profile.*]` and `[patch.*]` tables in the workspace root.** They're global; scattering them causes the "ignored profile" warning above.
- **Use `[workspace.dependencies]` + `dep.workspace = true`** for every dependency shared by more than one member. One version, one place to bump.
- **Add a `lto = "thin"` + `codegen-units = 1` + `strip = "symbols"` release profile** for production binaries: it meaningfully shrinks and speeds up the artifact for a modest build-time cost. Measure with `cargo build --timings` (it writes an HTML flamegraph to `target/cargo-timings/`).
- **Commit `Cargo.lock`** for binaries and applications (reproducible builds); libraries traditionally don't, though committing it for CI determinism is increasingly common and harmless.
- **Treat `[patch]` as temporary.** It's perfect for testing an upstream fix or a fork, but a long-lived patch is technical debt; track it and remove it once the fix lands upstream.
- **Prefer `cargo metadata` (typed) over hand-parsing `Cargo.lock`.** It already resolved features and the full graph for you.
- **Use aliases for the common 80%** and an `xtask` crate for anything that needs real logic, so your "scripts" stay portable across every contributor's shell.

---

## Real-World Example

A common production need: a small internal tool that audits the workspace — counts crates and flags the most dependency-heavy one (a rough complexity signal). Instead of shelling out and parsing JSON by hand, depend on the **`cargo_metadata`** crate, which deserializes `cargo metadata` into typed structs.

```bash
cargo new --name metatool metatool
cd metatool
cargo add cargo_metadata
```

```rust
// src/main.rs
use cargo_metadata::MetadataCommand;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Runs `cargo metadata` under the hood and parses it into typed structs —
    // no manual JSON wrangling, no fragile text scraping.
    let md = MetadataCommand::new().exec()?;

    // `workspace_members` are *our* crates; `packages` includes downloaded deps too.
    let members = md.workspace_members.len();
    let total = md.packages.len();
    println!("{members} workspace member(s), {total} package(s) total");

    // Find the crate with the most declared dependencies — a cheap "blast radius" proxy.
    if let Some(p) = md.packages.iter().max_by_key(|p| p.dependencies.len()) {
        println!("most-connected crate: {} ({} deps)", p.name, p.dependencies.len());
    }
    Ok(())
}
```

Real output when run inside this single-crate project (which pulls in `cargo_metadata` and its dependencies):

```text
$ cargo run -q
1 workspace member(s), 18 package(s) total
most-connected crate: syn (15 deps)
```

This is exactly how `cargo-deny`, `cargo-udeps`, and rust-analyzer discover your project structure: they call `cargo metadata` and build on the typed result. Reaching for the same crate keeps your internal tooling on the supported, stable interface.

> **Tip:** The `cargo_metadata` crate version resolves automatically with `cargo add` (it was `0.23.x` at the time of writing). Always let `cargo add` pick the current version rather than pinning from memory.

---

## Further Reading

- [The Cargo Book — Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)
- [The Cargo Book — Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [The Cargo Book — Overriding Dependencies (`[patch]`)](https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html)
- [The Cargo Book — `cargo metadata`](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html)
- [The Cargo Book — Configuration (`.cargo/config.toml`, aliases, `[net] offline`)](https://doc.rust-lang.org/cargo/reference/config.html)
- [`cargo_metadata` crate docs](https://docs.rs/cargo_metadata)
- Guide cross-links:
  - [01 — Cargo Basics](/01-getting-started/03-cargo-basics/) — the introductory tour this page builds on
  - [12 — Modules & Packages](/12-modules-packages/) — crates, modules, and visibility
  - [Useful Cargo Plugins](/24-tooling/11-cargo-plugins/) — `nextest`, `watch`, `audit`, `deny`, `expand`, and more
  - [Formatting with rustfmt](/24-tooling/01-formatting/) — `rustfmt` and `rustfmt.toml`
  - [Linting with Clippy](/24-tooling/02-linting/) — Clippy and lint levels
  - [CI/CD Concepts for Rust](/24-tooling/07-ci-cd/) and [A Real GitHub Actions Workflow for Rust](/24-tooling/08-github-actions/) — using profiles/offline/caching in CI
  - [Dockerizing Rust](/24-tooling/09-docker/) — release profiles and offline builds in multi-stage images
  - [Advanced Topics](/25-advanced-topics/) — where to go next

---

## Exercises

### Exercise 1: A faster-compiling release profile

**Difficulty:** Beginner

**Objective:** Create a custom profile that produces an optimized binary but compiles faster than full `release`, for use while profiling.

**Instructions:** In a fresh `cargo new` project, add a `[profile.release-fast]` table that inherits from `release` but disables LTO and uses many codegen units. Build with `cargo build --profile release-fast` and confirm Cargo reports the `release-fast` profile and creates a `target/release-fast/` directory.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "myapp"
version = "0.1.0"
edition = "2024"

[dependencies]

[profile.release-fast]
inherits = "release"
lto = false
codegen-units = 16
```

```text
$ cargo build --profile release-fast
   Compiling myapp v0.1.0 (/private/tmp/cargo_probe/myapp)
    Finished `release-fast` profile [optimized] target(s) in 0.08s

$ ls target/
CACHEDIR.TAG  debug  release  release-fast
```

A custom profile *must* declare `inherits`; omitting it is a hard error. The separate output directory means `release` and `release-fast` artifacts coexist without recompilation.

</details>

### Exercise 2: A two-crate workspace with shared dependencies

**Difficulty:** Intermediate

**Objective:** Build a workspace where a library crate and a binary crate share a single dependency version declared once at the root.

**Instructions:** Create a virtual-manifest workspace with members `crates/core-lib` (a library that derives `serde::Serialize`) and `crates/app` (a binary depending on `core-lib` and `anyhow`). Declare `serde`, `anyhow`, and the `core-lib` path dep in `[workspace.dependencies]`, share `version`/`edition` via `[workspace.package]`, set `default-members` to the app, and confirm a bare `cargo run` runs the app.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml (workspace root, virtual manifest)
[workspace]
resolver = "3"
members = ["crates/*"]
default-members = ["crates/app"]

[workspace.package]
version = "0.2.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
anyhow = "1"
core-lib = { path = "crates/core-lib" }
```

```toml
# crates/core-lib/Cargo.toml
[package]
name = "core-lib"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
```

```rust
// crates/core-lib/src/lib.rs
use serde::Serialize;

#[derive(Serialize)]
pub struct Order {
    pub id: u64,
    pub total_cents: u64,
}

pub fn describe(o: &Order) -> String {
    format!("order {} = ${:.2}", o.id, o.total_cents as f64 / 100.0)
}
```

```toml
# crates/app/Cargo.toml
[package]
name = "app"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
core-lib.workspace = true
anyhow.workspace = true
```

```rust
// crates/app/src/main.rs
use core_lib::{describe, Order};

fn main() -> anyhow::Result<()> {
    let o = Order { id: 7, total_cents: 12_999 };
    println!("{}", describe(&o));
    Ok(())
}
```

```text
$ cargo run
   Compiling core-lib v0.2.0 (/private/tmp/ws_probe/crates/core-lib)
   Compiling app v0.2.0 (/private/tmp/ws_probe/crates/app)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.03s
     Running `target/debug/app`
order 7 = $129.99
```

`cargo run` builds only the `default-members` (the app), but both crates share one `target/` and one `Cargo.lock`.

</details>

### Exercise 3: A workspace auditor using `cargo metadata`

**Difficulty:** Advanced

**Objective:** Write a tool that consumes `cargo metadata` (via the typed `cargo_metadata` crate) and prints the workspace member count and the most dependency-heavy crate.

**Instructions:** In a new project, `cargo add cargo_metadata`. Use `MetadataCommand::new().exec()` to obtain the metadata, then report how many workspace members and total packages exist, and which package declares the most dependencies. Run it inside any Cargo project and verify the counts.

<details>
<summary>Solution</summary>

```bash
cargo new --name metatool metatool
cd metatool
cargo add cargo_metadata
```

```rust
// src/main.rs
use cargo_metadata::MetadataCommand;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let md = MetadataCommand::new().exec()?;

    let members = md.workspace_members.len();
    let total = md.packages.len();
    println!("{members} workspace member(s), {total} package(s) total");

    if let Some(p) = md.packages.iter().max_by_key(|p| p.dependencies.len()) {
        println!("most-connected crate: {} ({} deps)", p.name, p.dependencies.len());
    }
    Ok(())
}
```

```text
$ cargo run -q
1 workspace member(s), 18 package(s) total
most-connected crate: syn (15 deps)
```

`md.workspace_members` are the IDs of *your* crates; `md.packages` includes every resolved dependency too. The exact totals depend on which crates `cargo_metadata` itself pulls in, so your numbers may differ; the structure of the answer is what matters. This is the same supported path `cargo-deny`, `cargo-udeps`, and rust-analyzer use to understand a project.

</details>
