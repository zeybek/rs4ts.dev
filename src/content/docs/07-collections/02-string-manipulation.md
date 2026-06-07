---
title: "String Manipulation"
description: "Split, trim, replace, search, and parse text in Rust: a translation table from JavaScript's String.prototype, where many methods return lazy iterators and"
---

Splitting, trimming, replacing, searching, parsing, and building text are everyday tasks. In TypeScript these all live on the `String.prototype`; in Rust they are split across `&str` methods, the `String` type, and the iterator system. This page is your translation table.

---

## Quick Overview

Most of what you do with JavaScript's `string` methods (`.split()`, `.trim()`, `.replace()`, `.toUpperCase()`, `parseInt`, building up text) has a direct Rust counterpart on `&str` and `String`. The two big surprises for a TypeScript developer: many "transforming" methods return **iterators** (lazy, zero-allocation) instead of arrays, and you **cannot index a string by integer** (`s[0]` does not compile) because Rust strings are UTF-8 byte sequences, not arrays of characters.

> **Note:** This page focuses on *operations*. For the foundational distinction between `String` (owned, growable) and `&str` (a borrowed view), and why slicing happens on byte boundaries, read [Strings](/07-collections/01-strings/) first.

---

## TypeScript/JavaScript Example

```typescript
// A small log-line processor — the kind of code you write all the time.
function parseLogLine(line: string): { level: string; message: string } {
  const trimmed = line.trim();
  const level = trimmed.split(/\s+/)[1] ?? "UNKNOWN";
  const message = trimmed.replace("ERROR", "ERR");
  return { level, message };
}

// Splitting, joining, searching, casing
const csv = "name,age,city";
const fields = csv.split(",");                 // ["name", "age", "city"]
const rejoined = fields.join(" | ");           // "name | age | city"

const greeting = "Hello, World";
greeting.toUpperCase();                          // "HELLO, WORLD"
greeting.includes("World");                      // true
greeting.startsWith("Hello");                    // true
greeting.indexOf("o");                           // 4

// Parsing numbers
const n = parseInt("42", 10);                    // 42
const f = parseFloat("3.14");                    // 3.14
const bad = Number("not a number");              // NaN (no error!)

// Building strings
let buf = "";
for (const w of ["a", "b", "c"]) buf += w;       // "abc"
const bio = `${"Ada"} is ${36}`;                 // template literal

// Iterating characters
for (const ch of "héllo") console.log(ch);       // h é l l o (code points)
"hello".length;                                  // 5
"héllo".length;                                  // 5 (UTF-16 code units!)
```

Every method above is on the single `string` type, and indexing (`csv[0]`) returns a one-character string. JavaScript hides the underlying UTF-16 representation almost entirely, until you hit emoji or surrogate pairs.

---

## Rust Equivalent

```rust
fn main() {
    // The same log-line processor.
    let line = "  2026-05-31 ERROR  Disk full on /dev/sda1  ";

    let trimmed = line.trim();                                  // &str view, no allocation
    let level = trimmed.split_whitespace().nth(1).unwrap_or("UNKNOWN");
    let message = trimmed.replace("ERROR", "ERR");              // allocates a new String
    println!("level={level} message={message}");

    // Splitting, joining, searching, casing
    let csv = "name,age,city";
    let fields: Vec<&str> = csv.split(',').collect();           // collect the iterator
    let rejoined = fields.join(" | ");
    println!("{rejoined}");

    let greeting = "Hello, World";
    println!("{}", greeting.to_uppercase());                   // "HELLO, WORLD" (new String)
    println!("{}", greeting.contains("World"));                // true
    println!("{}", greeting.starts_with("Hello"));             // true
    println!("{:?}", greeting.find('o'));                      // Some(4)  (byte index)

    // Parsing numbers — returns Result, never silently NaN
    let n: i32 = "42".parse().unwrap();                        // 42
    let f: f64 = "3.14".parse().unwrap();                      // 3.14
    let bad: Result<i32, _> = "not a number".parse();          // Err(...)
    println!("{n} {f} {}", bad.is_err());

    // Building strings
    let mut buf = String::new();
    for w in ["a", "b", "c"] { buf.push_str(w); }              // "abc"
    let bio = format!("{} is {}", "Ada", 36);
    println!("{buf} / {bio}");

    // Iterating characters (Unicode scalar values, not bytes)
    for ch in "héllo".chars() { print!("{ch} "); }
    println!();
    println!("{}", "héllo".len());                             // 6 (BYTES, not chars!)
    println!("{}", "héllo".chars().count());                   // 5 (characters)
}
```

```text
level=ERROR message=2026-05-31 ERR  Disk full on /dev/sda1
name | age | city
HELLO, WORLD
true
true
Some(4)
42 3.14 true
abc / Ada is 36
h é l l o
6
5
```

---

## Detailed Explanation

### `trim` returns a borrowed slice, `replace` allocates

`line.trim()` returns a `&str` that points *into* the original `line`. No new memory is allocated, it just narrows the start/end. By contrast `trimmed.replace("ERROR", "ERR")` must build a brand-new `String`, because the result has different contents than the input. This split is a recurring theme: **methods that only narrow or scan return borrowed `&str`; methods that produce different text return an owned `String`.** TypeScript hides this distinction because every string is heap-allocated and immutable.

### Splitting produces a lazy iterator, not an array

`csv.split(',')` does **not** give you a `Vec`. It returns a `Split` iterator that yields each piece on demand. That is why we wrote `let fields: Vec<&str> = csv.split(',').collect();`: `.collect()` runs the iterator and gathers the results. If you only need to walk the pieces once, skip the `Vec` entirely and iterate directly:

```rust
fn main() {
    for field in "name,age,city".split(',') {
        println!("- {field}");
    }
}
```

This laziness mirrors how Rust treats all sequence transformations; see [Iterators](/07-collections/06-iterators/).

### `find` returns a *byte* index wrapped in `Option`

JavaScript's `indexOf` returns `-1` when not found. Rust's `find` returns `Option<usize>` (`Some(index)` or `None`), so the "not found" case is part of the type and the compiler forces you to handle it. The number it returns is a **byte offset**, which matters for non-ASCII text (more below).

### `parse` returns `Result`, never a silent `NaN`

`"not a number"` in JavaScript yields `NaN`, a value that silently poisons later arithmetic. Rust's `.parse()` returns `Result<T, E>` and you must deal with the error. The target type drives the parsing: `let n: i32 = "42".parse()...` or the turbofish form `"42".parse::<i32>()`. Error handling for `Result` is covered in [Error Handling](/08-error-handling/).

### `chars()` vs `bytes()` vs `len()`

A Rust string is UTF-8 bytes. `"héllo".len()` is `6` because `é` takes two bytes. To count **characters** (Unicode scalar values) use `.chars().count()`, which is `5`. JavaScript's `.length` counts UTF-16 code units, so it gives `5` here but `2` for a single emoji like `"\u{1F600}".length` (a surrogate pair). None of the three languages' counts agree in general; be explicit about whether you want bytes or characters.

---

## Key Differences

| Task | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| Split | `s.split(",")` → array | `s.split(',')` → lazy iterator (`.collect()` for `Vec`) |
| Split on whitespace | `s.split(/\s+/)` (regex) | `s.split_whitespace()` (built-in, no regex) |
| Split into lines | `s.split("\n")` | `s.lines()` (handles `\n` and `\r\n`) |
| Trim | `s.trim()` | `s.trim()` → borrowed `&str` |
| Replace all | `s.replaceAll("a","b")` | `s.replace("a", "b")` (all by default) |
| Replace first | `s.replace("a","b")` | `s.replacen("a", "b", 1)` |
| Uppercase | `s.toUpperCase()` | `s.to_uppercase()` → new `String` |
| Contains | `s.includes(x)` | `s.contains(x)` |
| Index of | `s.indexOf(x)` → `-1` if absent | `s.find(x)` → `Option<usize>` (byte index) |
| Parse int | `parseInt(s)` → `NaN` on fail | `s.parse::<i32>()` → `Result` |
| Char access | `s[0]` → 1-char string | `s.chars().nth(0)` → `Option<char>` |
| Length | `s.length` (UTF-16 units) | `s.len()` (bytes) / `.chars().count()` (chars) |
| Concatenate | `a + b`, `` `${a}${b}` `` | `format!("{a}{b}")`, `a.push_str(b)`, `a + &b` |
| Join | `arr.join(", ")` | `arr.join(", ")` |
| Repeat | `s.repeat(3)` | `s.repeat(3)` |

> **Tip:** When a JavaScript method returns an array (`split`, `match`), the Rust analog almost always returns an **iterator**. Add `.collect::<Vec<_>>()` only when you actually need a collection.

### Why no integer indexing?

`s[0]` is rejected at compile time in Rust. UTF-8 means byte `0` might be only *half* of a character, so returning "the first character" by byte index would be a footgun. Rust makes you choose your intent: `.chars().next()` for the first character, `.bytes().next()` for the first byte, or `&s[0..n]` for a byte-range slice (which panics if the range falls inside a character).

---

## Common Pitfalls

### Pitfall 1: Trying to index a string by integer

```rust
fn main() {
    let s = String::from("hello");
    let first = s[0]; // does not compile (error E0277: `str` cannot be indexed by `{integer}`)
    println!("{first}");
}
```

Real compiler error:

```text
error[E0277]: the type `str` cannot be indexed by `{integer}`
 --> src/main.rs:3:19
  |
3 |     let first = s[0];
  |                   ^ string indices are ranges of `usize`
  |
  = help: the trait `SliceIndex<str>` is not implemented for `{integer}`
  = note: you can use `.chars().nth()` or `.bytes().nth()`
```

**Fix:** Use `s.chars().next()` for the first character (returns `Option<char>`), or `&s[0..1]` for a byte slice when you know the boundary is safe.

### Pitfall 2: `parse` without a target type

```rust
fn main() {
    let n = "42".parse().unwrap(); // does not compile (error E0284: type annotations needed)
    println!("{n}");
}
```

Real compiler error:

```text
error[E0284]: type annotations needed
 --> src/main.rs:2:9
  |
2 |     let n = "42".parse().unwrap();
  |         ^        ----- type must be known at this point
  |
help: consider giving `n` an explicit type
  |
2 |     let n: /* Type */ = "42".parse().unwrap();
  |          ++++++++++++
```

`parse` is generic over *any* type that implements `FromStr`, so the compiler cannot guess. **Fix:** annotate the binding (`let n: i32 = ...`) or use the turbofish (`"42".parse::<i32>()`).

### Pitfall 3: Slicing across a character boundary

Byte-range slicing compiles fine but **panics at runtime** if the boundary cuts a multi-byte character in half:

```rust
fn main() {
    let s = "héllo";       // é is 2 bytes, so bytes are: h(0) é(1..3) l(3) l(4) o(5)
    let sub = &s[0..2];    // panics at runtime: byte 2 is inside 'é'
    println!("{sub}");
}
```

Real runtime output:

```text
thread 'main' panicked at src/main.rs:3:17:
byte index 2 is not a char boundary; it is inside 'é' (bytes 1..3) of `héllo`
```

**Fix:** iterate with `.chars()` / `.char_indices()`, or use `.get(0..2)` which returns `Option<&str>` (`None` instead of panicking).

### Pitfall 4: Using a `String` after `+` consumes it

The `+` operator takes the left operand **by value** (it moves it):

```rust
fn main() {
    let a = String::from("foo");
    let b = String::from("bar");
    let c = a + &b;          // `a` is moved into the result
    println!("{a} {c}");     // does not compile (error E0382: borrow of moved value: `a`)
}
```

Real compiler error (abridged):

```text
error[E0382]: borrow of moved value: `a`
 --> src/main.rs:5:16
  |
4 |     let c = a + &b;
  |             - value moved here
5 |     println!("{a} {c}");
  |                ^ value borrowed here after move
help: consider cloning the value if the performance cost is acceptable
```

**Fix:** prefer `format!("{a}{b}")` (borrows both, clearer intent), or clone if you genuinely need `a` afterward. Note the asymmetry: with `a + &b`, the left side must be an owned `String` and the right side must be a `&str`.

### Pitfall 5: Expecting `split` to return a `Vec`

`let parts = "a,b".split(',');` gives you an *iterator*, not a `Vec`. Calling `.len()` on it fails because iterators have no length. **Fix:** add `.collect::<Vec<_>>()` when you need indexing or a length, or call `.count()` to consume it just for the number of items.

---

## Best Practices

- **Prefer `&str` parameters.** A function that reads text should take `&str`, not `&String`; it accepts both `String` and string literals via deref coercion:

  ```rust
  fn shout(s: &str) -> String { s.to_uppercase() }

  fn main() {
      let owned = String::from("hi");
      println!("{}", shout(&owned)); // String coerces to &str
      println!("{}", shout("there")); // literal works too
  }
  ```

- **Use `format!` for readable concatenation**, `push_str`/`push` in hot loops, and `String::with_capacity(n)` when you know the rough output size to avoid reallocations (see [Collection Performance](/07-collections/09-collection-performance/)).
- **Reach for the right splitter:** `split_whitespace()` instead of a regex for whitespace; `lines()` instead of `split('\n')` so you get `\r\n` handling for free; `split_once(d)` when you expect exactly one delimiter (returns `Option<(&str, &str)>`).
- **Build strings with iterators + `collect()`** for transformations: `text.chars().filter(..).collect::<String>()` is idiomatic and often faster than manual `push` loops.
- **Be deliberate about bytes vs chars.** Use `.bytes()` / `.len()` for protocol/encoding work, `.chars()` / `.chars().count()` for user-facing text.
- **Use `strip_prefix` / `strip_suffix`** (returning `Option<&str>`) instead of manual slicing to safely peel known prefixes like `"https://"`.

---

## Real-World Example

A URL-slug generator, the kind of helper a web backend uses to turn an article title into a clean path segment. It exercises trimming, casing, character classification, filtering, splitting, and joining.

```rust
/// Turn an arbitrary title into a lowercase, hyphen-separated slug.
/// Non-alphanumeric runs collapse to a single hyphen; edges are trimmed.
fn slugify(title: &str) -> String {
    title
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()       // intermediate: "hello--world---rust---ts"
        .split('-')                // split on the hyphens we inserted
        .filter(|piece| !piece.is_empty()) // drop the empty runs
        .collect::<Vec<_>>()
        .join("-")                 // re-join with single hyphens
}

fn main() {
    println!("{}", slugify("  Hello, World!  Rust & TS  "));
    println!("{}", slugify("Café del Mar"));
}
```

```text
hello-world-rust-ts
café-del-mar
```

Notice how `.is_alphanumeric()` is Unicode-aware: `é` survives the filter and ends up in the slug, exactly as a TypeScript `Intl`-aware implementation would (and unlike a naive `[a-z0-9]` regex, which would strip it).

---

## Further Reading

- [`str` method documentation](https://doc.rust-lang.org/std/primitive.str.html) — the canonical list of every `&str` method.
- [`String` documentation](https://doc.rust-lang.org/std/string/struct.String.html) — owned, growable strings.
- [`char` documentation](https://doc.rust-lang.org/std/primitive.char.html) — `is_alphanumeric`, `to_uppercase`, etc.
- [The Book: Storing UTF-8 Text with Strings](https://doc.rust-lang.org/book/ch08-02-strings.html) — the official walkthrough.
- Related sections in this guide:
  - [Strings](/07-collections/01-strings/) — `String` vs `&str`, UTF-8, ownership, byte-boundary slicing (read this first).
  - [Vectors](/07-collections/00-vectors/) — `Vec<T>`, where `split().collect()` results land.
  - [Iterators](/07-collections/06-iterators/) and [Iterator Consumers](/07-collections/07-iterator-consumers/) — the lazy machinery behind `split`, `chars`, `map`, and `collect`.
  - [Collection Performance](/07-collections/09-collection-performance/) — when to preallocate string capacity.
  - [Error Handling](/08-error-handling/) — handling `parse`'s `Result`.
  - [Output and Formatting](/02-basics/04-output/) — `format!` and `println!` formatting in depth.

---

## Exercises

### Exercise 1: Normalize whitespace

- **Difficulty:** Easy
- **Objective:** Practice splitting and joining.
- **Instructions:** Write `fn normalize_whitespace(input: &str) -> String` that collapses every run of whitespace into a single space and trims the ends. `"  the   quick \n brown  fox "` should become `"the quick brown fox"`.

```rust
fn normalize_whitespace(input: &str) -> String {
    // TODO: split on whitespace and re-join with single spaces
    todo!()
}

fn main() {
    assert_eq!(normalize_whitespace("  the   quick \n brown  fox "), "the quick brown fox");
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust
fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn main() {
    assert_eq!(normalize_whitespace("  the   quick \n brown  fox "), "the quick brown fox");
    println!("ok");
}
```

`split_whitespace()` already drops empty pieces and handles all Unicode whitespace, so a single `join(" ")` finishes the job. Verified output: `ok`.

</details>

### Exercise 2: Case-insensitive word frequency

- **Difficulty:** Medium
- **Objective:** Combine character filtering, casing, splitting, and a `HashMap`.
- **Instructions:** Write `fn word_count(text: &str) -> HashMap<String, usize>` that counts how often each word appears, ignoring case and stripping punctuation. `"The cat sat. The CAT ran!"` should count `cat` → 2, `the` → 2, `sat` → 1, `ran` → 1.

```rust
use std::collections::HashMap;

fn word_count(text: &str) -> HashMap<String, usize> {
    // TODO: for each whitespace-separated word, strip non-alphanumeric chars,
    //       lowercase it, and bump its count
    todo!()
}

fn main() {
    let counts = word_count("The cat sat. The CAT ran!");
    assert_eq!(counts.get("cat"), Some(&2));
    assert_eq!(counts.get("the"), Some(&2));
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

fn word_count(text: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        let key: String = word
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        if !key.is_empty() {
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    counts
}

fn main() {
    let counts = word_count("The cat sat. The CAT ran!");
    assert_eq!(counts.get("cat"), Some(&2));
    assert_eq!(counts.get("the"), Some(&2));
    println!("ok");
}
```

The `entry(key).or_insert(0)` pattern is the idiomatic "increment-or-create"; see [HashMaps](/07-collections/03-hashmaps/) for the full entry API. Verified output: `ok`.

</details>

### Exercise 3: Mask a credit-card number

- **Difficulty:** Medium/Hard
- **Objective:** Practice character classification, building strings, and safe slicing of the *last* N characters.
- **Instructions:** Write `fn mask_card(card: &str) -> String` that keeps only the digits, then replaces every digit except the last four with `*`. `"4111 1111 1111 1234"` should become `"************1234"`. If there are four or fewer digits, return them unmasked.

```rust
fn mask_card(card: &str) -> String {
    // TODO: extract digits, then mask all but the last 4
    todo!()
}

fn main() {
    assert_eq!(mask_card("4111 1111 1111 1234"), "************1234");
    assert_eq!(mask_card("12-34"), "1234");
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust
fn mask_card(card: &str) -> String {
    let digits: String = card.chars().filter(|c| c.is_ascii_digit()).collect();
    let n = digits.len();
    if n <= 4 {
        return digits;
    }
    let masked = "*".repeat(n - 4);
    let last4 = &digits[n - 4..]; // safe: digits are ASCII (1 byte each)
    format!("{masked}{last4}")
}

fn main() {
    assert_eq!(mask_card("4111 1111 1111 1234"), "************1234");
    assert_eq!(mask_card("12-34"), "1234");
    println!("ok");
}
```

Because we filtered to `is_ascii_digit()`, every remaining character is exactly one byte, so the byte slice `&digits[n - 4..]` can never split a character; slicing here is safe. Verified output: `ok`.

</details>
