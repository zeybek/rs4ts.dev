---
title: "Declarative Macros with `macro_rules!`"
description: "Write macro_rules! macros that match Rust tokens and stamp out code at compile time, with hygiene and trailing commas, then view expansions with cargo expand."
---

A practical tour of Rust's declarative macros: how `macro_rules!` matches the tokens you pass it, how it stamps out code at compile time, and how to actually *see* what your macro expands into with `cargo expand`.

## Quick Overview

A **declarative macro** is a compile-time pattern-matcher over Rust source tokens: you give it patterns to match (matchers) and the code to generate for each (transcribers), and the compiler expands every call site before type checking and code generation. The result is real, type-checked Rust with **zero runtime cost**: there is no interpreter, no reflection, and nothing left over at runtime. For a TypeScript/JavaScript developer the closest mental hook is a code generator or a tagged template that produces source, except it runs *inside the compiler* and the output is fully checked.

> **Note:** This file covers the everyday `macro_rules!` form. The companion pages go deeper: see [Macro Patterns](/14-macros/02-macro-patterns/) for the full set of fragment specifiers, [Macro Repetition](/14-macros/03-repetition/) for the repetition operators, and [Macro Basics](/14-macros/00-macro-basics/) for the conceptual "what macros are and are NOT". The fundamentally different *procedural* macros (custom `#[derive]`, attributes) live in [Procedural Macros](/14-macros/07-proc-macros/).

## TypeScript/JavaScript Example

TypeScript/JavaScript has no language-level macro system. When you need to avoid repeating yourself, you reach for runtime helper functions, builder objects, or, at the extreme, code generation as a build step. Here is the kind of boilerplate a macro would later replace, written the way you would actually write it in TypeScript:

```typescript
// A small helper that builds a Map from key/value pairs.
// This is a *runtime* function: it runs every time the program executes.
function makeMap<K, V>(pairs: Array<[K, V]>): Map<K, V> {
  const map = new Map<K, V>();
  for (const [key, value] of pairs) {
    map.set(key, value);
  }
  return map;
}

const scores = makeMap<string, number>([
  ["alice", 95],
  ["bob", 87],
  ["carol", 92],
]);

// A logging wrapper — also a runtime function call.
function logInfo(...args: unknown[]): void {
  console.log("[INFO] ", ...args);
}

const user = "alice";
logInfo(`user ${user} signed in`); // [INFO]  user alice signed in
```

Two things to notice, because they are exactly where Rust macros differ:

1. `makeMap` and `logInfo` are ordinary functions. They exist at runtime, take values, and return values.
2. The `[...]` literal and the template string `` `user ${user}...` `` are the only *syntactic* shortcuts the language gives you, and they are fixed. You cannot define your own `myliteral { ... }` syntax.

## Rust Equivalent

A declarative macro lets you define new *syntax* that expands into code. Below, `hashmap!` accepts `key => value` pairs and expands — at compile time — into a sequence of `insert` calls. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically, and `macro_rules!` works on every edition.

```rust playground
use std::collections::HashMap;

// Define a macro that builds a HashMap from `key => value` pairs.
macro_rules! hashmap {
    // One matcher: a comma-separated list of `key => value` pairs.
    // `$(,)?` allows an optional trailing comma, just like a Rust array literal.
    ( $( $key:expr => $value:expr ),* $(,)? ) => {{
        let mut map = HashMap::new();
        $(
            map.insert($key, $value);
        )*
        map
    }};
}

fn main() {
    let scores = hashmap! {
        "alice" => 95,
        "bob"   => 87,
        "carol" => 92,
    };

    let mut keys: Vec<_> = scores.keys().collect();
    keys.sort();
    for k in keys {
        println!("{k}: {}", scores[k]);
    }
}
```

Running it:

```
alice: 95
bob: 87
carol: 92
```

The `!` after `hashmap` is the giveaway that you are calling a macro, not a function: the same `!` you already use with `println!` and `vec!`. Unlike the TypeScript `makeMap`, there is **no `makeMap` left in the compiled binary**: by the time the program runs, `hashmap! { ... }` has already become three `map.insert(...)` lines.

## Detailed Explanation

A `macro_rules!` definition is a set of **rules**, each of the form `(matcher) => {transcriber};`. The compiler tries the rules top to bottom; the first matcher whose pattern fits the tokens at the call site wins, and its transcriber is substituted in place of the call.

### Anatomy of the simplest macro

```rust playground
// A macro with no arguments: it always expands to the same fixed block.
macro_rules! greet {
    () => {
        println!("Hello from a macro!");
    };
}

// A macro that captures one expression.
macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}

fn main() {
    greet!(); // expands to: println!("Hello from a macro!");

    let n = 5;
    println!("{}", square!(n));     // expands to: n * n      -> 25
    println!("{}", square!(2 + 1)); // expands to: (2 + 1) * (2 + 1) -> 9
}
```

Output:

```
Hello from a macro!
25
9
```

Walking through the pieces:

- **`macro_rules! square { ... }`** declares a macro named `square`.
- **`($x:expr)`** is the matcher. `$x` is a **metavariable** (the `$` marks it), and `:expr` is its **fragment specifier**, meaning "match a complete Rust expression here". When you call `square!(2 + 1)`, the metavariable `$x` is bound to the expression `2 + 1`.
- **`=> { $x * $x }`** is the transcriber. Every occurrence of `$x` is replaced by the matched fragment, so the call expands to `$x * $x`.

> **Note:** The `2 + 1` result of `9` (not `5`) is important and surprising if you come from C-style text macros. A fragment captured as `:expr` is stored as a **single parsed expression node**, not as loose tokens. When it is substituted back in it behaves as if parenthesized, so `square!(2 + 1)` is `(2 + 1) * (2 + 1)`, never `2 + 1 * 2 + 1`. Rust macros operate on the abstract syntax tree, not on raw text. This is one of the biggest differences from C's preprocessor.

### The repetition in `hashmap!`

The `hashmap!` macro used two repetition constructs:

- **In the matcher:** `$( $key:expr => $value:expr ),*` means "match zero or more `key => value` groups, separated by commas". The `*` is the repetition count (zero-or-more).
- **In the transcriber:** `$( map.insert($key, $value); )*` repeats the `insert` line once per group that was matched, reusing the `$key` and `$value` captured each time.

So the input

```rust
hashmap! { "alice" => 95, "bob" => 87 }
```

expands to roughly:

```rust
{
    let mut map = HashMap::new();
    map.insert("alice", 95);
    map.insert("bob", 87);
    map
}
```

Repetition has its own dedicated page — see [Macro Repetition](/14-macros/03-repetition/) for `*`, `+`, `?`, and the different separators — so this file keeps to the minimum needed to read the examples.

### Why the doubled braces `{{ ... }}`?

In `hashmap!` the transcriber is wrapped in `{{ ... }}`. The outer braces are macro-syntax punctuation; the inner braces make the expansion a **block expression** that evaluates to a value (the `map`). Without the inner block, `let scores = hashmap! { ... };` would not have a single expression to bind. This is the standard idiom for a macro that "returns" something.

### Hygiene: macro-introduced names do not leak

Rust macros are **hygienic**: identifiers a macro introduces internally live in their own syntactic context and cannot accidentally collide with the caller's variables.

```rust playground
macro_rules! double_it {
    ($x:expr) => {{
        let result = $x * 2; // `result` here is the macro's own, hygienic binding
        result
    }};
}

fn main() {
    let result = 100; // the caller has a variable *also* named `result`
    let doubled = double_it!(21);
    // The macro's internal `result` did NOT overwrite the caller's `result`:
    println!("caller result = {result}, doubled = {doubled}");
}
```

Output:

```
caller result = 100, doubled = 42
```

In a C-style textual macro this would be a classic bug (variable capture). In Rust the two `result`s are genuinely distinct. Hygiene is covered more fully in [Macro Basics](/14-macros/00-macro-basics/).

## Key Differences

| Aspect | TypeScript/JavaScript helper | Rust `macro_rules!` |
| --- | --- | --- |
| When it runs | At runtime, every call | At compile time, expanded once per call site |
| What it operates on | Runtime values | Source tokens / AST fragments |
| Cost in the binary | A real function in the bundle | Nothing — only the expanded code remains |
| Can it invent syntax? | No (only literals/template strings) | Yes: `mymacro! { a => b, ... }` |
| Type checking | At runtime (or via TypeScript before transpile) | After expansion, by the normal Rust compiler |
| Variable capture | Possible if you reuse names | Prevented by hygiene |
| Argument arity | Fixed (or `...rest`) | Patterns can match variable shapes and counts |
| Error timing | Mostly runtime | Compile time (a malformed call fails to build) |

The headline difference: a TypeScript helper is a *value-level* abstraction; a Rust macro is a *syntax-level* abstraction. A macro can do things a function cannot (accept a variable number of differently-typed arguments, take a block of code, or generate `let` bindings) precisely because it works on syntax before types exist.

> **Warning:** A macro is *not* a decorator, and `macro_rules!` is *not* a function with weird syntax. If you only need to abstract over values, write a function. It is simpler, easier to read, and gives better error messages. Reach for a macro only when you genuinely need new syntax, variadic/heterogeneous arguments, or compile-time code generation. The "when to reach for a macro" decision is discussed in [Macro Basics](/14-macros/00-macro-basics/).

## Common Pitfalls

### Pitfall 1: Calling a macro before it is defined

Unlike functions, `macro_rules!` macros are **textually scoped**: a call only sees macros defined *earlier* in the source (or imported). This trips up developers used to JavaScript function hoisting.

```rust
fn main() {
    greet!(); // does not compile: macro used before its definition
}

macro_rules! greet {
    () => { println!("hi"); };
}
```

The real compiler error:

```
error: cannot find macro `greet` in this scope
 --> src/main.rs:2:5
  |
2 |     greet!(); // does not compile: macro used before its definition
  |     ^^^^^ consider moving the definition of `greet` before this call
  |
note: a macro with the same name exists, but it appears later
 --> src/main.rs:5:14
  |
5 | macro_rules! greet {
  |              ^^^^^
```

**Fix:** move the `macro_rules!` definition above its first use. To use a macro from other modules or crates, annotate it with `#[macro_export]` (which lifts it to the crate root) and import it like any other item — see [Modules & Packages](/12-modules-packages/).

### Pitfall 2: Passing tokens that do not match the fragment specifier

If the call site does not match any matcher, you get a compile error pointing at the exact token that broke the match.

```rust
macro_rules! only_ident {
    ($name:ident) => {
        let $name = 1;
    };
}

fn main() {
    only_ident!(x);     // fine: `x` is an identifier
    only_ident!(1 + 2); // does not compile: `1 + 2` is not an `ident`
}
```

The real compiler error:

```
error: no rules expected `1`
 --> src/main.rs:9:17
  |
1 | macro_rules! only_ident {
  | ----------------------- when calling this macro
...
9 |     only_ident!(1 + 2); // does not compile: `1 + 2` is not an `ident`
  |                 ^ no rules expected this token in macro call
  |
note: while trying to match meta-variable `$name:ident`
 --> src/main.rs:2:6
  |
2 |     ($name:ident) => {
  |      ^^^^^^^^^^^
```

**Fix:** pick the fragment specifier that matches what callers will actually pass (`:expr` for arbitrary expressions, `:ident` only for plain names). The full list is in [Macro Patterns](/14-macros/02-macro-patterns/).

### Pitfall 3: Reaching for a macro when a function would do

```rust
// Over-engineered: a macro that only ever forwards two values.
macro_rules! add {
    ($a:expr, $b:expr) => { $a + $b };
}

// A plain generic function is clearer, type-checked at definition, and
// shows up in IDE autocompletion and docs.
fn add<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
    a + b
}
```

Both compile, but the function is the right tool. Macros sacrifice readability and tooling support; only pay that price when you need syntax a function cannot express.

### Pitfall 4: Forgetting that `expr` fragments are atomic, then "fixing" it the wrong way

Because `:expr` fragments behave as if parenthesized, you do **not** need to wrap them defensively for precedence the way you would in a C macro. Adding `($x)` is harmless but redundant for `:expr`. The trap is assuming the *opposite* (that `square!(2 + 1)` is `5`) and writing tests against the wrong expected value. It is `9`.

## Best Practices

- **Prefer a function unless you need syntax.** This is the single most important rule. Macros are a last resort, not a first reach.
- **Support an optional trailing comma** with `$(,)?` in list-like macros, so callers can format multi-line invocations the way `vec!`, arrays, and struct literals allow.
- **Wrap value-producing expansions in a block** (`{{ ... }}`) so the macro can be used wherever an expression is expected.
- **Name fragments descriptively** (`$key`, `$value`, `$body`) rather than `$x`, `$y`. The matcher is documentation.
- **Forward formatting macros transparently** with `$($arg:tt)*`. A logging wrapper should accept exactly what `println!` accepts:

  ```rust playground
  macro_rules! log_info {
      ($($arg:tt)*) => {{
          print!("[INFO]  ");
          println!($($arg)*);
      }};
  }

  fn main() {
      let user = "alice";
      log_info!("user {user} signed in");      // inline capture works
      log_info!("retried {} times", 3);        // positional args work
  }
  ```

  Output:

  ```
  [INFO]  user alice signed in
  [INFO]  retried 3 times
  ```

- **Inspect expansions with `cargo expand`** while developing; see the next section. It turns "what does this even generate?" into a one-command answer.
- **Export deliberately.** Only add `#[macro_export]` when a macro is part of your crate's public surface; otherwise keep it module-local.

### Seeing the expansion with `cargo expand`

`cargo expand` is a developer tool (a Cargo subcommand) that prints your code *after* all macros have been expanded. It is the macro author's best friend. Install it once and run it from any project:

```bash
# Install the subcommand (built on top of cargo; network access required).
cargo install cargo-expand

# Print the whole crate, fully macro-expanded.
cargo expand
```

> **Tip:** `cargo expand` invokes the nightly compiler's unstable pretty-printer under the hood, so it needs a nightly toolchain available (`rustup toolchain install nightly`). Your *project* still builds on stable; only the expansion view uses nightly.

Given the `main` from earlier, `cargo expand` shows it desugar to (prelude header kept for context):

```rust
#![feature(prelude_import)]
#[macro_use]
extern crate std;
#[prelude_import]
use std::prelude::rust_2024::*;
fn main() {
    {
        ::std::io::_print(format_args!("Hello from a macro!\n"));
    };
    let n = 5;
    {
        ::std::io::_print(format_args!("{0}\n", n * n));
    };
    {
        ::std::io::_print(format_args!("{0}\n", (2 + 1) * (2 + 1)));
    };
}
```

Two lessons jump out. First, `square!(2 + 1)` expanded to `(2 + 1) * (2 + 1)`: concrete proof that the `:expr` fragment was treated as one parenthesized unit. Second, even `println!` is itself a macro: it expanded into a `format_args!` call routed through `::std::io::_print`. Macros expand recursively until only plain Rust remains. (The built-in standard-library macros, including `println!`, `format!`, and `vec!`, are catalogued in [Common Standard-Library Macros](/14-macros/08-common-macros/).)

For the `hashmap!` macro, the body of `main` expands to exactly the unrolled inserts:

```rust
fn main() {
    let scores = {
        let mut map = HashMap::new();
        map.insert("alice", 95);
        map.insert("bob", 87);
        map
    };
    {
        ::std::io::_print(format_args!("{0}\n", scores.len()));
    };
}
```

## Real-World Example

A common production need is lightweight instrumentation: log a message at a level, and time how long a block of work takes. In TypeScript you would write helper functions and pass a callback. In Rust a small set of declarative macros gives you cleaner call sites and zero indirection: the timing code is inlined directly where you use it.

```rust playground
use std::time::Instant;

// Structured logging macros. `$($arg:tt)*` captures *all* the remaining tokens,
// so these forward the full `println!`/`eprintln!` argument syntax unchanged.
macro_rules! log_info {
    ($($arg:tt)*) => {{
        print!("[INFO]  ");
        println!($($arg)*);
    }};
}

macro_rules! log_error {
    ($($arg:tt)*) => {{
        eprint!("[ERROR] ");
        eprintln!($($arg)*);
    }};
}

// `time_it!` takes a label and a *block*, runs the block, reports the elapsed
// time, and evaluates to the block's value. A function cannot take a block of
// statements as an argument the way this matcher (`$body:block`) can.
macro_rules! time_it {
    ($label:expr, $body:block) => {{
        let start = Instant::now();
        let result = $body;
        log_info!("{} took {:?}", $label, start.elapsed());
        result
    }};
}

fn main() {
    let user = "alice";
    log_info!("user {user} signed in");
    log_error!("failed after {} retries", 3);

    // `time_it!` evaluates to whatever the block returns.
    let sum = time_it!("summation", {
        (1..=1_000u64).sum::<u64>()
    });

    log_info!("sum = {sum}");
}
```

Output (the exact timing varies run to run):

```
[INFO]  user alice signed in
[ERROR] failed after 3 retries
[INFO]  summation took 7.333µs
[INFO]  sum = 500500
```

This is the legitimate use case for declarative macros: `time_it!` needs to *wrap a block of code* and reuse it, which no function signature can express, and the logging macros need to forward arbitrary format arguments. The expansions are inlined, so there is no closure allocation or call overhead at the timed site.

> **Note:** For real applications, prefer the ecosystem's [`log`](https://docs.rs/log) or [`tracing`](https://docs.rs/tracing) crates rather than hand-rolled logging macros; they handle levels, filtering, and structured fields. The example above is to demonstrate the macro mechanics, not to reinvent logging.

## Further Reading

- [Macros chapter — *The Rust Programming Language*](https://doc.rust-lang.org/book/ch20-05-macros.html) — the official book's macro overview.
- [Macros By Example — *The Rust Reference*](https://doc.rust-lang.org/reference/macros-by-example.html) — the precise grammar of `macro_rules!`, matchers, and repetition.
- [*The Little Book of Rust Macros*](https://veykril.github.io/tlborm/) — a community deep look at declarative-macro techniques.
- [`cargo expand` on crates.io](https://crates.io/crates/cargo-expand) — the expansion-viewing subcommand used above.
- Sibling pages in this section: [Macro Basics](/14-macros/00-macro-basics/) (what macros are and are not, hygiene, when to use them), [Macro Patterns](/14-macros/02-macro-patterns/) (fragment specifiers and multiple rules), [Macro Repetition](/14-macros/03-repetition/) (the repetition operators and a `vec!`-style macro), and [Common Standard-Library Macros](/14-macros/08-common-macros/) (the standard-library macros).
- For *procedural* macros — custom `#[derive]`, attribute, and function-like macros built with `syn` and `quote` — see [Procedural Macros](/14-macros/07-proc-macros/), [Derive Macros](/14-macros/04-derive-macros/), [Attribute Macros](/14-macros/05-attribute-macros/), and [Function-Like Procedural Macros](/14-macros/06-function-like-macros/).
- Foundations referenced here: [output and the `println!` family](/02-basics/04-output/), [getting started](/01-getting-started/), [the introduction](/00-introduction/), and — for how derive macros power serialization with `serde` — [Serialization](/15-serialization/).

## Exercises

### Exercise 1: A `triple!` macro

**Difficulty:** Easy

**Objective:** Write your first single-rule macro and confirm you understand that `:expr` fragments are atomic.

**Instructions:** Define a macro `triple!` that takes one expression and produces three times its value. Verify that `triple!(4)` is `12` and that `triple!(2 + 1)` is `9` (not `7`), proving the fragment is treated as a unit.

```rust
macro_rules! triple {
    // TODO: match one expression and multiply it by 3
}

fn main() {
    assert_eq!(triple!(4), 12);
    assert_eq!(triple!(2 + 1), 9);
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust playground
macro_rules! triple {
    ($x:expr) => {
        ($x) * 3
    };
}

fn main() {
    assert_eq!(triple!(4), 12);
    assert_eq!(triple!(2 + 1), 9); // (2 + 1) * 3 == 9
    println!("ok");
}
```

The parentheses around `$x` are not strictly required here because `:expr` fragments are already atomic, but they make the precedence obvious to a reader. This compiles and prints `ok`.

</details>

### Exercise 2: A `max_of!` macro

**Difficulty:** Medium

**Objective:** Practice matching two metavariables and generating an expression that uses both.

**Instructions:** Define `max_of!(a, b)` that expands to an expression evaluating to the larger of the two values. It should work for any type that supports `>=`. Confirm `max_of!(3, 7) == 7` and `max_of!(10, -4) == 10`.

```rust
macro_rules! max_of {
    // TODO: match two expressions; expand to the larger one
}

fn main() {
    assert_eq!(max_of!(3, 7), 7);
    assert_eq!(max_of!(10, -4), 10);
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust playground
macro_rules! max_of {
    ($a:expr, $b:expr) => {
        if $a >= $b { $a } else { $b }
    };
}

fn main() {
    assert_eq!(max_of!(3, 7), 7);
    assert_eq!(max_of!(10, -4), 10);
    println!("ok");
}
```

This compiles and prints `ok`. Note that because `$a` and `$b` are substituted directly into both branches of the `if`, an argument with side effects would be evaluated more than once — a real consideration for macros (and a reason `std::cmp::max` exists as a function for the common case).

</details>

### Exercise 3: A `debug_vars!` macro using repetition and `stringify!`

**Difficulty:** Hard

**Objective:** Combine repetition with the built-in `stringify!` macro to print a variable number of variables as `name = value` lines.

**Instructions:** Define `debug_vars!` that accepts a comma-separated list of identifiers (with an optional trailing comma) and, for each one, prints `name = value` where `value` uses debug formatting (`{:?}`). Each variable must already be in scope. Calling `debug_vars!(count, label, ratio)` should print one line per variable.

```rust
macro_rules! debug_vars {
    // TODO: match zero-or-more identifiers, allow a trailing comma,
    //       and println! each as "name = value"
}

fn main() {
    let count = 42;
    let label = "active";
    let ratio = 0.75;
    debug_vars!(count, label, ratio);
}
```

<details>
<summary>Solution</summary>

```rust playground
macro_rules! debug_vars {
    ( $( $name:ident ),* $(,)? ) => {
        $(
            println!("{} = {:?}", stringify!($name), $name);
        )*
    };
}

fn main() {
    let count = 42;
    let label = "active";
    let ratio = 0.75;
    debug_vars!(count, label, ratio);
}
```

Output:

```
count = 42
label = "active"
ratio = 0.75
```

The key trick is `stringify!($name)`, a built-in macro that turns the *identifier token* `count` into the string literal `"count"` at compile time. There is no runtime reflection involved. The `$( ... ),*` repetition matches the list, and the matching `$( println!(...); )*` emits one line per identifier. See [Common Standard-Library Macros](/14-macros/08-common-macros/) for `stringify!` and friends, and [Macro Repetition](/14-macros/03-repetition/) for more on the repetition operators.

</details>
