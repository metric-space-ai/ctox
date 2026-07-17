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

test('measurement semantics never fall back to legacy radial load and retain zeroes', () => {
  assert.equal(hooks.tangentialEquivalentForce({ radial_load_N: 4 }), '');
  assert.equal(hooks.tangentialEquivalentForce({ radial_load_N: 4, tangential_equivalent_force_N: 0 }), 0);
  assert.equal(hooks.metricPropellerLength({ prop_diameter_mm: 0, prop_diameter_in: 9 }, 'prop_diameter'), 0);

  const measurements = hooks.aggregateMeasurements([
    { source_id: 'source-1', snapshot_id: 'snap-1', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', radial_load_N: 4, rpm: 0 },
    { source_id: 'source-1', snapshot_id: 'snap-1', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', tangential_equivalent_force_N: 0, force_N: 0 },
  ]);
  assert.equal(measurements.get('source-1').maxTangentialEquivalent, 0);
  assert.equal(measurements.get('source-1').maxRpm, 0);
});

test('measurement rows require individually matching source snapshot lineage', () => {
  const source = {
    id: 'source-1',
    evidenceEligible: true,
    row: {
      source_id: 'source-1',
      canonical_url: 'https://example.test/source-1',
      snapshot_id: 'snap-1',
      snapshot_hash: `sha256:${'1'.repeat(64)}`,
    },
  };
  const rows = [
    { source_id: 'source-1', snapshot_id: 'snap-1', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', force_N: 10 },
    { source_id: 'source-1', snapshot_id: '', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', force_N: 20 },
    { source_id: 'source-1', snapshot_id: 'snap-other', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', force_N: 30 },
    { source_id: 'source-1', snapshot_id: 'snap-1', snapshot_hash: `sha256:${'2'.repeat(64)}`, canonical_url: 'https://example.test/source-1', force_N: 40 },
    { source_id: 'source-1', snapshot_id: 'snap-1', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/other', force_N: 50 },
    { source_id: 'source-2', snapshot_id: 'snap-1', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', force_N: 60 },
  ];

  assert.equal(hooks.filterMeasurementRowsForEvidence(rows, [source]).length, 1);
  assert.equal(hooks.aggregateMeasurements(rows, [source]).get('source-1').count, 1);
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

test('systematic research pins every write to one immutable run and command', () => {
  assert.match(researchSource, /const researchRunId = `research_run_\$\{crypto\.randomUUID\(\)\}`/);
  assert.match(researchSource, /research_run_id: researchRunId/);
  assert.match(researchSource, /research_command_id: commandId/);
  assert.match(researchSource, /jede in diesem Lauf erzeugte oder aktualisierte Knowledge-Zeile research_run_id=/);
  assert.match(researchSource, /row_lineage_required/);

  const targetedStart = researchSource.indexOf('async function dispatchTargetedGraphResearch');
  const targetedEnd = researchSource.indexOf('function eligibleGraphFocusSourceIds', targetedStart);
  const targetedSource = researchSource.slice(targetedStart, targetedEnd);
  assert.ok(targetedStart >= 0 && targetedEnd > targetedStart);
  assert.match(targetedSource, /const researchRunId = `research_run_\$\{crypto\.randomUUID\(\)\}`/);
  assert.match(targetedSource, /research_run_id: researchRunId/);
  assert.match(targetedSource, /research_command_id: commandId/);
  assert.match(targetedSource, /row_lineage_required/);
  assert.match(targetedSource, /id: researchRunId/);
});

test('knowledge refresh contract preserves living research lineage and source provenance', () => {
  const task = { id: 'task-1', title: 'Bearing loads', knowledge_domain: 'drone_bearing_design' };
  const snapshotHash = `sha256:${'a'.repeat(64)}`;
  const base = { tables: [
    {
      id: 'table:sources', table_key: 'source_catalog', knowledge_version_id: 'knowledge-v7',
      knowledge_version: { version_id: 'knowledge-v7', status: 'current' },
      rows: [{ source_id: 'source-1', canonical_url: 'https://example.test/source-1', source_receipt_url: 'https://receipt.test/source-1', snapshot_id: 'snap-1', snapshot_hash: snapshotHash, verification_status: 'verified', transport_verified: true, content_extracted: true, actual_full_text_or_data: true, evidence_relevance_score: 9, http_status: 200, evidence_eligible: true, source_tier: 'primary' }],
    },
    { id: 'table:evidence', table_key: 'evidence_points', rows: [{ evidence_id: 'evidence-1', source_id: 'source-1', canonical_url: 'https://example.test/source-1', snapshot_id: 'snap-1', snapshot_hash: snapshotHash }] },
  ] };
  const payload = hooks.knowledgeRefreshPayload(task, base, { id: 'run-7', knowledge_version_id: 'knowledge-v7' });

  assert.equal(payload.update_mode, 'upsert');
  assert.equal(payload.research_run_id, 'run-7');
  assert.equal(payload.knowledge_contract.provenance_required, true);
  assert.equal(payload.knowledge_version_id, 'knowledge-v7');
  assert.equal(payload.knowledge_version.status, 'current');
  assert.equal(payload.knowledge_contract.source_of_truth, 'original_sources');
  assert.deepEqual(payload.writeback_contract.lineage.table_ids, ['table:sources', 'table:evidence']);
  assert.deepEqual(payload.requested_snapshot_hashes, [snapshotHash]);
  assert.equal(payload.source_lineage[0].source_id, 'source-1');
  assert.equal(payload.evidence_lineage[0].evidence_id, 'evidence-1');
  assert.match(payload.instruction, /source_id\/source_url/);
});

test('graph document lineage is native-contract-shaped and fail-closed', () => {
  const snapshotHash = `sha256:${'b'.repeat(64)}`;
  const source = {
    id: 'source-1',
    evidenceEligible: true,
    row: {
      source_id: 'source-1',
      canonical_url: 'https://example.test/source-1',
      source_receipt_url: 'https://receipt.test/source-1',
      snapshot_id: 'snap-1',
      snapshot_hash: snapshotHash,
    },
  };
  const base = {
    knowledge_version_id: 'knowledge-v9',
    knowledge_version: { version_id: 'knowledge-v9', status: 'current' },
    tables: [],
  };
  const lineage = hooks.graphDocumentLineage({ id: 'task-1' }, base, { id: 'run-9', knowledge_version_id: 'knowledge-v9' }, [source], ['source-1']);

  assert.equal(lineage.ok, true);
  assert.equal(lineage.knowledge_version_id, 'knowledge-v9');
  assert.deepEqual(lineage.requested_snapshot_hashes, [snapshotHash]);
  assert.deepEqual(lineage.source_receipts.map((receipt) => receipt.source_id), ['source-1']);
  assert.equal(lineage.evidence_lineage.source_receipts[0].receipt_url, 'https://receipt.test/source-1');
  assert.equal(hooks.graphDocumentLineage({ id: 'task-1' }, { tables: [] }, null, [source]).ok, false);
});

test('graph document lineage requires a persisted receipt locator and never uses canonical URL', () => {
  const snapshotHash = `sha256:${'c'.repeat(64)}`;
  const base = {
    knowledge_version_id: 'knowledge-v10',
    knowledge_version: { version_id: 'knowledge-v10', status: 'current' },
    tables: [],
  };
  const canonicalOnly = {
    id: 'source-canonical-only',
    evidenceEligible: true,
    row: {
      source_id: 'source-canonical-only',
      canonical_url: 'https://example.test/canonical-only',
      snapshot_id: 'snap-canonical-only',
      snapshot_hash: snapshotHash,
    },
  };
  const rejected = hooks.graphDocumentLineage({ id: 'task-1' }, base, null, [canonicalOnly]);
  assert.equal(rejected.ok, false);
  assert.match(rejected.reason, /receipt lineage/i);

  const receiptIdOnly = {
    ...canonicalOnly,
    id: 'source-receipt-id-only',
    row: {
      ...canonicalOnly.row,
      source_id: 'source-receipt-id-only',
      source_receipt_id: 'receipt-10',
    },
  };
  const accepted = hooks.graphDocumentLineage({ id: 'task-1' }, base, null, [receiptIdOnly]);
  assert.equal(accepted.ok, true);
  assert.equal(accepted.source_receipts[0].receipt_id, 'receipt-10');
  assert.equal(accepted.source_receipts[0].receipt_url, '');
  assert.notEqual(accepted.source_receipts[0].receipt_url, accepted.source_receipts[0].canonical_url);
});

test('systematic research scoring contract pins all source gates and independent audits', () => {
  const contract = hooks.researchScoringContract([{ id: 'evidence_strength', label: 'Evidence', weight: 1 }]);
  assert.deepEqual(contract.required_source_fields, [
    'verification_status',
    'transport_verified',
    'content_extracted',
    'actual_full_text_or_data',
    'evidence_relevance_score',
    'http_status',
    'snapshot_hash',
    'canonical_url',
    'evidence_eligible',
    'source_tier',
  ]);
  assert.deepEqual(contract.required_audits, ['source', 'data', 'claim']);
  assert.match(contract.rule, /canonical_url/);
});

test('knowledge lineage selects the latest run that actually used accepted evidence', () => {
  const runs = [
    { id: 'old-good', task_id: 'task-1', used_count: 3, updated_at_ms: 10 },
    { id: 'latest-empty', task_id: 'task-1', used_count: 0, accepted_count: 0, updated_at_ms: 30 },
    { id: 'other-task', task_id: 'task-2', used_count: 8, updated_at_ms: 40 },
  ];
  assert.equal(hooks.latestEvidenceRunForTask('task-1', runs)?.id, 'old-good');
  assert.equal(hooks.latestEvidenceRunForTask('missing', runs), null);
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
    transport_verified: true,
    content_extracted: true,
    http_status: 200,
    snapshot_hash: `sha256:${'a'.repeat(64)}`,
    canonical_url: 'https://example.test/valid',
    evidence_eligible: true,
    source_tier: 'primary',
    actual_full_text_or_data: true,
    evidence_relevance_score: 9,
  };
  const rows = [
    valid,
    { ...valid, source_id: 'not-found', title: '404 candidate', http_status: 404 },
    { ...valid, source_id: 'transport', title: 'Unverified transport', transport_verified: false },
    { ...valid, source_id: 'empty', title: 'Empty source shell', content_extracted: false },
    { ...valid, source_id: 'no-canonical', title: 'Canonical URL missing', canonical_url: '' },
    { ...valid, source_id: 'metadata', title: 'Metadata only candidate', metadata_only: true },
    { ...valid, source_id: 'off-topic', title: 'Fachfremde candidate', relevance_status: 'fachfremd' },
    { ...valid, source_id: 'rejected', title: 'Rejected candidate', verification_status: 'rejected', review_status: 'rejected' },
    { ...valid, source_id: 'aggregated', title: 'Aggregated candidate', source_tier: 'aggregated' },
    { ...valid, source_id: 'metadata-url', title: 'Metadata URL candidate', canonical_url: 'https://doi.org/10.1000/test' },
    { ...valid, source_id: 'bad-hash', title: 'Unbound snapshot candidate', snapshot_hash: 'sha256:valid' },
    { ...valid, source_id: 'no-fulltext', title: 'No original content', actual_full_text_or_data: false },
    { ...valid, source_id: 'weak-relevance', title: 'Weak relevance', evidence_relevance_score: 7 },
    { ...valid, source_id: 'explicit-rejection', title: 'Explicit rejection', evidence_rejection_reason: 'off_topic' },
    { source_id: 'legacy', title: 'Legacy candidate', source_url: 'https://example.test/legacy' },
  ];
  const models = hooks.buildSourceModels(task, rows, [], []);
  const byId = new Map(models.map((model) => [model.id, model]));

  assert.equal(byId.get('valid').evidenceEligible, true);
  assert.ok(byId.get('valid').score > 4);
  assert.notEqual(byId.get('valid').dimensions.evidence_strength, null);

  for (const id of ['not-found', 'transport', 'empty', 'no-canonical', 'metadata', 'off-topic', 'rejected', 'aggregated', 'metadata-url', 'bad-hash', 'no-fulltext', 'weak-relevance', 'explicit-rejection', 'legacy']) {
    const model = byId.get(id);
    assert.equal(model.evidenceEligible, false, id);
    assert.equal(model.score, null, id);
    assert.equal(model.grade, '—', id);
    assert.equal(model.dimensions.evidence_strength, null, id);
    assert.match(model.evidenceStatusLabel, /HTTP 404|Metadata|Rejected|Aggregated|Legacy|not verified|Transport not verified|No source content extracted|Canonical source missing|snapshot|full text|Relevance|Evidence rejected/i, id);
  }
  assert.deepEqual(models.filter((model) => model.evidenceEligible).map((model) => model.id), ['valid']);
  assert.equal(hooks.formatPortfolioScore(null), '—');
  assert.equal(hooks.formatDimensionScore(null), '—');
});

test('source table exposes canonical links only for evidence-eligible sources', () => {
  const markup = hooks.renderSourcesTable([
    {
      id: 'verified',
      title: 'Verified source',
      sourceClass: 'dataset',
      evidenceStatus: 'verified',
      evidenceStatusLabel: 'Verified',
      evidenceEligible: true,
      canonicalUrl: 'https://example.test/canonical',
      url: 'https://example.test/discovery',
      grade: 'A',
      score: 85,
      dimensions: {},
    },
    {
      id: 'candidate',
      title: 'Discovery candidate',
      sourceClass: 'web',
      evidenceStatus: 'unverified',
      evidenceStatusLabel: 'Not verified',
      evidenceEligible: false,
      canonicalUrl: 'https://example.test/unverified-canonical',
      url: 'https://example.test/unverified-discovery',
      grade: '—',
      score: null,
      dimensions: {},
    },
  ]);

  assert.match(markup, /https:\/\/example\.test\/canonical/);
  assert.doesNotMatch(markup, /https:\/\/example\.test\/discovery/);
  assert.doesNotMatch(markup, /unverified-canonical|unverified-discovery/);
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

test('targeted graph research carries only currently eligible source ids', () => {
  const sourceModels = [
    { id: 'verified', evidenceEligible: true },
    { id: 'legacy', evidenceEligible: false },
  ];
  assert.deepEqual(
    hooks.eligibleGraphFocusSourceIds({ sourceIds: ['verified', 'legacy', 'verified'] }, sourceModels),
    ['verified'],
  );
});

test('research reports contain only live documents with explicit task or domain lineage', () => {
  const task = { id: 'task-1', knowledge_domain: 'drone_bearing_design' };
  const reports = hooks.researchReportsForTask(task, [
    { id: 'task-report', title: 'Task report', filename: 'task.docx', linked_records: [{ kind: 'research_task', id: 'task-1' }], updated_at_ms: 20 },
    { id: 'domain-report', title: 'Domain report', filename: 'domain.docx', linked_records: [{ kind: 'knowledge_domain', id: 'drone_bearing_design' }], updated_at_ms: 30 },
    { id: 'unlinked-demo', title: 'Legacy demo', filename: 'legacy.md', linked_records: [], updated_at_ms: 40 },
    { id: 'deleted', title: 'Deleted', filename: 'deleted.docx', linked_records: [{ kind: 'research_task', id: 'task-1' }], is_deleted: true, updated_at_ms: 50 },
  ]);

  assert.deepEqual(reports.map((report) => report.id), ['domain-report', 'task-report']);
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
