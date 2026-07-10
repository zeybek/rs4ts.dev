---
title: "Const Generics"
description: "Const generics make Rust types generic over a value like array length N, so [u8; 4] and [u8; 8] differ. Unlike TypeScript tuple lengths, enforced at runtime."
---

**Const generics** let a type or function be generic over a *constant value* — most commonly an array length — rather than only over a type. They are how `[T; N]` works, how a fixed-capacity buffer can be a distinct type from a different-capacity one, and how Rust can verify dimensions like "a `2x3` matrix times a `3x2` matrix" at compile time with zero runtime cost.

---

## Quick Overview

A normal generic parameter stands in for a *type* (`Vec<T>`). A **const generic** parameter stands in for a *value* of some primitive type, written `const N: usize`. The classic case is the array type `[T; N]`: the length `N` is part of the type, so `[u8; 4]` and `[u8; 8]` are genuinely different types.

For a TypeScript/JavaScript developer the nearest analogy is a **fixed-length tuple type** such as `readonly [number, number, number]`. The important difference: TypeScript's tuple length is a *compile-time-only* annotation that is fully erased. At runtime it is just a `number[]`, and a length mismatch passes silently. Rust's const generics are **real, monomorphized types** that the compiler turns into distinct, separately-optimized machine code, so the length is enforced and exploitable at runtime too.

---

## TypeScript/JavaScript Example

TypeScript can describe a fixed-length vector with a tuple type and even catch a length mismatch during type-checking:

```typescript
// A 3-element vector, described with a fixed-length tuple type.
type Vec3 = readonly [number, number, number];

function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

const a: Vec3 = [1, 2, 3];
const b: Vec3 = [4, 5, 6];
console.log("dot =", dot(a, b)); // dot = 32

// Passing the wrong length is a *compile-time* type error:
dot(a, [4, 5]);
//     ~~~~~~ Argument of type '[number, number]' is not assignable
//            to parameter of type 'Vec3'. Source has 2 element(s)
//            but target requires 3.
```

That `tsc --strict` error is real. But the length lives **only** in the type system. There is no `Vec3` value at runtime — it is an ordinary JavaScript array — and a generic helper that takes `number[]` will happily run on the wrong length:

```javascript
// Plain JavaScript: the length annotation is gone. Nothing stops a mismatch.
function dot(a, b) {
  return a.reduce((sum, x, i) => sum + x * b[i], 0);
}

console.log(dot([1, 2, 3], [4, 5, 6])); // 32
console.log(dot([1, 2, 3], [4, 5]));     // NaN  — silently wrong
```

So in TypeScript the length is a *hint to the checker*, not a property of the runtime value. And tuple types do not let you write code that is generic over an *arbitrary* fixed length `N` while still tracking that length: you would end up with a separate `Vec2`, `Vec3`, `Vec4`, or fall back to `number[]` and lose the guarantee entirely.

---

## Rust Equivalent

In Rust the length is a first-class generic parameter. One definition covers every length, and the length is part of the type at runtime:

```rust playground
use std::ops::{Add, Index, IndexMut};

// A fixed-size mathematical vector, generic over its length N.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Vector<const N: usize> {
    data: [f64; N],
}

impl<const N: usize> Vector<N> {
    fn from_array(data: [f64; N]) -> Self {
        Vector { data }
    }

    fn zero() -> Self {
        Vector { data: [0.0; N] }
    }

    fn dot(&self, other: &Vector<N>) -> f64 {
        let mut sum = 0.0;
        for i in 0..N {
            sum += self.data[i] * other.data[i];
        }
        sum
    }

    fn len(&self) -> usize {
        N
    }
}

// Adding two vectors is legal only when their lengths match: the same N.
impl<const N: usize> Add for Vector<N> {
    type Output = Vector<N>;
    fn add(self, rhs: Vector<N>) -> Vector<N> {
        let mut out = [0.0; N];
        for i in 0..N {
            out[i] = self.data[i] + rhs.data[i];
        }
        Vector { data: out }
    }
}

impl<const N: usize> Index<usize> for Vector<N> {
    type Output = f64;
    fn index(&self, i: usize) -> &f64 {
        &self.data[i]
    }
}

impl<const N: usize> IndexMut<usize> for Vector<N> {
    fn index_mut(&mut self, i: usize) -> &mut f64 {
        &mut self.data[i]
    }
}

fn main() {
    let a = Vector::from_array([1.0, 2.0, 3.0]); // Vector<3>
    let b = Vector::from_array([4.0, 5.0, 6.0]); // Vector<3>

    let sum = a + b;
    println!("a + b   = {:?}", sum.data);
    println!("a . b   = {}", a.dot(&b));
    println!("len(a)  = {}", a.len());

    let z = Vector::<4>::zero(); // explicitly Vector<4>
    println!("zero(4) = {:?}", z.data);

    let c = Vector::from_array([7.0, 8.0]); // Vector<2>
    println!("c[1]    = {}", c[1]);
}
```

Real output:

```text
a + b   = [5.0, 7.0, 9.0]
a . b   = 32
len(a)  = 3
zero(4) = [0.0, 0.0, 0.0, 0.0]
c[1]    = 8
```

One `Vector<const N: usize>` definition serves every length, the length is recoverable at runtime (`a.len()` returns `3`), and — as the next sections show — a length mismatch is a compile error, not a silent `NaN`.

---

## Detailed Explanation

### Declaring a const generic parameter

```rust
struct Vector<const N: usize> {
    data: [f64; N],
}
```

The syntax `const N: usize` declares a generic parameter whose value is a `usize` constant. It sits in the same `<...>` angle-bracket list as type parameters and lifetimes. The ordering rule is: lifetimes first, then type and const parameters (which may be interleaved). Inside the body, `N` is usable anywhere a constant of that type is needed: most importantly as an array length `[f64; N]`, but also in expressions like `0..N`.

> **Note:** The type after `const` (here `usize`) is the *type of the constant value*, not a type parameter. The only types currently allowed for a const generic parameter are the integer types, `bool`, and `char`, confirmed by the compiler error in the next section.

### Supplying the value

There are three ways `N` gets a concrete value:

1. **Inference from an argument.** `Vector::from_array([1.0, 2.0, 3.0])` passes a `[f64; 3]`, so the compiler infers `N = 3`.
2. **Explicit turbofish.** `Vector::<4>::zero()` names the value directly, just like `Vec::<i32>::new()` names a type. Const generic arguments go in the same `::<...>` list.
3. **From a type annotation.** `let z: Vector<4> = Vector::zero();` lets the binding's declared type drive `N`.

Const generic arguments are written as a value (`4`, `true`, `'.'`) or a *standalone* const parameter (`N`). A more complex expression like `N + 1` is **not** allowed in argument position on stable; see [Key Differences](#key-differences).

### Monomorphization makes each length a real type

Rust **monomorphizes** generics: the compiler stamps out a separate, fully-specialized copy of the code for each concrete set of generic arguments actually used. `Vector<3>` and `Vector<2>` compile to two distinct types with two distinct `add` functions, each with `N` substituted as a literal. This is the same machinery described in [Generic Functions](/09-generics-traits/00-generic-functions/), extended from types to values. It is the opposite of TypeScript's **type erasure**, where `Vec3` vanishes before the code runs.

Because `N` is a compile-time literal inside each monomorphized copy, loops like `for i in 0..N` have a known trip count, arrays are sized exactly, and the optimizer can unroll or vectorize freely. There is no length field stored at runtime and no heap allocation: `Vector<3>` is exactly three `f64`s, `24` bytes, on the stack.

### `impl<const N: usize>` blocks

```rust
impl<const N: usize> Vector<N> { /* ... */ }
```

To write methods that work for *every* `N`, the `impl` block itself must introduce the const parameter, exactly as it would introduce a type parameter `impl<T> Wrapper<T>`. You can also write an `impl` for one specific length — `impl Vector<3> { ... }` — to add methods that only make sense at that size (for example, a `cross` product that exists only for 3-vectors).

### The standard library is built on this

The array type `[T; N]` is the most pervasive const-generic type in Rust, and the standard library exposes const-generic combinators over it:

```rust playground
fn main() {
    // `<[T; N]>::map` keeps the length in the type: [i32; 4] -> [i32; 4].
    let doubled = [1, 2, 3, 4].map(|x| x * 2);
    println!("map double = {:?}", doubled);

    // `std::array::from_fn` builds an array, inferring N from the target type.
    let squares: [usize; 5] = std::array::from_fn(|i| i * i);
    println!("squares    = {:?}", squares);

    // Arrays own their elements and yield them by value via IntoIterator
    // (stable since the 2021 edition).
    let odd_sum: i32 = [1, 2, 3].into_iter().filter(|&x| x % 2 == 1).sum();
    println!("odd sum    = {}", odd_sum);
}
```

Real output:

```text
map double = [2, 4, 6, 8]
squares    = [0, 1, 4, 9, 16]
odd sum    = 4
```

`std::array::from_fn` is the idiomatic way to initialize a `[T; N]` element-by-element when `T` is not `Copy` (you cannot write `[value; N]` unless `value: Copy`).

---

## Key Differences

| Aspect | TypeScript fixed-length tuple | Rust const generics |
| --- | --- | --- |
| What it parameterizes over | A *literal* tuple shape (`[number, number, number]`) | Any constant `N` of an integer / `bool` / `char` type |
| Generic over an arbitrary length | No, you write `Vec2`, `Vec3`, … separately | Yes, one `Vector<const N: usize>` covers all |
| Runtime representation | Erased; just a JS array | A real, distinct, monomorphized type per `N` |
| Length recoverable at runtime | Only via `arr.length` (no type involvement) | `N` is a compile-time constant usable in code |
| Mismatch detection | Compile-time only; silent at runtime | Compile-time **and** structurally distinct types |
| Cost | Zero (erased) | Zero at runtime; larger binary (monomorphization) |
| Allowed parameter "types" | Any tuple of types | Integers, `bool`, `char` only (stable) |

### Stable vs. nightly: the const-generics roadmap

What shipped in stable Rust (often called **`min_const_generics`**) is deliberately conservative. The rules you can rely on today:

- Const generic parameters may be **integers, `bool`, or `char`**.
- A const generic *argument* must be either a literal/`const` value, or a **standalone** const parameter (just `N`, not `N + 1` or `N * 2`).

Computing with const parameters — e.g. returning `[T; N + 1]`, or splitting `[T; N]` into `[[T; CHUNK]; N / CHUNK]` — needs the **unstable** `generic_const_exprs` feature and a nightly compiler. The standalone-argument restriction is exactly what produces the error in the next section. (For why some type-system features stay unstable, see the sibling note on [Specialization](/25-advanced-topics/07-specialization/).)

---

## Common Pitfalls

### Pitfall 1: Trying to do arithmetic on a const parameter

A natural first instinct — return an array one element longer — does not compile on stable:

```rust
// does not compile: const arithmetic in array sizes is unstable.
fn push_one<const N: usize>(arr: [i32; N], x: i32) -> [i32; N + 1] {
    let mut out = [0; N + 1];
    out[..N].copy_from_slice(&arr);
    out[N] = x;
    out
}
```

The real compiler emits the same error for *each* use of `N + 1` (here, both the return type and the `[0; N + 1]` array); the representative one is:

```text
error: generic parameters may not be used in const operations
 --> src/main.rs:2:61
  |
2 | fn push_one<const N: usize>(arr: [i32; N], x: i32) -> [i32; N + 1] {
  |                                                             ^ cannot perform const operation using `N`
  |
  = help: const parameters may only be used as standalone arguments here, i.e. `N`
```

> **Tip:** When you truly need a derived length, the stable workarounds are: return a `Vec<T>` (heap, dynamic length), take/return slices `&[T]`, or — if `N` is small and known — hard-code the specific sizes. Reaching for nightly `generic_const_exprs` should be a last resort because the feature is incomplete and its rules still change.

### Pitfall 2: Using a forbidden parameter type

Integers, `bool`, and `char` work; floating-point and arbitrary structs do not:

```rust
struct Flag<const ON: bool>;             // ok
struct Grid<const W: usize, const FILL: char>; // ok
struct Scaled<const FACTOR: f64>;        // does not compile
```

The real error:

```text
error: `f64` is forbidden as the type of a const generic parameter
 --> src/main.rs:6:29
  |
6 | struct Scaled<const FACTOR: f64>;
  |                             ^^^
  |
  = note: the only supported types are integers, `bool`, and `char`
```

Floats are disallowed because const generics rely on *structural equality* of values to decide when two types are the same, and IEEE-754 equality (`NaN != NaN`, `0.0 == -0.0`) does not give a clean, total notion of equality.

### Pitfall 3: The compiler cannot infer `N`

If nothing at the call site pins down `N`, you get an inference error, not a default of zero:

```rust
fn make<const N: usize>() -> [u8; N] {
    [0; N]
}

fn main() {
    let arr = make(); // does not compile: N is unknown
    println!("{:?}", arr);
}
```

The real error tells you exactly how to fix it (the compiler reports `E0284` once per unconstrained use, so you may also see a follow-on copy pointing at the `println!`; the key one is the first):

```text
error[E0284]: type annotations needed for `[u8; _]`
 --> src/main.rs:7:9
  |
7 |     let arr = make();
  |         ^^^   ------ type must be known at this point
  |
note: required by a const generic parameter in `make`
 --> src/main.rs:1:9
  |
1 | fn make<const N: usize>() -> [u8; N] {
  |         ^^^^^^^^^^^^^^ required by this const generic parameter in `make`
help: consider giving `arr` an explicit type, where the value of const parameter `N` is specified
  |
7 |     let arr: [_; N] = make();
  |            ++++++++
```

The fix is to annotate (`let arr: [u8; 4] = make();`) or turbofish (`make::<4>()`).

### Pitfall 4: Expecting different lengths to be interchangeable

Because each `N` is a distinct type, mixing them is a type error, which is the whole point, but it surprises developers coming from `number[]`:

```rust
let a = Vector { data: [1.0, 2.0, 3.0] }; // Vector<3>
let b = Vector { data: [4.0, 5.0] };      // Vector<2>
let _ = a + b; // does not compile
```

The real error:

```text
error[E0308]: mismatched types
  --> src/main.rs:20:17
   |
20 |     let _ = a + b; // mismatched lengths
   |                 ^ expected `3`, found `2`
   |
   = note: expected struct `Vector<3>`
              found struct `Vector<2>`
```

Where TypeScript catches this only during type-checking (and plain JavaScript not at all, producing `NaN`), Rust makes the two lengths *structurally different types*, so the guarantee holds all the way through to the running binary.

---

## Best Practices

- **Prefer `[T; N]` over `Vec<T>` only when the size is genuinely fixed and known at compile time.** Const generics give you stack allocation, no length field, and aggressive optimization, but they also multiply monomorphized code. If lengths vary at runtime, a `Vec<T>` or slice is the right tool. See [Section 21: Performance](/21-performance/) for the binary-size and inlining trade-offs of monomorphization.
- **Encode invariants in the type when a mismatch would be a real bug.** Matrix and vector dimensions, fixed protocol-frame sizes, and ring-buffer capacities are excellent candidates: a wrong size becomes a compile error instead of a runtime panic or silent corruption.
- **Use `std::array::from_fn` to initialize non-`Copy` arrays.** `[None; N]` fails when the element is not `Copy`; `std::array::from_fn(|_| None)` works for any `N`.
- **Give const parameters defaults when one size dominates.** `struct Buffer<T, const N: usize = 32>` lets callers write `Buffer<u8>` for the common case and `Buffer<u8, 8>` to override:

  ```rust playground
  struct Buffer<T, const N: usize = 32> {
      data: [T; N],
  }

  impl<T: Default + Copy, const N: usize> Buffer<T, N> {
      fn new() -> Self {
          Buffer { data: [T::default(); N] }
      }
  }

  fn main() {
      let a: Buffer<u8> = Buffer::new();    // default N = 32
      let b: Buffer<u8, 8> = Buffer::new(); // override
      println!("{} {}", a.data.len(), b.data.len()); // 32 8
  }
  ```

- **Reach for ecosystem crates instead of hand-rolling.** Const generics power `heapless` (fixed-capacity, allocation-free `Vec`/`String`/maps for embedded and real-time code) and the linear-algebra crate `nalgebra` (statically-sized `Matrix<R, C>`). Don't reinvent a fixed-capacity collection if one of those fits.

  ```toml
  # cargo add heapless
  [dependencies]
  heapless = "0.9.3"
  ```

  ```rust playground
  use heapless::Vec;

  fn main() {
      // heapless::Vec<T, const N: usize>: a fixed-capacity, no-heap vector.
      let mut v: Vec<u8, 4> = Vec::new();
      v.push(1).unwrap();
      v.push(2).unwrap();
      v.push(3).unwrap();
      v.push(4).unwrap();

      // Capacity is part of the type; the fifth push has nowhere to go.
      assert_eq!(v.push(5), Err(5));

      println!("len={} capacity={}", v.len(), v.capacity());
  }
  ```

  Real output:

  ```text
  len=4 capacity=4
  ```

---

## Real-World Example

A statically-sized matrix where the type system enforces multiplication shapes. A `2x3` matrix can only multiply a `3xK` matrix: the shared inner dimension `3` must match, and the result type `2xK` is computed by the compiler. A shape mismatch never reaches runtime.

```rust playground
// An R x C matrix whose dimensions live in the type.
#[derive(Debug, Clone, Copy)]
struct Matrix<const R: usize, const C: usize> {
    rows: [[f64; C]; R],
}

impl<const R: usize, const C: usize> Matrix<R, C> {
    fn new(rows: [[f64; C]; R]) -> Self {
        Matrix { rows }
    }

    // (R x C) * (C x K) = (R x K). The shared dimension C is enforced by the
    // type system, so a shape mismatch is a compile error, never a panic.
    fn mul<const K: usize>(&self, other: &Matrix<C, K>) -> Matrix<R, K> {
        let mut out = [[0.0; K]; R];
        for i in 0..R {
            for j in 0..K {
                for k in 0..C {
                    out[i][j] += self.rows[i][k] * other.rows[k][j];
                }
            }
        }
        Matrix { rows: out }
    }
}

fn main() {
    let a: Matrix<2, 3> = Matrix::new([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
    let b: Matrix<3, 2> = Matrix::new([[7.0, 8.0], [9.0, 10.0], [11.0, 12.0]]);

    let c = a.mul(&b); // inferred as Matrix<2, 2>
    println!("{:?}", c.rows);
}
```

Real output:

```text
[[58.0, 64.0], [139.0, 154.0]]
```

If you try to multiply incompatible shapes — say a `Matrix<2, 3>` by a `Matrix<2, 4>` — the compiler rejects it because the method requires the right-hand operand to be `Matrix<C, K>` where `C = 3` (the full message also prints a `note: method defined here` pointing at `mul`, trimmed here for brevity):

```text
error[E0308]: mismatched types
  --> src/main.rs:23:19
   |
23 |     let _ = a.mul(&b);
   |               --- ^^ expected `3`, found `2`
   |               |
   |               arguments to this method are incorrect
   |
   = note: expected reference `&Matrix<3, _>`
              found reference `&Matrix<2, 4>`
```

This is "make illegal states unrepresentable" applied to numeric code: dimension bugs that would be runtime exceptions in NumPy or a JavaScript math library become compile errors in Rust.

---

## Further Reading

- [The Rust Reference: Const generics](https://doc.rust-lang.org/reference/items/generics.html#const-generics) — the precise rules for declaring and using const parameters.
- [`std::array::from_fn`](https://doc.rust-lang.org/std/array/fn.from_fn.html) and [`<[T; N]>::map`](https://doc.rust-lang.org/std/primitive.array.html#method.map): the const-generic array combinators in `std`.
- [The `min_const_generics` stabilization note](https://blog.rust-lang.org/2021/02/26/const-generics-mvp-beta.html): what shipped first and why the scope was limited.
- [`heapless` on docs.rs](https://docs.rs/heapless) and [`nalgebra` on docs.rs](https://docs.rs/nalgebra): production crates built on const generics.

Cross-links within this guide:

- [Generic Functions](/09-generics-traits/00-generic-functions/) — monomorphization and the turbofish, which const generics extend from types to values.
- [Section 06: Data Structures](/06-data-structures/): arrays, tuples, and `impl` blocks, the building blocks const generics operate on.
- [PhantomData & zero-sized types](/25-advanced-topics/00-phantom-data/): another type-level tool for encoding invariants without runtime data.
- [Generic Associated Types (GATs)](/25-advanced-topics/06-gat/) and [Specialization](/25-advanced-topics/07-specialization/): neighboring type-system features, one stable, one still nightly.
- [Section 21: Performance](/21-performance/) — the binary-size and optimization consequences of monomorphization.
- [Section 26: Systems Programming](/26-systems-programming/): where allocation-free, fixed-size types like these matter most (embedded, real-time).

---

## Exercises

### Exercise 1: Identity matrix and trace

**Difficulty:** Beginner

**Objective:** Practice writing methods on a const-generic type and using `N` inside the body.

**Instructions:** Define `struct Square<const N: usize> { cells: [[f64; N]; N] }`. Implement `Square::<N>::identity()` that returns the `N x N` identity matrix (1.0 on the diagonal, 0.0 elsewhere), and a `trace(&self) -> f64` method that sums the diagonal. Print `trace` of a `3x3` identity (it should be `3`).

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug)]
struct Square<const N: usize> {
    cells: [[f64; N]; N],
}

impl<const N: usize> Square<N> {
    fn identity() -> Self {
        let mut cells = [[0.0; N]; N];
        for i in 0..N {
            cells[i][i] = 1.0;
        }
        Square { cells }
    }

    fn trace(&self) -> f64 {
        (0..N).map(|i| self.cells[i][i]).sum()
    }
}

fn main() {
    let id = Square::<3>::identity();
    println!("trace I3 = {}", id.trace()); // trace I3 = 3
}
```

</details>

### Exercise 2: Fixed-capacity stack

**Difficulty:** Intermediate

**Objective:** Build a small allocation-free collection whose capacity is a type parameter, handling the "full" case without panicking.

**Instructions:** Implement `struct Stack<T, const CAP: usize>` backed by `[Option<T>; CAP]` and a length counter. Provide `new()`, `push(&mut self, value: T) -> Result<(), T>` (returning `Err(value)` when full), `pop(&mut self) -> Option<T>`, and `is_full(&self) -> bool`. Note that you must use `std::array::from_fn(|_| None)` to initialize the array, since `T` is not `Copy`.

<details>
<summary>Solution</summary>

```rust playground
struct Stack<T, const CAP: usize> {
    items: [Option<T>; CAP],
    len: usize,
}

impl<T, const CAP: usize> Stack<T, CAP> {
    fn new() -> Self {
        Stack { items: std::array::from_fn(|_| None), len: 0 }
    }

    fn push(&mut self, value: T) -> Result<(), T> {
        if self.len == CAP {
            return Err(value);
        }
        self.items[self.len] = Some(value);
        self.len += 1;
        Ok(())
    }

    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        self.items[self.len].take()
    }

    fn is_full(&self) -> bool {
        self.len == CAP
    }
}

fn main() {
    let mut s: Stack<&str, 2> = Stack::new();
    assert!(s.push("a").is_ok());
    assert!(s.push("b").is_ok());
    assert_eq!(s.push("c"), Err("c")); // full
    assert!(s.is_full());
    println!("pop = {:?}", s.pop()); // pop = Some("b")
}
```

</details>

### Exercise 3: Polynomial evaluation

**Difficulty:** Intermediate

**Objective:** Use a const generic to size a coefficient array and evaluate efficiently.

**Instructions:** Define `struct Polynomial<const N: usize> { coeffs: [f64; N] }` where `coeffs[i]` is the coefficient of `x^i`. Implement `new(coeffs: [f64; N])` and `eval(&self, x: f64) -> f64` using Horner's method (fold from the highest-degree coefficient down). Verify that `2 + 3x + x^2` evaluated at `x = 2` gives `12`.

<details>
<summary>Solution</summary>

```rust playground
struct Polynomial<const N: usize> {
    coeffs: [f64; N], // coeffs[i] is the coefficient of x^i
}

impl<const N: usize> Polynomial<N> {
    fn new(coeffs: [f64; N]) -> Self {
        Polynomial { coeffs }
    }

    // Horner's method: ((... ) * x + c1) * x + c0.
    fn eval(&self, x: f64) -> f64 {
        self.coeffs.iter().rev().fold(0.0, |acc, &c| acc * x + c)
    }
}

fn main() {
    // 2 + 3x + x^2, at x = 2 -> 2 + 6 + 4 = 12
    let p = Polynomial::new([2.0, 3.0, 1.0]);
    println!("p(2) = {}", p.eval(2.0)); // p(2) = 12
}
```

</details>
