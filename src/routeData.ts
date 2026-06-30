import { readFileSync } from 'node:fs'
import { defineRouteMiddleware } from '@astrojs/starlight/route-data'
import {
	SITE_URL,
	buildArticleGraph,
	buildBreadcrumbJsonLd,
	buildFaqJsonLd,
	buildSiteRootGraph,
	type Crumb,
} from './lib/site-meta'
import { publishedForSlug } from './lib/git-dates.mjs'
import { buildSeoDescription, buildSeoTitle } from './lib/seo-title'

// Slug of the FAQ chapter — the one page that also gets FAQPage JSON-LD.
const FAQ_SLUG = '00-introduction/04-faq'

// Raw markdown body for a docs slug: prefer the loaded entry body, fall back to
// reading the source file (cwd is the project root during the build).
function docBody(slug: string, entry: unknown): string | undefined {
	const fromEntry = (entry as { body?: unknown })?.body
	if (typeof fromEntry === 'string' && fromEntry.trim()) return fromEntry
	try {
		return readFileSync(`src/content/docs/${slug}.md`, 'utf8').replace(/^---[\s\S]*?\n---\n?/, '')
	} catch {
		return undefined
	}
}

function ldScript(json: unknown) {
	return {
		tag: 'script' as const,
		attrs: { type: 'application/ld+json' },
		content: JSON.stringify(json),
	}
}

function upsertMeta(
	head: any[],
	match: (tag: any) => boolean,
	entry: { tag: 'meta'; attrs: Record<string, string> }
) {
	const existing = head.find(match)
	if (existing) existing.attrs = { ...(existing.attrs ?? {}), ...entry.attrs }
	else head.push(entry)
}

function escapeHtmlText(value: string): string {
	return value
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
}

function applySeoTitle(head: any[], seoTitle: string) {
	const title = head.find((tag) => tag.tag === 'title')
	const escapedTitle = escapeHtmlText(seoTitle)
	if (title) title.content = escapedTitle
	else head.push({ tag: 'title', content: escapedTitle })

	upsertMeta(
		head,
		(tag) => tag.tag === 'meta' && tag.attrs?.property === 'og:title',
		{ tag: 'meta', attrs: { property: 'og:title', content: seoTitle } }
	)
	upsertMeta(
		head,
		(tag) => tag.tag === 'meta' && tag.attrs?.name === 'twitter:title',
		{ tag: 'meta', attrs: { name: 'twitter:title', content: seoTitle } }
	)
}

function applySeoDescription(head: any[], seoDescription: string) {
	upsertMeta(
		head,
		(tag) => tag.tag === 'meta' && tag.attrs?.name === 'description',
		{ tag: 'meta', attrs: { name: 'description', content: seoDescription } }
	)
	upsertMeta(
		head,
		(tag) => tag.tag === 'meta' && tag.attrs?.property === 'og:description',
		{ tag: 'meta', attrs: { property: 'og:description', content: seoDescription } }
	)
	upsertMeta(
		head,
		(tag) => tag.tag === 'meta' && tag.attrs?.name === 'twitter:description',
		{ tag: 'meta', attrs: { name: 'twitter:description', content: seoDescription } }
	)
}

function hasCurrent(entry: any): boolean {
	if (entry.type === 'link') return entry.isCurrent === true
	if (entry.type === 'group') return entry.entries.some(hasCurrent)
	return false
}
function sectionLabel(sidebar: any[]): string | undefined {
	for (const entry of sidebar) {
		if (entry.type === 'group' && hasCurrent(entry)) return entry.label
	}
	return undefined
}

function titleCaseSlug(seg: string): string {
	return seg
		.replace(/^\d+-/, '')
		.split('-')
		.map((w) => w.charAt(0).toUpperCase() + w.slice(1))
		.join(' ')
}

export const onRequest = defineRouteMiddleware((context) => {
	const route = context.locals.starlightRoute
	const site = context.site ?? new URL(SITE_URL)
	const slug = route.id
	const head = route.head

	const canonical = new URL(context.url.pathname, site).href
	const ogImage = new URL(`/og/${slug || 'index'}.png`, site).href
	const section = slug ? sectionLabel(route.sidebar) : undefined
	const seoTitle = buildSeoTitle({
		slug,
		title: route.entry.data.title,
		seoTitle: (route.entry.data as { seoTitle?: string }).seoTitle,
		section,
	})
	const seoDescription = buildSeoDescription({
		slug,
		title: route.entry.data.title,
		description: route.entry.data.description,
		seoDescription: (route.entry.data as { seoDescription?: string }).seoDescription,
		section,
	})
	applySeoTitle(head, seoTitle)
	applySeoDescription(head, seoDescription)

	const imageAlt = route.entry.data.title
	head.push({ tag: 'meta', attrs: { property: 'og:image', content: ogImage } })
	head.push({ tag: 'meta', attrs: { property: 'og:image:width', content: '1200' } })
	head.push({ tag: 'meta', attrs: { property: 'og:image:height', content: '630' } })
	head.push({ tag: 'meta', attrs: { property: 'og:image:alt', content: imageAlt } })
	head.push({ tag: 'meta', attrs: { name: 'twitter:image', content: ogImage } })
	head.push({ tag: 'meta', attrs: { name: 'twitter:image:alt', content: imageAlt } })

	if (slug === '') {
		// The landing page is a site front door, not an article. Starlight defaults
		// og:type to "article" everywhere; correct it in place (avoid a duplicate tag).
		const ogType = head.find(
			(t) => t.tag === 'meta' && (t.attrs as Record<string, unknown>).property === 'og:type'
		)
		if (ogType) (ogType.attrs as Record<string, unknown>).content = 'website'
		head.push(ldScript(buildSiteRootGraph()))
		return
	}

	const dateModified =
		route.lastUpdated instanceof Date ? route.lastUpdated.toISOString() : undefined
	const datePublished = publishedForSlug(slug)

	head.push(
		ldScript(
			buildArticleGraph({
				title: seoTitle,
				description: seoDescription,
				url: canonical,
				image: ogImage,
				section,
				datePublished,
				dateModified,
			})
		)
	)

	if (slug === FAQ_SLUG) {
		const body = docBody(slug, route.entry)
		if (body) {
			const faq = buildFaqJsonLd({ body, url: canonical })
			if (faq) head.push(ldScript(faq))
		}
	}

	const segments = slug.split('/').filter(Boolean)
	const crumbs: Crumb[] = [{ name: 'Home', item: new URL('/', site).href }]
	if (segments[0]) {
		crumbs.push({
			name: section ?? titleCaseSlug(segments[0]),
			item: new URL(`/${segments[0]}/`, site).href,
		})
	}
	if (segments.length >= 2) {
		crumbs.push({ name: route.entry.data.title, item: canonical })
	}
	if (crumbs.length >= 2) head.push(ldScript(buildBreadcrumbJsonLd(crumbs)))
})
