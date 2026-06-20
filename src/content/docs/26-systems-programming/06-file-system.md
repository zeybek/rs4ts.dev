---
title: "Advanced File-System Operations"
description: "Go past read_to_string in Rust: file metadata, Unix permission bits, symlinks, directory walking, and mmap, mapped to Node's fs/promises API."
---

In your first weeks with Rust you reach for `std::fs::read_to_string` and `std::fs::write`, and they feel exactly like Node's `fs/promises`, minus the `await`. This file goes one level deeper: inspecting **metadata** (size, timestamps, type), reading and changing **Unix permission bits**, creating and resolving **symbolic links**, **walking directory trees** efficiently, and **memory-mapping** a file so the OS hands you its bytes as a plain slice. These are the building blocks of real tools — backup utilities, linters, asset bundlers, log scanners — and Rust's standard library exposes them with far more precision than the JavaScript `fs` module does.

---

## Quick Overview

Rust's [`std::fs`](https://doc.rust-lang.org/std/fs/) gives you portable, blocking file-system operations, and platform extension traits (in `std::os::unix` / `std::os::windows`) give you the lower-level details that JavaScript's `fs` hides behind loosely-typed objects. For a TypeScript/JavaScript developer the headline is that file metadata in Rust is **strongly typed**: a `Metadata` value tells you at compile time which questions you may ask, the permission model is explicit rather than a magic `mode` number, and `std::fs::metadata` follows symlinks while `std::fs::symlink_metadata` does not, a distinction Node makes with the easy-to-confuse `stat` vs `lstat` pair. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically.

> **Note:** This file covers the deeper file-system surface: metadata, permissions, symlinks, walking, and `mmap`. Basic reading and writing of whole files is introduced earlier in the guide; for spawning external programs see [Process Management with `std::process::Command`](/26-systems-programming/09-process-management/), and for the security implications of path handling and symlink following see [Security](/27-security/).

---

## TypeScript/JavaScript Example

In Node.js you inspect and manipulate the file system through `node:fs/promises`. A typical "scan a project and report on it" helper mixes `stat`, `lstat`, permission changes, symlink reads, and a hand-rolled recursive walk:

```typescript
// Node.js v22 — inspecting and walking a directory tree
import {
  stat,
  lstat,
  readlink,
  chmod,
  readdir,
} from "node:fs/promises";
import { join, extname } from "node:path";

interface FileInfo {
  path: string;
  size: number;
  isSymlink: boolean;
}

// stat() follows symlinks; lstat() reports on the link itself.
async function describe(path: string): Promise<FileInfo> {
  const meta = await stat(path);
  const linkMeta = await lstat(path);
  return {
    path,
    size: meta.size, // bytes; a `number`, so > 2^53 loses precision
    isSymlink: linkMeta.isSymbolicLink(),
  };
}

// Lock a sensitive file down to owner read/write (octal 0o600).
async function lockDown(path: string): Promise<void> {
  await chmod(path, 0o600);
}

// Recursive walk: readdir with `withFileTypes` avoids an extra stat per entry.
async function* walk(dir: string): AsyncGenerator<string> {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules") continue; // prune
      yield* walk(full);
    } else if (entry.isFile()) {
      yield full;
    }
  }
}

async function countByExtension(root: string): Promise<Map<string, number>> {
  const counts = new Map<string, number>();
  for await (const file of walk(root)) {
    const ext = extname(file) || "<none>";
    counts.set(ext, (counts.get(ext) ?? 0) + 1);
  }
  return counts;
}
```

This works, but notice the soft edges: `meta.size` is a JavaScript `number` (an IEEE-754 `f64`), so a file larger than 9 PB would silently lose precision; `entry.isSymbolicLink()` is a method that exists at runtime whether or not it makes sense; and the `mode` you pass to `chmod` is an untyped integer with no guard rails.

---

## Rust Equivalent

Here is the same set of capabilities in idiomatic Rust. Metadata is typed, sizes are exact `u64`, permission bits are reached through an explicit extension trait, and we lean on the [`walkdir`](https://docs.rs/walkdir) crate for an ergonomic, prunable recursive walk:

```rust playground
// cargo add walkdir
use std::collections::HashMap;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt; // brings `mode()` / `set_mode()` into scope
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug)]
struct FileInfo {
    size: u64, // exact, never loses precision
    is_symlink: bool,
}

// `metadata` follows symlinks; `symlink_metadata` reports on the link itself.
fn describe(path: &Path) -> io::Result<FileInfo> {
    let meta = fs::metadata(path)?;
    let link_meta = fs::symlink_metadata(path)?;
    Ok(FileInfo {
        size: meta.len(),
        is_symlink: link_meta.file_type().is_symlink(),
    })
}

// Lock a sensitive file down to owner read/write: rw------- (octal 0o600).
fn lock_down(path: &Path) -> io::Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)
}

// Recursive walk with pruning, counting files per extension.
fn count_by_extension(root: &Path) -> io::Result<HashMap<String, usize>> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| e.file_name() != "node_modules") // prune
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| "<none>".to_string());
        *counts.entry(ext).or_insert(0) += 1;
    }
    Ok(counts)
}

fn main() -> io::Result<()> {
    // Build a tiny tree so the example is self-contained and runnable.
    let root = std::env::temp_dir().join("ts2rust_overview");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src"))?;
    fs::write(root.join("src/main.rs"), b"fn main() {}")?;
    fs::write(root.join("README.md"), b"# demo")?;

    let info = describe(&root.join("README.md"))?;
    println!("README size = {} bytes, symlink = {}", info.size, info.is_symlink);

    lock_down(&root.join("README.md"))?;

    let mut counts: Vec<(String, usize)> =
        count_by_extension(&root)?.into_iter().collect();
    counts.sort();
    println!("by extension = {counts:?}");

    fs::remove_dir_all(&root)?;
    Ok(())
}
```

Running it prints:

```text
README size = 6 bytes, symlink = false
by extension = [("md", 1), ("rs", 1)]
```

---

## Detailed Explanation

### Metadata is a typed query object

`fs::metadata(path)` returns `io::Result<Metadata>`. A [`Metadata`](https://doc.rust-lang.org/std/fs/struct.Metadata.html) value answers a fixed, compile-checked set of questions:

```rust playground
use std::fs;
use std::io;

fn main() -> io::Result<()> {
    let root = std::env::temp_dir().join("ts2rust_fs_demo");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("logs"))?;
    fs::write(root.join("config.toml"), b"port = 8080\n")?;

    let meta = fs::metadata(root.join("config.toml"))?;
    println!("is_file     = {}", meta.is_file());
    println!("is_dir      = {}", meta.is_dir());
    println!("len (bytes) = {}", meta.len());
    println!("readonly    = {}", meta.permissions().readonly());

    // Timestamps are `SystemTime`, not numbers — you convert explicitly.
    let modified = meta.modified()?;
    let secs = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    println!("modified > 0 = {}", secs > 0);

    fs::remove_dir_all(&root)?;
    Ok(())
}
```

Output:

```text
is_file     = true
is_dir      = false
len (bytes) = 12
readonly    = false
modified > 0 = true
```

A few contrasts with `node:fs`:

- **`len()` is a `u64`.** Node's `stats.size` is a `number` (f64). For ordinary files the difference never bites, but Rust's type is correct for files larger than `2^53` bytes, where JavaScript would lose integer precision.
- **Timestamps are `SystemTime`, not milliseconds.** `meta.modified()`, `meta.accessed()`, and `meta.created()` each return `io::Result<SystemTime>`, a `Result` because some platforms or filesystems do not record every timestamp. You convert to a number deliberately via `duration_since(UNIX_EPOCH)`, which makes the "what if the clock went backwards?" case impossible to ignore.
- **`len = 12`** because `b"port = 8080\n"` is twelve bytes (eleven characters plus the newline). Rust counts bytes, exactly as the file holds them.

> **Tip:** If you already have an open `File`, call `file.metadata()` instead of `fs::metadata(path)`. It uses the existing file descriptor (one fewer path lookup) and cannot race with the file being moved between the open and the stat.

### Permissions: an explicit model, not a magic number

Cross-platform code uses `Permissions::readonly()` / `set_readonly()`. On Unix you usually want the real mode bits, which live behind the `std::os::unix::fs::PermissionsExt` trait. **Importing the trait is what makes the `mode()` and `set_mode()` methods appear**, a recurring source of confusion for newcomers:

```rust playground
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::PermissionsExt;

fn main() -> io::Result<()> {
    let dir = std::env::temp_dir().join("ts2rust_perm_demo");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    let path = dir.join("secret.key");
    File::create(&path)?;

    let mode = fs::metadata(&path)?.permissions().mode();
    println!("mode before = {:o}", mode & 0o777);

    // Tighten to owner read/write only: rw-------
    let mut perms = fs::metadata(&path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(&path, perms)?;

    let mode = fs::metadata(&path)?.permissions().mode();
    println!("mode after  = {:o}", mode & 0o777);

    fs::remove_dir_all(&dir)?;
    Ok(())
}
```

Output (your "before" value depends on your `umask`; `644` is typical):

```text
mode before = 644
mode after  = 600
```

The `{:o}` formatter prints octal, the natural base for Unix modes. We mask with `& 0o777` because the full `mode()` value also encodes the file *type* bits in its high bits. This is the same `0o600` you pass to Node's `chmod`, but here the bit-twiddling is visible and the extension trait makes it clear you are stepping onto platform-specific ground.

### Symlinks: `metadata` follows, `symlink_metadata` does not

This is the single most important distinction in this file. `fs::metadata` resolves symbolic links (like `stat(2)` / Node's `stat`); `fs::symlink_metadata` reports on the link object itself (like `lstat(2)` / Node's `lstat`):

```rust playground
use std::fs;
use std::io;
use std::os::unix::fs as unix_fs;

fn main() -> io::Result<()> {
    let dir = std::env::temp_dir().join("ts2rust_link_demo");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;

    let target = dir.join("data.txt");
    fs::write(&target, b"payload")?;
    let link = dir.join("latest.txt");

    // Create the symlink `latest.txt -> data.txt`.
    unix_fs::symlink(&target, &link)?;

    let link_meta = fs::symlink_metadata(&link)?; // lstat: the link
    let target_meta = fs::metadata(&link)?;        // stat: the target
    println!("is_symlink (lstat) = {}", link_meta.file_type().is_symlink());
    println!("is_file    (stat)  = {}", target_meta.is_file());

    // Read where the link points (one hop, not fully resolved).
    let pointed = fs::read_link(&link)?;
    println!("points to = {}", pointed.file_name().unwrap().to_string_lossy());

    // canonicalize() resolves ALL symlinks + `..` to a real absolute path.
    let real = fs::canonicalize(&link)?;
    println!("canonical ends with data.txt = {}", real.ends_with("data.txt"));

    fs::remove_dir_all(&dir)?;
    Ok(())
}
```

Output:

```text
is_symlink (lstat) = true
is_file    (stat)  = true
points to = data.txt
canonical ends with data.txt = true
```

Note that on Unix `symlink` lives in `std::os::unix::fs`; on Windows the equivalents are `symlink_file` / `symlink_dir` in `std::os::windows::fs` (Windows distinguishes the two and typically requires elevated privileges). `read_link` returns the literal target of one link, whereas `canonicalize` walks the entire chain — resolving every intermediate symlink and `..` component — and returns an absolute path, returning an error if any component does not exist.

### Walking a directory tree

For a single level, `fs::read_dir` yields `io::Result<DirEntry>` items. Each `DirEntry` caches its file type, so `entry.file_type()` is usually free of an extra syscall:

```rust playground
use std::fs;
use std::io;

fn main() -> io::Result<()> {
    let root = std::env::temp_dir().join("ts2rust_readdir");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root)?;
    fs::write(root.join("Cargo.toml"), b"")?;
    fs::create_dir(root.join("src"))?;

    let mut names: Vec<String> = fs::read_dir(&root)?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort(); // read_dir order is OS-dependent, so sort for determinism
    println!("top level = {names:?}");

    fs::remove_dir_all(&root)?;
    Ok(())
}
```

Output:

```text
top level = ["Cargo.toml", "src"]
```

`read_dir` does **not** recurse. The standard library deliberately omits a built-in recursive walker, so the ecosystem standard is the [`walkdir`](https://docs.rs/walkdir) crate. It handles depth limits, symlink-loop detection, and, most importantly, `filter_entry`, which prunes a subtree *before descending into it* (so you never pay to walk `target/` or `node_modules/`):

```rust playground
// cargo add walkdir
use std::fs;
use std::io;
use walkdir::WalkDir;

fn main() -> io::Result<()> {
    let root = std::env::temp_dir().join("ts2rust_walk");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src/util"))?;
    fs::write(root.join("src/main.rs"), b"fn main() {}")?;
    fs::write(root.join("src/util/mod.rs"), b"")?;

    // Count only `.rs` files, anywhere in the tree.
    let rs_count = WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .count();
    println!("rust files = {rs_count}");

    fs::remove_dir_all(&root)?;
    Ok(())
}
```

Output:

```text
rust files = 2
```

`is_some_and` (stable since Rust 1.70) is the tidy way to test "the `Option` is `Some` and its value satisfies a predicate", the same intent as JavaScript's `ext?.toLowerCase() === "rs"`, but it short-circuits cleanly on `None`.

### Memory-mapping a file

Memory-mapping asks the OS to make a file's bytes appear directly in your address space; the kernel pages data in on demand instead of you issuing `read` calls. For large, randomly-accessed files this can be dramatically faster and avoids copying the whole file into a heap buffer. JavaScript has no first-class equivalent; you would `fs.read` chunks into a `Buffer`. The ecosystem standard is [`memmap2`](https://docs.rs/memmap2):

```rust
// cargo add memmap2
use std::fs::File;
use std::io;
use memmap2::Mmap;

fn main() -> io::Result<()> {
    let path = std::env::temp_dir().join("ts2rust_mmap.txt");
    std::fs::write(&path, b"alpha\nbeta\ngamma\ndelta\n")?;

    let file = File::open(&path)?;
    // SAFETY: undefined behavior results if another process truncates or
    // resizes the file while it is mapped. We control this temp file, so it is safe.
    let mmap = unsafe { Mmap::map(&file)? };

    // The whole file is now a read-only &[u8] — no explicit read() call.
    println!("mapped bytes = {}", mmap.len());
    let lines = mmap.split(|&b| b == b'\n').filter(|l| !l.is_empty()).count();
    println!("line count   = {lines}");
    println!("first 5      = {:?}", std::str::from_utf8(&mmap[..5]).unwrap());

    std::fs::remove_file(&path)?;
    Ok(())
}
```

Output:

```text
mapped bytes = 23
line count   = 4
first 5      = "alpha"
```

`Mmap::map` is `unsafe` for an honest reason: a memory map is only sound as long as the underlying file is not resized out from under you by another process, which would turn your `&[u8]` into a window onto invalid memory. Mark the `unsafe` block with a `// SAFETY:` comment explaining why you can rule that out, a convention covered in [Security](/27-security/).

---

## Key Differences

| Concern | TypeScript / Node.js | Rust |
| --- | --- | --- |
| Metadata shape | `Stats` object; all fields always present | `Metadata` struct; timestamps are `Result` (may be unsupported) |
| File size type | `number` (f64, lossy past 2^53) | `u64` (exact) |
| Follow symlink? | `stat` follows, `lstat` does not | `fs::metadata` follows, `fs::symlink_metadata` does not |
| Permissions | untyped `mode` integer | `Permissions` + `PermissionsExt::mode()` (trait must be imported) |
| Timestamps | milliseconds since epoch (`number`) | `SystemTime`; convert via `duration_since` |
| Recursive walk | hand-rolled with `readdir` (or a dependency) | `walkdir` crate; `filter_entry` prunes subtrees |
| Memory-mapping | none (read into a `Buffer`) | `memmap2` crate; `unsafe` map into `&[u8]` |
| Errors | thrown `Error` / rejected promise | `io::Result<T>`; propagate with `?` |
| Async by default | yes (`fs/promises`) | no; `std::fs` is blocking (use `tokio::fs` for async) |

The deepest conceptual difference: **`std::fs` is blocking and synchronous.** In Node every `fs/promises` call yields to the event loop. Rust's `std::fs` does not; it parks the calling OS thread until the syscall returns. That is exactly right for CLI tools and batch jobs, but inside an `async` server you must use `tokio::fs` (which offloads to a blocking thread pool) or you will stall the runtime's worker threads. Rust makes this an explicit choice rather than a hidden default.

---

## Common Pitfalls

### Forgetting to import `PermissionsExt`

`mode()` and `set_mode()` are not inherent methods on `Permissions`; they come from the `std::os::unix::fs::PermissionsExt` trait. Forget the `use` and the compiler refuses:

```rust
use std::fs;

fn main() {
    let meta = fs::metadata("Cargo.toml").unwrap();
    let _mode = meta.permissions().mode(); // does not compile (error[E0599])
    println!("ok");
}
```

The real error from `cargo build`:

```text
error[E0599]: no method named `mode` found for struct `Permissions` in the current scope
   --> src/main.rs:5:36
    |
  5 |     let _mode = meta.permissions().mode();
    |                                    ^^^^
    |
   ::: .../library/std/src/os/unix/fs.rs:355:8
    |
355 |     fn mode(&self) -> u32;
    |        ---- the method is available for `Permissions` here
    |
    = help: items from traits can only be used if the trait is in scope
```

The fix is the line `use std::os::unix::fs::PermissionsExt;`. This "the method exists but the trait is not in scope" message is one you will see often in Rust; the cure is always to import the trait the method comes from.

### Confusing `metadata` (follow) with `symlink_metadata` (no follow)

If you use `fs::metadata` to classify directory entries while walking, a symlink that points at a directory will report `is_dir() == true`, and a broken symlink (target deleted) will make `fs::metadata` return an `Err` even though the link clearly exists. To classify the *entry itself*, use `symlink_metadata` (or `entry.file_type()` from `read_dir`/`walkdir`, which is `lstat`-based and never follows). Following symlinks blindly is also how programs get tricked into reading or writing outside an intended directory. See [Security](/27-security/).

### Assuming `read_dir` returns sorted entries

`fs::read_dir` yields entries in **OS-dependent order**, typically the raw directory order, which is neither alphabetical nor stable across runs or filesystems. If your output or your tests depend on order, collect into a `Vec` and `sort()` it yourself. This trips up developers who expect Node's behavior, but Node's `readdir` is *also* unsorted unless you pass extra options; the assumption is wrong in both languages.

### Treating `mmap` as safe

`Mmap::map` is `unsafe` because the borrow checker cannot prove the file will stay the same size for the lifetime of the map. If another process truncates the file, accessing the mapped slice is undefined behavior (typically a `SIGBUS` crash). Only memory-map files you control, and keep the `// SAFETY:` comment honest about that assumption.

### Reaching for `std::fs` inside async code

Calling `std::fs::read_to_string` from inside a `tokio` task blocks the runtime worker thread for the entire duration of the syscall, throttling everything else scheduled on that thread. In async contexts use `tokio::fs` (or `spawn_blocking`). The compiler will not warn you — this is a logic bug, not a type error.

---

## Best Practices

- **Prefer `Path`/`PathBuf` over `String` for paths.** They handle separators portably and offer `extension()`, `file_name()`, `join()`, and `ends_with()` without string surgery. Accept `impl AsRef<Path>` in public functions so callers can pass `&str`, `String`, or `PathBuf`.
- **Use `entry.file_type()` from `read_dir`/`walkdir` to classify entries**, not a fresh `fs::metadata` call. It is cached, avoids a syscall, and — being `lstat`-based — does not silently follow symlinks.
- **Reach for `walkdir` for recursion** and use `filter_entry` to prune unwanted subtrees (`target/`, `.git/`, `node_modules/`) *before* descending, rather than walking them and discarding the results.
- **Gate platform-specific code** behind `#[cfg(unix)]` / `#[cfg(windows)]` when you need raw mode bits or `symlink`, so the crate still compiles everywhere.
- **Propagate errors with `?` and `io::Result`.** A function that touches the file system should almost always return `io::Result<T>` rather than `unwrap`-ing, so the caller decides how to handle a missing file or permission denial. See [why Rust](/01-getting-started/00-why-rust/) for the philosophy behind this.
- **Annotate every `unsafe` block** (such as `Mmap::map`) with a `// SAFETY:` comment stating the invariant you rely on.
- **Use `tokio::fs` in async code**, never `std::fs`, to avoid blocking runtime workers.

---

## Real-World Example

A common tool to build is a **disk-usage analyzer**: walk a project, skip the build directory, and report total on-disk bytes per file extension. It exercises `walkdir`, `filter_entry` pruning, cached `file_type()`, and per-entry `metadata().len()` — and demonstrates the exact `u64` arithmetic that JavaScript's `number` cannot guarantee:

```rust playground
// cargo add walkdir
use std::collections::HashMap;
use std::fs;
use std::io;
use walkdir::WalkDir;

/// Sum on-disk size per file extension under `root`, skipping `target/`.
fn disk_usage_by_ext(root: &str) -> io::Result<HashMap<String, u64>> {
    let mut totals: HashMap<String, u64> = HashMap::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| e.file_name() != "target") // prune before descending
        .filter_map(Result::ok)
    {
        // file_type() comes from the cached dir entry: no extra syscall.
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| "<none>".to_string());

        // len() is the exact file size as a u64 — no precision loss.
        let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
        *totals.entry(ext).or_insert(0) += len;
    }
    Ok(totals)
}

fn main() -> io::Result<()> {
    // Build a representative tree, including a `target/` we expect to skip.
    let root = std::env::temp_dir().join("ts2rust_usage");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("target"))?;
    fs::write(root.join("src/main.rs"), vec![b'x'; 100])?;
    fs::write(root.join("src/lib.rs"), vec![b'y'; 50])?;
    fs::write(root.join("README.md"), vec![b'#'; 30])?;
    fs::write(root.join("target/junk.rs"), vec![b'z'; 9999])?; // must be ignored

    let mut usage: Vec<(String, u64)> =
        disk_usage_by_ext(root.to_str().unwrap())?.into_iter().collect();
    usage.sort();
    for (ext, bytes) in &usage {
        println!("{ext:<7} {bytes:>5} bytes");
    }

    fs::remove_dir_all(&root)?;
    Ok(())
}
```

Output (note `target/junk.rs`'s 9999 bytes are correctly excluded):

```text
md         30 bytes
rs        150 bytes
```

The `rs` total is `100 + 50 = 150` — the two source files only — because `filter_entry` pruned `target/` before `walkdir` ever stepped inside it. In a production tool you would add this to a [`clap`](https://docs.rs/clap)-driven CLI (see Section 18) and format the totals into human-readable units.

---

## Further Reading

- [`std::fs` module](https://doc.rust-lang.org/std/fs/) — the full file-system API
- [`std::fs::Metadata`](https://doc.rust-lang.org/std/fs/struct.Metadata.html) and [`Permissions`](https://doc.rust-lang.org/std/fs/struct.Permissions.html)
- [`std::os::unix::fs::PermissionsExt`](https://doc.rust-lang.org/std/os/unix/fs/trait.PermissionsExt.html) — Unix mode bits
- [`walkdir` crate docs](https://docs.rs/walkdir) and [`memmap2` crate docs](https://docs.rs/memmap2)
- [The Rust Programming Language — File I/O](https://doc.rust-lang.org/book/) (official book)
- Related guide sections: [Process Management with `std::process::Command`](/26-systems-programming/09-process-management/) (running external programs), [Low-Level Networking](/26-systems-programming/07-networking/) (the other half of low-level I/O), [Native Threads with `std::thread`](/26-systems-programming/00-threads/) (parallelizing a directory scan), and [Security](/27-security/) (path traversal and symlink safety)
- Foundations: [why Rust](/01-getting-started/00-why-rust/), [types](/02-basics/01-types/), and the [introduction](/00-introduction/)

---

## Exercises

### Exercise 1: Report file metadata

**Difficulty:** Easy

**Objective:** Practice reading `Metadata` and converting a `SystemTime`.

**Instructions:** Write `fn report(path: &Path) -> io::Result<()>` that prints whether the path is a file or directory, its size in bytes, and its modified time as whole seconds since the Unix epoch. Test it against a file you create in a temp directory.

<details>
<summary>Solution</summary>

```rust playground
use std::fs;
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

fn report(path: &Path) -> io::Result<()> {
    let meta = fs::metadata(path)?;
    let kind = if meta.is_dir() { "directory" } else { "file" };
    let secs = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    println!("{kind}, {} bytes, modified at {secs}s", meta.len());
    Ok(())
}

fn main() -> io::Result<()> {
    let path = std::env::temp_dir().join("ts2rust_ex1.txt");
    fs::write(&path, b"hello world")?; // 11 bytes
    report(&path)?;
    fs::remove_file(&path)?;
    Ok(())
}
```

Output (the timestamp varies by run):

```text
file, 11 bytes, modified at 1796000000s
```

`modified()` returns a `Result` because not every platform records the time; `duration_since(UNIX_EPOCH)` also returns a `Result` in case the clock is set before 1970, which we handle with `unwrap_or(0)`.

</details>

### Exercise 2: Detect executable files

**Difficulty:** Medium

**Objective:** Read Unix mode bits through the `PermissionsExt` trait.

**Instructions:** Write `fn is_executable(path: &Path) -> io::Result<bool>` that returns `true` if **any** execute bit (owner, group, or other) is set. Create one file with mode `0o755` and one with `0o644` and confirm the function distinguishes them. Remember which trait must be in scope.

<details>
<summary>Solution</summary>

```rust playground
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn is_executable(path: &Path) -> io::Result<bool> {
    let mode = fs::metadata(path)?.permissions().mode();
    // 0o111 = the three execute bits (--x--x--x).
    Ok(mode & 0o111 != 0)
}

fn main() -> io::Result<()> {
    let dir = std::env::temp_dir().join("ts2rust_ex2");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;

    let script = dir.join("run.sh");
    fs::write(&script, b"#!/bin/sh\necho hi\n")?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755))?;

    let data = dir.join("data.bin");
    fs::write(&data, b"\x00\x01")?;
    fs::set_permissions(&data, fs::Permissions::from_mode(0o644))?;

    println!("run.sh   executable = {}", is_executable(&script)?);
    println!("data.bin executable = {}", is_executable(&data)?);

    fs::remove_dir_all(&dir)?;
    Ok(())
}
```

Output:

```text
run.sh   executable = true
data.bin executable = false
```

`from_mode` is a constructor on `Permissions` provided by the same `PermissionsExt` trait, so the single `use` import gives you both reading (`mode()`) and constructing (`from_mode`) permission values.

</details>

### Exercise 3: Recursive directory copy

**Difficulty:** Hard

**Objective:** Combine `read_dir`, `file_type()`, recursion, and `fs::copy`.

**Instructions:** Write `fn copy_tree(src: &Path, dst: &Path) -> io::Result<()>` that recreates the directory tree rooted at `src` under `dst`, copying every regular file. Use the cached `entry.file_type()` to decide whether to recurse or copy. Verify that a nested file lands at the right place in the destination.

<details>
<summary>Solution</summary>

```rust playground
use std::fs;
use std::io;
use std::path::Path;

fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        // file_type() is cached on the DirEntry: no extra syscall, no symlink follow.
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let base = std::env::temp_dir().join("ts2rust_ex3");
    let _ = fs::remove_dir_all(&base);

    let src = base.join("src");
    fs::create_dir_all(src.join("nested"))?;
    fs::write(src.join("top.txt"), b"top")?;
    fs::write(src.join("nested/data.bin"), b"\x00\x01")?;

    let dst = base.join("backup");
    copy_tree(&src, &dst)?;

    println!("top copied    = {}", dst.join("top.txt").exists());
    println!("nested copied = {}", dst.join("nested/data.bin").exists());

    fs::remove_dir_all(&base)?;
    Ok(())
}
```

Output:

```text
top copied    = true
nested copied = true
```

Using `entry.file_type()?` rather than `fs::metadata(&from)?.is_dir()` avoids both an extra `stat` syscall and the risk of following a symlinked directory into an unbounded recursion. For production use, `walkdir` plus `fs::copy` (or the [`fs_extra`](https://docs.rs/fs_extra) crate) handles the edge cases — symlink loops, permissions, progress — that a teaching example glosses over.

</details>
