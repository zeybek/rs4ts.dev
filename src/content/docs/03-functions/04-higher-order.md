---
title: "Higher-Order Functions"
description: "Rust's map, filter, and fold mirror TypeScript array methods but run lazily on iterators. Pass closures with impl Fn and return them via impl Fn or Box<dyn Fn>."
---

Higher-order functions are functions that take other functions as parameters or return functions as results. If you write TypeScript, you already use them constantly: `array.map`, `array.filter`, `array.reduce`, event handlers, and middleware are all higher-order functions. Rust has the same toolkit, but the type system makes the relationships explicit.

---

## Quick Overview

A **higher-order function** either accepts a function (or closure) as an argument, returns one, or both. In TypeScript these are everywhere and untyped at runtime; in Rust the compiler needs to know *how* a function is passed — by generic bound (`impl Fn`), or behind a pointer (`Box<dyn Fn>`) — because closures have no single fixed size or type. This matters to a TypeScript/JavaScript developer because the iterator methods you reach for daily (`map`, `filter`, `reduce`) exist in Rust too, just lazier and with a few extra rules.

---

## TypeScript/JavaScript Example

```typescript
// TypeScript - higher-order functions are the bread and butter of array work
const numbers = [1, 2, 3, 4, 5];

// map: transform every element
const doubled = numbers.map((n) => n * 2);
console.log(doubled); // [2, 4, 6, 8, 10]

// filter: keep elements that pass a predicate
const evens = numbers.filter((n) => n % 2 === 0);
console.log(evens); // [2, 4]

// reduce: fold everything into a single value
const sum = numbers.reduce((acc, n) => acc + n, 0);
console.log(sum); // 15

// Chaining is idiomatic
const sumOfOddSquares = numbers
  .filter((n) => n % 2 === 1)
  .map((n) => n * n)
  .reduce((acc, n) => acc + n, 0);
console.log(sumOfOddSquares); // 35

// A function that RETURNS a function (a closure factory)
function makeAdder(n: number): (x: number) => number {
  return (x) => x + n;
}
const add5 = makeAdder(5);
console.log(add5(10)); // 15

// A function that TAKES a function
function applyTwice(f: (x: number) => number, x: number): number {
  return f(f(x));
}
console.log(applyTwice((x) => x + 3, 10)); // 16
```

**Key points:**

- `map`/`filter`/`reduce` are methods on `Array`, and they run **eagerly** — each one builds a new array immediately.
- A function type is just `(args) => ret`; you can store it, pass it, and return it without ceremony.
- At runtime there is no difference between a closure that captures variables and one that does not — they are all just JavaScript functions (objects).

---

## Rust Equivalent

```rust playground
fn main() {
    let numbers = vec![1, 2, 3, 4, 5];

    // map: transform every element (note: .iter() then .collect())
    let doubled: Vec<i32> = numbers.iter().map(|n| n * 2).collect();
    println!("{:?}", doubled); // [2, 4, 6, 8, 10]

    // filter: keep elements that pass a predicate
    let evens: Vec<i32> = numbers.iter().filter(|&&n| n % 2 == 0).copied().collect();
    println!("{:?}", evens); // [2, 4]

    // reduce -> fold (with an explicit initial accumulator)
    let sum: i32 = numbers.iter().fold(0, |acc, &n| acc + n);
    println!("{}", sum); // 15

    // Chaining is idiomatic here too
    let sum_of_odd_squares: i32 = numbers
        .iter()
        .filter(|&&n| n % 2 == 1)
        .map(|&n| n * n)
        .sum();
    println!("{}", sum_of_odd_squares); // 35
}

// A function that RETURNS a closure (impl Fn)
fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n
}

// A function that TAKES a closure (generic over F: Fn)
fn apply_twice<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(f(x))
}
```

Calling those last two:

```rust
fn main() {
    let add5 = make_adder(5);
    println!("{}", add5(10)); // 15

    println!("{}", apply_twice(|x| x + 3, 10)); // 16
}
```

**Key points:**

- The iterator adapters (`map`, `filter`) are **lazy**: they build a description of work, and nothing happens until a *consumer* like `collect`, `sum`, or `fold` drives them.
- `reduce` in JavaScript with an initial value maps to Rust's `fold`. (Rust also has a `reduce` method, but it has no seed and returns an `Option`.)
- Passing a closure uses a **generic bound** (`F: Fn(i32) -> i32`); returning one uses `impl Fn(...) -> ...` or, when the concrete type can vary, `Box<dyn Fn(...) -> ...>`.

> **Note:** `Fn`, `FnMut`, and `FnOnce` are the three closure traits. They are covered in depth in [Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/); here we focus on *using* them in higher-order signatures.

---

## Detailed Explanation

### Why `.iter()` and `.collect()`?

In JavaScript, `numbers.map(...)` is a method on the array that returns a brand-new array. In Rust, `map` is a method on an **iterator**, not on the `Vec` itself. So you first turn the collection into an iterator, then adapt it, then collect the results back into a concrete collection:

```rust playground
fn main() {
    let numbers = vec![1, 2, 3, 4, 5];
    let doubled: Vec<i32> = numbers.iter().map(|n| n * 2).collect();
    //                      ^^^^^^^         ^^^                ^^^^^^^
    //                      make iterator   transform          materialize
    println!("{:?}", doubled);
}
```

The `: Vec<i32>` annotation on the left is usually required because `collect` is generic over *what* it collects into (it could build a `Vec`, a `HashSet`, a `String`, etc.). The annotation tells the compiler which one you want. This is one of the few places Rust's inference needs a hint that TypeScript would not.

> **Tip:** `iter()` borrows each element (`&i32`), `iter_mut()` borrows mutably (`&mut i32`), and `into_iter()` *consumes* the collection and yields owned values (`i32`). Choosing the right one is an ownership decision, covered in [Section 05: Ownership](/05-ownership/).

### Laziness: the big difference from JavaScript

JavaScript's array methods are eager. Each `.map(...)` allocates a new array on the spot. Rust's iterator adapters are **lazy**: they do nothing until consumed. This is closer to a generator pipeline than to `Array.prototype.map`.

```rust playground
fn main() {
    let numbers = vec![1, 2, 3, 4, 5];

    // This line allocates NOTHING and runs NO closures yet:
    let pipeline = numbers.iter().filter(|&&n| n % 2 == 1).map(|&n| n * n);

    // The work happens here, when a consumer drives the iterator:
    let total: i32 = pipeline.sum();
    println!("{}", total); // 35
}
```

The payoff is that a long chain of adapters fuses into a single pass with no intermediate `Vec` allocations: `filter().map().sum()` walks the data once. In JavaScript the same chain would allocate one array per step.

### The `|&&n|` double-reference pattern

You will see odd-looking patterns like `|&&n|` in `filter`. Here is the why: `numbers.iter()` yields `&i32`. `filter`'s closure receives a *reference to the item*, so it gets `&&i32`. The pattern `|&&n|` destructures both references, binding `n` to a plain `i32` you can compare:

```rust playground
fn main() {
    let numbers = vec![1, 2, 3, 4, 5];
    let evens: Vec<i32> = numbers
        .iter()                     // yields &i32
        .filter(|&&n| n % 2 == 0)   // closure gets &&i32; &&n -> n: i32
        .copied()                   // &i32 -> i32
        .collect();
    println!("{:?}", evens); // [2, 4]
}
```

`map`, by contrast, receives the item by value-of-the-iterator (`&i32` here), so `|&n| n * n` destructures one layer. This referencing dance has no parallel in TypeScript, where everything is a reference under the hood and you never write it out.

### Passing a closure: `impl Fn` vs generic bound

There are three equivalent ways to write "this function takes a closure." All three monomorphize: the compiler stamps out a specialized copy of the function for each closure type you pass, with zero indirection at runtime (like how TypeScript generics work in your head, except TypeScript erases them and Rust keeps them).

```rust
// 1. Generic with an inline trait bound
fn apply_twice<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(f(x))
}

// 2. The same thing, using `impl Trait` in argument position (sugar for #1)
fn apply_twice_impl(f: impl Fn(i32) -> i32, x: i32) -> i32 {
    f(f(x))
}

// 3. The same thing again, moving the bound into a `where` clause
fn apply_twice_where<F>(f: F, x: i32) -> i32
where
    F: Fn(i32) -> i32,
{
    f(f(x))
}
```

Use `Fn` when the closure only reads captured state, `FnMut` when it mutates captured state, and `FnOnce` when it consumes captured state. Asking for the *least* demanding trait you need makes your function accept the *most* callers.

```rust playground
// FnMut: the closure mutates state it captured
fn call_n_times<F: FnMut()>(mut f: F, n: usize) {
    for _ in 0..n {
        f();
    }
}

// FnOnce: the closure may consume what it captured (called at most once)
fn consume<F: FnOnce() -> String>(f: F) -> String {
    f()
}

fn main() {
    let mut count = 0;
    call_n_times(|| count += 1, 5);
    println!("count = {}", count); // count = 5

    let owned = String::from("hello");
    let joined = consume(move || owned + " world"); // moves `owned` in
    println!("{}", joined); // hello world
}
```

> **Note:** Notice `mut f` in `call_n_times`. Calling an `FnMut` closure requires a mutable binding, so the parameter itself is declared `mut`. This is a small ownership detail TypeScript never surfaces.

### Returning a closure: `impl Fn` vs `Box<dyn Fn>`

Returning a function is where Rust diverges most sharply from TypeScript, because every closure has a unique, compiler-generated, anonymous type, and you cannot name it. You have two options.

**Option 1 — `impl Fn`** when there is exactly one closure shape returned. This is zero-cost (no heap allocation, no indirection):

```rust
fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n
}
```

The `move` keyword forces the closure to *take ownership* of `n` so the closure can outlive the `make_adder` call. Without it, the closure would try to borrow a local that is about to be destroyed (see Pitfall 3).

**Option 2 — `Box<dyn Fn>`** when the concrete closure type can differ between branches. Because `impl Fn` means "one specific hidden type," you cannot return two different closures from two `if`/`match` arms with it. Boxing them behind a trait object (`dyn Fn`) erases the difference, at the cost of a heap allocation and dynamic dispatch:

```rust playground
fn make_op(kind: &str) -> Box<dyn Fn(i32) -> i32> {
    match kind {
        "double" => Box::new(|x| x * 2),
        "negate" => Box::new(|x| -x),
        _ => Box::new(|x| x),
    }
}

fn main() {
    let double = make_op("double");
    let negate = make_op("negate");
    println!("{} {}", double(21), negate(7)); // 42 -7
}
```

This `dyn` (dynamic dispatch through a pointer) is the closest analogy to how a TypeScript function value works at runtime: a pointer you call indirectly.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust |
| --- | --- | --- |
| `map` / `filter` | Methods on `Array`, **eager** | Methods on `Iterator`, **lazy** until consumed |
| `reduce(fn, init)` | `Array.prototype.reduce` | `Iterator::fold(init, fn)` |
| `reduce(fn)` (no init) | Uses the first element as the seed; throws `TypeError` on an empty array | `Iterator::reduce(fn)` returns `Option<T>` |
| Building the result | Returns a new array automatically | `.collect()` / `.sum()` / `.count()` etc. |
| Taking a function | `(x) => T` param, untyped at runtime | `impl Fn` / `F: Fn` (monomorphized, zero-cost) |
| Returning a function | `(): (x) => T` | `impl Fn` (one type) or `Box<dyn Fn>` (varying types) |
| Capturing variables | Always by reference to the closure scope | By ref / by mut ref / by move; `Fn`/`FnMut`/`FnOnce` |
| Cost | Heap-allocated function objects, GC | `impl Fn`: zero-cost; `Box<dyn Fn>`: one allocation + dynamic dispatch |

The mental model shift: in TypeScript a function is *a value of one type*. In Rust, every closure has its *own* type, so "a function parameter" must be expressed as a generic bound or a trait object. There is no single `Function` type that covers them all.

> **Note:** Unlike TypeScript, where `array.map` always produces a fresh array, a Rust adapter chain produces nothing until consumed and then runs in a single fused pass. If you write a `map` and never collect or otherwise consume it, the closure never runs at all (see Pitfall 1).

---

## Common Pitfalls

### Pitfall 1: Treating iterators as eager (forgetting to consume)

A TypeScript developer expects `map` to *do something*. In Rust, an unconsumed adapter does nothing, and the compiler warns you:

```rust playground
fn main() {
    let names = vec!["alice", "bob"];
    names.iter().map(|n| println!("Hello, {n}!"));
    println!("done");
}
```

The "Hello" lines never print. The real compiler warning:

```
warning: unused `Map` that must be used
 --> src/main.rs:3:5
  |
3 |     names.iter().map(|n| println!("Hello, {n}!"));
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: iterators are lazy and do nothing unless consumed
  = note: `#[warn(unused_must_use)]` on by default
help: use `let _ = ...` to ignore the resulting value
  |
3 |     let _ = names.iter().map(|n| println!("Hello, {n}!"));
  |     +++++++

warning: `Iterator::map` call that discard the iterator's values
 --> src/main.rs:3:18
  |
3 |     names.iter().map(|n| println!("Hello, {n}!"));
  |                  ^^^^---------------------------^
  ...
help: you might have meant to use `Iterator::for_each`
  |
3 -     names.iter().map(|n| println!("Hello, {n}!"));
3 +     names.iter().for_each(|n| println!("Hello, {n}!"));
  |
```

**Fix:** if you only want side effects, use `for_each` (or a plain `for` loop); if you want results, `collect` them.

```rust playground
fn main() {
    let names = vec!["alice", "bob"];
    names.iter().for_each(|n| println!("Hello, {n}!"));
}
```

### Pitfall 2: Returning two different closures from one `impl Fn`

This looks fine to a TypeScript eye but does not compile, because the two `move` closures capture `n` and are therefore *distinct anonymous types*:

```rust
fn make_op(double: bool, n: i32) -> impl Fn(i32) -> i32 {
    if double {
        move |x| x * n
    } else {
        move |x| x + n
    }
}
```

The real error:

```
error[E0308]: `if` and `else` have incompatible types
 --> src/main.rs:5:9
  |
2 | /     if double {
3 | |         move |x| x * n
  | |         --------------
  | |         |
  | |         the expected closure
  | |         expected because of this
4 | |     } else {
5 | |         move |x| x + n
  | |         ^^^^^^^^^^^^^^ expected closure, found a different closure
6 | |     }
  | |_____- `if` and `else` have incompatible types
  |
  = note: expected closure `{closure@src/main.rs:3:9: 3:17}`
             found closure `{closure@src/main.rs:5:9: 5:17}`
  = note: no two closures, even if identical, have the same type
  = help: consider boxing your closure and/or using it as a trait object
help: you could change the return type to be a boxed trait object
  |
1 - fn make_op(double: bool, n: i32) -> impl Fn(i32) -> i32 {
1 + fn make_op(double: bool, n: i32) -> Box<dyn Fn(i32) -> i32> {
  |
```

**Fix:** box them, exactly as the compiler suggests:

```rust
fn make_op(double: bool, n: i32) -> Box<dyn Fn(i32) -> i32> {
    if double {
        Box::new(move |x| x * n)
    } else {
        Box::new(move |x| x + n)
    }
}
```

### Pitfall 3: Returning a closure that borrows a local (missing `move`)

```rust
fn make_greeter(name: String) -> impl Fn() -> String {
    || format!("Hello, {name}!")
}
```

The closure borrows `name`, but `name` is dropped when `make_greeter` returns. The real error:

```
error[E0373]: closure may outlive the current function, but it borrows `name`, which is owned by the current function
 --> src/main.rs:2:5
  |
2 |     || format!("Hello, {name}!")
  |     ^^                  ---- `name` is borrowed here
  |     |
  |     may outlive borrowed value `name`
  |
note: closure is returned here
 --> src/main.rs:2:5
  |
2 |     || format!("Hello, {name}!")
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: to force the closure to take ownership of `name` (and any other referenced variables), use the `move` keyword
  |
2 |     move || format!("Hello, {name}!")
  |     ++++
```

**Fix:** add `move` so the closure owns `name`:

```rust
fn make_greeter(name: String) -> impl Fn() -> String {
    move || format!("Hello, {name}!")
}
```

### Pitfall 4: Reaching for `reduce` when you mean `fold`

JavaScript's `reduce(fn, init)` is Rust's `fold(init, fn)`. Rust's `reduce` is the *seedless* variant and returns `Option<T>` (because an empty iterator has no first element to start from):

```rust playground
fn main() {
    let numbers = vec![1, 2, 3, 4, 5];

    // fold: has a seed, returns the accumulator type directly
    let sum: i32 = numbers.iter().fold(0, |acc, &n| acc + n);
    println!("{}", sum); // 15

    // reduce: no seed, returns Option<T>
    let max = numbers.iter().copied().reduce(|a, b| if a > b { a } else { b });
    println!("{:?}", max); // Some(5)
}
```

Mixing up the argument order (`fold(fn, init)`) or forgetting that `reduce` yields an `Option` are the two most common slips.

---

## Best Practices

### Prefer `impl Fn` in arguments and returns; box only when forced

`impl Fn` (and the generic `F: Fn` form) is zero-cost. Reach for `Box<dyn Fn>` only when you genuinely need to return or store closures of *different* concrete types (e.g., from different `match` arms, or in a `Vec` of callbacks).

```rust playground
// Good: zero-cost, single closure shape
fn scaler(factor: f64) -> impl Fn(f64) -> f64 {
    move |x| x * factor
}

// Necessary boxing: heterogeneous callbacks in one collection
fn pipeline() -> Vec<Box<dyn Fn(i32) -> i32>> {
    vec![
        Box::new(|x| x + 1),
        Box::new(|x| x * 3),
        Box::new(|x| x - 2),
    ]
}

fn main() {
    let half = scaler(0.5);
    println!("{}", half(10.0)); // 5

    let result = pipeline().iter().fold(5, |acc, step| step(acc));
    println!("{}", result); // ((5+1)*3)-2 = 16
}
```

### Ask for the weakest closure trait that works

Bound on `Fn` if you only read, `FnMut` if you mutate, `FnOnce` if you consume. A function bounded on `FnOnce` accepts the widest set of closures; a function bounded on `Fn` is the most restrictive but composes most freely (you can call it many times). Pick based on what your function actually does with the closure.

### Use the purpose-built consumers instead of `fold` where they exist

`sum()`, `product()`, `count()`, `max()`, `min()`, `any()`, `all()`, and `find()` express intent more clearly than a hand-rolled `fold` and are just as fast.

```rust playground
fn main() {
    let prices = vec![19.99, 5.49, 120.0];
    let total: f64 = prices.iter().sum();             // clearer than fold
    let expensive = prices.iter().any(|&p| p > 100.0); // clearer than fold
    println!("total={total:.2} expensive={expensive}");
}
```

### Store closures behind a trait object for plugin-style designs

A struct field typed `Box<dyn Fn(...)>` lets you hold a callback whose body you do not know at compile time — the Rust equivalent of stashing a TypeScript function on an object.

```rust playground
struct Button {
    on_click: Box<dyn Fn()>,
}

fn main() {
    let btn = Button {
        on_click: Box::new(|| println!("clicked!")),
    };
    (btn.on_click)(); // clicked!
}
```

---

## Real-World Example

A small order-processing report, the kind of thing you would write in a backend service. It demonstrates a higher-order function that *returns* a reusable predicate (`min_total`), plus a `filter`/`map`/`fold` pipeline that processes the data in a single pass.

```rust playground
#[derive(Debug)]
struct Order {
    id: u32,
    customer: String,
    total: f64,
    paid: bool,
}

/// Returns a reusable predicate closure. The `impl Fn(&Order) -> bool`
/// return type means callers get a zero-cost, directly-callable filter.
fn min_total(threshold: f64) -> impl Fn(&Order) -> bool {
    move |order| order.total >= threshold
}

fn main() {
    let orders = vec![
        Order { id: 1, customer: "Alice".into(), total: 120.0, paid: true },
        Order { id: 2, customer: "Bob".into(),   total: 35.5,  paid: false },
        Order { id: 3, customer: "Carol".into(), total: 250.0, paid: true },
        Order { id: 4, customer: "Dan".into(),   total: 80.0,  paid: true },
    ];

    let is_big = min_total(100.0);

    // Keep paid + big orders, format "#id Name", collect into a Vec<String>.
    let vip: Vec<String> = orders
        .iter()
        .filter(|o| o.paid)
        .filter(|o| is_big(o))
        .map(|o| format!("#{} {}", o.id, o.customer))
        .collect();
    println!("VIP customers: {:?}", vip);

    // Total revenue from paid orders via fold.
    let revenue: f64 = orders
        .iter()
        .filter(|o| o.paid)
        .fold(0.0, |acc, o| acc + o.total);
    println!("Paid revenue: {:.2}", revenue);

    // Count unpaid orders.
    let unpaid = orders.iter().filter(|o| !o.paid).count();
    println!("Unpaid orders: {}", unpaid);
}
```

**Output:**

```
VIP customers: ["#1 Alice", "#3 Carol"]
Paid revenue: 450.00
Unpaid orders: 1
```

The equivalent TypeScript would chain `.filter(...).filter(...).map(...)` (allocating an array per step) and a `.reduce(...)` for revenue. The Rust version reads almost identically but runs each pipeline in one fused, allocation-free pass, and `min_total` hands back a typed, reusable predicate.

---

## Further Reading

### Official Documentation

- [The Rust Book — Closures and the `Fn` traits](https://doc.rust-lang.org/book/ch13-01-closures.html)
- [The Rust Book — Processing a Series of Items with Iterators](https://doc.rust-lang.org/book/ch13-02-iterators.html)
- [`std::iter::Iterator` — all adapters and consumers](https://doc.rust-lang.org/std/iter/trait.Iterator.html)
- [Rust by Example — Higher Order Functions](https://doc.rust-lang.org/rust-by-example/fn/hof.html)

### Related Sections in This Guide

- [Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/): the `|args|` syntax, `Fn`/`FnMut`/`FnOnce`, and capture-by-ref vs `move`
- [Function Pointers](/03-functions/05-function-pointers/): the `fn` type, passing named functions, and how function items differ from closures
- [Basic Functions](/03-functions/00-basic-functions/): `fn` signatures and the expression-vs-statement model the closures here rely on
- [Return Values](/03-functions/02-return-values/): tail expressions and returning multiple values
- [Section 05: Ownership](/05-ownership/): why `iter` vs `into_iter` and `move` matter
- [Section 04: Control Flow](/04-control-flow/): loops as an alternative to iterator chains
- [Section 02: Variables and Mutability](/02-basics/00-variables/): the `mut` rule behind `FnMut`

---

## Exercises

### Exercise 1: Translate a `map`/`filter`/`reduce` chain

**Difficulty:** Easy

**Objective:** Build muscle memory for the lazy iterator pipeline and `fold`.

**Instructions:** Given the TypeScript below, write the equivalent Rust. It should take a slice of word lengths, keep the ones longer than 3, double each, and sum them.

```typescript
const lengths = [2, 5, 3, 8, 1, 4];
const result = lengths
  .filter((n) => n > 3)
  .map((n) => n * 2)
  .reduce((acc, n) => acc + n, 0);
console.log(result); // (5+8+4)*2 = 34
```

<details>
<summary>Solution</summary>

```rust playground
fn main() {
    let lengths = [2, 5, 3, 8, 1, 4];
    let result: i32 = lengths
        .iter()
        .filter(|&&n| n > 3)
        .map(|&n| n * 2)
        .sum(); // sum() is the idiomatic consumer for "+ with 0 seed"
    println!("{}", result); // 34
}
```

You could also write the last step as `.fold(0, |acc, n| acc + n)`, which mirrors the TypeScript `reduce` exactly; `sum()` is the idiomatic shorthand.

</details>

### Exercise 2: Write a closure factory

**Difficulty:** Medium

**Objective:** Practice returning a closure with `impl Fn` and capturing with `move`.

**Instructions:** Write a function `make_multiplier(factor: i32)` that returns a closure multiplying its argument by `factor`. Then write `compose(f, g)` that returns a closure applying `f` first, then `g`. Use them so that multiplying by 3 then adding 1 turns `10` into `31`.

<details>
<summary>Solution</summary>

```rust playground
fn make_multiplier(factor: i32) -> impl Fn(i32) -> i32 {
    move |x| x * factor
}

// Generic over the input/intermediate/output types.
fn compose<A, B, C>(f: impl Fn(A) -> B, g: impl Fn(B) -> C) -> impl Fn(A) -> C {
    move |x| g(f(x))
}

fn main() {
    let times3 = make_multiplier(3);
    let add1 = |x: i32| x + 1;

    let times3_then_add1 = compose(times3, add1);
    println!("{}", times3_then_add1(10)); // (10*3)+1 = 31
}
```

`move` is required in both factories so the returned closures own the values they captured (`factor`, and the inner `f`/`g`).

</details>

### Exercise 3: A boxed-callback pipeline

**Difficulty:** Hard

**Objective:** Use `Box<dyn Fn>` to store and run a heterogeneous list of transformations, and combine it with `filter_map`.

**Instructions:** Write `total_valid(inputs: &[&str]) -> i32` that parses each string to an `i32`, discards the ones that fail to parse (use `filter_map` with `.parse().ok()`), keeps only positive numbers, and sums the rest. Then build a `Vec<Box<dyn Fn(i32) -> i32>>` of three steps (`+1`, `*3`, `-2`) and fold the seed `5` through them. Verify the parse step yields `35` for `["10", "-3", "abc", "5", "0", "20"]` and the pipeline yields `16`.

<details>
<summary>Solution</summary>

```rust playground
fn total_valid(inputs: &[&str]) -> i32 {
    inputs
        .iter()
        .filter_map(|s| s.parse::<i32>().ok()) // keep only the Ok values
        .filter(|&n| n > 0)
        .sum()
}

fn main() {
    let inputs = ["10", "-3", "abc", "5", "0", "20"];
    println!("{}", total_valid(&inputs)); // 10 + 5 + 20 = 35

    // A heterogeneous pipeline of boxed closures.
    let steps: Vec<Box<dyn Fn(i32) -> i32>> = vec![
        Box::new(|x| x + 1),
        Box::new(|x| x * 3),
        Box::new(|x| x - 2),
    ];
    let result = steps.iter().fold(5, |acc, step| step(acc));
    println!("{}", result); // ((5+1)*3)-2 = 16
}
```

`filter_map` fuses a `map` and a `filter`: the closure returns an `Option`, and `None` values are dropped. `.parse::<i32>()` returns a `Result`, and `.ok()` converts it to an `Option`, throwing away the error. Boxing the steps is required because each closure has a distinct anonymous type, yet they must live together in one `Vec`.

</details>
