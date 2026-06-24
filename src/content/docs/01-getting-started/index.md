---
title: "Getting Started with Rust"
sidebar:
  label: "Overview"
description: "Welcome to Rust! This section will get you up and running with your first Rust program."
---

Welcome to Rust! This section will get you up and running with your first Rust program.

---

## What's in This Section

- **[Why Rust](/01-getting-started/00-why-rust/)** - Detailed reasons to learn Rust as a TS/JS developer
- **[Installing Rust](/01-getting-started/01-installation/)** - Setup guide for all platforms
- **[Hello World](/01-getting-started/02-hello-world/)** - Your first Rust program
- **[Understanding Cargo](/01-getting-started/03-cargo-basics/)** - Rust's build tool and package manager
- **[Rust Playground](/01-getting-started/04-playground/)** - Try Rust without installing anything

---

## Learning Objectives

By the end of this section, you will:

- Understand why Rust is valuable for TS/JS developers
- Have Rust installed and configured on your system
- Write and run your first Rust program
- Understand Cargo and its role (think: npm + webpack + more)
- Know how to use the online Rust Playground

---

## Time Estimate

- **Reading:** 45-60 minutes
- **Installation & Setup:** 15-30 minutes
- **Hands-on Practice:** 30-45 minutes
- **Total:** 1.5-2 hours

---

## Quick Start Path

If you want to jump right in:

1. **[Install Rust](/01-getting-started/01-installation/)** (15 minutes)
2. **[Hello World](/01-getting-started/02-hello-world/)** (15 minutes)
3. **[Cargo Basics](/01-getting-started/03-cargo-basics/)** (15 minutes)

Then come back and read the other topics at your leisure.

---

## Detailed Learning Path

### Recommended Order

```
1. Why Rust (understand the motivation)
   ↓
2. Installation (get Rust on your machine)
   ↓
3. Hello World (write your first program)
   ↓
4. Cargo Basics (learn the tooling)
   ↓
5. Playground (for quick experiments)
```

### Alternative: Hands-On First

```
1. Playground (try Rust immediately, no install)
   ↓
2. Installation (once you're convinced)
   ↓
3. Hello World
   ↓
4. Why Rust (understand what you just did)
   ↓
5. Cargo Basics
```

---

## What You'll Learn

### Why Rust? (vs TypeScript/Node.js)

Compare Rust to what you already know:

**TypeScript/Node.js:**

- Interpreted/JIT compiled
- Garbage collected
- Single-threaded event loop
- Runtime errors
- 50-100ms startup time
- 50-200 MB memory baseline

**Rust:**

- Compiled to native code
- No garbage collection
- Safe, compiler-checked, opt-in multithreading (fearless concurrency)
- Compile-time error checking
- <1ms startup time
- 1-5 MB memory baseline

### Installation

You'll learn:

- How to install Rust using rustup
- How to verify your installation
- How to update Rust
- Editor setup for the best experience

### Hello World

Your first Rust program:

```rust playground
fn main() {
    println!("Hello, world!");
}
```

Compared to TypeScript, where top-level code runs directly with no required entry-point function:

```typescript
console.log("Hello, world!");
```

### Cargo: The Complete Toolchain

Cargo is like npm, webpack, and more combined:

| Feature         | TypeScript/JS | Rust        |
| --------------- | ------------- | ----------- |
| Package Manager | npm/yarn/pnpm | Cargo       |
| Build Tool      | webpack/vite  | Cargo       |
| Test Runner     | jest/vitest   | Cargo       |
| Formatter       | prettier      | rustfmt     |
| Linter          | eslint        | clippy      |
| Documentation   | typedoc       | rustdoc     |
| Benchmarking    | (various)     | Cargo bench |
| Task Runner     | npm scripts   | Cargo       |

**All in one tool!**

### The Playground

[play.rust-lang.org](https://play.rust-lang.org/) lets you:

- Try Rust without installing anything
- Share code snippets
- Test ideas quickly
- Learn with instant feedback

---

## What Makes Rust Different

### 1. Compiled, Not Interpreted

**TypeScript/JavaScript:**

```bash
node app.js        # V8 parses, then JIT-compiles hot code at runtime
ts-node app.ts     # Strips types, then runs through the same V8 pipeline
```

**Rust:**

```bash
rustc main.rs      # Compiled to native binary
./main             # Execute native code
```

**Impact:**

- Faster execution, especially CPU-bound work (no GC pauses)
- Smaller deployments (single binary)
- No runtime dependencies
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Slower build times (seconds vs milliseconds)

### 2. No Garbage Collector

**TypeScript/JavaScript:**

- Automatic memory management
- GC pauses (unpredictable timing)
- Memory overhead
- Easy to use

**Rust:**

- Ownership system (compile-time memory management)
- No GC pauses (predictable performance)
- Minimal memory overhead
- Learning curve

### 3. Stronger Type System

**TypeScript:**

```typescript
let x: any = "hello"; // Escape hatch — type checking is disabled
x = 42; // No complaint from the compiler
let y = x.length; // No error: silently `undefined` at runtime
```

**Rust:**

```rust
let x = "hello";      // Type inference
let y = x.len();      // Always safe, or won't compile
```

### 4. Explicit Error Handling

**TypeScript:**

```typescript
function divide(a: number, b: number): number {
  if (b === 0) {
    throw new Error("Division by zero"); // Exception
  }
  return a / b;
}
```

**Rust:**

```rust
fn divide(a: i32, b: i32) -> Result<i32, String> {
    if b == 0 {
        Err("Division by zero".to_string()) // Explicit error value
    } else {
        Ok(a / b)
    }
}
```

**No exceptions in Rust!** All errors are explicit in the type system.

---

## Your Goals for This Section

### Minimum Goals

- [ ] Install Rust on your machine
- [ ] Run `cargo --version` successfully
- [ ] Create and run a "Hello World" program
- [ ] Understand what Cargo does

### Stretch Goals

- [ ] Configure your editor with rust-analyzer
- [ ] Experiment with the Rust Playground
- [ ] Create a custom Cargo project
- [ ] Read Cargo.toml and understand its structure

---

## Key Concepts Preview

### Concepts You'll Encounter

**rustup** - The Rust toolchain installer (like nvm for Node.js)

**cargo** - Build tool and package manager (like npm + webpack)

**rustc** - The Rust compiler. Where `tsc` erases types and emits JavaScript that still needs a runtime, `rustc` produces a self-contained native binary.

**crates** - Rust packages (like npm packages)

**Cargo.toml** - Project manifest (like package.json)

**main.rs** - Entry point file (like index.ts)

Don't worry if these don't make sense yet - they will soon!

---

## Common First-Time Issues

### "rustup: command not found"

**Problem:** Installation didn't add rustup to PATH

**Solution:**

```bash
# Restart your terminal first.
# On macOS/Linux you can also source the env file in the current shell:
source "$HOME/.cargo/env"
```

> **Note:** `source ~/.cargo/env` is a Unix shell command and does **not** work in Windows PowerShell or `cmd.exe`. On Windows, the rustup installer adds Cargo to your `PATH` automatically; just open a new terminal window.

### "cargo: command not found"

**Problem:** Same as above

**Solution:** Restart your terminal (Windows), or source the cargo env file in the current shell (macOS/Linux)

### "error: linking with `cc` failed"

**Problem:** Missing C compiler (Rust needs it for linking)

**Solution:**

- **macOS:** `xcode-select --install`
- **Linux:** `sudo apt install build-essential`
- **Windows:** Install Visual Studio Build Tools

### Slow Compilation

**Problem:** First compile is always slow

**Why:** Rust compiles dependencies from source

**Tips:**

- Use `cargo build` without `--release` during development
- Dependencies are cached after first build
- Consider using `sccache` for faster recompilation

---

## Additional Resources

### Official Resources

- [Rust Installation Guide](https://www.rust-lang.org/tools/install)
- [Cargo Book](https://doc.rust-lang.org/cargo/)
- [Rust Playground](https://play.rust-lang.org/)

### Community Resources

- [Rust Discord](https://discord.gg/rust-lang)
- [r/rust](https://reddit.com/r/rust)
- [This Week in Rust](https://this-week-in-rust.org/)

---

## Ready to Begin?

Let's get Rust installed and write your first program!

### [Start with Why Rust](/01-getting-started/00-why-rust/)

Or jump straight to:

- [Installation Guide](/01-getting-started/01-installation/)
- [Hello World](/01-getting-started/02-hello-world/)
