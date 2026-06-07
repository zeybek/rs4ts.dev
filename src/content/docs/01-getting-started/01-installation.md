---
title: "Installing Rust"
description: "Let's get Rust installed on your system. This guide covers all major platforms: macOS, Linux, and Windows."
---

Let's get Rust installed on your system. This guide covers all major platforms: macOS, Linux, and Windows.

---

## Quick Overview

Rust installation is simple using **rustup**, the official Rust toolchain installer. Think of it like nvm for Node.js - it manages Rust versions and tooling.

**Time required:** 10-20 minutes

---

## TypeScript/JavaScript Comparison

**Installing Node.js:**

```bash
# Download installer from nodejs.org
# Or use nvm
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
nvm install --lts   # Installs current LTS (e.g. Node 22)
nvm use --lts
```

**Installing Rust:**

```bash
# Use rustup (official installer)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

**Key similarities:**

- Both use version managers (nvm/rustup)
- Both install command-line tools
- Both update your PATH

**Key differences:**

- The rustup download is larger (~200 MB) because it bundles the toolchain, but it's a single step
- Rust includes everything (compiler, package manager, formatter, linter)
- No separate npm install - Cargo is built-in!

---

## Installation by Platform

### macOS

#### Prerequisites

First, install Xcode Command Line Tools (if not already installed):

```bash
xcode-select --install
```

This provides the C compiler needed for Rust's linker.

#### Install Rust

```bash
# Download and run rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**What this does:**

1. Downloads rustup
2. Installs the latest stable Rust
3. Installs cargo (package manager)
4. Adds Rust to your PATH
5. Installs rustfmt (formatter) and cargo-clippy (linter)

**Interactive options:**

```
1) Proceed with installation (default)
2) Customize installation
3) Cancel installation
```

Just press Enter for option 1 (default).

#### Configure Your Shell

```bash
# Add cargo to your PATH
source $HOME/.cargo/env

# Or restart your terminal
```

**For zsh (default on macOS):**
rustup automatically adds this line to `~/.zshrc`:

```bash
. "$HOME/.cargo/env"
```

**For bash:**
rustup adds this to `~/.bash_profile`:

```bash
. "$HOME/.cargo/env"
```

#### Verify Installation

```bash
# Check Rust compiler
rustc --version
# Output: rustc 1.96.0 (or newer)

# Check cargo
cargo --version
# Output: cargo 1.96.0 (or newer)

# Check rustup
rustup --version
# Output: rustup 1.29.0 (or newer)
```

---

### Linux

#### Prerequisites

Install build essentials (C compiler and linker):

**Debian/Ubuntu:**

```bash
sudo apt update
sudo apt install build-essential
```

**Fedora:**

```bash
sudo dnf groupinstall "Development Tools"
```

**Arch:**

```bash
sudo pacman -S base-devel
```

#### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the same steps as macOS.

#### Configure Your Shell

```bash
source $HOME/.cargo/env
```

Add to your shell rc file (`~/.bashrc`, `~/.zshrc`, etc.):

```bash
. "$HOME/.cargo/env"
```

#### Verify Installation

```bash
rustc --version
cargo --version
rustup --version
```

---

### Windows

#### Option 1: Native Windows (Recommended)

**Prerequisites:**

Install Visual Studio C++ Build Tools:

1. Download from: https://visualstudio.microsoft.com/visual-cpp-build-tools/
2. Run the installer
3. Select "C++ build tools" workload
4. Install (takes ~5-10 minutes)

**Install Rust:**

1. Download rustup-init.exe from: https://rustup.rs/
2. Run the installer
3. Follow prompts (default options work fine)
4. Restart your terminal

**Verify:**

```powershell
rustc --version
cargo --version
```

#### Option 2: WSL2 (Windows Subsystem for Linux)

If you're using WSL2, follow the Linux instructions above.

**Why WSL2?**

- Better compatibility with Unix-based tools
- Faster compilation in some cases
- More similar to Linux production environments

**How to install WSL2:**

```powershell
# In PowerShell (Administrator)
wsl --install
```

Then follow Linux instructions inside WSL2.

---

## Detailed Explanation

### What Gets Installed?

When you run rustup, you get:

**1. rustc** - The Rust compiler

```bash
rustc main.rs # Compiles to executable
```

**2. cargo** - Package manager and build tool

```bash
cargo new my_project  # Create new project
cargo build           # Build project
cargo run             # Build and run
cargo test            # Run tests
```

**3. rustup** - Toolchain manager

```bash
rustup update         # Update Rust
rustup default stable # Set default version
rustup doc            # Open docs
```

**4. rustfmt** - Code formatter

```bash
rustfmt main.rs       # Format code
cargo fmt             # Format whole project
```

**5. clippy** - Linter

```bash
cargo clippy          # Run linter
```

**6. rust-docs** - Offline documentation

```bash
rustup doc            # Open in browser
rustup doc --std      # Standard library docs
```

**All included!** No separate installations needed (unlike npm, prettier, eslint, etc. in Node.js).

### Directory Structure

Rust installs to:

**macOS/Linux:**

```
~/.cargo/          # Cargo home
├── bin/           # Executables (cargo, rustc, rustfmt, etc.)
├── registry/      # Downloaded crate metadata
└── git/           # Git dependencies

~/.rustup/         # Rustup home
├── toolchains/    # Installed Rust versions
└── settings.toml  # Configuration
```

**Windows:**

```
C:\Users\<you>\.cargo\     # Cargo home
C:\Users\<you>\.rustup\    # Rustup home
```

### Environment Variables

Rustup adds these to your PATH:

```bash
# macOS/Linux
export PATH="$HOME/.cargo/bin:$PATH"

# Windows (added by installer)
%USERPROFILE%\.cargo\bin
```

**Compare to Node.js:**

```bash
# Node.js (with nvm)
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"
```

Similar concept, different implementation.

---

## Key Differences from Node.js

### 1. Single Installer

**Node.js ecosystem:**

```bash
# Multiple separate installs
npm install -g typescript
npm install -g ts-node
npm install -g prettier
npm install -g eslint
```

**Rust:**

```bash
# All in one!
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# You now have: compiler, package manager, formatter, linter, docs
```

### 2. One-Step Installation

**Node.js:**

- Download size: ~50-100 MB
- Installation time: 2-5 minutes
- node_modules can be gigabytes per project

**Rust:**

- Download size: ~200 MB (includes the full toolchain, so the download is larger)
- Installation time: 2-5 minutes
- A single command sets up the compiler, package manager, formatter, and linter; project dependencies are compiled and cached under `~/.cargo`

### 3. Version Management Built-In

**Node.js:**

```bash
# Need separate tool (nvm, n, fnm)
nvm install --lts
nvm use --lts
nvm list
```

**Rust:**

```bash
# Built into rustup
rustup install stable
rustup default stable
rustup toolchain list
```

### 4. Offline Documentation

**Node.js:**

```bash
# Online only (mostly)
# Browse to nodejs.org or devdocs.io
```

**Rust:**

```bash
# Full docs offline!
rustup doc         # Opens local browser
rustup doc --std   # Standard library
rustup doc --book  # The Rust Book
```

---

## Common Pitfalls

### Pitfall 1: "command not found" After Installation

**Problem:**

```bash
rustc --version
# zsh: command not found: rustc
```

**Solution:**

```bash
# Option 1: Restart your terminal

# Option 2: Source the cargo env
source $HOME/.cargo/env

# Option 3: Check PATH
echo $PATH | grep cargo
# Should see: /Users/you/.cargo/bin
```

**Why:** Your shell needs to reload its configuration to see the new PATH.

### Pitfall 2: "linking with `cc` failed"

**Problem:**

```bash
cargo build
# error: linking with `cc` failed
```

**Solution:**

**macOS:**

```bash
xcode-select --install
```

**Linux:**

```bash
sudo apt install build-essential  # Debian/Ubuntu
```

**Windows:**
Install Visual Studio C++ Build Tools

**Why:** Rust needs a C compiler for linking. The Rust compiler produces object files, but needs a linker to create the final executable.

### Pitfall 3: Old Version of Rust

**Problem:**
Some examples don't compile because you're using an old Rust version.

**Solution:**

```bash
# Update to latest stable
rustup update

# Check version
rustc --version
```

**Best practice:** Run `rustup update` periodically; a new stable release ships every 6 weeks.

### Pitfall 4: Permission Errors on Linux

**Problem:**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Permission denied
```

**Solution:**
Don't use sudo! Install in your home directory:

```bash
# Correct (no sudo)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Then install system packages with sudo if needed
sudo apt install build-essential
```

---

## Best Practices

### 1. Keep Rust Updated

```bash
# Update Rust (a new stable lands every 6 weeks)
rustup update

# Check for updates
rustup check
```

New versions come every 6 weeks with improvements and bug fixes.

### 2. Install rust-analyzer (VS Code)

```bash
# In VS Code, install extension:
# rust-lang.rust-analyzer

# Or from terminal:
code --install-extension rust-lang.rust-analyzer
```

**What it provides:**

- Code completion
- Inline errors
- Go to definition
- Refactoring tools
- Inline type hints

**Compare to TypeScript:**

```typescript
// TypeScript has built-in language server
// Rust needs rust-analyzer extension
```

### 3. Configure Your Editor

**VS Code settings.json:**

```json
{
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
  },
  "rust-analyzer.checkOnSave": true,
  "rust-analyzer.check.command": "clippy"
}
```

This enables:

- Format on save (like Prettier)
- Clippy linting on save (like ESLint)

### 4. Set Up Shell Completions

```bash
# For bash
rustup completions bash >> ~/.bash_completion

# For zsh
rustup completions zsh > ~/.zfunc/_rustup

# For fish
rustup completions fish > ~/.config/fish/completions/rustup.fish
```

Now you can tab-complete Rust commands!

---

## Real-World Example: Complete Setup

Here's a complete setup script for a new machine:

**macOS/Linux:**

```bash
#!/bin/bash

# 1. Install prerequisites
# macOS
xcode-select --install

# Linux (Ubuntu/Debian)
# sudo apt update && sudo apt install build-essential

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# 3. Source cargo env
source $HOME/.cargo/env

# 4. Verify installation
rustc --version
cargo --version

# 5. Install rust-analyzer (if using VS Code)
code --install-extension rust-lang.rust-analyzer

# 6. Configure VS Code
mkdir -p ~/.config/Code/User
cat > ~/.config/Code/User/settings.json << EOF
{
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
  }
}
EOF

echo "Rust installed successfully!"
```

**Compare to Node.js setup:**

```bash
#!/bin/bash

# 1. Install nvm
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash

# 2. Install Node.js (current LTS)
nvm install --lts
nvm use --lts

# 3. Install global tools
npm install -g typescript ts-node prettier eslint

# 4. Verify
node --version
npm --version

echo "Node.js installed successfully!"
```

---

## Further Reading

### Official Documentation

- [Rustup Book](https://rust-lang.github.io/rustup/) - Complete rustup guide
- [Installation Page](https://www.rust-lang.org/tools/install) - Official installation guide
- [Rust Forge](https://forge.rust-lang.org/) - Infrastructure documentation

### Editor Setup

- [rust-analyzer Manual](https://rust-analyzer.github.io/manual.html) - VS Code extension docs
- [IntelliJ Rust](https://plugins.jetbrains.com/plugin/8182-rust) - For IntelliJ/CLion
- [Vim Rust](https://github.com/rust-lang/rust.vim) - For Vim/Neovim

### Platform-Specific

- [Windows Installation Guide](https://doc.rust-lang.org/book/ch01-01-installation.html#installing-rustup-on-windows)
- [WSL2 Setup](https://docs.microsoft.com/en-us/windows/wsl/install)

---

## Exercises

### Exercise 1: Verify Your Installation

Run these commands and verify the output:

```bash
# Should print version (1.96.0 or newer)
rustc --version

# Should print version
cargo --version

# Should print version
rustup --version

# Should list one toolchain (stable)
rustup toolchain list

# Should open documentation in browser
rustup doc

# Should print help text
cargo --help
```

### Exercise 2: Update Rust

```bash
# Check for updates
rustup check

# Update all components
rustup update

# Verify new version
rustc --version
```

### Exercise 3: Install Additional Components

```bash
# Install nightly (for experimental features)
rustup toolchain install nightly

# Install source code (for rust-analyzer)
rustup component add rust-src

# List installed components
rustup component list --installed
```

### Exercise 4: Configure Your Editor

1. Install rust-analyzer for your editor
2. Create a test file: `test.rs`
3. Type: `fn main() {`
4. Verify that auto-completion works
5. Add a syntax error and verify that inline errors appear

---

## Summary

**What you've learned:**

- How to install Rust using rustup
- What tools come with Rust (rustc, cargo, rustfmt, clippy)
- How to verify your installation
- Common pitfalls and solutions
- Best practices for editor setup

**What you have now:**

- Rust compiler (rustc)
- Package manager (cargo)
- Formatter (rustfmt)
- Linter (clippy)
- Documentation (rustup doc)
- Version manager (rustup)

**Compare to Node.js:**

- Node.js requires separate tools (npm, prettier, eslint)
- Rust includes everything in one install
- Both use version managers (nvm/rustup)
- Rust has offline documentation built-in

**You're ready to write Rust!**
