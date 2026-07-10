import assert from "node:assert/strict";
import test from "node:test";

import worker from "./index.js";

function assetEnvironment() {
  const requests = [];
  return {
    requests,
    env: {
      ASSETS: {
        async fetch(request) {
          requests.push(request);
          const pathname = new URL(request.url).pathname;
          const isMarkdown = pathname.endsWith(".md");
          return new Response(isMarkdown ? "# Markdown\n" : "<!doctype html>", {
            status: 200,
            headers: {
              "content-type": isMarkdown
                ? "text/markdown; charset=utf-8"
                : "text/html; charset=utf-8",
            },
          });
        },
      },
    },
  };
}

test("redirects HTTP requests for the production host", async () => {
  const { env, requests } = assetEnvironment();
  const response = await worker.fetch(new Request("http://rs4ts.dev/guide?q=1"), env);

  assert.equal(response.status, 301);
  assert.equal(response.headers.get("location"), "https://rs4ts.dev/guide?q=1");
  assert.equal(requests.length, 0);
});

test("keeps Wrangler's local HTTP origin reachable", async () => {
  const { env, requests } = assetEnvironment();
  const response = await worker.fetch(new Request("http://localhost:8787/"), env);

  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type"), /^text\/html/);
  assert.equal(requests.length, 1);
});

test("negotiates Markdown using media-range quality values", async (t) => {
  const cases = [
    { accept: "text/markdown", contentType: /^text\/markdown/ },
    { accept: "text/markdown, */*", contentType: /^text\/markdown/ },
    { accept: "text/html, text/markdown;q=0", contentType: /^text\/html/ },
    { accept: "text/html;q=1, text/markdown;q=0.5", contentType: /^text\/html/ },
    { accept: "text/html;q=0.5, text/markdown;q=0.8", contentType: /^text\/markdown/ },
    { accept: "text/markdown;q=invalid", contentType: /^text\/html/ },
    { accept: "*/*", contentType: /^text\/html/ },
  ];

  for (const { accept, contentType } of cases) {
    await t.test(accept, async () => {
      const { env } = assetEnvironment();
      const request = new Request("https://rs4ts.dev/05-ownership/", {
        headers: { accept },
      });
      const response = await worker.fetch(request, env);

      assert.equal(response.status, 200);
      assert.match(response.headers.get("content-type"), contentType);
    });
  }
});
