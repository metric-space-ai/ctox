#!/usr/bin/env node
// Builds src/core/business_os/business_os_schema_contract.json, the native
// peer's static RxDB schema contract, from the Business OS modules.
//
// Source: each module's collections.schema.json — the only schema source
// (§16.13; schema.js was deleted by the v4 app ports, deletion plan
// L-09/L-10). The document is read through the v4 shell manifest generator's
// own normalizer (normalizeCollectionsSchema), so `{ schema, ... }` wrappers
// and both migration formats are understood exactly as the shell reads them.
//
// Key order: collections.schema.json files are key-sorted, the committed
// contract keeps the historical schema.js order. Object keys therefore keep
// the order of the committed contract; keys it does not know yet follow in
// source order. Order is not semantic (hashes use canonicalJson), this only
// keeps the contract diff limited to real schema changes.
import { access, readdir, readFile, stat, writeFile } from 'node:fs/promises';

import { normalizeCollectionsSchema } from '../../../apps/business-os/v4/tools/manifest/gen-manifest.mjs';

const OUTPUT_PATH = new URL('../../business_os/business_os_schema_contract.json', import.meta.url);
const MODULES_ROOT = new URL('../../../apps/business-os/modules/', import.meta.url);
const SCHEMA_FILE = 'collections.schema.json';

async function buildContract() {
  const schemas = {};
  const modules = await discoverModules();
  const manifestCollections = [];
  for (const moduleInfo of modules) {
    const document = await readSchemaDocument(moduleInfo.schemaUrl, moduleInfo.id);
    for (const [collection, schema] of Object.entries(document.collections)) {
      schemas[collection] = schemas[collection]
        ? mergeSchemaDefinitions(collection, schemas[collection], schema)
        : schema;
    }
    for (const collection of moduleInfo.collections) {
      manifestCollections.push({ module: moduleInfo, collection });
    }
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

function mergeSchemaDefinitions(collection, current, incoming) {
  const left = cloneJson(current);
  const right = cloneJson(incoming);
  for (const key of ['version', 'primaryKey', 'type']) {
    if (left[key] !== undefined && right[key] !== undefined && JSON.stringify(left[key]) !== JSON.stringify(right[key])) {
      throw new Error(
        `Conflicting ${key} for duplicated Business OS collection ${collection}: `
        + `${JSON.stringify(left[key])} vs ${JSON.stringify(right[key])}`,
      );
    }
  }

  const merged = { ...left, ...right };
  merged.properties = mergeProperties(left.properties, right.properties);
  merged.required = mergeStringList(left.required, right.required);
  merged.indexes = mergeIndexes(left.indexes, right.indexes);
  merged.internalIndexes = mergeIndexes(left.internalIndexes, right.internalIndexes);
  merged.encrypted = mergeStringList(left.encrypted, right.encrypted);

  for (const key of ['additionalProperties', 'keyCompression']) {
    if (left[key] !== undefined && right[key] !== undefined && left[key] !== right[key]) {
      throw new Error(
        `Conflicting ${key} for duplicated Business OS collection ${collection}: `
        + `${JSON.stringify(left[key])} vs ${JSON.stringify(right[key])}`,
      );
    }
    if (left[key] !== undefined) merged[key] = left[key];
  }

  return stripEmptyOptionalArrays(merged);
}

function mergeProperties(left = {}, right = {}) {
  const merged = {};
  for (const key of Object.keys(left || {})) merged[key] = cloneJson(left[key]);
  for (const [key, value] of Object.entries(right || {})) {
    if (!merged[key]) {
      merged[key] = cloneJson(value);
      continue;
    }
    merged[key] = mergePropertySchema(merged[key], value);
  }
  return merged;
}

function mergePropertySchema(left, right) {
  if (!left || typeof left !== 'object' || !right || typeof right !== 'object') {
    return schemaScore(right) > schemaScore(left) ? cloneJson(right) : cloneJson(left);
  }
  if (left.type !== undefined && right.type !== undefined && JSON.stringify(left.type) !== JSON.stringify(right.type)) {
    return schemaScore(right) > schemaScore(left) ? cloneJson(right) : cloneJson(left);
  }
  const merged = { ...cloneJson(left), ...cloneJson(right) };
  if (left.properties || right.properties) {
    merged.properties = mergeProperties(left.properties, right.properties);
  }
  if (left.items || right.items) {
    merged.items = mergePropertySchema(left.items || {}, right.items || {});
  }
  if (left.required || right.required) {
    merged.required = mergeStringList(left.required, right.required);
  }
  return schemaScore(merged) >= Math.max(schemaScore(left), schemaScore(right))
    ? merged
    : (schemaScore(right) > schemaScore(left) ? cloneJson(right) : cloneJson(left));
}

function mergeStringList(left = [], right = []) {
  const merged = [];
  for (const value of [...asArray(left), ...asArray(right)]) {
    if (typeof value !== 'string' || merged.includes(value)) continue;
    merged.push(value);
  }
  return merged;
}

function mergeIndexes(left = [], right = []) {
  const merged = [];
  const seen = new Set();
  for (const index of [...asArray(left), ...asArray(right)]) {
    const normalized = normalizeIndex(index);
    if (!normalized.length) continue;
    const key = JSON.stringify(normalized);
    if (seen.has(key)) continue;
    seen.add(key);
    merged.push(normalized.length === 1 ? normalized[0] : normalized);
  }
  return merged;
}

function normalizeIndex(index) {
  if (typeof index === 'string') return [index];
  if (Array.isArray(index)) return index.map((field) => String(field || '').trim()).filter(Boolean);
  return [];
}

function asArray(value) {
  return Array.isArray(value) ? value : [];
}

function stripEmptyOptionalArrays(schema) {
  for (const key of ['required', 'indexes', 'internalIndexes', 'encrypted']) {
    if (Array.isArray(schema[key]) && schema[key].length === 0) delete schema[key];
  }
  return schema;
}

function schemaScore(value) {
  if (value == null) return 0;
  if (Array.isArray(value)) return value.reduce((score, item) => score + schemaScore(item), 1);
  if (typeof value !== 'object') return 1;
  let score = Object.keys(value).length;
  for (const child of Object.values(value)) score += schemaScore(child);
  return score;
}

function cloneJson(value) {
  return value == null ? value : JSON.parse(JSON.stringify(value));
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
    const schemaUrl = new URL(SCHEMA_FILE, dirUrl);
    if (!(await fileExists(schemaUrl))) {
      if (collections.length && !isInstalledOnlyRuntimeSchemaPath({ manifest, dirUrl })) {
        throw new Error(`${entry}/module.json declares collections but ${schemaUrl.pathname} is missing.`);
      }
      continue;
    }
    modules.push({
      id: entry,
      dirUrl,
      manifest,
      installScope,
      collections,
      schemaUrl,
    });
  }
  if (!modules.length) {
    throw new Error(`No Business OS modules discovered under ${MODULES_ROOT.pathname}`);
  }
  return modules;
}

async function readSchemaDocument(url, moduleId) {
  let raw;
  try {
    raw = JSON.parse(await readFile(url, 'utf8'));
  } catch (error) {
    throw new Error(`Failed to read ${SCHEMA_FILE} for ${moduleId}: ${error?.message || error}`);
  }
  if (!raw || typeof raw !== 'object' || !raw.collections || typeof raw.collections !== 'object' || Array.isArray(raw.collections)) {
    throw new Error(`${moduleId}/${SCHEMA_FILE} has no collections object.`);
  }
  return normalizeCollectionsSchema(raw);
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
