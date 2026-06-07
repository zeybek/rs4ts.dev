---
title: "Regular Expressions with the `regex` Crate"
description: "Rust has no regex literal: the regex crate is a compiled value you build once and reuse, with no backtracking and no ReDoS, unlike JavaScript's RegExp."
---

In JavaScript, regular expressions are a built-in language feature: `/\d+/` is valid syntax and `RegExp` is always available. Rust has no regex literal and no regex in the standard library; instead, the community-standard [`regex`](https://docs.rs/regex) crate provides a fast, predictable engine that you add as a dependency.

---

## Quick Overview

The `regex` crate gives you Unicode-aware pattern matching that runs in **linear time**. It deliberately omits backtracking features (lookahead, backreferences) to guarantee it can never blow up on adversarial input. For a TypeScript/JavaScript developer the two big mental shifts are: regexes are **compiled values you build once and reuse** (not throwaway literals), and the engine **cannot suffer catastrophic backtracking (ReDoS)** because it is built on finite automata.

---

## TypeScript/JavaScript Example

```typescript
// JavaScript/TypeScript - regex is a language built-in

// A literal compiles when the script is parsed.
const DATE_RE = /(\d{4})-(\d{2})-(\d{2})/;

function parseDate(input: string): { year: string; month: string; day: string } | null {
  const m = DATE_RE.exec(input);
  if (m === null) return null;
  // Numbered groups via array indices, or named groups via .groups
  return { year: m[1], month: m[2], day: m[3] };
}

console.log(parseDate("2026-06-02")); // { year: '2026', month: '06', day: '02' }

// Find all matches with the /g flag
const emails = "ping a@b.com or c@d.org".match(/\w+@\w+\.\w+/g);
console.log(emails); // [ 'a@b.com', 'c@d.org' ]

// Replace with a backreference in the template
const masked = "a@b.com".replace(/(\w+)@(\w+\.\w+)/, "***@$2");
console.log(masked); // ***@b.com

// DANGER: a literal is cheap to *write*, but `new RegExp(str)` inside a
// hot loop recompiles every iteration — and some patterns can ReDoS.
const evil = /(a+)+$/; // catastrophic backtracking on "aaaa...X"
```

**Key points:**

- Regex is built into the language; no import, no dependency.
- A `/.../ ` literal is compiled once; `new RegExp(userInput)` compiles each time it runs.
- The engine is backtracking-based, so lookahead and backreferences work, but pathological patterns can hang the event loop (ReDoS).

---

## Rust Equivalent

First add the crate to a project created with `cargo new` (which selects the current stable toolchain, Rust 1.96.0 on the 2024 edition, automatically):

```bash
cargo add regex
```

```toml
# Cargo.toml
[dependencies]
regex = "1.12"
```

```rust
use regex::Regex;
use std::sync::LazyLock;

// Compile ONCE for the whole program. `LazyLock` builds the Regex the first
// time it is touched, then hands out the same value forever after.
static DATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})").unwrap());

fn parse_date(input: &str) -> Option<(String, String, String)> {
    let caps = DATE_RE.captures(input)?;
    Some((
        caps["year"].to_string(),
        caps["month"].to_string(),
        caps["day"].to_string(),
    ))
}

fn main() {
    println!("{:?}", parse_date("2026-06-02"));
    // Some(("2026", "06", "02"))

    // Find all matches (the equivalent of the /g flag).
    let email_re = Regex::new(r"\w+@\w+\.\w+").unwrap();
    let emails: Vec<&str> = email_re
        .find_iter("ping a@b.com or c@d.org")
        .map(|m| m.as_str())
        .collect();
    println!("{emails:?}"); // ["a@b.com", "c@d.org"]

    // Replace with a captured group via $2 (or ${2} when ambiguous).
    let mask_re = Regex::new(r"(\w+)@(\w+\.\w+)").unwrap();
    let masked = mask_re.replace("a@b.com", "***@$2");
    println!("{masked}"); // ***@b.com
}
```

Running this prints exactly:

```text
Some(("2026", "06", "02"))
["a@b.com", "c@d.org"]
***@b.com
```

> **Note:** Patterns use **raw string literals** (`r"..."`) so that backslashes
> are passed to the regex engine verbatim. Without the `r`, `"\d"` would be a
> Rust string-escape error. See [Basic Types](/02-basics/01-types/) for
> string literal forms.

---

## Detailed Explanation

**No literal syntax.** Rust has no `/.../ ` token. A regex is an ordinary value of type `Regex`, constructed by parsing a pattern string with `Regex::new`. Parsing can fail (the pattern might be malformed), so `Regex::new` returns `Result<Regex, regex::Error>`. That is why the examples call `.unwrap()` on a known-good literal pattern.

**Compile once, match many.** Building a `Regex` is relatively expensive: the crate parses the pattern and constructs the finite-automata it will execute. Matching against that compiled value is cheap. The idiom is therefore to construct the `Regex` exactly once and reuse it. The `static ... LazyLock<Regex>` block does precisely that: the closure runs on first access and the resulting `Regex` lives for the rest of the program. `LazyLock` has been in the standard library since Rust 1.80, so you no longer need the `once_cell` or `lazy_static` crates for this. (See [Other Essential Crates](/23-ecosystem/10-useful-crates/) for where `once_cell` still earns its keep.)

**Captures.** `re.captures(text)` returns `Option<Captures>`: `None` when nothing matched, mirroring JavaScript's `RegExp.exec` returning `null`. Unlike JavaScript, indexing a `Captures` is type-directed:

- `&caps[1]` or `&caps["year"]` → `&str`, and **panics** if that group did not participate in the match.
- `caps.get(1)` or `caps.name("year")` → `Option<Match>`, the safe form for optional groups. A `Match` carries `.as_str()`, `.start()`, and `.end()` (byte offsets).

```rust
use regex::Regex;

fn main() {
    let re = Regex::new(r"(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})").unwrap();
    let caps = re.captures("2026-06-02").unwrap();
    println!("year={} month={} day={}", &caps["year"], &caps["month"], &caps["day"]);
    println!("group 1 = {}", caps.get(1).unwrap().as_str());
}
```

This prints:

```text
year=2026 month=06 day=02
group 1 = 2026
```

**Iterating matches.** Instead of a `/g` flag, you pick the method:

| You want | Method | Yields |
| --- | --- | --- |
| Does it match anywhere? | `is_match` | `bool` |
| First match | `find` | `Option<Match>` |
| All matches | `find_iter` | iterator of `Match` |
| First match with groups | `captures` | `Option<Captures>` |
| All matches with groups | `captures_iter` | iterator of `Captures` |
| Substitute | `replace` / `replace_all` | `Cow<str>` |
| Split on the pattern | `split` | iterator of `&str` |

```rust
use regex::Regex;

fn main() {
    let re = Regex::new(r"(?<key>\w+)=(?<val>\d+)").unwrap();
    for caps in re.captures_iter("a=1 b=22 c=333") {
        println!("{} -> {}", &caps["key"], &caps["val"]);
    }
}
```

Output:

```text
a -> 1
b -> 22
c -> 333
```

**The no-backtracking guarantee.** The crate is built on finite automata, not a recursive backtracking matcher. The upside is a hard worst-case bound: matching is **O(m × n)** in the length of the pattern and the input, with no exponential cliff. The downside is that features which *require* backtracking are simply not in the syntax: **lookahead/lookbehind** (`(?=...)`, `(?<=...)`) and **backreferences** (`\1`, `\k<name>`). This is the single biggest behavioral difference from JavaScript's `RegExp`. The classic ReDoS pattern below is harmless here; it runs in linear time:

```rust
use regex::Regex;

fn main() {
    let re = Regex::new(r"(a+)+$").unwrap();
    let evil = "a".repeat(40) + "X"; // the input that hangs a backtracking engine
    println!("catastrophic? {}", re.is_match(&evil));
}
```

Output (returns immediately, no hang):

```text
catastrophic? false
```

The same `(a+)+$` against `"aaaa…X"` can freeze a JavaScript engine for seconds or minutes. Rust's engine answers instantly because it never backtracks.

---

## Key Differences

| Aspect | JavaScript/TypeScript `RegExp` | Rust `regex` crate |
| --- | --- | --- |
| Availability | Built into the language | External crate (`cargo add regex`) |
| Literal syntax | `/pattern/flags` | None — `Regex::new(r"pattern")` |
| Construction cost | Hidden; recompiles with `new RegExp` | Explicit; compile once, reuse |
| Failure on bad pattern | Throws `SyntaxError` at runtime | `Result` from `Regex::new` |
| No-match result | `null` (exec) / `false` (test) | `None` / `false` |
| Lookahead / lookbehind | Supported | **Not supported** |
| Backreferences | Supported | **Not supported** |
| Worst-case time | Exponential (ReDoS possible) | Linear, guaranteed no ReDoS |
| Unicode | Per-flag (`u`, `v`) | Unicode-aware by default |
| Global iteration | `/g` flag + `matchAll` | `find_iter` / `captures_iter` |
| Case-insensitive | `/i` flag | `(?i)` inline flag |

The trade-off is deliberate: by dropping the two backtracking-only features, the crate buys a guarantee you cannot get from `RegExp`: that an attacker who controls the input string can never make your matcher hang. When you genuinely need lookahead, backreferences, or a recursive grammar, reach for a real parser instead (see [Parsing](/23-ecosystem/09-parsing/)).

---

## Common Pitfalls

### Pitfall 1: Compiling the regex inside a hot function

The most common performance mistake is constructing the `Regex` on every call, paying the compilation cost repeatedly:

```rust
use regex::Regex;

fn is_valid_slug(s: &str) -> bool {
    // Recompiles the pattern on EVERY call — slow in a loop.
    let re = Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap();
    re.is_match(s)
}
```

This compiles and runs correctly, but in a loop over thousands of inputs it can be **orders of magnitude** slower than compiling once. The crate's own documentation calls this out explicitly. The fix is a `static LazyLock<Regex>`:

```rust
use regex::Regex;
use std::sync::LazyLock;

fn is_valid_slug(s: &str) -> bool {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap());
    RE.is_match(s)
}
```

This is the same instinct a JavaScript developer already has — hoist a `new RegExp(...)` out of a loop — made structural by the type system.

### Pitfall 2: Expecting lookahead or backreferences to work

Porting a JavaScript pattern that uses `(?=...)` or `\1` does not fail to compile in Rust. It fails when `Regex::new` *runs*, because the pattern string is rejected at parse time. With a lookahead:

```rust
use regex::Regex;

fn main() {
    let result = Regex::new(r"foo(?=bar)"); // lookahead
    match result {
        Ok(_) => println!("compiled"),
        Err(e) => println!("ERROR:\n{e}"),
    }
}
```

The real error printed is:

```text
ERROR:
regex parse error:
    foo(?=bar)
       ^^^
error: look-around, including look-ahead and look-behind, is not supported
```

A backreference fails the same way:

```rust
use regex::Regex;

fn main() {
    if let Err(e) = Regex::new(r"(\w+)\s\1") {
        println!("{e}");
    }
}
```

Real output:

```text
regex parse error:
    (\w+)\s\1
           ^^
error: backreferences are not supported
```

> **Warning:** Because the failure is a runtime `Err`, calling `.unwrap()` on
> such a pattern compiles fine and then **panics** at the first execution.
> Validate patterns you build from non-literal strings instead of unwrapping.

### Pitfall 3: Panicking on an optional capture group

`&caps[1]` panics if group 1 did not participate in the match. When a group sits behind `?` or `|` and might be absent, use the `Option`-returning form:

```rust
use regex::Regex;

fn main() {
    let re = Regex::new(r"(\d+)(?:px)?").unwrap();
    let caps = re.captures("16").unwrap();

    // Fine here, but `&caps[2]` would panic — there is no group 2 at all.
    // The safe pattern for optional groups:
    match caps.get(1) {
        Some(m) => println!("number = {}", m.as_str()),
        None => println!("no number"),
    }
}
```

### Pitfall 4: Forgetting raw strings

Writing `Regex::new("\d+")` is not a regex bug; it is a *Rust string* bug. `\d` is not a valid Rust escape, so the file will not compile (`unknown character escape: d`). Always use a raw string literal `r"\d+"` for patterns. For a pattern that itself contains a quote, use `r#"..."#`.

---

## Best Practices

- **Compile once.** Store each pattern in a `static LazyLock<Regex>` (or build it once at startup and pass it around). Never compile inside a hot path.
- **Always use raw strings.** `r"..."` keeps backslashes literal; `r#"..."#` when the pattern contains `"`.
- **Prefer named captures.** `(?<name>...)` and `&caps["name"]` survive refactors that reorder groups; numbered indices do not.
- **Use non-capturing groups** `(?:...)` when you only need grouping, not extraction; it is clearer and slightly cheaper.
- **Reach for the right method.** Use `is_match` when you only need a yes/no answer; it can stop at the first match and skip building `Captures`.
- **Validate untrusted patterns.** If a pattern comes from user input or config, handle the `Result` from `Regex::new` rather than `.unwrap()`. Optionally cap input or pattern size with `RegexBuilder::size_limit`.
- **Don't reach for regex when you need a grammar.** Nested or recursive structure (JSON, source code, balanced brackets) is a parser's job; see [Parsing](/23-ecosystem/09-parsing/).
- **Use the `bytes` API for non-UTF-8 data.** `regex::bytes::Regex` matches over `&[u8]` when the input is not guaranteed valid UTF-8.

---

## Real-World Example

Parsing an NGINX/Apache-style access log line — a job you might do with `String.prototype.match` in Node — using a single compiled, commented (`(?x)` verbose-mode) pattern and named captures:

```rust
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
struct LogLine {
    ip: String,
    method: String,
    path: String,
    status: u16,
}

// `(?x)` enables verbose mode: insignificant whitespace and `#` comments are
// ignored, so a complex pattern stays readable.
static LOG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)
        ^(?<ip>\d{1,3}(?:\.\d{1,3}){3})   # client IP
        \s-\s-\s
        \[[^\]]+\]\s                       # timestamp (ignored)
        "(?<method>[A-Z]+)\s
        (?<path>\S+)\s
        HTTP/[\d.]+"\s
        (?<status>\d{3})                   # status code
        "#,
    )
    .unwrap()
});

fn parse_line(line: &str) -> Option<LogLine> {
    let caps = LOG_RE.captures(line)?;
    Some(LogLine {
        ip: caps["ip"].to_string(),
        method: caps["method"].to_string(),
        path: caps["path"].to_string(),
        status: caps["status"].parse().ok()?, // group is \d{3}, parse cannot overflow u16
    })
}

fn main() {
    let line = r#"192.168.1.10 - - [02/Jun/2026:09:15:32 +0000] "GET /api/users HTTP/1.1" 200"#;
    match parse_line(line) {
        Some(entry) => println!("{entry:?}"),
        None => println!("no match"),
    }
}
```

Real output:

```text
LogLine { ip: "192.168.1.10", method: "GET", path: "/api/users", status: 200 }
```

Two things to notice. The pattern is compiled exactly once for the whole process, so parsing a million log lines pays the build cost a single time. And the named groups feed straight into typed struct fields: `status` is parsed into a `u16`, turning a stringly-typed regex match into structured data the rest of your program can rely on (see [Structs](/06-data-structures/00-structs/)).

---

## Further Reading

- [`regex` crate documentation](https://docs.rs/regex) — the canonical reference, including the full supported syntax.
- [`regex` syntax reference](https://docs.rs/regex/latest/regex/#syntax) — every supported metacharacter and flag.
- [`RegexBuilder`](https://docs.rs/regex/latest/regex/struct.RegexBuilder.html) — case-insensitivity, size limits, and multi-line toggles set programmatically.
- [`regex::bytes`](https://docs.rs/regex/latest/regex/bytes/index.html) — matching over raw bytes instead of `&str`.
- ["Why is Rust's regex crate so fast?"](https://blog.burntsushi.net/regex-internals/) — the finite-automata design behind the linear-time guarantee.
- [Parsing](/23-ecosystem/09-parsing/) — when a grammar (nom, pest) is the right tool instead of a regex.
- [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/) — where `regex` sits among the most-used crates.
- [Other Essential Crates](/23-ecosystem/10-useful-crates/) — `once_cell`/`LazyLock` and other glue crates.
- [Basic Types](/02-basics/01-types/) — Rust string and raw-string literals.
- [Understanding Cargo](/01-getting-started/03-cargo-basics/) — adding and managing dependencies.
- [Tooling](/24-tooling/) — tooling that complements the ecosystem crates.

---

## Exercises

### Exercise 1: Hex color validator

**Difficulty:** Beginner

**Objective:** Practice compiling a pattern once and using `is_match`.

**Instructions:** Write `fn is_hex_color(s: &str) -> bool` that returns `true` for strings like `#fff` or `#ffaa00` (a `#` followed by exactly 3 or 6 hexadecimal digits) and `false` otherwise. Compile the `Regex` exactly once with `LazyLock`. Verify that `#ggg` is rejected.

<details>
<summary>Solution</summary>

```rust
use regex::Regex;
use std::sync::LazyLock;

static HEX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$").unwrap());

fn is_hex_color(s: &str) -> bool {
    HEX_RE.is_match(s)
}

fn main() {
    println!("{}", is_hex_color("#fff"));     // true
    println!("{}", is_hex_color("#ffaa00"));  // true
    println!("{}", is_hex_color("#ggg"));     // false
}
```

Output:

```text
true
true
false
```

The `^...$` anchors prevent partial matches, and `(?:...)` groups the two
alternatives without creating a capture group.

</details>

### Exercise 2: Extract unique hashtags

**Difficulty:** Intermediate

**Objective:** Use `captures_iter` and a numbered capture group, and deduplicate while preserving order.

**Instructions:** Write `fn hashtags(text: &str) -> Vec<String>` that finds every `#word` token, lowercases it, and returns the tags in first-seen order with duplicates removed. For input `"Loving #Rust and #rust and #WASM"` the result should be `["rust", "wasm"]`.

<details>
<summary>Solution</summary>

```rust
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

fn hashtags(text: &str) -> Vec<String> {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#(\w+)").unwrap());
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for caps in RE.captures_iter(text) {
        let tag = caps[1].to_lowercase();
        if seen.insert(tag.clone()) {
            out.push(tag);
        }
    }
    out
}

fn main() {
    println!("{:?}", hashtags("Loving #Rust and #rust and #WASM"));
}
```

Output:

```text
["rust", "wasm"]
```

`HashSet::insert` returns `false` when the value was already present, which is
exactly the "skip duplicates" test. The `Vec` keeps insertion order.

</details>

### Exercise 3: Redact phone numbers with a replacement closure

**Difficulty:** Advanced

**Objective:** Use `replace_all` with a closure that inspects each match, rather than a static template string.

**Instructions:** Write `fn redact(text: &str) -> String` that finds US-style phone numbers of the form `NNN-NNN-NNNN` (anchored on word boundaries) and rewrites each to keep only the area code, replacing the rest with asterisks: e.g. `415-555-0199` becomes `415-***-****`. Pass a closure to `replace_all` so you can build the replacement from the captured area code.

<details>
<summary>Solution</summary>

```rust
use regex::{Captures, Regex};
use std::sync::LazyLock;

fn redact(text: &str) -> String {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b(\d{3})-(\d{3})-(\d{4})\b").unwrap());
    RE.replace_all(text, |caps: &Captures| {
        format!("{}-***-****", &caps[1])
    })
    .into_owned()
}

fn main() {
    println!("{}", redact("Call 415-555-0199 or 212-555-0188"));
}
```

Output:

```text
Call 415-***-**** or 212-***-****
```

`replace_all` accepts anything implementing the `Replacer` trait, including a
closure `Fn(&Captures) -> String`. It returns a `Cow<str>` (borrowed when
nothing matched, owned otherwise); `.into_owned()` yields a plain `String`.

</details>
