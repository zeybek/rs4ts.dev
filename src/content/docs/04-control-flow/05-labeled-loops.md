---
title: "Labeled Loops"
description: "Name an outer loop with 'label to break or continue it from nested loops. Like JavaScript labels, but type-checked in Rust and able to break with a value."
---

When you nest loops, a plain `break` or `continue` only affects the *innermost* loop. **Labeled loops** let you name an outer loop and target it directly, so you can break out of several loops at once or skip to the next iteration of an outer loop. JavaScript has the exact same feature, so the concept will be familiar. But Rust's version is type-checked, can carry a value, and even works on plain blocks.

---

## Quick Overview

A **loop label** is an identifier prefixed with an apostrophe (`'outer:`) placed before a `loop`, `while`, or `for`. You then write `break 'outer` or `continue 'outer` to control *that* loop from anywhere inside the nested loops. This is Rust's clean, structured alternative to flag variables or `goto`, and unlike JavaScript labels, a labeled `loop` can also `break` *with a value*.

> **Note:** This page focuses on the **labeling** mechanism. Plain `break`/`continue` (including `break` returning a value from a single `loop`) is covered in [Break and Continue](/04-control-flow/04-break-continue/), and the loop forms themselves in [Loops](/04-control-flow/01-loops/).

---

## TypeScript/JavaScript Example

JavaScript has supported labeled statements since the beginning. A label is an identifier followed by a colon, and `break`/`continue` can name it:

```typescript
// Find the first cell matching a target in a 2-D grid.
const grid: number[][] = [
  [1, 2, 3],
  [4, 5, 99],
  [7, 8, 9],
];
const target = 99;
let found: [number, number] | null = null;

outer: for (let r = 0; r < grid.length; r++) {
  for (let c = 0; c < grid[r].length; c++) {
    if (grid[r][c] === target) {
      found = [r, c];
      break outer; // breaks BOTH loops at once
    }
  }
}

console.log(
  found ? `Found ${target} at row ${found[0]}, col ${found[1]}` : "not found",
);
// → Found 99 at row 1, col 2

// `continue` to a label: abandon the current row, move to the next one.
const matrix: number[][] = [
  [1, 2, -1, 4],
  [5, 6, 7, 8],
];

rows: for (const row of matrix) {
  let sum = 0;
  for (const val of row) {
    if (val < 0) {
      console.log("negative -> skip rest of row");
      continue rows; // jump to the next iteration of `rows`
    }
    sum += val;
  }
  console.log("row sum:", sum);
}
// → negative -> skip rest of row
// → row sum: 26
```

Two things to note about JavaScript labels: there is **no leading sigil** (just `outer:`), and a labeled `break` can *never* produce a value. Labels are purely control-flow markers.

---

## Rust Equivalent

The same two programs in Rust. The structure is nearly identical: the differences are the leading apostrophe on the label and that we iterate with iterators instead of C-style index loops:

```rust playground
fn main() {
    // Find the first cell matching a target in a 2-D grid.
    let grid = [
        [1, 2, 3],
        [4, 5, 99],
        [7, 8, 9],
    ];
    let target = 99;
    let mut found = None;

    'rows: for (r, row) in grid.iter().enumerate() {
        for (c, &val) in row.iter().enumerate() {
            if val == target {
                found = Some((r, c));
                break 'rows; // breaks BOTH loops at once
            }
        }
    }

    match found {
        Some((r, c)) => println!("Found {target} at row {r}, col {c}"),
        None => println!("{target} not found"),
    }
}
```

Real output:

```text
Found 99 at row 1, col 2
```

And `continue` to a label:

```rust playground
fn main() {
    let matrix = [
        [1, 2, -1, 4],
        [5, 6, 7, 8],
        [9, -1, 11, 12],
    ];

    'rows: for row in &matrix {
        let mut sum = 0;
        for &val in row {
            if val < 0 {
                println!("Negative found, skipping rest of this row");
                continue 'rows; // jump to the next iteration of `'rows`
            }
            sum += val;
        }
        println!("Row sum: {sum}");
    }
}
```

Real output:

```text
Negative found, skipping rest of this row
Row sum: 26
Negative found, skipping rest of this row
```

> **Note:** The label is written `'rows` with a leading apostrophe, the same syntax Rust uses for [lifetimes](/05-ownership/04-lifetimes/). They are unrelated concepts that happen to share the apostrophe sigil; a loop label is never a lifetime, and the compiler always knows which one you mean from context.

---

## Detailed Explanation

### Defining and targeting a label

A label is an identifier with a leading apostrophe, written immediately before the loop keyword and followed by a colon:

```rust
'outer: for i in 0..3 {
    'inner: for j in 0..3 {
        // ...
    }
}
```

Inside the nested loops, `break`/`continue` take an *optional* label argument:

| Statement         | Effect                                                              |
| ----------------- | ------------------------------------------------------------------- |
| `break`           | Exits the **innermost** enclosing loop.                             |
| `break 'outer`    | Exits the loop labeled `'outer` (and everything nested inside it).  |
| `continue`        | Skips to the next iteration of the **innermost** loop.              |
| `continue 'outer` | Skips to the next iteration of the loop labeled `'outer`.           |

Without a label, control flow always targets the closest loop, exactly like JavaScript. The label simply lets you "reach past" inner loops.

### Walking through the grid search

```rust
'rows: for (r, row) in grid.iter().enumerate() {  // (1)
    for (c, &val) in row.iter().enumerate() {     // (2)
        if val == target {
            found = Some((r, c));                 // (3)
            break 'rows;                          // (4)
        }
    }
}
```

1. The outer loop is labeled `'rows`. `.enumerate()` pairs each row with its index `r`.
2. The inner loop is **unlabeled**; we never need to target it specifically.
3. We record the coordinates in `found` (a `mut` variable) before leaving.
4. `break 'rows` exits *both* loops in one statement. A plain `break` here would only end the inner loop, and the outer loop would keep scanning the remaining rows.

### Why a label instead of a flag?

Without labels, escaping nested loops in many languages requires a boolean flag that you check after the inner loop:

```rust playground
fn main() {
    let grid = [[1, 2, 3], [4, 5, 99], [7, 8, 9]];
    let target = 99;
    let mut found = None;

    let mut done = false;            // extra bookkeeping
    for (r, row) in grid.iter().enumerate() {
        for (c, &val) in row.iter().enumerate() {
            if val == target {
                found = Some((r, c));
                done = true;
                break;               // only breaks the inner loop
            }
        }
        if done {                    // ...so we re-check out here
            break;
        }
    }

    println!("{found:?}");
}
```

The labeled version removes the flag and the second check entirely. It is shorter, has no extra mutable state, and makes the intent ("leave the whole search") obvious at the `break` site.

### `continue 'label` vs `break 'label`

`continue 'rows` does **not** leave the outer loop. It abandons the rest of the *current* outer iteration and advances `'rows` to its next value. In the matrix example, when a negative number appears we stop summing that row and move straight to the next row, which is why `Row sum:` is never printed for rows containing a negative.

### Labeled `loop` can break *with a value*

This is where Rust goes beyond JavaScript. A plain `loop` can return a value via `break value` (see [Break and Continue](/04-control-flow/04-break-continue/)), and that works through a label too. So you can search nested loops and produce the result as an expression:

```rust playground
fn main() {
    let needle = 7;
    let haystack = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];

    // The whole search is an expression that evaluates to the position.
    let position = 'search: loop {
        for (r, row) in haystack.iter().enumerate() {
            for (c, &v) in row.iter().enumerate() {
                if v == needle {
                    break 'search Some((r, c)); // exits the loop WITH a value
                }
            }
        }
        break 'search None; // searched everything, found nothing
    };

    println!("position = {position:?}");
}
```

Real output:

```text
position = Some((2, 0))
```

Here `'search` labels a `loop`, the inner `for` loops do the scanning, and `break 'search <value>` jumps out and hands `position` its value. The outer `loop` runs its body exactly once; it exists purely so we have a breakable construct that can carry a value.

> **Important:** Only `loop` can `break` with a value. You **cannot** `break 'label some_value` out of a `for` or `while` loop — see the Common Pitfalls section for the exact compiler error.

### Labeled blocks (no loop required)

Since Rust 1.65, you can label a *plain block* and `break` out of it, no loop involved. This is handy for "compute a value with early exits" without a function:

```rust playground
fn classify(score: i32) -> &'static str {
    let label = 'check: {
        if score < 0 {
            break 'check "invalid";
        }
        if score >= 90 {
            break 'check "A";
        }
        if score >= 80 {
            break 'check "B";
        }
        "C or below" // the block's final value if no `break` fired
    };
    label
}

fn main() {
    println!("{}", classify(95)); // A
    println!("{}", classify(82)); // B
    println!("{}", classify(50)); // C or below
    println!("{}", classify(-3)); // invalid
}
```

Real output:

```text
A
B
C or below
invalid
```

A labeled block uses `break 'label value` to short-circuit, and the block evaluates to that value. JavaScript has no equivalent; its labeled blocks support `break label` but cannot produce a value.

---

## Key Differences from TypeScript/JavaScript

| Aspect                       | JavaScript                          | Rust                                                       |
| ---------------------------- | ----------------------------------- | ---------------------------------------------------------- |
| Label syntax                 | `outer:` (no sigil)                 | `'outer:` (leading apostrophe)                             |
| `break label` / `continue label` | Supported                       | Supported (`break 'label` / `continue 'label`)             |
| Break **with a value**       | Not possible; labels are flow only  | A labeled **`loop`** can `break 'label value`              |
| Label on a plain block       | `block: { break block; }` (no value)| `'block: { break 'block value; }` (yields a value, 1.65+)  |
| Unused label                 | Silently allowed                    | Triggers a `warning: unused label`                         |
| Typo'd / unknown label       | `SyntaxError: Undefined label`      | Compile error `E0426` with a "did you mean" suggestion     |
| `continue label` to a non-loop | `SyntaxError`                     | Compile error                                              |

The headline differences: Rust requires the apostrophe, treats labels as type-checked control flow (so a wrong name is a compile error, not a runtime surprise), warns when a label is unused, and lets a labeled `loop` carry a value out, something JavaScript labels can never do.

> **Tip:** Conventionally Rust labels describe what the loop iterates over (`'rows`, `'pixels`, `'retry`, `'search`) rather than generic `'outer`/`'inner`. A descriptive name makes `break 'rows` read like a sentence.

---

## Common Pitfalls

### Pitfall 1: Forgetting the leading apostrophe

JavaScript labels have no sigil, so it is natural to write `outer:` in Rust. The compiler rejects it:

```rust
fn main() {
    outer: for i in 0..3 {   // does not compile (error: malformed loop label)
        for j in 0..3 {
            if i + j == 2 { break outer; }
        }
    }
}
```

The real error (abridged):

```text
error: malformed loop label
 --> src/main.rs:2:5
  |
2 |     outer: for i in 0..3 {   // does not compile (error: malformed loop label)
  |     ^^^^^
  |
help: use the correct loop label format
  |
2 |     'outer: for i in 0..3 {   // does not compile (error: malformed loop label)
  |     +
```

The fix is exactly what the compiler suggests: add the `'`, giving `'outer: for ...` and `break 'outer`.

### Pitfall 2: Misspelling the label

A label name that doesn't exist is a compile-time error (`E0426`), not a runtime failure as in some languages. The compiler even suggests the nearest match:

```rust
fn main() {
    'outer: for i in 0..3 {
        for j in 0..3 {
            if i + j == 2 {
                break 'outr; // does not compile (error[E0426]: use of undeclared label `'outr`)
            }
        }
    }
}
```

The real error (abridged):

```text
error[E0426]: use of undeclared label `'outr`
 --> src/main.rs:5:23
  |
2 |     'outer: for i in 0..3 {
  |     ------ a label with a similar name is reachable
...
5 |                 break 'outr; // typo: label doesn't exist
  |                       ^^^^^ undeclared label `'outr`
  |                       help: try using similarly named label: `'outer`
```

### Pitfall 3: Trying to `break` with a value out of a labeled `for`/`while`

Because plain `loop` is the only construct that returns a value, attempting `break 'label value` from a `for` (or `while`) is rejected with `E0571`:

```rust
fn main() {
    let x = 'outer: for i in 0..5 {
        for j in 0..5 {
            if i * j > 6 {
                break 'outer i * j; // does not compile (error[E0571])
            }
        }
    };
    println!("{x:?}");
}
```

The real error (abridged):

```text
error[E0571]: `break` with value from a `for` loop
 --> src/main.rs:5:17
  |
2 |     let x = 'outer: for i in 0..5 {
  |             --------------------- you can't `break` with a value in a `for` loop
...
5 |                 break 'outer i * j;
  |                 ^^^^^^^^^^^^^^^^^^ can only break with a value inside `loop` or breakable block
help: use `break` on its own without a value inside this `for` loop
```

**Fixes:** either (a) set a `mut` variable before a plain `break 'outer`, or (b) restructure as a labeled `loop` (which *can* carry a value), or (c) use a labeled block. The Real-World Example below uses approach (b).

### Pitfall 4: A label you never target (dead label)

If you label a loop but every `break`/`continue` inside is unlabeled, the label does nothing, and Rust warns you, which often reveals a bug (you *meant* to write `break 'rows`):

```rust playground
fn main() {
    'outer: for i in 0..3 { // label defined but never targeted
        for j in 0..3 {
            if i + j == 2 {
                break; // breaks only the inner loop
            }
        }
    }
}
```

The real warning:

```text
warning: unused label
 --> src/main.rs:2:5
  |
2 |     'outer: for i in 0..3 { // label defined but never targeted
  |     ^^^^^^
  |
  = note: `#[warn(unused_labels)]` on by default
```

### Pitfall 5: Reaching for labels when an iterator method is clearer

Labeled loops are great for genuinely nested control flow, but a flat search over a single sequence is usually better expressed with iterator adapters. Prefer `.find()`/`.position()`/`.any()` (covered in [Loops](/04-control-flow/01-loops/) and the collections section) when there is no real nesting; they are shorter and harder to get wrong than a hand-rolled labeled loop.

---

## Best Practices

### 1. Name labels after what they iterate

`'rows`, `'tiles`, `'retry`, `'connections` read far better at the `break` site than `'outer`/`'l1`. The label is documentation: `continue 'rows` should tell the reader exactly what is being skipped.

### 2. Use a labeled `loop` (not `for`) when you need a value out

If the goal of the search is to produce a result, model it as `let result = 'name: loop { ...; break 'name value; };`. This keeps the result as a single immutable binding instead of a `mut` flag mutated across iterations.

### 3. Only label the loop you actually target

Adding a label to a loop you never `break`/`continue` to is noise, and Rust warns about it. Label exactly the loop(s) that need targeting; leave the rest unlabeled.

### 4. Don't nest deeper than you can read

Three or more nested labeled loops are a smell. Consider extracting the inner work into a function that returns `Option`/`Result` and using `?` or an early `return`, which often removes the need for labels altogether. (Early `return` from a helper is frequently the cleanest "break out of everything".)

### 5. Reach for iterators first for non-nested logic

As in Pitfall 5: a label on a single loop scanning one collection usually wants to be `.find()`/`.any()`/`.filter()` instead.

---

## Real-World Example

### Connection failover with bounded retries

A common production pattern: try a list of endpoints, and for each, retry a few times before moving on. The moment any attempt succeeds, abandon *all* remaining endpoints and attempts. A labeled `loop` returning a value expresses this cleanly:

```rust playground
#[derive(Debug)]
struct Endpoint {
    name: &'static str,
    // For the demo: the attempt number on which this endpoint starts working.
    // 0 means it never succeeds.
    succeeds_on_attempt: u32,
}

fn try_connect(ep: &Endpoint, attempt: u32) -> Result<String, String> {
    if ep.succeeds_on_attempt != 0 && attempt >= ep.succeeds_on_attempt {
        Ok(format!("connected to {} on attempt {attempt}", ep.name))
    } else {
        Err(format!("{} refused (attempt {attempt})", ep.name))
    }
}

fn main() {
    let endpoints = [
        Endpoint { name: "primary",  succeeds_on_attempt: 0 }, // never succeeds
        Endpoint { name: "replica",  succeeds_on_attempt: 2 }, // works on the 2nd try
        Endpoint { name: "fallback", succeeds_on_attempt: 1 },
    ];
    const MAX_ATTEMPTS: u32 = 3;

    // The whole failover dance is one expression yielding the connection.
    let connection = 'endpoints: loop {
        for ep in &endpoints {
            for attempt in 1..=MAX_ATTEMPTS {
                match try_connect(ep, attempt) {
                    Ok(conn) => break 'endpoints Some(conn), // success: leave everything
                    Err(e) => {
                        eprintln!("warn: {e}");
                        if attempt == MAX_ATTEMPTS {
                            eprintln!("giving up on {}, trying next endpoint", ep.name);
                        }
                    }
                }
            }
        }
        break 'endpoints None; // every endpoint exhausted
    };

    match connection {
        Some(conn) => println!("OK: {conn}"),
        None => println!("ERROR: all endpoints failed"),
    }
}
```

Real output:

```text
warn: primary refused (attempt 1)
warn: primary refused (attempt 2)
warn: primary refused (attempt 3)
giving up on primary, trying next endpoint
warn: replica refused (attempt 1)
OK: connected to replica on attempt 2
```

The `break 'endpoints Some(conn)` punches straight out of the attempt loop *and* the endpoint loop *and* the outer `loop`, delivering the successful connection as `connection` in one move. Without the label this would need a flag checked after each loop level, or the whole thing extracted into a helper that `return`s early.

> **Tip:** In real networking code each retry would be spaced out with a backoff delay and `try_connect` would be `async`. The control-flow shape — labeled loop, break-with-value on success — stays exactly the same; async control flow is covered later in the guide.

---

## Further Reading

### Official Documentation

- [The Rust Book — Loop Labels to Disambiguate Between Multiple Loops](https://doc.rust-lang.org/book/ch03-05-control-flow.html#loop-labels-to-disambiguate-between-multiple-loops)
- [Rust Reference — Loop labels](https://doc.rust-lang.org/reference/expressions/loop-expr.html#loop-labels)
- [Rust Reference — Labelled block expressions](https://doc.rust-lang.org/reference/expressions/loop-expr.html#labelled-block-expressions)
- [MDN — `label` statement (JavaScript)](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/label)

### Related Sections in This Guide

- [Loops](/04-control-flow/01-loops/): `for`, `while`, and `loop`; `break` returning a value from a single `loop`; no C-style `for`.
- [break / continue](/04-control-flow/04-break-continue/): the unlabeled forms these statements build on.
- [match](/04-control-flow/02-match/): exhaustive pattern matching, often pairs with labeled loops in search code.
- [if let / while let](/04-control-flow/03-if-let-while-let/) — concise pattern matching that can replace some labeled-loop bookkeeping.
- [Conditionals](/04-control-flow/00-conditionals/): `if` as an expression; Rust has no truthiness.
- [Section 04 overview](/04-control-flow/)
- Earlier groundwork: [Getting Started](/01-getting-started/), [Basics](/02-basics/), and the [Introduction](/00-introduction/).
- Next up: [Ownership](/05-ownership/) — where the apostrophe shows up again as a *lifetime* (a different concept).

---

## Exercises

### Exercise 1: First common element

**Difficulty:** Beginner

**Objective:** Use a labeled loop to break out of two nested loops at once.

**Instructions:** Implement `first_common`, which returns the first element of `a` that also appears in `b` (scanning `a` in order), or `None` if there is no overlap. Break out of both loops as soon as you find a match.

```rust
fn first_common(a: &[i32], b: &[i32]) -> Option<i32> {
    // TODO: scan `a`; for each element scan `b`; on a match, break out of BOTH loops.
    /* ??? */
}

fn main() {
    println!("{:?}", first_common(&[1, 2, 3, 4], &[9, 8, 3, 1])); // Some(1)
    println!("{:?}", first_common(&[1, 2], &[3, 4]));             // None
}
```

<details>
<summary>Solution</summary>

Because a `for` loop cannot `break` with a value, record the hit in a `mut` variable and then break the labeled loop:

```rust playground
fn first_common(a: &[i32], b: &[i32]) -> Option<i32> {
    let mut result = None;
    'outer: for &x in a {
        for &y in b {
            if x == y {
                result = Some(x);
                break 'outer; // leave both loops; value is returned below
            }
        }
    }
    result
}

fn main() {
    println!("{:?}", first_common(&[1, 2, 3, 4], &[9, 8, 3, 1])); // Some(1)
    println!("{:?}", first_common(&[1, 2], &[3, 4]));             // None
}
```

Real output:

```text
Some(1)
None
```

> **Note:** The answer is `Some(1)`, not `Some(3)`: we scan `a` in order, and `1` (the first element of `a`) is present in `b`, so it matches before we ever reach `3`.

</details>

### Exercise 2: Skip invalid rows with `continue 'label`

**Difficulty:** Intermediate

**Objective:** Use `continue` to a label to abandon a whole outer iteration.

**Instructions:** Implement `sum_valid_rows`. Sum every row of the grid, but if a row contains a `0`, treat the entire row as invalid and exclude it from the total. Use `continue 'rows` so that hitting a `0` immediately skips the rest of that row.

```rust
fn sum_valid_rows(grid: &[[i32; 3]]) -> i32 {
    // TODO: a labeled outer loop; `continue 'rows` when you see a 0.
    /* ??? */
}

fn main() {
    let grid = [
        [1, 2, 3], // valid -> 6
        [4, 0, 6], // has a 0 -> discarded
        [7, 8, 9], // valid -> 24
    ];
    println!("{}", sum_valid_rows(&grid)); // 30
}
```

<details>
<summary>Solution</summary>

```rust playground
fn sum_valid_rows(grid: &[[i32; 3]]) -> i32 {
    let mut total = 0;
    'rows: for row in grid {
        let mut row_sum = 0;
        for &v in row {
            if v == 0 {
                continue 'rows; // discard this row entirely, move to the next
            }
            row_sum += v;
        }
        total += row_sum; // only reached if no 0 was found in the row
    }
    total
}

fn main() {
    let grid = [
        [1, 2, 3],
        [4, 0, 6],
        [7, 8, 9],
    ];
    println!("{}", sum_valid_rows(&grid)); // 30
}
```

Real output:

```text
30
```

The key is that `continue 'rows` skips the `total += row_sum;` line for the discarded row, because it jumps directly to the next iteration of the outer loop.

</details>

### Exercise 3: 3-D search with a value-returning labeled loop

**Difficulty:** Advanced

**Objective:** Combine three nested loops with a labeled `loop` that breaks *with a value*.

**Instructions:** Implement `find_in_cube`, which searches a 2x2x2 cube of integers and returns the `(x, y, z)` coordinates of the first cell equal to `target`, or `None`. Model the search as a labeled `loop` so you can `break 'search Some((x, y, z))` directly with the coordinates.

```rust
fn find_in_cube(cube: &[[[i32; 2]; 2]; 2], target: i32) -> Option<(usize, usize, usize)> {
    // TODO: 'search: loop { for x { for y { for z { ... break 'search Some(...) } } } break 'search None }
    /* ??? */
}

fn main() {
    let cube = [
        [[0, 1], [2, 3]],
        [[4, 5], [6, 7]],
    ];
    println!("{:?}", find_in_cube(&cube, 6));  // Some((1, 1, 0))
    println!("{:?}", find_in_cube(&cube, 99)); // None
}
```

<details>
<summary>Solution</summary>

```rust playground
fn find_in_cube(cube: &[[[i32; 2]; 2]; 2], target: i32) -> Option<(usize, usize, usize)> {
    'search: loop {
        for (x, plane) in cube.iter().enumerate() {
            for (y, row) in plane.iter().enumerate() {
                for (z, &v) in row.iter().enumerate() {
                    if v == target {
                        break 'search Some((x, y, z)); // out of all three loops, with the coords
                    }
                }
            }
        }
        break 'search None; // searched the whole cube, found nothing
    }
}

fn main() {
    let cube = [
        [[0, 1], [2, 3]],
        [[4, 5], [6, 7]],
    ];
    println!("{:?}", find_in_cube(&cube, 6));  // Some((1, 1, 0))
    println!("{:?}", find_in_cube(&cube, 99)); // None
}
```

Real output:

```text
Some((1, 1, 0))
None
```

Wrapping the three `for` loops in a single `'search: loop` gives you one breakable construct that can carry the `Option` straight out of all the nesting: no `mut` flag, no post-loop bookkeeping.

</details>

---

## Summary

**What you've learned:**

- Labels are identifiers with a leading apostrophe (`'rows:`) placed before `loop`, `while`, or `for`.
- `break 'label` exits that loop (and everything nested in it); `continue 'label` skips to its next iteration.
- A labeled **`loop`** can `break 'label value`; JavaScript labels can never produce a value.
- Labeled **blocks** (1.65+) let you `break` out of a plain block with a value.
- The compiler warns on unused labels (`unused_labels`) and rejects misspelled (`E0426`), malformed, and value-breaking-`for` (`E0571`) labels at compile time.

**Key differences from JavaScript:** the apostrophe sigil, type-checked labels (errors not runtime surprises), the unused-label warning, and value-returning labeled `loop`s/blocks.
