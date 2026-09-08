import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';

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

const { __documentsTestHooks: hooks } = await importBrowserBundle('./index.js');

test('native Word dirty events never republish a stale persisted version head', t => {
  const scheduled = [];
  const cancelled = [];
  t.mock.method(globalThis, 'setTimeout', (callback, delay) => {
    scheduled.push({ callback, delay });
    return scheduled.length;
  });
  t.mock.method(globalThis, 'clearTimeout', id => cancelled.push(id));
  const persisted = { id: 'word', current_version_id: 'v2', status: 'Final' };
  const record = { ...persisted, current_version_id: 'v3' };
  const handle = { kind: 'ctox-documents', recordId: 'word', activity: 7 };
  const state = { editorHandle: handle, selectedId: 'word', selectedVersion: { id: 'v3' },
    dirty: false, needsFinalSave: false, superdocSaveTimer: null,
    ctx: { db: { collection() { assert.fail('dirty event must not read or write replicated records'); } } },
  };
  hooks.markCtoxDocumentsDraft(state, record, handle);
  hooks.markCtoxDocumentsDraft(state, record, handle);
  assert.equal(handle.activity, 9);
  assert.equal(state.dirty, true);
  assert.equal(state.needsFinalSave, true);
  assert.equal(record.status, 'Draft');
  assert.equal(record.current_version_id, 'v3');
  assert.equal(state.selectedVersion.id, 'v3');
  assert.deepEqual(persisted, { id: 'word', current_version_id: 'v2', status: 'Final' });
  assert.deepEqual(scheduled.map(item => item.delay), [900, 900]);
  assert.deepEqual(cancelled, [1]);
});

for (const scenario of ['missing-head', 'missing-history', 'head-after-recovery', 'no-head']) {
  test(`Word version loading ${scenario} never rewrites the authoritative head`, async () => {
    const record = { id: 'word', title: 'Word', document_type: 'word_document',
      current_version_id: scenario === 'no-head' ? '' : 'v3' };
    let reads = 0;
    let fallbackReads = 0;
    const exactId = scenario === 'missing-history' ? 'history-v1' : 'v3';
    const state = { documents: [record], selectedId: 'word', selectedVersion: null,
      editorHandle: null, dirty: false,
      requestedVersionId: scenario === 'missing-history' ? exactId : '',
      requestedVersionDocumentId: 'word',
      ctx: {
        db: { collection(name) {
          assert.equal(name, 'document_versions', 'reading a version cannot write the documents collection');
          return {
            findOne(id) { assert.equal(id, exactId); return { async exec() {
              reads += 1;
              return scenario === 'head-after-recovery' && reads > 1
                ? { toJSON: () => ({ id: 'v3', document_id: 'word' }) } : null;
            } }; },
            find() {
              fallbackReads += 1;
              assert.equal(scenario, 'no-head', 'missing exact version must not fall back to another version');
              return { exec: async () => [{ toJSON: () => ({ id: 'v2', document_id: 'word' }) }] };
            },
          };
        } },
        sync: { startCollection: async () => ({ state: { waitForOpenPeerId: async () => 'native' } }) },
      },
    };
    const loaded = await hooks.loadSelectedVersion(state);
    assert.equal(loaded?.id || null, scenario === 'no-head' ? 'v2' : scenario === 'head-after-recovery' ? 'v3' : null);
    assert.equal(record.current_version_id, scenario === 'no-head' ? '' : 'v3');
    assert.equal(fallbackReads, scenario === 'no-head' ? 1 : 0);
  });
}

test('document chunk refresh does not re-query the file library, runbooks or Knowledge', async () => {
  const queried = [];
  const state = { documents: [], selectedId: '', ctx: {
    host: { querySelector: () => null },
    db: { collection(name) { queried.push(name); return { find: () => ({ exec: async () => [] }) }; } },
  } };
  await hooks.refreshDocumentsFromLocal(state, new Set(['document_blob_chunks']));
  assert.deepEqual(queried, []);
  await hooks.refreshDocumentsFromLocal(state, new Set(['documents']));
  assert.deepEqual(queried, ['documents']);
});

test('Knowledge refresh reads only changed collections and preserves other cached context', async () => {
  const queried = [];
  const items = [{ id: 'cached-item' }];
  const runbooks = [{ id: 'cached-runbook' }];
  const state = { knowledgeItems: items, knowledgeRunbooks: runbooks, knowledgeTables: [{ id: 'old' }], ctx: {
    db: { collection(name) { queried.push(name); return { find: () => ({ exec: async () => [] }) }; } },
  } };
  await hooks.refreshKnowledge(state, new Set(['knowledge_tables']));
  assert.deepEqual(queried, ['knowledge_tables']);
  assert.equal(state.knowledgeItems, items);
  assert.equal(state.knowledgeRunbooks, runbooks);
  assert.deepEqual(state.knowledgeTables, []);
});

test('document background refresh does not render after disposal during its read', async () => {
  let active = true;
  let release;
  const held = new Promise(resolve => { release = resolve; });
  const state = { documents: [], selectedId: '', ctx: {
    host: { querySelector: () => assert.fail('disposed app must not render') },
    db: { collection() { return { find: () => ({ exec: () => held }) }; } },
  } };
  const pending = hooks.refreshDocumentsFromLocal(state, new Set(['documents']), () => active);
  active = false;
  release([]);
  await pending;
});

test('document records without is_deleted are active', () => {
  assert.equal(hooks.isActiveDocumentRecord({ id: 'doc_1' }), true);
  assert.equal(hooks.isActiveDocumentRecord({ id: 'doc_1', is_deleted: false }), true);
  assert.equal(hooks.isActiveDocumentRecord({ id: 'doc_1', is_deleted: true }), false);
});

test('document launch arguments resolve the requested record for initial and repeated app opens', () => {
  assert.equal(hooks.documentIdFromLaunchArgs({ record: 'doc_17' }), 'doc_17');
  assert.equal(hooks.documentIdFromLaunchArgs({ documentId: 'doc_18' }), 'doc_18');
  assert.equal(hooks.documentIdFromLaunchArgs({ record: '  ' }), '');
  assert.equal(hooks.versionIdFromLaunchArgs({ version: 'doc_17_v2' }), 'doc_17_v2');
  assert.equal(hooks.versionIdFromLaunchArgs({ versionId: 'doc_18_v3' }), 'doc_18_v3');
});

test('active editor teardown is awaited once across concurrent renders', async () => {
  let destroyCalls = 0;
  let releaseDestroy;
  const state = {
    editorHandle: {
      destroy: async () => {
        destroyCalls += 1;
        await new Promise((resolve) => { releaseDestroy = resolve; });
      },
    },
    editorDestroyPromise: null,
  };

  const first = hooks.destroyActiveEditor(state);
  const second = hooks.destroyActiveEditor(state);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(destroyCalls, 1);
  releaseDestroy();
  await Promise.all([first, second]);
  assert.equal(destroyCalls, 1);
  assert.equal(state.editorHandle, null);
  assert.equal(state.editorDestroyPromise, null);
});

test('editor target identity keeps repeated realtime renders on the same document version', async () => {
  const key = hooks.editorRenderKey({ id: 'letter_1' }, { id: 'letter_1_recipient_7' }, 'ctox_documents');
  assert.equal(key, 'letter_1:letter_1_recipient_7:ctox_documents');
  assert.equal(hooks.editorRenderKey({ id: 'letter_1' }, null, 'ctox_documents'), '');

  let releaseMount;
  const mountPromise = new Promise((resolve) => { releaseMount = resolve; });
  const state = { editorHandle: null, editorMountKey: '', editorMountPromise: null };
  const tracked = hooks.trackEditorMount(state, key, mountPromise);

  assert.equal(hooks.currentEditorTarget(state, key), tracked);
  assert.equal(hooks.currentEditorTarget(state, 'letter_1:letter_1_recipient_8:ctox_documents'), null);

  releaseMount({ kind: 'ctox-documents', renderKey: key });
  await tracked;
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(state.editorMountKey, '');
  assert.equal(state.editorMountPromise, null);

  state.editorHandle = { kind: 'ctox-documents', renderKey: key };
  assert.deepEqual(await hooks.currentEditorTarget(state, key), state.editorHandle);
  assert.equal(hooks.isCurrentEditorRender({ renderSerial: 7, disposed: false }, 7), true);
  assert.equal(hooks.isCurrentEditorRender({ renderSerial: 7, disposed: true }, 7), false);
  assert.equal(hooks.isCurrentEditorRender({ renderSerial: 8, disposed: false }, 7), false);
});

test('only transient Office startup and load-version failures are retryable', () => {
  assert.equal(hooks.isTransientOfficeStartupError(new Error('CTOX product iframe load timed out')), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('Office RPC timed out: editor.ready')), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('Office RPC timed out: bridge.loadVersion')), true);
  assert.equal(hooks.isTransientOfficeStartupError(Object.assign(new Error('Office RPC timed out'), {
    code: 'rpc_timeout',
    details: { method: 'editor.open' },
  })), true);
  assert.equal(hooks.isTransientOfficeStartupError(Object.assign(new Error('Office RPC peer is closed'), {
    code: 'rpc_closed',
  })), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('CTOX Documents app-ready timed out')), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('CTOX document fork SDK load timed out')), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('WebRTC replication cancelled')), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('QUERY_CANCELLED while reading the document version')), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('Document SHA-256 mismatch')), false);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('Document blob was not found')), false);
  assert.equal(hooks.isTransientOfficeStartupError(Object.assign(new Error('Read access denied'), {
    code: 'permission_denied',
  })), false);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('Unsupported editor protocol')), false);
});

test('Office startup recovery works with the real default delay, not only an injected test clock', async () => {
  let attempts = 0;
  const result = await hooks.initializeOfficeEditorWithRecovery({
    initialize: async () => { if (++attempts === 1) throw new Error('temporary startup failure'); },
    shouldRetry: () => true,
    retryDelayMs: 1,
    maxRecoveryAttempts: 1,
  });
  assert.equal(attempts, 2);
  assert.deepEqual(result, { recovered: true, recoveryAttempts: 1 });
});

test('transient loadVersion timeout reinitializes the Office editor once and continues', async () => {
  const instances = [];
  const destroyedInstances = [];
  const listenerCleanup = [];
  const retryFeedback = [];
  const waits = [];
  const result = await hooks.initializeOfficeEditorWithRecovery({
    initialize: async () => {
      const instance = {
        id: instances.length + 1,
        async open() {
          if (this.id === 1) {
            throw Object.assign(new Error('Office RPC timed out: bridge.loadVersion'), {
              code: 'rpc_timeout',
              details: { method: 'bridge.loadVersion' },
            });
          }
        },
        async destroy() { destroyedInstances.push(this.id); },
      };
      instances.push(instance);
      await hooks.openOfficeEditorInstance(instance, { recordId: 'merge_1', versionId: 'merge_1_v2' }, [
        () => listenerCleanup.push(instance.id),
      ]);
    },
    isCurrent: () => true,
    shouldRetry: hooks.isTransientOfficeStartupError,
    onRetry: async (error, attempt) => retryFeedback.push({ message: error.message, attempt }),
    wait: async (ms) => waits.push(ms),
    retryDelayMs: 25,
    maxRecoveryAttempts: 1,
  });

  assert.deepEqual(instances.map(({ id }) => id), [1, 2]);
  assert.deepEqual(destroyedInstances, [1]);
  assert.deepEqual(listenerCleanup, [1]);
  assert.deepEqual(retryFeedback, [{ message: 'Office RPC timed out: bridge.loadVersion', attempt: 1 }]);
  assert.deepEqual(waits, [25]);
  assert.deepEqual(result, { recovered: true, recoveryAttempts: 1 });
});

test('document versions open from local state before waiting for full replication', async () => {
  const events = [];
  const localVersion = { id: 'merge_1_recipient_1' };
  const result = await hooks.resolveLocalFirst(
    async () => {
      events.push('local');
      return localVersion;
    },
    async () => events.push('sync'),
  );

  assert.equal(result, localVersion);
  assert.deepEqual(events, ['local']);
});

test('missing or transient local document versions wait for replication once', async () => {
  const events = [];
  let localReads = 0;
  const result = await hooks.resolveLocalFirst(
    async () => {
      localReads += 1;
      events.push(`local-${localReads}`);
      if (localReads === 1) throw new Error('WebRTC replication cancelled');
      return { id: 'merge_1_recipient_2' };
    },
    async () => events.push('sync'),
  );

  assert.deepEqual(result, { id: 'merge_1_recipient_2' });
  assert.deepEqual(events, ['local-1', 'sync', 'local-2']);
});

test('document metadata deadlines recover once with a bounded network read', async () => {
  const budgets = [];
  let recoveries = 0;
  const version = { id: 'reconnected_version' };
  const result = await hooks.resolveLocalFirst(async (timeoutMs) => {
    budgets.push(timeoutMs);
    if (budgets.length === 1) {
      return hooks.withDocumentVersionTimeout(new Promise(() => {}), 1, 'metadata deadline');
    }
    return version;
  }, async () => { recoveries += 1; });
  assert.equal(result, version);
  assert.deepEqual(budgets, [4500, 60000]);
  assert.equal(recoveries, 1);
});

test('document metadata timeout does not relabel permission or integrity errors', async () => {
  const error = Object.assign(new Error('permission denied'), { code: 'permission_denied' });
  let recoveries = 0;
  await assert.rejects(hooks.resolveLocalFirst(
    () => hooks.withDocumentVersionTimeout(Promise.reject(error), 100, 'deadline'),
    async () => { recoveries += 1; },
  ), actual => actual === error);
  assert.equal(recoveries, 0);
});

test('document recovery upgrades a follower and waits for its native peer, not full replication', async () => {
  const events = [];
  const direct = { state: {
    async waitForOpenPeerId(timeoutMs) {
      assert.equal(timeoutMs, 60000);
      events.push('native-peer');
      return 'native';
    },
    async awaitInitialReplication() { assert.fail('no full collection download'); },
    async awaitInSync() { assert.fail('no full collection synchronization'); },
  } };
  await hooks.awaitDocumentVersionReplication({ mode: 'follower' }, { sync: {
    async startCollection(name, options) {
      assert.equal(name, 'document_versions');
      assert.deepEqual(options, { forceDirect: true });
      events.push('direct');
      return { ready: Promise.resolve(direct) };
    },
  } });
  assert.deepEqual(events, ['direct', 'native-peer']);
});

test('document recovery propagates a refused channel without another metadata read', async () => {
  let reads = 0;
  let starts = 0;
  const refused = Object.assign(new Error('forbidden'), { code: 'permission_denied' });
  await assert.rejects(hooks.resolveLocalFirst(async () => { reads += 1; return null; },
    () => hooks.awaitDocumentVersionReplication(null, { sync: {
      async startCollection() { starts += 1; throw refused; },
    } })), actual => actual === refused);
  assert.equal(reads, 1);
  assert.equal(starts, 1);
});

test('non-retryable Office load failures fail closed without reinitialization', async () => {
  let initializeCalls = 0;
  let retryCalls = 0;
  const integrityError = Object.assign(new Error('Document SHA-256 mismatch'), {
    code: 'blob_sha256_mismatch',
  });

  await assert.rejects(
    hooks.initializeOfficeEditorWithRecovery({
      initialize: async () => {
        initializeCalls += 1;
        throw integrityError;
      },
      shouldRetry: hooks.isTransientOfficeStartupError,
      onRetry: async () => { retryCalls += 1; },
      wait: async () => {},
      retryDelayMs: 0,
      maxRecoveryAttempts: 1,
    }),
    integrityError,
  );
  assert.equal(initializeCalls, 1);
  assert.equal(retryCalls, 0);
});

test('documents module declares the shared Knowledge collections', async () => {
  const moduleJson = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));
  const registryJson = JSON.parse(await readFile(new URL('../registry.json', import.meta.url), 'utf8'));
  const collectionSchema = JSON.parse(await readFile(new URL('./collections.schema.json', import.meta.url), 'utf8'));
  const registryModule = registryJson.modules.find((item) => item.id === 'documents');
  const required = ['knowledge_items', 'knowledge_runbooks', 'knowledge_tables'];
  for (const name of required) {
    assert.ok(moduleJson.collections.includes(name));
    assert.ok(registryModule.collections.includes(name));
    assert.ok(collectionSchema.collections[name]);
  }
});

test('document knowledge context references a chunked table only once', () => {
  const tables = hooks.mergeKnowledgeTableReferences([
    {
      id: 'table:loads:chunk:0001',
      payload: {
        logical_table_id: 'table:loads',
        domain: 'drone_bearing_design_verified',
        chunk_index: 1,
        chunk_count: 2,
      },
    },
    {
      id: 'table:loads',
      payload: {
        logical_table_id: 'table:loads',
        domain: 'drone_bearing_design_verified',
        chunk_index: 0,
        chunk_count: 2,
      },
    },
  ]);

  assert.equal(tables.length, 1);
  assert.equal(tables[0].id, 'table:loads');
  assert.equal(tables[0].domain, 'drone_bearing_design_verified');
});

test('document knowledge aggregation is deterministic and preserves every chunk lineage', () => {
  const tables = hooks.mergeKnowledgeTableReferences([
    {
      id: 'table:loads:chunk:0002',
      payload: {
        logical_table_id: 'table:loads',
        domain: 'drone_bearing_design_verified',
        chunk_index: 2,
        chunk_count: 3,
        chunk_row_offset: 3,
        chunk_row_count: 1,
        row_count: 4,
        projected_row_count: 4,
        rows: [{ source_row: 3 }],
      },
    },
    {
      id: 'table:loads',
      payload: {
        logical_table_id: 'table:loads',
        domain: 'drone_bearing_design_verified',
        chunk_index: 0,
        chunk_count: 3,
        chunk_row_offset: 0,
        chunk_row_count: 2,
        row_count: 4,
        projected_row_count: 4,
        rows: [{ source_row: 0 }, { source_row: 1 }],
      },
    },
    {
      id: 'table:loads:chunk:0001',
      payload: {
        logical_table_id: 'table:loads',
        domain: 'drone_bearing_design_verified',
        chunk_index: 1,
        chunk_count: 3,
        chunk_row_offset: 2,
        chunk_row_count: 1,
        row_count: 4,
        projected_row_count: 4,
        rows: [{ source_row: 2 }],
      },
    },
  ]);

  assert.equal(tables.length, 1);
  assert.equal(tables[0].id, 'table:loads');
  assert.deepEqual(tables[0].rows.map((row) => row.source_row), [0, 1, 2, 3]);
  assert.equal(tables[0].rows_complete, true);
  assert.equal(tables[0].chunk_status, 'complete');
  assert.deepEqual(tables[0].chunk_ids, [
    'table:loads',
    'table:loads:chunk:0001',
    'table:loads:chunk:0002',
  ]);
  assert.deepEqual(tables[0].chunk_lineage.map((entry) => [entry.chunk_index, entry.row_offset]), [
    [0, 0],
    [1, 2],
    [2, 3],
  ]);
  assert.deepEqual(tables[0].payload.chunk_lineage, tables[0].chunk_lineage);
});

test('document knowledge aggregation marks duplicate and missing chunk indices incomplete', () => {
  const [table] = hooks.mergeKnowledgeTableReferences([
    {
      id: 'table:loads',
      logical_table_id: 'table:loads',
      chunk_index: 0,
      chunk_count: 3,
      rows: [{ source_row: 0 }],
    },
    {
      id: 'table:loads:chunk:0001a',
      logical_table_id: 'table:loads',
      chunk_index: 1,
      chunk_count: 3,
      rows: [{ source_row: 1 }],
    },
    {
      id: 'table:loads:chunk:0001b',
      logical_table_id: 'table:loads',
      chunk_index: 1,
      chunk_count: 3,
      rows: [{ source_row: 1.5 }],
    },
  ]);

  assert.equal(table.rows_complete, false);
  assert.equal(table.chunk_status, 'incomplete');
  assert.ok(table.chunk_validation_errors.includes('duplicate_chunk_index'));
  assert.ok(table.chunk_validation_errors.includes('missing_chunk_index:2'));
  assert.equal(table.payload.rows_complete, false);
});

test('document knowledge aggregation marks inconsistent counts and offsets incomplete', () => {
  const [table] = hooks.mergeKnowledgeTableReferences([
    {
      id: 'table:loads',
      payload: {
        logical_table_id: 'table:loads',
        chunk_index: 0,
        chunk_count: 2,
        chunk_row_offset: 0,
        chunk_row_count: 1,
        row_count: 3,
        projected_row_count: 3,
        rows: [{ source_row: 0 }],
      },
    },
    {
      id: 'table:loads:chunk:0001',
      payload: {
        logical_table_id: 'table:loads',
        chunk_index: 1,
        chunk_count: 4,
        chunk_row_offset: 3,
        chunk_row_count: 1,
        row_count: 4,
        projected_row_count: 4,
        rows: [{ source_row: 1 }],
      },
    },
  ]);

  assert.equal(table.rows_complete, false);
  assert.ok(table.chunk_validation_errors.includes('inconsistent_chunk_count'));
  assert.ok(table.chunk_validation_errors.includes('inconsistent_chunk_offset'));
  assert.ok(table.chunk_validation_errors.includes('inconsistent_total_rows'));
  assert.ok(table.chunk_validation_errors.includes('inconsistent_projected_row_count'));
});

test('table-only Knowledge is selectable as data context, never as a procedural skill', () => {
  const state = {
    knowledgeItems: [],
    knowledgeRunbooks: [],
    knowledgeTables: [hooks.normalizeKnowledgeRecord({
      id: 'table:loads',
      kind: 'dataframe',
      title: 'Measured load points',
      domain: 'drone_bearing_design',
      rows: [{ source_row: 0 }],
    })],
  };

  const candidates = hooks.knowledgeCandidates(state);
  assert.deepEqual(candidates.map((item) => item.id), ['table:loads']);
  assert.equal(candidates[0].selection_type, 'table');
  assert.equal(candidates[0].is_procedural_skill, false);

  const context = hooks.resolveKnowledgeContext(state, 'table:loads', 'load points');
  assert.equal(context.id, 'table:loads');
  assert.equal(context.kind, 'dataframe');
  assert.equal(context.selection_type, 'table');
  assert.equal(context.is_procedural_skill, false);
  assert.deepEqual(context.table_ids, ['table:loads']);
  assert.match(hooks.knowledgeContextInstruction(context), /kein prozeduraler Skill/);
});

test('only superseded draft blobs are reclaimed, never the original or current blob', () => {
  // Successive autosaves: the previous draft blob is collectable.
  assert.equal(hooks.isReclaimableDraftBlob('v1_draft_100', 'v1_draft_200'), true);
  // The original imported source blob must be preserved on first edit.
  assert.equal(hooks.isReclaimableDraftBlob('v1_blob', 'v1_draft_200'), false);
  // Never delete the blob the version still points at.
  assert.equal(hooks.isReclaimableDraftBlob('v1_draft_200', 'v1_draft_200'), false);
  // No previous blob -> nothing to reclaim.
  assert.equal(hooks.isReclaimableDraftBlob('', 'v1_draft_200'), false);
});

test('typed runtime settings select CTOX Documents by default and preserve explicit legacy rollback', () => {
  assert.equal(hooks.officeEngineFromSettings({ office: { documents_engine: 'ctox_office' } }), 'ctox_documents');
  assert.equal(hooks.officeEngineFromSettings({ office: { documents_engine: 'ctox_documents' } }), 'ctox_documents');
  assert.equal(hooks.officeEngineFromSettings({ office: { documents_engine: 'legacy' } }), 'legacy');
  assert.equal(hooks.officeEngineFromSettings({}), 'ctox_documents');
});

test('CTOX Documents permissions expose comments and review only with full write access', () => {
  const writable = hooks.ctoxDocumentsPermissions({
    permissions: { canWriteCollection: () => true },
  });
  assert.deepEqual(writable, { read: true, write: true, export: true, comment: true, review: true });

  const readOnly = hooks.ctoxDocumentsPermissions({
    permissions: { canWriteCollection: (name) => name !== 'document_versions' },
  });
  assert.deepEqual(readOnly, { read: true, write: false, export: true, comment: false, review: false });
});

test('visibleDocuments filters active normalized rows by status, tag, search, and sort', () => {
  const state = {
    searchQuery: 'vertrag',
    statusFilter: 'Draft',
    tagFilter: 'kunde-a',
    sortBy: 'title_asc',
    documents: [
      hooks.normalizeDocumentRecord({
        id: 'doc_2',
        title: 'Zeta Vertrag',
        filename: 'zeta.md',
        status: 'Draft',
        tags: ['kunde-a'],
        updated_at_ms: 20,
      }),
      hooks.normalizeDocumentRecord({
        id: 'doc_1',
        title: 'Alpha Vertrag',
        filename: 'alpha.docx',
        status: 'Draft',
        tags: ['kunde-a'],
        updated_at_ms: 10,
      }),
      hooks.normalizeDocumentRecord({
        id: 'doc_3',
        title: 'Alpha Angebot',
        filename: 'angebot.docx',
        status: 'Final',
        tags: ['kunde-a'],
        updated_at_ms: 30,
      }),
    ],
  };

  assert.deepEqual(hooks.visibleDocuments(state).map((record) => record.id), ['doc_1', 'doc_2']);
});

test('mail merge and series letter records remain one list entry while normal documents stay individual', () => {
  const grouped = hooks.groupDocumentRecords([
    hooks.normalizeDocumentRecord({
      id: 'merge_1',
      title: 'MURRELEKTRONIK 2022 - Welle 3',
      filename: 'murrel-serienbrief.docx',
      document_type: 'mail_merge',
      status: 'Created',
      mail_merge: { recipient_count: 19, requested_count: 19 },
      provenance: { app_id: 'crm', source: 'campaign-mail-merge' },
    }),
    hooks.normalizeDocumentRecord({
      id: 'letter_1',
      title: 'Einzelbrief',
      filename: 'einzelbrief.docx',
      document_type: 'word_document',
      status: 'Draft',
    }),
    hooks.normalizeDocumentRecord({
      id: 'note_1',
      title: 'Notiz',
      filename: 'notiz.md',
      document_type: 'markdown_document',
      status: 'Draft',
    }),
  ]);

  assert.equal(grouped.length, 3);
  const merge = grouped.find((entry) => entry.id === 'merge_1');
  assert.equal(merge.is_mail_merge, true);
  assert.equal(merge.document_type, 'mail_merge');
  assert.equal(merge.recipient_count, 19);
  assert.deepEqual(grouped.filter((entry) => !entry.is_mail_merge).map((entry) => entry.id).sort(), ['letter_1', 'note_1']);
});

test('typed campaign mail merges group export revisions by campaign and template identity', () => {
  const records = [100, 200, 300].map((updatedAt, index) => hooks.normalizeDocumentRecord({
    id: `merge_revision_${index + 1}`,
    title: 'MURRELEKTRONIK 2022 - Welle 3 - Brief Erstkontakt - Serienbrief',
    filename: `murrel-serienbrief-${index + 1}.docx`,
    document_type: 'mail_merge',
    status: 'Created',
    idempotency_key: `crm-app:campaign-mail-merge:revision-${index + 1}`,
    mail_merge: { recipient_count: index === 0 ? 18 : 19, requested_count: 19 },
    template_ref: { template_id: 'brief-erstkontakt', version: 4 },
    provenance: {
      app_id: 'crm',
      source: 'campaign-mail-merge',
      selection_id: 4711,
    },
    updated_at_ms: updatedAt,
  }));

  const grouped = hooks.groupDocumentRecords(records);

  assert.equal(grouped.length, 1);
  assert.equal(grouped[0].is_mail_merge, true);
  assert.equal(grouped[0].recipient_count, 19);
  assert.equal(grouped[0].id, 'merge_revision_3');
  assert.deepEqual(grouped[0].record_ids, [
    'merge_revision_1',
    'merge_revision_2',
    'merge_revision_3',
  ]);
  assert.equal(
    grouped[0].bundle_key,
    'mail_merge:crm:campaign-mail-merge:4711:brief-erstkontakt:4',
  );
});

test('explicit mail merge run ids keep separate runs apart while grouping each retry', () => {
  const records = [
    ['run-a-v1', 'run-a', 100],
    ['run-a-v2', 'run-a', 200],
    ['run-b-v1', 'run-b', 300],
  ].map(([id, bundleId, updatedAt]) => hooks.normalizeDocumentRecord({
    id,
    title: 'Sommerkampagne - Serienbrief',
    filename: `${id}.docx`,
    document_type: 'mail_merge',
    status: 'Created',
    mail_merge: { bundle_id: bundleId, recipient_count: 19 },
    provenance: { app_id: 'crm', source: 'campaign-mail-merge' },
    updated_at_ms: updatedAt,
  }));

  const grouped = hooks.groupDocumentRecords(records);

  assert.equal(grouped.length, 2);
  assert.deepEqual(
    grouped.map(({ bundle_key: key }) => key).sort(),
    ['mail_merge:run-a', 'mail_merge:run-b'],
  );
  assert.deepEqual(
    grouped.find(({ bundle_key: key }) => key === 'mail_merge:run-a').record_ids,
    ['run-a-v1', 'run-a-v2'],
  );
});

test('campaign export run provenance is kept as revision metadata, not a separate sidebar bundle', () => {
  const records = [
    ['run-a-v1', 'run-a', 100],
    ['run-a-v2', 'run-a', 200],
    ['run-b-v1', 'run-b', 300],
  ].map(([id, exportRunId, updatedAt]) => hooks.normalizeDocumentRecord({
    id,
    title: 'Sommerkampagne - Serienbrief',
    filename: `${id}.docx`,
    document_type: 'mail_merge',
    status: 'Created',
    mail_merge: { recipient_count: 19 },
    template_ref: { template_id: 'brief-erstkontakt', version: 4 },
    provenance: {
      app_id: 'crm',
      source: 'campaign-mail-merge',
      selection_id: 4711,
      export_run_id: exportRunId,
    },
    updated_at_ms: updatedAt,
  }));

  const grouped = hooks.groupDocumentRecords(records);

  assert.equal(grouped.length, 1);
  assert.equal(
    grouped[0].bundle_key,
    'mail_merge:crm:campaign-mail-merge:4711:brief-erstkontakt:4',
  );
  assert.deepEqual(grouped[0].record_ids, ['run-a-v1', 'run-a-v2', 'run-b-v1']);
});

test('legacy per-recipient campaign documents group by stable provenance and template identity', () => {
  const records = ['Bjarne Schäfer', 'Daniel Floris', 'Markus H. Niedermayer'].map((recipient, index) => (
    hooks.normalizeDocumentRecord({
      id: `legacy_${index + 1}`,
      title: `MURRELEKTRONIK 2022 - Welle 3 - GT - 12.10.2022 - ${recipient} - Brief Erstkontakt`,
      filename: `recipient-${index + 1}.docx`,
      document_type: 'word_document',
      status: 'Draft',
      current_version_id: `legacy_${index + 1}_v1`,
      template_ref: { template_id: 'brief-erstkontakt', version: 4 },
      provenance: {
        app_id: 'crm',
        source: 'campaign-mail-merge',
        selection_id: 4711,
        selectionmember_id: index + 100,
      },
    })
  ));
  const grouped = hooks.groupDocumentRecords(records);

  assert.equal(grouped.length, 1);
  assert.equal(grouped[0].is_mail_merge, true);
  assert.equal(grouped[0].document_type, 'mail_merge');
  assert.equal(grouped[0].recipient_count, 3);
  assert.equal(grouped[0].title, 'MURRELEKTRONIK 2022 - Welle 3 - GT - 12.10.2022');
  assert.deepEqual(grouped[0].record_ids, ['legacy_1', 'legacy_2', 'legacy_3']);
});

test('legacy mail merge groups count and navigate duplicate exports once per CRM recipient', async () => {
  const documents = [100, 200, 300].map((updatedAt, index) => hooks.normalizeDocumentRecord({
    id: `duplicate_${index + 1}`,
    title: `Sommerkampagne - Ada Lovelace - Brief Erstkontakt ${index + 1}`,
    filename: `duplicate-${index + 1}.docx`,
    document_type: 'word_document',
    status: 'Draft',
    current_version_id: `duplicate_${index + 1}_v1`,
    template_ref: { template_id: 'brief-erstkontakt', version: 1 },
    provenance: {
      app_id: 'crm-app',
      source: 'campaign-mail-merge',
      selection_id: 1734,
      selectionmember_id: 42,
      recipient_label: 'Ada Lovelace',
    },
    updated_at_ms: updatedAt,
  }));
  const grouped = hooks.groupDocumentRecords(documents);
  assert.equal(grouped.length, 1);
  assert.equal(grouped[0].recipient_count, 1);
  assert.deepEqual(grouped[0].record_ids, ['duplicate_1', 'duplicate_2', 'duplicate_3']);

  const state = {
    ctx: { db: {} },
    documents,
    selectedId: 'duplicate_3',
    selectedVersion: { id: 'duplicate_3_v1' },
  };
  const navigation = await hooks.refreshMailMergeNavigation(state);
  assert.equal(navigation.entries.length, 1);
  assert.equal(navigation.entries[0].documentId, 'duplicate_3');
  assert.equal(navigation.entries[0].label, 'Ada Lovelace');
});

test('mail merge navigator loads recipient versions and resolves recipient search', async () => {
  const versions = [
    {
      id: 'merge_1_v2',
      document_id: 'merge_1',
      version: 2,
      source_kind: 'mail_merge_recipient',
      mail_merge_recipient: { id: 'person_2', label: 'Daniel Floris', index: 1, total: 3 },
    },
    {
      id: 'merge_1_v1',
      document_id: 'merge_1',
      version: 1,
      source_kind: 'mail_merge_recipient',
      mail_merge_recipient: { id: 'person_1', label: 'Bjarne Schäfer', index: 0, total: 3 },
    },
    {
      id: 'merge_1_v3',
      document_id: 'merge_1',
      version: 3,
      source_kind: 'mail_merge_recipient',
      mail_merge_recipient: { id: 'person_3', label: 'Markus H. Niedermayer', index: 2, total: 3 },
    },
  ];
  const state = {
    ctx: {
      db: {
        collection(name) {
          if (name !== 'document_versions') return null;
          return { find: () => ({ exec: async () => versions }) };
        },
      },
    },
    documents: [hooks.normalizeDocumentRecord({
      id: 'merge_1',
      title: 'MURRELEKTRONIK 2022',
      filename: 'murrel.docx',
      document_type: 'mail_merge',
      current_version_id: 'merge_1_v1',
      mail_merge: { recipient_count: 3 },
    })],
    selectedId: 'merge_1',
    selectedVersion: { id: 'merge_1_v2' },
  };

  const navigation = await hooks.refreshMailMergeNavigation(state);
  assert.deepEqual(navigation.entries.map(({ label }) => label), [
    'Bjarne Schäfer',
    'Daniel Floris',
    'Markus H. Niedermayer',
  ]);
  assert.equal(navigation.activeIndex, 1);
  assert.equal(hooks.findMailMergeRecipientIndex(navigation.entries, 'bjarne'), 0);
  assert.equal(hooks.findMailMergeRecipientIndex(navigation.entries, 'Niedermayer'), 2);
  assert.equal(hooks.findMailMergeRecipientIndex(navigation.entries, 'nicht vorhanden'), -1);
});

test('mail merge recipient selection keeps the rendered navigator during async refresh', () => {
  const rendered = {
    groupId: 'campaign-268',
    activeIndex: 0,
    entries: [
      { documentId: 'merge_1', versionId: 'merge_1_v1', label: 'Peter Mehrle' },
      { documentId: 'merge_1', versionId: 'merge_1_v2', label: 'Isabell Dapper' },
    ],
  };
  assert.deepEqual(
    hooks.resolveMailMergeRecipientSelection(null, rendered, 1),
    {
      navigation: rendered,
      index: 1,
      entry: rendered.entries[1],
    },
  );
  assert.equal(hooks.resolveMailMergeRecipientSelection(null, null, 1), null);
});

test('mail merge navigator uses the selected typed export when grouped revisions exist', async () => {
  const requestedDocumentIds = [];
  const versions = {
    merge_old: [{
      id: 'merge_old_v1',
      document_id: 'merge_old',
      source_kind: 'mail_merge_recipient',
      mail_merge_recipient: { id: 'old-person', label: 'Alte Revision', index: 0, total: 1 },
    }],
    merge_current: [
      {
        id: 'merge_current_v1',
        document_id: 'merge_current',
        source_kind: 'mail_merge_recipient',
        mail_merge_recipient: { id: 'person-1', label: 'Peter Mehrle', index: 0, total: 2 },
      },
      {
        id: 'merge_current_v2',
        document_id: 'merge_current',
        source_kind: 'mail_merge_recipient',
        mail_merge_recipient: { id: 'person-2', label: 'Isabell Dapper', index: 1, total: 2 },
      },
    ],
  };
  const common = {
    title: 'MURRELEKTRONIK 2022 - Brief Erstkontakt - Serienbrief',
    filename: 'murrel-serienbrief.docx',
    document_type: 'mail_merge',
    mail_merge: { recipient_count: 2 },
    template_ref: { template_id: 'letter-first-contact', version: 1 },
    provenance: { app_id: 'crm-app', source: 'campaign-mail-merge', selection_id: 268 },
  };
  const state = {
    ctx: {
      db: {
        collection(name) {
          if (name !== 'document_versions') return null;
          return {
            find({ selector }) {
              requestedDocumentIds.push(selector.document_id);
              return { exec: async () => versions[selector.document_id] || [] };
            },
          };
        },
      },
    },
    documents: [
      hooks.normalizeDocumentRecord({ ...common, id: 'merge_old', current_version_id: 'merge_old_v1', updated_at_ms: 100 }),
      hooks.normalizeDocumentRecord({ ...common, id: 'merge_current', current_version_id: 'merge_current_v1', updated_at_ms: 200 }),
    ],
    selectedId: 'merge_current',
    selectedVersion: { id: 'merge_current_v1' },
  };

  const navigation = await hooks.refreshMailMergeNavigation(state);
  assert.deepEqual(requestedDocumentIds, ['merge_current']);
  assert.deepEqual(navigation.entries.map(({ label }) => label), ['Peter Mehrle', 'Isabell Dapper']);
  assert.equal(navigation.activeIndex, 0);
});

test('document management searches grouped recipient text and filters type, status, and provenance source', () => {
  const documents = [
    hooks.normalizeDocumentRecord({
      id: 'merge_1',
      title: 'Sommerkampagne',
      filename: 'sommer.docx',
      document_type: 'mail_merge',
      status: 'Created',
      index_text: 'Bjarne Schäfer Daniel Floris',
      provenance: { app_id: 'crm', source: 'campaign-mail-merge' },
      mail_merge: { recipient_count: 2 },
      updated_at_ms: 30,
    }),
    hooks.normalizeDocumentRecord({
      id: 'word_1',
      title: 'Alpha Angebot',
      filename: 'alpha.docx',
      document_type: 'word_document',
      status: 'Final',
      provenance: { app_id: 'documents', source: 'manual-report' },
      updated_at_ms: 10,
    }),
    hooks.normalizeDocumentRecord({
      id: 'note_1',
      title: 'Zeta Notiz',
      filename: 'zeta.md',
      document_type: 'markdown_document',
      status: 'Draft',
      updated_at_ms: 20,
    }),
  ];
  const state = {
    documents,
    searchQuery: 'schäfer',
    typeFilter: 'mail_merge',
    statusFilter: 'Created',
    appFilter: 'crm',
    sourceFilter: 'campaign-mail-merge',
    tagFilter: 'all',
    sortBy: 'updated_desc',
  };

  assert.deepEqual(hooks.visibleDocuments(state).map(({ id }) => id), ['merge_1']);
  assert.equal(hooks.documentFilterCount(state), 4);

  Object.assign(state, {
    searchQuery: '',
    typeFilter: 'all',
    statusFilter: 'all',
    appFilter: 'all',
    sourceFilter: 'all',
    sortBy: 'updated_asc',
  });
  assert.deepEqual(hooks.visibleDocuments(state).map(({ id }) => id), ['word_1', 'note_1', 'merge_1']);
  state.sortBy = 'title_asc';
  assert.deepEqual(hooks.visibleDocuments(state).map(({ id }) => id), ['word_1', 'merge_1', 'note_1']);
  state.sortBy = 'creator_app';
  assert.deepEqual(hooks.visibleDocuments(state).map(({ id }) => id), ['merge_1', 'word_1', 'note_1']);
});

test('mail merge and series letter records use the DOCX render, save, and export path', () => {
  assert.equal(hooks.isDocxDocumentRecord({ document_type: 'word_document' }), true);
  assert.equal(hooks.isDocxDocumentRecord({ document_type: 'mail_merge' }), true);
  assert.equal(hooks.isDocxDocumentRecord({ document_type: 'series_letter' }), true);
  assert.equal(hooks.isDocxDocumentRecord({ document_type: 'markdown_document', filename: 'notes.md' }), false);
});

test('Documents UI is a two-pane file manager and Word editor without a right actions column', async () => {
  const [html, css, source, moduleJson, deMessages] = await Promise.all([
    readFile(new URL('./index.html', import.meta.url), 'utf8'),
    readFile(new URL('./index.css', import.meta.url), 'utf8'),
    readFile(new URL('./index.js', import.meta.url), 'utf8'),
    readFile(new URL('./module.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('./locales/de.json', import.meta.url), 'utf8').then(JSON.parse),
  ]);

  assert.match(html, /data-resize-frame/);
  assert.match(html, /class="ctox-column-resizer documents-library-resizer"/);
  assert.match(html, /data-resizer-var="--shell-col-left"/);
  assert.doesNotMatch(html, /documents-actions-resizer/);
  assert.doesNotMatch(html, /data-documents-actions-drawer/);
  assert.match(css, /\.documents-library-resizer[\s\S]*cursor:\s*col-resize/);
  assert.match(css, /\.documents-workbench\s*\{[\s\S]*container-type:\s*inline-size/);
  assert.match(css, /@container documents-workbench \(max-width: 560px\)[\s\S]*\.documents-recipient-navigator[\s\S]*minmax\(0, 1fr\)/);
  assert.match(css, /\.documents-strip-leading\s*\{[\s\S]*overflow:\s*hidden/);
  assert.match(source, /new ResizeObserver/);
  assert.match(source, /root\.classList\.toggle\('is-compact', width <= 768\)/);
  assert.match(html, /data-pg-search/);
  assert.match(html, /data-pg-view-cycle="list,cards"/);
  assert.match(html, /data-pg-footer/);
  assert.match(source, /autoWirePaneGrammar/);
  assert.match(source, /shellRightPane\.hidden = true/);
  assert.match(css, /\.documents-module\.is-compact \.documents-library-resizer[\s\S]*display:\s*none/);
  assert.match(css, /SuperDoc 1\.32\.0 ships 32px toolbar controls/);
  assert.match(css, /@media \(pointer: coarse\)[\s\S]*\.documents-superdoc-toolbar \.superdoc-toolbar[\s\S]*--sd-ui-toolbar-height:\s*44px/);
  assert.match(css, /@media \(pointer: coarse\)[\s\S]*\.documents-superdoc-toolbar \.toolbar-item[\s\S]*min-width:\s*44px/);
  assert.doesNotMatch(source, /data-documents-actions-toggle/);
  assert.match(source, /revisionedModuleAssetUrl\('\.\/index\.html'\)/);
  assert.match(source, /revisionedModuleAssetUrl\('\.\/index\.css'\)/);
  assert.equal(moduleJson.title, 'Dokumente');
  assert.equal(moduleJson.layout.shell_contract, 'v2');
  assert.equal(deMessages.documentsTitle, 'Dokumente');
});

test('Documents context menu stays inside the module shell', async () => {
  const [css, source] = await Promise.all([
    readFile(new URL('./index.css', import.meta.url), 'utf8'),
    readFile(new URL('./index.js', import.meta.url), 'utf8'),
  ]);
  assert.match(source, /const root = state\.ctx\.host\.querySelector\('\[data-documents-module\]'\) \|\| state\.ctx\.host/);
  assert.match(source, /root\.append\(menu\)/);
  assert.match(source, /const rootRect = state\.contextMenu\.parentElement\.getBoundingClientRect\(\)/);
  assert.match(css, /\.shell-window\[data-shell-contract="v2"\] \.documents-context-menu\s*\{[\s\S]*position:\s*absolute/);
});

test('new document validation requires title, runbook, and prompt', () => {
  assert.equal(hooks.validateNewDocumentInput({ title: 'Report', runbookId: 'research.report.auto', prompt: '' }).valid, false);
  assert.equal(hooks.validateNewDocumentInput({ title: 'Report', runbookId: '', prompt: 'Analyse' }).valid, false);
  assert.equal(hooks.validateNewDocumentInput({ title: 'Report', runbookId: 'research.report.auto', prompt: 'Analyse' }).valid, true);
});

test('visible new-document controls create a blank Word document directly', async () => {
  const [source, html] = await Promise.all([
    readFile(new URL('./index.js', import.meta.url), 'utf8'),
    readFile(new URL('./index.html', import.meta.url), 'utf8'),
  ]);
  assert.match(source, /data-documents-empty-new[^\n]+createBlankWordDocument/);
  assert.match(source, /data-documents-new-markdown[^\n]+[\s\S]{0,160}createBlankWordDocument/);
  assert.match(source, /sourceKind:\s*'created_blank'/);
  assert.match(html, /data-documents-new-markdown[^>]+Leeres Word-Dokument erstellen/);
});

test('knowledge selection supports explicit skills and automatic topic matching', () => {
  const state = {
    knowledgeItems: [
      hooks.normalizeKnowledgeRecord({ id: 'skill:bearings', kind: 'skill', title: 'Drone Bearing Loads', summary: 'Propeller torque and bearing force', payload: { domain: 'drone_bearing_design' }, updated_at_ms: 20 }),
      hooks.normalizeKnowledgeRecord({ id: 'skill:markets', kind: 'skill', title: 'Market Research', summary: 'Vendors and pricing', payload: { domain: 'market' }, updated_at_ms: 30 }),
    ],
    knowledgeRunbooks: [{ id: 'runbook:bearing-report', kind: 'runbook', payload: { domain: 'drone_bearing_design' } }],
    knowledgeTables: [{ id: 'table:bearing-loads', kind: 'dataframe', payload: { domain: 'drone_bearing_design' } }],
  };

  const automatic = hooks.resolveKnowledgeContext(state, 'auto', 'Analyse propeller torque for drone bearings');
  assert.equal(automatic.id, 'skill:bearings');
  assert.equal(automatic.selection_mode, 'auto');
  assert.deepEqual(automatic.table_ids, ['table:bearing-loads']);
  assert.deepEqual(automatic.linked_runbook_ids, ['runbook:bearing-report']);

  const manual = hooks.resolveKnowledgeContext(state, 'skill:markets', 'bearing loads');
  assert.equal(manual.id, 'skill:markets');
  assert.equal(manual.selection_mode, 'manual');
});

test('documents become stale when their linked knowledge item is newer', () => {
  const record = { linked_records: [{ type: 'knowledge', id: 'skill:bearings', title: 'Bearing Loads', updated_at_ms: 100 }] };
  const state = { knowledgeItems: [{ id: 'skill:bearings', updated_at_ms: 101 }] };
  assert.equal(hooks.documentKnowledgeLink(record).id, 'skill:bearings');
  assert.equal(hooks.isDocumentKnowledgeStale(state, record), true);
  state.knowledgeItems[0].updated_at_ms = 100;
  assert.equal(hooks.isDocumentKnowledgeStale(state, record), false);
});

test('import validation requires a supported file', () => {
  assert.equal(hooks.validateImportInput({ file: null }).valid, false);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'notes.md', { type: 'text/plain' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'notes.txt', { type: 'text/plain' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'image.png', { type: 'image/png' }) }).valid, false);
});

test('file-open deduplication reuses the imported document with the same source hash', () => {
  const records = [
    { id: 'doc_other', source_sha256: 'aaaa' },
    { id: 'doc_report', source_sha256: 'CAFE' },
  ];
  assert.equal(hooks.documentBySourceSha(records, 'cafe')?.id, 'doc_report');
  assert.equal(hooks.documentBySourceSha(records, 'missing'), null);
});

test('document blob chunks are persisted with one bulk write', async () => {
  const bulkWrites = [];
  let acknowledged = false;
  const blobChunks = {
    bulkUpsert: async (docs) => {
      bulkWrites.push(docs);
      return docs.map(row => ({ toJSON: () => ({ ...row, _meta: { lwt: Date.now() } }) }));
    },
    insert: async () => { throw new Error('document_blob_chunks insert must not run per chunk'); },
  };
  const ctx = {
    sync: { async leaseCollection() { return { bridge: { state: {
      async waitForOpenPeerId() { return 'native'; },
      async pushDocumentsToPeer(_peer, rows) { assert.equal(rows.length, bulkWrites[0].length); acknowledged = true; },
    } }, async release() {} }; } },
    db: {
      collection(name) {
        if (name === 'document_blob_chunks') return blobChunks;
        if (name === 'documents' || name === 'document_versions') return {};
        return null;
      },
    },
  };

  const bytes = new Uint8Array(260 * 1024);
  bytes.fill(66);
  await hooks.saveBlobChunks(ctx, {
    blobId: 'blob_bulk',
    documentId: 'doc_bulk',
    versionId: 'version_bulk',
    mimeType: 'application/octet-stream',
    bytes,
  });

  assert.equal(bulkWrites.length, 1, 'blob chunks are written through one bulkUpsert call');
  assert.ok(bulkWrites[0].length > 1, 'test payload spans multiple chunk documents');
  assert.equal(acknowledged, true, 'source bytes are acknowledged before exposing references');
});
