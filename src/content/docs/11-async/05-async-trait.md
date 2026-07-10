---
title: "Async Functions in Traits"
description: "Async fn in traits is stable since Rust 1.75, no crate needed, but it is not dyn-compatible. When to reach for async-trait, plus the RPITIT desugaring."
---

In TypeScript, putting an `async` method on an interface is unremarkable. An interface method that returns `Promise<T>` is just a method that returns an object. Rust took years to reach the same place, and the result has a few sharp edges that every TypeScript/JavaScript developer hits. This page explains **native `async fn` in traits** (stable since Rust 1.75, no crate required), when you still reach for the **`async-trait`** crate, and the desugaring (**RPITIT**) that explains why.

---

## Quick Overview

You can write `async fn` directly in a **trait** (Rust's version of an interface) since Rust 1.75; no external crate needed. The catch: a trait with a native `async fn` is **not `dyn`-compatible**, so you cannot put it behind `Box<dyn Trait>`. When you need that (heterogeneous collections, plugin systems, dependency injection), you reach for the `async-trait` crate, which boxes the returned future. This is the opposite of TypeScript, where async interface methods "just work" behind any reference because every object is already heap-allocated and dynamically dispatched.

---

## TypeScript/JavaScript Example

In TypeScript, an interface with `async` methods is completely ordinary. The methods just return `Promise<T>`, and you can use the interface as a type, store implementations in arrays, and pass them around freely.

```typescript
// A repository interface with async methods — totally routine in TypeScript.
interface UserRepository {
  find(id: number): Promise<User | null>;
  save(user: User): Promise<void>;
}

interface User {
  id: number;
  name: string;
}

class InMemoryRepo implements UserRepository {
  private users = new Map<number, User>();

  async find(id: number): Promise<User | null> {
    return this.users.get(id) ?? null;
  }

  async save(user: User): Promise<void> {
    this.users.set(user.id, user);
  }
}

class SqlRepo implements UserRepository {
  async find(id: number): Promise<User | null> {
    // imagine: await this.pool.query(...)
    return { id, name: `db-user-${id}` };
  }
  async save(_user: User): Promise<void> {}
}

// Store DIFFERENT implementations behind the interface type — no ceremony.
const repos: UserRepository[] = [new InMemoryRepo(), new SqlRepo()];

// Pass the interface around; the caller never knows the concrete class.
async function rename(repo: UserRepository, id: number, name: string) {
  const user = await repo.find(id);
  if (!user) throw new Error("not found");
  user.name = name;
  await repo.save(user);
  return user;
}
```

Two things are happening that you never think about:

1. `UserRepository[]` holds different concrete classes side by side — **dynamic dispatch** through the prototype chain.
2. Every method returns a `Promise`, which is a heap object the engine manages.

Rust supports both ideas, but it makes the trade-offs explicit.

---

## Rust Equivalent

The idiomatic, **crate-free** way to write an async method on a trait:

```rust
// Native async fn in traits — stable since Rust 1.75, NO crate needed.
use std::collections::HashMap;

trait DataStore {
    async fn get(&self, key: &str) -> Option<String>;
    async fn set(&mut self, key: String, value: String);
}

struct MemoryStore {
    data: HashMap<String, String>,
}

impl DataStore for MemoryStore {
    async fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).cloned()
    }

    async fn set(&mut self, key: String, value: String) {
        self.data.insert(key, value);
    }
}

#[tokio::main]
async fn main() {
    let mut store = MemoryStore { data: HashMap::new() };
    store.set("greeting".to_string(), "hello".to_string()).await;
    let value = store.get("greeting").await;
    println!("{value:?}");
}
```

Running it prints the real output:

```text
Some("hello")
```

> **Note:** This requires no `async-trait` in `Cargo.toml`. The only dependency here is Tokio, which provides the **runtime** that actually drives the futures. Recall that [Rust futures are lazy](/11-async/00-promises-vs-futures/) and do nothing until polled.

This compiles and runs on stable Rust. But notice we called `store.get(...)` on a **concrete** `MemoryStore`, not through a `dyn DataStore`. That distinction is the whole story of this page.

---

## Detailed Explanation

### `async fn` in a trait is real, and crate-free

Before Rust 1.75, writing `async fn` in a trait was a compile error, and the entire ecosystem used the `async-trait` macro. Since 1.75, this is built into the language. On the repository's [pinned verification baseline](/00-introduction/05-version-policy/) and the 2024 edition, the example above needs nothing but the trait and the impl.

Line by line:

- `trait DataStore { async fn get(...) -> Option<String>; }` declares an async method. The trait is Rust's analog of a TypeScript `interface`. (See [Traits](/09-generics-traits/03-traits/).)
- `impl DataStore for MemoryStore { async fn get(...) {...} }` provides the body. Just like the TypeScript class implementing the interface.
- `store.get("greeting").await` — calling the method returns a **future**, and `.await` drives it to completion.

### What `async fn` desugars to: RPITIT

An `async fn` in a trait is **syntax sugar**. The compiler rewrites it into a normal method that returns `impl Future`. This feature is called **RPITIT** — *return-position `impl Trait` in trait*. These two trait definitions are equivalent:

```rust
// What `async fn` in a trait desugars to: RPITIT (return-position impl Trait in trait).
use std::future::Future;

// These two trait definitions are equivalent to the compiler.
trait FetcherSugar {
    async fn fetch(&self, url: &str) -> String;
}

trait FetcherDesugared {
    // `async fn` is sugar for a method returning `impl Future`.
    fn fetch(&self, url: &str) -> impl Future<Output = String>;
}

struct StubFetcher;

// Implement the desugared form by hand: return an async block.
impl FetcherDesugared for StubFetcher {
    fn fetch(&self, url: &str) -> impl Future<Output = String> {
        let url = url.to_string();
        async move { format!("body of {url}") }
    }
}

// You can also satisfy the sugared trait with plain `async fn`.
impl FetcherSugar for StubFetcher {
    async fn fetch(&self, url: &str) -> String {
        format!("body of {url}")
    }
}

#[tokio::main]
async fn main() {
    let f = StubFetcher;
    let a = FetcherDesugared::fetch(&f, "https://a.example").await;
    let b = FetcherSugar::fetch(&f, "https://b.example").await;
    println!("{a}");
    println!("{b}");
}
```

Real output:

```text
body of https://a.example
body of https://b.example
```

This desugaring is exactly why `dyn` does not work (next section). The returned future type is **anonymous** and **different for every impl**: there is no single, named type the compiler can put in a vtable. Contrast this with TypeScript: `Promise<T>` is one concrete runtime type regardless of which method produced it, so an interface array `UserRepository[]` is trivial.

> **Tip:** You can mix the two forms freely. If you need the `impl Future` signature for some reason (for example, to add a `+ Send` bound — see Common Pitfalls), write the desugared form. Otherwise, `async fn` reads better.

### Why does the compiler monomorphize this?

When you call the method through a concrete type or a generic type parameter (`<C: DataStore>`), the compiler knows the exact future type and **monomorphizes** the code: generates a specialized copy, zero-cost, no allocation. This is the same machinery as [generics](/09-generics-traits/00-generic-functions/). TypeScript erases generics at runtime; Rust specializes them at compile time. The price is that monomorphization needs a *concrete* type, which `dyn` deliberately throws away.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust |
| ------- | --------------------- | ---- |
| Async interface/trait method | Returns `Promise<T>`, always works | `async fn` in trait, stable since 1.75 |
| Return type | `Promise<T>` — one concrete runtime type | Anonymous `impl Future` — different per impl |
| Behind `interface`/`dyn` | Free (everything is a heap object + dynamic dispatch) | **Not allowed** for native async trait methods |
| Static dispatch | Not really a concept (always dynamic) | Default; monomorphized, zero-cost |
| Heterogeneous collection | `UserRepository[]` just works | Needs `Vec<Box<dyn Trait>>` + `async-trait` crate |
| Eager vs lazy | Promise **starts running** when created | Future does **nothing** until `.await`/polled |
| Cost of dynamic dispatch | Always paid (it is the only mode) | Opt-in; `async-trait` adds a heap allocation per call |

The headline difference: **TypeScript only has dynamic dispatch, so async interface methods are free. Rust defaults to static dispatch, which is free *only* when the concrete type is known, and `dyn` (the dynamic mode) is where async traits get awkward.**

> **Warning:** Do not assume `Vec<Box<dyn MyAsyncTrait>>` will compile with a native `async fn`. It will not. See the next section for the exact error and the fix.

---

## Common Pitfalls

### Pitfall 1: `Box<dyn Trait>` with a native async method

This is the single most common surprise. A TypeScript developer reaches for `Box<dyn Trait>` expecting it to behave like a `UserRepository[]` element:

```rust
// does not compile (error[E0038]: the trait `Notifier` is not dyn compatible)
trait Notifier {
    async fn notify(&self, msg: &str);
}

struct EmailNotifier;

impl Notifier for EmailNotifier {
    async fn notify(&self, msg: &str) {
        println!("email: {msg}");
    }
}

fn make_notifier() -> Box<dyn Notifier> {
    Box::new(EmailNotifier)
}
```

The real compiler error:

```text
error[E0038]: the trait `Notifier` is not dyn compatible
  --> src/main.rs:14:27
   |
14 | fn make_notifier() -> Box<dyn Notifier> {
   |                           ^^^^^^^^^^^^ `Notifier` is not dyn compatible
   |
note: for a trait to be dyn compatible it needs to allow building a vtable
      for more information, visit <https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>
  --> src/main.rs:3:14
   |
 2 | trait Notifier {
   |       -------- this trait is not dyn compatible...
 3 |     async fn notify(&self, msg: &str);
   |              ^^^^^^ ...because method `notify` is `async`
   = help: consider moving `notify` to another trait
```

The phrase **"not dyn compatible"** (older Rust called this "not object safe") means: the method returns an anonymous future type that differs per impl, so the compiler cannot build a vtable. **Fix:** use the `async-trait` crate (see Best Practices), or restructure to use generics/static dispatch.

### Pitfall 2: spawning a trait future fails with "future cannot be sent between threads"

Native async-trait futures are **not guaranteed to be `Send`**. The moment you try to move one into `tokio::spawn` (which runs it on the multi-threaded scheduler and therefore requires `Send`), it breaks:

```rust
// does not compile (future created by async block is not `Send`)
trait Worker {
    async fn run(&self) -> u32;
}

async fn run_on_task<W: Worker + Send + Sync + 'static>(worker: W) -> u32 {
    let handle = tokio::spawn(async move { worker.run().await });
    handle.await.unwrap()
}
```

The real error (abridged), and note the compiler even suggests the fix:

```text
error: future cannot be sent between threads safely
   --> src/main.rs:8:18
    |
  8 |     let handle = tokio::spawn(async move { worker.run().await });
    |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ future created by async block is not `Send`
    |
note: future is not `Send` as it awaits another future which is not `Send`
note: required by a bound in `tokio::spawn`
help: `Send` can be made part of the associated future's guarantees for all implementations of `Worker::run`
    |
  4 -     async fn run(&self) -> u32;
  4 +     fn run(&self) -> impl std::future::Future<Output = u32> + Send;
    |
```

**Fix:** write the desugared form with an explicit `+ Send` bound on the returned future:

```rust
// Fix: require the returned future to be Send by writing the desugared form.
use std::future::Future;

trait Worker {
    fn run(&self) -> impl Future<Output = u32> + Send;
}

async fn run_on_task<W: Worker + Send + Sync + 'static>(worker: W) -> u32 {
    let handle = tokio::spawn(async move { worker.run().await });
    handle.await.unwrap()
}

struct Counter;

impl Worker for Counter {
    fn run(&self) -> impl Future<Output = u32> + Send {
        async { 7 }
    }
}

#[tokio::main]
async fn main() {
    let total = run_on_task(Counter).await;
    println!("{total}");
}
```

This prints `7`. (More on `tokio::spawn` and `Send` in [Spawning Tasks](/11-async/09-spawning-tasks/).)

### Pitfall 3: assuming you "need the async-trait crate" like the old days

A lot of older blog posts and Stack Overflow answers say you *must* use `async-trait`. That advice is **outdated**. For the common case — a trait used through generics or concrete types — native `async fn` is correct and faster. Only reach for the crate when you genuinely need `dyn`. (Contrast with the equally common myth that you need the `async-trait` crate for *all* async traits: you do not.)

### Pitfall 4: expecting the future to start when the method is called

Calling `store.get("k")` does **not** run anything; it builds a future. In TypeScript, `repo.find(id)` starts the async work immediately (Promises are **eager**). In Rust, nothing happens until `.await`. Forgetting the `.await` on a trait method gives you an unused-future warning, not a running task. See [Promises vs Futures](/11-async/00-promises-vs-futures/).

---

## Best Practices

### Prefer native `async fn` in traits (no crate)

For traits consumed via concrete types or generic bounds, use plain `async fn`. It is built in, monomorphized, and allocation-free:

```rust
// Static dispatch with native async fn in traits — no crate, no boxing.
trait Cache {
    async fn lookup(&self, key: &str) -> Option<u64>;
}

struct AlwaysHit;
struct AlwaysMiss;

impl Cache for AlwaysHit {
    async fn lookup(&self, _key: &str) -> Option<u64> {
        Some(42)
    }
}

impl Cache for AlwaysMiss {
    async fn lookup(&self, _key: &str) -> Option<u64> {
        None
    }
}

// Generic over the concrete cache type: monomorphized, zero-cost, no boxing.
async fn report<C: Cache>(cache: &C, key: &str) {
    match cache.lookup(key).await {
        Some(v) => println!("{key} -> {v}"),
        None => println!("{key} -> miss"),
    }
}

#[tokio::main]
async fn main() {
    report(&AlwaysHit, "user:1").await;
    report(&AlwaysMiss, "user:2").await;
}
```

Output:

```text
user:1 -> 42
user:2 -> miss
```

### Use the `async-trait` crate when you need `dyn`

When you genuinely need trait objects — heterogeneous collections, plugin registries, dependency injection where the concrete type is chosen at runtime — add `async-trait` and annotate the trait and every impl:

```bash
cargo add async-trait
```

```rust playground
// async-trait crate — makes the trait dyn-compatible by boxing the returned future.
use async_trait::async_trait;

#[async_trait]
trait Notifier {
    async fn notify(&self, msg: &str);
}

struct EmailNotifier;
struct SmsNotifier;

#[async_trait]
impl Notifier for EmailNotifier {
    async fn notify(&self, msg: &str) {
        println!("email: {msg}");
    }
}

#[async_trait]
impl Notifier for SmsNotifier {
    async fn notify(&self, msg: &str) {
        println!("sms: {msg}");
    }
}

#[tokio::main]
async fn main() {
    // A heterogeneous collection of trait objects — now possible.
    let notifiers: Vec<Box<dyn Notifier>> =
        vec![Box::new(EmailNotifier), Box::new(SmsNotifier)];

    for n in &notifiers {
        n.notify("server is down").await;
    }
}
```

Output:

```text
email: server is down
sms: server is down
```

Under the hood, `#[async_trait]` rewrites each method to return `Pin<Box<dyn Future + Send + '_>>`. That is: it **boxes the future** (one heap allocation per call) so there *is* a single concrete return type the vtable can hold. This is exactly what TypeScript does implicitly for every Promise. Rust just makes the cost visible and opt-in.

> **Note:** `#[async_trait]` adds a `Send` bound by default, which fixes Pitfall 2 for free. If you need a non-`Send` variant (for example, a single-threaded runtime holding `Rc`), write `#[async_trait(?Send)]`.

### For `Send` bounds without `dyn`, consider `trait-variant`

If you want native (non-boxed) async traits *and* `Send` futures for spawning, the `trait-variant` crate generates a `Send`-bounded variant so you can keep writing plain `async fn`:

```bash
cargo add trait-variant
```

```rust
// `trait-variant` generates a Send-bounded variant of an async trait.
#[trait_variant::make(HttpService: Send)]
trait LocalHttpService {
    async fn fetch(&self, url: &str) -> String;
}

struct Client;

impl HttpService for Client {
    async fn fetch(&self, url: &str) -> String {
        format!("200 OK {url}")
    }
}

async fn run_on_task<S: HttpService + Send + Sync + 'static>(svc: S) -> String {
    tokio::spawn(async move { svc.fetch("https://api.example.com").await })
        .await
        .unwrap()
}

#[tokio::main]
async fn main() {
    println!("{}", run_on_task(Client).await);
}
```

Output:

```text
200 OK https://api.example.com
```

The macro generates a `Send`-bounded `HttpService` from the base `LocalHttpService`. You implement `HttpService` with ordinary `async fn`, and its futures are `Send`, so `tokio::spawn` accepts them.

### Decision guide

| Your situation | Use |
| -------------- | --- |
| Trait used via concrete types or `<T: Trait>` generics | Native `async fn` (no crate) |
| Need `Box<dyn Trait>` / `Vec<Box<dyn Trait>>` / trait object | `async-trait` crate |
| Need native (unboxed) traits whose futures are `Send` | `+ Send` desugared form, or `trait-variant` |
| Single-threaded runtime, want `dyn`, futures need not be `Send` | `#[async_trait(?Send)]` |

---

## Real-World Example

A repository pattern, the Rust equivalent of the opening TypeScript example. The service layer depends on a `dyn UserRepository` so it is decoupled from the concrete backend (in-memory for tests, SQL in production). Because we need `dyn`, this uses the `async-trait` crate.

```rust playground
// Real-world: a `Repository` trait with two backends, used behind a `dyn` trait
// object so the service layer is decoupled from the concrete database.
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone)]
struct User {
    id: u64,
    name: String,
}

// We want `dyn UserRepository`, so we use the async-trait crate.
#[async_trait]
trait UserRepository: Send + Sync {
    async fn find(&self, id: u64) -> Option<User>;
    async fn save(&self, user: User) -> Result<(), String>;
}

// Backend 1: an in-memory store (great for tests).
struct InMemoryRepo {
    users: Mutex<HashMap<u64, User>>,
}

#[async_trait]
impl UserRepository for InMemoryRepo {
    async fn find(&self, id: u64) -> Option<User> {
        self.users.lock().unwrap().get(&id).cloned()
    }

    async fn save(&self, user: User) -> Result<(), String> {
        self.users.lock().unwrap().insert(user.id, user);
        Ok(())
    }
}

// Backend 2: a stand-in for a real database client.
struct SqlRepo;

#[async_trait]
impl UserRepository for SqlRepo {
    async fn find(&self, id: u64) -> Option<User> {
        // Imagine an `sqlx::query!(...).fetch_optional(&pool).await` here.
        Some(User { id, name: format!("db-user-{id}") })
    }

    async fn save(&self, _user: User) -> Result<(), String> {
        Ok(())
    }
}

// The service holds a trait object — it does not care which backend it got.
struct UserService {
    repo: Box<dyn UserRepository>,
}

impl UserService {
    async fn rename(&self, id: u64, new_name: &str) -> Result<User, String> {
        let mut user = self.repo.find(id).await.ok_or("not found")?;
        user.name = new_name.to_string();
        self.repo.save(user.clone()).await?;
        Ok(user)
    }
}

#[tokio::main]
async fn main() {
    // Swap backends without touching UserService.
    let mem = InMemoryRepo { users: Mutex::new(HashMap::new()) };
    mem.save(User { id: 1, name: "Ada".into() }).await.unwrap();
    let svc = UserService { repo: Box::new(mem) };
    println!("{:?}", svc.rename(1, "Ada Lovelace").await);

    let svc2 = UserService { repo: Box::new(SqlRepo) };
    println!("{:?}", svc2.rename(7, "Grace").await);
}
```

Real output:

```text
Ok(User { id: 1, name: "Ada Lovelace" })
Ok(User { id: 7, name: "Grace" })
```

A few production notes:

- `UserRepository: Send + Sync` is the **supertrait** bound (see [Supertraits](/09-generics-traits/09-supertraits/)) you almost always want on a `dyn` service trait, so the boxed repository can be shared across tasks.
- The `?` operator works inside async trait methods just like in any `async fn` (see [Async/Await](/11-async/01-async-await/)).
- For an even more decoupled design, store `Arc<dyn UserRepository>` so the repo can be cloned cheaply across spawned tasks. See [Arc + Mutex Pattern](/11-async/12-arc-mutex-pattern/).

---

## Further Reading

- [Async fn in traits (Rust 1.75 release notes)](https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits/): the official announcement and rationale.
- [The Async Book: async in traits](https://rust-lang.github.io/async-book/) — broader async patterns.
- [`async-trait` crate docs](https://docs.rs/async-trait/): the macro, including `?Send`.
- [`trait-variant` crate docs](https://docs.rs/trait-variant/) — generating `Send` variants.
- [dyn compatibility reference](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility): the rules for trait objects.
- Related sections in this guide:
  - [Promises vs Futures](/11-async/00-promises-vs-futures/) — why Rust futures are lazy.
  - [Async/Await](/11-async/01-async-await/): `async fn`, `.await`, and `?`.
  - [Async Functions](/11-async/04-async-functions/) — async blocks, capturing, lifetimes.
  - [Spawning Tasks](/11-async/09-spawning-tasks/): `tokio::spawn` and the `Send` requirement.
  - [Tokio Intro](/11-async/02-tokio-intro/) and [Tokio Setup](/11-async/03-tokio-setup/) — the runtime.
  - [Traits](/09-generics-traits/03-traits/), [Trait Objects](/09-generics-traits/06-trait-objects/), and [`impl Trait`](/09-generics-traits/07-impl-trait/) — the non-async foundations.
  - Next up, organizing all this code into [Modules & Packages](/12-modules-packages/).

---

## Exercises

### Exercise 1: Native async trait with a default method

**Difficulty:** Easy

**Objective:** Confirm you can declare an async trait and provide a default async method, with no crate.

**Instructions:**

1. Define a trait `HealthCheck` with `async fn ping(&self) -> bool`.
2. Add a **default** `async fn is_healthy(&self) -> &'static str` that calls `self.ping().await` and returns `"ok"` or `"down"`.
3. Implement it for two structs, one whose `ping` returns `true` and one `false`.
4. Print the result of `is_healthy().await` for each.

```rust
trait HealthCheck {
    async fn ping(&self) -> bool;

    async fn is_healthy(&self) -> &'static str {
        // TODO: call self.ping().await and return "ok" / "down"
        /* ??? */
    }
}
// TODO: ServiceA (true), ServiceB (false), and a #[tokio::main] main
```

<details>
<summary>Solution</summary>

```rust
trait HealthCheck {
    async fn ping(&self) -> bool;

    // Default method calls another async method on the trait.
    async fn is_healthy(&self) -> &'static str {
        if self.ping().await {
            "ok"
        } else {
            "down"
        }
    }
}

struct ServiceA;
struct ServiceB;

impl HealthCheck for ServiceA {
    async fn ping(&self) -> bool {
        true
    }
}

impl HealthCheck for ServiceB {
    async fn ping(&self) -> bool {
        false
    }
}

#[tokio::main]
async fn main() {
    println!("A: {}", ServiceA.is_healthy().await);
    println!("B: {}", ServiceB.is_healthy().await);
}
```

Output:

```text
A: ok
B: down
```

</details>

### Exercise 2: Make a plugin trait `dyn`-compatible

**Difficulty:** Medium

**Objective:** Build a plugin pipeline where different plugins are stored in `Vec<Box<dyn Plugin>>` — which forces you to use the `async-trait` crate.

**Instructions:**

1. Run `cargo add async-trait`.
2. Define `#[async_trait] trait Plugin: Send + Sync { async fn execute(&self, payload: &str) -> String; }`.
3. Implement two plugins: one uppercases its input, one reverses it.
4. Write `run_pipeline(plugins: &[Box<dyn Plugin>], input: &str) -> String` that feeds each plugin's output into the next.
5. Run uppercase then reverse on `"rust"`.

<details>
<summary>Solution</summary>

```rust playground
use async_trait::async_trait;

#[async_trait]
trait Plugin: Send + Sync {
    async fn execute(&self, payload: &str) -> String;
}

struct UppercasePlugin;
struct ReversePlugin;

#[async_trait]
impl Plugin for UppercasePlugin {
    async fn execute(&self, payload: &str) -> String {
        payload.to_uppercase()
    }
}

#[async_trait]
impl Plugin for ReversePlugin {
    async fn execute(&self, payload: &str) -> String {
        payload.chars().rev().collect()
    }
}

async fn run_pipeline(plugins: &[Box<dyn Plugin>], input: &str) -> String {
    let mut current = input.to_string();
    for plugin in plugins {
        current = plugin.execute(&current).await;
    }
    current
}

#[tokio::main]
async fn main() {
    let plugins: Vec<Box<dyn Plugin>> =
        vec![Box::new(UppercasePlugin), Box::new(ReversePlugin)];
    println!("{}", run_pipeline(&plugins, "rust").await);
}
```

Output (`"rust"` → `"RUST"` → reversed):

```text
TSUR
```

</details>

### Exercise 3: A spawnable native async trait

**Difficulty:** Hard

**Objective:** Write a **native** (unboxed) async trait whose futures are `Send`, then run its work across tasks with `tokio::spawn`.

**Instructions:**

1. Define `trait Transform: Send + Sync + 'static` with a method `fn apply(&self, input: u64) -> impl Future<Output = u64> + Send;` (the desugared `+ Send` form — a plain `async fn` here would fail to spawn).
2. Implement `Doubler`, which doubles its input.
3. Write `run_parallel<T: Transform>(transform: T, inputs: Vec<u64>) -> Vec<u64>` that wraps the transform in an `Arc`, `tokio::spawn`s one task per input, and collects the results.
4. Run it on `vec![1, 2, 3, 4]`.

**Hint:** Wrap the transform in `Arc` and `clone` the `Arc` into each spawned task.

<details>
<summary>Solution</summary>

```rust
use std::future::Future;
use std::sync::Arc;

trait Transform: Send + Sync + 'static {
    fn apply(&self, input: u64) -> impl Future<Output = u64> + Send;
}

struct Doubler;

impl Transform for Doubler {
    fn apply(&self, input: u64) -> impl Future<Output = u64> + Send {
        async move { input * 2 }
    }
}

// Spawns the work on separate tasks; requires the future to be Send.
async fn run_parallel<T: Transform>(transform: T, inputs: Vec<u64>) -> Vec<u64> {
    let transform = Arc::new(transform);
    let mut handles = Vec::new();
    for input in inputs {
        let t = transform.clone();
        handles.push(tokio::spawn(async move { t.apply(input).await }));
    }
    let mut out = Vec::new();
    for h in handles {
        out.push(h.await.unwrap());
    }
    out
}

#[tokio::main]
async fn main() {
    let results = run_parallel(Doubler, vec![1, 2, 3, 4]).await;
    println!("{results:?}");
}
```

Output:

```text
[2, 4, 6, 8]
```

> Note: the task order is deterministic here because we collect `handles` in order and `await` them sequentially; the *execution* may interleave across threads.

</details>
