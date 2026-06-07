# Contributing Guide

Thank you for your interest in contributing to "Rust for TS/JS Developers"! This guide will help you get started.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How Can I Contribute?](#how-can-i-contribute)
- [Getting Started](#getting-started)
- [Content Guidelines](#content-guidelines)
- [Submitting Changes](#submitting-changes)
- [Style Guide](#style-guide)
- [Review Process](#review-process)

---

## Code of Conduct

### Our Pledge

We are committed to providing a welcoming and inclusive environment for everyone, regardless of:

- Experience level
- Gender identity and expression
- Sexual orientation
- Disability
- Personal appearance
- Body size
- Race
- Ethnicity
- Age
- Religion
- Nationality

### Our Standards

**Positive behavior includes:**

- Using welcoming and inclusive language
- Being respectful of differing viewpoints
- Gracefully accepting constructive criticism
- Focusing on what's best for the community
- Showing empathy towards others

**Unacceptable behavior includes:**

- Harassment of any kind
- Trolling or insulting/derogatory comments
- Personal or political attacks
- Publishing others' private information
- Other conduct inappropriate in a professional setting

---

## How Can I Contribute?

### 1. Reporting Issues

Found an error or have a suggestion?

- **Check existing issues** first to avoid duplicates
- **Use issue templates** when available
- **Be specific**: Include section names, line numbers, and clear descriptions
- **Provide context**: Explain what you expected vs. what you found

### 2. Fixing Typos and Errors

Small fixes are always welcome!

- Fix spelling, grammar, or formatting errors
- Correct factual inaccuracies
- Update outdated information
- No need for an issue - just submit a PR

### 3. Improving Examples

Help make our examples better:

- Add more realistic use cases
- Improve code clarity
- Add helpful comments
- Test and verify examples work
- Suggest better alternatives

### 4. Writing New Content

Want to contribute new sections or topics?

- **Open an issue first** to discuss your idea
- Follow the [Style Guide](#style-guide) below
- Ensure examples are tested and working

### 5. Adding Exercises

Create practice problems:

- Match the difficulty to the section
- Provide clear instructions
- Include solutions separately
- Make them practical and educational

### 6. Reviewing Pull Requests

Help review others' contributions:

- Test code examples
- Check for clarity and accuracy
- Verify formatting and style
- Provide constructive feedback

---

## Getting Started

### Prerequisites

- **Rust**: Install from [rustup.rs](https://rustup.rs/)
- **Node.js**: For testing TypeScript examples
- **Git**: For version control
- **Text Editor**: VS Code, Vim, or your preference

### Setup

1. **Fork the repository**

```bash
# On GitHub, click "Fork" button
```

2. **Clone your fork**

```bash
git clone https://github.com/YOUR_USERNAME/rs4ts.git
cd rs4ts
```

3. **Add upstream remote**

```bash
git remote add upstream https://github.com/ORIGINAL_OWNER/rs4ts.git
```

4. **Create a branch**

```bash
git checkout -b feat/your-feature-name
```

### Testing Your Changes

#### Test Rust Examples

```bash
# Navigate to example directory
cd examples/

# Run the code
rustc example.rs
./example
```

#### Test TypeScript Examples

```bash
# Install TypeScript if needed
npm install -g typescript

# Compile and run
tsc example.ts
node example.js
```

#### Preview Markdown

Use your editor's markdown preview or:

```bash
# Install markdown preview tool
npm install -g markdown-preview

# Preview file
markdown-preview README.md
```

---

## Content Guidelines

### Writing Style

- **Friendly and encouraging**: Learning Rust is challenging
- **Clear and concise**: Avoid unnecessary complexity
- **Practical**: Focus on real-world applications
- **Consistent**: Follow existing patterns

See the [Style Guide](#style-guide) below for formatting and terminology conventions.

### Code Examples

**Every code example must:**

1. **Compile and run** (unless explicitly marked as pseudo-code)
2. **Be realistic**: Use meaningful names and scenarios
3. **Include context**: Show necessary imports and setup
4. **Have comments**: Explain non-obvious parts
5. **Show both languages**: TypeScript/JavaScript AND Rust

**Example structure:**

````markdown
## Topic Name

Brief introduction to the topic.

### TypeScript Example

```typescript
// Clear, real-world TypeScript code
```
````

### Rust Equivalent

```rust
// Equivalent Rust code with explanations
```

### Explanation

Detailed explanation of differences and similarities.

```

### Documentation

- Use clear, descriptive headers
- Add table of contents for long sections
- Link to related sections
- Include "Further Reading" sections
- Add exercises where appropriate

---

## Submitting Changes

### Before Submitting

**Checklist:**

- [ ] All code examples compile and run
- [ ] TypeScript and Rust examples are equivalent
- [ ] Followed the [Style Guide](#style-guide) conventions
- [ ] Added/updated relevant documentation
- [ ] Checked spelling and grammar
- [ ] Tested all links
- [ ] Committed with clear messages

### Commit Messages

Follow this format:

```

<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**

- `feat`: New content
- `fix`: Corrections
- `docs`: Documentation updates
- `style`: Formatting changes
- `refactor`: Content reorganization
- `test`: Example updates

**Example:**

```
feat(ownership): add borrowing examples

Add comprehensive examples showing mutable and immutable borrows
with real-world scenarios.

Closes #42
```

### Creating a Pull Request

1. **Push your changes**

```bash
git push origin feat/your-feature-name
```

2. **Open a Pull Request on GitHub**

- Use a clear, descriptive title
- Reference related issues
- Describe what changed and why
- Add screenshots if relevant
- Request review from maintainers

3. **Respond to feedback**

- Be open to suggestions
- Make requested changes promptly
- Ask questions if unclear
- Update your PR as needed

---

## Style Guide

### File Structure

```
src/content/docs/
└── NN-section/         # e.g. 05-ownership/
    ├── index.md        # Section overview (the chapter's landing page)
    └── NN-topic.md     # Individual topic pages (e.g. 00-moves.md)
```

### Markdown Formatting

```markdown
# Title (H1)

## Major Section (H2)

### Subsection (H3)

**Bold** for emphasis
_Italic_ sparingly
`code` for inline code

> **Note**: Important callouts
```

### Code Blocks

Always specify language:

````markdown
```typescript
// TypeScript code
```

```rust
// Rust code
```
````

### Terminology

Use the Rust-accurate term, especially where it differs from a TS/JS habit:

| Use this                | Not this                                          |
| ----------------------- | ------------------------------------------------- |
| TypeScript / JavaScript | TS/JS (except in headings, where space is tight)  |
| borrow                  | "reference" (when discussing ownership)           |
| trait                   | "interface" (when discussing Rust)                |
| crate                   | "package" (when discussing Rust)                  |
| lifetime                | "scope" (when discussing annotations)             |

Spell out abbreviations on first use — Application Programming Interface (API), Command Line Interface (CLI) — then the short form is fine.

---

## Review Process

### What Happens After Submission?

1. **Initial Review** (1-3 days)

   - Maintainers check for obvious issues
   - Automated tests run

2. **Technical Review** (3-7 days)

   - Code examples are tested
   - Content accuracy is verified
   - Style consistency is checked

3. **Feedback** (ongoing)

   - Reviewers provide constructive feedback
   - You address comments
   - Discussion happens in PR comments

4. **Approval and Merge**
   - Once approved, PR is merged
   - Your contribution goes live!
   - You're added to contributors list 🎉

### Review Criteria

Reviewers look for:

- **Accuracy**: Is the information correct?
- **Clarity**: Is it easy to understand?
- **Completeness**: Are examples comprehensive?
- **Consistency**: Does it match existing content?
- **Quality**: Is code idiomatic and well-tested?

---

## Getting Help

### Questions?

- **General questions**: Open a GitHub Discussion
- **Specific issues**: Create a GitHub Issue
- **Quick questions**: Comment on relevant PR/Issue
- **Private concerns**: Email maintainers directly

### Resources

- [Rust Documentation](https://doc.rust-lang.org/)
- [TypeScript Documentation](https://www.typescriptlang.org/docs/)
- [Markdown Guide](https://www.markdownguide.org/)
- [Git Documentation](https://git-scm.com/doc)

---

## Recognition

### Contributors

All contributors are listed in:

- GitHub contributors page
- CHANGELOG.md for specific contributions
- Special mentions for significant contributions

### Types of Contributions Recognized

- Content writing
- Code examples
- Technical review
- Issue triage
- Documentation improvements
- Translation
- Community support

---

## License

By contributing, you agree that your contributions will be licensed under the same license as the project (see [LICENSE](./LICENSE)).

---

## Thank You! 🙏

Your contributions help TypeScript/JavaScript developers learn Rust more effectively. Every improvement, no matter how small, makes a difference!

---

_Last Updated: 2025-10-25_
