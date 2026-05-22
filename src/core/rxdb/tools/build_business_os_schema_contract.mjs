#!/usr/bin/env node
import { readFile, writeFile } from 'node:fs/promises';

const MODULES = [
  'ctox',
  'desktop',
  'documents',
  'conversations',
  'outbound',
  'knowledge',
  'creator',
  'shiftflow',
  'spreadsheets',
  'notes',
  'research',
  'app-store',
  'matching',
  'reports',
];

const OUTPUT_PATH = new URL('../../business_os/business_os_schema_contract.json', import.meta.url);

async function buildContract() {
  const schemas = {};
  for (const name of MODULES) {
    const mod = await import(`../../../apps/business-os/modules/${name}/schema.js`);
    for (const [collection, definition] of Object.entries(mod.collections || {})) {
      if (schemas[collection]) continue;
      schemas[collection] = definition?.schema || definition;
    }
  }

  const ordered = {};
  for (const key of Object.keys(schemas).sort()) {
    ordered[key] = schemas[key];
  }
  return `${JSON.stringify(ordered, null, 2)}\n`;
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
