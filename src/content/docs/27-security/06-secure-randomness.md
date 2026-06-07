---
title: "Secure Randomness"
description: "Generate tokens, keys, and nonces from a CSPRNG in Rust with rand and getrandom. Unlike JavaScript's Math.random, the convenient default is already secure."
---

Not all random numbers are equal. The generator behind your dice roll in a game is a poor choice for a session token, a password-reset link, or an encryption key. This topic shows how Rust separates fast-but-predictable randomness from cryptographically secure randomness, and how to reach for the right one.

---

## Quick Overview

A **cryptographically secure pseudo-random number generator** (CSPRNG) produces output that an attacker cannot predict or reproduce, even after observing many prior outputs. Anything that becomes a secret (session IDs, CSRF tokens, API keys, salts, nonces, reset links) **must** come from a CSPRNG seeded by the operating system's entropy pool.

The good news for TypeScript/JavaScript developers: Rust's standard randomness crate (`rand`) is secure by default. Its thread-local generator is a CSPRNG, and the foundation crate (`getrandom`) talks directly to the OS (`getrandom(2)` on Linux, `BCryptGenRandom` on Windows, `getentropy` on macOS). The danger is the *opposite* of JavaScript: in JS the convenient `Math.random()` is the insecure one and you must remember to switch to `crypto`; in Rust you have to go out of your way to pick a *non*-secure generator like `SmallRng`.

> **Note:** This file is about *generating* random bytes securely. Hashing those bytes (passwords) lives in [Password Hashing](/27-security/04-password-hashing/), and using them as keys/nonces for encryption lives in [Cryptography Done Right](/27-security/03-cryptography/).

---

## TypeScript/JavaScript Example

In Node.js, `Math.random()` is **not** cryptographically secure — it is a fast PRNG (V8 uses xorshift128+) seeded once at startup. Using it for anything security-sensitive is a real, common vulnerability. The secure path is the `node:crypto` module.

```typescript
import { randomBytes, randomInt, randomUUID } from "node:crypto";

// INSECURE: Math.random() is predictable. NEVER use it for secrets.
function weakToken(): string {
  let s = "";
  for (let i = 0; i < 32; i++) {
    s += Math.floor(Math.random() * 16).toString(16);
  }
  return s; // an attacker who learns the seed can reproduce every token
}

// SECURE: node:crypto is backed by the OS CSPRNG.
function sessionToken(): string {
  return randomBytes(32).toString("hex"); // 256 bits of real entropy
}

// A 6-digit one-time code, unbiased, from the secure source.
function otpCode(): string {
  return randomInt(0, 1_000_000).toString().padStart(6, "0");
}

console.log("weak   :", weakToken());
console.log("session:", sessionToken());
console.log("otp    :", otpCode());
console.log("uuid   :", randomUUID()); // v4 UUID, also crypto-backed
```

Running this under Node v22 prints something like:

```text
weak   : 7a3c0f9e1b...        (predictable — do not use)
session: 8049bcbf450e98a95fe346ba408f71fc...   (64 hex chars = 32 bytes)
otp    : 090096
uuid   : 78432f6b-7d49-4050-8f6a-114605c45d02
```

**Key points:**

- `Math.random()` returns an IEEE-754 `f64` in `[0, 1)` and is **not** secure. It is fine for shuffling a UI carousel; it is a vulnerability for a token.
- `crypto.randomBytes`, `crypto.randomInt`, and `crypto.randomUUID` are the secure, OS-backed APIs. In the browser the equivalent is `crypto.getRandomValues` / `crypto.randomUUID`.
- The footgun is that the *easy-to-type* function is the *insecure* one.

---

## Rust Equivalent

In Rust the defaults are inverted: the convenient `rand::rng()` (the thread-local generator) is already a CSPRNG, and `getrandom`/`SysRng` go straight to the OS. You have to deliberately choose `SmallRng` to get a *non*-secure generator.

The examples below use the current `rand` crate. Add it to a project with:

```bash
cargo add rand
```

This resolves to `rand = "0.10"` (the current stable line; `0.10.1` at the time of writing). The API changed meaningfully across `0.8 → 0.9 → 0.10`, so code you find online may not match; the snippets here are written and compiled against `0.10`.

```rust
use rand::{rng, RngExt};        // RngExt provides the ergonomic .random* methods
use rand::rngs::SysRng;         // SysRng = the OS CSPRNG (called OsRng before 0.10)
use rand::TryRng;               // SysRng's fill is fallible, via try_fill_bytes

fn main() {
    // The thread-local CSPRNG: auto-seeded from the OS, periodically reseeded.
    // This is the right default for almost everything.
    let mut r = rng();
    let n: u32 = r.random();
    let dice: u8 = r.random_range(1..=6);
    println!("random u32 = {n}");
    println!("dice = {dice}");

    // Fill a buffer with random bytes (infallible on the thread RNG).
    let mut token = [0u8; 16];
    r.fill(&mut token[..]);
    println!("token len = {}", token.len());

    // Convenience free functions — they use the thread RNG under the hood.
    let coin = rand::random::<bool>();
    let pct: u8 = rand::random_range(0..=100);
    println!("coin = {coin}, pct = {pct}");

    // SysRng: bytes straight from the OS, no userspace PRNG state.
    // OS calls can fail, so the API is fallible (try_fill_bytes).
    let mut key = [0u8; 32];
    SysRng.try_fill_bytes(&mut key).expect("OS entropy unavailable");
    println!("key first byte = {}", key[0]);
}
```

Real output from one run (your values will differ — that is the point):

```text
random u32 = 474142268
dice = 5
token len = 16
coin = true, pct = 70
key first byte = 241
```

There is no insecure-by-accident path here. `rng()`, `SysRng`, and the `rand::random*` free functions are all cryptographically secure.

---

## Detailed Explanation

**`rng()` — the everyday CSPRNG.** `rand::rng()` returns a `ThreadRng`, a handle to a thread-local generator. On first use it is seeded from the operating system (`SysRng`), then it produces output with the ChaCha12 stream cipher and reseeds itself periodically. It is a CSPRNG, it requires no setup, and because it is thread-local it is fast (no syscall per number). This is the JS `crypto`-quality default, but with the ergonomics of `Math.random()`.

**`RngExt`, where the methods live.** In `rand 0.10` the trait you import for `.random()`, `.random_range()`, and `.fill()` is `RngExt` (it was simply `Rng` in 0.8/0.9). The bare `Rng` trait still exists but is now the low-level "produce raw words" trait (the old `RngCore`). If you forget `use rand::RngExt;`, the compiler tells you exactly which trait to import (see Common Pitfalls).

**`random()` vs `random_range()`.** `random::<T>()` fills a value of type `T` from the full range of its `StandardUniform` distribution (every `u32`, a `bool`, a tuple, etc.). `random_range(1..=6)` maps the bytes into a bounded range. By default `random_range` uses a fast widening-multiply method that has a tiny statistical bias; enabling the crate's `unbiased` feature switches it to rejection sampling for perfectly uniform output. For dice and OTPs this default bias is negligible; if you need provably uniform sampling (e.g. drawing a card for a regulated game), enable `unbiased`.

**`SysRng` — the operating system source.** `SysRng` (re-exported from `getrandom`) is a stateless interface to the OS entropy source. Every call is essentially a syscall, so it is slower than `ThreadRng` but never holds predictable userspace state. Because OS calls can fail (a sandbox with no `getrandom`, an extremely early boot), its fill method is `try_fill_bytes`, returning a `Result`. This is why it lives behind the `TryRng` trait rather than the infallible `Rng` trait.

**`getrandom`, the foundation.** Underneath everything sits the `getrandom` crate, which is the portable shim over each platform's OS CSPRNG. You rarely call it directly, but it is the reason `rand` is secure by default and works on `no_std` and WebAssembly targets.

```rust
fn main() {
    // Lowest level: bytes straight from the OS, no PRNG abstraction at all.
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).expect("OS entropy unavailable");
    let n: u32 = getrandom::u32().expect("OS entropy unavailable");
    println!("seed byte 0 = {}, n = {n}", seed[0]);
}
```

Real output:

```text
seed byte 0 = 102, n = 2173754844
```

**`SmallRng` and `StdRng` — the explicit choices.** `SmallRng` is a small, fast, *non-cryptographic* PRNG for simulations and benchmarks. `StdRng` is the same CSPRNG algorithm that backs `ThreadRng`, but you own the instance — useful when you want a seedable secure generator. Critically, only secure generators implement the `CryptoRng` marker trait, which lets you encode "I require secure randomness" directly in a function signature (shown below).

---

## Key Differences

| Concern | TypeScript / Node.js | Rust (`rand` 0.10) |
| --- | --- | --- |
| Convenient default | `Math.random()`, **insecure** | `rng()`, **secure** (CSPRNG) |
| Secure source | `node:crypto` (`randomBytes`, `randomInt`) | `rng()`, `SysRng`, `getrandom::fill` |
| Insecure source | `Math.random()` (always there) | `SmallRng` (must opt in) |
| Bytes into a buffer | `randomBytes(n)` → `Buffer` | `rng().fill(&mut buf)` / `SysRng.try_fill_bytes` |
| Bounded integer | `crypto.randomInt(0, n)` | `rng().random_range(0..n)` |
| UUID v4 | `crypto.randomUUID()` | `uuid` crate, `Uuid::new_v4()` |
| Secure-only at the type level | not expressible | `R: CryptoRng` bound |
| Failure handling | throws | `SysRng` returns `Result` (fallible) |

The deepest difference is the **type-level guarantee**. In JavaScript nothing stops you from passing the output of `Math.random()` where a secret is expected; it is all `number`. In Rust you can write a function that *only* accepts a cryptographically secure generator, and the compiler rejects a fast non-secure one:

```rust
use rand::{RngExt, SeedableRng, CryptoRng};
use rand::rngs::{StdRng, SmallRng, SysRng};

// Accepts ONLY a cryptographically secure RNG — enforced at compile time.
fn make_token<R: CryptoRng>(rng: &mut R) -> [u8; 16] {
    let mut buf = [0u8; 16];
    rng.fill(&mut buf[..]);
    buf
}

fn main() {
    // StdRng seeded from the OS IS a CryptoRng — accepted.
    let mut secure = StdRng::try_from_rng(&mut SysRng).expect("OS entropy");
    let token = make_token(&mut secure);
    println!("token byte 0 = {}", token[0]);

    // SmallRng is fast but NOT cryptographic — fine for simulations only.
    let mut fast = SmallRng::seed_from_u64(7);
    let x: u64 = fast.random();
    println!("non-crypto value = {x}");
    // make_token(&mut fast); // would NOT compile — see Common Pitfalls
}
```

Real output:

```text
token byte 0 = 246
non-crypto value = 1021219803524665661
```

This is "make illegal states unrepresentable" applied to entropy: the same parse-don't-validate idea explored in [Input Validation and Sanitization](/27-security/00-input-validation/), but for randomness quality.

---

## Common Pitfalls

### Pitfall 1: Forgetting to import `RngExt`

The ergonomic methods live on a trait, and the method is invisible until that trait is in scope. This is the single most common confusion for newcomers (and for anyone copying `rand 0.8` code).

```rust
use rand::rng;   // forgot: use rand::RngExt;

fn main() {
    let mut r = rng();
    let n: u32 = r.random();   // does not compile (error[E0599]: no method named `random`)
    println!("{n}");
}
```

The real compiler output points straight at the fix:

```text
error[E0599]: no method named `random` found for struct `ThreadRng` in the current scope
 --> src/main.rs:5:20
  |
5 |     let n: u32 = r.random();   //
  |                    ^^^^^^
  |
  = help: items from traits can only be used if the trait is in scope
help: trait `RngExt` which provides `random` is implemented but not in scope; perhaps you want to import it
  |
1 + use rand::RngExt;
```

Add `use rand::RngExt;` (or `use rand::prelude::*;`, which pulls in `RngExt`, `SeedableRng`, and the common generators) and it compiles.

### Pitfall 2: Using a non-CSPRNG (or a fixed seed) for a secret

`SmallRng` is fast and great for Monte-Carlo work, but it is **not** secure — its output is predictable from a small amount of observed data, and worse, code often seeds it with a constant. If you try to use a non-secure generator where a `CryptoRng` is required, the compiler stops you:

```rust
use rand::{RngExt, SeedableRng, CryptoRng};
use rand::rngs::SmallRng;

fn make_token<R: CryptoRng>(rng: &mut R) -> [u8; 16] {
    let mut buf = [0u8; 16];
    rng.fill(&mut buf[..]);
    buf
}

fn main() {
    let mut fast = SmallRng::seed_from_u64(7);
    let _token = make_token(&mut fast);   // does not compile (error[E0277])
}
```

The real error spells out that `SmallRng` is not cryptographic:

```text
error[E0277]: the trait bound `SmallRng: CryptoRng` is not satisfied
  --> src/main.rs:12:29
   |
12 |     let _token = make_token(&mut fast);   //
   |                  ---------- ^^^^^^^^^ ...
   |
   = note: required for `SmallRng` to implement `TryCryptoRng`
   = note: required for `SmallRng` to implement `CryptoRng`
note: required by a bound in `make_token`
```

The takeaway: bound your security-sensitive helpers on `CryptoRng` so this class of mistake cannot reach production. A `SmallRng::seed_from_u64(7)` would otherwise compile happily and produce *the same token on every run* — a catastrophic auth bug.

### Pitfall 3: Comparing secret tokens with `==`

Generating a token securely is only half the job. Verifying it with the ordinary `==` operator leaks length and prefix information through timing, because `==` returns as soon as it finds the first differing byte. Compare in **constant time** with the `subtle` crate (`cargo add subtle`):

```rust
use subtle::ConstantTimeEq;

fn tokens_match(stored: &str, candidate: &str) -> bool {
    // Returns false fast on length mismatch, then compares bytes in constant time.
    stored.as_bytes().ct_eq(candidate.as_bytes()).into()
}

fn main() {
    let stored = "uVnwjGNJWAL_bhnr3QywCwwpNeelGKSJquMt1U3hZf0";
    println!("correct  = {}", tokens_match(stored, stored));
    println!("tampered = {}", tokens_match(stored, "not-the-token"));
}
```

Real output:

```text
correct  = true
tampered = false
```

> **Warning:** Timing-safe comparison matters for any secret you compare against attacker-supplied input — tokens, HMAC tags, API keys. It does *not* substitute for hashing: see [Cryptography Done Right](/27-security/03-cryptography/) for the full picture.

### Pitfall 4: Too few bytes

A "random" token is only as strong as its entropy. A 4-byte (32-bit) token has only ~4 billion possibilities — brute-forceable. Use **at least 16 bytes (128 bits)** for tokens, and 32 bytes (256 bits) when in doubt. Bytes are cheap.

---

## Best Practices

- **Default to `rand::rng()`.** It is a CSPRNG, requires no setup, and is fast. Reach for `SysRng`/`getrandom` only when you specifically want a stateless OS source (e.g. generating a one-off key with no thread-local generator around).
- **Use `SmallRng` only for non-security work**, and say so in a comment. Simulations, fuzz inputs, procedural generation: fine. Tokens, keys, salts, nonces: never.
- **Encode the requirement in types.** Make security-sensitive functions generic over `R: CryptoRng` (or just take `&mut impl CryptoRng`). The compiler then forbids passing a `SmallRng`.
- **Generate at least 128 bits of entropy** for tokens; 256 bits for long-lived secrets.
- **Encode for transport, don't shrink.** Turn raw bytes into hex (`hex` crate) or URL-safe base64 (`base64` crate) for cookies and links — never truncate the entropy to make it "look nicer."
- **Compare secrets in constant time** with `subtle::ConstantTimeEq`.
- **Don't seed a CSPRNG from a timestamp, PID, or counter.** That defeats the entire point; let it seed from the OS.
- **For UUIDs, use the `uuid` crate** with `Uuid::new_v4()` (it pulls entropy from `getrandom`) rather than assembling one by hand.

---

## Real-World Example

A production-grade, single-use token type: the kind you would hand out for a password-reset or email-confirmation link. It generates 256 bits of OS entropy, encodes it URL-safe so it can live in a query string, and verifies in constant time.

Dependencies (`cargo add rand base64 subtle getrandom`):

```toml
[dependencies]
rand = "0.10"
base64 = "0.22"
subtle = "2.6"
getrandom = "0.4"
```

```rust
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::rngs::SysRng;
use rand::TryRng;
use subtle::ConstantTimeEq;

/// A short-lived, single-use token (password reset, email confirmation, ...).
#[derive(Clone)]
struct ResetToken(String);

impl ResetToken {
    /// 256 bits of OS entropy, URL-safe so it can go directly in a link.
    fn generate() -> Result<Self, getrandom::Error> {
        let mut bytes = [0u8; 32];
        SysRng.try_fill_bytes(&mut bytes)?;        // OS CSPRNG; fallible
        Ok(Self(URL_SAFE_NO_PAD.encode(bytes)))
    }

    /// Compare against a candidate in constant time (no timing side channel).
    fn matches(&self, candidate: &str) -> bool {
        self.0.as_bytes().ct_eq(candidate.as_bytes()).into()
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

fn main() -> Result<(), getrandom::Error> {
    let token = ResetToken::generate()?;

    println!(
        "reset link: https://app.example.com/reset?token={}",
        token.as_str()
    );
    println!("token length (chars) = {}", token.as_str().len());
    println!("correct match  = {}", token.matches(token.as_str()));
    println!("tampered match = {}", token.matches("not-the-token"));
    Ok(())
}
```

Real output (the token differs every run — exactly what you want):

```text
reset link: https://app.example.com/reset?token=uVnwjGNJWAL_bhnr3QywCwwpNeelGKSJquMt1U3hZf0
token length (chars) = 43
correct match  = true
tampered match = false
```

In a real service you would store a *hash* of this token (see [Password Hashing](/27-security/04-password-hashing/)) so a database leak does not expose live reset links, deliver it over TLS (see [TLS/SSL with rustls](/27-security/05-tls-ssl/)), and keep the value out of your logs (see [Secrets Management](/27-security/07-secrets-management/)).

---

## Further Reading

- [The `rand` book](https://rust-random.github.io/book/) — concepts, the `0.10` migration notes, and choosing a generator.
- [`rand` crate docs](https://docs.rs/rand) and [`getrandom` crate docs](https://docs.rs/getrandom): the secure foundation and its platform table.
- [`CryptoRng` trait](https://docs.rs/rand_core/latest/rand_core/trait.CryptoRng.html): the marker that distinguishes secure generators.
- [`subtle` crate](https://docs.rs/subtle): constant-time equality and selection.
- [MDN: `Crypto.getRandomValues()`](https://developer.mozilla.org/en-US/docs/Web/API/Crypto/getRandomValues) and [Node `crypto`](https://nodejs.org/api/crypto.html): the JavaScript equivalents.
- Within this guide: [Cryptography Done Right](/27-security/03-cryptography/) (using random bytes as keys/nonces), [Password Hashing](/27-security/04-password-hashing/) (salts), [Input Validation and Sanitization](/27-security/00-input-validation/) (type-driven guarantees), and the section landing page [Security](/27-security/).
- Foundations referenced here: traits and trait bounds build on the earlier guide material — see [Section 02: Basics](/02-basics/) for types and [Section 01: Getting Started](/01-getting-started/) / [Section 00: Introduction](/00-introduction/) for `cargo add` and project setup. Hardening for deployment continues in [Section 28: Production](/28-production/).

---

## Exercises

### Exercise 1: Secure API key generator

**Difficulty:** Beginner

**Objective:** Get comfortable with the secure default generator and byte encoding.

**Instructions:** Write a function `api_key() -> String` that returns a 24-byte (192-bit) key encoded as lowercase hex (so 48 characters). Use the thread-local CSPRNG. Add the `hex` crate with `cargo add hex`. Print three keys from `main` and confirm they differ on every run.

<details>
<summary>Solution</summary>

```rust
// cargo add rand hex
use rand::{rng, RngExt};

fn api_key() -> String {
    let mut bytes = [0u8; 24];     // 192 bits of entropy
    rng().fill(&mut bytes[..]);    // ThreadRng is a CSPRNG
    hex::encode(bytes)             // 48 lowercase hex chars
}

fn main() {
    for _ in 0..3 {
        let key = api_key();
        println!("key = {key} ({} chars)", key.len());
    }
}
```

Real output (different every run):

```text
key = 4c264f9695fb29d591cd6e9dbc7bcc6aba4eb4bc4614b8bd (48 chars)
key = a1f0...  (48 chars)
key = 7e93...  (48 chars)
```

</details>

### Exercise 2: A zero-padded one-time code

**Difficulty:** Intermediate

**Objective:** Generate an unbiased bounded integer from a secure source and format it.

**Instructions:** Write `otp_code() -> String` that returns a 6-digit numeric one-time code, zero-padded (so `42` becomes `"000042"`). Use `random_range` on the thread RNG. Bonus: explain in a comment why drawing a `u32` in `0..1_000_000` and formatting is better than `rng().random::<u32>() % 1_000_000`.

<details>
<summary>Solution</summary>

```rust
// cargo add rand
use rand::{rng, RngExt};

/// A 6-digit numeric one-time code, zero-padded.
fn otp_code() -> String {
    // random_range maps entropy into 0..1_000_000 cleanly.
    // Doing `random::<u32>() % 1_000_000` introduces modulo bias because
    // 2^32 is not a multiple of 1_000_000, so low codes are slightly more
    // likely. random_range avoids that (and the `unbiased` feature removes
    // even the residual bias entirely).
    let n: u32 = rng().random_range(0..1_000_000);
    format!("{n:06}")
}

fn main() {
    for _ in 0..3 {
        println!("{}", otp_code());
    }
}
```

Real output:

```text
678917
092963
003057
```

</details>

### Exercise 3: Enforce secure randomness at the type level

**Difficulty:** Advanced

**Objective:** Use the `CryptoRng` bound so misuse is a compile error, and prove it.

**Instructions:** Write `fn nonce<R: CryptoRng>(rng: &mut R) -> [u8; 12]` that fills a 12-byte nonce. Call it from `main` with a secure generator (`StdRng::try_from_rng(&mut SysRng)`). Then add a commented-out line that tries to call it with `SmallRng` and, in a comment, paste or paraphrase the error you get when you uncomment it. Explain why this matters for AEAD nonces.

<details>
<summary>Solution</summary>

```rust
// cargo add rand
use rand::{RngExt, SeedableRng, CryptoRng};
use rand::rngs::{StdRng, SmallRng, SysRng};

/// Produce a 12-byte nonce. The CryptoRng bound forbids non-secure RNGs.
fn nonce<R: CryptoRng>(rng: &mut R) -> [u8; 12] {
    let mut buf = [0u8; 12];
    rng.fill(&mut buf[..]);
    buf
}

fn main() {
    // Secure: StdRng seeded from the OS implements CryptoRng.
    let mut secure = StdRng::try_from_rng(&mut SysRng).expect("OS entropy");
    let n = nonce(&mut secure);
    println!("nonce = {n:?}");

    // Insecure: SmallRng is NOT a CryptoRng. Uncommenting this fails to compile:
    let mut _fast = SmallRng::seed_from_u64(7);
    // let _bad = nonce(&mut _fast);
    // error[E0277]: the trait bound `SmallRng: CryptoRng` is not satisfied
    //   = note: required for `SmallRng` to implement `CryptoRng`
    //   note: required by a bound in `nonce`
}
```

Real output (the uncommented program runs cleanly; values differ each run):

```text
nonce = [188, 51, 9, 247, 22, 130, 64, 201, 78, 5, 233, 17]
```

**Why it matters:** AEAD ciphers (AES-GCM, ChaCha20-Poly1305) require a nonce that is *never reused* under the same key — a repeated nonce can leak plaintext and even the authentication key. A predictable generator like a fixed-seed `SmallRng` would hand out the same nonce sequence on every process start, which is exactly the kind of catastrophic reuse the `CryptoRng` bound prevents at compile time. The encryption side of this is covered in [Cryptography Done Right](/27-security/03-cryptography/).

</details>
