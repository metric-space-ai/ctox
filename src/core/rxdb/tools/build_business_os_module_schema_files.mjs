#!/usr/bin/env node
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const SCHEMA_FORMAT = 'ctox-business-os-module-collections-v1';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, '../../../..');
const appRoot = resolve(repoRoot, 'src/apps/business-os');
const sourceRoots = [
  resolve(appRoot, 'modules'),
  resolve(appRoot, 'installed-modules'),
];

const write = process.argv.includes('--write');

const changed = [];
const missing = [];

for (const sourceRoot of sourceRoots) {
  if (!existsSync(sourceRoot)) continue;
  for (const name of await readdir(sourceRoot)) {
    const moduleDir = resolve(sourceRoot, name);
    const manifestPath = resolve(moduleDir, 'module.json');
    if (!existsSync(manifestPath)) continue;
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    const schemaJsPath = resolveSchemaJsPath(moduleDir, manifest, sourceRoot);
    const schemaJsonPath = resolve(moduleDir, 'collections.schema.json');
    if (!schemaJsPath) {
      if (!existsSync(schemaJsonPath)) {
        missing.push(relative(repoRoot, schemaJsonPath));
      }
      continue;
    }
    if (String(manifest.install_scope || '').toLowerCase() === 'sample') continue;
    const schemaModule = await import(pathToFileURL(schemaJsPath).href);
    const collections = normalizeCollections(schemaModule.collections || {});
    const existingDocument = existsSync(schemaJsonPath)
      ? JSON.parse(await readFile(schemaJsonPath, 'utf8'))
      : {};
    const document = {
      schema_format: SCHEMA_FORMAT,
      module_id: manifest.id || name,
      collections,
    };
    if (
      existingDocument.migration_strategies
      && typeof existingDocument.migration_strategies === 'object'
      && !Array.isArray(existingDocument.migration_strategies)
    ) {
      document.migration_strategies = existingDocument.migration_strategies;
    }
    const sortedDocument = sortObjectDeep(document);
    const next = `${JSON.stringify(sortedDocument, null, 2)}\n`;
    const current = existsSync(schemaJsonPath)
      ? await readFile(schemaJsonPath, 'utf8')
      : '';
    if (current !== next) {
      changed.push(relative(repoRoot, schemaJsonPath));
      if (write) {
        await mkdir(dirname(schemaJsonPath), { recursive: true });
        await writeFile(schemaJsonPath, next);
      }
    }
  }
}

if (!write && (changed.length || missing.length)) {
  const details = [
    ...changed.map((path) => `stale: ${path}`),
    ...missing.map((path) => `missing: ${path}`),
  ];
  console.error(`Business OS module schema files are stale. Run with --write.\n${details.join('\n')}`);
  process.exit(1);
}

console.log(write
  ? `Business OS module schema files written (${changed.length} changed).`
  : 'Business OS module schema files are current.');

function normalizeCollections(collections) {
  const normalized = {};
  for (const [collection, definition] of Object.entries(collections)) {
    const schema = definition?.schema || definition;
    assertJsonSerializable(schema, `collections.${collection}`);
    normalized[collection] = normalizeSchemaIndexes(schema);
  }
  return sortObjectDeep(normalized);
}

function resolveSchemaJsPath(moduleDir, manifest, sourceRoot) {
  const local = resolve(moduleDir, 'schema.js');
  if (existsSync(local)) return local;
  if (sourceRoot.endsWith('/installed-modules')) {
    const source = resolve(appRoot, 'modules', manifest.id || '', 'schema.js');
    if (existsSync(source)) return source;
  }
  return null;
}

function normalizeSchemaIndexes(schema) {
  const next = structuredClone(schema);
  if (Array.isArray(next.indexes)) {
    next.indexes = next.indexes.map((index) => Array.isArray(index) ? index : index);
  }
  return next;
}

function assertJsonSerializable(value, path) {
  if (typeof value === 'function' || typeof value === 'symbol' || typeof value === 'undefined') {
    throw new Error(`${path} contains non-JSON value ${typeof value}`);
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) => assertJsonSerializable(item, `${path}[${index}]`));
    return;
  }
  if (value && typeof value === 'object') {
    for (const [key, item] of Object.entries(value)) {
      assertJsonSerializable(item, `${path}.${key}`);
    }
  }
}

function sortObjectDeep(value) {
  if (Array.isArray(value)) return value.map(sortObjectDeep);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, sortObjectDeep(value[key])]),
    );
  }
  return value;
}
