import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

async function read(relativePath) {
  return readFile(path.join(repoRoot, relativePath), "utf8");
}

async function markdownFiles(directory = "src/content/docs") {
  const absolute = path.join(repoRoot, directory);
  const entries = await readdir(absolute, { withFileTypes: true });
  const nested = await Promise.all(entries.map(async (entry) => {
    const relative = path.join(directory, entry.name);
    if (entry.isDirectory()) return markdownFiles(relative);
    return /\.mdx?$/.test(entry.name) ? [relative] : [];
  }));
  return nested.flat();
}

test("security examples fail closed and do not log secrets", async () => {
  const jwt = await read("src/content/docs/16-web-apis/13-jwt.md");
  assert.doesNotMatch(jwt, /JWT_SECRET[^\n]*unwrap_or(?:_else)?/);
  assert.doesNotMatch(jwt, /dev-only-secret/);
  assert.match(jwt, /JWT_SECRET must (?:be set to|contain) at least 32 unpredictable bytes/);

  const crypto = await read("src/content/docs/27-security/03-cryptography.md");
  assert.doesNotMatch(crypto, /derived key \(hex\)|hex::encode\(key_bytes\)/);
  assert.doesNotMatch(crypto, /println!\([^\n]*4111 1111 1111 1111/);
  assert.match(crypto, /Never print `key_bytes` or the recovered plaintext/);
});

test("XSS and CSRF examples use safe output and a session synchronizer token", async () => {
  const page = await read("src/content/docs/27-security/02-xss-csrf.md");
  assert.doesNotMatch(page, /Html\(format!\("Accepted:/);
  assert.doesNotMatch(page, /const CSRF_COOKIE|CookieJar/);
  assert.match(page, /SESSION_SECRET must contain at least 32 unpredictable bytes/);
  assert.match(page, /String response is text\/plain/);
  assert.match(page, /SessionManagerLayer::new\(MemoryStore::default\(\)\)/);
  assert.match(page, /session\.get::<String>\(CSRF_KEY\)/);
  assert.match(page, /session\.remove::<String>\(CSRF_KEY\)/);
  assert.match(page, /naïve unsigned cookie\/form variant is vulnerable/);
});

test("foundational claims distinguish warnings, concurrency, and memory safety", async () => {
  const readme = await read("README.md");
  const landing = await read("src/content/docs/index.mdx");
  const whyRust = await read("src/content/docs/01-getting-started/00-why-rust.md");
  const errors = await read("src/content/docs/08-error-handling/index.md");

  assert.doesNotMatch(readme, /no memory leaks/i);
  assert.doesNotMatch(landing, /no event loop required|you can't forget/i);
  assert.doesNotMatch(whyRust, /Processes in parallel across all CPU cores|impossible to forget error handling/);
  assert.doesNotMatch(errors, /compiler refuses to let you ignore them/);
  assert.match(whyRust, /unused_must_use/);
  assert.match(whyRust, /CPU-intensive work[^\n]+Rayon/);
});

test("FFI guidance matches non-unwinding ABI behavior", async () => {
  const ffi = await read("src/content/docs/20-unsafe-ffi/03-ffi-basics.md");
  assert.match(ffi, /Rust panic that reaches that boundary aborts the process/);
  assert.match(ffi, /native foreign exception[^\n]+undefined behavior/);
  assert.doesNotMatch(ffi, /panic!.*inside an `extern "C"` function[^\n]+undefined behavior/);
  assert.doesNotMatch(ffi, /catch_unwind stops any panic[^\n]+UB/);
});

test("learning paths keep time estimates consistent and landing routes purpose-based", async () => {
  const readme = await read("README.md");
  const landing = await read("src/content/docs/index.mdx");
  const howToRead = await read("src/content/docs/00-introduction/01-how-to-read.md");
  const combined = `${readme}\n${landing}\n${howToRead}`;

  assert.match(readme, /Quick \(70–85h\).*Standard \(180–230h\).*Complete \(300–400h\)/s);
  assert.match(landing, /language\/[\s\S]*00–15[\s\S]*applications\/[\s\S]*16–24[\s\S]*shipping\/[\s\S]*25–30/);
  assert.doesNotMatch(landing, /\d+[–-]\d+h|\d+-\d+ hours/);
  assert.match(landing, /default route, not a requirement/);
  assert.match(howToRead, /70-85 hours/);
  assert.match(howToRead, /300-400 hours/);
  assert.doesNotMatch(combined, /20[–-]30h|60[–-]80h|120[–-]150h|20-30 hours|60-80 hours/);
});

test("front-door comparison contains no unsupported headline benchmark table", async () => {
  const whyRust = await read("src/content/docs/01-getting-started/00-why-rust.md");
  const readme = await read("README.md");
  assert.doesNotMatch(whyRust, /HTTP requests\/sec[\s\S]{0,400}300k-1M/);
  assert.doesNotMatch(whyRust, /CPU-bound task \(all cores\)/);
  assert.match(whyRust, /There is no honest universal multiplier/);
  assert.match(whyRust, /noUncheckedIndexedAccess/);
  assert.doesNotMatch(readme, /processData\(data: any\[\]\)/);
});

test("version prose treats Rust 1.96 as a pin, not the latest release", async () => {
  const files = ["README.md", ...(await markdownFiles())];
  const staleClaims = [];
  const stalePattern = /(?:current|latest) stable[^\n]{0,100}(?:Rust )?1\.96|(?:Rust )?1\.96[^\n]{0,100}(?:current|latest) stable/i;

  for (const file of files) {
    const text = await read(file);
    for (const [index, line] of text.split("\n").entries()) {
      if (stalePattern.test(line) && !/not a moving .*latest stable.*claim/i.test(line)) {
        staleClaims.push(`${file}:${index + 1}: ${line.trim()}`);
      }
    }
  }

  assert.deepEqual(staleClaims, []);
  const policy = await read("src/content/docs/00-introduction/05-version-policy.md");
  assert.match(policy, /source of truth is \[`rust-toolchain\.toml`\]/);
  assert.match(policy, /tested baseline, not a claim/);
});

test("session and randomness explanations are internally consistent", async () => {
  const sessions = await read("src/content/docs/16-web-apis/14-sessions.md");
  const crypto = await read("src/content/docs/27-security/03-cryptography.md");
  const passwords = await read("src/content/docs/27-security/04-password-hashing.md");
  const randomness = await read("src/content/docs/27-security/06-secure-randomness.md");

  assert.match(sessions, /\.with_secure\(true\)/);
  assert.match(sessions, /SameSite=Lax; Secure/);
  assert.doesNotMatch(sessions, /flipped to `false` above|With `with_secure\(false\)` so a plain-HTTP/);
  assert.doesNotMatch(sessions, /double-submit CSRF token|double-submit CSRF protection backed by the session/i);

  assert.doesNotMatch(crypto, /default `rand` generator is (?:not suitable|unsuitable)/i);
  assert.doesNotMatch(passwords, /default `rand` thread RNG without `OsRng`/i);
  assert.match(randomness, /ThreadRng` does not automatically reseed after a process fork/);
});
