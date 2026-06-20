---
title: "Function Pointers"
description: "Rust's fn pointer type is TypeScript's (x) => x, but Rust splits named functions, fn pointers, and closures into distinct types, and only non-capturing ones coerce."
---

In JavaScript and TypeScript, functions are just values: you assign them to variables, stash them in objects, and pass them around without a second thought. Rust lets you do the same, but it draws a sharp line between a **function pointer** (the `fn` type) and a **closure**, and it even gives every named function its own unique zero-sized type.

---

## Quick Overview

A **function pointer** in Rust is a value whose type is written `fn(Args) -> Ret`: a plain pointer to compiled code, exactly like passing a named function in TypeScript. The twist is that each named function actually has its own unique **function item** type (with size 0), which Rust silently coerces to a `fn` pointer when needed. Function pointers are the lightweight cousin of closures: they cannot capture surrounding state, so reach for them when you only need to pass an existing named function around.

---

## TypeScript/JavaScript Example

```typescript
// A couple of named functions with the same shape.
function double(x: number): number {
  return x * 2;
}

function increment(x: number): number {
  return x + 1;
}

// A higher-order function: its parameter `f` is itself a function.
// In TypeScript a function type is written `(arg: T) => R`.
function applyTwice(f: (x: number) => number, value: number): number {
  return f(f(value));
}

console.log(applyTwice(double, 5)); // double(double(5)) = 20
console.log(applyTwice(increment, 5)); // increment(increment(5)) = 7

// Functions are values: store them in arrays, objects, anywhere.
const ops: Array<(x: number) => number> = [double, increment];
for (const op of ops) {
  console.log(op(10));
}
```

In TypeScript every function is a first-class object. There is exactly one notion of "a function value", and whether it captures variables (a closure) or not makes no difference to its type: `(x: number) => number` describes both.

---

## Rust Equivalent

```rust playground
fn double(x: i32) -> i32 {
    x * 2
}

fn increment(x: i32) -> i32 {
    x + 1
}

// `f: fn(i32) -> i32` is a function POINTER parameter.
fn apply_twice(f: fn(i32) -> i32, value: i32) -> i32 {
    f(f(value))
}

fn main() {
    // Bind a named function to a variable of fn-pointer type.
    let f: fn(i32) -> i32 = double;
    println!("{}", f(21)); // 42

    println!("{}", apply_twice(double, 5)); // 20
    println!("{}", apply_twice(increment, 5)); // 7

    // Store function pointers in an array, just like in TypeScript.
    let ops: [fn(i32) -> i32; 2] = [double, increment];
    for op in ops {
        println!("{}", op(10));
    }
}
```

Output (real, from `cargo run`):

```text
42
20
7
20
11
```

The `fn(i32) -> i32` type is the Rust spelling of TypeScript's `(x: number) => number`, *for named functions and non-capturing closures only*. The moment a function needs to capture state, you need a closure type instead, which is what the [Arrow Functions and Closures](/03-functions/03-arrow-vs-closures/) topic covers.

---

## Detailed Explanation

### Two types hide behind one function

When you write `fn double(x: i32) -> i32 { .. }`, Rust creates a value `double` whose type is **not** `fn(i32) -> i32`. It is a unique, anonymous **function item type**: the compiler refers to it as `fn(i32) -> i32 {double}`. Every named function gets its own distinct item type, and that type is **zero-sized**: it carries no data at runtime because the identity of the function is known at compile time.

A **function pointer** (`fn(i32) -> i32`, with no name in braces) is the runtime-flavored version: an actual pointer to code, the size of one machine word. You rarely write the function-item type yourself. Rust coerces a function item to a function pointer automatically wherever a `fn` type is expected (assigning to an annotated variable, passing to a `fn` parameter, putting items of different functions into the same collection, and so on).

```rust playground
fn double(x: i32) -> i32 { x * 2 }

fn main() {
    // The function ITEM `double` is a zero-sized type.
    println!("size of function item: {}", std::mem::size_of_val(&double));

    // A function POINTER is a real pointer (8 bytes on a 64-bit target).
    let p: fn(i32) -> i32 = double;
    println!("size of fn pointer:   {}", std::mem::size_of_val(&p));
}
```

Real output:

```text
size of function item: 0
size of fn pointer:   8
```

> **Note:** The zero-sized function item is a performance feature. When you call `double` directly, or pass it to a generic `fn foo<F: Fn(...)>(f: F)`, the compiler knows the exact function and can inline the call with no indirection. Coercing to a `fn` pointer erases that identity, so calls go through a real (cheap, but non-inlinable) indirect jump.

### Passing a named function by name

To "pass a function" you just use its name as a value, no `&`, no parentheses, no special syntax:

```rust playground
fn square(x: i32) -> i32 { x * x }

fn main() {
    let nums = [1, 2, 3];
    // `square` (a function item) is accepted because `map` takes anything
    // implementing `Fn`, and every function item/pointer implements `Fn`.
    let squared: Vec<i32> = nums.iter().copied().map(square).collect();
    println!("{squared:?}"); // [1, 4, 9]
}
```

This is the direct analogue of `numbers.map(square)` in JavaScript. The difference is purely in the type machinery: `map` is generic over `Fn`, and `square`'s function-item type implements `Fn`, so it fits.

### `fn` pointers versus closures

A closure is a function bundled with the environment it captured. A `fn` pointer has *no* environment. It is just an address. That leads to the single most important rule:

> A closure can be coerced to a `fn` pointer **only if it captures nothing**.

```rust playground
fn run(f: fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}

// Generic over `Fn` — accepts BOTH fn pointers and capturing closures.
fn run_any<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(x)
}

fn square(x: i32) -> i32 { x * x }

fn main() {
    // A non-capturing closure coerces to a fn pointer.
    let f: fn(i32) -> i32 = |x| x + 100;
    println!("{}", run(f, 1)); // 101

    // A capturing closure is NOT a fn pointer, but satisfies `Fn`.
    let factor = 3;
    let scale = |x: i32| x * factor;
    println!("{}", run_any(scale, 10)); // 30

    // Function items satisfy `Fn` too, so they also work with generics/adapters.
    println!("{:?}", [1, 2, 3].map(square)); // [1, 4, 9]
}
```

Real output:

```text
101
30
[1, 4, 9]
```

The takeaway for API design: **accept `impl Fn(...)` (a generic), not `fn(...)`**, unless you specifically need a plain pointer (an FFI boundary, a `static` table, or storing many functions of identical type compactly). A `fn` parameter is the most restrictive choice; `impl Fn` accepts function pointers *and* closures.

### Constructors are functions too

A subtle, delightful Rust feature: tuple-struct constructors and enum-variant constructors are themselves callable function values. You can pass their name anywhere a function is expected.

```rust playground
#[derive(Debug)]
struct Meters(f64);

#[derive(Debug)]
enum Token {
    Word(String),
}

fn main() {
    // `Meters` is a function value of type `fn(f64) -> Meters`.
    let readings: Vec<Meters> = [1.0, 2.5, 3.0].into_iter().map(Meters).collect();
    println!("{readings:?}");

    // `String::from` and `Token::Word` used as function values.
    let tokens: Vec<Token> = ["hi", "there"]
        .into_iter()
        .map(String::from) // associated fn as a value
        .map(Token::Word)  // enum variant constructor as a value
        .collect();
    println!("{tokens:?}");
}
```

Real output:

```text
[Meters(1.0), Meters(2.5), Meters(3.0)]
[Word("hi"), Word("there")]
```

> **Note:** Compiling this exact snippet also emits two benign `dead_code` warnings (`field `0` is never read`), because the tuple fields are only observed through the derived `Debug` impl, which the dead-code analysis intentionally ignores. The program still runs and prints the output above; add `#[allow(dead_code)]` to silence the warnings if you copy this into a probe.

There is no TypeScript equivalent: a `class`/tuple type does not give you a free `(x) => new T(x)` function to pass around. In Rust, `Meters` *is* that function.

---

## Key Differences

| Aspect | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| Function type syntax | `(x: number) => number` | `fn(i32) -> i32` |
| One type for all functions? | Yes: closures and plain functions share a type | No: function *items*, `fn` pointers, and closures are distinct |
| Capturing state | Any function may close over variables | Only closures capture; `fn` pointers never do |
| Size of a function value | Heap-allocated object reference | Function item: `0` bytes; `fn` pointer: 1 word |
| Passing a named function | `arr.map(double)` | `arr.map(double)` (identical syntax) |
| Constructors as values | Not available | Tuple-struct / enum-variant constructors *are* functions |
| Storing mixed functions | Trivial (`Function[]`) | Need a common `fn` type, or `Box<dyn Fn>` for closures |

### The trait hierarchy in one breath

Every function pointer and function item implements all three closure traits — `Fn`, `FnMut`, and `FnOnce` — because it has no state to mutate or consume. That is why a `fn` pointer is accepted by any generic bounded on the least restrictive trait it needs:

```rust playground
fn shout(s: &str) -> String { s.to_uppercase() }

fn call_fn<F: Fn(&str) -> String>(f: F, s: &str) -> String { f(s) }
fn call_fnmut<F: FnMut(&str) -> String>(mut f: F, s: &str) -> String { f(s) }
fn call_fnonce<F: FnOnce(&str) -> String>(f: F, s: &str) -> String { f(s) }

fn main() {
    let p: fn(&str) -> String = shout;
    println!("{}", call_fn(p, "hi"));     // HI
    println!("{}", call_fnmut(p, "hi"));  // HI
    println!("{}", call_fnonce(p, "hi")); // HI
}
```

Real output:

```text
HI
HI
HI
```

> **Tip:** The relationship is one-directional. A `fn` pointer is usable wherever `Fn`/`FnMut`/`FnOnce` is required, but a capturing closure is *not* usable where a `fn` pointer is required. The `Fn` traits are the more general abstraction; `fn` is the concrete, capture-free special case. The full story of `Fn`/`FnMut`/`FnOnce` lives in [Arrow Functions and Closures](/03-functions/03-arrow-vs-closures/).

---

## Common Pitfalls

### Pitfall 1: Expecting a capturing closure to be a `fn` pointer

Coming from TypeScript, where `(x) => x * factor` is "just a function", it is natural to write a `fn`-typed parameter and hand it a closure that captures a variable.

```rust
fn run(f: fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}

fn main() {
    let factor = 3;
    let scale = |x: i32| x * factor; // captures `factor`
    println!("{}", run(scale, 10));  // does not compile
}
```

The real compiler error:

```text
error[E0308]: mismatched types
 --> src/main.rs:8:24
  |
7 |     let scale = |x: i32| x * factor; // captures `factor`
  |                 -------- the found closure
8 |     println!("{}", run(scale, 10));  // does not compile
  |                    --- ^^^^^ expected fn pointer, found closure
  |                    |
  |                    arguments to this function are incorrect
  |
  = note: expected fn pointer `fn(i32) -> i32`
                found closure `{closure@src/main.rs:7:17: 7:25}`
note: closures can only be coerced to `fn` types if they do not capture any variables
 --> src/main.rs:7:30
  |
7 |     let scale = |x: i32| x * factor; // captures `factor`
  |                              ^^^^^^ `factor` captured here
```

**Fix:** make the parameter generic so it accepts closures too: `fn run<F: Fn(i32) -> i32>(f: F, x: i32) -> i32`. Unlike TypeScript, the *capture* changes the type, and the compiler points exactly at the captured variable.

### Pitfall 2: Storing two different functions and getting a "different fn item" error

Each named function has its own type, so inferring a binding from one function fixes it to *that* item type. Assigning a second function then fails.

```rust
fn double(x: i32) -> i32 { x * 2 }
fn triple(x: i32) -> i32 { x * 3 }

fn main() {
    let mut op = double; // type inferred as the item type of `double`
    op = triple;         // a different item type
    println!("{}", op(4));
}
```

The real compiler error:

```text
error[E0308]: mismatched types
 --> src/main.rs:6:10
  |
5 |     let mut op = double; // type inferred as the item type of `double`
  |                  ------ expected due to this value
6 |     op = triple;         // a different item type
  |          ^^^^^^ expected fn item, found a different fn item
  |
  = note: expected fn item `fn(_) -> _ {double}`
             found fn item `fn(_) -> _ {triple}`
  = note: different fn items have unique types, even if their signatures are the same
  = help: consider casting both fn items to fn pointers using `as fn(i32) -> i32`
```

**Fix:** force the function-pointer type, either with an annotation (`let mut op: fn(i32) -> i32 = double;`) or with an `as` cast, exactly as the compiler suggests:

```rust playground
fn double(x: i32) -> i32 { x * 2 }
fn triple(x: i32) -> i32 { x * 3 }

fn main() {
    // Cast to a fn pointer up front so both branches share one type.
    let mut op = double as fn(i32) -> i32;
    println!("{}", op(4)); // 8
    op = triple;           // OK: same fn-pointer type
    println!("{}", op(4)); // 12
}
```

Real output:

```text
8
12
```

> **Note:** Array and `vec!` literals are smart enough to coerce a mix of function items to a common `fn` pointer on their own, so `let ops = [double, triple];` compiles fine. The reassignment case above does not get that help because the binding's type is locked in by the *first* value.

### Pitfall 3: Reaching for `fn` when you mean "any callable"

A `fn(T) -> R` parameter quietly rejects every closure that captures anything, which, in practice, is most closures. If your function takes a callback, default to a generic `impl Fn` bound. Use a bare `fn` pointer only when you have a concrete reason (FFI, a `static`/`const` table, or you genuinely want the smaller, non-generic type to avoid monomorphization bloat).

### Pitfall 4: Thinking `&function` is how you pass a function

In TypeScript you never take the address of a function — and you do not in Rust either. Writing `apply(&double, 5)` produces `&fn`-of-item confusion. Pass the bare name: `apply(double, 5)`.

---

## Best Practices

- **Prefer `impl Fn(...)` for callback parameters.** It accepts function pointers *and* closures and stays zero-cost via monomorphization. Reserve `fn(...)` for the special cases below. See [Higher-Order Functions](/03-functions/04-higher-order/) for the full pattern of accepting and returning closures.
- **Use `fn` pointers for homogeneous, capture-free tables.** A registry of commands, a parser's keyword-to-handler map, or a state machine's transition table are all naturally `HashMap<K, fn(..) -> ..>`: small, copyable, and storable in `static`/`const`.
- **Add a `type` alias for repeated signatures.** `type Handler = fn(&Request) -> Response;` reads far better than repeating `fn(&Request) -> Response` across a codebase.
- **Pass constructors directly.** `iter.map(Meters)` or `iter.map(Some)` is idiomatic and clearer than `iter.map(|x| Meters(x))`.
- **Annotate the binding when collecting heterogeneous functions.** `let ops: Vec<fn(i32) -> i32> = vec![double, triple];` sidesteps the "different fn item" trap before it happens.
- **Mark FFI callbacks `extern "C" fn`.** When a function pointer crosses into C, its type must carry the C ABI: `extern "C" fn(c_int) -> c_int`. This is covered in [Unsafe and FFI](/20-unsafe-ffi/).

---

## Real-World Example

A command dispatch table, the kind of thing you would build for a calculator REPL, a chat-bot command router, or a tiny scripting layer. Each command is a plain function with an identical signature, and the registry maps names to function pointers. This is where `fn` pointers genuinely shine: every value is a single word, copyable, and storable in a `HashMap`.

```rust playground
use std::collections::HashMap;

// Each command is a plain function with the same signature.
fn cmd_add(args: &[f64]) -> f64 {
    args.iter().sum()
}

fn cmd_max(args: &[f64]) -> f64 {
    args.iter().copied().fold(f64::MIN, f64::max)
}

fn cmd_mean(args: &[f64]) -> f64 {
    if args.is_empty() {
        0.0
    } else {
        args.iter().sum::<f64>() / args.len() as f64
    }
}

// A readable alias for the shared signature.
type Command = fn(&[f64]) -> f64;

fn build_registry() -> HashMap<&'static str, Command> {
    let mut registry: HashMap<&'static str, Command> = HashMap::new();
    registry.insert("add", cmd_add);
    registry.insert("max", cmd_max);
    registry.insert("mean", cmd_mean);
    registry
}

fn main() {
    let registry = build_registry();
    let data = [10.0, 4.0, 6.0];

    for name in ["add", "max", "mean", "nope"] {
        match registry.get(name) {
            Some(command) => println!("{name} -> {}", command(&data)),
            None => println!("{name} -> unknown command"),
        }
    }
}
```

Real output:

```text
add -> 20
max -> 10
mean -> 6.666666666666667
nope -> unknown command
```

Compare the TypeScript shape you would write today:

```typescript
type Command = (args: number[]) => number;

const registry: Record<string, Command> = {
  add: (args) => args.reduce((a, b) => a + b, 0),
  max: (args) => Math.max(...args),
  mean: (args) => (args.length ? args.reduce((a, b) => a + b, 0) / args.length : 0),
};

const data = [10, 4, 6];
for (const name of ["add", "max", "mean", "nope"]) {
  const cmd = registry[name];
  console.log(name, "->", cmd ? cmd(data) : "unknown command");
}
```

The structure is nearly identical. The differences are Rust-flavored: the registry value type is the concrete `fn(&[f64]) -> f64` (not a closure type), the lookup returns an `Option` you must handle, and `cmd_max` passes `f64::max` itself as the folding function, a method path used as a function value, just like passing `Meters` earlier.

> **Tip:** If any command needed to capture state — say, a counter or a shared config — you could no longer use `fn`. The registry value would become `Box<dyn Fn(&[f64]) -> f64>`. That trade-off (cheap copyable `fn` vs. heap-allocated, capturing `dyn Fn`) is the practical reason to know the difference between the two.

---

## Further Reading

### Official Documentation

- [The Rust Reference - Function pointer types](https://doc.rust-lang.org/reference/types/function-pointer.html)
- [The Rust Reference - Function item types](https://doc.rust-lang.org/reference/types/function-item.html)
- [`std::primitive::fn`](https://doc.rust-lang.org/std/primitive.fn.html)
- [The Rust Book - Function Pointers](https://doc.rust-lang.org/book/ch13-01-closures.html) (closures chapter; function pointers appear near the end)
- [Rust by Example - Higher Order Functions](https://doc.rust-lang.org/rust-by-example/fn/hof.html)

### Related Sections in This Guide

- [Basic Functions](/03-functions/00-basic-functions/) — how `fn` definitions and signatures are built in the first place
- [Function Parameters](/03-functions/01-parameters/) — passing data into functions; idiomatic alternatives to default/rest params
- [Arrow Functions and Closures](/03-functions/03-arrow-vs-closures/) — closures, capture modes, and the `Fn`/`FnMut`/`FnOnce` traits in depth
- [Higher-Order Functions](/03-functions/04-higher-order/) — taking `impl Fn` and returning closures
- [Recursion](/03-functions/06-recursion/) — recursive functions and their stack considerations
- [Section 02: Basic Types](/02-basics/01-types/) — the `i32`, `f64`, and tuple types used throughout these signatures
- [Section 04: Control Flow](/04-control-flow/) — `match` and pattern matching, used to select a function above
- [Unsafe and FFI](/20-unsafe-ffi/) — `extern "C" fn` pointers across the C boundary

---

## Exercises

### Exercise 1: Operator lookup

**Difficulty:** Easy

**Objective:** Return a function pointer chosen at runtime.

**Instructions:** Write `fn operation(symbol: char) -> Option<fn(i32, i32) -> i32>` that returns `Some(..)` containing the matching binary function for `'+'`, `'-'`, and `'*'`, and `None` for anything else. Call it from `main` and apply the returned function to two numbers.

<details>
<summary>Solution</summary>

```rust playground
fn add(a: i32, b: i32) -> i32 { a + b }
fn sub(a: i32, b: i32) -> i32 { a - b }
fn mul(a: i32, b: i32) -> i32 { a * b }

fn operation(symbol: char) -> Option<fn(i32, i32) -> i32> {
    match symbol {
        '+' => Some(add),
        '-' => Some(sub),
        '*' => Some(mul),
        _ => None,
    }
}

fn main() {
    if let Some(op) = operation('*') {
        println!("{}", op(6, 7)); // 42
    }
}
```

Each `match` arm returns a different function *item*, but the declared return type `fn(i32, i32) -> i32` makes Rust coerce all of them to the same `fn` pointer. Output: `42`.

</details>

### Exercise 2: Transform with a function pointer

**Difficulty:** Medium

**Objective:** Write a higher-order function whose callback parameter is a `fn` pointer.

**Instructions:** Implement `fn transform_all(values: &[i32], f: fn(i32) -> i32) -> Vec<i32>` that applies `f` to every element and collects the results. Test it by passing a named `negate` function. Then, in a comment, explain why you could *not* pass `|x| x * some_local` to it.

<details>
<summary>Solution</summary>

```rust playground
fn transform_all(values: &[i32], f: fn(i32) -> i32) -> Vec<i32> {
    values.iter().map(|&v| f(v)).collect()
}

fn negate(x: i32) -> i32 {
    -x
}

fn main() {
    println!("{:?}", transform_all(&[1, 2, 3], negate)); // [-1, -2, -3]

    // A closure that captures a local, e.g. `|x| x * some_local`, has a closure
    // type, not `fn(i32) -> i32`, so it would NOT compile here. To accept such
    // closures, change the bound to a generic: `f: impl Fn(i32) -> i32`.
}
```

Output: `[-1, -2, -3]`.

</details>

### Exercise 3: A retry runner

**Difficulty:** Medium/Hard

**Objective:** Pass a fallible function as a `fn` pointer and call it repeatedly.

**Instructions:** Write `fn retry(f: fn(u32) -> Result<u32, String>, max: u32) -> Result<u32, String>` that calls `f` with attempt numbers `1..=max`, returning the first `Ok`, or the last `Err` if every attempt fails. Test it with a `flaky` function that only succeeds once `attempt >= 3`.

<details>
<summary>Solution</summary>

```rust playground
fn flaky(attempt: u32) -> Result<u32, String> {
    if attempt >= 3 {
        Ok(attempt)
    } else {
        Err(format!("failed on attempt {attempt}"))
    }
}

fn retry(f: fn(u32) -> Result<u32, String>, max: u32) -> Result<u32, String> {
    let mut last = Err(String::from("never ran"));
    for attempt in 1..=max {
        last = f(attempt);
        if last.is_ok() {
            return last;
        }
    }
    last
}

fn main() {
    println!("{:?}", retry(flaky, 5)); // Ok(3)
    println!("{:?}", retry(flaky, 2)); // Err("failed on attempt 2")
}
```

Output:

```text
Ok(3)
Err("failed on attempt 2")
```

Because `flaky` neither captures state nor needs to, a `fn` pointer is the perfect parameter type. If `retry` had to accept a closure that captured, say, a shared HTTP client, you would switch the bound to `f: impl FnMut(u32) -> Result<u32, String>`.

</details>
