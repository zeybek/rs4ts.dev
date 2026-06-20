---
title: "Setting Up VS Code for Rust"
description: "Set up VS Code for Rust with the one rust-analyzer extension: rustfmt format-on-save, Clippy on save, and the modern check.command keys."
---

## Quick Overview

Setting up VS Code for Rust is the moment your editor stops being a text box and starts being an IDE: red squiggles on type errors, autocomplete that understands traits, inline type hints, and one-click formatting on save. For a TypeScript/JavaScript developer the workflow is familiar — install one extension, write a `settings.json` — but the names and a couple of recent config changes are different. This topic gets you from a blank VS Code to a fully wired Rust editor and explains the single most common stale-config trap: the deprecated `rust-analyzer.checkOnSave.command` versus the modern `rust-analyzer.check.command`.

> **Note:** The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects that edition automatically. The official VS Code extension is `rust-lang.rust-analyzer`, which is recommended over and replaces the older, now-deprecated `rust-lang.rust` extension.

---

## TypeScript/JavaScript Example

In a Node.js project, "editor setup" is mostly about telling VS Code which extensions and formatter to use, then committing a workspace settings file so the whole team gets the same experience. The TypeScript language server is bundled with VS Code itself, so you typically install ESLint and Prettier extensions and wire up format-on-save:

```jsonc
// .vscode/extensions.json — recommended extensions for this workspace
{
  "recommendations": [
    "dbaeumer.vscode-eslint",
    "esbenp.prettier-vscode"
  ]
}
```

```jsonc
// .vscode/settings.json — committed workspace settings
{
  "editor.formatOnSave": true,
  "editor.defaultFormatter": "esbenp.prettier-vscode",
  "editor.codeActionsOnSave": {
    "source.fixAll.eslint": "explicit"
  },
  "[typescript]": {
    "editor.defaultFormatter": "esbenp.prettier-vscode"
  },
  "typescript.tsdk": "node_modules/typescript/lib",
  "typescript.preferences.importModuleSpecifier": "non-relative"
}
```

When you open a `.ts` file, the bundled TypeScript server gives you diagnostics and IntelliSense, Prettier formats on save, and ESLint auto-fixes on save. The team shares this by committing the `.vscode/` folder.

---

## Rust Equivalent

Rust's setup mirrors that shape almost exactly — recommend one extension, commit a `settings.json` — but the language intelligence comes from **rust-analyzer**, which you install as a VS Code extension. There is no separate "Prettier extension" and "ESLint extension"; `rustfmt` and Clippy are part of the toolchain, and rust-analyzer drives both.

```jsonc
// .vscode/extensions.json — the one extension you actually need
{
  "recommendations": [
    "rust-lang.rust-analyzer"
  ]
}
```

```jsonc
// .vscode/settings.json — committed workspace settings for Rust
{
  // Format on save, routed through rust-analyzer (which calls rustfmt).
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
  },

  // Run Clippy (not plain `cargo check`) for richer diagnostics on save.
  // MODERN keys (rust-analyzer >= 2023): a boolean toggle + a separate command.
  "rust-analyzer.checkOnSave": true,
  "rust-analyzer.check.command": "clippy",

  // Check every target (bins, tests, benches, examples), not just the default.
  "rust-analyzer.check.allTargets": true,

  // Activate all Cargo features so feature-gated code is analyzed too.
  "rust-analyzer.cargo.features": "all"
}
```

Install the extension from the command line (if you have the `code` CLI) or the Extensions panel:

```bash
# Install rustup + the extension (one-time)
# 1. Install rustup from https://rustup.rs (gives you cargo, rustc, clippy, rustfmt)
rustup component add clippy rustfmt   # usually already present on stable

# 2. Install the editor extension
code --install-extension rust-lang.rust-analyzer
```

Open any `.rs` file and rust-analyzer indexes the workspace, then gives you type-aware completion, inline diagnostics, inlay hints, and format-on-save: the Rust counterpart of the TypeScript/JavaScript setup above.

---

## Detailed Explanation

### One extension, not three

In the Node.js world the language server (TypeScript) is bundled, and you add Prettier and ESLint extensions separately. In Rust, a single extension — **rust-analyzer** — provides the language server, and it *delegates* to the toolchain components:

- **Diagnostics** come from `cargo check` (or `cargo clippy`) run in the background.
- **Formatting** comes from `rustfmt`, invoked when you save.
- **Completion, go-to-definition, inlay hints, refactors** come from rust-analyzer's own analysis.

So the mental mapping is:

| Node.js piece | Rust piece |
| --- | --- |
| Bundled TypeScript language service | `rust-lang.rust-analyzer` extension |
| Prettier extension | `rustfmt` (driven by rust-analyzer on save) |
| ESLint extension | Clippy (driven by rust-analyzer via `check.command`) |

> **Warning:** Do **not** install the old `rust-lang.rust` extension. It is deprecated and conflicts with rust-analyzer. If you used Rust in VS Code years ago, uninstall `rust-lang.rust` first.

### The `editor.defaultFormatter` block

```jsonc
"[rust]": {
  "editor.defaultFormatter": "rust-lang.rust-analyzer",
  "editor.formatOnSave": true
}
```

This is identical in spirit to the TypeScript `"[typescript]": { ... }` block. The `[rust]` language scope says: for Rust files, format with rust-analyzer (which shells out to `rustfmt`) every time you save. Without naming a default formatter, VS Code can prompt "multiple formatters installed" or silently do nothing. See [Formatting with rustfmt](/24-tooling/01-formatting/) for the `rustfmt` side, including `rustfmt.toml`.

### The check-on-save settings — the important part

This is the one place TS/JS muscle memory leads people astray, because the config changed. rust-analyzer runs a Cargo command in the background to produce the red/yellow squiggles. Two separate settings control it:

```jsonc
"rust-analyzer.checkOnSave": true,         // boolean: run a check when I save?
"rust-analyzer.check.command": "clippy"    // string: which command to run
```

- **`rust-analyzer.checkOnSave`** is now a **boolean** (default `true`). It only answers "should I run the check command on save?" Set it to `false` to disable background checking entirely.
- **`rust-analyzer.check.command`** is a **string** (default `"check"`). It chooses *what* to run: `"check"` for plain `cargo check`, or `"clippy"` to get Clippy's richer lints inline as you save.

Older guides and Stack Overflow answers tell you to set **`rust-analyzer.checkOnSave.command`**, a *nested* string. That form is **deprecated**. rust-analyzer still understands it for backward compatibility, but it can emit an "unknown/invalid config" warning, and copy-pasting it next to the new boolean `checkOnSave` produces a confusing config where one key is a boolean and a child key is a string. Use the two modern keys shown above.

> **Tip:** The quickest way to remember it: `checkOnSave` = the on/off switch (boolean); `check.command` = the command (string). The deprecated `checkOnSave.command` jammed both ideas into one nested key.

### Why `"clippy"` instead of `"check"`

Setting `check.command` to `"clippy"` means every save runs `cargo clippy` and surfaces Clippy lints as inline warnings, the closest analogue to ESLint highlighting issues in your editor. With a small program like this:

```rust playground
fn double(x: i32) -> i32 {
    return x * 2;
}

fn main() {
    println!("{}", double(21));
}
```

running `cargo clippy` produces a real, default-on lint that rust-analyzer would draw under the `return`:

```text
warning: unneeded `return` statement
 --> src/main.rs:2:5
  |
2 |     return x * 2;
  |     ^^^^^^^^^^^^
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_return
  = note: `#[warn(clippy::needless_return)]` on by default
help: remove `return`
  |
2 -     return x * 2;
2 +     x * 2
  |
```

With `check.command` left at the default `"check"`, you would only see compiler errors and warnings, not Clippy's idiom lints. The trade-off is speed: Clippy is a touch slower than `cargo check`. See [ESLint to Clippy](/24-tooling/02-linting/) and [Common Clippy lints](/24-tooling/03-clippy-lints/) for the lints themselves.

### `cargo.features` and `check.allTargets`

```jsonc
"rust-analyzer.cargo.features": "all",
"rust-analyzer.check.allTargets": true
```

- **`rust-analyzer.cargo.features`** controls which Cargo features are active during analysis. The default is `[]` (only default features). Setting it to the string `"all"` passes `--all-features`, so feature-gated `#[cfg(feature = "...")]` code is analyzed and gets diagnostics; otherwise that code is invisible to the editor. You can also pass a list like `["postgres", "tls"]`.
- **`rust-analyzer.check.allTargets`** (default `true`) checks binaries, tests, benches, and examples, not just the library/default target, so a type error inside a `#[test]` shows up without you running the tests.

> **Note:** Background checking compiles your crate, so the first check after opening a project (or after touching `Cargo.toml`) can take a while and uses the same `target/` directory as `cargo build`. This is normal: rust-analyzer is not a second, separate compiler; it reuses the toolchain.

### Workspace vs. user settings

VS Code resolves settings in layers, exactly as in a Node.js project:

- **User settings** (`~/.config/Code/User/settings.json` or via *Preferences: Open User Settings (JSON)*) apply to every project.
- **Workspace settings** (`.vscode/settings.json` in the repo) override user settings for that project and are committed so the whole team shares them.

Put project-specific choices (`cargo.features`, a custom `check.command`) in the committed workspace file; keep personal preferences (theme, inlay-hint verbosity) in your user settings.

---

## Key Differences

| Aspect | VS Code for TypeScript/JavaScript | VS Code for Rust |
| --- | --- | --- |
| Language server | Bundled TypeScript service | `rust-lang.rust-analyzer` extension (install it) |
| Formatter | Prettier extension | `rustfmt` via rust-analyzer (no separate extension) |
| Linter in editor | ESLint extension | Clippy via `rust-analyzer.check.command` |
| Format-on-save key | `"[typescript]": { editor.formatOnSave }` | `"[rust]": { editor.formatOnSave }` |
| Lint-on-save | ESLint runs as you type | `rust-analyzer.checkOnSave` (boolean) |
| "which linter" | ESLint config / plugins | `rust-analyzer.check.command` (`"check"` / `"clippy"`) |
| Cost of diagnostics | Lightweight (in-memory TS server) | Compiles the crate (`cargo check`/`clippy`) |
| Deprecated trap | — | `checkOnSave.command` (nested string) |
| Required toolchain | Node + `typescript` dep | rustup (cargo, rustc, clippy, rustfmt) |

The conceptual takeaway: VS Code for Rust is *more* integrated (one extension covers what three do in Node.js) but its diagnostics are *heavier*, because they come from actually compiling your code rather than a lightweight in-memory analyzer. That is why the `checkOnSave` toggle exists at all: there is a real cost to turning it on.

---

## Common Pitfalls

### Pitfall 1: Using the deprecated `checkOnSave.command`

This is the headline trap. You find an old blog post and paste:

```jsonc
// deprecated form — works but warns and confuses
"rust-analyzer.checkOnSave.command": "clippy"
```

Modern rust-analyzer treats `checkOnSave` as a boolean and `check.command` as the command, so the nested `checkOnSave.command` can trigger an *invalid configuration* notification in VS Code and is no longer the documented way. The fix is the two-key form:

```jsonc
// modern form
"rust-analyzer.checkOnSave": true,
"rust-analyzer.check.command": "clippy"
```

### Pitfall 2: Installing the old `rust-lang.rust` extension

Searching the marketplace for "rust" surfaces the legacy `rust-lang.rust` extension. It is deprecated and, if installed alongside rust-analyzer, fights over diagnostics and formatting. Install only `rust-lang.rust-analyzer`, and uninstall the old one if present.

### Pitfall 3: Expecting it to work without rustup

The extension is a *client*; it needs the rust-analyzer language server binary and the toolchain. If `rustc` is not on `PATH`, rust-analyzer shows an error like "rust-analyzer failed to discover workspace" or "can't find Cargo.toml / rustc". Install [rustup](https://rustup.rs) first (see [Installation](/01-getting-started/01-installation/)); the extension downloads the matching server binary automatically.

### Pitfall 4: Forgetting the `[rust]` language scope on format-on-save

Setting a global `"editor.formatOnSave": true` works, but if you also have other formatters installed, VS Code may not know which one to use for `.rs` files and silently skips formatting. Always scope the default formatter:

```jsonc
"[rust]": {
  "editor.defaultFormatter": "rust-lang.rust-analyzer",
  "editor.formatOnSave": true
}
```

### Pitfall 5: Missing feature-gated code

By default rust-analyzer only sees code behind *default* features. If half your crate is under `#[cfg(feature = "server")]`, that half gets no diagnostics or completion until you set `rust-analyzer.cargo.features` to `"all"` or list the features you develop against. Symptom: a function you can see in the file shows no type hints and "go to definition" fails inside it.

### Pitfall 6: A type error appears, then vanishes — and you didn't save

rust-analyzer's *typing* diagnostics update live, but the **`cargo check`/`clippy` diagnostics only run on save** (that is exactly what `checkOnSave` controls). If you expect the red squiggle from a borrow-check error to appear as you type, it will not. Save the file (or disable/re-enable as needed). This surprises TypeScript developers used to ESLint reacting on every keystroke.

---

## Best Practices

- **Commit `.vscode/extensions.json` and `.vscode/settings.json`.** Recommend `rust-lang.rust-analyzer` and pin the team's `check.command`, `formatOnSave`, and `cargo.features` so everyone gets identical diagnostics, the same reason you commit `.vscode/` for a Node.js repo.
- **Use the modern two-key check config.** `rust-analyzer.checkOnSave: true` + `rust-analyzer.check.command: "clippy"`. Never the deprecated `checkOnSave.command`.
- **Prefer Clippy for `check.command` on app code,** plain `"check"` if Clippy's extra latency bothers you on a very large workspace. You can always run `cargo clippy` manually in the terminal regardless.
- **Scope format-on-save to `[rust]`** with `rust-lang.rust-analyzer` as the default formatter, so it never collides with another installed formatter.
- **Set `cargo.features` to match how you build.** `"all"` is the simplest correct default; list specific features if `--all-features` does not compile (mutually exclusive features).
- **Keep personal vs. team settings separate.** Inlay-hint verbosity and theme belong in user settings; toolchain behavior belongs in committed workspace settings.

> **Tip:** rust-analyzer exposes hundreds of settings. You rarely need most of them. Start with the five keys in the Rust Equivalent section and add more only when you have a concrete reason. What the server can do is covered in [rust-analyzer](/24-tooling/05-rust-analyzer/).

---

## Real-World Example

A production Rust repository ships a `.vscode/` folder so a new teammate gets a working IDE the moment they open the project, the same convention as a well-run Node.js monorepo. Here is a complete, realistic setup.

### `.vscode/extensions.json`

```jsonc
// .vscode/extensions.json
{
  "recommendations": [
    "rust-lang.rust-analyzer",   // the language server
    "vadimcn.vscode-lldb",       // debugging (CodeLLDB) — see ./debugging.md
    "tamasfe.even-better-toml",  // Cargo.toml / *.toml editing
    "fill-labs.dependi"          // inline crate version info & updates
  ],
  "unwantedRecommendations": [
    "rust-lang.rust"             // the deprecated extension — flag it as unwanted
  ]
}
```

### `.vscode/settings.json`

```jsonc
// .vscode/settings.json
{
  // --- Formatting -----------------------------------------------------------
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
  },

  // --- Diagnostics on save --------------------------------------------------
  "rust-analyzer.checkOnSave": true,
  "rust-analyzer.check.command": "clippy",
  "rust-analyzer.check.allTargets": true,

  // --- Cargo / analysis scope ----------------------------------------------
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.procMacro.enable": true,

  // --- Inlay hints (tune to taste) -----------------------------------------
  "rust-analyzer.inlayHints.typeHints.enable": true,
  "rust-analyzer.inlayHints.parameterHints.enable": true,
  "rust-analyzer.inlayHints.chainingHints.enable": true,

  // --- Quality-of-life -------------------------------------------------------
  "editor.semanticHighlighting.enabled": true,
  "files.watcherExclude": {
    "**/target/**": true
  }
}
```

`procMacro.enable` (default `true`) lets rust-analyzer expand procedural macros like `#[derive(Serialize)]` so derived items get completion and go-to-definition; important for any crate using serde, thiserror, or similar. The `files.watcherExclude` entry keeps VS Code from watching the large `target/` directory.

### What this buys you, on real code

Open this program in the configured editor:

```rust playground
use std::collections::HashMap;

#[derive(Debug)]
struct Order {
    id: u32,
    customer: String,
    total: f64,
}

fn totals_by_customer(orders: &[Order]) -> HashMap<String, f64> {
    let mut totals = HashMap::new();
    for order in orders {
        *totals.entry(order.customer.clone()).or_insert(0.0) += order.total;
    }
    totals
}

fn main() {
    let orders = vec![
        Order { id: 1, customer: "alice".to_string(), total: 12.5 },
        Order { id: 2, customer: "bob".to_string(), total: 3.0 },
        Order { id: 3, customer: "alice".to_string(), total: 7.25 },
    ];
    let totals = totals_by_customer(&orders);
    let mut names: Vec<_> = totals.keys().collect();
    names.sort();
    for name in names {
        println!("{name}: {:.2}", totals[name]);
    }
}
```

Running it confirms the program is correct:

```text
alice: 19.75
bob: 3.00
```

But the editor also surfaces something `cargo run`'s success hides. Because nothing reads `Order.id`, the compiler emits a real, default-on warning that rust-analyzer underlines on the `id` field:

```text
warning: field `id` is never read
 --> src/main.rs:5:5
  |
4 | struct Order {
  |        ----- field in this struct
5 |     id: u32,
  |     ^^
  |
  = note: `Order` has a derived impl for the trait `Debug`, but this is intentionally ignored during dead code analysis
  = note: `#[warn(dead_code)]` on by default
```

You see that yellow squiggle the instant you save; no terminal round-trip. With `check.command` set to `"clippy"`, Clippy's idiom lints (like the `needless_return` shown earlier) appear the same way. That live feedback, plus type inlay hints over `let totals` and `let mut names`, is the whole reason to invest five minutes in the setup.

> **Note:** The inlay hints (the faint `: HashMap<String, f64>` after `let totals`, `: Vec<&String>` after `names`) are virtual text drawn by rust-analyzer; they are not in the file and never get committed. Toggle them with the `inlayHints.*` keys above. Details in [rust-analyzer](/24-tooling/05-rust-analyzer/).

---

## Further Reading

- [rust-analyzer User Manual](https://rust-analyzer.github.io/book/) — the authoritative reference for every config key, including `check.command` and `checkOnSave`.
- [rust-analyzer configuration reference](https://rust-analyzer.github.io/book/configuration.html) — searchable list of `rust-analyzer.*` settings and their defaults.
- [The rust-analyzer VS Code extension](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) — install page; note it replaces `rust-lang.rust`.
- [VS Code settings.json documentation](https://code.visualstudio.com/docs/getstarted/settings) — user vs. workspace settings and language scopes.
- [rust-analyzer](/24-tooling/05-rust-analyzer/) — what the language server actually gives you: inlay hints, code actions, and its full config surface.
- [Debugging Rust](/24-tooling/04-debugging/) — the CodeLLDB flow that pairs with this editor setup.
- [Formatting with rustfmt](/24-tooling/01-formatting/) and [ESLint to Clippy](/24-tooling/02-linting/) — the formatter and linter that rust-analyzer drives.
- [Common Clippy lints](/24-tooling/03-clippy-lints/) — the lints you will see inline once `check.command` is `"clippy"`.
- [Cargo deep dive](/24-tooling/00-cargo-deep-dive/) — features, workspaces, and profiles that `cargo.features` and `check.allTargets` interact with.
- Foundational background: [Installation](/01-getting-started/01-installation/), [Understanding Cargo](/01-getting-started/03-cargo-basics/), [Getting Started](/01-getting-started/), and [Rust Basics](/02-basics/).
- Continue to [Advanced Topics](/25-advanced-topics/) once your editor is dialed in.

---

## Exercises

### Exercise 1: Wire up format-on-save and verify it

**Difficulty:** Easy

**Objective:** Confirm that saving a Rust file reformats it through rust-analyzer.

**Instructions:**

1. Create a project: `cargo new vscode_practice && cd vscode_practice && code .`.
2. Install the `rust-lang.rust-analyzer` extension if you have not already.
3. Add a `.vscode/settings.json` that scopes `editor.formatOnSave` to `[rust]` with rust-analyzer as the default formatter.
4. Paste deliberately ugly code into `src/main.rs` (no spaces, everything on one line) and save. It should snap into `rustfmt` style.

<details>
<summary>Solution</summary>

`.vscode/settings.json`:

```jsonc
{
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
  }
}
```

Paste this into `src/main.rs` and save:

```rust playground
fn main(){let nums=vec![3,1,2];let mut s=nums.clone();s.sort();println!("{:?}",s);}
```

On save it becomes:

```rust playground
fn main() {
    let nums = vec![3, 1, 2];
    let mut s = nums.clone();
    s.sort();
    println!("{:?}", s);
}
```

If nothing happens, you either skipped the `[rust]` scope or the extension is not installed. See [Formatting with rustfmt](/24-tooling/01-formatting/) for the `rustfmt` side.

</details>

### Exercise 2: Switch from `cargo check` to Clippy in the editor

**Difficulty:** Medium

**Objective:** See the difference between the default `check` command and Clippy as the on-save check, using the modern config keys.

**Instructions:**

1. In your project, write a function with an idiom Clippy dislikes, e.g. a needless `return`.
2. With default settings (no `check.command`), save and note that no Clippy lint appears, only compiler diagnostics.
3. Add `rust-analyzer.checkOnSave: true` and `rust-analyzer.check.command: "clippy"` to `.vscode/settings.json`, reload the window, save again, and watch the Clippy warning appear inline.
4. Confirm the same lint by running `cargo clippy` in the terminal.

<details>
<summary>Solution</summary>

`src/main.rs`:

```rust playground
fn double(x: i32) -> i32 {
    return x * 2;
}

fn main() {
    println!("{}", double(21));
}
```

`.vscode/settings.json`:

```jsonc
{
  "rust-analyzer.checkOnSave": true,
  "rust-analyzer.check.command": "clippy"
}
```

After reloading the window (Command Palette → *Developer: Reload Window*) and saving, rust-analyzer underlines the `return`. The same lint from `cargo clippy` in the terminal is:

```text
warning: unneeded `return` statement
 --> src/main.rs:2:5
  |
2 |     return x * 2;
  |     ^^^^^^^^^^^^
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_return
  = note: `#[warn(clippy::needless_return)]` on by default
help: remove `return`
  |
2 -     return x * 2;
2 +     x * 2
  |
```

The key insight: `checkOnSave` is the boolean on/off switch and `check.command` is the command string. The deprecated `checkOnSave.command` would also "work" but is the wrong, warned-about form.

</details>

### Exercise 3: Make feature-gated code visible to the editor

**Difficulty:** Medium

**Objective:** Observe that rust-analyzer ignores code behind a non-default feature until you enable it via `cargo.features`.

**Instructions:**

1. In `Cargo.toml`, declare a feature: add a `[features]` section with `extra = []`.
2. In `src/main.rs`, put a function behind `#[cfg(feature = "extra")]` that contains an obvious type error (e.g. `let x: i32 = "nope";`).
3. With default settings, note that rust-analyzer shows *no* diagnostic for that function; it is not in the active feature set.
4. Set `rust-analyzer.cargo.features` to `"all"` (or `["extra"]`), reload the window, and confirm the type error now appears.

<details>
<summary>Solution</summary>

`Cargo.toml` (excerpt):

```toml
[features]
extra = []
```

`src/main.rs`:

```rust playground
#[cfg(feature = "extra")]
fn experimental() {
    let _x: i32 = "nope"; // does not compile (error[E0308]: mismatched types) — only when `extra` is active
}

fn main() {
    println!("hello");
}
```

`.vscode/settings.json`:

```jsonc
{
  "rust-analyzer.cargo.features": "all"
}
```

With the default `cargo.features` of `[]`, the `extra` feature is off, so `experimental` is excluded from analysis and you see no error: exactly the "function shows no diagnostics" symptom from Pitfall 5. After setting `cargo.features` to `"all"` and reloading, rust-analyzer analyzes the cfg'd code and underlines `"nope"` with the mismatched-types error. You can verify the same error from the terminal with `cargo check --features extra`. See [Cargo deep dive](/24-tooling/00-cargo-deep-dive/) for more on features.

</details>
