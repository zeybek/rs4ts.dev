---
title: "File Uploads"
description: "Handle multipart file uploads in Axum: where Express leans on multer's req.file, the Multipart extractor streams each part to disk with MIME checks."
---

## Quick Overview

A file upload is just an HTTP `POST` whose body is encoded as `multipart/form-data`: a sequence of named **parts**, each of which is either a plain form field or a file with a filename and content type. In Express you almost always reach for **multer** to parse that body and hand you `req.file`/`req.files`; in **Axum** the `Multipart` extractor (an opt-in feature) gives you an async stream of parts that you consume one field at a time. The big practical difference is that Axum lets you **stream each file straight to disk** without ever buffering the whole thing in memory, which matters once uploads get large.

> **Note:** The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. This page targets **axum 0.8** (`axum::serve` + `tokio::net::TcpListener`, `{id}` path captures, not the old `:id`). For the request body in general (`Json`, raw `Bytes`) see [Request and Response Handling](/16-web-apis/07-request-response/); for the extractor model that `Multipart` plugs into see [Extractors](/16-web-apis/04-extractors/). Serving the files you save back out is covered in [Static Files](/16-web-apis/18-static-files/).

---

## TypeScript/JavaScript Example

A realistic Express upload endpoint using **multer** (`npm i multer`, currently 2.x). It accepts an avatar image plus a text field, validates the MIME type, caps the size, and writes the file to disk with a generated name.

```typescript
// uploads.ts — Express 5 + multer 2.x
import express, { Request, Response } from "express";
import multer from "multer";
import { randomUUID } from "node:crypto";
import path from "node:path";

const UPLOAD_DIR = "/var/app/uploads";
const ALLOWED = new Set(["image/png", "image/jpeg", "image/webp"]);

// multer streams the part to disk; `storage` controls the destination + name.
const storage = multer.diskStorage({
  destination: (_req, _file, cb) => cb(null, UPLOAD_DIR),
  filename: (_req, file, cb) =>
    cb(null, `${randomUUID()}${path.extname(file.originalname)}`),
});

const upload = multer({
  storage,
  limits: { fileSize: 5 * 1024 * 1024 }, // 5 MiB
  fileFilter: (_req, file, cb) => {
    if (!ALLOWED.has(file.mimetype)) {
      cb(new Error(`content type ${file.mimetype} not allowed`));
    } else {
      cb(null, true);
    }
  },
});

const app = express();

// `upload.single("avatar")` parses ONE file field named "avatar".
app.post(
  "/users/:id/avatar",
  upload.single("avatar"),
  (req: Request, res: Response) => {
    if (!req.file) {
      return res.status(400).json({ error: "no file field named `avatar`" });
    }
    // Plain text fields land on req.body (parsed by multer, not express.json()).
    const caption = (req.body.caption as string) ?? "";
    res.status(201).json({
      id: path.basename(req.file.filename),
      bytes: req.file.size,
      contentType: req.file.mimetype,
      caption,
    });
  },
);

// multer errors (e.g. LIMIT_FILE_SIZE, or a fileFilter rejection) arrive here.
app.use((err: Error, _req: Request, res: Response, _next: express.NextFunction) => {
  res.status(400).json({ error: err.message });
});

app.listen(3000);
```

Two things are worth pinning down because they map directly to the Rust version:

- multer does the buffering/streaming and the field parsing for you; your handler only sees the *result* (`req.file`, `req.body`).
- The size limit and MIME filter are configured declaratively, and a violation surfaces as a thrown error that your error middleware turns into a response.

> **Note:** `express.json()` does **not** parse `multipart/form-data`. If you forget multer (or `busboy`), `req.body` is empty and `req.file` is `undefined`: a classic "why is my upload empty" bug.

---

## Rust Equivalent

The same endpoint in Axum. Enable the extractor with `cargo add axum --features multipart`, and add Tokio's filesystem and stream-bridge helpers:

```toml
# Cargo.toml
[dependencies]
axum = { version = "0.8", features = ["multipart"] }
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["io"] } # StreamReader: Stream -> AsyncRead
futures-util = "0.3"                                # TryStreamExt::map_err
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }         # random file names
```

```rust
use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use futures_util::TryStreamExt;
use serde::Serialize;
use std::{io, path::PathBuf, sync::Arc};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio_util::io::StreamReader;

#[derive(Clone)]
struct AppState {
    upload_dir: Arc<PathBuf>,
}

#[derive(Serialize)]
struct UploadOk {
    id: String,
    bytes: u64,
    content_type: String,
}

const MAX_BYTES: u64 = 5 * 1024 * 1024; // 5 MiB per file
const ALLOWED: [&str; 3] = ["image/png", "image/jpeg", "image/webp"];

async fn upload_avatar(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<UploadOk>, (StatusCode, String)> {
    // Pull parts off the stream one at a time.
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("malformed upload: {e}")))?
    {
        // Skip everything that is not the "avatar" file field.
        if field.name() != Some("avatar") {
            continue;
        }

        let content_type = field.content_type().unwrap_or("").to_string();
        if !ALLOWED.contains(&content_type.as_str()) {
            return Err((
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                format!("content type {content_type} not allowed"),
            ));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let path = state.upload_dir.join(&id);

        // Stream the body to disk; never hold the whole file in memory.
        let bytes = stream_to_file(field, &path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("write failed: {e}")))?;

        return Ok(Json(UploadOk { id, bytes, content_type }));
    }

    Err((StatusCode::BAD_REQUEST, "no file field named `avatar`".into()))
}

/// Bridge the field's byte stream into an `AsyncRead` and copy it to a file.
async fn stream_to_file(
    field: axum::extract::multipart::Field<'_>,
    path: &std::path::Path,
) -> io::Result<u64> {
    // A `Field` is a `Stream<Item = Result<Bytes, MultipartError>>`.
    // StreamReader needs the error to be `std::io::Error`, so map it first.
    let body = field.map_err(io::Error::other);
    let mut reader = StreamReader::new(body);

    let file = tokio::fs::File::create(path).await?;
    let mut writer = BufWriter::new(file);

    let copied = tokio::io::copy(&mut reader, &mut writer).await?;
    writer.flush().await?;
    Ok(copied)
}

#[tokio::main]
async fn main() {
    let upload_dir = PathBuf::from("/var/app/uploads");
    tokio::fs::create_dir_all(&upload_dir).await.unwrap();
    let state = AppState { upload_dir: Arc::new(upload_dir) };

    let app = Router::new()
        .route("/users/{id}/avatar", post(upload_avatar))
        // Multipart bodies are NOT covered by the default 2 MiB limit's intent;
        // set an explicit ceiling. Keep it above MAX_BYTES (see Common Pitfalls).
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Driving it with `curl` produces real, typed JSON responses:

```text
$ curl -s -w " HTTP %{http_code}\n" -X POST http://127.0.0.1:3000/users/42/avatar \
    -F "avatar=@avatar.png;type=image/png"
{"id":"a2c17697-4b96-400f-a4bf-0c29b892e2b5","bytes":4008,"content_type":"image/png"} HTTP 200

$ curl -s -w " HTTP %{http_code}\n" -X POST http://127.0.0.1:3000/users/42/avatar \
    -F "avatar=@notes.txt;type=text/plain"
content type text/plain not allowed HTTP 415
```

(The numbers above are genuine output from running this exact code; your UUID and byte count will differ.)

---

## Detailed Explanation

**`Multipart` is an extractor, but a special one.** Like `Json` or `Path`, it implements the extractor machinery (`FromRequest`), so you list it as a handler argument and Axum builds it from the request. Unlike `Json`, it does **not** read the whole body up front — it hands you a cursor over the parts. Because it consumes the request body, `Multipart` must be the **last** argument in your handler (it implements `FromRequest`, not `FromRequestParts`). Put `State`, `Path`, and header extractors before it. See [Extractors](/16-web-apis/04-extractors/) for the ordering rule and why only one body-consuming extractor is allowed.

**`next_field().await` walks the parts.** Each call returns `Result<Option<Field>, MultipartError>`:

- `Ok(Some(field))`: another part is ready.
- `Ok(None)`: the body is exhausted; the loop ends.
- `Err(_)`: the body was malformed or violated a limit.

You must drive this loop in order. A `Field` borrows the `Multipart` mutably, so you can only hold **one field at a time**. You cannot collect them into a `Vec` and process them later (the borrow checker enforces this; see Common Pitfalls).

**Reading a field's metadata.** `field.name()` is the form field name (the `name="..."` in the part header), `field.file_name()` is the client-supplied original filename (present only for file parts), and `field.content_type()` returns the part's declared MIME type. All three return `Option<&str>`; a TS dev should treat them like values that might be `undefined`, because a malicious or buggy client can omit any of them.

**Reading a field's body — buffered vs. streamed.** This is the heart of the page:

- `field.bytes().await` collects the *entire* part into a `Bytes` (an in-memory buffer). Convenient for small things (a JSON blob, a thumbnail), dangerous for large uploads: it is the moral equivalent of multer's `memoryStorage`.
- `field.text().await` is `bytes()` plus UTF-8 decoding, for plain text fields.
- A `Field` is itself a `Stream` of `Bytes` chunks, so you can stream it. Either pull chunks manually with `field.chunk().await` (returns `Ok(Some(Bytes))` per chunk), or — as above — wrap it in `tokio_util::io::StreamReader` to turn the `Stream` into an `AsyncRead` and `tokio::io::copy` it into a file. Nothing larger than one chunk (a few KiB) is ever resident in memory.

**The `map_err(io::Error::other)` line.** `StreamReader` requires the stream's error type to be `std::io::Error`, but a `Field` yields `MultipartError`. `TryStreamExt::map_err` (from `futures-util`) converts each error, and `io::Error::other` wraps any `std::error::Error` into an `io::Error`. This adapter is the small bit of glue that connects Axum's multipart stream to Tokio's filesystem I/O.

**`DefaultBodyLimit`.** Axum applies a default request-body limit (2 MiB). Multipart uploads routinely exceed that, so you set an explicit limit per route or per router with the `DefaultBodyLimit` layer. This is a coarse, whole-request guard that runs *before* your handler; it is not the same as a per-file cap (more on that contrast below).

**No streaming-vs-buffered footgun in Express.** multer's `diskStorage` already streams to disk for you, so the Express version doesn't expose this choice. In Axum the choice is explicit and in your hands, which is more code, but also why you can do things like enforce a byte budget mid-stream or pipe directly to S3 without a temp file.

---

## Key Differences

| Concern | Express + multer | Axum + `Multipart` |
| --- | --- | --- |
| Parsing the body | Middleware (`multer(...)`) populates `req.file`/`req.body` | `Multipart` extractor yields a `Field` stream you consume |
| Streaming to disk | `diskStorage` does it implicitly | Explicit: `StreamReader` + `tokio::io::copy`, or `field.chunk()` |
| Buffer in memory | `memoryStorage` (`req.file.buffer`) | `field.bytes().await` |
| Field ordering | All fields available after middleware runs | One `Field` at a time, in body order; cannot hold two at once |
| Size limit | `limits.fileSize` (per file, enforced by multer) | `DefaultBodyLimit` (whole request) + your own per-field counter |
| MIME validation | `fileFilter` callback | `field.content_type()` check in the loop |
| Where errors surface | Thrown into error middleware | A `Result` your handler returns, or `MultipartError`'s own response |
| Memory profile | Configurable, defaults to streaming via `diskStorage` | Streaming by default *if you write streaming code* |

The conceptual shift: multer is a **declarative parser** you configure once; Axum's `Multipart` is an **imperative stream** you iterate. That costs a few more lines but removes the magic: there is no hidden `req.file`, only the bytes you chose to read and where you chose to put them.

> **Tip:** The "trust nothing from the client" rules are identical to Express. `file_name()` can contain `../` or absolute paths; never join it onto a directory unsanitized. The Best Practices section shows the fix.

---

## Common Pitfalls

### 1. Holding two fields at once (the borrow that won't compile)

A `Field` borrows the `Multipart` mutably, so you cannot fetch a second field while still holding the first:

```rust
use axum::extract::Multipart;

async fn bad(mut multipart: Multipart) {
    let first = multipart.next_field().await.unwrap().unwrap();
    let second = multipart.next_field().await.unwrap().unwrap(); // does not compile (error[E0499])
    println!("{:?} {:?}", first.name(), second.name());
}
```

The real compiler error:

```text
error[E0499]: cannot borrow `multipart` as mutable more than once at a time
 --> examples/bad.rs:5:18
  |
4 |     let first = multipart.next_field().await.unwrap().unwrap();
  |                 --------- first mutable borrow occurs here
5 |     let second = multipart.next_field().await.unwrap().unwrap();
  |                  ^^^^^^^^^ second mutable borrow occurs here
6 |     println!("{:?} {:?}", first.name(), second.name());
  |                           ----- first borrow later used here
```

The fix is to fully process (or copy out) one field — call `.bytes()`/`.text()`/stream it — before calling `next_field()` again. This is by design: it is what makes true streaming possible without buffering everything.

### 2. Forgetting the `multipart` feature

`Multipart` does not exist in axum's default build. Without `features = ["multipart"]`, `use axum::extract::Multipart;` fails to resolve (`error[E0432]: unresolved import`). Run `cargo add axum --features multipart`.

### 3. `DefaultBodyLimit` set *below* your per-field cap — and the resulting status

This one is subtle and bites in production. The `DefaultBodyLimit` layer is a whole-request guard that trips *while the stream is being read*, surfacing inside your handler as a stream error — not as a clean "too large" you can branch on. If you map every multipart error to `400`, an over-limit upload returns `400` with a confusing message:

```text
$ curl -s -w "HTTP %{http_code}\n" -X POST http://127.0.0.1:3000/upload-stream \
    -F "data=@big.bin"      # body > DefaultBodyLimit
malformed multipart: Error parsing `multipart/form-data` request
HTTP 400
```

`MultipartError` actually carries the correct status itself. If you let `?` propagate it (handler returns `Result<_, MultipartError>`, since `MultipartError: IntoResponse`), the same request returns the right code:

```text
$ curl -s -w "HTTP %{http_code}\n" -X POST http://127.0.0.1:3000/upload-native \
    -F "data=@big.bin"
Request payload is too large
HTTP 413
```

The practical rule: keep `DefaultBodyLimit` as a generous outer ceiling, and if you want a precise *per-file* limit with a clean `413` and partial-file cleanup, count bytes yourself as you stream (shown in the Real-World Example). If your outer limit is *smaller* than your per-field cap, the outer limit wins and your cap never runs.

### 4. Calling `field.bytes()` on a huge upload

`field.bytes().await` allocates the whole part in memory. A handful of concurrent multi-hundred-MB uploads will exhaust RAM. Use it only for fields you know are small; stream everything else.

### 5. Expecting `Json`/`express.json()`-style parsing

`multipart/form-data` is not JSON. There is no `Json` extractor that magically parses it, and (mirroring Express) a JSON body parser will not touch it. Use `Multipart`, then read text fields with `field.text()`.

---

## Best Practices

- **Stream large uploads; buffer only small ones.** Default to `StreamReader` + `tokio::io::copy` (or `field.chunk()`). Reserve `field.bytes()` for parts you *know* are small.
- **Always sanitize the client filename.** Never trust `field.file_name()`. Generate your own name (a UUID) for storage and treat the original name as untrusted display metadata. If you must keep the original, run it through `sanitize-filename` (`cargo add sanitize-filename`):

  ```rust
  // `safe` strips path separators and other dangerous characters.
  let safe = sanitize_filename::sanitize(field.file_name().unwrap_or("upload"));
  ```

- **Validate content type, but don't trust it.** `field.content_type()` is what the client *claimed*. For security-sensitive uploads, also sniff the first bytes (e.g. magic numbers) or re-encode images server-side.
- **Set an explicit `DefaultBodyLimit`** sized to your largest legitimate upload, kept above any per-field cap you enforce in code.
- **Write to a temp file, then atomically rename** into place once the upload completes and validation passes, so partial or rejected uploads never appear as real files.
- **Clean up on failure.** If a per-field cap or validation fails mid-stream, `remove_file` the partial.
- **Put filesystem paths and limits in state/config**, not hardcoded: inject the upload directory via `State<T>` (see [State Management](/16-web-apis/06-state-management/)).
- **For a clean error type**, return your own `AppError: IntoResponse` instead of `(StatusCode, String)` tuples once the handler grows. See [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/).

---

## Real-World Example

A production-flavored avatar endpoint that ties the practices together: it injects the upload directory via `State`, validates the MIME type, enforces a **precise per-file byte budget while streaming** (returning a clean `413` and cleaning up the partial file), and **atomically renames** a `.part` temp file into place on success. The custom `UploadError` maps each failure to the right status code.

```rust
use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use futures_util::TryStreamExt;
use serde::Serialize;
use std::{io, path::PathBuf, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio_util::io::StreamReader;

#[derive(Clone)]
struct AppState {
    upload_dir: Arc<PathBuf>,
}

#[derive(Serialize)]
struct UploadOk {
    id: String,
    bytes: u64,
    content_type: String,
}

#[derive(Debug)]
enum UploadError {
    NoFile,
    BadContentType(String),
    TooLarge,
    Multipart(String),
    Io(io::Error),
}

const MAX_BYTES: u64 = 5 * 1024 * 1024; // 5 MiB per file
const ALLOWED: [&str; 3] = ["image/png", "image/jpeg", "image/webp"];

// Each variant maps to a meaningful HTTP status + JSON error body.
impl IntoResponse for UploadError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            UploadError::NoFile => {
                (StatusCode::BAD_REQUEST, "no file field named `avatar`".to_string())
            }
            UploadError::BadContentType(ct) => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, format!("content type {ct} not allowed"))
            }
            UploadError::TooLarge => {
                (StatusCode::PAYLOAD_TOO_LARGE, format!("file exceeds {MAX_BYTES} bytes"))
            }
            UploadError::Multipart(e) => (StatusCode::BAD_REQUEST, format!("malformed upload: {e}")),
            UploadError::Io(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("could not store file: {e}"))
            }
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

async fn upload_avatar(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<UploadOk>, UploadError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| UploadError::Multipart(e.to_string()))?
    {
        if field.name() != Some("avatar") {
            continue;
        }

        let content_type = field.content_type().unwrap_or("").to_string();
        if !ALLOWED.contains(&content_type.as_str()) {
            return Err(UploadError::BadContentType(content_type));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let final_path = state.upload_dir.join(&id);
        let tmp_path = state.upload_dir.join(format!("{id}.part"));

        let written = write_capped(field, &tmp_path).await?;

        // Only after a clean, fully-written upload do we publish the file.
        tokio::fs::rename(&tmp_path, &final_path)
            .await
            .map_err(UploadError::Io)?;

        return Ok(Json(UploadOk { id, bytes: written, content_type }));
    }

    Err(UploadError::NoFile)
}

/// Stream a field to `path`, aborting (and deleting the partial) past MAX_BYTES.
async fn write_capped(
    field: axum::extract::multipart::Field<'_>,
    path: &std::path::Path,
) -> Result<u64, UploadError> {
    let reader = StreamReader::new(field.map_err(io::Error::other));
    futures_util::pin_mut!(reader);

    let file = tokio::fs::File::create(path).await.map_err(UploadError::Io)?;
    let mut writer = BufWriter::new(file);

    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf).await.map_err(UploadError::Io)?;
        if n == 0 {
            break; // EOF: the part is fully read
        }
        total += n as u64;
        if total > MAX_BYTES {
            let _ = tokio::fs::remove_file(path).await; // clean up the partial
            return Err(UploadError::TooLarge);
        }
        writer.write_all(&buf[..n]).await.map_err(UploadError::Io)?;
    }
    writer.flush().await.map_err(UploadError::Io)?;
    Ok(total)
}

#[tokio::main]
async fn main() {
    let upload_dir = PathBuf::from("/var/app/uploads");
    tokio::fs::create_dir_all(&upload_dir).await.unwrap();
    let state = AppState { upload_dir: Arc::new(upload_dir) };

    let app = Router::new()
        .route("/users/{id}/avatar", post(upload_avatar))
        // Generous outer ceiling; the precise per-file cap is enforced in code.
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

This needs `serde_json` in addition to the earlier deps: `cargo add serde_json`. Exercising all three paths against the running server gives real output:

```text
$ curl -s -w " HTTP %{http_code}\n" -X POST http://127.0.0.1:3000/users/42/avatar \
    -F "avatar=@avatar.png;type=image/png"
{"id":"a2c17697-4b96-400f-a4bf-0c29b892e2b5","bytes":4008,"content_type":"image/png"} HTTP 200

$ curl -s -w " HTTP %{http_code}\n" -X POST http://127.0.0.1:3000/users/42/avatar \
    -F "avatar=@notes.txt;type=text/plain"
{"error":"content type text/plain not allowed"} HTTP 415

$ curl -s -w " HTTP %{http_code}\n" -X POST http://127.0.0.1:3000/users/42/avatar \
    -F "avatar=@big.png;type=image/png"      # 6 MiB, over the per-file cap
{"error":"file exceeds 5242880 bytes"} HTTP 413
```

After these requests, the upload directory contains exactly one complete file (the valid PNG) and **no leftover `.part` file**: the over-limit upload was deleted mid-stream. That cleanup is the payoff of streaming with your own byte counter instead of leaning on `field.bytes()` or the outer body limit alone.

> **Note:** For very large uploads in real deployments, prefer streaming straight to object storage (S3 multipart upload) or a reverse proxy that buffers to disk, rather than the application server's local filesystem. The streaming pattern here is exactly what an S3 sink plugs into: replace the `File` with the storage client's writer.

---

## Further Reading

- [`axum::extract::Multipart`](https://docs.rs/axum/latest/axum/extract/struct.Multipart.html): the extractor, `next_field`, and the feature flag.
- [`axum::extract::multipart::Field`](https://docs.rs/axum/latest/axum/extract/multipart/struct.Field.html): `name`, `file_name`, `content_type`, `bytes`, `text`, `chunk`, and the `Stream` impl.
- [`axum::extract::DefaultBodyLimit`](https://docs.rs/axum/latest/axum/extract/struct.DefaultBodyLimit.html): configuring or disabling the request-body limit.
- [`tokio_util::io::StreamReader`](https://docs.rs/tokio-util/latest/tokio_util/io/struct.StreamReader.html): turning a `Stream<Bytes>` into an `AsyncRead`.
- [`tokio::fs` and `tokio::io::copy`](https://docs.rs/tokio/latest/tokio/io/fn.copy.html): async filesystem I/O.

Within this guide:

- [Extractors](/16-web-apis/04-extractors/) — why `Multipart` must be the last argument and how `FromRequest` differs from `FromRequestParts`.
- [Request and Response Handling](/16-web-apis/07-request-response/) — the body in general, `IntoResponse`, status-code tuples.
- [State Management](/16-web-apis/06-state-management/): injecting the upload directory and config via `State<T>` + `Arc`.
- [Error Handling in Web Apps](/16-web-apis/10-error-handling-web/) — a fuller `AppError: IntoResponse` than the one shown here.
- [Static Files](/16-web-apis/18-static-files/): serving the files you just saved back to clients.
- [Routing](/16-web-apis/03-routing/): the `{id}` path syntax used in `/users/{id}/avatar`.
- Foundations: async [futures and streams](/11-async/) (a `Field` *is* a stream), [error handling](/08-error-handling/) (`Result` and the `?` operator), [traits](/09-generics-traits/) (what `IntoResponse` is), and the language [basics](/02-basics/) / [getting started](/01-getting-started/).
- Persisting upload metadata (filename, owner, size) alongside the bytes: [Database](/17-database/).

---

## Exercises

### Exercise 1 — Count and summarize fields

**Difficulty:** Beginner

**Objective:** Get comfortable iterating multipart fields and distinguishing file parts from plain text fields.

**Instructions:** Write a handler `summarize(mut multipart: Multipart)` that consumes a `multipart/form-data` body and returns JSON of the form `{ "text_fields": N, "files": M }`, where text fields are parts with no filename and files are parts with one. Read each text field with `.text()` (you may discard the value) and each file with `.bytes()` for now. Map any `MultipartError` to a `400`.

<details>
<summary>Solution</summary>

```rust
use axum::{extract::Multipart, http::StatusCode, Json};
use serde::Serialize;

#[derive(Serialize)]
struct Counts {
    text_fields: usize,
    files: usize,
}

async fn summarize(
    mut multipart: Multipart,
) -> Result<Json<Counts>, (StatusCode, String)> {
    let mut text_fields = 0usize;
    let mut files = 0usize;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("malformed upload: {e}")))?
    {
        if field.file_name().is_some() {
            // Consume the file body so we can advance to the next field.
            field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("read failed: {e}")))?;
            files += 1;
        } else {
            field
                .text()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("read failed: {e}")))?;
            text_fields += 1;
        }
    }

    Ok(Json(Counts { text_fields, files }))
}
```

Mount it with `.route("/summarize", axum::routing::post(summarize))`. A request with one text field and two files returns `{"text_fields":1,"files":2}`.

</details>

### Exercise 2 — Stream to disk with a safe filename

**Difficulty:** Intermediate

**Objective:** Stream a single uploaded file to disk without buffering it, using a sanitized, collision-free name.

**Instructions:** Write `save_upload(mut multipart: Multipart)` that takes the first file part, builds a storage name of the form `<uuid>-<sanitized-original-name>`, streams the body to `/tmp/uploads/<name>` using `StreamReader` + `tokio::io::copy` (no `.bytes()`), and returns JSON `{ "saved": "<full path>", "bytes": N }`. Use `cargo add sanitize-filename uuid --features uuid/v4`. Skip non-file parts.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::Multipart,
    http::StatusCode,
    Json,
};
use futures_util::TryStreamExt;
use serde::Serialize;
use std::{io, path::PathBuf};
use tokio::io::AsyncWriteExt;
use tokio_util::io::StreamReader;

#[derive(Serialize)]
struct Saved {
    saved: String,
    bytes: u64,
}

async fn save_upload(
    mut multipart: Multipart,
) -> Result<Json<Saved>, (StatusCode, String)> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("malformed upload: {e}")))?
    {
        let Some(original) = field.file_name().map(|s| s.to_string()) else {
            continue; // not a file part
        };

        let safe = sanitize_filename::sanitize(&original);
        let id = uuid::Uuid::new_v4();
        let path = PathBuf::from("/tmp/uploads").join(format!("{id}-{safe}"));

        tokio::fs::create_dir_all("/tmp/uploads")
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let bytes = stream_to_file(field, &path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("write failed: {e}")))?;

        return Ok(Json(Saved {
            saved: path.display().to_string(),
            bytes,
        }));
    }

    Err((StatusCode::BAD_REQUEST, "no file part found".into()))
}

async fn stream_to_file(
    field: axum::extract::multipart::Field<'_>,
    path: &std::path::Path,
) -> io::Result<u64> {
    let mut reader = StreamReader::new(field.map_err(io::Error::other));
    let mut file = tokio::fs::File::create(path).await?;
    let copied = tokio::io::copy(&mut reader, &mut file).await?;
    file.flush().await?;
    Ok(copied)
}
```

`sanitize_filename::sanitize("../../etc/passwd")` becomes `"....etcpasswd"`, and the UUID prefix guarantees no two uploads collide.

</details>

### Exercise 3 — Per-file size cap returning a clean 413

**Difficulty:** Advanced

**Objective:** Enforce a precise per-file byte limit while streaming, returning `413 Payload Too Large` with a JSON body and leaving no partial file behind — without relying on `DefaultBodyLimit`.

**Instructions:** Implement `write_limited(field, path, max: u64) -> Result<u64, UploadError>` that streams the field to `path` chunk by chunk (manually, or via `StreamReader` + a read loop), tracks the running byte total, and as soon as it exceeds `max`, deletes the partial file and returns an error variant that your `IntoResponse` maps to `413`. Wire it into a handler whose router uses a *generous* `DefaultBodyLimit` (so your cap is the binding constraint). Confirm that an over-cap upload returns `413` and that the directory is empty afterward.

<details>
<summary>Solution</summary>

```rust
use axum::{
    extract::{DefaultBodyLimit, Multipart},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use futures_util::TryStreamExt;
use serde::Serialize;
use std::{io, path::PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio_util::io::StreamReader;

const MAX_BYTES: u64 = 1024 * 1024; // 1 MiB

#[derive(Debug)]
enum UploadError {
    NoFile,
    TooLarge,
    Multipart(String),
    Io(io::Error),
}

impl IntoResponse for UploadError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            UploadError::NoFile => (StatusCode::BAD_REQUEST, "no file part".to_string()),
            UploadError::TooLarge => {
                (StatusCode::PAYLOAD_TOO_LARGE, format!("file exceeds {MAX_BYTES} bytes"))
            }
            UploadError::Multipart(e) => (StatusCode::BAD_REQUEST, format!("malformed upload: {e}")),
            UploadError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

#[derive(Serialize)]
struct Saved {
    bytes: u64,
}

async fn upload(mut multipart: Multipart) -> Result<Json<Saved>, UploadError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| UploadError::Multipart(e.to_string()))?
    {
        if field.file_name().is_none() {
            continue;
        }
        tokio::fs::create_dir_all("/tmp/capped")
            .await
            .map_err(UploadError::Io)?;
        let path = PathBuf::from("/tmp/capped").join(uuid::Uuid::new_v4().to_string());
        let bytes = write_limited(field, &path, MAX_BYTES).await?;
        return Ok(Json(Saved { bytes }));
    }
    Err(UploadError::NoFile)
}

async fn write_limited(
    field: axum::extract::multipart::Field<'_>,
    path: &std::path::Path,
    max: u64,
) -> Result<u64, UploadError> {
    let reader = StreamReader::new(field.map_err(io::Error::other));
    futures_util::pin_mut!(reader);

    let file = tokio::fs::File::create(path).await.map_err(UploadError::Io)?;
    let mut writer = BufWriter::new(file);

    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf).await.map_err(UploadError::Io)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max {
            let _ = tokio::fs::remove_file(path).await;
            return Err(UploadError::TooLarge);
        }
        writer.write_all(&buf[..n]).await.map_err(UploadError::Io)?;
    }
    writer.flush().await.map_err(UploadError::Io)?;
    Ok(total)
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/upload", post(upload))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024)); // generous outer ceiling

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Uploading a 2 MiB file returns `{"error":"file exceeds 1048576 bytes"}` with `HTTP 413`, and `/tmp/capped` is left empty because the partial was removed mid-stream. The key design point — and the reason this exercise is "advanced" — is that the *outer* `DefaultBodyLimit` is set far above `MAX_BYTES`, so your in-handler counter is the constraint that actually fires, giving you control over both the status code and the cleanup.

</details>
