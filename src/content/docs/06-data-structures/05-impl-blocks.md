---
title: "Methods and `impl` Blocks"
description: "Rust splits data and behavior: methods live in impl blocks, and &self, &mut self, or self spells out the borrowing TypeScript hides behind an implicit this."
---

In TypeScript you write methods *inside* the `class` body. In Rust, data and behavior are kept separate: you define a `struct` (the data) and then attach methods to it in a separate **`impl` block**. This file is about that split, and about the single most important thing the `impl` block forces you to be explicit about: how each method borrows or consumes the value it is called on.

---

## Quick Overview

An **`impl` block** (short for *implementation*) is where you write the methods for a type. Every method takes an explicit first parameter — `&self`, `&mut self`, or `self` — that says exactly how much access it needs to the value: read-only, mutable, or full ownership. That single annotation is the Rust equivalent of a TypeScript method's invisible `this`, except Rust makes the borrowing rules part of the signature so the compiler can enforce them.

---

## TypeScript/JavaScript Example

Here is a small `Rectangle` class, the kind of thing you would write every day in TypeScript. Notice that the data (`width`, `height`) and the behavior (`area`, `scale`, ...) all live inside one `class` body, and that every method silently has access to `this`.

```typescript
// TypeScript - data and methods bundled together in a class
class Rectangle {
  width: number;
  height: number;

  constructor(width: number, height: number) {
    this.width = width;
    this.height = height;
  }

  // Reads `this`, never mutates it
  area(): number {
    return this.width * this.height;
  }

  isSquare(): boolean {
    return this.width === this.height;
  }

  // Mutates `this` in place
  scale(factor: number): void {
    this.width *= factor;
    this.height *= factor;
  }

  // "Consumes" the rectangle conceptually, returns a new square
  intoSquare(): Rectangle {
    const side = Math.max(this.width, this.height);
    return new Rectangle(side, side);
  }
}

const rect = new Rectangle(3, 4);
console.log(rect.area());      // 12
console.log(rect.isSquare());  // false
rect.scale(2);
console.log(rect);             // Rectangle { width: 6, height: 8 }
```

There is no way, from the signatures alone, to tell that `area()` only *reads* while `scale()` *mutates*. TypeScript trusts you to keep that straight. Nothing stops a method from quietly mutating `this`, and nothing stops two parts of your program from holding the same object and stepping on each other.

---

## Rust Equivalent

In Rust the data lives in a `struct` and the behavior lives in an `impl` block. Each method spells out its relationship to the value via its first parameter.

```rust
#[derive(Debug, Clone)]
struct Rectangle {
    width: f64,
    height: f64,
}

impl Rectangle {
    // &self: read-only borrow (like a TS method that doesn't mutate)
    fn area(&self) -> f64 {
        self.width * self.height
    }

    fn is_square(&self) -> bool {
        self.width == self.height
    }

    // &mut self: mutable borrow, can change fields
    fn scale(&mut self, factor: f64) {
        self.width *= factor;
        self.height *= factor;
    }

    // self: takes ownership, consumes the value
    fn into_square(self) -> Rectangle {
        let side = self.width.max(self.height);
        Rectangle { width: side, height: side }
    }
}

fn main() {
    let mut rect = Rectangle { width: 3.0, height: 4.0 };
    println!("area = {}", rect.area());
    println!("is_square = {}", rect.is_square());

    rect.scale(2.0);
    println!("after scale: {:?}", rect);

    let sq = rect.into_square();
    println!("square: {:?}", sq);
    // `rect` was moved into `into_square`; using it here would not compile.
}
```

Running it prints:

```text
area = 12
is_square = false
after scale: Rectangle { width: 6.0, height: 8.0 }
square: Rectangle { width: 8.0, height: 8.0 }
```

> **Note:** `area = 12`, not `12.0`. Rust's `Display` formatting (`{}`) prints a whole-valued `f64` without a trailing `.0`. The `Debug` formatting (`{:?}`) used in the `after scale:` line *does* show `6.0`, so you can tell it is a float.

Three different methods, three different first parameters. The signature is the contract: `area` promises not to mutate, `scale` requires mutable access, and `into_square` takes the whole rectangle and never gives it back.

> This file assumes you already know how to declare and instantiate a struct. If `Rectangle { width: 3.0, height: 4.0 }` is unfamiliar, read [Structs](/06-data-structures/00-structs/) first. Constructors like `Rectangle::new(...)` (associated functions with no `self`) are covered in [Associated Functions and Constructors](/06-data-structures/06-associated-functions/).

---

## Detailed Explanation

### Why the data and methods are separate

In TypeScript the `class` keyword fuses fields and methods into one declaration. Rust deliberately splits them:

- `struct Rectangle { ... }` declares *what the data looks like*.
- `impl Rectangle { ... }` declares *what you can do with it*.

This split is more than stylistic. It means methods are decoupled from the type definition, which is what later lets you implement **traits** (Rust's version of interfaces) for your type in their own `impl` blocks. See [Associated Types and Associated Constants](/06-data-structures/07-associated-types/) for a first taste and Section 09 for the full story. A `struct` with no `impl` block is perfectly valid; it is just a bag of data.

### `self` is the receiver, and it is explicit

Every method's first parameter is some form of `self`. This is Rust's `this`, but unlike TypeScript's invisible, always-present `this`, you write it out and you choose its *flavor*:

| First parameter | Means | TypeScript analogy |
| --------------- | ----- | ------------------ |
| `&self` | Borrow the value immutably (read-only) | A method that only reads `this` |
| `&mut self` | Borrow the value mutably (can change fields) | A method that mutates `this` |
| `self` | Take ownership of the value (consume it) | *No clean analogy* — TS objects are never "used up" |

`&self` is shorthand for `self: &Self`, `&mut self` for `self: &mut Self`, and `self` for `self: Self`, where `Self` is an alias for the type the `impl` is for (`Rectangle` here). You will almost always use the shorthand.

### Method calls desugar to function calls

When you write `rect.area()`, Rust desugars it to a plain function call and automatically inserts the right kind of reference. These two lines are identical:

```rust
#[derive(Debug)]
struct User {
    name: String,
}

impl User {
    fn greet(&self) -> String {
        format!("Hi, {}!", self.name)
    }
}

fn main() {
    let user = User { name: String::from("Ada") };

    // These two calls are equivalent: method syntax desugars to the
    // fully-qualified function call, with Rust auto-referencing `&user`.
    println!("{}", user.greet());
    println!("{}", User::greet(&user));
}
```

Both print `Hi, Ada!`. The dot syntax is just sugar: Rust looks at `greet`'s signature, sees `&self`, and automatically passes `&user` instead of `user`. This automatic referencing/dereferencing is why you never have to write `(&rect).area()` or `(*ptr).area()` by hand: the compiler figures out how many `&` or `*` to add. (Contrast TypeScript, where `this` is bound dynamically at call time and can be lost when a method is detached; in Rust the receiver is resolved statically at compile time.)

### `&self`: the default, read-only method

`area`, `is_square`, `subtotal_cents` — anything that only *reads* — should take `&self`. An immutable borrow lets many parts of your program look at the value at once, and it guarantees the method cannot accidentally change anything. This is the most common receiver; reach for it first.

### `&mut self`: in-place mutation

`scale` changes the rectangle's fields, so it needs `&mut self`: a mutable (exclusive) borrow. While a `&mut self` method is running, nothing else can touch the value, which is how Rust statically prevents data races. Calling a `&mut self` method requires the variable itself to be declared `mut` (more on that in Pitfalls).

### `self`: consuming the value

`into_square` takes `self` *by value*. The rectangle is **moved** into the method, and the original binding is no longer usable afterward. This is the receiver with no TypeScript equivalent: in JavaScript an object lives until the garbage collector decides otherwise, and a method can never "use it up." In Rust, a `self`-taking method expresses a transformation that consumes the input. This is common when converting one type into another (the `into_*` naming convention) or when finalizing a builder. See [Associated Functions and Constructors](/06-data-structures/06-associated-functions/) for the builder pattern teaser.

---

## Multiple `impl` Blocks

A single type can have **as many `impl` blocks as you like**. They all contribute methods to the same type; there is no requirement to put everything in one block.

```rust
#[derive(Debug)]
struct Counter {
    count: u32,
    step: u32,
}

// First impl block: construction and core behavior
impl Counter {
    fn new(step: u32) -> Self {
        Counter { count: 0, step }
    }

    fn increment(&mut self) {
        self.count += self.step;
    }
}

// Second impl block: read-only queries (could live in another module/file)
impl Counter {
    fn value(&self) -> u32 {
        self.count
    }

    fn has_reached(&self, target: u32) -> bool {
        self.count >= target
    }
}

fn main() {
    let mut c = Counter::new(5);
    c.increment();
    c.increment();
    println!("value = {}", c.value());
    println!("reached 10? {}", c.has_reached(10));
    println!("{c:?}");
}
```

Output:

```text
value = 10
reached 10? true
Counter { count: 10, step: 5 }
```

For a plain struct like this, splitting into two blocks is purely organizational; the compiler treats it as one combined set of methods. So why is it allowed?

- **Organization.** Group constructors in one block, queries in another, mutators in a third. Larger types stay readable.
- **Conditional methods.** With generics you can add methods only when the type parameter satisfies a bound (e.g. only when `T: Display`). Each bound gets its own `impl` block. (Covered in Section 09.)
- **Trait implementations.** Inherent methods go in `impl Counter`, while `impl SomeTrait for Counter` lives in its own separate block. They never mix.
- **Spreading across files.** A type defined in one module can have additional `impl` blocks in other modules of the same crate.

> **Tip:** For a simple type, prefer one `impl` block. Reach for multiple blocks when there is a real reason: generic bounds, trait impls, or a genuinely large API. Splitting a five-method struct into five blocks just adds noise.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| ------- | ----------------------- | ---- |
| Where methods live | Inside the `class` body | In a separate `impl` block |
| The receiver | Implicit `this` | Explicit `self` / `&self` / `&mut self` |
| Read vs. mutate | Not visible in the signature | Encoded as `&self` vs. `&mut self` |
| Consuming the value | Not possible (GC owns lifetime) | `self` by value moves and consumes it |
| Number of "method bodies" per type | One `class` body | Any number of `impl` blocks |
| Static vs. instance methods | `static` keyword | Methods without `self` (associated functions) |
| `this` rebinding | Dynamic; can be lost/rebound | Resolved statically; cannot be lost |

The headline difference: **Rust puts the borrowing contract in the method signature.** A TypeScript reader cannot tell `area()` from `scale()` without reading the bodies. A Rust reader sees `&self` versus `&mut self` and knows immediately, and so does the compiler. That is what makes "this method won't mutate my object" a *guarantee* rather than a *convention*.

A second difference worth internalizing: `self` by value has no TypeScript counterpart. The idea that calling a method can *use up* the object — so that the variable is gone afterward — flows directly from [ownership](/05-ownership/). If you have not read Section 05 yet, the `self`-by-value examples here are a good motivation to.

---

## Common Pitfalls

### Pitfall 1: Calling a `&mut self` method on an immutable binding

You declared the variable without `mut`, then tried to call a mutating method.

```rust
struct BankAccount {
    balance: u64,
}

impl BankAccount {
    fn deposit(&mut self, amount: u64) {
        self.balance += amount;
    }
}

fn main() {
    let account = BankAccount { balance: 100 };
    account.deposit(50); // does not compile (error[E0596])
    println!("{}", account.balance);
}
```

The real compiler error:

```text
error[E0596]: cannot borrow `account` as mutable, as it is not declared as mutable
  --> src/main.rs:13:5
   |
13 |     account.deposit(50); // does not compile (error[E0596])
   |     ^^^^^^^ cannot borrow as mutable
   |
help: consider changing this to be mutable
   |
12 |     let mut account = BankAccount { balance: 100 };
   |         +++
```

**Fix:** declare the binding `let mut account = ...`. Mutability is a property of the *binding*, not the type, so the caller must opt in, just as `&mut self` opted in on the method side.

### Pitfall 2: Using a value after a `self`-taking method consumed it

A method that takes `self` by value moves the value; the original binding is dead afterward.

```rust
#[derive(Debug)]
struct Order {
    items: Vec<String>,
}

impl Order {
    fn finalize(self) -> usize {
        self.items.len()
    }
}

fn main() {
    let order = Order { items: vec![String::from("book"), String::from("pen")] };
    let count = order.finalize(); // consumes `order`
    println!("count = {}", count);
    println!("{:?}", order); // does not compile (error[E0382]): order was moved
}
```

The real compiler error (abridged). Note how it points right at `finalize`'s `self`:

```text
error[E0382]: borrow of moved value: `order`
  --> src/main.rs:16:22
   |
13 |     let order = Order { items: ... };
   |         ----- move occurs because `order` has type `Order`, which does not implement the `Copy` trait
14 |     let count = order.finalize(); // consumes `order`
   |                       ---------- `order` moved due to this method call
...
16 |     println!("{:?}", order); // order was moved
   |                      ^^^^^ value borrowed here after move
   |
note: `Order::finalize` takes ownership of the receiver `self`, which moves `order`
```

**Fix:** if you did not mean to consume the value, take `&self` instead of `self`. If consuming is intended (e.g. `into_*` conversions), just do everything you need with `order` *before* the consuming call, or have the method return what you still need.

### Pitfall 3: Forgetting `&mut self` and trying to mutate through `&self`

A `&self` method gets a shared, read-only borrow, so you cannot assign to fields through it.

```rust
struct Counter {
    count: u32,
}

impl Counter {
    fn increment(&self) {
        self.count += 1; // does not compile (error[E0594])
    }
}
```

The real compiler error helpfully suggests the fix:

```text
error[E0594]: cannot assign to `self.count`, which is behind a `&` reference
 --> src/main.rs:7:9
  |
7 |         self.count += 1; // does not compile (error[E0594])
  |         ^^^^^^^^^^^^^^^ `self` is a `&` reference, so the data it refers to cannot be written
  |
help: consider changing this to be a mutable reference
  |
6 |     fn increment(&mut self) {
  |                   +++
```

**Fix:** change the receiver to `&mut self`. Coming from TypeScript this trips people up because *every* TS method can mutate `this`; in Rust you must ask for write access.

### Pitfall 4: Reaching for `self` by value when `&self` would do

A subtle one: taking `self` by value when you only read makes every call consume the value, forcing the caller to clone or reconstruct it. Unless you are deliberately converting/finalizing, default to `&self`. Take `&mut self` to mutate, and `self` only when consumption is the point.

---

## Best Practices

- **Default to `&self`.** Most methods only read. Use the least permissive receiver that does the job: `&self` over `&mut self`, and `&mut self` over `self`.
- **Reserve `self` by value for transformations and finalizers.** Conversions named `into_*`, builder steps, and "this object is now spent" operations are the natural homes for `self`.
- **Use the `into_` / `to_` / `as_` naming conventions.** By convention, `into_*` consumes `self` (takes ownership), `to_*` borrows and produces an owned copy (often more expensive), and `as_*` is a cheap borrow-to-borrow view. Following the convention tells callers the cost without reading the body.
- **Keep mutation explicit and contained.** A small number of clearly named `&mut self` methods is easier to reason about than many methods that quietly mutate.
- **Group with `impl` blocks intentionally.** One block for a simple type. Separate blocks for trait impls, for generic-bounded methods, or to keep a large API navigable, not just for the sake of splitting.
- **Don't annotate the receiver's type by hand.** Write `&self`, not the longhand `self: &Self`. The shorthand is idiomatic; [Clippy](/01-getting-started/03-cargo-basics/)'s `needless_arbitrary_self_type` lint (warn-by-default) flags the `self: &Self` spelling and suggests `&self`. Note it only catches the `Self`-alias forms: write it out as a concrete type (`self: &Rectangle`) and Clippy stays silent, so don't rely on the linter to catch every longhand receiver.

---

## Real-World Example

A shopping cart that uses all three receiver kinds the way you would in production: `&mut self` to build it up, `&self` to query it, and `self` to finalize it into an immutable receipt. It is also split across two `impl` blocks: one for mutation, one for read-only queries.

```rust
#[derive(Debug, Clone)]
struct CartItem {
    name: String,
    price_cents: u64,
    quantity: u32,
}

#[derive(Debug, Default)]
struct ShoppingCart {
    items: Vec<CartItem>,
}

impl ShoppingCart {
    fn new() -> Self {
        ShoppingCart { items: Vec::new() }
    }

    // &mut self: mutate the cart in place
    fn add(&mut self, name: &str, price_cents: u64, quantity: u32) {
        self.items.push(CartItem {
            name: name.to_string(),
            price_cents,
            quantity,
        });
    }
}

// A second impl block grouping the read-only "query" methods.
impl ShoppingCart {
    // &self: read-only; borrows the cart without taking it
    fn subtotal_cents(&self) -> u64 {
        self.items
            .iter()
            .map(|item| item.price_cents * item.quantity as u64)
            .sum()
    }

    fn item_count(&self) -> u32 {
        self.items.iter().map(|item| item.quantity).sum()
    }

    fn describe(&self) -> String {
        let lines: Vec<String> = self
            .items
            .iter()
            .map(|item| format!("  {} x{}", item.name, item.quantity))
            .collect();
        lines.join("\n")
    }

    // self: consumes the cart, producing a final receipt
    fn checkout(self) -> String {
        format!(
            "{}\nOrder placed: {} item(s), total ${:.2}",
            self.describe(),
            self.item_count(),
            self.subtotal_cents() as f64 / 100.0
        )
    }
}

fn main() {
    let mut cart = ShoppingCart::new();
    cart.add("Rust Book", 3999, 1);
    cart.add("Sticker", 299, 3);

    println!("items in cart: {}", cart.item_count());
    println!("subtotal: ${:.2}", cart.subtotal_cents() as f64 / 100.0);

    let receipt = cart.checkout(); // cart is consumed here; can't use `cart` afterward
    println!("{receipt}");
}
```

Output:

```text
items in cart: 4
subtotal: $48.96
  Rust Book x1
  Sticker x3
Order placed: 4 item(s), total $48.96
```

The receiver kinds map directly onto the cart's lifecycle: you *build* it (`&mut self`), you *inspect* it as often as you like (`&self`), and you *check out* exactly once (`self`), after which the cart is gone and only the receipt remains. The compiler enforces that lifecycle: you cannot accidentally add an item to a cart you have already checked out.

### Bonus: fluent chaining with `mut self`

A close cousin is the owning-builder style, where each step takes `mut self` by value, mutates, and returns `self` so calls can be chained:

```rust
#[derive(Debug)]
struct RequestBuilder {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
}

impl RequestBuilder {
    fn new(url: &str) -> Self {
        RequestBuilder {
            url: url.to_string(),
            method: "GET".to_string(),
            headers: Vec::new(),
        }
    }

    // Takes `mut self` by value and returns it, enabling fluent chaining.
    fn method(mut self, method: &str) -> Self {
        self.method = method.to_string();
        self
    }

    fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }
}

fn main() {
    let req = RequestBuilder::new("https://api.example.com/users")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer token123");

    println!("{} {}", req.method, req.url);
    for (k, v) in &req.headers {
        println!("  {k}: {v}");
    }
}
```

Output:

```text
POST https://api.example.com/users
  Content-Type: application/json
  Authorization: Bearer token123
```

> **Note:** `mut self` here is *not* a fourth receiver kind: it is still `self` by value (the value is moved in), and `mut` just makes the moved-in binding mutable so the body can reassign fields. It reads almost exactly like a TypeScript fluent API that `return this`, except each step takes ownership and hands it back. The full builder pattern, including when to prefer `&mut self` chaining instead, is teased in [Associated Functions and Constructors](/06-data-structures/06-associated-functions/).

---

## Further Reading

### Official Documentation

- [The Rust Book - Method Syntax](https://doc.rust-lang.org/book/ch05-03-method-syntax.html)
- [Rust by Example - Methods](https://doc.rust-lang.org/rust-by-example/fn/methods.html)
- [Rust Reference - Implementations](https://doc.rust-lang.org/reference/items/implementations.html)
- [Rust API Guidelines - `as_`, `to_`, `into_` conventions](https://rust-lang.github.io/api-guidelines/naming.html#ad-hoc-conversions-follow-as_-to_-into_-conventions-c-conv)

### Related Sections in This Guide

- [Structs](/06-data-structures/00-structs/) — defining the data your `impl` block operates on
- [Associated Functions](/06-data-structures/06-associated-functions/) — methods *without* `self` (constructors like `Self::new`) and the builder teaser
- [Associated Types & Consts](/06-data-structures/07-associated-types/) — other things that live inside an `impl` block
- [Enums](/06-data-structures/02-enums/) and [Pattern Matching](/06-data-structures/04-pattern-matching/) — `impl` blocks work on enums too
- [Ownership](/05-ownership/) — the foundation behind `self` / `&self` / `&mut self`
- [Variables and Mutability](/02-basics/00-variables/) — why a binding must be `mut` to call a `&mut self` method
- [Collections](/07-collections/) — the `Vec<T>` used in the real-world example

---

## Exercises

### Exercise 1: Temperature methods

**Difficulty:** Easy

**Objective:** Practice choosing between `&self` and `&mut self`.

**Instructions:** Given the struct below, write a `to_fahrenheit` method that returns the temperature in Fahrenheit without changing the value, and a `warm_up` method that raises the Celsius value in place. Pick the correct receiver for each.

```rust
#[derive(Debug)]
struct Temperature {
    celsius: f64,
}

impl Temperature {
    fn to_fahrenheit(/* ??? */) -> f64 {
        // formula: C * 9/5 + 32
    }

    fn warm_up(/* ??? */, degrees: f64) {
        // TODO: raise self.celsius by `degrees`
    }
}

fn main() {
    let mut t = Temperature { celsius: 20.0 };
    println!("{}F", t.to_fahrenheit()); // 68
    t.warm_up(5.0);
    println!("now {}C = {}F", t.celsius, t.to_fahrenheit()); // 25C = 77F
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
struct Temperature {
    celsius: f64,
}

impl Temperature {
    fn to_fahrenheit(&self) -> f64 {
        self.celsius * 9.0 / 5.0 + 32.0
    }

    fn warm_up(&mut self, degrees: f64) {
        self.celsius += degrees;
    }
}

fn main() {
    let mut t = Temperature { celsius: 20.0 };
    println!("{}F", t.to_fahrenheit()); // 68
    t.warm_up(5.0);
    println!("now {}C = {}F", t.celsius, t.to_fahrenheit()); // 25C = 77F
}
```

`to_fahrenheit` only reads, so it takes `&self`. `warm_up` mutates, so it needs `&mut self`, and that is why `t` must be declared `let mut`.

</details>

### Exercise 2: A stack that can be drained

**Difficulty:** Medium

**Objective:** Use a `self`-by-value method to consume a value and hand back its contents.

**Instructions:** Complete the `Stack` so that `push` adds a value, `sum` reports the total without consuming the stack, and `into_vec` consumes the stack and returns the underlying `Vec<i32>`. After calling `into_vec`, the stack should no longer be usable.

```rust
#[derive(Debug)]
struct Stack {
    values: Vec<i32>,
}

impl Stack {
    fn new() -> Self {
        Stack { values: Vec::new() }
    }

    fn push(/* ??? */, value: i32) {
        // TODO
    }

    fn sum(/* ??? */) -> i32 {
        // TODO: total of all values
    }

    fn into_vec(/* ??? */) -> Vec<i32> {
        // TODO: return the underlying Vec, consuming self
    }
}

fn main() {
    let mut s = Stack::new();
    s.push(1);
    s.push(2);
    s.push(3);
    println!("sum = {}", s.sum()); // 6
    let v = s.into_vec();
    println!("vec = {:?}", v);     // [1, 2, 3]
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
struct Stack {
    values: Vec<i32>,
}

impl Stack {
    fn new() -> Self {
        Stack { values: Vec::new() }
    }

    fn push(&mut self, value: i32) {
        self.values.push(value);
    }

    fn sum(&self) -> i32 {
        self.values.iter().sum()
    }

    // Consumes the stack and hands back the underlying Vec.
    fn into_vec(self) -> Vec<i32> {
        self.values
    }
}

fn main() {
    let mut s = Stack::new();
    s.push(1);
    s.push(2);
    s.push(3);
    println!("sum = {}", s.sum()); // 6
    let v = s.into_vec();
    println!("vec = {:?}", v);     // [1, 2, 3]
}
```

`into_vec` takes `self` by value so it can *move* `self.values` out and return it. Because the stack is consumed, the compiler will reject any use of `s` after `s.into_vec()`. The `into_` prefix is the conventional signal that a method takes ownership.

</details>

### Exercise 3: A chainable account across two `impl` blocks

**Difficulty:** Medium/Hard

**Objective:** Combine multiple `impl` blocks with the owning-builder (`mut self`) chaining style.

**Instructions:** Split `Account` into two `impl` blocks. In the first, put the constructor `new(initial: i64)`. In the second, put `deposit` and `withdraw` (each takes `mut self`, adjusts the balance, and returns `Self` so calls can be chained) plus a read-only `balance(&self) -> i64`. Then make the `main` below print `balance = 120`.

```rust
#[derive(Debug)]
struct Account {
    balance: i64,
}

// impl block 1: construction
// impl block 2: deposit / withdraw / balance

fn main() {
    let account = Account::new(100)
        .deposit(50)
        .withdraw(30);
    println!("balance = {}", account.balance()); // 120
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug)]
struct Account {
    balance: i64,
}

impl Account {
    fn new(initial: i64) -> Self {
        Account { balance: initial }
    }
}

impl Account {
    fn deposit(mut self, amount: i64) -> Self {
        self.balance += amount;
        self
    }

    fn withdraw(mut self, amount: i64) -> Self {
        self.balance -= amount;
        self
    }

    fn balance(&self) -> i64 {
        self.balance
    }
}

fn main() {
    let account = Account::new(100)
        .deposit(50)
        .withdraw(30);
    println!("balance = {}", account.balance()); // 120
}
```

`100 + 50 - 30 = 120`. Each `deposit`/`withdraw` takes `mut self`, mutates the moved-in value, and returns it so the next call in the chain receives ownership. `balance` only reads, so it takes `&self`. Splitting construction from behavior into two `impl` blocks is purely organizational here; the compiler merges them into one method set.

</details>
