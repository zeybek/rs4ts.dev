---
title: "Input Validation and Sanitization"
description: "Turn untrusted input into trusted values in Rust with parse-don't-validate newtypes, serde, and the validator crate, versus relying on Zod in TypeScript."
---

Every byte that crosses a trust boundary — an HTTP body, a query string, a CLI flag, a file — is hostile until proven otherwise. This page is about turning that untrusted input into trustworthy values, the Rust way: **parse, don't validate**, push the rules into the type system, and reach for the `validator` crate when you want declarative field-level checks.

> **Note:** Validation is the foundation of the rest of this section. Validated input still needs safe handling downstream: see [SQL injection prevention](/27-security/01-sql-injection/) for the database boundary and [XSS and CSRF protection](/27-security/02-xss-csrf/) for the HTML/browser boundary. Validation is necessary but never sufficient.

---

## Quick Overview

Input validation answers one question: *can I trust this data enough to act on it?* In TypeScript you typically reach for a runtime schema library (Zod, Yup, io-ts) because TypeScript types are **erased at runtime** and cannot check anything about real values. Rust keeps its types at runtime through monomorphization, so the strongest technique is to **encode the rules in a type**: once you hold a `Email` or a `Quantity`, it is *guaranteed* valid, and the compiler stops you from forgetting to check. This is the "parse, don't validate" philosophy, and it eliminates a whole class of "I forgot to check that field" bugs that no amount of code review reliably catches.

---

## TypeScript/JavaScript Example

A realistic signup endpoint. TypeScript's compile-time types tell you *nothing* about the JSON that actually arrives, so you bolt on a runtime schema with [Zod](https://zod.dev) (v4):

```typescript
// npm install zod  (zod v4)
import { z } from "zod";

const SignupSchema = z.object({
  username: z.string().min(3).max(20),
  email: z.email(),
  age: z.number().int().min(18).max(120),
});

// The *inferred* type — but it only exists at compile time.
type Signup = z.infer<typeof SignupSchema>;

function handleSignup(body: unknown) {
  const result = SignupSchema.safeParse(body);
  if (!result.success) {
    for (const issue of result.error.issues) {
      console.log(issue.path.join("."), "-", issue.message);
    }
    return { status: 422 };
  }
  // result.data is now typed as Signup AND validated.
  const user: Signup = result.data;
  return { status: 201, user };
}

handleSignup({ username: "ab", email: "nope", age: 12 });
```

Running this against Node v22 with zod 4.4.3 prints:

```text
username - Too small: expected string to have >=3 characters
email - Invalid email address
age - Too small: expected number to be >=18
```

**Key points:**

- `z.infer` derives a *compile-time* type from the schema, but the schema object is what does the real work at runtime.
- Nothing in the type system forces you to call `safeParse`. You can write `body as Signup` and skip validation entirely: the cast compiles, and the bug ships.
- The validated `result.data` is a plain object; it is structurally identical to an unvalidated one, so once it leaves this function the "it's been validated" guarantee lives only in your head.

---

## Rust Equivalent

Rust gives you two complementary tools. First, the type-driven approach: **make an invalid value unrepresentable** by parsing into a dedicated type whose constructor is the only way in:

```rust
use std::fmt;

/// A validated username. If you hold one, it is guaranteed to satisfy the rules.
#[derive(Debug, Clone)]
struct Username(String);

#[derive(Debug)]
enum UsernameError {
    TooShort,
    TooLong,
    InvalidChar(char),
}

impl fmt::Display for UsernameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsernameError::TooShort => write!(f, "username must be at least 3 characters"),
            UsernameError::TooLong => write!(f, "username must be at most 20 characters"),
            UsernameError::InvalidChar(c) => write!(f, "invalid character: {c:?}"),
        }
    }
}
impl std::error::Error for UsernameError {}

impl Username {
    /// The *only* way to build a `Username`. Validation happens here, once.
    fn parse(raw: &str) -> Result<Self, UsernameError> {
        let trimmed = raw.trim();
        let len = trimmed.chars().count();
        if len < 3 {
            return Err(UsernameError::TooShort);
        }
        if len > 20 {
            return Err(UsernameError::TooLong);
        }
        if let Some(bad) = trimmed
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '_'))
        {
            return Err(UsernameError::InvalidChar(bad));
        }
        Ok(Username(trimmed.to_string()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

fn main() {
    for raw in ["  alice_99  ", "hi", "no spaces!"] {
        match Username::parse(raw) {
            Ok(u) => println!("ok: {:?} -> {:?}", raw, u.as_str()),
            Err(e) => println!("rejected {:?}: {e}", raw),
        }
    }
}
```

Real output:

```text
ok: "  alice_99  " -> "alice_99"
rejected "hi": username must be at least 3 characters
rejected "no spaces!": invalid character: ' '
```

Second, the declarative approach: the [`validator`](https://docs.rs/validator) crate, which feels closest to Zod. Add the dependency:

```bash
cargo add validator --features derive
cargo add serde --features derive
cargo add serde_json
```

```rust
use serde::Deserialize;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
struct SignupForm {
    #[validate(length(min = 3, max = 20, message = "username must be 3-20 chars"))]
    username: String,

    #[validate(email(message = "must be a valid email address"))]
    email: String,

    #[validate(length(min = 8, message = "password must be at least 8 chars"))]
    password: String,

    #[validate(range(min = 18, max = 120, message = "age must be between 18 and 120"))]
    age: u8,
}

fn main() {
    let raw = r#"
    {
        "username": "ab",
        "email": "not-an-email",
        "password": "short",
        "age": 12
    }"#;

    // serde_json checks the *shape* (types, presence); validator checks the *values*.
    let form: SignupForm = serde_json::from_str(raw).expect("valid JSON shape");

    match form.validate() {
        Ok(()) => println!("all fields valid"),
        Err(errors) => {
            for (field, errs) in errors.field_errors() {
                for e in errs {
                    let msg = e
                        .message
                        .as_ref()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| e.code.to_string());
                    println!("{field}: {msg}");
                }
            }
        }
    }
}
```

Real output (field order is not deterministic — `field_errors` returns a map):

```text
password: password must be at least 8 chars
age: age must be between 18 and 120
username: username must be 3-20 chars
email: must be a valid email address
```

**Key points:**

- The `Username` newtype makes "unvalidated username" a different *type* from "validated username". You cannot accidentally pass the wrong one; the compiler rejects it.
- The `validator` derive mirrors Zod's declarative feel: attributes describe constraints, `.validate()` runs them.
- `serde` deserialization already enforced the **shape** (an `age` of `"twelve"` fails before `validate()` ever runs); `validator` enforces the **business rules** on top.

---

## Detailed Explanation

### "Parse, don't validate"

The phrase comes from Alexis King's well-known essay, and Rust is the language where it shines. The idea: a *validator* is a function `(input) -> bool` that throws away its findings: after `isValidEmail(s)` returns `true`, you still hold a plain `string`, and the next function has no idea it was checked. A *parser* is a function `(input) -> Result<Parsed, Error>` that returns a **new type** carrying the proof. Once you hold an `Email`, the fact that it is well-formed is encoded in the type, not in a comment or a convention.

In the `Username::parse` example, look at what the type system now guarantees:

1. There is no public way to build a `Username` except `parse`. (We will tighten this with module privacy in the next subsection.)
2. Every function that accepts `&Username` can assume the rules hold: no re-checking, no defensive `if`.
3. If you refactor and add a new call site, you *cannot* forget to validate, because you literally cannot produce the value without going through `parse`.

Contrast this with the Zod version: `result.data` and an unchecked `body as Signup` have the **exact same type**. The "validated" property is invisible to the compiler, so the only thing stopping an unvalidated object from flowing downstream is your discipline.

### Type-driven validation with newtypes

A **newtype** is a single-field tuple struct (`struct Email(String)`) that wraps an existing type to give it new meaning and new guarantees. (Sections [Basic Types](/02-basics/01-types/) and the data-structures section cover the mechanics; here we use them for safety.) The key move is **field privacy**: keep the inner field private so outside code must use your constructor.

You can also parse *directly at the deserialization boundary* by implementing `Deserialize` for the newtype. Now invalid JSON never even produces a value of your type; the failure happens inside `serde_json::from_str`:

```rust
use serde::{Deserialize, Deserializer};

/// A validated email address. Holding one proves it is well-formed.
#[derive(Debug, Clone)]
struct Email(String);

impl Email {
    fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        // Deliberately simple structural check for the example.
        let (local, domain) = raw
            .split_once('@')
            .ok_or_else(|| "email must contain '@'".to_string())?;
        if local.is_empty() || domain.is_empty() || !domain.contains('.') {
            return Err("email is not well-formed".to_string());
        }
        Ok(Email(raw.to_lowercase()))
    }
    fn as_str(&self) -> &str {
        &self.0
    }
}

// Deserialize straight into the parsed type: invalid input is rejected by serde itself.
impl<'de> Deserialize<'de> for Email {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Email::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
struct Contact {
    name: String,
    email: Email,
}

fn main() {
    let good = r#"{ "name": "Bob", "email": "BOB@Example.COM " }"#;
    let bad = r#"{ "name": "Bob", "email": "not-an-email" }"#;

    match serde_json::from_str::<Contact>(good) {
        Ok(c) => println!("parsed: {} <{}>", c.name, c.email.as_str()),
        Err(e) => println!("error: {e}"),
    }
    match serde_json::from_str::<Contact>(bad) {
        Ok(c) => println!("parsed: {} <{}>", c.name, c.email.as_str()),
        Err(e) => println!("error: {e}"),
    }
}
```

Real output:

```text
parsed: Bob <bob@example.com>
error: email must contain '@' at line 1 column 42
```

Notice that the constructor also **normalized** the value (trimmed whitespace, lowercased the address). Parsing is the natural place to canonicalize input, so every `Email` you hold downstream is both valid and in one consistent form, which is exactly what you want before storing it or comparing it.

### Declarative validation with the `validator` crate

The newtype approach is the strongest guarantee, but it is verbose when a request has a dozen fields, each with simple length/range/format rules. That is where `validator` earns its place. It generates a `validate(&self) -> Result<(), ValidationErrors>` method from attributes. Built-in validators include `length`, `range`, `email`, `url`, `contains`, `does_not_contain`, `must_match`, `regex`, and `custom`.

Two rules to internalize:

- `serde` runs **first** and enforces structure: required fields, correct JSON types, no unexpected nulls. If the JSON shape is wrong, `from_str` fails and `validate()` never runs.
- `validator` runs **second** (you must call it explicitly) and enforces value-level business rules.

For rules the built-ins do not cover, write a `custom` function. It receives the field value and returns `Result<(), ValidationError>`. You can also enforce cross-field rules like "passwords must match":

```rust
use serde::Deserialize;
use validator::{Validate, ValidationError};

fn validate_no_spaces(value: &str) -> Result<(), ValidationError> {
    if value.contains(' ') {
        return Err(ValidationError::new("contains_space"));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Validate)]
struct CreateUser {
    #[validate(length(min = 3, max = 20), custom(function = "validate_no_spaces"))]
    username: String,
    #[validate(email)]
    email: String,
    #[validate(must_match(other = "password"))]
    password_confirm: String,
    #[validate(length(min = 8))]
    password: String,
}

fn main() {
    let body = CreateUser {
        username: "bad name".into(),
        email: "x@y.com".into(),
        password: "longenough".into(),
        password_confirm: "different".into(),
    };
    match body.validate() {
        Ok(()) => println!("valid"),
        Err(e) => println!("{e}"), // ValidationErrors has a flat Display impl
    }
}
```

Real output:

```text
username: Validation error: contains_space [{"value": String("bad name")}]
password_confirm: Validation error: must_match [{"value": String("different"), "other": String("longenough")}]
```

The raw `Display` output is developer-facing and leaks internal codes; in a real API you would map `ValidationErrors` into a clean response shape, as shown in the Real-World Example below.

> **Tip:** `validator` and newtypes compose. Use `validator` for the broad sweep of simple field rules on a request DTO, then convert that DTO into a domain struct made of newtypes (`Email`, `Username`, `Quantity`) so the *rest of your codebase* works only with already-proven values. The DTO is the airlock; the domain types are the clean room.

---

## Key Differences

| Concern | TypeScript (Zod/Yup) | Rust |
| --- | --- | --- |
| When do types exist? | Compile time only — **erased** at runtime | Runtime: types are monomorphized and real |
| What enforces validation? | A library call you must remember to make | The library call *or* the type system itself (newtypes) |
| Can you skip validation? | Yes: `body as T` casts compile silently | With newtypes, no: there is no other constructor |
| Shape vs. value checks | One library (Zod) does both | `serde` checks shape; `validator`/newtypes check values |
| "Validated" is visible to the compiler? | No: `data` and unchecked object share a type | Yes: `Email` and `String` are different types |
| Normalization (trim/lowercase) | Manual, easy to forget | Naturally lives in the parsing constructor |
| Default safety posture | Opt-in; forgetting = silent bug | Opt-in for `validator`; *enforced* for newtypes |

The deep difference: in TypeScript, validation is a **runtime gate you bolt on** because the type system already gave up at runtime. In Rust, validation can be **a property the type system carries forever**, so "is this validated?" becomes a compile-time question the compiler answers for you.

> **Warning:** "Unlike TypeScript," a Rust newtype is a genuine type boundary, not a type alias. `type UserId = string` in TypeScript is interchangeable with any other `string`; `struct UserId(String)` in Rust is not interchangeable with `String` or with `struct OrderId(String)`. The branded-type pattern in TypeScript tries to emulate this with phantom intersection types, but it is still erased and bypassable.

---

## Common Pitfalls

### Pitfall 1: Believing a TypeScript type validates anything at runtime

A TS dev's instinct is "the handler is typed `(body: Signup)`, so `body` is a valid `Signup`." It is not. The type was erased; `body` is whatever JSON arrived. The Rust analog of that mistake is trusting a `#[derive(Deserialize)]` struct without calling `validate()`; `serde` checked the shape but not the rules. Always run value-level validation, or deserialize into newtypes that validate themselves.

### Pitfall 2: Making the newtype field public (or forgetting module privacy)

The whole point of a validating newtype is that its constructor is the only door in. If you write `struct Email(pub String)`, or define the type and its caller in the same module, callers can build an invalid one directly. Keep the field private and put the type in its own module. The compiler then enforces the rule. Trying to skip the constructor produces a real error:

```rust
mod domain {
    pub struct Email(String); // field is private to the module

    impl Email {
        pub fn parse(raw: &str) -> Result<Self, String> {
            if raw.contains('@') {
                Ok(Email(raw.to_string()))
            } else {
                Err("bad email".into())
            }
        }
    }
}

fn main() {
    // does not compile (error[E0603]): tries to bypass the validating constructor
    let e = domain::Email("not-an-email".to_string());
    let _ = e;
}
```

The exact compiler error:

```text
error[E0603]: tuple struct constructor `Email` is private
  --> src/main.rs:17:21
   |
 2 |     pub struct Email(String); // field is private to the module
   |                      ------ a constructor is private if any of the fields is private
...
17 |     let e = domain::Email("not-an-email".to_string());
   |                     ^^^^^ private tuple struct constructor
   |
note: the tuple struct constructor `Email` is defined here
  --> src/main.rs:2:5
   |
 2 |     pub struct Email(String);
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^
```

This error is a *feature*: the type system is refusing to let you create an unvalidated value. That is the safety you are paying for.

### Pitfall 3: Validating but not normalizing

Two inputs can both be "valid" yet not equal: `"Bob@Example.com "` and `"bob@example.com"`. If you validate without canonicalizing, you get duplicate accounts, case-sensitive lookups that miss, and inconsistent stored data. Do trimming, case-folding, and Unicode normalization **inside the parsing constructor** so every value you hold is already in canonical form.

### Pitfall 4: Trusting client-side validation

A TS frontend often validates a form before submitting. That is a UX nicety, never a security control; an attacker calls your API directly with `curl`. Server-side validation is mandatory and independent. This is the same posture you bring to Rust: validate at the edge of *every* service, on every request, regardless of what the caller claims to have done.

### Pitfall 5: Using overly clever regexes for structured formats

Reaching for a giant regex to validate emails, URLs, or dates is a classic trap (and a denial-of-service risk via catastrophic backtracking). Prefer the `email`/`url` validators, or purpose-built parsers (the `url` crate, `time`/`chrono` for dates). If you do use the `regex` crate, note it is linear-time by design and cannot catastrophically backtrack — but it still should not be your tool for parsing structured data when a real parser exists.

---

## Best Practices

- **Validate at the boundary, once.** Parse untrusted input into trusted types as early as possible — ideally in the deserialization/extractor layer — and let the rest of the code work only with proven values.
- **Prefer parsing over validating.** When a rule matters to your domain (an email, a non-empty cart, a percentage 0–100), make a newtype. Make illegal states unrepresentable instead of re-checking them everywhere.
- **Keep newtype fields private** and expose a `parse`/`try_from`/`new` constructor. Implement `Deserialize` (or `TryFrom`) so the type validates itself at the edge.
- **Let `serde` do shape, `validator` do values.** Do not hand-roll presence/type checks that `serde` already gives you for free.
- **Normalize during parsing.** Trim, lowercase, and Unicode-normalize so stored and compared values are canonical.
- **Bound everything.** Cap string lengths, collection sizes, and numeric ranges. Unbounded input is a memory-exhaustion vector; pair this with body-size limits at the web layer (see [Production](/28-production/)).
- **Return structured, non-leaky errors.** Map validation failures to a clean `422`-style response with field-keyed messages; never echo internal error codes or the raw input back verbatim.
- **Validation is not the end of safety.** A validated string is still untrusted SQL and untrusted HTML. Always parameterize queries ([SQL injection prevention](/27-security/01-sql-injection/)) and encode output ([XSS and CSRF protection](/27-security/02-xss-csrf/)).

---

## Real-World Example

A production-flavored axum endpoint with a reusable `ValidatedJson<T>` extractor. It deserializes the body (shape check), runs `validate()` (value check), and short-circuits with a clean JSON `422` on failure, so every handler that uses it receives an already-valid payload. The current stable toolchain is Rust 1.96.0 on the 2024 edition, and `cargo new` selects it automatically.

```bash
cargo new signup-api && cd signup-api
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json
cargo add validator --features derive
```

```rust
use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use validator::Validate;

/// A reusable extractor: deserialize JSON, then run validation.
/// On any failure it short-circuits before the handler body runs.
struct ValidatedJson<T>(T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: for<'de> Deserialize<'de> + Validate,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // Step 1: shape check (serde). A bad JSON shape -> 400.
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|err: JsonRejection| {
                (StatusCode::BAD_REQUEST, Json(json!({ "error": err.body_text() })))
                    .into_response()
            })?;

        // Step 2: value check (validator). A bad value -> 422.
        value.validate().map_err(|errs| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "validation failed", "fields": errs.to_string() })),
            )
                .into_response()
        })?;

        Ok(ValidatedJson(value))
    }
}

#[derive(Debug, Deserialize, Validate)]
struct CreateUser {
    #[validate(length(min = 3, max = 20))]
    username: String,
    #[validate(email)]
    email: String,
    #[validate(length(min = 8))]
    password: String,
}

// By the time this runs, `payload` is structurally valid — no defensive checks needed.
async fn create_user(ValidatedJson(payload): ValidatedJson<CreateUser>) -> impl IntoResponse {
    (
        StatusCode::CREATED,
        Json(json!({ "username": payload.username, "email": payload.email })),
    )
}

#[tokio::main]
async fn main() {
    let app: Router = Router::new().route("/users", post(create_user));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

This compiles cleanly with axum 0.8 and passes `cargo clippy` with no warnings. The pattern is the Rust equivalent of a validating middleware in Express, but with an important upgrade: the handler's *type signature* (`ValidatedJson<CreateUser>`) makes the validation contract part of the function, so a handler simply cannot run on unvalidated input. Note the current axum idioms in use: `axum::serve(listener, app)` (not the old `Server::bind().serve()`), and `{id}`-style route placeholders if you add path parameters.

> **Note:** For an even stronger guarantee, have `CreateUser` deserialize its fields into newtypes (`Username`, `Email`) so validity is carried by the type all the way into your service layer, and `validator` becomes a convenience rather than the only line of defense.

---

## Further Reading

- [`validator` crate docs](https://docs.rs/validator) — the full attribute reference (`length`, `range`, `email`, `url`, `regex`, `custom`, `nested`).
- [serde documentation](https://serde.rs) — how deserialization enforces structure before any value-level checks.
- ["Parse, don't validate" by Alexis King](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/) — the essay behind the philosophy.
- [Rust API Guidelines: validation and constructors](https://rust-lang.github.io/api-guidelines/) — idiomatic constructor naming (`new`, `try_from`, `parse`).
- Related sections in this guide:
  - [SQL Injection Prevention](/27-security/01-sql-injection/) — never trust validated input as SQL; parameterize.
  - [XSS and CSRF Protection](/27-security/02-xss-csrf/) — never trust validated input as HTML; encode on output.
  - [Secrets Management](/27-security/07-secrets-management/) — handling sensitive validated values (passwords, tokens).
  - [Security](/27-security/) — the rest of the security section.
  - [Basic Types](/02-basics/01-types/) — the type-system mechanics newtypes build on.
  - [Result and Option](/08-error-handling/00-result-option/) — `Result` is how parsers report failure.
  - [Production](/28-production/) — body-size limits, rate limiting, and other edge hardening.

---

## Exercises

### Exercise 1: A self-normalizing `Slug` newtype

**Difficulty:** Beginner

**Objective:** Practice the parse-don't-validate pattern with normalization.

**Instructions:** Create a `Slug` newtype that wraps a `String`. Its `parse(&str) -> Result<Slug, String>` constructor should: trim whitespace, lowercase the input, reject an empty result, and reject any character that is not `a-z`, `0-9`, or `-`. Keep the inner field private and expose `as_str(&self) -> &str`. Show that `"  Hello-World  "` parses to `"hello-world"` and that `"bad slug!"` is rejected.

<details>
<summary>Solution</summary>

```rust
#[derive(Debug, Clone)]
struct Slug(String);

impl Slug {
    fn parse(raw: &str) -> Result<Self, String> {
        let normalized = raw.trim().to_lowercase();
        if normalized.is_empty() {
            return Err("slug must not be empty".to_string());
        }
        if let Some(bad) = normalized
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
        {
            return Err(format!("invalid character in slug: {bad:?}"));
        }
        Ok(Slug(normalized))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

fn main() {
    match Slug::parse("  Hello-World  ") {
        Ok(s) => println!("ok: {:?}", s.as_str()),
        Err(e) => println!("rejected: {e}"),
    }
    match Slug::parse("bad slug!") {
        Ok(s) => println!("ok: {:?}", s.as_str()),
        Err(e) => println!("rejected: {e}"),
    }
}
```

Output:

```text
ok: "hello-world"
rejected: invalid character in slug: ' '
```

</details>

### Exercise 2: A `validator`-based DTO with a custom rule

**Difficulty:** Intermediate

**Objective:** Combine built-in and custom `validator` rules on a deserialized struct.

**Instructions:** Define a `ProductForm` with fields `name: String` (length 1–100), `price_cents: u32` (range 1–1_000_000), and `sku: String`. Write a `custom` validator that requires `sku` to be exactly 8 ASCII-uppercase-alphanumeric characters. Deserialize a JSON body that violates the SKU rule and print the field errors. Add the dependencies with `cargo add validator --features derive`, `cargo add serde --features derive`, and `cargo add serde_json`.

<details>
<summary>Solution</summary>

```rust
use serde::Deserialize;
use validator::{Validate, ValidationError};

fn validate_sku(sku: &str) -> Result<(), ValidationError> {
    let ok = sku.len() == 8
        && sku.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(ValidationError::new("invalid_sku"))
    }
}

#[derive(Debug, Deserialize, Validate)]
struct ProductForm {
    #[validate(length(min = 1, max = 100))]
    name: String,
    #[validate(range(min = 1, max = 1_000_000))]
    price_cents: u32,
    #[validate(custom(function = "validate_sku"))]
    sku: String,
}

fn main() {
    let body = r#"
    {
        "name": "Wireless Mouse",
        "price_cents": 2499,
        "sku": "abc-123"
    }"#;

    let form: ProductForm = serde_json::from_str(body).expect("valid JSON shape");

    match form.validate() {
        Ok(()) => println!("valid"),
        Err(errors) => {
            for (field, errs) in errors.field_errors() {
                for e in errs {
                    println!("{field}: {}", e.code);
                }
            }
        }
    }
}
```

Output:

```text
sku: invalid_sku
```

</details>

### Exercise 3: Deserialize directly into a validated newtype

**Difficulty:** Advanced

**Objective:** Push validation into the `serde` boundary so invalid input can never construct your type.

**Instructions:** Create a `Percentage` newtype wrapping an `f64` constrained to the inclusive range `0.0..=100.0`. Implement `Deserialize` for it so that a JSON number outside the range fails *inside* `serde_json::from_str` (use `serde::de::Error::custom`). Embed it in a struct `Discount { label: String, amount: Percentage }`. Show that `42.5` succeeds and `150.0` produces a deserialization error.

<details>
<summary>Solution</summary>

```rust
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Copy)]
struct Percentage(f64);

impl Percentage {
    fn parse(value: f64) -> Result<Self, String> {
        if (0.0..=100.0).contains(&value) {
            Ok(Percentage(value))
        } else {
            Err(format!("percentage must be 0..=100, got {value}"))
        }
    }
    fn get(self) -> f64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Percentage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Percentage::parse(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
struct Discount {
    label: String,
    amount: Percentage,
}

fn main() {
    let good = r#"{ "label": "summer", "amount": 42.5 }"#;
    let bad = r#"{ "label": "broken", "amount": 150.0 }"#;

    match serde_json::from_str::<Discount>(good) {
        Ok(d) => println!("ok: {} -> {}%", d.label, d.amount.get()),
        Err(e) => println!("error: {e}"),
    }
    match serde_json::from_str::<Discount>(bad) {
        Ok(d) => println!("ok: {} -> {}%", d.label, d.amount.get()),
        Err(e) => println!("error: {e}"),
    }
}
```

Output:

```text
ok: summer -> 42.5%
error: percentage must be 0..=100, got 150 at line 1 column 38
```

</details>
