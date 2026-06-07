---
title: "When `unsafe` and FFI Are Actually Necessary (and the Many Times They Are Not)"
description: "Most application code never needs unsafe or FFI. A decision procedure, the few cases that truly require it, and the safe Rust tools that replace the rest."
---

The rest of this section taught you *how* to write `unsafe` and call into C. This page is about a harder, more valuable skill: deciding **whether you should at all**. For the overwhelming majority of application code (including code that feels like it "obviously needs" a native escape hatch) the answer is no, and reaching for `unsafe` or the Foreign Function Interface (FFI) trades away the exact guarantee you came to Rust for.

---

## Quick Overview

Coming from TypeScript, you have an instinct that "going low-level" means abandoning safety: in JavaScript there is no low-level to go to, so the very idea feels exotic and powerful. In Rust, `unsafe` is rarely the tool for performance, sharing, or cleverness; it is a tool for a **small, specific set of jobs the type system genuinely cannot express**: talking to C/C++, talking to hardware or the OS at the syscall level, and building a handful of foundational data structures whose internal invariants the borrow checker cannot see.

The practical rule for a working developer is this: **default to safe Rust, and treat every `unsafe` block or FFI boundary as a cost you must justify**: a piece of code where *you*, not the compiler, are now responsible for memory safety, and where a mistake is undefined behavior (UB) rather than a catchable exception. Most of the time the justified reason is "I must call code written in another language," not "I want this to be faster" or "the borrow checker is in my way." This page gives you a decision procedure and a set of safe alternatives so you can tell the difference.

---

## TypeScript/JavaScript Example

In Node.js, the equivalent decision is "should I drop down to a **native addon** (a `.node` binary built from C++/Rust) instead of staying in pure JavaScript?" TypeScript developers face this exact fork, and the reasoning maps closely onto Rust's `unsafe`/FFI decision.

```typescript
// checksum.ts — a CPU-bound task someone might think "needs a native addon."

// Option A: pure JavaScript. Boring, portable, no build step, no segfaults.
function crc32(data: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let i = 0; i < 8; i++) {
      crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

// Option B: a native addon (e.g. via `node-gyp` + C++, or a prebuilt .node file).
//   import { crc32 } from "fast-crc32-native";   // a compiled binary
// Faster on huge inputs — but now you own:
//   - a C/C++ toolchain in CI and on every contributor's machine
//   - prebuilt binaries per OS/arch (or a compile step at install time)
//   - the risk that a bug in the native code crashes the whole Node process
//     (a segfault, not a try/catch-able Error)

const payload = new TextEncoder().encode("the quick brown fox");
console.log("0x" + crc32(payload).toString(16).padStart(8, "0"));
// 0x91c102ca
```

Run with `node --experimental-strip-types checksum.ts` and it prints `0x91c102ca`. The lesson every senior Node developer eventually internalizes: a native addon is a **liability you take on for a measured win**, not a default. The same calculus — only sharper, because the failure mode is UB instead of a process crash you can at least observe — governs `unsafe` and FFI in Rust.

---

## Rust Equivalent

Here is the punchline first: that same CRC32, in Rust, needs **zero `unsafe` and zero FFI**, because a battle-tested, pure-safe-Rust crate already exists. The first move when you think "I need to call C" is almost always "search crates.io."

```rust
// src/main.rs  —  Cargo.toml: crc32fast = "1.5"
use crc32fast::Hasher;

// "We already have a C library for CRC32, let's FFI to it."
// Before writing a line of unsafe, check crates.io: a battle-tested,
// pure-safe-Rust crate almost always already exists.
fn checksum(data: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

fn main() {
    let payload = b"the quick brown fox";
    println!("crc32 = {:#010x}", checksum(payload));
}
```

```text
$ cargo run
crc32 = 0x91c102ca
```

`crc32fast` (current version `1.5.0`) exposes a fully safe API and uses CPU acceleration (SIMD) where available. Its small `unsafe` core — the SIMD hot path — is written, audited, and tested once inside the crate, so *your* code stays 100% safe. You got the speed of a native implementation, the matching checksum (`0x91c102ca`, identical to the Node output above), and you kept every memory-safety guarantee. That is the pattern: **the safe Rust ecosystem has usually already paid the `unsafe` cost for you, once, behind a reviewed boundary**, so you don't have to.

> **Tip:** When you catch yourself reaching for FFI or `unsafe`, run `cargo search <thing>` (or browse [lib.rs](https://lib.rs)) first. The answer to "is there a safe crate for this?" is yes far more often than newcomers expect.

---

## Detailed Explanation

To decide well, separate the *legitimate* reasons for `unsafe`/FFI from the *seductive but usually wrong* ones.

### The genuinely necessary cases

There are only a few, and they share a property: **the requirement comes from outside Rust's type system**, so no amount of safe-Rust cleverness can satisfy it.

1. **Calling code written in another language.** You must use an existing C/C++ library (OpenSSL, SQLite, a vendor SDK, a game engine, an OS API not yet wrapped). This is FFI's reason to exist. The `unsafe` is unavoidable *at the boundary*, because the Rust compiler cannot see across the language barrier to verify the foreign code keeps its promises. Covered in [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/) and [Generating Bindings with bindgen](/20-unsafe-ffi/05-bindgen/).

2. **Being called by another language.** You are shipping a Rust library that JavaScript, Python, or C must consume, for example a Node.js native addon. The `extern "C"` / `#[no_mangle]` boundary is necessarily `unsafe`. See [FFI Basics](/20-unsafe-ffi/03-ffi-basics/), [Node.js Native Addons with napi-rs](/20-unsafe-ffi/06-napi/), and [Node.js Native Addons with Neon](/20-unsafe-ffi/07-neon/).

3. **Foundational data structures the borrow checker cannot express.** A doubly linked list, an arena allocator, a lock-free queue, a custom smart pointer: these have aliasing patterns (two pointers to one node, self-references) that are *sound* but that the borrow checker conservatively rejects. The standard library, `Vec`, `Box`, `Rc`, and crates like `crossbeam` and `slotmap` are exactly this: a thin audited `unsafe` core under a safe API. The skill of writing that core is [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/). The skill on *this* page is recognizing that **you almost never need to write it yourself**, because it already exists.

4. **Hardware, kernels, embedded, and syscalls.** Memory-mapped I/O, custom allocators, `no_std` firmware, raw syscalls: places where there is no OS or runtime to provide safety, so you provide it. Out of scope for application developers; see [Section 26: Systems Programming](/26-systems-programming/).

5. **A *measured*, *hot* performance gain that safe Rust cannot reach.** Real, but rare, and last on the list deliberately. This means: you profiled, you found a specific bottleneck, the bounds check or the safe abstraction is provably the cost, and an `unsafe` version is meaningfully faster. The discipline of *measuring before optimizing* belongs to [Section 21: Performance](/21-performance/).

### The seductive-but-usually-wrong cases

These are the ones that catch TypeScript developers, because the instincts that serve you in JavaScript point the wrong way here.

- **"It'll be faster."** Usually false. Safe Rust is already a systems language: no garbage collector, no boxing-by-default, monomorphized generics, and an optimizer (LLVM) that aggressively elides bounds checks it can prove are redundant and *autovectorizes* tight loops into SIMD. The safe version is typically within noise of the `unsafe` one, and sometimes *faster*, because `unsafe` can inhibit optimizations the compiler would otherwise make under safe aliasing rules.

- **"The borrow checker won't let me share this."** This is the big one. In JavaScript every object is a shared, mutable, garbage-collected reference, so "two things point at one thing" is your default mental model. Rust forbids that by default. But it gives you safe tools for it: `Rc`/`Arc` for shared ownership, `RefCell`/`Mutex`/`RwLock` for interior mutability, and indices into a `Vec` (a "generational arena") instead of pointers between nodes. Reaching for raw pointers to "get around" the borrow checker is almost always a sign you should reach for one of these instead.

- **"I need to reinterpret these bytes."** `std::mem::transmute` is `unsafe` and a classic foot-gun. The safe, total, endianness-defined functions `to_be_bytes`/`from_le_bytes`/`as_bytes`, the `bytemuck` crate, and `TryFrom` cover the vast majority of real byte-wrangling.

- **"I need SIMD."** The autovectorizer often emits it from a plain loop. When you need explicit control, the safe wrappers around `std::arch` and the `wide`/`pulp` crates give safe SIMD APIs on stable; portable `std::simd` is also coming but is currently nightly-only (the `portable_simd` feature, tracking issue [#86656](https://github.com/rust-lang/rust/issues/86656)).

The throughline: in JavaScript "going native" is the *only* way to go fast or go low-level, so the instinct is to escape the language. In Rust, **safe Rust is already the low-level language**; the escape hatch is for crossing a language boundary, not for going faster within Rust.

---

## Key Differences

| Question | TypeScript/JavaScript answer | Rust answer |
| --- | --- | --- |
| "How do I make this CPU-bound code fast?" | Drop to a native addon (C++/Rust `.node`). | Stay in safe Rust; it's already native. Profile, then maybe `unsafe`. |
| "How do two parts share one mutable thing?" | Just pass the object; everything is a shared ref. | `Rc`/`Arc` + `RefCell`/`Mutex`, or arena indices. Raw pointers are a last resort. |
| "What happens if I get the low-level code wrong?" | A thrown `Error` or a process crash you can observe. | **Undefined behavior**: may corrupt data silently, may "work" until it doesn't. |
| Default posture toward the escape hatch | Avoid native addons unless measured. | Avoid `unsafe`/FFI unless required by a language boundary or measured. |
| Who is responsible for safety inside it? | The V8/Node runtime, mostly. | **You.** The compiler stops checking; the `// SAFETY:` comment is your proof. |
| Can tooling tell me it's there? | Hard (binary blob). | Yes: `#![forbid(unsafe_code)]`, `cargo geiger`, grep for `unsafe`. |

The most important row is the third. A bug in a Node native addon typically segfaults the process — bad, but *observable* and *local*. A bug in Rust `unsafe` is UB: the compiler is now allowed to assume your invariant held, so it may optimize based on a false premise, producing corruption that surfaces far from the cause. This is why the bar for `unsafe` is higher than the bar for a native addon, not lower.

---

## Common Pitfalls

### Pitfall 1: Using `unsafe` to "reinterpret" bytes when a safe API exists

A developer wants the four bytes of a `u32` and reaches for `transmute`. It does not even compile outside `unsafe`, and that compiler error is the language *telling you to stop and reconsider*:

```rust
fn main() {
    // does not compile (error[E0133]: call to unsafe function is unsafe and requires unsafe block)
    let bytes: [u8; 4] = std::mem::transmute(0x4142_4344_u32);
    println!("{bytes:?}");
}
```

The real error from `cargo build`:

```text
error[E0133]: call to unsafe function `std::intrinsics::transmute` is unsafe and requires unsafe block
 --> src/main.rs:3:26
  |
3 |     let bytes: [u8; 4] = std::mem::transmute(0x4142_4344_u32);
  |                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
  |
  = note: consult the function's documentation for information on how to avoid undefined behavior
```

The fix is not to add `unsafe { }`. It is to use the safe, explicit, **endianness-defined** API. `transmute` would silently give you platform-dependent byte order, which is a latent bug. The safe functions force you to *say which order you mean*:

```rust
fn main() {
    let value: u32 = 0x4142_4344;
    let be = value.to_be_bytes(); // big-endian: [0x41, 0x42, 0x43, 0x44]
    let le = value.to_le_bytes(); // little-endian
    println!("be = {be:?}");
    println!("le = {le:?}");

    // Reinterpreting back is itself a safe, total function:
    let back = u32::from_be_bytes(be);
    println!("round-trips: {}", back == value);
}
```

```text
$ cargo run
be = [65, 66, 67, 68]
le = [68, 67, 66, 65]
round-trips: true
```

### Pitfall 2: Reaching for raw pointers to "beat the borrow checker"

The classic case is "I need two mutable views into one buffer, so I'll cast to `*mut`." You do not: `split_at_mut` is a *safe* standard-library function whose `unsafe` is already written, audited, and tested inside `std`:

```rust
// "I need two mutable views into one Vec, which requires raw pointers." No —
// std gives you split_at_mut, a SAFE function whose unsafe is already audited.
fn normalize_halves(data: &mut [f64]) {
    let mid = data.len() / 2;
    let (left, right) = data.split_at_mut(mid); // two non-overlapping &mut, safely

    let lmax = left.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
    let rmax = right.iter().cloned().fold(0.0_f64, f64::max).max(1.0);

    for x in left.iter_mut() {
        *x /= lmax;
    }
    for x in right.iter_mut() {
        *x /= rmax;
    }
}

fn main() {
    let mut data = vec![2.0, 4.0, 10.0, 5.0];
    normalize_halves(&mut data);
    println!("{data:?}");
}
```

```text
$ cargo run
[0.5, 1.0, 1.0, 0.5]
```

If you had hand-rolled this with raw pointers, you would own the proof that the two views never overlap. `split_at_mut` *is* that proof, written once and reused. The general lesson: when the borrow checker rejects a sound pattern, look for the safe std/crate function that already encapsulates it before writing your own `unsafe`.

### Pitfall 3: Assuming `unsafe` is the path to performance

`unsafe` does not make code fast; it removes a *check*, and the checks safe Rust adds are usually free in release builds (the optimizer elides provable bounds checks and vectorizes loops). Worse, you can make code **slower** with `unsafe`: raw-pointer aliasing can defeat optimizations the compiler performs under safe-reference aliasing guarantees. The honest path is in [Section 21: Performance](/21-performance/): write safe code, profile with `cargo bench`/`criterion`, and only consider `unsafe` against a measured, specific hot spot, with the benchmark to prove the win.

### Pitfall 4: Treating FFI as "free interop"

Calling a C function looks like calling a Rust function, but every FFI call carries hidden obligations: the C side may not be thread-safe, may expect you to free memory it allocated (or it frees memory you must not touch), may use a different string encoding, and crashes it causes are UB on the Rust side too. A native addon also forces a C toolchain into your build and CI. If a pure-Rust crate covers the need, it eliminates an entire category of build-and-safety problems. That is a real engineering win, not laziness.

---

## Best Practices

- **Default to safe; make `unsafe`/FFI justify itself.** Treat every `unsafe` block as code requiring a written rationale and review. The standard is "what invariant am I promising, and why is it true here?"

- **Search the ecosystem first.** Before FFI, look for a pure-Rust crate (`crc32fast`, `ring`/`rustls` instead of OpenSSL, `image`, `regex`, `sha2`, `flate2`). Before raw pointers, look for the std type that already solves it (`Rc`, `RefCell`, `VecDeque`, `split_at_mut`, `slotmap`, `crossbeam`).

- **Prefer safe concurrency and parallelism over native escape hatches.** A CPU-bound aggregation that *feels* like it needs C is usually a `rayon` parallel iterator away. No `unsafe`, scales across cores:

  ```rust
  // src/main.rs  —  Cargo.toml: rayon = "1.12"
  use rayon::prelude::*;

  // A CPU-bound aggregation a TS dev might think "needs C / a native addon."
  // In Rust the SAFE answer is data parallelism via rayon — no unsafe, no FFI.
  // Integers keep the result exact: u64 addition is associative, so the
  // parallel reduction gives the same total no matter how rayon splits the work.
  fn sum_of_squares(data: &[u64]) -> u64 {
      data.par_iter().map(|x| x * x).sum()
  }

  fn main() {
      let data: Vec<u64> = (0..1_000u64).collect();
      println!("sum of squares = {}", sum_of_squares(&data));
  }
  ```

  ```text
  $ cargo run --release
  sum of squares = 332833500
  ```

  > **Note:** This uses `u64` deliberately. Integer addition is associative, so the parallel reduction is fully deterministic. With `f64`, floating-point addition is *not* associative, so the exact low-order digits of a parallel sum depend on how the work was split across threads, useful to know before you print a fixed float total as if it were stable.

- **Lock the door when you can.** If a crate or module genuinely contains no `unsafe`, declare it and let the compiler enforce that forever:

  ```rust
  // Crate-wide guarantee: "this crate contains zero unsafe code."
  // CI fails if anyone sneaks an unsafe block in.
  #![forbid(unsafe_code)]

  fn main() {
      let p = 0xdead_beef_usize as *const i32;
      let value = unsafe { *p }; // does not compile under #![forbid(unsafe_code)]
      println!("{value}");
  }
  ```

  ```text
  error: usage of an `unsafe` block
   --> src/main.rs:5:17
    |
  5 |     let value = unsafe { *p }; // does not compile under #![forbid(unsafe_code)]
    |                 ^^^^^^^^^^^^^
    |
  note: the lint level is defined here
   --> src/main.rs:1:11
    |
  1 | #![forbid(unsafe_code)]
    |           ^^^^^^^^^^^
  ```

  Remove the offending block and the crate is provably `unsafe`-free. Tools like `cargo geiger` audit an entire dependency tree for `unsafe` usage.

- **When you must use `unsafe`, isolate and document it.** Keep it in the smallest possible block, wrap it in a safe API ([Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/)), write a `// SAFETY:` comment stating the invariant you uphold, and run it under [Miri](https://github.com/rust-lang/miri) to catch UB the compiler can't.

- **Measure before optimizing with `unsafe`.** "Faster" is a claim that needs a benchmark. See [Section 21: Performance](/21-performance/).

---

## Real-World Example

A practical decision walkthrough: you are building a service that needs to **deduplicate and share a large in-memory catalog** across request handlers. The instinct from JavaScript — "just hold references to the same objects everywhere" — translates in a naive Rust port to "I need raw pointers so handlers can share the catalog and mutate it." That instinct is wrong; the safe tools cover it exactly.

```rust
use std::cell::RefCell;
use std::rc::Rc;

// The "two things point at one thing" pattern. In C you'd reach for raw
// pointers; in Rust the safe tools are Rc (shared ownership) + RefCell
// (runtime-checked interior mutability) — zero unsafe in YOUR code.
#[derive(Debug)]
struct Counter {
    value: i32,
}

fn main() {
    let shared = Rc::new(RefCell::new(Counter { value: 0 }));

    // Two independent owners of the SAME underlying Counter:
    let a = Rc::clone(&shared);
    let b = Rc::clone(&shared);

    a.borrow_mut().value += 1;
    b.borrow_mut().value += 41;

    println!("value = {}", shared.borrow().value);
    println!("strong refs = {}", Rc::strong_count(&shared));
}
```

```text
$ cargo run
value = 42
strong refs = 3
```

Three owners (`shared`, `a`, `b`) point at one `Counter`, all mutate it, and the program is fully memory-safe with no `unsafe` anywhere. For a multithreaded server you would swap `Rc`/`RefCell` for `Arc`/`Mutex` (or `RwLock`) and gain compile-time data-race freedom, still no raw pointers.

The decision record for this feature would read: *"Shared mutable catalog. Considered raw pointers (rejected: borrow-checker friction is a signal, not an obstacle). Considered FFI to a C cache library (rejected: no language boundary involved, adds toolchain). Chose `Arc<RwLock<Catalog>>`: safe, idiomatic, and the compiler proves the absence of data races."* That paper trail — *what we considered and why we did not need the escape hatch* — is the deliverable this page is teaching you to produce. The single time it should conclude "yes, use `unsafe`/FFI" is when a real language boundary or a measured, profiled bottleneck forces it.

---

## Further Reading

### Official documentation

- [The Rust Book — Unsafe Rust](https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html): when the five superpowers are appropriate.
- [The Rustonomicon](https://doc.rust-lang.org/nomicon/): the deep guide to writing sound `unsafe`, including why "faster" is rarely the reason.
- [`Rc`](https://doc.rust-lang.org/std/rc/struct.Rc.html), [`RefCell`](https://doc.rust-lang.org/std/cell/struct.RefCell.html), and [`std::sync`](https://doc.rust-lang.org/std/sync/index.html) — the safe sharing/mutation tools that replace most pointer-juggling.
- [`slice::split_at_mut`](https://doc.rust-lang.org/std/primitive.slice.html#method.split_at_mut) and [integer `to_be_bytes`/`from_le_bytes`](https://doc.rust-lang.org/std/primitive.u32.html#method.to_be_bytes) — safe APIs that cover common "I need `unsafe`" cases.
- [`cargo geiger`](https://github.com/geiger-rs/cargo-geiger) and [Miri](https://github.com/rust-lang/miri) — audit and verify `unsafe` usage in your tree.

### Related sections in this guide

- Foundations: [Why Rust?](/01-getting-started/00-why-rust/) explains why safe Rust is already a systems language; [Basics](/02-basics/) and [Section 00: Introduction](/00-introduction/) set the baseline mental model.
- This section: [What `unsafe` Really Means (and What It Does Not)](/20-unsafe-ffi/00-unsafe-intro/) (what `unsafe` is and is not) · [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/) · [Raw Pointers](/20-unsafe-ffi/02-raw-pointers/) · [FFI Basics](/20-unsafe-ffi/03-ffi-basics/) · [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/) · [Generating Bindings with bindgen](/20-unsafe-ffi/05-bindgen/) · [Node.js Native Addons with napi-rs](/20-unsafe-ffi/06-napi/) · [Node.js Native Addons with Neon](/20-unsafe-ffi/07-neon/) · [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/) (the unsafe-inside/safe-outside pattern, for the times you *do* need it).
- Going further: [Section 21: Performance](/21-performance/) — measure before you optimize, the right home for the "is `unsafe` faster?" question; [Section 26: Systems Programming](/26-systems-programming/) — the hardware/kernel cases where `unsafe` is genuinely required.

---

## Exercises

### Exercise 1: Replace an FFI plan with a safe crate

**Difficulty:** Beginner

**Objective:** Build the reflex of searching crates.io before writing FFI.

**Instructions:** A colleague proposes adding a C dependency and an `extern "C"` binding to compute SHA-256 hashes "because OpenSSL is fast." Without writing any `unsafe` or FFI, compute the SHA-256 of the bytes `b"hello world"` using a pure-Rust crate and print it as a lowercase hex string. (Hint: `cargo add sha2`, then `Sha256::digest`.) Confirm it prints the well-known value `b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9`.

<details>
<summary>Solution</summary>

```rust
// src/main.rs  —  Cargo.toml: sha2 = "0.11"  (or `cargo add sha2` for the current version)
use sha2::{Digest, Sha256};

fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    // Each byte -> two lowercase hex chars; no unsafe, no FFI.
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    println!("{}", sha256_hex(b"hello world"));
}
```

```text
$ cargo run
b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
```

`sha2` is pure, audited Rust with optional CPU acceleration — all the speed of a native hash with none of the FFI liabilities (no C toolchain, no manual memory management, no UB risk). When you genuinely *must* link an existing C library, the techniques are in [Calling C Code from Rust](/20-unsafe-ffi/04-calling-c/); the point of this exercise is that you usually do not have to.

</details>

### Exercise 2: Two mutable views without raw pointers

**Difficulty:** Intermediate

**Objective:** Recognize that "the borrow checker won't let me alias" has a safe answer.

**Instructions:** You must write a function that, given `&mut [i32]`, simultaneously holds a mutable view of the first half and the second half and adds the maximum of the second half to every element of the first half. A teammate insists this "requires casting to `*mut i32`." Implement it with **no `unsafe`**, then state in one sentence which standard-library function made the unsafe unnecessary.

<details>
<summary>Solution</summary>

```rust
fn boost_first_half(data: &mut [i32]) {
    let mid = data.len() / 2;
    // split_at_mut yields two non-overlapping &mut slices — safely.
    let (front, back) = data.split_at_mut(mid);
    let boost = back.iter().copied().max().unwrap_or(0);
    for x in front.iter_mut() {
        *x += boost;
    }
}

fn main() {
    let mut data = vec![1, 2, 3, 10, 20, 30];
    boost_first_half(&mut data);
    println!("{data:?}");
}
```

```text
$ cargo run
[31, 32, 33, 10, 20, 30]
```

`slice::split_at_mut` made the `unsafe` unnecessary: it is a safe standard-library function that returns two non-overlapping mutable slices, encapsulating (once, audited inside `std`) the very pointer reasoning the teammate wanted to hand-roll.

</details>

### Exercise 3: Write the decision record

**Difficulty:** Hard

**Objective:** Practice the judgment this page is really about — deciding *against* `unsafe`/FFI and justifying it.

**Instructions:** You are designing an in-memory graph (nodes with edges to other nodes) for a routing service. The "obvious" implementation uses `Box`/raw pointers for node-to-node links and immediately fights the borrow checker. Choose a **safe** representation, implement a minimal version that compiles and runs (build a 3-node graph and print each node's neighbor count), and write a two-to-three-sentence decision record explaining why you did *not* use raw pointers or FFI. (Hint: store nodes in a `Vec` and represent edges as indices — `usize` — into that `Vec`, a "generational arena" pattern.)

<details>
<summary>Solution</summary>

Representing edges as **indices into a `Vec`** sidesteps the self-referential-pointer problem entirely: indices are plain `usize` values the borrow checker is happy with, there is no aliasing to prove, and the whole structure is `Copy`-friendly and serializable for free.

```rust
struct Node {
    label: String,
    edges: Vec<usize>, // indices into Graph::nodes, not pointers
}

struct Graph {
    nodes: Vec<Node>,
}

impl Graph {
    fn new() -> Self {
        Graph { nodes: Vec::new() }
    }

    fn add_node(&mut self, label: &str) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            label: label.to_string(),
            edges: Vec::new(),
        });
        id
    }

    fn add_edge(&mut self, from: usize, to: usize) {
        self.nodes[from].edges.push(to);
    }
}

fn main() {
    let mut g = Graph::new();
    let a = g.add_node("A");
    let b = g.add_node("B");
    let c = g.add_node("C");
    g.add_edge(a, b);
    g.add_edge(a, c);
    g.add_edge(b, c);

    for node in &g.nodes {
        println!("{} -> {} neighbor(s)", node.label, node.edges.len());
    }
}
```

```text
$ cargo run
A -> 2 neighbor(s)
B -> 1 neighbor(s)
C -> 0 neighbor(s)
```

**Decision record:** *Considered `Box`/raw-pointer node links (rejected: self-referential pointers fight the borrow checker and would require an `unsafe` core to make sound). Considered FFI to a C graph library (rejected: no language boundary is involved, and it would add a C toolchain to CI). Chose index-based edges into a `Vec` — fully safe, no `unsafe`, trivially serializable, and the standard "arena" idiom; if profiling later showed indexing to be a measured bottleneck we would revisit, but not before.* The production-grade version of this idiom is the `slotmap` or `petgraph` crate, which adds generational indices to prevent stale-index bugs — again, safe Rust off the shelf.

</details>

---

**Next:** [↑ Section 20: Unsafe & FFI](/20-unsafe-ffi/) — return to the section landing page, or revisit [Building Safe Abstractions](/20-unsafe-ffi/08-safety-abstractions/) for the times you *do* need `unsafe`.

**Going further:** [Section 21: Performance →](/21-performance/) — the disciplined, measure-first home for the "but is `unsafe` faster?" question.
