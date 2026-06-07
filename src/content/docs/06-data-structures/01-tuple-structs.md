---
title: "Tuple Structs and Unit Structs"
description: "Rust tuple structs and unit structs give primitives a nominal, zero-cost identity via the newtype pattern, where TypeScript only has erasable branded types."
---

Beyond the named-field structs you met in [Structs](/06-data-structures/00-structs/), Rust offers two leaner shapes: **tuple structs**, which have positional fields and no names, and **unit structs**, which have no fields at all. Both enable the **newtype pattern**: a zero-cost way to give a primitive a distinct, type-checked identity.

---

## Quick Overview

A **tuple struct** is a named type whose fields are accessed by position (`.0`, `.1`) instead of by name, and a **unit struct** is a type that holds no data at all. To a TypeScript developer they look like odd corners of the language, but they enable something TypeScript cannot do cheaply: wrapping a primitive in a brand-new nominal type (the **newtype pattern**) so the compiler rejects mixing, say, a `Meters` with a `Seconds`.

---

## TypeScript/JavaScript Example

In TypeScript, you usually reach for a tuple type or an object when you want a small fixed group of values, and you fake distinct primitive types with **branded types** because TypeScript is structurally typed.

```typescript
// A positional pair — TypeScript tuple type
type Rgb = [number, number, number];
const red: Rgb = [255, 0, 0];
console.log(red[0], red[1], red[2]); // 255 0 0

// "Distinct" primitive types via branding (a common workaround)
type Meters = number & { readonly __brand: "Meters" };
type Seconds = number & { readonly __brand: "Seconds" };

const meters = (n: number): Meters => n as Meters;
const seconds = (n: number): Seconds => n as Seconds;

function addDistance(a: Meters, b: Meters): Meters {
  return meters(a + b);
}

const d = meters(5);
const t = seconds(2);
addDistance(d, t); // TS error, BUT only because of the fake brand
addDistance(d, 2 as Meters); // compiles — the brand is erased at runtime
```

**Key points:**

- TypeScript is **structurally typed**: any `number` is interchangeable with any other `number` at runtime.
- The "brand" trick (`number & { __brand }`) buys compile-time distinction, but it is erased at runtime and leaks through any `as` cast.
- There is no runtime cost difference: a branded `Meters` is just a `number`.

---

## Rust Equivalent

Rust gives you real, **nominally distinct** types with no runtime overhead, using tuple structs.

```rust
// A tuple struct: positional fields, accessed by .0 / .1 / .2
struct Rgb(u8, u8, u8);

// Newtype pattern: one-field tuple structs that are genuinely different types
struct Meters(f64);
struct Seconds(f64);

fn add_distance(a: Meters, b: Meters) -> Meters {
    Meters(a.0 + b.0)
}

fn main() {
    let red = Rgb(255, 0, 0);
    println!("{} {} {}", red.0, red.1, red.2); // 255 0 0

    let d = Meters(5.0);
    let t = Seconds(2.0);
    let total = add_distance(d, Meters(2.0)); //
    println!("{}", total.0); // 7
    // add_distance(d, t);    // does not compile (error[E0308]): expected `Meters`, found `Seconds`
    let _ = t;
}
```

Running it prints:

```text
255 0 0
7
```

**Key points:**

- `Rgb`, `Meters`, and `Seconds` are real types, not aliases — the compiler enforces the distinction with no `as`-style escape hatch.
- A one-field tuple struct like `Meters(f64)` is the **newtype pattern** (more below).
- Fields are positional: `.0`, `.1`, `.2`.

---

## Detailed Explanation

### Tuple structs: tuples with a name

A plain tuple `(u8, u8, u8)` is anonymous: any `(u8, u8, u8)` is the same type as any other. A **tuple struct** attaches a name, turning it into its own type:

```rust
struct Rgb(u8, u8, u8); // declaration: a 3-field tuple struct

fn main() {
    let red = Rgb(255, 0, 0); // construction looks like a function call
    println!("{}", red.0);     // field access by index
}
```

Compared with the named-field struct from [Structs](/06-data-structures/00-structs/), a tuple struct trades self-documenting field names for brevity. Use it when the meaning of each position is obvious — `Point(f64, f64)`, `Rgb(u8, u8, u8)` — and a named struct when it is not.

You can destructure a tuple struct in a `let` binding, just like a tuple (full coverage in [Pattern Matching](/06-data-structures/04-pattern-matching/)):

```rust
struct Point(f64, f64);

fn main() {
    let origin = Point(0.0, 0.0);
    let Point(x, y) = origin; // bind the two fields by position
    println!("x = {x}, y = {y}");
}
```

### Unit structs: a type with no data

A **unit struct** has no fields. It is named after the *unit type* `()` (the empty tuple), and it carries no data at all:

```rust
struct AlwaysEqual; // no fields, no parentheses

fn main() {
    let _subject = AlwaysEqual; // the value and the type share a name
}
```

Why would you want a value that holds nothing? Because in Rust, **behavior lives on types via traits** (see [impl blocks](/06-data-structures/05-impl-blocks/) and section 09). A unit struct is the perfect peg to hang a trait implementation on when there is no state to store: a strategy object, a marker, or a typestate token. It is **zero-sized**: it occupies 0 bytes.

```rust
fn main() {
    struct AlwaysEqual;
    println!("{}", std::mem::size_of::<AlwaysEqual>()); // 0
}
```

> **Note:** TypeScript has no real equivalent. The closest analogue to a stateless "strategy" is a function or an object literal with only methods, but those still allocate. A Rust unit struct is genuinely empty.

### The newtype pattern (teaser)

A tuple struct with exactly **one** field is called a **newtype**. It wraps an existing type to give it a fresh identity:

```rust
struct Meters(f64);  // a brand-new type that happens to hold an f64
struct UserId(u64);  // not interchangeable with a raw u64 or an OrderId
```

The newtype is the workhorse behind three big wins, each expanded in the sections below:

1. **Type-safe domain modeling**: a `Meters` cannot be passed where `Seconds` is expected.
2. **Zero runtime cost**: `Meters(f64)` has the exact same size as `f64` and is compiled away to a bare `f64` at runtime. (If you also need the *memory layout* guaranteed identical — for FFI, say — add `#[repr(transparent)]`, which makes that promise explicit; the default representation guarantees size and zero overhead but not layout.)
3. **Bypassing the orphan rule**: you can implement an external trait (like `Display`) on a newtype wrapping an external type (like `Vec<String>`).

That a newtype is free is easy to confirm:

```rust
#[derive(Clone, Copy)]
struct Wrapper(u32);

fn main() {
    println!("{}", std::mem::size_of::<u32>());     // 4
    println!("{}", std::mem::size_of::<Wrapper>()); // 4 — identical
}
```

> **Tip:** Because the newtype is the same size as its inner value and carries no runtime tag, you get nominal type safety for the price of structural data. This is the opposite trade-off from TypeScript's branded types, which are *only* compile-time and vanish entirely at runtime.

### Adding behavior with `impl`

Tuple structs and unit structs get `impl` blocks just like named structs ([impl blocks](/06-data-structures/05-impl-blocks/), [associated functions](/06-data-structures/06-associated-functions/)):

```rust
#[derive(Debug, Clone, Copy)]
struct Celsius(f64);

impl Celsius {
    // associated function (constructor) — see associated-functions.md
    fn from_fahrenheit(f: f64) -> Self {
        Celsius((f - 32.0) * 5.0 / 9.0)
    }

    // method
    fn to_fahrenheit(&self) -> f64 {
        self.0 * 9.0 / 5.0 + 32.0
    }
}

fn main() {
    let body = Celsius(37.0);
    println!("{:.1}F", body.to_fahrenheit());    // 98.6F
    let freezing = Celsius::from_fahrenheit(32.0);
    println!("{freezing:?}");                      // Celsius(0.0)
}
```

Output:

```text
98.6F
Celsius(0.0)
```

A unit struct most often exists *to* carry an `impl` of some trait:

```rust
trait Greet {
    fn greet(&self) -> String;
}

struct EnglishGreeter; // holds nothing; exists only to implement Greet

impl Greet for EnglishGreeter {
    fn greet(&self) -> String {
        "Hello!".to_string()
    }
}

fn main() {
    let greeter = EnglishGreeter;
    println!("{}", greeter.greet()); // Hello!
}
```

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust |
| --- | --- | --- |
| Positional group | `type T = [number, string]` (anonymous, structural) | `struct T(i32, String);` (named, nominal) |
| Distinct primitive | Branded type `number & { __brand }`: compile-time only, erased at runtime | Newtype `struct Meters(f64);`: real type, enforced everywhere |
| Empty value-with-behavior | function / object literal of methods (allocates) | unit struct `struct Marker;` (zero-sized) |
| Runtime cost of the wrapper | none (brand is erased), but no real safety | none (monomorphized away), full safety |
| Field access | `t[0]`, `t[1]` | `t.0`, `t.1` |
| Escape hatch | `value as Meters` defeats the brand | no implicit conversion; you must call `.0` deliberately |

The headline difference: **TypeScript is structurally typed, Rust is nominally typed.** In TypeScript two types with the same shape are the same type. In Rust, `Meters(f64)` and `Seconds(f64)` have identical shape but are categorically different, and the compiler will never silently convert one to the other.

> **Note:** Tuple structs vs plain tuples mirrors named structs vs object literals. A plain tuple `(f64, f64)` is structural and anonymous; the moment you write `struct Point(f64, f64);` you have minted a distinct, nominal type.

---

## Common Pitfalls

### Pitfall 1: Forgetting the field is just `.0`, not arithmetic-ready

A newtype is **not** its inner type, so operators do not pass through automatically.

```rust
struct Meters(f64);

fn main() {
    let a = Meters(5.0);
    let b = Meters(2.0);
    let _c = a + b; // does not compile (error[E0369])
}
```

Real compiler error:

```text
error[E0369]: cannot add `Meters` to `Meters`
 --> src/main.rs:6:16
  |
6 |     let _c = a + b; // does not compile (error[E0369])
  |              - ^ - Meters
  |              |
  |              Meters
  |
note: an implementation of `Add` might be missing for `Meters`
```

**Fix:** operate on the inner values explicitly, then re-wrap, or implement the `Add` trait (section 09) if the arithmetic is meaningful for the domain:

```rust
struct Meters(f64);

fn main() {
    let a = Meters(5.0);
    let b = Meters(2.0);
    let c = Meters(a.0 + b.0); // unwrap, add, re-wrap
    println!("{}", c.0);        // 7
}
```

### Pitfall 2: Expecting newtypes to be interchangeable with their inner type

This is the *point* of the pattern, but it surprises TypeScript developers used to branded types leaking through `as`.

```rust
struct Meters(f64);
struct Seconds(f64);

fn add_distance(a: Meters, b: Meters) -> Meters {
    Meters(a.0 + b.0)
}

fn main() {
    let d = Meters(5.0);
    let t = Seconds(2.0);
    add_distance(d, t); // does not compile (error[E0308])
}
```

Real compiler error:

```text
error[E0308]: mismatched types
  --> src/main.rs:11:21
   |
11 |     add_distance(d, t); // does not compile (error[E0308])
   |     ------------    ^ expected `Meters`, found `Seconds`
   |     |
   |     arguments to this function are incorrect
```

Unlike TypeScript's `2 as Meters`, there is no cast that papers over this. You must construct a `Meters` deliberately.

### Pitfall 3: A private inner field cannot be read from outside its module

When you put a newtype in a module and keep its field private (the default), callers cannot peek at `.0`. This is usually exactly what you want — it lets the constructor enforce invariants — but it trips people up.

```rust
mod email {
    #[derive(Debug)]
    pub struct Email(String); // the field is private (no `pub`)

    impl Email {
        pub fn parse(raw: &str) -> Email {
            Email(raw.to_string())
        }
    }
}

use email::Email;

fn main() {
    let e = Email::parse("a@b.com");
    println!("{}", e.0); // does not compile (error[E0616])
}
```

Real compiler error:

```text
error[E0616]: field `0` of struct `Email` is private
  --> src/main.rs:16:22
   |
16 |     println!("{}", e.0); // does not compile (error[E0616])
   |                      ^ private field
```

**Fix:** expose a method like `as_str(&self) -> &str` instead of the raw field. See [associated functions](/06-data-structures/06-associated-functions/) and section 12 for module visibility.

### Pitfall 4: `(5)` is not a one-element tuple

This bites people coming from any language. In Rust (as in math) `(5)` is just `5` with grouping parentheses. A one-element **tuple** needs a trailing comma: `(5,)`. This matters when you build *plain* tuples; tuple *structs* are unaffected because you write the name (`Wrapper(5)`).

```rust
fn main() {
    let not_a_tuple = (5,); // one-element tuple (note the comma)
    println!("{}", not_a_tuple.0); // 5
}
```

> **Warning:** Writing `let x = (5);` triggers the `unused_parens` lint. The compiler warns `unnecessary parentheses around assigned value`, because it parsed `(5)` as the integer `5`, not a tuple.

---

## Best Practices

### 1. Reach for a newtype to make illegal states unrepresentable

If a function takes a `u64` user ID and a `u64` order ID, nothing stops a caller from swapping them. Wrap each in a newtype and the swap becomes a compile error:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct UserId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OrderId(u64);
```

Derive the traits you would otherwise lose by wrapping the primitive (`Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Debug`). See [Structs](/06-data-structures/00-structs/) for the `derive` mechanism.

### 2. Keep the inner field private and validate in the constructor

A newtype whose field is private and whose only constructor validates input becomes a **smart constructor**: once you hold an `Email`, it is guaranteed valid.

```rust
mod email {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Email(String); // private field

    impl Email {
        pub fn parse(raw: &str) -> Result<Email, String> {
            if raw.contains('@') {
                Ok(Email(raw.to_lowercase()))
            } else {
                Err(format!("invalid email: {raw}"))
            }
        }

        pub fn as_str(&self) -> &str {
            &self.0
        }
    }
}

use email::Email;

fn main() {
    let e = Email::parse("Alice@Example.com").unwrap();
    println!("{}", e.as_str()); // alice@example.com

    match Email::parse("not-an-email") {
        Ok(e) => println!("ok: {}", e.as_str()),
        Err(msg) => println!("err: {msg}"),
    }
}
```

Output:

```text
alice@example.com
err: invalid email: not-an-email
```

(`Result`, `Ok`, `Err`, and `match` are covered in section 08 and [Pattern Matching](/06-data-structures/04-pattern-matching/).)

### 3. Use a unit struct for stateless strategies and markers

When a type exists only to carry trait behavior — a logging sink, a hashing strategy, a typestate flag — a unit struct says "no data here" loud and clear and costs nothing.

### 4. Reach for `Deref` only when the newtype really is "a kind of" the inner type

You can make a newtype transparently expose the inner type's methods by implementing `Deref` (a smart-pointer trait; full treatment in section 10). This is convenient for wrappers like `Username(String)`:

```rust
use std::ops::Deref;

struct Username(String);

impl Deref for Username {
    type Target = String;
    fn deref(&self) -> &String {
        &self.0
    }
}

fn main() {
    let name = Username("alice".to_string());
    // Deref coercion lets String/&str methods work directly:
    println!("{}", name.len());          // 5
    println!("{}", name.to_uppercase()); // ALICE
}
```

Output:

```text
5
ALICE
```

> **Warning:** `Deref` is a double-edged sword. It dilutes the encapsulation you bought with the newtype, because all the inner type's methods leak out. Prefer explicit accessors (`as_str`) unless the newtype is genuinely a thin pointer-like wrapper.

---

## Real-World Example

Two production-flavored uses of tuple structs come together here: **type-safe domain IDs** and a **newtype that bypasses the orphan rule** so we can implement `Display` for a `Vec<String>` (which we are not allowed to do directly, because both `Display` and `Vec` are defined outside our crate).

```rust
use std::fmt;

// Newtype over Vec<String> so we can implement the foreign trait `Display`.
// (Implementing `Display` for `Vec<String>` directly is forbidden by the
//  orphan rule, because we own neither the trait nor the type.)
struct CsvRow(Vec<String>);

impl fmt::Display for CsvRow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.join(","))
    }
}

// Type-safe IDs: UserId and OrderId both wrap u64 but are not interchangeable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct UserId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OrderId(u64);

fn fetch_user(id: UserId) -> String {
    format!("user #{}", id.0)
}

fn order_owner(order: OrderId) -> UserId {
    // pretend we looked this up in a database
    UserId(order.0 % 100)
}

fn main() {
    let row = CsvRow(vec!["alice".into(), "30".into(), "admin".into()]);
    println!("{row}");

    let order = OrderId(4207);
    let owner = order_owner(order);
    println!("{}", fetch_user(owner));
}
```

Output:

```text
alice,30,admin
user #7
```

Because `order_owner` returns a `UserId` and `fetch_user` demands a `UserId`, the pipeline is checked end to end. Passing the raw `4207` or an `OrderId` to `fetch_user` would not compile — the kind of swap bug that branded TypeScript types only *sometimes* catch (and never at runtime) is impossible here.

> **Note:** The **orphan rule** says you may implement a trait for a type only if your crate defines the trait *or* the type. Wrapping the foreign `Vec<String>` in your own `CsvRow` makes `CsvRow` a local type, so the `impl Display for CsvRow` is allowed. This is one of the most common reasons to introduce a newtype. See section 09 for the full rule.

---

## Further Reading

### Official Documentation

- [The Rust Book — Defining and Instantiating Structs (tuple & unit structs)](https://doc.rust-lang.org/book/ch05-01-defining-structs.html#using-tuple-structs-without-named-fields-to-create-different-types)
- [The Rust Book — Using the Newtype Pattern](https://doc.rust-lang.org/book/ch20-02-advanced-traits.html#using-the-newtype-pattern-to-implement-external-traits-on-external-types)
- [Rust by Example — Tuple Structs](https://doc.rust-lang.org/rust-by-example/custom_types/structs.html)
- [std::ops::Deref](https://doc.rust-lang.org/std/ops/trait.Deref.html)

### Related Topics in This Guide

- [Structs](/06-data-structures/00-structs/): named-field structs and the `derive` mechanism this file relies on
- [Field Init Shorthand](/06-data-structures/08-field-init-shorthand/): shorthand and struct-update syntax for named-field structs
- [impl Blocks](/06-data-structures/05-impl-blocks/): adding methods to any struct shape
- [Associated Functions](/06-data-structures/06-associated-functions/): `Self::new`-style constructors and smart constructors
- [Enums](/06-data-structures/02-enums/): when a value is one-of-several shapes rather than a single wrapper
- [Pattern Matching](/06-data-structures/04-pattern-matching/): destructuring tuple structs with `let` and `match`
- [Basic Types](/02-basics/01-types/): the primitives newtypes usually wrap, including plain tuples
- [Collections](/07-collections/): the `Vec<T>` wrapped in the real-world example

---

## Exercises

### Exercise 1: A unit-conversion newtype

**Difficulty:** Beginner

**Objective:** Practice declaring a newtype and giving it a method.

**Instructions:** Define two newtypes, `Kilometers(f64)` and `Miles(f64)`, that both derive `Debug`, `Clone`, `Copy`, and `PartialEq`. Add a method `to_miles(self) -> Miles` on `Kilometers` (1 km ≈ 0.621371 mi). Convert a marathon (`42.195` km) and print both values.

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
struct Kilometers(f64);

// TODO: define Miles

impl Kilometers {
    fn to_miles(self) -> Miles {
        /* ??? */
    }
}

fn main() {
    let marathon = Kilometers(42.195);
    // TODO: convert and print
}
```

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
struct Kilometers(f64);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Miles(f64);

impl Kilometers {
    fn to_miles(self) -> Miles {
        Miles(self.0 * 0.621_371)
    }
}

fn main() {
    let marathon = Kilometers(42.195);
    let in_miles = marathon.to_miles();
    println!("{marathon:?} = {in_miles:?}");
}
```

Output:

```text
Kilometers(42.195) = Miles(26.218749345)
```

</details>

### Exercise 2: A validated newtype (smart constructor)

**Difficulty:** Intermediate

**Objective:** Use a private field plus a fallible constructor so that holding the type guarantees an invariant.

**Instructions:** Build a `NonEmptyString` newtype whose inner `String` is private. Provide `new(s: impl Into<String>) -> Option<NonEmptyString>` that returns `None` when the string is empty or only whitespace, and `Some(...)` otherwise. Add `as_str(&self) -> &str`. Show that `"hello"` succeeds and `"   "` fails.

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct NonEmptyString(String);

impl NonEmptyString {
    pub fn new(s: impl Into<String>) -> Option<NonEmptyString> {
        let s = s.into();
        if s.trim().is_empty() {
            None
        } else {
            Some(NonEmptyString(s))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn main() {
    let ok = NonEmptyString::new("hello");
    let bad = NonEmptyString::new("   ");
    println!("{:?}", ok.map(|n| n.as_str().to_string()));  // Some("hello")
    println!("{:?}", bad.map(|n| n.as_str().to_string()));  // None
}
```

Output:

```text
Some("hello")
None
```

> Because the field is private and `new` is the only constructor, any `NonEmptyString` you ever hold is guaranteed non-empty. (`Option`, `map`, and `Into` are covered in [Option](/06-data-structures/03-option-enum/) and section 09.)

</details>

### Exercise 3: Unit structs as a strategy via a trait

**Difficulty:** Intermediate

**Objective:** Use stateless unit structs to implement the same trait with different behavior, then dispatch over them.

**Instructions:** Define a `Tax` trait with `fn rate(&self) -> f64`. Implement it for two unit structs, `UsTax` (0.07) and `EuTax` (0.20). Write `fn total(price: f64, policy: &dyn Tax) -> f64` that applies the rate, and print the total of `100.0` under each policy.

<details>
<summary>Solution</summary>

```rust
trait Tax {
    fn rate(&self) -> f64;
}

struct UsTax;
struct EuTax;

impl Tax for UsTax {
    fn rate(&self) -> f64 { 0.07 }
}

impl Tax for EuTax {
    fn rate(&self) -> f64 { 0.20 }
}

fn total(price: f64, policy: &dyn Tax) -> f64 {
    price * (1.0 + policy.rate())
}

fn main() {
    println!("US: {:.2}", total(100.0, &UsTax)); // US: 107.00
    println!("EU: {:.2}", total(100.0, &EuTax)); // EU: 120.00
}
```

Output:

```text
US: 107.00
EU: 120.00
```

> Each unit struct is zero-sized, so this strategy abstraction costs nothing in memory. `&dyn Tax` is dynamic dispatch (a trait object), covered in section 09.

</details>
