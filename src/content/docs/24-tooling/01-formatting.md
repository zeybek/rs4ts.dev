---
title: "Formatting with rustfmt"
description: "rustfmt is Rust's Prettier: built into the toolchain, one shared style, run via cargo fmt. Covers rustfmt.toml and the stable-vs-nightly options gotcha."
---

## Quick Overview

`rustfmt` is Rust's official code formatter, the equivalent of Prettier in the Node.js world. It is built into the toolchain (no install step, no plugin to configure), it has an opinionated default style that the entire community shares, and it integrates with Cargo as `cargo fmt`. For a TypeScript/JavaScript developer, the big mental shift is that there is *one* canonical Rust style and almost nobody argues about it, so most teams configure very little and simply run the formatter.

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition. `rustfmt` ships with every stable toolchain via `rustup component add rustfmt` (installed by default), so `cargo fmt` works out of the box.

---

## TypeScript/JavaScript Example

In a Node.js project, formatting is a separate tool you add, configure, and wire into your editor and CI. A typical setup pulls in Prettier as a dev dependency, adds a config file, and exposes npm scripts:

```jsonc
// package.json (excerpt)
{
  "scripts": {
    "format": "prettier --write .",
    "format:check": "prettier --check ."
  },
  "devDependencies": {
    "prettier": "^3.4.2"
  }
}
```

```jsonc
// .prettierrc.json
{
  "semi": true,
  "singleQuote": false,
  "printWidth": 80,
  "tabWidth": 2,
  "trailingComma": "all"
}
```

Given this unformatted source:

```typescript
// sample.ts (before formatting)
const order={id:1,items:['pen','ink'],total:12.5}
function summarize(orders:{id:number,total:number}[]){return orders.reduce((acc,o)=>{acc[o.id]=o.total;return acc},{} as Record<number,number>)}
```

Running `npx prettier --check sample.ts` reports the file as unformatted and exits non-zero:

```text
Checking formatting...
[warn] sample.ts
[warn] Code style issues found in the above file. Run Prettier with --write to fix.
```

Running `npx prettier --write sample.ts` rewrites it:

```typescript
// sample.ts (after prettier --write)
const order = { id: 1, items: ["pen", "ink"], total: 12.5 };
function summarize(orders: { id: number; total: number }[]) {
  return orders.reduce(
    (acc, o) => {
      acc[o.id] = o.total;
      return acc;
    },
    {} as Record<number, number>,
  );
}
```

This is the workflow you already know: install, configure, format-on-save, and a `--check` gate in CI.

---

## Rust Equivalent

Rust gives you the same three capabilities — write, check, and editor integration — but the tool is already installed and the config is optional. Here is unformatted Rust analogous to the TypeScript above:

```rust playground
// src/main.rs (before formatting)
use std::collections::HashMap;
#[derive(Debug)]
struct Order{id:u32,items:Vec<String>,total:f64}
fn summarize(orders:&[Order])->HashMap<u32,f64>{
let mut totals=HashMap::new();
for order in orders{totals.insert(order.id,order.total);}
totals
}
fn main(){
let orders=vec![Order{id:1,items:vec!["pen".to_string(),"ink".to_string()],total:12.5},Order{id:2,items:vec!["pad".to_string()],total:3.0}];
let totals=summarize(&orders);
println!("{:?}",totals);
}
```

Run the formatter from the crate root:

```bash
cargo fmt          # rewrite files in place  (≈ prettier --write)
cargo fmt --check  # fail if anything is unformatted  (≈ prettier --check)
```

After `cargo fmt`, the file becomes:

```rust playground
// src/main.rs (after cargo fmt)
use std::collections::HashMap;
#[derive(Debug)]
struct Order {
    id: u32,
    items: Vec<String>,
    total: f64,
}
fn summarize(orders: &[Order]) -> HashMap<u32, f64> {
    let mut totals = HashMap::new();
    for order in orders {
        totals.insert(order.id, order.total);
    }
    totals
}
fn main() {
    let orders = vec![
        Order {
            id: 1,
            items: vec!["pen".to_string(), "ink".to_string()],
            total: 12.5,
        },
        Order {
            id: 2,
            items: vec!["pad".to_string()],
            total: 3.0,
        },
    ];
    let totals = summarize(&orders);
    println!("{:?}", totals);
}
```

Notice that `rustfmt` expands the inline `Order { ... }` literals onto multiple lines and indents the loop body. It does the same kind of structural reflow Prettier does, driven by line width and nesting.

---

## Detailed Explanation

`cargo fmt` is a thin wrapper that finds every `.rs` file reachable from your crate's targets and runs the `rustfmt` binary on each. The two everyday invocations are:

- **`cargo fmt`**: formats and *writes* files in place. Use it locally, often bound to format-on-save.
- **`cargo fmt --check`**: formats in memory, prints a unified diff of what *would* change, and **exits with a non-zero status if any file is not already formatted**. It never writes. This is the CI gate.

Run `cargo fmt --check` on the unformatted source above and you get a real diff plus a failing exit code:

```text
Diff in /tmp/.../src/main.rs:1:
 use std::collections::HashMap;
 #[derive(Debug)]
-struct Order{id:u32,items:Vec<String>,total:f64}
-fn summarize(orders:&[Order])->HashMap<u32,f64>{
-let mut totals=HashMap::new();
-for order in orders{totals.insert(order.id,order.total);}
-totals
+struct Order {
+    id: u32,
+    items: Vec<String>,
+    total: f64,
 }
...
```

The shell exit code is `1`. After running `cargo fmt`, re-running `cargo fmt --check` prints nothing and exits `0`. That pass/fail pair is exactly what your CI relies on.

### What `rustfmt` decides for you

Unlike Prettier, where `printWidth`, `tabWidth`, quotes, and semicolons are all routine knobs, `rustfmt` makes most of these choices non-negotiable on the stable channel:

- **4-space indentation, spaces not tabs** (Prettier defaults to 2 spaces).
- **`max_width = 100`** by default (Prettier's `printWidth` defaults to 80).
- **Trailing commas** in multi-line literals, always.
- **Imports get sorted** within a `use` group automatically.

You can adjust a handful of these via `rustfmt.toml`, but the surface area is intentionally small.

### The `rustfmt.toml` config file

Place a `rustfmt.toml` (or `.rustfmt.toml`) at your crate or workspace root. It is the analogue of `.prettierrc`. Every key is a snake_case option; here is a realistic, **stable-only** configuration:

```toml
# rustfmt.toml
edition = "2024"               # match your crate's edition so idioms format correctly
max_width = 100                # the default; shown for clarity (Prettier printWidth)
hard_tabs = false              # spaces, not tabs (the default)
newline_style = "Unix"         # LF line endings, like Prettier's "lf"
use_small_heuristics = "Default"
```

> **Tip:** Set `edition` in `rustfmt.toml` so `rustfmt` parses edition-specific syntax and applies edition-aware idioms. `cargo fmt` already passes your crate's edition through, but a standalone `rustfmt file.rs` invocation does not. See Common Pitfalls.

### Stable vs. nightly options

This is the single most important `rustfmt` gotcha, and it has no Prettier analogue. Many attractive options — grouping `std`/external/local imports, merging imports by crate, wrapping comments, formatting code in doc comments — are **unstable** and only take effect on the **nightly** toolchain. If you put them in `rustfmt.toml` and run stable `cargo fmt`, they are silently ignored *with a warning* rather than applied:

```toml
# rustfmt.toml — these two keys are unstable
group_imports = "StdExternalCrate"
imports_granularity = "Crate"
```

On stable, `cargo fmt` prints:

```text
Warning: can't set `imports_granularity = Crate`, unstable features are only available in nightly channel.
Warning: can't set `group_imports = StdExternalCrate`, unstable features are only available in nightly channel.
```

The command still exits `0` and formats everything else, but the import grouping you wanted does *not* happen. To actually apply those options you must run the nightly formatter (`cargo +nightly fmt`), which many teams standardize on purely for formatting while building on stable.

---

## Key Differences

| Aspect | Prettier (Node.js) | rustfmt (Rust) |
| --- | --- | --- |
| Installation | `npm i -D prettier`; pick a version | Ships with the toolchain (`rustup component`) |
| Config file | `.prettierrc` (JSON/JS/YAML/TOML) | `rustfmt.toml` (TOML, snake_case keys) |
| Config philosophy | Many common knobs are routinely tuned | Tiny stable surface; community shares one style |
| Default line width | `printWidth` 80 | `max_width` 100 |
| Default indent | 2 spaces | 4 spaces |
| Write command | `prettier --write` | `cargo fmt` |
| Check command | `prettier --check` (exit 1 on diff) | `cargo fmt --check` (exit 1 on diff) |
| Per-file ignore | `// prettier-ignore` | `#[rustfmt::skip]` attribute |
| Ignore whole paths | `.prettierignore` | `ignore = [...]` in `rustfmt.toml` |
| Advanced options | All available everywhere | Many gated behind the **nightly** channel |
| Multiple languages | TS, JS, CSS, JSON, MD, ... | Rust only |

The conceptual takeaway: in Node.js the team debates Prettier settings; in Rust the team almost never does. The default style *is* the convention, and reviewers expect every file to be `cargo fmt`-clean. Spending your config budget on a few keys (`edition`, maybe `max_width`) is normal; rewriting the whole style is not.

---

## Common Pitfalls

### Pitfall 1: Expecting unstable options to work on stable

A TypeScript developer assumes that if a key is documented, setting it works. With `rustfmt`, options like `imports_granularity`, `group_imports`, `wrap_comments`, and `format_code_in_doc_comments` are nightly-only. On stable they emit the `Warning: can't set ... unstable features are only available in nightly channel.` message shown above and are skipped. The fix is either to drop those keys or to format with `cargo +nightly fmt`. Nothing errors; your code just isn't grouped the way you expected, which is easy to miss in a review.

### Pitfall 2: Running `rustfmt` directly without an edition

`cargo fmt` knows your crate's edition; the bare `rustfmt` binary defaults to an older edition and can misformat or reject edition-2024 syntax. Always prefer `cargo fmt`. If you must call `rustfmt` directly (for example in a pre-commit hook over a single file), pass the edition:

```bash
rustfmt --check --edition 2024 src/main.rs
```

Run against `fn  main( ){println!("hi") ;}`, this prints a real diff and exits `1`:

```text
Diff in /tmp/bad.rs:1:
-fn  main( ){println!("hi") ;}
+fn main() {
+    println!("hi");
+}
```

### Pitfall 3: Forgetting `--all` in a workspace

`cargo fmt` on its own formats the current package. In a multi-crate workspace, a member you didn't touch can still be unformatted and fail CI. Use `cargo fmt --all` to format (or check) every package in the workspace:

```bash
cargo fmt --all --check
```

### Pitfall 4: Reaching for `#[rustfmt::skip]` too often

`rustfmt` will reflow a hand-aligned matrix or table into something less readable. The correct escape hatch is the `#[rustfmt::skip]` attribute on the item, not disabling the formatter project-wide. It preserves your exact layout, and `cargo fmt --check` will then treat the skipped block as already-clean:

```rust playground
#[rustfmt::skip]
const MATRIX: [[i32; 3]; 3] = [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
];

fn main() {
    println!("{}", MATRIX[1][1]);
}
```

After `cargo fmt`, the matrix layout is preserved verbatim and `cargo fmt --check` exits `0`. Use this sparingly — it is the equivalent of scattering `// prettier-ignore` everywhere, and overusing it defeats the point of a shared style.

### Pitfall 5: Confusing `rustfmt` with Clippy

Prettier formats; ESLint lints. The same split exists in Rust: `rustfmt` only reformats whitespace and structure and never changes program meaning, while **Clippy** catches logic-level lint issues (needless clones, non-idiomatic patterns). They are separate tools and separate CI gates. See [ESLint to Clippy](/24-tooling/02-linting/) for the linting half.

---

## Best Practices

- **Run `cargo fmt` before every commit.** Most teams enforce it with format-on-save plus a CI `--check` gate, so unformatted code never lands.
- **Keep `rustfmt.toml` minimal.** Setting `edition` is worthwhile; beyond that, only override when you have a real reason. Resist porting your full Prettier config. The Rust default is the team's shared baseline.
- **Pin the channel you format with.** If you rely on nightly-only options, document `cargo +nightly fmt` and run that exact command in CI so local and CI output match. Otherwise stick to stable.
- **Format the whole workspace.** Prefer `cargo fmt --all` (and `cargo fmt --all --check` in CI) so no member crate slips through.
- **Use `#[rustfmt::skip]` for genuinely tabular data** (lookup tables, opcode maps, aligned test fixtures), not as a way to opt out of formatting normal code.
- **Add a `fmt` step early in CI.** It is the fastest check you have; failing fast on formatting keeps the rest of the pipeline focused on real problems.

> **Tip:** Editor integration is the habit that pays off most. In VS Code, enable `editor.formatOnSave` with `rust-analyzer` as the Rust formatter so files are `cargo fmt`-clean the moment you save. See [VS Code setup](/24-tooling/06-vscode-setup/) and [rust-analyzer](/24-tooling/05-rust-analyzer/).

---

## Real-World Example

A production project typically combines four things: a `rustfmt.toml`, format-on-save in the editor, an optional pre-commit hook, and a CI gate. Here is the editor and CI half.

### Format-on-save (VS Code)

Add this to the workspace `.vscode/settings.json`. It routes Rust files through `rust-analyzer`, which calls `cargo fmt` under the hood, and formats on every save:

```jsonc
// .vscode/settings.json
{
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
  }
}
```

### The CI formatting gate (GitHub Actions)

A minimal, self-contained job that fails the build if anything is unformatted. Because `cargo fmt --check` exits non-zero on the first unformatted file, the step fails the job automatically:

```yaml
# .github/workflows/ci.yml (formatting job)
name: ci
on: [push, pull_request]

jobs:
  fmt:
    name: rustfmt
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - name: Check formatting
        run: cargo fmt --all --check
```

This is intentionally just the formatting gate; the [CI/CD concepts](/24-tooling/07-ci-cd/) and [GitHub Actions](/24-tooling/08-github-actions/) topics show how it fits alongside the Clippy, test, and build jobs (and how to cache the `target` directory).

### Editor-agnostic check via `rustfmt --emit stdout`

Some tooling (custom hooks, non-VS-Code editors) prefers to read formatted output rather than rewrite the file. `rustfmt --emit stdout` prints the formatted source without touching disk:

```bash
rustfmt --emit stdout --edition 2024 src/main.rs
```

For the input `fn  main( ){let x=1;println!("{}",x);}` this prints the cleaned-up version and exits `0`:

```text
src/main.rs:

fn main() {
    let x = 1;
    println!("{}", x);
}
```

---

## Further Reading

- [The rustfmt repository and configuration reference](https://github.com/rust-lang/rustfmt): every option, with stable/nightly status.
- [Configuration docs (rust-lang.github.io/rustfmt)](https://rust-lang.github.io/rustfmt/): searchable list of `rustfmt.toml` keys.
- [The Cargo Book: `cargo fmt`](https://doc.rust-lang.org/cargo/commands/cargo-fmt.html) — the Cargo wrapper's flags.
- [ESLint to Clippy](/24-tooling/02-linting/): the linting half of the formatter/linter split.
- [Common Clippy lints](/24-tooling/03-clippy-lints/) — lints with before/after examples (the `uninlined_format_args` lint pairs well with formatting).
- [VS Code setup](/24-tooling/06-vscode-setup/) and [rust-analyzer](/24-tooling/05-rust-analyzer/): wiring up format-on-save.
- [CI/CD concepts](/24-tooling/07-ci-cd/) and [GitHub Actions](/24-tooling/08-github-actions/) — the `fmt --check` gate in a full pipeline.
- [Cargo deep dive](/24-tooling/00-cargo-deep-dive/): workspace mechanics behind `cargo fmt --all`.
- Foundational background: [Understanding Cargo](/01-getting-started/03-cargo-basics/), [Getting Started](/01-getting-started/), and [Rust Basics](/02-basics/).
- Continue to [Advanced Topics](/25-advanced-topics/) once you have your toolchain dialed in.

---

## Exercises

### Exercise 1: Make a file `cargo fmt`-clean

**Difficulty:** Easy

**Objective:** Build the muscle memory of the write/check loop.

**Instructions:**

1. Create a new project: `cargo new fmt_practice && cd fmt_practice`.
2. Replace `src/main.rs` with deliberately ugly code (no spaces around operators, everything on one line).
3. Run `cargo fmt --check` and observe the diff and the non-zero exit code (`echo $?`).
4. Run `cargo fmt`, then `cargo fmt --check` again and confirm it now exits `0`.

<details>
<summary>Solution</summary>

Paste this ugly source into `src/main.rs`:

```rust playground
fn main(){let nums=vec![3,1,2];let mut sorted=nums.clone();sorted.sort();println!("{:?} -> {:?}",nums,sorted);}
```

`cargo fmt --check` prints a diff and `echo $?` shows `1`. After `cargo fmt`, the file becomes:

```rust playground
fn main() {
    let nums = vec![3, 1, 2];
    let mut sorted = nums.clone();
    sorted.sort();
    println!("{:?} -> {:?}", nums, sorted);
}
```

Now `cargo fmt --check` prints nothing and `echo $?` shows `0`.

</details>

### Exercise 2: Tune `rustfmt.toml` and observe the effect

**Difficulty:** Medium

**Objective:** See how a stable config key changes output, and learn which keys are nightly-only.

**Instructions:**

1. In a project, add a function with several chained method calls or a long struct literal that sits near 100 columns.
2. Add `rustfmt.toml` with `max_width = 60` and run `cargo fmt`. Note how much earlier `rustfmt` breaks lines.
3. Now add `imports_granularity = "Crate"` to the same file, add a few `use` lines from the same crate, and run `cargo fmt` on stable. What happens?

<details>
<summary>Solution</summary>

Lowering `max_width` to `60` forces `rustfmt` to wrap earlier: a long single-line `vec![...]` or chained call that fit at 100 columns now splits across multiple lines. This is the same idea as lowering Prettier's `printWidth`.

Adding `imports_granularity = "Crate"` on the **stable** channel does *not* merge your imports. `cargo fmt` prints:

```text
Warning: can't set `imports_granularity = Crate`, unstable features are only available in nightly channel.
```

and exits `0` without grouping anything. To actually merge `use a::b;` and `use a::c;` into `use a::{b, c};`, run `cargo +nightly fmt`. This is the stable-vs-nightly distinction from the Detailed Explanation in action.

</details>

### Exercise 3: Protect a lookup table with `#[rustfmt::skip]`

**Difficulty:** Medium

**Objective:** Use the per-item escape hatch correctly and verify the formatter respects it.

**Instructions:**

1. Add a hand-aligned 2D array (a small matrix or a color palette table) to your project, formatted exactly the way you want it visually.
2. Run `cargo fmt` and watch `rustfmt` collapse or re-indent your nice alignment.
3. Add `#[rustfmt::skip]` directly above the item, run `cargo fmt` again, and confirm your layout survives and `cargo fmt --check` exits `0`.

<details>
<summary>Solution</summary>

```rust playground
#[rustfmt::skip]
const PALETTE: [[u8; 3]; 4] = [
    [255,   0,   0], // red
    [  0, 255,   0], // green
    [  0,   0, 255], // blue
    [255, 255, 255], // white
];

fn main() {
    // PALETTE[2] is blue.
    println!("{:?}", PALETTE[2]);
}
```

Without the attribute, `rustfmt` strips the column alignment inside each row. With `#[rustfmt::skip]` on the `const`, the exact spacing is preserved, `cargo fmt` leaves it untouched, and `cargo fmt --check` exits `0`. Reserve this for genuinely tabular data — overusing it is the Rust version of sprinkling `// prettier-ignore` everywhere.

</details>
