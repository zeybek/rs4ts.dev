---
title: "Real-World Rust Projects"
sidebar:
  label: "Overview"
description: "Six real Rust projects, each rebuilding a Node stack you know, an Express API, ws chat, canvas game, microservice, so you see what changes from TypeScript."
---

You have spent twenty-nine sections trading TypeScript intuitions for Rust ones: ownership instead of garbage collection, `Result` instead of `try/catch`, traits instead of structural interfaces, `cargo` instead of `npm`. This final section is where all of it pays off. Each of the six projects below is a small but *real* application: the kind of thing you would actually ship, not a snippet that demonstrates one keyword in isolation. If the earlier sections were the vocabulary, these are the essays.

Every project mirrors a stack you already know from the Node.js world: an Express REST API, a Commander CLI, a `canvas`-driven browser game, a `ws` chat server, a containerized microservice, and a single-page app talking to its own backend. We rebuild each one in idiomatic Rust so you can see, side by side, what changes and what stays the same. The walkthroughs are written for a senior JavaScript/TypeScript developer: they assume you know *why* you would build the thing and focus on *how* Rust does it differently.

Importantly, each walkthrough has a matching code directory that is a genuine, compiling Cargo project. Nothing here requires an external database, message broker, or cloud account to run: state lives in memory (typically an `Arc<Mutex<HashMap<..>>>` or `Arc<RwLock<HashMap<..>>>`, or a JSON file for the CLI) so you can `cargo run` and poke at it immediately, and every project notes how to swap in a real datastore when you outgrow that. The program output shown in each walkthrough is real, captured from actually running the code. Read the markdown top to bottom, or clone the code directory and start hacking.

## Projects

| # | Walkthrough | What you build | Draws on |
|---|-------------|----------------|----------|
| 1 | [REST API (Express → Axum)](/30-projects/00-rest-api/) | A JSON CRUD API with routing, extractors, shared state, and error handling. The Rust answer to an Express service. | [11-async](/11-async/), [15-serialization](/15-serialization/), [16-web-apis](/16-web-apis/), [08-error-handling](/08-error-handling/) |
| 2 | [CLI Tool (a task manager)](/30-projects/01-cli-tool/) | `taskr`, a task/notes manager with subcommands, flags, and JSON persistence, like a Commander/oclif tool. | [03-functions](/03-functions/), [08-error-handling](/08-error-handling/), [15-serialization](/15-serialization/), [18-cli-tools](/18-cli-tools/) |
| 3 | [WASM App (Conway's Game of Life)](/30-projects/02-wasm-app/) | A browser game compiled to WebAssembly, driven from JavaScript and rendered to a canvas. | [05-ownership](/05-ownership/), [07-collections](/07-collections/), [19-wasm](/19-wasm/) |
| 4 | [WebSocket Chat Server](/30-projects/03-websocket-chat/) | A real-time chat server with broadcast channels and per-connection tasks. The Rust equivalent of a `ws` server. | [11-async](/11-async/), [10-smart-pointers](/10-smart-pointers/), [16-web-apis](/16-web-apis/) |
| 5 | [Production Microservice (URL Shortener)](/30-projects/04-microservice/) | A deployable URL shortener with config, structured logging, health checks, graceful shutdown, and Docker. | [16-web-apis](/16-web-apis/), [17-database](/17-database/), [28-production](/28-production/) |
| 6 | [Full-Stack App (Axum API + WASM frontend)](/30-projects/05-full-stack/) | A Cargo workspace pairing an Axum backend with a WASM frontend that share one set of Rust types. | [09-generics-traits](/09-generics-traits/), [12-modules-packages](/12-modules-packages/), [16-web-apis](/16-web-apis/), [19-wasm](/19-wasm/) |

## How to Use These Projects

Each `*.md` walkthrough is paired with a `*-code/` directory holding the complete, runnable project. The code shown in a walkthrough matches the files in its directory verbatim, so you can follow along by reading the markdown, or just clone the directory and explore.

Use the repository's [pinned verification toolchain](/00-introduction/05-version-policy/) and the **2024 edition** so your local results match CI:

```bash
rustup update stable
rustc --version
```

The build and run commands depend on the project type:

**Native projects** — REST API, CLI tool, WebSocket chat, microservice. Standard Cargo workflow:

```bash
# from inside the project's code directory, e.g. examples/rest-api-code
cargo run              # build and run the binary
cargo build --release  # optimized build for deployment
cargo test             # run the test suite
```

The server projects print their listening address on startup; hit them with `curl` (or a WebSocket client) as shown in each walkthrough's **Running It** section. The CLI tool exposes subcommands; run `cargo run -- --help` to see them.

**WASM project** — the Game of Life. You need the WebAssembly target and `wasm-pack`:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack            # one-time

# from inside 30-projects/wasm-app-code
wasm-pack build --target web       # produces the pkg/ bundle
```

Then serve the directory with any static file server (for example `python3 -m http.server`) and open it in a browser. The walkthrough has the exact steps.

**Full-stack workspace** — the combined API + frontend. This one is a Cargo *workspace* with a native `backend` crate and a `wasm` `frontend` crate, plus a `build.sh` that builds both:

```bash
# from inside 30-projects/full-stack-code
rustup target add wasm32-unknown-unknown   # if you have not already
./build.sh             # compiles the WASM frontend, then runs the backend
```

> **In-memory by default.** None of these projects need Postgres, Redis, or any running service to build or run. They use an in-memory store (or a JSON file for the CLI). Every walkthrough's **Extending It** section explains how to swap in a real database; see [17-database](/17-database/) for the patterns.

## Prerequisites

These are capstones: they assume you have worked through the core of the guide. If a project uses something you have not seen, the **Prerequisites** section at the top of each walkthrough links the exact sections it builds on. At minimum you will want to be comfortable with:

- [05-ownership](/05-ownership/): borrowing, lifetimes, and `Arc`/`Mutex` for shared state.
- [08-error-handling](/08-error-handling/) — `Result`, `?`, and custom error types.
- [09-generics-traits](/09-generics-traits/): traits and generics, which underpin every framework here.
- [11-async](/11-async/) — `async`/`await` and the Tokio runtime (used by all the server projects).
- [12-modules-packages](/12-modules-packages/): how a multi-file Cargo crate is organized.
- [15-serialization](/15-serialization/) — `serde` for JSON, which shows up everywhere.

If you are arriving from the [migration guide](/29-migration-guide/), that section pairs naturally with these projects: it covers the *strategy* of porting a Node.js codebase, and these show the *result*.

## Project Index

Each walkthrough maps to the directory that holds its code:

| Walkthrough | Code directory | Cargo package | Type |
|-------------|----------------|---------------|------|
| [Project 1](/30-projects/00-rest-api/) | [`rest-api-code/`](https://github.com/zeybek/rs4ts.dev/tree/main/examples/rest-api-code) | `rest-api` | Native binary (Axum) |
| [Project 2](/30-projects/01-cli-tool/) | [`cli-tool-code/`](https://github.com/zeybek/rs4ts.dev/tree/main/examples/cli-tool-code) | `taskr` | Native binary (clap) |
| [Project 3](/30-projects/02-wasm-app/) | [`wasm-app-code/`](https://github.com/zeybek/rs4ts.dev/tree/main/examples/wasm-app-code) | `game-of-life` | WASM library |
| [Project 4](/30-projects/03-websocket-chat/) | [`websocket-chat-code/`](https://github.com/zeybek/rs4ts.dev/tree/main/examples/websocket-chat-code) | `websocket-chat-code` | Native binary (Axum + WebSocket) |
| [Project 5](/30-projects/04-microservice/) | [`microservice-code/`](https://github.com/zeybek/rs4ts.dev/tree/main/examples/microservice-code) | `url-shortener` | Native binary (Axum + in-memory store) |
| [Project 6](/30-projects/05-full-stack/) | [`full-stack-code/`](https://github.com/zeybek/rs4ts.dev/tree/main/examples/full-stack-code) | `backend` + `frontend` (workspace) | Native + WASM workspace |

## A Closing Note

If you build all six, you will have written — in Rust — a REST API, a CLI, a browser app, a real-time server, a deployable service, and a full-stack app sharing types across the network boundary. That is most of what day-to-day product engineering actually is, and you will have done it with a compiler that catches the data races and null-dereferences your TypeScript types only *hoped* you avoided.

There is no twentieth video to watch and no thirty-first section after this one. The next project is yours: pick one of these, follow an **Extending It** suggestion, and ship it. Welcome to Rust.
