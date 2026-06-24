---
title: "Rust in Production"
sidebar:
  label: "Overview"
description: "Getting a Rust service past \"it builds\" to production: typed config, graceful shutdown, health probes, metrics, tracing, rate limits, caching, and jobs."
---

Compiling and passing tests is the easy part; running a Rust service that survives real traffic, real failures, and a 3 a.m. page is the work this section covers. We move from "it builds" to "it operates": loading configuration in a disciplined, type-safe way; following the 12-factor model for environment-based config; shutting down gracefully so deploys cause zero dropped requests; exposing liveness and readiness probes; emitting metrics and distributed traces you can actually query; protecting the service with rate limiting and caching; running work outside the request cycle with background jobs; and finishing with a production readiness checklist you can run before shipping. Every code example is compile-verified against the current stable toolchain (Rust 1.96.0, 2024 edition — `cargo new` selects it automatically) and the current crate ecosystem (Axum 0.8, Tokio 1, Tower, the `tracing` and `metrics` ecosystems, `moka`, `redis`, and `tower-governor`).

---

## What You'll Learn

- How to load **layered, typed configuration** (defaults → files → environment) with the `config` and `figment` crates, validated once at startup instead of surfacing as `undefined` mid-request.
- How to apply the **12-factor** model: keep config in the environment, use `dotenvy` only in development, and fail loudly the instant a required variable is missing or malformed.
- How to shut down **gracefully** with Tokio signal handling and `axum::serve(...).with_graceful_shutdown(...)`, draining in-flight requests so a redeploy never returns `502`s.
- The difference between **liveness** and **readiness** probes, why conflating them is the most common health-check bug, and how to check dependencies safely.
- How to instrument a service with the **`metrics` facade** and a **Prometheus exporter**, and how to choose *which* numbers matter using the RED and USE methods.
- How to wire **distributed tracing** with `tracing` + OpenTelemetry, propagating trace context across service boundaries via request headers.
- How to add **rate limiting** with a Tower layer (`tower-governor`): per-IP limits, a global cap, and per-route policies.
- How to combine an **in-process (L1) cache** (`moka`) with a **shared (L2) cache** (Redis), set sensible TTLs, and handle the hard part — invalidation.
- How to run **background jobs** with `tokio::spawn`, bounded channels, and a dedicated runner — with backpressure, retries, and clean draining — plus where an in-process queue stops being enough.
- A concrete **production readiness checklist** spanning logging, error handling, timeouts, limits, observability, and security.

---

## Topics

| Topic | Description |
| --- | --- |
| [Application Configuration](/28-production/00-configuration/) | Application configuration: the `config` and `figment` crates, layered sources, and typed settings structs validated at startup. |
| [Environment-Based Configuration](/28-production/01-environment/) | Environment-based config: the 12-factor model, `dotenvy` in development, and validating required environment variables at startup. |
| [Graceful Shutdown](/28-production/02-graceful-shutdown/) | Graceful shutdown: Tokio signal handling plus `axum`'s `with_graceful_shutdown`, draining in-flight requests cleanly. |
| [Health and Readiness Endpoints](/28-production/03-health-checks/) | Health and readiness endpoints: liveness vs readiness, and how to check downstream dependencies safely. |
| [Metrics and Monitoring](/28-production/04-metrics/) | Metrics and monitoring: the `metrics` crate, a Prometheus exporter, and choosing signals with the RED and USE methods. |
| [Distributed Tracing](/28-production/05-distributed-tracing/) | Distributed tracing: `tracing` + OpenTelemetry, spans woven into the type system, and propagating context across services. |
| [Rate Limiting](/28-production/06-rate-limiting/) | Rate limiting: `tower-governor` as a Tower layer — per-IP limits, a global cap, and per-route policies. |
| [Caching Strategies](/28-production/07-caching/) | Caching strategies: in-process (`moka`) and shared (Redis) tiers, setting TTLs, and invalidation without stale data. |
| [Background Job Processing](/28-production/08-background-jobs/) | Background job processing: `tokio::spawn`, bounded channels, and a dedicated runner task with backpressure and retries. |
| [Production Readiness Checklist](/28-production/09-production-checklist/) | A production readiness checklist: logging, errors, timeouts, limits, observability, and security. |

---

## Learning Objectives

By the end of this section, you will be able to:

- **Load configuration safely** by merging defaults, files, and environment variables into one typed struct, and reject a misconfigured process before it accepts traffic.
- **Operate the same binary everywhere** by following the 12-factor model, using `dotenvy` for local development and validating required environment variables at boot.
- **Deploy with zero downtime** by catching `SIGTERM`/`SIGINT` with Tokio and draining in-flight requests through `axum::serve(...).with_graceful_shutdown(...)`.
- **Expose correct health probes** that distinguish "restart me" (liveness) from "stop routing to me" (readiness), without making liveness depend on the database.
- **Instrument and observe** a service with counters, gauges, and histograms exported to Prometheus, and with `tracing` spans exported to an OpenTelemetry backend.
- **Defend the service** with per-IP and global rate limits, and reduce load with a layered in-process plus Redis cache that you can invalidate correctly.
- **Move work off the request path** with bounded, observable, gracefully-drainable background jobs — and recognize when you need a durable queue instead.
- **Run a readiness review** against a concrete checklist before a service takes production traffic.

---

## Prerequisites

This section assembles the building blocks from earlier sections into an operable service. Before starting, you should be comfortable with:

- [Section 16: Web APIs](/16-web-apis/) — Axum 0.8 routing, extractors, middleware/Tower layers, and JSON responses, which nearly every example here builds on.
- [Section 23: Ecosystem](/23-ecosystem/) — the crate landscape (Tokio, Serde, `tracing`, reqwest) and the facade/implementation split that configuration, metrics, and tracing all rely on.

It also leans on [Section 05: Ownership](/05-ownership/) (shared state via `Arc`), [Section 08: Error Handling](/08-error-handling/), and [Section 11: Async](/11-async/) (tasks, channels, and `tokio::select!`), which the shutdown, caching, and background-job pages use directly.

---

## Estimated Time

- **Reading:** 7 hours
- **Hands-on Practice:** 5 hours
- **Exercises:** 2 hours
- **Total:** ~14 hours

> **Tip:** These pages stand on their own, so you can read them in the order your current service needs. A natural first pass is configuration → environment → graceful shutdown → health checks (the operational baseline), then metrics → distributed tracing (observability), and finally rate limiting → caching → background jobs before running the [production checklist](/28-production/09-production-checklist/).

---

**Next:** [Section 29: Migration Guide →](/29-migration-guide/) — the strategy for moving a Node.js service to Rust incrementally, keeping the API and data compatible, and measuring the performance payoff honestly.
