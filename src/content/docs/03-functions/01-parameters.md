---
title: "Function Parameters"
description: "Rust has no default, optional, or rest parameters like TypeScript. Map each one to its idiomatic replacement: Option, slices, Default structs, builders, and traits."
---

In TypeScript and JavaScript you reach for default parameters, rest parameters, and optional `?` arguments without a second thought. Rust has **none** of these features at the language level, and yet every one of those patterns has a clean, idiomatic Rust equivalent. This page maps each familiar TypeScript parameter trick to the Rust way of doing it.

---

## Quick Overview

Rust functions take a fixed number of typed, **positional** parameters: no defaults, no optional `?` markers, no `...rest`, and no overloading by signature. Instead, Rust expresses those needs through the type system: `Option<T>` for "maybe a value," slices (`&[T]`) for "zero or more," dedicated structs (often with `Default`) and the **builder pattern** for "lots of optional knobs," and **traits** for "the same operation on different argument types." Once you internalize this, function signatures become a precise, self-documenting contract.

---

## TypeScript/JavaScript Example

Here is a typical TypeScript module that leans on every flexible-parameter feature the language offers:

```typescript
// notifications.ts

// 1. Default parameter
function greet(name: string, greeting: string = "Hello"): string {
  return `${greeting}, ${name}!`;
}

// 2. Optional parameter (`?`)
function connect(host: string, port?: number): void {
  const resolvedPort = port ?? 8080;
  console.log(`Connecting to ${host}:${resolvedPort}`);
}

// 3. Rest parameters — "zero or more"
function sumAll(...numbers: number[]): number {
  return numbers.reduce((acc, n) => acc + n, 0);
}

// 4. Options object — a big bag of optional settings
interface EmailOptions {
  cc?: string[];
  replyTo?: string;
  priority?: "low" | "normal" | "high";
}

function sendEmail(
  to: string[],
  subject: string,
  body: string,
  options: EmailOptions = {},
): void {
  const priority = options.priority ?? "normal";
  console.log(`To: ${to.join(", ")} | ${subject} [${priority}]`);
}

greet("Ada"); // "Hello, Ada!"
greet("Ada", "Welcome"); // "Welcome, Ada!"
connect("localhost"); // port defaults to 8080
sumAll(1, 2, 3, 4, 5); // 15
sendEmail(["ada@example.com"], "Hi", "Body"); // options optional
```

**What this relies on:**

- A **default parameter** (`greeting = "Hello"`).
- An **optional parameter** (`port?`) plus the nullish-coalescing operator `??`.
- **Rest parameters** (`...numbers`) gathering a variable number of arguments.
- An **options object** so the caller only sets the fields they care about.

---

## Rust Equivalent

Rust has no syntax for any of those. Each one maps to a deliberate, type-driven pattern instead:

```rust playground
// 1. Default parameter  ->  Option<T> + unwrap_or
fn greet(name: &str, greeting: Option<&str>) -> String {
    let greeting = greeting.unwrap_or("Hello");
    format!("{greeting}, {name}!")
}

// 2. Optional parameter  ->  Option<T> (same idea)
fn connect(host: &str, port: Option<u16>) {
    let port = port.unwrap_or(8080);
    println!("Connecting to {host}:{port}");
}

// 3. Rest parameters  ->  a slice &[T]
fn sum_all(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}

// 4. Options object  ->  a struct with Default
#[derive(Debug, Default)]
struct EmailOptions {
    cc: Vec<String>,
    reply_to: Option<String>,
    priority: Priority,
}

#[derive(Debug, Default)]
enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

fn send_email(to: &[&str], subject: &str, body: &str, options: EmailOptions) {
    let _ = body;
    println!("To: {} | {subject} [{:?}]", to.join(", "), options.priority);
}

fn main() {
    println!("{}", greet("Ada", None));          // "Hello, Ada!"
    println!("{}", greet("Ada", Some("Welcome")));// "Welcome, Ada!"

    connect("localhost", None);                   // port defaults to 8080

    println!("{}", sum_all(&[1, 2, 3, 4, 5]));    // 15

    send_email(
        &["ada@example.com"],
        "Hi",
        "Body",
        EmailOptions::default(),                  // all options at their defaults
    );
}
```

**Verified output:**

```text
Hello, Ada!
Welcome, Ada!
Connecting to localhost:8080
15
To: ada@example.com | Hi [Normal]
```

> **Note:** Rust forces the caller to *say something* for every parameter, even if that something is `None` or `Default::default()`. This verbosity is the price of having no hidden "magic" defaults; in exchange, a function's full set of inputs is always visible at the call site.

---

## Detailed Explanation

### Default and optional parameters become `Option<T>`

In TypeScript, `port?: number` and `port: number = 8080` both let the caller omit the argument. Rust has no "omit," so the absence is encoded *in the type* with `Option<T>`. The caller passes `None` to mean "not provided" and `Some(value)` otherwise, and the function decides the fallback:

```rust playground
fn connect(retries: Option<u32>, timeout: Option<u64>) {
    // Match handles the present/absent cases explicitly...
    let retries = match retries {
        Some(n) => n,
        None => 3,
    };
    // ...or use a combinator for the common "default value" case.
    let timeout = timeout.unwrap_or(30);
    println!("retries={retries}, timeout={timeout}");
}

fn main() {
    connect(None, None);            // retries=3, timeout=30
    connect(Some(5), Some(60));     // retries=5, timeout=60
    connect(Some(0), None);         // retries=0, timeout=30
}
```

This runs and prints exactly:

```text
retries=3, timeout=30
retries=5, timeout=60
retries=0, timeout=30
```

Two things worth noticing:

- `unwrap_or(30)` is the closest thing to TypeScript's `port ?? 8080`. (We cover `Option` and its combinators in depth in [Error Handling](/08-error-handling/).)
- `Some(0)` is meaningfully different from `None` — the third call keeps `retries = 0`. In JavaScript, `port ?? 8080` correctly preserves `0` too (because `0 != null`), but a sloppy `port || 8080` would wrongly replace `0` with `8080`. Rust's `Option` has **no** such footgun: only `None` triggers the default.

### Rest parameters become slices

TypeScript's `...numbers: number[]` collects trailing arguments into an array. Rust passes a **slice** — a borrowed view `&[T]` over contiguous elements — so any array or `Vec` works without copying:

```rust playground
fn sum_all(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}

fn main() {
    println!("{}", sum_all(&[1, 2, 3, 4, 5])); // array literal -> 15
    let v = vec![10, 20, 30];
    println!("{}", sum_all(&v));               // &Vec coerces to &[i32] -> 60
    let arr = [7; 3];
    println!("{}", sum_all(&arr));             // [7, 7, 7] -> 21
}
```

The trade-off is at the call site: instead of `sumAll(1, 2, 3)` you write `sum_all(&[1, 2, 3])`. You give up the comma-separated ergonomics but gain a single, explicit, allocation-free parameter. Slices are central to Rust; we explore them more in [Collections](/07-collections/).

### Options objects become structs with `Default`

When a function has many optional settings, bundle them into a struct and derive `Default`. The caller constructs the struct, overriding only the fields they care about and filling the rest with **struct update syntax** (`..Default::default()`):

```rust playground
#[derive(Debug)]
struct ServerConfig {
    host: String,
    port: u16,
    max_connections: u32,
    tls: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 8080,
            max_connections: 100,
            tls: false,
        }
    }
}

fn start_server(config: ServerConfig) {
    println!(
        "Listening on {}:{} (max={}, tls={})",
        config.host, config.port, config.max_connections, config.tls
    );
}

fn main() {
    // Override just two fields, keep the rest at their defaults.
    start_server(ServerConfig {
        port: 3000,
        tls: true,
        ..Default::default()
    });

    start_server(ServerConfig::default());
}
```

Output:

```text
Listening on 127.0.0.1:3000 (max=100, tls=true)
Listening on 127.0.0.1:8080 (max=100, tls=false)
```

The `..Default::default()` line is the Rust analog of spreading defaults into an options object (`{ ...defaults, port: 3000 }`). Here we wrote `Default` by hand to show non-trivial defaults; when every field's default is also *its type's* default (empty `Vec`, `None`, `0`, `false`), you can simply `#[derive(Default)]` instead, which is exactly what the `EmailOptions` struct above does.

> **Tip:** `#[default]` on an enum variant (as on `Priority::Normal`) lets enums participate in `#[derive(Default)]`. This has been stable since Rust 1.62.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Default value | `function f(x = 10)` | `Option<T>` + `unwrap_or(10)`, or a `Default` struct field |
| Optional argument | `function f(x?: number)` | `Option<T>` parameter; caller passes `None` |
| Nullish fallback | `x ?? fallback` | `opt.unwrap_or(fallback)` / `unwrap_or_else(..)` |
| Variable arity ("rest") | `...args: T[]` | `&[T]` slice (or `Vec<T>` to take ownership) |
| Options bag | optional object literal | `struct` + `#[derive(Default)]` + `..Default::default()` |
| Many-step construction | options object | **builder pattern** (chained methods returning `Self`) |
| Overload by argument type | declaration merging / union types | **trait** bound (`fn log<T: Describe>(x: T)`) |
| Named arguments | object destructuring `{ a, b }` | not supported; use a struct |
| True variadics | `...args` | `macro_rules!` (e.g. `println!`), not functions |

The unifying idea: **TypeScript bends the call syntax; Rust bends the type.** Optionality, variadicity, and overloading are all encoded into a parameter's *type* rather than into special calling conventions. That keeps every Rust function a plain, monomorphic, statically-dispatched target with one obvious signature.

### "Overloading" via traits

Rust has no function overloading: you cannot declare `fn log(x: i32)` and `fn log(x: &str)` in the same scope. The idiomatic substitute is a **trait** that both types implement, plus one generic function:

```rust playground
trait Describe {
    fn describe(&self) -> String;
}

impl Describe for i32 {
    fn describe(&self) -> String {
        format!("the integer {self}")
    }
}

impl Describe for &str {
    fn describe(&self) -> String {
        format!("the string {self:?}")
    }
}

impl Describe for bool {
    fn describe(&self) -> String {
        format!("the boolean {self}")
    }
}

// One generic function accepts any type that implements Describe.
fn log<T: Describe>(value: T) {
    println!("Logging {}", value.describe());
}

fn main() {
    log(42);
    log("hello");
    log(true);
}
```

Output:

```text
Logging the integer 42
Logging the string "hello"
Logging the boolean true
```

Unlike TypeScript overloads (which are erased at runtime and resolved purely by the type checker), Rust **monomorphizes** `log`: the compiler generates a separate specialized copy for each concrete `T` actually used. Traits and generics get a full treatment in [Generics & Traits](/09-generics-traits/).

### Accepting "either type" with `impl Into<T>`

A lighter-weight form of polymorphic parameter is `impl Trait` in argument position. The classic case is "accept either a `&str` or an owned `String`":

```rust playground
// `impl Into<String>` accepts anything convertible into a String.
fn add_tag(tags: &mut Vec<String>, tag: impl Into<String>) {
    tags.push(tag.into());
}

fn main() {
    let mut tags: Vec<String> = Vec::new();
    add_tag(&mut tags, "rust");              // &str
    add_tag(&mut tags, String::from("web")); // String
    println!("{tags:?}");                    // ["rust", "web"]
}
```

`tag: impl Into<String>` is shorthand for a generic `<T: Into<String>>` parameter. It is a common ergonomic choice in library APIs because the caller never has to think about whether they're holding a borrowed or owned string.

---

## Common Pitfalls

### Pitfall 1: Expecting default parameters to exist

A TypeScript developer's muscle memory says "I'll just leave that argument off." Rust rejects the call:

```rust
fn greet(name: &str, greeting: &str) -> String {
    format!("{greeting}, {name}!")
}

fn main() {
    println!("{}", greet("Ada")); // missing the second argument
}
```

Real compiler error:

```text
error[E0061]: this function takes 2 arguments but 1 argument was supplied
 --> src/main.rs:6:20
  |
6 |     println!("{}", greet("Ada"));
  |                    ^^^^^------- argument #2 of type `&str` is missing
  |
note: function defined here
 --> src/main.rs:1:4
  |
1 | fn greet(name: &str, greeting: &str) -> String {
  |    ^^^^^             --------------
help: provide the argument
  |
6 |     println!("{}", greet("Ada", /* &str */));
  |                               ++++++++++++
```

The fix is to design the parameter as `Option<&str>` (and pass `None`) or to give the caller an overload-free default via a wrapper function.

### Pitfall 2: Writing a default value in the signature

Trying to port `greeting: &str = "Hello"` directly does not even parse:

```rust
fn greet(name: &str, greeting: &str = "Hello") -> String {
    format!("{greeting}, {name}!")
}
```

Real compiler error (first of several):

```text
error: expected parameter name, found `=`
 --> src/main.rs:1:37
  |
1 | fn greet(name: &str, greeting: &str = "Hello") -> String {
  |                                     ^ expected parameter name
```

There is no syntax for default argument values in Rust. Reach for `Option<T>`, a `Default` struct, or a builder instead.

### Pitfall 3: Forgetting to borrow when passing a `Vec` to a slice parameter

A slice parameter (`&[T]`) needs a *reference*. Passing a `Vec` by value is a type mismatch:

```rust
fn sum_all(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}

fn main() {
    let v = vec![1, 2, 3];
    println!("{}", sum_all(v)); // forgot the &
}
```

Real compiler error:

```text
error[E0308]: mismatched types
 --> src/main.rs:7:28
  |
7 |     println!("{}", sum_all(v)); // forgot the &
  |                    ------- ^ expected `&[i32]`, found `Vec<{integer}>`
  |                    |
  |                    arguments to this function are incorrect
  |
  = note: expected reference `&[i32]`
                found struct `Vec<{integer}>`
note: function defined here
 --> src/main.rs:1:4
  |
1 | fn sum_all(numbers: &[i32]) -> i32 {
  |    ^^^^^^^ ---------------
help: consider borrowing here
  |
7 |     println!("{}", sum_all(&v)); // forgot the &
  |                            +
```

The compiler even suggests the fix: add `&`. Rust will *deref-coerce* `&Vec<i32>` to `&[i32]`, but it will not insert the `&` for you. (Why borrowing matters is the subject of [Ownership](/05-ownership/).)

### Pitfall 4: Treating `None` and `Some(0)` the same

Because `Option` is a real enum, a present-but-zero value is distinct from an absent value, exactly the distinction JavaScript's `||` operator loses. Decide up front whether you want `unwrap_or(default)` (only `None` falls back) or genuinely want to coalesce zero too. The former is almost always correct and is one reason `Option` is safer than nullable numbers.

---

## Best Practices

- **Use `Option<T>` for one or two optional parameters.** It is the lightest tool and reads clearly at the call site (`f(x, None)`).
- **Prefer `unwrap_or` / `unwrap_or_else` / `unwrap_or_default`** over a hand-written `match` when you just need a fallback value. Use `unwrap_or_else` when the default is expensive to compute, so it is only built when actually needed.
- **Reach for a `struct` with `#[derive(Default)]`** once a function grows three or more optional settings. Name the struct after the operation (`EmailOptions`, `ServerConfig`); it doubles as documentation.
- **Use the builder pattern** when construction is multi-step, has interdependent fields, or benefits from validation in `build()`. Builders read fluently and keep required-vs-optional fields obvious.
- **Use slices `&[T]`** for "borrow zero-or-more" and `Vec<T>` only when the function must *own* the elements. A `&[T]` parameter is strictly more flexible: it accepts arrays, `Vec`s, and sub-slices alike.
- **Use `impl Into<String>` (or `impl AsRef<str>`)** in public APIs that take strings, so callers can pass either `&str` or `String` without ceremony.
- **Model overloading with a trait**, not with several same-named functions (which Rust forbids anyway). One generic function plus per-type `impl` blocks is the idiom.
- **Avoid the temptation to overuse `impl Trait` everywhere**: concrete types keep error messages and signatures simpler. Add abstraction only where callers genuinely need the flexibility.

---

## Real-World Example

A small email-sending API that combines several techniques: required fields are positional parameters, recipients use a slice (the "rest params" stand-in), the subject accepts both `&str` and `String` via `impl Into<String>`, and the grab-bag of optional settings lives in a `Default` struct.

```rust playground
#[derive(Debug, Default)]
struct EmailOptions {
    cc: Vec<String>,
    reply_to: Option<String>,
    priority: Priority,
}

#[derive(Debug, Default)]
enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

/// Send an email.
///
/// `to` is one-or-more recipients (slice = "rest" parameter).
/// `subject` accepts either a `&str` or an owned `String`.
/// `options` bundles everything optional, defaulting via `EmailOptions::default()`.
fn send_email(to: &[&str], subject: impl Into<String>, body: &str, options: EmailOptions) {
    let subject = subject.into();
    println!("To: {}", to.join(", "));
    println!("Subject: {subject}");
    if let Some(reply_to) = &options.reply_to {
        println!("Reply-To: {reply_to}");
    }
    if !options.cc.is_empty() {
        println!("Cc: {}", options.cc.join(", "));
    }
    println!("Priority: {:?}", options.priority);
    println!("Body: {body}");
}

fn main() {
    // Minimal call: defaults for everything optional.
    send_email(
        &["ada@example.com"],
        "Welcome!",
        "Thanks for signing up.",
        EmailOptions::default(),
    );

    println!("---");

    // Full call: multiple recipients + overridden options.
    send_email(
        &["ada@example.com", "grace@example.com"],
        String::from("Deploy finished"),
        "Build #128 is live.",
        EmailOptions {
            cc: vec!["ops@example.com".to_string()],
            priority: Priority::High,
            reply_to: Some("noreply@example.com".to_string()),
        },
    );
}
```

Verified output:

```text
To: ada@example.com
Subject: Welcome!
Priority: Normal
Body: Thanks for signing up.
---
To: ada@example.com, grace@example.com
Subject: Deploy finished
Reply-To: noreply@example.com
Cc: ops@example.com
Priority: High
Body: Build #128 is live.
```

> **Note:** For long-lived or interdependent configuration (say, an `HttpRequest` with method, headers, timeout, and retries), promote `EmailOptions` to a full builder so each setting gets its own validating method. See Exercise 3 for a worked builder.

### What about *true* variadics?

If you genuinely want `println!`-style "any number of arguments of mixed types," that is the job of **macros**, not functions. `macro_rules!` can match a repetition of expressions:

```rust playground
// A variadic sum macro (one or more arguments).
macro_rules! sum {
    ($($x:expr),+ $(,)?) => {{
        let mut total = 0;
        $( total += $x; )+
        total
    }};
}

fn main() {
    println!("{}", sum!(1, 2, 3));        // 6
    println!("{}", sum!(10, 20, 30, 40)); // 100
}
```

This compiles and prints `6` then `100`. Macros can do a lot, but they are a different tool with different rules; they are covered in [Macros](/14-macros/). For ordinary "many values of one type," a slice is simpler and almost always the right choice.

---

## Further Reading

### Official Documentation

- [The Rust Book — Functions](https://doc.rust-lang.org/book/ch03-03-how-functions-work.html)
- [The Rust Book — `Option<T>`](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html#the-option-enum-and-its-advantages-over-null-values)
- [`Option::unwrap_or` (std docs)](https://doc.rust-lang.org/std/option/enum.Option.html#method.unwrap_or)
- [The `Default` trait (std docs)](https://doc.rust-lang.org/std/default/trait.Default.html)
- [Rust by Example — Slices](https://doc.rust-lang.org/rust-by-example/primitives/array.html)
- [Rust Design Patterns — Builder](https://rust-unofficial.github.io/patterns/patterns/creational/builder.html)

### Related Sections in This Guide

- [Basic Functions](/03-functions/00-basic-functions/) — `fn` definitions, typed parameters, and return types
- [Return Values](/03-functions/02-return-values/) — return types, tail expressions, returning tuples
- [Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/) — passing behavior as a parameter
- [Higher-Order Functions](/03-functions/04-higher-order/) — functions that take closures (`impl Fn`) as parameters
- [Function Pointers](/03-functions/05-function-pointers/) — the `fn` type as a parameter
- [Basics — Types](/02-basics/01-types/) — the parameter types you will be passing
- [Generics & Traits](/09-generics-traits/) — trait bounds and `impl Trait` parameters
- [Ownership](/05-ownership/) — why slice parameters are borrowed
- [Control Flow](/04-control-flow/) — the next section, building on functions

---

## Exercises

### Exercise 1: Optional parameter with a default

**Difficulty:** Easy

**Objective:** Translate a TypeScript default parameter into idiomatic Rust using `Option<T>`.

**Instructions:** Port the following TypeScript function to Rust. The `exponent` should default to `2` when not provided. Make `power(5, None)` return `25` and `power(2, Some(10))` return `1024`.

```typescript
function power(base: number, exponent: number = 2): number {
  return base ** exponent;
}
```

<details>
<summary>Solution</summary>

```rust playground
fn power(base: i64, exponent: Option<u32>) -> i64 {
    let exponent = exponent.unwrap_or(2);
    base.pow(exponent)
}

fn main() {
    assert_eq!(power(5, None), 25);
    assert_eq!(power(2, Some(10)), 1024);
    println!("ok");
}
```

`unwrap_or(2)` supplies the default exactly when the caller passes `None`. Note the integer method `i64::pow`, which takes a `u32` exponent; there is no `**` operator in Rust.

</details>

### Exercise 2: Rest parameters as a slice

**Difficulty:** Medium

**Objective:** Replace TypeScript rest parameters with a slice parameter, and handle the empty case safely.

**Instructions:** Port `average` to Rust. It must accept any number of `f64` values via a slice and return the arithmetic mean. For an empty input, return `None` instead of dividing by zero (so the signature is `fn average(values: &[f64]) -> Option<f64>`). `average(&[2.0, 4.0, 6.0])` should be `Some(4.0)`; `average(&[])` should be `None`.

```typescript
function average(...values: number[]): number | null {
  if (values.length === 0) return null;
  return values.reduce((a, b) => a + b, 0) / values.length;
}
```

<details>
<summary>Solution</summary>

```rust playground
fn average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let sum: f64 = values.iter().sum();
    Some(sum / values.len() as f64)
}

fn main() {
    assert_eq!(average(&[2.0, 4.0, 6.0]), Some(4.0));
    assert_eq!(average(&[]), None);
    println!("ok");
}
```

Returning `Option<f64>` makes the "no values" case impossible to ignore: the caller must handle `None`, which is far safer than TypeScript's `number | null`. `values.len() as f64` performs the explicit `usize`-to-`f64` cast Rust requires.

</details>

### Exercise 3: A builder for many optional settings

**Difficulty:** Hard

**Objective:** Replace an options object with the builder pattern.

**Instructions:** Implement a `QueryBuilder` for a `Query { table, limit, ascending }`. `QueryBuilder::new(table)` should start with `limit = 100` and `ascending = true`. Provide chained methods `.limit(n)` and `.descending()`, and a `.build()` that returns the `Query`. The expression `QueryBuilder::new("users").limit(10).descending().build()` should equal `Query { table: "users".into(), limit: 10, ascending: false }`.

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug, PartialEq)]
struct Query {
    table: String,
    limit: u32,
    ascending: bool,
}

struct QueryBuilder {
    table: String,
    limit: u32,
    ascending: bool,
}

impl QueryBuilder {
    fn new(table: impl Into<String>) -> Self {
        QueryBuilder { table: table.into(), limit: 100, ascending: true }
    }

    fn limit(mut self, n: u32) -> Self {
        self.limit = n;
        self
    }

    fn descending(mut self) -> Self {
        self.ascending = false;
        self
    }

    fn build(self) -> Query {
        Query {
            table: self.table,
            limit: self.limit,
            ascending: self.ascending,
        }
    }
}

fn main() {
    let q = QueryBuilder::new("users").limit(10).descending().build();
    assert_eq!(
        q,
        Query { table: "users".into(), limit: 10, ascending: false }
    );
    println!("{q:?}");
}
```

Each setter takes `mut self` by value and returns `Self`, which is what enables method chaining: ownership flows through the chain and the final `build()` consumes the builder. `impl Into<String>` lets `new` accept both `&str` and `String`. This pattern scales to dozens of optional settings far better than a function with a dozen `Option` parameters.

</details>
