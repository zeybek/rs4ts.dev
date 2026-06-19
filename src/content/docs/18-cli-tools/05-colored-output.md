---
title: "Colored Terminal Output"
description: "Color CLI output in Rust with owo-colors and anstream the way you'd use chalk in Node, decoupling styling from detection so ANSI codes vanish in pipes."
---

How to add color and styling to a command-line tool's output in Rust, the way you would with `chalk` or `picocolors` in Node.js, and, importantly, how to make that color disappear cleanly when output is piped to a file or when the user sets `NO_COLOR`.

---

## Quick Overview

Color in a terminal is just **ANSI escape codes**: short byte sequences like `\x1b[32m` (green on) and `\x1b[39m` (default foreground) wrapped around your text. In Node.js you reach for `chalk`, `picocolors`, or `kleur`; in Rust the go-to crates are **owo-colors** (zero-cost styling extension methods), **anstream** (a smart stdout/stderr that strips color when it shouldn't be there), **anstyle** (the style vocabulary `clap` and Cargo speak), and **console** (batteries-included, auto-detecting). The single most important thing this page teaches: **respect `NO_COLOR`** and don't emit escape codes into pipes. Getting this right is the difference between a polished tool and one that dumps `\x1b[32m` garbage into log files.

The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically.

---

## TypeScript/JavaScript Example

In Node.js, `chalk` is the household name, though the tiny `picocolors` has largely won on size and speed. Both auto-detect whether output is a terminal and honor the `NO_COLOR` / `FORCE_COLOR` conventions for you.

```typescript
// npm install chalk    (chalk v5 is ESM-only)
import chalk from "chalk";

// Basic colors and styles
console.log(chalk.green("Success!"));
console.log(chalk.yellow("Warning!"));
console.log(chalk.red.bold("Error!"));
console.log(`${chalk.blue("INFO")} Server started`);

// Background colors, combinations, truecolor
console.log(chalk.white.bgRed.bold("Alert"));
console.log(chalk.dim("dimmed note"));
console.log(chalk.underline("underlined"));
console.log(chalk.rgb(255, 128, 0)("custom rgb"));

// A cargo-style diagnostic
function report(level: "info" | "warning" | "error", message: string) {
  const tag = {
    info: chalk.green.bold("info"),
    warning: chalk.yellow.bold("warning"),
    error: chalk.red.bold("error"),
  }[level];
  const line = `${tag}: ${message}`;
  if (level === "error") console.error(line);
  else console.log(line);
}

report("info", "compiling 12 modules");
report("error", "could not build");
```

Chalk decides at runtime whether to emit codes. If you run `node app.js | cat`, or set `NO_COLOR=1`, chalk produces plain text automatically. That auto-detection is the behavior we want to reproduce in Rust, and as you'll see, not every Rust crate does it for you by default.

---

## Rust Equivalent

The idiomatic modern stack is **owo-colors** for the styling syntax plus **anstream** for a terminal-aware output stream. owo-colors gives you `chalk`-like extension methods; anstream's `println!` is a drop-in that strips the codes when the destination is not a real terminal or when `NO_COLOR` is set.

Add the dependencies (in a fresh `cargo new` project):

```bash
cargo add owo-colors
cargo add anstream
```

```rust
use anstream::println; // terminal-aware: strips ANSI when not a TTY / NO_COLOR set
use owo_colors::OwoColorize;

fn main() {
    // Basic colors and styles — same vocabulary as chalk
    println!("{}", "Success!".green());
    println!("{}", "Warning!".yellow());
    println!("{}", "Error!".red().bold());
    println!("{} {}", "INFO".blue(), "Server started");

    // Background colors, combinations, truecolor
    println!("{}", "Alert".white().on_red().bold());
    println!("{}", "dimmed note".dimmed());
    println!("{}", "underlined".underline());
    println!("{}", "custom rgb".truecolor(255, 128, 0));
}
```

> **Tip:** The split is deliberate. owo-colors only *describes* the style; anstream *decides whether to keep it*. You can swap either piece independently — e.g. use owo-colors with a plain `std::println!` and your own detection, or use anstream with `anstyle` instead of owo-colors.

Here is the same `report` helper, written so color is automatically disabled in pipes and under `NO_COLOR`:

```rust
use anstream::{eprintln, println};
use owo_colors::OwoColorize;
use std::fmt::Display;

enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn label(&self) -> String {
        match self {
            Level::Info => "info".green().bold().to_string(),
            Level::Warn => "warning".yellow().bold().to_string(),
            Level::Error => "error".red().bold().to_string(),
        }
    }
}

fn report(level: Level, message: impl Display) {
    let line = format!("{}: {}", level.label(), message);
    match level {
        Level::Error => eprintln!("{line}"),
        _ => println!("{line}"),
    }
}

fn main() {
    report(Level::Info, "compiling 12 crates");
    report(Level::Warn, "unused import: `std::env`");
    report(Level::Error, "could not compile `app` (bin \"app\")");
}
```

Run through a pipe, the ANSI codes are gone:

```bash
cargo run -q | cat
```

```text
info: compiling 12 crates
warning: unused import: `std::env`
```

(The `error` line goes to stderr, so it doesn't show in the piped stdout, exactly like Cargo.)

---

## Detailed Explanation

### owo-colors: zero-cost styling via an extension trait

`use owo_colors::OwoColorize;` brings a trait into scope that adds methods like `.green()`, `.bold()`, and `.on_red()` to **every** type that implements `Display` (and `Debug`). This is the same ergonomic trick TypeScript devs know from prototype extension, but checked at compile time.

The key subtlety: `.green()` does **not** return a `String`. It returns a tiny wrapper struct — `FgColorDisplay<'_, Red, &str>` (here `Red` is shorthand for `owo_colors::colors::Red`, the fully-qualified path the compiler prints in the Pitfall 1 error below) — that holds a reference to your value and remembers the style. No allocation, no escape codes are produced *until the value is actually formatted*. When you eventually `println!("{}", x.green())`, the wrapper's `Display` implementation writes `\x1b[Progress Bars and Spinners with indicatif](/18-cli-tools/04-progress-bars/)) takes the chalk approach: `console::style(..)` returns a `StyledObject` that **auto-detects by default**, so you don't need a separate stream.

```bash
cargo add console
```

```rust
use console::style;

fn main() {
    // Auto-detects TTY + NO_COLOR/CLICOLOR; only emits ANSI when appropriate.
    println!("{}", style("Deploy succeeded").green().bold());
    println!("{}", style("Retrying...").yellow());

    // Override the decision explicitly when you must:
    println!("{}", style("always red").red().force_styling(true));
}
```

Piped, the auto-detecting styles vanish but the forced one survives (raw bytes):

```text
"Deploy succeeded\nRetrying...\n\x1b[Parsing Arguments with the clap Derive API](/18-cli-tools/01-clap-derive/)), but it's worth recognizing:

```rust
use anstyle::{AnsiColor, Color, Style};

fn main() {
    let heading = Style::new()
        .bold()
        .fg_color(Some(Color::Ansi(AnsiColor::Green)));

    // `{heading}` writes the opening sequence; `{heading:#}` writes the reset.
    println!("{heading}Section{heading:#}");
    // Equivalent, spelled out:
    println!("{}Section{}", heading.render(), heading.render_reset());
}
```

Both lines produce the same bytes — `\x1b[1m\x1b[32mSection\x1b[0m`. anstyle is *just* the codes; it has no opinion about detection, which is precisely why higher-level crates build on it.

---

## Key Differences

| Concern | TypeScript (chalk / picocolors) | Rust |
| --- | --- | --- |
| How styling attaches | Functions wrapping strings | Trait methods (`OwoColorize`) on any `Display` type, or `style(..)` wrappers |
| Return value of `.green()` | A new `string` | A lazy zero-cost wrapper (owo-colors) or `StyledObject` (console) — **not** a `String` |
| Auto terminal detection | Built in, on by default | owo-colors: **off** by default; console: **on**; anstream: **on** at the stream |
| `NO_COLOR` honored | Yes, automatically | Yes, via anstream, console, or owo-colors' `supports-colors` feature; **not** by raw owo-colors |
| Force color | `FORCE_COLOR=1` | `CLICOLOR_FORCE=1` (anstream/console), or explicit `force_styling(true)` / `set_override(true)` |
| Output target awareness | One `console.log` | Choose: terminal-aware `anstream::println!` vs raw `std::println!` |

The headline conceptual difference: **chalk couples styling and detection; the Rust ecosystem deliberately decouples them.** owo-colors answers "what does green look like?" while anstream answers "should this stream show green right now?". This separation is more verbose for a hello-world, but it scales: you set the policy once at the output boundary and never branch on `NO_COLOR` in business logic. If you want the chalk-style all-in-one feel, reach for `console`.

> **Note:** `NO_COLOR` (see <https://no-color.org>) is a cross-language convention: if the variable is **present and non-empty**, applications should not emit color. `CLICOLOR_FORCE` (non-empty) forces color on even when piped. These are the same conventions Cargo, ripgrep, and most modern CLIs follow.

---

## Common Pitfalls

### Pitfall 1: Treating a styled value like a `String`

A TypeScript dev expects `"error".red()` to be a string and tries to `+`-concatenate it like JavaScript:

```rust
use owo_colors::OwoColorize;

fn main() {
    let msg = "error".red() + ": something broke"; // does not compile (error[E0369])
    println!("{}", msg);
}
```

The real compiler error:

```text
error[E0369]: cannot add `&str` to `FgColorDisplay<'_, owo_colors::colors::Red, &str>`
 --> src/main.rs:6:29
  |
6 |     let msg = "error".red() + ": something broke";
  |               ------------- ^ ------------------- &str
  |               |
  |               FgColorDisplay<'_, owo_colors::colors::Red, &str>
  |
note: the foreign item type `FgColorDisplay<'_, owo_colors::colors::Red, &str>`
      doesn't implement `Add<&str>`
```

`.red()` returns a lazy display wrapper, not a `String`. The fix is to format the pieces together rather than add them:

```rust
use owo_colors::OwoColorize;

fn main() {
    let msg = format!("{}: something broke", "error".red());
    println!("{msg}");
}
```

### Pitfall 2: Using `std::println!` with owo-colors and leaking codes

Because raw owo-colors never detects the terminal, this writes escape bytes into a redirected file:

```rust
use owo_colors::OwoColorize;

fn main() {
    // std::println! does NO detection; owo-colors does NO detection.
    println!("{}", "result".green()); // leaks \x1b[32m...\x1b[39m into pipes/files
}
```

Run `cargo run | cat` and you'll see literal `\x1b[32m` in the output: the exact garbage-in-logs problem. **Fix:** import `anstream::println` (or use `console::style`, or gate with `if_supports_color`). One-line change:

```rust
use anstream::println; // ← decides keep-or-strip per stream
use owo_colors::OwoColorize;

fn main() {
    println!("{}", "result".green()); // stripped when piped, kept on a TTY
}
```

### Pitfall 3: Padding a value that already contains ANSI bytes

Alignment specifiers count **bytes**, and ANSI escape sequences are bytes. owo-colors' *lazy* wrapper is smart — it forwards the format spec to the inner text, so `{:<10}` pads correctly. But the moment you render a styled value into a `String` (with `.to_string()` or `format!`) and *then* try to pad it, the width counts the invisible escape bytes:

```rust
use owo_colors::OwoColorize;

fn main() {
    // Pre-rendered into a String — width now counts the escape bytes:
    let pre_rendered = "OK".green().to_string();
    println!("[{:<10}]", pre_rendered); // NOT padded to 10 visible columns
}
```

Raw bytes: `[\x1b[32mOK\x1b[39m]` — no padding at all, because the formatter saw a 10-byte-ish string of mostly escape characters. **Fix:** pad the plain text first, then style the padded result:

```rust
use owo_colors::OwoColorize;

fn main() {
    let padded = format!("{:<10}", "OK");
    println!("[{}]", padded.green()); // visible "OK" + 8 spaces, then colored
}
```

### Pitfall 4: Forgetting to enable the `supports-colors` feature

If you write `use owo_colors::{OwoColorize, Stream};` and call `if_supports_color` without the feature, you actually get **two** errors. First, the unresolved import of `Stream` (E0432) — and *this* is the one that carries the "configured out / gated behind `supports-colors`" note. Second, a separate "method not found" (E0599) for `if_supports_color`, with no note attached:

```text
error[E0432]: unresolved import `owo_colors::Stream`
 --> src/main.rs:1:31
  |
1 | use owo_colors::{OwoColorize, Stream};
  |                               ^^^^^^ no `Stream` in the root
  |
note: found an item that was configured out
  |
  |       --------------------------- the item is gated behind the `supports-colors` feature
...
  |     supports_colors::{Stream, SupportsColorsDisplay},
  |                       ^^^^^^

error[E0599]: no method named `if_supports_color` found for reference `&'static str` in the current scope
 --> src/main.rs:6:23
  |
6 |         "conditional".if_supports_color(Stream::Stdout, |t| t.green())
  |                       ^^^^^^^^^^^^^^^^^ method not found in `&'static str`
```

The lesson: the telltale "gated behind the `supports-colors` feature" note hangs off the `Stream` import error, not the method error. **Fix:** `cargo add owo-colors --features supports-colors`. (Or just use anstream/console, which need no extra feature flags.)

---

## Best Practices

- **Decide color once, at the output boundary.** Route all user-facing output through `anstream::stdout()/stderr()` (or their macros). Keep your styling code unconditional and free of `if NO_COLOR` checks.
- **Respect `NO_COLOR` and `CLICOLOR_FORCE`.** Don't roll your own env parsing if you can avoid it; anstream, console, and owo-colors' `supports-colors` feature all implement the conventions correctly. The check is: `NO_COLOR` present **and non-empty** disables; `CLICOLOR_FORCE` present and non-empty forces on.
- **Send errors and diagnostics to stderr, normal output to stdout**, and detect each independently. anstream's `stdout()` and `stderr()` evaluate their own stream, so a tool whose stdout is piped but whose stderr is a terminal still colorizes errors.
- **Prefer named ANSI colors over truecolor for portability.** `.green()` works on virtually every terminal; `.truecolor(...)` only renders correctly on 24-bit-capable terminals and silently degrades elsewhere.
- **Give users an explicit `--color <auto|always|never>` flag**, mapping to anstream/owo-colors overrides. This is what Cargo and ripgrep do, and power users expect it. With owo-colors, `owo_colors::set_override(true|false)` sets a process-wide decision that `if_supports_color` honors:

  ```rust
  use owo_colors::{OwoColorize, Stream};

  fn main() {
      owo_colors::set_override(false); // e.g. from `--color never`
      println!(
          "{}",
          "forced off".if_supports_color(Stream::Stdout, |t| t.red())
      ); // prints plain, even on a TTY
  }
  ```

- **Don't hand-write escape codes** like `"\x1b[32m"`. They're error-prone (easy to forget the reset), don't get stripped automatically, and break on non-ANSI consoles. Let a crate own the bytes.

> **Warning:** A subtle correctness issue: a manual or pre-rendered ANSI string that you later pass through a width/truncation formatter (`{:.10}`, `{:<10}`) will miscount, because escape bytes are counted but not displayed. Style **after** you've finished sizing the plain text. See Pitfall 3.

---

## Real-World Example

A small `cargo`-style diagnostic reporter — the kind of output a linter or build tool emits — that colorizes on a terminal, goes plain in pipes and under `NO_COLOR`, and sends errors to stderr. This is compile-verified end to end.

> **Note:** This example uses the unconditional-styling pattern (owo-colors always emits codes; `anstream` strips them at the boundary). With this pattern the user-facing color knobs are the environment variables `anstream` reads — `NO_COLOR` and `CLICOLOR_FORCE` — *not* `owo_colors::set_override`. `set_override` only governs `if_supports_color` and `Style`-based rendering (see Exercise 2), neither of which this code uses, so adding a `set_override` branch here would be a dead no-op. A real `--color <auto|always|never>` flag must instead be wired into `anstream`'s `ColorChoice` (e.g. via `anstream::AutoStream` constructed with an explicit choice). Exercise 2 shows the override path that *does* respond to `set_override`.

`Cargo.toml`:

```toml
[dependencies]
anstream = "1.0.0"
owo-colors = "4.3.0"
```

`src/main.rs`:

```rust
use anstream::{eprintln, println};
use owo_colors::OwoColorize;
use std::fmt::Display;

/// A diagnostic severity, like a compiler or linter would emit.
enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    /// The styled label. We always apply the style here; `anstream` strips it
    /// later if the destination is not a color-capable terminal.
    fn label(&self) -> String {
        match self {
            Level::Info => "info".green().bold().to_string(),
            Level::Warn => "warning".yellow().bold().to_string(),
            Level::Error => "error".red().bold().to_string(),
        }
    }
}

/// Print a `level: message` line, routing errors to stderr.
fn report(level: Level, message: impl Display) {
    let line = format!("{}: {}", level.label(), message);
    match level {
        Level::Error => eprintln!("{line}"),
        _ => println!("{line}"),
    }
}

fn main() {
    // No per-call color branching: styling is unconditional and `anstream`
    // strips it when stdout/stderr isn't a color-capable terminal. The user
    // controls color through the environment `anstream` reads — `NO_COLOR`
    // disables it, `CLICOLOR_FORCE` forces it on.
    report(Level::Info, "compiling 12 crates");
    report(
        Level::Warn,
        format!("unused variable: `{}`", "count".cyan()),
    );
    report(Level::Error, "could not compile `app` (bin \"app\")");
}
```

Behavior, verified by inspecting raw bytes:

- **Piped** (`cargo run -q | cat`): every line is plain text; the `error` line is on stderr.

  ```text
  info: compiling 12 crates
  warning: unused variable: `count`
  ```

- **On a real terminal** (`NO_COLOR` unset): labels are colored, `count` is cyan. Raw stdout bytes:

  ```text
  "\x1b[1m\x1b[32minfo\x1b[39m\x1b[0m: compiling 12 crates\n"
  "\x1b[1m\x1b[33mwarning\x1b[39m\x1b[0m: unused variable: `\x1b[36mcount\x1b[39m`\n"
  ```

- **`NO_COLOR=1` on a terminal**: anstream strips everything; output is identical to the piped case.

This is the whole discipline in one screen: style freely, decide once, route errors correctly, and the conventions take care of themselves.

---

## Further Reading

### Official Documentation

- [owo-colors on docs.rs](https://docs.rs/owo-colors/): the `OwoColorize` trait, `if_supports_color`, `set_override`.
- [anstream on docs.rs](https://docs.rs/anstream/): terminal-aware streams and the strip/keep logic.
- [anstyle on docs.rs](https://docs.rs/anstyle/): the shared `Style`/`Color` vocabulary.
- [console on docs.rs](https://docs.rs/console/): auto-detecting `style()` and terminal utilities.
- [supports-color on docs.rs](https://docs.rs/supports-color/): the detection crate behind the feature.
- [The NO_COLOR convention](https://no-color.org/) and the [CLICOLOR spec](https://bixense.com/clicolors/).
- [std::io::IsTerminal](https://doc.rust-lang.org/std/io/trait.IsTerminal.html): for rolling your own detection.

### Related Sections in This Guide

- [Parsing Arguments with the clap Derive API](/18-cli-tools/01-clap-derive/) and [Argument Parsing with clap (Builder API)](/18-cli-tools/00-clap-basics/): `clap` uses `anstyle` to color its help output; wire a `--color` flag here.
- [Git-like Subcommands with clap](/18-cli-tools/02-subcommands/): git-like subcommands that share a coloring policy.
- [Progress Bars and Spinners with indicatif](/18-cli-tools/04-progress-bars/): `indicatif` builds on `console` for spinners and bars.
- [Terminal UIs with Ratatui](/18-cli-tools/03-terminal-ui/): full-screen TUIs with `ratatui` (a different rendering model entirely).
- [Environment Variables](/18-cli-tools/08-environment-vars/): reading `NO_COLOR`/`CLICOLOR_FORCE` and other config via the environment.
- [Cross-Platform CLI Considerations](/18-cli-tools/09-cross-platform/): Windows console quirks and ANSI support.
- [Distributing CLI Tools](/18-cli-tools/10-distribution/): shipping the finished tool.
- Foundations: [Section 02 — Output and Formatting](/02-basics/04-output/) covers `println!`, `format!`, and format specifiers used throughout this page. New to the toolchain? See [Section 01 — Getting Started](/01-getting-started/) and [Section 00 — Introduction](/00-introduction/).
- Building a colorized WebAssembly logger for the browser console instead of a terminal? See [Section 19 — WebAssembly](/19-wasm/).

---

## Exercises

### Exercise 1: A leak-proof status line

**Difficulty:** Easy

**Objective:** Print a green `[ OK ]` prefix followed by a message, such that the color disappears when output is piped.

**Instructions:**

1. Create a project and `cargo add owo-colors anstream`.
2. Write `status("Database connected")` that prints `[ OK ] Database connected` with `[ OK ]` in bold green.
3. Verify that `cargo run -q | cat` shows no escape codes, but a real terminal shows green.

```rust
use anstream::println;
use owo_colors::OwoColorize;

fn status(message: &str) {
    // TODO: print a bold-green "[ OK ]" prefix, then the message
}

fn main() {
    status("Database connected");
}
```

<details>
<summary>Solution</summary>

```rust
use anstream::println;
use owo_colors::OwoColorize;

fn status(message: &str) {
    // Style freely; anstream's println! strips codes when not a TTY / NO_COLOR set.
    println!("{} {message}", "[ OK ]".green().bold());
}

fn main() {
    status("Database connected");
}
```

Piped, this prints exactly `[ OK ] Database connected` with no escape bytes; on a terminal the prefix is bold green.

</details>

### Exercise 2: Honor `--color=<auto|always|never>`

**Difficulty:** Medium

**Objective:** Add a color policy flag that overrides auto-detection, mirroring Cargo and ripgrep.

**Instructions:**

1. Read the first CLI argument. Map `--color=always`, `--color=never`, and anything else (auto) to a decision.
2. Use `owo_colors::set_override(..)` for `always`/`never`; leave it untouched for auto. Enable the `supports-colors` feature.
3. Print a line styled via `if_supports_color` and confirm: `--color=always` colors even when piped; `--color=never` is plain even on a TTY; with no flag, it follows the terminal and `NO_COLOR`.

```rust
use owo_colors::{OwoColorize, Stream};

fn main() {
    // TODO: parse args[1], set the override, then print a conditional-styled line
}
```

<details>
<summary>Solution</summary>

```rust
// Cargo.toml:
// owo-colors = { version = "4.3.0", features = ["supports-colors"] }
use owo_colors::{OwoColorize, Stream, Style};

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--color=always") => owo_colors::set_override(true),
        Some("--color=never") => owo_colors::set_override(false),
        _ => {} // auto: if_supports_color does TTY + NO_COLOR detection
    }

    // Build the combined style up front so the closure can apply it in one call.
    // (Chaining two methods inside the closure, like `|t| t.green().bold()`,
    // fails to compile — the second method would borrow a temporary.)
    let style = Style::new().green().bold();
    println!(
        "{}",
        "build finished".if_supports_color(Stream::Stdout, |t| t.style(style))
    );
}
```

`set_override(true)` forces the closure to run (color on) regardless of TTY/`NO_COLOR`; `set_override(false)` forces it off; with neither, `if_supports_color` consults the stream and the `NO_COLOR`/`CLICOLOR` environment.

</details>

### Exercise 3: A padded, colorized two-column report

**Difficulty:** Hard

**Objective:** Print an aligned two-column table (label left, value right) with colored labels — without the alignment breaking on the escape bytes.

**Instructions:**

1. Given pairs like `("Status", "online")` and `("Latency", "12ms")`, print each as `label` left-padded to 12 columns, then the value.
2. Color labels cyan and the `"online"` value green. The columns must line up visually.
3. The catch: you must pad the **plain** label to width *before* styling it (recall Pitfall 3). Verify alignment by piping to `cat` (codes stripped) — columns should still line up.

```rust
use anstream::println;
use owo_colors::OwoColorize;

fn row(label: &str, value: &str) {
    // TODO: left-pad `label` to 12 columns of PLAIN text, then color it,
    // then print the value (green if "online", else default)
}

fn main() {
    row("Status", "online");
    row("Latency", "12ms");
    row("Region", "us-east-1");
}
```

<details>
<summary>Solution</summary>

```rust
use anstream::println;
use owo_colors::OwoColorize;

fn row(label: &str, value: &str) {
    // 1) Size the PLAIN text first so the width counts visible columns only.
    let padded = format!("{label:<12}");
    // 2) Style the already-padded label.
    let styled_label = padded.cyan();
    // 3) Conditionally color the value.
    if value == "online" {
        println!("{styled_label}{}", value.green().bold());
    } else {
        println!("{styled_label}{value}");
    }
}

fn main() {
    row("Status", "online");
    row("Latency", "12ms");
    row("Region", "us-east-1");
}
```

Piped output (codes stripped by anstream) lines up correctly:

```text
Status      online
Latency     12ms
Region      us-east-1
```

On a terminal the labels are cyan and `online` is bold green, with the same alignment. Had we written `format!("{:<12}", "Status".cyan())`, the lazy owo-colors wrapper would actually *still* align (it forwards the spec) — but the moment you pre-render with `.to_string()`, alignment breaks. Padding the plain text first is the reliable habit.

</details>
