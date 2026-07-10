---
title: "Code Coverage and Faster Test Runs"
description: "There is no cargo test --coverage flag: install cargo-llvm-cov for LLVM source-based coverage and cargo-nextest for faster, process-per-test runs, like Vitest."
---

In the JavaScript world you reach for `jest --coverage` or `vitest --coverage` to see which lines your tests actually exercised, and you might lean on tools like Turborepo to keep test runs fast. Rust has direct equivalents: **`cargo-llvm-cov`** measures coverage using the compiler's own instrumentation, and **`cargo-nextest`** is a drop-in faster test runner. Neither ships in the box, but both install with a single `cargo install`.

---

## Quick Overview

**Code coverage** answers the question "which lines of my code ran during the test suite?" It is a way to find untested branches, not a proof of correctness. In Rust the de-facto tool is `cargo-llvm-cov`, a `cargo` subcommand that drives LLVM's source-based coverage (the same engine Clang uses for C++), so the numbers are precise rather than estimated. Separately, **`cargo-nextest`** is a next-generation test *runner* that compiles your tests the same way `cargo test` does but executes them in a smarter, faster, process-per-test model with cleaner output. The two compose: `cargo llvm-cov nextest` collects coverage *while* running under nextest.

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. The commands below work on any recent stable toolchain.

---

## TypeScript/JavaScript Example

With Vitest, coverage is a flag plus a tiny bit of config. Given a library and its tests:

```typescript
// src/temperature.ts
export function celsiusToFahrenheit(c: number): number {
  return (c * 9) / 5 + 32;
}

export function classify(c: number): string {
  if (c < 0) return "freezing";
  if (c < 15) return "cold";
  if (c < 30) return "mild";
  return "hot";
}
```

```typescript
// src/temperature.test.ts
import { describe, it, expect } from "vitest";
import { celsiusToFahrenheit, classify } from "./temperature";

describe("temperature", () => {
  it("converts boiling point", () => {
    expect(celsiusToFahrenheit(100)).toBe(212);
  });

  it("classifies the middle bands", () => {
    expect(classify(5)).toBe("cold");
    expect(classify(20)).toBe("mild");
  });
});
```

You enable coverage in `vitest.config.ts` and run it:

```typescript
// vitest.config.ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    coverage: {
      provider: "v8", // V8's built-in coverage
      reporter: ["text", "html", "lcov"],
    },
  },
});
```

```bash
npx vitest run --coverage
```

Vitest prints a per-file table and writes an HTML report you open in a browser. The numbers come from V8's runtime coverage counters (or, with `provider: "istanbul"`, from source instrumentation). Notice that the two tests above never hit the `"freezing"` or `"hot"` branches; a coverage report makes that gap visible.

Things a JavaScript developer takes for granted here:

- Coverage is a **flag on the test runner**, configured in a project file.
- You pick a **provider** (`v8` or `istanbul`) and a set of **reporters**.
- The output includes `lcov.info`, which Codecov, Coveralls, and editors understand.

Every one of those maps cleanly onto a Rust tool.

---

## Rust Equivalent

`cargo test` itself has no `--coverage` flag. Instead you install a subcommand once:

```bash
# One-time install. cargo has had `add`/`install` built in since 1.62 —
# no cargo-edit or extra tooling needed.
cargo install cargo-llvm-cov
```

`cargo-llvm-cov` needs the LLVM tools that ship with the toolchain. On a `rustup`-managed install it adds them automatically; if you manage Rust another way, run `rustup component add llvm-tools-preview` once.

Here is the same library and test suite in Rust:

```rust
// src/lib.rs
//! Temperature conversion and classification.

/// Converts Celsius to Fahrenheit.
pub fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

/// Converts Fahrenheit to Celsius.
pub fn fahrenheit_to_celsius(f: f64) -> f64 {
    (f - 32.0) * 5.0 / 9.0
}

/// Classifies a Celsius temperature into a human-readable band.
pub fn classify(c: f64) -> &'static str {
    if c < 0.0 {
        "freezing"
    } else if c < 15.0 {
        "cold"
    } else if c < 30.0 {
        "mild"
    } else {
        "hot"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boiling_point_converts() {
        assert_eq!(celsius_to_fahrenheit(100.0), 212.0);
    }

    #[test]
    fn freezing_point_converts_back() {
        assert_eq!(fahrenheit_to_celsius(32.0), 0.0);
    }

    #[test]
    fn classifies_cold_and_mild() {
        assert_eq!(classify(5.0), "cold");
        assert_eq!(classify(20.0), "mild");
    }
}
```

Run the coverage report with one command:

```bash
cargo llvm-cov
```

Real output (trimmed to the table; the test run is printed first, then the summary):

```text
running 3 tests
test tests::boiling_point_converts ... ok
test tests::freezing_point_converts_back ... ok
test tests::classifies_cold_and_mild ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

Filename                  Regions  Missed Regions  Cover  Functions  Missed Functions  Executed  Lines  Missed Lines  Cover  Branches  Missed Branches  Cover
------------------------------------------------------------------------------------------------------------------------------------------------------------
src/lib.rs                     29               2 93.10%          6                 0  100.00%     25             2 92.00%         0                0      -
------------------------------------------------------------------------------------------------------------------------------------------------------------
TOTAL                          29               2 93.10%          6                 0  100.00%     25             2 92.00%         0                0      -
```

> **Note:** The real column header is wider than shown; it has been compressed here to fit the page. The numbers are exact output from `cargo llvm-cov`.

The report shows **92.00% line coverage** with two missed lines: exactly the `"freezing"` and `"hot"` branches the tests never reach, mirroring the Vitest example.

---

## Detailed Explanation

### What `cargo llvm-cov` actually does

Under the hood, `cargo llvm-cov`:

1. Recompiles your crate and tests with `-C instrument-coverage`, the same source-based coverage that Clang uses for C and C++. This inserts counters into the generated machine code.
2. Runs the test binaries, which write raw `.profraw` counter files to disk.
3. Merges those into a `.profdata` file with `llvm-profdata`.
4. Renders a report with `llvm-cov` (the table you saw, or HTML/LCOV/JSON).

You do not run any of those steps by hand; the subcommand orchestrates them. This is **source-based** coverage: it knows about every region, function, line, and branch the compiler emitted, so the counts are precise. That is closer to Vitest's `istanbul` provider than to the sampling-flavored `v8` provider.

### Reading the columns

The table has several coverage dimensions, each more granular than the last:

- **Functions** — did every function get called at least once? Here all 6 functions ran (the three test functions plus the three public functions they call), so 100%.
- **Lines** — what fraction of source lines executed. 92% here: 23 of 25 ran.
- **Regions** — LLVM "regions" are sub-line spans (the two arms of an `if`, for example). This is the most precise metric and the one to watch. 93.10% here.
- **Branches** — true/false outcomes of conditions. It shows `-` because branch coverage is off by default; enable it with `--branch`.

> **Tip:** Prefer **region** coverage over line coverage as your headline number. A single line like `if a { x } else { y }` is "covered" the moment it executes once, even if `else` never ran, but region coverage will mark the untaken arm as missed.

### Finding the untested lines

The table tells you *how much* is uncovered; `--show-missing-lines` tells you *where*:

```bash
cargo llvm-cov --show-missing-lines
```

Real output appends this after the table:

```text
Uncovered Lines:
src/lib.rs: 16, 22
```

Lines 16 and 22 are the `"freezing"` and `"hot"` string literals: the two branches no test exercised. That is the actionable signal: write a test that passes `-5.0` and `35.0` to `classify`.

### Generating an HTML report

For a clickable, line-by-line view like Vitest's HTML reporter:

```bash
cargo llvm-cov --html        # writes target/llvm-cov/html/index.html
cargo llvm-cov --open        # same, and opens it in your browser
```

Real tail of the `--html` run:

```text
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s


    Finished report saved to /.../target/llvm-cov/html
```

### Generating machine-readable reports for CI

For uploading to Codecov, Coveralls, or an editor gutter plugin, emit LCOV, the same format Vitest's `lcov` reporter produces:

```bash
cargo llvm-cov --lcov --output-path lcov.info
```

The first lines of the resulting `lcov.info` are standard LCOV records:

```text
SF:/.../src/lib.rs
FN:4,_RNvCs..._21celsius_to_fahrenheit
FN:9,_RNvCs..._21fahrenheit_to_celsius
FN:14,_RNvCs..._8classify
...
FNF:6
FNH:6
```

`cargo-llvm-cov` can also emit `--cobertura`, `--json`, and `--codecov` formats. Because the output is plain LCOV, the same Codecov GitHub Action you might already use for a TypeScript repo works unchanged.

### `cargo-nextest`: the faster runner

`cargo test` runs all tests in one binary, on a thread pool, sharing one process. `cargo-nextest` instead runs **each test in its own process**, which gives it cleaner isolation, a tidy live progress UI, and — on large suites — a real speed win. Install it once:

```bash
cargo install cargo-nextest --locked
```

Then run your suite:

```bash
cargo nextest run
```

Real output:

```text
────────────
 Nextest run ID 79bddfe4-64ef-428c-a75a-96856227ac64 with nextest profile: default
    Starting 3 tests across 1 binary
        PASS [   0.024s] (1/3) temperature_lib tests::classifies_cold_and_mild
        PASS [   0.026s] (2/3) temperature_lib tests::boiling_point_converts
        PASS [   0.030s] (3/3) temperature_lib tests::freezing_point_converts_back
────────────
     Summary [   0.031s] 3 tests run: 3 passed, 0 skipped
```

Each test reports its own wall-clock time, and the per-test process model means one test that segfaults or calls `std::process::abort()` cannot take the others down with it; `cargo test` would lose the whole binary.

### Coverage *under* nextest

The two tools compose. To collect coverage while running under nextest's runner:

```bash
cargo llvm-cov nextest
```

Real output (nextest run, then the same coverage table):

```text
────────────
 Nextest run ID 87e23b61-a061-4b3f-88e4-4082637a21cb with nextest profile: default
    Starting 3 tests across 1 binary
        PASS [   0.010s] (1/3) temperature_lib tests::boiling_point_converts
        PASS [   0.010s] (2/3) temperature_lib tests::freezing_point_converts_back
        PASS [   0.012s] (3/3) temperature_lib tests::classifies_cold_and_mild
────────────
     Summary [   0.012s] 3 tests run: 3 passed, 0 skipped
Filename                  Regions  Missed Regions  Cover  ...  Lines  Missed Lines  Cover  ...
src/lib.rs                     29               2 93.10%  ...     25             2 92.00%  ...
TOTAL                          29               2 93.10%  ...     25             2 92.00%  ...
```

> **Warning:** Nextest does **not** run doc tests; it only sees `#[test]` functions in unit and integration binaries. `cargo test` runs doc tests too. If you rely on [doc tests](/13-testing/09-doc-tests/) for coverage or correctness, run them separately with `cargo test --doc` (or `cargo llvm-cov --doctests`, which is a nightly-only feature at the time of writing). This is the single most common surprise when switching runners.

---

## Key Differences

| Concept | Jest / Vitest | Rust |
| --- | --- | --- |
| Coverage entry point | `--coverage` flag on the runner | separate `cargo llvm-cov` subcommand |
| Engine | V8 counters or Istanbul instrumentation | LLVM source-based instrumentation |
| Configuration | `coverage` block in config file | command-line flags (or `[env]`/CI scripts) |
| Reporters | `text`, `html`, `lcov`, ... | `--summary-only`, `--html`, `--lcov`, `--cobertura`, `--json`, `--codecov` |
| Faster runner | parallel workers (built in) | `cargo-nextest` (separate install) |
| Test isolation | one process, many workers | nextest: **one process per test** |
| Doc tests | n/a (no such concept) | run by `cargo test`, **skipped by nextest** |
| CI gate | `coverage.thresholds` in config | `--fail-under-lines`, `--fail-under-regions`, ... |

Two differences deserve emphasis.

**Coverage is a separate tool, not a runner flag.** There is no `cargo test --coverage`. This feels like a missing battery at first, but it keeps the core toolchain small and lets the coverage tooling evolve independently. The install is a one-liner and only needed once per machine.

**Source-based, not sampled.** `cargo-llvm-cov` instruments the actual compiled code, so its region counts are exact: there is no statistical estimation as with V8's sampling mode. The trade-off is that a coverage build recompiles your crate with instrumentation, so it is slower than a plain `cargo test` and uses a separate `target/llvm-cov-target` directory (your normal `cargo build` cache is untouched).

---

## Common Pitfalls

### Pitfall 1: Expecting `cargo test --coverage` to exist

Coming from `jest --coverage`, the instinct is to pass a flag:

```bash
cargo test --coverage
```

Real error:

```text
error: unexpected argument '--coverage' found

  tip: to pass '--coverage' as a value, use '-- --coverage'

Usage: cargo test [OPTIONS] [TESTNAME] [-- [ARGS]...]
```

There is no such flag. Coverage lives in the `cargo llvm-cov` subcommand you install separately.

### Pitfall 2: Forgetting the LLVM tools component

On a non-`rustup` toolchain (or a stripped CI image), `cargo llvm-cov` fails because `llvm-profdata`/`llvm-cov` are missing. The fix is a one-time component install:

```bash
rustup component add llvm-tools-preview
```

With `rustup`, `cargo install cargo-llvm-cov` typically pulls this in for you, so you only hit this on minimal CI images. Add the line to your CI setup step to be safe.

### Pitfall 3: Comparing coverage-build timings to normal builds

A coverage run recompiles everything with instrumentation into `target/llvm-cov-target`, so the *first* `cargo llvm-cov` after a normal `cargo build` looks alarmingly slow. It is not your tests getting slower; it is a separate, instrumented build. Subsequent coverage runs reuse that cache. Keep coverage as its own CI job rather than gating every `cargo test`.

### Pitfall 4: Assuming nextest runs your doc tests

If your suite relies on doc tests and you switch CI from `cargo test` to `cargo nextest run`, those tests silently stop running; nextest reports green while skipping them entirely. Run doc tests explicitly:

```bash
cargo nextest run     # unit + integration tests
cargo test --doc      # doc tests, which nextest does not execute
```

### Pitfall 5: Nextest filter syntax is not identical to `cargo test`

A positional substring works like `cargo test`:

```bash
cargo nextest run classifies   # runs every test whose name contains "classifies"
```

Real output:

```text
────────────
 Nextest run ID 964d01ee-2b3c-4d94-af2e-fd02f4a6b8b4 with nextest profile: default
    Starting 2 tests across 1 binary (2 tests skipped)
        PASS [   0.009s] (1/2) temperature_lib edge_tests::classifies_freezing_and_hot
        PASS [   0.009s] (2/2) temperature_lib tests::classifies_cold_and_mild
────────────
     Summary [   0.014s] 2 tests run: 2 passed, 2 skipped
```

For anything more precise, nextest has its own **filterset** DSL behind `-E`, which is far more expressive than `cargo test`'s substring match; for example `-E 'test(/classif/)'` runs tests matching a regex:

```text
    Starting 2 tests across 1 binary (2 tests skipped)
        PASS [   0.010s] (1/2) temperature_lib tests::classifies_cold_and_mild
        PASS [   0.010s] (2/2) temperature_lib edge_tests::classifies_freezing_and_hot
     Summary [   0.012s] 2 tests run: 2 passed, 2 skipped
```

Do not assume every `cargo test -- <flag>` argument has the same meaning under nextest; consult `cargo nextest run --help`.

---

## Best Practices

### Treat coverage as a flashlight, not a finish line

A high coverage number means "these lines executed," not "these lines are correct." It is excellent at surfacing *untested* code (an `else` branch nobody hit, an error path nobody triggered) and useless as a measure of test quality. Use it to ask "why did no test reach this branch?", and consider [property testing](/13-testing/07-property-testing/) for branches that are hard to cover with hand-written examples.

### Pick region coverage as your CI gate

When you want CI to fail on a coverage regression, gate on region (or line) coverage with `--fail-under-*`:

```bash
cargo llvm-cov --summary-only --fail-under-lines 90
```

That run exits `0` because the suite is at 92% lines. Tighten the bar to a level the suite does not meet:

```bash
cargo llvm-cov --summary-only --fail-under-lines 95
```

The table prints, then the command exits with status `1`, failing the CI step:

```text
src/lib.rs                     29               2 93.10%  ...     25             2 92.00%  ...
TOTAL                          29               2 93.10%  ...     25             2 92.00%  ...
```

```bash
echo $?   # => 1
```

Set the threshold a little *below* current coverage so it catches regressions without blocking unrelated PRs over rounding noise.

### Exclude generated or untestable code

Mark code you intentionally do not test (generated bindings, `unreachable!()` arms, debug-only helpers) so it does not drag your number down. The standard attribute is recognized by `cargo-llvm-cov`:

```rust
#[coverage(off)]
fn debug_dump() {
    println!("internal state");
}
```

> **Note:** `#[coverage(off)]` is itself stabilizing; on stable today the portable approach is the `// cov-ignore`-style comments or per-file `--ignore-filename-regex` flag. Check `cargo llvm-cov --help` for the exact mechanism on your toolchain.

You can also drop whole paths with a regex:

```bash
cargo llvm-cov --ignore-filename-regex '(tests|benches)/'
```

### Use nextest locally and in CI for large suites

For a tiny crate the runners are equally fast, but as a workspace grows, nextest's parallel process model and its ability to **retry flaky tests** (`--retries N`) and **partition** tests across CI machines (`--partition count:1/3`) pay off. A common setup: `cargo nextest run` for the fast feedback loop and `cargo test --doc` alongside it for the doc tests nextest skips.

### Generate LCOV in CI and let a service track trends

Emit `lcov.info` and upload it to Codecov/Coveralls so coverage trends are tracked over time and shown on PRs, the same workflow you would use for a TypeScript repo:

```bash
cargo llvm-cov --lcov --output-path lcov.info
# then upload lcov.info with the Codecov action, exactly as in a JS repo
```

---

## Real-World Example

A production CI pipeline typically: runs tests fast (nextest), separately collects coverage, gates on a threshold, and uploads an LCOV report. Here is the library extended with full coverage so the suite reaches 100%, plus the commands a GitHub Actions job would run.

```rust
// src/lib.rs
//! Temperature conversion and classification.

/// Converts Celsius to Fahrenheit.
pub fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

/// Converts Fahrenheit to Celsius.
pub fn fahrenheit_to_celsius(f: f64) -> f64 {
    (f - 32.0) * 5.0 / 9.0
}

/// Classifies a Celsius temperature into a human-readable band.
pub fn classify(c: f64) -> &'static str {
    if c < 0.0 {
        "freezing"
    } else if c < 15.0 {
        "cold"
    } else if c < 30.0 {
        "mild"
    } else {
        "hot"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boiling_point_converts() {
        assert_eq!(celsius_to_fahrenheit(100.0), 212.0);
    }

    #[test]
    fn freezing_point_converts_back() {
        assert_eq!(fahrenheit_to_celsius(32.0), 0.0);
    }

    #[test]
    fn classifies_cold_and_mild() {
        assert_eq!(classify(5.0), "cold");
        assert_eq!(classify(20.0), "mild");
    }

    // The branches the coverage report flagged as missed: -5.0 and 35.0.
    #[test]
    fn classifies_freezing_and_hot() {
        assert_eq!(classify(-5.0), "freezing");
        assert_eq!(classify(35.0), "hot");
    }
}
```

With the two edge-case branches now exercised, `cargo llvm-cov --summary-only` reports a clean sheet:

```text
Filename                  Regions  Missed Regions   Cover  Functions  Missed Functions  Executed  Lines  Missed Lines   Cover  ...
-----------------------------------------------------------------------------------------------------------------------------------
src/lib.rs                     35               0 100.00%          7                 0  100.00%     29             0 100.00%  ...
-----------------------------------------------------------------------------------------------------------------------------------
TOTAL                          35               0 100.00%          7                 0  100.00%     29             0 100.00%  ...
```

Region, function, and line coverage are all 100%. A matching CI workflow:

```yaml
# .github/workflows/ci.yml
name: CI
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview

      # Fast test feedback (skips doc tests; run them separately below).
      - uses: taiki-e/install-action@cargo-nextest
      - run: cargo nextest run
      - run: cargo test --doc

      # Coverage, gated, then uploaded.
      - uses: taiki-e/install-action@cargo-llvm-cov
      - run: cargo llvm-cov --lcov --output-path lcov.info --fail-under-lines 90
      - uses: codecov/codecov-action@v5
        with:
          files: lcov.info
```

This pipeline gives the same guarantees a `vitest run --coverage` + Codecov setup would in a TypeScript project: fast tests, a coverage gate that fails the build on a regression, and a trend-tracking report on every PR.

### What a failing run looks like

Nextest's failure reporting is more structured than `cargo test`'s. If `classifies_cold_and_mild` were broken so that `classify(5.0)` was expected to be `"warm"`, `cargo nextest run` prints:

```text
────────────
 Nextest run ID 3c8b89eb-c99b-43af-a462-5353a50fa212 with nextest profile: default
    Starting 1 test across 1 binary (3 tests skipped)
        FAIL [   0.010s] (1/1) temperature_lib tests::classifies_cold_and_mild
  stdout ───

    running 1 test
    test tests::classifies_cold_and_mild ... FAILED

    failures:
        tests::classifies_cold_and_mild

    test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s

  stderr ───

    thread 'tests::classifies_cold_and_mild' panicked at src/lib.rs:42:9:
    assertion `left == right` failed
      left: "cold"
     right: "warm"
    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

  Cancelling due to test failure:
────────────
     Summary [   0.011s] 1 test run: 0 passed, 1 failed, 3 skipped
        FAIL [   0.010s] (1/1) temperature_lib tests::classifies_cold_and_mild
error: test run failed
```

Nextest groups each failure's captured stdout and stderr under clear headers and repeats the failing test name in the summary, and the process exits non-zero, so CI catches it. The `left`/`right` labels come from `assert_eq!` (see [Assertions](/13-testing/02-assertions/)).

---

## Further Reading

- [`cargo-llvm-cov` README](https://github.com/taiki-e/cargo-llvm-cov): full flag reference and CI recipes.
- [`cargo-nextest` book](https://nexte.st/): the runner's docs, including the filterset DSL and CI partitioning.
- [Rustc Book — Instrumentation-based Code Coverage](https://doc.rust-lang.org/rustc/instrument-coverage.html): the compiler feature underneath.
- [The Rust Book — Writing Automated Tests](https://doc.rust-lang.org/book/ch11-00-testing.html): the testing chapter that frames all of this.
- Sibling topics in this section:
  - [Unit Tests](/13-testing/00-unit-tests/): `#[test]` and `#[cfg(test)] mod tests`, the input to any coverage run.
  - [Integration Tests](/13-testing/04-integration-tests/): the `tests/` directory; coverage spans these too.
  - [Doc Tests](/13-testing/09-doc-tests/) — run by `cargo test` but **skipped by nextest**.
  - [Assertions](/13-testing/02-assertions/) — where the `left`/`right` failure output comes from.
  - [Property Testing](/13-testing/07-property-testing/) — a way to cover branches example tests miss.
  - [Benchmarking](/13-testing/08-benchmarking/), [Mocking](/13-testing/06-mocking/), [Test Fixtures](/13-testing/05-test-fixtures/), [Test Organization](/13-testing/01-test-organization/), [`#[should_panic]` and `Result` tests](/13-testing/03-should-panic/), [TDD Workflow](/13-testing/11-tdd-workflow/).
- Foundations used above:
  - [Cargo Basics](/01-getting-started/03-cargo-basics/) — `cargo install` and subcommands.
  - [Installation](/01-getting-started/01-installation/) — `rustup` and toolchain components.
  - [Modules and Packages](/12-modules-packages/) — how `#[cfg(test)]` modules fit into a crate.
  - [Macros](/14-macros/) — `#[test]` and `assert_eq!` are macros/attributes; this explains the machinery.

---

## Exercises

### Exercise 1: Install the tools and read your first report

**Difficulty:** Easy

**Objective:** Get a coverage number on a real crate.

**Instructions:** Create a new library with `cargo new --lib temps`, paste the `temperature` module and its first three tests from the "Rust Equivalent" section above (the version *without* `classifies_freezing_and_hot`). Install `cargo-llvm-cov` if you have not, run `cargo llvm-cov`, and read off the line-coverage percentage. Then run `cargo llvm-cov --show-missing-lines` and identify which two source lines are uncovered.

<details>
<summary>Solution</summary>

```bash
cargo install cargo-llvm-cov     # one-time
cargo new --lib temps
# paste the module + the three-test `mod tests` into src/lib.rs
cargo llvm-cov
cargo llvm-cov --show-missing-lines
```

`cargo llvm-cov` reports **92.00%** line coverage. `--show-missing-lines` ends with:

```text
Uncovered Lines:
src/lib.rs: 16, 22
```

Those are the `"freezing"` (line 16) and `"hot"` (line 22) arms of `classify`: the two branches no test reaches.

</details>

### Exercise 2: Close the gap and gate CI

**Difficulty:** Medium

**Objective:** Reach 100% region coverage and add a threshold that would fail CI on a regression.

**Instructions:** Add a test that exercises the two missing branches of `classify` (pass `-5.0` and `35.0`). Confirm `cargo llvm-cov` now reports 100%. Then run `cargo llvm-cov --summary-only --fail-under-lines 90` and check the exit code with `echo $?`. Finally, raise the bar to `--fail-under-lines 101` and confirm the command now exits non-zero.

<details>
<summary>Solution</summary>

```rust
// add inside `mod tests`
#[test]
fn classifies_freezing_and_hot() {
    assert_eq!(classify(-5.0), "freezing");
    assert_eq!(classify(35.0), "hot");
}
```

```bash
cargo llvm-cov --summary-only
# TOTAL ... 100.00% ... 100.00% ...

cargo llvm-cov --summary-only --fail-under-lines 90
echo $?     # => 0  (100% >= 90%)

cargo llvm-cov --summary-only --fail-under-lines 101
echo $?     # => 1  (no suite can reach 101%, so the gate fails)
```

The threshold flag makes coverage a hard CI gate: pick a number just under your current coverage so genuine regressions fail the build.

</details>

### Exercise 3: Switch the runner to nextest and keep doc tests

**Difficulty:** Medium

**Objective:** Replace `cargo test` with `cargo nextest run` without silently dropping doc tests.

**Instructions:** Add a documentation example to `celsius_to_fahrenheit` (a ` ```rust ` block in its `///` doc comment that asserts `celsius_to_fahrenheit(0.0) == 32.0`). Install `cargo-nextest`. Run `cargo nextest run` and observe that the doc test does **not** appear in the output. Then run the doc tests explicitly and confirm the example passes. Explain in one sentence why a CI job needs both commands.

<details>
<summary>Solution</summary>

```rust
/// Converts Celsius to Fahrenheit.
///
/// ```
/// use temps::celsius_to_fahrenheit;
/// assert_eq!(celsius_to_fahrenheit(0.0), 32.0);
/// ```
pub fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}
```

```bash
cargo install cargo-nextest --locked   # one-time

cargo nextest run        # runs unit/integration tests only — no doc test listed
cargo test --doc         # runs the doc test
```

`cargo test --doc` runs the example as a test, naming it after the file and the
line the code block starts on (the exact line number depends on your layout):

```text
running 1 test
test src/lib.rs - celsius_to_fahrenheit (line N) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

A CI job needs both because **nextest does not execute doc tests**: running only `cargo nextest run` would leave that documented example unverified, so a separate `cargo test --doc` step keeps the docs honest. See [Doc Tests](/13-testing/09-doc-tests/).

</details>
