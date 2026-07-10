---
title: "Attribute Macros"
description: "Rust's #[attribute] macros echo TypeScript decorators, but rewrite a function or struct's source at compile time, with zero runtime cost, using syn and quote."
---

If you have ever written `@Component`, `@Injectable`, or a custom method decorator in TypeScript, you already know the *shape* of an **attribute macro**: a name you attach above a declaration that transforms or augments it. Rust's `#[attribute]` syntax looks almost identical, but it runs at compile time and rewrites your code into entirely new source before the compiler ever type-checks it.

---

## Quick Overview

An **attribute macro** is a kind of **procedural macro** that you place *in front of* an item (usually a function, but also a struct, module, or `impl` block) to inspect and rewrite that item. The annotated item is handed to your macro as a stream of tokens, and whatever tokens you return *replace* it. Frameworks use this for things like `#[tokio::main]`, web routes (`#[get("/users")]`), and test harnesses (`#[test]`). This page covers the *concept* and a minimal, hand-written example; the mechanics of the `proc-macro` crate, `syn`, and `quote` are covered in depth in [Procedural Macros](/14-macros/07-proc-macros/).

---

## TypeScript/JavaScript Example

The closest TypeScript analogue is a **decorator**. Here is a standard (TC39 stage-3) method decorator that wraps a method so it logs when it is entered and how long it took, a pattern you have probably seen in logging or tracing libraries. It works in TypeScript 5.x and runs on Node v22 after compilation.

```typescript
// deco.ts
// Standard (TC39 stage-3) decorators, supported in TypeScript 5.x and Node v22.
function logCalls<This, Args extends unknown[], Return>(
  target: (this: This, ...args: Args) => Return,
  context: ClassMethodDecoratorContext,
) {
  const name = String(context.name);
  // The decorator REPLACES the method with this wrapper at runtime.
  return function (this: This, ...args: Args): Return {
    console.log(`-> entering ${name}`);
    const start = performance.now();
    const result = target.call(this, ...args);
    console.log(`<- leaving ${name} after ${(performance.now() - start).toFixed(3)}ms`);
    return result;
  };
}

class OrderService {
  @logCalls
  orderTotal(itemCents: number, qty: number): number {
    return itemCents * qty + 500; // items plus a flat $5.00 shipping fee
  }
}

const svc = new OrderService();
console.log("total =", svc.orderTotal(1299, 3));
```

Compiling with `tsc --target ES2022 deco.ts` and running `node deco.js` prints:

```text
-> entering orderTotal
<- leaving orderTotal after 0.007ms
total = 4397
```

The decorator runs **at runtime**: every time `orderTotal` is invoked, the wrapper function executes. The original method is still a real value being passed around and called through `target.call(...)`. There is per-call indirection, and the wrapping happens while your program is running.

> **Note:** A TypeScript decorator is a *runtime* function that receives the thing it decorates as a value. A Rust attribute macro is a *compile-time* function that receives the thing it annotates as **source code (tokens)** and emits new source code. This difference drives almost everything else on this page.

---

## Rust Equivalent

In Rust, the same "log on entry and exit" behavior is an attribute macro named `log_calls`. Attribute macros must live in their own crate of type `proc-macro`, so this is a two-crate setup: a macro crate that defines the attribute, and an application crate that uses it. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically.

The macro crate (`log_attr/Cargo.toml`) must opt in to being a proc-macro crate and depends on `syn` and `quote`:

```toml
# log_attr/Cargo.toml
[package]
name = "log_attr"
version = "0.1.0"
edition = "2024"

[lib]
proc-macro = true   # REQUIRED — this is what makes #[proc_macro_attribute] legal

[dependencies]
syn = { version = "2", features = ["full"] }
quote = "1"
proc-macro2 = "1"
```

The macro itself (`log_attr/src/lib.rs`) parses the annotated function, then re-emits it with a wrapped body:

```rust
// log_attr/src/lib.rs
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// An attribute macro that wraps a function so it logs when it is entered
/// and when it returns, including how long the body took.
#[proc_macro_attribute]
pub fn log_calls(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the annotated item as a complete function definition.
    let input = parse_macro_input!(item as ItemFn);

    // Pull apart the pieces of the original function.
    let vis = &input.vis;     // pub / pub(crate) / (nothing)
    let sig = &input.sig;     // fn name(args) -> Ret
    let block = &input.block; // the original { ... } body
    let name = &sig.ident;    // just the function name, e.g. `order_total`

    // Re-emit the function with the same signature, but a new body that
    // wraps the original block with logging on either side.
    let expanded = quote! {
        #vis #sig {
            println!("-> entering `{}`", stringify!(#name));
            let __started = std::time::Instant::now();
            let __result = (|| #block)();
            println!("<- leaving  `{}` after {:?}", stringify!(#name), __started.elapsed());
            __result
        }
    };

    expanded.into()
}
```

The application crate (`app/Cargo.toml`) depends on the macro crate by path:

```toml
# app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
log_attr = { path = "../log_attr" }
```

And uses the attribute exactly like a decorator; you place it above the function:

```rust
// app/src/main.rs
use log_attr::log_calls;

/// Compute the total price in cents, including a flat $5.00 shipping fee.
#[log_calls]
fn order_total(item_cents: u32, qty: u32) -> u32 {
    let shipping = 500;
    item_cents * qty + shipping
}

#[log_calls]
fn greet(name: &str) {
    println!("Hello, {name}!");
}

fn main() {
    let total = order_total(1299, 3);
    println!("total = {total} cents");
    greet("Ada");
}
```

Running `cargo run -p app` prints:

```text
-> entering `order_total`
<- leaving  `order_total` after 42ns
total = 4397 cents
-> entering `greet`
Hello, Ada!
<- leaving  `greet` after 500ns
```

> **Note:** Notice `log_calls` works on *both* `order_total` (returns a `u32`) and `greet` (returns `()`), with no generics or trait bounds. That is because the macro never sees types; it manipulates tokens. The wrapped body just forwards whatever the original returned.

---

## Detailed Explanation

### The attribute macro signature

Every attribute macro has the exact same signature:

```rust
// Signature shape — both parameters are token streams, the return is tokens.
#[proc_macro_attribute]
pub fn log_calls(_attr: TokenStream, item: TokenStream) -> TokenStream { /* ... */ }
```

- `attr` is the tokens *inside the parentheses of the attribute itself*. For `#[log_calls]` it is empty; for `#[get("/users")]` it would be the tokens `"/users"`.
- `item` is the tokens of *the thing the attribute is attached to*: here, the entire `fn order_total(...) { ... }`.
- The returned `TokenStream` **completely replaces** the original item. If you return `item` unchanged, the function is untouched. If you return nothing, the function disappears.

This is fundamentally different from a TypeScript decorator, which receives the method as a callable *value* and returns a replacement *value* at runtime.

### Parsing with `syn`

`parse_macro_input!(item as ItemFn)` turns the raw token stream into a structured syntax tree. `ItemFn` is `syn`'s representation of a free function, exposing fields like `.vis` (visibility), `.sig` (the signature: name, generics, parameters, return type), and `.block` (the body). Working with this tree is far safer than string manipulation: you get the real Rust grammar, not a regex.

### Generating with `quote`

The `quote! { ... }` macro is the inverse of parsing: it builds a new token stream from a template. Inside it, `#vis`, `#sig`, `#block`, and `#name` **interpolate** the pieces we extracted (the `#` is `quote`'s splice operator, unrelated to a comment). So we reconstruct the original function header, then write a brand-new body.

A few details in the generated body matter:

- `stringify!(#name)` turns the identifier into a string literal *at compile time*, so the log shows the function's actual name.
- `let __result = (|| #block)();` wraps the original body in an immediately-invoked closure. This lets the body `return` early without skipping the "leaving" log, and gives us a single value to log around. (For functions that themselves return early, this preserves correct control flow.)
- The underscore-prefixed names (`__started`, `__result`) are an intentional, low-collision choice. Rust's macro hygiene handles most identifier clashes, but `#[proc_macro_attribute]` macros are *less* hygienic than `macro_rules!` for identifiers that appear in the user's body, so distinctive names are a sensible habit. Hygiene is covered in [Macro Basics](/14-macros/00-macro-basics/).

### Seeing the expansion

You never have to imagine what an attribute macro produces. The `cargo expand` tool (install with `cargo install cargo-expand`) prints your code *after* macro expansion. For the `order_total` example above, `cargo expand --bin app` shows the rewritten function:

```rust
// Output of `cargo expand` (lightly trimmed): the macro replaced the function body.
fn order_total(item_cents: u32, qty: u32) -> u32 {
    {
        ::std::io::_print(format_args!("-> entering `{0}`\n", "order_total"));
    };
    let __started = std::time::Instant::now();
    let __result = (|| { item_cents * qty + 500 })();
    {
        ::std::io::_print(
            format_args!(
                "<- leaving  `{0}` after {1:?}\n",
                "order_total",
                __started.elapsed(),
            ),
        );
    };
    __result
}
```

This is the key insight: by the time the program runs, there *is no macro*, only ordinary, fully-monomorphized Rust. There is no decorator object, no per-call lookup, no wrapper allocation. The cost is paid once, during compilation.

---

## Key Differences

| Aspect | TypeScript decorator | Rust attribute macro |
| --- | --- | --- |
| When it runs | At **runtime** (and once at class-definition time) | At **compile time**, before type-checking the result |
| What it receives | The decorated value (a function/class) and a context object | The annotated item as **tokens** (source code) |
| What it returns | A replacement value, or `void` | A `TokenStream` that **replaces** the item's source |
| Runtime cost | Wrapper indirection on every call | Zero — the generated code *is* the code |
| Where it can live | Any module | A dedicated `proc-macro = true` crate |
| Can read types? | Yes, via reflection/metadata at runtime | No — it sees syntax only, never resolved types |
| Failure mode | Throws at runtime | Emits a **compiler error**; program never builds |

The mental-model shift for a TypeScript developer: a decorator is *a function that wraps a value*; an attribute macro is *a program that writes a program*. Because the macro emits plain source code that the compiler then checks normally, a mistake in your generated code surfaces as an ordinary type error pointing into the expanded code, not a runtime exception.

> **Tip:** The three flavors of procedural macro are attribute macros (`#[name]`, this page), [derive macros](/14-macros/04-derive-macros/) (`#[derive(Name)]`), and [function-like macros](/14-macros/06-function-like-macros/) (`name!(...)`). They share the same `proc-macro` crate plumbing but differ in how they are triggered.

---

## Common Pitfalls

### Pitfall 1: Forgetting `proc-macro = true`

Attribute macros are only legal in a crate that declares itself a proc-macro crate. Leave out the `[lib] proc-macro = true` line and the compiler rejects the attribute:

```rust
// log_attr/src/lib.rs — in a crate WITHOUT `proc-macro = true` in Cargo.toml
use proc_macro::TokenStream;

#[proc_macro_attribute] // does not compile
pub fn noop(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
```

The real error from `cargo build`:

```text
error: the `#[proc_macro_attribute]` attribute is only usable with crates of the `proc-macro` crate type
 --> src/lib.rs:3:1
  |
3 | #[proc_macro_attribute]
  | ^^^^^^^^^^^^^^^^^^^^^^^
```

The fix is the `[lib] proc-macro = true` stanza shown in the Rust Equivalent section.

### Pitfall 2: Applying the attribute to the wrong kind of item

Our macro parses its input `as ItemFn`. If you put `#[log_calls]` on a struct instead of a function, `parse_macro_input!` cannot match the `fn` grammar and produces a precise error:

```rust
// app/src/main.rs
use log_attr::log_calls;

#[log_calls] // does not compile — log_calls expects a function, not a struct
struct Order {
    total: u32,
}
```

The real error from `cargo build`:

```text
error: expected `fn`
 --> app2/src/main.rs:4:1
  |
4 | struct Order {
  | ^^^^^^
```

> **Warning:** Coming from TypeScript, it is tempting to assume an attribute "just adds metadata" and is harmless anywhere. It is not: an attribute macro *replaces* the item, so it must understand what it was given. A well-written macro either accepts multiple item kinds or fails with a clear message.

### Pitfall 3: Expecting the macro to inspect types

A decorator in TypeScript can read parameter types via `reflect-metadata` at runtime. An attribute macro cannot. Inside `log_calls`, `input.sig` knows the parameter is *written* as `u32`, but it has no idea whether `u32` is an alias, what trait it implements, or whether it equals some other type. That information does not exist yet at macro-expansion time. If your design needs resolved type information, an attribute macro is the wrong tool; consider generics and traits (see [Generics & Traits](/09-generics-traits/)) instead.

### Pitfall 4: Generating code that does not compile

Because the macro emits source, a bug in your template becomes a compiler error *in the expanded code*. If you forget to handle the function's return value and drop `__result`, the wrapped function silently changes its return type to `()` and every caller fails to type-check. Run `cargo expand` whenever the error message is confusing; it shows you the exact code the compiler is complaining about.

---

## Best Practices

- **Reach for an attribute macro only when simpler tools fall short.** A higher-order function, a generic wrapper, or a `macro_rules!` ([Declarative Macros](/14-macros/01-declarative-macros/)) is far less machinery. Attribute macros shine when you must *rewrite a declaration's shape* — adding fields, generating sibling functions, registering routes — which ordinary code cannot do.
- **Parse into the most specific `syn` type you support.** `ItemFn` for functions, `ItemStruct` for structs, or `syn::Item` if you genuinely accept several kinds and want to branch. Specific types give better error messages.
- **Preserve everything you do not intend to change.** Re-emit the original visibility, generics, where-clauses, and attributes. The simplest faithful pattern is to interpolate the *whole* parsed item (`#input`) and only *add* around it, rather than reconstructing it field by field.
- **Emit real `compile_error!` diagnostics for misuse.** Returning `syn::Error::new_spanned(item, "message").to_compile_error()` points the error at the user's code with a message you control, much friendlier than a parse failure. This is detailed in [Procedural Macros](/14-macros/07-proc-macros/).
- **Keep the generated code minimal and predictable.** Smaller expansions compile faster and are easier to debug with `cargo expand`.
- **Pin `syn` with the `full` feature** when you parse function bodies or full items; the default feature set does not include the full grammar.

---

## Real-World Example

Web frameworks like Actix and Rocket let you annotate a handler with its HTTP route. Here is a minimal version of that idea: a `#[get("/path")]` attribute that keeps your handler intact *and* generates a sibling function describing where it is mounted. This demonstrates an attribute macro that **reads its own arguments** (the path literal): the `attr` parameter we ignored earlier.

The macro (added to `log_attr/src/lib.rs`):

```rust
// log_attr/src/lib.rs
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, LitStr};

/// A route-style attribute that takes a path literal, e.g. `#[get("/users")]`.
/// It keeps the original handler unchanged and generates a sibling function
/// `<name>_route()` returning the (method, path) the handler is mounted at.
#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    // The attribute's OWN tokens: parse them as a string literal.
    let path = parse_macro_input!(attr as LitStr);
    let input = parse_macro_input!(item as ItemFn);

    let name = &input.sig.ident;
    // Build a new identifier `<name>_route` for the generated helper.
    let route_fn = quote::format_ident!("{}_route", name);

    let expanded = quote! {
        // Emit the original handler unchanged...
        #input

        // ...plus a generated helper describing its mount point.
        pub fn #route_fn() -> (&'static str, &'static str) {
            ("GET", #path)
        }
    };

    expanded.into()
}
```

The handler in the application crate:

```rust
// app/src/main.rs
use log_attr::get;

#[get("/users")]
fn list_users() -> String {
    "[\"ada\", \"linus\"]".to_string()
}

fn main() {
    // `list_users` still exists and behaves normally...
    // ...and `list_users_route` was generated for us by the macro.
    let (method, path) = list_users_route();
    println!("mounted {method} {path} -> {}", list_users());
}
```

Running `cargo run -p app` prints:

```text
mounted GET /users -> ["ada", "linus"]
```

This is exactly the trick real frameworks use: the attribute does not change your handler, but it generates extra registration code beside it that the framework later collects. The `attr` parameter — empty for `#[log_calls]` — carries the `"/users"` literal here, parsed as a `syn::LitStr`. For the full machinery behind `proc-macro2`, custom parsing, and richer argument syntax, see [Procedural Macros](/14-macros/07-proc-macros/).

---

## Further Reading

- [The Rust Reference: Attribute macros](https://doc.rust-lang.org/reference/procedural-macros.html#attribute-macros), the authoritative specification.
- [The Rust Programming Language, Ch. 20: Macros](https://doc.rust-lang.org/book/ch20-05-macros.html), the book's overview of declarative and procedural macros.
- [`syn` crate documentation](https://docs.rs/syn/latest/syn/) and [`quote` crate documentation](https://docs.rs/quote/latest/quote/), parsing and generating tokens.
- [The Little Book of Rust Macros](https://veykril.github.io/tlborm/), a deep, community-maintained guide.
- Related sections in this guide:
  - [Macro Basics](/14-macros/00-macro-basics/) — what macros are and are *not*, hygiene, and when to reach for one.
  - [Declarative Macros](/14-macros/01-declarative-macros/) — `macro_rules!`, the simpler alternative.
  - [Derive Macros](/14-macros/04-derive-macros/) — `#[derive(...)]`, the sibling flavor for generating trait impls.
  - [Function-like Macros](/14-macros/06-function-like-macros/) — `name!(...)` procedural macros.
  - [Procedural Macros](/14-macros/07-proc-macros/) — the full `proc-macro` crate, `syn` 2.0, and `quote` mechanics.
  - [Generics & Traits](/09-generics-traits/) — often the better tool when you think you need a macro.
  - [Getting Started](/01-getting-started/) and [Basics](/02-basics/) — `cargo`, crates, and project layout.
  - [Serialization](/15-serialization/) — where `#[derive(Serialize)]` and field attributes appear constantly in practice.

---

## Exercises

### Exercise 1: A `#[timed]` attribute

**Difficulty:** Easy

**Objective:** Write an attribute macro that prints only how long a function took, leaving its return value and behavior unchanged.

**Instructions:**

1. In your `proc-macro` crate, add a `#[proc_macro_attribute] pub fn timed(...)`.
2. Parse the item as an `ItemFn`, wrap the original body so you measure elapsed time around it, and print the elapsed `Duration` to standard error using the function's name.
3. Apply `#[timed]` to a function such as `fn sum_to(n: u64) -> u64 { (1..=n).sum() }` and confirm the return value is unchanged.

<details>
<summary>Solution</summary>

```rust
// log_attr/src/lib.rs
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// `#[timed]` prints how long a function took, to standard error.
#[proc_macro_attribute]
pub fn timed(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let name = &sig.ident;

    quote! {
        #vis #sig {
            let __start = std::time::Instant::now();
            let __out = (|| #block)();
            eprintln!("[timed] {} took {:?}", stringify!(#name), __start.elapsed());
            __out
        }
    }
    .into()
}
```

```rust
// app/src/main.rs
use log_attr::timed;

#[timed]
fn sum_to(n: u64) -> u64 {
    (1..=n).sum()
}

fn main() {
    println!("sum = {}", sum_to(1_000));
}
```

Running `cargo run -p app` prints (the exact duration varies):

```text
[timed] sum_to took 6.917µs
sum = 500500
```

</details>

### Exercise 2: An attribute that reads an argument

**Difficulty:** Medium

**Objective:** Write `#[warn_deprecated("message")]` that prints a warning before the function body runs. This exercises the `attr` parameter.

**Instructions:**

1. Add `#[proc_macro_attribute] pub fn warn_deprecated(...)`.
2. Parse `attr` as a `syn::LitStr` to capture the message, and parse `item` as an `ItemFn`.
3. Re-emit the function with a body that first prints `WARNING: <name> is deprecated: <message>` to standard error, then runs the original block.

<details>
<summary>Solution</summary>

```rust
// log_attr/src/lib.rs
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, LitStr};

/// `#[warn_deprecated("use X instead")]` warns before running the body.
#[proc_macro_attribute]
pub fn warn_deprecated(attr: TokenStream, item: TokenStream) -> TokenStream {
    let message = parse_macro_input!(attr as LitStr);
    let input = parse_macro_input!(item as ItemFn);
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let name = &sig.ident;

    quote! {
        #vis #sig {
            eprintln!("WARNING: `{}` is deprecated: {}", stringify!(#name), #message);
            #block
        }
    }
    .into()
}
```

```rust
// app/src/main.rs
use log_attr::warn_deprecated;

#[warn_deprecated("call `checkout_v2` instead")]
fn checkout() -> &'static str {
    "ok"
}

fn main() {
    println!("checkout = {}", checkout());
}
```

Running `cargo run -p app` prints:

```text
WARNING: `checkout` is deprecated: call `checkout_v2` instead
checkout = ok
```

</details>

### Exercise 3: A route attribute that generates a sibling function

**Difficulty:** Hard

**Objective:** Recreate the `#[get("/path")]` attribute from the Real-World Example: keep the handler intact and generate a `<name>_route()` helper returning `("GET", path)`.

**Instructions:**

1. Parse `attr` as a `LitStr` (the path) and `item` as an `ItemFn` (the handler).
2. Build a new identifier `<name>_route` with `quote::format_ident!`.
3. Emit the original function unchanged (interpolate the whole `#input`) plus the generated helper.
4. Call both the handler and its generated `_route` function from `main`.

<details>
<summary>Solution</summary>

```rust
// log_attr/src/lib.rs
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, LitStr};

/// `#[get("/path")]` keeps the handler and generates `<name>_route()`.
#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    let path = parse_macro_input!(attr as LitStr);
    let input = parse_macro_input!(item as ItemFn);

    let name = &input.sig.ident;
    let route_fn = quote::format_ident!("{}_route", name);

    quote! {
        #input

        pub fn #route_fn() -> (&'static str, &'static str) {
            ("GET", #path)
        }
    }
    .into()
}
```

```rust
// app/src/main.rs
use log_attr::get;

#[get("/users")]
fn list_users() -> String {
    "[\"ada\", \"linus\"]".to_string()
}

fn main() {
    let (method, path) = list_users_route();
    println!("mounted {method} {path} -> {}", list_users());
}
```

Running `cargo run -p app` prints:

```text
mounted GET /users -> ["ada", "linus"]
```

</details>
