---
title: "The Decorator Pattern in Rust"
description: "Wrap a value and re-implement its trait to add caching, retries, or logging, via a zero-cost generic struct or Box<dyn Trait>, like a TypeScript decorator."
---

In TypeScript the **decorator pattern** means wrapping an object in another object that has the same interface, so the wrapper can add behavior — logging, caching, retries, buffering — without the wrapped object knowing. (This is the design pattern, not the TypeScript `@decorator` *syntax*, which is a different thing entirely. More on that below.) Rust supports the exact same idea: a wrapper type that holds an inner value and implements the same **trait**, forwarding each method through and adding behavior around the call. The twist is that Rust gives you two ways to hold the inner value: a `Box<dyn Trait>` (runtime, OO-shaped) or a generic type parameter `<S>` (compile-time, zero-cost), and the second is usually the idiomatic one. At the end we look at how the `tower` crate generalizes "a service wrapped by another service" into the reusable `Layer`/`Service` abstraction that powers `axum`'s middleware.

---

## Quick Overview

A **decorator** is a wrapper that:

- has the **same interface** as the thing it wraps (so callers can't tell the difference), and
- **adds behavior** around the wrapped object's methods (before, after, or instead of forwarding).

You stack decorators to compose behavior: `Retry(Cache(HttpClient))` is a client that retries, and each retry checks the cache first, and a cache miss hits the real network. Each layer is independent and reusable.

In Rust this is "a type that holds an inner value and implements the same trait." There are three encodings to know:

- **Trait-object decorator** (`inner: Box<dyn Trait>`): runtime composition, the closest match to the TypeScript shape; layers chosen from config, heterogeneous lists.
- **Generic decorator** (`inner: S` where `S: Trait`): compile-time composition, monomorphized to zero-cost static dispatch; the idiomatic default.
- **Function decorator** — a function that takes a closure and returns a wrapped closure; the lightest form when "the thing" is just a function.

> **Note:** This page is about wrapping types to add behavior. For *swapping* an algorithm (not wrapping it) see the sibling [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/); for *building* the wrapped values see [The Factory Pattern](/22-common-patterns/08-factory-pattern/). The `dyn`-vs-generics mechanics live in [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/) and [Section 09: Trait Bounds](/09-generics-traits/05-trait-bounds/).

---

## TypeScript/JavaScript Example

A classic data source that can be read and written, plus two decorators that transform the data on the way through. Each decorator implements the same `DataSource` interface and holds an `inner: DataSource`, so they nest freely.

```typescript
// TypeScript - the classic OO decorator pattern
interface DataSource {
  read(): string;
  write(data: string): void;
}

class FileSource implements DataSource {
  constructor(private contents: string) {}
  read(): string {
    return this.contents;
  }
  write(data: string): void {
    this.contents = data;
  }
}

// A decorator wraps an inner DataSource and forwards through it.
class UppercaseDecorator implements DataSource {
  constructor(private inner: DataSource) {}
  read(): string {
    return this.inner.read().toUpperCase();
  }
  write(data: string): void {
    this.inner.write(data.toUpperCase());
  }
}

class TrimDecorator implements DataSource {
  constructor(private inner: DataSource) {}
  read(): string {
    return this.inner.read().trim();
  }
  write(data: string): void {
    this.inner.write(data.trim());
  }
}

// Compose: trim first, then uppercase.
const source: DataSource = new UppercaseDecorator(
  new TrimDecorator(new FileSource("  hello world  ")),
);

console.log(source.read()); // "HELLO WORLD"
source.write("  changed  ");
console.log(source.read()); // "CHANGED"
```

Key properties of the TypeScript version: the decorator and the base share `interface DataSource`; the decorator stores `inner: DataSource` (a reference to *anything* implementing the interface); and you compose by passing one into another's constructor.

> **Warning:** Do not confuse this design pattern with TypeScript's `@decorator` *syntax* (`@Component`, `@Injectable`). Those are annotations applied to classes/methods, a metaprogramming feature, the rough analog of which in Rust is a **macro** ([Section 14: Macros](/14-macros/)), not the wrapping pattern on this page. The Rust equivalent of *this* pattern is wrapping a value and re-implementing its trait.

---

## Rust Equivalent

There are two idiomatic encodings. Start with the **trait-object** version because it maps one-to-one onto the TypeScript above, then see the **generic** version that Rust usually prefers.

### Version 1: trait-object decorator (the OO shape)

The inner value is a `Box<dyn DataSource>`, exactly like TypeScript's `inner: DataSource`: a pointer to *anything* implementing the trait, chosen at runtime.

```rust playground
trait DataSource {
    fn read(&self) -> String;
    fn write(&mut self, data: &str);
}

struct FileSource {
    contents: String,
}

impl DataSource for FileSource {
    fn read(&self) -> String {
        self.contents.clone()
    }
    fn write(&mut self, data: &str) {
        self.contents = data.to_string();
    }
}

// A decorator OWNS the inner source (a boxed trait object) and forwards through it.
struct UppercaseDecorator {
    inner: Box<dyn DataSource>,
}

impl DataSource for UppercaseDecorator {
    fn read(&self) -> String {
        self.inner.read().to_uppercase()
    }
    fn write(&mut self, data: &str) {
        self.inner.write(&data.to_uppercase());
    }
}

struct TrimDecorator {
    inner: Box<dyn DataSource>,
}

impl DataSource for TrimDecorator {
    fn read(&self) -> String {
        self.inner.read().trim().to_string()
    }
    fn write(&mut self, data: &str) {
        self.inner.write(data.trim());
    }
}

fn main() {
    let base = FileSource { contents: "  hello world  ".to_string() };
    // Wrap base in trim, then wrap that in uppercase. Layers compose.
    let mut source: Box<dyn DataSource> = Box::new(UppercaseDecorator {
        inner: Box::new(TrimDecorator { inner: Box::new(base) }),
    });

    println!("{}", source.read());
    source.write("  changed  ");
    println!("{}", source.read());
}
```

**Real output:**

```text
HELLO WORLD
CHANGED
```

This is the same behavior and the same shape as the TypeScript: each decorator implements `DataSource`, holds an `inner: Box<dyn DataSource>`, and forwards through it. Composition happens at runtime, and the concrete type is erased behind `dyn`.

### Version 2: generic decorator (static dispatch, zero-cost)

Make the inner type a **type parameter** instead of a boxed trait object. Now there is no `Box`, no heap allocation, and no vtable: the compiler monomorphizes each layer and can inline straight through the stack.

```rust playground
trait DataSource {
    fn read(&self) -> String;
}

struct FileSource {
    contents: String,
}

impl DataSource for FileSource {
    fn read(&self) -> String {
        self.contents.clone()
    }
}

// The inner source is a TYPE PARAMETER, so there is no Box and no vtable.
struct Uppercase<S: DataSource> {
    inner: S,
}

impl<S: DataSource> DataSource for Uppercase<S> {
    fn read(&self) -> String {
        self.inner.read().to_uppercase()
    }
}

struct Trim<S: DataSource> {
    inner: S,
}

impl<S: DataSource> DataSource for Trim<S> {
    fn read(&self) -> String {
        self.inner.read().trim().to_string()
    }
}

fn main() {
    let base = FileSource { contents: "  hello  ".to_string() };
    // The full type is Uppercase<Trim<FileSource>> -- known at compile time.
    let source = Uppercase { inner: Trim { inner: base } };
    println!("{}", source.read());

    // Proof there is no boxing: the concrete type is statically known.
    let _the_type: Uppercase<Trim<FileSource>> = source;
}
```

**Real output:**

```text
HELLO
```

The stacked value has the concrete type `Uppercase<Trim<FileSource>>`. The whole decorator chain is one statically-known type, so the optimizer treats the layers as ordinary function calls it can inline. This is the version to reach for first.

> **Tip:** Choose the encoding by asking "do I know the layers at compile time?" If yes (the common case), use generics. If the set of decorators is open or built from config at runtime, use `Box<dyn Trait>`. You can even mix them: a generic decorator wrapping a `Box<dyn DataSource>` base.

---

## Detailed Explanation

### A decorator implements the trait it wraps

The defining move — in both languages — is that the wrapper has the *same interface* as the wrapped value. In TypeScript: `class UppercaseDecorator implements DataSource`. In Rust: `impl DataSource for UppercaseDecorator`. Because the wrapper *is a* `DataSource`, callers that expect a `DataSource` accept it transparently, and you can wrap a decorator in another decorator without limit.

Each method does one of three things: forward to the inner value unchanged, transform the arguments before forwarding, or transform the result after forwarding. `read` forwards then post-processes (`.to_uppercase()`); `write` pre-processes then forwards (`&data.to_uppercase()`). That before/after symmetry is the whole pattern.

### `Box<dyn>` vs `<S>` — runtime vs compile-time composition

In the trait-object version, `inner: Box<dyn DataSource>` is a fat pointer (data pointer + vtable pointer). Every `self.inner.read()` is a virtual call dispatched through the vtable at runtime, exactly like a TypeScript method call on an interface-typed field. The concrete type is erased; you can store wildly different `DataSource`s in the same field, and you can build the stack from a loop or config.

In the generic version, `inner: S` *is* the concrete inner value, inlined into the struct's memory. `Uppercase<Trim<FileSource>>` is a single struct with the `FileSource`'s `String` nested two structs deep: no pointers, no heap, no vtable. The compiler generates a specialized `read` for that exact type and can inline the entire chain. The cost is that the full type is "spelled out" in your signatures, and you cannot put two *different* stacks in the same `Vec` without boxing.

This is the same static-vs-dynamic-dispatch trade-off you meet everywhere in Rust ([Section 09: Trait Objects](/09-generics-traits/06-trait-objects/)), applied to the inner field of a wrapper.

### You already use this pattern — in the standard library

Rust's I/O traits are built on decoration. `BufReader<R>` wraps **any** `R: Read` and adds buffering, and `BufReader<R>` is *itself* a `Read`, so it composes. Same for `BufWriter<W>`, `flate2`'s `GzEncoder<W>`, and so on: each wraps a `Read`/`Write` and is one too.

```rust playground
use std::io::{BufReader, Read};

fn main() {
    // `&[u8]` implements Read. BufReader<R> wraps any Read and adds buffering;
    // it is itself a Read, so it composes -- the std-library decorator pattern.
    let data: &[u8] = b"hello, decorators";
    let mut reader = BufReader::new(data);
    let mut out = String::new();
    reader.read_to_string(&mut out).unwrap();
    println!("read {} bytes: {out}", out.len());

    // The wrapped type is BufReader<&[u8]> -- static, zero-cost composition.
    fn assert_is_read<R: Read>(_: &R) {}
    let again = BufReader::new(b"x".as_slice());
    assert_is_read(&again);
}
```

**Real output:**

```text
read 17 bytes: hello, decorators
```

`BufReader::new` is the *generic* decorator pattern, verbatim: a wrapper type, parameterized by the type it wraps, implementing the same trait. When you see `BufReader<File>`, you are reading a decorator chain.

### Decoration vs inheritance

The TypeScript `extends`-and-`super` approach to "add behavior to a method" is *inheritance*; decoration is the composition alternative ("favor composition over inheritance"). Rust has **no inheritance at all**: there is no `extends`, no `super`, no base class. So in Rust, wrapping is *the* mechanism for layering behavior over an existing type, not one option among several. That makes the decorator pattern feel less like a "pattern" in Rust and more like the default way you build things up.

---

## Key Differences

| Aspect | TypeScript decorator | Rust decorator |
|---|---|---|
| Shared interface | `class W implements I` | `impl I for W` |
| Inner field | `private inner: I` (always a reference) | `Box<dyn I>` (runtime) **or** `S: I` (compile-time) |
| Dispatch | always dynamic (interface method call) | your choice: vtable (`dyn`) or monomorphized (`<S>`) |
| Cost of a layer | a heap object + a virtual call | `dyn`: a pointer + virtual call; `<S>`: **zero** (inlined) |
| Type after stacking | still `I` (erased) | `dyn`: erased; `<S>`: full type `Uppercase<Trim<...>>` |
| Heterogeneous list of stacks | trivial (`I[]`) | needs `Vec<Box<dyn I>>` |
| Inheritance available? | yes (`extends`/`super`), the alternative | **no**; decoration is the main tool |
| `@decorator` syntax relation | unrelated metaprogramming feature | the analog is a macro, not this pattern |

The headline difference is that Rust lets you keep the decorator pattern's *flexibility* while paying *none* of its runtime cost, by choosing the generic encoding. TypeScript's interface dispatch is always virtual.

---

## Common Pitfalls

### Pitfall 1: storing a bare `dyn Trait` in a field

A TypeScript developer writes `inner: DataSource` and expects the Rust field to be `inner: dyn DataSource`. But `dyn DataSource` is **unsized** (its size isn't known at compile time), so it can't live inline in a struct. You must put it behind a pointer (`Box<dyn DataSource>`, `&dyn DataSource`, `Rc<dyn DataSource>`) or make it a generic parameter.

```rust
trait DataSource {
    fn read(&self) -> String;
}

// does not compile (E0277): a bare `dyn` field is unsized.
struct Uppercase {
    inner: dyn DataSource,
}

fn main() {
    let _ = std::mem::size_of::<Uppercase>();
}
```

The real compiler error:

```text
error[E0277]: the size for values of type `(dyn DataSource + 'static)` cannot be known at compilation time
   --> src/main.rs:11:33
    |
 11 |     let _ = std::mem::size_of::<Uppercase>();
    |                                 ^^^^^^^^^ doesn't have a size known at compile-time
    |
    = help: within `Uppercase`, the trait `Sized` is not implemented for `(dyn DataSource + 'static)`
note: required because it appears within the type `Uppercase`
   --> src/main.rs:6:8
    |
  6 | struct Uppercase {
    |        ^^^^^^^^^
```

**Fix:** `inner: Box<dyn DataSource>` for the runtime version, or `struct Uppercase<S: DataSource> { inner: S }` for the generic version.

### Pitfall 2: reaching for `impl Trait` in the field type

The next instinct is `inner: impl DataSource`. But `impl Trait` is only allowed in function argument and return position, never in a struct field.

```rust
trait DataSource {
    fn read(&self) -> String;
}

// does not compile (E0562): `impl Trait` is not allowed in struct fields.
struct Uppercase {
    inner: impl DataSource,
}

fn main() {}
```

The real compiler error:

```text
error[E0562]: `impl Trait` is not allowed in field types
 --> src/main.rs:7:12
  |
7 |     inner: impl DataSource,
  |            ^^^^^^^^^^^^^^^
  |
  = note: `impl Trait` is only allowed in arguments and return types of functions and methods
```

**Fix:** a named generic parameter, `struct Uppercase<S: DataSource> { inner: S }`. That gives you the same "any inner type that implements `DataSource`" meaning that `impl Trait` *looks* like it should provide.

### Pitfall 3: forgetting that decorators are nested *types*, not just nested values

With the generic encoding, every wrap changes the type. If a function returns "a decorated source," you cannot write the return type as `DataSource`; you must either spell the full nested type, use `impl DataSource`, or box it. Trying to return two different stacks from the two arms of an `if` will fail to unify unless you box them into `Box<dyn DataSource>`. This is the same "mismatched-types from heterogeneous branches" issue covered in [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/); the fix is the same: box at the seam where the concrete type must be forgotten.

### Pitfall 4: assuming `dyn` decoration is "free" like in TypeScript

In TypeScript every method call is already a dynamic dispatch, so a `Box<dyn>` decorator chain *feels* identical. In Rust it is not free relative to the generic version: each `dyn` layer is a heap allocation plus a virtual call the optimizer usually cannot inline through. On a hot path, prefer the generic encoding. Use `dyn` deliberately, when you need runtime flexibility, not by default.

---

## Best Practices

- **Default to the generic encoding** (`struct Deco<S: Trait> { inner: S }`). It is zero-cost and composes by type. Reach for `Box<dyn Trait>` only when the layers are chosen at runtime or you need a heterogeneous collection of stacks.
- **Keep the trait small and focused.** Decorators must implement every method, so a fat trait makes every wrapper verbose. A narrow trait (one or two methods) keeps decorators short and makes them dyn-compatible for the boxed version.
- **Provide a `new` constructor** (`Deco::new(inner)`) so callers compose with `Retry::new(Cache::new(base))` instead of struct-literal nesting. See [The Factory Pattern](/22-common-patterns/08-factory-pattern/).
- **Hold shared per-decorator state with the right cell.** A caching decorator needs interior mutability behind `&self`; use `RefCell<T>` for single-threaded and `Mutex<T>`/`RwLock<T>` for shared-across-threads. See [Section 10: Smart Pointers](/10-smart-pointers/).
- **For middleware over a request/response service, do not hand-roll it; use `tower`.** `Layer`/`Service` is the community-standard, composable form of this pattern (next section).
- **Don't confuse the pattern with `#[derive]`/attribute macros.** If you actually want to *annotate* a type and generate code, that's a macro ([Section 14: Macros](/14-macros/)), not a wrapper type.

---

## Real-World Example: a caching + retrying HTTP client

A production-flavored client that layers two independent concerns over a base fetcher: a **cache** decorator that memoizes responses, and a **retry** decorator that re-attempts failed fetches. Each is a generic wrapper implementing the shared `Fetcher` trait; the cache uses `RefCell` for interior mutability behind `&self`.

```rust playground
use std::cell::RefCell;
use std::collections::HashMap;

// A simple synchronous "fetcher": given a URL, return a body or an error.
trait Fetcher {
    fn fetch(&self, url: &str) -> Result<String, String>;
}

// The base fetcher: pretend this hits the network.
struct HttpFetcher;
impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str) -> Result<String, String> {
        println!("[http] GET {url}");
        Ok(format!("body of {url}"))
    }
}

// Decorator: remember responses so repeated URLs skip the inner fetch.
struct Cached<F: Fetcher> {
    inner: F,
    cache: RefCell<HashMap<String, String>>,
}
impl<F: Fetcher> Cached<F> {
    fn new(inner: F) -> Self {
        Cached { inner, cache: RefCell::new(HashMap::new()) }
    }
}
impl<F: Fetcher> Fetcher for Cached<F> {
    fn fetch(&self, url: &str) -> Result<String, String> {
        if let Some(hit) = self.cache.borrow().get(url) {
            println!("[cache] hit for {url}");
            return Ok(hit.clone());
        }
        let body = self.inner.fetch(url)?;
        self.cache.borrow_mut().insert(url.to_string(), body.clone());
        Ok(body)
    }
}

// Decorator: retry the inner fetch up to `attempts` times on error.
struct Retry<F: Fetcher> {
    inner: F,
    attempts: u32,
}
impl<F: Fetcher> Fetcher for Retry<F> {
    fn fetch(&self, url: &str) -> Result<String, String> {
        let mut last = Err("never ran".to_string());
        for n in 1..=self.attempts {
            last = self.inner.fetch(url);
            if last.is_ok() {
                return last;
            }
            println!("[retry] attempt {n} failed");
        }
        last
    }
}

fn main() {
    // Compose: retry wraps caching wraps the real HTTP fetcher.
    let client = Retry {
        inner: Cached::new(HttpFetcher),
        attempts: 3,
    };

    println!("{:?}", client.fetch("/users/1"));
    println!("{:?}", client.fetch("/users/1")); // served from cache
    println!("{:?}", client.fetch("/users/2"));
}
```

**Real output:**

```text
[http] GET /users/1
Ok("body of /users/1")
[cache] hit for /users/1
Ok("body of /users/1")
[http] GET /users/2
Ok("body of /users/2")
```

The second `/users/1` is served from the cache without touching the HTTP layer: the cache decorator short-circuited before delegating. Each concern (caching, retry, transport) is a separate, testable, reusable type, and the whole client is the single static type `Retry<Cached<HttpFetcher>>` with no boxing.

---

## How `tower` generalizes this: `Layer` and `Service`

The decorator pattern shows up so often in network code — logging, timeouts, retries, rate limiting, auth, compression — that the Rust ecosystem standardized it. The [`tower`](https://docs.rs/tower) crate defines two traits:

- **`Service<Request>`**: "an async function from a request to a response," with `poll_ready` (backpressure) and `call`. This is the *thing being decorated*.
- **`Layer<S>`**: a factory that wraps a `Service` `S` and returns a new, decorated `Service`. This is the *decorator's constructor*.

A middleware is just a `Service` that holds an inner `Service` and adds behavior around `call`: the generic decorator pattern, applied to async request handlers. `axum`, `tonic`, `hyper`, and `reqwest` all speak `tower`, so a `Layer` you write works across the whole stack. Here is a logging middleware and a timing middleware stacked over a base service with `ServiceBuilder`:

First the dependencies (resolve and compile-verify in a probe project):

```toml
# Cargo.toml
[dependencies]
tower = { version = "0.5.3", features = ["util"] }
tokio = { version = "1.52", features = ["rt", "macros"] }
```

Or run `cargo add tower --features util` and `cargo add tokio --features rt,macros`.

```rust playground
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use tower::{Layer, Service, ServiceBuilder, ServiceExt};

// The base service: turn a request String into a response String.
#[derive(Clone)]
struct Greeter;

impl Service<String> for Greeter {
    type Response = String;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<String, Infallible>> + Send>>;
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, req: String) -> Self::Future {
        Box::pin(async move { Ok(format!("Hello, {req}!")) })
    }
}

// Decorator 1: log each request and when the inner service finishes.
#[derive(Clone)]
struct Logging<S> {
    inner: S,
}
impl<S> Service<String> for Logging<S>
where
    S: Service<String, Response = String> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }
    fn call(&mut self, req: String) -> Self::Future {
        println!("[log] request = {req:?}");
        // Clone-and-swap so the *ready* inner service is the one we call.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            let res = inner.call(req).await;
            println!("[log] done");
            res
        })
    }
}
// The Layer is the factory for the decorator: it knows how to wrap any S.
#[derive(Clone)]
struct LoggingLayer;
impl<S> Layer<S> for LoggingLayer {
    type Service = Logging<S>;
    fn layer(&self, inner: S) -> Logging<S> {
        Logging { inner }
    }
}

// Decorator 2: time how long the inner service took.
#[derive(Clone)]
struct Timing<S> {
    inner: S,
}
impl<S> Service<String> for Timing<S>
where
    S: Service<String, Response = String> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }
    fn call(&mut self, req: String) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            let start = Instant::now();
            let res = inner.call(req).await;
            let _elapsed = start.elapsed();
            println!("[time] inner service finished");
            res
        })
    }
}
#[derive(Clone)]
struct TimingLayer;
impl<S> Layer<S> for TimingLayer {
    type Service = Timing<S>;
    fn layer(&self, inner: S) -> Timing<S> {
        Timing { inner }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // ServiceBuilder stacks layers outside-in: Logging wraps Timing wraps Greeter.
    let mut service = ServiceBuilder::new()
        .layer(LoggingLayer)
        .layer(TimingLayer)
        .service(Greeter);

    let response = service
        .ready()
        .await
        .unwrap()
        .call("world".to_string())
        .await
        .unwrap();

    println!("final = {response}");
}
```

**Real output:**

```text
[log] request = "world"
[time] inner service finished
[log] done
final = Hello, world!
```

The structure is identical to the synchronous `Cached`/`Retry` decorators: each middleware is `struct M<S> { inner: S }` that implements the same trait (`Service`) and forwards through `inner` while adding behavior. The two new ideas `tower` adds are (1) the `Layer` trait, which packages "how to wrap" so a `ServiceBuilder` can stack decorators declaratively (outer-to-inner), and (2) the `poll_ready`/`Future` machinery for async backpressure. The `.layer(LoggingLayer).layer(TimingLayer)` call reads top-down as the order requests flow through: `Logging` sees the request first and the response last, just like `Retry(Cache(...))`.

> **Note:** The clone-and-`mem::replace` dance in `call` is the standard `tower` idiom: a `Service` may only be `call`ed after `poll_ready` returns `Ready`, and the readiness applies to *this* service instance, so middleware clones the inner service to keep a ready copy for the spawned future. In an `axum` app you almost never write this by hand; you use the ready-made layers from [`tower-http`](https://docs.rs/tower-http) (`TraceLayer`, `TimeoutLayer`, `CompressionLayer`, `CorsLayer`, …), which are exactly these decorators, written once. See [Section 16: Web APIs](/16-web-apis/) and [Section 23: Ecosystem](/23-ecosystem/).

---

## Further Reading

### Official documentation

- [The Rust Book — Trait Objects for Values of Different Types](https://doc.rust-lang.org/book/ch18-02-trait-objects.html): the `Box<dyn Trait>` decorator encoding
- [The Rust Book — Generic Data Types](https://doc.rust-lang.org/book/ch10-01-syntax.html) — the `struct Deco<S>` decorator encoding
- [`std::io::BufReader`](https://doc.rust-lang.org/std/io/struct.BufReader.html): the canonical decorator in the standard library
- [`tower::Service`](https://docs.rs/tower/latest/tower/trait.Service.html) and [`tower::Layer`](https://docs.rs/tower/latest/tower/trait.Layer.html) — the generalized middleware abstraction
- [`tower-http`](https://docs.rs/tower-http): ready-made `Service` decorators (tracing, timeout, compression, CORS)

### Related topics in this guide

- [Section 22 overview](/22-common-patterns/) — the full map of common patterns
- [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/): *swapping* an algorithm vs *wrapping* one; the dispatch trade-offs in depth
- [The Factory Pattern](/22-common-patterns/08-factory-pattern/) — `new` constructors and factories that build the wrapped values
- [The Newtype Pattern](/22-common-patterns/01-newtype/): a related "wrapper type" pattern, but for type safety rather than added behavior
- [RAII and Drop Guards](/22-common-patterns/10-raii-pattern/) — wrapper types whose added behavior runs on `Drop`
- [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/): `Box<dyn Trait>` mechanics and dynamic dispatch
- [Section 09: Trait Bounds](/09-generics-traits/05-trait-bounds/) — `<S: Trait>` static dispatch and monomorphization
- [Section 10: Smart Pointers](/10-smart-pointers/): `Box`, `Rc`, and `RefCell` for inner state
- [Section 14: Macros](/14-macros/) — the real Rust analog of TypeScript's `@decorator` *syntax*
- [Section 16: Web APIs](/16-web-apis/): `tower`/`axum` middleware in practice
- [Section 23: Ecosystem](/23-ecosystem/) — `tower`, `tower-http`, and the middleware ecosystem
- Foundations: [Getting Started](/01-getting-started/) and [Basics](/02-basics/)

---

## Exercises

### Exercise 1: stacked notifiers

**Difficulty:** Beginner

**Objective:** Build the basic decorator shape — a wrapper that implements the same trait as the thing it wraps.

**Instructions:** Define a `trait Notifier { fn send(&self, msg: &str) -> String; }` and a base `struct Base` whose `send` returns `format!("email: {msg}")`. Write a generic decorator `Urgent<N: Notifier>` that prepends `"[URGENT] "` to the message before delegating, and a generic decorator `AlsoSlack<N: Notifier>` that calls the inner notifier and then appends `"; slack: {msg}"` to the result. Compose `AlsoSlack { inner: Urgent { inner: Base } }` and send `"disk full"`.

<details>
<summary>Solution</summary>

```rust playground
trait Notifier {
    fn send(&self, msg: &str) -> String;
}

struct Base;
impl Notifier for Base {
    fn send(&self, msg: &str) -> String {
        format!("email: {msg}")
    }
}

// Prepend a tag, then delegate.
struct Urgent<N: Notifier> {
    inner: N,
}
impl<N: Notifier> Notifier for Urgent<N> {
    fn send(&self, msg: &str) -> String {
        self.inner.send(&format!("[URGENT] {msg}"))
    }
}

// Delegate, then add a Slack copy of the message it was handed.
struct AlsoSlack<N: Notifier> {
    inner: N,
}
impl<N: Notifier> Notifier for AlsoSlack<N> {
    fn send(&self, msg: &str) -> String {
        let primary = self.inner.send(msg);
        format!("{primary}; slack: {msg}")
    }
}

fn main() {
    let n = AlsoSlack { inner: Urgent { inner: Base } };
    println!("{}", n.send("disk full"));
}
```

**Real output:**

```text
email: [URGENT] disk full; slack: disk full
```

Note how each layer sees the message at *its* point in the chain: `AlsoSlack` forwards the raw `"disk full"`, which `Urgent` tags before it reaches `Base`, while the Slack copy uses the untagged message `AlsoSlack` was handed.

</details>

### Exercise 2: a counting decorator with interior mutability

**Difficulty:** Intermediate

**Objective:** Add per-decorator state that mutates behind a shared `&self`, the way a real cache or metrics layer does.

**Instructions:** Define `trait Source { fn value(&self) -> i64; }` and a base `struct Const(i64)`. Write a generic decorator `Counting<S: Source>` that counts how many times `value` is called and still returns the inner value. Because `value` takes `&self`, you cannot use a plain `u32` field; use `std::cell::Cell<u32>`. Give it a `Counting::new(inner)` constructor. Call `value` three times and print the final value and call count.

<details>
<summary>Solution</summary>

```rust playground
use std::cell::Cell;

trait Source {
    fn value(&self) -> i64;
}

struct Const(i64);
impl Source for Const {
    fn value(&self) -> i64 {
        self.0
    }
}

struct Counting<S: Source> {
    inner: S,
    calls: Cell<u32>, // interior mutability: mutate through &self
}
impl<S: Source> Counting<S> {
    fn new(inner: S) -> Self {
        Counting { inner, calls: Cell::new(0) }
    }
}
impl<S: Source> Source for Counting<S> {
    fn value(&self) -> i64 {
        self.calls.set(self.calls.get() + 1);
        self.inner.value()
    }
}

fn main() {
    let s = Counting::new(Const(42));
    s.value();
    s.value();
    println!("value={}, calls={}", s.value(), s.calls.get());
}
```

**Real output:**

```text
value=42, calls=3
```

`Cell<u32>` lets the decorator track state while keeping the `&self` signature that the trait requires, the same trick a caching decorator uses (it just stores a `RefCell<HashMap<...>>` instead). For a thread-safe version you would reach for `AtomicU32` or `Mutex<u32>`.

</details>

### Exercise 3: a function decorator

**Difficulty:** Intermediate

**Objective:** Apply the decorator idea to a *function* instead of an object — the lightest form of the pattern, and the one closest to JavaScript's higher-order functions.

**Instructions:** Write `fn with_logging<F>(handler: F) -> impl Fn(&str) -> String where F: Fn(&str) -> String`. It should return a *new* closure that prints the request, calls the wrapped `handler`, prints the result, and returns it. Decorate a handler `|name| format!("Hi {name}")` and call the result with `"Ada"`.

<details>
<summary>Solution</summary>

```rust playground
// `with_logging` takes any handler closure and returns a wrapped one.
fn with_logging<F>(handler: F) -> impl Fn(&str) -> String
where
    F: Fn(&str) -> String,
{
    move |req: &str| {
        println!("[log] handling {req:?}");
        let res = handler(req);
        println!("[log] -> {res:?}");
        res
    }
}

fn main() {
    let handler = with_logging(|name: &str| format!("Hi {name}"));
    println!("{}", handler("Ada"));
}
```

**Real output:**

```text
[log] handling "Ada"
[log] -> "Hi Ada"
Hi Ada
```

This is the same shape as a JavaScript `withLogging(fn)` that returns a wrapping function: the wrapper closure `move`s the inner handler in and runs behavior around it. When "the thing being decorated" is just a function, this is far lighter than a trait and a wrapper struct. (`tower`'s `Service` is the async, backpressure-aware generalization of exactly this.)

</details>
