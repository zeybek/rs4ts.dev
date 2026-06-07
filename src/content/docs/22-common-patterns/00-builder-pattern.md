---
title: "The Builder Pattern"
description: "Assemble complex values step by step in Rust. Unlike a TypeScript fluent builder, an owned move builder makes reusing a half-built value a compile error."
---

The **builder pattern** assembles a complex value step by step, one named setter at a time, instead of forcing everything through a single positional constructor. If you have ever written a fluent `new HttpRequestBuilder(url).method("POST").header(...).build()` chain in TypeScript, you already know the shape. Rust uses the same idea, but the choice of *who owns the builder* (and the option to make required fields a **compile error** when missing) turns a convenience pattern into a genuine correctness tool.

---

## Quick Overview

A builder is a helper type that accumulates configuration through chained method calls and produces the finished value with a final `build()`. It matters to a TypeScript/JavaScript developer for two reasons. First, Rust has **no function overloading and no optional/named parameters**, so a struct with many optional fields has no ergonomic constructor. The builder fills that gap. Second, Rust lets you push correctness into the type system: an *owned* builder, a fallible `Result`-returning builder, or a *type-state* builder can each make "you forgot a required field" fail at compile time rather than at run time.

> **Note:** This file focuses purely on the builder pattern. The compile-time-state technique used here is covered in depth in [The Type-State Pattern](/22-common-patterns/02-type-state/); for wrapping a single field in a meaningful type see [The Newtype Pattern](/22-common-patterns/01-newtype/).

---

## TypeScript/JavaScript Example

A classic fluent builder in TypeScript. Each setter mutates `this` and returns `this`, so calls chain. The constructor takes the one truly required value (the URL); everything else has a default or is optional.

```typescript
// TypeScript - a fluent, mutate-this-and-return-this builder
class HttpRequestBuilder {
  private method = "GET";
  private headers: [string, string][] = [];
  private body: string | undefined;
  private timeoutMs = 30_000;

  constructor(private url: string) {}

  setMethod(method: string): this {
    this.method = method;
    return this;
  }
  header(name: string, value: string): this {
    this.headers.push([name, value]);
    return this;
  }
  setBody(body: string): this {
    this.body = body;
    return this;
  }
  timeout(ms: number): this {
    this.timeoutMs = ms;
    return this;
  }

  build() {
    return {
      method: this.method,
      url: this.url,
      headers: this.headers,
      body: this.body,
      timeoutMs: this.timeoutMs,
    };
  }
}

const req = new HttpRequestBuilder("https://api.example.com/users")
  .setMethod("POST")
  .header("Content-Type", "application/json")
  .header("Authorization", "Bearer token123")
  .setBody(JSON.stringify({ name: "Bob" }))
  .timeout(5_000)
  .build();

console.log(req);
```

Running this under Node v22 prints the assembled object:

```text
{
  method: 'POST',
  url: 'https://api.example.com/users',
  headers: [
    [ 'Content-Type', 'application/json' ],
    [ 'Authorization', 'Bearer token123' ]
  ],
  body: '{"name":"Bob"}',
  timeoutMs: 5000
}
```

Notice what TypeScript *cannot* do here: nothing stops you from calling `.build()` immediately after `new HttpRequestBuilder(url)` and skipping a step you meant to set. Required-ness lives in your head, not in the type. Rust offers ways to fix exactly that.

---

## Rust Equivalent

There are two idiomatic shapes for a Rust builder. The most common is the **owned (move) builder**: each setter takes `self` by value, mutates it, and returns it. Because the value is moved through the chain, the compiler guarantees you never accidentally reuse a half-finished builder.

```rust
// Rust - an owned (move) builder: each setter consumes `self` and returns it
#[derive(Debug)]
struct HttpRequest {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    timeout_ms: u64,
}

struct HttpRequestBuilder {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    timeout_ms: u64,
}

impl HttpRequestBuilder {
    // The one required value goes in `new`; everything else gets a default.
    fn new(url: impl Into<String>) -> Self {
        HttpRequestBuilder {
            method: "GET".to_string(),
            url: url.into(),
            headers: Vec::new(),
            body: None,
            timeout_ms: 30_000,
        }
    }

    fn method(mut self, method: impl Into<String>) -> Self {
        self.method = method.into();
        self
    }

    fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    // `build` consumes the builder one last time and yields the finished value.
    fn build(self) -> HttpRequest {
        HttpRequest {
            method: self.method,
            url: self.url,
            headers: self.headers,
            body: self.body,
            timeout_ms: self.timeout_ms,
        }
    }
}

fn main() {
    let req = HttpRequestBuilder::new("https://api.example.com/users")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer token123")
        .body(r#"{"name":"Bob"}"#)
        .timeout_ms(5_000)
        .build();

    println!("{req:#?}");
}
```

Running it prints the finished `HttpRequest`:

```text
HttpRequest {
    method: "POST",
    url: "https://api.example.com/users",
    headers: [
        (
            "Content-Type",
            "application/json",
        ),
        (
            "Authorization",
            "Bearer token123",
        ),
    ],
    body: Some(
        "{\"name\":\"Bob\"}",
    ),
    timeout_ms: 5000,
}
```

> **Tip:** Accepting `impl Into<String>` in setters is the idiomatic way to let callers pass either a `&str` literal or an owned `String` without ceremony. It is the closest Rust gets to TypeScript's "just pass a string."

---

## Detailed Explanation

### Why a builder at all in Rust?

In TypeScript a builder is mostly about *fluency* and *readability*; you could just pass an options object. Rust reaches for builders for a more structural reason: **there is no function overloading, no default parameter values, and no named arguments.** A struct with five fields, three of them optional, has exactly one way to be constructed with a literal, and that literal requires every field, in order, with no names at call sites that read well. A builder restores named, optional, order-independent configuration.

### The owned-builder mechanics, line by line

- `fn method(mut self, ...) -> Self` takes `self` **by value** (`self`, not `&self` or `&mut self`). The `mut` is on the *binding*, letting the body reassign fields; it is not part of the public signature. The method returns `Self`, so the chain continues on the value that was just moved out and back.
- Because each call moves the builder, the previous binding is consumed. You physically cannot touch a stale copy: there is only ever one live builder threading through the chain. This is ownership ([Section 05](/05-ownership/)) doing correctness work for free.
- `build(self)` is the terminal move. After it runs, the builder no longer exists; it has been transformed into the `HttpRequest`.

### The `&mut self` builder variant

The other common shape borrows the builder mutably instead of moving it. Each setter takes `&mut self` and returns `&mut Self`:

```rust
// Rust - a `&mut self` builder; the builder lives in a local you keep
#[derive(Debug, Default)]
struct ServerConfig {
    host: String,
    port: u16,
    workers: usize,
    tls: bool,
}

#[derive(Default)]
struct ServerConfigBuilder {
    host: Option<String>,
    port: Option<u16>,
    workers: Option<usize>,
    tls: bool,
}

impl ServerConfigBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn host(&mut self, host: impl Into<String>) -> &mut Self {
        self.host = Some(host.into());
        self
    }

    fn port(&mut self, port: u16) -> &mut Self {
        self.port = Some(port);
        self
    }

    fn tls(&mut self, enabled: bool) -> &mut Self {
        self.tls = enabled;
        self
    }

    // `build` borrows, so the builder can be reused or inspected afterward.
    fn build(&mut self) -> ServerConfig {
        ServerConfig {
            host: self.host.clone().unwrap_or_else(|| "127.0.0.1".to_string()),
            port: self.port.unwrap_or(8080),
            workers: self.workers.unwrap_or(4),
            tls: self.tls,
        }
    }
}

fn main() {
    let mut builder = ServerConfigBuilder::new();
    builder.host("0.0.0.0").port(443).tls(true);
    let config = builder.build();
    println!("{config:?}");
}
```

Output:

```text
ServerConfig { host: "0.0.0.0", port: 443, workers: 4, tls: true }
```

The difference is who keeps the builder. With `&mut self` the builder lives in a `let mut` binding you hold; the chain mutates it in place and `build()` *reads* from it (note the `.clone()` — since `build` only borrows, it cannot move the `String` out). This shape shines when the same builder is configured across several statements, or in `if`/`loop` blocks where moving on every branch would be awkward.

> **Warning:** A `&mut self` builder still works as a one-shot temporary as long as the chain ends in an owned-returning `build()`: the temporary builder lives to the end of the statement. What you cannot do is let the intermediate `&mut Self` borrow *escape* that temporary: bind it to a `let` that outlives the statement, or return it. That fails with `E0716`/`E0515` ("temporary value dropped while borrowed" / "cannot return value referencing temporary value"). Owned builders sidestep this entirely because they yield an owned value at every step. See **Common Pitfalls** below.

### Optional fields are just defaults

In both shapes, "optional" simply means the field has a sensible default the builder seeds (in `new` or via `#[derive(Default)]`), and the setter overrides it. Fields the *finished* type models as genuinely absent use `Option<T>` (like `body: Option<String>` above), so `None` is a first-class "not set" rather than a magic sentinel value.

### Required fields: runtime vs compile time

The owned and `&mut self` builders above make *every* field optional once you have a builder; `new` only demands the URL. If you have fields that must be supplied but cannot be defaulted, you have two real choices, covered in the next two sections: a **fallible `build()`** that returns `Result` (checked at run time), or a **type-state builder** that makes `build()` unavailable until the requirements are met (checked at compile time).

---

## Key Differences

| Aspect | TypeScript builder | Rust builder |
| --- | --- | --- |
| Receiver | Always mutates `this`, returns `this` | Choose **owned `self`** or **`&mut self`** |
| Stale-state reuse | Possible; not prevented | Owned builder *moves*, so reuse is a compile error |
| Optional params | Optional/default params or options object | No language defaults; builder seeds them |
| Missing required field | Runtime bug at best | Can be a **compile error** (type-state) or a `Result` |
| Overloading | Allowed | Not allowed; builder works around it |
| Generated boilerplate | Manual or libraries | Manual, or `bon` / `derive_builder` macros |

The headline difference is the move builder. In TypeScript a builder is a *style*. In Rust the owned builder additionally encodes a guarantee — once a step has been taken, the pre-step builder is gone — that the borrow checker enforces with zero runtime cost.

---

## Common Pitfalls

### Pitfall 1: reusing an owned builder after it was moved

A TypeScript habit is to keep a "base" builder and branch off it. With an owned builder, the first chain *consumes* the base:

```rust
// does not compile (error[E0382]: use of moved value: `base`)
struct Widget { width: u32, height: u32 }
struct WidgetBuilder { width: u32, height: u32 }
impl WidgetBuilder {
    fn new() -> Self { WidgetBuilder { width: 0, height: 0 } }
    fn width(mut self, w: u32) -> Self { self.width = w; self }
    fn height(mut self, h: u32) -> Self { self.height = h; self }
    fn build(self) -> Widget { Widget { width: self.width, height: self.height } }
}
fn main() {
    let base = WidgetBuilder::new().width(100);
    let a = base.height(50).build();
    let b = base.height(80).build(); // base was already moved by the first chain
    println!("{} {}", a.width, b.width);
}
```

The real compiler error:

```text
error[E0382]: use of moved value: `base`
  --> src/main.rs:12:13
   |
10 |     let base = WidgetBuilder::new().width(100);
   |         ---- move occurs because `base` has type `WidgetBuilder`, which does not implement the `Copy` trait
11 |     let a = base.height(50).build();
   |                  ---------- `base` moved due to this method call
12 |     let b = base.height(80).build(); // base was already moved by the first chain
   |             ^^^^ value used here after move
   |
note: `WidgetBuilder::height` takes ownership of the receiver `self`, which moves `base`
```

To branch off a shared base with an owned builder, derive `Clone` on the builder and clone before each branch (`base.clone().height(50)`), or use a `&mut self` builder instead.

### Pitfall 2: the `&mut self` builder's borrow outlives what you expect

If you bind the `&mut Self` a chain returns and then keep using the original builder, you get an aliasing conflict: the chain's borrow is still alive:

```rust
// does not compile (error[E0499]: cannot borrow `b` as mutable more than once at a time)
#[derive(Default)]
struct ConfigBuilder { port: u16, host: String }
impl ConfigBuilder {
    fn port(&mut self, p: u16) -> &mut Self { self.port = p; self }
    fn host(&mut self, h: &str) -> &mut Self { self.host = h.to_string(); self }
}
fn main() {
    let mut b = ConfigBuilder::default();
    let chained = b.port(8080).host("localhost"); // first &mut borrow, kept in `chained`
    b.port(9090);                                  // second &mut borrow while the first is live
    println!("{}", chained.port);
}
```

The real compiler error:

```text
error[E0499]: cannot borrow `b` as mutable more than once at a time
  --> src/main.rs:10:5
   |
 9 |     let chained = b.port(8080).host("localhost"); // first &mut borrow, kept in `chained`
   |                   - first mutable borrow occurs here
10 |     b.port(9090);                                  // second &mut borrow while the first is live
   |     ^ second mutable borrow occurs here
11 |     println!("{}", chained.port);
   |                    ------------ first borrow later used here
```

Do not store the intermediate `&mut Self`. Either chain through to a value (`let cfg = b.port(8080).host("localhost").build();`) or finish the chain on its own statement and then read `b` directly. This is exactly why owned builders compose more freely as one-shot expressions.

### Pitfall 3: forgetting to return `self` from a setter

Coming from TypeScript, where `return this;` is easy to drop, the equivalent slip in Rust is writing a setter body that ends in `;` after the field assignment without the final `self`. That makes the method return `()` instead of `Self`, and the *next* `.method(...)` in the chain fails to resolve because `()` has no such method. The fix is to ensure the last expression of the setter is `self` (no trailing semicolon on it).

---

## Best Practices

- **Default to the owned (move) builder.** It is the most common form, composes as a single expression, and gives you free protection against reusing a half-built value. Reach for `&mut self` only when the builder genuinely lives across multiple statements or branches.
- **Put truly required, non-defaultable values in `new` / the constructor function.** A builder's job is the *optional* surface. If exactly one value is mandatory (the URL above), require it up front and you avoid needing fallible builds at all.
- **Do not hand-roll a builder when the struct just needs defaults.** Often `#[derive(Default)]` plus struct-update syntax is enough and far less code:

```rust
// Rust - sometimes Default + struct-update beats a whole builder
#[derive(Debug)]
struct Settings {
    verbose: bool,
    retries: u32,
    color: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { verbose: false, retries: 3, color: true }
    }
}

fn main() {
    // Override only what differs; `..Default::default()` fills the rest.
    let custom = Settings { verbose: true, ..Settings::default() };
    println!("{custom:?}");
}
```

Output:

```text
Settings { verbose: true, retries: 3, color: true }
```

- **Reach for a derive macro before writing a large builder by hand.** The `bon` crate (current version 3.9.1) generates a builder that even checks required fields at compile time; `derive_builder` (0.20.2) is the long-standing alternative that produces a `Result`-returning `build`. Hand-write a builder only when you need behavior the macros do not give you, most often the type-state shape.
- **Name terminal and setter methods clearly.** `build()` (or `finish()`/`spawn()` when domain-appropriate) for the consumer; plain field names for setters. Avoid `set_`/`with_` prefixes unless they read better in your domain — Rust API guidelines prefer the bare field name.

---

## Real-World Example

In production you rarely hand-roll the boilerplate. The `bon` crate gives you a derive macro that produces a builder *and* makes missing required fields a compile error: no `Result`, no `unwrap`, no manual `Option` plumbing. Add it with `cargo add bon` (resolves to 3.9.1 on the current stable toolchain, Rust 1.96.0 / 2024 edition):

```toml
# Cargo.toml
[dependencies]
bon = "3"
```

```rust
// Rust - a derive-generated builder with required + optional + defaulted fields
use bon::Builder;

#[derive(Debug, Builder)]
struct Notification {
    // Required: a plain field with no default must be provided.
    // `#[builder(into)]` lets the setter accept anything that converts via Into,
    // so a `&str` literal works for a `String` field.
    #[builder(into)]
    recipient: String,
    #[builder(into)]
    subject: String,
    // Optional: an `Option<T>` field may be omitted; it defaults to `None`.
    #[builder(into)]
    cc: Option<String>,
    // Optional with an explicit default value.
    #[builder(default = 3)]
    retries: u32,
    // Optional, defaulting to the type's `Default` (`false`).
    #[builder(default)]
    urgent: bool,
}

fn main() {
    let n = Notification::builder()
        .recipient("ops@example.com")
        .subject("Deploy finished")
        .urgent(true)
        .build();

    println!("{n:#?}");
}
```

Output:

```text
Notification {
    recipient: "ops@example.com",
    subject: "Deploy finished",
    cc: None,
    retries: 3,
    urgent: true,
}
```

The payoff is that *forgetting a required field does not compile.* Dropping the `.subject(...)` call yields a clear, readable diagnostic from `bon` (excerpt; the real output also carries per-`note:` `--> src/main.rs` locations and a trailing "this error originates in the derive macro" note):

```text
error[E0277]: the member `Unset<subject>` was not set, but this method requires it to be set
 --> src/main.rs:9:66
  |
9 |     let n = Notification::builder().recipient("ops@example.com").build();
  |                                                                  ^^^^^ the member `Unset<subject>` was not set, but this method requires it to be set
  |
  = help: the trait `IsSet` is not implemented for `Unset<subject>`
note: required for `SetRecipient` to implement `IsComplete`
note: required by a bound in `NotificationBuilder::<S>::build`
```

Under the hood `bon` is generating the type-state machinery described in the next section. You get the strongest guarantee with the least code.

---

## Further Reading

- The Rust API Guidelines on builders: <https://rust-lang.github.io/api-guidelines/type-safety.html#builders-enable-construction-of-complex-values-c-builder>
- `bon` (compile-checked derive builders): <https://bon-rs.com/>
- `derive_builder` (Result-based derive builders): <https://docs.rs/derive_builder>
- The Rust Design Patterns book, builder chapter: <https://rust-unofficial.github.io/patterns/patterns/creational/builder.html>
- Related guide sections:
  - [The Type-State Pattern](/22-common-patterns/02-type-state/) — encoding "which required fields are set" in the type, the technique behind a compile-checked builder.
  - [The Newtype Pattern](/22-common-patterns/01-newtype/) — give individual builder fields safer, meaningful types.
  - [The Factory Pattern](/22-common-patterns/08-factory-pattern/) — associated functions like `Self::new` and other construction patterns.
  - [The Error-Propagation Pattern](/22-common-patterns/03-error-propagation/) — designing the error type a fallible `build()` should return.
  - [Ownership](/05-ownership/) — why moving `self` makes the owned builder safe.
  - [Ecosystem](/23-ecosystem/) — where `bon`, `derive_builder`, and friends fit in the wider crate ecosystem.

---

## Exercises

### Exercise 1: a move builder for a `Pizza`

**Difficulty:** Beginner

**Objective:** Practice the owned-builder shape with optional fields and a defaulted setter.

**Instructions:** Define a `Pizza { size: String, cheese: bool, toppings: Vec<String> }` and a `PizzaBuilder`. The size is required (take it in `new`); `cheese` defaults to `true`; `toppings` starts empty. Provide a `cheese(bool)` setter and a `topping(impl Into<String>)` setter that *appends* (so it can be called several times), plus `build()`. Construct a large pizza with no cheese and one `"mushroom"` topping and print it with `{:?}`.

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
struct Pizza {
    size: String,
    cheese: bool,
    toppings: Vec<String>,
}

struct PizzaBuilder {
    size: String,
    cheese: bool,
    toppings: Vec<String>,
}

impl PizzaBuilder {
    fn new(size: impl Into<String>) -> Self {
        PizzaBuilder {
            size: size.into(),
            cheese: true,
            toppings: Vec::new(),
        }
    }

    fn cheese(mut self, yes: bool) -> Self {
        self.cheese = yes;
        self
    }

    fn topping(mut self, t: impl Into<String>) -> Self {
        self.toppings.push(t.into());
        self
    }

    fn build(self) -> Pizza {
        Pizza {
            size: self.size,
            cheese: self.cheese,
            toppings: self.toppings,
        }
    }
}

fn main() {
    let pizza = PizzaBuilder::new("large")
        .cheese(false)
        .topping("mushroom")
        .build();
    println!("{pizza:?}");
}
```

Output:

```text
Pizza { size: "large", cheese: false, toppings: ["mushroom"] }
```

</details>

### Exercise 2: a fallible `Result` builder with required fields

**Difficulty:** Intermediate

**Objective:** Enforce required fields at *run time* by returning a `Result` from `build()`.

**Instructions:** Build an `ApiClient { base_url: String, api_key: String, timeout_ms: u64 }`. Store the in-progress fields as `Option`s in the builder. `base_url` and `api_key` are required; `timeout_ms` defaults to `30_000`. `build()` should return `Result<ApiClient, String>`, returning an `Err` naming the first missing required field. Show one successful build and one that omits `base_url`, printing both results.

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
struct ApiClient {
    base_url: String,
    api_key: String,
    timeout_ms: u64,
}

#[derive(Default)]
struct ApiClientBuilder {
    base_url: Option<String>,
    api_key: Option<String>,
    timeout_ms: Option<u64>,
}

impl ApiClientBuilder {
    fn base_url(mut self, v: impl Into<String>) -> Self {
        self.base_url = Some(v.into());
        self
    }
    fn api_key(mut self, v: impl Into<String>) -> Self {
        self.api_key = Some(v.into());
        self
    }
    fn timeout_ms(mut self, v: u64) -> Self {
        self.timeout_ms = Some(v);
        self
    }

    fn build(self) -> Result<ApiClient, String> {
        Ok(ApiClient {
            base_url: self.base_url.ok_or("base_url is required")?,
            api_key: self.api_key.ok_or("api_key is required")?,
            timeout_ms: self.timeout_ms.unwrap_or(30_000),
        })
    }
}

fn main() {
    let client = ApiClientBuilder::default()
        .base_url("https://api.example.com")
        .api_key("secret")
        .build();
    println!("{client:?}");

    let bad = ApiClientBuilder::default().api_key("secret").build();
    println!("{bad:?}");
}
```

Output:

```text
Ok(ApiClient { base_url: "https://api.example.com", api_key: "secret", timeout_ms: 30000 })
Err("base_url is required")
```

> The `?` operator turns each missing `Option` into an early `Err`. The error type here is a `String` for brevity; a real library would use a dedicated error enum — see [The Error-Propagation Pattern](/22-common-patterns/03-error-propagation/).

</details>

### Exercise 3: a compile-checked (type-state) `QueryBuilder`

**Difficulty:** Advanced

**Objective:** Make "you forgot the required `from(table)`" a *compile error* rather than a runtime check.

**Instructions:** Build a `Query { table: String, columns: Vec<String>, limit: Option<u32> }`. Use a `QueryBuilder<State>` with zero-sized marker types `NoTable` and `HasTable`. `QueryBuilder::new()` starts in `NoTable`; calling `.from(table)` returns a `QueryBuilder<HasTable>`. The optional setters `.select(col)` (appends) and `.limit(n)` should work in any state. Define `build()` **only** for `QueryBuilder<HasTable>`. Build a query selecting `"id"` and `"email"` from `"users"` with a limit of 10, and print it. (For the deeper theory, see [The Type-State Pattern](/22-common-patterns/02-type-state/).)

<details>
<summary>Solution</summary>

```rust
use std::marker::PhantomData;

struct NoTable;
struct HasTable;

#[derive(Debug)]
struct Query {
    table: String,
    columns: Vec<String>,
    limit: Option<u32>,
}

struct QueryBuilder<State> {
    table: Option<String>,
    columns: Vec<String>,
    limit: Option<u32>,
    _state: PhantomData<State>,
}

impl QueryBuilder<NoTable> {
    fn new() -> Self {
        QueryBuilder {
            table: None,
            columns: Vec::new(),
            limit: None,
            _state: PhantomData,
        }
    }

    // Setting the table is the only way into the `HasTable` state.
    fn from(self, table: impl Into<String>) -> QueryBuilder<HasTable> {
        QueryBuilder {
            table: Some(table.into()),
            columns: self.columns,
            limit: self.limit,
            _state: PhantomData,
        }
    }
}

// Optional setters are available in every state and keep the state unchanged.
impl<State> QueryBuilder<State> {
    fn select(mut self, col: impl Into<String>) -> Self {
        self.columns.push(col.into());
        self
    }
    fn limit(mut self, n: u32) -> Self {
        self.limit = Some(n);
        self
    }
}

// `build` exists ONLY once we are in `HasTable`.
impl QueryBuilder<HasTable> {
    fn build(self) -> Query {
        Query {
            table: self.table.unwrap(), // safe: `HasTable` guarantees it was set
            columns: self.columns,
            limit: self.limit,
        }
    }
}

fn main() {
    let q = QueryBuilder::new()
        .from("users")
        .select("id")
        .select("email")
        .limit(10)
        .build();
    println!("{q:?}");
}
```

Output:

```text
Query { table: "users", columns: ["id", "email"], limit: Some(10) }
```

Try deleting the `.from("users")` line: the value stays `QueryBuilder<NoTable>`, `build` is not in scope for that type, and compilation fails with `error[E0599]: no method named `build` found for struct `QueryBuilder<NoTable>``. The mistake is now impossible to ship.

</details>
