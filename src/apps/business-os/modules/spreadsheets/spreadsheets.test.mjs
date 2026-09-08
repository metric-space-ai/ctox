import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import fs from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
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
    const state = {
      disposed: false, selectedId: 'sheet', selectedVersion: { id: 'version' },
      versionLoad: { selection: 'sheet', versionId: 'version', status: 'ready' },
      spreadsheets: [{ id: 'sheet', title: 'Test', filename: `test.${format}`, current_version_id: 'version' }],
      officeEngine: 'unavailable-test-engine', t: (_key, fallback) => fallback,
      ctx: { host: { isConnected: true, querySelector(selector) {
        if (selector === '[data-spreadsheets-editor]') return shell;
        if (selector === 'template[data-spreadsheets-head="editor"]') {
          return { content: { cloneNode: () => head } };
        }
        return null;
      } } },
    };
    const pending = hooks.renderCenter(state);
    assert.equal(badge.hidden, true, 'status stays hidden while opening');
    await pending;
    assert.match(canvas.innerHTML, /spreadsheets-error/);
    assert.equal(badge.hidden, true, 'failure cannot show the saved badge');
    assert.equal(state.editorHandle, null);
  });
}

test('spreadsheet chunk refresh does not re-query the file library or runbooks', async () => {
  const queried = [];
  const state = { spreadsheets: [], selectedId: '', ctx: {
    host: { querySelector: () => null },
    db: { collection(name) { queried.push(name); return { find: () => ({ exec: async () => [] }) }; } },
  } };
  await hooks.refreshSpreadsheetsFromLocal(state, new Set(['spreadsheet_blob_chunks']));
  assert.deepEqual(queried, []);
  await hooks.refreshSpreadsheetsFromLocal(state, new Set(['spreadsheets']));
  assert.deepEqual(queried, ['spreadsheets']);
});

test('spreadsheet background refresh does not render after disposal during a read', async () => {
  let active = true;
  let release;
  const held = new Promise(resolve => { release = resolve; });
  const state = { spreadsheets: [], selectedId: '', ctx: {
    host: { querySelector: () => assert.fail('disposed app must not render') },
    db: { collection() { return { find: () => ({ exec: () => held }) }; } },
  } };
  const pending = hooks.refreshSpreadsheetsFromLocal(state, new Set(['spreadsheets']), () => active);
  active = false;
  release([]);
  await pending;
});

test('spreadsheet version reads return locally without starting replication', async () => {
  const version = { id: 'local' };
  assert.equal(await hooks.resolveSpreadsheetVersionLocalFirst(async timeoutMs => {
    assert.equal(timeoutMs, 4500);
    return version;
  }, () => assert.fail('must not wait for sync')), version);
});

test('spreadsheet metadata deadline recovers once with a bounded network read', async () => {
  const budgets = [];
  let recoveries = 0;
  const version = { id: 'reconnected' };
  assert.equal(await hooks.resolveSpreadsheetVersionLocalFirst(async timeoutMs => {
    budgets.push(timeoutMs);
    if (budgets.length === 1) {
      return hooks.withSpreadsheetVersionTimeout(new Promise(() => {}), 1, 'deadline');
    }
    return version;
  }, async () => { recoveries += 1; }), version);
  assert.deepEqual(budgets, [4500, 60000]);
  assert.equal(recoveries, 1);
});

test('spreadsheet metadata recovery does not relabel integrity or permission failures', async () => {
  for (const code of ['permission_denied', 'blob_sha256_mismatch']) {
    const error = Object.assign(new Error(code), { code });
    assert.equal(hooks.isTransientSpreadsheetVersionReadError(error), false);
    await assert.rejects(hooks.resolveSpreadsheetVersionLocalFirst(
      () => hooks.withSpreadsheetVersionTimeout(Promise.reject(error), 100, 'deadline'),
      () => assert.fail('must not recover non-transient failure'),
    ), actual => actual === error);
  }
});

for (const failures of [1, 2, 3]) {
  test(`spreadsheet peer-reopen failure is bounded after ${failures} reads`, async () => {
    const error = new Error('Timed out waiting for WebRTC peer reopen for spreadsheet_versions');
    const version = { id: 'reopened' };
    const budgets = [];
    let recoveries = 0;
    const pending = hooks.resolveSpreadsheetVersionLocalFirst(async budget => {
      budgets.push(budget);
      if (budgets.length <= failures) throw error;
      return version;
    }, async () => { recoveries += 1; });
    if (failures === 3) await assert.rejects(pending, actual => actual === error);
    else assert.equal(await pending, version);
    assert.deepEqual(budgets, failures === 1 ? [4500, 60000] : [4500, 60000, 60000]);
    assert.equal(recoveries, Math.min(failures, 2));
  });
}

test('spreadsheet metadata absence remains absence after successful recovery', async () => {
  let reads = 0;
  let recoveries = 0;
  assert.equal(await hooks.resolveSpreadsheetVersionLocalFirst(async () => {
    reads += 1;
    return null;
  }, async () => { recoveries += 1; }), null);
  assert.equal(reads, 2);
  assert.equal(recoveries, 1);
});

test('spreadsheet terminal codes override a misleading transport error message', async () => {
  for (const code of ['permission_denied', 'schema_validation_failed', 'blob_sha256_mismatch']) {
    const error = Object.assign(new Error('Timed out waiting for WebRTC peer reopen for spreadsheet_versions'), { code });
    assert.equal(hooks.isTransientSpreadsheetVersionReadError(error), false);
    await assert.rejects(hooks.resolveSpreadsheetVersionLocalFirst(async () => { throw error; },
      () => assert.fail('terminal error must not reconnect')), actual => actual === error);
  }
});

for (const boundary of ['before-read', 'read-result', 'read-error', 'recovery-result', 'recovery-error']) {
  test(`spreadsheet supersession at ${boundary} cancels without applying or retrying`, async () => {
    let current = boundary !== 'before-read';
    let reads = 0;
    let recoveries = 0;
    const result = await hooks.resolveSpreadsheetVersionLocalFirst(async () => {
      reads += 1;
      if (boundary === 'read-result') { current = false; return { id: 'stale' }; }
      if (boundary === 'read-error') current = false;
      if (reads === 1) throw new Error('query_cancelled');
      return { id: 'stale' };
    }, async () => {
      recoveries += 1;
      current = false;
      if (boundary === 'recovery-error') throw new Error('query_cancelled');
    }, { isCurrent: () => current });
    assert.equal(result, null);
    assert.equal(reads, boundary === 'before-read' ? 0 : 1);
    assert.equal(recoveries, boundary.startsWith('recovery') ? 1 : 0);
  });
}

test('spreadsheet recovery resolves a pending direct bridge and waits only for its peer', async () => {
  const events = [];
  await hooks.awaitSpreadsheetVersionReplication({ sync: {
    async startCollection(name, options) {
      assert.equal(name, 'spreadsheet_versions');
      assert.deepEqual(options, { forceDirect: true });
      events.push('direct');
      return { ready: Promise.resolve({ state: {
        async waitForOpenPeerId(timeoutMs) {
          assert.equal(timeoutMs, 60000);
          events.push('native-peer');
          return 'native';
        },
        async awaitInitialReplication() { assert.fail('no full collection download'); },
        async awaitInSync() { assert.fail('no full collection synchronization'); },
      } }) };
    },
  } });
  assert.deepEqual(events, ['direct', 'native-peer']);
});

test('spreadsheet recovery propagates a refused direct channel without another read', async () => {
  let reads = 0;
  let starts = 0;
  const refused = new Error('forbidden');
  await assert.rejects(hooks.resolveSpreadsheetVersionLocalFirst(async () => { reads += 1; return null; },
    () => hooks.awaitSpreadsheetVersionReplication({ sync: {
      async startCollection() { starts += 1; throw refused; },
    } })), actual => actual === refused);
  assert.equal(reads, 1);
  assert.equal(starts, 1);
});

test('spreadsheet failed reconnect attempts consume the bounded recovery budget', async () => {
  let recoveries = 0;
  let reads = 0;
  const error = new Error('Timed out waiting for WebRTC peer reopen for spreadsheet_versions');
  await assert.rejects(hooks.resolveSpreadsheetVersionLocalFirst(async () => {
    reads += 1;
    return null;
  }, async () => { recoveries += 1; throw error; }), actual => actual === error);
  assert.equal(reads, 1);
  assert.equal(recoveries, 2);
});

test('spreadsheet cancelled direct bridge never continues to readiness or peer wait', async () => {
  for (const boundary of ['start', 'ready']) {
    let current = true;
    await hooks.awaitSpreadsheetVersionReplication({ sync: {
      async startCollection() {
        if (boundary === 'start') current = false;
        return { ready() {
          assert.equal(boundary, 'ready');
          current = false;
          return Promise.resolve({ state: {
            waitForOpenPeerId() { assert.fail('cancelled readiness must not wait for peer'); },
          } });
        } };
      },
    } }, () => current);
  }
});

function versionState(read) {
  return {
    selectedId: 'sheet', selectedVersion: null, editorHandle: null,
    spreadsheets: [{ id: 'sheet', current_version_id: 'version' }],
    dirty: false, saving: false,
    ctx: { db: { collection() { return {
      findOne: () => ({ exec: read }),
      find: () => ({ exec: async () => [] }),
    }; } }, sync: { startCollection: async () => ({}) } },
  };
}

test('spreadsheet loader distinguishes failed transport, missing metadata, and ready version', async t => {
  t.mock.method(console, 'warn', () => {});
  const error = new Error('Timed out waiting for WebRTC peer reopen for spreadsheet_versions');
  const state = versionState(async () => { throw error; });
  await hooks.loadSelectedVersion(state);
  assert.equal(hooks.currentSpreadsheetVersionLoad(state).status, 'error');
  assert.equal(state.versionLoad.error, error);
  assert.equal(state.selectedVersion, null);
  const absent = versionState(async () => null);
  await hooks.loadSelectedVersion(absent);
  assert.equal(hooks.currentSpreadsheetVersionLoad(absent).status, 'missing');
  const version = { id: 'version' };
  const ready = versionState(async () => ({ toJSON: () => version }));
  await hooks.loadSelectedVersion(ready);
  assert.equal(hooks.currentSpreadsheetVersionLoad(ready).status, 'ready');
  assert.equal(ready.selectedVersion, version);
});

for (const change of ['selection', 'dispose', 'draft']) {
  test(`spreadsheet loader cannot commit or retry after concurrent ${change}`, async () => {
    let release;
    let reads = 0;
    const held = new Promise(resolve => { release = resolve; });
    const state = versionState(async () => { reads += 1; return held; });
    const pending = hooks.loadSelectedVersion(state);
    if (change === 'selection') state.selectedId = 'other';
    if (change === 'dispose') state.disposed = true;
    if (change === 'draft') {
      state.dirty = true;
      state.editorHandle = { kind: 'ctox-spreadsheets', recordId: 'sheet', activity: 1 };
    }
    release({ toJSON: () => ({ id: 'stale' }) });
    assert.equal(await pending, null);
    assert.equal(state.selectedVersion, null);
    assert.equal(reads, 1);
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
  assert.match(source, /state\.rightPaneEl\.hidden = true/);
  assert.doesNotMatch(html, /data-spreadsheets-head="runbooks"/);
  assert.doesNotMatch(html, /data-spreadsheets-toggle-actions/);
  assert.equal(manifest.layout.right, undefined);
});

test('spreadsheet runtime waits for initial replication before reading collections', async () => {
  const events = [];
  const ready = await hooks.ensureSpreadsheetRuntimeReady({
    actions: {
      async ensureRuntimeReady() {
        events.push('ready');
      },
    },
  });

  assert.equal(ready, true);
  assert.deepEqual(events, ['ready']);
  assert.equal(await hooks.ensureSpreadsheetRuntimeReady({}), false);
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

test('spreadsheet blob chunks are persisted with one bulk write', async () => {
  const bulkWrites = [];
  let acknowledged = false;
  const blobChunks = {
    bulkUpsert: async (docs) => {
      bulkWrites.push(docs);
      return docs.map(row => ({ toJSON: () => ({ ...row, _meta: { lwt: Date.now() } }) }));
    },
    insert: async () => { throw new Error('spreadsheet_blob_chunks insert must not run per chunk'); },
  };
  const ctx = {
    sync: { async leaseCollection() { return { bridge: { state: {
      async waitForOpenPeerId() { return 'native'; },
      async pushDocumentsToPeer(_peer, rows) { assert.equal(rows.length, bulkWrites[0].length); acknowledged = true; },
    } }, async release() {} }; } },
    db: {
      collection(name) {
        if (name === 'spreadsheet_blob_chunks') return blobChunks;
        return {};
      },
    },
  };

  const bytes = new Uint8Array(260 * 1024);
  bytes.fill(67);
  await hooks.saveBlobChunks(ctx, {
    blobId: 'sheet_blob_bulk',
    spreadsheetId: 'sheet_bulk',
    versionId: 'sheet_version_bulk',
    mimeType: 'application/octet-stream',
    bytes,
  });

  assert.equal(bulkWrites.length, 1, 'blob chunks are written through one bulkUpsert call');
  assert.ok(bulkWrites[0].length > 1, 'test payload spans multiple chunk documents');
  assert.equal(acknowledged, true, 'source bytes are acknowledged before exposing references');
});

test('empty spreadsheet explorer shows syncing only while the collection is unready', () => {
  const unready = { collection: 'spreadsheets', state: 'catching-up', ready: false, syncing: true, updatedAt: 0 };
  const offlinePending = { collection: 'spreadsheets', state: 'offline-pending', ready: false, syncing: false, updatedAt: 0 };
  const live = { collection: 'spreadsheets', state: 'live', ready: true, syncing: false, updatedAt: 1 };

  // Empty + unready ⇒ syncing shell (render hint, includes offline-pending).
  assert.equal(hooks.shouldRenderSpreadsheetsSyncing({ spreadsheets: [], spreadsheetsReadiness: unready }), true);
  assert.equal(hooks.shouldRenderSpreadsheetsSyncing({ spreadsheets: [], spreadsheetsReadiness: offlinePending }), true);
  // Empty + ready ⇒ regular ctox-empty.
  assert.equal(hooks.shouldRenderSpreadsheetsSyncing({ spreadsheets: [], spreadsheetsReadiness: live }), false);
  // Rows always win, regardless of readiness.
  assert.equal(hooks.shouldRenderSpreadsheetsSyncing({ spreadsheets: [{ id: 'sheet_1' }], spreadsheetsReadiness: unready }), false);
  // No readiness signal (older shells/tests) keeps the previous empty behaviour.
  assert.equal(hooks.shouldRenderSpreadsheetsSyncing({ spreadsheets: [] }), false);
  // Fallback reads the canonical shell API when no snapshot is cached.
  assert.equal(
    hooks.shouldRenderSpreadsheetsSyncing({
      spreadsheets: [],
      ctx: { sync: { collectionReadiness: (name) => (name === 'spreadsheets' ? unready : null) } },
    }),
    true,
  );
});

test('spreadsheets context menu remains scoped to the mounted module host', async () => {
  const source = await fs.readFile(new URL('./index.js', import.meta.url), 'utf8');
  assert.match(source, /const moduleHost = state\.ctx\?\.host/);
  assert.match(source, /moduleHost\.append\(menu\)/);
  assert.doesNotMatch(source, /document\.body\.append\(menu\)/);
});
