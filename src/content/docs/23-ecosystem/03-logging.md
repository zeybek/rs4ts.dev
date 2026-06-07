---
title: "Logging with the `log` Facade and `env_logger`"
description: "Rust splits logging into the log facade (info!, warn!, error!) and a backend like env_logger, with RUST_LOG control. The TypeScript pino/debug parallel."
---

## Quick Overview

In Node you reach for `console.log` first and a real logger (`winston`, `pino`, `debug`) later. In Rust the idiomatic starting point is the **`log` crate**: a lightweight logging *facade* of macros (`info!`, `warn!`, `error!`, `debug!`, `trace!`) that produce records, plus a separate *implementation* crate (most commonly **`env_logger`**) that decides where those records go and which ones are shown. This split is the key idea: your code (and especially your **library** code) depends only on the facade, while the **binary** at the top of the dependency tree picks one logger and configures it.

> **Note:** This page covers the classic `log` + `env_logger` stack: the simplest, most-used option for CLIs and small services. For structured, span-aware logging in async services (the `tracing` ecosystem, JSON logs, request spans), see [Structured Logging and Spans with `tracing`](/23-ecosystem/04-tracing/). The two interoperate, so starting with `log` is never a dead end.

---

## TypeScript/JavaScript Example

A typical Node service mixes raw `console.*` with environment-driven verbosity, usually through a library like `debug` or `pino`:

```typescript
// logger.ts — a hand-rolled level-aware logger, the kind teams write before
// adopting pino/winston.
type Level = "error" | "warn" | "info" | "debug" | "trace";

const ORDER: Level[] = ["error", "warn", "info", "debug", "trace"];

// LOG_LEVEL=debug node app.js   → show error..debug, hide trace
const threshold = (process.env.LOG_LEVEL as Level) ?? "error";
const maxIndex = ORDER.indexOf(threshold);

function log(level: Level, msg: string): void {
  if (ORDER.indexOf(level) <= maxIndex) {
    const sink = level === "error" || level === "warn" ? console.error : console.log;
    sink(`[${new Date().toISOString()}] ${level.toUpperCase()} ${msg}`);
  }
}

export function processOrder(orderId: number, amount: number): void {
  log("info", `processing order ${orderId} for $${amount.toFixed(2)}`);
  if (amount > 10_000) log("warn", `order ${orderId} exceeds review threshold`);
  log("debug", `validating payment for order ${orderId}`);
  if (amount <= 0) {
    log("error", `order ${orderId} has a non-positive amount: ${amount}`);
    return;
  }
  log("info", `order ${orderId} confirmed`);
}
```

Two pain points are visible here: every project re-invents the level filtering, and the *library* code is hard-wired to one concrete `console` sink. Rust's `log` facade removes both problems.

---

## Rust Equivalent

Add the two crates to a binary project:

```bash
cargo add log env_logger
```

```rust
// src/main.rs
use log::{debug, error, info, trace, warn};

fn process_order(order_id: u64, amount: f64) {
    info!("processing order {order_id} for ${amount:.2}");

    if amount > 10_000.0 {
        warn!("order {order_id} exceeds the manual-review threshold");
    }

    debug!("validating payment method for order {order_id}");

    if amount <= 0.0 {
        error!("order {order_id} has a non-positive amount: {amount}");
        return;
    }

    trace!("order {order_id} state transition: NEW -> CONFIRMED");
    info!("order {order_id} confirmed");
}

fn main() {
    // Reads the RUST_LOG environment variable to decide which levels to show.
    env_logger::init();

    info!("service starting up");
    process_order(1001, 49.95);
    process_order(1002, 25_000.0);
    process_order(1003, -5.0);
}
```

`env_logger` reads the **`RUST_LOG`** environment variable (the analogue of Node's `LOG_LEVEL`/`DEBUG`). Running the program at different levels produces real, level-filtered output:

```text
$ cargo run                      # RUST_LOG unset → default level is "error"
[2026-06-01T13:17:12Z ERROR probe] order 1003 has a non-positive amount: -5

$ RUST_LOG=info cargo run
[2026-06-01T13:17:18Z INFO  probe] service starting up
[2026-06-01T13:17:18Z INFO  probe] processing order 1001 for $49.95
[2026-06-01T13:17:18Z INFO  probe] order 1001 confirmed
[2026-06-01T13:17:18Z INFO  probe] processing order 1002 for $25000.00
[2026-06-01T13:17:18Z WARN  probe] order 1002 exceeds the manual-review threshold
[2026-06-01T13:17:18Z INFO  probe] order 1002 confirmed
[2026-06-01T13:17:18Z INFO  probe] processing order 1003 for $-5.00
[2026-06-01T13:17:18Z ERROR probe] order 1003 has a non-positive amount: -5

$ RUST_LOG=trace cargo run
[2026-06-01T13:17:24Z INFO  probe] service starting up
[2026-06-01T13:17:24Z INFO  probe] processing order 1001 for $49.95
[2026-06-01T13:17:24Z DEBUG probe] validating payment method for order 1001
[2026-06-01T13:17:24Z TRACE probe] order 1001 state transition: NEW -> CONFIRMED
[2026-06-01T13:17:24Z INFO  probe] order 1001 confirmed
...
```

No bespoke filtering code, no `Date().toISOString()` plumbing. The level, timestamp, and target column come for free, and `RUST_LOG` controls verbosity without recompiling.

---

## Detailed Explanation

### The facade-and-implementation split

The single most important concept is that **`log` is a facade, not a logger**. The `log` crate defines:

- five macros — `error!`, `warn!`, `info!`, `debug!`, `trace!` — and
- a `Log` trait describing what a "logger" must do.

It contains *no code that prints anything*. By itself, calling `info!(...)` builds a log `Record` and hands it to whatever global logger has been installed. If nothing is installed, the record is discarded.

`env_logger` is one *implementation* of that trait. Calling `env_logger::init()` installs an `env_logger` instance as the process-wide logger. Other implementations exist — `simple_logger`, `fern`, `tracing-log`, `systemd-journal-logger`, and (most importantly) the `tracing` ecosystem's bridge — and you can swap them without touching a single `info!` call.

> **Tip:** This is exactly the dependency-inversion pattern you would hand-roll in TypeScript by injecting a `Logger` interface. Rust formalizes it at the ecosystem level: a library crate adds `log` (~zero cost, no opinions) and the application chooses the backend once.

### Why libraries must only depend on `log`

In Node, a published package that imports `winston` forces *your* logging choice onto every consumer. The Rust convention avoids this: a library depends only on `log`, emits records, and stays silent unless the final binary installs a logger. This is why crates across the ecosystem (`hyper`, `reqwest`, `mio`, and many more) emit `log` records you can switch on with `RUST_LOG`.

### `env_logger::init()` and `RUST_LOG`

`env_logger::init()` parses `RUST_LOG` and installs the logger. The default level when `RUST_LOG` is unset is **`error`** (note how the first run above showed only the `ERROR` line). The directive grammar is richer than a single level, covered under *Levels and Targets* below.

> **Warning:** `init()` may be called **only once** per process. A second call (or two libraries both calling it) returns an error; `env_logger::init()` panics on the resulting `SetLoggerError`, while `try_init()` returns it as a `Result` you can handle. Initialize exactly once, early in `main`.

### Format-string ergonomics

The macros use Rust's standard formatting machinery, so captured identifiers and format specs work directly:

```rust
let order_id = 1001u64;
let amount = 49.95_f64;
log::info!("processing order {order_id} for ${amount:.2}");
```

`{order_id}` captures the local variable inline (no positional `, order_id` needed), and `{amount:.2}` applies the same `:.2` precision spec you would use with `println!` or `format!`. This is the current idiom; avoid the redundant `"{x}", x = x` form.

### Levels are ordered and compile-time-aware

The five levels, from most to least severe, are `Error > Warn > Info > Debug > Trace`. Setting a threshold enables that level **and everything more severe**. Importantly, a disabled log call is *cheap*: the macro checks whether the level/target is enabled before evaluating the message arguments, so `debug!("{}", expensive())` does not call `expensive()` when debug is off. You can also set a compile-time maximum via the `max_level_*` Cargo features of `log` to compile lower levels out entirely in release builds.

---

## Key Differences

| Concern | Node.js | Rust (`log` + `env_logger`) |
| --- | --- | --- |
| Default sink | `console.log` is always there | Nothing prints until a logger is installed |
| Facade vs. backend | Often coupled (`import pino`) | Strictly split: `log` (facade) vs. `env_logger` (backend) |
| Level control | Custom `LOG_LEVEL` or `DEBUG` glob | `RUST_LOG` directive grammar, parsed by the backend |
| Disabled-call cost | You write the `if` guard yourself | Macro short-circuits before formatting args |
| Per-module filtering | `DEBUG=app:db,app:http` globbing | `RUST_LOG=app::db=debug,app::http=warn` by module path |
| Structured fields | First-class in `pino` | Opt-in via `log`'s `kv` feature; richer in `tracing` |
| Output destination | stdout/stderr per call | `env_logger` writes to **stderr** by default |
| When to use | All cases | CLIs, simple services; graduate to `tracing` for async/spans |

The mental-model shift: in Node, logging is a *function you call*; in Rust, logging is a *facade you emit into* and a *backend you install*. The decoupling is the feature.

> **Note:** `env_logger` writes to **stderr** by default, not stdout. That is deliberate: it keeps diagnostic output off your program's actual stdout data stream, which matters for CLIs that pipe results. You can switch it to stdout via the builder (`.target(env_logger::Target::Stdout)`).

---

## Common Pitfalls

### Forgetting to install a logger

The most common surprise: log calls compile fine but produce **no output**, even at `RUST_LOG=trace`, because no logger was installed.

```rust
use log::info;

fn main() {
    // No env_logger::init() — every log call is a silent no-op.
    info!("you will never see this");
    println!("program finished");
}
```

```text
$ RUST_LOG=trace cargo run
program finished
```

This is not an error; it is by design (libraries must stay silent when no backend is present). The fix is one line: call `env_logger::init()` early in `main`. When debugging "why are there no logs," check that the binary installs a logger *before* the first log call.

### Using structured key-values without the `kv` feature

`log`'s structured-field syntax (`info!(user_id = 42; "...")`) is gated behind the `kv` Cargo feature. Forgetting to enable it produces a real, clear compiler error:

```rust
use log::info;

fn main() {
    env_logger::init();
    info!(user_id = 42; "user authenticated"); // does not compile without `kv`
}
```

```text
error: key value support requires the `kv` feature of `log`
 --> src/main.rs:5:5
  |
5 |     info!(user_id = 42; "user authenticated");
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: this error originates in the macro `$crate::__log_key` which comes from the
          expansion of the macro `info` (in Nightly builds, run with -Z macro-backtrace
          for more info)
```

The macro expansion actually reports this error twice — once for the key (`__log_key`) and once for the value (`__log_value`) — so a real build shows two identical messages. Fix it by enabling the feature: `cargo add log --features kv`.

### Expecting `RUST_LOG` to show output by default

Because the default level is `error`, a fresh `cargo run` with `info!`/`debug!` calls looks "broken": only errors appear. Set `RUST_LOG=info` (or configure a different default in the builder, shown below). This trips up developers who assume `console.log`-style "everything prints."

### Calling `init()` twice

If two code paths (or two dependencies) both call `env_logger::init()`, the second call panics with `env_logger::init should not be called after logger initialized`. Prefer `try_init()` and ignore the `Err`, or guarantee a single initialization point. This is the analogue of two libraries both trying to monkey-patch `console`.

### Confusing the level threshold direction

A directive like `RUST_LOG=warn` shows `warn` **and** `error` (more severe), not `warn` and below. Newcomers from systems where a "level" is a numeric verbosity sometimes invert this. More severe is always included.

---

## Best Practices

- **Libraries depend only on `log`.** Never make a reusable crate pull in `env_logger` or any concrete backend; let the application choose. (See [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/) for where `log` sits in the ecosystem.)
- **Initialize exactly once, early in `main`.** Use `env_logger::init()` for the common case, or `try_init()` when another component might also initialize.
- **Set a sensible default level for app users.** Bare `env_logger::init()` defaults to `error`, which is too quiet for most apps. Use the builder with `default_filter_or("info")` so a fresh checkout shows useful output without anyone setting `RUST_LOG`.
- **Use targets to namespace subsystems.** Reach for the module path automatically, or set `target: "audit"` for cross-cutting concerns (covered next).
- **Pick the right level.** `error!` for failures the operator must act on; `warn!` for recoverable anomalies; `info!` for high-level lifecycle events; `debug!` for developer diagnostics; `trace!` for very fine-grained, high-volume detail.
- **Don't pre-format expensive messages.** Pass values to the macro and let it short-circuit; or guard with `log_enabled!(Level::Debug)` for genuinely costly work.
- **Graduate to `tracing` when you need spans, async context, or JSON.** `log` records can be bridged into `tracing` via `tracing-log`, so the migration is incremental. See [Structured Logging and Spans with `tracing`](/23-ecosystem/04-tracing/).

---

## Levels and Targets

### Levels recap

Five levels, ordered `Error > Warn > Info > Debug > Trace`. Selecting one enables it and all more-severe levels.

### Targets: the per-module switch

Every log record carries a **target** string. By default the target is the **module path** where the macro is invoked (e.g. `myapp::billing`), which is what fills the bracketed column in the output. You can override it explicitly with `target:`.

`RUST_LOG` accepts a comma-separated list of `target=level` directives, plus an optional bare global level. This is the precise, hierarchical analogue of Node's `DEBUG=app:db,app:http` globbing, but matched by module path prefix:

```rust
use log::{debug, info};

mod payments {
    use log::info;
    pub fn charge(cents: u64) {
        // Logs under the target "probe::payments" by default.
        info!("charging {cents} cents");
        // Override the target explicitly:
        info!(target: "audit", "PAYMENT amount={cents}");
    }
}

fn main() {
    env_logger::init();
    info!("app started");   // target: "probe"
    debug!("cache warmed"); // target: "probe"
    payments::charge(500);
}
```

Real output under different directives:

```text
$ RUST_LOG="probe::payments=info" cargo run
[2026-06-01T13:17:38Z INFO  probe::payments] charging 500 cents

$ RUST_LOG="warn,probe::payments=debug,audit=info" cargo run
[2026-06-01T13:17:39Z INFO  probe::payments] charging 500 cents
[2026-06-01T13:17:39Z INFO  audit] PAYMENT amount=500

$ RUST_LOG="audit=info" cargo run
[2026-06-01T13:17:40Z INFO  audit] PAYMENT amount=500
```

Notice three things in that output:

1. `probe::payments=info` shows the `payments` module's `info!` but suppresses the top-level `probe` logs entirely (they were not enabled by any directive).
2. The combined directive `warn,probe::payments=debug,audit=info` sets a global floor of `warn`, raises `probe::payments` to `debug`, and enables the custom `audit` target: independent dials on independent subsystems.
3. The `target: "audit"` record appears under `audit`, *not* under `probe::payments`, even though it was emitted from inside that module. The target, not the call site, decides the namespace.

> **Tip:** This is how you turn on verbose logging for a single dependency in production. For example, `RUST_LOG="warn,reqwest=debug,hyper=info"` keeps your app quiet while surfacing HTTP-client internals, because those crates emit `log` records under their own crate-path targets.

### `RUST_LOG` directive cheatsheet

| `RUST_LOG` value | Effect |
| --- | --- |
| *(unset)* | Default level only (`error` for `init()`) |
| `info` | Global level `info` for all targets |
| `myapp=debug` | `myapp` (and its submodules) at `debug`; everything else off |
| `warn,myapp::db=trace` | Global `warn`, but `myapp::db` at `trace` |
| `myapp=info,reqwest=warn` | Two independent target levels |
| `off` | Disable all logging |

### Filtering expensive work with `log_enabled!`

When constructing a message is genuinely expensive (serializing a large structure, walking a graph), guard it so the work happens only when the level is active:

```rust
use log::{info, log_enabled, Level};

fn expensive_summary() -> String {
    // Pretend this is costly to compute.
    "big-report".to_string()
}

fn main() {
    env_logger::init();
    if log_enabled!(Level::Debug) {
        info!("debug summary: {}", expensive_summary());
    }
}
```

For ordinary arguments you do *not* need this guard; the macros already skip argument evaluation when the level is disabled.

### Customizing the output format

`env_logger::Builder` lets the binary configure the default level, honor `RUST_LOG` overrides, and rewrite the line format:

```rust
use log::{info, warn, LevelFilter};
use std::io::Write;

fn main() {
    env_logger::Builder::new()
        // Default level when RUST_LOG is unset.
        .filter_level(LevelFilter::Info)
        // Still let RUST_LOG override the defaults if present.
        .parse_default_env()
        // Custom one-line format: LEVEL target: message
        .format(|buf, record| {
            writeln!(
                buf,
                "{:<5} {}: {}",
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();

    info!("server listening on port 8080");
    warn!("disk usage at 85%");
}
```

```text
$ cargo run                # no RUST_LOG → uses the Info default
INFO  probe: server listening on port 8080
WARN  probe: disk usage at 85%

$ RUST_LOG=warn cargo run  # RUST_LOG overrides the default
WARN  probe: disk usage at 85%
```

---

## Real-World Example

A production-flavored layout: a `billing` module that behaves like a reusable **library** — it emits records through the `log` facade and chooses a `target`, but never installs a logger — while `main` owns logger configuration. The binary sets a useful default level so operators see `info` without configuring anything, while `RUST_LOG` can still dial in per-target detail.

```rust
use log::{info, warn};

/// A library-style module. It only depends on the `log` facade — never on a
/// concrete logger implementation. The binary decides how logs are rendered.
mod billing {
    use log::{debug, error, info, warn};

    #[derive(Debug)]
    pub enum ChargeError {
        InvalidAmount,
    }

    pub fn charge_card(customer: &str, cents: u64) -> Result<u64, ChargeError> {
        info!(target: "billing", "charging customer {customer}: {cents} cents");

        if cents == 0 {
            error!(target: "billing", "refusing zero-amount charge for {customer}");
            return Err(ChargeError::InvalidAmount);
        }
        if cents > 1_000_000 {
            warn!(target: "billing", "large charge for {customer}: {cents} cents");
        }

        let txn_id = 0xABCD;
        debug!(target: "billing", "gateway accepted, txn_id={txn_id:#x}");
        Ok(txn_id)
    }
}

fn main() {
    // Honor RUST_LOG if present, otherwise default to "info" so operators
    // get useful output out of the box.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .init();

    info!("billing worker online");

    for (customer, cents) in [("alice", 4_999u64), ("bob", 0), ("carol", 5_000_000)] {
        match billing::charge_card(customer, cents) {
            Ok(txn) => info!("charge ok for {customer}, txn={txn:#x}"),
            Err(e) => warn!("charge failed for {customer}: {e:?}"),
        }
    }
}
```

Default run (no `RUST_LOG`). Note how the `billing` records carry their own target column and the `debug!` gateway line stays hidden:

```text
$ cargo run
[2026-06-01T13:18:44Z INFO  probe] billing worker online
[2026-06-01T13:18:44Z INFO  billing] charging customer alice: 4999 cents
[2026-06-01T13:18:44Z INFO  probe] charge ok for alice, txn=0xabcd
[2026-06-01T13:18:44Z INFO  billing] charging customer bob: 0 cents
[2026-06-01T13:18:44Z ERROR billing] refusing zero-amount charge for bob
[2026-06-01T13:18:44Z WARN  probe] charge failed for bob: InvalidAmount
[2026-06-01T13:18:44Z INFO  billing] charging customer carol: 5000000 cents
[2026-06-01T13:18:44Z WARN  billing] large charge for carol: 5000000 cents
[2026-06-01T13:18:44Z INFO  probe] charge ok for carol, txn=0xabcd
```

Turning on `billing` diagnostics while quieting everything else to `warn` reveals the previously-hidden `debug!` lines:

```text
$ RUST_LOG="billing=debug,warn" cargo run
[2026-06-01T13:18:34Z INFO  billing] charging customer alice: 4999 cents
[2026-06-01T13:18:34Z DEBUG billing] gateway accepted, txn_id=0xabcd
[2026-06-01T13:18:34Z INFO  billing] charging customer bob: 0 cents
[2026-06-01T13:18:34Z ERROR billing] refusing zero-amount charge for bob
[2026-06-01T13:18:34Z WARN  probe] charge failed for bob: InvalidAmount
[2026-06-01T13:18:34Z INFO  billing] charging customer carol: 5000000 cents
[2026-06-01T13:18:34Z WARN  billing] large charge for carol: 5000000 cents
[2026-06-01T13:18:34Z DEBUG billing] gateway accepted, txn_id=0xabcd
```

This is the everyday operational workflow: ship a quiet binary, then crank up exactly the subsystem you are investigating with a single environment variable: no redeploy, no code change.

---

## Further Reading

- [`log` crate documentation](https://docs.rs/log) — the facade, macros, `Level`/`LevelFilter`, and the `kv` structured-field API.
- [`env_logger` crate documentation](https://docs.rs/env_logger) — the `Builder`, `Env`, target selection, and the full `RUST_LOG` directive grammar.
- [The Rust `log` book / README](https://github.com/rust-lang/log) — the canonical explanation of the facade pattern.
- Within this guide:
  - [Structured Logging and Spans with `tracing`](/23-ecosystem/04-tracing/) — structured logging, spans, `#[instrument]`, and JSON output for async services; the next step up from `log`.
  - [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/) — where `log`/`env_logger` fit among the most-used crates and their npm equivalents.
  - [Async Runtimes](/23-ecosystem/02-async-runtimes/) and [Web Frameworks](/23-ecosystem/01-web-frameworks/) — the contexts where you typically graduate from `log` to `tracing`.
  - [Section 02: Comments and Output](/02-basics/04-output/) — `println!`/`eprintln!` and formatting macros that the log macros build on.
  - [Section 00: Introduction](/00-introduction/) and [Section 01: Getting Started](/01-getting-started/) — Cargo and project setup if `cargo add` is new to you.
  - [Section 24: Tooling](/24-tooling/) — complementary developer tooling.

---

## Exercises

### Exercise 1: Switch on a dependency's logs

**Difficulty:** Beginner

**Objective:** Internalize how `RUST_LOG` targets and the default-`error` level interact.

**Instructions:** Start from the order-processing example in the *Rust Equivalent* section. Without changing any Rust code, find the `RUST_LOG` value that shows the `warn!` and `error!` lines but hides `info!`, `debug!`, and `trace!`. Then find the value that shows everything *except* `trace!`. Explain why a bare `cargo run` (no `RUST_LOG`) prints only the single `ERROR` line.

<details>
<summary>Solution</summary>

```text
# Show warn and error only (warn enables warn + the more-severe error):
$ RUST_LOG=warn cargo run

# Show everything except trace:
$ RUST_LOG=debug cargo run
```

A bare `cargo run` leaves `RUST_LOG` unset, and `env_logger::init()` defaults to the `error` level. Levels are inclusive of more-severe levels only, so the `error` threshold enables just `error!`, which is why only the single non-positive-amount line appears. Setting `RUST_LOG=warn` raises the threshold to include `warn!` and `error!`; `RUST_LOG=debug` includes `error!` through `debug!` but still excludes the less-severe `trace!`.

</details>

### Exercise 2: Map CLI verbosity to a level

**Difficulty:** Intermediate

**Objective:** Configure `env_logger` programmatically instead of relying on `RUST_LOG`, mirroring the common `-v`/`-vv`/`-vvv` flag pattern.

**Instructions:** Write a program that reads a verbosity count (simulate it by reading the first command-line argument as a number) and maps it to a `log::LevelFilter`: `0 → Warn`, `1 → Info`, `2 → Debug`, `3 or more → Trace`. Use `env_logger::Builder` with `.filter_level(...)`. Emit one message at each of the five levels and confirm the filtering changes with the argument.

<details>
<summary>Solution</summary>

```rust
use log::{debug, error, info, trace, warn, LevelFilter};

fn level_from_verbosity(v: u8) -> LevelFilter {
    match v {
        0 => LevelFilter::Warn,
        1 => LevelFilter::Info,
        2 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    }
}

fn emit_all() {
    error!("error msg");
    warn!("warn msg");
    info!("info msg");
    debug!("debug msg");
    trace!("trace msg");
}

fn main() {
    // In a real CLI this count comes from clap's `ArgAction::Count` on `-v`.
    let verbosity: u8 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    env_logger::Builder::new()
        .filter_level(level_from_verbosity(verbosity))
        .init();

    emit_all();
}
```

Real output:

```text
$ cargo run                # verbosity 0 → Warn
[2026-06-01T13:18:55Z ERROR probe] error msg
[2026-06-01T13:18:55Z WARN  probe] warn msg

$ cargo run -- 1           # → Info
[2026-06-01T13:18:56Z ERROR probe] error msg
[2026-06-01T13:18:56Z WARN  probe] warn msg
[2026-06-01T13:18:56Z INFO  probe] info msg

$ cargo run -- 3           # → Trace
[2026-06-01T13:18:56Z ERROR probe] error msg
[2026-06-01T13:18:56Z WARN  probe] warn msg
[2026-06-01T13:18:56Z INFO  probe] info msg
[2026-06-01T13:18:56Z DEBUG probe] debug msg
[2026-06-01T13:18:56Z TRACE probe] trace msg
```

</details>

### Exercise 3: Render structured key-value fields

**Difficulty:** Advanced

**Objective:** Use `log`'s `kv` feature to attach structured fields and a custom `env_logger` format to render them: the precursor to the structured logging you get out of the box with `tracing`.

**Instructions:** Enable the `kv` feature (`cargo add log --features kv`). Emit a log with structured fields, e.g. `info!(user_id = 42, action = "checkout", cart_total = 129.50; "order placed")`. Write a custom `env_logger` format closure that appends each key-value pair as ` key=value` after the message. (Hint: implement `log::kv::VisitSource` to collect the pairs.)

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
log = { version = "0.4", features = ["kv"] }
env_logger = "0.11"
```

```rust
use log::{info, kv::{Key, Value, VisitSource}, LevelFilter};
use std::io::Write;

// A visitor that appends ` key=value` pairs to the formatted line.
struct KvCollector(String);

impl<'kvs> VisitSource<'kvs> for KvCollector {
    fn visit_pair(
        &mut self,
        key: Key<'kvs>,
        value: Value<'kvs>,
    ) -> Result<(), log::kv::Error> {
        self.0.push_str(&format!(" {key}={value}"));
        Ok(())
    }
}

fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format(|buf, record| {
            let mut kvs = KvCollector(String::new());
            record.key_values().visit(&mut kvs).ok();
            writeln!(buf, "{} {}{}", record.level(), record.args(), kvs.0)
        })
        .init();

    info!(user_id = 42, action = "checkout", cart_total = 129.50; "order placed");
}
```

Real output:

```text
$ cargo run
INFO order placed user_id=42 action=checkout cart_total=129.5
```

This works, but notice how much manual machinery it takes to render structured data well. That is precisely the gap the `tracing` ecosystem fills with first-class structured fields, spans, and JSON formatters — see [Structured Logging and Spans with `tracing`](/23-ecosystem/04-tracing/).

</details>
