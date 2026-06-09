#!/usr/bin/env node
import { readFile, writeFile } from 'node:fs/promises';

const MODULES = [
  'ctox',
  'desktop',
  'browser',
  'documents',
  'conversations',
  'customers',
  'outbound',
  'knowledge',
  'creator',
  'shiftflow',
  'spreadsheets',
  'notes',
  'research',
  'app-store',
  'iot',
  'matching',
  'reports',
  'tickets',
];

const OUTPUT_PATH = new URL('../../business_os/business_os_schema_contract.json', import.meta.url);

async function buildContract() {
  const schemas = {};
  for (const name of MODULES) {
    const mod = await importSchemaModule(new URL(`../../../apps/business-os/modules/${name}/schema.js`, import.meta.url));
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

async function importSchemaModule(url) {
  const source = await readFile(url, 'utf8');
  const reexport = source.match(/^\s*export\s+\{\s*collections\s*,\s*migrationStrategies\s*\}\s+from\s+['"]([^'"]+)['"];\s*$/m);
  if (reexport) {
    return importSchemaModule(new URL(reexport[1], url));
  }
  const transformed = source
    .replace(/\bexport\s+const\s+collections\s*=/, 'const collections =')
    .replace(/\bexport\s+const\s+migrationStrategies\s*=/, 'const migrationStrategies =');
  return Function(`
    ${transformed}
    return {
      collections: typeof collections === 'undefined' ? {} : collections,
      migrationStrategies: typeof migrationStrategies === 'undefined' ? {} : migrationStrategies,
    };
  `)();
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
