---
title: "Comments and Documentation"
description: "Comments in Rust are similar to TypeScript/JavaScript, with built-in additions for generating documentation automatically."
---

Comments in Rust are similar to TypeScript/JavaScript, with built-in additions for generating documentation automatically.

---

## Quick Overview

Rust has three types of comments:

- **Regular comments:** `//` and `/* */` (like JS/TS)
- **Doc comments:** `///` and `//!` (generates HTML docs!)
- **No JSDoc needed:** Built into the language

**Doc comments are a first-class feature in Rust!**

---

## TypeScript/JavaScript Example

```typescript
// Single-line comment

/* 
   Multi-line comment
   across several lines
*/

/**
 * JSDoc documentation comment
 * @param name - User's name
 * @param age - User's age
 * @returns Greeting message
 */
function greet(name: string, age: number): string {
  return `Hello, ${name}! You are ${age} years old.`;
}

// Inline comment at end of line
let x = 5; // This is x
```

**JSDoc is a convention, not built into JavaScript.**

---

## Rust Equivalent

```rust
// Single-line comment

/*
   Multi-line comment
   across several lines
*/

/// Documentation comment for the function
///
/// # Arguments
/// * `name` - User's name
/// * `age` - User's age
///
/// # Returns
/// A greeting message
fn greet(name: &str, age: u32) -> String {
    format!("Hello, {}! You are {} years old.", name, age)
}

// Inline comment at end of line
let x = 5; // This is x
```

**Doc comments are built into Rust and generate HTML docs!**

---

## Detailed Explanation

### Regular Comments

```rust
// This is a single-line comment

// You can chain them
// to create multiple lines
// of comments

/* This is a multi-line comment */

/*
   Multi-line comments
   can span multiple lines
   and can /* nest */ too!
*/

fn main() {
    let x = 5; // Inline comment
    // let y = 10; // Commented-out code
}
```

**Same as TypeScript/JavaScript!**

### Documentation Comments

**Outer doc comments** (document the item that follows):

````rust
/// Adds two numbers together
///
/// # Examples
///
/// ```
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// ```
fn add(a: i32, b: i32) -> i32 {
    a + b
}
````

**Inner doc comments** (document the enclosing item):

```rust
//! This module provides math utilities
//!
//! It includes functions for basic arithmetic operations.

fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

**Generate HTML docs:**

```bash
cargo doc --open
```

This creates beautiful HTML documentation automatically!

### Doc Comment Structure

**Common sections:**

````rust
/// Brief description (one line)
///
/// More detailed explanation can go here
/// across multiple lines.
///
/// # Arguments
/// * `x` - The first number
/// * `y` - The second number
///
/// # Returns
/// The sum of x and y
///
/// # Examples
/// ```
/// let result = add(5, 3);
/// assert_eq!(result, 8);
/// ```
///
/// # Panics
/// This function panics if...
///
/// # Errors
/// Returns an error if...
///
/// # Safety
/// This function is unsafe because...
fn add(x: i32, y: i32) -> i32 {
    x + y
}
````

**Markdown supported:**

````rust
/// This function uses **bold** and *italic*
///
/// # Examples
///
/// ```
/// let x = my_func();
/// ```
///
/// See also: [other_func]
fn my_func() -> i32 {
    42
}
````

### Code Examples in Docs

````rust
/// Multiplies two numbers
///
/// # Examples
///
/// ```
/// let result = multiply(2, 3);
/// assert_eq!(result, 6);
/// ```
///
/// This example shows error handling:
/// ```
/// let result = checked_multiply(i32::MAX, 2);
/// assert!(result.is_none());
/// ```
fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
````

**Examples are automatically tested!**

```bash
cargo test --doc
```

This runs all code examples in your documentation.

> **Note:** Each example is compiled and run as its own small crate that depends on yours, so anything it references (your functions, types, etc.) must be `pub` and brought into scope with `use`. To keep that setup out of the rendered docs, prefix the line with `#` (see Pitfall 3 below).

---

## Key Differences from TypeScript/JavaScript

### 1. Built-in Documentation

**TypeScript:**

```typescript
/**
 * JSDoc comment (convention, not part of TS)
 * @param x - Number
 * @returns Result
 */
function double(x: number): number {
  return x * 2;
}

// Need external tool (TypeDoc) to generate docs
```

**Rust:**

```rust
/// Documentation comment (built into Rust!)
///
/// # Arguments
/// * `x` - Number
///
/// # Returns
/// Result
fn double(x: i32) -> i32 {
    x * 2
}

// cargo doc generates HTML automatically
```

### 2. Testable Examples

**TypeScript:**

````typescript
/**
 * @example
 * ```typescript
 * const result = add(2, 3);
 * // Examples are not automatically tested
 * ```
 */
````

**Rust:**

````rust
/// # Examples
/// ```
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// // This example is automatically tested!
/// ```
````

### 3. Nested Comments

**JavaScript:**

```javascript
/*
   /* This won't work! */
   Can't nest multi-line comments
*/
```

**Rust:**

```rust
/*
   /* This works! */
   Rust supports nested comments
*/
```

---

## Common Pitfalls

### Pitfall 1: Using JSDoc Style

**Problem:**

```rust
/**
 * @param x - A number
 * @returns The doubled value
 */
fn double(x: i32) -> i32 {
    x * 2
}
```

**Why:** This works, but isn't idiomatic Rust.

**Solution:**

```rust
/// Doubles a number
///
/// # Arguments
/// * `x` - A number
///
/// # Returns
/// The doubled value
fn double(x: i32) -> i32 {
    x * 2
}
```

### Pitfall 2: Forgetting Outer Doc Comment After Inner

**Problem:**

```rust
//! Module documentation

fn my_func() {  // No doc comment!
    // ...
}
```

**Solution:**

```rust
//! Module documentation

/// Function documentation
fn my_func() {
    // ...
}
```

### Pitfall 3: Items Not in Scope in Doc Examples

**Problem:**

````rust
/// # Examples
/// ```
/// let result = add(2, 3); // Will fail doc test!
/// assert_eq!(result, 5);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
````

**Why:** A doc test is compiled as a **separate crate** that depends on yours, so your item is not automatically in scope. The example above fails with `error[E0425]: cannot find function add in this scope` and a help line suggesting `use your_crate::add;`. (Note: a missing `main` is _not_ the cause — rustdoc automatically wraps each example in a `main` function for you.)

**Solution:**

````rust
/// # Examples
/// ```
/// # use your_crate::add;
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
````

Use a `#`-prefixed line to hide setup (such as `use` imports) from the rendered docs while still compiling and running it in the test.

---

## Best Practices

### 1. Use Doc Comments for Public API

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Don't document everything:**

```rust
// Helper function
fn internal_helper(x: i32) -> i32 {
    x * 2
}

/// Public API should be documented
pub fn public_api(x: i32) -> i32 {
    internal_helper(x)
}
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Document public items:**

````rust
// Internal, no doc comment needed
fn internal_helper(x: i32) -> i32 {
    x * 2
}

/// Doubles the input number
///
/// # Examples
/// ```
/// let result = public_api(5);
/// assert_eq!(result, 10);
/// ```
pub fn public_api(x: i32) -> i32 {
    internal_helper(x)
}
````

### 2. Include Examples

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Just describe:**

```rust
/// Adds two numbers
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Show how to use:**

````rust
/// Adds two numbers
///
/// # Examples
/// ```
/// let sum = add(2, 3);
/// assert_eq!(sum, 5);
/// ```
fn add(a: i32, b: i32) -> i32 {
    a + b
}
````

### 3. Document Panics and Errors

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Don't hide gotchas:**

```rust
/// Divides two numbers
fn divide(a: i32, b: i32) -> i32 {
    a / b  // Panics if b is 0!
}
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Document behavior:**

````rust
/// Divides two numbers
///
/// # Panics
/// Panics if `b` is zero
///
/// # Examples
/// ```
/// let result = divide(10, 2);
/// assert_eq!(result, 5);
/// ```
fn divide(a: i32, b: i32) -> i32 {
    a / b
}
````

### 4. Use Markdown for Formatting

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Plain text:**

```rust
/// Returns the user's age.
/// Arguments:
/// - name: the user's name
/// - id: the user's ID
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Use Markdown:**

```rust
/// Returns the user's age
///
/// # Arguments
/// * `name` - The user's name
/// * `id` - The user's ID
///
/// # Returns
/// The user's age as `u32`
```

---

## Real-World Example

### Documented Math Module

**TypeScript:**

```typescript
/**
 * Math utilities module
 */

/**
 * Calculates the factorial of a number
 * @param n - The number to calculate factorial for
 * @returns The factorial of n
 * @throws Error if n is negative
 */
export function factorial(n: number): number {
  if (n < 0) throw new Error("Negative number");
  if (n === 0) return 1;
  return n * factorial(n - 1);
}

/**
 * Checks if a number is prime
 * @param n - The number to check
 * @returns true if prime, false otherwise
 */
export function isPrime(n: number): boolean {
  if (n < 2) return false;
  for (let i = 2; i <= Math.sqrt(n); i++) {
    if (n % i === 0) return false;
  }
  return true;
}
```

**Rust:**

````rust
//! Math utilities module
//!
//! This module provides common mathematical functions.

/// Calculates the factorial of a number
///
/// # Arguments
/// * `n` - The number to calculate factorial for
///
/// # Returns
/// The factorial of n
///
/// # Panics
/// Panics if `n` is negative
///
/// # Examples
/// ```
/// # use math::factorial;
/// let result = factorial(5);
/// assert_eq!(result, 120);
/// ```
pub fn factorial(n: i32) -> i32 {
    assert!(n >= 0, "factorial is undefined for negative numbers");
    if n == 0 {
        1
    } else {
        n * factorial(n - 1)
    }
}

/// Checks if a number is prime
///
/// # Arguments
/// * `n` - The number to check
///
/// # Returns
/// `true` if prime, `false` otherwise
///
/// # Examples
/// ```
/// # use math::is_prime;
/// assert!(is_prime(7));
/// assert!(!is_prime(8));
/// ```
pub fn is_prime(n: u32) -> bool {
    if n < 2 {
        return false;
    }
    // Use integer arithmetic (`i * i <= n`) for the loop bound. Converting
    // to `f64` and calling `.sqrt()` works for small values but is a fragile
    // idiom: float rounding can make the bound off by one for large inputs.
    let mut i = 2;
    while i * i <= n {
        if n % i == 0 {
            return false;
        }
        i += 1;
    }
    true
}
````

> **Note:** The TypeScript `factorial` above signals invalid input with `throw new Error(...)`, which is a _recoverable_ error a caller can catch. The Rust version uses `assert!`/`panic!` to keep the comparison short, but in idiomatic Rust a recoverable failure like this is normally modeled with a return type of `Result<T, E>` (covered in the error-handling section) rather than a panic. Reserve `panic!` for genuinely unrecoverable bugs.

**Generate docs:**

```bash
cargo doc --open
```

Beautiful HTML documentation is generated automatically!

---

## Doc Comment Sections

Common sections in Rust doc comments:

### Standard Sections

```rust
/// Brief one-line description
///
/// # Arguments
/// Parameter documentation
///
/// # Returns
/// Return value documentation
///
/// # Errors
/// Error conditions (for Result)
///
/// # Panics
/// Panic conditions
///
/// # Safety
/// Safety requirements (for unsafe)
///
/// # Examples
/// Usage examples
///
/// # See Also
/// Related functions
```

### Custom Sections

You can create custom sections:

```rust
/// My function
///
/// # Performance
/// This function runs in O(n) time
///
/// # Thread Safety
/// This function is thread-safe
fn my_func() {
    // ...
}
```

---

## Further Reading

### Official Documentation

- [The Rust Book - Comments](https://doc.rust-lang.org/book/ch03-04-comments.html)
- [Rust Reference - Comments](https://doc.rust-lang.org/reference/comments.html)
- [rustdoc Book](https://doc.rust-lang.org/rustdoc/)

### Related Topics

- [Documentation Guidelines](https://rust-lang.github.io/api-guidelines/documentation.html)

---

## Exercises

### Exercise 1: Add Documentation

Add doc comments to this function:

```rust
fn square(x: i32) -> i32 {
    x * x
}
```

<details>
<summary>Solution</summary>

````rust
/// Calculates the square of a number
///
/// # Arguments
/// * `x` - The number to square
///
/// # Returns
/// The square of `x`
///
/// # Examples
/// ```
/// let result = square(5);
/// assert_eq!(result, 25);
/// ```
fn square(x: i32) -> i32 {
    x * x
}
````

</details>

### Exercise 2: Document Panic Condition

Document the panic condition:

```rust
fn divide(a: i32, b: i32) -> i32 {
    if b == 0 {
        panic!("Division by zero!");
    }
    a / b
}
```

<details>
<summary>Solution</summary>

````rust
/// Divides two numbers
///
/// # Arguments
/// * `a` - The dividend
/// * `b` - The divisor
///
/// # Returns
/// The result of a / b
///
/// # Panics
/// Panics if `b` is zero
///
/// # Examples
/// ```
/// let result = divide(10, 2);
/// assert_eq!(result, 5);
/// ```
fn divide(a: i32, b: i32) -> i32 {
    if b == 0 {
        panic!("Division by zero!");
    }
    a / b
}
````

</details>

### Exercise 3: Module Documentation

Add module-level documentation:

```rust
// math.rs

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
```

<details>
<summary>Solution</summary>

````rust
//! Math utilities module
//!
//! This module provides basic arithmetic operations.
//!
//! # Examples
//! ```
//! let sum = math::add(2, 3);
//! assert_eq!(sum, 5);
//! ```

/// Adds two numbers
///
/// # Examples
/// ```
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Subtracts two numbers
///
/// # Examples
/// ```
/// let result = subtract(5, 3);
/// assert_eq!(result, 2);
/// ```
pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
````

</details>

---

## Summary

**What you've learned:**

- Regular comments (`//`, `/* */`)
- Doc comments (`///`, `//!`)
- Doc comment structure (Arguments, Returns, Examples, etc.)
- Markdown in doc comments
- Testable code examples
- Generating HTML documentation

**Key differences from TypeScript:**

| Feature          | TypeScript         | Rust                    |
| ---------------- | ------------------ | ----------------------- |
| Regular comments | `//`, `/* */`      | Same                    |
| Doc comments     | JSDoc (convention) | `///`, `//!` (built-in) |
| Doc generation   | TypeDoc (external) | `cargo doc` (built-in)  |
| Example testing  | Manual             | Automatic               |
| Nested comments | <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> No | <span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Yes |

**Doc comments are a first-class feature in Rust!**
