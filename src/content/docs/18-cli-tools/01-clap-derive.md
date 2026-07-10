---
title: "Parsing Arguments with the clap Derive API"
description: "Declare CLI arguments as struct fields and let clap's derive macro generate the parser, validation, and --help — no runtime method chains like yargs."
---

In Node, you reach for `commander` or `yargs` and wire up options with a chain of method calls. In Rust, the idiomatic way to build a command-line interface is **clap's derive API**: you describe your arguments as fields on a `struct`, slap `#[derive(Parser)]` on it, and the macro generates the parser, the `--help` text, the validation, and the error messages for you.

---

## Quick Overview

`clap` is the de-facto standard argument parser in the Rust ecosystem. Its **derive API** lets you define your CLI as a plain `struct` whose fields *are* the arguments; the compiler turns that struct into a fully-featured parser at build time. For a TypeScript/JavaScript developer, this feels like declaring your options with a schema (think `zod` for `process.argv`): the struct is the single source of truth for parsing, type conversion, defaults, validation, and the generated help screen.

> **Note:** This page focuses on the **derive** approach, which is what you should reach for in almost all new code. The sibling page [Argument Parsing with clap (Builder API)](/18-cli-tools/00-clap-basics/) covers the lower-level **builder** API (`Command::new(...).arg(...)`), and [Git-like Subcommands with clap](/18-cli-tools/02-subcommands/) covers `git`-style subcommands with `#[derive(Subcommand)]`.

---

## TypeScript/JavaScript Example

A typical small CLI built with **commander** (the most popular Node argument parser) looks like this:

```typescript
// cli.ts — run with: npx tsx cli.ts app.log -p ERROR -vv --format json
import { Command, Option } from "commander";

const program = new Command();

program
  .name("loganalyze")
  .description("A fast log file analyzer.")
  .version("1.2.0")
  .argument("<path>", "path to the log file to analyze")
  .option("-p, --pattern <pattern>", "filter to lines containing this pattern")
  .option("-c, --context <n>", "number of lines of context to show", "3")
  .option("-v, --verbose", "increase logging verbosity", (_, prev: number) => prev + 1, 0)
  .option("--case-sensitive", "treat the pattern as case-sensitive", false)
  .addOption(
    new Option("--format <format>", "output format")
      .choices(["text", "json", "csv"])
      .default("text"),
  )
  .option("-i, --ignore <file...>", "files to ignore (repeatable)", []);

program.parse();

const path = program.args[0];
const opts = program.opts();

// Everything coming back is `any` / `string` — note `context` is the STRING "3":
console.log({ path, ...opts });
```

**Key points:**

- The CLI shape lives in a chain of runtime method calls.
- Types are weak: `opts.context` is the string `"3"`, not a number, unless you pass a custom coercion function. `program.opts()` returns `OptionValues`, effectively `Record<string, any>`.
- `--help` and `--version` are generated for you, which is the part clap also automates.
- Validation (e.g. "this must be a number ≥ 1024") is on you to write by hand.

---

## Rust Equivalent

The same CLI with clap's derive API. First add the dependency (the `derive` feature is **not** on by default):

```toml
# Cargo.toml
[dependencies]
clap = { version = "4", features = ["derive"] }
```

Or from the shell. `cargo add` has been built into Cargo since 1.62, so no extra tooling is needed:

```bash
cargo add clap --features derive
```

> The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. The examples on this page target clap 4.6.

```rust
// src/main.rs
use std::path::PathBuf;
use clap::Parser;

/// A fast log file analyzer.
#[derive(Parser, Debug)]
#[command(name = "loganalyze")]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to the log file to analyze
    path: PathBuf,

    /// Filter to lines containing this pattern
    #[arg(short, long)]
    pattern: Option<String>,

    /// Number of lines of context to show
    #[arg(short, long, default_value_t = 3)]
    context: usize,

    /// Increase logging verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Treat the pattern as case-sensitive
    #[arg(long)]
    case_sensitive: bool,

    /// Output format
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Files to ignore (repeatable)
    #[arg(short, long)]
    ignore: Vec<String>,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Format {
    Text,
    Json,
    Csv,
}

fn main() {
    let cli = Cli::parse();
    println!("{cli:#?}");
}
```

Running `loganalyze app.log -p ERROR -vv --format json -i a.log -i b.log` prints the fully-typed, validated struct (real output):

```text
Cli {
    path: "app.log",
    pattern: Some(
        "ERROR",
    ),
    context: 3,
    verbose: 2,
    case_sensitive: false,
    format: Json,
    ignore: [
        "a.log",
        "b.log",
    ],
}
```

**Key points:**

- The struct *is* the CLI specification. Each field becomes an argument; the field's **type** decides parsing and validation.
- `context` is a real `usize` (not a string), `format` is a real enum variant, `verbose` is a `u8` count: all checked at parse time.
- The doc comments (`/// ...`) become the help text. There is no separate help-string registry to keep in sync.

---

## Detailed Explanation

### The field type drives everything

The single most important idea: in the derive API, the **Rust type of the field** controls how an argument is parsed, whether it's required, and how it's validated. There is no separate "type" parameter as in `yargs`.

| Field type | clap behavior |
| --- | --- |
| `String`, `PathBuf`, `u16`, `usize`, … | **Required** positional or option; the string is parsed into that type. |
| `Option<T>` | **Optional**; absent → `None`, present → `Some(parsed_value)`. |
| `bool` | A **flag**; present → `true`, absent → `false`. |
| `Vec<T>` | **Repeatable**; each occurrence appends one `T`. |
| `T` with `default_value_t` | Optional with a fallback value. |

Compare this to commander, where *everything* arrives as a string (or `true` for a bare flag) and you coerce it yourself.

### Container attributes vs. field attributes

There are two attribute namespaces:

- `#[command(...)]` configures the whole command (the `struct`). Common keys: `name`, `version`, `about`, `long_about`, `author`.
- `#[arg(...)]` configures a single argument (a field). Common keys: `short`, `long`, `default_value`, `default_value_t`, `value_enum`, `value_parser`, `action`, `env`, `value_name`, `required`, `num_args`.

```rust
#[derive(Parser, Debug)]
#[command(name = "loganalyze")]            // container attribute
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long, default_value_t = 3)] // field attribute
    context: usize,
}
```

`#[command(version, about)]` with no value pulls the version and description straight from `Cargo.toml` (`package.version` and `package.description`). Set `version = "1.2.0"` and `description = "..."` in `Cargo.toml` and clap reuses them, so you never duplicate your version string.

### `short` and `long`

- `#[arg(short, long)]` derives the short flag from the first letter of the field name (`-c`) and the long flag from the field name with hyphens (`--context`).
- Field names in `snake_case` become `--kebab-case` long flags automatically: `case_sensitive` → `--case-sensitive`.
- Override either: `#[arg(short = 'C', long = "ctx")]`.

### Doc comments become help text

The doc comment on a field is its help string; the doc comment on the struct is the command's `about`. This is why the `--help` output below needs no extra wiring (real output from `loganalyze --help`):

```text
A fast log file analyzer

Usage: loganalyze [OPTIONS] <PATH>

Arguments:
  <PATH>  Path to the log file to analyze

Options:
  -p, --pattern <PATTERN>  Filter to lines containing this pattern
  -c, --context <CONTEXT>  Number of lines of context to show [default: 3]
  -v, --verbose...         Increase logging verbosity (-v, -vv, -vvv)
      --case-sensitive     Treat the pattern as case-sensitive
      --format <FORMAT>    Output format [default: text] [possible values: text, json, csv]
  -i, --ignore <IGNORE>    Files to ignore (repeatable)
  -h, --help               Print help
  -V, --version            Print version
```

> **Note:** The `Usage:` line shows the *binary name* taken from `argv[0]` at runtime, while `--version` prints the `name` you set in `#[command(name = ...)]`. Don't be surprised if `cargo run` shows the project's binary name in usage but your chosen name in the version string.

### Defaults: `default_value_t` vs. `default_value`

This trips up newcomers, so it's worth being precise:

- `default_value_t = <expr>`: the default is a **value of the field's type**. clap formats it via `Display`. Use it for typed defaults: `default_value_t = 3`, `default_value_t = Format::Text`.
- `default_value = "<string>"`: the default is a **string** that is run through the same parser as user input. Use it when a string literal reads naturally: `default_value = "127.0.0.1"`.

```rust
#[arg(long, default_value_t = 8080)]        // typed default: the integer 8080
port: u16,

#[arg(long, default_value = "127.0.0.1")]   // string default, parsed like input
host: String,
```

A field with a default is automatically optional — the user can omit it. A field without a default and without `Option<T>` is required.

### Counting flags and other actions

`action = clap::ArgAction::Count` turns repeated occurrences into a number. `-vvv` becomes `3`. This is the idiomatic Rust replacement for commander's custom `(_, prev) => prev + 1` reducer. Other useful actions include `ArgAction::SetTrue` / `SetFalse` (which a bare `bool` field already implies) and `ArgAction::Append` (which a `Vec<T>` field already implies).

### Enums via `ValueEnum`

Derive `ValueEnum` on an enum and clap restricts the argument to the variant names (lowercased, kebab-cased) and lists them in `--help` as `[possible values: text, json, csv]`. Reject anything else automatically; no manual `.choices([...])` array to maintain.

---

## Key Differences

| Concern | commander / yargs (Node) | clap derive (Rust) |
| --- | --- | --- |
| Source of truth | Chain of runtime method calls | A `struct` definition |
| Value types | Mostly `string` / `any`; you coerce | The field's Rust type; parsed and checked |
| Required vs optional | `<required>` vs `[optional]` in a string | `T` (required) vs `Option<T>` (optional) |
| Repeated values | `<x...>` + default `[]` | `Vec<T>` |
| Enumerated choices | `.choices([...])` (runtime array) | `#[derive(ValueEnum)]` (a real enum) |
| Counting flags | Custom reducer function | `ArgAction::Count` on a `u8` |
| Validation | Hand-written in your code | `value_parser` returns `Result`; clap reports the error |
| Help text | Strings passed to each method | Doc comments on the struct/fields |
| When errors surface | Runtime | Parse time, before your logic runs |

The deeper shift: in Node the parser hands you a loosely-typed bag and your program does the validating. In Rust, by the time `Cli::parse()` returns, **every field is already the correct, validated type** — invalid input never reaches your `main`. This is the same payoff you get from parsing untyped JSON into a typed struct with `serde` (see [Section 15](/15-serialization/)).

> **Tip:** Think of `Cli::parse()` as "parse, don't validate" applied to `argv`. The struct you get back makes illegal states unrepresentable.

---

## Common Pitfalls

### Forgetting the `derive` feature

clap's derive macros live behind a Cargo feature. If you `cargo add clap` without it, the `Parser` trait is in scope but the derive macro is not, and you get a real compiler error:

```text
error: cannot find derive macro `Parser` in this scope
note: `Parser` is imported here, but it is only a trait, without a derive macro

error[E0599]: no function or associated item named `parse` found for struct `Cli` in the current scope
```

Fix it by enabling the feature:

```bash
cargo add clap --features derive
```

### Using `default_value_t` on a `bool`

A `bool` field is already a flag that defaults to `false`. Trying to give it `default_value_t = false` is redundant and confusing; just declare the field:

```rust
#[arg(long)]
case_sensitive: bool,   // absent → false, present → true
```

If you actually need a flag that defaults to *on* and can be turned off, use an explicit `ArgAction::SetFalse` with a `--no-...` long name, or model it as an `Option<bool>` / enum.

### A field type with no parser

Every non-`bool` field must know how to turn a string into its type. clap can do this for any type that implements `FromStr` or `ValueEnum`, or for which you supply a `value_parser`. Use a plain custom struct and you get a long but informative error (real output):

```rust
#[derive(Debug, Clone)]
struct Color { r: u8, g: u8, b: u8 }   // does not compile (error[E0599])

#[derive(clap::Parser, Debug)]
struct Cli {
    #[arg(long)]
    color: Color,   // clap can't parse a string into Color
}
```

```text
error[E0599]: the method `value_parser` exists for reference `&&&&&&_infer_ValueParser_for<Color>`, but its trait bounds were not satisfied
   |
13 |     #[arg(long)]
   |     ^ method cannot be called on `&&&&&&_infer_ValueParser_for<Color>` due to unsatisfied trait bounds
   |
   = note: the following trait bounds were not satisfied:
           `Color: ValueEnum`
           `Color: ValueParserFactory`
           `Color: From<OsString>`
           ...
```

The fix is to either implement `FromStr` for `Color`, derive `ValueEnum` (if it's a simple enum), or attach a `value_parser = my_fn`.

> **Note:** That cascade of `&&&&&&` references is clap's autoref-based specialization trick for *inferring* a parser from the field type. It looks alarming, but the `note:` lines tell you exactly what's missing: a way to build the type from a string.

### Confusing the two default attributes

`default_value = 3` (an integer where clap expects a string) won't compile; `default_value_t = "3"` (a string where clap wants the field's type) also won't. Remember: `_t` = **t**yped value, plain `default_value` = string. When in doubt, prefer `default_value_t` so the default is checked by the type system.

### Expecting `Option<T>` and getting a required arg

A field declared as `String` is **required**. If the user omits it, clap exits with code 2 before `main` runs (real output):

```text
error: the following required arguments were not provided:
  <PATH>

Usage: probe <PATH>

For more information, try '--help'.
```

If the argument should be optional, make it `Option<String>` (or give it a default). This is the opposite default from commander, where an option with no value is simply `undefined`.

---

## Best Practices

- **Prefer derive over the builder API** for new tools. It is less code, harder to get out of sync, and the type system catches mistakes. Drop to the builder only when you need fully dynamic commands decided at runtime (see [Argument Parsing with clap (Builder API)](/18-cli-tools/00-clap-basics/)).
- **Pull `version`/`about` from `Cargo.toml`** with `#[command(version, about)]` so your CLI's version always matches the crate version.
- **Use precise field types.** Make `port` a `u16`, a path a `PathBuf`, a level a `ValueEnum`. Let clap reject bad input instead of validating later.
- **Use `Option<T>` for genuinely optional inputs and a default for "has a sensible fallback."** Don't reach for `String::new()` sentinels.
- **Group reusable arguments with `#[derive(Args)]` + `#[command(flatten)]`** so several commands can share a block of options:

```rust
use clap::{Args, Parser};

/// Options shared across several commands.
#[derive(Args, Debug)]
struct GlobalOpts {
    /// Suppress all non-error output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Path to the config file
    #[arg(long, default_value = "config.toml")]
    config: String,
}

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// The input file
    input: String,

    #[command(flatten)]
    global: GlobalOpts,
}

fn main() {
    let cli = Cli::parse();
    println!("{cli:#?}");
}
```

Running `app data.csv --quiet` flattens the shared options into the struct (real output):

```text
Cli {
    input: "data.csv",
    global: GlobalOpts {
        quiet: true,
        config: "config.toml",
    },
}
```

- **Validate with `value_parser` functions** that return `Result<T, String>` (or any `Display` error). The `Err` string is shown to the user verbatim, prefixed by clap.
- **Read from the environment with `env = "VAR"`** (needs the `env` feature: `cargo add clap --features derive,env`) so flags can fall back to environment variables, handy for secrets you don't want on the command line. See [Environment Variables](/18-cli-tools/08-environment-vars/).
- **Use `try_parse()` instead of `parse()`** when you want to handle parse failures yourself rather than letting clap exit the process:

```rust
match Cli::try_parse() {
    Ok(cli) => run(cli),
    Err(e) => e.exit(), // prints help/error and exits with the conventional code
}
```

---

## Real-World Example

A miniature `wc` (word count) tool: it parses arguments with derive, reads a file, handles I/O errors gracefully, and returns a proper process exit code. This compiles and runs as shown.

```rust
// src/main.rs
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

/// Count lines, words, and bytes in a file (a tiny `wc`).
#[derive(Parser, Debug)]
#[command(name = "rwc", version, about)]
struct Cli {
    /// File to inspect
    file: PathBuf,

    /// Count lines only
    #[arg(short, long)]
    lines: bool,

    /// Count words only
    #[arg(short, long)]
    words: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let contents = match fs::read_to_string(&cli.file) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("rwc: {}: {err}", cli.file.display());
            return ExitCode::FAILURE;
        }
    };

    let line_count = contents.lines().count();
    let word_count = contents.split_whitespace().count();
    let byte_count = contents.len();

    // If neither flag is set, show all three (like real `wc`).
    let show_all = !cli.lines && !cli.words;

    if cli.lines || show_all {
        print!("{line_count:>8} ");
    }
    if cli.words || show_all {
        print!("{word_count:>8} ");
    }
    if show_all {
        print!("{byte_count:>8} ");
    }
    println!("{}", cli.file.display());

    ExitCode::SUCCESS
}
```

Given a file `sample.txt` containing `hello world\nfoo bar baz\n`, here is the real output:

```text
$ rwc sample.txt
       2        5       24 sample.txt

$ rwc --lines sample.txt
       2 sample.txt

$ rwc /tmp/nope.txt
rwc: /tmp/nope.txt: No such file or directory (os error 2)
$ echo $?
1
```

Notice how returning `ExitCode` from `main` lets the tool report failure to the shell; a script calling `rwc` can check `$?` exactly as it would for a C program. For richer error reporting in larger tools, return `anyhow::Result<()>` from `main` instead (covered in [Section 08](/08-error-handling/)). File reading and buffered I/O are explored further in [File I/O with `std::fs`](/18-cli-tools/06-file-io/), and `PathBuf` handling in [Path and PathBuf](/18-cli-tools/07-path-handling/).

---

## Further Reading

- [clap derive tutorial (official docs)](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html) — the canonical walkthrough.
- [clap derive reference (all attributes)](https://docs.rs/clap/latest/clap/_derive/index.html) — every `#[command(...)]` and `#[arg(...)]` key.
- [`clap::Parser` trait](https://docs.rs/clap/latest/clap/trait.Parser.html) — `parse`, `try_parse`, and friends.
- [Command Line Apps in Rust (the official book)](https://rust-cli.github.io/book/index.html) — broader CLI guidance.
- Sibling pages: [Argument Parsing with clap (Builder API)](/18-cli-tools/00-clap-basics/) (builder API) · [Git-like Subcommands with clap](/18-cli-tools/02-subcommands/) (`#[derive(Subcommand)]`) · [Colored Terminal Output](/18-cli-tools/05-colored-output/) · [Progress Bars and Spinners with indicatif](/18-cli-tools/04-progress-bars/) · [Environment Variables](/18-cli-tools/08-environment-vars/) · [Distributing CLI Tools](/18-cli-tools/10-distribution/).
- Foundations: [Section 02 — Basics](/02-basics/) · [Section 06 — Data Structures (structs & enums)](/06-data-structures/) · [Section 14 — Macros](/14-macros/) explains the derive-macro machinery behind `#[derive(Parser)]`.

---

## Exercises

### Exercise 1: A typed greeter

**Difficulty:** Beginner

**Objective:** Build the smallest possible derive CLI with a required positional argument and an option that has a typed default.

**Instructions:** Write a `greet` tool that takes a required `name` and an optional `--count` / `-c` (default `1`) and prints `Hello, <name>!` that many times.

```rust playground
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about = "Greets a person")]
struct Cli {
    // TODO: a required `name`
    // TODO: a `--count` / `-c` with a typed default of 1
}

fn main() {
    let cli = Cli::parse();
    // TODO: print the greeting `cli.count` times
}
```

<details>
<summary>Solution</summary>

```rust
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about = "Greets a person")]
struct Cli {
    /// Name of the person to greet
    name: String,

    /// Number of times to repeat the greeting
    #[arg(short, long, default_value_t = 1)]
    count: u8,
}

fn main() {
    let cli = Cli::parse();
    for _ in 0..cli.count {
        println!("Hello, {}!", cli.name);
    }
}
```

Running `greet Alice -c 3` prints (real output):

```text
Hello, Alice!
Hello, Alice!
Hello, Alice!
```

</details>

### Exercise 2: Multiple files, verbosity, and a conflict

**Difficulty:** Intermediate

**Objective:** Use a `Vec`, a counting flag, and a mutual-exclusion rule.

**Instructions:** Write a tool that accepts **one or more** file paths as positionals, a `-v` flag that can repeat to raise verbosity, and a `-q` / `--quiet` flag. `--verbose` and `--quiet` must not be allowed together. When not quiet, print the verbosity level and each file being processed.

<details>
<summary>Solution</summary>

```rust
use std::path::PathBuf;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Files to process (one or more)
    #[arg(required = true, num_args = 1..)]
    files: Vec<PathBuf>,

    /// Increase verbosity (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count, conflicts_with = "quiet")]
    verbose: u8,

    /// Suppress output
    #[arg(short, long)]
    quiet: bool,
}

fn main() {
    let cli = Cli::parse();
    if !cli.quiet {
        println!("verbosity={}", cli.verbose);
        for f in &cli.files {
            println!("processing {}", f.display());
        }
    }
}
```

Running `app a.txt b.txt -vv` prints (real output):

```text
verbosity=2
processing a.txt
processing b.txt
```

Running `app -q -v` is rejected at parse time:

```text
error: the argument '--quiet' cannot be used with '--verbose...'
```

And running with no files reports the missing required argument:

```text
error: the following required arguments were not provided:
  <FILES>...
```

</details>

### Exercise 3: Enums, custom validation, and environment fallback

**Difficulty:** Advanced

**Objective:** Combine `ValueEnum`, a custom `value_parser`, and `env`.

**Instructions:** Build a server-config CLI with: a `--log-level` restricted to `debug`/`info`/`warn`/`error` (default `info`); a `--port` (`u16`, default `8080`) that is **rejected** if below `1024` via a custom parser function returning `Result<u16, String>`; and an `--api-key` that falls back to the `API_KEY` environment variable. (Enable the `env` feature: `cargo add clap --features derive,env`.)

<details>
<summary>Solution</summary>

```rust playground
use clap::{Parser, ValueEnum};

#[derive(ValueEnum, Clone, Debug)]
enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

fn parse_port(s: &str) -> Result<u16, String> {
    let port: u16 = s.parse().map_err(|_| format!("`{s}` is not a number"))?;
    if port < 1024 {
        return Err("port must be >= 1024".to_string());
    }
    Ok(port)
}

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Logging level
    #[arg(long, value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,

    /// Port to listen on (>= 1024)
    #[arg(long, default_value_t = 8080, value_parser = parse_port)]
    port: u16,

    /// API key (falls back to API_KEY env var)
    #[arg(long, env = "API_KEY")]
    api_key: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    println!("{cli:#?}");
}
```

Running with `API_KEY=abc app --log-level warn --port 9000` prints (real output):

```text
Cli {
    log_level: Warn,
    port: 9000,
    api_key: Some(
        "abc",
    ),
}
```

Running `app --port 80` is rejected by the custom parser:

```text
error: invalid value '80' for '--port <PORT>': port must be >= 1024

For more information, try '--help'.
```

</details>
