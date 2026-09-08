#!/usr/bin/env node
// Every collection a scoped system module reads through its database handle
// must be on the module's allowlist in app.js (SCOPED_SYSTEM_MODULE_DB_COLLECTIONS)
// and in its module.json — a name that is missing is denied silently at runtime
// (ctx.db.collection() returns null), which hid the crew from the CTOX app on
// 07.09.2026. Companion surfaces (chat bar, reporter) are checked the same way.
//
// Usage: node scripts/assert-module-collection-allowlists.mjs [--json]
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const appRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const appJs = readFileSync(join(appRoot, 'app.js'), 'utf8');

function parseAllowlistBlock(constName) {
  const match = appJs.match(new RegExp(`const\\s+${constName}\\s*=\\s*(?:Object\\.freeze\\()?\\[([\\s\\S]*?)\\]\\)?;`));
  if (!match) throw new Error(`${constName} not found in app.js`);
  return [...match[1].matchAll(/'([a-z0-9_]+)'/g)].map((m) => m[1]);
}

function parseScopedModules() {
  const match = appJs.match(/const\s+SCOPED_SYSTEM_MODULE_DB_COLLECTIONS\s*=\s*Object\.freeze\(\s*\{([\s\S]*?)\n\}\);/);
  if (!match) throw new Error('SCOPED_SYSTEM_MODULE_DB_COLLECTIONS not found in app.js');
  const modules = new Map();
  for (const entry of match[1].matchAll(/['"]?([a-z0-9-]+)['"]?\s*:\s*Object\.freeze\(\[([\s\S]*?)\]\)/g)) {
    const names = [...entry[2].matchAll(/'([a-z0-9_]+)'/g)].map((m) => m[1]);
    if (entry[2].includes('WORKSPACE_BRANDING_COLLECTION')) names.push('business_workspace_branding');
    modules.set(entry[1], new Set(names));
  }
  return modules;
}

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules' || name === 'output' || name.startsWith('.')) continue;
    const path = join(dir, name);
    const stats = statSync(path);
    if (stats.isDirectory()) walk(path, out);
    else if (/\.(m?js)$/.test(name) && !/\.test\.m?js$|test\.js$|\.browser\.m?js$/.test(name)) out.push(path);
  }
  return out;
}

// Only direct handle reads count: `ctx.db.collection('x')`, `db.raw.x`,
// `db?.raw?.x`, `ctoxCollection(ctx, 'x')`, `loadLocalCollection(ctx, 'x')`.
const ACCESS_PATTERNS = [
  /\.collection\(\s*['"]([a-z0-9_]+)['"]\s*\)/g,
  /\.raw\??\.([a-z][a-z0-9_]+)/g,
  /ctoxCollection\([^,]+,\s*['"]([a-z0-9_]+)['"]/g,
  /loadLocalCollection\([^,]+,\s*['"]([a-z0-9_]+)['"]/g,
  /findLocalDocs\(\s*ctoxCollection\([^,]+,\s*['"]([a-z0-9_]+)['"]/g,
];

function collectionsReadBy(files) {
  const found = new Map();
  for (const file of files) {
    const source = readFileSync(file, 'utf8');
    for (const pattern of ACCESS_PATTERNS) {
      for (const match of source.matchAll(pattern)) {
        const name = match[1];
        if (!/^[a-z][a-z0-9_]*$/.test(name) || name.length < 6) continue;
        if (!found.has(name)) found.set(name, new Set());
        found.get(name).add(relative(appRoot, file));
      }
    }
  }
  return found;
}

const offenders = [];
const report = {};
const scoped = parseScopedModules();
for (const [moduleId, allowed] of scoped) {
  const moduleDir = join(appRoot, 'modules', moduleId);
  if (!existsSync(moduleDir)) continue;
  const manifestPath = join(moduleDir, 'module.json');
  const manifest = existsSync(manifestPath) ? JSON.parse(readFileSync(manifestPath, 'utf8')) : {};
  const declared = new Set(Array.isArray(manifest.collections) ? manifest.collections : []);
  const reads = collectionsReadBy(walk(moduleDir));
  const missing = [];
  for (const [name, files] of reads) {
    // Names that are neither declared nor allowed are usually not collections
    // (a property called `raw.value`); require the manifest to mention them.
    if (!declared.has(name)) continue;
    if (!allowed.has(name)) missing.push(`${name} (${[...files].join(', ')})`);
  }
  report[moduleId] = { declared: declared.size, read: reads.size, missing };
  for (const item of missing) {
    offenders.push(`modules/${moduleId} reads ${item} but SCOPED_SYSTEM_MODULE_DB_COLLECTIONS.${moduleId} does not allow it`);
  }
}

// Companion surfaces: the chat bar and the reporter read through their own facades.
for (const [constName, files] of [
  ['BUSINESS_CHAT_DB_COLLECTIONS', [join(appRoot, 'shared/business-chat.js')]],
  ['BUSINESS_REPORTER_DB_COLLECTIONS', [join(appRoot, 'shared/business-reporter.js')].filter(existsSync)],
]) {
  if (!files.length) continue;
  const allowed = new Set(parseAllowlistBlock(constName));
  const reads = collectionsReadBy(files);
  const knownCollections = new Set([...scoped.values()].flatMap((set) => [...set]).concat([...allowed]));
  const missing = [...reads.keys()].filter((name) => knownCollections.has(name) && !allowed.has(name));
  report[constName] = { read: reads.size, missing };
  for (const name of missing) offenders.push(`${relative(appRoot, files[0])} reads ${name} but ${constName} does not allow it`);
}

if (process.argv.includes('--json')) console.log(JSON.stringify({ ok: offenders.length === 0, report, offenders }, null, 2));
if (offenders.length) {
  console.error(`Module collection allowlists failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}
console.log(`module_collection_allowlists_ok=1 modules=${Object.keys(report).length}`);
