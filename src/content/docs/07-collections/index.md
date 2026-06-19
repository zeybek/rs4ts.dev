---
title: "Collections"
sidebar:
  label: "Overview"
description: "JS Array, string, Map, and Set become Rust's typed Vec, String/&str, HashMap, and BTreeMap, reshaped by lazy iterators that do no work until you ask."
---

In TypeScript you keep data in `Array`, `string`, `Object`/`Map`, and `Set`, and you reshape it with array methods like `.map()`, `.filter()`, and `.reduce()`. Rust ships the same shapes as concrete, typed standard-library collections — `Vec<T>`, `String`/`&str`, `HashMap<K, V>`, `HashSet<T>`, and the sorted `BTreeMap`/`BTreeSet` — plus a sharper tool for processing them: the **lazy iterator** system. This section maps each TypeScript habit onto its idiomatic Rust collection, then shows how iterator adaptors and consumers replace your array-method muscle memory while doing strictly less work (nothing runs until you ask for a result).

---

## What You'll Learn

- How a JavaScript `Array` becomes a typed, growable `Vec<T>`: `push`/`pop`/`get`/indexing, iteration, the `vec!` macro, and explicit **capacity** so you can pre-allocate
- Why Rust splits the one JavaScript `string` into the owned, growable **`String`** and the borrowed view **`&str`**, and what UTF-8 and byte-boundary slicing mean for "indexing" a string
- The everyday `String`/`&str` methods (`split`, `trim`, `replace`, `parse`, `chars()` vs `bytes()`) and the idiomatic ways to build strings without re-allocating on every concatenation
- How a plain object or `Map` becomes a **`HashMap<K, V>`** whose keys and values are typed and owned, lookups return `Option<&V>` instead of `undefined`, and updates go through the `entry` API
- How a JavaScript `Set` becomes a **`HashSet<T>`** with first-class `union`/`intersection`/`difference`/`symmetric_difference` set algebra
- When to reach for the **sorted** collections **`BTreeMap`**/**`BTreeSet`** — guaranteed key order and `O(log n)` range queries — instead of their hashing cousins
- Why Rust's iterator adaptors (`map`, `filter`, `take`, `skip`, `zip`, `enumerate`) are **lazy**, doing no work until a consumer pulls values through them: the opposite of eager JavaScript array methods
- The **consuming adaptors** that finish a chain (`collect`, `fold`, `sum`, `count`, `find`, `any`/`all`, `min`/`max`, `reduce`) and how they line up with `reduce`/`find`/`some`/`every`
- How to build your own lazy data producer by implementing the **`Iterator`** trait (one `next` method) and **`IntoIterator`** so `for` loops work on your type
- How to choose a collection deliberately using its documented **Big-O** cost model, pre-allocate with `with_capacity`, and why an iterator chain compiles to essentially the same machine code as a hand-written loop

---

## Topics

| Topic | Description |
| --- | --- |
| [Vectors](/07-collections/00-vectors/) | `Array` → `Vec<T>`: `push`/`pop`/`get`/indexing, iteration, capacity and growth, and the `vec!` macro. |
| [Strings: `String` vs `&str`](/07-collections/01-strings/) | The critical split of one JS `string` into owned `String` vs borrowed `&str`: UTF-8, ownership, and slicing on byte boundaries. |
| [String Manipulation](/07-collections/02-string-manipulation/) | Common `String`/`&str` methods: `split`/`trim`/`replace`/`parse`, `chars()` vs `bytes()`, and building strings efficiently. |
| [HashMaps](/07-collections/03-hashmaps/) | Object/`Map` → `HashMap<K, V>`: `insert`/`get`, the `entry` API, iteration, and ownership of keys and values. |
| [Sets and HashSet](/07-collections/04-hashsets/) | `Set` → `HashSet<T>`: membership tests and the `union`/`intersection`/`difference` set operations. |
| [Sorted Collections: BTreeMap and BTreeSet](/07-collections/05-btreemap-btreeset/) | The sorted collections `BTreeMap`/`BTreeSet`: guaranteed ordering, range queries, and how they compare to `HashMap`. |
| [Iterators](/07-collections/06-iterators/) | Array methods → iterator adaptors: **lazy** evaluation, and `map`/`filter`/`take`/`skip`/`zip`/`enumerate`. |
| [Iterator Consumers](/07-collections/07-iterator-consumers/) | The consuming adaptors that run a chain: `collect`, `fold`, `sum`, `count`, `find`, `any`/`all`, `min`/`max`, `reduce`. |
| [Custom Iterators](/07-collections/08-custom-iterators/) | Implementing the `Iterator` trait, `impl Iterator` for your own type, and `IntoIterator` for `for`-loop support. |
| [Collection Performance](/07-collections/09-collection-performance/) | Big-O characteristics, when to use `Vec`/`HashMap`/`BTreeMap`, capacity preallocation, and iterator vs loop. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Choose the right container for a given job — `Vec` for ordered lists, `String`/`&str` for text, `HashMap`/`HashSet` for keyed lookup and uniqueness, `BTreeMap`/`BTreeSet` when order matters — and justify the choice with its cost model
- Explain the `String` vs `&str` distinction, take a `&str` parameter for cheap borrowing, and slice strings safely on UTF-8 byte boundaries
- Translate JavaScript array-method chains (`.map().filter().reduce()`) into idiomatic Rust iterator pipelines, and explain why the Rust version is lazy and allocation-free until consumed
- Use the `entry` API to insert-or-update map values in a single lookup, and handle missing keys through the `Option` that lookups return instead of relying on `undefined`
- Pick a consumer (`collect`, `fold`, `sum`, `find`, `any`/`all`, `max`) to finish a chain, and recognize the compiler warning you get when an iterator is built but never consumed
- Implement the `Iterator` and `IntoIterator` traits to make your own type loopable and inherit the entire adaptor toolbox
- Pre-allocate with `with_capacity`/`reserve` where it pays off, and reason about when a manual loop and an iterator chain are equivalent

---

## Prerequisites

- [Section 05: Ownership](/05-ownership/): collections **own** their elements; pushing into a `Vec` *moves* a value in, indexing *borrows*, and `into_iter()` *consumes* the collection. The `String` vs `&str` split is ownership applied to text, so be comfortable with moves, borrows, and `Clone` first.
- [Section 06: Data Structures](/06-data-structures/): collections hold your `struct`s and `enum`s, lookups return [`Option<T>`](/06-data-structures/03-option-enum/), and you will pattern-match on the results. Knowing structs, enums, and `Option` makes this section click.
- [Section 02: Basics](/02-basics/): concrete types (`usize`, `i64`, `f64`), immutability-by-default and `let mut`, and `{:?}` debug formatting all show up throughout.

---

## Estimated Time

- **Reading:** 5-6 hours
- **Hands-on Practice:** 4-5 hours
- **Exercises:** 3 hours
- **Total:** 12-14 hours

> **Tip:** Read the topics in order. Start with the *containers* — `Vec`, then strings, then the maps and sets — so you know what shapes hold your data. Then learn the *iterator system* (`iterators` → `iterator-consumers` → `custom-iterators`), which is how you transform every one of those containers with the same vocabulary. Finish with `collection-performance` to tie the choices together. The single biggest mental shift for a TypeScript developer is **laziness**: `arr.map(...)` in JavaScript allocates a new array immediately, but a Rust iterator adaptor does *nothing* until a consumer runs it.


---

## Frequently asked questions

### What is the Rust equivalent of a JavaScript array?

`Vec<T>`, a growable, heap-allocated sequence of one element type. Unlike a JS array it cannot mix types and is indexed by `usize`. A fixed-size `[T; N]` array lives on the stack. See [Vectors](/07-collections/00-vectors/).

### Why don't my `.map()` and `.filter()` calls run?

Iterator adaptors are lazy: they build a recipe and do nothing until a consumer drives them. Add `.collect()`, `.sum()`, or a `for` loop to pull the values through. See [Iterators](/07-collections/06-iterators/) and [Iterator Consumers](/07-collections/07-iterator-consumers/).

### What replaces a JavaScript object or `Map`?

`HashMap<K, V>` for dynamic keys, or a `struct` for a fixed shape. `map.get(&key)` returns `Option<&V>`, so a missing key is handled at compile time instead of yielding `undefined`. See [HashMaps](/07-collections/03-hashmaps/).

---

**Next:** [Section 08: Error Handling →](/08-error-handling/) — `Result`, `Option`, the `?` operator, and custom error types, replacing the `throw`/`try`/`catch` you used in TypeScript.
