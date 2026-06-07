---
title: "Deploying WebAssembly Applications"
description: "Ship a Rust .wasm module to production: wire it into Vite or webpack, serve it as application/wasm so streaming works, and compress and cache it on a CDN."
---

Getting a Rust-compiled `.wasm` module from your machine onto a production CDN: wiring it into Vite or webpack, serving the binary with the right MIME type, and caching it correctly at the edge.

---

## Quick Overview

A compiled WebAssembly (**WASM**) module is just another static asset — a `.wasm` file with a `.js` loader beside it — but it has two deployment requirements an ordinary JavaScript bundle does not: the server **must** send the `Content-Type: application/wasm` header so the browser can use the fast streaming compiler, and the binary is large enough that compression and long-lived caching genuinely matter. This file covers the last mile: integrating the `wasm-pack` output with a bundler (Vite/webpack), configuring static hosts and CDNs to serve `.wasm` correctly, and the cache/compression headers that make a Rust front end load fast. For a TypeScript/JavaScript developer, almost everything here is familiar from shipping a normal SPA. The new parts are the MIME type and the `application/wasm`-aware compression step.

> **Note:** This file assumes you already have a `pkg/` directory from `wasm-pack`. Building it (and the meaning of the `web` / `bundler` / `nodejs` targets) is covered in [Setting Up wasm-pack](/19-wasm/01-wasm-pack/); shrinking the binary with `wasm-opt`/`twiggy` and the boundary-cost analysis live in [WebAssembly Performance](/19-wasm/09-performance/). This file is strictly about getting the artifact served correctly in production.

---

## TypeScript/JavaScript Example

A typical TypeScript SPA deployment is a solved problem. You build with Vite, get a hashed bundle in `dist/`, and ship it to a static host or CDN. The host serves `.js` as `text/javascript`, the bundler fingerprints filenames so you can cache them forever, and a `netlify.toml` / `vercel.json` / nginx config sets the headers.

```typescript
// vite.config.ts — a normal TypeScript SPA build
import { defineConfig } from "vite";

export default defineConfig({
  build: {
    outDir: "dist",
    // Vite fingerprints assets: app.4f2a1b.js, so they can be cached forever.
    assetsInlineLimit: 4096,
  },
});
```

```bash
npm run build
# dist/index.html
# dist/assets/index-4f2a1b9c.js   <- content-hashed, immutable
# dist/assets/index-9e8d7c6b.css
```

```toml
# netlify.toml — cache the fingerprinted bundle aggressively
[[headers]]
  for = "/assets/*"
  [headers.values]
    Cache-Control = "public, max-age=31536000, immutable"
```

The mental model to carry forward: **build → fingerprinted static assets → CDN with correct `Content-Type` and `Cache-Control`.** Deploying a Rust WASM module reuses this exact pipeline. The only genuinely new requirements are that one of those assets is a `.wasm` binary that needs the `application/wasm` MIME type, and that it is large enough that gzip/brotli compression stops being optional.

---

## Rust Equivalent

The Rust artifact you deploy is the `pkg/` directory that `wasm-pack` produces. Building a small image-filter crate (`crate-type = ["cdylib", "rlib"]`, depending only on `wasm-bindgen`) with the browser target produces exactly this:

```bash
wasm-pack build --target web
```

```text
[INFO]:  Checking for the Wasm target...
[INFO]:  Compiling to Wasm...
[INFO]: found wasm-opt at "/opt/homebrew/bin/wasm-opt"
[INFO]: Optimizing wasm binaries with `wasm-opt`...
[INFO]:   Done in 1.15s
[INFO]:   Your wasm pkg is ready to publish at .../image-filter/pkg.
```

```text
pkg/
├── image_filter_bg.wasm        16631 bytes  <- the binary you must serve as application/wasm
├── image_filter_bg.wasm.d.ts
├── image_filter.js              5815 bytes  <- ES-module loader (the "glue")
├── image_filter.d.ts
└── package.json
```

The generated `package.json` declares this as a real ES module package, which is what lets a bundler import it like any npm dependency:

```json
{
  "name": "image-filter",
  "type": "module",
  "version": "0.1.0",
  "files": ["image_filter_bg.wasm", "image_filter.js", "image_filter.d.ts"],
  "main": "image_filter.js",
  "types": "image_filter.d.ts",
  "sideEffects": ["./snippets/*"]
}
```

There are two deployment paths, and which one you take depends on the `wasm-pack --target` you chose (see [Setting Up wasm-pack](/19-wasm/01-wasm-pack/)):

### Path A — `--target bundler`: let Vite/webpack handle it

This is the default target and the path most production SPAs take. The `pkg/` becomes a local dependency of your front-end app, and the bundler fingerprints, compresses, and copies the `.wasm` into `dist/` for you, exactly as it already does for `.js` and `.css`.

```bash
# Build the WASM package for a bundler
wasm-pack build --target bundler

# In your Vite/webpack app, depend on it as a local package
npm install ../image-filter/pkg
```

**Vite** (v5+) needs no plugin for the `bundler` target: it understands the `import` of a `.wasm` file emitted by the glue, fingerprints it, and serves it with the correct MIME during `vite dev` and in the production build.

```typescript
// src/main.ts — import the Rust functions like any ES module
import init, { invert, version } from "image-filter";

await init();              // load + instantiate the .wasm (async)
console.log(version());    // "image-filter 0.1.0"

const pixels = new Uint8Array([10, 20, 30, 255]);
invert(pixels);            // pixels is now [245, 235, 225, 255]
```

> **Tip:** On older webpack 4 you needed `experiments.asyncWebAssembly`. With webpack 5 and Vite 5+ the WASM import works out of the box for the `bundler` target. If your bundler predates that, prefer the `web` target (Path B) and skip the bundler integration entirely.

### Path B — `--target web`: ship `pkg/` as static files

With `--target web`, the glue is a self-contained ES module that loads the binary itself, no bundler required. You copy `pkg/` into your static site and `<script type="module">` it. The catch is that the glue uses `WebAssembly.instantiateStreaming`, and **streaming only works if your server sends `Content-Type: application/wasm`.** This is the single most common WASM deployment failure, and it is the focus of the next section.

```html
<!-- index.html, deployed as a static file alongside pkg/ -->
<script type="module">
  import init, { version } from "./pkg/image_filter.js";
  await init();                 // fetches ./pkg/image_filter_bg.wasm
  document.body.textContent = version();
</script>
```

---

## Detailed Explanation

### Why `application/wasm` is mandatory, not cosmetic

Look at what the generated loader actually does. The `web`-target glue resolves the binary's URL relative to itself and fetches it:

```javascript
// generated image_filter.js (excerpt) — how the binary is located
if (module_or_path === undefined) {
    module_or_path = new URL('image_filter_bg.wasm', import.meta.url);
}
// ...
module_or_path = fetch(module_or_path);
```

Then it tries the fast path, `WebAssembly.instantiateStreaming`, which compiles the module *while it is still downloading*, and contains an explicit fallback with a warning if your server got the MIME type wrong:

```javascript
// generated image_filter.js (excerpt) — the streaming path + real warning
if (typeof WebAssembly.instantiateStreaming === 'function') {
    try {
        return await WebAssembly.instantiateStreaming(module, imports);
    } catch (e) {
        const validResponse = module.ok && expectedResponseType(module.type);
        if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
            console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);
        } else { throw e; }
    }
}
const bytes = await module.arrayBuffer();
return await WebAssembly.instantiate(bytes, imports);
```

That warning string is emitted verbatim by `wasm-bindgen`'s glue (it is not something this guide wrote). If you see it in production, your host is serving the `.wasm` as `application/octet-stream` or `text/plain`. The page still works — the glue falls back to `arrayBuffer()` + `WebAssembly.instantiate` — but you lose streaming compilation, so the module is downloaded fully *before* compilation begins instead of overlapping the two. For a multi-megabyte Rust front end that is a measurable startup regression.

### Most servers already know `.wasm` — but not all

The good news: the `.wasm → application/wasm` mapping has been in the IANA registry for years, so modern tooling ships it by default. For example, Python 3's standard-library `http.server` (the one used for local testing in [Your First Rust → WebAssembly Module](/19-wasm/02-first-wasm/)) already maps it:

```bash
$ python3 -c "import mimetypes; print(mimetypes.guess_type('x.wasm'))"
('application/wasm', None)
```

Vite, Netlify, Vercel, Cloudflare Pages, and GitHub Pages all serve `.wasm` correctly out of the box. The servers that historically did **not** include older nginx releases (the bundled `mime.types` lacked the `wasm` entry until recent versions) and any hand-rolled Node `http` server, because Node has no built-in static MIME database: `express`/`serve-static` get it from the `mime-types` package, and a raw `createServer` sets nothing unless you do.

### Compression: the part that actually moves the needle

A `.wasm` binary is highly compressible. The image-filter module above is 16,631 bytes raw (built with wasm-pack 0.13.1 / wasm-bindgen 0.2.122 / wasm-opt v129; exact bytes vary by toolchain version), but transfers far smaller once the server compresses it:

| Encoding | Bytes on the wire | vs raw |
| --- | --- | --- |
| none (raw `.wasm`) | 16,631 | 100% |
| `gzip -9` | 7,728 | 46% |
| `brotli -q 11` | 6,687 | 40% |

For real Rust front ends (hundreds of KB to a few MB), brotli routinely cuts transfer size by 60–70%. Every serious CDN compresses `application/wasm` automatically *if you let it*. The failure mode is a host whose compression allowlist is keyed on `Content-Type` and does not include `application/wasm`. That is one more reason the MIME type must be right: it gates compression too.

> **Note:** Shrinking the *uncompressed* binary itself — `wasm-opt -Oz`, `twiggy` to find bloat, trimming `panic` machinery — is the subject of [WebAssembly Performance](/19-wasm/09-performance/). This section is about the transport layer: compress whatever binary you ship, and cache it.

### Caching: fingerprint and freeze

The `.wasm` binary is immutable for a given build. With the `bundler` target, Vite/webpack content-hash the filename (`image_filter_bg.<hash>.wasm`), so you can set `Cache-Control: public, max-age=31536000, immutable`, identical to how you cache a hashed `.js` bundle. With the `web` target the filename is *not* hashed by default (`image_filter_bg.wasm`), so either let a bundler hash it or version the path yourself (`/v3/pkg/...`) before applying an immutable cache, otherwise a deploy can leave clients pinned to a stale binary.

---

## Key Differences

| Concern | TypeScript SPA asset | Rust `.wasm` asset |
| --- | --- | --- |
| Artifact | `index-<hash>.js` (text) | `*_bg.wasm` (binary) + `.js` glue |
| Required `Content-Type` | `text/javascript` (everyone gets it right) | **`application/wasm`** (some hosts miss it) |
| Cost of wrong MIME | none | loses `instantiateStreaming` → slower startup |
| Compression | gzip/brotli, always on | gzip/brotli, only if allowlist includes `application/wasm` |
| Fingerprinting | automatic in every bundler | automatic with `--target bundler`; manual with `--target web` |
| Instantiation | synchronous module eval | **async** `await init()` before first call |
| Cross-origin isolation | not needed | needed only if you use WASM **threads** (`SharedArrayBuffer`) |

The deepest conceptual difference is the async load. A JavaScript module is ready the instant its `<script>` evaluates; a WASM module must be fetched, compiled, and instantiated, all asynchronous. Your deployment must therefore tolerate a brief window where the Rust functions are not yet callable: show a loading state, and never call an export before `await init()` resolves.

---

## Common Pitfalls

### Pitfall 1: Server serves `.wasm` as `application/octet-stream`

The symptom is the exact `wasm-bindgen` console warning shown above and a measurably slower first load. The fix is host-specific configuration. nginx:

```nginx
# /etc/nginx/conf.d/wasm.conf
types {
    application/wasm wasm;
}

location ~ \.wasm$ {
    add_header Content-Type application/wasm;
    add_header Cache-Control "public, max-age=31536000, immutable";
    gzip on;
    gzip_types application/wasm;   # gzip won't touch it unless listed
}
```

Netlify (`netlify.toml`) and Vercel (`vercel.json`) set it via headers config:

```toml
# netlify.toml
[[headers]]
  for = "/*.wasm"
  [headers.values]
    Content-Type = "application/wasm"
    Cache-Control = "public, max-age=31536000, immutable"
```

### Pitfall 2: A hand-rolled Node server sets no MIME at all

A raw `node:http` server returns no `Content-Type` for a `.wasm` file unless you set it. The minimal correct handler:

```javascript
// server.mjs — a static server that serves .wasm correctly
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";

createServer(async (req, res) => {
  if (req.url.endsWith(".wasm")) {
    const buf = await readFile(`.${req.url}`);
    res.setHeader("Content-Type", "application/wasm");        // the critical line
    res.setHeader("Cache-Control", "public, max-age=31536000, immutable");
    res.end(buf);
  } else {
    res.statusCode = 404;
    res.end("not found");
  }
}).listen(8099);
```

Verified with `curl`, this returns the right header:

```text
$ curl -sI http://localhost:8099/image_filter_bg.wasm | grep -i content-type
Content-Type: application/wasm
```

### Pitfall 3: `strip = true` in `[profile.release]` breaks the `wasm-pack` build

A natural size-optimization profile is tempting, but adding `strip = true` while `wasm-opt` is installed makes `wasm-pack` fail. This profile:

```toml
# Cargo.toml — DON'T add strip here when wasm-opt runs
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true          # <- this line causes the failure below
```

produces this real error (wasm-pack 0.13.1 driving wasm-opt v129; the exact validator text is version-sensitive). The compile succeeds; the failure is in the post-build `wasm-opt` pass, and the diagnostic that matters is the `wasm-validator` block at the top, not the generic tail:

```text
[wasm-validator error in function 2] unexpected false: memory.copy operations require bulk memory operations [--enable-bulk-memory-opt], on
(memory.copy
 (local.get $4)
 (local.get $0)
 (local.get $1)
)
[wasm-validator error in function 33] unexpected false: memory.fill operations require bulk memory [--enable-bulk-memory-opt], on
(memory.fill
 (local.get $1)
 (i32.const 0)
 (local.get $0)
)
Fatal: error validating input
Error: failed to execute `wasm-opt`: exited with exit status: 1
  full command: ".../wasm-opt" ".../image_filter_bg.wasm" "-o" ".../image_filter_bg.wasm-opt.wasm" "-O"
To disable `wasm-opt`, add `wasm-opt = false` to your package metadata in your `Cargo.toml`.
```

The cause is **not** double-stripping. With `strip = true`, the binary `wasm-pack` hands to its post-build `wasm-opt` pass fails `wasm-opt`'s validator: the binary uses bulk-memory operations (`memory.copy`/`memory.fill`), but those are not enabled for that validation run, so `wasm-opt` aborts before it can optimize anything. The fix is to drop `strip = true` and let `wasm-opt` own the size pass; the rest of the profile is fine and builds cleanly. (Symbol stripping and size tuning belong in [WebAssembly Performance](/19-wasm/09-performance/).)

### Pitfall 4: Calling an export before `init()` resolves

```javascript
import init, { version } from "./pkg/image_filter.js";
console.log(version());   // throws: wasm is undefined — init() hasn't run
await init();
console.log(version());   // now the module is instantiated
```

Because instantiation is asynchronous, any code that touches an export must run after `await init()` (or inside a `.then`). This bites people who treat the WASM module like a synchronously-loaded JS module.

### Pitfall 5: Forgetting cross-origin isolation when using WASM threads

If your Rust uses `wasm-bindgen-rayon` or any threaded build, the browser only exposes `SharedArrayBuffer` when the page is **cross-origin isolated**, which requires two response headers on the *HTML document*:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

Without them the module instantiates but thread spawning fails at runtime. Single-threaded WASM (the common case) does **not** need these headers. Adding them unnecessarily can break third-party embeds, so only set them when you actually ship threads.

---

## Best Practices

- **Prefer `--target bundler` for an app, `--target web` for a static drop-in.** Inside a Vite/webpack SPA, let the bundler fingerprint, compress, and MIME-tag the `.wasm` for you. Use `web` only when there is no build step.
- **Verify the MIME type after every deploy.** A one-line `curl -sI https://your.site/path/to_bg.wasm | grep -i content-type` catches the most common production WASM bug in seconds.
- **Compress `application/wasm` at the edge.** Confirm your CDN's compression allowlist includes the WASM MIME type; brotli typically cuts a Rust binary by 60–70% on the wire.
- **Fingerprint, then cache immutably.** Hashed filename + `Cache-Control: public, max-age=31536000, immutable`. With the `web` target, add the hash yourself or version the directory so a redeploy doesn't strand clients on a stale binary.
- **Keep `wasm-opt` enabled and don't fight it.** Let `wasm-pack` run `wasm-opt`; do not also set Cargo `strip = true` (Pitfall 3). Tune size in the build, not by stripping twice.
- **Only enable COOP/COEP when you ship threads.** Cross-origin isolation is a requirement for `SharedArrayBuffer`, not for ordinary single-threaded modules.
- **Show a loading state.** Render a placeholder until `await init()` resolves so the async instantiation window is invisible to users.

---

## Real-World Example

A production-flavored setup: a Rust image-filter module deployed inside a Vite SPA, served behind nginx with correct MIME, compression, and caching. This is the full, build-verified picture from Rust source to served headers.

The Rust crate (`crate-type = ["cdylib", "rlib"]`, `wasm-bindgen = "0.2"`), built with `wasm-pack build --target bundler`:

```rust
// src/lib.rs — compile-verified against wasm32-unknown-unknown
use wasm_bindgen::prelude::*;

/// Invert the colors of an RGBA pixel buffer in place.
/// `pixels` is a flat array of bytes: [r, g, b, a, r, g, b, a, ...].
#[wasm_bindgen]
pub fn invert(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        chunk[0] = 255 - chunk[0]; // R
        chunk[1] = 255 - chunk[1]; // G
        chunk[2] = 255 - chunk[2]; // B
        // chunk[3] is alpha, left unchanged
    }
}

#[wasm_bindgen]
pub fn version() -> String {
    "image-filter 0.1.0".to_string()
}
```

The size-optimized release profile (note: **no** `strip` — see Pitfall 3):

```toml
# Cargo.toml
[package]
name = "image-filter"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"

[profile.release]
opt-level = "z"     # optimize for size
lto = true          # link-time optimization
codegen-units = 1   # better optimization at the cost of build time
panic = "abort"     # drop unwinding machinery
```

Wiring it into the SPA and gating UI on `init()`:

```typescript
// src/main.ts
import init, { invert, version } from "image-filter";

async function main() {
  const status = document.getElementById("status")!;
  status.textContent = "Loading WASM…";
  await init();                               // async instantiation window
  status.textContent = `Ready: ${version()}`;

  // Apply the Rust filter to a canvas frame:
  const canvas = document.querySelector("canvas")!;
  const ctx = canvas.getContext("2d")!;
  const frame = ctx.getImageData(0, 0, canvas.width, canvas.height);
  invert(frame.data);                         // mutates the RGBA buffer in Rust
  ctx.putImageData(frame, 0, 0);
}

main();
```

```bash
# CI build + verify the deployed MIME type
wasm-pack build --target bundler
npm --prefix web ci
npm --prefix web run build           # Vite fingerprints image_filter_bg.<hash>.wasm
# after deploy, smoke-test the header:
curl -sI https://app.example.com/assets/image_filter_bg.4f2a1b.wasm | grep -i 'content-type\|content-encoding'
# Content-Type: application/wasm
# Content-Encoding: br
```

The nginx config in front of the static `dist/`:

```nginx
# nginx.conf — serve a fingerprinted Vite + WASM build
server {
    listen 443 ssl;
    root /var/www/app/dist;

    types { application/wasm wasm; }   # ensure the MIME type exists

    # brotli/gzip the WASM (compression module must list the MIME type)
    gzip on;
    gzip_types application/wasm application/javascript text/css;

    # Fingerprinted assets are immutable -> cache for a year
    location /assets/ {
        add_header Cache-Control "public, max-age=31536000, immutable";
        try_files $uri =404;
    }

    # SPA fallback for client-side routing
    location / {
        try_files $uri /index.html;
    }
}
```

The verified deployment facts behind this setup, all measured on the real artifact:

- `wasm-pack build --target web` of the crate above emits a 16,631-byte `image_filter_bg.wasm` (wasm-pack 0.13.1 / wasm-bindgen 0.2.122 / wasm-opt v129; exact bytes vary by toolchain version).
- That binary compresses to 7,728 bytes with `gzip -9` and 6,687 bytes with `brotli -q 11`, so the brotli transfer is ~40% of raw.
- A static server that sets `Content-Type: application/wasm` lets the browser use streaming instantiation; one that does not triggers the `wasm-bindgen` fallback warning quoted earlier.

---

## Further Reading

- [MDN: `WebAssembly.instantiateStreaming()`](https://developer.mozilla.org/en-US/docs/WebAssembly/JavaScript_interface/instantiateStreaming_static): why the `application/wasm` MIME type is required for the fast path.
- [IANA media types — `application/wasm`](https://www.iana.org/assignments/media-types/application/wasm): the registered type your server must send.
- [MDN: Cross-Origin-Embedder-Policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cross-Origin-Embedder-Policy) — the COOP/COEP headers needed for WASM threads (`SharedArrayBuffer`).
- [Vite — WebAssembly support](https://vite.dev/guide/features.html#webassembly) — how Vite imports and fingerprints `.wasm`.
- [The `wasm-bindgen` book — Deploying](https://rustwasm.github.io/wasm-bindgen/) — target-specific glue details.
- Within this guide: [Setting Up wasm-pack](/19-wasm/01-wasm-pack/) (building `pkg/` and choosing a target), [Your First Rust → WebAssembly Module](/19-wasm/02-first-wasm/) (loading from a static page), [WebAssembly Performance](/19-wasm/09-performance/) (shrinking the binary with `wasm-opt`/`twiggy` and the boundary cost), [Using Web APIs from Rust with web-sys](/19-wasm/06-web-apis/) and [Manipulating the DOM from Rust with web-sys](/19-wasm/07-dom-manipulation/) (using browser APIs), and [Frontend Frameworks in Rust](/19-wasm/08-yew-leptos/) (full Rust front ends, which deploy the same way). For native code instead of the browser, see [Unsafe & FFI](/20-unsafe-ffi/). Bundler/module fundamentals are in [Section 12: Modules & Packages](/12-modules-packages/); project setup basics in [Section 01](/01-getting-started/).

---

## Exercises

### Exercise 1: Catch a wrong MIME type

**Difficulty:** Beginner

**Objective:** Reproduce and then fix the `application/wasm` deployment bug.

**Instructions:**

1. Build any `wasm-pack --target web` package and serve the `pkg/` directory with a static server that returns `Content-Type: application/octet-stream` for `.wasm` (a misconfigured one).
2. Open the page, find the `wasm-bindgen` warning in the browser console, and write it down.
3. Switch to a server that sends `application/wasm` and confirm the warning disappears.

<details>
<summary>Solution</summary>

The misconfigured server triggers exactly this `wasm-bindgen`-emitted console warning (it is part of the generated glue, not your code):

```text
`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:
```

A correct static server sets the header explicitly. A minimal Node version:

```javascript
// server.mjs
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";

createServer(async (req, res) => {
  if (req.url.endsWith(".wasm")) {
    res.setHeader("Content-Type", "application/wasm");
    res.end(await readFile(`.${req.url}`));
  } else {
    res.statusCode = 404;
    res.end("not found");
  }
}).listen(8099);
```

Verifying the header confirms the fix:

```text
$ curl -sI http://localhost:8099/pkg/your_module_bg.wasm | grep -i content-type
Content-Type: application/wasm
```

With the correct MIME type, `WebAssembly.instantiateStreaming` succeeds and the warning is gone. (Python's `python3 -m http.server` also serves it correctly, since its standard library already maps `.wasm → application/wasm`.)

</details>

### Exercise 2: Measure compression savings

**Difficulty:** Intermediate

**Objective:** Quantify why compressing `application/wasm` matters before configuring a CDN.

**Instructions:**

1. Build a `wasm-pack` package and locate its `_bg.wasm` file.
2. Measure the raw size, the `gzip -9` size, and (if `brotli` is installed) the `brotli -q 11` size.
3. Compute each compressed size as a percentage of raw, and decide which `Content-Encoding` you would prefer at the edge.

<details>
<summary>Solution</summary>

```bash
RAW=$(wc -c < pkg/your_module_bg.wasm)
GZ=$(gzip -9 -c pkg/your_module_bg.wasm | wc -c)
BR=$(brotli -q 11 -c pkg/your_module_bg.wasm | wc -c)
echo "raw=$RAW gzip=$GZ brotli=$BR"
```

For the 16,631-byte image-filter module from this chapter (wasm-pack 0.13.1 / wasm-bindgen 0.2.122 / wasm-opt v129; your exact bytes will vary with toolchain version), the real numbers are:

```text
raw=16631 gzip=7728 brotli=6687
```

That is 46% of raw for gzip and 40% for brotli. Brotli wins, so configure the CDN to prefer `Content-Encoding: br` (with gzip as the fallback for clients that do not advertise `br` in `Accept-Encoding`). The key configuration step is ensuring the compression allowlist includes `application/wasm`: many defaults compress `text/*` and `application/javascript` but silently skip the WASM MIME type.

</details>

### Exercise 3: Write a cache-and-MIME header policy

**Difficulty:** Advanced

**Objective:** Produce a correct production header policy for a fingerprinted WASM build and reason about the `web`-vs-`bundler` caching difference.

**Instructions:**

1. Write a `netlify.toml` (or equivalent) that serves every `.wasm` as `application/wasm` and caches `/assets/*` immutably for one year.
2. Explain in one or two sentences why this `immutable` cache policy is safe for a `--target bundler` build but *dangerous* for a `--target web` build deployed at a fixed path.

<details>
<summary>Solution</summary>

```toml
# netlify.toml
[[headers]]
  for = "/*.wasm"
  [headers.values]
    Content-Type = "application/wasm"

[[headers]]
  for = "/assets/*"
  [headers.values]
    Cache-Control = "public, max-age=31536000, immutable"
```

The `immutable` policy is safe for `--target bundler` because Vite/webpack content-hash the filename (`image_filter_bg.<hash>.wasm`): a new build produces a *new* URL, so caching the old one forever can never serve stale code. It is dangerous for a `--target web` build served at a fixed path like `/pkg/image_filter_bg.wasm`, because the filename never changes: an `immutable, max-age=31536000` cache can pin clients to last month's binary across deploys. For the `web` target you must either let a bundler hash the file, version the directory (`/v3/pkg/...`), or use a short `max-age` with revalidation instead of `immutable`.

</details>
