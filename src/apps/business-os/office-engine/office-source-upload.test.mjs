import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
import { createBusinessOsOfficeBridge } from './src/business-os-bridge.mjs';

function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

function fixture(kind, { upload, writable = true, cancelled = false, missingApi = false } = {}) {
  const events = [];
  const stored = [];
  const state = {
    cancelled,
    async waitForOpenPeerId() { events.push('peer'); return 'native'; },
    async pushDocumentsToPeer(peer, rows) {
      assert.equal(peer, 'native');
      assert.deepEqual(rows, stored);
      events.push('upload');
      const result = await upload?.(rows);
      events.push('ack');
      return result;
    },
    async pushToPeer() { assert.fail('source upload must not sweep unrelated blobs'); },
  };
  if (missingApi) delete state.pushDocumentsToPeer;
  const ctx = {
    permissions: { canWriteCollection: () => writable },
    db: { collection(name) {
      if (name !== `${kind}_blob_chunks`) return {};
      return { async bulkUpsert(rows) {
        events.push('stage');
        const persisted = rows.map(row => ({ ...row, _meta: { lwt: Date.now(), marker: 'stored' } }));
        stored.push(...persisted);
        return persisted.map(row => ({ toJSON: () => structuredClone(row) }));
      } };
    } },
    sync: { async leaseCollection(name) {
      assert.equal(name, `${kind}_blob_chunks`);
      events.push('lease');
      return { bridge: { state }, async release() { events.push('release'); } };
    } },
  };
  return { ctx, events, stored, bridge: createBusinessOsOfficeBridge(ctx, kind) };
}

const input = (bytes = new Uint8Array([0, 255, 16])) => ({
  recordId: 'owned-record', versionId: 'owned-v1', blobId: 'owned-original-blob', mimeType: 'text/csv', bytes,
});

for (const kind of ['document', 'spreadsheet']) {
  test(`${kind}: source publication waits for exact native acknowledgement`, async () => {
    const entered = deferred();
    const ack = deferred();
    const f = fixture(kind, { upload: async () => { entered.resolve(); await ack.promise; } });
    let completed = false;
    const pending = f.bridge.stageSourceBlob(input()).then(() => { completed = true; });
    try {
      await entered.promise;
      assert.equal(completed, false);
      assert.deepEqual(f.events, ['lease', 'stage', 'peer', 'upload']);
      ack.resolve();
      await pending;
      assert.deepEqual(f.events, ['lease', 'stage', 'peer', 'upload', 'ack', 'release']);
    } finally { ack.resolve(); await pending; }
  });

  for (const mime of ['text/csv', 'text/tab-separated-values', 'text/markdown', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet']) {
    test(`${kind}: original source MIME and multi-chunk bytes preserved: ${mime}`, async () => {
      const f = fixture(kind);
      const bytes = Uint8Array.from({ length: 512017 }, (_, i) => i % 251);
      await f.bridge.stageSourceBlob({ ...input(bytes), mimeType: mime });
      assert.equal(f.stored.length, 3);
      for (const [idx, row] of f.stored.entries()) {
        assert.equal(row.blob_id, 'owned-original-blob');
        assert.equal(row[`${kind}_id`], 'owned-record');
        assert.equal(row.version_id, 'owned-v1');
        assert.equal(row.idx, idx);
        assert.equal(row.total, 3);
        assert.equal(row.mime_type, mime);
        assert.equal(row._meta.marker, 'stored');
        assert.ok(Buffer.byteLength(JSON.stringify(row)) < 262144);
      }
      assert.deepEqual(Buffer.concat(f.stored.map(row => Buffer.from(row.data, 'base64'))), Buffer.from(bytes));
    });
  }

  test(`${kind}: valid empty source is acknowledged as one empty chunk`, async () => {
    const f = fixture(kind);
    await f.bridge.stageSourceBlob(input(new Uint8Array()));
    assert.equal(f.stored.length, 1);
    assert.equal(f.stored[0].data, '');
    assert.ok(f.events.includes('ack'));
  });

  test(`${kind}: write denial occurs before lease or persistence`, async () => {
    const f = fixture(kind, { writable: false });
    await assert.rejects(() => f.bridge.stageSourceBlob(input()), error => error.code === 'permission_denied');
    assert.deepEqual(f.events, []);
  });

  test(`${kind}: invalid IDs/MIME/bytes rejected before persistence`, async () => {
    for (const field of ['recordId', 'versionId', 'blobId', 'mimeType']) {
      for (const value of ['', '  ', null, 42]) {
        const f = fixture(kind);
        await assert.rejects(() => f.bridge.stageSourceBlob({ ...input(), [field]: value }));
        assert.deepEqual(f.events, []);
      }
    }
    for (const bytes of [undefined, null, 'csv', []]) {
      const f = fixture(kind);
      await assert.rejects(() => f.bridge.stageSourceBlob({ ...input(), bytes }));
      assert.deepEqual(f.events, []);
    }
  });

  test(`${kind}: upload rejection keeps identity and releases lease`, async () => {
    const failure = Object.assign(new Error('native permission denied'), { code: 'permission_denied' });
    const f = fixture(kind, { upload: async () => { throw failure; } });
    await assert.rejects(() => f.bridge.stageSourceBlob(input()), error => error === failure);
    assert.equal(f.events.at(-1), 'release');
    assert.ok(!f.events.includes('ack'));
  });

  for (const options of [{ cancelled: true }, { missingApi: true }]) {
    test(`${kind}: unavailable targeted transport fails closed ${JSON.stringify(options)}`, async () => {
      const f = fixture(kind, options);
      await assert.rejects(() => f.bridge.stageSourceBlob(input()), /sync push unavailable/);
      assert.equal(f.events.at(-1), 'release');
      assert.ok(!f.events.includes('upload'));
    });
  }

  test(`${kind}: actual module source-save helper awaits native acknowledgement`, async () => {
    const moduleName = kind === 'document' ? 'documents' : 'spreadsheets';
    const result = await build({
      entryPoints: [fileURLToPath(new URL(`../modules/${moduleName}/index.js`, import.meta.url))],
      bundle: true, format: 'esm', platform: 'browser', write: false,
    });
    const mod = await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`);
    const hooks = mod[kind === 'document' ? '__documentsTestHooks' : '__spreadsheetsTestHooks'];
    const entered = deferred();
    const ack = deferred();
    const f = fixture(kind, { upload: async () => { entered.resolve(); await ack.promise; } });
    let completed = false;
    const pending = hooks.saveBlobChunks(f.ctx, {
      ...input(), [kind === 'document' ? 'documentId' : 'spreadsheetId']: 'owned-record',
    }).then(() => { completed = true; });
    try {
      const first = await Promise.race([entered.promise.then(() => 'upload'), pending.then(() => 'completed')]);
      assert.equal(first, 'upload', 'module must not expose metadata before upload');
      assert.equal(completed, false);
      ack.resolve();
      await pending;
      assert.equal(f.events.at(-1), 'release');
    } finally { ack.resolve(); await pending; }
  });
}
