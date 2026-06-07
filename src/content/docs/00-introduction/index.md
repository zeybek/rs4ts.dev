---
title: "Introduction"
sidebar:
  label: "Overview"
description: "Welcome to Rust for TS/JS Developers! This section introduces the guide and helps you get oriented."
---

Welcome to **Rust for TS/JS Developers**! This section introduces the guide and helps you get oriented.

---

## What's in This Section

- **[Target Audience](/00-introduction/00-target-audience/)** - Who this guide is for
- **[How to Read This Guide](/00-introduction/01-how-to-read/)** - Learning strategies and navigation
- **[Prerequisites](/00-introduction/02-prerequisites/)** - What you need to know before starting

---

## About This Guide

This is a thorough, book-style guide designed specifically for TypeScript and JavaScript developers who want to learn Rust. Unlike general Rust tutorials, this guide:

### Leverages Your Existing Knowledge

We assume you already know TypeScript/JavaScript well. Instead of explaining programming from scratch, we:

- **Show familiar code first** - Start with TypeScript/JavaScript you already understand
- **Compare side-by-side** - See the Rust equivalent next to familiar code
- **Explain the differences** - Focus on what's new, not what you already know
- **Flag gotchas** - Point out common mistakes TS/JS developers make

### Example: Variables

Instead of just teaching Rust variables, we compare:

**TypeScript:**

```typescript
let x = 5; // mutable
const y = 10; // immutable
x = 6; // allowed
// y = 11;        // error
```

**Rust:**

```rust
let x = 5;        // immutable by default!
let mut y = 10;   // explicitly mutable
// x = 6;         // error - x is immutable
y = 11;           // allowed - y is mut
```

**Key Difference:** Rust is immutable by default (opposite of JavaScript!)

---

## What You'll Learn

### Core Rust Concepts

1. **The Ownership System** - Rust's secret sauce for memory safety without garbage collection
2. **Borrowing & Lifetimes** - How Rust prevents data races at compile time
3. **Type System** - Stronger than TypeScript's, but for good reasons
4. **Error Handling** - No exceptions, use Result and Option instead
5. **Pattern Matching** - Like switch statements on steroids
6. **Traits** - Similar to interfaces but more capable
7. **Async/Await** - Different syntax, same concepts
8. **Macros** - Metaprogramming (like decorators but compile-time)

### Practical Skills

- Building REST APIs (Axum vs Express)
- Working with databases (SQLx, Diesel)
- Creating CLI tools (better performance than Node.js CLIs)
- WebAssembly (run Rust in the browser)
- Systems programming
- Performance optimization
- Production deployment

---

## Why Learn Rust?

### For TypeScript/JavaScript Developers

**You should learn Rust if you want to:**

- Build **high-performance backends** (no GC pauses; far lower p99 latency and memory use than Node.js)
- Create **CLI tools** that start instantly (no Node.js startup time)
- Eliminate **entire classes of bugs** at compile time
- Work on **systems programming** (OS, embedded, game engines)
- Add **Rust to JavaScript** via WebAssembly or native addons
- Learn a language that **makes you a better programmer**

**Rust might NOT be for you if:**

- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> You only build simple web frontends (stick with TypeScript)
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> You need extremely fast development iteration (Node.js is faster to prototype)
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Your team isn't ready for the learning curve
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> You're building CRUD apps where performance isn't critical

### Real-World Use Cases

**Companies using Rust:**

- **Discord** - Rewrote services, 10x performance improvement
- **AWS** - Building Firecracker, Lambda runtime
- **Microsoft** - Windows components, Azure services
- **Meta** - Source control system (Sapling)
- **Cloudflare** - Edge computing platform
- **Dropbox** - File sync engine
- **npm** - Registry infrastructure

**Common Rust Projects:**

- Web servers (faster than Node.js)
- CLI tools (replace Node.js CLIs)
- Game engines
- Blockchain/crypto
- Embedded systems
- Operating systems
- Database engines
- Network tools

---

## Guide Philosophy

### 1. Side-by-Side Comparisons

We **always** show TypeScript/JavaScript first, then Rust. This helps you:

- Connect new concepts to existing knowledge
- Understand why Rust does things differently
- Spot patterns and differences quickly

### 2. Real-World Examples

No toy examples or contrived code. Every example:

- Uses realistic scenarios
- Includes proper error handling
- Follows best practices
- Could be used in production

### 3. Honest About Trade-offs

We don't pretend Rust is perfect. We'll tell you:

- When Rust is harder than TypeScript
- When Node.js might be a better choice
- Where the learning curve gets steep
- How to overcome common struggles

### 4. Progressive Learning

Content is carefully sequenced:

- Foundation → Core Language → Practical Skills → Advanced Topics → Production
- Each section builds on previous ones
- Dependencies are clearly marked
- You can skip ahead, but we don't recommend it

---

## Learning Time Estimates

### Quick Path: 20-30 hours

Focus on essentials: Installation, basics, ownership, web APIs
**Result:** Build a simple Rust web service

### Standard Path: 60-80 hours

Cover sections 00-19 thoroughly
**Result:** Confidently build production Rust applications

### Complete Path: 120-150 hours

All sections + projects + exercises
**Result:** Rust mastery, ready for any Rust project

**Reality check:** Most developers spend 40-60 hours before feeling comfortable with Rust. The ownership system is the hardest part, usually taking 10-20 hours to "click."

---

## How This Guide Is Different

### vs. The Rust Book

- **The Rust Book:** Thorough, assumes no prior programming knowledge
- **This Guide:** Assumes strong TS/JS knowledge, focuses on differences

### vs. Rust by Example

- **Rust by Example:** Brief examples with minimal explanation
- **This Guide:** Detailed explanations with side-by-side comparisons

### vs. YouTube Tutorials

- **YouTube:** Sequential watching, hard to reference
- **This Guide:** Written format, easy to search, return to, and reference

### vs. Rustlings

- **Rustlings:** Small exercises to learn by doing
- **This Guide:** Full explanations + exercises

**Best approach:** Use this guide as your primary resource, supplement with The Rust Book and Rustlings for practice.

---

## The Journey Ahead

### Phase 1: Foundation (Sections 00-05)

**Time:** ~15-20 hours  
**Goal:** Understand Rust basics and the ownership system

This is where most developers struggle. Take your time with ownership!

### Phase 2: Core Language (Sections 06-10)

**Time:** ~20-25 hours  
**Goal:** Master data structures, collections, error handling, traits

You'll start feeling productive here.

### Phase 3: Practical Skills (Sections 11-19)

**Time:** ~25-30 hours  
**Goal:** Build real applications (web APIs, CLI tools, WASM)

This is where Rust becomes fun!

### Phase 4: Advanced Topics (Sections 20-30)

**Time:** ~40-50 hours  
**Goal:** Production readiness and mastery

Polish your skills and build complete projects.

---

## Overcoming the Learning Curve

### Common Struggles

**Week 1:** "Why won't the compiler let me do this simple thing?"

- Normal! The borrow checker is strict
- Your code is probably not wrong, just needs restructuring
- This teaches you to write better code

**Week 2-3:** "I'm fighting with lifetimes!"

- Also normal! Lifetimes are the hardest part
- Most code doesn't need explicit lifetime annotations
- It gets easier with practice

**Week 4+:** "Oh, I get it now!"

- The "aha!" moment
- Everything starts to make sense
- You begin appreciating Rust's design

### Tips for Success

1. **Don't fight the compiler** - It's trying to help you
2. **Read error messages carefully** - They're very helpful
3. **Practice daily** - Even 30 minutes helps
4. **Join the community** - Discord, Reddit, forums
5. **Build projects** - Theory only goes so far
6. **Be patient** - Everyone struggles at first
7. **Celebrate small wins** - You're learning a hard language!

---

## Next Steps

Ready to begin? Here's your roadmap:

1. **You're here!** - Introduction
2. **[Target Audience](/00-introduction/00-target-audience/)** - Make sure this guide is right for you
3. **[How to Read](/00-introduction/01-how-to-read/)** - Learn navigation strategies
4. **[Prerequisites](/00-introduction/02-prerequisites/)** - Verify you're ready
5. **[Section 01: Getting Started](/01-getting-started/)** - Install Rust and write your first program!

---

## Questions?

- **"Should I learn Rust or Go?"** - Different use cases. Rust for systems/performance, Go for simplicity/concurrency
- **"Will this replace JavaScript?"** - No, different domains. But you might build backends in Rust
- **"How long until I'm productive?"** - 3-4 weeks for basic projects, 2-3 months for production code
- **"Is Rust dying?"** - No! It's been "most loved language" on Stack Overflow for 8 years running
- **"Should I learn Rust in 2025?"** - Yes! Adoption is growing, especially in backend/systems

---

## Let's Begin!

You're about to learn one of the most capable programming languages in existence. It will be challenging, but incredibly rewarding.

**Remember:** Every Rust developer started exactly where you are now. You've got this!

### [Continue to Target Audience](/00-introduction/00-target-audience/)
