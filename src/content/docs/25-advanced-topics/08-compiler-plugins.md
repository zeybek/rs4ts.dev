---
title: "Compiler Plugins, Build Scripts, and What Needs Nightly"
description: "Proc macros and build.rs are Rust's type-safe answer to Babel plugins and npm prebuild scripts: how they hook into the compiler, and what still needs nightly."
---

## Quick Overview

"Compiler plugins" is the loose name for code that hooks into the Rust build to **generate or transform other code before the final compile**: chiefly **procedural macros** (Rust programs that run *inside* the compiler) and **build scripts** (`build.rs`, a program Cargo runs *before* the compile). This is the part of Rust that does the work a TypeScript developer would reach for Babel plugins, `ts-node` transformers, `tsc` custom transforms, code generators, or `prebuild`/`postbuild` npm scripts to do, except it is type-safe, hygienic, runs on every build, and produces zero-overhead native code.

This page is about the **tooling and codegen mechanics**: how a proc macro plugs into the compiler, how `build.rs` participates in the build graph, and the dividing line between what stable Rust can do today and what still requires the **nightly** toolchain. The deep mechanics of *writing* a proc macro with `syn`/`quote` live in [Section 14: Procedural Macros](/14-macros/07-proc-macros/); here we focus on the compiler-integration angle.

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. Every Rust snippet below was compiled and run on a stable toolchain (with `syn` 2, `quote` 1, `proc-macro2` 1 for the macro example); the nightly-only snippet was compiled with a nightly `rustc` to capture the real error and the real success output.

---

## TypeScript/JavaScript Example

In the JavaScript/TypeScript world, "make the build do extra work for me" splits into two families that map almost one-to-one onto Rust's two mechanisms.

**1. Transform the source as it compiles.** A Babel plugin or a `tsc` custom transformer walks the AST and rewrites it. Here is a Babel-style plugin that, for every class, generates a sibling `xxxPath` constant from a decorator-like comment:

```typescript
// babel-style transformer — runs during the BUILD, on the AST.
import type { PluginObj } from "@babel/core";

// Rewrites a function annotated with `// @route /users/:id` so that an extra
// `getUserPath()` function is emitted alongside it. This is codegen: new
// source is produced and then type-checked like anything else.
export default function routePlugin(): PluginObj {
  return {
    name: "route-codegen",
    visitor: {
      FunctionDeclaration(path) {
        const leading = path.node.leadingComments ?? [];
        const tag = leading.find((c) => c.value.trim().startsWith("@route "));
        if (!tag) return;

        const route = tag.value.trim().slice("@route ".length);
        const name = path.node.id?.name ?? "anon";
        // Append a generated function to the program body.
        path.insertAfter(
          // (pseudo) build an AST node equivalent to:
          //   export function ${name}Path() { return "${route}"; }
          buildPathFn(name, route),
        );
      },
    },
  };
}
```

**2. Run a program before/around the compile** — `package.json` lifecycle scripts:

```json
{
  "scripts": {
    "prebuild": "node scripts/gen-version.js && node-gyp rebuild",
    "build": "tsc"
  }
}
```

```javascript
// scripts/gen-version.js — runs as a `prebuild` step, writing a source file
// that the real build then imports. Classic "bake the git hash in" pattern.
import { execSync } from "node:child_process";
import { writeFileSync } from "node:fs";

const hash = execSync("git rev-parse --short HEAD").toString().trim();
writeFileSync("src/version.generated.ts", `export const GIT_HASH = "${hash}";\n`);
```

The first family rewrites code; the second compiles native addons (`node-gyp`) and generates source. Rust has a direct, first-class answer for each.

---

## Rust Equivalent

### Family 1 — proc macros: programs that run inside the compiler

A **procedural macro** is the Rust analogue of a Babel transform. It lives in a special crate type (`proc-macro = true`), receives the annotated code as a `TokenStream`, and returns a new `TokenStream` that the compiler type-checks. Unlike Babel, it is **type-aware-adjacent, hygienic, and emits real Rust**. There is no separate runtime step.

Here is the exact analogue of the Babel `routePlugin`: an attribute macro `#[route("/users/{id}")]` that keeps the original function and **generates** a `<name>_path()` function next to it.

```rust
// route_macro/src/lib.rs  — a crate with `proc-macro = true` in Cargo.toml.
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, LitStr};

/// `#[route("/users/{id}")]` runs INSIDE the compiler. It reads the function
/// it is attached to and emits a second, generated function `<name>_path()`
/// returning the route string, plus the original function unchanged. This is
/// code generation, not reflection: the new function is real source that the
/// compiler then type-checks.
#[proc_macro_attribute]
pub fn route(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the attribute argument as a string literal, and the item as a fn.
    let path = parse_macro_input!(attr as LitStr);
    let func = parse_macro_input!(item as ItemFn);

    let fn_name = &func.sig.ident;
    let path_fn = syn::Ident::new(&format!("{fn_name}_path"), fn_name.span());

    // `quote!` is a templating macro for Rust syntax. `#x` interpolates.
    let expanded = quote! {
        #func

        pub fn #path_fn() -> &'static str {
            #path
        }
    };

    expanded.into()
}
```

The `Cargo.toml` that makes the crate a compiler plugin is tiny but mandatory:

```toml
# route_macro/Cargo.toml
[package]
name = "route_macro"
version = "0.1.0"
edition = "2024"

[lib]
proc-macro = true        # <- this is what lets the crate run in the compiler

[dependencies]
syn = { version = "2", features = ["full"] }
quote = "1"
proc-macro2 = "1"
```

A consumer crate depends on it like any library and applies the attribute:

```rust
// app/src/main.rs
use route_macro::route;

#[route("/users/{id}")]
fn get_user(id: u32) -> String {
    format!("user #{id}")
}

#[route("/health")]
fn health() -> String {
    "ok".to_string()
}

fn main() {
    // The original functions still exist...
    println!("{}", get_user(42));
    println!("{}", health());

    // ...and the macro GENERATED these `*_path` functions at compile time.
    println!("route: {}", get_user_path());
    println!("route: {}", health_path());
}
```

Real output from `cargo run`:

```text
user #42
ok
route: /users/{id}
route: /health
```

### Family 2 — build scripts: a program Cargo runs before the compile

A **build script** is a file named `build.rs` in the package root. Cargo compiles and runs it *before* compiling the crate, exactly like an npm `prebuild` step, but it communicates with Cargo through structured `println!` directives rather than by convention. Here is the Rust version of "bake the git hash in":

```rust playground
// build.rs
use std::process::Command;

fn main() {
    // Try to capture the current git commit. Fall back gracefully so the
    // build never fails on a machine without git or outside a repo.
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Expose it to the crate as a compile-time env var, readable via env!().
    println!("cargo::rustc-env=GIT_HASH={git_hash}");

    // Rebuild if HEAD moves (new commit) so the baked-in value stays fresh.
    println!("cargo::rerun-if-changed=.git/HEAD");
}
```

```rust
// src/main.rs
fn main() {
    // `env!` reads an environment variable AT COMPILE TIME and bakes the
    // value into the binary as a &'static str.
    println!("built from commit {}", env!("GIT_HASH"));
}
```

Real output, run once inside a git repo and once with no repo present:

```text
=== inside a git repo ===
built from commit 7eb8bda
=== with no git repo (fallback) ===
built from commit unknown
```

No `package.json` script, no separate generator binary to wire up — `build.rs` is discovered by name and is part of the same Cargo invocation.

---

## Detailed Explanation

### How a proc macro plugs into the compiler

When `rustc` parses `app/src/main.rs` and reaches `#[route("/users/{id}")]`, it does something no other dependency can do: it **loads `route_macro` as a dynamic library and calls the `route` function**, passing the tokens for the attribute and for `fn get_user`. Whatever tokens the function returns are spliced back into the program *before* name resolution and type checking. That is why the generated `get_user_path()` is visible to `main`: by the time the compiler type-checks `main`, the function genuinely exists.

This explains three otherwise-surprising rules:

- **`proc-macro = true` is required.** The crate is compiled for the *host* (the machine running the compiler), not the target, and is loaded as a compiler plugin. Without that flag the `#[proc_macro_attribute]` annotation has no meaning and `use proc_macro::...` does not resolve (see Common Pitfalls).
- **A proc-macro crate can export *only* macros.** Because it is loaded into the compiler, it cannot also be a normal library you link against at runtime. The common pattern is a thin `mycrate-macros` proc-macro crate re-exported by a normal `mycrate` crate.
- **It is a real program with no sandbox.** A proc macro can read files, hit the network, or panic. A panic becomes a compile error; the freedom is why `sqlx::query!` can talk to your database *at compile time* to type-check SQL.

You can see exactly what the macro produced with the `cargo expand` tool (`cargo install cargo-expand`). Running it on the consumer crate shows the generated functions:

```text
$ cargo expand
fn get_user(id: u32) -> String {
    ::alloc::__export::must_use({ ::alloc::fmt::format(format_args!("user #{0}", id)) })
}
pub fn get_user_path() -> &'static str {
    "/users/{id}"
}
fn health() -> String {
    "ok".to_string()
}
pub fn health_path() -> &'static str {
    "/health"
}
```

That output is the real expansion: the `*_path` functions are now ordinary source, and the `format!` you wrote has itself been expanded by the built-in `format_args!` machinery. This is the closest equivalent to inspecting the post-Babel output of your code, and it is the single most useful debugging tool when a proc macro misbehaves.

### How a build script plugs into the build graph

`build.rs` is compiled to an executable and run by Cargo with a curated set of environment variables (`OUT_DIR`, `PROFILE`, `TARGET`, `CARGO_CFG_*`, every `CARGO_PKG_*`, and so on). It talks back to Cargo by printing lines that begin with `cargo::` to stdout. The four you will use constantly:

| Directive | Effect | TypeScript analogue |
| --- | --- | --- |
| `cargo::rustc-env=KEY=VAL` | Sets an env var visible to `env!()` during the compile | writing a `.generated.ts` constant |
| `cargo::rustc-cfg=NAME` | Enables `#[cfg(NAME)]` blocks in the crate | a `process.env.FLAG` build flag |
| `cargo::rustc-link-lib=foo` / `rustc-link-search=path` | Tell the linker to link a native library | `node-gyp` / linker flags |
| `cargo::rerun-if-changed=PATH` / `rerun-if-env-changed=VAR` | Cache invalidation: only re-run when these change | a watch list / `--watch` glob |

> **Note:** Cargo accepts both `cargo::key=value` (the current double-colon form, recommended on the 2024 edition) and the older single-colon `cargo:key=value`. New code should use `cargo::`.

A second canonical use is **generating source into `OUT_DIR`** and pulling it in with `include!`:

```rust
// build.rs — emit a Rust source file the crate will include.
use std::{env, fs, path::Path};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("build_info.rs");

    let profile = env::var("PROFILE").unwrap();      // "debug" or "release"
    let pkg = env::var("CARGO_PKG_NAME").unwrap();

    let code = format!(
        "pub const BUILD_PROFILE: &str = {profile:?};\n\
         pub const PKG_NAME: &str = {pkg:?};\n"
    );
    fs::write(&dest, code).unwrap();

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=PROFILE");
}
```

```rust
// src/main.rs — `include!` splices the generated file in textually.
include!(concat!(env!("OUT_DIR"), "/build_info.rs"));

fn main() {
    println!("package: {PKG_NAME}");
    println!("profile: {BUILD_PROFILE}");
}
```

Real output, debug and release:

```text
=== debug ===
package: probe
profile: debug
=== release ===
package: probe
profile: release
```

> **Tip:** `OUT_DIR` is the *only* directory a build script may write to. Writing generated files into `src/` works once but breaks reproducible and read-only builds; always emit into `OUT_DIR` and `include!` from there. This is the same discipline as never checking in a `*.generated.ts` file that your build also overwrites.

### The dividing line: stable vs nightly

Both proc macros and build scripts are **fully stable**. The "needs nightly" question is about the *language features your generated or hand-written code uses*, plus a handful of advanced introspection capabilities. Crate authors gate experimental language features behind `#![feature(...)]`, and that attribute is rejected outright on stable. Trying to compile a crate that opts into the unstable `never_type` feature on stable produces a real, specific error:

```rust
#![feature(never_type)] // does not compile on stable (error[E0554])

fn main() {
    println!("hi");
}
```

```text
error[E0554]: `#![feature]` may not be used on the stable release channel
 --> src/main.rs:1:1
  |
1 | #![feature(never_type)]
  | ^^^^^^^^^^^^^^^^^^^^^^^

For more information about this error, try `rustc --explain E0554`.
```

`rustc --explain E0554` states it plainly: *"Feature attributes are only allowed on the nightly release channel. Stable or beta compilers will not comply."* The exact same file compiles and runs on a nightly toolchain. The feature is real, it is simply not promised stable yet.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Source-rewriting plugin | Babel plugin / `tsc` transformer, on the AST | Proc macro, on a `TokenStream`, **inside** the compiler |
| Plugin output is checked? | Type-checked separately, if at all | Type-checked as ordinary Rust, every time |
| Pre-build program | `package.json` `prebuild`/`postbuild` script | `build.rs`, discovered by name |
| Pre-build ↔ build comms | Convention (write files, set env) | Structured `cargo::` stdout directives |
| Inspect generated code | Read post-Babel/`tsc` output | `cargo expand` |
| Native-addon build step | `node-gyp` + bindings | `build.rs` + `cc`/`bindgen`, `cargo::rustc-link-*` |
| Experimental language features | Behind a flag / a newer `tsc` you can just install | Behind `#![feature(...)]`, **nightly only** |
| Plugin runs where? | A Node process in your build | A library loaded into `rustc` (host arch) |

The deepest conceptual difference: in JavaScript the line between "plugin" and "application" is blurry: both run on Node. In Rust a proc macro runs on the **host** at *compile* time, while the crate it transforms is compiled for the **target** and runs later. A macro can therefore do compile-time work (open a database, parse a schema, validate a regex) and emit only the distilled result into the final binary, which carries none of the macro's dependencies.

> **Warning:** The unstable, internal `rustc_plugin` / "lint plugin" mechanism that existed years ago was removed. When people say "compiler plugin" in modern Rust they mean **proc macros** (for codegen) and tools built on `rustc_private` like Clippy (for lints), not a stable plugin ABI you load into `rustc` yourself. There is no stable way to write a custom lint that links against the compiler on the stable channel.

---

## Common Pitfalls

### Forgetting `proc-macro = true`

Annotating a function with `#[proc_macro_attribute]` in a crate that is *not* declared as a proc-macro crate gives two real errors at once:

```text
error: the `#[proc_macro_attribute]` attribute is only usable with crates of the `proc-macro` crate type
 --> src/lib.rs:3:1
  |
3 | #[proc_macro_attribute]
  | ^^^^^^^^^^^^^^^^^^^^^^^

error[E0432]: unresolved import `proc_macro`
 --> src/lib.rs:1:5
  |
1 | use proc_macro::TokenStream;
  |     ^^^^^^^^^^ use of unresolved module or unlinked crate `proc_macro`
```

The fix is the `[lib] proc-macro = true` stanza shown earlier. The `proc_macro` crate is a compiler-provided facade that only exists for crates of that type, which is why the import also fails.

### Mixing macros and normal code in one crate

A proc-macro crate cannot export functions, structs, or constants for runtime use — only `#[proc_macro]`, `#[proc_macro_attribute]`, and `#[proc_macro_derive]` entry points. New TypeScript-minded developers often try to colocate a helper struct with the macro. Split it: a `foo-macros` proc-macro crate plus a normal `foo` crate that re-exports the macro and contains the runtime code. Nearly every ecosystem crate with a derive (serde, thiserror, sqlx) is structured this way.

### Build scripts that never invalidate (or invalidate every time)

If you omit `cargo::rerun-if-changed`/`rerun-if-env-changed` *entirely*, Cargo applies a default of "re-run if any file in the package changed," which is usually too aggressive and rebuilds constantly. If you emit one `rerun-if-changed` for a single file, Cargo runs the script *only* when that file changes — so forgetting to list an input means stale generated code that never updates. List every real input precisely.

### Expecting a proc macro to see resolved types

A proc macro receives **tokens**, not a type-checked AST. `#[derive(Serialize)]` cannot know whether a field's type implements `Serialize`; it can only emit code that *assumes* it does and lets the later type-check catch the mistake. This trips up developers who expect Babel-with-the-type-checker-attached. The macro sees syntax; the compiler sees types, afterward.

### Reaching for nightly when stable will do

Seeing a `#![feature(...)]` in a blog post and pinning your project to nightly is a common over-correction. Most "advanced" needs (const generics, GATs, native `async fn` in traits) have been stable for years. Before adopting nightly, check whether the feature has stabilized, and read the clear-eyed stable-vs-nightly notes in the sibling pages on [Specialization](/25-advanced-topics/07-specialization/) and [GATs](/25-advanced-topics/06-gat/).

---

## Best Practices

- **Prefer a declarative macro (`macro_rules!`) first.** If pattern-matching tokens is enough, you avoid a whole proc-macro crate and its `syn`/`quote` build cost. Reach for a proc macro only when you need to *compute* over the input. See [Section 14: Declarative Macros](/14-macros/01-declarative-macros/).
- **Always build proc macros on `proc-macro2` + `syn` + `quote`.** `proc-macro2` lets you unit-test macro logic outside the compiler; `syn` 2 is the current parsing crate; `quote` is the current templating crate. Pin major versions (`syn = "2"`), never write `TokenStream` parsing by hand.
- **Emit good spans.** Use the input's `.span()` (as the `route` example does for the generated identifier) so that errors in generated code point at the user's source, not at the macro internals.
- **Keep `build.rs` fast and hermetic.** It runs on *every* clean build and on CI. Avoid network calls when you can; if you must shell out (like to `git`), degrade gracefully so an offline or non-repo build still succeeds.
- **Write only into `OUT_DIR`, and list every input** with `rerun-if-changed`. Treat the build script as a pure function of its declared inputs.
- **Stay on stable.** Use nightly only for a feature you have confirmed is unstable and genuinely need; document the pin in `rust-toolchain.toml` so the requirement is explicit and reproducible. Watch for stabilization and drop the pin when you can.

---

## Real-World Example

A production pattern that uses *both* mechanisms together: a build script that bakes build metadata into the binary, exposed through a tiny stable API. This is what `--version` output relies on in countless CLI tools.

```rust playground
// build.rs
use std::process::Command;

fn main() {
    // Git commit (short), with a graceful fallback for tarball/CI builds.
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Optional build-time feature flag, keyed off an env var so the example
    // is deterministic. Declare the custom cfg so the unexpected-cfgs lint
    // (on by default on the 2024 edition) stays quiet.
    println!("cargo::rustc-check-cfg=cfg(fast_path)");
    if std::env::var("ENABLE_FAST_PATH").is_ok() {
        println!("cargo::rustc-cfg=fast_path");
    }

    println!("cargo::rustc-env=GIT_HASH={git_hash}");
    println!("cargo::rerun-if-changed=.git/HEAD");
    println!("cargo::rerun-if-env-changed=ENABLE_FAST_PATH");
    println!("cargo::rerun-if-changed=build.rs");
}
```

```rust
// src/main.rs
/// Compile-time build metadata, baked in by build.rs. No runtime cost.
mod build_meta {
    pub const GIT_HASH: &str = env!("GIT_HASH");
    pub const PROFILE: &str = if cfg!(debug_assertions) { "debug" } else { "release" };
}

fn process() -> &'static str {
    // A build-time switch chooses the code path; the unused branch is
    // compiled out entirely, not merely skipped at runtime.
    #[cfg(fast_path)]
    {
        "using the build-enabled fast path"
    }
    #[cfg(not(fast_path))]
    {
        "using the portable default path"
    }
}

fn main() {
    println!(
        "myapp {} ({}, {})",
        env!("CARGO_PKG_VERSION"),
        build_meta::GIT_HASH,
        build_meta::PROFILE,
    );
    println!("{}", process());
}
```

Real output, default build then with the flag set:

```text
=== default ===
using the portable default path
=== ENABLE_FAST_PATH=1 ===
using the build-enabled fast path
```

(The version line prints e.g. `myapp 0.1.0 (7eb8bda, debug)` — the hash and profile vary per build.) The build flag is resolved *before* compilation, so the unused branch is not in the final binary at all. That is the payoff of build-time codegen over runtime configuration: the decision costs nothing at runtime because it was already made by the compiler.

> **Tip:** For metadata-heavy projects, the community crates `vergen` (build/git info via `build.rs`) and `built` automate exactly this pattern, emitting a richer set of `cargo::rustc-env` values for you. Add them with `cargo add vergen --build` and call them from `build.rs`.

---

## Further Reading

- [The Cargo Book: Build Scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html) — the authoritative list of `cargo::` directives and build-script environment variables.
- [The Rust Reference: Procedural Macros](https://doc.rust-lang.org/reference/procedural-macros.html) — the crate type, the three macro kinds, and hygiene rules.
- [The Unstable Book](https://doc.rust-lang.org/unstable-book/). The canonical catalogue of nightly `#![feature(...)]` gates and their status.
- [`syn` documentation](https://docs.rs/syn) and [`quote` documentation](https://docs.rs/quote) — the parsing and templating crates every nontrivial proc macro uses.
- [Section 14: Procedural Macros](/14-macros/07-proc-macros/) — the hands-on guide to *writing* a derive/attribute/function macro with `syn` and `quote`.
- [Section 14: Declarative Macros](/14-macros/01-declarative-macros/) — the simpler `macro_rules!` alternative to reach for first.
- [Section 20: Unsafe & FFI](/20-unsafe-ffi/) — where `build.rs` + `cc`/`bindgen` link native C libraries.
- [Specialization](/25-advanced-topics/07-specialization/) and [Generic Associated Types](/25-advanced-topics/06-gat/) — concrete case studies in the stable-vs-nightly divide.
- [Section 26: Systems Programming](/26-systems-programming/) — more build-script-driven native integration in context.
- [Section 00: Introduction](/00-introduction/) · [Section 01: Getting Started](/01-getting-started/) · [Section 02: Basics](/02-basics/) — start here if any prerequisite feels shaky.

---

## Exercises

### Exercise 1: A build script that bakes in the build time

**Difficulty:** Beginner

**Objective:** Use a `build.rs` to expose the build timestamp to your program with no runtime dependency.

**Instructions:** In a fresh `cargo new` project, write a `build.rs` that computes seconds-since-the-Unix-epoch with `std::time::SystemTime` and emits it via `cargo::rustc-env=BUILD_UNIX_TIME=...`. Read it in `main` with `env!` and print it. Add a `rerun-if-changed` for `build.rs`. Confirm the value changes only when you force a rebuild (`touch build.rs`).

<details>
<summary>Solution</summary>

```rust playground
// build.rs
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo::rustc-env=BUILD_UNIX_TIME={now}");
    println!("cargo::rerun-if-changed=build.rs");
}
```

```rust
// src/main.rs
fn main() {
    // Parsed at compile time from the &'static str env! produces.
    let t: u64 = env!("BUILD_UNIX_TIME").parse().unwrap();
    println!("built at unix time {t}");
}
```

Running it prints a line like `built at unix time 1748793600`. Because the only declared input is `build.rs`, the timestamp is *not* refreshed on an unchanged rebuild; it updates only after `touch build.rs` (or a `cargo clean`). That demonstrates the cache-invalidation contract: the build script is treated as a function of its declared inputs.

</details>

### Exercise 2: An attribute macro that adds a name method

**Difficulty:** Intermediate

**Objective:** Write a proc macro that generates a method, proving you understand the `proc-macro = true` crate type and token round-tripping.

**Instructions:** Create a proc-macro crate exposing `#[named]` as an attribute on a struct. It should leave the struct unchanged and additionally emit `impl <Struct> { pub fn type_name() -> &'static str { "<Struct>" } }`. Use `syn::ItemStruct` and `quote!`. Apply it in a consumer crate and print `Foo::type_name()`.

<details>
<summary>Solution</summary>

```rust
// named_macro/src/lib.rs  (Cargo.toml has [lib] proc-macro = true,
//   deps: syn = { version = "2", features = ["full"] }, quote = "1")
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemStruct};

#[proc_macro_attribute]
pub fn named(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_struct = parse_macro_input!(item as ItemStruct);
    let name = &item_struct.ident;
    let name_str = name.to_string();

    let expanded = quote! {
        #item_struct

        impl #name {
            pub fn type_name() -> &'static str {
                #name_str
            }
        }
    };
    expanded.into()
}
```

```rust
// app/src/main.rs   (depends on named_macro = { path = "../named_macro" })
use named_macro::named;

#[named]
struct Foo {
    _x: i32,
}

fn main() {
    println!("{}", Foo::type_name());
}
```

This prints `Foo`. The macro echoes the struct back verbatim (`#item_struct`) and tacks on a generated `impl`. Run `cargo expand` in the consumer to see the `impl Foo { pub fn type_name() ... }` block the compiler now sees as ordinary source.

</details>

### Exercise 3: Detect the toolchain channel from a build script

**Difficulty:** Advanced

**Objective:** Emit a `cfg` flag from `build.rs` that lets a crate conditionally use a nightly feature only when built on nightly: the real-world pattern crates use to opt into nightly perks without breaking stable users.

**Instructions:** In `build.rs`, run `rustc -vV` (the version of the compiler Cargo is using, available via the `RUSTC` env var), parse the `release:` line, and emit `cargo::rustc-cfg=nightly_compiler` when the version string contains `-nightly`. Declare the cfg with `rustc-check-cfg`. In `main`, print different text under `#[cfg(nightly_compiler)]` vs not. Verify the stable path prints on your stable toolchain.

<details>
<summary>Solution</summary>

```rust playground
// build.rs
use std::process::Command;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(nightly_compiler)");

    // RUSTC points at the exact compiler Cargo will use for this build.
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let out = Command::new(rustc)
        .arg("-vV")
        .output()
        .expect("failed to run rustc -vV");
    let text = String::from_utf8_lossy(&out.stdout);

    let is_nightly = text
        .lines()
        .find_map(|l| l.strip_prefix("release: "))
        .map(|v| v.contains("-nightly"))
        .unwrap_or(false);

    if is_nightly {
        println!("cargo::rustc-cfg=nightly_compiler");
    }
    println!("cargo::rerun-if-changed=build.rs");
}
```

```rust playground
// src/main.rs
fn main() {
    #[cfg(nightly_compiler)]
    println!("nightly toolchain: experimental fast path available");

    #[cfg(not(nightly_compiler))]
    println!("stable toolchain: using the portable path");
}
```

On a stable toolchain this prints `stable toolchain: using the portable path`; building the same project with a nightly `rustc` (for example via a `+nightly` override or a `rustc-toolchain.toml`) flips it to the nightly branch. This is precisely how crates such as the older `rayon`/`hashbrown` builds enabled nightly-only optimizations transparently: detect the channel, emit a `cfg`, and guard the feature gate behind it so stable builds simply skip it.

</details>
