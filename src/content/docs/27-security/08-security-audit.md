---
title: "Auditing Dependencies and Supply-Chain Hygiene"
description: "Audit your Rust dependency tree with cargo audit and cargo deny, the RUSTSEC-backed answer to npm audit, plus license, duplicate, and supply-chain policy."
---

Modern applications are mostly other people's code. A typical Rust service pulls in dozens to hundreds of transitive crates, and a typical Node service pulls in *thousands* of transitive packages. Any one of them can ship a known vulnerability, an incompatible license, or, in the worst case, a malicious update. This topic covers how to audit your Rust dependency tree with `cargo audit` and `cargo deny`, and the supply-chain habits that keep that tree trustworthy.

---

## Quick Overview

**Dependency auditing** means continuously checking every crate you depend on (direct *and* transitive) against a database of known security advisories, and enforcing policy on licenses, duplicate versions, and where crates come from. In the Rust world the security advisory database is **RUSTSEC** (maintained by the [RustSec project](https://rustsec.org/)), the audit tool is **`cargo audit`**, and the broader policy tool is **`cargo deny`**. If you have used `npm audit` you already understand the goal; the Rust tools are a close analog with a few important differences.

> **Note:** Auditing is the *detection* half of supply-chain security. The *prevention* half (pinning versions, reviewing updates, minimizing dependencies) is supply-chain hygiene, covered at the end of this file.

---

## TypeScript/JavaScript Example

In the Node ecosystem the built-in tool is `npm audit`, which compares your `package-lock.json` against the GitHub Advisory Database.

```bash
# Node v22 / npm 10 — audit the locked dependency tree
npm audit

# Machine-readable output for CI
npm audit --json

# Fail CI only on serious issues
npm audit --audit-level=high

# Attempt automatic remediation (may apply breaking major bumps with --force)
npm audit fix
```

A typical run reports something like:

```text
# npm audit report

minimist  <1.2.6
Severity: critical
Prototype Pollution in minimist - https://github.com/advisories/GHSA-xvch-5gv4-984h
fix available via `npm audit fix`
node_modules/minimist

1 critical severity vulnerability

To address all issues, run:
  npm audit fix
```

Teams typically wire a license and policy check on top with a third-party tool:

```jsonc
// package.json — using a third-party license checker in CI
{
  "scripts": {
    "audit": "npm audit --audit-level=high",
    "licenses": "license-checker --onlyAllow 'MIT;Apache-2.0;ISC;BSD-3-Clause'"
  }
}
```

**Key points about the Node workflow:**

- `npm audit` is built in; license and duplicate-version policy require *extra* tools (`license-checker`, `npm dedupe`, `lockfile-lint`, Socket, Snyk, etc.).
- `npm audit fix` can silently change your lockfile, sometimes with breaking upgrades under `--force`.
- The advisory data is GitHub's; the tree is huge, so noise (especially `devDependencies`-only advisories) is common.

---

## Rust Equivalent

Rust splits the same responsibilities across two focused, installable subcommands. Both read the `Cargo.lock` produced by `cargo build`, so they audit the *exact* versions you ship.

```bash
# Install once (these are standalone binaries, not built into cargo)
cargo install cargo-audit --locked
cargo install cargo-deny  --locked

# 1. Vulnerability scan against the RUSTSEC advisory database
cargo audit

# 2. Policy engine: advisories + licenses + duplicate versions + crate sources
cargo deny check
```

Run against a project that depends on an old, vulnerable version of the `time` crate, `cargo audit` reports the real advisory:

```text
    Fetching advisory database from `https://github.com/RustSec/advisory-db.git`
      Loaded 1100 security advisories (from /Users/you/.cargo/advisory-db)
    Updating crates.io index
    Scanning Cargo.lock for vulnerabilities (7 crate dependencies)
Crate:     time
Version:   0.1.44
Title:     Potential segfault in the time crate
Date:      2020-11-18
ID:        RUSTSEC-2020-0071
URL:       https://rustsec.org/advisories/RUSTSEC-2020-0071
Severity:  6.2 (medium)
Solution:  Upgrade to >=0.2.23
Dependency tree:
time 0.1.44
└── probe 0.1.0

error: 1 vulnerability found!
```

When the tree is clean, `cargo audit` prints the scan line and exits `0` with no findings:

```text
      Loaded 1100 security advisories (from /Users/you/.cargo/advisory-db)
    Scanning Cargo.lock for vulnerabilities (12 crate dependencies)
```

**Key points about the Rust workflow:**

- `cargo audit` is *only* about RUSTSEC security advisories: focused and low-noise.
- `cargo deny` is a separate, configurable policy engine for advisories **plus** licenses, banned/duplicate crates, and allowed sources; no extra third-party tools needed.
- Both exit non-zero on a finding, so dropping them into CI is a one-liner.

---

## Detailed Explanation

### How `cargo audit` works

`cargo audit` reads `Cargo.lock`, then for each `name@version` checks the local clone of the **RUSTSEC advisory database** (a Git repo of TOML files at <https://github.com/rustsec/advisory-db>). On each run it `git fetch`es the latest advisories, so you do not need to reinstall the tool to get new data.

Walk through the failing report field by field:

- **`Crate` / `Version`**: the exact locked version found in your tree. `time 0.1.44` here.
- **`ID`**: the advisory identifier, e.g. `RUSTSEC-2020-0071`. This is the stable handle you reference when ignoring or tracking it.
- **`Severity`**: a CVSS score when the advisory has one (`6.2 (medium)`).
- **`Solution`**: the maintainers' recommended fix, usually a version constraint: `Upgrade to >=0.2.23`.
- **`Dependency tree`**: *why* the crate is in your build. Critically this shows whether a vulnerable crate is a direct dependency you can bump, or a transitive one pulled in by something else.

The fix for the example above is a `cargo update`:

```bash
# Pull a newer, unaffected version within your version constraints
cargo update -p time
```

If your `Cargo.toml` pins `time` to an old major (`time = "0.1"`), `cargo update` alone can't cross the major boundary; you must edit `Cargo.toml` (`cargo add time` to get the current release) and adapt to API changes. This is the same trade-off as `npm audit fix` vs `npm audit fix --force`, except Rust makes you do the breaking bump deliberately rather than behind a flag.

> **Note:** `cargo audit` reasons purely from `Cargo.lock`, so it audits what you will actually compile. If your `Cargo.lock` is out of date, run `cargo generate-lockfile` first. To also scan an already-compiled binary's embedded dependency list, build it with the [`cargo auditable`](https://github.com/rust-secure-code/cargo-auditable) wrapper and run `cargo audit bin ./path/to/binary`.

### How `cargo deny` works

`cargo deny` constructs the full dependency graph (via `cargo metadata`) and runs four independent **checks**, each governed by a `deny.toml` in your project root:

| Check | What it enforces |
| --- | --- |
| `advisories` | RUSTSEC vulnerabilities, plus *unmaintained*, *unsound*, and *yanked* crates |
| `licenses` | Only allowed SPDX licenses appear in the tree |
| `bans` | No banned crates, no accidental *duplicate versions* of the same crate, no wildcard deps |
| `sources` | Crates come only from approved registries / Git hosts |

Generate a starter config and run all checks:

```bash
cargo deny init    # writes a heavily-commented deny.toml template
cargo deny check   # runs advisories + licenses + bans + sources
```

Run against the same vulnerable project, the `advisories` check produces a rich diagnostic with file spans:

```text
error[vulnerability]: Potential segfault in the time crate
  ┌─ /path/to/probe/Cargo.lock:3:1
  │
3 │ time 0.1.44 registry+https://github.com/rust-lang/crates.io-index
  │ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ security vulnerability detected
  │
  ├ ID: RUSTSEC-2020-0071
  ├ Advisory: https://rustsec.org/advisories/RUSTSEC-2020-0071
  ...
  ├ Solution: Upgrade to >=0.2.23 (try `cargo update -p time`)
  ├ time v0.1.44
    └── probe v0.1.0
```

At the end it prints a one-line summary of every check:

```text
advisories FAILED, bans ok, licenses FAILED, sources ok
```

You can run a single check when you only care about one dimension:

```bash
cargo deny check advisories
cargo deny check licenses
cargo deny check bans
cargo deny check sources
```

### A realistic `deny.toml`

The template from `cargo deny init` denies everything by default, so out of the box even permissive licenses like `MIT OR Apache-2.0` are *rejected* until you allow them. A practical config for a typical service looks like this:

```toml
# deny.toml

[advisories]
# Fail on any known vulnerability or unmaintained crate.
# Temporarily silence a specific advisory you have triaged and accepted:
ignore = [
    # "RUSTSEC-2020-0071",  # tracked in JIRA-1234; no fix shipped yet
]

[licenses]
# Explicit allow-list — the standard permissive set for most companies.
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
]
# Reject copyleft etc. by simply never allowing them; nothing else needed.
confidence-threshold = 0.9

[bans]
# Surface accidental duplicate major versions of the same crate.
multiple-versions = "warn"
# Wildcard ("*") version requirements are a supply-chain footgun — forbid them.
wildcards = "deny"
deny = [
    # Example: ban a crate you have a policy against pulling in directly.
    # { name = "openssl-sys" },  # we standardize on rustls
]

[sources]
# Only crates.io and explicitly trusted Git sources are allowed.
unknown-registry = "deny"
unknown-git = "deny"
allow-git = [
    # "https://github.com/your-org/internal-fork",
]
```

The two tools overlap on advisories but are complementary: `cargo audit` is the fast, focused vulnerability gate; `cargo deny` is the broader policy gate that *also* covers licenses, duplicate versions, and crate provenance. Many teams run both.

---

## Key Differences

| Concern | Node (`npm audit` + extras) | Rust (`cargo audit` / `cargo deny`) |
| --- | --- | --- |
| Advisory source | GitHub Advisory Database | RUSTSEC advisory-db (curated, Rust-specific) |
| Scope of audit | Whole `node_modules` tree | Exact `Cargo.lock` versions |
| Built-in vs installed | `npm audit` is built into npm | Standalone binaries you `cargo install` |
| Licenses | Needs `license-checker`/Snyk | Built into `cargo deny` |
| Duplicate versions | `npm dedupe` (separate) | `cargo deny` `bans.multiple-versions` |
| Source/registry policy | `lockfile-lint` (separate) | `cargo deny` `sources` check |
| Auto-fix | `npm audit fix` (can break, `--force`) | `cargo update` / manual major bump (deliberate) |
| Typical tree size | thousands of packages | dozens–hundreds of crates |
| Lockfile committed | always (`package-lock.json`) | **apps yes; libraries increasingly yes too** |

### Smaller trees, lower noise

A core cultural difference: the Rust ecosystem favors fewer, larger, more carefully reviewed crates, where Node favors many tiny packages. A `left-pad`-style one-line package is rare in Rust. Smaller trees mean fewer advisories to triage and a smaller attack surface, but they do not eliminate risk, which is why auditing is still mandatory.

### `Cargo.lock`: commit it for binaries

Whether to commit `Cargo.lock` is a real difference from Node, where you always commit `package-lock.json`. The Rust guidance: **commit `Cargo.lock` for binaries/applications** (so builds and audits are reproducible). Libraries traditionally omitted it, though current guidance increasingly favors committing it for reproducible CI; either way, downstream consumers still resolve their own versions from your `Cargo.toml`. Auditing only means something against a committed lockfile, so any deployable service must commit it.

> **Tip:** RUSTSEC advisories include categories Node's tooling lacks first-class support for: `unmaintained` and `unsound`. An *unmaintained* crate is not a vulnerability today but is a future liability; an *unsound* crate has a memory-safety hole reachable from safe code. `cargo deny` lets you set the severity for each independently.

---

## Common Pitfalls

### Pitfall 1: Auditing a stale or missing lockfile

Both tools read `Cargo.lock`. If it does not match your `Cargo.toml`, your audit is auditing fiction. Always make sure the lockfile is current. In CI, run the audit *after* a build, or run `cargo generate-lockfile` first, and never `.gitignore` `Cargo.lock` for a deployable app.

### Pitfall 2: Treating the audit as a one-time event

A clean audit today says nothing about tomorrow. New advisories are published against crates you already depend on, without you changing a line. The fix is to run audits **on a schedule** (e.g. a nightly CI job), not only on pull requests. RUSTSEC publishes new advisories continuously.

### Pitfall 3: Silencing an advisory the wrong way

When you must accept a finding temporarily (no patch exists yet, or the vulnerable path is unreachable), do it explicitly and with a paper trail. `cargo audit` takes `--ignore`:

```bash
# Suppress one advisory by ID; exits 0 if that is the only finding
cargo audit --ignore RUSTSEC-2020-0071
```

In `cargo deny`, record it in `deny.toml` under `[advisories] ignore = [...]` *with a comment explaining why and when it will be revisited*. Never blanket-ignore a whole check or silence findings by deleting the audit step; that converts a known risk into an unknown one.

### Pitfall 4: Forgetting `--locked` when installing audit tools

If you `cargo install cargo-audit` without `--locked`, Cargo resolves the tool's *own* dependencies fresh, which occasionally fails to build on older toolchains. Use `--locked` so you install the exact dependency set the maintainers tested:

```bash
cargo install cargo-audit --locked
cargo install cargo-deny  --locked
```

### Pitfall 5: Expecting `cargo audit fix` to exist

Older blog posts mention `cargo audit fix`. In current `cargo-audit` (0.22) that subcommand is **not** in the default build, so this fails:

```text
error: unrecognized subcommand 'fix'

Usage: cargo audit [OPTIONS] [COMMAND]
```

Remediate with `cargo update -p <crate>` for in-range fixes, or edit `Cargo.toml` and adapt to API changes for major bumps. Making remediation explicit is intentional: a security upgrade should be a reviewed change, not a silent one.

### Pitfall 6: Surprise license rejections from the default `deny.toml`

The `cargo deny init` template ships with an **empty allow-list**, so the very first `cargo deny check licenses` rejects ordinary permissive licenses. The diagnostic looks like this (the exact crate it names first depends on your tree):

```text
error[rejected]: failed to satisfy license requirements
   ┌─ /Users/you/.cargo/registry/.../some-crate/Cargo.toml
   │
   │ license = "MIT OR Apache-2.0"
   │            ━━━────━━━━━━━━━━
   │            │      │
   │            │      rejected: license is not explicitly allowed
   │            rejected: license is not explicitly allowed
```

This is not a bug; it is `cargo deny` refusing to assume a policy for you. Add an explicit `allow = [ ... ]` list (see the realistic `deny.toml` above) listing the SPDX identifiers your organization permits.

---

## Best Practices

- **Run both tools in CI and fail the build on findings.** `cargo audit` for fast vulnerability gating, `cargo deny check` for the full policy gate.
- **Add a scheduled audit job**, not just a pull-request job, so newly-disclosed advisories against unchanged dependencies are caught within a day.
- **Commit `Cargo.lock` for every binary/service.** Reproducible builds are a prerequisite for meaningful audits.
- **Keep a small, explicit `deny.toml`** with an allow-list of licenses, `wildcards = "deny"`, and `multiple-versions = "warn"`. Review it like code.
- **Triage, do not blanket-ignore.** When you must accept a finding, pin the specific advisory ID with a dated comment and a tracking ticket.
- **Minimize dependencies.** Every crate you add is code you now trust with your process. Prefer the standard library and a few well-maintained crates over many micro-crates. Run `cargo tree` to understand what you are actually pulling in.
- **Review dependency updates, especially new transitive crates.** A version bump that *adds* a brand-new transitive dependency deserves a look; tools like `cargo deny` (`bans`/`sources`) and bot-driven update PRs (Dependabot/Renovate) help here.
- **Pin tool versions in CI** with `--locked` installs so audit results are reproducible across runs.

### Wiring it into CI (GitHub Actions)

```yaml
# .github/workflows/security-audit.yml
name: security-audit
on:
  push:
  pull_request:
  schedule:
    - cron: "0 6 * * *"   # daily, so new advisories are caught quickly

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install audit tools
        run: |
          cargo install cargo-audit --locked
          cargo install cargo-deny  --locked
      - name: cargo audit
        run: cargo audit
      - name: cargo deny
        run: cargo deny check
```

> **Tip:** There are also maintained GitHub Actions (`rustsec/audit-check` and `EmbarkStudios/cargo-deny-action`) that cache the tool and the advisory DB for you. The hand-rolled job above shows exactly what they do under the hood.

---

## Real-World Example

A small but realistic setup for a deployable web service: a `deny.toml` policy, a `Makefile`-style task runner, and a `pre-push` Git hook so nobody pushes a tree with a known critical vulnerability.

**`deny.toml`** (project root):

```toml
# deny.toml — policy for an internal web service

[advisories]
# By default cargo-deny treats vulnerable, unmaintained, and yanked crates as hard errors.
ignore = []   # keep empty; add dated, ticketed entries only after triage

[licenses]
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Zlib",
]
confidence-threshold = 0.9

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

**A `cargo-make`-style task file (`Makefile.toml`)** so the same commands run locally and in CI:

```toml
# Makefile.toml — run with `cargo make audit`
[tasks.audit]
description = "Full security gate: vulnerabilities + policy"
script = [
    "cargo audit",
    "cargo deny check",
]
```

**A `pre-push` hook (`.git/hooks/pre-push`, made executable)** that blocks pushing a known-vulnerable tree:

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "Running cargo audit before push..."
if ! cargo audit; then
    echo "cargo audit found a vulnerability — push aborted." >&2
    echo "  Fix with 'cargo update -p <crate>' or triage in deny.toml." >&2
    exit 1
fi
echo "audit clean"
```

With this in place, a developer who runs `cargo add time@0.1` and tries to push sees the push rejected locally with the RUSTSEC report shown earlier, long before the change reaches CI or production. The CI job is the backstop; the hook and the daily scheduled run are the fast feedback loops.

This connects directly to deployment: see [Production](/28-production/) for releasing the audited binary and keeping the pipeline healthy.

---

## Further Reading

- [The RustSec Advisory Database](https://rustsec.org/) — the data both tools consume
- [`cargo-audit` documentation](https://github.com/rustsec/rustsec/tree/main/cargo-audit) — flags, `audit.toml`, `cargo audit bin`
- [`cargo-deny` book](https://embarkstudios.github.io/cargo-deny/) — every `deny.toml` field explained
- [The Cargo Book: `cargo update` and `Cargo.lock`](https://doc.rust-lang.org/cargo/reference/cargo-toml-vs-cargo-lock.html) — when to commit the lockfile
- [`cargo auditable`](https://github.com/rust-secure-code/cargo-auditable) — embed the dependency list in the binary so it can be audited after build

Related topics in this guide:

- [Secrets Management](/27-security/07-secrets-management/) — keeping credentials out of your repo (and your logs)
- [Cryptography Done Right](/27-security/03-cryptography/) — why "use vetted crates" is itself supply-chain hygiene
- [TLS/SSL with rustls](/27-security/05-tls-ssl/) — `rustls` vs OpenSSL, a supply-chain-relevant dependency choice
- [Input Validation and Sanitization](/27-security/00-input-validation/) — defense in depth alongside dependency hygiene
- [Section 00: Introduction](/00-introduction/) · [Section 01: Getting Started](/01-getting-started/) · [Section 02: Basics](/02-basics/)
- Next: [Section 28: Production](/28-production/)

---

## Exercises

### Exercise 1: Catch a real vulnerability

**Difficulty:** Beginner

**Objective:** Install `cargo audit` and watch it flag a known-vulnerable crate.

**Instructions:**

1. Create a fresh project: `cargo new audit-demo && cd audit-demo`.
2. Add a deliberately old, vulnerable dependency by editing `Cargo.toml` so it contains `time = "=0.1.44"`, then run `cargo generate-lockfile`.
3. Install and run the auditor. Identify the **advisory ID** and the **recommended solution** from the output.

<details>
<summary>Solution</summary>

```bash
cargo new audit-demo && cd audit-demo

# Cargo.toml -> [dependencies]
#   time = "=0.1.44"
cargo generate-lockfile

cargo install cargo-audit --locked
cargo audit
```

The run reports advisory **`RUSTSEC-2020-0071`** ("Potential segfault in the time crate"), severity `6.2 (medium)`, with **`Solution: Upgrade to >=0.2.23`**. Because `Cargo.toml` pins an exact old major (`=0.1.44`), `cargo update` cannot fix it — you must change the requirement (`cargo add time` to get the current release) and adapt to the new API. After bumping to a current `time`, `cargo audit` exits `0` with no findings.

</details>

### Exercise 2: Enforce a license policy

**Difficulty:** Intermediate

**Objective:** Configure `cargo deny` to allow only permissive licenses and confirm the `licenses` check passes on a clean project.

**Instructions:**

1. In any small Rust project (e.g. one depending on `serde` with the `derive` feature), run `cargo deny init`.
2. Run `cargo deny check licenses` and observe that the default empty allow-list **rejects** ordinary permissive licenses.
3. Edit `deny.toml` to add an explicit allow-list, then re-run until the check passes.

<details>
<summary>Solution</summary>

```bash
cargo new license-demo && cd license-demo
cargo add serde --features derive
cargo install cargo-deny --locked
cargo deny init
cargo deny check licenses   # FAILS: licenses not explicitly allowed
```

Because `cargo new` leaves your own crate unlicensed, add a `license` field to its `[package]` table; otherwise `cargo deny` reports `error[unlicensed]: license-demo = 0.1.0 is unlicensed` for your own crate and the check fails no matter what the allow-list says:

```toml
# Cargo.toml -> [package]
license = "MIT"
```

Then edit `deny.toml`:

```toml
[licenses]
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-3-Clause",
    "Unicode-3.0",
]
confidence-threshold = 0.9
```

```bash
cargo deny check licenses   # now reports "licenses ok"
```

The key insight: `cargo deny` never assumes a license policy for you. An empty allow-list rejects *everything*, including `MIT OR Apache-2.0`, until you state which SPDX identifiers your organization permits — and it flags your *own* unlicensed crate just as readily as a dependency's.

</details>

### Exercise 3: A CI gate that fails on findings

**Difficulty:** Advanced

**Objective:** Write a CI job (or local script) that runs both auditors and fails the build when either reports a finding, and add a triaged exception for a single advisory.

**Instructions:**

1. Write a shell script `scripts/security-gate.sh` that runs `cargo audit` and `cargo deny check`, propagating any non-zero exit code.
2. Suppose `RUSTSEC-2020-0071` has no available fix yet but you have triaged it as unreachable. Add a *documented* exception so the gate passes — once via the `cargo audit` CLI and once via `deny.toml`.
3. Explain why an exception belongs in version control rather than as a hidden CLI flag.

<details>
<summary>Solution</summary>

`scripts/security-gate.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "==> cargo audit"
cargo audit                # add --ignore RUSTSEC-XXXX only for triaged items

echo "==> cargo deny check"
cargo deny check           # advisories + licenses + bans + sources

echo "==> security gate passed"
```

`set -e` plus the natural non-zero exit codes mean either tool failing aborts the script (and thus CI).

To document the triaged exception, prefer `deny.toml` so it lives in version control and is reviewed:

```toml
[advisories]
ignore = [
    # No upstream fix yet; vulnerable path is unreachable in our build.
    # Tracking: SEC-482. Re-check on every dependency bump. Added 2026-06-02.
    "RUSTSEC-2020-0071",
]
```

The equivalent `cargo audit` CLI form is `cargo audit --ignore RUSTSEC-2020-0071`, which I verified exits `0` when that is the only finding — but a CLI flag is invisible to reviewers and easy to copy-paste without context. Putting the exception in `deny.toml` (or `cargo audit`'s config file) makes the decision *auditable*: the diff shows who accepted which risk, when, and why, and the comment forces a revisit on the next change. A security exception should be a reviewed code change, never a hidden command-line argument.

</details>
