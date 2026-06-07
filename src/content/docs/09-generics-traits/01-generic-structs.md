---
title: "Generic Structs"
description: "Generic structs put type parameters on Rust data, like a generic TypeScript class. Build reusable containers, constrain impl blocks, and see monomorphization."
---

Generic structs let one `struct` definition work with many types: the direct Rust counterpart of a generic TypeScript `class` or `interface`. They are how you build reusable containers (stacks, caches, trees) without copy-pasting a version per element type, and they are everywhere in the standard library (`Vec<T>`, `HashMap<K, V>`, `Box<T>`).

---

## Quick Overview

A **generic struct** is a struct parameterized by one or more **type parameters** (written in angle brackets, like `Stack<T>`). At compile time Rust **monomorphizes** each concrete usage — it stamps out a specialized copy for `Stack<i32>`, another for `Stack<String>`, and so on — so there is zero runtime cost. This is the opposite of TypeScript, where generics are **erased** before the code ever runs. In this file we focus on generic *data structures*: declaring them, using multiple type parameters, and restricting which methods exist via **constraints on `impl` blocks**.

> **Note:** Generic *functions* (`fn largest<T>(...)`) live in [Generic Functions](/09-generics-traits/00-generic-functions/), generic *enums* (`Option<T>`, `Result<T, E>`) in [Generic Enums](/09-generics-traits/02-generic-enums/), and the trait bound syntax (`<T: Trait>`, `where`) gets its own chapter in [Trait Bounds](/09-generics-traits/05-trait-bounds/). This file uses bounds only as much as needed to constrain `impl` blocks.

---

## TypeScript/JavaScript Example

A reusable stack in TypeScript, plus a key/value pair type. This is the kind of generic container you write all the time:

```typescript
// A generic stack that works for any element type.
class Stack<T> {
  private items: T[] = [];

  push(item: T): void {
    this.items.push(item);
  }

  pop(): T | undefined {
    return this.items.pop();
  }

  peek(): T | undefined {
    return this.items[this.items.length - 1];
  }

  get size(): number {
    return this.items.length;
  }

  isEmpty(): boolean {
    return this.items.length === 0;
  }
}

const numbers = new Stack<number>();
numbers.push(1);
numbers.push(2);
numbers.push(3);
console.log("size =", numbers.size); // size = 3
console.log("peek =", numbers.peek()); // peek = 3
console.log("pop =", numbers.pop()); // pop = 3

// An interface with TWO type parameters.
interface Pair<K, V> {
  key: K;
  value: V;
}

const p: Pair<string, number> = { key: "age", value: 30 };
console.log(p); // { key: 'age', value: 30 }
```

Running this with Node v22 (`node --experimental-strip-types`) prints exactly:

```text
size = 3
peek = 3
pop = 3
{ key: 'age', value: 30 }
```

> **Note:** `console.log(p)` prints the structured object `{ key: 'age', value: 30 }`, not `[object Object]`. The `[object Object]` string only appears when an object is coerced to a string (e.g. `"" + p`).

A few things to notice about the TypeScript version, because they contrast sharply with Rust:

- The `<T>` and `<K, V>` parameters exist only for the type-checker. After `tsc` (or Node's type-stripping) runs, there is a single `Stack` whose methods accept `any`.
- `pop()` returns `T | undefined`; you must check for `undefined` yourself or risk a runtime surprise.

---

## Rust Equivalent

The same stack and pair, idiomatic Rust:

```rust
#[derive(Debug)]
struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    fn new() -> Self {
        Stack { items: Vec::new() }
    }

    fn push(&mut self, item: T) {
        self.items.push(item);
    }

    fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }

    fn peek(&self) -> Option<&T> {
        self.items.last()
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

// A struct with TWO type parameters.
#[derive(Debug)]
struct Pair<K, V> {
    key: K,
    value: V,
}

fn main() {
    let mut numbers: Stack<i32> = Stack::new();
    numbers.push(1);
    numbers.push(2);
    numbers.push(3);
    println!("len = {}", numbers.len());     // len = 3
    println!("peek = {:?}", numbers.peek()); // peek = Some(3)
    println!("pop = {:?}", numbers.pop());   // pop = Some(3)

    let p = Pair { key: "age", value: 30 };
    println!("pair = {:?}", p);              // pair = Pair { key: "age", value: 30 }
}
```

Real output, captured from `cargo run`:

```text
len = 3
peek = Some(3)
pop = Some(3)
pair = Pair { key: "age", value: 30 }
```

> **Tip:** `Vec<T>` and `Option<T>` are themselves generic structs/enums from the standard library — building `Stack<T>` on top of `Vec<T>` means the heavy lifting (growth, bounds-checking) is already done for you. `pop()` returns `Option<T>`, the type-system-enforced version of TypeScript's `T | undefined`. See [The Option Type](/06-data-structures/03-option-enum/).

---

## Detailed Explanation

Let's walk through the Rust version line by line and contrast it with the TypeScript.

### Declaring the type parameter on the struct

```rust
struct Stack<T> {
    items: Vec<T>,
}
```

`<T>` introduces a **type parameter** named `T` (any identifier works; `T`, `K`, `V`, `E`, `Item` are conventions). Inside the braces, `T` stands for "whatever type the caller picks." `Vec<T>` says: a growable vector whose elements are that same `T`. This mirrors `private items: T[]` in TypeScript, but in Rust the `T` is a genuine part of the type's identity, not erased decoration.

### Declaring the type parameter on the `impl` block

```rust
impl<T> Stack<T> {
    // methods...
}
```

This is the line most surprising to a TypeScript developer. You must write `<T>` **twice**: once right after `impl` to *declare* the parameter, and once in `Stack<T>` to *use* it. In TypeScript the methods live inside the `class` body, so the `<T>` from `class Stack<T>` is automatically in scope. In Rust, `impl` blocks are separate items, so each one re-declares the generics it needs.

Read it as: "*For any type `T`, here are the methods on `Stack<T>`.*"

> **Warning:** Forgetting the `<T>` after `impl` is the #1 beginner mistake; see [Common Pitfalls](#common-pitfalls) for the exact compiler error.

### `Self` and the constructor

```rust
fn new() -> Self {
    Stack { items: Vec::new() }
}
```

`Self` is an alias for "the type this `impl` is for": here, `Stack<T>`. Rust has no `new` keyword and no constructors; by convention you write an **associated function** called `new` (associated functions are covered in [Associated Functions and Constructors](/06-data-structures/06-associated-functions/)). The caller writes `Stack::new()` instead of TypeScript's `new Stack<number>()`.

### Method receivers: `&self`, `&mut self`, `self`

```rust
fn push(&mut self, item: T) { ... }  // needs to mutate -> &mut self
fn peek(&self) -> Option<&T> { ... } // only reads -> &self
fn swap(self) -> Pair<V, K> { ... }  // consumes the value -> self (by value)
```

In TypeScript every method implicitly receives a mutable `this`. In Rust you choose the **borrow** explicitly: `&self` for read-only access, `&mut self` to mutate, or `self` (by value) to consume. The compiler enforces this: calling `push` on a non-`mut` binding is a compile error. This is ownership, covered in [Section 05](/05-ownership/).

Note `peek(&self) -> Option<&T>`: it returns a **borrow** of the top element (`&T`), not a copy, and wraps it in `Option` to encode "the stack might be empty." TypeScript's `peek(): T | undefined` is the moral equivalent, but Rust makes the borrow and the emptiness both explicit and checked.

### Multiple type parameters

```rust
struct Pair<K, V> {
    key: K,
    value: V,
}
```

You can have as many parameters as you like, separated by commas, exactly like `interface Pair<K, V>` in TypeScript. `K` and `V` are independent: `Pair<String, i32>`, `Pair<i32, i32>`, and `Pair<bool, Vec<u8>>` are all distinct, fully separate types after monomorphization.

### Type inference fills in the parameters

In `let p = Pair { key: "age", value: 30 };` we never wrote `Pair<&str, i32>`. Rust infers `K = &str` and `V = i32` from the field values, just as TypeScript infers them from the object literal. For the stack we *did* annotate (`let mut numbers: Stack<i32>`) because `Stack::new()` starts empty and gives the compiler nothing to infer from. More on that in the pitfalls.

---

## Key Differences

### Monomorphization vs type erasure

This is the deepest conceptual difference. When you use `Stack<i32>` and `Stack<String>`, the Rust compiler generates **two separate, specialized structs and method sets**, as if you had hand-written `StackI32` and `StackString`. Each is compiled to machine code that knows the exact size and layout of its element type. There is no boxing, no `any`, no runtime type tag.

TypeScript does the opposite: after compilation, generics vanish entirely. There is one `Stack` at runtime, and `T` is effectively `any`. You cannot ask "what was `T`?" at runtime, and there is no per-instantiation code.

| Aspect | TypeScript generics | Rust generics |
| --- | --- | --- |
| When resolved | Compile-time only, then **erased** | Compile-time, then **monomorphized** |
| Runtime representation | One class; `T` ≈ `any` | A distinct specialized type per concrete `T` |
| Runtime type info | None (cannot inspect `T`) | None needed; type is baked into the code |
| Performance | Same as untyped JS | Zero-cost; as fast as hand-specialized code |
| Code size | One copy | One copy **per concrete instantiation** (can grow the binary) |
| `instanceof Stack<number>` | Not possible (no such check) | Not applicable — types are static |

> **Note:** The trade-off for monomorphization's speed is **code bloat**: using a generic struct with 20 different element types produces 20 copies of its methods in the binary. This is usually fine; when it matters, trait objects (`Box<dyn Trait>`) trade some speed for a single shared copy. See [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/).

### `derive` is conditional on the type parameter

```rust
#[derive(Debug, Clone, Default, PartialEq)]
struct Point<T> {
    x: T,
    y: T,
}
```

When you `#[derive(...)]` on a generic struct, Rust generates an implementation that is **conditional**: `Point<T>` is `Clone` *only when `T: Clone`*, `Debug` *only when `T: Debug`*, and so on. You get exactly the capabilities your `T` supports — no more, no less. The following compiles and runs:

```rust
#[derive(Debug, Clone, Default, PartialEq)]
struct Point<T> {
    x: T,
    y: T,
}

fn main() {
    let a = Point { x: 1, y: 2 };
    let b = a.clone();
    println!("{:?} == {:?} -> {}", a, b, a == b);

    let origin: Point<f64> = Point::default();
    println!("origin = {:?}", origin);
}
```

Real output:

```text
Point { x: 1, y: 2 } == Point { x: 1, y: 2 } -> true
origin = Point { x: 0.0, y: 0.0 }
```

TypeScript has no equivalent: there is no automatic structural equality, cloning, or default-value generation tied to a type parameter.

### One type parameter means *one* type

`Pair<K, V>` has two parameters, so `key` and `value` can differ. But in `Pair2<T> { first: T, second: T }`, **both** fields must be the *same* type. TypeScript enforces the identical rule, but because errors there are often deferred or widened, Rust's version tends to surface the mismatch more bluntly (see Pitfall 2).

---

## Common Pitfalls

### Pitfall 1: Forgetting `<T>` after `impl`

The single most common mistake. You write the struct's `impl` but only put `<T>` in the type, not after the `impl` keyword:

```rust
struct Container<T> {
    item: T,
}

// does not compile (error[E0412]: cannot find type `T` in this scope)
impl Container<T> {
    fn get(&self) -> &T {
        &self.item
    }
}
```

The real compiler error:

```text
error[E0412]: cannot find type `T` in this scope
 --> src/main.rs:6:16
  |
6 | impl Container<T> {
  |                ^ not found in this scope
  |
help: you might be missing a type parameter
  |
6 | impl<T> Container<T> {
  |     +++
```

**Fix:** declare the parameter after `impl`: `impl<T> Container<T> { ... }`. The compiler's `help` even shows the exact insertion. Remember: the first `<T>` *declares*, the second *uses*.

### Pitfall 2: Expecting one type parameter to hold two types

```rust
struct Pair<T> {
    first: T,
    second: T,
}

fn main() {
    // does not compile (error[E0308]: mismatched types)
    let p = Pair { first: 5, second: "hello" };
    let _ = p;
}
```

Real error:

```text
error[E0308]: mismatched types
 --> src/main.rs:8:38
  |
8 |     let p = Pair { first: 5, second: "hello" };
  |                                      ^^^^^^^ expected integer, found `&str`
```

**Fix:** if the two fields can be different types, give the struct two parameters: `struct Pair<T, U> { first: T, second: U }`. With a single `T`, both fields are locked to the same concrete type.

### Pitfall 3: The compiler can't infer the type parameter

A generic constructor that produces an *empty* container gives Rust nothing to infer `T` from:

```rust
struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    fn new() -> Self {
        Stack { items: Vec::new() }
    }
    fn len(&self) -> usize {
        self.items.len()
    }
}

fn main() {
    let s = Stack::new(); // does not compile (error[E0282]: type annotations needed)
    println!("{}", s.len());
}
```

Real error:

```text
error[E0282]: type annotations needed for `Stack<_>`
  --> src/main.rs:15:9
   |
15 |     let s = Stack::new(); // nothing tells the compiler what T is
   |         ^   ------------ type must be known at this point
   |
help: consider giving `s` an explicit type, where the type for type parameter `T` is specified
   |
15 |     let s: Stack<T> = Stack::new(); // nothing tells the compiler what T is
   |          ++++++++++
```

**Fix:** annotate the binding (`let s: Stack<i32> = Stack::new();`), use the turbofish (`Stack::<i32>::new()`), or push an element so inference has something to work with. This is the analogue of `new Stack<number>()` in TypeScript; Rust just needs the hint at a slightly different spot. (The turbofish `::<>` is covered in [Generic Functions](/09-generics-traits/00-generic-functions/).)

### Pitfall 4: Calling a method that only exists for *some* instantiations

If you add a method in a constrained `impl` (next section), it exists only for the matching types. Calling it on a different instantiation fails:

```rust
struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    fn new() -> Self { Stack { items: Vec::new() } }
    fn push(&mut self, item: T) { self.items.push(item); }
}

// `total()` exists only for Stack<i32>.
impl Stack<i32> {
    fn total(&self) -> i32 { self.items.iter().sum() }
}

fn main() {
    let mut words: Stack<String> = Stack::new();
    words.push("hi".to_string());
    let _ = words.total(); // does not compile (error[E0599]: no method named `total`)
}
```

Real error:

```text
error[E0599]: no method named `total` found for struct `Stack<String>` in the current scope
  --> src/main.rs:18:19
   |
 1 | struct Stack<T> {
   | --------------- method `total` not found for this struct
...
18 |     let _ = words.total(); // total() exists only for Stack<i32>
   |                   ^^^^^ method not found in `Stack<String>`
   |
   = note: the method was found for
           - `Stack<i32>`
```

This is a *feature*, not a limitation: the compiler tells you the method exists, just for a different type. There is no TypeScript equivalent of "this method exists only when `T = number`" with a clean compile-time error like this.

---

## Best Practices

### Constrain `impl` blocks, not the struct definition

Prefer to put trait bounds on the `impl` block (or individual methods) rather than on the `struct` itself:

```rust
// Idiomatic: the struct stays unconstrained...
struct Wrapper<T> {
    value: T,
}

impl<T> Wrapper<T> {
    fn new(value: T) -> Self {
        Wrapper { value }
    }
}

// ...and bounds gate only the methods that actually need them.
impl<T: std::fmt::Display + PartialOrd> Wrapper<T> {
    fn announce_larger(&self, other: &T) {
        if self.value >= *other {
            println!("{} is the largest", self.value);
        } else {
            println!("{} is the largest", other);
        }
    }
}

fn main() {
    let w = Wrapper::new(42);
    w.announce_larger(&10); // 42 is the largest
}
```

Output: `42 is the largest`.

This way you can still construct `Wrapper<SomethingNotDisplay>`; you just can't call `announce_larger` on it. Putting `T: Display` on the `struct` would forbid the whole type for non-`Display` `T`, which is almost never what you want.

> **Warning:** Rust strongly discourages bounds directly on struct definitions (e.g. `struct Wrapper<T: Display>`). They infect every `impl` and signature that mentions the struct without buying real safety, and the community lint guidance is to omit them. Keep bounds where they are used.

### Use `where` clauses when bounds get long

When a constrained `impl` has several bounds, a `where` clause reads better than stacking them in the angle brackets:

```rust
use std::collections::HashMap;
use std::hash::Hash;

struct Cache<K, V> {
    store: HashMap<K, V>,
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn new() -> Self {
        Cache { store: HashMap::new() }
    }
}
fn main() { let _c: Cache<String, i32> = Cache::new(); }
```

The full `where`-clause story is in [Trait Bounds](/09-generics-traits/05-trait-bounds/).

### Name parameters meaningfully

`T` for a single "the element," `K`/`V` for key/value, `E` for an error type, `Item`/`Output` when a descriptive name aids readability. Single uppercase letters are the convention; resist `TElement`-style Hungarian names that TypeScript codebases sometimes use.

### Reach for the standard library's generic types first

Before writing your own generic container, check whether `Vec<T>`, `VecDeque<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`, or `Box<T>` already does it. Your `Stack<T>` above is a thin, type-safe wrapper over `Vec<T>`, which is exactly the right amount of code to write.

---

## Real-World Example

A production-flavored generic cache: a bounded key/value store with eviction, reused for two completely different element types. The constrained `impl` requires `K: Eq + Hash + Clone` (so we can use a `HashMap` and clone a key to evict) and `V: Clone`.

```rust
use std::collections::HashMap;
use std::hash::Hash;

/// A bounded in-memory cache keyed by `K`, storing `V`, with a max capacity.
struct Cache<K, V> {
    store: HashMap<K, V>,
    capacity: usize,
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn new(capacity: usize) -> Self {
        Cache {
            store: HashMap::new(),
            capacity,
        }
    }

    /// Inserts a value, evicting an entry first if we are at capacity.
    /// Returns the previous value for `key`, if any.
    fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.store.len() >= self.capacity && !self.store.contains_key(&key) {
            // Evict an arbitrary existing entry to stay within capacity.
            if let Some(victim) = self.store.keys().next().cloned() {
                self.store.remove(&victim);
            }
        }
        self.store.insert(key, value)
    }

    fn get(&self, key: &K) -> Option<&V> {
        self.store.get(key)
    }

    fn len(&self) -> usize {
        self.store.len()
    }
}

#[derive(Debug, Clone)]
struct Session {
    user_id: u64,
    token: String,
}

fn main() {
    // 1) A cache of String -> Session, capacity 2.
    let mut sessions: Cache<String, Session> = Cache::new(2);
    sessions.insert(
        "alice".to_string(),
        Session { user_id: 1, token: "tok_a".to_string() },
    );
    sessions.insert(
        "bob".to_string(),
        Session { user_id: 2, token: "tok_b".to_string() },
    );
    sessions.insert(
        "carol".to_string(),
        Session { user_id: 3, token: "tok_c".to_string() },
    );

    println!("cache size = {}", sessions.len()); // stays at capacity
    if let Some(s) = sessions.get(&"carol".to_string()) {
        println!("carol -> user {} token {}", s.user_id, s.token);
    }

    // 2) The SAME generic type, now caching u32 -> u32. No new code.
    let mut squares: Cache<u32, u32> = Cache::new(10);
    for n in 1..=5 {
        squares.insert(n, n * n);
    }
    println!("4^2 = {:?}", squares.get(&4));
}
```

Real output, captured from `cargo run`:

```text
cache size = 2
carol -> user 3 token tok_c
4^2 = Some(16)
```

The headline: one `Cache<K, V>` definition serves both a `String`-keyed session store and a `u32`-keyed squares table, and each is monomorphized into specialized, allocation-free-dispatch code. The bounds on the `impl` (`Eq + Hash + Clone`, `Clone`) document precisely what a key and value must support: the type system rejects, at compile time, any `K` that can't be a `HashMap` key.

> **Tip:** If you reach for an owned heap-allocated container that must be shared or have a single owner with indirection, the smart pointers in [Section 10](/10-smart-pointers/) (`Box<T>`, `Rc<T>`, `RefCell<T>`) are themselves generic structs — they compose cleanly with your own.

---

## Further Reading

### Official Documentation

- [The Rust Book — Generic Data Types](https://doc.rust-lang.org/book/ch10-01-syntax.html) (the "In Struct Definitions" and "In Method Definitions" subsections directly cover this file's scope)
- [Rust by Example — Generics](https://doc.rust-lang.org/rust-by-example/generics.html)
- [Rust Reference — Generic parameters](https://doc.rust-lang.org/reference/items/generics.html)
- [The Rust Book — Const generics / `const` parameters](https://doc.rust-lang.org/reference/items/generics.html#const-generics) (used in Exercise 3)

### Related Sections in This Guide

- [Generic Functions](/09-generics-traits/00-generic-functions/): generic functions, monomorphization vs erasure, the turbofish `::<>`
- [Generic Enums](/09-generics-traits/02-generic-enums/) — generic enums, with `Option<T>` and `Result<T, E>` as the canonical examples
- [Trait Bounds](/09-generics-traits/05-trait-bounds/) — the full grammar of `<T: Trait>`, multiple bounds, and `where` clauses
- [Traits](/09-generics-traits/03-traits/): defining and implementing traits (the "interfaces" your bounds refer to)
- [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/) — `Box<dyn Trait>` when you want one shared copy instead of monomorphization
- [Structs](/06-data-structures/00-structs/) and [Methods and `impl` Blocks](/06-data-structures/05-impl-blocks/) — non-generic structs and `impl` basics
- [The Option Type](/06-data-structures/03-option-enum/): `Option<T>`, returned by `pop()` and `peek()` above
- [Section 05: Ownership](/05-ownership/) — why method receivers are `&self` / `&mut self` / `self`
- [Section 02: Basic Types](/02-basics/01-types/) — the concrete types (`i32`, `String`, ...) that fill in your parameters

---

## Exercises

### Exercise 1: A mappable wrapper

**Difficulty:** Easy

**Objective:** Practice declaring a generic struct and a generic *method* that changes the type parameter.

**Instructions:** Define `Wrapper<T>` holding a single `value: T`. Give it a `new(value: T)` constructor and a `map` method that takes a closure `F: FnOnce(T) -> U` and returns a `Wrapper<U>`. Then wrap a number and `map` it into a wrapped `String`.

```rust
struct Wrapper<T> {
    value: T,
}

impl<T> Wrapper<T> {
    fn new(value: T) -> Self {
        /* ??? */
    }

    fn map</* ??? */>(self, f: F) -> Wrapper<U> {
        /* ??? */
    }
}

fn main() {
    let w = Wrapper::new(5);
    let s = w.map(|n| format!("n = {n}"));
    println!("{}", s.value); // n = 5
}
```

<details>
<summary>Solution</summary>

```rust
struct Wrapper<T> {
    value: T,
}

impl<T> Wrapper<T> {
    fn new(value: T) -> Self {
        Wrapper { value }
    }

    // `map` introduces a NEW type parameter `U` (the closure's output)
    // and a closure type `F`. It consumes `self` (note `self`, not `&self`)
    // because the value is moved into the closure.
    fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Wrapper<U> {
        Wrapper { value: f(self.value) }
    }
}

fn main() {
    let w = Wrapper::new(5);
    let s = w.map(|n| format!("n = {n}"));
    println!("{}", s.value); // n = 5
}
```

Output: `n = 5`. The method-level `<U, F>` is declared on the method, separate from the struct's `<T>`. This is the per-`impl`/per-method generic declaration rule in action.

</details>

### Exercise 2: A key/value store with a constrained convenience method

**Difficulty:** Medium

**Objective:** Build a two-parameter generic struct and add a method that exists *only* when the value type supports more traits: the "constraints on impls" idea.

**Instructions:** Define `Store<K, V>` wrapping a `HashMap<K, V>`. In an `impl` bounded by `K: Eq + Hash`, add `new`, `set`, and `get`. In a *second*, more constrained `impl` (`V: Clone + Default`), add `get_or_default(&self, k: &K) -> V` that returns a clone of the stored value or `V::default()` if the key is missing.

```rust
use std::collections::HashMap;
use std::hash::Hash;

struct Store<K, V> {
    map: HashMap<K, V>,
}

// impl 1: basic operations
impl</* ??? */> Store<K, V> {
    fn new() -> Self { /* ??? */ }
    fn set(&mut self, k: K, v: V) { /* ??? */ }
    fn get(&self, k: &K) -> Option<&V> { /* ??? */ }
}

// impl 2: only when V: Clone + Default
impl</* ??? */> Store<K, V> {
    fn get_or_default(&self, k: &K) -> V { /* ??? */ }
}

fn main() {
    let mut store: Store<String, i32> = Store::new();
    store.set("hits".to_string(), 7);
    println!("hits = {:?}", store.get(&"hits".to_string())); // hits = Some(7)
    println!("misses = {}", store.get_or_default(&"misses".to_string())); // misses = 0
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;
use std::hash::Hash;

struct Store<K, V> {
    map: HashMap<K, V>,
}

// Basic operations need K to be a valid HashMap key.
impl<K: Eq + Hash, V> Store<K, V> {
    fn new() -> Self {
        Store { map: HashMap::new() }
    }
    fn set(&mut self, k: K, v: V) {
        self.map.insert(k, v);
    }
    fn get(&self, k: &K) -> Option<&V> {
        self.map.get(k)
    }
}

// This method exists ONLY when V is Clone + Default.
impl<K: Eq + Hash, V: Clone + Default> Store<K, V> {
    fn get_or_default(&self, k: &K) -> V {
        self.map.get(k).cloned().unwrap_or_default()
    }
}

fn main() {
    let mut store: Store<String, i32> = Store::new();
    store.set("hits".to_string(), 7);
    println!("hits = {:?}", store.get(&"hits".to_string())); // hits = Some(7)
    println!("misses = {}", store.get_or_default(&"misses".to_string())); // misses = 0
}
```

Output:

```text
hits = Some(7)
misses = 0
```

`get_or_default` is available because `i32` is both `Clone` and `Default` (its default is `0`). Build a `Store` whose `V` lacks `Default` and the basic methods still work, but `get_or_default` simply isn't there: the same constrained-`impl` behavior as Pitfall 4.

</details>

### Exercise 3: A fixed-capacity ring buffer with const generics

**Difficulty:** Hard

**Objective:** Use a **const generic** parameter so the capacity is part of the type, with zero heap allocation.

**Instructions:** Define `RingBuffer<T, const N: usize>` holding `data: [Option<T>; N]` and a `head: usize`. In an `impl` bounded by `T: Copy` (so the `[None; N]` array literal works), add `new`, `push` (wrapping with `% N`), and `capacity()` returning `N`. Const generics let the size live in the type, so `RingBuffer<i32, 3>` and `RingBuffer<i32, 8>` are distinct types whose buffers are stack-allocated.

```rust
struct RingBuffer<T, const N: usize> {
    data: [Option<T>; N],
    head: usize,
}

impl<T: Copy, const N: usize> RingBuffer<T, N> {
    fn new() -> Self { /* ??? */ }
    fn push(&mut self, item: T) { /* ??? */ }
    fn capacity(&self) -> usize { /* ??? */ }
}

fn main() {
    let mut rb: RingBuffer<i32, 3> = RingBuffer::new();
    rb.push(10);
    rb.push(20);
    println!("capacity = {}, first = {:?}", rb.capacity(), rb.data[0]);
    // capacity = 3, first = Some(10)
}
```

<details>
<summary>Solution</summary>

```rust
struct RingBuffer<T, const N: usize> {
    data: [Option<T>; N],
    head: usize,
}

impl<T: Copy, const N: usize> RingBuffer<T, N> {
    fn new() -> Self {
        // `[None; N]` requires the element type (Option<T>) to be Copy,
        // which holds because we bounded T: Copy.
        RingBuffer { data: [None; N], head: 0 }
    }

    fn push(&mut self, item: T) {
        self.data[self.head % N] = Some(item);
        self.head += 1;
    }

    fn capacity(&self) -> usize {
        N
    }
}

fn main() {
    let mut rb: RingBuffer<i32, 3> = RingBuffer::new();
    rb.push(10);
    rb.push(20);
    println!("capacity = {}, first = {:?}", rb.capacity(), rb.data[0]);
    // capacity = 3, first = Some(10)
}
```

Output: `capacity = 3, first = Some(10)`.

`const N: usize` is a **const generic** — a value (not a type) baked into the type signature. The whole buffer lives inline on the stack with no `Vec`, and the compiler knows its exact size. This is a capability with no TypeScript analogue: in TypeScript, array length is never part of a generic type parameter you can compute over.

</details>
