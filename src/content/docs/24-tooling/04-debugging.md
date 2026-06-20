---
title: "Debugging Rust"
description: "Rust has no console.log or built-in inspector. Debug with the dbg! macro, RUST_BACKTRACE panic traces, and stepping through LLDB or CodeLLDB in VS Code."
---

When a Node program misbehaves, you reach for `console.log`, the Chrome DevTools debugger, or `node --inspect`. Rust has the same two layers, print-style debugging and a real source-level debugger, but the tools are native (LLDB or GDB) and the print macro, `dbg!`, is purpose-built for the job. This topic shows the full debugging workflow: the `dbg!` macro, reading a panic and its backtrace via `RUST_BACKTRACE`, and stepping through a compiled binary with LLDB on the command line and through the CodeLLDB extension in VS Code.

---

## Quick Overview

**Debugging** in Rust spans two complementary tools. For quick, ad-hoc inspection there is the `dbg!` macro. Think `console.log`, but it prints the file, line, the *source text* of the expression, and the value, then hands the value back so you can wrap an expression inline. For real interactive debugging there is a native **debugger** — **LLDB** or **GDB** — that sets breakpoints, steps through source, and inspects variables in a compiled binary, exactly like the V8 inspector but operating on machine code.

The third pillar is the **panic message and backtrace**. When Rust hits an unrecoverable error it *panics*, prints where it happened, and (if you set the `RUST_BACKTRACE` environment variable) prints the full call stack. This is the closest analog to a JavaScript `Error.stack`, except you have to opt into the stack trace.

> **Note:** This topic is about debugging *bugs you already have*. For measuring *where time goes*, see [Section 21: Profiling](/21-performance/00-profiling/); for structured application logs in production, see [Section 23: Logging](/23-ecosystem/03-logging/). For setting up the VS Code editor itself, see the sibling [Setting Up VS Code for Rust](/24-tooling/06-vscode-setup/).

---

## TypeScript/JavaScript Example

A TypeScript developer debugging a shopping-cart total has three habits. First, sprinkle `console.log`:

```typescript
// cart.ts
interface LineItem {
  name: string;
  unitPriceCents: number;
  quantity: number;
}

function subtotalCents(items: LineItem[]): number {
  let total = 0;
  for (const item of items) {
    total += item.unitPriceCents * item.quantity;
  }
  return total;
}

function totalAfterDiscount(items: LineItem[], discountCents: number): number {
  const subtotal = subtotalCents(items);
  console.log("subtotal:", subtotal); // ad-hoc print debugging
  return subtotal - discountCents;
}

const cart: LineItem[] = [
  { name: "Coffee mug", unitPriceCents: 1299, quantity: 2 },
  { name: "Sticker pack", unitPriceCents: 499, quantity: 1 },
];

console.log(totalAfterDiscount(cart, 500));
```

Second, when an exception escapes, Node prints a stack trace automatically:

```text
Error: discount exceeds subtotal
    at totalAfterDiscount (/app/cart.ts:18:11)
    at Object.<anonymous> (/app/cart.ts:27:13)
```

Third, for anything non-trivial you launch a real debugger (`node --inspect-brk cart.js` and attach Chrome DevTools, or hit **F5** in VS Code with a `launch.json`), set breakpoints, and step line by line while watching variables.

Rust offers an equivalent for each habit. Let's translate them in order.

---

## Rust Equivalent

Here is the same cart in Rust, instrumented with `dbg!` instead of `console.log`. Create it with `cargo new shopping_cart` and paste this into `src/main.rs`:

```rust playground
/// A line item in a shopping cart.
#[derive(Debug, Clone)]
struct LineItem {
    name: String,
    unit_price_cents: u64,
    quantity: u32,
}

/// Sum the cart line items into a subtotal in cents.
fn subtotal_cents(items: &[LineItem]) -> u64 {
    items
        .iter()
        .map(|item| item.unit_price_cents * item.quantity as u64)
        .sum()
}

/// Apply a flat discount.
fn total_after_discount(items: &[LineItem], discount_cents: u64) -> u64 {
    let subtotal = subtotal_cents(items);
    subtotal - discount_cents
}

fn main() {
    let cart = vec![
        LineItem { name: "Coffee mug".into(), unit_price_cents: 1299, quantity: 2 },
        LineItem { name: "Sticker pack".into(), unit_price_cents: 499, quantity: 1 },
    ];

    for item in &cart {
        println!("{} x{}", item.name, item.quantity);
    }

    // dbg! prints file:line, the expression's source text, and its value to
    // stderr, then RETURNS the value so it stays inside the expression.
    let total = dbg!(total_after_discount(&cart, 500));
    println!("Total: ${:.2}", total as f64 / 100.0);
}
```

Running it with `cargo run` produces (the `[src/...]` lines are `dbg!` output on **stderr**; the rest is `println!` on stdout):

```text
Coffee mug x2
Sticker pack x1
[src/main.rs:36:17] total_after_discount(&cart, 500) = 2597
Total: $25.97
```

Notice what `dbg!` gave you that `console.log("subtotal:", subtotal)` did not: the file, line, *and column*, plus the literal text `total_after_discount(&cart, 500)`. You never had to type a label. And because `dbg!` returns its argument, you wrapped it around an existing expression without restructuring the code.

> **Tip:** `#[derive(Debug)]` on `LineItem` is what lets a value be printed with the `{:?}` "debug" formatter that `dbg!` uses internally. Without it, `dbg!(some_line_item)` would not compile. Deriving `Debug` on your own types is the single most useful habit for debuggability. See [Section 06: Structs](/06-data-structures/).

---

## Detailed Explanation

### The `dbg!` macro, line by line

```rust
let total = dbg!(total_after_discount(&cart, 500));
```

- `dbg!` is a **macro** (note the `!`), so it can capture the *source text* of its argument at compile time. `console.log` can never know it was passed `subtotal`; it only sees the value. (Macros are not decorators; see [Section 14: Macros](/14-macros/).)
- It prints to **stderr**, not stdout. That matters: your program's real output (often piped into another tool) stays clean, while debug noise goes to the terminal. With `console.log` everything lands on stdout together.
- It returns the value of the expression, so `let total = dbg!(...)` binds the same `u64` that `total_after_discount(...)` produced. You can even nest it mid-expression: `let avg = dbg!(sum) / count;`.
- The value is printed with the **pretty** debug format (`{:#?}`), so structs and vectors print multi-line and indented.

To inspect a value *without* moving it, borrow inside the macro — `dbg!(&scores)` — exactly as you would pass a reference anywhere else:

```rust playground
fn main() {
    let scores = vec![88, 92, 47, 73];
    dbg!(&scores); // borrow: does NOT take ownership of scores
    let average = dbg!(scores.iter().sum::<i32>()) / scores.len() as i32;
    println!("average = {average}");
}
```

Real output:

```text
[src/main.rs:4:5] &scores = [
    88,
    92,
    47,
    73,
]
[src/main.rs:5:19] scores.iter().sum::<i32>() = 300
average = 75
```

### Panics and `RUST_BACKTRACE`

JavaScript throws exceptions and prints a stack trace for free. Rust's equivalent for *programmer errors* (a bug, not an expected failure) is a **panic**. Our cart had a latent bug: `subtotal - discount_cents` underflows if the discount exceeds the subtotal. In a **debug build**, Rust checks for integer overflow and panics. Run a version where the discount is `1000` on a `499` subtotal and you get:

```text
thread 'main' panicked at src/main.rs:18:5:
attempt to subtract with overflow
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

That tells you *what* (`attempt to subtract with overflow`) and *where* (`src/main.rs:18:5`), but not *how you got there*. Set `RUST_BACKTRACE=1` and rerun:

```bash
RUST_BACKTRACE=1 cargo run
```

```text
thread 'main' panicked at src/main.rs:18:5:
attempt to subtract with overflow
stack backtrace:
   0: __rustc::rust_begin_unwind
   1: core::panicking::panic_fmt
   2: core::panicking::panic_const::panic_const_sub_overflow
   3: shopping_cart::total_after_discount
             at ./src/main.rs:18:5
   4: shopping_cart::main
             at ./src/main.rs:27:17
   5: core::ops::function::FnOnce::call_once
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
```

Read it bottom-up like a JavaScript stack trace: `main` (frame 4) called `total_after_discount` (frame 3), which panicked. Frames 0–2 are the runtime panic machinery. `RUST_BACKTRACE=full` adds the internal `std`/`core` frames; `RUST_BACKTRACE=1` is the readable default you will use 95% of the time.

> **Warning:** Integer overflow only panics in **debug** builds. In a `--release` build the same subtraction *wraps* silently: `499 - 1000` becomes `18446744073709551115`. This is the opposite of JavaScript, where `number` arithmetic never wraps (it loses precision on huge integers, but `499 - 1000` is just `-501`). Catch these bugs by testing in debug, or use checked arithmetic such as `subtotal.checked_sub(discount_cents)` which returns an `Option`. See [Section 02: Types](/02-basics/01-types/) and [Section 08: Error Handling](/08-error-handling/).

### Stepping through with a real debugger (LLDB)

`cargo build` produces a binary at `target/debug/<name>` that already contains **debug info** (the default `dev` profile sets `debug = true`), so a debugger can map machine instructions back to your source. On macOS and most Rust installs you get **LLDB**; on Linux you typically also have **GDB**. Rust ships `rust-lldb` and `rust-gdb` wrappers that load pretty-printers so a `String` shows as text and a `Vec` shows its elements.

Drive LLDB from the terminal. Here is a real session debugging `subtotal_cents`, condensed to the meaningful commands (the `(lldb)` lines are what you type):

```text
$ rust-lldb ./target/debug/shopping_cart
(lldb) breakpoint set --name subtotal_cents
Breakpoint 1: where = shopping_cart`shopping_cart::subtotal_cents + 24 at main.rs:11:21
(lldb) run
Process launched: '.../target/debug/shopping_cart' (arm64)
* thread #1, name = 'main', stop reason = breakpoint 1.1
    frame #0: shopping_cart::subtotal_cents(items=size=2) at main.rs:11:21
   10  	fn subtotal_cents(items: &[LineItem]) -> u64 {
-> 11  	    let mut total = 0;
   12  	    for item in items {
(lldb) frame variable items
(&[shopping_cart::LineItem]) items = size=2 {
  [0] = {
    name = "Coffee mug"
    unit_price_cents = 1299
    quantity = 2
  }
  [1] = {
    name = "Sticker pack"
    unit_price_cents = 499
    quantity = 1
  }
}
(lldb) next
(lldb) next
(lldb) frame variable total item
(unsigned long) total = 0
(shopping_cart::LineItem *) item = 0x0000600002f24050
(lldb) continue
Subtotal: $30.97
Process exited with status = 0 (0x00000000)
(lldb) quit
```

The slice `items` printed its two `LineItem`s with the `name` field rendered as readable text. That is the Rust pretty-printer the `rust-lldb` wrapper installed. The most common LLDB commands map cleanly onto the DevTools debugger:

| Task | LLDB command (short) | DevTools / Node equivalent |
| ---- | -------------------- | -------------------------- |
| Break on a function | `breakpoint set --name foo` (`b foo`) | Click the gutter / `debugger;` |
| Break at a line | `breakpoint set --file main.rs --line 13` | Click the line gutter |
| Start / restart | `run` (`r`) | Reload with debugger attached |
| Step over | `next` (`n`) | Step Over (F10) |
| Step into | `step` (`s`) | Step Into (F11) |
| Continue | `continue` (`c`) | Resume (F8) |
| Print a variable | `frame variable x` / `p x` | Hover / Watch panel |
| Show the call stack | `thread backtrace` (`bt`) | Call Stack panel |
| Quit | `quit` (`q`) | Stop |

> **Tip:** If you have only `gdb`, the workflow is identical: substitute `rust-gdb ./target/debug/shopping_cart` and use GDB's `break`, `run`, `next`, `step`, `print`, `backtrace`. Both debuggers read the same DWARF debug info that Cargo emits.

### CodeLLDB in VS Code

Few developers debug at a raw `(lldb)` prompt for long. The standard graphical flow is the **CodeLLDB** extension (`vadimcn.vscode-lldb`), which bundles its own LLDB and gives you breakpoints in the gutter, a variables pane, watch expressions, and a call-stack view: the F5 experience you know from Node, applied to a compiled Rust binary.

Install CodeLLDB from the Extensions view, then add a `.vscode/launch.json`:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug shopping_cart",
      "cargo": {
        "args": ["build", "--bin=shopping_cart"],
        "filter": { "name": "shopping_cart", "kind": "bin" }
      },
      "args": [],
      "cwd": "${workspaceFolder}"
    }
  ]
}
```

The `"cargo"` block is the Rust-specific glue: CodeLLDB runs `cargo build` for you, then asks Cargo (via `--message-format=json`) which binary it produced, and launches that under LLDB. You never hand-write a path to `target/debug/...`. Click in the gutter to set a breakpoint, press **F5**, and you get the same Variables/Watch/Call-Stack panels as a Node debug session — with Rust pretty-printers already wired in. For a test instead of a binary, change the filter to `"kind": "test"` (or use the **Debug** code lens rust-analyzer shows above each `#[test]` function — see [rust-analyzer](/24-tooling/05-rust-analyzer/)).

> **Note:** The Microsoft **C/C++** extension's `cppdbg`/`cppvsdbg` debuggers can also debug Rust, but CodeLLDB is the community default because it ships the Rust formatters out of the box and works the same on macOS, Linux, and Windows.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| ------- | ----------------------- | ---- |
| Quick print | `console.log(x)` — value only, on stdout | `dbg!(x)` — file, line, source text + value, on **stderr** |
| Print labeling | You type a label string | Macro captures the expression text automatically |
| Debug output in releases | Manually delete `console.log`s | `dbg!` is **not** stripped; you must remove it (Clippy flags it — see [Linting with Clippy](/24-tooling/02-linting/)) |
| Stack trace | Automatic on every thrown `Error` | Opt-in via `RUST_BACKTRACE=1` on panic |
| What gets a stack trace | Any exception | A **panic** (bug), not a recoverable `Result::Err` |
| Debugger | V8 inspector (built into the runtime) | LLDB / GDB on the compiled binary |
| Debug info | Always present (it's interpreted/JIT'd) | Emitted by Cargo's `dev` profile; thin/absent in `release` |
| Editor debug | DevTools / VS Code `node`/`pwa-node` | CodeLLDB (`type: "lldb"`) with a `cargo` launch block |

Two differences deserve emphasis. First, **`dbg!` is a debugging tool that ships in your binary unless you remove it**. Unlike a transpiler that can drop `console.log`, Rust keeps it. Treat `dbg!` like a `// TODO`: useful while you work, removed before you commit. Clippy's `dbg_macro` lint can enforce this in CI.

Second, **a backtrace requires debug info and an opt-in environment variable**. JavaScript stack traces are free because the engine always knows the source. Rust's `release` profile strips most debug info to shrink and speed up the binary, so a production panic backtrace may show addresses instead of function names unless you keep some debug info (`[profile.release] debug = 1`), a trade-off covered in [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/) and [Section 21: Binary Size](/21-performance/08-binary-size/).

---

## Common Pitfalls

### Pitfall 1: Expecting `dbg!` output on stdout

`dbg!` writes to **stderr**. If you run `cargo run > out.txt`, the `[src/...]` lines still appear in your terminal because only stdout was redirected. This is intentional and useful, but it surprises people who expect `console.log`-style stdout. To capture it, redirect stderr: `cargo run 2> debug.txt`.

### Pitfall 2: `dbg!` moves the value

`dbg!(x)` takes `x` **by value** (it returns it, so ownership flows through). If you log a value you still need afterward, you get a move error:

```rust
fn main() {
    let cart = vec!["mug", "sticker"];
    dbg!(cart);              // moves cart into dbg!
    println!("{}", cart.len()); // error[E0382]: borrow of moved value: `cart`
}
```

The real compiler error (from `cargo build`) is:

```text
error[E0382]: borrow of moved value: `cart`
 --> src/main.rs:4:20
  |
2 |     let cart = vec!["mug", "sticker"];
  |         ---- move occurs because `cart` has type `Vec<&str>`, which does not implement the `Copy` trait
3 |     dbg!(cart);              // moves cart into dbg!
  |     ---------- value moved here
4 |     println!("{}", cart.len()); // error[E0382]: borrow of moved value: `cart`
  |                    ^^^^ value borrowed here after move
  |
help: consider borrowing instead of transferring ownership
  |
3 |     dbg!(&cart);              // moves cart into dbg!
  |          +
```

The fix is one character: `dbg!(&cart)` borrows instead of moving. See [Section 05: Ownership](/05-ownership/) for why.

### Pitfall 3: No backtrace, just "run with RUST_BACKTRACE=1"

A panic with no stack frames is not a broken debugger; you simply forgot the environment variable. Rust *tells* you: `note: run with RUST_BACKTRACE=1 ...`. Set it (`RUST_BACKTRACE=1 cargo run`, or `export RUST_BACKTRACE=1` for the whole shell session) and rerun. There is no way to get the stack without it, because collecting one has a cost Rust will not pay unless asked.

### Pitfall 4: Symbols missing when debugging a release binary

Setting a breakpoint in a `--release` build often lands you in optimized, inlined code where variables read as `<optimized out>` and stepping jumps around. Debug the `dev` build (the default) for source-level stepping. If you must debug an optimized build, add `[profile.release] debug = true` to `Cargo.toml` to keep symbols (it does not disable optimizations; it just keeps the debug info alongside them).

### Pitfall 5: Overflow panics in debug but wraps in release

As shown above, the same arithmetic bug panics under `cargo run` but silently wraps under `cargo run --release`. Do not assume "it worked in release" means the logic is correct. Use `checked_*`/`saturating_*` methods for arithmetic that can legitimately go out of range, and rely on debug builds (and tests) to surface accidental overflow.

---

## Best Practices

- **Reach for `dbg!`, not `println!`, for ad-hoc inspection.** It records the location and expression for free and uses the debug formatter. Keep `println!`/`eprintln!` for output you actually intend to show users.
- **Remove `dbg!` before committing.** Enable Clippy's `dbg_macro` lint (`#![deny(clippy::dbg_macro)]` or in CI) so a stray `dbg!` fails the build. See [Linting with Clippy](/24-tooling/02-linting/) and [Common Clippy Lints, Explained](/24-tooling/03-clippy-lints/).
- **Set `RUST_BACKTRACE=1` in your dev shell.** Add it to your shell profile or a `.cargo/config.toml`'s `[env]` table so every panic during development shows a stack.
- **Derive `Debug` on your types.** It is the price of admission for `dbg!`, `assert_eq!` failure messages, and good debugger output. Derive `Debug` everywhere it is cheap.
- **Prefer the debugger for control-flow and data-structure bugs**; prefer `dbg!`/`eprintln!` for "what is this value right here" questions. Use whichever is faster for the bug in front of you, exactly as you switch between `console.log` and DevTools.
- **For production diagnostics, graduate from `dbg!` to structured logging** with the `tracing` crate, which gives you levels, spans, and filtering you can leave in the binary. See [Section 23: Logging](/23-ecosystem/03-logging/).
- **Use `debug_assert!` for invariant checks** you want enforced in debug builds but compiled out of release. There is no JavaScript equivalent, and it is a great place to catch the overflow-style bugs above.

---

## Real-World Example

A common real-world debugging task is parsing messy input, exactly where a TypeScript developer scatters `console.log`. Here is a price parser that strips formatting and returns cents, instrumented with `eprintln!` so the debug output goes to stderr and the parsed results to stdout:

```rust playground
/// Extract the digit-only cents value from a free-form price string.
/// Returns `None` if there are no digits to parse.
fn parse_amount(s: &str) -> Option<u64> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    // Debug trace on stderr: shows the raw input and what we extracted.
    eprintln!("[debug] parse_amount({s:?}) -> cleaned={cleaned:?}");
    cleaned.parse().ok()
}

fn main() {
    let inputs = ["$12.99", "free", "  300 "];
    for input in inputs {
        match parse_amount(input) {
            Some(cents) => println!("{input} = {cents}"),
            None => println!("{input} = (unparseable)"),
        }
    }
}
```

Running it, with the `[debug]` lines on stderr interleaved with the stdout results:

```text
[debug] parse_amount("$12.99") -> cleaned="1299"
$12.99 = 1299
[debug] parse_amount("free") -> cleaned=""
free = (unparseable)
[debug] parse_amount("  300 ") -> cleaned="300"
  300  = 300
```

The trace immediately reveals the bug class: `"$12.99"` parses to `1299` cents (correct *only* because the input always has two decimal places) while `"  300 "` yields `300` — three hundred *cents*, not dollars. The `{:?}` debug formatter is what makes the empty-string case (`cleaned=""`) visible at a glance; a plain `{}` would have printed nothing and hidden the problem. Because the trace is on stderr, piping the program's real output (`cargo run 2>/dev/null`) gives you clean results while keeping the instrumentation one redirect away.

When you have seen enough and want to step rather than print, set a breakpoint on `parse_amount` in CodeLLDB (or `b parse_amount` in `rust-lldb`), press F5, and inspect `cleaned` interactively — no edit-recompile cycle needed.

---

## Further Reading

Official documentation:

- [`std::dbg!` macro](https://doc.rust-lang.org/std/macro.dbg.html). The full contract: stderr, pretty-print, returns the value.
- [`std::eprintln!`](https://doc.rust-lang.org/std/macro.eprintln.html) and [`std::println!`](https://doc.rust-lang.org/std/macro.println.html): stderr vs stdout printing.
- [The `RUST_BACKTRACE` reference](https://doc.rust-lang.org/std/backtrace/index.html) and the [`std::backtrace`](https://doc.rust-lang.org/std/backtrace/struct.Backtrace.html) API for capturing backtraces programmatically.
- [Rust panics, in the Book](https://doc.rust-lang.org/book/ch09-01-unrecoverable-errors-with-panic.html): when and why Rust panics.
- [CodeLLDB user manual](https://github.com/vadimcn/codelldb/blob/master/MANUAL.md) — the `cargo` launch block, watch expressions, and more.
- [LLDB command map for GDB users](https://lldb.llvm.org/use/map.html): if you know one debugger, this maps to the other.

Related sections of this guide:

- [Setting Up VS Code for Rust](/24-tooling/06-vscode-setup/): installing extensions and the modern `rust-analyzer.check.command` setting; CodeLLDB lives here too.
- [rust-analyzer](/24-tooling/05-rust-analyzer/) — the **Debug** code lens above tests and `main` that launches CodeLLDB for you.
- [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/): `[profile.*]` `debug`/`split-debuginfo` settings that control what the debugger can see.
- [Linting with Clippy](/24-tooling/02-linting/) and [Common Clippy Lints, Explained](/24-tooling/03-clippy-lints/) — the `dbg_macro` lint that keeps stray `dbg!` out of your codebase.
- [Section 08: Error Handling](/08-error-handling/): `Result` vs panics, and `panic = "abort"`.
- [Section 23: Logging](/23-ecosystem/03-logging/) — `tracing` for diagnostics you keep in production.
- [Section 21: Profiling](/21-performance/00-profiling/): finding *slow* code, the complement to finding *wrong* code.
- [Section 25: Advanced Topics](/25-advanced-topics/) — deeper runtime introspection once the basics here are second nature.

---

## Exercises

### Exercise 1: Wrap an expression with `dbg!`

**Difficulty:** Easy

**Objective:** Use `dbg!` to inspect an intermediate value without restructuring code, and confirm it goes to stderr.

**Instructions:**

1. Create `cargo new dbg_practice` and write a `main` that computes `let cents = 1299; let dollars = cents / 100;` and prints `dollars`.
2. Without adding a separate statement, wrap the `cents / 100` expression so you see both the source text and the value of the division on stderr.
3. Run it redirecting stdout to a file (`cargo run > out.txt`) and confirm the `dbg!` line still appears in your terminal.

<details>
<summary>Solution</summary>

```rust playground
fn main() {
    let cents = 1299;
    // dbg! wraps the expression in place and returns its value.
    let dollars = dbg!(cents / 100);
    println!("dollars = {dollars}");
}
```

Running normally:

```text
[src/main.rs:4:19] cents / 100 = 12
dollars = 12
```

Running `cargo run > out.txt` still shows the `[src/main.rs:4:19] cents / 100 = 12` line in the terminal, because `dbg!` writes to **stderr**; only the `dollars = 12` line lands in `out.txt`.

</details>

### Exercise 2: Read a panic backtrace

**Difficulty:** Medium

**Objective:** Trigger a panic, get a backtrace, and identify the offending line from it.

**Instructions:**

1. Write a function `nth_word(text: &str, n: usize) -> &str` that returns `text.split_whitespace().nth(n).unwrap()`.
2. In `main`, call it with an `n` larger than the number of words so the `unwrap()` panics.
3. Run with `RUST_BACKTRACE=1` and identify, from the backtrace, the line number inside `nth_word` that panicked.

<details>
<summary>Solution</summary>

```rust
/// Return the nth whitespace-separated word. Panics if `n` is out of range.
fn nth_word(text: &str, n: usize) -> &str {
    text.split_whitespace().nth(n).unwrap()
}

fn main() {
    let sentence = "the quick brown fox";
    // Only 4 words (indices 0..=3); asking for index 10 panics.
    println!("{}", nth_word(sentence, 10));
}
```

Running `RUST_BACKTRACE=1 cargo run` prints a panic message pointing at the `.unwrap()` line, e.g.:

```text
thread 'main' panicked at src/main.rs:3:36:
called `Option::unwrap()` on a `None` value
stack backtrace:
   ...
   N: dbg_practice::nth_word
             at ./src/main.rs:3:36
   N+1: dbg_practice::main
             at ./src/main.rs:9:20
```

Read bottom-up: `main` called `nth_word`, which panicked at `src/main.rs:3:36`, the `.unwrap()` on a `None`. (Exact frame numbers and column vary by toolchain.) The fix is to return an `Option<&str>` and handle the missing word with a `match` or `?` instead of `unwrap()`-ing. See [Section 08: Error Handling](/08-error-handling/).

</details>

### Exercise 3: Step through with a debugger

**Difficulty:** Medium

**Objective:** Set a breakpoint, inspect a variable, and step a line using LLDB (or CodeLLDB in VS Code).

**Instructions:**

1. Reuse the `subtotal_cents` program from this topic (or any function with a loop and a local accumulator).
2. Build the debug binary with `cargo build`.
3. Launch it under `rust-lldb`, set a breakpoint on the accumulator function, run, print the function's argument with `frame variable`, step one line, and print the accumulator. Then `continue` to completion.

<details>
<summary>Solution</summary>

```bash
cargo build
rust-lldb ./target/debug/shopping_cart
```

Then, at the `(lldb)` prompt:

```text
(lldb) breakpoint set --name subtotal_cents   # or: b subtotal_cents
(lldb) run                                     # stops at the breakpoint
(lldb) frame variable items                    # prints the slice argument
(lldb) next                                    # step one line
(lldb) frame variable total                    # inspect the accumulator
(lldb) continue                                # run to completion
(lldb) quit
```

The slice prints with each `LineItem`'s `name` as readable text because `rust-lldb` loads Rust's pretty-printers. In VS Code, the equivalent is: install **CodeLLDB**, add the `launch.json` with the `cargo` block shown above, click the gutter next to the function to set a breakpoint, press **F5**, then use the Variables pane and **Step Over (F10)**: the same actions, in a GUI.

</details>
