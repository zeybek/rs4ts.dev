---
title: "Dockerizing Rust"
description: "A Rust image holds one self-contained binary, not a Node runtime plus node_modules. Multi-stage builds, cargo-chef caching, and a 2 MB scratch image."
---

## Quick Overview

Packaging a Rust service as a Docker image is conceptually the same exercise you already know from Node.js (copy the project in, install dependencies, build, ship) but the trade-offs are inverted. A Node image carries a heavy runtime (`node`) plus a giant `node_modules` directory at runtime; a Rust image compiles down to a single self-contained binary, so the *final* image can be astonishingly small (single-digit megabytes) while the *build* is slow and benefits enormously from careful layer caching. This page covers the three things that matter most for production Rust images: **multi-stage builds**, **dependency caching with `cargo-chef`**, and **minimal final images on distroless or `scratch`**.

> **Note:** The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. The examples below pin the official `rust:1.96-slim-bookworm` build image (the repository pin); prefer pinning to a specific version tag over `rust:latest` for reproducible builds.

---

## TypeScript/JavaScript Example

A typical production Dockerfile for a TypeScript service uses a multi-stage build to keep `devDependencies` and the TypeScript compiler out of the runtime image, but the final image still ships a Node.js runtime and the entire production `node_modules` tree:

```dockerfile
# Dockerfile (Node.js / TypeScript service)

# ---- Build stage ----
FROM node:22-slim AS builder
WORKDIR /app
# Copy manifests first so the install layer caches independently of source.
COPY package.json package-lock.json ./
RUN npm ci
COPY tsconfig.json ./
COPY src ./src
RUN npm run build               # tsc -> dist/

# ---- Runtime stage ----
FROM node:22-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production
COPY package.json package-lock.json ./
RUN npm ci --omit=dev           # production deps only
COPY --from=builder /app/dist ./dist
EXPOSE 8080
USER node
CMD ["node", "dist/server.js"]
```

```jsonc
// .dockerignore
node_modules
dist
.git
```

The two big ideas you already rely on carry straight over to Rust:

1. **Copy the manifest before the source** so the slow dependency layer is cached and only re-runs when `package.json`/`package-lock.json` changes.
2. **Use a second stage** so build-only tooling (the TypeScript compiler, `devDependencies`) never reaches the runtime image.

The result is still a fairly large image: the `node:22-slim` base alone is around 200 MB, plus your production `node_modules`. In Rust, both of these ideas exist but go much further: the runtime image can drop the language runtime entirely, and dependency caching needs an extra trick because Cargo does not have a clean "install deps only" command the way `npm ci` does.

---

## Rust Equivalent

A naive but correct Rust multi-stage Dockerfile. The build stage uses the official `rust` image; the runtime stage copies just the compiled binary onto a slim Debian base:

```dockerfile
# Dockerfile (Rust / axum service) — multi-stage, no fancy caching yet

# ---- Build stage ----
FROM rust:1.96-slim-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

# ---- Runtime stage ----
FROM debian:bookworm-slim AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/greeter /usr/local/bin/greeter
EXPOSE 8080
CMD ["greeter"]
```

```text
# .dockerignore
target
.git
```

The service being built is a small `axum` web server:

```rust
// src/main.rs
use axum::{routing::get, Json, Router};
use serde::Serialize;
use std::net::SocketAddr;

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(|| async { "Hello from Rust in Docker!" }))
        .route("/health", get(health));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("listening on {addr}");
    axum::serve(listener, app).await.unwrap();
}
```

```toml
# Cargo.toml
[package]
name = "greeter"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8.9"
tokio = { version = "1.52.3", features = ["full"] }
serde = { version = "1.0.228", features = ["derive"] }
```

Building and running it produces a working container:

```text
$ docker build -t greeter:slim .
...
$ docker run -d --name greeter-test -p 8080:8080 greeter:slim
$ curl -s http://localhost:8080/
Hello from Rust in Docker!
$ curl -s http://localhost:8080/health
{"status":"ok"}
```

The resulting image is **99.1 MB**, already far smaller than a comparable Node image, with the `node` runtime gone entirely. But it has a serious flaw for day-to-day development (every code change recompiles all dependencies from scratch) and it is still bigger than it needs to be. The next two sections fix both problems.

> **Note:** `rust:1.96-slim-bookworm` is just the Debian "slim" flavor of the official Rust image. There is also a default `rust:1.96` (larger, includes more build tooling) and `rust:1.96-alpine` (musl-based, used later for the `scratch` build). Pin to a version tag, not `latest`, for reproducible CI builds.

---

## Detailed Explanation

### Why multi-stage is non-negotiable for Rust

The `rust` build image contains the entire toolchain — `rustc`, `cargo`, the standard library sources, a linker, and often a full C build environment — which is well over a gigabyte. You never want that in production. A **multi-stage build** runs the compiler in a throwaway `builder` stage, then `COPY --from=builder` lifts *only* the finished binary into a clean runtime stage. Everything not explicitly copied forward is discarded. This is the same pattern as the Node example, but the payoff is larger because the build toolchain is so heavy and the artifact is so light.

### The caching problem Cargo creates

In the Node example, `COPY package.json package-lock.json` before the source means `RUN npm ci` is a cached layer until the manifests change. The equivalent attempt in Rust does not work, because **Cargo has no "install dependencies only" step**. `cargo build` compiles your dependencies *and* your crate in one pass. If you copy `Cargo.toml` and then `src`, any change to a single line in `src/main.rs` invalidates the `COPY src` layer, which invalidates the `cargo build` layer, which recompiles every dependency from source again. For a project with `tokio` and `axum`, that is dozens of crates and tens of seconds (or minutes, for a large service) on every code change.

The old folk-remedy was to copy `Cargo.toml`, create a dummy `src/main.rs` containing `fn main() {}`, run `cargo build --release` to cache deps, then delete the dummy and copy the real source. It works but is fiddly and breaks for workspaces and library crates. The modern, reliable answer is `cargo-chef`.

### How `cargo-chef` solves it

[`cargo-chef`](https://github.com/LukeMathWalker/cargo-chef) splits the build into two cacheable phases:

1. `cargo chef prepare` reads your `Cargo.toml`/`Cargo.lock` and emits a `recipe.json` — a minimal description of your dependency graph that does **not** include your application source. Because it ignores `src`, the recipe only changes when your dependencies change.
2. `cargo chef cook` consumes that `recipe.json` and compiles *only the dependencies*, producing a cached `target/` directory. This layer is cached until `recipe.json` changes.

Only after `cook` do you copy your real source and run `cargo build`, which now reuses the already-compiled dependencies and recompiles just your crate.

```dockerfile
# Dockerfile.chef — multi-stage build with cargo-chef dependency caching

FROM rust:1.96-slim-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# Stage 1: produce a recipe.json describing the dependency graph (no app source).
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: build (and cache) ONLY the dependencies from the recipe.
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
# Now copy the real source and build only the application crate.
COPY . .
RUN cargo build --release

# Stage 3: minimal runtime.
FROM gcr.io/distroless/cc-debian12 AS runtime
COPY --from=builder /app/target/release/greeter /usr/local/bin/greeter
EXPOSE 8080
USER nonroot:nonroot
CMD ["greeter"]
```

The measured difference is dramatic. A **cold** build (nothing cached) of this image:

```text
$ time docker build -f Dockerfile.chef -t greeter:chef .
...
real 77.27
```

Then changing only `src/main.rs` (no dependency change) and rebuilding:

```text
$ docker build -f Dockerfile.chef -t greeter:chef .
#13 [builder 2/4] RUN cargo chef cook --release --recipe-path recipe.json
#13 CACHED
#15 [builder 4/4] RUN cargo build --release
...
real 4.81
```

The `cargo chef cook` layer is reported as `CACHED` because the dependency graph did not change, so the second build skips all dependency compilation and finishes in **4.81 s instead of 77 s**. Only the application crate is rebuilt. That is the entire reason `cargo-chef` exists.

> **Tip:** `cargo install cargo-chef --locked` is the right way to install it inside the image (it honors the tool's own `Cargo.lock`). To avoid reinstalling it on every build, the official pattern uses the prebuilt `lukemathwalker/cargo-chef` base image; installing it yourself, as above, is simpler to read and still cached as its own layer.

### Choosing the runtime base: slim vs distroless vs scratch

The final stage determines image size and attack surface. Three common choices, with the **real measured sizes** for the `greeter` binary above:

| Final base | Image size | What it contains | Has a shell? | Notes |
|---|---|---|---|---|
| `debian:bookworm-slim` | 99.1 MB | minimal Debian userland + glibc | yes (`sh`) | easy to debug; works with dynamically-linked glibc binaries |
| `gcr.io/distroless/cc-debian12` | 36.1 MB | glibc + CA certs, **no shell, no package manager** | no | runs glibc binaries; smaller attack surface; built-in `nonroot` user |
| `scratch` (with a static musl binary) | 1.96 MB | **nothing** but your binary | no | requires a fully static build; no CA certs, no `/etc/passwd` |

> **Note:** These numbers are from a tiny service. The runtime base contributes a fixed overhead (0 MB for `scratch`, ~36 MB for distroless, ~74 MB for slim) and your binary adds the rest. As your binary grows, the *relative* difference between bases shrinks, but the security argument for distroless/`scratch` (no shell for an attacker to land in, no package manager, no unused libraries) remains.

### The `scratch` + static musl path

`scratch` is the empty image — zero files. To run on it, your binary must have **no dynamic dependencies** at all, including the C runtime. The standard way to get this is to target musl libc, which Rust can link statically. The easiest route is to build on the `rust:*-alpine` image, whose host toolchain already targets `*-unknown-linux-musl`, so a plain release build produces a static binary:

```dockerfile
# Dockerfile.scratch — fully static musl binary on an empty base

# Build a fully static musl binary on Alpine, then ship it on scratch.
FROM rust:1.96-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
# The Alpine toolchain's host target is already *-unknown-linux-musl,
# so a plain release build yields a static binary.
RUN cargo build --release

FROM scratch AS runtime
COPY --from=builder /app/target/release/greeter /greeter
EXPOSE 8080
ENTRYPOINT ["/greeter"]
```

```text
$ docker build -f Dockerfile.scratch -t greeter:scratch .
...
$ docker images greeter:scratch --format '{{.Size}}'
1.96MB
$ docker run -d -p 8080:8080 greeter:scratch
$ curl -s http://localhost:8080/health
{"status":"ok"}
```

A **1.96 MB** image that serves real HTTP traffic. On an x86_64 host you would instead `rustup target add x86_64-unknown-linux-musl` and `cargo build --release --target x86_64-unknown-linux-musl`, then copy from `target/x86_64-unknown-linux-musl/release/`; building on the matching Alpine image (as above) keeps the Dockerfile architecture-agnostic. For the deeper story on musl and cross-targets, see [Cross-Compilation](/24-tooling/10-cross-compilation/).

---

## Key Differences

| Aspect | Node.js / TypeScript | Rust |
|---|---|---|
| Runtime in final image | The `node` binary **must** ship | Nothing — the compiled binary is self-contained |
| `node_modules` / deps at runtime | Production `node_modules` shipped | Dependencies are compiled *into* the binary, not shipped |
| "Install deps only" layer | `npm ci` (clean, cacheable) | No native equivalent → use `cargo-chef` |
| Build speed | Fast (transpile) | Slow (full native compile) → caching matters more |
| Smallest realistic image | ~150–200 MB (slim Node + deps) | ~2 MB on `scratch`, ~36 MB distroless |
| Glibc vs musl | Irrelevant (interpreted) | Determines whether `scratch` works at all |
| Final-stage shell | Often present (debugging) | Often absent (distroless/`scratch`) by design |

The mental model shift: in Node, the image is "a runtime plus your code plus its dependencies." In Rust, the image is "your binary, and maybe a libc and some CA certs." The language runtime simply does not exist as a separate thing to ship; it is linked into your executable at build time. This is why a Rust final image can be smaller than the *base image* of a Node service.

> **Warning:** Smaller is not automatically better. A `scratch` image has no shell, no `/etc/passwd`, no CA certificates, and no DNS resolver config beyond what the binary handles itself. If your service makes outbound HTTPS calls, you must `COPY` CA certificates into the image or use a TLS stack that bundles roots (e.g. `rustls` with `webpki-roots`). Distroless's `cc` variant includes CA certs and a `nonroot` user, which is why it is the pragmatic default for most services.

---

## Common Pitfalls

### Forgetting `.dockerignore` and copying `target/`

Without a `.dockerignore` that excludes `target`, the `COPY . .` in the planner stage drags your host's multi-gigabyte `target/` directory into the build context and the image. Always exclude it:

```text
# .dockerignore
target
.git
```

This is the direct analog of excluding `node_modules` and `dist` in a Node `.dockerignore`. The host `target/` is also built for your host platform and is useless inside a Linux container, so copying it is pure waste.

### Shipping a glibc binary on `scratch`

This is the most common and most confusing `scratch` mistake. If you build on the default (glibc) `rust` image and then copy onto `scratch`, the binary is dynamically linked against glibc's loader, which does not exist in the empty image. The container fails to start with a misleading message:

```text
$ docker run --rm greeter:broken
exec /greeter: no such file or directory
```

The file *is* there — the "no such file" refers to the missing dynamic linker (`/lib64/ld-linux-...`), not your binary. The fix is to produce a **static** binary (the musl approach shown above) or to use distroless `cc` (which provides glibc) instead of `scratch`. When you see `exec ...: no such file or directory` for a binary you know you copied, suspect a missing dynamic loader.

### Expecting Cargo's manifest-first trick to "just work"

Copying `Cargo.toml` before `src` caches nothing useful on its own, because `cargo build` still compiles dependencies and your crate together — so the first change to `src` invalidates the whole build. This is *not* like `COPY package.json && RUN npm ci`. Reach for `cargo-chef` (or the dummy-`main.rs` hack) to get a genuinely cacheable dependency layer.

### Running as root in the final image

`debian:bookworm-slim` and `scratch` run as `root` by default. Add a non-root user or use distroless, which provides one:

```dockerfile
# distroless ships a ready-made unprivileged user
USER nonroot:nonroot
```

For a `debian-slim` base you can create one explicitly (`RUN useradd -r appuser && USER appuser`). Running services as root inside a container is a needless privilege escalation if the container is ever compromised.

### Mismatched `EXPOSE`/bind address

The server must bind to `0.0.0.0`, not `127.0.0.1` — a container's loopback is not reachable from the host. The example uses `SocketAddr::from(([0, 0, 0, 0], 8080))` for exactly this reason. `EXPOSE 8080` is documentation; you still need `-p 8080:8080` (or a compose port mapping) to publish it. This trips up Node developers too, but it is worth restating because a `scratch` image gives you no shell to diagnose it from inside.

---

## Best Practices

- **Always multi-stage.** Compile in a `rust` builder stage; ship only the binary. Never run production on the full `rust` image.
- **Use `cargo-chef` for dependency caching.** It is the de-facto standard, works with workspaces and library crates, and turns a 77 s rebuild into a ~5 s one after source-only changes.
- **Default to distroless `cc` for services.** It is small (~36 MB overhead), includes CA certificates for outbound TLS, ships a `nonroot` user, and removes the shell and package manager that attackers look for.
- **Reach for `scratch` + static musl when size or attack surface is paramount** (sidecars, CLIs, FaaS), accepting that you must handle CA certs and that some C-dependent crates may not link statically without effort.
- **Pin base image versions** (`rust:1.96-slim-bookworm`, not `rust:latest`) for reproducible builds, and pin your `Cargo.lock` by copying it into the image.
- **Set `EXPOSE` and bind to `0.0.0.0`.** Add a `HEALTHCHECK` if your orchestrator does not provide one.
- **Combine with build-cache mounts in CI.** BuildKit cache mounts (`RUN --mount=type=cache,target=/app/target ...`) complement `cargo-chef` by persisting the `target` dir across builds; pair this with a CI registry cache. See [A Real GitHub Actions Workflow for Rust](/24-tooling/08-github-actions/) and [CI/CD Concepts for Rust](/24-tooling/07-ci-cd/) for caching Rust builds in pipelines.
- **Strip and optimize the binary** via a release profile (`strip = true`, `lto = true`, `opt-level = "z"` for size) configured in `Cargo.toml` — see [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/) for profile tuning. A stripped binary shrinks every final image.

> **Tip:** Enable Docker BuildKit (the default in modern Docker) so multi-stage builds run stages in parallel and support `--mount=type=cache`. The build numbers on this page were produced with BuildKit active.

---

## Real-World Example

A production-oriented Dockerfile that combines everything: `cargo-chef` caching, a BuildKit cache mount for the `target` directory, a size-optimized release profile, distroless runtime, a non-root user, and a healthcheck. This is the template most teams should start from.

```toml
# Cargo.toml — add a release profile tuned for small, fast binaries
[package]
name = "greeter"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8.9"
tokio = { version = "1.52.3", features = ["full"] }
serde = { version = "1.0.228", features = ["derive"] }

[profile.release]
strip = true        # remove debug symbols from the binary
lto = true          # link-time optimization (smaller, faster)
codegen-units = 1   # better optimization at the cost of build parallelism
panic = "abort"     # no unwinding machinery -> slightly smaller binary
```

```dockerfile
# Dockerfile — production Rust service: cargo-chef + distroless + non-root
# syntax=docker/dockerfile:1

FROM rust:1.96-slim-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# Cache the dependency build keyed on recipe.json (deps-only graph).
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
# Persist the target dir across builds with a BuildKit cache mount, and
# copy the finished binary out so it survives the (unmounted) layer.
RUN --mount=type=cache,target=/app/target/release/incremental \
    cargo build --release && \
    cp target/release/greeter /app/greeter

FROM gcr.io/distroless/cc-debian12 AS runtime
WORKDIR /app
COPY --from=builder /app/greeter /usr/local/bin/greeter
EXPOSE 8080
USER nonroot:nonroot
# distroless has no shell, so the healthcheck calls the binary/endpoint
# via the orchestrator instead; declare the port and run unprivileged.
ENTRYPOINT ["greeter"]
```

```yaml
# compose.yaml — local dev / smoke test
services:
  greeter:
    build: .
    image: greeter:latest
    ports:
      - "8080:8080"
    restart: unless-stopped
```

Building and exercising it:

```text
$ docker build -t greeter:prod .
$ docker compose up -d
$ curl -s http://localhost:8080/health
{"status":"ok"}
```

The same shape scales to a multi-binary workspace: `cargo chef cook` builds every workspace dependency once, and a final stage copies whichever binary that service needs (`target/release/<bin-name>`). For a Kubernetes deployment you would add a `livenessProbe`/`readinessProbe` hitting `/health` (distroless has no shell, so prefer an HTTP probe over an `exec` probe).

> **Note:** The `cargo chef cook` step rebuilds the dependency cache only when `recipe.json` changes. Adding, removing, or upgrading a dependency changes the recipe and correctly busts the cache; editing application code does not. This is exactly the caching behavior you want in CI.

---

## Further Reading

- [cargo-chef](https://github.com/LukeMathWalker/cargo-chef): the dependency-caching tool and its official Docker patterns
- [Distroless container images](https://github.com/GoogleContainerTools/distroless): Google's minimal runtime bases (`cc`, `static`, `base`)
- [Official `rust` Docker image](https://hub.docker.com/_/rust): available tags (`slim`, `alpine`, version pins)
- [Dockerfile multi-stage builds](https://docs.docker.com/build/building/multi-stage/) and [BuildKit cache mounts](https://docs.docker.com/build/cache/optimize/): the Docker-side primitives
- [The Rust + musl story](https://doc.rust-lang.org/rustc/platform-support.html): static linking and target support
- Related guide sections:
  - [Cross-Compilation](/24-tooling/10-cross-compilation/): musl targets, `rustup target add`, and the `cross` tool in depth
  - [Cargo Deep Dive](/24-tooling/00-cargo-deep-dive/): release profiles (`strip`, `lto`, `opt-level`) that shrink the binary you containerize
  - [A Real GitHub Actions Workflow for Rust](/24-tooling/08-github-actions/) and [CI/CD Concepts for Rust](/24-tooling/07-ci-cd/): caching and building Docker images in CI
  - [Section 16: Web APIs](/16-web-apis/): the `axum` service shown here, in full
  - [Understanding Cargo](/01-getting-started/03-cargo-basics/): Cargo fundamentals
  - [Section 25: Advanced Topics](/25-advanced-topics/): where deployment and runtime concerns continue

---

## Exercises

### Exercise 1 — Shrink the naive image with distroless

**Difficulty:** Beginner

**Objective:** Take the naive `debian-slim` multi-stage Dockerfile and cut its size by moving the runtime stage to distroless.

**Instructions:** Start from the multi-stage Dockerfile in the "Rust Equivalent" section (final stage `debian:bookworm-slim`). Change only the runtime stage to use `gcr.io/distroless/cc-debian12`, run as the `nonroot` user, and rebuild. Compare `docker images` sizes before and after. Confirm the container still answers `GET /health`.

<details>
<summary>Solution</summary>

```dockerfile
# Dockerfile — distroless runtime stage
FROM rust:1.96-slim-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12 AS runtime
COPY --from=builder /app/target/release/greeter /usr/local/bin/greeter
EXPOSE 8080
USER nonroot:nonroot
CMD ["greeter"]
```

```text
$ docker build -t greeter:distroless .
$ docker images greeter --format '{{.Repository}}:{{.Tag}} {{.Size}}'
greeter:distroless 36.1MB
greeter:slim 99.1MB
$ docker run -d -p 8080:8080 greeter:distroless
$ curl -s http://localhost:8080/health
{"status":"ok"}
```

Only the runtime base changed — the binary is identical — yet the image dropped from 99.1 MB to 36.1 MB and lost its shell and package manager (a security win). The `cc` distroless variant still provides glibc, so the dynamically-linked binary runs unchanged.

</details>

### Exercise 2 — Prove the `cargo-chef` cache hit

**Difficulty:** Intermediate

**Objective:** Demonstrate that `cargo-chef` caches the dependency layer across a source-only change.

**Instructions:** Build the `Dockerfile.chef` from the Detailed Explanation once (cold). Then edit a string literal in `src/main.rs` (not `Cargo.toml`) and rebuild. Inspect the build output and confirm the `cargo chef cook` step reports `CACHED` and the rebuild is dramatically faster than the cold build. Then add a new dependency to `Cargo.toml` and rebuild again — confirm the `cook` step now re-runs.

<details>
<summary>Solution</summary>

```text
# Cold build:
$ time docker build -f Dockerfile.chef -t greeter:chef .
real 77.27

# Edit only src/main.rs (change a response string), rebuild:
$ docker build -f Dockerfile.chef -t greeter:chef . 2>&1 | grep -E "cook|CACHED"
#13 [builder 2/4] RUN cargo chef cook --release --recipe-path recipe.json
#13 CACHED
$ time docker build -f Dockerfile.chef -t greeter:chef .
real 4.81
```

The `cook` layer is `CACHED` because `recipe.json` (the deps-only graph) did not change, so the ~77 s of dependency compilation is skipped and only the app crate recompiles, hence ~5 s.

Now change a dependency:

```text
$ cargo add uuid           # changes Cargo.toml/Cargo.lock -> recipe.json
$ docker build -f Dockerfile.chef -t greeter:chef . 2>&1 | grep -E "cook"
#13 [builder 2/4] RUN cargo chef cook --release --recipe-path recipe.json
#13 [builder 2/4] RUN cargo chef cook ...   (runs, not CACHED)
```

Because the dependency graph changed, `recipe.json` changed, so the `cook` layer is correctly invalidated and dependencies recompile — exactly the cache behavior you want. (Your wall-clock numbers will differ; the structure of the result is the point.)

</details>

### Exercise 3 — Ship a 2 MB image on `scratch`

**Difficulty:** Advanced

**Objective:** Produce a fully static `scratch` image and diagnose the classic glibc-on-scratch failure.

**Instructions:** First, deliberately reproduce the failure: build the binary on the glibc `rust:1.96-slim-bookworm` image, copy it onto `scratch`, and run the container — observe and explain the error. Then fix it by building on `rust:1.96-alpine` (musl) so the binary is static, copy onto `scratch`, and confirm a working ~2 MB image.

<details>
<summary>Solution</summary>

**Step 1 — the failure.** A glibc binary on `scratch` has no dynamic loader:

```dockerfile
# Dockerfile.broken
FROM rust:1.96-slim-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM scratch AS runtime
COPY --from=builder /app/target/release/greeter /greeter
ENTRYPOINT ["/greeter"]
```

```text
$ docker build -f Dockerfile.broken -t greeter:broken .
$ docker run --rm greeter:broken
exec /greeter: no such file or directory
```

The binary exists; the "no such file" is the *missing dynamic linker* it was compiled to need. `scratch` contains nothing, including `ld-linux`.

**Step 2 — the fix.** Build a static musl binary on Alpine, whose host target is already `*-unknown-linux-musl`:

```dockerfile
# Dockerfile.scratch
FROM rust:1.96-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM scratch AS runtime
COPY --from=builder /app/target/release/greeter /greeter
EXPOSE 8080
ENTRYPOINT ["/greeter"]
```

```text
$ docker build -f Dockerfile.scratch -t greeter:scratch .
$ docker images greeter:scratch --format '{{.Size}}'
1.96MB
$ docker run -d -p 8080:8080 greeter:scratch
$ curl -s http://localhost:8080/
Hello from Rust in Docker!
```

A 1.96 MB image with no OS underneath it, serving real traffic. On an x86_64 host you would instead `rustup target add x86_64-unknown-linux-musl` and build with `--target x86_64-unknown-linux-musl`; building on the matching Alpine image avoids hardcoding the architecture. Remember that `scratch` has no CA certificates — if this service made outbound HTTPS calls you would need to `COPY` a `ca-certificates.crt` into the image or use a TLS stack that bundles roots. See [Cross-Compilation](/24-tooling/10-cross-compilation/) for the full musl story.

</details>
