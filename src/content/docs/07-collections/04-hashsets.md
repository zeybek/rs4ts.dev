---
title: "Sets and HashSet"
description: "Rust's HashSet<T> is the JavaScript Set for unique values, with insert returning a bool and built-in union, intersection, and difference set algebra since"
---

Rust's `HashSet<T>` is the standard hash-based collection of **unique** values: the direct counterpart to the JavaScript `Set`. If you have ever reached for `new Set()` to deduplicate an array or to answer "have I seen this before?", you already know the use case; this page shows the idiomatic Rust version and the rich set algebra Rust gives you out of the box.

---

## Quick Overview

A **`HashSet<T>`** stores each value at most once, with average **O(1)** insertion, removal, and membership checks. Unlike a JavaScript `Set`, a Rust `HashSet` is **homogeneous** (every element is the same type `T`), elements must be **hashable and comparable** (`T: Hash + Eq`), and iteration order is **unspecified and randomized**. Rust also ships first-class **set operations** (`union`, `intersection`, `difference`, and `symmetric_difference`) as methods and operators, which JavaScript only standardized very recently.

---

## TypeScript/JavaScript Example

```typescript
// Track which user IDs have already seen a feature flag, then reason about roles.
const seen = new Set<number>();

const events = [10, 20, 10, 30, 20, 10];
let firstTime = 0;
for (const id of events) {
  if (!seen.has(id)) {
    seen.add(id);
    firstTime++;
  }
}

console.log("unique users:", seen.size); // 3
console.log("first-time impressions:", firstTime); // 3
console.log("has user 20:", seen.has(20)); // true

// Set algebra. Native Set methods (union/intersection/difference) only landed
// in Node 22 / ES2024; before that everyone hand-rolled these with spreads.
const granted = new Set(["read", "write", "deploy"]);
const required = new Set(["read", "deploy", "admin"]);

const missing = required.difference(granted); // Set { 'admin' }
const hasAll = required.isSubsetOf(granted);   // false
console.log([...missing], hasAll);

// The classic one-liner: deduplicate an array.
const unique = [...new Set(["a", "b", "a", "c", "b"])]; // ['a', 'b', 'c']
console.log(unique);
```

**Key points:**

- A `Set` can hold values of mixed types (`new Set([1, "a", {}])`).
- `has`, `add`, and `delete` are the core operations.
- Membership uses **SameValueZero** equality (reference equality for objects — two distinct `{x:1}` objects are *different* members).
- Insertion order is **preserved** and iteration is deterministic.

---

## Rust Equivalent

```rust
use std::collections::HashSet;

fn main() {
    // Track which user IDs have already been seen.
    let mut seen: HashSet<u64> = HashSet::new();

    let events = [10u64, 20, 10, 30, 20, 10];
    let mut first_time = 0;
    for id in events {
        // `insert` returns `true` if the value was NOT already present.
        if seen.insert(id) {
            first_time += 1;
        }
    }

    println!("unique users: {}", seen.len()); // 3
    println!("first-time impressions: {first_time}"); // 3
    println!("has user 20: {}", seen.contains(&20)); // true

    // Set algebra — built in since Rust 1.0.
    let granted: HashSet<&str> = HashSet::from(["read", "write", "deploy"]);
    let required: HashSet<&str> = HashSet::from(["read", "deploy", "admin"]);

    let missing: Vec<&str> = required.difference(&granted).copied().collect();
    let has_all = required.is_subset(&granted);
    println!("missing: {missing:?}, has_all: {has_all}"); // missing: ["admin"], has_all: false

    // Deduplicate by collecting into a HashSet.
    let unique: HashSet<&str> = ["a", "b", "a", "c", "b"].into_iter().collect();
    println!("unique count: {}", unique.len()); // 3
}
```

Running this prints (your exact ordering for `missing` will be stable here because it has one element, but multi-element sets print in random order):

```text
unique users: 3
first-time impressions: 3
has user 20: true
missing: ["admin"], has_all: false
unique count: 3
```

**Key points:**

- `HashSet<T>` is **homogeneous**: one element type, checked at compile time.
- `insert` and `remove` return a `bool` telling you whether the set changed.
- `contains` takes a **reference** (`&20`), not the value.
- The element type must satisfy `Hash + Eq` (see [Key Differences](#key-differences)).

---

## Detailed Explanation

### Importing and constructing

`HashSet` lives in `std::collections`, so it always needs a `use` line. There is no `Set` in the prelude the way `Vec` and `String` are:

```rust
use std::collections::HashSet;

fn main() {
    // Empty, type annotated.
    let mut a: HashSet<String> = HashSet::new();
    a.insert("rust".to_string());

    // Preallocate buckets when you know roughly how many elements you'll store.
    let mut b: HashSet<u32> = HashSet::with_capacity(100);
    b.insert(1);

    // From an array literal (Rust 1.56+). Type inferred from the elements.
    let c = HashSet::from([1, 2, 3, 4]);

    // From any iterator via `collect`.
    let d: HashSet<i32> = (1..=5).collect();

    println!("{} {} {} {}", a.len(), b.len(), c.len(), d.len()); // 1 1 4 5
}
```

> **Note:** Unlike `Vec` (which has the `vec!` macro), there is no `set!` macro in the standard library. Use `HashSet::from([...])` or `.collect()`.

### `insert` and `remove` return whether the set changed

This is the single most useful difference from JavaScript's `add`/`delete`, which return the set and a boolean respectively but are rarely used for their return value. In Rust the return value is *the* idiomatic way to do "insert if new":

```rust
use std::collections::HashSet;

fn main() {
    let mut tags: HashSet<String> = HashSet::new();

    let added = tags.insert("wasm".to_string()); // true  — newly inserted
    let again = tags.insert("wasm".to_string()); // false — already present
    println!("added = {added}, again = {again}");

    let removed = tags.remove("wasm"); // true  — it was there
    let missing = tags.remove("nope"); // false — wasn't there
    println!("removed = {removed}, missing = {missing}");
}
```

Output:

```text
added = true, again = false
removed = true, missing = false
```

### Membership: `contains`, `get`, and `take`

```rust
use std::collections::HashSet;

fn main() {
    let words: HashSet<String> = HashSet::from(["rust".to_string()]);

    // `contains` borrows the query. Thanks to the `Borrow` trait you can query a
    // HashSet<String> with a &str — no allocation needed.
    println!("{}", words.contains("rust")); // true

    // `get` returns Option<&T> — useful when the stored value carries more than
    // the part you compare on.
    println!("{:?}", words.get("rust")); // Some("rust")
    println!("{:?}", words.get("go"));   // None

    // `take` removes and hands you back the owned value.
    let mut owned: HashSet<String> = HashSet::from(["a".to_string(), "b".to_string()]);
    let taken = owned.take("a"); // Some("a")
    println!("{:?}, len {}", taken, owned.len()); // Some("a"), len 1
}
```

### Set operations return lazy iterators

`union`, `intersection`, `difference`, and `symmetric_difference` do **not** allocate a new set. They return an **iterator** of references (`&T`) that you consume, typically with `.collect()` or `.copied()/.cloned()` first. This is consistent with Rust's [lazy iterator philosophy](/07-collections/06-iterators/).

```rust
use std::collections::HashSet;

fn main() {
    let a: HashSet<i32> = HashSet::from([1, 2, 3, 4]);
    let b: HashSet<i32> = HashSet::from([3, 4, 5, 6]);

    // `.copied()` turns the &i32 iterator into an i32 iterator (i32 is Copy).
    // We sort only to get deterministic output for this example.
    let mut union: Vec<i32> = a.union(&b).copied().collect();
    union.sort();
    println!("union = {union:?}"); // [1, 2, 3, 4, 5, 6]

    let mut inter: Vec<i32> = a.intersection(&b).copied().collect();
    inter.sort();
    println!("intersection = {inter:?}"); // [3, 4]

    // difference is directional: a - b (in a but not in b).
    let mut diff: Vec<i32> = a.difference(&b).copied().collect();
    diff.sort();
    println!("difference (a - b) = {diff:?}"); // [1, 2]

    // symmetric_difference: in exactly one of the two.
    let mut sym: Vec<i32> = a.symmetric_difference(&b).copied().collect();
    sym.sort();
    println!("symmetric_difference = {sym:?}"); // [1, 2, 5, 6]
}
```

Output:

```text
union = [1, 2, 3, 4, 5, 6]
intersection = [3, 4]
difference (a - b) = [1, 2]
symmetric_difference = [1, 2, 5, 6]
```

> **Tip:** When you want a new `HashSet` rather than a `Vec`, collect straight into one: `let u: HashSet<i32> = a.union(&b).copied().collect();`.

### Operator overloads on `&HashSet`

For sets of `Clone` elements, the bitwise operators are overloaded to mirror set algebra. They operate on **references** and produce an **owned** `HashSet`:

```rust
use std::collections::HashSet;

fn main() {
    let a: HashSet<i32> = HashSet::from([1, 2, 3]);
    let b: HashSet<i32> = HashSet::from([2, 3, 4]);

    let union = &a | &b;                  // union
    let intersection = &a & &b;           // intersection
    let difference = &a - &b;             // a minus b
    let symmetric = &a ^ &b;              // symmetric difference

    let mut u: Vec<i32> = union.into_iter().collect();        u.sort();
    let mut i: Vec<i32> = intersection.into_iter().collect(); i.sort();
    let mut d: Vec<i32> = difference.into_iter().collect();   d.sort();
    let mut s: Vec<i32> = symmetric.into_iter().collect();    s.sort();

    println!("a | b = {u:?}"); // [1, 2, 3, 4]
    println!("a & b = {i:?}"); // [2, 3]
    println!("a - b = {d:?}"); // [1]
    println!("a ^ b = {s:?}"); // [1, 4]
}
```

### Relationship predicates

```rust
use std::collections::HashSet;

fn main() {
    let a: HashSet<i32> = HashSet::from([1, 2, 3, 4]);
    let small: HashSet<i32> = HashSet::from([1, 2]);
    let other: HashSet<i32> = HashSet::from([9, 10]);

    println!("{}", small.is_subset(&a));   // true
    println!("{}", a.is_superset(&small)); // true
    println!("{}", a.is_disjoint(&other)); // true  — no shared elements
}
```

### In-place mutation: `extend` and `retain`

```rust
use std::collections::HashSet;

fn main() {
    // Union into an existing set without allocating a new one.
    let mut acc: HashSet<i32> = HashSet::from([1, 2, 3]);
    acc.extend([3, 4, 5]); // 3 already present, ignored

    let mut v: Vec<i32> = acc.iter().copied().collect();
    v.sort();
    println!("after extend = {v:?}"); // [1, 2, 3, 4, 5]

    // Filter in place — keep only the elements matching the predicate.
    let mut nums: HashSet<i32> = (1..=10).collect();
    nums.retain(|&n| n % 2 == 0);
    let mut evens: Vec<i32> = nums.into_iter().collect();
    evens.sort();
    println!("evens = {evens:?}"); // [2, 4, 6, 8, 10]
}
```

---

## Key Differences

| Aspect | JavaScript `Set` | Rust `HashSet<T>` |
| --- | --- | --- |
| Element types | Mixed allowed (`Set<any>`) | Homogeneous; one `T`, enforced at compile time |
| Element requirements | Any value | `T: Hash + Eq` |
| Object equality | Reference identity (`{}` ≠ `{}`) | **Value** equality via derived `Hash + Eq` |
| Iteration order | Insertion order, deterministic | **Unspecified and randomized** per run |
| `add`/`insert` return | The set itself | `bool` (was it newly inserted?) |
| `has`/`contains` | `set.has(x)` | `set.contains(&x)` (takes a reference) |
| Set algebra | `union`/`intersection`/`difference` (ES2024, Node 22+) | Built in since 1.0, plus `|`, `&`, `-`, `^` operators |
| Result of set ops | A new `Set` | A lazy **iterator** of `&T` you `.collect()` |

### Why `Hash + Eq`?

A hash set finds elements by hashing them into buckets and comparing for equality within a bucket. So `T` must be able to (a) produce a hash (`Hash`) and (b) be compared for total equality (`Eq`, which implies `PartialEq`). Most built-in types — integers, `bool`, `char`, `String`, `&str`, tuples and `Vec`s of those — already implement both. For your own structs and enums, derive them:

```rust
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UserId(u64);

fn main() {
    let mut ids: HashSet<UserId> = HashSet::new();
    ids.insert(UserId(1));
    ids.insert(UserId(1)); // value-equal, ignored
    println!("{}", ids.len()); // 1
}
```

> **Note:** Two structs are "the same element" when their derived `Hash` and `Eq` say so, i.e. all fields are equal. This is **value** equality, the opposite of JavaScript, where two distinct objects are always different `Set` members even if their fields match.

### Why is iteration order random?

Rust's default hasher (**SipHash 1-3**) is seeded with a per-process random key. This makes the collection resistant to *HashDoS* attacks, where an attacker crafts colliding keys to degrade performance. The trade-off is that you must **never** rely on iteration order. If you need ordering, use a [`BTreeSet`](/07-collections/05-btreemap-btreeset/) (sorted) or collect into a `Vec` and `sort()`.

---

## Common Pitfalls

### Pitfall 1: Using a type that isn't `Hash + Eq`

Forgetting to derive the required traits produces a confusing error: it says `insert` "exists but its trait bounds were not satisfied."

```rust
use std::collections::HashSet;

#[derive(Debug)]
struct Point { x: i32, y: i32 } // no Eq/Hash

fn main() {
    let mut seen: HashSet<Point> = HashSet::new();
    seen.insert(Point { x: 1, y: 2 }); // does not compile (error[E0599])
}
```

Real compiler output:

```text
error[E0599]: the method `insert` exists for struct `HashSet<Point>`, but its trait bounds were not satisfied
 --> src/main.rs:8:10
  |
4 | struct Point { x: i32, y: i32 }
  | ------------ doesn't satisfy `Point: Eq` or `Point: Hash`
...
8 |     seen.insert(Point { x: 1, y: 2 });
  |          ^^^^^^
  |
  = note: the following trait bounds were not satisfied:
          `Point: Eq`
          `Point: Hash`
help: consider annotating `Point` with `#[derive(Eq, Hash, PartialEq)]`
```

The fix is exactly what the compiler suggests: `#[derive(PartialEq, Eq, Hash)]`.

### Pitfall 2: Floating-point elements

`f64`/`f32` implement `PartialEq` but **not** `Eq` (because `NaN != NaN`), and they have no `Hash`. So `HashSet<f64>` does not work:

```rust
use std::collections::HashSet;

fn main() {
    let mut s: HashSet<f64> = HashSet::new();
    s.insert(3.14); // does not compile (error[E0599])
}
```

Real compiler output:

```text
error[E0599]: the method `insert` exists for struct `HashSet<f64>`, but its trait bounds were not satisfied
 --> src/main.rs:5:7
  |
5 |     s.insert(3.14);
  |       ^^^^^^
  |
  = note: the following trait bounds were not satisfied:
          `f64: Eq`
          `f64: Hash`
```

Coming from JavaScript — where `new Set([3.14])` just works — this surprises people. Store an integer key, a rounded/quantized representation, or use the `ordered-float` crate's `OrderedFloat` wrapper if you genuinely need float set membership.

### Pitfall 3: Expecting deterministic iteration order

```typescript
// JavaScript: order is the insertion order, every time.
const s = new Set(["c", "a", "b"]);
console.log([...s]); // ['c', 'a', 'b'] — always
```

In Rust, `for x in &set` visits elements in a random order that changes between program runs. Do not write tests or output that assume an order. To compare a set to an expected sequence, sort first:

```rust
use std::collections::HashSet;

fn main() {
    let s: HashSet<&str> = HashSet::from(["c", "a", "b"]);
    let mut v: Vec<&str> = s.into_iter().collect();
    v.sort();
    assert_eq!(v, ["a", "b", "c"]); // deterministic
}
```

### Pitfall 4: Passing the value instead of a reference to `contains`

`contains` (and `remove`, `get`, `take`) take a **reference**. `set.contains(20)` for a `HashSet<i32>` fails to compile; write `set.contains(&20)`. For a `HashSet<String>`, you can pass a `&str` directly (`set.contains("hi")`) thanks to the `Borrow` trait; no `String` allocation required.

---

## Best Practices

- **Reach for `HashSet` to deduplicate.** `iter.collect::<HashSet<_>>()` is the idiomatic dedup; if you also need to preserve first-seen order, keep a `Vec` and a `HashSet` together and consult the set before pushing.
- **Use the `insert` return value** to express "insert if new" in one branch instead of a separate `contains` + `insert`. The two-call version also double-hashes.
- **Derive `Hash, Eq, PartialEq`** (plus `Clone`/`Debug`) on any struct or enum you intend to put in a set.
- **Preallocate with `with_capacity`** when the size is roughly known to avoid repeated rehashing; see [collection performance](/07-collections/09-collection-performance/).
- **Choose `BTreeSet` when you need order** (sorted iteration or range queries); choose `HashSet` for the fastest membership tests. See [BTreeMap and BTreeSet](/07-collections/05-btreemap-btreeset/).
- **Collect set operations directly into the type you want.** Need a set back? `a.union(&b).copied().collect::<HashSet<_>>()`. Need a list? collect into a `Vec` and `sort()`.
- **Swap the hasher for non-adversarial hot paths.** SipHash is DoS-resistant but not the fastest; for trusted internal data, a crate like `ahash` or `rustc-hash` (`FxHashSet`) is materially faster. Keep the default for anything touching untrusted input.

---

## Real-World Example

A small access-control check: given the permissions a user has been **granted** and the permissions an action **requires**, compute exactly which permissions are missing, and deduplicate an incoming audit log of accessed resources.

```rust
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Permission(String);

/// Returns the permissions that `required` needs but `granted` lacks.
fn missing_permissions(granted: &HashSet<Permission>, required: &HashSet<Permission>) -> Vec<Permission> {
    let mut missing: Vec<Permission> = required.difference(granted).cloned().collect();
    missing.sort_by(|a, b| a.0.cmp(&b.0)); // stable output for logging
    missing
}

fn perm(name: &str) -> Permission {
    Permission(name.to_string())
}

fn main() {
    let granted: HashSet<Permission> = ["read", "write", "deploy"].into_iter().map(perm).collect();
    let required: HashSet<Permission> = ["read", "deploy", "admin"].into_iter().map(perm).collect();

    if required.is_subset(&granted) {
        println!("access granted");
    } else {
        let missing = missing_permissions(&granted, &required);
        println!("access denied; missing: {missing:?}");
    }

    // The action also accessed several resources, some repeatedly. How many
    // distinct resources were touched?
    let accessed = ["db", "cache", "db", "queue", "cache", "db"];
    let distinct: HashSet<&str> = accessed.into_iter().collect();
    println!("distinct resources touched: {}", distinct.len());

    // Which resources are "hot" (touched) but not in our known set?
    let known: HashSet<&str> = HashSet::from(["db", "cache"]);
    let mut unknown: Vec<&str> = distinct.difference(&known).copied().collect();
    unknown.sort();
    println!("unknown resources: {unknown:?}");
}
```

Output:

```text
access denied; missing: [Permission("admin")]
distinct resources touched: 3
unknown resources: ["queue"]
```

This is the kind of code where Rust's compile-time guarantees pay off: `Permission` is a distinct type (not a bare string you might mistype), the set bounds are enforced, and the set algebra reads almost like the prose specification.

---

## Further Reading

- [`std::collections::HashSet` API docs](https://doc.rust-lang.org/std/collections/struct.HashSet.html)
- [`std::collections` module overview](https://doc.rust-lang.org/std/collections/index.html): when to use which collection
- [The Rust Book, Ch. 8: Common Collections](https://doc.rust-lang.org/book/ch08-00-common-collections.html)
- Sibling pages in this section:
  - [Vectors (`Vec<T>`)](/07-collections/00-vectors/) — the growable array
  - [HashMaps (`HashMap<K, V>`)](/07-collections/03-hashmaps/): the key/value sibling of `HashSet`; a set is essentially a map with no values
  - [BTreeMap and BTreeSet](/07-collections/05-btreemap-btreeset/) — the **sorted** set when you need ordering or range queries
  - [Iterators](/07-collections/06-iterators/) and [Iterator consumers](/07-collections/07-iterator-consumers/) — how `union`/`difference` results are consumed
  - [Collection performance](/07-collections/09-collection-performance/) — Big-O and choosing the right collection
- Foundational background:
  - [Ownership](/05-ownership/) — why `contains` borrows and set ops yield `&T`
  - [Data structures](/06-data-structures/) — deriving `Hash`, `Eq`, `PartialEq`
  - [Basic types](/02-basics/01-types/) — why `f64` can't be a set element
- Next: [Error Handling](/08-error-handling/)

---

## Exercises

### Exercise 1: Count unique words

**Difficulty:** Beginner

**Objective:** Use a `HashSet` to deduplicate, ignoring case.

**Instructions:** Implement `count_unique_words(text: &str) -> usize` that returns the number of distinct words (split on whitespace), treating `"The"` and `"the"` as the same word.

```rust
use std::collections::HashSet;

fn count_unique_words(text: &str) -> usize {
    // TODO: lowercase each word and collect into a HashSet
    todo!()
}

fn main() {
    assert_eq!(count_unique_words("the cat the dog The CAT"), 3);
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashSet;

fn count_unique_words(text: &str) -> usize {
    text.split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<HashSet<String>>()
        .len()
}

fn main() {
    assert_eq!(count_unique_words("the cat the dog The CAT"), 3);
    println!("ok");
}
```

</details>

### Exercise 2: Common elements of two slices

**Difficulty:** Intermediate

**Objective:** Use set intersection to find shared elements.

**Instructions:** Implement `common(a: &[i32], b: &[i32]) -> Vec<i32>` that returns the values present in both slices, **sorted ascending** with no duplicates.

```rust
use std::collections::HashSet;

fn common(a: &[i32], b: &[i32]) -> Vec<i32> {
    // TODO: build two HashSets and intersect them
    todo!()
}

fn main() {
    assert_eq!(common(&[1, 2, 3, 4], &[3, 4, 5, 6]), vec![3, 4]);
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashSet;

fn common(a: &[i32], b: &[i32]) -> Vec<i32> {
    let set_a: HashSet<i32> = a.iter().copied().collect();
    let set_b: HashSet<i32> = b.iter().copied().collect();
    let mut out: Vec<i32> = set_a.intersection(&set_b).copied().collect();
    out.sort();
    out
}

fn main() {
    assert_eq!(common(&[1, 2, 3, 4], &[3, 4, 5, 6]), vec![3, 4]);
    println!("ok");
}
```

</details>

### Exercise 3: First duplicate in a stream

**Difficulty:** Advanced

**Objective:** Exploit the boolean return value of `insert` to detect repeats in a single pass.

**Instructions:** Implement `first_duplicate(items: &[&str]) -> Option<String>` returning the first item that appears a second time (in input order), or `None` if all items are distinct. Iterate only once.

```rust
use std::collections::HashSet;

fn first_duplicate(items: &[&str]) -> Option<String> {
    // TODO: insert each item; the first time `insert` returns false, you found it
    todo!()
}

fn main() {
    assert_eq!(first_duplicate(&["a", "b", "c", "b", "a"]), Some("b".to_string()));
    assert_eq!(first_duplicate(&["a", "b", "c"]), None);
    println!("ok");
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashSet;

fn first_duplicate(items: &[&str]) -> Option<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    for &item in items {
        // `insert` returns false when the value was already present.
        if !seen.insert(item) {
            return Some(item.to_string());
        }
    }
    None
}

fn main() {
    assert_eq!(first_duplicate(&["a", "b", "c", "b", "a"]), Some("b".to_string()));
    assert_eq!(first_duplicate(&["a", "b", "c"]), None);
    println!("ok");
}
```

</details>
