import contributorsData from '../data/contributors.json' with { type: 'json' }

const contributors = new Map()
for (const [slug, entry] of Object.entries(contributorsData)) {
	contributors.set(slug, Array.isArray(entry?.contributors) ? entry.contributors : [])
}

function pathnameToSlug(pathname) {
	return pathname.replace(/^\/+|\/+$/g, '')
}

export function contributorsForSlug(slug) {
	return contributors.get(slug) || []
}
export function contributorsForPathname(pathname) {
	return contributorsForSlug(pathnameToSlug(pathname))
}
