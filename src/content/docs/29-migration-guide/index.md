---
title: "Node.js to Rust Migration Guide"
sidebar:
  label: "Overview"
description: "A pragmatic playbook for migrating Node.js services to Rust: strangler-fig increments, API and data compatibility, honest benchmarks, and team challenges."
---

Migrating from Node.js to Rust is a strategy problem before it is a coding problem. This section takes the pragmatic path: move incrementally rather than betting the company on a big-bang rewrite, port a Node service one endpoint at a time while keeping its behavior identical, maintain wire-level API compatibility so clients never notice, migrate the data underneath without downtime, measure the performance payoff honestly, and go in clear-eyed about the challenges teams actually hit — including the cases where you should not migrate at all. Every code example is compile-verified against the current stable toolchain (Rust 1.96.0, 2024 edition) and the crate versions it targets (Axum 0.8, Tokio 1, Serde 1, sqlx 0.8, Criterion 0.8).

---

## What You'll Learn

- How to apply the **strangler-fig pattern** to replace a Node.js system slice by slice, with an instant rollback path at every step.
- How to **port an Express endpoint to Axum** so that clients cannot tell the implementation changed — same JSON, same status codes, same headers.
- How to keep the **API contract byte-for-byte compatible** using Serde attributes (`rename_all`, `skip_serializing_if`, custom serializers) and golden-fixture tests.
- The three **data-migration strategies** — shared database, dual-write, and backfill — and how to gate a cutover on a reconciliation report instead of optimism.
- How to **measure performance honestly**: percentiles over averages, the right unit of work, memory (RSS) as well as latency, and the measurement traps (debug builds, coordinated omission) that produce misleading numbers.
- The real **human and ecosystem challenges**: the ownership learning curve, crate-ecosystem gaps, team ramp-up, and a decision framework for *when not to migrate*.

---

## Topics

| Topic | What it covers |
| --- | --- |
| [Incremental Migration](/29-migration-guide/00-incremental/) | The strangler-fig approach: running Node and Rust side by side behind a proxy, going service-by-service, and porting the hot paths first. |
| [Porting a Node.js Service to Rust](/29-migration-guide/01-node-to-rust/) | A worked Express-to-Axum walkthrough that keeps observable behavior identical, including the subtle path-parsing differences a naive port gets wrong. |
| [Maintaining API Compatibility](/29-migration-guide/02-api-compatibility/) | Matching JSON shapes (casing, `null`-vs-omitted, big integers, dates), status codes, and headers so the wire contract does not drift. |
| [Data Migration Strategies](/29-migration-guide/03-data-migration/) | Database migration during a rewrite: shared DB, dual-write, chunked/resumable/idempotent backfill, and a reconciliation gate before cutover. |
| [Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/) | Benchmarking the right thing: latency percentiles (p50/p99), memory footprint, Criterion and HdrHistogram, and avoiding coordinated omission. |
| [Common Migration Challenges](/29-migration-guide/05-common-challenges/) | The ownership learning curve, ecosystem gaps, team ramp-up, and a framework for deciding when migrating is the wrong call. |

---

## Learning Objectives

By the end of this section you will be able to:

- **Plan a migration** by choosing high-value slices (hot paths) instead of easy ones, and sequencing them to ship and roll back independently.
- **Translate** an Express handler into an idiomatic Axum handler whose responses are indistinguishable from the original.
- **Pin a wire contract** with golden fixtures and contract tests so neither side can break the JSON shape, status codes, or headers by accident.
- **Move live data** between a Node-owned and a Rust-owned store using dual-write and backfill, and prove the stores agree before cutting over.
- **Report a defensible performance result** with percentiles and memory numbers, stated alongside the conditions that make it reproducible.
- **Make the migrate / don't-migrate decision** on measurable grounds, recognizing the I/O-bound, churning, or ecosystem-gap cases where Rust is the wrong tool.

---

## Prerequisites

This section assumes you have worked through the building blocks it ties together:

- [Section 16: Web APIs](/16-web-apis/) — Axum routing, extractors, JSON responses, and error handling, which the Express-to-Axum port builds on.
- [Section 17: Database](/17-database/) — `sqlx`, connection pooling, transactions, and schema migrations, which the data-migration strategies rely on.
- [Section 28: Production](/28-production/) — configuration, health checks, metrics, graceful shutdown, and tracing, which a migrated service needs to operate safely.

It also leans on the foundations in [Section 05: Ownership](/05-ownership/) (the heart of the learning curve), [Section 08: Error Handling](/08-error-handling/), [Section 11: Async](/11-async/01-async-await/), [Section 15: Serialization](/15-serialization/), and [Section 21: Performance](/21-performance/).

---

## Estimated Time

**Approximately 10 hours** — roughly 1.5 hours per topic for reading and running the examples, plus the exercises. Budget more if you follow along by standing up a real Node service and porting one endpoint end to end, which is the best way to internalize the material.

---

**Next:** [Section 30: Projects](/30-projects/) — apply everything from the guide, including this migration playbook, on full end-to-end builds.
