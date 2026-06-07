---
title: "Specialization"
description: "Rust specialization lets a specific impl override a blanket one, but it is still nightly-only. The stable patterns that replace it, vs TypeScript overloads."
---

**Specialization** is a long-promised Rust feature that would let a *more specific* trait implementation override a *more general* one. For example, a blanket `impl<T> Trait for T` that a dedicated `impl Trait for String` is allowed to refine. It is genuinely useful, partially implemented, and **still not stable** after a decade of design work. This page explains what it would buy you, exactly why it is stuck on nightly, and — most importantly — the safe, stable patterns that cover the vast majority of cases today.

---

## Quick Overview

Today, Rust's **coherence** rules forbid two trait implementations from overlapping: you cannot have both `impl<T> Summary for T` and `impl Summary for String`, because `String` matches both and the compiler refuses to guess which one you meant. Specialization would lift that restriction in a controlled way: the *most specific* applicable impl wins, and a base impl marks the methods that may be overridden with the `default` keyword.

For a TypeScript/JavaScript developer the closest mental model is **function overloads** (`function f(x: string): ...; function f(x: number): ...`) or a chain of `typeof`/`instanceof` checks: one logical operation with several behaviors chosen by the argument's concrete type. The important difference is *when* the choice happens. TypeScript overloads are erased: at runtime there is a single function body that hand-dispatches on `typeof`. Rust's specialization would resolve at **compile time** through monomorphization, producing separate, fully-optimized machine code per type with zero runtime dispatch. The catch: getting that resolution to be *sound* in the presence of lifetimes has resisted a complete solution, which is why the feature is still gated.

> **Note:** Everything in the "Rust Equivalent," "Best Practices," and "Real-World Example" sections compiles on **stable Rust 1.96.0 (2024 edition)**. The genuine `specialization` feature only appears in the "What Specialization Would Enable" subsection and the pitfalls, and is clearly marked as nightly-only.

---

## TypeScript/JavaScript Example

TypeScript lets you give one function several **overload signatures** and write a single implementation that dispatches on the runtime type. This is the everyday tool a TS developer reaches for when one operation should behave differently for different concrete types:

```typescript
// One name, several specialized signatures, one hand-written dispatcher.
function describe(value: string): string;
function describe(value: number): string;
function describe(value: unknown[]): string;
function describe(value: unknown): string {
  // The "specialization" is a manual runtime type test.
  if (typeof value === "string") {
    return `a string of length ${value.length}`;
  }
  if (typeof value === "number") {
    return `the number ${value} (even? ${value % 2 === 0})`;
  }
  if (Array.isArray(value)) {
    return `an array of ${value.length} items`;
  }
  return `some value: ${String(value)}`;
}

console.log(describe("hello"));
console.log(describe(42));
console.log(describe([1, 2, 3]));
```

Running it with Node v22 prints:

```text
a string of length 5
the number 42 (even? true)
an array of 3 items
```

Two things matter here. First, the overload *signatures* are pure compile-time annotations: `tsc` erases them, and at runtime there is exactly one function whose body inspects `value`. Second, that inspection is the only dispatch mechanism JavaScript has: a runtime `typeof`/`Array.isArray` chain. There is no way to ask the engine "compile a separate, specialized version of this function for `string`." Rust's specialization aims to provide exactly that, but at compile time, and that is precisely what makes it hard.

---

## Rust Equivalent

On stable Rust you cannot write overlapping impls, so the idiomatic equivalent of the overloaded `describe` is a **trait with a default method that specific types override**. The blanket behavior lives in the trait's default body; any type that wants something more specific provides its own `impl`. Because the consumer is generic, dispatch is resolved at compile time with zero runtime cost:

```rust
use std::fmt::Debug;

// A metrics sink. Every metric type gets a sensible *default* encoding; types
// that care can override it with a compact, allocation-light one. This is the
// trait-default-override pattern -- the stable workhorse approximation of
// specialization.
trait Metric: Debug {
    // Default: a verbose, Debug-based line that works for any metric type.
    fn encode(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Debug)]
struct Counter {
    name: &'static str,
    value: u64,
}

#[derive(Debug)]
struct Gauge {
    name: &'static str,
    value: f64,
}

// Counter provides a *more specific* implementation, overriding the default.
impl Metric for Counter {
    fn encode(&self) -> String {
        format!("{}_total {}", self.name, self.value)
    }
}

// Gauge does NOT override encode(): it falls back to the Debug-based default.
impl Metric for Gauge {}

// Generic consumer: monomorphized, so each call dispatches statically at compile
// time with zero runtime cost -- exactly what real specialization promises.
fn emit<M: Metric>(m: &M) {
    println!("{}", m.encode());
}

fn main() {
    emit(&Counter { name: "requests", value: 99 });
    emit(&Gauge { name: "cpu", value: 0.7 });
}
```

Output:

```text
requests_total 99
Gauge { name: "cpu", value: 0.7 }
```

`Counter` uses its specialized `encode`; `Gauge` silently inherits the generic default. This is not the *same* as true specialization (you cannot have a blanket `impl<T> Metric for T` *and* override it for `Counter`), but for the common "general behavior plus per-type refinements" shape, it is the correct, stable answer.

---

## Detailed Explanation

Let us connect the TypeScript and Rust versions line by line, then explain why the obvious translation does not compile.

**What the TypeScript does.** `describe`'s three overload signatures tell `tsc` which argument types are legal and what each returns. The single implementation body is the only thing that exists at runtime, and it dispatches with `typeof`/`Array.isArray`. The "specialization" is therefore a *runtime* branch: JavaScript has no concept of compiling a separate `describe_for_string`.

**What the stable Rust does.** The `Metric` trait declares `encode` with a *default body*. `impl Metric for Counter { fn encode ... }` overrides that body; `impl Metric for Gauge {}` accepts it. When you call `emit(&counter)`, the compiler **monomorphizes** `emit` into a version specialized to `Counter` and statically resolves `m.encode()` to `Counter::encode`. There is no runtime type tag, no branch; the right code is baked in at compile time. (Monomorphization and static dispatch are covered in [Generic Functions](/09-generics-traits/00-generic-functions/) and [Default implementations](/09-generics-traits/08-default-impls/).)

**Why the "natural" translation fails.** The instinct is to write a blanket impl and then refine it:

```rust
use std::fmt::Display;

trait Summary {
    fn summarize(&self) -> String;
}

// A blanket impl for everything that is Display.
impl<T: Display> Summary for T {
    fn summarize(&self) -> String {
        format!("{self}")
    }
}

// A "more specific" impl for String -- but String IS Display, so they overlap.
// does not compile (error[E0119]: conflicting implementations)
impl Summary for String {
    fn summarize(&self) -> String {
        format!("string: {self}")
    }
}

fn main() {}
```

The compiler rejects this with the real error:

```text
error[E0119]: conflicting implementations of trait `Summary` for type `String`
  --> src/main.rs:15:1
   |
 8 | impl<T: Display> Summary for T {
   | ------------------------------ first implementation here
...
15 | impl Summary for String {
   | ^^^^^^^^^^^^^^^^^^^^^^^ conflicting implementation for `String`

For more information about this error, try `rustc --explain E0119`.
```

This is the entire problem in one message. `String` satisfies both impls, so the compiler has two candidate `summarize` methods and no rule to pick between them. Coherence demands that *at most one* impl applies to any given type, so it refuses the program. **Specialization is the feature that would teach the compiler the missing rule: "when two impls overlap, prefer the more specific one."**

### What specialization would enable (nightly only)

On a **nightly** compiler you can turn the feature on and write exactly the overlapping impls that stable forbids. The base impl marks overridable methods with `default`, and the specific impl wins:

```rust
// requires nightly: #![feature(specialization)] is rejected on stable.
#![feature(specialization)]
#![allow(incomplete_features)]

trait Summary {
    fn summarize(&self) -> String;
}

// Blanket impl. `default` says "a more specific impl may override this".
impl<T: std::fmt::Debug> Summary for T {
    default fn summarize(&self) -> String {
        format!("generic: {self:?}")
    }
}

// The more specific impl for String "wins" over the blanket one.
impl Summary for String {
    fn summarize(&self) -> String {
        format!("a string of length {}", self.len())
    }
}

fn main() {
    println!("{}", 42i32.summarize());
    println!("{}", vec![1, 2].summarize());
    println!("{}", String::from("hello").summarize());
}
```

Compiled with `rustc 1.98.0-nightly` (using `rustup run nightly rustc spec.rs`), this builds and runs:

```text
generic: 42
generic: [1, 2]
a string of length 5
```

The `i32` and `Vec` fall through to the blanket impl; `String` uses its specialized one, *all resolved statically*, with no runtime type check. That is the prize. The reason you cannot rely on it is in the next section.

> **Warning:** The full `specialization` feature is flagged `incomplete_features` for a reason: it can accept *unsound* programs and has been known to ICE (internal compiler error). Do not build production code on it. The narrower `min_specialization` gate is safer but still nightly-only and still incomplete.

---

## Key Differences

| Aspect | TypeScript overloads / `typeof` dispatch | Rust specialization (hypothetical, nightly) | Stable Rust today |
| --- | --- | --- | --- |
| When dispatch happens | Runtime (`typeof`, `instanceof`) | Compile time (monomorphization) | Compile time (generics + default methods) |
| Overlapping definitions allowed? | Yes — overloads are erased to one body | Yes — most specific impl wins | **No** — coherence forbids overlap (E0119) |
| Cost | A runtime branch per call | Zero — separate compiled code per type | Zero — separate compiled code per type |
| Stability | Stable TS feature | **Unstable**, gated behind nightly flags | Fully stable |
| Soundness concern | None (dynamically typed anyway) | Lifetime-dependent specialization can be unsound | N/A — the patterns are sound by construction |

The headline conceptual difference from TypeScript: in TS, "which version runs" is a *value-level* question answered at runtime, so it can never be unsound — every path is just JavaScript. In Rust, "which impl applies" is a *type-level* question the compiler must answer while still guaranteeing memory safety. The hard part is that Rust **erases lifetimes** before generating code, yet specialization wants to choose impls partly based on type identity. If an impl could be selected based on a lifetime (say a special case for `&'static T` versus `&'a T`), the choice would depend on information that no longer exists at codegen time. That mismatch is the root of the unsoundness, and resolving it cleanly is what has kept the feature off stable for years.

---

## Common Pitfalls

### Pitfall 1: Assuming the `specialization` feature is stable or "almost ready"

It is neither. Turning it on requires a nightly toolchain, and even there the compiler shouts that it is unsafe to rely on:

```rust
// does not compile on stable (error[E0554])
#![feature(specialization)]
// ... trait + impls ...
fn main() {}
```

On stable `rustc 1.96.0`, `cargo run` produces:

```text
error[E0554]: `#![feature]` may not be used on the stable release channel
 --> src/main.rs:1:1
  |
1 | #![feature(specialization)]
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: the feature `specialization` is incomplete and may not be safe to use and/or cause compiler crashes
 --> src/main.rs:1:12
  |
1 | #![feature(specialization)]
  |            ^^^^^^^^^^^^^^
  |
  = note: see issue #31844 <https://github.com/rust-lang/rust/issues/31844> for more information
  = help: consider using `min_specialization` instead, which is more stable and complete
  = note: `#[warn(incomplete_features)]` on by default
```

Tracking issue [#31844](https://github.com/rust-lang/rust/issues/31844) has been open since 2016. Treat specialization as a research feature, not a roadmap item you can schedule against.

### Pitfall 2: Writing overlapping impls on stable and expecting "most specific wins"

This is the E0119 error shown earlier. Newcomers from C++ (where partial template specialization exists) or from dynamic languages (where you just branch at runtime) often write a blanket impl plus a refinement and are surprised it is rejected. The fix is one of the stable patterns in [Best Practices](#best-practices): there is no flag you can flip on stable to make overlap legal.

### Pitfall 3: Forgetting `default` even on nightly with `min_specialization`

`min_specialization` is the conservative subset the Rust team hopes to stabilize first. It still requires the *base* impl to mark a method `default` before any specific impl may override it. Omitting it is an error, even on nightly:

```rust
// requires nightly AND still fails: base method must be `default`
#![feature(min_specialization)]

trait Fast {
    fn run(&self) -> &'static str;
}

impl<T> Fast for T {
    // No `default` here -- so this method is final and cannot be specialized.
    fn run(&self) -> &'static str {
        "generic"
    }
}

impl Fast for u8 {
    fn run(&self) -> &'static str {
        "u8"
    }
}

fn main() {}
```

`rustup run nightly rustc` reports:

```text
error[E0520]: `run` specializes an item from a parent `impl`, but that item is not marked `default`
  --> nodefault.rs:15:5
   |
 7 | impl<T> Fast for T {
   | ------------------ parent `impl` is here
...
15 |     fn run(&self) -> &'static str {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ cannot specialize default item `run`
   |
   = note: to specialize, `run` in the parent `impl` must be marked `default`

error: aborting due to 1 previous error
```

Adding `default fn run` to the blanket impl makes both versions of the program (`specialization` and `min_specialization`) compile and run, printing `generic` then `u8`.

### Pitfall 4: Reaching for the autoref trick when a simpler tool exists

There is a clever stable technique (next section) that *approximates* compile-time specialization using method-resolution priority. It works, but it is subtle, fragile under refactoring, and confusing to readers who do not know it. Use it only when you truly need compile-time selection between "implements trait X" and "implements trait Y," and document it. For most code, a default method, an enum, or explicit methods are clearer.

---

## Best Practices

When you feel the pull toward specialization, pick the **simplest stable pattern that fits the shape of your problem**. In rough order of preference:

### 1. Trait with default methods (general behavior + refinements)

This is the pattern in the [Rust Equivalent](#rust-equivalent) above: put the general behavior in a trait's default method and let specific types override it. It covers the *vast* majority of "I want a fallback plus special cases" needs and is completely idiomatic. See [Default implementations](/09-generics-traits/08-default-impls/).

### 2. An enum, when the set of types is closed

If you know *all* the types up front, do not abstract over them at all; enumerate them. An `enum` plus an exhaustive `match` is faster, clearer, and impossible to get wrong than any specialization scheme:

```rust
// When the set of "specialized" cases is closed, an enum + match beats any
// specialization machinery: one type, exhaustive dispatch, no overlap problem.
enum Value {
    Text(String),
    Int(i64),
    List(Vec<i64>),
}

impl Value {
    fn render(&self) -> String {
        match self {
            Value::Text(s) => format!("text({}): {s:?}", s.len()),
            Value::Int(n) => format!("int: {n} (even? {})", n % 2 == 0),
            Value::List(xs) => format!("list of {}", xs.len()),
        }
    }
}

fn main() {
    for v in [
        Value::Text("hi".into()),
        Value::Int(42),
        Value::List(vec![1, 2, 3]),
    ] {
        println!("{}", v.render());
    }
}
```

Output:

```text
text(2): "hi"
int: 42 (even? true)
list of 3
```

### 3. Runtime dispatch with `Any` / downcasting, when the special case is rare

If you have a truly generic `fn f<T: ...>(x: &T)` and want a fast path for one or two concrete types, you can ask at runtime via [`std::any::Any`](https://doc.rust-lang.org/std/any/trait.Any.html). This trades a tiny runtime check for not needing nightly:

```rust
use std::any::Any;
use std::fmt::Debug;

// A generic function that does something *special* for String / i32 and the
// generic thing otherwise. The "specialization" is a runtime type check.
fn describe<T: Debug + Any>(value: &T) -> String {
    let any = value as &dyn Any;
    if let Some(s) = any.downcast_ref::<String>() {
        format!("a string of length {}: {s:?}", s.len())
    } else if let Some(n) = any.downcast_ref::<i32>() {
        format!("the integer {n} (even? {})", n % 2 == 0)
    } else {
        format!("some value: {value:?}")
    }
}

fn main() {
    println!("{}", describe(&String::from("hi")));
    println!("{}", describe(&42i32));
    println!("{}", describe(&vec![1, 2, 3]));
}
```

Output:

```text
a string of length 2: "hi"
the integer 42 (even? true)
some value: [1, 2, 3]
```

> **Tip:** `Any` only works for `'static` types (no borrowed lifetimes), and the check is a runtime comparison, so do not put it on a hot inner loop where you expected zero-cost dispatch. Use it for occasional fast paths, not pervasive ones.

### 4. The autoref (method-resolution) trick, for compile-time selection on trait bounds

When you genuinely need the compiler to choose at compile time between "T implements Display" and "T only implements Debug," you can exploit how method resolution prefers fewer autorefs. Two traits share a method name; one is implemented on `T` (fewer autorefs, tried first), the other on `&T`. The compiler reaches the `&T` impl only when the `T` impl does not apply:

```rust
use std::fmt::{Debug, Display};

// More specific: requires Display. Implemented on `T`, so it is tried first.
trait ViaDisplay {
    fn render(&self) -> String;
}
impl<T: Display> ViaDisplay for T {
    fn render(&self) -> String {
        format!("via Display: {self}")
    }
}

// Less specific: works for any Debug, but implemented on `&T`. Because it needs
// one extra autoref, the compiler only reaches it when the Display impl above
// does not apply.
trait ViaDebug {
    fn render(&self) -> String;
}
impl<T: Debug> ViaDebug for &T {
    fn render(&self) -> String {
        format!("via Debug: {self:?}")
    }
}

#[derive(Debug)]
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    // i32 implements Display -> the Display impl wins.
    let n = 7;
    println!("{}", (&n).render());

    // Point implements only Debug -> autoref reaches the &T impl.
    let p = Point { x: 1, y: 2 };
    println!("{}", (&p).render());
}
```

Output:

```text
via Display: 7
via Debug: Point { x: 1, y: 2 }
```

This compiles cleanly on stable; the `(&value).render()` call site forces method resolution to consider the autoref ladder. It is the technique behind crates like `anyhow` (selecting an error-conversion strategy) and the well-known "autoref specialization" blog posts. Reserve it for library code where the ergonomics justify the cleverness, and comment it heavily.

> **Note:** Several real `std` methods already contain *internal* specialization (compiled with nightly inside the standard library). For example, `Vec::extend` and `From<&[T]>` use a hidden `SpecExtend`/`SpecFrom` trait to pick a `memcpy` fast path for `Copy` elements. You benefit from this every day; you just cannot write it yourself on stable. This is also why "specialization is unstable" coexists with "specialization ships in your binary."

---

## Real-World Example

A common production need: a serializer that wants a **zero-extra-allocation fast path** for string slices and a **general path** for anything `Display`. With true specialization you would write one generic method and let a `&str` impl override it. On stable, the honest, zero-magic version exposes two explicit methods and lets the call site pick: clearer than the autoref trick and just as fast, because both are statically dispatched:

```rust
use std::fmt::Display;

// A tiny JSON-string encoder. We want a fast path for `&str` (push the bytes
// directly, no temporary allocation) and a general path for anything Display.
// Stable Rust cannot overlap impls, so we offer two methods; each is resolved
// statically, so there is no runtime dispatch cost.
struct Encoder {
    out: String,
}

impl Encoder {
    fn new() -> Self {
        Encoder { out: String::new() }
    }

    // General path: any Display value, rendered into a temporary then quoted.
    fn write_display<T: Display>(&mut self, value: &T) {
        self.out.push('"');
        self.out.push_str(&value.to_string()); // allocates a temporary String
        self.out.push('"');
    }

    // Specialized fast path: a &str needs no intermediate String at all.
    fn write_str(&mut self, value: &str) {
        self.out.push('"');
        self.out.push_str(value); // borrow directly, zero extra allocation
        self.out.push('"');
    }
}

fn main() {
    let mut enc = Encoder::new();
    enc.write_str("hello"); // fast path: no temporary String
    enc.write_display(&42); // general path
    enc.write_display(&3.5f64); // general path
    println!("{}", enc.out);
}
```

Output:

```text
"hello""42""3.5"
```

If, later, the set of supported value kinds becomes closed and known, you would refactor `Encoder` to accept an `enum Value` and `match` on it (Best Practice #2), eliminating even the possibility of calling the wrong method. The lesson generalizes: most desires for specialization are really desires for *one* of "a default plus overrides," "an enum," "an occasional runtime check," or "compile-time trait selection," and stable Rust has a clean, sound tool for each. The actual `specialization` feature remains a research project: worth understanding so you can read nightly-only library internals, but not something to design around.

---

## Further Reading

- [Specialization tracking issue #31844](https://github.com/rust-lang/rust/issues/31844) — the canonical record of design, blockers, and the soundness debate.
- [RFC 1210: Impl specialization](https://rust-lang.github.io/rfcs/1210-impl-specialization.html) — the original proposal and motivation.
- [Niko Matsakis: "Maximally minimal specialization"](https://smallcultfollowing.com/babysteps/blog/2018/02/09/maximally-minimal-specialization-always-applicable-impls/) — why `min_specialization` exists and what "always-applicable" impls are.
- [dtolnay's autoref-specialization gist](https://github.com/dtolnay/case-studies/blob/master/autoref-specialization/README.md) — the canonical write-up of the stable method-resolution trick.
- [`std::any::Any`](https://doc.rust-lang.org/std/any/trait.Any.html) — the runtime downcasting tool used in approximation #3.

Cross-links within this guide:

- [Default implementations](/09-generics-traits/08-default-impls/) — the trait-default-override pattern that replaces most specialization needs.
- [Trait bounds](/09-generics-traits/05-trait-bounds/), [Generic Functions](/09-generics-traits/00-generic-functions/), and [The orphan rule](/09-generics-traits/12-orphan-rule/) — the coherence machinery that makes overlapping impls illegal in the first place.
- [Trait objects](/09-generics-traits/06-trait-objects/) — `dyn Trait` runtime dispatch, an alternative when types are open and selection must be dynamic.
- Sibling advanced topics: [Generic Associated Types (GATs)](/25-advanced-topics/06-gat/) — a feature that *did* reach stable; [Const generics](/25-advanced-topics/05-const-generics/) and [PhantomData & zero-sized types](/25-advanced-topics/00-phantom-data/) — other type-system tools; [Compiler & tooling internals](/25-advanced-topics/08-compiler-plugins/) — more on what still needs nightly.
- Foundations: [Section 00: Introduction](/00-introduction/), [Section 01: Getting Started](/01-getting-started/), [Section 02: Basics](/02-basics/).
- [Section 26: Systems Programming](/26-systems-programming/) — where the `Copy`-vs-general fast paths (like `Vec::extend`'s internal specialization) matter most.

---

## Exercises

### Exercise 1: Default-method "specialization"

**Difficulty:** Beginner

**Objective:** Replace a would-be specialization with the stable trait-default-override pattern.

**Instructions:** Define a trait `Notify` with a default method `message(&self) -> String` returning `"You have a notification"`. Implement `Notify` for a `struct Email { subject: String }` so it overrides the default with `format!("New email: {}", self.subject)`, and for a `struct Heartbeat;` that accepts the default. Write a generic `fn send<N: Notify>(n: &N)` that prints `n.message()`, and call it with both types.

<details>
<summary>Solution</summary>

```rust
trait Notify {
    fn message(&self) -> String {
        String::from("You have a notification")
    }
}

struct Email {
    subject: String,
}

struct Heartbeat;

impl Notify for Email {
    fn message(&self) -> String {
        format!("New email: {}", self.subject)
    }
}

// Heartbeat takes the default message.
impl Notify for Heartbeat {}

fn send<N: Notify>(n: &N) {
    println!("{}", n.message());
}

fn main() {
    send(&Email { subject: "Invoice".into() });
    send(&Heartbeat);
}
```

Output:

```text
New email: Invoice
You have a notification
```

</details>

### Exercise 2: Runtime fast path with `Any`

**Difficulty:** Intermediate

**Objective:** Use `std::any::Any` to give one concrete type a special path inside an otherwise-generic function.

**Instructions:** Write `fn byte_len<T: std::any::Any>(value: &T) -> Option<usize>` that returns `Some(n)` with the byte length when `value` is a `String` *or* a `&'static str`, and `None` for everything else. (Hint: downcast to `String` and to `&'static str` separately.) Test it with a `String`, a `&'static str`, and an `i32`.

<details>
<summary>Solution</summary>

```rust
use std::any::Any;

fn byte_len<T: Any>(value: &T) -> Option<usize> {
    let any = value as &dyn Any;
    if let Some(s) = any.downcast_ref::<String>() {
        Some(s.len())
    } else if let Some(s) = any.downcast_ref::<&'static str>() {
        Some(s.len())
    } else {
        None
    }
}

fn main() {
    println!("{:?}", byte_len(&String::from("hello"))); // Some(5)
    println!("{:?}", byte_len(&"hi")); // Some(2)
    println!("{:?}", byte_len(&42i32)); // None
}
```

Output:

```text
Some(5)
Some(2)
None
```

> Note that `Any` requires `'static`, which is why we match `&'static str` specifically; a borrowed `&'a str` with a non-`'static` lifetime is not `Any`.

</details>

### Exercise 3: Autoref specialization

**Difficulty:** Advanced

**Objective:** Use the stable autoref method-resolution trick to pick, at compile time, between a "clone via `Clone`" path and a "describe via `Debug`" fallback.

**Instructions:** Define a wrapper `struct Wrap<T>(T)`. Give it an **inherent** method `act(&self) -> String` on `impl<T: Clone> Wrap<T>` returning `"cloned"`, and a *trait* method of the same name on `impl<T: Debug> DebugFallback for Wrap<T>` returning `format!("debug: {:?}", self.0)`. Because inherent methods are resolved before trait methods, the `Clone` path wins whenever `T: Clone`, and the trait fallback is reached otherwise. Verify that `Wrap(vec![1, 2, 3])` (whose `T` is `Clone`) takes the `"cloned"` path, while a type that is `Debug` but **not** `Clone` takes the fallback.

<details>
<summary>Solution</summary>

```rust
use std::fmt::Debug;

// We want: if T: Clone, say "cloned"; otherwise (T: Debug) say "debug: ...".
// Wrap the value so we control the method-resolution ladder precisely.
struct Wrap<T>(T);

// More specific: an *inherent* method on Wrap<T> when T: Clone. Inherent methods
// are tried before trait methods, so this wins whenever T: Clone.
impl<T: Clone> Wrap<T> {
    fn act(&self) -> String {
        String::from("cloned")
    }
}

// Fallback: a trait method, reached only when the inherent method above does not
// apply (i.e. T is not Clone).
trait DebugFallback {
    fn act(&self) -> String;
}
impl<T: Debug> DebugFallback for Wrap<T> {
    fn act(&self) -> String {
        format!("debug: {:?}", self.0)
    }
}

// NotClone derives only Debug, so it is NOT Clone.
#[derive(Debug)]
struct NotClone {
    id: u32,
}

fn main() {
    let v = Wrap(vec![1, 2, 3]); // T = Vec<i32>: Clone -> inherent method wins
    println!("{}", v.act());

    let nc = Wrap(NotClone { id: 7 }); // T: Debug, not Clone -> trait fallback
    println!("{}", nc.act());
}
```

Output:

```text
cloned
debug: NotClone { id: 7 }
```

`Vec<i32>` is `Clone`, so the inherent `act` applies and, because inherent methods outrank trait methods in resolution, it wins. `NotClone` is not `Clone`, so the inherent method does not apply and the compiler falls through to the `DebugFallback` trait method. Choosing an inherent method over a trait method by their *bounds* is the reliable form of the autoref trick; `anyhow` uses a closely related construction to select an error-conversion strategy at compile time, entirely on stable Rust.

</details>
