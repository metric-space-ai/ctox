import assert from 'node:assert/strict';
import test from 'node:test';
import { CtoxRecoveryJournal } from '../src/recovery-journal.mjs';

// A request-driven fixture rejects bulk payload reads. Real IndexedDB storage,
// upgrades and durability remain covered by recovery-journal-browser-smoke.
function fixture(records, onGet = () => {}) {
  const stores = Object.fromEntries(['batches', 'conflicts', 'meta'].map((name) => [
    name, new Map((records[name] || []).map((row) => [row.batchId || row.conflictId || row.key, structuredClone(row)])),
  ]));
  const reads = [];
  const db = {
    transaction(name) {
      let completed = false;
      const tx = {
        objectStore() {
          const values = stores[name];
          const finish = () => setImmediate(() => {
            if (!completed) { completed = true; tx.oncomplete?.(); }
          });
          const requestFor = (operation) => {
            const request = {};
            queueMicrotask(() => {
              try { request.result = operation(); request.onsuccess?.(); finish(); }
              catch (error) { request.error = error; request.onerror?.(); finish(); }
            });
            return request;
          };
          const cursorFor = (rows) => {
            const request = {};
            let offset = 0;
            const advance = () => queueMicrotask(() => {
              request.result = offset < rows.length ? {
                value: structuredClone(rows[offset++]),
                continue: advance,
              } : null;
              request.onsuccess?.();
              if (!request.result) finish();
            });
            advance();
            return request;
          };
          const bulk = () => { throw new Error('recovery startup must not materialize all journal payloads'); };
          return {
            getAll: bulk,
            openCursor: () => cursorFor([...values.values()]),
            index(index) {
              return {
                getAll: bulk,
                openCursor: (key) => cursorFor([...values.values()].filter((row) => (
                  index === 'stateCollection'
                    ? row.state === key[0] && row.collection === key[1]
                    : row.state === key
                ))),
              };
            },
            get: (key) => requestFor(() => {
              reads.push([name, key]); onGet(stores, name, key);
              return structuredClone(values.get(key));
            }),
            put: (row) => requestFor(() => {
              const key = row.batchId || row.conflictId || row.key;
              values.set(key, structuredClone(row)); return key;
            }),
          };
        },
        abort() { tx.onabort?.(); },
      };
      return tx;
    },
  };
  const journal = new CtoxRecoveryJournal(db, { databaseName: 'owned-fixture', instanceId: 'fixture' });
  journal.publishStatus = async () => {};
  return { journal, stores, reads };
}
const bytes = (value) => new TextEncoder().encode(JSON.stringify(value)).byteLength;
const batch = (batchId, sequence, extra = {}) => ({
  batchId, sequence, collection: 'tickets', state: 'pending',
  rows: [{ id: batchId, title: 'offline résumé 🐈' }],
  documentIds: [batchId], ackedIds: [], primaryCommittedAtMs: 0,
  createdAtMs: 100 + sequence, ...extra,
});

test('status scans pending records with exact UTF-8 bytes and partial acknowledgements', async () => {
  const pending = [
    batch('a', 2, { documentIds: ['a', 'b'], ackedIds: ['a'] }),
    batch('c', 1, { primaryCommittedAtMs: 80 }),
  ];
  const conflicts = [{ conflictId: 'pending', state: 'pending', local: { title: '恢复 🐈' } }];
  const { journal } = fixture({
    batches: [...pending, batch('acked', 0, { state: 'master_acked' })],
    conflicts: [...conflicts, { conflictId: 'resolved', state: 'resolved', local: { title: 'old' } }],
    meta: [{ key: 'lastExport', value: 77 }],
  });
  const status = await journal.getStatus();
  assert.equal(status.pendingBatches, 2);
  assert.equal(status.pendingWrites, 2);
  assert.equal(status.pendingBytes, bytes(pending) + bytes(conflicts));
  assert.equal(status.oldestPendingAtMs, 101);
  assert.equal(status.unresolvedConflicts, 1);
  assert.equal(status.lastExportAtMs, 77);
});

test('empty status preserves the two empty-array byte count', async () => {
  const { journal } = fixture({});
  const status = await journal.getStatus();
  assert.equal(status.pendingBytes, 4);
  assert.equal(status.pendingWrites, 0);
  assert.equal(status.pendingBatches, 0);
  assert.equal(status.oldestPendingAtMs, 0);
  assert.equal(status.unresolvedConflicts, 0);
});

test('replay reads one candidate at a time in sequence order and rechecks durable state', async () => {
  const rows = [
    batch('second', 2), batch('committed', 0, { primaryCommittedAtMs: 88 }),
    batch('first', 1), batch('race', 3), batch('other', 4, { collection: 'other' }),
    batch('unregistered', 5, { collection: 'unregistered' }),
  ];
  const { journal, stores, reads } = fixture({ batches: rows }, (data, name, key) => {
    if (name === 'batches' && key === 'race') data.batches.get(key).state = 'master_acked';
  });
  const applied = [];
  journal.registerCollection('tickets', { applyBatch: async (entry) => {
    applied.push(structuredClone(entry));
    return { success: { [entry.batchId]: entry.rows[0] } };
  } });
  const result = await journal.replayRegisteredCollections('tickets');
  assert.deepEqual(result.map((entry) => [entry.batchId, entry.status]), [['first', 'replayed'], ['second', 'replayed']]);
  assert.deepEqual(applied.map((entry) => entry.rows[0]), [rows[2].rows[0], rows[0].rows[0]]);
  assert(!reads.some(([name, key]) => name === 'batches' && ['committed', 'other', 'unregistered'].includes(key)));
  assert(stores.batches.get('first').primaryCommittedAtMs > 0);
  assert.equal(stores.batches.get('race').state, 'master_acked');
});

test('bounded replay retains schema and application failures as recoverable conflicts', async () => {
  const { journal, stores } = fixture({ batches: [
    batch('schema', 1, { schemaHash: 'old', baseById: { schema: { title: 'base' } } }),
    batch('failed', 2, { schemaHash: 'new' }),
    batch('unregistered', 3, { collection: 'unregistered' }),
  ] });
  journal.registerCollection('tickets', { schemaHash: 'new', applyBatch: async () => {
    throw Object.assign(new Error('fixture refusal'), { code: 'fixture_refused' });
  } });
  const result = await journal.replayRegisteredCollections();
  assert.deepEqual(result.map((entry) => [entry.batchId, entry.status]), [['schema', 'conflict'], ['failed', 'conflict']]);
  const conflicts = [...stores.conflicts.values()];
  assert.deepEqual(conflicts.map((entry) => entry.code), ['recovery_schema_mismatch', 'fixture_refused']);
  assert.deepEqual(conflicts[0].base, { schema: { title: 'base' } });
  assert.equal(conflicts[1].local[0].title, 'offline résumé 🐈');
  assert.equal(stores.batches.get('schema').state, 'conflict');
  assert.equal(stores.batches.get('failed').state, 'conflict');
  assert.equal(stores.batches.get('unregistered').state, 'pending');
});

test('outstanding ID scan excludes ACKs, other collections and duplicate versions', async () => {
  const { journal } = fixture({ batches: [
    batch('partial', 1, { documentIds: ['a', 'b'], ackedIds: ['a'] }),
    batch('duplicate', 2, { documentIds: ['b', 'c'], primaryCommittedAtMs: 99 }),
    batch('acked', 3, { state: 'master_acked', documentIds: ['d'] }),
    batch('other', 4, { collection: 'other', documentIds: ['e'] }),
  ] });
  assert.deepEqual((await journal.pendingDocumentIds('tickets')).sort(), ['b', 'c']);
  assert.deepEqual(await journal.pendingDocumentIds('missing'), []);
});
