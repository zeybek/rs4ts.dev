import { definePlugin } from "@expressive-code/core";

// Expressive Code plugin: opt-in "Run" button for Rust code blocks.
//
// Mark a fence with the `playground` flag to make it runnable:
//
//     ```rust playground
//     fn main() { println!("hi"); }
//     ```
//
// At render time we tag the block's <figure> with `rs-playground` and stash the
// exact source (base64, so newlines/quotes/unicode survive as an attribute).
// The shipped client module (jsModules) lazily adds a Run button that POSTs the
// code to play.rust-lang.org's CORS-enabled /execute API and renders the real
// stdout/stderr inline. Blocks with a `fn main` run as a binary; fragments
// without one are sent as a library so the button becomes a compile-check.
//
// Code runs on the stable channel, edition 2024 by default; pin an older
// edition per fence with `edition="2021"` for the rare snippet that needs it.
//
// Known limits (mirror the Rust Playground itself): only crates in the
// playground's bundled set work — std, tokio, serde, rand, clap, rayon, regex,
// anyhow/thiserror, etc. — while sea-orm, sqlx, diesel, axum, wasm-bindgen,
// proc-macro crates, and anything needing a live database or the network do
// not. Only mark blocks you expect to run.

const clientJs = `
const ENDPOINT = "https://play.rust-lang.org/execute";

function decodeCode(b64) {
  const bin = atob(b64);
  const bytes = Uint8Array.from(bin, function (c) { return c.charCodeAt(0); });
  return new TextDecoder().decode(bytes);
}

function setup(fig) {
  if (fig.dataset.rsReady) return;
  fig.dataset.rsReady = "1";
  const code = decodeCode(fig.dataset.rsCode || "");
  if (!code) return;

  const hasMain = /\\bfn\\s+main\\s*\\(/.test(code);
  const edition = fig.dataset.rsEdition || "2024";

  const bar = document.createElement("div");
  bar.className = "rs-run-bar";

  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "rs-run-btn";
  btn.textContent = hasMain ? "\\u25B6 Run" : "\\u25B6 Check";

  const hint = document.createElement("span");
  hint.className = "rs-run-hint";
  hint.textContent = "play.rust-lang.org";

  bar.appendChild(btn);
  bar.appendChild(hint);
  fig.appendChild(bar);

  // The output panel is created lazily on the first click, so nothing extra
  // shows until the reader actually runs the snippet.
  let out = null;

  btn.addEventListener("click", async function () {
    btn.disabled = true;
    const label = btn.textContent;
    btn.textContent = "Running\\u2026";
    if (!out) {
      out = document.createElement("pre");
      out.setAttribute("role", "status");
      fig.appendChild(out);
    }
    out.className = "rs-run-output is-running";
    out.textContent = "Compiling on play.rust-lang.org\\u2026";
    try {
      const res = await fetch(ENDPOINT, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          channel: "stable",
          mode: "debug",
          edition: edition,
          crateType: hasMain ? "bin" : "lib",
          tests: false,
          backtrace: false,
          code: code,
        }),
      });
      if (!res.ok) throw new Error("HTTP " + res.status);
      const data = await res.json();
      if (data.success) {
        out.className = "rs-run-output is-ok";
        const stdout = data.stdout || "";
        out.textContent = stdout || (hasMain ? "(ran successfully, no output)" : "(compiles \\u2713)");
      } else {
        out.className = "rs-run-output is-err";
        out.textContent = (data.stderr || data.error || "Unknown error").replace(/\\s+$/, "");
      }
    } catch (err) {
      out.className = "rs-run-output is-err";
      out.textContent = "Could not reach the Rust Playground: " + (err && err.message ? err.message : String(err));
    } finally {
      btn.disabled = false;
      btn.textContent = label;
    }
  });
}

function init() {
  const figs = document.querySelectorAll("figure.rs-playground");
  for (let i = 0; i < figs.length; i++) setup(figs[i]);
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", init);
} else {
  init();
}
document.addEventListener("astro:page-load", init);
`;

const styles = `
.rs-playground { position: relative; }
.rs-playground .rs-run-bar {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.5rem 0.8rem;
  border-top: 1px solid var(--ec-brdCol, rgba(127, 127, 127, 0.25));
}
.rs-playground .rs-run-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  font-family: var(--sl-font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
  font-size: 0.82rem;
  font-weight: 600;
  line-height: 1;
  color: #fff;
  background: #ce422b;
  border: 0;
  border-radius: 4px;
  padding: 0.45rem 0.85rem;
  cursor: pointer;
  transition: background 0.15s ease;
}
.rs-playground .rs-run-btn:hover { background: #e4593f; }
.rs-playground .rs-run-btn:focus-visible { outline: 2px solid #e4593f; outline-offset: 2px; }
.rs-playground .rs-run-btn:disabled { opacity: 0.6; cursor: progress; }
.rs-playground .rs-run-hint {
  font-family: var(--sl-font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
  font-size: 0.72rem;
  opacity: 0.55;
}
.rs-playground .rs-run-output {
  margin: 0;
  padding: 0.7rem 0.9rem;
  font-family: var(--sl-font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
  font-size: 0.8rem;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
  border-top: 1px solid var(--ec-brdCol, rgba(127, 127, 127, 0.25));
  border-left: 3px solid transparent;
  background: rgba(0, 0, 0, 0.22);
  max-height: 22rem;
  overflow: auto;
}
.rs-playground .rs-run-output.is-running { opacity: 0.7; }
.rs-playground .rs-run-output.is-ok { border-left-color: #3fb950; }
.rs-playground .rs-run-output.is-err { border-left-color: #f85149; }
`;

export function pluginRustPlayground() {
  return definePlugin({
    name: "rs4ts:rust-playground",
    jsModules: [clientJs],
    baseStyles: styles,
    hooks: {
      postprocessRenderedBlock: ({ codeBlock, renderData }) => {
        if (codeBlock.language !== "rust") return;
        if (!codeBlock.metaOptions.getBoolean("playground")) return;

        const root = renderData.blockAst;
        if (!root || root.type !== "element") return;

        const props = root.properties || (root.properties = {});
        const existing = props.className;
        const classes = Array.isArray(existing)
          ? existing
          : existing
            ? [existing]
            : [];
        classes.push("rs-playground");
        props.className = classes;

        // Exact source, base64(utf8) so it round-trips losslessly as an attr.
        props["data-rs-code"] = Buffer.from(codeBlock.code, "utf8").toString("base64");

        const edition = codeBlock.metaOptions.getString("edition");
        if (edition) props["data-rs-edition"] = edition;
      },
    },
  });
}
