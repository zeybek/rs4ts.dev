---
title: "Frontend Frameworks in Rust: Yew and Leptos"
description: "Build UIs entirely in Rust with Yew (React-style hooks and virtual DOM) or Leptos (SolidJS-style signals), compared to React, Svelte, and TypeScript."
---

If you have built UIs with React, Vue, or Svelte, you already know the two big ideas this page is about: a **component model** and a **reactivity system**. Rust has mature, WebAssembly-native frameworks built on both. **Yew** feels like React with a strict compiler; **Leptos** feels like Svelte/SolidJS with fine-grained reactivity. This page gives you a working mental model of each and a tiny, compile-verified example so you can decide which fits your project.

---

## Quick Overview

So far in this section you have called individual Rust functions from JavaScript and poked at the DOM by hand. A **frontend framework** lets you instead write your *entire* UI in Rust: components, state, events, and rendering, all compiled to a single `.wasm` module. For a TypeScript developer the headline is that you keep the architectures you already know — declarative components and reactive state — but the compiler now enforces them, and there is no virtual-DOM library or framework runtime shipped as JavaScript.

The two leading choices in 2026 are **Yew** (a React/Elm-style component framework with a virtual DOM and hooks) and **Leptos** (a SolidJS-style framework built on *fine-grained reactivity*, with no virtual DOM). Both run client-side in the browser; Leptos additionally has first-class server-side rendering and "server functions," which makes it closer to a Next.js-style full-stack framework.

---

## TypeScript/JavaScript Example

Here is the canonical counter as a React function component in TypeScript: `useState`, an event handler, and JSX:

```tsx
// Counter.tsx — React 19 + TypeScript
import { useState } from "react";

export function Counter() {
  const [count, setCount] = useState(0);

  // A new closure is created on every render; React diffs the virtual DOM
  // and patches only the <p> text node that actually changed.
  const increment = () => setCount((c) => c + 1);

  return (
    <div>
      <button onClick={increment}>Add task</button>
      <p>Tasks: {count}</p>
    </div>
  );
}
```

And the same idea in Svelte 5, which uses *runes* (`$state`) for fine-grained reactivity: no virtual DOM, the compiler wires the `<p>` directly to the `count` variable:

```svelte
<!-- Counter.svelte — Svelte 5 -->
<script lang="ts">
  let count = $state(0);
</script>

<div>
  <button onclick={() => (count += 1)}>Add task</button>
  <p>Tasks: {count}</p>
</div>
```

These two snippets are two libraries and, more to the point, the two *philosophies* that Yew and Leptos map onto. Keep them in mind: Yew is the React column, Leptos is the Svelte/Solid column.

---

## Rust Equivalent

### Yew — the React/hooks model

> **Note:** The recorded verification run for this page used **Yew 0.23.0**, **Leptos 0.8.19**, Rust 1.96.0, the 2024 edition, and the `wasm32-unknown-unknown` target. Rust 1.96.0 is the repository's [pinned baseline](/00-introduction/05-version-policy/), not a moving “latest stable” claim. Add the dependency with `cargo add yew --features csr` (`csr` = client-side rendering).

```rust
// src/lib.rs  —  cargo add yew --features csr
use yew::prelude::*;

#[function_component]
fn App() -> Html {
    // use_state is Yew's useState. It returns a handle that derefs to the value.
    let count = use_state(|| 0_i32);

    let onclick = {
        // The handle is not Copy, so clone it for the closure to own.
        let count = count.clone();
        Callback::from(move |_| count.set(*count + 1))
    };

    html! {
        <div>
            <button {onclick}>{ "Add task" }</button>
            <p>{ format!("Tasks: {}", *count) }</p>
        </div>
    }
}

// In a real crate this is your wasm entry point; the bundler calls it.
pub fn run() {
    yew::Renderer::<App>::new().render();
}
```

### Leptos — the SolidJS/signals model

```rust
// src/lib.rs  —  cargo add leptos --features csr
use leptos::prelude::*;

#[component]
fn Counter(#[prop(default = 0)] start: i32) -> impl IntoView {
    // signal() returns a (ReadSignal, WriteSignal) pair — like Solid's createSignal.
    let (count, set_count) = signal(start);

    view! {
        <div>
            <button on:click=move |_| set_count.update(|n| *n += 1)>"Add task"</button>
            <p>"Tasks: " {count}</p>
        </div>
    }
}

pub fn run() {
    // Mount the component tree into <body> for client-side rendering.
    leptos::mount::mount_to_body(|| view! { <Counter start=0 /> });
}
```

Both compile cleanly for `wasm32-unknown-unknown`. You build and serve them with a Wasm-aware bundler, most commonly **Trunk** (`cargo install trunk`, then `trunk serve`), which compiles the crate, runs `wasm-bindgen`, and serves an `index.html` with hot reload. Deployment is covered in [Deploying WebAssembly Applications](/19-wasm/10-deployment/); the underlying `wasm-bindgen` machinery in [wasm-bindgen Deep Dive](/19-wasm/05-wasm-bindgen/).

---

## Detailed Explanation

### Reading the Yew version

- **`use yew::prelude::*;`** pulls in `function_component`, `use_state`, `Callback`, `html!`, and `Html` in one line, the same convenience as a barrel import in TypeScript.
- **`#[function_component]`** turns a plain `fn App() -> Html` into a component. This is the direct analogue of a React function component. There is no separate "props interface" here because `App` takes no props; a component with props takes `&SomeProps` as its single argument (shown in the Real-World Example).
- **`use_state(|| 0_i32)`** is `useState(0)`. The argument is a *closure* that produces the initial value, so the work only runs on first render, like React's lazy `useState(() => expensive())`. It returns a `UseStateHandle<i32>`.
- **`count.clone()` before the closure** is the line every React developer trips over. In JavaScript a closure captures `count` by reference and you never think about it. In Rust the closure must *own* what it captures (`move`), and `UseStateHandle` is a cheap reference-counted handle that is *not* `Copy`, so you clone it. Cloning is O(1): it bumps a refcount, it does not copy your state. (See [Section 05 — Move, Copy, Clone](/05-ownership/06-move-copy-clone/).)
- **`Callback::from(move |_| ...)`** wraps the closure in Yew's event-callback type. `count.set(*count + 1)` reads the current value via the `Deref` to `i32` (`*count`) and schedules a re-render with the new value, exactly `setCount(count + 1)`.
- **`html! { ... }`** is a procedural macro that parses JSX-like syntax *at compile time* into virtual-DOM nodes. Note the differences from JSX: text and expressions are wrapped in braces (`{ "Add task" }`, `{ *count }`), and the shorthand `{onclick}` is attribute punning (like `<button onClick={onclick}>` written `<button {onclick}>`).
- **`yew::Renderer::<App>::new().render()`** mounts the app into `<body>`. This replaced the old `yew::start_app` API several versions ago; `Renderer` is the current entry point.

### Reading the Leptos version

- **`#[component]`** marks `Counter` as a component. Unlike Yew, props are ordinary *function parameters*: `start: i32` is a prop, and `#[prop(default = 0)]` gives it a default value so `<Counter />` is valid. This is far closer to how you think about props in TypeScript than Yew's separate props struct.
- **`signal(start)`** creates a reactive signal and returns a `(ReadSignal<i32>, WriteSignal<i32>)` tuple. (Older Leptos called this `create_signal`; that name is deprecated, use `signal` on 0.7+.) Read with `count.get()`, write with `set_count.set(v)` or `set_count.update(|n| *n += 1)`.
- **`{count}` in the view binds the signal directly.** This is the crux of fine-grained reactivity: Leptos does *not* re-run the whole component when `count` changes. It runs `Counter` exactly once to build the DOM, and the `{count}` expression creates a tiny reactive effect that updates only that one text node when the signal changes. There is no virtual DOM and no diffing. This is the Svelte/Solid model.
- **`on:click=move |_| ...`** attaches a real DOM event listener. The `move` keyword is mandatory: the closure outlives `Counter`'s stack frame (it lives as long as the button does), so it must own its captures. Forget `move` and the compiler stops you — see Common Pitfalls.
- **`mount_to_body`** is the client-side-rendering entry point, taking a closure that returns a view.

### The one-render difference, made concrete

In React/Yew, the component function runs *again* on every state change, and the framework diffs the result. In Solid/Leptos, the component function runs *once*; only the reactive expressions re-run. This is why a React developer reaches for `useMemo`/`useCallback` to avoid re-creating things on every render, while a Leptos developer rarely needs to — there *is* no "every render."

---

## Key Differences

| Concept | React/TypeScript | Yew (Rust) | Leptos (Rust) |
| --- | --- | --- | --- |
| Mental model | Component re-renders + virtual DOM | Component re-renders + virtual DOM | Fine-grained reactivity, no VDOM |
| Component runs… | on every state change | on every state change | **once** (effects re-run, not the fn) |
| Template syntax | JSX (`.tsx`) | `html! { }` macro, braces for exprs | `view! { }` macro, string literals for text |
| State | `useState(0)` | `use_state(\|\| 0)` | `signal(0)` → `(read, write)` |
| Read state | `count` | `*count` (Deref) | `count.get()` or bind `{count}` |
| Write state | `setCount(c => c+1)` | `count.set(...)` | `set_count.update(\|n\| *n += 1)` |
| Props | `interface Props {}` | `#[derive(Properties, PartialEq)] struct` | plain `fn` parameters + `#[prop(..)]` |
| Event handler | `onClick={fn}` | `onclick={Callback::from(..)}` | `on:click=move \|_\| ..` |
| Capture in closure | implicit by-ref | explicit `.clone()` + `move` | explicit `move` |
| SSR / full-stack | Next.js (separate) | community SSR | **built-in** SSR + server functions |
| Runtime shipped as JS | React (~40 KB+) | none (all in Wasm) | none (all in Wasm) |

Two takeaways for a TypeScript developer:

1. **Neither framework ships a JavaScript runtime to the browser.** React itself is JavaScript you download; Yew and Leptos compile their machinery into your `.wasm`. That trades a JS download for a (often larger, but cacheable) Wasm download; the bundle-size tradeoffs are analyzed in [WebAssembly Performance](/19-wasm/09-performance/).
2. **Yew minimizes new concepts; Leptos minimizes runtime work.** If your team thinks in React, Yew's hooks and virtual DOM transfer almost one-to-one. If you want Svelte/Solid-style "set a variable, the DOM updates," and possibly full-stack Rust, Leptos is the closer fit — at the cost of learning signals deeply (the ownership rules around `move` closures bite more often).

> **Tip:** Both projects are production-used and actively maintained. Yew is the more conservative, "it works like React" choice; Leptos is the more ambitious, "one Rust codebase for client and server" choice. Picking is mostly about whether you want SSR/full-stack and which reactivity model your team prefers.

---

## Common Pitfalls

### Pitfall 1 (Yew): using a `use_state` handle after moving it into a closure

A React developer writes the handler inline and reuses `count` in the JSX, expecting it to "just work." In Rust the closure *moves* the handle, so the later use in `html!` is a use-after-move.

```rust
// does not compile (error[E0382]: borrow of moved value: `count`)
use yew::prelude::*;

#[function_component]
fn App() -> Html {
    let count = use_state(|| 0_i32);
    let onclick = Callback::from(move |_| count.set(*count + 1)); // moves `count`
    html! {
        <div>
            <button {onclick}>{ "+1" }</button>
            <p>{ *count }</p>   // `count` was moved into the closure above
        </div>
    }
}

pub fn run() { yew::Renderer::<App>::new().render(); }
```

The real compiler error:

```text
error[E0382]: borrow of moved value: `count`
   --> src/lib.rs:10:19
    |
  5 |     let count = use_state(|| 0_i32);
    |         ----- move occurs because `count` has type `UseStateHandle<i32>`, which does not implement the `Copy` trait
  6 |     let onclick = Callback::from(move |_| count.set(*count + 1));
    |                                  -------- ----- variable moved due to use in closure
    |                                  |
    |                                  value moved into closure here
...
 10 |             <p>{ *count }</p>
    |                   ^^^^^ value borrowed here after move
```

The fix is the `clone()`-into-a-block pattern from the Rust Equivalent: clone the handle just for the closure, leaving the original free to use in the view.

### Pitfall 2 (Yew): a `Properties` struct without `PartialEq`

Props in Yew must derive both `Properties` *and* `PartialEq`. Yew uses `PartialEq` to decide whether a child needs re-rendering (its memoization). Forgetting `PartialEq` produces a confusing "can't compare" error:

```rust
// does not compile (error[E0277]: can't compare `CardProps` with `CardProps`)
use yew::prelude::*;

#[derive(Properties)]      // missing PartialEq
struct CardProps { title: String }

#[function_component]
fn Card(props: &CardProps) -> Html {
    html! { <h2>{ &props.title }</h2> }
}
```

```text
error[E0277]: can't compare `CardProps` with `CardProps`
 --> src/lib.rs:3:10
  |
3 | #[derive(Properties)]
  |          ^^^^^^^^^^ no implementation for `CardProps == CardProps`
  |
  = help: the trait `PartialEq` is not implemented for `CardProps`
```

Fix: `#[derive(Properties, PartialEq)]`.

### Pitfall 3 (Leptos): forgetting `move` on an event closure

Leptos event handlers must be `'static` because they outlive the component's function call. A non-`move` closure borrows the signal, which cannot outlive the stack frame:

```rust
// does not compile (error[E0373]: closure may outlive the current function)
use leptos::prelude::*;

#[component]
fn Counter() -> impl IntoView {
    let (count, set_count) = signal(0);
    view! {
        <button on:click=|_| set_count.update(|n| *n += 1)>"+1"</button> // missing `move`
        <p>{count}</p>
    }
}
```

```text
error[E0373]: closure may outlive the current function, but it borrows `set_count`, which is owned by the current function
 --> src/lib.rs:7:26
  |
7 |         <button on:click=|_| set_count.update(|n| *n += 1)>"+1"</button>
  |                          ^^^ --------- `set_count` is borrowed here
  |                          |
  |                          may outlive borrowed value `set_count`
  |
note: function requires argument type to outlive `'static`
```

Fix: add `move` — `on:click=move |_| set_count.update(|n| *n += 1)`. Because Leptos signals are `Copy` (they are arena handles, not the data), `move` closures can capture the same signal in several handlers without any cloning, a nice ergonomic win over Yew's `use_state` handles.

### Pitfall 4 (Leptos): reading a signal eagerly instead of reactively

A subtle conceptual trap, not a compiler error. Writing `{count.get()}` in a `view!` reads the value *once* at build time and never updates. To stay reactive you bind the signal itself (`{count}`) or wrap the expression in a closure (`{move || count.get() * 2}`) so Leptos can re-run it when the signal changes.

```rust
// Both compile, but only one stays reactive:
// <p>{count.get()}</p>          // read once, never updates  — usually a bug
// <p>{count}</p>                // reactive: updates on every change
// <p>{move || count.get() * 2}</p> // reactive derived value
```

This is the inverse of a React habit: in React you *always* read the current value (`count`) and the re-render handles updates; in Leptos you must hand the framework something it can *re-run*.

---

## Best Practices

- **Pick the model your team already thinks in.** React shop with no SSR need → Yew. Want Svelte/Solid reactivity or full-stack Rust with SSR → Leptos. Do not fight a framework's philosophy.
- **Use Trunk for client-side apps.** `cargo install trunk && trunk serve` gives you compile + `wasm-bindgen` + dev server + hot reload from one tool. It is the de-facto standard for Yew/Leptos CSR; bundler integration is covered in [Deploying WebAssembly Applications](/19-wasm/10-deployment/).
- **Yew: always derive `Properties, PartialEq` together** on props structs, and keep props cheap to compare (Yew compares them to decide whether to re-render).
- **Leptos: prefer `.update()` over `.set()`** when the new value depends on the old, and remember that `signal()` returns *two* handles: keep the read handle for reading/binding and the write handle for mutation.
- **Keep components small and pass data down as props.** The component boundary is also your re-render boundary in Yew and your reactivity boundary in Leptos.
- **Do not reach for a framework for a single widget.** If you are adding one fast function to an existing JS/TS app, plain `wasm-bindgen` ([Your First Rust → WebAssembly Module](/19-wasm/02-first-wasm/)) is lighter than pulling in a whole UI framework. Frameworks pay off when *the UI itself* is Rust.
- **Test logic in plain Rust.** Extract pure logic (reducers, validation, formatting) into ordinary functions you can unit-test with `cargo test` on the host, and keep components thin. (See [Section 13 — Testing](/13-testing/).)

---

## Real-World Example

A small but realistic todo app — text input, add button, click-to-toggle-done, and a live count — written once in each framework. Both compile cleanly for `wasm32-unknown-unknown`.

### Yew

The input handler reaches into `web_sys` to read the DOM element's value, so add the feature: `cargo add web-sys --features HtmlInputElement` (web-sys and feature flags are covered in [Using Web APIs from Rust with web-sys](/19-wasm/06-web-apis/)).

```rust
// src/lib.rs
// cargo add yew --features csr
// cargo add web-sys --features HtmlInputElement
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Clone, PartialEq)]
struct Todo {
    text: String,
    done: bool,
}

#[function_component]
fn App() -> Html {
    let todos = use_state(Vec::<Todo>::new);
    let draft = use_state(String::new);

    // Two-way bind the input field to `draft`.
    let on_input = {
        let draft = draft.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            draft.set(input.value());
        })
    };

    let on_add = {
        let todos = todos.clone();
        let draft = draft.clone();
        Callback::from(move |_| {
            if draft.is_empty() {
                return;
            }
            let mut next = (*todos).clone();
            next.push(Todo { text: (*draft).clone(), done: false });
            todos.set(next);
            draft.set(String::new());
        })
    };

    // A helper that builds a per-item toggle callback.
    let toggle = |idx: usize, todos: UseStateHandle<Vec<Todo>>| {
        Callback::from(move |_| {
            let mut next = (*todos).clone();
            next[idx].done = !next[idx].done;
            todos.set(next);
        })
    };

    html! {
        <section>
            <h1>{ "Todos" }</h1>
            <input value={(*draft).clone()} oninput={on_input} placeholder="What needs doing?" />
            <button onclick={on_add}>{ "Add" }</button>
            <ul>
                { for todos.iter().enumerate().map(|(idx, todo)| {
                    let style = if todo.done { "text-decoration: line-through" } else { "" };
                    html! {
                        <li {style} onclick={toggle(idx, todos.clone())}>
                            { &todo.text }
                        </li>
                    }
                }) }
            </ul>
            <p>{ format!("{} item(s)", todos.len()) }</p>
        </section>
    }
}

pub fn run() {
    yew::Renderer::<App>::new().render();
}
```

Notice the Yew idiom: state is one big `Vec`, and every mutation clones it, edits the clone, and calls `.set()`: the same immutable-update pattern you use with React's `useState` and a setter. The whole `App` re-renders and the virtual DOM diffs the result.

### Leptos

```rust
// src/lib.rs
// cargo add leptos --features csr
use leptos::prelude::*;

#[derive(Clone)]
struct Todo {
    id: usize,
    text: String,
    done: RwSignal<bool>, // each todo owns its own fine-grained signal
}

#[component]
fn App() -> impl IntoView {
    let (todos, set_todos) = signal(Vec::<Todo>::new());
    let (draft, set_draft) = signal(String::new());
    let next_id = RwSignal::new(0usize);

    let add = move |_| {
        let text = draft.get();
        if text.is_empty() {
            return;
        }
        let id = next_id.get();
        next_id.set(id + 1);
        set_todos.update(|list| {
            list.push(Todo { id, text, done: RwSignal::new(false) });
        });
        set_draft.set(String::new());
    };

    // Derived value: recomputed only when its dependencies change.
    let remaining = move || todos.get().iter().filter(|t| !t.done.get()).count();

    view! {
        <section>
            <h1>"Todos"</h1>
            <input
                prop:value=draft
                on:input=move |e| set_draft.set(event_target_value(&e))
                placeholder="What needs doing?"
            />
            <button on:click=add>"Add"</button>
            <ul>
                <For
                    each=move || todos.get()
                    key=|todo| todo.id
                    children=move |todo| {
                        let done = todo.done;
                        view! {
                            <li
                                style=move || if done.get() { "text-decoration: line-through" } else { "" }
                                on:click=move |_| done.update(|d| *d = !*d)
                            >
                                {todo.text.clone()}
                            </li>
                        }
                    }
                />
            </ul>
            <p>{remaining} " remaining"</p>
        </section>
    }
}

pub fn run() {
    leptos::mount::mount_to_body(App);
}
```

The Leptos version shows the fine-grained model paying off. Toggling a single todo flips that todo's own `RwSignal<bool>`; only that one `<li>`'s `style` effect re-runs. The list is rendered with `<For>`, which keys items by `id` and adds/removes only the DOM nodes that actually changed — no full re-render, no virtual-DOM diff. `event_target_value(&e)` is Leptos's helper for reading an input's value, sparing you the manual `web_sys` cast the Yew version needed.

---

## Further Reading

- **Yew documentation**: <https://yew.rs> · API on <https://docs.rs/yew>
- **Leptos book**: <https://book.leptos.dev> · API on <https://docs.rs/leptos>
- **Trunk** (the build/dev tool for both): <https://trunkrs.dev>
- **SolidJS reactivity** (the model Leptos follows) — <https://www.solidjs.com/guides/reactivity>

Related guide sections:

- Previous in this section: [DOM manipulation with web-sys](/19-wasm/07-dom-manipulation/), what these frameworks do for you under the hood.
- The boundary machinery these frameworks build on: [wasm-bindgen in depth](/19-wasm/05-wasm-bindgen/) · [Web APIs with web-sys](/19-wasm/06-web-apis/) · [your first Wasm module](/19-wasm/02-first-wasm/).
- Shipping a framework app: [WebAssembly Performance](/19-wasm/09-performance/) (bundle size) · [Deploying WebAssembly Applications](/19-wasm/10-deployment/) (bundlers, serving `.wasm`).
- Rust foundations these examples lean on: [Section 05 — Move, Copy, Clone](/05-ownership/06-move-copy-clone/) (why closures need `.clone()`/`move`) · [Section 05 — Borrowing](/05-ownership/02-borrowing/) · [Section 13 — Testing](/13-testing/).
- The lower-level cousin of crossing into the browser: [Section 20 — Unsafe & FFI](/20-unsafe-ffi/).

---

## Exercises

### Exercise 1: A bounded counter in Yew

**Difficulty:** Easy

**Objective:** Practice the `use_state` + cloned-handle pattern with multiple event handlers.

**Instructions:** Build a Yew component with three buttons — `-1`, `+1`, and `reset` — and a `<span>` showing the count. The count is a `u32` and must never go below zero. (Hint: `u32` has a `saturating_sub` method, and `UseStateHandle<u32>` derefs to `u32`.)

```rust
use yew::prelude::*;

#[function_component]
fn Counter() -> Html {
    let count = use_state(|| 0_u32);
    // TODO: build inc, dec (saturating), and reset callbacks
    html! {
        <div>
            /* TODO: three buttons + a <span> showing the count */
        </div>
    }
}

pub fn run() { yew::Renderer::<Counter>::new().render(); }
```

<details>
<summary>Solution</summary>

```rust
use yew::prelude::*;

#[function_component]
fn Counter() -> Html {
    let count = use_state(|| 0_u32);

    let inc = {
        let count = count.clone();
        Callback::from(move |_| count.set(*count + 1))
    };
    let dec = {
        let count = count.clone();
        Callback::from(move |_| count.set(count.saturating_sub(1)))
    };
    let reset = {
        let count = count.clone();
        Callback::from(move |_| count.set(0))
    };

    html! {
        <div>
            <button onclick={dec}>{ "-1" }</button>
            <span>{ *count }</span>
            <button onclick={inc}>{ "+1" }</button>
            <button onclick={reset}>{ "reset" }</button>
        </div>
    }
}

pub fn run() {
    yew::Renderer::<Counter>::new().render();
}
```

This compiles cleanly for `wasm32-unknown-unknown`. Each handler clones the handle in its own block so all three can capture it. `count.saturating_sub(1)` calls `u32::saturating_sub` through the handle's `Deref`, returning `0` instead of panicking when the count is already zero.

</details>

### Exercise 2: A reactive temperature converter in Leptos

**Difficulty:** Medium

**Objective:** Use a signal plus a *derived* value so the output updates automatically.

**Instructions:** Build a Leptos component with a numeric input for Celsius and a paragraph showing the equivalent Fahrenheit, formatted to one decimal place. The Fahrenheit value must be a **derived** value (a `move ||` closure), not a second signal you keep in sync by hand. Use `event_target_value(&e)` to read the input and `.parse().unwrap_or(0.0)` to tolerate empty/invalid input.

```rust
use leptos::prelude::*;

#[component]
fn TempConverter() -> impl IntoView {
    let (celsius, set_celsius) = signal(0.0_f64);
    // TODO: a derived `fahrenheit` value
    view! {
        <div>
            /* TODO: a number input bound to celsius, and a <p> showing fahrenheit */
        </div>
    }
}

pub fn run() { leptos::mount::mount_to_body(TempConverter); }
```

<details>
<summary>Solution</summary>

```rust
use leptos::prelude::*;

#[component]
fn TempConverter() -> impl IntoView {
    let (celsius, set_celsius) = signal(0.0_f64);

    // Derived signal: recomputed automatically whenever `celsius` changes.
    let fahrenheit = move || celsius.get() * 9.0 / 5.0 + 32.0;

    view! {
        <div>
            <label>
                "Celsius: "
                <input
                    type="number"
                    prop:value=celsius
                    on:input=move |e| {
                        let v = event_target_value(&e).parse().unwrap_or(0.0);
                        set_celsius.set(v);
                    }
                />
            </label>
            <p>{move || format!("{:.1}", fahrenheit())} " F"</p>
        </div>
    }
}

pub fn run() {
    leptos::mount::mount_to_body(TempConverter);
}
```

This compiles cleanly for `wasm32-unknown-unknown`. `fahrenheit` is a plain closure; Leptos re-runs it inside the `<p>` whenever `celsius` changes, so there is no second piece of state to keep in sync. Wrapping the `format!` in `move ||` keeps the text node reactive (Pitfall 4).

</details>

### Exercise 3: A child component with props (Yew)

**Difficulty:** Hard

**Objective:** Split a UI into a parent and a reusable child component that takes props and a callback, the way you would lift state up in React.

**Instructions:** Write a `TaskItem` child component that takes a `label: String`, a `done: bool`, and an `on_toggle: Callback<()>` prop, rendering an `<li>` (struck through when `done`) that emits `on_toggle` when clicked. Then write a parent `App` holding a `Vec<(String, bool)>` in `use_state` that renders one `TaskItem` per task and toggles the matching entry when a child fires its callback. Remember props need `#[derive(Properties, PartialEq)]`.

<details>
<summary>Solution</summary>

```rust
use yew::prelude::*;

#[derive(Properties, PartialEq)]
struct TaskItemProps {
    label: String,
    done: bool,
    on_toggle: Callback<()>,
}

#[function_component]
fn TaskItem(props: &TaskItemProps) -> Html {
    let style = if props.done { "text-decoration: line-through" } else { "" };
    let on_toggle = props.on_toggle.clone();
    let onclick = Callback::from(move |_| on_toggle.emit(()));
    html! {
        <li {style} {onclick}>{ &props.label }</li>
    }
}

#[function_component]
fn App() -> Html {
    let tasks = use_state(|| vec![
        ("Write Rust".to_string(), true),
        ("Compile to Wasm".to_string(), false),
    ]);

    html! {
        <ul>
            { for tasks.iter().enumerate().map(|(idx, (label, done))| {
                let on_toggle = {
                    let tasks = tasks.clone();
                    Callback::from(move |_| {
                        let mut next = (*tasks).clone();
                        next[idx].1 = !next[idx].1;
                        tasks.set(next);
                    })
                };
                html! {
                    <TaskItem label={label.clone()} done={*done} {on_toggle} />
                }
            }) }
        </ul>
    }
}

pub fn run() {
    yew::Renderer::<App>::new().render();
}
```

This compiles cleanly for `wasm32-unknown-unknown`. The pattern mirrors React precisely: state lives in the parent, the child receives data plus an `on_toggle: Callback<()>` (React's `onToggle: () => void`), and clicking the child calls `on_toggle.emit(())`. Each row builds its own callback in a block that clones the `tasks` handle, captures the row index, and performs the immutable update — the Yew equivalent of an inline `() => setTasks(...)` arrow in JSX.

</details>
