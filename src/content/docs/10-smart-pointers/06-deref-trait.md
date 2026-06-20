---
title: "The `Deref` Trait and Deref Coercion"
description: "The Deref trait lets a Rust smart pointer act like the value it wraps. It explains why &String works where &str is expected and why you rarely write * on a Box."
---

The `Deref` trait is the mechanism that lets a smart pointer behave like the value it points to. It is also the quiet machinery behind one of the first "magic" things a TypeScript developer notices in Rust: why a `&String` works where a `&str` is expected, and why you almost never have to write `*` to call a method on a `Box`.

---

## Quick Overview

In JavaScript and TypeScript there is no concept of dereferencing: a variable that holds an object *is* the reference, and `obj.method()` always reaches the object directly. Rust separates a value from a pointer to that value, so it needs a rule for "follow the pointer." The **`Deref`** trait defines that rule, and the compiler uses it to perform **deref coercion**: automatically converting `&Wrapper` into `&Inner` (and `&Inner` into `&InnerInner`, as deep as needed) at method-call and argument-passing sites.

For a working TypeScript developer, the payoff is concrete: `Deref` is why `Box<T>`, `Rc<T>`, `String`, and `Vec<T>` all let you call the inner type's methods directly, and why passing `&my_string` to a `fn(&str)` "just works."

---

## TypeScript/JavaScript Example

TypeScript has no dereference operator and no user-definable "act like the thing I wrap" hook. The closest everyday experiences are: (1) accessing a wrapped value works transparently because there is no wrapper to see through, and (2) string primitives auto-box to `String` objects so you can call methods on them.

```typescript
// In TypeScript a "wrapper" has to expose the inner API by hand —
// there is no language hook that says "treat me as my inner value."
class Username {
  constructor(private readonly value: string) {}

  // You must manually re-expose every method you want callers to use...
  get length(): number {
    return this.value.length;
  }
  toUpperCase(): string {
    return this.value.toUpperCase();
  }
  // ...or expose the raw string and make callers reach in:
  unwrap(): string {
    return this.value;
  }
}

function logLine(label: string, value: string): void {
  console.log(`${label}: ${value}`);
}

const user = new Username("ada_lovelace");

// A `Username` is NOT a `string`, so this is a type error in TS:
// logLine("user", user);          // Argument of type 'Username' is not assignable to 'string'
logLine("user", user.unwrap()); // you must unwrap explicitly
console.log(user.length); // 12 — only works because we hand-wrote a getter
console.log(user.toUpperCase()); // works only because we re-implemented it

// Separately: JS string *primitives* auto-box to String objects,
// which is the one place JS does something "deref-like" for you.
const s = "hello";
console.log(s.toUpperCase()); // HELLO — the primitive is boxed transiently
```

**Key points:**

- A `Username` wrapper has to manually forward every method, or force callers to `unwrap()`.
- There is no way to say "a `Username` can be used anywhere a `string` is expected."
- The only built-in "transparent" behavior is primitive auto-boxing (`"hello".toUpperCase()`), and you cannot extend it to your own types.

---

## Rust Equivalent

Rust lets your wrapper opt into "act like my inner value" by implementing `Deref`. After that, the inner type's methods are callable directly, and a `&Wrapper` coerces into a `&Inner` at call sites. No manual forwarding, no `unwrap()`.

```rust playground
use std::ops::Deref;

/// A newtype guaranteeing the wrapped string is non-empty and trimmed.
#[derive(Debug, Clone)]
struct Username(String);

impl Username {
    fn new(raw: &str) -> Result<Username, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            Err("username must not be empty".to_string())
        } else {
            Ok(Username(trimmed.to_string()))
        }
    }
}

// Deref to `str` exposes the entire string-slice API for free.
impl Deref for Username {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

fn log_line(label: &str, value: &str) {
    println!("{label}: {value}");
}

fn main() {
    let user = Username::new("  ada_lovelace  ").expect("valid");

    log_line("user", &user); // &Username coerces to &str — no unwrap
    println!("len   = {}", user.len()); // str::len, straight through
    println!("upper = {}", user.to_uppercase()); // str::to_uppercase, straight through
    println!("ada?  = {}", user.starts_with("ada"));
}
```

**Output:**

```text
user: ada_lovelace
len   = 12
upper = ADA_LOVELACE
ada?  = true
```

The single `impl Deref` replaces all the hand-written forwarding the TypeScript version needed. The validating constructor still guarantees the invariant; `Deref` only governs *read* access to the inner `str`.

> **Note:** Implementing `Deref` to expose an inner API is acceptable for thin **newtype** wrappers like this, but it is _not_ a general inheritance mechanism. See [Common Pitfalls](#common-pitfalls) and [Best Practices](#best-practices). Use it deliberately.

---

## Detailed Explanation

### What `Deref` actually is

`Deref` is a trait from `std::ops` with one associated type and one method:

```rust
// (Definition from the standard library, shown for reference.)
pub trait Deref {
    type Target: ?Sized;
    fn deref(&self) -> &Self::Target;
}
```

- `Target` is the type you "become" when dereferenced.
- `deref(&self) -> &Target` returns a **reference** to the inner value (never a moved/owned value; that is important for the borrow checker).

When you write `*value` and `value` is not a plain reference, the compiler rewrites it as `*(value.deref())`. That is: call `deref()` to get a `&Target`, then apply the built-in `*` to that reference. So `*boxed_string` means `*(boxed_string.deref())`.

### Deref coercion: the automatic part

The genuinely useful behavior is **deref coercion**. In two specific situations the compiler will *automatically* insert `.deref()` calls, as many as needed, to make types line up:

1. **Argument passing / reference site:** if you have a `&T` and a function wants `&U`, and `T: Deref<Target = U>` (transitively), the compiler inserts the coercion.
2. **Method receiver resolution:** when you call `value.method()`, the compiler tries `value`, then `*value`, then `**value`, and so on, following `Deref` impls, until it finds a type that has `method`.

This is why all of the following work without a single explicit `*`:

```rust playground
use std::rc::Rc;

fn main() {
    // Box<T> implements Deref<Target = T>, so String's methods are reachable.
    let boxed = Box::new(String::from("hello"));
    println!("len via box = {}", boxed.len()); // String::len through Box
    println!("upper = {}", boxed.to_uppercase()); // String::to_uppercase through Box

    // Rc<T> also implements Deref (read-only).
    let shared = Rc::new(vec![10, 20, 30]);
    println!("first = {:?}", shared.first()); // <[i32]>::first through Rc -> Vec -> slice
    println!("sum = {}", shared.iter().sum::<i32>());

    // Three different smart pointers, all coerce to &str for one &str function:
    fn shout(s: &str) -> String {
        s.to_uppercase()
    }
    let a = String::from("box");
    let b = Box::new(String::from("rc"));
    let c = Rc::new(String::from("string"));
    println!("{} {} {}", shout(&a), shout(&b), shout(&c));
}
```

**Output:**

```text
len via box = 5
upper = HELLO
first = Some(10)
sum = 60
BOX RC STRING
```

Notice the `shared.first()` case chains coercions: `Rc<Vec<i32>>` → `Vec<i32>` → `[i32]` (a slice), and `first` lives on the slice. The compiler walks the whole `Deref` chain for you.

### Why `&String` works where `&str` is expected

This is the canonical example, and it is pure deref coercion. The standard library implements `impl Deref for String { type Target = str; ... }`. So when you call a `fn greet(name: &str)` with a `&String`, the compiler sees the mismatch, finds `String: Deref<Target = str>`, and inserts the coercion automatically.

```rust playground
fn greet(name: &str) {
    println!("Hello, {name}!");
}

fn main() {
    let owned: String = String::from("Ada");

    greet(&owned); // &String -> &str, coercion inserted by the compiler
    greet("Grace"); // a literal is already &str

    // Box<String> -> &String -> &str (two coercions in a row):
    let boxed: Box<String> = Box::new(String::from("Linus"));
    greet(&boxed);

    // The hand-written equivalents the compiler is sparing you:
    greet(owned.as_str()); // explicit &str
    greet(&owned[..]); // explicit full-range slice
}
```

**Output:**

```text
Hello, Ada!
Hello, Grace!
Hello, Linus!
Hello, Ada!
Hello, Ada!
```

> **Tip:** Prefer `&str` over `&String` for function parameters _because_ of this coercion. A `fn(&str)` accepts string literals, `&String`, `&Box<String>`, and slices; a `fn(&String)` accepts only an owned `String` behind a reference. This is covered more in [Functions](/03-functions/).

### `DerefMut`: the mutable counterpart

`Deref` gives `&Target`. To get `&mut Target` (for mutation through the wrapper, or to mutate behind a `Box`), implement `DerefMut`:

```rust playground
use std::ops::{Deref, DerefMut};

struct MyBox<T>(T);

impl<T> MyBox<T> {
    fn new(x: T) -> MyBox<T> {
        MyBox(x)
    }
}

impl<T> Deref for MyBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for MyBox<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

fn main() {
    let b = MyBox::new(5);
    println!("*b = {}", *b); // Deref: *(b.deref())

    let mut words = MyBox::new(vec!["a", "b"]);
    words.push("c"); // DerefMut coercion: (*words).push("c")
    println!("len = {}", words.len()); // Deref coercion: (*words).len()
    println!("{:?}", *words);
}
```

**Output:**

```text
*b = 5
len = 3
["a", "b", "c"]
```

`DerefMut` requires `Deref` as a supertrait (you must have both), and coercion of mutable references (`&mut T` → `&mut U`) follows the same rules through `DerefMut`.

### How `Box` deref differs slightly

`Box<T>` participates in deref coercion exactly like the examples above, so `boxed.method()` and `greet(&boxed)` work. `Box<T>` has one extra power the others lack: because the compiler knows a `Box` is the *unique* owner, dereferencing an **owned** box (`let inner = *boxed;`) can *move* the value out of the heap. That is a special compiler rule for `Box`, not something `Deref` itself provides; `deref()` only ever returns a `&Target`. The owning-move behavior is covered in [Box&lt;T&gt;](/10-smart-pointers/00-box/); here, focus on the shared-reference coercion that all smart pointers share.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust with `Deref` |
| --- | --- | --- |
| Dereferencing | No concept; a variable is the reference | `*value` follows the pointer (sugar for `*value.deref()`) |
| "Act like inner value" | Manual method forwarding or `.unwrap()` | One `impl Deref` exposes the inner API |
| `&String` vs `&str` | n/a (one `string` type) | Coercion makes `&String` usable as `&str` |
| Auto-conversion at call site | Only primitive auto-boxing, not extensible | Deref coercion, user-extensible per type |
| Mutable access | Always available on the object | Separate `DerefMut` trait, opt-in |
| Method lookup | Single prototype chain | Compiler walks the `Deref` chain (`value`, `*value`, `**value`, ...) |
| Where it triggers | n/a | Method receivers and `&`/`&mut` argument sites only |

The mental shift: in TypeScript "transparency" is the default and you _can't_ customize it. In Rust, transparency is opt-in (`impl Deref`), explicit about read vs. write (`Deref` vs `DerefMut`), and applies only in the two well-defined coercion situations: never to operators like `==`, never to generic trait bounds.

### Where the `&String`/`&str` analogy breaks down

Unlike TypeScript's single `string`, Rust deliberately has two types: `String` (owned, growable, heap) and `str` / `&str` (a borrowed view). `Deref<Target = str>` is the bridge that makes them interoperate smoothly, but they remain distinct types. You cannot, for example, push to a `&str` (it is a read-only view), and a function returning `&str` is promising not to allocate. The coercion smooths the ergonomics; it does not erase the distinction the way TypeScript's single type does.

---

## Common Pitfalls

### Pitfall 1: Expecting deref coercion to satisfy a generic trait bound

Coercion happens for **references at call sites** and **method receivers** — _not_ for generic type parameters. A `Box<String>` does **not** implement a trait just because `String` does.

```rust
trait Greet {
    fn greet(&self) -> String;
}

impl Greet for String {
    fn greet(&self) -> String {
        format!("hi {self}")
    }
}

fn run<T: Greet>(t: T) -> String {
    t.greet()
}

fn main() {
    let boxed = Box::new(String::from("ada"));
    println!("{}", run(boxed)); // does not compile (error[E0277]: Box<String>: Greet not satisfied)
}
```

The real compiler error:

```text
error[E0277]: the trait bound `Box<String>: Greet` is not satisfied
  --> src/main.rs:19:24
   |
19 |     println!("{}", run(boxed));
   |                    --- ^^^^^ the trait `Greet` is not implemented for `Box<String>`
   |                    |
   |                    required by a bound introduced by this call
   |
note: required by a bound in `run`
  --> src/main.rs:11:11
   |
11 | fn run<T: Greet>(t: T) -> String {
   |           ^^^^^ required by this bound in `run`
help: consider dereferencing here
   |
19 |     println!("{}", run(*boxed));
   |                        +
```

The fix is in the help text: dereference explicitly (`run(*boxed)`) so you pass a real `String`. Deref coercion will not do this for you across a generic bound.

### Pitfall 2: Mutating through a wrapper that only implements `Deref`

If you implement `Deref` but forget `DerefMut`, you get read access but not write access through the wrapper.

```rust
use std::ops::Deref;

struct Wrapper(Vec<i32>);

impl Deref for Wrapper {
    type Target = Vec<i32>;
    fn deref(&self) -> &Vec<i32> {
        &self.0
    }
}

fn main() {
    let mut w = Wrapper(vec![1, 2, 3]);
    println!("len = {}", w.len()); // ok: Deref gives &Vec
    w.push(4); // does not compile (error[E0596]: needs DerefMut)
    println!("{:?}", *w);
}
```

The real compiler error (trimmed):

```text
error[E0596]: cannot borrow data in dereference of `Wrapper` as mutable
  --> src/main.rs:15:5
   |
15 |     w.push(4);                     // needs DerefMut, which is not implemented
   |     ^ cannot borrow as mutable
   |
   = help: trait `DerefMut` is required to modify through a dereference, but it is not implemented for `Wrapper`
```

The error message names the exact fix: implement `DerefMut for Wrapper`.

### Pitfall 3: Assuming operators like `==` use deref coercion

Deref coercion applies to method calls and reference arguments — **not** to binary operators. `==` is governed by the `PartialEq` trait, which is not subject to coercion.

```rust
use std::ops::Deref;

struct MyBox<T>(T);

impl<T> Deref for MyBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

fn main() {
    let b = MyBox(String::from("hi"));
    let same = b == String::from("hi"); // does not compile (error[E0369])
    println!("{same}");
}
```

The real compiler error:

```text
error[E0369]: binary operation `==` cannot be applied to type `MyBox<String>`
  --> src/main.rs:10:18
   |
10 |     let same = b == String::from("hi");
   |                - ^^ ------------------ String
   |                |
   |                MyBox<String>
   |
note: an implementation of `PartialEq<String>` might be missing for `MyBox<String>`
```

To compare, dereference explicitly (`*b == String::from("hi")`) or derive/implement `PartialEq` on the wrapper.

### Pitfall 4: Using `Deref` as a substitute for inheritance

It is tempting, coming from class-based TypeScript, to give a "subclass-like" struct a `Deref` to a "base" struct so the base's methods leak through. The Rust community considers this an **anti-pattern**: it confuses method resolution, surprises readers (`self_type.some_method()` may belong to the target), and breaks down because traits are not inherited via `Deref`. `Deref` is for smart pointers and thin newtypes whose `Target` genuinely "is" the value — not for modeling "is-a" relationships. Prefer composition with explicit delegation, or traits, instead.

---

## Best Practices

- **Implement `Deref` only for smart-pointer-like types and thin newtypes.** Good targets: a guard that wraps one value, a validated newtype that should behave like its inner `str`/slice. Bad target: a domain struct you want to "inherit" from another.
- **Choose the most useful `Target`.** For a string newtype, `Deref<Target = str>` (not `String`) exposes the slice API and coerces further to `&str`. For a `Vec` newtype, `Deref<Target = [T]>` gives the slice API while hiding mutating `Vec` methods.
- **Implement `DerefMut` only when mutation through the wrapper is genuinely intended**, and remember it requires `Deref` as a supertrait.
- **Prefer `&str` and `&[T]` parameters over `&String`/`&Vec<T>`.** Deref coercion makes the slice forms strictly more flexible callers-side. (See [Functions](/03-functions/).)
- **Never return an owned value from `deref`.** The signature is `&self -> &Target`; trying to compute and return a fresh value each call is a sign you want a method or a `From`/`Into` conversion instead.
- **Reach for `AsRef<T>` / `Into<T>` for explicit conversions.** If you only need a one-off conversion (not transparent pointer-like behavior), `AsRef`/`From`/`Into` express intent more honestly than `Deref`.

---

## Real-World Example

A common production pattern is a **bounded stack**: a collection with its own `push`/`pop` policy that nonetheless wants to offer the full read-only slice API (`len`, `iter`, `first`, `contains`, ...) without re-implementing each method. `Deref<Target = [T]>` delivers exactly that: the wrapper controls mutation, while reads flow through to the slice.

```rust playground
use std::ops::Deref;

/// A stack with a fixed capacity. It owns its `push`/`pop` policy, but
/// `Deref`s to `[T]` so every read-only slice method is available for free.
struct BoundedStack<T> {
    items: Vec<T>,
    capacity: usize,
}

impl<T> BoundedStack<T> {
    fn new(capacity: usize) -> Self {
        BoundedStack { items: Vec::new(), capacity }
    }

    /// Push, or hand the rejected item back if the stack is full.
    fn push(&mut self, item: T) -> Result<(), T> {
        if self.items.len() == self.capacity {
            Err(item)
        } else {
            self.items.push(item);
            Ok(())
        }
    }

    fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }
}

// Deref to a slice: the whole read-only `[T]` API, for free.
impl<T> Deref for BoundedStack<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.items
    }
}

fn main() {
    let mut stack: BoundedStack<i32> = BoundedStack::new(2);
    println!("push 1: {:?}", stack.push(1));
    println!("push 2: {:?}", stack.push(2));
    println!("push 3: {:?}", stack.push(3)); // full -> Err(3)

    // Everything below comes from `[T]` via deref coercion:
    println!("len      = {}", stack.len());
    println!("is_empty = {}", stack.is_empty());
    println!("first    = {:?}", stack.first());
    println!("contains = {}", stack.contains(&2));
    let total: i32 = stack.iter().sum();
    println!("sum      = {total}");

    println!("pop = {:?}", stack.pop());
    println!("len = {}", stack.len());
}
```

**Output:**

```text
push 1: Ok(())
push 2: Ok(())
push 3: Err(3)
len      = 2
is_empty = false
first    = Some(1)
contains = true
sum      = 3
pop = Some(2)
len = 1
```

The `BoundedStack` exposes a deliberately small *mutating* surface (`push` enforces the capacity, `pop` is the only other writer) while inheriting the entire read API of a slice. Because the `Target` is `[T]` rather than `Vec<T>`, callers cannot bypass the policy by calling `Vec::push` through the wrapper: the slice type has no such method.

---

## Further Reading

### Official documentation

- [The Rust Book — Treating Smart Pointers Like Regular References with `Deref`](https://doc.rust-lang.org/book/ch15-02-deref.html)
- [`std::ops::Deref` API docs](https://doc.rust-lang.org/std/ops/trait.Deref.html)
- [`std::ops::DerefMut` API docs](https://doc.rust-lang.org/std/ops/trait.DerefMut.html)
- [Rust by Example — `Deref`](https://doc.rust-lang.org/rust-by-example/trait/deref.html)
- [Rust API Guidelines — Smart pointers do not add inherent methods (C-SMART-PTR)](https://rust-lang.github.io/api-guidelines/predictability.html#smart-pointers-do-not-add-inherent-methods-c-smart-ptr)

### Related topics in this guide

- [Section 10 overview](/10-smart-pointers/): the full map of smart pointers
- [Box&lt;T&gt;](/10-smart-pointers/00-box/): `Box<T>` and its special owned-deref move
- [Shared Ownership with `Rc<T>` and `Arc<T>`](/10-smart-pointers/01-rc-arc/): `Rc`/`Arc` also implement `Deref` for read access
- [Interior Mutability](/10-smart-pointers/02-refcell-mutex/): interior mutability; `borrow()`/`lock()` return deref-able guards
- [Clone-on-Write with `Cow<'_, T>`](/10-smart-pointers/04-cow/) — `Cow` implements `Deref` to its borrowed/owned target
- [Choosing a Smart Pointer](/10-smart-pointers/07-comparison/) — decision guide: which smart pointer to pick when
- [Functions](/03-functions/) — why `&str` parameters beat `&String` (deref coercion in action)
- [Ownership](/05-ownership/) — references and borrowing, the foundation `Deref` builds on
- [Generics & Traits](/09-generics-traits/) — traits and trait bounds (and why coercion does not satisfy them)
- [Async](/11-async/) — `Pin` and deref show up together when working with futures

---

## Exercises

### Exercise 1: A sentence newtype that acts like a `str`

**Difficulty:** Beginner

**Objective:** Implement `Deref` so a wrapper type can be used wherever a `&str` is expected and can call `str` methods directly.

**Instructions:** Define `struct Sentence(String)`. Implement `Deref<Target = str>` for it. Then write a free function `fn word_count(s: &str) -> usize` and call it by passing `&sentence` (relying on deref coercion). Also call `to_uppercase()` and `len()` directly on the `Sentence` value.

```rust
use std::ops::Deref;

struct Sentence(String);

impl Deref for Sentence {
    type Target = str;
    fn deref(&self) -> &str {
        /* ??? */
    }
}

fn word_count(s: &str) -> usize {
    // TODO: count whitespace-separated words
}

fn main() {
    let s = Sentence(String::from("the quick brown fox"));
    println!("words = {}", word_count(&s)); // expect 4
    println!("upper = {}", s.to_uppercase());
    println!("len   = {}", s.len());
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::ops::Deref;

struct Sentence(String);

impl Deref for Sentence {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

fn main() {
    let s = Sentence(String::from("the quick brown fox"));
    // Deref coercion: &Sentence -> &str
    println!("words = {}", word_count(&s));
    // str methods straight through the wrapper:
    println!("upper = {}", s.to_uppercase());
    println!("len   = {}", s.len());
}
```

**Output:**

```text
words = 4
upper = THE QUICK BROWN FOX
len   = 19
```

The single `impl Deref` is all it takes: `&Sentence` coerces to `&str` for `word_count`, and `to_uppercase`/`len` resolve onto the `str` target via method-receiver deref.

</details>

### Exercise 2: A read-counting smart pointer with `Deref` and `DerefMut`

**Difficulty:** Intermediate

**Objective:** Build a real smart pointer that observes how often its inner value is read, demonstrating both `Deref` and `DerefMut`.

**Instructions:** Define `struct Logged<T>` holding the value and a read counter. Implement `Deref` so each call to `deref` bumps the counter, and `DerefMut` so the value can be mutated. Use `std::cell::Cell<u32>` for the counter so you can mutate it from `&self` (see [Cell&lt;T&gt;](/10-smart-pointers/03-cell/)). Add a `read_count(&self) -> u32` method. Show that calling `.len()`, `.first()`, and `.iter()` on a wrapped `Vec` each count as a read, and that `*counter += 1` works via `DerefMut`.

<details>
<summary>Solution</summary>

```rust playground
use std::cell::Cell;
use std::ops::{Deref, DerefMut};

struct Logged<T> {
    value: T,
    reads: Cell<u32>,
}

impl<T> Logged<T> {
    fn new(value: T) -> Self {
        Logged { value, reads: Cell::new(0) }
    }
    fn read_count(&self) -> u32 {
        self.reads.get()
    }
}

impl<T> Deref for Logged<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.reads.set(self.reads.get() + 1); // Cell lets us mutate from &self
        &self.value
    }
}

impl<T> DerefMut for Logged<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

fn main() {
    let logged = Logged::new(vec![1, 2, 3]);
    println!("len   = {}", logged.len()); // deref #1
    println!("first = {:?}", logged.first()); // deref #2
    let _sum: i32 = logged.iter().sum(); // deref #3
    println!("reads = {}", logged.read_count());

    let mut counter = Logged::new(0u32);
    *counter += 1; // DerefMut
    *counter += 1; // DerefMut
    println!("counter = {}", *counter);
}
```

**Output:**

```text
len   = 3
first = Some(1)
reads = 3
counter = 2
```

Each method call on `logged` goes through `deref` once, so the counter lands on `3`. `DerefMut` is what makes `*counter += 1` legal — without it you would hit `error[E0594]`/`E0596`. Using `Cell` for the counter is the idiomatic way to mutate bookkeeping state from the `&self` signature `deref` requires.

</details>

### Exercise 3: A guard that deref-coerces to its protected value

**Difficulty:** Intermediate / Advanced

**Objective:** Model the "RAII guard" shape (like `MutexGuard` or `Ref`) where a wrapper grants temporary access to an inner value via `Deref`/`DerefMut` and runs cleanup on drop.

**Instructions:** Define `struct Resource { name: String, data: Vec<i32> }`. Define `struct Guard<'a>(&'a mut Resource)` that borrows it. Implement `Deref<Target = Resource>` and `DerefMut` for `Guard` so callers can read and mutate the `Resource` through the guard. Implement `Drop` for `Guard` to print `"released <name>"`. In `main`, acquire a guard, push to `data` through it (`guard.data.push(...)` works via `DerefMut`), read `data.len()`, then let the guard drop and observe the cleanup message.

<details>
<summary>Solution</summary>

```rust playground
use std::ops::{Deref, DerefMut};

struct Resource {
    name: String,
    data: Vec<i32>,
}

struct Guard<'a>(&'a mut Resource);

impl<'a> Deref for Guard<'a> {
    type Target = Resource;
    fn deref(&self) -> &Resource {
        self.0
    }
}

impl<'a> DerefMut for Guard<'a> {
    fn deref_mut(&mut self) -> &mut Resource {
        self.0
    }
}

impl<'a> Drop for Guard<'a> {
    fn drop(&mut self) {
        println!("released {}", self.0.name);
    }
}

impl Resource {
    fn acquire(&mut self) -> Guard<'_> {
        println!("acquired {}", self.name);
        Guard(self)
    }
}

fn main() {
    let mut resource = Resource {
        name: "db-pool".to_string(),
        data: vec![1, 2],
    };

    {
        let mut guard = resource.acquire();
        // DerefMut: reach Resource.data through the guard and mutate it.
        guard.data.push(3);
        guard.data.push(4);
        // Deref: read through the guard.
        println!("len while held = {}", guard.data.len());
    } // guard drops here -> "released db-pool"

    println!("final data = {:?}", resource.data);
}
```

**Output:**

```text
acquired db-pool
len while held = 4
released db-pool
final data = [1, 2, 3, 4]
```

This is the exact shape of `std::sync::MutexGuard` and `std::cell::RefMut`: a short-lived handle that `Deref`s to the protected value and cleans up in `Drop`. The lifetime `'a` ties the guard to the borrow of `Resource`, so the borrow checker guarantees the guard cannot outlive what it protects. Compare this with the guards returned by `RefCell::borrow_mut` and `Mutex::lock` in [Interior Mutability](/10-smart-pointers/02-refcell-mutex/).

</details>
