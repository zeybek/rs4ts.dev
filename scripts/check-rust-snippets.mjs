import { spawn } from "node:child_process";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const docsRoot = path.join(repoRoot, "src/content/docs");
const minimumCoverage = 700;

// These roots need Cargo dependencies or a target/runtime that standalone rustc
// cannot provide. Package names with '-' use their Rust import form with '_'.
const externalCrateRoots = new Set([
  "aes_gcm", "ammonia", "anyhow", "askama", "async_stream", "axum",
  "axum_extra", "base64", "bcrypt", "bindgen", "bon", "bumpalo", "bytes",
  "cc", "chrono", "clap", "config", "console", "crc32fast", "criterion", "crossbeam",
  "crossbeam_channel", "ctrlc", "diesel", "dotenvy", "envy", "figment",
  "flate2", "futures", "futures_util", "getrandom", "glob", "handlebars",
  "hdrhistogram", "heapless", "hkdf", "hmac", "http", "hyper", "indexmap",
  "indicatif", "insta", "itertools", "jiff", "jsonwebtoken", "libc", "log",
  "metrics", "moka", "mongodb", "napi",
  "neon", "num_cpus", "once_cell", "opentelemetry", "owo_colors",
  "parking_lot", "pin_project", "pin_project_lite", "poem", "proptest",
  "rand", "rayon", "rcgen", "redis", "regex", "reqwest", "ring", "rocket",
  "rstest", "rustls", "scopeguard", "secrecy", "serde", "serde_json", "serial_test",
  "sha2", "signal_hook", "smallvec", "socket2", "sqlx", "subtle", "syn",
  "tempfile", "thiserror", "time", "tokio", "tokio_rustls", "tokio_stream",
  "tower", "tower_http", "tower_sessions", "tracing", "tracing_subscriber",
  "trybuild", "uuid", "validator", "wasm_bindgen", "wasm_bindgen_futures",
  "walkdir", "web_sys", "windows", "zeroize",
]);

async function walk(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const children = await Promise.all(entries.map(async (entry) => {
    const child = path.join(directory, entry.name);
    if (entry.isDirectory()) return walk(child);
    return /\.mdx?$/.test(entry.name) ? [child] : [];
  }));
  return children.flat().sort();
}

function hasExternalCrate(code) {
  for (const root of externalCrateRoots) {
    const pattern = new RegExp(`(^|[^A-Za-z0-9_])${root}\\s*::`, "m");
    if (pattern.test(code)) return true;
    const externPattern = new RegExp(`\\bextern\\s+crate\\s+${root}\\b`);
    if (externPattern.test(code)) return true;
  }
  return false;
}

function extractSnippets(file, source) {
  const snippets = [];
  const lines = source.split("\n");
  const headings = [];

  for (let index = 0; index < lines.length; index += 1) {
    const heading = lines[index].match(/^(#{1,6})\s+(.+?)\s*$/);
    if (heading) {
      const level = heading[1].length;
      headings.length = level - 1;
      headings[level - 1] = heading[2];
      continue;
    }

    const opening = lines[index].match(/^\s*(`{3,}|~{3,})\s*([^`]*)$/);
    if (!opening) continue;

    const marker = opening[1];
    const info = opening[2].trim();
    const startLine = index + 2;
    const body = [];
    index += 1;
    while (index < lines.length) {
      const closing = lines[index].match(/^\s*(`{3,}|~{3,})\s*$/);
      if (closing && closing[1][0] === marker[0] && closing[1].length >= marker.length) break;
      body.push(lines[index]);
      index += 1;
    }

    if (!/^rust\s+playground(?:\s|$)/.test(info)) continue;
    const section = headings.filter(Boolean).join(" > ");
    const code = body.join("\n");
    const skipReason =
      /\b(?:compile_fail|ignore)\b/.test(info) ? "fence metadata" :
      /Common Pitfalls|Exercises/i.test(section) ? "teaching section" :
      !/\bfn\s+main\s*\(/.test(code) ? "fragment without main" :
      hasExternalCrate(code) ? "external crate" :
      null;

    snippets.push({ file, startLine, section, code, skipReason });
  }

  return snippets;
}

function run(command, args, options = {}) {
  return new Promise((resolve) => {
    const child = spawn(command, args, { cwd: repoRoot, ...options });
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (chunk) => { stdout += chunk; });
    child.stderr?.on("data", (chunk) => { stderr += chunk; });
    child.on("error", (error) => resolve({ code: null, stdout, stderr, error }));
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

const files = await walk(docsRoot);
const allSnippets = (await Promise.all(files.map(async (file) =>
  extractSnippets(file, await readFile(file, "utf8"))))).flat();
const selected = allSnippets.filter((snippet) => !snippet.skipReason);

if (selected.length < minimumCoverage) {
  console.error(`Snippet selection unexpectedly fell to ${selected.length}; expected at least ${minimumCoverage}.`);
  process.exit(1);
}

const toolchain = await run("rustc", ["--version"]);
if (toolchain.code !== 0) {
  console.error(toolchain.stderr || "rustc is unavailable");
  process.exit(1);
}

const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "rs4ts-snippets-"));
const failures = [];

try {
  await Promise.all(selected.map(async (snippet, index) => {
    snippet.sourcePath = path.join(temporaryRoot, `snippet_${index}.rs`);
    snippet.outputPath = path.join(temporaryRoot, `snippet_${index}.rmeta`);
    await writeFile(snippet.sourcePath, `${snippet.code}\n`, "utf8");
  }));

  let nextIndex = 0;
  const workerCount = Math.min(Math.max(os.availableParallelism?.() ?? 2, 1), 8, selected.length);
  const workers = Array.from({ length: workerCount }, async () => {
    while (nextIndex < selected.length) {
      const index = nextIndex;
      nextIndex += 1;
      const snippet = selected[index];
      const result = await run("rustc", [
        "--edition=2024",
        "--crate-name", `rs4ts_snippet_${index}`,
        "--emit=metadata",
        "-Awarnings",
        snippet.sourcePath,
        "-o", snippet.outputPath,
      ]);
      if (result.code !== 0) failures.push({ snippet, result });
    }
  });
  await Promise.all(workers);
} finally {
  await rm(temporaryRoot, { recursive: true, force: true });
}

if (failures.length > 0) {
  failures.sort((a, b) => a.snippet.file.localeCompare(b.snippet.file) || a.snippet.startLine - b.snippet.startLine);
  for (const { snippet, result } of failures.slice(0, 20)) {
    const relative = path.relative(repoRoot, snippet.file);
    console.error(`\n${relative}:${snippet.startLine} (${snippet.section || "no heading"})`);
    console.error(result.stderr.trim());
  }
  if (failures.length > 20) console.error(`\n...and ${failures.length - 20} more failures.`);
  console.error(`\n${failures.length} of ${selected.length} selected snippets failed.`);
  process.exit(1);
}

const skipped = allSnippets.length - selected.length;
console.log(`${selected.length} self-contained Rust 2024 snippets compiled with ${toolchain.stdout.trim()}.`);
console.log(`${skipped} playground blocks were outside scope (fragment, teaching section, or external dependency).`);
