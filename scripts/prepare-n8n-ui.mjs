#!/usr/bin/env node
// Copy the built n8n editor-ui dist into adk-server's static dir and apply the
// same placeholder substitutions n8n's own server does in
// `packages/cli/src/commands/start.ts` (generateStaticAssets), so the SPA boots
// same-origin against adk-server's /rest surface.
//
// Usage: node scripts/prepare-n8n-ui.mjs [<n8n-repo-dir>]
import { existsSync } from 'node:fs';
import { cp, mkdir, readFile, readdir, writeFile, rm } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';

const REPO = process.argv[2] ?? path.resolve('n8n');
const SRC = path.join(REPO, 'packages/frontend/editor-ui/dist');
const DEST = path.resolve('crates/adk-server/static/n8n-editor-ui');

// Mirror n8n config defaults: endpoints.rest = 'rest', path = '/'.
const N8N_PATH = '/';
const REST_ENDPOINT = 'rest';
const b64 = (s) => Buffer.from(s).toString('base64');
const sentry = JSON.stringify({ dsn: '', environment: 'development', release: 'n8n@local' });
const CONFIG_TAGS =
	`<meta name="n8n:config:rest-endpoint" content="${b64(REST_ENDPOINT)}">` +
	`<meta name="n8n:config:sentry" content="${b64(sentry)}">`;

function transform(contents, isHtml) {
	let out = contents
		.replaceAll('%CONFIG_TAGS%', CONFIG_TAGS)
		.replaceAll('/{{BASE_PATH}}/', N8N_PATH)
		.replaceAll('/%7B%7BBASE_PATH%7D%7D/', N8N_PATH)
		.replaceAll('/%257B%257BBASE_PATH%257D%257D/', N8N_PATH);
	if (isHtml) out = out.replaceAll('{{REST_ENDPOINT}}', REST_ENDPOINT);
	return out;
}

async function* walk(dir) {
	for (const entry of await readdir(dir, { withFileTypes: true })) {
		const full = path.join(dir, entry.name);
		if (entry.isDirectory()) yield* walk(full);
		else yield full;
	}
}

async function main() {
	if (!existsSync(SRC)) {
		console.error(`n8n dist not found at ${SRC}. Build it first:`);
		console.error(`  (cd ${REPO} && pnpm install --filter "n8n-editor-ui..." && pnpm turbo run build --filter=n8n-editor-ui)`);
		process.exit(1);
	}
	await rm(DEST, { recursive: true, force: true });
	await mkdir(DEST, { recursive: true });
	await cp(SRC, DEST, { recursive: true });

	let processed = 0;
	for await (const file of walk(DEST)) {
		if (!/\.(html|js|css)$/.test(file)) continue;
		const isHtml = file.endsWith('.html');
		const original = await readFile(file, 'utf8');
		if (!/%CONFIG_TAGS%|\{\{BASE_PATH\}\}|%7B%7BBASE_PATH|\{\{REST_ENDPOINT\}\}/.test(original)) continue;
		await writeFile(file, transform(original, isHtml), 'utf8');
		processed++;
	}
	console.log(`Copied n8n editor-ui dist -> ${DEST}`);
	console.log(`Applied placeholder substitutions to ${processed} files (rest='${REST_ENDPOINT}', base='${N8N_PATH}').`);
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});
