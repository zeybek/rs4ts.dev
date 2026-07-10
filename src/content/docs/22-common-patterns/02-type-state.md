---
title: "The Type-State Pattern"
description: "Move an object's state into its type so Rust rejects wrong-state calls at compile time, unlike a TypeScript status field plus runtime guard. Zero runtime cost."
---

The **type-state pattern** moves an object's *state* out of its runtime data and into its *type*, so the compiler can reject illegal operations before the program ever runs. Methods only exist on the states where they make sense, turning "this method isn't valid right now" runtime bugs into compile errors.

---

## Quick Overview

In TypeScript you usually track state with a field (`status: "draft" | "published"`) and then *check* that field at runtime before doing anything. The type-state pattern flips this around: each state becomes a distinct type (`Post<Draft>` vs `Post<Published>`), and a method like `.send()` simply **does not exist** on the wrong state. Misusing the object becomes a type error, not a runtime panic. And because the state markers are zero-sized, you pay nothing for the safety at runtime.

This guide checks the pattern with the repository's [pinned verification toolchain](/00-introduction/05-version-policy/) on the 2024 edition. Type-state is one of Rust's clearest ways to encode an invariant in types rather than comments.

---

## TypeScript/JavaScript Example

Here is a typical blog-post workflow in TypeScript. The state lives in a field, and every method that cares about state has to guard against the wrong one:

```typescript
// A post can be a draft or published. State is a runtime field.
class Post {
  private content = "";
  private state: "draft" | "published" = "draft";

  addText(text: string): void {
    if (this.state !== "draft") {
      throw new Error("cannot edit a published post");
    }
    this.content += text;
  }

  publish(): void {
    if (this.state !== "draft") {
      throw new Error("post is already published");
    }
    this.state = "published";
  }

  views(): number {
    if (this.state !== "published") {
      throw new Error("a draft has no views");
    }
    return 42;
  }
}

const post = new Post();
post.addText("Hello, type states!");
post.publish();
console.log(post.views()); // 42

// Nothing stops this from compiling — it throws at RUNTIME:
const draft = new Post();
console.log(draft.views()); // Error: a draft has no views
```

**Key points:**

- The state is *data* (`this.state`), so the type system cannot see it.
- Every method re-checks the state and throws: boilerplate that is easy to forget.
- `draft.views()` type-checks happily; the bug only appears when that line actually runs.
- A reviewer cannot tell from the *type* `Post` whether a given value is safe to publish.

> **Note:** TypeScript can do *better* than this with discriminated unions and "branded" types, and we will compare those at the end. But the field-plus-guard style above is what most production TypeScript actually looks like.

---

## Rust Equivalent

In Rust, the state becomes a generic type parameter. `Post<Draft>` and `Post<Published>` are different types, and each one only has the methods that are valid for it:

```rust playground
use std::marker::PhantomData;

// State marker types. They hold no data and are never instantiated.
struct Draft;
struct Published;

// The post carries its state as the generic parameter `S`.
// `PhantomData<S>` records the state in the type without storing any bytes.
struct Post<S> {
    content: String,
    _state: PhantomData<S>,
}

// Methods available in *any* state.
impl<S> Post<S> {
    fn content(&self) -> &str {
        &self.content
    }
}

// Methods available ONLY on a Draft post.
impl Post<Draft> {
    fn new() -> Post<Draft> {
        Post { content: String::new(), _state: PhantomData }
    }

    fn add_text(&mut self, text: &str) {
        self.content.push_str(text);
    }

    // Consumes the Draft and returns a Published — a *type transition*.
    fn publish(self) -> Post<Published> {
        Post { content: self.content, _state: PhantomData }
    }
}

// Methods available ONLY on a Published post.
impl Post<Published> {
    fn views(&self) -> u32 {
        42
    }
}

fn main() {
    let mut draft = Post::<Draft>::new();
    draft.add_text("Hello, type states!");
    println!("draft content: {}", draft.content());

    let published = draft.publish(); // draft is consumed here
    println!("published content: {}", published.content());
    println!("views: {}", published.views());
}
```

Real output:

```text
draft content: Hello, type states!
published content: Hello, type states!
views: 42
```

The interesting line is the one we *cannot* write. Calling `views()` on a draft is not a runtime error. It does not compile:

```rust
// does not compile (error[E0599]: no method named `views`)
let draft = Post::<Draft>::new();
let _ = draft.views();
```

The real compiler error:

```text
error[E0599]: no method named `views` found for struct `Post<Draft>` in the current scope
  --> src/main.rs:27:19
   |
 6 | struct Post<S> {
   | -------------- method `views` not found for this struct
...
27 |     let _ = draft.views();
   |                   ^^^^^ method not found in `Post<Draft>`
   |
   = note: the method was found for
           - `Post<Published>`
```

---

## Detailed Explanation

Let's walk through the pieces, contrasting each with the TypeScript version.

**`struct Draft;` and `struct Published;` are zero-sized marker types.** They have no fields and you never create a value of them. Their only job is to be *distinct types* the compiler can tell apart. This is closest to TypeScript's `unique symbol` or a branded type — except in Rust the brand participates in method resolution, not just assignability.

**`Post<S>` makes the state a type parameter.** Where TypeScript wrote `state: "draft" | "published"` as a field, Rust writes `<S>` as part of the type. `Post<Draft>` and `Post<Published>` are now as different to the compiler as `Vec<String>` and `Vec<i32>`.

**`PhantomData<S>` is the bridge.** A struct must "use" each of its type parameters, but `S` here is only a label: there is no actual `Draft` value to store. `std::marker::PhantomData<S>` is a zero-sized standard-library type that tells the compiler "pretend this struct contains an `S`" without occupying any memory. Without it, `struct Post<S> { content: String }` would fail to compile because `S` is unused. (We confirm the zero-size claim below.)

**`impl<S> Post<S>` defines shared methods.** Anything in this block — like `content()` — is available no matter what state you are in. This is the equivalent of a method with no state guard.

**`impl Post<Draft>` and `impl Post<Published>` define state-specific methods.** This is the heart of the pattern. `add_text` and `publish` live on `Post<Draft>`; `views` lives on `Post<Published>`. Because Rust resolves methods against the *concrete* type, `views` is simply absent from a draft. There is no guard, no `throw`, no runtime cost; the method just is not there.

**`fn publish(self) -> Post<Published>` is the transition.** Note `self` by value (not `&self` or `&mut self`). Taking `self` **consumes** the draft: the old `Post<Draft>` is moved into `publish` and a brand-new `Post<Published>` comes out. This guarantees you cannot keep using the draft after publishing it: the compiler enforces a strict, linear lifecycle. Compare this to the TypeScript `publish()`, which mutates a field in place and leaves the now-stale `Post` object lying around for someone to misuse.

This consume-and-return shape is why type-state pairs so naturally with Rust's [ownership and move semantics](/05-ownership/): a state transition is literally a value moving from one type to another.

---

## Key Differences

| Aspect | TypeScript (field + guard) | Rust (type-state) |
| --- | --- | --- |
| Where state lives | A runtime field (`this.state`) | A type parameter (`Post<Draft>`) |
| When misuse is caught | At runtime, when the line executes | At **compile time**, before running |
| Wrong-state method call | Type-checks, then throws | Does not compile (no such method) |
| Cost of a state check | An `if` on every call | Zero — the check is the type system |
| Stale-object reuse | Possible (object still exists) | Prevented (transition consumes `self`) |
| Runtime memory overhead | A discriminant field (string/number) | None: markers are zero-sized |
| Visible in the signature? | No — `Post` hides its state | Yes — `Post<Published>` is self-documenting |

**Why Rust leans on this.** Rust has no exceptions and discourages "this should never happen" panics. The compiler is the cheapest, earliest place to catch a mistake, so idiomatic Rust pushes invariants into types whenever the set of states is small and known. The payoff is "make illegal states unrepresentable": if your function takes a `Post<Published>`, you *know* it was published; you do not have to defensively check.

**Zero cost is real, not a slogan.** The marker types and `PhantomData` compile to nothing. We can prove it with `std::mem::size_of`:

```rust playground
use std::marker::PhantomData;

struct Draft;
struct Published;

struct Post<S> {
    content: String,
    _state: PhantomData<S>,
}

fn main() {
    println!("size_of String          = {}", std::mem::size_of::<String>());
    println!("size_of Post<Draft>      = {}", std::mem::size_of::<Post<Draft>>());
    println!("size_of Post<Published>  = {}", std::mem::size_of::<Post<Published>>());
    println!("size_of Draft marker     = {}", std::mem::size_of::<Draft>());
    println!("size_of PhantomData      = {}", std::mem::size_of::<PhantomData<Draft>>());
}
```

Real output:

```text
size_of String          = 24
size_of Post<Draft>      = 24
size_of Post<Published>  = 24
size_of Draft marker     = 0
size_of PhantomData      = 0
```

`Post<Draft>` is exactly the size of its real data (the `String`); the state label costs nothing. Contrast this with the TypeScript version, where every object carries a `state` string for its entire lifetime.

---

## Common Pitfalls

### 1. Forgetting `PhantomData` (unused type parameter)

If you write the struct without `PhantomData`, the compiler rejects it because `S` is never used:

```rust
// does not compile (error[E0392]: type parameter `S` is never used)
struct Post<S> {
    content: String,
}
```

The real error tells you exactly what to do:

```text
error[E0392]: type parameter `S` is never used
 --> src/main.rs:1:13
  |
1 | struct Post<S> {
  |             ^ unused type parameter
  |
  = help: consider removing `S`, referring to it in a field, or using a marker such as `PhantomData`
  = help: if you intended `S` to be a const parameter, use `const S: /* Type */` instead
```

The fix is the `_state: PhantomData<S>` field shown earlier. The leading underscore in the field name silences the "field never read" lint, since you never read a `PhantomData`.

### 2. Expecting the transition method to leave the old value usable

Because `publish(self)` takes `self` by value, the original draft is *moved*. Trying to use it again is a borrow-checker error, not a logic bug:

```rust
// does not compile (error[E0382]: use of moved value)
let draft = Post::<Draft>::new();
let _published = draft.publish();
let _again = draft.publish(); // draft was already consumed
```

The real error:

```text
error[E0382]: use of moved value: `draft`
  --> src/main.rs:24:18
   |
21 |     let draft = Post::<Draft>::new();
   |         ----- move occurs because `draft` has type `Post<Draft>`, which does not implement the `Copy` trait
22 |     let _published = draft.publish(); // moves `draft`
   |                            --------- `draft` moved due to this method call
...
24 |     let _again = draft.publish();
   |                  ^^^^^ value used here after move
```

This is the pattern *working as intended*, but it surprises TypeScript developers, who are used to objects living on after a mutating call. Reassign the result (`let post = post.transition();`, often shadowing the same name) instead of reusing the old binding.

### 3. Reaching for `dyn`/trait objects to "store either state"

You cannot put `Post<Draft>` and `Post<Published>` in the same `Vec` without erasing their distinguishing types, which defeats the pattern. If you genuinely need a runtime-variable state (for example, a value whose state is decided by user input at runtime), type-state is the wrong tool; use a plain `enum` instead. Type-state shines when the transitions are known *statically* in the code path. See the strategy and command patterns ([The Strategy Pattern in Rust](/22-common-patterns/05-strategy-pattern/), [The Command Pattern in Rust](/22-common-patterns/07-command-pattern/)) for the runtime-dispatch alternatives.

### 4. Over-applying it

Three states with a clear linear flow? Great fit. A dozen states with arbitrary transitions, or states chosen at runtime? The type explosion and `impl` duplication will hurt more than the safety helps. Reach for an `enum` state machine there.

---

## Best Practices

- **Use unit structs as markers** (`struct Draft;`), not enums or empty modules. They are the lightest possible label and are conventional.
- **Make transitions consume `self`** so stale states cannot linger. Return the new typed value.
- **Put shared methods in `impl<S> Post<S>`** and state-specific methods in `impl Post<ConcreteState>`. Keep the split obvious.
- **Name the marker field `_state`** (leading underscore) to document intent and silence dead-code lints.
- **Seal your states with a private trait** when you want to *guarantee* no downstream crate invents a new state. Bound the type parameter on a `State` trait whose supertrait lives in a private module:

  ```rust playground
  use std::marker::PhantomData;

  mod sealed {
      pub trait Sealed {}
  }

  // Public marker trait, but only types that impl the private `Sealed`
  // (all of which live in this crate) can satisfy it.
  trait State: sealed::Sealed {}

  struct Locked;
  struct Unlocked;

  impl sealed::Sealed for Locked {}
  impl sealed::Sealed for Unlocked {}
  impl State for Locked {}
  impl State for Unlocked {}

  struct Door<S: State> {
      _state: PhantomData<S>,
  }

  impl Door<Locked> {
      fn new() -> Self {
          Door { _state: PhantomData }
      }
      fn unlock(self) -> Door<Unlocked> {
          println!("click — unlocked");
          Door { _state: PhantomData }
      }
  }

  impl Door<Unlocked> {
      fn lock(self) -> Door<Locked> {
          println!("clunk — locked");
          Door { _state: PhantomData }
      }
      fn open(&self) {
          println!("the door swings open");
      }
  }

  fn main() {
      let door = Door::<Locked>::new();
      let door = door.unlock();
      door.open();
      let _door = door.lock();
  }
  ```

  Real output:

  ```text
  click — unlocked
  the door swings open
  clunk — locked
  ```

  An outside attempt to add its own state by writing `struct Rogue;` and `impl State for Rogue {}` is rejected, because `Rogue` cannot implement the private `Sealed` supertrait:

  ```text
  error[E0277]: the trait bound `Rogue: Sealed` is not satisfied
    --> src/main.rs:42:16
     |
  42 | impl State for Rogue {}
     |                ^^^^^ the trait `Sealed` is not implemented for `Rogue`
     |
     = help: the following other types implement trait `Sealed`:
               Locked
               Unlocked
  note: required by a bound in `State`
    --> src/main.rs:7:14
     |
   7 | trait State: sealed::Sealed {}
     |              ^^^^^^^^^^^^^^ required by this bound in `State`
  ```

  This sealed-trait technique is the same one the standard library uses to keep traits like `std::error::Error`'s relatives closed; it pairs naturally with type-state.

---

## Real-World Example

A very common real use is a **typed request builder** that refuses to `send()` until both the HTTP method and the URL are set, caught at compile time, with the call order left flexible. Here we track two independent bits of state with two type parameters:

```rust playground
use std::marker::PhantomData;

// Two markers, reused for both "slots".
struct Unset;
struct Set;

struct RequestBuilder<M, U> {
    method: Option<String>,
    url: Option<String>,
    headers: Vec<(String, String)>,
    _method: PhantomData<M>,
    _url: PhantomData<U>,
}

impl RequestBuilder<Unset, Unset> {
    fn new() -> Self {
        RequestBuilder {
            method: None,
            url: None,
            headers: Vec::new(),
            _method: PhantomData,
            _url: PhantomData,
        }
    }
}

// Setting the method flips only `M` to `Set`, preserving whatever `U` was.
impl<U> RequestBuilder<Unset, U> {
    fn method(self, m: &str) -> RequestBuilder<Set, U> {
        RequestBuilder {
            method: Some(m.to_string()),
            url: self.url,
            headers: self.headers,
            _method: PhantomData,
            _url: PhantomData,
        }
    }
}

// Setting the url flips only `U` to `Set`, preserving whatever `M` was.
impl<M> RequestBuilder<M, Unset> {
    fn url(self, u: &str) -> RequestBuilder<M, Set> {
        RequestBuilder {
            method: self.method,
            url: Some(u.to_string()),
            headers: self.headers,
            _method: PhantomData,
            _url: PhantomData,
        }
    }
}

// `header` works in any state and leaves the state markers unchanged.
impl<M, U> RequestBuilder<M, U> {
    fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.to_string(), v.to_string()));
        self
    }
}

// `send` exists ONLY when both markers are `Set`.
impl RequestBuilder<Set, Set> {
    fn send(self) -> String {
        format!(
            "{} {} ({} header(s))",
            self.method.unwrap(),
            self.url.unwrap(),
            self.headers.len()
        )
    }
}

fn main() {
    let response = RequestBuilder::new()
        .method("GET")
        .header("Accept", "application/json")
        .url("https://example.com/api")
        .send();
    println!("{response}");

    // Order does not matter — url first, then method, also compiles:
    let response2 = RequestBuilder::new()
        .url("https://example.com/users")
        .method("POST")
        .send();
    println!("{response2}");
}
```

Real output:

```text
GET https://example.com/api (1 header(s))
POST https://example.com/users (0 header(s))
```

Forgetting a required slot is a compile error. Calling `.send()` with only the method set yields:

```rust
// does not compile (error[E0599]: no method named `send`)
let _ = RequestBuilder::new()
    .method("GET")
    .send(); // url is still Unset
```

The real error names the exact incomplete state:

```text
error[E0599]: no method named `send` found for struct `RequestBuilder<Set, Unset>` in the current scope
  --> src/main.rs:33:10
   |
 6 |   struct RequestBuilder<M, U> {
   |   --------------------------- method `send` not found for this struct
...
33 | |         .send();
   | |         -^^^^ method not found in `RequestBuilder<Set, Unset>`
   |
   = note: the method was found for
           - `RequestBuilder<Set, Set>`
```

This is exactly the "compile-checked required fields" idea from the [builder pattern](/22-common-patterns/00-builder-pattern/), taken to its logical conclusion: type-state *is* how a builder enforces required fields. The two patterns are deeply related: the builder chapter covers the everyday ergonomic version, and this chapter covers the type-level machinery underneath it.

> **Tip:** Many production crates use this exact shape. The HTTP client `reqwest`, for instance, returns typed builder states, and embedded HAL crates use type-state to model peripheral pin modes (input vs output) so you cannot read from a pin configured for output. You will meet more such crates in [Section 23: Ecosystem](/23-ecosystem/).

---

## Further Reading

- [Rust API Guidelines — type-state / builders](https://rust-lang.github.io/api-guidelines/) — official conventions for typed builders.
- [`std::marker::PhantomData`](https://doc.rust-lang.org/std/marker/struct.PhantomData.html): the standard-library docs for the marker type.
- [The Embedded Rust Book — Typestate Programming](https://docs.rust-embedded.org/book/static-guarantees/typestate-programming.html) — the canonical real-world write-up.
- [The Rustonomicon — PhantomData](https://doc.rust-lang.org/nomicon/phantom-data.html) — deeper coverage of variance and `PhantomData`.

Related chapters in this guide:

- [Builder Pattern](/22-common-patterns/00-builder-pattern/): the ergonomic everyday version of compile-checked required fields.
- [Newtype Pattern](/22-common-patterns/01-newtype/) — wrapping a single type for safety; a sibling idea to markers.
- [Strategy Pattern](/22-common-patterns/05-strategy-pattern/) and [Command Pattern](/22-common-patterns/07-command-pattern/) — when state must be chosen at runtime, use these instead.
- [Ownership (Section 05)](/05-ownership/): move semantics make consuming transitions possible.
- [Generics & Traits (Section 09)](/09-generics-traits/): the underlying machinery for `<S>` and trait bounds.
- [Basics (Section 02)](/02-basics/): for the type-system fundamentals these patterns build on.

---

## Exercises

### Exercise 1 — A connection lifecycle

**Difficulty:** Beginner

**Objective:** Build a type-state value that can only `send` while open.

**Instructions:** Create a `Connection<S>` with two states, `Closed` and `Open`. A new connection starts `Closed`. Provide `connect(self) -> Connection<Open>`, a `send(&self, msg: &str)` method that exists only when open, and `close(self) -> Connection<Closed>`. Store the address string and print it during transitions. Verify that calling `send` on a closed connection does not compile.

<details>
<summary>Solution</summary>

```rust playground
use std::marker::PhantomData;

struct Closed;
struct Open;

struct Connection<S> {
    addr: String,
    _state: PhantomData<S>,
}

impl Connection<Closed> {
    fn new(addr: &str) -> Connection<Closed> {
        Connection { addr: addr.to_string(), _state: PhantomData }
    }
    fn connect(self) -> Connection<Open> {
        println!("connecting to {}", self.addr);
        Connection { addr: self.addr, _state: PhantomData }
    }
}

impl Connection<Open> {
    fn send(&self, msg: &str) {
        println!("[{}] sending: {msg}", self.addr);
    }
    fn close(self) -> Connection<Closed> {
        println!("closing {}", self.addr);
        Connection { addr: self.addr, _state: PhantomData }
    }
}

fn main() {
    let conn = Connection::<Closed>::new("127.0.0.1:8080");
    let conn = conn.connect();
    conn.send("ping");
    let _conn = conn.close();
    // conn.send("late"); // would not compile: no `send` on Connection<Closed>
}
```

Real output:

```text
connecting to 127.0.0.1:8080
[127.0.0.1:8080] sending: ping
closing 127.0.0.1:8080
```

</details>

### Exercise 2 — A shared method across states

**Difficulty:** Intermediate

**Objective:** Add a method that works in every state by bounding the type parameter on a marker trait carrying an associated constant.

**Instructions:** Model a turnstile with states `Locked` and `Unlocked`. Define a `TurnstileState` trait with an associated `const NAME: &'static str`, implement it for both states, and add a `status(&self) -> &'static str` method available in *every* state that returns `S::NAME`. `Locked::insert_coin` transitions to `Unlocked`; `Unlocked::push` transitions back to `Locked`, carrying along a coin counter.

<details>
<summary>Solution</summary>

```rust playground
use std::marker::PhantomData;

trait TurnstileState {
    const NAME: &'static str;
}

struct Locked;
struct Unlocked;
impl TurnstileState for Locked { const NAME: &'static str = "locked"; }
impl TurnstileState for Unlocked { const NAME: &'static str = "unlocked"; }

struct Turnstile<S: TurnstileState> {
    coins: u32,
    _state: PhantomData<S>,
}

// One method, available in every state via the trait bound.
impl<S: TurnstileState> Turnstile<S> {
    fn status(&self) -> &'static str {
        S::NAME
    }
}

impl Turnstile<Locked> {
    fn new() -> Self {
        Turnstile { coins: 0, _state: PhantomData }
    }
    fn insert_coin(self) -> Turnstile<Unlocked> {
        Turnstile { coins: self.coins + 1, _state: PhantomData }
    }
}

impl Turnstile<Unlocked> {
    fn push(self) -> Turnstile<Locked> {
        println!("clack — you walked through (total coins: {})", self.coins);
        Turnstile { coins: self.coins, _state: PhantomData }
    }
}

fn main() {
    let t = Turnstile::<Locked>::new();
    println!("status: {}", t.status());
    let t = t.insert_coin();
    println!("status: {}", t.status());
    let t = t.push();
    println!("status: {}", t.status());
}
```

Real output:

```text
status: locked
status: unlocked
clack — you walked through (total coins: 1)
status: locked
```

</details>

### Exercise 3 — Two required fields via two type parameters

**Difficulty:** Advanced

**Objective:** Build a config builder that refuses to `build()` until both `host` and `port` are set, in any order.

**Instructions:** Create `ConfigBuilder<H, P>` with markers `Missing` and `Provided`. `new()` starts at `<Missing, Missing>`. `host(self, &str)` flips only `H` to `Provided`; `port(self, u16)` flips only `P`. A `build(self) -> Config` method exists only on `ConfigBuilder<Provided, Provided>`. Confirm that omitting either field fails to compile.

<details>
<summary>Solution</summary>

```rust playground
use std::marker::PhantomData;

struct Missing;
struct Provided;

struct ConfigBuilder<H, P> {
    host: Option<String>,
    port: Option<u16>,
    _h: PhantomData<H>,
    _p: PhantomData<P>,
}

impl ConfigBuilder<Missing, Missing> {
    fn new() -> Self {
        ConfigBuilder { host: None, port: None, _h: PhantomData, _p: PhantomData }
    }
}

impl<P> ConfigBuilder<Missing, P> {
    fn host(self, h: &str) -> ConfigBuilder<Provided, P> {
        ConfigBuilder { host: Some(h.to_string()), port: self.port, _h: PhantomData, _p: PhantomData }
    }
}

impl<H> ConfigBuilder<H, Missing> {
    fn port(self, p: u16) -> ConfigBuilder<H, Provided> {
        ConfigBuilder { host: self.host, port: Some(p), _h: PhantomData, _p: PhantomData }
    }
}

#[derive(Debug)]
struct Config {
    host: String,
    port: u16,
}

impl ConfigBuilder<Provided, Provided> {
    fn build(self) -> Config {
        // Unwraps are safe: the type `<Provided, Provided>` proves both are Some.
        Config { host: self.host.unwrap(), port: self.port.unwrap() }
    }
}

fn main() {
    let cfg = ConfigBuilder::new().host("localhost").port(5432).build();
    println!("{cfg:?}");
    // ConfigBuilder::new().host("localhost").build(); // no `build` on <Provided, Missing>
}
```

Real output:

```text
Config { host: "localhost", port: 5432 }
```

> Note the `.unwrap()` calls in `build` never panic: the type `ConfigBuilder<Provided, Provided>` is a compile-time *proof* that both fields were set, so the `Option`s are guaranteed `Some`.

</details>
