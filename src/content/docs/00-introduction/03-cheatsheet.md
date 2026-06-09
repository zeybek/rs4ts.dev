---
title: "JavaScript & TypeScript to Rust Cheatsheet"
description: "The fast map from JavaScript and TypeScript to Rust: variables, array methods, null handling, promises, classes, and modules, each linked to a full chapter."
---

A single-page lookup for the question you will ask most often while learning Rust: *"what is the Rust equivalent of this JavaScript or TypeScript thing?"* Each row pairs the code you already write on the left with its idiomatic Rust counterpart on the right. The mappings are deliberately terse; every section links to the full chapter where the *why* lives.

If you only skim one page before starting, make it this one, then keep it open in a tab.

---

## Variables and types

| TypeScript / JavaScript | Rust |
| --- | --- |
| `let x = 1` (reassignable) | `let mut x = 1;` |
| `const x = 1` (no reassign) | `let x = 1;` (immutable by default) |
| `const MAX = 100` (true constant) | `const MAX: i32 = 100;` |
| reassign with a new type | shadowing: `let x = 5; let x = "five";` |
| `number` | sized: `i32`, `i64`, `u32`, `usize`, `f64` |
| `bigint` (arbitrary precision) | `i128` / `u128` (fixed 128-bit; for true arbitrary precision use the `num-bigint` crate) |
| `string` | `String` (owned) and `&str` (borrowed) |
| `boolean` | `bool` |
| `null` / `undefined` | `Option<T>` with `None` |
| `any` | no equivalent; reach for generics, an `enum`, or `serde_json::Value` |
| `[1, 2, 3]` | `vec![1, 2, 3]` (a growable `Vec<T>`) |
| fixed-length tuple `[string, number]` | `(String, i32)` |
| `{ id: 1 }` object | a `struct`, or a `HashMap` for dynamic keys |

See: [Variables and Mutability](/02-basics/00-variables/), [Basic Types](/02-basics/01-types/), [Stack vs Heap](/05-ownership/00-stack-heap/).

---

## Functions and closures

| TypeScript / JavaScript | Rust |
| --- | --- |
| `function add(a: number, b: number): number` | `fn add(a: i32, b: i32) -> i32` |
| `return a + b;` | `a + b` (last expression, no `return`, no `;`) |
| `(x) => x + 1` | `\|x\| x + 1` |
| capturing closure `() => count++` | `move \|\| count += 1` (see `Fn`/`FnMut`/`FnOnce`) |
| default parameter `f(a = 1)` | no defaults; take `Option<T>` or use a builder |
| rest parameter `...args` | a slice `args: &[T]` |
| `void` return | `()` (the unit type) |
| pass a function `g(f)` | `fn g(f: impl Fn() -> T)` |

See: [Basic Functions](/03-functions/00-basic-functions/), [Parameters](/03-functions/01-parameters/), [Arrow Functions vs Closures](/03-functions/03-arrow-vs-closures/), [Higher-Order Functions](/03-functions/04-higher-order/).

---

## Control flow

| TypeScript / JavaScript | Rust |
| --- | --- |
| `cond ? a : b` | `if cond { a } else { b }` (an expression) |
| `if (value)` (truthy) | `if value` (must be a real `bool`) |
| `switch (x) { ... }` | `match x { ... }` (exhaustive, no fall-through) |
| `for (let i = 0; i < n; i++)` | `for i in 0..n` |
| `for (const x of arr)` | `for x in &arr` |
| `arr.forEach(f)` | `arr.iter().for_each(f)` or a `for` loop |
| `while (cond)` | `while cond` |
| `while (true) { ... }` | `loop { ... }` (can `break value`) |
| labelled `break outer` | `'outer: loop { break 'outer; }` |

See: [Conditionals](/04-control-flow/00-conditionals/), [Match](/04-control-flow/02-match/), [Loops](/04-control-flow/01-loops/), [if let / while let](/04-control-flow/03-if-let-while-let/).

---

## Null, undefined, and errors

There is no `null` and no `undefined` in Rust. Absence is the `None` variant of `Option<T>`, and a failure is the `Err` variant of `Result<T, E>`. The type system makes you handle both.

| TypeScript / JavaScript | Rust |
| --- | --- |
| `value ?? fallback` | `option.unwrap_or(fallback)` |
| `obj?.prop` (optional chaining) | `option.map(\|o\| o.prop)` / `.and_then(...)` |
| `if (x != null) { use(x) }` | `if let Some(x) = option { use(x) }` |
| `throw new Error("boom")` | `return Err(MyError::Boom)` |
| `try { ... } catch (e) { ... }` | `match result { Ok(v) => ..., Err(e) => ... }` |
| `const v = await f()` (may throw) | `let v = f().await?;` |
| rethrow / propagate | the `?` operator |
| `class HttpError extends Error` | `enum AppError { ... }` with `thiserror` |

See: [Result and Option](/08-error-handling/00-result-option/), [The `?` Operator](/08-error-handling/01-question-mark/), [Option Enum](/06-data-structures/03-option-enum/), [Custom Errors](/08-error-handling/04-custom-errors/).

---

## Collections and array methods

Most array methods exist in Rust, but on **iterators**, and they are **lazy**: nothing runs until a consumer such as `.collect()`, `.sum()`, or a `for` loop pulls the values through.

| TypeScript / JavaScript | Rust |
| --- | --- |
| `arr.map(f)` | `arr.iter().map(f).collect()` |
| `arr.filter(f)` | `arr.iter().filter(f).collect()` |
| `arr.reduce(f, init)` | `arr.iter().fold(init, f)` |
| `arr.find(f)` | `arr.iter().find(f)` (returns `Option`) |
| `arr.some(f)` / `arr.every(f)` | `arr.iter().any(f)` / `.all(f)` |
| `arr.includes(x)` | `arr.contains(&x)` |
| `arr.push(x)` / `arr.length` | `vec.push(x)` / `vec.len()` |
| `arr.slice(a, b)` | `&vec[a..b]` |
| `arr.sort()` | `vec.sort()` |
| `[...a, ...b]` | `a.iter().chain(&b).collect()` |
| `new Map()` / `map.get(k)` | `HashMap::new()` / `map.get(&k)` (returns `Option`) |
| `new Set()` | `HashSet::new()` |
| `Object.keys(o)` / `Object.values(o)` | `map.keys()` / `map.values()` |
| `Array.from({ length: n }, ...)` | `(0..n).map(...).collect()` |

See: [Vectors](/07-collections/00-vectors/), [Iterators](/07-collections/06-iterators/), [Iterator Consumers](/07-collections/07-iterator-consumers/), [HashMaps](/07-collections/03-hashmaps/).

---

## Strings

| TypeScript / JavaScript | Rust |
| --- | --- |
| `"hello " + name` | `format!("hello {name}")` |
| `` `total: ${n}` `` (template) | `format!("total: {n}")` |
| `s.length` (UTF-16 code units — `"🎉".length === 2`) | `s.chars().count()` (Unicode scalars) or `s.len()` (UTF-8 bytes); neither matches JS exactly |
| `s.toUpperCase()` | `s.to_uppercase()` |
| `s.split(",")` | `s.split(',')` |
| `s.includes("x")` | `s.contains("x")` |
| `s.trim()` | `s.trim()` |
| `s.replace(a, b)` | `s.replace(a, b)` |
| `s.startsWith("/")` | `s.starts_with('/')` |
| accept a string argument | take `&str`, return `String` |

See: [Strings](/07-collections/01-strings/), [String Manipulation](/07-collections/02-string-manipulation/).

---

## Structs, enums, classes, and interfaces

Rust has no classes and no inheritance. Data lives in a `struct` or `enum`; behaviour lives in `impl` blocks; shared behaviour is a `trait` (an interface you can implement for any type).

| TypeScript / JavaScript | Rust |
| --- | --- |
| `interface User { id: number }` | `struct User { id: i32 }` |
| `class C { method() {} }` | `struct C; impl C { fn method(&self) {} }` |
| `type Shape = Circle \| Square` | `enum Shape { Circle, Square }` |
| discriminated union with data | `enum` variants carry data |
| `implements Serializable` | `impl Serializable for T` |
| `extends Base` (inheritance) | composition plus traits (no inheritance) |
| `this` | `&self`, `&mut self`, or `self` |
| `new C(args)` | `C::new(args)` (a convention, not a keyword) |
| `instanceof` | `match` on an `enum` or a trait object |
| generic `class Box<T>` | `struct Box<T>` with trait bounds |

See: [Structs](/06-data-structures/00-structs/), [Enums](/06-data-structures/02-enums/), [impl Blocks](/06-data-structures/05-impl-blocks/), [Traits](/09-generics-traits/03-traits/), [Trait Objects](/09-generics-traits/06-trait-objects/).

---

## Async

The keywords match, but Rust futures are **lazy** (they do nothing until `.await`ed) and there is no built-in event loop, so you pick a runtime such as Tokio.

| TypeScript / JavaScript | Rust |
| --- | --- |
| `Promise<T>` | `impl Future<Output = T>` |
| `async function f()` | `async fn f()` |
| `await p` | `p.await` |
| `Promise.all([a, b])` | `tokio::join!(a, b)` / `futures::future::join_all` |
| `Promise.race([a, b])` | `tokio::select!` |
| built-in event loop | a runtime via `#[tokio::main]` |
| `setTimeout(fn, ms)` | `tokio::time::sleep(Duration::from_millis(ms)).await` |
| `for await (const x of stream)` | `while let Some(x) = stream.next().await` |

See: [Promises vs Futures](/11-async/00-promises-vs-futures/), [async/await](/11-async/01-async-await/), [select and join](/11-async/07-select-join/), [Async vs Sync](/11-async/13-async-vs-sync/).

---

## Modules, packages, and tooling

| TypeScript / JavaScript | Rust |
| --- | --- |
| `import { x } from "./m"` | `use crate::m::x;` |
| `export function f()` | `pub fn f()` |
| `export default` | no default export; name the item |
| a file is a module | declare modules with `mod` |
| `package.json` | `Cargo.toml` |
| `npm install serde` | `cargo add serde` |
| `npm run build` | `cargo build --release` |
| `node index.js` | `cargo run` |
| `npm test` | `cargo test` |
| `tsc` (type-check) | `cargo check` |
| ESLint / Prettier | `cargo clippy` / `cargo fmt` |
| `node_modules/` | `~/.cargo/` plus the `target/` build dir |

See: [The Module Tree](/12-modules-packages/01-module-tree/), [The `use` Keyword](/12-modules-packages/02-use-keyword/), [Visibility](/12-modules-packages/03-pub-visibility/), [Cargo](/12-modules-packages/04-cargo/).

---

## Everyday idioms

| TypeScript / JavaScript | Rust |
| --- | --- |
| `console.log(x)` | `println!("{x}")` or `println!("{x:?}")` for any `Debug` type |
| `console.error(x)` | `eprintln!("{x}")` |
| quick debug print | `dbg!(x)` |
| `JSON.stringify(v)` | `serde_json::to_string(&v)?` |
| `JSON.parse(s)` | `serde_json::from_str(&s)?` |
| `Number("42")` | `"42".parse::<i32>()?` |
| `x as Y` (numeric) | `x as Y` |
| `typeof x` (runtime) | not needed; types are checked at compile time |
| object spread `{ ...a, b: 1 }` | struct update `User { b: 1, ..a }` |
| immutability by convention | immutable by default; opt in with `mut` |

See: [Output](/02-basics/04-output/), [JSON with Serde](/15-serialization/03-json/), [Serde Basics](/15-serialization/01-serde-basics/).

---

## Where to go next

This page is the map; the territory is the rest of the guide. The one idea with no JavaScript analogue, and the one worth learning first, is **ownership**: who is responsible for each value and when it is freed. Start there.

- [Why Rust?](/01-getting-started/00-why-rust/) for the motivation.
- [Ownership](/05-ownership/) for the concept that makes everything else click.
- [Error Handling](/08-error-handling/) to replace `try/catch` with values.
- [The Migration Guide](/29-migration-guide/) when you are ready to port real code.
