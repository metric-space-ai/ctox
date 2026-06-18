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
  'buchhaltung',
  'calendar',
  'coding-agents',
  'iot',
  'matching',
  'reports',
  'tickets',
  'credentials',
  'consent',
  'intake',
  'submissions',
  'placements',
  'interviews',
  'esign',
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
  // Real ESM import instead of the old strip-exports/Function() transform:
  // module schema files may use any import/re-export pattern (full re-exports
  // like modules/reports, partial cross-module imports like
  // modules/conversations pulling the outbound-owned collections). The
  // emitted contract content is unchanged — same exported objects either way.
  const mod = await import(url.href);
  return {
    collections: mod.collections || {},
    migrationStrategies: mod.migrationStrategies || {},
  };
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
