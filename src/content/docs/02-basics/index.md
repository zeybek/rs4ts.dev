---
title: "Basics"
sidebar:
  label: "Overview"
description: "Learn the fundamental building blocks of Rust: variables, types, operators, and basic syntax."
---

Learn the fundamental building blocks of Rust: variables, types, operators, and basic syntax.

---

## What's in This Section

- **[Variables and Mutability](/02-basics/00-variables/)** - let, mut, shadowing, constants
- **[Basic Types](/02-basics/01-types/)** - Integers, floats, booleans, characters, tuples
- **[Operators](/02-basics/02-operators/)** - Arithmetic, comparison, logical, and more
- **[Comments](/02-basics/03-comments/)** - Code comments and documentation
- **[Output and Formatting](/02-basics/04-output/)** - println!, print!, formatting

---

## Learning Objectives

By the end of this section, you will:

- Understand Rust's immutability-by-default philosophy
- Know all basic data types and when to use them
- Use type annotations and type inference
- Perform arithmetic and logical operations
- Write clear comments and documentation
- Format output with println! macro

---

## Time Estimate

- **Reading:** 60-90 minutes
- **Hands-on Practice:** 45-60 minutes
- **Exercises:** 30-45 minutes
- **Total:** 2.5-3 hours

---

## Quick Start Path

If you want to jump right in:

1. **[Variables](/02-basics/00-variables/)** (20 min) - Most important!
2. **[Types](/02-basics/01-types/)** (20 min)
3. **[Output](/02-basics/04-output/)** (15 min)
4. Skip to exercises

Then come back for operators and comments.

---

## Detailed Learning Path

### Recommended Order

```
1. Variables and Mutability (the big difference!)
   ↓
2. Basic Types (integers, floats, etc.)
   ↓
3. Operators (arithmetic, comparison)
   ↓
4. Comments (documentation)
   ↓
5. Output and Formatting (println!)
```

All topics are interconnected, sequential reading recommended.

---

## What You'll Learn

### Variables: Immutable by Default

The **biggest** difference from TypeScript/JavaScript:

**TypeScript:**

```typescript
let x = 5; // mutable
x = 6; // OK
```

**Rust:**

```rust
let x = 5;      // immutable by default!
x = 6;          // Compile error!

let mut y = 5;  // explicitly mutable
y = 6;          // OK
```

**Key insight:** Rust makes you think about mutability upfront.

### Rich Type System

**TypeScript:**

```typescript
let x: number = 42; // One number type
let y = 3.14; // Also number
```

**Rust:**

```rust
let x: i32 = 42;    // 32-bit signed integer
let y: f64 = 3.14;  // 64-bit float
let z: u8 = 255;    // 8-bit unsigned integer
```

**Why:** More control over memory and performance.

### Type Inference

**Both languages have it:**

```typescript
let x = 5; // TypeScript infers: number
```

```rust
let x = 5; // Rust infers: i32
```

But Rust's inference goes further in complex scenarios!

### Operators

**Mostly the same:**

```typescript
// TypeScript
let sum = 5 + 3;
let is_equal = x === y;
```

```rust
// Rust
let sum = 5 + 3;
let is_equal = x == y;  // Note: no ===, just ==
```

### Comments

**Similar syntax:**

```typescript
// TypeScript
// Single line comment
/* Multi-line comment */

/** JSDoc documentation */
```

```rust
// Rust
// Single line comment
/* Multi-line comment */

/// Documentation comment (generates docs!)
```

### Formatted Output

**Different, but does more:**

```typescript
// TypeScript
console.log(`Hello, ${name}!`);
console.log("x =", x, "y =", y);
```

```rust
// Rust
println!("Hello, {}!", name);
println!("x = {} y = {}", x, y);
```

---

## Key Differences from TypeScript

### 1. Mutability

| Aspect         | TypeScript      | Rust              |
| -------------- | --------------- | ----------------- |
| Default        | Mutable (`let`) | Immutable (`let`) |
| Make immutable | Use `const`     | Default!          |
| Make mutable   | Default!        | Use `let mut`     |
| Reassignment   | Always allowed  | Only with `mut`   |

### 2. Type System

| Feature          | TypeScript     | Rust                  |
| ---------------- | -------------- | --------------------- |
| Number types     | `number`       | `i8`, `i32`, `f64`... |
| Integer types    | `number`       | 12 different types!   |
| Type annotations | `: Type`       | `: Type` (same!)      |
| Type inference   | Good           | Excellent             |
| Null safety      | `strict` mode  | No `null` at all (`Option<T>`) |
| Escape hatches   | `any` (opt-out)| `unsafe` / `dyn Any` (rare)    |

### 3. Variables

**TypeScript:**

```typescript
let x = 5;
let x = 10; // Error: cannot redeclare
```

**Rust:**

```rust
let x = 5;
let x = 10; // OK - this is "shadowing"!
```

**Shadowing** is a Rust feature that lets you reuse variable names.

### 4. Constants

**TypeScript:**

```typescript
const MAX_SIZE = 100; // Runtime constant
```

**Rust:**

```rust
const MAX_SIZE: u32 = 100;  // Compile-time constant
//    ^^^ Must have type annotation!
```

---

## Important Concepts

### Immutability by Default

**Why does Rust do this?**

1. **Safety:** Prevents accidental modification
2. **Concurrency:** Immutable data is thread-safe
3. **Optimization:** Compiler can optimize better
4. **Intent:** Makes mutability explicit

**Coming from JavaScript, this feels backwards at first!**

```javascript
// JavaScript - everything is mutable
let x = 5;
x = 6; // No problem
```

```rust
// Rust - explicit mutability
let x = 5;
// x = 6;        // Won't compile

let mut y = 5;
y = 6;           // OK
```

**You'll get used to it!** After a week, you'll appreciate it.

### Type Annotations vs Inference

**Rust is smart about types:**

```rust
let x = 5;              // Inferred as i32
let y: u8 = 5;          // Explicitly u8
let z = 5u8;            // Suffix notation
```

**When you need annotations:**

```rust
// Compiler can't infer
let numbers: Vec<i32> = Vec::new();

// Multiple possible types
let guess: u32 = "42".parse().expect("Not a number!");
```

### Expressions vs Statements

**Everything in Rust is an expression (almost):**

```rust
let x = {
    let y = 3;
    y + 1  // No semicolon - this is the return value!
};
// x is now 4
```

**Compare to TypeScript:**

```typescript
// TypeScript - blocks don't return values
let x = (() => {
  let y = 3;
  return y + 1;
})();
```

---

## Your Goals for This Section

### Minimum Goals

- [ ] Understand `let` vs `let mut`
- [ ] Know the basic integer types (`i32`, `u32`, etc.)
- [ ] Use `println!` with formatting
- [ ] Write simple arithmetic expressions

### Stretch Goals

- [ ] Understand shadowing vs reassignment
- [ ] Know when to use each integer type
- [ ] Use all comparison operators
- [ ] Write documentation comments
- [ ] Use advanced formatting (debug, alignment)

---

## Common First-Time Issues

### "cannot assign twice to immutable variable"

```rust
let x = 5;
x = 6;  // Error!
```

**Solution:** Add `mut`:

```rust
let mut x = 5;
x = 6;  // OK
```

### "type annotations needed"

```rust
let x = Vec::new();  // What type?
```

**Solution:** Add type annotation:

```rust
let x: Vec<i32> = Vec::new();  // OK
```

### Integer overflow

```rust
let x: u8 = 255;
let y = x + 1;  // Compile error: this arithmetic operation will overflow
```

When **both operands are constants**, the compiler catches the overflow at compile
time (`error: this arithmetic operation will overflow`). To see the *runtime*
behavior, the overflow has to involve a value the compiler can't fold away:

```rust
let mut x: u8 = 255;
x += 1;  // Panics in debug mode; silently wraps to 0 in release mode
```

In debug builds this panics with `attempt to add with overflow`; in release
builds the check is removed and the value wraps around to `0`.

**Solution:** When you *want* wraparound, use the explicit wrapping/saturating
arithmetic methods (`wrapping_add`, `saturating_add`, `checked_add`); we'll
cover these later.

---

## Section Overview

### What Makes This Section Important

**These are the building blocks!** Everything in Rust builds on:

- Variables and mutability
- The type system
- Basic operations

**Master these, and the rest becomes easier.**

### How This Relates to Later Sections

- **Section 03 (Functions):** Use these types as parameters
- **Section 05 (Ownership):** Immutability is key to ownership
- **Section 06 (Data Structures):** Compose basic types
- **Section 08 (Error Handling):** Type system enables Result/Option

### Comparison to TypeScript Fundamentals

| Concept     | TypeScript Section | Rust Section | Difficulty |
| ----------- | ------------------ | ------------ | ---------- |
| Variables   | Day 1              | Day 1        | Medium     |
| Types       | Day 1-2            | Day 1-2      | Medium     |
| Operators   | Day 1              | Day 1        | Easy       |
| Mutability  | Not emphasized     | Critical     | Hard       |
| Type safety | Optional           | Mandatory    | Medium     |

**The mutability concept is the hardest part for JS/TS developers.**

---

## Ready to Begin?

Let's start with the most important concept: variables and mutability!

### [Start with Variables and Mutability](/02-basics/00-variables/)

Or jump to:

- [Basic Types](/02-basics/01-types/)
- [Operators](/02-basics/02-operators/)
- [Comments](/02-basics/03-comments/)
- [Output and Formatting](/02-basics/04-output/)

---

## Frequently asked questions

### Are Rust variables mutable like JavaScript's `let`?

No. `let x = 5;` is immutable, and you opt into mutation with `let mut x = 5;`. This is the reverse of JavaScript, where `let` is reassignable and only `const` locks a binding. Rust's `const` is a separate, always-immutable, compile-time constant. See [Variables and Mutability](/02-basics/00-variables/).

### Why does Rust have so many number types instead of one `number`?

JavaScript's single `f64` loses integer precision past 2^53 and hides cost. Rust's sized integers (`i32`, `u64`, `usize`, …) and floats (`f32`, `f64`) make range, signedness, and memory explicit, and the compiler refuses to silently overflow them. See [Basic Types](/02-basics/01-types/).

### What is shadowing?

Re-declaring a name with `let`: `let id = "5"; let id: i32 = id.parse()?;`. The new binding can change type and reuses the name, unlike reassignment, which cannot. It is handy for refining a value through a few steps without inventing new names. See [Variables and Mutability](/02-basics/00-variables/).

