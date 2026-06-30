---
title: "Progress Bars and Spinners with indicatif"
description: "Add progress bars, spinners, byte counters, and multi-progress to Rust CLIs with indicatif, the thread-safe answer to Node's cli-progress and ora."
---

Give long-running CLI work a heartbeat: progress bars, spinners, byte counters, and stacked multi-progress displays, all rendered to the terminal with the `indicatif` crate.

---

## Quick Overview

When a command does real work — downloading files, processing thousands of records, compiling — users want feedback. In Node you reach for `cli-progress` or `ora`; in Rust the de-facto choice is **`indicatif`**. It draws single bars, animated spinners, and several stacked bars at once, automatically redrawing the terminal in place. Importantly, it knows when its output is *not* a terminal (a pipe, a CI log) and quietly does nothing, so you never spew thousands of garbled lines into a log file.

> **Note:** This file uses `indicatif` **0.18** with Rust **1.96.0** on the latest stable edition (2024). The current API centers on `ProgressBar`, `ProgressStyle::with_template`, and `MultiProgress`. The pre-0.17 `ProgressStyle::default_bar().template("…")` chaining and the old `{wide_bar}`-only styling are superseded by the `with_template(...).unwrap()` builder shown throughout.

---

## TypeScript/JavaScript Example

A typical Node CLI that downloads several files, using `cli-progress` for bars and `ora` for a spinner:

```typescript
// npm install cli-progress ora
import cliProgress from "cli-progress";
import ora from "ora";

// A spinner for an indeterminate step (we don't know how long DNS/handshake takes)
const spinner = ora("Resolving dependencies...").start();
await new Promise((r) => setTimeout(r, 800));
spinner.succeed("Resolved 42 packages");

// A determinate bar for a download where we know the total size
const bar = new cliProgress.SingleBar(
  {
    format: "Downloading [{bar}] {percentage}% | {value}/{total} bytes",
    hideCursor: true,
  },
  cliProgress.Presets.shades_classic,
);

const total = 5 * 1024 * 1024; // 5 MiB
bar.start(total, 0);
let downloaded = 0;
while (downloaded < total) {
  await new Promise((r) => setTimeout(r, 2));
  downloaded = Math.min(downloaded + 64 * 1024, total);
  bar.update(downloaded);
}
bar.stop();
console.log("Done.");
```

You manually wire up two different libraries, pass a format string with `{bar}`/`{value}`/`{total}` tokens, and call `start`/`update`/`stop`. Updating multiple bars at once requires `cli-progress`'s `MultiBar`.

---

## Rust Equivalent

The same two phases (an indeterminate spinner, then a determinate byte bar) with `indicatif`:

```rust
// Cargo.toml: cargo add indicatif
use indicatif::{ProgressBar, ProgressStyle};
use std::thread;
use std::time::Duration;

fn main() {
    // 1. Spinner for an indeterminate step.
    let spinner = ProgressBar::new_spinner();
    spinner.set_message("Resolving dependencies...");
    spinner.enable_steady_tick(Duration::from_millis(80));
    thread::sleep(Duration::from_millis(800)); // pretend work
    spinner.finish_with_message("Resolved 42 packages");

    // 2. Determinate bar for a 5 MiB download.
    let total_bytes: u64 = 5 * 1024 * 1024;
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::with_template(
            "Downloading [{bar:40.cyan/blue}] {percent}% | {bytes}/{total_bytes}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    let chunk = 64 * 1024;
    let mut downloaded = 0u64;
    while downloaded < total_bytes {
        thread::sleep(Duration::from_millis(2)); // pretend network I/O
        downloaded = (downloaded + chunk).min(total_bytes);
        pb.set_position(downloaded);
    }
    pb.finish_with_message("Done.");
}
```

While running in a terminal, the bar redraws in place. The final frame looks like:

```text
Downloading [########################################] 100% | 5.00 MiB/5.00 MiB
```

One crate covers both the spinner and the bar, the `{bytes}`/`{total_bytes}` tokens format raw byte counts as human-readable units automatically, and (as you'll see below) the same `MultiProgress` type handles many bars at once.

---

## Detailed Explanation

### Creating a bar

`ProgressBar::new(len)` builds a **determinate** bar whose total is `len` (a `u64`). `ProgressBar::new_spinner()` builds an **indeterminate** spinner with no known total. Both return an owned `ProgressBar`.

A key fact for the rest of this file: `ProgressBar` is internally an `Arc` around shared state, so **cloning it is cheap and all clones drive the same on-screen bar**. That is why you can `move` a clone into a thread (shown later) without `Arc::new(Mutex::new(...))` ceremony.

### Advancing the bar

```rust
use indicatif::ProgressBar;

fn main() {
    let pb = ProgressBar::new(1000);
    pb.inc(1);            // advance by 1 (relative)
    pb.inc(10);           // advance by 10 more -> position is 11
    pb.set_position(500); // jump to an absolute position
    pb.set_length(2000);  // the total can change mid-flight
    println!("{}", pb.position()); // read current position: 500
    pb.finish();          // leave the completed bar on screen
}
```

- `inc(delta)` is the workhorse: call it once per processed item.
- `set_position(n)` sets an absolute value — ideal when you track bytes downloaded.
- `set_length(n)` adjusts the total if you discover more work later.
- A spinner has no length; you just `inc(1)` or let `enable_steady_tick` animate it on a timer.

### Finishing

How you end a bar matters because it decides what stays on screen:

| Method                          | Effect                                                              |
| ------------------------------- | ------------------------------------------------------------------- |
| `finish()`                      | Sets position to the length and leaves the full bar visible         |
| `finish_with_message(msg)`      | Same, plus sets the `{msg}` field to `msg`                          |
| `finish_and_clear()`            | Removes the bar entirely (good for transient spinners)              |
| `abandon()` / `abandon_with_message` | Stops redrawing, leaves the bar at its current (partial) position |

A spinner that should vanish once its step succeeds uses `finish_and_clear`:

```rust
use indicatif::ProgressBar;
use std::thread;
use std::time::Duration;

fn main() {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(100));
    pb.set_message("Fetching metadata...");
    thread::sleep(Duration::from_millis(300)); // work
    pb.finish_and_clear();
    println!("Fetched."); // the spinner line is gone; only this prints
}
```

### Styling with templates

`ProgressStyle::with_template(s)` parses a template string and returns a `Result<ProgressStyle, TemplateError>`. You must handle (or `.unwrap()`) it. The template is a sequence of literal text and `{token}` placeholders. Common tokens:

| Token              | Renders                                              |
| ------------------ | ---------------------------------------------------- |
| `{bar}` / `{bar:40}` | The bar itself; the number sets its width in cells |
| `{wide_bar}`       | A bar that expands to fill the remaining terminal width |
| `{spinner}`        | The animated spinner glyph                           |
| `{pos}` / `{len}`  | Raw current position / total                         |
| `{percent}`        | Integer percentage complete                          |
| `{bytes}` / `{total_bytes}` | Position / length formatted as `KiB`, `MiB`, … |
| `{bytes_per_sec}`  | Throughput as `… MiB/s`                              |
| `{per_sec}`        | Items per second                                     |
| `{elapsed_precise}`| Elapsed wall-clock time `HH:MM:SS`                   |
| `{eta}`            | Estimated time remaining                             |
| `{msg}`            | The message set via `set_message` / `finish_with_message` |
| `{prefix}`         | The prefix set via `set_prefix` (handy with multi-progress) |

A token can carry a width, an alignment, and a color spec: `{bar:40.cyan/blue}` means "40 cells wide, filled portion cyan, empty portion blue"; `{prefix:>12.green}` means "right-aligned in 12 columns, green". `.progress_chars("#>-")` chooses the glyphs for the filled cell, the in-progress cell, and the empty cell respectively.

```rust
use indicatif::ProgressStyle;

fn main() {
    let style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
    )
    .unwrap()
    .progress_chars("#>-");
    let _ = style; // attach to a bar with pb.set_style(style)
}
```

### Iterator and byte-stream adapters

Two ergonomic helpers remove most boilerplate.

`ProgressIterator::progress()` wraps any iterator with a known length and updates a bar automatically as you consume it:

```rust
// indicatif's ProgressIterator trait must be in scope
use indicatif::ProgressIterator;
use std::thread;
use std::time::Duration;

fn main() {
    let files = vec!["a.txt", "b.txt", "c.txt", "d.txt"];
    for file in files.iter().progress() {
        thread::sleep(Duration::from_millis(100)); // process each file
        let _ = file;
    }
} // bar finishes automatically when the iterator is exhausted
```

`ProgressBar::wrap_read` (and `wrap_write`) wraps a reader/writer so every byte that flows through advances the bar: perfect for hashing or copying a file with byte-accurate progress (see the Real-World Example).

### `{bytes}` and the `Human*` helpers

The same human-formatting logic behind `{bytes}` is exposed as standalone `Display` types you can use anywhere: in `println!`, in logs, or to build your own messages:

```rust
use indicatif::{HumanBytes, HumanCount, HumanDuration};
use std::time::Duration;

fn main() {
    println!("{}", HumanBytes(1_500_000));            // 1.43 MiB
    println!("{}", HumanCount(1_234_567));            // 1,234,567
    println!("{}", HumanDuration(Duration::from_secs(95))); // 2 minutes
}
```

Real output:

```text
1.43 MiB
1,234,567
2 minutes
```

---

## Key Differences

| Concern                         | Node (`cli-progress` / `ora`)                          | Rust (`indicatif`)                                                  |
| ------------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------- |
| Bars + spinners                 | Two separate libraries                                 | One crate; `ProgressBar` and `ProgressBar::new_spinner()`           |
| Template tokens                 | `{bar} {value} {total} {percentage}`                   | `{bar} {pos} {len} {percent} {bytes} {eta} …` (richer set)          |
| Byte formatting                 | Manual (`prettyBytes(value)`)                          | Built in: `{bytes}`, `{bytes_per_sec}`, `HumanBytes`                |
| Multiple bars                   | `MultiBar`                                             | `MultiProgress`                                                      |
| Animation timing                | Spinner auto-animates on a timer                       | Opt in with `enable_steady_tick(interval)`; otherwise you `tick()`  |
| Non-TTY behavior                | May still print lines unless you guard it              | **Auto-detects**: a piped/redirected target draws nothing           |
| Sharing across concurrency      | Single-threaded event loop; just close over the bar    | `ProgressBar` is `Arc`-backed and `Send + Sync`; clone into threads |
| Error handling on bad template  | Throws at runtime                                      | `with_template` returns a `Result` you must handle at compile time  |

The two deepest differences for a TypeScript/JavaScript developer:

1. **Concurrency is real here.** Node's bars live on one event loop. In Rust you typically spawn OS threads (or use `rayon`), and `indicatif` is designed for it: clone the bar (or use `MultiProgress`) and update from many threads safely. There is no single-threaded assumption to fall back on.
2. **Templates are validated up front.** `with_template` hands you a `Result`. A malformed template surfaces as a `TemplateError` you decide how to handle, rather than silently printing the wrong thing.

---

## Common Pitfalls

### Pitfall 1: Forgetting that `with_template` returns a `Result`

`set_style` wants a `ProgressStyle`, but `with_template` gives you a `Result<ProgressStyle, TemplateError>`. Passing it directly fails to compile:

```rust
use indicatif::{ProgressBar, ProgressStyle};

fn main() {
    let pb = ProgressBar::new(100);
    // does not compile (error[E0308]: mismatched types)
    pb.set_style(ProgressStyle::with_template("[{bar}] {pos}/{len}"));
}
```

The real compiler error:

```text
error[E0308]: mismatched types
 --> src/main.rs:6:18
  |
6 |     pb.set_style(ProgressStyle::with_template("[{bar}] {pos}/{len}"));
  |        --------- ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `ProgressStyle`, found `Result<ProgressStyle, TemplateError>`
  |        |
  |        arguments to this method are incorrect
  |
  = note: expected struct `ProgressStyle`
               found enum `Result<ProgressStyle, TemplateError>`
help: consider using `Result::expect` to unwrap the `Result<ProgressStyle, TemplateError>` value, panicking if the value is a `Result::Err`
```

**Fix:** `.unwrap()` (templates are usually constant, so a panic on a typo at startup is acceptable) or propagate the error with `?`:

```rust
use indicatif::{ProgressBar, ProgressStyle};

fn main() {
    let pb = ProgressBar::new(100);
    pb.set_style(ProgressStyle::with_template("[{bar}] {pos}/{len}").unwrap());
}
```

### Pitfall 2: Using `println!` while a bar is on screen

A live bar owns the bottom line of the terminal. A raw `println!` writes over it and leaves a garbled, doubled-up display. Route your output through the bar instead:

```rust
use indicatif::ProgressBar;
use std::thread;
use std::time::Duration;

fn main() {
    let pb = ProgressBar::new(5);
    for i in 0..5 {
        thread::sleep(Duration::from_millis(50));
        // pb.println prints above the bar, then redraws the bar intact
        pb.println(format!("processed item {i}"));
        pb.inc(1);
    }
    pb.finish_with_message("done");
}
```

For arbitrary code that prints (a logging macro, a library call) wrap it in `suspend`, which clears the bar, runs the closure, and redraws:

```rust
use indicatif::ProgressBar;
use std::thread;
use std::time::Duration;

fn main() {
    let pb = ProgressBar::new(3);
    pb.set_message("working");
    for _ in 0..3 {
        pb.suspend(|| {
            println!("a normal log line printed safely");
        });
        thread::sleep(Duration::from_millis(20));
        pb.inc(1);
    }
    pb.finish();
}
```

> **Tip:** If you use the `tracing` or `log` ecosystems, the `indicatif`-aware logger bridges exist (e.g. `tracing-indicatif`) so your structured logs and your bars coexist without manual `suspend` calls.

### Pitfall 3: The bar "doesn't show up" in CI or when piped

This is by design, not a bug. `indicatif` detects that stderr is not an interactive terminal and switches to a hidden draw target, so it produces no output when piped to a file or run in CI. To confirm your logic without a terminal, build a hidden bar explicitly — the API still works, which makes bars easy to keep in code paths that are also unit-tested:

```rust
use indicatif::ProgressBar;

fn main() {
    let pb = ProgressBar::hidden(); // never draws, but tracks state
    pb.inc(10);
    assert_eq!(pb.position(), 10);  // logic is exercisable without a TTY
}
```

If you genuinely want progress in a non-TTY context (a log you watch with `tail`), set an explicit draw target with `ProgressBar::with_draw_target` / `set_draw_target` using `ProgressDrawTarget::stderr()`.

### Pitfall 4: A spinner that never moves

A spinner only animates when something advances it. If your work is one long blocking call, you never call `tick()`, so the spinner freezes. Use `enable_steady_tick(interval)` to animate it on a background timer regardless of your work loop:

```rust
use indicatif::ProgressBar;
use std::time::Duration;

fn main() {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80)); // animates on its own
    // ... do one long blocking thing ...
    pb.finish_and_clear();
}
```

---

## Best Practices

- **Draw to stderr (the default).** Bars belong on stderr so a user can still pipe your program's *real* stdout output elsewhere without bar noise. `indicatif`'s default draw target is already stderr — don't move it to stdout.
- **Build the style once, clone it for many bars.** `ProgressStyle` is cheap to `clone()`; construct it a single time and hand a clone to each bar in a `MultiProgress`.
- **Set a reasonable redraw rate.** The default throttles redraws, but for tight loops with millions of iterations, `pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10))` or batching `inc` calls avoids spending all your time redrawing.
- **Prefer the adapters.** `iterator.progress()` and `pb.wrap_read(reader)` eliminate manual `inc`/`set_position` calls and are harder to get wrong.
- **Always finish the bar.** Call `finish*` (or `finish_and_clear` for spinners) so the cursor and terminal state are restored cleanly. Leaving a bar un-finished can leave the cursor hidden.
- **Use `{wide_bar}` for the main bar** so it adapts to terminal width, and reserve fixed `{bar:N}` widths for stacked multi-progress rows where alignment matters.

---

## Real-World Example

A production-flavored task: walk a file, hash-or-process it with **byte-accurate** progress by wrapping the reader, then report throughput. This mirrors what `sha256sum`-style or backup tools display.

```rust
// Cargo.toml: cargo add indicatif
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::{self, Read, Write};

fn main() -> io::Result<()> {
    // Set up a 2 MiB sample file to "process".
    let dir = std::env::temp_dir().join("indicatif_demo");
    fs::create_dir_all(&dir)?;
    let path = dir.join("data.bin");
    fs::write(&path, vec![0u8; 2 * 1024 * 1024])?;

    let len = fs::metadata(&path)?.len();
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.green/black}] \
             {bytes}/{total_bytes} ({bytes_per_sec})",
        )
        .unwrap()
        .progress_chars("##-"),
    );

    // wrap_read advances the bar by every byte read — no manual inc() needed.
    let file = fs::File::open(&path)?;
    let mut reader = pb.wrap_read(file);
    let mut sink = io::sink(); // stand-in for a hasher or destination file
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sink.write_all(&buf[..n])?;
    }

    pb.finish_with_message("hashed");
    fs::remove_dir_all(&dir)?;
    println!("processed {len} bytes");
    Ok(())
}
```

Running this in a terminal redraws a green byte bar that fills as the file is read; the program then prints `processed 2097152 bytes`. The byte counter, throughput, and elapsed time all come "for free" from the template tokens because `wrap_read` feeds the bar.

### Stacked bars for concurrent work

When several tasks run on their own threads, `MultiProgress` keeps each on its own line and redraws them as a group. Because `ProgressBar` is `Arc`-backed, you simply `move` each bar into its thread:

```rust
// Cargo.toml: cargo add indicatif
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::thread;
use std::time::Duration;

fn main() {
    let multi = MultiProgress::new();
    let style = ProgressStyle::with_template("{prefix:>12.cyan} [{bar:30}] {pos}/{len}")
        .unwrap()
        .progress_chars("=> ");

    let mut handles = Vec::new();
    for i in 0..3 {
        let pb = multi.add(ProgressBar::new(100)); // registered with the group
        pb.set_style(style.clone());               // reuse one style
        pb.set_prefix(format!("task-{i}"));
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                thread::sleep(Duration::from_millis(3 + i * 2));
                pb.inc(1);
            }
            pb.finish_with_message("done");
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}
```

In a terminal this renders three independently advancing bars stacked vertically, each prefixed `task-0` … `task-2`, all redrawing in place until every thread joins.

### Parallel iteration with `rayon`

If you already use `rayon` for data parallelism, enable indicatif's `rayon` feature and wrap a parallel iterator the same way you wrap a sequential one:

```toml
# Cargo.toml
[dependencies]
indicatif = { version = "0.18", features = ["rayon"] }
rayon = "1"
```

```rust
use indicatif::ParallelProgressIterator; // gated behind the "rayon" feature
use rayon::prelude::*;
use std::thread;
use std::time::Duration;

fn main() {
    let items: Vec<u64> = (0..200).collect();
    let sum: u64 = items
        .par_iter()
        .progress_count(items.len() as u64) // a thread-safe shared bar
        .map(|n| {
            thread::sleep(Duration::from_millis(5)); // pretend per-item work
            n * 2
        })
        .sum();
    println!("sum = {sum}");
}
```

Real output:

```text
sum = 39800
```

> **Warning:** `ParallelProgressIterator` is only exported when the `rayon` feature is enabled. Without it you get `error[E0432]: unresolved import indicatif::ParallelProgressIterator` and a note that the item is "configured out" by the `rayon` feature gate. Add the feature in `Cargo.toml` as shown.

---

## Further Reading

- [`indicatif` crate documentation (docs.rs)](https://docs.rs/indicatif/): full `ProgressBar`, `ProgressStyle`, and `MultiProgress` API.
- [`indicatif` template/style reference](https://docs.rs/indicatif/latest/indicatif/struct.ProgressStyle.html): every template token and color spec.
- [`indicatif` examples on GitHub](https://github.com/console-rs/indicatif/tree/main/examples): runnable bar, spinner, and multi-progress demos.

Within this guide:

- [clap derive API](/18-cli-tools/01-clap-derive/) and [subcommands](/18-cli-tools/02-subcommands/) — parse the arguments that drive the work your bar reports on.
- [Colored output](/18-cli-tools/05-colored-output/) — coloring text outside the bar, and respecting `NO_COLOR` (which also influences bar styling).
- [Terminal UI with ratatui](/18-cli-tools/03-terminal-ui/) — when a single bar isn't enough and you need a full-screen interactive UI.
- [File I/O with std::fs](/18-cli-tools/06-file-io/) — the readers/writers you wrap with `pb.wrap_read` / `pb.wrap_write`.
- [Cross-platform considerations](/18-cli-tools/09-cross-platform/) — terminal detection and behavior differences across platforms.
- Background on the macros and threading used here: [Section 02: Output and Formatting](/02-basics/04-output/) and [Section 01: Getting Started](/01-getting-started/).
- For browser-targeted progress UIs, bars do not apply; see [Section 19: WebAssembly](/19-wasm/).

---

## Exercises

### Exercise 1: A styled determinate bar

**Difficulty:** Beginner

**Objective:** Build a bar with a custom template and advance it in a loop.

**Instructions:** Create a `ProgressBar` for 50 units of work. Give it a style whose template shows a 30-cell cyan bar followed by `{pos}/{len}` and the ETA. Advance it by 1 each iteration with a short sleep, then finish with the message `complete`.

```rust
use indicatif::{ProgressBar, ProgressStyle};
use std::thread;
use std::time::Duration;

fn main() {
    let pb = ProgressBar::new(50);
    // TODO: set a style with template "[{bar:30.cyan/blue}] {pos}/{len} ({eta})"
    for _ in 0..50 {
        thread::sleep(Duration::from_millis(20));
        // TODO: advance the bar
    }
    // TODO: finish with the message "complete"
}
```

<details>
<summary>Solution</summary>

```rust
use indicatif::{ProgressBar, ProgressStyle};
use std::thread;
use std::time::Duration;

fn main() {
    let pb = ProgressBar::new(50);
    pb.set_style(
        ProgressStyle::with_template("[{bar:30.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    for _ in 0..50 {
        thread::sleep(Duration::from_millis(20));
        pb.inc(1);
    }
    pb.finish_with_message("complete");
}
```

</details>

### Exercise 2: A self-animating spinner

**Difficulty:** Intermediate

**Objective:** Show an indeterminate spinner that animates during one blocking step, then clears itself.

**Instructions:** Create a spinner, give it a custom set of tick glyphs, set the message `Connecting...`, enable a steady tick of 80 ms, sleep ~600 ms to simulate a blocking connection, then change the message to `Authenticating...`, sleep again, and finally `finish_and_clear` so no spinner line is left behind. Print `Connected.` afterward.

```rust
use indicatif::{ProgressBar, ProgressStyle};
use std::thread;
use std::time::Duration;

fn main() {
    let spinner = ProgressBar::new_spinner();
    // TODO: style with tick_strings, enable steady tick, drive two phases, clear
}
```

<details>
<summary>Solution</summary>

```rust
use indicatif::{ProgressBar, ProgressStyle};
use std::thread;
use std::time::Duration;

fn main() {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    spinner.enable_steady_tick(Duration::from_millis(80));

    spinner.set_message("Connecting...");
    thread::sleep(Duration::from_millis(600));

    spinner.set_message("Authenticating...");
    thread::sleep(Duration::from_millis(600));

    spinner.finish_and_clear();
    println!("Connected.");
}
```

</details>

### Exercise 3: Concurrent multi-progress downloads

**Difficulty:** Advanced

**Objective:** Drive several bars at once from separate threads using `MultiProgress`, with differently sized "downloads" measured in bytes.

**Instructions:** Write a helper `download(multi, name, size, style)` that adds a bar to a shared `MultiProgress`, sets its prefix to `name` and style to the shared style, then spawns a thread that advances the bar in 4 KiB chunks until it reaches `size`, finishing with a `done` message. In `main`, build a byte-oriented style (`{prefix} [{bar}] {bytes}/{total_bytes}`), launch three downloads of different sizes, join all threads, and print `All downloads finished`.

```rust
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::thread;
use std::time::Duration;

// TODO: fn download(...) -> thread::JoinHandle<()> { ... }

fn main() {
    let multi = MultiProgress::new();
    // TODO: build style, spawn three downloads, join, print summary
}
```

<details>
<summary>Solution</summary>

```rust
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::thread;
use std::time::Duration;

fn download(
    multi: &MultiProgress,
    name: &str,
    size: u64,
    style: &ProgressStyle,
) -> thread::JoinHandle<()> {
    let pb = multi.add(ProgressBar::new(size));
    pb.set_style(style.clone());
    pb.set_prefix(name.to_string());
    thread::spawn(move || {
        let mut done = 0;
        while done < size {
            thread::sleep(Duration::from_millis(4)); // pretend network I/O
            done = (done + 4096).min(size);
            pb.set_position(done);
        }
        pb.finish_with_message("done");
    })
}

fn main() {
    let multi = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{prefix:<14} [{bar:25.cyan/blue}] {bytes}/{total_bytes}",
    )
    .unwrap()
    .progress_chars("=> ");

    let handles = vec![
        download(&multi, "core.tar.gz", 300 * 1024, &style),
        download(&multi, "docs.tar.gz", 120 * 1024, &style),
        download(&multi, "extras.tar.gz", 500 * 1024, &style),
    ];
    for h in handles {
        h.join().unwrap();
    }
    println!("All downloads finished");
}
```

</details>
