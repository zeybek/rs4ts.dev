---
title: "Operators"
description: "Rust's operators are similar to TypeScript/JavaScript, with a few important differences. Here we cover arithmetic, comparison, logical, and bitwise operators."
---

Rust's operators are similar to TypeScript/JavaScript, with a few important differences. Here we cover arithmetic, comparison, logical, and bitwise operators.

---

## Quick Overview

Most operators work the same way, but Rust:

- Has no `===` (just `==`)
- Requires same types for operations
- Has explicit bitwise operators
- No automatic type coercion

**You'll feel right at home with 90% of operators!**

---

## TypeScript/JavaScript Example

```typescript
// Arithmetic
let sum = 5 + 3; // 8
let diff = 10 - 4; // 6
let product = 3 * 4; // 12
let quotient = 10 / 3; // 3.333...
let remainder = 10 % 3; // 1

// Comparison
let equal = 5 === 5; // true
let not_equal = 5 !== 6; // true
let greater = 10 > 5; // true

// Logical
let and = true && false; // false
let or = true || false; // true
let not = !true; // false

// Type coercion!
let result = "5" + 3; // "53" (string)
let result2 = "5" - 3; // 2 (number!)
```

**Key characteristic:** Lots of implicit conversions

---

## Rust Equivalent

```rust
// Arithmetic
let sum = 5 + 3;        // 8
let diff = 10 - 4;      // 6
let product = 3 * 4;    // 12
let quotient = 10 / 3;  // 3 (integer division!)
let remainder = 10 % 3; // 1
let float_div = 10.0 / 3.0; // 3.333...

// Comparison
let equal = 5 == 5;     // true (no ===!)
let not_equal = 5 != 6; // true
let greater = 10 > 5;   // true

// Logical
let and = true && false; // false
let or = true || false;  // true
let not = !true;         // false

// No type coercion!
// let result = "5" + 3;   // Error: can't add &str and i32
let result = "5".to_string() + &3.to_string(); // Explicit
```

**Key characteristic:** No implicit conversions, explicit types

---

## Detailed Explanation

### Arithmetic Operators

```rust
let a = 10;
let b = 3;

// Basic arithmetic
let sum = a + b;         // 13
let difference = a - b;  // 7
let product = a * b;     // 30
let quotient = a / b;    // 3 (integer division!)
let remainder = a % b;   // 1

// Unary minus
let negative = -a;       // -10

// Compound assignment
let mut x = 5;
x += 3;    // x = x + 3  → 8
x -= 2;    // x = x - 2  → 6
x *= 4;    // x = x * 4  → 24
x /= 3;    // x = x / 3  → 8
x %= 5;    // x = x % 5  → 3
```

**Integer vs Float Division:**

```rust
// Integer division (truncates)
let a = 10 / 3;      // 3

// Float division
let b = 10.0 / 3.0;  // 3.333...
let c = 10 as f64 / 3 as f64; // 3.333...
```

**Compare to TypeScript:**

```typescript
let a = 10 / 3; // 3.333... (always float)
let b = Math.floor(10 / 3); // 3 (explicit truncation)
```

### Comparison Operators

```rust
let a = 5;
let b = 10;

// Equality
let eq = a == b;         // false
let ne = a != b;         // true

// Ordering
let gt = a > b;          // false
let lt = a < b;          // true
let ge = a >= b;         // false
let le = a <= b;         // true
```

**No `===` in Rust!**

```rust
// Rust only has ==
let x = 5;
let y = 5;
let equal = x == y;  // true

// TypeScript has == and ===
// == does type coercion
// === doesn't
```

**In Rust, `==` is always strict** (no type coercion possible).

### Logical Operators

```rust
let t = true;
let f = false;

// AND
let and1 = t && f;   // false
let and2 = t && t;   // true

// OR
let or1 = t || f;    // true
let or2 = f || f;    // false

// NOT
let not1 = !t;       // false
let not2 = !f;       // true
```

**Short-circuit evaluation (like TypeScript):**

```rust
fn expensive_check() -> bool {
    println!("Called!");
    true
}

let x = false && expensive_check(); // "Called!" not printed
let y = true || expensive_check();  // "Called!" not printed
```

### Bitwise Operators

```rust
let a = 0b1100;  // 12 in binary
let b = 0b1010;  // 10 in binary

// Bitwise AND
let and = a & b;     // 0b1000 (8)

// Bitwise OR
let or = a | b;      // 0b1110 (14)

// Bitwise XOR
let xor = a ^ b;     // 0b0110 (6)

// Bitwise NOT (! on integers, not just bools)
let not = !a;        // -13: inverts all 32 bits of the i32

// Left shift
let left = a << 2;   // 0b110000 (48)

// Right shift
let right = a >> 2;  // 0b0011 (3)

// Compound assignment
let mut x = 5;
x &= 3;    // x = x & 3
x |= 2;    // x = x | 2
x ^= 1;    // x = x ^ 1
x <<= 1;   // x = x << 1
x >>= 1;   // x = x >> 1
```

**Compare to TypeScript:**

```typescript
let a = 12;
let b = 10;

let and = a & b; // 8
let or = a | b; // 14
let xor = a ^ b; // 6
let not = ~a; // -13 (two's complement)
let left = a << 2; // 48
let right = a >> 2; // 3
```

**Same syntax, same behavior!**

> **Note:** In Rust the same `!` does both **logical NOT** (on `bool`) and **bitwise NOT** (on integers): there is no separate `~` operator like JavaScript's. For a signed integer, `!a` inverts all of its bits, which is equal to `-1 - a` (two's complement). So with `a` inferred as `i32` and equal to `12`, `!a` is `-13`, matching JavaScript's `~12`. On an unsigned type the result stays non-negative: `!0u8` is `255`.

---

## Key Differences from TypeScript

### 1. No Type Coercion

**TypeScript:**

```typescript
let x = "5" + 3; // "53" (string)
let y = "5" - 3; // 2 (number!)
let z = "5" == 5; // true with ==, false with ===
```

**Rust:**

```rust
// let x = "5" + 3;   // Error: can't add &str and i32
let x = format!("{}{}", "5", 3); // "53"

// let y = "5" - 3;   // Error: can't subtract
let y = "5".parse::<i32>().unwrap() - 3; // 2

// let z = "5" == 5;  // Error: mismatched types
```

> **Warning:** `.unwrap()` extracts the value from a `Result`/`Option`, but **panics** (crashes) if it is an error or `None`. It is fine for examples and throwaway code where you know the input is valid, but in real code you should handle the error case explicitly with `match`, the `?` operator, or `.expect("message")` (covered in the error handling section).

### 2. Integer Division

**TypeScript:**

```typescript
let x = 10 / 3; // 3.333... (always float)
```

**Rust:**

```rust
let x = 10 / 3;      // 3 (integer division!)
let y = 10.0 / 3.0;  // 3.333... (float division)
```

### 3. No `===` Operator

**TypeScript:**

```typescript
5 == "5"; // true (type coercion)
5 === "5"; // false (strict equality)
```

**Rust:**

```rust
5 == 5;    // true
// 5 == "5";  // Error: mismatched types
```

**Rust only has `==`, but it's always strict!**

### 4. Overflow Behavior

**TypeScript:**

```typescript
let x: number = Number.MAX_SAFE_INTEGER;
x = x + 1; // Still works (but loses precision)
```

**Rust (debug mode):**

```rust
let mut x: i32 = i32::MAX;
// x = x + 1;  // Panics! (overflow)
x = x.wrapping_add(1); // Wraps around
```

---

## Common Pitfalls

### Pitfall 1: Integer Division Surprise

**Problem:**

```rust
let average = (5 + 10) / 2; // Expected 7.5, got 7!
```

**Why:** Integer division truncates.

**Solution:**

```rust
let average = (5.0 + 10.0) / 2.0;      // 7.5
let average = (5 + 10) as f64 / 2.0;   // 7.5
```

### Pitfall 2: Comparing Different Types

**Problem:**

```rust
let x: i32 = 5;
let y: i64 = 5;
// if x == y {  // Error: mismatched types
```

**Solution:**

```rust
let x: i32 = 5;
let y: i64 = 5;
if x as i64 == y {  // Convert x to i64
    println!("Equal!");
}
```

### Pitfall 3: Expecting `===`

**Problem:**

```rust
// if x === y {  // Error: no operator ===
```

**Solution:**

```rust
if x == y {  // Use ==
    println!("Equal!");
}
```

### Pitfall 4: Overflow Without Handling

**Problem:**

```rust
fn add_100(x: u8) -> u8 {
    x + 100 // Panics in debug mode when x is 200!
}

fn main() {
    println!("{}", add_100(200));
}
```

Running this in debug mode panics with `thread 'main' panicked at ...: attempt to add with overflow`.

> **Note:** If you instead write the overflow with two literals the compiler can see (for example `let y = 200u8 + 100;`), it does not even compile: you get `error: this arithmetic operation will overflow` at compile time. The runtime panic only happens when at least one operand isn't a compile-time constant (here `x` arrives as a function argument).

**Solution:**

```rust
let x: u8 = 200;
let y = x.saturating_add(100);  // 255 (clamps)
let z = x.wrapping_add(100);    // 44 (wraps)
let w = x.checked_add(100);     // None (returns Option)
```

---

## Best Practices

### 1. Use Explicit Types for Division

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Ambiguous:**

```rust
let result = total / count; // Integer or float division?
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Explicit:**

```rust
let result = total as f64 / count as f64; // Clearly float division
```

### 2. Use Checked Arithmetic for User Input

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Risky:**

```rust
let result = user_a + user_b; // Could overflow!
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Safe:**

```rust
let result = user_a.checked_add(user_b)
    .expect("Overflow!");
```

### 3. Use Ranges for Bounds Checks

Unlike Python, Rust has **no chained comparisons** (`0 < x < 100` does not parse). You either combine two comparisons with `&&`, or — often clearer — use a range's `.contains()` method:

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Verbose:**

```rust
if x > 0 && x < 100 {
    // ...
}
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Clear (when appropriate):**

```rust
if (0..100).contains(&x) {
    // ...
}
```

### 4. Parenthesize Complex Expressions

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Hard to read:**

```rust
let result = a + b * c - d / e;
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Clear:**

```rust
let result = a + (b * c) - (d / e);
```

---

## Real-World Example

### Temperature Conversion

**TypeScript:**

```typescript
function celsiusToFahrenheit(celsius: number): number {
  return (celsius * 9) / 5 + 32;
}

function fahrenheitToCelsius(fahrenheit: number): number {
  return ((fahrenheit - 32) * 5) / 9;
}

console.log(celsiusToFahrenheit(0)); // 32
console.log(fahrenheitToCelsius(32)); // 0
```

**Rust:**

```rust playground
fn celsius_to_fahrenheit(celsius: f64) -> f64 {
    (celsius * 9.0) / 5.0 + 32.0
}

fn fahrenheit_to_celsius(fahrenheit: f64) -> f64 {
    (fahrenheit - 32.0) * 5.0 / 9.0
}

fn main() {
    println!("{}", celsius_to_fahrenheit(0.0));  // 32
    println!("{}", fahrenheit_to_celsius(32.0)); // 0
}
```

### Calculating Percentage

**TypeScript:**

```typescript
function percentage(value: number, total: number): number {
  return (value / total) * 100;
}

console.log(percentage(25, 100)); // 25
console.log(percentage(1, 3)); // 33.333...
```

**Rust:**

```rust playground
fn percentage(value: f64, total: f64) -> f64 {
    (value / total) * 100.0
}

fn main() {
    println!("{}", percentage(25.0, 100.0)); // 25
    println!("{:.2}", percentage(1.0, 3.0)); // 33.33
}
```

### Bit Manipulation (Flags)

```rust playground
// Permission flags using bitwise operations
const READ: u8 = 0b0001;    // 1
const WRITE: u8 = 0b0010;   // 2
const EXECUTE: u8 = 0b0100; // 4
const DELETE: u8 = 0b1000;  // 8

fn main() {
    // Combine permissions
    let perms = READ | WRITE; // 0b0011 (3)

    // Check permission
    let has_read = (perms & READ) != 0;   // true
    let has_exec = (perms & EXECUTE) != 0; // false

    // Add permission
    let perms = perms | EXECUTE; // 0b0111 (7)

    // Remove permission
    let perms = perms & !WRITE; // 0b0101 (5)

    println!("Permissions: {:04b}", perms); // 0101
}
```

---

## Operator Precedence

From highest to lowest:

1. Unary: `-`, `!`
2. Multiplicative: `*`, `/`, `%`
3. Additive: `+`, `-`
4. Shift: `<<`, `>>`
5. Bitwise AND: `&`
6. Bitwise XOR: `^`
7. Bitwise OR: `|`
8. Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
9. Logical AND: `&&`
10. Logical OR: `||`

**Use parentheses when in doubt!**

```rust
let result = a + b * c;     // b * c first
let result = (a + b) * c;   // a + b first
```

---

## Further Reading

### Official Documentation

- [The Rust Book - Operators](https://doc.rust-lang.org/book/appendix-02-operators.html)
- [Rust Reference - Operators](https://doc.rust-lang.org/reference/expressions/operator-expr.html)
- [Rust by Example - Operations](https://doc.rust-lang.org/rust-by-example/primitives.html)

---

## Exercises

### Exercise 1: Fix Division

Fix this to return a float:

```rust
fn average(a: i32, b: i32) -> f64 {
    (a + b) / 2  // Returns integer!
}

fn main() {
    println!("{}", average(5, 10)); // Should be 7.5
}
```

<details>
<summary>Solution</summary>

```rust playground
fn average(a: i32, b: i32) -> f64 {
    (a + b) as f64 / 2.0
}

fn main() {
    println!("{}", average(5, 10)); // 7.5
}
```

</details>

### Exercise 2: Check Range

Check if a number is in range 10-20 (inclusive):

```rust
fn in_range(x: i32) -> bool {
    // Implement
}

fn main() {
    println!("{}", in_range(15));  // true
    println!("{}", in_range(25));  // false
}
```

<details>
<summary>Solution</summary>

```rust playground
fn in_range(x: i32) -> bool {
    x >= 10 && x <= 20
}

fn main() {
    println!("{}", in_range(15)); // true
    println!("{}", in_range(25)); // false
}
```

</details>

### Exercise 3: Swap Values

Swap two values without a temporary variable:

```rust playground
fn main() {
    let mut a = 5;
    let mut b = 10;

    // Swap a and b
    // Hint: Rust gives you two idiomatic, overflow-safe ways

    println!("a: {}, b: {}", a, b); // Should be a: 10, b: 5
}
```

<details>
<summary>Solution</summary>

The classic "arithmetic swap" (`a = a + b; b = a - b; a = a - b;`) works for small numbers but **overflows and panics in debug** when the values are near the integer bounds (e.g. `a = i32::MAX`). Rust gives you two safe, idiomatic alternatives instead:

```rust playground
fn main() {
    // Option 1: tuple destructuring assignment
    let mut a = 5;
    let mut b = 10;
    (a, b) = (b, a);
    println!("a: {}, b: {}", a, b); // a: 10, b: 5

    // Option 2: std::mem::swap
    let mut c = 5;
    let mut d = 10;
    std::mem::swap(&mut c, &mut d);
    println!("c: {}, d: {}", c, d); // c: 10, d: 5
}
```

> **Warning:** The arithmetic trick (`a = a + b; ...`) is a classic interview puzzle, but it is fragile: `a + b` can overflow. In debug builds Rust **panics**, in release builds it **wraps** to the wrong answer. Prefer tuple destructuring or `std::mem::swap`.

</details>

### Exercise 4: Check Power of Two

Check if a number is a power of 2 using bitwise operations:

```rust
fn is_power_of_two(n: u32) -> bool {
    // Hint: Powers of 2 have only one bit set
    // n & (n - 1) == 0 for powers of 2
}

fn main() {
    println!("{}", is_power_of_two(8));   // true
    println!("{}", is_power_of_two(10));  // false
    println!("{}", is_power_of_two(16));  // true
}
```

<details>
<summary>Solution</summary>

```rust playground
fn is_power_of_two(n: u32) -> bool {
    n != 0 && (n & (n - 1)) == 0
}

fn main() {
    println!("{}", is_power_of_two(8));  // true
    println!("{}", is_power_of_two(10)); // false
    println!("{}", is_power_of_two(16)); // true
}
```

</details>

---

## Summary

**What you've learned:**

- Arithmetic operators (`+`, `-`, `*`, `/`, `%`)
- Comparison operators (`==`, `!=`, `<`, `>`, etc.)
- Logical operators (`&&`, `||`, `!`)
- Bitwise operators (`&`, `|`, `^`, `<<`, `>>`)
- No type coercion in Rust
- Integer vs float division
- Operator precedence

**Key differences from TypeScript:**

| Aspect        | TypeScript   | Rust                |
| ------------- | ------------ | ------------------- |
| Equality      | `==`, `===`  | Only `==` (strict)  |
| Division      | Always float | Integer if integers |
| Type coercion | Implicit     | Never!              |
| Overflow      | Loses precision (f64) | Panics (debug), wraps (release) |

**Operators work mostly the same, but Rust is more strict!**
