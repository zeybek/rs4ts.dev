import type {
	WithContext,
	TechArticle,
	BreadcrumbList,
	Person,
	Organization,
	Course,
	WebSite,
	Graph,
} from 'schema-dts'

export const SITE_URL = 'https://rs4ts.dev'
export const SITE_NAME = 'Rust for TS/JS Developers'

export const PERSON_ID = `${SITE_URL}/#person`
export const ORG_ID = `${SITE_URL}/#org`
export const COURSE_ID = `${SITE_URL}/#course`
export const WEBSITE_ID = `${SITE_URL}/#website`

export const author: Person = {
	'@type': 'Person',
	'@id': PERSON_ID,
	name: 'Ahmet Zeybek',
	url: 'https://zeybek.dev',
	sameAs: ['https://github.com/zeybek'],
	knowsAbout: ['Rust', 'TypeScript', 'JavaScript', 'WebAssembly', 'Systems programming'],
}

export const organization: Organization = {
	'@type': 'Organization',
	'@id': ORG_ID,
	name: SITE_NAME,
	url: SITE_URL,
	logo: `${SITE_URL}/favicon.svg`,
	sameAs: ['https://github.com/zeybek/rs4ts.dev'],
}

export const course: Course = {
	'@type': 'Course',
	'@id': COURSE_ID,
	name: 'Rust for TypeScript & JavaScript Developers',
	description:
		'A free, side-by-side guide that teaches Rust by mapping every concept to the TypeScript and JavaScript you already know.',
	url: SITE_URL,
	provider: { '@id': ORG_ID },
	inLanguage: 'en',
	isAccessibleForFree: true,
	teaches: 'The Rust programming language for developers coming from TypeScript and JavaScript',
	about: 'Rust (programming language)',
	educationalLevel: 'Intermediate',
}

const website: WebSite = {
	'@type': 'WebSite',
	'@id': WEBSITE_ID,
	name: SITE_NAME,
	url: SITE_URL,
	inLanguage: 'en',
	publisher: { '@id': ORG_ID },

}

export function buildArticleJsonLd(opts: {
	title: string
	description?: string
	url: string
	image: string
	datePublished?: string
	dateModified?: string
}): WithContext<TechArticle> {
	return {
		'@context': 'https://schema.org',
		'@type': 'TechArticle',
		headline: opts.title,
		...(opts.description ? { description: opts.description } : {}),
		url: opts.url,
		image: opts.image,
		inLanguage: 'en',
		...(opts.datePublished ? { datePublished: opts.datePublished } : {}),
		...(opts.dateModified ? { dateModified: opts.dateModified } : {}),
		author: { '@id': PERSON_ID },
		publisher: { '@id': ORG_ID },
		isPartOf: { '@id': COURSE_ID },
	}
}

export type Crumb = { name: string; item: string }

export function buildBreadcrumbJsonLd(crumbs: Crumb[]): WithContext<BreadcrumbList> {
	return {
		'@context': 'https://schema.org',
		'@type': 'BreadcrumbList',
		itemListElement: crumbs.map((c, i) => ({
			'@type': 'ListItem',
			position: i + 1,
			name: c.name,
			item: c.item,
		})),
	}
}

export function buildSiteRootGraph(): { '@context': 'https://schema.org'; '@graph': Graph['@graph'] } {
	return {
		'@context': 'https://schema.org',
		'@graph': [website, organization, author, course],
	}
}
