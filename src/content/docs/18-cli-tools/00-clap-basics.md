---
title: "Argument Parsing with clap (Builder API)"
description: "Parse CLI args in Rust with clap's builder API, the commander/yargs equivalent: flags, options, typed values, auto-generated --help, --version, validation."
---

## Quick Overview

In Node you reach for `commander` or `yargs` to turn `process.argv` into structured options. Rust's equivalent is **clap** (Command-Line Argument Parser), the dominant crate for the job. This page covers clap's **builder API** (describing arguments, flags, and options with explicit method calls), which is the closest mental model to how `commander`/`yargs` are configured in JavaScript, and which gives you full control plus auto-generated `--help`, `--version`, and validation for free.

> **Note:** clap also has a `#[derive(Parser)]` API where you describe your CLI as a struct. That is the more idiomatic, less verbose approach and is covered in [clap derive API](/18-cli-tools/01-clap-derive/). Subcommands (the `git commit`-style verbs) are covered in [Subcommands](/18-cli-tools/02-subcommands/). This page deliberately stays on the builder API so you can see what the derive macro generates under the hood.

The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. The examples here use **clap 4.6**.

---

## TypeScript/JavaScript Example

Here is a small greeting tool built with `commander`, the most popular Node CLI library. It takes a required name, an option with a value, and a boolean flag, and `commander` generates `--help` and `--version` automatically.

```typescript
// greet.ts — run with: npx tsx greet.ts Alice --times 2 --shout
// Depends on: npm install commander
import { Command } from "commander";

const program = new Command();

program
  .name("greet")
  .version("1.0.0")
  .description("Greets a person from the command line")
  .argument("<name>", "who to greet")
  .option("-t, --times <n>", "how many times to repeat the greeting", "1")
  .option("-s, --shout", "print the greeting in upper case", false)
  .action((name: string, opts: { times: string; shout: boolean }) => {
    const times = parseInt(opts.times, 10) || 1;
    for (let i = 0; i < times; i++) {
      let greeting = `Hello, ${name}!`;
      if (opts.shout) greeting = greeting.toUpperCase();
      console.log(greeting);
    }
  });

program.parse();
```

```text
$ npx tsx greet.ts Alice --times 2 --shout
HELLO, ALICE!
HELLO, ALICE!
```

Note the JavaScript reality: `opts.times` arrives as the **string** `"1"`, never a number. `commander` does no type conversion unless you pass a custom parser, so `parseInt` is on you. A typo like `--times abc` silently becomes `NaN`, and the `|| 1` fallback papers over it. clap will not let that slide.

---

## Rust Equivalent

The same tool in Rust with clap's builder API. Add the dependency first:

```bash
cargo add clap
```

```rust
// src/main.rs
use clap::{Arg, ArgAction, Command};

fn main() {
    let matches = Command::new("greet")
        .version("1.0.0")
        .author("Jane Dev <jane@example.com>")
        .about("Greets a person from the command line")
        // Positional argument (required by default)
        .arg(
            Arg::new("name")
                .help("Who to greet")
                .required(true)
                .index(1),
        )
        // Option that takes a value: --times 3 / -t 3
        .arg(
            Arg::new("times")
                .short('t')
                .long("times")
                .help("How many times to repeat the greeting")
                .value_name("N")
                .value_parser(clap::value_parser!(usize))
                .default_value("1"),
        )
        // Boolean flag: --shout / -s
        .arg(
            Arg::new("shout")
                .short('s')
                .long("shout")
                .help("Print the greeting in upper case")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    let name = matches.get_one::<String>("name").expect("required");
    // clap parsed and validated `--times` as a `usize`, so a bad value like
    // `--times abc` is rejected with a clear error before `main` runs — no
    // silent fallback (contrast the JS `|| 1` above).
    let times: usize = *matches.get_one::<usize>("times").expect("has a default");
    let shout = matches.get_flag("shout");

    for _ in 0..times {
        let mut greeting = format!("Hello, {name}!");
        if shout {
            greeting = greeting.to_uppercase();
        }
        println!("{greeting}");
    }
}
```

Running it produces the same result:

```text
$ cargo run --quiet -- Alice --times 2 --shout
HELLO, ALICE!
HELLO, ALICE!
```

And `--help` is generated for you — no separate usage string to maintain:

```text
$ cargo run --quiet -- --help
Greets a person from the command line

Usage: greet [OPTIONS] <name>

Arguments:
  <name>  Who to greet

Options:
  -t, --times <N>  How many times to repeat the greeting [default: 1]
  -s, --shout      Print the greeting in upper case
  -h, --help       Print help
  -V, --version    Print version
```

> **Note:** The program name shown in `Usage:` comes from `Command::new("greet")`. When you run via `cargo run`, the auto-detected binary name (your crate name, e.g. `probe`) is used instead unless you also call `.bin_name("greet")`. Once the tool is installed and invoked directly as `greet`, the right name appears automatically.

---

## Detailed Explanation

### Building the `Command`

```rust
let matches = Command::new("greet")
    .version("1.0.0")
    .author("Jane Dev <jane@example.com>")
    .about("Greets a person from the command line")
    // ... .arg(...) calls ...
    .get_matches();
```

`Command` is the root of your CLI definition: the direct analogue of `commander`'s `new Command()`. Each method (`.version`, `.about`, `.arg`) returns the `Command` back so you can chain calls, exactly like `commander`'s fluent API. The terminal call `.get_matches()` does the real work: it reads `std::env::args()`, parses them against your definition, and, importantly, **exits the process itself** if parsing fails or if the user asked for `--help`/`--version`. That is why it returns `ArgMatches` directly rather than a `Result`: by the time control returns, the arguments are known-valid.

### Three kinds of arguments

clap models three things, all via the same `Arg` type:

1. **Positional arguments**: identified by position, not a flag. `Arg::new("name").index(1)` is the first positional. By default an `Arg` with no `short`/`long` is positional.
2. **Options** — flags that *take a value*: `Arg::new("times").short('t').long("times")`. The presence of `short`/`long` plus a value-taking action makes it an option.
3. **Flags** — boolean switches that take *no* value: `.action(ArgAction::SetTrue)` makes `--shout` a true/false toggle.

This maps cleanly onto `commander`'s `.argument()` vs `.option()` distinction, with `ArgAction` controlling the flag-vs-option behavior.

### `required` and defaults

```rust
Arg::new("name").required(true)         // positional, must be supplied
Arg::new("times").default_value("1")    // option, optional, falls back to "1"
```

Unlike `commander`, where `<name>` (angle brackets) means required and `[name]` means optional inside the argument string, clap is explicit: `.required(true)` or `.default_value(...)`. A positional argument is required by default; giving it a `default_value` makes it optional.

### Reading parsed values

```rust
let name = matches.get_one::<String>("name").expect("required");
let shout = matches.get_flag("shout");
```

You pull values out by the **id** you gave each `Arg`. The accessor must match how the argument was defined:

| Argument kind | Accessor | Returns |
| --- | --- | --- |
| Single value (option/positional) | `get_one::<T>("id")` | `Option<&T>` |
| Multiple values | `get_many::<T>("id")` | `Option<impl Iterator<Item = &T>>` |
| `ArgAction::SetTrue` flag | `get_flag("id")` | `bool` |
| `ArgAction::Count` flag | `get_count("id")` | `u8` |

By default an option's value type is `String`, so `get_one::<String>` is correct above. The `<T>` is not magic: clap stored the value as that type during parsing, and asking for the wrong `T` panics (a pitfall we cover below).

### Type conversion is opt-in but real

In the example we parsed `times` from a `String` by hand with `.parse()`. That works, but it pushes validation past clap. The idiomatic builder approach is `value_parser!`, which makes clap parse and validate the value *before* `get_matches()` returns. See Best Practices.

---

## Key Differences

### clap validates and exits for you

The deepest difference from a typical hand-rolled `process.argv` loop is that clap owns the failure path. When parsing fails, clap prints a formatted error to **stderr** and exits with status **2** (the conventional "usage error" code), so you never write that boilerplate.

```text
$ cargo run --quiet -- --times abc Bob
error: invalid value 'abc' for '--times <N>': invalid digit found in string

For more information, try '--help'.
```

(That message appears once you attach a `value_parser!(u32)`; see below.) In `commander` you would typically discover the bad value later, or not at all if a `parseInt` quietly returns `NaN`.

### Comparison table

| Concept | commander / yargs (Node) | clap builder (Rust) |
| --- | --- | --- |
| Define a CLI | `new Command()` | `Command::new("name")` |
| Required positional | `.argument("<name>")` | `Arg::new("name").required(true)` |
| Optional positional | `.argument("[name]")` | `Arg::new("name").default_value(...)` or omit `required` |
| Option with value | `.option("-t, --times <n>")` | `Arg::new("times").short('t').long("times")` |
| Boolean flag | `.option("-s, --shout")` | `.action(ArgAction::SetTrue)` |
| Repeatable / counted | `.option("-v", "...", increaseVerbosity, 0)` | `.action(ArgAction::Count)` |
| Multiple values | `.option("--tag <t...>")` | `.action(ArgAction::Append)` |
| Type conversion | manual `parseInt`, or a custom parser fn | `.value_parser(value_parser!(u32))` |
| Default value | 3rd arg to `.option(...)` | `.default_value("...")` |
| Auto `--help`/`--version` | yes | yes |
| Invalid input | up to you; often silent `NaN`/`undefined` | clap prints error + exits code 2 |

### Values are typed at the door

In JavaScript every option value is a string until you convert it. In Rust, once you attach a `value_parser!`, the value lives in `ArgMatches` *as the real type* (`u32`, `PathBuf`, an enum, etc.), already validated. The conversion happens once, at parse time, instead of scattered `parseInt` calls.

### The builder API is "stringly-typed" by id

You refer to arguments by string ids (`"name"`, `"times"`). A typo in an id is not caught at compile time: `get_one::<String>("naem")` compiles and panics at runtime. This is precisely the ergonomic weakness the [derive API](/18-cli-tools/01-clap-derive/) fixes by turning each argument into a real struct field.

---

## Common Pitfalls

### Pitfall 1: Using the wrong accessor for a flag

A `SetTrue` flag is stored as a `bool`, not a `String`. Asking for the wrong type compiles fine but panics at runtime:

```rust
use clap::{Arg, ArgAction, Command};

fn main() {
    let matches = Command::new("demo")
        .arg(Arg::new("verbose").long("verbose").action(ArgAction::SetTrue))
        .get_matches();

    // does not work at runtime: a SetTrue flag is a bool, not a String
    let v = matches.get_one::<String>("verbose");
    println!("{v:?}");
}
```

```text
$ cargo run --quiet -- --verbose
thread 'main' panicked at src/main.rs:9:21:
Mismatch between definition and access of `verbose`. Could not downcast to alloc::string::String, need to downcast to bool

note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

**Fix:** use `matches.get_flag("verbose")` for `SetTrue`, `get_count` for `Count`, and `get_one::<T>` only for value-bearing args where `T` matches the `value_parser`.

### Pitfall 2: Forgetting `ArgAction::SetTrue` on a boolean flag

In clap 4, an `Arg` with no explicit action and no value defaults to expecting a value. If you intend a bare flag but forget `.action(ArgAction::SetTrue)`, clap will demand a value after `--shout`, which is not what you want. Always set the action for flags.

### Pitfall 3: Mistyping an argument id

The builder API ties definitions and lookups together by string id. There is no compile-time check that `get_one("times")` matches `Arg::new("times")`. A typo such as `get_one::<String>("time")` builds successfully and then panics with `Mismatch between definition and access` at runtime. Keep ids in `const`s, or move to the [derive API](/18-cli-tools/01-clap-derive/) where fields are checked by the compiler.

### Pitfall 4: Expecting `get_one` to return the value directly

`get_one::<String>("name")` returns `Option<&String>`, not `String`. New Rustaceans coming from JavaScript expect the value itself. You must handle the `Option` (`.unwrap()`, `.expect(...)`, or pattern-match) and you get a reference, not an owned value. For a `required(true)` arg, `.expect("required")` is safe because clap guarantees it is present.

### Pitfall 5: Reading `--help` output from stdout in tests

clap prints normal `--help`/`--version` to **stdout** and exits with code 0, but prints *errors* to **stderr** and exits with code 2. If you test your CLI by capturing output, capture the right stream for the case you are asserting.

---

## Best Practices

### Use `value_parser!` for typed, validated values

Instead of pulling a `String` and calling `.parse()`, let clap own conversion and validation. `value_parser!(T)` works for any type clap knows how to parse (integers, floats, `bool`, `PathBuf`, and more):

```rust
use clap::{value_parser, Arg, ArgAction, Command};

fn main() {
    let matches = Command::new("greet")
        .arg(Arg::new("name").required(true))
        // value_parser teaches clap the target type; clap parses & validates for you
        .arg(
            Arg::new("times")
                .short('t')
                .long("times")
                .value_name("N")
                .value_parser(value_parser!(u32))
                .default_value("1"),
        )
        // Count occurrences: -vvv => 3 (classic verbosity)
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::Count),
        )
        // Accept many values: --tag a --tag b
        .arg(
            Arg::new("tag")
                .long("tag")
                .action(ArgAction::Append),
        )
        .get_matches();

    let name = matches.get_one::<String>("name").unwrap();
    let times = *matches.get_one::<u32>("times").unwrap(); // already a u32
    let verbosity = matches.get_count("verbose");
    let tags: Vec<&String> = matches
        .get_many::<String>("tag")
        .map(|vals| vals.collect())
        .unwrap_or_default();

    println!("name={name} times={times} verbosity={verbosity} tags={tags:?}");
}
```

```text
$ cargo run --quiet -- Bob -t 3 -vv --tag x --tag y
name=Bob times=3 verbosity=2 tags=["x", "y"]
```

Now bad input is rejected automatically, with a clear message and the conventional exit code 2:

```text
$ cargo run --quiet -- Bob -t notanumber
error: invalid value 'notanumber' for '--times <N>': invalid digit found in string

For more information, try '--help'.
```

Note `get_one::<u32>` now returns `Option<&u32>`: the value is a real `u32`, so we dereference with `*`. No `parseInt`, no `NaN`.

### Restrict an option to a fixed set of choices

For enum-like options, `PossibleValuesParser` both validates the input and lists the choices in `--help` and in error messages:

```rust
use clap::{builder::PossibleValuesParser, Arg, Command};

fn main() {
    let m = Command::new("log")
        .arg(
            Arg::new("level")
                .long("level")
                .required(true)
                .value_parser(PossibleValuesParser::new(["debug", "info", "warn", "error"])),
        )
        .arg(Arg::new("message").required(true).value_name("MSG"))
        .get_matches();

    let level = m.get_one::<String>("level").unwrap();
    let message = m.get_one::<String>("message").unwrap();
    println!("[{}] {message}", level.to_uppercase());
}
```

```text
$ cargo run --quiet -- --level trace "x"
error: invalid value 'trace' for '--level <level>'
  [possible values: debug, info, warn, error]

For more information, try '--help'.
```

### Express mutual exclusivity with `conflicts_with`

Rather than checking combinations by hand after parsing, declare the constraint and let clap enforce it:

```rust playground
use clap::{Arg, ArgAction, Command};

fn main() {
    let matches = Command::new("color")
        .bin_name("color")
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .short('v')
                .action(ArgAction::SetTrue)
                .conflicts_with("quiet"),
        )
        .arg(
            Arg::new("quiet")
                .long("quiet")
                .short('q')
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    println!(
        "verbose={} quiet={}",
        matches.get_flag("verbose"),
        matches.get_flag("quiet")
    );
}
```

```text
$ cargo run --quiet -- --verbose --quiet
error: the argument '--verbose' cannot be used with '--quiet'

Usage: color --verbose

For more information, try '--help'.
```

### Set `.bin_name(...)` so usage strings read correctly under `cargo run`

As noted earlier, the displayed program name defaults to the binary name. Set `.bin_name("yourtool")` (or rely on the installed binary name) so `Usage:` lines match what users actually type.

### Factor the `Command` into its own function

Returning the `Command` from a `build_cli()` function keeps `main` small and lets you reuse the definition for tests, completion generation, and man-page generation. The real-world example below does this.

### Prefer the derive API once the builder grows

The builder API is excellent for learning and for dynamic CLIs, but for most tools the [derive API](/18-cli-tools/01-clap-derive/) is shorter and removes the stringly-typed id pitfalls. The concepts transfer one-to-one; this page is the foundation.

---

## Real-World Example

A miniature `wc`-style tool that counts lines and words in files. It shows positional multi-values (`ArgAction::Append`), boolean flags that change behavior, a sensible "show everything by default" rule, and proper process exit codes via `std::process::ExitCode`.

```rust
// src/main.rs
use clap::{Arg, ArgAction, Command};
use std::fs;
use std::process::ExitCode;

fn build_cli() -> Command {
    Command::new("lc")
        .bin_name("lc")
        .version("0.2.0")
        .about("Count lines and words in files (a tiny `wc`)")
        .arg(
            Arg::new("files")
                .help("Files to read")
                .action(ArgAction::Append)
                .value_name("FILE"),
        )
        .arg(
            Arg::new("lines")
                .short('l')
                .long("lines")
                .help("Show the line count")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("words")
                .short('w')
                .long("words")
                .help("Show the word count")
                .action(ArgAction::SetTrue),
        )
}

fn main() -> ExitCode {
    let matches = build_cli().get_matches();

    let show_lines = matches.get_flag("lines");
    let show_words = matches.get_flag("words");
    // When neither flag is set, show both (classic wc behavior)
    let (show_lines, show_words) = if !show_lines && !show_words {
        (true, true)
    } else {
        (show_lines, show_words)
    };

    let files: Vec<&String> = matches
        .get_many::<String>("files")
        .map(|v| v.collect())
        .unwrap_or_default();

    if files.is_empty() {
        eprintln!("lc: no input files");
        return ExitCode::from(2);
    }

    let mut had_error = false;
    for path in files {
        match fs::read_to_string(path) {
            Ok(contents) => {
                let lines = contents.lines().count();
                let words = contents.split_whitespace().count();
                let mut parts = Vec::new();
                if show_lines {
                    parts.push(lines.to_string());
                }
                if show_words {
                    parts.push(words.to_string());
                }
                println!("{} {path}", parts.join(" "));
            }
            Err(err) => {
                eprintln!("lc: {path}: {err}");
                had_error = true;
            }
        }
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
```

Behavior (given a `sample.txt` with two lines and five words):

```text
$ cargo run --quiet -- sample.txt
2 5 sample.txt

$ cargo run --quiet -- --lines sample.txt
2 sample.txt

$ cargo run --quiet -- nope.txt
lc: nope.txt: No such file or directory (os error 2)
```

The last run exits with code 1 (`ExitCode::FAILURE`), while an unknown flag is caught by clap before your code runs at all:

```text
$ cargo run --quiet -- --bogus sample.txt
error: unexpected argument '--bogus' found

  tip: to pass '--bogus' as a value, use '-- --bogus'

Usage: lc [OPTIONS] [FILE]...

For more information, try '--help'.
```

> **Tip:** Returning `ExitCode` from `main` is the clean way to set the process status. For the file reading itself, see [File I/O](/18-cli-tools/06-file-io/); for richer error reporting, pair clap with `anyhow` as shown in [Section 08: Error Handling](/08-error-handling/).

---

## Key Concepts Reference

| Builder method | Purpose |
| --- | --- |
| `Command::new(name)` | Create the root command |
| `.version(...)` / `.about(...)` / `.author(...)` | Metadata shown in `--help`/`--version` |
| `.bin_name(...)` | Override the displayed program name |
| `Arg::new(id)` | Define an argument by id |
| `.short('x')` / `.long("name")` | Make it an option/flag with `-x` / `--name` |
| `.index(n)` | Position of a positional argument |
| `.required(true)` | Must be supplied |
| `.default_value("...")` | Fallback when omitted |
| `.value_name("N")` | Placeholder shown in help |
| `.value_parser(value_parser!(T))` | Parse & validate into type `T` |
| `.action(ArgAction::SetTrue)` | Boolean flag (no value) |
| `.action(ArgAction::Count)` | Counted flag (`-vvv` → 3) |
| `.action(ArgAction::Append)` | Collect multiple values |
| `.conflicts_with("id")` | Mutual exclusivity constraint |
| `matches.get_one::<T>("id")` | Read a single value (`Option<&T>`) |
| `matches.get_many::<T>("id")` | Read multiple values |
| `matches.get_flag("id")` | Read a `SetTrue` flag (`bool`) |
| `matches.get_count("id")` | Read a `Count` flag (`u8`) |

---

## Further Reading

### Official documentation

- [clap crate documentation (docs.rs)](https://docs.rs/clap/latest/clap/)
- [clap builder tutorial](https://docs.rs/clap/latest/clap/_tutorial/index.html)
- [`Command` API](https://docs.rs/clap/latest/clap/struct.Command.html) and [`Arg` API](https://docs.rs/clap/latest/clap/struct.Arg.html)
- [`ArgAction` enum](https://docs.rs/clap/latest/clap/enum.ArgAction.html)
- [`value_parser!` macro](https://docs.rs/clap/latest/clap/macro.value_parser.html)

### Related sections of this guide

- [clap derive API](/18-cli-tools/01-clap-derive/): the idiomatic, struct-based way to define the same CLIs without stringly-typed ids
- [Subcommands](/18-cli-tools/02-subcommands/) — `git`-style verbs (`commit`, `push`) built on top of `Command`
- [Colored output](/18-cli-tools/05-colored-output/) and [Progress bars](/18-cli-tools/04-progress-bars/): make the tool's output friendlier
- [File I/O](/18-cli-tools/06-file-io/) and [Path handling](/18-cli-tools/07-path-handling/) — what the parsed paths feed into
- [Environment variables](/18-cli-tools/08-environment-vars/): clap can also pull defaults from env vars
- [Cross-platform builds](/18-cli-tools/09-cross-platform/) — building and shipping the finished binary across operating systems
- [Section 00: Introduction](/00-introduction/) and [Section 01: Getting Started](/01-getting-started/) — toolchain and `cargo` basics if you are just starting
- [Section 08: Error Handling](/08-error-handling/): pairing clap with `Result`-based error reporting
- [Section 19: WebAssembly](/19-wasm/) — when your tool targets the browser instead of the terminal

---

## Exercises

### Exercise 1: A flag and an option

**Difficulty:** Beginner

**Objective:** Practice defining a boolean flag and a value-bearing option with the builder API and reading them back with the correct accessors.

**Instructions:** Build a `Command` named `echo` that accepts a required positional `text`, a `--upper`/`-u` boolean flag, and a `--repeat`/`-r` option (default `"1"`). Print `text` `repeat` times, uppercased if `--upper` is set.

```rust playground
use clap::{Arg, ArgAction, Command};

fn main() {
    let matches = Command::new("echo")
        // TODO: add the `text` positional, the `upper` flag, the `repeat` option
        .get_matches();

    // TODO: read values and print
}
```

<details>
<summary>Solution</summary>

```rust
use clap::{Arg, ArgAction, Command};

fn main() {
    let matches = Command::new("echo")
        .arg(Arg::new("text").required(true).value_name("TEXT"))
        .arg(
            Arg::new("upper")
                .short('u')
                .long("upper")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("repeat")
                .short('r')
                .long("repeat")
                .value_name("N")
                .default_value("1"),
        )
        .get_matches();

    let text = matches.get_one::<String>("text").unwrap();
    let upper = matches.get_flag("upper");
    let repeat: usize = matches
        .get_one::<String>("repeat")
        .unwrap()
        .parse()
        .unwrap_or(1);

    let out = if upper { text.to_uppercase() } else { text.clone() };
    for _ in 0..repeat {
        println!("{out}");
    }
}
```

Verified output:

```text
$ cargo run --quiet -- hi -u -r 2
HI
HI
```

</details>

### Exercise 2: Typed values with `value_parser!`

**Difficulty:** Intermediate

**Objective:** Replace manual `.parse()` with clap's `value_parser!` so invalid input is rejected automatically.

**Instructions:** Build a `ping`-like tool with a required `host` positional, a `--count`/`-c` option typed as `u16` (default `4`), and a `--quiet`/`-q` flag. Print a fake ping line per count unless `--quiet`, then a summary. Confirm that `--count notnum` produces a clap error with exit code 2.

```rust playground
use clap::{value_parser, Arg, ArgAction, Command};

fn main() {
    let m = Command::new("ping")
        // TODO: host, count (u16, default 4), quiet flag
        .get_matches();

    // TODO: read host (String), count (u16), quiet (bool) and print
}
```

<details>
<summary>Solution</summary>

```rust
use clap::{value_parser, Arg, ArgAction, Command};

fn main() {
    let m = Command::new("ping")
        .about("Pretend to ping a host")
        .arg(Arg::new("host").required(true).value_name("HOST"))
        .arg(
            Arg::new("count")
                .short('c')
                .long("count")
                .value_name("N")
                .value_parser(value_parser!(u16))
                .default_value("4"),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    let host = m.get_one::<String>("host").unwrap();
    let count = *m.get_one::<u16>("count").unwrap();
    let quiet = m.get_flag("quiet");

    for i in 1..=count {
        if !quiet {
            println!("PING {host}: seq={i}");
        }
    }
    println!("{count} packets transmitted to {host}");
}
```

Verified output:

```text
$ cargo run --quiet -- example.com -c 2
PING example.com: seq=1
PING example.com: seq=2
2 packets transmitted to example.com

$ cargo run --quiet -- example.com --count notnum
error: invalid value 'notnum' for '--count <N>': invalid digit found in string

For more information, try '--help'.
```

</details>

### Exercise 3: Constrained choices and a required option

**Difficulty:** Advanced

**Objective:** Restrict an option to a fixed set of values with `PossibleValuesParser`, and observe the auto-generated help and error messages.

**Instructions:** Build a `log` tool with a required `--level` option limited to `debug`, `info`, `warn`, `error`, plus a required positional `message`. Print the message prefixed with the upper-cased level. Confirm that an invalid level lists the allowed values.

```rust playground
use clap::{builder::PossibleValuesParser, Arg, Command};

fn main() {
    let m = Command::new("log")
        // TODO: required --level limited to the four levels; required message positional
        .get_matches();

    // TODO: print "[LEVEL] message"
}
```

<details>
<summary>Solution</summary>

```rust
use clap::{builder::PossibleValuesParser, Arg, Command};

fn main() {
    let m = Command::new("log")
        .about("Emit a log line at a chosen level")
        .arg(
            Arg::new("level")
                .long("level")
                .required(true)
                .value_parser(PossibleValuesParser::new(["debug", "info", "warn", "error"])),
        )
        .arg(Arg::new("message").required(true).value_name("MSG"))
        .get_matches();

    let level = m.get_one::<String>("level").unwrap();
    let message = m.get_one::<String>("message").unwrap();
    println!("[{}] {message}", level.to_uppercase());
}
```

Verified output:

```text
$ cargo run --quiet -- --level warn "disk almost full"
[WARN] disk almost full

$ cargo run --quiet -- --level trace "x"
error: invalid value 'trace' for '--level <level>'
  [possible values: debug, info, warn, error]

For more information, try '--help'.
```

</details>

---

_Next: [clap derive API](/18-cli-tools/01-clap-derive/) — define the same CLIs as a struct and let the macro handle the ids._
