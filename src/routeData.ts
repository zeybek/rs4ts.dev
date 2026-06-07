import { defineRouteMiddleware } from '@astrojs/starlight/route-data'
import {
	SITE_URL,
	buildArticleJsonLd,
	buildBreadcrumbJsonLd,
	buildSiteRootGraph,
	type Crumb,
} from './lib/site-meta'
import { publishedForSlug } from './lib/git-dates.mjs'

function ldScript(json: unknown) {
	return {
		tag: 'script' as const,
		attrs: { type: 'application/ld+json' },
		content: JSON.stringify(json),
	}
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

	head.push({ tag: 'meta', attrs: { property: 'og:image', content: ogImage } })
	head.push({ tag: 'meta', attrs: { property: 'og:image:width', content: '1200' } })
	head.push({ tag: 'meta', attrs: { property: 'og:image:height', content: '630' } })
	head.push({ tag: 'meta', attrs: { name: 'twitter:image', content: ogImage } })

	if (slug === '') {
		head.push(ldScript(buildSiteRootGraph()))
		return
	}

	const dateModified =
		route.lastUpdated instanceof Date ? route.lastUpdated.toISOString() : undefined
	const datePublished = publishedForSlug(slug)

	head.push(
		ldScript(
			buildArticleJsonLd({
				title: route.entry.data.title,
				description: route.entry.data.description,
				url: canonical,
				image: ogImage,
				datePublished,
				dateModified,
			})
		)
	)

	const segments = slug.split('/').filter(Boolean)
	const crumbs: Crumb[] = [{ name: 'Home', item: new URL('/', site).href }]
	if (segments[0]) {
		crumbs.push({
			name: sectionLabel(route.sidebar) ?? titleCaseSlug(segments[0]),
			item: new URL(`/${segments[0]}/`, site).href,
		})
	}
	if (segments.length >= 2) {
		crumbs.push({ name: route.entry.data.title, item: canonical })
	}
	if (crumbs.length >= 2) head.push(ldScript(buildBreadcrumbJsonLd(crumbs)))
})
