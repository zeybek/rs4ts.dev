---
title: "Strings: `String` vs `&str`"
description: "Rust splits the JavaScript string into two types: the owned, growable String and the borrowed slice &str. Learn which to use, plus UTF-8 and byte lengths."
---

Rust splits the single JavaScript `string` into two types: the owned, growable `String` and the borrowed slice `&str`. Understanding which one to use, and why, is one of the most useful things a TypeScript/JavaScript developer can learn early.

---

## Quick Overview

In JavaScript a `string` is one thing: an immutable, garbage-collected sequence of UTF-16 code units. Rust has **two** core string types instead: a `String` (owned, heap-allocated, growable) and a `&str` (a borrowed, read-only view into UTF-8 bytes someone else owns). The distinction matters because it ties directly into [ownership and borrowing](/05-ownership/), and once it clicks, the rest of Rust's string story (UTF-8, slicing, no integer indexing) follows naturally.

---

## TypeScript/JavaScript Example

```typescript
// TypeScript - one string type, does everything
let greeting: string = "Hello";

// "Mutation" actually creates a brand-new string each time
greeting = greeting + ", world";
greeting += "!";
console.log(greeting); // "Hello, world!"

// Strings are indexable and have a .length
console.log(greeting.length);    // 13
console.log(greeting[0]);        // "H"
console.log(greeting.charAt(0)); // "H"

// Passing a string to a function is cheap — it's a reference under the hood
function shout(text: string): string {
  return text.toUpperCase();
}
console.log(shout(greeting)); // "HELLO, WORLD!"

// But .length and indexing count UTF-16 code units, not characters:
console.log("café".length);    // 4
console.log("\u{1F44B}".length);      // 2  (one emoji, but a surrogate pair!)
console.log([..."\u{1F44B}"].length); // 1  (spread iterates by code point)
```

**Key points:**

- One type (`string`) for literals, fields, parameters, and return values.
- Strings are immutable; `+=` allocates a new string each time.
- `.length` and `s[i]` operate on UTF-16 code units, which silently lies for emoji and other astral-plane characters.

---

## Rust Equivalent

```rust
fn main() {
    // Two types, two jobs:
    let literal: &str = "Hello";              // borrowed slice, baked into the binary
    let mut owned: String = String::from("Hello"); // heap-allocated, growable

    // Real in-place mutation (owned must be `mut`):
    owned.push_str(", world");
    owned.push('!');
    println!("{owned}"); // "Hello, world!"

    // No .length — `len()` is the number of UTF-8 *bytes*:
    println!("{}", owned.len());          // 13
    println!("{}", "café".len());         // 5  (é is two bytes)
    println!("{}", "café".chars().count()); // 4  (count actual characters)

    // Functions take `&str` so they accept BOTH a &String and a literal:
    println!("{}", shout(&owned));  // pass a &String
    println!("{}", shout(literal)); // pass a &str literal
}

fn shout(text: &str) -> String {
    text.to_uppercase()
}
```

Real output:

```text
Hello, world!
13
5
4
HELLO, WORLD!
HELLO
```

**Key points:**

- `&str` is the borrowed view; `String` is the owned, growable buffer.
- A `String` is mutable only when bound with `let mut`; a `&str` is never mutable.
- `len()` returns **bytes**, not characters. Rust never pretends UTF-8 is fixed-width.
- Idiomatic functions accept `&str` parameters so callers can pass either type for free.

---

## Detailed Explanation

### Two types, one mental model

Picture the JavaScript string split along the **ownership** axis you learned in [Section 05](/05-ownership/):

| Concept | JavaScript | Rust |
| --- | --- | --- |
| Owns its heap buffer, can grow | (every `string`, via the GC) | `String` |
| Borrows someone else's bytes, read-only | (not a distinct type) | `&str` |

A `String` is essentially a struct of three machine words: a pointer to a heap buffer, a length, and a capacity, almost exactly like a `Vec<u8>` (see [Vectors](/07-collections/00-vectors/)) that is guaranteed to hold valid UTF-8.

A `&str` is a **fat pointer**: just a pointer to some UTF-8 bytes plus a length. It does not own those bytes, so it cannot free them, cannot grow, and cannot outlive whatever it points into. String **literals** like `"Hello"` are `&'static str`: they point into the compiled binary's read-only data and live for the entire program.

### Creating each type

```rust
fn main() {
    // &str: just write a literal
    let a: &str = "hello";

    // String: several equivalent constructors
    let b = String::from("hello");
    let c = "hello".to_string();
    let d = "hello".to_owned();

    println!("{a} {b} {c} {d}");
}
```

Real output:

```text
hello hello hello hello
```

`to_string()`, `to_owned()`, and `String::from()` all heap-allocate a copy. Reach for them only when you genuinely need ownership: a value you will mutate, store in a struct, or return from a function.

### Borrowing: `&String` coerces to `&str`

Rust applies **deref coercion** so that a `&String` automatically becomes a `&str` wherever one is expected. This is why a single `&str` parameter accepts every common caller:

```rust
fn main() {
    let s = String::from("greetings");
    takes_str(&s);        // &String -> &str (deref coercion)
    takes_str("hello");   // &str literal passes directly
    takes_str(&s[0..3]);  // a sub-slice of the String
}

fn takes_str(s: &str) {
    println!("got: {s}");
}
```

Real output:

```text
got: greetings
got: hello
got: gre
```

> **Tip:** Prefer `&str` over `&String` for function parameters. `&str` accepts strictly more callers (literals, slices, and `&String`), so there is no reason to write `fn f(s: &String)`.

### UTF-8, length, and characters

Every Rust string — `String` or `&str` — is guaranteed to be **valid UTF-8**. That guarantee is the root cause of the three biggest surprises for JavaScript developers.

First, `len()` is bytes, and `chars().count()` is characters:

```rust
fn main() {
    let cafe = "café";  // 'c' 'a' 'f' are 1 byte each; 'é' is 2 bytes
    println!("len (bytes)      = {}", cafe.len());           // 5
    println!("chars().count()  = {}", cafe.chars().count()); // 4

    let wave = "\u{1F44B}";
    println!("emoji bytes = {}", wave.len());            // 4
    println!("emoji chars = {}", wave.chars().count());  // 1
}
```

Real output:

```text
len (bytes)      = 5
chars().count()  = 4
emoji bytes = 4
emoji chars = 1
```

Contrast with Node v22, where `"café".length` is `4` and `"\u{1F44B}".length` is `2` (a UTF-16 surrogate pair counted as two units). Neither language gives you "characters" from a length property — JavaScript counts UTF-16 units and Rust counts UTF-8 bytes — but Rust forces you to *say which one you mean*.

> **Note:** A Rust `char` is a **Unicode scalar value** and is always 4 bytes wide, regardless of how it is encoded inside a string. The string itself stays compact UTF-8; only the standalone `char` type is fixed-width. See [Basic Types](/02-basics/01-types/) for the `char` type.

### No integer indexing — slice by byte range instead

JavaScript lets you write `s[0]`. Rust does **not** let you index a string by an integer, because answering "what is byte 0?" versus "what is character 0?" is ambiguous in a variable-width encoding, and a single-`char` answer might require reading several bytes. Instead you slice by a **byte range**:

```rust
fn main() {
    let hello = "hello";
    let h = &hello[0..1]; // a &str of length 1: "h"
    println!("{h}");

    // The "nth character" comes from the chars() iterator, not indexing:
    let third = "hello".chars().nth(2); // Some('l')
    println!("{third:?}");
}
```

Real output:

```text
h
Some('l')
```

Slicing returns a `&str` (a borrowed view): no allocation, no copy. But the byte offsets you pass **must land on character boundaries**, which is the next pitfall.

### Iterating: `chars()`, `bytes()`, `char_indices()`

```rust
fn main() {
    for (i, ch) in "café".char_indices() {
        print!("({i},{ch}) ");
    }
    println!();
}
```

Real output:

```text
(0,c) (1,a) (2,f) (3,é) 
```

Notice the byte index jumps to `3` for `'é'`, then the next character (if any) would start at byte `5`. The indices are byte offsets, which is exactly what you need to slice safely. (More iteration methods — `split`, `trim`, `replace`, `parse` — are covered in [String Manipulation](/07-collections/02-string-manipulation/).)

---

## Key Differences

| Aspect | JavaScript `string` | Rust `String` | Rust `&str` |
| --- | --- | --- | --- |
| Ownership | GC-managed | Owns a heap buffer | Borrows bytes it does not own |
| Mutability | Immutable (re-binds) | Mutable when `let mut` | Always read-only |
| Growable | n/a (new string each time) | Yes (`push`, `push_str`) | No |
| Encoding | UTF-16 internally | Guaranteed valid UTF-8 | Guaranteed valid UTF-8 |
| `.length` / `len()` | UTF-16 code units | UTF-8 **bytes** | UTF-8 **bytes** |
| Integer indexing `s[i]` | Allowed (UTF-16 unit) | Not allowed (compile error) | Not allowed (compile error) |
| Range slicing `&s[a..b]` | n/a (use `slice`) | Allowed (must hit char boundary) | Allowed (must hit char boundary) |
| Typical role | everything | a value you build/own/store | a parameter or temporary view |

### Why two types?

The split is not arbitrary; it is ownership applied to text:

- A `&str` is a cheap, copyable **view**. Passing one moves only a pointer and a length; no heap data is touched. This is why parameters are almost always `&str`.
- A `String` is what you reach for when you need to **own** the bytes: build a value piece by piece, mutate it, store it in a struct, or return freshly created text from a function.

This mirrors `&[T]` vs `Vec<T>` for non-text data ([Vectors](/07-collections/00-vectors/)): a slice borrows, the `Vec`/`String` owns.

---

## Common Pitfalls

### Pitfall 1: Trying to index a string with an integer

```rust
fn main() {
    let s = String::from("hello");
    let ch = s[0]; // does not compile (error[E0277]) — strings aren't integer-indexable
    println!("{ch}");
}
```

Real compiler error:

```text
error[E0277]: the type `str` cannot be indexed by `{integer}`
 --> src/main.rs:3:16
  |
3 |     let ch = s[0]; // try to index by integer
  |                ^ string indices are ranges of `usize`
  |
  = help: the trait `SliceIndex<str>` is not implemented for `{integer}`
  = note: you can use `.chars().nth()` or `.bytes().nth()`
          for more information, see chapter 8 in The Book: <https://doc.rust-lang.org/book/ch08-02-strings.html#indexing-into-strings>
```

The compiler even tells you the fix: use `.chars().nth(i)` for the i-th character or `.bytes().nth(i)` for the i-th byte.

### Pitfall 2: Slicing in the middle of a multi-byte character

This one compiles fine but **panics at runtime**, the most dangerous kind, because the type checker can't catch a byte offset computed at runtime:

```rust
fn main() {
    let s = "café";        // 'caf' = bytes 0..3, 'é' = bytes 3..5
    let bad = &s[0..4];    // panics: byte 4 splits 'é' in half
    println!("{bad}");
}
```

Real runtime panic:

```text
thread 'main' panicked at src/main.rs:3:17:
byte index 4 is not a char boundary; it is inside 'é' (bytes 3..5) of `café`
```

The fix is to use boundaries you got from `char_indices()`, or use the non-panicking `.get(range)` which returns an `Option` instead:

```rust
fn main() {
    let s = "café";
    println!("{:?}", s.get(0..3)); // Some("caf")
    println!("{:?}", s.get(0..4)); // None — not a boundary, no panic
    println!("{}", s.is_char_boundary(3)); // true
    println!("{}", s.is_char_boundary(4)); // false
}
```

Real output:

```text
Some("caf")
None
true
false
```

### Pitfall 3: Expecting a `&str` to be mutable or growable

```rust
fn main() {
    let greeting: &str = "hello";
    greeting.push_str(" world"); // does not compile (error[E0599])
    println!("{greeting}");
}
```

Real compiler error:

```text
error[E0599]: no method named `push_str` found for reference `&str` in the current scope
 --> src/main.rs:3:14
  |
3 |     greeting.push_str(" world"); // &str has no push_str; also not mutable
  |              ^^^^^^^^ method not found in `&str`
```

To grow text, you need an owned, mutable `String`: `let mut greeting = String::from("hello");`.

### Pitfall 4: Returning a `&str` that borrows a local

A function that builds a `String` locally cannot hand back a `&str` into it; the buffer is freed when the function returns:

```rust
fn make_greeting() -> &str { // does not compile (error[E0106])
    let s = String::from("hello");
    &s
}
```

Real compiler error (abridged):

```text
error[E0106]: missing lifetime specifier
 --> src/main.rs:1:23
  |
1 | fn make_greeting() -> &str {
  |                       ^ expected named lifetime parameter
  |
  = help: this function's return type contains a borrowed value, but there is no value for it to be borrowed from
help: instead, you are more likely to want to return an owned value
  |
1 - fn make_greeting() -> &str {
1 + fn make_greeting() -> String {
```

The compiler's suggested fix is correct: return an owned `String`. You can only return a `&str` when it borrows from one of the function's **input** references (so the caller still owns the data); see `first_word` in the Best Practices section.

---

## Best Practices

### Accept `&str`, return `String` (the default rule of thumb)

```rust
fn main() {
    let owned = String::from("the quick brown fox");
    println!("{}", first_word(&owned)); // borrow input, return a borrowed slice
}

// Borrows the input; the returned &str points into the caller's data.
fn first_word(s: &str) -> &str {
    match s.find(' ') {
        Some(idx) => &s[..idx],
        None => s,
    }
}
```

Real output:

```text
the
```

Returning a `&str` here is fine because it borrows from the `s` parameter, which the caller owns: no dangling reference. When you *create* new text, return an owned `String` instead.

### Build strings with `push_str`/`push` or `format!`, not repeated `+`

The `+` operator on strings consumes (moves) the left-hand `String` and borrows the right-hand `&str`, which is awkward and easy to get wrong. Prefer `format!` for assembling, or `push_str` in a loop:

```rust
fn main() {
    // `format!` borrows its arguments — nothing is consumed:
    let a = String::from("foo");
    let b = String::from("bar");
    let joined = format!("{a}-{b}");
    println!("{joined} (a still usable: {a})");

    // The `+` operator MOVES the left operand:
    let hello = String::from("Hello, ");
    let world = String::from("world!");
    let combined = hello + &world; // `hello` is moved into `combined`
    println!("{combined}");
}
```

Real output:

```text
foo-bar (a still usable: foo)
Hello, world!
```

> **Tip:** Use inline format arguments — `format!("{a}-{b}")` — not the older positional `format!("{}-{}", a, b)`. Both work, but the inline form is the current idiom and reads better. (See [Output](/02-basics/04-output/).)

### Comparing strings is direct

Unlike some languages, you compare string contents with `==`, and `String` compares cleanly against `&str` in either order:

```rust
fn main() {
    let name = String::from("alice");
    println!("{}", name == "alice"); // true
    println!("{}", "alice" == name); // true
}
```

Real output:

```text
true
true
```

### Reserve capacity when building large strings

If you know roughly how big a `String` will get, `String::with_capacity(n)` pre-allocates and avoids repeated re-allocations as it grows, the same idea as `Vec::with_capacity` (see [Collection Performance](/07-collections/09-collection-performance/)).

---

## Real-World Example

A small text-normalization module of the kind you'd find in a web backend: canonicalizing user handles, and truncating a bio for display without ever splitting a multi-byte character. Note how every function takes `&str` (so callers pass a `String`, a `&String`, or a literal) and only allocates a `String` when it must build new text.

```rust
/// Normalize a user-supplied handle into a canonical form.
/// Takes `&str` so callers can pass a `String`, a `&String`, or a literal.
fn normalize_handle(raw: &str) -> String {
    raw.trim()                       // drop surrounding whitespace
        .trim_start_matches('@')     // strip a leading @ if present
        .to_lowercase()              // case-insensitive handles
}

/// Truncate to at most `max_chars` *characters*, never splitting a
/// multi-byte character. Returns a borrowed slice — no allocation.
fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        // `byte_idx` came from char_indices(), so it IS a char boundary.
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s, // fewer than max_chars chars: return the whole thing
    }
}

fn main() {
    let from_form = String::from("  @AliceWonderland ");
    let from_literal = "@Bob";

    // A &String and a &str both work, because the parameter is &str:
    println!("{}", normalize_handle(&from_form));   // "alicewonderland"
    println!("{}", normalize_handle(from_literal)); // "bob"

    // Multi-byte-safe truncation:
    let bio = "héllo wörld"; // contains 2-byte é and ö
    println!("{:?}", truncate_chars(bio, 5));   // "héllo"
    println!("{:?}", truncate_chars(bio, 100)); // whole string

    let kanji = "日本語テスト"; // each kanji is 3 bytes
    println!("{:?}", truncate_chars(kanji, 2)); // "日本"
}
```

Real output:

```text
alicewonderland
bob
"héllo"
"héllo wörld"
"日本"
```

This passes `cargo clippy` with no warnings. The point: `truncate_chars` returns a borrowed `&str` (zero allocation) precisely because it borrows its input, while `normalize_handle` returns an owned `String` because `to_lowercase()` produces new text that must be owned by *someone*.

---

## Further Reading

- [The Rust Book — Storing UTF-8 Encoded Text with Strings](https://doc.rust-lang.org/book/ch08-02-strings.html)
- [`std::string::String` API docs](https://doc.rust-lang.org/std/string/struct.String.html)
- [`str` primitive API docs](https://doc.rust-lang.org/std/primitive.str.html)
- [Rust by Example — Strings](https://doc.rust-lang.org/rust-by-example/std/str.html)
- Sibling topics: [String manipulation methods](/07-collections/02-string-manipulation/) · [Vectors (`Vec<T>`)](/07-collections/00-vectors/) · [Iterators](/07-collections/06-iterators/)
- Foundations: [Ownership & Borrowing](/05-ownership/) · [Basic Types & `char`](/02-basics/01-types/) · [Output & `format!`](/02-basics/04-output/)
- When string operations can fail (e.g. `parse`), see [Error Handling](/08-error-handling/).

---

## Exercises

### Exercise 1: Count the vowels

**Difficulty:** Beginner

**Objective:** Practice iterating over characters with `chars()` instead of reaching for an index.

**Instructions:** Write a function `count_vowels(s: &str) -> usize` that returns how many vowels (`a`, `e`, `i`, `o`, `u`, case-insensitive) the string contains. It should satisfy:

```rust
fn count_vowels(s: &str) -> usize {
    // TODO: iterate with .chars() and count the vowels
    /* ??? */
}

fn main() {
    assert_eq!(count_vowels("Hello"), 2);
    assert_eq!(count_vowels("rhythm"), 0);
    assert_eq!(count_vowels("AEIOU"), 5);
    println!("all good");
}
```

<details>
<summary>Solution</summary>

```rust
fn count_vowels(s: &str) -> usize {
    s.chars()
        .filter(|c| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u'))
        .count()
}

fn main() {
    assert_eq!(count_vowels("Hello"), 2);
    assert_eq!(count_vowels("rhythm"), 0);
    assert_eq!(count_vowels("AEIOU"), 5);
    println!("all good");
}
```

`chars()` yields each Unicode character; `matches!` is a compact pattern test, and `count()` consumes the filtered iterator. (Iterator adaptors like `filter` are covered in [Iterators](/07-collections/06-iterators/).)

</details>

### Exercise 2: A non-panicking prefix

**Difficulty:** Intermediate

**Objective:** Slice by character count without ever panicking on a multi-byte boundary.

**Instructions:** Write `safe_prefix(s: &str, n: usize) -> Option<&str>` that returns the first `n` *characters* as a borrowed `&str`. If the string has fewer than `n` characters, return the whole string. The result must always be a valid `&str` (never a panic), so use `char_indices()` to find a real boundary and `.get(..)` for the slice.

```rust
fn safe_prefix(s: &str, n: usize) -> Option<&str> {
    // TODO
    /* ??? */
}

fn main() {
    assert_eq!(safe_prefix("café", 3), Some("caf"));
    assert_eq!(safe_prefix("café", 4), Some("café"));
    assert_eq!(safe_prefix("café", 10), Some("café"));
    assert_eq!(safe_prefix("\u{1F980}x", 1), Some("\u{1F980}"));
    println!("all good");
}
```

<details>
<summary>Solution</summary>

```rust
fn safe_prefix(s: &str, n: usize) -> Option<&str> {
    match s.char_indices().nth(n) {
        // The nth char's start byte is a guaranteed boundary.
        Some((byte_idx, _)) => s.get(..byte_idx),
        None => Some(s), // fewer than n chars: the whole string
    }
}

fn main() {
    assert_eq!(safe_prefix("café", 3), Some("caf"));
    assert_eq!(safe_prefix("café", 4), Some("café"));
    assert_eq!(safe_prefix("café", 10), Some("café"));
    assert_eq!(safe_prefix("\u{1F980}x", 1), Some("\u{1F980}"));
    println!("all good");
}
```

`char_indices().nth(n)` gives the byte offset where the (n+1)-th character starts — always a char boundary — and `.get(..byte_idx)` slices safely. If `nth(n)` is `None`, the string was shorter than `n` chars, so we return all of it.

</details>

### Exercise 3: Title-case a sentence

**Difficulty:** Advanced

**Objective:** Build a new `String` from borrowed input, distinguishing where you own versus borrow.

**Instructions:** Write `title_case(input: &str) -> String` that uppercases the first character of each whitespace-separated word and lowercases the rest, then joins the words with a single space. Collapse runs of whitespace (use `split_whitespace`).

```rust
fn title_case(input: &str) -> String {
    // TODO
    /* ??? */
}

fn main() {
    assert_eq!(title_case("hello world"), "Hello World");
    assert_eq!(title_case("  the QUICK brown  "), "The Quick Brown");
    assert_eq!(title_case(""), "");
    println!("all good");
}
```

<details>
<summary>Solution</summary>

```rust
fn title_case(input: &str) -> String {
    input
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    // Uppercase the first char, lowercase the remaining slice.
                    first.to_uppercase().collect::<String>()
                        + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
    assert_eq!(title_case("hello world"), "Hello World");
    assert_eq!(title_case("  the QUICK brown  "), "The Quick Brown");
    assert_eq!(title_case(""), "");
    println!("all good");
}
```

`split_whitespace()` borrows views into `input` (no allocation) and conveniently skips leading/trailing/duplicate whitespace. Each `map` closure builds an owned `String` per word — note `first.to_uppercase()` returns an iterator (one character can uppercase to several, like `ß`), so we `collect` it into a `String`. `chars.as_str()` hands back the unconsumed remainder as a `&str`. (`collect` and friends are explored in [Iterator Consumers](/07-collections/07-iterator-consumers/).)

</details>

---

_Next: [String manipulation methods →](/07-collections/02-string-manipulation/)_
