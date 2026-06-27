import {
  CtoxIndexedDbCollection,
  createRxDatabase,
  ctoxIndexedDbStorageTestInternals,
  ctoxRxdbTestInternals,
} from '../dist/ctox-rxdb-js.mjs';

const {
  canUseBoundedCollectionCursor,
  encodeIndexValue,
  documentMatchesReplicationOrigin,
  indexValuesFor,
  normalizeDocument,
  normalizeStoredReplicationFlags,
  normalizeSchemaIndexes,
  primaryKeyCandidateIds,
  replicationScanLimit,
  schemaIndexEntriesFor,
  schemaIndexQueryPlanFor,
  selectBestIndex,
  shouldUsePushableReplicationIndex,
} = ctoxIndexedDbStorageTestInternals;
const { normalizeDoc } = ctoxRxdbTestInternals;

const schema = {
  primaryKey: 'message.key',
  indexes: [
    ['thread_key', 'external_created_at'],
    'updated_at_ms',
  ],
};

const indexes = normalizeSchemaIndexes(schema);
assert(indexes.length === 3, 'schema indexes were not normalized');
assert(indexes[0].fields.join(',') === '_deleted,thread_key,external_created_at,message.key', 'compound index fields mismatch');
assert(indexes[1].fields.join(',') === '_deleted,updated_at_ms,message.key', 'single index field mismatch');
assert(indexes[2].fields.join(',') === '_meta.lwt,message.key', 'internal lwt index mismatch');

const doc = normalizeDoc({
  message: { key: 'message-1' },
  thread_key: 'thread-1',
  external_created_at: '2026-05-22T09:00:00.000Z',
  updated_at_ms: 42,
}, 'message.key');
assert(doc.id === 'message-1', 'nested primary key was not promoted to id');
assert(doc.message.key === 'message-1', 'nested primary key path was not preserved');

const values = indexValuesFor(indexes, doc);
assert(values['idx_0__deleted_thread_key_external_created_at_message.key'].join('|') === 'false|thread-1|2026-05-22T09:00:00.000Z|message-1', 'compound index values mismatch');
assert(values['idx_1__deleted_updated_at_ms_message.key'].join('|') === 'false|42|message-1', 'single index value mismatch');
const indexEntries = schemaIndexEntriesFor(indexes, doc, doc.id, 'messages');
assert(
  indexEntries.some((entry) => entry.join('|') === 'messages|idx_0__deleted_thread_key_external_created_at_message.key|b|0|s|thread-1|s|2026-05-22T09:00:00.000Z|s|message-1|message-1'),
  'compound schema-index entry was not materialized for IndexedDB multiEntry lookup',
);
assert(encodeIndexValue(false).join('|') === 'b|0', 'boolean index encoding mismatch');

const selected = selectBestIndex(indexes, ['thread_key'], ['external_created_at']);
assert(selected?.name === 'idx_0__deleted_thread_key_external_created_at_message.key', 'best compound index was not selected');
assert(selected.matchedFields === 2, 'compound index match score mismatch');
const equalityPlan = schemaIndexQueryPlanFor({ selector: { thread_key: 'thread-1' } }, indexes);
assert(equalityPlan?.index?.name === selected.name, 'schema-index equality plan was not selected');
assert(equalityPlan.direction === 'next', 'schema-index equality plan should scan forward by default');
assert(equalityPlan.sortCovered === true, 'unsorted equality plan should be sort-covered');
const rangeAscPlan = schemaIndexQueryPlanFor({
  selector: {
    thread_key: 'thread-1',
    external_created_at: {
      $gte: '2026-05-22T00:00:00.000Z',
      $lt: '2026-05-23T00:00:00.000Z',
    },
  },
  sort: [{ external_created_at: 'asc' }],
  limit: 20,
}, indexes);
assert(rangeAscPlan?.index?.name === selected.name, 'schema-index range plan was not selected');
assert(rangeAscPlan.direction === 'next', 'ascending range plan must use forward cursor');
assert(rangeAscPlan.sortCovered === true, 'ascending range sort should be covered');
assert(
  rangeAscPlan.ranges[0].lower.join('|') === 'b|0|s|thread-1|s|2026-05-22T00:00:00.000Z',
  'range lower bound mismatch',
);
assert(
  rangeAscPlan.ranges[0].upper.join('|') === 'b|0|s|thread-1|s|2026-05-23T00:00:00.000Z',
  'open upper bound must not include high sentinel',
);
const rangeDescPlan = schemaIndexQueryPlanFor({
  selector: { thread_key: 'thread-1', external_created_at: { $gte: 'a', $lte: 'z' } },
  sort: [{ external_created_at: 'desc' }],
  limit: 20,
}, indexes);
assert(rangeDescPlan?.direction === 'prev', 'descending range plan must use reverse cursor');
assert(
  rangeDescPlan.ranges[0].upper.join('|') === 'b|0|s|thread-1|s|z|\uffff',
  'closed upper bound must include high sentinel',
);
const unsupportedPlan = schemaIndexQueryPlanFor({ selector: { thread_key: { $regex: '^t' } }, limit: 10 }, indexes);
assert(unsupportedPlan === null, 'unsupported regex selector must not report schema-index execution');
const nonPrefixPlan = schemaIndexQueryPlanFor({ selector: { external_created_at: '2026-05-22T09:00:00.000Z' } }, indexes);
assert(nonPrefixPlan === null, 'non-prefix compound selector must not report schema-index execution');
const collectionPlanProbe = new CtoxIndexedDbCollection(null, 'messages', { schema });
const executablePlan = collectionPlanProbe.queryPlanFor({
  selector: { thread_key: 'thread-1', external_created_at: { $gte: 'a', $lte: 'z' } },
  sort: [{ external_created_at: 'desc' }],
  limit: 20,
});
assert(executablePlan.strategy === 'schema-index', 'queryPlanFor must report real schema-index execution strategy');
assert(executablePlan.indexed === true && executablePlan.schemaIndexed === true, 'queryPlanFor schema-index flags mismatch');
const unsupportedExecutablePlan = collectionPlanProbe.queryPlanFor({
  selector: { thread_key: { $regex: '^t' } },
  limit: 10,
});
assert(unsupportedExecutablePlan.strategy === 'bounded-collection', 'unsupported regex should fall back to bounded collection cursor');
assert(unsupportedExecutablePlan.indexed === false && unsupportedExecutablePlan.schemaIndexed === false, 'unsupported regex must not report indexed execution');
const allDocumentsExecutablePlan = collectionPlanProbe.queryPlanFor({
  selector: { thread_key: { $regex: '^t' } },
  sort: [{ external_created_at: 'desc' }],
});
assert(allDocumentsExecutablePlan.strategy === 'all-documents', 'unbounded sorted regex must be classified as all-documents fallback');
assert(allDocumentsExecutablePlan.allDocumentsFallback === true, 'all-documents query plan must expose fallback flag');
collectionPlanProbe.setQueryPerformancePolicy({ rejectAllDocumentsFallback: true });
await assertRejects(
  () => collectionPlanProbe.queryDocuments({
    selector: { thread_key: { $regex: '^t' } },
    sort: [{ external_created_at: 'desc' }],
  }),
  (error) => error?.code === 'CTOX_INDEXEDDB_ALL_DOCUMENTS_FALLBACK',
  'strict query-performance policy must reject allDocuments fallback before opening IndexedDB',
);
const fallbackStats = collectionPlanProbe.getQueryPerformanceStats();
assert(fallbackStats.allDocumentsFallbackCalls === 1, 'allDocuments fallback rejection must increment fallback calls');
assert(fallbackStats.allDocumentsCalls === 0, 'strict fallback rejection must happen before allDocuments() executes');

const pulledDoc = normalizeDocument(
  { id: 'chunk-1', file_id: 'file-1', data: 'abc' },
  100,
  { role: 'ctox_instance', peerId: 'peer-1', sessionId: 'session-1', collection: 'desktop_file_chunks' },
);
assert(
  pulledDoc._meta?.ctoxReplicationOrigin?.role === 'ctox_instance',
  'replication origin marker was not attached to pulled documents',
);
assert(
  documentMatchesReplicationOrigin(pulledDoc, 'ctox_instance'),
  'origin marker must allow echo suppression for CTOX-origin documents',
);
const locallyEditedDoc = normalizeDocument({ ...pulledDoc, data: 'edited' }, 101);
assert(
  !locallyEditedDoc._meta?.ctoxReplicationOrigin,
  'local writes must clear replication origin so browser edits remain pushable',
);
assert(shouldUsePushableReplicationIndex('ctox_instance') === true, 'ctox-origin push reads must use the pushable index');
assert(shouldUsePushableReplicationIndex('browser') === false, 'non-ctox roles keep the generic changed-since scan');
const legacyRemoteRecord = normalizeStoredReplicationFlags({
  collection: 'messages',
  id: 'remote-legacy',
  lwt: 99,
  doc: pulledDoc,
});
assert(legacyRemoteRecord.pushable === 0, 'legacy replicated records must migrate as non-pushable');
assert(legacyRemoteRecord.replicationOriginRole === 'ctox_instance', 'legacy replicated records must preserve origin role');
const legacyLocalRecord = normalizeStoredReplicationFlags({
  collection: 'messages',
  id: 'local-legacy',
  lwt: 100,
  doc: locallyEditedDoc,
});
assert(legacyLocalRecord.pushable === 1, 'legacy local records must migrate as pushable');
assert(
  primaryKeyCandidateIds({ selector: { 'message.key': 'message-1' } }, 'message.key').join(',') === 'message-1',
  'nested primary-key equality should be detected as a bounded candidate query',
);
assert(
  primaryKeyCandidateIds({ selector: { id: { $in: ['a', 'b', 'a'] } } }, 'message.key').join(',') === 'a,b',
  'primary-key $in should deduplicate bounded candidate ids',
);
assert(replicationScanLimit(1) === 50, 'single-row pushes must not inherit a 500-entry scan floor');
assert(replicationScanLimit(6) === 300, 'chunk-sized pushes must not inherit a 500-entry scan floor');
assert(replicationScanLimit(10) === 500, 'larger command batches retain the scan multiplier');
assert(replicationScanLimit(200) === 5000, 'replication scan limit must cap large batches');
assert(
  canUseBoundedCollectionCursor({ selector: { status: 'open' }, limit: 10 }) === true,
  'small unsorted browser queries must use bounded collection cursor',
);
assert(
  canUseBoundedCollectionCursor({ selector: { status: 'open' }, sort: [{ updated_at_ms: 'desc' }], limit: 10 }) === false,
  'sorted queries need a dedicated indexed plan, not the unsorted bounded cursor',
);

const previousIdbKeyRange = globalThis.IDBKeyRange;
globalThis.IDBKeyRange = {
  bound(lower, upper, lowerOpen = false, upperOpen = false) {
    return { lower, upper, lowerOpen, upperOpen };
  },
  only(value) {
    return { only: value };
  },
};
try {
  const fakeDb = createFakeIndexedDb([{
    collection: 'upsert_perf',
    id: 'doc-1',
    lwt: 10,
    deleted: false,
    doc: { id: 'doc-1', value: 'old', _meta: { lwt: 10 } },
  }]);
  const upsertCollection = new CtoxIndexedDbCollection(fakeDb, 'upsert_perf', { schema: { primaryKey: 'id' } });
  let changeEvents = 0;
  upsertCollection.observe(() => { changeEvents += 1; });
  const written = await upsertCollection.upsert({ id: 'doc-1', value: 'new', extra: true, updated_at_ms: 11 });

  assert(written.id === 'doc-1', 'upsert must return the written document');
  assert(written.value === 'new' && written.extra === true, 'upsert must merge and return updated fields');
  assert(written._meta.lwt === 11, 'upsert must preserve explicit newer lwt fields');
  assert(changeEvents === 1, 'upsert must emit exactly one change event');
  assert(fakeDb.stats.transactions.length === 1, `upsert must use one transaction, got ${fakeDb.stats.transactions.length}`);
  assert(fakeDb.stats.transactions[0].mode === 'readwrite', 'upsert transaction must be readwrite');
  assert(fakeDb.stats.gets === 1, `upsert must read the existing row once inside the write transaction, got ${fakeDb.stats.gets}`);
  assert(fakeDb.stats.puts === 1, `upsert must write exactly once, got ${fakeDb.stats.puts}`);
  assert(fakeDb.stats.cursorOpens === 1, `upsert must read the collection lwt floor once, got ${fakeDb.stats.cursorOpens}`);

  const fakeBulkDb = createFakeIndexedDb([{
    collection: 'bulk_upsert_perf',
    id: 'doc-1',
    lwt: 10,
    deleted: false,
    doc: { id: 'doc-1', preserved: true, value: 'old', _meta: { lwt: 10 } },
  }]);
  const bulkCollection = new CtoxIndexedDbCollection(fakeBulkDb, 'bulk_upsert_perf', { schema: { primaryKey: 'id' } });
  let bulkChangeEvents = 0;
  bulkCollection.observe(() => { bulkChangeEvents += 1; });
  const bulkResult = await bulkCollection.bulkUpsert([
    { id: 'doc-1', value: 'new', updated_at_ms: 20 },
    { id: 'doc-2', value: 'created', updated_at_ms: 21 },
    { id: 'doc-3', value: 'created', updated_at_ms: 22 },
  ]);
  assert(bulkResult.success['doc-1'].preserved === true, 'bulkUpsert must preserve existing fields during merge');
  assert(bulkResult.success['doc-1'].value === 'new', 'bulkUpsert must update merged fields');
  assert(Object.keys(bulkResult.success).length === 3, 'bulkUpsert must return all written docs');
  assert(bulkChangeEvents === 1, 'bulkUpsert must emit one coalesced change event');
  assert(fakeBulkDb.stats.transactions.length === 1, `bulkUpsert must use one transaction, got ${fakeBulkDb.stats.transactions.length}`);
  assert(fakeBulkDb.stats.gets === 3, `bulkUpsert must read existing rows once per doc in the write transaction, got ${fakeBulkDb.stats.gets}`);
  assert(fakeBulkDb.stats.puts === 3, `bulkUpsert must write each accepted doc once, got ${fakeBulkDb.stats.puts}`);
  assert(fakeBulkDb.stats.cursorOpens === 1, `bulkUpsert must read the collection lwt floor once, got ${fakeBulkDb.stats.cursorOpens}`);

  const pushableDb = createFakeIndexedDb([
    {
      collection: 'pushable_changes',
      id: 'remote-1',
      lwt: 10,
      pushable: 0,
      deleted: false,
      doc: { id: 'remote-1', _meta: { lwt: 10, ctoxReplicationOrigin: { role: 'ctox_instance' } } },
    },
    {
      collection: 'pushable_changes',
      id: 'remote-2',
      lwt: 11,
      pushable: 0,
      deleted: false,
      doc: { id: 'remote-2', _meta: { lwt: 11, ctoxReplicationOrigin: { role: 'ctox_instance' } } },
    },
    {
      collection: 'pushable_changes',
      id: 'local-1',
      lwt: 12,
      pushable: 1,
      deleted: false,
      doc: { id: 'local-1', _meta: { lwt: 12 } },
    },
  ]);
  const pushableCollection = new CtoxIndexedDbCollection(pushableDb, 'pushable_changes', { schema: { primaryKey: 'id' } });
  const pushableResult = await pushableCollection.getChangedDocumentsSince(null, 1, {
    excludeReplicationOriginRole: 'ctox_instance',
  });
  assert(pushableResult.documents.map((doc) => doc.id).join(',') === 'local-1', 'pushable index must return only local changes');
  assert(pushableResult.scanned === 1, `pushable index must not scan remote-origin rows, got scanned=${pushableResult.scanned}`);
} finally {
  if (previousIdbKeyRange === undefined) {
    delete globalThis.IDBKeyRange;
  } else {
    globalThis.IDBKeyRange = previousIdbKeyRange;
  }
}

const rxBulkStats = { bulkUpsertCalls: 0, bulkWriteCalls: 0, upsertCalls: 0 };
const rxDb = await createRxDatabase({
  name: 'bulk-upsert-facade',
  storage: {
    nativeStorage: {
      collection() {
        return {
          async bulkUpsert(rows) {
            rxBulkStats.bulkUpsertCalls += 1;
            return {
              success: Object.fromEntries(rows.map((row) => [row.id, { ...row, stored_via_bulk_upsert: true }])),
              error: [],
            };
          },
          async bulkWrite() {
            rxBulkStats.bulkWriteCalls += 1;
            return { success: {}, error: [] };
          },
          async upsert() {
            rxBulkStats.upsertCalls += 1;
            return null;
          },
        };
      },
      close() {},
    },
  },
});
await rxDb.addCollections({
  bulk_docs: {
    schema: {
      primaryKey: 'id',
      version: 1,
      properties: {},
    },
  },
});
const rxWritten = await rxDb.bulk_docs.bulkUpsert([
  { id: 'rx-1', value: 1 },
  { id: 'rx-2', value: 2 },
  { id: 'rx-3', value: 3 },
]);
assert(rxBulkStats.bulkUpsertCalls === 1, `Rx collection bulkUpsert must call storage bulkUpsert once, got ${rxBulkStats.bulkUpsertCalls}`);
assert(rxBulkStats.bulkWriteCalls === 0, 'Rx collection bulkUpsert must not fall back to bulkWrite when storage bulkUpsert exists');
assert(rxBulkStats.upsertCalls === 0, 'Rx collection bulkUpsert must not call storage upsert per document');
assert(rxWritten.length === 3, 'Rx collection bulkUpsert must return one document per input');
assert(rxWritten.every((doc) => doc.stored_via_bulk_upsert === true), 'Rx collection bulkUpsert must return stored docs from the batch result');
await rxDb.close();

console.log('ctox-rxdb-js storage index smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function assertRejects(fn, predicate, message) {
  try {
    await fn();
  } catch (error) {
    if (predicate(error)) return;
    throw new Error(`${message}: unexpected error ${error?.stack || error}`);
  }
  throw new Error(`${message}: expected rejection`);
}

function createFakeIndexedDb(seedRecords = []) {
  const records = new Map(seedRecords.map((record) => [recordKey(record.collection, record.id), cloneJson(record)]));
  const stats = {
    transactions: [],
    gets: 0,
    puts: 0,
    cursorOpens: 0,
  };
  return {
    stats,
    records,
    transaction(storeName, mode) {
      stats.transactions.push({ storeName, mode });
      return createFakeTransaction(records, stats);
    },
  };
}

function createFakeTransaction(records, stats) {
  const tx = {
    error: null,
    oncomplete: null,
    onabort: null,
    onerror: null,
    _pending: 0,
    _settled: false,
    objectStore() {
      return createFakeObjectStore(records, stats, tx);
    },
  };
  return tx;
}

function createFakeObjectStore(records, stats, tx) {
  return {
    get(key) {
      stats.gets += 1;
      return fakeRequest(tx, () => cloneJson(records.get(recordKeyFromIdbKey(key)) || null));
    },
    put(record) {
      stats.puts += 1;
      return fakeRequest(tx, () => {
        records.set(recordKey(record.collection, record.id), cloneJson(record));
        return record;
      });
    },
    index(indexName) {
      return {
        openCursor(range, direction = 'next') {
          stats.cursorOpens += 1;
          return fakeCursorRequest(tx, () => cursorRecordsForIndex(records, indexName, range, direction), records);
        },
      };
    },
  };
}

function fakeRequest(tx, produce) {
  const request = {
    result: undefined,
    error: null,
    onsuccess: null,
    onerror: null,
  };
  tx._pending += 1;
  queueMicrotask(() => {
    try {
      request.result = produce();
      request.onsuccess?.();
    } catch (error) {
      request.error = error;
      request.onerror?.();
    } finally {
      tx._pending -= 1;
      setTimeout(() => {
        if (tx._pending === 0 && !tx._settled) {
          tx._settled = true;
          tx.oncomplete?.();
        }
      }, 0);
    }
  });
  return request;
}

function fakeCursorRequest(tx, produce, records) {
  const request = {
    result: undefined,
    error: null,
    onsuccess: null,
    onerror: null,
  };
  tx._pending += 1;
  let rows = [];
  let offset = 0;
  let active = true;
  const finishCursor = () => {
    if (!active) return;
    active = false;
    tx._pending -= 1;
    settleTransactionIfIdle(tx);
  };
  const emit = () => {
    queueMicrotask(() => {
      try {
        if (!rows.length) rows = produce();
        const row = rows[offset] || null;
        if (!row) {
          request.result = null;
          request.onsuccess?.();
          finishCursor();
          return;
        }
        let continued = false;
        request.result = {
          value: cloneJson(row),
          continue() {
            if (continued || !active) return;
            continued = true;
            offset += 1;
            emit();
          },
          update(next) {
            return fakeRequest(tx, () => {
              const cloned = cloneJson(next);
              records.set(recordKey(cloned.collection, cloned.id), cloned);
              rows[offset] = cloned;
              return cloned;
            });
          },
        };
        request.onsuccess?.();
        setTimeout(() => {
          if (!continued) finishCursor();
        }, 0);
      } catch (error) {
        request.error = error;
        request.onerror?.();
        finishCursor();
      }
    });
  };
  emit();
  return request;
}

function settleTransactionIfIdle(tx) {
  setTimeout(() => {
    if (tx._pending === 0 && !tx._settled) {
      tx._settled = true;
      tx.oncomplete?.();
    }
  }, 0);
}

function cursorRecordsForIndex(records, indexName, range, direction) {
  const rows = Array.from(records.values())
    .map((record) => ({ record, key: keyForIndex(record, indexName) }))
    .filter((entry) => entry.key && keyInRange(entry.key, range))
    .sort((left, right) => compareKeys(left.key, right.key))
    .map((entry) => entry.record);
  return direction === 'prev' ? rows.reverse() : rows;
}

function keyForIndex(record, indexName) {
  if (indexName === 'collection') return [record.collection];
  if (indexName === 'collectionLwtId') return [record.collection, Number(record.lwt || 0), String(record.id || '')];
  if (indexName === 'collectionPushableLwtId') {
    return [record.collection, Number(record.pushable || 0), Number(record.lwt || 0), String(record.id || '')];
  }
  return null;
}

function keyInRange(key, range) {
  if (!range) return true;
  if (Object.hasOwn(range, 'only')) return compareKeys(key, Array.isArray(range.only) ? range.only : [range.only]) === 0;
  if (range.lower) {
    const lowerCmp = compareKeys(key, range.lower);
    if (lowerCmp < 0 || (lowerCmp === 0 && range.lowerOpen)) return false;
  }
  if (range.upper) {
    const upperCmp = compareKeys(key, range.upper);
    if (upperCmp > 0 || (upperCmp === 0 && range.upperOpen)) return false;
  }
  return true;
}

function compareKeys(left, right) {
  const len = Math.max(left.length, right.length);
  for (let i = 0; i < len; i += 1) {
    const a = left[i];
    const b = right[i];
    if (a === b) continue;
    if (a === undefined) return -1;
    if (b === undefined) return 1;
    if (typeof a === 'number' && typeof b === 'number') return a - b;
    return String(a).localeCompare(String(b));
  }
  return 0;
}

function recordKeyFromIdbKey(key) {
  return Array.isArray(key) ? recordKey(key[0], key[1]) : String(key || '');
}

function recordKey(collection, id) {
  return `${String(collection)}\u0000${String(id)}`;
}

function cloneJson(value) {
  return value == null ? value : JSON.parse(JSON.stringify(value));
}
