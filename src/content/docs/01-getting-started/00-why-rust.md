---
title: "Why Rust for TS/JS Developers?"
description: "You already know TypeScript and JavaScript. Why should you invest time learning Rust? Let's compare the two and explore where Rust excels."
---

You already know TypeScript and JavaScript. Why should you invest time learning Rust? Let's compare the two and explore where Rust excels.

---

## Quick Overview

Rust is **not** a replacement for TypeScript/JavaScript. They solve different problems:

- **TypeScript/JavaScript:** Web development, rapid prototyping, full-stack apps
- **Rust:** Performance-critical backends, systems programming, CLI tools, WebAssembly

**Think of it as:** Adding another tool to your toolbox, not replacing your existing tools.

---

## TypeScript/JavaScript Example

Let's start with a familiar example - a web server that handles user requests:

```typescript
// Express.js server
import express from "express";

interface User {
  id: number;
  name: string;
  email: string;
}

const app = express();
const users: User[] = [];

app.get("/users/:id", (req, res) => {
  const id = parseInt(req.params.id);
  const user = users.find((u) => u.id === id);

  if (user) {
    res.json(user);
  } else {
    res.status(404).json({ error: "User not found" });
  }
});

app.listen(3000, () => {
  console.log("Server running on port 3000");
});
```

**Characteristics:**

- Quick to write
- Easy to understand
- Fast development iteration
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Single-threaded (one CPU core)
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> ~50-100ms startup time
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> ~50-200 MB memory baseline
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Garbage collection pauses
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Runtime errors possible

---

## Rust Equivalent

The same server in Rust (using the **Axum** framework, version 0.8):

```rust
// Axum server
use axum::{
    extract::Path,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

type UserDb = Arc<RwLock<Vec<User>>>;

async fn get_user(
    Path(id): Path<u32>,
    db: axum::extract::State<UserDb>,
) -> Result<Json<User>, StatusCode> {
    let users = db.read().await;
    users
        .iter()
        .find(|u| u.id == id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[tokio::main]
async fn main() {
    let db: UserDb = Arc::new(RwLock::new(Vec::new()));

    let app = Router::new()
        .route("/users/{id}", get(get_user))
        .with_state(db);

    println!("Server running on port 3000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

**Characteristics:**

- Multi-threaded (the Tokio runtime can use all CPU cores)
- <1ms startup time
- ~2-5 MB memory baseline
- No garbage collection pauses
- Strong compile-time guarantees (whole classes of bugs are caught before runtime, though panics like `unwrap()` still exist)
- Often several times faster than Node.js for CPU-bound work (varies widely by workload)
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> More verbose
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Slower compilation
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Steeper learning curve

---

## Detailed Explanation

### Memory Management

**TypeScript/JavaScript:**

```typescript
let data = { value: 42 };
let data2 = data; // Both reference same object
data = null; // data2 still points to object
// Garbage collector cleans up eventually
```

- Memory is automatically managed
- Garbage collector runs periodically
- Unpredictable pauses (can be 1-100ms)
- Memory overhead for GC bookkeeping

**Rust:**

```rust
let data = String::from("hello");
let data2 = data; // Ownership moved to data2
// data is no longer valid - compile error if used!
// Memory freed immediately when data2 goes out of scope
```

- Memory is tracked at compile time
- No garbage collector needed
- Predictable performance (no pauses)
- Minimal memory overhead

**Why it matters:** For web servers handling thousands of requests, GC pauses can add up. Rust's approach eliminates these pauses entirely.

### Concurrency

**TypeScript/JavaScript:**

```typescript
// Single-threaded, event loop
async function processRequests(requests: Request[]) {
  // Processes one at a time (unless you spawn workers)
  for (const req of requests) {
    await handleRequest(req);
  }
}
```

- Single-threaded by default
- Worker threads are complex to use
- Async/await for I/O concurrency
- No true parallelism (without workers)

**Rust:**

```rust
// Opt-in parallelism: each task can run on any worker thread
async fn process_requests(requests: Vec<Request>) {
    // Processes in parallel across all CPU cores
    let handles: Vec<_> = requests
        .into_iter()
        .map(|req| tokio::spawn(handle_request(req)))
        .collect();

    for handle in handles {
        handle.await.unwrap();
    }
}
```

- Safe, opt-in multithreading (fearless concurrency)
- Thread safety guaranteed at compile time
- No data races possible
- True parallelism

**Why it matters:** CPU-intensive tasks (image processing, data transformation) run much faster in Rust because they can use all cores safely.

### Error Handling

**TypeScript/JavaScript:**

```typescript
function getUser(id: number): User {
  const user = database.find(id);
  if (!user) {
    throw new Error("User not found"); // Exception can crash the app
  }
  return user;
}

// Easy to forget error handling
const user = getUser(123); // What if this throws?
```

- Exceptions can be thrown anywhere
- Easy to forget to catch them
- Runtime crashes possible
- Try/catch adds noise

**Rust:**

```rust
fn get_user(id: u32) -> Result<User, String> {
    match database.find(id) {
        Some(user) => Ok(user),
        None => Err("User not found".to_string()),
    }
}

// Compiler forces you to handle errors
let user = get_user(123)?; // Must handle the Result
```

- No exceptions
- Errors are explicit in the type system
- Compiler ensures you handle them
- `?` operator for convenient error propagation

**Why it matters:** In production, unhandled exceptions cause outages. Rust makes it impossible to forget error handling.

---

## Key Differences

### 1. **Performance**

| Benchmark                  | Node.js | Rust       |
| -------------------------- | ------- | ---------- |
| HTTP requests/sec          | 20k     | 300k-1M    |
| JSON parsing (1 MB)        | 15ms    | 0.5ms      |
| Startup time               | 50ms    | <1ms       |
| Memory baseline            | 50 MB   | 2 MB       |
| CPU-bound task (1 core)    | 100ms   | 3ms        |
| CPU-bound task (all cores) | 100ms   | 0.4ms (8c) |

_Approximate figures, varies by workload_

**When it matters:**

- High-traffic APIs (save on cloud costs)
- Real-time systems (gaming, trading)
- CLI tools (instant startup)
- Data processing pipelines

**When it doesn't:**

- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Low-traffic CRUD apps
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Simple websites
- <span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> Prototypes

### 2. **Safety**

**TypeScript:**

```typescript
function getFirst<T>(arr: T[]): T {
  return arr[0]; // Returns `undefined` if empty, but the type claims `T` -- a latent bug
}
```

**Rust:**

```rust
fn get_first<T>(arr: &[T]) -> Option<&T> {
    arr.first() // Returns Option, forces you to handle empty case
}
```

**What Rust prevents at compile time:**

- Null/undefined dereferences
- Buffer overflows
- Data races
- Use-after-free
- Double-free
- Iterator invalidation

**In TypeScript/JavaScript, these surface as runtime errors or silent bugs!**

### 3. **Deployment**

**TypeScript/JavaScript:**

```bash
# Need Node.js runtime on server
node dist/index.js

# Docker image: ~200-500 MB
FROM node:18
COPY package*.json ./
RUN npm install
COPY . .
CMD ["node", "dist/index.js"]
```

**Rust:**

```bash
# Single binary, no runtime needed
./my-app

# Docker image: ~10-20 MB
FROM scratch
COPY ./my-app /
CMD ["/my-app"]
```

**Why it matters:**

- Faster deployments
- Smaller container images
- No runtime vulnerabilities
- Easier distribution (single file!)

### 4. **Type System**

**TypeScript:**

```typescript
// Type system can be escaped
let x: any = "hello";
x = 123; // Allowed with 'any'

// Under strict mode, the optional type is enforced
let y: string | undefined;
console.log(y.length); // Compile error under strict mode: 'y' is possibly 'undefined'
```

**Rust:**

```rust
// No escape hatches (except unsafe blocks)
let x: String = String::from("hello");
// x = 123; // Compile error!

// Optional is enforced
let y: Option<String> = None;
// println!("{}", y.len()); // Compile error!
println!("{}", y.unwrap_or_default().len()); // Must handle None case
```

**Why it matters:** Rust catches more bugs at compile time, TypeScript catches some but allows escape hatches.

---

## Common Pitfalls

### Pitfall 1: "Rust will make me more productive immediately"

**Reality:** Rust has a steep learning curve. Expect 3-4 weeks before you're comfortable, 2-3 months before you're productive.

**Mitigation:** Start with small projects, don't rewrite critical systems immediately.

### Pitfall 2: "I should rewrite everything in Rust"

**Reality:** Most web apps don't need Rust's performance. Node.js is fine for CRUD apps.

**Use Rust when:**

- You have performance problems
- You're building a new performance-critical service
- You need predictable latency
- You're building CLI tools

**Stick with Node.js when:**

- Rapid prototyping
- Simple CRUD apps
- Small teams without Rust experience
- Time-to-market is critical

### Pitfall 3: "Rust is just like TypeScript with different syntax"

**Reality:** The ownership system is completely new. You'll need to think differently about data lifecycle.

**What's similar:**

- Async/await syntax
- Type system concepts
- Generics

**What's different:**

- Ownership and borrowing
- No garbage collection
- No null (use Option instead)
- No exceptions (use Result instead)

### Pitfall 4: "Fighting the compiler"

**Reality:** The Rust compiler is strict. Don't fight it - learn from it!

```rust
// This won't compile
let s = String::from("hello");
let s2 = s;
println!("{}", s); // Error: value borrowed after move
```

**Don't think:** "The compiler is annoying!"  
**Think:** "The compiler is preventing a bug I would have in production!"

---

## Best Practices

### 1. **Use Rust for the Right Problems**

<span class="inline-icon inline-icon--check" role="img" aria-label="yes"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg></span> **Great for:**

- High-performance APIs
- CLI tools
- Systems programming
- WebAssembly
- Microservices with high load
- Replacing Python/Ruby/Node.js bottlenecks

<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **Not ideal for:**

- Simple CRUD apps
- Rapid prototyping
- When team doesn't know Rust
- When time-to-market is critical

### 2. **Hybrid Approach**

Many companies use **both** Node.js and Rust:

```
TypeScript/Node.js:
- Web frontend (Next.js, React)
- Simple APIs
- Admin dashboards
- Internal tools

Rust:
- Performance-critical services
- Data processing pipelines
- Real-time systems
- CLI tools
```

Example: Discord adopted Rust for latency-sensitive backend services (such as their Read States service) while keeping other languages elsewhere in their stack.

### 3. **Gradual Migration**

Don't rewrite everything at once:

**Phase 1:** Identify bottlenecks

```typescript
// Profile your Node.js app
// Find the slow parts
```

**Phase 2:** Extract and rewrite

```rust
// Rewrite just the slow service in Rust
// Keep the rest in Node.js
```

**Phase 3:** Compare

```
Measure performance improvement
If significant: keep Rust version
If not: was Node.js the problem?
```

### 4. **Learn the Ownership System First**

Don't skip the ownership section! It's the foundation of Rust:

1. Ownership rules
2. Borrowing and references
3. Lifetimes
4. Moving vs copying

Everything else in Rust builds on these concepts.

---

## Real-World Example

### Discord's Experience

Discord rewrote their **Read States** service from Go to Rust to eliminate garbage-collection latency:

**Before (Go):**

- Latency spikes every few minutes (caused by Go's garbage collector)
- Tail-latency tied to GC pauses
- Memory pressure under load

**After (Rust):**

- Consistent low latency
- No GC pauses
- Lower memory usage

**Source:** [Discord: Why Discord is switching from Go to Rust](https://discord.com/blog/why-discord-is-switching-from-go-to-rust)

### Figma's Experience

Figma rewrote performance-critical parts of its multiplayer server (originally TypeScript/Node.js) in Rust:

**Why Rust:**

- Needed predictable performance
- No GC pauses (critical for real-time collaboration)
- Multi-threaded performance

**Result:**

- Handles millions of concurrent users
- Consistent low latency
- Lower infrastructure costs

---

## Further Reading

### Official Resources

- [Rust vs Go](https://bitfieldconsulting.com/golang/rust-vs-go)
- [Rust for Node Developers](https://github.com/Mercateo/rust-for-node-developers)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)

### Case Studies

- [Discord: From Go to Rust](https://discord.com/blog/why-discord-is-switching-from-go-to-rust)
- [AWS: Why We Built Firecracker in Rust](https://aws.amazon.com/blogs/opensource/why-aws-loves-rust-and-how-wed-like-to-help/)
- [ZDNet: Microsoft to explore using Rust](https://www.zdnet.com/article/microsoft-to-explore-using-rust/)

### Community Discussions

- [r/rust: Success Stories](https://www.reddit.com/r/rust/search?q=success+story)
- [Rust Users Forum](https://users.rust-lang.org/)

---

## Exercises

### Exercise 1: Benchmark Comparison

Run a simple benchmark comparing Node.js and Rust:

**Node.js:**

```typescript
// fib.ts
function fibonacci(n: number): number {
  if (n <= 1) return n;
  return fibonacci(n - 1) + fibonacci(n - 2);
}

console.time("fib");
console.log(fibonacci(40));
console.timeEnd("fib");
```

**Rust:**

```rust playground
// fib.rs
fn fibonacci(n: u32) -> u32 {
    if n <= 1 {
        return n;
    }
    fibonacci(n - 1) + fibonacci(n - 2)
}

fn main() {
    let start = std::time::Instant::now();
    println!("{}", fibonacci(40));
    println!("Time: {:?}", start.elapsed());
}
```

**Task:** Run both (compile the Rust version with `cargo run --release`) and compare times. The exact ratio depends heavily on your machine, the input, and how well V8's JIT optimizes the JavaScript -- for this naive recursive benchmark expect Rust to be roughly a few times faster, not orders of magnitude. The dramatic wins show up in workloads that are tight loops over typed data, parallelism, or memory layout.

### Exercise 2: Identify Use Cases

For each scenario, decide: Node.js or Rust?

1. Simple blog with 100 daily users
2. Real-time stock trading system
3. CLI tool to process large CSV files
4. Admin dashboard for internal use
5. API serving 10,000 requests/second
6. Startup MVP with 2-week deadline

<details>
<summary>Solutions</summary>

1. **Node.js** - Simple, low traffic
2. **Rust** - Needs predictable low latency
3. **Rust** - CPU-intensive, CLI tools benefit from instant startup
4. **Node.js** - Internal tool, development speed matters
5. **Could be either** - Node.js can handle it, Rust would be more efficient
6. **Node.js** - Speed of development matters most

</details>

---

## Summary

**Why learn Rust as a TS/JS developer:**

1. **Performance** - often substantially faster for CPU-bound and memory-bound tasks (the exact margin depends on the workload)
2. **Safety** - Catch bugs at compile time
3. **Concurrency** - Fearless multi-threading
4. **Deployment** - Single binary, no runtime
5. **Career Growth** - High demand, interesting problems
6. **Better Developer** - Understanding low-level concepts improves all coding

**When to use Rust:**

- Performance-critical backends
- CLI tools
- Systems programming
- WebAssembly
- Microservices with high load

**When to use Node.js:**

- Rapid prototyping
- Simple CRUD apps
- Tight deadlines
- Small teams

**Remember:** Rust is not a replacement for TypeScript/JavaScript. It's an additional tool for specific problems.
