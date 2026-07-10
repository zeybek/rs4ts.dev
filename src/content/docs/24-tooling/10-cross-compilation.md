---
title: "Cross-Compilation"
description: "Build a Linux or ARM binary from your Mac. Rust cross-compiles natively with target triples, rustup targets, the cross tool, and fully static musl builds."
---

## Quick Overview

**Cross-compilation** means building an executable for a platform other than the one you are building *on*. For example, producing a Linux x86-64 binary from your Apple-silicon Mac, or an ARM Raspberry Pi binary from a CI runner. Because Rust compiles to native machine code (there is no interpreter to ship), the binary you deploy must match the *target's* CPU and operating system exactly. The payoff is huge: a single statically-linked Rust binary can run on a server, in a `FROM scratch` Docker image, or on an embedded board with **no runtime, no `node_modules`, and no installed dependencies**.

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition. `rustup` and `cargo` ship cross-compilation support out of the box: you add a *target* (precompiled standard library), and Cargo does the rest. The harder part is the *linker*, which we cover in depth below.

---

## TypeScript/JavaScript Example

In Node.js, "the binary" is the Node runtime, and your code is just text that the runtime interprets. You almost never think about the target CPU; you ship `.js` files and a `package.json`, and the user's installed Node handles the platform. "Cross-compilation" in the JS world is therefore an unusual, bolt-on concern handled by third-party tools that *bundle* a Node runtime with your script:

```jsonc
// package.json — using @yao-pkg/pkg (the maintained fork of vercel/pkg)
// to produce standalone executables for several platforms
{
  "name": "report-cli",
  "version": "1.0.0",
  "bin": "dist/index.js",
  "scripts": {
    "build": "tsc",
    "package": "pkg . --targets node22-linux-x64,node22-macos-arm64,node22-win-x64 --out-path bin"
  },
  "devDependencies": {
    "@yao-pkg/pkg": "^6.0.0",
    "typescript": "^5.7.0"
  }
}
```

Node v22 also ships an official **Single Executable Applications (SEA)** path via the built-in `node:sea` module, but it only produces a binary for the *current* platform; it cannot target a different OS or architecture:

```bash
# Node v22 SEA: build only for the machine you're on (no real cross-targeting)
node --experimental-sea-config sea-config.json
cp "$(command -v node)" report-cli            # copy the host's node binary...
npx postject report-cli NODE_SEA_BLOB sea-prep.blob \
  --sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2  # ...and inject your blob
```

The key truth: in both cases you are *shipping a Node runtime* (50-100+ MB), and even `pkg` produces large binaries because the whole interpreter rides along. There is no "build a tiny native binary for an architecture I don't own" story that is first-class in Node.

---

## Rust Equivalent

In Rust, cross-compilation is a built-in feature of the toolchain. The unit you choose is a **target triple** — a string like `x86_64-unknown-linux-musl` that encodes *CPU - vendor - OS - ABI*. You install the matching standard library with `rustup target add`, then pass `--target` to Cargo.

Here is the entire workflow for producing a fully static Linux binary that runs anywhere, including a `FROM scratch` container with no operating system libraries at all:

```bash
# 1. See what targets exist (rustc supports ~290 of them)
rustc --print target-list | wc -l       # -> 290

# 2. Add the precompiled std for your chosen target
rustup target add x86_64-unknown-linux-musl

# 3. Build for it (needs a linker for that target — see below)
cargo build --release --target x86_64-unknown-linux-musl

# 4. The artifact lands under a target-specific subdirectory:
#    target/x86_64-unknown-linux-musl/release/<binary>
```

Because the linker is the sticking point (a macOS or Windows host has no Linux linker by default), most teams reach for the **`cross`** tool, which runs the build inside a prebuilt Docker container that already contains the right linker and C toolchain:

```bash
# One-time install (a normal cargo binary)
cargo install cross

# Same command surface as cargo, but containerized:
cross build --release --target x86_64-unknown-linux-musl
cross test  --target aarch64-unknown-linux-gnu
```

This is the program we will cross-compile in the examples below:

```rust playground
// src/main.rs
fn main() {
    let arch = std::env::consts::ARCH; // e.g. "x86_64", "aarch64"
    let os = std::env::consts::OS;     // e.g. "linux", "macos", "windows"
    println!("Hello from a {arch} binary built for {os}!");
}
```

Built with the musl target inside a Linux container and then run, it prints (this is real captured output):

```text
Hello from a x86_64 binary built for linux!
```

And the binary is genuinely standalone. Running `ldd` and `file` on the musl artifact confirms it has **no dynamic dependencies at all**:

```text
$ ldd target/x86_64-unknown-linux-musl/release/greet
        statically linked

$ file target/x86_64-unknown-linux-musl/release/greet
ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV), static-pie linked, ... not stripped
```

A 547 KB self-contained executable that needs no libc, no interpreter, and no `node_modules`: that is the cross-compilation prize.

---

## Detailed Explanation

### Anatomy of a target triple

A target triple is the central concept. Despite the name, it often has four parts:

```text
x86_64 - unknown - linux  - musl
  CPU     vendor    OS       ABI / libc
```

| Triple | What it produces |
|--------|------------------|
| `x86_64-unknown-linux-gnu` | Linux on Intel/AMD, **dynamically** linked against glibc (the default on most distros) |
| `x86_64-unknown-linux-musl` | Linux on Intel/AMD, links against **musl** libc, can be fully **static** |
| `aarch64-unknown-linux-gnu` | 64-bit ARM Linux (AWS Graviton, modern Raspberry Pi OS, glibc) |
| `aarch64-apple-darwin` | Apple-silicon macOS |
| `x86_64-apple-darwin` | Intel macOS |
| `x86_64-pc-windows-msvc` | 64-bit Windows using the MSVC toolchain |
| `wasm32-unknown-unknown` | WebAssembly (covered in [Section 19](/19-wasm/)) |

Run `rustup target list` to see which are installed (marked `(installed)`) versus merely available. On a fresh Apple-silicon Mac the installed set looks like this:

```text
$ rustup target list --installed
aarch64-apple-darwin
x86_64-apple-darwin
```

### Step 1: `rustup target add` installs the *standard library*, not a compiler

`rustc` is already a cross-compiler: a single `rustc` can emit code for any of its ~290 targets. What you are *missing* for a new target is a precompiled copy of the standard library (`core`, `alloc`, `std`) for that platform. That is exactly what `rustup target add` downloads:

```text
$ rustup target add x86_64-unknown-linux-musl
info: downloading component 'rust-std' for 'x86_64-unknown-linux-musl'
info: installing component 'rust-std' for 'x86_64-unknown-linux-musl'
```

Note the component name: `rust-std`. After this, `rustc` can *compile* for the target. Whether it can *link* is a separate question.

### Step 2: the linker is the real obstacle

Compiling produces object files; **linking** stitches them into an executable and requires a linker that understands the target's binary format and a C toolchain (because `std` itself, and many crates, call into C). Your host linker usually cannot do this. If you try to build the musl target directly on macOS, compilation succeeds but linking fails. This is the real, unedited error:

```text
$ cargo build --target x86_64-unknown-linux-musl
   Compiling greet v0.1.0 (...)
 WARN ... Linker does not support -static-pie command line option. Retrying with -static instead.
error: linking with `cc` failed: exit status: 1
  |
  = note: ld: unknown options: --as-needed -Bstatic -Bdynamic --eh-frame-hdr -z --gc-sections -z -z
          clang: error: linker command failed with exit code 1 (use -v to see invocation)

error: could not compile `greet` (bin "greet") due to 1 previous error
```

`cargo` invoked the host's `cc`/`ld` (Apple's `clang`/`ld`), which does not understand the GNU-style linker flags Rust passes for a Linux ELF target. The fix is one of:

1. **Tell Cargo about a proper cross-linker** for the target (a `.cargo/config.toml` stanza), and install that linker yourself, or
2. **Use `cross`**, which supplies the whole linker + C toolchain inside Docker so you install nothing, or
3. **Build inside a native Docker container** for the target OS (effectively what `cross` automates).

### Step 3a: pointing Cargo at a cross-linker

If you install a cross-linker on the host (for example, via Homebrew or apt), you tell Cargo to use it per-target in `.cargo/config.toml`:

```toml
# .cargo/config.toml — use a cross-linker for a specific target
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
```

This file lives at the project root (or any ancestor) and Cargo reads it automatically; no flag needed. The `[target.<triple>]` table is keyed by the exact triple, so you can configure several at once.

### Step 3b: `cross` — the path of least resistance

`cross` is a thin wrapper around `cargo` that runs each build inside a target-specific Docker image maintained by the `cross-rs` project. Those images already contain the linker, the C cross-compiler, and common system libraries, so you do not install any of it on your host:

```bash
cargo install cross           # one-time
cross build --release --target x86_64-unknown-linux-musl
```

You can override the image or pass environment variables through to the container with an optional `Cross.toml` at the project root:

```toml
# Cross.toml — customize the container used for a target
[target.x86_64-unknown-linux-musl]
image = "ghcr.io/cross-rs/x86_64-unknown-linux-musl:main"

[build.env]
passthrough = ["RUST_LOG"]   # forward host env vars into the build container
```

> **Note:** `cross` requires a working container engine (Docker or Podman) running on the host. The actual `rustc`/`cargo` invocation happens *inside* the container, so your source is mounted in and the artifacts come back out under the usual `target/<triple>/` path.

### Why musl, and what "static" buys you

The `*-gnu` targets link dynamically against **glibc**, and glibc binaries are tied to the glibc version on the *build* machine: run one on an older distro and you get `version GLIBC_2.34 not found`. The `*-musl` targets instead link against **musl libc**, which supports full static linking. A static musl binary embeds *everything* it needs:

- It has zero shared-library dependencies (`ldd` prints `statically linked`).
- It runs on **any** Linux of the same architecture, regardless of distro or libc version.
- It can be dropped into a `FROM scratch` container: no base image, no OS userland.

That last point is what makes Rust + musl the gold standard for tiny containers, which the sibling [Dockerizing Rust](/24-tooling/09-docker/) builds on directly.

---

## Key Differences

| Concept | TypeScript / Node.js | Rust |
|---------|----------------------|------|
| What you ship | `.js` text + a runtime (Node) on the target | A single native binary for one target triple |
| "Cross-compile" support | Third-party (`pkg`); official SEA is host-only | First-class: `rustup target add` + `--target` |
| Binary size | 50-100+ MB (interpreter included) | KB-to-MB (just your code + `std`) |
| Runtime dependency | Node must exist or be bundled | None for a static musl build |
| The hard part | Bundling the runtime | Getting a *linker* for the target |
| Conditional code | `process.platform` checks at runtime | `#[cfg(target_os = "...")]` at compile time |
| Container base image | `node:22-slim` (~200 MB) | `scratch` (0 bytes) for static musl |

The deepest conceptual shift: in Node, *every* platform difference is resolved at runtime by the installed interpreter. In Rust, platform differences are resolved at **compile time**. You pick the target up front, and the compiler can even compile *different code* per platform:

```rust playground
// Compile-time platform branching — only the matching arm is compiled in.
fn platform_note() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "running on Linux"
    }
    #[cfg(target_os = "macos")]
    {
        "running on macOS"
    }
    #[cfg(target_os = "windows")]
    {
        "running on Windows"
    }
}

fn main() {
    println!("{}", platform_note());
}
```

Built for macOS and run, this prints `running on macOS`; the same source built for the musl target and run inside a Linux container prints `running on Linux`. Unlike a JavaScript `if (process.platform === 'linux')` check — which ships *all* branches and decides at runtime — the non-matching `#[cfg]` arms are removed entirely, so the Windows code never exists in your Linux binary. (See [Section 02](/02-basics/) for more on attributes.)

---

## Common Pitfalls

### Pitfall 1: forgetting `rustup target add`

If you pass `--target` for a triple you never installed, Cargo cannot find the standard library and the error is blunt:

```text
error[E0463]: can't find crate for `core`
  |
  = note: the `x86_64-unknown-linux-musl` target may not be installed
  = help: consider downloading the target with `rustup target add x86_64-unknown-linux-musl`
```

The fix is in the error itself: `rustup target add <triple>`. This trips up CI most often, where the runner is fresh and you must add the target as an explicit step.

### Pitfall 2: expecting `cargo build --target linux-musl` to "just work" on macOS or Windows

As shown above, compilation succeeds but **linking fails** because your host has no Linux linker. New Rust developers often read "Rust supports cross-compilation" and assume it is zero-config. It is zero-config for the *compiler*; the *linker* needs help. The pragmatic answer is `cross` (Docker) unless you have a specific reason to install a host cross-toolchain.

### Pitfall 3: confusing `*-gnu` with `*-musl`

A `*-gnu` binary built on a *newer* distro fails on an *older* one with a `GLIBC_x.yz not found` runtime error, even though it compiled and linked fine. If your goal is "build once, run on any Linux," choose `*-musl` and accept full static linking. If you specifically need glibc features (certain `dlopen` plugins, NSS-based DNS, some proprietary `.so`s), stay on `*-gnu` and build against the *oldest* glibc you must support.

> **Warning:** `getaddrinfo`-based DNS via glibc's NSS does not work in a fully static musl binary the same way. For most network apps this is fine — crates like `reqwest` use Rust resolvers — but be aware if you rely on `/etc/nsswitch.conf` behavior.

### Pitfall 4: running an `aarch64` binary on an `x86_64` host (or vice versa)

The architecture must match the hardware. A binary you cross-compiled for `aarch64-unknown-linux-gnu` will not run on an x86-64 server (`Exec format error`), and Docker will refuse a mismatched image unless you set `--platform` and have emulation (QEMU) configured. Always match the *CPU* part of the triple to where the code will actually execute.

### Pitfall 5: assuming `cross` needs nothing

`cross` needs a running container engine. If Docker/Podman is not running you get a clear failure before any compilation. Also note that `cross` builds inside a container, so anything outside your project directory (a sibling crate referenced by a relative `path =`, secrets in `$HOME`) is not visible unless you arrange for it (a `[patch]`/workspace layout, or `Cross.toml` `passthrough`).

---

## Best Practices

- **Prefer `cross` for Linux targets from a Mac/Windows dev box.** It removes the linker headache entirely and matches what CI typically does. Reserve a hand-configured `.cargo/config.toml` linker for cases where you cannot run Docker.
- **Pick the target by your deployment, not your laptop.** Containers and most cloud Linux use `x86_64-unknown-linux-gnu`; choose `*-musl` when you want a static binary for `scratch`/distroless images or "runs on any Linux."
- **Strip release binaries** to shrink them. Add this to `Cargo.toml`:

  ```toml
  [profile.release]
  strip = true        # remove symbols; smaller binary
  lto = true          # link-time optimization
  codegen-units = 1   # better optimization at the cost of build time
  ```

  (More release-profile tuning lives in [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/) and [Section 21](/21-performance/).)
- **Keep targets explicit in CI.** Add a `rustup target add` step (or use `cross`) so builds are reproducible on fresh runners. See [A Real GitHub Actions Workflow for Rust](/24-tooling/08-github-actions/) for a matrix that builds several targets in parallel.
- **Pin the target in `.cargo/config.toml`** if a project is *always* built for one cross target, so a bare `cargo build` does the right thing:

  ```toml
  # .cargo/config.toml — make this project default to the musl target
  [build]
  target = "x86_64-unknown-linux-musl"
  ```
- **Use `cargo build --target` (not environment hacks)** so artifacts land in the per-target `target/<triple>/` directory and never clobber your native build.

---

## Real-World Example

A common production task: from a developer laptop (or a CI job), produce a tiny static Linux binary for a CLI tool and ship it in a `FROM scratch` Docker image. Here is the complete, verified flow.

The tool, a trivial stand-in for a real CLI:

```rust playground
// src/main.rs
fn platform_note() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "running on Linux"
    }
    #[cfg(target_os = "macos")]
    {
        "running on macOS"
    }
    #[cfg(target_os = "windows")]
    {
        "running on Windows"
    }
}

fn main() {
    println!("{}", platform_note());
}
```

Optimize the release profile for size and self-containment:

```toml
# Cargo.toml
[profile.release]
strip = true
lto = true
codegen-units = 1
```

Cross-compile a static musl binary. Either of these works: `cross` on a Mac/Windows host, or a plain `cargo build` when you are already on Linux:

```bash
# Option A — from any host, via the cross tool (Docker-backed):
cross build --release --target x86_64-unknown-linux-musl

# Option B — on a Linux host (e.g. CI), once the target is added:
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

Confirm it is truly static (real captured output):

```text
$ ldd target/x86_64-unknown-linux-musl/release/greet
        statically linked

$ file target/x86_64-unknown-linux-musl/release/greet
ELF 64-bit LSB pie executable, x86-64, ... static-pie linked, ... not stripped
```

Now drop it into an image with **nothing else in it**:

```dockerfile
# Dockerfile — a container with literally only your binary
FROM scratch
COPY target/x86_64-unknown-linux-musl/release/greet /app
ENTRYPOINT ["/app"]
```

```bash
docker build --platform linux/amd64 -t greet:scratch .
docker run --rm --platform linux/amd64 greet:scratch
```

The container runs even though it has no shell, no libc, and no base OS at all (real captured output):

```text
running on Linux
```

And the entire image is **547 kB**: the size of the binary itself, because there is nothing else inside. Compare that to a `node:22-slim` image carrying a ~200 MB interpreter for an equivalent Node CLI. This is the combination — Rust's native compilation, musl static linking, and a `scratch` image — that makes Rust services start instantly and ship as kilobyte-sized containers. The Dockerfile mechanics (multi-stage builds, `cargo-chef` caching, distroless variants) are covered in depth in the sibling [Dockerizing Rust](/24-tooling/09-docker/).

---

## Further Reading

- [The rustc book — Platform Support](https://doc.rust-lang.org/rustc/platform-support.html) — the authoritative list of targets and their support tiers.
- [The Cargo Book — Configuration (`.cargo/config.toml`)](https://doc.rust-lang.org/cargo/reference/config.html) — `[target.<triple>]` linker keys, `[build] target`, and more.
- [The `cross` project](https://github.com/cross-rs/cross) — supported targets, `Cross.toml` reference, and prebuilt images.
- [rustup — Cross-compilation](https://rust-lang.github.io/rustup/cross-compilation.html) — managing targets with `rustup target`.
- Sibling topics in this section: [Dockerizing Rust](/24-tooling/09-docker/) (multi-stage builds and `scratch`/distroless images), [A Real GitHub Actions Workflow for Rust](/24-tooling/08-github-actions/) (build matrices that cross-compile in CI), [CI/CD Concepts for Rust](/24-tooling/07-ci-cd/) (build gates and caching), and [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/) (release profiles, `.cargo/config.toml`).
- Related sections: [Section 19: WebAssembly](/19-wasm/) (the `wasm32-*` targets), [Section 21: Performance](/21-performance/) (release-profile tuning), and [Section 25: Advanced Topics](/25-advanced-topics/).

---

## Exercises

### Exercise 1: Add a target and inspect it

**Difficulty:** Beginner

**Objective:** Get comfortable with `rustup target` and understand that adding a target installs the standard library, not a compiler.

**Instructions:**

1. Run `rustup target list --installed` and note your current targets.
2. Add the ARM64 Linux target: `rustup target add aarch64-unknown-linux-gnu`.
3. Run `rustup target list --installed` again and confirm the new triple appears.
4. In a new `cargo new` project, run `rustc --print target-list | wc -l` and report how many targets `rustc` knows about.

<details>
<summary>Solution</summary>

```bash
# 1. Before
rustup target list --installed
# e.g.
# aarch64-apple-darwin
# x86_64-apple-darwin

# 2. Add the target (downloads the precompiled std, component "rust-std")
rustup target add aarch64-unknown-linux-gnu
# info: downloading component 'rust-std' for 'aarch64-unknown-linux-gnu'
# info: installing component 'rust-std' for 'aarch64-unknown-linux-gnu'

# 3. After — the new triple is now listed
rustup target list --installed
# aarch64-apple-darwin
# aarch64-unknown-linux-gnu   <-- newly added
# x86_64-apple-darwin

# 4. How many targets rustc supports (real output: 290 on stable 1.96.x):
rustc --print target-list | wc -l
# 290
```

The key insight: `rustup target add` installed only `rust-std` (the standard library), because the single `rustc` you already have is itself a full cross-compiler.

</details>

---

### Exercise 2: Produce a static musl binary and prove it is static

**Difficulty:** Intermediate

**Objective:** Cross-compile a real static Linux binary and verify it has no dynamic dependencies.

**Instructions:**

1. Create a project with the `greet` program from this page (it prints the arch and OS).
2. Cross-compile it for `x86_64-unknown-linux-musl`. Use `cross` if you are on macOS/Windows, or `cargo build --target ...` if you are on Linux.
3. Run `file` and `ldd` on the resulting binary and confirm it reports `statically linked`.
4. Explain in one sentence why a `*-musl` build can be fully static but a default `*-gnu` build typically is not.

<details>
<summary>Solution</summary>

```bash
cargo new greet && cd greet
```

```rust playground
// src/main.rs
fn main() {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    println!("Hello from a {arch} binary built for {os}!");
}
```

```bash
# On macOS/Windows (Docker-backed, no host linker needed):
cargo install cross
cross build --release --target x86_64-unknown-linux-musl

# On Linux:
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

Verify (real captured output):

```text
$ ldd target/x86_64-unknown-linux-musl/release/greet
        statically linked

$ file target/x86_64-unknown-linux-musl/release/greet
ELF 64-bit LSB pie executable, x86-64, ... static-pie linked, ... not stripped
```

**Why musl can be fully static:** the musl C library is designed to be linked statically, so Rust can bundle all of libc into the executable; glibc (the `*-gnu` default) discourages and partially breaks static linking, so `*-gnu` binaries remain dynamically linked against the host's `libc.so` and are tied to its version.

</details>

---

### Exercise 3: Configure a project to default to a cross target

**Difficulty:** Advanced

**Objective:** Use `.cargo/config.toml` so that a bare `cargo build` (no `--target` flag) cross-compiles, and combine it with a release profile tuned for small static binaries.

**Instructions:**

1. In a project, add a `[profile.release]` section to `Cargo.toml` that strips symbols, enables LTO, and sets `codegen-units = 1`.
2. Add a `.cargo/config.toml` with a `[build] target = "x86_64-unknown-linux-musl"` line so the target is the default.
3. Explain what `cargo build --release` now does differently, and one reason you might *not* want this committed to a shared repository.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[package]
name = "greet"
version = "0.1.0"
edition = "2024"

[profile.release]
strip = true
lto = true
codegen-units = 1
```

```toml
# .cargo/config.toml
[build]
target = "x86_64-unknown-linux-musl"
```

```bash
# No --target needed anymore; Cargo reads [build] target from .cargo/config.toml
cargo build --release
# Artifacts land in target/x86_64-unknown-linux-musl/release/
```

**What changed:** `cargo build --release` now defaults to the musl triple instead of the host, producing a stripped, LTO-optimized static binary under `target/x86_64-unknown-linux-musl/release/`.

**Why you might not commit it:** this forces *every* developer (and every `cargo test`, `cargo run`, `rust-analyzer` check) onto the musl target, which needs that target installed and a working cross-linker (or `cross`). On a Mac without a Linux linker, a plain `cargo run` would suddenly fail to link. Teams often keep the cross target out of the shared `.cargo/config.toml` and instead select it explicitly in CI or release scripts, leaving day-to-day local builds on the native host target.

</details>
