// Markdown for Agents — content negotiation in front of the static assets.
//
// When a request asks for Markdown (`Accept: text/markdown`), serve the
// pre-generated `.md` sibling of the requested page with
// `Content-Type: text/markdown; charset=utf-8` and an `x-markdown-tokens`
// estimate. Everything else — browsers, crawlers, assets — falls through to the
// normal HTML/static response, so HTML stays the default.
//
// The `.md` files are produced at build time by src/pages/[...slug].md.ts.
// `run_worker_first = true` (wrangler.toml) ensures this runs even for paths
// that match a static asset, which is what lets us swap HTML for Markdown.
//
// Because the Worker runs first, Cloudflare does NOT apply public/_headers to
// these responses (the `_headers` file only attaches to responses the static
// asset layer generates on its own). So the cache-control, llms.txt `Link`, and
// `noindex` headers that public/_headers describes are (re)applied here instead,
// alongside a baseline set of security headers. See applyEdgeHeaders().

const ONE_YEAR_IMMUTABLE = "public, max-age=31536000, immutable";
const LLMS_TXT_PATHS = new Set(["/llms.txt", "/llms-full.txt", "/llms-small.txt"]);

/** True when the client explicitly accepts Markdown. Browsers never do. */
function wantsMarkdown(request) {
  const accept = (request.headers.get("accept") || "").toLowerCase();
  return accept.includes("text/markdown");
}

/** Only navigable HTML pages — extensionless paths or trailing-slash dirs. */
function isPagePath(pathname) {
  if (pathname.endsWith("/")) return true;
  const lastSegment = pathname.slice(pathname.lastIndexOf("/") + 1);
  return !lastSegment.includes(".");
}

/** Map a page path to its `.md` sibling. `/a/b/` -> `/a/b.md`, `/` -> `/index.md`. */
function toMarkdownPath(pathname) {
  if (pathname === "/" || pathname === "") return "/index.md";
  const trimmed = pathname.endsWith("/") ? pathname.slice(0, -1) : pathname;
  return `${trimmed}.md`;
}

/** Map a generated `.md` mirror back to its canonical HTML page. */
function toHtmlPagePath(markdownPath) {
  if (markdownPath === "/index.md") return "/";
  if (!markdownPath.endsWith(".md")) return undefined;
  return `${markdownPath.slice(0, -3)}/`;
}

function appendLink(headers, value) {
  const current = headers.get("link");
  if (current && current.includes(value)) return;
  headers.append("link", value);
}

/** Rough token estimate (~4 chars/token), good enough for the advisory header. */
function estimateTokens(text) {
  return Math.max(1, Math.ceil(text.length / 4));
}

/**
 * Apply the headers that public/_headers cannot, because `run_worker_first`
 * routes every request through this Worker (Cloudflare skips `_headers` for
 * Worker-generated responses). Mutates `headers` in place.
 */
function applyEdgeHeaders(headers, pathname, status = 200) {
  // Baseline security headers — safe, content-agnostic defaults for a static
  // docs site. (Deliberately no CSP/HSTS here: those need their own testing.)
  headers.set("x-content-type-options", "nosniff");
  headers.set("referrer-policy", "strict-origin-when-cross-origin");
  headers.set("x-frame-options", "SAMEORIGIN");

  // Content-hashed build assets and path-versioned fonts are effectively
  // immutable: a one-year immutable cache can never serve stale bytes.
  if (pathname.startsWith("/_astro/") || pathname.startsWith("/fonts/")) {
    headers.set("cache-control", ONE_YEAR_IMMUTABLE);
  }

  // RFC 8288: advertise the LLM-readable mirror from the site root.
  if (pathname === "/") {
    appendLink(headers, '</llms.txt>; rel="describedby"; type="text/plain"');
  }

  if (status === 200 && isPagePath(pathname)) {
    appendLink(
      headers,
      `<${toMarkdownPath(pathname)}>; rel="alternate"; type="text/markdown"`,
    );
  }

  if (status === 200 && pathname.endsWith(".md")) {
    const canonical = toHtmlPagePath(pathname);
    if (canonical) appendLink(headers, `<${canonical}>; rel="canonical"`);
  }

  // Keep the plain-text LLM mirrors fetchable by agents but out of search indexes.
  if (LLMS_TXT_PATHS.has(pathname)) {
    headers.set("x-robots-tag", "noindex");
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const isReadMethod = request.method === "GET" || request.method === "HEAD";

    if (isReadMethod && wantsMarkdown(request) && isPagePath(url.pathname)) {
      const mdPath = toMarkdownPath(url.pathname);
      if (mdPath) {
        const mdUrl = new URL(url);
        mdUrl.pathname = mdPath;
        mdUrl.search = "";
        const assetRes = await env.ASSETS.fetch(
          new Request(mdUrl, { method: "GET" }),
        );

        if (assetRes.ok) {
          const body = await assetRes.text();
          const headers = new Headers({
            "content-type": "text/markdown; charset=utf-8",
            "x-markdown-tokens": String(estimateTokens(body)),
            // Caches must key on Accept so browsers never get served Markdown.
            vary: "Accept",
            "cache-control":
              assetRes.headers.get("cache-control") ||
              "public, max-age=0, must-revalidate",
          });
          applyEdgeHeaders(headers, url.pathname, 200);
          return new Response(request.method === "HEAD" ? null : body, {
            status: 200,
            headers,
          });
        }
        // No `.md` for this page (e.g. the splash) → fall through to HTML.
      }
    }

    const response = await env.ASSETS.fetch(request);
    const headers = new Headers(response.headers);

    // Page HTML and Markdown are two representations of the same URL, so make
    // caches key on Accept; otherwise a cached HTML page could be handed to an
    // agent that asked for Markdown (or vice versa). Only stamp it on a real
    // 200 page response — not on 404s or redirects.
    if (isReadMethod && isPagePath(url.pathname) && response.status === 200) {
      const vary = headers.get("vary");
      if (!vary) headers.set("vary", "Accept");
      else if (
        !vary.toLowerCase().split(",").map((s) => s.trim()).includes("accept")
      )
        headers.set("vary", `${vary}, Accept`);
    }

    applyEdgeHeaders(headers, url.pathname, response.status);
    return new Response(response.body, {
      status: response.status,
      statusText: response.statusText,
      headers,
    });
  },
};
