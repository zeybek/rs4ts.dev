---
title: "Rust Playground"
description: "Try Rust in your browser without installing anything! The Rust Playground is an online tool for experimenting, learning, and sharing code."
---

Try Rust in your browser without installing anything! The Rust Playground is an online tool for experimenting, learning, and sharing code.

---

## Quick Overview

The Rust Playground ([play.rust-lang.org](https://play.rust-lang.org/)) lets you write Rust code in your browser and compile/run it on Rust's servers (in a sandbox, not in the browser itself). It's perfect for:

- Quick experiments
- Learning Rust
- Sharing code snippets
- Testing ideas

**Time required:** 10-15 minutes to explore

---

## TypeScript/JavaScript Comparison

**TypeScript Playground:**

- [TypeScript Playground](https://www.typescriptlang.org/play)
- Transpiles TypeScript to JavaScript
- Shows type errors
- Browser-based

**Rust Playground:**

- [Rust Playground](https://play.rust-lang.org/)
- Compiles and runs your code on Rust's servers in a sandbox (not in your browser)
- Shows compiler errors
- Can switch between editions
- Can run tests
- Can format code
- Can share via URL

**Similar tools:**

- [CodeSandbox](https://codesandbox.io/) - Full dev environment
- [StackBlitz](https://stackblitz.com/) - Online VS Code
- [Replit](https://replit.com/) - Multi-language online IDE

---

## Accessing the Playground

**URL:** https://play.rust-lang.org/

**When you open it, you see:**

```rust playground
fn main() {
    println!("Hello, world!");
}
```

**Ready to run immediately!** Click "Run" or press `Ctrl+Enter`.

---

## Detailed Explanation

### Main Features

#### 1. Code Editor

- Syntax highlighting
- Auto-completion
- Error underlining (as you type!)
- Multi-cursor editing (`Ctrl+Click`)

**Try this:**

```rust playground
fn main() {
    let x = 5;
    println!("x = {}", x);
}
```

#### 2. Compiler Output

**Click "Run"** and see:

```
   Compiling playground v0.0.1 (/playground)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.50s
     Running `target/debug/playground`
x = 5
```

- Shows compilation steps
- Shows program output
- Shows any errors or warnings

#### 3. Share Button

**Create shareable link:**

1. Write your code
2. Click "Share"
3. Copy the URL
4. Anyone with the URL can see and run your code

**Example URL:**

```
https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=abc123xyz
```

**Compare to TypeScript Playground:**

- TypeScript encodes code in URL
- Rust uses gist (cleaner URLs)

#### 4. Configuration Options

**Edition:**

- 2015 (original Rust)
- 2018 (modern features)
- 2021 (widely used)
- 2024 (latest stable edition, recommended for new code)

**Mode:**

- Debug (fast compile, slow runtime)
- Release (slow compile, fast runtime)

**Channel:**

- Stable (most recent release)
- Beta (next release preview)
- Nightly (latest features, unstable)

### Top Toolbar

```
[Run ▶] [Format] [Clippy] [Miri] [Tools ▼] [Config ▼] [Share] [Help]
```

**Run** - Compile and execute
**Format** - Auto-format code (like `cargo fmt`)
**Clippy** - Lint code (like `cargo clippy`)
**Miri** - Advanced interpreter (for unsafe code)
**Tools** - ASM, LLVM IR, MIR, macro expansion, etc.
**Config** - Edition, mode, channel
**Share** - Get shareable link
**Help** - Keyboard shortcuts

> **Note:** The exact button labels and grouping shift between Playground versions. The capabilities below are stable even if the menu names change.

---

## Key Differences from TypeScript Playground

### 1. Actual Compilation

**TypeScript Playground:**

```typescript
let x = 5;
console.log(x);
// Transpiled to JavaScript, runs in browser JS engine
```

**Rust Playground:**

```rust
let x = 5;
println!("{}", x);
// Sent to Rust's servers, compiled to a native binary, run in a sandbox
```

> **Note:** Unlike the TypeScript Playground (which transpiles and runs entirely in your browser), the Rust Playground sends your code to a backend, compiles it with a real `rustc`/`cargo` toolchain, and runs the resulting native binary in a sandboxed container. That's why it needs a moment to respond and why it can report genuine compile and runtime errors.

**Why it matters:** Rust Playground shows real compile errors, not just type errors.

### 2. More Tool Integration

**TypeScript:**

- Basic editor
- Type checking
- JS output

**Rust:**

- Editor with completions
- Compiler errors/warnings
- Format code
- Lint code
- View assembly
- Run tests
- View macro expansions

### 3. Shareable Builds

**Rust Playground can share:**

- Source code
- Compiler output
- Standard output
- Entire configuration

**Perfect for:**

- Asking questions on forums
- Reporting bugs
- Teaching examples

---

## Common Pitfalls

### Pitfall 1: Expecting File System Access

**Problem:**

```rust
use std::fs;

fn main() {
    let content = fs::read_to_string("file.txt").unwrap();
    println!("{}", content);
}
```

**Error:**

```
No such file or directory (os error 2)
```

**Why:** The playground is sandboxed, no file system access.

**Solution:** Use string literals instead:

```rust playground
fn main() {
    let content = "This is my file content";
    println!("{}", content);
}
```

### Pitfall 2: Network Requests Don't Work

**Problem:**

```rust
// This won't work!
let response = reqwest::get("https://api.example.com").await?;
```

**Why:** No network access in the playground.

**Solution:** For network code, install Rust locally.

### Pitfall 3: Limited Dependencies

**Problem:**

```rust
use some_obscure_crate::Thing;  // Not available
```

**Why:** Playground has only the top 100 crates.

**Available crates:**

- Standard library (always)
- Popular crates (serde, tokio, regex, etc.)

**To check if a crate is available:**
Look for the "ADD CRATE" button (shows available crates).

### Pitfall 4: Execution Time Limit

**Problem:**

```rust
fn main() {
    loop {
        // Infinite loop
    }
}
```

**Result:** Timeout after 30 seconds.

**Solution:** Write code that completes quickly.

---

## Best Practices

### 1. Use for Quick Tests

**Good use:**

```rust playground
// Test how Option works
fn main() {
    let x: Option<i32> = Some(5);
    match x {
        Some(val) => println!("Value: {}", val),
        None => println!("No value"),
    }
}
```

Quick test, immediate feedback!

### 2. Format Before Sharing

**Always click "Format"** before sharing code:

- Makes code readable
- Follows Rust conventions
- Professional appearance

### 3. Use Clippy for Learning

**Click "Clippy"** to get suggestions. Clippy goes beyond compiler errors and flags non-idiomatic code:

```rust playground
fn main() {
    let s = String::from("hello");
    if s.len() == 0 {
        println!("empty");
    }
}
```

**Clippy reports:**

```
warning: length comparison to zero
 --> src/main.rs:3:8
  |
3 |     if s.len() == 0 {
  |        ^^^^^^^^^^^^ help: using `is_empty` is clearer and more explicit: `s.is_empty()`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#len_zero
  = note: `#[warn(clippy::len_zero)]` on by default
```

**Learn from suggestions!**

### 4. Share Examples in Forums

When asking for help:

1. Write minimal example in playground
2. Click "Share"
3. Post the link

**Example:**
"I'm having trouble with this: https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=abc123"

Much better than pasting code in a forum post!

---

## Real-World Example: Testing an Idea

**Scenario:** You want to test how Rust's `Result` type works.

**1. Go to playground**
**2. Write code:**

```rust playground
fn divide(a: i32, b: i32) -> Result<i32, String> {
    if b == 0 {
        Err("Cannot divide by zero".to_string())
    } else {
        Ok(a / b)
    }
}

fn main() {
    match divide(10, 2) {
        Ok(result) => println!("Result: {}", result),
        Err(e) => println!("Error: {}", e),
    }

    match divide(10, 0) {
        Ok(result) => println!("Result: {}", result),
        Err(e) => println!("Error: {}", e),
    }
}
```

**3. Click "Run"**
**Output:**

```
Result: 5
Error: Cannot divide by zero
```

**4. Click "Format"** to clean up
**5. Click "Clippy"** for suggestions
**6. Click "Share"** if you want to save it

---

## Advanced Features

### 1. View Assembly

Open the tools/options menu and choose to show the assembly output.

See the actual machine code generated (x86-64, the Playground's target):

```rust
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

**Assembly output:**

```asm
playground::add:
        lea     eax, [rdi + rsi]
        ret
```

**Why useful:** Understanding optimization.

### 2. View Macro Expansion

Open the tools/options menu and choose to expand macros (this uses the nightly toolchain).

See what macros expand to:

```rust playground
fn main() {
    println!("Hello, {}", "world");
}
```

**Expanded:**

```rust
fn main() {
    {
        ::std::io::_print(format_args!("Hello, {0}\n", "world"));
    };
}
```

**Why useful:** Understanding how macros work.

### 3. Run Tests

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[test]
fn test_add() {
    assert_eq!(add(2, 3), 5);
}

#[test]
fn test_add_negative() {
    assert_eq!(add(-1, 1), 0);
}
```

**Click "Test"** instead of "Run":

```
running 2 tests
test test_add ... ok
test test_add_negative ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

---

## Keyboard Shortcuts

The most reliable shortcut is `Ctrl+Enter` (run the code). The editor also supports many of the usual code-editor bindings for commenting, moving lines, multi-cursor editing, and find/replace.

| Shortcut        | Action (typical)       |
| --------------- | ---------------------- |
| `Ctrl+Enter`    | Run code               |
| `Ctrl+K Ctrl+C` | Comment selection      |
| `Ctrl+K Ctrl+U` | Uncomment selection    |
| `Ctrl+/`        | Toggle comment         |
| `Alt+↑/↓`       | Move line up/down      |
| `Ctrl+D`        | Select next occurrence |
| `Ctrl+F`        | Find                   |
| `Ctrl+H`        | Find and replace       |

> **Note:** Editor bindings can differ by browser, platform, and Playground version, so treat the table above as a guide rather than a guarantee. Formatting and Clippy are run from their toolbar buttons. For the current authoritative list, click "Help" in the toolbar.

---

## Comparison to Local Development

| Feature              | Playground      | Local (cargo)             |
| -------------------- | --------------- | ------------------------- |
| Installation         | None needed     | rustup install            |
| Startup time         | Instant         | cargo run ~0.5s           |
| File system access | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> No | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes |
| Network access | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> No | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes |
| Available crates     | Top 100 only    | All crates                |
| Execution time limit | 30 seconds      | No limit                  |
| Code sharing | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Easy (URL) | Manual (gist, github) |
| Editor features      | Basic           | Full (with rust-analyzer) |
| Debugging | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Limited | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Full (lldb/gdb) |
| Performance          | Slightly slower | Full native speed         |

**When to use Playground:**

- Learning basics
- Quick tests
- Sharing examples
- No installation needed

**When to use local:**

- Real projects
- Need file system
- Need specific crates
- Performance testing

---

## Further Reading

### Official Resources

- [Rust Playground](https://play.rust-lang.org/)
- [Playground Repository](https://github.com/rust-lang/rust-playground)
- [Available Crates List](https://play.rust-lang.org/help)

### Similar Tools

- [Godbolt Compiler Explorer](https://godbolt.org/z/Rust) - View assembly
- [Rust Explorer](https://www.rustexplorer.com/) - Another online IDE

### Community

- Share playground links on [r/rust](https://reddit.com/r/rust)
- Use in [Rust Discord](https://discord.gg/rust-lang)
- Reference in GitHub issues

---

## Exercises

### Exercise 1: Hello Playground

1. Go to https://play.rust-lang.org/
2. Modify the default program
3. Click "Run"
4. Click "Format"
5. Click "Share" and copy the URL

### Exercise 2: Test Compiler Errors

Try this code:

```rust
fn main() {
    let x = 5;
    x = 10;  // Error!
    println!("{}", x);
}
```

Read the error message. What does it say?

<details>
<summary>Answer</summary>

```
error[E0384]: cannot assign twice to immutable variable `x`
```

Variables are immutable by default in Rust!

**Fix:**

```rust playground
fn main() {
    let mut x = 5;  // Add 'mut'
    x = 10;
    println!("{}", x);
}
```

</details>

### Exercise 3: Use Clippy

Write this code:

```rust playground
fn main() {
    let s = String::from("hello");
    let len = s.len();
    println!("{}", len);
}
```

Click "Clippy". Does it suggest anything?

### Exercise 4: Format Messy Code

Paste this:

```rust playground
fn main(){let x=5;let y=10;println!("{} {}",x,y);}
```

Click "Format". See the difference!

### Exercise 5: Share Your Code

1. Write any Rust code
2. Click "Share"
3. Open the link in a private/incognito window
4. Verify your code is there!

---

## Summary

**What you've learned:**

- How to use the Rust Playground
- When to use it vs local development
- How to share code via URL
- Available tools (Format, Clippy, etc.)
- Limitations (no file system, network)

**Rust Playground features:**

- Instant code execution
- Compiler error messages
- Format and lint tools
- Easy sharing
- No installation needed

**Best for:**

- Learning Rust
- Quick experiments
- Sharing examples
- Teaching

**Not suitable for:**

- Real projects
- File I/O
- Network requests
- Large dependencies

**URL to remember:** https://play.rust-lang.org/

**You're now ready to start learning Rust syntax!**
