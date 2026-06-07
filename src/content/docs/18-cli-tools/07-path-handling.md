---
title: "Path and PathBuf: Cross-Platform Path Handling"
description: "Build and split file paths in Rust with Path and PathBuf, mapping node:path onto typed accessors that return Option and where joining an absolute segment"
---

## Quick Overview

Every command-line tool eventually has to build, split, and inspect file paths. In Node you reach for the `node:path` module and pass strings around; Rust has dedicated **`Path`** and **`PathBuf`** types in the standard library that make path manipulation explicit, cross-platform, and hard to get wrong. This page maps the `node:path` API you already know onto Rust's `std::path`, and flags the places where the two genuinely disagree: most notably how joining an absolute segment behaves and why paths are *not* just strings.

The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. Everything here is in the standard library, so no crates are required.

> **Note:** This page is about *manipulating* paths (building them, pulling them apart, comparing them). Actually *reading and writing* files lives in [File I/O](/18-cli-tools/06-file-io/), and the broader portability story (line endings, `cfg!(windows)`, exit codes) is in [Cross-Platform Considerations](/18-cli-tools/09-cross-platform/).

---

## TypeScript/JavaScript Example

Here is a small helper module in Node that mirrors a source file under an output directory and swaps its extension: the kind of logic a static-site generator or asset pipeline runs constantly. It uses the `node:path` module throughout.

```typescript
// paths.ts — run with: npx tsx paths.ts
import path from "node:path";

// Build a path from segments.
console.log(path.join("/var/log", "app", "today.log")); // /var/log/app/today.log

// Pull a path apart.
const file = "/home/ada/notes/todo.txt";
console.log(path.basename(file));          // todo.txt
console.log(path.basename(file, ".txt"));  // todo
console.log(path.extname(file));           // .txt
console.log(path.dirname(file));           // /home/ada/notes

// path.parse gives you everything at once.
console.log(path.parse(file));
// { root: '/', dir: '/home/ada/notes', base: 'todo.txt', ext: '.txt', name: 'todo' }

// Mirror a source file under an output dir, swapping the extension.
function outputPath(srcRoot: string, outRoot: string, srcFile: string): string {
  const rel = path.relative(srcRoot, srcFile);   // strip the source root
  const mirrored = path.join(outRoot, rel);       // re-root under output
  const { dir, name } = path.parse(mirrored);
  return path.join(dir, `${name}.html`);          // swap extension
}

console.log(outputPath("content", "public", "content/blog/hello.md"));
// public/blog/hello.html
```

```text
$ npx tsx paths.ts
/var/log/app/today.log
todo.txt
todo
.txt
/home/ada/notes
{
  root: '/',
  dir: '/home/ada/notes',
  base: 'todo.txt',
  ext: '.txt',
  name: 'todo'
}
public/blog/hello.html
```

Everything here is a **string**. `path.join` is just clever string concatenation that normalizes separators, and there is nothing stopping you from accidentally treating a path as a generic string, slicing it by hand, or comparing it with `===` and getting tripped up by a trailing slash.

---

## Rust Equivalent

Rust models a path as its own type. A borrowed, unsized view is **`Path`** (the `&str` of the path world); an owned, growable buffer is **`PathBuf`** (the `String` of the path world). The pairing is exactly the `str`/`String` relationship from [the basics](/02-basics/).

```rust
// src/main.rs
use std::path::{Path, PathBuf};

fn main() {
    // Build a path from segments — push mutates a PathBuf in place.
    let mut path = PathBuf::from("/var/log");
    path.push("app");
    path.push("today.log");
    println!("{}", path.display()); // /var/log/app/today.log

    // Or chain `join`, which returns a fresh PathBuf each time.
    let p = Path::new("/var/log").join("app").join("today.log");
    println!("{}", p.display()); // /var/log/app/today.log

    // Pull a path apart — every accessor returns an Option.
    let file = Path::new("/home/ada/notes/todo.txt");
    println!("{:?}", file.file_name()); // Some("todo.txt")
    println!("{:?}", file.file_stem()); // Some("todo")
    println!("{:?}", file.extension()); // Some("txt")  — note: no leading dot
    println!("{:?}", file.parent());    // Some("/home/ada/notes")
}
```

```text
$ cargo run --quiet
/var/log/app/today.log
/var/log/app/today.log
Some("todo.txt")
Some("todo")
Some("txt")
Some("/home/ada/notes")
```

And here is the output-path helper, the direct equivalent of the TypeScript `outputPath` function:

```rust
// src/main.rs
use std::path::{Path, PathBuf};

/// Mirror `file` (which lives under `src_root`) into `out_root`,
/// swapping its extension. Returns `None` if `file` is not under `src_root`.
fn output_path(src_root: &Path, out_root: &Path, file: &Path, new_ext: &str) -> Option<PathBuf> {
    let rel = file.strip_prefix(src_root).ok()?; // strip the source root
    let mut dest = out_root.join(rel);           // re-root under output
    dest.set_extension(new_ext);                 // swap extension in place
    Some(dest)
}

fn main() {
    let dest = output_path(
        Path::new("content"),
        Path::new("public"),
        Path::new("content/blog/hello.md"),
        "html",
    );
    println!("{:?}", dest.map(|p| p.display().to_string()));
    // Some("public/blog/hello.html")
}
```

```text
$ cargo run --quiet
Some("public/blog/hello.html")
```

---

## Detailed Explanation

### `Path` vs `PathBuf` (and why there are two)

The split mirrors `str` vs `String` exactly:

- **`Path`** is a *borrowed*, unsized slice type. You almost always handle it behind a reference, `&Path`. `Path::new("…")` is a zero-cost view over an existing string; it allocates nothing.
- **`PathBuf`** is the *owned*, heap-allocated, growable version. `push`, `set_extension`, and `set_file_name` all mutate it in place; `from`, `join`, and `with_extension` create new ones.

A `&PathBuf` automatically coerces to `&Path` (via `Deref`), so you write functions that take `&Path` and call them with either type. That is the same pattern as accepting `&str` so you can pass both `&String` and string literals.

### Joining is not always concatenation

`PathBuf::push` and `Path::join` add a component, inserting the platform separator for you. But there is one rule that surprises every newcomer: **pushing an absolute path replaces the whole buffer.**

```rust
// src/main.rs
use std::path::Path;

fn main() {
    let weird = Path::new("/etc").join("/usr/local");
    println!("{}", weird.display()); // /usr/local — NOT /etc/usr/local
}
```

```text
$ cargo run --quiet
/usr/local
```

Node's `path.join("/etc", "/usr/local")` instead produces `/etc/usr/local` (it strips the leading slash and concatenates), whereas `path.resolve("/etc", "/usr/local")` produces `/usr/local`. Rust's `join` behaves like Node's `resolve` on this point, not like `join`. This is a deliberate safety property: if a later segment is absolute, it is taken to mean "start over from here," which matters when one of the segments comes from user input or an environment variable.

### Accessors return `Option`, and extensions have no dot

`file_name`, `file_stem`, `extension`, and `parent` all return `Option` because not every path has them: `/` has no file name, and `Makefile` has no extension. This is Rust pushing the "what if it's missing?" question into the type system instead of letting you discover it via `undefined` at runtime.

Two details to internalize:

- **`extension()` does not include the leading dot.** Rust gives you `"txt"`; Node's `path.extname` gives you `".txt"`. Adjust your comparisons accordingly.
- These accessors return `&OsStr`, not `&str`. See the next section.

```rust
// src/main.rs
use std::path::Path;

fn main() {
    // Multi-dot files: only the LAST segment is the extension.
    let archive = Path::new("backup.tar.gz");
    println!("{:?}", archive.file_stem()); // Some("backup.tar")
    println!("{:?}", archive.extension()); // Some("gz")

    // A leading-dot file is treated as a name, not an extension.
    let dot = Path::new(".gitignore");
    println!("{:?}", dot.file_name());     // Some(".gitignore")
    println!("{:?}", dot.file_stem());     // Some(".gitignore")
    println!("{:?}", dot.extension());     // None
}
```

```text
$ cargo run --quiet
Some("backup.tar")
Some("gz")
Some(".gitignore")
Some(".gitignore")
None
```

Node agrees on these: `path.extname("backup.tar.gz")` is `".gz"` and `path.extname(".gitignore")` is `""` (empty). Good news: the semantics line up here.

### `OsStr`, `to_str`, and `display`

A path on disk is not guaranteed to be valid UTF-8. On Unix a filename is an arbitrary sequence of bytes; on Windows it is UTF-16 that may contain unpaired surrogates. To model this honestly, path components are `OsStr`/`OsString` (the OS-native string type), not `str`/`String`. JavaScript pretends this problem does not exist and hands you a (possibly lossy) UTF-16 string; Rust makes the lossiness explicit:

- **`path.to_str() -> Option<&str>`**: `Some` only if the path is valid UTF-8, `None` otherwise. Use this when you want to *refuse* non-UTF-8 paths.
- **`path.to_string_lossy() -> Cow<str>`**: always succeeds, replacing invalid sequences with the U+FFFD replacement character. Use this for display when you would rather show *something* than fail.
- **`path.display()`** returns a helper whose `Display` impl is lossy in the same way; use it inside `println!("{}", path.display())`. A `Path` does *not* implement `Display` directly, precisely so you cannot accidentally print one without acknowledging the lossiness.

```rust
// src/main.rs
use std::path::Path;

fn main() {
    let p = Path::new("/srv/www/index.html");
    match p.to_str() {
        Some(s) => println!("utf-8: {s}"),
        None => println!("path is not valid UTF-8"),
    }
    println!("{}", p.to_string_lossy()); // /srv/www/index.html
}
```

```text
$ cargo run --quiet
utf-8: /srv/www/index.html
/srv/www/index.html
```

### Inspecting and comparing paths by component

`Path` offers structural queries that operate on whole *components*, not raw substrings:

```rust
// src/main.rs
use std::path::Path;

fn main() {
    let abs = Path::new("/srv/www/app/index.html");

    println!("{}", abs.is_absolute());            // true
    println!("{}", abs.starts_with("/srv/www"));  // true (component-wise)

    match abs.strip_prefix("/srv/www") {
        Ok(rel) => println!("{}", rel.display()), // app/index.html
        Err(_) => println!("not under that prefix"),
    }

    // ends_with matches trailing COMPONENTS, not a substring.
    println!("{}", abs.ends_with("index.html"));      // true
    println!("{}", abs.ends_with(".html"));           // false!
    println!("{}", abs.ends_with("app/index.html"));  // true

    // Walk the path one component at a time.
    print!("components:");
    for comp in Path::new("/usr/local/bin").components() {
        print!(" {comp:?}");
    }
    println!();
}
```

```text
$ cargo run --quiet
true
true
app/index.html
false
true
components: RootDir Normal("usr") Normal("local") Normal("bin")
```

The `ends_with(".html") == false` result is the headline surprise for a JavaScript developer: this is **not** `String.prototype.endsWith`. `Path::ends_with` asks "does this path end with these whole components?", so `.html` (which is not a complete final component) does not match. To test a file extension, use `extension()`, not `ends_with`.

---

## Key Differences

| Concern | Node `path` (strings) | Rust `std::path` |
| --- | --- | --- |
| Core type | plain `string` | `&Path` (borrowed) / `PathBuf` (owned) |
| Join a segment | `path.join(a, b)` | `a.join(b)` / `buf.push(b)` |
| Join with absolute segment | `join` concatenates; `resolve` restarts | `join`/`push` **restart** (like `resolve`) |
| File name | `path.basename(p)` → `string` | `p.file_name()` → `Option<&OsStr>` |
| Name without extension | `path.basename(p, ext)` / `parse().name` | `p.file_stem()` → `Option<&OsStr>` |
| Extension | `path.extname(p)` → `".txt"` (with dot) | `p.extension()` → `Some("txt")` (no dot) |
| Directory | `path.dirname(p)` | `p.parent()` → `Option<&Path>` |
| Everything at once | `path.parse(p)` | combine `file_stem` + `extension` + `parent` |
| Make relative | `path.relative(from, to)` | `p.strip_prefix(base)` → `Result` |
| Separator | `path.sep` | `std::path::MAIN_SEPARATOR{,_STR}` |
| Force a platform | `path.win32` / `path.posix` | always native; no in-API override |
| Missing piece | `undefined` / `""` at runtime | `None` / `Err` in the type |
| Non-UTF-8 path | silently lossy | explicit `OsStr` + `to_str`/`to_string_lossy` |

### Cross-platform behavior

`std::path` always targets the platform you compile for. On Unix the separator is `/`; on Windows the API accepts both `/` and `\` as separators and also understands drive prefixes like `C:` and UNC paths (`\\server\share`). The `components()` iterator normalizes all of this into typed `Component` values (`RootDir`, `Prefix`, `Normal`, `ParentDir`, `CurDir`), so your matching logic is portable without `if (process.platform === "win32")` branches.

```rust
// src/main.rs
use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR};

fn main() {
    // On Unix this prints '/'; on Windows it would print '\\'.
    println!("{MAIN_SEPARATOR:?}");
    println!("{MAIN_SEPARATOR_STR:?}");
}
```

```text
$ cargo run --quiet
'/'
"/"
```

Unlike Node, the Rust standard library does **not** expose `win32`/`posix` sub-modules to manipulate foreign-platform paths from the current platform. If you genuinely need to parse Windows paths on Unix (rare; usually a sign you should store data differently), reach for a crate such as `typed-path`. For normal CLI tools, just use `std::path` and let it pick the right behavior per target.

---

## Common Pitfalls

### Trying to build paths with string concatenation

A JavaScript habit is to glue paths together with `+`. `Path`/`PathBuf` do not implement `Add`, so this fails at compile time:

```rust
// src/main.rs
use std::path::Path;

fn main() {
    let dir = Path::new("/var/log");
    let logfile = dir + "/app.log"; // does not compile (error[E0369])
    println!("{}", logfile.display());
}
```

```text
$ cargo build
error[E0369]: cannot add `&str` to `&Path`
 --> src/main.rs:6:23
  |
6 |     let logfile = dir + "/app.log";
  |                   --- ^ ---------- &str
  |                   |
  |                   &Path
```

The fix is `dir.join("app.log")`. Note you pass `"app.log"`, not `"/app.log"`. A leading slash would make the segment absolute and (per the rule above) discard `/var/log` entirely.

### Comparing an extension against a `&str` with the leading dot

`extension()` yields `Option<&OsStr>`, so comparing it directly with `Some("png")` (a `&str`) is a type error, and even when you fix the type you must remember there is **no leading dot**:

```rust
// src/main.rs
use std::path::Path;

fn main() {
    let p = Path::new("photo.png");
    if p.extension() == Some("png") {   // does not compile (error[E0308])
        println!("it's a png");
    }
}
```

```text
$ cargo build
error[E0308]: mismatched types
 --> src/main.rs:6:30
  |
6 |     if p.extension() == Some("png") {   // does not compile (error[E0308])
  |                         ---- ^^^^^ expected `&OsStr`, found `&str`
  |                         |
  |                         arguments to this enum variant are incorrect
  |
  = note: expected reference `&OsStr`
             found reference `&'static str`
```

The idiomatic fix — which also handles case-insensitivity, something `=== ".png"` in JavaScript silently gets wrong — looks like this:

```rust
// src/main.rs
use std::path::Path;

fn is_png(path: &Path) -> bool {
    path.extension()
        .map(|e| e.eq_ignore_ascii_case("png"))
        .unwrap_or(false)
}

fn main() {
    println!("{}", is_png(Path::new("photo.PNG"))); // true
    println!("{}", is_png(Path::new("notes.md")));  // false
    println!("{}", is_png(Path::new("Makefile")));  // false
}
```

```text
$ cargo run --quiet
true
false
false
```

If you only need an exact, case-sensitive match, `path.extension() == Some(std::ffi::OsStr::new("png"))` also works.

### Assuming `set_extension` appends like a string

`set_extension("html")` *replaces* the existing extension; it does not append. `report.md` becomes `report.html`, not `report.md.html`. To add a second extension on purpose (e.g. `app.log` → `app.log.gz`), build the new file name explicitly rather than calling `set_extension`. Likewise, `with_extension` returns a *new* `PathBuf` and leaves the original untouched; handy when the source path is borrowed and you cannot mutate it.

### Reaching for `canonicalize` when you just want to normalize

`std::fs::canonicalize` resolves `.`, `..`, and symlinks into a real absolute path, but it **touches the filesystem** and errors if the path does not exist. It is not a pure string operation like Node's `path.normalize`. For purely lexical normalization without hitting disk, iterate `components()` yourself or use the `path-clean` crate. (On Windows, `canonicalize` also returns a verbatim `\\?\` prefix that surprises many programs; the `dunce` crate exists specifically to strip it.)

---

## Best Practices

- **Accept `impl AsRef<Path>` in your function signatures.** This lets callers pass a `&str`, `String`, `&Path`, or `PathBuf` interchangeably: the path analogue of taking `&str`. Convert once at the top with `.as_ref()`.

  ```rust
  // src/main.rs
  use std::path::{Path, PathBuf};

  fn log_path(base: impl AsRef<Path>, name: &str) -> PathBuf {
      base.as_ref().join(name).with_extension("log")
  }

  fn main() {
      println!("{}", log_path("/var/log", "app").display());        // &str
      println!("{}", log_path(String::from("/tmp"), "session").display()); // String
      println!("{}", log_path(Path::new("logs"), "errors").display());     // &Path
  }
  ```

  ```text
  $ cargo run --quiet
  /var/log/app.log
  /tmp/session.log
  logs/errors.log
  ```

- **Store `PathBuf`, pass `&Path`.** Keep owned paths in your structs and config; take `&Path` (or `impl AsRef<Path>`) in functions. Cloning a `PathBuf` allocates, so borrow when you can; see [Ownership](/05-ownership/).
- **Never hardcode `/` or `\`.** Use `join`/`push` so the right separator is chosen per platform. If you must show a literal separator in a message, use `MAIN_SEPARATOR_STR`.
- **Decide your non-UTF-8 policy explicitly.** Use `to_str()` (returns `Option`) when a non-UTF-8 path is an error you want to surface, and `to_string_lossy()` only for human-facing display.
- **Use `display()` for printing, never `to_str().unwrap()`.** The latter panics on the (admittedly rare) non-UTF-8 path; `display()` degrades gracefully.
- **Validate untrusted path input before joining.** Reject `..` components and absolute paths so a user-supplied "filename" cannot escape your working directory (see the directory-traversal exercise below).

---

## Real-World Example

A common CLI chore: given a source tree and an output directory, compute where each file should land after a conversion step, mirroring the directory structure and swapping the extension. This is the core of a static-site generator, an asset transpiler, or a backup tool. It exercises `strip_prefix`, `join`, `set_extension`, and `Option`/`Result` handling together.

```rust
// src/main.rs
use std::path::{Path, PathBuf};

/// Compute the output path for `file` by mirroring its location below
/// `src_root` into `out_root` and swapping the extension to `new_ext`.
/// Returns `None` when `file` does not actually live under `src_root`,
/// so callers can skip or report stray inputs instead of producing nonsense.
fn output_path(
    src_root: &Path,
    out_root: &Path,
    file: &Path,
    new_ext: &str,
) -> Option<PathBuf> {
    let rel = file.strip_prefix(src_root).ok()?;
    let mut dest = out_root.join(rel);
    dest.set_extension(new_ext);
    Some(dest)
}

fn main() {
    let src_root = Path::new("content");
    let out_root = Path::new("public");

    let inputs = [
        "content/index.md",
        "content/blog/hello-world.md",
        "content/blog/nested/deep.md",
        "elsewhere/strange.md", // not under src_root
    ];

    for input in inputs {
        let file = Path::new(input);
        match output_path(src_root, out_root, file, "html") {
            Some(dest) => println!("{input:<35} -> {}", dest.display()),
            None => println!("{input:<35} -> (skipped: outside source root)"),
        }
    }
}
```

```text
$ cargo run --quiet
content/index.md                    -> public/index.html
content/blog/hello-world.md         -> public/blog/hello-world.html
content/blog/nested/deep.md         -> public/blog/nested/deep.html
elsewhere/strange.md                -> (skipped: outside source root)
```

Notice how the "this file isn't under the root I expected" case is a `None` you must handle, not a silently wrong string. In the TypeScript version, `path.relative("content", "elsewhere/strange.md")` returns `"../elsewhere/strange.md"`, which would then be re-rooted into `public/../elsewhere/strange.html`: a bug that escapes your output directory and that nothing forces you to notice.

---

## Further Reading

- [`std::path` module](https://doc.rust-lang.org/std/path/index.html) — overview of `Path`, `PathBuf`, and `Component`.
- [`std::path::Path`](https://doc.rust-lang.org/std/path/struct.Path.html) and [`std::path::PathBuf`](https://doc.rust-lang.org/std/path/struct.PathBuf.html) — the full method lists.
- [`std::ffi::OsStr` / `OsString`](https://doc.rust-lang.org/std/ffi/index.html) — why path components are not `str`.
- [Node.js `path` module](https://nodejs.org/api/path.html) — the API this page maps from.
- Related sections in this guide:
  - [File I/O](/18-cli-tools/06-file-io/) — reading and writing the files these paths point at.
  - [Cross-Platform Considerations](/18-cli-tools/09-cross-platform/) — line endings, `cfg!(windows)`, and exit codes.
  - [Environment Variables](/18-cli-tools/08-environment-vars/) — building paths from `$HOME`, `$XDG_CONFIG_HOME`, etc.
  - [clap Derive API](/18-cli-tools/01-clap-derive/) — parsing a `PathBuf` argument straight from the command line.
  - [Ownership](/05-ownership/) — the borrowing rules behind "store `PathBuf`, pass `&Path`".
  - [The Basics](/02-basics/) — the `str`/`String` pairing that `Path`/`PathBuf` mirrors.
  - When you target the browser, file paths mostly disappear — see [WebAssembly](/19-wasm/).

---

## Exercises

### Exercise 1: Classify files by extension

**Difficulty:** Easy

**Objective:** Write a function `classify(path: &Path) -> &'static str` that returns `"image"`, `"video"`, `"text"`, `"other"`, or `"no extension"` based on a file's extension, matching case-insensitively.

**Instructions:**

1. Pull the extension with `extension()` and convert it to a `&str`.
2. Lowercase it and `match` on the known groups.
3. Handle the no-extension case (e.g. `Makefile`) by returning `"no extension"`.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
use std::path::Path;

fn classify(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => match ext.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" => "image",
            "mp4" | "mov" | "mkv" => "video",
            "md" | "txt" | "rst" => "text",
            _ => "other",
        },
        None => "no extension",
    }
}

fn main() {
    for f in ["a.PNG", "clip.mp4", "README.md", "Makefile", "archive.tar.gz"] {
        println!("{f:<16} -> {}", classify(Path::new(f)));
    }
}
```

```text
$ cargo run --quiet
a.PNG            -> image
clip.mp4         -> video
README.md        -> text
Makefile         -> no extension
archive.tar.gz   -> other
```

`extension()` for `archive.tar.gz` is `"gz"`, which isn't in any group, so it lands in `"other"`: a reminder that Rust only treats the final segment as the extension.

</details>

### Exercise 2: A traversal-safe join

**Difficulty:** Medium

**Objective:** Write `safe_join(base: &Path, user_input: &str) -> Option<PathBuf>` that joins user-supplied input under `base` but refuses anything that could escape `base` (absolute paths, `..`, drive prefixes, or a root segment).

**Instructions:**

1. Reject the input outright if `Path::new(user_input).is_absolute()`.
2. Iterate `components()` and reject if you see a `Component::ParentDir`, `Component::RootDir`, or `Component::Prefix`.
3. Otherwise return `Some(base.join(user_input))`.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
use std::path::{Component, Path, PathBuf};

fn safe_join(base: &Path, user_input: &str) -> Option<PathBuf> {
    let candidate = Path::new(user_input);
    if candidate.is_absolute() {
        return None;
    }
    for comp in candidate.components() {
        if matches!(
            comp,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return None;
        }
    }
    Some(base.join(candidate))
}

fn main() {
    let base = Path::new("/srv/uploads");
    for input in ["avatar.png", "img/cat.jpg", "../../etc/passwd", "/etc/passwd"] {
        match safe_join(base, input) {
            Some(p) => println!("{input:<20} -> {}", p.display()),
            None => println!("{input:<20} -> rejected"),
        }
    }
}
```

```text
$ cargo run --quiet
avatar.png           -> /srv/uploads/avatar.png
img/cat.jpg          -> /srv/uploads/img/cat.jpg
../../etc/passwd     -> rejected
/etc/passwd          -> rejected
```

This is the kind of check a file-serving CLI or upload handler needs. Matching on typed `Component` values is portable: the same code rejects a Windows drive prefix (`Component::Prefix`) without any `cfg!(windows)` branching.

</details>

### Exercise 3: Count files by extension

**Difficulty:** Medium

**Objective:** Given a slice of path strings, build a sorted report of how many files share each (lowercased) extension, with a `(none)` bucket for extensionless files.

**Instructions:**

1. Use a `BTreeMap<String, usize>` so the output comes out sorted by extension.
2. For each path, derive the key from `extension()` (lowercased) or `(none)` when there is no extension.
3. Increment the count with the entry API and print each `ext count` pair.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
use std::collections::BTreeMap;
use std::path::Path;

fn main() {
    let files = [
        "src/main.rs",
        "src/lib.rs",
        "README.md",
        "docs/guide.md",
        "assets/logo.png",
        "Makefile",
    ];

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for f in files {
        let key = Path::new(f)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_else(|| "(none)".to_string());
        *counts.entry(key).or_insert(0) += 1;
    }

    for (ext, n) in &counts {
        println!("{ext:<8} {n}");
    }
}
```

```text
$ cargo run --quiet
(none)   1
md       2
png      1
rs       2
```

The `BTreeMap` keeps the keys ordered, so `(none)` sorts first and the extensions follow alphabetically. Swap in a `HashMap` if you do not care about ordering; see [Collections](/07-collections/) for the trade-offs.

</details>
