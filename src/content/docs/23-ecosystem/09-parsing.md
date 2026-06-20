---
title: "Parsing: nom and pest"
description: "When input nests or has a grammar, regex stops being enough. Build real parsers in Rust with nom's combinators or pest's grammar files for typed output."
---

When a string has real *structure* (nesting, recursion, balanced delimiters, or a grammar you actually want to enforce), a regex stops being the right tool. Rust's ecosystem answers this with two mature, complementary libraries: **nom**, a parser-combinator library where you build parsers from small Rust functions, and **pest**, where you write a formal grammar in a `.pest` file and a derive macro generates the parser.

---

## Quick Overview

In Node, parsing anything beyond a flat field split usually means either a pile of regular expressions or pulling in a hand-written tokenizer/AST library. Rust pushes you toward **real parsers** that are fast, allocation-light, and produce precise error positions. **nom** lets you compose parsers as ordinary functions (great when the input is byte-oriented or you want full control); **pest** lets you declare a [PEG](https://en.wikipedia.org/wiki/Parsing_expression_grammar) grammar and get a parser plus readable error messages for free. The key mental shift for a TypeScript/JavaScript developer: the moment your input can nest, reach for a parser rather than a longer regex.

> **Note:** This page is about *structured text* parsing. For deserializing known data formats (JSON, YAML, TOML) you almost always want `serde` instead. See [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/). For flat pattern matching, the `regex` crate is covered in [Regular Expressions with the `regex` Crate](/23-ecosystem/08-regex/).

---

## TypeScript/JavaScript Example

A common Node task: parse a [semantic version](https://semver.org) string like `1.2.3` or `10.0.0-rc.1` into a structured object. The "just use a regex" reflex looks fine until you actually need precise errors and an optional pre-release tag:

```typescript
// semver.ts — the typical Node approach: one big regex
interface SemVer {
  major: number;
  minor: number;
  patch: number;
  pre?: string;
}

const SEMVER = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.]+))?$/;

function parseSemVer(input: string): SemVer {
  const m = SEMVER.exec(input);
  if (!m) {
    throw new Error(`invalid semver: ${input}`);
  }
  return {
    major: Number(m[1]),
    minor: Number(m[2]),
    patch: Number(m[3]),
    pre: m[4], // undefined when the group didn't match
  };
}

console.log(parseSemVer("1.2.3"));
// { major: 1, minor: 2, patch: 3, pre: undefined }
console.log(parseSemVer("10.0.0-rc.1"));
// { major: 10, minor: 0, patch: 0, pre: 'rc.1' }
```

This works, but notice what the regex *doesn't* give you: it can tell you the whole string failed, but not *where* or *why*. And the moment requirements grow — build metadata (`+build.5`), nested optional groups, comparison operators (`>=1.2.0`) — the regex becomes a write-only liability. That growth pressure is exactly when a real parser earns its place.

---

## Rust Equivalent

Here is the same semver parser built with **nom** as a set of composable functions. Each small parser does one job; the top-level `semver` parser threads them together. Add the crate first:

```toml
# Cargo.toml
[dependencies]
nom = "8"
```

```rust playground
// src/main.rs
// cargo add nom
use nom::{
    bytes::complete::take_while1,
    character::complete::{char, digit1},
    combinator::{map_res, opt},
    sequence::{preceded, terminated},
    IResult, Parser,
};

#[derive(Debug, PartialEq)]
struct SemVer {
    major: u32,
    minor: u32,
    patch: u32,
    pre: Option<String>,
}

// Parse one run of ASCII digits into a u32.
fn u32_num(input: &str) -> IResult<&str, u32> {
    map_res(digit1, str::parse::<u32>).parse(input)
}

// Parse an optional "-rc.1" style pre-release tag (the leading '-' is consumed).
fn pre_release(input: &str) -> IResult<&str, String> {
    let (rest, s) = preceded(
        char('-'),
        take_while1(|c: char| c.is_alphanumeric() || c == '.'),
    )
    .parse(input)?;
    Ok((rest, s.to_string()))
}

fn semver(input: &str) -> IResult<&str, SemVer> {
    let (input, major) = terminated(u32_num, char('.')).parse(input)?;
    let (input, minor) = terminated(u32_num, char('.')).parse(input)?;
    let (input, patch) = u32_num.parse(input)?;
    let (input, pre) = opt(pre_release).parse(input)?;
    Ok((input, SemVer { major, minor, patch, pre }))
}

fn main() {
    println!("{:?}", semver("1.2.3"));
    println!("{:?}", semver("10.0.0-rc.1"));
    println!("{:?}", semver("1.2")); // incomplete: missing ".patch"
}
```

Real output from `cargo run`:

```text
Ok(("", SemVer { major: 1, minor: 2, patch: 3, pre: None }))
Ok(("", SemVer { major: 10, minor: 0, patch: 0, pre: Some("rc.1") }))
Err(Error(Error { input: "", code: Char }))
```

The first element of every `Ok` tuple is the *remaining* unparsed input (`""` means the whole string was consumed). The third line shows nom's superpower over a regex: instead of a flat "no match," it reports the exact parser (`Char`, the `.` it expected after `1.2`) and the position (the input remaining at that point).

---

## Detailed Explanation

The core nom type is `IResult<I, O>`, short for `Result<(I, O), nom::Err<E>>`. A parser is any function (or value) that takes input `I` and returns `IResult<I, O>`: on success, the *leftover* input plus the parsed output `O`; on failure, an error that carries where it stopped. This is the **parser-combinator** model: small parsers are values you combine into bigger ones.

- **`digit1`** matches one-or-more ASCII digits and returns the matched `&str`. It is a *primitive* parser from `nom::character::complete`. The `complete` module assumes the whole input is available (the normal case); the sibling `streaming` module is for incremental network parsing and returns `Incomplete` instead of failing at end-of-input.
- **`map_res(parser, f)`** runs `parser`, then applies a fallible function `f` to the result. Here `str::parse::<u32>` turns the matched digits into a `u32`; if parsing overflowed, nom converts that into a parse error automatically.
- **`terminated(a, b)`** runs `a`, then `b`, and keeps only `a`'s output: perfect for "a number *followed by* a dot, but I only care about the number." Its siblings are `preceded` (keep the second), `pair`/`separated_pair` (keep both), and `delimited` (keep the middle).
- **`opt(parser)`** makes a parser optional, returning `Option<O>` — exactly mirroring the optional `pre` field. This is the structured equivalent of the regex's `(?:-(...))?` group, but it composes with everything else.
- **`.parse(input)`** is the call that actually runs a parser. In nom 8 every parser implements the `Parser` trait, and `.parse(...)` is the trait method — which is why `use nom::Parser;` appears in the imports. (Earlier nom versions called parsers like plain functions; nom 8 unified everything behind the trait.)

The `semver` function itself reads top-to-bottom like the grammar it encodes: major-dot, minor-dot, patch, optional pre-release. Each `?` short-circuits on failure and propagates the precise error — the same `?` you already use for `Result` everywhere else in Rust (see [section 08](/08-error-handling/)).

Contrast with the TypeScript regex: there, the *entire* structure lived in one opaque pattern string. In nom, the structure lives in ordinary, individually-testable, individually-named Rust functions. You can unit-test `u32_num` in isolation, reuse it in three different parsers, and the compiler type-checks that each piece produces the type the next piece expects.

---

## pest: a grammar instead of functions

nom puts the grammar *in your Rust code*. **pest** takes the opposite approach: you write the grammar in a separate `.pest` file using PEG notation, and `#[derive(Parser)]` generates the parser at compile time. This is closer to tools like ANTLR or PEG.js that a Node developer might have used. Add both crates:

```toml
# Cargo.toml
[dependencies]
pest = "2"
pest_derive = "2"
```

Put the grammar in `src/ini.pest`:

```text
// src/ini.pest
WHITESPACE = _{ " " | "\t" }

section_name = @{ (ASCII_ALPHANUMERIC | "_" | ".")+ }
section      = { "[" ~ section_name ~ "]" }

key   = @{ (ASCII_ALPHANUMERIC | "_")+ }
value = @{ (!NEWLINE ~ ANY)* }
pair  = { key ~ "=" ~ value }

line = _{ section | pair }
file = { SOI ~ (line? ~ NEWLINE)* ~ line? ~ EOI }
```

Then walk the parse tree in Rust to build a config map:

```rust
// src/main.rs
// cargo add pest pest_derive
use std::collections::BTreeMap;
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "ini.pest"]
struct IniParser;

fn parse_ini(src: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut config: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current = String::from("default");

    let file = IniParser::parse(Rule::file, src).unwrap().next().unwrap();
    for item in file.into_inner() {
        match item.as_rule() {
            Rule::section => {
                current = item.into_inner().next().unwrap().as_str().to_string();
            }
            Rule::pair => {
                let mut inner = item.into_inner();
                let key = inner.next().unwrap().as_str().to_string();
                let value = inner.next().unwrap().as_str().trim().to_string();
                config.entry(current.clone()).or_default().insert(key, value);
            }
            _ => {}
        }
    }
    config
}

fn main() {
    let src = "\
[server]
host = 0.0.0.0
port = 8080

[database]
url = postgres://localhost/app
";
    let config = parse_ini(src);
    for (section, kvs) in &config {
        println!("[{section}]");
        for (k, v) in kvs {
            println!("  {k} = {v}");
        }
    }
}
```

Real output:

```text
[database]
  url = postgres://localhost/app
[server]
  host = 0.0.0.0
  port = 8080
```

A few grammar notes that map onto regex intuition:

- **`~`** is sequence ("then"), **`|`** is ordered choice ("try left, else right"), **`*`/`+`/`?`** are the familiar repetition operators, and **`!`** is negative lookahead.
- **`@{ ... }`** marks an *atomic* rule: no implicit whitespace inside, and it produces a single token rather than child nodes. Use it for terminals like identifiers and numbers.
- **`_{ ... }`** marks a *silent* rule that matches but produces no node in the tree (here `WHITESPACE` and the `line` wrapper). `WHITESPACE` is special: pest inserts it automatically between tokens of non-atomic rules.
- **`SOI`/`EOI`** are start-of-input and end-of-input anchors; including `EOI` forces the parser to consume the *whole* input, the structured analog of a regex's `^...$`.

---

## Key Differences

| Aspect | regex (`regex` crate) | nom | pest |
| --- | --- | --- | --- |
| Where the grammar lives | one pattern string | Rust functions | separate `.pest` file |
| Handles nesting/recursion | No (it is a regular language) | Yes | Yes |
| Error reporting | match / no match | parser + position | rich, line/column, "expected X" |
| Output | matched substrings/captures | typed Rust values directly | a tree of `Pair`s you walk |
| Learning curve | low | medium (combinator style) | medium (learn PEG syntax) |
| Best for | flat fields, validation | byte/binary, performance, fine control | readable grammars, languages, configs |
| Compile-time grammar check | no | type-checked Rust | pest validates the grammar at build |

> **Tip:** A useful rule of thumb: if you can describe the input as "find these fields in a line," use `regex`. If you find yourself counting brackets, tracking depth, or writing `(?:...)` inside `(?:...)`, switch to a parser. If the grammar is something *other people* will read and extend, prefer pest's separate file; if you want maximum speed and to emit typed values straight from parsing, prefer nom.

The deepest difference from the TypeScript world is that JavaScript's `RegExp` engine *backtracks*, which lets it fake limited nesting via recursion-ish tricks and backreferences, at the cost of [catastrophic backtracking](https://www.regular-expressions.info/catastrophic.html) on adversarial input. Rust's `regex` crate deliberately has **no backtracking and no backreferences** (guaranteeing linear time; see [Regular Expressions with the `regex` Crate](/23-ecosystem/08-regex/)), so it *cannot* be abused into a half-parser. That constraint is a feature: it pushes you to a real parser exactly when you should be using one.

---

## When to reach for a real parser over regex

Regex is a fine tool — until the input is genuinely a *language*. The clearest signal is **nesting**. Consider extracting balanced parentheses from `f(g(x), h(y))`. In Node:

```typescript
const s = "f(g(x), h(y))";
const naive = /\(([^()]*)\)/; // can only match a non-nested group
console.log(JSON.stringify(s.match(naive)));
// ["(x)","x"]   <-- it grabbed the innermost group, not "g(x), h(y)"
```

The regex matched `(x)` and stopped; it has no concept of depth, so it cannot return the outer group's contents. Validating arbitrary-depth balance is provably impossible with a true regular expression — it requires a stack, i.e. a parser.

The Rust `regex` crate makes this boundary explicit by *rejecting* the features people use to fake structure. Backreferences, for instance, simply do not compile:

```rust playground
// src/main.rs
// cargo add regex
use regex::Regex;

fn main() {
    // Backreferences are not supported (the crate guarantees linear-time matching).
    match Regex::new(r"(\w+)\s+\1") {
        Ok(_) => println!("compiled"),
        Err(e) => println!("ERROR: {e}"),
    }
}
```

Real output:

```text
ERROR: regex parse error:
    (\w+)\s+\1
            ^^
error: backreferences are not supported
```

So in Rust the decision is sharp. Reach for a **parser** when any of these are true:

- The input can **nest** (expressions, JSON-like data, balanced brackets, S-expressions).
- You need a precise **error position** and message, not just match/no-match.
- The grammar is **recursive** or has **operator precedence** (arithmetic, query languages).
- You want to emit **typed values** as you parse, not post-process captured strings.
- The pattern is becoming an unreadable wall of `(?:...)` groups.

Stay with **regex** when the input is flat — log fields, a date inside prose, a quick validation — and you mainly need to *find* or *check* substrings.

---

## Common Pitfalls

### A parser succeeding does not mean it consumed everything

This trips up nearly every newcomer. A nom parser can succeed while leaving trailing garbage in the leftover input. If you only check `is_ok()`, you will accept malformed input.

```rust playground
// cargo add nom
use nom::{
    character::complete::digit1,
    combinator::{all_consuming, map_res},
    IResult, Parser,
};

fn number(input: &str) -> IResult<&str, u32> {
    map_res(digit1, str::parse::<u32>).parse(input)
}

fn number_strict(input: &str) -> IResult<&str, u32> {
    all_consuming(map_res(digit1, str::parse::<u32>)).parse(input)
}

fn main() {
    println!("loose:  {:?}", number("42abc"));        // succeeds, leftover "abc"!
    println!("strict: {:?}", number_strict("42abc")); // fails as it should
    println!("ok:     {:?}", number_strict("42"));
}
```

Real output:

```text
loose:  Ok(("abc", 42))
strict: Err(Error(Error { input: "abc", code: Eof }))
ok:     Ok(("", 42))
```

The fix is to wrap your *top-level* parser in `all_consuming`, which fails unless the entire input was used. Always do this at the entry point of a complete-input parser.

### Forgetting `use nom::Parser` (the `.parse` method)

In nom 8, parsers run via the `Parser` trait's `.parse()` method. If you forget `use nom::Parser;`, you get an error like `no method named parse found`. Bring the trait into scope. The imports in the examples above all include it.

### Calling a parser binding twice without `mut`

`Parser::parse` takes `&mut self`, so a parser bound to a `let` variable must be `mut` to be reused. Prefer running combinators inline (as the examples do) so each call constructs a fresh parser, or mark reused bindings `mut`. The compiler error is the standard `cannot borrow as mutable` (`error[E0596]`).

### pest: leaving out `EOI` lets trailing junk slip through

A pest grammar only consumes what its rules describe. Without an explicit `EOI`, `IniParser::parse(Rule::file, ...)` can match a *prefix* and silently ignore the rest. Anchor the top rule with `SOI ~ ... ~ EOI` (as in the grammar above) to require the full input.

### pest: `.unwrap()` on the parse tree assumes structure that may not be there

`item.into_inner().next().unwrap()` assumes a child exists. When you change the grammar, these positional `unwrap()`s can panic on perfectly valid input. Match on `as_rule()` and handle the `None` case, or use named extraction helpers, rather than blindly indexing the tree. pest's *parse* errors, by contrast, are excellent — a missing `=` in the INI input yields:

```text
 --> 2:1
  |
2 | host 0.0.0.0
  | ^---
  |
  = expected EOI, section, or pair
```

That line/column report is something a regex will never give you, and a big reason to reach for a parser.

---

## Best Practices

- **Build bottom-up and test each piece.** Write and unit-test the smallest parsers (`u32_num`, `pre_release`) first, then compose. Small parsers are trivially testable in isolation — a major advantage over one giant regex.
- **Wrap the entry point in `all_consuming` (nom) or anchor with `SOI`/`EOI` (pest).** Make "did not consume everything" a hard error, not a silent success.
- **Return typed values, not strings, from the parser.** Use `map`/`map_res` (nom) or build your `enum`/`struct` while walking the tree (pest). Parsing and validation should produce the domain type directly.
- **Use pest's `PrattParser` for operator precedence.** Do not hand-roll precedence climbing; pest ships a Pratt parser (shown below). nom 8 has no built-in precedence combinator, so with nom you compose precedence by hand from `alt`/`many0` (or pull in a dedicated helper crate).
- **Pick the library to fit the job, not dogma.** nom shines on binary/byte protocols and hot paths; pest shines when the grammar should be human-readable and shared. Both are production-grade — nom parses Cloudflare's traffic; pest backs many language tools.
- **Don't reach for a parser when regex suffices.** A 200-line grammar to extract one field from a log line is over-engineering. Match the tool to the structure.

---

## Real-World Example

Parsing a structured access-log line into a typed record is a textbook nom job: there is light structure (quoted request, numeric fields) but no deep nesting, and you want a precise error and typed output. This is production-flavored — the kind of code you would put behind a log-ingestion pipeline.

```rust playground
// src/main.rs
// cargo add nom
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{char, digit1, space1},
    combinator::{all_consuming, map, map_res},
    sequence::{delimited, separated_pair, terminated},
    IResult, Parser,
};

#[derive(Debug, PartialEq)]
enum Method {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Debug, PartialEq)]
struct LogLine<'a> {
    ip: &'a str,
    method: Method,
    path: &'a str,
    status: u16,
    bytes: u64,
}

fn ip(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_ascii_digit() || c == '.').parse(input)
}

fn method(input: &str) -> IResult<&str, Method> {
    alt((
        map(tag("GET"), |_| Method::Get),
        map(tag("POST"), |_| Method::Post),
        map(tag("PUT"), |_| Method::Put),
        map(tag("DELETE"), |_| Method::Delete),
    ))
    .parse(input)
}

fn u16_num(input: &str) -> IResult<&str, u16> {
    map_res(digit1, str::parse::<u16>).parse(input)
}

fn u64_num(input: &str) -> IResult<&str, u64> {
    map_res(digit1, str::parse::<u64>).parse(input)
}

// 127.0.0.1 - "GET /api/users" 200 1024
fn log_line(input: &str) -> IResult<&str, LogLine<'_>> {
    let (input, ip) = terminated(ip, tag(" - ")).parse(input)?;
    let (input, (method, path)) = delimited(
        char('"'),
        separated_pair(method, space1, take_until("\"")),
        char('"'),
    )
    .parse(input)?;
    let (input, _) = space1.parse(input)?;
    let (input, (status, bytes)) = separated_pair(u16_num, space1, u64_num).parse(input)?;
    Ok((input, LogLine { ip, method, path, status, bytes }))
}

fn parse_log(line: &str) -> Result<LogLine<'_>, String> {
    all_consuming(log_line)
        .parse(line)
        .map(|(_, parsed)| parsed)
        .map_err(|e| format!("invalid log line: {e}"))
}

fn main() {
    let line = r#"127.0.0.1 - "GET /api/users" 200 1024"#;
    match parse_log(line) {
        Ok(parsed) => println!("{parsed:#?}"),
        Err(e) => eprintln!("{e}"),
    }

    println!("{:?}", parse_log("garbage"));
}
```

Real output:

```text
LogLine {
    ip: "127.0.0.1",
    method: Get,
    path: "/api/users",
    status: 200,
    bytes: 1024,
}
Err("invalid log line: Parsing Error: Error { input: \"garbage\", code: TakeWhile1 }")
```

Note the lifetime `'a` on `LogLine`: the `ip` and `path` fields borrow slices *directly out of the input string* — no allocation, no copying. That zero-copy parsing is a defining nom strength and a big reason it is fast enough for line-rate log processing. The `Method` enum, meanwhile, is produced during parsing via `map`, so downstream code gets a real type to `match` on rather than a raw string.

For the pest equivalent of "structure with precedence," here is a calculator that evaluates arithmetic with correct operator precedence and parentheses using pest's built-in `PrattParser`:

```text
// src/calc.pest
WHITESPACE = _{ " " | "\t" }

integer     = @{ ASCII_DIGIT+ }
unary_minus = { "-" }
primary     = _{ integer | "(" ~ expr ~ ")" }
atom        = _{ unary_minus? ~ primary }

bin_op   = _{ add | subtract | multiply | divide }
    add      = { "+" }
    subtract = { "-" }
    multiply = { "*" }
    divide   = { "/" }

expr = { atom ~ (bin_op ~ atom)* }
```

```rust
// src/main.rs
// cargo add pest pest_derive
use pest::iterators::Pairs;
use pest::pratt_parser::{Assoc, Op, PrattParser};
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "calc.pest"]
struct Calculator;

fn pratt() -> PrattParser<Rule> {
    PrattParser::new()
        .op(Op::infix(Rule::add, Assoc::Left) | Op::infix(Rule::subtract, Assoc::Left))
        .op(Op::infix(Rule::multiply, Assoc::Left) | Op::infix(Rule::divide, Assoc::Left))
        .op(Op::prefix(Rule::unary_minus))
}

fn eval(pairs: Pairs<Rule>, pratt: &PrattParser<Rule>) -> f64 {
    pratt
        .map_primary(|primary| match primary.as_rule() {
            Rule::integer => primary.as_str().parse::<f64>().unwrap(),
            Rule::expr => eval(primary.into_inner(), pratt), // parenthesized sub-expression
            rule => unreachable!("unexpected primary: {rule:?}"),
        })
        .map_prefix(|op, rhs| match op.as_rule() {
            Rule::unary_minus => -rhs,
            _ => unreachable!(),
        })
        .map_infix(|lhs, op, rhs| match op.as_rule() {
            Rule::add => lhs + rhs,
            Rule::subtract => lhs - rhs,
            Rule::multiply => lhs * rhs,
            Rule::divide => lhs / rhs,
            _ => unreachable!(),
        })
        .parse(pairs)
}

fn main() {
    let pratt = pratt();
    for src in ["1 + 2 * 3", "(1 + 2) * 3", "10 / 2 - 3", "-5 + 8"] {
        let expr = Calculator::parse(Rule::expr, src).unwrap().next().unwrap();
        println!("{src} = {}", eval(expr.into_inner(), &pratt));
    }
}
```

Real output:

```text
1 + 2 * 3 = 7
(1 + 2) * 3 = 9
10 / 2 - 3 = 2
-5 + 8 = 3
```

The grammar declares *what* an expression is; the `PrattParser` configuration declares precedence (multiplication binds tighter than addition) and associativity. `1 + 2 * 3 = 7` (not `9`) and `(1 + 2) * 3 = 9` prove both are handled correctly — something no regex can do, because precedence-aware evaluation fundamentally requires a recursive parse tree.

---

## Further Reading

- [nom on docs.rs](https://docs.rs/nom): the API reference for combinators (`tag`, `alt`, `delimited`, `map_res`, ...).
- [nom recipes and tutorial](https://github.com/rust-bakery/nom/blob/main/doc/): official guides, including the choosing-a-combinator cheatsheet.
- [The pest book](https://pest.rs/book/) — grammar syntax, atomic/silent rules, and the `PrattParser` chapter.
- [pest.rs editor](https://pest.rs/#editor): an interactive playground to test grammars in the browser.
- [Parsing expression grammar (PEG)](https://en.wikipedia.org/wiki/Parsing_expression_grammar): background on the formalism pest implements.
- Related guide pages: [Regular Expressions with the `regex` Crate](/23-ecosystem/08-regex/) for when a flat pattern is enough, [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/) for `serde` (parsing known data formats), and the full ecosystem map in [the section README](/23-ecosystem/).
- Foundations: [Basic Types](/02-basics/01-types/) and [Functions](/03-functions/) for the function-composition style nom relies on; [Error Handling](/08-error-handling/) for the `?`/`Result` plumbing; and [Tooling](/24-tooling/) for testing and benchmarking your parsers. New to Rust? Start at [Introduction](/00-introduction/) and [Getting Started](/01-getting-started/).

---

## Exercises

### Exercise 1: Parse environment-variable lines

**Difficulty:** Beginner

**Objective:** Use nom combinators to parse a single `KEY = value` line into a `(String, String)` pair, tolerating optional whitespace around the `=` and trimming the value.

**Instructions:** Write a function `env_line(input: &str) -> IResult<&str, (String, String)>` that parses lines like `"  DB_HOST = localhost  "` into `("DB_HOST", "localhost")` and `"PORT=8080"` into `("PORT", "8080")`. The key is alphanumeric plus underscores; the value runs to end-of-line. Trim surrounding whitespace from the value. Print the result for both inputs.

> **Tip:** `space0` matches zero-or-more spaces; `take_while1`/`take_while` match runs of characters by predicate; `delimited(space0, char('='), space0)` eats the `=` and any spaces around it.

<details>
<summary>Solution</summary>

```rust playground
// cargo add nom
use nom::{
    bytes::complete::{take_while, take_while1},
    character::complete::{char, space0},
    sequence::delimited,
    IResult, Parser,
};

fn env_line(input: &str) -> IResult<&str, (String, String)> {
    let (input, _) = space0.parse(input)?;
    let (input, k) = take_while1(|c: char| c.is_alphanumeric() || c == '_').parse(input)?;
    let (input, _) = delimited(space0, char('='), space0).parse(input)?;
    let (input, v) = take_while(|c: char| c != '\n').parse(input)?;
    Ok((input, (k.to_string(), v.trim().to_string())))
}

fn main() {
    println!("{:?}", env_line("  DB_HOST = localhost  "));
    println!("{:?}", env_line("PORT=8080"));
}
```

Real output:

```text
Ok(("", ("DB_HOST", "localhost")))
Ok(("", ("PORT", "8080")))
```

</details>

### Exercise 2: Parse a human duration into seconds

**Difficulty:** Intermediate

**Objective:** Build a nom parser for durations like `1h30m15s` that returns the total number of seconds, using `many1` to repeat a unit parser and `all_consuming` to reject trailing junk.

**Instructions:** Write `duration_secs(input: &str) -> IResult<&str, u64>` where each part is a number followed by a unit suffix `h` (3600s), `m` (60s), or `s` (1s). Sum all parts. `"1h30m15s"` should yield `5415`, `"90m"` should yield `5400`, and an invalid input like `"5x"` should produce an error. Wrap the repetition in `all_consuming` so partial matches fail.

> **Tip:** Parse one `unit_part` (a number plus a unit), then use `many1(unit_part)` to collect a `Vec<u64>` and `.into_iter().sum()` to total them.

<details>
<summary>Solution</summary>

```rust playground
// cargo add nom
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::digit1,
    combinator::{all_consuming, map, map_res},
    multi::many1,
    IResult, Parser,
};

fn unit_part(input: &str) -> IResult<&str, u64> {
    let (input, n) = map_res(digit1, str::parse::<u64>).parse(input)?;
    let (input, mult) = alt((
        map(tag("h"), |_| 3600u64),
        map(tag("m"), |_| 60u64),
        map(tag("s"), |_| 1u64),
    ))
    .parse(input)?;
    Ok((input, n * mult))
}

fn duration_secs(input: &str) -> IResult<&str, u64> {
    map(all_consuming(many1(unit_part)), |parts| parts.into_iter().sum()).parse(input)
}

fn main() {
    println!("{:?}", duration_secs("1h30m15s").map(|(_, s)| s));
    println!("{:?}", duration_secs("90m").map(|(_, s)| s));
    println!("is_err for \"5x\": {:?}", duration_secs("5x").is_err());
}
```

Real output:

```text
Ok(5415)
Ok(5400)
is_err for "5x": true
```

</details>

### Exercise 3: Parse CSV records with pest

**Difficulty:** Advanced

**Objective:** Write a small pest grammar for a comma-separated file and walk the parse tree to produce `Vec<Vec<String>>`, one inner vector per record.

**Instructions:** Create a grammar with `field`, `record`, and `file` rules anchored by `SOI`/`EOI`, where a field is any run of characters that are not a comma or newline, records are comma-separated fields, and the file is newline-separated records. Parse the input `"name,age,city\nAlice,30,NYC\nBob,25,LA"` and print each record's fields as a `Vec`.

> **Tip:** A field can be written as `{ (!("," | NEWLINE) ~ ANY)* }`. Iterate `file.into_inner()`, keep the `Rule::record` pairs, and map each record's inner pairs to `as_str().to_string()`.

<details>
<summary>Solution</summary>

Grammar (`src/csv.pest`):

```text
field    = { (!("," | NEWLINE) ~ ANY)* }
record   = { field ~ ("," ~ field)* }
file     = { SOI ~ record ~ (NEWLINE ~ record)* ~ EOI }
```

Parser (`src/main.rs`):

```rust
// cargo add pest pest_derive
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "csv.pest"]
struct CsvParser;

fn parse_csv(input: &str) -> Vec<Vec<String>> {
    let file = CsvParser::parse(Rule::file, input)
        .expect("parse failed")
        .next()
        .unwrap();

    file.into_inner()
        .filter(|p| p.as_rule() == Rule::record)
        .map(|record| {
            record
                .into_inner()
                .map(|f| f.as_str().to_string())
                .collect()
        })
        .collect()
}

fn main() {
    let input = "name,age,city\nAlice,30,NYC\nBob,25,LA";
    for record in parse_csv(input) {
        println!("{record:?}");
    }
}
```

Real output:

```text
["name", "age", "city"]
["Alice", "30", "NYC"]
["Bob", "25", "LA"]
```

> **Note:** This minimal grammar does not handle quoted fields containing commas (e.g. `"Smith, John"`). Real CSV is surprisingly subtle. For production use, reach for the dedicated `csv` crate, which is built on serde. Hand-rolling a parser is a great learning exercise but rarely worth it when a battle-tested crate exists.

</details>
