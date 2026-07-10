---
title: "Clone-on-Write with `Cow<'_, T>`"
description: "Cow<'_, T> holds borrowed or owned data behind one type and clones only when you mutate, letting transform-if-needed functions skip allocation on the common path."
---

`Cow` ("clone on write") is a smart pointer that holds **either** borrowed **or** owned data behind a single type, and only allocates a fresh copy at the exact moment you need to mutate or keep it. It is Rust's idiomatic tool for functions that *usually* return their input unchanged but *occasionally* need to produce a modified version — letting you skip an allocation on the common path.

---

## Quick Overview

`Cow<'a, T>` (in `std::borrow`) is an enum with two variants: `Borrowed(&'a T)` and `Owned(<T as ToOwned>::Owned)`. For strings that means it wraps either a `&str` *or* a `String`; for slices, either a `&[T]` *or* a `Vec<T>`. Because it derefs to the borrowed form, you read through it like a plain reference, and you only pay for an allocation when you actually have to change the data.

For a TypeScript/JavaScript developer, the closest mental hook is a function that returns "the same string I was given, or a new one if I had to edit it." In JavaScript strings are immutable and copies are invisible, whereas `Cow` makes the borrow-versus-allocate decision explicit and free of charge on the hot path.

---

## TypeScript/JavaScript Example

In JavaScript, strings are immutable, so "modify if needed" functions either return the original reference or build a brand-new string. The allocation is implicit and you have no way to express "I borrowed the caller's data and changed nothing."

```typescript
// Normalize a URL to https, only producing a new string when we must change it.
function ensureHttps(url: string): string {
  if (url.startsWith("https://")) {
    return url; // returns the SAME reference — no copy
  }
  if (url.startsWith("http://")) {
    return "https://" + url.slice("http://".length); // brand-new string
  }
  return "https://" + url; // brand-new string
}

const already = ensureHttps("https://example.com"); // same reference back
const upgraded = ensureHttps("http://example.com"); // freshly allocated

console.log(already); // https://example.com
console.log(upgraded); // https://example.com

// In JS you cannot tell, from the outside, whether `already` is the original
// string or a copy — and you cannot prevent the engine from copying internally.
```

This pattern is everywhere: HTML escaping, input sanitizing, path normalization, config defaulting. In each case the *typical* input needs no change, but the type system gives you no way to say "borrowed, untouched" versus "owned, rebuilt."

---

## Rust Equivalent

`Cow<'_, str>` expresses exactly that distinction, and the compiler guarantees the borrow is valid:

```rust playground
use std::borrow::Cow;

fn ensure_https(url: &str) -> Cow<'_, str> {
    if url.starts_with("https://") {
        Cow::Borrowed(url) // no allocation: the input was already fine
    } else if let Some(rest) = url.strip_prefix("http://") {
        Cow::Owned(format!("https://{rest}")) // allocate only when we must change it
    } else {
        Cow::Owned(format!("https://{url}"))
    }
}

fn main() {
    let already = ensure_https("https://example.com");
    let upgraded = ensure_https("http://example.com");
    let bare = ensure_https("example.com");

    println!("{already}");
    println!("{upgraded}");
    println!("{bare}");

    // Inspect which variant we got:
    println!("already is borrowed: {}", matches!(already, Cow::Borrowed(_)));
    println!("upgraded is owned:   {}", matches!(upgraded, Cow::Owned(_)));
}
```

**Output (verified with `cargo run`):**

```text
https://example.com
https://example.com
https://example.com
already is borrowed: true
upgraded is owned:   true
```

The `already` case returns a `Cow::Borrowed` pointing straight at the caller's string slice — **zero heap allocation**. Only the `http://` and bare-host cases build a new `String`.

---

## Detailed Explanation

### The shape of `Cow`

`Cow` is a plain enum in the standard library, roughly:

```rust
// From std::borrow (simplified)
pub enum Cow<'a, B>
where
    B: ToOwned + ?Sized,
{
    Borrowed(&'a B),
    Owned(<B as ToOwned>::Owned),
}
```

The `B: ToOwned` bound is what makes the two halves line up. `ToOwned` is the trait that knows how to turn a borrowed value into an owned one:

| Borrowed form `B` | Owned form `<B as ToOwned>::Owned` |
| ----------------- | ---------------------------------- |
| `str`             | `String`                           |
| `[T]`             | `Vec<T>`                           |
| `Path`            | `PathBuf`                          |
| `CStr`            | `CString`                          |
| any `T: Clone`    | `T`                                |

So `Cow<'a, str>` is "either a `&'a str` or a `String`," and `Cow<'a, [i32]>` is "either a `&'a [i32]` or a `Vec<i32>`." (The lifetime `'a` only constrains the `Borrowed` variant; an `Owned` value has no borrow to track.)

### Reading through `Cow` (`Deref`)

`Cow<'_, B>` implements `Deref<Target = B>`, so every read-only method of the borrowed type is available directly — you do not match on the variant just to call `.len()`:

```rust playground
use std::borrow::Cow;

fn main() {
    let c: Cow<'_, str> = Cow::Borrowed("Hello, Cow");
    // All of these are &str methods, reached through Deref:
    println!("{}", c.len());
    println!("{}", c.to_uppercase());
    println!("{}", c.starts_with("Hello"));

    // Cow<'_, str> can be compared to &str directly:
    println!("{}", c == "Hello, Cow");

    // From conversions:
    let from_str: Cow<'_, str> = "literal".into();                // -> Borrowed
    let from_string: Cow<'_, str> = String::from("owned").into(); // -> Owned
    println!(
        "{} {}",
        matches!(from_str, Cow::Borrowed(_)),
        matches!(from_string, Cow::Owned(_))
    );
}
```

**Output (verified):**

```text
10
HELLO, COW
true
true
true true
```

The `Deref` coercion here is the same machinery that makes `&String` work where `&str` is expected — covered in depth in [The `Deref` Trait and Deref Coercion](/10-smart-pointers/06-deref-trait/). Notice also the `From` impls: a `&str` becomes `Cow::Borrowed`, a `String` becomes `Cow::Owned`, so `.into()` does the right thing.

### Writing through `Cow` (`to_mut` and `into_owned`)

The "clone on write" name comes from two methods:

- **`to_mut(&mut self) -> &mut <B as ToOwned>::Owned`** — gives you a mutable handle to the owned form. If the `Cow` is currently `Borrowed`, it clones the data into an owned value *first* (that is the "write" that triggers the "clone"), switches the variant to `Owned`, and hands you `&mut` to it. If it was already `Owned`, no clone happens.
- **`into_owned(self) -> <B as ToOwned>::Owned`** — unconditionally consumes the `Cow` and returns the owned value, cloning only if it was borrowed.

```rust playground
use std::borrow::Cow;

fn main() {
    // to_mut(): get a &mut to the owned data, cloning lazily on first call.
    let mut data: Cow<'_, str> = Cow::Borrowed("hello");
    println!("before: borrowed = {}", matches!(data, Cow::Borrowed(_)));
    data.to_mut().push_str(", world"); // clones "hello" into a String here
    println!("after:  borrowed = {}", matches!(data, Cow::Borrowed(_)));
    println!("value:  {data}");

    // into_owned(): unconditionally produce the owned type.
    let s: Cow<'_, str> = Cow::Borrowed("owned me");
    let owned: String = s.into_owned();
    println!("owned:  {owned}");
}
```

**Output (verified):**

```text
before: borrowed = true
after:  borrowed = false
value:  hello, world
owned:  owned me
```

The `push_str` call works because `to_mut()` returns `&mut String`, and `String` *does* have `push_str`. Before that call, `data` was a `Cow::Borrowed`; after it, it is `Cow::Owned` — the clone happened exactly once, on first mutation.

### Why this matters: the allocation you avoid

Imagine a sanitizer applied to every field of every request. If 99% of inputs are already clean, eagerly calling `.to_string()` allocates a `String` for every single one. `Cow` lets the clean path stay a borrow:

```rust playground
use std::borrow::Cow;

// Replace any control chars; only allocate if there is something to replace.
fn sanitize(input: &str) -> Cow<'_, str> {
    if input.chars().any(|c| c.is_control()) {
        // We must build a new String -> Owned variant.
        let cleaned: String = input.chars().filter(|c| !c.is_control()).collect();
        Cow::Owned(cleaned)
    } else {
        Cow::Borrowed(input) // hot path: clean input, zero allocation
    }
}

fn main() {
    let clean = sanitize("normal text");
    let dirty = sanitize("bad\u{7}text"); // contains a BEL control char

    println!("clean borrowed: {}", matches!(clean, Cow::Borrowed(_)));
    println!("dirty owned:    {}", matches!(dirty, Cow::Owned(_)));
    println!("dirty value:    {dirty:?}");
}
```

**Output (verified):**

```text
clean borrowed: true
dirty owned:    true
dirty value:    "badtext"
```

> **Tip:** The standard library uses this pattern in its own API. For example, `str::from_utf8_lossy` returns `Cow<'_, str>`: if the bytes are already valid UTF-8 it borrows them unchanged, and only allocates when it has to substitute the replacement character `�`.

---

## Key Differences

### `Cow` is not just for strings

`Cow<'_, [T]>` works for any slice whose element is `Clone`. The decision and the deref behavior are identical:

```rust playground
use std::borrow::Cow;

// Cow works for any [T] where T: Clone, not just str.
fn drop_negatives(nums: &[i32]) -> Cow<'_, [i32]> {
    if nums.iter().any(|&n| n < 0) {
        Cow::Owned(nums.iter().copied().filter(|&n| n >= 0).collect())
    } else {
        Cow::Borrowed(nums)
    }
}

fn main() {
    let already = drop_negatives(&[1, 2, 3]);
    let filtered = drop_negatives(&[1, -2, 3]);

    println!("already borrowed: {}", matches!(already, Cow::Borrowed(_)));
    println!("filtered owned:   {}", matches!(filtered, Cow::Owned(_)));
    println!("filtered: {filtered:?}");
    println!("len via deref: {}", filtered.len()); // read through Deref to &[i32]
}
```

**Output (verified):**

```text
already borrowed: true
filtered owned:   true
filtered: [1, 3]
len via deref: 2
```

### `Cow` versus the other smart pointers

| Need | Reach for | Why not `Cow` |
| ---- | --------- | ------------- |
| Heap-allocate one value, always owned | [`Box<T>`](/10-smart-pointers/00-box/) | `Cow` is about *avoiding* the allocation when possible |
| Share ownership across many holders | [`Rc<T>`/`Arc<T>`](/10-smart-pointers/01-rc-arc/) | `Cow` has a single logical owner; it does not refcount |
| Mutate shared data behind a shared reference | [`RefCell`/`Mutex`](/10-smart-pointers/02-refcell-mutex/), [`Cell`](/10-smart-pointers/03-cell/) | `Cow` mutation always produces a *private* owned copy |
| "Maybe borrow, maybe own, decide at runtime, allocate lazily" | **`Cow<'_, T>`** | — |

`Cow` is the only one of these focused on *deferring* an allocation. The full decision matrix lives in [Choosing a Smart Pointer](/10-smart-pointers/07-comparison/).

### Mental model versus TypeScript

- **JavaScript strings:** always immutable; "return original or new" is invisible to you, and the engine may copy freely. `Cow` makes the borrow/own choice a value you can inspect and a cost the compiler tracks.
- **TypeScript types are erased** at runtime; a `string` is just a `string`. Rust monomorphizes `Cow<'a, str>` into a concrete two-variant enum with a discriminant, so "is this borrowed or owned?" is a real, checkable runtime fact (`matches!(x, Cow::Borrowed(_))`).
- Unlike JavaScript's structural typing, the lifetime `'a` ties a `Cow::Borrowed` to the data it points at. You cannot return a `Cow` that borrows something the function is about to drop (see Pitfalls).

---

## Common Pitfalls

### Pitfall 1: borrowing a local into `Cow::Borrowed`

A natural mistake is to build an owned value, then wrap a reference *to it* in `Cow::Borrowed`. The local is dropped at the end of the function, so the borrow would dangle — Rust rejects it.

```rust
use std::borrow::Cow;

fn shout(input: &str) -> Cow<'_, str> {
    let upper = input.to_uppercase(); // local String
    Cow::Borrowed(&upper)             // does not compile (error[E0515])
}
```

Real compiler output (`cargo build`):

```text
error[E0515]: cannot return value referencing local variable `upper`
 --> src/main.rs:5:5
  |
5 |     Cow::Borrowed(&upper)             // does not compile (error[E0515])
  |     ^^^^^^^^^^^^^^------^
  |     |             |
  |     |             `upper` is borrowed here
  |     returns a value referencing data owned by the current function
```

**Fix:** if you built a new value, it is *owned*, so use `Cow::Owned(upper)` (move the `String` in, no `&`). Use `Cow::Borrowed` only for data the caller owns (the `&str` parameter) or `'static` literals.

### Pitfall 2: trying to mutate a `Cow` directly

`Cow<'_, str>` derefs to `&str`, which is immutable, so methods like `push_str` are not in scope on the `Cow` itself.

```rust
use std::borrow::Cow;

fn main() {
    let mut data: Cow<'_, str> = Cow::Borrowed("hello");
    data.push_str("!"); // does not compile (error[E0599])
    println!("{data}");
}
```

Real compiler output (`cargo build`):

```text
error[E0599]: no method named `push_str` found for enum `Cow<'_, str>` in the current scope
 --> src/main.rs:5:10
  |
5 |     data.push_str("!"); // does not compile (error[E0599])
  |          ^^^^^^^^ method not found in `Cow<'_, str>`
```

**Fix:** go through `to_mut()` to get the underlying `&mut String`: `data.to_mut().push_str("!")`. That is the "write" that triggers the lazy clone.

### Pitfall 3: using `Cow` when you always allocate anyway

If *every* code path produces an owned value, `Cow` buys you nothing but ceremony — just return `String`. `Cow` pays off only when at least one common path can stay borrowed.

```rust playground
// Anti-pattern: both arms are Owned, so Cow adds no value.
// fn f(s: &str) -> Cow<'_, str> { Cow::Owned(s.to_uppercase()) }
// Prefer:
fn f(s: &str) -> String {
    s.to_uppercase()
}

fn main() {
    println!("{}", f("hi"));
}
```

### Pitfall 4: assuming `Cow::Owned` from a `String` is free to compare

Comparisons and most reads work via `Deref`, so they are fine — but remember that putting a `Cow` into a struct adds a lifetime parameter to that struct when the `Borrowed` variant can occur. That can ripple through your API (see Best Practices).

---

## Best Practices

- **Return `Cow<'a, str>` (or `Cow<'a, [T]>`) from "transform-if-needed" functions:** escapers, sanitizers, normalizers, defaulters. This is the canonical, idiomatic use.
- **Prefer `strip_prefix`/`trim`/slicing in the borrowed arm.** These return sub-slices of the input, which stay `Cow::Borrowed` with no allocation. Reach for `format!`/`replace`/`collect` only in the owned arm.
- **Use `into_owned()` at the boundary where you need to store the value past the borrow's lifetime** (e.g. before putting it in a long-lived struct without a lifetime parameter, or sending it across threads). `into_owned` is cheaper than `to_string()` when the value is already `Owned`.
- **Let `.into()` pick the variant.** `let c: Cow<str> = some_string.into();` becomes `Owned`; `let c: Cow<str> = "x".into();` becomes `Borrowed`. This reads cleanly when the source type already makes the choice obvious.
- **Be deliberate about adding a lifetime to a struct.** A field of type `Cow<'a, str>` forces `struct Config<'a>`. If most instances are built at runtime and stored long-term, a plain `String` field may be simpler; use `Cow` in the field when you genuinely want to hold borrowed literals without copying.
- **Run Clippy.** It flags some redundant allocations and can suggest `Cow` patterns; the examples in this file are Clippy-clean on stable 1.96.0.

> **Note:** `Cow` deliberately favors *correctness and zero-copy on the common path* over micro-optimizing the rare path. The owned arm still allocates exactly as a hand-written `String` would; you have simply moved that cost off the hot path.

---

## Real-World Example

A small templating layer that HTML-escapes interpolated values. The vast majority of values are plain text needing no escaping, so we keep those borrowed and only allocate for values that contain `<`, `>`, `&`, or `"`.

```rust playground
use std::borrow::Cow;
use std::collections::HashMap;

/// Escape the HTML-significant characters in `input`.
/// Returns a borrow when nothing needs escaping (the common case for plain text),
/// and only allocates a new String when an unsafe character is present.
fn escape_html(input: &str) -> Cow<'_, str> {
    // Fast scan: is there anything to escape at all?
    if !input.bytes().any(|b| matches!(b, b'<' | b'>' | b'&' | b'"')) {
        return Cow::Borrowed(input);
    }

    let mut escaped = String::with_capacity(input.len() + 16);
    for ch in input.chars() {
        match ch {
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            other => escaped.push(other),
        }
    }
    Cow::Owned(escaped)
}

/// Render a tiny template row, escaping each interpolated value.
fn render_row(fields: &HashMap<&str, &str>) -> String {
    let name = escape_html(fields.get("name").copied().unwrap_or(""));
    let bio = escape_html(fields.get("bio").copied().unwrap_or(""));
    // `name` / `bio` are Cow<str>; they Deref to &str inside format!.
    format!("<tr><td>{name}</td><td>{bio}</td></tr>")
}

fn main() {
    let mut row = HashMap::new();
    row.insert("name", "Ada Lovelace"); // safe -> borrowed, no allocation
    row.insert("bio", "Wrote the first <algorithm> & more"); // unsafe -> owned
    println!("{}", render_row(&row));

    // Confirm the allocation decision:
    let safe = escape_html("plain");
    let unsafe_ = escape_html("a & b");
    println!("safe borrowed:  {}", matches!(safe, Cow::Borrowed(_)));
    println!("unsafe owned:   {}", matches!(unsafe_, Cow::Owned(_)));
}
```

**Output (verified with `cargo run`, and `cargo clippy` clean):**

```text
<tr><td>Ada Lovelace</td><td>Wrote the first &lt;algorithm&gt; &amp; more</td></tr>
safe borrowed:  true
unsafe owned:   true
```

In a request handler processing thousands of fields per second, the `Cow::Borrowed` fast path means the overwhelmingly common "no escaping needed" case touches the heap zero times — exactly the kind of needless-allocation savings `Cow` exists to provide.

> **Tip:** Holding the result as a `Cow` also lets the *caller* decide whether to keep it borrowed (cheap) or call `.into_owned()` to detach it. You preserve the choice instead of forcing an allocation on everyone.

---

## Further Reading

### Official documentation

- [`std::borrow::Cow`](https://doc.rust-lang.org/std/borrow/enum.Cow.html): the type, its variants, and methods (`to_mut`, `into_owned`)
- [`std::borrow::ToOwned`](https://doc.rust-lang.org/std/borrow/trait.ToOwned.html): the trait that links borrowed and owned forms
- [`str::from_utf8_lossy`](https://doc.rust-lang.org/std/str/fn.from_utf8_lossy.html): a real standard-library API returning `Cow<'_, str>`
- [Rust by Example — `Box`, stack and heap](https://doc.rust-lang.org/rust-by-example/std/box.html): background on heap allocation

### Related sections in this guide

- [Section 10 overview](/10-smart-pointers/): the full map of smart pointers
- [Box&lt;T&gt;](/10-smart-pointers/00-box/): `Box<T>` when you always want a single owned heap allocation
- [Shared Ownership with `Rc<T>` and `Arc<T>`](/10-smart-pointers/01-rc-arc/): `Rc`/`Arc` for *shared* ownership (the opposite axis from `Cow`)
- [Interior Mutability](/10-smart-pointers/02-refcell-mutex/) and [Cell&lt;T&gt;](/10-smart-pointers/03-cell/): interior mutability of shared data
- [The `Deref` Trait and Deref Coercion](/10-smart-pointers/06-deref-trait/): the `Deref` coercion that makes reading through a `Cow` feel like a plain reference
- [Choosing a Smart Pointer](/10-smart-pointers/07-comparison/) — decision guide: which smart pointer for which need
- [Section 05: Ownership](/05-ownership/): borrowing, lifetimes, and why `Cow::Borrowed` carries a `'a`
- [Section 02: Basic Types](/02-basics/01-types/): `str` versus `String`, the borrowed/owned pair `Cow` is built on
- [Section 11: Async](/11-async/): when crossing thread/`.await` boundaries, prefer `into_owned()` to detach a borrow

---

## Exercises

### Exercise 1: Trim only when needed

**Difficulty:** Beginner

**Objective:** Write a function that trims surrounding whitespace but stays `Cow::Borrowed` even when it does trim — proving that sub-slicing does not allocate.

**Instructions:** Implement `trim_if_needed(input: &str) -> Cow<'_, str>`. If the trimmed string equals the input length, return the input untouched; otherwise return the trimmed sub-slice. Both arms should be `Cow::Borrowed`. Verify with inputs `"clean"` and `"  spaced  "`.

```rust
use std::borrow::Cow;

fn trim_if_needed(input: &str) -> Cow<'_, str> {
    // TODO: return Cow::Borrowed in both cases (hint: trim() returns a sub-slice)
    todo!()
}

fn main() {
    let a = trim_if_needed("clean");
    let b = trim_if_needed("  spaced  ");
    println!("{a:?} borrowed={}", matches!(a, Cow::Borrowed(_)));
    println!("{b:?} borrowed={}", matches!(b, Cow::Borrowed(_)));
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::borrow::Cow;

fn trim_if_needed(input: &str) -> Cow<'_, str> {
    let trimmed = input.trim();
    if trimmed.len() == input.len() {
        Cow::Borrowed(input)
    } else {
        Cow::Borrowed(trimmed) // still a borrow! trim returns a sub-slice of input
    }
}

fn main() {
    let a = trim_if_needed("clean");
    let b = trim_if_needed("  spaced  ");
    println!("{a:?} borrowed={}", matches!(a, Cow::Borrowed(_)));
    println!("{b:?} borrowed={}", matches!(b, Cow::Borrowed(_)));
}
```

**Output:**

```text
"clean" borrowed=true
"spaced" borrowed=true
```

The key insight: `trim()` returns a slice *into* the original string, so no allocation occurs even when characters are removed. `Cow` carries the same lifetime, so the borrow stays valid.

</details>

---

### Exercise 2: Normalize line endings

**Difficulty:** Intermediate

**Objective:** Avoid allocating for text that is already in Unix line-ending form.

**Instructions:** Implement `normalize_newlines(text: &str) -> Cow<'_, str>` that converts `\r\n` and lone `\r` to `\n`. If the text contains no `\r`, return it borrowed; otherwise build the normalized `String` and return it owned. Verify with `"a\nb\nc"` (borrowed) and `"a\r\nb\r\nc"` (owned).

```rust
use std::borrow::Cow;

fn normalize_newlines(text: &str) -> Cow<'_, str> {
    // TODO: borrow if there is no '\r'; otherwise replace and own
    todo!()
}

fn main() {
    let unix = normalize_newlines("a\nb\nc");
    let win = normalize_newlines("a\r\nb\r\nc");
    println!("unix borrowed: {}", matches!(unix, Cow::Borrowed(_)));
    println!("win owned:     {}", matches!(win, Cow::Owned(_)));
    println!("win value:     {win:?}");
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::borrow::Cow;

fn normalize_newlines(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        // Replace CRLF first, then any remaining lone CR.
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

fn main() {
    let unix = normalize_newlines("a\nb\nc");
    let win = normalize_newlines("a\r\nb\r\nc");
    println!("unix borrowed: {}", matches!(unix, Cow::Borrowed(_)));
    println!("win owned:     {}", matches!(win, Cow::Owned(_)));
    println!("win value:     {win:?}");
}
```

**Output:**

```text
unix borrowed: true
win owned:     true
win value:     "a\nb\nc"
```

`str::replace` always allocates a `String`, which is why we only call it inside the owned arm — files that are already LF-only never touch the heap.

</details>

---

### Exercise 3: Store borrowed-or-owned in a struct

**Difficulty:** Advanced

**Objective:** Use `Cow` as a struct field so the same type can hold a `'static` literal *without copying* or a runtime-generated `String`, and write a function returning `Cow<'a, str>` that conditionally appends a default file extension.

**Instructions:** (a) Define `struct Config<'a> { name: Cow<'a, str> }` with constructors `from_static(name: &'a str)` (borrowed) and `generated(n: u32)` (owned, e.g. `format!("node-{n}")`). (b) Implement `with_extension<'a>(path: &'a str, ext: &str) -> Cow<'a, str>` that returns the path borrowed if it already has an extension, otherwise appends `.{ext}` and owns it. Use `std::path::Path::new(path).extension()` to detect an existing extension.

```rust
use std::borrow::Cow;

struct Config<'a> {
    name: Cow<'a, str>,
}

impl<'a> Config<'a> {
    fn from_static(name: &'a str) -> Self {
        todo!()
    }
    fn generated(n: u32) -> Self {
        todo!()
    }
}

fn with_extension<'a>(path: &'a str, ext: &str) -> Cow<'a, str> {
    todo!()
}

fn main() {
    let a = Config::from_static("default");
    let b = Config::generated(7);
    println!("a.name = {}, len {}", a.name, a.name.len());
    println!("b.name = {}", b.name);

    let p1 = with_extension("report.pdf", "txt");
    let p2 = with_extension("report", "txt");
    println!("{p1} borrowed={}", matches!(p1, Cow::Borrowed(_)));
    println!("{p2} owned={}", matches!(p2, Cow::Owned(_)));
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::borrow::Cow;

#[derive(Debug)]
struct Config<'a> {
    name: Cow<'a, str>,
}

impl<'a> Config<'a> {
    // Zero-copy when the caller passes a literal (or any &'a str).
    fn from_static(name: &'a str) -> Self {
        Config { name: Cow::Borrowed(name) }
    }
    // Owned when built at runtime.
    fn generated(n: u32) -> Self {
        Config { name: Cow::Owned(format!("node-{n}")) }
    }
}

fn with_extension<'a>(path: &'a str, ext: &str) -> Cow<'a, str> {
    if std::path::Path::new(path).extension().is_some() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(format!("{path}.{ext}"))
    }
}

fn main() {
    let a = Config::from_static("default");
    let b = Config::generated(7);
    // .name derefs to &str
    println!("a.name = {}, len {}", a.name, a.name.len());
    println!("b.name = {}", b.name);

    let p1 = with_extension("report.pdf", "txt");
    let p2 = with_extension("report", "txt");
    println!("{p1} borrowed={}", matches!(p1, Cow::Borrowed(_)));
    println!("{p2} owned={}", matches!(p2, Cow::Owned(_)));
}
```

**Output:**

```text
a.name = default, len 7
b.name = node-7
report.pdf borrowed=true
report.txt owned=true
```

Adding `Cow<'a, str>` as a field forces the lifetime parameter `'a` onto `Config`. That is the price of holding borrowed data, and the reward is that `from_static("default")` stores the literal with no heap allocation, while `generated` still owns its runtime string in the very same type.

</details>
