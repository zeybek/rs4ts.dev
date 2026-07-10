---
title: "Understanding Cargo"
description: "Cargo is Rust's build system and package manager. Think of it as npm, webpack, jest, prettier, and eslint all combined into one tool."
---

Cargo is Rust's build system and package manager. Think of it as npm, webpack, jest, prettier, and eslint all combined into one tool.

---

## Quick Overview

Cargo is **the** Rust tool. It handles:

- Project creation
- Dependency management
- Building and compiling
- Running tests
- Generating documentation
- Publishing packages
- Running benchmarks
- And much more!

**Time required:** 30-40 minutes

---

## TypeScript/JavaScript Comparison

**Node.js ecosystem (multiple tools):**

```bash
# Package manager
npm init
npm install express
npm install -D typescript

# Build tool
npx webpack

# Test runner
npx jest

# Formatter
npx prettier --write .

# Linter
npx eslint .

# Documentation
npx typedoc
```

**Rust (one tool - Cargo):**

```bash
# All-in-one tool
cargo new my_project    # Create project
cargo add axum          # Add dependency
cargo build             # Build
cargo test              # Run tests
cargo fmt               # Format code
cargo clippy            # Lint
cargo doc               # Generate docs
```

**Everything through one consistent interface!**

---

## Rust Equivalent: Creating a Project

**TypeScript/Node.js:**

```bash
# Create directory
mkdir my-project
cd my-project

# Initialize
npm init -y

# Install TypeScript
npm install -D typescript @types/node

# Create tsconfig.json
npx tsc --init

# Create src directory
mkdir src
echo 'console.log("Hello");' > src/index.ts

# Add scripts to package.json (manual)
# ...

# Run
npx ts-node src/index.ts
```

**Result:** `package.json`, `tsconfig.json`, `node_modules/`, `src/`

**Rust:**

```bash
# Create project (one command!)
cargo new my-project
cd my-project

# Run
cargo run
```

**Result:** `Cargo.toml`, `src/main.rs`, ready to go!

**That's it!** No configuration needed, batteries included.

---

## Detailed Explanation

### Project Structure

When you run `cargo new my_project`, you get:

```
my_project/
├── Cargo.toml          # Project manifest (like package.json)
├── .git/               # Git repository (auto-initialized)
├── .gitignore          # Ignores /target (just one line)
└── src/
    └── main.rs         # Entry point (with Hello World)
```

**Compare to Node.js/TypeScript:**

```
my-project/
├── package.json        # Project manifest
├── tsconfig.json       # TypeScript config
├── .gitignore          # Manual creation
├── node_modules/       # Dependencies (can be huge!)
└── src/
    └── index.ts        # Entry point (empty)
```

### Cargo.toml vs package.json

**Cargo.toml (Rust):**

```toml
[package]
name = "my_project"
version = "0.1.0"
edition = "2024"

[dependencies]
# Dependencies go here
```

> **Note:** You don't pick the `edition` yourself. `cargo new` writes the newest edition supported by your toolchain into `Cargo.toml` automatically — on a recent stable toolchain that is `"2024"` (the latest stable edition). Editions are opt-in language revisions; existing code keeps compiling because crates of different editions interoperate.

**package.json (Node.js):**

```json
{
  "name": "my-project",
  "version": "0.1.0",
  "main": "dist/index.js",
  "scripts": {
    "build": "tsc",
    "start": "node dist/index.js",
    "dev": "ts-node src/index.ts",
    "test": "jest"
  },
  "dependencies": {},
  "devDependencies": {
    "typescript": "^5.0.0",
    "@types/node": "^18.0.0"
  }
}
```

**Key differences:**

- Cargo.toml is simpler (TOML vs JSON)
- No "scripts" section needed (cargo commands are standard)
- No "main" field (uses `src/main.rs` or `src/lib.rs` by convention)
- No devDependencies distinction (just `[dependencies]` and `[dev-dependencies]`)

---

## Essential Cargo Commands

### 1. Project Creation

```bash
# Create binary (executable) project
cargo new my_app

# Create library project
cargo new my_lib --lib
```

**Compare to npm:**

```bash
npm init -y              # Creates package.json
# But doesn't create any code structure
```

### 2. Building

```bash
# Debug build (fast compile, slow runtime)
cargo build

# Release build (slow compile, fast runtime)
cargo build --release

# Check without building (faster)
cargo check
```

**Output locations:**

- Debug: `target/debug/my_app`
- Release: `target/release/my_app`

**Compare to TypeScript:**

```bash
tsc                      # Compile to JavaScript
npx webpack              # Bundle
npm run build            # (custom script)
```

**Build times comparison:**

```
cargo build            # 0.5s (debug)
cargo build --release  # 3-10s (release, but 10x faster execution)
tsc                    # 0.1-1s (depends on project size)
```

### 3. Running

```bash
# Build and run (debug)
cargo run

# Build and run (release)
cargo run --release

# Run with arguments
cargo run -- arg1 arg2
```

**The `--` separator** passes arguments to your program, not to cargo.

```bash
# These arguments go to cargo
cargo run --release

# These arguments go to your program
cargo run -- --help
```

**Compare to Node.js:**

```bash
node dist/index.js         # Run compiled JS
npx ts-node src/index.ts   # Run TS directly
npm start                  # (custom script)
```

### 4. Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture
```

**Compare to JavaScript:**

```bash
npm test            # (jest, vitest, etc.)
npm run test:watch
```

Cargo's test runner is built-in, no configuration needed!

### 5. Dependencies

```bash
# Add dependency
cargo add serde

# Add dev dependency
cargo add --dev criterion

# Add with specific version
cargo add serde --version 1.0

# Remove dependency
cargo remove serde

# Update dependencies
cargo update
```

**Compare to npm:**

```bash
npm install express
npm install -D jest
npm install express@4.18.0
npm uninstall express
npm update
```

> **Note:** `cargo add` is built into Cargo (since Cargo 1.62, June 2022) — no extra install needed. You may still see older tutorials say "run `cargo install cargo-edit` first"; that hasn't been required for years.

You can always edit `Cargo.toml` by hand instead if you prefer:

```toml
[dependencies]
serde = "1.0"
```

### 6. Code Quality

```bash
# Format code (like Prettier)
cargo fmt

# Check formatting without changing
cargo fmt -- --check

# Lint (like ESLint)
cargo clippy

# Lint with auto-fix suggestions
cargo clippy --fix
```

**Compare to JavaScript:**

```bash
npx prettier --write .
npx prettier --check .
npx eslint .
npx eslint --fix .
```

### 7. Documentation

```bash
# Build documentation
cargo doc

# Build and open in browser
cargo doc --open

# Include private items
cargo doc --document-private-items
```

**Compare to TypeScript:**

```bash
npx typedoc --out docs src/
```

Cargo docs are generated from code comments:

````rust
/// Returns the sum of two numbers
///
/// # Examples
///
/// ```
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
````

**`cargo doc`** automatically generates beautiful HTML documentation!

---

## Key Differences from npm

### 1. No node_modules!

**Node.js:**

```bash
npm install
# Creates node_modules/ (can be gigabytes!)
# Need to .gitignore it
# Re-download for every project
```

**Rust:**

```bash
cargo build
# Downloads to ~/.cargo/registry/ (shared across projects)
# Only compiled artifacts in target/
# Reused across projects
```

**Why it matters:**

- Faster builds (shared cache)
- Less disk space
- Faster git operations (no node_modules to scan)

### 2. Lock Files

**package-lock.json (npm):**

```json
{
  "name": "my-project",
  "lockfileVersion": 3,
  "requires": true,
  "packages": {
    "": {
      // ... hundreds of lines ...
    }
  }
}
```

- Always commit to git
- Ensures reproducible builds
- Can be huge (thousands of lines)

**Cargo.lock (Rust):**

```toml
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
[[package]]
name = "my_project"
version = "0.1.0"

[[package]]
name = "serde"
version = "1.0.152"
```

- Commit for binaries, don't commit for libraries
- Ensures reproducible builds
- Usually smaller

### 3. Scripts

**package.json:**

```json
{
  "scripts": {
    "build": "tsc",
    "start": "node dist/index.js",
    "dev": "ts-node src/index.ts",
    "test": "jest",
    "lint": "eslint .",
    "format": "prettier --write ."
  }
}
```

**Cargo:** No scripts needed! Everything is a standard command:

```bash
cargo build    # Standard
cargo run      # Standard
cargo test     # Standard
cargo fmt      # Standard
cargo clippy   # Standard
```

**For custom tasks** (the equivalent of npm `scripts`), Cargo has a couple of native options and several community tools:

- **Cargo aliases** — define command shortcuts in `.cargo/config.toml`:

  ```toml
  # .cargo/config.toml
  [alias]
  br = "build --release"
  ```

  Then `cargo br` runs `cargo build --release`. Aliases only chain Cargo subcommands, not arbitrary shell.
- **`cargo xtask`** — a convention where build automation lives in an ordinary Rust binary you run with `cargo xtask <task>`.
- **`cargo-make`** — a third-party task runner (`cargo install cargo-make`) for richer, cross-platform task definitions.

> **Note:** Unlike npm, Cargo has no built-in `scripts` table that runs shell commands. But most projects don't need one — the standard subcommands cover the common workflow.

### 4. Semantic Versioning

**Both** use semver, but Cargo enforces it more strictly:

**Cargo.toml:**

```toml
[dependencies]
serde = "1.0"      # Caret range: >=1.0.0, <2.0.0
tokio = "1.25.0"   # ALSO a caret range: >=1.25.0, <2.0.0 (NOT exact!)
tokio_exact = "=1.25.0"  # THIS is exact: only 1.25.0
regex = "1"        # Caret range: >=1.0.0, <2.0.0
```

> **Warning:** A bare version string like `"1.25.0"` is **not** an exact pin — Cargo treats it as the caret requirement `>=1.25.0, <2.0.0`. To require an exact version, prefix it with `=`, as in `"=1.25.0"`.

**package.json:**

```json
{
  "dependencies": {
    "express": "^4.18.0", // 4.18.0 <= version < 5.0.0
    "lodash": "4.17.21", // Exact
    "react": "^18" // Latest 18.x
  }
}
```

**Cargo is more conservative** with updates by default.

---

## Common Pitfalls

### Pitfall 1: Forgetting to Rebuild

**Problem:**

```bash
# Edit main.rs
# Run old binary
./target/debug/my_app  # Runs OLD version!
```

**Solution:**

```bash
cargo run              # Always rebuilds if needed
```

**Or:**

```bash
cargo build
./target/debug/my_app  # Now runs new version
```

### Pitfall 2: Large target/ Directory

**Problem:**

```bash
du -sh target/
# 5.0G  target/  # Huge!
```

**Solution:**

```bash
# Clean build artifacts
cargo clean

# Or clean all Rust projects
cargo install cargo-cache
cargo cache --autoclean
```

**Why it gets big:**

- Multiple debug/release builds
- Dependencies compiled for your project
- Test binaries

**Compare to node_modules:**

```bash
du -sh node_modules/
# 500M  node_modules/  # Also huge!
```

But `target/` can be cleaned and regenerated quickly.

### Pitfall 3: Slow First Build

**Problem:**

```bash
cargo build
# Compiling 150 crates... (takes 5 minutes)
```

**Why:** Cargo compiles all dependencies from source.

**Solutions:**

1. **Use `cargo check` during development:**

```bash
cargo check  # Faster, just type-checks
```

2. **Use release mode only when needed:**

```bash
cargo build  # Debug mode (fast compile)
# Only use --release for production
```

3. **Use `sccache` for caching:**

```bash
cargo install sccache
export RUSTC_WRAPPER=sccache
```

### Pitfall 4: Committing Cargo.lock (or not)

**Rule:**

- **Binary projects:** Commit `Cargo.lock`
- **Library projects:** Don't commit `Cargo.lock`

**Why:**

- Binaries need reproducible builds
- Libraries want to test with latest compatible versions

**Add to .gitignore (for libraries):**

```text
/target/
/Cargo.lock
```

---

## Best Practices

### 1. Use cargo fmt on Save

**VS Code settings.json:**

```json
{
  "[rust]": {
    "editor.formatOnSave": true
  }
}
```

**Or manually:**

```bash
cargo fmt
```

**Before commit:**

```bash
cargo fmt --check  # Verify formatting
```

### 2. Run clippy Before Commit

```bash
cargo clippy
```

**Or make it strict:**

```bash
cargo clippy -- -D warnings  # Treat warnings as errors
```

**CI integration:**

```yaml
# .github/workflows/ci.yml
- name: Run clippy
  run: cargo clippy -- -D warnings
```

### 3. Use cargo check During Development

**Instead of:**

```bash
cargo build     # Slow: creates executable
```

**Use:**

```bash
cargo check     # Fast: only type-checks
```

**3-5x faster!** Use during development, `cargo build` only when you need to run.

### 4. Separate Dev and Prod Dependencies

```toml
[dependencies]
# Production dependencies
serde = "1.0"
tokio = "1.25"

[dev-dependencies]
# Only used in tests/benchmarks
criterion = "0.5"
mock_instant = "0.3"
```

**Compare to package.json:**

```json
{
  "dependencies": {
    /* prod */
  },
  "devDependencies": {
    /* dev only */
  }
}
```

### 5. Use Workspaces for Monorepos

```
my-workspace/
├── Cargo.toml      # Workspace manifest
├── api/
│   ├── Cargo.toml
│   └── src/
├── cli/
│   ├── Cargo.toml
│   └── src/
└── shared/
    ├── Cargo.toml
    └── src/
```

**Root Cargo.toml:**

```toml
[workspace]
members = ["api", "cli", "shared"]
```

**Compare to npm workspaces:**

```json
{
  "workspaces": ["packages/*"]
}
```

Similar concept, different implementation.

---

## Real-World Example: Web API Project

**Create project:**

```bash
cargo new web_api
cd web_api
```

**Add dependencies:**

```bash
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
```

**Or manually edit Cargo.toml:**

```toml
[package]
name = "web_api"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }

[profile.release]
opt-level = 3        # Maximum optimization
lto = true           # Link-time optimization
codegen-units = 1    # Better optimization, slower compile
```

**Build and run:**

```bash
# Development
cargo run

# Production
cargo build --release
./target/release/web_api
```

**Compare to Node.js/Express:**

```bash
npm init -y
npm install express typescript @types/node @types/express
npm install -D ts-node nodemon
# Edit tsconfig.json
# Edit package.json scripts
npm run dev
```

Cargo is simpler!

---

## Further Reading

### Official Documentation

- [The Cargo Book](https://doc.rust-lang.org/cargo/) - Complete guide
- [Cargo Commands Reference](https://doc.rust-lang.org/cargo/commands/index.html)
- [Manifest Format (Cargo.toml)](https://doc.rust-lang.org/cargo/reference/manifest.html)

### Useful Tools

- [cargo-edit](https://github.com/killercup/cargo-edit) - `cargo upgrade`, `cargo set-version` (`cargo add`/`cargo remove` are now built into Cargo)
- [cargo-watch](https://github.com/watchexec/cargo-watch) - Auto-rebuild on file change
- [cargo-expand](https://github.com/dtolnay/cargo-expand) - Expand macros
- [cargo-tree](https://doc.rust-lang.org/cargo/commands/cargo-tree.html) - View dependency tree (built-in)

### Comparisons

- [Cargo vs npm](https://www.reddit.com/r/rust/comments/3x4c2h/cargo_vs_npm/)
- [Cargo for npm users](https://github.com/Mercateo/rust-for-node-developers/blob/master/Cargo-for-npm-users.md)

---

## Exercises

### Exercise 1: Create and Run a Project

```bash
cargo new hello_cargo
cd hello_cargo
cargo run
```

### Exercise 2: Add a Dependency

Add the `rand` crate and use it:

**Cargo.toml:**

```toml
[dependencies]
rand = "0.9"
```

**src/main.rs:**

```rust
use rand::Rng;

fn main() {
    let random_number = rand::rng().random_range(1..=100);
    println!("Random number: {random_number}");
}
```

> **Note:** rand 0.9 renamed the old `thread_rng()` to `rng()` and `gen_range()` to `random_range()` (because `gen` became a reserved keyword in edition 2024). If you follow a rand 0.8 tutorial, those are the two calls to update.

**Run:**

```bash
cargo run
```

### Exercise 3: Explore cargo Commands

```bash
cargo build           # Build
cargo clean           # Clean
cargo doc --open      # Generate and open docs
cargo tree            # Show dependency tree
cargo --version       # Show version
cargo --list          # List all commands
```

### Exercise 4: Create a Library

```bash
cargo new my_lib --lib
cd my_lib
```

**src/lib.rs:**

```rust
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
    }
}
```

**Run tests:**

```bash
cargo test
```

### Exercise 5: Format and Lint

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Lint
cargo clippy
```

---

## Summary

**What you've learned:**

- Cargo is the all-in-one Rust tool
- How to create projects (`cargo new`)
- How to build and run (`cargo build`, `cargo run`)
- How to manage dependencies (edit `Cargo.toml`)
- How to test, format, and lint
- Key differences from npm

**Essential commands:**

```bash
cargo new <project>      # Create project
cargo run                # Build and run
cargo test               # Run tests
cargo fmt                # Format code
cargo clippy             # Lint
cargo doc --open         # Generate docs
cargo clean              # Clean build artifacts
```

**Cargo.toml structure:**

```toml
[package]
name = "my_project"
version = "0.1.0"
edition = "2024"   # cargo new fills in the newest edition for you

[dependencies]
# Add dependencies here
```

**You now understand Rust's tooling!**
