---
title: "Dependency Injection in Rust"
description: "In Rust, dependency injection is constructor injection over a trait: generics wire it at compile time, trait objects at runtime. No DI container needed."
---

In TypeScript, dependency injection usually means a framework: NestJS `@Injectable()` providers, InversifyJS containers, or hand-rolled constructor injection wired by a DI container. The runtime resolves a dependency graph for you, often using decorators and reflection metadata. Rust has no DI container in the standard library and rarely needs one. **Dependency injection in Rust is just ordinary constructor injection over a trait**, with the compiler doing the wiring at type-check time instead of a runtime container doing it via reflection. This file is about the two ways to express that injection (**generics** for compile-time wiring, **trait objects** for runtime wiring), how to choose, and why both make code dramatically more testable than reaching for a global.

---

## Quick Overview

**Dependency injection (DI)** means a component receives the collaborators it needs from the outside instead of constructing or importing them itself. A `Notifier` that needs a clock and an email sender takes them as constructor arguments rather than calling `new SmtpSender()` internally. The payoff is testability and flexibility: in production you pass the real SMTP sender; in a test you pass a fake that records what it was asked to send.

Rust expresses the dependency as a **trait** (the "interface") and injects an implementation in one of two forms:

- **Generics** (`struct Service<S: Store>`): the dependency is chosen at compile time, monomorphized to zero-cost static dispatch. Like a TypeScript generic, but with no type erasure and no `vtable`.
- **Trait objects** (`Box<dyn Store>`, `Arc<dyn Store>`, `&dyn Store`): the dependency is chosen at runtime through a vtable, the closest analogue to how a TypeScript DI container hands you an object behind an interface.

> **Note:** This page is about *wiring dependencies into a component*. The mechanics of `dyn Trait`, object safety, and static-vs-dynamic dispatch live in [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/) and [Section 09: Trait Bounds](/09-generics-traits/05-trait-bounds/). For swapping an *algorithm* (a closely related pattern) see [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/); for *constructing* the concrete dependency see [The Factory Pattern](/22-common-patterns/08-factory-pattern/); for the mocking tools used in tests see [Section 13: Mocking](/13-testing/06-mocking/).

---

## TypeScript/JavaScript Example

A `WelcomeService` needs two collaborators: a clock (to timestamp the greeting) and a user store (to look up an email). The testable design injects both through the constructor rather than importing a singleton or calling `Date.now()` directly.

```typescript
// TypeScript - constructor injection behind interfaces.
interface Clock {
  nowUnix(): number;
}

interface UserStore {
  findEmail(userId: number): string | undefined;
}

class WelcomeService {
  // Dependencies arrive as constructor parameters — classic DI.
  constructor(
    private readonly clock: Clock,
    private readonly users: UserStore,
  ) {}

  greetingFor(userId: number): string {
    const email = this.users.findEmail(userId);
    if (email === undefined) throw new Error(`no user ${userId}`);
    return `Welcome ${email} (at ${this.clock.nowUnix()})`;
  }
}

// Production wiring (a DI container like NestJS/Inversify would do this for you):
const systemClock: Clock = { nowUnix: () => Math.floor(Date.now() / 1000) };
const userStore: UserStore = {
  findEmail: (id) => (id === 7 ? "ada@example.com" : undefined),
};

const service = new WelcomeService(systemClock, userStore);

// A test would inject fakes instead:
const fixedClock: Clock = { nowUnix: () => 1000 };
const testService = new WelcomeService(fixedClock, userStore);
console.log(testService.greetingFor(7));
```

**Output (Node v22):**

```text
Welcome ada@example.com (at 1000)
```

Two observations that drive the Rust comparison. First, every dependency is reached through an `interface`, and every call (`this.clock.nowUnix()`) is a dynamic property lookup; JavaScript has no other kind of dispatch. Second, a NestJS/InversifyJS container exists only to *automate the wiring* (resolve the graph, manage lifetimes/singletons); it does not change the fundamental shape, which is "pass collaborators into the constructor." Rust keeps that shape and deletes the container.

---

## Rust Equivalent

The dependency is a **trait**; the service receives an implementation. Here it is both ways — generics first (the idiomatic default), then trait objects (when you need runtime flexibility).

### Version 1: generics (compile-time injection, zero-cost)

The service is generic over its dependencies. The compiler generates a specialized copy of `WelcomeService` for each concrete `(Clock, UserStore)` pair you actually use (monomorphization), so every call is statically dispatched and inlinable: no vtable, no boxing.

```rust playground
// Rust - the service is generic over its dependencies (static dispatch).
trait Clock {
    fn now_unix(&self) -> u64;
}

trait UserStore {
    fn find_email(&self, user_id: u64) -> Option<String>;
}

// `C` and `S` are resolved at compile time.
struct WelcomeService<C: Clock, S: UserStore> {
    clock: C,
    users: S,
}

impl<C: Clock, S: UserStore> WelcomeService<C, S> {
    // Constructor injection: dependencies arrive as arguments, never as globals.
    fn new(clock: C, users: S) -> Self {
        Self { clock, users }
    }

    fn greeting_for(&self, user_id: u64) -> Result<String, String> {
        let email = self
            .users
            .find_email(user_id)
            .ok_or_else(|| format!("no user {user_id}"))?;
        Ok(format!("Welcome {email} (at {})", self.clock.now_unix()))
    }
}

// A fixed clock and a one-user store — exactly what a test injects.
struct FixedClock(u64);
impl Clock for FixedClock {
    fn now_unix(&self) -> u64 {
        self.0
    }
}

struct OneUser(&'static str);
impl UserStore for OneUser {
    fn find_email(&self, _id: u64) -> Option<String> {
        Some(self.0.to_string())
    }
}

fn main() {
    let service = WelcomeService::new(FixedClock(1000), OneUser("grace@example.com"));
    println!("{}", service.greeting_for(1).unwrap());
}
```

**Real output:**

```text
Welcome grace@example.com (at 1000)
```

### Version 2: trait objects (runtime injection, like a DI container)

When you need to decide an implementation at runtime (from config, a feature flag, or because you store a heterogeneous collection of services), hold the dependency as a **boxed trait object**. This is the form closest to a TypeScript DI container: one field type, many possible concrete values, dispatched through a vtable.

```rust playground
// Rust - the service owns boxed trait objects, chosen at runtime.
use std::collections::HashMap;

trait Clock {
    fn now_unix(&self) -> u64;
}

trait UserStore {
    fn find_email(&self, user_id: u64) -> Option<String>;
}

struct WelcomeService {
    clock: Box<dyn Clock>,
    users: Box<dyn UserStore>,
}

impl WelcomeService {
    fn new(clock: Box<dyn Clock>, users: Box<dyn UserStore>) -> Self {
        Self { clock, users }
    }

    fn greeting_for(&self, user_id: u64) -> Result<String, String> {
        let email = self
            .users
            .find_email(user_id)
            .ok_or_else(|| format!("no user {user_id}"))?;
        let ts = self.clock.now_unix();
        Ok(format!("Welcome {email} (at {ts})"))
    }
}

// --- Production implementations ---

struct SystemClock;
impl Clock for SystemClock {
    fn now_unix(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

struct InMemoryUsers {
    rows: HashMap<u64, String>,
}
impl UserStore for InMemoryUsers {
    fn find_email(&self, user_id: u64) -> Option<String> {
        self.rows.get(&user_id).cloned()
    }
}

fn main() {
    let mut rows = HashMap::new();
    rows.insert(7, "ada@example.com".to_string());

    // The composition root: the one place that names concrete implementations.
    let service = WelcomeService::new(
        Box::new(SystemClock),
        Box::new(InMemoryUsers { rows }),
    );

    match service.greeting_for(7) {
        Ok(msg) => println!("{msg}"),
        Err(e) => println!("error: {e}"),
    }
    println!("{:?}", service.greeting_for(99));
}
```

**Real output** (the timestamp reflects the wall clock when you run it):

```text
Welcome ada@example.com (at 1780382172)
Err("no user 99")
```

Both versions have identical call sites and identical behavior. The only difference is *when* the dependency is resolved: the generic version bakes it in at compile time; the trait-object version defers it to runtime. That single decision is the whole topic.

---

## Detailed Explanation

**The dependency is a trait, not a concrete type.** In both versions, `WelcomeService` never mentions `SystemClock` or `InMemoryUsers`. It speaks only to the `Clock` and `UserStore` traits. This is the dependency-inversion principle in its purest form: the high-level policy (greeting logic) depends on an abstraction, and the low-level detail (a real clock, a database) implements that abstraction. The TypeScript version does the same thing with `interface`; Rust's `trait` is the direct equivalent for this purpose.

**Constructor injection is `Self::new(...)`.** There is no annotation, no container, no decorator. The "wiring" is just calling `new` with the collaborators. Rust resolves the dependency graph at compile time by type-checking those calls. If a dependency doesn't satisfy the trait bound, the program does not compile. A TypeScript container resolves the graph at *runtime* by reading reflection metadata, which is why mis-wiring a NestJS provider surfaces as a runtime error at boot, not a type error.

**Generics: `<C: Clock, S: UserStore>`.** The bounds `C: Clock` and `S: UserStore` say "any type that implements these traits." The compiler monomorphizes: `WelcomeService<FixedClock, OneUser>` and `WelcomeService<SystemClock, InMemoryUsers>` become two distinct, fully-specialized types in the binary, each with its calls inlined. This is *exactly* how a TypeScript generic reads, but the runtime behavior is the opposite of TypeScript's. TS erases generics (`WelcomeService<A>` and `WelcomeService<B>` are the same code at runtime), while Rust duplicates and specializes. Zero dispatch cost, larger binary.

**Trait objects: `Box<dyn Clock>`.** `dyn Clock` is a single concrete-at-runtime type: a fat pointer carrying (a) a pointer to the data and (b) a pointer to a vtable of the trait's methods. `Box<dyn Clock>` owns that data on the heap. One `WelcomeService` type handles every implementation, and `self.clock.now_unix()` performs a vtable lookup. This is the same machinery JavaScript uses for *every* method call; Rust just makes it explicit and opt-in. You pay one pointer indirection per call and one heap allocation per dependency, in exchange for deciding the implementation at runtime.

**The composition root.** Notice that the concrete types `SystemClock` and `InMemoryUsers` appear in exactly one place: `fn main` (or a dedicated `build_app()` function). Everything else is generic or trait-object code. That single location where the abstract graph is bound to concrete implementations is called the **composition root**. It is the manual, compile-checked equivalent of a DI container's module registration. Keeping it in one spot is the whole discipline; the rest of the codebase stays decoupled.

---

## Key Differences

| Concern | TypeScript / DI container | Rust generics | Rust trait objects |
| --- | --- | --- | --- |
| The "interface" | `interface` | `trait` bound | `dyn Trait` |
| Wiring resolved | At runtime (container) | At compile time | At construction time |
| Dispatch | Always dynamic | Static (monomorphized) | Dynamic (vtable) |
| Misconfiguration shows up | Runtime (app boot) | Compile error | Compile error |
| Per-call cost | Property lookup | None (often inlined) | One vtable indirection |
| Per-dependency cost | Object allocation | None extra | One heap allocation (`Box`) |
| Binary size | N/A | Grows per instantiation | Constant |
| Heterogeneous collection | Easy (all objects) | Hard (each is a type) | Easy (`Vec<Box<dyn T>>`) |
| Needs a framework? | Usually yes | No | No |

**Static vs. dynamic, restated for a TypeScript developer.** In TypeScript everything is dynamic dispatch and generics vanish at runtime, so you never make this choice. In Rust you choose: generics are the default because they are free at runtime and catch more at compile time; trait objects are the escape hatch for when you genuinely need runtime polymorphism (config-driven implementations, plugin systems, storing many different services in one collection).

**There is no container, and that is the point.** A TypeScript DI container exists to manage object lifetimes (singleton vs. transient), resolve transitive dependencies automatically, and avoid threading every dependency through every constructor by hand. Rust handles lifetimes with ownership and `Arc` (a shared dependency is an `Arc<dyn Trait>` you clone), resolves the graph with the type system, and threads dependencies explicitly. For most applications the explicit wiring in one `build_app()` function is clearer and entirely sufficient. Crates like `shaku` exist if you truly want container-style registration, but reach for them only when manual wiring genuinely hurts.

**The shareable form: `Arc<dyn Trait>`.** In a web service, the wired dependency graph is shared across many concurrent request handlers. The idiomatic shape there is `Arc<dyn Trait + Send + Sync>`: cheap to clone (just bumps a refcount), thread-safe, and runtime-swappable. You will see this in the Real-World Example below, and it is what axum's application state typically holds.

---

## Common Pitfalls

### Pitfall 1: reaching for a global `static mut` instead of injecting

A TypeScript developer used to module-level singletons may try to make the clock a global mutable variable. Rust pushes back hard, because a mutable global is a data race waiting to happen.

```rust
// does not compile (error[E0133]): a mutable global is unsafe to touch.
static mut CLOCK_OFFSET: u64 = 0;

fn now() -> u64 {
    CLOCK_OFFSET + 100 // reading a `static mut` requires `unsafe`
}

fn main() {
    println!("{}", now());
}
```

The real error:

```text
error[E0133]: use of mutable static is unsafe and requires unsafe block
 --> src/main.rs:5:5
  |
5 |     CLOCK_OFFSET + 100 // reading a `static mut` requires `unsafe`
  |     ^^^^^^^^^^^^ use of mutable static
  |
  = note: mutable statics can be mutated by multiple threads: aliasing violations or data races will cause undefined behavior
```

The fix is the entire lesson of this page: don't reach for a global, inject the clock as a dependency. The injected version is also the testable version — a global clock is the classic reason a test suite becomes flaky.

### Pitfall 2: a non-dyn-compatible trait used as a trait object

You can only build `Box<dyn Trait>` from a **dyn-compatible** (formerly "object-safe") trait. A trait with a generic method cannot be put behind a vtable, because the compiler would need an infinite number of vtable entries.

```rust
// does not compile (error[E0038]): generic method makes the trait not dyn-compatible.
trait Repository {
    fn save<T: std::fmt::Debug>(&self, item: T);
}

struct Service {
    repo: Box<dyn Repository>, // can't build a vtable for `save`
}

fn main() {
    let _ = Service { repo: todo!() };
}
```

The real error:

```text
error[E0038]: the trait `Repository` is not dyn compatible
 --> src/main.rs:7:15
  |
7 |     repo: Box<dyn Repository>, // can't build a vtable for `save`
  |               ^^^^^^^^^^^^^^ `Repository` is not dyn compatible
  |
note: for a trait to be dyn compatible it needs to allow building a vtable
      for more information, visit <https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>
 --> src/main.rs:3:8
  |
2 | trait Repository {
  |       ---------- this trait is not dyn compatible...
3 |     fn save<T: std::fmt::Debug>(&self, item: T);
  |        ^^^^ ...because method `save` has generic type parameters
  = help: consider moving `save` to another trait
```

Two fixes: either use **generics** for this dependency (`struct Service<R: Repository>`), which has no such restriction, or change the method to a non-generic signature (e.g. take `&dyn Debug` instead of `<T: Debug>`).

### Pitfall 3: forgetting `Send + Sync` on a shared dependency

The moment your wired graph crosses threads (any async web handler), every `Arc<dyn Trait>` dependency must be `Send + Sync`. If a concrete implementation holds a non-thread-safe value like `Rc`, it cannot satisfy that bound.

```rust
// does not compile (error[E0277]): Rc is not Sync, so RcCache can't be a shared dependency.
use std::rc::Rc;
use std::sync::Arc;

trait Cache: Send + Sync {
    fn get(&self, k: &str) -> Option<String>;
}

struct RcCache {
    inner: Rc<Vec<String>>, // Rc is single-threaded
}
impl Cache for RcCache {
    fn get(&self, _k: &str) -> Option<String> {
        self.inner.first().cloned()
    }
}

fn main() {
    let _c: Arc<dyn Cache> = Arc::new(RcCache { inner: Rc::new(vec![]) });
}
```

The real error (first of two):

```text
error[E0277]: `Rc<Vec<String>>` cannot be shared between threads safely
  --> src/main.rs:12:16
   |
12 | impl Cache for RcCache {
   |                ^^^^^^^ `Rc<Vec<String>>` cannot be shared between threads safely
   |
   = help: within `RcCache`, the trait `Sync` is not implemented for `Rc<Vec<String>>`
note: required because it appears within the type `RcCache`
  --> src/main.rs:9:8
   |
 9 | struct RcCache {
   |        ^^^^^^^
note: required by a bound in `Cache`
  --> src/main.rs:5:21
   |
 5 | trait Cache: Send + Sync {
   |                     ^^^^ required by this bound in `Cache`
```

The fix is to use the thread-safe counterpart — `Arc<Vec<String>>` instead of `Rc<Vec<String>>` — or, if the value is mutated, an `Arc<Mutex<...>>`. The compiler catches the mistake at the boundary instead of at 3 a.m. in production, which is exactly the "fearless concurrency" the type system is for.

---

## Best Practices

- **Default to generics; reach for trait objects when you need runtime choice.** Generics give zero-cost dispatch and the strongest compile-time guarantees. Use `Box<dyn Trait>` / `Arc<dyn Trait>` when the implementation is chosen at runtime, when you must store heterogeneous services in one collection, or when monomorphization would bloat the binary (a deeply generic graph instantiated many ways).
- **Keep a single composition root.** Bind abstract dependencies to concrete implementations in one `build_app()` (or `main`) function. Everything downstream stays generic or `dyn`. This is the manual equivalent of a container's module registration and keeps coupling in one auditable place.
- **Inject behaviors that touch the outside world.** Clocks, random number generators, the filesystem, network clients, databases, and the current time are the dependencies worth abstracting — they are what make code non-deterministic and hard to test. Don't over-abstract pure logic that has no side effects.
- **Use `Arc<dyn Trait + Send + Sync>` for shared, cross-thread graphs.** It is cheap to clone, thread-safe, and runtime-swappable — the standard shape for web application state.
- **Prefer borrowed `&dyn Trait` for short-lived, non-owning injection.** If the service does not outlive its dependencies and you want zero allocation, `struct Service<'a> { dep: &'a dyn Trait }` borrows instead of boxing.
- **Don't build a DI framework prematurely.** Manual constructor injection scales remarkably far in Rust. Only consider `shaku` or similar when the wiring genuinely becomes a maintenance burden.

---

## Real-World Example

A `Notifier` that depends on a clock, an email sender, and an audit log — the kind of service you would register as application state in an axum/tokio app. Dependencies are injected as `Arc<dyn Trait + Send + Sync>` so the graph is cheap to clone into every request handler and safe to share across threads. The `#[cfg(test)]` module shows the payoff: all three real dependencies are swapped for deterministic fakes, and the service code is never touched.

```rust playground
// Rust - dependencies injected as Arc<dyn Trait>, the shape used for shared
// application state in a web service. Compile-verified.
use std::sync::Arc;

// --- The dependencies, as traits. `Send + Sync` so the graph crosses threads. ---

trait Clock: Send + Sync {
    fn now_unix(&self) -> u64;
}

trait EmailSender: Send + Sync {
    fn send(&self, to: &str, body: &str) -> Result<(), String>;
}

trait AuditLog: Send + Sync {
    fn record(&self, line: String);
}

// --- The service owns its dependencies as shared trait objects. ---

#[derive(Clone)] // cheap: cloning an Arc is a refcount bump
struct Notifier {
    clock: Arc<dyn Clock>,
    email: Arc<dyn EmailSender>,
    audit: Arc<dyn AuditLog>,
}

impl Notifier {
    fn new(
        clock: Arc<dyn Clock>,
        email: Arc<dyn EmailSender>,
        audit: Arc<dyn AuditLog>,
    ) -> Self {
        Self { clock, email, audit }
    }

    fn notify(&self, to: &str, message: &str) -> Result<(), String> {
        let ts = self.clock.now_unix();
        let body = format!("[{ts}] {message}");
        self.email.send(to, &body)?;
        self.audit.record(format!("sent to {to} at {ts}"));
        Ok(())
    }
}

// --- Production implementations ---

struct SystemClock;
impl Clock for SystemClock {
    fn now_unix(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

struct SmtpSender {
    host: String,
}
impl EmailSender for SmtpSender {
    fn send(&self, to: &str, body: &str) -> Result<(), String> {
        // A real impl would open a connection to self.host here.
        println!("SMTP({}) -> {to}: {body}", self.host);
        Ok(())
    }
}

struct StdoutAudit;
impl AuditLog for StdoutAudit {
    fn record(&self, line: String) {
        println!("AUDIT: {line}");
    }
}

// --- Composition root: the ONE place that picks concrete implementations. ---

fn build_notifier() -> Notifier {
    Notifier::new(
        Arc::new(SystemClock),
        Arc::new(SmtpSender { host: "smtp.example.com".into() }),
        Arc::new(StdoutAudit),
    )
}

fn main() {
    let notifier = build_notifier();
    notifier.notify("ada@example.com", "Your build passed").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Deterministic fakes: a fixed clock, an email sink that records calls,
    // and a no-op audit. All three deps are swapped without touching Notifier.
    struct FixedClock(u64);
    impl Clock for FixedClock {
        fn now_unix(&self) -> u64 {
            self.0
        }
    }

    #[derive(Default)]
    struct SpyEmail {
        sent: Mutex<Vec<(String, String)>>,
    }
    impl EmailSender for SpyEmail {
        fn send(&self, to: &str, body: &str) -> Result<(), String> {
            self.sent.lock().unwrap().push((to.into(), body.into()));
            Ok(())
        }
    }

    struct NullAudit;
    impl AuditLog for NullAudit {
        fn record(&self, _line: String) {}
    }

    #[test]
    fn injects_clock_into_the_body_and_sends_once() {
        let email = Arc::new(SpyEmail::default());
        let notifier = Notifier::new(
            Arc::new(FixedClock(1000)),
            email.clone(), // keep a typed handle to inspect afterwards
            Arc::new(NullAudit),
        );

        notifier.notify("grace@example.com", "hi").unwrap();

        let sent = email.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "grace@example.com");
        assert_eq!(sent[0].1, "[1000] hi"); // the injected clock shaped the body
    }
}
```

**Real output of `cargo run`** (the timestamp reflects the wall clock):

```text
SMTP(smtp.example.com) -> ada@example.com: [1780382372] Your build passed
AUDIT: sent to ada@example.com at 1780382372
```

**Real output of `cargo test`:**

```text
running 1 test
.
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Because `email.clone()` is just an `Arc` clone, the test keeps a typed `Arc<SpyEmail>` handle *and* injects the same value as an `Arc<dyn EmailSender>`, so it can inspect `sent` after the fact. This is the Rust equivalent of grabbing a spy out of a TypeScript DI container after the system under test has run. Note also that the production code wired three real I/O dependencies and the test wired three fakes, yet `Notifier::notify` is byte-for-byte identical in both — that is the entire value of injection.

> **Tip:** For a hands-off alternative to writing the `SpyEmail`/`FixedClock` fakes by hand, the `mockall` crate generates a `MockEmailSender` from the trait, with per-test expectations (`.expect_send().times(1).returning(...)`). See [Section 13: Mocking](/13-testing/06-mocking/). Hand-written fakes are often clearer for simple traits; generated mocks shine when you need to assert on exact call sequences and arguments.

---

## Further Reading

- [The Rust Programming Language — Using Trait Objects That Allow for Values of Different Types](https://doc.rust-lang.org/book/ch18-02-trait-objects.html): official treatment of `dyn Trait`.
- [The Rust Reference — `dyn` compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility): the precise rules behind the E0038 in Pitfall 2.
- [Rust API Guidelines — Flexibility](https://rust-lang.github.io/api-guidelines/flexibility.html): when to accept generic vs. trait-object parameters.
- [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/) and [Section 09: Trait Bounds](/09-generics-traits/05-trait-bounds/): the dispatch mechanics underpinning this page.
- [Section 13: Mocking](/13-testing/06-mocking/): generating test doubles with `mockall` for injected traits.
- Related patterns in this section: [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/) (swapping an algorithm), [The Factory Pattern](/22-common-patterns/08-factory-pattern/) (constructing the concrete dependency), [The Builder Pattern](/22-common-patterns/00-builder-pattern/) (assembling a complex dependency), and [The Decorator Pattern in Rust](/22-common-patterns/06-decorator-pattern/) (wrapping an injected dependency to add behavior).
- [Section 22 overview](/22-common-patterns/) and the [ecosystem guide](/23-ecosystem/) for crates like `shaku` if you want container-style registration.

---

## Exercises

### Exercise 1: Swap a real dependency for a fake

**Difficulty:** Beginner

**Objective:** Practice constructor injection and the generic form of DI.

**Instructions:** Define a trait `PriceFeed { fn price(&self, symbol: &str) -> Option<f64>; }`. Write a `Portfolio<F: PriceFeed>` that is constructed with a feed and a `Vec<(String, f64)>` of `(symbol, shares)`. Add a method `total_value(&self) -> f64` that sums `shares * price` for every holding, treating a missing price as `0.0`. In `main`, inject a fake feed that returns a fixed price and print the total.

<details>
<summary>Solution</summary>

```rust playground
trait PriceFeed {
    fn price(&self, symbol: &str) -> Option<f64>;
}

struct Portfolio<F: PriceFeed> {
    feed: F,
    holdings: Vec<(String, f64)>,
}

impl<F: PriceFeed> Portfolio<F> {
    fn new(feed: F, holdings: Vec<(String, f64)>) -> Self {
        Self { feed, holdings }
    }

    fn total_value(&self) -> f64 {
        self.holdings
            .iter()
            .map(|(symbol, shares)| self.feed.price(symbol).unwrap_or(0.0) * shares)
            .sum()
    }
}

// A fake feed for development/testing: every symbol costs $10.
struct FlatFeed(f64);
impl PriceFeed for FlatFeed {
    fn price(&self, _symbol: &str) -> Option<f64> {
        Some(self.0)
    }
}

fn main() {
    let portfolio = Portfolio::new(
        FlatFeed(10.0),
        vec![("AAPL".to_string(), 3.0), ("MSFT".to_string(), 2.0)],
    );
    println!("total: {}", portfolio.total_value());
}
```

**Real output:**

```text
total: 50
```

</details>

### Exercise 2: Choose implementations at runtime with trait objects

**Difficulty:** Intermediate

**Objective:** Use `Box<dyn Trait>` to inject a dependency picked from a runtime value, and see why generics alone cannot express it.

**Instructions:** Define a trait `Notifier { fn notify(&self, msg: &str) -> String; }` with two implementations: `EmailNotifier` (returns `format!("email: {msg}")`) and `SmsNotifier` (returns `format!("sms: {msg}")`). Write a function `pick(kind: &str) -> Box<dyn Notifier>` that returns one or the other based on the string. In `main`, pick a notifier from a runtime value and call it. Explain in a comment why the return type must be `Box<dyn Notifier>` and not a generic.

<details>
<summary>Solution</summary>

```rust playground
trait Notifier {
    fn notify(&self, msg: &str) -> String;
}

struct EmailNotifier;
impl Notifier for EmailNotifier {
    fn notify(&self, msg: &str) -> String {
        format!("email: {msg}")
    }
}

struct SmsNotifier;
impl Notifier for SmsNotifier {
    fn notify(&self, msg: &str) -> String {
        format!("sms: {msg}")
    }
}

// The concrete type depends on a RUNTIME value, so the two branches return
// different types. A generic `-> impl Notifier` requires ONE concrete return
// type across all paths; only a trait object can unify the two branches.
fn pick(kind: &str) -> Box<dyn Notifier> {
    match kind {
        "sms" => Box::new(SmsNotifier),
        _ => Box::new(EmailNotifier),
    }
}

fn main() {
    let chosen = "sms"; // imagine this comes from config or a CLI flag
    let notifier = pick(chosen);
    println!("{}", notifier.notify("build passed"));
}
```

**Real output:**

```text
sms: build passed
```

</details>

### Exercise 3: Inject a clock to make time deterministic in tests

**Difficulty:** Advanced

**Objective:** Use dependency injection to eliminate the classic source of flaky tests — wall-clock time — by injecting a clock the test controls.

**Instructions:** Build a `RateLimiter<C: Clock>` that allows at most `max_in_window` calls per `window_secs` seconds. It takes an injected `Clock`, records the window start and a count using `Cell`, and exposes `allow(&self) -> bool` that returns `true` if the call is permitted and `false` otherwise, resetting the window when enough time has passed. Write a test using a `ManualClock` you can advance by hand, proving the limiter blocks after the cap and resets after the window — with no real sleeping.

<details>
<summary>Solution</summary>

```rust playground
use std::cell::Cell;

trait Clock {
    fn now_unix(&self) -> u64;
}

struct RateLimiter<C: Clock> {
    clock: C,
    window_secs: u64,
    max_in_window: u32,
    window_start: Cell<u64>,
    count: Cell<u32>,
}

impl<C: Clock> RateLimiter<C> {
    fn new(clock: C, window_secs: u64, max_in_window: u32) -> Self {
        let start = clock.now_unix();
        Self {
            clock,
            window_secs,
            max_in_window,
            window_start: Cell::new(start),
            count: Cell::new(0),
        }
    }

    fn allow(&self) -> bool {
        let now = self.clock.now_unix();
        if now >= self.window_start.get() + self.window_secs {
            self.window_start.set(now);
            self.count.set(0);
        }
        if self.count.get() < self.max_in_window {
            self.count.set(self.count.get() + 1);
            true
        } else {
            false
        }
    }
}

fn main() {
    println!("run `cargo test` to exercise the injected clock");
}

#[cfg(test)]
mod tests {
    use super::*;

    // A clock the test advances by hand — no sleeping, no flakiness.
    struct ManualClock {
        t: Cell<u64>,
    }
    impl ManualClock {
        fn new(start: u64) -> Self {
            Self { t: Cell::new(start) }
        }
        fn advance(&self, secs: u64) {
            self.t.set(self.t.get() + secs);
        }
    }
    // Implementing Clock for `&ManualClock` lets the test keep its own handle
    // to call `advance` while the limiter borrows the same clock.
    impl Clock for &ManualClock {
        fn now_unix(&self) -> u64 {
            self.t.get()
        }
    }

    #[test]
    fn limits_then_resets_after_the_window() {
        let clock = ManualClock::new(0);
        let limiter = RateLimiter::new(&clock, 60, 2);

        assert!(limiter.allow()); // 1
        assert!(limiter.allow()); // 2
        assert!(!limiter.allow()); // 3 -> blocked

        clock.advance(61); // jump past the window — no real sleep needed
        assert!(limiter.allow()); // window reset
    }
}
```

**Real output of `cargo test`:**

```text
running 1 test
.
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

</details>
