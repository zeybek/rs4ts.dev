---
title: "Cryptography Done Right"
description: "Encrypt data in Rust with AES-GCM and ChaCha20-Poly1305 AEAD where the auth tag can't be forgotten, unlike Node's separate getAuthTag/setAuthTag in TypeScript."
---

Cryptography is one of the few areas of programming where "clever" is a synonym for "broken." The same rule applies in TypeScript and in Rust: you never invent your own primitives, and you reach for well-reviewed libraries that expose hard-to-misuse APIs. This chapter shows the Rust equivalents of Node's `crypto` module, centered on **AEAD** (Authenticated Encryption with Associated Data), the only kind of symmetric encryption you should be using in 2026.

---

## Quick Overview

In Node you call into `node:crypto`, which wraps OpenSSL. Rust has two mainstream, audited options that play the same role:

- **[RustCrypto](https://github.com/RustCrypto)** — a family of pure-Rust crates (`aes-gcm`, `chacha20poly1305`, `sha2`, `hkdf`, `hmac`, ...) with a shared trait system. Pure Rust, no C dependency, great for portability and WebAssembly.
- **[`ring`](https://github.com/briansmith/ring)**: a focused, opinionated library (Rust + vendored BoringSSL assembly) used by `rustls`. Fewer knobs, very fast.

The golden rule for a TypeScript/JavaScript developer moving to Rust is unchanged: **do not roll your own crypto, and do not hand-assemble primitives.** Pick an AEAD construction (`AES-256-GCM` or `ChaCha20-Poly1305`), let the library generate keys and nonces, and treat ciphertext as opaque bytes. The big behavioral difference you must internalize: Rust's type system and the AEAD APIs make it *hard* to forget the authentication tag, unlike Node's `aes-256-gcm`, where the tag lives in a separate `getAuthTag()`/`setAuthTag()` call you can accidentally skip.

> **Note:** This chapter is about *encryption* (keeping data confidential and tamper-evident). For *passwords* you want a deliberately slow hash, not encryption; see [Password Hashing](/27-security/04-password-hashing/). For where the random bytes come from, see [Secure Randomness](/27-security/06-secure-randomness/). For keeping keys out of logs and memory, see [Secrets Management](/27-security/07-secrets-management/).

---

## TypeScript/JavaScript Example

A typical Node service encrypting a small secret (say, a stored API token) with AES-256-GCM:

```typescript
// Node v22 — symmetric encryption with the built-in crypto module
import { randomBytes, createCipheriv, createDecipheriv } from "node:crypto";

const key = randomBytes(32); // AES-256 key: 32 bytes
const nonce = randomBytes(12); // 96-bit IV — MUST be unique per message

// --- Encrypt ---
const cipher = createCipheriv("aes-256-gcm", key, nonce);
const ciphertext = Buffer.concat([
  cipher.update("transfer $100 to Bob", "utf8"),
  cipher.final(),
]);
const tag = cipher.getAuthTag(); // 16-byte auth tag — a SEPARATE value you must store!

// --- Decrypt ---
const decipher = createDecipheriv("aes-256-gcm", key, nonce);
decipher.setAuthTag(tag); // forget this and final() THROWS — but update() above already returned unauthenticated plaintext
const plaintext = Buffer.concat([decipher.update(ciphertext), decipher.final()]);

console.log("key:", key.length, "nonce:", nonce.length, "tag:", tag.length);
console.log("decrypted:", plaintext.toString("utf8"));
```

Running it under Node v22:

```text
key: 32 nonce: 12 tag: 16
decrypted: transfer $100 to Bob
```

This is correct, but notice the foot-guns baked into the API:

- The **authentication tag is a third value** you have to remember to capture (`getAuthTag()`), store alongside the ciphertext, and feed back in (`setAuthTag()`). Omit `setAuthTag()` and `final()` throws, but `decipher.update()` has already handed you unauthenticated plaintext, because GCM is CTR-mode underneath; the tag is only checked at `final()`. The integrity check is an opt-in second step you can read around.
- Nothing stops you from reusing `nonce` across messages (catastrophic for GCM).
- The algorithm is a magic string (`"aes-256-gcm"`); a typo or a downgrade to ECB compiles and runs.

---

## Rust Equivalent

The RustCrypto `aes-gcm` crate folds the tag *into* the ciphertext, so there is no separate value to forget. Add the dependency:

```toml
# Cargo.toml
[dependencies]
aes-gcm = "0.10.3"
chacha20poly1305 = "0.10.1"
```

Or from the shell (the repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically):

```bash
cargo add aes-gcm chacha20poly1305
```

```rust
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm,
};
use chacha20poly1305::ChaCha20Poly1305;

fn main() {
    // The library generates a correctly-sized random key for us.
    let key = Aes256Gcm::generate_key(&mut OsRng);
    let cipher = Aes256Gcm::new(&key);

    // A fresh 96-bit nonce. NEVER reuse one with the same key.
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let plaintext = b"transfer $100 to Bob";

    // Encrypt: the 16-byte auth tag is appended to the ciphertext automatically.
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_ref())
        .expect("encryption failure");

    // Decrypt: verifies the tag and only returns bytes if it matches.
    let decrypted = cipher
        .decrypt(&nonce, ciphertext.as_ref())
        .expect("decryption failure");

    assert_eq!(&decrypted, plaintext);
    println!("key:   {} bytes", key.len());
    println!("nonce: {} bytes", nonce.len());
    println!("ct:    {} bytes  (plaintext {} + 16-byte tag)", ciphertext.len(), plaintext.len());
    println!("plaintext recovered: {}", String::from_utf8_lossy(&decrypted));

    // Any tampering makes decryption FAIL — you get an Err, not garbage.
    let mut tampered = ciphertext.clone();
    tampered[0] ^= 0x01;
    match cipher.decrypt(&nonce, tampered.as_ref()) {
        Ok(_) => println!("tampered: ACCEPTED (bug!)"),
        Err(_) => println!("tampered: rejected (authentication failed)"),
    }

    // ChaCha20-Poly1305 is a drop-in alternative with the IDENTICAL trait API.
    let cck = ChaCha20Poly1305::generate_key(&mut OsRng);
    let cc = ChaCha20Poly1305::new(&cck);
    let cn = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let cct = cc.encrypt(&cn, b"same API".as_ref()).unwrap();
    println!("chacha: {}", String::from_utf8_lossy(&cc.decrypt(&cn, cct.as_ref()).unwrap()));
}
```

Real output:

```text
key:   32 bytes
nonce: 12 bytes
ct:    36 bytes  (plaintext 20 + 16-byte tag)
plaintext recovered: transfer $100 to Bob
tampered: rejected (authentication failed)
chacha: same API
```

The ciphertext is `36` bytes for a `20`-byte plaintext: the 16-byte Poly1305/GCM tag rides along inside it. There is no separate tag value to misplace, and switching from AES-GCM to ChaCha20-Poly1305 is a one-word change because both implement the same `Aead` trait.

---

## Detailed Explanation

### What "AEAD" buys you

**AEAD** = Authenticated Encryption with Associated Data. It provides two guarantees at once:

1. **Confidentiality**: an attacker who sees the ciphertext learns nothing about the plaintext.
2. **Integrity / authenticity**: if a single bit of the ciphertext (or the nonce, or the associated data) is changed, decryption **fails** rather than returning altered plaintext.

That second property is why you should never use a bare cipher like AES-CBC or AES-CTR on its own. Encryption without authentication is a classic vulnerability (padding-oracle and bit-flipping attacks). AEAD bundles a **Message Authentication Code (MAC)** into the construction so you can't forget it.

### Line-by-line

- **`Aes256Gcm::generate_key(&mut OsRng)`** produces a `Key` of exactly the right length (32 bytes for AES-256), drawn directly from the operating system's CSPRNG. You never type a key length or fill a buffer yourself, so you can't get it wrong. `rand::rng()` / `ThreadRng` is also a CSPRNG; this API uses `OsRng` because it is the explicit source accepted by the crypto library and has no forked userspace state to reseed. See [Secure Randomness](/27-security/06-secure-randomness/).
- **`Aes256Gcm::new(&key)`** builds a reusable cipher object bound to that key. The `new` method comes from the `KeyInit` trait, which is why that trait is in the `use` list.
- **`Aes256Gcm::generate_nonce(&mut OsRng)`** — a fresh **nonce** ("number used once"). For GCM it is 96 bits (12 bytes). The single most important rule of GCM: **never encrypt two different messages with the same `(key, nonce)` pair.** Doing so leaks plaintext relationships and can let an attacker forge the MAC.
- **`cipher.encrypt(&nonce, plaintext.as_ref())`** comes from the `Aead` trait. It returns a `Vec<u8>` that is `plaintext.len() + 16` bytes: the encrypted data with the authentication tag appended. The return type is `Result`, because encryption *can* fail (for example if the plaintext is too large for the construction).
- **`cipher.decrypt(&nonce, ciphertext.as_ref())`** recomputes and checks the tag. If verification fails it returns `Err(aead::Error)`; you never see corrupted plaintext.

### Why is there no separate tag, like in Node?

In Node, `aes-256-gcm` exposes the tag as a separate API surface (`getAuthTag`/`setAuthTag`) because the OpenSSL streaming model splits ciphertext and tag. The RustCrypto `Aead` trait deliberately hides that split: `encrypt` returns "ciphertext + tag" as one byte string and `decrypt` consumes it as one. The result is an API where the integrity check is **not optional**: there is no method that returns plaintext without verifying the tag first.

### AES-GCM vs ChaCha20-Poly1305

Both are modern AEADs and both are fine choices. The practical difference is performance characteristics:

- **AES-256-GCM** is fastest on hardware with AES-NI instructions (essentially all modern x86-64 and ARM server CPUs).
- **ChaCha20-Poly1305** is a constant-time software cipher that is fast *everywhere*, including older mobile/embedded CPUs without AES acceleration. It is also more forgiving of nonce sizing in its `XChaCha20Poly1305` variant (a 192-bit nonce, large enough to pick at random without birthday-bound worries).

Because they share the same trait API, you can pick one and swap later with a near-trivial diff.

---

## Key Differences

| Concern | Node `crypto` (TypeScript/JavaScript) | Rust (RustCrypto / `ring`) |
| --- | --- | --- |
| Underlying engine | OpenSSL (C) | Pure Rust (`aes-gcm`) or vendored BoringSSL (`ring`) |
| Algorithm selection | Magic string `"aes-256-gcm"` | A concrete type `Aes256Gcm`; typos are compile errors |
| Auth tag handling | Separate `getAuthTag`/`setAuthTag` you can forget | Folded into the ciphertext; not optional |
| Key generation | `randomBytes(32)` (you pick the length) | `Aes256Gcm::generate_key` (length is implied by the type) |
| Nonce reuse protection | None — your responsibility | None at the type level either; use `generate_nonce` per message |
| Failure on tampering | `final()` throws | `decrypt` returns `Err` |
| Algorithm agility | Strings make swapping easy but unsafe | The `Aead` trait makes swapping a one-word, type-checked change |

> **Note:** Neither ecosystem stops you from reusing a nonce; that is a property of the GCM/Poly1305 math, not the language. The defense is discipline: always derive nonces from `generate_nonce`/`OsRng`, or use a counter you are certain never repeats per key. When in doubt, prefer `XChaCha20Poly1305`'s 192-bit random nonces.

### The deeper philosophy: misuse-resistant APIs

A recurring theme in Rust crypto crates is **misuse resistance**: designing the API so the *easy* path is the *correct* path. Node's `crypto` is a thin, faithful wrapper over OpenSSL: powerful, but it will let you do almost anything, including unsafe things, without complaint. RustCrypto leans the other way: it exposes a small set of high-level AEAD constructions and makes the dangerous low-level pieces (raw block ciphers, ECB mode, unauthenticated CTR) harder to reach and clearly labeled. This mirrors Rust's broader ethos seen throughout this guide — make invalid states unrepresentable.

---

## Common Pitfalls

### Pitfall 1: Passing a raw `&[u8]` where a typed `Key` is expected

A natural mistake is to read 32 bytes from a config file and hand them straight to `new`:

```rust
use aes_gcm::{aead::KeyInit, Aes256Gcm};

fn main() {
    // does not compile (error[E0308]: mismatched types)
    let raw_key: &[u8] = b"0123456789abcdef0123456789abcdef"; // 32 bytes
    let _cipher = Aes256Gcm::new(raw_key);
}
```

The real compiler error:

```text
error[E0308]: mismatched types
   --> src/main.rs:6:34
    |
  6 |     let _cipher = Aes256Gcm::new(raw_key);
    |                   -------------- ^^^^^^^ expected `&GenericArray<u8, UInt<..., ...>>`, found `&[u8]`
    |                   |
    |                   arguments to this function are incorrect
    |
    = note: expected reference `&GenericArray<u8, UInt<UInt<UInt<UInt<UInt<UInt<UTerm, B1>, B0>, B0>, B0>, B0>, B0>>`
               found reference `&[u8]`
...
help: call `Into::into` on this expression to convert `&[u8]` into `&GenericArray<...>`
    |
  6 |     let _cipher = Aes256Gcm::new(raw_key.into());
    |                                         +++++++
```

This is the type system *protecting* you: a `&[u8]` could be any length, but `Aes256Gcm` needs exactly 32 bytes. The fix is to convert through `Key::from_slice`, which panics loudly if the length is wrong (do this at startup, not per-request):

```rust
use aes_gcm::{aead::KeyInit, Aes256Gcm, Key};

fn main() {
    let raw_key: &[u8] = b"0123456789abcdef0123456789abcdef"; // 32 bytes
    let key = Key::<Aes256Gcm>::from_slice(raw_key); // panics if len != 32
    let _cipher = Aes256Gcm::new(key);
    println!("cipher constructed from a {}-byte key", raw_key.len());
}
```

### Pitfall 2: Reusing a nonce

This compiles and runs, but it is the cardinal sin of GCM:

```rust
// logically broken (compiles fine): the SAME nonce reused for two messages.
use aes_gcm::{aead::{Aead, KeyInit, OsRng, AeadCore}, Aes256Gcm};

fn main() {
    let cipher = Aes256Gcm::new(&Aes256Gcm::generate_key(&mut OsRng));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // generated ONCE...

    let a = cipher.encrypt(&nonce, b"message one".as_ref()).unwrap();
    let b = cipher.encrypt(&nonce, b"message two".as_ref()).unwrap(); // ...reused. BAD.
    println!("{} {}", a.len(), b.len()); // "works" but leaks information
}
```

The compiler can't catch this; it is a property of the cryptography, not the types. Always call `generate_nonce` *inside* your encrypt path, once per message. Treat a nonce as single-use.

### Pitfall 3: Treating the nonce as a secret (it isn't)

Newcomers sometimes try to protect the nonce like a key. The nonce is **public**: it travels with the ciphertext in the clear. What it must be is **unique per key**, not secret. The standard pattern is to prepend the 12-byte nonce to the ciphertext and store/transmit them together (see the Real-World Example below).

### Pitfall 4: Reaching for a bare block cipher or hash

If you find yourself adding the `aes` (not `aes-gcm`) crate, or building "encryption" out of `sha2` and XOR, stop. A raw block cipher is unauthenticated and operates on a single 16-byte block; chaining it yourself reinvents the very modes that have CVEs. Use an `Aead`. Likewise, a hash like SHA-256 is **not** encryption and **not** a password hash. See [Password Hashing](/27-security/04-password-hashing/).

### Pitfall 5: Comparing secrets with `==`

Comparing a received MAC or token against the expected value with `==` short-circuits on the first differing byte, which leaks timing information. Use a constant-time comparison (shown in Exercise 3). The AEAD `decrypt` path already does this internally; the trap is in code you write *around* it.

---

## Best Practices

- **Prefer a high-level AEAD.** Reach for `aes-gcm` (`Aes256Gcm`) or `chacha20poly1305` (`ChaCha20Poly1305` / `XChaCha20Poly1305`). For the absolute simplest "just encrypt this blob" need, consider the [`age`](https://crates.io/crates/age) crate or `ring`'s `aead` module.
- **Let the library make keys and nonces.** `generate_key` and `generate_nonce` with `OsRng` are correct by construction.
- **One nonce per message, never reused per key.** If you can't guarantee a counter never repeats across restarts, use random 192-bit nonces via `XChaCha20Poly1305`.
- **Bind context with associated data (AAD).** Pass non-secret context (user ID, record ID, version tag) as AAD so a ciphertext can't be replayed in a different context.
- **Derive subkeys from a master key with HKDF.** Don't reuse one key for everything; use `hkdf` with a distinct `info` string per purpose (shown in the Real-World Example).
- **Keep keys out of logs and zero them when done.** Wrap key material in `secrecy::SecretBox` / `zeroize::Zeroizing`; see [Secrets Management](/27-security/07-secrets-management/).
- **Pin and audit your crypto crates.** Crypto bugs are high-severity; run `cargo audit` against RUSTSEC; see [Security Audit](/27-security/08-security-audit/).
- **Never invent primitives.** If a design needs a novel construction, get it reviewed by a cryptographer. The mantra holds in every language: don't roll your own crypto.

---

## Real-World Example

A common production task: encrypt a sensitive database field (here, a credit-card number) with a key **derived** from a master secret, and store the result as a self-contained `nonce || ciphertext` blob bound to its owning user via associated data.

```toml
# Cargo.toml
[dependencies]
aes-gcm = "0.10.3"
hkdf = "0.13.0"
sha2 = "0.11.0"
```

```rust
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    Aes256Gcm, Key, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha256;

/// Derive a 32-byte AES-256 key from a master secret using HKDF-SHA256.
/// `info` separates keys for different purposes from the SAME master,
/// so the field-encryption key is independent from, say, session keys.
fn derive_key(master: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(salt), master);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .expect("32 bytes is a valid output length for HKDF-SHA256");
    okm
}

/// Encrypt `plaintext`, authenticating (but not encrypting) `aad`.
/// Returns a self-contained blob: 12-byte nonce followed by ciphertext+tag.
fn seal(cipher: &Aes256Gcm, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // fresh, per message
    let ciphertext = cipher
        .encrypt(&nonce, Payload { msg: plaintext, aad })
        .expect("encryption failure");
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ciphertext);
    out
}

/// Reverse `seal`. Returns `None` if the blob is too short OR if the
/// ciphertext / aad was tampered with (authentication failure).
fn open(cipher: &Aes256Gcm, sealed: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < 12 {
        return None;
    }
    let (nonce_bytes, ciphertext) = sealed.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher.decrypt(nonce, Payload { msg: ciphertext, aad }).ok()
}

fn main() {
    // In production `master` comes from a KMS/secret store, never source code.
    let master = b"a-very-long-master-secret-from-your-kms";
    let salt = b"app-v1-salt";

    let key_bytes = derive_key(master, salt, b"db-field-encryption");
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    // Bind the ciphertext to the user it belongs to.
    let user_aad = b"user_id=42";
    let card_number = b"4111 1111 1111 1111";
    let sealed = seal(&cipher, card_number, user_aad);
    // Never print `key_bytes` or the recovered plaintext: both are secrets.
    println!("stored blob: {} bytes (12 nonce + 19 plaintext + 16 tag)", sealed.len());

    // Decrypting with the correct user context succeeds.
    let recovered = open(&cipher, &sealed, user_aad);
    println!("open (right user) -> {}", recovered.as_deref() == Some(card_number));

    // Decrypting with a different user context FAILS — the blob can't be
    // replayed against another account, even with the same key.
    println!("open (wrong user) -> {:?}", open(&cipher, &sealed, b"user_id=99"));
}
```

Real output:

```text
stored blob: 47 bytes (12 nonce + 19 plaintext + 16 tag)
open (right user) -> true
open (wrong user) -> None
```

Three production patterns are at work here:

1. **Key derivation (HKDF).** One master secret yields many independent keys, one per `info` label. Rotating or compartmentalizing keys becomes a string change, not a new secret to provision.
2. **Self-contained blobs.** Prepending the nonce means the stored value carries everything `open` needs: no second column for the nonce, no second value to lose.
3. **Associated data as a binding.** Passing `user_id=42` as AAD makes the ciphertext usable *only* in that context. An attacker who copies user 42's encrypted field into user 99's row gets `None`, not a decrypted card number.

The example reports only lengths and success/failure. It deliberately never logs the derived key or recovered card number; production logs must not become a second secret store.

> **Tip:** When you migrate encryption schemes, version your AAD or salt (`app-v1-salt`, `app-v2-...`). Old blobs keep decrypting under the old derivation while new writes use the new one.

---

## Further Reading

- [RustCrypto AEADs](https://github.com/RustCrypto/AEADs): `aes-gcm`, `chacha20poly1305`, and the shared `aead` trait crate.
- [`aes-gcm` on docs.rs](https://docs.rs/aes-gcm) and [`chacha20poly1305` on docs.rs](https://docs.rs/chacha20poly1305): the exact APIs used here.
- [`ring` documentation](https://docs.rs/ring): the alternative library underpinning `rustls`.
- [`hkdf` on docs.rs](https://docs.rs/hkdf): HMAC-based key derivation.
- [Node.js `crypto` module](https://nodejs.org/api/crypto.html): the TypeScript/JavaScript baseline.
- Cross-links within this guide:
  - [Secure Randomness](/27-security/06-secure-randomness/) — where keys and nonces come from, when `ThreadRng` is appropriate, and when to prefer the direct OS source.
  - [Password Hashing](/27-security/04-password-hashing/) — why passwords need Argon2, not encryption.
  - [Secrets Management](/27-security/07-secrets-management/) — `secrecy` and `zeroize` for the key material itself.
  - [TLS/SSL with rustls](/27-security/05-tls-ssl/) — encryption *in transit*, complementing the *at rest* encryption here.
  - [Security Audit](/27-security/08-security-audit/) — keeping crypto crates patched via `cargo audit`.
  - [Input Validation](/27-security/00-input-validation/) and [SQL Injection Prevention](/27-security/01-sql-injection/) — sibling defenses in this section.
  - [Section 00: Introduction](/00-introduction/) · [Section 01: Getting Started](/01-getting-started/) · [Section 02: Basics](/02-basics/)
  - [Section 28: Production](/28-production/) — operating these services safely in production.

---

## Exercises

### Exercise 1: Encrypt-then-decrypt round trip

**Difficulty:** Beginner

**Objective:** Confirm you can encrypt and decrypt a message with ChaCha20-Poly1305 and that a fresh nonce is used.

**Instructions:** Add `chacha20poly1305 = "0.10.1"`. Write a `main` that generates a key and nonce, encrypts the bytes `b"top secret"`, decrypts them back, and asserts the result equals the original. Print the ciphertext length and the recovered string.

<details>
<summary>Solution</summary>

```rust
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305,
};

fn main() {
    let key = ChaCha20Poly1305::generate_key(&mut OsRng);
    let cipher = ChaCha20Poly1305::new(&key);
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);

    let message = b"top secret";
    let ciphertext = cipher.encrypt(&nonce, message.as_ref()).expect("encrypt");
    let recovered = cipher.decrypt(&nonce, ciphertext.as_ref()).expect("decrypt");

    assert_eq!(&recovered, message);
    println!("ciphertext: {} bytes", ciphertext.len());
    println!("recovered:  {}", String::from_utf8_lossy(&recovered));
}
```

**Output:**

```text
ciphertext: 26 bytes
recovered:  top secret
```

The ciphertext is `10` plaintext bytes plus the `16`-byte Poly1305 tag. The `Aead` trait API is identical to AES-GCM — only the type name changed.

</details>

### Exercise 2: Prove tamper-detection

**Difficulty:** Intermediate

**Objective:** Show that AEAD decryption rejects any modified ciphertext.

**Instructions:** Encrypt a message with `Aes256Gcm`. Then flip one bit of the ciphertext and attempt to decrypt it. Your program should print whether the *original* decrypts successfully and whether the *tampered* version is rejected — without ever panicking.

<details>
<summary>Solution</summary>

```rust
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm,
};

fn main() {
    let cipher = Aes256Gcm::new(&Aes256Gcm::generate_key(&mut OsRng));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, b"important audit log entry".as_ref())
        .expect("encrypt");

    // Original decrypts fine.
    let ok = cipher.decrypt(&nonce, ciphertext.as_ref()).is_ok();
    println!("original decrypts: {ok}");

    // Flip a single bit anywhere in the ciphertext.
    let mut tampered = ciphertext.clone();
    tampered[3] ^= 0b0000_1000;
    let rejected = cipher.decrypt(&nonce, tampered.as_ref()).is_err();
    println!("tampered rejected: {rejected}");
}
```

**Output:**

```text
original decrypts: true
tampered rejected: true
```

Because GCM authenticates the entire ciphertext, even a one-bit change makes the recomputed tag mismatch, so `decrypt` returns `Err`. You never receive altered plaintext — the integrity guarantee that AEAD is named for.

</details>

### Exercise 3: Constant-time tag comparison

**Difficulty:** Advanced

**Objective:** Implement a timing-safe equality check for two byte strings, the way you'd compare a received MAC or token.

**Instructions:** Add `subtle = "2.6.1"`. Write `tags_equal(a: &[u8], b: &[u8]) -> bool` that returns `false` immediately on a length mismatch but otherwise compares the bytes in **constant time** using the `subtle` crate (so it doesn't leak how many leading bytes matched). Demonstrate it on a matching pair, a same-length mismatching pair, and a different-length pair.

<details>
<summary>Solution</summary>

```rust playground
use subtle::ConstantTimeEq;

/// Compare two byte strings in constant time. Returns `false` for a length
/// mismatch (that fact isn't secret), and otherwise compares every byte so
/// the running time does not reveal where the first difference is.
fn tags_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into() // `subtle::Choice` -> bool
}

fn main() {
    let expected = [0xde, 0xad, 0xbe, 0xef];
    println!("match:    {}", tags_equal(&expected, &[0xde, 0xad, 0xbe, 0xef]));
    println!("mismatch: {}", tags_equal(&expected, &[0xde, 0xad, 0xbe, 0x00]));
    println!("len diff: {}", tags_equal(&expected, &[0xde, 0xad]));
}
```

**Output:**

```text
match:    true
mismatch: false
len diff: false
```

`ct_eq` returns a `subtle::Choice` (a wrapped `u8` of `0` or `1`) instead of a `bool`, specifically so the compiler cannot optimize the comparison into an early-exit branch. Converting it to `bool` with `.into()` happens only after the full comparison. Reach for `subtle` whenever you compare secret values by hand; the AEAD `decrypt` path already does this internally.

</details>
