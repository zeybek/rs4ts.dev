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

/** Map a page path to its `.md` sibling. `/a/b/` -> `/a/b.md`. Root has none. */
function toMarkdownPath(pathname) {
  if (pathname === "/") return null;
  const trimmed = pathname.endsWith("/") ? pathname.slice(0, -1) : pathname;
  return `${trimmed}.md`;
}

/** Rough token estimate (~4 chars/token), good enough for the advisory header. */
function estimateTokens(text) {
  return Math.max(1, Math.ceil(text.length / 4));
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
          return new Response(request.method === "HEAD" ? null : body, {
            status: 200,
            headers,
          });
        }
        // No `.md` for this page (e.g. the splash) → fall through to HTML.
      }
    }

    const response = await env.ASSETS.fetch(request);

    // Page HTML and Markdown are two representations of the same URL, so make
    // caches key on Accept; otherwise a cached HTML page could be handed to an
    // agent that asked for Markdown (or vice versa).
    if (isReadMethod && isPagePath(url.pathname)) {
      const headers = new Headers(response.headers);
      const vary = headers.get("vary");
      if (!vary) headers.set("vary", "Accept");
      else if (!vary.toLowerCase().split(",").map((s) => s.trim()).includes("accept"))
        headers.set("vary", `${vary}, Accept`);
      return new Response(response.body, {
        status: response.status,
        statusText: response.statusText,
        headers,
      });
    }

    return response;
  },
};
