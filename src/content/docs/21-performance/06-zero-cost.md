---
title: "Zero-Cost Abstractions"
description: "In JavaScript a .map().filter() chain allocates and is slower than a loop. Rust's iterators compile to identical assembly, proven here with real disassembly."
---

In most languages there is a tax for writing expressive code: a `.map().filter().reduce()` chain allocates intermediate arrays and pays per-element callback overhead, so the "clean" version is measurably slower than a hand-written loop. Rust makes a stronger promise: its high-level abstractions, especially **iterators** and **closures**, compile down to the *same machine code* you would have written by hand. This topic explains that promise and, just as importantly, *shows the evidence*: real assembly and real benchmarks proving an iterator chain and a `for` loop are the same program.

---

## Quick Overview

A **zero-cost abstraction** is a feature that, once compiled, adds no runtime overhead compared to writing the low-level equivalent by hand. The phrase is usually attributed to Bjarne Stroustrup's rule for C++: "what you don't use, you don't pay for; and what you do use, you couldn't hand-code any better." Rust inherits that philosophy. Iterators, closures, generics, `Option`/`Result`, and `async` are all designed to vanish at compile time, leaving behind tight, optimized code.

For a TypeScript/JavaScript developer this is a genuine shift in intuition. In Node.js, `arr.map(f).filter(g)` builds a new array after `map`, then walks it again in `filter`, and calls `f` and `g` through a function reference each time, so the idiomatic version really is slower than a single `for` loop, and performance guides tell you to avoid chaining in hot paths. In Rust the opposite is true: the idiomatic iterator chain is *the* fast version. You are encouraged to write the expressive code, because the compiler erases the abstraction entirely.

> **Note:** "Zero-cost" means *zero runtime cost*, not *zero compile cost*. The compiler does real work — monomorphization and inlining — to make abstractions disappear, and that work shows up as longer build times. See [Reducing Compile Time](/21-performance/07-compilation-time/) for the trade-off.

---

## TypeScript/JavaScript Example

Here is a realistic computation: sum the squares of the even numbers in a large array. The expressive, idiomatic JavaScript uses array methods.

```typescript
// sumSquaresEven.ts
function sumSquaresEvenChained(data: number[]): number {
  return data
    .filter((x) => x % 2 === 0) // allocates a new array
    .map((x) => x * x)          // allocates a SECOND new array
    .reduce((acc, x) => acc + x, 0);
}

// The hand-written loop a performance-conscious dev would fall back to:
function sumSquaresEvenLoop(data: number[]): number {
  let total = 0;
  for (let i = 0; i < data.length; i++) {
    const x = data[i];
    if (x % 2 === 0) total += x * x;
  }
  return total;
}

const data = Array.from({ length: 1_000_000 }, (_, i) => i);
console.log(sumSquaresEvenChained(data)); // 166666166667000000 — but see below
console.log(sumSquaresEvenLoop(data));
```

The chained version is clean and reads top-to-bottom, but it does measurably more work: `.filter()` allocates an intermediate array of ~500,000 elements, `.map()` allocates another, and each `(x) => ...` is an indirect call the JIT must inline (and *may* deoptimize). That is why JavaScript performance advice routinely says "prefer a `for` loop in hot code." The abstraction is *not* free.

> **Warning:** That `console.log` value is also a reminder of a JS footgun unrelated to speed. `number` is always an IEEE-754 `f64`, so once intermediate sums exceed `Number.MAX_SAFE_INTEGER` (2^53 − 1) they silently lose precision. They do **not** wrap. The exact-integer arithmetic this topic relies on is something Rust's `u64`/`i64` give you for free.

---

## Rust Equivalent

The same two functions in Rust. The idiomatic one is the iterator chain.

```rust playground
/// Sum the squares of the even numbers — idiomatic, expressive, FAST.
fn sum_sq_even_iter(data: &[i64]) -> i64 {
    data.iter()
        .filter(|&&x| x % 2 == 0)
        .map(|&x| x * x)
        .sum()
}

/// The hand-written loop, for comparison.
fn sum_sq_even_loop(data: &[i64]) -> i64 {
    let mut total = 0;
    for &x in data {
        if x % 2 == 0 {
            total += x * x;
        }
    }
    total
}

fn main() {
    let data: Vec<i64> = (0..1_000_000).collect();
    println!("iter = {}", sum_sq_even_iter(&data));
    println!("loop = {}", sum_sq_even_loop(&data));
}
```

The difference from JavaScript is fundamental, not cosmetic. The Rust `.filter().map().sum()` chain does **not** allocate any intermediate `Vec`. Each adapter (`Filter`, `Map`) is a tiny struct that holds the previous iterator; calling `.sum()` drives a single pass that pulls one element at a time through the whole chain. This is called **iterator fusion**, and after optimization the compiler collapses all of it into one loop. The next section proves that with the generated assembly.

---

## Detailed Explanation

### How an iterator chain is built

An iterator in Rust is any type implementing the `Iterator` trait, whose core is one method:

```rust
pub trait Iterator {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
}
```

Adapters like `map` and `filter` do **not** run anything immediately. They return a new struct that *wraps* the source iterator:

- `data.iter()` returns a `slice::Iter` (a pointer pair walking the slice).
- `.filter(pred)` returns `Filter<slice::Iter, Closure>`: a struct holding the inner iterator and your closure.
- `.map(f)` returns `Map<Filter<...>, Closure2>`, another wrapper.
- `.sum()` is a **consumer**: it repeatedly calls `next()` on the outermost wrapper until it returns `None`.

Each `next()` call on `Map` calls `next()` on `Filter`, which calls `next()` on `Iter`. Because every one of these methods is small and marked for inlining, the optimizer (LLVM) splices them all into a single function body, deletes the wrapper structs entirely (they hold no heap data), and is left with: load element, test parity, square, accumulate. Exactly the body of the hand loop.

> **Note:** This laziness is the same principle behind Rust's `async`: a `Future` does nothing until polled, just as an iterator adapter does nothing until consumed. If you internalized "Rust futures are lazy, unlike eager JS Promises" from [11-async](/11-async/), iterators are the synchronous version of the same idea.

### Closures are monomorphized, not boxed

Each closure in Rust is its own anonymous **type** that implements one of the `Fn`/`FnMut`/`FnOnce` traits. When you pass a closure to a generic function, the compiler **monomorphizes**: it stamps out a specialized copy of that function for that exact closure type, then inlines the closure body in. There is no function pointer and no allocation. (Contrast TypeScript generics, which are *erased* at runtime; Rust generics are *specialized* at compile time, which is precisely what makes the closure call disappear.)

This is also why `i64`, the iterator chain, and the closures together produce no surprises: it is all one concrete, fully-typed program by the time codegen runs.

---

## The Evidence

Claims about "zero cost" are worthless without proof, so here is the proof, generated on a real Rust 1.96.0 toolchain (current stable, 2024 edition, which `cargo new` selects automatically).

### Evidence 1: identical assembly (x86-64)

Take the simplest possible case so the assembly is short, summing a slice two ways:

```rust
// src/lib.rs
#[unsafe(no_mangle)]
pub fn sum_loop(data: &[u64]) -> u64 {
    let mut total = 0;
    for &x in data {
        total += x;
    }
    total
}

#[unsafe(no_mangle)]
pub fn sum_iter(data: &[u64]) -> u64 {
    data.iter().copied().sum()
}
```

> **Tip:** `#[unsafe(no_mangle)]` keeps the function names readable in the assembly output (it disables Rust's name mangling). In the 2024 edition `no_mangle` is written inside `unsafe(...)` because exporting an unmangled symbol can cause link-time collisions. It is purely a labeling convenience here, not part of the abstraction.

Emit optimized x86-64 assembly with Intel syntax:

```bash
rustc --edition 2024 --crate-type lib -O \
  --target x86_64-unknown-linux-gnu \
  --emit asm -C "llvm-args=-x86-asm-syntax=intel" \
  -o out.s src/lib.rs
```

Both functions auto-vectorize to SSE2. Here is the **inner hot loop** of each, exactly as emitted:

```asm
; sum_loop — inner vectorized loop
.LBB0_6:
	movdqu	xmm2, xmmword ptr [rdi + 8*rax]
	paddq	xmm0, xmm2
	movdqu	xmm2, xmmword ptr [rdi + 8*rax + 16]
	paddq	xmm1, xmm2
	add	rax, 4
	cmp	r8, rax
	jne	.LBB0_6
```

```asm
; sum_iter — inner vectorized loop
.LBB1_6:
	movdqu	xmm2, xmmword ptr [rdi + 8*rax]
	paddq	xmm0, xmm2
	movdqu	xmm2, xmmword ptr [rdi + 8*rax + 16]
	paddq	xmm1, xmm2
	add	rax, 4
	cmp	rcx, rax
	jne	.LBB1_6
```

Running `diff` on the two loop bodies reports only this:

```text
1c1
< .LBB0_6:
---
> .LBB1_6:
7,8c7,8
< 	cmp	r8, rax
< 	jne	.LBB0_6
---
> 	cmp	rcx, rax
> 	jne	.LBB1_6
```

The *only* differences are the loop label (`.LBB0_6` vs `.LBB1_6`) and which register holds the precomputed loop bound (`r8` vs `rcx`). Those are cosmetic register-allocation choices. The computation — two 128-bit loads, two packed 64-bit adds, advance by four, branch — is **byte-for-byte identical**. The iterator chain *is* the hand loop.

### Evidence 2: identical assembly (ARM64 / Apple Silicon), including the filter+map case

On AArch64, even the richer `filter().map().sum()` example from earlier collapses to the same code. Both `sum_sq_even_loop` and `sum_sq_even_iter` produce this inner loop (4-wide unrolled):

```asm
; sum_sq_even_loop inner loop (AArch64)        ; sum_sq_even_iter inner loop (AArch64)
LBB0_5:                                          LBB1_5:
	ldp	x17, x2, [x15, #-16]                       ldp	x15, x16, [x13, #-16]
	ldp	x3, x4, [x15], #32                         ldp	x17, x2, [x13], #32
	mul	x5, x17, x17                               mul	x3, x15, x15
	mul	x6, x2, x2                                 mul	x4, x16, x16
	mul	x7, x3, x3                                 mul	x5, x17, x17
	mul	x19, x4, x4                                mul	x6, x2, x2
	tst	x17, #0x1                                  tst	x15, #0x1
	csel	x17, x5, xzr, eq                          csel	x15, x3, xzr, eq
	tst	x2, #0x1                                   tst	x16, #0x1
	csel	x2, x6, xzr, eq                           csel	x16, x4, xzr, eq
	tst	x3, #0x1                                   tst	x17, #0x1
	csel	x3, x7, xzr, eq                           csel	x17, x5, xzr, eq
	tst	x4, #0x1                                   tst	x2, #0x1
	csel	x4, x19, xzr, eq                          csel	x2, x6, xzr, eq
	add	x8, x17, x8                                add	x8, x15, x8
	add	x12, x2, x12                               add	x10, x16, x10
	add	x13, x3, x13                               add	x11, x17, x11
	add	x14, x4, x14                               add	x12, x2, x12
	subs	x16, x16, #4                               subs	x14, x14, #4
	b.ne	LBB0_5                                     b.ne	LBB1_5
```

Same 21 instructions, same order, same shape: four `mul` (the squares), four `tst`/`csel` pairs (the even-number `filter`, branchlessly), four `add` (the accumulation), one decrement, one branch. Only the *names* of the registers differ; the allocator made independent choices in each function. There is no `filter` overhead, no `map` overhead, no intermediate buffer. The high-level chain compiled to the loop a careful systems programmer would have written by hand.

### Evidence 3: closures inline to nothing

Does passing a closure cost anything? Take a generic function and call it with a literal closure:

```rust
// src/lib.rs
#[unsafe(no_mangle)]
pub fn double_sum(data: &[u64]) -> u64 {
    apply_to_each(data, |x| x * 2)
}

#[inline]
fn apply_to_each<F: Fn(u64) -> u64>(data: &[u64], f: F) -> u64 {
    data.iter().copied().map(f).sum()
}
```

Grepping the emitted assembly of `double_sum` for `call` instructions returns **zero**: the closure was inlined completely, there is no indirect call. Better still, the optimizer recognized `x * 2` and emitted it as a packed self-add: the `* 2` literally became `paddq xmm2, xmm2`:

```asm
.LBB0_...:
	movdqu	xmm2, xmmword ptr [rdi + 8*rax]
	movdqu	xmm3, xmmword ptr [rdi + 8*rax + 16]
	paddq	xmm2, xmm2          ; x * 2, vectorized
	paddq	xmm0, xmm2
	paddq	xmm3, xmm3
	paddq	xmm1, xmm3
	; ...
```

### Evidence 4: a benchmark confirms it (and where it can't)

Assembly is the definitive proof, but a benchmark backs it up. Using [criterion](/21-performance/02-benchmarking/) (`cargo add criterion --dev`, version 0.8.2) to time the two slice-sum functions over a one-million-element `Vec<u64>`:

```text
sum_1M_u64/hand_loop    time:   [105.47 µs 106.84 µs 108.41 µs]
sum_1M_u64/iterator     time:   [109.36 µs 111.69 µs 114.57 µs]
```

The confidence intervals overlap: the two are indistinguishable within measurement noise. The small apparent gap is run-to-run jitter, not a real difference, which is exactly what Evidence 1 predicts. (For a fair benchmark you must wrap inputs in `std::hint::black_box` so the optimizer cannot fold away the whole computation; criterion 0.8 deprecates its own `criterion::black_box` in favor of the standard-library one; see [Benchmarking with Criterion](/21-performance/02-benchmarking/).)

And finally, the functions agree at runtime:

```text
sum_loop  = 500000500000
sum_iter  = 500000500000
equal?      true
```

---

## Key Differences

| Aspect | TypeScript/JavaScript (`arr.map().filter()`) | Rust (`iter().map().filter()`) |
| --- | --- | --- |
| Intermediate arrays | Allocated after each adapter | None; adapters are fused into one pass |
| Per-element callback | Indirect call (JIT tries to inline) | Closure monomorphized + inlined; no call |
| Generics | Erased at runtime; one shared code path | Monomorphized; specialized per concrete type |
| Laziness | Eager: `map` runs immediately, returns array | Lazy: runs only when a consumer drives it |
| Hot-path advice | "Prefer a `for` loop" | "Prefer the iterator chain" |
| Bounds checks in the loop | N/A (engine-managed) | Optimizer proves them away in slice iteration |
| Cost model | Abstraction has measurable runtime cost | Abstraction has zero runtime cost (compile cost instead) |

The reasoning behind Rust's design: the language deliberately exposes ahead-of-time compilation, monomorphization, and inlining so that expressive code and fast code are the *same* code. You are not asked to choose between readability and speed. The cost is paid by the compiler (longer builds, larger binaries from monomorphization) rather than at runtime, a trade explored in [Reducing Compile Time](/21-performance/07-compilation-time/) and [Reducing Binary Size](/21-performance/08-binary-size/).

Iterator slice iteration also removes a cost you might expect: **bounds checks**. Indexing `data[i]` checks `i < data.len()` each time, but `data.iter()` walks pointers the compiler already knows are in range, so there is nothing to check. This is one reason iterators are often *faster* than a naive index loop; see [Optimization Techniques](/21-performance/03-optimization/).

---

## Common Pitfalls

### Pitfall 1: assuming `dyn` dispatch is also free — it is not

Zero cost applies to **static dispatch** (generics, `impl Fn`). The moment you erase the type behind a trait object — `&dyn Fn`, `Box<dyn Iterator>` — you opt into **dynamic dispatch**, which is a real, non-zero cost: a vtable lookup and an indirect call per element. Compare this `&dyn Fn` version:

```rust
#[unsafe(no_mangle)]
pub fn sum_dyn(data: &[u64], f: &dyn Fn(u64) -> u64) -> u64 {
    let mut total = 0;
    for &x in data {
        total += f(x); // indirect call through a vtable, every iteration
    }
    total
}
```

Its assembly contains a real indirect call in the loop body:

```asm
	call	r13      ; the closure is invoked through a function pointer
```

That call cannot be inlined or vectorized away. It is usually fine — dynamic dispatch costs single-digit nanoseconds — but it is *not* zero, so do not reach for `Box<dyn ...>` in a tight numeric loop expecting the iterator magic. Prefer generics (`impl Fn`, `<F: Fn(...)>`) on hot paths and save trait objects for where you genuinely need heterogeneous types. (See [09-generics-traits](/09-generics-traits/) for static vs dynamic dispatch.)

### Pitfall 2: forgetting iterators are lazy

Coming from JavaScript, where `arr.map(f)` runs `f` immediately, this surprises everyone. In Rust, an adapter with no consumer does *nothing*:

```rust playground
fn main() {
    let data = vec![1, 2, 3];
    data.iter().map(|x| {
        println!("touched {x}");
        x * 2
    });
    println!("done");
}
```

The closure never runs, and the compiler warns you:

```text
warning: unused `Map` that must be used
 --> src/main.rs:4:5
  |
4 | /     data.iter().map(|x| {
5 | |         println!("touched {x}");
6 | |         x * 2
7 | |     });
  | |______^
  |
  = note: iterators are lazy and do nothing unless consumed
```

The program prints only `done`. Add a consumer — `.collect::<Vec<_>>()`, `.sum()`, `.count()`, `.for_each(...)`, or a `for` loop — to actually drive the work.

### Pitfall 3: breaking fusion with a needless `collect`

Fusion only works while everything stays inside one chain. Calling `.collect()` in the *middle* forces a heap allocation and a second pass, throwing the benefit away:

```rust
// Wasteful: collects into a temporary Vec just to iterate it again.
fn slow(data: &[i64]) -> i64 {
    let evens: Vec<i64> = data.iter().copied().filter(|x| x % 2 == 0).collect();
    evens.iter().map(|x| x * x).sum()
}

// Fused: one pass, no allocation.
fn fast(data: &[i64]) -> i64 {
    data.iter().copied().filter(|x| x % 2 == 0).map(|x| x * x).sum()
}
```

Only `collect` when you actually need to *store* the result. See [Optimization Techniques](/21-performance/03-optimization/) for more on avoiding needless allocations.

### Pitfall 4: writing index loops out of habit

The `for i in 0..data.len() { data[i] }` pattern is a JavaScript reflex. In Rust it adds bounds checks and reads worse, and Clippy flags it:

```rust playground
fn main() {
    let data = vec![10, 20, 30];
    let mut sum = 0;
    for i in 0..data.len() {
        sum += data[i];
    }
    println!("{sum}");
}
```

```text
warning: the loop variable `i` is only used to index `data`
 --> src/main.rs:4:14
  |
4 |     for i in 0..data.len() {
  |              ^^^^^^^^^^^^^
  = note: `#[warn(clippy::needless_range_loop)]` on by default
help: consider using an iterator
  |
4 -     for i in 0..data.len() {
4 +     for <item> in &data {
```

Follow the lint: `for &x in &data { sum += x; }`, or just `data.iter().sum()`.

---

## Best Practices

- **Reach for iterators first.** They are the idiomatic, readable, *and* fast choice. Do not "drop down" to an index loop for performance; measure first ([When to Optimize](/21-performance/10-when-to-optimize/)); the chain is almost always equal or faster.
- **Keep the whole pipeline in one chain.** Avoid intermediate `collect()` calls. Let `filter`/`map`/`take_while`/`flat_map` fuse, and finish with one consumer.
- **Prefer static dispatch on hot paths.** Use `<F: Fn(...)>` or `impl Fn(...)` for closures and `impl Iterator` for returns when you want the abstraction to vanish. Reserve `dyn`/`Box<dyn ...>` for genuine runtime polymorphism.
- **Trust, but verify.** When a hot loop matters, confirm the abstraction collapsed. Emit assembly with `cargo rustc --release -- --emit asm`, or paste the function into [Compiler Explorer (godbolt.org)](https://rust.godbolt.org/) and watch the chain become a single loop. Then [benchmark](/21-performance/02-benchmarking/) to be sure.
- **Use `sum()`, `product()`, `min()`, `max()`, `count()`, `fold()` instead of manual accumulators.** They are specialized consumers and read as the intent they encode.
- **Enable optimizations before judging.** Zero-cost is a property of the optimized build. A debug (`cargo run`) build keeps every wrapper struct and call; only `--release` performs the inlining. Never benchmark or read assembly from a debug build.

---

## Real-World Example

A production-flavored task: parse an access log, keep only successful (2xx) responses, and total the bytes served. The natural Rust solution is one fused iterator pass with no intermediate collections, and it is also the fast one.

```rust playground
/// One parsed access-log record we care about.
#[derive(Debug)]
struct Hit {
    status: u16,
    bytes: u64,
}

/// Parse "METHOD PATH STATUS BYTES" lines, keep only 2xx hits, and total
/// the bytes served — entirely in one lazy pass with zero heap allocation.
fn total_2xx_bytes(log: &str) -> u64 {
    log.lines()
        .filter_map(|line| {
            // `?` short-circuits to None on any malformed line, skipping it.
            let mut parts = line.split_whitespace();
            let _method = parts.next()?;
            let _path = parts.next()?;
            let status: u16 = parts.next()?.parse().ok()?;
            let bytes: u64 = parts.next()?.parse().ok()?;
            Some(Hit { status, bytes })
        })
        .filter(|hit| (200..300).contains(&hit.status))
        .map(|hit| hit.bytes)
        .sum()
}

fn main() {
    let log = "\
GET /index.html 200 1024
POST /api/login 401 88
GET /style.css 200 4096
GET /missing 404 256
GET /data.json 200 2048";

    println!("2xx bytes served: {}", total_2xx_bytes(log));
}
```

Running it:

```text
2xx bytes served: 7168
```

That is `1024 + 4096 + 2048`. Note what did *not* happen: `lines()`, `filter_map`, `filter`, and `map` allocated **nothing**. The whole pipeline is one pass over the input string, and the `Hit` structs live transiently in registers, never on the heap. The JavaScript equivalent — `log.split("\n").map(parse).filter(...).map(...).reduce(...)` — would allocate three or four intermediate arrays. In Rust the readable code and the efficient code are the same code, which is the entire point of zero-cost abstractions.

> **Tip:** This same composition style scales to real workloads with [rayon](https://docs.rs/rayon): swap `.lines()` for `.par_lines()` (or `.iter()` for `.par_iter()`) and the fused pipeline runs across all cores with no other changes. Data parallelism stays a zero-cost abstraction too.

---

## Further Reading

- [The Rust Programming Language — Comparing Performance: Loops vs. Iterators](https://doc.rust-lang.org/book/ch13-04-performance.html): the official chapter with its own assembly walkthrough.
- [`std::iter` module documentation](https://doc.rust-lang.org/std/iter/index.html) — the full list of adapters and consumers, and how laziness works.
- [`Iterator` trait reference](https://doc.rust-lang.org/std/iter/trait.Iterator.html): every provided method.
- [Rust Compiler Explorer (godbolt.org)](https://rust.godbolt.org/): paste code and watch abstractions collapse to assembly in real time.
- Cross-links within this section:
  - [Benchmarking with Criterion](/21-performance/02-benchmarking/): measure the claim yourself with criterion and `black_box`.
  - [Optimization Techniques](/21-performance/03-optimization/) — avoiding needless `collect`/clones; iterators vs index loops; bounds-check elision.
  - [Profiling Rust Applications](/21-performance/00-profiling/): confirm a hot loop really is hot before micro-optimizing it.
  - [When to Optimize](/21-performance/10-when-to-optimize/) — measure first; readable-then-fast.
  - [Performance](/21-performance/09-comparison/): how this stacks up against the V8 JIT in Node.js.
- Related guide sections: [09-generics-traits](/09-generics-traits/) (monomorphization, static vs dynamic dispatch), [11-async](/11-async/) (lazy futures, the async cousin of lazy iterators), and the [common patterns](/22-common-patterns/) section for idiomatic iterator recipes.

---

## Exercises

### Exercise 1: Rewrite an index loop as an iterator chain

**Difficulty:** Easy

**Objective:** Convert a manual index loop into a fused iterator chain and confirm the result is unchanged.

**Instructions:** Given a `&[i32]`, write `max_abs` that returns the largest **absolute value** as an `Option<i32>` (`None` for an empty slice). Start from this index loop and rewrite it with `iter`, `map`, and a consumer. Avoid any `for` loop or manual indexing.

```rust
fn max_abs(data: &[i32]) -> Option<i32> {
    let mut best: Option<i32> = None;
    for i in 0..data.len() {           // needless_range_loop
        let a = data[i].abs();
        best = Some(match best {
            Some(b) if b >= a => b,
            _ => a,
        });
    }
    best
}
```

<details>
<summary>Solution</summary>

```rust playground
fn max_abs(data: &[i32]) -> Option<i32> {
    data.iter().map(|&x| x.abs()).max()
}

fn main() {
    println!("{:?}", max_abs(&[-5, 3, -1, 4])); // Some(5)
    println!("{:?}", max_abs(&[]));             // None
}
```

`max()` is a consumer that already returns `Option`, handling the empty case for free. Output:

```text
Some(5)
None
```

The whole thing fuses into one pass; no temporary `Vec`, no bounds checks.

</details>

### Exercise 2: A fused dot product

**Difficulty:** Medium

**Objective:** Combine two slices in a single allocation-free iterator pass using `zip`.

**Instructions:** Write `dot(a: &[f64], b: &[f64]) -> f64` returning the dot product `Σ aᵢ·bᵢ`. Use `iter`, `zip`, `map`, and `sum` — no intermediate `Vec`, no index loop. (Assume the slices are the same length; `zip` stops at the shorter one.)

<details>
<summary>Solution</summary>

```rust playground
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn main() {
    println!("{}", dot(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0])); // 32
}
```

`zip` pairs elements lazily; `map` multiplies each pair; `sum` accumulates. Output:

```text
32
```

`1·4 + 2·5 + 3·6 = 32`. On a release build this typically auto-vectorizes to packed floating-point multiply-add instructions, identical to a hand-written loop; verify it on [godbolt](https://rust.godbolt.org/).

</details>

### Exercise 3: Count without allocating, then prove it is zero-cost

**Difficulty:** Medium

**Objective:** Write an allocation-free count and verify it matches the hand-written loop in assembly.

**Instructions:**
1. Write `count_long_words(text: &str, n: usize) -> usize` returning how many whitespace-separated words have length greater than `n`. Use `split_whitespace`, `filter`, and `count` — do **not** collect into a `Vec`.
2. Write the equivalent `for`-loop version, then describe how you would confirm the two compile to the same code.

<details>
<summary>Solution</summary>

```rust playground
fn count_long_words(text: &str, n: usize) -> usize {
    text.split_whitespace().filter(|w| w.len() > n).count()
}

// Equivalent hand-written loop:
fn count_long_words_loop(text: &str, n: usize) -> usize {
    let mut count = 0;
    for w in text.split_whitespace() {
        if w.len() > n {
            count += 1;
        }
    }
    count
}

fn main() {
    let text = "the quick brown fox jumps";
    println!("{}", count_long_words(text, 3));      // 3
    println!("{}", count_long_words_loop(text, 3)); // 3
}
```

Output:

```text
3
3
```

`"quick"`, `"brown"`, and `"jumps"` each have length 5 > 3, while `"the"` (3) and `"fox"` (3) do not.

To **prove** they are the same code, emit optimized assembly for both functions and diff them:

```bash
rustc --edition 2024 --crate-type lib -O --emit asm -o out.s src/lib.rs
```

(After marking each `pub` and `#[unsafe(no_mangle)]` so the symbols are findable.) The inner loop bodies will be identical apart from label names and register-allocation choices, exactly as Evidence 1 in this topic showed. Alternatively, paste both into [godbolt](https://rust.godbolt.org/) and compare side by side. The `count()` consumer compiles to a counter increment guarded by the same length test the loop performs by hand — no `Vec`, no second pass.

</details>
