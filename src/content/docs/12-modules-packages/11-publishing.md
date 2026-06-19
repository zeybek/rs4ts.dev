---
title: "Publishing to crates.io"
description: "Publish a Rust crate to crates.io like npm publish, but permanent: cargo publish, required metadata, enforced SemVer, and cargo yank instead of unpublish."
---

Once your crate is useful to more than just you, you publish it to **crates.io** — Rust's central registry, the equivalent of the public npm registry. This page covers the full publishing workflow: preparing manifest metadata, the `cargo publish` command, SemVer-based versioning, and what to do when a release goes wrong (`cargo yank`).

---

## Quick Overview

Publishing a Rust crate is conceptually the same as running `npm publish`: you push a versioned, immutable artifact to a shared registry that everyone can depend on. The mechanics differ in ways that matter to a TypeScript/JavaScript developer: crates.io publishes are **permanent and append-only** (you can never `unpublish` and reuse a version the way `npm unpublish` allows within 72 hours), versions must follow **strict Semantic Versioning** that Cargo enforces in resolution, and a rich set of `[package]` metadata fields is expected before the registry will accept your crate. This page focuses on the publish workflow itself; the [manifest](/12-modules-packages/04-cargo/), [dependency specs](/12-modules-packages/06-dependencies/), and [workspaces](/12-modules-packages/08-workspaces/) are covered in sibling files.

---

## TypeScript/JavaScript Example

Publishing a small library to the npm registry. You authenticate once, bump the version, and run `npm publish`:

```jsonc
// package.json — the metadata npm uses for the registry listing
{
  "name": "@ada/slugify",
  "version": "0.1.0",
  "description": "Convert arbitrary text into clean, URL-safe slugs",
  "license": "MIT",
  "author": "Ada Zeybek <me@zeybek.dev>",
  "homepage": "https://github.com/ada/slugify#readme",
  "repository": { "type": "git", "url": "https://github.com/ada/slugify.git" },
  "keywords": ["slug", "url", "text", "string"],
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "files": ["dist"], // allow-list of what gets shipped in the tarball
  "publishConfig": { "access": "public" }
}
```

```bash
npm login                    # authenticate (writes a token to ~/.npmrc)
npm version patch            # 0.1.0 -> 0.1.1, also creates a git tag
npm publish --access public  # upload the tarball to the registry
```

npm lets you _undo_ a recent mistake: within 72 hours you can `npm unpublish @ada/slugify@0.1.1` and then re-publish that same version. npm also lets you `npm deprecate` a version to warn installers without removing it.

---

## Rust Equivalent

The same library as a crate published to crates.io. Metadata lives in `Cargo.toml`, and the workflow is `login` -> bump version -> `publish`:

```toml
# Cargo.toml
[package]
name = "slugify-rs"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
description = "Convert arbitrary text into clean, URL-safe slugs"
license = "MIT OR Apache-2.0"
repository = "https://github.com/ada/slugify-rs"
homepage = "https://github.com/ada/slugify-rs"
documentation = "https://docs.rs/slugify-rs"
readme = "README.md"
keywords = ["slug", "url", "text", "string", "web"]
categories = ["text-processing", "web-programming"]
exclude = ["/.github", "/benches", "*.png"]

[dependencies]
```

```rust
// src/lib.rs
//! Convert arbitrary text into clean, URL-safe slugs.

/// Turn an arbitrary string into a lowercase, hyphen-separated slug.
///
/// ```
/// assert_eq!(slugify_rs::slugify("Hello, World!"), "hello-world");
/// ```
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}
```

```bash
cargo login                 # authenticate (paste a token from crates.io/me)
cargo publish --dry-run     # rehearse: package + build, but do NOT upload
cargo publish               # upload the .crate tarball to crates.io
```

> **Warning:** Unlike `npm unpublish`, there is **no** way to remove `slugify-rs 0.1.0` and re-upload different bytes under that same version. crates.io publishes are permanent. The closest tool is `cargo yank` (below), which only hides a version from _new_ dependency resolution; it does not delete anything.

---

## Detailed Explanation

### Authenticating with `cargo login`

Before your first publish, create an account at [crates.io](https://crates.io) (sign in with GitHub), then generate an API token under **Account Settings → API Tokens**. Run:

```bash
cargo login
```

Cargo prompts you to paste the token and stores it in `~/.cargo/credentials.toml`. This is the analogue of `npm login` writing an auth token to `~/.npmrc`. Modern tokens are **scoped**: when you create one on crates.io you choose which crates and which actions (publish-new, publish-update, yank) it may perform, much like a fine-grained npm automation token.

> **Tip:** For CI, do not run interactive `cargo login`. Pass the token directly: `cargo publish --token "$CRATES_IO_TOKEN"`, with the token stored as a CI secret.

### Required metadata

crates.io refuses to accept a crate that has no `description` and no `license` (or `license-file`). The `cargo publish --dry-run` step warns you about missing fields. Here is the **real** warning for a freshly-generated crate with no metadata:

```text
warning: manifest has no description, license, license-file, documentation, homepage or repository.
See https://doc.rust-lang.org/cargo/reference/manifest.html#package-metadata for more info.
```

The fields that matter, and their npm `package.json` cousins:

| `Cargo.toml` field | npm `package.json` field | Purpose |
| --- | --- | --- |
| `name` | `name` | Globally unique on the registry; reserved on first publish |
| `version` | `version` | SemVer triple; each value is published once, forever |
| `description` | `description` | **Required** by crates.io; one-line summary |
| `license` | `license` | **Required** (or `license-file`); SPDX expression |
| `repository` | `repository.url` | Source link shown on the crate page |
| `homepage` | `homepage` | Project landing page |
| `documentation` | (no direct equiv) | Usually `https://docs.rs/<crate>` |
| `readme` | (implicit `README.md`) | Rendered on the crate page |
| `keywords` | `keywords` | Up to 5; max 20 chars each |
| `categories` | (no equiv) | Must match the [fixed slug list](https://crates.io/category_slugs) |
| `exclude` / `include` | `files` | Controls what goes in the tarball |

> **Note:** `license = "MIT OR Apache-2.0"` is the de-facto standard for the Rust ecosystem. Dual-licensing under MIT and Apache-2.0 is what the standard library and most popular crates use. It is a single SPDX expression, not two separate fields.

### What gets uploaded: `cargo package`

`cargo publish` first runs the equivalent of `cargo package`: it collects your source into a `.crate` tarball, then **builds that tarball from scratch in an isolated directory** to verify it compiles standalone. This catches files you forgot to include. You can inspect the file list without uploading:

```bash
cargo package --list
```

By default Cargo includes everything tracked by git, minus your `.gitignore`d files, and always injects a normalized `Cargo.toml` plus the `Cargo.lock`. Use `exclude` to drop large fixtures, screenshots, or benchmark data the consumer does not need (as in the example's `exclude = ["/.github", "/benches", "*.png"]`), or `include` for an explicit allow-list (the analogue of npm's `files`).

### The `--dry-run` rehearsal

Always run `cargo publish --dry-run` first. It performs every step except the final upload. Here is the **real** tail of a successful dry run for the `slugify-rs` example:

```text
   Packaging slugify-rs v0.1.0 (/tmp/.../slugify-rs)
    Packaged 7 files, 2.7KiB (1.5KiB compressed)
   Verifying slugify-rs v0.1.0 (/tmp/.../slugify-rs)
   Compiling slugify-rs v0.1.0 (/tmp/.../slugify-rs/target/package/slugify-rs-0.1.0)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.00s
   Uploading slugify-rs v0.1.0 (/tmp/.../slugify-rs)
warning: aborting upload due to dry run
```

The `warning: aborting upload due to dry run` line confirms nothing was sent. Drop `--dry-run` to actually publish.

### After publishing

Within minutes, your crate appears at `https://crates.io/crates/slugify-rs`, and [docs.rs](https://docs.rs) automatically builds and hosts your API documentation at `https://docs.rs/slugify-rs`. There is no separate "publish the docs" step, which is a pleasant contrast to wiring up TypeDoc and a hosting provider yourself.

---

## Key Differences

### Versioning is SemVer, and Cargo enforces it

In npm, SemVer is a convention that the registry mostly trusts you to follow. In Rust, SemVer is woven into **dependency resolution**: a caret requirement like `serde = "1.0"` means "any `1.x` ≥ `1.0.0`", and the compiler/registry expectation is that a minor or patch bump never breaks downstream code. Choosing the right bump is therefore a contract:

```rust
use semver::Version;

/// Demonstrate which part of MAJOR.MINOR.PATCH to bump.
pub fn classify(old: &str, new: &str) -> &'static str {
    let o = Version::parse(old).unwrap();
    let n = Version::parse(new).unwrap();
    if n.major > o.major {
        "breaking change -> bump MAJOR"
    } else if n.minor > o.minor {
        "new backward-compatible feature -> bump MINOR"
    } else if n.patch > o.patch {
        "backward-compatible bug fix -> bump PATCH"
    } else {
        "no change"
    }
}
```

The accompanying tests (using the `semver` crate, the same SemVer engine Cargo uses internally) pass:

```text
running 1 test
test tests::bumps ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

| Change you made | Version bump | Why |
| --- | --- | --- |
| Fixed a bug, no API change | PATCH (`1.4.2` → `1.4.3`) | Existing callers keep working |
| Added a public function/type | MINOR (`1.4.2` → `1.5.0`) | Additive, backward-compatible |
| Removed/renamed a public item, changed a signature | MAJOR (`1.4.2` → `2.0.0`) | Breaks existing callers |

> **Note:** For `0.x` crates the rules shift left: under SemVer, `0.y.z` treats the **`y`** as the breaking-change component. Cargo follows this, so `0.1` and `0.2` are considered _incompatible_, and `version = "0.1"` resolves to `>=0.1.0, <0.2.0`. This is why pre-`1.0` crates can break you on a "minor" bump.

The pre-release ordering also follows SemVer precisely: `1.0.0-alpha.1 < 1.0.0`. Verified with the `semver` crate:

```text
major=1 minor=4 patch=2
1.4.2 matches ^1.2: true
2.0.0 matches ^1.2: false
1.0.0-alpha.1 < 1.0.0: true
```

### Immutability: published versions are forever

| Scenario | npm | crates.io |
| --- | --- | --- |
| Reuse a version number | `unpublish` within 72h, then re-publish | **Never** — the version is burned permanently |
| Remove a broken version from new installs | `unpublish` (with limits) | `cargo yank` (hides, does not delete) |
| Warn users off a version | `npm deprecate` | `cargo yank` + a note in the changelog/README |
| Fully delete a package | possible within limits | only crates.io admins, for legal/policy reasons |

The mental model shift: **treat every `cargo publish` as irreversible.** The `--dry-run` step exists precisely because there is no undo.

### Names are claimed on first publish

The first person to publish `foo` owns `foo`. crates.io has a single flat namespace with **no scopes**: there is no `@ada/slugify` equivalent. This is why crate names often carry suffixes like `-rs` or `-rust` to disambiguate. Names are normalized so that `slugify_rs` and `slugify-rs` are considered the same and cannot both be claimed.

---

## Common Pitfalls

### Pitfall 1: Publishing a crate that depends on a `path` dependency

A very common mistake when extracting a crate from a workspace: you depend on a sibling crate by `path`, but that sibling is not on crates.io. crates.io rejects path-only dependencies because consumers downloading from the registry cannot see your local filesystem.

```toml
# Cargo.toml — app_crate depends on an unpublished sibling
[dependencies]
util_lib = { version = "0.1.0", path = "../util_lib" }
```

Running `cargo publish` on `app_crate` produces this **real** error (`util_lib` is not on crates.io):

```text
   Packaging app_crate v0.1.0 (/tmp/.../app_crate)
    Updating crates.io index
error: failed to prepare local package for uploading

Caused by:
  no matching package named `util_lib` found
  location searched: crates.io index
  required by package `app_crate v0.1.0 (/tmp/.../app_crate)`
```

> **Tip:** During the publish, Cargo uses the `version` field of a `{ version = "...", path = "..." }` dependency and **ignores** the `path` (path is for local development only). So the fix is to **publish `util_lib` first**, then publish `app_crate`. Publish workspace members in dependency order, bottom-up.

### Pitfall 2: Missing `description` or `license`

If you skip the metadata, the dry run only _warns_, but the real crates.io upload **rejects** the crate. Add at minimum:

```toml
[package]
description = "One concise sentence about what this crate does"
license = "MIT OR Apache-2.0"
```

### Pitfall 3: Assuming you can unpublish like npm

There is no `cargo unpublish`. If you ship a broken `1.2.0`, you cannot replace its bytes — you must publish a fixed `1.2.1` and `cargo yank` the broken `1.2.0`. Plan releases accordingly, and lean on `--dry-run` plus a tag-based CI release flow.

### Pitfall 4: Forgetting that `version` is the source of truth

Cargo does not auto-increment for you the way `npm version patch` does (which also creates a git tag). You must edit the `version` field in `Cargo.toml` yourself (or use a helper like `cargo release` / `cargo-edit`'s `cargo set-version`). Trying to `cargo publish` a `version` that already exists on the registry fails: the registry rejects duplicates.

### Pitfall 5: Treating a minor bump as "safe" for `0.x` crates

As noted above, `0.1.x` → `0.2.0` is a **breaking** change under SemVer. If you are still pre-`1.0`, communicate breaking changes by bumping the second number, and do not be surprised when downstream `version = "0.1"` requirements do _not_ pick up your `0.2`.

---

## Best Practices

- **Rehearse with `cargo publish --dry-run`** every time. It packages and compiles the tarball in isolation, catching missing files and standalone-build failures before they become permanent.
- **Inspect the tarball** with `cargo package --list` to confirm you are not shipping secrets, fixtures, or multi-megabyte test data. Use `exclude`/`include` to trim it.
- **Fill in `repository`, `documentation`, `keywords`, and `categories`.** They power discovery on crates.io and make your crate page look professional. `documentation = "https://docs.rs/<crate>"` is conventional even though docs.rs builds automatically.
- **Add `rust-version`** (the Minimum Supported Rust Version, MSRV). Cargo errors early if a consumer's toolchain is too old, which is friendlier than a cryptic build failure deep in compilation.
- **Keep a `CHANGELOG.md`** and tag each release in git (`git tag v0.1.0`). crates.io itself does not store release notes, so the changelog is your record.
- **Automate releases in CI.** A common pattern: a workflow that triggers on a `v*` git tag, runs `cargo publish --token "$CRATES_IO_TOKEN"`. Trusted Publishing (OIDC, no long-lived token) is also supported by crates.io for GitHub Actions.
- **Dual-license `MIT OR Apache-2.0`** unless you have a specific reason not to — it matches ecosystem norms and maximizes who can depend on you.
- **Yank, do not panic.** If a release is broken, publish a fix and yank the bad one. Do not assume there is any way to delete it.

---

## Real-World Example

A realistic release procedure for a small library crate, end to end. Assume the working tree is clean and committed.

```toml
# Cargo.toml
[package]
name = "slugify-rs"
version = "0.2.0"            # bumped from 0.1.x: we added a public `Options` API (MINOR)
edition = "2024"
rust-version = "1.85"
description = "Convert arbitrary text into clean, URL-safe slugs"
license = "MIT OR Apache-2.0"
repository = "https://github.com/ada/slugify-rs"
documentation = "https://docs.rs/slugify-rs"
readme = "README.md"
keywords = ["slug", "url", "text", "string", "web"]
categories = ["text-processing", "web-programming"]
exclude = ["/.github", "/benches", "*.png"]

[dependencies]
```

```rust
// src/lib.rs
//! Convert arbitrary text into clean, URL-safe slugs.

/// Configuration for slug generation.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// The character inserted between words.
    pub separator: char,
    /// Cap the slug length (in characters); `None` means no limit.
    pub max_len: Option<usize>,
}

impl Default for Options {
    fn default() -> Self {
        Options { separator: '-', max_len: None }
    }
}

/// Slugify with default options (`-` separator, no length cap).
///
/// ```
/// assert_eq!(slugify_rs::slugify("Hello, World!"), "hello-world");
/// ```
pub fn slugify(input: &str) -> String {
    slugify_with(input, Options::default())
}

/// Slugify with explicit [`Options`].
///
/// ```
/// use slugify_rs::{slugify_with, Options};
/// let opts = Options { separator: '_', max_len: Some(7) };
/// assert_eq!(slugify_with("Hello, World!", opts), "hello_w");
/// ```
pub fn slugify_with(input: &str, opts: Options) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_sep = false;
    for ch in input.chars() {
        if let Some(max) = opts.max_len {
            if out.chars().count() >= max {
                break;
            }
        }
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_sep = false;
        } else if !prev_sep && !out.is_empty() {
            out.push(opts.separator);
            prev_sep = true;
        }
    }
    out.trim_end_matches(opts.separator).to_string()
}
```

The release run, with **real** doctest output verifying both examples compile and pass:

```text
running 2 tests
test src/lib.rs - slugify (line 21) ... ok
test src/lib.rs - slugify_with (line 30) ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

```bash
# 1. Verify everything (tests + doctests + lints)
cargo test
cargo clippy -- -D warnings

# 2. Rehearse the package and standalone build
cargo publish --dry-run

# 3. Tag the release in git so the changelog and registry stay in sync
git tag v0.2.0
git push origin v0.2.0

# 4. Publish for real
cargo publish
```

If a consumer later reports that `0.2.0` panics on certain Unicode input, the recovery flow is:

```bash
# Ship the fix as a new PATCH version...
# (edit Cargo.toml: version = "0.2.1", then)
cargo publish

# ...and discourage new installs of the broken version.
cargo yank --version 0.2.0
```

`cargo yank --version 0.2.0` removes `0.2.0` from the index for **new** resolution, while existing `Cargo.lock` files that already pin `0.2.0` keep building (so you do not break people mid-deploy). If it turns out the yank was a mistake, undo it:

```bash
cargo yank --version 0.2.0 --undo
```

> **Note:** Yanking is not deletion. The `0.2.0` `.crate` tarball stays downloadable so existing lockfiles still resolve; it just stops being chosen for _new_ requirements unless a lockfile explicitly pins it.

---

## Further Reading

- [The Cargo Book — Publishing on crates.io](https://doc.rust-lang.org/cargo/reference/publishing.html): the authoritative workflow reference.
- [The Cargo Book — The Manifest Format](https://doc.rust-lang.org/cargo/reference/manifest.html): every `[package]` metadata field.
- [`cargo yank` reference](https://doc.rust-lang.org/cargo/commands/cargo-yank.html): yanking and un-yanking.
- [SemVer Compatibility chapter](https://doc.rust-lang.org/cargo/reference/semver.html): exactly which changes are breaking vs. compatible in Rust.
- [crates.io category slugs](https://crates.io/category_slugs): the fixed list valid for `categories`.
- [docs.rs about page](https://docs.rs/about): how automatic documentation hosting works.
- Sibling pages in this section: [Cargo.toml manifest](/12-modules-packages/04-cargo/), [dependencies](/12-modules-packages/06-dependencies/), [workspaces](/12-modules-packages/08-workspaces/), [feature flags](/12-modules-packages/09-feature-flags/), [cargo commands](/12-modules-packages/05-cargo-commands/).
- Related guide sections: [01 — Getting Started](/01-getting-started/) for installing the toolchain, [02 — Basics](/02-basics/), and [13 — Testing](/13-testing/) for the `cargo test` step in your release checklist.

---

## Exercises

### Exercise 1: Make a minimal crate publishable

**Difficulty:** Easy

**Objective:** Take a default `cargo new --lib` crate and add exactly the metadata crates.io requires, then confirm with a dry run.

**Instructions:** Create a library crate, add the smallest set of fields that removes the "manifest has no description, license..." warning, and run `cargo publish --dry-run` (use `--allow-dirty` if you have not committed). List which two fields are strictly required.

<details>
<summary>Solution</summary>

The two strictly-required fields are `description` and `license` (or `license-file`).

```toml
# Cargo.toml
[package]
name = "tinytool"
version = "0.1.0"
edition = "2024"
description = "A tiny demonstration crate"
license = "MIT OR Apache-2.0"

[dependencies]
```

```bash
cargo publish --dry-run --allow-dirty
```

With both fields present, the metadata warning disappears and the dry run ends with the real line `warning: aborting upload due to dry run`, confirming the crate is ready to publish. (Adding `repository`, `keywords`, and `categories` is recommended but not required.)

</details>

### Exercise 2: Pick the right version bump

**Difficulty:** Medium

**Objective:** Apply SemVer rules to a sequence of API changes.

**Instructions:** Your crate is at `1.4.2`. For each change below, give the next version: (a) you fixed a typo in a doc comment; (b) you added a new public function `parse_strict`; (c) you removed the deprecated public function `parse_loose`. Then explain what version a downstream `version = "1.4"` requirement would resolve to after each release.

<details>
<summary>Solution</summary>

You can verify the classification mechanically with the same `semver` engine Cargo uses:

```rust
use semver::Version;

fn classify(old: &str, new: &str) -> &'static str {
    let o = Version::parse(old).unwrap();
    let n = Version::parse(new).unwrap();
    if n.major > o.major {
        "MAJOR (breaking)"
    } else if n.minor > o.minor {
        "MINOR (additive)"
    } else if n.patch > o.patch {
        "PATCH (fix)"
    } else {
        "no change"
    }
}

fn main() {
    println!("(a) {}", classify("1.4.2", "1.4.3")); // doc-only fix
    println!("(b) {}", classify("1.4.2", "1.5.0")); // new public fn
    println!("(c) {}", classify("1.4.2", "2.0.0")); // removed public fn
}
```

Real output:

```text
(a) PATCH (fix)
(b) MINOR (additive)
(c) MAJOR (breaking)
```

- **(a)** `1.4.3` — a doc-only change is backward compatible: PATCH.
- **(b)** `1.5.0` — adding a public item is backward compatible: MINOR.
- **(c)** `2.0.0` — removing a public item breaks callers: MAJOR.

A downstream `version = "1.4"` requirement (which means `>=1.4.0, <2.0.0`) would automatically pick up `1.4.3` and `1.5.0`. That is the whole point of compatible bumps. It would **not** pick up `2.0.0`; the consumer must opt in by changing their requirement to `"2"`.

</details>

### Exercise 3: Recover from a bad release

**Difficulty:** Hard

**Objective:** Walk through the correct recovery procedure when a published version is broken, and explain precisely what `cargo yank` does and does not do.

**Instructions:** You published `data-parser 0.3.0`, and it panics on empty input. A teammate suggests "just unpublish 0.3.0 and re-publish a fixed 0.3.0." Explain why that is impossible on crates.io, give the exact commands you would run instead, and describe what happens to (1) a brand-new project that runs `cargo add data-parser` and (2) an existing project whose `Cargo.lock` already pins `0.3.0`.

<details>
<summary>Solution</summary>

crates.io publishes are **permanent and immutable**: there is no `cargo unpublish`, and a version number, once used, can never be reused with different bytes. So you cannot "re-publish a fixed 0.3.0." The correct flow is to ship a fix under a new version and yank the broken one:

```bash
# Edit Cargo.toml: version = "0.3.1", commit the fix, then:
cargo test                      # confirm the panic is gone
cargo publish --dry-run         # rehearse
cargo publish                   # ship 0.3.1

cargo yank --version 0.3.0      # hide the broken 0.3.0 from NEW resolution
```

What `cargo yank` does:

1. **A brand-new project running `cargo add data-parser`** gets `0.3.1`: the yanked `0.3.0` is excluded from new dependency resolution, so it will never be freshly selected (unless explicitly pinned).
2. **An existing project whose `Cargo.lock` already pins `0.3.0`** keeps building exactly as before. Yanking does **not** delete the tarball; the `0.3.0` `.crate` remains downloadable so locked builds are not broken mid-flight. That project's maintainer should run `cargo update -p data-parser` (or `cargo update`) to move to `0.3.1`.

If you yanked by mistake, restore it with `cargo yank --version 0.3.0 --undo`. The point: yanking is a soft "stop offering this for new installs" signal, not a delete button.

</details>
