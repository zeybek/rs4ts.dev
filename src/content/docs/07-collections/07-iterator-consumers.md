---
title: "Iterator Consumers: From Array Methods to `collect`, `fold`, and Friends"
description: "Iterator consumers run a Rust pipeline and return a value: collect, fold, sum, count, find, any, all, min, and max replace JavaScript's reduce, find, some"
---

The sibling page on [iterators](/07-collections/06-iterators/) covered the **lazy** adaptors — `map`, `filter`, `take`, `zip` — that build a pipeline but compute nothing on their own. This page covers the other half: the **consuming adaptors** (or *consumers*) that actually run the pipeline and hand you a result. These are the methods that line up with `reduce`, `find`, `some`, `every`, and the implicit "now give me an array" that every JavaScript chain ends with.

---

## Quick Overview

A **consumer** is an iterator method that takes ownership of the iterator (`self`, not `&mut self`), drives it to completion (or until it can short-circuit), and produces a final value: a number, a `bool`, an `Option`, or a whole new collection. In TypeScript an array method like `.reduce()` or `.filter()` always produces a value immediately; in Rust the lazy chain does nothing until a consumer like `collect`, `sum`, `fold`, `find`, `any`, `all`, or `max` is called at the end. **The consumer is what makes the whole pipeline run** — without one, the compiler will warn you that your iterator was never used.

---

## TypeScript/JavaScript Example

Here is a realistic chunk of e-commerce analytics: given a list of orders, compute several aggregates. Notice every line produces a value eagerly, and that `.reduce()` on an empty array *throws*.

```typescript
interface Order {
  id: number;
  customer: string;
  totalCents: number;
  paid: boolean;
}

const orders: Order[] = [
  { id: 1, customer: "alice", totalCents: 4999, paid: true },
  { id: 2, customer: "bob", totalCents: 12000, paid: false },
  { id: 3, customer: "alice", totalCents: 8050, paid: true },
  { id: 4, customer: "carol", totalCents: 0, paid: true },
];

// count → .filter(...).length
const paidCount = orders.filter((o) => o.paid).length;

// sum → .reduce(...)
const revenue = orders
  .filter((o) => o.paid)
  .reduce((sum, o) => sum + o.totalCents, 0);

// any / all → .some() / .every()
const hasUnpaid = orders.some((o) => !o.paid);
const allHaveCustomer = orders.every((o) => o.customer.length > 0);

// find → .find() (returns the element or undefined)
const firstBig = orders.find((o) => o.totalCents > 5000);

// max → .reduce() with a comparator (no built-in maxBy!)
const priciest = orders.reduce((best, o) =>
  o.totalCents > best.totalCents ? o : best
);

// build a new array → .filter(...).map(...)
const paidCustomers = orders.filter((o) => o.paid).map((o) => o.customer);
```

Two things to keep in mind, because Rust handles them differently: `.find()` returns `undefined` when nothing matches (no error), and `.reduce()` with **no initial value** on an **empty** array throws a `TypeError`.

---

## Rust Equivalent

```rust
#[derive(Debug)]
struct Order {
    id: u32,
    customer: String,
    total_cents: u64,
    paid: bool,
}

fn main() {
    let orders = vec![
        Order { id: 1, customer: "alice".into(), total_cents: 4_999, paid: true },
        Order { id: 2, customer: "bob".into(),   total_cents: 12_000, paid: false },
        Order { id: 3, customer: "alice".into(), total_cents: 8_050, paid: true },
        Order { id: 4, customer: "carol".into(), total_cents: 0,      paid: true },
    ];

    // count: how many orders are paid?
    let paid_count = orders.iter().filter(|o| o.paid).count();

    // sum: total revenue from paid orders, in cents.
    let revenue: u64 = orders
        .iter()
        .filter(|o| o.paid)
        .map(|o| o.total_cents)
        .sum();

    // any / all: quick boolean checks that short-circuit.
    let has_unpaid = orders.iter().any(|o| !o.paid);
    let all_have_customer = orders.iter().all(|o| !o.customer.is_empty());

    // find: first matching element, as Option<&Order>.
    let first_big = orders.iter().find(|o| o.total_cents > 5_000);

    // max_by_key: the priciest order (Option, because the list could be empty).
    let priciest = orders.iter().max_by_key(|o| o.total_cents);

    // collect: build a brand-new Vec of just the paid customers' names.
    let paid_customers: Vec<&str> =
        orders.iter().filter(|o| o.paid).map(|o| o.customer.as_str()).collect();

    // fold: a running custom aggregation (here, a formatted receipt).
    let receipt = orders.iter().fold(String::new(), |mut acc, o| {
        acc.push_str(&format!("#{} ${:.2}\n", o.id, o.total_cents as f64 / 100.0));
        acc
    });

    println!("paid_count      = {paid_count}");
    println!("revenue (cents) = {revenue}");
    println!("has_unpaid      = {has_unpaid}");
    println!("all_have_cust   = {all_have_customer}");
    println!("first_big       = {:?}", first_big.map(|o| o.id));
    println!("priciest        = {:?}", priciest.map(|o| o.id));
    println!("paid_customers  = {paid_customers:?}");
    print!("{receipt}");
}
```

Real output:

```text
paid_count      = 3
revenue (cents) = 13049
has_unpaid      = true
all_have_cust   = true
first_big       = Some(2)
priciest        = Some(2)
paid_customers  = ["alice", "alice", "carol"]
#1 $49.99
#2 $120.00
#3 $80.50
#4 $0.00
```

The aggregate numbers match the TypeScript version exactly (`paid_count = 3`, `revenue = 13049`). The differences are all about **types**. `find` and `max_by_key` give you an `Option`, never `undefined`; `count` returns a `usize`; and `revenue` needs an explicit `u64` annotation so `sum` knows what to add into.

---

## Detailed Explanation

### `collect` — the universal "materialize" consumer

In TypeScript a chain *ends* in an array because the methods return arrays. In Rust the chain ends in `.collect()`, which can build **many** different collections (`Vec`, `String`, `HashMap`, `HashSet`, even `Result`) depending on the target type you ask for:

```rust
use std::collections::{HashMap, HashSet};

fn main() {
    let v: Vec<i32> = (1..=5).collect();
    let s: String = vec!['h', 'i', '!'].into_iter().collect();
    let set: HashSet<i32> = vec![1, 2, 2, 3, 3, 3].into_iter().collect();
    let map: HashMap<&str, i32> = vec![("a", 1), ("b", 2)].into_iter().collect();

    println!("v={v:?}");
    println!("s={s}");
    println!("set.len()={}", set.len());
    println!("map.get(b)={:?}", map.get("b"));

    // Turbofish form: annotate the call instead of the binding.
    let doubled = (1..=3).map(|n| n * 2).collect::<Vec<_>>();
    println!("doubled={doubled:?}");
}
```

```text
v=[1, 2, 3, 4, 5]
s=hi!
set.len()=3
map.get(b)=Some(2)
doubled=[2, 4, 6]
```

`collect` is generic over the trait `FromIterator`. Because the *same* call can produce a `Vec`, a `String`, a `HashMap`, and so on, **you must tell Rust which one you want**: either with a binding annotation (`let v: Vec<i32> =`) or with the `::<Vec<_>>` "turbofish". This is the single biggest surprise for newcomers, and it has no TypeScript analogue, where `.map()` *always* returns an array.

> **Tip:** When the element type is obvious but the container is not, use `collect::<Vec<_>>()`. The `_` lets the compiler infer the element type while you pin the container.

### `collect` into `Result` — short-circuiting validation

One of the most useful tricks: an iterator of `Result`s can collect into a single `Result<Vec<_>, E>`. The first `Err` stops the process and becomes the whole result. This is the idiomatic way to "parse every item, but bail on the first failure":

```rust
fn main() {
    let good: Result<Vec<i32>, _> =
        vec!["1", "2", "3"].iter().map(|s| s.parse::<i32>()).collect();
    let bad: Result<Vec<i32>, _> =
        vec!["1", "x", "3"].iter().map(|s| s.parse::<i32>()).collect();

    println!("good={good:?}");
    println!("bad is_err={}", bad.is_err());
}
```

```text
good=Ok([1, 2, 3])
bad is_err=true
```

There is no clean JavaScript equivalent. You would reach for a `for` loop with a `try/catch`, or `Promise.all` semantics if it were async. See [Section 08 — Error Handling](/08-error-handling/) for why this pattern is everywhere in Rust.

### `sum`, `product`, `count` — numeric reductions

```rust
fn main() {
    let total: i32 = (1..=5).sum();        // 1+2+3+4+5
    let fact: u64 = (1..=5u64).product();  // 5!
    let evens = (1..=10).filter(|n| n % 2 == 0).count();

    println!("total={total} fact={fact} evens={evens}");
}
```

```text
total=15 fact=120 evens=5
```

`sum` and `product` are like `collect`: they are generic over the *output* type, so you usually annotate it (`let total: i32`). `count` always returns a `usize`, so it never needs annotating. Each replaces a `reduce` you would write by hand in JavaScript.

### `min`, `max`, and the `_by` / `_by_key` family

```rust
fn main() {
    let nums = vec![3, 7, 2, 9, 4];
    println!("min={:?} max={:?}", nums.iter().min(), nums.iter().max());

    let words = vec!["pear", "fig", "banana"];
    println!("longest={:?}", words.iter().max_by_key(|w| w.len()));
    println!("shortest={:?}", words.iter().min_by_key(|w| w.len()));

    // f64 isn't `Ord`, so plain `.max()` won't compile — fold with f64::max.
    let temps = vec![19.5_f64, 22.0, 18.0];
    let hottest = temps.iter().cloned().fold(f64::MIN, f64::max);
    println!("hottest={hottest}");
}
```

```text
min=Some(2) max=Some(9)
longest=Some("banana")
shortest=Some("fig")
hottest=22
```

JavaScript has no `Array.prototype.maxBy`; you write `.reduce()` with a comparator. Rust gives you the whole family directly:

- `min` / `max`: compare elements with their natural ordering (`Ord`).
- `min_by_key` / `max_by_key`: compare a derived key (cheap, computed once per element).
- `min_by` / `max_by`: compare with a custom `|a, b| a.cmp(b)` closure returning `Ordering`.

All of them return `Option<T>` (`None` for an empty iterator). The `f64` case is a real gotcha covered under [Common Pitfalls](#common-pitfalls): floats are not totally ordered (because of `NaN`), so `f64` does not implement `Ord`, and `.max()` won't compile.

### `find`, `position`, `find_map` — "give me the first one that…"

```rust
fn main() {
    let data = vec!["", "  ", "hello", "world"];

    // find: first ELEMENT matching the predicate.
    let first_nonblank = data.iter().find(|s| !s.trim().is_empty());

    // position: INDEX of the first match (like Array.prototype.findIndex).
    let idx = data.iter().position(|s| !s.trim().is_empty());

    // find_map: first item where the closure returns Some(...).
    let parsed = vec!["x", "12", "y"].iter().find_map(|s| s.parse::<i32>().ok());

    println!("first_nonblank={first_nonblank:?}");
    println!("idx={idx:?}");
    println!("parsed={parsed:?}");
}
```

```text
first_nonblank=Some("hello")
idx=Some(2)
parsed=Some(12)
```

`find` ≈ `Array.prototype.find`, `position` ≈ `findIndex`, and `find_map` has no single JS equivalent: it is `find` and `map` fused so you compute the transformed value exactly once for the first match. All of them **short-circuit**: they stop the moment they have an answer, never touching the rest of the iterator.

### `any` / `all` — the boolean short-circuiters

```rust
fn main() {
    let nums = vec![2, 4, 6, 8];
    let has_odd = nums.iter().any(|n| n % 2 == 1);  // like .some()
    let all_even = nums.iter().all(|n| n % 2 == 0); // like .every()

    // Edge cases on an EMPTY iterator — note the defaults!
    let any_empty = std::iter::empty::<i32>().any(|n| n > 0);
    let all_empty = std::iter::empty::<i32>().all(|n| n > 0);

    println!("has_odd={has_odd} all_even={all_even}");
    println!("any_empty={any_empty} all_empty={all_empty}");
}
```

```text
has_odd=false all_even=true
any_empty=false all_empty=true
```

These behave exactly like JavaScript's `some`/`every`, including the "vacuous truth" defaults: `any` on an empty iterator is `false`, `all` on an empty iterator is `true`.

### `fold` vs `reduce` — the key distinction

This is where Rust splits one JavaScript method into two:

```rust
fn main() {
    // fold: you SUPPLY a seed, so the result type can differ from the items.
    let folded = (1..=4).fold(100, |acc, n| acc + n); // 100 + 1+2+3+4

    // reduce: NO seed; the first item is the seed. Returns Option.
    let reduced = (1..=4).reduce(|acc, n| acc + n);

    // reduce on an EMPTY iterator returns None — it does NOT panic.
    let empty_reduced = std::iter::empty::<i32>().reduce(|a, b| a + b);

    println!("folded={folded} reduced={reduced:?} empty_reduced={empty_reduced:?}");
}
```

```text
folded=110 reduced=Some(10) empty_reduced=None
```

- **`fold(init, f)`** ≈ JavaScript `reduce(f, initialValue)`. The accumulator type comes from `init`, so it can be anything: a number, a `String`, a `HashMap`. This is the workhorse for building up custom aggregates (the receipt-building example earlier folds into a `String`).
- **`reduce(f)`** ≈ JavaScript `reduce(f)` *without* an initial value. Because there might be zero elements, Rust returns `Option<T>` instead of throwing. This is the important safety difference: JavaScript's seedless `[].reduce(...)` throws a `TypeError`; Rust's `reduce` quietly gives you `None`.

Reach for `fold` by default. Reach for `reduce` only when there is no sensible identity value (e.g. "combine these UI nodes into one") and you genuinely want the `Option`.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust | Notes |
| --- | --- | --- | --- |
| Build a collection | implicit; methods return arrays | explicit `.collect()` | must annotate the target type |
| Multiple target types | always `Array` | `Vec`, `String`, `HashMap`, `HashSet`, `Result`… | one method, many `FromIterator` impls |
| Sum | `arr.reduce((a, b) => a + b, 0)` | `.sum::<T>()` | annotate the numeric type |
| Count | `arr.length` after filtering | `.count()` | returns `usize` |
| First match (value) | `.find()` → element \| `undefined` | `.find()` → `Option<T>` | no `undefined`/`null` |
| First match (index) | `.findIndex()` → `number` (`-1` miss) | `.position()` → `Option<usize>` | `None`, not `-1` |
| Some / every | `.some()` / `.every()` | `.any()` / `.all()` | identical short-circuit semantics |
| Max element | `.reduce()` with comparator | `.max()`, `.max_by_key()`, `.max_by()` | built-in; returns `Option` |
| Reduce with seed | `.reduce(f, init)` | `.fold(init, f)` | accumulator type = seed type |
| Reduce without seed | `.reduce(f)` — **throws if empty** | `.reduce(f)` → `Option<T>` | `None` instead of a `TypeError` |
| Laziness | eager, runs at each call | lazy, runs only at the consumer | the consumer drives the chain |

> **Note:** The deepest difference is **laziness**. A JavaScript `.filter().map()` builds two intermediate arrays immediately. A Rust `.filter().map()` builds *nothing*. It is a description of work that only happens when a consumer like `collect`, `sum`, or `for` pulls items through. See [Iterators](/07-collections/06-iterators/) for the full story on lazy adaptors.

---

## Common Pitfalls

### Pitfall 1: `collect` (or `sum`) with no type annotation

The compiler cannot guess which collection or numeric type you want:

```rust
fn main() {
    let doubled = (1..=3).map(|n| n * 2).collect(); // does not compile (error[E0283])
    println!("{doubled:?}");
}
```

Real `rustc` output:

```text
error[E0283]: type annotations needed
 --> src/main.rs:2:9
  |
2 |     let doubled = (1..=3).map(|n| n * 2).collect();
  |         ^^^^^^^                          ------- type must be known at this point
  |
  = note: cannot satisfy `_: FromIterator<i32>`
help: consider giving `doubled` an explicit type
  |
2 |     let doubled: Vec<_> = (1..=3).map(|n| n * 2).collect();
  |                ++++++++
```

The same `E0283` appears for `sum`:

```rust
fn main() {
    let nums = vec![1, 2, 3];
    let total = nums.iter().sum(); // does not compile (error[E0283]: type annotations needed)
    println!("{total}");
}
```

```text
error[E0283]: type annotations needed
 --> src/main.rs:3:9
  |
3 |     let total = nums.iter().sum();
  |         ^^^^^               --- type must be known at this point
  |
  = note: cannot satisfy `_: Sum<&i32>`
help: consider giving `total` an explicit type
  |
3 |     let total: /* Type */ = nums.iter().sum();
  |              ++++++++++++
```

**Fix:** annotate the binding (`let total: i32`) or use a turbofish (`.sum::<i32>()`, `.collect::<Vec<_>>()`).

### Pitfall 2: using an iterator after a consumer has eaten it

Consumers take `self` by value. Once you call one, the iterator is **moved** and gone; you cannot call a second consumer on the same iterator:

```rust
fn main() {
    let nums = vec![1, 2, 3];
    let it = nums.iter();
    let count = it.count();    // count() consumes `it`
    let total: i32 = it.sum(); // does not compile (error[E0382]: use of moved value)
    println!("{count} {total}");
}
```

Real `rustc` output (trimmed):

```text
error[E0382]: use of moved value: `it`
 --> src/main.rs:5:22
  |
3 |     let it = nums.iter();
  |         -- move occurs because `it` has type `std::slice::Iter<'_, i32>`, which does not implement the `Copy` trait
4 |     let count = it.count();    // count() consumes `it`
  |                    ------- `it` moved due to this method call
5 |     let total: i32 = it.sum(); //
  |                      ^^ value used here after move
```

**Fix:** make a fresh iterator each time (`nums.iter().count()` then `nums.iter().sum()`), since `iter()` only borrows the `Vec`. In TypeScript you would just call two methods on the same array; an iterator is single-use, more like a generator you have already exhausted.

### Pitfall 3: `min`/`max` on floats

`f64` and `f32` are **not** `Ord` (because `NaN` breaks total ordering), so `.min()` / `.max()` simply do not exist for them:

```rust
fn main() {
    let temps = vec![19.5_f64, 22.0, 18.0];
    let hottest = temps.iter().max(); // does not compile (error[E0277]: f64: Ord not satisfied)
    println!("{hottest:?}");
}
```

```text
error[E0277]: the trait bound `f64: Ord` is not satisfied
 --> src/main.rs:3:32
  |
3 |     let hottest = temps.iter().max();
  |                                ^^^ the trait `Ord` is not implemented for `f64`
```

**Fix:** decide how to treat `NaN` yourself. The simplest is to fold with the partial-order-aware `f64::max`: `temps.iter().cloned().fold(f64::MIN, f64::max)`. Or use `.max_by(|a, b| a.partial_cmp(b).unwrap())` if you are certain there are no `NaN`s. This is a place where the analogy to JavaScript's `Math.max(...arr)` (which silently returns `NaN` if any element is `NaN`) breaks down. Rust forces you to confront the ambiguity.

### Pitfall 4: forgetting the consumer entirely

A chain of lazy adaptors with no consumer does nothing, and the compiler warns:

```rust
fn main() {
    let nums = vec![1, 2, 3];
    nums.iter().map(|n| println!("{n}")); // runs NOTHING; warning: unused `Map`
}
```

Rust emits `warning: unused 'Map' that must be used` with the note `iterators are lazy and do nothing unless consumed`. If you wanted side effects, use a `for` loop or the `for_each` consumer: `nums.iter().for_each(|n| println!("{n}"));`. Coming from JavaScript, where `.map()` always executes, this is the most common "why is nothing happening?" moment.

---

## Best Practices

- **Pick the most specific consumer.** Prefer `count()` over `.collect::<Vec<_>>().len()`, `sum()` over a hand-rolled `fold`, and `max_by_key` over `fold` with a manual comparison. They are clearer and let the compiler optimize.
- **Annotate the output type at the consumer.** `let total: u64 = ...` or `.sum::<u64>()`. Decide the integer width deliberately to avoid overflow on large sums.
- **Use `collect::<Result<Vec<_>, _>>()` for fallible pipelines.** It short-circuits on the first error and keeps the happy path flat — no manual loop with early returns.
- **Reach for `fold` over `reduce`.** `fold` is total (always returns a value) and lets the accumulator be any type. Use `reduce` only when there is genuinely no identity element and you want the `Option`.
- **`partition` splits in one pass.** When you would write two `filter`s, use `partition` instead. It walks the iterator once and returns a `(matches, rest)` tuple:

```rust
fn main() {
    let nums = vec![1, 2, 3, 4, 5, 6];
    let (evens, odds): (Vec<i32>, Vec<i32>) = nums.iter().partition(|&&n| n % 2 == 0);
    println!("evens={evens:?} odds={odds:?}");
}
```

```text
evens=[2, 4, 6] odds=[1, 3, 5]
```

- **Use `try_fold` for short-circuiting accumulation.** It stops at the first `None`/`Err`, which is perfect for checked arithmetic or validation that builds state:

```rust
fn main() {
    let nums = vec![1, 2, 3, 4];
    let checked: Option<i32> = nums.iter().try_fold(0i32, |acc, &n| acc.checked_add(n));
    println!("{checked:?}"); // Some(10); None if any add overflowed
}
```

```text
Some(10)
```

> **Tip:** Run `cargo clippy`. It will nudge you toward the idiomatic consumer — e.g. flagging a `fold` that should be a `sum` (via the default `clippy::unnecessary_fold` lint). And with the nursery lint `clippy::needless_collect` enabled (`-W clippy::needless_collect`), it suggests `.count()` instead of collecting just to call `.len()`.

---

## Real-World Example

A small log-analysis pass: parse raw lines into structured entries, drop the malformed ones, and compute a handful of metrics, every one of them a different consumer.

```rust
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct LogEntry {
    status: u16,
    bytes: u64,
    path: String,
}

fn parse(line: &str) -> Option<LogEntry> {
    let mut parts = line.split_whitespace();
    let status = parts.next()?.parse::<u16>().ok()?;
    let bytes = parts.next()?.parse::<u64>().ok()?;
    let path = parts.next()?.to_string();
    Some(LogEntry { status, bytes, path })
}

fn main() {
    let raw = "\
200 1024 /home
404 0 /missing
200 2048 /home
500 0 /api
200 512 /about
bad line
301 128 /old";

    // Parse every line, silently dropping the ones that don't parse (filter_map).
    let entries: Vec<LogEntry> = raw.lines().filter_map(parse).collect();

    // sum: total bytes served.
    let total_bytes: u64 = entries.iter().map(|e| e.bytes).sum();

    // any: did anything 5xx happen? (short-circuits)
    let has_5xx = entries.iter().any(|e| e.status >= 500);

    // count + count: success rate.
    let ok = entries.iter().filter(|e| (200..300).contains(&e.status)).count();
    let success_rate = ok as f64 / entries.len() as f64;

    // fold into a HashMap, then max_by_key: the busiest path.
    let hits = entries.iter().fold(HashMap::<&str, u32>::new(), |mut acc, e| {
        *acc.entry(e.path.as_str()).or_insert(0) += 1;
        acc
    });
    let busiest = hits.iter().max_by_key(|(_, count)| **count);

    // partition: separate redirects from everything else, in one pass.
    let (redirects, others): (Vec<_>, Vec<_>) =
        entries.iter().partition(|e| (300..400).contains(&e.status));

    println!("parsed entries  = {}", entries.len());
    println!("total_bytes     = {total_bytes}");
    println!("has_5xx         = {has_5xx}");
    println!("success_rate    = {success_rate:.2}");
    println!("busiest         = {:?}", busiest.map(|(p, c)| (*p, *c)));
    println!("redirects       = {}", redirects.len());
    println!("others          = {}", others.len());
}
```

Real output:

```text
parsed entries  = 6
total_bytes     = 3712
has_5xx         = true
success_rate    = 0.50
busiest         = Some(("/home", 2))
redirects       = 1
others          = 5
```

Every metric is a consumer doing one job: `filter_map(...).collect()` to materialize, `sum` for bytes, `any` for the 5xx check, `count` for the rate, `fold` + `max_by_key` for the busiest path, and `partition` for the split. In TypeScript this would be a mix of `.filter().map()`, manual `.reduce()` accumulators, and an object-as-map for the counts; and `parse` returning `Option` would instead be `null`-checks scattered through the pipeline.

> **Note:** Building the frequency map with `fold` here is idiomatic, but for grouping you will often reach for the `entry` API directly in a loop — see [HashMaps](/07-collections/03-hashmaps/). And `filter_map(parse)` works because `parse` returns `Option`; that is the consumer-side payoff of returning `Option` from fallible helpers, covered in [Section 08 — Error Handling](/08-error-handling/).

---

## Further Reading

### Official Documentation

- [`Iterator` trait — all consuming methods](https://doc.rust-lang.org/std/iter/trait.Iterator.html): `collect`, `fold`, `sum`, `find`, `any`, `all`, `max`, and the rest
- [`FromIterator` trait](https://doc.rust-lang.org/std/iter/trait.FromIterator.html): what makes `collect` polymorphic over the target type
- [The Rust Book — Processing a Series of Items with Iterators](https://doc.rust-lang.org/book/ch13-02-iterators.html)
- [Rust by Example — Iterators](https://doc.rust-lang.org/rust-by-example/trait/iter.html)

### Related Topics in This Guide

- [Iterators](/07-collections/06-iterators/) — the **lazy** adaptors (`map`, `filter`, `take`, `zip`, `enumerate`) that feed these consumers
- [Custom Iterators](/07-collections/08-custom-iterators/): implement `Iterator` for your own types so every consumer here works on them
- [Vectors](/07-collections/00-vectors/): `Vec<T>`, the most common thing you `collect` into
- [HashMaps](/07-collections/03-hashmaps/) and [HashSets](/07-collections/04-hashsets/): also valid `collect` targets
- [Collection Performance](/07-collections/09-collection-performance/): when an iterator chain beats a manual loop, and pre-sizing `collect`
- [Section 08 — Error Handling](/08-error-handling/): `collect::<Result<_, _>>()` and returning `Option`/`Result` from pipeline helpers
- [Section 02 — Basics: Types](/02-basics/01-types/): `usize`, `Option`, and why `f64` is not `Ord`
- [Section 01 — Getting Started](/01-getting-started/): setting up `cargo` to run these examples

---

## Exercises

### Exercise 1: Order statistics

**Difficulty:** Beginner

**Objective:** Combine `sum`, `max`, and `count` (via `len`) into one function, handling the empty case.

**Instructions:** Write `order_stats(prices: &[u32]) -> (u32, u32, f64)` returning `(total, max, average)`. For an empty slice it must return `(0, 0, 0.0)` and must **not** panic. (Hint: `max()` returns an `Option`; use `unwrap_or(0)`.)

```rust
fn order_stats(prices: &[u32]) -> (u32, u32, f64) {
    // TODO: total via sum, max via max(), average guarding against empty
    todo!()
}

fn main() {
    let (t, m, a) = order_stats(&[1200, 950, 4000, 300]);
    println!("total={t} max={m} avg={a:.2}");
    assert_eq!(order_stats(&[]), (0, 0, 0.0));
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust
fn order_stats(prices: &[u32]) -> (u32, u32, f64) {
    let count = prices.len();
    let total: u32 = prices.iter().sum();
    let max = prices.iter().copied().max().unwrap_or(0);
    let avg = if count == 0 {
        0.0
    } else {
        total as f64 / count as f64
    };
    (total, max, avg)
}

fn main() {
    let (t, m, a) = order_stats(&[1200, 950, 4000, 300]);
    println!("total={t} max={m} avg={a:.2}");
    assert_eq!(order_stats(&[]), (0, 0, 0.0));
    println!("ok");
}
```

Output:

```text
total=6450 max=4000 avg=1612.50
ok
```

</details>

### Exercise 2: Validate and sum in one pass

**Difficulty:** Intermediate

**Objective:** Use `collect` into a `Result` to parse a list of strings, bailing on the first bad one.

**Instructions:** Write `parse_all(tokens: &[&str]) -> Result<i32, std::num::ParseIntError>` that parses every token as `i32` and returns their **sum**, or the first parse error. Do not write a manual loop with early returns — let `collect` do the short-circuiting.

```rust
fn parse_all(tokens: &[&str]) -> Result<i32, std::num::ParseIntError> {
    // TODO: map -> collect into Result<Vec<i32>, _>, then sum
    todo!()
}

fn main() {
    assert_eq!(parse_all(&["1", "2", "3"]).unwrap(), 6);
    assert!(parse_all(&["1", "oops", "3"]).is_err());
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust
fn parse_all(tokens: &[&str]) -> Result<i32, std::num::ParseIntError> {
    let nums: Vec<i32> = tokens
        .iter()
        .map(|t| t.parse::<i32>())
        .collect::<Result<_, _>>()?;
    Ok(nums.iter().sum())
}

fn main() {
    assert_eq!(parse_all(&["1", "2", "3"]).unwrap(), 6);
    assert!(parse_all(&["1", "oops", "3"]).is_err());
    println!("ok");
}
```

The `?` unwraps the `Result<Vec<i32>, _>` produced by `collect`; if any token failed to parse, that error is returned immediately. Output:

```text
ok
```

</details>

### Exercise 3: Most common word

**Difficulty:** Advanced

**Objective:** Chain a transforming pipeline with a `fold`-built frequency map and a `max_by_key` consumer.

**Instructions:** Write `most_common_word(text: &str) -> Option<(String, u32)>` that lowercases words, strips surrounding punctuation, ignores empty tokens, counts occurrences, and returns the most frequent `(word, count)` — or `None` for empty input. (Hints: `split_whitespace`, `trim_matches`, `to_lowercase`, `fold` into a `HashMap<String, u32>`, then `into_iter().max_by_key(...)`.)

```rust
use std::collections::HashMap;

fn most_common_word(text: &str) -> Option<(String, u32)> {
    // TODO: normalize words, fold into a count map, then max_by_key
    todo!()
}

fn main() {
    let text = "The fox, the hound, and THE FOX!";
    println!("{:?}", most_common_word(text));
    assert_eq!(most_common_word(""), None);
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

fn most_common_word(text: &str) -> Option<(String, u32)> {
    let counts = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .fold(HashMap::<String, u32>::new(), |mut acc, w| {
            *acc.entry(w).or_insert(0) += 1;
            acc
        });

    counts.into_iter().max_by_key(|(_, count)| *count)
}

fn main() {
    let text = "The fox, the hound, and THE FOX!";
    println!("{:?}", most_common_word(text));
    assert_eq!(most_common_word(""), None);
    println!("ok");
}
```

Output:

```text
Some(("the", 3))
ok
```

> **Note:** `max_by_key` returns *some* maximal element when several tie; which one is unspecified for `HashMap` iteration order. If you need deterministic tie-breaking, fold into a [`BTreeMap`](/07-collections/05-btreemap-btreeset/) or sort first.

</details>
