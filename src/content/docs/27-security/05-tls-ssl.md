---
title: "TLS/SSL with rustls"
description: "Terminate TLS in Rust with rustls instead of Node's OpenSSL: load certs and keys, secure-by-default TLS 1.3, with tokio-rustls and an Axum server."
---

In Node.js you reach for the built-in `https`/`tls` modules, which wrap OpenSSL, and in production you usually let a reverse proxy (nginx, a cloud load balancer) terminate TLS for you. Rust's ecosystem offers the same two options (terminate TLS in-process or behind a proxy), but the in-process story is dominated by **rustls**, a memory-safe TLS library written in Rust that does *not* depend on OpenSSL at all. This topic shows how to terminate TLS with rustls (both at the raw socket level and inside an Axum server), how certificates and private keys are loaded, and how rustls differs from the OpenSSL bindings you may know from Node.

---

## Quick Overview

**Transport Layer Security (TLS)**, the protocol behind every `https://` URL, encrypts the bytes flowing between a client and a server and authenticates the server (and optionally the client) with X.509 certificates. **rustls** is the de-facto Rust TLS stack: it implements TLS 1.2 and 1.3 in safe Rust, ships modern defaults (TLS 1.3 preferred, no insecure ciphers to misconfigure), and plugs a swappable cryptography backend (`aws-lc-rs` by default, `ring` as an alternative) underneath. For a TypeScript/JavaScript developer, the big mental shifts are: TLS configuration in Rust is explicit and type-checked rather than a bag of OpenSSL options, and "secure by default" is enforced by the library: there is no `SSLv3` or `RC4` knob to turn on by accident.

> **Note:** The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024), which `cargo new` selects automatically. The crates below resolve to rustls 0.23, tokio-rustls 0.26, axum 0.8, axum-server 0.8, reqwest 0.13, and rcgen 0.14 at the time of writing.

---

## TypeScript/JavaScript Example

In Node.js, terminating TLS means loading a PEM certificate and key, then handing them to `https.createServer`. Node's TLS layer is a thin wrapper over OpenSSL.

```typescript
// server.ts — HTTPS server in Node.js (built on OpenSSL)
import https from "node:https";
import { readFileSync } from "node:fs";

const options = {
  cert: readFileSync("cert.pem"),
  key: readFileSync("key.pem"),
  // You *can* reach into OpenSSL knobs here — and footgun yourself:
  // minVersion: "TLSv1",          // insecure if you enable it
  // ciphers: "ALL",               // would re-enable weak ciphers
  minVersion: "TLSv1.2",           // already the Node default; shown explicitly
};

const server = https.createServer(options, (req, res) => {
  res.writeHead(200, { "Content-Type": "application/json" });
  res.end(JSON.stringify({ status: "ok" }));
});

server.listen(8443, () => {
  console.log("HTTPS server on https://localhost:8443");
});
```

On the client side, the global `fetch` (or `https.request`) validates the server certificate against the OS/Node root store automatically. The escape hatch everyone eventually finds — and must never ship — is disabling verification:

```typescript
// DO NOT do this in production: it disables certificate verification globally.
process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
await fetch("https://localhost:8443/health"); // now accepts ANY certificate
```

**Key points:**

- Node's TLS is OpenSSL under the hood; security depends on you *not* loosening defaults.
- Weak protocol versions and ciphers are reachable through options.
- Certificate verification is on by default, but trivially disabled with one env var.

---

## Rust Equivalent

The idiomatic in-process approach uses **rustls**. Here is a complete server that terminates TLS on a raw TCP socket with `tokio-rustls` and writes a minimal HTTP response: the lowest-level view, so you can see exactly what TLS termination is.

```rust
// Cargo.toml dependencies:
//   cargo add tokio --features full
//   cargo add tokio-rustls
//   cargo add rustls
//   cargo add rustls-pemfile
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// Load a PEM-encoded certificate chain from disk.
fn load_certs(path: &str) -> std::io::Result<Vec<CertificateDer<'static>>> {
    let mut reader = BufReader::new(File::open(path)?);
    rustls_pemfile::certs(&mut reader).collect()
}

/// Load a PEM-encoded private key (PKCS#8, PKCS#1, or SEC1) from disk.
fn load_key(path: &str) -> std::io::Result<PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(File::open(path)?);
    rustls_pemfile::private_key(&mut reader)
        .map(|opt| opt.expect("no private key found in file"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install the default crypto provider once at process startup.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install default crypto provider");

    let certs = load_certs("cert.pem")?;
    let key = load_key("key.pem")?;

    // Build the server-side TLS configuration: present our chain + key,
    // and (because this is a public server) require no client certificate.
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    let acceptor = TlsAcceptor::from(Arc::new(config));

    let listener = TcpListener::bind("127.0.0.1:8443").await?;
    println!("listening on https://127.0.0.1:8443");

    loop {
        let (tcp, peer) = listener.accept().await?;
        let acceptor = acceptor.clone();

        // Each connection is handled on its own task; the TLS handshake
        // runs inside the task so a slow client can't block the accept loop.
        tokio::spawn(async move {
            match acceptor.accept(tcp).await {
                Ok(mut tls) => {
                    let mut buf = [0u8; 1024];
                    let _ = tls.read(&mut buf).await; // read the request
                    let body = "Hello over TLS!";
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = tls.write_all(resp.as_bytes()).await;
                    let _ = tls.shutdown().await;
                }
                Err(e) => eprintln!("TLS handshake with {peer} failed: {e}"),
            }
        });
    }
}
```

With a self-signed certificate for `localhost` in `cert.pem`/`key.pem` (we generate one in the Real-World Example below), running the server and connecting with `curl` produces real output:

```text
$ cargo run
listening on https://127.0.0.1:8443

$ curl --cacert cert.pem --resolve localhost:8443:127.0.0.1 https://localhost:8443/
Hello over TLS!
```

Inspecting the handshake with `curl -v` confirms rustls negotiated TLS 1.3 by default, no configuration required:

```text
* SSL connection using TLSv1.3 / AEAD-CHACHA20-POLY1305-SHA256
*  subject: CN=rcgen self signed cert
*  subjectAltName: host "localhost" matched cert's "localhost"
*  issuer: CN=rcgen self signed cert
```

**Notice what is *absent*:** there is no place to enable SSLv3, no cipher string to fat-finger, and no global "reject unauthorized" flag to disable. rustls simply does not implement the insecure options.

---

## Detailed Explanation

Going line by line through the Rust server and contrasting with Node:

- **The crypto provider.** rustls separates the *protocol* (handshake state machine, record layer) from the *cryptography* (AES-GCM, ChaCha20-Poly1305, ECDSA, X25519). The cryptographic primitives come from a **`CryptoProvider`**: by default `aws-lc-rs` (a Rust binding to AWS's vetted libcrypto fork; the alternative is `ring`). Calling `rustls::crypto::aws_lc_rs::default_provider().install_default()` once at startup registers it process-wide. In Node you never see this seam because OpenSSL is baked in.

- **Loading certificates and keys.** `rustls_pemfile::certs` parses a PEM file into a `Vec<CertificateDer>` (the certificate *chain*: your leaf cert first, then any intermediates). `rustls_pemfile::private_key` parses the matching private key, transparently handling PKCS#8, PKCS#1 (RSA), and SEC1 (EC) encodings. These are strongly typed. `CertificateDer` and `PrivateKeyDer` are distinct types, so you cannot accidentally swap them. In Node both are just `Buffer`s.

- **`ServerConfig::builder()`** is a *type-state builder*. You must answer "do you require a client certificate?" (`with_no_client_auth()` for ordinary public servers, or `with_client_cert_verifier(...)` for mutual TLS) *before* you can call `with_single_cert(certs, key)`. The compiler enforces the order, so there is no half-configured server. The result is wrapped in an `Arc` and shared across every connection.

- **`TlsAcceptor::accept(tcp)`** performs the TLS handshake over an accepted TCP stream and yields a `TlsStream` that implements `AsyncRead`/`AsyncWrite`. From that point on you read and write plaintext; rustls encrypts/decrypts transparently. This is the exact analogue of what `https.createServer` does for you in Node, just made explicit.

- **Per-connection tasks.** Spawning the handshake inside `tokio::spawn` matters: the TLS handshake involves a round trip, so doing it inline in the accept loop would let one slow client stall all new connections, the Rust equivalent of blocking the event loop. See [Section 11: Async](/11-async/) for the runtime model.

By contrast, the Node version delegates all of this to OpenSSL, and the security posture depends on which `options` you remember to set. rustls inverts that: the safe configuration is the *only* configuration, and you opt *into* features like client-cert auth, not *out of* insecurity.

---

## Key Differences

| Aspect | Node.js (OpenSSL) | Rust (rustls) |
| --- | --- | --- |
| TLS implementation | OpenSSL (C), linked into Node | Pure Rust, memory-safe |
| Default protocol | Configurable; you set `minVersion` | TLS 1.3 preferred, 1.2 floor; no opt-in to older |
| Weak ciphers / SSLv3 | Reachable via options | Not implemented — cannot be enabled |
| Cert/key types | `Buffer` (untyped) | `CertificateDer` / `PrivateKeyDer` (distinct types) |
| Crypto backend | Fixed (OpenSSL) | Swappable provider (`aws-lc-rs` default, `ring`) |
| Disabling verification | `NODE_TLS_REJECT_UNAUTHORIZED=0` (one env var) | Requires a custom `dangerous()` verifier — deliberate and visible |
| Config errors | Often runtime / silent | Many caught at compile time by the builder |
| Build dependency | System OpenSSL or bundled | No system OpenSSL needed (`aws-lc-rs`/`ring` build their own) |

**rustls vs OpenSSL, in one paragraph:** OpenSSL is a 25-year-old C library with a vast feature surface and a long CVE history (Heartbleed being the famous memory-safety bug). The Rust bindings (`native-tls`, `openssl` crate) inherit a dependency on the *system* OpenSSL, which complicates cross-compilation and static linking. rustls avoids OpenSSL entirely: it is memory-safe by construction, ships secure defaults, and links a self-contained crypto library. The practical trade-offs: rustls does not support TLS 1.0/1.1 or some legacy enterprise features (older client certs, certain PKCS#11 HSM flows), and `aws-lc-rs` requires a C compiler at build time (use the `ring` provider, or rustls's `aws-lc-rs` prebuilt path, if that is a problem). For new services, rustls is the recommended default.

> **Tip:** If you genuinely need OpenSSL compatibility (legacy protocol versions, an HSM via PKCS#11, FIPS modules), the `tokio-native-tls` and `openssl` crates exist. For everything else, prefer rustls.

---

## Common Pitfalls

### Pitfall 1: Forgetting to install a crypto provider

If you enable *both* the `aws-lc-rs` and `ring` features (directly or transitively), rustls cannot guess which to use, and building a config panics at runtime. This is the real message:

```rust
// panics at runtime when more than one crypto provider feature is enabled
// and you never called CryptoProvider::install_default()
use rustls::ServerConfig;

fn main() {
    let _ = ServerConfig::builder().with_no_client_auth();
    println!("never reached");
}
```

Real output from `cargo run`:

```text
thread 'main' panicked at .../rustls-0.23.40/src/crypto/mod.rs:249:14:

Could not automatically determine the process-level CryptoProvider from Rustls crate features.
Call CryptoProvider::install_default() before this point to select a provider manually, or make sure exactly one of the 'aws-lc-rs' and 'ring' features is enabled.
See the documentation of the CryptoProvider type for more information.
```

**Fix:** call `rustls::crypto::aws_lc_rs::default_provider().install_default()` (or the `ring` equivalent) once at startup, *or* ensure only one provider feature is enabled. When exactly one provider is compiled in (the common case), rustls picks it automatically and the explicit call is optional, but adding it is good defensive practice so behavior does not change when a transitive dependency pulls in the other backend.

### Pitfall 2: Swapping the certificate and the key

`with_single_cert(certs, key)` takes the chain first, then the key. Because rustls uses distinct types (`Vec<CertificateDer>` vs `PrivateKeyDer`), passing them in the wrong order does not even compile: a mismatch Node would only surface at handshake time, if at all. Likewise, putting the leaf certificate *after* its intermediates in the PEM file produces a chain clients can't validate; the leaf must come first.

### Pitfall 3: Disabling certificate verification "just to make it work"

The Node habit of `NODE_TLS_REJECT_UNAUTHORIZED=0` tempts people to look for the rustls equivalent. There is one, but it is intentionally hard to reach: you must implement a `ServerCertVerifier` and register it through `ClientConfig::builder().dangerous().with_custom_certificate_verifier(...)`. The `dangerous()` in the path is a deliberate signpost. **Never** ship a no-op verifier; if you need to trust an internal CA, add that CA to the root store (shown below) instead of disabling verification.

### Pitfall 4: Trusting an empty/system root store you forgot to populate

A `ClientConfig` built `.with_root_certificates(RootCertStore::empty())` trusts *nothing* and rejects every server. For public endpoints, seed the store from the bundled Mozilla roots via the `webpki-roots` crate (no OS dependency) or from the OS trust store via `rustls-native-certs`. Reqwest's rustls backend does this for you automatically, but a hand-rolled `ClientConfig` does not.

---

## Best Practices

- **Prefer a higher-level server integration over raw `tokio-rustls`.** For an HTTP service, use `axum-server` (shown below), `hyper-rustls`, or terminate TLS at a reverse proxy. Drop to raw `tokio-rustls` only for non-HTTP protocols.
- **Keep rustls's defaults.** Do not lower the minimum protocol version or fiddle with cipher suites unless a compliance requirement forces it; the defaults are deliberately strong (TLS 1.3 + 1.2, AEAD ciphers only).
- **Reload certificates without downtime.** `axum-server`'s `RustlsConfig` supports `reload_from_pem_file`, so you can rotate a renewed certificate (e.g. from Let's Encrypt) without restarting the process.
- **Use the bundled root store for clients** (`webpki-roots`) for reproducible, OS-independent trust, or `rustls-native-certs` when you must honor the host's trust decisions.
- **Let your HTTP client default to rustls.** With `reqwest`, build it with `--no-default-features --features rustls,json` to skip the system-OpenSSL dependency entirely.
- **In production, TLS termination at the edge is still common and fine.** A managed load balancer or [reverse proxy](/28-production/) terminating TLS is a perfectly good architecture; in-process rustls shines for internal service-to-service mTLS, single-binary deployments, and when you want one fewer moving part.

> **Warning:** `aws-lc-rs` compiles C and (on some targets) assembly, so it needs a C compiler (and CMake) available at build time. In minimal CI/Docker images this can fail; either install build tools, switch to the `ring` provider, or use a base image that includes a toolchain. See [Section 28: Production](/28-production/).

---

## Real-World Example

A production-flavored HTTPS server using **Axum** with **axum-server** for TLS termination: the way you would actually expose a JSON API over HTTPS in a single binary. We also show generating a self-signed certificate with **rcgen** for local development (in production you would use a CA-issued certificate, e.g. from Let's Encrypt).

```rust
// Cargo.toml dependencies:
//   cargo add axum
//   cargo add axum-server --features tls-rustls-no-provider
//   cargo add tokio --features full
//   cargo add rustls
//   cargo add serde --features derive
use std::net::SocketAddr;

use axum::{routing::get, Json, Router};
use axum_server::tls_rustls::RustlsConfig;
use serde::Serialize;

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Pick the crypto backend explicitly (we built axum-server with the
    // `-no-provider` feature, so rustls won't guess for us).
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install crypto provider");

    let app = Router::new().route("/health", get(health));

    // Load the certificate chain and private key from PEM files.
    let tls = RustlsConfig::from_pem_file("cert.pem", "key.pem").await?;

    let addr = SocketAddr::from(([127, 0, 0, 1], 8443));
    println!("HTTPS server on https://{addr}");

    // `bind_rustls` terminates TLS for every accepted connection, then
    // hands the decrypted HTTP stream to the Axum `app`.
    axum_server::bind_rustls(addr, tls)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
```

To generate the development certificate with rcgen (run once before starting the server):

```rust
// Cargo.toml: cargo add rcgen
// A throwaway helper that writes cert.pem/key.pem for localhost.
fn main() {
    let names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let cert = rcgen::generate_simple_self_signed(names).unwrap();
    std::fs::write("cert.pem", cert.cert.pem()).unwrap();
    std::fs::write("key.pem", cert.signing_key.serialize_pem()).unwrap();
    println!("wrote cert.pem and key.pem");
}
```

Running the server and calling the endpoint over HTTPS gives the real output:

```text
$ cargo run
HTTPS server on https://127.0.0.1:8443

$ curl --cacert cert.pem --resolve localhost:8443:127.0.0.1 https://localhost:8443/health
{"status":"ok"}
```

### The matching rustls-based client

To call an HTTPS endpoint from Rust with the rustls backend, `reqwest` is the ergonomic choice. For public sites it trusts Mozilla's bundled roots automatically; here we additionally trust our self-signed CA the *right* way, by adding it to the trust store, never by disabling verification:

```rust
// Cargo.toml:
//   cargo add reqwest --no-default-features --features rustls,json
//   cargo add tokio --features full
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Trust our self-signed CA explicitly. Public sites need no such step:
    // reqwest's rustls backend ships Mozilla's root store by default.
    let pem = std::fs::read("cert.pem")?;
    let cert = reqwest::Certificate::from_pem(&pem)?;

    let client = reqwest::Client::builder()
        .add_root_certificate(cert)
        .use_rustls_tls()
        .build()?;

    let resp = client.get("https://localhost:8443/health").send().await?;
    println!("status: {}", resp.status());
    println!("body:   {}", resp.text().await?);
    Ok(())
}
```

Real output, with the Axum server above running:

```text
status: 200 OK
body:   {"status":"ok"}
```

For lower-level control — say, a non-HTTP protocol where you build the `ClientConfig` yourself — you seed the root store from the bundled Mozilla roots (`webpki-roots`) and optionally add your own CA:

```rust
// Cargo.toml:
//   cargo add rustls
//   cargo add rustls-pemfile
//   cargo add webpki-roots
use std::fs::File;
use std::io::BufReader;

use rustls::pki_types::CertificateDer;
use rustls::{ClientConfig, RootCertStore};

fn build_client_config() -> Result<ClientConfig, Box<dyn std::error::Error>> {
    // Start from the bundled Mozilla root store (no OS dependency).
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    // Additionally trust our own private CA for internal services.
    let mut reader = BufReader::new(File::open("cert.pem")?);
    let extra: Vec<CertificateDer<'static>> =
        rustls_pemfile::certs(&mut reader).collect::<Result<_, _>>()?;
    for cert in extra {
        roots.add(cert)?;
    }

    Ok(ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth())
}

fn main() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("install provider");
    let cfg = build_client_config().expect("client config");
    println!(
        "client config ready with {} cipher suites",
        cfg.crypto_provider().cipher_suites.len()
    );
}
```

Real output:

```text
client config ready with 9 cipher suites
```

---

## Further Reading

- [rustls documentation](https://docs.rs/rustls/latest/rustls/) — the core library and `CryptoProvider` model.
- [tokio-rustls](https://docs.rs/tokio-rustls/latest/tokio_rustls/) — async `TlsAcceptor`/`TlsConnector`.
- [axum-server `RustlsConfig`](https://docs.rs/axum-server/latest/axum_server/tls_rustls/) — TLS termination and hot certificate reload for Axum.
- [rustls vs OpenSSL background](https://www.memorysafety.org/initiative/rustls/) — the memory-safety motivation behind rustls.
- [Let's Encrypt](https://letsencrypt.org/) — free, automated CA-issued certificates for production.
- Related guide sections: [Section 16: Web APIs](/16-web-apis/) (building the Axum service you put behind TLS), [Section 11: Async](/11-async/) (the Tokio runtime model), and [Section 28: Production](/28-production/) (edge TLS termination and deployment).
- Sibling security topics: [Password Hashing](/27-security/04-password-hashing/), [Cryptography](/27-security/03-cryptography/), [Secure Randomness](/27-security/06-secure-randomness/), [Secrets Management](/27-security/07-secrets-management/), and [Dependency Auditing](/27-security/08-security-audit/).

---

## Exercises

### Exercise 1: Terminate TLS for a health endpoint

**Difficulty:** Beginner

**Objective:** Stand up an HTTPS server with a self-signed certificate and verify it from the command line.

**Instructions:** Using `rcgen`, generate a self-signed certificate and key for `localhost`. Then write an Axum + `axum-server` application that serves `GET /ping` returning the plain-text body `pong` over HTTPS on `127.0.0.1:8443`. Confirm it works with `curl --cacert cert.pem --resolve localhost:8443:127.0.0.1 https://localhost:8443/ping`.

<details>
<summary>Solution</summary>

```rust
// Cargo.toml:
//   cargo add axum
//   cargo add axum-server --features tls-rustls-no-provider
//   cargo add tokio --features full
//   cargo add rustls
//   cargo add rcgen
use std::net::SocketAddr;

use axum::{routing::get, Router};
use axum_server::tls_rustls::RustlsConfig;

async fn ping() -> &'static str {
    "pong"
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Generate a dev certificate on first run if it's missing.
    if !std::path::Path::new("cert.pem").exists() {
        let names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        let cert = rcgen::generate_simple_self_signed(names)?;
        std::fs::write("cert.pem", cert.cert.pem())?;
        std::fs::write("key.pem", cert.signing_key.serialize_pem())?;
    }

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("install crypto provider");

    let app = Router::new().route("/ping", get(ping));
    let tls = RustlsConfig::from_pem_file("cert.pem", "key.pem").await?;
    let addr = SocketAddr::from(([127, 0, 0, 1], 8443));
    println!("HTTPS on https://{addr}");

    axum_server::bind_rustls(addr, tls)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
```

```text
$ curl --cacert cert.pem --resolve localhost:8443:127.0.0.1 https://localhost:8443/ping
pong
```

</details>

### Exercise 2: A rustls client that trusts a private CA

**Difficulty:** Intermediate

**Objective:** Build a `reqwest` client that trusts a specific self-signed certificate *without* disabling verification, and prove that an untrusting client is rejected.

**Instructions:** Against the server from Exercise 1, write two clients: (a) one that adds `cert.pem` as a root certificate and successfully fetches `/ping`; (b) one that uses default roots only (does *not* add `cert.pem`) and observe that the request fails with a certificate-verification error. Print the status on success and the error on failure.

<details>
<summary>Solution</summary>

```rust
// Cargo.toml:
//   cargo add reqwest --no-default-features --features rustls
//   cargo add tokio --features full
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = "https://localhost:8443/ping";

    // (a) Trusting client: add our CA to the root set.
    let pem = std::fs::read("cert.pem")?;
    let cert = reqwest::Certificate::from_pem(&pem)?;
    let trusting = reqwest::Client::builder()
        .add_root_certificate(cert)
        .use_rustls_tls()
        .build()?;
    let resp = trusting.get(url).send().await?;
    println!("trusting client -> {}", resp.status());

    // (b) Default-roots client: does NOT trust our self-signed cert.
    let untrusting = reqwest::Client::builder().use_rustls_tls().build()?;
    match untrusting.get(url).send().await {
        Ok(r) => println!("untrusting client -> {} (unexpected)", r.status()),
        Err(e) => println!("untrusting client -> rejected: {e}"),
    }
    Ok(())
}
```

Real output (with the Exercise 1 server running):

```text
trusting client -> 200 OK
untrusting client -> rejected: error sending request for url (https://localhost:8443/ping)
```

The trusting client succeeds; the untrusting client is rejected. Walking the untrusting error's `source()` chain reveals the real root cause — `invalid peer certificate: ... certificate is not trusted` — because the self-signed certificate is not anchored to any trusted root. The point: you fix trust by *adding the CA*, never by turning verification off.

</details>

### Exercise 3: Mutual TLS (client certificate authentication)

**Difficulty:** Advanced

**Objective:** Configure a server that *requires* a client certificate (mTLS), the way internal service-to-service traffic is often secured.

**Instructions:** Build a `rustls::ServerConfig` that uses `with_client_cert_verifier` (instead of `with_no_client_auth`) so only clients presenting a certificate signed by a CA you trust can connect. Use `tokio-rustls` for the acceptor. You will need a CA certificate, a server certificate, and a client certificate (generate them with `rcgen`, signing the leaf certs with your CA). Outline the server configuration; full key generation is the involved part.

<details>
<summary>Solution</summary>

The key change is swapping `with_no_client_auth()` for a verifier built from your trusted client-CA root store:

```rust
// Cargo.toml:
//   cargo add rustls
//   cargo add tokio --features full
//   cargo add tokio-rustls
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};

/// Build an mTLS server config: present `server_chain`/`server_key`, and
/// require every client to present a cert signed by a CA in `client_ca`.
fn mtls_server_config(
    server_chain: Vec<CertificateDer<'static>>,
    server_key: PrivateKeyDer<'static>,
    client_ca: CertificateDer<'static>,
) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let mut roots = RootCertStore::empty();
    roots.add(client_ca)?;

    // This verifier rejects any client whose cert doesn't chain to `roots`.
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()?;

    let config = ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(server_chain, server_key)?;
    Ok(config)
}

fn main() {
    // In a real program: install the crypto provider, load the three PEM
    // inputs, call mtls_server_config(...), wrap it in a TlsAcceptor, and
    // accept connections as in the main example. Clients must then supply
    // their certificate via TlsConnector / reqwest `.identity(...)`.
    println!("mTLS config builder ready");
}
```

The conceptual takeaway: in rustls, "require a client certificate" is a different builder method (`with_client_cert_verifier`) that the type system forces you to choose explicitly: there is no ambiguous middle state. The client must then present a matching identity (e.g. `reqwest::Identity` plus `.identity(...)`, or a `tokio-rustls` `TlsConnector` configured with a client cert).

</details>
