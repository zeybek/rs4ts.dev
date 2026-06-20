---
title: "Macro Repetition: Building Variadic Macros"
description: "How macro_rules! repeats a chunk of code once per matched item, and how to use that to build your own vec!-style variadic macros."
---

How `macro_rules!` repeats a chunk of code once per matched item, and how to use that to build your own `vec!`-style variadic macros.

---

## Quick Overview

Macro **repetition** is the feature that lets a single `macro_rules!` rule accept *any number* of arguments and emit one piece of code per argument. It is written `$( ... )sep rep`, where `sep` is an optional separator token (like `,` or `;`) and `rep` is `*`, `+`, or `?`. This is how the built-in `vec![1, 2, 3]` accepts a comma-separated list of any length, and it is the closest macro-level analog to TypeScript's **rest parameters** (`...args`), except it happens entirely at compile time and generates real, monomorphized code rather than iterating over a runtime array.

---

## TypeScript/JavaScript Example

In TypeScript, "accept any number of arguments" is a runtime concept: you collect them into an array with a rest parameter and loop over that array.

```typescript
// A variadic builder: collect any number of items into an array of strings.
function makeList(...items: unknown[]): string[] {
  const out: string[] = [];
  for (const item of items) {
    out.push(String(item));
  }
  return out;
}

const names = makeList("Alice", "Bob", "Charlie");
console.log(names); // [ 'Alice', 'Bob', 'Charlie' ]

// A nested example: a "grid" is a list of rows, each row a list of cells.
function grid(...rows: number[][]): number[][] {
  return rows.map((row) => [...row]);
}

console.log(grid([1, 2, 3], [4, 5, 6])); // [ [ 1, 2, 3 ], [ 4, 5, 6 ] ]
```

The key traits to keep in mind:

- `...items` is **runtime**: `items` is a real array that exists while the program runs.
- The loop body runs once per element **at runtime**.
- The arguments are all the same statically-known type (`unknown[]` here), and generics are erased: there is no per-call specialization.

---

## Rust Equivalent

In Rust, the equivalent variadic behavior is resolved at **compile time** by a macro. The `$( ... ),*` syntax means "match a comma-separated list, and stamp out the body once per match."

```rust playground
// A variadic builder macro: turn any number of expressions into a Vec<String>.
macro_rules! string_list {
    // $( $x:expr ),*  matches zero-or-more comma-separated expressions.
    // $(,)?           allows an optional trailing comma.
    ( $( $x:expr ),* $(,)? ) => {{
        let mut out: Vec<String> = Vec::new();
        // The body inside $( ... )* is emitted once per matched $x.
        $(
            out.push($x.to_string());
        )*
        out
    }};
}

fn main() {
    let names = string_list!["Alice", "Bob", "Charlie"];
    println!("{:?}", names); // ["Alice", "Bob", "Charlie"]

    // Trailing comma is fine:
    let more = string_list!["x", "y",];
    println!("{:?}", more); // ["x", "y"]

    // Zero items also works because we used `*` (zero-or-more):
    let empty: Vec<String> = string_list![];
    println!("{:?}", empty); // []
}
```

Running this prints:

```text
["Alice", "Bob", "Charlie"]
["x", "y"]
[]
```

> **Note:** Unlike the TypeScript version, no array of arguments ever exists at runtime. The macro **expands** at compile time into three literal `out.push(...)` statements before the compiler even type-checks the program.

> **Tip:** The empty invocation `string_list![]` expands to a `let mut out` that is never pushed to, so it emits a benign `warning: variable does not need to be mutable` (the `unused_mut` lint). It is harmless here; a production macro would add a separate `() => { Vec::new() }` rule to handle the empty case cleanly.

---

## Detailed Explanation

### Anatomy of a repetition

A repetition has three parts:

```text
$( ... )  sep  rep
 ^^^^^^^   ^^^   ^^
 body     separator (optional)   repetition operator
```

- **`$( ... )`**: the *repeated fragment*. Whatever is inside is matched (in the matcher) or emitted (in the body) once per iteration.
- **`sep`**: an *optional* single token that must appear *between* repetitions (commonly `,` or `;`). It does **not** appear after the last item.
- **`rep`**: the repetition operator:

| Operator | Meaning            | TypeScript analog                          |
| -------- | ------------------ | ------------------------------------------ |
| `*`      | zero or more       | `...args` where `args` may be empty        |
| `+`      | one or more        | a function requiring at least one argument |
| `?`      | zero or one        | an optional parameter `arg?`               |

> **Note:** `?` is the *optional* operator and never takes a separator (there is at most one item, so there is nothing to separate). `*` and `+` may or may not have a separator.

### Matcher vs. transcriber

Repetition appears in **two** places, and they must agree:

```rust playground
macro_rules! demo {
    //  v-- matcher: how we PARSE the input
    ( $( $x:expr ),* ) => {
        //  v-- transcriber (body): what we EMIT, once per $x
        vec![ $( $x * 2 ),* ]
    };
}

fn main() {
    println!("{:?}", demo!(1, 2, 3)); // [2, 4, 6]
}
```

The compiler binds `$x` to each matched expression in the matcher, then walks the body's `$( ... )*` once per binding, substituting that iteration's `$x`. Because `$x` is named *inside* the repetition, you must also *use* it inside a repetition in the body; referencing `$x` outside any `$( ... )` is an error.

### The three separators in practice

```rust
fn main() {
    // Comma-separated (the most common):
    let a = comma_list!(1, 2, 3);
    // Semicolon-separated:
    let b = semi_list!(1; 2; 3);
    // No separator at all (whitespace only) — valid but unusual:
    let c = space_list!(1 2 3);

    println!("{:?} {:?} {:?}", a, b, c);
}

macro_rules! comma_list { ( $( $x:expr ),* ) => { vec![ $( $x ),* ] }; }
macro_rules! semi_list  { ( $( $x:expr );* ) => { vec![ $( $x ),* ] }; }
macro_rules! space_list { ( $( $x:expr )*  ) => { vec![ $( $x ),* ] }; }
```

This prints:

```text
[1, 2, 3] [1, 2, 3] [1, 2, 3]
```

The separator in the **matcher** controls how callers must punctuate the input. The separator in the **body** (here always `,`) controls how the emitted output is punctuated; they are independent.

### Building a faithful `vec!` clone

The real `vec!` macro has *two* rules: one for the list form `vec![a, b, c]` and one for the repeat form `vec![value; count]`. Here is a working re-implementation that mirrors how the standard library defines it:

```rust playground
macro_rules! myvec {
    // Repeat form: [elem; count]
    ( $elem:expr ; $count:expr ) => {
        ::std::vec::from_elem($elem, $count)
    };
    // List form: [a, b, c] with an optional trailing comma
    ( $( $x:expr ),* $(,)? ) => {
        <[_]>::into_vec(::std::boxed::Box::new([ $( $x ),* ]))
    };
}

fn main() {
    let a = myvec![1, 2, 3];
    let b = myvec![0u8; 4];
    println!("a = {:?}", a); // a = [1, 2, 3]
    println!("b = {:?}", b); // b = [0, 0, 0, 0]
}
```

Output:

```text
a = [1, 2, 3]
b = [0, 0, 0, 0]
```

A few things worth unpacking:

- **Order of rules matters.** `macro_rules!` tries rules top to bottom. The repeat form (`$elem:expr ; $count:expr`) is listed first; if the input does not contain a `;`, that rule fails to match and Rust falls through to the list form.
- **`::std::...` paths** make the macro reliable no matter what the caller has (or hasn't) imported. This is good hygiene for a reusable macro, covered more in [Macro Basics](/14-macros/00-macro-basics/).
- The list form expands `vec![1, 2, 3]` into `[1, 2, 3]` (an array), boxes it, and turns it into a `Vec`: exactly the strategy the standard library uses so the elements are placed directly into the allocation.

### Nested repetition

Repetitions can nest, which is how you express "a list of lists." Each `$( ... )` level corresponds to one level of grouping:

```rust playground
// A grid: rows separated by `;`, cells within a row separated by `,`.
macro_rules! grid {
    ( $( $( $cell:expr ),+ );+ $(,)? ) => {
        vec![ $( vec![ $( $cell ),+ ] ),+ ]
    };
}

fn main() {
    let g = grid![
        1, 2, 3;
        4, 5, 6;
        7, 8, 9
    ];
    println!("{:?}", g);
}
```

Output:

```text
[[1, 2, 3], [4, 5, 6], [7, 8, 9]]
```

The outer `$( ... );+` iterates over rows; the inner `$( $cell:expr ),+` iterates over cells within the row currently being expanded. Note both use `+` (one-or-more), so an empty grid or an empty row is rejected at compile time.

### Pairing two repetitions of the same length

When the body expands two metavariables together inside one `$( ... )`, Rust steps through them **in lockstep**: they must have the same number of items:

```rust playground
macro_rules! zip_print {
    ( [ $( $a:expr ),* ] , [ $( $b:expr ),* ] ) => {
        $(
            println!("{} -> {}", $a, $b);
        )*
    };
}

fn main() {
    zip_print!(["a", "b", "c"], [1, 2, 3]);
}
```

Output:

```text
a -> 1
b -> 2
c -> 3
```

This is the macro-level equivalent of zipping two arrays, but the "zip" happens at compile time, and a length mismatch is a *compile error*, not a runtime surprise (see [Common Pitfalls](#common-pitfalls)).

---

## Key Differences

### Compile time vs. runtime

| Aspect              | TypeScript `...args`                       | Rust `$( ... )*` repetition                    |
| ------------------- | ------------------------------------------ | ---------------------------------------------- |
| When it runs        | Runtime: builds a real array, loops        | Compile time: stamps out code, no loop emitted |
| Per-item cost       | One loop iteration each call               | Zero — the code is generated inline            |
| Argument types      | All collected into one array type          | Each `$x:expr` can be a *different* type        |
| Empty case          | Empty array, fine                          | `*` allows empty; `+` rejects it at compile    |
| Wrong arity         | Silent (extra args ignored / `undefined`)  | Compile error                                  |
| Trailing comma      | Allowed by the parser                      | Only if you write `$(,)?` explicitly           |

### Each item can be a different type

Because the macro generates separate code for each item *before* type checking, a single call can mix types as long as the generated code type-checks. With `string_list!`, every `$x.to_string()` only requires that `$x` implement `Display`/`ToString`, so `&str`, `i32`, and `bool` can all appear in the same call. TypeScript's rest parameter, by contrast, forces a single element type (or a union) on the whole array.

### `*` is not a runtime loop

This is the mental-model shift that trips up TypeScript developers most. `$( out.push($x); )*` is **not** a `for` loop. It is a *template* the compiler unrolls. `string_list!["a", "b", "c"]` literally becomes:

```rust
{
    let mut out: Vec<String> = Vec::new();
    out.push("a".to_string());
    out.push("b".to_string());
    out.push("c".to_string());
    out
}
```

There is no iterator, no closure, and no array of arguments at runtime: just three statements.

### Counting items at compile time

There is no built-in "length of repetition" operator, but a common idiom expands each item into a `1usize` and sums them, letting you pre-size a `Vec`:

```rust playground
// Count metavariables by mapping each to `1usize` and summing.
macro_rules! count {
    () => (0usize);
    ( $head:expr $(, $tail:expr )* ) => (1usize + count!( $( $tail ),* ));
}

macro_rules! sized_vec {
    ( $( $x:expr ),* $(,)? ) => {{
        let mut v = Vec::with_capacity(count!( $( $x ),* ));
        $( v.push($x); )*
        v
    }};
}

fn main() {
    let v = sized_vec![10, 20, 30];
    println!("len={} cap={} {:?}", v.len(), v.capacity(), v);
}
```

Output:

```text
len=3 cap=3 [10, 20, 30]
```

The recursive `count!` is itself a small macro. Recursion is a standard `macro_rules!` technique, and the patterns behind it (the `:expr` fragment specifier, multiple rules) live in [Macro Patterns](/14-macros/02-macro-patterns/).

---

## Common Pitfalls

### Pitfall 1: Mismatched repetition lengths

When two metavariables are expanded together in one repetition, they must have equal counts. This does **not** compile:

```rust
macro_rules! zip_bad {
    ( [ $( $a:expr ),* ] , [ $( $b:expr ),* ] ) => {
        // does not compile: $a and $b expanded together but differ in length
        $( println!("{} {}", $a, $b); )*
    };
}

fn main() {
    zip_bad!(["a", "b"], [1, 2, 3]);
}
```

The compiler reports the exact mismatch:

```text
error: meta-variable `a` repeats 2 times, but `b` repeats 3 times
 --> src/main.rs:4:10
  |
4 |         $( println!("{} {}", $a, $b); )*
  |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The fix is to ensure both lists carry the same number of items at the call site, or to expand them in separate `$( ... )` groups if they are genuinely independent.

### Pitfall 2: Using `+` then passing zero items

`+` means *one or more*. Calling such a macro with nothing produces a real error:

```rust
macro_rules! at_least_one {
    ( $( $x:expr ),+ ) => {
        vec![ $( $x ),+ ]
    };
}

fn main() {
    let v: Vec<i32> = at_least_one![]; // does not compile
    println!("{:?}", v);
}
```

The actual message:

```text
error: unexpected end of macro invocation
 --> src/main.rs:8:23
  |
1 | macro_rules! at_least_one {
  | ------------------------- when calling this macro
...
8 |     let v: Vec<i32> = at_least_one![];
  |                       ^^^^^^^^^^^^^^^ missing tokens in macro arguments
  |
note: while trying to match meta-variable `$x:expr`
 --> src/main.rs:2:10
  |
2 |     ( $( $x:expr ),+ ) => {
  |          ^^^^^^^
```

If an empty invocation should be legal, use `*` instead of `+`.

### Pitfall 3: Wrong separator at the call site

The matcher's separator is mandatory and exact. A macro that matches commas will reject semicolons:

```rust
macro_rules! commas {
    ( $( $x:expr ),* ) => { vec![ $( $x ),* ] };
}

fn main() {
    let v = commas!(1; 2; 3); // does not compile: wrong separator
    println!("{:?}", v);
}
```

Real error:

```text
error: no rules expected `;`
 --> src/main.rs:5:22
  |
1 | macro_rules! commas {
  | ------------------- when calling this macro
...
5 |     let v = commas!(1; 2; 3);  // wrong separator
  |                      ^ no rules expected this token in macro call
  |
  = note: while trying to match `,`
```

### Pitfall 4: Forgetting the optional trailing comma

Rust does **not** accept a trailing comma in a repetition unless you allow it. `string_list!["a", "b",]` only works because the matcher ends with `$(,)?`. Without that, the trailing comma is an unexpected token. Adding `$(,)?` after the repetition is cheap and matches what callers expect from `vec!`, so most production macros include it.

### Pitfall 5: Referencing a metavariable outside its repetition

If `$x` is bound *inside* `$( ... )`, you can only use it *inside* a matching `$( ... )` in the body. Writing `$x` at the top level produces an error like ``variable `x` is still repeating at this depth``. Keep the depth of use equal to the depth of binding.

---

## Best Practices

### Always support a trailing comma

Match `vec!`'s ergonomics by ending list-style matchers with `$(,)?`:

```rust
macro_rules! list {
    ( $( $x:expr ),* $(,)? ) => { vec![ $( $x ),* ] };
}
```

This makes multi-line invocations diff-friendly: adding a line never forces editing the previous line's punctuation.

### Choose `*` vs. `+` by intent

Use `*` when an empty invocation is meaningful (an empty collection). Use `+` only when "at least one" is a genuine requirement and an empty call should be a *compile error*, not a silent empty result.

### Prefer separators over no separator

Although `$( $x:expr )*` (whitespace-only) compiles, it reads poorly and is fragile. Real DSLs read far better with explicit `,` or `;` separators that match how a human would write the data.

### Use fully-qualified paths in reusable macros

Macros are expanded wherever they are called, so they should not assume the caller imported anything. Write `::std::collections::HashMap` rather than `HashMap`, `::std::vec::Vec`, and so on. (Macro hygiene and path resolution are covered in [Macro Basics](/14-macros/00-macro-basics/).)

### Reach for a declarative macro only when a function won't do

Repetition macros shine for *variadic* construction and *literal* DSLs. If your arguments are uniform and runtime-known, a function taking a slice (`&[T]`) or an `IntoIterator` is simpler, easier to debug, and just as fast. Macros earn their keep when you need a different *number* or *kind* of arguments than a function signature allows. See [Macro Basics](/14-macros/00-macro-basics/) for the "when to reach for a macro" checklist.

---

## Real-World Example

A genuinely useful application of repetition is generating **table-driven tests**. In TypeScript you might loop over an array of cases inside a single `it.each(...)`. In Rust, a small macro can stamp out a *separate* `#[test]` function per row, so each case shows up individually in the test runner with its own name.

```rust playground
// A slug generator we want to test thoroughly.
fn slugify(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// Generate one #[test] fn per row: `name: input => expected`.
macro_rules! slug_tests {
    ( $( $name:ident : $input:expr => $expected:expr ),+ $(,)? ) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(slugify($input), $expected);
            }
        )+
    };
}

slug_tests! {
    basic:        "Hello World"         => "hello-world",
    punctuation:  "Rust & TypeScript!"  => "rust-typescript",
    trim_edges:   "  spaced out  "      => "spaced-out",
}

fn main() {
    println!("{}", slugify("Hello, Reader!")); // hello-reader
}
```

Running `cargo test` produces three independently-named tests:

```text
running 3 tests
test basic ... ok
test punctuation ... ok
test trim_edges ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Why this is better than a runtime loop:

- Each case is its own test, so a failure names exactly which row broke and the others still run.
- The `name: input => expected` shape is a tiny DSL — `=>` is just a literal token the matcher consumes between two `$expr` fragments.
- `$name:ident` binds an identifier used as a function name; `$( ... ),+ $(,)?` accepts one-or-more rows with an optional trailing comma.

> **Tip:** This pattern is exactly what crates like `test-case` and `rstest` formalize. For everyday testing it is often enough to hand-roll a small macro like this. See [Testing](/13-testing/) for the broader testing toolkit.

---

## Further Reading

### Official Documentation

- [The Rust Reference — Macros By Example (repetitions)](https://doc.rust-lang.org/reference/macros-by-example.html#repetitions)
- [The Rust Book — Ch. 20.5: Macros](https://doc.rust-lang.org/book/ch20-05-macros.html)
- [The Little Book of Rust Macros — Repetition](https://veykril.github.io/tlborm/decl-macros/macros-methodical.html#repetitions)
- [`std::vec!` macro source and docs](https://doc.rust-lang.org/std/macro.vec.html)

### Related Sections in This Guide

- [Macro Basics](/14-macros/00-macro-basics/) — what macros are (and are *not*), compile-time expansion, hygiene.
- [Declarative Macros with `macro_rules!`](/14-macros/01-declarative-macros/) — `macro_rules!` fundamentals and `cargo expand`.
- [Macro Patterns](/14-macros/02-macro-patterns/) — fragment specifiers (`:expr`, `:ident`, `:ty`, `:tt`) and multiple rules.
- [Function-Like Procedural Macros](/14-macros/06-function-like-macros/) — invocation-style macros like `vec!` and `println!` and when to reach for them.
- [Output and Formatting](/02-basics/04-output/) — `println!`/`format!`, your first taste of macro invocation syntax.
- [Serialization](/15-serialization/) — `serde_json::json!` builds JSON with a similar repetition-based DSL.

---

## Exercises

### Exercise 1

**Difficulty:** Easy

**Objective:** Write a comma-separated repetition with a trailing comma.

**Instructions:** Implement a macro `set!` that takes zero-or-more comma-separated expressions and builds a `std::collections::HashSet`, deduplicating values. Support an optional trailing comma. Calling `set![1, 2, 2, 3, 3, 3]` should yield a set containing `{1, 2, 3}`.

```rust
macro_rules! set {
    // TODO: match zero-or-more comma-separated exprs with optional trailing comma,
    //       insert each into a HashSet.
}

fn main() {
    let s = set![1, 2, 2, 3, 3, 3];
    let mut sorted: Vec<_> = s.into_iter().collect();
    sorted.sort();
    println!("{:?}", sorted); // [1, 2, 3]
}
```

<details>
<summary>Solution</summary>

```rust playground
macro_rules! set {
    ( $( $x:expr ),* $(,)? ) => {{
        let mut s = ::std::collections::HashSet::new();
        $( s.insert($x); )*
        s
    }};
}

fn main() {
    let s = set![1, 2, 2, 3, 3, 3];
    let mut sorted: Vec<_> = s.into_iter().collect();
    sorted.sort();
    println!("{:?}", sorted); // [1, 2, 3]
}
```

The `$( ... ),*` matches the list, `$(,)?` permits the trailing comma, and the
fully-qualified `::std::collections::HashSet` keeps the macro usable without an
import. Running it prints `[1, 2, 3]`.

</details>

### Exercise 2

**Difficulty:** Medium

**Objective:** Use `=>` as a literal separator token inside a repetition to build a key/value DSL.

**Instructions:** Implement a macro `hashmap!` that accepts entries written as `key => value`, comma-separated, with an optional trailing comma, and produces a `std::collections::HashMap`. Calling `hashmap!{ "alice" => 95, "bob" => 87 }` should build a map where `"alice"` maps to `95`.

```rust
macro_rules! hashmap {
    // TODO: match `$key:expr => $val:expr` entries, comma-separated, optional trailing comma.
}

fn main() {
    let scores = hashmap!{ "alice" => 95, "bob" => 87 };
    println!("{}", scores["alice"]); // 95
}
```

<details>
<summary>Solution</summary>

```rust playground
macro_rules! hashmap {
    ( $( $key:expr => $val:expr ),* $(,)? ) => {{
        let mut map = ::std::collections::HashMap::new();
        $( map.insert($key, $val); )*
        map
    }};
}

fn main() {
    let scores = hashmap!{ "alice" => 95, "bob" => 87 };
    println!("{}", scores["alice"]); // 95
}
```

`=>` is not special here; it is simply a literal token the matcher requires
between the two `$expr` fragments of each entry. The repetition then iterates
over entries, emitting one `map.insert(key, val);` per pair. Running it prints
`95`.

</details>

### Exercise 3

**Difficulty:** Hard

**Objective:** Combine nested repetition with the count idiom to build and pre-size a structure.

**Instructions:** Implement a macro `matrix!` that takes rows separated by `;`, each row a `,`-separated list of `i32`, and returns a `Vec<Vec<i32>>` where each inner row is pre-allocated with the correct capacity. Use a `count!` helper macro to size each row's `Vec`. Both rows and cells must require at least one element (`+`). Calling `matrix![1, 2, 3; 4, 5, 6]` should produce `[[1, 2, 3], [4, 5, 6]]`, and each inner `Vec` should have `capacity == 3`.

```rust
macro_rules! count {
    // TODO: recursively count items, mapping each to 1usize.
}

macro_rules! matrix {
    // TODO: nested repetition; use count! to pre-size each row.
}

fn main() {
    let m = matrix![1, 2, 3; 4, 5, 6];
    println!("{:?}", m);
    println!("row0 cap = {}", m[0].capacity()); // 3
}
```

<details>
<summary>Solution</summary>

```rust playground
macro_rules! count {
    () => (0usize);
    ( $head:expr $(, $tail:expr )* ) => (1usize + count!( $( $tail ),* ));
}

macro_rules! matrix {
    ( $( $( $cell:expr ),+ );+ $(,)? ) => {
        vec![
            $(
                {
                    let mut row = Vec::with_capacity(count!( $( $cell ),+ ));
                    $( row.push($cell); )+
                    row
                }
            ),+
        ]
    };
}

fn main() {
    let m = matrix![1, 2, 3; 4, 5, 6];
    println!("{:?}", m);              // [[1, 2, 3], [4, 5, 6]]
    println!("row0 cap = {}", m[0].capacity()); // 3
}
```

The outer `$( ... );+` iterates over rows, the inner `$( $cell:expr ),+`
iterates over cells. Inside each row's block, `count!( $( $cell ),+ )` recounts
just *that* row's cells (the inner repetition expands per-row), so
`Vec::with_capacity` is sized exactly. Running it prints `[[1, 2, 3], [4, 5, 6]]`
followed by `row0 cap = 3`.

</details>
