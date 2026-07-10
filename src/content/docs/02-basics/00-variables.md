---
title: "Variables and Mutability"
description: "Rust variables are immutable by default, the opposite of JavaScript. See how let, let mut, shadowing, and const differ from TypeScript's let and const bindings."
---

Understanding variables in Rust matters. Unlike JavaScript/TypeScript where everything is mutable by default, Rust makes variables **immutable by default**. This is the single biggest mindset shift you'll need to make.

---

## Quick Overview

In Rust, variables are immutable unless explicitly marked as mutable with `mut`. This forces you to think about which data needs to change and which doesn't, leading to safer and more predictable code.

**In short:** `let` creates immutable bindings, `let mut` creates mutable bindings.

---

## TypeScript/JavaScript Example

```typescript
// TypeScript/JavaScript - everything mutable by default
let x = 5;
x = 6; // OK - can reassign

const y = 10;
// y = 11;  // Error - const prevents reassignment

// But const doesn't prevent mutation!
const arr = [1, 2, 3];
arr.push(4); // OK - array itself is mutable!
arr[0] = 99; // OK
// arr = [];  // Error - can't reassign

const obj = { value: 42 };
obj.value = 43; // OK - object is mutable!
// obj = {};     // Error - can't reassign
```

**Key points:**

- `let` = mutable (can reassign)
- `const` = can't reassign, but contents can mutate
- Mutability is the default

---

## Rust Equivalent

```rust
// Rust - immutable by default!
let x = 5;
// x = 6;  // Compile error: cannot assign twice to immutable variable

let mut y = 10;
y = 11;    // OK - explicitly marked as mutable

// Arrays/vectors
let arr = vec![1, 2, 3];
// arr.push(4);  // Error - arr is immutable
// arr[0] = 99;  // Error

let mut arr2 = vec![1, 2, 3];
arr2.push(4);  // OK
arr2[0] = 99;  // OK

// Constants (compile-time constants)
const MAX_POINTS: u32 = 100_000;
// MAX_POINTS = 200_000;  // Error - constants are never mutable
```

**Key points:**

- `let` = immutable (cannot reassign or mutate)
- `let mut` = mutable (can reassign and mutate)
- `const` = compile-time constant (must have type annotation)
- Immutability is the default

---

## Detailed Explanation

### Immutable Variables

```rust playground
fn main() {
    let x = 5;
    println!("The value of x is: {}", x);

    // x = 6;  // This would cause a compile error:
    // error[E0384]: cannot assign twice to immutable variable `x`
}
```

**Why immutable by default?**

1. **Safety:** Prevents accidental modification
2. **Concurrency:** Immutable data is safe to share across threads
3. **Optimization:** Compiler can make better optimizations
4. **Intent:** Makes your intentions explicit

**This is the opposite of JavaScript!**

### Mutable Variables

```rust playground
fn main() {
    let mut x = 5;
    println!("The value of x is: {}", x);

    x = 6;  // OK - x is mutable
    println!("The value of x is: {}", x);
}
```

**Output:**

```
The value of x is: 5
The value of x is: 6
```

**When to use `mut`:**

- Counters in loops
- Accumulators
- Any value that needs to change over time

### Shadowing

Rust has a unique feature called **shadowing** that lets you redeclare a variable:

```rust playground
fn main() {
    let x = 5;

    let x = x + 1;  // OK - shadowing

    {
        let x = x * 2;  // OK - shadows in this scope
        println!("Inner scope: {}", x);  // 12
    }

    println!("Outer scope: {}", x);  // 6
}
```

**Output:**

```
Inner scope: 12
Outer scope: 6
```

**Shadowing vs Mutation:**

```rust
// Shadowing - creates new variable
let x = 5;
let x = x + 1;  // New variable, can change type

// Mutation - changes existing variable
let mut y = 5;
y = y + 1;      // Same variable, same type
```

**Shadowing allows type changes:**

```rust
let spaces = "   ";           // &str
let spaces = spaces.len();    // OK - now usize

let mut spaces2 = "   ";
// spaces2 = spaces2.len();   // Error - type mismatch
```

### Constants

```rust playground
// Constants must:
// 1. Have a type annotation
// 2. Be assigned a constant expression (evaluated at compile time)
// 3. Use SCREAMING_SNAKE_CASE naming

const MAX_POINTS: u32 = 100_000;
const PI: f64 = 3.14159;
const APP_NAME: &str = "MyApp";

fn main() {
    println!("Max points: {}", MAX_POINTS);
    // MAX_POINTS = 200_000;  // Error - constants are always immutable
}
```

> **Note:** "Constant expression" does not mean "no function calls." A `const`
> can call any **`const fn`** and many standard-library methods that are marked
> `const`, as long as everything is evaluable at compile time. For example,
> `const NAME_LEN: usize = "MyApp".len();` and a call to your own
> `const fn double(n: u32) -> u32 { n * 2 }` both work. What you cannot do is
> call a *non-`const`* function (anything that needs to run at runtime, like
> reading the clock or allocating a `Vec`).

**Constants vs Immutable Variables:**

| Feature           | `const`              | `let`               |
| ----------------- | -------------------- | ------------------- |
| Mutability        | Never                | Unless `mut`        |
| Type annotation   | Required             | Optional (inferred) |
| Computation       | Compile-time only    | Runtime OK          |
| Scope             | Global or local      | Local only          |
| Naming convention | SCREAMING_SNAKE_CASE | snake_case          |
| Can be shadowed   | No                   | Yes                 |

**When to use `const`:**

- Configuration values
- Mathematical constants
- String literals used multiple times
- Values that truly never change

---

## Key Differences from TypeScript/JavaScript

### 1. Default Mutability

**JavaScript:**

```javascript
let x = 5; // Can change
const y = 5; // Can't reassign (but contents can mutate)
```

**Rust:**

```rust
let x = 5;      // Can't change
let mut y = 5;  // Can change
const Z: i32 = 5;  // Can't change, compile-time constant
```

**Mental model shift:** Rust's `let` is *stricter* than JavaScript's `const`.
JavaScript `const` only blocks **reassignment** of the binding; the value it
points to can still be mutated (`arr.push(4)`). Rust's immutable `let` blocks
**both** reassignment *and* mutation of the value. (And don't confuse this with
Rust's own `const`, which is a separate, compile-time-only construct covered
above, not the everyday tool for local bindings.)

### 2. Mutation vs Reassignment

**JavaScript `const`:**

```javascript
const arr = [1, 2, 3];
arr.push(4); // OK - array is mutable
arr[0] = 99; // OK
arr = []; // Error - can't reassign
```

**Rust `let`:**

```rust
let arr = vec![1, 2, 3];
// arr.push(4);  // Error - vector is immutable
// arr[0] = 99;  // Error
// arr = vec![]; // Error
```

**In Rust, immutable means truly immutable!**

### 3. Shadowing

**JavaScript:**

```javascript
let x = 5;
let x = 10; // SyntaxError: Identifier 'x' has already been declared
```

**Rust:**

```rust
let x = 5;
let x = 10;  // OK - shadowing
```

### 4. Type Changes

**TypeScript:**

```typescript
let spaces = "   ";
spaces = spaces.length; // Type error: string to number
```

**Rust with shadowing:**

```rust
let spaces = "   ";
let spaces = spaces.len();  // OK - shadowing allows type change
```

**Rust with mutation:**

```rust
let mut spaces = "   ";
// spaces = spaces.len();  // Error - can't change type
```

---

## Common Pitfalls

### Pitfall 1: Forgetting `mut`

**Problem:**

```rust
fn main() {
    let counter = 0;

    for i in 1..=5 {
        counter += 1;  // Error: cannot assign to immutable variable
    }
}
```

**Error** (the compiler also warns that `counter` and `i` are unused, since the code never compiles far enough to use them):

```
warning: variable `counter` is assigned to, but never used
 --> src/main.rs:2:9
  |
2 |     let counter = 0;
  |         ^^^^^^^
  |
  = note: consider using `_counter` instead
  = note: `#[warn(unused_variables)]` on by default

warning: unused variable: `i`
 --> src/main.rs:4:9
  |
4 |     for i in 1..=5 {
  |         ^ help: if this is intentional, prefix it with an underscore: `_i`

error[E0384]: cannot assign twice to immutable variable `counter`
 --> src/main.rs:5:9
  |
2 |     let counter = 0;
  |         ------- first assignment to `counter`
...
5 |         counter += 1;  // Error: cannot assign to immutable variable
  |         ^^^^^^^^^^^^ cannot assign twice to immutable variable
  |
help: consider making this binding mutable
  |
2 |     let mut counter = 0;
  |         +++
```

**Solution:**

```rust playground
fn main() {
    let mut counter = 0;  // Add 'mut'

    for i in 1..=5 {
        counter += 1;  // OK
    }

    println!("Counter: {}", counter);
}
```

### Pitfall 2: Thinking `const` Works Like TypeScript

**Problem:**

```rust
const MAX_POINTS = 100_000;  // Error: missing type annotation
```

**Error:**

```
error: missing type for `const` item
 --> src/main.rs:1:17
  |
1 | const MAX_POINTS = 100_000;  // Error: missing type annotation
  |                 ^ help: provide a type for the constant: `: i32`
```

**Solution:**

```rust
const MAX_POINTS: u32 = 100_000;  // OK - type annotation required
```

### Pitfall 3: Trying to Mutate Immutable Data

**Problem:**

```rust
fn main() {
    let v = vec![1, 2, 3];
    v.push(4);  // Error: cannot borrow as mutable
}
```

**Error:**

```
error[E0596]: cannot borrow `v` as mutable, as it is not declared as mutable
 --> src/main.rs:3:5
  |
3 |     v.push(4);  // Error: cannot borrow as mutable
  |     ^ cannot borrow as mutable
  |
help: consider changing this to be mutable
  |
2 |     let mut v = vec![1, 2, 3];
  |         +++
```

**Solution:**

```rust playground
fn main() {
    let mut v = vec![1, 2, 3];  // Add 'mut'
    v.push(4);  // OK
}
```

### Pitfall 4: Confusing Shadowing with Mutation

**Problem thinking:**

```rust
let x = 5;
let x = 6;  // "This is reassignment, right?"
```

**Reality:** It's shadowing - a new variable with the same name.

```rust playground
fn main() {
    let x = 5;
    println!("Address: {:p}", &x);

    let x = 6;
    println!("Address: {:p}", &x);  // a different address
}
```

The two `&x` print *different* addresses, which proves these are two separate
variables rather than one variable being reassigned. The actual addresses and
the gap between them are not specified by the language — they depend on the
platform, the optimization level, and stack layout, so don't read anything into
the exact hex values or assume a fixed offset.

**These are different variables in memory!**

---

## Best Practices

### 1. Prefer Immutability

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Don't default to `mut`:**

```rust playground
fn main() {
    let mut x = 5;  // Unnecessary mut
    let mut y = 10; // Unnecessary mut
    println!("{} {}", x, y);
}
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Only use `mut` when needed:**

```rust playground
fn main() {
    let x = 5;      // Immutable - won't change
    let mut y = 10; // Mutable - will change
    y += 5;
    println!("{} {}", x, y);
}
```

**Why:** Immutability makes code easier to reason about and enables compiler optimizations.

### 2. Use Shadowing for Transformations

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Don't create new variable names:**

```rust
let spaces_str = "   ";
let spaces_count = spaces_str.len();
let spaces_display = format!("Count: {}", spaces_count);
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Use shadowing for the same concept:**

```rust
let spaces = "   ";
let spaces = spaces.len();
let spaces = format!("Count: {}", spaces);
```

**When shadowing makes sense:**

- Type transformations
- Progressive calculations
- Parsing/validation steps

### 3. Use Constants for True Constants

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Don't use variables for constants:**

```rust playground
fn main() {
    let max_points = 100_000;  // Used everywhere
    // ... lots of code ...
}
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Use `const` for values that never change:**

```rust playground
const MAX_POINTS: u32 = 100_000;

fn main() {
    // MAX_POINTS available everywhere
}
```

### 4. Clear Variable Names

**<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Don't use generic names:**

```rust
let mut x = vec![];
let mut y = vec![];
```

**<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> Use descriptive names:**

```rust
let mut users = vec![];
let mut products = vec![];
```

**Even with mutability, clarity matters!**

---

## Real-World Example

### Calculating Running Average

**TypeScript:**

```typescript
function calculateRunningAverage(numbers: number[]): number[] {
  let sum = 0;
  const averages = [];

  for (let i = 0; i < numbers.length; i++) {
    sum += numbers[i];
    averages.push(sum / (i + 1));
  }

  return averages;
}

const nums = [10, 20, 30, 40, 50];
console.log(calculateRunningAverage(nums));
// [10, 15, 20, 25, 30]
```

**Rust:**

```rust playground
fn calculate_running_average(numbers: &[i32]) -> Vec<f64> {
    let mut sum = 0;
    let mut averages = Vec::new();

    for (i, &num) in numbers.iter().enumerate() {
        sum += num;
        averages.push(sum as f64 / (i + 1) as f64);
    }

    averages
}

fn main() {
    let nums = vec![10, 20, 30, 40, 50];
    let result = calculate_running_average(&nums);
    println!("{:?}", result);
    // [10.0, 15.0, 20.0, 25.0, 30.0]
}
```

**Notice:**

- `sum` and `averages` are `mut` (they change)
- `numbers` is immutable (just reading)
- Clear intent about what changes

> **Note:** This example sneaks in a few things from later chapters — `&[i32]` (a borrowed slice, [Section 05](/05-ownership/)), `.iter().enumerate()` ([Section 07: Iterators](/07-collections/06-iterators/)), and `as f64` casts ([Basic Types](/02-basics/01-types/)). Don't worry about those yet; focus on which bindings are `mut` and which are not.

### Configuration with Constants

**TypeScript:**

```typescript
const CONFIG = {
  MAX_CONNECTIONS: 100,
  TIMEOUT_MS: 5000,
  API_URL: "https://api.example.com",
};

function connect() {
  // Use CONFIG.MAX_CONNECTIONS
}
```

**Rust:**

```rust playground
const MAX_CONNECTIONS: u32 = 100;
const TIMEOUT_MS: u64 = 5000;
const API_URL: &str = "https://api.example.com";

fn connect() {
    // Use MAX_CONNECTIONS
}

fn main() {
    println!("Max connections: {}", MAX_CONNECTIONS);
}
```

**Constants are available globally without runtime overhead!**

---

## Further Reading

### Official Documentation

- [The Rust Book - Variables and Mutability](https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html)
- [Rust Reference - Constants](https://doc.rust-lang.org/reference/items/constant-items.html)
- [Rust by Example - Variable Bindings](https://doc.rust-lang.org/rust-by-example/variable_bindings.html)

### Related Topics

- [Ownership](/05-ownership/) - Immutability is key to ownership
- [Borrowing](/05-ownership/02-borrowing/) - Mutable vs immutable references

---

## Exercises

### Exercise 1: Fix the Mutability

Fix this code:

```rust
fn main() {
    let x = 5;
    println!("x is: {}", x);

    x = 6;
    println!("x is now: {}", x);
}
```

<details>
<summary>Solution</summary>

```rust playground
fn main() {
    let mut x = 5;  // Add 'mut'
    println!("x is: {}", x);

    x = 6;
    println!("x is now: {}", x);
}
```

</details>

### Exercise 2: Use Shadowing

Rewrite using shadowing to convert a string to its length:

```rust playground
fn main() {
    let text = "Hello, Rust!";
    let text_length = text.len();
    println!("Text: '{}' has length {}", text, text_length);
}
```

<details>
<summary>Solution</summary>

```rust playground
fn main() {
    let text = "Hello, Rust!";
    println!("Text: '{}'", text);

    let text = text.len();  // Shadow with new type
    println!("Length: {}", text);
}
```

</details>

### Exercise 3: Constants

Create constants for a game:

- Maximum health: 100
- Starting gold: 50
- Player name: "Hero"

```rust
// Add constants here

fn main() {
    println!("Max health: {}", /* use constant */);
    println!("Starting gold: {}", /* use constant */);
    println!("Player: {}", /* use constant */);
}
```

<details>
<summary>Solution</summary>

```rust playground
const MAX_HEALTH: u32 = 100;
const STARTING_GOLD: u32 = 50;
const PLAYER_NAME: &str = "Hero";

fn main() {
    println!("Max health: {}", MAX_HEALTH);
    println!("Starting gold: {}", STARTING_GOLD);
    println!("Player: {}", PLAYER_NAME);
}
```

</details>

### Exercise 4: Counter

Write a function that counts from 1 to n:

```rust playground
fn count_to(n: i32) {
    // Implement using a mutable counter
}

fn main() {
    count_to(5);
    // Should print the numbers 1 to 5, space-separated
}
```

<details>
<summary>Solution</summary>

```rust playground
fn count_to(n: i32) {
    let mut counter = 1;
    while counter <= n {
        print!("{} ", counter);
        counter += 1;
    }
    println!();
}

fn main() {
    count_to(5);
}
```

</details>

---

## Summary

**What you've learned:**

- Rust variables are immutable by default
- Use `let mut` for mutable variables
- Shadowing lets you reuse names and change types
- Constants require type annotations and are compile-time
- Immutability leads to safer, more maintainable code

**Key syntax:**

```rust
let x = 5;              // Immutable
let mut y = 10;         // Mutable
const Z: i32 = 100;     // Constant

let x = x + 1;          // Shadowing (new variable)
y = y + 1;              // Mutation (same variable)
```

**Mental model:**

- Default to immutable
- Add `mut` only when needed
- Use shadowing for transformations
- Use `const` for true constants

**This is the foundation!** Everything else in Rust builds on this concept.
