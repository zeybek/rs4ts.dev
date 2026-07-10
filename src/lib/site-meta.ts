import type {
	WithContext,
	TechArticle,
	BreadcrumbList,
	FAQPage,
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
	// A free, self-paced online course. hasCourseInstance (with courseMode +
	// courseWorkload) and offers are what make a Course eligible for Google's
	// Course rich result; the price:0 Offer marks it explicitly free.
	hasCourseInstance: {
		'@type': 'CourseInstance',
		courseMode: 'online',
		courseWorkload: 'PT80H',
	},
	offers: {
		'@type': 'Offer',
		category: 'Free',
		price: 0,
		priceCurrency: 'USD',
		availability: 'https://schema.org/InStock',
	},
}

const website: WebSite = {
	'@type': 'WebSite',
	'@id': WEBSITE_ID,
	name: SITE_NAME,
	url: SITE_URL,
	inLanguage: 'en',
	publisher: { '@id': ORG_ID },

}

// A content page's JSON-LD. The TechArticle references author/publisher/isPartOf
// by @id, so the Person/Organization/Course nodes those ids point at MUST travel
// in the same @graph — otherwise the references dangle and Google sees an Article
// with no resolvable publisher/author (breaking rich-result eligibility). The
// full entity graph lives only on the landing page (buildSiteRootGraph), so we
// re-emit the referenced nodes here per page.
export function buildArticleGraph(opts: {
	title: string
	description?: string
	url: string
	image: string
	section?: string
	datePublished?: string
	dateModified?: string
}): { '@context': 'https://schema.org'; '@graph': Graph['@graph'] } {
	const article: TechArticle = {
		'@type': 'TechArticle',
		'@id': `${opts.url}#article`,
		headline: opts.title,
		...(opts.description ? { description: opts.description } : {}),
		url: opts.url,
		mainEntityOfPage: opts.url,
		image: opts.image,
		inLanguage: 'en',
		...(opts.section ? { articleSection: opts.section } : {}),
		...(opts.datePublished ? { datePublished: opts.datePublished } : {}),
		...(opts.dateModified ? { dateModified: opts.dateModified } : {}),
		author: { '@id': PERSON_ID },
		publisher: { '@id': ORG_ID },
		isPartOf: { '@id': COURSE_ID },
	}
	return {
		'@context': 'https://schema.org',
		'@graph': [article, organization, author, course],
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

// Strip the small subset of Markdown the FAQ answers use down to the plain text
// Google expects in an Answer's `text` (it must match the visible answer).
function mdToText(s: string): string {
	return s
		.replace(/\[([^\]]+)\]\([^)]*\)/g, '$1') // [label](url) -> label
		.replace(/`([^`]+)`/g, '$1') // `code` -> code
		.replace(/\*\*([^*]+)\*\*/g, '$1') // **bold** -> bold
		.replace(/__([^_]+)__/g, '$1') // __bold__ -> bold
		.replace(/(?<!\*)\*([^*]+)\*(?!\*)/g, '$1') // *italic* -> italic
		.replace(/\s+/g, ' ')
		.trim()
}

// Parse the FAQ page body (`## Question?` heading followed by a prose answer,
// sections split by `---`) into Q&A pairs. The trailing "See …" cross-link
// paragraph is navigation, not part of the answer, so it is dropped.
function parseFaqItems(body: string): { question: string; answer: string }[] {
	const items: { question: string; answer: string }[] = []
	let question: string | null = null
	let buffer: string[] = []

	const flush = () => {
		if (question) {
			const paragraphs = buffer
				.join('\n')
				.split(/\n\s*\n/)
				.map((p) => p.trim())
				.filter((p) => p && !/^see\s/i.test(p))
			const answer = mdToText(paragraphs.join(' '))
			if (answer) items.push({ question, answer })
		}
		question = null
		buffer = []
	}

	for (const raw of body.split('\n')) {
		const heading = /^##\s+(.+?)\s*$/.exec(raw)
		if (heading) {
			flush()
			const text = heading[1].trim()
			question = text.endsWith('?') ? mdToText(text) : null
			continue
		}
		if (/^---\s*$/.test(raw)) {
			flush()
			continue
		}
		if (question) buffer.push(raw)
	}
	flush()
	return items
}

// FAQPage schema for the FAQ chapter — eligible for FAQ rich results and a
// strong, directly-extractable Q&A source for answer engines (GEO). Returns
// null when no Q&A pairs are found so the caller can skip emitting the script.
export function buildFaqJsonLd(opts: { body: string; url: string }): WithContext<FAQPage> | null {
	const items = parseFaqItems(opts.body)
	if (items.length === 0) return null
	return {
		'@context': 'https://schema.org',
		'@type': 'FAQPage',
		url: opts.url,
		inLanguage: 'en',
		isPartOf: { '@id': COURSE_ID },
		mainEntity: items.map((item) => ({
			'@type': 'Question',
			name: item.question,
			acceptedAnswer: { '@type': 'Answer', text: item.answer },
		})),
	}
}
