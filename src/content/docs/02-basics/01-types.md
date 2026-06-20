---
title: "Basic Types"
description: "Rust replaces TypeScript's one number type with sized ints, floats, bool, char, and tuples, plus explicit casts and overflow checks, not silent coercion."
---

Rust has a rich type system with explicit types for different sizes and purposes. Unlike TypeScript's single `number` type, Rust has many numeric types optimized for different use cases.

---

## Quick Overview

Rust's type system is:

- **Explicit:** Different types for integers, floats, etc.
- **Safe:** No implicit conversions that lose data
- **Efficient:** Choose the right size for your needs

**Key types:** Integers (`i32`, `u8`...), floats (`f32`, `f64`), booleans (`bool`), characters (`char`), tuples

---

## TypeScript/JavaScript Example

```typescript
// TypeScript - Simple type system
let integer: number = 42;
let float: number = 3.14;
let negative: number = -10;
let big: number = 9007199254740991; // Max safe integer

let flag: boolean = true;
let letter: string = "a"; // No char type
let tuple: [number, string] = [1, "hello"];

// All numbers are the same type
let x: number = 42;
let y: number = 3.14;
console.log(typeof x); // "number"
console.log(typeof y); // "number"
```

**Key points:**

- One `number` type for all numbers
- No distinction between integers and floats
- No control over size/memory usage
- Strings for single characters

---

## Rust Equivalent

```rust
// Rust - Rich type system
let integer: i32 = 42;          // 32-bit signed integer
let float: f64 = 3.14;          // 64-bit float
let negative: i32 = -10;        // Signed integer
let small: u8 = 255;            // 8-bit unsigned (0-255)
let big: i64 = 9_223_372_036_854_775_807; // 64-bit signed

let flag: bool = true;          // Boolean
let letter: char = 'a';         // Unicode scalar value (4 bytes!)
let tuple: (i32, &str) = (1, "hello"); // Tuple

// Different types for different purposes
let x: i32 = 42;    // Integer type
let y: f64 = 3.14;  // Float type
// let z = x + y;   // Error: can't add i32 and f64!
let z = x as f64 + y; // Explicit conversion
```

**Key points:**

- Multiple integer types (by size and signedness)
- Separate float types (f32, f64)
- Explicit conversions required
- True character type (Unicode)

---

## Detailed Explanation

### Integer Types

Rust has **12** integer types based on:

1. **Signedness:** Signed (`i`) or unsigned (`u`)
2. **Size:** 8, 16, 32, 64, 128 bits, or architecture-dependent

**Signed integers (can be negative):**

| Type    | Size     | Range                             |
| ------- | -------- | --------------------------------- |
| `i8`    | 8 bits   | -128 to 127                       |
| `i16`   | 16 bits  | -32,768 to 32,767                 |
| `i32`   | 32 bits  | -2,147,483,648 to 2,147,483,647   |
| `i64`   | 64 bits  | -9,223,372,036,854,775,808 to ... |
| `i128`  | 128 bits | Very large range                  |
| `isize` | arch     | Depends on CPU (32 or 64 bits)    |

**Unsigned integers (only positive):**

| Type    | Size     | Range                      |
| ------- | -------- | -------------------------- |
| `u8`    | 8 bits   | 0 to 255                   |
| `u16`   | 16 bits  | 0 to 65,535                |
| `u32`   | 32 bits  | 0 to 4,294,967,295         |
| `u64`   | 64 bits  | 0 to 18,446,744,073,709... |
| `u128`  | 128 bits | Very large range           |
| `usize` | arch     | Depends on CPU             |

**Default:** When you write `let x = 42;`, Rust infers `i32`.

**Examples:**

```rust
// Signed integers
let a: i8 = -128;       // Smallest i8
let b: i8 = 127;        // Largest i8
let c: i32 = -2_000_000; // Default integer type

// Unsigned integers
let d: u8 = 255;        // Largest u8
let e: u16 = 65_535;    // Largest u16
let f: u32 = 4_000_000; // Common for large counts

// Architecture-dependent
let idx: usize = 10;    // Used for array indices, sizes
let offset: isize = -5; // Used for pointer arithmetic
```

**Number literals:**

```rust
let decimal = 98_222;      // Underscore for readability
let hex = 0xff;            // Hexadecimal
let octal = 0o77;          // Octal
let binary = 0b1111_0000;  // Binary
let byte = b'A';           // u8 only (ASCII)

// Type suffix
let x = 42i32;     // Explicitly i32
let y = 100_u8;    // Explicitly u8
let z = 3.14f32;   // Explicitly f32
```

### Floating-Point Types

Rust has two floating-point types:

| Type  | Size    | Precision    | Default |
| ----- | ------- | ------------ | ------- |
| `f32` | 32 bits | ~7 decimals  | No      |
| `f64` | 64 bits | ~15 decimals | Yes     |

**Examples:**

```rust
let x = 2.0;        // f64 (default)
let y: f32 = 3.0;   // f32 (explicit)

let pi: f64 = 3.14159265359;
let e: f32 = 2.71828;

// Scientific notation
let large = 1e10;   // f64; prints as 10000000000 (whole-valued floats omit the .0)
let small = 1e-5;   // f64; prints as 0.00001
```

**When to use f32 vs f64:**

- **f32:** Graphics, game engines (GPU prefers f32)
- **f64:** Scientific computing, default (more precise)

**Compare to TypeScript:**

```typescript
// TypeScript - one type
let x: number = 3.14; // Always IEEE-754 f64 (Rust's f64)
let y: number = 2.0; // Same type; JS has no f32
```

### Boolean Type

Simple true/false:

```rust
let t = true;           // Inferred as bool
let f: bool = false;    // Explicit type

// Common use in conditionals
let is_active = true;
if is_active {
    println!("Active!");
}

// Size: 1 byte (8 bits)
```

**Same as TypeScript:**

```typescript
let t: boolean = true;
let f: boolean = false;
```

### Character Type

Rust's `char` is a full **Unicode Scalar Value**, covering far more than ASCII!

```rust
let c = 'z';                // Inferred as char
let z: char = 'ℤ';          // Unicode
let heart = '\u{2764}';     // Emoji (a Unicode scalar)
let chinese = '中';         // Chinese character

// Size: 4 bytes (32 bits)
// Range: U+0000 to U+D7FF and U+E000 to U+10FFFF
```

**Compare to TypeScript:**

```typescript
// TypeScript - no char type, use string
let c: string = "z";
let emoji: string = "\u{2764}";
```

**Important:** In Rust, `'a'` is a char, `"a"` is a string!

```rust
let char_a = 'a';    // char type
let string_a = "a";  // &str type (string slice)
```

### Tuple Type

Group multiple values of different types:

```rust
let tup: (i32, f64, u8) = (500, 6.4, 1);

// Destructuring
let (x, y, z) = tup;
println!("x: {}, y: {}, z: {}", x, y, z);

// Access by index
let five_hundred = tup.0;
let six_point_four = tup.1;
let one = tup.2;

// Empty tuple (unit type)
let unit: () = ();
```

**Compare to TypeScript:**

```typescript
// TypeScript tuples
let tup: [number, number, number] = [500, 6.4, 1];

// Destructuring
let [x, y, z] = tup;

// Access by index
let five_hundred = tup[0];
```

**Tuple use cases:**

- Return multiple values from functions
- Group related values temporarily
- Pattern matching (we'll see later)

**Unit type `()`:**

```rust
fn do_something() {
    // No return value means returns ()
}

fn explicit_unit() -> () {
    println!("Returns unit");
}
```

This looks like TypeScript's `void`, but the two differ. TypeScript's `void` is a type used to say "ignore whatever this returns"; there is no `void` *value* you can hold. Rust's `()` (the **unit type**) is a real type with exactly one value, also written `()`. A function with no `-> ...` returns `()`, and you can bind it: `let nothing: () = do_something();` is valid Rust.

---

## Key Differences from TypeScript

### 1. Multiple Number Types

**TypeScript:**

```typescript
let x: number = 42;
let y: number = 3.14;
let z: number = x + y; // OK
```

**Rust:**

```rust
let x: i32 = 42;
let y: f64 = 3.14;
// let z = x + y;     // Error: mismatched types
let z = x as f64 + y; // Explicit conversion
```

### 2. Overflow Behavior

**TypeScript:**

```typescript
let x: number = 255;
x = x + 1; // 256 (no problem)
```

**Rust (debug mode):**

```rust
let mut x: u8 = 255;
let one = std::env::args().count() as u8; // 1 at runtime, not a constant
x = x + one; // Panics at runtime: "attempt to add with overflow"
```

**Rust (release mode):**

```rust
let mut x: u8 = 255;
let one = std::env::args().count() as u8; // 1 at runtime, not a constant
x = x + one; // Wraps to 0 (two's complement)
```

> **Note:** Both operands above are computed at runtime on purpose. If you instead write `let x: u8 = 255; x + 1` with two literals, the compiler folds the constants and rejects it outright with `error: this arithmetic operation will overflow` — it never even gets to run.

> **Warning:** This release-mode wrap is a *documented logic error*, not a feature to lean on. Relying on the silent wrap is discouraged; if you genuinely want wrapping, say so explicitly with `wrapping_add` so the behavior is the same in debug and release:
>
> ```rust
> let x: u8 = 255;
> let y = x.wrapping_add(1); // 0, intentional and identical in debug + release
> ```

**Explicit overflow handling:**

```rust
let x: u8 = 255;

// Wrapping (always wraps)
let y = x.wrapping_add(1);  // 0

// Saturating (clamps to max)
let y = x.saturating_add(1); // 255

// Checked (returns Option)
let y = x.checked_add(1);    // None

// Overflowing (returns tuple)
let (y, overflowed) = x.overflowing_add(1); // (0, true)
```

### 3. No Implicit Conversions

**TypeScript:**

```typescript
let x: number = 42;
let y: string = String(x); // Explicit, but common
let z = x + ""; // Implicit conversion
```

**Rust:**

```rust
let x: i32 = 42;
// let y: String = x; // Error: can't convert
let y: String = x.to_string(); // Explicit

// Type casting with 'as'
let a: i32 = 42;
let b: f64 = a as f64;
let c: u8 = 255;
let d: i32 = c as i32;
```

### 4. Character vs String

**TypeScript:**

```typescript
let c = "a"; // string
let s = "hello"; // string (same type)
```

**Rust:**

```rust
let c = 'a';     // char (4 bytes)
let s = "hello"; // &str (string slice)
// Different types!
```

---

## Common Pitfalls

### Pitfall 1: Integer Overflow

**Problem:**

```rust
fn add_one(x: u8) -> u8 {
    x + 1 // Panics in debug, wraps in release when x is 255
}

fn main() {
    let y = add_one(255);
    println!("{y}");
}
```

> **Note:** Here `x` arrives as a function argument, so the overflow is only discovered at runtime: debug builds panic with `attempt to add with overflow`, release builds silently wrap to `0`. Writing the same overflow with two literals (`let x: u8 = 255; x + 1`) is caught at compile time instead. The compiler emits `error: this arithmetic operation will overflow`.

**Solution:**

```rust playground
fn main() {
    let x: u8 = 255;

    // Choose appropriate method
    let y = x.saturating_add(1);  // 255 (clamps)
    let y = x.wrapping_add(1);     // 0 (wraps)

    // Or use larger type
    let x: u16 = 255;
    let y = x + 1; // 256
}
```

### Pitfall 2: Mixing Integer Types

**Problem:**

```rust
let x: i32 = 10;
let y: i64 = 20;
// let z = x + y; // Error: mismatched types
```

**Solution:**

```rust
let x: i32 = 10;
let y: i64 = 20;
let z = (x as i64) + y; // Convert x to i64
```

### Pitfall 3: Division Truncation

**Problem:**

```rust
let x = 5 / 2; // Result is 2, not 2.5!
```

**Why:** Integer division truncates.

**Solution:**

```rust
let x = 5.0 / 2.0;      // 2.5 (float division)
let y = 5 as f64 / 2.0; // 2.5 (convert to float)
```

### Pitfall 4: Char vs String Confusion

**Problem:**

```rust
let c: char = "a"; // Error: expected char, found &str
```

**Solution:**

```rust
let c: char = 'a';    // Single quotes for char
let s: &str = "a";    // Double quotes for string
```

---

## Best Practices

### 1. Default to i32, Then Specialize

The Rust Book's advice: **start with `i32`** as your default integer. It is fast on modern CPUs and rarely the wrong choice. Reach for a different type only when you have a concrete reason — a domain bound, a memory budget, or an API that demands it.

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Default unless you have a reason:**

```rust
let count = 10;         // i32 by inference — the sensible default
let id: u32 = 123456;   // u32 because the ID is never negative
let size: usize = 1000; // usize because it indexes a collection
```

**Specialize when the domain or API calls for it:**

```rust
let pixel: u8 = 255;    // a color channel really is 0-255
let port: u16 = 8080;   // ports are 0-65535
let idx: usize = 10;    // indexing/length is always usize
```

> **Tip:** Choosing the *smallest* type that fits is not automatically better. Picking `u8` for an age, for example, buys almost nothing and just invites overflow bugs as soon as you add to it. Optimize the size only where it matters (large arrays, packed structs, fixed protocol fields).

**Rough guidance on when to deviate from `i32`:**

- `u8`: bytes, color channels, raw ASCII
- `u16`: ports, values bounded above (e.g. HTTP status 100-599)
- `i64`/`u64`: timestamps, file sizes, values that outgrow 32 bits
- `usize`/`isize`: collection indices, lengths, sizes — required by the standard library

### 2. Use Type Inference When Clear

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Don't over-annotate:**

```rust
let x: i32 = 42;
let y: i32 = 10;
let z: i32 = x + y;
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Let Rust infer:**

```rust
let x = 42;        // Inferred as i32
let y = 10;        // Inferred as i32
let z = x + y;     // Inferred as i32
```

**Annotate when needed:**

```rust
let x: u8 = 42;    // Need u8, not i32
let y = Vec::new(); // Need to specify: Vec<i32>
```

### 3. Use Suffixes for Clarity

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Ambiguous:**

```rust
let x = 42;        // Is this i32, u32, i64?
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Clear intent:**

```rust
let x = 42u8;      // Explicitly u8
let y = 1_000_000u32; // Explicitly u32
let z = 3.14f32;   // Explicitly f32
```

### 4. Handle Overflow Explicitly

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Hope for the best:**

```rust
let x: u8 = user_input + 10; // What if it overflows?
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Handle explicitly:**

```rust
let x: u8 = user_input.saturating_add(10); // Clamps to 255
// or
let x = user_input.checked_add(10)
    .expect("Overflow!");
```

---

## Real-World Example

### Type-Safe Configuration

**TypeScript:**

```typescript
interface Config {
  port: number; // Could be negative!
  maxConnections: number;
  timeout: number; // Milliseconds
  name: string;
}

const config: Config = {
  port: 8080,
  maxConnections: 100,
  timeout: 5000,
  name: "MyServer",
};
```

**Rust:**

```rust
struct Config {
    port: u16,           // 0-65535 (valid port range)
    max_connections: u32, // Only positive
    timeout_ms: u64,     // Milliseconds, large range
    name: String,
}

let config = Config {
    port: 8080,
    max_connections: 100,
    timeout_ms: 5000,
    name: String::from("MyServer"),
};

// Can't set invalid port
// let bad = Config { port: -1, ... };  // Won't compile!
// let bad = Config { port: 70000, ... }; // Won't compile!
```

**The type system enforces validity!**

### Pixel Color Representation

```rust
// RGB color (0-255 per channel)
struct Color {
    r: u8, // Red: 0-255
    g: u8, // Green: 0-255
    b: u8, // Blue: 0-255
}

let red = Color { r: 255, g: 0, b: 0 };
let white = Color { r: 255, g: 255, b: 255 };

// Can't set invalid values
// let bad = Color { r: 256, g: 0, b: 0 }; // Won't compile!
```

**u8 is perfect for this!** Saves memory and enforces range.

---

## Further Reading

### Official Documentation

- [The Rust Book - Data Types](https://doc.rust-lang.org/book/ch03-02-data-types.html)
- [Rust Reference - Types](https://doc.rust-lang.org/reference/types.html)
- [Rust by Example - Primitives](https://doc.rust-lang.org/rust-by-example/primitives.html)

### Related Topics

- [Type Conversions](https://doc.rust-lang.org/rust-by-example/types/cast.html)
- [Overflow Handling](https://doc.rust-lang.org/std/primitive.u8.html#method.wrapping_add)

---

## Exercises

### Exercise 1: Choose the Right Type

For each value, choose the most appropriate type:

```rust
// 1. Person's age
let age: /* what type? */ = 25;

// 2. HTTP status code (100-599)
let status: /* what type? */ = 200;

// 3. Temperature in Celsius (-273.15 to ∞)
let temp: /* what type? */ = -5.5;

// 4. Array index
let idx: /* what type? */ = 10;

// 5. Unicode emoji
let emoji: /* what type? */ = '\u{1F600}';
```

<details>
<summary>Solutions</summary>

```rust
let age: u8 = 25;           // 0-255 is enough
let status: u16 = 200;      // 100-599 fits in u16
let temp: f32 = -5.5;       // Float for decimals
let idx: usize = 10;        // usize for indices
let emoji: char = '\u{1F600}'; // char for Unicode
```

</details>

### Exercise 2: Fix Type Errors

Fix this code:

```rust
fn main() {
    let x: i32 = 10;
    let y: f64 = 3.14;
    let z = x + y;
    println!("Result: {}", z);
}
```

<details>
<summary>Solution</summary>

```rust playground
fn main() {
    let x: i32 = 10;
    let y: f64 = 3.14;
    let z = x as f64 + y; // Convert x to f64
    println!("Result: {}", z);
}
```

</details>

### Exercise 3: Tuple Destructuring

Create a tuple with (name, age, score) and destructure it:

```rust
fn main() {
    // Create tuple
    let student = /* create tuple */;

    // Destructure
    let (/* fill in */) = student;

    println!("Name: {}, Age: {}, Score: {}", name, age, score);
}
```

<details>
<summary>Solution</summary>

```rust playground
fn main() {
    let student = ("Alice", 20, 95.5);
    let (name, age, score) = student;
    println!("Name: {}, Age: {}, Score: {}", name, age, score);
}
```

</details>

### Exercise 4: Safe Arithmetic

Implement safe addition that doesn't panic:

```rust
fn safe_add(a: u8, b: u8) -> u8 {
    // Use saturating or checked addition
}

fn main() {
    println!("{}", safe_add(200, 100)); // Should be 255
    println!("{}", safe_add(10, 20));   // Should be 30
}
```

<details>
<summary>Solution</summary>

```rust playground
fn safe_add(a: u8, b: u8) -> u8 {
    a.saturating_add(b) // Clamps to 255
}

fn main() {
    println!("{}", safe_add(200, 100)); // 255
    println!("{}", safe_add(10, 20));   // 30
}
```

</details>

---

## Summary

**What you've learned:**

- Rust has many integer types (i8, i32, u64, etc.)
- Two float types (f32, f64)
- Boolean and character types
- Tuples for grouping values
- No implicit conversions (use `as`)
- Explicit overflow handling

**Key types:**

```rust
// Integers
i8, i16, i32, i64, i128, isize  // Signed
u8, u16, u32, u64, u128, usize  // Unsigned

// Floats
f32, f64

// Others
bool    // true/false
char    // Unicode scalar value
()      // Unit type
(T, U)  // Tuple
```

**Type conversion:**

```rust
let x: i32 = 42;
let y: f64 = x as f64;  // Explicit cast
let z: String = x.to_string(); // Method call
```

**Choose types wisely for memory efficiency and correctness!**
