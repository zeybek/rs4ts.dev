---
title: "Password Hashing"
description: "Hash passwords in Rust with Argon2 and bcrypt, slow and salted via PHC strings, mapping the Node argon2 package and explaining why SHA-256 is the wrong tool."
---

Storing user passwords is the one place where "just hash it" is dangerous advice. A fast hash like SHA-256 is *worse* than useless for passwords because attackers can try billions of guesses per second. This page shows how to do it right in Rust with **Argon2** (and **bcrypt**), and how it maps to the Node `argon2`/`bcrypt` packages you may already know.

---

## Quick Overview

A **password hash** is a deliberately *slow*, *salted*, one-way transformation of a password that you store instead of the password itself. When a user logs in, you re-run the function on their attempt and compare. The defining feature of a good password hash is that it is **memory-hard and tunably slow**, so a stolen database is expensive to brute-force.

For a TypeScript/JavaScript developer, the mental model is almost identical to the npm `argon2` package: you call a `hash` function that bakes a random **salt** into a self-describing string, and a `verify` function that reads the salt back out. In Rust this is built around the [`password-hash`](https://docs.rs/password-hash) crate's `PasswordHasher` and `PasswordVerifier` traits, with [`argon2`](https://docs.rs/argon2) as the recommended implementation.

> **Note:** This page is about *password* hashing specifically. For general-purpose cryptography (encryption, AEAD, MACs) see [Cryptography Done Right](/27-security/03-cryptography/); for the random number sources used to make salts see [Secure Randomness](/27-security/06-secure-randomness/).

---

## TypeScript/JavaScript Example

A typical Node service using the native [`argon2`](https://www.npmjs.com/package/argon2) package (the recommended choice over `bcryptjs` today):

```typescript
// npm install argon2
import argon2 from "argon2";

interface UserRecord {
  username: string;
  passwordHash: string; // store THIS, never the password
}

// Registration: hash the password for storage.
async function register(username: string, password: string): Promise<UserRecord> {
  // argon2.hash() defaults to Argon2id, generates a random salt,
  // and returns a self-describing PHC string.
  const passwordHash = await argon2.hash(password);
  return { username, passwordHash };
}

// Login: re-hash the attempt and compare in constant time.
async function verifyLogin(record: UserRecord, attempt: string): Promise<boolean> {
  // verify() reads the salt + parameters out of the stored hash.
  return argon2.verify(record.passwordHash, attempt);
}

const user = await register("alice", "s3cr3t-p@ssw0rd");
console.log(user.passwordHash);
// $argon2id$v=19$m=65536,t=3,p=4$8ykm3QdFyZnBmyrRKiy2MQ$y+Ds3M9MFMMBACIQHF/c2iZ5U5+oa8d2mS/nyuJ2Kt0

console.log(await verifyLogin(user, "s3cr3t-p@ssw0rd")); // true
console.log(await verifyLogin(user, "wrong")); // false
```

**Key points:**

- `argon2.hash(password)` returns a **PHC string** that embeds the algorithm, version, parameters, salt, *and* digest.
- You never store or manage the salt yourself; it travels inside the hash string.
- `argon2.verify(hash, attempt)` returns a `Promise<boolean>`; it does the constant-time comparison internally.

> **Warning:** Do **not** use Node's built-in `crypto.createHash("sha256")` or `md5` for passwords. Those are fast general-purpose hashes; the same warning applies to Rust's `sha2` crate. Password hashing needs a *purpose-built, slow* function.

---

## Rust Equivalent

The same registration/login flow with the `argon2` crate. Add the dependency first:

```bash
cargo add argon2 --features std
```

This pulls in `argon2 = "0.5"`, which re-exports the [`password-hash`](https://docs.rs/password-hash) crate (the traits, the salt type, and a CSPRNG-backed salt generator).

```rust
use argon2::{
    password_hash::{
        rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};

fn main() {
    let password = b"correct horse battery staple";

    // 1. Generate a random salt from the OS CSPRNG.
    let salt = SaltString::generate(&mut OsRng);

    // 2. Argon2::default() is Argon2id with current recommended parameters.
    let argon2 = Argon2::default();

    // 3. Hash into a self-describing PHC string (store THIS).
    let hash = argon2
        .hash_password(password, &salt)
        .expect("hashing failed")
        .to_string();
    println!("PHC string: {hash}");

    // 4. Verify: parse the stored string, then check the attempt.
    let parsed = PasswordHash::new(&hash).expect("invalid PHC string");
    let ok = Argon2::default()
        .verify_password(password, &parsed)
        .is_ok();
    println!("correct password verifies: {ok}");

    let wrong = Argon2::default()
        .verify_password(b"wrong password", &parsed)
        .is_ok();
    println!("wrong password verifies:   {wrong}");

    // 5. The same input hashed twice differs, because the salt is random.
    let salt2 = SaltString::generate(&mut OsRng);
    let hash2 = Argon2::default()
        .hash_password(password, &salt2)
        .unwrap()
        .to_string();
    println!("same input, hashes equal:  {}", hash == hash2);
}
```

Real output:

```text
PHC string: $argon2id$v=19$m=19456,t=2,p=1$hOtbciLFrd1ihAGsRiEoIg$RN2Tf/CQZ8OcIaVfm3vz0q+bujmy5+/vxzMamQ3t4D0
correct password verifies: true
wrong password verifies:   false
same input, hashes equal:  false
```

**Key points:**

- `hash_password` and `verify_password` come from the `PasswordHasher` / `PasswordVerifier` **traits**. You must bring them into scope with `use`.
- The output is a [PHC string](https://github.com/P-H-C/phc-string-format/blob/master/phc-sf-spec.md), exactly like the Node `argon2` package produces. It is a single `String` you put in one database column.
- Passwords are passed as `&[u8]` (byte slices), not `&str`. Calling `.as_bytes()` on a `String`/`&str` is how you get there.

---

## Detailed Explanation

### Why slow + salted, line by line

A password hash defends against two distinct attacks, and each design choice targets one of them:

1. **Salting defeats precomputation.** A **salt** is a random value mixed into the hash so that two users with the same password get different hashes. Without it, an attacker uses a *rainbow table* (a precomputed map of `hash → password`) and cracks the whole database at once. `SaltString::generate(&mut OsRng)` draws ~16 bytes from the operating system CSPRNG. That is why our two-hashes-of-the-same-password check printed `false`.

2. **Slowness defeats brute force.** `Argon2::default()` is configured to take meaningful CPU *and* memory per hash. The PHC parameters `m=19456,t=2,p=1` mean 19,456 KiB (19 MiB) of memory, 2 iterations, and 1 lane of parallelism. That is fast enough for a login request (single-digit milliseconds) but turns an attacker's "billions of guesses per second" on a fast hash into a far smaller number on commodity hardware. The 19 MiB memory cost specifically blunts GPU and ASIC cracking rigs.

### Anatomy of the PHC string

```text
$argon2id$v=19$m=19456,t=2,p=1$hOtbciLFrd1ihAGsRiEoIg$RN2Tf/CQZ8OcIaVfm3vz0q+bujmy5+/vxzMamQ3t4D0
 └─ algo ─┘└ver┘└──── params ────┘└──── salt (b64) ────┘└─────────── digest (b64) ───────────────┘
```

Because the salt and parameters are stored *inside* the string, `verify_password` can reproduce the exact computation. You never need a separate `salt` column, and you can change your global parameters later without breaking existing users — `verify` uses whatever is embedded in each stored hash.

### `Argon2id` vs `Argon2i` vs `Argon2d`

Argon2 has three variants. `Argon2::default()` picks **Argon2id**, the hybrid that the [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html) recommends for password storage — it resists both side-channel attacks (the `i` strength) and GPU/time-memory tradeoff attacks (the `d` strength). Unless you have a specific reason, use the default.

### Tuning the cost parameters

`Argon2::default()` is a good baseline, but you may want to set parameters explicitly so they are visible and reviewable:

```rust
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Algorithm, Argon2, Params, Version,
};

fn main() {
    // m_cost = 19456 KiB (19 MiB), t_cost = 2 iterations, p_cost = 1 lane.
    // These are a current OWASP-recommended starting point.
    let params = Params::new(19_456, 2, 1, None).expect("invalid params");
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2
        .hash_password(b"hunter2", &salt)
        .expect("hash failed")
        .to_string();
    println!("{hash}");
}
```

Real output:

```text
$argon2id$v=19$m=19456,t=2,p=1$nDrBHfbTwksob591a0f2XA$wqAf/QsgYOxUHbz/VRWJJs9dlmQt86VXYWmEvTfT5tg
```

> **Tip:** Tune so that a single hash takes roughly **0.5–1 second** on *your* production hardware, then back off if that hurts throughput. The right numbers depend on your CPU and how many logins per second you must serve. Benchmark; don't cargo-cult a number.

### The bcrypt alternative

bcrypt predates Argon2 and is still perfectly acceptable for password storage; it is what many existing systems use. The [`bcrypt`](https://docs.rs/bcrypt) crate has a simpler, function-style API:

```bash
cargo add bcrypt
```

```rust
use bcrypt::{hash, verify, DEFAULT_COST};

fn main() {
    let password = "correct horse battery staple";

    // hash() generates a random salt and embeds it in the output.
    let hashed = hash(password, DEFAULT_COST).expect("hashing failed");
    println!("bcrypt hash: {hashed}");
    println!("cost (DEFAULT_COST) = {DEFAULT_COST}");

    println!("correct verifies: {}", verify(password, &hashed).unwrap());
    println!("wrong verifies:   {}", verify("nope", &hashed).unwrap());
}
```

Real output:

```text
bcrypt hash: $2b$12$Ttcq3h2TaM9ZeEWCGgiVge5yj33FaydsAeRyhyVQWoxTX/K5YW4Ru
cost (DEFAULT_COST) = 12
correct verifies: true
wrong verifies:   false
```

The `$2b$12$...` prefix is bcrypt's own self-describing format: variant `2b`, cost factor `12` (meaning 2^12 rounds), then salt+digest. The cost is the one knob — raise it as hardware gets faster.

> **Warning:** bcrypt silently **truncates passwords to 72 bytes**. Anything past byte 72 is ignored, so a 100-character passphrase is no stronger than its first 72 bytes. Argon2 has no such limit. This is a real, frequently-overlooked footgun (demonstrated under *Common Pitfalls*).

---

## Key Differences

| Concern | TypeScript / Node | Rust |
| --- | --- | --- |
| Recommended package | `argon2` (npm) | `argon2` crate |
| Default algorithm | Argon2id | Argon2id (`Argon2::default()`) |
| API shape | async functions `hash`/`verify` | trait methods `hash_password`/`verify_password` (synchronous) |
| Where the salt lives | inside the PHC string | inside the PHC string |
| Output type | `Promise<string>` | `Result<PasswordHash, Error>` → `.to_string()` |
| Password input | `string` | `&[u8]` (use `.as_bytes()`) |
| Comparison safety | constant-time inside `verify` | constant-time inside `verify_password` |
| CPU binding | native addon (libsodium-style) | pure Rust, no system libs |

A few conceptual differences worth internalizing:

- **Synchronous, not async.** Node's `argon2.hash` returns a `Promise` because the native addon offloads work to a thread pool. The Rust `argon2` crate is a plain synchronous CPU computation. In an async service (axum, tokio) you should wrap a hash call in [`tokio::task::spawn_blocking`](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) so the ~1 ms–1 s of CPU work does not stall the async runtime's worker thread. See [Async](/11-async/) for the blocking-vs-async distinction.

- **Traits, not free functions.** `hash_password` lives on the `PasswordHasher` trait. If you forget the `use`, the method appears to not exist (see *Common Pitfalls*). This is Rust's normal "methods come from traits in scope" rule — see [Generics & Traits](/09-generics-traits/).

- **Errors are values.** Node throws on a malformed hash; Rust returns `Result`. A failed verify is `Err(...)`, not an exception, so you handle it with `match`/`?` like any other [error handling](/08-error-handling/).

- **No global state.** There is no implicit "pepper" or process-wide config. Everything that affects a hash is either in the PHC string or in the `Argon2` value you constructed.

---

## Common Pitfalls

### Forgetting to import the trait

The methods come from `PasswordHasher` / `PasswordVerifier`. Omit the `use` and the compiler says the method does not exist:

```rust
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2,
};

fn main() {
    let salt = SaltString::generate(&mut OsRng);
    // does not compile (error[E0599]) — PasswordHasher trait not in scope
    let _hash = Argon2::default().hash_password(b"pw", &salt).unwrap();
}
```

Real compiler error (truncated):

```text
error[E0599]: no method named `hash_password` found for struct `Argon2` in the current scope
   --> src/main.rs:9:35
    |
  9 |     let _hash = Argon2::default().hash_password(b"pw", &salt).unwrap();
    |                                   ^^^^^^^^^^^^^
...
    = help: items from traits can only be used if the trait is in scope
help: trait `PasswordHasher` which provides `hash_password` is implemented but not in scope; perhaps you want to import it
    |
  1 + use argon2::PasswordHasher;
    |
```

The fix is exactly what the compiler suggests: `use argon2::PasswordHasher;` (and `PasswordVerifier` for verifying).

### Comparing hashes with `==` instead of verifying

A tempting but wrong instinct is to re-hash the attempt and compare strings:

```rust
// Logic bug, NOT a compile error — this will reject every correct password.
// let attempt_hash = Argon2::default()
//     .hash_password(attempt, &fresh_salt)?
//     .to_string();
// let ok = attempt_hash == stored_hash; // ALWAYS false: different random salts!
```

Because each hash uses a *new random salt*, two hashes of the same password never match by string equality — that is the whole point of salting. You must call `verify_password`, which re-uses the salt embedded in the stored hash and performs a **constant-time** digest comparison. String `==` would also leak timing information even if the salts matched.

### Using a fast general-purpose hash

```rust
// Insecure for passwords (compiles fine, ships a vulnerability).
// use sha2::{Digest, Sha256};
// let digest = Sha256::digest(password); // GPU-crackable at billions/sec, no salt
```

SHA-256, SHA-512, and MD5 are designed to be *fast*, which is precisely the wrong property here. They are correct for file integrity and HMACs, never for passwords. Reach for `argon2` (or `bcrypt`) instead.

### bcrypt's 72-byte truncation

```rust
use bcrypt::{hash, verify, DEFAULT_COST};

fn main() {
    let base = "a".repeat(72);
    let longer = format!("{base}EXTRA-IGNORED-BYTES");

    let h = hash(&base, DEFAULT_COST).unwrap();
    // Extra bytes past 72 are ignored, so a DIFFERENT password verifies.
    println!(
        "72-byte password verifies with +18 extra bytes: {}",
        verify(&longer, &h).unwrap()
    );
}
```

Real output:

```text
72-byte password verifies with +18 extra bytes: true
```

If you must support long passphrases with bcrypt, pre-hash with SHA-256 and base64-encode before bcrypt, or simply use Argon2, which has no length limit.

### Logging or returning the hash

A PHC string is not a secret you should display, but it is also not something to scatter through logs. Treat it like any credential material: do not log it, do not return it in API responses. For active in-memory secrets, see [Secrets Management](/27-security/07-secrets-management/).

---

## Best Practices

- **Default to Argon2id.** Use `Argon2::default()` (or explicit `Algorithm::Argon2id`) unless you are interoperating with an existing bcrypt store.
- **Never manage salts manually.** Let `SaltString::generate(&mut OsRng)` and the PHC string handle it. A salt you generate with a non-CSPRNG (e.g. the default `rand` thread RNG without `OsRng`) is a bug — see [Secure Randomness](/27-security/06-secure-randomness/).
- **Store the whole PHC string in one column.** No separate salt/params columns. This makes parameter upgrades trivial.
- **Run hashing off the async executor.** In tokio/axum services, wrap `hash_password`/`verify_password` in `spawn_blocking` so a slow hash does not block other requests.
- **Re-hash on login when parameters change.** After a successful `verify_password`, check whether the stored hash used your *current* parameters; if not, transparently re-hash the just-verified plaintext and update the row. This lets you raise cost over time without forcing password resets.
- **Compare in constant time.** Always go through `verify_password` (Argon2) or `verify` (bcrypt); never `==` on hashes or digests.
- **Pin and audit your dependencies.** Password hashing is exactly the kind of code where a known-vulnerable transitive dependency matters; run `cargo audit` (see [Auditing Dependencies and Supply-Chain Hygiene](/27-security/08-security-audit/)).
- **Cap input length before hashing.** Reject absurdly long passwords (e.g. > 1 KiB) at the validation layer to avoid a denial-of-service where an attacker submits megabyte passwords to your slow hasher — see [Input Validation and Sanitization](/27-security/00-input-validation/).

---

## Real-World Example

A small, production-flavored auth module with a typed error enum. It models a `users` table row, hashes on registration, and verifies on login, returning a deliberately generic error so the response cannot distinguish "no such user" from "wrong password".

```bash
cargo add argon2 --features std
cargo add thiserror
```

```rust
use argon2::{
    password_hash::{
        rand_core::OsRng, Error as PwHashError, PasswordHash, PasswordHasher,
        PasswordVerifier, SaltString,
    },
    Argon2,
};
use thiserror::Error;

#[derive(Debug, Error)]
enum AuthError {
    #[error("could not hash password")]
    Hash(#[source] PwHashError),
    #[error("stored credential is corrupt")]
    CorruptHash(#[source] PwHashError),
    #[error("invalid username or password")]
    BadCredentials,
}

/// Stand-in for a `users` table row.
struct UserRecord {
    username: String,
    password_hash: String, // the PHC string, safe to store in a DB column
}

/// Hash a new user's password for storage.
fn register(username: &str, password: &str) -> Result<UserRecord, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(AuthError::Hash)?
        .to_string();

    Ok(UserRecord {
        username: username.to_owned(),
        password_hash,
    })
}

/// Check a login attempt against a stored record.
fn verify_login(record: &UserRecord, attempt: &str) -> Result<(), AuthError> {
    // A malformed stored hash is a server fault, distinct from a bad password.
    let parsed =
        PasswordHash::new(&record.password_hash).map_err(AuthError::CorruptHash)?;

    Argon2::default()
        .verify_password(attempt.as_bytes(), &parsed)
        // Collapse any verify failure into ONE generic error: never reveal which part failed.
        .map_err(|_| AuthError::BadCredentials)
}

fn main() {
    let user = register("alice", "s3cr3t-p@ssw0rd").expect("register failed");
    println!("stored for {}: {}", user.username, user.password_hash);

    match verify_login(&user, "s3cr3t-p@ssw0rd") {
        Ok(()) => println!("login ok"),
        Err(e) => println!("login failed: {e}"),
    }

    match verify_login(&user, "guess") {
        Ok(()) => println!("login ok"),
        Err(e) => println!("login failed: {e}"),
    }
}
```

Real output (PHC body redacted here for length; it is a full base64 salt+digest at runtime):

```text
stored for alice: $argon2id$v=19$m=19456,t=2,p=1$<salt>$<digest>
login ok
login failed: invalid username or password
```

In a real service the `UserRecord` would come from a database — see [Databases](/17-database/) — and the `register`/`verify_login` calls would be wrapped in `spawn_blocking` inside your axum handlers (see [Web APIs](/16-web-apis/)). The generic `BadCredentials` error is intentional: returning the same message and status for unknown-user and wrong-password prevents username enumeration, and you should also keep the *timing* of both paths similar in production hardening (see [Production](/28-production/)).

> **Note:** This page presents `argon2 = "0.5"`, `bcrypt = "0.19"`, and `thiserror = "2"`. The current stable toolchain is Rust 1.96.0 on the 2024 edition, which `cargo new` selects automatically. Always run `cargo add <crate>` to resolve the latest compatible versions rather than copying pins.

---

## Further Reading

- [`argon2` crate docs](https://docs.rs/argon2): the recommended password hasher.
- [`password-hash` crate docs](https://docs.rs/password-hash): the `PasswordHasher`/`PasswordVerifier` traits and the PHC string types shared across RustCrypto hashers.
- [`bcrypt` crate docs](https://docs.rs/bcrypt): the bcrypt alternative.
- [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html): current parameter recommendations.
- [PHC string format spec](https://github.com/P-H-C/phc-string-format/blob/master/phc-sf-spec.md): the structure of the stored hash string.
- Related sections of this guide:
  - [Secure Randomness](/27-security/06-secure-randomness/): where salts come from (`OsRng`).
  - [Cryptography Done Right](/27-security/03-cryptography/): general crypto; why password hashing is *not* encryption.
  - [Secrets Management](/27-security/07-secrets-management/): handling secrets in memory.
  - [Input Validation and Sanitization](/27-security/00-input-validation/): capping password length before hashing.
  - [Auditing Dependencies and Supply-Chain Hygiene](/27-security/08-security-audit/) — keeping these crates patched.
  - [Error Handling](/08-error-handling/) and [Generics & Traits](/09-generics-traits/) — the `Result` and trait mechanics used above.

---

## Exercises

### Exercise 1: Round-trip a password

**Difficulty:** Easy

**Objective:** Get comfortable with the `hash_password` → store → `verify_password` cycle and confirm salting works.

**Instructions:** Write a program that hashes the password `"hunter2"` with `Argon2::default()` and a freshly generated salt, prints the PHC string, and then verifies *both* `"hunter2"` (should succeed) and `"Hunter2"` (should fail). As a final check, hash `"hunter2"` a second time and assert the two PHC strings are different. Fill in the `/* ??? */` parts:

```typescript
// The TypeScript you are translating:
import argon2 from "argon2";
const hash = await argon2.hash("hunter2");
console.log(await argon2.verify(hash, "hunter2")); // true
console.log(await argon2.verify(hash, "Hunter2")); // false
```

<details>
<summary>Solution</summary>

```rust
// cargo add argon2 --features std
use argon2::{
    password_hash::{
        rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};

fn main() {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(b"hunter2", &salt)
        .unwrap()
        .to_string();
    println!("{hash}");

    let parsed = PasswordHash::new(&hash).unwrap();
    println!(
        "hunter2 -> {}",
        Argon2::default().verify_password(b"hunter2", &parsed).is_ok()
    );
    println!(
        "Hunter2 -> {}",
        Argon2::default().verify_password(b"Hunter2", &parsed).is_ok()
    );

    let salt2 = SaltString::generate(&mut OsRng);
    let hash2 = Argon2::default()
        .hash_password(b"hunter2", &salt2)
        .unwrap()
        .to_string();
    assert_ne!(hash, hash2, "random salts must produce different hashes");
    println!("two hashes differ: {}", hash != hash2);
}
```

Running this prints a PHC string, then `hunter2 -> true`, `Hunter2 -> false`, and `two hashes differ: true`.

</details>

### Exercise 2: A reusable, parameter-aware hasher

**Difficulty:** Medium

**Objective:** Wrap Argon2 behind a small struct with explicit cost parameters and clean error handling.

**Instructions:** Define a `Hasher` struct that owns an `Argon2<'static>` configured with `Params::new(19_456, 2, 1, None)` and `Algorithm::Argon2id`. Give it two methods: `hash(&self, password: &str) -> Result<String, argon2::password_hash::Error>` and `verify(&self, password: &str, stored: &str) -> Result<bool, argon2::password_hash::Error>` (return `Ok(true)`/`Ok(false)` for match/mismatch, and propagate only *structural* errors such as a corrupt PHC string). Demonstrate it on `"pa$$w0rd"`.

<details>
<summary>Solution</summary>

```rust
// cargo add argon2 --features std
use argon2::{
    password_hash::{
        rand_core::OsRng, Error, PasswordHash, PasswordHasher, PasswordVerifier,
        SaltString,
    },
    Algorithm, Argon2, Params, Version,
};

struct Hasher {
    inner: Argon2<'static>,
}

impl Hasher {
    fn new() -> Self {
        let params = Params::new(19_456, 2, 1, None).expect("valid params");
        Hasher {
            inner: Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
        }
    }

    fn hash(&self, password: &str) -> Result<String, Error> {
        let salt = SaltString::generate(&mut OsRng);
        Ok(self
            .inner
            .hash_password(password.as_bytes(), &salt)?
            .to_string())
    }

    fn verify(&self, password: &str, stored: &str) -> Result<bool, Error> {
        // A bad PHC string is a real error; a wrong password is just `Ok(false)`.
        let parsed = PasswordHash::new(stored)?;
        match self.inner.verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            Err(Error::Password) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

fn main() -> Result<(), Error> {
    let hasher = Hasher::new();
    let stored = hasher.hash("pa$$w0rd")?;
    println!("{stored}");
    println!("right: {}", hasher.verify("pa$$w0rd", &stored)?); // true
    println!("wrong: {}", hasher.verify("nope", &stored)?); // false
    Ok(())
}
```

The key idea is distinguishing a *wrong password* (`Error::Password` → `Ok(false)`, a normal control-flow outcome) from a *structural failure* (a corrupt stored hash → propagated `Err`). This mirrors how you would surface a `401` vs a `500` in a web service.

</details>

### Exercise 3: Detect outdated hashes for transparent rehashing

**Difficulty:** Hard

**Objective:** Implement the "upgrade cost over time" best practice: after a successful login, decide whether a stored hash used weaker-than-current parameters and should be re-hashed.

**Instructions:** Write `needs_rehash(stored: &str, current: &Params) -> bool` that parses the stored PHC string, reads its embedded Argon2 `Params`, and returns `true` if the stored memory cost (`m_cost`) or iteration count (`t_cost`) is lower than `current`'s. Use `argon2::Params::try_from(&PasswordHash)` to recover the parameters. Test it with a hash made at `m=8,t=1` against a current policy of `m=19_456,t=2`.

> **Hint:** `Params::try_from(&password_hash)` returns the parameters encoded in the PHC string. Compare `.m_cost()` and `.t_cost()`.

<details>
<summary>Solution</summary>

```rust
// cargo add argon2 --features std
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, SaltString},
    Algorithm, Argon2, Params, Version,
};

fn needs_rehash(stored: &str, current: &Params) -> bool {
    let parsed = match PasswordHash::new(stored) {
        Ok(p) => p,
        Err(_) => return true, // unparseable -> force a rehash on next login
    };
    match Params::try_from(&parsed) {
        Ok(used) => used.m_cost() < current.m_cost() || used.t_cost() < current.t_cost(),
        Err(_) => true,
    }
}

fn main() {
    // An old, weak hash: 8 KiB memory, 1 iteration.
    let weak_params = Params::new(8, 1, 1, None).unwrap();
    let weak = Argon2::new(Algorithm::Argon2id, Version::V0x13, weak_params)
        .hash_password(b"pw", &SaltString::generate(&mut OsRng))
        .unwrap()
        .to_string();

    // Current policy: 19 MiB memory, 2 iterations.
    let current = Params::new(19_456, 2, 1, None).unwrap();

    println!("weak hash needs rehash: {}", needs_rehash(&weak, &current)); // true

    // A hash made at current policy does NOT need a rehash.
    let strong = Argon2::new(Algorithm::Argon2id, Version::V0x13, current.clone())
        .hash_password(b"pw", &SaltString::generate(&mut OsRng))
        .unwrap()
        .to_string();
    println!("strong hash needs rehash: {}", needs_rehash(&strong, &current)); // false
}
```

In a real login handler you would call `needs_rehash` *after* a successful `verify_password`, and if it returns `true`, re-hash the plaintext you just verified (with current parameters) and update the database row — upgrading every active user's security without a forced password reset.

</details>
