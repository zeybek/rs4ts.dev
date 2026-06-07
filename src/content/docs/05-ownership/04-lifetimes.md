---
title: "Lifetimes"
description: "Rust lifetimes ('a) prove every reference stays valid without a garbage collector. Learn what TypeScript's GC hides and when to annotate functions and structs."
---

In TypeScript and JavaScript, the garbage collector quietly keeps any object alive for as long as *something* still points at it. Rust has no garbage collector, so the compiler must instead **prove** that every reference points at data that is still alive. **Lifetimes** are the labels the compiler uses to track and relate how long borrowed data stays valid, and `'a` annotations are how you spell those relationships out when the compiler cannot figure them out alone.

---

## Quick Overview

A **lifetime** is a region of code during which a reference is guaranteed to be valid. The borrow checker (introduced in [Borrowing](/05-ownership/02-borrowing/) and [Mutable References](/05-ownership/03-mutable-references/)) already tracks lifetimes for *every* reference automatically; **lifetime annotations** like `'a` are simply the syntax you use to describe the *relationship between* the lifetimes of inputs and outputs when a function or struct hands out a reference. They never change how long anything actually lives; they are a contract the compiler verifies, not a runtime mechanism. For a TypeScript developer, the closest mental model is "type-level documentation that the borrow points back into one of the arguments," enforced at compile time.

> **Note:** Lifetimes are the part of Rust that feels most alien at first. The good news: thanks to [lifetime elision](/05-ownership/05-lifetime-elision/), you will write explicit `'a` annotations far less often than you fear. This page explains what they *mean* so the elided cases make sense.

---

## TypeScript/JavaScript Example

In TypeScript, a function that returns "whichever string is longer" is trivial, and a class that holds a reference to a shared object is just as easy. Nothing forces you to think about how long the underlying data lives:

```typescript
// strings.ts

// Returns whichever argument is longer. The returned string is the
// SAME object as one of the inputs (strings are immutable, but the point
// stands for objects: no copy is made).
function longest(x: string, y: string): string {
  return x.length > y.length ? x : y;
}

// A class that holds a reference to data it does NOT own.
class Parser {
  private position = 0;

  // `source` is just a reference; the GC keeps it alive as long as
  // this Parser (or anything else) can still reach it.
  constructor(private source: string) {}

  nextWord(): string | null {
    const remaining = this.source.slice(this.position);
    const match = remaining.match(/\S+/);
    if (!match) return null;
    this.position += (match.index ?? 0) + match[0].length;
    return match[0];
  }
}

const result = longest("a long string", "short");
console.log(result); // "a long string"

const parser = new Parser("  hello   world  ");
let word: string | null;
while ((word = parser.nextWord()) !== null) {
  console.log(word); // "hello", then "world"
}
```

**What the runtime does for you here:**

- `longest` returns one of its arguments by reference. JavaScript does not care which — the GC keeps both alive until nobody references them.
- `Parser` stores `source`. Even if the original variable that created the string goes out of scope, the GC keeps the string alive because the `Parser` still points at it.

This "the GC will sort it out" freedom is exactly what Rust removes, and lifetimes are what replace it.

---

## Rust Equivalent

The same two patterns in Rust require us to *name* the lifetime relationship with `'a`:

```rust
// A function that returns one of its borrowed inputs must declare a
// lifetime: the output borrows for as long as BOTH inputs are valid.
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

// A struct that holds a reference must declare the lifetime of that reference.
// `Parser<'a>` means "this Parser cannot outlive the str it borrows".
struct Parser<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Parser<'a> {
        Parser { source, position: 0 }
    }

    // The returned slice borrows from `self.source`, so it shares lifetime 'a.
    fn next_word(&mut self) -> Option<&'a str> {
        let remaining = &self.source[self.position..];
        let start = remaining.find(|c: char| !c.is_whitespace())?;
        let rest = &remaining[start..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        self.position += start + end;
        Some(&rest[..end])
    }
}

fn main() {
    let result = longest("a long string", "short");
    println!("{result}"); // a long string

    let text = String::from("  hello   world  ");
    let mut parser = Parser::new(&text);
    while let Some(word) = parser.next_word() {
        println!("{word:?}"); // "hello", then "world"
    }
}
```

Running it prints exactly what the TypeScript version did:

```text
a long string
"hello"
"world"
```

The `<'a>` after the function or struct name introduces a **generic lifetime parameter**, just like `<T>` introduces a generic type parameter. The annotations `&'a str` then say "this reference lives for the region `'a`," and reusing the same `'a` across multiple spots is how you tie them together.

---

## Detailed Explanation

### Why does Rust need lifetimes at all?

Every reference in Rust has a lifetime: the span during which it is valid. Usually the compiler infers it silently (you have already written dozens of references without annotations). Annotations become necessary only when the compiler **cannot tell, from the signature alone, how the lifetimes relate**.

Consider `longest` without any annotation:

```rust
fn longest(x: &str, y: &str) -> &str { // does not compile (error[E0106])
    if x.len() > y.len() { x } else { y }
}
```

The compiler rejects this. The returned `&str` borrows from *something*, but which one — `x` or `y`? The body decides at runtime based on `len()`, so the signature is genuinely ambiguous. The real error is:

```text
error[E0106]: missing lifetime specifier
 --> src/main.rs:1:33
  |
1 | fn longest(x: &str, y: &str) -> &str {
  |               ----     ----     ^ expected named lifetime parameter
  |
  = help: this function's return type contains a borrowed value, but the
          signature does not say whether it is borrowed from `x` or `y`
help: consider introducing a named lifetime parameter
  |
1 | fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
  |           ++++     ++          ++          ++
```

The compiler even suggests the fix. By writing `<'a>` and tagging `x`, `y`, and the return with the same `'a`, you promise: *the returned reference is valid only for as long as **both** `x` and `y` are valid.* That is enough information to check every call site.

### Annotations describe relationships, not durations

This is the single most important idea, and the one that trips up newcomers most:

> **`'a` does not make anything live longer or shorter. It only describes how the lifetime of one reference relates to another.**

`<'a>` is generic. At each call site, the compiler picks the *smallest concrete region* that satisfies all the `'a` constraints, typically the overlap of all the inputs' lifetimes. You are not choosing a duration; you are stating a rule the borrow checker must respect.

### How the constraint is enforced at the call site

The annotation pays off when references have *different* scopes. This compiles, because both inputs and the use of `result` all overlap:

```rust
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

fn main() {
    let s1 = String::from("a long string");
    let result;
    {
        let s2 = String::from("short");
        result = longest(s1.as_str(), s2.as_str());
        println!("The longest string is: {result}"); // used INSIDE the inner scope
    }
}
```

```text
The longest string is: a long string
```

But move the `println!` *out* of the inner block, so `result` is used after `s2` has been dropped, and the borrow checker stops you, because the return *might* have borrowed from `s2`:

```rust
fn main() {
    let s1 = String::from("a long string");
    let result;
    {
        let s2 = String::from("short");
        result = longest(s1.as_str(), s2.as_str());
    } // s2 dropped here
    println!("The longest string is: {result}"); // does not compile (error[E0597])
}
```

The real compiler error is precise about the cause:

```text
error[E0597]: `s2` does not live long enough
  --> src/main.rs:10:39
   |
 9 |         let s2 = String::from("short");
   |             -- binding `s2` declared here
10 |         result = longest(s1.as_str(), s2.as_str());
   |                                       ^^ borrowed value does not live long enough
11 |     } // s2 dropped here
   |     - `s2` dropped here while still borrowed
12 |     println!("The longest string is: {result}"); // does not compile (error[E0597])
   |                                       ------ borrow later used here
```

In TypeScript this code is perfectly fine: the GC keeps `s2`'s string alive because `result` still references it. Rust has no GC, so it forbids the dangling reference *at compile time*. This is the same dangling-prevention guarantee from [Borrowing](/05-ownership/02-borrowing/), now lifted to function boundaries.

### Lifetimes in struct definitions

When a struct field is a reference, the struct must be parameterized by that reference's lifetime. `Parser<'a>` means "a `Parser` that borrows a `str` for the region `'a`, and therefore the `Parser` itself cannot outlive that `str`."

```rust
struct Highlight<'a> {
    text: &'a str,
}
```

If you let the borrowed data drop while the struct still holds the reference, the same `E0597` fires:

```rust
struct Highlight<'a> {
    text: &'a str,
}

fn main() {
    let highlight;
    {
        let sentence = String::from("important note");
        highlight = Highlight { text: &sentence }; // does not compile (error[E0597])
    } // sentence dropped here while still borrowed
    println!("{}", highlight.text);
}
```

```text
error[E0597]: `sentence` does not live long enough
  --> src/main.rs:9:39
   |
 8 |         let sentence = String::from("important note");
   |             -------- binding `sentence` declared here
 9 |         highlight = Highlight { text: &sentence };
   |                                       ^^^^^^^^^ borrowed value does not live long enough
10 |     }
   |     - `sentence` dropped here while still borrowed
11 |     println!("{}", highlight.text);
   |                    -------------- borrow later used here
```

> **Tip:** If a struct holding a reference starts causing lifetime headaches, ask whether it should **own** its data instead (`text: String`). Owning is the simpler default; borrowing structs are an optimization for short-lived "view" types like parsers and iterators. See [Move, Copy, Clone](/05-ownership/06-move-copy-clone/) and [`Rc`/`Arc`](/05-ownership/07-reference-counting/) for the owning alternatives.

### Lifetimes on `impl` blocks

The `impl<'a> Parser<'a>` line declares `'a` for the impl and then uses it on the type. The `<'a>` after `impl` is the *declaration*; the `<'a>` after `Parser` is the *use*, exactly mirroring how generic types work with `impl<T> Wrapper<T>`.

### Distinct lifetimes when inputs are unrelated

You do **not** have to give every reference the same `'a`. If the output only ever borrows from *one* argument, give the others their own lifetime so callers are not over-constrained:

```rust
// The return is only ever borrowed from `text`, never from `prefix`,
// so they get independent lifetimes 'a and 'b.
fn strip_prefix<'a, 'b>(text: &'a str, prefix: &'b str) -> &'a str {
    if let Some(rest) = text.strip_prefix(prefix) {
        rest
    } else {
        text
    }
}

fn main() {
    let path = String::from("/api/users");
    let stripped;
    {
        let prefix = String::from("/api"); // shorter-lived than `path`
        stripped = strip_prefix(&path, &prefix);
    } // `prefix` dropped here — fine, the output never borrowed from it
    println!("{stripped}"); // /users
}
```

```text
/users
```

Had we forced `prefix: &'a str`, the compiler would have demanded `prefix` live as long as the output — an unnecessary restriction that would reject this perfectly safe program. **Use the loosest annotations that are still correct.**

### The `'static` lifetime

`'static` is a special, built-in lifetime meaning "valid for the entire program." Every string literal has type `&'static str` because the text is baked into the binary:

```rust
fn app_name() -> &'static str {
    "ts-to-rust"
}
```

> **Warning:** Do not reach for `'static` to silence a lifetime error — it almost always makes the problem worse by demanding data live *forever*. The fix is usually to relate lifetimes correctly or to own the data, not to claim `'static`.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Keeping referenced data alive | Garbage collector, at runtime | Lifetimes, proven at compile time |
| Returning "one of the arguments" | Always fine | Needs a lifetime relating output to inputs |
| A class/struct holding a reference | Free; GC tracks reachability | Struct must declare `<'a>`; cannot outlive borrowed data |
| Dangling reference | Impossible (GC) but stale data is possible | Rejected by the compiler (`E0597`/`E0515`) |
| Runtime cost of the mechanism | GC pauses, extra memory | **Zero**: lifetimes are erased after checking |
| "Forever" reference | Any long-lived object | `'static` |
| Where you write it | Nowhere | Function signatures, struct defs, `impl` blocks |

The deepest contrast: **lifetimes are a compile-time-only construct.** Like TypeScript types (which are erased before the JS runs), `'a` annotations vanish entirely after the borrow checker has done its job. There is no runtime representation of a lifetime, no overhead, and no equivalent of a GC pause. They are pure, checked documentation of an invariant the machine code already upholds.

A second contrast worth internalizing: in TypeScript, a *reference is the cheap default* and copying is the thing you opt into. In Rust, the borrow checker treats every borrow as a liability it must account for, which is precisely why borrowed return values and borrowed struct fields require you to spell out the relationship.

---

## Common Pitfalls

### Pitfall 1: Returning a reference to a local variable

A classic mistake, and one no annotation can rescue, because the data genuinely dies:

```rust
fn dangling<'a>() -> &'a str {
    let s = String::from("temporary");
    &s // does not compile (error[E0515]): returns a reference to data owned by the current function
}
```

```text
error[E0515]: cannot return reference to local variable `s`
 --> src/main.rs:3:5
  |
3 |     &s // does not compile (error[E0515]): returns a reference to data owned by the current function
  |     ^^ returns a reference to data owned by the current function
```

`s` is dropped when `dangling` returns, so any reference to it would dangle. The compiler is not asking for a lifetime annotation here — it is telling you the design is impossible. **Fix:** return the owned `String` instead of a reference.

### Pitfall 2: Thinking `'a` extends a value's life

Newcomers often add `'static` or a longer-looking lifetime hoping to "keep the data around." It does the opposite of what you imagine — it *requires* the caller to supply data that lives that long, usually producing a worse error. `'a` constrains; it never extends. The fix for "this data does not live long enough" is almost always to restructure ownership (own the data, or make the borrower's scope shorter), not to relabel the lifetime.

### Pitfall 3: Over-constraining with a single shared lifetime

Reusing one `'a` for every parameter when the output only borrows from one of them forces callers to keep *all* arguments alive as long as the result. This compiles but needlessly rejects valid call sites (as in the `strip_prefix` example). Give unrelated inputs their own lifetimes.

### Pitfall 4: Forgetting the lifetime on the `impl`

Writing `impl Parser` instead of `impl<'a> Parser<'a>` fails because `Parser` is not a complete type without its lifetime parameter, just as `impl Wrapper` would fail for `Wrapper<T>`. You must declare `<'a>` after `impl` and then apply it to the type.

### Pitfall 5: Reaching for annotations the compiler does not want

Because of [lifetime elision](/05-ownership/05-lifetime-elision/), many single-reference functions need *no* annotation. Adding `<'a>` everywhere is noise; Clippy's `needless_lifetimes` lint will flag the redundant ones. Write annotations only when the compiler asks for them or when elision would pick the wrong relationship.

---

## Best Practices

- **Let elision do the work.** Annotate only when the compiler demands it. See [Lifetime Elision](/05-ownership/05-lifetime-elision/) for the three rules that cover the common cases.
- **Prefer owning over borrowing in stored data.** If a struct does not *need* to borrow, store `String`/`Vec<T>` instead of `&str`/`&[T]`. Lifetime-parameterized structs ripple their `'a` through every function that touches them.
- **Use the loosest correct lifetimes.** Give unrelated references distinct lifetime parameters so you do not over-constrain callers.
- **Name lifetimes meaningfully when there are several.** `'a` is conventional for one; for multiple, descriptive names like `<'input, 'config>` read better than `<'a, 'b, 'c>`.
- **Reach for `'static` only when the data truly is static** (literals, leaked allocations, lazily-initialized globals), never as a quick fix.
- **When fighting the borrow checker, change the design, not the annotation.** Shorter borrows, cloning ([Move, Copy, Clone](/05-ownership/06-move-copy-clone/)), or shared ownership via [`Rc`/`Arc`](/05-ownership/07-reference-counting/) are usually the real answer.

---

## Real-World Example

A log-line parser is a realistic place where borrowing (and therefore lifetimes) earns its keep: parsing a `&str` into structured fields that are *slices of the original buffer* avoids allocating new strings for every line, important in a hot logging path.

```rust
/// A parsed log line whose fields borrow directly from the original buffer.
/// `LogLine<'a>` cannot outlive the string it was parsed from.
#[derive(Debug)]
struct LogLine<'a> {
    level: &'a str,
    message: &'a str,
}

/// Parse "LEVEL: message" into borrowed slices. The `'_` in the return type
/// is the elided lifetime tied to `line` (covered in lifetime-elision.md).
fn parse_log_line(line: &str) -> Option<LogLine<'_>> {
    let (level, message) = line.split_once(": ")?;
    Some(LogLine {
        level,
        message: message.trim(),
    })
}

fn main() {
    let raw = String::from("WARN: disk almost full");
    let parsed = parse_log_line(&raw).expect("well-formed line");

    println!("{parsed:?}");
    assert_eq!(parsed.level, "WARN");
    assert_eq!(parsed.message, "disk almost full");
    println!("ok");
}
```

```text
LogLine { level: "WARN", message: "disk almost full" }
ok
```

No heap allocation happens during parsing: `level` and `message` are *windows into* `raw`. The `LogLine<'a>` annotation is what lets the compiler guarantee those windows can never outlive `raw`. In TypeScript you would get this "view" behavior for free (substrings reference the same backing data, GC permitting), but you would also have zero compile-time protection against using a `LogLine` after its source buffer became unreachable. Rust makes the same zero-copy design *and* makes the dangling case a compile error.

> **Tip:** When a borrowing parser like this becomes awkward to thread through your program, switch the fields to owned `String`s. You trade a small allocation per line for freedom from lifetime parameters, often the right call outside of hot paths.

---

## Further Reading

### Official Documentation

- [The Rust Book — Validating References with Lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html)
- [The Rust Book — Lifetime Annotations in Struct Definitions](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html#lifetime-annotations-in-struct-definitions)
- [Rust by Example — Lifetimes](https://doc.rust-lang.org/rust-by-example/scope/lifetime.html)
- [Rust Reference — Lifetime elision](https://doc.rust-lang.org/reference/lifetime-elision.html)
- [The `'static` lifetime (std docs)](https://doc.rust-lang.org/std/keyword.static.html)
- [Error index — E0106](https://doc.rust-lang.org/error_codes/E0106.html), [E0515](https://doc.rust-lang.org/error_codes/E0515.html), [E0597](https://doc.rust-lang.org/error_codes/E0597.html)

### Related Sections in This Guide

- [Borrowing](/05-ownership/02-borrowing/): references `&`, shared borrows, and dangling prevention (the foundation for lifetimes)
- [Mutable References](/05-ownership/03-mutable-references/): `&mut`, the borrow rules, and non-lexical lifetimes
- [Lifetime Elision](/05-ownership/05-lifetime-elision/) — the three rules that let you *omit* most `'a` annotations
- [Ownership Rules](/05-ownership/01-ownership-rules/): owners, moves, and scope-based drops that lifetimes build on
- [Move, Copy, Clone](/05-ownership/06-move-copy-clone/): owning your data as an alternative to borrowing it
- [Reference Counting (`Rc`/`Arc`)](/05-ownership/07-reference-counting/) — shared ownership when a single lifetime will not do
- [Stack vs Heap](/05-ownership/00-stack-heap/): where the borrowed data actually lives
- [Functions — Parameters](/03-functions/01-parameters/): why slice and `&str` parameters are borrowed
- [Basics — Types](/02-basics/01-types/) — the underlying types (`str`, slices) you borrow
- [Data Structures](/06-data-structures/): structs and enums that may carry lifetime parameters
- [Ownership — Section Overview](/05-ownership/)

---

## Exercises

### Exercise 1: Your first explicit lifetime

**Difficulty:** Easy

**Objective:** Write a function that returns a borrowed slice of one of its inputs, declaring the lifetime relationship yourself.

**Instructions:** Implement `first_word` so it returns the first whitespace-delimited word of `s` as a `&str` borrowed from `s`. If there is no space, return the whole string. Make `first_word("hello world")` return `"hello"` and `first_word("single")` return `"single"`. Write the lifetime annotation explicitly (do not rely on elision for this exercise).

```rust
fn first_word<'a>(s: &'a str) -> &'a str {
    // TODO: find the first space; return the slice before it, or all of `s`.
    todo!()
}
```

<details>
<summary>Solution</summary>

```rust
fn first_word<'a>(s: &'a str) -> &'a str {
    match s.find(' ') {
        Some(i) => &s[..i],
        None => s,
    }
}

fn main() {
    assert_eq!(first_word("hello world"), "hello");
    assert_eq!(first_word("single"), "single");
    println!("ex1 ok");
}
```

The single `'a` ties the output slice to the input string, so the compiler knows the returned `&str` cannot outlive `s`. (Thanks to elision, you could drop the annotations entirely here, but writing them out shows what the compiler infers.)

</details>

### Exercise 2: A struct that borrows

**Difficulty:** Medium

**Objective:** Define a struct that holds a reference, parameterized by a lifetime, and add a method that returns borrows tied to that same lifetime.

**Instructions:** Define `Tokenizer<'a>` holding an `input: &'a str`. Add `new(input: &'a str) -> Self` and a method `tokens(&self) -> Vec<&'a str>` that splits `input` on commas and trims whitespace from each token. For input `"a, b ,c"`, `tokens()` must return `vec!["a", "b", "c"]`.

<details>
<summary>Solution</summary>

```rust
struct Tokenizer<'a> {
    input: &'a str,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Tokenizer { input }
    }

    fn tokens(&self) -> Vec<&'a str> {
        self.input.split(',').map(|t| t.trim()).collect()
    }
}

fn main() {
    let data = String::from("a, b ,c");
    let tk = Tokenizer::new(&data);
    assert_eq!(tk.tokens(), vec!["a", "b", "c"]);
    println!("ex2 ok");
}
```

Note that `tokens` returns `Vec<&'a str>`, not `Vec<&str>` tied to `&self`: each token borrows from `self.input` (lifetime `'a`), so the returned slices are valid for as long as the original `data` string lives, independent of how long the `Tokenizer` itself does.

</details>

### Exercise 3: Borrowing from a slice and handling emptiness

**Difficulty:** Medium–Hard

**Objective:** Return a reference into a borrowed collection, relating the output lifetime to the slice, while handling the empty case without panicking.

**Instructions:** Implement `longest_in(items: &[String]) -> Option<&str>` that returns the longest string in the slice as a borrowed `&str`, or `None` if the slice is empty. The returned reference must borrow from `items` (so it stays valid as long as `items` does). `longest_in(&["a".into(), "ccc".into(), "bb".into()])` should be `Some("ccc")`; an empty slice should give `None`.

<details>
<summary>Solution</summary>

```rust
fn longest_in<'a>(items: &'a [String]) -> Option<&'a str> {
    items
        .iter()
        .max_by_key(|s| s.len())
        .map(|s| s.as_str())
}

fn main() {
    let words = vec![
        String::from("a"),
        String::from("ccc"),
        String::from("bb"),
    ];
    assert_eq!(longest_in(&words), Some("ccc"));

    let empty: Vec<String> = Vec::new();
    assert_eq!(longest_in(&empty), None);
    println!("ex3 ok");
}
```

`max_by_key` returns `Option<&String>` borrowed from the slice; `.map(|s| s.as_str())` converts the inner `&String` to a `&str` *without* changing the lifetime, so the result is `Option<&'a str>`. The `None` arm handles the empty slice: there is no panic and no need for a sentinel value, mirroring the TypeScript `string | null` return but enforced by the type system.

As in Exercise 1, the `'a` here is elidable: `fn longest_in(items: &[String]) -> Option<&str>` compiles identically and is what Clippy's `needless_lifetimes` lint will nudge you toward. It is written explicitly above to keep the input→output borrow relationship visible while you are still learning to read it.

</details>
