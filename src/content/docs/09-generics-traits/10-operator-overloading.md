---
title: "Operator Overloading"
description: "Rust operators are trait methods: a + b means a.add(b), so implementing Add, Mul, or Index opts your types into syntax JavaScript and TypeScript never let"
---

In TypeScript and JavaScript, `+`, `-`, `*`, and `[]` are baked into the language: you cannot teach `+` how to add two of your own objects. Rust takes the opposite stance. Every operator is sugar for a **trait method**, so `a + b` literally means `a.add(b)`, and you opt your own types into that syntax by implementing the right trait.

---

## Quick Overview

Rust's arithmetic, comparison, and indexing operators are defined by traits in the standard library's `std::ops` (and `std::cmp`) modules. Implementing `Add` for your `Vector` type makes `v1 + v2` compile; implementing `Index` makes `matrix[(row, col)]` work. This matters to a TypeScript/JavaScript developer because it removes a whole class of awkward `.add()` / `.times()` method chains and lets numeric, geometric, and money types read like the math they model, while the compiler still enforces exact types and ownership.

> **Note:** Rust does **not** let you invent brand-new operators or change an operator's precedence/associativity. You can only give existing operators (`+`, `-`, `*`, `/`, `%`, `&`, `|`, `^`, `<<`, `>>`, unary `-`, unary `!`, `[]`) new meanings for your own types.

---

## TypeScript/JavaScript Example

In JavaScript there is no operator overloading. The closest you get is naming methods like `add` and `scale`, then calling them explicitly. The `+` operator on objects does something, but almost never what you want.

```typescript
// TypeScript - a 2D vector with explicit methods (no operator overloading)
class Vector2 {
  x: number;
  y: number;

  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }

  add(other: Vector2): Vector2 {
    return new Vector2(this.x + other.x, this.y + other.y);
  }

  scale(s: number): Vector2 {
    return new Vector2(this.x * s, this.y * s);
  }

  toString(): string {
    return `(${this.x}, ${this.y})`;
  }
}

const a = new Vector2(1, 2);
const b = new Vector2(3, 4);

console.log(`a.add(b)   = ${a.add(b)}`);
console.log(`a.scale(2) = ${a.scale(2)}`);

// What does `+` actually do to two objects?
console.log("a + b      =", (a as unknown as number) + (b as unknown as number));
```

Running this under Node v22 prints:

```text
a.add(b)   = (4, 6)
a.scale(2) = (2, 4)
a + b      = (1, 2)(3, 4)
```

**Key points:**

- You must write and call `a.add(b)`; there is no way to make `a + b` mean vector addition.
- `a + b` does not error; JavaScript coerces both objects to strings via `toString()` and **concatenates** them, giving the surprising `"(1, 2)(3, 4)"`. If `toString()` were absent you would get `"[object Object][object Object]"`. Either way it is a silent bug, not a type error.
- TypeScript's type system cannot rescue you here: `+` on two `Vector2` values is a type error, so the example above needs an `as unknown as number` cast to even compile — exactly the kind of escape hatch that hides the runtime nonsense.

---

## Rust Equivalent

In Rust, `+` is sugar for the `Add` trait. Implement `Add` for `Vector2` and `a + b` simply works, with full type checking and zero runtime surprises.

```rust
// Rust - a 2D vector that supports the `+` operator
use std::ops::Add;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Vector2 {
    x: f64,
    y: f64,
}

impl Add for Vector2 {
    type Output = Vector2; // the result type of `self + rhs`

    fn add(self, other: Vector2) -> Vector2 {
        Vector2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

fn main() {
    let a = Vector2 { x: 1.0, y: 2.0 };
    let b = Vector2 { x: 3.0, y: 4.0 };

    let c = a + b; // desugars to Add::add(a, b)
    println!("{:?}", c);
    println!("equal? {}", c == Vector2 { x: 4.0, y: 6.0 });
}
```

Real output from `cargo run`:

```text
Vector2 { x: 4.0, y: 6.0 }
equal? true
```

**Key points:**

- `a + b` is rewritten by the compiler to `Add::add(a, b)`. The operator is just a method call with nice syntax.
- `type Output = Vector2;` is an **associated type**: it names what `+` returns. It does not have to be `Self` (e.g. `Vector2 * f64` can return `Vector2`, and a dot-product could return `f64`).
- The `#[derive(PartialEq)]` is what makes `==` work — comparison operators come from `std::cmp`, separate from arithmetic. More on that below.

---

## Detailed Explanation

### Operators are traits

Every overloadable operator maps to one trait method. Here is the core table:

| Operator        | Trait (`std::ops` unless noted) | Method            |
| --------------- | ------------------------------- | ----------------- |
| `a + b`         | `Add`                           | `add`             |
| `a - b`         | `Sub`                           | `sub`             |
| `a * b`         | `Mul`                           | `mul`             |
| `a / b`         | `Div`                           | `div`             |
| `a % b`         | `Rem`                           | `rem`             |
| `-a`            | `Neg`                           | `neg`             |
| `!a`            | `Not`                           | `not`             |
| `a += b`        | `AddAssign`                     | `add_assign`      |
| `a -= b`        | `SubAssign`                     | `sub_assign`      |
| `a & b`         | `BitAnd`                        | `bitand`          |
| `a \| b`        | `BitOr`                         | `bitor`           |
| `a ^ b`         | `BitXor`                        | `bitxor`          |
| `a << b`        | `Shl`                           | `shl`             |
| `a >> b`        | `Shr`                           | `shr`             |
| `a[i]` (read)   | `Index`                         | `index`           |
| `a[i] = v`      | `IndexMut`                      | `index_mut`       |
| `a == b`        | `PartialEq` (`std::cmp`)        | `eq`              |
| `a < b`         | `PartialOrd` (`std::cmp`)       | `lt`              |

> The comparison operators (`<`, `>`, `<=`, `>=`) each desugar to `lt`/`gt`/`le`/`ge`. Those have default bodies built on `partial_cmp`, the trait's single *required* method, so you normally implement only `partial_cmp` (or `#[derive(PartialOrd)]`) and get all four operators for free.

When you write `a + b`, the compiler resolves it to `<TypeOfA as Add<TypeOfB>>::add(a, b)`. If no matching `Add` impl exists, you get a compile error — never a silent string concatenation.

### Walking through the `Add` impl

```rust
use std::ops::Add;

impl Add for Vector2 {
    type Output = Vector2;
    fn add(self, other: Vector2) -> Vector2 { /* ... */ }
}
```

- `impl Add for Vector2` is shorthand for `impl Add<Vector2> for Vector2`. `Add` has a generic parameter `Rhs` (the right-hand side) that **defaults to `Self`**, so for same-type addition you can omit it.
- `type Output` is the associated type that the `add` method returns.
- `fn add(self, other: Vector2)` takes `self` **by value**: it consumes both operands. For `Copy` types like `Vector2` (note the `#[derive(Copy)]`) this is free; for owned types like `String` or `Vec`, this means `a + b` *moves* `a` and `b`. We address that in Common Pitfalls.

### A different right-hand side: scaling by a scalar

`Add` defaults `Rhs` to `Self`, but you can set `Rhs` to a different type. A common case is multiplying a vector by a scalar `f64`:

```rust
use std::ops::Mul;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Vector2 {
    x: f64,
    y: f64,
}

// Vector2 * f64  -> Vector2
impl Mul<f64> for Vector2 {
    type Output = Vector2;
    fn mul(self, scalar: f64) -> Vector2 {
        Vector2 { x: self.x * scalar, y: self.y * scalar }
    }
}

// f64 * Vector2  -> Vector2  (the operands flipped)
impl Mul<Vector2> for f64 {
    type Output = Vector2;
    fn mul(self, v: Vector2) -> Vector2 {
        Vector2 { x: self * v.x, y: self * v.y }
    }
}

fn main() {
    let v = Vector2 { x: 1.0, y: 2.0 };
    println!("v * 2.0 = {:?}", v * 2.0);
    println!("2.0 * v = {:?}", 2.0 * v);
}
```

Output:

```text
v * 2.0 = Vector2 { x: 2.0, y: 4.0 }
2.0 * v = Vector2 { x: 2.0, y: 4.0 }
```

> **Important:** `v * 2.0` and `2.0 * v` are **two different impls**. Operators are not automatically commutative: `a * b` only resolves to `Mul::mul(a, b)`, never `Mul::mul(b, a)`. If you want both orders, you write both impls (the left-hand operand's type owns the impl, which is why `impl Mul<Vector2> for f64` lives "on" `f64`).

### Compound assignment, negation, and indexing

The assignment operators take `&mut self` and return `()`. Unary `-` is `Neg`. Indexing returns a *reference* so the compiler can also support `a[i] = v` through `IndexMut`. Here they are together:

```rust
use std::ops::{AddAssign, Neg, Index, IndexMut};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Vector2 {
    x: f64,
    y: f64,
}

impl AddAssign for Vector2 {
    fn add_assign(&mut self, rhs: Vector2) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Neg for Vector2 {
    type Output = Vector2;
    fn neg(self) -> Vector2 {
        Vector2 { x: -self.x, y: -self.y }
    }
}

// Read access: `v[0]` and `v[1]`
impl Index<usize> for Vector2 {
    type Output = f64;
    fn index(&self, i: usize) -> &f64 {
        match i {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("Vector2 index out of range: {i}"),
        }
    }
}

// Write access: `v[0] = ...`
impl IndexMut<usize> for Vector2 {
    fn index_mut(&mut self, i: usize) -> &mut f64 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => panic!("Vector2 index out of range: {i}"),
        }
    }
}

fn main() {
    let mut v = Vector2 { x: 1.0, y: 2.0 };
    v += Vector2 { x: 10.0, y: 20.0 };
    println!("after += : {:?}", v);
    println!("negated  : {:?}", -v);
    println!("v[0] = {}, v[1] = {}", v[0], v[1]);
    v[0] = 99.0;
    println!("after v[0] = 99: {:?}", v);
}
```

Output:

```text
after += : Vector2 { x: 11.0, y: 22.0 }
negated  : Vector2 { x: -11.0, y: -22.0 }
v[0] = 11, v[1] = 22
after v[0] = 99: Vector2 { x: 99.0, y: 22.0 }
```

> **Note:** `Index::index` returns `&Self::Output`, not `Self::Output`. That reference is what lets `v[0]` be used both for reading and (via `IndexMut`) as the target of an assignment. This is the same machinery `Vec<T>` and `HashMap<K, V>` use — see [the collections section](/07-collections/).

### Comparison operators are a separate family

`==`, `!=`, `<`, `>`, `<=`, `>=` are **not** in `std::ops`. They come from `PartialEq`/`Eq` and `PartialOrd`/`Ord` in `std::cmp`. You almost never hand-write these; you `#[derive(...)]` them:

```rust
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u32,
    minor: u32,
}
```

`#[derive(PartialEq)]` gives you `==`/`!=`; `#[derive(PartialOrd)]` gives you `<`/`>`/`<=`/`>=` using lexicographic field order. This is why the very first `Vector2` example could write `c == Vector2 { .. }` after only deriving `PartialEq`. (Floating point gets `PartialEq`/`PartialOrd` but not `Eq`/`Ord`, because `NaN != NaN`.)

---

## Key Differences

| Aspect                        | TypeScript / JavaScript                              | Rust                                                          |
| ----------------------------- | ---------------------------------------------------- | ------------------------------------------------------------- |
| Custom `+` on your types      | Impossible; use named methods (`a.add(b)`)           | Implement the `Add` trait; `a + b` just works                 |
| `+` on two objects            | Silent coercion to strings (`toString`)              | Compile error unless an `Add` impl exists                     |
| What an operator *is*         | Built-in syntax                                      | Sugar for a trait method (`a + b` == `Add::add(a, b)`)        |
| Result type                   | Always whatever JS coercion produces                 | Chosen by you via the `Output` associated type                |
| Commutativity                 | N/A                                                  | Not automatic — `a * b` and `b * a` are separate impls        |
| Equality (`==`)               | `===` compares references for objects                | `==` uses your `PartialEq` impl (value equality)              |
| Invent new operators          | No                                                   | No; only existing operators can be overloaded                 |
| Where you can implement       | N/A                                                  | Subject to the **orphan rule** (see below)                    |

### Why design it this way?

Rust's "operators are traits" model is the same idea as TypeScript's "iterables implement `Symbol.iterator`," generalized to every operator. It means the language core stays tiny, your numeric types are first-class citizens (a `BigDecimal` from a crate adds with `+` exactly like `i32` does), and generic code can be written over "anything that supports `+`" via the bound `T: Add<Output = T>`:

```rust
use std::ops::Add;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Point<T> {
    x: T,
    y: T,
}

// `+` works for any component type T that itself supports `+`
impl<T: Add<Output = T>> Add for Point<T> {
    type Output = Point<T>;
    fn add(self, rhs: Point<T>) -> Point<T> {
        Point { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

fn main() {
    println!("{:?}", Point { x: 1, y: 2 } + Point { x: 3, y: 4 });
    println!("{:?}", Point { x: 1.5, y: 2.5 } + Point { x: 0.5, y: 0.5 });
}
```

Output:

```text
Point { x: 4, y: 6 }
Point { x: 2.0, y: 3.0 }
```

That `T: Add<Output = T>` is a **trait bound**. The broader story of bounds lives in [Trait Bounds](/09-generics-traits/05-trait-bounds/), and generic structs like `Point<T>` are covered in [Generic Structs](/09-generics-traits/01-generic-structs/).

---

## Common Pitfalls

### Pitfall 1: Forgetting that `add(self, ...)` *moves* its operands

For non-`Copy` types, `a + b` consumes both `a` and `b`. Reusing them afterward fails to compile.

```rust
use std::ops::Add;

#[derive(Debug)]
struct Money {
    cents: i64,
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money { cents: self.cents + rhs.cents }
    }
}

fn main() {
    let a = Money { cents: 100 };
    let b = Money { cents: 250 };
    let total = a + b;
    println!("{:?}", total);
    println!("{:?}", a); // does not compile (error[E0382]: borrow of moved value: `a`)
}
```

The real compiler error:

```text
error[E0382]: borrow of moved value: `a`
  --> src/main.rs:20:22
   |
16 |     let a = Money { cents: 100 };
   |         - move occurs because `a` has type `Money`, which does not implement the `Copy` trait
...
18 |     let total = a + b;
   |                 ----- `a` moved due to usage in operator
19 |     println!("{:?}", total);
20 |     println!("{:?}", a); // does not compile (error[E0382]: borrow of moved value: `a`)
   |                      ^ value borrowed here after move
   |
note: calling this operator moves the left-hand side
```

**Fixes:** derive `Clone`/`Copy` if the type is small and cheap, or implement the operator on **references** so it borrows instead of moves:

```rust
use std::ops::Add;

#[derive(Debug)]
struct Matrix {
    data: Vec<f64>,
}

// `&Matrix + &Matrix` borrows both operands; they survive the call
impl Add for &Matrix {
    type Output = Matrix;
    fn add(self, rhs: &Matrix) -> Matrix {
        let data = self.data.iter().zip(&rhs.data).map(|(a, b)| a + b).collect();
        Matrix { data }
    }
}

fn main() {
    let a = Matrix { data: vec![1.0, 2.0] };
    let b = Matrix { data: vec![3.0, 4.0] };
    let c = &a + &b; // borrow, don't move
    println!("{:?}", c.data);
    println!("a still usable: {:?}", a.data);
    println!("b still usable: {:?}", b.data);
}
```

Output:

```text
[4.0, 6.0]
a still usable: [1.0, 2.0]
b still usable: [3.0, 4.0]
```

The standard library does both: it implements `Add` for `i32` (Copy, by value) *and* for `&i32`. Mirroring that for your own owned types is idiomatic. (Ownership and the move-vs-borrow distinction are the subject of [Section 05: Ownership](/05-ownership/).)

### Pitfall 2: Assuming `a * b` implies `b * a`

If you only write `impl Mul<f64> for Vector2`, then `2.0 * v` does **not** compile, because that would need `impl Mul<Vector2> for f64`:

```rust
use std::ops::Mul;

#[derive(Debug, Clone, Copy)]
struct Vector2 { x: f64, y: f64 }

impl Mul<f64> for Vector2 {
    type Output = Vector2;
    fn mul(self, s: f64) -> Vector2 {
        Vector2 { x: self.x * s, y: self.y * s }
    }
}

fn main() {
    let v = Vector2 { x: 1.0, y: 2.0 };
    let r = 2.0 * v; // does not compile (error[E0277]: cannot multiply `{float}` by `Vector2`)
    println!("{:?}", r);
}
```

Real error:

```text
error[E0277]: cannot multiply `{float}` by `Vector2`
  --> src/main.rs:16:17
   |
16 |     let r = 2.0 * v; // scalar on the left: no impl
   |                 ^ no implementation for `{float} * Vector2`
   |
   = help: the trait `Mul<Vector2>` is not implemented for `{float}`
```

**Fix:** add the second impl (`impl Mul<Vector2> for f64`), as shown earlier.

### Pitfall 3: Trying to implement a foreign operator trait for a foreign type (orphan rule)

You cannot `impl Add for String` (or any other standard type): both `Add` and `String` belong to other crates.

```rust
use std::ops::Add;

impl Add<i32> for String { // does not compile (error[E0117])
    type Output = String;
    fn add(self, n: i32) -> String {
        format!("{self}{n}")
    }
}

fn main() {}
```

Real error:

```text
error[E0117]: only traits defined in the current crate can be implemented for types defined outside of the crate
 --> src/main.rs:3:1
  |
3 | impl Add<i32> for String {
  | ^^^^^--------^^^^^------
  |      |            |
  |      |            `String` is not defined in the current crate
  |      `i32` is not defined in the current crate
  |
  = note: define and implement a trait or new type instead
```

**Fix:** wrap the foreign type in a **newtype** (`struct MyString(String);`) and implement the operator on *that*. The coherence rules behind this — and the newtype pattern — are covered in depth in [The Orphan Rule and Coherence](/09-generics-traits/12-orphan-rule/).

### Pitfall 4: Reaching for operator overloading when a method is clearer

Operators should preserve their intuitive meaning. Overloading `*` to mean "repeat a task N times" or `+` to mean "merge configs" makes code unreadable; a maintainer expects arithmetic-like semantics. Use a named method (`config.merge(other)`) unless your type genuinely models something `+` already describes (vectors, money, durations, matrices, complex numbers).

---

## Best Practices

- **Only overload operators with their conventional meaning.** `+` should be addition-like, `*` multiplication-like. If the operator would surprise a reader, write a named method instead.
- **Set the `Output` type deliberately.** It is often `Self`, but not always — a dot product `Vector · Vector` returns a scalar, so its `Output` is `f64`, not `Vector`.
- **Implement both by-value and by-reference variants for owned types.** Provide `impl Add for &T` (and combinations) so callers are not forced to clone, matching how `i32`/`&i32` behave in std.
- **Keep `AddAssign` and `Add` consistent.** If you offer `+`, also offer `+=` when it makes sense; users expect `a += b` to match `a = a + b`.
- **Derive comparison traits; don't hand-roll them.** `#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]` is correct and exhaustive far more often than a manual `impl`.
- **Add `#[derive(Clone, Copy)]` to small value types** (vectors, points, money-as-i64) so `a + b` is cheap and ownership never gets in the way.
- **Reach for crates instead of reinventing math.** For arbitrary-precision decimals use `rust_decimal`; for linear algebra use `nalgebra` or `glam`. They implement all the operator traits idiomatically.

---

## Real-World Example

A currency-safe `Money` type, stored as integer cents to avoid floating-point rounding errors, that supports `+`, `-`, `* quantity`, and `.sum()` over an iterator. This is the kind of value object you would put at the heart of a billing or checkout service.

```rust
use std::iter::Sum;
use std::ops::{Add, Mul, Sub};

/// Money stored as integer cents to avoid floating-point rounding errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Money {
    cents: i64,
}

impl Money {
    fn dollars(d: i64) -> Self {
        Money { cents: d * 100 }
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money { cents: self.cents + rhs.cents }
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money { cents: self.cents - rhs.cents }
    }
}

// Scale a unit price by an integer quantity: price * qty
impl Mul<i64> for Money {
    type Output = Money;
    fn mul(self, qty: i64) -> Money {
        Money { cents: self.cents * qty }
    }
}

// Lets `iter.sum()` produce a Money total
impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money { cents: 0 }, |acc, m| acc + m)
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sign = if self.cents < 0 { "-" } else { "" };
        let abs = self.cents.abs();
        write!(f, "{sign}${}.{:02}", abs / 100, abs % 100)
    }
}

struct LineItem {
    name: &'static str,
    price: Money,
    qty: i64,
}

impl LineItem {
    fn subtotal(&self) -> Money {
        self.price * self.qty // uses our Mul<i64> impl
    }
}

fn main() {
    let cart = [
        LineItem { name: "Keyboard", price: Money::dollars(80), qty: 1 },
        LineItem { name: "Cable", price: Money { cents: 1299 }, qty: 3 },
    ];

    for item in &cart {
        println!(
            "{:<10} {} x{} = {}",
            item.name,
            item.price,
            item.qty,
            item.subtotal()
        );
    }

    let total: Money = cart.iter().map(|i| i.subtotal()).sum(); // uses Sum
    let discount = Money::dollars(10);
    println!("Subtotal: {total}");
    println!("After {discount} discount: {}", total - discount); // uses Sub
}
```

Output from `cargo run`:

```text
Keyboard   $80.00 x1 = $80.00
Cable      $12.99 x3 = $38.97
Subtotal: $118.97
After $10.00 discount: $108.97
```

Notice how `price * qty`, `cart.iter()...sum()`, and `total - discount` all read like ordinary arithmetic, yet `Money` can only ever be combined with other `Money` (or scaled by an `i64`) — you cannot accidentally add a `Money` to a bare `i64`, which is precisely the class of bug that floating-point dollars and untyped `number` invite.

> **Tip:** In a real production service you would also consider overflow. The std arithmetic used inside these impls panics on overflow in debug builds and wraps in release builds; for money, prefer the checked variants (`self.cents.checked_add(rhs.cents)`) and return a `Result`. See [Section 08: Error Handling](/08-error-handling/).

---

## Further Reading

### Official Documentation

- [`std::ops` module](https://doc.rust-lang.org/std/ops/index.html) — every arithmetic, bitwise, indexing, and assignment operator trait
- [`std::ops::Add`](https://doc.rust-lang.org/std/ops/trait.Add.html) and [`std::ops::Index`](https://doc.rust-lang.org/std/ops/trait.Index.html) — the canonical examples, with the `Rhs`/`Output` story
- [`std::cmp::PartialEq`](https://doc.rust-lang.org/std/cmp/trait.PartialEq.html) and [`std::cmp::PartialOrd`](https://doc.rust-lang.org/std/cmp/trait.PartialOrd.html) — the traits behind `==` and `<`
- [The Rust Book — Operator Overloading](https://doc.rust-lang.org/book/ch10-02-traits.html#default-implementations) and [Advanced Traits: associated types & default generic params](https://doc.rust-lang.org/book/ch20-02-advanced-traits.html#default-generic-type-parameters-and-operator-overloading)
- [Rust by Example — Operator Overloading](https://doc.rust-lang.org/rust-by-example/trait/ops.html)

### Related Sections in This Guide

- [Traits](/09-generics-traits/03-traits/): how to define and implement a trait; the `impl Trait for Type` syntax that operator traits build on
- [Trait Methods](/09-generics-traits/04-trait-methods/): required vs. provided methods (operator traits like `Add` are pure required-method traits)
- [Trait Bounds](/09-generics-traits/05-trait-bounds/): the `T: Add<Output = T>` bound used in the generic `Point<T>` example
- [Generic Structs](/09-generics-traits/01-generic-structs/): building generic value types like `Point<T>` that you then add operators to
- [The Orphan Rule and Coherence](/09-generics-traits/12-orphan-rule/): why `impl Add for String` is rejected, and the newtype workaround
- [Marker Traits](/09-generics-traits/11-marker-traits/): `Copy`, which decides whether `a + b` moves or copies its operands
- [Section 02: Operators](/02-basics/02-operators/): the built-in operator behavior these traits extend
- [Section 05: Ownership](/05-ownership/): why `add(self, ...)` consumes its operands, and when to implement on references
- [Section 07: Collections](/07-collections/): `Vec`/`HashMap` indexing, the most common real-world `Index` users
- [Section 10: Smart Pointers](/10-smart-pointers/): `Deref`/`DerefMut`, the operator-like traits behind `*` on `Box`, `Rc`, and friends

---

## Exercises

### Exercise 1: A complex number type

**Difficulty:** Beginner

**Objective:** Implement `Add` and `Mul` for a complex number so `a + b` and `a * b` work.

**Instructions:** Define `struct Complex { re: f64, im: f64 }`. Implement `Add` (component-wise) and `Mul` (using `(a+bi)(c+di) = (ac - bd) + (ad + bc)i`). Add a `Display` impl so `1+2i` prints nicely, then print `a + b` and `a * b` for `a = 1 + 2i` and `b = 3 - 1i`.

<details>
<summary>Solution</summary>

```rust
use std::ops::{Add, Mul};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Complex {
    re: f64,
    im: f64,
}

impl Add for Complex {
    type Output = Complex;
    fn add(self, rhs: Complex) -> Complex {
        Complex { re: self.re + rhs.re, im: self.im + rhs.im }
    }
}

impl Mul for Complex {
    type Output = Complex;
    fn mul(self, rhs: Complex) -> Complex {
        Complex {
            re: self.re * rhs.re - self.im * rhs.im,
            im: self.re * rhs.im + self.im * rhs.re,
        }
    }
}

impl std::fmt::Display for Complex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.im >= 0.0 {
            write!(f, "{}+{}i", self.re, self.im)
        } else {
            write!(f, "{}{}i", self.re, self.im)
        }
    }
}

fn main() {
    let a = Complex { re: 1.0, im: 2.0 };
    let b = Complex { re: 3.0, im: -1.0 };
    println!("a + b = {}", a + b);
    println!("a * b = {}", a * b);
}
```

**Output:**

```text
a + b = 4+1i
a * b = 5+5i
```

</details>

### Exercise 2: An accumulator with `AddAssign`

**Difficulty:** Intermediate

**Objective:** Use a non-`Self` right-hand side so `stats += sample` folds an `f64` into a running aggregate.

**Instructions:** Define `struct Stats { count: u32, total: f64 }` deriving `Default`. Implement `AddAssign<f64>` so that `stats += x` increments `count` and adds `x` to `total`. Add a `mean(&self) -> f64` method (return `0.0` when empty). Feed `[10.0, 20.0, 30.0]` and print the stats and the mean.

<details>
<summary>Solution</summary>

```rust
use std::ops::AddAssign;

#[derive(Debug, Default, Clone, Copy)]
struct Stats {
    count: u32,
    total: f64,
}

impl AddAssign<f64> for Stats {
    fn add_assign(&mut self, sample: f64) {
        self.count += 1;
        self.total += sample;
    }
}

impl Stats {
    fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total / self.count as f64
        }
    }
}

fn main() {
    let mut s = Stats::default();
    for x in [10.0, 20.0, 30.0] {
        s += x;
    }
    println!("{s:?}");
    println!("mean = {}", s.mean());
}
```

**Output:**

```text
Stats { count: 3, total: 60.0 }
mean = 20
```

</details>

### Exercise 3: A 2D matrix indexed by `(row, col)`

**Difficulty:** Advanced

**Objective:** Implement `Index` and `IndexMut` with a tuple index so a flat-backed matrix supports `m[(r, c)]` for both reading and writing.

**Instructions:** Define `struct Matrix { rows: usize, cols: usize, data: Vec<f64> }` backed by a single `Vec<f64>` in row-major order. Add a `zeros(rows, cols)` constructor. Implement `Index<(usize, usize)>` and `IndexMut<(usize, usize)>` that map `(r, c)` to `r * cols + c`, with a bounds `assert!`. Build a 2×3 matrix, write a couple of cells, and read them back.

<details>
<summary>Solution</summary>

```rust
use std::ops::{Index, IndexMut};

struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

impl Matrix {
    fn zeros(rows: usize, cols: usize) -> Self {
        Matrix { rows, cols, data: vec![0.0; rows * cols] }
    }
}

impl Index<(usize, usize)> for Matrix {
    type Output = f64;
    fn index(&self, (r, c): (usize, usize)) -> &f64 {
        assert!(r < self.rows && c < self.cols, "index out of bounds");
        &self.data[r * self.cols + c]
    }
}

impl IndexMut<(usize, usize)> for Matrix {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut f64 {
        assert!(r < self.rows && c < self.cols, "index out of bounds");
        &mut self.data[r * self.cols + c]
    }
}

fn main() {
    let mut m = Matrix::zeros(2, 3);
    m[(0, 0)] = 1.0;
    m[(1, 2)] = 9.0;
    println!("m[(0,0)] = {}", m[(0, 0)]);
    println!("m[(1,2)] = {}", m[(1, 2)]);
    println!("m[(0,1)] = {}", m[(0, 1)]);
}
```

**Output:**

```text
m[(0,0)] = 1
m[(1,2)] = 9
m[(0,1)] = 0
```

> The destructuring `(r, c): (usize, usize)` in the parameter list pattern-matches the tuple index directly. Because `index`/`index_mut` return references, the *same* `m[(r, c)]` syntax serves reads and the left side of an assignment.

</details>
