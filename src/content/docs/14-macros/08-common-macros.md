---
title: "Common Standard-Library Macros"
description: "Rust's everyday macros (vec!, println!, format!, write!, matches!, assert!, dbg!, todo!, include_str!) mapped to the TS and JS features you already know."
---

## Quick Overview

Rust's standard library ships a small toolbox of **built-in macros** — `vec!`, `println!`, `format!`, `write!`, `matches!`, the `assert*!` family, `todo!`, `unimplemented!`, `dbg!`, `include_str!`, and friends — that you will reach for constantly. They look like function calls but are expanded at compile time, which is exactly why they can do things ordinary functions cannot: take a variable number of typed arguments, check format strings at compile time, build a `Vec` of any element type, or read a file into your binary before the program ever runs. This page is a practical tour of the ones a TypeScript/JavaScript developer meets in the first week of writing Rust.

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects the newest edition automatically. Every Rust snippet here was compiled and run on stable.

> **Tip:** These are *uses* of macros, not lessons in *writing* them. For the mental model of what a macro is (and is not), see [Macro Basics](/14-macros/00-macro-basics/); to write your own, see [Declarative Macros with `macro_rules!`](/14-macros/01-declarative-macros/) and [Procedural Macros](/14-macros/07-proc-macros/).

---

## TypeScript/JavaScript Example

In TypeScript/JavaScript the equivalents are scattered across language features and standard library functions, none of which run at compile time. You build arrays with literals, format with template literals or helpers, accumulate strings by concatenation, test shapes with `typeof`/`instanceof`, and assert with `console.assert` (which famously does **not** throw).

```typescript
// Build a list — an array literal.
const evens: number[] = [2, 4, 6, 8];
const zeros: number[] = new Array(4).fill(0);

// Format / print — template literals + console.log, and a returned string.
const name = "Ada";
const score = 0.8137;
console.log(`Hello, ${name}!`);
console.log(score.toFixed(2));          // "0.81"
console.log("right".padStart(8));       // "   right"
const pct = (score * 100).toFixed(0);
const summary = `${name} scored ${pct}%`; // build a string, don't print it

// Accumulate text into a buffer — string concatenation.
let report = "";
["Berlin", "Lyon"].forEach((city, i) => {
  report += `${i}: ${city}\n`;
});

// Test the shape of a value — typeof / instanceof / a discriminant check.
type Event =
  | { kind: "click"; x: number; y: number }
  | { kind: "key"; code: string }
  | { kind: "close" };
const e: Event = { kind: "key", code: "q" };
const isQuit = e.kind === "key" && e.code === "q";

// Assert an invariant — console.assert does NOT throw; it just logs and continues.
console.assert(evens.length > 0, "expected non-empty");

// "Not done yet" — throw a placeholder.
function render(width: number): string {
  throw new Error("not implemented");
}

// Quick debug print — console.log returns undefined, so you can't inline it cleanly.
const doubled = evens.map((n) => n * 2);
console.log("doubled", doubled);
```

Three things to carry into Rust. First, `console.assert` **logs and keeps running** — it is not a hard stop. Second, `console.log(obj)` prints a structured view like `{ name: 'Bob' }`, not `[object Object]`. Third, none of this happens at compile time: a template literal, a `.fill()`, a `throw` — all run when the program runs.

---

## Rust Equivalent

The same jobs map onto standard macros. Each one is expanded into ordinary, fully type-checked Rust at compile time.

```rust
use std::fmt::Write as _; // brings the write!/writeln! target trait into scope

fn main() {
    // vec! — build a Vec. Two forms: a list, or "value; count".
    let evens: Vec<i32> = vec![2, 4, 6, 8];
    let zeros = vec![0u8; 4]; // [0, 0, 0, 0]
    println!("evens = {evens:?}");
    println!("zeros = {zeros:?}");

    // println! / format! — formatted output. format! returns a String instead of printing.
    let name = "Ada";
    let score = 0.8137;
    println!("Hello, {name}!");      // inline capture of `name`
    println!("{score:.2}");          // 0.81  — two decimals
    println!("{:>8}", "right");      // right-aligned in 8 columns
    let pct = score * 100.0;
    let summary = format!("{name} scored {pct:.0}%"); // build, don't print
    println!("built string = {summary}");

    // writeln! — like println! but writes into a buffer (here, a String).
    let mut report = String::new();
    for (i, city) in ["Berlin", "Lyon"].iter().enumerate() {
        writeln!(report, "{i}: {city}").unwrap();
    }
    print!("{report}");

    // matches! — test whether a value matches a pattern, returns bool.
    let e = Event::Key('q');
    let is_quit = matches!(e, Event::Key('q'));
    println!("is_quit = {is_quit}");

    // assert! family — panics (aborts the thread) if the condition is false.
    assert!(!evens.is_empty(), "expected non-empty");
    assert_eq!(evens.len(), 4);

    // dbg! — print "file:line = value" to stderr AND return the value, so it inlines.
    let doubled: Vec<i32> = evens.iter().map(|n| dbg!(n * 2)).collect();
    println!("doubled = {doubled:?}");
}

#[derive(Debug)]
enum Event {
    Click { x: i32, y: i32 },
    Key(char),
    Close,
}
```

Real output (stdout and stderr interleaved):

```text
evens = [2, 4, 6, 8]
zeros = [0, 0, 0, 0]
Hello, Ada!
0.81
   right
built string = Ada scored 81%
0: Berlin
1: Lyon
is_quit = true
[src/main.rs:37:50] n * 2 = 4
[src/main.rs:37:50] n * 2 = 8
[src/main.rs:37:50] n * 2 = 12
[src/main.rs:37:50] n * 2 = 16
doubled = [4, 8, 12, 16]
```

The big behavioral contrast jumps out immediately: Rust's `assert!` **panics** (a hard stop), unlike `console.assert` which merely logs. And `dbg!` returns its argument, so you can wrap any subexpression without restructuring your code.

---

## Detailed Explanation

### `vec!` — make a `Vec`

`vec![]` has two forms, both expanded at compile time:

- `vec![a, b, c]` — a comma list of elements (trailing comma allowed).
- `vec![value; count]` — `count` copies of `value` (the value must be `Clone`).

A plain function could not provide both forms with a single name and arbitrary arity; the macro can, because it expands to different code per call site. There is nothing magic about the element type; it is inferred or annotated like any other value. (For how a `vec!`-style macro is built from repetition, see [Macro Repetition](/14-macros/03-repetition/).)

### `println!` / `print!` / `eprintln!` / `format!` — formatting

These share one **format string** mini-language. The key rules:

- The first argument **must be a string literal**; it is parsed at compile time. You cannot pass a `String` variable as the format string.
- `{}` uses the `Display` trait (human-readable); `{:?}` uses `Debug` (developer-readable); `{:#?}` is pretty-printed `Debug`.
- **Inline captures**: `{name}` reads a variable named `name` in scope. This is the modern, preferred style (stable since Rust 1.58) and replaces the older `"{}", name` positional form for simple cases.
- **Format specs** go after a colon: `{score:.2}` (2 decimals), `{:>8}` (right-align, width 8), `{:08.3}` (zero-pad to width 8 with 3 decimals), `{n:b}` (binary), `{n:x}` (hex).
- `format!` returns a `String`; `println!`/`print!` write to stdout; `eprintln!`/`eprint!` write to stderr; `panic!` uses the same syntax to build its message.

This is the rough analogue of template literals, but checked at compile time: a `{}` with no matching argument is a compile error, not a silent `undefined`.

### `write!` / `writeln!` — formatted output into a target

`write!(target, "...", ...)` uses the same format language but writes into a `target` instead of stdout. The target is anything implementing `std::fmt::Write` (e.g. `String`) or `std::io::Write` (e.g. `Vec<u8>`, a `File`, a socket). Because the write can fail (a file might error), these macros **return a `Result`**, which is why you see `.unwrap()` or `?` after them. This is the idiomatic way to build up text efficiently without repeated `+ "..."` allocations.

### `matches!` — pattern test as a boolean

`matches!(value, PATTERN)` expands to a `match` that returns `true` for the pattern and `false` otherwise. It supports the full pattern grammar — bindings, `|` alternatives, ranges, and `if` guards — so it is far more expressive than a single `===` check. It is the cleanest way to ask "is this value one of these shapes?" without writing a full `match`.

### `assert!`, `assert_eq!`, `assert_ne!` — invariants

- `assert!(cond)` panics if `cond` is false; you may add a custom message: `assert!(cond, "msg {x}", x = x)` (or inline-capture form).
- `assert_eq!(a, b)` / `assert_ne!(a, b)` compare and, on failure, **print both values**. They require the operands to implement `PartialEq` (to compare) and `Debug` (to print on failure).
- `debug_assert!` / `debug_assert_eq!` compile to nothing in release builds. Use them for expensive checks you only want in debug mode.

Unlike `console.assert`, a failed Rust assertion **panics**: it unwinds the current thread (the process exits with a nonzero code unless caught). Assertions are for *bugs* (broken invariants), not for expected runtime errors; those use `Result` (see [08-error-handling](/08-error-handling/)).

### `todo!` / `unimplemented!` / `unreachable!` / `panic!`

These all panic, but communicate different intents:

- `todo!()`: "I will implement this later." Panics with `not yet implemented`.
- `unimplemented!()` — "This is intentionally not supported here." Panics with `not implemented`.
- `unreachable!()`: "Control flow can never reach this point." Panics if it somehow does.
- `panic!("msg")` — unconditional abort with your message.

Importantly, all four **return the never type `!`**, which coerces to *any* type. That is why `todo!()` type-checks as the body of a function that is supposed to return a `String`: the compiler accepts the stub so the rest of your code compiles while you fill in the real logic. A `throw` in TypeScript does not give you that type-level convenience.

### `dbg!` — inspect-and-return debugging

`dbg!(expr)` prints `[file:line:col] expr = value` to **stderr** using `Debug`, then **returns the value**. Because it returns the value, you can wrap any subexpression in place — `let x = dbg!(a + b);` — without breaking the data flow. One gotcha: `dbg!(x)` takes `x` **by value** (it moves non-`Copy` types). Use `dbg!(&x)` to borrow instead. Remove `dbg!` calls before committing; `println!`/`tracing` are for permanent output.

### `include_str!` / `include_bytes!` / `env!` / `concat!` / `stringify!`

These do compile-time work that has no TypeScript equivalent in the language itself:

- `include_str!("path")` reads a file **at compile time** and embeds its contents as a `&'static str` baked into the binary. `include_bytes!` does the same as `&'static [u8]`. Paths are relative to the current source file.
- `env!("VAR")` reads an environment variable **at compile time** (compile error if missing). `option_env!` yields `Option` instead.
- `concat!("a", "b")` joins literals into one `&'static str` at compile time.
- `stringify!(tokens)` turns the literal source tokens into a string **without evaluating them**: `stringify!(1 + 2)` is `"1 + 2"`, not `"3"`.

```rust
fn main() {
    // include_str! bakes a file's contents into the binary at compile time.
    const BANNER: &str = include_str!("../banner.txt");
    print!("{BANNER}");

    let label = stringify!(1 + 2 * 3); // tokens, not the result
    println!("label = {label}");

    println!("built crate: {}", env!("CARGO_PKG_NAME"));
    const GREETING: &str = concat!("Hello", ", ", "world!");
    println!("{GREETING}");
}
```

With a `banner.txt` next to the binary's source containing `== ACME CLI v1.0 ==` followed by a trailing newline (the only newline `print!` emits here comes from the file itself), this prints:

```text
== ACME CLI v1.0 ==
label = 1 + 2 * 3
built crate: probe
Hello, world!
```

---

## Key Differences

| Task | TypeScript/JavaScript | Rust macro | When it happens |
| --- | --- | --- | --- |
| Build a list | `[2, 4, 6, 8]` array literal | `vec![2, 4, 6, 8]` | compile-time expansion |
| `n` copies | `new Array(4).fill(0)` | `vec![0u8; 4]` | compile-time expansion |
| Print formatted | `console.log(\`${name}\`)` | `println!("{name}")` | format string checked at compile time |
| Build a string | template literal | `format!("{name}")` | compile-time-checked, runtime-built |
| Append to buffer | `s += "..."` | `write!(&mut s, "...")` | returns `Result` |
| Shape test | `e.kind === "key"` | `matches!(e, Event::Key(_))` | full pattern grammar |
| Assert invariant | `console.assert` (logs, continues) | `assert!` (**panics**) | runtime panic on failure |
| Placeholder | `throw new Error("todo")` | `todo!()` (type `!`) | coerces to any return type |
| Inline debug | `console.log(x)` (returns `undefined`) | `dbg!(x)` (**returns x**) | inlines anywhere |
| Embed a file | read at runtime (`fs.readFileSync`) | `include_str!` (**compile time**) | baked into binary |

Three differences are worth internalizing:

1. **Format strings are checked at compile time.** `println!("{a} {b}", a = 1)` with no `b` is a compile error, not a runtime `undefined`. This is closer to a typed `printf` than to a template literal.
2. **Assertions panic.** A failed `assert!` aborts the thread; it is for catching *bugs*, whereas TypeScript's `console.assert` is a soft log. Recoverable conditions belong in `Result`, not assertions.
3. **`include_str!`/`env!` run during compilation.** The data is in the binary; there is no file read or environment lookup at runtime. Node has nothing equivalent at the language level.

---

## Common Pitfalls

### Pitfall 1: passing a variable as the format string

The first argument to `println!`/`format!` must be a **string literal** so the compiler can parse it.

```rust
fn main() {
    let msg = String::from("hello {name}");
    let name = "world";
    println!(msg); // does not compile
    let _ = name;
}
```

Real compiler error:

```text
error: format argument must be a string literal
 --> src/main.rs:4:14
  |
4 |     println!(msg); // does not compile
  |              ^^^
  |
help: you might be missing a string literal to format with
  |
4 |     println!("{}", msg); // does not compile
  |              +++++
```

Fix: `println!("{msg}")` (or `println!("{}", msg)`). If you genuinely need a runtime-chosen format, that is not what `println!` does; build the string yourself.

### Pitfall 2: `dbg!` moves its argument

`dbg!(x)` takes ownership of `x` (it returns the value). For a non-`Copy` type, that moves it.

```rust
fn main() {
    let v = vec![1, 2, 3];
    dbg!(v);
    println!("{}", v.len()); // does not compile
}
```

Real compiler error (abridged):

```text
error[E0382]: borrow of moved value: `v`
 --> src/main.rs:4:20
  |
2 |     let v = vec![1, 2, 3];
  |         - move occurs because `v` has type `Vec<i32>`, which does not implement the `Copy` trait
3 |     dbg!(v);
  |     ------- value moved here
4 |     println!("{}", v.len()); // does not compile
  |                    ^ value borrowed here after move
  |
help: consider borrowing instead of transferring ownership
  |
3 |     dbg!(&v);
  |          +
```

Fix: borrow with `dbg!(&v)`, or use the value `dbg!` returns (`let v = dbg!(v);`).

### Pitfall 3: `assert_eq!` needs `PartialEq` and `Debug`

`assert_eq!` compares with `==` (needs `PartialEq`) and prints both sides on failure (needs `Debug`).

```rust
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let a = Point { x: 1, y: 2 };
    let b = Point { x: 1, y: 2 };
    assert_eq!(a, b); // does not compile: Point implements neither trait
}
```

Real compiler errors (abridged):

```text
error[E0369]: binary operation `==` cannot be applied to type `Point`
 --> src/main.rs:9:5
  |
9 |     assert_eq!(a, b); // does not compile: Point implements neither trait
  |     ^^^^^^^^^^^^^^^^
  ...
help: consider annotating `Point` with `#[derive(PartialEq)]`

error[E0277]: `Point` doesn't implement `Debug`
 --> src/main.rs:9:5
  ...
help: consider annotating `Point` with `#[derive(Debug)]`
```

Fix: `#[derive(Debug, PartialEq)]` on `Point`. This is the same trait requirement you hit when printing with `{:?}`; see [Derive Macros](/14-macros/04-derive-macros/).

### Pitfall 4: expecting `assert!` to behave like `console.assert`

In Node, a failed `console.assert` logs `Assertion failed: ...` and the program keeps running. In Rust, a failed `assert!` **panics** and unwinds the thread. Do not use assertions for input validation or recoverable errors; reach for `Result` / `?` instead (see [08-error-handling](/08-error-handling/)). Reserve `assert!` for internal invariants that, if violated, mean your code has a bug.

### Pitfall 5: forgetting the `Write` trait import for `write!`

`write!(my_string, ...)` will not compile unless `std::fmt::Write` is in scope, even though `String` is built in. The macro calls the trait's `write_str`/`write_fmt` methods, and trait methods require the trait to be imported. The compiler error says ``no method named `write_fmt` found``. Fix: `use std::fmt::Write;` (for `String`) or `use std::io::Write;` (for `Vec<u8>`/files).

---

## Best Practices

- **Prefer inline captures.** `println!("{name} = {value:?}")` is clearer than `println!("{} = {:?}", name, value)`. Use positional/named args only when an argument is a complex expression.
- **Use `format!` to build, the `print` family to emit.** Do not `println!` into the void and then reconstruct; build a `String` with `format!`/`write!`, then print or return it.
- **Use `write!`/`writeln!` for loops.** Accumulating with `s += &format!(...)` allocates repeatedly; `write!(&mut s, ...)` writes in place.
- **`matches!` for boolean shape checks; full `match` when you need the bound data.** If you immediately want the inner values, write a `match` or `if let`.
- **Assertions are for bugs.** Use `assert!`/`assert_eq!` for invariants and in tests; use `Result` for anything a caller could reasonably hit at runtime. Use `debug_assert!` for checks too expensive for release builds.
- **`todo!()` over `unimplemented!()` while iterating.** Both panic, but `todo!()` signals "coming soon" and pairs naturally with the `!` type so stubs compile. Reach for `unimplemented!()` only when a branch is intentionally unsupported.
- **Strip `dbg!` before committing.** It writes to stderr and is meant to be transient. For lasting, structured diagnostics use the `tracing` crate or `eprintln!`.
- **`include_str!` for embedding assets, not config that should be editable at runtime.** It bakes the file into the binary; changing the file requires a rebuild.

---

## Real-World Example

A small trade-report builder that exercises the macros together: `include_str!` bakes in a header, `writeln!` accumulates lines into a `String`, `format!`-style specs align columns, `matches!` classifies a value, `assert!` guards an invariant, and `eprintln!` emits a diagnostic to stderr.

```rust
use std::fmt::Write as _;

#[derive(Debug, Clone)]
struct Trade {
    symbol: String,
    qty: i64,
    price_cents: i64,
}

/// Build a plain-text report, accumulating into one String with writeln!.
fn build_report(trades: &[Trade]) -> String {
    let mut out = String::new();

    // Header template is embedded into the binary at compile time.
    out.push_str(include_str!("../report_header.txt"));

    let mut total_cents: i64 = 0;
    for t in trades {
        let line_cents = t.qty * t.price_cents;
        total_cents += line_cents;
        writeln!(
            out,
            "{symbol:<6} {qty:>5} @ {price:>8.2}  = {line:>10.2}",
            symbol = t.symbol,
            qty = t.qty,
            price = t.price_cents as f64 / 100.0,
            line = line_cents as f64 / 100.0,
        )
        .expect("writing to a String never fails");
    }

    writeln!(out, "{:-<33}", "").unwrap();
    writeln!(out, "TOTAL {:>26.2}", total_cents as f64 / 100.0).unwrap();
    out
}

fn classify(qty: i64) -> &'static str {
    // matches! reads cleanly as a series of boolean range tests.
    if matches!(qty, i64::MIN..=0) {
        "non-positive"
    } else if matches!(qty, 1..=100) {
        "small"
    } else {
        "large"
    }
}

fn main() {
    let trades = vec![
        Trade { symbol: "AAPL".into(), qty: 10, price_cents: 19042 },
        Trade { symbol: "MSFT".into(), qty: 4, price_cents: 41310 },
    ];

    // Guard an invariant: a positive quantity. A violation here means a bug upstream.
    for t in &trades {
        assert!(t.qty > 0, "qty must be positive for {}, got {}", t.symbol, t.qty);
    }

    let report = build_report(&trades);
    print!("{report}");

    println!("\nfirst order is {}", classify(trades[0].qty));

    // Diagnostics go to stderr so they never pollute the report on stdout.
    eprintln!("[debug] processed {} trades", trades.len());
}
```

With a `report_header.txt` (next to the source) containing `=== Trade Report ===` followed by a trailing newline (the header is `push_str`-ed verbatim, so its line break comes from that newline in the file), the program prints to stdout, then the diagnostic to stderr:

```text
=== Trade Report ===
AAPL      10 @   190.42  =    1904.20
MSFT       4 @   413.10  =    1652.40
---------------------------------
TOTAL                    3556.60

first order is small
[debug] processed 2 trades
```

Every column alignment, the dashed separator (`{:-<33}` means "pad with `-` to width 33, left-aligned"), and the totals are produced purely by format specs: no manual string padding.

---

## Further Reading

### Official documentation

- [The Rust Book — `println!` and formatting](https://doc.rust-lang.org/book/ch05-02-example-structs.html#adding-useful-functionality-with-derived-traits)
- [`std::fmt`](https://doc.rust-lang.org/std/fmt/): the full format-string syntax (fill, align, width, precision, `{:?}`, `{:#?}`, `{:b}`, `{:x}`)
- [`std::macro` index](https://doc.rust-lang.org/std/#macros): every standard macro: `vec!`, `format!`, `write!`, `matches!`, `assert!`, `dbg!`, `todo!`, `include_str!`, and more
- [`vec!`](https://doc.rust-lang.org/std/macro.vec.html), [`matches!`](https://doc.rust-lang.org/std/macro.matches.html), [`dbg!`](https://doc.rust-lang.org/std/macro.dbg.html), [`todo!`](https://doc.rust-lang.org/std/macro.todo.html), [`include_str!`](https://doc.rust-lang.org/std/macro.include_str.html)

### Related sections in this guide

- [Macro Basics](/14-macros/00-macro-basics/): what a macro is, and why it is **not** a function or a decorator
- [Declarative Macros with `macro_rules!`](/14-macros/01-declarative-macros/): writing your own `macro_rules!` (and `cargo expand`)
- [Macro Repetition](/14-macros/03-repetition/): how a `vec!`-style macro is built from repetition
- [Derive Macros](/14-macros/04-derive-macros/) — `#[derive(Debug, PartialEq)]`, the traits `assert_eq!` and `{:?}` need
- [Function-Like Procedural Macros](/14-macros/06-function-like-macros/) — `foo!(...)` procedural macros vs. these built-ins
- [Procedural Macros](/14-macros/07-proc-macros/) — writing a custom derive with `syn` 2 + `quote`
- [02-basics](/02-basics/): `println!` and formatting basics
- [08-error-handling](/08-error-handling/) — `Result`/`?` for recoverable errors vs. `assert!`/`panic!` for bugs
- [13-testing](/13-testing/): `assert_eq!` and the `Debug` requirement in tests
- [15-serialization](/15-serialization/) — `serde`'s derives for turning structs into JSON

---

## Exercises

### Exercise 1

**Difficulty:** Beginner

**Objective:** Use `format!` with alignment and precision specs to build (not print) a labeled row.

**Instructions:** Implement `row(label, value)` so it returns a `String` where the label is left-aligned in 10 columns and the value is right-aligned in 8 columns with exactly 2 decimal places. For example, `row("balance", 1234.5)` should equal `"balance   :  1234.50"`.

```rust
fn row(label: &str, value: f64) -> String {
    /* ??? */
}

fn main() {
    println!("[{}]", row("balance", 1234.5));
    println!("[{}]", row("fees", 9.0));
}
```

<details>
<summary>Solution</summary>

Use `{label:<10}` for left-align width 10 and `{value:>8.2}` for right-align width 8, precision 2:

```rust
fn row(label: &str, value: f64) -> String {
    format!("{label:<10}: {value:>8.2}")
}

fn main() {
    println!("[{}]", row("balance", 1234.5));
    println!("[{}]", row("fees", 9.0));
}
```

Output:

```text
[balance   :  1234.50]
[fees      :     9.00]
```

`<` is left-align, `>` is right-align, the number after each is the minimum width, and `.2` is the precision. The brackets in `main` just show that the padding is real.

</details>

### Exercise 2

**Difficulty:** Intermediate

**Objective:** Use `matches!` with `|` alternatives and a struct pattern to write a boolean classifier.

**Instructions:** Implement `is_visible(&Status) -> bool` so that a `Status::Active` and any `Status::Suspended { .. }` are visible, but `Status::Deleted` is not. Use a single `matches!` call. Then make the three assertions in `main` pass.

```rust
#[derive(Debug)]
enum Status {
    Active,
    Suspended { reason: String },
    Deleted,
}

fn is_visible(s: &Status) -> bool {
    /* ??? */
}

fn main() {
    assert!(is_visible(&Status::Active));
    assert!(is_visible(&Status::Suspended { reason: "late payment".into() }));
    assert!(!is_visible(&Status::Deleted));
    println!("visibility checks passed");
}
```

<details>
<summary>Solution</summary>

`matches!` accepts the full pattern grammar, so a `|` alternative with a struct pattern (`{ .. }` ignores the fields) does the whole job:

```rust
#[derive(Debug)]
enum Status {
    Active,
    Suspended { reason: String },
    Deleted,
}

fn is_visible(s: &Status) -> bool {
    matches!(s, Status::Active | Status::Suspended { .. })
}

fn main() {
    assert!(is_visible(&Status::Active));
    assert!(is_visible(&Status::Suspended { reason: "late payment".into() }));
    assert!(!is_visible(&Status::Deleted));
    println!("visibility checks passed");
}
```

Output:

```text
visibility checks passed
```

`Status::Suspended { .. }` matches the variant regardless of its `reason` field. If you wanted to *use* the `reason`, you would write a `match` or `if let` instead; `matches!` only yields a `bool`.

</details>

### Exercise 3

**Difficulty:** Intermediate

**Objective:** Combine `writeln!`, `format!` specs, and `assert_eq!` to build and verify a small CSV in memory.

**Instructions:** Implement `to_csv(rows)` so it returns a `String` with a `name,count` header line followed by one `name,count` line per row. Then complete the `assert_eq!` in `main` so it checks that the result has exactly 3 lines for the given input (header + 2 rows). Remember to bring the right trait into scope for `writeln!`.

```rust
// TODO: a use statement is needed here

fn to_csv(rows: &[(&str, u32)]) -> String {
    /* ??? */
}

fn main() {
    let csv = to_csv(&[("apples", 3), ("pears", 7)]);
    print!("{csv}");
    assert_eq!(/* number of lines */, 3);
    println!("csv has the expected number of lines");
}
```

<details>
<summary>Solution</summary>

`writeln!` into a `String` requires `use std::fmt::Write`. Build the header, loop the rows, then count lines with `.lines().count()`:

```rust
use std::fmt::Write as _;

fn to_csv(rows: &[(&str, u32)]) -> String {
    let mut out = String::new();
    writeln!(out, "name,count").unwrap();
    for (name, count) in rows {
        writeln!(out, "{name},{count}").unwrap();
    }
    out
}

fn main() {
    let csv = to_csv(&[("apples", 3), ("pears", 7)]);
    print!("{csv}");
    assert_eq!(csv.lines().count(), 3);
    println!("csv has the expected number of lines");
}
```

Output:

```text
name,count
apples,3
pears,7
csv has the expected number of lines
```

`writeln!` returns a `Result` because the underlying write can fail; for a `String` it never does, so `.unwrap()` is fine here. The `as _` in the import brings the trait's methods into scope without binding the name `Write` (handy when you only need the trait for its methods).

</details>
