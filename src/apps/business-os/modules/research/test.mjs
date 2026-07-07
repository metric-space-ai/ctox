import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
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

const { __researchTestHooks: hooks } = await importBrowserBundle('./index.js');

const bases = [
  { domain: 'research/vendor-ai-agents', title: 'Vendor AI Agents' },
];

test('create dialog validation requires title, local domain, and task prompt', () => {
  assert.equal(hooks.validateResearchTaskInput({ title: '', domain: bases[0].domain, prompt: 'Analyse' }, bases).valid, false);
  assert.equal(hooks.validateResearchTaskInput({ title: 'Vendor Research', domain: 'research/missing', prompt: 'Analyse' }, bases).valid, false);
  assert.equal(hooks.validateResearchTaskInput({ title: 'Vendor Research', domain: bases[0].domain, prompt: '' }, bases).valid, false);
  assert.equal(hooks.validateResearchTaskInput({ title: 'Vendor Research', domain: bases[0].domain, prompt: 'Analyse vendors' }, bases).valid, true);
});

test('create task preserves selected local knowledge domain ids', () => {
  const knowledgeBases = [{ domain: 'drone_bearing_design', title: 'Drone Bearing Design' }];

  assert.equal(
    hooks.researchDomainFromFormValue('drone_bearing_design', knowledgeBases, 'Fallback Research'),
    'drone_bearing_design',
  );
  assert.equal(
    hooks.researchDomainFromFormValue('Vendor Research', knowledgeBases, 'Fallback Research'),
    'research/vendor-research',
  );
});

test('run button validation requires a selected task with a loaded knowledge domain', () => {
  assert.equal(hooks.validateSelectedResearchTask(null, bases).valid, false);
  assert.equal(hooks.validateSelectedResearchTask({ id: 'task-1', title: 'Vendor Research', knowledge_domain: '' }, bases).valid, false);
  assert.equal(hooks.validateSelectedResearchTask({ id: 'task-1', title: 'Vendor Research', knowledge_domain: 'research/missing' }, bases).valid, false);
  assert.equal(hooks.validateSelectedResearchTask({ id: 'task-1', title: 'Vendor Research', knowledge_domain: bases[0].domain }, bases).valid, true);
});

test('diagnostic rows distinguish sync failures from local no-data', () => {
  const rows = hooks.collectionDiagnosticRows(['research_runs', 'research_notes', 'knowledge_tables'], {
    research_runs: { sync: { kind: 'failed', message: 'WebRTC replication failed' } },
    research_notes: { sync: { kind: 'local', message: 'Lokaler Modus' } },
    knowledge_tables: { read: { kind: 'ok', message: '0 rows' } },
  });

  assert.deepEqual(rows.map((row) => row.kind), ['failed', 'local', 'ok']);
  assert.match(rows[0].label, /WebRTC/);
});

test('knowledge base grouping ignores legacy parquet docs without domain and table key', () => {
  const grouped = hooks.knowledgeBasesFromTables([
    {
      id: 'parquet:legacy-source-catalog',
      payload: {
        id: 'parquet:legacy-source-catalog',
        title: 'source catalog',
        parquet_path: '/runtime/knowledge/data/drone_bearing_design/source_catalog.parquet',
      },
    },
    {
      id: 'table:source-catalog',
      payload: {
        id: 'table:source-catalog',
        domain: 'drone_bearing_design',
        table_key: 'source_catalog',
        row_count: 22,
        title: 'Source catalog for drone bearing design load data',
      },
    },
  ]);

  assert.deepEqual(grouped.map((base) => base.domain), ['drone_bearing_design']);
  assert.equal(grouped[0].tables.length, 1);
});

test('empty knowledge read retries only when knowledge_tables sync is live', () => {
  const previousWindow = globalThis.window;
  try {
    globalThis.window = { ctoxBusinessOsSyncDiagnostics: { collections: {} } };
    assert.equal(hooks.shouldRetryEmptyKnowledgeTables(), true);

    globalThis.window.ctoxBusinessOsSyncDiagnostics.collections.knowledge_tables = { status: 'connected' };
    assert.equal(hooks.shouldRetryEmptyKnowledgeTables(), true);

    globalThis.window.ctoxBusinessOsSyncDiagnostics.collections.knowledge_tables = { initialReplicationState: 'complete' };
    assert.equal(hooks.shouldRetryEmptyKnowledgeTables(), true);
  } finally {
    if (previousWindow === undefined) {
      delete globalThis.window;
    } else {
      globalThis.window = previousWindow;
    }
  }
});

test('empty dashboard keeps standard header and disabled workbench controls', () => {
  const markup = hooks.renderNoTaskCenter();

  assert.match(markup, /ctox-pane-header ctox-pane-band research-center-header/);
  assert.match(markup, /data-action="refresh"/);
  assert.match(markup, /data-action="new-task"/);
  assert.match(markup, /research-empty-workbench/);
  assert.match(markup, /disabled/);
  assert.match(markup, /Quellensuche|Source search/);
  assert.doesNotMatch(markup, /Reload Diagnose|Collection|Sync-Diagnosen|rows/);
});

test('research module catalog grants knowledge and document collections', async () => {
  const moduleJson = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));
  const registryJson = JSON.parse(await readFile(new URL('../registry.json', import.meta.url), 'utf8'));
  const registryModule = registryJson.modules.find((item) => item.id === 'research');
  const required = [
    'business_commands',
    'business_chats',
    'ctox_queue_tasks',
    'research_tasks',
    'research_runs',
    'research_notes',
    'knowledge_tables',
    'documents',
    'document_versions',
    'document_blob_chunks',
  ];

  assert.ok(registryModule, 'registry exposes the research module');
  assert.deepEqual(moduleJson.collections, required);
  assert.deepEqual(registryModule.collections, required);
});
