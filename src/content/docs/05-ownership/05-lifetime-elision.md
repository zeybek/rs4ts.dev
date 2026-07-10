---
title: "Lifetime Elision"
description: "Rust's three lifetime elision rules let you omit 'a annotations the compiler can infer. See exactly when they apply and why TypeScript never needs them."
---

In the [previous topic](/05-ownership/04-lifetimes/) you saw that references in function signatures sometimes need explicit lifetime annotations like `'a`. But you've also written (and will write) hundreds of functions that take and return references *without* a single `'a` in sight. That's not magic. It's a small, deterministic set of rules the compiler applies for you called **lifetime elision**.

---

## Quick Overview

**Lifetime elision** is a set of three rules the Rust compiler uses to *infer* lifetimes in function and method signatures so you don't have to write them by hand. When the rules fully determine every lifetime, you can omit the annotations entirely; when they don't, the compiler asks you to be explicit. Understanding the rules tells you *exactly* when annotations are required and when they're just noise: the difference between fighting the borrow checker and ignoring it.

**The point:** Elision is not inference of *behavior*; it's a fixed, predictable shorthand. The same rules run every time, and you can apply them in your head.

---

## TypeScript/JavaScript Example

TypeScript has no concept of lifetimes at all, so there's nothing to elide. The garbage collector keeps any object alive as long as *something* references it, and the type system says nothing about how long a returned reference stays valid:

```typescript
// TypeScript: a function that returns part of its input.
// The returned string and the input are completely independent
// objects as far as the type system is concerned.
function firstWord(sentence: string): string {
  const spaceIndex = sentence.indexOf(" ");
  return spaceIndex === -1 ? sentence : sentence.slice(0, spaceIndex);
}

class Headers {
  constructor(private raw: string) {}

  // Returns a substring of `this.raw`. TypeScript tracks the *type*
  // (string), but nothing about how the result relates to `this`.
  get(name: string): string | undefined {
    for (const line of this.raw.split("\n")) {
      const [key, value] = line.split(": ");
      if (key.toLowerCase() === name.toLowerCase()) return value;
    }
    return undefined;
  }
}

const headers = new Headers("Host: example.com\nAccept: */*");
console.log(headers.get("host")); // example.com
```

> **Note:** In JavaScript, `slice` returns a brand-new string, so there is no "borrowing" relationship to track. The whole problem lifetime elision solves (*which input does this returned reference borrow from?*) simply does not exist in a garbage-collected language.

---

## Rust Equivalent

Here is the same code in Rust. Notice that **none** of these signatures carry a `'a` annotation, even though they take and return references. The elision rules fill them in:

```rust playground
// One reference in, one reference out: the compiler infers the lifetime.
fn first_word(s: &str) -> &str {
    let bytes = s.as_bytes();
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == b' ' {
            return &s[..i];
        }
    }
    s
}

struct Headers<'a> {
    raw: &'a str,
}

impl<'a> Headers<'a> {
    fn new(raw: &'a str) -> Self {
        Headers { raw }
    }

    // `&self` plus another reference parameter, returning a reference:
    // elision ties the output to `&self`, so no annotation is needed.
    fn get(&self, name: &str) -> Option<&str> {
        self.raw
            .lines()
            .filter_map(|line| line.split_once(": "))
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    }
}

fn main() {
    let sentence = String::from("hello world rust");
    println!("first word: {}", first_word(&sentence));

    let headers = Headers::new("Host: example.com\nAccept: */*");
    println!("host: {:?}", headers.get("host"));
}
```

Running it:

```
first word: hello
host: Some("example.com")
```

The signatures are as clean as the TypeScript versions. That's the whole point of elision. But unlike TypeScript, the compiler has *still verified* that every returned reference is valid for exactly as long as the thing it borrows from. You get the safety guarantee without the syntactic ceremony.

---

## Detailed Explanation

### What "elision" actually means

Every reference in Rust has a lifetime; that's non-negotiable. **Elision** (from "to elide," meaning to omit) is purely about whether *you* have to write that lifetime down. When you write:

```rust
fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or("")
}
```

the compiler treats it *exactly* as if you had written the fully annotated version:

```rust
fn first_explicit<'a>(s: &'a str) -> &'a str {
    s.split_whitespace().next().unwrap_or("")
}
```

Both compile and behave identically; the second is just the *desugared* form of the first. The compiler runs a mechanical procedure to expand the short form into the long form. If that procedure succeeds, the annotations were unnecessary. If it gets stuck, it stops and asks you to annotate.

> **Tip:** "Elided" does not mean "ignored" or "inferred from the function body." The compiler decides lifetimes purely from the *signature*, before it ever looks at the body. This is deliberate: changing a function's body can never silently change its public lifetime contract.

### The three elision rules

The compiler applies these rules **in order**, to the function signature:

1. **Each elided lifetime in the *parameters* gets its own distinct lifetime.**
   `fn f(x: &str, y: &str)` is treated as `fn f<'a, 'b>(x: &'a str, y: &'b str)`. The two parameters get *separate* lifetimes. The compiler never assumes two input references live the same length.

2. **If there is exactly one input lifetime (elided or not), it is assigned to every elided *output* lifetime.**
   `fn f(x: &str) -> &str` becomes `fn f<'a>(x: &'a str) -> &'a str`. The single input is the only thing the output could possibly borrow from, so the compiler wires them together.

3. **If there are multiple input lifetimes *but one of them is `&self` or `&mut self`*, the lifetime of `self` is assigned to every elided output lifetime.**
   This is the rule that makes methods so ergonomic: a method that returns a reference almost always returns a piece of `self`, so the compiler defaults to that.

If, after applying all three rules, any output lifetime is *still* unassigned, elision **fails** and the compiler demands explicit annotations.

### Walking the rules through `first_word`

```rust
fn first_word(s: &str) -> &str { /* ... */ }
```

- **Rule 1** gives the single parameter its own lifetime: `fn first_word<'a>(s: &'a str) -> &str`.
- **Rule 2** applies because there is exactly one input lifetime, so it's copied to the output: `fn first_word<'a>(s: &'a str) -> &'a str`.
- Every output lifetime is now assigned. Elision succeeds. No annotation required.

### Walking the rules through `Headers::get`

```rust
fn get(&self, name: &str) -> Option<&str> { /* ... */ }
```

- **Rule 1** gives each parameter its own lifetime: `&self` gets `'s`, `name` gets `'n`.
- **Rule 2** does *not* apply: there are two input lifetimes, not one.
- **Rule 3** applies because one parameter is `&self`. The output's elided lifetime becomes `'s` (the `self` lifetime): `fn get<'s, 'n>(&'s self, name: &'n str) -> Option<&'s str>`.
- Output lifetime assigned. Elision succeeds.

This is exactly what you want: `get` returns a slice of `self.raw`, and the result is valid for as long as the `Headers` value is borrowed, *not* for as long as the `name` argument lives.

### When the rules run out: multiple inputs, no `self`

```rust
fn longest(a: &str, b: &str) -> &str { /* ... */ } // does not compile (error E0106)
```

- **Rule 1** gives `a` lifetime `'a` and `b` lifetime `'b`.
- **Rule 2** does not apply (two input lifetimes).
- **Rule 3** does not apply (no `self`).
- The output lifetime is still unassigned. **Elision fails**, so you must annotate.

The real compiler error spells this out precisely:

```
error[E0106]: missing lifetime specifier
 --> src/main.rs:1:33
  |
1 | fn longest(a: &str, b: &str) -> &str {
  |               ----     ----     ^ expected named lifetime parameter
  |
  = help: this function's return type contains a borrowed value, but the signature does not say whether it is borrowed from `a` or `b`
help: consider introducing a named lifetime parameter
  |
1 | fn longest<'a>(a: &'a str, b: &'a str) -> &'a str {
  |           ++++     ++          ++          ++
```

Notice the help text: *"the signature does not say whether it is borrowed from `a` or `b`."* That's the elision rules failing in plain English: the compiler genuinely cannot pick for you, so you have to. (The fix, and *why* you write `'a` here, is the subject of the [lifetimes topic](/05-ownership/04-lifetimes/).)

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| Tracking how long a reference is valid | Not tracked; GC keeps things alive | Tracked at compile time via lifetimes |
| Writing lifetime annotations | N/A — no such thing | Required only when elision can't infer |
| Where lifetimes are inferred from | N/A | The **signature**, never the body |
| Returning a substring | New independent string (heap copy) | A *borrow* whose lifetime must be derived |
| "One input → output" case | No annotation (nothing to annotate) | No annotation (rule 2 handles it) |
| Method returning part of `self` | No annotation | No annotation (rule 3 handles it) |
| Function with two refs, returning one | No annotation | **Annotation required** (elision fails) |

### Elision applies to functions and methods — *not* to structs

An important boundary: the three rules apply only to **function and method signatures**. They do **not** apply to struct (or enum) definitions. A struct that holds a reference must always name the lifetime explicitly:

```rust
struct Wrapper {
    text: &str,   // does not compile (error E0106): no elision for struct fields
}
```

The real error:

```
error[E0106]: missing lifetime specifier
 --> src/main.rs:3:11
  |
3 |     text: &str,   // no elision for struct fields
  |           ^ expected named lifetime parameter
  |
help: consider introducing a named lifetime parameter
  |
2 ~ struct Wrapper<'a> {
3 ~     text: &'a str,   // no elision for struct fields
  |
```

The fix is `struct Wrapper<'a> { text: &'a str }`. Storing a reference is a long-term commitment, and the compiler refuses to guess how long it must stay valid. See [Lifetimes](/05-ownership/04-lifetimes/) for structs that hold references.

### Elision is about *omitting*, not *changing*, the rules

A common misconception is that elided code is "less strict" than annotated code. It isn't. The elided form `fn first_word(s: &str) -> &str` enforces *exactly* the same contract as the explicit `fn first_word<'a>(s: &'a str) -> &'a str`. Elision changes the *amount you type*, never the *guarantees you get*.

---

## Common Pitfalls

### Pitfall 1: Assuming the output borrows from "the obvious" parameter

When a method takes `&self` **and** another reference, rule 3 *always* ties the elided output to `self`, even if your code returns a slice of the *other* parameter. This produces a confusing error if your intent differs:

```rust
struct Config {
    name: String,
}

impl Config {
    // Intent: return a slice of `text`. But rule 3 ties the output to `&self`.
    fn extract<'b>(&self, text: &'b str) -> &str {  // does not compile
        &text[..3]
    }
}
```

The real compiler error makes the mismatch explicit:

```
error: lifetime may not live long enough
 --> src/main.rs:8:9
  |
7 |     fn extract<'b>(&self, text: &'b str) -> &str {
  |                --  - let's call the lifetime of this reference `'1`
  |                |
  |                lifetime `'b` defined here
8 |         &text[..3]
  |         ^^^^^^^^^^ method was supposed to return data with lifetime `'1` but it is returning data with lifetime `'b`
  |
help: consider reusing a named lifetime parameter and update trait if needed
  |
7 |     fn extract<'b>(&self, text: &'b str) -> &'b str {
  |                                              ++
```

The compiler is saying: elision made the return `&'self`, but your body returns `&'b` data. The fix is to *opt out* of rule 3 by annotating explicitly: write `-> &'b str` so the output borrows from `text`, exactly as the help suggests.

> **Warning:** Rule 3 is a convenience, not a mind reader. When a method returns a borrow of a *parameter* rather than of `self`, you must annotate to override the default.

### Pitfall 2: Trying to return a reference with no input to borrow from

If a function returns a reference but has *no* reference parameters, none of the rules can supply an output lifetime, because there is nothing to borrow from:

```rust
fn make_greeting() -> &str {   // does not compile (error E0106)
    "hello"
}
```

The real error even guesses your two likely intentions:

```
error[E0106]: missing lifetime specifier
 --> src/main.rs:1:23
  |
1 | fn make_greeting() -> &str {
  |                       ^ expected named lifetime parameter
  |
  = help: this function's return type contains a borrowed value, but there is no value for it to be borrowed from
help: consider using the `'static` lifetime, but this is uncommon unless you're returning a borrowed value from a `const` or a `static`
  |
1 | fn make_greeting() -> &'static str {
  |                        +++++++
help: instead, you are more likely to want to return an owned value
  |
1 - fn make_greeting() -> &str {
1 + fn make_greeting() -> String {
  |
```

For a string literal, `&'static str` is correct (literals live for the whole program). But the more common fix, as the compiler hints, is to return an **owned** `String`. Returning owned data is the right move whenever the function *creates* the value rather than borrowing it from a caller; see [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/).

### Pitfall 3: Believing two input references share a lifetime

Rule 1 gives each input its *own* lifetime. TypeScript developers sometimes assume both arguments "must be the same," but Rust deliberately keeps them separate so the most flexible signature is the default:

```rust
// Elided form...
fn f(a: &str, b: &str) -> usize { a.len() + b.len() }
// ...desugars to TWO distinct lifetimes, NOT one:
fn f_explicit<'a, 'b>(a: &'a str, b: &'b str) -> usize { a.len() + b.len() }
```

This only matters once you start *returning* a reference. At that point you usually *do* want to relate the lifetimes, which is why such functions can't be elided and force you to annotate (Pitfall 1's `longest` is the canonical case).

### Pitfall 4: Expecting elision in closures or function pointers the same way

The three rules are defined for `fn` and method signatures. Closures infer lifetimes through a separate mechanism, and trait objects / `dyn` types have their own default-lifetime rules. Don't assume a hand-written `fn(&str) -> &str` type alias elides the same way a function definition does. When in doubt, annotate. (Closures are covered in [Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/).)

---

## Best Practices

### 1. Omit lifetimes whenever elision allows it

Idiomatic Rust does **not** write annotations the compiler can infer. Adding `'a` where it isn't needed is noise that the Clippy lint `needless_lifetimes` will flag:

```rust
// Unidiomatic: explicit lifetime the rules would infer anyway.
fn first<'a>(s: &'a str) -> &'a str { s.split_whitespace().next().unwrap_or("") }

// Idiomatic: let elision do its job.
fn first(s: &str) -> &str { s.split_whitespace().next().unwrap_or("") }
```

### 2. Reach for annotations only when the compiler asks

The right workflow is: write the clean, elided signature *first*. If it compiles, you're done. If you get `E0106` or a "lifetime may not live long enough" error, *then* add exactly the annotation the error suggests. Don't preemptively annotate "to be safe."

### 3. Run the three rules in your head before reaching for `'a`

When you hit a lifetime error, mentally apply rules 1–3 to the signature. If you can see *why* the output lifetime is unassigned (multiple inputs, no `self`) or *wrong* (rule 3 picked `self` but you meant a parameter), the fix is obvious and you avoid guessing.

### 4. Prefer owned return types when the function creates the data

If a function builds a new string or vector rather than borrowing from an argument, return `String` / `Vec<T>`, not a reference. There's no lifetime to elide because there's no borrow, and the API is simpler for callers. Borrow in, own out is a common and healthy pattern.

> **Tip:** A function whose every reference is handled by elision is a sign of a well-shaped API: it either passes borrows straight through (one input → output) or returns part of `self`. When you find yourself needing many explicit `'a`s, consider whether returning owned data would be cleaner.

---

## Real-World Example

A small, production-flavored HTTP header parser. It borrows the raw request text and hands back slices into it. Almost every method relies on elision. The only explicit lifetime is on the struct itself (where elision never applies) and on a free function that overrides rule defaults:

```rust playground
/// A zero-copy view over raw HTTP header text.
/// The struct borrows the buffer, so it needs an explicit lifetime.
struct Headers<'a> {
    raw: &'a str,
}

impl<'a> Headers<'a> {
    fn new(raw: &'a str) -> Self {
        Headers { raw }
    }

    // Rule 3: `&self` + `name: &str` → output borrows from `&self`.
    // No annotation needed; the returned slice points into `self.raw`.
    fn get(&self, name: &str) -> Option<&str> {
        self.raw
            .lines()
            .filter_map(|line| line.split_once(": "))
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    }

    // Rule 3 again: returns a slice of `self.raw`.
    fn first_line(&self) -> &str {
        self.raw.lines().next().unwrap_or("")
    }
}

// Free function, one reference input → rule 2 supplies the output lifetime.
// `prefix` is NOT a reference we return, so its lifetime is irrelevant here.
fn trim_prefix<'p>(s: &'p str, prefix: &str) -> &'p str {
    s.strip_prefix(prefix).unwrap_or(s)
}

fn main() {
    let raw = "Host: example.com\nContent-Type: text/html\nAccept: */*";
    let headers = Headers::new(raw);

    println!("first line: {}", headers.first_line());
    println!("content-type: {:?}", headers.get("content-type"));
    println!("missing: {:?}", headers.get("authorization"));

    println!("trimmed: {}", trim_prefix("v1.2.3", "v"));
}
```

Real output:

```
first line: Host: example.com
content-type: Some("text/html")
missing: None
trimmed: 1.2.3
```

> **Note:** `trim_prefix` is written with an explicit `'p` *only on `s`* to document that the return value borrows from `s`, not `prefix`. Rule 2 would actually elide this correctly because `prefix`'s lifetime never reaches the output, but here the annotation is intentional documentation, not a requirement. This is the rare case where being explicit aids the reader. The struct's `<'a>`, by contrast, is mandatory: elision never applies to fields.

---

## Further Reading

### Official Documentation

- [The Rust Book — Lifetime Elision](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html#lifetime-elision) — the canonical explanation of the three rules.
- [The Rustonomicon — Lifetime Elision](https://doc.rust-lang.org/nomicon/lifetime-elision.html) — a precise, table-driven restatement with edge cases.
- [Rust Reference — Lifetime Elision](https://doc.rust-lang.org/reference/lifetime-elision.html) — the formal specification, including `impl` and trait-object defaults.

### Related Topics in This Guide

- [Lifetimes](/05-ownership/04-lifetimes/) — what the `'a` annotations *mean* and why they exist; required reading if elision fails.
- [Borrowing](/05-ownership/02-borrowing/) — shared references, the foundation lifetimes track.
- [Mutable References](/05-ownership/03-mutable-references/) — `&mut` and non-lexical lifetimes.
- [Ownership Rules](/05-ownership/01-ownership-rules/) — moves, owners, and scope-based drop.
- [Move, Copy, Clone](/05-ownership/06-move-copy-clone/) — when to return owned data instead of a borrow.
- [Section 05 overview](/05-ownership/) — the full ownership system.
- [Functions: parameters](/03-functions/01-parameters/) and [arrow vs closures](/03-functions/03-arrow-vs-closures/) — where references in signatures first appear.
- [Variables and Mutability](/02-basics/00-variables/) — the immutability foundation.
- [Data Structures](/06-data-structures/) — structs that own vs. borrow their fields.

---

## Exercises

### Exercise 1: Spot the elision

**Difficulty:** Easy

**Objective:** Predict whether a signature needs annotations, then confirm by writing the desugared form.

**Instructions:** For the function below, decide whether lifetime elision succeeds. Then write out the fully annotated (desugared) signature the compiler produces, and add a second function `last_char` that returns `Option<char>` from a `&str`.

```rust
fn last_char(s: &str) -> Option<char> {
    /* ??? */
}

fn main() {
    assert_eq!(last_char("abc"), Some('c'));
    assert_eq!(last_char(""), None);
    println!("ok");
}
```

<details>
<summary>Solution</summary>

Elision **succeeds**: there is one input lifetime (rule 1), and rule 2 copies it to any elided output lifetime. Here the return type is `Option<char>` — `char` is an owned `Copy` type, not a reference — so there's no output lifetime to assign at all. No annotation is needed either way.

```rust playground
fn last_char(s: &str) -> Option<char> {
    s.chars().last()
}

fn main() {
    assert_eq!(last_char("abc"), Some('c'));
    assert_eq!(last_char(""), None);
    println!("ok");
}
```

The desugared signature is simply `fn last_char<'a>(s: &'a str) -> Option<char>`; the lifetime exists on the input but is never used in the output.

</details>

### Exercise 2: Make a method compile via rule 3

**Difficulty:** Medium

**Objective:** Use a method that returns a borrow of `self`, relying on elision rule 3.

**Instructions:** Complete the `head` method so it returns the first `n` bytes of the buffer as a `&str` (clamped to the buffer's length). Do **not** add any explicit lifetime annotations to `head` — let elision handle it.

```rust
struct Buffer {
    data: String,
}

impl Buffer {
    fn head(&self, n: usize) -> &str {
        /* ??? */
    }
}

fn main() {
    let buf = Buffer { data: "hello world".to_string() };
    assert_eq!(buf.head(5), "hello");
    assert_eq!(buf.head(100), "hello world");
    println!("ok");
}
```

<details>
<summary>Solution</summary>

Rule 3 ties the elided output lifetime to `&self`, so the returned slice borrows from `self.data` with no annotation needed:

```rust playground
struct Buffer {
    data: String,
}

impl Buffer {
    fn head(&self, n: usize) -> &str {
        &self.data[..n.min(self.data.len())]
    }
}

fn main() {
    let buf = Buffer { data: "hello world".to_string() };
    assert_eq!(buf.head(5), "hello");
    assert_eq!(buf.head(100), "hello world");
    println!("ok");
}
```

`n.min(self.data.len())` clamps the index so slicing never panics past the end.

</details>

### Exercise 3: When elision fails

**Difficulty:** Hard

**Objective:** Recognize a signature elision cannot resolve and supply the correct annotation.

**Instructions:** Write `pick` so it returns `a` when `flag` is true and `b` otherwise. Because the function has two reference parameters and no `self`, elision fails, so you must add the annotation that says "the result lives as long as both inputs." Explain in one sentence why elision can't do this for you.

```rust
fn pick(flag: bool, a: &str, b: &str) -> &str {
    /* ??? */
}

fn main() {
    assert_eq!(pick(true, "yes", "no"), "yes");
    assert_eq!(pick(false, "yes", "no"), "no");
    println!("ok");
}
```

<details>
<summary>Solution</summary>

Rule 1 gives `a` and `b` separate lifetimes; rule 2 doesn't apply (two inputs); rule 3 doesn't apply (no `self`). With the output lifetime unassigned, elision fails. Tie both inputs and the output to a single lifetime `'a`:

```rust playground
fn pick<'a>(flag: bool, a: &'a str, b: &'a str) -> &'a str {
    if flag { a } else { b }
}

fn main() {
    assert_eq!(pick(true, "yes", "no"), "yes");
    assert_eq!(pick(false, "yes", "no"), "no");
    println!("ok");
}
```

**Why elision can't decide:** the compiler infers lifetimes from the *signature alone*, and the signature is honest that the result could come from `a` *or* `b` — so it cannot know which input the output borrows from without you saying so. The `bool` parameter (which the body uses to choose) is invisible to the elision rules. See [Lifetimes](/05-ownership/04-lifetimes/) for the full meaning of `'a` here.

</details>

---

## Summary

**What you've learned:**

- Lifetime **elision** lets you omit `'a` annotations the compiler can infer from a signature.
- The **three rules**, applied in order: (1) each input reference gets its own lifetime; (2) one input lifetime → copied to all outputs; (3) `&self` present → its lifetime → all outputs.
- Elision runs on the **signature**, never the body, and never changes the guarantees, only the typing.
- It applies to **functions and methods only**; struct fields always need explicit lifetimes.
- When the rules leave an output lifetime unassigned (multiple inputs, no `self`) or assign the *wrong* one (rule 3 picks `self` but you meant a parameter), the compiler asks you to annotate.

**Mental model:**

- Write the clean, un-annotated signature first.
- If it compiles, elision handled it. Leave it alone.
- If it doesn't, run the three rules in your head, find the unassigned output, and add exactly that one annotation.
