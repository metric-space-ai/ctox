import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { CtoxIndexedDbCollection } from '../dist/ctox-rxdb-js.mjs';

const testDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(testDir, '../..');

const representativePlans = [
  {
    collection: 'business_commands',
    schema: {
      primaryKey: 'id',
      indexes: [
        ['status', 'updated_at_ms'],
        ['module_id', 'type', 'status', 'updated_at_ms'],
        'command_id',
      ],
    },
    query: {
      selector: { status: 'pending' },
      sort: [{ updated_at_ms: 'desc' }],
      limit: 50,
    },
    expectedStrategy: 'schema-index',
  },
  {
    collection: 'ctox_queue_tasks',
    schema: {
      primaryKey: 'id',
      indexes: [
        ['status', 'updated_at_ms'],
        ['command_id', 'updated_at_ms'],
      ],
    },
    query: {
      selector: { status: 'running' },
      sort: [{ updated_at_ms: 'desc' }],
      limit: 50,
    },
    expectedStrategy: 'schema-index',
  },
  {
    collection: 'desktop_files',
    schema: {
      primaryKey: 'id',
      indexes: [
        ['parent_id', 'name'],
        ['source_root', 'path'],
      ],
    },
    query: {
      selector: { parent_id: 'root' },
      sort: [{ name: 'asc' }],
      limit: 100,
    },
    expectedStrategy: 'schema-index',
  },
  {
    collection: 'desktop_file_chunks',
    schema: {
      primaryKey: 'id',
      indexes: [
        ['file_id', 'generation_id', 'idx'],
      ],
    },
    query: {
      selector: { file_id: 'file-1', generation_id: 'gen-1' },
      sort: [{ idx: 'asc' }],
      limit: 256,
    },
    expectedStrategy: 'schema-index',
  },
  {
    collection: 'business_records',
    schema: {
      primaryKey: 'id',
      indexes: [
        ['collection', 'updated_at_ms'],
      ],
    },
    query: {
      selector: { collection: 'customers' },
      sort: [{ updated_at_ms: 'desc' }],
      limit: 100,
    },
    expectedStrategy: 'schema-index',
  },
];

for (const { collection, schema, query, expectedStrategy } of representativePlans) {
  const storageCollection = new CtoxIndexedDbCollection(null, collection, { schema });
  const plan = storageCollection.queryPlanFor(query);
  assert(
    plan.strategy === expectedStrategy,
    `${collection} representative query must use ${expectedStrategy}, got ${plan.strategy}`,
  );
  assert(
    plan.allDocumentsFallback === false,
    `${collection} representative query must not fall back to allDocuments()`,
  );
  assert(
    plan.selectedIndex?.name,
    `${collection} representative query must expose the selected IndexedDB schema index`,
  );
}

const strictCollection = new CtoxIndexedDbCollection(null, 'business_commands', {
  schema: representativePlans[0].schema,
});
strictCollection.setQueryPerformancePolicy({ rejectAllDocumentsFallback: true });
await assertRejects(
  () => strictCollection.queryDocuments({
    selector: { status: { $regex: '^pending' } },
    sort: [{ updated_at_ms: 'desc' }],
  }),
  (error) => error?.code === 'CTOX_INDEXEDDB_ALL_DOCUMENTS_FALLBACK',
  'strict browser query-performance policy must reject unexpected allDocuments() fallback before opening IndexedDB',
);
const stats = strictCollection.getQueryPerformanceStats();
assert(stats.allDocumentsFallbackCalls === 1, 'strict fallback rejection must increment allDocumentsFallbackCalls');
assert(stats.allDocumentsCalls === 0, 'strict fallback rejection must not execute allDocuments()');
assert(
  stats.lastAllDocumentsFallback?.collection === 'business_commands',
  'strict fallback rejection must retain collection attribution',
);
assert(
  stats.lastAllDocumentsFallback?.selectorFields?.join(',') === 'status',
  'strict fallback rejection must retain selector-field attribution',
);
assert(
  stats.lastAllDocumentsFallback?.sortFields?.join(',') === 'updated_at_ms',
  'strict fallback rejection must retain sort-field attribution',
);

const sourceOffenders = [];
for (const file of sourceFiles([
  join(appRoot, 'shared'),
  join(appRoot, 'modules'),
  join(appRoot, 'desktop-apps'),
])) {
  const text = readFileSync(file, 'utf8');
  if (/\.allDocuments\s*\(/.test(text)) {
    sourceOffenders.push(`${relativeAppPath(file)}: app code must not call storage allDocuments() directly`);
  }
  if (/\bdesktop_file_chunks\b[\s\S]{0,200}\.find\s*\(\s*(?:\{\s*\})?\s*\)\s*\.exec\s*\(/.test(text)) {
    sourceOffenders.push(`${relativeAppPath(file)}: desktop_file_chunks must use rxdb.file.fetch or keyed/ranged chunk reads, not broad find().exec()`);
  }
  if (/collection\s*\(\s*['"]desktop_file_chunks['"]\s*\)[\s\S]{0,160}\.find\s*\(\s*(?:\{\s*\})?\s*\)\s*\.exec\s*\(/.test(text)) {
    sourceOffenders.push(`${relativeAppPath(file)}: desktop_file_chunks collection lookups must not broad find().exec()`);
  }
}
assert(
  sourceOffenders.length === 0,
  `browser performance source guard found broad reads:\n${sourceOffenders.join('\n')}`,
);

console.log('ctox-rxdb-js browser query performance guards smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function assertRejects(fn, predicate, message) {
  try {
    await fn();
  } catch (error) {
    if (predicate(error)) return;
    throw new Error(`${message}: unexpected error ${error?.stack || error}`);
  }
  throw new Error(`${message}: expected rejection`);
}

function sourceFiles(roots) {
  const files = [];
  for (const root of roots) {
    walk(root, files);
  }
  return files.sort();
}

function walk(path, files) {
  const dependencyTreeDir = 'node' + '_modules';
  let stat;
  try {
    stat = statSync(path);
  } catch {
    return;
  }
  if (stat.isDirectory()) {
    for (const name of readdirSync(path)) {
      if (name === 'dist' || name === dependencyTreeDir) continue;
      walk(join(path, name), files);
    }
    return;
  }
  if (/\.(mjs|js|jsx)$/.test(path)) {
    files.push(path);
  }
}

function relativeAppPath(file) {
  return file.slice(appRoot.length + 1);
}
