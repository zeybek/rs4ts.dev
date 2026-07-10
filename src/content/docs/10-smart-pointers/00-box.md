---
title: "Box&lt;T&gt;: Heap Allocation"
description: "Box<T> is Rust's simplest smart pointer: a single owner putting one value on the heap, used for recursive types and trait objects, freed without a garbage collector."
---

`Box<T>` is the simplest **smart pointer** in Rust: a single owner of a value that lives on the **heap** instead of the stack. It is the tool you reach for when a type's size cannot be known at compile time (recursive types), or when you want to store a value of an unknown concrete type behind a trait.

---

## Quick Overview

In JavaScript and TypeScript, *every* object, array, closure, and class instance already lives on the heap behind a reference. The engine decides, and you never write the allocation. Rust flips that default: values live on the **stack** unless you explicitly ask for the heap. `Box<T>` is that explicit ask. It is a thin, owning pointer to a heap allocation that frees itself automatically when it goes out of scope — no garbage collector, no `free()`.

You will use `Box<T>` mainly for three things: **recursive data structures** (linked lists, trees, ASTs), **trait objects** (`Box<dyn Trait>`), and occasionally to move a large value to the heap so it is cheap to pass around.

---

## TypeScript/JavaScript Example

In TypeScript, heap allocation is invisible. Every node of a linked list, every tree, every polymorphic shape is "just an object," and the runtime quietly boxes everything onto the heap and reference-counts it for the garbage collector.

```typescript
// TypeScript — a recursive linked list. Heap allocation is implicit.
type List =
  | { kind: "cons"; value: number; rest: List }
  | { kind: "nil" };

const list: List = {
  kind: "cons",
  value: 1,
  rest: {
    kind: "cons",
    value: 2,
    rest: { kind: "cons", value: 3, rest: { kind: "nil" } },
  },
};

function sum(list: List): number {
  return list.kind === "cons" ? list.value + sum(list.rest) : 0;
}

console.log(sum(list)); // 6

// Polymorphism is also implicit — an array of "any Shape".
interface Shape {
  area(): number;
  name(): string;
}

class Circle implements Shape {
  constructor(private radius: number) {}
  area() {
    return Math.PI * this.radius ** 2;
  }
  name() {
    return "circle";
  }
}

const shapes: Shape[] = [new Circle(2)];
console.log(shapes[0].area()); // 12.566...
```

**Key points:**

- `rest: List` self-references with no special syntax; the engine stores a *pointer* to another heap object.
- `Shape[]` holds heterogeneous objects; the runtime dispatches `.area()` dynamically.
- You never decide stack vs. heap; the JavaScript engine always heap-allocates objects.

---

## Rust Equivalent

Rust requires you to opt into the heap. A self-referential `enum` would have *infinite size* on the stack, so each recursive child goes behind a `Box`.

```rust
// Rust — a recursive cons list. `Box` provides the heap indirection.
#[derive(Debug)]
enum List {
    Cons(i32, Box<List>),
    Nil,
}

use List::{Cons, Nil};

fn sum(list: &List) -> i32 {
    match list {
        Cons(value, rest) => value + sum(rest),
        Nil => 0,
    }
}

fn main() {
    let list = Cons(1, Box::new(Cons(2, Box::new(Cons(3, Box::new(Nil))))));
    println!("{list:?}");
    println!("sum = {}", sum(&list));
}
```

**Output:**

```text
Cons(1, Cons(2, Cons(3, Nil)))
sum = 6
```

The `Box::new(...)` call allocates on the heap and returns an owning pointer. When `list` drops at the end of `main`, the whole chain is freed recursively, deterministically, with no garbage collector.

---

## Detailed Explanation

### What a `Box<T>` actually is

A `Box<T>` is a pointer-sized value (8 bytes on a 64-bit machine) stored on the stack, pointing at a `T` stored on the heap. That is the entire data structure. There is no reference count and no extra metadata (for `Sized` types). It is the lowest-overhead way to put something on the heap.

```rust playground
fn main() {
    let boxed: Box<i32> = Box::new(42);
    println!("boxed value = {}", *boxed); // explicit dereference
    println!("boxed value = {}", boxed);  // Box<T> forwards Display to its inner T
}
```

**Output:**

```text
boxed value = 42
boxed value = 42
```

The `*boxed` syntax **dereferences** the box to reach the `i32` it owns. Because `Box<T>` implements the `Deref` trait, you rarely need the explicit `*`: method calls, field access, and most trait usage automatically "see through" the box (this is **deref coercion**, covered in the Deref trait topic of this section).

```rust playground
fn main() {
    let boxed = Box::new(vec![1, 2, 3]);
    println!("len = {}", boxed.len());        // auto-deref: (*boxed).len()
    println!("first = {:?}", boxed.first());  // auto-deref
    let total: i32 = boxed.iter().sum();
    println!("sum = {total}");
}
```

**Output:**

```text
len = 3
first = Some(1)
sum = 6
```

### Why recursive types need a `Box`

When the Rust compiler lays out a type, it needs to know its **exact size in bytes** at compile time. Consider this naive definition:

```rust
enum List {
    Cons(i32, List), // does not compile (error[E0072]: recursive type has infinite size)
    Nil,
}
```

To size `List`, the compiler must size `Cons`, which contains a `List`, which contains a `Cons`, which contains a `List`… forever. The size equation has no solution. The real compiler error spells this out:

```text
error[E0072]: recursive type `List` has infinite size
 --> src/main.rs:1:1
  |
1 | enum List {
  | ^^^^^^^^^
2 |     Cons(i32, List), // no indirection
  |               ---- recursive without indirection
  |
help: insert some indirection (e.g., a `Box`, `Rc`, or `&`) to break the cycle
  |
2 |     Cons(i32, Box<List>), // no indirection
  |               ++++    +
```

The compiler literally suggests the fix: wrap the recursive field in a `Box`. A `Box<List>` is always one pointer wide regardless of how deep the list goes, so `List` now has a finite, known size. We can confirm it:

```rust playground
use std::mem;

enum List {
    Cons(i32, Box<List>),
    Nil,
}

fn main() {
    println!("size of Box<List> = {}", mem::size_of::<Box<List>>());
    println!("size of List = {}", mem::size_of::<List>());
    println!("size of i32 = {}", mem::size_of::<i32>());
}
```

**Output:**

```text
size of Box<List> = 8
size of List = 16
size of i32 = 4
```

`List` is 16 bytes: 4 for the `i32`, 8 for the `Box` pointer, plus padding/the discriminant, and importantly *fixed*, no matter how long the actual list is. The heap holds the rest.

> **Note:** Only the recursion needs a box. `Vec<T>`, `String`, `HashMap`, and similar collections *already* store their contents on the heap internally, so a `Vec<Tree>` is fine without an explicit `Box` around each element. You add a `Box` when a single field directly contains another instance of the same type.

### Trait objects in a `Box`

The second classic use is storing a value whose concrete type you do not know at compile time, but which implements a known trait. `Box<dyn Trait>` is the owning version of a trait object.

```rust playground
trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> &str;
}

struct Circle { radius: f64 }
struct Rectangle { width: f64, height: f64 }

impl Shape for Circle {
    fn area(&self) -> f64 { std::f64::consts::PI * self.radius * self.radius }
    fn name(&self) -> &str { "circle" }
}

impl Shape for Rectangle {
    fn area(&self) -> f64 { self.width * self.height }
    fn name(&self) -> &str { "rectangle" }
}

// One function, two different concrete return types — only possible behind `dyn`.
fn make_shape(kind: &str) -> Box<dyn Shape> {
    match kind {
        "circle" => Box::new(Circle { radius: 2.0 }),
        _ => Box::new(Rectangle { width: 3.0, height: 4.0 }),
    }
}

fn main() {
    let shapes: Vec<Box<dyn Shape>> = vec![
        make_shape("circle"),
        make_shape("rectangle"),
    ];

    for shape in &shapes {
        println!("{} has area {:.2}", shape.name(), shape.area());
    }
}
```

**Output:**

```text
circle has area 12.57
rectangle has area 12.00
```

A `Circle` and a `Rectangle` have different sizes, so `Vec<Box<dyn Shape>>` cannot store them inline — but a `Box<dyn Shape>` is a fixed-size **fat pointer** (data pointer + vtable pointer), so a `Vec` of them works. This is the closest Rust equivalent to TypeScript's `Shape[]`. The `dyn` keyword and the full mechanics of dynamic dispatch are covered in [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/); here the point is simply that `Box` is what *owns* the trait object.

> **Tip:** A function can only return *one* concrete type with `impl Trait`. When you need to return *different* concrete types from different branches — as `make_shape` does — you must use `Box<dyn Trait>`.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust with `Box<T>` |
| --- | --- | --- |
| Where objects live | Always heap (engine-managed) | Stack by default; heap only via `Box`/collections |
| Who frees memory | Garbage collector, eventually | Deterministic `drop` at end of scope |
| Pointer cost | Hidden reference + GC bookkeeping | One machine word, zero runtime bookkeeping |
| Recursive types | `rest: List` "just works" | Must break the cycle with `Box`/`Rc`/`&` |
| Polymorphic container | `Shape[]` (implicit boxing) | `Vec<Box<dyn Shape>>` (explicit) |
| Ownership | Shared by default (multiple refs) | Single owner; move semantics |
| Null | `null` / `undefined` allowed | No null; absence is `Option<Box<T>>` |

The mental shift for a TypeScript developer: in JavaScript *everything is a `Box` you never see*. In Rust, the stack is the default and the heap is a deliberate choice. That choice is cheap (`Box` is the minimal smart pointer), but it is yours to make.

### `Box<T>` vs. the other smart pointers

`Box<T>` gives **single ownership** and **no extra capabilities**: it is purely "this value, but on the heap." When you need *shared* ownership, reach for [`Rc`/`Arc`](/10-smart-pointers/01-rc-arc/). When you need to mutate through a shared pointer, reach for [`RefCell`/`Mutex`](/10-smart-pointers/02-refcell-mutex/) or [`Cell`](/10-smart-pointers/03-cell/). A decision guide on which smart pointer to pick lives in this section's overview.

---

## Common Pitfalls

### Pitfall 1: Forgetting indirection in a recursive type

The error message is friendly, but new Rustaceans still get tripped up: defining `Cons(i32, List)` fails with `error[E0072]: recursive type has infinite size`. The fix is the compiler's own suggestion: wrap the recursive field in `Box<List>` (or `Rc`/`&`). See the verified message in the Detailed Explanation above.

### Pitfall 2: Trying to move a value out of a borrowed `Box`

Coming from JavaScript, where copying a reference is free, you might try to "take" a boxed value through a shared reference:

```rust
fn main() {
    let boxed = Box::new(String::from("hello"));
    let reference = &boxed;
    let moved = *reference; // does not compile (error[E0507])
    println!("{moved}");
}
```

The real compiler error:

```text
error[E0507]: cannot move out of `*reference` which is behind a shared reference
 --> src/main.rs:4:17
  |
4 |     let moved = *reference; // cannot move out of `*reference`
  |                 ^^^^^^^^^^ move occurs because `*reference` has type `Box<String>`, which does not implement the `Copy` trait
  |
help: consider removing the dereference here
  |
4 -     let moved = *reference; // cannot move out of `*reference`
4 +     let moved = reference; // cannot move out of `*reference`
  |
help: consider cloning the value if the performance cost is acceptable
  |
4 -     let moved = *reference; // cannot move out of `*reference`
4 +     let moved = reference.clone(); // cannot move out of `*reference`
  |
```

You cannot move ownership out through a *shared* `&`. Either borrow the inner value (`let moved: &String = reference;`), clone it (`reference.clone()`), or — if you own the box outright — dereference the owned box directly (next pitfall).

### Pitfall 3: Surprise — you *can* move out of an owned `Box`

`Box<T>` is special: unlike most types, dereferencing an *owned* box (a "deref move") moves the value out and frees the box.

```rust playground
fn main() {
    let boxed = Box::new(String::from("hello"));
    let owned: String = *boxed; // moves the String off the heap into `owned`
    println!("{owned}");        // prints: hello
    // `boxed` is now consumed; using it here would be a compile error.
}
```

This works because the compiler knows `boxed` is the unique owner. It does *not* work through a reference (Pitfall 2). If you only have a reference, clone instead.

### Pitfall 4: Reaching for `Box` when you don't need the heap

A common over-correction is boxing small values "to be safe." A plain `i32`, a small struct, or a fixed-size array belongs on the stack. Boxing it just adds a pointer indirection and a heap allocation for no benefit. Use `Box` for recursion, trait objects, or genuinely large values you want to move cheaply — not as a default.

---

## Best Practices

- **Box the recursion, not the leaves.** Put `Box` only on the field that creates the cycle. Let `Vec`, `String`, and other collections do their own heap management.
- **Prefer `Option<Box<T>>` for optional children.** A tree node's `left`/`right` are naturally `Option<Box<TreeNode>>`: `None` is the empty subtree, with no null pointer in sight.
- **Use `Box<dyn Trait>` for heterogeneous collections and "factory" return types.** When branches return different concrete types, `Box<dyn Trait>` is the idiomatic answer; `impl Trait` cannot do it.
- **Reach for `Box<dyn Error>` in application code.** `Box<dyn std::error::Error>` is the standard "any error" return type for `main` and glue code (see [Error Handling](/08-error-handling/)).
- **Don't box what fits on the stack.** Boxing a `Copy` scalar or a small struct usually pessimizes performance.
- **Let `Drop` do the work.** Never write manual cleanup; a `Box` frees its heap allocation automatically and deterministically when it leaves scope.

---

## Real-World Example

A production-flavored use of `Box` is an **abstract syntax tree (AST)**: for example, evaluating an arithmetic expression. Each operator node holds sub-expressions of arbitrary depth, so the children live behind `Box`. This is exactly the shape a calculator, query parser, or template engine uses internally.

```rust playground
// An arithmetic expression AST. Each operator node owns boxed sub-expressions,
// so an expression of any depth has a fixed-size root node.
#[derive(Debug)]
enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

impl Expr {
    fn eval(&self) -> Result<f64, String> {
        match self {
            Expr::Number(n) => Ok(*n),
            Expr::Add(a, b) => Ok(a.eval()? + b.eval()?),
            Expr::Sub(a, b) => Ok(a.eval()? - b.eval()?),
            Expr::Mul(a, b) => Ok(a.eval()? * b.eval()?),
            Expr::Div(a, b) => {
                let divisor = b.eval()?;
                if divisor == 0.0 {
                    Err("division by zero".to_string())
                } else {
                    Ok(a.eval()? / divisor)
                }
            }
        }
    }
}

// A small helper keeps tree construction readable.
fn num(n: f64) -> Box<Expr> {
    Box::new(Expr::Number(n))
}

fn main() {
    // (2 + 3) * (10 - 4)  =>  30
    let expr = Expr::Mul(
        Box::new(Expr::Add(num(2.0), num(3.0))),
        Box::new(Expr::Sub(num(10.0), num(4.0))),
    );
    match expr.eval() {
        Ok(result) => println!("result = {result}"),
        Err(e) => println!("error: {e}"),
    }

    // 1 / 0  =>  error, propagated via `?`
    let bad = Expr::Div(num(1.0), num(0.0));
    match bad.eval() {
        Ok(result) => println!("result = {result}"),
        Err(e) => println!("error: {e}"),
    }
}
```

**Output:**

```text
result = 30
error: division by zero
```

The recursion in `eval` mirrors the recursion in the type, and the `?` operator threads `Result` errors up through the tree. Because each child is owned by its parent `Box`, dropping the root drops the entire tree exactly once.

---

## Further Reading

### Official documentation

- [The Rust Book — Using `Box<T>` to Point to Data on the Heap](https://doc.rust-lang.org/book/ch15-01-box.html)
- [The Rust Book — Smart Pointers (chapter 15)](https://doc.rust-lang.org/book/ch15-00-smart-pointers.html)
- [`std::boxed::Box` API docs](https://doc.rust-lang.org/std/boxed/struct.Box.html)
- [Rust by Example — Box, stack and heap](https://doc.rust-lang.org/rust-by-example/std/box.html)
- [The Rustonomicon — "Too Many Linked Lists" (deep dive on Box-based lists)](https://rust-unofficial.github.io/too-many-lists/)

### Related topics in this guide

- [Section 10 overview](/10-smart-pointers/): the full map of smart pointers
- [Shared Ownership with `Rc<T>` and `Arc<T>`](/10-smart-pointers/01-rc-arc/) — when you need *shared* ownership instead of a single owner
- [Interior Mutability](/10-smart-pointers/02-refcell-mutex/): mutating through a shared pointer (interior mutability)
- The `Deref` trait topic of this section — the trait that makes `*boxed` and auto-deref work (forthcoming)
- This section's overview; decision guide: which smart pointer to pick when
- [Trait Objects and Dynamic Dispatch](/09-generics-traits/06-trait-objects/) — the full story on `dyn Trait` and dynamic dispatch
- [Ownership](/05-ownership/): ownership and move semantics, the foundation `Box` builds on
- [Async](/11-async/) — `Box::pin` and `Pin<Box<dyn Future>>` show up when working with async

---

## Exercises

### Exercise 1: A stack as a singly linked list

**Difficulty:** Beginner

**Objective:** Build a recursive data structure with `Box` and understand why the indirection is required.

**Instructions:** Implement a `Stack` of `i32` backed by a singly linked list. Use an `enum Link { Empty, More(Box<Node>) }` where `Node` holds a `value` and a `next: Link`. Provide `new`, `push`, and `pop`. (Hint: `std::mem::replace(&mut self.head, Link::Empty)` lets you take ownership of the current head without leaving a hole.)

```rust
enum Link {
    Empty,
    More(Box<Node>),
}

struct Node {
    value: i32,
    next: Link,
}

struct Stack {
    head: Link,
}

impl Stack {
    fn new() -> Self {
        // TODO
    }
    fn push(&mut self, value: i32) {
        // TODO
    }
    fn pop(&mut self) -> Option<i32> {
        // TODO
    }
}

fn main() {
    let mut stack = Stack::new();
    stack.push(1);
    stack.push(2);
    stack.push(3);
    while let Some(v) = stack.pop() {
        print!("{v} "); // expected: 3 2 1
    }
    println!();
}
```

<details>
<summary>Solution</summary>

```rust playground
enum Link {
    Empty,
    More(Box<Node>),
}

struct Node {
    value: i32,
    next: Link,
}

struct Stack {
    head: Link,
}

impl Stack {
    fn new() -> Self {
        Stack { head: Link::Empty }
    }

    fn push(&mut self, value: i32) {
        // Take the old head out, then make a new node point at it.
        let new_node = Box::new(Node {
            value,
            next: std::mem::replace(&mut self.head, Link::Empty),
        });
        self.head = Link::More(new_node);
    }

    fn pop(&mut self) -> Option<i32> {
        match std::mem::replace(&mut self.head, Link::Empty) {
            Link::Empty => None,
            Link::More(node) => {
                self.head = node.next; // move the tail back into head
                Some(node.value)
            }
        }
    }
}

fn main() {
    let mut stack = Stack::new();
    stack.push(1);
    stack.push(2);
    stack.push(3);
    while let Some(v) = stack.pop() {
        print!("{v} ");
    }
    println!();
}
```

**Output:**

```text
3 2 1
```

`std::mem::replace` is the key trick: it swaps in `Link::Empty` and hands you the previous value to own. Without it, you would be trying to move `self.head` out of a `&mut self`, which the borrow checker rejects.

</details>

### Exercise 2: A JSON-like value tree

**Difficulty:** Intermediate

**Objective:** Model a recursive document format and write a function that walks it.

**Instructions:** Define an `enum Json` with variants `Null`, `Bool(bool)`, `Number(f64)`, `Str(String)`, `Array(Vec<Json>)`, and `Tagged(String, Box<Json>)` (a label wrapping one nested value — this is the variant that *requires* an explicit `Box`). Write `fn depth(value: &Json) -> usize` returning the maximum nesting depth (a scalar is depth 0; each `Array`/`Tagged` layer adds 1).

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<Json>),
    Tagged(String, Box<Json>),
}

fn depth(value: &Json) -> usize {
    match value {
        Json::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        Json::Tagged(_, inner) => 1 + depth(inner),
        _ => 0, // Null, Bool, Number, Str are leaves
    }
}

fn main() {
    let doc = Json::Array(vec![
        Json::Number(1.0),
        Json::Tagged(
            "nested".to_string(),
            Box::new(Json::Array(vec![Json::Bool(true), Json::Null])),
        ),
        Json::Str("hi".to_string()),
    ]);
    println!("depth = {}", depth(&doc));
    println!("{doc:?}");
}
```

**Output:**

```text
depth = 3
Array([Number(1.0), Tagged("nested", Array([Bool(true), Null])), Str("hi")])
```

Note that `Array(Vec<Json>)` does *not* need a `Box`; `Vec` already heap-allocates its elements. Only `Tagged`, which holds a single nested `Json` directly, needs the `Box` to break the size cycle.

</details>

### Exercise 3: A command queue of boxed trait objects

**Difficulty:** Intermediate / Advanced

**Objective:** Use `Box<dyn Trait>` to store and run a heterogeneous list of behaviors — the Rust analog of an array of polymorphic objects in TypeScript.

**Instructions:** Define a trait `Command` with `fn run(&self, state: &mut i32)` and `fn describe(&self) -> String`. Implement it for three unit/tuple structs: `Add(i32)`, `Mul(i32)`, and `Reset`. Write `fn run_all(commands: &[Box<dyn Command>]) -> i32` that starts from `0`, applies each command in order, prints `"<description> -> <state>"` after each, and returns the final state.

<details>
<summary>Solution</summary>

```rust playground
trait Command {
    fn run(&self, state: &mut i32);
    fn describe(&self) -> String;
}

struct Add(i32);
struct Mul(i32);
struct Reset;

impl Command for Add {
    fn run(&self, state: &mut i32) { *state += self.0; }
    fn describe(&self) -> String { format!("add {}", self.0) }
}

impl Command for Mul {
    fn run(&self, state: &mut i32) { *state *= self.0; }
    fn describe(&self) -> String { format!("mul {}", self.0) }
}

impl Command for Reset {
    fn run(&self, state: &mut i32) { *state = 0; }
    fn describe(&self) -> String { "reset".to_string() }
}

fn run_all(commands: &[Box<dyn Command>]) -> i32 {
    let mut state = 0;
    for command in commands {
        command.run(&mut state);
        println!("{:<8} -> {state}", command.describe());
    }
    state
}

fn main() {
    let program: Vec<Box<dyn Command>> = vec![
        Box::new(Add(5)),
        Box::new(Mul(3)),
        Box::new(Add(1)),
        Box::new(Reset),
        Box::new(Add(42)),
    ];
    let final_state = run_all(&program);
    println!("final = {final_state}");
}
```

**Output:**

```text
add 5    -> 5
mul 3    -> 15
add 1    -> 16
reset    -> 0
add 42   -> 42
final = 42
```

The `Vec<Box<dyn Command>>` stores three different concrete types behind one fixed-size pointer type, exactly like a TypeScript `Command[]`. The difference: in Rust the boxing is explicit, ownership is single and clear, and each command is freed deterministically when the `Vec` drops.

</details>
