---
title: "rust-analyzer: The Rust Language Server"
description: "rust-analyzer is Rust's language server, the tsserver counterpart: completions, inline type hints, code actions, and live diagnostics from its own engine."
---

**rust-analyzer** is the official Language Server Protocol (LSP) implementation for Rust. It is the engine behind autocomplete, go-to-definition, inline type display, refactors, and red squiggles in your editor. If you used the TypeScript Language Service (the brains inside VS Code's TypeScript support), rust-analyzer is its direct counterpart for Rust.

---

## Quick Overview

When you edit `.ts` files, VS Code talks to **tsserver** (the TypeScript Language Service), which gives you completions, hovers, rename, and inline errors. Rust has the exact same architecture: your editor talks to **rust-analyzer** over the Language Server Protocol, and rust-analyzer continuously parses and type-checks your crate in the background.

Why it matters to a TypeScript/JavaScript developer: Rust's compiler is strict, and a tight feedback loop is what makes that strictness pleasant instead of painful. rust-analyzer shows you the inferred type of every `let` binding, surfaces `E0308` type mismatches *as you type* (not on save), and offers one-keystroke code actions to fill in match arms or implement a trait. The current stable toolchain is Rust 1.96.0 on the 2024 edition, and rust-analyzer ships as an official `rustup` component that tracks it.

---

## TypeScript/JavaScript Example

In a TypeScript project, the language service is invisible but always running. You never install it directly; it ships with VS Code (or with the `typescript` package your editor points at). It powers everything that *isn't* the type-checker on the command line:

```typescript
// orders.ts — what tsserver does for you while you type
interface Order {
  id: string;
  total: number;
  items: string[];
}

function summarize(orders: Order[]) {
  // Hover over `expensive` → tsserver shows: const expensive: Order[]
  const expensive = orders.filter((o) => o.total > 100);

  // Type `.` after `expensive` → completion list of Array methods
  // Type `o.` inside the callback → completion list of Order fields
  const ids = expensive.map((o) => o.id);

  // Inlay hints (an opt-in editor setting) render the inferred types inline:
  //   const ids: string[]
  return ids;
}

// Red squiggle appears immediately, before you save or run `tsc`:
const broken: number = "not a number";
//    ^^^^^^ Type 'string' is not assignable to type 'number'. ts(2322)
```

You configure tsserver through `.vscode/settings.json` (and the `typescript.*` / `javascript.*` keys), e.g. turning inlay hints on:

```jsonc
// .vscode/settings.json (TypeScript side)
{
  "typescript.inlayHints.parameterNames.enabled": "all",
  "typescript.inlayHints.variableTypes.enabled": true
}
```

---

## Rust Equivalent

rust-analyzer plays the identical role. You install it once as a `rustup` component (your editor extension usually does this for you), and configure it through the `rust-analyzer.*` keys:

```rust playground
// src/main.rs — what rust-analyzer does for you while you type
use std::collections::HashMap;

fn parse_port(raw: &str) -> Result<u16, std::num::ParseIntError> {
    raw.trim().parse()
}

fn main() {
    // Inlay hint renders `: i32` after `count` (i32 is the default integer).
    let count = 3;
    let names = vec!["alice", "bob", "carol"];

    // Chaining hints reveal the iterator's item type at each `.method()`:
    let upper: Vec<String> = names
        .iter()
        .filter(|n| n.len() > 3)
        .map(|n| n.to_uppercase())
        .collect();

    let mut scores = HashMap::new();
    scores.insert("alice", 10);
    scores.insert("bob", 7);

    // Inlay hint renders `: i32` after `total`.
    let total: i32 = scores.values().sum();

    match parse_port("8080") {
        Ok(port) => println!("port = {port}"),
        Err(e) => println!("bad port: {e}"),
    }

    println!("count={count}, upper={upper:?}, total={total}");
}
```

Running this prints:

```text
port = 8080
count=3, upper=["ALICE", "CAROL"], total=17
```

And you enable the inline type display through `rust-analyzer.*` settings, the direct analog of the `typescript.inlayHints.*` keys:

```jsonc
// .vscode/settings.json (Rust side) — these are ON by default
{
  "rust-analyzer.inlayHints.typeHints.enable": true,
  "rust-analyzer.inlayHints.parameterHints.enable": true,
  "rust-analyzer.inlayHints.chainingHints.enable": true
}
```

> **Note:** Installing rust-analyzer is a one-liner: `rustup component add rust-analyzer`. Most editor extensions (the VS Code "rust-analyzer" extension, the Zed/Neovim LSP clients) download and manage it for you, so you rarely run this by hand. See [Setting Up VS Code for Rust](/24-tooling/06-vscode-setup/) for the full editor walkthrough.

---

## Detailed Explanation

### What an LSP server actually is

The **Language Server Protocol** is a JSON-RPC contract Microsoft designed so that one language implementation can serve *every* editor. tsserver and rust-analyzer both speak it. Your editor (the *client*) sends requests like "what completions are valid at line 12, column 8?" and the *server* answers. This is why the same rust-analyzer binary powers VS Code, Neovim, Helix, Zed, and Emacs identically: the intelligence lives in the server, not the editor.

### rust-analyzer is its own type-checker, not a `cargo check` wrapper

This is the single most important thing to understand, and it is subtler than the TypeScript case. tsserver and `tsc` share the same type-checking core. rust-analyzer, by contrast, contains its **own** parser, name resolver, and trait-solving type inference engine, separate from `rustc`. That is what lets it give you instant feedback on a half-typed expression that `rustc` would refuse to even parse.

You can see rust-analyzer's independent analysis directly. Running its diagnostics engine over a project with a type error produces (real output, abbreviated):

```text
processing crate: probe, module: .../src/main.rs
Error RustcHardError("E0308") from LineCol { line: 1, col: 20 } to LineCol { line: 1, col: 26 }: expected u16, found &'static str

diagnostic scan complete
```

That `expected u16, found &'static str` came from rust-analyzer's own inference, before `cargo` ran. rust-analyzer *also* runs the real compiler in the background (`cargo check` by default) and merges those richer diagnostics in — see the next section.

### `check` on save vs. live diagnostics

rust-analyzer shows you two tiers of diagnostics:

1. **Live, in-memory diagnostics** from its own engine (type mismatches, unresolved names, syntax errors), updated keystroke-by-keystroke.
2. **Full compiler diagnostics** from running `cargo check` (the `rust-analyzer.check.command`, default `"check"`) when you save. These include borrow-checker errors and every Clippy lint if you point it at Clippy.

The settings that control tier 2 are `rust-analyzer.checkOnSave` (default `true`) and `rust-analyzer.check.command` (default `"check"`). A common upgrade is to run Clippy instead of plain check:

```jsonc
// .vscode/settings.json
{
  "rust-analyzer.checkOnSave": true,
  "rust-analyzer.check.command": "clippy"
}
```

> **Warning:** Use the modern `rust-analyzer.check.command` key. The older `rust-analyzer.checkOnSave.command` (a *string* command nested under `checkOnSave`) is deprecated. Today `checkOnSave` is a plain boolean and the command lives in `check.command`. The deprecated form is one of the most common stale-blog-post traps; [Setting Up VS Code for Rust](/24-tooling/06-vscode-setup/) covers it in detail. Wiring Clippy in is covered in [Linting with Clippy](/24-tooling/02-linting/) and [Common Clippy Lints, Explained](/24-tooling/03-clippy-lints/).

### Inlay hints: Rust's killer LSP feature

Because Rust infers most types, the inlay hints rust-analyzer draws are far more valuable than in TypeScript, where you usually wrote the types yourself. Hints are *editor decorations*: they are not part of your file, they never get saved, and they vanish if you open the file in `cat`. The ones enabled by default include:

| Hint kind | Setting | Default | Renders |
| --- | --- | --- | --- |
| Variable types | `inlayHints.typeHints.enable` | `true` | `let x` → `let x: i32` |
| Parameter names | `inlayHints.parameterHints.enable` | `true` | `f(width, height)` → `f(width: w, height: h)` |
| Method chains | `inlayHints.chainingHints.enable` | `true` | type after each `.method()` in a chain |
| Closing braces | `inlayHints.closingBraceHints.enable` | `true` | `} // fn main` on long blocks |
| Lifetime elision | `inlayHints.lifetimeElisionHints.enable` | `"never"` | the elided `'a` lifetimes |
| Binding modes | `inlayHints.bindingModeHints.enable` | `false` | `ref`/`&` inserted by pattern matching |
| Adjustments | `inlayHints.expressionAdjustmentHints.enable` | `"never"` | auto-deref/`.borrow()` the compiler inserts |

The hints that are *off* by default (lifetime elision, binding modes, expression adjustments) are deeper learning aids. Turning them on for a few weeks is one of the fastest ways to internalize ownership and borrowing — concepts covered in [Section 05: Ownership](/05-ownership/). For example:

```jsonc
// .vscode/settings.json — extra hints that teach you the borrow rules
{
  "rust-analyzer.inlayHints.lifetimeElisionHints.enable": "always",
  "rust-analyzer.inlayHints.bindingModeHints.enable": true,
  "rust-analyzer.inlayHints.expressionAdjustmentHints.enable": "always"
}
```

### Code actions (quick fixes / "assists")

Code actions are the lightbulb menu (`Ctrl+.` / `Cmd+.` in VS Code). They are TypeScript's "Quick Fix" and "Refactor" menus by another name. rust-analyzer ships hundreds; the ones you will reach for daily:

- **Add missing match arms** — turns a non-exhaustive `match` into a complete one, generating `Variant => todo!()` for each missing case.
- **Implement missing members** — after `impl SomeTrait for MyType {`, fills in every required method signature.
- **Add `use`** (auto-import) — when you reference `HashMap` with no import, offers to insert `use std::collections::HashMap;`.
- **Extract into function / variable** — select an expression and hoist it.
- **Wrap return type in `Result`** / **Convert to `?`** — restructure error handling (see [Section 08: Error Handling](/08-error-handling/)).
- **Fill struct fields** — expand `Config { .. }` to list every field.

The "fill" actions use `rust-analyzer.assist.expressionFillDefault` (default `"todo"`) to decide whether placeholders are `todo!()` or `Default::default()`.

### Auto-import and import organization

Auto-import (`rust-analyzer.completion.autoimport.enable`, default `true`) is like TypeScript's "auto-import on completion": pick `Duration` from the completion list and rust-analyzer adds `use std::time::Duration;` for you. How it *groups* those imports is governed by `rust-analyzer.imports.granularity.group` (default `"crate"`), which merges imports from the same crate into one `use` block, the rust-analyzer equivalent of an import-sorting ESLint rule.

---

## Key Differences

| Concern | TypeScript (tsserver) | Rust (rust-analyzer) |
| --- | --- | --- |
| Protocol | Language Server Protocol | Language Server Protocol (identical) |
| Shares core with CLI checker? | Yes: tsserver and `tsc` share internals | **No**: rust-analyzer has its own engine, separate from `rustc` |
| Where you get the binary | Bundled with editor / `typescript` package | `rustup component add rust-analyzer` (editor manages it) |
| Inline type display | Nice-to-have (you usually wrote the types) | Essential (most types are inferred) |
| Background compile | Type errors only | Type errors **plus** borrow-check via `cargo check`/`clippy` |
| Config namespace | `typescript.*` / `javascript.*` | `rust-analyzer.*` |
| Macro support | N/A | Expands declarative + procedural macros to resolve names |
| Project model | `tsconfig.json` | `Cargo.toml` (+ `cargo metadata`) |

### The trait-solving difference

tsserver resolves structural types: if two objects have the same shape, they are compatible. rust-analyzer must solve **trait obligations** — "does `Vec<String>` implement `Iterator`?", "is there a `From<Celsius>` impl in scope?". This is why rust-analyzer occasionally pauses on a freshly opened large project: it is priming a cache of trait resolutions (`rust-analyzer.cachePriming.enable`, default `true`). Once primed, completion and hover are instant.

### Macros are part of the language model

Rust macros generate code, so rust-analyzer must expand them to know what names exist. `rust-analyzer.procMacro.enable` (default `true`) lets it compile and run your procedural macros (like `#[derive(Serialize)]` from [Section 15: Serialization](/15-serialization/)). This has no TypeScript analog (TypeScript has no macro system), and it is why a `derive` can give you working completions on generated methods. The "Expand macro recursively" command shows you the generated code; macros themselves are covered in [Section 14: Macros](/14-macros/).

---

## Common Pitfalls

### Pitfall 1: Expecting borrow errors to appear instantly

rust-analyzer's *live* engine catches type errors keystroke-by-keystroke, but full **borrow-checker** errors only arrive after the background `cargo check` finishes (on save by default). New users sometimes write borrow-violating code, see no red squiggle for a second or two, and assume it compiles. It does not — wait for the save-triggered check, or run `cargo check` yourself.

For example, this real type mismatch *does* show up live, courtesy of rust-analyzer's own inference:

```rust
fn main() {
    let port: u16 = "8080"; // does not compile (error[E0308]: mismatched types)
    println!("{port}");
}
```

The real compiler message rust-analyzer surfaces:

```text
error[E0308]: mismatched types
 --> src/main.rs:2:21
  |
2 |     let port: u16 = "8080";
  |               ---   ^^^^^^ expected `u16`, found `&str`
  |               |
  |               expected due to this

For more information about this error, try `rustc --explain E0308`.
```

### Pitfall 2: Trusting inlay hints as if they were source code

Inlay hints are rendered *decorations*, not text in your file. A reader who pastes a screenshot of hinted code into a `.rs` file will get a syntax error, because `let x: i32 = 3` was actually `let x = 3` with a fake `: i32`. VS Code lets you "accept" a hint to materialize it into real code, but until you do, it is purely visual. (TypeScript inlay hints behave the same way; they just matter less there.)

### Pitfall 3: "Add missing match arms" is a fix for a *real* error

When you write a non-exhaustive match, the code action is offered *because* the code does not compile. The underlying error is real:

```rust
enum Status {
    Ok,
    NotFound,
    ServerError,
}

fn label(s: Status) -> &'static str {
    // does not compile (error[E0004]: non-exhaustive patterns)
    match s {
        Status::Ok => "ok",
        Status::NotFound => "not found",
    }
}

fn main() {
    println!("{}", label(Status::Ok));
}
```

Real compiler output:

```text
error[E0004]: non-exhaustive patterns: `Status::ServerError` not covered
 --> src/main.rs:8:11
  |
8 |     match s {
  |           ^ pattern `Status::ServerError` not covered
...
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown
  |
10~         Status::NotFound => "not found",
11~         Status::ServerError => todo!(),
  |
```

The "Add missing match arms" assist inserts exactly that `Status::ServerError => todo!()` arm for you.

### Pitfall 4: Stale analysis after editing `Cargo.toml`

If completions for a freshly added dependency don't appear, rust-analyzer may not have reloaded the workspace. `rust-analyzer.cargo.autoreload` (default `true`) usually handles it, but after editing `Cargo.toml` by hand you can force it with the **"rust-analyzer: Restart server"** command (or **"Reload Workspace"**). This is the analog of restarting tsserver when `tsconfig.json` changes don't take effect.

### Pitfall 5: Missing `rust-src` breaks standard-library navigation

Go-to-definition into `std` (e.g. jumping into `Vec::push`) needs the standard library *source*, which ships in the `rust-src` component. `rustup` installs it for you in most setups, but if `std` hovers show "no definition found," run `rustup component add rust-src`.

---

## Best Practices

- **Let your editor extension manage the binary.** The VS Code "rust-analyzer" extension (and the Neovim/Zed/Helix LSP integrations) keep rust-analyzer in lockstep with your toolchain. Only run `rustup component add rust-analyzer` manually if you are wiring up a bare-bones editor. See [Setting Up VS Code for Rust](/24-tooling/06-vscode-setup/).
- **Point on-save checks at Clippy** once you are comfortable: `"rust-analyzer.check.command": "clippy"` gives you lints inline without a separate run. Details in [Linting with Clippy](/24-tooling/02-linting/).
- **Use the extra inlay hints as a learning curriculum.** Turn on `lifetimeElisionHints`, `bindingModeHints`, and `expressionAdjustmentHints` while learning ownership; turn them off once the rules are second nature.
- **Commit a `.vscode/settings.json`** (or a checked-in editor config) so the whole team gets identical analysis behavior: the equivalent of committing your `tsconfig` and ESLint setup.
- **Learn three keybindings:** Go to Definition (`F12`), Quick Fix / code actions (`Ctrl+.` / `Cmd+.`), and Rename Symbol (`F2`). They cover 80% of daily LSP value.
- **Prefer per-project config for monorepos.** A checked-in `rust-analyzer.toml` (or `.vscode/settings.json`) lets a workspace declare its own features and check command, instead of relying on each developer's global settings.
- **Don't fight cache priming.** The brief startup pause on big projects is rust-analyzer building its trait-resolution cache. Leave `cachePriming.enable` on; it makes everything after startup snappy.

---

## Real-World Example

Here is the kind of code where rust-analyzer earns its keep: a small status-code module with a trait `impl` and an exhaustive `match`. As you write it, rust-analyzer's "Implement missing members" fills the `Display` skeleton, "Add missing match arms" completes the `match`, and inlay hints confirm the inferred types throughout.

```rust playground
use std::fmt;

/// A subset of HTTP statuses for a tiny router.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Status {
    Ok,
    NotFound,
    ServerError,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // rust-analyzer's "Add missing match arms" generated this skeleton,
        // then warned (live) until every arm was covered.
        let text = match self {
            Status::Ok => "200 OK",
            Status::NotFound => "404 Not Found",
            Status::ServerError => "500 Internal Server Error",
        };
        write!(f, "{text}")
    }
}

fn classify(code: u16) -> Status {
    match code {
        200..=299 => Status::Ok,
        404 => Status::NotFound,
        _ => Status::ServerError,
    }
}

fn main() {
    for code in [200, 404, 503] {
        // Inlay hints show `code: u16` and the `Status` returned by classify.
        println!("{code} -> {}", classify(code));
    }
}
```

Compiling and running prints:

```text
200 -> 200 OK
404 -> 404 Not Found
503 -> 500 Internal Server Error
```

The workflow that produced it: you typed `impl fmt::Display for Status {`, pressed `Ctrl+.`, and chose "Implement missing members" to generate the `fn fmt` stub. Inside it you wrote `match self {`, and the live diagnostic flagged the non-exhaustive match (`E0004`) until "Add missing match arms" filled in `Status::ServerError`. Throughout, chaining and type hints confirmed you were holding a `Status`, not a `&Status` or an `Option<Status>`.

### A team-wide configuration

A production repo typically checks in editor settings so analysis is consistent across the team. For VS Code:

```jsonc
// .vscode/settings.json
{
  // Run Clippy on save instead of plain `cargo check`.
  "rust-analyzer.checkOnSave": true,
  "rust-analyzer.check.command": "clippy",

  // Analyze every target (bins, examples, tests, benches), not just the default.
  "rust-analyzer.cargo.allTargets": true,

  // Build with the feature flags the team uses day to day.
  "rust-analyzer.cargo.features": ["postgres", "tracing"],

  // Group auto-imports per crate (one `use` block per crate).
  "rust-analyzer.imports.granularity.group": "crate"
}
```

Editor-independent settings can instead live in a `rust-analyzer.toml` at the workspace root, which the server reads regardless of which editor each developer uses:

```toml
# rust-analyzer.toml
[cargo]
allTargets = true
features = ["postgres", "tracing"]

[check]
command = "clippy"
```

> **Tip:** `rust-analyzer.cargo.allTargets = true` (the default) means completions and diagnostics also cover your `#[cfg(test)]` modules and `examples/`, so you get full IDE support inside tests. See [Section 13: Testing](/13-testing/).

---

## Further Reading

- [rust-analyzer User Manual](https://rust-analyzer.github.io/manual.html) — the authoritative reference for every feature and setting.
- [rust-analyzer Configuration reference](https://rust-analyzer.github.io/manual.html#configuration) — the full list of `rust-analyzer.*` keys with defaults.
- [Language Server Protocol specification](https://microsoft.github.io/language-server-protocol/) — the JSON-RPC contract shared with tsserver.
- [The `rust-analyzer` rustup component](https://rust-lang.github.io/rustup/concepts/components.html) — how it ships with the toolchain.

Related guide sections:

- [Setting Up VS Code for Rust](/24-tooling/06-vscode-setup/): installing the extension and the modern `check.command` setup.
- [Linting with Clippy](/24-tooling/02-linting/) and [Common Clippy Lints, Explained](/24-tooling/03-clippy-lints/): wiring Clippy into on-save checks.
- [Formatting with rustfmt](/24-tooling/01-formatting/) — rustfmt integration (format-on-save) alongside rust-analyzer.
- [Debugging Rust](/24-tooling/04-debugging/): the CodeLLDB flow that pairs with rust-analyzer's "Run"/"Debug" code lenses.
- [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/) — `cargo metadata`, the project model rust-analyzer loads.
- [Section 05: Ownership](/05-ownership/): the concepts the extra inlay hints help you learn.
- [Section 14: Macros](/14-macros/) — what "Expand macro recursively" reveals.
- [Section 25: Advanced Topics](/25-advanced-topics/): where deeper trait and type machinery is covered.

---

## Exercises

### Exercise 1: Turn on the teaching hints

**Difficulty:** Beginner

**Objective:** Build a feel for what rust-analyzer infers by enabling the hints that are off by default.

**Instructions:**

1. Create a project: `cargo new ra_hints && cd ra_hints`.
2. Open it in an editor with rust-analyzer installed.
3. Add a `.vscode/settings.json` (or your editor's LSP config) enabling `inlayHints.expressionAdjustmentHints.enable = "always"` and `inlayHints.bindingModeHints.enable = true`.
4. Write the conversion code below. Observe the type hint that appears after `converted` and the adjustment hints around the `From`/`into` calls.

```rust
#[derive(Debug)]
struct Celsius(f64);

#[derive(Debug)]
struct Fahrenheit(f64);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        /* ??? convert and return a Fahrenheit */
    }
}

fn main() {
    let body = Celsius(37.0);
    let converted = Fahrenheit::from(body);
    let freezing: Fahrenheit = Celsius(0.0).into();
    println!("body -> {:.1} F", converted.0);
    println!("freezing -> {:.1} F", freezing.0);
}
```

<details>
<summary>Solution</summary>

```rust playground
#[derive(Debug)]
struct Celsius(f64);

#[derive(Debug)]
struct Fahrenheit(f64);

// rust-analyzer's "Implement missing members" generates the `fn from` stub
// after you type `impl From<Celsius> for Fahrenheit {`.
impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 * 9.0 / 5.0 + 32.0)
    }
}

fn main() {
    let body = Celsius(37.0);
    // Inlay hint renders `: Fahrenheit` after `converted`.
    let converted = Fahrenheit::from(body);
    // `.into()` works because `From` implies `Into`; the hint shows the target type.
    let freezing: Fahrenheit = Celsius(0.0).into();
    println!("body -> {:.1} F", converted.0);
    println!("freezing -> {:.1} F", freezing.0);
}
```

Running prints:

```text
body -> 98.6 F
freezing -> 32.0 F
```

The settings to add:

```jsonc
// .vscode/settings.json
{
  "rust-analyzer.inlayHints.expressionAdjustmentHints.enable": "always",
  "rust-analyzer.inlayHints.bindingModeHints.enable": true
}
```

</details>

### Exercise 2: Trigger and read a real code action

**Difficulty:** Intermediate

**Objective:** Use the "Add missing match arms" assist on code that genuinely does not compile, and confirm the resulting error went away.

**Instructions:**

1. In a new or existing project, paste the non-exhaustive `match` below into `src/main.rs`.
2. Run `cargo build` and read the real `E0004` error.
3. Place your cursor on the `match`, open the code-action menu (`Ctrl+.` / `Cmd+.`), and apply "Add missing match arms."
4. Fill in the generated `todo!()` arm with a sensible string and confirm `cargo build` now succeeds.

```rust
enum Status {
    Ok,
    NotFound,
    ServerError,
}

fn label(s: Status) -> &'static str {
    match s {
        Status::Ok => "ok",
        Status::NotFound => "not found",
    }
}

fn main() {
    println!("{}", label(Status::Ok));
}
```

<details>
<summary>Solution</summary>

Before the fix, `cargo build` reports (real output):

```text
error[E0004]: non-exhaustive patterns: `Status::ServerError` not covered
 --> src/main.rs:8:11
  |
8 |     match s {
  |           ^ pattern `Status::ServerError` not covered
```

After applying the assist and filling the arm:

```rust playground
enum Status {
    Ok,
    NotFound,
    ServerError,
}

fn label(s: Status) -> &'static str {
    match s {
        Status::Ok => "ok",
        Status::NotFound => "not found",
        Status::ServerError => "server error",
    }
}

fn main() {
    println!("{}", label(Status::Ok));
}
```

Now `cargo build` succeeds and `cargo run` prints `ok`. The lesson: the lightbulb appeared *because* the code was broken — code actions are fixes for real diagnostics, not cosmetic helpers.

</details>

### Exercise 3: Switch on-save checks from `check` to `clippy`

**Difficulty:** Intermediate

**Objective:** Get Clippy lints inline by reconfiguring rust-analyzer's background check command, using the *modern* key.

**Instructions:**

1. Write the word-counting program below — it compiles and runs cleanly.
2. Set `rust-analyzer.check.command` to `"clippy"` and ensure `rust-analyzer.checkOnSave` is `true`.
3. Introduce a small lint-able pattern (for instance, write `counts.len() == 0` somewhere instead of `counts.is_empty()`), save, and watch Clippy's hint appear inline — without running Clippy in a terminal.
4. Confirm you used `rust-analyzer.check.command`, **not** the deprecated `rust-analyzer.checkOnSave.command`.

```rust
use std::collections::BTreeMap;

fn word_counts(text: &str) -> BTreeMap<String, usize> {
    // TODO: count each lowercased word
    /* ??? */
}

fn main() {
    let counts = word_counts("the cat the hat THE end");
    for (word, n) in &counts {
        println!("{word}: {n}");
    }
}
```

<details>
<summary>Solution</summary>

```rust playground
use std::collections::BTreeMap;

fn word_counts(text: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for word in text.split_whitespace() {
        let key = word.to_lowercase();
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn main() {
    let counts = word_counts("the cat the hat THE end");
    for (word, n) in &counts {
        println!("{word}: {n}");
    }
}
```

Running prints (a `BTreeMap` iterates in sorted key order):

```text
cat: 1
end: 1
hat: 1
the: 3
```

The configuration that makes Clippy run on save:

```jsonc
// .vscode/settings.json — the modern, correct keys
{
  "rust-analyzer.checkOnSave": true,
  "rust-analyzer.check.command": "clippy"
}
```

The deprecated form to avoid:

```jsonc
// Deprecated: `checkOnSave.command` is no longer the place for the command.
{
  "rust-analyzer.checkOnSave": { "command": "clippy" }
}
```

With Clippy wired in, writing `counts.len() == 0` triggers the `clippy::len_zero` lint inline, suggesting `counts.is_empty()`. See [Common Clippy Lints, Explained](/24-tooling/03-clippy-lints/) for that and other common lints.

</details>
