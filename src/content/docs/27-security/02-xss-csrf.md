---
title: "XSS and CSRF Protection"
description: "Defend against XSS and CSRF in Rust with askama auto-escaping, SameSite cookies, CSRF tokens, and a CSP header, mapping each habit from React and Express."
---

## Quick Overview

**Cross-Site Scripting (XSS)** happens when attacker-controlled text is rendered as HTML/JavaScript instead of inert data; **Cross-Site Request Forgery (CSRF)** happens when another origin makes the user's browser send an authenticated request your server trusts. In TypeScript you lean on React's auto-escaping and a CSRF middleware; in Rust you get the same defenses — auto-escaping templates (askama), CSRF tokens, `SameSite` cookies, and a Content-Security-Policy header — but assembled from small, explicit, compile-checked pieces. This file shows how each TypeScript habit maps onto an idiomatic, current Rust web stack (axum 0.8 + askama 0.16).

> **Note:** The toolchain here is Rust 1.96.0 on the latest stable edition (2024), which `cargo new` selects automatically. Every Rust snippet below was compiled and run; the outputs are real.

---

## TypeScript/JavaScript Example

A typical Express handler that renders user-submitted comments and protects a form with a CSRF token. The bug-prone version is shown first, then the safe version.

```typescript
import express from "express";
import cookieParser from "cookie-parser";
import crypto from "node:crypto";

const app = express();
app.use(express.urlencoded({ extended: true }));
app.use(cookieParser());

// XSS: interpolating raw user input straight into HTML.
app.get("/unsafe", (req, res) => {
  const name = String(req.query.name ?? "");
  res.send(`<h1>Hello, ${name}</h1>`); // ?name=<script>steal()</script> runs!
});

// Escape on output. (React/JSX does this for you; raw string templates do not.)
function escapeHtml(s: string): string {
  const map: Record<string, string> = {
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#x27;",
  };
  return s.replace(/[&<>"']/g, (c) => map[c]);
}

app.get("/safe", (req, res) => {
  const name = escapeHtml(String(req.query.name ?? ""));
  res.send(`<h1>Hello, ${name}</h1>`);
});

// CSRF: double-submit cookie + hidden field, compared in constant time.
app.get("/form", (req, res) => {
  const token = crypto.randomBytes(32).toString("base64url");
  res.cookie("csrf_token", token, {
    httpOnly: true,
    secure: true,
    sameSite: "strict",
    path: "/",
  });
  res.send(
    `<form method="post" action="/submit">
       <input type="hidden" name="csrf_token" value="${token}">
       <input name="message"><button>Send</button>
     </form>`,
  );
});

app.post("/submit", (req, res) => {
  const cookieToken = String(req.cookies.csrf_token ?? "");
  const formToken = String(req.body.csrf_token ?? "");
  const a = Buffer.from(cookieToken);
  const b = Buffer.from(formToken);
  if (a.length !== b.length || !crypto.timingSafeEqual(a, b)) {
    return res.status(403).send("CSRF validation failed");
  }
  res.send(`Accepted: ${escapeHtml(String(req.body.message ?? ""))}`);
});
```

Running the `escapeHtml` helper on Node v22:

```text
raw:     <script>alert('xss')</script>
escaped: &lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;
```

The shape is familiar: **escape on output**, **issue a CSRF token**, **set `SameSite` cookies**, **compare tokens in constant time**. The Rust version keeps every one of those ideas; it just makes the escaping the default instead of something you must remember.

---

## Rust Equivalent

The idiomatic Rust stack uses **askama** for auto-escaping templates and **axum** + **axum-extra** for cookies and middleware. Add the dependencies in a fresh project:

```bash
cargo new comment-app && cd comment-app
cargo add axum
cargo add tokio --features full
cargo add axum-extra --features cookie
cargo add askama
cargo add getrandom
cargo add subtle
cargo add base64
cargo add serde --features derive
```

First, output encoding. Put a template in `templates/comment.html`:

```html
<article class="comment">
  <h3>{{ author }}</h3>
  <p>{{ body }}</p>
</article>
```

```rust
use askama::Template;

#[derive(Template)]
#[template(path = "comment.html")]
struct CommentTemplate<'a> {
    author: &'a str,
    body: &'a str,
}

fn main() {
    let tpl = CommentTemplate {
        author: "Mallory",
        // Attacker-supplied content that would be an XSS payload if injected raw:
        body: "<script>steal(document.cookie)</script>",
    };
    let html = tpl.render().unwrap();
    println!("{html}");
}
```

Real output. Note that the `<script>` was neutralized **without any escaping call in your code**:

```text
<article class="comment">
  <h3>Mallory</h3>
  <p>&#60;script&#62;steal(document.cookie)&#60;/script&#62;</p>
</article>
```

Second, CSRF protection with a `SameSite` cookie, a Content-Security-Policy header, and constant-time token comparison. A complete axum app:

```rust
use axum::{
    extract::{Form, Request},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use subtle::ConstantTimeEq;

const CSRF_COOKIE: &str = "csrf_token";

/// 32 random bytes (256 bits) from the OS CSPRNG, URL-safe base64.
fn generate_csrf_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS RNG failed");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Constant-time string comparison — never use `==` on secrets.
fn tokens_match(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

async fn show_form(jar: CookieJar) -> impl IntoResponse {
    let token = generate_csrf_token();
    let cookie = Cookie::build((CSRF_COOKIE, token.clone()))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Strict)
        .path("/")
        .build();
    let body = format!(
        r#"<form method="post" action="/submit">
  <input type="hidden" name="csrf_token" value="{token}">
  <input name="message"><button>Send</button>
</form>"#
    );
    (jar.add(cookie), Html(body))
}

#[derive(Deserialize)]
struct SubmitForm {
    csrf_token: String,
    message: String,
}

async fn submit(jar: CookieJar, Form(form): Form<SubmitForm>) -> Response {
    match jar.get(CSRF_COOKIE).map(|c| c.value().to_owned()) {
        Some(ct) if tokens_match(&ct, &form.csrf_token) => {
            Html(format!("Accepted: {}", form.message)).into_response()
        }
        _ => (StatusCode::FORBIDDEN, "CSRF validation failed").into_response(),
    }
}

/// Middleware adding a strict Content-Security-Policy to every response.
async fn add_csp(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; object-src 'none'; frame-ancestors 'none'",
        ),
    );
    response
}

fn app() -> Router {
    Router::new()
        .route("/form", get(show_form))
        .route("/submit", post(submit))
        .layer(middleware::from_fn(add_csp))
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

Exercised end-to-end against a running instance, the responses are:

```text
CSP header: default-src 'self'; script-src 'self'; object-src 'none'; frame-ancestors 'none'
Set-Cookie: csrf_token=0hGesYorQSmkT5ocqggUAmV9BzUAqPCJsmWsOdKzOuI; HttpOnly; SameSite=Strict; Secure; Path=/
valid token  -> 200 OK
forged token -> 403 Forbidden
```

A request with a matching cookie + form token returns **200**; a forged form token returns **403**, and every response carries the CSP header.

---

## Detailed Explanation

### Output encoding (XSS defense)

XSS is fundamentally a **context confusion** bug: data crosses into a code context (HTML, JS, an attribute, a URL) without being neutralized for that context. The fix is **contextual output encoding**: escape the bytes that are special in the destination context, at the moment you render.

- In the askama example, `#[derive(Template)]` + `#[template(path = "...")]` generates a `render()` method at **compile time**. Askama detects from the `.html` extension that the output is HTML and **auto-escapes every `{{ ... }}` expression** by default. The `<script>` became `&#60;script&#62;` with zero escaping code on your side. This mirrors how React/JSX escapes `{value}` automatically.
- `render()` returns `Result<String, askama::Error>`. In a handler you render to a `String` and wrap it in axum's `Html(...)`, which sets `Content-Type: text/html; charset=utf-8`:

  ```rust
  use askama::Template;
  use axum::{http::StatusCode, response::{Html, IntoResponse, Response}};

  #[derive(Template)]
  #[template(path = "page.html")]
  struct Page { title: String }

  async fn handler() -> Response {
      let page = Page { title: "<b>hi</b>".into() };
      match page.render() {
          Ok(html) => Html(html).into_response(),
          Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
      }
  }
  ```

  With `templates/page.html` containing `<h1>{{ title }}</h1>`, this renders `<b>hi</b>` as `<h1>&#60;b&#62;hi&#60;/b&#62;</h1>`: escaped, with no escaping call in `handler`.

- When you *don't* use a template engine, escape manually with a tiny helper (this is what askama does internally):

  ```rust playground
  fn escape_html(input: &str) -> String {
      let mut out = String::with_capacity(input.len());
      for c in input.chars() {
          match c {
              '&' => out.push_str("&amp;"),
              '<' => out.push_str("&lt;"),
              '>' => out.push_str("&gt;"),
              '"' => out.push_str("&quot;"),
              '\'' => out.push_str("&#x27;"),
              _ => out.push(c),
          }
      }
      out
  }

  fn main() {
      let attacker = r#"<script>alert('xss')</script>"#;
      println!("raw:     {attacker}");
      println!("escaped: {}", escape_html(attacker));
  }
  ```

  Real output:

  ```text
  raw:     <script>alert('xss')</script>
  escaped: &lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;
  ```

  Prefer a library (`askama`, `v_htmlescape`, `askama_escape`) for hot paths (they SIMD-accelerate the scan), but understanding the five-character core (`& < > " '`) is what matters.

### CSRF protection

CSRF exploits **ambient authority**: the browser auto-attaches the session cookie to *any* request to your origin, including a form auto-submitted from `evil.com`. Two layers defeat it, and you want both:

1. **`SameSite` cookies.** `same_site(SameSite::Strict)` tells the browser to omit the cookie on cross-site requests entirely. `SameSite::Lax` (the modern browser default for cookies without an explicit attribute) sends the cookie on top-level GET navigations but not on cross-site POSTs, usually the right choice for session cookies, because Strict breaks "follow a link into the app while logged in." This is your first, mostly-free line of defense.
2. **A CSRF token.** A random value the server issues and the form echoes back. The **double-submit cookie** pattern (shown above) stores the token in a cookie *and* a hidden field; on POST you require them to match. An attacker on another origin can't read your cookie (Same-Origin Policy) and can't guess a 256-bit value, so they can't forge the field.

Walking the Rust handler:

- `getrandom::fill(&mut bytes)` fills the buffer from the **operating-system CSPRNG** (`getrandom(2)` / `BCryptGenRandom`), never a seeded or predictable generator. Token unpredictability is the whole game. See [Secure Randomness](/27-security/06-secure-randomness/) for why `getrandom`/`OsRng` and not a fast non-cryptographic PRNG.
- `URL_SAFE_NO_PAD.encode(bytes)` turns 32 bytes into a 43-character URL/cookie-safe string.
- `tokens_match` uses `subtle::ConstantTimeEq`. A naive `cookie_token == form_token` returns early on the first differing byte, leaking *how many leading bytes matched* through timing: enough, in theory, to recover a token byte-by-byte. `ct_eq` examines every byte unconditionally and returns a `subtle::Choice`, converted to `bool` with `bool::from(...)`. (Check the length *before* the constant-time compare; length isn't secret.)
- `CookieJar` is an axum extractor: `jar.get(...)` reads an incoming cookie, and **returning** `jar.add(cookie)` from the handler emits the `Set-Cookie` header. The handler returns the tuple `(jar, Html(body))` because a leading `CookieJar` in a response tuple writes the cookies.

### Content-Security-Policy (defense in depth)

A CSP is a server-set HTTP header that tells the browser which sources of script, style, images, etc. are allowed: a backstop that limits the damage **even if** an XSS slips past your encoding. `script-src 'self'` blocks inline `<script>` and third-party scripts; `object-src 'none'` kills Flash/`<object>` vectors; `frame-ancestors 'none'` is the modern replacement for `X-Frame-Options` that prevents clickjacking. The `add_csp` middleware (`middleware::from_fn`) runs on every response via `.layer(...)`, the same way you'd add an Express middleware with `app.use(...)`.

---

## Key Differences

| Concern | TypeScript / Node | Rust (axum + askama) |
| --- | --- | --- |
| Default escaping | React/JSX escapes; raw template strings do **not** | askama `{{ }}` auto-escapes by default for `.html` |
| Opting out (raw HTML) | `dangerouslySetInnerHTML` | the `\|safe` filter — both are loud, deliberate opt-outs |
| When escaping is decided | Runtime, per render call | Compile time — escaping code is generated by the macro |
| Secure randomness | `crypto.randomBytes` | `getrandom::fill` / `OsRng` (OS CSPRNG) |
| Constant-time compare | `crypto.timingSafeEqual` | `subtle::ConstantTimeEq::ct_eq` |
| Cookie attributes | object: `{ httpOnly, secure, sameSite }` | builder: `.http_only(true).secure(true).same_site(...)` |
| CSP / security headers | `helmet` middleware | `tower-http` `SetResponseHeaderLayer` or `from_fn` |
| Type safety of responses | strings; mistakes are runtime | `IntoResponse`; many mistakes are compile errors |

The deepest difference: in Rust, **escaping is the default and the unsafe path is named and visible**. In a raw JS template literal the *safe* path is the one you must remember (`escapeHtml(...)`); forget it and you ship an XSS. With askama you must *type extra characters* (`|safe`) to become unsafe, which is exactly backwards from string templating, in the secure direction.

> **Note:** Rust gives you no magic immunity here. A web server that builds HTML with `format!` and serves it via `Html(format!("<h1>{name}</h1>"))` is *just as vulnerable* as the Express `/unsafe` route; it compiles fine. Memory safety and type safety do not prevent injection; **encoding** does.

---

## Common Pitfalls

### Pitfall 1: Bypassing auto-escaping with `|safe`

Askama's `|safe` filter (and the `Safe`/`Html` markers) emit a value **without escaping**. Given `templates/raw.html`:

```html
<div>{{ body|safe }}</div>
```

```rust
use askama::Template;

#[derive(Template)]
#[template(path = "raw.html")]
struct RawTemplate<'a> {
    body: &'a str,
}

fn main() {
    let tpl = RawTemplate {
        body: "<b>trusted</b> but <script>alert(1)</script> sneaks through",
    };
    println!("{}", tpl.render().unwrap());
}
```

Real output — the `<script>` is **not** escaped:

```text
<div><b>trusted</b> but <script>alert(1)</script> sneaks through</div>
```

Only use `|safe` on HTML you produced and *already sanitized* (e.g., output of a Markdown renderer run through the `ammonia` HTML-sanitizer crate). Never on raw user input.

### Pitfall 2: Building HTML with `format!` and serving it raw

This compiles and runs, and is a textbook XSS:

```rust
use axum::response::Html;

async fn greet(name: String) -> Html<String> {
    // Vulnerable: `name` is interpolated unescaped. Same bug as the Node /unsafe route.
    Html(format!("<h1>Hello, {name}</h1>"))
}
```

This **compiles cleanly**; the danger is silent.

The compiler is happy because there's no *type* error — only a *security* error. Route HTML through askama or your `escape_html` helper instead.

### Pitfall 3: Comparing tokens with `==`

```rust playground
fn tokens_match(cookie: &str, form: &str) -> bool {
    cookie == form // early-exit comparison: leaks match length via timing
}

fn main() {
    let _ = tokens_match("a", "b");
}
```

This compiles and *seems* correct, but `==` on `&str`/`[u8]` returns as soon as bytes differ, exposing a timing side channel. Use `subtle::ConstantTimeEq::ct_eq` as shown earlier.

### Pitfall 4: Returning mismatched response types from a handler

A real compiler error you *will* hit when a handler's branches return different types:

```rust
use axum::{
    http::StatusCode,
    response::{Html, Response},
};

async fn handler(ok: bool) -> Response {
    if ok {
        Html("<p>ok</p>")             // does not compile (E0308)
    } else {
        (StatusCode::FORBIDDEN, "no")
    }
}

fn main() {
    let _ = handler;
}
```

The actual `rustc` message:

```text
error[E0308]: mismatched types
 --> src/main.rs:8:9
  |
6 | async fn handler(ok: bool) -> Response {
  |                               -------- expected `Response<Body>` because of return type
7 |     if ok {
8 |         Html("<p>ok</p>")             // does not compile (E0308)
  |         ^^^^^^^^^^^^^^^^^ expected `Response<Body>`, found `Html<&str>`
  |
  = note: expected struct `Response<Body>`
             found struct `Html<&str>`
```

Fix it by calling `.into_response()` on each arm so both unify to `Response` — that is *why* the CSRF handler above does exactly that. This is annoying at first but valuable: the type system forces you to think about what every branch actually sends.

### Pitfall 5: Forgetting `secure` / `http_only` on cookies

`SameSite` alone is not enough. A CSRF/session cookie should also be `HttpOnly` (so XSS-injected JS can't read it) and `Secure` (so it's never sent over plain HTTP and can't be stolen on the wire). Set all three together, as in `show_form`.

---

## Best Practices

- **Encode at output, in the right context.** Use askama (or another auto-escaping engine) so HTML escaping is the default. Reserve `|safe` for content you sanitized yourself.
- **Never build HTML by string concatenation with user data.** If you must, run every interpolated value through an escaper for its exact context (HTML body vs. attribute vs. URL vs. JS differ).
- **Sanitize rich HTML with `ammonia`.** When users legitimately submit HTML (Markdown comments, WYSIWYG), render to HTML then pass it through `ammonia::clean(...)` to strip scripts and event handlers — *then* `|safe` is acceptable.
- **Layer CSRF defenses.** `SameSite=Lax` for session cookies *plus* a per-session/per-request CSRF token for state-changing routes. For pure JSON APIs called from your own SPA with a custom header (e.g., `Authorization: Bearer`), CSRF risk is low because cross-site requests can't set custom headers without a CORS preflight, but cookie-authenticated endpoints still need tokens.
- **Generate tokens from the OS CSPRNG** (`getrandom`/`OsRng`), ≥ 128 bits, and compare with `subtle`. See [Secure Randomness](/27-security/06-secure-randomness/).
- **Ship a Content-Security-Policy** as defense in depth. Prefer `tower-http`'s `SetResponseHeaderLayer` for static headers or `middleware::from_fn` for per-request nonces. Also set `X-Content-Type-Options: nosniff` and `Referrer-Policy`.
- **Set cookies `HttpOnly` + `Secure` + `SameSite`** every time. For tamper-proof cookies, use `axum-extra`'s `SignedCookieJar` or `PrivateCookieJar`.
- **Validate inputs at the boundary** so malformed data never reaches your renderer — see [Input Validation](/27-security/00-input-validation/). XSS defense (encoding) and input validation are complementary, not interchangeable: validate on input, encode on output.

---

## Real-World Example

A small "guestbook" service: it stores comments, renders them with auto-escaping, protects the POST with a CSRF token, and sends a CSP header: the production-flavored version of the snippets above. Dependencies:

```bash
cargo add axum
cargo add tokio --features full
cargo add axum-extra --features cookie
cargo add askama
cargo add getrandom subtle base64
cargo add serde --features derive
```

`templates/guestbook.html`:

```html
<!doctype html>
<title>Guestbook</title>
<ul>
{% for c in comments %}
  <li><strong>{{ c.author }}</strong>: {{ c.body }}</li>
{% endfor %}
</ul>
<form method="post" action="/comments">
  <input type="hidden" name="csrf_token" value="{{ csrf_token }}">
  <input name="author" placeholder="name">
  <input name="body" placeholder="comment">
  <button>Post</button>
</form>
```

`src/main.rs`:

```rust
use std::sync::{Arc, Mutex};

use askama::Template;
use axum::{
    extract::{Form, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use subtle::ConstantTimeEq;

const CSRF_COOKIE: &str = "csrf_token";

#[derive(Clone)]
struct Comment {
    author: String,
    body: String,
}

type AppState = Arc<Mutex<Vec<Comment>>>;

#[derive(Template)]
#[template(path = "guestbook.html")]
struct GuestbookTemplate {
    comments: Vec<Comment>,
    csrf_token: String,
}

fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS RNG failed");
    URL_SAFE_NO_PAD.encode(bytes)
}

fn tokens_match(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

async fn index(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    let token = new_token();
    let cookie = Cookie::build((CSRF_COOKIE, token.clone()))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Strict)
        .path("/")
        .build();

    let comments = state.lock().unwrap().clone();
    let page = GuestbookTemplate { comments, csrf_token: token };
    match page.render() {
        Ok(html) => (jar.add(cookie), Html(html)).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Deserialize)]
struct NewComment {
    csrf_token: String,
    author: String,
    body: String,
}

async fn add_comment(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<NewComment>,
) -> Response {
    let valid = jar
        .get(CSRF_COOKIE)
        .map(|c| tokens_match(c.value(), &form.csrf_token))
        .unwrap_or(false);

    if !valid {
        return (StatusCode::FORBIDDEN, "CSRF validation failed").into_response();
    }

    // Note: author/body are stored RAW and only escaped at render time by askama.
    state.lock().unwrap().push(Comment {
        author: form.author,
        body: form.body,
    });
    (StatusCode::SEE_OTHER, [(header::LOCATION, "/")]).into_response()
}

async fn add_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; object-src 'none'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

#[tokio::main]
async fn main() {
    let state: AppState = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/", get(index))
        .route("/comments", axum::routing::post(add_comment))
        .layer(middleware::from_fn(add_security_headers))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("guestbook on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}
```

Every comment posted with `<script>` in the body is stored verbatim but rendered escaped (because askama escapes `{{ c.body }}`), so it can never execute. Every state-changing POST is gated on a CSRF token bound to a `SameSite=Strict` cookie, and every response carries a CSP. That is the same defense-in-depth posture you'd build with `helmet` + a CSRF middleware in Express, expressed in types the compiler checks for you.

> **Tip:** In a real deployment put this behind TLS (see [TLS/SSL with rustls](/27-security/05-tls-ssl/)) — `Secure` cookies require HTTPS — and store comments in a database with parameterized queries (see [SQL Injection Prevention](/27-security/01-sql-injection/)).

---

## Further Reading

- [OWASP XSS Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross_Site_Scripting_Prevention_Cheat_Sheet.html)
- [OWASP CSRF Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)
- [MDN: Content-Security-Policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Content-Security-Policy)
- [MDN: SameSite cookies](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Set-Cookie/SameSite)
- [askama documentation](https://docs.rs/askama) and [the askama book](https://askama.readthedocs.io/) — auto-escaping and the `|safe` filter
- [`subtle` crate](https://docs.rs/subtle) — constant-time primitives; [`ammonia`](https://docs.rs/ammonia) — HTML sanitization
- [axum 0.8 docs](https://docs.rs/axum) and [`axum-extra` cookies](https://docs.rs/axum-extra/latest/axum_extra/extract/cookie/)
- Related guide sections: [Input Validation](/27-security/00-input-validation/) · [SQL Injection Prevention](/27-security/01-sql-injection/) · [Secure Randomness](/27-security/06-secure-randomness/) · [Cryptography](/27-security/03-cryptography/) · [Secrets Management](/27-security/07-secrets-management/)
- Web foundations: [Section 16: Web APIs](/16-web-apis/) — extractors, middleware, sessions · production hardening: [Section 28: Production](/28-production/)
- Background: [Getting Started](/01-getting-started/) · [Basics](/02-basics/)

---

## Exercises

### Exercise 1: Per-request CSP nonce

**Difficulty:** Beginner

**Objective:** Generate a cryptographically random Content-Security-Policy **nonce** so you can allow specific inline scripts while keeping `script-src` locked down.

**Instructions:** Write a function `csp_nonce() -> String` that produces 16 random bytes from the OS CSPRNG and encodes them as standard base64. Print a `script-src` directive that uses it and a matching `<script nonce="...">` tag. (In a real app the *same* nonce goes in both the header and every inline `<script>` tag for that response.)

<details>
<summary>Solution</summary>

```rust playground
use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Generate a per-request CSP nonce: 16 random bytes, standard base64.
/// Use it in BOTH the CSP header (`script-src 'nonce-...'`) and the matching
/// `<script nonce="...">` tags so only your inline scripts execute.
fn csp_nonce() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("OS RNG failed");
    STANDARD.encode(bytes)
}

fn main() {
    let nonce = csp_nonce();
    println!("script-src 'self' 'nonce-{nonce}'");
    println!("<script nonce=\"{nonce}\">/* inline */</script>");
}
```

Real output (the nonce changes every run — that is the point):

```text
script-src 'self' 'nonce-YayonlD+bckJKKUWVlYNAw=='
<script nonce="YayonlD+bckJKKUWVlYNAw==">/* inline */</script>
```

Dependencies: `cargo add getrandom base64`.

</details>

### Exercise 2: A correct `SameSite` session cookie

**Difficulty:** Intermediate

**Objective:** Build a session cookie with the right security attributes and understand why `Lax` (not `Strict`) is the usual choice for *session* cookies.

**Instructions:** Write `build_session_cookie(value: &str) -> Cookie<'static>` that sets `HttpOnly`, `Secure`, `SameSite=Lax`, and `Path=/`. Print the rendered cookie string. Then, in a comment, explain when you'd choose `Strict` instead.

<details>
<summary>Solution</summary>

```rust
use axum_extra::extract::cookie::{Cookie, SameSite};

fn build_session_cookie(value: &str) -> Cookie<'static> {
    Cookie::build(("session", value.to_owned()))
        .http_only(true) // JS (even injected XSS) cannot read it
        .secure(true) // never sent over plain HTTP
        .same_site(SameSite::Lax) // sent on top-level GET navigations, blocked on cross-site POST
        .path("/")
        .build()
}

fn main() {
    let c = build_session_cookie("abc123");
    println!("{}", c);
    // Choose SameSite::Strict for high-value cookies (e.g. an admin/banking session)
    // where you accept that following an external link into the app shows logged-out
    // until the next same-site request. Lax keeps "click a link while logged in" working,
    // which is why it's the default for ordinary session cookies.
}
```

Real output:

```text
session=abc123; HttpOnly; SameSite=Lax; Secure; Path=/
```

Dependencies: `cargo add axum-extra --features cookie`.

</details>

### Exercise 3: Timing-safe CSRF validator with length check

**Difficulty:** Advanced

**Objective:** Implement a CSRF validator that is correct against both forgery *and* timing attacks, and prove the constant-time path is taken.

**Instructions:** Write `validate(cookie: &str, form: &str) -> bool` that (a) rejects immediately if lengths differ — length is not secret — and (b) compares equal-length tokens in constant time with `subtle`. Add tests that a matching pair returns `true`, a wrong same-length token returns `false`, and a different-length token returns `false`. Explain why the early length check does not reintroduce a meaningful side channel.

<details>
<summary>Solution</summary>

```rust playground
use subtle::ConstantTimeEq;

/// Validate a CSRF token. Length is compared in variable time (it is not secret),
/// but token *bytes* of equal length are compared in constant time so an attacker
/// cannot recover the token byte-by-byte from response timing.
fn validate(cookie: &str, form: &str) -> bool {
    let (a, b) = (cookie.as_bytes(), form.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    bool::from(a.ct_eq(b))
}

fn main() {
    let token = "0hGesYorQSmkT5ocqggUAmV9BzUAqPCJsmWsOdKzOuI";
    println!("match same       = {}", validate(token, token));
    println!("match wrong      = {}", validate(token, "0000000000000000000000000000000000000000000"));
    println!("match diff-len   = {}", validate(token, "short"));
}

#[cfg(test)]
mod tests {
    use super::validate;

    #[test]
    fn matching_pair_is_valid() {
        assert!(validate("deadbeef", "deadbeef"));
    }

    #[test]
    fn wrong_same_length_is_invalid() {
        assert!(!validate("deadbeef", "deadbeee"));
    }

    #[test]
    fn different_length_is_invalid() {
        assert!(!validate("deadbeef", "dead"));
    }
}
```

Real output:

```text
match same       = true
match wrong      = false
match diff-len   = false
```

**Why the length check is safe:** the token length is fixed and public (every token you issue is the same size), so leaking "lengths differ" reveals nothing an attacker doesn't already know. The *contents* — the only secret — are compared with `ct_eq`, which touches every byte regardless of where the first mismatch is, so timing reveals nothing about how many bytes matched. Dependencies: `cargo add subtle`.

</details>
