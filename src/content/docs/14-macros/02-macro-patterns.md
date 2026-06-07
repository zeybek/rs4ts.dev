---
title: "Macro Patterns: Fragment Specifiers and Multiple Rules"
description: "Match meaningfully in macro_rules! with fragment specifiers like :expr, :ident, :ty, and :tt, plus multiple rules for the shape-based overloading Rust"
---

Once you can write a basic `macro_rules!` macro, the next step is matching *meaningfully* on what the caller passed. This page is about the two tools that do the heavy lifting in `macro_rules!`: **fragment specifiers** (telling Rust *what kind* of syntax each captured piece is) and **multiple rules** (overload-like dispatch on the shape of the input).

---

## Quick Overview

A `macro_rules!` macro is a set of pattern-matching rules over Rust **syntax**, not over runtime values. Each rule has a **matcher** (the pattern) and a **transcriber** (the code it expands to). Inside a matcher you capture pieces of input into **metavariables** like `$x`, and you must tell the compiler what *category* of syntax each one is using a **fragment specifier** such as `:expr`, `:ident`, `:ty`, `:pat`, or `:tt`. A single macro can hold several rules and tries them top-to-bottom, which gives you arity- and shape-based overloading that ordinary Rust functions cannot express.

> **Note:** This page covers matchers in detail. For *what a macro is* (and is not — it is **not** a decorator or a function), see [Macro Basics](/14-macros/00-macro-basics/). For the `macro_rules!` syntax itself, see [Declarative Macros](/14-macros/01-declarative-macros/). For the `$(...)*` repetition operator that pairs with these specifiers, see [Repetition](/14-macros/03-repetition/). The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically.

---

## TypeScript/JavaScript Example

TypeScript and JavaScript have **no compile-time macro system**. There is nothing that inspects your *syntax* and rewrites it before compilation. When a JS/TS developer wants "the same call to behave differently depending on its arguments," they reach for one of three runtime tricks: variadic functions with `arguments`/rest params, runtime branching on `typeof`/`arguments.length`, or — the closest *syntactic* analog — **tagged template literals**.

```javascript
// macro-like-helpers.mjs — the JS toolbox that stands in for "macros"

// 1. Variadic + runtime branching on argument count/type.
//    This is "overloading" but it happens at RUNTIME, with type checks.
function min(...nums) {
  return nums.reduce((a, b) => (a < b ? a : b));
}

// 2. A factory that builds a config value at RUNTIME. There is no static
//    type attached to the result — TypeScript types are erased before this runs.
function makeConfig(key, fallback) {
  const raw = process.env[key];
  const parsed = raw === undefined ? fallback : Number(raw);
  return Number.isFinite(parsed) ? parsed : fallback;
}

// 3. A tagged template literal — the nearest JS thing to "macro-ish syntax".
//    `strings` and `values` are pulled apart by the runtime, then recombined.
function sql(strings, ...values) {
  return strings.reduce(
    (acc, s, i) => acc + s + (i < values.length ? `$${i + 1}` : ""),
    "",
  );
}

console.log("min =", min(8, 3, 5, 1, 9));
console.log("port =", makeConfig("PORT", 8080));
const table = "users",
  id = 7;
console.log(sql`SELECT * FROM ${table} WHERE id = ${id}`);
```

Running this under Node v22 prints:

```text
min = 1
port = 8080
SELECT * FROM $1 WHERE id = $2
```

Every one of these dispatches happens **while the program runs**. `min` re-checks `arguments` on each call; `makeConfig` parses strings at runtime; the `sql` tag receives already-evaluated values. By the time any of this executes, the TypeScript types are gone; generics are erased. A Rust macro does the *opposite*: it dispatches on syntax categories *before* the program is compiled, and produces real, type-checked code with zero runtime dispatch cost.

---

## Rust Equivalent

Here is a small request-router builder. It uses two **rules** (overloading on shape) and four different **fragment specifiers**: `:literal`, `:expr`, and `:ident`, plus the `stringify!` helper.

```rust
// A small request-routing macro showing several fragment specifiers and
// multiple rules selected by the SHAPE of the input.
macro_rules! route {
    // Rule 1: a literal `GET` keyword, a string-literal path, then a handler expression.
    (GET $path:literal => $handler:expr) => {
        Route { method: "GET", path: $path, handler: $handler }
    };
    // Rule 2: any method captured as an identifier (POST, PUT, DELETE, ...).
    ($method:ident $path:literal => $handler:expr) => {
        Route { method: stringify!($method), path: $path, handler: $handler }
    };
}

struct Route {
    method: &'static str,
    path: &'static str,
    handler: fn() -> String,
}

fn list_users() -> String { "200 OK: users".to_string() }
fn create_user() -> String { "201 Created".to_string() }

fn main() {
    let routes = [
        route!(GET "/users" => list_users),
        route!(POST "/users" => create_user),
    ];
    for r in &routes {
        println!("{} {} -> {}", r.method, r.path, (r.handler)());
    }
}
```

Output:

```text
GET /users -> 200 OK: users
POST /users -> 201 Created
```

The compiler picks Rule 1 for `route!(GET ...)` because the bare token `GET` matches the literal `GET` in the first matcher. For `route!(POST ...)` the first rule fails (the token is not literally `GET`), so it falls through to Rule 2, which captures `POST` as an `:ident` and turns it into the string `"POST"` with `stringify!`. No runtime dispatch happens: by the time `main` runs, each `route!(...)` is already a plain `Route { .. }` struct literal.

---

## Detailed Explanation

### Why specifiers exist at all

Rust parses your program into an abstract syntax tree. A macro matcher does not see characters — it sees a stream of **token trees**. When you write `$x` in a matcher, the compiler needs to know how much of the token stream to consume and how to validate it. That is the job of the fragment specifier after the colon: `$x:expr` says "parse one *expression* here," `$name:ident` says "parse one *identifier* here," and so on. This is fundamentally different from a C-style textual macro, which just pastes characters.

A captured fragment is treated as a **single opaque syntax node** in the output. This is the property that makes `:expr` safe:

```rust
macro_rules! square_expr {
    ($x:expr) => { $x * $x };
}

fn main() {
    // `2 + 3` is captured as ONE expression node, so this is (2+3)*(2+3) = 25,
    // NOT 2 + 3 * 2 + 3 = 11 (which is what a C-style textual macro would give).
    println!("square_expr!(2 + 3) = {}", square_expr!(2 + 3));
}
```

Output:

```text
square_expr!(2 + 3) = 25
```

Even better: a macro **invocation in expression position is itself a single node**. So when you embed a macro call inside a larger expression, the whole expansion is grouped:

```rust
macro_rules! add_expr {
    ($x:expr) => { $x + 1 };
}

fn main() {
    // `add_expr!(2)` expands to the single node `(2 + 1)`, so this is 3 * 3 = 9,
    // NOT the textual `3 * 2 + 1` = 7.
    println!("{}", 3 * add_expr!(2));
}
```

Output:

```text
9
```

> **Tip:** Because `:expr` and macro-invocation-in-expression-position are atomic, you rarely need to add parentheses for the *captured* fragments. You still might choose to parenthesize the *whole transcriber* (`{ ($x * $x) }`) for defensive clarity, but the `9` above shows Rust already groups the invocation for you.

### The fragment specifiers you will actually use

| Specifier    | Captures                                  | Example input              |
| ------------ | ----------------------------------------- | -------------------------- |
| `:expr`      | An expression                             | `2 + 3`, `foo()`, `vec![1]`|
| `:ident`     | An identifier or keyword                  | `count`, `String`, `GET`   |
| `:ty`        | A type                                    | `u32`, `Vec<String>`, `&str`|
| `:pat`       | A pattern (as in `match`/`let`)           | `Some(x)`, `1..=10`, `_`   |
| `:literal`   | A literal value                           | `42`, `"hi"`, `3.14`, `true`|
| `:path`      | A path                                    | `std::cmp::max`, `Option::None`|
| `:block`     | A brace-delimited block                   | `{ let x = 1; x + 1 }`     |
| `:stmt`      | A statement                               | `let x = 1`, `foo()`       |
| `:tt`        | A single **token tree** (most flexible)   | `+`, `foo`, `(a, b)`, `{ .. }`|
| `:meta`      | The contents of an attribute              | `derive(Debug)`, `cfg(test)`|
| `:vis`       | A visibility qualifier (may be empty)     | `pub`, `pub(crate)`, *(nothing)*|
| `:lifetime`  | A lifetime                                | `'a`, `'static`            |
| `:item`      | A whole item                              | `fn f() {}`, `struct S;`   |

> **Note:** `:pat` in the 2021 and 2024 editions also matches *or-patterns* like `A | B`. The older, single-alternative form is available as `:pat_param` when you need to forbid top-level `|`. For everyday macros, `:pat` is what you want.

Here are the most common ones exercised in one program:

```rust
use std::collections::HashMap;

// :ident + :ty + :expr together — declare a typed constant.
macro_rules! declare_const {
    ($name:ident: $ty:ty = $value:expr) => {
        const $name: $ty = $value;
    };
}
declare_const!(MAX_RETRIES: u32 = 5);

// :pat — build a `matches!`-style check.
macro_rules! is_some_of {
    ($value:expr, $pat:pat) => {
        matches!($value, Some($pat))
    };
}

// :literal — only accepts literals, never variables or expressions.
macro_rules! describe_literal {
    ($l:literal) => { format!("literal: {}", $l) };
}

// :tt — captures any single token tree; used here to count tokens recursively.
macro_rules! count_tts {
    () => { 0 };
    ($head:tt $($rest:tt)*) => { 1 + count_tts!($($rest)*) };
}

// :block — captures a `{ ... }` block and runs it twice.
macro_rules! run_twice {
    ($b:block) => {{ $b $b }};
}

// :path — captures a path and calls it.
macro_rules! call_path {
    ($p:path, $arg:expr) => { $p($arg) };
}

fn double(n: i32) -> i32 { n * 2 }

fn main() {
    println!("MAX_RETRIES = {}", MAX_RETRIES);
    println!("is_some_of(Some(7), 1..=10) = {}", is_some_of!(Some(7i32), 1..=10));
    println!("{}", describe_literal!(42));
    println!("{}", describe_literal!("hi"));
    println!("count_tts!(a b c) = {}", count_tts!(a b c));

    let mut counter = 0;
    run_twice!({ counter += 1; });
    println!("counter after run_twice = {}", counter);

    println!("call_path double(21) = {}", call_path!(double, 21));

    // Avoid an unused-variable warning by reading the HashMap import in scope.
    let _seen: HashMap<&str, i32> = HashMap::new();
}
```

Output:

```text
MAX_RETRIES = 5
is_some_of(Some(7), 1..=10) = true
literal: 42
literal: hi
count_tts!(a b c) = 3
counter after run_twice = 2
call_path double(21) = 42
```

### `:expr` vs `:tt` — the central trade-off

`:expr` is **strict**: it parses and validates a full expression, and the result is opaque. `:tt` is **maximally permissive**: it grabs a single token tree (one token, or a balanced `(...)`/`[...]`/`{...}` group) without interpreting it. You reach for `:tt` when you need to:

- accept syntax that does not fit a single fragment category (e.g. arbitrary tokens to forward elsewhere),
- recurse over an unknown token stream (the `count_tts!` example above), or
- forward tokens verbatim to another macro.

The cost of `:tt` is that you lose the parser's help: nothing checks that the tokens form a valid expression until the *expansion* is re-parsed. For anything that *is* an expression, prefer `:expr`. You get better error messages and grouping for free.

### Multiple rules: overloading by shape

A `macro_rules!` macro is an ordered list of `(matcher) => { transcriber };` arms. The compiler tries them **top to bottom** and uses the **first** matcher that fits the entire input. This is how you emulate function overloading, something Rust functions cannot do:

```rust
macro_rules! greet {
    () => {
        String::from("Hello, world!")
    };
    ($name:expr) => {
        format!("Hello, {}!", $name)
    };
    ($greeting:expr, $name:expr) => {
        format!("{}, {}!", $greeting, $name)
    };
}

fn main() {
    println!("{}", greet!());
    println!("{}", greet!("Ada"));
    println!("{}", greet!("Hi there", "Ada"));
}
```

Output:

```text
Hello, world!
Hello, Ada!
Hi there, Ada!
```

Because order matters, put **more specific** rules first. In the `route!` example, the `GET` literal rule must come before the general `$method:ident` rule. If the `:ident` rule were first, it would happily capture `GET` as an identifier and the specific rule would never fire.

---

## Key Differences

| Concept                | TypeScript/JavaScript                              | Rust `macro_rules!`                                  |
| ---------------------- | -------------------------------------------------- | ---------------------------------------------------- |
| When dispatch happens  | Runtime (`typeof`, `arguments.length`)             | Compile time, on syntax categories                   |
| What is matched        | Runtime values                                     | Token trees / syntax fragments                       |
| "Overloading"          | Not real; one function body branches at runtime    | Multiple rules, first match wins, zero runtime cost  |
| Type information       | Erased before execution                            | Fully present; expansion is type-checked normally    |
| Grouping/precedence    | N/A (values already evaluated)                     | Each `:expr` / invocation is one atomic node         |
| Closest syntactic kin  | Tagged template literals                           | Fragment specifiers + matchers                       |
| Failure mode           | Throws at runtime, or silently wrong               | Compile error before the program ever runs           |

**Unlike TypeScript**, where a "polymorphic" function inspects its arguments while running, a Rust macro has *already chosen* a rule and produced concrete code by the time compilation finishes. There is no `arguments` object, no runtime type check, and no per-call overhead. The macro is gone after expansion.

**Unlike a JavaScript variadic function**, Rust macro rules can require *categories* of syntax. `declare_const!` literally cannot be called with `1 + 1` where it expects an `:ident`; the compiler rejects it. JavaScript would accept anything and fail (or misbehave) later.

---

## Common Pitfalls

### Pitfall 1: A fragment is followed by a disallowed token

After certain fragment specifiers, only specific tokens may appear. In particular, an `:expr` fragment may only be followed by `=>`, `,`, or `;`. This **follow-set** rule prevents ambiguous parsing. Writing `[` after an `:expr` is a hard error:

```rust
macro_rules! bad_follow {
    ($e:expr [ $i:expr ]) => { $e[$i] }; // does not compile
}

fn main() {
    let arr = [10, 20, 30];
    println!("{}", bad_follow!(arr [1]));
}
```

The real compiler error:

```text
error: `$e:expr` is followed by `[`, which is not allowed for `expr` fragments
 --> src/main.rs:2:14
  |
2 |     ($e:expr [ $i:expr ]) => { $e[$i] };
  |              ^ not allowed after `expr` fragments
  |
  = note: allowed there are: `=>`, `,` or `;`
```

**Fix:** Capture the indexable thing as a `:tt` (or restructure the matcher to separate the pieces with an allowed delimiter such as a comma), then build the index expression in the transcriber. Here `:tt` accepts the single identifier `arr` and the parser is satisfied:

```rust
macro_rules! index {
    ($container:tt [ $i:expr ]) => { $container[$i] };
}

fn main() {
    let arr = [10, 20, 30];
    println!("{}", index!(arr[1])); // prints 20
}
```

### Pitfall 2: Passing the wrong category of syntax

If the caller hands a macro the wrong kind of token for a specifier, the matcher simply fails to match, and because there is no other rule, you get a "no rules expected this token" error that points at the metavariable it was trying to match:

```rust
macro_rules! declare_const {
    ($name:ident: $ty:ty = $value:expr) => {
        const $name: $ty = $value;
    };
}

fn main() {
    declare_const!(1 + 1: u32 = 5); // does not compile: `1` is not an :ident
    println!("done");
}
```

The real compiler error:

```text
error: no rules expected `1`
 --> src/main.rs:9:20
  |
1 | macro_rules! declare_const {
  | -------------------------- when calling this macro
...
9 |     declare_const!(1 + 1: u32 = 5);
  |                    ^ no rules expected this token in macro call
  |
note: while trying to match meta-variable `$name:ident`
 --> src/main.rs:2:6
  |
2 |     ($name:ident: $ty:ty = $value:expr) => {
```

**Fix:** Pass an actual identifier: `declare_const!(MAX: u32 = 5);`. The `note:` line telling you *which metavariable* failed to match is your most useful clue when debugging a stubborn macro call.

### Pitfall 3: Rule order — the general rule shadows the specific one

The first matching rule wins, so a too-general rule placed first will swallow inputs you meant for a specific rule:

```rust
// BUG: the :ident rule is first, so `route!(GET ...)` never reaches the GET rule.
macro_rules! route {
    ($method:ident $path:literal => $handler:expr) => { /* always taken */ "general" };
    (GET $path:literal => $handler:expr) => { "specific GET" }; // unreachable in practice
}
```

This usually compiles but quietly does the wrong thing (`route!(GET ...)` produces `"general"`). **Fix:** Order rules from most specific to least specific, exactly as in the working `route!` example above.

### Pitfall 4: A captured `:expr` re-used multiple times is evaluated multiple times

A metavariable substitutes the *syntax*, not a cached value. If you mention `$x` twice and the caller passes a side-effecting expression, it runs twice:

```rust
macro_rules! square {
    ($x:expr) => { $x * $x };
}

fn main() {
    let mut calls = 0;
    let mut next = || { calls += 1; 3 };
    let result = square!(next()); // expands to `next() * next()`
    println!("result = {result}, calls = {calls}"); // result = 9, calls = 2
}
```

**Fix:** Bind once with a `let` inside a block-transcriber so the expression is evaluated a single time:

```rust
macro_rules! square {
    ($x:expr) => {{
        let v = $x;
        v * v
    }};
}
```

---

## Best Practices

- **Choose the most specific specifier that fits.** Prefer `:expr`, `:ident`, `:ty`, or `:pat` over `:tt`. The stricter the specifier, the earlier and clearer the error the caller gets, and the better the grouping guarantees.
- **Reserve `:tt` for forwarding and recursion.** Use it when you genuinely need raw, uninterpreted tokens, for example to count items or to pass a token stream on to another macro.
- **Order rules specific → general.** Literal/keyword-anchored rules first, catch-all rules last, so the right arm fires.
- **Evaluate side-effecting `:expr` arguments exactly once.** Bind them to a `let` in a `{{ ... }}` block before reusing them.
- **Use `stringify!`, `concat!`, and `$crate` to keep expansions reliable.** `stringify!($x)` turns captured syntax into a string literal; `$crate` makes a macro callable from other crates without the caller needing your crate in scope.
- **Reach for a function first.** A macro is the right tool only when you need to operate on *syntax* (variadic shapes, generating items, capturing identifiers/types). If a generic function would do, write the function; see [Generics and Traits](/09-generics-traits/).

---

## Real-World Example

A common production need is reading typed configuration from environment variables with optional defaults — the Rust analog of the JavaScript `makeConfig` helper above, but type-checked at compile time. This macro uses `:ident`, `:ty`, `:literal`, and `:expr`, plus two rules (required vs. defaulted):

```rust
use std::collections::HashMap;

// Read a typed env var into a local binding.
//   config_field!(name: Type = env "KEY");                  // required
//   config_field!(name: Type = env "KEY", default expr);    // optional with default
macro_rules! config_field {
    ($name:ident: $ty:ty = env $key:literal) => {
        let $name: $ty = std::env::var($key)
            .ok()
            .and_then(|v| v.parse::<$ty>().ok())
            .unwrap_or_else(|| panic!("missing/invalid env var {}", $key));
    };
    ($name:ident: $ty:ty = env $key:literal, default $default:expr) => {
        let $name: $ty = std::env::var($key)
            .ok()
            .and_then(|v| v.parse::<$ty>().ok())
            .unwrap_or($default);
    };
}

// A tiny map literal: zero pairs, or one `key => value` pair.
macro_rules! map {
    () => { HashMap::new() };
    ($k:expr => $v:expr) => {{
        let mut m = HashMap::new();
        m.insert($k, $v);
        m
    }};
}

fn main() {
    // No env vars set here, so both fall back to their defaults.
    config_field!(port: u16 = env "PORT", default 8080u16);
    config_field!(workers: usize = env "WORKERS", default 4usize);
    println!("port={port} workers={workers}");

    let single: HashMap<&str, i32> = map!("answer" => 42);
    println!("single = {single:?}");
    let empty: HashMap<&str, i32> = map!();
    println!("empty = {empty:?}");
}
```

Output:

```text
port=8080 workers=4
single = {"answer": 42}
empty = {}
```

The `config_field!` macro picks the second rule when a `default ...` clause is present and the first rule otherwise: a clean shape-based dispatch. Each invocation expands to an ordinary `let` binding with a known static type, so the rest of `main` type-checks exactly as if you had written the bindings by hand. The `map!` macro previews the repetition you will build out fully in [Repetition](/14-macros/03-repetition/) (a real `vec!`-style macro that takes *any* number of pairs).

---

## Further Reading

- [Macros by Example — The Rust Reference](https://doc.rust-lang.org/reference/macros-by-example.html): the authoritative list of fragment specifiers and follow-set rules
- [The Rust Book: Macros](https://doc.rust-lang.org/book/ch20-05-macros.html) — gentle introduction with `vec!` worked out
- [The Little Book of Rust Macros](https://veykril.github.io/tlborm/): the definitive community guide to `macro_rules!` patterns
- [`stringify!`](https://doc.rust-lang.org/std/macro.stringify.html) and [`matches!`](https://doc.rust-lang.org/std/macro.matches.html) — std macros used on this page
- Related sections in this guide:
  - [Macro Basics](/14-macros/00-macro-basics/): what macros are and are *not* (not decorators, not functions)
  - [Declarative Macros](/14-macros/01-declarative-macros/) — the `macro_rules!` syntax and a first example expanded
  - [Repetition](/14-macros/03-repetition/): the `$(...)*` / `$(...),*` operator that consumes these specifiers in bulk
  - [Common Macros](/14-macros/08-common-macros/) — `vec!`, `println!`, `matches!`, `assert!` and friends
  - [Procedural Macros](/14-macros/07-proc-macros/): when `macro_rules!` is not enough and you reach for `syn` + `quote`
  - [Function-like Macros](/14-macros/06-function-like-macros/) — procedural `foo!(...)` macros versus declarative ones
  - [Pattern Matching with `match`](/04-control-flow/02-match/): the `:pat` fragments you capture mirror real `match` patterns
  - [Generics and Traits](/09-generics-traits/) — prefer these when a function would do
  - [Serialization](/15-serialization/): `serde` leans on derive macros built from these same building blocks

---

## Exercises

### Exercise 1: A variadic `min!`

**Difficulty:** Easy

**Objective:** Combine multiple rules with the `:expr` specifier and recursion.

**Instructions:**

1. Write a macro `min!` that returns the smallest of its arguments.
2. It must accept a single expression (`min!(5)` returns `5`) and any number of comma-separated expressions (`min!(8, 3, 5, 1, 9)` returns `1`).
3. Accept an optional trailing comma. Use a recursive rule that peels off the first argument and calls `min!` on the rest.
4. Print `min!(8, 3, 5, 1, 9)`.

<details>
<summary>Solution</summary>

```rust
macro_rules! min {
    ($a:expr) => { $a };
    ($a:expr, $($rest:expr),+ $(,)?) => {{
        let a = $a;
        let b = min!($($rest),+);
        if a < b { a } else { b }
    }};
}

fn main() {
    println!("min = {}", min!(8, 3, 5, 1, 9)); // min = 1
    println!("single = {}", min!(42));         // single = 42
}
```

The first rule is the recursion base case (one expression). The second rule binds the head, recurses on the tail with the `$(...),+` repetition, and compares. The `$(,)?` allows a trailing comma. Output: `min = 1` then `single = 42`.

</details>

### Exercise 2: A `getter!` method generator

**Difficulty:** Medium

**Objective:** Use the `:ident` and `:ty` specifiers together to generate code (a method), not just a value.

**Instructions:**

1. Write a macro `getter!(field: Type)` that expands to a method named after `field` returning `&Type`, reading `self.field`.
2. Define `struct Account { owner: String, balance: u64 }`.
3. Inside `impl Account`, invoke `getter!` twice to generate `owner()` and `balance()` accessors.
4. Construct an `Account` and print both accessor results.

<details>
<summary>Solution</summary>

```rust
macro_rules! getter {
    ($field:ident : $ty:ty) => {
        fn $field(&self) -> &$ty {
            &self.$field
        }
    };
}

struct Account {
    owner: String,
    balance: u64,
}

impl Account {
    getter!(owner: String);
    getter!(balance: u64);
}

fn main() {
    let acc = Account { owner: String::from("Ada"), balance: 100 };
    println!("owner={} balance={}", acc.owner(), acc.balance());
}
```

`$field:ident` becomes both the method name and the field name accessed via `self.$field`; `$ty` becomes the return type. Output: `owner=Ada balance=100`.

</details>

### Exercise 3: A `match_or!` expression

**Difficulty:** Medium-Hard

**Objective:** Use the `:pat` specifier alongside `:expr` to build a control-flow construct.

**Instructions:**

1. Write a macro `match_or!(value, PATTERN => RESULT, _ => DEFAULT)` that expands to a `match` returning `RESULT` when `value` matches `PATTERN` (binding any captured variables for use in `RESULT`), and `DEFAULT` otherwise.
2. Use it on `Some(42)` with the pattern `Some(n)` to produce `"got 42"`, and confirm `None` falls through to a default.

<details>
<summary>Solution</summary>

```rust
macro_rules! match_or {
    ($value:expr, $pat:pat => $result:expr, _ => $default:expr) => {
        match $value {
            $pat => $result,
            _ => $default,
        }
    };
}

fn main() {
    let hit = match_or!(Some(42), Some(n) => format!("got {n}"), _ => "none".to_string());
    println!("{hit}");

    let miss: Option<i32> = None;
    let label = match_or!(miss, Some(n) => format!("got {n}"), _ => "none".to_string());
    println!("{label}");
}
```

The `$pat:pat` fragment captures `Some(n)` as a real pattern, so the binding `n` is in scope inside the `$result` expression. Output: `got 42` then `none`.

</details>
