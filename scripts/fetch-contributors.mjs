// Build-time contributor resolver.
//
// For every doc file we combine two sources, neither of which needs a
// hand-maintained mapping:
//   - GitHub commits API → who each commit's email belongs to (login + avatar +
//     profile). GitHub resolves email→account server-side. (VitePress data
//     loader / all-contributors pattern.)
//   - local `git log --numstat` → how much each author contributed to that file
//     (commit count + lines added/removed), plus recency order.
//
// The merged result is written to src/data/contributors.json, which the site
// reads synchronously at build time, so the deploy itself needs no token or
// network. Each contributor entry:
//   { name, login, avatar, url, commits, additions, deletions }
// in recency order (newest committer first → drives "last updated by"); the
// footer sorts a copy by contribution for its ranked list.
//
// The cache is incremental: an entry is reused only when the file's latest
// commit SHA is unchanged AND the entry already carries the stats fields.
//
// Runs as a `prebuild` step: the Cloudflare build (with a GITHUB_TOKEN secret)
// regenerates the data fresh on every deploy — nothing is committed back. With
// NO token the script SKIPS and the committed src/data/contributors.json is used
// as-is, so local `npm run build` stays token-free and leaves the file untouched.
// Refresh the committed baseline locally with:
//   GITHUB_TOKEN=$(gh auth token) npm run contributors

import { execSync } from 'node:child_process'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = fileURLToPath(new URL('..', import.meta.url))
const DOCS = 'src/content/docs/'
const OUT = new URL('../src/data/contributors.json', import.meta.url)
const PER_PAGE = 100
const MAX_PAGES = 5
const CONCURRENCY = 10

const TOKEN = process.env.GITHUB_TOKEN || process.env.GH_TOKEN || ''

function git(cmd) {
	return execSync(cmd, { cwd: ROOT, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 })
}

function repoSlug() {
	const url = git('git remote get-url origin').trim()
	const m = url.match(/github\.com[/:]([^/]+)\/(.+?)(?:\.git)?$/)
	if (!m) throw new Error(`cannot parse owner/repo from remote: ${url}`)
	return { owner: m[1], repo: m[2] }
}

function fileToSlug(file) {
	let rel = file.startsWith(DOCS) ? file.slice(DOCS.length) : file
	rel = rel.replace(/\.(md|mdx)$/, '')
	if (rel === 'index') return ''
	if (rel.endsWith('/index')) return rel.slice(0, -'/index'.length)
	return rel
}

function docFiles() {
	return git(`git ls-files -- ${DOCS}`)
		.split('\n')
		.map((l) => l.trim())
		.filter((f) => f.endsWith('.md') || f.endsWith('.mdx'))
}

function latestShaByFile() {
	const out = git('git log --no-merges --format=@@@%H --name-only')
	const map = new Map()
	let sha = null
	for (const line of out.split('\n')) {
		if (line.startsWith('@@@')) {
			sha = line.slice(3).trim()
			continue
		}
		const f = line.trim()
		if (!f || !sha) continue
		if (!map.has(f)) map.set(f, sha)
	}
	return map
}

function gitStats(file) {
	const out = git(`git log --no-merges --numstat --format=@@@%ae%x09%an -- "${file}"`)
	const byEmail = new Map()
	const order = []
	let email = null
	for (const line of out.split('\n')) {
		if (line.startsWith('@@@')) {
			const [e, n] = line.slice(3).split('\t')
			email = (e || '').trim().toLowerCase()
			if (!byEmail.has(email)) {
				byEmail.set(email, { email, name: (n || '').trim(), commits: 0, additions: 0, deletions: 0 })
				order.push(email)
			}
			byEmail.get(email).commits++
			continue
		}
		const m = line.match(/^(\d+|-)\t(\d+|-)\t/)
		if (m && email) {
			const e = byEmail.get(email)
			e.additions += m[1] === '-' ? 0 : Number(m[1])
			e.deletions += m[2] === '-' ? 0 : Number(m[2])
		}
	}
	return order.map((e) => byEmail.get(e))
}

function loadCache() {
	try {
		return JSON.parse(readFileSync(OUT, 'utf8'))
	} catch {
		return {}
	}
}

async function apiIdentities(owner, repo, file) {
	const map = new Map()
	for (let page = 1; page <= MAX_PAGES; page++) {
		const u = `https://api.github.com/repos/${owner}/${repo}/commits?path=${encodeURIComponent(file)}&per_page=${PER_PAGE}&page=${page}`
		const res = await fetch(u, {
			headers: {
				Accept: 'application/vnd.github+json',
				'X-GitHub-Api-Version': '2022-11-28',
				'User-Agent': 'rs4ts-contributors-script',
				...(TOKEN ? { Authorization: `Bearer ${TOKEN}` } : {}),
			},
		})
		if (res.status === 403 || res.status === 429) throw new Error(`rate limited (${res.status}) on ${file}`)
		if (!res.ok) throw new Error(`GitHub API ${res.status} on ${file}`)
		const commits = await res.json()
		if (!Array.isArray(commits) || commits.length === 0) break
		for (const c of commits) {
			const email = c?.commit?.author?.email?.trim().toLowerCase()
			if (!email || map.has(email)) continue
			map.set(email, {
				login: c?.author?.login || null,
				avatar: c?.author?.avatar_url || null,
				url: c?.author?.html_url || null,
			})
		}
		if (commits.length < PER_PAGE) break
	}
	return map
}

function mergeContributors(stats, ids) {
	const byKey = new Map()
	const order = []
	for (const s of stats) {
		const id = ids.get(s.email) || { login: null, avatar: null, url: null }
		const key = id.login ? `gh:${id.login.toLowerCase()}` : `em:${s.email}`
		if (!byKey.has(key)) {
			byKey.set(key, {
				name: s.name,
				login: id.login,
				avatar: id.avatar,
				url: id.url,
				commits: 0,
				additions: 0,
				deletions: 0,
			})
			order.push(key)
		}
		const e = byKey.get(key)
		e.commits += s.commits
		e.additions += s.additions
		e.deletions += s.deletions
		if (!e.login && id.login) {
			e.login = id.login
			e.avatar = id.avatar
			e.url = id.url
		}
	}
	return order.map((k) => byKey.get(k))
}

function hasStats(entry) {
	return (
		entry &&
		Array.isArray(entry.contributors) &&
		entry.contributors.every((c) => typeof c.commits === 'number')
	)
}

async function pool(items, worker) {
	const results = new Array(items.length)
	let i = 0
	const runners = Array.from({ length: Math.min(CONCURRENCY, items.length) }, async () => {
		while (i < items.length) {
			const idx = i++
			results[idx] = await worker(items[idx], idx)
		}
	})
	await Promise.all(runners)
	return results
}

async function main() {
	if (!TOKEN) {
		process.stdout.write(
			'no GITHUB_TOKEN/GH_TOKEN — skipping refresh, using committed src/data/contributors.json\n',
		)
		return
	}
	const { owner, repo } = repoSlug()
	const files = docFiles()
	const latest = latestShaByFile()
	const cache = loadCache()
	const next = {}
	let fetched = 0
	let reused = 0
	let failed = 0

	await pool(files, async (file) => {
		const slug = fileToSlug(file)
		const sha = latest.get(file) || null
		const prev = cache[slug]
		if (prev && prev.sha && prev.sha === sha && hasStats(prev)) {
			next[slug] = prev
			reused++
			return
		}
		try {
			const stats = gitStats(file)
			const ids = await apiIdentities(owner, repo, file)
			next[slug] = { sha, contributors: mergeContributors(stats, ids) }
			fetched++
		} catch (err) {
			if (prev) {
				next[slug] = prev
			} else {
				next[slug] = { sha, contributors: [] }
			}
			failed++
			process.stderr.write(`! ${file}: ${err.message}\n`)
		}
	})

	const sorted = {}
	for (const slug of Object.keys(next).sort()) sorted[slug] = next[slug]

	mkdirSync(dirname(fileURLToPath(OUT)), { recursive: true })
	writeFileSync(OUT, JSON.stringify(sorted, null, '\t') + '\n')

	const totalPeople = new Set(
		Object.values(sorted).flatMap((e) => e.contributors.map((c) => c.login || c.name)),
	).size
	process.stdout.write(
		`contributors.json: ${files.length} files (fetched ${fetched}, reused ${reused}, failed ${failed}), ${totalPeople} unique contributors\n`,
	)
}

main().catch((err) => {
	process.stderr.write(`fatal: ${err.stack || err.message}\n`)
	process.exit(1)
})
