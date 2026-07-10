import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

async function read(relativePath) {
  return readFile(path.join(repoRoot, relativePath), "utf8");
}

function rustBlockContaining(source, needle) {
  const blocks = source.matchAll(/^```rust[^\n]*\n([\s\S]*?)^```\s*$/gm);
  for (const match of blocks) {
    if (match[1].includes(needle)) return `${match[1].trim()}\n`;
  }
  throw new Error(`Could not find Rust block containing ${JSON.stringify(needle)}`);
}

function run(command, args, cwd) {
  return new Promise((resolve) => {
    const child = spawn(command, args, { cwd, env: process.env });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", (error) => resolve({ code: null, stdout, stderr, error }));
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

const [jwtPage, xssPage, cryptoPage, toolchainFile] = await Promise.all([
  read("src/content/docs/16-web-apis/13-jwt.md"),
  read("src/content/docs/27-security/02-xss-csrf.md"),
  read("src/content/docs/27-security/03-cryptography.md"),
  read("rust-toolchain.toml"),
]);

const channel = toolchainFile.match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
if (!channel) throw new Error("rust-toolchain.toml does not declare a channel");

const bins = new Map([
  ["jwt", rustBlockContaining(jwtPage, "struct AdminUser(Claims)")],
  ["csrf", rustBlockContaining(xssPage, "async fn submit(session: Session")],
  ["guestbook", rustBlockContaining(xssPage, "struct GuestbookTemplate")],
  ["crypto", rustBlockContaining(cryptoPage, "fn derive_key(master:")],
]);

const cargoToml = `[package]
name = "rs4ts-security-snippet-check"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
aes-gcm = "0.10.3"
askama = "0.16"
axum = "0.8"
base64 = "0.22"
getrandom = "0.3"
hkdf = "0.13"
jsonwebtoken = { version = "10", features = ["rust_crypto"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.11"
subtle = "2.6"
thiserror = "2"
tokio = { version = "1", features = ["full"] }
tower-sessions = "0.14"
`;

const guestbookTemplate = `<!doctype html>
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
`;

const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "rs4ts-security-snippets-"));
try {
  await mkdir(path.join(temporaryRoot, "src/bin"), { recursive: true });
  await mkdir(path.join(temporaryRoot, "templates"), { recursive: true });
  await Promise.all([
    writeFile(path.join(temporaryRoot, "Cargo.toml"), cargoToml, "utf8"),
    writeFile(path.join(temporaryRoot, "templates/guestbook.html"), guestbookTemplate, "utf8"),
    ...[...bins].map(([name, code]) =>
      writeFile(path.join(temporaryRoot, `src/bin/${name}.rs`), code, "utf8")),
  ]);

  const result = await run("cargo", [`+${channel}`, "check", "--bins"], temporaryRoot);
  if (result.code !== 0) {
    console.error(result.stdout);
    console.error(result.stderr);
    process.exitCode = 1;
  } else {
    const cryptoRun = await run("cargo", [`+${channel}`, "run", "--quiet", "--bin", "crypto"], temporaryRoot);
    if (cryptoRun.code !== 0) {
      console.error(cryptoRun.stdout);
      console.error(cryptoRun.stderr);
      process.exitCode = 1;
    } else {
      const expected = [
        "stored blob: 47 bytes (12 nonce + 19 plaintext + 16 tag)",
        "open (right user) -> true",
        "open (wrong user) -> None",
      ];
      const output = cryptoRun.stdout.trim().split("\n");
      if (JSON.stringify(output) !== JSON.stringify(expected)) {
        console.error(`Unexpected cryptography output:\n${cryptoRun.stdout}`);
        process.exitCode = 1;
      } else {
        console.log(`Compiled ${bins.size} security-sensitive documentation programs with Rust ${channel}.`);
        console.log("Executed the cryptography example without logging key or plaintext material.");
      }
    }
  }
} finally {
  await rm(temporaryRoot, { recursive: true, force: true });
}
