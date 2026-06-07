---
title: "The Command Pattern in Rust"
description: "Reify an action as an enum variant or a Box<dyn Fn> instead of a TypeScript command class, then get undo and redo by inverting each recorded command."
---

In TypeScript the **command pattern** is a classic of object-oriented design: wrap "an action and its arguments" in an object with an `execute()` method (and often an `undo()`), then store those objects in a list to queue, log, or replay them. Rust gives you two idiomatic encodings of the same idea, and they sit at opposite ends of a spectrum. When the set of actions is **closed and known**, you reach for an **enum of commands** that an interpreter `match`es over. When a command is really just "some code to run later," you reach for a **`Box<dyn Fn>`**, a boxed closure. This file walks through both, and through the feature that makes the command pattern earn its keep: **undo/redo**.

---

## Quick Overview

A **command** reifies an action: instead of calling `editor.insert(...)` directly, you create an `Insert { ... }` value that represents "insert this text here" and hand it to an invoker that decides *when* and *whether* to run it. Turning a verb into a noun lets you queue, log, retry, replay, and, most usefully, **undo**, because a recorded command can describe how to reverse itself.

Rust expresses this with two tools a TypeScript developer should learn to choose between:

- **An enum of commands** (`enum Command { Insert { .. }, Delete { .. } }`): the data-oriented form. The variants are a closed set, the invoker interprets them with one `match`, and the values are trivially cloneable, serializable, and inspectable. This is the most idiomatic Rust default.
- **`Box<dyn Fn>` / `Box<dyn FnMut>`** — the closure form. A command is just deferred code; you box it so a heterogeneous collection of closures can share one `Vec`. Closest to passing a bare function in JavaScript.
- **A `Command` trait with `Box<dyn Command>`**: the form that maps one-to-one onto the OO version, used when each command needs both `execute` and `undo` plus its own captured state. This is what production undo stacks usually look like.

> **Note:** This page is about the command pattern specifically. For the dispatch mechanics behind `Box<dyn Trait>` versus generics, see [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/). The closely related sibling patterns are [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/) (an algorithm you swap, not an action you record) and [The Visitor Pattern](/22-common-patterns/04-visitor-pattern/) (the same enum-plus-`match` shape applied to traversal). For grouping commands into reversible units see also the transaction idea in [The Error-Propagation Pattern](/22-common-patterns/03-error-propagation/).

---

## TypeScript/JavaScript Example

A text editor with undo and redo. The textbook approach: a `Command` interface with `execute()` and `undo()`, a concrete class per action, and a `History` that owns two stacks.

```typescript
// TypeScript - the classic OO command pattern with undo/redo
interface Command {
  execute(): void;
  undo(): void;
}

class Editor {
  content = "";
}

class InsertText implements Command {
  constructor(
    private editor: Editor,
    private at: number,
    private text: string,
  ) {}
  execute(): void {
    const c = this.editor.content;
    this.editor.content = c.slice(0, this.at) + this.text + c.slice(this.at);
  }
  undo(): void {
    const c = this.editor.content;
    this.editor.content =
      c.slice(0, this.at) + c.slice(this.at + this.text.length);
  }
}

class History {
  private done: Command[] = [];
  private undone: Command[] = [];

  run(cmd: Command): void {
    cmd.execute();
    this.done.push(cmd);
    this.undone = []; // a fresh action invalidates the redo stack
  }
  undo(): void {
    const cmd = this.done.pop();
    if (cmd) {
      cmd.undo();
      this.undone.push(cmd);
    }
  }
  redo(): void {
    const cmd = this.undone.pop();
    if (cmd) {
      cmd.execute();
      this.done.push(cmd);
    }
  }
}

const editor = new Editor();
const history = new History();
history.run(new InsertText(editor, 0, "hello world"));
console.log(editor.content); // hello world
history.undo();
console.log(JSON.stringify(editor.content)); // ""
history.redo();
console.log(editor.content); // hello world
```

**Output (Node v22):**

```text
hello world
""
hello world
```

Two things to carry into the Rust version. First, every command holds a reference back to the `Editor` it mutates (`this.editor`) — shared mutable aliasing that JavaScript permits freely and Rust's borrow checker does not. Second, each command is a separate `class`, even though `InsertText` is really just a tiny bundle of data (`at`, `text`) plus two functions. Rust's enum form collapses that bundle into a single data type.

---

## Rust Equivalent

Here is the same editor, idiomatic Rust style: commands are an **enum**, the editor is the single owner of its state, and undo is computed by **inverting** a recorded command rather than by giving each command a back-reference.

```rust
// Rust - commands as a plain enum; the editor interprets each variant.
#[derive(Clone, Debug)]
enum Command {
    Insert { at: usize, text: String },
    // Delete carries the removed text so the command can invert itself later.
    Delete { at: usize, removed: String },
}

#[derive(Default)]
struct Editor {
    content: String,
    history: Vec<Command>, // executed commands, for undo
    redo: Vec<Command>,    // undone commands, for redo
}

impl Editor {
    /// Run a command, record it, and clear the redo stack (a new edit
    /// invalidates any "future" the user had undone into).
    fn execute(&mut self, cmd: Command) {
        self.apply(&cmd);
        self.history.push(cmd);
        self.redo.clear();
    }

    /// The interpreter: one `match` arm per command kind.
    fn apply(&mut self, cmd: &Command) {
        match cmd {
            Command::Insert { at, text } => self.content.insert_str(*at, text),
            Command::Delete { at, removed } => {
                self.content.replace_range(*at..*at + removed.len(), "")
            }
        }
    }

    /// The command that reverses `cmd`. Each variant carries enough data
    /// to be inverted with no access to outside state.
    fn inverse(cmd: &Command) -> Command {
        match cmd {
            Command::Insert { at, text } => Command::Delete {
                at: *at,
                removed: text.clone(),
            },
            Command::Delete { at, removed } => Command::Insert {
                at: *at,
                text: removed.clone(),
            },
        }
    }

    fn undo(&mut self) -> bool {
        let Some(cmd) = self.history.pop() else {
            return false;
        };
        self.apply(&Self::inverse(&cmd));
        self.redo.push(cmd);
        true
    }

    fn redo(&mut self) -> bool {
        let Some(cmd) = self.redo.pop() else {
            return false;
        };
        self.apply(&cmd);
        self.history.push(cmd);
        true
    }
}

fn main() {
    let mut editor = Editor::default();
    editor.execute(Command::Insert { at: 0, text: "hello world".into() });
    editor.execute(Command::Delete { at: 5, removed: " world".into() });
    println!("after edits:  {:?}", editor.content);

    editor.undo();
    println!("after undo:   {:?}", editor.content);

    editor.redo();
    println!("after redo:   {:?}", editor.content);

    editor.undo();
    editor.undo();
    println!("fully undone: {:?}", editor.content);
    println!("undo on empty history returns: {}", editor.undo());
}
```

**Real output:**

```text
after edits:  "hello"
after undo:   "hello world"
after redo:   "hello"
fully undone: ""
undo on empty history returns: false
```

The shape is the same as TypeScript (execute, two stacks, undo, redo) but the encoding is different in three ways worth dwelling on.

---

## Detailed Explanation

**Commands are data, not objects.** `Command` is an `enum`, so each variant is a fixed bundle of fields. There is no `InsertText` *class* with methods; the behavior lives in the `apply` interpreter, which `match`es over the variants. This is the same enum-plus-`match` structure as the [visitor pattern](/22-common-patterns/04-visitor-pattern/), and it is the idiomatic Rust answer whenever the set of actions is closed. Because `Command` is a plain data type, it gets `#[derive(Clone, Debug)]` for free, and you could add `Serialize`/`Deserialize` to log a command journal to disk (an event-sourcing log) with no extra code.

**The editor owns its state; commands do not alias it.** In TypeScript every command held `this.editor`. In Rust that would be shared mutable aliasing, which the borrow checker forbids without `Rc<RefCell<...>>`. The idiomatic fix is to flip the relationship: the `Editor` owns the data *and* the history, and `apply(&mut self, &Command)` mutates `self` directly. The command is pure data describing *what* to do; the editor decides *how*.

**Undo is inversion, not a stored `undo()` method.** The OO version paired each command with an `undo()` closure that captured the editor. Rust computes the inverse on demand: `inverse(&Command::Insert { at, text })` is `Command::Delete { at, removed: text }`, and vice versa. The important detail, and a place the analogy bites, is that `Delete` must **capture the removed text at execute time** (`removed: " world"`). A command that only stored `{ at, len }` could not reconstruct the deleted characters once they were gone; the first version of this example panicked for exactly that reason. The general principle: **an undoable command must record enough of the prior state to reverse itself** (a "memento"). Sometimes that is the inverse operation; sometimes it is a snapshot of what changed.

**`let ... else` for the empty-stack case.** `undo` and `redo` use `let Some(cmd) = self.history.pop() else { return false; };`. This is the **let-else** form: bind `cmd` if the pop succeeded, otherwise run the `else` block, which must diverge (here `return false`). It replaces the TypeScript `if (cmd) { ... }` guard with a flat, early-return style. See [Section 06: The Option Enum](/06-data-structures/03-option-enum/) for the combinators behind `Option`.

**Clearing redo on a new action.** Both `execute` (Rust) and `run` (TypeScript) clear the redo stack when a fresh command arrives, because undoing three steps and then typing creates a new branch of history — the old "redo future" is no longer reachable. That logic is pattern-independent; it is the same in both languages.

---

## Key Differences

| Aspect | TypeScript (OO) | Rust (enum form) | Rust (`Box<dyn Fn>` form) |
| --- | --- | --- | --- |
| What a command *is* | a class instance with methods | a value of an `enum` | a boxed closure |
| Behavior lives in | each command's `execute`/`undo` | one `match` interpreter | the closure body |
| Adding a new command kind | new class (open set) | new variant + arm (closed set) | new closure (open set) |
| Adding a new operation over all commands | edit every class | add one function with a `match` | not applicable |
| State to mutate | back-reference (`this.editor`) | the invoker owns it; passed as `&mut` | captured or passed in |
| Inspect / clone / serialize | manual | free via `derive` | not possible (a closure is opaque) |
| Dispatch | dynamic property lookup | none; `match` on a tag | dynamic via vtable |

The deciding question is **open versus closed**. An `enum` makes the set of commands *closed*: the compiler forces every `match` to handle every variant, so adding a `Delete` while forgetting to handle it in `apply` is a compile error, not a runtime surprise. A `Box<dyn Fn>` or a `Box<dyn Command>` keeps the set *open*: any closure or any type implementing the trait is a valid command, which you want when commands are plugins, are constructed from user scripts, or are simply too numerous to enumerate. This is the same trade-off as the [strategy pattern](/22-common-patterns/05-strategy-pattern/), applied to actions instead of algorithms.

### The closure form

When a command carries no data worth naming and you never need to inspect or serialize it, skip the enum entirely. A command is a `Box<dyn FnMut>`:

```rust
// A command is just a boxed closure that mutates the receiver.
type Command = Box<dyn FnMut(&mut Counter)>;

#[derive(Default)]
struct Counter {
    value: i64,
}

struct Invoker {
    queue: Vec<Command>,
}

impl Invoker {
    fn new() -> Self {
        Invoker { queue: Vec::new() }
    }

    // Accept any closure; box it so heterogeneous closures share one Vec.
    fn schedule(&mut self, cmd: impl FnMut(&mut Counter) + 'static) {
        self.queue.push(Box::new(cmd));
    }

    fn run_all(&mut self, target: &mut Counter) {
        for cmd in &mut self.queue {
            cmd(target);
        }
        self.queue.clear();
    }
}

fn main() {
    let mut counter = Counter::default();
    let mut invoker = Invoker::new();

    invoker.schedule(|c| c.value += 10);
    let step = 5; // captured by the closure, like a bound parameter
    invoker.schedule(move |c| c.value -= step);
    invoker.schedule(|c| c.value *= 3);

    invoker.run_all(&mut counter);
    println!("final value: {}", counter.value); // (0 + 10 - 5) * 3
}
```

**Real output:**

```text
final value: 15
```

Note `move |c| c.value -= step`: the closure captures `step` by value, which is exactly how the OO version bound constructor arguments into a command object. The `Box` is mandatory because **every closure has its own unique anonymous type**: two closures with identical signatures are still different types, so they cannot share a `Vec` without being erased behind `dyn`. That fact is the source of the most common closure-command compile error, covered next.

---

## Common Pitfalls

### Pitfall 1: pushing two different closures into a `Vec` without boxing

A TypeScript developer expects `[() => ..., () => ...]` to just work, because all JS functions share one type. In Rust each closure is its own type:

```rust
fn main() {
    let mut commands = Vec::new();
    commands.push(|x: &mut i32| *x += 1);
    commands.push(|x: &mut i32| *x *= 2); // does not compile (error[E0308])
    let mut v = 10;
    for cmd in &commands {
        cmd(&mut v);
    }
    println!("{v}");
}
```

The first `push` pins the `Vec`'s element type to the *first* closure's anonymous type; the second closure is a different type:

```text
error[E0308]: mismatched types
   --> src/main.rs:4:19
    |
  3 |     commands.push(|x: &mut i32| *x += 1);
    |     --------      ---------------------
    |     |             |
    |     |             the expected closure
    |     |             this argument has type `{closure@src/main.rs:3:19: 3:32}`...
    |     ... which causes `commands` to have type `Vec<{closure@src/main.rs:3:19: 3:32}>`
  4 |     commands.push(|x: &mut i32| *x *= 2); // second, distinct closure type
    |              ---- ^^^^^^^^^^^^^^^^^^^^^ expected closure, found a different closure
    |              |
    |              arguments to this method are incorrect
    |
    = note: expected closure `{closure@src/main.rs:3:19: 3:32}`
                found closure `{closure@src/main.rs:4:19: 4:32}`
    = note: no two closures, even if identical, have the same type
    = help: consider boxing your closure and/or using it as a trait object
```

The compiler even spells out the fix in its last line. Declare the element type as a trait object so the closures are erased to a common type:

```rust
fn main() {
    let mut commands: Vec<Box<dyn Fn(&mut i32)>> = Vec::new();
    commands.push(Box::new(|x: &mut i32| *x += 1));
    commands.push(Box::new(|x: &mut i32| *x *= 2));
    let mut v = 10;
    for cmd in &commands {
        cmd(&mut v);
    }
    println!("{v}"); // (10 + 1) * 2 = 22
}
```

> **Tip:** Non-capturing closures with identical signatures used directly in `if`/`match` arms *are* coerced to a shared function-pointer type (`fn(i32) -> i32`), so that narrower case compiles without boxing. But the moment they live in a collection or capture anything, you need `Box<dyn ...>`.

### Pitfall 2: borrowing the history while mutating the same struct

A natural "replay all commands" loop runs straight into the borrow checker, and this trips up TypeScript developers more than anything else in this pattern, because aliasing `this` is free in JS:

```rust
enum Command { Inc, Dec }

struct App {
    value: i64,
    history: Vec<Command>,
}

impl App {
    fn apply(&mut self, cmd: &Command) {
        match cmd {
            Command::Inc => self.value += 1,
            Command::Dec => self.value -= 1,
        }
    }
    fn replay(&mut self) {
        // does not compile (error[E0502])
        for cmd in &self.history {
            self.apply(cmd);
        }
    }
}
```

```text
error[E0502]: cannot borrow `*self` as mutable because it is also borrowed as immutable
  --> src/main.rs:18:13
   |
17 |         for cmd in &self.history {
   |                    -------------
   |                    |
   |                    immutable borrow occurs here
   |                    immutable borrow later used here
18 |             self.apply(cmd);
   |             ^^^^^^^^^^^^^^^ mutable borrow occurs here
```

Iterating `&self.history` holds an immutable borrow of `self` for the whole loop, but `self.apply(...)` needs `&mut self`. The compiler cannot see that `apply` touches only `self.value` and never `self.history`. The cleanest fix is to take the history out by value with `std::mem::take`, leaving an empty `Vec` behind, so the borrow of the commands is no longer a borrow of `self`:

```rust
fn replay(&mut self) {
    // Take the command list out, leaving an empty Vec, so `self` is free
    // to be borrowed mutably while we iterate the owned commands.
    let commands = std::mem::take(&mut self.history);
    for cmd in &commands {
        self.apply(cmd);
    }
    self.history = commands;
}
```

> **Note:** `mem::take` works because `Vec` implements `Default` (an empty vec is cheap to create). This "swap the field out, work on the owned value, swap it back" move is a recurring Rust idiom whenever an interpreter and the data it interprets live in the same struct.

### Pitfall 3: a command that cannot undo itself because it discarded state

The very first draft of the editor stored `Delete { at, len }` and tried to reconstruct the deleted text from the *current* content at undo time. By then the text was gone, and the program panicked with `byte index 11 is out of bounds`. An undoable command must capture whatever the inverse needs **at execute time** — here, the removed substring (`Delete { at, removed: String }`). When the prior state is large or expensive to clone, store a compact diff or a snapshot instead, but never assume you can re-derive it later.

---

## Best Practices

- **Default to an enum** when the set of commands is closed. You get exhaustive `match` checking, free `Clone`/`Debug`/serialization, and the ability to add new operations (validate, log, count) as functions without touching the command definitions.
- **Reach for `Box<dyn Fn>`** when a command is genuinely just deferred code with nothing to inspect — UI callbacks, a job queue, a macro recorder. Prefer `impl Fn(...)` in function *parameters* (static dispatch, no allocation) and box only when you need to store heterogeneous commands together.
- **Use a `Command` trait with `Box<dyn Command>`** when each command needs both `execute` and `undo` plus its own captured state, and the set is open. This is the production undo-stack shape (see the next section).
- **Make undo self-contained.** Either compute the inverse from the command's own fields, or have `execute` return / store a memento of the prior state. Avoid undo that depends on reading current global state.
- **Clear the redo stack on every new command.** Forgetting this lets a user "redo" into a branch of history that the new command made unreachable.
- **Keep the invoker the sole owner of mutable state.** Pass `&mut State` into commands rather than letting each command hold a back-reference; it sidesteps the aliasing problems that `Rc<RefCell<...>>` would otherwise force on you.

---

## Real-World Example

A drawing application's command bus: each command implements an `execute`/`undo` trait and captures the state it needs to reverse itself, the bus owns the document and two stacks of `Box<dyn Command>`, and `AddShape`/`MoveShape` show the two ways a command records undo data (remembering the id it created, and snapshotting the position it overwrote).

```rust
use std::collections::HashMap;

/// Shared application state the commands operate on.
#[derive(Default)]
struct Document {
    shapes: HashMap<u32, (f64, f64)>, // id -> position
    next_id: u32,
}

/// A command knows how to do and undo itself against the document.
trait Command {
    fn execute(&mut self, doc: &mut Document);
    fn undo(&mut self, doc: &mut Document);
    fn label(&self) -> &str;
}

struct AddShape {
    pos: (f64, f64),
    id: Option<u32>, // filled in on first execute, reused on redo
}

impl Command for AddShape {
    fn execute(&mut self, doc: &mut Document) {
        let id = self.id.unwrap_or_else(|| {
            let id = doc.next_id;
            doc.next_id += 1;
            id
        });
        self.id = Some(id);
        doc.shapes.insert(id, self.pos);
    }
    fn undo(&mut self, doc: &mut Document) {
        if let Some(id) = self.id {
            doc.shapes.remove(&id);
        }
    }
    fn label(&self) -> &str {
        "add shape"
    }
}

struct MoveShape {
    id: u32,
    to: (f64, f64),
    from: Option<(f64, f64)>, // captured at execute time so undo can restore it
}

impl Command for MoveShape {
    fn execute(&mut self, doc: &mut Document) {
        if let Some(pos) = doc.shapes.get_mut(&self.id) {
            self.from = Some(*pos);
            *pos = self.to;
        }
    }
    fn undo(&mut self, doc: &mut Document) {
        if let (Some(pos), Some(from)) = (doc.shapes.get_mut(&self.id), self.from) {
            *pos = from;
        }
    }
    fn label(&self) -> &str {
        "move shape"
    }
}

/// The invoker owns the document and the two history stacks of trait objects.
struct CommandBus {
    doc: Document,
    done: Vec<Box<dyn Command>>,
    undone: Vec<Box<dyn Command>>,
}

impl CommandBus {
    fn new() -> Self {
        CommandBus {
            doc: Document::default(),
            done: Vec::new(),
            undone: Vec::new(),
        }
    }

    fn dispatch(&mut self, mut cmd: Box<dyn Command>) {
        cmd.execute(&mut self.doc);
        self.done.push(cmd);
        self.undone.clear();
    }

    fn undo(&mut self) -> Option<String> {
        let mut cmd = self.done.pop()?;
        cmd.undo(&mut self.doc);
        let label = cmd.label().to_string();
        self.undone.push(cmd);
        Some(label)
    }

    fn redo(&mut self) -> Option<String> {
        let mut cmd = self.undone.pop()?;
        cmd.execute(&mut self.doc);
        let label = cmd.label().to_string();
        self.done.push(cmd);
        Some(label)
    }
}

fn main() {
    let mut bus = CommandBus::new();

    bus.dispatch(Box::new(AddShape { pos: (0.0, 0.0), id: None }));
    bus.dispatch(Box::new(MoveShape { id: 0, to: (3.0, 4.0), from: None }));
    println!("after 2 commands: {:?}", bus.doc.shapes);

    println!("undo: {:?}", bus.undo());
    println!("after undo:       {:?}", bus.doc.shapes);

    println!("redo: {:?}", bus.redo());
    println!("after redo:       {:?}", bus.doc.shapes);
}
```

**Real output:**

```text
after 2 commands: {0: (3.0, 4.0)}
undo: Some("move shape")
after undo:       {0: (0.0, 0.0)}
redo: Some("move shape")
after redo:       {0: (3.0, 4.0)}
```

This is the trait-object form, and it earns the extra machinery here: each command carries *different* undo state (`AddShape` remembers the id it generated so redo reuses it; `MoveShape` snapshots the position it overwrote), so a single enum-plus-`match` would not be as clean. The `?` in `self.done.pop()?` propagates the `None` from an empty stack straight out of `undo`, returning `None` to the caller: the command-pattern flavor of the error-propagation idiom in [The Error-Propagation Pattern](/22-common-patterns/03-error-propagation/). Production undo systems (text editors, CAD tools, the `undo` crate on [crates.io](/23-ecosystem/)) generalize exactly this structure, often adding command *merging* (coalescing many keystrokes into one undo step) and grouping (see Exercise 3).

---

## Further Reading

- [The Rust Programming Language — Closures](https://doc.rust-lang.org/book/ch13-01-closures.html) — `Fn`, `FnMut`, `FnOnce` and capturing, the basis of the closure form.
- [The Rust Programming Language — Trait Objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) — `Box<dyn Trait>` and dynamic dispatch.
- [Rust Design Patterns — Command](https://rust-unofficial.github.io/patterns/patterns/behavioural/command.html) — the community pattern catalog's take, including the trait and `Fn` variants.
- [`std::mem::take`](https://doc.rust-lang.org/std/mem/fn.take.html): the field-swap trick used to escape the replay borrow conflict.
- Sibling patterns: [The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/) (swappable algorithm vs. recorded action), [The Visitor Pattern](/22-common-patterns/04-visitor-pattern/) (the same enum-plus-`match` shape), [The Error-Propagation Pattern](/22-common-patterns/03-error-propagation/) (the `?` used above).
- Foundations: [Section 06: Enums](/06-data-structures/02-enums/), [Section 06: The Option Enum](/06-data-structures/03-option-enum/), [Section 09: Trait Objects](/09-generics-traits/06-trait-objects/).
- Next section: [Section 23: The Rust Ecosystem](/23-ecosystem/).

---

## Exercises

### Exercise 1: an undoable calculator

**Difficulty:** Beginner

**Objective:** Build the enum form of the command pattern with a single undo stack.

**Instructions:** Define `enum Op { Add(f64), Mul(f64) }` and a `Calculator` with a running `total` and an `undo_stack`. Implement `apply(&mut self, op: Op)` that pushes the *previous* `total` onto the stack before mutating, and `undo(&mut self) -> bool` that restores the last snapshot (returning `false` when there is nothing to undo). Verify that `Add(5.0)`, `Mul(3.0)`, then two `undo`s walks the total back through `15 → 5 → 0`.

<details>
<summary>Solution</summary>

```rust
#[derive(Clone, Copy, Debug)]
enum Op {
    Add(f64),
    Mul(f64),
}

#[derive(Default)]
struct Calculator {
    total: f64,
    undo_stack: Vec<f64>, // snapshot of `total` before each op
}

impl Calculator {
    fn apply(&mut self, op: Op) {
        self.undo_stack.push(self.total);
        match op {
            Op::Add(n) => self.total += n,
            Op::Mul(n) => self.total *= n,
        }
    }

    fn undo(&mut self) -> bool {
        match self.undo_stack.pop() {
            Some(prev) => {
                self.total = prev;
                true
            }
            None => false,
        }
    }
}

fn main() {
    let mut calc = Calculator::default();
    calc.apply(Op::Add(5.0));
    calc.apply(Op::Mul(3.0));
    println!("total: {}", calc.total); // 15
    calc.undo();
    println!("after undo: {}", calc.total); // 5
    calc.undo();
    println!("after undo: {}", calc.total); // 0
    println!("undo empty: {}", calc.undo()); // false
}
```

**Real output:**

```text
total: 15
after undo: 5
after undo: 0
undo empty: false
```

This stores a full snapshot of the prior total rather than computing an inverse op — simpler here, and immune to floating-point drift that repeated inverse multiplications could introduce.

</details>

### Exercise 2: a macro recorder with `Box<dyn Fn>`

**Difficulty:** Intermediate

**Objective:** Use the closure form to record a sequence of actions once and replay it many times.

**Instructions:** Define a `Turtle { x, y, pen_down }` and a `MacroRecorder` holding a `Vec<Box<dyn Fn(&mut Turtle)>>`. Give the recorder a `record` method that accepts any `Fn(&mut Turtle) + 'static`, boxes it, and returns `&mut Self` so calls can be chained. Add `play(&self, turtle: &mut Turtle)` that runs every recorded step in order. Record a short macro (pen down, move, pen up) and play it onto two different turtles to confirm the recording is reusable.

<details>
<summary>Solution</summary>

```rust
#[derive(Default)]
struct Turtle {
    x: i32,
    y: i32,
    pen_down: bool,
}

struct MacroRecorder {
    steps: Vec<Box<dyn Fn(&mut Turtle)>>,
}

impl MacroRecorder {
    fn new() -> Self {
        MacroRecorder { steps: Vec::new() }
    }

    fn record(&mut self, step: impl Fn(&mut Turtle) + 'static) -> &mut Self {
        self.steps.push(Box::new(step));
        self
    }

    fn play(&self, turtle: &mut Turtle) {
        for step in &self.steps {
            step(turtle);
        }
    }
}

fn main() {
    let mut macro_rec = MacroRecorder::new();
    macro_rec
        .record(|t| t.pen_down = true)
        .record(|t| t.x += 10)
        .record(|t| t.y += 5)
        .record(|t| t.pen_down = false);

    let mut turtle = Turtle::default();
    macro_rec.play(&mut turtle);
    println!("({}, {}) pen_down={}", turtle.x, turtle.y, turtle.pen_down);

    // Replaying onto a second turtle reuses the same recorded macro.
    let mut other = Turtle { x: 100, ..Turtle::default() };
    macro_rec.play(&mut other);
    println!("({}, {}) pen_down={}", other.x, other.y, other.pen_down);
}
```

**Real output:**

```text
(10, 5) pen_down=false
(110, 5) pen_down=false
```

The `Box<dyn Fn>` is required because the four closures are four distinct anonymous types that must share one `Vec` (Pitfall 1). The chained `record` calls returning `&mut Self` are the builder-style fluent API from [The Builder Pattern](/22-common-patterns/00-builder-pattern/).

</details>

### Exercise 3: a composite (transactional) command

**Difficulty:** Advanced

**Objective:** Make a *group* of commands itself a command, so a multi-step transaction undoes atomically.

**Instructions:** Define a `Command` trait with `execute(&mut self, &mut Bank)` and `undo(&mut self, &mut Bank)`. Implement `Deposit` and `Withdraw` over a `Bank { accounts: HashMap<String, i64> }`. Then implement a `Transaction { steps: Vec<Box<dyn Command>> }` that *also* implements `Command`: `execute` runs the steps in order, and `undo` undoes them **in reverse order**. Model a transfer as a `Transaction` of one `Withdraw` plus one `Deposit`, execute it, then undo it, and confirm both balances return to their starting values.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;

#[derive(Default)]
struct Bank {
    accounts: HashMap<String, i64>, // name -> balance in cents
}

trait Command {
    fn execute(&mut self, bank: &mut Bank);
    fn undo(&mut self, bank: &mut Bank);
}

struct Deposit {
    account: String,
    amount: i64,
}

impl Command for Deposit {
    fn execute(&mut self, bank: &mut Bank) {
        *bank.accounts.entry(self.account.clone()).or_insert(0) += self.amount;
    }
    fn undo(&mut self, bank: &mut Bank) {
        *bank.accounts.entry(self.account.clone()).or_insert(0) -= self.amount;
    }
}

struct Withdraw {
    account: String,
    amount: i64,
}

impl Command for Withdraw {
    fn execute(&mut self, bank: &mut Bank) {
        *bank.accounts.entry(self.account.clone()).or_insert(0) -= self.amount;
    }
    fn undo(&mut self, bank: &mut Bank) {
        *bank.accounts.entry(self.account.clone()).or_insert(0) += self.amount;
    }
}

/// A transaction is itself a Command made of sub-commands.
struct Transaction {
    steps: Vec<Box<dyn Command>>,
}

impl Command for Transaction {
    fn execute(&mut self, bank: &mut Bank) {
        for step in &mut self.steps {
            step.execute(bank);
        }
    }
    fn undo(&mut self, bank: &mut Bank) {
        // Undo in reverse order so each step sees the state it produced.
        for step in self.steps.iter_mut().rev() {
            step.undo(bank);
        }
    }
}

fn main() {
    let mut bank = Bank::default();
    bank.accounts.insert("alice".into(), 10_000);
    bank.accounts.insert("bob".into(), 0);

    // A transfer is two sub-commands grouped into one undoable unit.
    let mut transfer = Transaction {
        steps: vec![
            Box::new(Withdraw { account: "alice".into(), amount: 2_500 }),
            Box::new(Deposit { account: "bob".into(), amount: 2_500 }),
        ],
    };

    transfer.execute(&mut bank);
    println!("after transfer: alice={} bob={}", bank.accounts["alice"], bank.accounts["bob"]);

    transfer.undo(&mut bank);
    println!("after undo:     alice={} bob={}", bank.accounts["alice"], bank.accounts["bob"]);
}
```

**Real output:**

```text
after transfer: alice=7500 bob=2500
after undo:     alice=10000 bob=0
```

Because `Transaction` implements the same `Command` trait as its parts, it slots straight into a `CommandBus` like the one in the Real-World Example — a composite command is indistinguishable from a leaf command to the invoker. Undoing in reverse (`iter_mut().rev()`) matters: if a step depended on an earlier step's effect, undoing front-to-back could leave the bank in an inconsistent intermediate state.

</details>
