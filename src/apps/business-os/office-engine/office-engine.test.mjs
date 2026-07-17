import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { MessageChannel } from 'node:worker_threads';

import { createBusinessOsOfficeBridge, __officeBridgeTestHooks } from './src/business-os-bridge.mjs';
import { OfficeRpcError, OfficeRpcPeer } from './src/rpc.mjs';
import { __spreadsheetRuntimeTestHooks } from './src/legacy-runtime/spreadsheet.mjs';
import { __ctoxForkTestHooks } from './src/runtime/ctox-fork-core.mjs';

test('message-channel RPC supports calls, events, typed failures, and close', async () => {
  const channel = new MessageChannel();
  const server = new OfficeRpcPeer(channel.port1, {
    add: ({ left, right }) => left + right,
    fail: () => { const error = new Error('denied'); error.code = 'permission_denied'; throw error; },
  });
  const client = new OfficeRpcPeer(channel.port2);
  assert.equal(await client.call('add', { left: 20, right: 22 }), 42);
  await assert.rejects(client.call('fail'), (error) => error instanceof OfficeRpcError && error.code === 'permission_denied');
  await assert.rejects(client.call('missing'), (error) => error.code === 'rpc_method_not_found');
  const event = new Promise((resolve) => client.on('dirty', resolve));
  server.emit('dirty', { value: true });
  assert.deepEqual(await event, { value: true });
  client.close();
  server.close();
});

test('blob helper rejects incomplete chunks and rebuilds valid bytes', async () => {
  const bytes = new TextEncoder().encode('office-data');
  const rows = [];
  const chunks = {
    async bulkUpsert(next) { rows.push(...next); },
    find({ selector }) {
      return { async exec() { return rows.filter((row) => row.blob_id === selector.blob_id); } };
    },
  };
  await __officeBridgeTestHooks.saveBlob(chunks, {
    recordIdField: 'document_id',
    mime: 'application/test',
  }, { blobId: 'blob_1', recordId: 'doc_1', versionId: 'v1', bytes });
  assert.deepEqual(await __officeBridgeTestHooks.loadBlob(chunks, 'blob_1'), bytes);
  rows[0].total = 2;
  await assert.rejects(__officeBridgeTestHooks.loadBlob(chunks, 'blob_1'), (error) => error.code === 'blob_incomplete');
});

test('blob helper rejects a hash mismatch and recovers after replicated chunks are repaired', async () => {
  const good = new TextEncoder().encode('office-data');
  const bad = new TextEncoder().encode('office-datx');
  const rows = [{ id: 'blob_1_0000', blob_id: 'blob_1', idx: 0, total: 2, data: Buffer.from(good.subarray(0, 6)).toString('base64') }];
  const chunks = {
    find({ selector }) { return { async exec() { return rows.filter((row) => row.blob_id === selector.blob_id); } }; },
  };
  await assert.rejects(__officeBridgeTestHooks.loadBlob(chunks, 'blob_1'), (error) => error.code === 'blob_incomplete');
  rows.push({ id: 'blob_1_0001', blob_id: 'blob_1', idx: 1, total: 2, data: Buffer.from(bad.subarray(6)).toString('base64') });
  const expected = '2768ca93550a46fe3442ce4d59d50c272b9f8f693e9a3b8c1ab134db5180d7a1';
  await assert.rejects(__officeBridgeTestHooks.loadBlob(chunks, 'blob_1', expected), (error) => error.code === 'blob_hash_mismatch');
  rows[1].data = Buffer.from(good.subarray(6)).toString('base64');
  assert.deepEqual(await __officeBridgeTestHooks.loadBlob(chunks, 'blob_1', expected), good);
});

test('blob helper strictly revalidates an empty cached selector before failing', async () => {
  const bytes = new TextEncoder().encode('replicated-office-data');
  const queries = [];
  const chunks = {
    find(query) {
      queries.push(query);
      return {
        async exec() {
          if (!query.requireRevision) return [];
          return [{
            id: 'blob_1_0000',
            blob_id: 'blob_1',
            idx: 0,
            total: 1,
            data: Buffer.from(bytes).toString('base64'),
          }];
        },
      };
    },
  };
  assert.deepEqual(await __officeBridgeTestHooks.loadBlob(chunks, 'blob_1'), bytes);
  assert.equal(queries.length, 2);
  assert.match(queries[1].requireRevision, /^blob:blob_1:/);
});

test('blob helper recovers missing structured chunks through the WebRTC file stream', async () => {
  const bytes = new TextEncoder().encode('native-streamed-editor-blob');
  const firstChunk = bytes.subarray(0, 8);
  const secondChunk = bytes.subarray(8);
  let queries = 0;
  const chunks = {
    find() { return { async exec() { queries += 1; return []; } }; },
  };
  const fileIds = [];
  const fileLoader = {
    async fetchFile(fileId) {
      fileIds.push(fileId);
      return [
        {
          sequence: 1,
          bytesBase64: Buffer.from(secondChunk).toString('base64'),
          hash: createHash('sha256').update(secondChunk).digest('hex'),
        },
        {
          sequence: 0,
          bytesBase64: Buffer.from(firstChunk).toString('base64'),
          hash: createHash('sha256').update(firstChunk).digest('hex'),
        },
      ];
    },
  };
  assert.deepEqual(await __officeBridgeTestHooks.loadBlob(chunks, 'editor_blob_1', '', fileLoader), bytes);
  assert.equal(queries, 2);
  assert.deepEqual(fileIds, ['editor_blob_1']);
});

test('blob helper rejects a streamed chunk that fails transport verification', async () => {
  const chunks = { find() { return { async exec() { return []; } }; } };
  const fileLoader = {
    async fetchFile() {
      return [{
        sequence: 0,
        bytesBase64: Buffer.from('altered').toString('base64'),
        hash: createHash('sha256').update('original').digest('hex'),
      }];
    },
  };
  await assert.rejects(
    __officeBridgeTestHooks.loadBlob(chunks, 'streamed_corrupt', '', fileLoader),
    (error) => error.code === 'blob_chunk_hash_mismatch',
  );
});

test('blob helper refreshes stale local chunks before using the file stream', async () => {
  const staleBytes = new TextEncoder().encode('stale-local-blob');
  const currentBytes = new TextEncoder().encode('current-local-blob');
  const expectedSha256 = createHash('sha256').update(currentBytes).digest('hex');
  const queries = [];
  const chunks = {
    find(query) {
      queries.push(query);
      return {
        async exec() {
          const bytes = query.requireRevision ? currentBytes : staleBytes;
          return [{
            id: 'blob_refresh_0000',
            blob_id: 'blob_refresh',
            idx: 0,
            total: 1,
            data: Buffer.from(bytes).toString('base64'),
          }];
        },
      };
    },
  };
  let streams = 0;
  const fileLoader = {
    async fetchFile() {
      streams += 1;
      return [];
    },
  };
  assert.deepEqual(
    await __officeBridgeTestHooks.loadBlob(chunks, 'blob_refresh', expectedSha256, fileLoader),
    currentBytes,
  );
  assert.equal(queries.length, 2);
  assert.match(queries[1].requireRevision, /^blob:blob_refresh:/);
  assert.equal(streams, 0);
});

test('bridge blocks writes without collection permission before staging any bytes', async () => {
  let writes = 0;
  let commands = 0;
  const bridge = createBusinessOsOfficeBridge({
    db: { collection() { return { async bulkUpsert() { writes += 1; } }; } },
    permissions: { canWriteCollection: () => false },
    commandBus: { async dispatch() { commands += 1; } },
  }, 'document');
  await assert.rejects(bridge.commit({ recordId: 'doc_1', baseVersionId: 'v1', bytes: new Uint8Array([1]) }),
    (error) => error.code === 'permission_denied');
  assert.equal(writes, 0);
  assert.equal(commands, 0);
});

test('bridge preserves native stale-base conflicts and waits for RxDB reconnect before dispatch', async () => {
  const events = [];
  const bridge = createBusinessOsOfficeBridge({
    db: { collection() { return {}; } },
    sync: {
      async startCollection(name) {
        events.push(`start:${name}`);
        return { state: { async awaitInSync() { events.push(`synced:${name}`); } } };
      },
      async leaseCollection(name, reason) {
        events.push(`lease:${name}:${reason}`);
        return {
          bridge: { state: { async awaitInSync() { events.push(`synced:${name}`); } } },
          async release() { events.push(`release:${name}`); },
        };
      },
    },
    commandBus: { async dispatch() {
      events.push('dispatch');
      return { status: 'failed', payload: { outcome: { ok: false, error_code: 'version_conflict', error: 'stale base version' } } };
    } },
  }, 'spreadsheet');
  await assert.rejects(bridge.prepare({ recordId: 'sheet_1', versionId: 'v1' }),
    (error) => error.code === 'version_conflict');
  assert.deepEqual(events, [
    'start:spreadsheets', 'synced:spreadsheets',
    'start:spreadsheet_versions', 'synced:spreadsheet_versions',
    'lease:spreadsheet_blob_chunks:spreadsheets-prepare', 'synced:spreadsheet_blob_chunks',
    'dispatch',
    'release:spreadsheet_blob_chunks',
  ]);
});

test('Business OS bridge reads versions and dispatches typed office commands', async () => {
  const source = new TextEncoder().encode('docx');
  const base64 = Buffer.from(source).toString('base64');
  const commands = [];
  const docs = new Map([
    ['doc_1', { id: 'doc_1', current_version_id: 'doc_1_v1', filename: 'one.docx' }],
    ['doc_1_v1', { id: 'doc_1_v1', document_id: 'doc_1', blob_id: 'blob_1' }],
  ]);
  const chunks = [{ id: 'blob_1_0000', blob_id: 'blob_1', document_id: 'doc_1', version_id: 'doc_1_v1', idx: 0, total: 1, data: base64 }];
  const leases = [];
  const collection = (name) => ({
    findOne(id) { return { async exec() { return docs.get(id) || null; } }; },
    find({ selector }) { return { async exec() { return name === 'document_blob_chunks' ? chunks.filter((row) => row.blob_id === selector.blob_id) : []; } }; },
    async bulkUpsert(rows) { chunks.push(...rows); },
  });
  const ctx = {
    db: { collection },
    sync: {
      async startCollection() { return { state: { async awaitInSync() {} } }; },
      async leaseCollection(name, reason) {
        leases.push(`lease:${name}:${reason}`);
        return {
          bridge: { state: { async awaitInSync() {}, async pushToRemotePeers() { leases.push(`push:${name}`); } } },
          async release() { leases.push(`release:${name}`); },
        };
      },
    },
    permissions: { canWriteCollection: () => true },
    commandBus: { async dispatch(command, options) {
      commands.push({ command, options });
      if (command.type === 'office.document.export') {
        return { status: 'completed', payload: { outcome: { ok: true, blob_id: 'blob_1', version_id: 'doc_1_v1' } } };
      }
      return { status: 'completed', payload: { outcome: { ok: true } } };
    } },
  };
  const bridge = createBusinessOsOfficeBridge(ctx, 'document');
  const loaded = await bridge.loadVersion({ recordId: 'doc_1' });
  assert.deepEqual(loaded.canonicalBytes, source);
  await bridge.prepare({ recordId: 'doc_1', versionId: 'doc_1_v1' });
  assert.equal(commands[0].command.type, 'office.document.prepare');
  assert.equal(commands[0].command.client_context.transport, 'rxdb-webrtc');
  assert.equal(commands[0].options.until, 'terminal');
  await bridge.commit({
    recordId: 'doc_1',
    baseVersionId: 'doc_1_v1',
    editorProtocol: 'euro-office-cell-binary-v10',
    editorProtocolVersion: 10,
    implementedFeatures: [
      'document.open-render-zoom',
      'document.edit-save',
      'document.undo-clipboard-keyboard',
      'document.character-paragraph-formatting',
      'document.styles-lists-numbering',
      'document.tables',
      'document.images-positioning',
      'document.sections-headers-footers',
      'document.links-bookmarks-fields',
      'document.comments-track-changes',
      'document.drawings-charts',
    ],
    reason: 'test',
    bytes: source,
  });
  assert.equal(commands[1].command.type, 'office.document.commit');
  assert.deepEqual(commands[1].command.payload.implemented_features, [
    'document.open-render-zoom',
    'document.edit-save',
    'document.undo-clipboard-keyboard',
    'document.character-paragraph-formatting',
    'document.styles-lists-numbering',
    'document.tables',
    'document.images-positioning',
    'document.sections-headers-footers',
    'document.links-bookmarks-fields',
    'document.comments-track-changes',
    'document.drawings-charts',
  ]);
  const exported = await bridge.export({ recordId: 'doc_1', versionId: 'doc_1_v1', format: 'docx' });
  assert.deepEqual(exported.bytes, source);
  assert.equal(commands[2].command.type, 'office.document.export');
  assert.deepEqual(leases, [
    'lease:document_blob_chunks:documents-load-version', 'release:document_blob_chunks',
    'lease:document_blob_chunks:documents-prepare', 'release:document_blob_chunks',
    'lease:document_blob_chunks:documents-commit', 'push:document_blob_chunks', 'release:document_blob_chunks',
    'lease:document_blob_chunks:documents-export', 'release:document_blob_chunks',
  ]);
});

test('prepare result is immediately readable before the native version projection arrives', async () => {
  const canonicalBytes = new TextEncoder().encode('canonical-docx');
  const editorBytes = new TextEncoder().encode('DOCY;v10;0;prepared-editor');
  const canonicalSha = createHash('sha256').update(canonicalBytes).digest('hex');
  const editorSha = createHash('sha256').update(editorBytes).digest('hex');
  const docs = new Map([
    ['doc_1', { id: 'doc_1', current_version_id: 'doc_1_v1', filename: 'one.docx' }],
    ['doc_1_v1', { id: 'doc_1_v1', document_id: 'doc_1', blob_id: 'canonical_1', source_sha256: canonicalSha }],
  ]);
  const localChunks = [{
    id: 'canonical_1_0000',
    blob_id: 'canonical_1',
    document_id: 'doc_1',
    version_id: 'doc_1_v1',
    idx: 0,
    total: 1,
    data: Buffer.from(canonicalBytes).toString('base64'),
  }];
  const streamed = [];
  const collection = (name) => ({
    findOne(id) { return { async exec() { return docs.get(id) || null; } }; },
    find({ selector }) {
      return {
        async exec() {
          return name === 'document_blob_chunks'
            ? localChunks.filter((row) => row.blob_id === selector.blob_id)
            : [];
        },
      };
    },
  });
  const fileLoader = {
    async fetchFile(blobId) {
      streamed.push(blobId);
      return [{ sequence: 0, bytesBase64: Buffer.from(editorBytes).toString('base64') }];
    },
  };
  const bridge = createBusinessOsOfficeBridge({
    db: { collection },
    sync: {
      async startCollection() { return { state: { async awaitInSync() {} } }; },
      async leaseCollection() {
        return {
          bridge: { state: { async awaitInSync() {}, demandFileLoader: fileLoader } },
          async release() {},
        };
      },
    },
    commandBus: {
      async dispatch() {
        return {
          status: 'completed',
          result: {
            ok: true,
            version_id: 'doc_1_v1',
            editor_blob_id: 'editor_1',
            editor_protocol: 'euro-office-word-binary-v10',
            editor_protocol_version: 10,
            editor_sha256: editorSha,
            source_sha256: canonicalSha,
            manifest: { schema_version: 'ctox-office-semantic-manifest-v1' },
            editor_manifest: { schema_version: 'ctox-office-editor-payload-manifest-v1' },
          },
        };
      },
    },
  }, 'document');

  await bridge.prepare({ recordId: 'doc_1', versionId: 'doc_1_v1' });
  const loaded = await bridge.loadVersion({ recordId: 'doc_1', versionId: 'doc_1_v1' });

  assert.equal(docs.get('doc_1_v1').editor_blob_id, undefined);
  assert.equal(loaded.version.editor_blob_id, 'editor_1');
  assert.equal(loaded.version.conversion_state, 'prepared');
  assert.deepEqual(loaded.canonicalBytes, canonicalBytes);
  assert.deepEqual(loaded.editorBytes, editorBytes);
  assert.deepEqual(streamed, ['editor_1']);
});

test('office bridge resumes an inserted command after a retryable push timeout', async () => {
  let dispatches = 0;
  let resumes = 0;
  const timeout = Object.assign(new Error('push timed out'), { code: 'sync_unavailable', retryable: true });
  const bridge = createBusinessOsOfficeBridge({
    db: { collection() { return {}; } },
    sync: {
      async startCollection() { return { state: { async awaitInSync() {} } }; },
      async leaseCollection() {
        return { bridge: { state: { async awaitInSync() {} } }, async release() {} };
      },
    },
    commandBus: {
      async dispatch() { dispatches += 1; throw timeout; },
      async getStatus(commandId) { return { id: commandId, status: 'pending_sync' }; },
      async resumeTracking() {
        resumes += 1;
        return { status: 'completed', result: { ok: true, version_id: 'doc_1_v1', editor_blob_id: 'editor_1' } };
      },
    },
  }, 'document');

  const result = await bridge.prepare({ recordId: 'doc_1', versionId: 'doc_1_v1' });
  assert.equal(result.editor_blob_id, 'editor_1');
  assert.equal(dispatches, 1);
  assert.equal(resumes, 1);
});

test('spreadsheet commit stages XLSX bytes and carries the conflict base through the command bus', async () => {
  const staged = [];
  const commands = [];
  const ctx = {
    db: { collection(name) {
      if (name !== 'spreadsheet_blob_chunks') return {};
      return { async bulkUpsert(rows) { staged.push(...rows); } };
    } },
    permissions: { canWriteCollection: () => true },
    sync: {
      async leaseCollection() {
        return {
          bridge: { state: { async awaitInSync() {}, async pushToRemotePeers() {} } },
          async release() {},
        };
      },
    },
    commandBus: { async dispatch(command, options) {
      commands.push({ command, options });
      return { status: 'completed', payload: { outcome: { ok: true, record_id: 'sheet_1', version_id: 'sheet_1_v2', blob_id: 'sheet_1_v2_blob' } } };
    } },
  };
  const bridge = createBusinessOsOfficeBridge(ctx, 'spreadsheet');
  const result = await bridge.commit({
    recordId: 'sheet_1',
    baseVersionId: 'sheet_1_v1',
    editorProtocol: 'euro-office-cell-binary-v10',
    editorProtocolVersion: 10,
    implementedFeatures: ['spreadsheet.open-render-sheets', 'spreadsheet.edit-save'],
    reason: 'test',
    bytes: new TextEncoder().encode('XLSY;v10;0;payload'),
  });
  assert.equal(result.version_id, 'sheet_1_v2');
  assert.equal(staged.length, 1);
  assert.equal(commands[0].command.type, 'office.spreadsheet.commit');
  assert.equal(commands[0].command.payload.base_version_id, 'sheet_1_v1');
  assert.equal(commands[0].command.client_context.transport, 'rxdb-webrtc');
  assert.deepEqual(commands[0].command.payload.implemented_features, ['spreadsheet.open-render-sheets', 'spreadsheet.edit-save']);
  assert.equal(commands[0].options.until, 'terminal');
});

test('Business OS bridge releases a demand-only chunk lease after an operation fails', async () => {
  let released = 0;
  const bridge = createBusinessOsOfficeBridge({
    db: { collection() { return {}; } },
    sync: {
      async startCollection() { return { state: { async awaitInSync() {} } }; },
      async leaseCollection() {
        return {
          bridge: { state: { async awaitInSync() {} } },
          async release() { released += 1; },
        };
      },
    },
    commandBus: { async dispatch() { throw new Error('native command failed'); } },
  }, 'document');
  await assert.rejects(bridge.prepare({ recordId: 'doc_1', versionId: 'v1' }), /native command failed/);
  assert.equal(released, 1);
});

test('Office RPC budgets all storage-backed editor operations for live replication latency', async () => {
  const capsule = await readFile(new URL('./src/capsule.mjs', import.meta.url), 'utf8');
  const frameRuntime = await readFile(new URL('./src/frame-runtime.mjs', import.meta.url), 'utf8');
  assert.match(capsule, /const OFFICE_OPERATION_TIMEOUT_MS = 120000/);
  for (const method of ['open', 'save', 'export']) {
    assert.match(capsule, new RegExp(`editor\\.${method}[^\\n]+timeoutMs: OFFICE_OPERATION_TIMEOUT_MS`));
  }
  for (const method of ['loadVersion', 'prepare', 'commit', 'export']) {
    assert.match(frameRuntime, new RegExp(`bridge\\.${method}[^\\n]+timeoutMs: OFFICE_OPERATION_TIMEOUT_MS`));
  }
});

test('CTOX Spreadsheets production lifecycle uses the Business OS wrapper and fork UI', async () => {
  const harness = await readFile(new URL('./oracle/business-os-spreadsheet-open-render-sheets.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/spreadsheet.production-lifecycle.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.production-lifecycle.json', import.meta.url), 'utf8'));
  assert.match(harness, /runtimeSettings\.office\.spreadsheets_engine/);
  assert.match(harness, /await mount\(ctx\)/);
  assert.match(flow, /remount\('legacy'\)/);
  assert.match(flow, /remount\('ctox_spreadsheets'\)/);
  assert.match(flow, /page\.reload/);
  assert.deepEqual(evidence.lifecycle, {
    initial_ctox_iframes: 1,
    after_legacy_iframes: 0,
    after_ctox_remount_iframes: 1,
    after_browser_reload_iframes: 1,
    typed_setting: 'office.spreadsheets_engine',
    environment_toggle_used: false,
  });
  assert.deepEqual(evidence.matrix.map(({ locale, shell_style, result }) => ({ locale, shell_style, result })), [
    { locale: 'en', shell_style: 'macos', result: 'passed' },
    { locale: 'de', shell_style: 'windows', result: 'passed' },
  ]);
  assert.equal(evidence.browser_health.console_errors, 0);
});

test('CTOX Documents production lifecycle uses the Business OS wrapper and complete fork UI closure', async () => {
  const harness = await readFile(new URL('./oracle/business-os-document-production-lifecycle.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/document.production-lifecycle.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/document.production-lifecycle.json', import.meta.url), 'utf8'));
  const provenance = JSON.parse(await readFile(new URL('../vendor/ctox-office/provenance.json', import.meta.url), 'utf8'));
  assert.match(harness, /runtimeSettings\.office\.documents_engine/);
  assert.match(harness, /await mount\(ctx\)/);
  assert.match(flow, /remount\('legacy'\)/);
  assert.match(flow, /remount\('ctox_documents'\)/);
  assert.match(flow, /page\.reload/);
  assert.match(flow, /Kommentar hinzufügen/);
  assert.match(flow, /Nachverfolgen von Änderungen/);
  assert.match(flow, /Überprüfung/);
  assert.equal(evidence.wrapper, 'modules/documents.mount(ctx)');
  assert.deepEqual(evidence.lifecycle, {
    initial_ctox_iframes: 1,
    after_legacy_iframes: 0,
    after_ctox_remount_iframes: 1,
    after_browser_reload_iframes: 1,
    typed_setting: 'office.documents_engine',
    environment_toggle_used: false,
  });
  assert.deepEqual(evidence.ctox_documents_permissions, {
    comments_button_enabled: true,
    add_comment_button_enabled: true,
    track_changes_button_enabled: true,
    review_mode_entered: true,
  });
  assert.equal(evidence.browser_health.console_errors, 0);
  assert.ok(provenance.artifacts.some(({ path, sha256 }) => path.endsWith('/icon-document.svg') && sha256 === evidence.closure_regression.sha256));
});

test('CTOX Documents and CTOX Spreadsheets pass the Business OS product UI matrix', async () => {
  const flow = await readFile(new URL('./oracle/flows/office.fork-business-os-ui.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/office.fork-business-os-ui.json', import.meta.url), 'utf8'));
  assert.equal(evidence.status, 'passed');
  assert.deepEqual(evidence.products, ['ctox-documents', 'ctox-spreadsheets']);
  assert.deepEqual(evidence.required_locales, ['de', 'en']);
  assert.deepEqual(evidence.required_themes, ['light', 'dark']);
  assert.deepEqual(evidence.required_widths, [360, 640, 1600]);
  assert.equal(evidence.results.length, 8);
  assert.ok(evidence.results.every(({ status }) => status === 'passed'));
  assert.equal(evidence.assertions.visible_foreign_brand, false);
  assert.equal(evidence.assertions.browser_errors, 0);
  assert.match(flow, /business-os-document-production-lifecycle\.html/);
  assert.match(flow, /business-os-spreadsheet-open-render-sheets\.html/);
  assert.match(flow, /visibleForeignBrand/);
  assert.match(flow, /live_theme_switch/);
});

test('clean-profile gate starts both Business OS apps without inherited browser state', async () => {
  const flow = await readFile(new URL('./oracle/flows/office.clean-profile.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/office.clean-profile.json', import.meta.url), 'utf8'));
  assert.match(flow, /browser\.newContext/);
  assert.match(flow, /initialStorage\.localStorageKeys\.length/);
  assert.match(flow, /context\.close/);
  assert.deepEqual(evidence.cases.map(({ kind, initial_local_storage_keys, initial_session_storage_keys, console_errors }) => ({ kind, initial_local_storage_keys, initial_session_storage_keys, console_errors })), [
    { kind: 'document', initial_local_storage_keys: [], initial_session_storage_keys: [], console_errors: 0 },
    { kind: 'spreadsheet', initial_local_storage_keys: [], initial_session_storage_keys: [], console_errors: 0 },
  ]);
  assert.equal(evidence.profiles_closed, true);
  assert.equal(evidence.http_business_data_routes, false);
});

test('offline/reconnect gate stages locally and dispatches only after RxDB sync recovers', async () => {
  const flow = await readFile(new URL('./oracle/flows/office.offline-reconnect.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/office.offline-reconnect.json', import.meta.url), 'utf8'));
  assert.match(flow, /setOffline\(true\)/);
  assert.match(flow, /replicationUp === false/);
  assert.match(flow, /setOffline\(false\)/);
  assert.deepEqual(evidence.cases.map(({ kind, commands_while_offline, commands_after_reconnect, transport, result }) => ({ kind, commands_while_offline, commands_after_reconnect, transport, result })), [
    { kind: 'document', commands_while_offline: 0, commands_after_reconnect: 1, transport: 'rxdb-webrtc', result: 'passed' },
    { kind: 'spreadsheet', commands_while_offline: 0, commands_after_reconnect: 1, transport: 'rxdb-webrtc', result: 'passed' },
  ]);
  assert.equal(evidence.native_daemon_restart_covered, false);
});

test('native peer restart gate commits both Office kinds through the real WebRTC command path', async () => {
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/office.native-peer-restart.json', import.meta.url), 'utf8'));
  assert.equal(evidence.status, 'passed');
  assert.deepEqual(evidence.cases.map(({ kind, command_type, status, queue_tasks, base_version, committed_version, canonical_blob_chunks }) => ({ kind, command_type, status, queue_tasks, base_version, committed_version, canonical_blob_chunks })), [
    { kind: 'document', command_type: 'office.document.commit', status: 'completed', queue_tasks: 0, base_version: 'v1', committed_version: 'v2', canonical_blob_chunks: 1 },
    { kind: 'spreadsheet', command_type: 'office.spreadsheet.commit', status: 'completed', queue_tasks: 0, base_version: 'v1', committed_version: 'v2', canonical_blob_chunks: 1 },
  ]);
  assert.equal(evidence.transport, 'rxdb-webrtc');
  assert.equal(evidence.restart.browser_remained_open, true);
  assert.equal(evidence.restart.unknown_signals, 0);
});

test('spreadsheet formulas preserve reference semantics, cached values, and errors', () => {
  const cell = (reference, display, formula = null) => ({ reference, display, formula });
  const overview = { name: 'Overview', cells: [cell('B2', '10'), cell('C2', '20'), cell('B3', '20', '=B2*2'), cell('B4', '15', '=$B$2+5')] };
  const details = { name: 'Details', cells: [cell('B4', '42')] };
  const workbook = { sheets: [overview, details] };
  assert.deepEqual(__spreadsheetRuntimeTestHooks.evaluateFormula(workbook, overview, 'SUM(B2:B4)'), { value: '45', error: false });
  assert.deepEqual(__spreadsheetRuntimeTestHooks.evaluateFormula(workbook, overview, "'Details'!B4+1"), { value: '43', error: false });
  assert.deepEqual(__spreadsheetRuntimeTestHooks.evaluateFormula(workbook, overview, '1/0'), { value: '#DIV/0!', error: true });
  assert.equal(__spreadsheetRuntimeTestHooks.shiftFormula('=B2+$B$2+B$2+$B2', 'B7', 'C7'), '=C2+$B$2+C$2+$B2');
});

test('spreadsheet formula differential uses native XLSY, the CTOX Spreadsheets fork, Rust export, and Business OS mount', async () => {
  const flow = await readFile(new URL('./oracle/flows/spreadsheet.formulas-references.playwright.js', import.meta.url), 'utf8');
  const businessOsFlow = await readFile(new URL('./oracle/flows/spreadsheet.formulas-references.business-os.playwright.js', import.meta.url), 'utf8');
  const harness = await readFile(new URL('./oracle/ctox-spreadsheet-formulas-references.html', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.formulas-references.json', import.meta.url), 'utf8'));
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /XLSY;v10/);
  assert.doesNotMatch(harness, /ctox-xlsx-bootstrap/);
  assert.match(flow, /#ce-cell-content/);
  assert.match(flow, /Kopieren \(⌘\+C\)/);
  assert.match(flow, /Einfügen \(⌘\+V\)/);
  assert.match(businessOsFlow, /modules\/spreadsheets\.mount\(ctx\)/);
  assert.match(businessOsFlow, /office\.spreadsheet\.commit/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.runtime, 'ctox-spreadsheets-fork');
  assert.equal(evidence.ctox.native_initial_payload.bytes, 2667);
  assert.deepEqual(evidence.ctox.canonical_export.changed_parts, ['xl/worksheets/sheet1.xml']);
});

test('spreadsheet multi-sheet merge freeze uses CTOX Spreadsheets and native pane records end to end', async () => {
  const flow = await readFile(new URL('./oracle/flows/spreadsheet.multi-sheet-merge-freeze.playwright.js', import.meta.url), 'utf8');
  const businessOsFlow = await readFile(new URL('./oracle/flows/spreadsheet.multi-sheet-merge-freeze.business-os.playwright.js', import.meta.url), 'utf8');
  const harness = await readFile(new URL('./oracle/ctox-spreadsheet-multi-sheet-merge-freeze.html', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.multi-sheet-merge-freeze.json', import.meta.url), 'utf8'));
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /XLSY;v10/);
  assert.doesNotMatch(harness, /ctox-xlsx-bootstrap/);
  assert.match(flow, /Verbinden und zentrieren/);
  assert.match(flow, /asc_freezePane\(null, 1, 2\)/);
  assert.match(flow, /OPERATIONS_MARKER_A9C4/);
  assert.match(flow, /Archive/);
  assert.match(businessOsFlow, /modules\/spreadsheets\.mount\(ctx\)/);
  assert.match(businessOsFlow, /office\.spreadsheet\.commit/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.runtime, 'ctox-spreadsheets-fork');
  assert.equal(evidence.ctox.initial_editor_payload.bytes, 1704);
  assert.deepEqual(evidence.ctox.editor_export.merged_cells, ['B3:C3']);
  assert.equal(evidence.ctox.editor_export.frozen_pane.active_pane, 'bottomRight');
  assert.deepEqual(evidence.ctox.canonical_export.changed_parts, ['xl/worksheets/sheet1.xml']);
  assert.equal(evidence.ctox.business_os_mount.transport, 'rxdb-webrtc');
});

test('spreadsheet sort filter tables uses CTOX Spreadsheets and native XLSY table records end to end', async () => {
  const flow = await readFile(new URL('./oracle/flows/spreadsheet.sort-filter-tables.playwright.js', import.meta.url), 'utf8');
  const businessOsFlow = await readFile(new URL('./oracle/flows/spreadsheet.sort-filter-tables.business-os.playwright.js', import.meta.url), 'utf8');
  const harness = await readFile(new URL('./oracle/ctox-spreadsheet-sort-filter-tables.html', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.sort-filter-tables.json', import.meta.url), 'utf8'));
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /XLSY;v10/);
  assert.doesNotMatch(harness, /ctox-xlsx-bootstrap/);
  assert.match(flow, /Tabellen-Design/);
  assert.match(flow, /Asc\.editor\.asc_getActiveCellCoord\(\)/);
  assert.match(flow, /Absteigend sortieren/);
  assert.match(flow, /hasText: 'North'/);
  assert.match(businessOsFlow, /modules\/spreadsheets\.mount\(ctx\)/);
  assert.match(businessOsFlow, /office\.spreadsheet\.commit/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.runtime, 'ctox-spreadsheets-fork');
  assert.equal(evidence.ctox.initial_editor_payload.bytes, 2300);
  assert.deepEqual(evidence.ctox.editor_export.hidden_rows, [4, 5, 6]);
  assert.deepEqual(evidence.ctox.canonical_export.changed_parts, ['xl/tables/table1.xml', 'xl/worksheets/sheet1.xml']);
  assert.equal(evidence.ctox.business_os_mount.transport, 'rxdb-webrtc');
});

test('spreadsheet validation conditional formatting uses CTOX Spreadsheets and native XLSY records end to end', async () => {
  const flow = await readFile(new URL('./oracle/flows/spreadsheet.validation-conditional-formatting.playwright.js', import.meta.url), 'utf8');
  const businessOsFlow = await readFile(new URL('./oracle/flows/spreadsheet.validation-conditional-formatting.business-os.playwright.js', import.meta.url), 'utf8');
  const harness = await readFile(new URL('./oracle/ctox-spreadsheet-validation-conditional-formatting.html', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.validation-conditional-formatting.json', import.meta.url), 'utf8'));
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /XLSY;v10/);
  assert.doesNotMatch(harness, /editorBytes:state\.bytes/);
  assert.match(flow, /Datenüberprüfung/);
  assert.match(flow, /Draft;Review;Final;Approved/);
  assert.match(flow, /asc_getDataValidationProps/);
  assert.match(businessOsFlow, /modules\/spreadsheets\.mount\(ctx\)/);
  assert.match(businessOsFlow, /office\.spreadsheet\.commit/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.runtime, 'ctox-spreadsheets-fork');
  assert.equal(evidence.ctox.initial_editor_payload.bytes, 2840);
  assert.equal(evidence.ctox.editor_export.conditional_rules, 2);
  assert.deepEqual(evidence.ctox.canonical_export.changed_parts, ['xl/sharedStrings.xml', 'xl/worksheets/sheet1.xml']);
  assert.equal(evidence.ctox.business_os_mount.transport, 'rxdb-webrtc');
});

test('spreadsheet comments names protection uses CTOX Spreadsheets and native XLSY records end to end', async () => {
  const flow = await readFile(new URL('./oracle/flows/spreadsheet.comments-names-protection.playwright.js', import.meta.url), 'utf8');
  const businessOsFlow = await readFile(new URL('./oracle/flows/spreadsheet.comments-names-protection.business-os.playwright.js', import.meta.url), 'utf8');
  const harness = await readFile(new URL('./oracle/ctox-spreadsheet-comments-names-protection.html', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.comments-names-protection.json', import.meta.url), 'utf8'));
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /XLSY;v10/);
  assert.match(flow, /CTOX_ADDED_CELL_COMMENT/);
  assert.match(flow, /CTOX_Amount_Reviewed/);
  assert.match(flow, /Blatt/);
  assert.match(businessOsFlow, /modules\/spreadsheets\.mount\(ctx\)/);
  assert.match(businessOsFlow, /office\.spreadsheet\.commit/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.runtime, 'ctox-spreadsheets-fork');
  assert.equal(evidence.ctox.initial_editor_payload.bytes, 2398);
  assert.deepEqual(evidence.ctox.editor_export.comments, [
    'B4:CTOX_EXISTING_CELL_COMMENT',
    'C4:CTOX_ADDED_CELL_COMMENT',
  ]);
  assert.deepEqual(evidence.ctox.canonical_export.changed_parts, [
    'xl/comments1.xml',
    'xl/drawings/vmlDrawing1.vml',
    'xl/workbook.xml',
    'xl/worksheets/sheet1.xml',
  ]);
  assert.equal(evidence.ctox.canonical_export.vml_note_shapes, 2);
  assert.equal(evidence.ctox.business_os_mount.transport, 'rxdb-webrtc');
});

test('spreadsheet charts use CTOX Spreadsheets chart UI and native DrawingML records end to end', async () => {
  const flow = await readFile(new URL('./oracle/flows/spreadsheet.charts.playwright.js', import.meta.url), 'utf8');
  const businessOsFlow = await readFile(new URL('./oracle/flows/spreadsheet.charts.business-os.playwright.js', import.meta.url), 'utf8');
  const harness = await readFile(new URL('./oracle/ctox-spreadsheet-charts.html', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.charts.json', import.meta.url), 'utf8'));
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /ctox-export\.Editor\.bin/);
  assert.match(harness, /XLSY;v10/);
  assert.match(flow, /Diagrammeinstellungen/);
  assert.match(flow, /Stil 2/);
  assert.match(flow, /capture-ctox\/spreadsheet\.charts/);
  assert.match(businessOsFlow, /modules\/spreadsheets\.mount\(ctx\)/);
  assert.match(businessOsFlow, /office\.spreadsheet\.commit/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.runtime, 'ctox-spreadsheets-fork');
  assert.equal(evidence.ctox.initial_editor_payload.bytes, 3379);
  assert.deepEqual(evidence.ctox.canonical_export.changed_parts, [
    'xl/charts/chart1.xml',
    'xl/drawings/drawing1.xml',
  ]);
  assert.equal(evidence.ctox.canonical_export.unchanged_parts, 14);
  assert.equal(evidence.ctox.business_os_mount.transport, 'rxdb-webrtc');
});

test('spreadsheet pivot and print layout use CTOX Spreadsheets and native XLSY records end to end', async () => {
  const flow = await readFile(new URL('./oracle/flows/spreadsheet.pivot-print-layout.playwright.js', import.meta.url), 'utf8');
  const businessOsFlow = await readFile(new URL('./oracle/flows/spreadsheet.pivot-print-layout.business-os.playwright.js', import.meta.url), 'utf8');
  const harness = await readFile(new URL('./oracle/ctox-spreadsheet-pivot-print-layout.html', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.pivot-print-layout.json', import.meta.url), 'utf8'));
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /ctox-export\.Editor\.bin/);
  assert.match(harness, /XLSY;v10/);
  assert.match(flow, /Einstellungen der Pivot-Tabelle/);
  assert.match(flow, /CTOXRevenuePivot2026/);
  assert.match(flow, /Erste Seite anders/);
  assert.match(businessOsFlow, /modules\/spreadsheets\.mount\(ctx\)/);
  assert.match(businessOsFlow, /office\.spreadsheet\.commit/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.runtime, 'ctox-spreadsheets-fork');
  assert.deepEqual(evidence.ctox.canonical_export.changed_parts, [
    'xl/pivotTables/pivotTable1.xml',
    'xl/worksheets/sheet1.xml',
  ]);
  assert.equal(evidence.ctox.canonical_export.unchanged_parts, 16);
  assert.equal(evidence.ctox.business_os_mount.transport, 'rxdb-webrtc');
});

test('spreadsheet XLSX corpus aggregates all native and Oracle gates', async () => {
  const corpus = JSON.parse(await readFile(new URL('../../../../tests/fixtures/office/spreadsheet/corpus.json', import.meta.url), 'utf8'));
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.xlsx-roundtrip-corpus.json', import.meta.url), 'utf8'));
  const validator = await readFile(new URL('./oracle/validate-spreadsheet-corpus.mjs', import.meta.url), 'utf8');
  assert.equal(corpus.entries.length, 11);
  assert.equal(corpus.entries.reduce((sum, entry) => sum + entry.parts, 0), 147);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.native_identity_roundtrip.all_original_parts_byte_identical, true);
  assert.equal(evidence.ctox.dependency_gate.oracle_reopen, 11);
  assert.match(validator, /differential_passed/);
  assert.match(validator, /rxdb-webrtc/);
});

test('spreadsheet validation and conditional formatting preserve rule semantics', () => {
  const sheet = {
    cells: [{ reference: 'D2', display: '10' }, { reference: 'D3', display: '60' }, { reference: 'D4', display: '100' }, { reference: 'E2', display: '75' }],
    validations: [
      { type: 'list', sqref: 'B2', allowBlank: false, formula1: '"Draft,Review,Final"' },
      { type: 'whole', operator: 'between', sqref: 'C2', allowBlank: false, formula1: '1', formula2: '10' },
    ],
    conditionalFormats: [
      { type: 'colorScale', sqref: 'D2:D4', colors: ['FFF8696B', 'FFFFEB84', 'FF63BE7B'] },
      { type: 'cellIs', operator: 'greaterThan', sqref: 'E2:E2', formula: '50', colors: [] },
    ],
  };
  assert.equal(__spreadsheetRuntimeTestHooks.validateCellValue(sheet, 'B2', 'Final').valid, true);
  assert.equal(__spreadsheetRuntimeTestHooks.validateCellValue(sheet, 'B2', 'Blocked').valid, false);
  assert.equal(__spreadsheetRuntimeTestHooks.validateCellValue(sheet, 'C2', '15').valid, false);
  assert.equal(__spreadsheetRuntimeTestHooks.validateCellValue(sheet, 'C2', '8').valid, true);
  assert.deepEqual(__spreadsheetRuntimeTestHooks.conditionalStyleFor(sheet, 'D4', '100'), { background: '#63BE7B' });
  assert.deepEqual(__spreadsheetRuntimeTestHooks.conditionalStyleFor(sheet, 'E2', '75'), { background: '#c6efce', color: '#006100' });
});

test('CTOX Spreadsheets fork runtime accepts only the Euro-Office XLSY binary protocol', () => {
  const binary = new TextEncoder().encode('XLSY;v10;0;payload');
  const xlsx = new Uint8Array([0x50, 0x4b, 0x03, 0x04]);
  assert.equal(__ctoxForkTestHooks.hasCellBinarySignature(binary), true);
  assert.equal(__ctoxForkTestHooks.hasCellBinarySignature(xlsx), false);
});

test('CTOX Documents fork runtime accepts only the Euro-Office DOCY binary protocol', () => {
  const binary = new TextEncoder().encode('DOCY;v10;0;payload');
  const docx = new Uint8Array([0x50, 0x4b, 0x03, 0x04]);
  assert.equal(__ctoxForkTestHooks.hasEditorBinarySignature(binary, 'document'), true);
  assert.equal(__ctoxForkTestHooks.hasEditorBinarySignature(docx, 'document'), false);
});

test('CTOX Documents fork runtime registers DOCX media as object URLs for Euro-Office image loading', async () => {
  const docx = storedZip([
    ['word/document.xml', '<w:document/>'],
    ['word/media/image1.png', 'png-one'],
    ['word/media/image2.jpeg', 'jpeg-two'],
  ]);
  const media = await __ctoxForkTestHooks.extractOfficeZipMedia(docx);
  assert.deepEqual(media.map(({ name }) => name), ['word/media/image1.png', 'word/media/image2.jpeg']);
  assert.deepEqual(media.map(({ bytes }) => new TextDecoder().decode(bytes)), ['png-one', 'jpeg-two']);

  const registered = {};
  const revoked = [];
  let objectUrlIndex = 0;
  const upstream = {
    Blob,
    URL: {
      createObjectURL: () => `blob:ctox-${++objectUrlIndex}`,
      revokeObjectURL: (url) => revoked.push(url),
    },
    AscCommon: {
      g_oDocumentUrls: {
        addUrls(urls) { Object.assign(registered, urls); },
      },
    },
  };
  const resolver = await __ctoxForkTestHooks.installDocumentMediaResolver(upstream, docx);
  assert.equal(resolver.count, 2);
  assert.equal(registered['media/image1.png'], 'blob:ctox-1');
  assert.equal(registered['media/media/image1.png'], 'blob:ctox-1');
  assert.equal(registered['image1.png'], 'blob:ctox-1');
  assert.equal(registered['media/image2.jpeg'], 'blob:ctox-2');
  assert.equal(
    __ctoxForkTestHooks.resolveOfficeMediaUrl(
      'http://127.0.0.1/upstream/web-apps/apps/documenteditor/main/media/image1.png',
      registered,
    ),
    'blob:ctox-1',
  );
  assert.equal(__ctoxForkTestHooks.resolveOfficeMediaUrl('media/image2.jpeg', registered), 'blob:ctox-2');
  resolver.destroy();
  assert.deepEqual(revoked, ['blob:ctox-1', 'blob:ctox-2']);
});

test('CTOX product bundle provenance contains both CTOX forks and their pinned dependency closures', async () => {
  const provenance = JSON.parse(await readFile(new URL('../vendor/ctox-office/provenance.json', import.meta.url), 'utf8'));
  assert.equal(provenance.upstream_source_status, 'pinned-web-apps-sdkjs-document-spreadsheet-closure');
  assert.deepEqual(provenance.fork_products.map(({ product_id, runtime_entry }) => ({ product_id, runtime_entry })), [
    { product_id: 'ctox-documents', runtime_entry: 'runtime/ctox-documents.mjs' },
    { product_id: 'ctox-spreadsheets', runtime_entry: 'runtime/ctox-spreadsheets.mjs' },
  ]);
  assert.ok(provenance.upstream_static_inputs.some(({ path }) => path === 'sdkjs/word/sdk-all.js'));
  assert.ok(provenance.upstream_static_inputs.some(({ path }) => path === 'web-apps/apps/documenteditor/main/app.js'));
  assert.ok(provenance.upstream_static_inputs.some(({ path }) => path === 'sdkjs/cell/sdk-all.js'));
  assert.ok(provenance.upstream_static_inputs.some(({ path }) => path === 'web-apps/apps/spreadsheeteditor/main/app.js'));
  assert.ok(provenance.upstream_static_inputs.some(({ path, sha256 }) => path === 'sdkjs/common/Images/fonts_thumbnail.png.bin'
    && sha256 === '6f7ab1b9cae008638d7eeccb9d69fcd291765b3cd5ec3d1422af2aa2bdd6dfac'));
  assert.ok(provenance.artifacts.some(({ path }) => path.endsWith('/forks/ctox-documents/business-os.css')));
  assert.ok(provenance.artifacts.some(({ path }) => path.endsWith('/forks/ctox-spreadsheets/business-os.css')));
  assert.equal(provenance.artifacts.some(({ path }) => path.endsWith('/runtime/document.mjs')), false);
  assert.equal(provenance.artifacts.some(({ path }) => path.endsWith('/runtime/spreadsheet.mjs')), false);
  assert.equal(provenance.bundle_inputs.some((path) => path.includes('/legacy-runtime/')), false);
  assert.ok(provenance.artifacts.length > 500, 'the real document/spreadsheet closure must be inventoried');
  const artifactPaths = new Set();
  for (const artifact of provenance.artifacts) {
    assert.match(artifact.path, /^src\/apps\/business-os\/vendor\/ctox-office\//);
    assert.equal(artifactPaths.has(artifact.path), false, `duplicate Office artifact ${artifact.path}`);
    artifactPaths.add(artifact.path);
    const file = new URL(`../../../../${artifact.path}`, import.meta.url);
    assert.equal(await sha256File(file), artifact.sha256, `Office artifact hash ${artifact.path}`);
  }
});

test('CTOX Documents and Spreadsheets own distinct fork identities and Business OS chrome', async () => {
  const documents = JSON.parse(await readFile(new URL('./src/forks/ctox-documents/manifest.json', import.meta.url), 'utf8'));
  const spreadsheets = JSON.parse(await readFile(new URL('./src/forks/ctox-spreadsheets/manifest.json', import.meta.url), 'utf8'));
  const documentsEntry = await readFile(new URL('./src/document.mjs', import.meta.url), 'utf8');
  const spreadsheetsEntry = await readFile(new URL('./src/spreadsheet.mjs', import.meta.url), 'utf8');
  const chrome = await readFile(new URL('./src/forks/shared/business-os.css', import.meta.url), 'utf8');
  const runtime = await readFile(new URL('./src/runtime/ctox-fork-core.mjs', import.meta.url), 'utf8');
  const capsule = await readFile(new URL('./src/capsule.mjs', import.meta.url), 'utf8');
  assert.deepEqual([documents.product_id, spreadsheets.product_id], ['ctox-documents', 'ctox-spreadsheets']);
  assert.notEqual(documents.runtime_entry, spreadsheets.runtime_entry);
  assert.match(documentsEntry, /createCtoxDocumentsEditor/);
  assert.match(documentsEntry, /CTOX_DOCUMENTS_PRODUCT_ID = 'ctox-documents'/);
  assert.match(spreadsheetsEntry, /createCtoxSpreadsheetsEditor/);
  assert.match(spreadsheetsEntry, /CTOX_SPREADSHEETS_PRODUCT_ID = 'ctox-spreadsheets'/);
  assert.match(runtime, /frame\.title = `\$\{productName\} Editor`/);
  assert.match(runtime, /installCtoxForkUi/);
  assert.match(runtime, /dataset\.ctoxProduct/);
  assert.doesNotMatch(runtime, /frame\.title = `Euro-Office/);
  assert.match(chrome, /--ctox-fork-accent/);
  assert.match(chrome, /#left-btn-about/);
  assert.match(chrome, /prefers-reduced-motion/);
  assert.match(capsule, /MutationObserver/);
  assert.match(capsule, /editor\.setTheme/);
});

test('CTOX Spreadsheets comparison config matches the pinned Oracle view contract', () => {
  const config = __ctoxForkTestHooks.editorConfig('de', { write: false });
  assert.equal(config.mode, 'view');
  assert.deepEqual(config.user, { id: 'ctox-local-user', name: 'CTOX' });
  assert.deepEqual(config.customization, {
    about: false,
    feedback: false,
    help: false,
    plugins: false,
    macros: false,
    compactHeader: true,
    compactToolbar: false,
    hideRightMenu: true,
    uiTheme: 'theme-light',
    zoom: 100,
  });
  assert.equal(__ctoxForkTestHooks.editorConfig('de', { write: true }, 'dark').customization.uiTheme, 'theme-dark');
  const spreadsheet = __ctoxForkTestHooks.documentConfig('sheet', { filename: 'fixture.xlsx' }, {
    write: false, export: true, comment: false, review: false,
  });
  assert.deepEqual(spreadsheet.permissions, {
    edit: false, download: true, print: false, comment: false, review: false, chat: false,
  });
  const document = __ctoxForkTestHooks.documentConfig('doc', { filename: 'fixture.docx' }, {
    write: true, export: true, comment: true, review: true,
  }, 'document');
  assert.deepEqual(document.permissions, {
    edit: true, download: true, print: false, comment: true, review: true, chat: false,
  });
});

test('spreadsheet visual harness fails closed unless pinned Oracle provenance is present', async () => {
  const sideBySide = await readFile(new URL('./oracle/side-by-side.html', import.meta.url), 'utf8');
  const ctoxHarness = await readFile(new URL('./oracle/ctox-spreadsheet-open-render-sheets.html', import.meta.url), 'utf8');
  assert.match(sideBySide, /ctox-runtime-not-fork/);
  assert.match(sideBySide, /ctox-fork-closure-unproven/);
  assert.match(sideBySide, /ctox-editor-payload-not-rust-generated/);
  assert.match(sideBySide, /configuration-mismatch/);
  assert.match(sideBySide, /geometry-mismatch/);
  assert.match(sideBySide, /visual-review-not-passed/);
  assert.match(ctoxHarness, /ctox-spreadsheets-fork/);
  assert.match(ctoxHarness, /XLSY;v10/);
  assert.match(ctoxHarness, /ctox-rust\.Editor\.bin/);
  assert.match(ctoxHarness, /rust_generated:\s*true/);
  assert.doesNotMatch(ctoxHarness, /open-render-sheets\.xlsx.*editorBytes/s);
});

test('spreadsheet open render uses the CTOX Spreadsheets status-bar tabs and native XLSY', async () => {
  const flow = await readFile(new URL('./oracle/flows/spreadsheet.open-render-sheets.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.open-render-sheets.json', import.meta.url), 'utf8'));
  assert.match(flow, /ctoxOfficeComparison\.validate\(\)/);
  assert.match(flow, /spreadsheeteditor\/main\/index\.html/);
  assert.match(flow, /#statusbar_bottom \[data-label="Details"\]/);
  assert.match(flow, /asc_getWorksheetsCount/);
  assert.match(flow, /asc_isWorksheetHidden/);
  assert.match(flow, /active_sheet !== 'Overview'/);
  assert.match(flow, /active_sheet !== 'Details'/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.initial_semantic_parity.archive_hidden, true);
  assert.equal(evidence.latest_side_by_side_attempt.terminal_semantic_parity.active_sheet, 'Details');
  assert.equal(evidence.latest_side_by_side_attempt.native_writer.protocol, 'euro-office-cell-binary-v10');
  assert.equal(evidence.latest_side_by_side_attempt.no_change_export.ctox_reopen, 'passed');
  assert.equal(evidence.latest_side_by_side_attempt.no_change_export.oracle_reopen, 'passed');
  assert.equal(evidence.latest_side_by_side_attempt.remaining_gate, 'None for spreadsheet.open-render-sheets.');
});

test('spreadsheet formatting flow preserves unrelated styles and requires Business OS commit evidence', async () => {
  const flow = await readFile(new URL('./oracle/flows/spreadsheet.cell-format-rows-columns.playwright.js', import.meta.url), 'utf8');
  const businessOsFlow = await readFile(new URL('./oracle/flows/spreadsheet.cell-format-rows-columns.business-os.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/spreadsheet.cell-format-rows-columns.json', import.meta.url), 'utf8'));
  assert.match(flow, /A1 header style changed unintentionally/);
  assert.match(flow, /Buchhaltungsformat/);
  assert.match(flow, /Benutzerdefinierte Zeilenhöhe/);
  assert.match(businessOsFlow, /modules\/spreadsheets\.mount\(ctx\)/);
  assert.match(businessOsFlow, /office\.spreadsheet\.commit/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.cell_format.unrelated_A1_unchanged, true);
  assert.equal(evidence.browser_health.console_errors, 0);
});

test('document visual harness records the reviewed Rust payload', async () => {
  const ctoxHarness = await readFile(new URL('./oracle/ctox-document-open-render-zoom.html', import.meta.url), 'utf8');
  assert.match(ctoxHarness, /ctox-office-document\.mjs/);
  assert.match(ctoxHarness, /ctox-rust\.Editor\.bin/);
  assert.match(ctoxHarness, /rust_generated:\s*true/);
  assert.match(ctoxHarness, /visual_review_passed:\s*true/);
});

test('document open/render differential uses the real toolbar and keyboard flow', async () => {
  const flow = await readFile(new URL('./oracle/flows/document.open-render-zoom.playwright.js', import.meta.url), 'utf8');
  assert.match(flow, /getByRole\('button', \{ name: 'Vergrößern \(⌘\+=\)'/);
  assert.match(flow, /keyboard\.press\('PageDown'\)/);
  assert.match(flow, /Seite 2 von 3/);
  assert.match(flow, /Seite 3 von 3/);
  assert.doesNotMatch(flow, /goToPage/);
});

test('document edit/save differential uses measured geometry and the CTOX Documents toolbar save', async () => {
  const harness = await readFile(new URL('./oracle/ctox-document-edit-save.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/document.edit-save.playwright.js', import.meta.url), 'utf8');
  const runtime = await readFile(new URL('./src/runtime/ctox-fork-core.mjs', import.meta.url), 'utf8');
  assert.match(harness, /ctox-office-document\.mjs/);
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /rust_generated:\s*true/);
  assert.match(harness, /visual_review_passed:\s*true/);
  assert.match(flow, /boundingBox\(\)/);
  assert.match(flow, /box\.width \* 0\.36/);
  assert.match(flow, /keyboard\.press\('Shift\+Home'\)/);
  assert.match(flow, /Speichern \(⌘\+S\)/);
  assert.match(flow, /editor\.inspect\(\)\)\.dirty === false/);
  assert.match(flow, /state\/document\.edit-save/);
  assert.match(runtime, /asc_nativeGetFile2\(\)/);
  assert.doesNotMatch(runtime, /fetch\([^\n]*downloadas/);
});

test('document undo/clipboard differential uses CTOX Documents and complete font closure', async () => {
  const harness = await readFile(new URL('./oracle/ctox-document-undo-clipboard-keyboard.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/document.undo-clipboard-keyboard.playwright.js', import.meta.url), 'utf8');
  const provenance = JSON.parse(await readFile(new URL('../vendor/ctox-office/provenance.json', import.meta.url), 'utf8'));
  assert.match(harness, /ctox-office-document\.mjs/);
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /rust_generated:\s*true/);
  assert.match(harness, /visual_review_passed:\s*true/);
  assert.match(flow, /keyboard\.press\('Meta\+Z'\)/);
  assert.match(flow, /keyboard\.press\('Meta\+Y'\)/);
  assert.match(flow, /Rückgängig \(⌘\+Z\)/);
  assert.match(flow, /Wiederholen \(⌘\+Y\)/);
  assert.match(flow, /keyboard\.press\('Meta\+C'\)/);
  assert.match(flow, /keyboard\.press\('Meta\+X'\)/);
  assert.match(flow, /keyboard\.press\('Meta\+V'\)/);
  assert.ok(provenance.upstream_static_inputs.some(({ path, sha256 }) =>
    path === 'fonts/104' && sha256 === '1ccd17b3a3a63bb8ac4dd49fd3dd45fc549bcc9cec08d02e6af59c8e3ab82fba'));
  assert.ok(provenance.upstream_static_inputs.some(({ path, sha256 }) =>
    path === 'fonts/105' && sha256 === '0e051194266362ea5280e62f98a05c31e63dd7a19cc3940c61b358757c0d388e'));
});

test('document character/paragraph formatting differential uses CTOX Documents toolbar controls', async () => {
  const harness = await readFile(new URL('./oracle/ctox-document-character-paragraph-formatting.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/document.character-paragraph-formatting.playwright.js', import.meta.url), 'utf8');
  assert.match(harness, /ctox-office-document\.mjs/);
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /rust_generated:\s*true/);
  assert.match(harness, /visual_review_passed:\s*true/);
  assert.match(flow, /Fett \(⌘\+B\)/);
  assert.match(flow, /Kursiv \(⌘\+I\)/);
  assert.match(flow, /Unterstrichen \(⌘\+U\)/);
  assert.match(flow, /input\[aria-label="Schriftgrad"\]/);
  assert.match(flow, /palette-color-effect\.color-953735/);
  assert.match(flow, /Zentriert ausrichten \(⌘\+E\)/);
  assert.match(flow, /Einzug vergrößern \(⌘\+M\)/);
  assert.match(flow, /name: 'Zeilenabstand'/);
  assert.match(flow, /getByText\('1\.5'/);
  assert.match(flow, /bottom-up/);
});

test('document styles/lists differential uses the CTOX Documents gallery and numbering menus', async () => {
  const harness = await readFile(new URL('./oracle/ctox-document-styles-lists-numbering.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/document.styles-lists-numbering.playwright.js', import.meta.url), 'utf8');
  assert.match(harness, /ctox-office-document\.mjs/);
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /rust_generated:\s*true/);
  assert.match(harness, /visual_review_passed:\s*true/);
  assert.match(flow, /documentApi\?\.Search\?\.\(target\)\?\.\[0\]/);
  assert.match(flow, /GetAllParagraphs\?\.\(\)\?\.\[0\]/);
  assert.match(flow, /selectMarkerParagraph\(frame, 'STYLE_HEADING1_TARGET'\)/);
  assert.match(flow, /selectMarkerParagraph\(frame, 'STYLE_QUOTE_TARGET'\)/);
  assert.match(flow, /styleGalleryIndexes/);
  assert.match(flow, /menu-picker:visible \.style/);
  assert.match(flow, /boundingBox\(\)/);
  assert.match(flow, /page\.mouse\.click\(box\.x \+ box\.width \/ 2, box\.y \+ box\.height \/ 2\)/);
  assert.match(flow, /'Überschrift 1'/);
  assert.match(flow, /'Zitat'/);
  assert.match(flow, /Einzug vergrößern \(⌘\+M\)/);
  assert.match(flow, /item-multilevellist:visible/);
  assert.match(flow, /Nummerierung fortführen/);
  assert.match(flow, /keyboard\.press\('ArrowRight'\)/);
  assert.match(flow, /locator\('#id_viewer_overlay'\)\.press\('ContextMenu'\)/);
  assert.match(flow, /selectMarker\(frame, 'NUMBER_CONTINUE_TARGET'\)/);
  assert.doesNotMatch(flow, /continuationRatio/);
  assert.doesNotMatch(flow, /markerPositions/);
  assert.doesNotMatch(flow, /mouse\.dblclick/);
  assert.match(flow, /capture-ctox\/document\.styles-lists-numbering/);
  assert.doesNotMatch(flow, /asc_[A-Za-z]/);
});

test('document tables flow uses CTOX Documents and requires Oracle save evidence', async () => {
  const harness = await readFile(new URL('./oracle/ctox-document-tables.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/document.tables.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/document.tables.json', import.meta.url), 'utf8'));
  assert.match(harness, /ctox-office-document\.mjs/);
  assert.match(harness, /ctox-rust\.Editor\.bin/);
  assert.match(harness, /ctox-saved\.Editor\.bin/);
  assert.match(harness, /rust_generated:\s*true/);
  assert.match(harness, /visual_review_passed:\s*true/);
  assert.match(flow, /ctoxOfficeComparison\.validate\(\)/);
  assert.match(flow, /web-apps\/apps\/documenteditor\/main\/index\.html/);
  assert.match(flow, /vendor\/ctox-office\/upstream\/web-apps\/apps\/documenteditor\/main\/index\.html/);
  assert.match(flow, /GetAllTables\(\)/);
  assert.match(flow, /ContextMenu|button: 'right'/);
  assert.match(flow, /Zeile unterhalb/);
  assert.match(flow, /Spalte nach rechts/);
  assert.match(flow, /keyboard\.type\('TABLE_EDITED_VALUE'\)/);
  assert.match(flow, /MergeCells/);
  assert.match(flow, /Split/);
  assert.match(flow, /TABLE_EDITED_VALUE/);
  assert.match(flow, /NESTED_A1/);
  assert.match(flow, /NESTED_B2/);
  assert.match(flow, /Speichern \(⌘\+S\)/);
  assert.match(flow, /capture-ctox\/document\.tables/);
  assert.match(flow, /state\/document\.tables/);
  assert.match(flow, /Oracle terminal save callback missing/);
  assert.doesNotMatch(flow, /innerHTML\s*=/);
  assert.doesNotMatch(flow, /<table/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.remaining_gate, 'none for document.tables');
  assert.equal(evidence.ctox.canonical_export.oracle_reopen, 'passed');
});

test('document images positioning uses CTOX/Oracle split-screen UI and has differential evidence', async () => {
  const oracleHarness = await readFile(new URL('./oracle/document-images-positioning.html', import.meta.url), 'utf8');
  const ctoxHarness = await readFile(new URL('./oracle/ctox-document-images-positioning.html', import.meta.url), 'utf8');
  const sideBySide = await readFile(new URL('./oracle/side-by-side.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/document.images-positioning.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/document.images-positioning.json', import.meta.url), 'utf8'));
  assert.match(oracleHarness, /comparison_config/);
  assert.match(ctoxHarness, /ctox-office-document\.mjs/);
  assert.match(ctoxHarness, /ctox-rust\.Editor\.bin/);
  assert.match(ctoxHarness, /ctox-saved\.Editor\.bin/);
  assert.match(ctoxHarness, /rust_generated:\s*true/);
  assert.match(ctoxHarness, /visual_review_passed:\s*true/);
  assert.match(sideBySide, /value === 'document-ready' \? 'ready'/);
  assert.match(sideBySide, /statusFor\(frames\.oracle\) !== 'document-ready'/);
  assert.doesNotMatch(sideBySide, /\/ready\/i/);
  assert.match(flow, /ctoxOfficeComparison\.validate\(\)/);
  assert.match(flow, /web-apps\/apps\/documenteditor\/main\/index\.html/);
  assert.match(flow, /vendor\/ctox-office\/upstream\/web-apps\/apps\/documenteditor\/main\/index\.html/);
  assert.match(flow, /ImgApply\(image\)/);
  assert.match(flow, /Asc\.c_oAscWrapStyle2\.Square/);
  assert.match(flow, /capture-ctox\/document\.images-positioning/);
  assert.doesNotMatch(flow, /innerHTML\s*=/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.status, 'differential_passed');
  assert.match(evidence.latest_side_by_side_attempt.finding, /full split-screen flow now runs/);
  assert.match(evidence.latest_side_by_side_attempt.root_cause, /extracts word\/media\/\*/);
  assert.match(evidence.latest_side_by_side_attempt.root_cause, /accepts only the exact document-ready state/);
  assert.equal(evidence.latest_side_by_side_attempt.ctox_only_pptxdata_smoke.status, 'failed');
  assert.equal(evidence.latest_side_by_side_attempt.ctox_only_media_resolver_smoke.status, 'passed');
  assert.equal(evidence.latest_side_by_side_attempt.ctox_only_media_resolver_smoke.console_errors, 0);
  assert.deepEqual(evidence.latest_side_by_side_attempt.ctox_only_media_resolver_smoke.media_http_requests, []);
  assert.match(evidence.latest_side_by_side_attempt.ctox_only_media_resolver_smoke.result, /visibly rendered the inline blue image/);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.status, 'passed');
  assert.equal(evidence.ctox.canonical_export.ctox_clean_profile_reopen, 'passed');
  assert.equal(evidence.ctox.canonical_export.oracle_reopen, 'passed');
  assert.deepEqual(evidence.ctox.canonical_export.changed_parts, ['word/document.xml']);
  assert.equal(evidence.latest_side_by_side_attempt.remaining_gate, 'none for document.images-positioning');
});

test('document sections headers footers keeps full differential gate after HdrFtr render slice', async () => {
  const oracleHarness = await readFile(new URL('./oracle/document-sections-headers-footers.html', import.meta.url), 'utf8');
  const ctoxHarness = await readFile(new URL('./oracle/ctox-document-sections-headers-footers.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/document.sections-headers-footers.json', import.meta.url), 'utf8');
  const browserFlow = await readFile(new URL('./oracle/flows/document.sections-headers-footers.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/document.sections-headers-footers.json', import.meta.url), 'utf8'));
  assert.match(oracleHarness, /comparison_config/);
  assert.match(oracleHarness, /ctox-local-user/);
  assert.match(ctoxHarness, /ctox-office-document\.mjs/);
  assert.match(ctoxHarness, /ctox-rust\.Editor\.bin/);
  assert.match(ctoxHarness, /ctox-saved\.Editor\.bin/);
  assert.match(ctoxHarness, /canonicalBytes:\s*canonicalBytes\.slice\(\), editorBytes:\s*savedBytes\.slice\(\)/);
  assert.match(ctoxHarness, /rust_generated:\s*true/);
  assert.match(flow, /side-by-side\.html\?feature=document\.sections-headers-footers/);
  assert.match(flow, /document\.sections-headers-footers\.playwright\.js/);
  assert.match(flow, /assert_initial_layout/);
  assert.match(browserFlow, /GetSections/);
  assert.match(browserFlow, /HEADER_SECTION1_FIRST/);
  assert.match(browserFlow, /FOOTER_SECTION1_DEFAULT/);
  assert.match(browserFlow, /GoToPage/);
  assert.match(browserFlow, /SetPageSize/);
  assert.match(browserFlow, /SetHeaderDistance/);
  assert.match(browserFlow, /SetFooterDistance/);
  assert.match(browserFlow, /SECTION2_PAGE_SETUP_TARGET/);
  assert.match(browserFlow, /add_SectionBreak/);
  assert.match(browserFlow, /GetCurrentParagraph/);
  assert.match(browserFlow, /HeadersAndFooters_LinkToPrevious/);
  assert.match(browserFlow, /SECTION_HDRFTR_SAVE_MARKER/);
  assert.match(browserFlow, /capture-ctox\/document\.sections-headers-footers/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.status, 'differential_passed');
  assert.match(evidence.latest_side_by_side_attempt.gate, /passed for exact document-ready/);
  assert.match(evidence.latest_side_by_side_attempt.implemented_slice, /pPr\.SectPr/);
  assert.match(evidence.latest_side_by_side_attempt.implemented_slice, /HdrFtr table/);
  assert.match(evidence.latest_side_by_side_attempt.implemented_slice, /link-to-previous/);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.status, 'passed');
  assert.deepEqual(evidence.latest_side_by_side_attempt.differential_flow.terminal_pages, { oracle: 'Seite 3 von 3', ctox: 'Seite 3 von 3' });
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.ctox_reopen.status, 'document-ready');
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.oracle_reopen.status, 'document-ready');
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.section_edit.ctox.width_twips, 15840);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.section_edit.ctox.header_distance_twips, 850);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.section_break.ctox.sections_count, 3);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.section_break.ctox.inserted_type, 'nextPage');
  assert.deepEqual(evidence.latest_side_by_side_attempt.differential_flow.ctox_export.added_parts, ['word/header3.xml']);
  assert.match(evidence.latest_side_by_side_attempt.differential_flow.ctox_export.changed_parts.join(','), /word\/_rels\/document\.xml\.rels/);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.ctox_export.section_assertions.sectPr_count, 3);
  assert.match(evidence.latest_side_by_side_attempt.differential_flow.ctox_export.section_assertions.paragraph_section_pgSz, /landscape/);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.ctox_export.section_assertions.inserted_section_break_type, 'nextPage');
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.ctox_export.section_assertions.header_footer_refs_preserved, true);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.ctox_export.section_assertions.link_to_previous_false_header_materialized, true);
  assert.match(evidence.latest_side_by_side_attempt.finding, /HeadersAndFooters_LinkToPrevious\(false\)/);
  assert.match(evidence.latest_side_by_side_attempt.finding, /add_SectionBreak/);
  assert.equal(evidence.latest_side_by_side_attempt.required_next_work, 'None for document.sections-headers-footers; proceed to document.links-bookmarks-fields.');
});

test('document links bookmarks fields uses CTOX/Oracle split-screen UI and native DOCY records', async () => {
  const oracleHarness = await readFile(new URL('./oracle/document-links-bookmarks-fields.html', import.meta.url), 'utf8');
  const ctoxHarness = await readFile(new URL('./oracle/ctox-document-links-bookmarks-fields.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/document.links-bookmarks-fields.json', import.meta.url), 'utf8');
  const browserFlow = await readFile(new URL('./oracle/flows/document.links-bookmarks-fields.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/document.links-bookmarks-fields.json', import.meta.url), 'utf8'));
  assert.match(oracleHarness, /comparison_config/);
  assert.match(oracleHarness, /oracleRun/);
  assert.match(ctoxHarness, /ctox-office-document\.mjs/);
  assert.match(ctoxHarness, /ctox-rust\.Editor\.bin/);
  assert.match(ctoxHarness, /ctox-saved\.Editor\.bin/);
  assert.match(ctoxHarness, /rust_generated:\s*true/);
  assert.match(flow, /side-by-side\.html\?feature=document\.links-bookmarks-fields/);
  assert.match(flow, /document\.links-bookmarks-fields\.playwright\.js/);
  assert.match(browserFlow, /ctoxOfficeComparison\.validate\(\)/);
  assert.match(browserFlow, /CHyperlinkProperty/);
  assert.match(browserFlow, /add_Hyperlink/);
  assert.match(browserFlow, /asc_GetBookmarksManager/);
  assert.match(browserFlow, /asc_AddBookmark/);
  assert.match(browserFlow, /UpdateAllFields/);
  assert.match(browserFlow, /capture-ctox\/document\.links-bookmarks-fields/);
  assert.match(browserFlow, /Speichern \(⌘\+S\)/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.status, 'passed');
  assert.deepEqual(evidence.latest_side_by_side_attempt.differential_flow.ctox_export.changed_parts,
    ['word/_rels/document.xml.rels', 'word/document.xml']);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.ctox_export.custom_xml_escrow_preserved, true);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.ctox_reopen.status, 'document-ready');
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.oracle_reopen.status, 'document-ready');
  assert.equal(evidence.latest_side_by_side_attempt.required_next_work,
    'None for document.links-bookmarks-fields; proceed to document.comments-track-changes.');
});

test('document comments track changes uses CTOX Documents Review UI and passes the native roundtrip gate', async () => {
  const oracleHarness = await readFile(new URL('./oracle/document-comments-track-changes.html', import.meta.url), 'utf8');
  const ctoxHarness = await readFile(new URL('./oracle/ctox-document-comments-track-changes.html', import.meta.url), 'utf8');
  const browserFlow = await readFile(new URL('./oracle/flows/document.comments-track-changes.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/document.comments-track-changes.json', import.meta.url), 'utf8'));
  assert.match(oracleHarness, /comparison_config/);
  assert.match(oracleHarness, /oracleRun/);
  assert.match(ctoxHarness, /ctox-office-document\.mjs/);
  assert.match(ctoxHarness, /ctox-rust\.Editor\.bin/);
  assert.match(ctoxHarness, /ctox-saved\.Editor\.bin/);
  assert.match(ctoxHarness, /rust_generated:\s*true/);
  assert.match(browserFlow, /pluginMethod_AddComment/);
  assert.match(browserFlow, /pluginMethod_ChangeComment/);
  assert.match(browserFlow, /Nachverfolgen von Änderungen/);
  assert.match(browserFlow, /page\.keyboard\.type/);
  assert.match(browserFlow, /pluginMethod_RejectReviewChanges/);
  assert.match(browserFlow, /pluginMethod_AcceptReviewChanges/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.initial_semantic_parity.status, 'passed');
  assert.equal(evidence.latest_side_by_side_attempt.browser_flow.status, 'passed');
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.status, 'passed');
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.ctox_export.custom_xml_escrow_preserved, true);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.ctox_reopen.revision_stack, 1);
  assert.equal(evidence.latest_side_by_side_attempt.differential_flow.oracle_reopen.has_revisions, true);
  assert.equal(evidence.latest_side_by_side_attempt.remaining_gate, 'None for document.comments-track-changes.');
});

test('document drawings charts uses CTOX Documents object APIs and native DrawingML roundtrip', async () => {
  const oracleHarness = await readFile(new URL('./oracle/document-drawings-charts.html', import.meta.url), 'utf8');
  const ctoxHarness = await readFile(new URL('./oracle/ctox-document-drawings-charts.html', import.meta.url), 'utf8');
  const flow = await readFile(new URL('./oracle/flows/document.drawings-charts.json', import.meta.url), 'utf8');
  const browserFlow = await readFile(new URL('./oracle/flows/document.drawings-charts.playwright.js', import.meta.url), 'utf8');
  const evidence = JSON.parse(await readFile(new URL('./oracle/evidence/document.drawings-charts.json', import.meta.url), 'utf8'));
  assert.match(oracleHarness, /comparison_config/);
  assert.match(oracleHarness, /source === 'saved'/);
  assert.match(oracleHarness, /saved-ctox\/document\.drawings-charts\.docx/);
  assert.match(ctoxHarness, /ctox-office-document\.mjs/);
  assert.match(ctoxHarness, /ctox-rust\.Editor\.bin/);
  assert.match(ctoxHarness, /ctox-saved\.Editor\.bin/);
  assert.match(ctoxHarness, /ctox-export\.Editor\.bin/);
  assert.match(ctoxHarness, /rust_generated:\s*true/);
  assert.match(flow, /side-by-side\.html\?feature=document\.drawings-charts/);
  assert.match(browserFlow, /ctoxOfficeComparison\.validate\(\)/);
  assert.match(browserFlow, /asc_CShapeProperty/);
  assert.match(browserFlow, /asc_CShapeFill/);
  assert.match(browserFlow, /asc_putRot\(Math\.PI \/ 2\)/);
  assert.match(browserFlow, /put_ChartProperties/);
  assert.match(browserFlow, /capture-ctox\/document\.drawings-charts/);
  assert.match(browserFlow, /Speichern \(⌘\+S\)/);
  assert.equal(evidence.status, 'differential_passed');
  assert.equal(evidence.ctox.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.status, 'differential_passed');
  assert.equal(evidence.latest_side_by_side_attempt.browser_flow.status, 'passed');
  assert.equal(evidence.latest_side_by_side_attempt.native_roundtrip.document_xml.shape_rotation_units, 5400000);
  assert.equal(evidence.latest_side_by_side_attempt.native_roundtrip.chart_xml.built_in_style, 2);
  assert.equal(evidence.latest_side_by_side_attempt.native_roundtrip.embedded_workbook_byte_preserved, true);
  assert.deepEqual(evidence.ctox.canonical_export.changed_understood_parts,
    ['word/charts/chart1.xml', 'word/document.xml']);
  assert.deepEqual(evidence.ctox.canonical_export.missing_original_parts, []);
  assert.equal(evidence.latest_side_by_side_attempt.reopen.saved_docy_split_screen, 'passed');
  assert.equal(evidence.latest_side_by_side_attempt.reopen.ctox_export_in_oracle, 'passed');
  assert.equal(evidence.latest_side_by_side_attempt.remaining_gate, 'None for document.drawings-charts.');
});

function storedZip(entries) {
  const encoder = new TextEncoder();
  const chunks = [];
  for (const [name, value] of entries) {
    const nameBytes = encoder.encode(name);
    const data = typeof value === 'string' ? encoder.encode(value) : value;
    const header = new Uint8Array(30);
    writeU32(header, 0, 0x04034b50);
    writeU16(header, 4, 20);
    writeU16(header, 8, 0);
    writeU32(header, 18, data.byteLength);
    writeU32(header, 22, data.byteLength);
    writeU16(header, 26, nameBytes.byteLength);
    chunks.push(header, nameBytes, data);
  }
  const total = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0);
  const output = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    output.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return output;
}

function writeU16(bytes, offset, value) {
  bytes[offset] = value & 0xff;
  bytes[offset + 1] = (value >>> 8) & 0xff;
}

function writeU32(bytes, offset, value) {
  bytes[offset] = value & 0xff;
  bytes[offset + 1] = (value >>> 8) & 0xff;
  bytes[offset + 2] = (value >>> 16) & 0xff;
  bytes[offset + 3] = (value >>> 24) & 0xff;
}

async function sha256File(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}
