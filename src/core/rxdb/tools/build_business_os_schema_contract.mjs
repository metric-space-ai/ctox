#!/usr/bin/env node
// E45: v4/schemas/*.collections.schema.json is the sole packaged schema source.
// Module manifests only assert coverage. Native migration declarations and
// source-executed parity fixtures are generated with the contract.
import { access, readdir, readFile, stat, writeFile } from 'node:fs/promises';
import { buildNativeMigrations } from './business_os_v4_migrations.mjs';

import { normalizeCollectionsSchema } from '../../../apps/business-os/v4/tools/manifest/gen-manifest.mjs';

const OUTPUT_PATH = new URL('../../business_os/business_os_schema_contract.json', import.meta.url);
const MODULES_ROOT = new URL('../../../apps/business-os/modules/', import.meta.url);
const V4_ROOT = new URL('../../../apps/business-os/v4/schemas/', import.meta.url);
const MIGRATIONS_PATH = new URL('../../business_os/business_os_schema_migrations.json', import.meta.url);
const CORPUS_PATH = new URL('../tests/fixtures/business-os-v4-migrations.json', import.meta.url);
const documents = [];

async function buildContract() {
  const schemas = {};
  const modules = await discoverModules();
  const manifestCollections = [];
  for (const name of (await readdir(V4_ROOT)).filter(name => name.endsWith('.collections.schema.json')).sort()) {
    const url = new URL(name, V4_ROOT);
    const raw = JSON.parse(await readFile(url, 'utf8'));
    const document = normalizeCollectionsSchema(raw);
    documents.push({ raw, url });
    for (const [collection, schema] of Object.entries(document.collections)) {
      if (Object.hasOwn(schemas, collection)) throw new Error('Duplicate v4 collection owner: ' + collection);
      schemas[collection] = schema;
    }
  }
  for (const moduleInfo of modules) {
    for (const collection of moduleInfo.collections) manifestCollections.push({ module: moduleInfo, collection });
  }

  const missing = [];
  for (const item of manifestCollections) {
    if (schemas[item.collection]) continue;
    if (isInstalledOnlyRuntimeSchemaPath(item.module)) continue;
    missing.push(`${item.module.id}: ${item.collection}`);
  }
  if (missing.length) {
    throw new Error(
      'Business OS schema contract discovery found module.json collections not covered by the static contract or an installed-only runtime schema path:\n'
      + missing.map((line) => `- ${line}`).join('\n'),
    );
  }

  const reference = await readCommittedContract();
  const ordered = {};
  for (const key of Object.keys(schemas).sort()) {
    ordered[key] = orderLike(schemas[key], reference[key]);
  }
  return `${JSON.stringify(ordered, null, 2)}\n`;
}

async function readCommittedContract() {
  try {
    const parsed = JSON.parse(await readFile(OUTPUT_PATH, 'utf8'));
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

// Reorders object keys like `reference` without adding, dropping or changing
// any value: keys known to the reference first (in its order), then the rest.
function orderLike(value, reference) {
  if (Array.isArray(value)) {
    return value.map((item, index) => orderLike(item, Array.isArray(reference) ? reference[index] : undefined));
  }
  if (!value || typeof value !== 'object') return value;
  const ref = reference && typeof reference === 'object' && !Array.isArray(reference) ? reference : {};
  const keys = [
    ...Object.keys(ref).filter((key) => Object.hasOwn(value, key)),
    ...Object.keys(value).filter((key) => !Object.hasOwn(ref, key)),
  ];
  return Object.fromEntries(keys.map((key) => [key, orderLike(value[key], ref[key])]));
}

async function discoverModules() {
  const entries = await readdir(MODULES_ROOT);
  const modules = [];
  for (const entry of entries.sort()) {
    const dirUrl = new URL(`${entry}/`, MODULES_ROOT);
    const dirStats = await stat(dirUrl).catch(() => null);
    if (!dirStats?.isDirectory()) continue;
    const manifestUrl = new URL('module.json', dirUrl);
    if (!(await fileExists(manifestUrl))) continue;
    const manifest = JSON.parse(await readFile(manifestUrl, 'utf8'));
    const installScope = String(manifest.install_scope || '').trim().toLowerCase();
    if (installScope === 'sample') continue;
    const collections = Array.isArray(manifest.collections)
      ? manifest.collections.map((name) => String(name || '').trim()).filter(Boolean)
      : [];
    modules.push({
      id: entry,
      dirUrl,
      manifest,
      installScope,
      collections,
    });
  }
  if (!modules.length) {
    throw new Error(`No Business OS modules discovered under ${MODULES_ROOT.pathname}`);
  }
  return modules;
}

function isInstalledOnlyRuntimeSchemaPath(moduleInfo) {
  const manifest = moduleInfo.manifest || {};
  const installScope = String(moduleInfo.installScope || manifest.install_scope || '').trim().toLowerCase();
  const entry = String(manifest.entry || '').trim();
  return installScope === 'installed' || entry.startsWith('installed-modules/');
}

async function fileExists(url) {
  try {
    await access(url);
    return true;
  } catch {
    return false;
  }
}

const next = await buildContract();
const native = await buildNativeMigrations(documents, V4_ROOT);
for (const [path, value] of [[MIGRATIONS_PATH, native.strategies], [CORPUS_PATH, native.corpus]]) {
  const text = JSON.stringify(value, null, 2) + '\n';
  if (process.argv.includes('--write')) await writeFile(path, text);
  else if (await readFile(path, 'utf8').catch(() => '') !== text) throw new Error('Native v4 migration contract is stale: ' + path.pathname);
}
if (process.argv.includes('--write')) {
  await writeFile(OUTPUT_PATH, next);
  console.log(`wrote ${OUTPUT_PATH.pathname}`);
} else {
  const current = await readFile(OUTPUT_PATH, 'utf8');
  if (current !== next) {
    console.error('Business OS schema contract is stale. Run with --write.');
    process.exit(1);
  }
  console.log('Business OS schema contract is current.');
}
