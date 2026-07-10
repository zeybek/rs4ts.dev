---
title: "Optimization Techniques: Clones, Allocations, `&str`, Iterators, and Capacity"
description: "Write fast, idiomatic Rust by avoiding clones and allocations, taking &str over String, chaining lazy iterators, and pre-sizing with with_capacity."
---

## Quick Overview

Most of Rust's everyday performance comes not from clever tricks but from *not doing wasted work*: chiefly, not allocating and copying data you only need to read. This page covers the four highest-leverage habits a TypeScript/JavaScript developer should build: avoiding needless `.clone()` calls and heap allocations, accepting `&str` instead of `String` in function arguments, leaning on lazy iterators instead of building intermediate collections, and pre-sizing growable collections with `with_capacity`. These are not micro-optimizations; they are the *default idiomatic style*, and getting them right removes whole categories of overhead that a garbage-collected runtime would otherwise hide from you, and bill you for at runtime.

> **Note:** Optimize with evidence, not vibes. Everything here is worth doing as a default coding habit, but before you go hunting for hot spots, read [When to Optimize](/21-performance/10-when-to-optimize/) and measure with the tools in [Profiling](/21-performance/00-profiling/) and [Benchmarking](/21-performance/02-benchmarking/). The timings on this page are single-run illustrations of *direction*, not benchmark-grade numbers.

---

## TypeScript/JavaScript Example

A realistic task: parse server log lines, keep only the 5xx errors, normalize their paths, and produce a small report. In JavaScript you do not think about copies or allocations at all; the engine and the garbage collector handle every string and array behind the scenes.

```typescript
interface Request {
  method: string;
  path: string;
  status: number;
}

function parseLine(line: string): Request | null {
  const [method, path, statusStr] = line.split(/\s+/);
  const status = Number(statusStr);
  if (!method || !path || Number.isNaN(status)) return null;
  return { method, path, status };
}

function normalizePath(p: string): string {
  // Always returns a (possibly new) string — even when nothing changed.
  return p.includes("//") ? p.replaceAll("//", "/") : p;
}

function serverErrors(raw: string): Request[] {
  return raw
    .split("\n")
    .map(parseLine) // allocates an array of objects
    .filter((r): r is Request => r !== null) // allocates another array
    .filter((r) => r.status >= 500) // and another
    .map((r) => ({ ...r, path: normalizePath(r.path) })); // and another
}

const raw = `GET /api//users 200
POST /api/login 401
GET /health 200
DELETE /api//cache 500`;

console.log(serverErrors(raw));
// [ { method: 'DELETE', path: '/api/cache', status: 500 } ]
```

Every `.map()`/`.filter()` builds a fresh intermediate array, every `{ ...r }` copies an object, and every string lives on the GC heap. It is correct and readable, and the cost is real but invisible. The Rust version below produces the same answer while allocating almost nothing.

---

## Rust Equivalent

The slices borrow directly from the input string, the iterator chain is a single lazy pass with no intermediate `Vec`, and a `Cow` (clone-on-write) means the path is only re-allocated when it actually needs rewriting.

```rust playground
use std::borrow::Cow;

#[derive(Debug)]
struct Request<'a> {
    method: &'a str,
    path: &'a str,
    status: u16,
}

/// Parse one line into borrowed slices of the input — zero owned `String`s.
fn parse_line(line: &str) -> Option<Request<'_>> {
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    let status: u16 = parts.next()?.parse().ok()?;
    Some(Request { method, path, status })
}

/// Only allocate a new `String` when the path actually changes.
/// Otherwise return a borrow of the input — zero allocation.
fn normalize_path(path: &str) -> Cow<'_, str> {
    if path.contains("//") {
        Cow::Owned(path.replace("//", "/")) // needs rewriting -> allocate once
    } else {
        Cow::Borrowed(path) // already clean -> just borrow
    }
}

fn main() {
    let raw = "\
GET /api//users 200
POST /api/login 401
GET /health 200
DELETE /api//cache 500";

    // One lazy pass: parse -> keep 5xx -> normalize. No intermediate Vec until
    // the final collect, and the slices borrow straight from `raw`.
    let server_errors: Vec<(&str, Cow<'_, str>, u16)> = raw
        .lines()
        .filter_map(parse_line)
        .filter(|req| req.status >= 500)
        .map(|req| (req.method, normalize_path(req.path), req.status))
        .collect();

    for (method, path, status) in &server_errors {
        println!("{status} {method} {path}");
    }
}
```

Verified output:

```text
500 DELETE /api/cache
```

The only heap allocation in the whole pipeline is the single `path.replace("//", "/")` for the one dirty path, plus the final `Vec` that holds the results. The `method` and the clean paths are borrowed slices pointing into `raw`. In the JavaScript version, *every* string and *every* intermediate array was a separate heap allocation.

---

## Detailed Explanation

### 1. `&str` over `String` in arguments

A `String` is an **owned, heap-allocated, growable** buffer (a pointer + length + capacity). A `&str` is a **borrowed view** into string data: a pointer + length, no ownership, no allocation. The single most common over-allocation a newcomer makes is taking `String` (or `&String`) as a function parameter when the function only reads the text.

Take `&str` instead. It accepts *both* owned `String`s (via automatic deref coercion) and string literals, with no copy:

```rust playground
fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn main() {
    let owned: String = String::from("one two three");
    let literal: &str = "four five";

    // &str accepts both: a String derefs to &str, and a literal is already &str.
    println!("{}", word_count(&owned));   // &String -> &str via deref coercion
    println!("{}", word_count(literal));  // already &str
    println!("{}", word_count("just a literal"));

    // `owned` is still fully usable: we only borrowed it.
    println!("still have: {owned}");
}
```

Verified output:

```text
3
2
3
still have: one two three
```

If `word_count` had taken `String`, the caller would have to either give up ownership (a move) or `clone()` (a full heap copy) for every call, and a string literal could not be passed at all without `.to_string()`. Taking `&str` makes the function maximally flexible *and* allocation-free. The same logic applies to other owned/borrowed pairs: take `&[T]` instead of `Vec<T>`, `&Path` instead of `PathBuf`. Ownership and borrowing are covered in depth in [Section 05 — Ownership](/05-ownership/); this is the performance payoff of getting them right.

### 2. Avoiding needless clones

`.clone()` on a heap type (`String`, `Vec<T>`, `HashMap`, etc.) does a **deep copy**: it allocates a new buffer and copies every byte. JavaScript has no direct equivalent because you never explicitly copy; assigning an object just copies a reference. In Rust, the borrow checker sometimes *seems* to demand a clone, and reaching for `.clone()` to make the error go away is the classic beginner reflex.

The fix is almost always to borrow instead. Compare a clone-in-loop against the same loop that borrows (release build):

```rust playground
use std::time::Instant;

fn shout_clone(words: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(words.len());
    for w in words {
        let owned: String = w.clone(); // needless: we only read it
        out.push(owned.to_uppercase());
    }
    out
}

fn shout_borrow(words: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(words.len());
    for w in words {
        out.push(w.to_uppercase()); // borrow; no clone
    }
    out
}

fn main() {
    let words: Vec<String> = (0..200_000).map(|i| format!("word{i}")).collect();

    let t = Instant::now();
    let a = shout_clone(&words);
    let clone_t = t.elapsed();

    let t = Instant::now();
    let b = shout_borrow(&words);
    let borrow_t = t.elapsed();

    assert_eq!(a.len(), b.len());
    println!("clone-in-loop : {clone_t:?}");
    println!("borrow        : {borrow_t:?}");
}
```

Verified output (single run, release build, your numbers will differ; the *direction* is the point):

```text
clone-in-loop : 7.937625ms
borrow        : 4.214458ms
```

Each `w.clone()` allocated and filled a throwaway `String` that was immediately consumed by `to_uppercase()`. Removing it roughly halved the work here. Clippy flags the most obvious cases automatically; cloning a `Copy` type is a hard "just remove it":

```rust playground
fn main() {
    let x: i32 = 42;
    let y = x.clone(); // clippy warns: clone on a Copy type is pointless
    println!("{y}");
}
```

Real Clippy output:

```text
warning: using `clone` on type `i32` which implements the `Copy` trait
 --> src/main.rs:3:13
  |
3 |     let y = x.clone();
  |             ^^^^^^^^^ help: try removing the `clone` call: `x`
  |
  = note: `#[warn(clippy::clone_on_copy)]` on by default
```

`i32` is `Copy`, so `let y = x;` already gives `y` its own independent value at zero cost; `.clone()` adds nothing. (The difference between move, copy, and clone is the subject of [move/copy/clone](/05-ownership/06-move-copy-clone/).)

### 3. `Cow` — pay only when you mutate

Sometimes a function *usually* returns its input unchanged but *occasionally* needs to produce a modified, owned version. Returning `String` every time forces an allocation even on the common no-op path. `Cow<str>` ("clone on write") lets you return a borrow when nothing changed and an owned `String` only when you actually rewrote something. That is exactly what `normalize_path` in the [Rust Equivalent](#rust-equivalent) does. It is the principled middle ground between "always borrow" (sometimes impossible) and "always clone" (often wasteful).

### 4. Iterators: lazy, fused, no intermediate collections

In JavaScript, `arr.map(...).filter(...).reduce(...)` allocates a new array at each step and walks the data multiple times. In Rust, iterator adaptors are **lazy**: `.map()` and `.filter()` build a tiny zero-cost state machine and do *no work* until a consumer (`.sum()`, `.collect()`, a `for` loop) pulls elements through. The whole chain runs in **one pass**, pulling a single element all the way through before touching the next, and the compiler fuses and inlines it into code equivalent to a hand-written loop.

```rust playground
fn sum_even_squares(nums: &[i64]) -> i64 {
    // One pass, no intermediate Vec, no per-element bounds checks.
    nums.iter().filter(|&&n| n % 2 == 0).map(|&n| n * n).sum()
}

fn main() {
    let nums = [1, 2, 3, 4, 5, 6];
    println!("sum_even_squares: {}", sum_even_squares(&nums));
}
```

Verified output:

```text
sum_even_squares: 56
```

The key difference from JavaScript: `.filter(...).map(...).sum()` here makes **zero** intermediate allocations, with no temporary array of evens and no temporary array of squares. (`2² + 4² + 6² = 4 + 16 + 36 = 56`.) Because the iterator and the loop compile to the same machine code, you pick whichever reads best, which is almost always the iterator. The proof that this is genuinely zero-cost lives in [Zero-Cost Abstractions](/21-performance/06-zero-cost/); the laziness mechanics are in [Iterators](/07-collections/06-iterators/).

A common subtler waste is collecting into a `Vec` only to immediately iterate it again. If you do not need to *store* the intermediate, do not `.collect()` it:

```rust playground
fn total_len(words: &[&str]) -> usize {
    // stays lazy: sums lengths in one pass, allocates nothing
    words.iter().map(|w| w.len()).sum()
    // wasteful: words.iter().map(|w| w.len()).collect::<Vec<_>>().iter().sum()
}

fn main() {
    println!("{}", total_len(&["alpha", "beta", "gamma"]));
}
```

Verified output:

```text
14
```

### 5. Capacity: pre-size to avoid reallocation churn

A `Vec` (and `String`, `HashMap`, `HashSet`) tracks both a **length** and a **capacity**. When you push past capacity, it allocates a bigger buffer — roughly **doubling** — and copies everything over. If you know roughly how many items are coming, tell it up front with `with_capacity`. The difference is stark:

```rust playground
fn main() {
    let mut grown: Vec<u64> = Vec::new();
    let mut reallocs = 0;
    let mut cap = grown.capacity();
    for i in 0..1_000u64 {
        grown.push(i);
        if grown.capacity() != cap {
            reallocs += 1;
            cap = grown.capacity();
        }
    }
    println!("Vec::new()           -> {reallocs} reallocations");

    let mut sized: Vec<u64> = Vec::with_capacity(1_000);
    let mut r2 = 0;
    let mut c2 = sized.capacity();
    for i in 0..1_000u64 {
        sized.push(i);
        if sized.capacity() != c2 {
            r2 += 1;
            c2 = sized.capacity();
        }
    }
    println!("Vec::with_capacity() -> {r2} reallocations");
}
```

Verified output:

```text
Vec::new()           -> 9 reallocations
Vec::with_capacity() -> 0 reallocations
```

Nine buffer reallocations (each copying the whole `Vec`) versus zero. The same idea applies to strings: building text with `String::with_capacity(total)` plus `push_str` avoids re-growing the buffer. JavaScript exposes none of this: you cannot pre-size a `Map`, `Set`, or even an `Array` in a way the engine guarantees to respect. The full treatment of `Vec` growth lives in [Vectors](/07-collections/00-vectors/) and the collection-by-collection guidance in [Collection Performance](/07-collections/09-collection-performance/).

> **Tip:** When you build a collection with `.collect()`, you usually get this for free: the iterator reports a size hint and `collect` pre-allocates accordingly. Reach for an explicit `with_capacity` mainly when growing element-by-element in a hot loop where you can estimate the final size.

---

## Key Differences

| Concern                       | TypeScript / JavaScript                          | Rust                                                              |
| ----------------------------- | ------------------------------------------------ | ----------------------------------------------------------------- |
| Copying data                  | Implicit; assigning copies a reference, GC cleans up | Explicit: `.clone()` is a visible deep copy; borrow `&` is free |
| Read-only string argument     | Always a `string` (one type)                     | Take `&str`, not `String`/`&String`                               |
| "Sometimes own, sometimes borrow" | Always returns a string                       | `Cow<str>` — borrow on the no-op path, allocate only on rewrite   |
| `map`/`filter` chains         | Allocate an intermediate array per step, multiple passes | Lazy + fused, single pass, zero intermediates             |
| Pre-allocation                | Not reliably exposed                             | `with_capacity(n)` / `reserve(n)` on all growable collections     |
| Where data lives              | GC heap, engine-managed                          | You choose: stack, borrowed slice, or explicit heap (`String`/`Vec`/`Box`) |
| Cost visibility               | Hidden; surfaces as GC pauses and reallocations  | Visible at the call site — allocations are where you wrote them   |

### Why Rust surfaces all of this

A garbage-collected language optimizes for "don't make the developer think about memory." Rust optimizes for "nothing allocates or copies unless you can see it in the source." The upside is that performance is predictable and tunable, with no surprise GC pause and no hidden array reallocation. The cost is that *you* are responsible for not writing the wasteful version, but the compiler, the borrow checker, and Clippy do an enormous amount of the catching for you. Importantly, the allocation-free style is also the *idiomatic* style, so writing clean Rust and writing fast Rust usually pull in the same direction.

---

## Common Pitfalls

### Pitfall 1: Cloning to silence the borrow checker

When the borrow checker complains that a value is moved or borrowed, the tempting fix is `.clone()`. Often the *right* fix is to borrow, restructure, or take `&str`/`&[T]`. The clone compiles and is correct; it is just silently allocating and copying on every call, and it will not show up until it is on a hot path. There is no compiler error here; it is a design smell. Reach for Clippy (`cargo clippy`) which flags many redundant clones, and ask "do I actually need ownership here, or just to read this?"

### Pitfall 2: Taking `&String` instead of `&str`

Writing `fn f(s: &String)` works but is strictly worse than `fn f(s: &str)`: it cannot accept string literals without an allocation, and it pins the caller to an owned `String`. Clippy catches this by default:

```rust playground
fn count_chars(s: &String) -> usize { // should be &str
    s.chars().count()
}

fn main() {
    println!("{}", count_chars(&"hello".to_string()));
}
```

Real Clippy output:

```text
warning: writing `&String` instead of `&str` involves a new object where a slice will do
 --> src/main.rs:1:19
  |
1 | fn count_chars(s: &String) -> usize {
  |                   ^^^^^^^ help: change this to: `&str`
  |
  = note: `#[warn(clippy::ptr_arg)]` on by default
```

**Fix:** change the parameter to `&str`. The body usually needs no other change because `&String` already derefs to `&str`.

### Pitfall 3: Trying to return a borrow of a local to "avoid" an allocation

In your zeal to return `&str` instead of `String`, you may try to return a reference to a `String` created inside the function. That data is dropped when the function returns, so the reference would dangle, and Rust refuses to compile it:

```rust
fn make_label<'a>(id: u32) -> &'a str {
    let s = format!("item-{id}");
    &s // does not compile (error[E0515]): returns a reference to local data
}

fn main() {
    println!("{}", make_label(7));
}
```

Real compiler error:

```text
error[E0515]: cannot return reference to local variable `s`
 --> src/main.rs:3:5
  |
3 |     &s
  |     ^^ returns a reference to data owned by the current function

For more information about this error, try `rustc --explain E0515`.
```

**Fix:** if the function genuinely creates new text, it *must* return an owned `String` (or `Cow<str>` if it sometimes borrows the input). You can only return `&str` when it borrows from one of the function's *inputs*, not from a local. This is the line where the "avoid allocation" rule correctly bottoms out: creating new data requires owning it.

### Pitfall 4: `iter().cloned().collect()` when `to_vec()` (or no copy) will do

Reaching for `.iter().cloned().collect::<Vec<_>>()` to duplicate a slice is both slower and noisier than `.to_vec()`, and often you did not need the copy at all; a borrow would have worked.

```rust playground
fn main() {
    let v = vec![1, 2, 3];
    let w = v.iter().cloned().collect::<Vec<_>>(); // verbose redundant clone
    println!("{w:?}");
}
```

Real Clippy output:

```text
warning: called `iter().cloned().collect()` on a slice to create a `Vec`. Calling `to_vec()` is both faster and more readable
 --> src/main.rs:3:14
  |
3 |     let w = v.iter().cloned().collect::<Vec<_>>();
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: try: `.to_vec()`
  |
  = note: `#[warn(clippy::iter_cloned_collect)]` on by default
```

**Fix:** use `.to_vec()` if you truly need an owned copy; or, better, pass `&[T]` and avoid the copy entirely.

---

## Best Practices

### 1. Borrow by default, own only when you must

Accept `&str`, `&[T]`, and `&T` in function signatures; return owned values when you create new data. Make `.clone()` a deliberate decision, not a reflex to appease the compiler. Run `cargo clippy` regularly; it catches `clone_on_copy`, `ptr_arg`, `redundant_clone` (on nightly), and many more allocation smells for free.

### 2. Keep iterator chains lazy; collect once, at the end

Chain `.filter().map().take()...` freely: it is one fused pass with no intermediates. Only `.collect()` when you actually need to *store* the result or iterate it more than once. Never `.collect()` into a `Vec` solely to call `.iter()` on it again.

### 3. Reach for `Cow` on the "usually unchanged" path

When a function returns its input untouched most of the time and a modified copy occasionally (normalization, escaping, trimming), return `Cow<str>`. The common case allocates nothing.

### 4. Pre-size collections on hot, size-known loops

Use `Vec::with_capacity(n)`, `String::with_capacity(n)`, and `HashMap::with_capacity(n)` when you grow a collection element-by-element to an estimable size in a hot loop. For `.collect()`-built collections, trust the automatic size hint instead of adding noise.

### 5. Measure before and after — do not guess

These habits are safe defaults, but for *targeted* optimization always benchmark. Use [Criterion](/21-performance/02-benchmarking/) for statistically sound before/after numbers and a [profiler](/21-performance/00-profiling/) to confirm where the time actually goes. The quick `Instant::now()` timings on this page are illustrative; Criterion is what you commit to a repo. And remember the discipline in [When to Optimize](/21-performance/10-when-to-optimize/): readable first, then fast where the profiler points.

---

## Real-World Example

A request-routing audit: ingest a batch of raw access-log lines, find every server error, and group the offending paths by HTTP method, all in one lazy pass with the minimum possible allocation. Paths are normalized through a `Cow` so only the genuinely-malformed ones allocate, and the report map is pre-sized.

```rust playground
use std::borrow::Cow;
use std::collections::HashMap;

#[derive(Debug)]
struct Request<'a> {
    method: &'a str,
    path: &'a str,
    status: u16,
}

/// Borrowed parse: every field is a slice of the input line.
fn parse_line(line: &str) -> Option<Request<'_>> {
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    let status: u16 = parts.next()?.parse().ok()?;
    Some(Request { method, path, status })
}

/// Borrow when already clean; allocate only to collapse duplicate slashes.
fn normalize_path(path: &str) -> Cow<'_, str> {
    if path.contains("//") {
        Cow::Owned(path.replace("//", "/"))
    } else {
        Cow::Borrowed(path)
    }
}

/// Group normalized error paths by method. Takes `&str` (not `String`),
/// returns owned data because the report outlives the input borrow.
fn audit_errors(raw: &str) -> HashMap<String, Vec<String>> {
    // Rough pre-size: a handful of distinct HTTP methods.
    let mut by_method: HashMap<String, Vec<String>> = HashMap::with_capacity(8);

    raw.lines()
        .filter_map(parse_line)
        .filter(|req| req.status >= 500)
        .for_each(|req| {
            let path = normalize_path(req.path).into_owned();
            by_method
                .entry(req.method.to_string())
                .or_default()
                .push(path);
        });

    by_method
}

fn main() {
    let raw = "\
GET /api//users 200
POST /api//orders 503
GET /api/health 200
POST /api/orders 500
DELETE /api//cache 500
GET /api//users 502";

    let report = audit_errors(raw);

    // HashMap order is randomized; sort keys for stable output.
    let mut methods: Vec<&String> = report.keys().collect();
    methods.sort();
    for method in methods {
        println!("{method}: {:?}", report[method]);
    }
}
```

Verified output:

```text
DELETE: ["/api/cache"]
GET: ["/api/users"]
POST: ["/api/orders", "/api/orders"]
```

Walk the allocation budget: parsing allocates nothing (all slices borrow from `raw`); the iterator chain runs in a single pass with no intermediate `Vec`; `normalize_path` only allocates for the three paths that actually contained `//`; the report map is pre-sized so it never reallocates; and the only unavoidable owned data is what the returned `HashMap` must hold beyond the lifetime of the input. The `.into_owned()` is the deliberate, *visible* point where a borrow becomes an owned `String` because it has to be stored. That visibility — knowing exactly where each allocation happens — is the whole point.

> **Note:** `entry(...).or_default()` does a single hash lookup that finds-or-creates the `Vec` for that method; `.push(path)` then appends in place. This is the same allocation-conscious entry pattern covered in [HashMaps](/07-collections/03-hashmaps/), reused here. For more allocation- and ownership-aware design patterns, see [Section 22 — Common Patterns](/22-common-patterns/).

---

## Further Reading

### Official Documentation

- [The Rust Performance Book](https://nnethercote.github.io/perf-book/): the canonical guide; chapters on heap allocations, `Cow`, and avoiding copies
- [`std::borrow::Cow`](https://doc.rust-lang.org/std/borrow/enum.Cow.html): clone-on-write
- [`str` (the `&str` primitive)](https://doc.rust-lang.org/std/primitive.str.html) and [`String`](https://doc.rust-lang.org/std/string/struct.String.html)
- [`Vec::with_capacity`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.with_capacity) and [`Vec::reserve`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.reserve)
- [Clippy lint list](https://rust-lang.github.io/rust-clippy/master/): `clone_on_copy`, `ptr_arg`, `iter_cloned_collect`, `redundant_clone`, and more

### Related Topics in This Guide

- [When to Optimize](/21-performance/10-when-to-optimize/): measure first; readable-then-fast
- [Profiling](/21-performance/00-profiling/) and [Flame Graphs](/21-performance/01-flamegraph/): find the hot spots before changing anything
- [Benchmarking](/21-performance/02-benchmarking/): Criterion for statistically sound before/after numbers
- [Zero-Cost Abstractions](/21-performance/06-zero-cost/): proof that iterators compile to the same code as hand loops
- [Memory Layout](/21-performance/04-memory-layout/) and [Cache Efficiency](/21-performance/05-cache-efficiency/): the next level of allocation-aware design
- [Performance vs Node.js](/21-performance/09-comparison/): the honest, end-to-end comparison
- [Section 05 — Ownership](/05-ownership/) and [move/copy/clone](/05-ownership/06-move-copy-clone/): why borrowing is free and cloning is not
- [Vectors](/07-collections/00-vectors/), [Iterators](/07-collections/06-iterators/), [Collection Performance](/07-collections/09-collection-performance/): capacity, laziness, and Big-O
- [Section 22 — Common Patterns](/22-common-patterns/): idiomatic, allocation-aware designs

---

## Exercises

### Exercise 1: Make the Signature Lean

**Difficulty:** Beginner

**Objective:** Replace needless ownership/allocation in a function signature and body.

**Instructions:** The function below takes a `&String` and clones words it does not need to clone. Rewrite it to take `&str`, allocate nothing extra, and still return the number of words longer than `min_len`. It should compile with no Clippy warnings.

```rust playground
fn count_long_words(text: &String, min_len: usize) -> usize {
    let owned = text.clone(); // ??? do we need this?
    owned.split_whitespace().filter(|w| w.len() > min_len).count()
}

fn main() {
    let s = String::from("the quick brown fox");
    println!("{}", count_long_words(&s, 3)); // expects 2: "quick", "brown"
}
```

<details>
<summary>Solution</summary>

```rust playground
fn count_long_words(text: &str, min_len: usize) -> usize {
    // No clone, no owned String: borrow and count in one lazy pass.
    text.split_whitespace().filter(|w| w.len() > min_len).count()
}

fn main() {
    let s = String::from("the quick brown fox");
    println!("{}", count_long_words(&s, 3)); // 2: "quick", "brown"
    println!("{}", count_long_words("a literal works too", 1)); // 3
}
```

Output:

```text
2
3
```

Taking `&str` lets the function accept both the `String` (via deref) and a literal, and dropping the `.clone()` removes the allocation entirely. The word counting stays a single lazy iterator pass.

</details>

### Exercise 2: Allocate Only When You Mutate

**Difficulty:** Intermediate

**Objective:** Use `Cow<str>` so the no-op path allocates nothing.

**Instructions:** Write `trim_whitespace(s: &str) -> Cow<str>` that returns the input unchanged (borrowed, zero allocation) when it has no leading/trailing whitespace, and an owned, trimmed `String` only when it does. Prove with `matches!` that the clean input is borrowed and the dirty input is owned.

```rust
use std::borrow::Cow;

fn trim_whitespace(s: &str) -> Cow<'_, str> {
    // TODO: borrow when already trimmed; allocate only when trimming changes it
    todo!()
}

fn main() {
    let clean = trim_whitespace("hello");
    let dirty = trim_whitespace("  hello  ");
    println!("clean = {clean:?}, dirty = {dirty:?}");
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::borrow::Cow;

fn trim_whitespace(s: &str) -> Cow<'_, str> {
    let trimmed = s.trim();
    if trimmed.len() == s.len() {
        Cow::Borrowed(s) // nothing to trim -> no allocation
    } else {
        Cow::Owned(trimmed.to_string()) // changed -> allocate once
    }
}

fn main() {
    let clean = trim_whitespace("hello");
    let dirty = trim_whitespace("  hello  ");
    println!("clean borrowed? {}", matches!(clean, Cow::Borrowed(_)));
    println!("dirty owned?    {}", matches!(dirty, Cow::Owned(_)));
    println!("dirty value = {:?}", trim_whitespace("  hi  "));
}
```

Output:

```text
clean borrowed? true
dirty owned?    true
dirty value = "hi"
```

`s.trim()` itself returns a borrowed `&str` and allocates nothing; we only spend a `to_string()` when trimming actually shortened the slice. The clean path returns `Cow::Borrowed`, allocating nothing at all.

</details>

### Exercise 3: One Lazy Pass, Pre-Sized Output

**Difficulty:** Advanced

**Objective:** Combine borrowing, lazy iteration, capacity pre-allocation, and `HashSet`-based deduplication into one allocation-conscious function.

**Instructions:** Write `distinct_even_squares(nums: &[i64]) -> Vec<i64>` that returns the squares of the *distinct even* values in `nums`, in any order. Borrow the slice (do not take `Vec`), deduplicate with a `HashSet` pre-sized to the input length, build the output `Vec` with a fitting capacity, and do the squaring lazily. Verify on `[2, 2, 3, 4, 4, 6]`.

```rust
use std::collections::HashSet;

fn distinct_even_squares(nums: &[i64]) -> Vec<i64> {
    // TODO: dedup evens with a pre-sized HashSet, then square them lazily
    todo!()
}

fn main() {
    let mut out = distinct_even_squares(&[2, 2, 3, 4, 4, 6]);
    out.sort(); // sort only for stable display
    println!("{out:?}");
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::collections::HashSet;

fn distinct_even_squares(nums: &[i64]) -> Vec<i64> {
    // Pre-size the set to the upper bound on distinct values.
    let mut seen: HashSet<i64> = HashSet::with_capacity(nums.len());
    for &n in nums {
        if n % 2 == 0 {
            seen.insert(n); // dedup happens here
        }
    }
    // Pre-size the output, then square lazily in one pass.
    let mut out: Vec<i64> = Vec::with_capacity(seen.len());
    out.extend(seen.into_iter().map(|n| n * n));
    out
}

fn main() {
    let mut out = distinct_even_squares(&[2, 2, 3, 4, 4, 6]);
    out.sort(); // sort only for stable display
    println!("{out:?}");
}
```

Output:

```text
[4, 16, 36]
```

The function borrows the slice (callers keep ownership), the `HashSet` is pre-sized so it never reallocates while inserting, the squaring is a lazy `.map()` consumed straight into the pre-sized output `Vec` via `extend`, and `into_iter()` *moves* the deduplicated values out of the set rather than cloning them. The distinct evens are `2, 4, 6`, whose squares are `4, 16, 36` (the duplicate `2` and `4`, and the odd `3`, are dropped).

</details>
