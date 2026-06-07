---
title: "Macro Basics: What Macros Are (and Are Not)"
description: "Rust macros write code at compile time before type-checking. They are not functions or TypeScript decorators: they take tokens, expand inline, and stay hygienic."
---

## Quick Overview

A Rust **macro** is code that writes code: at compile time, the compiler expands a macro invocation like `vec![1, 2, 3]` into ordinary Rust before type-checking begins. Macros are how Rust gets variadic, type-generic constructs like `println!`, `vec!`, and `#[derive(...)]` without runtime reflection or a garbage-collected `arguments` object. This page is about the *mental model*: what macros are, what they are emphatically **not** (they are not functions and not decorators), how compile-time expansion and **hygiene** work, and when reaching for a macro is the right call.

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects the newest edition automatically. Every Rust snippet here was compiled and run on stable.

---

## TypeScript/JavaScript Example

In TypeScript/JavaScript there is no compile-time code generation in the language itself. The two things that *feel* closest to macros are **functions** (especially variadic ones) and **decorators**. Both run at *runtime*, and both are fundamentally different from a Rust macro.

```typescript
// 1. A variadic helper — runs at runtime, fixed behavior, types erased.
function sum(...nums: number[]): number {
  return nums.reduce((acc, n) => acc + n, 0);
}
console.log(sum(1, 2, 3)); // 6  — a real call happens at runtime

// 2. A "constructor helper" — also a runtime function call.
function dict<V>(...pairs: [string, V][]): Record<string, V> {
  return Object.fromEntries(pairs);
}
const scores = dict(["alice", 95], ["bob", 87]);
console.log(scores); // { alice: 95, bob: 87 }

// 3. A decorator — the thing people WRONGLY compare to Rust attributes.
//    It is a runtime function that receives and may replace the target.
function logged(value: Function, ctx: ClassMethodDecoratorContext) {
  return function (this: unknown, ...args: unknown[]) {
    console.log(`calling ${String(ctx.name)}`);
    return value.apply(this, args); // wraps and calls at runtime
  };
}

class Api {
  @logged
  fetchUser(id: number) {
    return { id };
  }
}
new Api().fetchUser(1); // logs "calling fetchUser", then runs
```

Three things to hold onto: the variadic `sum`/`dict` are **runtime function calls**, the generic `<V>` is **erased** before the code runs, and the `@logged` **decorator is a runtime wrapper function**. None of them generate new source code that the compiler then checks.

---

## Rust Equivalent

The Rust counterparts look superficially similar but happen entirely at **compile time**, before type checking, and produce real, checked Rust code.

```rust
use std::collections::HashMap;

// A variadic, type-GENERIC constructor macro. A plain function cannot do this:
// a function has a fixed arity and a single concrete element type.
macro_rules! hashmap {
    // Match zero or more `key => value` pairs, allowing a trailing comma.
    ( $( $key:expr => $val:expr ),* $(,)? ) => {{
        let mut map = HashMap::new();
        $( map.insert($key, $val); )*
        map
    }};
}

fn main() {
    // Expands at compile time into: a `let mut map`, three `insert` calls, etc.
    let scores = hashmap! {
        "alice" => 95,
        "bob"   => 87,
        "carol" => 91,
    };

    let mut entries: Vec<_> = scores.iter().collect();
    entries.sort();
    println!("{entries:?}");

    // The SAME macro with a totally different value type — no overloads needed,
    // because the macro is expanded and type-checked fresh at each call site.
    let flags = hashmap! { "debug" => true, "verbose" => false };
    println!("debug = {:?}", flags.get("debug"));
}
```

Real output:

```text
[("alice", 95), ("bob", 87), ("carol", 91)]
debug = Some(true)
```

The `hashmap!` invocation is *replaced* by the compiler with the block of code on the right-hand side of the rule. There is no `hashmap` function in the binary, only the `HashMap::new()` and `insert` calls it generated.

> **Note:** The standard library does not ship a `hashmap!` macro (only `vec!`). We build one here precisely because it shows what a macro can do that a function cannot. The popular [`maplit`](https://crates.io/crates/maplit) crate provides a real one.

---

## Detailed Explanation

### Macros run at compile time, functions run at runtime

When you write `square!(5)` with this macro:

```rust
macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}

fn main() {
    let n = 5;
    println!("square = {}", square!(n)); // prints: square = 25
}
```

the compiler does **not** emit a call instruction. It textually-but-structurally substitutes the body, so `square!(n)` becomes `n * n` *in the source* before anything is type-checked. There is no `square` symbol in the compiled binary; the generated `n * n` is. This is **zero runtime cost** for the abstraction: a macro never adds a function call, a heap allocation, or a vtable lookup of its own.

Contrast with TypeScript's `square(n)`, which compiles to a genuine function call that the JavaScript engine executes (and may or may not inline) at runtime.

### Macros are NOT functions

This is the single most important correction for a TypeScript/JavaScript developer:

- A function receives **values**; a macro receives **tokens** (pieces of source code) and produces **tokens**.
- A function has a fixed **arity** and **types**; a macro can accept a variable number of arguments of arbitrary, mixed shapes (that is how `println!("{}", x)` and `vec![1, 2, 3]` and `hashmap!{ a => b }` all work).
- A function call exists in the running program; a macro is gone by runtime, replaced by what it generated.

The trailing `!` is the syntactic flag that says "this is a macro invocation, not a function call": `println!`, `vec!`, `assert_eq!`. (Attribute and derive macros use `#[...]` instead, covered in the sibling pages.)

### Because macros take *tokens*, they preserve grouping

A famous footgun in C's textual macros does **not** happen with `macro_rules!` fragment matchers. Consider:

```rust
macro_rules! square {
    ($x:expr) => { $x * $x };
}

fn main() {
    let n = 4;
    let r = square!(n + 1);          // captured as ONE expression: (n + 1)
    println!("square!(n + 1) = {r}");
    println!("manual n + 1 * n + 1 = {}", n + 1 * n + 1);
}
```

Real output:

```text
square!(n + 1) = 25
manual n + 1 * n + 1 = 9
```

A C-style textual macro would paste `n + 1 * n + 1` and print `9`. Rust's `:expr` **fragment specifier** captures `n + 1` as a single, already-parsed expression node, so it expands as if parenthesized and prints `25`. Rust macros operate on the parsed token tree, not raw text; they are structurally aware. (The full menu of fragment specifiers like `:expr`, `:ident`, `:ty`, `:tt` lives in [Macro Patterns](/14-macros/02-macro-patterns/).)

### Hygiene: macro-introduced names cannot collide with yours

This is the property that makes `macro_rules!` safe to use and is unlike anything in TypeScript/JavaScript text- or AST-based code generation. Identifiers a macro *creates* live in their own syntactic context and will not capture or be captured by identifiers at the call site:

```rust
// A macro that introduces a temporary binding `tmp` internally.
macro_rules! swap {
    ($a:expr, $b:expr) => {{
        let tmp = $a;   // this `tmp` belongs to the macro, NOT the caller
        $a = $b;
        $b = tmp;
    }};
}

fn main() {
    let mut tmp = 1;     // the caller has its OWN `tmp`
    let mut other = 2;
    swap!(tmp, other);   // works correctly despite the name clash
    println!("tmp = {tmp}, other = {other}");
}
```

Real output:

```text
tmp = 2, other = 1
```

Even though both the macro and the caller use the name `tmp`, they refer to different variables. The compiler tracks where each identifier was *written* (the macro definition vs. the call site) and keeps them separate. In JavaScript, a naive string-template code generator that emitted `let tmp = ...` would silently clobber a caller's `tmp`. Rust's macro hygiene prevents that class of bug entirely.

> **Note:** Hygiene applies to identifiers the macro *invents*. Identifiers you *pass in* (here `$a` and `$b`) deliberately resolve at the call site; that is what lets `swap!` touch the caller's variables. Hygiene is about accidental capture, not about blocking intentional access.

### Macros are NOT decorators

TypeScript decorators (`@logged`) are runtime functions that observe or wrap a target *after* the program starts. Rust's `#[derive(Debug)]` and other attribute macros *look* similar but are compile-time code generators: `#[derive(Debug)]` literally writes a `Debug` implementation into your binary at compile time; there is no runtime wrapping and no reflection. Saying "Rust attributes are like decorators" is a common but misleading analogy. See [Derive Macros](/14-macros/04-derive-macros/) and [Attribute Macros](/14-macros/05-attribute-macros/) for the real picture.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust macro |
| --- | --- | --- |
| When it runs | Runtime (functions, decorators) | Compile time, before type checking |
| What it operates on | Values (and erased types) | Tokens / parsed syntax |
| Runtime cost | A real call / wrapper exists | None — expands to inline code |
| Arity & types | Fixed per function | Variadic, mixed, type-generic |
| Name collisions | Possible in string codegen | Prevented by hygiene |
| Invocation marker | `f(...)`, `@dec` | `name!(...)`, `#[name]`, `#[derive(Name)]` |
| Generics | Erased at runtime | Monomorphized at compile time |
| Closest TS analogy | none is exact | decorators ≈ attributes (but compile-time) |

### Why Rust has macros at all

Rust's type system is strict and there is no runtime reflection (unlike, say, decorators inspecting metadata). Macros fill the gap that dynamic languages fill with runtime metaprogramming: removing boilerplate, building domain-specific syntax, and producing variadic constructs, all without paying a runtime price and without weakening type safety, because the *generated* code is type-checked exactly like hand-written code.

### Two families of macros

You will meet two kinds in this section:

1. **Declarative macros** (`macro_rules!`) — pattern-match on token trees and substitute. Great for "this expands to that" templates. Covered in [Declarative Macros](/14-macros/01-declarative-macros/), [Macro Patterns](/14-macros/02-macro-patterns/), and [Repetition](/14-macros/03-repetition/).
2. **Procedural macros** — small compiler plugins written in Rust that take a `TokenStream` and return one, typically using the `syn` and `quote` crates. These power custom `#[derive(...)]`, attribute macros, and function-like `foo!(...)` procedural macros. Covered in [Derive Macros](/14-macros/04-derive-macros/), [Attribute Macros](/14-macros/05-attribute-macros/), [Function-like Macros](/14-macros/06-function-like-macros/), and [Procedural Macros](/14-macros/07-proc-macros/).

---

## Common Pitfalls

### Pitfall 1: Forgetting the `!`

A macro invocation needs the bang. Without it, the compiler tries to parse a function call or expression and fails:

```rust
fn main() {
    let v = vec[1, 2, 3];   // does not compile — forgot the `!`
    println!("{v:?}");
}
```

Real compiler error:

```text
error: expected one of `.`, `?`, `]`, or an operator, found `,`
 --> src/main.rs:2:18
  |
2 |     let v = vec[1, 2, 3];   // forgot the !
  |                  ^ expected one of `.`, `?`, `]`, or an operator
```

The fix is `vec![1, 2, 3]`. The compiler saw `vec[...]` and tried to parse it as indexing into a thing named `vec`, hence the confusing message. A good reminder that *without the bang it is not a macro at all*.

### Pitfall 2: Expecting a runtime function to exist

Because a macro is gone after expansion, you cannot pass it as a value, store it in a variable, or use it as a callback the way you can a JavaScript function:

```rust
// does not compile — there is no `println` value to pass around.
// let f = println;   // error[E0423]: expected value, found macro `println`
```

A macro name on its own is not an expression. If you need first-class behavior, wrap the macro in a closure or function: `let f = |s: &str| println!("{s}");`.

### Pitfall 3: Assuming format/argument errors are caught at runtime

`println!` and friends validate their format string at **compile time**, a genuine advantage over `console.log` template strings:

```rust
fn main() {
    println!("{} and {}", 42);   // does not compile — 2 placeholders, 1 argument
}
```

Real compiler error:

```text
error: 2 positional arguments in format string, but there is 1 argument
 --> src/main.rs:2:15
  |
2 |     println!("{} and {}", 42);   // 2 placeholders, 1 argument
  |               ^^     ^^   --
```

In TypeScript, a malformed template only misbehaves at runtime, if at all.

### Pitfall 4: Typos surface as "cannot find macro"

Because macros are resolved by name during expansion, a typo gives a clear (and helpfully suggestive) error:

```rust
fn main() {
    primtln!("hello");   // does not compile — typo
}
```

Real compiler error:

```text
error: cannot find macro `primtln` in this scope
 --> src/main.rs:2:5
  |
2 |     primtln!("hello");   // typo
  |     ^^^^^^^ help: a macro with a similar name exists: `println`
```

### Pitfall 5: Reaching for a macro when a function would do

Macros are harder to read, harder to document, and produce worse IDE and error experiences than plain functions. The biggest *conceptual* mistake is treating them as the default tool. See Best Practices below.

---

## Best Practices

### Prefer a function unless you truly need a macro

If a regular function (possibly generic, possibly with a trait bound) can express it, use the function. Reach for a macro only when you need one of these things a function genuinely cannot provide:

1. **Variadic arguments** with mixed types — e.g. `println!`, `vec!`, `hashmap!`.
2. **New syntax / a mini-DSL** — e.g. building a routing table or a SQL-ish query block.
3. **Operating on the source itself** — capturing the *text* of an expression (`stringify!`), the current file and line (`file!`, `line!`), or generating trait `impl`s from a type definition (`#[derive(...)]`).
4. **Eliminating boilerplate** that would otherwise be copy-pasted across many types.

If none of those apply, a function is clearer, faster to compile, and friendlier to tooling.

### Keep macros small and well-documented

A macro's expansion is invisible at the call site, so document what it generates and give a worked example. When debugging, inspect the expansion with [`cargo expand`](https://github.com/dtolnay/cargo-expand) (`cargo install cargo-expand`, then `cargo expand`), which prints your code with all macros expanded — covered further in [Declarative Macros](/14-macros/01-declarative-macros/).

### Lean on the standard library's macros first

You rarely need to write a macro at all. `vec!`, `format!`, `assert_eq!`, `matches!`, `todo!`, `dbg!`, `include_str!`, and the rest cover a huge amount of ground; see [Common Macros](/14-macros/08-common-macros/) before writing your own.

### Trust hygiene, but pass identifiers explicitly when you need access

Let the macro invent its own temporaries freely; hygiene keeps them safe. When the macro must touch a caller's binding, take it as an argument (`$a:expr` / `$name:ident`) rather than hard-coding a name and hoping it matches.

---

## Real-World Example

A small, production-flavored logging macro that captures the *expression source* and the *current location* — two things a function literally cannot do, because by the time a function runs, the original source text and call site are gone. This mirrors the standard `dbg!` macro.

```rust
/// Logs an expression's source text, its file:line, and its value,
/// then returns the value so it can be used inline. Like a typed `console.log`
/// that also tells you WHERE and WHAT it logged — checked at compile time.
macro_rules! trace {
    ($e:expr) => {{
        let value = $e; // hygienic temporary; cannot clash with caller code
        eprintln!("[{}:{}] {} = {:?}", file!(), line!(), stringify!($e), &value);
        value
    }};
}

fn parse_port(raw: &str) -> u16 {
    // We can drop `trace!` around any sub-expression without changing behavior.
    trace!(raw.trim().parse::<u16>().unwrap_or(8080))
}

fn main() {
    let port = parse_port("  9000 ");
    println!("listening on port {port}");
}
```

Real output (the `trace!` line goes to stderr, the result to stdout):

```text
[src/main.rs:14] raw.trim().parse::<u16>().unwrap_or(8080) = 9000
listening on port 9000
```

The macro recorded the literal text `raw.trim().parse::<u16>().unwrap_or(8080)` via `stringify!`, the exact `file!`/`line!`, and the computed value, all resolved at compile time, with the temporary `value` kept hygienically separate from anything in `parse_port`. The standard library ships exactly this idea as `dbg!`:

```rust
fn main() {
    let n = 5;
    let doubled = dbg!(n * 2); // prints to stderr, returns the value
    println!("doubled = {doubled}");
}
```

Real output:

```text
[src/main.rs:3:19] n * 2 = 10
doubled = 10
```

Reach for `dbg!` (see [Common Macros](/14-macros/08-common-macros/)) before hand-rolling your own; the example above exists to show *why* this can only be a macro.

---

## Further Reading

### Official documentation

- [The Rust Book — Macros](https://doc.rust-lang.org/book/ch20-05-macros.html): declarative vs. procedural macros.
- [The Little Book of Rust Macros](https://veykril.github.io/tlborm/): the definitive in-depth guide to `macro_rules!`, hygiene, and patterns.
- [Reference — Macros](https://doc.rust-lang.org/reference/macros.html) and [Macro hygiene](https://doc.rust-lang.org/reference/macros-by-example.html#hygiene).
- [`std::dbg!`](https://doc.rust-lang.org/std/macro.dbg.html) and [`std::stringify!`](https://doc.rust-lang.org/std/macro.stringify.html).

### Related sections in this guide

- Next: [Declarative Macros](/14-macros/01-declarative-macros/): write your first `macro_rules!`.
- [Macro Patterns](/14-macros/02-macro-patterns/) — fragment specifiers (`:expr`, `:ident`, `:ty`, `:tt`, ...).
- [Repetition](/14-macros/03-repetition/) — `$(...),*` and building a `vec!`-like macro.
- [Derive Macros](/14-macros/04-derive-macros/) and [Attribute Macros](/14-macros/05-attribute-macros/) — why these are *not* decorators.
- [Procedural Macros](/14-macros/07-proc-macros/): `syn` 2 + `quote` and a real custom derive.
- [Common Macros](/14-macros/08-common-macros/) — the standard-library macros you will use daily.
- Background: [Output and Formatting](/02-basics/04-output/) introduced `println!`; [Getting Started](/01-getting-started/) and the [Introduction](/00-introduction/) set the stage.
- Looking ahead: derive macros power [Serialization](/15-serialization/) via `#[derive(Serialize, Deserialize)]`.

---

## Exercises

### Exercise 1

**Difficulty:** Easy

**Objective:** Confirm for yourself that a macro substitutes code rather than calling a function, and that fragment specifiers preserve grouping.

**Instructions:** Write a declarative macro `max2!` that takes two expressions and evaluates to the larger one. Invoke it as `max2!(3 + 4, 10)` and print the result. Predict the output before running.

```rust
macro_rules! max2 {
    // TODO: match two expressions and expand to an `if`/`else`
}

fn main() {
    println!("max2 = {}", max2!(3 + 4, 10));
}
```

<details>
<summary>Solution</summary>

```rust
macro_rules! max2 {
    ($a:expr, $b:expr) => {
        if $a >= $b { $a } else { $b }
    };
}

fn main() {
    // `3 + 4` is captured as one expression, so this compares 7 vs 10.
    println!("max2 = {}", max2!(3 + 4, 10)); // max2 = 10
}
```

Output:

```text
max2 = 10
```

</details>

### Exercise 2

**Difficulty:** Medium

**Objective:** Use `stringify!` to capture an expression's *source text* — something only a macro can do — and observe hygiene in action.

**Instructions:** Write a macro `show!` that takes one expression, prints it as `<source> = <value>` using `stringify!` and `{:?}`, and then *returns* the value so it can be used inline. Internally bind the value to a temporary named `val`; call `show!` from a `main` that also has its own `val` to prove the names do not collide.

```rust
macro_rules! show {
    // TODO: bind to a temporary, print `stringify!` of the expr, return the value
}

fn main() {
    let val = "untouched";
    let x = show!(2 * 21);
    println!("returned {x}, caller val = {val}");
}
```

<details>
<summary>Solution</summary>

```rust
macro_rules! show {
    ($e:expr) => {{
        let val = $e; // hygienic: does NOT clash with the caller's `val`
        println!("{} = {:?}", stringify!($e), val);
        val
    }};
}

fn main() {
    let val = "untouched";
    let x = show!(2 * 21);
    println!("returned {x}, caller val = {val}");
}
```

Output:

```text
2 * 21 = 42
returned 42, caller val = untouched
```

The macro's `val` and the caller's `val` are independent, demonstrating hygiene.

</details>

### Exercise 3

**Difficulty:** Hard

**Objective:** Build a variadic, compile-time construct that no single Rust function could express, using recursion and repetition.

**Instructions:** Write a macro `count!` that accepts any number of comma-separated expressions and expands to the number of arguments as a `usize`, fully at compile time. Handle the empty case `count!()` and a non-empty list. Verify `count!(10, 20, 30, 40)` is `4`.

> **Tip:** Define two rules: one for the empty input, one that peels off a head argument and recurses on the tail. Repetition syntax is detailed in [Repetition](/14-macros/03-repetition/).

```rust
macro_rules! count {
    // TODO: base case for `()`
    // TODO: recursive case `$head:expr $(, $tail:expr)*`
}

fn main() {
    println!("count = {}", count!(10, 20, 30, 40));
    println!("empty = {}", count!());
}
```

<details>
<summary>Solution</summary>

```rust
macro_rules! count {
    () => { 0usize };
    ($head:expr $(, $tail:expr)*) => {
        1usize + count!($($tail),*)
    };
}

fn main() {
    println!("count = {}", count!(10, 20, 30, 40)); // 4
    println!("empty = {}", count!());               // 0
}
```

Output:

```text
count = 4
empty = 0
```

Each expansion strips one argument and adds `1usize`, recursing until the empty rule terminates the chain. The entire count is computed during compilation; the binary just contains the constant.

</details>
