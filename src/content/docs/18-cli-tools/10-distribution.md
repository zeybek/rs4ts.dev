---
title: "Distributing CLI Tools"
description: "Ship a Rust CLI as one native binary instead of an npm package: cargo install, cross-compiled prebuilt binaries, cargo-dist releases, and tuned release profiles."
---

## Quick Overview

You wrote a great command-line tool. Now how do people get it? In the Node world the answer is almost always "publish to npm and tell users to `npm install -g`", which means every machine needs Node and a network round-trip to a registry. Rust compiles to a **single self-contained native binary**, which opens up faster, simpler distribution paths: `cargo install` from source, prebuilt downloads, and automated GitHub releases. This page covers the four pieces you actually ship with: `cargo install`, prebuilt binaries (and how to cross-compile them), the `cargo-dist`/`dist` tool that automates a full release pipeline, and the **release profiles** that decide how small and fast your binary is.

The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. The release-profile and version-embedding examples are pure standard Cargo and need no dependencies; the optional build-metadata example uses the `built` crate.

---

## TypeScript/JavaScript Example

Distributing a Node CLI means publishing a package and relying on the user's installed runtime. A typical `package.json` for a global CLI looks like this:

```json
{
  "name": "@acme/greet",
  "version": "1.2.0",
  "type": "module",
  "bin": {
    "greet": "./dist/cli.js"
  },
  "files": ["dist"],
  "engines": {
    "node": ">=18"
  },
  "scripts": {
    "build": "tsc -p tsconfig.json",
    "prepublishOnly": "npm run build"
  }
}
```

```typescript
#!/usr/bin/env node
// dist/cli.js — the shebang tells the OS to run this with node.
console.log(`Hello from greet ${process.env.npm_package_version ?? ""}!`);
```

Users then install it globally and run it:

```text
$ npm install -g @acme/greet
$ greet
Hello from greet !
```

This works, but notice what travels with the tool. The `bin` entry is a **JavaScript file**, not an executable; it relies on the `#!/usr/bin/env node` shebang and the user already having a compatible Node (the `engines` field is a *hint*, not enforcement). The user downloads your code plus a `node_modules` tree, then your code is interpreted at every launch. There is no single artifact you can hand someone who does not have Node installed.

> **Note:** Tools like `pkg`, `nexe`, or Node 21+'s Single Executable Applications can bundle Node into one binary, but they are bolt-on solutions producing 40-90 MB artifacts. With Rust, a single static-ish binary is the *default* output, not an afterthought.

---

## Rust Equivalent

A Rust CLI compiles to one native executable. The Cargo manifest declares the binary, and `cargo build --release` produces it:

```toml
# Cargo.toml
[package]
name = "greet"
version = "1.2.0"
edition = "2024"

# A [[bin]] is implied for src/main.rs, but you can name it explicitly.
[[bin]]
name = "greet"
path = "src/main.rs"
```

```rust
// src/main.rs
use std::process::ExitCode;

fn main() -> ExitCode {
    // Cargo bakes these values in at compile time from Cargo.toml.
    let name = env!("CARGO_PKG_NAME");
    let version = env!("CARGO_PKG_VERSION");
    println!("Hello from {name} {version}!");
    ExitCode::SUCCESS
}
```

Building and running it:

```text
$ cargo build --release
   Compiling greet v1.2.0 (/tmp/greet)
    Finished `release` profile [optimized] target(s) in 0.34s

$ ./target/release/greet
Hello from greet 1.2.0!
```

That `target/release/greet` file is the *entire* deliverable. There is no runtime to install, no `node_modules`, and no interpreter. You can copy it to a machine that has never seen Rust and run it. The rest of this page is about (1) the three ways to get that binary to users and (2) tuning how it is built.

---

## Detailed Explanation

### Path 1: `cargo install` — the `npm install -g` analogue

`cargo install` is the closest thing to `npm install -g`, with one big difference: it compiles from source on the user's machine and drops the resulting **binary** into `~/.cargo/bin` (which `rustup` already put on your `PATH`). The user needs a Rust toolchain, but the artifact they end up with is a native executable, not interpreted source.

```text
# From crates.io (like `npm install -g <name>`):
$ cargo install ripgrep        # installs the `rg` binary

# From a specific version (a caret range, newest matching):
$ cargo install ripgrep --version "14"

# Directly from a git repository (no registry needed):
$ cargo install --git https://github.com/BurntSushi/ripgrep

# From a local checkout — the inner-loop command while developing:
$ cargo install --path .
```

Here is a real `cargo install --path .` run for a small tool (output paths abbreviated):

```text
$ cargo install --path .
   Compiling clap v4.6.1
   Compiling mytool v0.1.0 (/tmp/mytool)
    Finished `release` profile [optimized] target(s) in 9.98s
  Installing /home/you/.cargo/bin/mytool
   Installed package `mytool v0.1.0 (/tmp/mytool)` (executable `mytool`)
warning: be sure to add `/home/you/.cargo/bin` to your PATH to be able to run the installed binaries
```

Key behaviors a Node developer should internalize:

- **It always builds in release mode.** Unlike `cargo build` (debug by default), `cargo install` optimizes by default; there is no `--release` flag to pass.
- **It installs the binary, not the source.** `~/.cargo/bin/mytool` is a standalone executable; the source is discarded after the build.
- **Reinstalling upgrades in place.** Running `cargo install` again with a newer version replaces the old binary. Use `cargo install --list` to see what is installed, and `cargo uninstall mytool` to remove it.

```text
$ cargo install --list
mytool v0.1.0:
    mytool
ripgrep v14.1.1:
    rg
```

The trade-off versus npm: the user pays a one-time **compile cost** (seconds to minutes) instead of a download. That is fine for developer-facing tools but a poor experience for everyone else, which is why you also ship prebuilt binaries.

> **Tip:** [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall) (current version 1.19.1) is a popular companion: `cargo binstall mytool` downloads a **prebuilt** binary from your GitHub releases when one exists and only falls back to compiling if it does not — combining the discoverability of `cargo install` with the speed of a download.

### Path 2: Prebuilt binaries

The reason `ripgrep`, `fd`, `bat`, and friends feel instant to install is that their maintainers publish **prebuilt binaries** for each platform. A user (or a Homebrew/Scoop/apt formula) downloads a `.tar.gz` or `.zip`, extracts one file, and runs it: no toolchain, no compile.

To produce a binary for a platform other than your own, you cross-compile to a **target triple**. Add the target with `rustup`, then pass `--target`:

```text
# See your host triple and what's installed:
$ rustc -vV | grep host
host: aarch64-apple-darwin

$ rustup target list --installed
aarch64-apple-darwin
x86_64-unknown-linux-gnu
wasm32-unknown-unknown

# Add a target and build for it:
$ rustup target add x86_64-unknown-linux-musl
$ cargo build --release --target x86_64-unknown-linux-musl
```

The output then lives under `target/<triple>/release/`. Common triples you will ship:

| Target triple | Platform | Notes |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | Linux (Intel/AMD) | Dynamically links glibc |
| `x86_64-unknown-linux-musl` | Linux (Intel/AMD) | **Fully static**; runs on any distro, Alpine included |
| `aarch64-unknown-linux-gnu` | Linux (ARM64) | Servers, Raspberry Pi |
| `x86_64-apple-darwin` | macOS (Intel) | |
| `aarch64-apple-darwin` | macOS (Apple Silicon) | |
| `x86_64-pc-windows-msvc` | Windows | Produces `.exe`; needs the MSVC linker |

Pure cross-compiling works out of the box for many targets, but anything that links C libraries (or needs a different libc) often needs a cross linker. Two tools smooth this over:

- **`cross`** (current 0.2.5) — a drop-in `cargo` replacement that runs the build inside a Docker/Podman container with the right toolchain: `cross build --release --target aarch64-unknown-linux-gnu`. No host setup beyond a container runtime.
- **GitHub Actions matrix builds** — run the native compiler on each OS runner (`ubuntu-latest`, `macos-latest`, `windows-latest`) so no cross-compilation is needed at all. This is what `cargo-dist` automates (Path 3).

> **Tip:** For maximum portability on Linux, build the **musl** target. A `x86_64-unknown-linux-musl` binary is statically linked, so it runs on Ubuntu, Debian, Alpine, and inside scratch containers without "glibc version too old" surprises; a class of problem Node sidesteps by shipping its own runtime and that Rust sidesteps by static linking.

### Path 3: `cargo-dist` — automating the whole release

Building six binaries by hand, tarring them up, computing checksums, writing installer scripts, and attaching everything to a GitHub release is tedious and error-prone. **`cargo-dist`** (crate `cargo-dist`, current 0.32.0; the command was renamed to `dist`) generates a complete release pipeline from a few lines of config. Think of it as the Rust analogue of a fully wired `npm publish` plus `release-please` plus platform installers, but producing native binaries.

Install it and initialize:

```text
$ cargo install cargo-dist
$ dist init
```

`dist init` interviews you (which targets, which installers, CI provider) and writes configuration into your `Cargo.toml`/`dist-workspace.toml` plus a `.github/workflows/release.yml`. The config looks roughly like this:

```toml
# In Cargo.toml (or dist-workspace.toml for a workspace)
[workspace.metadata.dist]
# Which cargo-dist version to pin the generated CI to:
cargo-dist-version = "0.32.0"
# Which CI provider to generate workflows for:
ci = ["github"]
# Generated user-facing installers:
installers = ["shell", "powershell", "homebrew"]
# Platforms to build for:
targets = [
    "x86_64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
]
```

From then on, pushing a version tag (`git tag v1.2.0 && git push --tags`) triggers the generated workflow, which:

1. Builds release binaries for every target on the matching CI runner.
2. Bundles them into per-platform archives with checksums.
3. Creates a GitHub Release and uploads the artifacts.
4. Publishes the installer scripts, so users can run a one-liner like
   `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/you/mytool/releases/latest/download/mytool-installer.sh | sh`.

The end result: your users get the instant-download experience of `ripgrep`, and you maintain a handful of config lines instead of a fragile shell script.

> **Note:** `cargo-dist` reads your release profile (below) and builds with it. It defines its own optimized build settings; if you want to customize them, add a `[profile.dist]` section that `inherits = "release"`.

### Release profiles — how Cargo decides what to build

A **profile** is a named set of compiler settings. Cargo ships two you use constantly:

- `dev` (used by `cargo build`, `cargo run`): no optimization, fast compiles, full debug info. The `target/debug/` binary.
- `release` (used by `cargo build --release`, and always by `cargo install`): optimized, slower to compile. The `target/release/` binary.

The single most common distribution mistake is shipping the **debug** binary. The difference is dramatic in both size and speed, so for anything users run, always build `--release`.

You tune the release build in `Cargo.toml` under `[profile.release]`. Here is a size-focused profile, with each knob explained:

```toml
# Cargo.toml
[profile.release]
opt-level = "z"     # optimize aggressively for size ("s" is a milder variant)
lto = true          # link-time optimization: inline/strip across crate boundaries
codegen-units = 1   # one codegen unit = better optimization, slower compile
strip = true        # remove symbol/debug info from the final binary
panic = "abort"     # drop unwinding tables; smaller binary, but no catch_unwind
```

The effect is real and measurable. For the same trivial program, with the default release profile versus this size-tuned profile:

```text
default release bytes:   406272
optimized release bytes: 285936
```

That is a ~30% reduction on a tiny program; on a real CLI with many dependencies the absolute savings are far larger. The knobs in detail:

| Setting | Default (release) | What it does | Cost |
| --- | --- | --- | --- |
| `opt-level` | `3` (speed) | `"z"`/`"s"` optimize for size; `3` for raw speed | Size vs. speed trade |
| `lto` | `false` | `true`/`"thin"` optimize across crates | Longer link time |
| `codegen-units` | `16` | `1` lets the optimizer see everything | Slower, non-parallel compile |
| `strip` | `false` | `true`/`"symbols"` removes symbols | Worse stack traces |
| `panic` | `"unwind"` | `"abort"` removes unwinding machinery | No `catch_unwind`; affects some tests |

You can also define **custom profiles** that inherit from another, exactly what `cargo-dist` does:

```toml
# Cargo.toml
[profile.dist]
inherits = "release"
lto = "thin"
codegen-units = 1
```

```text
$ cargo build --profile dist
    Finished `dist` profile [optimized] target(s) in 0.41s
# Output lands in target/dist/ (named after the profile).
```

> **Warning:** `panic = "abort"` is great for shrinking a binary, but it disables `std::panic::catch_unwind` and changes how some test harnesses behave. If your tool deliberately catches panics (rare in a CLI) or you hit odd test failures, drop this one knob first.

---

## Key Differences

| Concern | Node / npm | Rust / Cargo |
| --- | --- | --- |
| What ships | JS source + `node_modules` | One native binary |
| Runtime on user's machine | Required (Node ≥ X) | None (self-contained) |
| Registry-based install | `npm install -g pkg` | `cargo install pkg` (compiles from source) |
| Prebuilt-binary install | `pkg`/`nexe` (bolt-on, large) | First-class: any target triple |
| Cross-platform builds | Same JS runs everywhere | Compile per target triple (`--target`) |
| Static linking | N/A (ships the runtime) | `*-linux-musl` → fully static |
| Automated release pipeline | release-please, np, etc. | `cargo-dist` / `dist` |
| Build tuning | minifier/bundler config | `[profile.release]` knobs |
| Versioning source of truth | `package.json` `version` | `Cargo.toml` `version` (`env!("CARGO_PKG_VERSION")`) |

The deepest difference is the *shape of the artifact*. An npm package is fundamentally a bundle of source plus a manifest, interpreted at runtime; a Cargo "package" is a recipe that produces a compiled binary. Distribution in Node is about getting source and dependencies to a runtime; distribution in Rust is about getting one compiled file to a machine. That is why `cargo install` *compiles* (it received a recipe, not a product) and why prebuilt binaries are the natural high-performance path.

A second difference: in Node the version a user runs is whatever `package.json` said *and* whatever Node they happen to have. In Rust the version, the target triple, and even the build profile are baked into the binary at compile time, so `mytool --version` can report exactly what was shipped (see the Real-World Example).

---

## Common Pitfalls

### Pitfall 1: Distributing the debug binary

`cargo build` (no flags) writes to `target/debug/`, which is unoptimized and often several times larger and much slower than release. Shipping that is the classic mistake.

```text
$ cargo build            # -> target/debug/mytool   (unoptimized, large)
$ cargo build --release  # -> target/release/mytool (what you ship)
```

`cargo install` sidesteps this because it *always* builds release, but if you copy a binary out of `target/` by hand, double-check you took it from `target/release/`.

### Pitfall 2: `env!` fails the build when the variable is missing

The `env!` macro reads an environment variable **at compile time** and is a hard error if it is unset. People reach for it to embed a git hash, then break the build on machines where that variable was never exported. The real compiler error:

```rust
fn main() {
    // does not compile if GIT_HASH is not set at build time
    let hash = env!("GIT_HASH");
    println!("{hash}");
}
```

```text
error: environment variable `GIT_HASH` not defined at compile time
 --> src/main.rs:3:16
  |
3 |     let hash = env!("GIT_HASH");
  |                ^^^^^^^^^^^^^^^^
```

Use `option_env!` instead, which returns `Option<&str>` and never fails to compile:

```rust playground
fn main() {
    // option_env! yields None instead of failing the build.
    let hash = option_env!("GIT_HASH").unwrap_or("dev");
    println!("build {hash}");
}
```

```text
$ cargo run --quiet
build dev
```

Better still, set the variable yourself from a `build.rs` script (shown in the Real-World Example) so it is always present.

### Pitfall 3: Forgetting that `cargo install` requires a toolchain

`cargo install mytool` is convenient for Rust developers but useless to someone without `rustup`/`cargo`. If your audience is non-developers, *do not* tell them to `cargo install`. Ship a prebuilt binary or an installer script (Path 2/3). Reserve `cargo install` instructions for fellow Rust users.

### Pitfall 4: Assuming one binary runs on every Linux

A default `x86_64-unknown-linux-gnu` build links against the host's glibc. A binary built on a new Ubuntu may refuse to start on an older distro ("version `GLIBC_2.34' not found"). For broad Linux compatibility, build the **musl** target, which links statically and has no such dependency:

```text
$ rustup target add x86_64-unknown-linux-musl
$ cargo build --release --target x86_64-unknown-linux-musl
```

### Pitfall 5: Bumping the version in the wrong place

In Node you bump `package.json` (often via `npm version`). In Rust the source of truth is `Cargo.toml`'s `version`, surfaced at runtime via `env!("CARGO_PKG_VERSION")` and by clap's `#[command(version)]`. Editing a constant in `main.rs` instead means `--version` and the published crate disagree. Keep one source of truth: `Cargo.toml`.

---

## Best Practices

- **Always build and ship `--release`.** Add a tuned `[profile.release]` once and forget it. Start with `lto = true`, `codegen-units = 1`, and `strip = true`; add `opt-level = "z"` and `panic = "abort"` only if binary size matters more than raw speed.
- **Let `Cargo.toml` own the version.** Read it with `env!("CARGO_PKG_VERSION")` and wire clap's `#[command(version)]` to it so `--version` is always correct. See [Parsing Arguments with the clap Derive API](/18-cli-tools/01-clap-derive/).
- **Offer both install paths.** Document `cargo install mytool` for Rust developers *and* a prebuilt-binary/installer one-liner for everyone else.
- **Automate releases with `cargo-dist` early.** Wiring it up on day one is far cheaper than retrofitting a hand-rolled release script later.
- **Build musl for Linux distribution.** A static binary eliminates an entire class of glibc support tickets.
- **Embed build metadata in `--version`** (git hash, target triple, profile) so a bug report tells you exactly which build the user has.
- **Set sensible package metadata** — `description`, `license`, `repository`, `keywords`, `categories` in `Cargo.toml` — before `cargo publish`; crates.io enforces some of these and users read them.

---

## Real-World Example

A production CLI's `--version` should answer "exactly which build is this?"; semver alone is not enough when you are chasing a bug. This example wires up a `build.rs` script that captures the git commit at build time, exposes it via an environment variable, and composes a rich version string that clap prints. It degrades gracefully when there is no git repository (e.g. building from a release tarball).

```toml
# Cargo.toml
[package]
name = "deploy"
version = "1.2.0"
edition = "2024"

[dependencies]
clap = { version = "4", features = ["derive"] }

[profile.release]
lto = true
codegen-units = 1
strip = true
```

```rust playground
// build.rs — runs at build time, before the crate is compiled.
use std::process::Command;

fn main() {
    // Capture the short git hash, if git and a repository are available.
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string());

    match git_hash {
        // Expose the value to the crate as the GIT_HASH compile-time env var.
        Some(hash) => println!("cargo:rustc-env=GIT_HASH={hash}"),
        None => println!("cargo:rustc-env=GIT_HASH=unknown"),
    }

    // Re-run this script when HEAD moves, so the hash stays current.
    println!("cargo:rerun-if-changed=.git/HEAD");
}
```

```rust
// src/main.rs
use clap::Parser;

// Compose a richer version string from compile-time values.
// CARGO_PKG_* come from Cargo.toml; GIT_HASH is set by build.rs.
const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("GIT_HASH"),
    ", ",
    env!("CARGO_PKG_NAME"),
    ")"
);

/// A deploy helper whose --version reports the exact build.
#[derive(Parser)]
#[command(name = "deploy", version = VERSION)]
struct Cli {
    /// Environment to deploy to.
    #[arg(default_value = "staging")]
    target: String,
}

fn main() {
    let cli = Cli::parse();
    println!("deploying to {}", cli.target);
}
```

Running it outside a git repository (e.g. an unpacked release tarball) falls back cleanly, and inside one it reports the commit:

```text
# No git repository present:
$ cargo run --quiet -- --version
deploy 1.2.0 (unknown, deploy)

# Inside a git repository:
$ cargo run --quiet -- --version
deploy 1.2.0 (1aa8128, deploy)

$ cargo run --quiet
deploying to staging
```

For an even richer version line (target triple, build profile, rustc version), the [`built`](https://crates.io/crates/built) crate captures all of it for you. Add it as a build dependency and generate the metadata:

```toml
# Cargo.toml
[build-dependencies]
built = { version = "0.8", features = ["git2"] }
```

```rust
// build.rs
fn main() {
    // Writes $OUT_DIR/built.rs with PKG_VERSION, GIT_COMMIT_HASH_SHORT,
    // TARGET, PROFILE, RUSTC_VERSION, and more as constants.
    built::write_built_file().expect("failed to gather build-time info");
}
```

```rust
// src/main.rs
use clap::Parser;

// Pull in the constants `built` generated.
mod build_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

fn long_version() -> &'static str {
    use build_info::{GIT_COMMIT_HASH_SHORT, PKG_VERSION, PROFILE, TARGET};
    let commit = GIT_COMMIT_HASH_SHORT.unwrap_or("unknown");
    // Leak the formatted string so clap can hold a &'static str.
    Box::leak(format!("{PKG_VERSION} ({commit} {TARGET} {PROFILE})").into_boxed_str())
}

/// A tool whose --version shows the full build provenance.
#[derive(Parser)]
#[command(name = "buildinfo", version = long_version())]
struct Cli {}

fn main() {
    let _ = Cli::parse();
    println!("running");
}
```

```text
$ cargo run --quiet -- --version
buildinfo 0.1.0 (374eb71 aarch64-apple-darwin debug)
```

Now every bug report that includes `--version` tells you the semver, the exact commit, the platform it was built for, and whether it was a debug or release build; invaluable when you ship binaries to platforms you do not control.

---

## Further Reading

- [The Cargo Book — `cargo install`](https://doc.rust-lang.org/cargo/commands/cargo-install.html) — full reference for installing from source.
- [The Cargo Book — Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) — every release-profile setting and its default.
- [The Cargo Book — Build Scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html) — how `build.rs` and `cargo:rustc-env=` work.
- [The Cargo Book — Publishing on crates.io](https://doc.rust-lang.org/cargo/reference/publishing.html) — metadata requirements and `cargo publish`.
- [`cargo-dist` documentation](https://opensource.axo.dev/cargo-dist/) — the automated release pipeline.
- [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall) and [`cross`](https://github.com/cross-rs/cross) — fast binary install and painless cross-compilation.
- Related guide sections: [clap derive API](/18-cli-tools/01-clap-derive/) for wiring `--version`; [Cross-platform considerations](/18-cli-tools/09-cross-platform/) for target triples and exit codes; [Environment variables](/18-cli-tools/08-environment-vars/) for runtime configuration; [File I/O](/18-cli-tools/06-file-io/) and [Path handling](/18-cli-tools/07-path-handling/) for the rest of a CLI's plumbing.
- Foundations: [Understanding Cargo](/01-getting-started/03-cargo-basics/) and [Section 02: Basics](/02-basics/). For shipping to the browser instead of a binary, see [Section 19: WebAssembly](/19-wasm/).

---

## Exercises

### Exercise 1: A size-tuned release profile

**Difficulty:** Beginner

**Objective:** Measure the impact of release-profile settings on binary size.

**Instructions:** Create a new binary crate with `cargo new sizer`. Build it once with the default release profile (`cargo build --release`) and record the size of `target/release/sizer`. Then add a `[profile.release]` section enabling `lto = true`, `codegen-units = 1`, and `strip = true`, rebuild, and compare. (On macOS/Linux, `ls -l target/release/sizer` shows the byte count.)

<details>
<summary>Solution</summary>

Add this to `Cargo.toml`:

```toml
# Cargo.toml
[profile.release]
lto = true          # optimize across crate boundaries at link time
codegen-units = 1   # let the optimizer see the whole crate
strip = true        # remove symbol/debug info from the binary
```

```rust playground
// src/main.rs
fn main() {
    println!("Hello, world!");
}
```

```text
$ cargo build --release   # before adding the profile
$ ls -l target/release/sizer    # note the size
$ cargo build --release   # after adding the profile
$ ls -l target/release/sizer    # smaller
```

For a near-trivial program, the default release profile produced a ~406 KB binary while a tuned profile (here using `opt-level = "z"` as well) produced ~286 KB — about a 30% reduction. `strip = true` removes the symbol table (which is why the binary shrinks even though the *code* is identical), and `lto` + `codegen-units = 1` let the optimizer eliminate cross-crate dead code. The savings grow with the number of dependencies.

</details>

### Exercise 2: Wire `--version` to `Cargo.toml`

**Difficulty:** Intermediate

**Objective:** Make a single source of truth for the version, surfaced via `--version`.

**Instructions:** Using clap's derive API, build a CLI named `whatver` whose `--version` prints the value from `Cargo.toml` and nothing hard-coded in source. Set the `version` field in `Cargo.toml` to `"2.3.1"` and confirm `whatver --version` reports it. (Add clap with `cargo add clap --features derive`.)

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "whatver"
version = "2.3.1"
edition = "2024"

[dependencies]
clap = { version = "4", features = ["derive"] }
```

```rust playground
// src/main.rs
use clap::Parser;

/// A tool that reports its version from Cargo.toml.
// `version` with no value uses env!("CARGO_PKG_VERSION") automatically.
#[derive(Parser)]
#[command(name = "whatver", version)]
struct Cli {
    /// Optional name to greet.
    #[arg(default_value = "world")]
    name: String,
}

fn main() {
    let cli = Cli::parse();
    println!("Hello, {}!", cli.name);
}
```

```text
$ cargo run --quiet -- --version
whatver 2.3.1

$ cargo run --quiet -- Ada
Hello, Ada!
```

`#[command(version)]` with no explicit value tells clap to read `CARGO_PKG_VERSION`, which Cargo bakes in from the manifest. Bumping the version in `Cargo.toml` is now the *only* place you change it; `--version` and the published crate can never drift apart.

</details>

### Exercise 3: Embed a build-time fingerprint

**Difficulty:** Advanced

**Objective:** Use a `build.rs` script to inject a value into the binary that is available at runtime, degrading gracefully.

**Instructions:** Write a `build.rs` that sets a `BUILD_TIME` environment variable to the current UTC date (you may shell out to `date -u +%Y-%m-%d`, or hard-code a placeholder if `date` is unavailable). In `main.rs`, read it with `option_env!` so the build never fails if the variable is missing, defaulting to `"unknown"`, and print it. Explain why `option_env!` is the safe choice over `env!` here.

<details>
<summary>Solution</summary>

```rust playground
// build.rs
use std::process::Command;

fn main() {
    // Try to capture today's UTC date; fall back if `date` is unavailable.
    let build_date = Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string());

    if let Some(date) = build_date {
        // Expose it as a compile-time env var for the crate.
        println!("cargo:rustc-env=BUILD_TIME={date}");
    }
    // If we printed nothing, BUILD_TIME is simply unset — handled below.
}
```

```rust playground
// src/main.rs
fn main() {
    // option_env! returns Option<&str> resolved at compile time.
    // It yields None (instead of failing the build) when BUILD_TIME is unset.
    let built = option_env!("BUILD_TIME").unwrap_or("unknown");
    println!("built on {built}");
}
```

```text
$ cargo run --quiet
built on 2026-06-01
```

`option_env!` is the safe choice because `env!` is a **compile error** if the variable is not defined, and a build script is not guaranteed to set it (the `date` command might fail, or someone might compile in an unusual environment). `option_env!` turns "not set" into a `None` you can handle (`unwrap_or("unknown")`), so the binary always builds. The `build.rs` runs before crate compilation and communicates back to the compiler purely through the `cargo:rustc-env=KEY=VALUE` lines it prints to stdout.

</details>
