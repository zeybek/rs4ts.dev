export const ROOT_SEO_TITLE = 'Rust for TypeScript & JavaScript Developers - Free Guide'
export const ROOT_SEO_DESCRIPTION =
	'Learn Rust through TypeScript and JavaScript mental models, with side-by-side examples, migration notes, and a free chapter-by-chapter guide.'

type SeoTitleInput = {
	slug: string
	title: string
	section?: string
	seoTitle?: string
}

type SeoDescriptionInput = {
	slug: string
	title: string
	description?: string
	section?: string
	seoDescription?: string
}

const SEO_TITLE_OVERRIDES: Record<string, string> = {
	'00-introduction': 'Rust for JavaScript & TypeScript Developers: Complete Guide',
	'00-introduction/00-target-audience': 'Who This Rust Guide Is For: TypeScript and JavaScript Developers',
	'00-introduction/01-how-to-read': 'How to Learn Rust as a JavaScript or TypeScript Developer',
	'00-introduction/02-prerequisites': 'Rust Prerequisites for JavaScript and TypeScript Developers',
	'00-introduction/03-cheatsheet': 'JavaScript and TypeScript to Rust Cheat Sheet',
	'00-introduction/04-faq': 'Rust for JavaScript and TypeScript Developers: FAQ',
	'01-getting-started': 'Getting Started with Rust for JavaScript and TypeScript Developers',
	'01-getting-started/00-why-rust': 'Why Learn Rust as a JavaScript or TypeScript Developer',
	'01-getting-started/01-installation': 'Install Rust for JavaScript and TypeScript Developers',
	'01-getting-started/02-hello-world': 'Rust Hello World for JavaScript Developers',
	'01-getting-started/03-cargo-basics': 'Cargo for npm Users: Rust Package Management Basics',
	'01-getting-started/04-playground': 'Rust Playground for JavaScript Developers',
	'05-ownership': 'Rust Ownership for JavaScript Developers: Moves, Borrows, and Lifetimes',
	'05-ownership/00-stack-heap': 'Rust Stack vs Heap for JavaScript Developers',
	'05-ownership/01-ownership-rules': 'Rust Ownership Rules for JavaScript Developers: Move vs Reference',
	'05-ownership/02-borrowing':
		'Rust Borrowing for JavaScript Developers: References vs Object References',
	'05-ownership/03-mutable-references':
		'Rust Mutable References for JavaScript Developers: &mut vs Object Mutation',
	'05-ownership/04-lifetimes': 'Rust Lifetimes for JavaScript Developers: Why References Stay Valid',
	'05-ownership/05-lifetime-elision': 'Rust Lifetime Elision for JavaScript Developers',
	'05-ownership/06-move-copy-clone': 'Rust Move, Copy, and Clone for JavaScript Developers',
	'05-ownership/07-reference-counting': 'Rust Rc and Arc for JavaScript Developers: Shared Ownership',
	'05-ownership/08-drop-trait': 'Rust Drop Trait and RAII for JavaScript Developers',
	'09-generics-traits/03-traits': 'Rust Traits vs TypeScript Interfaces',
	'16-web-apis': 'Rust Web APIs with Axum for Node.js Developers',
	'16-web-apis/00-framework-comparison': 'Axum vs Actix Web vs Rocket for Node.js Developers',
	'16-web-apis/01-axum-basics': 'Axum Basics for Express Developers',
	'16-web-apis/02-axum-setup': 'Set Up an Axum Project for Node.js Developers',
	'16-web-apis/03-routing': 'Axum Routing for Express Developers',
	'16-web-apis/04-extractors': 'Axum Extractors for Express and TypeScript Developers',
	'16-web-apis/08-json-apis': 'Rust JSON APIs with Axum for Express Developers',
	'16-web-apis/09-validation': 'Request Validation in Axum for Express Developers',
	'16-web-apis/10-error-handling-web': 'Axum Error Handling for Express Developers',
	'16-web-apis/12-authentication': 'Rust Authentication with Axum for Node.js Developers',
	'16-web-apis/13-jwt': 'JWT Authentication in Rust for Node.js Developers',
	'16-web-apis/14-sessions': 'Sessions in Rust Web Apps for Express Developers',
	'16-web-apis/15-websockets': 'WebSockets with Axum for Node.js Developers',
	'16-web-apis/16-sse': 'Server-Sent Events in Axum for Node.js Developers',
	'16-web-apis/18-static-files': 'Serve Static Files with Axum for Express Developers',
	'17-database': 'Rust Databases for Node.js Developers: SQLx, Diesel, SeaORM',
	'17-database/00-sqlx-intro': 'SQLx for Node.js Developers: Compile-Time Checked SQL',
	'17-database/01-sqlx-queries': 'SQLx Queries for Node.js Developers',
	'17-database/02-sqlx-transactions': 'SQLx Transactions for Node.js Developers',
	'17-database/03-diesel-intro': 'Diesel ORM for TypeORM Developers',
	'17-database/04-diesel-queries': 'Diesel Query Builder for Knex and TypeORM Developers',
	'17-database/10-orm-comparison': 'SQLx vs Diesel vs SeaORM for Node.js Developers',
	'18-cli-tools': 'Rust CLI Tools for Node.js Developers',
	'18-cli-tools/00-clap-basics': 'clap Basics for Node.js CLI Developers',
	'18-cli-tools/01-clap-derive': 'clap Derive for TypeScript CLI Developers',
	'19-wasm': 'Rust WebAssembly for JavaScript Developers',
	'19-wasm/00-wasm-intro': 'Rust and WebAssembly for JavaScript Developers',
	'19-wasm/01-wasm-pack': 'wasm-pack for JavaScript Developers',
	'19-wasm/02-first-wasm': 'Build Your First Rust WebAssembly Module',
	'19-wasm/03-js-interop': 'Rust and JavaScript Interop with WebAssembly',
	'19-wasm/04-rust-from-js': 'Call Rust from JavaScript with WebAssembly',
	'19-wasm/05-wasm-bindgen': 'wasm-bindgen for JavaScript Developers',
	'19-wasm/06-web-apis': 'Use Browser Web APIs from Rust WebAssembly',
	'19-wasm/07-dom-manipulation': 'DOM Manipulation from Rust WebAssembly',
	'19-wasm/08-yew-leptos': 'Yew vs Leptos for JavaScript Developers',
	'19-wasm/09-performance': 'Rust WebAssembly Performance for JavaScript Developers',
	'19-wasm/10-deployment': 'Deploy Rust WebAssembly Apps for JavaScript Developers',
	'20-unsafe-ffi': 'Unsafe Rust and FFI for JavaScript Native Addon Developers',
	'20-unsafe-ffi/06-napi': 'napi-rs for Node.js Native Addon Developers',
	'20-unsafe-ffi/07-neon': 'Neon for Node.js Native Addon Developers',
	'20-unsafe-ffi/09-when-to-use': 'When to Use unsafe and FFI in Rust',
	'21-performance': 'Rust Performance for Node.js and TypeScript Developers',
	'21-performance/10-when-to-optimize': 'When to Optimize Rust: Measure First, Then Tune',
	'21-performance/09-comparison': 'Rust vs Node.js Performance: Where Rust Wins',
	'24-tooling': 'Rust Tooling for TypeScript Developers',
	'24-tooling/05-rust-analyzer': 'rust-analyzer Setup for VS Code Users',
	'24-tooling/06-vscode-setup': 'Set Up VS Code for Rust Development',
	'25-advanced-topics/03-allocators': 'Rust Custom Allocators: GlobalAlloc, jemalloc, and mimalloc',
	'29-migration-guide': 'Node.js to Rust Migration Guide',
	'29-migration-guide/00-incremental': 'Incremental Node.js to Rust Migration Strategy',
	'29-migration-guide/01-node-to-rust': 'Port a Node.js Service to Rust',
	'29-migration-guide/02-api-compatibility': 'Keep API Compatibility During Node.js to Rust Migration',
	'29-migration-guide/03-data-migration': 'Data Migration Strategies for Node.js to Rust',
	'29-migration-guide/04-performance-gains': 'Measure Node.js to Rust Performance Gains',
	'29-migration-guide/05-common-challenges': 'Common Node.js to Rust Migration Challenges',
	'30-projects': 'Rust Projects for JavaScript and TypeScript Developers',
	'30-projects/00-rest-api': 'Build a Rust REST API for Express Developers',
	'30-projects/01-cli-tool': 'Build a Rust CLI Tool for Node.js Developers',
	'30-projects/02-wasm-app': 'Build a Rust WebAssembly App for JavaScript Developers',
	'30-projects/03-websocket-chat': 'Build a WebSocket Chat App with Rust and Axum',
	'30-projects/04-microservice': 'Build a Rust Microservice for Node.js Developers',
	'30-projects/05-full-stack': 'Build a Full-Stack Rust App for TypeScript Developers',
	'23-ecosystem/10-useful-crates': 'Essential Rust Crates: itertools, rayon, uuid, bytes, dashmap',
}

const SEO_DESCRIPTION_OVERRIDES: Record<string, string> = {
	'00-introduction':
		'Start the Rust guide built for JavaScript and TypeScript developers, with side-by-side examples that turn familiar concepts into Rust syntax.',
	'00-introduction/01-how-to-read':
		'Learn how to move through this Rust guide efficiently, using your JavaScript and TypeScript experience to pick the right chapter order.',
	'00-introduction/03-cheatsheet':
		'Use a fast JavaScript and TypeScript to Rust cheat sheet for syntax, types, ownership, async, collections, errors, and everyday patterns.',
	'01-getting-started':
		'Get Rust running from a JavaScript developer baseline, from installation and Cargo to your first runnable project.',
	'01-getting-started/00-why-rust':
		'See why JavaScript and TypeScript developers reach for Rust, with practical tradeoffs around speed, safety, WebAssembly, and production services.',
	'01-getting-started/01-installation':
		'Install Rust the practical way, with rustup, Cargo, editor setup, and the checks JavaScript developers need before writing code.',
	'01-getting-started/03-cargo-basics':
		'Learn Cargo through the npm mental model, including build, run, test, package layout, dependencies, and the commands you will use daily.',
	'05-ownership':
		'Understand Rust ownership through JavaScript reference and garbage-collection habits, with examples for moves, borrows, lifetimes, and shared ownership.',
	'05-ownership/01-ownership-rules':
		'Learn Rust ownership rules through JavaScript assignment and function-call examples, so moves, drops, and references stop feeling mysterious.',
	'05-ownership/02-borrowing':
		'Learn Rust borrowing through the JavaScript object-reference model, with side-by-side TypeScript and Rust examples.',
	'05-ownership/03-mutable-references':
		'Learn mutable references in Rust through JavaScript object mutation, with examples that show why one writer beats hidden shared state.',
	'05-ownership/04-lifetimes':
		'Learn Rust lifetimes through the memory-safety problems garbage collection hides, with examples that make reference validity concrete.',
	'05-ownership/06-move-copy-clone':
		'Compare Rust move, Copy, and Clone with JavaScript reference sharing, so you know when values transfer, duplicate, or deep-copy.',
	'04-control-flow/03-if-let-while-let':
		'Learn Rust if let, while let, and let-else pattern matching through JavaScript control flow, with concise side-by-side examples.',
	'09-generics-traits/03-traits':
		'Compare Rust traits with TypeScript interfaces, including nominal impls, default methods, trait bounds, and behavior you can attach safely.',
	'16-web-apis':
		'Build Rust web APIs with Axum through Express and Node.js mental models, from routing and extractors to validation, auth, and deployment.',
	'16-web-apis/00-framework-comparison':
		'Compare Axum, Actix Web, and Rocket from a Node.js developer perspective, with tradeoffs for routing, async, ecosystem, and production fit.',
	'16-web-apis/01-axum-basics':
		'Learn Axum basics through Express concepts, with handlers, routers, extractors, and typed responses shown side by side.',
	'16-web-apis/03-routing':
		'Map Express routes to Axum routing, with path params, nesting, method handlers, and typed handlers you can use in a Rust API.',
	'16-web-apis/04-extractors':
		'Learn Axum extractors through Express request objects, with typed path params, query strings, JSON bodies, headers, and shared state.',
	'16-web-apis/08-json-apis':
		'Build JSON APIs in Rust with Axum and Serde, using the same request and response workflow you know from Express.',
	'16-web-apis/09-validation':
		'Add request validation to Axum like Express middleware, with typed payloads, validator errors, and clean client-facing responses.',
	'16-web-apis/10-error-handling-web':
		'Handle Axum errors through the Express error-middleware mental model, with typed failures that return predictable HTTP responses.',
	'16-web-apis/12-authentication':
		'Build authentication in an Axum app from a Node.js baseline, covering passwords, sessions, JWTs, cookies, and secure handler state.',
	'16-web-apis/13-jwt':
		'Implement JWT authentication in Rust for Node.js developers, with signed claims, middleware-style extraction, and safe error responses.',
	'17-database':
		'Learn Rust database access from a Node.js perspective, comparing SQLx, Diesel, SeaORM, Redis, MongoDB, pools, and migrations.',
	'17-database/00-sqlx-intro':
		'Learn SQLx as a Node.js developer, with raw SQL that Rust can check at compile time instead of failing late in production.',
	'17-database/01-sqlx-queries':
		'Write SQLx queries in Rust with typed rows, bind parameters, compile-time checks, and examples that map cleanly from Node database code.',
	'17-database/02-sqlx-transactions':
		'Learn SQLx transactions through the Node.js try-finally mental model, with commit, rollback, and ownership patterns that prevent leaks.',
	'17-database/03-diesel-intro':
		'Compare Diesel with TypeORM and Knex, then see how Rust turns schema, entities, and query mistakes into compile-time feedback.',
	'17-database/04-diesel-queries':
		'Write Diesel queries through the Knex and TypeORM mental model, with typed filters, joins, ordering, and compile-time column checks.',
	'17-database/10-orm-comparison':
		'Compare SQLx, Diesel, and SeaORM against Node.js database tools, so you can pick raw SQL, query DSL, or ORM for a Rust service.',
	'19-wasm':
		'Learn Rust WebAssembly from a JavaScript baseline, from wasm-pack and wasm-bindgen to DOM interop, frameworks, performance, and deployment.',
	'19-wasm/02-first-wasm':
		'Build your first Rust WebAssembly module for JavaScript, with the smallest working path from Rust code to a browser-callable function.',
	'19-wasm/03-js-interop':
		'Learn Rust and JavaScript WebAssembly interop, with examples for values, strings, objects, errors, and the boundary costs that matter.',
	'19-wasm/09-performance':
		'Measure Rust WebAssembly performance from a JavaScript perspective, with realistic advice on when WASM helps and when boundary costs dominate.',
	'20-unsafe-ffi/06-napi':
		'Build Node.js native addons with napi-rs, mapping Rust functions to JavaScript calls while keeping ownership and error handling explicit.',
	'21-performance':
		'Learn where Rust beats Node.js on performance, with practical guidance for CPU work, memory use, latency, profiling, and measurement.',
	'21-performance/09-comparison':
		'Compare Rust and Node.js performance honestly, with examples showing where Rust wins, where Node is enough, and what to measure first.',
	'24-tooling/05-rust-analyzer':
		'Set up rust-analyzer like a TypeScript language server, with editor feedback, refactors, diagnostics, and navigation that make Rust productive.',
	'24-tooling/06-vscode-setup':
		'Set up VS Code for Rust development, with rust-analyzer, formatting, debugging, tasks, and the workflow TypeScript developers expect.',
	'29-migration-guide':
		'Plan a Node.js to Rust migration with incremental service moves, API compatibility, data migration, performance checks, and team risks.',
	'29-migration-guide/00-incremental':
		'Use the strangler pattern to migrate from Node.js to Rust one route at a time, with rollback paths and low-risk production steps.',
	'29-migration-guide/01-node-to-rust':
		'Port a Node.js endpoint to Rust with Axum, keeping JSON, status codes, headers, and behavior compatible while adopting typed handlers.',
	'29-migration-guide/02-api-compatibility':
		'Keep API compatibility during a Node.js to Rust migration, with JSON casing, null handling, headers, status codes, and golden fixtures.',
	'29-migration-guide/04-performance-gains':
		'Measure Node.js to Rust performance gains with p99 latency, memory, throughput, and benchmarks that avoid misleading averages.',
	'30-projects':
		'Build practical Rust projects from a JavaScript and TypeScript baseline, including REST APIs, CLIs, WebAssembly apps, microservices, and full-stack code.',
	'30-projects/00-rest-api':
		'Build a Rust REST API through the Express mental model, with Axum routing, JSON handlers, validation, errors, and production-ready structure.',
	'30-projects/01-cli-tool':
		'Build a Rust CLI tool from a Node.js baseline, with clap arguments, terminal output, filesystem access, errors, and packaging flow.',
	'30-projects/02-wasm-app':
		'Build a Rust WebAssembly app for JavaScript developers, from Rust code and wasm-bindgen to browser integration and deployment.',
	'30-projects/04-microservice':
		'Build a Rust microservice from a Node.js service baseline, covering HTTP, config, logging, health checks, database access, and deployment.',
}

const AUDIENCE_BY_SECTION: Record<string, string> = {
	'00-introduction': 'JavaScript and TypeScript Developers',
	'01-getting-started': 'JavaScript and TypeScript Developers',
	'02-basics': 'JavaScript Developers',
	'03-functions': 'JavaScript Developers',
	'04-control-flow': 'JavaScript Developers',
	'05-ownership': 'JavaScript Developers',
	'06-data-structures': 'TypeScript Developers',
	'07-collections': 'JavaScript Developers',
	'08-error-handling': 'TypeScript Developers',
	'09-generics-traits': 'TypeScript Developers',
	'10-smart-pointers': 'JavaScript Developers',
	'11-async': 'JavaScript Developers',
	'12-modules-packages': 'npm Users',
	'13-testing': 'TypeScript Developers',
	'14-macros': 'TypeScript Developers',
	'15-serialization': 'TypeScript Developers',
	'16-web-apis': 'Node.js Developers',
	'17-database': 'Node.js Developers',
	'18-cli-tools': 'Node.js Developers',
	'19-wasm': 'JavaScript Developers',
	'20-unsafe-ffi': 'Node.js Developers',
	'21-performance': 'Node.js Developers',
	'22-common-patterns': 'TypeScript Developers',
	'23-ecosystem': 'TypeScript Developers',
	'24-tooling': 'TypeScript Developers',
	'25-advanced-topics': 'TypeScript Developers',
	'26-systems-programming': 'JavaScript Developers',
	'27-security': 'Node.js Developers',
	'28-production': 'Node.js Developers',
	'29-migration-guide': 'Node.js Developers',
	'30-projects': 'JavaScript and TypeScript Developers',
}

const MENTAL_MODEL_BY_SECTION: Record<string, string> = {
	'00-introduction': 'JavaScript and TypeScript mental models',
	'01-getting-started': 'JavaScript and TypeScript mental models',
	'02-basics': 'JavaScript and TypeScript syntax habits',
	'03-functions': 'TypeScript function patterns',
	'04-control-flow': 'JavaScript control-flow habits',
	'05-ownership': 'JavaScript object-reference habits',
	'06-data-structures': 'TypeScript data-modeling habits',
	'07-collections': 'JavaScript collection APIs',
	'08-error-handling': 'TypeScript error-handling habits',
	'09-generics-traits': 'TypeScript type-system mental models',
	'10-smart-pointers': 'JavaScript reference and heap mental models',
	'11-async': 'JavaScript async and Promise habits',
	'12-modules-packages': 'npm and TypeScript module habits',
	'13-testing': 'Jest and Vitest testing habits',
	'14-macros': 'TypeScript metaprogramming mental models',
	'15-serialization': 'JSON and TypeScript data habits',
	'16-web-apis': 'Express and Node.js mental models',
	'17-database': 'Node.js database-tooling mental models',
	'18-cli-tools': 'Node.js CLI tooling habits',
	'19-wasm': 'JavaScript and WebAssembly mental models',
	'20-unsafe-ffi': 'Node.js native-addon mental models',
	'21-performance': 'Node.js performance mental models',
	'22-common-patterns': 'TypeScript application patterns',
	'23-ecosystem': 'TypeScript ecosystem habits',
	'24-tooling': 'TypeScript tooling habits',
	'25-advanced-topics': 'TypeScript type-system and runtime mental models',
	'26-systems-programming': 'Node.js systems-programming mental models',
	'27-security': 'Express and Node.js security habits',
	'28-production': 'Node.js production-service habits',
	'29-migration-guide': 'Node.js migration mental models',
	'30-projects': 'JavaScript and TypeScript project patterns',
}

export function buildSeoTitle({ slug, title, seoTitle }: SeoTitleInput): string {
	if (seoTitle) return normalizeWhitespace(seoTitle)
	if (slug === '') return ROOT_SEO_TITLE
	if (SEO_TITLE_OVERRIDES[slug]) return SEO_TITLE_OVERRIDES[slug]

	const clean = cleanTitle(title)
	if (hasStrongSearchIntent(clean)) return compactTitle(clean)

	const section = slug.split('/')[0] || ''
	const audience = AUDIENCE_BY_SECTION[section] ?? 'JavaScript and TypeScript Developers'
	return compactTitle(`${toRustTopic(clean)} for ${audience}`)
}

export function buildSeoDescription({
	slug,
	title,
	description,
	section,
	seoDescription,
}: SeoDescriptionInput): string {
	if (seoDescription) return compactDescription(seoDescription)
	if (slug === '') return ROOT_SEO_DESCRIPTION
	if (SEO_DESCRIPTION_OVERRIDES[slug]) return SEO_DESCRIPTION_OVERRIDES[slug]

	const clean = cleanTitle(title)
	const sectionSlug = slug.split('/')[0] || ''
	const model =
		MENTAL_MODEL_BY_SECTION[sectionSlug] ??
		(section ? `${section.toLowerCase()} concepts` : 'JavaScript and TypeScript mental models')
	const { action, topic } = descriptionActionAndTopic(clean)
	const detail = descriptionDetail(description)

	return compactDescription(
		`${action} ${topic} through ${model}, with side-by-side Rust examples${detail}.`
	)
}

function cleanTitle(title: string): string {
	return normalizeWhitespace(
		title
			.replace(/`/g, '')
			.replace(/&lt;/g, '<')
			.replace(/&gt;/g, '>')
			.replace(/&amp;/g, '&')
			.replace(/&quot;/g, '"')
			.replace(/\s+-\s+/g, ': ')
	)
}

function descriptionActionAndTopic(title: string): { action: string; topic: string } {
	const normalized = normalizeWhitespace(title.replace(/&/g, 'and'))
	const actionMatch =
		/^(Build|Compare|Install|Set Up|Deploy|Port|Keep|Measure|Serve|Use|Write|Add|Handle|Run|Ship|Benchmark|Profile|Trace|Defend|Choose|Catch|Move|Map|Try|Learn)\s+(.+)$/i.exec(
			normalized
		)

	if (actionMatch) {
		return {
			action: sentenceCaseAction(actionMatch[1]),
			topic: normalizeTopic(actionMatch[2]),
		}
	}

	if (/^Why\s+/i.test(normalized)) {
		return { action: 'Understand', topic: normalizeTopic(normalized.replace(/^Why\s+/i, 'why ')) }
	}

	if (/^When to\s+/i.test(normalized)) {
		return { action: 'Decide', topic: normalizeTopic(normalized.replace(/^When to\s+/i, 'when to ')) }
	}

	const gerundMatch = /^(Calling|Creating|Implementing|Understanding)\s+(.+)$/i.exec(normalized)
	if (gerundMatch) {
		const actionByGerund: Record<string, string> = {
			calling: 'Call',
			creating: 'Create',
			implementing: 'Implement',
			understanding: 'Understand',
		}
		return {
			action: actionByGerund[gerundMatch[1].toLowerCase()] ?? 'Learn',
			topic: normalizeTopic(gerundMatch[2]),
		}
	}

	return { action: 'Learn', topic: normalizeTopic(toRustTopic(normalized)) }
}

function sentenceCaseAction(action: string): string {
	return action
		.split(/\s+/)
		.map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
		.join(' ')
}

function normalizeTopic(topic: string): string {
	return normalizeWhitespace(
		topic
			.replace(/^The\s+/i, '')
			.replace(/\bin Rust$/i, '')
			.replace(/^(.+) in Axum$/i, 'Axum $1')
			.replace(/^(.+) in Rust$/i, 'Rust $1')
			.replace(/\s*:\s*/g, ': ')
	)
}

function descriptionDetail(description?: string): string {
	if (!description) return ' and practical migration notes'
	const text = normalizeWhitespace(
		description
			.replace(/`/g, '')
			.replace(/&lt;/g, '<')
			.replace(/&gt;/g, '>')
			.replace(/&amp;/g, '&')
			.replace(/&quot;/g, '"')
	)

	if (/compile/i.test(text)) return ' and the compile-time checks that matter'
	if (/\bperformance|latency|memory|benchmark|profil/i.test(text)) {
		return ' and the performance tradeoffs that matter'
	}
	if (/\bExpress|Axum|handler|HTTP|API|request|response|route/i.test(text)) {
		return ' and the web API patterns that matter'
	}
	if (/\bTypeScript|JavaScript|Node\.js|npm|Jest|Vitest|Express/i.test(text)) {
		return ' and the migration pitfalls that matter'
	}
	return ' and practical migration notes'
}

function hasStrongSearchIntent(title: string): boolean {
	return /\b(for (TS\/JS|TypeScript|JavaScript|Node\.js|Express|npm) Developers|vs |Node\.js to Rust|Rust vs Node\.js|TypeScript Interfaces|JavaScript and TypeScript to Rust)\b/i.test(
		title
	)
}

function toRustTopic(title: string): string {
	const withoutRustSuffix = title
		.replace(/^The (.+) in Rust$/i, 'Rust $1')
		.replace(/\bin Rust$/i, '')

	if (/^(Rust|Cargo|Axum|Tokio|Serde|SQLx|Diesel|SeaORM|WebAssembly|wasm-pack|wasm-bindgen|napi-rs|Neon|FFI|CORS|Redis|MongoDB|Dockerizing Rust|Benchmarking with Criterion|Profiling Rust|rust-analyzer)\b/i.test(withoutRustSuffix)) {
		return withoutRustSuffix
	}

	return `Rust ${withoutRustSuffix}`
}

function compactTitle(title: string): string {
	let compact = normalizeWhitespace(title)
	if (compact.length <= 78) return compact

	compact = compact
		.replace(/TypeScript and JavaScript/g, 'TS/JS')
		.replace(/JavaScript and TypeScript/g, 'JS/TS')
		.replace(/TypeScript & JavaScript/g, 'TS/JS')
		.replace(/JavaScript & TypeScript/g, 'JS/TS')
		.replace(/JavaScript Developers/g, 'JS Developers')
		.replace(/TypeScript Developers/g, 'TS Developers')
		.replace(/Node\.js Developers/g, 'Node.js Devs')
		.replace(/\s+and\s+/g, ' & ')

	compact = normalizeWhitespace(compact)
	if (compact.length <= 78) return compact

	const withoutAudience = compact.replace(
		/\s+for (JS Developers|TS Developers|Node\.js Devs|npm Users)$/,
		''
	)
	return normalizeWhitespace(withoutAudience)
}

function compactDescription(description: string): string {
	let compact = normalizeWhitespace(description)
		.replace(/\s*([.!?])(?:\s+[A-Z].*)?$/, '$1')
		.replace(/\s*\.+$/, '')

	if (!/[.!?]$/.test(compact)) compact = `${compact}.`
	if (compact.length <= 160) return compact

	compact = compact
		.replace(/TypeScript and JavaScript/g, 'TS/JS')
		.replace(/JavaScript and TypeScript/g, 'JS/TS')
		.replace(/practical migration notes/g, 'migration notes')
		.replace(/mental models/g, 'model')
		.replace(/the tradeoffs that matter/g, 'key tradeoffs')
		.replace(/the patterns that matter/g, 'key patterns')
		.replace(/the checks that matter/g, 'key checks')

	compact = normalizeWhitespace(compact)
	if (compact.length <= 160) return compact

	compact = compact
		.replace(/, with side-by-side Rust examples and [^.]+\.$/, ', with Rust examples.')
		.replace(/, with side-by-side Rust examples\.$/, ', with Rust examples.')

	compact = normalizeWhitespace(compact)
	if (compact.length <= 160) return compact

	const wordBoundary = compact.lastIndexOf(' ', 157)
	const trimmed = compact.slice(0, wordBoundary > 120 ? wordBoundary : 157).replace(/[,\s;:.-]+$/, '')
	return `${trimmed}.`
}

function normalizeWhitespace(value: string): string {
	return value.replace(/\s+/g, ' ').trim()
}
