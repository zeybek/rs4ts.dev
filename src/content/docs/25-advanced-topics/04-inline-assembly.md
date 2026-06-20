---
title: "Inline Assembly with `asm!`"
description: "Drop raw CPU instructions into Rust on stable with asm!, wiring variables to registers under unsafe. Why JavaScript and WebAssembly offer no real equivalent."
---

Rust lets you drop a few raw machine instructions directly into a function with the `asm!` macro, on a stable compiler, on every officially-supported architecture. This page is about the rare cases where that is the right tool, how the **register-constraint** syntax keeps the optimizer informed, and the safety contract you are signing when you write an `unsafe { asm!(...) }` block.

---

## Quick Overview

Inline assembly is the lowest level Rust offers: you write literal CPU instructions as strings, and Rust's `asm!` macro wires your Rust variables to specific registers, lets the compiler keep optimizing around the block, and refuses to compile if your operand list is malformed. It became **stable in Rust 1.59** and is available without nightly on x86, x86-64, ARM, AArch64, RISC-V, and several other targets.

For a TypeScript/JavaScript developer there is no equivalent: the V8 engine never lets your code see a register, and the nearest "go fast, trust me" escape hatches are hand-written **WebAssembly** or raw `DataView`/`Buffer` reads, both of which the runtime still sandboxes. Rust's `asm!` has no sandbox. A wrong register constraint here is *undefined behavior*, not a thrown exception, which is why the entire feature lives behind `unsafe`.

> **Note:** You almost never need this. Reach for `asm!` only after intrinsics ([`std::arch`](https://doc.rust-lang.org/core/arch/index.html)), `core::hint`, and a careful look at the generated code have failed you. This page exists so that when you *do* need it, you write it correctly. The broader story of stepping outside the safe-Rust guarantees is in [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/) and [When `unsafe` and FFI Are Actually Necessary (and the Many Times They Are Not)](/20-unsafe-ffi/09-when-to-use/).

---

## TypeScript/JavaScript Example

JavaScript runs on a managed virtual machine. You cannot name a CPU register, you cannot emit an instruction, and you cannot read a flag. The two closest things a senior developer reaches for are (1) hand-written **WebAssembly** bytes when they want predictable, near-metal numeric code, and (2) a `DataView` over an `ArrayBuffer` when they want to reinterpret raw bytes. Both are bounded and checked by the runtime.

```typescript
// metal.mts — the closest JavaScript gets to "drop to the machine"
// Run with: node metal.mts   (Node v22)

// There is NO inline-assembly facility in JS/TS. The lowest you can go is to
// ship hand-assembled WebAssembly bytes and let the engine JIT them.
const wasmBytes = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic + version
  0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f, // type: (i32,i32)->i32
  0x03, 0x02, 0x01, 0x00, // function section
  0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00, // export "add"
  // body: local.get 0; local.get 1; i32.add; end
  0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
]);

const { instance } = await WebAssembly.instantiate(wasmBytes);
console.log("wasm add(37, 5) =", instance.exports.add(37, 5));

// The everyday "trust me, these bytes are a little-endian u32" move:
const buf = new Uint8Array([0xde, 0xad, 0xbe, 0xef]);
const view = new DataView(buf.buffer);
console.log("u32le =", view.getUint32(0, true));
```

Running it under Node v22 prints:

```text
wasm add(37, 5) = 42
u32le = 4022250974
```

Notice what the runtime guarantees: the WebAssembly module is validated before it runs, `getUint32` bounds-checks the offset, and the worst case is a thrown error. You are *never* one typo away from corrupting memory. Rust's `asm!` removes that net entirely. That is the whole point, and the whole danger.

---

## Rust Equivalent

The simplest useful `asm!` block: take a value in a register, run one instruction, hand a value back. Here it is on **AArch64** (Apple Silicon, ARM servers), the architecture this page was compiled and run on:

```rust playground
use std::arch::asm;

/// Add 5 to `x` using a single AArch64 `add` instruction.
fn add_five(x: u64) -> u64 {
    let result: u64;
    // SAFETY: this block reads `x`, writes a fresh register into `result`, and
    // has no memory effects, so it cannot violate any of Rust's invariants.
    unsafe {
        asm!(
            "add {result}, {x}, #5",
            x = in(reg) x,
            result = out(reg) result,
        );
    }
    result
}

fn main() {
    println!("add_five(37) = {}", add_five(37));
}
```

The exact same idea on **x86-64** uses different mnemonics. `{0}` is a positional operand, and `inout(reg)` reuses one register for both input and output:

```rust
use std::arch::asm;

/// Add 5 to `x` using a single x86-64 `add` instruction.
fn add_five(x: u64) -> u64 {
    let mut result = x;
    // SAFETY: reads/writes one register, no memory effects.
    unsafe {
        asm!("add {0}, 5", inout(reg) result);
    }
    result
}

fn main() {
    println!("add_five(37) = {}", add_five(37));
}
```

Both versions, run on their respective targets, print:

```text
add_five(37) = 42
```

> **Tip:** Inline assembly is *not* portable. The instruction text is target-specific. Gate each version behind `#[cfg(target_arch = "...")]` (shown later) or you will get a build that only compiles on one machine. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically, and `asm!` needs no feature flag there.

---

## Detailed Explanation

`asm!` is a macro, not a function, because it has to inspect the *template string* at compile time and match every `{name}` / `{0}` placeholder against an operand. Let's read the AArch64 example line by line.

### The template string

```rust
"add {result}, {x}, #5",
```

This is literal AArch64 assembly with **placeholders** in braces. `{x}` and `{result}` are *not* register names; they are names you bind below. `#5` is an immediate (literal) operand in ARM syntax. Rust concatenates multiple string arguments into one program, so you can write one instruction per string for readability:

```rust
asm!(
    "lsl {tmp}, {x}, #1",   // these three strings...
    "lsl {result}, {x}, #3",
    "sub {result}, {result}, {tmp}", // ...form one assembly program
    // ...operands here...
);
```

### Operand specifiers

After the template strings come the operands. Each one tells the compiler *which Rust value* fills a placeholder and *how the register is used*:

| Specifier            | Meaning                                                                                  |
| -------------------- | ---------------------------------------------------------------------------------------- |
| `in(reg) x`          | Compiler picks a register, puts `x` in it, treats it as read-only.                       |
| `out(reg) y`         | Compiler picks a register, you write into it, the value lands in `y`. Input is garbage.  |
| `inout(reg) z`       | One register: `z` goes in, the new value comes back out into `z`.                        |
| `inout(reg) a => b`  | One register: input `a`, output written to a *different* variable `b`.                   |
| `out(reg) _`         | A scratch register you clobber but don't read back (the `_` means "discard").            |
| `in("eax") v`        | An **explicit** register (`eax`). Required when an instruction hard-codes a register.     |
| `const N`            | A compile-time constant baked straight into the instruction stream.                      |
| `sym some_fn`        | The symbol (address) of a Rust `fn` or `static`.                                         |

`reg` means "any general-purpose register the allocator likes": this is the key to *not* fighting the optimizer. You let the compiler choose; it slots your `asm!` into its register allocation like any other code.

### Why `unsafe`?

The compiler cannot read your assembly. It does not know whether `"add {result}, {x}, #5"` actually matches the constraints you declared, whether you trashed a register you promised not to, or whether you read past a buffer. From the borrow checker's perspective, `asm!` is a black box. So the whole construct is `unsafe`: *you* are asserting the instructions honor every promise the operand list makes. This is the same contract discussed in [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/). `asm!` is one of the five "unsafe superpowers."

### Options

The trailing `options(...)` list tells the optimizer what your block does *not* do, which enables more aggressive scheduling:

```rust
asm!(
    "add {out}, {x}, #5",
    x = in(reg) x,
    out = out(reg) out,
    options(pure, nomem, nostack),
);
```

- `nomem` — the block reads/writes no memory.
- `nostack` — it does not push/pop the stack.
- `pure` — same inputs always give the same outputs (lets the compiler dedup/hoist it). `pure` requires `nomem` or `readonly`.
- `preserves_flags` — it does not modify the condition flags.
- `noreturn` — control never returns (then the block has no outputs).

These are **promises**, not requests. If you say `nomem` and then write memory, that is undefined behavior. When in doubt, omit them. The default (no options) is the conservative, always-correct choice.

---

## Key Differences

| Aspect                     | TypeScript / JavaScript                                  | Rust `asm!`                                                            |
| -------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------- |
| Access to CPU registers    | None, fully abstracted by the engine                     | Direct, named or compiler-allocated                                    |
| Closest "low-level" tool   | Hand-written WebAssembly, `DataView`, typed arrays       | Intrinsics (`std::arch`), then `asm!` as a last resort                 |
| Failure mode               | Thrown exception, sandboxed                               | **Undefined behavior**: memory corruption, no exception                |
| Portability                | WebAssembly bytes run anywhere                            | Instruction text is per-architecture; must `#[cfg]`-gate               |
| Optimizer interaction      | JIT owns everything                                       | You declare constraints; LLVM schedules around the block               |
| Safety gate                | Implicit, always on                                      | Explicit `unsafe` block, mandatory                                     |

The single most important difference: **JavaScript's low-level escape hatches are still inside the VM's safety net, and Rust's `asm!` is not.** When a TypeScript developer writes `value as Foo`, the worst outcome is a `TypeError` later. When you mis-declare a register clobber in `asm!`, the worst outcome is silent data corruption that may surface anywhere, anytime.

> **Note:** A common misconception is that `asm!` is "faster than Rust." It is not, by default. The optimizer produces excellent code for ordinary Rust, and an opaque `asm!` block can actually *prevent* optimizations (inlining across it, constant-folding through it). Inline assembly is for instructions the compiler *cannot otherwise emit* — privileged instructions, special registers, exotic SIMD — not for hand-tuning arithmetic.

---

## Common Pitfalls

### Forgetting the `unsafe` block

`asm!` is always unsafe. This is the first wall every newcomer hits:

```rust
use std::arch::asm;

fn main() {
    let mut x: u64 = 10;
    asm!("add {0}, {0}, #5", inout(reg) x); // does not compile (error[E0133])
    println!("{x}");
}
```

The real compiler output:

```text
error[E0133]: use of inline assembly is unsafe and requires unsafe block
 --> src/main.rs:4:5
  |
4 |     asm!("add {0}, {0}, #5", inout(reg) x);
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ use of inline assembly
  |
  = note: inline assembly is entirely unchecked and can cause undefined behavior
```

The fix is to wrap it: `unsafe { asm!(...) }`.

### A placeholder with no matching operand

If your template references `{1}` but you only supplied one operand, the macro catches it at compile time:

```rust
use std::arch::asm;

fn main() {
    let x: u64;
    unsafe {
        asm!("mov {0}, {1}", out(reg) x); // does not compile
    }
    println!("{x}");
}
```

Real output:

```text
error: invalid reference to argument at index 1
 --> src/main.rs:5:24
  |
5 |         asm!("mov {0}, {1}", out(reg) x);
  |                        ^^^ from here
  |
  = note: there is 1 argument
```

### Clobbering a register without declaring it

The most *dangerous* mistake compiles cleanly and then corrupts your program. If your assembly writes to a register that you did not list as an output or clobber, the compiler assumes that register is untouched — it may have been holding a live value. The classic case is calling another function: a `bl`/`call` instruction clobbers all the caller-saved registers per the ABI. You must tell the compiler with `clobber_abi("C")`:

```rust playground
use std::arch::asm;

extern "C" fn the_answer() -> u64 { 42 }

fn sym_demo() -> u64 {
    let result: u64;
    // SAFETY: `clobber_abi("C")` declares that the call trashes the C ABI's
    // caller-saved registers, so the compiler will not assume they survive.
    unsafe {
        asm!(
            "bl {f}",
            f = sym the_answer,
            lateout("x0") result, // AArch64 returns in x0
            clobber_abi("C"),
        );
    }
    result
}

fn main() {
    println!("sym_demo() = {}", sym_demo());
}
```

Run on AArch64, this prints `sym_demo() = 42`. Omit the `clobber_abi("C")` and the program may *appear* to work in a small test and then break once the surrounding function gets more complex and the optimizer keeps a value in a now-clobbered register: a textbook heisenbug.

### Reusing `out` when you meant `lateout`

`out` operands may share a register with `in` operands *only* if the compiler can prove timing is safe. When your assembly reads all its inputs *before* writing any output, use `lateout`, which lets the allocator reuse an input register for the output and produces tighter code. Using plain `out` everywhere is always correct but can waste a register.

### Assuming AT&T vs Intel syntax

On x86/x86-64, Rust defaults to **Intel** syntax (`add dst, src`). If you paste AT&T-syntax assembly (`add src, dst`, `%`-prefixed registers) it will not assemble. Add `options(att_syntax)` if you truly need AT&T. ARM and AArch64 have one syntax, so this trap is x86-only.

---

## Best Practices

1. **Exhaust the alternatives first.** Try a `std::arch` intrinsic, a `core::hint` helper, or just trusting the optimizer. `asm!` is the last 1%.
2. **Always write a `// SAFETY:` comment** above the block stating which invariants you have personally verified, the same discipline used throughout [Building Safe Abstractions Over `unsafe`](/20-unsafe-ffi/08-safety-abstractions/).
3. **Prefer `{name} =` operands over positional `{0}`** for anything longer than one instruction; named operands survive edits and re-orderings.
4. **Let the allocator choose registers (`reg`)** unless an instruction hard-requires a specific one (`cpuid` → EAX/EBX/ECX/EDX, shift counts → CL, etc.).
5. **Declare every effect.** Outputs, clobbered scratch registers (`out(reg) _`), `clobber_abi` for calls, and accurate `options`. The compiler trusts you completely; reward that trust.
6. **Gate per architecture** with `#[cfg(target_arch = "...")]` and provide a fallback `#[cfg(not(...))]` arm so the crate still builds elsewhere.
7. **Wrap `asm!` in a safe function** with a clear contract, so callers never touch `unsafe` themselves.
8. **Verify the generated code** with `cargo asm`, `objdump`, or the [Compiler Explorer](https://godbolt.org/) before trusting it in production.

> **Tip:** For writing an entire function body in assembly (e.g. a custom calling convention, an interrupt handler, a context switch), use `naked_asm!` inside a `#[unsafe(naked)]` function instead of `asm!`. Naked functions became stable in Rust 1.88 and give you a function with *no* compiler-generated prologue/epilogue, useful for OS and embedded work (see [Systems Programming](/26-systems-programming/)).

---

## Real-World Example

A genuinely justified use of `asm!`: reading the CPU's hardware cycle/tick counter with the absolute minimum overhead, for fine-grained microbenchmarking. There is no single stable, portable intrinsic that lowers to exactly one instruction here, and the counter lives in a special register, so a one-instruction `asm!` is the right call. We provide both x86-64 (`rdtsc`) and AArch64 (`cntvct_el0`) versions behind `cfg`, wrapped in one safe function.

```rust playground
use std::arch::asm;

/// Read a monotonically increasing hardware cycle/tick counter with the lowest
/// possible overhead. There is no portable stable intrinsic that maps to a
/// single instruction here, so a one-instruction `asm!` is justified.
///
/// On x86-64 this is the time-stamp counter (`rdtsc`); on AArch64 it is the
/// virtual count register (`cntvct_el0`). Both are reads with no memory
/// effects, so `nomem` + `nostack` let the optimizer schedule around them.
#[inline]
fn read_cycle_counter() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        let lo: u32;
        let hi: u32;
        // SAFETY: `rdtsc` writes EAX:EDX, reads no memory, uses no stack.
        unsafe {
            asm!(
                "rdtsc",
                out("eax") lo,
                out("edx") hi,
                options(nomem, nostack),
            );
        }
        ((hi as u64) << 32) | (lo as u64)
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ticks: u64;
        // SAFETY: reads a system register into one register, no memory/stack.
        unsafe {
            asm!(
                "mrs {ticks}, cntvct_el0",
                ticks = out(reg) ticks,
                options(nomem, nostack),
            );
        }
        ticks
    }
}

fn fibonacci(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        (a, b) = (b, a + b);
    }
    a
}

fn main() {
    let start = read_cycle_counter();
    let result = fibonacci(90);
    let end = read_cycle_counter();
    println!("fib(90) = {result}");
    println!("elapsed ticks: {}", end.wrapping_sub(start));
}
```

Compiled and run on AArch64, one sample run printed:

```text
fib(90) = 2880067194370816120
elapsed ticks: 14
```

The tick count varies run to run (it is a real hardware counter) and the *units* differ between architectures — `rdtsc` counts reference cycles, `cntvct_el0` counts a fixed-frequency timer — which is exactly why this belongs in a clearly-documented, architecture-gated helper rather than scattered through your code. For production timing prefer `std::time::Instant`; reach for the raw counter only when you need sub-nanosecond, instruction-level resolution.

### When `asm!` is genuinely the answer

- **Special/privileged registers and instructions:** `cpuid`, `rdtsc`, `mrs`/`msr`, `svc`/`syscall`, `cli`/`sti`, `wfi` — things with no safe-Rust spelling.
- **Custom calling conventions / naked functions:** context switches, interrupt entry points, bootloaders.
- **An exotic instruction your target has but `std::arch` does not expose** as a stable intrinsic.
- **Bare-metal embedded** where you must poke a specific peripheral instruction.

If your reason is "I think I can beat the optimizer at integer math," it is almost certainly *not* the answer.

---

## Further Reading

- [The `asm!` chapter of the Rust Reference](https://doc.rust-lang.org/reference/inline-assembly.html): the authoritative spec for operands, options, and clobbers.
- [Inline assembly — Rust By Example](https://doc.rust-lang.org/rust-by-example/unsafe/asm.html): worked, runnable examples.
- [`std::arch` module docs](https://doc.rust-lang.org/core/arch/index.html): the **portable intrinsics** you should try *before* `asm!`.
- [Naked functions tracking and docs](https://doc.rust-lang.org/reference/attributes/codegen.html#the-naked-attribute): for whole-function assembly with `naked_asm!`.

Cross-links within this guide:

- [Unsafe Blocks and Operations](/20-unsafe-ffi/01-unsafe-rust/): the `unsafe` keyword and the safety contract `asm!` relies on.
- [When `unsafe` and FFI Are Actually Necessary (and the Many Times They Are Not)](/20-unsafe-ffi/09-when-to-use/): deciding whether to drop to unsafe at all.
- [Zero-Cost Abstractions](/21-performance/06-zero-cost/) — why ordinary Rust is usually already optimal.
- [Custom Allocators](/25-advanced-topics/03-allocators/) — another low-level hook (`#[global_allocator]`) that, unlike `asm!`, is safe to wire up.
- [Systems Programming](/26-systems-programming/) — bare-metal and OS contexts where assembly shows up most.
- [Introduction](/00-introduction/) · [Getting Started](/01-getting-started/) · [Basics](/02-basics/) — start here if any prerequisites are unfamiliar.

---

## Exercises

> Set up a probe project to check your answers: `cargo new asm_exercises && cd asm_exercises`. Inline assembly is target-specific, so each solution below provides an arm for x86-64 and one for AArch64. Build with `cargo run` on your native machine.

### Exercise 1

**Difficulty:** Beginner

**Objective:** Practice the basic `in`/`out` operand syntax with a shift-and-add trick.

**Instructions:** Write a function `times_nine(x: u64) -> u64` that computes `x * 9` *without* using a multiply instruction. Hint: `x * 9 == x + (x << 3)`. Implement it for your native architecture using a single instruction with `reg` operands and `options(pure, nomem, nostack)`.

<details>
<summary>Solution</summary>

```rust playground
use std::arch::asm;

// AArch64: a single `add` with a shifted operand does it in one instruction.
#[cfg(target_arch = "aarch64")]
fn times_nine(x: u64) -> u64 {
    let out: u64;
    // SAFETY: pure arithmetic on registers, no memory or stack effects.
    unsafe {
        asm!(
            "add {out}, {x}, {x}, lsl #3", // x + (x << 3) = x*9
            x = in(reg) x,
            out = out(reg) out,
            options(pure, nomem, nostack),
        );
    }
    out
}

// x86-64: `lea` computes address arithmetic, perfect for x + x*8.
#[cfg(target_arch = "x86_64")]
fn times_nine(x: u64) -> u64 {
    let out: u64;
    // SAFETY: pure arithmetic on registers, no memory or stack effects.
    unsafe {
        asm!(
            "lea {out}, [{x} + {x}*8]", // x + x*8 = x*9
            x = in(reg) x,
            out = out(reg) out,
            options(pure, nomem, nostack),
        );
    }
    out
}

fn main() {
    println!("times_nine(6) = {}", times_nine(6));
}
```

Output (both architectures):

```text
times_nine(6) = 54
```

</details>

### Exercise 2

**Difficulty:** Intermediate

**Objective:** Use a conditional/compare instruction and learn `inout`.

**Instructions:** Write a branchless `max_u64(a: u64, b: u64) -> u64` that returns the larger of the two values using a compare plus a conditional-select (AArch64 `csel`) or conditional-move (x86-64 `cmov`). Avoid any `if`/branch in your assembly.

<details>
<summary>Solution</summary>

```rust playground
use std::arch::asm;

#[cfg(target_arch = "aarch64")]
fn max_u64(a: u64, b: u64) -> u64 {
    let out: u64;
    // SAFETY: compare + conditional select on registers; no memory/stack.
    unsafe {
        asm!(
            "cmp {a}, {b}",
            "csel {out}, {a}, {b}, hs", // out = (a >= b unsigned) ? a : b
            a = in(reg) a,
            b = in(reg) b,
            out = out(reg) out,
            options(pure, nomem, nostack),
        );
    }
    out
}

#[cfg(target_arch = "x86_64")]
fn max_u64(a: u64, b: u64) -> u64 {
    let mut out = a;
    // SAFETY: compare + conditional move on registers; no memory/stack.
    unsafe {
        asm!(
            "cmp {out}, {b}",
            "cmovb {out}, {b}", // if out < b (unsigned), out = b
            out = inout(reg) out,
            b = in(reg) b,
            options(pure, nomem, nostack),
        );
    }
    out
}

fn main() {
    println!("max_u64(17, 42) = {}", max_u64(17, 42));
    println!("max_u64(99, 42) = {}", max_u64(99, 42));
}
```

Output (both architectures):

```text
max_u64(17, 42) = 42
max_u64(99, 42) = 99
```

</details>

### Exercise 3

**Difficulty:** Advanced

**Objective:** Drive a fixed-register instruction and wrap it in a safe, portable API.

**Instructions:** On x86-64, write a safe function `cpu_vendor() -> Option<String>` that executes `cpuid` with leaf 0 and assembles the 12-byte vendor string (the bytes come back as EBX, then EDX, then ECX). `cpuid` hard-codes its registers, so you must use explicit-register operands. And because LLVM reserves `rbx`, you must save and restore it yourself. On every non-x86-64 target, return `None` so the crate still builds. Wrap the `unsafe` block so callers never see it.

<details>
<summary>Solution</summary>

```rust playground
use std::arch::asm;

/// Returns the 12-byte CPU vendor string, or `None` on non-x86-64 targets.
fn cpu_vendor() -> Option<String> {
    #[cfg(target_arch = "x86_64")]
    {
        let (ebx, ecx, edx): (u32, u32, u32);
        // SAFETY: `cpuid` with leaf 0 only writes the four output registers and
        // touches no memory; we preserve rbx by saving/restoring it ourselves.
        unsafe {
            asm!(
                "mov {ebx_tmp:r}, rbx", // stash LLVM-reserved rbx
                "cpuid",
                "xchg {ebx_tmp:r}, rbx", // pull EBX out, restore rbx
                inout("eax") 0u32 => _,  // leaf 0 in EAX; EAX result discarded
                ebx_tmp = out(reg) ebx,
                out("ecx") ecx,
                out("edx") edx,
                options(nostack, preserves_flags),
            );
        }
        let mut v = Vec::with_capacity(12);
        v.extend_from_slice(&ebx.to_le_bytes());
        v.extend_from_slice(&edx.to_le_bytes());
        v.extend_from_slice(&ecx.to_le_bytes());
        return String::from_utf8(v).ok();
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        None
    }
}

fn main() {
    match cpu_vendor() {
        Some(v) => println!("vendor: {v}"),
        None => println!("vendor: <not available on this target>"),
    }
}
```

On an Intel x86-64 host this prints something like `vendor: GenuineIntel`; on AMD, `vendor: AuthenticAMD`. Built for `aarch64`, the `None` arm runs and prints:

```text
vendor: <not available on this target>
```

> The `{ebx_tmp:r}` syntax names the operand `ebx_tmp` and selects its 64-bit (`r`) register-class form. In real code you would prefer the safe [`std::arch::x86_64::__cpuid`](https://doc.rust-lang.org/core/arch/x86_64/fn.__cpuid.html) intrinsic, which handles the `rbx` dance for you — this exercise reimplements it to learn the mechanics.

</details>
