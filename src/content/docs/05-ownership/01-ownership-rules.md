---
title: "The Three Ownership Rules"
description: "One owner per value, ownership moves on assignment or a function call, and values drop at scope end. How Rust frees memory deterministically without JavaScript's GC."
---

Ownership is the mechanism Rust uses to manage memory **without a garbage collector**. Instead of a runtime that periodically scans for unreachable objects (the JavaScript model), Rust decides at *compile time* exactly when each value is freed. The whole system rests on three short rules, and this page is about those rules and nothing more.

---

## Quick Overview

In Rust, every value has exactly one **owner**; ownership **moves** when you assign the value or pass it to a function; and when the owner goes out of scope, the value is **dropped** (its memory freed) automatically. There is no `new`/`delete`, no `free()`, and no garbage collector. The compiler tracks ownership and inserts cleanup for you. For a TypeScript/JavaScript developer, the surprising part is not the cleanup (the GC already hides that) but that *handing a value to someone else can make your own variable unusable*.

> **Note:** This page covers the rules themselves and the **move** that happens on assignment and function calls. Borrowing (`&`), which lets you lend access *without* giving up ownership, is the subject of [Borrowing and References](/05-ownership/02-borrowing/). The stack/heap distinction that explains *why* moves matter lives in [Stack and Heap](/05-ownership/00-stack-heap/).

---

## TypeScript/JavaScript Example

In JavaScript and TypeScript you never think about who "owns" a value. You create objects, pass them around, and the **garbage collector** (GC) frees them whenever it decides nothing can reach them anymore.

```typescript
// TypeScript/JavaScript: values are shared freely, GC cleans up "eventually"
function archiveOrder(orderId: string): void {
  console.log(`Archiving ${orderId}`);
}

let orderId = "ORD-2026-0531";
let archived = orderId; // both names now refer to the same string value

console.log(orderId); // "ORD-2026-0531" — still usable
console.log(archived); // "ORD-2026-0531"

archiveOrder(orderId); // pass it to a function...
console.log(orderId); // still usable afterward — nothing was "consumed"

// When `orderId` and `archived` become unreachable, the GC frees the memory
// at some unspecified later time. You never decide when.
```

**Key points:**

- Assigning `archived = orderId` makes a second handle to the same data (for objects/arrays it is a shared reference; for primitives like strings it is a copy, but either way the original stays valid).
- Passing a value to a function does **not** invalidate the caller's variable.
- Cleanup timing is **non-deterministic**: the GC runs when it wants.

---

## Rust Equivalent

Rust enforces a stricter discipline. The same `let archived = order_id;` **moves** ownership, and the original binding becomes unusable.

```rust
fn print_receipt(id: String) {
    println!("Receipt: {id}");
} // `id` dropped here, its heap buffer freed

fn main() {
    // ---- Rule 1 & 2: single owner, move on assignment ----
    let order_id = String::from("ORD-2026-0531");
    let archived = order_id; // value MOVES; `order_id` is no longer valid
    println!("Archived order: {archived}");

    // ---- Rule 3: drop at end of scope ----
    {
        let temp = String::from("temporary buffer");
        println!("Inside block: {temp}");
    } // `temp` goes out of scope here -> its heap buffer is freed

    // ---- move into a function ----
    let receipt = String::from("RCPT-9001");
    print_receipt(receipt); // ownership moves into print_receipt
    // println!("{receipt}"); // would not compile: value moved

    // ---- Copy types are duplicated, not moved ----
    let total = 42_u32;
    let copy = total; // u32 is Copy: bitwise copy, both stay valid
    println!("total={total}, copy={copy}");
}
```

**Output:**

```
Archived order: ORD-2026-0531
Inside block: temporary buffer
Receipt: RCPT-9001
total=42, copy=42
```

**Key points:**

- `let archived = order_id;` **moves** ownership; `order_id` can no longer be used.
- Passing `receipt` to `print_receipt` moves it *into* the function.
- `temp` is freed precisely at the closing `}` of its block, deterministically.
- Small stack-only types like `u32` are `Copy`, so `let copy = total;` duplicates instead of moving. (`Copy` vs `Clone` is the topic of [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/).)

---

## Detailed Explanation

The Rust Book states the three rules verbatim:

1. **Each value in Rust has an owner.**
2. **There can only be one owner at a time.**
3. **When the owner goes out of scope, the value is dropped.**

Let's take them one at a time, line by line, and contrast each with what TypeScript/JavaScript does.

### Rule 1 — Each value has an owner

A **value** is a piece of data (a `String`, a `Vec`, a struct, a number). A **variable** (more precisely, a *binding*) is the name that owns it.

```rust
let order_id = String::from("ORD-2026-0531");
```

Here `order_id` is the owner of the heap-allocated string buffer. Ownership is a property the compiler tracks; it is not stored at runtime and costs nothing. There is no header, no reference count, no GC bookkeeping attached to the value. The compiler simply *knows* who the owner is at every point in the program.

In JavaScript there is no analogous concept. A value can be reachable through any number of variables and object fields simultaneously, and "who owns it" is a meaningless question because the GC owns everything.

### Rule 2 — One owner at a time: the move

This is the rule that bites TypeScript/JavaScript developers. Assigning a value to a new binding **transfers** ownership rather than copying or aliasing it:

```rust
let order_id = String::from("ORD-2026-0531");
let archived = order_id; // ownership MOVED into `archived`
```

A `String` is a three-word value on the stack — a pointer to a heap buffer, a length, and a capacity. The move copies those three words into `archived` and then **invalidates** `order_id`. It does *not* copy the heap buffer, and it does *not* create a second valid pointer to the same buffer. If both `order_id` and `archived` were allowed to be used, both would try to free the same buffer when they went out of scope: a classic *double-free* bug. The move rule makes that impossible.

> **Tip:** Read `let archived = order_id;` as "`archived` takes over ownership from `order_id`," not "`archived` is a copy of `order_id`." After this line, `order_id` is gone — not empty, not null, just *not a thing you can name anymore*.

Passing a value to a function is also a move, because the parameter is a new binding:

```rust
let receipt = String::from("RCPT-9001");
print_receipt(receipt); // `receipt` moved into the parameter `id`
```

After this call, `receipt` is no longer usable in `main`. Its ownership now belongs to `print_receipt`, and when that function ends, the value is dropped there.

**Why doesn't this happen with numbers?** Types whose data lives entirely on the stack and is cheap to duplicate — integers, floats, `bool`, `char`, and tuples of them — implement the `Copy` trait. For those, `let copy = total;` makes an independent bitwise copy and leaves the original valid. There is no heap buffer to double-free, so copying is safe. `String`, `Vec<T>`, and most structs are **not** `Copy`, so they move. See [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/) for the full story.

### Rule 3 — Dropped at end of scope

When a binding goes out of scope, Rust automatically runs cleanup for the value it owns. For a `String` or `Vec`, that means freeing the heap buffer; for a file handle, closing the file; for a lock guard, releasing the lock. This is **deterministic** and tied to lexical scope:

```rust
{
    let temp = String::from("temporary buffer");
    println!("Inside block: {temp}");
} // `temp` dropped HERE — exactly at this `}`, every time
```

You can *see* this happen by implementing the `Drop` trait so the value prints when it is cleaned up:

```rust
struct Connection {
    label: &'static str,
}

impl Drop for Connection {
    fn drop(&mut self) {
        println!("Closing connection: {}", self.label);
    }
}

fn main() {
    println!("Start of main");
    let first = Connection { label: "first" };
    let second = Connection { label: "second" };
    println!("Opened {} and {}", first.label, second.label);

    {
        let inner = Connection { label: "inner-block" };
        println!("Inside inner block with {}", inner.label);
    } // `inner` dropped here

    println!("Back in main");
} // `second` then `first` dropped here (reverse declaration order)
```

**Output:**

```
Start of main
Opened first and second
Inside inner block with inner-block
Closing connection: inner-block
Back in main
Closing connection: second
Closing connection: first
```

Two things to notice. First, `inner` is dropped at the inner `}`, *before* `Back in main` prints; cleanup is scope-bound, not deferred to the end of the program. Second, within a scope, values are dropped in **reverse order of declaration** (`second` before `first`), like unwinding a stack. The mechanics of `Drop` and this ordering are explored in depth in [The Drop Trait and RAII](/05-ownership/08-drop-trait/); here the point is simply that **Rule 3 frees memory automatically, at a moment you can predict by reading the code**.

This is the polar opposite of JavaScript. The JS engine's GC frees objects at an unspecified time after they become unreachable, and `FinalizationRegistry` callbacks are explicitly documented as offering *no timing guarantees* and may never run at all. Rust gives you precise, source-visible cleanup with zero runtime tracking.

### What "move" actually invalidates

A natural question: if you give a value away, how do you keep using your data? Three answers, in increasing order of preference:

1. **Get it back.** A function can take ownership and return it (possibly alongside other results):

   ```rust
   fn measure(s: String) -> (String, usize) {
       let len = s.len();
       (s, len) // hand the String back to the caller
   }

   fn main() {
       let s = String::from("ownership");
       let (s, len) = measure(s); // re-bind `s` from the returned tuple
       println!("'{s}' has length {len}");
   }
   ```

   Output: `'ownership' has length 9`

2. **Clone it.** Make an explicit, independent deep copy so each binding owns its own data:

   ```rust
   let original = String::from("config.toml");
   let backup = original.clone(); // explicit, potentially expensive deep copy
   println!("original = {original}");
   println!("backup   = {backup}");
   ```

3. **Borrow it (best).** Lend a reference so no move happens at all; the caller keeps ownership:

   ```rust
   fn char_count(s: &str) -> usize {
       s.chars().count()
   }

   let path = String::from("/etc/hosts");
   let length = char_count(&path); // pass a reference; no move
   println!("'{path}' has {length} chars"); // `path` still owned here
   ```

Borrowing is usually the right tool and is the subject of the [next page](/05-ownership/02-borrowing/). Returning ownership and cloning are covered above so you understand the alternatives the compiler keeps suggesting.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Who frees memory | Garbage collector, at an unspecified time | The compiler, deterministically at scope exit |
| Assignment (`b = a`) | New handle/reference (or value copy for primitives); `a` stays valid | **Move**: ownership transfers; `a` becomes unusable (unless the type is `Copy`) |
| Passing to a function | Caller's variable stays valid | Argument is **moved in**; caller's variable becomes unusable (unless `Copy` or you pass a reference) |
| Number of live references to one value | Unlimited | Exactly one *owner* (plus borrows, governed separately) |
| Cleanup timing | Non-deterministic; `FinalizationRegistry` gives no guarantees | Deterministic, at the closing `}` of the owner's scope |
| Runtime cost | GC pauses, allocation headers, tracing | Zero — ownership is a compile-time-only concept |
| Double-free / use-after-free | Not possible (GC), but memory leaks via retained references are | Prevented at compile time by the move rule |

**The core mental shift:** in JavaScript, *naming a value more times keeps it alive longer*. In Rust, naming a value a second time (by assignment or passing it) usually **consumes the first name**. You move the responsibility of freeing the value, and only one binding can hold that responsibility at a time.

> **Note:** "Move" in Rust is a *compile-time bookkeeping* operation. At runtime, moving a `String` is just copying three machine words (pointer, length, capacity); the heap buffer is untouched. The compiler then refuses to let you use the old binding. So a move is cheap *and* safe — you are not paying for a deep copy, and you cannot accidentally alias the buffer.

---

## Common Pitfalls

### Pitfall 1: Using a value after it was moved by assignment

This is the first error nearly every newcomer hits.

```rust
fn main() {
    let order_id = String::from("ORD-2026-0531");
    let archived = order_id;
    println!("{order_id}"); // does not compile (error[E0382]): use after move
    println!("{archived}");
}
```

The compiler is precise and even suggests the fix:

```
error[E0382]: borrow of moved value: `order_id`
 --> src/main.rs:4:16
  |
2 |     let order_id = String::from("ORD-2026-0531");
  |         -------- move occurs because `order_id` has type `String`, which does not implement the `Copy` trait
3 |     let archived = order_id;
  |                    -------- value moved here
4 |     println!("{order_id}"); // does not compile (error[E0382]): use after move
  |                ^^^^^^^^ value borrowed here after move
  |
  = note: this error originates in the macro `$crate::format_args_nl` which comes from the expansion of the macro `println` (in Nightly builds, run with -Z macro-backtrace for more info)
help: consider cloning the value if the performance cost is acceptable
  |
3 |     let archived = order_id.clone();
  |                            ++++++++
```

**Fix:** clone if you genuinely need two independent owners (`order_id.clone()`), or borrow if you only need to read it. The phrase "does not implement the `Copy` trait" is the compiler telling you *why* this was a move rather than a copy.

### Pitfall 2: Using a value after passing it to a function

Coming from JavaScript, you expect the caller's variable to survive a function call. It does not, if the value was moved in.

```rust
fn print_receipt(id: String) {
    println!("Receipt: {id}");
}

fn main() {
    let receipt = String::from("RCPT-9001");
    print_receipt(receipt);
    println!("Still have: {receipt}"); // does not compile (error[E0382]): moved into function
}
```

Real compiler output (abbreviated; note the targeted hint about the parameter type):

```
error[E0382]: borrow of moved value: `receipt`
 --> src/main.rs:8:28
  |
6 |     let receipt = String::from("RCPT-9001");
  |         ------- move occurs because `receipt` has type `String`, which does not implement the `Copy` trait
7 |     print_receipt(receipt);
  |                   ------- value moved here
8 |     println!("Still have: {receipt}"); // does not compile (error[E0382]): moved into function
  |                            ^^^^^^^ value borrowed here after move
  |
note: consider changing this parameter type in function `print_receipt` to borrow instead if owning the value isn't necessary
 --> src/main.rs:1:22
  |
1 | fn print_receipt(id: String) {
  |    -------------     ^^^^^^ this parameter takes ownership of the value
... (rustc continues with a "help: consider cloning the value" suggestion)
```

**Fix:** if the function only needs to *read* the value, take `&str`/`&String` instead of `String`, so the call borrows rather than moves. (That is exactly what [Borrowing and References](/05-ownership/02-borrowing/) teaches.)

### Pitfall 3: Moving a value inside a loop

A move that is fine once becomes an error when it repeats:

```rust
fn print_banner(text: String) {
    println!("{text}");
}

fn main() {
    let banner = String::from("=== Report ===");
    for _ in 0..3 {
        print_banner(banner); // does not compile (error[E0382]): moved in previous iteration
    }
}
```

The compiler specifically calls out the loop (output abbreviated):

```
error[E0382]: use of moved value: `banner`
 --> src/main.rs:8:22
  |
6 |     let banner = String::from("=== Report ===");
  |         ------ move occurs because `banner` has type `String`, which does not implement the `Copy` trait
7 |     for _ in 0..3 {
  |     ------------- inside of this loop
8 |         print_banner(banner); // does not compile (error[E0382]): moved in previous iteration
  |                      ^^^^^^ value moved here, in previous iteration of loop
... (rustc continues with "help: consider moving the expression out of the loop" and "help: consider cloning the value" suggestions)
```

**Fix:** pass a borrow (`print_banner(&banner)` with the parameter typed `&str`), or `.clone()` inside the loop if each iteration truly needs its own owned copy.

### Pitfall 4: Partial moves out of a struct

Moving one field out of a struct leaves the struct only *partially* valid: you can still read the fields that were not moved, but you can no longer use the struct as a whole.

```rust
#[derive(Debug)]
struct User {
    name: String,
    email: String,
}

fn main() {
    let user = User {
        name: String::from("Grace"),
        email: String::from("grace@example.com"),
    };
    let name = user.name; // moves the `name` field out
    println!("{}", user.email); // this field is still readable
    println!("{:?}", user);     // does not compile (error[E0382]): partial move
    println!("{name}");
}
```

Real output:

```
error[E0382]: borrow of partially moved value: `user`
  --> src/main.rs:14:22
   |
12 |     let name = user.name; // moves the `name` field out
   |                --------- value partially moved here
13 |     println!("{}", user.email); // this field is still readable
14 |     println!("{:?}", user);     // does not compile (error[E0382]): partial move
   |                      ^^^^ value borrowed here after partial move
   |
   = note: partial move occurs because `user.name` has type `String`, which does not implement the `Copy` trait
```

**Fix:** clone the field you want to take (`let name = user.name.clone();`), or borrow it (`let name = &user.name;`), so the struct stays whole.

---

## Best Practices

- **Borrow by default; move only when you mean to give the value away.** If a function just reads or inspects data, take `&T` (or `&str`/`&[T]`). Reserve owned parameters (`String`, `Vec<T>`) for functions that store, consume, or transform the value into something they return. This single habit eliminates most `E0382` errors.

- **Reach for `.clone()` deliberately, not reflexively.** The compiler's "consider cloning" hint is a convenient escape hatch, but a clone is a real allocation and copy. When you find yourself sprinkling `.clone()` to silence the borrow checker, pause and ask whether a borrow would do; that is usually the idiomatic answer.

- **Let scope drive cleanup.** Don't look for a `free`, `dispose`, or `close` to call manually. Structure your code so a value's scope matches its lifetime, and Rule 3 frees it at the right moment. If you need a value gone *early*, that is what `std::mem::drop(value)` is for (covered in [The Drop Trait and RAII](/05-ownership/08-drop-trait/)).

- **Return ownership instead of mutating shared state.** Where JavaScript code might push into a shared array passed by reference, idiomatic Rust often takes ownership of a value, transforms it, and returns the new owner: a clear, linear flow of who owns what.

- **Name the move when reading code.** When you see `let b = a;` or `f(a)`, train yourself to ask "is `a` `Copy`? If not, `a` is gone after this line." This makes ownership errors obvious before the compiler points them out.

---

## Real-World Example

A common production scenario: an upload pipeline that receives raw bytes from a client, validates and parses them, and produces a stored document. Ownership makes the *stages* explicit: the raw bytes have exactly one owner at each step, and once they are parsed, the original buffer is consumed and cannot be accidentally reused.

```rust
/// A raw upload received from a client.
struct Upload {
    filename: String,
    bytes: Vec<u8>,
}

/// A validated, parsed document ready to store.
struct Document {
    filename: String,
    line_count: usize,
}

/// Consumes the `Upload` (takes ownership) and hands back a `Document`.
/// Because `upload` is moved in, the caller can no longer use it afterward —
/// the type system guarantees the raw bytes have exactly one owner per stage.
fn parse(upload: Upload) -> Result<Document, String> {
    let text = String::from_utf8(upload.bytes)
        .map_err(|_| format!("{} is not valid UTF-8", upload.filename))?;
    let line_count = text.lines().count();
    Ok(Document {
        filename: upload.filename, // move the String out of `upload`
        line_count,
    })
} // `text` is dropped here; any unused bytes are freed automatically

fn main() {
    let upload = Upload {
        filename: String::from("notes.txt"),
        bytes: b"first line\nsecond line\nthird line".to_vec(),
    };

    // Ownership of `upload` moves into `parse`; we get a `Document` back.
    match parse(upload) {
        Ok(doc) => println!("Stored {} ({} lines)", doc.filename, doc.line_count),
        Err(e) => eprintln!("Rejected upload: {e}"),
    }
    // `upload` is gone here — referencing it would not compile.
}
```

**Output:**

```
Stored notes.txt (3 lines)
```

**Why this is idiomatic:**

- `parse` takes `upload` by value because it genuinely *consumes* it: once parsed, the raw bytes are no longer meaningful, and the type system enforces that nobody touches the stale upload.
- `upload.filename` is moved (not cloned) into the new `Document`, reusing the existing allocation rather than copying the string.
- When `parse` returns the `Document`, ownership flows to the `match` arm, and the `Document` is dropped at the end of that arm — no manual cleanup, no leak.
- The compiler would reject any attempt to use `upload` after the `parse(upload)` call, catching a whole class of "I accidentally reused the consumed input" bugs that a JavaScript reviewer would have to spot by eye.

---

## Further Reading

### Official Documentation

- [The Rust Book — What Is Ownership?](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html): the canonical statement of the three rules.
- [Rust by Example — Ownership and Moves](https://doc.rust-lang.org/rust-by-example/scope/move.html)
- [`std::mem::drop`](https://doc.rust-lang.org/std/mem/fn.drop.html): drop a value before the end of its scope.

### Related Sections in This Guide

- [Stack vs Heap](/05-ownership/00-stack-heap/): *why* moving a `String` is cheap and *why* it cannot be aliased.
- [Borrowing](/05-ownership/02-borrowing/) — lend access with `&` so you don't have to move or clone.
- [Mutable References](/05-ownership/03-mutable-references/): the one-mutable-XOR-many-shared rule.
- [Move, Copy, and Clone](/05-ownership/06-move-copy-clone/) — when assignment moves vs copies, and what `Copy`/`Clone` mean.
- [Reference Counting (`Rc`/`Arc`)](/05-ownership/07-reference-counting/): when you genuinely need *shared* ownership.
- [The Drop Trait and RAII](/05-ownership/08-drop-trait/) — the mechanics behind Rule 3, drop order, and early drop.
- [Variables and Mutability](/02-basics/00-variables/): immutability by default, the foundation ownership builds on.
- [Function Parameters](/03-functions/01-parameters/) — choosing between owned and borrowed parameters.
- [Data Structures](/06-data-structures/) — how ownership flows through structs and enums.

---

## Exercises

### Exercise 1

**Difficulty:** Easy

**Objective:** Recognize a use-after-move and fix it with a clone.

**Instructions:** The following code does not compile because `username` is moved into `greet` and then used again. Make it compile while still logging the username *and* greeting with it. Do not change the signature of `greet`.

```rust
fn greet(name: String) {
    println!("Welcome, {name}!");
}

fn main() {
    let username = String::from("ada_lovelace");
    greet(username);
    println!("Logging in: {username}"); // does not compile (error[E0382])
}
```

<details>
<summary>Solution</summary>

Clone before moving so each call has its own owned `String`, and log first for clarity:

```rust
fn greet(name: String) {
    println!("Welcome, {name}!");
}

fn main() {
    let username = String::from("ada_lovelace");
    let display = username.clone(); // clone so both names own data
    println!("Logging in: {username}");
    greet(display); // move the clone into greet
}
```

Output:

```
Logging in: ada_lovelace
Welcome, ada_lovelace!
```

> A borrow (`fn greet(name: &str)`) would be even better here, but the exercise fixed the signature, so cloning is the right tool. You'll learn the borrow version in [Borrowing and References](/05-ownership/02-borrowing/).

</details>

### Exercise 2

**Difficulty:** Medium

**Objective:** Use the "give it away, get it back" pattern to keep working with a value after a function consumes it.

**Instructions:** Write a function `append_event` that takes ownership of a `String` log and an event name (`&str`), appends `" <event>"` to the log, and **returns the updated `String`** so the caller can keep using it. Call it twice in `main` to build up a log, then print it. Starter:

```rust
fn main() {
    let mut log = String::from("event:");
    log = append_event(log, "login");
    log = append_event(log, "click");
    println!("{log}"); // should print: event: login click
}

fn append_event(/* ??? */) -> String {
    // TODO
}
```

<details>
<summary>Solution</summary>

```rust
fn main() {
    let mut log = String::from("event:");
    log = append_event(log, "login"); // give it away, get the updated one back
    log = append_event(log, "click");
    println!("{log}");
}

fn append_event(mut log: String, event: &str) -> String {
    log.push(' ');
    log.push_str(event);
    log // return ownership to the caller
}
```

Output:

```
event: login click
```

Note the `mut log: String` parameter: the binding takes ownership *and* is allowed to mutate the value it owns, then hands the same allocation back. No clone, no extra allocation per call.

</details>

### Exercise 3

**Difficulty:** Medium/Hard

**Objective:** Demonstrate that Rule 3 (drop at end of scope) is deterministic and predict the cleanup order.

**Instructions:** Define a `Timer` struct holding a `name: String` and implement `Drop` for it so that dropping prints `[<name>] finished, cleaning up`. In `main`, create a `Timer` named `"handle_request"`, then in an *inner block* create a second `Timer` named `"db_query"` and print `Running query...` inside that block. After the block, print `Sending response...`. Before running it, write down the order in which the two messages will print, then confirm.

<details>
<summary>Solution</summary>

```rust
struct Timer {
    name: String,
}

impl Drop for Timer {
    fn drop(&mut self) {
        println!("[{}] finished, cleaning up", self.name);
    }
}

fn main() {
    println!("Program start");
    let _request = Timer { name: String::from("handle_request") };
    {
        let _db = Timer { name: String::from("db_query") };
        println!("Running query...");
    } // _db dropped here — at the end of the inner block
    println!("Sending response...");
} // _request dropped here — at the end of main
```

Output:

```
Program start
Running query...
[db_query] finished, cleaning up
Sending response...
[handle_request] finished, cleaning up
```

The inner `db_query` timer is cleaned up the instant its block ends, *before* `Sending response...` prints: that is Rule 3 in action. The outer `handle_request` timer survives until the end of `main`. This deterministic, scope-bound cleanup is the foundation of RAII patterns like database transactions and lock guards, explored in [The Drop Trait and RAII](/05-ownership/08-drop-trait/).

> **Note:** The bindings are named with a leading underscore (`_request`, `_db`) so the compiler doesn't warn that they're "unused"; they exist only for their drop side effect, which is a perfectly legitimate use.

</details>
