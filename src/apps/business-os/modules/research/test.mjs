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
const researchSource = await readFile(new URL('./index.js', import.meta.url), 'utf8');

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

test('knowledge refresh contract preserves living research lineage and source provenance', () => {
  const task = { id: 'task-1', title: 'Bearing loads', knowledge_domain: 'drone_bearing_design' };
  const base = { tables: [
    { id: 'table:sources', table_key: 'source_catalog' },
    { id: 'table:evidence', table_key: 'evidence_points' },
  ] };
  const payload = hooks.knowledgeRefreshPayload(task, base, { id: 'run-7' });

  assert.equal(payload.update_mode, 'upsert');
  assert.equal(payload.research_run_id, 'run-7');
  assert.equal(payload.knowledge_contract.provenance_required, true);
  assert.equal(payload.knowledge_contract.source_of_truth, 'original_sources');
  assert.deepEqual(payload.writeback_contract.lineage.table_ids, ['table:sources', 'table:evidence']);
  assert.match(payload.instruction, /source_id\/source_url/);
});

test('UI evidence gate scores only verified, snapshotted, non-aggregated 2xx sources', () => {
  const task = {
    title: 'Drone bearing loads',
    prompt: 'Compare rotor load evidence',
    criteria: 'Traceable source evidence',
    knowledge_domain: 'drone_bearing_design',
  };
  const valid = {
    source_id: 'valid',
    title: 'Verified rotor load dataset',
    source_type: 'dataset',
    source_url: 'https://example.test/valid',
    verification_status: 'verified',
    http_status: 200,
    snapshot_hash: 'sha256:valid',
    evidence_eligible: true,
    source_tier: 'primary',
  };
  const rows = [
    valid,
    { ...valid, source_id: 'not-found', title: '404 candidate', http_status: 404 },
    { ...valid, source_id: 'metadata', title: 'Metadata only candidate', metadata_only: true },
    { ...valid, source_id: 'off-topic', title: 'Fachfremde candidate', relevance_status: 'fachfremd' },
    { ...valid, source_id: 'rejected', title: 'Rejected candidate', verification_status: 'rejected', review_status: 'rejected' },
    { ...valid, source_id: 'aggregated', title: 'Aggregated candidate', source_tier: 'aggregated' },
    { source_id: 'legacy', title: 'Legacy candidate', source_url: 'https://example.test/legacy' },
  ];
  const models = hooks.buildSourceModels(task, rows, [], []);
  const byId = new Map(models.map((model) => [model.id, model]));

  assert.equal(byId.get('valid').evidenceEligible, true);
  assert.ok(byId.get('valid').score > 4);
  assert.notEqual(byId.get('valid').dimensions.evidence_strength, null);

  for (const id of ['not-found', 'metadata', 'off-topic', 'rejected', 'aggregated', 'legacy']) {
    const model = byId.get(id);
    assert.equal(model.evidenceEligible, false, id);
    assert.equal(model.score, null, id);
    assert.equal(model.grade, '—', id);
    assert.equal(model.dimensions.evidence_strength, null, id);
    assert.match(model.evidenceStatusLabel, /HTTP 404|Metadata only|Rejected|Aggregated|Legacy|not verified/i, id);
  }
  assert.deepEqual(models.filter((model) => model.evidenceEligible).map((model) => model.id), ['valid']);
  assert.equal(hooks.formatPortfolioScore(null), '—');
  assert.equal(hooks.formatDimensionScore(null), '—');
});

test('evidence graph filtering removes unverified source nodes and provenance', () => {
  const filtered = hooks.filterGraphRowsForEvidence([
    { node_id: 'source:verified', label: 'Verified', source_ids_json: '["verified"]' },
    { node_id: 'source:legacy', label: 'Legacy', source_ids_json: '["legacy"]' },
    { node_id: 'concept:load', label: 'Load', source_ids_json: '["verified","legacy"]' },
    { node_id: 'concept:task', label: 'Task' },
  ], [
    { edge_id: 'valid-edge', source_id: 'source:verified', target_id: 'concept:load', source_ids_json: '["verified"]' },
    { edge_id: 'legacy-edge', source_id: 'source:legacy', target_id: 'concept:load', source_ids_json: '["legacy"]' },
  ], new Set(['verified']));

  assert.deepEqual(filtered.nodes.map((row) => row.node_id), ['source:verified', 'concept:load', 'concept:task']);
  assert.equal(filtered.nodes.find((row) => row.node_id === 'concept:load').source_ids_json, '["verified"]');
  assert.deepEqual(filtered.edges.map((row) => row.edge_id), ['valid-edge']);
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

test('initial research loading cannot masquerade as an empty knowledge base', () => {
  assert.match(researchSource, /initialDataReady: false/);
  assert.match(researchSource, /await waitForReplicationBridge\(bridge, collection\)/);
  assert.match(researchSource, /if \(!state\.initialDataReady\)[\s\S]*?Research-Daten werden mit dieser Instanz synchronisiert/);
  assert.match(researchSource, /await refreshAll\(\{ seed: true, mountToken \}\)[\s\S]*?state\.initialDataReady = true/);
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
  assert.equal(moduleJson.launch_kind, 'desktop-app');
  assert.equal(moduleJson.layout.shell, 'windowed');
  assert.equal(moduleJson.presentation.default_mode, 'window');
  assert.equal(registryModule.launch_kind, 'desktop-app');
  assert.equal(registryModule.layout.shell, 'windowed');
});

test('presentation layer stays compact and shell-native', async () => {
  const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
  const source = `${css}\n${await readFile(new URL('./index.js', import.meta.url), 'utf8')}`;
  const forbiddenSurfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'Prem' + 'ium', 'gla' + 'ss'].join('|'), 'i');

  assert.doesNotMatch(source, forbiddenSurfacePattern);
  assert.doesNotMatch(source, /border-(?:left|right)\s*:\s*(?:[2-9]|[0-9]{2,})px/);
  assert.doesNotMatch(source, /border-radius:\s*(?:8|10|12|14|16|18|20|24)px/);
  assert.doesNotMatch(source, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
  assert.doesNotMatch(source, /linear-gradient|radial-gradient/);
  assert.match(css, /grid-template-columns: var\(--research-left-width\) 6px minmax\(0, 1fr\) 6px var\(--research-right-width\)/);
  assert.match(css, /\.research-ai-prompt-pre/);
  assert.match(css, /@keyframes research-spin/);
});
