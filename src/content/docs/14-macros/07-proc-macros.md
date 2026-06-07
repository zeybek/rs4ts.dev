---
title: "Procedural Macros: Writing Compiler Plugins with syn and quote"
description: "Write Rust procedural macros that run inside the compiler: parse tokens with syn, generate code with quote, type-safe and zero-cost where TypeScript uses"
---

## Quick Overview

A **procedural macro** is a small Rust program that runs *inside the compiler*: it receives your code as a stream of tokens, transforms it with ordinary Rust logic, and hands back new tokens that the compiler then type-checks like anything else. This is how `#[derive(Serialize)]`, `#[tokio::main]`, and `sqlx::query!` work, and writing your own is the closest Rust gets to TypeScript's transformer plugins or Babel macros, except it is type-safe, hygienic, and has zero runtime cost. This page covers the mechanics: the special `proc-macro` crate type, the `TokenStream` type, and the `syn` 2 + `quote` + `proc-macro2` trio you will use for every nontrivial proc macro, ending with a full compile-verified custom `derive`.

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects the newest edition automatically. Every Rust snippet here was compiled and run on stable with `syn` 2.0.117, `quote` 1.0.45, and `proc-macro2` 1.0.106.

> **Tip:** If you have not read [Macro Basics](/14-macros/00-macro-basics/) and [Derive Macros](/14-macros/04-derive-macros/) yet, do so first. This page assumes you know that macros are compile-time, token-based, and hygienic, and that `#[derive(...)]` is one of the three procedural-macro flavors.

---

## TypeScript/JavaScript Example

There is nothing in plain TypeScript that operates on your code the way a proc macro does. The nearest cousins are **AST transformers** — a TypeScript compiler transform, a Babel plugin, or a tool like `babel-plugin-macros`. They parse source into an Abstract Syntax Tree, walk and rewrite nodes, and print code back out. Here is a hand-written Babel-style transform that auto-generates a `describe()` method onto a class:

```typescript
// babel-style transformer (runs in the BUILD step, on the AST).
import type { PluginObj, types as BabelTypes } from "@babel/core";

export default function addDescribe({ types: t }: { types: typeof BabelTypes }): PluginObj {
  return {
    name: "add-describe",
    visitor: {
      ClassDeclaration(path) {
        const className = path.node.id?.name ?? "Anonymous";

        // Collect the names of class fields from the AST.
        const fieldNames = path.node.body.body
          .filter((m) => t.isClassProperty(m) && t.isIdentifier(m.key))
          .map((m) => ((m as BabelTypes.ClassProperty).key as BabelTypes.Identifier).name);

        // Build the body of a `describe()` method as a template string.
        const lines = fieldNames
          .map((f) => `lines.push(\`  ${f} = \${JSON.stringify(this.${f})}\`);`)
          .join("\n");
        const methodSrc = `
          describe() {
            const lines = ["${className} {"];
            ${lines}
            lines.push("}");
            return lines.join("\\n");
          }`;

        // Parse that string and splice the new method into the class.
        const method = t.classMethod(/* ...build from methodSrc... */);
        path.node.body.body.push(method);
      },
    },
  };
}
```

A user of the plugin writes a normal class and the build step injects the method:

```typescript
class User {
  id = 0;
  name = "";
  active = false;
}
// After transform, `new User().describe()` exists at runtime.
```

Two things to hold onto. First, the transform manipulates an **AST** (`ClassDeclaration`, `ClassProperty`, `Identifier`) — exactly what `syn` gives you in Rust. Second, when this Babel plugin builds new code it does it by **string concatenation** (`methodSrc`), which is fragile: a stray brace or an unescaped value silently produces broken output, and nothing checks the result until runtime.

---

## Rust Equivalent

The Rust version is a separate crate of the special `proc-macro` type. It parses the input with `syn`, builds output with `quote!` (structured token interpolation, *not* string concatenation), and the compiler type-checks the generated code immediately.

The macro crate, `describe_derive/src/lib.rs`:

```rust
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

/// Derives a `describe(&self) -> String` method that lists each field
/// name and its `Debug` value — the Rust analogue of the Babel plugin.
#[proc_macro_derive(Describe)]
pub fn derive_describe(input: TokenStream) -> TokenStream {
    // 1. PARSE the incoming tokens into a typed syntax tree.
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    // 2. INSPECT: require a struct with named fields, or emit a real error.
    let fields = match &ast.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn::Error::new_spanned(
                    &ast.ident,
                    "Describe only supports structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                &ast.ident,
                "Describe can only be derived for structs",
            )
            .to_compile_error()
            .into();
        }
    };

    // 3. BUILD one push_str line per field. `quote!` interpolates with `#`.
    let field_lines = fields.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let field_label = field_name.to_string();
        quote! {
            out.push_str(&format!("  {} = {:?}\n", #field_label, &self.#field_name));
        }
    });

    let type_label = name.to_string();

    // 4. EMIT a complete `impl` block as new tokens.
    let expanded = quote! {
        impl #name {
            pub fn describe(&self) -> String {
                let mut out = String::new();
                out.push_str(&format!("{} {{\n", #type_label));
                #( #field_lines )*
                out.push('}');
                out
            }
        }
    };

    expanded.into()
}
```

Its `Cargo.toml`. Note the `proc-macro = true` flag, which makes this a compiler-plugin crate:

```toml
[package]
name = "describe_derive"
version = "0.1.0"
edition = "2024"

[lib]
proc-macro = true

[dependencies]
syn = "2"
quote = "1"
proc-macro2 = "1"
```

A separate consumer crate uses it like any built-in derive:

```rust
use describe_derive::Describe;

#[derive(Describe)]
struct User {
    id: u64,
    name: String,
    active: bool,
}

fn main() {
    let u = User { id: 7, name: "Ada".to_string(), active: true };
    println!("{}", u.describe());
}
```

Real output:

```text
User {
  id = 7
  name = "Ada"
  active = true
}
```

Unlike the Babel plugin's `methodSrc` string, the `quote! { ... }` block is parsed by the `quote` crate into structured tokens; a malformed interpolation is a compile error in the *macro* crate, and the generated `impl` is type-checked in the *consumer* crate. Nothing reaches runtime unchecked.

---

## Detailed Explanation

### The three crates: proc-macro, proc-macro2, and the syn/quote pair

Proc macros are built on three layers. Getting them straight removes most of the early confusion:

- **`proc_macro`** is a crate baked into the compiler (no `cargo add` needed). It defines `proc_macro::TokenStream`, the type your macro functions must take and return. It *only* exists while the compiler is running, so you cannot use it in a regular binary, a test, or a build script.
- **`proc-macro2`** ([crates.io](https://crates.io/crates/proc-macro2)) is a mirror of that API that works **anywhere**: in unit tests, build scripts, and on the host. Its `proc_macro2::TokenStream` is what `syn` and `quote` actually speak. You convert between the two with `.into()`.
- **`syn`** parses a `TokenStream` into a typed syntax tree (`DeriveInput`, `ItemFn`, `Expr`, ...), and **`quote`** does the reverse: it turns a template with `#interpolation` into a `TokenStream`. They are by the same author (dtolnay) and are the de-facto standard.

The data flow of every proc macro is the same pipeline:

```text
proc_macro::TokenStream  --.into()-->  proc_macro2::TokenStream
        (compiler)                            |
                                         syn::parse2 / parse_macro_input!
                                              v
                                      typed AST (DeriveInput, ...)
                                              |
                                         your Rust logic
                                              v
                                       quote! { ... }  --> proc_macro2::TokenStream
                                              |
proc_macro::TokenStream  <--.into()-----------'
        (back to compiler)
```

### What a `TokenStream` actually is

A `TokenStream` is a flat sequence of **token trees**: identifiers, literals, punctuation, and *delimited groups* (`(...)`, `[...]`, `{...}`) which nest. It is **not** a string and **not** a fully parsed AST; it sits between the two. For `struct User { id: u64 }` the stream is roughly the tokens `struct`, `User`, and a brace-group containing `id`, `:`, `u64`. Importantly, every token carries a **span**: a record of where it came from in the source, which drives both error messages and hygiene (see [Macro Basics](/14-macros/00-macro-basics/)). Because the input is already tokenized — not raw text — `quote!` cannot accidentally glue two identifiers together, and `:expr`-style grouping is preserved automatically.

You can build and inspect a `proc_macro2::TokenStream` entirely on the host, which is why the standalone demo below runs as a normal binary:

```rust
use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

fn main() {
    // `quote!` builds a proc_macro2::TokenStream from a template.
    let source: TokenStream = quote! {
        struct Point { x: i32, y: i32 }
    };

    // `syn::parse2` parses tokens into a typed AST node.
    let ast: DeriveInput = syn::parse2(source).expect("valid struct");
    println!("parsed type name: {}", ast.ident);

    // Build new tokens and render them back to a string.
    let name = ast.ident;
    let generated: TokenStream = quote! {
        impl #name {
            fn type_name() -> &'static str { stringify!(#name) }
        }
    };
    println!("generated tokens: {generated}");
}
```

Run as an ordinary binary (dependencies: `syn` with the `full` feature, `quote`, `proc-macro2`). Real output:

```text
parsed type name: Point
generated tokens: impl Point { fn type_name () -> & 'static str { stringify ! (Point) } }
```

Notice the `Display` of a `TokenStream` re-inserts spaces between every token; it is for debugging, not for pretty source. (For readable output, pipe through [`cargo expand`](https://github.com/dtolnay/cargo-expand).)

### Walking the `Describe` derive line by line

**`#[proc_macro_derive(Describe)]`** registers a derive macro named `Describe`. The function it decorates must have the signature `fn(TokenStream) -> TokenStream` and live at the crate root of a `proc-macro` crate. The function name (`derive_describe`) is irrelevant to users; the `Describe` in the attribute is what they write in `#[derive(Describe)]`.

**`parse_macro_input!(input as DeriveInput)`** parses the token stream into `syn`'s `DeriveInput`, the catch-all node for "the thing a derive is attached to" (a struct, enum, or union). Unlike `syn::parse2`, this macro is purpose-built for proc-macro entry points: on a parse error it *returns* a `compile_error!` invocation from your function automatically, so you never hand back garbage tokens.

**Matching on `ast.data`** distinguishes `Data::Struct`, `Data::Enum`, and `Data::Union`. We only support named-field structs, so every other shape produces a deliberate error via `syn::Error::new_spanned(&ast.ident, "...").to_compile_error()`. Passing `&ast.ident` as the span means the error underlines *the type name in the user's code*, not some location inside our macro.

**Building `field_lines`** maps over each field to produce a small `TokenStream` fragment. Inside `quote!`, `#field_label` interpolates the field's name as a string literal and `#field_name` interpolates the identifier so `self.#field_name` becomes e.g. `self.id`. These fragments are themselves tokens, so they compose.

**The final `quote!`** assembles the `impl` block. The repetition `#( #field_lines )*` — analogous to `macro_rules!` repetition from [Repetition](/14-macros/03-repetition/) — splices every fragment in sequence. Then `.into()` converts the `proc_macro2::TokenStream` that `quote!` produced back into the `proc_macro::TokenStream` the compiler expects.

### Seeing the generated code with `cargo expand`

You never have to guess what your macro emits. `cargo install cargo-expand`, then `cargo expand` prints the post-expansion source. For the `User` above, the relevant portion is:

```rust
impl User {
    pub fn describe(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("{} {{\n", "User"));
        out.push_str(&format!("  {} = {:?}\n", "id", &self.id));
        out.push_str(&format!("  {} = {:?}\n", "name", &self.name));
        out.push_str(&format!("  {} = {:?}\n", "active", &self.active));
        out.push('}');
        out
    }
}
```

(In the raw `cargo expand` output the `format!` calls are themselves further expanded into `::alloc::fmt::format(format_args!(...))`, because *every* macro is expanded, `format!` included. The shape above is the meaningful part.)

---

## Key Differences

| Concept | TypeScript AST transform (Babel/ts plugin) | Rust procedural macro |
| --- | --- | --- |
| When it runs | Build step, as a separate Node process | Inside `rustc`, during compilation |
| Input | AST nodes from the parser | `TokenStream` (token trees, not full AST) |
| Parsing | Done for you by the compiler | You call `syn` to parse tokens into an AST |
| Output construction | String templating or AST builders | `quote!` structured interpolation |
| Output checking | Re-parsed; type errors surface at runtime | Type-checked immediately as real Rust |
| Hygiene | Manual; name clashes are your problem | Span-based hygiene built in |
| Distribution | An npm package + build config | A crate with `proc-macro = true` |
| Runtime cost | The generated JS runs at runtime | Zero — only generated code remains |

### Three flavors share one foundation

All procedural macros use `syn`/`quote`/`proc-macro2`; they differ only in their entry-point attribute and signature:

| Flavor | Attribute | Signature | Invoked as | Covered in |
| --- | --- | --- | --- | --- |
| Custom derive | `#[proc_macro_derive(Name)]` | `fn(TokenStream) -> TokenStream` | `#[derive(Name)]` | [Derive Macros](/14-macros/04-derive-macros/) |
| Attribute | `#[proc_macro_attribute]` | `fn(TokenStream, TokenStream) -> TokenStream` | `#[name(args)]` | [Attribute Macros](/14-macros/05-attribute-macros/) |
| Function-like | `#[proc_macro]` | `fn(TokenStream) -> TokenStream` | `name!(...)` | [Function-like Macros](/14-macros/06-function-like-macros/) |

A derive macro **adds** code alongside the item it annotates (it never sees the item replaced). An attribute macro receives both its arguments and the *entire* annotated item, and returns whatever should replace it. A function-like macro receives only the tokens between its delimiters.

### Why a separate crate?

A `proc-macro` crate is compiled *for the compiler's own host* and loaded as a plugin while it compiles your other crates. That is a fundamentally different compilation target from your application, so the macro logic must live in its own crate that *only* exports macros (this is enforced; see the pitfalls). The common pattern in published libraries is a pair: a `foo_derive` proc-macro crate and a `foo` crate that re-exports it, so users only depend on `foo`. `serde` does exactly this with `serde` and `serde_derive`.

---

## Common Pitfalls

### Pitfall 1: Putting a proc macro in a normal crate

The `#[proc_macro_derive]` family is only legal in a crate with `proc-macro = true`. Forget the flag and you get two errors at once:

```rust
// in a crate WITHOUT `proc-macro = true` in Cargo.toml
use proc_macro::TokenStream; // does not compile (error E0432)

#[proc_macro_derive(Oops)] // does not compile
pub fn derive_oops(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}
```

Real compiler errors:

```text
error: the `#[proc_macro_derive]` attribute is only usable with crates of the `proc-macro` crate type
 --> src/lib.rs:3:1
  |
3 | #[proc_macro_derive(Oops)]
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0432]: unresolved import `proc_macro`
 --> src/lib.rs:1:5
  |
1 | use proc_macro::TokenStream;
  |     ^^^^^^^^^^ use of unresolved module or unlinked crate `proc_macro`
```

The fix is to add `[lib]` with `proc-macro = true` to the crate's `Cargo.toml`. The `proc_macro` crate only becomes importable once that flag is set.

### Pitfall 2: Confusing `proc_macro::TokenStream` with `proc_macro2::TokenStream`

These are two distinct types with the same name. `syn` and `quote` produce `proc_macro2::TokenStream`, but your entry point must return `proc_macro::TokenStream`. Forget the `.into()` and:

```rust
// inside #[proc_macro_derive], returning the quote! result directly:
    let expanded = quote! { /* ... */ };
    expanded // does not compile (error E0308) — missing `.into()`
```

Real compiler error (excerpt):

```text
error[E0308]: mismatched types
   --> describe_derive/src/lib.rs:54:5
    |
  6 | pub fn derive_describe(input: TokenStream) -> TokenStream {
    |                                               ----------- expected `proc_macro::TokenStream` because of return type
...
 54 |     expanded
    |     ^^^^^^^^ expected `proc_macro::TokenStream`, found `proc_macro2::TokenStream`
    |
    = note: `proc_macro2::TokenStream` and `proc_macro::TokenStream` have similar names, but are actually distinct types
help: call `Into::into` on this expression to convert `proc_macro2::TokenStream` into `proc_macro::TokenStream`
    |
 54 |     expanded.into()
    |             +++++++
```

The compiler even suggests the fix: `expanded.into()`. A clean convention is to keep all internal logic in `proc_macro2` and only convert at the very edges of the entry function.

### Pitfall 3: Returning a parse error as a panic instead of `compile_error!`

A tempting but bad habit is `let ast = syn::parse2(input).unwrap();`. If parsing fails, your macro *panics*, and the user sees an opaque "proc-macro panicked" message with no useful span. Instead, return errors as tokens: `syn::Error` has `.to_compile_error()` (one error) and `.into_compile_error()` (consuming form), and `parse_macro_input!` does this for you automatically. Always propagate errors through the token stream so the user gets a real, located diagnostic pointing at *their* code, as the `Describe` derive does for enums (see Pitfall 4).

### Pitfall 4: Forgetting to handle struct shapes (and emitting a vague error)

If you only handle named-field structs, decide what happens for everything else. The `Describe` derive returns a clear `compile_error!` for enums. With it applied to an enum:

```rust
#[derive(Describe)] // does not compile — Describe rejects enums on purpose
enum Shape {
    Circle,
    Square,
}
```

Real compiler error:

```text
error: Describe can only be derived for structs
 --> consumer/src/main.rs:4:6
  |
4 | enum Shape {
  |      ^^^^^
```

Because we built the error with `new_spanned(&ast.ident, ...)`, it underlines `Shape` — the user's type — instead of something inside the macro. Compare that to an unwrapped panic, which would point nowhere useful.

### Pitfall 5: Slow iteration and forgetting the `full` feature

`syn` is feature-gated to keep compile times down. The default features parse derive inputs and types; parsing whole function bodies, expressions, or statements needs `features = ["full"]`. If `syn::parse2::<ItemFn>(...)` fails to resolve or behaves oddly, check that you enabled `full`. And because every edit to a proc-macro crate recompiles `syn`, keep the macro crate small and test the pure logic with `proc-macro2` (next section) rather than recompiling the whole consumer each time.

---

## Best Practices

### Split a testable core from the thin entry point

The `proc_macro` types cannot be used outside the compiler, so wrap them at the boundary and put all real logic in a function over `proc_macro2::TokenStream`. That core is unit-testable like any function:

```rust
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse2, DeriveInput};

// Thin shim: convert, delegate, convert back. The only `proc_macro` contact.
#[proc_macro_derive(Named)]
pub fn derive_named(input: TokenStream) -> TokenStream {
    expand(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

// Pure core: uses only proc_macro2, so it runs on the host and in tests.
fn expand(input: TokenStream2) -> syn::Result<TokenStream2> {
    let ast: DeriveInput = parse2(input)?;
    let name = &ast.ident;
    let label = name.to_string();
    Ok(quote! {
        impl #name {
            pub fn type_name(&self) -> &'static str { #label }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::expand;
    use quote::quote;

    #[test]
    fn generates_type_name_impl() {
        let input = quote! { struct Widget { id: u32 } };
        let output = expand(input).unwrap().to_string();
        assert!(output.contains("impl Widget"));
        assert!(output.contains("\"Widget\""));
    }
}
```

Real output of `cargo test -p` on this crate:

```text
running 1 test
test tests::generates_type_name_impl ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Note how `expand` returns `syn::Result<TokenStream2>` and the shim funnels any error into `into_compile_error()`. This is the idiomatic shape used across the ecosystem. (For end-to-end tests that check the macro produces a *compile error* on bad input, reach for [`trybuild`](https://crates.io/crates/trybuild); see [Testing](/13-testing/).)

### Always preserve generics

A derive that assumes the type has no generics breaks the moment someone writes `struct Wrapper<T>`. `syn` makes this trivial with `split_for_impl`, which yields the three pieces an `impl` needs:

```rust
let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
let expanded = quote! {
    impl #impl_generics #name #ty_generics #where_clause {
        // ...
    }
};
```

This expands to `impl<T> Wrapper<T> where ... { ... }` automatically. If your impl requires a bound on each type parameter (e.g. `T: Debug`), add it to `ast.generics` before splitting.

### Use helper attributes instead of magic conventions

When a derive needs per-field configuration, declare **inert helper attributes** in the `attributes(...)` list rather than inventing naming conventions. This is exactly how `serde` offers `#[serde(rename = "...")]`. The Real-World Example below shows the full pattern with `attr.parse_nested_meta`.

### Point spans at the user's code, and prefer `quote_spanned!` for generated bounds

Every diagnostic your macro emits should underline something in *their* source. Use `syn::Error::new_spanned(&offending_node, msg)` so the squiggle lands on the right token. When generated code itself can fail to compile (a missing trait bound, say), `quote_spanned!` lets you attribute that failure to a meaningful user span instead of an opaque macro location.

---

## Real-World Example

A production-flavored custom derive, `Report`, that turns a struct into a human-readable report and supports two **helper attributes**: `#[report(label = "...")]` to rename a field and `#[report(skip)]` to omit a sensitive field. This mirrors how real derives (`serde`, `clap`) are configured per field.

The macro crate, `report_derive/src/lib.rs` (with `proc-macro = true` and the same `syn`/`quote`/`proc-macro2` dependencies):

```rust
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, LitStr};

/// Derives `report(&self) -> String`. Configure per field with
/// `#[report(label = "Nice Name")]` and `#[report(skip)]`.
#[proc_macro_derive(Report, attributes(report))]
pub fn derive_report(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    // let-else gives a clean early exit with a real compile error.
    let Data::Struct(data) = &ast.data else {
        return syn::Error::new_spanned(name, "Report can only be derived for structs")
            .to_compile_error()
            .into();
    };
    let Fields::Named(fields) = &data.fields else {
        return syn::Error::new_spanned(name, "Report needs named fields")
            .to_compile_error()
            .into();
    };

    let mut lines = Vec::new();
    for field in &fields.named {
        let ident = field.ident.as_ref().unwrap();
        let mut label = ident.to_string();
        let mut skip = false;

        // Parse each `#[report(...)]` helper attribute on this field.
        for attr in &field.attrs {
            if !attr.path().is_ident("report") {
                continue;
            }
            let parsed = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    skip = true;
                    Ok(())
                } else if meta.path.is_ident("label") {
                    let value: LitStr = meta.value()?.parse()?;
                    label = value.value();
                    Ok(())
                } else {
                    Err(meta.error("unknown `report` option"))
                }
            });
            if let Err(e) = parsed {
                return e.to_compile_error().into();
            }
        }

        if skip {
            continue;
        }
        lines.push(quote! {
            rows.push(format!("{}: {:?}", #label, &self.#ident));
        });
    }

    let expanded = quote! {
        impl #name {
            pub fn report(&self) -> String {
                let mut rows: Vec<String> = Vec::new();
                #( #lines )*
                rows.join("\n")
            }
        }
    };
    expanded.into()
}
```

The consumer crate:

```rust
use report_derive::Report;

#[derive(Report)]
struct Account {
    #[report(label = "Account ID")]
    id: u64,
    #[report(label = "Owner")]
    owner: String,
    #[report(skip)]
    #[allow(dead_code)] // never read because `report` skips it
    password_hash: String,
}

fn main() {
    let acct = Account {
        id: 42,
        owner: "Grace".to_string(),
        password_hash: "do-not-print".to_string(),
    };
    println!("{}", acct.report());
}
```

Real output:

```text
Account ID: 42
Owner: "Grace"
```

The `password_hash` field is omitted entirely because of `#[report(skip)]`, and the two visible fields use their custom labels. The key API is `attr.parse_nested_meta`, the `syn` 2 way to walk the comma-separated options inside `#[report(...)]`: `meta.path.is_ident("skip")` matches a bare flag, while `meta.value()?.parse::<LitStr>()?` reads the right-hand side of `label = "..."`. Anything unrecognized returns `meta.error(...)`, which surfaces as a located compile error on the user's attribute, never a panic.

> **Note:** `syn` 2.0 replaced the older 1.x `Attribute::parse_meta` / `NestedMeta` API with `Meta` plus `parse_nested_meta`. If you find a tutorial using `NestedMeta` or `AttributeArgs`, it targets `syn` 1 and will not compile against `syn` 2. Always check the version.

---

## Further Reading

### Official documentation

- [The Rust Reference — Procedural Macros](https://doc.rust-lang.org/reference/procedural-macros.html): the authoritative spec for derive, attribute, and function-like proc macros.
- [The Rust Book — Macros](https://doc.rust-lang.org/book/ch20-05-macros.html): includes a worked custom-derive walkthrough.
- [`syn` documentation](https://docs.rs/syn) and the [`syn` examples directory](https://github.com/dtolnay/syn/tree/master/examples): a full `derive(HeapSize)` example among others.
- [`quote` documentation](https://docs.rs/quote) — interpolation and repetition with `#(...)*`.
- [`proc-macro2` documentation](https://docs.rs/proc-macro2) — the host-usable `TokenStream`.
- [`cargo-expand`](https://github.com/dtolnay/cargo-expand) and [`trybuild`](https://crates.io/crates/trybuild): inspect expansions and test compile-fail cases.

### Related sections in this guide

- Foundation: [Macro Basics](/14-macros/00-macro-basics/): why macros are compile-time, token-based, and hygienic.
- [Derive Macros](/14-macros/04-derive-macros/) — what `#[derive(...)]` generates and a custom-derive overview.
- [Attribute Macros](/14-macros/05-attribute-macros/): the `#[proc_macro_attribute]` flavor.
- [Function-like Macros](/14-macros/06-function-like-macros/) — the `#[proc_macro]` `name!(...)` flavor.
- Contrast: [Declarative Macros](/14-macros/01-declarative-macros/), [Macro Patterns](/14-macros/02-macro-patterns/), and [Repetition](/14-macros/03-repetition/) cover `macro_rules!`, which needs no extra crates.
- [Common Macros](/14-macros/08-common-macros/) — the standard-library macros you will use daily.
- Testing your macros: [Section 13 — Testing](/13-testing/).
- The payoff in practice: [Serialization](/15-serialization/) is built on the `#[derive(Serialize, Deserialize)]` proc macros.
- Background: [Getting Started](/01-getting-started/) (crates and `cargo`), [Basics](/02-basics/), and the [Introduction](/00-introduction/).

---

## Exercises

### Exercise 1

**Difficulty:** Easy

**Objective:** Get a proc-macro crate building and prove the entry-point conversion to yourself.

**Instructions:** Create a `proc-macro` crate that exposes a `#[proc_macro_derive(TypeName)]` macro generating a `type_name(&self) -> &'static str` method returning the struct's name as a string literal. You do not need to inspect fields. Apply it to a struct in a consumer crate and print the result. Remember the `[lib] proc-macro = true` flag and the final `.into()`.

```rust
// type_name_derive/src/lib.rs
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(TypeName)]
pub fn derive_type_name(input: TokenStream) -> TokenStream {
    // TODO: parse, grab the ident, emit an impl with a type_name method
    todo!()
}
```

<details>
<summary>Solution</summary>

```rust
// type_name_derive/src/lib.rs  (crate has `proc-macro = true`, deps: syn 2, quote 1)
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(TypeName)]
pub fn derive_type_name(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;
    let label = name.to_string();
    let expanded = quote! {
        impl #name {
            pub fn type_name(&self) -> &'static str { #label }
        }
    };
    expanded.into()
}
```

```rust
// consumer/src/main.rs
use type_name_derive::TypeName;

#[derive(TypeName)]
struct Widget {
    #[allow(dead_code)]
    id: u32,
}

fn main() {
    let w = Widget { id: 1 };
    println!("{}", w.type_name());
}
```

Output:

```text
Widget
```

The `#label` interpolates the type name as a string literal; `.into()` converts the `quote!` result to the `proc_macro::TokenStream` the signature demands.

</details>

### Exercise 2

**Difficulty:** Medium

**Objective:** Write a *function-like* procedural macro and parse a literal from its input, a different entry point from a derive.

**Instructions:** Add a `#[proc_macro]` function `stars` so that `stars!(5)` expands to the `&str` literal `"*****"`, computed at compile time. Parse the input as a `syn::LitInt`, read it with `base10_parse::<usize>()`, build the string, and emit it as a literal with `quote!`. Verify `stars!(5)` prints five stars.

```rust
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitInt};

#[proc_macro]
pub fn stars(input: TokenStream) -> TokenStream {
    // TODO: parse a LitInt, repeat "*", emit the &str literal
    todo!()
}
```

<details>
<summary>Solution</summary>

```rust
// in a `proc-macro = true` crate (deps: syn 2, quote 1, proc-macro2 1)
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitInt};

#[proc_macro]
pub fn stars(input: TokenStream) -> TokenStream {
    let n = parse_macro_input!(input as LitInt);
    let count: usize = n.base10_parse().unwrap();
    let s = "*".repeat(count);
    quote! { #s }.into()
}
```

```rust
// consumer
use star_macro::stars;

fn main() {
    const BANNER: &str = stars!(5);
    println!("banner = {BANNER}");
}
```

Output:

```text
banner = *****
```

Because the string is built during compilation, `BANNER` is a genuine `&'static str` constant — there is no runtime work. See [Function-like Macros](/14-macros/06-function-like-macros/) for more on this flavor.

</details>

### Exercise 3

**Difficulty:** Hard

**Objective:** Inspect fields *and* handle generics correctly with `split_for_impl`.

**Instructions:** Write a `#[proc_macro_derive(FieldCount)]` that adds an associated constant `FIELD_COUNT: usize` equal to the number of fields, and that works for generic structs such as `Pair<T>`. Count named, unnamed (tuple-struct), and unit shapes. Use `ast.generics.split_for_impl()` so the generated `impl` carries the type parameters. Verify `Pair::<i32>::FIELD_COUNT == 2`.

```rust
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

#[proc_macro_derive(FieldCount)]
pub fn derive_field_count(input: TokenStream) -> TokenStream {
    // TODO: count fields by shape, split generics, emit `const FIELD_COUNT`
    todo!()
}
```

<details>
<summary>Solution</summary>

```rust
// in a `proc-macro = true` crate (deps: syn 2, quote 1, proc-macro2 1)
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

#[proc_macro_derive(FieldCount)]
pub fn derive_field_count(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let count = match &ast.data {
        Data::Struct(d) => match &d.fields {
            Fields::Named(f) => f.named.len(),
            Fields::Unnamed(f) => f.unnamed.len(),
            Fields::Unit => 0,
        },
        _ => 0,
    };

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub const FIELD_COUNT: usize = #count;
        }
    }
    .into()
}
```

```rust
// consumer
use field_count_derive::FieldCount;

#[derive(FieldCount)]
struct Pair<T> {
    #[allow(dead_code)]
    first: T,
    #[allow(dead_code)]
    second: T,
}

fn main() {
    println!("Pair<i32> has {} fields", Pair::<i32>::FIELD_COUNT);
}
```

Output:

```text
Pair<i32> has 2 fields
```

`split_for_impl` produces the three fragments that expand to `impl<T> Pair<T> { ... }`. Without it, the generated `impl Pair { ... }` would fail to compile for any generic type, because `Pair` alone is not a complete type. The field count is a compile-time `usize` constant baked into the binary.

</details>
