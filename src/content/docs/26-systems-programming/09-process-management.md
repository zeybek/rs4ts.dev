---
title: "Process Management with `std::process::Command`"
description: "Spawn subprocesses in Rust with std::process::Command: args, env, pipes, and exit status, the synchronous, typed cousin of Node's child_process."
---

Shelling out to another program is something every real-world tool does eventually: invoking `git`, running a formatter, kicking off a build step, or piping data through a Unix utility. In Node you reach for `child_process`; in Rust the standard library ships `std::process::Command`, a fluent builder that spawns subprocesses, wires up their pipes, and reports their exit status, all without any external crate.

---

## Quick Overview

`std::process::Command` is a **builder** for launching external programs. You configure the executable, its arguments, environment, working directory, and how its standard streams are connected, then either run it to completion or spawn it and manage it as a live `Child`. For a senior TypeScript/JavaScript developer it maps almost one-to-one onto Node's `child_process` (`spawn`, `exec`, `execFile`), but it is synchronous by default, strongly typed, and forces you to handle the "could not even start the program" error separately from "the program ran but exited non-zero."

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition, which `cargo new` selects automatically. Every Rust snippet below was compiled and run on stable; the output shown is the real program output.

---

## TypeScript/JavaScript Example

In Node, the `child_process` module gives you several entry points. `spawn` streams output and is the workhorse; `execFile`/`exec` buffer the whole output into a callback or promise. A realistic "run a command, capture its output, react to its exit code" flow looks like this:

```typescript
// runner.ts — Node v22
import { spawn } from "node:child_process";
import { once } from "node:events";

interface RunResult {
  code: number | null; // null when the process was killed by a signal
  stdout: string;
  stderr: string;
}

async function run(
  program: string,
  args: string[],
  env: Record<string, string>,
): Promise<RunResult> {
  const child = spawn(program, args, {
    // Merge a few extra variables on top of the parent's environment.
    env: { ...process.env, ...env },
    stdio: ["ignore", "pipe", "pipe"],
  });

  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => (stdout += chunk));
  child.stderr.on("data", (chunk) => (stderr += chunk));

  // `spawn` emits "error" if the binary cannot be launched at all,
  // and "close" once it has exited.
  const [code] = (await once(child, "close")) as [number | null];
  return { code, stdout, stderr };
}

const result = await run("git", ["rev-parse", "--abbrev-ref", "HEAD"], {});
if (result.code === 0) {
  console.log("branch:", result.stdout.trim());
} else {
  console.error("git failed:", result.stderr.trim());
}
```

A few things a TypeScript developer takes for granted here:

- `spawn` is **asynchronous**: it returns immediately and you await events.
- The distinction between "could not launch the binary" (`error` event) and "the binary exited non-zero" (`code !== 0`) is real but easy to conflate, because both surface through the same `ChildProcess` object.
- `code` is `null` when the child was terminated by a signal rather than exiting normally.

---

## Rust Equivalent

The same idea in Rust. `Command` is the builder, `.output()` runs it to completion and buffers stdout/stderr, and the returned `Output` carries the exit `status`:

```rust playground
use std::process::Command;

fn main() {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output() // runs to completion, capturing stdout + stderr
        .expect("failed to launch git"); // Err only if git could not start

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout);
        println!("branch: {}", branch.trim());
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        eprintln!("git failed: {}", err.trim());
    }
}
```

The two failure modes that blur together in Node are **separate and explicit** in Rust:

- `.output()` returns `Result<Output, io::Error>`. The `Err` case means the program could **not be started** (binary not found, no permission, etc.).
- If it started, you get `Ok(Output)`, and `output.status.success()` tells you whether it exited with code `0`.

Here is the simplest possible run: fire a command and inspect its `ExitStatus`. This is the analogue of Node's `spawn` with inherited stdio (the child writes directly to your terminal):

```rust playground
use std::process::Command;

fn main() {
    let status = Command::new("echo")
        .arg("Hello from a subprocess")
        .status() // child inherits our stdout/stderr; we just get the status
        .expect("failed to start echo");

    println!("echo exited with: {status}");
    println!("success? {}", status.success());
    println!("code: {:?}", status.code());
}
```

Real output:

```text
Hello from a subprocess
echo exited with: exit status: 0
success? true
code: Some(0)
```

---

## Detailed Explanation

`std::process::Command` follows the **builder pattern** (see [Higher-Order Functions](/03-functions/04-higher-order/) for how method chaining works in Rust). `Command::new("git")` returns a `Command`; every configuration method takes `&mut self` and returns `&mut Self`, so calls chain. Nothing actually happens until you call one of the three **terminal** methods:

| Method | What it does | Node analogue |
| --- | --- | --- |
| `.status()` | Runs to completion; child inherits the parent's stdin/stdout/stderr; returns `ExitStatus` | `spawn` with `stdio: "inherit"` then await `close` |
| `.output()` | Runs to completion; **captures** stdout + stderr into a buffer; returns `Output` | `execFile` / `exec` |
| `.spawn()` | Starts the child and returns a live `Child` handle immediately, without waiting | `spawn` (the raw form) |

### Arguments: `.arg()` vs `.args()`

```rust playground
use std::process::Command;

fn main() {
    // One at a time:
    let mut cmd = Command::new("cargo");
    cmd.arg("build").arg("--release");

    // Or all at once from anything iterable:
    let flags = ["--locked", "--offline"];
    cmd.args(flags);

    println!("{cmd:?}");
}
```

Each argument is passed to the OS as a **separate, already-tokenized string**. Importantly, Rust does **not** run your command through a shell, so there is no word-splitting, glob expansion, or `$VAR` interpolation. `Command::new("echo").arg("a b c")` passes the single argument `a b c`, not three arguments. This is the same safety property as Node's `execFile` (as opposed to `exec`, which does spawn a shell), and it is your first line of defense against shell-injection. The [security section](/27-security/) goes deeper on why "never build a shell string from untrusted input" matters.

### Capturing output: the `Output` struct

`.output()` returns `Output`, which has three fields:

```rust playground
use std::process::Command;

fn main() {
    let output = Command::new("echo")
        .arg("captured line")
        .output()
        .expect("failed to run echo");

    let stdout = String::from_utf8_lossy(&output.stdout); // Vec<u8> -> Cow<str>
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("status: {}", output.status);
    println!("stdout: {:?}", stdout.trim());
    println!("stderr: {:?}", stderr.trim());
}
```

Real output:

```text
status: exit status: 0
stdout: "captured line"
stderr: ""
```

Note that `output.stdout` and `output.stderr` are `Vec<u8>`, **not** `String`. A subprocess can emit arbitrary bytes, so Rust hands you the raw bytes and lets *you* decide how to decode them. `String::from_utf8_lossy` replaces invalid UTF-8 with the replacement character; use `String::from_utf8` if you want to treat invalid bytes as an error instead. This contrasts with Node, where stream chunks are decoded to a JavaScript string for you (and can silently corrupt non-UTF-8 output).

### Exit status: `success()`, `code()`, and signals

`ExitStatus` answers three related questions:

- `status.success()` — did it exit with code `0`?
- `status.code()` — `Option<i32>`: `Some(n)` for a normal exit, `None` if the process was **terminated by a signal** (Unix).
- Signal death (Unix) — when `code()` is `None`, the process was killed by a signal; you can inspect *which* via `std::os::unix::process::ExitStatusExt::signal()`.

```rust playground
use std::process::Command;

fn main() {
    let status = Command::new("ls")
        .arg("/nonexistent-path-xyz")
        .status()
        .expect("failed to start ls");

    println!("success? {}", status.success());
    println!("code: {:?}", status.code());
}
```

Real output (stderr from `ls` is also printed because `.status()` inherits it):

```text
success? false
code: Some(1)
```

That `Some(1)` is the direct equivalent of Node's `result.code === 1`. The `None` case (signal death) is what Node represents as `code === null` plus a non-null `signal`.

### Environment variables

Each `Command` starts by **inheriting the parent's environment**, just like Node. You can add, override, clear, or remove specific variables:

```rust playground
use std::process::Command;

fn main() {
    let output = Command::new("printenv")
        .arg("GREETING")
        .env_clear()                       // start from an EMPTY environment
        .env("GREETING", "hello from rust") // then set exactly what we want
        .output()
        .expect("failed to run printenv");

    print!("{}", String::from_utf8_lossy(&output.stdout));
}
```

Real output:

```text
hello from rust
```

| Method | Effect |
| --- | --- |
| `.env("KEY", "val")` | Set/override one variable (like Node's `{ ...process.env, KEY: "val" }`) |
| `.envs(iter)` | Set many from an iterator of `(key, value)` pairs |
| `.env_remove("KEY")` | Remove one inherited variable |
| `.env_clear()` | Start from a completely empty environment |

### Working directory

`.current_dir(path)` sets the child's working directory, the analogue of Node's `{ cwd }` option:

```rust playground
use std::process::Command;

fn main() {
    let output = Command::new("pwd")
        .current_dir("/usr")
        .output()
        .expect("failed to run pwd");
    print!("cwd: {}", String::from_utf8_lossy(&output.stdout));
}
```

Real output:

```text
cwd: /usr
```

---

## Key Differences

| Concept | Node `child_process` | Rust `std::process::Command` |
| --- | --- | --- |
| Default execution model | Asynchronous (event/promise based) | **Synchronous** (blocks until done) |
| Shell involved? | `exec`/`execSync` yes; `spawn`/`execFile` no | **Never**: no shell unless you spawn one explicitly |
| Captured output type | Decoded `string` (or `Buffer`) | Raw `Vec<u8>`; you decode it |
| "Couldn't start" vs "ran but failed" | Both via `ChildProcess` events; easy to conflate | **Distinct**: `Err(io::Error)` vs `Ok(status)` with `success() == false` |
| Unused result | Silently ignored | `Result` is `#[must_use]`; compiler **warns** if you drop it |
| Signal-terminated child | `code === null`, `signal` set | `status.code() == None` |
| Reaping zombies | Automatic | You must `.wait()` (or let `Child`'s drop happen — but drop does **not** wait) |

### Synchronous by default — and how to go async

Unlike JavaScript, where everything in `child_process` is non-blocking (or an explicit `…Sync` variant), `Command::output()` and `Command::status()` **block the current thread** until the child exits. That is exactly what you want in a CLI tool. If you need concurrency, you have two idiomatic choices:

1. Spawn the child on its own thread (see [Native Threads with `std::thread`](/26-systems-programming/00-threads/)) and `join` later.
2. Use an async runtime. Tokio offers a drop-in `tokio::process::Command` with the same builder API but `async` terminals (`.output().await`). That belongs to the async chapters; the std API here is the foundation it is built on.

> **Note:** Rust's std `Command` is blocking; the lazy-future async model (covered in the async sections) does **not** apply here. `tokio::process` is the async counterpart when you need it.

---

## Common Pitfalls

### Pitfall 1: Expecting a shell (globbing, pipes, `$VARS`)

```rust playground
use std::process::Command;

fn main() {
    // Does NOT do what a TS dev coming from `exec` expects.
    // There is no shell, so `*.txt` is passed literally and `|` is just an argument.
    let output = Command::new("ls")
        .arg("*.txt | wc -l")
        .output()
        .expect("failed");
    eprintln!("{}", String::from_utf8_lossy(&output.stderr).trim());
}
```

`ls` receives the literal string `*.txt | wc -l` as one filename and complains it does not exist. If you genuinely need shell features, invoke the shell explicitly — `Command::new("sh").arg("-c").arg("ls *.txt | wc -l")` — but only with **trusted** command strings, never with interpolated user input.

### Pitfall 2: Ignoring the `Result` — the compiler stops you

If you forget to handle the result of `.status()`/`.output()`/`.spawn()`, you get a real warning, because these return `#[must_use]` types:

```rust playground
use std::process::Command;

fn main() {
    Command::new("echo").arg("hi").status();
}
```

Real `cargo build` warning:

```text
warning: unused `Result` that must be used
 --> src/bin/warn1.rs:4:5
  |
4 |     Command::new("echo").arg("hi").status();
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: this `Result` may be an `Err` variant, which should be handled
  = note: `#[warn(unused_must_use)]` on by default
help: use `let _ = ...` to ignore the resulting value
  |
4 |     let _ = Command::new("echo").arg("hi").status();
  |     +++++++
```

In Node, a fire-and-forget `spawn` whose `error` event you never listen for can crash the whole process with an uncaught exception. Rust catches the omission at compile time instead.

### Pitfall 3: Moving `child.stdout` out twice

`child.stdout` is an `Option<ChildStdout>`; calling `.unwrap()` or `.take()` **moves** the value out, so you can only do it once:

```rust
use std::process::{Command, Stdio};

fn main() -> std::io::Result<()> {
    let child = Command::new("echo")
        .arg("hi")
        .stdout(Stdio::piped())
        .spawn()?;

    let out1 = child.stdout.unwrap();
    let out2 = child.stdout.unwrap(); // does not compile (error[E0382]: use of moved value)
    drop((out1, out2));
    Ok(())
}
```

Real `cargo build` error:

```text
error[E0382]: use of moved value: `child.stdout`
    --> src/bin/err1.rs:10:16
     |
   9 |     let out1 = child.stdout.unwrap();
     |                ------------ -------- `child.stdout` moved due to this method call
     |                |
     |                help: consider calling `.as_ref()` or `.as_mut()` to borrow the type's contents
  10 |     let out2 = child.stdout.unwrap(); // does not compile (error[E0382]: use of moved value)
     |                ^^^^^^^^^^^^ value used here after move
     |
note: `Option::<T>::unwrap` takes ownership of the receiver `self`, which moves `child.stdout`
```

The idiomatic fix is `let stdout = child.stdout.take().expect("stdout was piped");`, which leaves `None` behind and hands you the stream once. See [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/) for why ownership transfers like this are the norm.

### Pitfall 4: Deadlocking on full pipes

If you `.spawn()` a child with both stdin and stdout piped, write a large amount to its stdin, and only *then* read its stdout, you can deadlock: the child's stdout pipe buffer fills, the child blocks trying to write, and so it never drains your stdin. Two safe patterns:

- For modest data, use `.output()` / `child.wait_with_output()`, which handle the draining for you.
- For large or streaming data, read stdout on a separate thread (or with an async runtime) while you write stdin.

Also remember to **close the child's stdin** (by dropping the `ChildStdin`) when you are done writing; otherwise a tool like `wc` waits forever for EOF. The stdin example below shows the idiomatic scoped drop.

### Pitfall 5: Expecting drop to wait

Dropping a `Child` does **not** wait for it and does **not** kill it — the subprocess keeps running, orphaned, and may become a zombie until reaped. Always call `.wait()` (or `.wait_with_output()`), or explicitly `.kill()` then `.wait()`, before the handle goes out of scope.

---

## Best Practices

- **Prefer `.output()` or `.status()` over `.spawn()`** unless you specifically need to interact with the live process. They are simpler and harder to misuse.
- **Treat the two failure modes separately.** Pattern-match the `Result` for "could not start," then check `status.success()` for "started but failed." Conflating them is the most common bug ported from JavaScript.
- **Never assemble a shell string from untrusted input.** Pass arguments individually via `.arg()`/`.args()`; the OS receives them pre-tokenized, so injection is impossible. Reserve `sh -c "…"` for fully trusted, static commands. (More in [Security](/27-security/).)
- **Decode output explicitly.** Use `String::from_utf8_lossy` when you expect text and tolerate garbage, or `String::from_utf8` when invalid UTF-8 should be an error.
- **Always reap children.** Call `.wait()` after `.spawn()`; do not rely on drop.
- **Stream long-running output** with a `BufReader` over `child.stdout` instead of buffering everything, so users see progress.
- **Propagate exit codes** from wrapper tools with `std::process::exit(code)` so callers and CI see the real status.

---

## Real-World Example

A miniature task runner, the kind of glue script you might otherwise write in `npm`/`bash`. It runs a sequence of named shell tasks with a shared environment, **streams each task's output live** with a prefix, tracks failures, and exits non-zero if anything failed (so CI notices):

```rust
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

/// What we learned after running one task.
struct TaskOutcome {
    name: String,
    code: Option<i32>,
}

/// Run a shell command, streaming its stdout live with a per-task prefix,
/// then report the exit code.
fn run_task(
    name: &str,
    shell_cmd: &str,
    env: &HashMap<&str, &str>,
) -> std::io::Result<TaskOutcome> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(shell_cmd)
        .envs(env) // merged on top of the inherited environment
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Take the stream once, then read it line by line as the child produces it.
    let stdout = child.stdout.take().expect("stdout was piped");
    for line in BufReader::new(stdout).lines() {
        println!("[{name}] {}", line?);
    }

    let status = child.wait()?; // reap the child and get its status
    Ok(TaskOutcome { name: name.to_string(), code: status.code() })
}

fn main() -> std::io::Result<()> {
    let mut env = HashMap::new();
    env.insert("NODE_ENV", "production");

    let tasks = [
        ("env", "echo building in $NODE_ENV mode"),
        ("count", "seq 1 3"),
        ("lint", "echo 'lint: 1 problem' && exit 1"),
    ];

    let mut failed = 0;
    for (name, cmd) in tasks {
        let outcome = run_task(name, cmd, &env)?;
        match outcome.code {
            Some(0) => println!("OK   {}", outcome.name),
            other => {
                failed += 1;
                println!("FAIL {} (code {:?})", outcome.name, other);
            }
        }
    }

    println!("{failed} task(s) failed");
    if failed > 0 {
        std::process::exit(1); // propagate failure to the caller / CI
    }
    Ok(())
}
```

Real output (and the process exits with code `1`):

```text
[env] building in production mode
OK   env
[count] 1
[count] 2
[count] 3
OK   count
[lint] lint: 1 problem
FAIL lint (code Some(1))
1 task(s) failed
```

This is the synchronous, type-checked cousin of a Node task runner. Notice there is no callback nesting and no event wiring: the control flow reads top-to-bottom, and every fallible step is a `?` that short-circuits on error.

---

## Further Reading

- [`std::process::Command`](https://doc.rust-lang.org/std/process/struct.Command.html) — the full builder API.
- [`std::process::Child`](https://doc.rust-lang.org/std/process/struct.Child.html): handle for a spawned process (`wait`, `kill`, `id`, `stdin`/`stdout`/`stderr`).
- [`std::process::Stdio`](https://doc.rust-lang.org/std/process/struct.Stdio.html) — how to wire up the standard streams (`piped`, `inherit`, `null`, `from`).
- [`std::process::exit`](https://doc.rust-lang.org/std/process/fn.exit.html): terminate the current process with a chosen code.
- Sibling topics in this section: [Native Threads with `std::thread`](/26-systems-programming/00-threads/) (run a blocking child off the main thread), [Channels](/26-systems-programming/03-channels/) (collect results from many child-running threads), [Signal Handling and Clean Shutdown](/26-systems-programming/08-signals/) (graceful shutdown that also tears down children), [Low-Level Networking](/26-systems-programming/07-networking/).
- Foundations used above: [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/) (why `take()`/`unwrap()` move values), [Higher-Order Functions](/03-functions/04-higher-order/) (builder-style chaining), and the security guidance in [Security](/27-security/).
- Back to the [section overview](/26-systems-programming/), or revisit [getting started](/01-getting-started/) and [the basics](/02-basics/).

---

## Exercises

### Exercise 1: Capture and trim

**Difficulty:** Beginner

**Objective:** Get comfortable with `.output()` and decoding stdout.

**Instructions:** Write a program that runs `date "+%Y"` (the `date` command with that format string as a single argument), captures the output, trims trailing whitespace, and prints `Current year: <year>`. Handle the "could not start" case with `expect`, and verify `status.success()` before trusting the output.

<details>
<summary>Solution</summary>

```rust playground
use std::process::Command;

fn main() {
    let output = Command::new("date")
        .arg("+%Y")
        .output()
        .expect("failed to run date");

    if output.status.success() {
        let year = String::from_utf8_lossy(&output.stdout);
        println!("Current year: {}", year.trim());
    } else {
        eprintln!("date failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
}
```

A real run prints something like `Current year: 2026`. `String::from_utf8_lossy` turns the captured `Vec<u8>` into text, and `.trim()` removes the trailing newline `date` emits.

</details>

### Exercise 2: Pipe data into a child's stdin

**Difficulty:** Intermediate

**Objective:** Spawn a child with a piped stdin, write to it, and read the result back — without deadlocking.

**Instructions:** Spawn `wc -w` with `Stdio::piped()` for both stdin and stdout. Write the bytes `"one two three four five\n"` to the child's stdin, then **drop the stdin handle** so the child sees EOF. Collect the child's output with `wait_with_output()` and print the word count. (Expected: `5`.)

<details>
<summary>Solution</summary>

```rust playground
use std::io::Write;
use std::process::{Command, Stdio};

fn main() -> std::io::Result<()> {
    let mut child = Command::new("wc")
        .arg("-w")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    // Inner scope: when `stdin` is dropped here, the pipe closes and the
    // child sees EOF — otherwise `wc` would wait forever for more input.
    {
        let mut stdin = child.stdin.take().expect("child has no stdin");
        stdin.write_all(b"one two three four five\n")?;
    }

    let output = child.wait_with_output()?;
    print!("word count: {}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}
```

Real output:

```text
word count:        5
```

> **Note:** The exact leading-space padding of `wc` output is platform-dependent (macOS and GNU coreutils pad differently); the count `5` is the part that matters.

The scoped block is the key detail: dropping `ChildStdin` closes the write end of the pipe, and only then does `wc` finish reading and produce its count. Forgetting to close stdin is the classic cause of a hung subprocess.

</details>

### Exercise 3: Spawn, time out, and kill

**Difficulty:** Advanced

**Objective:** Manage a long-running `Child` directly: start it, give it a moment, then terminate it and reap it.

**Instructions:** Spawn `sleep 30`. Print its PID via `child.id()`. Sleep the main thread for 100 ms, then call `child.kill()` followed by `child.wait()`. Print whether the final status was successful and what `status.code()` returns. Explain in a comment why the code is `None`.

<details>
<summary>Solution</summary>

```rust playground
use std::process::Command;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut child = Command::new("sleep").arg("30").spawn()?;
    println!("spawned sleep with pid {}", child.id());

    std::thread::sleep(Duration::from_millis(100));

    child.kill()?;              // send SIGKILL to the child
    let status = child.wait()?; // reap it so it is not left as a zombie

    println!("after kill, success? {}", status.success());
    // code() is None because the process was terminated by a signal
    // (SIGKILL) rather than exiting normally with a numeric code.
    println!("code: {:?}", status.code());
    Ok(())
}
```

Real output (the PID varies):

```text
spawned sleep with pid 64828
after kill, success? false
code: None
```

`kill()` sends `SIGKILL`; because the process died from a signal rather than calling `exit`, `status.code()` is `None` — the Rust equivalent of Node's `code === null` with a non-null `signal`. The follow-up `wait()` reaps the child so it does not linger as a zombie.

</details>
