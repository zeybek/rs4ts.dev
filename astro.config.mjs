// @ts-check

import starlight from "@astrojs/starlight";
import sitemap from "@astrojs/sitemap";
import { unified } from "@astrojs/markdown-remark";
import { defineConfig } from "astro/config";
import starlightLinksValidator from "starlight-links-validator";
import starlightLlmsTxt from "starlight-llms-txt";
import starlightScrollToTop from "starlight-scroll-to-top";
import starlightHeadingBadges from "starlight-heading-badges";
import starlightImageZoom from "starlight-image-zoom";
import { modifiedForPathname } from "./src/lib/git-dates.mjs";

// `site` is the final public canonical URL (used for canonical <link> tags,
// the sitemap, OG image URLs, and llms.txt). Deployed to Cloudflare Workers
// (static assets) and served from this custom domain.
export default defineConfig({
	site: "https://rs4ts.dev",
	markdown: {
		processor: unified(),
	},
	integrations: [
		starlight({
			title: "Rust for TS/JS Developers",
			description:
				"Learn Rust by mapping every concept to the TypeScript and JavaScript you already know — side-by-side, chapter by chapter. Free and open source.",
			social: [
				{
					icon: "github",
					label: "GitHub",
					href: "https://github.com/zeybek/rs4ts.dev",
				},
			],
			editLink: {
				baseUrl: "https://github.com/zeybek/rs4ts.dev/edit/main/",
			},
			lastUpdated: true,
			// Inject SEO/OG/JSON-LD <head> tags the idiomatic way (overriding the
			// Head component is a Starlight "last resort"). See src/routeData.ts.
			routeMiddleware: "./src/routeData.ts",
			// Self-host the exact weights used by the theme. Vite fingerprints these
			// assets, so visitors do not need to contact a third-party font service.
			customCss: [
				"@fontsource/ibm-plex-mono/latin-400.css",
				"@fontsource/ibm-plex-mono/latin-500.css",
				"@fontsource/ibm-plex-mono/latin-600.css",
				"@fontsource/ibm-plex-sans/latin-400.css",
				"@fontsource/ibm-plex-sans/latin-500.css",
				"@fontsource/ibm-plex-sans/latin-600.css",
				"@fontsource/ibm-plex-sans/latin-700.css",
				"./src/styles/custom.css",
			],
			components: {
				// Landing page renders a bespoke glassy terminal nav; every other
				// route keeps the upstream header (delegated to Default inside).
				Header: "./src/components/Header.astro",
				// Drop Starlight's `.main-frame` / `.lg:sl-flex` layout wrappers so the
				// chapter nav, content pane and "On this page" TOC become same-level
				// children of `.page` — laid out as one CSS grid in custom.css (≥72rem).
				PageFrame: "./src/components/PageFrame.astro",
				TwoColumnContent: "./src/components/TwoColumnContent.astro",
				// Header right group + mobile menu footer: configured socials plus a
				// Rust-edition trust badge and a link to the generated /llms.txt.
				SocialIcons: "./src/components/SocialIcons.astro",
				Head: "./src/components/Head.astro",
				// Re-render the default title; append reading time + a "last updated
				// by" avatar (git author) next to it.
				PageTitle: "./src/components/PageTitle.astro",
				// Default to dark (the dominant terminal look) on first visit.
				ThemeProvider: "./src/components/ThemeProvider.astro",
				// Custom numbered "chapter index" navigation.
				Sidebar: "./src/components/Sidebar.astro",
				// Prev/next titles render `backtick` spans as inline-code chips.
				Pagination: "./src/components/Pagination.astro",
				// Append a per-page "Contributors" avatar list (git authors) above
				// the default last-updated / edit-link / pagination footer.
				Footer: "./src/components/Footer.astro",
			},
			plugins: [
				// Fail the build on any broken internal link or anchor.
				// Localhost URLs are instructional ("open … in your browser"), not site links.
				starlightLinksValidator({
					exclude: ["http://127.0.0.1*", "http://localhost*"],
				}),
				// Generate /llms.txt and /llms-full.txt so AI tools can consume the whole guide.
				starlightLlmsTxt(),
				starlightScrollToTop(),
				// Badges on headings AND in the table of contents, e.g.
				// `## Lifetimes :badge[Critical]{variant="caution"}`.
				starlightHeadingBadges(),
				starlightImageZoom(),
			],
			sidebar: [
				{
					label: "Introduction",
					collapsed: true,
					badge: { text: "Start here", variant: "tip" },
					items: [{ autogenerate: { directory: "00-introduction" } }],
				},
				{
					label: "Getting Started",
					collapsed: true,
					items: [{ autogenerate: { directory: "01-getting-started" } }],
				},
				{
					label: "Basics",
					collapsed: true,
					items: [{ autogenerate: { directory: "02-basics" } }],
				},
				{
					label: "Functions",
					collapsed: true,
					items: [{ autogenerate: { directory: "03-functions" } }],
				},
				{
					label: "Control Flow",
					collapsed: true,
					items: [{ autogenerate: { directory: "04-control-flow" } }],
				},
				{
					label: "Ownership",
					collapsed: true,
					badge: { text: "Critical", variant: "caution" },
					items: [{ autogenerate: { directory: "05-ownership" } }],
				},
				{
					label: "Data Structures",
					collapsed: true,
					items: [{ autogenerate: { directory: "06-data-structures" } }],
				},
				{
					label: "Collections",
					collapsed: true,
					items: [{ autogenerate: { directory: "07-collections" } }],
				},
				{
					label: "Error Handling",
					collapsed: true,
					items: [{ autogenerate: { directory: "08-error-handling" } }],
				},
				{
					label: "Generics & Traits",
					collapsed: true,
					items: [{ autogenerate: { directory: "09-generics-traits" } }],
				},
				{
					label: "Smart Pointers",
					collapsed: true,
					items: [{ autogenerate: { directory: "10-smart-pointers" } }],
				},
				{
					label: "Async",
					collapsed: true,
					items: [{ autogenerate: { directory: "11-async" } }],
				},
				{
					label: "Modules & Packages",
					collapsed: true,
					items: [{ autogenerate: { directory: "12-modules-packages" } }],
				},
				{
					label: "Testing",
					collapsed: true,
					items: [{ autogenerate: { directory: "13-testing" } }],
				},
				{
					label: "Macros",
					collapsed: true,
					items: [{ autogenerate: { directory: "14-macros" } }],
				},
				{
					label: "Serialization",
					collapsed: true,
					items: [{ autogenerate: { directory: "15-serialization" } }],
				},
				{
					label: "Web APIs",
					collapsed: true,
					items: [{ autogenerate: { directory: "16-web-apis" } }],
				},
				{
					label: "Databases",
					collapsed: true,
					items: [{ autogenerate: { directory: "17-database" } }],
				},
				{
					label: "CLI Tools",
					collapsed: true,
					items: [{ autogenerate: { directory: "18-cli-tools" } }],
				},
				{
					label: "WebAssembly",
					collapsed: true,
					items: [{ autogenerate: { directory: "19-wasm" } }],
				},
				{
					label: "Unsafe & FFI",
					collapsed: true,
					items: [{ autogenerate: { directory: "20-unsafe-ffi" } }],
				},
				{
					label: "Performance",
					collapsed: true,
					items: [{ autogenerate: { directory: "21-performance" } }],
				},
				{
					label: "Common Patterns",
					collapsed: true,
					items: [{ autogenerate: { directory: "22-common-patterns" } }],
				},
				{
					label: "Ecosystem",
					collapsed: true,
					items: [{ autogenerate: { directory: "23-ecosystem" } }],
				},
				{
					label: "Tooling",
					collapsed: true,
					items: [{ autogenerate: { directory: "24-tooling" } }],
				},
				{
					label: "Advanced Topics",
					collapsed: true,
					items: [{ autogenerate: { directory: "25-advanced-topics" } }],
				},
				{
					label: "Systems Programming",
					collapsed: true,
					items: [{ autogenerate: { directory: "26-systems-programming" } }],
				},
				{
					label: "Security",
					collapsed: true,
					items: [{ autogenerate: { directory: "27-security" } }],
				},
				{
					label: "Production",
					collapsed: true,
					items: [{ autogenerate: { directory: "28-production" } }],
				},
				{
					label: "Migration Guide",
					collapsed: true,
					items: [{ autogenerate: { directory: "29-migration-guide" } }],
				},
				{
					label: "Projects",
					collapsed: true,
					items: [{ autogenerate: { directory: "30-projects" } }],
				},
			],
		}),
		// Adding @astrojs/sitemap ourselves makes Starlight skip its bundled copy
		// (no duplicate), letting us stamp each URL with an accurate <lastmod> from
		// the page's last git commit — the only sitemap metadata Google trusts.
		// priority/changefreq are intentionally omitted (Google ignores them).
		sitemap({
			serialize(item) {
				const lastmod = modifiedForPathname(new URL(item.url).pathname);
				if (lastmod) item.lastmod = lastmod;
				return item;
			},
		}),
	],
});
