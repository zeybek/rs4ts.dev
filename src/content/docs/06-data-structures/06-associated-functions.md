---
title: "Associated Functions and Constructors"
description: "Rust has no constructor or static keyword; associated functions like Self::new do both jobs, returning Result for fallible construction and enabling builders."
---

In TypeScript, a `class` gives you a built-in `constructor` and lets you mark helpers as `static`. Rust has neither keyword. Instead, both jobs are done by **associated functions**: functions that live inside an `impl` block but, unlike methods, do *not* take `self`. This file is about those functions: the `Self::new` constructors you will write constantly, named alternative constructors, and a first look at the builder pattern they enable.

---

## Quick Overview

An **associated function** is a function defined in an `impl` block that has no `self` parameter. You call it on the *type* with the `::` path operator (`User::new(...)`) rather than on a value with the `.` operator. It is Rust's equivalent of a TypeScript `static` method, and the conventional `new` associated function is as close as Rust gets to a constructor. The key thing for a TypeScript/JavaScript developer to absorb: **Rust gives you no constructor for free.** If you want `User::new(...)`, you write it yourself.

---

## TypeScript/JavaScript Example

Here is a small `User` class in TypeScript. It uses the language's built-in `constructor`, a couple of `static` factory methods that return new instances, and a `static` helper that returns a plain value rather than an instance.

```typescript
// TypeScript - a class with a constructor and static factory methods
class User {
  id: number;
  name: string;
  isActive: boolean;

  // The built-in constructor: called via `new User(...)`
  constructor(id: number, name: string) {
    this.id = id;
    this.name = name;
    this.isActive = true;
  }

  // A static factory: an alternative way to build a User
  static guest(): User {
    const u = new User(0, "guest");
    u.isActive = false;
    return u;
  }

  // A static helper that does NOT return a User
  static isValidId(id: number): boolean {
    return Number.isInteger(id) && id >= 0;
  }

  // An ordinary (instance) method, for contrast — it uses `this`
  label(): string {
    return `#${this.id} ${this.name}`;
  }
}

const alice = new User(1, "Alice");
const visitor = User.guest();

console.log(alice.label());        // #1 Alice
console.log(visitor.isActive);     // false
console.log(User.isValidId(-3));   // false
```

Three things to notice, because each maps to a distinct Rust idea:

- `new User(...)` invokes the `constructor`. The `new` keyword and the `constructor` slot are **built in**: you get them whether you write a constructor or not.
- `User.guest()` and `User.isValidId(...)` are `static`: called on the *class*, not on an instance, and they have no `this`.
- `label()` is an instance method: called on a value and using `this`.

---

## Rust Equivalent

Rust splits these into two categories. Functions with `self` are **methods** (covered in [Methods and `impl` Blocks](/06-data-structures/05-impl-blocks/)); functions *without* `self` are **associated functions**. There is no `new` keyword and no automatic constructor. You write `new` as a plain associated function that returns `Self`.

```rust playground
#[derive(Debug)]
struct User {
    id: u32,
    name: String,
    is_active: bool,
}

impl User {
    // Associated function: no `self`. Called as `User::new(...)`.
    // `Self` is an alias for `User`, the type this `impl` block is for.
    fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            is_active: true,
        }
    }

    // Another associated function: a named alternative constructor,
    // the equivalent of TypeScript's `static guest()`.
    fn guest() -> Self {
        Self {
            id: 0,
            name: String::from("guest"),
            is_active: false,
        }
    }

    // An associated function that does NOT return Self — like a
    // TypeScript `static` helper that returns a plain value.
    fn is_valid_id(id: u32) -> bool {
        id != 0
    }

    // A method (takes &self), for contrast. Called as `value.label()`.
    fn label(&self) -> String {
        format!("#{} {}", self.id, self.name)
    }
}

fn main() {
    // Associated functions are called on the TYPE with `::`, not on a value.
    let alice = User::new(1, "Alice");
    let visitor = User::guest();

    println!("{}", alice.label());            // method call, uses `.`
    println!("{}", visitor.is_active);
    println!("{}", User::is_valid_id(0));     // associated fn, uses `::`
    println!("{alice:?}");
}
```

Running it prints:

```text
#1 Alice
false
false
User { id: 1, name: "Alice", is_active: true }
```

The whole distinction comes down to one rule: **a function with `self` is a method (call it with `.`); a function without `self` is an associated function (call it with `::`).** `new`, `guest`, and `is_valid_id` have no `self`, so they belong to the type, not to any particular value.

> **Note:** `Self` (capital S) is a type alias for the type the `impl` block is for. Writing `-> Self` and `Self { ... }` instead of `-> User` and `User { ... }` is idiomatic: it keeps the constructor working unchanged if you rename the struct, and reads clearly as "return a value of my own type."

---

## Detailed Explanation

### `new` is a convention, not a keyword

In TypeScript, `new User(1, "Alice")` is special syntax: the `new` operator allocates an object and calls the `constructor`. In Rust there is no `new` operator and no special constructor slot. `User::new(...)` is calling a perfectly ordinary associated function that you happened to name `new`. You could name it `create`, `make`, or `from_parts` and it would work identically. `new` is simply the community convention for "the primary, no-frills constructor."

This means the *real* way to build a struct is the literal struct syntax from [Structs](/06-data-structures/00-structs/):

```rust
let alice = User { id: 1, name: String::from("Alice"), is_active: true };
```

A `new` associated function is just a convenience wrapper around that literal. It earns its keep by (a) hiding boilerplate (like defaulting `is_active` to `true`), and (b) being callable from other modules even when the struct's fields are private (see [Best Practices](#best-practices)).

### Associated functions vs. methods: the `self` test

Everything inside an `impl` block is an **associated item**. Among the functions, the dividing line is the first parameter:

| First parameter | Category | Called with | TypeScript analogy |
| --------------- | -------- | ----------- | ------------------ |
| `&self`, `&mut self`, or `self` | **method** | `value.method()` | instance method (uses `this`) |
| *(none)* | **associated function** | `Type::function()` | `static` method (no `this`) |

Because an associated function has no `self`, the word `self` is simply not in scope inside its body. There is no instance to refer to: you are constructing or computing something *for the type*, not operating on an existing value.

### `Self` and delegating between constructors

`Self` lets one constructor build on another. A named constructor can call `Self::new` to avoid repeating field logic:

```rust playground
#[derive(Debug)]
struct Temperature {
    celsius: f64,
}

impl Temperature {
    fn new(celsius: f64) -> Self {
        Self { celsius }
    }

    // Delegates to `new` via `Self::new`, so the field logic lives in one place.
    fn from_fahrenheit(f: f64) -> Self {
        Self::new((f - 32.0) * 5.0 / 9.0)
    }

    // An associated function that returns a plain value, not Self.
    fn absolute_zero_c() -> f64 {
        -273.15
    }
}

fn main() {
    let t = Temperature::from_fahrenheit(212.0);
    println!("{:.1}", t.celsius);                 // 100.0
    println!("{}", Temperature::absolute_zero_c()); // -273.15
}
```

Output:

```text
100.0
-273.15
```

`Self::new(...)` is the type-level counterpart of calling another method on `self`, except there is no `self`, so you reach for the function through the type alias `Self`.

### Method-call syntax is sugar; `::` is the real form

When you call a method, `value.method()` quietly desugars to `Type::method(value)` (the compiler also auto-inserts `&` or `&mut` as needed — see [Methods and `impl` Blocks](/06-data-structures/05-impl-blocks/)). Associated functions don't get that sugar because there is no receiver value to put before the dot. So you always write the fully-qualified `Type::function()` form. That is *why* `User::new(...)` and `alice.label()` look syntactically different even though both are functions in the same `impl` block.

### `Default`: a standard-library constructor convention

Rust's standard library has a trait, `Default`, whose single associated function `default()` returns a "sensible zero value" of the type. You rarely write it by hand: `#[derive(Default)]` generates it for you when every field is itself `Default` (numbers default to `0`, `String` to `""`, `bool` to `false`, `Option<T>` to `None`).

```rust playground
// #[derive(Default)] writes `Settings::default()` for you.
#[derive(Debug, Default)]
struct Settings {
    verbose: bool,
    retries: u32,
    label: String,
}

fn main() {
    let s = Settings::default();
    println!("{s:?}");
}
```

Output:

```text
Settings { verbose: false, retries: 0, label: "" }
```

`Settings::default()` is an associated function (no `self`), just like `new`. The difference is that `default()` comes from a *trait*, so it is part of a shared, well-known interface; many APIs accept "anything that has a sensible default." Reach for `Default` when "all fields zeroed" is a meaningful starting point, and for a hand-written `new` when construction needs arguments or non-trivial defaults. Full trait coverage is in [Section 09](/09-generics-traits/).

> **Tip:** `new` and `default` are not mutually exclusive. A common pattern is to derive `Default`, then have `new()` (with no arguments) just call `Self::default()`, so callers can use whichever they prefer.

---

## Fallible Constructors: returning `Result`

A TypeScript constructor can only `throw` on bad input, and a thrown error is invisible in the type signature. Rust associated functions return a value, so a constructor that can fail simply returns a [`Result`](/08-error-handling/) instead of `Self`. The convention is to keep `new` infallible and use a descriptive name (or `try_new`) for the checked version.

```rust playground
#[derive(Debug)]
struct Percentage {
    value: u8,
}

impl Percentage {
    // A validated constructor returns Result instead of panicking.
    fn try_new(value: u8) -> Result<Self, String> {
        if value > 100 {
            Err(format!("{value} is greater than 100"))
        } else {
            Ok(Self { value })
        }
    }
}

fn main() {
    match Percentage::try_new(42) {
        Ok(p) => println!("ok: {p:?}"),
        Err(e) => println!("err: {e}"),
    }
    match Percentage::try_new(150) {
        Ok(p) => println!("ok: {p:?}"),
        Err(e) => println!("err: {e}"),
    }
}
```

Output:

```text
ok: Percentage { value: 42 }
err: 150 is greater than 100
```

The signature `try_new(value: u8) -> Result<Self, String>` tells the caller, at compile time, that construction can fail and *forces* them to handle the failure. Contrast TypeScript, where `new Percentage(150)` throwing is something you can only discover by reading the constructor body or the docs.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| ------- | ----------------------- | ---- |
| Constructor | Built-in `constructor`, called with `new` | A plain associated function you write, named `new` by convention |
| Is a constructor provided for you? | Yes (implicit no-arg one if omitted) | No; you must write it |
| Static helper | `static foo()` | Associated function with no `self` |
| How you call it | `Type.foo()` (dot) | `Type::foo()` (path `::`) |
| Distinguishing static from instance | The `static` keyword | Presence/absence of a `self` parameter |
| Returning the new value | Implicit (`new` returns the instance) | Explicit `-> Self` and an explicit `Self { ... }` |
| Failing construction | `throw` (invisible in the type) | Return `Result<Self, E>` (visible in the type) |
| "Zero value" factory | Ad hoc | The standard `Default` trait, often derived |

The conceptual headline: **TypeScript bakes construction into the language (`new` + `constructor`); Rust treats construction as just another function.** That sounds like extra work, and it is a little — but it pays off. There is no magic allocation step, fallible construction is expressed in the type system instead of via exceptions, and a type can have *as many* named constructors as it wants (`new`, `guest`, `from_fahrenheit`, ...) instead of being limited to one overloaded `constructor`.

---

## Common Pitfalls

### Pitfall 1: Calling an associated function on a value with `.`

Coming from TypeScript, where `instance.staticMethod()` sometimes "works" by accident, it is tempting to call `new` (or any associated function) on an existing value.

```rust
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn origin() -> Self {
        Self { x: 0, y: 0 }
    }
}

fn main() {
    let p = Point::origin();
    // `origin` has no `self`, so it must be called on the TYPE, not a value:
    let q = p.origin(); // does not compile (error[E0599])
    println!("{} {}", q.x, q.y);
}
```

The real compiler error points you straight at the fix:

```text
error[E0599]: no method named `origin` found for struct `Point` in the current scope
  --> src/main.rs:15:15
   |
 1 | struct Point {
   | ------------ method `origin` not found for this struct
...
15 |     let q = p.origin(); // does not compile (error[E0599])
   |             --^^^^^^--
   |             | |
   |             | this is an associated function, not a method
   |             help: use associated function syntax instead: `Point::origin()`
   |
   = note: found the following associated functions; to be used as methods, functions must have a `self` parameter
```

**Fix:** call it on the type, `Point::origin()`. The note even spells out the rule: to be callable as a method, a function must have a `self` parameter.

### Pitfall 2: Trying to use `self` inside an associated function

If you reach for `self` in a function that has no `self` parameter, there is nothing for it to refer to.

```rust
struct Counter {
    count: u32,
}

impl Counter {
    fn new() -> Self {
        // An associated function has no `self` in scope.
        Self { count: self.count } // does not compile (error[E0424])
    }
}

fn main() {
    let _c = Counter::new();
}
```

The real compiler error:

```text
error[E0424]: expected value, found module `self`
 --> src/main.rs:8:23
  |
6 |     fn new() -> Self {
  |        --- this function doesn't have a `self` parameter
7 |         // An associated function has no `self` in scope.
8 |         Self { count: self.count } // does not compile (error[E0424])
  |                       ^^^^ `self` value is a keyword only available in methods with a `self` parameter
```

**Fix:** an associated function builds a value from scratch (or from its arguments): `Self { count: 0 }`. If you genuinely need access to an existing instance, you wanted a *method*, so add a `self` receiver and call it on a value.

### Pitfall 3: Assuming a `new` exists by default

There is no automatically generated constructor. If you never wrote `new`, `Type::new(...)` does not exist.

```rust
#[derive(Debug)]
struct Point {
    x: i32,
    y: i32,
}

// No `impl` block defines `new`, so `Point::new` does not exist.

fn main() {
    let p = Point::new(1, 2); // does not compile (error[E0599])
    println!("{p:?}");
}
```

The real compiler error:

```text
error[E0599]: no function or associated item named `new` found for struct `Point` in the current scope
  --> src/main.rs:10:20
   |
 2 | struct Point {
   | ------------ function or associated item `new` not found for this struct
...
10 |     let p = Point::new(1, 2); // does not compile (error[E0599])
   |                    ^^^ function or associated item not found in `Point`
```

**Fix:** either construct it with struct literal syntax — `Point { x: 1, y: 2 }` — or write your own `fn new(x: i32, y: i32) -> Self`. Unlike TypeScript, omitting a constructor gives you *no* constructor, not an implicit empty one.

### Pitfall 4: Making `new` fallible by panicking instead of returning `Result`

It is tempting to write `fn new(...) -> Self` and `panic!` on bad input, mirroring a TypeScript constructor that `throw`s. That hides the failure from the caller's type signature and turns a recoverable problem into a crash. If construction can legitimately fail on caller-supplied data, return `Result<Self, E>` from a `try_new`/`parse`/`from_*` function instead (see [Section 08](/08-error-handling/)). Reserve panicking for cases that indicate a programmer bug, not bad input.

---

## Best Practices

- **Name your primary constructor `new`.** It is the universal convention; readers expect `Type::new(...)` to be the no-frills way to build a value. Don't invent `create`/`make` unless `new` would be ambiguous.
- **Return `Self`, not the spelled-out type.** Write `-> Self` and `Self { ... }`. It is idiomatic and survives renames.
- **Use named alternative constructors freely.** `from_fahrenheit`, `with_capacity`, `guest`, `parse` — Rust has no constructor overloading, so multiple well-named associated functions are the idiomatic substitute. Follow the `from_*` convention when building from a single other value.
- **Make fallible construction return `Result`.** Express "this can fail on bad input" in the type, not via panics. Keep `new` infallible; add `try_new` (or a parse-style function) for the checked path.
- **Derive `Default` for "all-zero" construction.** If a sensible empty/zero value exists, `#[derive(Default)]` gives callers `Type::default()` and plugs into the wider ecosystem. Have `new()` delegate to `Self::default()` when they coincide.
- **Reach for the builder pattern when a constructor has many optional parameters.** Rust has no named or default arguments, so a six-argument `new(...)` quickly becomes unreadable. A builder (below) is the idiomatic replacement.
- **Privacy makes constructors load-bearing.** When a struct's fields are private to its module, outside code *cannot* use struct-literal syntax — your `new`/`try_new` becomes the only entry point, letting you enforce invariants. See [Section 12](/12-modules-packages/) for visibility.

---

## Real-World Example

When a type has many optional settings, a single `new(...)` with a long argument list is painful. Rust has no default or named arguments, so callers must pass *everything* in order. The idiomatic answer is the **builder pattern**: an associated function returns a builder, fluent methods configure it, and a final `build()` produces the immutable value.

This is the production shape of the chaining teaser from [Methods and `impl` Blocks](/06-data-structures/05-impl-blocks/). The associated function `HttpRequest::builder(...)` is the entry point; the rest of the API hangs off it.

```rust playground
#[derive(Debug)]
struct HttpRequest {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    timeout_ms: u64,
}

// The builder holds the in-progress configuration.
#[derive(Debug)]
struct HttpRequestBuilder {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    timeout_ms: u64,
}

impl HttpRequest {
    // Associated function: the single entry point into the builder.
    // Returns the BUILDER, not a finished HttpRequest.
    fn builder(url: &str) -> HttpRequestBuilder {
        HttpRequestBuilder {
            url: url.to_string(),
            method: String::from("GET"),
            headers: Vec::new(),
            body: None,
            timeout_ms: 30_000,
        }
    }
}

impl HttpRequestBuilder {
    // Each step takes `mut self` by value, mutates, and returns Self,
    // so calls chain. (Ownership flows through the chain.)
    fn method(mut self, method: &str) -> Self {
        self.method = method.to_string();
        self
    }

    fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    fn body(mut self, body: &str) -> Self {
        self.body = Some(body.to_string());
        self
    }

    fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    // Consumes the builder, producing the finished, immutable request.
    fn build(self) -> HttpRequest {
        HttpRequest {
            url: self.url,
            method: self.method,
            headers: self.headers,
            body: self.body,
            timeout_ms: self.timeout_ms,
        }
    }
}

fn main() {
    let request = HttpRequest::builder("https://api.example.com/users")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer token123")
        .body(r#"{"name":"Alice"}"#)
        .timeout_ms(5_000)
        .build();

    println!("{} {}", request.method, request.url);
    for (key, value) in &request.headers {
        println!("  {key}: {value}");
    }
    if let Some(body) = &request.body {
        println!("  body: {body}");
    }
    println!("  timeout: {}ms", request.timeout_ms);
}
```

Output:

```text
POST https://api.example.com/users
  Content-Type: application/json
  Authorization: Bearer token123
  body: {"name":"Alice"}
  timeout: 5000ms
```

This reads almost exactly like a TypeScript fluent builder that returns `this`, with one Rust twist: each step takes `self` *by value* and hands it back, so ownership flows down the chain and `build()` consumes the builder at the end; the builder cannot be reused afterward. Defaults (`GET`, empty headers, `30_000`ms) live in `builder()`, and callers only override what they care about, in any order. That is exactly the ergonomics named/default arguments give you in TypeScript, reconstructed from associated functions and ownership.

> **Note:** For real builders you usually pair this with `#[derive(Default)]` on the builder and a `build()` that returns `Result` when required fields might be missing. Mature crates often generate the whole thing with the [`derive_builder`](https://docs.rs/derive_builder) or [`bon`](https://docs.rs/bon) crate. The hand-written version here shows what those macros expand to.

---

## Further Reading

### Official Documentation

- [The Rust Book - Associated Functions](https://doc.rust-lang.org/book/ch05-03-method-syntax.html#associated-functions)
- [Rust by Example - Associated functions & Methods](https://doc.rust-lang.org/rust-by-example/fn/methods.html)
- [`std::default::Default`](https://doc.rust-lang.org/std/default/trait.Default.html)
- [Rust API Guidelines - Constructors are static, inherent methods (`new`)](https://rust-lang.github.io/api-guidelines/predictability.html#constructors-are-static-inherent-methods-c-ctor)
- [Rust Design Patterns - Builder](https://rust-unofficial.github.io/patterns/patterns/creational/builder.html)

### Related Sections in This Guide

- [Structs](/06-data-structures/00-structs/) — the struct-literal syntax a constructor wraps
- [Methods and `impl` Blocks](/06-data-structures/05-impl-blocks/) — associated functions' counterpart: functions *with* `self`
- [Field Init Shorthand](/06-data-structures/08-field-init-shorthand/) — `Self { id, name }` shorthand and `..other` update syntax used inside constructors
- [Associated Types & Consts](/06-data-structures/07-associated-types/) — other items that live inside an `impl` block
- [Tuple Structs](/06-data-structures/01-tuple-structs/) — newtypes whose validated `try_new` is a classic associated function
- [Ownership](/05-ownership/) — why each builder step takes and returns `self`
- [Variables and Mutability](/02-basics/00-variables/) — `mut self` in builder steps
- [Error Handling](/08-error-handling/) — fallible constructors returning `Result`
- [Collections](/07-collections/) — `Vec::new` and `Vec::with_capacity` are associated functions you already use

---

## Exercises

### Exercise 1: Multiple constructors for a `Duration`

**Difficulty:** Easy

**Objective:** Write a primary `new` constructor plus named alternative constructors.

**Instructions:** Complete the `impl` block so that `Duration::new(seconds)` stores the seconds directly, while `Duration::from_minutes` and `Duration::from_hours` convert to seconds. All three are associated functions returning `Self`.

```rust
#[derive(Debug)]
struct Duration {
    seconds: u64,
}

impl Duration {
    fn new(seconds: u64) -> Self {
        // TODO
    }

    fn from_minutes(minutes: u64) -> Self {
        // TODO: 1 minute = 60 seconds
    }

    fn from_hours(hours: u64) -> Self {
        // TODO: 1 hour = 3600 seconds
    }
}

fn main() {
    let a = Duration::new(90);
    let b = Duration::from_minutes(2);
    let c = Duration::from_hours(1);
    println!("{}", a.seconds); // 90
    println!("{}", b.seconds); // 120
    println!("{}", c.seconds); // 3600
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug)]
struct Duration {
    seconds: u64,
}

impl Duration {
    fn new(seconds: u64) -> Self {
        Self { seconds }
    }

    fn from_minutes(minutes: u64) -> Self {
        Self { seconds: minutes * 60 }
    }

    fn from_hours(hours: u64) -> Self {
        Self { seconds: hours * 3600 }
    }
}

fn main() {
    let a = Duration::new(90);
    let b = Duration::from_minutes(2);
    let c = Duration::from_hours(1);
    println!("{}", a.seconds); // 90
    println!("{}", b.seconds); // 120
    println!("{}", c.seconds); // 3600
}
```

None of these take `self`; they are associated functions, called on the type with `::`. Where TypeScript would force you into one overloaded `constructor` (or several `static` factories), Rust treats each named constructor as an ordinary function returning `Self`.

</details>

### Exercise 2: A validated constructor that returns `Result`

**Difficulty:** Medium

**Objective:** Write a fallible constructor that rejects bad input via the type system instead of panicking.

**Instructions:** Complete `Username::try_new` so it trims whitespace, rejects an empty name with `"username cannot be empty"`, rejects names longer than 20 characters with `"username too long: N chars"` (where `N` is the length), and otherwise returns `Ok(Username(...))`.

```rust
#[derive(Debug)]
struct Username(String);

impl Username {
    fn try_new(raw: &str) -> Result<Self, String> {
        // TODO: trim, validate, return Ok(...) or Err(...)
    }
}

fn main() {
    for raw in ["  alice ", "", "this_name_is_definitely_way_too_long"] {
        match Username::try_new(raw) {
            Ok(u) => println!("ok: {u:?}"),
            Err(e) => println!("rejected {raw:?}: {e}"),
        }
    }
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug)]
struct Username(String);

impl Username {
    fn try_new(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(String::from("username cannot be empty"));
        }
        if trimmed.len() > 20 {
            return Err(format!("username too long: {} chars", trimmed.len()));
        }
        Ok(Self(trimmed.to_string()))
    }
}

fn main() {
    for raw in ["  alice ", "", "this_name_is_definitely_way_too_long"] {
        match Username::try_new(raw) {
            Ok(u) => println!("ok: {u:?}"),
            Err(e) => println!("rejected {raw:?}: {e}"),
        }
    }
}
```

Output:

```text
ok: Username("alice")
rejected "": username cannot be empty
rejected "this_name_is_definitely_way_too_long": username too long: 36 chars
```

Returning `Result<Self, String>` makes the possibility of failure part of the signature; callers cannot ignore it, unlike a TypeScript constructor that silently `throw`s. `Self(...)` is the tuple-struct construction shorthand for `Username(...)`.

</details>

### Exercise 3: A small builder

**Difficulty:** Medium/Hard

**Objective:** Build the builder pattern from scratch, starting with an associated `builder()` entry point.

**Instructions:** Give `Pizza` an associated function `builder(size)` that returns a `PizzaBuilder` (default: no toppings, no extra cheese). On `PizzaBuilder`, add chainable `topping(name)` (appends) and `extra_cheese()` (sets the flag), each taking `mut self` and returning `Self`, plus a `build()` that consumes the builder and returns a `Pizza`. Make the `main` below compile and run.

```rust
#[derive(Debug)]
struct Pizza {
    size: String,
    toppings: Vec<String>,
    extra_cheese: bool,
}

#[derive(Debug)]
struct PizzaBuilder {
    // TODO: same fields as Pizza
}

// impl Pizza { fn builder(size: &str) -> PizzaBuilder { ... } }
// impl PizzaBuilder { topping / extra_cheese / build }

fn main() {
    let pizza = Pizza::builder("large")
        .topping("mushroom")
        .topping("olive")
        .extra_cheese()
        .build();

    println!("{} pizza, extra cheese: {}", pizza.size, pizza.extra_cheese);
    println!("toppings: {:?}", pizza.toppings);
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug)]
struct Pizza {
    size: String,
    toppings: Vec<String>,
    extra_cheese: bool,
}

#[derive(Debug)]
struct PizzaBuilder {
    size: String,
    toppings: Vec<String>,
    extra_cheese: bool,
}

impl Pizza {
    fn builder(size: &str) -> PizzaBuilder {
        PizzaBuilder {
            size: size.to_string(),
            toppings: Vec::new(),
            extra_cheese: false,
        }
    }
}

impl PizzaBuilder {
    fn topping(mut self, name: &str) -> Self {
        self.toppings.push(name.to_string());
        self
    }

    fn extra_cheese(mut self) -> Self {
        self.extra_cheese = true;
        self
    }

    fn build(self) -> Pizza {
        Pizza {
            size: self.size,
            toppings: self.toppings,
            extra_cheese: self.extra_cheese,
        }
    }
}

fn main() {
    let pizza = Pizza::builder("large")
        .topping("mushroom")
        .topping("olive")
        .extra_cheese()
        .build();

    println!("{} pizza, extra cheese: {}", pizza.size, pizza.extra_cheese);
    println!("toppings: {:?}", pizza.toppings);
}
```

Output:

```text
large pizza, extra cheese: true
toppings: ["mushroom", "olive"]
```

`Pizza::builder(...)` is the associated-function entry point and returns the *builder*, not a `Pizza`. Each configuration step takes `mut self` and returns it so calls chain; `build()` takes `self` by value, consuming the builder to hand back the finished `Pizza`. This is the idiomatic stand-in for TypeScript's optional/named arguments.

</details>
