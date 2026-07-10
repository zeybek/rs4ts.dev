---
title: "Signal Handling and Clean Shutdown"
description: "Catch SIGINT and SIGTERM in Rust for graceful shutdown with ctrlc and signal-hook, the systems-level answer to Node's process.on(SIGTERM)."
---

A long-running process (a web server, a queue worker, a daemon) does not get to choose when it dies. The operating system, an orchestrator like Kubernetes, or a developer hitting Ctrl-C will send it a **signal** and expect it to wind down gracefully: stop accepting work, finish in-flight requests, flush logs and buffers, release locks, and exit. This page is about catching those signals in Rust and turning an abrupt kill into an orderly shutdown.

---

## Quick Overview

A **signal** is an asynchronous notification the kernel delivers to a process: `SIGINT` (Ctrl-C), `SIGTERM` (the polite "please stop" an orchestrator sends), `SIGHUP` (terminal hang-up, often repurposed as "reload config"), and others. By default most of them just terminate your process immediately. To shut down cleanly you install a handler that flips a flag, and your main loop notices the flag and unwinds in an orderly way.

If you have written a Node service you have done this with `process.on("SIGTERM", ...)`. Rust has no built-in `process.on`; instead you reach for one of two well-maintained crates: **`ctrlc`** for the simple "catch Ctrl-C / terminate signals and run a closure" case, or **`signal-hook`** for everything more demanding (specific signals, iterating over signals on a dedicated thread, double-Ctrl-C force-quit, async runtimes). The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition, and `cargo new` selects that edition automatically; every Rust snippet below was compiled and run on stable.

> **Note:** Signals are a Unix concept. On Windows the closest equivalents are console control events (Ctrl-C, Ctrl-Break) and there is no `SIGTERM`. The `ctrlc` crate papers over the difference for the Ctrl-C case; `signal-hook` and Tokio's Unix signal API are Unix-only. This page focuses on Unix (Linux/macOS), which is where servers run. For related building blocks see [atomic operations](/26-systems-programming/04-atomic-operations/) (the `AtomicBool` flag pattern), [channels](/26-systems-programming/03-channels/) (waking a worker on shutdown), and [process management](/26-systems-programming/09-process-management/) (*sending* signals to child processes).

---

## TypeScript/JavaScript Example

In Node, the `process` object is an `EventEmitter` and signals arrive as events. A production-grade shutdown handler typically: sets a "shutting down" flag so a second Ctrl-C can force-quit, stops the work source, drains in-flight work with a timeout, then calls `process.exit`.

```typescript
// shutdown.mts — Node v22
let shuttingDown = false;
const timer = setInterval(() => {}, 1000); // stand-in for "the server is alive"

function shutdown(signal: NodeJS.Signals): void {
  // A SECOND signal while we are already draining: give up and force-quit.
  if (shuttingDown) {
    console.log(`\n${signal} again — force exit`);
    process.exit(1);
  }
  shuttingDown = true;
  console.log(`\n${signal} received: draining...`);
  clearInterval(timer); // stop accepting new work
  // Finish in-flight work, then exit. A real server would await its drain.
  setTimeout(() => {
    console.log("clean exit");
    process.exit(0);
  }, 50);
}

process.on("SIGINT", () => shutdown("SIGINT")); // Ctrl-C
process.on("SIGTERM", () => shutdown("SIGTERM")); // `kill`, Docker stop, k8s
console.log(`pid ${process.pid} running`);
```

Running it and sending `SIGTERM` (the same signal `docker stop` and Kubernetes send) prints:

```text
pid 80723 running

SIGTERM received: draining...
clean exit
```

Key facts about the Node model:

- The signal handler runs on the **main thread** between event-loop ticks — it is not preemptive. Your callback is ordinary JavaScript and can touch any state.
- If you register **no** `SIGTERM` listener, Node uses the OS default and the process dies instantly with no chance to clean up.
- The async drain (`setTimeout` / `await`) works because Node's event loop is still running; once you call `process.exit`, pending callbacks are abandoned.

---

## Rust Equivalent

Rust does not let you run arbitrary code in a real OS signal handler (signal handlers are wildly restricted — almost nothing is *async-signal-safe*). The idiomatic pattern is therefore: a tiny handler that only sets an `AtomicBool`, and a main loop that polls it. The `ctrlc` crate wraps this safely.

```toml
# Cargo.toml
[dependencies]
ctrlc = "3"
```

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    // Shared, thread-safe flag. The signal handler sets it; main reads it.
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);

    // `set_handler` installs a handler for Ctrl-C (and SIGTERM with the
    // default feature set on Unix). The closure runs on a dedicated thread.
    ctrlc::set_handler(move || {
        println!("\nreceived Ctrl-C, shutting down...");
        r.store(false, Ordering::SeqCst);
    })
    .expect("error setting Ctrl-C handler");

    println!("working... press Ctrl-C to stop");
    let mut tick = 0u64;
    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(50));
        tick += 1;
        // (Demo only) stop on our own after a few ticks so the program ends.
        // In a real service this loop runs until a signal flips `running`.
        if tick >= 4 {
            running.store(false, Ordering::SeqCst);
        }
    }
    println!("clean exit after {tick} ticks");
}
```

Output (the demo stops itself; in a real run, pressing Ctrl-C ends it):

```text
working... press Ctrl-C to stop
clean exit after 4 ticks
```

For real services you usually want more control: specific signals, a `SIGHUP`-means-reload distinction, or the double-signal force-quit. That is `signal-hook`'s job:

```toml
# Cargo.toml
[dependencies]
signal-hook = "0.4"
```

```rust
use signal_hook::consts::TERM_SIGNALS; // SIGTERM, SIGQUIT, SIGINT
use signal_hook::flag;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shutdown = Arc::new(AtomicBool::new(false));

    // `flag::register` wires "when this signal arrives, set this bool to true".
    // No closure, no async-signal-safety footguns — the crate does it right.
    for &sig in TERM_SIGNALS {
        flag::register(sig, Arc::clone(&shutdown))?;
    }

    println!(
        "server up (pid {}); send SIGINT/SIGTERM to stop",
        std::process::id()
    );

    while !shutdown.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(100));
    }

    println!("shutdown flag set — draining connections, flushing logs...");
    println!("bye");
    Ok(())
}
```

Running it and sending `SIGTERM` (via `kill -TERM <pid>`) prints:

```text
server up (pid 51687); send SIGINT/SIGTERM to stop
shutdown flag set — draining connections, flushing logs...
bye
```

---

## Detailed Explanation

### Why you cannot just run code in a signal handler

When the kernel delivers a signal, it interrupts your program *wherever it happens to be* — possibly in the middle of `malloc`, holding a lock, halfway through a `println!`. Inside a real signal handler you may only call a short list of **async-signal-safe** functions; allocating memory, locking a mutex, or doing formatted I/O can deadlock or corrupt state. JavaScript hides this because Node delivers the "signal" to your callback between event-loop ticks, not in the real handler.

Both `ctrlc` and `signal-hook` solve this the same way: the actual OS handler does almost nothing (it writes to a `static AtomicBool` or to a self-pipe) and your *normal* code observes that later. The `AtomicBool` + polling loop is the correct architecture, not a workaround you settle for.

### `ctrlc::set_handler` line by line

- `Arc::new(AtomicBool::new(true))` creates a flag on the heap that multiple threads can share. `Arc` is the thread-safe reference count (see [reference counting](/05-ownership/07-reference-counting/)); `AtomicBool` lets us mutate it without a `Mutex`.
- `let r = Arc::clone(&running)` makes a second handle. We `move` `r` into the closure because the closure outlives `main`'s stack frame — it runs on a dedicated handler thread `ctrlc` spawns.
- `r.store(false, Ordering::SeqCst)` is the entire handler body: flip the flag. (`Ordering` controls how this write becomes visible to other threads — see [memory ordering](/26-systems-programming/05-memory-ordering/). `SeqCst` is the safe default; `Relaxed` is also fine for a simple shutdown flag.)
- The `while running.load(...)` loop is your real work. It checks the flag each iteration and exits the moment it goes `false`.

### `signal-hook`'s `flag::register`

`flag::register(sig, Arc<AtomicBool>)` is `signal-hook`'s most ergonomic primitive: it associates a signal number with a bool and returns a `SigId` you could later use to unregister. Looping over `TERM_SIGNALS` (a constant slice of `[SIGTERM, SIGQUIT, SIGINT]` on Linux) registers all the "please terminate" signals at once, so any of them sets the same flag. There is no closure to get wrong and nothing unsafe in your code.

### Where the analogy to Node breaks down

| Aspect | Node.js | Rust (`ctrlc` / `signal-hook`) |
| --- | --- | --- |
| Handler runs on | Main thread, between ticks | A dedicated handler thread (`ctrlc`) or you read a flag/stream |
| Handler body | Arbitrary JS, can touch all state | Effectively just "set a flag"; do real work in your own code |
| Default if unhandled | Process terminates | Process terminates |
| Async drain | `await` works (event loop alive) | You poll/select the flag; with Tokio you `select!` on a signal future |
| `SIGTERM` on Windows | Emulated/limited | Does not exist; only Ctrl-C/Ctrl-Break console events |

> **Tip:** Unlike Node, where forgetting to `clearInterval`/`unref` can keep the process alive, a Rust binary exits as soon as `main` returns. Your job is purely to *break out of the loop* cleanly; you do not have to tear down timers to let the process die.

---

## Key Differences

### Setting a flag vs. doing the work

The single most important mental shift: in Rust the signal handler **does not perform the shutdown**. It records that a shutdown was requested. The actual draining, flushing, and joining happens in your normal control flow once it observes the flag. This keeps everything you do during shutdown (allocating, logging, locking) out of the dangerous signal context.

### `ctrlc` vs. `signal-hook` — which to reach for

| Need | Use |
| --- | --- |
| "Catch Ctrl-C / terminate, run a closure" | `ctrlc` |
| Handle *specific* signals (`SIGHUP`, `SIGUSR1`, ...) | `signal-hook` |
| Distinguish reload (`SIGHUP`) from shutdown | `signal-hook` (iterator) |
| Double-Ctrl-C force-quit | `signal-hook` (`register_conditional_shutdown`) |
| React on a dedicated thread, by name | `signal-hook` (`iterator::Signals`) |
| Inside Tokio / async | `tokio::signal` (built into Tokio) |

`ctrlc` is intentionally minimal; `signal-hook` is the toolbox. They can even coexist, but pick one owner per signal.

### Signals you will actually see in production

- **`SIGINT` (2)**: interactive Ctrl-C in a terminal.
- **`SIGTERM` (15)**: the polite "stop now"; what `kill <pid>`, `docker stop`, and Kubernetes send first. Honor it.
- **`SIGKILL` (9)**: *uncatchable*. You cannot handle it; the kernel destroys you. This is why orchestrators give you a grace period after `SIGTERM` before escalating to `SIGKILL`.
- **`SIGHUP` (1)** — historically "terminal disconnected"; conventionally repurposed by daemons to mean "reload configuration without restarting."
- **`SIGQUIT` (3)** — like `SIGINT` but conventionally produces a core dump; Ctrl-\\ in a terminal.

> **Warning:** You can never catch or ignore `SIGKILL` (9) or `SIGSTOP` (17/19). If your process must survive a `kill -9`, the answer is external supervision (systemd, an orchestrator) plus crash-safe persistence — not a signal handler.

---

## Common Pitfalls

### Pitfall 1: Doing heavy work inside the handler closure

It is tempting to write the whole shutdown — close the database, flush files, log a summary — directly inside `ctrlc::set_handler(|| { ... })`. With `ctrlc` the closure runs on a normal thread so it *can* allocate, but you still create races: the handler thread now mutates shared state concurrently with your main thread with no coordination. Keep the handler to "set the flag" and do the work in `main`, where you already own that state.

### Pitfall 2: Registering two handlers for the same signal

`ctrlc::set_handler` may be called only **once** per process. A second call returns an error rather than silently replacing the first:

```rust
fn main() {
    ctrlc::set_handler(|| {}).expect("first handler ok");
    // Registering a SECOND handler returns Err(Error::MultipleHandlers).
    match ctrlc::set_handler(|| {}) {
        Ok(()) => println!("second handler installed (unexpected)"),
        Err(e) => println!("second set_handler failed: {e}"),
    }
}
```

Real runtime output:

```text
second set_handler failed: Ctrl-C error: Ctrl-C signal handler already registered
```

The fix is architectural: install the handler exactly once at startup and share the resulting flag (or a channel) with everything that needs to know about shutdown.

### Pitfall 3: A blocking call that never re-checks the flag

If your main loop is parked inside a blocking call (`TcpListener::accept()`, a blocking `recv()`, a long `thread::sleep`), flipping the flag does nothing until that call returns. The flag is set, but you are not looking at it. Solutions: use a non-blocking listener and poll (shown in the [Real-World Example](#real-world-example)), set a read timeout, or wake the blocked thread with a [channel](/26-systems-programming/03-channels/) the handler sends to.

### Pitfall 4: Forgetting `move` and hitting a borrow/lifetime error

The handler closure outlives `main`'s stack frame, so it must **own** what it captures. If you write `ctrlc::set_handler(|| { r.store(...) })` without `move` while `r` is a local, the compiler rejects it because the closure would borrow a value that does not live long enough. Add `move` and clone the `Arc` first (`let r = Arc::clone(&running);`) so both the handler and `main` keep a live handle. See [closures](/03-functions/03-arrow-vs-closures/) and [ownership rules](/05-ownership/01-ownership-rules/) for the underlying mechanics.

### Pitfall 5: Expecting `SIGTERM` on Windows

`signal-hook` and `tokio::signal::unix` are Unix-only; `SIGTERM` has no Windows equivalent. If you ship cross-platform, gate Unix-specific signal code behind `#[cfg(unix)]` and rely on `ctrlc` (which handles Ctrl-C on both) for the portable path.

---

## Best Practices

- **Handler sets a flag; `main` does the work.** This is the whole discipline. Never drain or flush from inside the OS handler context.
- **Catch the full terminate family, not just Ctrl-C.** Use `signal-hook`'s `TERM_SIGNALS` so `kill` / `docker stop` (which send `SIGTERM`) are honored, not only interactive `SIGINT`.
- **Bound your drain with a timeout.** Orchestrators escalate `SIGTERM` to an uncatchable `SIGKILL` after a grace period (Kubernetes defaults to 30 seconds). If your clean shutdown can hang, cap it so you exit cleanly before you are killed.
- **Make the second signal a force-quit.** A user who hits Ctrl-C twice wants out *now*. `signal-hook`'s `register_conditional_shutdown` forces an immediate process exit on the second signal so the process dies right away (shown below).
- **In async code, use the runtime's signal support.** Inside Tokio, `tokio::signal::unix::signal` gives you a future you can `select!` on; do not block a Tokio worker on a polling loop.
- **Log that you received a signal and when you finish.** Shutdown is exactly when observability matters; a "received SIGTERM, draining, done" trail turns a mysterious restart into a readable one.

### The production "graceful, then force-quit" pattern

This combines two `signal-hook` registrations per signal. `register_conditional_shutdown` says "if the flag is already `true` when this signal fires, immediately exit the process with the given status code (here `1`)"; the plain `register` sets the flag. So the first signal requests a graceful shutdown and the second one force-exits the process. The `1` is the exit code used for the force-quit.

```rust
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::flag;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let term = Arc::new(AtomicBool::new(false));

    for &sig in TERM_SIGNALS {
        // If `term` is ALREADY true when this signal fires, exit immediately
        // with status 1 (no graceful path) — so a 2nd Ctrl-C force-quits.
        flag::register_conditional_shutdown(sig, 1, Arc::clone(&term))?;
        // First signal: set our flag so the main loop shuts down gracefully.
        flag::register(sig, Arc::clone(&term))?;
    }

    println!(
        "running (pid {}). 1st signal = graceful, 2nd = force-quit",
        std::process::id()
    );

    let mut work_left = 30;
    while !term.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(100));
        work_left -= 1;
        if work_left == 0 {
            break;
        }
    }

    if term.load(Ordering::Relaxed) {
        println!("graceful shutdown requested; finishing current job...");
        thread::sleep(Duration::from_millis(150)); // simulate draining
        println!("done");
    } else {
        println!("work completed normally");
    }
    Ok(())
}
```

Sending a single `SIGINT` lets it drain and print `done`:

```text
running (pid 58542). 1st signal = graceful, 2nd = force-quit
graceful shutdown requested; finishing current job...
done
```

Sending a **second** `SIGINT` while it is draining cuts the drain short: it never prints `done` and the process exits immediately with a failure status:

```text
running (pid 58841). 1st signal = graceful, 2nd = force-quit
graceful shutdown requested; finishing current job...
```

> **Note:** The order of the two registrations matters. Register the conditional-shutdown first so that on the *second* signal it runs and force-exits the process; the plain `register` keeps setting the flag for the graceful path on the first signal.

### Reacting to signals by name on a dedicated thread

When you need to *distinguish* signals — `SIGHUP` reloads config, terminate signals shut down — `signal-hook`'s `iterator::Signals` turns signals into an ordinary iterator you drain on a background thread. `low_level::signal_name` gives you a printable name.

```rust
use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use signal_hook::iterator::Signals;
use signal_hook::low_level;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut signals = Signals::new([SIGINT, SIGTERM, SIGHUP, SIGQUIT])?;
    let handle = signals.handle(); // lets us stop the iterator later

    // A background thread owns the signal stream and reacts to each signal.
    let worker = std::thread::spawn(move || {
        for signal in &mut signals {
            let name = low_level::signal_name(signal).unwrap_or("UNKNOWN");
            match signal {
                SIGHUP => println!("got {name}: reloading config (not shutting down)"),
                SIGINT | SIGTERM | SIGQUIT => {
                    println!("got {name}: initiating clean shutdown");
                    break;
                }
                _ => unreachable!(),
            }
        }
        println!("signal thread done");
    });

    // Simulate a config reload, then a termination request.
    low_level::raise(SIGHUP)?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    low_level::raise(SIGTERM)?;

    worker.join().unwrap();
    handle.close(); // unblock the iterator if it were still looping
    println!("main exiting cleanly");
    Ok(())
}
```

Output:

```text
got SIGHUP: reloading config (not shutting down)
got SIGTERM: initiating clean shutdown
signal thread done
main exiting cleanly
```

### Async shutdown with Tokio

If your service is async, do not spawn a blocking poll loop; use Tokio's own signal futures and `select!` so a signal cancels your server alongside everything else.

```toml
# Cargo.toml
[dependencies]
tokio = { version = "1", features = ["full"] }
```

```rust
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    // A long-running task we want to cancel cleanly on shutdown.
    let server = tokio::spawn(async {
        loop {
            sleep(Duration::from_millis(50)).await; // pretend: serve requests
        }
    });

    println!("server running (pid {})", std::process::id());

    // Wait for whichever termination signal arrives first.
    tokio::select! {
        _ = sigint.recv()  => println!("\nSIGINT received"),
        _ = sigterm.recv() => println!("\nSIGTERM received"),
    }

    println!("shutting down: stopping background tasks...");
    server.abort();
    sleep(Duration::from_millis(100)).await; // drain in-flight work
    println!("graceful shutdown complete");
    Ok(())
}
```

Sending `SIGTERM` produces:

```text
server running (pid 72128)

SIGTERM received
shutting down: stopping background tasks...
graceful shutdown complete
```

> **Note:** Rust futures are **lazy** — `signal(...)` only produces values once it is polled by a runtime, the opposite of an eager JavaScript `Promise`. That is why the signal future lives inside `select!` under `#[tokio::main]` rather than firing on its own. For more on this model see [Section 11: async/concurrency](/11-async/10-concurrency/).

---

## Real-World Example

A TCP echo server that shuts down gracefully on `SIGINT`/`SIGTERM`. The catch with `std`'s `TcpListener` is that `accept()` blocks forever, so a flag alone would never be checked. The realistic `std`-only fix is a **non-blocking** listener that wakes every 50 ms to re-check the shutdown flag. When shutdown is requested, it stops accepting new connections and exits cleanly.

```toml
# Cargo.toml
[dependencies]
signal-hook = "0.4"
```

```rust
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::flag;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn handle_client(mut stream: TcpStream) -> std::io::Result<()> {
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf)?;
    stream.write_all(&buf[..n])?; // echo it back
    Ok(())
}

fn main() -> std::io::Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));
    for &sig in TERM_SIGNALS {
        flag::register(sig, Arc::clone(&shutdown))
            .expect("failed to register signal handler");
    }

    let listener = TcpListener::bind("127.0.0.1:0")?; // port 0 = OS picks one
    let addr = listener.local_addr()?;
    // Don't block forever in accept(); wake periodically to check the flag.
    listener.set_nonblocking(true)?;
    println!("echo server listening on {addr}");

    let mut handled = 0u32;
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _peer)) => {
                // Serve this connection in blocking mode (toy: one at a time).
                stream.set_nonblocking(false).ok();
                if handle_client(stream).is_ok() {
                    handled += 1;
                }
            }
            // No pending connection right now: nap, then re-check the flag.
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e),
        }
    }

    println!("shutdown signal received; served {handled} request(s). Closing listener.");
    Ok(())
}
```

Connecting two clients (each sends a line, gets it echoed) and then sending `SIGTERM` produces this server output:

```text
echo server listening on 127.0.0.1:51002
shutdown signal received; served 2 request(s). Closing listener.
```

For a production server you would offload `handle_client` to a [thread pool](/26-systems-programming/01-thread-pools/) or run the whole thing on [Tokio](/11-async/) and track in-flight connections so the drain waits for them. The signal-handling skeleton — register a flag, poll it in the accept loop, report a clean exit — stays the same. For the lower-level networking details see [low-level networking](/26-systems-programming/07-networking/).

---

## Further Reading

- [The Rust Standard Library — `std::process`](https://doc.rust-lang.org/std/process/): `exit`, `id`, and process basics.
- [`ctrlc` crate documentation](https://docs.rs/ctrlc/): the simple Ctrl-C / terminate handler.
- [`signal-hook` crate documentation](https://docs.rs/signal-hook/): `flag`, `iterator::Signals`, `low_level`, `register_conditional_shutdown`.
- [`tokio::signal` documentation](https://docs.rs/tokio/latest/tokio/signal/index.html): async signal handling inside Tokio.
- [`signal(7)` man page](https://man7.org/linux/man-pages/man7/signal.7.html): the authoritative list of Unix signals and default dispositions.
- Related guide sections:
  - [Atomic operations](/26-systems-programming/04-atomic-operations/): the `AtomicBool` flag the handler flips.
  - [Memory ordering](/26-systems-programming/05-memory-ordering/): what `Ordering::SeqCst` / `Relaxed` mean for the flag.
  - [Channels](/26-systems-programming/03-channels/): waking a blocked worker thread on shutdown.
  - [Process management](/26-systems-programming/09-process-management/): *sending* signals to child processes you spawned.
  - [Low-level networking](/26-systems-programming/07-networking/): the `TcpListener` used in the real-world example.
  - [Section 11: async/concurrency](/11-async/10-concurrency/): the Tokio model behind the async example.
  - [Section 27: Security](/27-security/): why a clean shutdown that flushes audit logs and releases secrets matters.
  - [Section 01: Getting Started](/01-getting-started/) and [Section 02: Basics](/02-basics/): Cargo and language fundamentals if any syntax here is new.

---

## Exercises

### Exercise 1 — Graceful counter

**Difficulty:** Beginner

**Objective:** Use `ctrlc` to turn an abrupt Ctrl-C into a clean exit that reports how much work it finished.

**Instructions:** Write a program that increments a counter in a loop (sleeping briefly each iteration). Install a `ctrlc` handler that flips a shared `AtomicBool`. When the flag goes false, break out and print `Processed N items before shutdown. Goodbye!`. (For a self-contained demo you may stop after a few iterations; in a real run, Ctrl-C ends it.)

<details>
<summary>Solution</summary>

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    ctrlc::set_handler(move || r.store(false, Ordering::SeqCst))
        .expect("error setting handler");

    let mut processed = 0u64;
    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(20));
        processed += 1;
        if processed >= 3 {
            running.store(false, Ordering::SeqCst); // demo stop
        }
    }
    println!("Processed {processed} items before shutdown. Goodbye!");
}
```

Output:

```text
Processed 3 items before shutdown. Goodbye!
```

</details>

### Exercise 2 — Reload vs. shutdown

**Difficulty:** Intermediate

**Objective:** Distinguish `SIGHUP` (reload) from terminate signals using `signal-hook`'s iterator.

**Instructions:** Register the terminate signals plus `SIGHUP`. On a background thread, iterate the signals: count each `SIGHUP` as a config reload and print `reload #N`; on any terminate signal, print how many reloads happened and break. Drive it by `raise`-ing two `SIGHUP`s and then a `SIGTERM`.

<details>
<summary>Solution</summary>

```rust
use signal_hook::consts::{SIGHUP, SIGTERM, TERM_SIGNALS};
use signal_hook::iterator::Signals;
use signal_hook::low_level;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut sigs: Vec<i32> = TERM_SIGNALS.to_vec();
    sigs.push(SIGHUP);
    let mut signals = Signals::new(&sigs)?;

    let t = std::thread::spawn(move || {
        let mut reloads = 0u32;
        for signal in &mut signals {
            if signal == SIGHUP {
                reloads += 1;
                println!("reload #{reloads}");
            } else {
                println!("terminating after {reloads} reload(s)");
                break;
            }
        }
    });

    low_level::raise(SIGHUP)?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    low_level::raise(SIGHUP)?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    low_level::raise(SIGTERM)?;

    t.join().unwrap();
    Ok(())
}
```

Output:

```text
reload #1
reload #2
terminating after 2 reload(s)
```

</details>

### Exercise 3 — Channel-driven shutdown

**Difficulty:** Intermediate / Advanced

**Objective:** Combine `ctrlc` with an [mpsc channel](/26-systems-programming/03-channels/) so a blocked main loop wakes the instant a signal arrives, instead of polling a flag.

**Instructions:** Create a `channel::<()>()`. In the `ctrlc` handler, `send(())`. In the main loop, use `rx.recv_timeout(...)`: on `Ok(())` (or a `Disconnected` error) print a shutdown line and break; on `Timeout`, do one unit of work. This is strictly better than spin-polling because `recv_timeout` blocks until *either* a signal arrives or the timeout elapses.

<details>
<summary>Solution</summary>

```rust
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Duration;

fn main() {
    let (tx, rx) = channel::<()>();
    ctrlc::set_handler(move || {
        let _ = tx.send(()); // ignore error if the receiver is already gone
    })
    .expect("error setting handler");

    let mut iterations = 0u64;
    loop {
        match rx.recv_timeout(Duration::from_millis(20)) {
            // A signal was delivered (or the sender was dropped) -> shut down.
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                println!("shutdown signal -> stopping after {iterations} iterations");
                break;
            }
            // No signal yet: do a unit of work and loop.
            Err(RecvTimeoutError::Timeout) => {
                iterations += 1;
                if iterations >= 3 {
                    println!("work complete after {iterations} iterations");
                    break;
                }
            }
        }
    }
}
```

Output (the demo finishes its work before any signal arrives):

```text
work complete after 3 iterations
```

In a real run, pressing Ctrl-C during the loop makes `recv_timeout` return `Ok(())` immediately and prints the `shutdown signal -> stopping after N iterations` branch.

</details>
