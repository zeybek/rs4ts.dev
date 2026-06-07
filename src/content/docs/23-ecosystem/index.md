---
title: "Ecosystem"
sidebar:
  label: "Overview"
description: "Map your npm packages to Rust crates: serde, tokio, axum, reqwest, chrono, regex, tracing, plus the web frameworks and async runtimes Node devs need."
---

Coming from Node, you lean on npm: Express for web, axios for HTTP, winston for logs, date-fns for dates, lodash for utilities, and a hundred small packages besides. Rust ships a deliberately small standard library and leans on a tight set of community **crates** that have become near-universal. This section is a guided tour of that ecosystem for a Node developer: the headline crates and the npm packages they replace, the web frameworks and async runtimes, logging and structured tracing, documentation, HTTP clients, date/time, regular expressions, and real parsing, so you know what to install and why on day one.

---

## What You'll Learn

- The **most-used crates** and the npm packages they replace (serde, tokio, clap, reqwest, anyhow/thiserror, ...)
- The **web framework** landscape — Axum, Actix Web, Rocket, Poem — and which fits which job
- Why **Tokio** dominates the async-runtime space, and where async-std and smol fit
- `console.log` → the **`log` facade + `env_logger`**: levels, targets, and the facade/implementation split
- **Structured logging** with `tracing` and `tracing-subscriber`: spans, `#[instrument]`, and JSON logs
- JSDoc → **rustdoc**: doc comments, intra-doc links, examples-as-tests, and publishing to docs.rs
- axios/fetch → **reqwest** (and where hyper fits): JSON GET/POST, headers, and reusing a client
- `Date` → **chrono** and the **time** crate: parsing, formatting, time zones, and durations
- Regular expressions with the **regex** crate: compile-once, captures, and the no-backtracking guarantee
- **Parser combinators** with nom and pest, and when to reach for a real parser over a regex
- A grab-bag of other essentials: **itertools, rayon, once_cell/LazyLock, uuid, indexmap, bytes, dashmap**

---

## Topics

| Topic | Description |
| --- | --- |
| [Popular Crates](/23-ecosystem/00-popular-crates/) | The most-used crates and the npm packages they replace (serde, tokio, clap, reqwest, anyhow, ...). |
| [Web Frameworks](/23-ecosystem/01-web-frameworks/) | The web framework ecosystem — Axum, Actix Web, Rocket, Poem — their maturity and fit. |
| [Async Runtimes](/23-ecosystem/02-async-runtimes/) | Tokio vs async-std vs smol, and why Tokio has become the ecosystem default. |
| [Logging](/23-ecosystem/03-logging/) | `console.log` → the `log` facade + `env_logger`: levels, targets, and the facade/implementation split. |
| [Tracing](/23-ecosystem/04-tracing/) | Structured logging and spans with `tracing` + `tracing-subscriber`: `#[instrument]` and JSON logs. |
| [Documentation](/23-ecosystem/05-documentation/) | JSDoc → rustdoc: doc comments, intra-doc links, examples-as-tests, and publishing to docs.rs. |
| [HTTP Clients](/23-ecosystem/06-http-clients/) | axios/fetch → reqwest (and a note on hyper): JSON GET/POST, headers, and async client reuse. |
| [Date and Time](/23-ecosystem/07-date-time/) | `Date` → chrono and the time crate: parsing/formatting, time zones, and durations. |
| [Regular Expressions](/23-ecosystem/08-regex/) | The regex crate: compile-once, captures, and the linear-time no-backtracking guarantee. |
| [Parsing](/23-ecosystem/09-parsing/) | Parser combinators with nom and pest, and when to reach for a real parser over a regex. |
| [Useful Crates](/23-ecosystem/10-useful-crates/) | Other essentials: itertools, rayon, once_cell/LazyLock, uuid, indexmap, bytes, dashmap. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Map the npm packages you already use onto their Rust crate equivalents and reach for the right one immediately
- Choose a web framework and an async runtime with the same confidence you choose Express vs NestJS
- Add leveled logging with `log` + `env_logger`, then graduate to span-based structured `tracing` when a service needs it
- Document a crate with rustdoc, write examples that double as tests, and understand what docs.rs publishes for free
- Call HTTP APIs with a reusable reqwest client, and parse and format dates and durations with chrono or the time crate
- Decide between a regex, a `serde` deserializer, and a real parser (nom/pest) for a given input
- Round out a project with the second-tier utility crates that fill the gaps Node hides inside the language

---

## Prerequisites

- [Section 12: Modules & Packages](/12-modules-packages/): the ecosystem is delivered as crates, so the `Cargo.toml`, `cargo add`, and dependency model comes first.

> **Note:** Several pages link out to the deeper, hands-on guides where a topic gets full treatment: async mechanics live in [Section 11: Async](/11-async/), and the build-oriented Axum walkthrough lives in [Section 16: Web APIs](/16-web-apis/). The pages here stay at survey altitude: what each crate is, why it won, and when to reach for it.

---

## Estimated Time

- **Reading:** 5 hours
- **Hands-on Practice:** 3 hours
- **Exercises:** 2 hours
- **Total:** 10 hours

> **Tip:** You do not need to read these pages in order. Skim [Popular Crates](/23-ecosystem/00-popular-crates/) first to build a mental map, then jump to whichever crate your current project needs.

---

**Next:** [Section 24: Tooling →](/24-tooling/) — Cargo beyond the basics, rustfmt and Clippy, debugging, rust-analyzer, CI/CD, Docker, and the cargo plugins worth installing.
