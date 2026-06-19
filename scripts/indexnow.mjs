// Ping IndexNow after a deploy so Bing / Copilot / DuckDuckGo (and the rest of
// the IndexNow consortium) recrawl the pages that just changed. Google does NOT
// use IndexNow, so this is purely the Bing-family fast path — it complements,
// not replaces, Search Console. Best-effort: this never fails the deploy.
//
// The key is read from the public/<key>.txt file (the IndexNow ownership proof)
// so the key and its published location can never drift out of sync. Only the
// docs URLs touched by the last commit are submitted (same source of truth as
// the sitemap's <lastmod>), to avoid spamming the endpoint with the whole site.

import { execSync } from 'node:child_process'
import { readFileSync, readdirSync } from 'node:fs'

const SITE_URL = 'https://rs4ts.dev'
const HOST = 'rs4ts.dev'
const DOCS = 'src/content/docs/'
const ENDPOINT = 'https://api.indexnow.org/indexnow'

function findKey() {
	// The IndexNow key file is a hex-named .txt at the site root (public/).
	// robots.txt and friends are not hex, so they won't match.
	const file = readdirSync('public').find((f) => /^[a-f0-9]{8,128}\.txt$/.test(f))
	if (!file) return null
	const key = file.replace(/\.txt$/, '')
	const contents = readFileSync(`public/${file}`, 'utf8').trim()
	if (contents !== key) {
		console.warn(`[indexnow] ${file} contents do not match its name — skipping.`)
		return null
	}
	return { key, keyLocation: `${SITE_URL}/${file}` }
}

function fileToUrl(file) {
	if (!/\.(md|mdx)$/.test(file)) return null
	let rel = file.startsWith(DOCS) ? file.slice(DOCS.length) : file
	rel = rel.replace(/\.(md|mdx)$/, '')
	if (rel === 'index') return `${SITE_URL}/`
	if (rel.endsWith('/index')) rel = rel.slice(0, -'/index'.length)
	return `${SITE_URL}/${rel}/`
}

function changedUrls() {
	let out = ''
	try {
		// ACMR = added/copied/modified/renamed — exclude deletions (they 404).
		out = execSync(`git diff --name-only --diff-filter=ACMR HEAD~1 HEAD -- ${DOCS}`, {
			encoding: 'utf8',
		})
	} catch {
		return [] // shallow clone or initial commit — nothing to diff against.
	}
	const urls = new Set()
	for (const line of out.split('\n')) {
		const url = fileToUrl(line.trim())
		if (url) urls.add(url)
	}
	return [...urls]
}

async function main() {
	const id = findKey()
	if (!id) {
		console.log('[indexnow] no key file in public/ — skipping ping.')
		return
	}
	const urlList = changedUrls()
	if (urlList.length === 0) {
		console.log('[indexnow] no changed docs in the last commit — nothing to submit.')
		return
	}
	const payload = { host: HOST, key: id.key, keyLocation: id.keyLocation, urlList }
	try {
		const res = await fetch(ENDPOINT, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json; charset=utf-8' },
			body: JSON.stringify(payload),
		})
		// 200 = accepted, 202 = accepted/validation pending. Either is success.
		console.log(`[indexnow] submitted ${urlList.length} URL(s) → HTTP ${res.status}`)
	} catch (err) {
		console.warn(`[indexnow] ping failed (ignored): ${err?.message ?? err}`)
	}
}

await main()
