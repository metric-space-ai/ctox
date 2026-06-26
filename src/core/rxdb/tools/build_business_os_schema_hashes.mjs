#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';

import {
  canonicalJson,
  normalizeSchema,
} from '../../../apps/business-os/rxdb/src/schema.mjs';

const CONTRACT_PATH = new URL('../../business_os/business_os_schema_contract.json', import.meta.url);
const HASHES_PATH = new URL('../../business_os/business_os_schema_hashes.json', import.meta.url);
const SCHEMA_JS_PATH = new URL('../../../apps/business-os/rxdb/src/schema.mjs', import.meta.url);

function sha256Hex(text) {
  return createHash('sha256').update(text).digest('hex');
}

async function buildHashes() {
  const contract = JSON.parse(await readFile(CONTRACT_PATH, 'utf8'));
  const hashes = {};
  for (const collection of Object.keys(contract).sort()) {
    hashes[collection] = sha256Hex(canonicalJson(normalizeSchema(contract[collection])));
  }
  return `${JSON.stringify(hashes, null, 2)}\n`;
}

function registrySourceFromHashes(hashesText) {
  const hashes = JSON.parse(hashesText);
  const entries = Object.keys(hashes)
    .sort()
    .map((collection) => `  ${collection}: '${hashes[collection]}',`)
    .join('\n');
  return `export const CTOX_BUSINESS_OS_SCHEMA_HASHES = Object.freeze({\n${entries}\n});`;
}

async function updateSchemaRegistry(hashesText) {
  const schemaSource = await readFile(SCHEMA_JS_PATH, 'utf8');
  const nextRegistry = registrySourceFromHashes(hashesText);
  const pattern = /export const CTOX_BUSINESS_OS_SCHEMA_HASHES = Object\.freeze\(\{\n[\s\S]*?\n\}\);/;
  if (!pattern.test(schemaSource)) {
    throw new Error('Could not find CTOX_BUSINESS_OS_SCHEMA_HASHES registry in schema.mjs');
  }
  const nextSource = schemaSource.replace(pattern, nextRegistry);
  await writeFile(SCHEMA_JS_PATH, nextSource);
}

const next = await buildHashes();
if (process.argv.includes('--write')) {
  await writeFile(HASHES_PATH, next);
  await updateSchemaRegistry(next);
  console.log(`wrote ${HASHES_PATH.pathname}`);
  console.log(`updated ${SCHEMA_JS_PATH.pathname}`);
} else {
  const currentHashes = await readFile(HASHES_PATH, 'utf8');
  const currentSchema = await readFile(SCHEMA_JS_PATH, 'utf8');
  const expectedRegistry = registrySourceFromHashes(next);
  const hashFileCurrent = currentHashes === next;
  const registryCurrent = currentSchema.includes(expectedRegistry);
  if (!hashFileCurrent || !registryCurrent) {
    console.error('Business OS schema hashes are stale. Run with --write.');
    if (!hashFileCurrent) console.error(`stale: ${HASHES_PATH.pathname}`);
    if (!registryCurrent) console.error(`stale: ${SCHEMA_JS_PATH.pathname}`);
    process.exit(1);
  }
  console.log('Business OS schema hashes are current.');
}
