---
title: "Test Fixtures: Setup, Teardown, and Shared State"
description: "Rust has no beforeEach/afterEach. Build fixtures with helper functions, Drop for RAII teardown, and LazyLock for shared state, mapped from Jest and Vitest habits."
---

In Jest or Vitest you reach for `beforeEach`, `afterEach`, and shared module-level constants to prepare the world your tests run in. Rust has **no lifecycle hooks**. Instead it gives you plain functions, the `Drop` trait, and a couple of lazy-initialization types. This topic maps each fixture habit you already have onto its idiomatic Rust counterpart.

---

## Quick Overview

A **fixture** is the prepared state a test runs against: a seeded database, a temp directory, a configured client, a couple of sample records. Rust has no `beforeEach`/`afterEach` decorators, so you build fixtures with ordinary **helper functions** (setup), the **`Drop` trait** (RAII teardown that runs even on panic), and **`LazyLock`** / the **`once_cell`** crate for read-only state computed once and shared across tests. The mental shift for a TypeScript developer is that teardown is tied to a value's *scope*, not to a hook the runner calls for you.

---

## TypeScript/JavaScript Example

```typescript
// user-store.test.ts (Vitest / Jest — the API is nearly identical)
import { beforeEach, afterEach, describe, expect, it } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

interface User {
  id: number;
  name: string;
  email: string;
  active: boolean;
}

// A helper constructor: sensible defaults, override what matters.
function makeUser(overrides: Partial<User> = {}): User {
  return {
    id: 1,
    name: "Alice",
    email: "alice@example.com",
    active: true,
    ...overrides,
  };
}

// Read-only data computed once and shared by every test in the file.
const TEST_CONFIG = { baseUrl: "http://localhost:0", timeoutMs: 250 };

describe("temp-dir fixture", () => {
  let dir: string;

  beforeEach(() => {
    // SETUP: runs before every test
    dir = mkdtempSync(join(tmpdir(), "store-"));
  });

  afterEach(() => {
    // TEARDOWN: runs after every test, even if it threw
    rmSync(dir, { recursive: true, force: true });
  });

  it("writes and reads a file", () => {
    const file = join(dir, "note.txt");
    writeFileSync(file, "persisted");
    expect(readFileSync(file, "utf8")).toBe("persisted");
  });

  it("uses the shared config", () => {
    expect(TEST_CONFIG.timeoutMs).toBe(250);
  });

  it("builds a customized user", () => {
    const u = makeUser({ id: 42, active: false });
    expect(u.name).toBe("Alice"); // default kept
    expect(u.active).toBe(false); // override applied
  });
});
```

**Key points:**

- `beforeEach`/`afterEach` are **lifecycle hooks** the test runner calls for you.
- `afterEach` is where you clean up; the runner guarantees it runs even when the test body throws.
- `makeUser(overrides)` is a **factory** that fills in defaults: the workhorse fixture pattern.
- `TEST_CONFIG` is a module constant, evaluated once when the file is imported.

---

## Rust Equivalent

```rust
// src/lib.rs — the code under test
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u32,
    pub name: String,
    pub email: String,
    pub active: bool,
}

impl User {
    /// A test-friendly constructor: sensible defaults, override what matters.
    pub fn new(id: u32, name: &str) -> Self {
        User {
            id,
            name: name.to_string(),
            email: format!("{}@example.com", name.to_lowercase()),
            active: true,
        }
    }
}

#[derive(Debug, Default)]
pub struct UserStore {
    users: HashMap<u32, User>,
}

impl UserStore {
    pub fn new() -> Self {
        UserStore { users: HashMap::new() }
    }
    pub fn insert(&mut self, user: User) {
        self.users.insert(user.id, user);
    }
    pub fn get(&self, id: u32) -> Option<&User> {
        self.users.get(&id)
    }
    pub fn active_count(&self) -> usize {
        self.users.values().filter(|u| u.active).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SETUP as a plain helper function — call it from each test that needs it.
    fn store_with_two_users() -> UserStore {
        let mut store = UserStore::new();
        store.insert(User::new(1, "Alice"));
        store.insert(User::new(2, "Bob"));
        store
    }

    #[test]
    fn finds_an_existing_user() {
        let store = store_with_two_users();
        let alice = store.get(1).expect("user 1 should exist");
        assert_eq!(alice.name, "Alice");
        assert_eq!(alice.email, "alice@example.com");
    }

    #[test]
    fn counts_active_users() {
        let mut store = store_with_two_users();
        let mut carol = User::new(3, "Carol");
        carol.active = false;
        store.insert(carol);
        assert_eq!(store.active_count(), 2);
    }

    #[test]
    fn override_a_single_field() {
        // Start from the constructor, tweak one field — the `makeUser` pattern.
        let mut user = User::new(42, "Dave");
        user.email = "dave@corp.internal".to_string();
        assert_eq!(user.id, 42);
        assert!(user.active);
    }
}
```

Running it produces real output (here, the helper-function tests plus the rest of this topic's examples in one crate):

```text
running 12 tests
test more::tests::appends_to_shared_log ... ok
test shared::tests::reads_once_cell_lazy ... ok
test shared::tests::reads_shared_config_a ... ok
test shared::tests::reads_shared_config_b ... ok
test more::tests::env_var_is_restored_after_the_test ... ok
test tests::counts_active_users ... ok
test tests::finds_an_existing_user ... ok
test tests::override_a_single_field ... ok
test realworld::tests::parses_explicit_values ... ok
test realworld::tests::falls_back_to_defaults ... ok
test more::tests::teardown_runs_even_when_the_test_panics - should panic ... ok
test shared::tests::writes_and_reads_inside_a_temp_dir ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

> **Note:** This uses the current stable toolchain: Rust 1.96.0 on the 2024 edition. `cargo new` selects the newest edition automatically, so the snippets here run as-is. `LazyLock` (used below) has been part of the standard library since Rust 1.80.

---

## Detailed Explanation

### Setup is a function, not a hook

There is no `#[before_each]` attribute. The idiomatic replacement is a **helper function** that builds and returns the fixture:

```rust
fn store_with_two_users() -> UserStore { /* ... */ }
```

Each test calls it explicitly: `let store = store_with_two_users();`. This is *more* verbose than `beforeEach` by one line per test, but it is also more honest: you can see exactly which fixture each test uses, and a test that needs a different setup just calls a different helper. There is no hidden, file-wide `beforeEach` quietly running before every case.

Because each `#[test]` gets a fresh call to the helper, you also get **fresh state per test for free**, the equivalent of `beforeEach` (not `beforeAll`). Rust runs tests in **parallel threads by default**, so isolated per-test state is required for correctness, not just convenient (more on this under Common Pitfalls).

### Helper constructors replace `makeUser(overrides)`

TypeScript's spread-with-defaults trick (`{ ...defaults, ...overrides }`) has no direct syntax twin, but Rust offers two idioms:

1. **A constructor with the common arguments**, like `User::new(id, name)`, then mutate the one or two fields a test cares about.
2. **Struct update syntax** (`..`) when you have a default value to start from:

```rust
#[derive(Debug, Clone, PartialEq)]
struct User {
    id: u32,
    name: String,
    email: String,
    active: bool,
}

impl Default for User {
    fn default() -> Self {
        User {
            id: 1,
            name: "Alice".into(),
            email: "alice@example.com".into(),
            active: true,
        }
    }
}

fn main() {
    // The closest analog to `makeUser({ id: 42, active: false })`:
    let u = User { id: 42, active: false, ..User::default() };
    println!("{u:?}");
}
```

The `..User::default()` fills in every field you did not name — the same role as `...defaults` in a TypeScript object literal.

### `Drop` is your `afterEach`

Rust ties cleanup to a value's **scope** instead of to a runner hook. When a value goes out of scope, Rust calls its `drop` method (if it has one). Implement `Drop` and the cleanup runs automatically at the end of the test, and, importantly, **during unwinding if the test panics** (an `assert!` failure panics). That is exactly the guarantee `afterEach` gives you, but enforced by the language rather than the runner:

```rust
struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    fn new(label: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        path.push(format!("rs4ts_test_{label}_{}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(TempDir { path }) // SETUP
    }
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // TEARDOWN — runs at end of scope, even on panic.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn demo() -> std::io::Result<()> {
    let dir = TempDir::new("rw")?; // setup
    let file = dir.path().join("note.txt");
    std::fs::write(&file, b"persisted")?;
    assert_eq!(std::fs::read_to_string(&file)?, "persisted");
    Ok(())
    // `dir` drops here -> directory removed. No explicit cleanup call.
}
```

This `Drop`-on-scope-exit pattern is called **RAII** (Resource Acquisition Is Initialization): acquiring the resource and binding its lifetime are the same act. You hold the guard in a `let` binding; when the binding dies, the resource is released.

### `LazyLock` and `once_cell` for shared, read-only state

TypeScript's module-level `const TEST_CONFIG = ...` runs once and is visible everywhere in the file. Rust `static` items must be initialized with a *constant expression*, so you cannot write `static CONFIG: HashMap<...> = { ... }` with runtime work. The fix is a lazily-initialized static, initialized the first time it is touched, then shared:

```rust
use std::collections::HashMap;
use std::sync::LazyLock;

// Computed once, on first access; shared across every test. Read-only.
static TEST_CONFIG: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("base_url", "http://localhost:0");
    m.insert("timeout_ms", "250");
    m
});

fn main() {
    assert_eq!(TEST_CONFIG["base_url"], "http://localhost:0");
    assert_eq!(TEST_CONFIG["timeout_ms"], "250");
}
```

`LazyLock<T>` is thread-safe: even though tests run on many threads, the closure runs **exactly once** and all threads see the same value. This is the *shared-across-tests* behavior of a module constant, plus the parallel-safety Rust's test harness needs.

Before `LazyLock` was stabilized, the community crate **`once_cell`** filled this role, and you will still see it everywhere. Its `Lazy<T>` type is a drop-in equivalent:

```toml
# Cargo.toml
[dependencies]
once_cell = "1"
```

```rust
use once_cell::sync::Lazy;

static GREETING: Lazy<String> = Lazy::new(|| format!("hello, {}", "world"));

fn main() {
    assert_eq!(&*GREETING, "hello, world");
}
```

> **Tip:** On a current toolchain, prefer the standard library's `LazyLock` for new code; no dependency required. Reach for `once_cell` only when you must support a Rust version older than 1.80, or when you need its richer API (such as `OnceCell`/`Lazy` in non-`'static` positions).

---

## Key Differences

| Concern | Jest / Vitest | Rust |
| --- | --- | --- |
| Per-test setup | `beforeEach(fn)` | Call a helper function from each test |
| One-time setup | `beforeAll(fn)` | `LazyLock` / `once_cell` static (lazy, once) |
| Teardown | `afterEach(fn)` | `impl Drop` on a guard value (RAII) |
| Teardown on failure | Runner guarantees `afterEach` runs | Language runs `drop` during unwinding |
| Default-with-overrides | `{ ...defaults, ...overrides }` | Constructor + mutate, or `..Default::default()` |
| Shared module constant | top-level `const` | `static` + `LazyLock` (runtime init needs laziness) |
| Test isolation | Opt-in; files isolated, cases share module scope | Each `#[test]` is a fresh function; parallel by default |
| Ordering of cleanups | Registration order, reversed | Reverse declaration order of `let` bindings |

A few of these deserve emphasis:

- **No magic ordering.** With `beforeEach` the runner decides when your setup fires. In Rust the setup runs exactly where you call it, and teardown runs exactly when the binding goes out of scope — in **reverse order** of declaration, like a stack. This is predictable and visible in the source.
- **Teardown can fail loudly or be ignored.** `Drop::drop` returns `()` and cannot return a `Result`. So you typically *ignore* cleanup errors (`let _ = std::fs::remove_dir_all(...)`), because panicking inside `drop` during an already-unwinding panic aborts the whole process. Jest's `afterEach` can `throw` and surface a second failure; Rust cannot, by design.
- **Parallelism is the default, not an option.** Vitest isolates files but runs cases within a file sequentially. Rust runs `#[test]` functions across a thread pool. Fixtures that touch shared, *mutable* global state must synchronize (see Pitfalls).

---

## Common Pitfalls

### Pitfall 1: Dropping the guard immediately with `let _`

The single most common RAII mistake: binding a guard to `_` (a bare underscore) instead of a named variable. `let _ = expr;` evaluates `expr` and **drops the result at the end of that statement**: your fixture is torn down before the test body even starts.

```rust
struct Guard(&'static str);
impl Drop for Guard {
    fn drop(&mut self) {
        println!("dropping {}", self.0);
    }
}

fn main() {
    let _ = Guard("temp_a");      // drops IMMEDIATELY (end of this statement)
    println!("after let _");

    let _guard = Guard("temp_b"); // lives until end of scope
    println!("after let _guard");
}
```

Real output, note that `temp_a` is destroyed *before* "after let _" prints:

```text
dropping temp_a
after let _
after let _guard
dropping temp_b
```

A name beginning with an underscore (`_guard`) still binds and keeps the value alive; a bare `_` does not. The compiler will not warn you here — this is a logic bug, not a type error — so train your eye for it.

> **Warning:** `let _guard = TempDir::new()?;` is correct; `let _ = TempDir::new()?;` deletes the temp dir on the very next line. Always bind RAII guards to a named variable.

### Pitfall 2: Putting non-`Sync` state in a `static`

Coming from JavaScript's single-threaded model, it is tempting to keep mutable shared state in a `static RefCell`. Because Rust runs tests in parallel, the compiler refuses: a `static` must be `Sync`, and `RefCell` is not.

```rust
use std::cell::RefCell;
use std::sync::LazyLock;

// does not compile (error[E0277]: `RefCell<u32>` cannot be shared between threads safely)
static COUNTER: LazyLock<RefCell<u32>> = LazyLock::new(|| RefCell::new(0));

fn main() {
    *COUNTER.borrow_mut() += 1;
}
```

The real error from `rustc`:

```text
error[E0277]: `RefCell<u32>` cannot be shared between threads safely
 --> src/main.rs:5:17
  |
5 | static COUNTER: LazyLock<RefCell<u32>> = LazyLock::new(|| RefCell::new(0));
  |                 ^^^^^^^^^^^^^^^^^^^^^^ `RefCell<u32>` cannot be shared between threads safely
  |
  = help: the trait `Sync` is not implemented for `RefCell<u32>`
  = note: if you want to do aliasing and mutation between multiple threads, use `std::sync::RwLock` instead
  = note: required for `LazyLock<RefCell<u32>>` to implement `Sync`
  = note: shared static variables must have a type that implements `Sync`
```

The fix is to use a thread-safe interior-mutability type: `Mutex`, `RwLock`, or an atomic such as `AtomicU32`. (Smart pointers and interior mutability are covered in [Section 10 — Smart Pointers](/10-smart-pointers/).)

```rust
use std::sync::{LazyLock, Mutex};

static CALL_LOG: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

fn main() {
    CALL_LOG.lock().unwrap().push("event".to_string());
    assert_eq!(CALL_LOG.lock().unwrap().len(), 1);
}
```

### Pitfall 3: Expecting `beforeEach`-style isolation from shared mutable statics

If two tests both push into one global `Mutex<Vec<_>>`, they see each other's data; there is no per-test reset. With parallel execution, the *order* is nondeterministic too. Prefer fresh per-test fixtures (a helper that returns a brand-new value). Reserve shared mutable statics for things that are genuinely process-wide and either append-only or order-independent. If you truly need a test to run alone, gate the shared resource behind a `Mutex` and hold the lock for the whole test, or run with `cargo test -- --test-threads=1` to force sequential execution.

### Pitfall 4: Forgetting `Drop` does not run on `std::process::exit`

`Drop` runs on normal scope exit and during a panic *unwind*. It does **not** run if the process is killed with `std::process::exit`, or if a panic is configured to `abort` instead of unwind (`panic = "abort"`). Test teardown relies on unwinding, which is the default in the test profile, so this rarely bites in tests — but do not put critical, must-happen cleanup solely in `Drop` if your binary might `exit` mid-test-helper.

---

## Best Practices

- **Prefer a small helper function over a clever macro** for setup. `fn store_with_two_users() -> UserStore` reads clearly and composes; call several helpers in one test if needed.
- **Return the fixture; do not stash it in a global.** Returning a value gives each test its own copy and keeps parallel runs correct.
- **Use the `tempfile` crate for temp files and directories.** It generates collision-free names and deletes on drop, so you get the RAII guard for free without hand-rolling `TempDir`.

  ```toml
  # Cargo.toml
  [dev-dependencies]
  tempfile = "3"
  ```

- **Make teardown idempotent and error-tolerant.** Because `drop` cannot return `Result`, swallow cleanup errors with `let _ = ...` rather than `unwrap()`, which would risk a double-panic.
- **Bind guards to named variables**, even when unused, with a leading-underscore name (`_guard`) to silence the unused-variable warning while still keeping the value alive.
- **Reach for `LazyLock` (std) first**, falling back to `once_cell` only for compatibility or its extra API surface.
- **Keep shared statics read-only when you can.** Read-only `LazyLock` data is trivially safe across threads; mutable shared state always needs a `Mutex`/`RwLock`/atomic and careful thought about isolation.

---

## Real-World Example

A configuration loader, tested against a *real* file in a real temp directory. The fixture creates a temp dir, writes a config file into it, and returns both the `TempDir` guard and the file path. The test binds the guard, so the directory survives for the whole test and is deleted automatically when the guard drops — even if an assertion fails.

```rust
// src/lib.rs
use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct Config {
    pub port: u16,
    pub verbose: bool,
}

/// Parses a tiny `key=value` config file.
pub fn load_config(path: &Path) -> std::io::Result<Config> {
    let text = std::fs::read_to_string(path)?;
    let mut port = 8080;
    let mut verbose = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        match key.trim() {
            "port" => port = value.trim().parse().unwrap_or(8080),
            "verbose" => verbose = value.trim() == "true",
            _ => {}
        }
    }
    Ok(Config { port, verbose })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir; // dev-dependency: tempfile = "3"

    // Fixture: a temp dir holding a config file with the given body.
    // The returned `TempDir` is the RAII guard; the caller binds it so the
    // directory lives for the whole test and is deleted on drop.
    fn config_fixture(body: &str) -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().expect("create temp dir");
        let path = dir.path().join("app.conf");
        let mut file = std::fs::File::create(&path).expect("create file");
        file.write_all(body.as_bytes()).expect("write body");
        (dir, path)
    }

    #[test]
    fn parses_explicit_values() {
        let (_dir, path) = config_fixture("port = 9000\nverbose = true\n");
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg, Config { port: 9000, verbose: true });
    }

    #[test]
    fn falls_back_to_defaults() {
        let (_dir, path) = config_fixture("# empty file\n");
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg, Config { port: 8080, verbose: false });
    }
}
```

Real output:

```text
running 2 tests
test realworld::tests::parses_explicit_values ... ok
test realworld::tests::falls_back_to_defaults ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Notice the binding `let (_dir, path) = config_fixture(...)`. The `_dir` part keeps the `TempDir` guard alive for the body of the test; if you had written `let (_, path) = ...`, the temp directory would be deleted *before* `load_config` ran (Pitfall 1). The leading underscore silences the unused-variable warning without dropping the value.

---

## Further Reading

- [`std::sync::LazyLock`](https://doc.rust-lang.org/std/sync/struct.LazyLock.html): standard-library lazy static initialization.
- [The `Drop` trait](https://doc.rust-lang.org/std/ops/trait.Drop.html) and [the Drop chapter of the Book](https://doc.rust-lang.org/book/ch15-03-drop.html): RAII and deterministic destruction.
- [The `once_cell` crate](https://docs.rs/once_cell/): `Lazy`/`OnceCell` for older toolchains and richer use cases.
- [The `tempfile` crate](https://docs.rs/tempfile/) — temp files and directories that clean themselves up on drop.
- [`std::sync::Mutex`](https://doc.rust-lang.org/std/sync/struct.Mutex.html) — thread-safe interior mutability for shared fixtures.
- Within this section: [Unit Tests](/13-testing/00-unit-tests/) for the `#[test]` and `#[cfg(test)]` basics, [Test Organization](/13-testing/01-test-organization/) for where fixtures live, [Should Panic & Result tests](/13-testing/03-should-panic/) for `Result`-returning tests with `?`, [Integration Tests](/13-testing/04-integration-tests/) for shared helper modules under `tests/`, and [Mocking](/13-testing/06-mocking/) for trait-based test doubles.
- Related guide sections: [Section 10 — Smart Pointers](/10-smart-pointers/) for `Mutex`/`RwLock`/interior mutability, and [Section 14 — Macros](/14-macros/) if you eventually want to abstract repetitive fixture boilerplate.
- New to Rust testing? Start at [Section 00 — Introduction](/00-introduction/), [Section 01 — Getting Started](/01-getting-started/), and [Section 02 — Basics](/02-basics/).

---

## Exercises

### Exercise 1: A builder-style fixture

**Difficulty:** Easy

**Objective:** Replace TypeScript's `makeUser(overrides)` with an idiomatic Rust test builder.

**Instructions:**

Given this struct, write a `UserBuilder` inside a `#[cfg(test)] mod tests` that starts from sensible defaults and lets a test override fields fluently (`.id(7).name("Eve").inactive().build()`). Write two tests: one that overrides several fields, and one that relies entirely on the defaults.

```rust
#[derive(Debug, PartialEq, Clone)]
pub struct User {
    pub id: u32,
    pub name: String,
    pub active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UserBuilder {
        user: User,
    }
    impl UserBuilder {
        fn new() -> Self { /* ??? */ }
        // TODO: id(...), name(...), inactive(), build()
    }

    #[test]
    fn builder_defaults_then_overrides() {
        // TODO
    }
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, PartialEq, Clone)]
pub struct User {
    pub id: u32,
    pub name: String,
    pub active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    // A test builder: start from a default user, override fields fluently.
    struct UserBuilder {
        user: User,
    }
    impl UserBuilder {
        fn new() -> Self {
            UserBuilder {
                user: User { id: 1, name: "Default".into(), active: true },
            }
        }
        fn id(mut self, id: u32) -> Self {
            self.user.id = id;
            self
        }
        fn name(mut self, name: &str) -> Self {
            self.user.name = name.into();
            self
        }
        fn inactive(mut self) -> Self {
            self.user.active = false;
            self
        }
        fn build(self) -> User {
            self.user
        }
    }

    #[test]
    fn builder_defaults_then_overrides() {
        let u = UserBuilder::new().id(7).name("Eve").inactive().build();
        assert_eq!(u, User { id: 7, name: "Eve".into(), active: false });
    }

    #[test]
    fn builder_uses_defaults_when_untouched() {
        let u = UserBuilder::new().build();
        assert_eq!(u.id, 1);
        assert!(u.active);
    }
}
```

Each setter takes `self` by value, mutates, and returns `self`, so calls chain. `build()` consumes the builder and hands back the finished `User`. This output is verified: both tests pass with `cargo test`.

</details>

### Exercise 2: An environment-variable RAII guard

**Difficulty:** Medium

**Objective:** Build a guard that sets an environment variable on creation and restores the previous value on drop — Rust's answer to a `beforeEach`/`afterEach` pair around `process.env`.

**Instructions:**

Write an `EnvGuard` with `EnvGuard::set(key, value)` that records the prior value, sets the new one, and restores (or removes) the original in its `Drop` impl. Then write a test proving the variable is gone again once the guard's scope ends.

> **Note:** As of recent Rust editions, `std::env::set_var` / `remove_var` are `unsafe` because mutating the environment is not thread-safe. Wrap the calls in `unsafe { ... }` and keep the test single-threaded around that variable.

<details>
<summary>Solution</summary>

```rust
#[cfg(test)]
mod tests {
    struct EnvGuard {
        key: String,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: this test does not spawn threads that touch the env.
            unsafe { std::env::set_var(key, value) };
            EnvGuard { key: key.to_string(), previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.previous {
                    Some(v) => std::env::set_var(&self.key, v),
                    None => std::env::remove_var(&self.key),
                }
            }
        }
    }

    #[test]
    fn env_var_is_restored_after_the_test() {
        assert!(std::env::var("APP_FIXTURE_MODE").is_err());
        {
            let _guard = EnvGuard::set("APP_FIXTURE_MODE", "test");
            assert_eq!(std::env::var("APP_FIXTURE_MODE").unwrap(), "test");
        } // guard drops here -> var removed
        assert!(std::env::var("APP_FIXTURE_MODE").is_err());
    }
}
```

The guard captures the previous value in `set`, applies the new one, and restores it in `drop`. Binding it to `_guard` (named, not bare `_`) keeps it alive for the inner block. This test passes; verified with `cargo test`.

</details>

### Exercise 3: A unique-ID fixture for parallel tests

**Difficulty:** Medium

**Objective:** Provide a shared, thread-safe fixture that hands out a unique ID per call so that tests running in parallel never collide.

**Instructions:**

Inside a test module, create a `static` counter and a `fresh_id()` helper that returns a new value each call. It must be safe to call from multiple threads (remember: Rust runs tests in parallel). Write two tests that each grab two IDs and assert they differ.

<details>
<summary>Solution</summary>

```rust
#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    // Fixture helper: hand out a fresh, unique id per call so parallel
    // tests never collide on the same record.
    static NEXT_ID: AtomicU32 = AtomicU32::new(1);

    fn fresh_id() -> u32 {
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn ids_are_unique_1() {
        let a = fresh_id();
        let b = fresh_id();
        assert_ne!(a, b);
    }

    #[test]
    fn ids_are_unique_2() {
        let a = fresh_id();
        let b = fresh_id();
        assert_ne!(a, b);
    }
}
```

`AtomicU32::fetch_add` atomically increments and returns the previous value, so every caller — on any thread — gets a distinct number. No `Mutex` is needed for a simple counter, and no lazy initialization is required because `AtomicU32::new` is a `const fn` usable directly in a `static`. Both tests pass; verified with `cargo test`.

</details>
