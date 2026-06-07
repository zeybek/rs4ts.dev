---
title: "Common Migration Challenges"
description: "The real walls porting Node.js to Rust: the ownership learning curve, crate-ecosystem gaps, team ramp-up, and knowing when not to migrate at all."
---

Moving a codebase from Node.js to Rust is rarely blocked by syntax. The hard parts are the **ownership learning curve**, **gaps in the crate ecosystem**, **getting a team productive**, and — most underrated — **recognizing when migrating is the wrong call**. This page is the honest counterweight to the rest of Section 29: the other topics show you *how* to migrate; this one shows you what tends to go wrong and how to avoid burning a quarter on it.

---

## Quick Overview

The biggest obstacles in a Node-to-Rust migration are organizational and conceptual, not technical. The borrow checker forces TypeScript developers to make ownership decisions the JavaScript runtime used to make for them (via the garbage collector), some libraries you took for granted in npm have no mature Rust equivalent, and a team that was shipping daily in TypeScript will slow down before it speeds up. Knowing these costs up front — and knowing the cases where you should *not* migrate at all — is what separates a successful rewrite from a cautionary blog post.

---

## TypeScript/JavaScript Example

Here is a pattern that is completely normal in a Node service: a module-level mutable cache that any function in the module can read and write, plus a couple of callbacks that close over the same array. The JavaScript runtime allows unrestricted aliasing — multiple references to the same mutable object — because the garbage collector and the single-threaded event loop hide the consequences.

```typescript
// metrics.ts — idiomatic Node.js
const counts: Record<string, number> = {};

export function recordHit(route: string): void {
  counts[route] = (counts[route] ?? 0) + 1;
}

// Two independent callbacks both mutate the same shared array.
const events: string[] = [];
const logInfo = () => events.push("info");
const logWarn = () => events.push("warn");

logInfo();
logWarn();

export function snapshot() {
  return { counts: { ...counts }, events: [...events] };
}
```

Nothing here is wrong in JavaScript. `counts` and `events` are freely shared, mutated from anywhere, and captured by as many closures as you like. The runtime tracks lifetimes for you, and because Node runs your code on one thread, there are no data races to worry about. When a TypeScript developer ports this to Rust, the instinct is to write the same shape, and that is exactly where the borrow checker pushes back.

---

## Rust Equivalent

The same idea — a shared counter that any code path can update — compiles cleanly in Rust once you make ownership explicit. A `&mut HashMap` passed into a function is the direct analogue of "mutate the shared map," and it is perfectly idiomatic:

```rust
use std::collections::HashMap;

// A direct port of the JS module-level mutable cache.
// In JS you'd just `counts[key] = (counts[key] ?? 0) + 1` from anywhere.
fn record_hit(cache: &mut HashMap<String, u64>, key: &str) {
    *cache.entry(key.to_string()).or_insert(0) += 1;
}

fn main() {
    let mut cache: HashMap<String, u64> = HashMap::new();
    record_hit(&mut cache, "/users");
    record_hit(&mut cache, "/users");
    record_hit(&mut cache, "/orders");

    let mut entries: Vec<_> = cache.iter().collect();
    entries.sort();
    for (route, hits) in entries {
        println!("{route}: {hits}");
    }
}
```

Output:

```
/orders: 1
/users: 2
```

The friction appears when you try to replicate the *two callbacks closing over the same array* part. That is the first real wall most TypeScript developers hit, and the next section breaks it down.

---

## Detailed Explanation

### The ownership learning curve

In JavaScript, a closure captures a variable by *reference to a shared, garbage-collected slot*. Two closures can capture the same array and both mutate it; the runtime keeps the array alive as long as either closure exists. Rust does not have a garbage collector, so it enforces a single rule at compile time: you may have **either** many shared (`&`) borrows **or** exactly one mutable (`&mut`) borrow of a value at a time, never both. Two closures that each capture the array `&mut` violate that rule.

Here is the naive port, and it does **not** compile:

```rust
fn main() {
    let mut events: Vec<String> = Vec::new();

    // In JS you'd freely close over `events` in two callbacks.
    let mut log_info = || events.push("info".to_string()); // does not compile (error[E0499])
    let mut log_warn = || events.push("warn".to_string()); // second &mut borrow

    log_info();
    log_warn();

    println!("{:?}", events);
}
```

The real compiler error is:

```
error[E0499]: cannot borrow `events` as mutable more than once at a time
 --> src/main.rs:6:24
  |
5 |     let mut log_info = || events.push("info".to_string());
  |                        -- ------ first borrow occurs due to use of `events` in closure
  |                        |
  |                        first mutable borrow occurs here
6 |     let mut log_warn = || events.push("warn".to_string());
  |                        ^^ ------ second borrow occurs due to use of `events` in closure
  |                        |
  |                        second mutable borrow occurs here
7 |
8 |     log_info();
  |     -------- first borrow later used here
```

This message is not a bug; it is the compiler telling you that the JavaScript ownership model does not transfer directly. There are three idiomatic ways out, in increasing order of cost:

1. **Don't alias.** If you can restructure so that only one thing mutates the value, do that. One closure that takes the level as a parameter replaces two closures that each capture the vector:

   ```rust
   fn main() {
       let mut events: Vec<String> = Vec::new();

       // Each call borrows, runs, and releases before the next one starts.
       let mut log = |level: &str| events.push(level.to_string());
       log("info");
       log("warn");

       println!("{:?}", events);
   }
   ```

   Output:

   ```
   ["info", "warn"]
   ```

2. **Share within one thread** with `Rc<RefCell<T>>`: reference-counted ownership plus a runtime-checked borrow. This is the closest analogue to "two callbacks share one object" and is covered in depth in [Reference Counting with `Rc<T>` and `Arc<T>`](/05-ownership/07-reference-counting/).

3. **Share across threads** with `Arc<Mutex<T>>`: atomically reference-counted, lock-guarded. This is what you reach for when a Node single-threaded shared `Map` becomes a multi-threaded Rust service (see the Real-World Example below).

The point is not that Rust is harder — it is that Rust asks you to *choose* the sharing strategy that Node chose for you implicitly. For senior TypeScript developers, the learning curve is overwhelmingly about ownership, lifetimes, and these three patterns. Plan for **four to eight weeks** before a strong TypeScript developer stops fighting the borrow checker and starts using it as a design tool. Ground that ramp-up in [Section 05: Ownership](/05-ownership/). It is the highest-leverage section for a migrating team.

### Where the JavaScript mental model breaks down

| JavaScript assumption | Rust reality | Migration consequence |
| --- | --- | --- |
| GC keeps anything alive as long as it's referenced | You decide who owns each value; lifetimes are checked at compile time | You spend early effort modeling ownership, not writing features |
| Any number of references can mutate the same object | One `&mut` xor many `&` at a time | Aliased-mutation patterns must be redesigned, not transliterated |
| One thread, so no data races | Compiler *forbids* unsynchronized shared mutation across threads | "Just add a worker thread" requires `Arc`/`Mutex`/channels |
| `async` functions are eager Promises that start immediately | Futures are **lazy** and do nothing until polled by a runtime | You must pick and start a runtime (Tokio); forgetting `.await` is a no-op, not a pending Promise |
| `npm i anything` — a package exists for everything | Some niche needs have no mature crate | Audit dependencies *before* committing to a rewrite |

> **Note:** The async row trips up even strong developers. A JavaScript `Promise` begins running the moment it is created; a Rust `Future` is inert until a runtime polls it. This is the opposite default, and it means "I called the function but nothing happened" is an expected early mistake. See [common patterns](/22-common-patterns/) and your chosen runtime's docs.

### Ecosystem gaps

The npm registry has roughly an order of magnitude more packages than crates.io. For the common backend stack the Rust ecosystem is excellent and current: `serde`/`serde_json` for serialization, `tokio` for async I/O, `axum` for HTTP, `sqlx` and `diesel` for databases, `reqwest` for HTTP clients, `tracing` for structured logs. But gaps are real and you must check *before* you plan a migration, not after. Run `cargo add <crate> --dry-run` in a scratch project to resolve the current version and confirm the crate exists and is maintained:

```toml
# Current, maintained equivalents for a typical Node backend (resolve versions with `cargo add`)
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
axum = "0.8"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres"] }
reqwest = { version = "0.13", features = ["json"] }
tracing = "0.1"
```

Where gaps still bite teams in 2026:

- **Vendor SDKs.** Many SaaS vendors ship first-class Node SDKs and only a community-maintained (or no) Rust crate. You may end up calling the vendor's REST API directly with `reqwest`, or keeping that one integration in a Node sidecar.
- **Highly dynamic / reflection-heavy libraries.** Tools that lean on JavaScript's runtime dynamism (some ORMs, schema-from-shape validators, certain plugin systems) have no clean Rust analogue because Rust monomorphizes generics and erases nothing at runtime the way TypeScript types are erased.
- **Browser/DOM-adjacent tooling** belongs in WebAssembly territory, not a server rewrite.

The mitigation is mechanical: list every production dependency, find each one's Rust equivalent (or decide it stays in Node), and only then estimate the migration. A single missing SDK can be the difference between a clean cut-over and an indefinite hybrid deployment, which is exactly why incremental, service-by-service migration (see [Incremental Migration](/29-migration-guide/00-incremental/)) is usually safer than a big-bang rewrite.

### Team ramp-up

Three things consistently determine how fast a team becomes productive:

1. **Lean on the compiler, not memorization.** The Rust compiler's error messages are unusually good. Teach the team to read them slowly (they often contain the fix verbatim) rather than guessing. Run `cargo clippy` from day one; its lints encode idioms that would otherwise take months to learn.
2. **Pair on the first real service.** The borrow checker is best learned on a concrete problem with someone who has already internalized ownership. A worked walkthrough like [Porting a Node.js Service to Rust](/29-migration-guide/01-node-to-rust/) is a good template for the first port.
3. **Set expectations honestly.** Velocity dips for the first month or two and then recovers. If leadership expects same-day parity, the migration will be judged a failure during precisely the period when it always looks worst.

### When NOT to migrate

Rust is not always the right answer. Do **not** migrate when:

- **The bottleneck is I/O or the database, not CPU.** If your service spends its time waiting on Postgres or downstream APIs, rewriting the glue code in Rust will not move your p99 latency. Profile first (see [Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/)); a query index or a connection-pool fix may give you the win for a fraction of the cost.
- **The code changes constantly and is shipped by a small team.** Early-stage product code that gets rewritten every sprint benefits from TypeScript's iteration speed. Rust's compile-time guarantees pay off on code that must be *correct and stable*, not code that must be *fast to change*.
- **You need a deep, Node-only SDK** with no maintained Rust equivalent, and a sidecar is not acceptable.
- **The motivation is résumé-driven or hype-driven.** "Rewrite it in Rust" is not a strategy. There must be a measurable problem — CPU cost, memory footprint, latency tail, correctness class — that Rust specifically addresses.

Good reasons to migrate: a genuinely CPU-bound hot path, a need to cut memory/instance count, a desire to eliminate a whole class of runtime errors at compile time, or shipping a single static binary. Match the reason to the cost, and migrate the *part* of the system that has the problem, not the whole thing by default.

---

## Key Differences

| Challenge | Node.js / TypeScript | Rust | Why it matters during migration |
| --- | --- | --- | --- |
| Memory model | Garbage collector tracks lifetimes | Ownership + borrowing, checked at compile time | The #1 learning cost; budget weeks, not days |
| Shared mutable state | Free aliasing on one thread | `&mut` xor `&`; share via `Rc`/`RefCell` or `Arc`/`Mutex` | Aliased-mutation code must be redesigned |
| Concurrency | One event-loop thread | Real threads; data races are compile errors | Adding parallelism needs explicit synchronization |
| Async semantics | Eager Promises | Lazy futures + a runtime | Forgetting `.await` is a silent no-op |
| Library breadth | npm (vast) | crates.io (excellent core, narrower edges) | Audit dependencies before committing |
| Iteration speed | Edit-save-reload | Compile-check cycle (caught by the compiler) | Velocity dips before it recovers |
| Generics at runtime | Types erased | Monomorphized | Reflection-style libraries don't port |

The unifying theme: Rust moves work from *runtime* (where Node hides it behind the GC and the event loop) to *compile time* (where you must make it explicit). That is the source of both the friction and the payoff.

---

## Common Pitfalls

### Pitfall 1: Transliterating aliased mutation

The most common early mistake is porting JavaScript's "two things mutate the same object" pattern verbatim, producing the `error[E0499]` shown earlier. The fix is not to fight the borrow checker but to choose a sharing strategy: restructure to avoid aliasing, or move to `Rc<RefCell<T>>` (single thread) / `Arc<Mutex<T>>` (multi-thread). Treat `E0499` and its sibling `E0502` ("cannot borrow as immutable because it is also borrowed as mutable") as design prompts, not obstacles.

### Pitfall 2: Cloning everything to silence the borrow checker

When borrows get hard, the tempting escape hatch is to `.clone()` liberally. It compiles, but you have thrown away one of Rust's main advantages and may end up *slower* than the Node version you replaced. Clone deliberately, not reflexively. If you find yourself cloning a large structure on a hot path, that is a signal to rethink ownership (pass a `&` reference, or restructure who owns the data), not to keep cloning.

### Pitfall 3: Assuming an npm package has a drop-in crate

Planning a migration on the assumption that "there's a crate for that" and discovering mid-sprint that a critical vendor SDK has no maintained equivalent is a schedule-killer. Resolve every dependency with `cargo add <crate> --dry-run` *during planning*. For example, these all resolve to current, maintained crates today, but you only know that because you checked, not because you assumed.

### Pitfall 4: Expecting async to behave like Promises

A TypeScript developer writes an `async fn`, calls it, and is surprised nothing runs. Unlike a Promise, a Rust future does nothing until it is `.await`ed inside a running runtime (e.g. Tokio). Forgetting `.await` does not leave a pending Promise; it leaves an unused value, and the compiler will warn `unused implementer of Future that must be used`. Internalize "futures are lazy" early.

### Pitfall 5: Migrating the wrong thing

Rewriting an I/O-bound service in Rust and then being disappointed that latency did not improve. If the service waits on the database 95% of the time, Rust changes nothing meaningful. Profile first; migrate the CPU-bound piece, not the part that happens to be written in the language you want to leave.

---

## Best Practices

- **Audit dependencies before you commit to a migration.** List every production npm package, map it to a crate (or a "stays in Node" decision), and resolve versions with `cargo add --dry-run`.
- **Migrate incrementally and measure honestly.** Use the strangler-fig approach from [Incremental Migration](/29-migration-guide/00-incremental/), keep [API compatibility](/29-migration-guide/02-api-compatibility/), and validate the wins with the methodology in [Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/). Never report a speedup you did not benchmark.
- **Teach ownership first, syntax second.** Run [Section 05: Ownership](/05-ownership/) as onboarding. The syntax is the easy part for a senior TypeScript developer; the model is the work.
- **Turn on `clippy` and `rustfmt` from commit one.** Let the tooling encode idioms so reviewers don't have to.
- **Clone with intent.** A `.clone()` is fine to unblock yourself, but flag it for review; reflexive cloning erodes the reason you migrated.
- **Write down why you're migrating, in measurable terms.** "Cut p99 from 180 ms to under 50 ms" or "halve instance count" is a goal you can verify. "It'll be more modern" is not.

> **Tip:** Keep a running "ownership log" in your first migrated service — a short note each time the borrow checker forced a redesign and what the fix was. After a few weeks it becomes the most useful onboarding doc your team has, because it is written in your codebase's vocabulary.

---

## Real-World Example

A Node service keeps a single in-memory request counter: one shared `Map` for the whole process, mutated from every request handler. That works because Node is single-threaded. The Rust equivalent that actually uses multiple OS threads must make the sharing explicit with `Arc<Mutex<T>>`: `Arc` gives every thread a counted owner of the same allocation, and `Mutex` guarantees only one thread mutates at a time. The compiler will not let you forget the lock.

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

/// A request-counter shared by many worker threads, the way a Node
/// process keeps a single in-memory `Map` for the whole event loop.
#[derive(Default)]
struct Metrics {
    counts: HashMap<String, u64>,
}

impl Metrics {
    fn record(&mut self, route: &str) {
        *self.counts.entry(route.to_string()).or_insert(0) += 1;
    }
}

fn main() {
    let metrics = Arc::new(Mutex::new(Metrics::default()));

    let mut handles = Vec::new();
    for worker in 0..4 {
        let metrics = Arc::clone(&metrics); // each thread gets its own counted handle
        handles.push(thread::spawn(move || {
            let route = if worker % 2 == 0 { "/users" } else { "/orders" };
            for _ in 0..25 {
                metrics.lock().unwrap().record(route);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let metrics = metrics.lock().unwrap();
    let mut rows: Vec<_> = metrics.counts.iter().collect();
    rows.sort();
    for (route, hits) in rows {
        println!("{route}: {hits}");
    }
}
```

Output:

```
/orders: 50
/users: 50
```

The takeaway for a migrating team: the Node version had this concurrency "for free" because there was no concurrency: one thread did everything. The Rust version is genuinely parallel, and the handful of extra keystrokes for `Arc`, `Mutex`, `.lock()`, and `Arc::clone` are the compiler making you pay, once and visibly, for the data-race safety that Node simply could not offer. That trade — explicit synchronization in exchange for fearless parallelism — is the migration in miniature.

---

## Further Reading

### Official Documentation

- [The Rust Book — Understanding Ownership](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html)
- [The Rust Book — Shared-State Concurrency](https://doc.rust-lang.org/book/ch16-03-shared-state.html)
- [The Rust Book — `Rc<T>` and Reference Counting](https://doc.rust-lang.org/book/ch15-04-rc.html)
- [The Cargo Book — `cargo add`](https://doc.rust-lang.org/cargo/commands/cargo-add.html)
- [crates.io](https://crates.io/): search for and vet crate equivalents

### Related Topics

- [Incremental migration](/29-migration-guide/00-incremental/): the strangler-fig approach that limits the blast radius of these challenges
- [Node.js service walkthrough](/29-migration-guide/01-node-to-rust/): a concrete first port to pair on
- [Maintaining API compatibility](/29-migration-guide/02-api-compatibility/): keeping clients happy mid-migration
- [Measuring performance honestly](/29-migration-guide/04-performance-gains/): deciding whether a migration was worth it
- [Section 05: Ownership](/05-ownership/): the highest-leverage section for ramp-up
- [Reference counting (`Rc`/`Arc`)](/05-ownership/07-reference-counting/): sharing without a garbage collector
- [Why Rust](/01-getting-started/00-why-rust/) and [Basics: Variables](/02-basics/00-variables/): foundations for the migrating team
- [Section 30: Projects](/30-projects/): apply these lessons on a full build

---

## Exercises

### Exercise 1: Read the borrow-checker error

**Difficulty:** Beginner

**Objective:** Build the instinct of reading `error[E0499]` as a design prompt rather than an obstacle.

**Instructions:** The following code is a direct port of a JavaScript "two callbacks share one array" pattern. It does not compile. Without using `Rc`, `RefCell`, `Arc`, or `clone`, change it so it compiles and prints `["info", "warn"]`.

```rust
fn main() {
    let mut events: Vec<String> = Vec::new();

    let mut log_info = || events.push("info".to_string()); // does not compile (error[E0499])
    let mut log_warn = || events.push("warn".to_string());

    log_info();
    log_warn();

    println!("{:?}", events);
}
```

<details>
<summary>Solution</summary>

Collapse the two aliasing closures into one closure that takes the level as a parameter, so each call borrows and releases `events` in turn:

```rust
fn main() {
    let mut events: Vec<String> = Vec::new();

    // One closure, parameterized — borrows and releases on each call.
    let mut log = |level: &str| events.push(level.to_string());
    log("info");
    log("warn");

    println!("{:?}", events);
}
```

Output:

```
["info", "warn"]
```

</details>

### Exercise 2: Share state across threads

**Difficulty:** Intermediate

**Objective:** Convert a single-threaded shared counter into a thread-safe one using `Arc<Mutex<T>>`.

**Instructions:** Start from a single-threaded counter and make it correctly increment from three threads, 10 times each, then print `total: 30`.

```rust
use std::collections::HashMap;

fn main() {
    let mut counts: HashMap<String, u64> = HashMap::new();
    // TODO: increment counts["total"] ten times from each of three threads.
    *counts.entry("total".to_string()).or_insert(0) += 1;
    println!("total: {}", counts["total"]);
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    let counts: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));

    let mut handles = Vec::new();
    for _ in 0..3 {
        let counts = Arc::clone(&counts);
        handles.push(thread::spawn(move || {
            for _ in 0..10 {
                *counts.lock().unwrap().entry("total".to_string()).or_insert(0) += 1;
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    println!("total: {}", counts.lock().unwrap()["total"]);
}
```

Output:

```
total: 30
```

</details>

### Exercise 3: Make the migration decision

**Difficulty:** Advanced (analysis, no code)

**Objective:** Practice deciding *whether* to migrate, which is the most valuable skill in this section.

**Instructions:** For each service below, decide migrate-now, migrate-later, or do-not-migrate, and justify it in one or two sentences using the criteria from this page.

1. A JSON transformation service that is pinned at 100% CPU and is the slowest hop in your request path.
2. A thin REST wrapper around a third-party SaaS whose only maintained SDK is for Node, spending 90% of its time awaiting that vendor's API.
3. A six-week-old internal admin tool, owned by one engineer, whose feature set changes every sprint.
4. A billing-calculation library where a single rounding or null-handling bug has caused two production incidents this year.

<details>
<summary>Solution</summary>

1. **Migrate (good candidate).** CPU-bound and on the hot path: exactly where Rust's performance and lack of GC pauses pay off. Benchmark first (see [Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/)) to set a measurable target, then migrate this service in isolation.
2. **Do not migrate (or keep as a Node sidecar).** It is I/O-bound — the latency lives in the vendor's API, not your code — and the only SDK is Node-only. Rewriting it gains nothing and *loses* the maintained SDK. This is the textbook "rewriting won't move p99" case plus an ecosystem gap.
3. **Migrate later, if ever.** Young, single-owner code that changes weekly benefits from TypeScript's iteration speed; Rust's compile-time guarantees pay off on stable code, not churn. Revisit only if it stabilizes and develops a real performance or correctness problem.
4. **Strong migrate candidate for a correctness reason, not a speed reason.** Rust's type system (no `null`, exhaustive `match`, explicit error handling) can eliminate whole classes of the bugs causing those incidents. The motivation is measurable ("no more rounding/null incidents"), which is exactly the kind of justification that makes a migration worth it.

</details>
