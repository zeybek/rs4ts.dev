---
title: "File I/O with `std::fs`"
description: "Read and write files in Rust with std::fs, where every call returns a Result you must handle and you choose whole-file helpers or a buffered BufReader."
---

## Quick Overview

Almost every command-line tool reads or writes files: a linter slurps source, a log processor streams gigabytes, a config tool writes back settings. In Node you reach for `fs.readFileSync`, `fs.promises.readFile`, or streams; in Rust the equivalent lives in the standard library's `std::fs` and `std::io` modules. No crate required. The two big differences for a TypeScript/JavaScript developer: every fallible operation returns a `Result` you **must** handle (no silent `ENOENT`), and Rust draws a sharp line between cheap whole-file helpers and explicit **buffered** readers/writers for streaming.

This page covers the everyday operations: reading and writing whole files, buffering with `BufReader`/`BufWriter`, and reading a file line by line. Turning a string path into a real, cross-platform `Path` is covered in [Path handling](/18-cli-tools/07-path-handling/); reading configuration from the environment is in [Environment variables](/18-cli-tools/08-environment-vars/).

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. Everything here is in the standard library, so there are no dependencies to add.

---

## TypeScript/JavaScript Example

Here is a small log-filter tool in Node. It reads a file, keeps the lines containing a search term, writes them to a second file, and appends a one-line summary to an audit log.

```typescript
// filter.ts — run with: npx tsx filter.ts access.log errors.log 500
// Uses only Node's built-in `node:fs` — no npm install needed.
import { readFileSync, writeFileSync, appendFileSync } from "node:fs";

const [input, output, needle] = process.argv.slice(2);
if (!input || !output || !needle) {
  console.error("usage: filter <input> <output> <needle>");
  process.exit(2);
}

// Read the whole file into a string (UTF-8 by default).
const text = readFileSync(input, "utf8");

// Filter the lines.
const matches = text.split("\n").filter((line) => line.includes(needle));

// Write the result and append a summary.
writeFileSync(output, matches.join("\n") + "\n");
appendFileSync("audit.log", `filtered ${matches.length} lines from ${input}\n`);

console.log(`wrote ${matches.length} matching line(s) to ${output}`);
```

```text
$ npx tsx filter.ts access.log errors.log 500
wrote 2 matching line(s) to errors.log
```

This is idiomatic Node, but it has two quiet hazards that Rust forces you to confront. First, `readFileSync` loads the **entire file** into memory: fine for a 4 KB config, a problem for a 4 GB log. Second, if `input` does not exist, `readFileSync` throws and the error propagates as an uncaught exception; nothing in the type system reminded you to handle it.

---

## Rust Equivalent

The direct translation uses the whole-file helpers `fs::read_to_string`, `fs::write`, and `OpenOptions` for appending. Notice the `?` after every file call and the `-> io::Result<()>` on `main`.

```rust
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::process::ExitCode;

fn run() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let (input, output, needle) = match args.as_slice() {
        [_, i, o, n] => (i, o, n),
        _ => {
            eprintln!("usage: filter <input> <output> <needle>");
            std::process::exit(2);
        }
    };

    // Read the whole file as a UTF-8 String. `?` returns early on any I/O error.
    let text = fs::read_to_string(input)?;

    // Filter the lines. `lines()` is an iterator; `collect` into a Vec<&str>.
    let matches: Vec<&str> = text.lines().filter(|line| line.contains(needle)).collect();

    // Write the result in one call. Joining with '\n' rebuilds the file body.
    fs::write(output, matches.join("\n") + "\n")?;

    // Append a summary line, creating the audit log if needed.
    let mut audit = OpenOptions::new().create(true).append(true).open("audit.log")?;
    writeln!(audit, "filtered {} lines from {input}", matches.len())?;

    println!("wrote {} matching line(s) to {output}", matches.len());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
```

```text
$ cargo run --quiet -- access.log errors.log 500
wrote 2 matching line(s) to errors.log
```

This is correct and concise, and it mirrors the Node version closely. But like the Node version, it reads the whole file into RAM. The [Real-World Example](#real-world-example) below rewrites it to **stream** the file with `BufReader`, so memory stays flat no matter how large the input is.

---

## Detailed Explanation

### Whole-file helpers: `read_to_string`, `read`, `write`

`std::fs` gives you three one-call helpers that open, do the work, and close the file for you. They are the equivalent of Node's `readFileSync` / `writeFileSync`:

```rust playground
use std::fs;
use std::io;

fn main() -> io::Result<()> {
    fs::write("notes.txt", "first line\nsecond line\nthird line\n")?;

    // Read the file as a UTF-8 String. Errors if the bytes are not valid UTF-8.
    let contents: String = fs::read_to_string("notes.txt")?;
    println!("--- read_to_string ---");
    print!("{contents}");

    // Read the file as raw bytes. Never fails on encoding — bytes are bytes.
    let bytes: Vec<u8> = fs::read("notes.txt")?;
    println!("--- read (bytes) ---");
    println!("{} bytes", bytes.len());

    // `lines()` on a String/str splits on '\n' (and trims a trailing '\r').
    let count = contents.lines().count();
    println!("line count: {count}");

    fs::remove_file("notes.txt")?;
    Ok(())
}
```

```text
--- read_to_string ---
first line
second line
third line
--- read (bytes) ---
34 bytes
line count: 3
```

- **`fs::write(path, data)`** accepts anything that is `AsRef<[u8]>` — a `&str`, a `String`, a `&[u8]`, or a `Vec<u8>`. It **truncates and overwrites**, exactly like `writeFileSync` with no flag. It creates the file if it does not exist.
- **`fs::read_to_string(path)`** returns `io::Result<String>`. It fails with `ErrorKind::InvalidData` if the file is not valid UTF-8. Rust will not hand you a half-broken string. Use `fs::read` for arbitrary bytes.
- **`fs::read(path)`** returns `io::Result<Vec<u8>>` and is the binary-safe counterpart, like `readFileSync(path)` with no encoding argument (which returns a `Buffer` in Node).

> **Note:** `String::lines()` is the precise analogue of JavaScript's `text.split("\n")`, with two refinements: it does **not** yield a trailing empty string when the file ends in a newline, and it strips a trailing `\r` so Windows `\r\n` files just work. We lean on that for cross-platform line handling; see [Cross-platform considerations](/18-cli-tools/09-cross-platform/).

### The `?` operator and `io::Result`

Every fallible call returns `Result<T, std::io::Error>`, aliased as `io::Result<T>`. The `?` operator unwraps the `Ok` value or returns the `Err` from the current function. Because `main` here is declared `-> io::Result<()>`, an unhandled error is printed via its `Debug` representation and the process exits non-zero. The mechanics of `?` are covered in depth in [The `?` operator](/08-error-handling/01-question-mark/); for an error type that adds context, see [`anyhow` and `thiserror`](/08-error-handling/06-anyhow-thiserror/).

### Buffered I/O: `BufReader` and `BufWriter`

A raw `File` performs **one system call per read or write**. Writing 100,000 lines directly to a `File` means 100,000 `write(2)` syscalls. Slow. `BufWriter` batches them into a memory buffer (8 KB by default) and flushes in big chunks; `BufReader` does the symmetric thing for reads. This is the explicit version of what Node's stream layer does for you under the hood.

```rust playground
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};

fn main() -> io::Result<()> {
    // --- buffered writing ---
    let file = File::create("log.txt")?;
    let mut writer = BufWriter::new(file);
    for i in 1..=5 {
        // `writeln!` writes into the buffer, not straight to disk.
        writeln!(writer, "event {i}")?;
    }
    writer.flush()?; // push the buffer to disk before we read it back

    // --- buffered, line-by-line reading ---
    let file = File::open("log.txt")?;
    let reader = BufReader::new(file);

    let mut total = 0usize;
    for line in reader.lines() {
        let line = line?; // each item is io::Result<String>
        if line.contains('3') {
            println!("matched: {line}");
        }
        total += 1;
    }
    println!("read {total} lines");

    std::fs::remove_file("log.txt")?;
    Ok(())
}
```

```text
matched: event 3
read 5 lines
```

Key points:

- **`File::create`** opens for writing, truncating any existing file; **`File::open`** opens read-only and errors if the file is missing.
- **`writeln!` / `write!`** are macros that work on any `Write` target (a `BufWriter`, a `File`, even `Vec<u8>`). They are the file-writing cousins of `println!`/`print!`. You must bring the `Write` trait into scope with `use std::io::Write` to call them; see the [pitfall below](#common-pitfalls).
- **`reader.lines()`** requires the `BufRead` trait (`use std::io::BufRead`). It yields `io::Result<String>` items: each line is a freshly allocated, owned `String` with the line terminator stripped.

### Reading lines: three approaches, three trade-offs

There is more than one way to iterate lines, and the right choice depends on file size and whether you need owned strings.

```rust playground
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};

fn main() -> io::Result<()> {
    fs::write("audit.log", "boot\n")?;

    // Append mode — like fs.appendFile / { flags: "a" } in Node.
    let mut f = fs::OpenOptions::new().append(true).open("audit.log")?;
    writeln!(f, "user logged in")?;
    writeln!(f, "user logged out")?;
    drop(f);

    // Reuse one String buffer across reads to avoid a per-line allocation.
    let file = File::open("audit.log")?;
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    let mut n = 0;
    while reader.read_line(&mut buf)? != 0 {
        print!("{n}: {buf}");
        buf.clear(); // crucial: read_line APPENDS, it does not overwrite
        n += 1;
    }

    fs::remove_file("audit.log")?;
    Ok(())
}
```

```text
0: boot
1: user logged in
2: user logged out
```

The three approaches, ranked from simplest to fastest:

1. **`fs::read_to_string(path)?.lines()`**: read it all, then iterate `&str` slices. Zero per-line allocation, but the whole file is in memory. Best for small-to-medium files.
2. **`BufReader::new(file).lines()`**: streams the file, allocating a new `String` per line. The most readable for big files; the allocation is usually negligible.
3. **`reader.read_line(&mut buf)`** in a loop streams the file and **reuses one buffer**, the lowest-allocation option. Note `read_line` *appends* to `buf` (it does not clear it) and **keeps the trailing `\n`**, so you call `buf.clear()` each iteration. This is the hot-loop choice for multi-gigabyte inputs.

### `OpenOptions`: append, and everything `File::create`/`open` cannot express

`File::open` and `File::create` are shorthands. For anything else — append mode, create-if-missing-but-don't-truncate, create-only-if-new — use `OpenOptions`, the builder equivalent of Node's `fs.open(path, flags)`:

| Node `flags` | `OpenOptions` builder |
| --- | --- |
| `"r"` | `OpenOptions::new().read(true)` (or just `File::open`) |
| `"w"` | `OpenOptions::new().write(true).create(true).truncate(true)` (or `File::create`) |
| `"a"` | `OpenOptions::new().append(true).create(true)` |
| `"wx"` (fail if exists) | `OpenOptions::new().write(true).create_new(true)` |

`.append(true)` implies write and seeks to the end before every write; concurrent appends from multiple processes do not clobber each other on most platforms.

---

## Key Differences

| Concern | Node.js (`node:fs`) | Rust (`std::fs` / `std::io`) |
| --- | --- | --- |
| Missing-file handling | Throws (sync) / rejects (async); easy to forget | Returns `Result`; the compiler warns if you ignore it |
| Default read result | `Buffer`, or `string` with `"utf8"` | `Vec<u8>` (`fs::read`) or `String` (`fs::read_to_string`) |
| Invalid UTF-8 | Silently replaced with `�` in `"utf8"` mode | `read_to_string` errors with `ErrorKind::InvalidData` |
| Buffering | Automatic in the streams layer | Explicit: wrap in `BufReader` / `BufWriter` |
| Flushing | Handled by stream `end()`/GC | You call `.flush()` (or rely on `Drop`, which can hide errors) |
| Line splitting | `text.split("\n")` (keeps trailing empty, keeps `\r`) | `str::lines()` (drops trailing empty, strips `\r`) |
| Sync vs async | `readFileSync` vs `fs.promises` / streams | `std::fs` is blocking; async needs `tokio::fs` |

### `std::fs` is blocking — and that is fine for a CLI

Every `std::fs` call blocks the current thread until the OS finishes. For a typical CLI tool that does its work and exits, blocking is exactly what you want: it is simpler and faster than an async runtime. You only need `tokio::fs` (covered in [Section 11: Async](/11-async/)) when file I/O happens *inside* an async server that must keep serving other requests. Do not reach for async file I/O in a command-line tool by reflex; in JavaScript the async API is the default, in Rust the blocking API is the default for CLIs.

---

## Common Pitfalls

### Ignoring the `Result` from a write

In JavaScript, `writeFileSync` either works or throws; you can fire and forget. In Rust, a `Result` you do not use triggers a warning, because the write may have silently failed (disk full, permission denied).

```rust playground
use std::fs;

fn main() {
    // compiles, but with a warning (unused `Result` that must be used)
    fs::write("out.txt", "data");
    println!("done");
}
```

```text
warning: unused `Result` that must be used
 --> src/main.rs:5:5
  |
5 |     fs::write("out.txt", "data");
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: this `Result` may be an `Err` variant, which should be handled
  = note: `#[warn(unused_must_use)]` on by default
help: use `let _ = ...` to ignore the resulting value
  |
5 |     let _ = fs::write("out.txt", "data");
  |     +++++++
```

Handle it with `?` (and an `io::Result` return type) or `.expect("...")`. Only use `let _ =` when you have genuinely decided the failure is irrelevant.

### Forgetting `use std::io::Write` (or `BufRead`)

`writeln!` on a writer needs the `Write` trait in scope; `.lines()` / `.read_line()` need `BufRead`. Without the import, the methods appear not to exist:

```rust
use std::fs::File;
use std::io::BufWriter; // missing: use std::io::Write;

fn main() -> std::io::Result<()> {
    let mut w = BufWriter::new(File::create("x.txt")?);
    // does not compile (error[E0599]: cannot write into `BufWriter<File>`)
    writeln!(w, "hello")?;
    Ok(())
}
```

The real error names the trait you forgot (the `:::` line points into your local toolchain's copy of the standard library, so its path will differ on your machine):

```text
error[E0599]: cannot write into `BufWriter<File>`
    --> src/main.rs:6:14
     |
   6 |     writeln!(w, "hello")?;
     |              ^
     |
    ::: /home/you/.rustup/toolchains/stable/lib/rustlib/src/rust/library/std/src/io/mod.rs:1950:8
     |
1950 |     fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<()> {
     |        --------- the method is available for `BufWriter<File>` here
     |
note: must implement `io::Write`, `fmt::Write`, or have a `write_fmt` method
    --> src/main.rs:6:14
     |
   6 |     writeln!(w, "hello")?;
     |              ^
     = help: items from traits can only be used if the trait is in scope
help: trait `Write` which provides `write_fmt` is implemented but not in scope; perhaps you want to import it
     |
   1 + use std::io::Write;
     |
```

The fix is exactly what the compiler suggests: add `use std::io::Write;`.

### Dropping a `BufWriter` without flushing

A `BufWriter` flushes its buffer when it is dropped, but the flush at drop time **cannot return an error**, so a failure (disk full, broken pipe) is silently swallowed. Always call `.flush()?` explicitly when you care whether the bytes actually landed:

```rust playground
use std::fs::File;
use std::io::{self, BufWriter, Write};

fn save(path: &str, data: &[&str]) -> io::Result<()> {
    let mut w = BufWriter::new(File::create(path)?);
    for line in data {
        writeln!(w, "{line}")?;
    }
    w.flush()?; // surfaces any error HERE, instead of losing it at drop
    Ok(())
}

fn main() -> io::Result<()> {
    save("ok.txt", &["one", "two"])?;
    std::fs::remove_file("ok.txt")?;
    Ok(())
}
```

> **Warning:** This is a real correctness bug, not a style nit. Without the explicit `flush()`, a program can print "Saved!" and exit 0 while the last buffered chunk never reached disk.

### Expecting `read_to_string` to tolerate non-UTF-8 bytes

Node's `"utf8"` mode quietly substitutes `�` for invalid bytes; `fs::read_to_string` refuses and returns an error. Read raw bytes and convert lossily if you want the Node behavior:

```rust playground
use std::fs;
use std::io;

fn main() -> io::Result<()> {
    fs::write("bin.dat", [0x68, 0x69, 0xFF])?; // 0xFF is never valid UTF-8

    match fs::read_to_string("bin.dat") {
        Ok(s) => println!("text: {s}"),
        Err(e) => println!("read_to_string failed: kind={:?}", e.kind()),
    }

    let raw = fs::read("bin.dat")?;
    // Best-effort, like Buffer.toString("utf8") with replacement chars.
    println!("lossy: {}", String::from_utf8_lossy(&raw));

    fs::remove_file("bin.dat")?;
    Ok(())
}
```

```text
read_to_string failed: kind=InvalidData
lossy: hi�
```

### Forgetting that `read_line` keeps the newline and appends

A surprising number of bugs come from `read_line` not behaving like a "give me the next line, trimmed" function. It **appends** to the buffer (so you must `buf.clear()` each loop) and it **retains** the trailing `\n` (use `line.trim_end()` if you need it gone). The `.lines()` iterator, by contrast, strips the terminator for you.

---

## Best Practices

- **Match the tool to the file size.** Reach for `fs::read_to_string` / `fs::write` for small files (configs, single source files). Switch to `BufReader`/`BufWriter` the moment a file could be large or unbounded, like a log stream.
- **Always wrap a `File` in `BufReader`/`BufWriter` when you read or write in a loop.** Unbuffered per-iteration syscalls are the most common accidental performance cliff.
- **Call `.flush()?` explicitly** on any `BufWriter` whose success you report to the user; do not rely on the silent drop-time flush.
- **Return `io::Result<T>` and propagate with `?`** rather than `.unwrap()` in real tools. Reserve `.unwrap()`/`.expect()` for tests and quick prototypes; see [`unwrap` and `expect`](/08-error-handling/03-unwrap-expect/).
- **Match on `error.kind()`** to recover from expected conditions (a missing optional config file) while still failing loudly on unexpected ones:

```rust playground
use std::fs;
use std::io::ErrorKind;

fn load_config() -> String {
    match fs::read_to_string("config.toml") {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            // Missing config is fine — fall back to defaults.
            String::from("default = true")
        }
        Err(e) => {
            eprintln!("failed to read config: {e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    println!("config = {:?}", load_config());
}
```

```text
config = "default = true"
```

> **Tip:** `ErrorKind::NotFound` is the moral equivalent of checking `err.code === "ENOENT"` in Node, but it is a typed enum variant the compiler knows about, no stringly-typed comparison.

- **Use `fs::exists(path)?`** (stabilized in recent Rust) rather than the older `Path::exists()` when you want to distinguish "does not exist" from "exists but I lack permission to check": `fs::exists` returns `io::Result<bool>` and surfaces the permission error instead of swallowing it.

---

## Real-World Example

Here is the log-filter from the top, rewritten to **stream** through `BufReader`/`BufWriter`. It processes one line at a time, so a 50 GB log uses the same memory as a 50-byte one. It exits with a meaningful status code, so it composes in shell pipelines; exit codes are covered in [Cross-platform considerations](/18-cli-tools/09-cross-platform/).

```rust
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::process::ExitCode;

/// Stream `input`, writing every line that contains `needle` into `output`.
/// Memory stays flat regardless of file size — we never hold the whole file,
/// only one line at a time.
fn filter_file(input: &str, output: &str, needle: &str) -> io::Result<usize> {
    let reader = BufReader::new(File::open(input)?);
    let mut writer = BufWriter::new(File::create(output)?);

    let mut matches = 0;
    for line in reader.lines() {
        let line = line?;
        if line.contains(needle) {
            writeln!(writer, "{line}")?;
            matches += 1;
        }
    }
    writer.flush()?; // make errors surface here, not silently on drop
    Ok(matches)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: {} <input> <output> <needle>", args[0]);
        return ExitCode::from(2);
    }
    match filter_file(&args[1], &args[2], &args[3]) {
        Ok(n) => {
            println!("wrote {n} matching line(s) to {}", args[2]);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
```

Given an `access.log` of:

```text
GET /index 200
POST /login 500
GET /style.css 200
GET /api 500
```

it runs as:

```text
$ cargo run --quiet -- access.log errors.log 500
wrote 2 matching line(s) to errors.log

$ cat errors.log
POST /login 500
GET /api 500

$ cargo run --quiet -- access.log
usage: target/debug/probe <input> <output> <needle>
$ echo $?
2
```

A missing input file no longer crashes with a stack trace; it is caught by `?`, formatted by the `Err` arm, and turned into exit code 1:

```text
$ cargo run --quiet -- nope.log out.log 500
error: No such file or directory (os error 2)
$ echo $?
1
```

In a real tool you would parse these three positional arguments with clap instead of indexing `args` by hand (see [clap derive API](/18-cli-tools/01-clap-derive/)), and you might wrap errors with [`anyhow`](/08-error-handling/06-anyhow-thiserror/) to attach the offending path to the message.

---

## Further Reading

### Official documentation

- [`std::fs` module](https://doc.rust-lang.org/std/fs/index.html) — `read`, `read_to_string`, `write`, `copy`, `metadata`, directory operations
- [`std::io` module](https://doc.rust-lang.org/std/io/index.html) — the `Read`, `Write`, and `BufRead` traits
- [`BufReader`](https://doc.rust-lang.org/std/io/struct.BufReader.html) and [`BufWriter`](https://doc.rust-lang.org/std/io/struct.BufWriter.html)
- [`OpenOptions`](https://doc.rust-lang.org/std/fs/struct.OpenOptions.html) — the flag builder for opening files
- [`std::io::ErrorKind`](https://doc.rust-lang.org/std/io/enum.ErrorKind.html) — the typed error categories (`NotFound`, `PermissionDenied`, ...)
- [Rust Book, Ch. 12 — building a `grep` clone](https://doc.rust-lang.org/book/ch12-00-an-io-project.html)

### Related sections of this guide

- [Path handling](/18-cli-tools/07-path-handling/) — building cross-platform `Path`/`PathBuf` values to feed these functions
- [Environment variables](/18-cli-tools/08-environment-vars/) — reading config from the environment instead of files
- [Cross-platform considerations](/18-cli-tools/09-cross-platform/) — line endings, `\r\n`, and exit codes
- [clap derive API](/18-cli-tools/01-clap-derive/) — parse the file paths these tools take as arguments
- [The `?` operator](/08-error-handling/01-question-mark/) and [`anyhow` / `thiserror`](/08-error-handling/06-anyhow-thiserror/) — reliable error propagation
- [Strings and string slices](/07-collections/01-strings/) — the `String`/`&str` distinction that `read_to_string` and `lines()` rely on
- [Section 11: Async](/11-async/) — when (and when not) to use `tokio::fs` for non-blocking file I/O

---

## Exercises

### Exercise 1: Line numbering

**Difficulty:** Beginner

**Objective:** Practice buffered reading and writing with `BufReader`/`BufWriter`.

**Instructions:** Write a function `number_lines(input: &str, output: &str) -> io::Result<()>` that copies `input` to `output`, prefixing each line with its 1-based number right-aligned in a 4-character field followed by two spaces (so line 1 becomes `   1  <text>`). Use buffered I/O and propagate errors with `?`.

<details>
<summary>Solution</summary>

```rust playground
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};

fn number_lines(input: &str, output: &str) -> io::Result<()> {
    let reader = BufReader::new(File::open(input)?);
    let mut writer = BufWriter::new(File::create(output)?);
    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        writeln!(writer, "{:>4}  {line}", i + 1)?;
    }
    writer.flush()
}

fn main() -> io::Result<()> {
    std::fs::write("in.txt", "alpha\nbeta\ngamma\n")?;
    number_lines("in.txt", "out.txt")?;
    print!("{}", std::fs::read_to_string("out.txt")?);
    std::fs::remove_file("in.txt")?;
    std::fs::remove_file("out.txt")?;
    Ok(())
}
```

```text
   1  alpha
   2  beta
   3  gamma
```

`enumerate()` pairs each line with its index; `{:>4}` right-aligns the number in 4 columns. The final `writer.flush()` (whose `io::Result` becomes the function's return value) guarantees everything reaches disk before `number_lines` returns.

</details>

### Exercise 2: A tiny `wc`

**Difficulty:** Intermediate

**Objective:** Combine whole-file reading with graceful error handling on `ErrorKind`.

**Instructions:** Write `count(path: &str) -> io::Result<(usize, usize, usize)>` returning `(lines, words, bytes)` for a file. Then, in `main`, count a list of paths and, for any file that does not exist, print `<path>: no such file` to stderr and continue with the rest instead of aborting. (Hint: `str::split_whitespace` counts words; `str::len` counts bytes.)

<details>
<summary>Solution</summary>

```rust playground
use std::fs;
use std::io::{self, ErrorKind};

fn count(path: &str) -> io::Result<(usize, usize, usize)> {
    let text = fs::read_to_string(path)?;
    let lines = text.lines().count();
    let words = text.split_whitespace().count();
    let bytes = text.len();
    Ok((lines, words, bytes))
}

fn main() {
    fs::write("sample.txt", "the quick brown fox\njumps over\n").unwrap();
    for path in ["sample.txt", "missing.txt"] {
        match count(path) {
            Ok((l, w, b)) => println!("{l:>3} {w:>3} {b:>3} {path}"),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                eprintln!("{path}: no such file");
            }
            Err(e) => eprintln!("{path}: {e}"),
        }
    }
    fs::remove_file("sample.txt").unwrap();
}
```

```text
  2   6  31 sample.txt
missing.txt: no such file
```

The `Err(e) if e.kind() == ErrorKind::NotFound` guard handles the expected "file missing" case, while a final `Err(e)` arm still reports anything unexpected (like a permission error). Because each path is handled independently inside the loop, one missing file does not stop the others.

</details>

### Exercise 3: Streaming `uniq`

**Difficulty:** Advanced

**Objective:** Process an arbitrarily large file in constant memory using `read_line` with a reused buffer.

**Instructions:** Write `uniq(input: &str, output: &str) -> io::Result<usize>` that copies `input` to `output`, collapsing **consecutive** identical lines into one (like the Unix `uniq` command). Return the number of lines written. Constraint: you must not load the whole file into memory; read one line at a time, comparing only against the previous line.

<details>
<summary>Solution</summary>

```rust playground
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};

/// Like `uniq`: drop a line if it is identical to the line just written.
/// Uses two reused buffers so memory does not grow with the file.
fn uniq(input: &str, output: &str) -> io::Result<usize> {
    let mut reader = BufReader::new(File::open(input)?);
    let mut writer = BufWriter::new(File::create(output)?);

    let mut current = String::new();
    let mut previous = String::new();
    let mut written = 0;
    let mut first = true;

    while reader.read_line(&mut current)? != 0 {
        if first || current != previous {
            write!(writer, "{current}")?; // read_line keeps the '\n'
            written += 1;
            first = false;
        }
        std::mem::swap(&mut previous, &mut current);
        current.clear();
    }
    writer.flush()?;
    Ok(written)
}

fn main() -> io::Result<()> {
    std::fs::write("dup.txt", "a\na\nb\nb\nb\na\n")?;
    let n = uniq("dup.txt", "uniq.txt")?;
    println!("kept {n} lines:");
    print!("{}", std::fs::read_to_string("uniq.txt")?);
    std::fs::remove_file("dup.txt")?;
    std::fs::remove_file("uniq.txt")?;
    Ok(())
}
```

```text
kept 3 lines:
a
b
a
```

The trick is two `String` buffers swapped with `std::mem::swap`: after writing `current`, it becomes the new `previous` (no allocation, just a pointer swap), and `current.clear()` readies it for the next `read_line`. Because `read_line` retains the trailing `\n`, we use `write!` (not `writeln!`) so we do not double the newlines. Memory is bounded by the longest single line, not the file size.

</details>
