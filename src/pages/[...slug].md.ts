import type { APIRoute, GetStaticPaths } from "astro";
import { getCollection } from "astro:content";

// Per-page Markdown sources for agent content negotiation.
//
// Every docs page is emitted a second time as a sibling `.md` file (e.g.
// `/05-ownership/02-borrowing.md` next to `/05-ownership/02-borrowing/`). The
// content-negotiation Worker (worker/index.js) serves these when a request
// arrives with `Accept: text/markdown`, so agents get clean Markdown while
// browsers keep getting the rendered HTML. The body is the authored Markdown
// source (the truest Markdown form of the page), with the frontmatter title and
// description promoted to a leading H1 + blockquote.

export const getStaticPaths: GetStaticPaths = async () => {
  const docs = await getCollection("docs");
  return docs
    // The splash homepage (`index.mdx`) is a marketing page, not prose content.
    .filter((entry) => entry.id !== "index")
    .map((entry) => ({ params: { slug: entry.id }, props: { entry } }));
};

export const GET: APIRoute = ({ props }) => {
  const { entry } = props as { entry: { data: Record<string, any>; body?: string } };
  const title = entry.data.title ? `# ${entry.data.title}\n\n` : "";
  const description = entry.data.description ? `> ${entry.data.description}\n\n` : "";
  const body = (entry.body ?? "").trim();
  const markdown = `${title}${description}${body}\n`;

  return new Response(markdown, {
    headers: {
      "Content-Type": "text/markdown; charset=utf-8",
    },
  });
};
