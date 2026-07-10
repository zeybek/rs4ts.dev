---
title: "Specifying Dependencies: `cargo add`, SemVer, Features, and Git/Path Sources"
description: "cargo add is npm install, but a bare version is already a caret range. Covers SemVer grammar, features, default-features, and git, path, and registry sources."
---

`cargo add` is `npm install`, and a line in `[dependencies]` is a line in `package.json`'s `dependencies`. But the resemblance hides important differences: a bare version string in Cargo is already a caret range, features replace npm's "install the optional thing" guesswork, and you can pull a crate straight from a git repository or a folder on disk. This page is the practical grammar of declaring what your crate depends on.

---

## Quick Overview

In Rust you add a **crate** (Rust's word for a package) with `cargo add <name>`, which edits `[dependencies]` in `Cargo.toml` and re-resolves the lockfile, exactly like `npm install <name>` editing `package.json` and `package-lock.json`. The two ideas with no clean npm analogue are **features** (opt-in, compile-time capabilities a crate exposes) and the fact that Cargo can resolve a dependency from **crates.io**, a **git repository**, or a **local path** with the same uniform syntax. This page covers `cargo add`, the full SemVer requirement grammar (caret, tilde, exact, comparison, wildcard), enabling features, and git/path sources; the manifest's overall shape lives in [Cargo.toml](/12-modules-packages/04-cargo/), and feature *design* is covered in [Feature Flags and Conditional Compilation](/12-modules-packages/09-feature-flags/).

---

## TypeScript/JavaScript Example

In a Node project you install dependencies with npm, which writes them into `package.json` and pins the full tree in `package-lock.json`:

```bash
# Add runtime deps; npm writes a caret range like "^1.0.0" by default
npm install zod
npm install chalk@5.3.0          # a specific version
npm install some-fork --save     # from the default registry

# You can also depend on a git repo or a local folder:
npm install github:colinhacks/zod          # straight from GitHub
npm install ../shared-utils                # a sibling folder on disk
```

The resulting `package.json`:

```jsonc
{
  "dependencies": {
    "zod": "^3.23.0", // caret: >=3.23.0 <4.0.0
    "chalk": "5.3.0", // NO caret -> exactly 5.3.0
    "shared-utils": "file:../shared-utils", // local path
    "fork": "github:colinhacks/zod" // git source
  }
}
```

To enable an optional capability there is no first-class mechanism: you either install an extra companion package, or a library reads an environment variable / config flag at runtime. npm has no concept of "compile this part of the dependency in, leave that part out."

---

## Rust Equivalent

The same set of moves with Cargo. `cargo add` (built into Cargo since 1.62; **no `cargo-edit` install needed**) edits the manifest for you:

```bash
# Add from crates.io; Cargo writes the resolved version, treated as a CARET range
cargo add serde --features derive   # add a crate AND turn on a feature
cargo add serde_json
cargo add regex@1.10                # constrain to a SemVer requirement

# From a local folder (a sibling crate on disk):
cargo add geometry --path ../geometry

# Straight from a git repository:
cargo add anyhow --git https://github.com/dtolnay/anyhow
```

Running `cargo add serde --features derive` prints the real output below; note how Cargo lists every feature and marks which are on (`+`) and off (`-`):

```text
    Updating crates.io index
      Adding serde v1.0.228 to dependencies
             Features:
             + derive
             + serde_derive
             + std
             - alloc
             - rc
             - unstable
    Updating crates.io index
     Locking 7 packages to latest Rust 1.96.0 compatible versions
```

The resulting `Cargo.toml`:

```toml
[dependencies]
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
regex = "1.10"
geometry = { version = "0.1.0", path = "../geometry" }
anyhow = { git = "https://github.com/dtolnay/anyhow", version = "1.0.102" }
```

> **Note:** Cargo writes the **full resolved version** (`"1.0.228"`), but that string is still a *caret* requirement, not an exact pin; see the next section. The byte-exact tree is pinned separately in `Cargo.lock`, the twin of `package-lock.json`.

A small program that exercises the crates.io dependencies above (compile-verified):

```rust playground
// src/main.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
    #[serde(default)]
    tls: bool,
}

fn main() {
    let raw = r#"{ "host": "0.0.0.0", "port": 8080 }"#;
    let cfg: ServerConfig = serde_json::from_str(raw).expect("valid config");
    println!("parsed: {cfg:?}");
    println!("re-serialized: {}", serde_json::to_string(&cfg).unwrap());
}
```

Real output:

```text
parsed: ServerConfig { host: "0.0.0.0", port: 8080, tls: false }
re-serialized: {"host":"0.0.0.0","port":8080,"tls":false}
```

---

## Detailed Explanation

### `cargo add` vs `npm install`

`cargo add <crate>` does three things, just like `npm install <pkg>`:

1. Contacts the registry (crates.io by default) to find the latest compatible version.
2. Writes a line into `[dependencies]` in `Cargo.toml`.
3. Updates `Cargo.lock` with the resolved version and a checksum.

Unlike `npm install` with no arguments (which installs everything already listed), there is no `cargo install` for project dependencies. That command installs *binaries* globally, like `npm install -g`. To download/build what's already in your manifest, you just run `cargo build`; Cargo fetches missing crates automatically. There is no `node_modules/`: crate *sources* are cached once in `~/.cargo/registry/` and compiled into your project's `target/` directory.

### SemVer requirements: the part TypeScript developers misread

This is the single biggest gotcha. In `package.json`, a version with **no prefix** means *exactly that version*:

```jsonc
"chalk": "5.3.0"   // npm: EXACTLY 5.3.0
```

In `Cargo.toml`, a bare version string is a **caret requirement**, the same as if you'd written `^` in npm:

```toml
regex = "1.10"     // Cargo: ^1.10  ==  >=1.10.0, <2.0.0
```

Cargo supports five requirement operators. Each row below was verified by resolving it against the live crates.io index:

| Requirement       | Meaning                                  | Verified resolution (latest available)                          |
| ----------------- | ---------------------------------------- | --------------------------------------------------------------- |
| `"1.0"` (caret)   | `>=1.0.0, <2.0.0`, any compatible 1.x   | `serde_json = "1.0"` → resolved **1.0.150**                     |
| `"~1.10.0"` (tilde) | `>=1.10.0, <1.11.0` — patch updates only | `regex = "~1.10.0"` → resolved **1.10.6** (while 1.12.3 exists) |
| `"=1.0.100"` (exact) | exactly that version                  | `serde_json = "=1.0.100"` → resolved **1.0.100**               |
| `">=1.5, <1.11"`  | a comparison range (comma = AND)         | `regex = ">=1.5, <1.11"` → resolved **1.10.6**                 |
| `"1.*"` (wildcard) | any 1.x (`*` matches any value)         | `regex = "1.*"` → resolved **1.12.3**                          |

The key insights for a TypeScript/JavaScript developer:

- **Caret is the default and the idiom.** Writing `"1"`, `"1.0"`, or `"1.0.150"` all mean "any 1.x at or above this". Prefer the shortest form that expresses your floor; `"1"` is common and perfectly idiomatic.
- **Caret on a `0.x` version is special.** Just like npm, `^0.3` means `>=0.3.0, <0.4.0`: for pre-1.0 crates, the *minor* number acts as the breaking-change boundary. So `tokio = "0.3"` would not auto-upgrade to `0.4`.
- **Tilde pins the minor.** `~1.10.0` accepts patch releases (`1.10.x`) but not `1.11`. The table above proves it: it picked `1.10.6` even though `1.12.3` was available.
- **Exact (`=`) is what npm's bare version does.** Reach for it only when you truly must (a known-bad later release, byte-reproducibility outside the lockfile).
- **Wildcard `*` is discouraged** and crates.io **rejects a bare `*`** when you publish (it cannot be resolved reproducibly downstream).

> **Warning:** The lockfile, not the requirement, controls *exactly* which version a build uses. A caret `"1"` plus a committed `Cargo.lock` already gives you reproducible builds. You do **not** need to pin with `=` to get determinism. Pinning with `=` mostly *removes* flexibility for downstream users of a library.

### Features: opt-in capabilities

A **feature** is a named, conditionally-compiled chunk of a crate. Enabling one can switch on extra modules, derive macros, trait implementations, or transitive dependencies. There is no real npm equivalent; the closest mental model is "this package ships several builds and you pick which parts compile in."

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }   # turn on the derive macros
tokio = { version = "1", features = ["full"] }        # "full" = the umbrella feature
```

`cargo add` shows you the feature menu. Adding `tokio` with the `full` feature prints (real output, trimmed):

```text
      Adding tokio v1.52.3 to dependencies
             Features:
             + bytes
             + fs
             + full
             + io-util
             + macros
             + net
             + rt
             + rt-multi-thread
             + sync
             + time
             ...
             - test-util
             - tracing
```

The `+` lines are enabled, the `-` lines are available-but-off. Most crates ship a set of **default features** that are on unless you opt out with `default-features = false`:

```toml
[dependencies]
# Disable the defaults, then opt back into only what you need (smaller build):
uuid = { version = "1", default-features = false, features = ["v4"] }
```

> **Tip:** Features are **additive** by design: enabling a feature should only *add* capability, never remove or change existing behavior. Cargo unifies features across your whole dependency graph: if crate A enables `serde/std` and crate B does not, the unified build still has `std` on. Design and `#[cfg(feature = "...")]` are covered in [Feature Flags and Conditional Compilation](/12-modules-packages/09-feature-flags/).

### Path dependencies: a folder on disk

A **path dependency** points at a crate in your filesystem: ideal for local development, examples, or splitting a project into crates before publishing. It mirrors npm's `file:../shared-utils`.

```toml
[dependencies]
geometry = { version = "0.1.0", path = "../geometry" }
```

Given a library crate at `../geometry` exposing `pub fn rectangle_area(...)`, the dependent crate uses it like any other (compile-verified — prints `area = 12`):

```rust
// src/main.rs in the dependent crate
use geometry::rectangle_area;

fn main() {
    let area = rectangle_area(3.0, 4.0);
    println!("area = {area}");
}
```

The `version` is optional for purely local use but recommended: if you later publish, Cargo uses `version` for crates.io and `path` only for local builds. Multiple path-linked crates in one repository are usually better modeled as a **workspace**; see [Cargo Workspaces](/12-modules-packages/08-workspaces/).

### Git dependencies: straight from a repository

A **git dependency** fetches the crate from a repository, like npm's `github:owner/repo`. Useful for unreleased fixes, private forks, or crates not on crates.io.

```toml
[dependencies]
# Track the default branch (re-resolves to its latest commit on `cargo update`):
anyhow = { git = "https://github.com/dtolnay/anyhow" }

# Pin to a branch, a tag, or an exact commit:
anyhow = { git = "https://github.com/dtolnay/anyhow", branch = "master" }
anyhow = { git = "https://github.com/dtolnay/anyhow", tag = "1.0.102" }
anyhow = { git = "https://github.com/dtolnay/anyhow", rev = "841522b" }
```

`cargo add anyhow --git https://github.com/dtolnay/anyhow` produces this real output and manifest entry:

```text
    Updating git repository `https://github.com/dtolnay/anyhow`
      Adding anyhow (git) to dependencies
             Features:
             + std
             - backtrace
    Updating git repository `https://github.com/dtolnay/anyhow`
     Locking 1 package to latest Rust 1.96.0 compatible version
```

Whatever commit Cargo resolves is recorded in `Cargo.lock`, so a git dependency is still reproducible. The lockfile entry looks like:

```toml
[[package]]
name = "anyhow"
version = "1.0.102"
source = "git+https://github.com/dtolnay/anyhow#841522b2aa09732fecee40804440d2c35c68c480"
```

> **Note:** Without `branch`/`tag`/`rev`, a git dependency tracks the default branch's HEAD *at the time it was first locked*. It does not silently float on every build; `cargo update` is what advances it, just as `npm update` advances a git dep.

### Renaming a dependency

If you want to refer to a crate under a different name in code (or depend on two majors of the same crate), use the `package` key. This is the manifest equivalent of npm aliases (`npm install json@npm:serde_json`):

```toml
[dependencies]
json = { package = "serde_json", version = "1.0" }
```

Now the crate is reachable as `json` in your code (compile-verified — prints `true`):

```rust
fn main() {
    let v: json::Value = json::from_str(r#"{"ok":true}"#).unwrap();
    println!("{}", v["ok"]);
}
```

---

## Key Differences

| Concept                  | npm / `package.json`                       | Cargo / `Cargo.toml`                                  |
| ------------------------ | ------------------------------------------ | ----------------------------------------------------- |
| Add a dependency         | `npm install <pkg>`                        | `cargo add <crate>` (built in since 1.62)             |
| Bare version string      | **Exact** (`"5.3.0"` = only 5.3.0)         | **Caret** (`"1.0"` = `>=1.0.0, <2.0.0`)               |
| Explicit caret           | `"^5.3.0"`                                 | Default; can also write `"^1"` explicitly             |
| Tilde                    | `"~5.3.0"` (≈ patch-level)                 | `"~1.10.0"` (`>=1.10.0, <1.11.0`)                     |
| Exact pin                | bare version, or `"5.3.0"`                 | `"=1.0.100"`                                          |
| Optional capabilities    | (no real equivalent)                       | **features** (`features = [...]`, `default-features`) |
| Local folder             | `"file:../x"`                              | `{ path = "../x" }`                                   |
| Git source               | `"github:owner/repo"`                      | `{ git = "url", branch/tag/rev = ... }`               |
| Alias / rename           | `npm:real-name`                            | `{ package = "real-name" }`                           |
| Two majors side by side  | dedup'd; awkward                           | Allowed natively (e.g. `rand` 0.8 *and* 0.9)          |
| Stored on disk           | `node_modules/` per project                | shared `~/.cargo/registry/`, compiled into `target/`  |

The deepest difference is the **caret default**. A TypeScript developer reads `regex = "1.10"` as "locked to 1.10" and is surprised when `cargo update` jumps to `1.12`. It is the opposite of npm's bare-version behavior. The second is **features**: Cargo bakes "which optional parts to compile" into the dependency declaration, so a single crate can be lean in one project and full-featured in another without a different package.

---

## Common Pitfalls

### Pitfall 1: Reading a bare version as a pin

```toml
[dependencies]
tokio = "1.45.0"   # This is ^1.45.0, NOT a pin to exactly 1.45.0
```

A `package.json`-trained eye reads this as "exactly 1.45.0". It is `>=1.45.0, <2.0.0`. If you genuinely need exactly one version, write `tokio = "=1.45.0"`. But for reproducibility you almost never need to: commit `Cargo.lock` and the *resolved* version is fixed regardless of the caret range.

### Pitfall 2: Forgetting a feature, then hitting a "method not found" error

The most common feature mistake: the function you want lives behind a non-default feature. For example, `Uuid::new_v4()` requires the `v4` feature. With `uuid` added but `v4` *off*:

```toml
[dependencies]
uuid = "1"          # missing features = ["v4"]
```

```rust playground
use uuid::Uuid;

fn main() {
    let id = Uuid::new_v4(); // does not compile (error[E0599]) — needs the "v4" feature
    println!("{id}");
}
```

The real error from `cargo build`:

```text
error[E0599]: no function or associated item named `new_v4` found for struct `Uuid` in the current scope
   --> src/main.rs:4:20
    |
  4 |     let id = Uuid::new_v4(); // does not compile (error[E0599]) — needs the "v4" feature
    |                    ^^^^^^ function or associated item not found in `Uuid`
    |
note: if you're trying to build a new `Uuid` consider using one of the following associated functions:
      uuid::builder::<impl Uuid>::nil
      uuid::builder::<impl Uuid>::max
      ...
For more information about this error, try `rustc --explain E0599`.
```

The message points at the type but not always at the missing feature, which makes this confusing. **Fix:** enable the feature. `uuid = { version = "1", features = ["v4"] }`, or `cargo add uuid --features v4`. When a function is "documented but missing," suspect a feature flag and check the crate's docs.rs feature list. (See also the analogous `serde` derive trap in [Feature Flags and Conditional Compilation](/12-modules-packages/09-feature-flags/).)

### Pitfall 3: Requiring a version that doesn't exist

```toml
[dependencies]
serde_json = "=1.0.99999"   # no such version
```

```text
error: failed to select a version for the requirement `serde_json = "=1.0.99999"`
candidate versions found which didn't match: 1.0.150, 1.0.149, 1.0.148, ...
location searched: crates.io index
required by package `conflictprobe v0.1.0 (...)`
```

This happens when you pin too aggressively or typo a version. Cargo lists the candidates it *did* find. **Fix:** widen the requirement (drop the `=`) or pick a real version from the list.

### Pitfall 4: Expecting `*` to work, or to behave like a lazy "latest"

A bare `regex = "*"` resolves locally but crates.io **refuses to publish** a crate that depends on `"*"`, because it is not reproducible for downstream users. Even `"1.*"` is discouraged. Prefer a caret floor like `"1"`. Cargo's caret already means "latest compatible," so wildcards buy you nothing but trouble.

### Pitfall 5: Assuming git deps auto-update on every build

Adding `{ git = "..." }` does not mean "always the newest commit." Cargo locks the resolved commit into `Cargo.lock` and reuses it. To advance to a newer commit you must run `cargo update` (or change the `branch`/`tag`/`rev`). This surprises people expecting npm-style behavior, though npm git deps behave the same way once locked.

---

## Best Practices

- **Let `cargo add` write the manifest.** `cargo add serde --features derive` finds the latest version, writes the correct inline table, and updates the lockfile. Hand-typing version strings invites typos and stale versions.
- **Prefer short caret requirements.** `"1"` or `"1.0"` is idiomatic. Reach for `"=x.y.z"` only when you have a concrete reason (a regression in a later release, a strict reproducibility requirement outside the lockfile).
- **Commit `Cargo.lock` for binaries/applications; omit it for libraries.** Reproducible app builds want a committed lockfile; libraries want to be tested against the newest compatible deps. This is the same convention as npm.
- **Enable only the features you use; consider `default-features = false`** for leaner builds and faster compiles when you don't need a crate's defaults, then opt back into the specific features you need.
- **Use path deps for local multi-crate work, and graduate to a workspace** once you have more than two crates in one repository. See [Cargo Workspaces](/12-modules-packages/08-workspaces/).
- **Pin git deps with `tag` or `rev` for anything shipping.** A floating branch is fine for experiments but a tagged/`rev`-pinned commit is auditable and reproducible.
- **Audit and update intentionally.** `cargo update` advances within your SemVer ranges; `cargo outdated` (a popular plugin) and `cargo audit` (security advisories) are the analogues of `npm outdated` / `npm audit`. See [Cargo Commands](/12-modules-packages/05-cargo-commands/).

---

## Real-World Example

A production-flavored configuration loader: it depends on `serde` (with the `derive` feature) and `toml` from crates.io, and parses a service config with sensible defaults. Both the manifest and the program were compiled and run as-is.

```toml
# Cargo.toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
toml = "1"
```

```rust playground
// src/main.rs
use serde::Deserialize;

/// Application config, parsed from a TOML string (inlined here for the demo).
#[derive(Debug, Deserialize)]
struct Config {
    name: String,
    #[serde(default = "default_workers")]
    workers: u32,
    database: Database,
}

#[derive(Debug, Deserialize)]
struct Database {
    url: String,
    #[serde(default)]
    pool_size: u32,
}

fn default_workers() -> u32 {
    4
}

fn main() -> Result<(), toml::de::Error> {
    let raw = r#"
        name = "billing-service"

        [database]
        url = "postgres://localhost/billing"
        pool_size = 16
    "#;

    let cfg: Config = toml::from_str(raw)?;
    println!("service: {}", cfg.name);
    println!("workers: {} (defaulted)", cfg.workers);
    println!("db url:  {}", cfg.database.url);
    println!("pool:    {}", cfg.database.pool_size);
    Ok(())
}
```

Real `cargo run` output:

```text
service: billing-service
workers: 4 (defaulted)
db url:  postgres://localhost/billing
pool:    16
```

This is the everyday shape of a Rust dependency story: a crate (`serde`) you enable a feature on, a sibling crate (`toml`) that integrates with it through that feature, and zero manual wiring. The `#[serde(default)]` attributes work *only because* the `derive` feature is on, the same opt-in mechanism we covered above. Inspect the resolved graph with `cargo tree` (the analogue of `npm ls`):

```text
$ cargo tree
billing-service v0.1.0 (/path/to/billing-service)
├── serde v1.0.228
│   ├── serde_core v1.0.228
│   └── serde_derive v1.0.228 (proc-macro)
│       ├── proc-macro2 v1.0.106
│       │   └── unicode-ident v1.0.24
│       ├── quote v1.0.45
│       │   └── proc-macro2 v1.0.106 (*)
│       └── syn v2.0.117
│           ├── proc-macro2 v1.0.106 (*)
│           ├── quote v1.0.45 (*)
│           └── unicode-ident v1.0.24
└── toml v1.1.2+spec-1.1.0
    ├── serde_core v1.0.228
    ├── serde_spanned v1.1.1
    │   └── serde_core v1.0.228
    ├── toml_datetime v1.1.1+spec-1.1.0
    │   └── serde_core v1.0.228
    ├── toml_parser v1.1.2+spec-1.1.0
    │   └── winnow v1.0.3
    ├── toml_writer v1.1.1+spec-1.1.0
    └── winnow v1.0.3
```

---

## Further Reading

### Official documentation

- [The Cargo Book — Specifying Dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html) — caret/tilde/exact/wildcard grammar, git and path sources.
- [The Cargo Book — `cargo add`](https://doc.rust-lang.org/cargo/commands/cargo-add.html)
- [The Cargo Book — Features](https://doc.rust-lang.org/cargo/reference/features.html)
- [The SemVer specification](https://semver.org/) — what major/minor/patch actually promise.

### Cross-links in this guide

- [Cargo.toml: the manifest, lockfile & profiles](/12-modules-packages/04-cargo/) — the file these dependencies live in.
- [Cargo commands](/12-modules-packages/05-cargo-commands/) — `add`, `build`, `update`, and friends.
- [Dev & build dependencies](/12-modules-packages/07-dev-dependencies/) — `[dev-dependencies]`, `[build-dependencies]`, and optional deps.
- [Feature flags](/12-modules-packages/09-feature-flags/) — designing `[features]` and `#[cfg(feature = "...")]`.
- [Workspaces](/12-modules-packages/08-workspaces/) — when path deps grow into a monorepo with a shared lockfile.
- [Publishing to crates.io](/12-modules-packages/11-publishing/) — why `*` requirements are rejected, and how versioning works for publishers.
- [Modules: ES modules → `mod`](/12-modules-packages/00-modules/) and [`use` & re-exports](/12-modules-packages/02-use-keyword/) — organizing code *within* a crate once a dependency is in scope.
- Foundations: [Why Rust](/00-introduction/) · [Understanding Cargo (intro)](/01-getting-started/03-cargo-basics/) · [Basics](/02-basics/).
- Next section: [Testing](/13-testing/) — where `[dev-dependencies]` like `proptest` and `mockall` come into play.

---

## Exercises

### Exercise 1: Translate npm installs into Cargo declarations

**Difficulty:** Beginner

**Objective:** Build the right `[dependencies]` table and understand caret-by-default.

**Instructions:** A teammate ran these npm commands. Write the equivalent `[dependencies]` lines (or `cargo add` commands), keeping the version semantics correct. Note where the bare-version meaning differs between npm and Cargo.

```bash
npm install zod          # latest 3.x acceptable
npm install chalk@5.3.0  # they want EXACTLY 5.3.0
npm install ../shared    # a sibling folder on disk
```

<details>
<summary>Solution</summary>

```toml
[dependencies]
# "latest 3.x" — a bare caret string IS that in Cargo (unlike npm, where bare = exact):
serde = "1"                                 # (zod has no Rust twin; serde stands in)
# "exactly 5.3.0" — npm's bare version == Cargo's `=` operator:
some_crate = "=5.3.0"
# a sibling folder on disk -> a path dependency:
shared = { path = "../shared" }
```

Equivalently with the CLI:

```bash
cargo add serde
cargo add some_crate@=5.3.0
cargo add shared --path ../shared
```

The teachable point: npm's `chalk@5.3.0` (no caret) is an **exact** request, so it maps to Cargo's `=5.3.0`, *not* a bare `"5.3.0"` (which would be a caret range `>=5.3.0, <6.0.0`).

</details>

### Exercise 2: Add a crate with a feature and use it

**Difficulty:** Intermediate

**Objective:** Practice enabling a feature and confirm the program runs.

**Instructions:** Starting from a fresh `cargo new`, add the `rand` crate and make this program compile and print a random dice roll. Fill in the `/* ??? */`.

```rust
fn main() {
    let roll: u32 = /* ??? */; // a random number in 1..=6
    println!("you rolled a {roll}");
}
```

<details>
<summary>Solution</summary>

```bash
cargo add rand
```

`Cargo.toml` ends up with `rand = "0.10.1"` (the current release at the time of writing — yours may be newer; a caret range, so `0.10.x` is fine).

```rust
fn main() {
    // rand 0.10 exposes a `random_range` free function (no trait import needed):
    let roll: u32 = rand::random_range(1..=6);
    println!("you rolled a {roll}");
}
```

Real output (nondeterministic — your number will differ):

```text
you rolled a 6
```

> **Note:** This uses the current `rand` 0.10 API. The free `rand::random_range` used here needs no import. If you instead reach for the method form, `rand::rng()` returns an explicit generator whose `random_range` method needs `use rand::RngExt;` in scope — the familiar `use rand::Rng;` no longer provides it and produces `error[E0599]`. Older guides show rand 0.8's `thread_rng()` / `gen_range()`; that API is gone, do not copy it.

</details>

### Exercise 3: Choose the right SemVer requirement

**Difficulty:** Advanced

**Objective:** Reason about caret vs tilde vs exact, and predict what Cargo resolves.

**Instructions:** You depend on a crate `widget` that has published `1.4.0`, `1.4.7`, `1.5.2`, and `2.0.0`. For each requirement string, state (a) the version range it allows and (b) which of those four versions Cargo would pick. Then say which requirement you'd choose if you want bug-fix updates but want to avoid any new `1.x` features, and why.

1. `widget = "1.4"`
2. `widget = "~1.4.0"`
3. `widget = "=1.4.0"`
4. `widget = "1"`

<details>
<summary>Solution</summary>

| Requirement     | Allowed range          | Picks (of 1.4.0 / 1.4.7 / 1.5.2 / 2.0.0) |
| --------------- | ---------------------- | ----------------------------------------- |
| `"1.4"` (caret) | `>=1.4.0, <2.0.0`      | **1.5.2** (highest <2.0.0)                |
| `"~1.4.0"` (tilde) | `>=1.4.0, <1.5.0`   | **1.4.7** (highest 1.4.x)                 |
| `"=1.4.0"` (exact) | exactly 1.4.0       | **1.4.0**                                 |
| `"1"` (caret)   | `>=1.0.0, <2.0.0`      | **1.5.2** (highest <2.0.0)                |

> **Note:** A bare `"1.4"` is a *caret*, so it allows the whole `1.x` line; it does **not** lock you to `1.4.*`. That is exactly the npm-vs-Cargo trap.

**Best choice for "bug fixes only, no new minor features":** `widget = "~1.4.0"`. The tilde operator accepts patch releases within `1.4.x` (so you get `1.4.7`'s fixes) but refuses `1.5.2`, which under SemVer may introduce new (additive) features you want to avoid for now. Caret (`"1.4"` / `"1"`) would happily jump to `1.5.2`; exact (`"=1.4.0"`) would freeze you on the buggy `1.4.0` and miss the patch fixes.

This guide verified the tilde behavior live: `regex = "~1.10.0"` resolves to `1.10.6` even though `1.12.3` is available; the tilde holds the minor at `1.10`.

</details>
