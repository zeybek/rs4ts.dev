---
title: "Function-Like Procedural Macros"
description: "Unlike a TypeScript runtime function, a Rust foo!(...) procedural macro runs real code at compile time to parse a custom DSL and turn bad input into compiler errors."
---

## Quick Overview

A **function-like procedural macro** is invoked exactly like the macros you already know — `foo!(...)` — but instead of pattern-matching tokens the way `macro_rules!` does, it is an actual Rust function that receives a `TokenStream`, runs *real Rust code* at compile time, and returns the `TokenStream` it generated. That extra power lets a function-like macro do things a declarative macro cannot: parse a custom mini-language, talk to the file system, and **validate its arguments at compile time** so a bad input becomes a compiler error rather than a runtime exception. This page is about *when* to reach for a function-like proc macro versus a `macro_rules!` one. The mechanics of writing them live in [Procedural Macros](/14-macros/07-proc-macros/).

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects the newest edition automatically. The procedural-macro examples here were compiled and run with `syn` 2.0, `quote` 1.0, and `proc-macro2` 1.0.

---

## TypeScript/JavaScript Example

There is no compile-time code generation in TypeScript or JavaScript, so the closest analog to "call a thing with `name(...)` and have it process arbitrary input and validate it" is an ordinary **runtime function**. The catch is in the word *runtime*: any validation it performs only happens when the program actually executes that line.

```typescript
// A runtime "route registry" — the nearest JavaScript/TypeScript analog to a function-like macro.
type Handler = () => string;

function makeRoute(method: string, path: string, handler: Handler) {
  // Validation can ONLY happen at runtime — when this code actually executes.
  if (!path.startsWith("/")) {
    throw new Error(`route path must start with '/': got ${path}`);
  }
  return { key: `${method.toUpperCase()} ${path}`, handler };
}

const ok = makeRoute("get", "/users", () => "users");
console.log(ok.key); // GET /users

try {
  makeRoute("get", "users", () => "oops"); // bad path — no leading slash
} catch (e) {
  console.log("caught at runtime:", (e as Error).message);
}
```

Running it (Node v22, via `tsx`) prints:

```text
GET /users
caught at runtime: route path must start with '/': got users
```

The bad path is *accepted by the compiler* and only blows up when that branch runs. If `makeRoute("get", "users")` sits behind a rarely-hit code path, it can ship to production undetected. Hold onto that: a function-like proc macro turns exactly this kind of check into a **compile error**.

---

## Rust Equivalent

A function-like procedural macro lives in its own crate (a crate with `proc-macro = true`). Here is the macro definition, then how a consumer calls it. The macro parses two string literals and rejects a path that does not start with `/`, **at compile time**.

```rust
// ===== crate `mymacros` (lib.rs) — must be a proc-macro crate =====
use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, LitStr, Token};

// We teach `syn` how to parse our input: two string literals separated by a comma.
struct Route {
    method: LitStr,
    _comma: Token![,],
    path: LitStr,
}

impl Parse for Route {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Route {
            method: input.parse()?,
            _comma: input.parse()?,
            path: input.parse()?,
        })
    }
}

#[proc_macro]
pub fn route(input: TokenStream) -> TokenStream {
    let Route { method, path, .. } = parse_macro_input!(input as Route);
    let path_value = path.value();

    // Real Rust code, running in the compiler: validate the literal.
    if !path_value.starts_with('/') {
        return syn::Error::new(path.span(), "route path must start with '/'")
            .to_compile_error()
            .into();
    }

    let combined = format!("{} {}", method.value().to_uppercase(), path_value);
    quote! { #combined }.into() // emit a &'static str literal
}
```

```rust
// ===== crate `app` (main.rs) — depends on `mymacros` =====
use mymacros::route;

fn main() {
    // Each call is parsed, validated, and replaced by a string literal — at compile time.
    let users: &str = route!("get", "/users");
    let health: &str = route!("POST", "/health");
    println!("{users}");
    println!("{health}");
}
```

Real output:

```text
GET /users
POST /health
```

And the payoff: give it a bad path and the program **does not compile**:

```rust
// does not compile — path has no leading '/'
use mymacros::route;

fn main() {
    let bad: &str = route!("get", "users");
    println!("{bad}");
}
```

Real compiler error:

```text
error: route path must start with '/'
 --> src/main.rs:4:35
  |
4 |     let bad: &str = route!("get", "users");
  |                                   ^^^^^^^
```

The error points straight at the offending literal, with our own message. The runtime exception from the TypeScript version is now impossible: the broken route never makes it into a binary.

> **Note:** Function-like proc macros must be defined in a dedicated `proc-macro` crate; the consumer crate adds it as a dependency. The boilerplate of setting that up, and the full `syn`/`quote` story, is covered in [Procedural Macros](/14-macros/07-proc-macros/). To follow along, `cargo add syn --features full`, `cargo add quote`, and `cargo add proc-macro2` in the macro crate.

---

## Detailed Explanation

### The shape of a function-like proc macro

Every function-like procedural macro has the same signature:

```rust
#[proc_macro]
pub fn my_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // ...
}
```

- The `#[proc_macro]` attribute marks it as a function-like macro (as opposed to `#[proc_macro_derive(...)]` for derives or `#[proc_macro_attribute]` for attribute macros; see [Derive Macros](/14-macros/04-derive-macros/) and [Attribute Macros](/14-macros/05-attribute-macros/)).
- It takes one `TokenStream` (everything inside the `( )`, `[ ]`, or `{ }` of the call) and returns one `TokenStream` (the code that replaces the call).
- It is a genuine Rust function that runs inside `rustc` during compilation. It can use loops, `if`, the standard library, helper crates, anything.

The caller writes `route!("get", "/users")` and the compiler hands the tokens `"get" , "/users"` to your function. Whatever tokens you return are spliced back in where the call appeared.

### `syn` parses, `quote` generates

The two workhorse crates:

- **`syn`** turns the raw `TokenStream` into structured data. `parse_macro_input!(input as Route)` runs the `Parse` implementation we wrote and either produces a `Route` or emits a parse error for us. `syn` already knows how to parse `LitStr` (a string literal), `Ident`, `LitInt`, punctuation tokens like `Token![,]`, and full Rust syntax.
- **`quote`** is the inverse: the `quote! { ... }` macro is a templating language for *building* a `TokenStream`. Inside it, `#combined` interpolates the value of the variable `combined`.

This split (parse with `syn`, generate with `quote`) is the standard structure of essentially every procedural macro.

### Why this is "more power" than `macro_rules!`

A `macro_rules!` macro matches token patterns and substitutes; it can *see* tokens but it cannot *compute* with their contents. It cannot ask "does this string literal start with a slash?" because it has no way to inspect the characters inside a literal. A function-like proc macro can, because by the time your function runs you have the literal's value as a normal Rust `String`:

```rust
let path_value = path.value(); // a real String we can call .starts_with('/') on
```

That single capability, running arbitrary logic over the parsed input, is the line between the two macro families.

### Function-like proc macros can validate, count, and even read files

Because the body is ordinary Rust, a function-like macro can:

- **Validate** input and emit a custom compile error (the `route!` example).
- **Compute** values at compile time (parse and re-emit, fold constants, build lookup tables).
- **Read the file system at build time.** This is exactly how the standard `include_str!` and `include_bytes!` work, and how SQL crates like `sqlx`'s `query!` validate your SQL against a real database schema during compilation.

None of these are possible with a declarative macro alone.

### A small DSL: `config!`

Function-like macros shine at parsing a *custom* syntax that is not valid expression-grammar Rust. Here a `config!` macro reads `key = value` pairs and emits a struct literal:

```rust
// ===== in the proc-macro crate =====
use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Ident, LitInt, LitStr, Token};

struct Setting {
    key: Ident,
    value: SettingValue,
}

enum SettingValue {
    Str(LitStr),
    Int(LitInt),
}

impl Parse for Setting {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let value = if input.peek(LitStr) {
            SettingValue::Str(input.parse()?)
        } else {
            SettingValue::Int(input.parse()?)
        };
        Ok(Setting { key, value })
    }
}

struct ConfigInput {
    settings: Punctuated<Setting, Token![,]>,
}

impl Parse for ConfigInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(ConfigInput {
            settings: Punctuated::parse_terminated(input)?,
        })
    }
}

#[proc_macro]
pub fn config(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as ConfigInput);
    let fields = parsed.settings.iter().map(|s| {
        let key = &s.key;
        match &s.value {
            SettingValue::Str(lit) => quote! { #key: #lit.to_string() },
            SettingValue::Int(lit) => quote! { #key: #lit },
        }
    });
    quote! {
        Config { #( #fields ),* }
    }
    .into()
}
```

```rust
// ===== in the consumer crate =====
use mymacros::config;

#[derive(Debug)]
struct Config {
    host: String,
    port: u16,
}

fn main() {
    let cfg = config! {
        host = "localhost",
        port = 8080,
    };
    println!("listening on {}:{}", cfg.host, cfg.port);
}
```

Real output:

```text
listening on localhost:8080
```

The `config! { ... }` block is *not* a Rust expression you could write by hand: `host = "localhost"` is not valid in an expression position. The macro defines its own grammar, parses it with `syn`, and emits a normal `Config { .. }` literal. This is the kind of mini-DSL that function-like proc macros power, for instance `sqlx`'s `query!` (which type-checks SQL against a real schema at compile time) or the `html!`/`view!` macros in web frameworks like Yew and Leptos. (Note that not every DSL needs a proc macro: `serde_json::json!`, despite its rich syntax, is a *declarative* `macro_rules!` macro, a reminder that the declarative family is more capable than it first appears.)

---

## Key Differences

### Function-like proc macro vs. declarative `macro_rules!`

| Aspect | `macro_rules!` (declarative) | `#[proc_macro]` (function-like procedural) |
| --- | --- | --- |
| How it works | Pattern-match token trees, substitute | A Rust function: `TokenStream` in, `TokenStream` out |
| Where it lives | Anywhere (inline in any module) | A separate `proc-macro = true` crate |
| Can run arbitrary logic | No — match and emit only | Yes — full Rust, loops, std, helper crates |
| Inspect a literal's contents | No | Yes (`lit.value()` gives a `String`/number) |
| Validate input at compile time | Only structural shape | Arbitrary checks → custom `compile_error!` |
| Read files at build time | No | Yes (like `include_str!`) |
| Compile-time cost | Cheap, fast | Compiles a whole crate + `syn`; slower builds |
| Best for | Variadic construction, simple templates | Custom DSLs, validation, codegen from input |

### Function-like proc macro vs. derive vs. attribute

All three are procedural macros (real functions in a `proc-macro` crate), distinguished by *how they are invoked*:

| Kind | Invoked as | Attribute on the definition | Typical job |
| --- | --- | --- | --- |
| Function-like | `foo!(...)` / `foo! { ... }` | `#[proc_macro]` | Parse a call's arguments, emit code |
| Derive | `#[derive(Foo)]` on a type | `#[proc_macro_derive(Foo)]` | Generate `impl`s for a struct/enum |
| Attribute | `#[foo]` on an item | `#[proc_macro_attribute]` | Transform/wrap the item it decorates |

This page is about the first row; the other two are in [Derive Macros](/14-macros/04-derive-macros/) and [Attribute Macros](/14-macros/05-attribute-macros/).

### Function-like macros can appear in many positions

Like `macro_rules!` macros, a function-like proc macro call can stand wherever its output is valid: in **expression** position (`let x = foo!();`), **statement** position, or **item** position (generating whole functions, structs, or `impl`s at module level). The `config!` example produced an expression; the `routes!` macro below produces an entire function. A plain function call can only ever be an expression.

---

## Common Pitfalls

### Pitfall 1: Defining a `#[proc_macro]` in a normal crate

Function-like procedural macros *must* live in a crate whose `Cargo.toml` declares `[lib] proc-macro = true`. Putting `#[proc_macro]` in a regular binary or library crate fails:

```rust
// does not compile — this is a normal binary crate, not a proc-macro crate
use proc_macro::TokenStream;

#[proc_macro]
pub fn noop(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

fn main() {}
```

Real compiler error:

```text
error: the `#[proc_macro]` attribute is only usable with crates of the `proc-macro` crate type
 --> src/main.rs:3:1
  |
3 | #[proc_macro]
  | ^^^^^^^^^^^^^

error[E0432]: unresolved import `proc_macro`
 --> src/main.rs:1:5
  |
1 | use proc_macro::TokenStream;
  |     ^^^^^^^^^^ use of unresolved module or unlinked crate `proc_macro`
```

The fix is to create a dedicated crate (often named `yourcrate-macros`) with `proc-macro = true`, and depend on it from your application crate. See [Procedural Macros](/14-macros/07-proc-macros/) for the full workspace layout.

### Pitfall 2: Expecting it to accept runtime values

A function-like macro receives **source tokens**, not values. If your `Parse` impl expects string *literals*, you cannot pass a variable:

```rust
// does not compile — `route!` parses string LITERALS, not runtime variables
use mymacros::route;

fn main() {
    let method = "get";
    let r: &str = route!(method, "/users");
    println!("{r}");
}
```

Real compiler error:

```text
error: expected string literal
 --> app/src/main.rs:6:26
  |
6 |     let r: &str = route!(method, "/users");
  |                          ^^^^^^
```

This is the deepest mental-model shift for a TypeScript/JavaScript developer: the macro runs *before* there is any such thing as the value of `method`. It sees the *identifier token* `method` and your parser rejects it because it wanted a literal. If you need a runtime value, use a function, not a macro.

### Pitfall 3: Assuming it is zero-cost to compile

A declarative macro is essentially free at build time. A function-like proc macro drags in `syn`, `quote`, and `proc-macro2`, all of which must compile, and the macro crate compiles as its own unit. For a one-off "build this string" task that `macro_rules!` could handle, the proc-macro overhead is rarely worth it. (The expansion itself is still zero *runtime* cost; only build time is affected.)

### Pitfall 4: Forgetting `--features full` on `syn`

`syn`'s default features only parse a subset of Rust. If you parse expressions, items, or use `Punctuated`, you typically need the `full` feature; otherwise you get confusing "cannot find type" or "no method" errors at compile time of the macro crate. Add it explicitly:

```toml
[dependencies]
syn = { version = "2", features = ["full"] }
quote = "1"
proc-macro2 = "1"
```

### Pitfall 5: Reaching for a proc macro when `macro_rules!` suffices

The single most common design mistake is jumping to a procedural macro for something declarative macros do cleanly. If you only need variadic construction or a "this expands to that" template, write a `macro_rules!`: it is simpler, faster to compile, and needs no extra crate. Use the decision rule in Best Practices below.

---

## Best Practices

### Decide declarative-first, procedural-only-when-needed

Use this checklist before writing a function-like proc macro:

1. **Can a plain function do it?** If the inputs are runtime values, use a function. Stop.
2. **Can `macro_rules!` do it?** If you only need variadic args, simple repetition, or a fixed "expands to that" template, write a declarative macro (see [Declarative Macros](/14-macros/01-declarative-macros/) and [Repetition](/14-macros/03-repetition/)). Stop.
3. **Do you need to *inspect or validate* the input's contents, parse a *custom grammar*, or *read files* at build time?** Now a function-like procedural macro earns its keep.

### Emit good compile errors with spans

When you reject input, build the error with `syn::Error::new(span, message)` and return `.to_compile_error()`. Attaching the right `span` (here `path.span()`) makes the caret point at the exact token, just like a built-in error. Vague errors with the wrong span are a frequent source of user frustration.

### Keep the macro crate thin

Put as little logic as possible directly in the `#[proc_macro]` function; factor parsing and generation into helper functions and types so you can unit-test them. Procedural-macro code that returns a `proc_macro2::TokenStream` from a helper is far easier to test than one tangled with `proc_macro::TokenStream` (which only exists inside the compiler).

### Name the crate by convention

The community convention is to name the proc-macro crate `<yourcrate>-macros` (or `<yourcrate>-derive`) and re-export its macros from your main crate so users see a single dependency. Many crates you already use — `serde`, `tokio`, `clap` — do exactly this.

### Pin the standard toolset

Use `syn` 2.x, `quote` 1.x, and `proc-macro2` 1.x: the current, stable, near-universal stack. (`syn` 1.x is legacy; new code should target 2.x.)

---

## Real-World Example

A production-flavored routing-table DSL. The macro reads entries of the form `METHOD "/path" => handler`, validates every path at compile time, and generates a `dispatch` function, an *item*, not just an expression. This is the same idea behind web-framework route macros, distilled.

```rust
// ===== crate `mymacros` (lib.rs) =====
use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Ident, LitStr, Token};

struct RouteEntry {
    method: Ident,
    path: LitStr,
    handler: Ident,
}

impl Parse for RouteEntry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let method: Ident = input.parse()?;
        let path: LitStr = input.parse()?;
        input.parse::<Token![=>]>()?;
        let handler: Ident = input.parse()?;
        Ok(RouteEntry { method, path, handler })
    }
}

struct RoutesInput {
    entries: Punctuated<RouteEntry, Token![,]>,
}

impl Parse for RoutesInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(RoutesInput {
            entries: Punctuated::parse_terminated(input)?,
        })
    }
}

#[proc_macro]
pub fn routes(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as RoutesInput);

    let mut arms = Vec::new();
    for entry in parsed.entries.iter() {
        let path_value = entry.path.value();
        // Compile-time validation, once per route.
        if !path_value.starts_with('/') {
            return syn::Error::new(entry.path.span(), "route path must start with '/'")
                .to_compile_error()
                .into();
        }
        let method = entry.method.to_string().to_uppercase();
        let path = &entry.path;
        let handler_name = entry.handler.to_string();
        arms.push(quote! {
            (#method, #path) => #handler_name,
        });
    }

    quote! {
        fn dispatch(method: &str, path: &str) -> &'static str {
            match (method, path) {
                #( #arms )*
                _ => "404 Not Found",
            }
        }
    }
    .into()
}
```

```rust
// ===== crate `app` (main.rs) =====
use mymacros::routes;

// Expands into a whole `fn dispatch(..)` at module level (item position).
routes! {
    GET  "/users"  => list_users,
    POST "/users"  => create_user,
    GET  "/health" => health_check,
}

fn main() {
    println!("{}", dispatch("GET", "/users"));
    println!("{}", dispatch("POST", "/users"));
    println!("{}", dispatch("DELETE", "/users"));
}
```

Real output:

```text
list_users
create_user
404 Not Found
```

Three things to notice. First, the macro runs a *loop* over the parsed entries; declarative macros cannot do arbitrary iteration with logic in the body like this. Second, every path is validated during compilation; a typo'd `"users"` would be a compiler error before the program ever runs, unlike the TypeScript registry that only threw at runtime. Third, the macro emitted an entire function definition, demonstrating that function-like proc macros work in item position, not just as expressions.

---

## Further Reading

### Official documentation

- [The Rust Reference — Function-like procedural macros](https://doc.rust-lang.org/reference/procedural-macros.html#function-like-procedural-macros)
- [The Rust Book — Macros](https://doc.rust-lang.org/book/ch20-05-macros.html)
- [`syn` documentation](https://docs.rs/syn/latest/syn/) and [`quote` documentation](https://docs.rs/quote/latest/quote/)
- [`proc_macro` standard module](https://doc.rust-lang.org/proc_macro/): the `TokenStream` API the compiler hands you.
- [`std::include_str!`](https://doc.rust-lang.org/std/macro.include_str.html): a built-in example of build-time file reading.

### Related sections in this guide

- [Procedural Macros](/14-macros/07-proc-macros/) — the full proc-macro crate setup, `TokenStream`, `syn` 2 + `quote`, and a compile-verified custom derive.
- [Macro Basics](/14-macros/00-macro-basics/): what macros are and are *not* (not decorators, not functions); compile-time expansion and hygiene.
- [Declarative Macros](/14-macros/01-declarative-macros/) and [Repetition](/14-macros/03-repetition/) — the simpler `macro_rules!` family you should try first.
- [Macro Patterns](/14-macros/02-macro-patterns/): fragment specifiers (`:expr`, `:ident`, `:ty`, `:tt`), which `syn` mirrors at the parser level.
- [Derive Macros](/14-macros/04-derive-macros/) and [Attribute Macros](/14-macros/05-attribute-macros/) — the other two procedural-macro shapes.
- [Common Macros](/14-macros/08-common-macros/): `include_str!`, `format!`, and the rest of the standard library's macros.
- Background: [Output and Formatting](/02-basics/04-output/) introduced macro-call syntax; the [Introduction](/00-introduction/) and [Getting Started](/01-getting-started/) set the stage.
- Applied: [Serialization](/15-serialization/) leans on `serde`'s derive and the `serde_json::json!` DSL; table-driven tests appear in [Testing](/13-testing/).

---

## Exercises

### Exercise 1

**Difficulty:** Easy

**Objective:** Write your first function-like procedural macro that processes its input with real Rust code.

**Instructions:** In a `proc-macro` crate, write a `shout!` macro that takes a single string literal and expands to the same string uppercased at compile time. `shout!("hello")` should evaluate to the `&'static str` `"HELLO"`. (Use `parse_macro_input!(input as LitStr)`, then `.value().to_uppercase()`, then `quote!`.)

```rust
// in the proc-macro crate
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitStr};

#[proc_macro]
pub fn shout(input: TokenStream) -> TokenStream {
    // TODO: parse a LitStr, uppercase its value, emit it
}
```

<details>
<summary>Solution</summary>

```rust
// in the proc-macro crate (lib.rs)
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitStr};

#[proc_macro]
pub fn shout(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    let shouted = lit.value().to_uppercase();
    quote! { #shouted }.into()
}
```

```rust
// in the consumer crate (main.rs)
use mymacros::shout;

fn main() {
    let s: &str = shout!("hello");
    println!("{s}");
}
```

Output:

```text
HELLO
```

The uppercasing runs in the compiler; the binary just contains the literal `"HELLO"`.

</details>

### Exercise 2

**Difficulty:** Medium

**Objective:** Parse two arguments of different kinds and compute a result at compile time.

**Instructions:** Write a `repeat_str!` macro that takes a string literal and an integer literal, and expands to the string repeated that many times. `repeat_str!("ab", 3)` should evaluate to `"ababab"`. Define a `Parse` impl that reads `LitStr , LitInt`, convert the count with `count.base10_parse::<usize>()`, and use `str::repeat`.

```rust
// in the proc-macro crate
use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, LitInt, LitStr, Token};

struct RepeatInput {
    text: LitStr,
    _comma: Token![,],
    count: LitInt,
}

impl Parse for RepeatInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // TODO: parse text, comma, count
    }
}

#[proc_macro]
pub fn repeat_str(input: TokenStream) -> TokenStream {
    // TODO: parse, compute the repeated string, emit it
}
```

<details>
<summary>Solution</summary>

```rust
// in the proc-macro crate (lib.rs)
use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, LitInt, LitStr, Token};

struct RepeatInput {
    text: LitStr,
    _comma: Token![,],
    count: LitInt,
}

impl Parse for RepeatInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(RepeatInput {
            text: input.parse()?,
            _comma: input.parse()?,
            count: input.parse()?,
        })
    }
}

#[proc_macro]
pub fn repeat_str(input: TokenStream) -> TokenStream {
    let RepeatInput { text, count, .. } = parse_macro_input!(input as RepeatInput);
    let n: usize = match count.base10_parse() {
        Ok(n) => n,
        Err(e) => return e.to_compile_error().into(),
    };
    let repeated = text.value().repeat(n);
    quote! { #repeated }.into()
}
```

```rust
// in the consumer crate (main.rs)
use mymacros::repeat_str;

fn main() {
    let r: &str = repeat_str!("ab", 3);
    println!("{r}");
}
```

Output:

```text
ababab
```

Returning `e.to_compile_error()` means a count that overflows `usize` (a literal too large to fit, like `repeat_str!("ab", 99999999999999999999999999999999)`) produces a clean compiler error instead of a panic inside the macro. A non-numeric token is rejected even earlier, by the `count: LitInt` parse step, which expects an integer literal.

</details>

### Exercise 3

**Difficulty:** Hard

**Objective:** Validate the *contents* of a literal and emit a custom compile error: the capability that sets function-like proc macros apart from `macro_rules!`.

**Instructions:** Write a `hex_color!` macro that takes a string literal like `"#ff8800"` and expands to a `(u8, u8, u8)` RGB tuple, parsed at compile time. It must accept an optional leading `#`, require exactly six hex digits, and emit a compile error (via `syn::Error::new(span, msg).to_compile_error()`) for anything else. `hex_color!("#ff8800")` should evaluate to `(255, 136, 0)`, while `hex_color!("#zz0011")` should fail to compile.

```rust
// in the proc-macro crate
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitStr};

#[proc_macro]
pub fn hex_color(input: TokenStream) -> TokenStream {
    // TODO: parse a LitStr, strip optional '#', validate 6 hex digits,
    //       parse each pair with u8::from_str_radix(.., 16), emit (r, g, b)
}
```

<details>
<summary>Solution</summary>

```rust
// in the proc-macro crate (lib.rs)
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitStr};

#[proc_macro]
pub fn hex_color(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    let s = lit.value();
    let hex = s.strip_prefix('#').unwrap_or(&s);

    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return syn::Error::new(lit.span(), "expected a 6-digit hex color like \"#ff8800\"")
            .to_compile_error()
            .into();
    }

    let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap();
    quote! { (#r, #g, #b) }.into()
}
```

```rust
// in the consumer crate (main.rs)
use mymacros::hex_color;

fn main() {
    let (red, green, blue): (u8, u8, u8) = hex_color!("#ff8800");
    println!("rgb({red}, {green}, {blue})");
}
```

Output:

```text
rgb(255, 136, 0)
```

Feeding it an invalid color — `hex_color!("#zz0011")` — produces a compile error instead. With this consumer file:

```rust
// does not compile — "#zz0011" is not valid hex
use mymacros::hex_color;

fn main() {
    let _c: (u8, u8, u8) = hex_color!("#zz0011");
}
```

the compiler reports:

```text
error: expected a 6-digit hex color like "#ff8800"
 --> src/main.rs:4:39
  |
4 |     let _c: (u8, u8, u8) = hex_color!("#zz0011");
  |                                       ^^^^^^^^^
```

The validation runs entirely in the compiler, so an invalid color literal can never reach a running program, the kind of guarantee no runtime JavaScript validator can give you.

</details>
