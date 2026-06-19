---
title: "Weak References with `Weak<T>`"
description: "Rc/Arc cycles leak in Rust with no garbage collector. Weak<T> is the non-owning back-reference you upgrade() to access, mirroring JavaScript's WeakRef."
---

`Rc<T>` and `Arc<T>` give you shared ownership, but two values that own each other form a **reference cycle** that never reaches a count of zero: a genuine memory leak even in safe Rust. `Weak<T>` is the non-owning reference that lets you point *back* into a graph (child to parent, observer to subject) without keeping the target alive.

---

## Quick Overview

A **`Weak<T>`** is a reference to data managed by an `Rc<T>` (or `Arc<T>`) that **does not contribute to the strong count**, so it never keeps the value alive. Because the data behind a `Weak` can disappear at any time, you cannot read it directly: you call **`upgrade()`**, which returns an `Option<Rc<T>>`: `Some` if the value is still alive, `None` if it has already been dropped. This is the closest Rust analog to JavaScript's `WeakRef`, and it exists for exactly the same reason: to reference something without preventing it from being reclaimed.

> **Note:** If you have not read it yet, start with [`01_rc-arc.md`](/10-smart-pointers/01-rc-arc/). `Weak<T>` only makes sense once you understand strong reference counting.

---

## TypeScript/JavaScript Example

In JavaScript you almost never think about reference cycles, because the garbage collector traces reachability and reclaims cycles automatically. A parent/child tree where children point back at their parents "just works":

```typescript
// TypeScript — a tree where children link back to their parents.
class TreeNode {
  value: string;
  parent: TreeNode | null = null; // strong reference back to parent
  children: TreeNode[] = [];

  constructor(value: string) {
    this.value = value;
  }

  appendChild(child: TreeNode): void {
    child.parent = this; // child -> parent
    this.children.push(child); // parent -> child  (a cycle!)
  }

  path(): string {
    return this.parent ? `${this.parent.path()}/${this.value}` : this.value;
  }
}

const html = new TreeNode("html");
const body = new TreeNode("body");
const p = new TreeNode("p");
html.appendChild(body);
body.appendChild(p);

console.log(p.path()); // "html/body/p"
console.log(`body has ${body.children.length} child(ren)`); // 1
```

The `parent` and `children` fields form a cycle (`body` references `p`, and `p` references `body`), yet the V8 garbage collector reclaims the whole tree once it is unreachable from the roots. JavaScript also has an explicit **`WeakRef`** for cases where you want to reference an object *without* keeping it alive:

```typescript
// JavaScript WeakRef — the closest analog to Rust's Weak<T>.
let strong: { value: number } | null = { value: 42 };
const weak = new WeakRef(strong);

console.log(weak.deref()); // { value: 42 }  — still reachable
strong = null; // drop the only strong reference
// Once the GC runs, weak.deref() *may* return undefined.
// (Timing is non-deterministic — the GC decides when.)
```

> **Note:** `WeakRef.prototype.deref()` returns the object **or `undefined`**, never an error. Rust's `Weak::upgrade()` returns the value wrapped in an `Rc`, **or `None`**. The shapes line up almost exactly.

---

## Rust Equivalent

Rust has **no garbage collector**, so a reference cycle built from `Rc<T>` is a real leak: the strong counts prop each other up and never hit zero. The fix is to make the "back pointer" a `Weak<T>` so it does not count toward ownership.

```rust
// Rust — a tree where children link back to their parents via Weak.
use std::cell::RefCell;
use std::rc::{Rc, Weak};

type ElementRef = Rc<Element>;

struct Element {
    tag: String,
    // Strong: a parent OWNS its children.
    children: RefCell<Vec<ElementRef>>,
    // Weak: a child REFERENCES its parent without owning it.
    parent: RefCell<Weak<Element>>,
}

impl Element {
    fn new(tag: &str) -> ElementRef {
        Rc::new(Element {
            tag: tag.to_string(),
            children: RefCell::new(Vec::new()),
            parent: RefCell::new(Weak::new()), // no parent yet
        })
    }

    fn append_child(parent: &ElementRef, child: ElementRef) {
        // Rc::downgrade turns a strong Rc into a non-owning Weak.
        *child.parent.borrow_mut() = Rc::downgrade(parent);
        parent.children.borrow_mut().push(child);
    }

    fn path(&self) -> String {
        // upgrade() returns Option<Rc<Element>>: Some if the parent is alive.
        match self.parent.borrow().upgrade() {
            Some(parent) => format!("{}/{}", parent.path(), self.tag),
            None => self.tag.clone(),
        }
    }
}

fn main() {
    let html = Element::new("html");
    let body = Element::new("body");
    let para = Element::new("p");

    Element::append_child(&html, Rc::clone(&body));
    Element::append_child(&body, Rc::clone(&para));

    println!("{}", para.path()); // "html/body/p"
    println!("body has {} child(ren)", body.children.borrow().len()); // 1
}
```

Real output:

```text
html/body/p
body has 1 child(ren)
```

The shape mirrors the TypeScript tree: children point down with strong references and back up with weak ones. The important difference is that in Rust the *direction* of ownership is something you choose and the compiler/runtime enforces, whereas in JavaScript every reference is equally "strong" and the GC sorts it out.

> **Note:** `RefCell` provides interior mutability so we can rewrite `parent`/`children` through a shared `&` reference. See [`02_refcell-mutex.md`](/10-smart-pointers/02-refcell-mutex/) for why that wrapper is needed here.

---

## Detailed Explanation

### Why a cycle of `Rc` leaks

Every `Rc<T>` allocation carries two counts in its heap header: a **strong count** and a **weak count**. The value `T` is dropped when the strong count reaches `0`; the allocation itself is freed when *both* counts reach `0`. If two `Rc`s own each other, their strong counts can never fall to zero:

```rust
// This LEAKS: a strong cycle whose Drop never runs.
use std::cell::RefCell;
use std::rc::Rc;

struct Node {
    name: String,
    parent: RefCell<Option<Rc<Node>>>, // STRONG back-pointer — the bug
    children: RefCell<Vec<Rc<Node>>>,
}

impl Drop for Node {
    fn drop(&mut self) {
        println!("Dropping node {}", self.name);
    }
}

fn main() {
    let parent = Rc::new(Node {
        name: "parent".to_string(),
        parent: RefCell::new(None),
        children: RefCell::new(vec![]),
    });
    let child = Rc::new(Node {
        name: "child".to_string(),
        parent: RefCell::new(None),
        children: RefCell::new(vec![]),
    });

    parent.children.borrow_mut().push(Rc::clone(&child)); // parent -> child (strong)
    *child.parent.borrow_mut() = Some(Rc::clone(&parent)); // child -> parent (strong)

    println!("parent strong = {}", Rc::strong_count(&parent));
    println!("child strong  = {}", Rc::strong_count(&child));
}
```

Real output:

```text
parent strong = 2
child strong  = 2
```

Notice what is **missing**: there are no `Dropping node ...` lines. When `main` ends, `parent`'s strong count drops from 2 to 1 (the `child.parent` field still holds one) and `child`'s drops from 2 to 1 (the `parent.children` field still holds one). Both stall at 1, neither destructor runs, and the memory is leaked. This compiles and runs without a single warning: the leak is logically wrong but not *unsafe*, which is why the borrow checker does not catch it.

### The fix: make the back-pointer `Weak`

Change `parent` from `Rc<Node>` to `Weak<Node>` and the cycle is broken, because a `Weak` does not raise the strong count.

```rust
use std::cell::RefCell;
use std::rc::{Rc, Weak};

struct Node {
    name: String,
    parent: RefCell<Weak<Node>>,      // Weak — does NOT keep parent alive
    children: RefCell<Vec<Rc<Node>>>, // Strong — parent owns its children
}

impl Drop for Node {
    fn drop(&mut self) {
        println!("Dropping node {}", self.name);
    }
}

fn main() {
    let parent = Rc::new(Node {
        name: "parent".to_string(),
        parent: RefCell::new(Weak::new()),
        children: RefCell::new(vec![]),
    });
    let child = Rc::new(Node {
        name: "child".to_string(),
        parent: RefCell::new(Weak::new()),
        children: RefCell::new(vec![]),
    });

    parent.children.borrow_mut().push(Rc::clone(&child));
    *child.parent.borrow_mut() = Rc::downgrade(&parent); // strong -> weak

    println!(
        "parent strong = {}, weak = {}",
        Rc::strong_count(&parent),
        Rc::weak_count(&parent)
    );
    println!(
        "child strong  = {}, weak = {}",
        Rc::strong_count(&child),
        Rc::weak_count(&child)
    );

    if let Some(p) = child.parent.borrow().upgrade() {
        println!("child's parent is {}", p.name);
    }
}
```

Real output:

```text
parent strong = 1, weak = 1
child strong  = 2, weak = 0
child's parent is parent
Dropping node parent
Dropping node child
```

Now both destructors run. The `parent` has `strong = 1` (the local `parent` binding) and `weak = 1` (the `child.parent` field). When `main` ends, `parent`'s strong count goes to `0`, its `Node` is dropped, which drops the `children` vector, which drops the last strong reference to `child`, which then drops too. No leak.

### `downgrade` and `upgrade`

These two functions are the whole API:

| Operation | Signature | Meaning |
| --- | --- | --- |
| `Rc::downgrade(&rc)` | `&Rc<T> -> Weak<T>` | Create a non-owning handle; bumps the **weak** count |
| `weak.upgrade()` | `&Weak<T> -> Option<Rc<T>>` | Try to get a real, owning `Rc`; bumps the **strong** count if it succeeds |
| `Weak::new()` | `() -> Weak<T>` | An empty `Weak` that points at nothing (always upgrades to `None`) |

`upgrade()` is the safety valve. Because the data may already be gone, you are *forced* to handle the `None` case; there is no way to dereference a `Weak` directly:

```rust
use std::rc::{Rc, Weak};

fn main() {
    let weak: Weak<i32>;
    {
        let strong = Rc::new(42);
        weak = Rc::downgrade(&strong);
        println!("inside scope: upgrade() = {:?}", weak.upgrade()); // Some(42)
    } // `strong` dropped here -> value deallocated

    println!("after scope:  upgrade() = {:?}", weak.upgrade()); // None

    let empty: Weak<i32> = Weak::new();
    println!("empty:        upgrade() = {:?}", empty.upgrade()); // None
}
```

Real output:

```text
inside scope: upgrade() = Some(42)
after scope:  upgrade() = None
empty:        upgrade() = None
```

This is exactly the discipline JavaScript's `WeakRef.deref()` asks of you (it may return `undefined`), except Rust encodes the possibility in the type system: `Option<Rc<T>>` cannot be ignored without the compiler complaining.

### `strong_count` vs `weak_count`

`Rc::strong_count` and `Rc::weak_count` let you inspect the header counts, which is invaluable when debugging "why didn't this drop?" issues. A live `Weak` raises `weak_count` but never `strong_count`; only `upgrade()` (while it succeeds) momentarily raises the strong count for the lifetime of the returned `Rc`.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Reclaiming cycles | Automatic: the GC traces reachability | Manual design: you must break cycles with `Weak<T>` |
| Default reference strength | Every reference is "strong" | You choose `Rc`/`Arc` (strong) or `Weak` (weak) per field |
| Non-owning reference | `WeakRef<T>` | `Weak<T>` |
| Reading through a weak ref | `weak.deref()` -> `T \| undefined` | `weak.upgrade()` -> `Option<Rc<T>>` |
| What happens on a leak | Effectively impossible with the GC | `Rc` cycles silently leak (safe, but wrong) |
| When the value drops | When the GC decides nothing is reachable | Deterministically, when the last `Rc` strong count hits 0 |
| Single-thread vs multi-thread | One model | `std::rc::Weak` (with `Rc`) or `std::sync::Weak` (with `Arc`) |

### Rust's reasoning

Rust's ownership model is built on **deterministic destruction**: you know *exactly* when a value is freed (when its owning scope ends or its last strong owner is dropped). A GC would undermine that guarantee. The trade-off is that *you* are responsible for not constructing ownership cycles, and `Weak<T>` is the tool the standard library gives you to express "I want to look at this, but I don't own it." The mental model that maps cleanly from JavaScript is:

> **strong reference (`Rc`/`Arc`) = "keep this alive"; weak reference (`Weak`) = "let me peek if it's still around."**

### Two `Weak` types

There are two distinct `Weak` types, and they are *not* interchangeable:

- `std::rc::Weak<T>` pairs with `Rc<T>` — single-threaded, cheaper.
- `std::sync::Weak<T>` pairs with `Arc<T>` — thread-safe, atomic counts.

```rust
// The thread-safe analog: Arc::downgrade produces a std::sync::Weak.
use std::sync::{Arc, Weak};

fn main() {
    let strong = Arc::new("shared".to_string());
    let weak: Weak<String> = Arc::downgrade(&strong);

    println!(
        "strong = {}, weak = {}",
        Arc::strong_count(&strong),
        Arc::weak_count(&strong)
    );
    println!("upgrade = {:?}", weak.upgrade());

    drop(strong);
    println!("after drop, upgrade = {:?}", weak.upgrade());
}
```

Real output:

```text
strong = 1, weak = 1
upgrade = Some("shared")
after drop, upgrade = None
```

The API is identical; only the import (`std::sync` vs `std::rc`) and the partner type (`Arc` vs `Rc`) change. See [`01_rc-arc.md`](/10-smart-pointers/01-rc-arc/) for the strong-pointer differences.

---

## Common Pitfalls

### Pitfall 1: Trying to use a `Weak` as if it were the value

A `Weak<T>` does **not** implement `Deref`, so you cannot read fields or call methods on it directly. You must `upgrade()` first.

```rust
use std::rc::{Rc, Weak};

struct Node {
    value: i32,
}

fn main() {
    let strong = Rc::new(Node { value: 10 });
    let weak: Weak<Node> = Rc::downgrade(&strong);

    // does not compile (error[E0609]: no field `value` on type `std::rc::Weak<Node>`)
    println!("{}", weak.value);
}
```

Real compiler error:

```text
error[E0609]: no field `value` on type `std::rc::Weak<Node>`
  --> src/main.rs:12:25
   |
12 |     println!("{}", weak.value);
   |                         ^^^^^ unknown field
```

The fix is to upgrade and handle the `Option`: `if let Some(node) = weak.upgrade() { println!("{}", node.value); }`.

### Pitfall 2: Forgetting that `upgrade()` returns an `Option`

Even after you remember to call `upgrade()`, the result is `Option<Rc<T>>`, not `Rc<T>`. You still have to unwrap or pattern-match it.

```rust
use std::rc::{Rc, Weak};

struct Node {
    value: i32,
}

fn main() {
    let strong = Rc::new(Node { value: 10 });
    let weak: Weak<Node> = Rc::downgrade(&strong);

    // does not compile (error[E0609]: no field `value` on type `Option<Rc<Node>>`)
    let node = weak.upgrade();
    println!("{}", node.value);
}
```

Real compiler error:

```text
error[E0609]: no field `value` on type `Option<Rc<Node>>`
  --> src/main.rs:13:25
   |
13 |     println!("{}", node.value);
   |                         ^^^^^ unknown field
   |
help: one of the expressions' fields has a field of the same name
   |
13 |     println!("{}", node.unwrap().value);
   |                         +++++++++
```

> **Tip:** The compiler suggests `.unwrap()`, but in real code prefer `if let Some(node) = weak.upgrade()` or `weak.upgrade()?` so a dropped target turns into a graceful `None` rather than a panic.

### Pitfall 3: Making the child-to-parent link strong "for convenience"

This is the original leak. If both directions are `Rc`, nothing is ever freed. The rule of thumb that prevents it: **in a hierarchy, the owner direction is `Rc`/`Arc`; the back-reference is `Weak`.** A parent owns its children; a child merely *knows* its parent.

### Pitfall 4: Expecting an `upgraded` `Rc` to "stay" weak

`upgrade()` returns a genuine strong `Rc`. While you hold that `Rc`, the value is guaranteed alive and the strong count is raised. That is the point (it lets you safely use the value), but if you stash that upgraded `Rc` into a long-lived field, you have just re-created a strong reference (and possibly a cycle). Upgrade, use, and let the temporary `Rc` drop.

---

## Best Practices

### 1. Encode ownership direction deliberately

Decide which way ownership flows and stick to it: **strong down the tree, weak back up**. This single rule eliminates the vast majority of `Rc` cycles before they happen.

### 2. Initialize back-pointers with `Weak::new()`

When constructing a node before its parent exists, seed the field with `Weak::new()`. It upgrades to `None`, which correctly models "no parent yet" (a root node), and you fill it in with `Rc::downgrade` once the parent is known.

### 3. Always handle the `None` from `upgrade()`

Treat `upgrade()` like any other fallible operation. Use `if let`, `match`, or the `?` operator. Reaching for `.unwrap()` defeats the purpose of `Weak`: the whole reason the value is weak is that it might be gone.

### 4. Use `Weak` for caches and observers, not just trees

A registry of `Weak` handles is a clean way to build a cache or an observer list that does **not** keep its entries alive: dead entries simply fail to upgrade and can be pruned. (See Exercise 3.)

### 5. Reach for `Weak` only when you actually need shared ownership

Most parent/child relationships in Rust are better modeled with plain references and lifetimes, an arena/index-based graph (e.g. storing nodes in a `Vec` and referring to them by `usize` index), or an ownership tree without back-pointers. `Rc`/`Weak` is the right tool when you genuinely need *shared* ownership with back-references and the graph shape is dynamic.

> **Tip:** Index-based graphs (a `Vec<Node>` plus `usize` "handles") sidestep both reference counting and `Weak` entirely, and are often the idiomatic choice for performance-sensitive graph and ECS code.

---

## Real-World Example

A DOM-like element tree is the canonical case: rendering walks *down* into children, while event bubbling and selector matching walk *up* to parents and the document root. The downward links own the nodes; the upward links must not, or the document would never free.

```rust
// Real-world: a DOM-like element tree with safe upward navigation.
use std::cell::RefCell;
use std::rc::{Rc, Weak};

type ElementRef = Rc<Element>;

struct Element {
    tag: String,
    children: RefCell<Vec<ElementRef>>, // strong: parent owns children
    parent: RefCell<Weak<Element>>,     // weak: child references parent
}

impl Element {
    fn new(tag: &str) -> ElementRef {
        Rc::new(Element {
            tag: tag.to_string(),
            children: RefCell::new(Vec::new()),
            parent: RefCell::new(Weak::new()),
        })
    }

    fn append_child(parent: &ElementRef, child: ElementRef) {
        *child.parent.borrow_mut() = Rc::downgrade(parent);
        parent.children.borrow_mut().push(child);
    }

    /// Build a `/html/body/p`-style path by walking weak parent links up.
    fn path(&self) -> String {
        match self.parent.borrow().upgrade() {
            Some(parent) => format!("{}/{}", parent.path(), self.tag),
            None => self.tag.clone(),
        }
    }
}

impl Drop for Element {
    fn drop(&mut self) {
        println!("freeing <{}>", self.tag);
    }
}

fn main() {
    let html = Element::new("html");
    let body = Element::new("body");
    let para = Element::new("p");

    Element::append_child(&html, Rc::clone(&body));
    Element::append_child(&body, Rc::clone(&para));

    // Navigate downward (strong) and upward (weak).
    println!("path to <p>: {}", para.path());
    println!("<body> has {} child(ren)", body.children.borrow().len());
    println!(
        "<html> strong={}, weak={}",
        Rc::strong_count(&html),
        Rc::weak_count(&html)
    );

    drop(para);
    drop(body);
    println!("-- dropping document root --");
    drop(html);
}
```

Real output:

```text
path to <p>: html/body/p
<body> has 1 child(ren)
<html> strong=1, weak=1
-- dropping document root --
freeing <html>
freeing <body>
freeing <p>
```

The `<html>` node has `strong=1` (the `html` binding) and `weak=1` (the back-pointer from `<body>`). When we drop the document root, the entire tree is reclaimed in order — no leak, fully deterministic. Swap `Rc`/`std::rc::Weak` for `Arc`/`std::sync::Weak` and the same structure works across threads.

---

## Further Reading

### Official Documentation

- [`std::rc::Weak`](https://doc.rust-lang.org/std/rc/struct.Weak.html) — the single-threaded weak pointer
- [`std::sync::Weak`](https://doc.rust-lang.org/std/sync/struct.Weak.html) — the thread-safe weak pointer
- [The Rust Book: Reference Cycles Can Leak Memory](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html)
- [`Rc::downgrade`](https://doc.rust-lang.org/std/rc/struct.Rc.html#method.downgrade) and [`Weak::upgrade`](https://doc.rust-lang.org/std/rc/struct.Weak.html#method.upgrade)

### Related Topics

- [`01_rc-arc.md`](/10-smart-pointers/01-rc-arc/): strong shared ownership; `Weak` is its non-owning counterpart
- [`02_refcell-mutex.md`](/10-smart-pointers/02-refcell-mutex/) — interior mutability, used here to mutate `parent`/`children`
- [`00_box.md`](/10-smart-pointers/00-box/): single-owner heap allocation and recursive types
- [`07_comparison.md`](/10-smart-pointers/07-comparison/) — decision guide for choosing a smart pointer
- [Section 05: Ownership](/05-ownership/) — the model that makes all of this necessary
- [Section 02: Basics](/02-basics/) — types and ownership fundamentals
- [Section 11: Async](/11-async/): where `Arc`/`Weak` show up in shared concurrent state

---

## Exercises

### Exercise 1: Count the Living

**Difficulty:** Beginner

**Objective:** Practice `downgrade` and `upgrade` and observe how a `Weak` reflects whether its target is alive.

**Instructions:** Given a `Vec<Weak<i32>>`, write code that returns how many of the weak references still point to a live value. Construct a scenario where at least one target has been dropped, and verify the count.

```rust
use std::rc::{Rc, Weak};

fn count_living(weaks: &[Weak<i32>]) -> usize {
    // TODO: count how many entries still upgrade to Some
    /* ??? */
}

fn main() {
    // TODO: build some Rc<i32>, downgrade them, drop one, then count.
}
```

<details>
<summary>Solution</summary>

```rust
use std::rc::{Rc, Weak};

fn count_living(weaks: &[Weak<i32>]) -> usize {
    weaks.iter().filter(|w| w.upgrade().is_some()).count()
}

fn main() {
    let a = Rc::new(1);
    let mut all: Vec<Weak<i32>> = vec![Rc::downgrade(&a), Rc::downgrade(&a)];

    let b = Rc::new(2);
    all.push(Rc::downgrade(&b));

    drop(b); // invalidate the third weak

    println!("living = {}", count_living(&all)); // 2
}
```

Real output:

```text
living = 2
```

</details>

### Exercise 2: Walk the Ancestors

**Difficulty:** Intermediate

**Objective:** Build a parent/child tree with weak back-pointers and traverse upward without leaking.

**Instructions:** Implement a `TreeNode` with a strong `children` vector and a `Weak` `parent`. Add an `ancestors(&self) -> Vec<i32>` method that returns the values of every ancestor from the immediate parent up to the root, by following the weak links.

```rust
use std::cell::RefCell;
use std::rc::{Rc, Weak};

struct TreeNode {
    value: i32,
    parent: RefCell<Weak<TreeNode>>,
    children: RefCell<Vec<Rc<TreeNode>>>,
}

impl TreeNode {
    fn new(value: i32) -> Rc<TreeNode> { /* ??? */ }
    fn add_child(parent: &Rc<TreeNode>, child: Rc<TreeNode>) { /* ??? */ }
    fn ancestors(&self) -> Vec<i32> {
        // TODO: walk parent links upward, collecting values
        /* ??? */
    }
}

fn main() {
    // Build root(1) -> mid(2) -> leaf(3); print leaf.ancestors()
}
```

<details>
<summary>Solution</summary>

```rust
use std::cell::RefCell;
use std::rc::{Rc, Weak};

struct TreeNode {
    value: i32,
    parent: RefCell<Weak<TreeNode>>,
    children: RefCell<Vec<Rc<TreeNode>>>,
}

impl TreeNode {
    fn new(value: i32) -> Rc<TreeNode> {
        Rc::new(TreeNode {
            value,
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(Vec::new()),
        })
    }

    fn add_child(parent: &Rc<TreeNode>, child: Rc<TreeNode>) {
        *child.parent.borrow_mut() = Rc::downgrade(parent);
        parent.children.borrow_mut().push(child);
    }

    fn ancestors(&self) -> Vec<i32> {
        let mut out = Vec::new();
        let mut current = self.parent.borrow().upgrade();
        while let Some(node) = current {
            out.push(node.value);
            current = node.parent.borrow().upgrade();
        }
        out
    }
}

fn main() {
    let root = TreeNode::new(1);
    let mid = TreeNode::new(2);
    let leaf = TreeNode::new(3);
    TreeNode::add_child(&root, Rc::clone(&mid));
    TreeNode::add_child(&mid, Rc::clone(&leaf));

    println!("ancestors of leaf: {:?}", leaf.ancestors()); // [2, 1]
}
```

Real output:

```text
ancestors of leaf: [2, 1]
```

</details>

### Exercise 3: A Weak-Valued Cache

**Difficulty:** Advanced

**Objective:** Use `Weak` to build a cache that does **not** keep its values alive; entries vanish automatically once the last strong owner is dropped.

**Instructions:** Implement a `Cache` storing `HashMap<String, Weak<String>>`. The method `get_or_insert(key, make)` should return a live `Rc<String>` if the cached `Weak` still upgrades, otherwise build a new value with `make`, cache a `Weak` to it, and return the strong `Rc`. Demonstrate that a second call with both owners still alive hits the cache, but after dropping all strong owners the next call rebuilds.

```rust
use std::collections::HashMap;
use std::rc::{Rc, Weak};

struct Cache {
    map: HashMap<String, Weak<String>>,
}

impl Cache {
    fn new() -> Self { /* ??? */ }
    fn get_or_insert(&mut self, key: &str, make: impl FnOnce() -> String) -> Rc<String> {
        // TODO: return the cached value if it still upgrades, else build+cache
        /* ??? */
    }
}

fn main() {
    // Show a cache hit, then drop all owners and show a rebuild.
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;
use std::rc::{Rc, Weak};

struct Cache {
    map: HashMap<String, Weak<String>>,
}

impl Cache {
    fn new() -> Self {
        Cache { map: HashMap::new() }
    }

    fn get_or_insert(&mut self, key: &str, make: impl FnOnce() -> String) -> Rc<String> {
        if let Some(existing) = self.map.get(key).and_then(Weak::upgrade) {
            return existing;
        }
        let value = Rc::new(make());
        self.map.insert(key.to_string(), Rc::downgrade(&value));
        value
    }
}

fn main() {
    let mut cache = Cache::new();

    let a = cache.get_or_insert("k", || {
        println!("building k");
        "value".to_string()
    });
    // Both owners alive -> cache hit, no "building k".
    let b = cache.get_or_insert("k", || {
        println!("building k");
        "value".to_string()
    });
    println!("same allocation: {}", Rc::ptr_eq(&a, &b));

    drop(a);
    drop(b); // last strong owner gone -> cached Weak is now dead

    // Rebuilds: "building k" prints again.
    let _c = cache.get_or_insert("k", || {
        println!("building k");
        "value".to_string()
    });
}
```

Real output:

```text
building k
same allocation: true
building k
```

The first call builds and caches a `Weak`; the second upgrades the live `Weak` (same allocation, no rebuild). After both strong owners drop, the cached `Weak` can no longer upgrade, so the third call rebuilds. This is precisely how a memory-sensitive cache avoids keeping objects alive purely for caching.

</details>

---

## Summary

- A cycle of `Rc<T>`/`Arc<T>` **leaks** in Rust — there is no GC to reclaim it. The strong counts hold each other above zero forever.
- **`Weak<T>`** is a non-owning reference: it raises the *weak* count, never the *strong* count, so it never keeps a value alive.
- Create one with **`Rc::downgrade(&rc)`** (or `Arc::downgrade`), and access the data with **`upgrade()`**, which returns **`Option<Rc<T>>`** — `Some` if alive, `None` if dropped.
- The idiomatic rule: **own downward with strong references, point back upward with weak ones.**
- `std::rc::Weak` pairs with `Rc` (single-threaded); `std::sync::Weak` pairs with `Arc` (thread-safe). The API is otherwise identical.
