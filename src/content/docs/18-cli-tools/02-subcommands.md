---
title: "Git-like Subcommands with clap"
description: "Model git-style verbs like commit and remote add as a Rust enum with clap's Subcommand derive, getting exhaustive matching and nested help commander can't."
---

Many serious command-line tools are not a single command but a **family** of commands sharing one binary: `git commit`, `git remote add`, `cargo build`, `docker container ls`. In Rust, clap's `#[derive(Subcommand)]` turns a plain enum into exactly this kind of dispatch tree, with help, validation, and nesting generated for you.

---

## Quick Overview

A **subcommand** is a verb that selects one branch of your program (`add`, `commit`, `remote`). With clap's derive API you model each verb as a variant of an `enum`, attach `#[derive(Subcommand)]`, and embed that enum in your top-level `#[derive(Parser)]` struct. clap then parses `mytool <verb> <args…>`, generates per-command `--help`, and lets you `match` on a strongly-typed value: no string switching, no manual `process.argv` slicing.

For a TypeScript/JavaScript developer this is the same job that `commander`'s `program.command("add")` or `yargs`'s `.command()` does, but the result is an exhaustive enum the compiler forces you to handle completely.

> **Note:** This page assumes you already know clap's derive basics. If `#[derive(Parser)]`, `#[arg(...)]`, and default values are new to you, read [Parsing Arguments with the clap Derive API](/18-cli-tools/01-clap-derive/) first; for the lower-level builder API see [Argument Parsing with clap (Builder API)](/18-cli-tools/00-clap-basics/).

---

## TypeScript/JavaScript Example

A typical Node CLI with [`commander`](https://www.npmjs.com/package/commander) defines each verb with a callback. Here is a small git-like tool with a nested `remote` command group:

```typescript
// cli.mts — run with: node cli.mts <args>
// Requires: npm install commander
import { Command } from "commander";

const program = new Command();
program
  .name("rgit")
  .description("A tiny git-like CLI")
  .version("1.0.0");

program
  .command("init")
  .description("Create an empty repository")
  .argument("[path]", "where to create the repository", ".") // optional, default "."
  .action((path: string) => {
    console.log(`Initialized empty repository in ${path}`);
  });

program
  .command("add")
  .description("Add file contents to the staging area")
  .argument("<files...>", "files to stage") // 1 or more
  .option("-a, --all", "stage every tracked file")
  .action((files: string[], opts: { all?: boolean }) => {
    if (opts.all) console.log("Staging all tracked files");
    else console.log(`Staging ${files.length} file(s): ${files.join(", ")}`);
  });

// A nested command group: `rgit remote add <name> <url>`
const remote = program
  .command("remote")
  .description("Manage the set of tracked repositories");

remote
  .command("add")
  .argument("<name>")
  .argument("<url>")
  .action((name: string, url: string) => {
    console.log(`Added remote '${name}' -> ${url}`);
  });

program.parse();
```

**Key points about the commander version:**

- Each `command()` registers a verb; `action()` receives positional arguments and a parsed-options object.
- Argument syntax is encoded in **strings**: `"<files...>"` means "required, variadic"; `"[path]"` means "optional".
- The `opts` object is typed only because *you* wrote the `{ all?: boolean }` annotation; commander itself returns `any`. A typo like `opts.al` compiles and silently reads `undefined`.
- Nesting is done by calling `.command()` on a sub-`Command` instance.

---

## Rust Equivalent

In clap, the verbs become an `enum`, each variant carrying its own typed arguments. The compiler, not a hand-written annotation, guarantees the types:

```rust
// Cargo.toml: clap = { version = "4", features = ["derive"] }
use clap::{Parser, Subcommand};

/// A tiny git-like version control tool.
#[derive(Parser)]
#[command(name = "rgit", version, about = "A tiny git-like CLI")]
struct Cli {
    /// Enable verbose output across all commands
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create an empty repository
    Init {
        /// Where to create the repository
        #[arg(default_value = ".")]
        path: String,
    },
    /// Add file contents to the staging area
    Add {
        /// Files to stage
        #[arg(required = true)]
        files: Vec<String>,
        /// Stage every tracked file
        #[arg(short, long)]
        all: bool,
    },
    /// Record staged changes to the repository
    Commit {
        /// The commit message
        #[arg(short, long)]
        message: String,
    },
}

fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        eprintln!("[verbose mode on]");
    }

    match cli.command {
        Commands::Init { path } => {
            println!("Initialized empty repository in {path}");
        }
        Commands::Add { files, all } => {
            if all {
                println!("Staging all tracked files");
            } else {
                println!("Staging {} file(s): {}", files.len(), files.join(", "));
            }
        }
        Commands::Commit { message } => {
            println!("Committed with message: {message}");
        }
    }
}
```

Running it (here the binary is named `rgit`; with `cargo run` you would write `cargo run -- init myrepo`):

```text
$ rgit init myrepo
Initialized empty repository in myrepo

$ rgit add src/main.rs Cargo.toml
Staging 2 file(s): src/main.rs, Cargo.toml

$ rgit commit -m "first commit"
Committed with message: first commit
```

If you run it with no subcommand, clap prints the auto-generated help and exits with status `2`:

```text
$ rgit
A tiny git-like CLI

Usage: rgit [OPTIONS] <COMMAND>

Commands:
  init    Create an empty repository
  add     Add file contents to the staging area
  commit  Record staged changes to the repository
  help    Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output across all commands
  -h, --help     Print help
  -V, --version  Print version
```

All of this output is generated by clap from the `enum` and its doc comments. You wrote zero help text by hand.

---

## Detailed Explanation

### The two derives work together

```rust
#[derive(Parser)]          // the top-level container
struct Cli {
    #[command(subcommand)] // "the chosen verb lives here"
    command: Commands,
}

#[derive(Subcommand)]      // marks an enum as a set of verbs
enum Commands { /* ... */ }
```

- `#[derive(Parser)]` implements `Cli::parse()`, which reads `std::env::args_os()`, parses it, and on any error prints a message and exits.
- `#[command(subcommand)]` on a field tells clap "the user picks exactly one variant of this enum, by name." Each variant becomes a verb; its name is the lowercase/`kebab-case` version of the variant identifier (`Commit` → `commit`).
- `#[derive(Subcommand)]` on the enum is what makes that legal. The two attributes are a pair: the field attribute references an enum that carries the enum derive.

> **Note:** clap derives `Subcommand` *and* requires the enum to be `Clone`-able internally. If you forget `#[command(subcommand)]`, clap instead tries to treat the field as a *single positional value* of type `Commands`, which fails to compile — see [Common Pitfalls](#common-pitfalls).

### Variants carry their own arguments

Each variant is either a unit variant (`Status`) or a **struct-like variant** whose named fields become that verb's arguments:

```rust
Commit {
    #[arg(short, long)]   // -m / --message
    message: String,      // required (not Option<_>) → clap demands it
},
```

The field-level rules are identical to a normal `#[derive(Parser)]` struct (covered in [Parsing Arguments with the clap Derive API](/18-cli-tools/01-clap-derive/)):

- A plain `String`/number field is a **required** positional or option.
- `Option<T>` makes it optional; `bool` makes it a flag; `Vec<T>` makes it variadic.
- `#[arg(short, long)]` turns a positional into a named option; `#[arg(default_value = "…")]` supplies a fallback.

### Matching is exhaustive

```rust
match cli.command {
    Commands::Init { path } => { /* ... */ }
    Commands::Add { files, all } => { /* ... */ }
    Commands::Commit { message } => { /* ... */ }
}
```

Because `command` is an `enum`, the `match` must cover every variant. Add a new verb later and the compiler refuses to build until you handle it. In commander/yargs nothing reminds you that a new command needs wiring; a forgotten `.action()` is silently a no-op.

### Global flags

```rust
#[arg(short, long, global = true)]
verbose: bool,
```

`global = true` means `--verbose` is accepted *after any subcommand too*: `rgit add -a --verbose` and `rgit --verbose add -a` both set it. Without `global`, the flag would only be valid before the verb. This mirrors how `git --no-pager log` vs `git log` behaves.

### The program name vs. the binary name

`#[command(name = "rgit")]` sets the name used in `--version` output and as the program's logical name. The name shown in **usage strings**, however, defaults to the actual file name of the running binary (`argv[0]`). Under `cargo run` that file is named after your package; once installed as `rgit` the usage line reads `Usage: rgit …`. To force a fixed display name regardless of how the binary is invoked, add `#[command(bin_name = "rgit")]`.

---

## Nested Subcommands

Real tools nest verbs: `git remote add`, `docker image prune`, `cargo install`. You nest by giving a variant its **own** `#[command(subcommand)]` field pointing at a second `Subcommand` enum:

```rust
// Cargo.toml: clap = { version = "4", features = ["derive"] }
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rgit", version, about = "A tiny git-like CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage the set of tracked repositories
    Remote {
        #[command(subcommand)]
        action: RemoteAction,
    },
    /// Show the working tree status
    Status,
}

#[derive(Subcommand)]
enum RemoteAction {
    /// Add a new remote
    Add {
        /// Short name for the remote, e.g. "origin"
        name: String,
        /// The remote URL
        url: String,
    },
    /// Remove an existing remote
    Remove {
        /// The remote to remove
        name: String,
    },
    /// List configured remotes
    List,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Status => println!("On branch main, nothing to commit"),
        Commands::Remote { action } => match action {
            RemoteAction::Add { name, url } => println!("Added remote '{name}' -> {url}"),
            RemoteAction::Remove { name } => println!("Removed remote '{name}'"),
            RemoteAction::List => println!("origin"),
        },
    }
}
```

```text
$ rgit remote add origin https://example.com/repo.git
Added remote 'origin' -> https://example.com/repo.git

$ rgit remote list
origin
```

Each level gets its own help page automatically:

```text
$ rgit remote --help
Manage the set of tracked repositories

Usage: rgit remote <COMMAND>

Commands:
  add     Add a new remote
  remove  Remove an existing remote
  list    List configured remotes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

The nesting is just enums-within-enums, and the matching is just `match`-within-`match`. There is no depth limit; `docker`-style three-level trees (`tool group sub action`) compose the same way.

---

## Key Differences

| Concept | TypeScript (`commander`/`yargs`) | Rust (`clap` derive) |
| --- | --- | --- |
| How a verb is declared | `program.command("add")` + `.action(cb)` | An `enum` variant `Add { … }` |
| Argument arity | Encoded in strings: `"<x>"`, `"[x]"`, `"<x...>"` | Encoded in the type: `String`, `Option<T>`, `Vec<T>` |
| Argument types | Strings by default; you cast/validate manually | Parsed into real types (`u16`, `PathBuf`, enums) |
| Dispatch | Each command has a callback closure | One value you `match` on |
| Forgetting to handle a command | Silent no-op at runtime | **Compile error** (non-exhaustive `match`) |
| Help / usage / version | Mostly automatic, some manual `.description()` | Fully automatic from doc comments + attributes |
| Nesting | `.command()` on a sub-`Command` | A nested `#[command(subcommand)]` enum |
| Unknown subcommand | You handle it (or it errors) | clap errors with exit code `2` by default |

The deepest difference is **where the contract lives**. In commander, the shape of a command is data assembled at runtime; mistakes surface when a user runs the wrong path. In clap, the shape is the *type system*: an unhandled verb or a misread field is a build failure. For a TypeScript developer, think of it as the difference between validating with a hand-written `if (typeof x === "string")` and having a discriminated union the compiler checks for you, except clap also *parses the input into* that union.

> **Tip:** The clap enum is a discriminated union, exactly like a TypeScript `type Cmd = { kind: "add"; … } | { kind: "commit"; … }`. The `match` is your exhaustive `switch (cmd.kind)`, and Rust enforces the `default`-is-not-needed exhaustiveness that TypeScript only gives you with `never` tricks.

---

## Common Pitfalls

### Forgetting `#[command(subcommand)]`

If you embed the enum but omit the attribute, clap does not see "a set of verbs"; it tries to parse the field as one positional *value*, which the enum cannot satisfy. The error is about missing trait bounds, not a friendly "you forgot an attribute":

```rust
// does not compile (error[E0277]/error[E0599])
use clap::{Parser, Subcommand};

#[derive(Parser)]
struct Cli {
    command: Commands, // missing #[command(subcommand)] !
}

#[derive(Subcommand)]
enum Commands {
    Status,
}

fn main() {
    let _ = Cli::parse();
}
```

Real compiler output:

```text
error[E0277]: the trait bound `Commands: Clone` is not satisfied
 --> src/main.rs:6:14
  |
6 |     command: Commands,
  |              ^^^^^^^^ the trait `Clone` is not implemented for `Commands`
  |
note: required by a bound in `ArgMatches::remove_one`
help: consider annotating `Commands` with `#[derive(Clone)]`
```

The fix is to add the attribute (`#[command(subcommand)] command: Commands`), not to chase the `Clone` suggestion. The misleading hint is exactly why this trips up newcomers.

### Making the subcommand optional by accident — or on purpose

A bare `Commands` field means a subcommand is **required**; run the tool with none and clap errors. Sometimes you *want* a default action (like `cargo` running `cargo build`-ish behavior, or a tool that prints status when invoked alone). Use `Option<Commands>`:

```rust
// Cargo.toml: clap = { version = "4", features = ["derive"] }
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rgit", version = "1.0.0", about = "A tiny git-like CLI")]
#[command(arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show the working tree status (the default action)
    Status,
    /// Record staged changes
    Commit {
        #[arg(short, long)]
        message: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Status) | None => println!("On branch main, working tree clean"),
        Some(Commands::Commit { message }) => println!("Committed: {message}"),
    }
}
```

Here `None` is treated the same as `status`. Note `#[command(arg_required_else_help = true)]`: with an `Option` subcommand, clap would otherwise accept "no arguments" silently; this attribute makes a bare invocation print help instead. Verified output:

```text
$ rgit --version
rgit 1.0.0

$ rgit              # arg_required_else_help → prints help, exits 2
A tiny git-like CLI

Usage: rgit [COMMAND]

Commands:
  status  Show the working tree status (the default action)
  commit  Record staged changes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Expecting `--version`/`--help` to print the `name`, not the binary name

As noted earlier, `--version` uses `#[command(name = "rgit")]` (it printed `rgit 1.0.0` above), but the **usage** line uses the binary's real `argv[0]`. Under `cargo run` you will see your package name in usage strings; that is not a bug. Set `#[command(bin_name = "rgit")]` if you need it pinned.

### Required positional after an optional one

Within a single variant, the same ordering rules as any clap struct apply: a required positional cannot follow an optional/variadic one, or clap will reject the definition at parse-setup time. Keep `Vec<T>` and `Option<T>` positionals last. (See [Parsing Arguments with the clap Derive API](/18-cli-tools/01-clap-derive/) for the full ordering rules.)

### Verbose-flag placement

Without `global = true`, a top-level flag is only accepted *before* the subcommand. Users will naturally type `rgit add file --verbose`; if `--verbose` is not global, that errors with "unexpected argument." Mark cross-cutting flags `global = true`.

---

## Best Practices

- **One enum variant per verb; nest with sub-enums.** Keep each `Subcommand` enum focused; deep trees read better as several small enums than one giant one.
- **Document with doc comments, not `help =`.** clap turns `/// Add a new remote` into the command's help text. This keeps the source self-documenting and the `--help` output in sync.
- **Put shared arguments in a `#[derive(Args)]` struct** and embed it with `#[command(flatten)]`, so several verbs can reuse the same option set without duplication (shown in the real-world example below).
- **Use `Option<Commands>` + `arg_required_else_help = true`** when a no-subcommand invocation should show help rather than error opaquely.
- **Add `visible_alias`/`alias` for ergonomic shortcuts** (`rgit a` for `add`) when it matches user expectations from the tool you are emulating.
- **Keep the `match` exhaustive and let the compiler guard new verbs.** Resist a catch-all `_ =>` arm so adding a command forces you to wire it.
- **Validate values with types and `value_enum`,** not hand-written checks: a `Priority` enum gives free `[possible values: …]` help and rejects bad input for you.

---

## Real-World Example

A small project task runner, `devctl`, combining everything: a top-level verb (`add`) with a `ValueEnum` option, a nested config command group, a shared `#[derive(Args)]` struct flattened into one variant, and a `visible_alias`.

```rust
// Cargo.toml: clap = { version = "4", features = ["derive"] }
use clap::{Args, Parser, Subcommand, ValueEnum};

/// devctl — a project task runner.
#[derive(Parser)]
#[command(name = "devctl", version, about = "Manage project tasks and config")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new task
    #[command(visible_alias = "a")]
    Add {
        /// What needs doing
        title: String,
        /// Task priority
        #[arg(short, long, value_enum, default_value_t = Priority::Medium)]
        priority: Priority,
    },
    /// Operate on project configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Read a config value
    Get { key: String },
    /// Write a config value
    Set(SetArgs), // a tuple variant holding a flattened Args struct
}

/// Reusable argument set for the `config set` command.
#[derive(Args)]
struct SetArgs {
    key: String,
    value: String,
    /// Apply to the global config instead of the project
    #[arg(long)]
    global: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, ValueEnum)]
enum Priority {
    Low,
    Medium,
    High,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Add { title, priority } => {
            println!("Added task {title:?} with {priority:?} priority");
        }
        Commands::Config { action } => match action {
            ConfigAction::Get { key } => println!("config get {key}"),
            ConfigAction::Set(SetArgs { key, value, global }) => {
                let scope = if global { "global" } else { "project" };
                println!("config set {key}={value} ({scope})");
            }
        },
    }
}
```

Verified runs (with `#[derive(Debug)]` on `Priority`, clap's default `Debug` prints the variant name like `High`):

```text
$ devctl a "write the docs" --priority high     # 'a' is the alias for 'add'
Added task "write the docs" with High priority

$ devctl add "review PR"                          # uses default_value_t
Added task "review PR" with Medium priority

$ devctl config set editor vim --global
config set editor=vim (global)

$ devctl config get editor
config get editor
```

The `ValueEnum` gives the priority option self-validating help and rejects bad input automatically:

```text
$ devctl add --help
Add a new task

Usage: devctl add [OPTIONS] <TITLE>

Arguments:
  <TITLE>  What needs doing

Options:
  -p, --priority <PRIORITY>  Task priority [default: medium] [possible values: low, medium, high]
  -h, --help                 Print help

$ devctl add x -p urgent
error: invalid value 'urgent' for '--priority <PRIORITY>'
  [possible values: low, medium, high]

For more information, try '--help'.
```

The tuple variant `Set(SetArgs)` shows the `#[command(flatten)]`-by-construction pattern: a `#[derive(Args)]` struct can be reused across multiple commands, and the variant simply holds it. You destructure it in the `match` arm exactly like any struct.

> **Tip:** For tools that pair subcommands with progress feedback or colored status lines, combine this dispatch with [Progress Bars and Spinners with indicatif](/18-cli-tools/04-progress-bars/) and [Colored Terminal Output](/18-cli-tools/05-colored-output/). For reading the files a verb operates on, see [File I/O with `std::fs`](/18-cli-tools/06-file-io/) and [Path and PathBuf](/18-cli-tools/07-path-handling/).

---

## Further Reading

- [clap derive reference (docs.rs)](https://docs.rs/clap/latest/clap/_derive/index.html) — every `#[command(...)]` and `#[arg(...)]` attribute.
- [`Subcommand` trait (docs.rs)](https://docs.rs/clap/latest/clap/trait.Subcommand.html) — what the derive implements.
- [clap cookbook: git-style subcommands](https://docs.rs/clap/latest/clap/_derive/_cookbook/index.html) — official worked examples, including `external_subcommand`.
- [Argument Parsing with clap (Builder API)](/18-cli-tools/00-clap-basics/) — the builder API and how args/flags/options/help work underneath the derive.
- [Parsing Arguments with the clap Derive API](/18-cli-tools/01-clap-derive/) — `#[derive(Parser)]`, arg attributes, and default values (read this before this page).
- [Cross-Platform CLI Considerations](/18-cli-tools/09-cross-platform/) — exit codes (clap exits `2` on a usage error) and platform notes for CLIs.
- [Distributing CLI Tools](/18-cli-tools/10-distribution/) — shipping your multi-command binary via `cargo install` and prebuilt artifacts.
- [Section 08: Error Handling](/08-error-handling/) — returning `Result` from command handlers instead of `panic!`/`exit`.
- [Section 06: Data Structures](/06-data-structures/) — enums and pattern matching, the foundation of clap subcommands.
- Next: [Section 19: WebAssembly](/19-wasm/).

---

## Exercises

### Exercise 1: Add a `log` subcommand

**Difficulty:** Beginner

**Objective:** Extend the first git-like CLI with a new verb and feel the exhaustiveness check.

**Instructions:** Starting from the `Commands` enum in the [Rust Equivalent](#rust-equivalent) section, add a `Log` variant that takes an optional `--max-count`/`-n` numeric option (default `10`). Build *before* adding the `match` arm and observe the compiler complaining about the missing case, then handle it by printing `Showing up to N commits`.

<details>
<summary>Solution</summary>

```rust
// Cargo.toml: clap = { version = "4", features = ["derive"] }
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rgit", version, about = "A tiny git-like CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Record staged changes
    Commit {
        #[arg(short, long)]
        message: String,
    },
    /// Show commit history
    Log {
        /// Limit the number of commits shown
        #[arg(short = 'n', long = "max-count", default_value_t = 10)]
        max_count: u32,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Commit { message } => println!("Committed: {message}"),
        Commands::Log { max_count } => println!("Showing up to {max_count} commits"),
    }
}
```

Verified runs:

```text
$ rgit log
Showing up to 10 commits

$ rgit log -n 3
Showing up to 3 commits
```

If you skip the `Commands::Log` arm, the build fails with `error[E0004]: non-exhaustive patterns: `Commands::Log { .. }` not covered`. The compiler will not let you forget the new verb.

</details>

### Exercise 2: A nested key-value store with a missing-key exit code

**Difficulty:** Intermediate

**Objective:** Build a tool with `set`/`get`/`keys` verbs and return a non-zero exit status when a key is missing: the Rust equivalent of `process.exit(1)`.

**Instructions:** Create a `kv` CLI with three subcommands: `set <key> <value>`, `get <key>`, and `keys`. Seed an in-memory `HashMap` with one entry. On `get` of a missing key, print an error to stderr and exit with status `1`. (You will not persist between runs — that is fine for the exercise.)

<details>
<summary>Solution</summary>

```rust
// Cargo.toml: clap = { version = "4", features = ["derive"] }
use clap::{Parser, Subcommand};
use std::collections::HashMap;

#[derive(Parser)]
#[command(name = "kv", version, about = "A toy key-value store CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Store a value under a key
    Set { key: String, value: String },
    /// Retrieve a value by key
    Get { key: String },
    /// List all keys
    Keys,
}

fn main() {
    // A real tool would persist this; we hard-code a store for the demo.
    let mut store: HashMap<String, String> = HashMap::new();
    store.insert("name".into(), "ada".into());

    let cli = Cli::parse();
    match cli.command {
        Command::Set { key, value } => {
            store.insert(key.clone(), value.clone());
            println!("set {key} = {value}");
        }
        Command::Get { key } => match store.get(&key) {
            Some(v) => println!("{v}"),
            None => {
                eprintln!("error: key {key:?} not found");
                std::process::exit(1);
            }
        },
        Command::Keys => {
            let mut keys: Vec<_> = store.keys().cloned().collect();
            keys.sort();
            for k in keys {
                println!("{k}");
            }
        }
    }
}
```

Verified runs:

```text
$ kv get name
ada

$ kv get missing ; echo "exit=$?"
error: key "missing" not found
exit=1

$ kv keys
name
```

</details>

### Exercise 3: External (plugin-style) subcommands

**Difficulty:** Advanced

**Objective:** Let unknown verbs be captured instead of rejected, so your tool can dispatch to `tool-<name>` plugins the way `git` calls `git-<name>` and `cargo` calls `cargo-<name>`.

**Instructions:** Define a CLI with one built-in `hello` command and a catch-all variant marked `#[command(external_subcommand)]` that captures the verb plus its remaining arguments as a `Vec<OsString>`. When an external subcommand is matched, print what you *would* dispatch to. (Actually `std::process::Command::new`-ing the plugin is optional bonus work.)

<details>
<summary>Solution</summary>

```rust
// Cargo.toml: clap = { version = "4", features = ["derive"] }
use clap::{Parser, Subcommand};
use std::ffi::OsString;

#[derive(Parser)]
#[command(name = "tool", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// A built-in command
    Hello,
    /// Anything else is forwarded to an external `tool-<name>` binary
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Hello => println!("hello from the built-in"),
        Commands::External(args) => {
            // args[0] is the unknown verb, the rest are its arguments.
            println!("would dispatch to tool-{:?}", args);
            // Bonus: actually run it, e.g.
            // let (verb, rest) = args.split_first().unwrap();
            // std::process::Command::new(format!("tool-{}", verb.to_string_lossy()))
            //     .args(rest).status().ok();
        }
    }
}
```

Verified runs:

```text
$ tool hello
hello from the built-in

$ tool frobnicate --flag x
would dispatch to tool-["frobnicate", "--flag", "x"]
```

The `external_subcommand` variant must hold a `Vec<OsString>` (or `Vec<String>`); clap funnels the unrecognized verb and everything after it into that vector instead of erroring.

</details>
