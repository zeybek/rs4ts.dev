---
title: "Hello World in Rust"
description: "Your first Rust program! We'll create a classic \"Hello World\" and compare it to TypeScript/JavaScript."
---

Your first Rust program! We'll create a classic "Hello World" and compare it to TypeScript/JavaScript.

---

## Quick Overview

Rust programs start with a `main` function, just like many other languages. Unlike JavaScript, which is interpreted, Rust is compiled to a native executable.

**Time required:** 15-20 minutes

---

## TypeScript/JavaScript Example

Let's start with something familiar:

```typescript
// hello.ts
function main() {
  console.log("Hello, world!");
}

main();
```

**Running it:**

```bash
# Option 1: With ts-node
ts-node hello.ts

# Option 2: Compile then run
tsc hello.ts     # Creates hello.js
node hello.js    # Runs the JS file
```

**What happens:**

1. TypeScript is transpiled to JavaScript
2. Node.js interprets the JavaScript
3. V8 JIT-compiles hot code paths
4. Output appears in terminal

---

## Rust Equivalent

```rust
// hello.rs
fn main() {
    println!("Hello, world!");
}
```

**Running it:**

```bash
# Compile
rustc hello.rs   # Creates executable 'hello'

# Run
./hello          # macOS/Linux
hello.exe        # Windows
```

**What happens:**

1. Rust is compiled to native machine code
2. Linker creates standalone executable
3. No runtime or interpreter needed
4. Output appears in terminal

---

## Detailed Explanation

### Line-by-Line Comparison

**TypeScript:**

```typescript
function main() {
  // Function declaration
  console.log("Hello, world!"); // Print to console
}

main(); // Call the function
```

**Rust:**

```rust
fn main() {                    // Function declaration
    println!("Hello, world!"); // Print to console (with newline)
}
                               // main() is auto-called (entry point)
```

### Key Syntax Differences

| Aspect           | TypeScript                     | Rust                          |
| ---------------- | ------------------------------ | ----------------------------- |
| Function keyword | `function` or `=>`             | `fn`                          |
| Print function   | `console.log()`                | `println!()`                  |
| Statement end    | `;` optional (ASI; most styles keep it) | `;` required                  |
| Block style      | `{` same line or next          | `{` same line (idiomatic)     |
| Indentation      | 2 spaces (common)              | 4 spaces (standard)           |
| Entry point      | Call `main()` explicitly       | `main()` called automatically |
| Macro syntax     | None (no macros)               | `!` indicates macro           |

### The `println!` Macro

Notice the `!` in `println!()`:

```rust
println!("Hello, world!");  // Macro (notice the !)
```

**What's a macro?**

- Code that writes code at compile time
- Like a template that expands before compilation
- `println!` is a macro, not a function

**Why `!`?**

- The `!` simply marks a **macro invocation** — it has nothing to do with format strings
- Macros can do things functions can't (e.g. accept a variable number of arguments and check the format string at compile time)
- You'll see it on `vec!`, `format!`, `assert!`, and many others

**Compare to TypeScript:**

```typescript
// TypeScript has no macro system
// Closest equivalent: template literals
console.log(`Hello, world!`);
```

**Rust also has `print!` (without newline):**

```rust
print!("Hello, ");   // No newline
print!("world!\n");  // Explicit newline
```

---

## Multiple Ways to Create and Run

### Method 1: Single File with rustc

**Create the file:**

```bash
# Create hello.rs
echo 'fn main() {
    println!("Hello, world!");
}' > hello.rs
```

**Compile and run:**

```bash
# Compile
rustc hello.rs

# Run
./hello
```

**Pros:**

- Simple and direct
- No additional files needed
- Good for learning

**Cons:**

- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> No dependency management
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> No standard project structure
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Manual compilation

**When to use:** Quick tests, learning exercises

---

### Method 2: Cargo Project (Recommended)

**Create project:**

```bash
# Create new project
cargo new hello_world
cd hello_world
```

**This creates:**

```
hello_world/
├── Cargo.toml          # Project manifest (like package.json)
└── src/
    └── main.rs         # Your code (already has Hello World!)
```

**Run it:**

```bash
cargo run
```

**Output:**

```text
   Compiling hello_world v0.1.0 (/path/to/hello_world)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s
     Running `target/debug/hello_world`
Hello, world!
```

**Pros:**

- Standard project structure
- Dependency management built-in
- Automatic compilation
- Testing framework included

**Cons:**

- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Slightly more complex for tiny scripts

**When to use:** Any real project (always use this!)

**Compare to Node.js/TypeScript:**

```bash
# Node.js equivalent
npm init -y           # Create package.json
npm install -D typescript @types/node
npx tsc --init        # Create tsconfig.json
# Create src/index.ts
npm run dev           # (after configuring package.json)
```

Cargo is simpler - one command!

---

### Method 3: Cargo Script (nightly-only, unstable)

> **Warning:** Single-file "cargo scripts" are an **unstable** feature. As of
> Rust 1.96.0 they only work on the **nightly** toolchain behind the `-Zscript`
> flag. On stable, `cargo` rejects an embedded manifest with
> `error: embedded manifest ... requires -Zscript`.

When enabled, a script carries its dependencies in a `---` TOML **frontmatter**
block at the top of the file (this is the real syntax — there is no
`// cargo.toml` comment manifest):

```rust
#!/usr/bin/env -S cargo +nightly -Zscript
---
[package]
edition = "2024"
---

fn main() {
    println!("Hello, world!");
}
```

**Run directly:**

```bash
chmod +x hello.rs
./hello.rs
```

**When to use:** Quick automation scripts — but only once you are on nightly and
accept that the format may still change before it stabilizes. For anything real,
prefer Method 2 (a Cargo project).

---

## Key Differences from TypeScript/JavaScript

### 1. Compilation

**TypeScript:**

```bash
tsc hello.ts          # Transpiles to hello.js
node hello.js         # Node.js interprets

# Size: hello.js is ~same size as hello.ts
```

**Rust:**

```bash
rustc hello.rs        # Compiles to machine code

# Size: executable is larger (~400KB for "Hello World")
# But runs much faster and needs no runtime
```

**Why Rust binary is larger:**

- Includes all dependencies statically
- No runtime needed
- Can be stripped for production

**Strip symbols for smaller binary:**

```bash
strip hello           # Removes debug symbols (roughly a 20-30% cut for a tiny binary)
```

### 2. Startup Time

**TypeScript/Node.js:**

```bash
time node hello.js
# real 0m0.050s (50ms)
```

**Rust:**

```bash
time ./hello
# real 0m0.001s (1ms)
```

**Why:** Rust is native code, no runtime or interpreter startup.

### 3. Entry Point

**TypeScript:**

```typescript
// Must explicitly call main
function main() {
  console.log("Hello!");
}

main(); // ← Required!
```

**Rust:**

```rust
// main() is automatically called at program start
fn main() {
    println!("Hello!");
}
// No explicit call needed
```

### 4. Print Statement

**TypeScript/JavaScript:**

```typescript
console.log("Hello"); // No newline is added
console.log("World"); // Separate line

// Output:
// Hello
// World
```

**Rust:**

```rust
println!("Hello");         // Newline added automatically
println!("World");         // Separate line

print!("Hello");           // No newline
print!(" World\n");        // Manual newline

// Both output:
// Hello
// World
```

---

## Common Pitfalls

### Pitfall 1: Forgetting the `!` in println

**Wrong:**

```rust
fn main() {
    println("Hello, world!");  // Error: expected function, found macro `println`
}
```

**Right:**

```rust
fn main() {
    println!("Hello, world!"); // Correct: println! is a macro
}
```

**Error message:**

```text
error[E0423]: expected function, found macro `println`
 --> hello.rs:2:5
  |
2 |     println("Hello, world!");
  |     ^^^^^^^ not a function
  |
help: use `!` to invoke the macro
  |
2 |     println!("Hello, world!");
  |            +
```

**Why:** `println!` is a macro, not a function. The `!` is required.

### Pitfall 2: Missing Semicolon

A missing `;` is only an error when another statement follows it (or when a
non-`()` value would be silently discarded). A single trailing expression with
no semicolon is fine — it just becomes the block's return value.

**Compiles and runs** (the trailing expression evaluates to `()`):

```rust
fn main() {
    println!("Hello, world!")  // OK as the last expression — no `;` needed
}
```

**Error** — the moment a second statement follows the un-terminated one:

```rust
fn main() {
    println!("Hello, world!")  // now this needs a `;`
    println!("Again!");
}
```

**Error message:**

```text
error: expected `;`, found `println`
 --> hello.rs:2:30
  |
2 |     println!("Hello, world!")
  |                              ^ help: add `;` here
3 |     println!("Again!");
  |     ------- unexpected token

error: aborting due to 1 previous error
```

**Why:** Unlike JavaScript (which inserts semicolons automatically), Rust uses
`;` to separate statements. The safe habit is to end every statement with `;`;
omit it only on a deliberate trailing expression.

### Pitfall 3: Wrong File Extension

**Wrong:**

```bash
# Creating hello.js or hello.txt
echo 'fn main() { ... }' > hello.js
rustc hello.js  // Won't work well
```

**Right:**

```bash
# Use .rs extension
echo 'fn main() { ... }' > hello.rs
rustc hello.rs  //
```

**Why:** While rustc might compile it, tooling (rust-analyzer, cargo) expects `.rs` files.

### Pitfall 4: Trying to Run the .rs File

**Wrong:**

```bash
./hello.rs  // Cannot execute text file
```

**Right:**

```bash
# Compile first
rustc hello.rs

# Then run the executable
./hello     //
```

**Why:** Rust is compiled, not interpreted. You run the compiled binary, not the source file.

---

## Best Practices

### 1. Always Use Cargo for Projects

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Don't:**

```bash
rustc my_app.rs
rustc module1.rs
rustc module2.rs
# Manual compilation is painful
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Do:**

```bash
cargo new my_app
cd my_app
cargo run
# Cargo handles everything
```

### 2. Use `cargo run` for Development

**During development:**

```bash
cargo run                    # Debug build, fast compile
```

**For production:**

```bash
cargo build --release       # Optimized build, slow compile, fast execution
./target/release/hello_world
```

**Compare compile times:**

```bash
time cargo build            # ~0.5s (debug)
time cargo build --release  # ~3s (release, but executable is 10x faster)
```

### 3. Format Your Code

```bash
# Format a single file
rustfmt hello.rs

# Format entire project
cargo fmt

# Check without formatting
cargo fmt -- --check
```

**Rust standard style:**

- 4-space indentation (not 2!)
- Opening brace on same line
- No trailing commas in single-line

### 4. Use `println!` for Debugging

```rust
fn main() {
    let x = 42;
    println!("The value is: {}", x);        // Format with variables
    println!("x = {x}");                     // Shorter syntax (Rust 2021+)
    println!("Multiple: {} and {}", x, 10);  // Multiple values
}
```

**Compare to TypeScript:**

```typescript
const x = 42;
console.log("The value is:", x); // Comma-separated
console.log(`The value is: ${x}`); // Template literal
```

---

## Real-World Example: Slightly More Complex Hello World

Let's make it more interesting:

**TypeScript:**

```typescript
function greet(name: string): void {
  console.log(`Hello, ${name}!`);
}

function main(): void {
  const names = ["Alice", "Bob", "Charlie"];
  names.forEach((name) => greet(name));
}

main();
```

**Rust:**

```rust
fn greet(name: &str) {
    println!("Hello, {}!", name);
}

fn main() {
    let names = vec!["Alice", "Bob", "Charlie"];
    for name in names {
        greet(name);
    }
}
```

**Run it:**

```bash
# Save as greet.rs
rustc greet.rs
./greet
```

**Output:**

```
Hello, Alice!
Hello, Bob!
Hello, Charlie!
```

**Key differences:**

- `&str` is a string slice (we'll learn this in section 02)
- `vec![]` is a vector (like an array)
- `for` loop instead of `.forEach()`
- No `void` return type needed

---

## Further Reading

### Official Documentation

- [The Rust Book - Hello World](https://doc.rust-lang.org/book/ch01-02-hello-world.html)
- [println! macro docs](https://doc.rust-lang.org/std/macro.println.html)
- [Macros in Rust](https://doc.rust-lang.org/book/ch19-06-macros.html)

### Related Topics

- [Cargo Book - Creating a New Project](https://doc.rust-lang.org/cargo/guide/creating-a-new-project.html)
- [rustc Book](https://doc.rust-lang.org/rustc/)

---

## Exercises

### Exercise 1: Basic Hello World

Create and run a Hello World program using both methods:

```bash
# Method 1: rustc
echo 'fn main() { println!("Hello from rustc!"); }' > hello1.rs
rustc hello1.rs
./hello1

# Method 2: cargo
cargo new hello2
cd hello2
cargo run
```

### Exercise 2: Modify the Message

Change the program to print your name:

```rust
fn main() {
    println!("Hello, <YOUR_NAME>!");
}
```

### Exercise 3: Multiple Prints

Create a program that prints multiple lines:

```rust
fn main() {
    println!("Line 1");
    println!("Line 2");
    println!("Line 3");
}
```

### Exercise 4: Print Without Newline

Use `print!` to print on the same line:

```rust
fn main() {
    print!("Hello, ");
    print!("world!");
    println!();  // Add newline at end
}
```

### Exercise 5: Format with Variables

```rust
fn main() {
    let name = "Rustacean";
    let age = 5;
    println!("My name is {} and I am {} years old", name, age);
}
```

<details>
<summary>All Solutions</summary>

All solutions are provided in the exercises above! Try modifying them and running with `cargo run`.

</details>

---

## Summary

**What you've learned:**

- Basic Rust syntax (`fn main`, `println!`)
- How to compile with `rustc`
- How to create projects with `cargo`
- Differences between `print!` and `println!`
- Key differences from TypeScript/JavaScript

**Key syntax:**

```rust
fn main() {                    // Entry point
    println!("Text");          // Print with newline
    println!("Value: {}", x);  // Print with formatting
}
```

**Commands to remember:**

```bash
rustc file.rs          # Compile single file
cargo new project      # Create new project
cargo run              # Build and run
cargo fmt              # Format code
```

**You're ready to learn Cargo in depth!**
