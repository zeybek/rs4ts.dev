---
title: "Rust Web APIs with Axum"
sidebar:
  label: "Overview"
description: "Build web APIs in Rust with Axum, mapping Express's router-handler-middleware model to typed extractors, IntoResponse, and Tower layers on Tokio for a full JSON API."
---

In Node.js you reach for Express, NestJS, or Fastify and trust the framework to wire up routing, body parsing, and middleware. Rust's web story keeps that same shape but moves the guarantees into the type system: **Axum** — built on the [Tokio](/11-async/) async runtime and the Tower middleware ecosystem — turns request inputs into typed **extractors**, responses into anything that implements `IntoResponse`, and middleware into composable **layers**. This section takes you from a one-route hello-server to a production-flavored JSON API with shared state, validation, typed error handling, CORS, JWT authentication, sessions, WebSockets, Server-Sent Events, multipart uploads, static files, and a Dockerized deployment.

Every example targets **axum 0.8** (current stable 0.8.9): servers are started with `axum::serve(listener, app)` over a `tokio::net::TcpListener` (the removed `Server::bind().serve()` builder is gone), and path parameters use `{id}` brace syntax (the old `:id` colon form was dropped in 0.8). The toolchain throughout is Rust 1.96.0 on the latest stable edition (2024), which `cargo new` selects automatically.

---

## What You'll Learn

- How the Express mental model — router, handlers, `(req, res)`, middleware — maps onto Axum's `Router`, async `fn` handlers, extractors, and Tower layers
- How to choose between Axum, Actix Web, and Rocket the way you'd choose between Express, NestJS, and Fastify
- How to set up a Tokio + Axum project and run a server with `axum::serve` and `tokio::net::TcpListener`
- Routing: path params (`{id}` — never `:id` in 0.8), query strings, method routing, nested routers, and fallbacks
- Extractors (`Path`, `Query`, `Json`, `State`, headers), how `FromRequest`/`FromRequestParts` work, and why extractor **ordering** matters
- Middleware as Tower layers: `tower-http` (`TraceLayer`, `CorsLayer`, compression) and custom `from_fn` middleware
- Shared application state with `State<T>` and `Arc`, when to reach for request extensions, and injecting a DB pool or config
- Building JSON REST APIs with Serde and the `Json` extractor/response, including a full CRUD resource
- Request validation and returning helpful `400 Bad Request` responses
- Typed error handling with a custom `AppError` that implements `IntoResponse`, backed by `thiserror`
- CORS, authentication (extractor-as-guard and middleware), JWT, and cookie/server-side sessions
- Real-time features: WebSockets and Server-Sent Events
- Multipart file uploads, serving static files with an SPA fallback, and deploying with a multi-stage Docker build

---

## Topics

| Topic | Description |
| --- | --- |
| [Framework Comparison](/16-web-apis/00-framework-comparison/) | Axum vs Actix Web vs Rocket (vs Express/NestJS): tradeoffs and when to pick which. |
| [Axum Basics](/16-web-apis/01-axum-basics/) | Express.js → Axum fundamentals: `Router`, handlers, async `fn` handlers, running with `axum::serve` + `tokio::net::TcpListener` (0.8). |
| [Axum Setup](/16-web-apis/02-axum-setup/) | Setting up an Axum project: tokio + axum deps/features, project layout, and a compile-verified hello-server. |
| [Routing](/16-web-apis/03-routing/) | Route handlers, path params (`{id}` in 0.8 — **not** `:id`), query params, method routing, nested routers, and fallback. |
| [Extractors](/16-web-apis/04-extractors/) | Request extractors: `Path`, `Query`, `Json`, `State`, headers; how `FromRequest`/`FromRequestParts` work; extractor ordering. |
| [Middleware](/16-web-apis/05-middleware/) | Express middleware → Tower/Axum layers; `tower-http` (`TraceLayer`, `CorsLayer`, compression); `from_fn` middleware. |
| [State Management](/16-web-apis/06-state-management/) | Shared application state with `State<T>` + `Arc`; when to use extensions; injecting a DB pool or config. |
| [Request and Response](/16-web-apis/07-request-response/) | Request/response handling; `IntoResponse`; status codes; setting headers; `(StatusCode, Json)` tuples. |
| [JSON APIs](/16-web-apis/08-json-apis/) | Building JSON REST APIs with Serde and the `Json` extractor/response; a small CRUD resource. |
| [Validation](/16-web-apis/09-validation/) | Request validation (the `validator` crate and/or manual) and returning `400`s with helpful messages. |
| [Error Handling](/16-web-apis/10-error-handling-web/) | Error handling in handlers: a custom `AppError` implementing `IntoResponse`, `thiserror`, mapping errors to status codes. |
| [CORS](/16-web-apis/11-cors/) | CORS configuration with `tower-http` `CorsLayer`; permissive vs locked-down. |
| [Authentication](/16-web-apis/12-authentication/) | Authentication patterns: extractor-as-guard, middleware auth, an `AuthUser` extractor. |
| [JWT](/16-web-apis/13-jwt/) | JWT authentication with the `jsonwebtoken` crate: encoding/decoding, `Claims`, expiry, verifying in an extractor. |
| [Sessions](/16-web-apis/14-sessions/) | Session management: cookies, server-side sessions (`tower-sessions`), and CSRF considerations. |
| [WebSockets](/16-web-apis/15-websockets/) | WebSocket support with `axum::extract::ws`: upgrade, send/receive loop, echo server. |
| [Server-Sent Events](/16-web-apis/16-sse/) | Server-Sent Events with `axum::response::Sse` and a `Stream` of events. |
| [File Uploads](/16-web-apis/17-file-uploads/) | Handling multipart file uploads (axum `Multipart`); streaming to disk. |
| [Static Files](/16-web-apis/18-static-files/) | Serving static files with `tower-http` `ServeDir`/`ServeFile`; SPA fallback. |
| [Deployment](/16-web-apis/19-deployment/) | Deploying Axum apps: release builds, multi-stage Docker, binding `0.0.0.0`, reverse proxy, and env config. |

---

## Learning Objectives

By the end of this section, you will be able to:

- Explain how Axum's router-plus-handlers model corresponds to Express, and where the analogy breaks down (extractors instead of `req`, `IntoResponse` instead of `res`, lazy futures that need a runtime)
- Choose a Rust web framework deliberately based on philosophy, ecosystem fit, and team experience
- Stand up an Axum project from scratch and serve it with `axum::serve` + `tokio::net::TcpListener`
- Define routes with path and query parameters, method routing, nested routers, and fallbacks using current 0.8 syntax
- Pull typed inputs out of requests with the standard extractors and reason about why ordering matters
- Compose cross-cutting concerns as Tower layers and write custom middleware with `from_fn`
- Share a database pool, configuration, or in-memory store across handlers with `State<T>` and `Arc`
- Build a complete JSON CRUD resource that validates input, returns correct status codes, and maps errors to responses through a single `AppError`
- Add CORS, JWT-based and middleware-based authentication, and cookie/server-side sessions
- Implement real-time endpoints with WebSockets and Server-Sent Events, accept file uploads, serve static assets, and ship the result in a Docker container

---

## Prerequisites

- [Section 11: Async Programming](/11-async/) — Axum is built on Tokio. Handlers are `async fn`s, the server is started inside `#[tokio::main]`, and Rust futures are **lazy** and need a runtime to drive them. The async vocabulary from Section 11 is assumed throughout.
- [Section 15: Serialization](/15-serialization/) — request bodies and JSON responses ride on Serde. The `Json` extractor deserializes with `Deserialize` and the `Json` response serializes with `Serialize`, so the derive macros and attributes from Section 15 carry over directly.
- Helpful but not required: [Section 08: Error Handling](/08-error-handling/) — the `AppError`/`thiserror`/`?` pattern in [Error Handling](/16-web-apis/10-error-handling-web/) builds on that section's error vocabulary.

---

## Estimated Time

- **Reading:** 8-9 hours
- **Hands-on Practice:** 6-7 hours
- **Exercises:** 4 hours
- **Total:** 18-20 hours

> **Tip:** Read [Framework Comparison](/16-web-apis/00-framework-comparison/) → [Axum Basics](/16-web-apis/01-axum-basics/) → [Axum Setup](/16-web-apis/02-axum-setup/) → [Routing](/16-web-apis/03-routing/) → [Extractors](/16-web-apis/04-extractors/) as one connected track — that gives you a working server and the core request model. Then [State Management](/16-web-apis/06-state-management/), [Request and Response](/16-web-apis/07-request-response/), [JSON APIs](/16-web-apis/08-json-apis/), [Validation](/16-web-apis/09-validation/), and [Error Handling](/16-web-apis/10-error-handling-web/) compose into a real API. Treat [Middleware](/16-web-apis/05-middleware/), [CORS](/16-web-apis/11-cors/), [Authentication](/16-web-apis/12-authentication/), [JWT](/16-web-apis/13-jwt/), [Sessions](/16-web-apis/14-sessions/), [WebSockets](/16-web-apis/15-websockets/), [Server-Sent Events](/16-web-apis/16-sse/), [File Uploads](/16-web-apis/17-file-uploads/), [Static Files](/16-web-apis/18-static-files/), and [Deployment](/16-web-apis/19-deployment/) as a toolbox to pull from as each need arises.

---

**Next:** [Section 17: Database →](/17-database/) — connecting your Axum API to PostgreSQL, SQLite, MongoDB, and Redis with SQLx, Diesel, and SeaORM, including the connection pool you'll inject via `State`.
