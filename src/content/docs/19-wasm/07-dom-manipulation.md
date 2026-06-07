---
title: "Manipulating the DOM from Rust with web-sys"
description: "Grab the document, query and create elements, and attach event listeners from Rust with web-sys. querySelector returns Result; casts are checked at runtime."
---

A WebAssembly module compiled from Rust has no `document`, no `querySelector`, and no event system of its own; those all live in JavaScript. The **`web-sys`** crate gives Rust a typed view of every browser API, so you can grab the document, query and create elements, set text and attributes, and attach event listeners *from Rust* while the browser does the actual rendering. This page is the practical, DOM-focused companion to the boundary mechanics in [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/).

---

## Quick Overview

`web-sys` is a single, enormous crate that mirrors the entire Web IDL surface — `Window`, `Document`, `Element`, `HtmlInputElement`, `Event`, and thousands more — as Rust types with methods. Because it is so large, **every API is hidden behind a Cargo feature flag**, and you enable only the types you touch, which keeps compile times and bundle size sane. For a TypeScript developer the mental model is: `web-sys` is the `lib.dom.d.ts` of the Rust world, except the "types" are real, callable wrappers over the JavaScript glue rather than erased declarations. The big adjustments are that DOM calls return `Result`/`Option` instead of throwing or returning `null`, and that the DOM's runtime class hierarchy (`Node` → `Element` → `HtmlInputElement`) becomes an *explicit downcast* (`dyn_into`) in Rust rather than the implicit narrowing TypeScript does with `as`.

---

## TypeScript/JavaScript Example

Here is a small, realistic to-do widget written the way a front-end engineer would: grab elements, listen for the Enter key, append a list item, and toggle items done by clicking them.

```typescript
// todo.ts — runs in the browser's main thread
class TodoApp {
  private items: string[] = [];

  constructor(private root: HTMLElement) {
    const input = document.createElement("input");
    input.placeholder = "What needs doing?";
    input.id = "new-todo";

    const list = document.createElement("ul");
    list.id = "todo-list";

    this.root.append(input, list);

    // Add an item when the user presses Enter.
    input.addEventListener("keydown", (ev: KeyboardEvent) => {
      if (ev.key !== "Enter") return;
      const text = input.value.trim();
      if (!text) return;
      this.items.push(text);

      const li = document.createElement("li");
      li.textContent = text;
      list.append(li);
      input.value = "";
    });

    // Event delegation: one listener on the list toggles the clicked item.
    list.addEventListener("click", (ev: MouseEvent) => {
      const li = (ev.target as HTMLElement).closest("li");
      li?.classList.toggle("done");
    });
  }

  get count(): number {
    return this.items.length;
  }
}

const app = new TodoApp(document.querySelector<HTMLElement>("#root")!);
```

The logic is so routine it is almost invisible to a JavaScript developer. To check the exact runtime behavior of the bookkeeping — trimming, counting, clearing — the same model run under Node v22 prints:

```text
added "buy milk" (now 1 items)
added "write Rust guide" (now 2 items)
[ 'buy milk', 'write Rust guide' ]
cleared
0
```

Four operations matter, because each maps onto a `web-sys` idiom: getting a typed element (`querySelector<HTMLElement>`), creating and appending nodes (`createElement` / `append`), reading a typed property (`input.value`), and registering event listeners (`addEventListener`). We rebuild all four in Rust below.

---

## Rust Equivalent

Create a library crate that builds a `cdylib` (see [Setting Up wasm-pack](/19-wasm/01-wasm-pack/)), then add the boundary crates. The current stable toolchain is Rust 1.96.0 on the 2024 edition, and `cargo new` selects that edition automatically:

```bash
cargo new --lib todo-dom
cd todo-dom
cargo add wasm-bindgen js-sys
cargo add web-sys --features "Window,Document,Element,HtmlElement,HtmlInputElement,Node,NodeList,Event,EventTarget,KeyboardEvent,DomTokenList,console"
```

That last command is the part newcomers miss: **`web-sys` does nothing until you opt into the specific types via features.** The resulting `Cargo.toml`:

```toml
[package]
name = "todo-dom"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
js-sys = "0.3.99"
wasm-bindgen = "0.2.122"
web-sys = { version = "0.3.99", features = [
    "Window", "Document", "Element", "HtmlElement", "HtmlInputElement",
    "Node", "NodeList", "Event", "EventTarget", "KeyboardEvent",
    "DomTokenList", "console",
] }
```

Now the four core operations in `src/lib.rs`. Each is a free function here for clarity; the [Real-World Example](#real-world-example) assembles them into a stateful component.

```rust
use wasm_bindgen::prelude::*;
use web_sys::{HtmlInputElement, window};

// 1. Get the document, query an element, set its text content.
#[wasm_bindgen]
pub fn render_greeting(name: &str) -> Result<(), JsValue> {
    // window() returns Option<Window> — there is no implicit global in Rust.
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

    // query_selector returns Result<Option<Element>, JsValue>:
    // the Result is the "the selector was invalid" channel, the Option is "not found".
    let heading = document
        .query_selector("#greeting")?
        .expect("no #greeting element");

    // textContent is set via a method, and it takes Option<&str> (None clears it).
    heading.set_text_content(Some(&format!("Hello, {name}!")));
    Ok(())
}

// 2. Create an element, set attributes/classes, append it to a parent.
#[wasm_bindgen]
pub fn add_todo(text: &str) -> Result<(), JsValue> {
    let document = window().unwrap().document().unwrap();

    let li = document.create_element("li")?; // Result<Element, JsValue>
    li.set_text_content(Some(text));
    li.set_class_name("todo-item");
    li.set_attribute("data-done", "false")?;

    let list = document
        .get_element_by_id("todo-list")
        .expect("#todo-list missing");
    list.append_child(&li)?; // Node::append_child, returns Result<Node, JsValue>
    Ok(())
}

// 3. Read a typed property — requires downcasting Element to HtmlInputElement.
#[wasm_bindgen]
pub fn read_input_value(selector: &str) -> Result<String, JsValue> {
    let document = window().unwrap().document().unwrap();
    let el = document
        .query_selector(selector)?
        .ok_or_else(|| JsValue::from_str("element not found"))?;

    // .value() lives on HtmlInputElement, not the generic Element — narrow first.
    let input: HtmlInputElement = el
        .dyn_into::<HtmlInputElement>()
        .map_err(|_| JsValue::from_str("not an <input>"))?;

    Ok(input.value())
}

// 4. Register a click listener via a Closure, kept alive with forget().
#[wasm_bindgen]
pub fn wire_up_button() -> Result<(), JsValue> {
    let document = window().unwrap().document().unwrap();
    let button = document
        .get_element_by_id("click-me")
        .expect("#click-me missing");

    let count = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let count_for_cb = count.clone();

    // The closure receives the DOM Event. Shared state goes through Rc<Cell>.
    let on_click = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
        count_for_cb.set(count_for_cb.get() + 1);
        web_sys::console::log_1(&format!("clicked {} times", count_for_cb.get()).into());
    });

    button.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())?;

    // forget() leaks the Closure for the program's lifetime so the listener
    // keeps firing. Store it in a struct instead when you need to remove it.
    on_click.forget();
    Ok(())
}
```

Building against the browser target compiles cleanly:

```text
$ cargo build --target wasm32-unknown-unknown
   Compiling wasm-bindgen v0.2.122
   Compiling js-sys v0.3.99
   Compiling web-sys v0.3.99
   Compiling todo-dom v0.1.0 (/.../todo-dom)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 00s
```

> **Note:** Plain `cargo build` type-checks against your host platform; only `--target wasm32-unknown-unknown` emits a real `.wasm`. In practice you run `wasm-pack build` ([Setting Up wasm-pack](/19-wasm/01-wasm-pack/)), which compiles and runs the `wasm-bindgen` CLI to produce the JavaScript glue in one step.

---

## Detailed Explanation

### Getting the document: no ambient globals

In JavaScript, `document` and `window` are ambient: they are just *there*. In Rust there is no global state you can dereference; you call `web_sys::window()`, which returns `Option<Window>` (`None` when there is no global `window`, e.g. inside a Web Worker), and then `window.document()`, which is also an `Option`. The idiomatic pattern is:

```rust
use web_sys::window;

let document = window()
    .expect("should run in a browser context")
    .document()
    .expect("document should exist");
```

`.expect()` is fine for a hard precondition like "we are in a browser." In production code you would return `Result<_, JsValue>` and propagate with `?`, as the snippets above do, so a missing document becomes a thrown JavaScript exception rather than a panic that aborts the whole module. The distinction between `Option` (covered in [Section 08](/08-error-handling/00-result-option/)) and `Result` is load-bearing here: **`web-sys` uses `Option` for "the thing might not be present" and `Result<_, JsValue>` for "the underlying JavaScript call can throw."**

### Why DOM calls return `Result` and `Option`

`document.querySelector("#x")` in JavaScript returns `Element | null`, and it *throws* a `SyntaxError` if the selector string is malformed. `web-sys` models both outcomes honestly in the type:

```text
query_selector(&self, selectors: &str) -> Result<Option<Element>, JsValue>
```

The `Result` is the throw channel (invalid selector); the inner `Option` is the `null` channel (valid selector, no match). This is the recurring shape across `web-sys`: anything that can throw returns `Result<_, JsValue>`, and anything nullable returns `Option`. A TypeScript developer used to `el!.textContent` (a non-null assertion) writes `el.expect(...)` or `el.ok_or_else(...)?` instead. The compiler will not let you forget the absent case, which is the whole point.

### The DOM class hierarchy and downcasting

The browser's DOM is an inheritance tree: `EventTarget` → `Node` → `Element` → `HTMLElement` → `HTMLInputElement`, and so on. TypeScript reflects this with `lib.dom.d.ts` interfaces and lets you narrow with `as` or type guards — a *compile-time-only* operation that does nothing at runtime. `web-sys` reflects the same tree with concrete Rust types, but a generic `Element` does **not** expose `.value()`; that method only exists on `HtmlInputElement`. To reach it you perform a **checked downcast** with `dyn_into`:

```rust
use wasm_bindgen::JsCast; // brings dyn_into / dyn_ref into scope (re-exported by the prelude)
use web_sys::HtmlInputElement;

let input: HtmlInputElement = element
    .dyn_into::<HtmlInputElement>()       // Result<HtmlInputElement, Element>
    .map_err(|_| JsValue::from_str("not an <input>"))?;
```

This is the single biggest difference from TypeScript narrowing. `element as HTMLInputElement` in TypeScript is erased: if the element is *not* an input, you get `undefined` when you read `.value` and a silent bug. `dyn_into` performs a real runtime `instanceof`-style check and returns `Err(original)` when the cast fails, so a wrong assumption surfaces immediately. Use `dyn_ref::<T>()` when you only need a borrowed `&T` without consuming the value, and `dyn_into::<T>()` when you want to take ownership. The `JsCast` trait that provides them is part of `wasm_bindgen::prelude`, so `use wasm_bindgen::prelude::*;` brings it in.

### Creating, configuring, and inserting nodes

The node-construction APIs map almost one-to-one, just renamed to Rust's `snake_case` and returning `Result`:

| JavaScript | `web-sys` (Rust) |
|---|---|
| `document.createElement("li")` | `document.create_element("li")?` |
| `el.textContent = "x"` | `el.set_text_content(Some("x"))` |
| `el.className = "c"` | `el.set_class_name("c")` |
| `el.setAttribute("k", "v")` | `el.set_attribute("k", "v")?` |
| `el.id = "x"` | `el.set_id("x")` |
| `parent.appendChild(child)` | `parent.append_child(&child)?` |
| `el.remove()` | `el.remove()` |

`append_child` lives on `Node` (every `Element` *is* a `Node`, so the method is available via deref). It borrows the child (`&child`) because the DOM takes a reference into its own tree; ownership of the Rust `Element` handle stays with you. Two convenient shortcuts: `set_text_content(Some(""))` empties an element's children in one call (the fast way to clear a list), and `set_inner_html` exists too, but treat it exactly as you would in JavaScript, as an XSS hazard unless the content is trusted or sanitized.

### Event listeners: closures that must outlive the call

This is where Rust's ownership model meets the DOM head-on. `addEventListener` needs a callable function and the browser keeps calling it *later*, long after your Rust function has returned. A Rust closure cannot be handed to JavaScript directly; you wrap it in a `wasm_bindgen::closure::Closure`, which allocates a JS function that trampolines back into Rust. The catch is lifetime:

```rust
let on_click = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
    // ... handler body ...
});
button.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())?;
```

- `add_event_listener_with_callback` expects a `&js_sys::Function`. `on_click.as_ref()` yields a `&JsValue`, and `.unchecked_ref()` reinterprets it as the `&Function` the API wants. (This is the one place an *unchecked* cast is idiomatic, because we know a `Closure` is a function.)
- The `Closure` is valid only while it is alive in Rust. If it is dropped, the JavaScript function dangles and the next click throws `closure invoked recursively or after being dropped`. You therefore must either **store it** somewhere that outlives the listener, or call **`.forget()`** to leak it for the program's lifetime.
- `Closure::<dyn FnMut(...)>::new` is for repeated calls (events, intervals); `Closure::once` frees itself after a single call.

`.forget()` is the right tool for a listener that lives as long as the page (a global click handler). For a listener you intend to *remove* later, you must keep the `Closure` in a field so you can pass the same function reference to `remove_event_listener_with_callback`. The Real-World Example stores its closure; the deeper rules are in [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/#closures-the-hardest-part-of-the-boundary).

### Mutable shared state without `&mut`

The click handler increments a counter, but the closure is `move` and outlives the function — there is no `&mut` you could capture safely. Single-threaded WASM in the browser means the answer is `Rc<Cell<T>>` (or `Rc<RefCell<T>>` for non-`Copy` data), the single-threaded interior-mutability pattern from [Section 10](/10-smart-pointers/02-refcell-mutex/). `Rc` gives shared ownership so both the closure and the surrounding code hold the value; `Cell`/`RefCell` provide the mutation. You do **not** reach for `Arc`/`Mutex` here: there are no threads to guard against, and the atomics would be wasted overhead.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust + `web-sys` |
|---|---|---|
| Access to DOM types | Ambient (`lib.dom.d.ts`), always available | Behind per-type Cargo features you must enable |
| `document` / `window` | Ambient globals | `web_sys::window()` → `Option<Window>` → `.document()` → `Option<Document>` |
| `querySelector` result | `Element \| null`, throws on bad selector | `Result<Option<Element>, JsValue>` |
| Narrowing a node type | `el as HTMLInputElement` (erased, no-op at runtime) | `el.dyn_into::<HtmlInputElement>()` (real runtime check, returns `Result`) |
| Setting properties | `el.textContent = x` (assignment) | `el.set_text_content(Some(x))` (method, `Option<&str>`) |
| Event handler | Any function; GC'd automatically | `Closure<dyn FnMut(Event)>`; you manage its lifetime |
| Mutable captured state | Plain closure over a variable | `Rc<Cell<T>>` / `Rc<RefCell<T>>` |
| Errors | Exceptions you may ignore | `Result<_, JsValue>` you must handle or `?`-propagate |

The conceptual core is the **inversion of nullability and error handling**. JavaScript lets you write `document.querySelector("#x").textContent = "hi"` and only discover at runtime that `querySelector` returned `null`. Rust forces every "might be absent" and "might throw" into the type, so the same line becomes several explicit steps: more verbose, but the class of "cannot read properties of null" runtime error simply cannot occur. The other pillar is that **the DOM's runtime polymorphism becomes explicit, checked downcasts**, replacing TypeScript's erased `as`.

> **Note:** `web-sys` is auto-generated from the browser's Web IDL, so its coverage is essentially complete and its naming is mechanical: `getElementById` → `get_element_by_id`, `addEventListener` → `add_event_listener_with_callback` (the suffix names the argument variant). When you cannot find a method, search [docs.rs/web-sys](https://docs.rs/web-sys/) for the type and remember the `snake_case` + `with_*` convention.

---

## Common Pitfalls

### Pitfall 1: Forgetting the Cargo feature flag

Every `web-sys` type is gated behind a feature. If you `use web_sys::HtmlInputElement` without enabling the `HtmlInputElement` feature, the type simply does not exist in the crate. This is the most common first error, and the message is initially confusing because it looks like a typo:

```rust
use wasm_bindgen::prelude::*;
use web_sys::{HtmlInputElement, window}; // does not compile if "HtmlInputElement" feature is off

#[wasm_bindgen]
pub fn read_value(selector: &str) -> Result<String, JsValue> {
    let document = window().unwrap().document().unwrap();
    let el = document.query_selector(selector)?.unwrap();
    let input: HtmlInputElement = el.dyn_into().map_err(|_| JsValue::from_str("nope"))?;
    Ok(input.value())
}
```

Real compiler output:

```text
error[E0432]: unresolved import `web_sys::HtmlInputElement`
 --> src/lib.rs:2:15
  |
2 | use web_sys::{HtmlInputElement, window};
  |               ^^^^^^^^^^^^^^^^ no `HtmlInputElement` in the root

For more information about this error, try `rustc --explain E0432`.
```

**Fix:** add the feature — `cargo add web-sys --features HtmlInputElement` — and note that *methods* are gated too. Calling `el.class_list()` needs the `DomTokenList` feature even though `class_list` is a method on `Element`; `el.style()` needs `CssStyleDeclaration`. When a method "doesn't exist," check its return type's feature, not just the receiver's.

### Pitfall 2: Calling a subtype method on a generic `Element`

Because `query_selector` returns the *base* `Element`, calling an `HtmlInputElement` method on it without downcasting fails. This trips up TypeScript developers who are used to `querySelector<HTMLInputElement>(...)` baking the narrow type into the call:

```rust
use wasm_bindgen::prelude::*;
use web_sys::window;

#[wasm_bindgen]
pub fn read_value(selector: &str) -> Result<String, JsValue> {
    let document = window().unwrap().document().unwrap();
    let el = document.query_selector(selector)?.unwrap();
    Ok(el.value()) // does not compile (error[E0599]): value() is not on Element
}
```

Real compiler output:

```text
error[E0599]: no method named `value` found for struct `web_sys::Element` in the current scope
 --> src/lib.rs:9:11
  |
9 |     Ok(el.value())
  |           ^^^^^
  |
help: there is a method `value_of` with a similar name
  |
9 |     Ok(el.value_of())
  |                +++

For more information about this error, try `rustc --explain E0599`.
```

> **Warning:** Do not take the compiler's `value_of` suggestion. `value_of` is the generic `Object.valueOf()` from `js-sys`, *not* an input's `.value`. This is a case where the "did you mean" hint is misleading. The real fix is to downcast: `let input: HtmlInputElement = el.dyn_into().map_err(...)?; input.value()`.

### Pitfall 3: The dropped-closure runtime crash

This one *compiles* and then fails at runtime, which makes it the nastiest. If you create a `Closure`, register it, and let it fall out of scope, the JS function is freed while the DOM still references it:

```rust
// Compiles fine, breaks at the first click.
let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| { /* ... */ });
button.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())?;
// `cb` is dropped here at end of scope; the listener now dangles.
```

There is no E-code because Rust's type system cannot see across the boundary into the DOM's reference. The browser console throws `closure invoked recursively or after being dropped` on the first click. **Fix:** either `cb.forget()` for a page-lifetime listener, or store `cb` in a struct field that outlives the element (as the Real-World Example does). This is the DOM-flavored version of the closure-lifetime rule covered in [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/#closures-the-hardest-part-of-the-boundary).

### Pitfall 4: Treating `query_selector`'s `Option` as the error

`query_selector` returns `Result<Option<Element>, JsValue>`. Newcomers sometimes `?`-propagate and then `.unwrap()` the `Option`, conflating "not found" with "error." A missing element is *not* an exception; it is a `None` you should handle deliberately (return a typed error, create the element, or skip). Use `.ok_or_else(|| JsValue::from_str("..."))?` to turn "not found" into a meaningful thrown error, rather than a bare `.unwrap()` that panics with `called Option::unwrap() on a None value` and aborts the module.

### Pitfall 5: Reaching for `&mut self` in a handler

Trying to capture `&mut something` in an event closure runs into the borrow checker because the closure is `'static` (it outlives the current frame) and may be invoked re-entrantly. The fix is not `unsafe` or a raw pointer — it is `Rc<RefCell<T>>`. Capture an `Rc` clone, and `borrow_mut()` inside the handler. Forgetting and trying to share a plain `&mut` produces a lifetime error pointing at the closure's `'static` requirement; the structural fix is interior mutability ([Section 10](/10-smart-pointers/02-refcell-mutex/)).

---

## Best Practices

- **Enable only the features you use.** Each `web-sys` feature you add increases compile time. Keep the `features = [...]` list tight and let the compiler's "unresolved import" errors tell you what to add, rather than enabling everything.
- **Return `Result<_, JsValue>` from exported DOM functions.** Propagate with `?` so a missing element or a throwing call surfaces as a JavaScript exception the caller can `try/catch`, instead of a panic that aborts the whole WASM instance.
- **Prefer `dyn_into` / `dyn_ref` over `unchecked_into`.** The checked casts catch a wrong assumption at the moment it happens. Reserve `unchecked_ref`/`unchecked_into` for the narrow case where you *know* the type, chiefly handing a `Closure` to `add_event_listener_with_callback`.
- **Store long-lived closures; `forget()` only page-lifetime ones.** If a listener must be removable, keep its `Closure` in a struct field and pair `add_event_listener_with_callback` with `remove_event_listener_with_callback`. Use `.forget()` deliberately and comment *why* you are leaking.
- **Batch DOM writes.** Crossing into the DOM is a boundary call; build a subtree (or an HTML string) and append it once rather than touching the live tree in a tight loop. The boundary economics are in [WebAssembly Performance](/19-wasm/09-performance/).
- **Use `Rc<RefCell<T>>`, never `Arc<Mutex<T>>`, for shared UI state.** Browser WASM is single-threaded; the atomic machinery buys you nothing and costs size and speed.
- **Install `console_error_panic_hook` in debug builds** so a Rust panic shows a readable stack trace in the console instead of `unreachable executed`. See [Your First Rust → WebAssembly Module](/19-wasm/02-first-wasm/).
- **Reach for a framework when the DOM logic grows.** Hand-written `web-sys` is perfect for a focused widget or a fast kernel that touches a few nodes. For a whole reactive UI, the imperative create/append/listen code becomes a maintenance burden. That is what Yew and Leptos solve ([Frontend Frameworks in Rust](/19-wasm/08-yew-leptos/)).

---

## Real-World Example

A self-contained to-do component that owns its state and keeps its event closure alive: the production-flavored version of the TypeScript class at the top. It mounts into `#root`, adds an item on Enter, exposes a `count` getter to JavaScript, and can `clear()` itself. The whole module compiles cleanly against `wasm32-unknown-unknown`.

```rust
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, HtmlInputElement, KeyboardEvent, window};

// Shared, mutable state. Browser WASM is single-threaded, so Rc<RefCell<_>>
// is the right shared-ownership tool — no Arc/Mutex.
#[derive(Default)]
struct State {
    items: Vec<String>,
}

#[wasm_bindgen]
pub struct TodoApp {
    document: Document,
    list: Element,
    state: Rc<RefCell<State>>,
    // The closure must outlive the listener, so the struct owns it.
    _on_keydown: Closure<dyn FnMut(KeyboardEvent)>,
}

#[wasm_bindgen]
impl TodoApp {
    /// Mount the app into `#root`, wiring the input's Enter key to add items.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<TodoApp, JsValue> {
        let document = window()
            .ok_or_else(|| JsValue::from_str("no window"))?
            .document()
            .ok_or_else(|| JsValue::from_str("no document"))?;

        let root = document
            .get_element_by_id("root")
            .ok_or_else(|| JsValue::from_str("missing #root"))?;

        // Build the input and list once.
        let input: HtmlInputElement = document
            .create_element("input")?
            .dyn_into()
            .map_err(|_| JsValue::from_str("not an input"))?;
        input.set_attribute("placeholder", "What needs doing?")?;
        input.set_id("new-todo");

        let list = document.create_element("ul")?;
        list.set_id("todo-list");

        root.append_child(&input)?;
        root.append_child(&list)?;

        let state = Rc::new(RefCell::new(State::default()));

        // The handler captures clones of everything it needs to live on its own.
        let cb_document = document.clone();
        let cb_list = list.clone();
        let cb_state = Rc::clone(&state);
        let cb_input = input.clone();
        let on_keydown = Closure::<dyn FnMut(KeyboardEvent)>::new(move |ev: KeyboardEvent| {
            if ev.key() != "Enter" {
                return;
            }
            let text = cb_input.value();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return;
            }
            cb_state.borrow_mut().items.push(trimmed.to_string());

            // Render one <li> for the new item.
            if let Ok(li) = cb_document.create_element("li") {
                li.set_text_content(Some(trimmed));
                let _ = cb_list.append_child(&li);
            }
            cb_input.set_value("");
        });

        input.add_event_listener_with_callback("keydown", on_keydown.as_ref().unchecked_ref())?;

        Ok(TodoApp {
            document,
            list,
            state,
            _on_keydown: on_keydown,
        })
    }

    /// How many items are stored — exposed to JavaScript as a getter.
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.state.borrow().items.len()
    }

    /// Remove every rendered <li> and clear the backing state.
    pub fn clear(&self) -> Result<(), JsValue> {
        self.state.borrow_mut().items.clear();
        // set_text_content(Some("")) empties a node's children in one call.
        self.list.set_text_content(Some(""));
        let _ = &self.document; // keep the field referenced
        Ok(())
    }
}
```

From TypeScript, the generated module is used like any class. The `.d.ts` that `wasm-pack` emits types `count` as a `number` getter and `clear()` as `() => void`:

```typescript
import init, { TodoApp } from "./pkg/todo_dom.js";

await init(); // load + instantiate the .wasm once (see ./rust-from-js.md)

const app = new TodoApp(); // constructor mounts the UI into #root
// ...user types and presses Enter a few times...
console.log(app.count); // e.g. 3
app.clear();
app.free(); // opaque structs are not GC'd — free when done (see ./wasm-bindgen.md)
```

Three deliberate choices make this idiomatic. The `_on_keydown` field stores the `Closure` so the listener survives the constructor returning — drop it and the next keystroke throws. `Rc<RefCell<State>>` lets the closure and the struct's methods share one mutable list without `&mut` gymnastics, and it is the *single-threaded* shared-state pattern rather than `Arc<Mutex<_>>`. And every fallible DOM call is `?`-propagated into a `Result<_, JsValue>`, so a missing `#root` becomes a clean thrown exception rather than a panic that poisons the WASM instance.

> **Warning:** Because `TodoApp` is a `#[wasm_bindgen]` struct, JavaScript holds an opaque pointer into WASM memory; it is not garbage collected. Call `app.free()` when the component is torn down (or use `wasm-bindgen`'s optional `WeakRef` finalization), or the instance — and its forgotten closure — leak for the page's lifetime. See [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/#pitfall-5-forgetting-that-opaque-structs-are-not-garbage-collected).

---

## Further Reading

- [The `web-sys` crate on docs.rs](https://docs.rs/web-sys/): every browser type, searchable; the place to confirm a method name and its required feature.
- [`web-sys` chapter of the `wasm-bindgen` Guide](https://rustwasm.github.io/docs/wasm-bindgen/web-sys/index.html) — feature flags, the DOM example, and how the bindings are generated.
- [`JsCast` documentation](https://docs.rs/wasm-bindgen/latest/wasm_bindgen/trait.JsCast.html): `dyn_into`, `dyn_ref`, and the unchecked variants.
- [MDN: Document, Element, EventTarget](https://developer.mozilla.org/en-US/docs/Web/API/Document): the JavaScript APIs `web-sys` mirrors one-to-one.
- Section cross-links: [What Is WebAssembly and Why Compile Rust to It?](/19-wasm/00-wasm-intro/) · [Setting Up wasm-pack](/19-wasm/01-wasm-pack/) · [Your First Rust → WebAssembly Module](/19-wasm/02-first-wasm/) · [Calling JavaScript from Rust](/19-wasm/03-js-interop/) · [Calling Rust from JavaScript](/19-wasm/04-rust-from-js/) · [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/) · [Using Web APIs from Rust with web-sys](/19-wasm/06-web-apis/) · [Frontend Frameworks in Rust](/19-wasm/08-yew-leptos/) · [WebAssembly Performance](/19-wasm/09-performance/) · [Deploying WebAssembly Applications](/19-wasm/10-deployment/)
- Foundations: [Section 08 — Result and Option](/08-error-handling/00-result-option/) (why DOM calls return them) · [Section 10 — RefCell & interior mutability](/10-smart-pointers/02-refcell-mutex/) (`Rc<RefCell<T>>` for shared UI state) · [Section 01 — Why Rust?](/01-getting-started/00-why-rust/) · [Section 02 — Types](/02-basics/01-types/) · the lower-level cousin in [Section 20 — Unsafe & FFI](/20-unsafe-ffi/).

---

## Exercises

### Exercise 1: Toggle a CSS class

**Difficulty:** Beginner

**Objective:** Get the document, find an element, and mutate its `classList`, the most common DOM operation there is.

**Instructions:** Write a `#[wasm_bindgen]` function `toggle_panel() -> Result<bool, JsValue>` that finds the element with id `panel`, toggles the class `"open"` on it, and returns whether the class is now present. Enable the right features (`Window`, `Document`, `Element`, `DomTokenList`). Confirm it compiles against `wasm32-unknown-unknown`.

<details><summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;
use web_sys::window;

#[wasm_bindgen]
pub fn toggle_panel() -> Result<bool, JsValue> {
    let document = window().unwrap().document().unwrap();
    let panel = document
        .get_element_by_id("panel")
        .ok_or_else(|| JsValue::from_str("#panel not found"))?;

    // class_list() needs the "DomTokenList" feature; toggle() returns
    // Result<bool, JsValue> — true if the class is now present.
    let now_open = panel.class_list().toggle("open")?;
    Ok(now_open)
}
```

`class_list().toggle("open")` mirrors JavaScript's `el.classList.toggle("open")` exactly, returning the new state. Note `get_element_by_id` returns `Option<Element>` (not a `Result`), because it cannot throw — only be absent — so we convert the `None` into a thrown error with `ok_or_else`. This compiles cleanly against the WASM target.

</details>

### Exercise 2: Build a list in one append

**Difficulty:** Intermediate

**Objective:** Create a subtree off-DOM and attach it with a single boundary touch, contrasting with a chatty per-item loop.

**Instructions:** Write `render_list(items: Vec<String>) -> Result<(), JsValue>` that finds `#list-root`, builds a `<ul>` containing one `<li>` per item (with the item as text content), clears any previous content of `#list-root`, and appends the finished `<ul>`. Why is building the `<ul>` *before* appending it better than appending each `<li>` to a live list? Compile-verify.

<details><summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;
use web_sys::window;

#[wasm_bindgen]
pub fn render_list(items: Vec<String>) -> Result<(), JsValue> {
    let document = window().unwrap().document().unwrap();
    let container = document
        .get_element_by_id("list-root")
        .ok_or_else(|| JsValue::from_str("#list-root not found"))?;

    // Build the whole subtree off the live DOM first.
    let ul = document.create_element("ul")?;
    for item in &items {
        let li = document.create_element("li")?;
        li.set_text_content(Some(item));
        ul.append_child(&li)?;
    }

    // Clear old content, then attach the new subtree in one operation.
    container.set_text_content(Some(""));
    container.append_child(&ul)?;
    Ok(())
}
```

`Vec<String>` crosses the boundary as a `string[]` (`wasm-bindgen` handles the conversion). Building the `<ul>` in a detached node means the browser does layout/reflow work only once, when the finished subtree is inserted, rather than after every individual `<li>` append to a live, rendered list; the same reason JavaScript developers build a `DocumentFragment` or an HTML string before inserting. The function compiles against `wasm32-unknown-unknown`.

</details>

### Exercise 3: Event delegation with one listener

**Difficulty:** Advanced

**Objective:** Attach a single listener to a parent and use `event.target` plus `closest` to handle clicks on dynamically-added children: the idiomatic way to avoid one listener per row.

**Instructions:** Write `install_delegated_handler() -> Result<(), JsValue>` that puts **one** `"click"` listener on `#todo-list`. In the handler, read `event.target`, downcast it to an `Element`, walk up to the nearest ancestor `<li>` with `closest("li")`, and toggle the class `"done"` on it. Keep the listener alive for the page's lifetime. Explain why `closest` is needed rather than checking the target directly, and why the `Closure` is leaked. Compile-verify.

<details><summary>Solution</summary>

```rust
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlElement, window};

#[wasm_bindgen]
pub fn install_delegated_handler() -> Result<(), JsValue> {
    let document = window().unwrap().document().unwrap();
    let list = document
        .get_element_by_id("todo-list")
        .ok_or_else(|| JsValue::from_str("#todo-list not found"))?;

    let handler = Closure::<dyn FnMut(Event)>::new(move |ev: Event| {
        // event.target is Option<EventTarget>; narrow it to an Element.
        let Some(target) = ev.target() else { return };
        let Ok(el) = target.dyn_into::<Element>() else {
            return;
        };
        // closest() walks up to the nearest <li>, exactly like JS delegation.
        if let Ok(Some(li)) = el.closest("li") {
            if let Ok(li) = li.dyn_into::<HtmlElement>() {
                let _ = li.class_list().toggle("done");
            }
        }
    });

    list.add_event_listener_with_callback("click", handler.as_ref().unchecked_ref())?;
    // Page-lifetime listener: forget() leaks it deliberately so it keeps firing.
    handler.forget();
    Ok(())
}
```

`closest("li")` is necessary because the click `target` is whatever specific node the user clicked: it might be a `<span>`, an icon, or text *inside* the `<li>`, not the `<li>` itself. Walking up to the nearest matching ancestor is exactly how event delegation works in JavaScript (`(e.target as HTMLElement).closest("li")`). The `Closure` is `forget()`-ten because this listener should live as long as the page; there is no struct to own it and no intent to remove it, so leaking it for the program's lifetime is the correct, idiomatic choice (the alternative — letting it drop — would make the listener dangle and throw on the first click). The handler needs the `HtmlElement` feature for `class_list().toggle`; this compiles cleanly against `wasm32-unknown-unknown`.

</details>
