---
title: "Output and Formatting"
description: "Learn how to print output in Rust using the println! macro and its rich formatting capabilities."
---

Learn how to print output in Rust using the `println!` macro and its rich formatting capabilities.

---

## Quick Overview

Rust uses macros for printing:

- `println!()` - Print with newline
- `print!()` - Print without newline
- Format strings use `{}` placeholders
- Rich formatting options built-in

**Similar to Python's f-strings or C's printf!**

---

## TypeScript/JavaScript Example

```typescript
// Simple printing
console.log("Hello, world!");

// With variables
const name = "Alice";
const age = 30;

// Template literals
console.log(`Hello, ${name}!`);
console.log(`${name} is ${age} years old`);

// Multiple values
console.log("Name:", name, "Age:", age);

// Formatting numbers
const pi = 3.14159;
console.log(pi.toFixed(2)); // "3.14"

// Objects
const user = { name: "Bob", age: 25 };
console.log(user); // { name: 'Bob', age: 25 }
console.log(JSON.stringify(user)); // {"name":"Bob","age":25}
```

**TypeScript uses template literals and method calls.**

---

## Rust Equivalent

```rust
// Simple printing
println!("Hello, world!");

// With variables
let name = "Alice";
let age = 30;

// Format strings
println!("Hello, {}!", name);
println!("{} is {} years old", name, age);

// Inline capture (placeholder names a variable in scope)
println!("Hello, {name}!");

// Named arguments (placeholder maps to an explicit value)
println!("Hello, {who}!", who = name);

// Formatting numbers
let pi = 3.14159;
println!("{:.2}", pi); // "3.14"

// Debug formatting
let user = ("Bob", 25);
println!("{:?}", user); // ("Bob", 25)

// Pretty debug
println!("{:#?}", user);
```

**Rust uses macros with expressive format strings.**

---

## Detailed Explanation

### Basic Printing

```rust
fn main() {
    // Print with newline
    println!("Hello!");
    println!("World!");

    // Print without newline
    print!("Hello, ");
    print!("World!\n");

    // Print to stderr
    eprintln!("Error message!");
}
```

**Output:**

```
Hello!
World!
Hello, World!
Error message!
```

### Placeholders

```rust
fn main() {
    let name = "Alice";
    let age = 30;

    // Positional
    println!("{} is {} years old", name, age);

    // Indexed
    println!("{0} is {1} years old", name, age);
    println!("{1} is the age of {0}", name, age); // Reorder!

    // Inline capture (names a variable already in scope)
    println!("{name} is {age} years old");

    // Named arguments (the placeholder maps to a value you pass)
    println!("{who} is {years} years old", who = name, years = age);

    // Mixed positional and named
    println!("{0} is {years} years old", name, years = age);
}
```

**Compare to TypeScript:**

```typescript
const name = "Alice";
const age = 30;

// Template literals
console.log(`${name} is ${age} years old`);

// Only sequential order possible
```

### Display vs Debug

**Display (`{}`)** - Human-readable output:

```rust
let x = 42;
let s = "hello";
let b = true;

println!("{}", x);  // 42
println!("{}", s);  // hello
println!("{}", b);  // true
```

**Debug (`{:?}`)** - Programmer-friendly output:

```rust
let tuple = (1, "hello", true);
let vec = vec![1, 2, 3];

// println!("{}", tuple);  // Error: tuples don't implement Display
println!("{:?}", tuple);   // (1, "hello", true)
println!("{:?}", vec);     // [1, 2, 3]
```

**Pretty Debug (`{:#?}`)** - Formatted debug:

```rust
let data = vec![
    ("Alice", 30),
    ("Bob", 25),
    ("Charlie", 35),
];

println!("{:#?}", data);
```

**Output:**

```
[
    (
        "Alice",
        30,
    ),
    (
        "Bob",
        25,
    ),
    (
        "Charlie",
        35,
    ),
]
```

### Number Formatting

```rust
let pi = 3.14159;

// Decimal places
println!("{:.2}", pi);      // 3.14
println!("{:.4}", pi);      // 3.1416

// Width and alignment
println!("{:10}", 42);      // "        42" (right-aligned)
println!("{:<10}", 42);     // "42        " (left-aligned)
println!("{:^10}", 42);     // "    42    " (centered)

// Zero-padding
println!("{:05}", 42);      // 00042

// Sign
println!("{:+}", 42);       // +42
println!("{:+}", -42);      // -42

// Hexadecimal
println!("{:x}", 255);      // ff
println!("{:X}", 255);      // FF
println!("{:#x}", 255);     // 0xff

// Binary
println!("{:b}", 42);       // 101010
println!("{:#b}", 42);      // 0b101010

// Octal
println!("{:o}", 42);       // 52
println!("{:#o}", 42);      // 0o52
```

**Compare to TypeScript:**

```typescript
const pi = 3.14159;

console.log(pi.toFixed(2)); // "3.14"
console.log(pi.toPrecision(4)); // "3.142"

const n = 255;
console.log(n.toString(16)); // "ff"
console.log(n.toString(2)); // "11111111"
```

### String Formatting

```rust
let name = "Alice";

// Truncate
println!("{:.5}", name);     // Alice (already < 5)
println!("{:.3}", name);     // Ali

// Width and fill
println!("{:10}", name);     // "Alice     "
println!("{:<10}", name);    // "Alice     " (left)
println!("{:>10}", name);    // "     Alice" (right)
println!("{:^10}", name);    // "  Alice   " (center)
println!("{:*<10}", name);   // "Alice*****" (custom fill)
```

### Escape Sequences

```rust
println!("New line\n");
println!("Tab\there");
println!("Backslash: \\");
println!("Quote: \"");
println!("Single quote: \'");
println!("Unicode: \u{1F44D}");  //
```

### Format! Macro

Create formatted strings without printing:

```rust
let name = "Alice";
let age = 30;

// Create a String
let s = format!("{} is {} years old", name, age);
println!("{}", s);  // Alice is 30 years old

// Useful for building strings
let filename = format!("report_{}.txt", 2024);
```

**Compare to TypeScript:**

```typescript
const name = "Alice";
const age = 30;

const s = `${name} is ${age} years old`;
console.log(s);
```

---

## Key Differences from TypeScript/JavaScript

### 1. Macros vs Functions

**TypeScript:**

```typescript
console.log("Hello"); // Function call
```

**Rust:**

```rust
println!("Hello"); // Macro invocation (note the !)
```

**Why macros?** They're expanded at compile time and can check format strings!

### 2. Format String Syntax

**TypeScript:**

```typescript
const name = "Alice";
console.log(`Hello, ${name}!`); // Template literal
```

**Rust:**

```rust
let name = "Alice";
println!("Hello, {}!", name); // Format string
```

### 3. Debug Printing

**TypeScript:**

```typescript
const obj = { name: "Alice", age: 30 };
console.log(obj); // Pretty print in browser/Node.js
console.log(JSON.stringify(obj)); // Manual serialization
```

**Rust:**

```rust
let tuple = ("Alice", 30);
// println!("{}", tuple);  // Error
println!("{:?}", tuple);   // Debug format
println!("{:#?}", tuple);  // Pretty debug
```

### 4. Number Formatting

**TypeScript:**

```typescript
const n = 42;
console.log(n.toString(16)); // "2a" (hex)
console.log(n.toString(2)); // "101010" (binary)
```

**Rust:**

```rust
let n = 42;
println!("{:x}", n); // 2a (hex)
println!("{:b}", n); // 101010 (binary)
```

---

## Common Pitfalls

### Pitfall 1: Forgetting Braces

**Problem:**

```rust
let name = "Alice";
println!("Hello, name!"); // Prints literally "Hello, name!"
```

**Solution:**

```rust
let name = "Alice";
println!("Hello, {}!", name); // "Hello, Alice!"
```

### Pitfall 2: Type Doesn't Implement Display

**Problem:**

```rust
let tuple = (1, 2);
println!("{}", tuple); // Error: tuple doesn't implement Display
```

**Solution:**

```rust
let tuple = (1, 2);
println!("{:?}", tuple); // Use debug format
```

### Pitfall 3: Wrong Number of Arguments

**Problem:**

```rust
println!("{} and {}", 42); // Error: 2 placeholders, 1 argument
```

**Solution:**

```rust
println!("{} and {}", 42, 100); // Match placeholders
```

### Pitfall 4: Trying to Format References

**Problem:**

```rust
let s = String::from("hello");
println!("{}", &s); // Works, but...
```

**Why it works:** Rust automatically dereferences for Display.

**Better:**

```rust
let s = String::from("hello");
println!("{}", s); // Clearer
```

---

## Best Practices

### 1. Use Debug for Development

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Implement Display for everything:**

```rust
struct Point { x: i32, y: i32 }
// Lots of boilerplate to implement Display...
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Use derive Debug:**

```rust
#[derive(Debug)]
struct Point { x: i32, y: i32 }

let p = Point { x: 10, y: 20 };
println!("{:?}", p); // Point { x: 10, y: 20 }
```

### 2. Use Inline Captures for Clarity

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Unclear:**

```rust
println!("User {} (ID: {}) logged in at {}", name, id, time);
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Clear:**

```rust
println!("User {name} (ID: {id}) logged in at {time}");
```

> **Tip:** Since Rust 2021, named placeholders capture identifiers from the
> surrounding scope directly. The older explicit form
> `println!("{name}", name = name)` is exactly what `clippy::uninlined_format_args`
> flags, with the suggestion to drop the redundant `name = name` argument.

### 3. Use format! for String Building

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Concatenation:**

```rust
let s = "Hello, ".to_string() + name + "!";
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Use format!:**

```rust
let s = format!("Hello, {}!", name);
```

### 4. Pretty Print Complex Data

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Hard to read:**

```rust
println!("{:?}", complex_data);
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Pretty print:**

```rust
println!("{:#?}", complex_data);
```

---

## Real-World Example

### Logging with Formatting

**TypeScript:**

```typescript
function log(level: string, message: string) {
  const timestamp = new Date().toISOString();
  console.log(`[${timestamp}] ${level}: ${message}`);
}

log("INFO", "Server started");
log("ERROR", "Connection failed");
```

**Rust:**

```rust
fn log(level: &str, message: &str) {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    println!("[{}] {}: {}", timestamp, level, message);
}

fn main() {
    log("INFO", "Server started");
    log("ERROR", "Connection failed");
}
```

### Displaying Table Data

```rust
fn print_table(users: &[(&str, u8, f64)]) {
    println!("{:<15} {:>5} {:>10}", "Name", "Age", "Score");
    println!("{:-<32}", ""); // Separator

    for (name, age, score) in users {
        println!("{:<15} {:>5} {:>10.2}", name, age, score);
    }
}

fn main() {
    let users = [
        ("Alice", 30, 95.5),
        ("Bob", 25, 87.3),
        ("Charlie", 35, 92.0),
    ];

    print_table(&users);
}
```

**Output:**

```
Name              Age      Score
--------------------------------
Alice              30      95.50
Bob                25      87.30
Charlie            35      92.00
```

---

## Format Specifiers Reference

| Specifier | Description           | Example                 | Output     |
| --------- | --------------------- | ----------------------- | ---------- |
| `{}`      | Display               | `println!("{}", 42)`    | 42         |
| `{:?}`    | Debug                 | `println!("{:?}", x)`   | (1, 2)     |
| `{:#?}`   | Pretty debug          | `println!("{:#?}", x)`  | Multi-line |
| `{:b}`    | Binary                | `println!("{:b}", 42)`  | 101010     |
| `{:x}`    | Lowercase hex         | `println!("{:x}", 42)`  | 2a         |
| `{:X}`    | Uppercase hex         | `println!("{:X}", 42)`  | 2A         |
| `{:o}`    | Octal                 | `println!("{:o}", 42)`  | 52         |
| `{:p}`    | Pointer               | `println!("{:p}", &x)`  | 0x7fff...  |
| `{:e}`    | Lowercase exponential | `println!("{:e}", pi)`  | 3.14159e0  |
| `{:E}`    | Uppercase exponential | `println!("{:E}", pi)`  | 3.14159E0  |
| `{:.N}`   | Precision (N digits)  | `println!("{:.2}", pi)` | 3.14       |
| `{:W}`    | Width (W characters)  | `println!("{:5}", 42)`  | " 42"      |
| `{:<W}`   | Left align            | `println!("{:<5}", 42)` | "42 "      |
| `{:>W}`   | Right align           | `println!("{:>5}", 42)` | " 42"      |
| `{:^W}`   | Center align          | `println!("{:^5}", 42)` | " 42 "     |
| `{:0W}`   | Zero pad              | `println!("{:05}", 42)` | 00042      |
| `{:+}`    | Always show sign      | `println!("{:+}", 42)`  | +42        |

---

## Further Reading

### Official Documentation

- [std::fmt module](https://doc.rust-lang.org/std/fmt/)
- [The Rust Book - Output](https://doc.rust-lang.org/book/ch03-05-control-flow.html)
- [println! macro](https://doc.rust-lang.org/std/macro.println.html)

---

## Exercises

### Exercise 1: Basic Printing

Print "Hello, Rust!" three times:

```rust
fn main() {
    // Your code here
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    println!("Hello, Rust!");
    println!("Hello, Rust!");
    println!("Hello, Rust!");
}
```

</details>

### Exercise 2: Variable Printing

Print a person's name and age in format: "Alice is 30 years old"

```rust
fn main() {
    let name = "Alice";
    let age = 30;
    // Your code here
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let name = "Alice";
    let age = 30;
    println!("{} is {} years old", name, age);
}
```

</details>

### Exercise 3: Number Formatting

Format π (3.14159) with 2 decimal places:

```rust
fn main() {
    let pi = 3.14159;
    // Your code here
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let pi = 3.14159;
    println!("{:.2}", pi); // 3.14
}
```

</details>

### Exercise 4: Debug Printing

Print a vector using debug format:

```rust
fn main() {
    let numbers = vec![1, 2, 3, 4, 5];
    // Your code here
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let numbers = vec![1, 2, 3, 4, 5];
    println!("{:?}", numbers); // [1, 2, 3, 4, 5]
}
```

</details>

### Exercise 5: Create Formatted String

Create a string "User: Alice, Score: 95.5" using format!:

```rust
fn main() {
    let name = "Alice";
    let score = 95.5;
    let s = /* your code */;
    println!("{}", s);
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let name = "Alice";
    let score = 95.5;
    let s = format!("User: {}, Score: {}", name, score);
    println!("{}", s);
}
```

</details>

---

## Summary

**What you've learned:**

- `println!` and `print!` macros
- Format placeholders (`{}`, `{:?}`, etc.)
- Number formatting (precision, width, padding)
- Debug vs Display formatting
- `format!` macro for string building
- Named and positional parameters

**Key macros:**

```rust
println!()   // Print with newline
print!()     // Print without newline
eprintln!()  // Print to stderr
format!()    // Create formatted String
```

**Common patterns:**

```rust
println!("{}", x);        // Display
println!("{:?}", x);      // Debug
println!("{:#?}", x);     // Pretty debug
println!("{:.2}", pi);    // Precision
println!("{:10}", n);     // Width
```

**Formatting is expressive and type-safe in Rust!**
