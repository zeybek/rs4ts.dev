import { execSync } from 'node:child_process'

const DOCS = 'src/content/docs/'

function fileToSlug(file) {
	let rel = file.startsWith(DOCS) ? file.slice(DOCS.length) : file
	rel = rel.replace(/\.(md|mdx)$/, '')
	if (rel === 'index') return ''
	if (rel.endsWith('/index')) return rel.slice(0, -'/index'.length)
	return rel
}

function buildDates() {
	const modified = new Map()
	const published = new Map()
	let out = ''
	try {
		out = execSync(`git log --no-merges --format=@@@%cI --name-only -- ${DOCS}`, {
			encoding: 'utf8',
			maxBuffer: 256 * 1024 * 1024,
		})
	} catch {
		return { modified, published }
	}
	let date = null
	for (const line of out.split('\n')) {
		if (line.startsWith('@@@')) {
			date = line.slice(3).trim() || null
			continue
		}
		const file = line.trim()
		if (!file || !date || (!file.endsWith('.md') && !file.endsWith('.mdx'))) continue
		const slug = fileToSlug(file)
		if (!modified.has(slug)) modified.set(slug, date)
		published.set(slug, date)
	}
	return { modified, published }
}

const { modified, published } = buildDates()

export function pathnameToSlug(pathname) {
	return pathname.replace(/^\/+|\/+$/g, '')
}

export function modifiedForSlug(slug) {
	return modified.get(slug)
}
export function publishedForSlug(slug) {
	return published.get(slug)
}
export function modifiedForPathname(pathname) {
	return modified.get(pathnameToSlug(pathname))
}
