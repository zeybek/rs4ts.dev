import type { APIRoute } from "astro";
import { getCollection } from "astro:content";

// Markdown view of the homepage for agents (served on `Accept: text/markdown`
// for `/`). The live splash page is component-heavy marketing, so instead of
// its raw body we synthesise a genuinely useful agent landing page: the site
// title/description plus a linked table of contents of every chapter, built
// from the docs collection. Content pages serve their own Markdown via
// src/pages/[...slug].md.ts.

const SITE_TITLE = "Rust for TypeScript & JavaScript Developers";
const SITE_DESC =
  "Learn Rust by mapping every concept to the TypeScript and JavaScript you already know — side by side, chapter by chapter. Free and open source.";

/** "02-basics" -> "Basics", "16-web-apis" -> "Web Apis". */
function chapterName(dir: string): string {
  return dir
    .replace(/^\d+-/, "")
    .split("-")
    .map((w) => (w ? w[0].toUpperCase() + w.slice(1) : w))
    .join(" ");
}

export const GET: APIRoute = async () => {
  const docs = await getCollection("docs");
  const groups = new Map<string, { id: string; title: string }[]>();
  for (const entry of docs) {
    if (entry.id === "index" || !entry.id.includes("/")) continue;
    const dir = entry.id.split("/")[0];
    if (!groups.has(dir)) groups.set(dir, []);
    groups.get(dir)!.push({ id: entry.id, title: entry.data.title });
  }

  let toc = "";
  for (const dir of [...groups.keys()].sort()) {
    toc += `\n### ${chapterName(dir)}\n\n`;
    for (const item of groups.get(dir)!.sort((a, b) => a.id.localeCompare(b.id))) {
      toc += `- [${item.title}](/${item.id}/)\n`;
    }
  }

  const md =
    `# ${SITE_TITLE}\n\n` +
    `> ${SITE_DESC}\n\n` +
    `This is the Markdown view of the site for agents. Request any page with ` +
    `\`Accept: text/markdown\` to receive its Markdown source. The whole guide ` +
    `is also available as [/llms.txt](/llms.txt) and [/llms-full.txt](/llms-full.txt).\n\n` +
    `## Contents\n${toc}`;

  return new Response(md, {
    headers: { "Content-Type": "text/markdown; charset=utf-8" },
  });
};
