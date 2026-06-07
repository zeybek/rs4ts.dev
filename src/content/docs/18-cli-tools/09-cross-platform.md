---
title: "Cross-Platform CLI Considerations"
description: "Ship one Rust CLI that runs everywhere: handle line endings, path separators, OS detection with cfg!, and typed ExitCode where Node leans on process.platform."
---

A CLI you ship is run on machines you will never see: a colleague's Windows laptop, a Linux CI runner, your own Mac. The bytes a program reads and writes, the way it spells a file path, and the number it returns when it exits all differ subtly between operating systems. Rust gives you compile-time tools (`#[cfg(...)]`, `cfg!(...)`) and portable standard-library types (`Path`, `ExitCode`) that let you write **one** program that behaves correctly everywhere.

---

## Quick Overview

Cross-platform correctness is mostly about four things: **line endings** (`\n` vs `\r\n`), **paths** (separator, drive letters, case-sensitivity), **OS detection** (`cfg!(windows)` / `#[cfg(...)]`), and **exit codes** (the integer your process returns to the shell). Rust's standard library models these portably: `Path`/`PathBuf` abstract over separators, `ExitCode` types your process result, and `cfg!` lets a single binary branch on the target OS. For a TypeScript/JavaScript developer, this is the same role played by `os.EOL`, `path.sep`, `process.platform`, and `process.exitCode` in Node, but Rust pushes most of it into the type system and the compiler, so a wrong assumption tends to fail to compile rather than corrupt a file in production.

> **Note:** This page covers the *cross-platform mindset*: line endings, OS detection, and exit codes. The mechanics of building and manipulating paths live in [Path and PathBuf](/18-cli-tools/07-path-handling/), reading and writing files in [File I/O with `std::fs`](/18-cli-tools/06-file-io/), environment variables in [Environment Variables](/18-cli-tools/08-environment-vars/), and shipping binaries for each platform in [Distributing CLI Tools](/18-cli-tools/10-distribution/). Here we focus on the gotchas that bite when the *same* code runs on a *different* OS.

---

## TypeScript/JavaScript Example

A Node CLI that normalizes a file's line endings, then reports a meaningful exit status. This is the kind of tool you might run in a pre-commit hook.

```typescript
// crlf.ts — run with: npx tsx crlf.ts --lf file.txt
import { readFileSync, writeFileSync } from "node:fs";
import { EOL } from "node:os";
import { sep } from "node:path";
import { basename } from "node:path";

const [mode, ...files] = process.argv.slice(2);

// Node tells you about the host platform at runtime:
console.error(`platform=${process.platform} eol=${JSON.stringify(EOL)} sep=${JSON.stringify(sep)}`);

if (mode !== "--lf" && mode !== "--crlf") {
  console.error("usage: crlf <--lf|--crlf> <file>...");
  process.exit(2); // 2 = usage error, by convention
}

const target = mode === "--lf" ? "\n" : "\r\n";
let failed = false;

for (const file of files) {
  try {
    const text = readFileSync(file, "utf8");
    // Collapse every CRLF to LF first, then expand to the target.
    const normalized = text.replace(/\r\n/g, "\n").replace(/\n/g, target);
    if (normalized !== text) {
      writeFileSync(file, normalized);
      console.log(`converted ${basename(file)}`);
    } else {
      console.log(`unchanged ${basename(file)}`);
    }
  } catch (err) {
    console.error(`crlf: ${file}: ${(err as Error).message}`);
    failed = true;
  }
}

// Setting process.exitCode lets the event loop drain before exiting.
process.exitCode = failed ? 1 : 0;
```

**Key points and weak spots:**

- `process.platform` is a runtime string (`"win32"`, `"darwin"`, `"linux"`). There is no compile step, so a platform-specific bug only shows up when that platform runs the code.
- `os.EOL` is `"\r\n"` on Windows and `"\n"` everywhere else, but `readFileSync(..., "utf8")` does **not** normalize endings for you; you do it by hand with a regex.
- `process.exit(n)` terminates *immediately* and can truncate buffered `stdout`; `process.exitCode = n` is the safer idiom. Exit codes are plain JavaScript numbers with no type checking. Passing `256` silently wraps to `0`.
- `path.sep` and `path.basename` keep you off the literal `/`, but nothing stops you from writing `file.split("/")` and shipping a Windows bug.

---

## Rust Equivalent

The same tool in idiomatic Rust. Notice that line endings are handled with the standard library, paths never touch a literal separator, and the exit status is a typed `ExitCode` rather than a bare integer.

```rust
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

/// The newline style a file should use.
#[derive(Clone, Copy)]
enum Ending {
    Lf,
    Crlf,
}

/// How `run` failed, so `main` can pick a conventional exit code.
enum RunError {
    /// A bad invocation the caller can fix: unknown/missing mode, no files.
    Usage(String),
    /// A file could not be read or written.
    Io(String),
}

/// Normalize every line ending in `text` to `target`.
fn normalize(text: &str, target: Ending) -> String {
    // Collapse CRLF -> LF first so mixed-ending files become uniform,
    // then expand to the requested style. This is deterministic.
    let lf = text.replace("\r\n", "\n");
    match target {
        Ending::Lf => lf,
        Ending::Crlf => lf.replace('\n', "\r\n"),
    }
}

fn run() -> Result<(), RunError> {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .ok_or_else(|| RunError::Usage("usage: crlf <--lf|--crlf> <file>...".to_string()))?;
    let target = match mode.as_str() {
        "--lf" => Ending::Lf,
        "--crlf" => Ending::Crlf,
        other => return Err(RunError::Usage(format!("unknown mode: {other}"))),
    };

    let files: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if files.is_empty() {
        return Err(RunError::Usage("no input files".to_string()));
    }

    for path in &files {
        let text = fs::read_to_string(path)
            .map_err(|e| RunError::Io(format!("{}: {e}", path.display())))?;
        let out = normalize(&text, target);
        if out != text {
            fs::write(path, &out).map_err(|e| RunError::Io(format!("{}: {e}", path.display())))?;
            println!("converted {}", path.display());
        } else {
            println!("unchanged {}", path.display());
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    if cfg!(windows) {
        eprintln!("platform=windows");
    } else {
        eprintln!("platform=unix");
    }

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(RunError::Usage(msg)) => {
            eprintln!("crlf: {msg}");
            ExitCode::from(2) // 2 = usage error, mirroring the Node version
        }
        Err(RunError::Io(msg)) => {
            eprintln!("crlf: {msg}");
            ExitCode::from(1) // 1 = I/O failure
        }
    }
}
```

Running it against a CRLF file on macOS/Linux produces this **real** output (the file is rewritten with LF-only line endings, verified with a hex dump):

```text
platform=unix
converted /tmp/test.txt
```

A second run is a no-op, and a missing file exits with status `1`:

```text
platform=unix
unchanged /tmp/test.txt
```

```text
platform=unix
crlf: /tmp/does-not-exist.txt: No such file or directory (os error 2)
```

The shell sees exit code `2` for a usage error (an unknown mode, a missing mode, or no input files), `1` for an I/O failure, and `0` for success: the same contract the Node version encodes with `process.exit(2)` and `process.exitCode = 1`, and exactly what a Makefile, CI step, or pre-commit hook depends on. The `RunError` enum is what keeps the two failure classes distinct so `main` can map each to the right code.

---

## Detailed Explanation

### Line endings

A text file's line ending is just bytes. Unix-family systems (Linux, macOS) use a single line feed `\n` (`0x0A`). Windows uses a carriage-return + line-feed pair `\r\n` (`0x0D 0x0A`), inherited from typewriters and DOS. The classic cross-platform bug is reading a Windows file on Linux and getting a stray `\r` on the end of every "line".

Rust's standard library leans your way here. `str::lines()` splits on `\n` **and strips a trailing `\r`** if present, so it transparently handles both styles:

```rust
fn main() {
    let windows_text = "line one\r\nline two\r\nline three\r\n";
    for (i, line) in windows_text.lines().enumerate() {
        // `{line:?}` prints with quotes, so a stray `\r` would be visible.
        println!("{i}: {line:?}");
    }
}
```

Real output. Note that there is **no** trailing `\r` in any line:

```text
0: "line one"
1: "line two"
2: "line three"
```

This is the same convenience as JavaScript's `text.split(/\r?\n/)`, but built into the iterator you would reach for anyway. The catch: `lines()` only helps on **read**. When you **write**, Rust emits exactly the bytes you give it: `writeln!` and `println!` always emit `\n`, never `\r\n`, on every platform. That is usually what you want (LF is the portable default and Git normalizes for you), but if you must produce CRLF for a Windows-only consumer, do it explicitly:

```rust
fn main() {
    // Collapse to LF, then expand — handles mixed-ending input.
    let normalized = "a\r\nb\nc".replace("\r\n", "\n");
    println!("LF form  = {normalized:?}");
    let crlf = normalized.replace('\n', "\r\n");
    println!("CRLF form = {crlf:?}");
}
```

Real output:

```text
LF form  = "a\nb\nc"
CRLF form = "a\r\nb\r\nc"
```

> **Tip:** Reading with `fs::read_to_string` then writing with `fs::write` preserves the bytes you produce; it does **not** silently convert endings. This is the opposite of some text editors and unlike Python's text-mode `open()`. If you want LF everywhere, normalize on write.

### Detecting the OS: `cfg!` vs `#[cfg(...)]`

Rust gives you two related tools, and the difference matters.

**`cfg!(...)`** is a macro that evaluates to a `bool` *at compile time* but is used like a normal runtime expression. Both branches of the surrounding `if` must type-check and compile, even on the platform where the condition is false:

```rust
fn main() {
    let shell = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
    println!("default shell: {shell}");
}
```

Real output on macOS:

```text
default shell: /bin/sh
```

**`#[cfg(...)]`** is an *attribute* that includes or **excludes the item entirely** before type-checking. Code behind a `#[cfg(windows)]` that doesn't compile on macOS is simply not compiled there, which lets you call Windows-only APIs without `#[cfg]`-ing every use:

```rust
#[cfg(windows)]
fn config_template() -> &'static str {
    "%APPDATA%\\mytool" // only compiled on Windows
}

#[cfg(not(windows))]
fn config_template() -> &'static str {
    "$HOME/.config/mytool"
}

fn main() {
    println!("config: {}", config_template());
}
```

Real output on macOS:

```text
config: $HOME/.config/mytool
```

You can also query the target at runtime via `std::env::consts`:

```rust
fn main() {
    println!("OS         = {}", std::env::consts::OS);          // "macos", "windows", "linux"
    println!("FAMILY     = {}", std::env::consts::FAMILY);      // "unix" or "windows"
    println!("EXE_SUFFIX = {:?}", std::env::consts::EXE_SUFFIX);// ".exe" on Windows, "" elsewhere
}
```

Real output on macOS:

```text
OS         = macos
FAMILY     = unix
EXE_SUFFIX = ""
```

These are baked in at compile time for the *target* you built for, so cross-compiling from a Mac to Windows reports `windows`/`.exe`, not your host. The common `cfg` keys are `target_os` (`"windows"`, `"macos"`, `"linux"`, `"android"`, …), `target_family` (`"unix"`, `"windows"`, `"wasm"`), `target_arch` (`"x86_64"`, `"aarch64"`, …), and the shorthands `unix` / `windows`.

> **Note:** Prefer `#[cfg(...)]` when an entire function or `use` only makes sense on one OS, and `cfg!(...)` for a small inline branch where both arms compile everywhere. `#[cfg(...)]` keeps platform-specific imports out of the other platform's build entirely.

### Paths

Paths are the densest source of portability bugs, so they get their own page ([Path and PathBuf](/18-cli-tools/07-path-handling/)). The cross-platform rule of thumb: **never hardcode a separator, never split a path on `'/'`.** Build paths with `Path::join` or collect components, and the right separator is inserted for the target:

```rust
use std::path::{Path, PathBuf};

fn main() {
    let p: PathBuf = ["config", "app", "settings.toml"].iter().collect();
    println!("joined = {}", p.display());
    println!("separator = {:?}", std::path::MAIN_SEPARATOR);

    let f = Path::new("archive.tar.gz");
    println!("extension = {:?}", f.extension()); // last component only
    println!("file_name = {:?}", f.file_name());
}
```

Real output on macOS (on Windows the same code joins with `\` and reports `'\\'`):

```text
joined = config/app/settings.toml
separator = '/'
extension = Some("gz")
file_name = Some("archive.tar.gz")
```

### Exit codes

The exit code is the single integer your process hands back to the shell. By convention `0` means success and non-zero means failure; tools layer meaning on top (`grep` returns `1` for "no match", `2` for a real error). On Unix only the **low 8 bits** are kept (so `256` becomes `0`), which is exactly why typing the code matters.

Rust offers three ways to set it:

1. **Return `ExitCode` from `main`** (preferred). `main() -> ExitCode` runs all destructors first, then exits with `ExitCode::SUCCESS` (0), `ExitCode::FAILURE` (1), or `ExitCode::from(n)` for any `u8`.
2. **Return `Result<(), E>` from `main`.** `Ok` exits `0`; `Err` prints the error's `Debug` representation to stderr and exits with `ExitCode::FAILURE` (1).
3. **Call `std::process::exit(n)`.** This terminates *immediately* and **skips destructors**, the analogue of `process.exit()`. Use it only when you genuinely need to bail out early.

The typed `u8` of `ExitCode::from` is the safety win over JavaScript's `number`: you cannot accidentally pass `300` and have it wrap to a misleading `44`. To map error kinds to conventional codes (here, BSD `sysexits.h` style):

```rust
use std::io;
use std::process::ExitCode;

fn exit_code_for(err: &io::Error) -> ExitCode {
    match err.kind() {
        io::ErrorKind::NotFound => ExitCode::from(66),         // EX_NOINPUT
        io::ErrorKind::PermissionDenied => ExitCode::from(77), // EX_NOPERM
        _ => ExitCode::from(74),                               // EX_IOERR
    }
}

fn main() -> ExitCode {
    let path = "/tmp/definitely-not-here-xyz";
    match std::fs::read_to_string(path) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error reading {path}: {e}");
            exit_code_for(&e)
        }
    }
}
```

Real output (and the shell sees exit code `66`):

```text
error reading /tmp/definitely-not-here-xyz: No such file or directory (os error 2)
```

> **Warning:** The OS error *text* is platform-specific. On Unix a missing file reports `No such file or directory (os error 2)`; on Windows the same operation reports `The system cannot find the file specified. (os error 2)`. Match on `err.kind()`, never on the message string.

---

## Key Differences

| Concern | TypeScript / Node | Rust |
| --- | --- | --- |
| OS detection | `process.platform` (runtime string) | `cfg!(windows)` / `#[cfg(...)]` (compile-time) + `std::env::consts::OS` |
| Native EOL | `os.EOL` (`"\r\n"` on Windows, else `"\n"`) | No single constant; `println!`/`writeln!` always emit `\n`; `lines()` strips `\r` on read |
| Path separator | `path.sep`, `path.delimiter` | `std::path::MAIN_SEPARATOR`; `Path::join` inserts it for you |
| Path type | strings, normalized by `path` module | `Path` / `PathBuf` (own type; not a `String`) |
| Exit code type | `number` (silently wraps mod 256) | `ExitCode` wrapping a typed `u8` |
| Immediate exit | `process.exit(n)` (skips cleanup) | `std::process::exit(n)` (skips destructors) |
| Graceful exit | `process.exitCode = n` | `main() -> ExitCode` (runs destructors) |
| Where bugs surface | at runtime, on the affected OS | many at compile time; the rest via portable types |

The deeper conceptual difference: **Node defers everything to runtime, Rust pushes it to compile time.** A `#[cfg(windows)]` block that references a non-existent Unix API never compiles on Linux, so an entire class of "works on my machine" bugs cannot ship. The trade-off is that you must *think about the target at build time*, and that cross-compiling means the `cfg`s reflect the **target**, not your dev box.

> Unlike TypeScript, there is no `os.EOL`-style "native newline" constant that Rust's output macros consult. Rust's stance is that LF is the portable default and your output should be deterministic regardless of host; convert to CRLF only when a specific consumer demands it.

---

## Common Pitfalls

### Pitfall 1: Splitting a path on a literal `'/'`

```rust
fn main() {
    let raw = "config/app/settings.toml";
    // Works on Unix, WRONG on Windows where '\' separates components.
    let last = raw.rsplit('/').next().unwrap();
    println!("buggy last = {last}");
}
```

This compiles and looks fine on your Mac, then misbehaves on a Windows path like `config\app\settings.toml`. The fix is to let `Path` parse it:

```rust
use std::path::Path;

fn main() {
    let raw = "config/app/settings.toml";
    let last = Path::new(raw).file_name().unwrap();
    println!("correct = {last:?}");
}
```

Real output: `correct = "settings.toml"`. See [Path and PathBuf](/18-cli-tools/07-path-handling/) for the full story.

### Pitfall 2: Printing a `Path` with `{}`

`Path` and `OsStr` deliberately do **not** implement `Display`, because they may contain bytes that are not valid UTF-8 (legal on Unix, and on Windows paths are UTF-16). Writing `println!("{}", some_path)` fails to compile:

```rust
use std::path::Path;

fn main() {
    let p = Path::new("/etc/hosts");
    println!("path is {}", p); // does not compile (error[E0277])
}
```

The real compiler error is explicit about the fix:

```text
error[E0277]: `Path` doesn't implement `std::fmt::Display`
 --> src/main.rs:5:28
  |
5 |     println!("path is {}", p); // does not compile (error[E0277])
  |                       --   ^ `Path` cannot be formatted with the default formatter; call `.display()` on it
  |                       |
  |                       required by this formatting parameter
  |
  = help: the trait `std::fmt::Display` is not implemented for `Path`
  = note: in format strings you may be able to use `{:?}` (or {:#?} for pretty-print) instead
  = note: call `.display()` or `.to_string_lossy()` to safely print paths, as they may contain non-Unicode data
  = note: required for `&Path` to implement `std::fmt::Display`
```

Use `p.display()` for human output, or `p.to_string_lossy()` when you need an owned `String` (it replaces invalid bytes with `�`).

### Pitfall 3: Comparing extensions case-sensitively

On Windows and the default macOS filesystem, `PHOTO.JPG` and `photo.jpg` are the *same* file, and extensions are not case-significant. A naive equality check silently misbehaves:

```rust
use std::path::Path;

fn main() {
    let p = Path::new("PHOTO.JPG");
    // case-sensitive — false even though this IS a jpg
    let naive = p.extension().map(|e| e == "jpg").unwrap_or(false);
    // case-insensitive — correct
    let correct = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("jpg"))
        .unwrap_or(false);
    println!("naive = {naive}, correct = {correct}");
}
```

Real output: `naive = false, correct = true`.

### Pitfall 4: Reaching for `std::process::exit` and losing your cleanup

`std::process::exit(1)` terminates the process *right now*: buffered writers are not flushed and `Drop` impls do not run. If you wrote to a `BufWriter` (see [File I/O with `std::fs`](/18-cli-tools/06-file-io/)) and then called `process::exit`, you can lose the unflushed tail. Prefer returning an `ExitCode` (or a `Result`) from `main` so the stack unwinds normally. The Node mirror is preferring `process.exitCode = n` over `process.exit(n)`.

### Pitfall 5: Matching on OS error *messages*

`err.to_string()` differs across platforms (see the Warning above). Code that does `if msg.contains("No such file")` works on Linux and silently fails on Windows. Match on `err.kind()` instead; `io::ErrorKind` is the portable, stable surface.

---

## Best Practices

- **Default to LF on write.** Emit `\n` everywhere and let Git's `core.autocrlf` / `.gitattributes` handle checkout conversion. Produce CRLF only for a consumer that explicitly requires it.
- **Read with `lines()` / `read_to_string`** and rely on `\r` stripping rather than hand-rolling a regex. For binary-safe line splitting, read bytes and split on `b'\n'`.
- **Never hardcode separators.** Build paths with `Path::join` or `[..].iter().collect::<PathBuf>()`; query `std::path::MAIN_SEPARATOR` only for display.
- **Print paths with `.display()`**, and store them as `PathBuf`, not `String`.
- **Type your exit codes.** Return `ExitCode` from `main`; reserve `std::process::exit` for genuine early aborts and flush buffers first.
- **Adopt an exit-code convention and document it** in `--help`: `0` success, `1` expected failure, `2` usage error is a widely understood baseline (it matches `grep` and many GNU tools).
- **Reach for `cfg!` for inline branches and `#[cfg]` for whole items.** Keep platform-only imports behind `#[cfg]` so the other platform never compiles them.
- **Use the `dirs` crate for standard locations** instead of building `~/.config` paths by hand. It returns the correct per-platform directory at runtime (XDG on Linux, `Library/Application Support` on macOS, `%APPDATA%` on Windows). Add it with `cargo add dirs`:

  ```rust
  fn main() {
      println!("config = {:?}", dirs::config_dir());
      println!("cache  = {:?}", dirs::cache_dir());
      println!("home   = {:?}", dirs::home_dir());
  }
  ```

  Real output on macOS (paths differ per platform and user):

  ```text
  config = Some("/Users/<you>/Library/Application Support")
  cache  = Some("/Users/<you>/Library/Caches")
  home   = Some("/Users/<you>")
  ```

- **Test on all targets in CI.** A matrix build across `ubuntu-latest`, `macos-latest`, and `windows-latest` catches separator and EOL bugs before users do. Distribution and release builds are covered in [Distributing CLI Tools](/18-cli-tools/10-distribution/).

---

## Real-World Example

A small `tree-clean` utility that walks a directory, normalizes every `.txt` file's line endings to LF, refuses to touch files outside the given root, and returns conventional exit codes. It demonstrates portable path handling, line-ending normalization, OS detection for a friendly banner, and typed exit codes working together.

```rust
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Recursively collect `.txt` files under `root`.
fn collect_txt(root: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_txt(&path, out)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("txt"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Normalize `text` to LF-only endings.
fn to_lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn run(root: &Path) -> Result<usize, String> {
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }

    let mut files = Vec::new();
    collect_txt(root, &mut files).map_err(|e| format!("{}: {e}", root.display()))?;

    let mut changed = 0;
    for path in &files {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let lf = to_lf(&text);
        if lf != text {
            fs::write(path, &lf).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("normalized {}", path.display());
            changed += 1;
        }
    }
    Ok(changed)
}

fn main() -> ExitCode {
    // A tiny platform banner — both arms compile on every OS.
    let host = if cfg!(windows) { "Windows" } else { "Unix-like" };
    eprintln!("tree-clean on {host} ({})", std::env::consts::OS);

    let root = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("usage: tree-clean <directory>");
            return ExitCode::from(2); // usage error
        }
    };

    match run(&root) {
        Ok(0) => {
            println!("nothing to do");
            ExitCode::SUCCESS
        }
        Ok(n) => {
            println!("normalized {n} file(s)");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("tree-clean: {msg}");
            ExitCode::FAILURE // exit code 1
        }
    }
}
```

Built and run against a directory containing a CRLF file `notes.txt` and an already-LF `readme.txt`, this prints (real output, captured on macOS):

```text
tree-clean on Unix-like (macos)
normalized /tmp/demo/notes.txt
normalized 1 file(s)
```

Running with no argument exits `2`; pointing it at a non-directory exits `1`. On Windows the banner reads `tree-clean on Windows (windows)` and the same logic applies: the path joining, extension comparison, and exit codes are all portable. This pairs naturally with clap for richer argument parsing ([Parsing Arguments with the clap Derive API](/18-cli-tools/01-clap-derive/)) and indicatif for a progress bar on large trees ([Progress Bars and Spinners with indicatif](/18-cli-tools/04-progress-bars/)).

---

## Further Reading

- [`std::process::ExitCode`](https://doc.rust-lang.org/std/process/struct.ExitCode.html) and [`std::process::exit`](https://doc.rust-lang.org/std/process/fn.exit.html) — the two ways to set an exit status.
- [Conditional compilation](https://doc.rust-lang.org/reference/conditional-compilation.html) (the Rust Reference) — every `cfg` key (`target_os`, `target_family`, `target_arch`, …) and the `cfg!`/`#[cfg]`/`cfg_attr` forms.
- [`std::env::consts`](https://doc.rust-lang.org/std/env/consts/index.html) — `OS`, `FAMILY`, `ARCH`, `EXE_SUFFIX`, and friends.
- [`str::lines`](https://doc.rust-lang.org/std/primitive.str.html#method.lines) — the `\r`-stripping line iterator.
- [`dirs` crate](https://docs.rs/dirs) — portable standard directories.
- Related guide sections: [Path and PathBuf](/18-cli-tools/07-path-handling/) (building paths), [File I/O with `std::fs`](/18-cli-tools/06-file-io/) (reading/writing), [Environment Variables](/18-cli-tools/08-environment-vars/) (config via env), [Distributing CLI Tools](/18-cli-tools/10-distribution/) (shipping per-platform binaries).
- Foundations: [Section 00 — Introduction](/00-introduction/), [Section 01 — Getting Started](/01-getting-started/), [Section 02 — Basics](/02-basics/). When your tool needs to run in the browser, see [Section 19 — WebAssembly](/19-wasm/).

---

## Exercises

### Exercise 1: Report the host platform

**Difficulty:** Beginner

**Objective:** Use `cfg!` and `std::env::consts` to print a platform summary.

**Instructions:** Write a program that prints a single line of the form `os=<os> family=<family> sep=<separator>`, using `std::env::consts::OS`, `std::env::consts::FAMILY`, and `std::path::MAIN_SEPARATOR`. Then add an `if cfg!(windows)` branch that prints `mode=windows` or `mode=unix` on a second line.

<details>
<summary>Solution</summary>

```rust
fn main() {
    println!(
        "os={} family={} sep={:?}",
        std::env::consts::OS,
        std::env::consts::FAMILY,
        std::path::MAIN_SEPARATOR,
    );
    if cfg!(windows) {
        println!("mode=windows");
    } else {
        println!("mode=unix");
    }
}
```

Real output on macOS:

```text
os=macos family=unix sep='/'
mode=unix
```

</details>

### Exercise 2: A line-ending detector with exit codes

**Difficulty:** Intermediate

**Objective:** Read a file and exit with a code that reflects its line-ending style.

**Instructions:** Take a file path as the first argument. Read it with `fs::read_to_string`. Exit `0` if the file contains only LF endings (no `\r\n`), `1` if it contains any CRLF, and `2` for a usage/IO error (no argument, or the file can't be read). Print a human-readable summary to stderr. Return an `ExitCode` from `main`.

<details>
<summary>Solution</summary>

```rust
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: detect <file>");
            return ExitCode::from(2);
        }
    };

    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("detect: {path}: {e}");
            return ExitCode::from(2);
        }
    };

    if text.contains("\r\n") {
        eprintln!("{path}: contains CRLF endings");
        ExitCode::from(1)
    } else {
        eprintln!("{path}: LF only");
        ExitCode::SUCCESS
    }
}
```

Given a file `unix.txt` written with LF only, this prints `unix.txt: LF only` to stderr and exits `0`; given a CRLF file it prints `... contains CRLF endings` and exits `1`. (Verified: a `printf 'a\r\nb\r\n'` file yields exit `1`, a `printf 'a\nb\n'` file yields exit `0`.)

</details>

### Exercise 3: Platform-specific config path with conditional compilation

**Difficulty:** Advanced

**Objective:** Use `#[cfg(...)]` to compile a different `config_path` per OS, and verify the choice with the `dirs` crate.

**Instructions:** Write a `config_path()` function that returns a `PathBuf` to `mytool/config.toml` inside the platform's config directory. Implement it two ways and compare: (a) a hand-rolled version using `#[cfg(windows)]` / `#[cfg(not(windows))]` that joins onto `dirs::config_dir()`, falling back to the current directory if `None`; and (b) confirm both branches type-check by printing the result. Add `dirs` with `cargo add dirs`. Bonus: explain in a comment why `#[cfg]` is preferable to `cfg!` here.

<details>
<summary>Solution</summary>

```rust
use std::path::PathBuf;

// `#[cfg]` is preferable to `cfg!` here because the two branches build
// genuinely different file names; with `#[cfg]` only the matching item is
// compiled, so neither version pays for the other's string at runtime.

#[cfg(windows)]
fn app_subdir() -> &'static str {
    "MyTool" // Windows convention: PascalCase app folder
}

#[cfg(not(windows))]
fn app_subdir() -> &'static str {
    "mytool" // Unix convention: lowercase
}

fn config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(app_subdir()).join("config.toml")
}

fn main() {
    println!("config path: {}", config_path().display());
}
```

Real output on macOS (the config dir and app folder differ per platform):

```text
config path: /Users/<you>/Library/Application Support/mytool/config.toml
```

On Windows the same binary would resolve to something like `C:\Users\<you>\AppData\Roaming\MyTool\config.toml`, and on Linux to `/home/<you>/.config/mytool/config.toml`, all from one source file, because `dirs` resolves the base at runtime and `#[cfg]` selects the app-folder name at build time.

</details>
