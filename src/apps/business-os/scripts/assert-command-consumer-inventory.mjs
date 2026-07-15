import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(scriptDir, '..');
const inventoryPath = path.join(scriptDir, 'command-consumer-inventory.json');
const inventory = JSON.parse(fs.readFileSync(inventoryPath, 'utf8'));

const runtimeFiles = walk(appRoot)
  .filter((file) => /\.(?:js|mjs)$/.test(file))
  .filter((file) => !excluded(relative(file)));

const dispatchConsumers = matchingFiles(/commandBus(?:\?\.|\.)dispatch\b|requireCommandBus\([^)]*\)\.dispatch\b/);
const projectionReaders = matchingFiles(
  /(?:collection\?*\.\(['"]business_commands['"]\)|\.business_commands\b|activeCollection\(['"]business_commands['"]\)|documentCollection\([^\n]+['"]business_commands['"]\))/,
);
const legacyProjectionWaiters = matchingFiles(/function\s+waitForCommandProjection\s*\(/);
const directIntentWriters = runtimeFiles
  .filter((file) => {
    const source = fs.readFileSync(file, 'utf8');
    return /(?:businessCommandsCollection|commandsCollection|business_commands)(?:\?\.|\.)\s*(?:insert|upsert|incrementalUpsert)(?:\?\.)?\s*\(/.test(source)
      || /upsertDoc\s*\(\s*[^,\n]{0,240}business_commands/.test(source);
  })
  .map(relative)
  .sort();

assertExact('dispatch_consumers', dispatchConsumers);
assertExact('projection_readers', projectionReaders);
assertExact('legacy_projection_waiters', legacyProjectionWaiters);
assertExact('direct_intent_writers', directIntentWriters);

console.log('Business OS command consumer inventory OK', {
  dispatchConsumers: dispatchConsumers.length,
  projectionReaders: projectionReaders.length,
  legacyProjectionWaiters: legacyProjectionWaiters.length,
  directIntentWriters: directIntentWriters.length,
});

function matchingFiles(pattern) {
  return runtimeFiles
    .filter((file) => pattern.test(fs.readFileSync(file, 'utf8')))
    .map(relative)
    .sort();
}

function assertExact(field, actual) {
  const expected = [...(inventory[field] || [])].sort();
  if (JSON.stringify(actual) === JSON.stringify(expected)) return;
  const missing = expected.filter((file) => !actual.includes(file));
  const unexpected = actual.filter((file) => !expected.includes(file));
  throw new Error(`${field} drifted\nmissing: ${missing.join(', ') || '-'}\nunexpected: ${unexpected.join(', ') || '-'}`);
}

function relative(file) {
  return path.relative(appRoot, file).split(path.sep).join('/');
}

function excluded(file) {
  return file.startsWith('rxdb/dist/')
    || file.startsWith('rxdb/src/')
    || file.startsWith('rxdb/tests/')
    || file.startsWith('scripts/')
    || file.endsWith('.test.js')
    || file.endsWith('.test.mjs')
    || file.endsWith('/test.mjs')
    || file.endsWith('/schema.js');
}

function walk(directory) {
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...walk(target));
    else files.push(target);
  }
  return files;
}
