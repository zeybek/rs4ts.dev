---
title: "Prerequisites"
description: "Before you start Rust, let's make sure you have the necessary background knowledge and tools. This will set you up for a smooth learning experience."
---

Before you start Rust, let's make sure you have the necessary background knowledge and tools. This will set you up for a smooth learning experience.

---

## Required Knowledge

### 1. TypeScript/JavaScript Experience (Required)

You should be comfortable with:

#### Basic Programming

```typescript
// Variables and types
let name: string = "Alice";
const age: number = 30;
let isActive: boolean = true;

// Functions
function greet(name: string): string {
  return `Hello, ${name}!`;
}

// Arrow functions
const add = (a: number, b: number): number => a + b;
```

**Why it matters:** These map directly to Rust concepts. If this looks unfamiliar, review JavaScript/TypeScript basics first.

#### Arrays and Objects

```typescript
// Arrays
const numbers: number[] = [1, 2, 3, 4, 5];
const doubled = numbers.map((n) => n * 2);
const filtered = numbers.filter((n) => n > 2);

// Objects and interfaces
interface User {
  id: number;
  name: string;
  email: string;
}

const user: User = {
  id: 1,
  name: "Alice",
  email: "alice@example.com",
};
```

**Why it matters:** Collections work similarly in Rust (Vec, HashMap, etc.)

#### Async/Await

```typescript
// Promises and async/await
async function fetchUser(id: number): Promise<User> {
  const response = await fetch(`/api/users/${id}`);
  const data = await response.json();
  return data;
}

// Error handling
try {
  const user = await fetchUser(1);
  console.log(user);
} catch (error) {
  console.error("Failed:", error);
}
```

**Why it matters:** Rust has async/await too, with similar (but not identical) syntax.

#### Generics

```typescript
// Generic functions
function first<T>(arr: T[]): T | undefined {
  return arr[0];
}

// Generic interfaces
interface Response<T> {
  data: T;
  status: number;
}

const userResponse: Response<User> = {
  data: user,
  status: 200,
};
```

**Why it matters:** Rust's generics are similar but can do more.

#### Modules and Imports

```typescript
// Importing
import { User, fetchUser } from "./api";
import type { Config } from "./config";

// Exporting
export interface Product {
  id: number;
  name: string;
}

export function getProducts(): Product[] {
  // ...
}
```

**Why it matters:** Rust's module system is similar conceptually.

---

### 2. Command Line Basics (Required)

You should know how to:

```bash
# Navigate directories
cd ~/projects
ls
pwd

# Create files and directories
mkdir my-project
touch file.txt

# Run commands
node app.js
npm install
npm start

# View file contents
cat README.md

# Basic git
git clone <url>
git add .
git commit -m "message"
```

**Why it matters:** You'll use terminal commands for Cargo and rustc.

---

### 3. Development Tools (Required)

#### Text Editor/IDE

You need a text editor. Recommended options:

**VS Code** (Most popular)

```bash
# Install VS Code
# Then install rust-analyzer extension
code --install-extension rust-lang.rust-analyzer
```

**Other options:**

- **IntelliJ IDEA** with Rust plugin
- **Vim/Neovim** with rust.vim
- **Emacs** with rust-mode
- **Sublime Text** with Rust Enhanced

**Why it matters:** You'll write lots of Rust code!

#### Terminal/Shell

- macOS/Linux: Built-in terminal
- Windows: PowerShell, WSL2, or Git Bash

---

## Helpful but Not Required

### Nice to Have

#### System Programming Concepts

If you've heard of these, great! If not, don't worry, we'll explain:

- **Stack vs Heap** - We'll teach you
- **Pointers** - Rust handles them safely
- **Memory management** - Rust's ownership system makes it easier
- **Compilation** - You'll learn as you go

#### Other Compiled Languages

Experience with these helps but isn't necessary:

- C, C++, Go, Java, C#, Swift, Kotlin

**If you have:** Great! Some concepts will feel familiar  
**If you don't:** Perfect! You'll learn Rust's way without bad habits

---

## What You DON'T Need

### You Don't Need to Know

<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **Computer Science degree** - Nope!
<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **Low-level programming** - We'll teach you
<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **C or C++** - Actually better if you don't (fewer preconceptions)
<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **Assembly** - Not at all
<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **Operating systems** - Helpful but not required
<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **Math** - Just basic programming logic

### You Don't Need to Be

<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **A "10x developer"** - Everyone starts somewhere
<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **A CS major** - Self-taught? Perfect!
<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **Young** - Age doesn't matter
<span class="inline-icon inline-icon--x" role="img" aria-label="no"><svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span> **A native English speaker** - Code is universal

---

## System Requirements

### Operating System

Rust works on:

- **Linux** (any distro)
- **macOS** (10.7+)
- **Windows** (7+, but 10+ recommended)
- **BSD, etc.**

**Note for Windows users:** Consider using [WSL2](https://docs.microsoft.com/en-us/windows/wsl/install) for a better experience. Or use PowerShell/CMD - both work fine.

### Hardware

**Minimum:**

- 2 GB RAM (4 GB recommended)
- 2 GB disk space
- Any processor from the last 10 years

**Why it matters:** Rust compilation can be slow on old hardware.

### Internet Connection

Required for:

- Installing Rust
- Downloading dependencies (crates)
- Accessing documentation

**Tip:** Once you've downloaded crates, you can work offline.

---

## Required Software

### 1. Node.js (for TypeScript examples)

```bash
# Check if installed
node --version  # Should be 14+
npm --version   # Should be 6+

# If not installed: https://nodejs.org/
```

**Why:** We'll compare TypeScript and Rust code side-by-side.

### 2. TypeScript (optional but recommended)

```bash
# Install globally
npm install -g typescript

# Verify
tsc --version
```

**Why:** To run the TypeScript examples in this guide.

### 3. Git (recommended)

```bash
# Check if installed
git --version

# If not installed: https://git-scm.com/
```

**Why:** For cloning examples and managing your code.

---

## Pre-Learning Checklist

Before starting Section 01, verify:

### Knowledge Checklist

- [ ] I can write TypeScript functions with type annotations
- [ ] I understand async/await and Promises
- [ ] I've used generics (`Array<T>`, `Promise<User>`)
- [ ] I know basic terminal commands (cd, ls, mkdir)
- [ ] I've built at least one TypeScript/JavaScript application

### Tools Checklist

- [ ] I have a text editor/IDE installed
- [ ] I can open and use a terminal
- [ ] I have Node.js installed (node --version works)
- [ ] I have Git installed (optional but recommended)
- [ ] I have at least 2 GB free disk space

### Mindset Checklist

- [ ] I'm ready to invest 20+ hours learning
- [ ] I'm okay with compiler errors (they're helpful!)
- [ ] I'm willing to think differently about programming
- [ ] I'm patient with myself as I learn
- [ ] I'm excited to learn Rust!

---

## Quick Self-Assessment

### Test Your TypeScript Knowledge

Try to answer these without looking them up:

**Question 1:**

```typescript
const numbers = [1, 2, 3, 4, 5];
const result = numbers.map((n) => n * 2).filter((n) => n > 5);
```

What is `result`?

<details>
<summary>Answer</summary>

`[6, 8, 10]` - Numbers doubled, then filtered to only values > 5

</details>

**Question 2:**

```typescript
interface User {
  name: string;
  age?: number;
}
```

What does the `?` mean?

<details>
<summary>Answer</summary>

Optional property - `age` may or may not be present

</details>

**Question 3:**

```typescript
async function fetchData(): Promise<string> {
  const response = await fetch("/api/data");
  return response.text();
}
```

What does this function return?

<details>
<summary>Answer</summary>

A `Promise<string>` - The function is async, so it always returns a Promise

</details>

**Scoring:**

- **3/3:** Perfect! You're ready
- **2/3:** Good, review weak areas
- **1/3 or less:** Review TypeScript basics first

---

## Recommended Pre-Reading (Optional)

If you want to prepare even more:

### For TypeScript Review

- [TypeScript Handbook](https://www.typescriptlang.org/docs/handbook/intro.html)
- [TypeScript Deep Dive](https://basarat.gitbook.io/typescript/)

### For Programming Concepts

- [JavaScript.info](https://javascript.info/) - Modern JavaScript
- [MDN Web Docs](https://developer.mozilla.org/) - Web APIs

### For Command Line

- [Command Line Crash Course](https://developer.mozilla.org/en-US/docs/Learn/Tools_and_testing/Understanding_client-side_tools/Command_line)

---

## Ready to Install Rust?

If you've checked all the boxes above, you're ready!

### What's Next?

1. **You've verified prerequisites**
2. **Next: [Section 01 - Getting Started](/01-getting-started/)**
3. **Install Rust and write your first program!**

---

## Still Not Sure?

### Common Concerns

**"I've only used JavaScript, not TypeScript."**
→ That's okay! You'll learn TypeScript concepts as we compare to Rust. Just note that examples use TypeScript syntax.

**"I've never compiled a program before."**
→ Perfect! We'll walk you through it. Compilation is just turning code into an executable.

**"The terminal scares me."**
→ Don't worry! We'll explain every command. You'll be comfortable with it soon.

**"I don't have much time."**
→ Start with 30 minutes a day. Progress is progress!

**"What if I get stuck?"**
→ Join the [Rust Discord](https://discord.gg/rust-lang) - the community is super helpful!

**"Am I too old/young/etc. to learn Rust?"**
→ Absolutely not! If you can write TypeScript, you can learn Rust.

---

## You're All Set!

You have the knowledge and tools needed. Time to write some Rust!
