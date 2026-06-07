# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- **Complete content for all 31 sections (00–30)** (~330 files, ~221,000 lines): the language core (00–15) plus the practical ecosystem and production track (16–30) — Web APIs (Axum), Database (SQLx/Diesel/MongoDB/Redis), CLI tools (clap/ratatui), WebAssembly (wasm-bindgen), Unsafe/FFI, Performance, Common Patterns, Ecosystem, Tooling, Advanced Topics, Systems Programming, Security, Production, and a Migration guide. Each topic file follows the 10-part format with side-by-side TypeScript/Rust comparisons, pitfalls, best practices, real-world examples, and exercises.
- **Section 30 — six complete capstone projects** with runnable code directories that compile: a REST API (Axum), a CLI task manager (clap), a WASM Game of Life, a WebSocket chat server, a URL-shortener microservice, and a full-stack app (Axum backend + WASM frontend).
- Section landing-page READMEs for every section (03–30).
- LICENSE (MIT).

### Changed

- All Rust examples updated to and verified against the current stable toolchain (Rust 1.96.0, 2024 edition) and current crate versions (axum 0.8, tokio 1.52, serde 1, thiserror 2, anyhow 1, syn 2, criterion 0.8, proptest 1.11, mockall 0.14, sqlx 0.8/0.9, diesel 2, clap 4, wasm-bindgen 0.2, validator 0.20, uuid 1, chrono 0.4, jsonwebtoken 9, argon2 0.5, rayon 1, tower-http, regex 1).
- README, PROGRESS, and TODO updated to reflect 100% completion — all 31 sections written and code-verified.

### Fixed

- Sections 01–02: corrected fabricated compiler/Clippy output, a non-compiling Axum example (now `axum::serve` + `{id}` routes on axum 0.8), the Discord "Go → Rust" case study, inline format-arg idioms, accurate JavaScript/Node behavior claims, and other review findings.
- Removed rustdoc/mdBook hidden-line (`# `) syntax that rendered literally in plain-markdown code blocks; added dependency notes to examples using external crates.

### Notes

- Every runnable Rust example is compile-verified; intentionally non-compiling snippets (illustrating pitfalls) are marked as such.

### Status

- All 31 sections (00–30) are written and code-verified against Rust 1.96.0 (2024 edition), including the six capstone project code directories.

---

## [0.1.0] - 2025-10-25

### Project Inception

- Established project goals and scope
- Created comprehensive plan for 30 content sections
- Defined learning path from basics to advanced topics
- Set up project documentation infrastructure

### Planned Sections

**Foundation (00-05)**

- 00-introduction: Book overview and prerequisites
- 01-getting-started: Installation and first steps
- 02-basics: Variables, types, and operators
- 03-functions: Functions and closures
- 04-control-flow: Conditionals and loops
- 05-ownership: The ownership system (critical!)

**Core Language (06-10)**

- 06-data-structures: Structs and enums
- 07-collections: Vec, String, HashMap
- 08-error-handling: Result and Option
- 09-generics-traits: Generics and trait system
- 10-smart-pointers: Box, Rc, Arc, RefCell

**Async & Organization (11-14)**

- 11-async: Async/await and Tokio
- 12-modules-packages: Module system and Cargo
- 13-testing: Testing strategies
- 14-macros: Macro system

**Practical Skills (15-19)**

- 15-serialization: Serde and data formats
- 16-web-apis: Web frameworks (Axum)
- 17-database: Database access patterns
- 18-cli-tools: CLI development
- 19-wasm: WebAssembly integration

**Advanced Topics (20-26)**

- 20-unsafe-ffi: Unsafe Rust and FFI
- 21-performance: Optimization techniques
- 22-common-patterns: Design patterns
- 23-ecosystem: Popular crates
- 24-tooling: Development tools
- 25-advanced-topics: Advanced features
- 26-systems-programming: Systems programming

**Production & Projects (27-30)**

- 27-security: Security best practices
- 28-production: Production deployment
- 29-migration-guide: Migration strategies
- 30-projects: Six complete project examples

---

## Version History

### Version Numbering

- **Major version (X.0.0)**: Complete releases, major restructuring
- **Minor version (0.X.0)**: New sections or significant content additions
- **Patch version (0.0.X)**: Bug fixes, corrections, minor improvements

### Release Schedule

- **Alpha (0.x.x)**: Initial development, content creation
- **Beta (1.x.x)**: Content complete, under review
- **Stable (2.0.0)**: Fully reviewed and polished
- **Future (2.x.x)**: Ongoing improvements and updates

---

## Notes

### How to Update This Changelog

When making changes, add entries under the `[Unreleased]` section in the appropriate category:

- **Added**: New features or content
- **Changed**: Changes to existing content
- **Deprecated**: Features or approaches being phased out
- **Removed**: Removed features or content
- **Fixed**: Bug fixes or corrections
- **Security**: Security-related changes

When releasing a new version, move entries from `[Unreleased]` to a new version section with the release date.

### Categories

- **Added**: New sections, topics, or features
- **Changed**: Updates to existing content
- **Deprecated**: Soon-to-be removed content
- **Removed**: Deleted sections or deprecated content
- **Fixed**: Corrections to errors or bugs
- **Security**: Security improvements or fixes

---

_Last Updated: 2025-10-25_
