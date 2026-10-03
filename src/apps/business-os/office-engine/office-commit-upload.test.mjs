import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test as nodeTest } from 'node:test';
import { createBusinessOsOfficeBridge } from './src/business-os-bridge.mjs';
import { createRxDatabase } from '../rxdb/src/rx-database.mjs';

for (const kind of ['document', 'spreadsheet']) {
  nodeTest(`${kind}: commit preserves the real CTOX collection's storage result`, async () => {
    const persisted = new Map();
    const collectionName = `${kind}_blob_chunks`;
    const db = await createRxDatabase({
      name: `office-upload-contract-${kind}`,
      storage: { nativeStorage: {
        collection(name) {
          assert.equal(name, collectionName);
          return { async bulkUpsert(rows) {
            const success = {};
            for (const row of rows) {
              assert.ok(row._meta.lwt > 0, 'CTOX collection normalizes storage metadata');
              const stored = { ...row, _meta: { ...row._meta, lwt: row._meta.lwt + 1 } };
              persisted.set(row.id, structuredClone(stored));
              success[row.id] = stored;
            }
            return { success, error: {} };
          } };
        },
        close() {},
      } },
    });
    await db.addCollections({ [collectionName]: { schema: {
      version: 0, primaryKey: 'id', type: 'object', properties: { id: { type: 'string' } },
    } } });
    let acknowledged = false;
    let released = false;
    const bytes = new Uint8Array(512017).fill(37);
    const bridge = createBusinessOsOfficeBridge({
      db,
      permissions: { canWriteCollection: () => true },
      sync: { async leaseCollection() {
        return {
          bridge: { state: {
            async waitForOpenPeerId() { return 'native'; },
            async pushDocumentsToPeer(peer, rows) {
              assert.equal(peer, 'native');
              assert.equal(rows.length, 3);
              assert.deepEqual(rows, [...persisted.values()]);
              assert.deepEqual(Buffer.concat(rows.map(row => Buffer.from(row.data, 'base64'))), Buffer.from(bytes));
              acknowledged = true;
            },
          } },
          async release() { released = true; },
        };
      } },
      commandBus: { async dispatch(command) {
        assert.equal(acknowledged, true);
        assert.equal(command.type, `office.${kind}.commit`);
        return { status: 'completed', result: { ok: true, version_id: 'v2' } };
      } },
    }, kind);
    try {
      assert.equal((await bridge.commit({ recordId: 'owned', baseVersionId: 'v1', bytes })).version_id, 'v2');
      assert.equal(released, true);
    } finally { await db.close(); }
  });
}

function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}

function fixture(kind, transport, startCollection, path = 'bulk', transform = (docs) => docs) {
  const events = [];
  const staged = [];
  const persisted = [];
  const commands = [];
  const store = (rows) => {
    staged.push(...rows);
    const docs = rows.map((row) => {
      const json = {
        ...row, _rev: `1-stored-${row.idx}`,
        _meta: { lwt: 1700000000000 + row.idx, stored_marker: 'retained' },
      };
      persisted.push(json);
      return { toJSON: () => ({ ...json, _meta: { ...json._meta } }) };
    });
    return transform(docs);
  };
  const chunks = `${kind}_blob_chunks`;
  const lease = { bridge: transport, async release() { events.push('release'); } };
  const ctx = {
    permissions: { canWriteCollection: () => true },
    db: { collection(name) {
      assert.equal(name, chunks);
      return path === 'bulk'
        ? { async bulkUpsert(rows) { events.push('stage'); return store(rows); } }
        : { async incrementalUpsert(row) {
          if (!staged.length) events.push('stage');
          return store([row])?.[0];
        } };
    } },
    sync: {
      async leaseCollection(name) { assert.equal(name, chunks); return lease; },
      async startCollection(name, options) {
        assert.equal(name, chunks);
        assert.deepEqual(options, { pin: false, forceDirect: true });
        events.push('direct');
        return startCollection?.();
      },
    },
    commandBus: { async dispatch(command) {
      events.push('dispatch'); commands.push(command);
      return { status: 'completed', result: { ok: true, version_id: 'v2' } };
    } },
  };
  const bytes = new Uint8Array(512017);
  for (let i = 0; i < bytes.length; i += 1) bytes[i] = i % 251;
  bytes.set(new TextEncoder().encode(kind === 'document' ? 'DOCY;v10;' : 'XLSY;v10;'));
  const commit = () => createBusinessOsOfficeBridge(ctx, kind).commit({
    recordId: 'owned_record', baseVersionId: 'v1', bytes,
    editorProtocol: `office-${kind}`, editorProtocolVersion: 10,
    implementedFeatures: [`${kind}.edit-save`], reason: 'test',
  });
  return { commit, events, staged, persisted, commands, bytes };
}

for (const kind of ['document', 'spreadsheet']) {
  for (const path of ['bulk', 'incremental']) {
  const test = (name, run) => nodeTest(`${path}: ${name}`, run);
  const makeFixture = (transport, startCollection, transform) =>
    fixture(kind, transport, startCollection, path, transform);
  test(`${kind} commit waits for acknowledged peer upload before native dispatch`, async () => {
    const entered = deferred();
    const acknowledged = deferred();
    let f;
    const state = {
      async waitForOpenPeerId(timeout) {
        assert.ok(timeout > 0 && timeout <= 60000); f.events.push('peer'); return 'native';
      },
      async pushToPeer() { assert.fail('unrelated full sweep must not run'); await new Promise(() => {}); },
      async pushDocumentsToPeer(peer, documents) {
        assert.equal(documents.length, 3);
        assert.deepEqual(documents, f.persisted);
        assert.deepEqual(documents.map((row) => row.id), f.staged.map((row) => row.id));
        assert.deepEqual(
          Buffer.concat(documents.map((row) => Buffer.from(row.data, 'base64'))),
          Buffer.from(f.bytes),
        );
        assert.equal(peer, 'native'); f.events.push('upload'); entered.resolve();
        await acknowledged.promise; f.events.push('ack');
      },
      async pushToRemotePeers() { f.events.push('background'); },
      async awaitInSync() { assert.fail('commit must not download the entire collection'); },
    };
    f = makeFixture({ mode: 'leader', state });
    const pending = f.commit();
    try {
      const first = await Promise.race([
        entered.promise.then(() => 'upload'), pending.then(() => 'completed'),
      ]);
      assert.equal(first, 'upload', 'native dispatch must not race ahead of chunk acknowledgement');
      assert.equal(f.commands.length, 0);
      acknowledged.resolve();
      assert.equal((await pending).version_id, 'v2');
      assert.deepEqual(f.events, ['stage', 'peer', 'upload', 'ack', 'dispatch', 'release']);
      assert.equal(f.commands[0].payload.base_version_id, 'v1');
      assert.equal(f.commands[0].payload.editor_sha256, createHash('sha256').update(f.bytes).digest('hex'));
      assert.equal(f.commands[0].payload.editor_blob_id, f.staged[0].blob_id);
    } finally { acknowledged.resolve(); await pending.catch(() => null); }
  });

  test(`${kind} commit propagates failed upload without dispatching a native command`, async () => {
    const failure = Object.assign(new Error('native peer disconnected'), { code: 'sync_unavailable' });
    const f = makeFixture({ mode: 'leader', state: {
      async waitForOpenPeerId() { return 'native'; },
      async pushDocumentsToPeer() { throw failure; },
      async pushToRemotePeers() { /* background sweep may defer failed peers */ },
    } });
    await assert.rejects(f.commit(), (error) => error === failure);
    assert.deepEqual(f.events, ['stage', 'release']);
    assert.equal(f.commands.length, 0);
  });

  for (const mode of ['pending', 'follower', 'missing']) {
    test(`${kind} commit obtains a direct upload bridge from a ${mode} lease`, async () => {
      let f;
      const initial = mode === 'missing' ? null : { mode, state: null };
      f = makeFixture(initial, () => ({ mode: 'leader', state: {
        async waitForOpenPeerId() { return 'native'; },
        async pushDocumentsToPeer() { f.events.push('ack'); },
      } }));
      await f.commit();
      assert.deepEqual(f.events, ['stage', 'direct', 'ack', 'dispatch', 'release']);
      assert.equal(f.commands.length, 1);
    });
  }

  test(`${kind} commit fails closed when no push bridge becomes available`, async () => {
    const f = makeFixture({ mode: 'pending', state: null }, () => null);
    await assert.rejects(f.commit(), /sync push unavailable/);
    assert.equal(f.commands.length, 0);
    assert.deepEqual(f.events, ['stage', 'direct', 'release']);
  });
  test(`${kind} preserves terminal permission rejection identity`, async () => {
    const failure = Object.assign(new Error('permission denied'), { code: 'permission_denied' });
    const f = makeFixture({ state: {
      async waitForOpenPeerId() { return 'native'; },
      async pushDocumentsToPeer() { throw failure; },
      async pushToPeer() { assert.fail('no full sweep'); },
    } });
    await assert.rejects(f.commit(), (error) => error === failure);
    assert.deepEqual(f.events, ['stage', 'release']);
    assert.equal(f.commands.length, 0);
  });

  test(`${kind} accepts native stored metadata without inventing a revision`, async () => {
    let uploaded;
    const f = makeFixture({ state: {
      async waitForOpenPeerId() { return 'native'; },
      async pushDocumentsToPeer(peer, docs) { uploaded = docs; },
    } }, undefined, docs => docs.map(doc => ({ toJSON() {
      const row = doc.toJSON(); delete row._rev; return row;
    } })));
    await f.commit();
    assert.equal(uploaded.length, 3);
    assert.ok(uploaded.every(row => !('_rev' in row) && row._meta.lwt > 0));
    assert.equal(f.commands.length, 1);
  });

  const invalidResults = {
    missing: () => undefined,
    empty: () => [],
    serializer: (docs) => docs.map((doc) => doc.toJSON()),
    nullJSON: () => [{ toJSON: () => null }],
    wrongId: (docs) => docs.map((doc) => ({ toJSON: () => ({ ...doc.toJSON(), id: 'other' }) })),
    payload: (docs) => docs.map((doc) => ({ toJSON: () => ({ ...doc.toJSON(), data: 'changed' }) })),
    metadata: (docs) => docs.map((doc) => ({
      toJSON: () => ({ ...doc.toJSON(), version_id: 'other' }),
    })),
    deleted: (docs) => docs.map((doc) => ({
      toJSON: () => ({ ...doc.toJSON(), _deleted: true }),
    })),
    missingMeta: (docs) => docs.map((doc) => ({
      toJSON: () => ({ ...doc.toJSON(), _meta: undefined }),
    })),
    invalidMeta: (docs) => docs.map((doc) => ({
      toJSON: () => ({ ...doc.toJSON(), _meta: { lwt: NaN } }),
    })),
  };
  if (path === 'bulk') {
    invalidResults.incomplete = (docs) => docs.slice(1);
    invalidResults.duplicate = (docs) => [docs[0], docs[0], docs[2]];
  }
  for (const [label, transform] of Object.entries(invalidResults)) {
    test(`${kind} fails closed on ${label} persisted rows`, async () => {
      const f = makeFixture({ state: {
        async waitForOpenPeerId() { assert.fail('invalid staging must not acquire peer'); },
        async pushDocumentsToPeer() { assert.fail('invalid staging must not upload'); },
        async pushToPeer() { assert.fail('no full sweep'); },
      } }, undefined, transform);
      await assert.rejects(f.commit(), /staged blob rows are invalid/);
      assert.equal(f.commands.length, 0);
      assert.equal(f.events.at(-1), 'release');
    });
  }

  for (const condition of ['missing-api', 'cancelled', 'cancelled-after-peer', 'false-ack']) {
    test(`${kind} fails closed on ${condition}`, async () => {
      let pushes = 0;
      const state = {
        cancelled: condition === 'cancelled',
        async waitForOpenPeerId() {
          if (condition === 'cancelled-after-peer') state.cancelled = true;
          return 'native';
        },
        async pushDocumentsToPeer() { pushes += 1; return false; },
        async pushToPeer() { assert.fail('no full sweep fallback'); },
        async pushToRemotePeers() { assert.fail('no background fallback'); },
        async scheduleLocalWritePush() { assert.fail('no scheduled fallback'); },
      };
      if (condition === 'missing-api') delete state.pushDocumentsToPeer;
      const f = makeFixture({ state });
      await assert.rejects(f.commit(), /sync push unavailable/);
      assert.equal(pushes, condition === 'false-ack' ? 1 : 0);
      assert.equal(f.commands.length, 0);
      assert.equal(f.events.at(-1), 'release');
    });
  }

  test(`${kind} awaits pending direct bridge readiness`, async () => {
    const ready = deferred();
    const acquiring = deferred();
    let f;
    f = makeFixture({ mode: 'follower', state: {
      async pushToPeer() { assert.fail('no follower sweep'); },
    } }, () => {
      acquiring.resolve();
      return { mode: 'pending', ready: ready.promise };
    });
    const pending = f.commit();
    await acquiring.promise;
    assert.equal(f.commands.length, 0);
    ready.resolve({ mode: 'leader', state: {
      async waitForOpenPeerId() { return 'native'; },
      async pushDocumentsToPeer(peer, docs) {
        assert.equal(peer, 'native');
        assert.deepEqual(docs, f.persisted);
        f.events.push('ack');
      },
    } });
    await pending;
    assert.deepEqual(f.events, ['stage', 'direct', 'ack', 'dispatch', 'release']);
  });

  for (const held of ['acquisition', 'ready', 'peer', 'upload']) {
    test(`${kind} bounds ${held} and prevents late dispatch or writes`, async (t) => {
      t.mock.timers.enable({ apis: ['setTimeout', 'Date'] });
      const entered = deferred();
      const late = deferred();
      let pushes = 0;
      const state = {
        async waitForOpenPeerId() {
          if (held === 'peer') { entered.resolve(); return late.promise; }
          return 'native';
        },
        async pushDocumentsToPeer() {
          pushes += 1;
          if (held === 'upload') { entered.resolve(); return late.promise; }
        },
        async pushToPeer() { assert.fail('no full sweep'); },
        async pushToRemotePeers() { assert.fail('no background fallback'); },
      };
      const direct = { mode: 'leader', state };
      const f = makeFixture(
        held === 'acquisition' || held === 'ready' ? { mode: 'pending' } : direct,
        () => {
          entered.resolve();
          return held === 'acquisition' ? late.promise : { ready: late.promise };
        },
      );
      const pending = f.commit();
      const rejected = assert.rejects(pending, (error) => {
        assert.equal(error.code, 'sync_timeout');
        assert.equal(error.phase, held === 'upload' ? 'uploading' : 'waiting_for_peer');
        assert.ok(!error.message.includes('native'));
        assert.ok(!error.message.includes(f.staged[0].blob_id));
        return true;
      });
      await entered.promise;
      t.mock.timers.tick(60000);
      await rejected;
      assert.equal(f.commands.length, 0);
      assert.equal(f.events.at(-1), 'release');
      late.resolve(held === 'peer' ? 'native' : direct);
      // Drain continuations of acquisition, ready, and peer promises.
      for (let i = 0; i < 12; i += 1) await Promise.resolve();
      assert.equal(pushes, held === 'upload' ? 1 : 0);
      assert.equal(f.commands.length, 0);
    });
  }
  }
}
