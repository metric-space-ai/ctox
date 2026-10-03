// Fake-peer proof for rxdb.rows.*: chunk order, retryable errors, abort,
// paging past 2_500 rows, a null loader without the rows capability, and no
// collection writes.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import {
  CTOX_ROWS_FETCH_CAPABILITY,
  CTOX_ROWS_RPC,
} from '../src/protocol-contract.generated.mjs';
import {
  createDemandLoadingTransport,
  createRowsDemandLoader,
  remoteSupportsRowsFetch,
  replicationWebRtcTestInternals,
} from '../dist/ctox-rxdb-js.mjs';

const loaderSource = readFileSync(fileURLToPath(new URL('../src/rows-demand-loader.mjs', import.meta.url)), 'utf8');
assert.doesNotMatch(loaderSource, /bulkWrite|indexedDB|storageCollection/, 'rows loader must not touch a collection or IndexedDB');

const ReplicationState = replicationWebRtcTestInternals.getReplicationStateClass();
const writes = [];

function openPeer() {
  return {
    connections: new Map([
      ['peer-rows', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }],
    ]),
    async request(peerId, method, params) {
      const body = params?.[0] || null;
      sent.push({ peerId, method, body });
      if (method === CTOX_ROWS_RPC.fetch && mode === 'error') {
        queueMicrotask(() => {
          transport.requestHandlers[CTOX_ROWS_RPC.error]({
            params: [{
              requestId: body.requestId,
              code: 'ROWS_SOURCE_ERROR',
              message: 'parquet read failed',
              retryable: true,
            }],
          });
        });
      } else if (method === CTOX_ROWS_RPC.fetch && mode === 'fatal') {
        queueMicrotask(() => {
          transport.requestHandlers[CTOX_ROWS_RPC.error]({
            params: [{
              requestId: body.requestId,
              code: 'ROWS_TABLE_NOT_FOUND',
              message: 'archived',
              retryable: false,
            }],
          });
        });
      } else if (method === CTOX_ROWS_RPC.fetch && mode === 'page') {
        queueMicrotask(() => deliverPage(body));
      }
      return { ack: true };
    },
  };
}

const sent = [];
let mode = 'manual';
let deliverPage = () => {};
let transport = createDemandLoadingTransport({ getPeerId: () => 'peer-rows' });
transport.attach(openPeer());
let loader = createRowsDemandLoader({ transport, storageCollection: { bulkWrite() { writes.push('direct'); } } });

function chunk(body, seq, final, rows) {
  return {
    requestId: body.requestId,
    seq,
    final,
    tableId: body.tableId,
    offset: body.offset,
    rowCount: body.rowCount,
    contentHash: body.contentHash,
    schemaHash: body.schemaHash,
    rows,
  };
}

async function pushChunk(frame) {
  await transport.requestHandlers[CTOX_ROWS_RPC.chunk]({ params: [frame] });
}

{
  mode = 'manual';
  sent.length = 0;
  const pending = loader.fetchRows('table:kdt-order', { offset: 4, limit: 5000 });
  await waitUntil(() => sent.some((entry) => entry.method === CTOX_ROWS_RPC.fetch));
  const fetch = sent.find((entry) => entry.method === CTOX_ROWS_RPC.fetch);
  assert.equal(fetch.body.collectionName, 'knowledge_tables');
  assert.equal(fetch.body.tableId, 'kdt-order', 'table: prefix is stripped on the wire');
  assert.equal(fetch.body.offset, 4);
  assert.equal(fetch.body.limit, CTOX_ROWS_RPC.maxRowsPerWindow, 'limit is capped at the contract window');
  const frame = {
    requestId: fetch.body.requestId,
    tableId: 'kdt-order',
    offset: 4,
    rowCount: 9,
    contentHash: 'hash-content',
    schemaHash: 'hash-schema',
  };
  let settled = false;
  pending.then(() => { settled = true; }, () => { settled = true; });
  await pushChunk(chunk(frame, 2, true, [{ id: 'c' }]));
  await pushChunk(chunk(frame, 0, false, [{ id: 'a' }]));
  await delay(30);
  assert.equal(settled, false, 'window must not resolve before every seq through final is present');
  await pushChunk(chunk(frame, 1, false, [{ id: 'b' }]));
  const result = await pending;
  assert.deepEqual(result.rows.map((row) => row.id), ['a', 'b', 'c']);
  assert.equal(result.rowCount, 9);
  assert.equal(result.contentHash, 'hash-content');
  assert.equal(result.schemaHash, 'hash-schema');
  assert.equal(result.offset, 4);
  assert.equal(writes.length, 0, 'assembling a window must not write a collection');
}

{
  mode = 'error';
  await assert.rejects(
    loader.fetchRows('kdt-order', { offset: 0, limit: 10 }),
    (error) => error?.code === 'ROWS_SOURCE_ERROR' && error.retryable === true,
  );
  mode = 'fatal';
  await assert.rejects(
    loader.fetchRows('kdt-order', { offset: 0, limit: 10 }),
    (error) => error?.code === 'ROWS_TABLE_NOT_FOUND' && error.retryable === false,
  );
}

{
  mode = 'manual';
  sent.length = 0;
  const controller = new AbortController();
  const pending = loader.fetchRows('kdt-order', { offset: 0, limit: 10, signal: controller.signal });
  await waitUntil(() => sent.some((entry) => entry.method === CTOX_ROWS_RPC.fetch));
  const requestId = sent.find((entry) => entry.method === CTOX_ROWS_RPC.fetch).body.requestId;
  controller.abort();
  await assert.rejects(pending, (error) => error?.code === 'ROWS_CANCELLED' && error.retryable === false);
  await waitUntil(() => sent.some((entry) => entry.method === CTOX_ROWS_RPC.cancel && entry.body?.requestId === requestId));
  const cancel = sent.find((entry) => entry.method === CTOX_ROWS_RPC.cancel && entry.body?.requestId === requestId);
  assert.deepEqual(Object.keys(cancel.body), ['requestId']);
}

{
  mode = 'page';
  sent.length = 0;
  const total = 2500;
  deliverPage = (body) => {
    const offset = Number(body.offset) || 0;
    const limit = Number(body.limit) || 0;
    const count = Math.max(0, Math.min(limit, total - offset));
    const rows = Array.from({ length: count }, (_, index) => ({ id: offset + index }));
    const mid = Math.ceil(rows.length / 2);
    const frame = {
      requestId: body.requestId,
      tableId: body.tableId,
      offset,
      rowCount: total,
      contentHash: 'hash-page',
      schemaHash: 'hash-schema',
    };
    if (count === 0) {
      pushChunk(chunk(frame, 0, true, []));
      return;
    }
    pushChunk(chunk(frame, 1, true, rows.slice(mid)));
    pushChunk(chunk(frame, 0, false, rows.slice(0, mid)));
  };
  const all = await loader.fetchAllRows('table:kdt-measured');
  assert.equal(all.rows.length, total);
  assert.equal(all.rowCount, total);
  assert.equal(all.rows[0].id, 0);
  assert.equal(all.rows[total - 1].id, total - 1);
  assert.equal(all.contentHash, 'hash-page');
  const fetches = sent.filter((entry) => entry.method === CTOX_ROWS_RPC.fetch);
  assert.deepEqual(fetches.map((entry) => entry.body.offset), [0, 1000, 2000]);
  assert.ok(fetches.every((entry) => entry.body.limit === CTOX_ROWS_RPC.maxRowsPerWindow));
  assert.ok(fetches.every((entry) => entry.body.tableId === 'kdt-measured'));
  assert.equal(writes.length, 0, 'paging rows must not write a collection');
}

assert.equal(remoteSupportsRowsFetch(null), false);
assert.equal(remoteSupportsRowsFetch({ capabilities: [] }), false);
assert.equal(remoteSupportsRowsFetch({ capabilities: [CTOX_ROWS_FETCH_CAPABILITY] }), true);

{
  const missing = await makeKnowledgeState([]);
  await missing.enableDemandLoading({ indexedDbAvailable: false });
  assert.equal(missing.knowledgeRowsLoader, null, 'loader stays null without the rows capability');
  await missing.cancel();
  assert.equal(missing.knowledgeRowsLoader, null);

  const other = await makeKnowledgeState([CTOX_ROWS_FETCH_CAPABILITY], 'business_records');
  await other.enableDemandLoading({ indexedDbAvailable: false });
  assert.equal(other.knowledgeRowsLoader, null, 'rows loader is only attached to knowledge_tables');
  await other.cancel();

  const capable = await makeKnowledgeState([CTOX_ROWS_FETCH_CAPABILITY]);
  const bulkWrite = capable.collection.storageCollection.bulkWrite;
  await capable.enableDemandLoading({ indexedDbAvailable: false });
  assert.equal(typeof capable.knowledgeRowsLoader?.fetchRows, 'function');
  capable.collection.storageCollection.bulkWrite = async (...args) => {
    writes.push(args);
    return bulkWrite(...args);
  };
  const page = await capable.knowledgeRowsLoader.fetchRows('table:kdt-capable', { offset: 0, limit: 2 });
  assert.equal(page.rows.length, 2);
  assert.equal(writes.length, 0, 'knowledgeRowsLoader must not write the collection');
  await capable.cancel();
  assert.equal(capable.knowledgeRowsLoader, null, 'cancel drops the rows loader');
}

console.log('rows-demand-loader-smoke: ok');

async function makeKnowledgeState(capabilities, collectionName = 'knowledge_tables') {
  const rowsTransport = createDemandLoadingTransport({ getPeerId: () => 'peer-rows' });
  const peer = {
    connections: new Map([
      ['peer-rows', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }],
    ]),
    async request(_peerId, method, params) {
      const body = params?.[0] || {};
      if (method === CTOX_ROWS_RPC.fetch) {
        queueMicrotask(() => {
          rowsTransport.requestHandlers[CTOX_ROWS_RPC.chunk]({
            params: [{
              requestId: body.requestId,
              seq: 0,
              final: true,
              tableId: body.tableId,
              offset: body.offset,
              rowCount: 2,
              contentHash: 'hash-capable',
              schemaHash: 'hash-schema',
              rows: [{ id: 'one' }, { id: 'two' }],
            }],
          });
        });
      }
      return { ack: true };
    },
  };
  rowsTransport.attach(peer);
  const state = new ReplicationState({
    collection: mockCollection(collectionName),
    topic: `rows-${collectionName}`,
    pull: { batchSize: 5 },
    push: { batchSize: 5 },
    retryTime: 60,
    ctox: {},
  });
  state.initialReplication?.catch?.(() => {});
  state.activeRemotePeerId = 'peer-rows';
  state.shared = {
    peer,
    demandTransport: rowsTransport,
    negotiated: {
      peerId: 'peer-rows',
      remoteProtocol: { capabilities: [...capabilities] },
    },
    getTransportStatus: () => ({}),
    unregister() {},
    isPeerOpen: () => true,
  };
  state.peerStates$.next(new Map([
    ['peer-rows', { remoteProtocol: { capabilities: [...capabilities] } }],
  ]));
  return state;
}

function mockCollection(name) {
  let demandLoader = null;
  return {
    name,
    schema: {
      version: 0,
      primaryPath: 'id',
      hash: async () => `hash-${name}`,
    },
    observe() { return { unsubscribe() {} }; },
    setDemandLoader(next) { demandLoader = next; },
    get demandLoader() { return demandLoader; },
    storageCollection: {
      databaseName: `db-${name}`,
      replicationCheckpointStatus: async () => ({ epoch: 'e1', state: 'ready' }),
      getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
      getStoredRecord: async () => null,
      bulkWrite: async () => {
        writes.push('storage');
        return {};
      },
    },
  };
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitUntil(predicate, timeoutMs = 1000) {
  const deadline = Date.now() + timeoutMs;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error('timed out waiting for rows peer');
    await delay(10);
  }
}
