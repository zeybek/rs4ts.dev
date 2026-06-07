---
title: "Control Flow"
sidebar:
  label: "Overview"
description: "Rust control flow is expression-based: if, match, and loop each return a value, replacing the ternary and switch. Plus no truthiness and exhaustive matching."
---

You already write conditionals, loops, and `switch` statements every day in TypeScript and JavaScript. Rust gives you the same building blocks — `if`/`else`, `for`/`while`/`loop`, and the `match` expression — but with one defining shift. Rust control flow is **expression-based**, not statement-based. An `if`, a `match`, and even a `loop` each *evaluate to a value*, so they replace the ternary, the discriminated-union `switch`, and the "declare a `let` outside the loop and mutate it" pattern all at once. Two more rules ripple through everything: conditions must be a real `bool` (there is no truthiness), and `match` is exhaustive (the compiler refuses to let you forget a case).

---

## What You'll Learn

- Treat `if`/`else` and `match` as **expressions** that produce values, replacing the ternary `? :` and `switch`
- Write conditions as explicit `bool` tests. Rust has **no truthiness**, so `if count`, `if name`, and `if 0` are all compile errors
- Iterate over **ranges** (`0..n`, `1..=n`) and collections instead of a C-style `for` counter, which Rust does not have
- Use the dedicated infinite `loop` and return a value from it with `break value`
- Replace `switch` with `match`: exhaustiveness, the `_` catch-all, guards, `|` alternatives, ranges, `@` bindings, and destructuring, with no fall-through
- Reach for the lightweight `if let`, `while let`, and `let ... else` when only one case matters
- Control nested loops with **labeled loops** (`'outer:`) and understand how they differ from JavaScript labels

---

## Topics

| # | Topic | What it covers |
| --- | --- | --- |
| 1 | [Conditionals](/04-control-flow/00-conditionals/) | `if`/`else`; the ternary becomes `if` as an **expression**; no truthiness (conditions are `bool` only); an `if let` teaser |
| 2 | [Loops](/04-control-flow/01-loops/) | `for` over ranges/iterators, `while`, the infinite `loop`; `loop` returning a value via `break`; why there is **no C-style `for`** |
| 3 | [`match`](/04-control-flow/02-match/) | `switch` becomes `match`: exhaustiveness, the `_` arm, guards, `\|` patterns, ranges, the `@` binding, and destructuring |
| 4 | [`if let` / `while let`](/04-control-flow/03-if-let-while-let/) | Concise pattern matching: `if let` / `else`, `while let`, and `let ... else` for early-return guard clauses |
| 5 | [`break` and `continue`](/04-control-flow/04-break-continue/) | `break`/`continue`; `break` carrying a value out of a `loop`; `continue` in `for` and `while` |
| 6 | [Labeled Loops](/04-control-flow/05-labeled-loops/) | Labeled loops (`'outer:`); `break`/`continue` to a label; nested-loop control versus JavaScript labels |

---

## Learning Objectives

After completing this section, a TypeScript/JavaScript developer should be able to:

1. Bind the result of an `if`/`else` or `match` directly to a `let`, and explain why both arms must unify to a single type.
2. Rewrite any truthiness-based condition (`if (x)`, `while (queue.length)`) as an explicit `bool` test (`if x != 0`, `while !queue.is_empty()`).
3. Translate a C-style `for (let i = 0; i < n; i++)` into a range loop, and use `.enumerate()` when an index is genuinely needed.
4. Produce a value from a loop with `loop { ... break value; }` instead of a pre-declared mutable variable.
5. Replace a `switch` with an exhaustive `match`, using `|`, ranges, guards, `@`, and destructuring, and know why `match` never falls through.
6. Choose the lightest pattern-matching tool: `if let`, `while let`, `let ... else`, or a full `match`.
7. Use `break 'label` / `continue 'label` to control nested loops, and recognize when an iterator method is clearer.

---

## Prerequisites

This section assumes you have completed:

- **[Section 03: Functions](/03-functions/)**: the `fn` signature, typed parameters, and especially the **tail expression** (a block's last line, with no semicolon, becomes its value). That expression-as-value rule is exactly what makes `if`, `match`, and `loop` usable as values here.

If the expression-oriented model (`let x = { ...; a + b };`) or the statement-vs-expression distinction feels unfamiliar, revisit [Section 02 — Variables and Mutability](/02-basics/00-variables/) before starting.

> **Note:** A few topics here _preview_ concepts covered fully later. `Option<T>` and the `?` operator are introduced in [Section 08 — Error Handling](/08-error-handling/); the borrow-checker rules behind "you can't mutate a collection while iterating it" belong to [Section 05 — Ownership](/05-ownership/); and the full iterator toolbox (`map`/`filter`/`find`/`take_while`) lives in [Section 07 — Collections](/07-collections/). You do not need those sections first; the links are there for when you want to go deeper.

> **Tip:** The apostrophe in a loop label (`'outer:`) is the same sigil Rust uses for **lifetimes** in [Section 05 — Ownership](/05-ownership/04-lifetimes/). They are unrelated concepts that share punctuation; the compiler always knows which one you mean from context.

---

## Estimated Time

- **Reading:** 2.5-3.5 hours
- **Hands-on Practice & Exercises:** 2.5-3.5 hours
- **Total:** 5-7 hours

A reasonable order is the list order above: conditionals → loops → `match` → `if let`/`while let` → `break`/`continue` → labeled loops. `match` (topic 3) is the conceptual heart of the section — it underpins `if let`, `while let`, and `let ... else` — so do not skip it.

---

## Frequently asked questions

### Does Rust have a ternary operator?

No, because `if` is itself an expression: `let max = if a > b { a } else { b };`. Both branches must produce the same type. This replaces `cond ? a : b` and removes the need for a separate ternary. See [Conditionals](/04-control-flow/00-conditionals/).

### How is `match` different from `switch`?

`match` is exhaustive (the compiler rejects unhandled cases), has no fall-through, returns a value, and can destructure and bind variables. A forgotten case is a compile error, not a silent bug. See [Match](/04-control-flow/02-match/).

### How do I write a C-style `for (i = 0; …)` loop?

You iterate a range instead: `for i in 0..n`. To count down use `(0..n).rev()`, and to get the index while iterating use `.enumerate()`. Rust has no three-clause `for`. See [Loops](/04-control-flow/01-loops/).

