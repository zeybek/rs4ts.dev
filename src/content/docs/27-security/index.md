---
title: "Rust Security Best Practices"
sidebar:
  label: "Overview"
description: "Secure Rust web services across the request lifecycle: input validation, SQL injection, XSS/CSRF, crypto, TLS, secrets, and auditing, mapped from Node habits."
---

This section is about shipping Rust services and applications that hold up under attack. It walks the full request lifecycle of a modern web service: validating untrusted input, talking to a database without opening a SQL-injection hole, rendering HTML and handling cookies without XSS or CSRF, encrypting and hashing correctly, terminating TLS, generating unguessable secrets, keeping those secrets out of logs and memory, and auditing the dependency tree they all rely on. The recurring theme is the same one that makes Rust attractive elsewhere: the type system can carry security guarantees the way it carries memory safety, so the safe path is usually the ergonomic one — and where it is not, this section shows you exactly where the language stops protecting you and your discipline takes over.

For a TypeScript/JavaScript developer the threat model is familiar (you have fought XSS, CSRF, and injection in Node before), but the tooling is new. Each topic maps a habit you already have — Zod schemas, `pg` parameterized queries, React auto-escaping, the npm `argon2` package, `process.env`, `npm audit` — onto its idiomatic Rust counterpart, and is explicit about where the analogy breaks down.

---

## What You'll Learn

- How to turn untrusted input into trustworthy values with **parse-don't-validate**, type-driven newtypes, and the `validator` crate.
- Why **parameterized queries** (SQLx bind parameters) are the only safe way to pass values to a database, and how to handle genuinely dynamic SQL with allowlists and `QueryBuilder`.
- How to defend Rust web apps against **XSS and CSRF** with auto-escaping templates, CSRF tokens, `SameSite` cookies, and a Content-Security-Policy header.
- How to do **cryptography** correctly with vetted crates (RustCrypto, `ring`), why you must never roll your own, and the basics of AEAD (authenticated encryption).
- How to **hash passwords** with Argon2 (and bcrypt) via the `PasswordHasher`/`PasswordVerifier` traits, including salting and verification.
- How to terminate **TLS** with rustls, load certificates and keys, and why rustls is the memory-safe default over OpenSSL bindings.
- How to generate **cryptographically secure randomness** with `rand`/`getrandom` and the OS CSPRNG, and how to avoid the non-secure generators.
- How to **manage secrets** with environment variables and secret stores, the `secrecy` and `zeroize` crates, and the rule "never log a secret."
- How to **audit dependencies** for known vulnerabilities with `cargo audit` (RUSTSEC) and enforce supply-chain policy with `cargo deny`.

---

## Topics

| Topic | Description |
| --- | --- |
| [Input Validation and Sanitization](/27-security/00-input-validation/) | Parse-don't-validate, type-driven validation with newtypes, and declarative checks with the `validator` crate. |
| [SQL Injection Prevention](/27-security/01-sql-injection/) | Always parameterize with SQLx bind params; never format SQL strings, and handle dynamic identifiers with allowlists. |
| [XSS and CSRF Protection](/27-security/02-xss-csrf/) | Output encoding with auto-escaping templates, CSRF tokens, `SameSite` cookies, and Content-Security-Policy. |
| [Cryptography Done Right](/27-security/03-cryptography/) | Use vetted crates (RustCrypto, `ring`); never roll your own; AEAD basics with AES-GCM and ChaCha20-Poly1305. |
| [Password Hashing](/27-security/04-password-hashing/) | Hashing passwords with Argon2 (and bcrypt): salting, the `PasswordHasher` trait, and constant-time verification. |
| [TLS/SSL with rustls](/27-security/05-tls-ssl/) | Terminating TLS with rustls, loading certificates and keys, and how rustls compares to OpenSSL. |
| [Secure Randomness](/27-security/06-secure-randomness/) | Cryptographically secure randomness with `rand` + `getrandom` and the OS CSPRNG, versus the non-CSPRNG choices. |
| [Secrets Management](/27-security/07-secrets-management/) | Loading secrets from env/secret stores, the `secrecy` and `zeroize` crates, and never logging secrets. |
| [Auditing Dependencies](/27-security/08-security-audit/) | Auditing the dependency tree with `cargo audit` (RUSTSEC) and `cargo deny`, plus supply-chain hygiene. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Encode validation rules in the type system so the compiler refuses to let unvalidated input flow downstream, and reach for `validator` when declarative field-level checks fit better.
- Recognize string-concatenated SQL as a defect on sight and rewrite it with bound parameters, including dynamic filters and sort columns.
- Render user-supplied content and forms in a Rust web app without exposing an XSS or CSRF hole, and set the cookie and CSP headers that harden the browser side.
- Choose an appropriate AEAD construction, let the library manage keys and nonces, and treat ciphertext as opaque, tamper-evident bytes.
- Store and verify passwords with Argon2, understand why a fast hash is the wrong tool, and produce self-describing salted hashes.
- Stand up an HTTPS service with rustls — both at the socket level and behind an Axum server — and reason about in-process versus edge TLS termination.
- Generate session tokens, API keys, salts, and nonces from a CSPRNG, and explain why `SmallRng`/`Math.random()`-style generators are unsafe for secrets.
- Hold secrets in wrapper types that refuse to print themselves and scrub their bytes on drop, and load them from the environment without leaking them into logs.
- Wire `cargo audit` and `cargo deny` into CI and respond to a RUSTSEC advisory in your dependency tree.

---

## Prerequisites

This section builds directly on the web and database material, and assumes you are comfortable with Rust's ownership and error-handling fundamentals:

- [Section 16: Web APIs](/16-web-apis/): Axum extractors, middleware, sessions, and authentication are the surface most of these defenses sit on.
- [Section 17: Database](/17-database/) — the SQLx and Diesel chapters that SQL-injection prevention extends.
- [Section 05: Ownership](/05-ownership/): moves and `Drop` underpin how `zeroize` scrubs secrets from memory.
- [Section 08: Error Handling](/08-error-handling/) — `Result` is how parsers and validators report failure.
- [Section 11: Async Programming](/11-async/): query builders, TLS sockets, and web handlers are lazy futures that need a runtime.

---

## Estimated Time

**12 hours**: roughly 7 hours of reading and worked examples plus 5 hours on the exercises. Security is best learned by attacking your own code, so budget time to run the injection and validation examples and watch the malicious payloads fail.

---

**Next:** [Section 28: Production →](/28-production/)
