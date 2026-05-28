import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

async function importBrowserBundle(relativePath) {
  const bundledModule = await build({
    entryPoints: [fileURLToPath(new URL(relativePath, import.meta.url))],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    write: false,
  });

  const [{ text: bundledSource }] = bundledModule.outputFiles;
  return import(`data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`);
}

const { __knowledgeTestHooks: hooks } = await importBrowserBundle('./index.js');

const {
  buildKnowledgeBundles,
  canEditSelectedMarkdown,
  isKnowledgeActionFormReady,
  isKnowledgeTabDisabled,
  knowledgeItemsFromTables,
  localDataFrameRows,
  localDataFrameSchema,
  mergeKnowledgeTableData,
  normalizeStoredKnowledgeRecord,
  sourceScopeFor,
  valueForColumn,
} = hooks;

const tests = [];
function test(name, fn) {
  tests.push({ name, fn });
}

test('groups unknown knowledge records instead of rendering a false empty state', () => {
  const groups = buildKnowledgeBundles([
    {
      id: 'note:ops-runner',
      kind: 'note',
      title: 'Ops Runner Notes',
      subtitle: 'User · Operations',
      summary: 'Operational knowledge that is not a skillbook.',
    },
  ], [], []);

  assert.equal(groups.length, 1);
  assert.equal(groups[0].id, 'knowledge/operations');
  assert.equal(groups[0].entries[0].id, 'note:ops-runner');
});

test('projects knowledge table records into visible dataframe entries', () => {
  const groups = buildKnowledgeBundles([], [], [{
    id: 'table:load-points',
    kind: 'dataframe',
    title: 'Measured load points',
    payload: {
      domain: 'drone_bearing_design',
      rows: [{ measurement_id: 'MLP-001', thrust_N: 3.2 }],
      schema: { columns: [{ name: 'measurement_id', type: 'string' }, { name: 'thrust_N', type: 'number' }] },
    },
  }]);

  assert.equal(groups.length, 1);
  assert.equal(groups[0].id, 'tables/drone_bearing_design');
  assert.equal(groups[0].entries[0].id, 'table:load-points');
  assert.equal(groups[0].entries[0].has_table, true);
  assert.deepEqual(groups[0].tableIds, ['table:load-points']);
});

test('normalizes RxDB payload records without dropping table rows or schema', () => {
  const record = normalizeStoredKnowledgeRecord({
    id: 'table:source-catalog',
    title: 'Source catalog',
    has_table: true,
    payload: {
      id: 'table:source-catalog',
      title: 'Payload title',
      rows: [{ source_id: 'NASA-MTB2' }],
      schema: { columns: [{ name: 'source_id', type: 'string' }] },
    },
  });

  assert.equal(record.title, 'Source catalog');
  assert.equal(record.has_table, true);
  assert.equal(localDataFrameRows(record).length, 1);
  assert.equal(localDataFrameSchema(record).columns[0].key, 'source_id');
});

test('merges item metadata with table payload data for dataframe rendering', () => {
  const [tableItem] = knowledgeItemsFromTables([{
    id: 'table:metrics',
    payload: {
      rows: [{ metric_id: 'm1', score: 88 }],
      schema: { columns: [{ name: 'metric_id' }, { name: 'score' }] },
      title: 'Payload metrics',
    },
  }]);
  const merged = mergeKnowledgeTableData({ id: 'table:metrics', title: 'Metrics', has_table: true }, tableItem);

  assert.equal(merged.title, 'Metrics');
  assert.equal(localDataFrameRows(merged)[0].score, 88);
  assert.equal(localDataFrameSchema(merged).columns.length, 2);
  assert.equal(valueForColumn({ score_value: 91 }, { key: 'score_value', label: 'Score' }), 91);
});

test('source filters classify user and system knowledge', () => {
  assert.equal(sourceScopeFor({ source_path: 'embedded:skills/system/drone.md' }), 'system');
  assert.equal(sourceScopeFor({ source_system: 'ctox_core' }), 'system');
  assert.equal(sourceScopeFor({ source_path: 'workspace/knowledge/customer.md' }), 'user');
});

test('runbooks and data tabs are disabled without a selected knowledge item', () => {
  assert.equal(isKnowledgeTabDisabled('skill', ''), false);
  assert.equal(isKnowledgeTabDisabled('runbooks', ''), true);
  assert.equal(isKnowledgeTabDisabled('data', ''), true);
  assert.equal(isKnowledgeTabDisabled('data', 'skill:drone'), false);
});

test('edit markdown requires an existing selected item', () => {
  const items = [{ id: 'skill:drone', title: 'Drone Skill' }];
  assert.equal(canEditSelectedMarkdown('', items), false);
  assert.equal(canEditSelectedMarkdown('missing', items), false);
  assert.equal(canEditSelectedMarkdown('skill:drone', items), true);
});

test('action dialogs require non-empty required fields before submit', () => {
  assert.equal(isKnowledgeActionFormReady({ title: '' }, ['title']), false);
  assert.equal(isKnowledgeActionFormReady({ title: '  ' }, ['title']), false);
  assert.equal(isKnowledgeActionFormReady({ title: 'Customer Knowledge' }, ['title']), true);
  assert.equal(isKnowledgeActionFormReady({ destination: '' }, ['destination']), false);
  assert.equal(isKnowledgeActionFormReady({ destination: 'runtime/knowledge/exports/' }, ['destination']), true);
});

let passed = 0;
for (const entry of tests) {
  try {
    await entry.fn();
    passed += 1;
    console.log(`ok - ${entry.name}`);
  } catch (error) {
    console.error(`not ok - ${entry.name}`);
    throw error;
  }
}

console.log(`${passed} knowledge tests passed`);
