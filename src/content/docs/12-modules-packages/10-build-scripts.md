---
title: "Build Scripts: Code Generation and Native Linking with `build.rs`"
description: "build.rs is Rust's compile-time hook, like an npm prebuild or node-gyp: generate code into OUT_DIR, link C with the cc crate, and control reruns with cargo:: lines."
---

A **build script** (`build.rs`) is Rust code that Cargo compiles and runs *before* it builds your crate. It is the closest thing Rust has to a `prebuild`/`postinstall` npm hook or a `node-gyp` step, but it is plain Rust, it talks to Cargo over `stdout`, and it is sandboxed to a private output directory.

---

## Quick Overview

In the Node.js world, anything you need to happen before your code runs (generating files, compiling a native addon, downloading a binary) lives in a `scripts` hook (`prebuild`, `postinstall`) or a tool like `node-gyp`. Rust folds all of that into a single optional file named `build.rs` at the crate root: Cargo compiles it, runs it once per build, and listens to special `cargo::` lines it prints to decide what to compile, what native libraries to link, and when to re-run. This page focuses on the three jobs build scripts do most: **generating Rust source code**, **compiling and linking native (C/C++) libraries**, and **controlling re-runs** with `cargo::rerun-if-*` directives.

> **Note:** A build script is *not* a place to run your app's logic. It runs at compile time on the *build* machine, not at runtime. Think `tsc` plugin or `node-gyp`, not `node index.js`.

---

## TypeScript/JavaScript Example

In a Node project, build-time work is wired through `package.json` lifecycle scripts and sometimes a helper script that generates code:

```json
// package.json
{
  "name": "status-lib",
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "generate": "node scripts/generate-status.mjs",
    "prebuild": "npm run generate",
    "build": "tsc"
  },
  "devDependencies": {
    "typescript": "^5.5.0"
  }
}
```

```javascript
// scripts/generate-status.mjs — run before `tsc` to emit a TS file from data
import { readFile, writeFile } from "node:fs/promises";

const raw = await readFile(new URL("../status_codes.json", import.meta.url), "utf8");
const map = JSON.parse(raw);

let out = "// AUTO-GENERATED — do not edit\n";
out += "export function reasonPhrase(code: number): string | undefined {\n";
out += "  switch (code) {\n";
for (const [code, reason] of Object.entries(map)) {
  out += `    case ${code}: return ${JSON.stringify(reason)};\n`;
}
out += "    default: return undefined;\n  }\n}\n";

await writeFile(new URL("../src/status.generated.ts", import.meta.url), out);
```

This works, but notice the friction: the script writes *into your source tree* (`src/status.generated.ts`), you must remember to `.gitignore` it, and `prebuild` runs every single time regardless of whether the input changed. For native addons you would reach for an entirely separate toolchain (`node-gyp` + a `binding.gyp` file).

---

## Rust Equivalent

The same code-generation task as a Cargo build script. The generated file goes into a Cargo-managed scratch directory (`OUT_DIR`), never your source tree:

```toml
# Cargo.toml
[package]
name = "status-lib"
version = "1.0.0"
edition = "2024"

[build-dependencies]
serde_json = "1"
```

```rust
// build.rs — Cargo compiles and runs this before building the crate
use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn main() {
    // Cargo hands every build script a private scratch directory via OUT_DIR.
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("status.rs");

    let raw = fs::read_to_string("status_codes.json").expect("status_codes.json");
    let map: BTreeMap<u16, String> =
        serde_json::from_str(&raw).expect("valid JSON object of code->reason");

    // Build a `match`-based lookup function as a String of Rust source.
    let mut code = String::from(
        "pub fn reason_phrase(code: u16) -> Option<&'static str> {\n    match code {\n",
    );
    for (status, reason) in &map {
        // {reason:?} prints the &str with quotes + escaping — valid Rust literal.
        writeln!(code, "        {status} => Some({reason:?}),").unwrap();
    }
    code.push_str("        _ => None,\n    }\n}\n");
    fs::write(&dest, code).unwrap();

    // Re-run ONLY when these inputs change (see the directives section below).
    println!("cargo::rerun-if-changed=status_codes.json");
    println!("cargo::rerun-if-changed=build.rs");
}
```

```rust
// src/lib.rs — pull the generated file in at compile time
include!(concat!(env!("OUT_DIR"), "/status.rs"));

#[cfg(test)]
mod tests {
    #[test]
    fn known_code() {
        assert_eq!(super::reason_phrase(404), Some("Not Found"));
    }
}
```

Running `cargo build` produces (real output from a `cargo run` driver of this exact code):

```text
   Compiling status-lib v1.0.0 (/private/tmp/.../status-lib)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.03s
200 => Some("OK")
404 => Some("Not Found")
418 => None
```

The generated `status.rs` lives under `target/`, so it is gitignored automatically and never pollutes your source tree.

---

## Detailed Explanation

A build script is **convention over configuration**: if a file named `build.rs` exists at the crate root, Cargo automatically compiles it as a separate tiny binary and runs it before compiling your crate. There is no manifest entry to add (though you can point at a different file with `build = "path/to/build.rs"` under `[package]`).

Walking through the moving parts:

- **`[build-dependencies]`** is a separate dependency table just for the build script. Crates listed here are compiled for the *host* (build) machine and are **not** linked into your final binary. `serde_json` here is used only during the build. This is distinct from `[dependencies]` and `[dev-dependencies]`; see [Dev & Build Dependencies](/12-modules-packages/07-dev-dependencies/).

- **`OUT_DIR`** is an environment variable Cargo sets for the build script, pointing at a unique per-crate directory like `target/debug/build/status-lib-<hash>/out`. You write generated files *there*, never into `src/`. The `unwrap()` is safe because Cargo always sets it.

- **Communication is via `stdout`.** The build script does not return data through a function. Instead it *prints* lines beginning with `cargo::` (note the **double colon** — the modern syntax). Cargo parses those lines and acts on them. Lines that do not start with `cargo::` are treated as ordinary log output.

- **`include!(concat!(env!("OUT_DIR"), "/status.rs"))`** is the bridge back into your crate. Here `env!` is a *compile-time* macro (it reads the env var while `rustc` runs, not at program startup), `concat!` builds the path literal, and `include!` textually pastes the generated tokens into your module, exactly as if you had typed them. This is the standard pattern for consuming generated code.

- **`{reason:?}`** in the `writeln!` uses Rust's `Debug` formatting for `&str`, which emits a properly quoted and escaped string literal. Generating source as text means you must produce *syntactically valid Rust*; `Debug` formatting of strings does the escaping for you.

Contrast with the Node version: there is no `prebuild` hook to wire up, the output never touches your source tree, and, most importantly, the `cargo::rerun-if-changed` lines let Cargo *skip* the script entirely when nothing relevant changed. The Node `prebuild` runs unconditionally.

> **Tip:** The build script is just Rust. You can unit-test the generation logic by factoring it into a function, and you get the full standard library plus any `[build-dependencies]`.

---

## Compiling and Linking Native Libraries

The second major job of build scripts is building and linking C/C++ code: Rust's answer to `node-gyp`. The community standard is the [`cc`](https://docs.rs/cc) crate, which locates a C compiler, compiles your sources into a static library, and emits the correct link directives automatically.

```toml
# Cargo.toml
[package]
name = "native_math"
version = "0.1.0"
edition = "2024"

[build-dependencies]
cc = "1.2"
```

```c
/* csrc/checksum.c — a small Fletcher-16 checksum in C */
#include <stddef.h>
#include <stdint.h>

uint16_t fletcher16(const uint8_t *data, size_t len) {
    uint16_t sum1 = 0, sum2 = 0;
    for (size_t i = 0; i < len; i++) {
        sum1 = (sum1 + data[i]) % 255;
        sum2 = (sum2 + sum1) % 255;
    }
    return (sum2 << 8) | sum1;
}
```

```rust
// build.rs
fn main() {
    // cc finds a C compiler, builds the source into a static lib, AND emits
    // the cargo::rustc-link-* directives for you. No manual linking needed.
    cc::Build::new()
        .file("csrc/checksum.c")
        .compile("checksum");

    // Recompile the C only when the C source changes.
    println!("cargo::rerun-if-changed=csrc/checksum.c");
}
```

```rust
// src/main.rs — call the C function through an FFI declaration
// In edition 2024, `extern` blocks MUST be marked `unsafe`.
unsafe extern "C" {
    fn fletcher16(data: *const u8, len: usize) -> u16;
}

fn checksum(bytes: &[u8]) -> u16 {
    // SAFETY: we pass a valid pointer + matching length from a live slice.
    unsafe { fletcher16(bytes.as_ptr(), bytes.len()) }
}

fn main() {
    let sum = checksum(b"abcde");
    println!("fletcher16(\"abcde\") = {sum:#06x}");
}
```

Real output from `cargo run`:

```text
   Compiling cc v1.2.63
   Compiling native_math v0.1.0 (/private/tmp/.../native_math)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.11s
     Running `target/debug/native_math`
fletcher16("abcde") = 0xc8f0
```

Under `cargo build -vv` (very verbose) you can see the directives `cc` emits on the build script's behalf. These are the real lines from this build:

```text
[native_math 0.1.0] cargo:rustc-link-lib=static=checksum
[native_math 0.1.0] cargo:rustc-link-search=native=.../build/native_math-<hash>/out
```

If you are linking against a *pre-installed* system library rather than compiling your own C, you skip `cc` and print the directives yourself:

```rust
// build.rs — link against a system-installed library (e.g. libz)
fn main() {
    // Tell the linker to link `libz` dynamically.
    println!("cargo::rustc-link-lib=dylib=z");
    // Add a directory to the linker's search path if it is non-standard.
    println!("cargo::rustc-link-search=native=/usr/local/lib");
}
```

> **Note:** FFI, `unsafe`, and raw pointers are a large topic in their own right, covered in depth in section [20 — Unsafe & FFI](/20-unsafe-ffi/). The build-script side is just the link wiring; this page stays in that lane.

---

## The `cargo::rerun-if-*` Directives

By default, **a build script re-runs whenever any file in the package changes**. That is rarely what you want — it makes builds slower and can re-run expensive code generation needlessly. The moment you print *any* `cargo::rerun-if-*` line, you switch off the default and take explicit control: Cargo re-runs the script only if one of the conditions you listed is met.

| Directive | Re-run when... | TS/JS analogy |
| --- | --- | --- |
| `cargo::rerun-if-changed=PATH` | the file or directory at `PATH` changes | a watched input in a `chokidar` glob |
| `cargo::rerun-if-env-changed=VAR` | the value of env var `VAR` differs from the last build | reading `process.env.X` and reacting to it |

```rust
// build.rs — explicit, minimal re-run triggers
use std::env;

fn main() {
    // Read configuration from the environment, with a default.
    let api_base = env::var("API_BASE_URL")
        .unwrap_or_else(|_| "https://api.example.com".to_string());

    // Expose it to the crate as a compile-time env var read with env!().
    println!("cargo::rustc-env=API_BASE_URL={api_base}");

    // Re-run if the env var changes between builds...
    println!("cargo::rerun-if-env-changed=API_BASE_URL");
    // ...and if the build script itself changes.
    println!("cargo::rerun-if-changed=build.rs");
}
```

```rust
// src/main.rs
fn main() {
    println!("API base: {}", env!("API_BASE_URL"));
}
```

Real output across two builds:

```text
--- default ---
API base: https://api.example.com
--- overridden ---
API base: https://staging.internal
```

> **Warning:** Once you print **one** `rerun-if-changed`, the *blanket* "re-run on any file change" behavior is disabled. If your script reads three input files, you must list **all three** `rerun-if-changed` lines, or edits to the unlisted files will be silently ignored until something else triggers a rebuild.

Two more directives round out the common set:

- **`cargo::rustc-env=KEY=VALUE`** sets an env var that your crate reads with the `env!("KEY")` macro at compile time. This is the cleanest way to embed small values like a git hash or build timestamp without writing a file.
- **`cargo::warning=MESSAGE`** surfaces a warning in the build output, useful for nudging users about missing optional tooling.

```rust
// build.rs — embed the git commit hash, no file generation needed
use std::process::Command;

fn main() {
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo::rustc-env=GIT_HASH={git_hash}");
    println!("cargo::rerun-if-changed=.git/HEAD"); // re-run when HEAD moves
}
```

```rust
// src/main.rs
fn main() {
    println!("version {} (commit {})",
        env!("CARGO_PKG_VERSION"), // Cargo sets this one for you
        env!("GIT_HASH"));         // we set this one in build.rs
}
```

Real output: `version 0.1.0 (commit c530f7d)`.

---

## Key Differences

| Concept | Node.js / TypeScript | Rust (`build.rs`) |
| --- | --- | --- |
| Where build logic lives | `scripts` in `package.json` + helper `.mjs` files | a single `build.rs` at the crate root (auto-detected) |
| Language | JavaScript / shell | plain Rust, with `[build-dependencies]` |
| When it runs | on the lifecycle event (`prebuild`, `postinstall`) | once before each crate build, *if* re-run conditions are met |
| Incremental skipping | none built in (runs every time) | `cargo::rerun-if-*` lets Cargo skip it |
| Output location | wherever your script writes (often into `src/`) | the Cargo-managed `OUT_DIR` under `target/` |
| Native addons | `node-gyp` + `binding.gyp` (separate toolchain) | the `cc` crate (or your own link directives) |
| Talking to the build tool | exit code / writing files | printing `cargo::...` lines to `stdout` |
| Consuming generated code | `import` the emitted file | `include!(concat!(env!("OUT_DIR"), "/file.rs"))` |

The deepest difference is **integration**. In Node, `prebuild` is a string of shell that npm runs blindly; it has no idea what your script reads or produces, so it cannot skip work. Cargo's build script is a first-class citizen: it declares its inputs (`rerun-if-changed`), its outputs go to a directory Cargo owns and cleans, and its effect on the *real* compile (link flags, cfg flags, env vars) is communicated through a typed protocol. The build graph stays correct and incremental.

> **Note:** The directive syntax changed from a single colon (`cargo:rustc-env=...`) to a double colon (`cargo::rustc-env=...`) when the new form stabilized in Rust 1.77. The single-colon form still works for backward compatibility, but **prefer the double colon** in new code, and use it consistently. This guide targets the latest stable Rust (1.96.0) and the 2024 edition.

---

## Common Pitfalls

### Pitfall 1: Writing generated files into `src/`

Coming from the Node `prebuild` habit of writing `src/status.generated.ts`, it is tempting to write generated Rust into your source tree. Don't. Always write to `OUT_DIR`. Files in `src/` get committed, fight with formatting/linting, and break reproducible builds. Files in `OUT_DIR` live under `target/`, are gitignored by default, and are regenerated cleanly.

### Pitfall 2: Forgetting that one `rerun-if-changed` disables the blanket re-run

If your script reads `data/a.csv` and `data/b.csv` but only prints `cargo::rerun-if-changed=data/a.csv`, then editing `b.csv` will **not** trigger a rebuild. List every input, or point at the directory: `cargo::rerun-if-changed=data`.

### Pitfall 3: Mistyping a directive key

Cargo validates `cargo::` keys. A typo produces a hard error, not a silent no-op. This is the real message for an unknown key:

```rust
// build.rs
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::totally-made-up-key=oops"); // does not compile/build
}
```

```text
error: invalid output in build script of `buildgen v0.1.0 (...)`: `cargo::totally-made-up-key=oops`
Unknown key: `totally-made-up-key`.
See https://doc.rust-lang.org/cargo/reference/build-scripts.html#outputs-of-the-build-script for more information about build script outputs.
```

### Pitfall 4: Forgetting `unsafe` on `extern` blocks in edition 2024

In the 2024 edition, FFI declaration blocks must be marked `unsafe extern "C"`. Writing a plain `extern "C"` block is a compile error. This is the real `rustc` message:

```rust
// does not compile in edition 2024 — needs `unsafe extern "C"`
extern "C" {
    fn fletcher16(data: *const u8, len: usize) -> u16;
}
```

```text
error: extern blocks must be unsafe
 --> src/main.rs:1:1
  |
1 | / extern "C" {
2 | |     fn fletcher16(data: *const u8, len: usize) -> u16;
3 | | }
  | |_^
```

The fix is to write `unsafe extern "C" { ... }`. Note that *declaring* the block `unsafe` is separate from the `unsafe { ... }` block you still need at each *call* site.

### Pitfall 5: Expecting `build.rs` to run at program runtime

A build script runs at compile time on the build machine. It cannot read your end user's environment, cannot make runtime network calls for your app, and its `println!` output is build-log noise, not your program's output. If you need runtime behavior, that belongs in `src/`, not `build.rs`.

---

## Best Practices

- **Reach for a crate before hand-rolling.** Use `cc` for C/C++, `bindgen` for generating Rust FFI bindings from C headers, `pkg-config` for discovering system libraries, and `prost-build`/`tonic-build` for Protocol Buffers. They emit the correct directives for you and handle cross-platform quirks.
- **Always write to `OUT_DIR`.** Never generate into `src/`.
- **Be specific about re-runs.** List exactly the files and env vars your script depends on with `cargo::rerun-if-changed` / `cargo::rerun-if-env-changed`. This keeps incremental builds fast and correct.
- **Prefer `cargo::rustc-env` over file generation for scalar values.** A git hash or build timestamp does not need a generated `.rs` file; one `rustc-env` line plus `env!()` is cleaner.
- **Keep build scripts cheap and deterministic.** They run on every fresh build and on CI. Avoid slow network calls; if you must fetch, cache into `OUT_DIR` and guard with re-run directives.
- **Add only build-time crates to `[build-dependencies]`.** They are compiled for the host and excluded from your final binary; keep your runtime dependency tree lean.
- **Emit a friendly `cargo::warning`** when an optional native dependency is missing, so users get a clear message instead of a cryptic linker error.

> **Tip:** Reserve build scripts for things that genuinely must happen at build time. If a problem can be solved with a regular function, a `const`, a [feature flag](/12-modules-packages/09-feature-flags/), or a procedural macro (see [14 — Macros](/14-macros/)), prefer that — build scripts add a compile step and a maintenance surface.

---

## Real-World Example

A common production need: turn a checked-in data file into a fast, allocation-free, compile-time lookup table. Here we generate a `match`-based function from a CSV of color names: no runtime parsing, no `HashMap` allocation, and a hard compile error if the data file is malformed.

```toml
# Cargo.toml
[package]
name = "color_codes"
version = "0.1.0"
edition = "2024"
# No [build-dependencies] needed — std is enough for CSV this simple.
```

```text
# colors.csv (checked into the repo)
red,#ff0000
green,#00ff00
blue,#0000ff
slate,#708090
```

```rust
// build.rs
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("colors.rs");

    let csv = fs::read_to_string("colors.csv").expect("colors.csv must exist");

    // Generate a `match`-based lookup function from the rows.
    let mut code = String::from(
        "/// Look up a hex code for a named color (generated from colors.csv).\n\
         pub fn hex_for(name: &str) -> Option<&'static str> {\n    match name {\n",
    );
    for line in csv.lines().filter(|l| !l.trim().is_empty()) {
        let (name, hex) = line.split_once(',').expect("each row is name,hex");
        writeln!(
            code,
            "        {name:?} => Some({hex:?}),",
            name = name.trim(),
            hex = hex.trim()
        )
        .unwrap();
    }
    code.push_str("        _ => None,\n    }\n}\n");
    fs::write(&dest, code).unwrap();

    // Regenerate only when the data or the generator changes.
    println!("cargo::rerun-if-changed=colors.csv");
    println!("cargo::rerun-if-changed=build.rs");
}
```

```rust
// src/main.rs
include!(concat!(env!("OUT_DIR"), "/colors.rs"));

fn main() {
    for name in ["slate", "blue", "chartreuse"] {
        match hex_for(name) {
            Some(hex) => println!("{name:>10} -> {hex}"),
            None => println!("{name:>10} -> (unknown)"),
        }
    }
}
```

Real output from `cargo run`:

```text
   Compiling color_codes v0.1.0 (/private/tmp/.../color_codes)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.69s
     Running `target/debug/color_codes`
     slate -> #708090
      blue -> #0000ff
chartreuse -> (unknown)
```

The payoff over a runtime approach: the lookup compiles to a jump table, there is zero startup cost, and a malformed `colors.csv` fails the *build* (via the `expect` panics in `build.rs`) rather than crashing at runtime. Edit `colors.csv` and `cargo build` regenerates and recompiles automatically; edit an unrelated file and the script is skipped.

---

## Further Reading

- [The Cargo Book — Build Scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html): the authoritative reference for `build.rs` and every `cargo::` directive.
- [The Cargo Book — Build Script Examples](https://doc.rust-lang.org/cargo/reference/build-script-examples.html): code generation, linking, and conditional compilation recipes.
- [`cc` crate docs](https://docs.rs/cc): compiling C/C++ from a build script.
- [`bindgen` User Guide](https://rust-lang.github.io/rust-bindgen/): generating Rust FFI bindings from C headers in `build.rs`.
- Related pages in this section:
  - [Cargo.toml: The Manifest](/12-modules-packages/04-cargo/): where `[build-dependencies]` and `build = "..."` are declared.
  - [Dev & Build Dependencies](/12-modules-packages/07-dev-dependencies/): how `[build-dependencies]` differs from `[dependencies]` and `[dev-dependencies]`.
  - [Dependencies](/12-modules-packages/06-dependencies/): adding crates like `cc` and `serde_json` with `cargo add`.
  - [Feature Flags & Conditional Compilation](/12-modules-packages/09-feature-flags/): `#[cfg(...)]` and an often-better alternative to build-script logic.
  - [Cargo Commands](/12-modules-packages/05-cargo-commands/): `cargo build -vv` to inspect build-script output.
- Cross-section links:
  - [01 — Getting Started](/01-getting-started/) and [Understanding Cargo](/01-getting-started/03-cargo-basics/) for Cargo fundamentals.
  - [02 — Basics](/02-basics/) for the `println!`/`format!` macros used to emit generated code.
  - [20 — Unsafe & FFI](/20-unsafe-ffi/) for the `unsafe extern "C"` and raw-pointer details behind native linking.
  - [13 — Testing](/13-testing/) for testing the code your build script generates.

---

## Exercises

### Exercise 1: Embed a build timestamp

**Difficulty:** Easy

**Objective:** Use a build script to bake the build's Unix timestamp into the binary, with no generated file.

**Instructions:**

1. Create a binary crate.
2. In `build.rs`, compute the current time as seconds since the Unix epoch.
3. Expose it to the crate via `cargo::rustc-env=BUILD_UNIX_TIME=...`.
4. In `main`, print it using the `env!` macro.

<details>
<summary>Solution</summary>

```rust
// build.rs
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo::rustc-env=BUILD_UNIX_TIME={secs}");
    println!("cargo::rerun-if-changed=build.rs");
}
```

```rust
// src/main.rs
fn main() {
    println!("built at unix time {}", env!("BUILD_UNIX_TIME"));
}
```

Real output: `built at unix time 1780203193`.

</details>

### Exercise 2: Configurable API base URL with re-run control

**Difficulty:** Medium

**Objective:** Read an environment variable in `build.rs`, fall back to a default, and make Cargo re-run the script when that variable changes.

**Instructions:**

1. In `build.rs`, read `API_BASE_URL` from the environment, defaulting to `https://api.example.com`.
2. Expose it via `cargo::rustc-env`.
3. Print `cargo::rerun-if-env-changed=API_BASE_URL` so changing it triggers a rebuild.
4. Verify that `cargo run` uses the default and `API_BASE_URL=... cargo run` uses the override.

<details>
<summary>Solution</summary>

```rust
// build.rs
use std::env;

fn main() {
    let api_base = env::var("API_BASE_URL")
        .unwrap_or_else(|_| "https://api.example.com".to_string());
    println!("cargo::rustc-env=API_BASE_URL={api_base}");
    println!("cargo::rerun-if-env-changed=API_BASE_URL");
    println!("cargo::rerun-if-changed=build.rs");
}
```

```rust
// src/main.rs
fn main() {
    println!("API base: {}", env!("API_BASE_URL"));
}
```

Real output:

```text
--- default ---
API base: https://api.example.com
--- overridden ---
API base: https://staging.internal
```

</details>

### Exercise 3: Generate a lookup table from a JSON data file

**Difficulty:** Hard

**Objective:** Use a `[build-dependencies]` crate (`serde_json`) to parse a JSON file and generate a `match`-based lookup function written into `OUT_DIR`.

**Instructions:**

1. Add `serde_json` to `[build-dependencies]` with `cargo add --build serde_json`.
2. Create `status_codes.json` mapping HTTP codes (as string keys) to reason phrases.
3. In `build.rs`, parse it into a `BTreeMap<u16, String>`, generate `fn reason_phrase(code: u16) -> Option<&'static str>`, and write it to `OUT_DIR/status.rs`.
4. `include!` the generated file and look up a few codes (including one not in the file).
5. Add `cargo::rerun-if-changed` for both the JSON and `build.rs`.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[build-dependencies]
serde_json = "1"
```

```rust
// build.rs
use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("status.rs");

    let raw = fs::read_to_string("status_codes.json").expect("status_codes.json");
    let map: BTreeMap<u16, String> =
        serde_json::from_str(&raw).expect("valid JSON object of code->reason");

    let mut code = String::from(
        "pub fn reason_phrase(code: u16) -> Option<&'static str> {\n    match code {\n",
    );
    for (status, reason) in &map {
        writeln!(code, "        {status} => Some({reason:?}),").unwrap();
    }
    code.push_str("        _ => None,\n    }\n}\n");
    fs::write(&dest, code).unwrap();

    println!("cargo::rerun-if-changed=status_codes.json");
    println!("cargo::rerun-if-changed=build.rs");
}
```

```json
// status_codes.json
{ "200": "OK", "404": "Not Found", "500": "Internal Server Error" }
```

```rust
// src/main.rs
include!(concat!(env!("OUT_DIR"), "/status.rs"));

fn main() {
    for code in [200u16, 404, 418] {
        println!("{code} => {:?}", reason_phrase(code));
    }
}
```

Real output:

```text
200 => Some("OK")
404 => Some("Not Found")
418 => None
```

</details>
