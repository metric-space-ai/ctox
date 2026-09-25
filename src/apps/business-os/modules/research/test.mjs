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
const researchGraphSource = await readFile(new URL('./research-graph.mjs', import.meta.url), 'utf8');
const researchMarkup = await readFile(new URL('./index.html', import.meta.url), 'utf8');
const researchCss = await readFile(new URL('./index.css', import.meta.url), 'utf8');
const researchManifest = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));

const bases = [
  { domain: 'research/vendor-ai-agents', title: 'Vendor AI Agents' },
];

test('shell-v2 research keeps headers, icons, and overlays module-scoped', () => {
  assert.equal(researchManifest.layout.shell_contract, 'v2');
  assert.equal((researchMarkup.match(/data-shell-v2-header-row="1"/g) || []).length, 3);
  assert.match(researchSource, /research-task-dialog research-module-overlay/);
  assert.match(researchSource, /research-prompt-viewer research-module-overlay/);
  assert.match(researchSource, /const mountTarget = state\.ctx\?\.host\?\.querySelector\('\[data-research-root\]'\)/);
  assert.match(researchSource, /if \(mountTarget\) mountTarget\.appendChild\(backdrop\)/);
  assert.match(researchSource, /const RESEARCH_ICON_FALLBACK_PATHS = Object\.freeze/);
  assert.doesNotMatch(researchSource, /onclick="this\.closest\('\.ctox-modal'\)\.remove\(\)"/);
  assert.match(researchCss, /\.shell-window\[data-shell-contract="v2"\] \.research-module-overlay\s*\{[\s\S]*position:\s*absolute[\s\S]*inset:\s*0/);
});

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
    { source_id: 'source-1', evidence_id: 'evidence-1', snapshot_id: 'snap-1', snapshot_path: 'runtime/snapshots/source-1.html', retrieved_at: '2026-07-17T00:00:00Z', url_role: 'original_content', content_scope: 'full_text', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', radial_load_N: 4, rpm: 0 },
    { source_id: 'source-1', evidence_id: 'evidence-2', snapshot_id: 'snap-1', snapshot_path: 'runtime/snapshots/source-1.html', retrieved_at: '2026-07-17T00:00:00Z', url_role: 'original_content', content_scope: 'full_text', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', tangential_equivalent_force_N: 0, force_N: 0 },
  ]);
  assert.equal(measurements.get('source-1').maxTangentialEquivalent, 0);
  assert.equal(measurements.get('source-1').maxRpm, 0);
});

test('research tasks keep evidence and direct measurements in separate tables', () => {
  const base = {
    tables: [
      { table_key: 'evidence_points' },
      { table_key: 'measured_load_points' },
    ],
  };

  assert.equal(hooks.defaultMeasurementsTableKey(base), 'measured_load_points');
  assert.equal(hooks.defaultMeasurementsTableKey({ tables: [{ table_key: 'evidence_points' }] }), 'measured_load_points');
});

test('measurement rows require individually matching source snapshot lineage', () => {
  const source = {
    id: 'source-1',
    evidenceEligible: true,
    row: {
      source_id: 'source-1',
      evidence_id: 'evidence-1',
      canonical_url: 'https://example.test/source-1',
      snapshot_id: 'snap-1',
      snapshot_path: 'runtime/snapshots/source-1.html',
      retrieved_at: '2026-07-17T00:00:00Z',
      url_role: 'original_content',
      content_scope: 'full_text',
      snapshot_hash: `sha256:${'1'.repeat(64)}`,
    },
  };
  const rows = [
    { source_id: 'source-1', evidence_id: 'evidence-1', snapshot_id: 'snap-1', snapshot_path: 'runtime/snapshots/source-1.html', retrieved_at: '2026-07-17T00:00:00Z', url_role: 'original_content', content_scope: 'full_text', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', force_N: 10 },
    { source_id: 'source-1', evidence_id: 'evidence-2', snapshot_id: '', snapshot_path: 'runtime/snapshots/source-1.html', retrieved_at: '2026-07-17T00:00:00Z', url_role: 'original_content', content_scope: 'full_text', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', force_N: 20 },
    { source_id: 'source-1', evidence_id: 'evidence-3', snapshot_id: 'snap-other', snapshot_path: 'runtime/snapshots/source-1.html', retrieved_at: '2026-07-17T00:00:00Z', url_role: 'original_content', content_scope: 'full_text', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', force_N: 30 },
    { source_id: 'source-1', evidence_id: 'evidence-4', snapshot_id: 'snap-1', snapshot_path: 'runtime/snapshots/source-1.html', retrieved_at: '2026-07-17T00:00:00Z', url_role: 'original_content', content_scope: 'full_text', snapshot_hash: `sha256:${'2'.repeat(64)}`, canonical_url: 'https://example.test/source-1', force_N: 40 },
    { source_id: 'source-1', evidence_id: 'evidence-5', snapshot_id: 'snap-1', snapshot_path: 'runtime/snapshots/source-1.html', retrieved_at: '2026-07-17T00:00:00Z', url_role: 'original_content', content_scope: 'full_text', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/other', force_N: 50 },
    { source_id: 'source-2', evidence_id: 'evidence-6', snapshot_id: 'snap-1', snapshot_path: 'runtime/snapshots/source-1.html', retrieved_at: '2026-07-17T00:00:00Z', url_role: 'original_content', content_scope: 'full_text', snapshot_hash: `sha256:${'1'.repeat(64)}`, canonical_url: 'https://example.test/source-1', force_N: 60 },
  ];

  assert.equal(hooks.filterMeasurementRowsForEvidence(rows, [source]).length, 1);
  assert.equal(hooks.aggregateMeasurements(rows, [source]).get('source-1').count, 1);
});

test('evidence gate accepts verified CSV boolean fields from knowledge imports', () => {
  const gate = hooks.evidenceGate({
    source_id: 'src-dataset-1',
    verification_status: 'verified',
    transport_verified: 'true',
    content_extracted: 'true',
    actual_full_text_or_data: 'true',
    evidence_eligible: 'true',
    http_status: '200',
    snapshot_hash: `sha256:${'a'.repeat(64)}`,
    snapshot_id: 'snap-dataset-1',
    snapshot_path: '/snapshots/source-1/source.zip',
    evidence_id: 'ev-dataset-1',
    retrieved_at: '2026-07-21T04:00:00Z',
    url_role: 'dataset_archive',
    content_scope: 'full_dataset',
    canonical_url: 'https://zenodo.org/api/records/20111572/files/Propeller_Database.zip/content',
    evidence_relevance_score: '9',
    source_tier: 'primary_dataset',
    source_type: 'dataset',
  });

  assert.equal(gate.eligible, true);
  assert.equal(gate.status, 'verified');
});

test('evidence gate accepts receipt-bound original data files', () => {
  const gate = hooks.evidenceGate({
    source_id: 'src-data-file-1',
    verification_status: 'verified',
    transport_verified: true,
    content_extracted: true,
    actual_full_text_or_data: true,
    evidence_eligible: true,
    http_status: 200,
    snapshot_hash: `sha256:${'b'.repeat(64)}`,
    snapshot_id: 'snap-data-file-1',
    snapshot_path: '/snapshots/source-1/data.csv',
    evidence_id: 'ev-data-file-1',
    retrieved_at: '2026-07-21T04:00:00Z',
    url_role: 'original_data',
    content_scope: 'data_file',
    canonical_url: 'https://example.test/primary-data.csv',
    evidence_relevance_score: 8,
    source_tier: 'primary_dataset',
    source_type: 'dataset',
  });

  assert.equal(gate.eligible, true);
  assert.equal(gate.status, 'verified');
});

test('discovery candidates retain candidate ids without becoming evidence', () => {
  const models = hooks.buildSourceModels({}, [{
    candidate_id: 'CAND-0001',
    candidate_key: 'doi:10.1234/example',
    title: 'Candidate paper',
    source_type: 'article',
    requested_url: 'https://example.test/candidate.pdf',
    verification_state: 'rejected',
    rejection_reason: 'http_404',
  }], [], []);

  assert.equal(models.length, 1);
  assert.equal(models[0].id, 'CAND-0001');
  assert.equal(models[0].url, 'https://example.test/candidate.pdf');
  assert.equal(models[0].evidenceEligible, false);
});

test('audited source tiers control visible grades and ranking order', () => {
  const verified = {
    verification_status: 'verified',
    transport_verified: true,
    content_extracted: true,
    actual_full_text_or_data: true,
    evidence_eligible: true,
    http_status: 200,
    snapshot_hash: `sha256:${'a'.repeat(64)}`,
    retrieved_at: '2026-07-27T00:00:00Z',
    url_role: 'original_content',
    content_scope: 'full_text',
    evidence_relevance_score: 9,
  };
  const models = hooks.buildSourceModels({}, [
    {
      ...verified,
      source_id: 'source-b',
      evidence_id: 'evidence-b',
      snapshot_id: 'snapshot-b',
      snapshot_path: '/snapshots/source-b.pdf',
      canonical_url: 'https://example.test/source-b',
      source_tier: 'B - verified',
    },
    {
      ...verified,
      source_id: 'source-a',
      evidence_id: 'evidence-a',
      snapshot_id: 'snapshot-a',
      snapshot_path: '/snapshots/source-a.pdf',
      canonical_url: 'https://example.test/source-a',
      source_tier: 'A',
    },
  ], [], []);

  assert.deepEqual(models.map((model) => model.id), ['source-a', 'source-b']);
  assert.deepEqual(models.map((model) => model.grade), ['A', 'B']);
  assert.equal(hooks.sourceTierGrade({ source_tier: 'C - supplementary' }), 'C');
});

test('research task history collapses into one visible domain lineage', () => {
  const tasks = hooks.collapseResearchTaskLineages([
    { id: 'task-old', knowledge_domain: 'drone_bearing_design', updated_at_ms: 10 },
    { id: 'task-current', knowledge_domain: 'drone_bearing_design', updated_at_ms: 20 },
    { id: 'task-other', knowledge_domain: 'other_domain', updated_at_ms: 15 },
  ]);

  assert.equal(tasks.length, 2);
  assert.equal(tasks[0].id, 'task-current');
  assert.deepEqual(tasks[0].lineage_task_ids, ['task-current', 'task-old']);
});

test('deleted tasks never lead or join a domain lineage', () => {
  assert.equal(hooks.isDeletedResearchTask({ id: 'a', status: 'deleted' }), true);
  assert.equal(hooks.isDeletedResearchTask({ id: 'a', status: 'ready', is_deleted: true }), true);
  assert.equal(hooks.isDeletedResearchTask({ id: 'a', status: 'ready', _deleted: true }), true);
  assert.equal(hooks.isDeletedResearchTask({ id: 'a', status: 'ready' }), false);

  // skf.ctox.dev, 02.09.2026: der geloeschte, aber zuletzt aktualisierte Task
  // gewann die Lineage und stand mit fremdem Titel als aktiv in der Liste.
  const tasks = hooks.collapseResearchTaskLineages([
    { id: 'task-live', title: 'Drone Bearing Design Verified', knowledge_domain: 'drone_bearing_design', status: 'ready', updated_at_ms: 10 },
    { id: 'task-deleted', title: 'Integrated Rolling Bearing', knowledge_domain: 'drone_bearing_design', status: 'deleted', is_deleted: true, updated_at_ms: 20 },
    { id: 'task-gone', knowledge_domain: 'only_deleted', status: 'deleted', updated_at_ms: 30 },
  ]);

  assert.equal(tasks.length, 1);
  assert.equal(tasks[0].id, 'task-live');
  assert.deepEqual(tasks[0].lineage_task_ids, ['task-live']);
});

test('counts stay hidden until the first reload finished and retries back off after failures', () => {
  // Fresh module state: nothing loaded yet -> no numbers, only an ellipsis.
  assert.equal(hooks.researchDataState(), 'syncing');
  assert.equal(hooks.countText(138), '…');
  assert.equal(hooks.countText(0), '…');
  assert.equal(
    hooks.taskSourceSummary({ id: 'task-x', knowledge_domain: 'drone_bearing_design' }),
    'Quellen werden synchronisiert …',
  );

  assert.equal(hooks.failureRetryDelay(0), 5000);
  assert.equal(hooks.failureRetryDelay(1), 10000);
  assert.equal(hooks.failureRetryDelay(2), 20000);
  assert.equal(hooks.failureRetryDelay(3), 40000);
  assert.equal(hooks.failureRetryDelay(4), 60000);
  assert.equal(hooks.failureRetryDelay(9), 60000);
});

test('a terminal command status overrides a stale open queue projection', () => {
  assert.equal(hooks.resolveRunStatus({ status: 'queued' }, { status: 'cancelled' }, { status: 'chat' }), 'cancelled');
  assert.equal(hooks.resolveRunStatus({ status: 'pending' }, { status: 'failed' }, null), 'failed');
  assert.equal(hooks.resolveRunStatus({ status: 'running' }, { status: 'accepted' }, null), 'running');
  assert.equal(hooks.resolveRunStatus(null, { status: 'accepted' }, { status: 'chat' }), 'accepted');
  assert.equal(hooks.resolveRunStatus(null, null, { status: 'chat' }), 'chat');
});

test('sub-theme chips only offer clusters that match a source in the current list', () => {
  const all = hooks.availableSubthemes([], 'all');
  assert.deepEqual(all.map((theme) => theme.id), ['all']);

  const sources = [
    { id: 'src-1', title: 'Propeller thrust and rotor load measurements', sourceClass: 'dataset', row: {} },
    { id: 'src-2', title: 'Wind tunnel aerodynamic study', sourceClass: 'article', row: {} },
  ];
  const offered = hooks.availableSubthemes(sources, 'all').map((theme) => theme.id);
  assert.equal(offered[0], 'all');
  assert.ok(offered.length >= 2, 'a matching cluster is offered');
  assert.ok(offered.length < 6, 'clusters without a matching source are not offered');

  // Der aktive Chip bleibt auch ohne Treffer sichtbar, damit er sich
  // abwaehlen laesst.
  const sticky = hooks.availableSubthemes([], offered[1]).map((theme) => theme.id);
  assert.deepEqual(sticky, ['all', offered[1]]);
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

test('systematic research command context references knowledge tables without embedding rows', () => {
  const refs = hooks.compactKnowledgeTableReferences([
    {
      id: 'table:source_catalog',
      table_key: 'source_catalog',
      domain: 'drone_bearing_design_verified',
      knowledge_version_id: 'knowledge-v2',
      rows: Array.from({ length: 68 }, (_, index) => ({ index, body: 'x'.repeat(10_000) })),
    },
  ]);

  assert.deepEqual(refs, [{
    id: 'table:source_catalog',
    table_key: 'source_catalog',
    domain: 'drone_bearing_design_verified',
    row_count: 68,
    knowledge_version_id: 'knowledge-v2',
  }]);
  assert.doesNotMatch(researchSource, /knowledge_tables:\s*base\?\.tables/);
  assert.match(researchSource, /knowledge_table_refs:\s*knowledgeTableRefs/);
});

test('systematic research keeps discovery candidates out of the verified source registry', () => {
  assert.match(researchSource, /source_candidates:\s*\{\s*title: 'Discovery Candidates'/);
  assert.match(researchSource, /source_catalog:\s*\{\s*title: 'Verified Source Registry'/);
  assert.match(researchSource, /Behandle Discovery nur als Kandidatenmenge/);
  assert.match(researchSource, /vom Evidence-Gate zugelassenen Originalquellen/);
  assert.match(researchSource, /source_candidates: task\.candidate_catalog_key \|\| 'source_candidates'/);
  assert.doesNotMatch(researchSource, /Schreibe jede Discovery-Runde sofort nach source_catalog/);
});

test('systematic research app delegates workflow policy to the system skill', () => {
  assert.match(researchSource, /mit dem System-Skill systematic-research/);
  assert.match(researchSource, /ctox_scholarly_search/);
  assert.match(researchSource, /ctox_web_read/);
  assert.doesNotMatch(researchSource, /ctox web scholarly search --query/);
  assert.doesNotMatch(researchSource, /ctox knowledge data describe --domain/);
  assert.doesNotMatch(researchSource, /Die UI-Evidence-Gate-Felder/);
});

test('knowledge refresh contract preserves living research lineage and source provenance', () => {
  const task = { id: 'task-1', title: 'Bearing loads', knowledge_domain: 'drone_bearing_design' };
  const snapshotHash = `sha256:${'a'.repeat(64)}`;
  const base = { tables: [
    {
      id: 'table:sources', table_key: 'source_catalog', knowledge_version_id: 'knowledge-v7',
      knowledge_version: { version_id: 'knowledge-v7', status: 'current' },
      rows: [{ source_id: 'source-1', evidence_id: 'evidence-1', canonical_url: 'https://example.test/source-1', source_receipt_url: 'https://receipt.test/source-1', snapshot_id: 'snap-1', snapshot_path: 'runtime/snapshots/source-1.html', retrieved_at: '2026-07-17T00:00:00Z', url_role: 'original_content', content_scope: 'full_text', snapshot_hash: snapshotHash, verification_status: 'verified', transport_verified: true, content_extracted: true, actual_full_text_or_data: true, evidence_relevance_score: 9, http_status: 200, evidence_eligible: true, source_tier: 'primary' }],
    },
    { id: 'table:evidence', table_key: 'evidence_points', rows: [{ evidence_id: 'evidence-1', source_id: 'source-1', canonical_url: 'https://example.test/source-1', snapshot_id: 'snap-1', snapshot_path: 'runtime/snapshots/source-1.html', retrieved_at: '2026-07-17T00:00:00Z', url_role: 'original_content', content_scope: 'full_text', snapshot_hash: snapshotHash }] },
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
      evidence_id: 'evidence-1',
      canonical_url: 'https://example.test/source-1',
      source_receipt_url: 'https://receipt.test/source-1',
      snapshot_id: 'snap-1',
      snapshot_path: 'runtime/snapshots/source-1.html',
      retrieved_at: '2026-07-17T00:00:00Z',
      url_role: 'original_content',
      content_scope: 'full_text',
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
      evidence_id: 'evidence-canonical-only',
      canonical_url: 'https://example.test/canonical-only',
      snapshot_id: 'snap-canonical-only',
      snapshot_path: 'runtime/snapshots/canonical-only.html',
      retrieved_at: '2026-07-17T00:00:00Z',
      url_role: 'original_content',
      content_scope: 'full_text',
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
    'source_id',
    'verification_status',
    'transport_verified',
    'content_extracted',
    'actual_full_text_or_data',
    'evidence_relevance_score',
    'http_status',
    'snapshot_id',
    'snapshot_path',
    'snapshot_hash',
    'canonical_url',
    'evidence_id_or_claim_id',
    'retrieved_at',
    'url_role',
    'content_scope',
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

test('validates and flattens a realistic 4,876-row chunked table within the explicit display cap', () => {
  const chunkSizes = [1000, 1000, 1000, 1000, 876];
  let offset = 0;
  const chunks = chunkSizes.map((size, index) => {
    const rows = Array.from({ length: size }, (_, rowIndex) => ({
      source_id: `source_${offset + rowIndex}`,
      rpm: 9000 + (offset + rowIndex) % 1200,
      force_N: 12.5 + ((offset + rowIndex) % 17) / 10,
    }));
    const chunk = { index, chunk_count: chunkSizes.length, offset, row_count: rows.length, rows };
    offset += size;
    return chunk;
  });
  const startedAt = performance.now();
  const result = hooks.validateChunkSequence(chunks, {
    expectedChunkCount: 5,
    expectedItemCount: 4876,
    indexFields: ['index'],
    countFields: ['chunk_count'],
    offsetFields: ['offset'],
    itemCountFields: ['row_count'],
    itemArrayFields: ['rows'],
    itemLabel: 'rows',
  });
  const elapsedMs = performance.now() - startedAt;

  assert.equal(result.valid, true);
  assert.equal(result.chunkCount, 5);
  assert.equal(result.rowCount, 4876);
  assert.equal(result.rows[0].source_id, 'source_0');
  assert.equal(result.rows.at(-1).source_id, 'source_4875');
  assert.ok(elapsedMs < 500, `chunk validation took ${elapsedMs.toFixed(1)}ms`);
  assert.equal(hooks.normalizeKnowledgeTableRows({ row_count: 4876, rows: result.rows }, 'table:4876').rows.length, 4876);
});

test('rejects duplicate, gapped, inconsistent, and misaligned table chunks', () => {
  const valid = [
    { index: 0, chunk_count: 2, offset: 0, row_count: 2, rows: [{ id: 1 }, { id: 2 }] },
    { index: 1, chunk_count: 2, offset: 2, row_count: 1, rows: [{ id: 3 }] },
  ];
  const options = {
    expectedChunkCount: 2,
    expectedItemCount: 3,
    indexFields: ['index'],
    countFields: ['chunk_count'],
    offsetFields: ['offset'],
    itemCountFields: ['row_count'],
    itemArrayFields: ['rows'],
  };
  assert.equal(hooks.validateChunkSequence(valid, options).valid, true);
  assert.equal(hooks.validateChunkSequence([{ ...valid[0] }, { ...valid[1], index: 0 }], options).valid, false);
  assert.equal(hooks.validateChunkSequence([{ ...valid[0] }, { ...valid[1], index: 2 }], options).valid, false);
  assert.equal(hooks.validateChunkSequence([{ ...valid[0] }, { ...valid[1], chunk_count: 3 }], options).valid, false);
  assert.equal(hooks.validateChunkSequence([{ ...valid[0] }, { ...valid[1], offset: 3 }], options).valid, false);
  assert.equal(hooks.validateChunkSequence([{ ...valid[0] }, { ...valid[1], row_count: 4 }], options).valid, false);
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
    evidence_id: 'evidence-valid',
    title: 'Verified rotor load dataset',
    source_type: 'dataset',
    source_url: 'https://example.test/valid',
    verification_status: 'verified',
    transport_verified: true,
    content_extracted: true,
    http_status: 200,
    snapshot_hash: `sha256:${'a'.repeat(64)}`,
    snapshot_id: 'snapshot-valid',
    snapshot_path: 'runtime/snapshots/valid.html',
    retrieved_at: '2026-07-17T00:00:00Z',
    url_role: 'original_content',
    content_scope: 'full_text',
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
  assert.equal(hooks.buildSourceModels(task, [{ ...valid, source_id: '' }], [], []).length, 0);
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

test('evidence graph filtering fails closed when persisted rows lose source provenance', () => {
  const filtered = hooks.filterGraphRowsForEvidence([
    { node_id: 'source:verified', label: 'Verified', source_ids_json: '["verified"]' },
    { node_id: 'source:legacy', label: 'Legacy', source_ids_json: '["legacy"]' },
    { node_id: 'concept:load', label: 'Load', source_ids_json: '["verified","legacy"]' },
    { node_id: 'concept:task', label: 'Task' },
  ], [
    { edge_id: 'valid-edge', source_id: 'source:verified', target_id: 'concept:load', source_ids_json: '["verified"]' },
    { edge_id: 'legacy-edge', source_id: 'source:legacy', target_id: 'concept:load', source_ids_json: '["legacy"]' },
  ], new Set(['verified']));

  assert.equal(filtered.status, 'invalid_graph_contract');
  assert.deepEqual(filtered.nodes, []);
  assert.deepEqual(filtered.edges, []);
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

test('research launch deduplicates projected sources and repairs a legacy inflated target', () => {
  const source = {
    id: 'source-1',
    evidenceEligible: true,
    row: { source_id: 'source-1', canonical_url: 'https://example.test/source-1' },
  };
  const duplicateProjection = {
    ...source,
    id: 'projection-copy-1',
    row: {
      ...source.row,
      source_id: 'projection-copy-1',
      canonical_url: 'https://EXAMPLE.test/source-1/#projection',
    },
  };
  const unique = hooks.uniqueSourceModels([source, duplicateProjection]);

  assert.equal(unique.length, 1);
  assert.equal(hooks.boundedVerifiedSourceCount(Array.from({ length: 276 }), { row_count: 138 }), 138);
  assert.equal(hooks.effectiveTargetVerifiedSources(276, 276, 276, 138), 100);
  assert.equal(hooks.effectiveTargetVerifiedSources(276, 138, 138, 138), 100);
  assert.equal(hooks.effectiveTargetVerifiedSources(150, 276, 276, 138), 150);
});

test('research launch can rebuild verified models from the authoritative source table', () => {
  const rows = Array.from({ length: 138 }, (_, index) => ({
    source_id: `source-${index}`,
    title: `Source ${index}`,
    verification_status: 'verified',
    transport_verified: true,
    content_extracted: true,
    actual_full_text_or_data: true,
    evidence_eligible: true,
    http_status: 200,
    snapshot_hash: `sha256:${index.toString(16).padStart(64, '0')}`,
    snapshot_id: `snapshot-${index}`,
    snapshot_path: `/snapshots/source-${index}/source.pdf`,
    evidence_id: `evidence-${index}`,
    retrieved_at: '2026-07-27T00:00:00Z',
    url_role: 'original_content',
    content_scope: 'full_text',
    canonical_url: `https://example.test/source-${index}`,
    evidence_relevance_score: 9,
    source_tier: 'primary',
  }));
  const models = hooks.buildSourceModels(
    { payload: { scoring_dimensions: [{ id: 'relevance', label: 'Relevance' }] } },
    rows,
    [],
    [],
  );

  assert.equal(models.length, 138);
  assert.equal(models.filter((source) => source.evidenceEligible).length, 138);
  assert.equal(hooks.effectiveTargetVerifiedSources(276, 138, 138, 138), 100);
});

test('research reports contain only live documents with explicit task or domain lineage', () => {
  const task = { id: 'task-1', lineage_task_ids: ['task-1', 'task-legacy'], knowledge_domain: 'drone_bearing_design' };
  const reports = hooks.researchReportsForTask(task, [
    { id: 'task-report', title: 'Task report', filename: 'task.docx', linked_records: [{ kind: 'research_task', id: 'task-1' }], updated_at_ms: 20 },
    { id: 'lineage-report', title: 'Lineage report', filename: 'lineage.docx', linked_records: [{ kind: 'research_task', id: 'task-legacy' }], updated_at_ms: 25 },
    { id: 'domain-report', title: 'Domain report', filename: 'domain.docx', linked_records: [{ kind: 'knowledge_domain', id: 'drone_bearing_design' }], updated_at_ms: 30 },
    { id: 'unlinked-demo', title: 'Legacy demo', filename: 'legacy.md', linked_records: [], updated_at_ms: 40 },
    { id: 'deleted', title: 'Deleted', filename: 'deleted.docx', linked_records: [{ kind: 'research_task', id: 'task-1' }], is_deleted: true, updated_at_ms: 50 },
  ]);

  assert.deepEqual(reports.map((report) => report.id), ['domain-report', 'lineage-report', 'task-report']);
});

test('diagnostic rows distinguish sync failures from local no-data', () => {
  const rows = hooks.collectionDiagnosticRows(['research_runs', 'research_notes', 'knowledge_tables'], {
    research_runs: { sync: { kind: 'failed', message: 'Synchronisierung fehlgeschlagen' } },
    research_notes: { sync: { kind: 'local', message: 'Lokaler Modus' } },
    knowledge_tables: { read: { kind: 'ok', message: '0 rows' } },
  });

  assert.deepEqual(rows.map((row) => row.kind), ['failed', 'local', 'ok']);
  assert.match(rows[0].label, /Synchronisierung/);
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

test('knowledge base grouping merges replicated table chunks in row order', () => {
  const chunks = [
    {
      id: 'table:measurements:chunk:0001',
      payload: {
        id: 'table:measurements:chunk:0001',
        logical_table_id: 'table:measurements',
        domain: 'verified_research',
        table_key: 'measured_load_points',
        row_count: 3,
        chunk_index: 1,
        chunk_count: 2,
        rows_complete: true,
        rows: [{ source_row: 2 }],
      },
    },
    {
      id: 'table:measurements',
      payload: {
        id: 'table:measurements',
        logical_table_id: 'table:measurements',
        domain: 'verified_research',
        table_key: 'measured_load_points',
        row_count: 3,
        chunk_index: 0,
        chunk_count: 2,
        rows_complete: true,
        rows: [{ source_row: 0 }, { source_row: 1 }],
      },
    },
  ];

  const merged = hooks.mergeKnowledgeTableChunks(chunks);
  assert.equal(merged.length, 1);
  assert.equal(merged[0].id, 'table:measurements');
  assert.equal(merged[0].row_count, 3);
  assert.equal(merged[0].chunk_count, 2);
  assert.equal(merged[0].rows_complete, true);
  assert.deepEqual(merged[0].rows.map((row) => row.source_row), [0, 1, 2]);

  const grouped = hooks.knowledgeBasesFromTables(chunks);
  assert.equal(grouped.length, 1);
  assert.equal(grouped[0].tables.length, 1);
  assert.deepEqual(grouped[0].tables[0].rows.map((row) => row.source_row), [0, 1, 2]);
});

test('knowledge table loader derives bounded follow-up reads from base chunks', () => {
  assert.deepEqual(hooks.knowledgeTableChunkDocumentIds([
    {
      id: 'table:sources',
      payload: {
        logical_table_id: 'table:sources',
        chunk_index: 0,
        chunk_count: 3,
      },
    },
    {
      id: 'table:single',
      chunk_index: 0,
      chunk_count: 1,
    },
  ]), [
    'table:sources:chunk:0001',
    'table:sources:chunk:0002',
  ]);
});

test('RxDB documents are detached before chunk cache eviction mutates live values', () => {
  const liveValue = {
    id: 'table:candidates',
    payload: {
      logical_table_id: 'table:candidates',
      chunk_index: 0,
      chunk_count: 2,
      rows: [{ candidate_id: 'candidate-1' }],
    },
  };
  const snapshot = hooks.toJson({ toJSON: () => liveValue });

  liveValue.payload.rows.length = 0;
  liveValue.payload.chunk_count = 0;

  assert.equal(snapshot.payload.chunk_count, 2);
  assert.deepEqual(snapshot.payload.rows, [{ candidate_id: 'candidate-1' }]);
});

test('empty knowledge read retries follow canonical knowledge_tables readiness', () => {
  const snapshot = (state, ready, syncing) => ({ collection: 'knowledge_tables', state, ready, syncing, updatedAt: ready ? 1 : null });

  // Without a readiness API the optimistic legacy default stays.
  assert.equal(hooks.shouldRetryEmptyKnowledgeTables(), true);
  // Live channels may still receive demand-loaded chunks; catching-up
  // channels may still deliver the initial replication.
  assert.equal(hooks.shouldRetryEmptyKnowledgeTables(snapshot('live', true, false)), true);
  assert.equal(hooks.shouldRetryEmptyKnowledgeTables(snapshot('catching-up', false, true)), true);
  // Known offline/failed or never-synced channels make retries pointless.
  assert.equal(hooks.shouldRetryEmptyKnowledgeTables(snapshot('offline-pending', false, false)), false);
  assert.equal(hooks.shouldRetryEmptyKnowledgeTables(snapshot('never-synced', false, true)), false);
});

test('data-driven empties follow collection readiness (syncing vs empty)', () => {
  assert.equal(hooks.dataEmptyShowsSyncing(true, { ready: false }), true);
  assert.equal(hooks.dataEmptyShowsSyncing(true, { ready: true }), false);
  assert.equal(hooks.dataEmptyShowsSyncing(false, { ready: false }), false);
  assert.equal(hooks.dataEmptyShowsSyncing(true, undefined), false);
  assert.match(researchSource, /renderListOrState\(tables, collectionReadiness\('knowledge_tables'\)/);

  const live = (collection) => ({ collection, state: 'live', ready: true, syncing: false, updatedAt: 1 });

  // Empty + unready: the syncing shell replaces the empty copy.
  hooks.setCollectionReadinessForTest('research_tasks', { collection: 'research_tasks', state: 'catching-up', ready: false, syncing: true, updatedAt: null });
  assert.equal(hooks.emptyStateForNoTask().kind, 'syncing');
  assert.match(hooks.renderNoTasksEmpty(), /class="ctox-syncing research-empty-card" role="status" aria-live="polite"/);
  assert.match(hooks.renderNoTaskCenter(), /class="ctox-syncing research-empty-state-panel" role="status" aria-live="polite"/);

  // Empty + ready: the regular empty state renders.
  hooks.setCollectionReadinessForTest('research_tasks', live('research_tasks'));
  hooks.setCollectionReadinessForTest('knowledge_tables', live('knowledge_tables'));
  assert.equal(hooks.emptyStateForNoTask().kind, 'empty');
  assert.match(hooks.renderNoTasksEmpty(), /class="ctox-empty research-empty research-empty-card"/);
  assert.match(hooks.renderNoTaskCenter(), /class="ctox-empty research-empty-state-panel"/);

  hooks.setCollectionReadinessForTest('research_tasks', null);
  hooks.setCollectionReadinessForTest('knowledge_tables', null);
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
  assert.match(researchSource, /subscribeCollectionReadiness/);
  assert.match(researchSource, /dataEmptyShowsSyncing\(true, tasksReadiness\)/);
  assert.match(researchSource, /Research-Daten werden mit dieser Instanz synchronisiert/);
  assert.match(researchSource, /await refreshAll\(\{ seed: true, mountToken \}\)[\s\S]*?state\.initialDataReady = true/);
});

test('research and knowledge events use independent refresh timers', () => {
  assert.match(researchSource, /researchRefreshTimer: null/);
  assert.match(researchSource, /knowledgeRefreshTimer: null/);
  assert.match(researchSource, /knowledgeRefreshInFlight: false/);
  assert.match(researchSource, /function scheduleLocalRefresh[\s\S]*?state\.researchRefreshTimer/);
  assert.match(researchSource, /function scheduleKnowledgeRefresh[\s\S]*?state\.knowledgeRefreshTimer/);
  assert.match(researchSource, /if \(state\.knowledgeRefreshInFlight\) return/);
  assert.match(researchSource, /const active = knowledgeTableLoads\.get\(key\);[\s\S]*?if \(active\) return active/);
  assert.match(researchSource, /knowledgeLifecycleCollections[\s\S]*?'research_runs'[\s\S]*?'business_commands'[\s\S]*?'ctox_queue_tasks'/);
  assert.doesNotMatch(researchSource, /readableCollection\('knowledge_tables'\)\?\.\$\?\./);
  assert.doesNotMatch(researchSource, /state\.refreshTimer/);
  assert.match(researchSource, /rowLimitWarnings/);
  assert.match(researchSource, /Anzeige auf \$\{ROW_LIMIT/);
});

test('discovery graph prefers persisted citation paths over inferred tag clusters', () => {
  assert.match(researchSource, /function persistedCitationDiscoveryGraph/);
  assert.match(researchSource, /discovery_paths_json/);
  assert.match(researchSource, /seed_source_id/);
  assert.match(researchSource, /citation_hop/);
  assert.match(researchSource, /citation_direction/);
  assert.match(researchSource, /const persisted = persistedCitationDiscoveryGraph\(task\);[\s\S]*?if \(persisted\) return persisted/);
  assert.match(researchSource, /state\.graph\.visibleLimit \|\| 60/);
});

test('research graph releases its WebGL context when the surface is remounted', () => {
  assert.match(researchGraphSource, /zoomToFit\?\.\([\s\S]*?projection\.visibleNodeIds\.has\(node\.id\)/);
  assert.match(researchGraphSource, /graph\.pauseAnimation\?\.\(\)/);
  assert.match(researchGraphSource, /renderer\?\.dispose\?\.\(\)/);
  assert.match(researchGraphSource, /renderer\?\.forceContextLoss\?\.\(\)/);
  assert.match(researchGraphSource, /renderer\?\.domElement\?\.remove\?\.\(\)/);
  assert.match(researchGraphSource, /WEBGL_lose_context/);
  assert.match(researchGraphSource, /loseContext\?\.\(\)/);
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
  assert.match(css, /@container business-app-window \(max-width: 1024px\)[\s\S]*?\.ctox-workspace\.research-module\s*\{[\s\S]*?grid-template-columns: minmax\(0, 1fr\)/);
  assert.match(css, /grid-template-areas:\s*"research-center"\s*"research-left"\s*"research-right"/);
  assert.match(css, /\.research-ai-prompt-pre/);
  assert.match(css, /@keyframes research-spin/);
});

/* Guard (02.09.2026): a source model must only carry the criteria its task is actually scored on. The module
   keeps an internal catalogue that also holds competitive-research criteria (buyer clarity, pricing clarity,
   …); those are meaningless for an engineering base and used to leak into the UI, the drawer and the export. */
test('source scoring exposes only the criteria of the task', async () => {
  const bearingTask = { id: 'r1', knowledge_domain: 'drone_bearing_design_verified', title: 'Drone Bearing Design Verified', prompt: 'bearing loads propeller rpm thrust', criteria: '' };
  const row = {
    source_id: 'SRC-0001',
    title: 'Bearing load measurements on a UAV propeller test stand',
    source_url: 'https://example.test/a.pdf',
    canonical_url: 'https://example.test/a.pdf',
    snapshot_id: 'snapshot-src-0001',
    snapshot_path: '/snap/a.pdf',
    snapshot_hash: 'sha256:'.concat('a'.repeat(64)),
    evidence_id: 'EVID-0001',
    retrieved_at: '2026-07-25T10:00:00Z',
    url_role: 'original_content',
    content_scope: 'full_text',
    verification_status: 'verified',
    transport_verified: true,
    content_extracted: true,
    actual_full_text_or_data: true,
    evidence_relevance_score: 100,
    http_status: 200,
    evidence_eligible: true,
    source_tier: 'A',
    source_type: 'article',
  };
  const [model] = hooks.buildSourceModels(bearingTask, [row], [], []);
  const expected = new Set(hooks.scoringDimensionsForTask(bearingTask).map((axis) => axis.id));

  assert.equal(model.evidenceEligible, true);
  assert.deepEqual(new Set(Object.keys(model.dimensions)), expected);
  for (const leaked of ['buyer_clarity', 'pricing_clarity', 'enterprise_readiness', 'autonomous_agent_depth', 'integration_api', 'proof_customer_evidence', 'trust_compliance', 'overlap']) {
    assert.equal(Object.hasOwn(model.dimensions, leaked), false, leaked);
  }
  assert.ok(Number.isFinite(model.dimensions.portfolio_priority));

  const ineligible = hooks.buildSourceModels(bearingTask, [{ source_id: 'SRC-0002', title: 'Metadata only' }], [], []);
  assert.deepEqual(new Set(Object.keys(ineligible[0].dimensions)), expected);
});

/* Guard (02.09.2026): Knowledge shows the consolidated claims of the base — one block per statement with its
   knowledge book, statement type, contributing sources and verbatim evidence — not a link list of tables and
   not the raw text chunks that the evidence rows carry. */
test('knowledge view lists consolidated claims with their evidence', async () => {
  hooks.setStateForTest({
    claimRows: [
      { claim_id: 'CLM-1001', claim_text: 'Unwucht erzeugt eine drehzahlsynchrone radiale Erregerkraft, die mit dem Quadrat der Drehzahl wächst.', statement_type: 'direct_measurement', confidence: 'high', knowledge_book: 'Unwucht-, Vibrations- und transiente Lasten', source_id: 'SRC-0015;SRC-0125', evidence_id: 'EVID-CLAIM-1001', limitations: 'Gilt für die untersuchten Propellergrößen.' },
      { claim_id: 'CLM-1002', claim_text: 'Der statische Sicherheitsfaktor eines Wälzlagers ist s0 = C0/P0.', statement_type: 'normative', confidence: 'high', knowledge_book: 'Statische und dynamische Lagerauslegung', source_id: 'SRC-0119', evidence_id: 'EVID-CLAIM-1002', limitations: '' },
    ],
    evidenceRows: [
      { evidence_id: 'EVID-CLAIM-1001', claim_id: 'CLM-1001', evidence_kind: 'claim_support', source_id: 'SRC-0015', source_locator: 'Seite 4, Abschnitt 2.1', quote: 'the unbalance force is proportional to the square of the rotational speed' },
      { evidence_id: 'EVID-REL-1', source_id: 'SRC-0015', evidence_kind: 'source_relevance', fact_value: 'Relevanzurteil, kein Claim' },
    ],
    knowledgeTopic: '',
    knowledgeType: 'all',
  });

  const claims = hooks.knowledgeClaims();
  assert.equal(claims.length, 2);
  assert.deepEqual(claims[0].sources, ['SRC-0015', 'SRC-0125']);

  const markup = hooks.renderKnowledgeTables({ id: 'r1' });
  assert.match(markup, /CLM-1001/);
  assert.match(markup, /drehzahlsynchrone radiale Erregerkraft/);
  assert.match(markup, /Mehrere Quellen<\/?[^>]*> ?<span>1<\/span>|Mehrere Quellen <span>1<\/span>/);
  assert.match(markup, /data-action="knowledge-book" data-knowledge-book="Statische und dynamische Lagerauslegung"/);
  assert.match(markup, /data-action="select-source" data-source-id="SRC-0125"/);
  assert.match(markup, /Seite 4, Abschnitt 2\.1/);
  assert.doesNotMatch(markup, /Relevanzurteil, kein Claim/);
  assert.doesNotMatch(markup, /data-action="open-knowledge"/);

  hooks.setStateForTest({ knowledgeType: 'multi' });
  const multiOnly = hooks.renderKnowledgeTables({ id: 'r1' });
  assert.match(multiOnly, /CLM-1001/);
  assert.doesNotMatch(multiOnly, /CLM-1002/);

  // bases without a claims table keep working through the claim_support evidence rows
  hooks.setStateForTest({ claimRows: [], knowledgeType: 'all' });
  assert.equal(hooks.knowledgeClaims().length, 1);
  hooks.setStateForTest({ claimRows: [], evidenceRows: [], knowledgeTopic: '', knowledgeType: 'all' });
});

/* Guard (03.09.2026): a research run must not stop at "sources verified". The task the app seeds carries the
   claims table in its contract and asks, in prompt and criteria, for a content evaluation of every admitted
   source plus cross-source consolidation — otherwise the agent produces a large verified corpus with a
   handful of claims and an unconnected graph, which is exactly the failure this wiring exists to prevent. */
test('seeded research task demands per-source evaluation and consolidation', async () => {
  const contract = hooks.RESEARCH_TABLE_CONTRACT;
  assert.ok(contract.claims, 'claims table missing from the research table contract');
  for (const column of ['claim_id', 'claim_text', 'statement_type', 'evidence_id', 'source_id', 'exact_short_quote_or_table_ref', 'confidence', 'limitations', 'knowledge_book']) {
    assert.ok(contract.claims.columns.includes(column), `claims column ${column}`);
  }

  const prompt = hooks.defaultPromptForKnowledgeBase({ domain: 'drone_bearing_design_verified', title: 'Drone Bearing', tables: [] });
  assert.match(prompt, /JEDE aufgenommene Quelle|EVERY admitted source/);
  assert.match(prompt, /claims/);
  assert.match(prompt, /[Kk]onsolidiere|[Cc]onsolidate/);

  const de = JSON.parse(await readFile(new URL('./locales/de.json', import.meta.url), 'utf8'));
  const en = JSON.parse(await readFile(new URL('./locales/en.json', import.meta.url), 'utf8'));
  for (const [lang, dict] of [['de', de], ['en', en]]) {
    assert.ok(dict.defaultCriteriaText, `${lang}: defaultCriteriaText missing`);
    assert.match(dict.defaultCriteriaText, /claims/, `${lang}: criteria must name the claims table`);
    assert.ok(dict.defaultPromptText.includes('claims'), `${lang}: prompt must name the claims table`);
  }

  // the skill carries the same methodology, so an agent run without the app still evaluates every source
  const skill = await readFile(new URL('../../../../skills/system/research/systematic-research/SKILL.md', import.meta.url), 'utf8');
  assert.match(skill, /Evaluate Every Admitted Source/);
  assert.match(skill, /Consolidate Across Sources/);
  assert.match(skill, /least two claims\*\* — an unevaluated verified source blocks completion/);
  assert.match(skill, /verbatim quote/);
  const contractDoc = await readFile(new URL('../../../../skills/system/research/systematic-research/WORKFLOW_CONTRACT.md', import.meta.url), 'utf8');
  assert.match(contractDoc, /`claims` table/);
  assert.match(contractDoc, /claim_support/);
});

const CATALOG_DOMAIN = 'drone_bearing_design_verified';
const CATALOG_SNAPSHOT_HASH = `sha256:${'1'.repeat(64)}`;
const CATALOG_LINEAGE = Object.freeze({
  source_id: 'SRC-1',
  snapshot_id: 'snap-1',
  snapshot_path: 'runtime/snapshots/source-1.html',
  retrieved_at: '2026-07-17T00:00:00Z',
  url_role: 'original_content',
  content_scope: 'full_text',
  snapshot_hash: CATALOG_SNAPSHOT_HASH,
  canonical_url: 'https://example.test/source-1',
});

function catalogKnowledgeDocument({
  id,
  tableId,
  domain,
  tableKey,
  rowCount,
  hash,
  title = tableKey,
  chunkCount = 4,
}) {
  const payload = {
    id,
    logical_table_id: id,
    table_id: tableId,
    domain,
    table_key: tableKey,
    title,
    projection_version: 2,
    rows_source: 'rxdb.rows.fetch',
    rows_complete: true,
    row_count: rowCount,
    content_hash: hash,
    chunk_count: chunkCount,
  };
  return { ...payload, payload };
}

function verifiedCatalogSourceRow() {
  return {
    ...CATALOG_LINEAGE,
    evidence_id: 'EVID-1',
    title: 'Verified propeller source',
    source_url: 'https://example.test/source-1',
    verification_status: 'verified',
    transport_verified: true,
    content_extracted: true,
    actual_full_text_or_data: true,
    evidence_eligible: true,
    http_status: 200,
    evidence_relevance_score: 9,
    source_tier: 'A',
    source_type: 'dataset',
  };
}

function catalogMeasurementRow(index) {
  return {
    ...CATALOG_LINEAGE,
    evidence_id: `EVID-M-${index + 1}`,
    measurement_id: `MLP-${index + 1}`,
    force_N: 1,
    rpm: 100,
  };
}

function matchesKnowledgeSelector(document, selector = {}) {
  return Object.entries(selector).every(([key, expected]) => {
    if (Object.prototype.hasOwnProperty.call(document, key) && document[key] !== undefined) {
      return document[key] === expected;
    }
    const payload = document?.payload;
    return Boolean(payload) && payload[key] === expected;
  });
}

function fakeKnowledgeCollection(documents, lookups, writes) {
  return {
    find({ selector } = {}) {
      const docs = documents.filter((document) => matchesKnowledgeSelector(document, selector));
      return { exec: async () => docs.map((document) => ({ toJSON: () => document })) };
    },
    findOne(id) {
      lookups.push(id);
      const document = documents.find((entry) => entry.id === id) || null;
      return { exec: async () => (document ? { toJSON: () => document } : null) };
    },
    upsert(doc) {
      writes.push(doc);
      throw new Error('knowledge rows must stay out of the collection');
    },
    insert(doc) {
      writes.push(doc);
      throw new Error('knowledge rows must stay out of the collection');
    },
  };
}

function installKnowledgeRowsHarness({ documents, loader, lookups = [], writes = [] }) {
  globalThis.window = globalThis;
  hooks.resetKnowledgeRowsForTest();
  hooks.setStateForTest({
    ctx: {
      host: null,
      db: {
        collection(name) {
          return name === 'knowledge_tables'
            ? fakeKnowledgeCollection(documents, lookups, writes)
            : null;
        },
      },
      permissions: {
        canReadCollection: () => true,
        canWriteCollection: () => false,
      },
      sync: {
        async startCollection(name) {
          if (name !== 'knowledge_tables') return null;
          return { state: { knowledgeRowsLoader: loader } };
        },
      },
    },
    tasks: [],
    knowledgeBases: [],
    selectedTaskId: '',
    sourceRows: [],
    measurementRows: [],
    sourceModels: [],
    rowLimitWarnings: [],
    chunkDiagnostics: [],
  });
}

function restoreKnowledgeRowsHarness() {
  hooks.resetKnowledgeRowsForTest();
  hooks.setStateForTest({
    ctx: null,
    tasks: [],
    knowledgeBases: [],
    selectedTaskId: '',
    selectedSourceId: '',
    candidateRows: [],
    candidateModels: [],
    sourceRows: [],
    curatedRows: [],
    claimRows: [],
    evidenceRows: [],
    measurementRows: [],
    derivedMeasurementRows: [],
    graphNodeRows: [],
    graphEdgeRows: [],
    sourceModels: [],
    graphProjection: null,
    rowLimitWarnings: [],
    chunkDiagnostics: [],
    diagnostics: {
      collections: {},
      reloadStartedAt: 0,
      reloadFinishedAt: 0,
      reloadCount: 0,
      postSyncRefreshes: 0,
      failureRetries: 0,
      failureRetryAt: 0,
      loadedOnce: false,
    },
  });
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((next, fail) => {
    resolve = next;
    reject = fail;
  });
  return { promise, resolve, reject };
}

async function waitUntil(predicate, timeoutMs = 2000) {
  const started = Date.now();
  while (!predicate()) {
    if (Date.now() - started > timeoutMs) throw new Error('timed out waiting for catalog row fetch');
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}

test('catalog tables do not generate chunk document ids', () => {
  assert.deepEqual(hooks.knowledgeTableChunkDocumentIds([
    catalogKnowledgeDocument({
      id: 'table:kdt-measurements',
      tableId: 'kdt-measurements',
      domain: CATALOG_DOMAIN,
      tableKey: 'measured_load_points',
      rowCount: 5103,
      hash: 'hash-5103',
      chunkCount: 9,
    }),
    {
      id: 'table:sources',
      payload: {
        logical_table_id: 'table:sources',
        chunk_index: 0,
        chunk_count: 3,
      },
    },
  ]), [
    'table:sources:chunk:0001',
    'table:sources:chunk:0002',
  ]);
});

test('catalog knowledge tables load every row without the chunk cap', async () => {
  const rowCount = 5103;
  const calls = [];
  const lookups = [];
  const writes = [];
  const sourceDoc = catalogKnowledgeDocument({
    id: 'table:kdt-sources',
    tableId: 'kdt-sources',
    domain: CATALOG_DOMAIN,
    tableKey: 'source_catalog',
    rowCount: 1,
    hash: 'hash-sources',
    title: 'Source catalog',
  });
  const measurementDoc = catalogKnowledgeDocument({
    id: 'table:kdt-measurements',
    tableId: 'kdt-measurements',
    domain: CATALOG_DOMAIN,
    tableKey: 'measured_load_points',
    rowCount,
    hash: 'hash-5103',
    title: 'Measured load points',
  });
  const sourceRow = verifiedCatalogSourceRow();
  const task = {
    id: 'task-catalog-rows',
    knowledge_domain: CATALOG_DOMAIN,
    title: 'Catalog row load',
    prompt: 'bearing loads propeller rpm thrust',
    source_catalog_key: 'source_catalog',
    measurements_table_key: 'measured_load_points',
    payload: {},
  };
  installKnowledgeRowsHarness({
    documents: [sourceDoc, measurementDoc],
    lookups,
    writes,
    loader: {
      async fetchAllRows(tableId) {
        calls.push(tableId);
        if (tableId === 'kdt-sources') {
          return { rows: [sourceRow], rowCount: 1, contentHash: 'hash-sources' };
        }
        if (tableId === 'kdt-measurements') {
          return {
            rows: Array.from({ length: rowCount }, (_, index) => catalogMeasurementRow(index)),
            rowCount,
            contentHash: 'hash-5103',
          };
        }
        throw new Error(`unexpected table ${tableId}`);
      },
    },
  });

  try {
    const bases = await hooks.loadKnowledgeBases({ domains: [CATALOG_DOMAIN] });
    const measurementTable = bases[0].tables.find((table) => table.table_key === 'measured_load_points');
    const sourceTable = bases[0].tables.find((table) => table.table_key === 'source_catalog');
    assert.equal(measurementTable.rows.length, rowCount);
    assert.equal(measurementTable.rows.at(-1).measurement_id, 'MLP-5103');
    assert.equal(measurementTable.rows_origin, 'rxdb.rows.fetch');
    assert.equal(sourceTable.rows.length, 1);
    assert.deepEqual(lookups, []);
    assert.deepEqual(writes, []);
    assert.deepEqual(calls, ['kdt-sources', 'kdt-measurements']);

    const models = hooks.buildSourceModels(task, sourceTable.rows, [], measurementTable.rows);
    assert.equal(models[0].evidenceEligible, true);
    assert.equal(hooks.aggregateMeasurements(measurementTable.rows, models).get('SRC-1').count, rowCount);

    hooks.setStateForTest({
      knowledgeBases: bases,
      tasks: [task],
      selectedTaskId: task.id,
      claimRows: [],
      evidenceRows: [],
    });
    hooks.setLoadedOnceForTest();
    await hooks.loadDashboardData();
    assert.match(hooks.renderMeasurementsTable(), /Direkte Messwerte <span>5\.103<\/span>/);
    assert.doesNotMatch(hooks.renderKnowledgeTables(task), /Anzeige auf/);

    await hooks.loadKnowledgeBases({ domains: [CATALOG_DOMAIN] });
    assert.deepEqual(calls, ['kdt-sources', 'kdt-measurements']);
    measurementDoc.content_hash = 'hash-5103-b';
    measurementDoc.payload.content_hash = 'hash-5103-b';
    const reloaded = await hooks.loadKnowledgeBases({ domains: [CATALOG_DOMAIN] });
    const reloadedMeasurements = reloaded[0].tables.find((table) => table.table_key === 'measured_load_points');
    assert.deepEqual(calls, ['kdt-sources', 'kdt-measurements', 'kdt-measurements']);
    assert.equal(reloadedMeasurements.rows.length, rowCount);
    assert.deepEqual(lookups, []);
    assert.deepEqual(writes, []);
  } finally {
    restoreKnowledgeRowsHarness();
  }
});

test('chunk knowledge tables still load embedded rows without the rows loader', async () => {
  const lookups = [];
  const writes = [];
  const calls = [];
  const documents = [
    {
      id: 'table:measurements',
      payload: {
        id: 'table:measurements',
        logical_table_id: 'table:measurements',
        domain: 'verified_research',
        table_key: 'measured_load_points',
        row_count: 3,
        chunk_index: 0,
        chunk_count: 2,
        rows_complete: true,
        rows: [{ source_row: 0 }, { source_row: 1 }],
      },
    },
    {
      id: 'table:measurements:chunk:0001',
      payload: {
        id: 'table:measurements:chunk:0001',
        logical_table_id: 'table:measurements',
        domain: 'verified_research',
        table_key: 'measured_load_points',
        row_count: 3,
        chunk_index: 1,
        chunk_count: 2,
        rows_complete: true,
        rows: [{ source_row: 2 }],
      },
    },
  ];
  installKnowledgeRowsHarness({
    documents,
    lookups,
    writes,
    loader: {
      async fetchAllRows(tableId) {
        calls.push(tableId);
        throw new Error(`chunk table must not use fetchAllRows (${tableId})`);
      },
    },
  });

  try {
    const bases = await hooks.loadKnowledgeBases({ domains: ['verified_research'] });
    assert.deepEqual(calls, []);
    assert.deepEqual(lookups, ['table:measurements:chunk:0001']);
    assert.deepEqual(writes, []);
    assert.equal(bases.length, 1);
    assert.deepEqual(bases[0].tables[0].rows.map((row) => row.source_row), [0, 1, 2]);
    assert.equal(bases[0].tables[0].chunk_count, 2);
    assert.equal(bases[0].tables[0].rows_origin, undefined);

    const hybrid = hooks.mergeKnowledgeTableChunks([{
      id: 'table:hybrid',
      projection_version: 2,
      rows_source: 'rxdb.rows.fetch',
      domain: 'verified_research',
      table_key: 'measured_load_points',
      chunk_index: 0,
      chunk_count: 1,
      row_count: 1,
      rows: [{ source_row: 7 }],
    }]);
    assert.deepEqual(hybrid[0].rows, [{ source_row: 7 }]);
    assert.equal(hybrid[0].rows_origin, undefined);
  } finally {
    restoreKnowledgeRowsHarness();
  }
});

test('a retryable rows error stays on that table and leaves the rest of the dashboard', async () => {
  const lookups = [];
  const writes = [];
  const sourceDoc = catalogKnowledgeDocument({
    id: 'table:kdt-sources',
    tableId: 'kdt-sources',
    domain: CATALOG_DOMAIN,
    tableKey: 'source_catalog',
    rowCount: 1,
    hash: 'hash-sources',
    title: 'Source catalog',
  });
  const measurementDoc = catalogKnowledgeDocument({
    id: 'table:kdt-measurements',
    tableId: 'kdt-measurements',
    domain: CATALOG_DOMAIN,
    tableKey: 'measured_load_points',
    rowCount: 4,
    hash: 'hash-measurements',
    title: 'Measured load points',
  });
  const task = {
    id: 'task-catalog-error',
    knowledge_domain: CATALOG_DOMAIN,
    title: 'Catalog row error',
    prompt: 'bearing loads',
    source_catalog_key: 'source_catalog',
    measurements_table_key: 'measured_load_points',
    payload: {},
  };
  installKnowledgeRowsHarness({
    documents: [sourceDoc, measurementDoc],
    lookups,
    writes,
    loader: {
      async fetchAllRows(tableId) {
        if (tableId === 'kdt-measurements') {
          const error = new Error('Parquet-Fenster fehlgeschlagen');
          error.retryable = true;
          error.code = 'ROWS_SOURCE_ERROR';
          throw error;
        }
        return { rows: [verifiedCatalogSourceRow()], rowCount: 1, contentHash: 'hash-sources' };
      },
    },
  });

  try {
    const bases = await hooks.loadKnowledgeBases({ domains: [CATALOG_DOMAIN] });
    const sourceTable = bases[0].tables.find((table) => table.table_key === 'source_catalog');
    const measurementTable = bases[0].tables.find((table) => table.table_key === 'measured_load_points');
    assert.equal(sourceTable.rows.length, 1);
    assert.equal(measurementTable.rows, undefined);
    hooks.setStateForTest({
      knowledgeBases: bases,
      tasks: [task],
      selectedTaskId: task.id,
    });
    hooks.setLoadedOnceForTest();
    await hooks.loadDashboardData();

    const states = hooks.knowledgeTableRowStates();
    assert.equal(states['table:kdt-measurements'].phase, 'error');
    assert.equal(states['table:kdt-measurements'].retryable, true);
    assert.equal(states['table:kdt-sources'], undefined);
    const banner = hooks.renderKnowledgeTableRowStates();
    assert.match(banner, /data-table-id="table:kdt-measurements"/);
    assert.match(banner, /data-row-state="error"/);
    assert.match(banner, /Parquet-Fenster fehlgeschlagen/);
    assert.match(banner, /Erneut versuchen/);
    assert.doesNotMatch(banner, /table:kdt-sources/);
    assert.match(hooks.renderSourcesTable(), /Verified propeller source/);
    assert.match(hooks.renderMeasurementsTable(), /Direkte Messwerte <span>0<\/span>/);
    assert.deepEqual(lookups, []);
    assert.deepEqual(writes, []);
  } finally {
    restoreKnowledgeRowsHarness();
  }
});

test('a domain change aborts in-flight catalog fetchAllRows', async () => {
  const lookups = [];
  const writes = [];
  const calls = [];
  const started = deferred();
  const documents = [
    catalogKnowledgeDocument({
      id: 'table:kdt-a',
      tableId: 'kdt-a',
      domain: 'domain-a',
      tableKey: 'measured_load_points',
      rowCount: 1,
      hash: 'hash-a',
      title: 'Domain A',
    }),
    catalogKnowledgeDocument({
      id: 'table:kdt-b',
      tableId: 'kdt-b',
      domain: 'domain-b',
      tableKey: 'measured_load_points',
      rowCount: 0,
      hash: 'hash-b',
      title: 'Domain B',
    }),
  ];
  installKnowledgeRowsHarness({
    documents,
    lookups,
    writes,
    loader: {
      async fetchAllRows(tableId, { signal } = {}) {
        calls.push({ tableId, signal });
        if (tableId !== 'kdt-a') {
          return { rows: [], rowCount: 0, contentHash: 'hash-b' };
        }
        started.resolve();
        await new Promise((resolve, reject) => {
          const cancel = () => {
            const error = new Error('ROWS_CANCELLED: domain-change');
            error.name = 'AbortError';
            error.code = 'ROWS_CANCELLED';
            reject(error);
          };
          if (signal?.aborted) {
            cancel();
            return;
          }
          signal?.addEventListener('abort', cancel, { once: true });
        });
        return { rows: [{ source_id: 'SRC-1' }], rowCount: 1, contentHash: 'hash-a' };
      },
    },
  });

  try {
    const first = hooks.loadKnowledgeBases({ domains: ['domain-a'] });
    await started.promise;
    assert.equal(hooks.knowledgeTableRowStates()['table:kdt-a'].phase, 'loading');
    const second = hooks.loadKnowledgeBases({ domains: ['domain-b'] });
    const [basesA, basesB] = await Promise.all([first, second]);
    assert.equal(calls[0].tableId, 'kdt-a');
    assert.equal(calls[0].signal.aborted, true);
    assert.equal(basesA[0].tables[0].rows, undefined);
    assert.equal(basesB[0].domain, 'domain-b');
    assert.deepEqual(basesB[0].tables[0].rows, []);
    assert.equal(hooks.knowledgeTableRowStates()['table:kdt-a'], undefined);
    assert.deepEqual(lookups, []);
    assert.deepEqual(writes, []);
  } finally {
    restoreKnowledgeRowsHarness();
  }
});

test('catalog row fetches stay at three tables in flight', async () => {
  const lookups = [];
  const writes = [];
  let active = 0;
  let maxActive = 0;
  const releases = [];
  const documents = [1, 2, 3, 4].map((index) => catalogKnowledgeDocument({
    id: `table:kdt-${index}`,
    tableId: `kdt-${index}`,
    domain: 'domain-pool',
    tableKey: `measured_load_points_${index}`,
    rowCount: 1,
    hash: `hash-${index}`,
    title: `Table ${index}`,
  }));
  installKnowledgeRowsHarness({
    documents,
    lookups,
    writes,
    loader: {
      async fetchAllRows(tableId) {
        active += 1;
        maxActive = Math.max(maxActive, active);
        await new Promise((resolve) => {
          releases.push(resolve);
        });
        active -= 1;
        return { rows: [{ tableId }], rowCount: 1, contentHash: `hash-${tableId.slice(-1)}` };
      },
    },
  });

  try {
    const pending = hooks.loadKnowledgeBases({ domains: ['domain-pool'] });
    await waitUntil(() => releases.length >= 3);
    assert.equal(releases.length, 3);
    assert.equal(maxActive, 3);
    releases[0]();
    await waitUntil(() => releases.length === 4);
    assert.equal(maxActive, 3);
    releases.slice(1).forEach((release) => release());
    const bases = await pending;
    assert.equal(bases[0].tables.length, 4);
    assert.equal(bases[0].tables.every((table) => table.rows.length === 1), true);
    assert.deepEqual(lookups, []);
    assert.deepEqual(writes, []);
  } finally {
    restoreKnowledgeRowsHarness();
  }
});
