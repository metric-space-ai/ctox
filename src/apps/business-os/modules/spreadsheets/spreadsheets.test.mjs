import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import fs from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join as joinPath } from 'node:path';
// Lader (OLDTEST): esbuild aus dem Klon, sonst ESBUILD_MODULE, sonst aus dem Haupt-Checkout
// ~/Documents/ctox/src/apps/business-os/node_modules (wie die v4-api-Tests mit PLAYWRIGHT_MODULE). Nicht installieren.
const requireDev = createRequire(import.meta.url);
function loadDev(name, envVar) {
  try { return requireDev(name); } catch {
    return requireDev(process.env[envVar] || joinPath(homedir(), 'Documents/ctox/src/apps/business-os/node_modules', name));
  }
}
const { build } = loadDev('esbuild', 'ESBUILD_MODULE');

// Lader (OLDTEST): wie der Service Worker – ctox:shared/x → shared/x (§16.15); ctox:lib/<name>@<major>/ ist eine
// Shell-Standardbibliothek, die erst zur Laufzeit gebunden wird (dynamischer Import) – im Bündel extern lassen.
const CTOX_SPECIFIERS = {
  name: 'ctox-specifiers',
  setup(b) {
    b.onResolve({ filter: /^ctox:shared\// }, (args) => ({ path: fileURLToPath(new URL(`../../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)) }));
    b.onResolve({ filter: /^ctox:lib\// }, (args) => ({ path: args.path, external: true }));
  },
};
const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
  plugins: [CTOX_SPECIFIERS],
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __spreadsheetsTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

for (const format of ['csv', 'unsupported']) {
  test(`spreadsheet ${format} editor failure never advertises saved metadata as a loaded file`, async t => {
    t.mock.method(console, 'error', () => {});
    const label = { textContent: '' };
    const badge = { hidden: false, classList: { toggle() {} }, querySelector: () => label };
    const head = {
      querySelector: selector => selector === '[data-spreadsheets-dirty-indicator]' ? badge : null,
      querySelectorAll: () => [],
    };
    let canvas;
    const shell = {
      replaceChildren(_head, next) { canvas = next; },
      querySelector: selector => selector === '[data-spreadsheets-canvas]' ? canvas : null,
    };
    const previousDocument = Object.getOwnPropertyDescriptor(globalThis, 'document');
    Object.defineProperty(globalThis, 'document', { configurable: true, value: {
      createElement: () => ({ isConnected: true, setAttribute() {} }),
    } });
    t.after(() => {
      if (previousDocument) Object.defineProperty(globalThis, 'document', previousDocument);
      else delete globalThis.document;
    });
    const reports = [];
    const state = {
      disposed: false, selectedId: 'sheet', selectedVersion: { id: 'version' },
      versionLoad: { selection: 'sheet', versionId: 'version', status: 'ready' },
      spreadsheets: [{ id: 'sheet', title: 'Test', filename: `test.${format}`, current_version_id: 'version' }],
      officeEngine: 'unavailable-test-engine', t: (_key, fallback) => fallback,
      // App-API 4.0: Fenster-Root aus mount(root, api) statt ctx.host; Meldungen über api.status.
      root: { isConnected: true, querySelector(selector) {
        if (selector === '[data-spreadsheets-editor]') return shell;
        if (selector === 'template[data-spreadsheets-head="editor"]') {
          return { content: { cloneNode: () => head } };
        }
        return null;
      } },
      api: { status: { report: (entry) => reports.push(entry), clear() {} }, metrics: { get: () => ({ backlog: 0 }) } },
    };
    const pending = hooks.renderCenter(state);
    assert.equal(badge.hidden, true, 'status stays hidden while opening');
    await pending;
    // v4 (R11): der Fehler geht in die Zustandszone, nie als Kasten in den Inhalt.
    assert.deepEqual(reports.map((entry) => entry.code), [format === 'csv' ? 'spreadsheets.editor' : 'spreadsheets.format.unsupported']);
    assert.equal(canvas?.innerHTML, undefined);
    assert.equal(badge.hidden, true, 'failure cannot show the saved badge');
    assert.equal(state.editorHandle, null);
  });
}

test('spreadsheet refresh for unrelated changes reads no collection and no version', async () => {
  // App-API 4.0: Bibliothek und Runbooks kommen über Abos (api.query(...).subscribe); ein Refresh liest nur die
  // Version des ausgewählten Blatts, und nur, wenn sich spreadsheets/spreadsheet_versions geändert haben.
  const reads = [];
  const api = {
    get: (collection, id) => { reads.push(`get:${collection}`); return { get: () => (id === 'v2' ? { id: 'v2', spreadsheet_id: 'sheet' } : null) }; },
    query: (collection) => ({ get: () => { reads.push(`query:${collection}`); return []; }, subscribe: () => () => {} }),
  };
  const state = { api, root: { isConnected: true, querySelector: () => null },
    spreadsheets: [{ id: 'sheet', current_version_id: 'v2' }], selectedId: 'sheet', selectedVersion: { id: 'v1' } };
  await hooks.refreshSpreadsheetsFromLocal(state, new Set(['files']));
  assert.deepEqual(reads, []);
  await hooks.refreshSpreadsheetsFromLocal(state, new Set(['spreadsheet_versions']));
  assert.deepEqual(reads, ['get:spreadsheet_versions']);
  assert.equal(state.selectedVersion.id, 'v2');
});

test('spreadsheet background refresh does not render after disposal during a read', async () => {
  let active = true;
  const api = {
    // Die Fensterschließung fällt mitten in das Lesen der Version.
    get: () => { active = false; return { get: () => ({ id: 'v2', spreadsheet_id: 'sheet' }) }; },
    query: () => ({ get: () => [], subscribe: () => () => {} }),
  };
  const state = { api, spreadsheets: [{ id: 'sheet', current_version_id: 'v2' }], selectedId: 'sheet', selectedVersion: null,
    root: { isConnected: true, querySelector: () => assert.fail('disposed app must not render') } };
  await hooks.refreshSpreadsheetsFromLocal(state, new Set(['spreadsheets']), () => active);
  assert.equal(active, false);
});

function versionState(read) {
  // App-API 4.0: Versionen liegen lokal vollständig vor (api.get/api.query synchron); kein Transportfehler, kein Retry.
  return {
    selectedId: 'sheet', selectedVersion: null, editorHandle: null,
    spreadsheets: [{ id: 'sheet', current_version_id: 'version' }],
    dirty: false, saving: false,
    api: {
      get: () => ({ get: read }),
      query: () => ({ get: () => [], subscribe: () => () => {} }),
    },
  };
}

test('spreadsheet loader distinguishes missing metadata and a ready version', async () => {
  const absent = versionState(() => null);
  await hooks.loadSelectedVersion(absent);
  assert.equal(hooks.currentSpreadsheetVersionLoad(absent).status, 'missing');
  assert.equal(absent.selectedVersion, null);
  const version = { id: 'version', data: { cells: [1] } };
  const ready = versionState(() => version);
  await hooks.loadSelectedVersion(ready);
  assert.equal(hooks.currentSpreadsheetVersionLoad(ready).status, 'ready');
  assert.deepEqual(ready.selectedVersion, version);
  assert.notEqual(ready.selectedVersion, version, 'the editor works on its own copy of the frozen live row');
});

for (const change of ['dispose', 'draft']) {
  test(`spreadsheet loader cannot commit after ${change}`, async () => {
    let reads = 0;
    const state = versionState(() => { reads += 1; return { id: 'stale' }; });
    if (change === 'dispose') state.disposed = true;
    if (change === 'draft') {
      state.dirty = true;
      state.editorHandle = { kind: 'ctox-spreadsheets', recordId: 'sheet', activity: 1 };
    }
    assert.equal(await hooks.loadSelectedVersion(state), null);
    assert.equal(state.selectedVersion, null);
    assert.equal(reads, 0);
    assert.equal(state.dirty, change === 'draft');
    assert.equal(hooks.currentSpreadsheetVersionLoad(state), null);
  });
}

test('spreadsheet chrome is a two-pane file manager without a right runbook column', async () => {
  const [source, html, manifest] = await Promise.all([
    fs.readFile(new URL('./index.js', import.meta.url), 'utf8'),
    fs.readFile(new URL('./index.html', import.meta.url), 'utf8'),
    fs.readFile(new URL('./module.json', import.meta.url), 'utf8').then(JSON.parse),
  ]);
  assert.match(source, /data-spreadsheets-new[^\n]+[\s\S]{0,140}requestBlankSpreadsheet/);
  assert.match(source, /const BLANK_GRID_DATA/);
  // v4: es gibt keine rechte Spalte mehr (renderRight ist leer), statt sie zur Laufzeit zu verstecken.
  assert.match(source, /function renderRight\(\) \{\}/);
  assert.doesNotMatch(source, /rightPaneEl/);
  assert.doesNotMatch(html, /data-spreadsheets-head="runbooks"/);
  assert.doesNotMatch(html, /data-spreadsheets-toggle-actions/);
  assert.equal(manifest.layout.right, undefined);
});

test('spreadsheet records without is_deleted remain visible', () => {
  assert.equal(hooks.isActiveSpreadsheetRecord({ id: 'sheet_1' }), true);
  assert.equal(hooks.isActiveSpreadsheetRecord({ id: 'sheet_1', is_deleted: false }), true);
  assert.equal(hooks.isActiveSpreadsheetRecord({ id: 'sheet_1', is_deleted: true }), false);
});

test('visibleSpreadsheets filters normalized rows by status, tag, search, and sort', () => {
  const state = {
    searchQuery: 'budget',
    statusFilter: 'Imported',
    tagFilter: 'finance',
    sortBy: 'title_asc',
    spreadsheets: [
      hooks.normalizeSpreadsheetRecord({
        id: 'sheet_2',
        title: 'Zeta Budget',
        filename: 'zeta.csv',
        status: 'Imported',
        tags: ['finance'],
        updated_at_ms: 20,
      }),
      hooks.normalizeSpreadsheetRecord({
        id: 'sheet_1',
        title: 'Alpha Budget',
        filename: 'alpha.csv',
        status: 'Imported',
        tags: ['finance'],
        updated_at_ms: 10,
      }),
      hooks.normalizeSpreadsheetRecord({
        id: 'sheet_3',
        title: 'Alpha Forecast',
        filename: 'forecast.csv',
        status: 'Draft',
        tags: ['finance'],
        updated_at_ms: 30,
      }),
    ],
  };

  assert.deepEqual(hooks.visibleSpreadsheets(state).map((record) => record.id), ['sheet_1', 'sheet_2']);
});

test('new spreadsheet validation requires a title before persistence', () => {
  assert.equal(hooks.validateNewSpreadsheetInput({ title: '' }).valid, false);
  assert.equal(hooks.validateNewSpreadsheetInput({ title: '  ' }).valid, false);
  assert.equal(hooks.validateNewSpreadsheetInput({ title: 'Budget 2026' }).valid, true);
});

test('import validation requires a supported spreadsheet file', () => {
  assert.equal(hooks.validateImportInput({ file: null }).valid, false);
  assert.equal(hooks.validateImportInput({ file: new File(['a,b'], 'budget.csv', { type: 'text/csv' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['a\tb'], 'budget.tsv', { type: 'text/tab-separated-values' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['PK'], 'budget.xlsx', { type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'notes.txt', { type: 'text/plain' }) }).valid, false);
});

test('browser-provided Research lineage is retained but cannot self-authorize factual tables', () => {
  const snapshotHash = `sha256:${'a'.repeat(64)}`;
  const ingestion = hooks.normalizeSpreadsheetIngestion({
    source_kind: 'research_generated',
    linked_records: [
      { kind: 'source_receipt', id: 'source-7', snapshot_hash: snapshotHash },
      { kind: 'claim', id: 'claim-7', evidence_id: 'evidence-7' },
    ],
    source_receipt_snapshot_hashes: [snapshotHash],
    knowledge_version: { version_id: 'knowledge-v7' },
  });

  assert.equal(ingestion.kind, 'research_generated');
  assert.equal(ingestion.valid, false);
  assert.match(ingestion.message, /confirmed provenance/i);
  assert.deepEqual(ingestion.linkedRecords, [
    { kind: 'source_receipt', id: 'source-7', snapshot_hash: snapshotHash },
    { kind: 'claim', id: 'claim-7', evidence_id: 'evidence-7' },
  ]);
  assert.deepEqual(ingestion.sourceReceiptSnapshotHashes, [snapshotHash]);
  assert.deepEqual(ingestion.knowledgeVersion, { version_id: 'knowledge-v7' });
});

test('Research spreadsheet ingestion fails closed when lineage is incomplete', () => {
  const ingestion = hooks.normalizeSpreadsheetIngestion({
    source_kind: 'research_generated',
    linked_records: [{ kind: 'claim', id: 'claim-without-receipt' }],
    knowledge_version: 'knowledge-v7',
  });

  assert.equal(ingestion.valid, false);
  assert.match(ingestion.message, /confirmed provenance/i);
  assert.throws(() => hooks.assertSpreadsheetIngestionAllowed(ingestion), (error) => {
    assert.equal(error.code, 'SPREADSHEET_LINEAGE_REQUIRED');
    return true;
  });
});

test('ordinary spreadsheet uploads remain explicit user imports', () => {
  const ingestion = hooks.normalizeSpreadsheetIngestion({
    filename: 'budget.csv',
    linked_records: [],
  });

  assert.equal(ingestion.kind, 'user_import');
  assert.equal(ingestion.valid, true);
  assert.deepEqual(ingestion.linkedRecords, []);
});

test('unresolved sourceFileId fails closed instead of becoming a user import', async () => {
  const sourceFiles = {
    findOne(id) {
      assert.equal(id, 'missing-source-file');
      return { exec: async () => null };
    },
  };
  const ingestion = await hooks.resolveSpreadsheetIngestion({
    ctx: {
      db: {
        collection(name) {
          assert.equal(name, 'desktop_files');
          return sourceFiles;
        },
      },
    },
  }, {
    sourceFileId: 'missing-source-file',
    filename: 'budget.csv',
  });

  assert.equal(ingestion.valid, false);
  assert.notEqual(ingestion.kind, 'user_import');
  assert.match(ingestion.message, /source file.*could not be resolved/i);
  assert.throws(() => hooks.assertSpreadsheetIngestionAllowed(ingestion), (error) => {
    assert.equal(error.code, 'SPREADSHEET_LINEAGE_REQUIRED');
    return true;
  });
});

test('file opening validates requested provenance before same-hash deduplication', async () => {
  const collectionCalls = [];
  const state = {
    spreadsheets: [{ id: 'existing-sheet', source_sha256: 'same-source-hash' }],
    ctx: {
      db: {
        collection(name) {
          collectionCalls.push(name);
          throw new Error(`deduplication should not read ${name}`);
        },
      },
    },
  };

  await assert.rejects(
    hooks.openSpreadsheetFile(state, {
      file: new File(['a,b\n1,2'], 'budget.csv', { type: 'text/csv' }),
      source_kind: 'research_generated',
    }),
    (error) => {
      assert.equal(error.code, 'SPREADSHEET_LINEAGE_REQUIRED');
      return true;
    },
  );
  assert.deepEqual(collectionCalls, []);
});

async function snapshotReportInput(overrides = {}) {
  const file = new File(['PK snapshot fixture'], 'outbound-snapshot.xlsx', {
    type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  });
  const hash = new Uint8Array(await crypto.subtle.digest('SHA-256', await file.arrayBuffer()));
  const fileSha256 = Array.from(hash, (byte) => byte.toString(16).padStart(2, '0')).join('');
  return {
    file,
    source_kind: 'research_generated',
    open_purpose: 'snapshot_report',
    report_snapshot: {
      source_module: 'outbound-lead-generation',
      source_collection: 'outbound_lead_generation_leads',
      source_record_ids: ['saved-lead-1'],
      captured_at_ms: 1790970000000,
      file_sha256: fileSha256,
    },
    ...overrides,
  };
}

// Exercise openFile through the real persistence path of App-API 4.0: bytes via api.files.put (one blob, the
// reference is usable after `local`), records via api.put. The live subscription is simulated: a put on
// `spreadsheets` appears in state.spreadsheets like the pushed query result. No editor/native conversion here.
function snapshotReportState(initialRecords = []) {
  const rows = new Map([
    ['spreadsheets', initialRecords.map((row) => ({ ...row }))],
    ['spreadsheet_versions', []],
    ['files', []],
  ]);
  const writes = [];
  const acknowledged = [];
  const events = [];
  const blobs = new Map();
  let state;
  const api = {
    get: (name, id) => ({ get: () => (rows.get(name) || []).find((row) => row.id === id) || null }),
    query: (name) => ({ get: () => [...(rows.get(name) || [])], subscribe: () => () => {} }),
    put(name, row) {
      events.push(`put:${name}`);
      writes.push({ name, row });
      rows.get(name).push({ ...row });
      if (name === 'spreadsheets') {
        state.rawSpreadsheets.push({ ...row });
        state.spreadsheets.push(hooks.normalizeSpreadsheetRecord({ ...row }));
      }
      return { local: Promise.resolve() };
    },
    files: {
      put(blob, meta) {
        events.push('file-put');
        writes.push({ name: 'files', meta });
        blobs.set(meta.id, blob);
        rows.get('files').push({ id: meta.id, ref: `ref:${meta.id}` });
        return {
          local: Promise.resolve().then(() => { events.push('source-ack'); acknowledged.push(meta.id); }),
          ref: Promise.resolve(`ref:${meta.id}`),
        };
      },
      async get(ref) { return blobs.get(String(ref).replace(/^ref:/, '')) || null; },
    },
    command() { throw new Error('opening a snapshot must not dispatch research'); },
    status: { report() {}, clear() {} },
    metrics: { get: () => ({ backlog: 0 }) },
  };
  state = {
    rawSpreadsheets: initialRecords.map((row) => ({ ...row })),
    spreadsheets: initialRecords.map((row) => hooks.normalizeSpreadsheetRecord({ ...row })),
    selectedId: initialRecords[0]?.id || '',
    selectedVersion: null,
    disposed: false,
    t: (_key, fallback) => fallback,
    api,
    root: { isConnected: false, querySelector: () => null },
  };
  return {
    state, rows, writes, acknowledged, events, blobs,
    remount() {
      return {
        ...state,
        rawSpreadsheets: rows.get('spreadsheets').map((row) => ({ ...row })),
        spreadsheets: rows.get('spreadsheets').map((row) => hooks.normalizeSpreadsheetRecord({ ...row })),
        selectedVersion: null,
        editorHandle: null,
        dirty: false,
        saving: false,
      };
    },
  };
}

test('explicit saved-state XLSX report persists its exact bytes and non-evidence origin', async () => {
  const input = await snapshotReportInput();
  const fixture = snapshotReportState();
  const opened = await hooks.openSpreadsheetFile(fixture.state, input);
  assert.ok(opened?.id);
  assert.equal(fixture.rows.get('spreadsheets').length, 1);
  assert.equal(fixture.rows.get('spreadsheet_versions').length, 1);
  for (const record of [opened, fixture.rows.get('spreadsheet_versions')[0]]) {
    assert.equal(record.source_kind, 'research_generated');
    assert.equal(record.ingestion_kind, 'research_generated');
    assert.deepEqual(record.knowledge_lineage.report_snapshot, input.report_snapshot);
    assert.equal(record.knowledge_lineage.open_purpose, 'snapshot_report');
    assert.equal(record.knowledge_lineage.evidence_eligible, false);
  }
  assert.equal(opened.source_sha256, input.report_snapshot.file_sha256);
  const version = fixture.rows.get('spreadsheet_versions')[0];
  const persisted = fixture.blobs.get(version.blob_id);
  assert.deepEqual(Buffer.from(await persisted.arrayBuffer()), Buffer.from(await input.file.arrayBuffer()));
  assert.deepEqual(fixture.acknowledged, [version.blob_id]);
  const ackIndex = fixture.events.indexOf('source-ack');
  assert.ok(ackIndex >= 0);
  for (const name of ['spreadsheet_versions', 'spreadsheets']) {
    assert.ok(fixture.events.indexOf(`put:${name}`) > ackIndex,
      `${name} must not publish a source reference before acknowledgement`);
  }
  const reopened = await hooks.openSpreadsheetFile(fixture.remount(), input);
  assert.equal(reopened.id, opened.id);
  assert.equal(fixture.rows.get('spreadsheets').length, 1, 'same file and report origin reuse the record');
  assert.deepEqual(reopened.knowledge_lineage.report_snapshot, input.report_snapshot);
  assert.equal(reopened.knowledge_lineage.evidence_eligible, false);
});

test('same XLSX bytes cannot reuse an ordinary import or a different report origin', async () => {
  const input = await snapshotReportInput();
  const fixture = snapshotReportState([{
    id: 'ordinary-upload', filename: input.file.name, status: 'Imported',
    source_kind: 'user_import', ingestion_kind: 'user_import',
    source_sha256: input.report_snapshot.file_sha256,
  }]);
  const report = await hooks.openSpreadsheetFile(fixture.state, input);
  assert.notEqual(report.id, 'ordinary-upload');
  const otherInput = { ...input, report_snapshot: { ...input.report_snapshot, source_record_ids: ['saved-lead-2'] } };
  const otherReport = await hooks.openSpreadsheetFile(fixture.state, otherInput);
  assert.notEqual(otherReport.id, report.id);
  const upload = await hooks.openSpreadsheetFile(fixture.state, { file: input.file });
  assert.equal(upload.id, 'ordinary-upload', 'ordinary import must not inherit research report origin');
  assert.equal(fixture.rows.get('spreadsheets').length, 3);
});

test('snapshot reports reject missing or mismatched descriptors before persistence', async () => {
  const input = await snapshotReportInput();
  for (const report_snapshot of [
    undefined,
    { ...input.report_snapshot, file_sha256: '' },
    { ...input.report_snapshot, file_sha256: '0'.repeat(64) },
    { ...input.report_snapshot, source_record_ids: [] },
    { ...input.report_snapshot, source_module: '' },
    { ...input.report_snapshot, source_collection: '' },
    { ...input.report_snapshot, captured_at_ms: -1 },
    { ...input.report_snapshot, captured_at_ms: Number.MAX_SAFE_INTEGER },
    { ...input.report_snapshot, source_record_ids: [null] },
  ]) {
    const fixture = snapshotReportState();
    await assert.rejects(hooks.openSpreadsheetFile(fixture.state, { ...input, report_snapshot }));
    assert.deepEqual(fixture.writes, []);
    assert.deepEqual(fixture.acknowledged, []);
  }
});

test('snapshot digest is checked before reusing a matching saved report', async () => {
  const input = await snapshotReportInput();
  const fixture = snapshotReportState([{
    id: 'existing-report', source_sha256: input.report_snapshot.file_sha256,
    source_kind: 'research_generated', ingestion_kind: 'research_generated',
    knowledge_lineage: {
      open_purpose: 'snapshot_report', evidence_eligible: false,
      report_snapshot: input.report_snapshot,
    },
  }]);
  await assert.rejects(hooks.openSpreadsheetFile(fixture.state, {
    ...input, report_snapshot: { ...input.report_snapshot, file_sha256: '0'.repeat(64) },
  }), (error) => error.code === 'SPREADSHEET_LINEAGE_REQUIRED');
  assert.deepEqual(fixture.writes, []);
  assert.deepEqual(fixture.acknowledged, []);
});

// FIX5: Die Digest-Prüfung steht genau einmal (openSpreadsheetFile, vor Wiederverwendung und Import). Ein
// gespeicherter Bericht mit derselben (falschen) Beschreibung würde ohne sie wiederverwendet, ein neuer ohne sie
// angelegt – beide Wege müssen ablehnen.
test('snapshot digest guards reuse of a saved report with the same stale descriptor', async () => {
  const input = await snapshotReportInput();
  const stale = { ...input.report_snapshot, file_sha256: '0'.repeat(64) };
  const fixture = snapshotReportState([{
    id: 'stale-report', source_sha256: input.report_snapshot.file_sha256,
    source_kind: 'research_generated', ingestion_kind: 'research_generated',
    knowledge_lineage: { open_purpose: 'snapshot_report', evidence_eligible: false, report_snapshot: stale },
  }]);
  fixture.state.selectedId = '';
  await assert.rejects(hooks.openSpreadsheetFile(fixture.state, { ...input, report_snapshot: stale }),
    (error) => error.code === 'SPREADSHEET_LINEAGE_REQUIRED');
  assert.equal(fixture.state.selectedId, '', 'the stale saved report is not opened');
  assert.deepEqual(fixture.writes, []);
});

test('snapshot purpose cannot turn an unresolved source file into an import', async () => {
  const fixture = snapshotReportState();
  const input = await snapshotReportInput({ sourceFileId: 'missing-source-file' });
  await assert.rejects(hooks.openSpreadsheetFile(fixture.state, input), /could not be resolved/i);
  assert.deepEqual(fixture.writes, []);
});

test('report provenance assertions cannot authorize the evidence opening path', async () => {
  const input = await snapshotReportInput();
  const fixture = snapshotReportState();
  await assert.rejects(hooks.openSpreadsheetFile(fixture.state, {
    ...input,
    open_purpose: 'evidence',
    // An already-normalized-looking caller object is still untrusted input.
    valid: true,
    kind: 'research_generated',
    evidence_eligible: true,
    provenance_verified: true,
    knowledge_lineage: { evidence_eligible: true, report_snapshot: input.report_snapshot },
  }), (error) => error.code === 'SPREADSHEET_LINEAGE_REQUIRED');
  assert.deepEqual(fixture.writes, []);
});

test('file-open deduplication reuses the imported spreadsheet with the same source hash', () => {
  const records = [
    { id: 'sheet_other', source_sha256: 'aaaa' },
    { id: 'sheet_loads', source_sha256: 'BEEF' },
  ];
  assert.equal(hooks.spreadsheetBySourceSha(records, 'beef')?.id, 'sheet_loads');
  assert.equal(hooks.spreadsheetBySourceSha(records, 'missing'), null);
});

test('supported records always use the real CTOX Office spreadsheet engine', () => {
  assert.equal(hooks.isOfficeSpreadsheetRecord({ filename: 'loads.csv', mime_type: 'text/csv' }), true);
  assert.equal(hooks.isOfficeSpreadsheetRecord({ filename: 'loads.xlsx' }), true);
  assert.equal(hooks.isOfficeSpreadsheetRecord({ filename: 'loads.tsv' }), true);
  assert.equal(hooks.isOfficeSpreadsheetRecord({ filename: 'model.json', mime_type: 'application/json' }), false);
});

test('malformed spreadsheet models normalize to a renderable grid', () => {
  const model = hooks.normalizeSpreadsheetModel({ data: [['A', 'B']] });
  assert.deepEqual(model.data, [['A', 'B']]);
  assert.equal(model.columns.length, 2);
});

test('CSV serialization quotes only when required, preserving numeric round-trip', () => {
  // Plain and numeric cells stay unquoted so their type survives re-import.
  assert.equal(hooks.escapeCsvCell(30), '30');
  assert.equal(hooks.escapeCsvCell('plain'), 'plain');
  assert.equal(hooks.escapeCsvCell(''), '');
  // Delimiters, quotes, newlines, and edge whitespace force quoting.
  assert.equal(hooks.escapeCsvCell('a,b'), '"a,b"');
  assert.equal(hooks.escapeCsvCell('a"b'), '"a""b"');
  assert.equal(hooks.escapeCsvCell('line1\nline2'), '"line1\nline2"');
  assert.equal(hooks.escapeCsvCell(' pad '), '" pad "');

  assert.equal(
    hooks.rowsToCsv([['Name', 'Total'], ['Acme, Inc', 30], ['', 'plain']]),
    'Name,Total\n"Acme, Inc",30\n,plain'
  );
});

test('spreadsheet bytes are persisted as one file through api.files', async () => {
  // App-API 4.0: keine spreadsheet_blob_chunks mehr; die Bytes gehen als EIN Blob über api.files.put, die Referenz
  // wird erst nach der lokalen Annahme (handle.local) zurückgegeben.
  const puts = [];
  let accepted = false;
  const api = { files: { put(blob, meta) {
    puts.push({ blob, meta });
    // Lokale Annahme kommt später als die Referenz: wer nicht auf `local` wartet, gibt die Referenz zu früh heraus.
    return { local: new Promise((resolve) => setTimeout(() => { accepted = true; resolve(); }, 5)), ref: Promise.resolve('sha-ref') };
  } } };
  const bytes = new Uint8Array(260 * 1024);
  bytes.fill(67);
  const out = await hooks.saveBlobChunks({ api }, {
    blobId: 'sheet_blob_bulk',
    spreadsheetId: 'sheet_bulk',
    versionId: 'sheet_version_bulk',
    mimeType: 'application/octet-stream',
    bytes,
    name: 'bulk.xlsx',
  });
  assert.equal(puts.length, 1, 'bytes are written through one api.files.put call');
  assert.equal(puts[0].blob.size, bytes.byteLength);
  assert.equal(puts[0].meta.id, 'sheet_blob_bulk');
  assert.equal(accepted, true, 'the reference is exposed only after the local acceptance');
  assert.deepEqual(out, { blob_id: 'sheet_blob_bulk', ref: 'sha-ref' });
});

test('spreadsheets context menu remains scoped to the mounted module host', async () => {
  const source = await fs.readFile(new URL('./index.js', import.meta.url), 'utf8');
  // v4: Kontextmenü hängt am Fenster-Root aus mount(root, api).
  assert.match(source, /const moduleHost = state\.root;/);
  assert.match(source, /moduleHost\.append\(menu\)/);
  assert.doesNotMatch(source, /document\.body\.append\(menu\)/);
});
