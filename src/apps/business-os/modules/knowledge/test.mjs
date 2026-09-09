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

const { __knowledgeTestHooks: hooks } = await importBrowserBundle('./index.js');

const {
  buildKnowledgeBundles,
  isInternalSkillOnlyGroup,
  canEditSelectedMarkdown,
  isKnowledgeActionFormReady,
  isKnowledgeTabDisabled,
  knowledgeItemsFromTables,
  mergeKnowledgeTableChunks,
  knowledgeGroupMatchesDomain,
  knowledgeListStateHtml,
  runCoalescedRefresh,
  validateKnowledgeTableChunks,
  dataFrameCompleteness,
  localDataFrameRows,
  localDataFrameSchema,
  mergeKnowledgeTableData,
  canonicalCellValue,
  columnHeaderHelp,
  columnHeaderLabel,
  dataframeToCsv,
  formatCell,
  normalizeColumns,
  normalizeStoredKnowledgeRecord,
  knowledgeResourcesForEntries,
  sourceScopeFor,
  sortKnowledgeRecords,
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

test('hides internal skill-only groups from the default customer knowledge view', () => {
  assert.equal(isInternalSkillOnlyGroup({
    entries: [{
      id: 'skill:system/review',
      kind: 'skill',
      source_path: 'embedded:skills/system/review/SKILL.md',
    }],
    runbookIds: [],
    tableIds: [],
  }), true);

  assert.equal(isInternalSkillOnlyGroup({
    entries: [{
      id: 'skillbook:drone-bearing-design',
      kind: 'skillbook',
      source_path: '/home/ctox/knowledge/drone-bearing-design.md',
    }],
    runbookIds: [],
    tableIds: [],
  }), false);
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

test('merges physical table chunks into one logical dataframe before grouping', () => {
  const [table] = mergeKnowledgeTableChunks([
    {
      id: 'table:load-points:chunk:1',
      payload: {
        id: 'table:load-points:chunk:1',
        logical_table_id: 'table:load-points',
        domain: 'drone_bearing_design_verified',
        chunk_index: 1,
        chunk_count: 2,
        rows_complete: true,
        rows: [{ measurement_id: 'MLP-002' }],
      },
    },
    {
      id: 'table:load-points:chunk:0',
      payload: {
        id: 'table:load-points:chunk:0',
        logical_table_id: 'table:load-points',
        domain: 'drone_bearing_design_verified',
        chunk_index: 0,
        chunk_count: 2,
        rows_complete: true,
        rows: [{ measurement_id: 'MLP-001' }],
      },
    },
  ]);

  assert.equal(table.id, 'table:load-points');
  assert.equal(table.payload.logical_table_id, 'table:load-points');
  assert.equal(table.payload.source_chunk_count, 2);
  assert.equal(table.payload.chunk_count, 1);
  assert.equal(table.payload.rows_complete, true);
  assert.deepEqual(table.payload.rows.map((row) => row.measurement_id), ['MLP-001', 'MLP-002']);

  const groups = buildKnowledgeBundles([], [], [table]);
  const hub = groups.find((group) => group.id === 'research/drone-design/drone-bearing-loads');
  assert.ok(hub);
  assert.deepEqual(hub.tableIds, ['table:load-points']);
});

test('matches a Research handoff to a Knowledge group by entry domain', () => {
  const group = {
    id: 'research/drone-design/drone-bearing-loads',
    domain: 'drone_design',
    entries: [{ id: 'table:loads', payload: { domain: 'drone_bearing_design' } }],
  };
  assert.equal(knowledgeGroupMatchesDomain(group, 'drone_bearing_design'), true);
  assert.equal(knowledgeGroupMatchesDomain(group, 'unrelated_domain'), false);
});

test('groups linked SKF skillbooks, runbooks, resources, and tables into one domain hub', () => {
  const groups = buildKnowledgeBundles([
    {
      id: 'skillbook:drone-bearing-design-verified-v1',
      kind: 'skillbook',
      title: 'Verified propeller input evidence',
      linked_runbook_ids: ['runbook:verification'],
    },
    {
      id: 'runbook:verification',
      kind: 'runbook',
      title: 'Prüf- und Validierungsverfahren',
      problem_domain: 'UAS bearing verification',
    },
    {
      id: 'resource:source-001',
      kind: 'resource',
      title: 'Source without domain keywords',
      skillbook_id: 'drone-bearing-design-verified-v1',
    },
  ], [], [{
    id: 'table:verified-measurements',
    kind: 'dataframe',
    title: 'Measurements',
    payload: {
      domain: 'drone_bearing_design_verified',
      rows: [{ measurement_id: 'M-001' }],
      schema: { columns: [{ name: 'measurement_id', type: 'string' }] },
    },
  }]);

  const hub = groups.find((group) => group.id === 'research/drone-design/drone-bearing-loads');
  assert.ok(hub);
  assert.equal(hub.domain, 'drone_bearing_design_verified');
  assert.deepEqual(new Set(hub.entries.map((entry) => entry.id)), new Set([
    'skillbook:drone-bearing-design-verified-v1',
    'runbook:verification',
    'resource:source-001',
    'table:verified-measurements',
  ]));
  assert.ok(hub.runbookIds.includes('runbook:verification'));
  assert.equal(groups.some((group) => group.id === 'bundle/drone-bearing-design-verified-v1'), false);
});

test('associates ordinary persisted runbooks by id, skillbook id, domain, and source path exactly once', () => {
  const groups = buildKnowledgeBundles([
    {
      id: 'skillbook:alpha-ops',
      kind: 'skillbook',
      title: 'Alpha Operations',
      linked_runbook_ids: ['runbook:explicit-alpha'],
    },
    {
      id: 'skillbook:finance-book',
      kind: 'skillbook',
      title: 'Finance Book',
      domain: 'finance-controls',
      source_path: '/packs/finance-book/SKILL.md',
    },
    {
      id: 'skillbook:shared-domain-book',
      kind: 'skillbook',
      title: 'Shared Domain Book',
      domain: 'finance-controls',
    },
  ], [
    { id: 'runbook:explicit-alpha', title: 'Explicit Alpha' },
    { id: 'runbook:alpha-by-id', skillbook_id: 'alpha-ops', title: 'Alpha by ID' },
    { id: 'runbook:finance-domain', domain: 'finance-controls', title: 'Finance by domain' },
    { id: 'runbook:finance-path', source_path: '/packs/finance-book/runbooks/month-close.md', title: 'Finance by path' },
    { id: 'runbook:finance-path', source_path: '/packs/finance-book/runbooks/duplicate.md', title: 'Duplicate record' },
    { id: 'runbook:specific-owner', skillbook_id: 'shared-domain-book', domain: 'finance-controls', title: 'Specific owner wins' },
  ], []);

  const alpha = groups.find((group) => group.title === 'Alpha Operations');
  const finance = groups.find((group) => group.title === 'Finance Book');
  const sharedDomain = groups.find((group) => group.title === 'Shared Domain Book');
  assert.deepEqual(new Set(alpha.runbookIds), new Set(['runbook:explicit-alpha', 'runbook:alpha-by-id']));
  assert.deepEqual(new Set(finance.runbookIds), new Set(['runbook:finance-domain', 'runbook:finance-path']));
  assert.deepEqual(sharedDomain.runbookIds, ['runbook:specific-owner']);

  const assignments = groups.flatMap((group) => group.runbookIds);
  for (const id of new Set(assignments)) {
    assert.equal(assignments.filter((candidate) => candidate === id).length, 1, `${id} is assigned once`);
  }
});

test('treats source_path records as visible sources without duplicating explicit resources', () => {
  const resources = knowledgeResourcesForEntries([
    { id: 'skillbook:with-source', kind: 'skillbook', source_path: '/packs/with-source/SKILL.md' },
    { id: 'resource:manual', kind: 'resource', source_path: '/docs/manual.pdf' },
    { id: 'note:no-source', kind: 'note' },
    { id: 'resource:manual', kind: 'resource', source_path: '/docs/duplicate.pdf' },
  ]);

  assert.deepEqual(resources.map((entry) => entry.id), ['skillbook:with-source', 'resource:manual']);
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

test('sorts locally loaded records without requiring an RxDB query index', () => {
  const records = sortKnowledgeRecords([
    { id: 'older', updated_at_ms: 100 },
    { id: 'same-b', updated_at_ms: 200 },
    { id: 'same-a', updated_at_ms: 200 },
  ]);

  assert.deepEqual(records.map((record) => record.id), ['same-a', 'same-b', 'older']);
});

test('assembles only complete contiguous knowledge table chunks', () => {
  const result = validateKnowledgeTableChunks([
    { chunk_index: 1, chunk_count: 2, row_offset: 2, rows_total: 3, rows_complete: true, rows: [{ id: 'b' }] },
    { chunk_index: 0, chunk_count: 2, row_offset: 0, rows_total: 3, rows_complete: true, rows: [{ id: 'a' }, { id: 'a2' }] },
  ]);

  assert.equal(result.complete, true);
  assert.equal(result.expectedRows, 3);
  assert.deepEqual(result.rows.map((row) => row.id), ['a', 'a2', 'b']);
});

test('fails closed for duplicate or non-contiguous chunk indexes', () => {
  const duplicate = validateKnowledgeTableChunks([
    { chunk_index: 0, chunk_count: 2, row_offset: 0, rows_total: 2, rows: [{ id: 'a' }] },
    { chunk_index: 0, chunk_count: 2, row_offset: 1, rows_total: 2, rows: [{ id: 'b' }] },
  ]);
  const gap = validateKnowledgeTableChunks([
    { chunk_index: 0, chunk_count: 2, row_offset: 0, rows_total: 2, rows: [{ id: 'a' }] },
    { chunk_index: 2, chunk_count: 2, row_offset: 1, rows_total: 2, rows: [{ id: 'b' }] },
  ]);

  assert.equal(duplicate.complete, false);
  assert.deepEqual(duplicate.rows, []);
  assert.match(duplicate.reason, /duplicate/);
  assert.equal(gap.complete, false);
  assert.match(gap.reason, /contiguous/);
});

test('fails closed for conflicting chunk count, offsets, totals, and rows_complete', () => {
  const conflictingCount = validateKnowledgeTableChunks([
    { chunk_index: 0, chunk_count: 2, row_offset: 0, rows_total: 2, rows: [{ id: 'a' }] },
    { chunk_index: 1, chunk_count: 3, row_offset: 1, rows_total: 2, rows: [{ id: 'b' }] },
  ]);
  const invalidOffset = validateKnowledgeTableChunks([
    { chunk_index: 0, chunk_count: 2, row_offset: 0, rows_total: 2, rows: [{ id: 'a' }] },
    { chunk_index: 1, chunk_count: 2, row_offset: 2, rows_total: 2, rows: [{ id: 'b' }] },
  ]);
  const invalidTotal = validateKnowledgeTableChunks([
    { chunk_index: 0, chunk_count: 2, row_offset: 0, rows_total: 3, rows: [{ id: 'a' }] },
    { chunk_index: 1, chunk_count: 2, row_offset: 1, rows_total: 3, rows: [{ id: 'b' }] },
  ]);
  const incompleteFlag = validateKnowledgeTableChunks([
    { chunk_index: 0, chunk_count: 2, row_offset: 0, rows_total: 2, rows_complete: false, rows: [{ id: 'a' }] },
    { chunk_index: 1, chunk_count: 2, row_offset: 1, rows_total: 2, rows_complete: true, rows: [{ id: 'b' }] },
  ]);

  for (const result of [conflictingCount, invalidOffset, invalidTotal, incompleteFlag]) {
    assert.equal(result.complete, false);
    assert.deepEqual(result.rows, []);
  }
  assert.match(conflictingCount.reason, /chunk_count/);
  assert.match(invalidOffset.reason, /gap|overlap/);
  assert.match(invalidTotal.reason, /row total/);
  assert.match(incompleteFlag.reason, /rows_complete/);
});

test('marks chunked dataframe completeness in the browser data model', () => {
  const incomplete = dataFrameCompleteness({
    chunks: [{ chunk_index: 0, chunk_count: 2, row_offset: 0, rows_total: 2, rows: [{ id: 'only' }] }],
  });

  assert.equal(incomplete.complete, false);
  assert.deepEqual(incomplete.rows, []);
});

test('coalesces refresh requests into one trailing refresh', async () => {
  const status = { refreshInFlight: false, refreshPending: false };
  let runs = 0;
  let releaseFirst;
  const firstRefresh = new Promise((resolve) => { releaseFirst = resolve; });
  const refresh = async () => {
    runs += 1;
    if (runs === 1) await firstRefresh;
  };

  const first = runCoalescedRefresh(status, refresh);
  await Promise.resolve();
  await Promise.all([
    runCoalescedRefresh(status, refresh),
    runCoalescedRefresh(status, refresh),
    runCoalescedRefresh(status, refresh),
  ]);
  assert.equal(status.refreshPending, true);
  releaseFirst();
  await first;

  assert.equal(runs, 2);
  assert.equal(status.refreshInFlight, false);
  assert.equal(status.refreshPending, false);
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

test('standardizes dataframe headers with units and hover help', () => {
  const [propeller, thrust, loadCase] = normalizeColumns([
    { name: 'propeller_size', label: 'Propeller size' },
    { name: 'thrust_N', type: 'number' },
    { name: 'load_case', type: 'string' },
  ]);

  assert.equal(columnHeaderLabel(propeller), 'Propellergröße (Durchmesser x Steigung, mm)');
  assert.match(columnHeaderHelp(propeller), /9x5 bedeutet 9 Zoll Durchmesser und 5 Zoll Steigung/);
  assert.equal(columnHeaderLabel(thrust), 'Kraft (N)');
  assert.match(columnHeaderHelp(thrust), /Newton/i);
  assert.equal(columnHeaderLabel(loadCase), 'Load Case');
  assert.doesNotMatch(columnHeaderHelp(loadCase), /Source unit: N/);

  const [torque] = normalizeColumns([{ name: 'torque_Nm', label: 'Torque N m', unit: 'N m' }]);
  assert.equal(columnHeaderLabel(torque), 'Moment/Torque (N m)');
});

test('formats factual numeric values without locale separators', () => {
  const [thrust, length] = normalizeColumns([
    { name: 'thrust_N', type: 'number' },
    { name: 'arm_length', unit: 'in', type: 'number' },
  ]);

  assert.equal(formatCell(1234.5, thrust), '1234,5');
  assert.equal(formatCell('1.234,50', thrust), '1234,5');
  assert.equal(formatCell(9, length), '228,6');
});

test('infers inch source units from dataframe column names and exports metric values', () => {
  const [diameter, pitch] = normalizeColumns([
    { name: 'prop_diameter_in', label: 'Prop Diameter In (mm)', unit: 'mm', type: 'number' },
    { name: 'prop_pitch_in', label: 'Prop Pitch In (mm)', unit: 'mm', type: 'number' },
  ]);

  assert.equal(columnHeaderLabel(diameter), 'Durchmesser (mm)');
  assert.equal(columnHeaderLabel(pitch), 'Steigung (mm)');
  assert.match(columnHeaderHelp(diameter), /Source unit: in/);
  assert.match(columnHeaderHelp(diameter), /Shown\/exported metric unit: mm/);
  assert.equal(formatCell(9, diameter), '228,6');
  assert.equal(formatCell(5, pitch), '127');
  assert.equal(dataframeToCsv([diameter, pitch], [{ prop_diameter_in: 9, prop_pitch_in: 5 }]), 'Durchmesser (mm);Steigung (mm)\n228,6;127');
});

test('normalizes propeller sizes from inch shorthand to metric dimensions', () => {
  const [propeller] = normalizeColumns([{ name: 'propeller_size', label: 'Propeller size' }]);

  assert.equal(canonicalCellValue('9x5', propeller), '228,6 x 127');
  assert.equal(canonicalCellValue('10.5x4.5', propeller), '266,7 x 114,3');
});

test('exports dataframe CSV with metric headers and Excel-friendly numeric cells', () => {
  const columns = normalizeColumns([
    { name: 'propeller_size', label: 'Propeller size' },
    { name: 'thrust_N', type: 'number' },
  ]);
  const csv = dataframeToCsv(columns, [
    { propeller_size: '9x5', thrust_N: '1.234,50' },
  ]);

  assert.equal(csv, 'Propellergröße (Durchmesser x Steigung, mm);Kraft (N)\n228,6 x 127;1234,5');
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

test('presentation follows compact Business OS knowledge contract', async () => {
  const css = await readFile(fileURLToPath(new URL('./index.css', import.meta.url)), 'utf8');
  const html = await readFile(fileURLToPath(new URL('./index.html', import.meta.url)), 'utf8');
  const js = await readFile(fileURLToPath(new URL('./index.js', import.meta.url)), 'utf8');
  const manifest = JSON.parse(await readFile(fileURLToPath(new URL('./module.json', import.meta.url)), 'utf8'));

  assert.doesNotMatch(html, /ctox-pane--glass/);
  assert.doesNotMatch(css, /border-(?:left|right):\s*(?:[2-9]|[0-9]{2,})px/);
  assert.doesNotMatch(css, /border-radius:\s*(?:10|12|14|16|18|20|24)px/);
  assert.doesNotMatch(css, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
  assert.match(css, /--knowledge-shadow:\s*none;/);
  assert.match(css, /--knowledge-panel-radius:\s*var\(--surface-radius\)/);
  assert.match(css, /--knowledge-control-radius:\s*var\(--control-radius\)/);
  // Der 128px-Pilot wurde am 31.08.2026 nach visueller Abnahme verworfen:
  // Knowledge folgt der geteilten 80px/37px-Geometrie und definiert keine
  // eigenen Shell-Tokens mehr.
  assert.doesNotMatch(css, /--shell-v2-icon-size:\s*128px/);
  assert.doesNotMatch(css, /--shell-v2-header-row-size:\s*64px/);
  // Pilot-Steuerungsgroesse (64px) mit dem 128px-Icon verworfen (31.08.2026).
  assert.doesNotMatch(css, /shell-window-control--close\s*\{[\s\S]*?width:\s*64px;/);
  assert.match(css, /knowledge-left > \.ctox-pane-header\s*\{[\s\S]*?padding-left:\s*calc\(var\(--shell-v2-icon-size, 80px\) \+ 8px\);/);
  assert.match(css, /\.knowledge-filterbar\s*\{[\s\S]*?padding-left:\s*calc\(var\(--shell-v2-icon-size, 80px\) \+ 8px\);/);
  assert.match(css, /\.ctox-column-resizer::before\s*\{[\s\S]*?width:\s*2px;[\s\S]*?height:\s*100%;/);
  assert.match(css, /\.ctox-column-resizer\.is-active::before\s*\{[\s\S]*?width:\s*6px;/);
  // Shards are pure selectors: no inline expansion machinery — the content
  // pane's tabs + second-level switcher are the only navigation into a group.
  assert.doesNotMatch(css, /bundle-caret|knowledge-bundle-items/);
  assert.match(css, /\.bundle-meta\s*\{/);
  // The full-height divider starts at the top and centers only horizontally.
  assert.match(css, /\.ctox-column-resizer::before\s*\{[^}]*left:\s*50%;[^}]*top:\s*0;[^}]*transform:\s*translateX\(-50%\)/);
  assert.deepEqual(
    [...new Set([...css.matchAll(/@container business-app-window \(max-width:\s*(\d+)px\)/g)].map((match) => Number(match[1])))].sort((a, b) => a - b),
    [768, 1024],
    'responsive stages follow the two shell breakpoints',
  );
  assert.match(css, /\.knowledge-app-overlay\s*\{[\s\S]*?position:\s*absolute;[\s\S]*?inset:\s*0/);
  assert.match(js, /function openKnowledgeOverlay/);
  assert.doesNotMatch(js, /state\.ctx\.open(?:Left|Right|Bottom)Drawer/);
  assert.match(js, /els\.markdownView\.innerHTML = .{0,20}\$\{escapeHtml\(copy\.detailEmptyHint/);
  assert.equal(manifest.layout.min_width, 360);
  assert.equal(manifest.presentation.minimum_size.width, 360);
});

test('pane chrome follows the canonical data-pg-* grammar contract', async () => {
  const js = await readFile(fileURLToPath(new URL('./index.js', import.meta.url)), 'utf8');
  const html = await readFile(fileURLToPath(new URL('./index.html', import.meta.url)), 'utf8');
  const css = await readFile(fileURLToPath(new URL('./index.css', import.meta.url)), 'utf8');

  // The shell wires search / tray / reset / band / counts / footer from
  // the markup (autoWirePaneGrammar); the module never re-codes that chrome.
  for (const attr of ['data-pg-search', 'data-pg-tray-toggle', 'data-pg-tray', 'data-pg-reset', 'data-pg-filter', 'data-pg-band', 'data-pg-count', 'data-pg-footer']) {
    assert.match(html, new RegExp(attr), `index.html carries ${attr}`);
  }
  // Standardansicht seit 31.08.2026: Karten (Detail-Shards); Liste ist die
  // kompakte Alternative.
  assert.match(html, /data-pg-default-view="cards"/);
  // knowledge-view-toggle ist seit 31.08.2026 der EINE modul-verdrahtete
  // Umschalt-Knopf (Betreiber-Direktive); verboten bleiben Kicker und das
  // alte shell-verdrahtete Knopfpaar.
  assert.doesNotMatch(html, /ctox-pane-kicker|data-pg-view=/);
  assert.equal((html.match(/<button[^>]*data-knowledge-view-toggle/g) || []).length, 1);
  // The module consumes the grammar through the bubbling change event and the
  // null-guarded pane handle — not through hand-rolled search/tray wiring.
  assert.match(js, /ctox-pane-grammar-change/);
  assert.match(js, /__ctoxPaneGrammar/);
  assert.doesNotMatch(js, /data-action="toggle-filters"|data-action="reset-filters"|\[data-view-mode\]|\[data-tab\]/);
  // Import/Export stay collected header actions (top-right icons).
  assert.match(html, /ctox-pane-actions[\s\S]*data-action="import-knowledge-book"[\s\S]*data-action="export-knowledge-book"/);
  for (const action of ['create-knowledge-book', 'import-knowledge-book', 'export-knowledge-book']) {
    assert.match(js, new RegExp(`querySelector\\('\\[data-action="${action}"\\]\\'\\)\\?\\.addEventListener\\('click'`));
  }
  // Selection is signaled canonically and flipped in place, never via a list
  // rebuild: aria-selected + is-selected on existing rows.
  assert.match(js, /applyKnowledgeSelection/);
  assert.match(js, /aria-selected/);
  assert.doesNotMatch(js, /aria-current/);
  assert.doesNotMatch(css, /aria-current/);
  // The counted band keeps ≥2 real views with zeros rendered by the grammar.
  assert.match(html, /data-pg-band="skill"/);
  assert.match(html, /data-pg-band="runbooks"/);
  assert.match(html, /data-pg-band="resources"/);
  assert.match(html, /data-pg-band="data"/);
  assert.match(html, /data-skillbook-switcher/);
  assert.match(html, /data-resource-switcher/);
  // Per-pane one-line footers; no module-wide app floor.
  assert.doesNotMatch(html, /knowledge-footer/);
  assert.doesNotMatch(css, /\.knowledge-footer\b/);
  // Kit tokens are owned by the kit (shared/base.css), never re-defined here.
  assert.doesNotMatch(css, /--kit-fill:\s|--kit-hover:\s|--kit-fill-strong:\s|--focus-ring:\s/);
  // No module-owned localStorage persistence (ctx.storageScope is the rule).
  assert.doesNotMatch(js, /localStorage/);
  // Markup is fetched from index.html with the JS cache-buster (single source).
  assert.match(js, /loadModuleMarkup/);
  assert.match(js, /\.\/index\.html/);
});

test('data-driven empty states show syncing shell only until the collection is live', () => {
  const syncing = knowledgeListStateHtml({
    dataDriven: true,
    readiness: { ready: false, state: 'catching-up' },
    message: 'Keine Knowledge-Einträge gefunden.',
    syncingText: 'Daten werden synchronisiert.',
  });
  assert.match(syncing, /class="ctox-syncing"/);
  assert.match(syncing, /role="status"/);
  assert.match(syncing, /aria-live="polite"/);
  assert.match(syncing, /Daten werden synchronisiert\./);

  const offlinePending = knowledgeListStateHtml({
    dataDriven: true,
    readiness: { ready: false, state: 'offline-pending' },
    message: 'Keine Knowledge-Einträge gefunden.',
    syncingText: 'Daten werden synchronisiert.',
  });
  assert.match(offlinePending, /class="ctox-syncing"/);

  const empty = knowledgeListStateHtml({
    dataDriven: true,
    readiness: { ready: true, state: 'live' },
    message: 'Keine Knowledge-Einträge gefunden.',
    syncingText: 'Daten werden synchronisiert.',
  });
  assert.match(empty, /class="ctox-empty"/);
  assert.doesNotMatch(empty, /ctox-syncing/);
  assert.match(empty, /Keine Knowledge-Einträge gefunden\./);

  // Unknown readiness fails open (no spinner forever), and selection/filter
  // empties never get the syncing shell even while the collection syncs.
  const unknown = knowledgeListStateHtml({ dataDriven: true, readiness: null, message: 'm', syncingText: 's' });
  assert.match(unknown, /class="ctox-empty"/);
  const selectionEmpty = knowledgeListStateHtml({
    dataDriven: false,
    readiness: { ready: false, state: 'catching-up' },
    message: 'Keine Runbooks vorhanden.',
    syncingText: 'Daten werden synchronisiert.',
  });
  assert.match(selectionEmpty, /class="ctox-empty"/);
  assert.doesNotMatch(selectionEmpty, /ctox-syncing/);
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
