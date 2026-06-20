---
title: "Common Clippy Lints, Explained"
description: "The Clippy lints you hit daily in Rust, with before/after fixes: needless_return, len_zero, redundant_clone, or_fun_call, and the eager-vs-lazy trap."
---

## Quick Overview

[Clippy](/24-tooling/02-linting/) ships with over 750 lints, and the first time you run it on a real codebase it can feel like a very opinionated code reviewer dumped a wall of warnings on you. This page walks through the handful of lints you will actually see every day — `needless_return`, `uninlined_format_args`, `clone_on_copy`, `len_zero`, `redundant_clone`, and friends — with a **before/after** for each and the *real* Clippy output. The goal is that after reading this you can look at a warning, understand *why* Clippy is suggesting the change, and decide whether to take it.

> **Note:** This file is the "lint catalogue". For how to *run* Clippy, set lint levels (`allow`/`warn`/`deny`), and wire `#![deny(clippy::all)]` into a crate, see [Linting with Clippy](/24-tooling/02-linting/). For the formatter (rustfmt), see [Formatting with rustfmt](/24-tooling/01-formatting/).

---

## TypeScript/JavaScript Example

A senior TypeScript developer is used to ESLint plus `typescript-eslint` rules like `no-useless-return`, `prefer-template`, `@typescript-eslint/prefer-string-starts-ends-with`, and `no-unnecessary-condition`. Those rules push you toward idiomatic, less-error-prone code, and you mostly internalize them until you stop writing the flagged patterns at all.

Here is a small order-summary helper that an ESLint config with the recommended rules would nudge on:

```typescript
// summary.ts
interface Order {
  id: number;
  items: string[];
  note?: string;
}

function summarize(order: Order): string {
  const id = order.id;
  const count = order.items.length;

  // ESLint: prefer `order.items.length === 0` is fine in JS, but the analogous
  // Rust `.len() == 0` has a dedicated `.is_empty()`.
  if (order.items.length === 0) {
    return "empty order";
  }

  // no-useless-return / prefer-template territory:
  const note = order.note ?? "(none)";
  const names = order.items.map((i) => i); // pointless .map identity
  return "order " + id + " has " + count + " items: " + names.join(", ") + " [" + note + "]";
}

console.log(summarize({ id: 7, items: ["pen", "ink"] }));
```

The ESLint analogues here would be `prefer-template` (use a template literal instead of `+` concatenation) and the pointless identity `.map((i) => i)`. Clippy is the same kind of tool for Rust, except its suggestions are usually *machine-applicable*: it can rewrite the code for you.

---

## Rust Equivalent

Here is the same function written the way a TypeScript developer often writes Rust on day one. It compiles and runs correctly, but Clippy has opinions about almost every line:

```rust playground
// src/main.rs — the "before". Compiles, runs, but Clippy flags it.
#[derive(Debug)]
struct Order {
    id: u32,
    items: Vec<String>,
    note: Option<String>,
}

fn summarize(order: &Order) -> String {
    let id = order.id;
    let count = order.items.len();
    if order.items.len() == 0 {
        return String::from("empty order");
    }
    let note = order.note.clone().unwrap_or(String::from("(none)"));
    let names: Vec<String> = order.items.iter().map(|i| i.clone()).collect();
    return format!("order {} has {} items: {} [{}]", id, count, names.join(", "), note);
}

fn main() {
    let order = Order {
        id: 7,
        items: vec![String::from("pen"), String::from("ink")],
        note: None,
    };
    println!("{}", summarize(&order));
}
```

Running `cargo clippy` on this (with `#![warn(clippy::uninlined_format_args)]` added so the format-string lint also fires) produces real warnings for `len() == 0`, the identity `.map(|i| i.clone())`, the trailing `return`, and the un-inlined format arguments. Here is the cleaned-up **after**. It is Clippy-clean even under the stricter `clippy::pedantic` group, and arguably easier to read:

```rust playground
// src/main.rs — the "after". Clean under `cargo clippy -- -W clippy::pedantic`.
#[derive(Debug)]
struct Order {
    id: u32,
    items: Vec<String>,
    note: Option<String>,
}

fn summarize(order: &Order) -> String {
    if order.items.is_empty() {
        return String::from("empty order");
    }
    let id = order.id;
    let count = order.items.len();
    let note = order.note.as_deref().unwrap_or("(none)");
    let names = order.items.join(", ");
    format!("order {id} has {count} items: {names} [{note}]")
}

fn main() {
    let order = Order {
        id: 7,
        items: vec![String::from("pen"), String::from("ink")],
        note: None,
    };
    println!("{}", summarize(&order));
}
```

Running it prints:

```text
order 7 has 2 items: pen, ink [(none)]
```

The rest of this page breaks the individual lints out one at a time, because each one teaches a small, transferable piece of "how Rust wants to be written".

---

## Detailed Explanation

Each lint below shows the **before**, the **real Clippy warning** (captured by running `cargo clippy` on a probe project with `rustc 1.96.0` / `clippy 0.1.96`), and the **after**. Every warning ends with a `help:` link to `rust-lang.github.io/rust-clippy`, and tells you the lint name in the `#[warn(...)]` note. That name is what you `allow`/`deny` (see [Linting with Clippy](/24-tooling/02-linting/)).

### `needless_return` — drop the trailing `return`

Rust is expression-oriented: the last expression in a block *is* the return value, no `return` keyword needed. Coming from JavaScript, where `return` is mandatory, you will reach for it reflexively.

```rust
// before
fn needless_return(x: i32) -> i32 {
    return x + 1;
}
```

Real Clippy output:

```text
warning: unneeded `return` statement
 --> src/main.rs:7:5
  |
7 |     return x + 1;
  |     ^^^^^^^^^^^^
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_return
  = note: `#[warn(clippy::needless_return)]` on by default
help: remove `return`
  |
7 -     return x + 1;
7 +     x + 1
  |
```

```rust
// after
fn needless_return(x: i32) -> i32 {
    x + 1
}
```

> **Tip:** Note the missing semicolon. `x + 1` is an *expression* that becomes the function's value; `x + 1;` is a *statement* of type `()` and would be a type error. An early `return` in the *middle* of a function (like the `is_empty()` guard above) is fine and idiomatic. `needless_return` only fires on a `return` in tail position.

### `uninlined_format_args` — put the variable in the braces

Since Rust 1.58, format strings can capture variables by name directly: `format!("{name}")` instead of `format!("{}", name)`. Clippy nudges you toward the inline form (this lint is in the `style` group; it is on by default in recent Clippy and shown here enabled explicitly so it always fires).

```rust
// before
fn uninlined(name: &str, age: u32) {
    println!("{} is {} years old", name, age);
}
```

Real Clippy output:

```text
warning: variables can be used directly in the `format!` string
 --> src/main.rs:4:5
  |
4 |     println!("{} is {} years old", name, age);
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#uninlined_format_args
help: change this to
  |
4 -     println!("{} is {} years old", name, age);
4 +     println!("{name} is {age} years old");
  |
```

```rust
// after
fn uninlined(name: &str, age: u32) {
    println!("{name} is {age} years old");
}
```

> **Note:** Inline capture only works for *bare identifiers in scope*. `format!("{}", user.name)` cannot become `format!("{user.name}")`; field accesses and method calls still go in the trailing argument list. This is the same restriction as JavaScript template literals only being terse for simple `${name}` substitutions.

### `clone_on_copy` — you do not clone a number

Types that implement the `Copy` trait (`i32`, `f64`, `bool`, `char`, small `#[derive(Copy)]` structs) are copied implicitly on assignment. Calling `.clone()` on them is harmless but redundant, and signals you have not internalized the [Copy vs Clone distinction](/05-ownership/06-move-copy-clone/) yet.

```rust
// before
fn clone_on_copy() -> i32 {
    let x: i32 = 5;
    x.clone()
}
```

Real Clippy output:

```text
warning: using `clone` on type `i32` which implements the `Copy` trait
 --> src/main.rs:3:5
  |
3 |     x.clone()
  |     ^^^^^^^^^ help: try removing the `clone` call: `x`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#clone_on_copy
  = note: `#[warn(clippy::clone_on_copy)]` on by default
```

```rust
// after
fn clone_on_copy() -> i32 {
    let x: i32 = 5;
    x
}
```

### `len_zero` — use `is_empty()`

`x.len() == 0` works, but `x.is_empty()` is clearer and, for some types, cheaper (it does not have to compute the full length). This is the Rust analogue of the lint family that pushes JavaScript toward `array.length === 0` checks being written consistently.

```rust
// before
fn len_zero(v: &[i32]) -> bool {
    v.len() == 0
}
```

Real Clippy output:

```text
warning: length comparison to zero
 --> src/main.rs:2:5
  |
2 |     v.len() == 0
  |     ^^^^^^^^^^^^ help: using `is_empty` is clearer and more explicit: `v.is_empty()`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#len_zero
  = note: `#[warn(clippy::len_zero)]` on by default
```

```rust
// after
fn len_zero(v: &[i32]) -> bool {
    v.is_empty()
}
```

### `map_clone` — use `.cloned()` (or `.copied()`)

`iter().map(|x| x.clone())` is so common that there is a dedicated adapter: `.cloned()` for `Clone` types, `.copied()` for `Copy` types. It is shorter and reads as "give me owned values".

```rust
// before
fn map_clone(v: &[String]) -> Vec<String> {
    v.iter().map(|s| s.clone()).collect()
}
```

Real Clippy output:

```text
warning: you are using an explicit closure for cloning elements
  --> src/main.rs:10:5
   |
10 |     v.iter().map(|s| s.clone()).collect()
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: consider calling the dedicated `cloned` method: `v.iter().cloned()`
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#map_clone
   = note: `#[warn(clippy::map_clone)]` on by default
```

```rust
// after
fn map_clone(v: &[String]) -> Vec<String> {
    v.iter().cloned().collect()
}
```

### `ptr_arg` — take `&str`, not `&String`; `&[T]`, not `&Vec<T>`

A function that only *reads* a string should accept `&str`, and one that reads a list should accept `&[T]`. These "slice" types accept more callers (a `&String` coerces to `&str` for free, but not vice versa) and avoid a layer of indirection. This is one of the most important ergonomic lessons for newcomers.

```rust
// before
fn first_word(s: &String) -> &str {
    s.split(' ').next().unwrap_or("")
}
```

Real Clippy output:

```text
warning: writing `&String` instead of `&str` involves a new object where a slice will do
 --> src/main.rs:9:17
  |
9 | fn redundant(s: &String) -> usize {
  |                 ^^^^^^^ help: change this to: `&str`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#ptr_arg
  = note: `#[warn(clippy::ptr_arg)]` on by default
```

```rust
// after
fn first_word(s: &str) -> &str {
    s.split(' ').next().unwrap_or("")
}
```

### `needless_borrow` — drop the extra `&`

Method resolution auto-references for you, so `(&s).len()` is just `s.len()`. Writing the explicit `&` is the kind of habit you pick up from fighting the borrow checker and then never unlearn.

```rust
// before
fn needless_borrow(s: &str) {
    println!("{}", (&s).len());
}
```

Real Clippy output:

```text
warning: this expression creates a reference which is immediately dereferenced by the compiler
  --> src/main.rs:14:20
   |
14 |     println!("{}", (&s).len());
   |                    ^^^^ help: change this to: `s`
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_borrow
   = note: `#[warn(clippy::needless_borrow)]` on by default
```

```rust
// after
fn needless_borrow(s: &str) {
    println!("{}", s.len());
}
```

### `unwrap_or_default` — let the type provide its default

`option.unwrap_or(String::new())` constructs an empty `String` whose only purpose is to be the fallback. `Default::default()` is exactly that value, and `.unwrap_or_default()` expresses the intent directly.

```rust
// before
fn or_fun_call(o: Option<String>) -> String {
    o.unwrap_or(String::new())
}
```

Real Clippy output:

```text
warning: use of `unwrap_or` to construct default value
 --> src/main.rs:6:7
  |
6 |     o.unwrap_or(String::new())
  |       ^^^^^^^^^^^^^^^^^^^^^^^^ help: try: `unwrap_or_default()`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unwrap_or_default
  = note: `#[warn(clippy::unwrap_or_default)]` on by default
```

```rust
// after
fn or_fun_call(o: Option<String>) -> String {
    o.unwrap_or_default()
}
```

### `or_fun_call` — `unwrap_or_else` is lazy, `unwrap_or` is eager

This is the most *behaviorally significant* lint in this list, and the most important one for a JavaScript developer to internalize. `unwrap_or(x)` evaluates `x` **eagerly**, *before* it knows whether the `Option` is `Some`. So `o.unwrap_or(expensive())` runs `expensive()` every time, even when its result is thrown away. `unwrap_or_else(|| expensive())` is **lazy**: the closure only runs on the `None` branch. (This is the same eager-vs-lazy trap as JavaScript's `a ?? defaultExpr` always evaluating `defaultExpr`, versus wrapping it in a function.)

```rust
// before
fn lookup(o: Option<&str>) -> String {
    o.map(str::to_string).unwrap_or(String::from("anonymous"))
}
```

Real Clippy output:

```text
warning: function call inside of `unwrap_or`
 --> src/main.rs:4:27
  |
4 |     o.map(str::to_string).unwrap_or(String::from("anonymous"))
  |                           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: try: `unwrap_or_else(|| String::from("anonymous"))`
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#or_fun_call
  = note: `#[warn(clippy::or_fun_call)]`
```

```rust
// after
fn lookup(o: Option<&str>) -> String {
    o.map(str::to_string)
        .unwrap_or_else(|| String::from("anonymous"))
}
```

### `redundant_clone` — you cloned, then dropped the original anyway

This lint (in the `nursery`/`pedantic` family) does real ownership analysis: if you `clone()` a value and the original is never used again, the clone was pointless. You could have moved the original. Cloning is one of the most common ways newcomers "make the borrow checker happy" without realizing it costs a heap allocation.

```rust
// before
fn process(data: Vec<i32>) -> i32 {
    let copy = data.clone();
    copy.iter().sum()
}
```

Real Clippy output (with `#![warn(clippy::redundant_clone)]`):

```text
warning: redundant clone
 --> src/main.rs:4:20
  |
4 |     let copy = data.clone();
  |                    ^^^^^^^^ help: remove this
  |
note: this value is dropped without further use
 --> src/main.rs:4:16
  |
4 |     let copy = data.clone();
  |                ^^^^
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#redundant_clone
```

```rust
// after
fn process(data: Vec<i32>) -> i32 {
    data.iter().sum()
}
```

### `needless_range_loop` and `manual_map` — let iterators do the work

Two more "write it the idiomatic way" lints. `for i in 0..v.len() { ... v[i] ... }` is the C-style index loop; Rust prefers iterating the collection directly. And a `match` that maps `Some(x) => Some(f(x))` / `None => None` is exactly what `Option::map` does.

```rust
// before
fn needless_range_loop(v: &[i32]) {
    for i in 0..v.len() {
        println!("{}", v[i]);
    }
}

fn manual_map(o: Option<i32>) -> Option<i32> {
    match o {
        Some(x) => Some(x + 1),
        None => None,
    }
}
```

Real Clippy output:

```text
warning: the loop variable `i` is only used to index `v`
  --> src/main.rs:18:14
   |
18 |     for i in 0..v.len() {
   |              ^^^^^^^^^^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_range_loop
   = note: `#[warn(clippy::needless_range_loop)]` on by default
help: consider using an iterator
   |
18 -     for i in 0..v.len() {
18 +     for <item> in &v {
   |

warning: manual implementation of `Option::map`
  --> src/main.rs:24:5
   |
24 | /     match o {
25 | |         Some(x) => Some(x + 1),
26 | |         None => None,
27 | |     }
   | |_____^ help: try: `o.map(|x| x + 1)`
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#manual_map
   = note: `#[warn(clippy::manual_map)]` on by default
```

```rust
// after
fn needless_range_loop(v: &[i32]) {
    for item in v {
        println!("{item}");
    }
}

fn manual_map(o: Option<i32>) -> Option<i32> {
    o.map(|x| x + 1)
}
```

### A few more you will meet quickly

| Lint | Flags | Fix |
| --- | --- | --- |
| `redundant_field_names` | `P { name: name }` | `P { name }` (struct field shorthand, like JS `{ name }`) |
| `bool_comparison` | `if flag == true` | `if flag` |
| `collapsible_if` | `if a { if b { .. } }` | `if a && b { .. }` |
| `let_and_return` | `let w = f(); w` | `f()` |
| `redundant_closure` | `.map(|x| f(x))` | `.map(f)` |
| `single_char_pattern` | `s.split("x")` | `s.split('x')` (a `char`, not a one-char `&str`) |

---

## Key Differences

| Aspect | ESLint / typescript-eslint | Clippy |
| --- | --- | --- |
| Suggestions | Some rules autofixable (`--fix`) | Most lints are **machine-applicable**; `cargo clippy --fix` rewrites them |
| Type awareness | Needs `@typescript-eslint` + type info | Always has full type + borrow info (it runs inside the compiler) |
| Categories | `recommended`, plugin presets | Groups: `correctness` (deny), `style`, `complexity`, `perf`, `suspicious`, `pedantic`, `nursery`, `cargo` |
| Default strictness | Off until configured | `correctness`/`style`/`complexity`/`perf`/`suspicious` are on by default; `pedantic`/`nursery`/`cargo` are opt-in |
| Behavioral lints | Rare | `or_fun_call` and `redundant_clone` change *runtime cost/behavior*, not just style |
| Lint reference | Per-rule docs pages | Every warning links to `rust-lang.github.io/rust-clippy` |

The single biggest mental shift: in ESLint, most rules are pure style. In Clippy, a large fraction of the default lints are about **correctness and cost** because they rely on the type system and ownership analysis. `clone_on_copy`, `redundant_clone`, and `or_fun_call` exist because Clippy can *see* that a heap allocation or eager evaluation is unnecessary, something a syntactic linter could never know.

> **Note:** Clippy's `correctness` group is set to `deny` by default. Those lints catch likely *bugs* (e.g. `# [derive(Hash)]` with a manual `PartialEq`), so they fail the build, not just warn. The other default groups warn. See [Linting with Clippy](/24-tooling/02-linting/) for adjusting these levels.

---

## Common Pitfalls

### 1. Treating every Clippy warning as gospel

Clippy is heuristic. A suggestion can be a wash for readability, or occasionally wrong for your context. The fix is not to disable Clippy but to `allow` the specific lint *at the narrowest scope* with a comment explaining why:

```rust
// A dot product genuinely needs the index to pair two parallel slices,
// so the range loop is intentional here.
#[allow(clippy::needless_range_loop)] // index pairs `a[i]` with `b[i]`
fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut sum = 0.0;
    for i in 0..a.len() {
        sum += a[i] * b[i];
    }
    sum
}
```

This compiles with no warnings and documents the deviation. (The truly idiomatic version is `a.iter().zip(b).map(|(x, y)| x * y).sum()`, but the point stands: scoped `allow` beats globally disabling a lint.)

### 2. Blindly running `cargo clippy --fix` and not reading the diff

`--fix` is great, but a few lints (notably `or_fun_call`) change semantics from eager to lazy evaluation. That is almost always what you want, but you should still review the diff, the same way you would review a large ESLint `--fix` autofix. Run it on a clean working tree so `git diff` shows exactly what changed:

```bash
git status                         # make sure the tree is clean first
cargo clippy --fix                 # rewrites the source in place
git diff                           # review every change
```

### 3. Confusing `unwrap_or` (eager) with `unwrap_or_else` (lazy)

The classic real bug: `cache.get(key).unwrap_or(load_from_disk(key))` hits the disk on *every* call, cache hit or not, because the argument to `unwrap_or` is evaluated before the lookup result is known. This is the same footgun as JavaScript's `cache.get(key) ?? loadFromDisk(key)` always calling `loadFromDisk`. Clippy's `or_fun_call` catches the common shapes, but not all of them: understand the rule, do not rely on the lint to always fire.

### 4. Expecting `pedantic`/`nursery` lints by default

`cargo clippy` does **not** run the `pedantic`, `nursery`, or `cargo` groups. If a tutorial shows `redundant_clone` or `cast_precision_loss` firing, they enabled it. Opt in per crate or per run:

```bash
cargo clippy -- -W clippy::pedantic        # one run
```

```rust
// or at the top of lib.rs / main.rs, for the whole crate
#![warn(clippy::pedantic)]
```

> **Warning:** `clippy::pedantic` is *intentionally* noisy: it includes subjective lints like `cast_precision_loss` and `module_name_repetitions`. Enable it, then `#![allow(...)]` the handful you disagree with, rather than leaving it off entirely. See [Linting with Clippy](/24-tooling/02-linting/) for the recommended baseline.

---

## Best Practices

- **Run Clippy in CI with `-D warnings`** so a regression fails the build, not just scrolls past in a log. See [CI/CD Concepts for Rust](/24-tooling/07-ci-cd/) and [A Real GitHub Actions Workflow for Rust](/24-tooling/08-github-actions/).

  ```bash
  cargo clippy --all-targets --all-features -- -D warnings
  ```

- **Read the lint name and visit its docs page before silencing it.** Every lint at `rust-lang.github.io/rust-clippy` explains *why* it exists and gives a "Known problems" section listing false positives.

- **Prefer narrow `#[allow(...)]` with a justifying comment** over disabling a lint crate-wide. A reviewer should be able to see *why* the deviation is intentional.

- **Turn on `clippy::pedantic` early in a new project**, while the codebase is small, and curate the allows. Retrofitting it onto a large crate is a slog.

- **Let `cargo clippy --fix` do the boring rewrites**, then commit those separately from logic changes so the diff is easy to review.

- **Internalize the *category* of each lint.** The `perf` and the behavioral lints (`redundant_clone`, `or_fun_call`, `map_clone`) teach you how Rust *wants* you to manage [ownership and allocation](/05-ownership/06-move-copy-clone/); the `style` ones just teach you Rust's surface idioms.

---

## Real-World Example

A small log-parsing utility, written the "first draft" way and then cleaned up using Clippy. The first version compiles and runs, but earns a stack of warnings.

```rust playground
// src/main.rs — first draft, before Clippy
#[derive(Debug)]
struct LogLine {
    level: String,
    message: String,
}

fn parse(raw: &Vec<String>) -> Vec<LogLine> {
    let mut out: Vec<LogLine> = Vec::new();
    for i in 0..raw.len() {
        let line = raw[i].clone();
        if line.len() == 0 {
            continue;
        }
        let parts: Vec<String> = line.split(' ').map(|p| p.to_string()).collect();
        let level = parts.get(0).cloned().unwrap_or(String::from("INFO"));
        let message = parts[1..].join(" ");
        out.push(LogLine { level: level, message: message });
    }
    return out;
}

fn main() {
    let raw = vec![
        String::from("ERROR disk full"),
        String::from(""),
        String::from("INFO started"),
    ];
    for line in parse(&raw) {
        println!("[{}] {}", line.level, line.message);
    }
}
```

Running `cargo clippy` flags `ptr_arg` (`&Vec<String>` → `&[String]`), `needless_range_loop`, `len_zero`, `get(0)` → `first()`, `unwrap_or` constructing a default, `redundant_field_names`, and `needless_return`. After taking the suggestions (and inlining the format args), the cleaned-up version is Clippy-clean under `clippy::all`:

```rust playground
// src/main.rs — after Clippy. Clean under `cargo clippy -- -W clippy::all`.
#[derive(Debug)]
struct LogLine {
    level: String,
    message: String,
}

fn parse(raw: &[String]) -> Vec<LogLine> {
    let mut out: Vec<LogLine> = Vec::new();
    for line in raw {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(' ').collect();
        let level = parts.first().map_or("INFO", |p| *p).to_string();
        let message = parts[1..].join(" ");
        out.push(LogLine { level, message });
    }
    out
}

fn main() {
    let raw = vec![
        String::from("ERROR disk full"),
        String::from(""),
        String::from("INFO started"),
    ];
    for line in parse(&raw) {
        println!("[{}] {}", line.level, line.message);
    }
}
```

It prints:

```text
[ERROR] disk full
[INFO] started
```

The cleaned-up version is shorter, and it also allocates less (no per-line `.clone()`, no `Vec<String>` of split parts when `Vec<&str>` will do) and reads more like idiomatic Rust. That is the real payoff: Clippy is a teacher that nudges you from "Rust that works" toward "Rust the way Rust developers write it".

---

## Further Reading

- [Clippy lint list](https://rust-lang.github.io/rust-clippy/master/index.html) — every lint, searchable, with rationale and known problems
- [The Clippy book](https://doc.rust-lang.org/clippy/) — official guide to configuration, lint groups, and `clippy.toml`
- [Linting with Clippy](/24-tooling/02-linting/) — how to run Clippy, set lint levels, and `#![deny(clippy::all)]` (this section's "ESLint → Clippy" page)
- [Formatting with rustfmt](/24-tooling/01-formatting/) — the companion formatter, rustfmt
- [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/) — Cargo profiles, aliases, and workspace tricks
- [CI/CD Concepts for Rust](/24-tooling/07-ci-cd/) and [A Real GitHub Actions Workflow for Rust](/24-tooling/08-github-actions/) — gating `fmt` + `clippy` + `test` in CI
- [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/) — the Copy/Clone distinction behind `clone_on_copy` and `redundant_clone`
- [Understanding Cargo](/01-getting-started/03-cargo-basics/) — Cargo fundamentals if you are new to the tool
- [Advanced Topics](/25-advanced-topics/) — where to go after the tooling section

---

## Exercises

### Exercise 1 — Take the suggestions

**Difficulty:** Beginner

**Objective:** Practice reading Clippy output and applying the fix.

**Instructions:** In a fresh `cargo new` project, paste the function below into `src/main.rs`, run `cargo clippy`, and rewrite the function so it produces zero warnings. The function returns the first character of each name.

```rust
fn initials(names: &Vec<String>) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    for i in 0..names.len() {
        let name = names[i].clone();
        match name.chars().next() {
            Some(c) => out.push(c),
            None => {}
        }
    }
    return out;
}
```

<details>
<summary>Solution</summary>

Clippy flags `ptr_arg` (`&Vec<String>`), `needless_range_loop`, the redundant `.clone()`, the `match` that should be a `filter_map`, and `needless_return`. The idiomatic version:

```rust playground
// Clean under `cargo clippy`. Verified output below.
fn initials(names: &[String]) -> Vec<char> {
    names
        .iter()
        .filter_map(|n| n.chars().next())
        .collect()
}

fn main() {
    let names = vec![String::from("Ann"), String::from("Bob")];
    println!("{:?}", initials(&names));
}
```

Running it prints:

```text
['A', 'B']
```

</details>

### Exercise 2 — Eager vs lazy

**Difficulty:** Intermediate

**Objective:** Understand *why* `or_fun_call` matters, not just how to silence it.

**Instructions:** The function below uses `unwrap_or` with a function call. (1) Explain in one sentence what is wrong with it from a performance standpoint. (2) Rewrite it so `slow_default()` is only called when the `Option` is `None`. (3) Add a `println!` inside `slow_default` and confirm by running that the `Some` case never calls it.

```rust
fn slow_default() -> String {
    // imagine this hits the network
    String::from("fallback")
}

fn resolve(o: Option<String>) -> String {
    o.unwrap_or(slow_default())
}
```

<details>
<summary>Solution</summary>

(1) `unwrap_or(slow_default())` evaluates `slow_default()` **eagerly**, every call, even when `o` is `Some` and the result is discarded. (2) Use the lazy `unwrap_or_else` with a closure. (3) The instrumentation confirms it.

```rust playground
fn slow_default() -> String {
    println!("slow_default() was called");
    String::from("fallback")
}

fn resolve(o: Option<String>) -> String {
    o.unwrap_or_else(slow_default)
}

fn main() {
    // Some case: slow_default must NOT be called.
    println!("{}", resolve(Some(String::from("cached"))));
    // None case: slow_default IS called.
    println!("{}", resolve(None));
}
```

Running it prints (note `slow_default() was called` appears only once, for the `None` case):

```text
cached
slow_default() was called
fallback
```

Passing `slow_default` directly (instead of `|| slow_default()`) also satisfies Clippy's `redundant_closure` lint.

</details>

### Exercise 3 — Tame `pedantic`

**Difficulty:** Advanced

**Objective:** Refactor an imperative function into idiomatic iterator style and survive `clippy::pedantic`.

**Instructions:** Add `#![warn(clippy::pedantic)]` to the top of `src/main.rs`, paste the code below, run `cargo clippy`, and resolve every warning. `active_names` should return the names of active users; `average_active_age` should return the average age of active users, or `None` if there are none. One of the warnings is a `cast_precision_loss` from `pedantic`; decide whether to restructure the code to avoid the cast or to `#[allow]` it with a justification.

```rust
#[derive(Debug, Clone)]
struct User {
    name: String,
    active: bool,
    age: u32,
}

fn active_names(users: &Vec<User>) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..users.len() {
        if users[i].active == true {
            out.push(users[i].name.clone());
        }
    }
    return out;
}

fn average_active_age(users: &Vec<User>) -> Option<f64> {
    let mut total = 0;
    let mut n = 0;
    for u in users {
        if u.active {
            total += u.age;
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    return Some(total as f64 / n as f64);
}
```

<details>
<summary>Solution</summary>

This is clean under `cargo clippy -- -W clippy::all` (the default groups). The `cast_precision_loss` warning only appears under `pedantic`; here we keep the cast but it is justified by `n` being a small count, so a scoped `#[allow]` with a comment is the pragmatic choice.

```rust playground
#[derive(Debug, Clone)]
struct User {
    name: String,
    active: bool,
    age: u32,
}

fn active_names(users: &[User]) -> Vec<String> {
    users
        .iter()
        .filter(|u| u.active)
        .map(|u| u.name.clone())
        .collect()
}

#[allow(clippy::cast_precision_loss)] // counts are small; f64 is exact here
fn average_active_age(users: &[User]) -> Option<f64> {
    let ages: Vec<u32> = users.iter().filter(|u| u.active).map(|u| u.age).collect();
    if ages.is_empty() {
        return None;
    }
    let total: u32 = ages.iter().sum();
    Some(f64::from(total) / ages.len() as f64)
}

fn main() {
    let users = vec![
        User { name: String::from("Ann"), active: true, age: 30 },
        User { name: String::from("Bob"), active: false, age: 40 },
        User { name: String::from("Cy"), active: true, age: 50 },
    ];
    println!("{:?}", active_names(&users));
    println!("{:?}", average_active_age(&users));
}
```

Running it prints:

```text
["Ann", "Cy"]
Some(40.0)
```

Fixes applied: `ptr_arg` (`&Vec<User>` → `&[User]`), `needless_range_loop`, `bool_comparison` (`== true`), the redundant `.clone()` pattern moved into a clean `map`, and `needless_return`.

</details>
