---
title: "Shared Ownership with `Rc<T>` and `Arc<T>`"
description: "Rc<T> and Arc<T> give a heap value multiple owners via reference counting, recreating JavaScript's shared references but freed deterministically at count zero."
---

In TypeScript and JavaScript, sharing is invisible and free: assign an object to ten variables and all ten point at the same thing, with a garbage collector deciding when to clean up. Rust's default ownership model is the opposite (one value, exactly one owner), so to get that "many references to one object" behavior on purpose you reach for a **reference-counted smart pointer**: `Rc<T>` for single-threaded code and `Arc<T>` for sharing across threads.

---

## Quick Overview

`Rc<T>` (**reference counted**) and `Arc<T>` (**atomically reference counted**) are smart pointers that let a single heap-allocated value have **multiple owners**. Each pointer holds an integer count of how many owners exist; cloning the pointer bumps the count, dropping one lowers it, and when the count reaches zero the value is freed. For a TypeScript/JavaScript developer this is the closest Rust gets to "everything is a shared reference," except the bookkeeping is a tiny counter the compiler manages deterministically, not a tracing garbage collector that runs whenever it feels like it.

> **Note:** This page covers **shared ownership** specifically: `Rc` vs `Arc`, the strong count, and why cloning is cheap. The two reference-counting types are immutable on their own; to *mutate* shared data you combine them with the interior-mutability types in [Interior Mutability](/10-smart-pointers/02-refcell-mutex/). Breaking reference **cycles** (which `Rc`/`Arc` cannot do alone) is covered in [Weak References with `Weak<T>`](/10-smart-pointers/05-weak/). For heap allocation with a *single* owner, see [Box&lt;T&gt;](/10-smart-pointers/00-box/). Section 05 has a gentler first introduction in [Reference Counting](/05-ownership/07-reference-counting/); this page assumes you have met the [ownership rules](/05-ownership/01-ownership-rules/).

---

## TypeScript/JavaScript Example

In JavaScript and TypeScript, object variables are references, and assignment shares the reference rather than copying the object. A configuration object can be handed to many parts of an application, and they all see — and reach — the same instance.

```typescript
// TypeScript/JavaScript: object references are shared implicitly
interface AppConfig {
  serviceName: string;
  featureFlags: string[];
}

const config: AppConfig = {
  serviceName: "checkout",
  featureFlags: ["new-cart", "fast-pay"],
};

// A handler holds the SAME config object, not a copy of it.
class Handler {
  constructor(
    public route: string,
    public config: AppConfig,
  ) {}
  describe(): string {
    return `${this.route} -> service=${this.config.serviceName}`;
  }
}

const handlers = [
  new Handler("/cart", config),
  new Handler("/pay", config),
  new Handler("/health", config),
];

for (const h of handlers) {
  console.log(h.describe());
}

// You never count references and never free anything. When `config` and
// every `handler` become unreachable, the garbage collector reclaims the
// object — at some unspecified later time.
```

**Key points:**

- Passing `config` into three `Handler`s does **not** copy it; all three hold a reference to one object.
- There is no count you can read and no `free` to call. Cleanup is **non-deterministic**: the garbage collector decides when.
- Any holder of the reference could mutate the shared object (`config.featureFlags.push(...)`) and the change would be visible to all. Rust will make that mutation explicit and opt-in.

---

## Rust Equivalent

Rust will not let you freely alias an owned value. That is the entire point of the [ownership rules](/05-ownership/01-ownership-rules/). To opt into shared ownership you wrap the value in `Rc<T>` and create additional owners with `Rc::clone`. Each clone is a cheap reference-count bump that hands back another owner pointing at the **same** allocation.

```rust
use std::rc::Rc;

// A read-only piece of shared context every request handler can see.
#[derive(Debug)]
struct AppConfig {
    service_name: String,
    feature_flags: Vec<String>,
}

struct Handler {
    route: String,
    config: Rc<AppConfig>, // shared ownership of the same config
}

impl Handler {
    fn new(route: &str, config: &Rc<AppConfig>) -> Self {
        Handler {
            route: route.to_string(),
            config: Rc::clone(config), // cheap: bump the count
        }
    }

    fn describe(&self) -> String {
        format!(
            "{} -> service={}, flags={:?}",
            self.route, self.config.service_name, self.config.feature_flags
        )
    }
}

fn main() {
    let config = Rc::new(AppConfig {
        service_name: "checkout".to_string(),
        feature_flags: vec!["new-cart".to_string(), "fast-pay".to_string()],
    });

    let handlers = vec![
        Handler::new("/cart", &config),
        Handler::new("/pay", &config),
        Handler::new("/health", &config),
    ];

    // 1 original + 3 handlers all sharing one allocation.
    println!("config shared by {} owners", Rc::strong_count(&config));

    for h in &handlers {
        println!("{}", h.describe());
    }

    // Dropping all handlers releases their handles; only `config` remains.
    drop(handlers);
    println!("after handlers dropped: {} owner", Rc::strong_count(&config));
}
```

**Real output:**

```text
config shared by 4 owners
/cart -> service=checkout, flags=["new-cart", "fast-pay"]
/pay -> service=checkout, flags=["new-cart", "fast-pay"]
/health -> service=checkout, flags=["new-cart", "fast-pay"]
after handlers dropped: 1 owner
```

The data lives once on the heap. The `Vec<Handler>` and the original `config` variable together own four handles to it; when the `Vec` is dropped, three handles go away and the count drops to one.

---

## Detailed Explanation

Let's walk through what each piece does and contrast it with the JavaScript version.

### `Rc::new(value)` — move the value onto the heap and start counting

```rust
let config = Rc::new(AppConfig { /* ... */ });
```

`Rc::new` takes ownership of the `AppConfig`, allocates it on the heap alongside a counter, and returns an `Rc<AppConfig>`, a pointer whose **strong count** starts at `1`. In JavaScript the object was already on the GC heap with no visible counter; in Rust the allocation and the counter are explicit and tied together.

### `Rc::clone(&config)` — add an owner, don't copy the data

```rust
config: Rc::clone(config), // `config` here is the &Rc passed into `new`
```

This is the line that trips up newcomers. `Rc::clone` does **not** deep-copy the `AppConfig`. It increments the strong count and returns a new `Rc` pointing at the *same* heap allocation. That is why the output shows four owners of one allocation rather than four separate configs. It is the same sharing you got for free in JavaScript, only here it is named and counted.

> **Tip:** The idiom is `Rc::clone(&x)` (fully-qualified) rather than `x.clone()`. Both do exactly the same thing — a cheap ref-count bump — but the explicit form signals to a reader "this is a shared-ownership bump, not an expensive deep clone." The standard library and Clippy both encourage it.

### `Rc::strong_count(&config)` — read the live owner count

```rust
Rc::strong_count(&config) // -> 4, then 1 after the handlers drop
```

`strong_count` returns how many `Rc` handles currently point at the allocation. It is an associated function (`Rc::strong_count(&x)`), not a method, again to avoid colliding with methods on the wrapped type. There is no equivalent in JavaScript: the GC's reference graph is not something you can query at runtime.

### `drop(handlers)` — release owners; data freed only at zero

When the `Vec<Handler>` is dropped, each `Handler` is dropped, each one's `Rc<AppConfig>` field is dropped, and each drop decrements the count. The `AppConfig` itself is freed **only** when the count reaches zero, which here it never does, because `config` still holds the last handle. This is RAII reference counting: deterministic, tied to scope, and visible in the code.

### Accessing the data — `Deref` makes it transparent

Notice `self.config.service_name`: you read fields straight through the `Rc` as if it were the `AppConfig`. `Rc<T>` implements `Deref` to `T`, so `&Rc<AppConfig>` coerces to `&AppConfig` automatically (the mechanics live in [The `Deref` Trait and Deref Coercion](/10-smart-pointers/06-deref-trait/)). The catch: `Deref` gives you a **shared** `&T`, never `&mut T` — shared ownership is read-only by default, which is why mutation needs the extra tools mentioned below.

---

## `Rc` vs `Arc`: single-threaded vs thread-safe

`Rc` and `Arc` have an almost identical API. The difference is *how the counter is updated*:

- `Rc<T>` uses an ordinary integer increment/decrement. This is fast but not safe if two threads touch the count at once, so `Rc<T>` is deliberately **not** `Send`/`Sync`: the compiler refuses to let it cross a thread boundary.
- `Arc<T>` updates the count with **atomic** CPU instructions, which makes sharing across threads safe. Atomics cost a little more than a plain increment, which is the only reason `Rc` still exists: don't pay for thread-safety you don't use.

Here is `Arc<T>` shared across threads. Swap `use std::rc::Rc` for `use std::sync::Arc` and the shape is the same:

```rust
use std::sync::Arc;
use std::thread;

#[derive(Debug)]
struct Config {
    base_url: String,
    retries: u32,
}

fn main() {
    // Arc = Atomically Reference Counted. Same API as Rc, but the count
    // is updated atomically so it is safe to share across threads.
    let config = Arc::new(Config {
        base_url: String::from("https://api.example.com"),
        retries: 3,
    });

    let mut handles = Vec::new();
    for worker_id in 0..3 {
        // Each thread gets its own owning handle (a cheap ref-count bump).
        let config = Arc::clone(&config);
        let handle = thread::spawn(move || {
            // The thread reads the shared config without copying it.
            println!(
                "worker {worker_id}: {} (retries={})",
                config.base_url, config.retries
            );
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Every worker has finished and dropped its handle, so only the
    // original owner remains.
    println!("final count = {}", Arc::strong_count(&config));
}
```

**Real output** (worker order varies; threads race):

```text
worker 1: https://api.example.com (retries=3)
worker 2: https://api.example.com (retries=3)
worker 0: https://api.example.com (retries=3)
final count = 1
```

Each thread `move`s its own `Arc` handle into the closure (`let config = Arc::clone(&config);` shadows the outer name with a fresh owner). After all threads `join`, their handles have been dropped and the count is back to `1`.

> **Note:** JavaScript has no real shared-memory threading for plain objects. Web Workers and Node `worker_threads` communicate by *copying* messages (structured clone) or via the low-level `SharedArrayBuffer`. `Arc<T>` is genuine shared memory across OS threads, with the borrow checker guaranteeing there are no data races. Threads themselves are covered more in [Section 11: Async and Concurrency](/11-async/).

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust `Rc<T>` / `Arc<T>` |
| --- | --- | --- |
| How sharing happens | Implicit: every object variable is a shared reference | Explicit: wrap in `Rc`/`Arc`, share with `::clone` |
| Cleanup timing | Non-deterministic (tracing GC) | Deterministic: freed the instant the count hits zero |
| Can you read the count? | No | Yes: `Rc::strong_count(&x)` / `Arc::strong_count(&x)` |
| Cost of "copying" a handle | Reference assignment | Count increment (atomic for `Arc`); data not copied |
| Mutation through a shared handle | Allowed by default | **Not** allowed by default; needs `RefCell`/`Mutex` ([Interior Mutability](/10-smart-pointers/02-refcell-mutex/)) |
| Cross-thread sharing | Workers copy messages; `SharedArrayBuffer` for bytes | `Arc<T>` is real shared memory, race-free at compile time |
| Reference cycles | Collected by tracing GC | **Leak**: must break with `Weak<T>` ([Weak References with `Weak<T>`](/10-smart-pointers/05-weak/)) |

### Why two types instead of one?

A common question: why not always use `Arc` and forget `Rc`? Because atomic operations have a real (if small) cost, and the *common* case — sharing within one thread — does not need them. Rust's philosophy is "zero-cost abstractions": you opt into thread-safety only when you actually share across threads. The compiler enforces the boundary, so you cannot accidentally use the cheaper `Rc` where `Arc` was required (see the first pitfall below).

### Shared ownership is read-only

This is the biggest surprise coming from JavaScript, where you can freely mutate a shared object. Both `Rc<T>` and `Arc<T>` only hand out shared (`&T`) access. That is a direct consequence of the borrow checker's one-mutable-XOR-many-shared rule: if many owners can read the data, none may mutate it through the pointer. To mutate shared data you wrap the inner value in a cell type — `Rc<RefCell<T>>` (single-thread) or `Arc<Mutex<T>>` (multi-thread) — which moves the borrow check to runtime. That combination is the subject of [Interior Mutability](/10-smart-pointers/02-refcell-mutex/).

---

## Common Pitfalls

### Pitfall 1: Trying to send an `Rc` across threads

A natural mistake is to share an `Rc` with a spawned thread. The compiler stops you, because `Rc`'s non-atomic counter is not thread-safe.

```rust
use std::rc::Rc;
use std::thread;

fn main() {
    let data = Rc::new(vec![1, 2, 3]);
    let data2 = Rc::clone(&data);

    // does not compile (error[E0277]: `Rc<Vec<i32>>` cannot be sent between threads safely)
    let handle = thread::spawn(move || {
        println!("{:?}", data2);
    });

    handle.join().unwrap();
    println!("{:?}", data);
}
```

**Real compiler error (abridged):**

```text
error[E0277]: `Rc<Vec<i32>>` cannot be sent between threads safely
   --> src/main.rs:9:32
    |
  9 |       let handle = thread::spawn(move || {
    |                    ------------- ^------
    |                    |             |
    |  __________________|_____________within this `{closure@src/main.rs:9:32: 9:39}`
    | |                  |
    | |                  required by a bound introduced by this call
...
    = help: within `{closure@src/main.rs:9:32: 9:39}`, the trait `Send` is not implemented for `Rc<Vec<i32>>`
note: required by a bound in `spawn`
```

**Fix:** swap `Rc` for `Arc` (and `std::rc::Rc` for `std::sync::Arc`). The error literally points at the missing `Send` bound; the cure is the thread-safe variant.

### Pitfall 2: Expecting `.clone()` to copy the data

```rust
let original = Rc::new(vec![1, 2, 3]);
let alias = Rc::clone(&original);
// `alias` is NOT an independent Vec — it points at the SAME one.
```

If you genuinely want a separate copy of the inner data, clone the inner value, not the `Rc`: `let independent = (*original).clone();` (or `original.as_ref().clone()`). Reflexively writing `original.clone()` and expecting a deep copy is a TypeScript-shaped assumption: `Rc`'s clone is the cheap, sharing kind.

### Pitfall 3: Trying to mutate through an `Rc`

```rust
use std::rc::Rc;

fn main() {
    let counter = Rc::new(0_i32);
    let _other = Rc::clone(&counter);

    // does not compile (error[E0594]: cannot assign to data in an `Rc`)
    *counter += 1;
}
```

**Real compiler error:**

```text
error[E0594]: cannot assign to data in an `Rc`
 --> src/main.rs:8:5
  |
8 |     *counter += 1;
  |     ^^^^^^^^^^^^^ cannot assign
  |
  = help: trait `DerefMut` is required to modify through a dereference, but it is not implemented for `Rc<i32>`
```

**Fix:** shared ownership is read-only. For a mutable shared counter use `Rc<RefCell<i32>>` (single-thread) or `Arc<Mutex<i32>>` / an atomic type (multi-thread). See [Interior Mutability](/10-smart-pointers/02-refcell-mutex/).

### Pitfall 4: Creating a reference cycle (a silent memory leak)

`Rc`/`Arc` are reference counters, **not** a tracing garbage collector. If two `Rc`s point at each other — a parent holding a child that holds the parent back — neither count ever reaches zero and the memory leaks, even though both are unreachable. JavaScript's GC handles cycles automatically; Rust's counter cannot.

```rust
// Conceptual: A owns B, B owns A. Both strong counts stay >= 1 forever.
// The cure is to make one direction a Weak<T> (a non-owning handle).
```

This is important enough to have its own page: [Weak References with `Weak<T>`](/10-smart-pointers/05-weak/) shows how `Weak<T>` and `upgrade()` break cycles in a parent/child graph.

---

## Best Practices

- **Prefer borrowing first.** Reach for `Rc`/`Arc` only when a value genuinely has *no single owner*: a node shared by many edges, an immutable config many components hold. If one part of the code can own the data and everyone else can borrow `&T`, that is simpler and faster. Don't sprinkle `Arc` everywhere to "make the borrow checker happy."
- **Use `Rc::clone(&x)` / `Arc::clone(&x)`, not `x.clone()`.** Functionally identical, but the explicit form reads as "cheap ref-count bump" rather than a potentially expensive deep clone. (The Clippy lint `clippy::clone_on_ref_ptr` enforces this; it is allow-by-default, so enable it if your team wants the discipline.)
- **Default to `Rc`; upgrade to `Arc` only when crossing threads.** Don't pay for atomics you don't need. The compiler forces the upgrade when you actually share across threads, so starting with `Rc` is safe: you'll get a clear error if it ever needs to be `Arc`.
- **`Rc<RefCell<T>>` and `Arc<Mutex<T>>` are the standard "shared *and* mutable" combos.** When you see one of these patterns, read it as "many owners, with interior mutability." Keep `Arc<Mutex<_>>` for threads and `Rc<RefCell<_>>` for single-threaded code: they don't mix.
- **Watch for cycles in graph-shaped data.** Any time an `Rc`/`Arc` can transitively point back at itself (trees with parent pointers, doubly linked lists, observer graphs), make the back-edge a `Weak<T>`. See [Weak References with `Weak<T>`](/10-smart-pointers/05-weak/).
- **`strong_count` is for understanding and debugging, not control flow.** It is great in tests and `println!` debugging; avoid branching on it in production logic, especially with `Arc`, where the count can change between the read and your next line.

---

## Real-World Example

A practical, single-threaded scenario: a small interpreter or template engine where many evaluation nodes need read access to one shared, immutable environment (interned strings, built-in functions, configuration). Cloning the whole environment per node would be wasteful; borrowing would tangle lifetimes through the whole tree. Shared ownership via `Rc` is the clean fit, and `Drop` lets us *see* the deterministic cleanup.

```rust
use std::rc::Rc;

/// Read-only environment shared by every node in an evaluation tree.
#[derive(Debug)]
struct Environment {
    name: String,
    builtins: Vec<String>,
}

impl Drop for Environment {
    fn drop(&mut self) {
        // Proves the allocation is freed exactly once, at count zero.
        println!("[freed] Environment({})", self.name);
    }
}

/// A node that evaluates against the shared environment.
struct Node {
    label: String,
    env: Rc<Environment>,
}

impl Node {
    fn new(label: &str, env: &Rc<Environment>) -> Self {
        Node {
            label: label.to_string(),
            env: Rc::clone(env),
        }
    }

    fn eval(&self) {
        println!(
            "node {:>6} can call {} builtins from env '{}'",
            self.label,
            self.env.builtins.len(),
            self.env.name
        );
    }
}

fn main() {
    let env = Rc::new(Environment {
        name: "global".to_string(),
        builtins: vec!["len".to_string(), "print".to_string(), "map".to_string()],
    });

    // Build a little tree of nodes, all sharing the one environment.
    let nodes = vec![
        Node::new("expr-1", &env),
        Node::new("expr-2", &env),
        Node::new("expr-3", &env),
    ];

    println!("env owners: {}", Rc::strong_count(&env)); // 1 + 3 = 4

    for node in &nodes {
        node.eval();
    }

    // Drop the original handle: count goes 4 -> 3. NOT freed yet,
    // because the nodes still own handles.
    drop(env);
    println!("dropped original handle; tree still holds the env");

    // When `nodes` goes out of scope at the end of main, the last three
    // handles drop, the count hits zero, and Environment::drop runs once.
    println!("end of main: nodes about to drop...");
}
```

**Real output:**

```text
env owners: 4
node expr-1 can call 3 builtins from env 'global'
node expr-2 can call 3 builtins from env 'global'
node expr-3 can call 3 builtins from env 'global'
dropped original handle; tree still holds the env
end of main: nodes about to drop...
[freed] Environment(global)
```

The `[freed]` line printing **last** — after `drop(env)`, not at it — is the whole point: dropping one owner only lowers the count. The `Environment` is freed exactly once, deterministically, when the final owner (inside `nodes`) goes away at the end of `main`. No garbage collector, no double-free, no leak.

> **Tip:** For the multi-threaded version of this exact pattern — say worker threads sharing one immutable lookup table — change `Rc` to `Arc` and `std::rc::Rc` to `std::sync::Arc`. Everything else stays the same; that symmetry is deliberate.

---

## Further Reading

**Official documentation**

- [`std::rc::Rc`](https://doc.rust-lang.org/std/rc/struct.Rc.html) — the single-threaded reference-counted pointer
- [`std::sync::Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html) — the atomic, thread-safe variant
- [The Rust Book, ch. 15.4 — `Rc<T>`, the Reference Counted Smart Pointer](https://doc.rust-lang.org/book/ch15-04-rc.html)
- [The Rust Book, ch. 16.3 — Shared-State Concurrency](https://doc.rust-lang.org/book/ch16-03-shared-state.html) (where `Arc<Mutex<T>>` appears)
- [`Send` and `Sync`](https://doc.rust-lang.org/std/marker/trait.Send.html) — the marker traits that decide what may cross threads

**Related sections in this guide**

- [Box&lt;T&gt;](/10-smart-pointers/00-box/) — `Box<T>` for single-owner heap allocation (the contrast to shared ownership)
- [Interior Mutability](/10-smart-pointers/02-refcell-mutex/) — adding interior mutability: `Rc<RefCell<T>>` and `Arc<Mutex<T>>`
- [Weak References with `Weak<T>`](/10-smart-pointers/05-weak/) — `Weak<T>` to break the reference cycles `Rc`/`Arc` cannot
- [The `Deref` Trait and Deref Coercion](/10-smart-pointers/06-deref-trait/) — why you can read fields straight through an `Rc`
- [Choosing a Smart Pointer](/10-smart-pointers/07-comparison/) — a decision table for choosing among all the smart pointers
- [Section 05 — Reference Counting](/05-ownership/07-reference-counting/) — the first, gentler introduction
- [Section 05 — The Ownership Rules](/05-ownership/01-ownership-rules/) and [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/) — the foundation this builds on
- [Section 11 — Async and Concurrency](/11-async/) — where `Arc` is used heavily

---

## Exercises

### Exercise 1: Share a dictionary among readers

**Difficulty:** Easy

**Objective:** Get comfortable with `Rc::new`, `Rc::clone`, and `Rc::strong_count`.

**Instructions:**

1. Create an `Rc<Vec<String>>` holding a few words (a shared dictionary).
2. Create two additional reader handles with `Rc::clone`.
3. Print the strong count (it should be `3`), then read an element through each reader.
4. `drop` one reader and print the count again (it should be `2`).

<details>
<summary>Solution</summary>

```rust
use std::rc::Rc;

fn main() {
    let dictionary = Rc::new(vec![
        "alpha".to_string(),
        "bravo".to_string(),
        "charlie".to_string(),
    ]);

    let reader_a = Rc::clone(&dictionary);
    let reader_b = Rc::clone(&dictionary);

    println!("count = {}", Rc::strong_count(&dictionary)); // 3
    println!("a[0] = {}", reader_a[0]);
    println!("b[2] = {}", reader_b[2]);

    drop(reader_a);
    println!("count after one drop = {}", Rc::strong_count(&dictionary)); // 2
    // reader_b is still owned here and lives to the end of `main`, keeping the
    // count at 2. (Binding it to `_` would drop it immediately, so we don't.)
}
```

**Output:**

```text
count = 3
a[0] = alpha
b[2] = charlie
count after one drop = 2
```

</details>

### Exercise 2: Observe deterministic cleanup

**Difficulty:** Medium

**Objective:** Prove to yourself that the inner value is freed exactly once, at the moment the count reaches zero, not before.

**Instructions:**

1. Define a `struct Resource { label: String }` and implement `Drop` for it so that dropping prints `dropping Resource(<label>)`.
2. Wrap one `Resource` in an `Rc` and make a second handle with `Rc::clone`.
3. `drop` the first handle and print a line afterward. Confirm (by output order) that the `Resource` is **not** dropped yet.
4. `drop` the second handle and confirm the `Resource` is dropped only now.

<details>
<summary>Solution</summary>

```rust
use std::rc::Rc;

struct Resource {
    label: String,
}

impl Drop for Resource {
    fn drop(&mut self) {
        println!("dropping Resource({})", self.label);
    }
}

fn main() {
    let a = Rc::new(Resource { label: "db-pool".into() });
    println!("count = {}", Rc::strong_count(&a)); // 1

    let b = Rc::clone(&a);
    println!("count = {}", Rc::strong_count(&a)); // 2

    drop(a); // count -> 1, Resource NOT dropped yet
    println!("after drop(a): still alive, count via b = {}", Rc::strong_count(&b));

    drop(b); // count -> 0, NOW the Resource is dropped
    println!("after drop(b): done");
}
```

**Output:**

```text
count = 1
count = 2
after drop(a): still alive, count via b = 1
dropping Resource(db-pool)
after drop(b): done
```

The `dropping Resource(db-pool)` line appears between `drop(b)` and the final print, proving cleanup happened exactly at count zero.

</details>

### Exercise 3: Parallel sum over a shared dataset

**Difficulty:** Hard

**Objective:** Share one immutable dataset across multiple threads with `Arc`, and confirm the count returns to `1` after the threads finish.

**Instructions:**

1. Build an `Arc<Vec<u64>>` containing the numbers `1..=1000`.
2. Spawn 4 threads. Give each its own `Arc::clone` and a distinct slice range of the data.
3. Each thread sums its slice and returns the partial sum from the closure.
4. `join` all threads, add the partial sums (expected total: `500500`), and print the final strong count (should be `1`).

> **Hint:** `Arc::clone(&data)` inside the loop, then `move` the clone into the closure. Index the slice with `data[start..end]`.

<details>
<summary>Solution</summary>

```rust
use std::sync::Arc;
use std::thread;

fn main() {
    let data: Arc<Vec<u64>> = Arc::new((1..=1000).collect());
    let chunk = data.len() / 4;

    let mut handles = Vec::new();
    for i in 0..4 {
        let data = Arc::clone(&data); // each thread's own owning handle
        let start = i * chunk;
        let end = if i == 3 { data.len() } else { start + chunk };
        handles.push(thread::spawn(move || {
            let partial: u64 = data[start..end].iter().sum();
            partial
        }));
    }

    let total: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
    println!("total = {total}");
    println!("owners remaining = {}", Arc::strong_count(&data));
}
```

**Output:**

```text
total = 500500
owners remaining = 1
```

After every thread joins, its `Arc` handle has been dropped, so only the original owner remains, and the count is back to `1`.

</details>
