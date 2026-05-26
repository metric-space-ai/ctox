import {
  ctoxIndexedDbStorageTestInternals,
  ctoxRxdbTestInternals,
} from '../dist/ctox-rxdb-js.mjs';

const {
  documentMatchesReplicationOrigin,
  indexValuesFor,
  normalizeDocument,
  normalizeSchemaIndexes,
  selectBestIndex,
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
assert(indexes.length === 2, 'schema indexes were not normalized');
assert(indexes[0].fields.join(',') === 'thread_key,external_created_at', 'compound index fields mismatch');
assert(indexes[1].fields[0] === 'updated_at_ms', 'single index field mismatch');

const doc = normalizeDoc({
  message: { key: 'message-1' },
  thread_key: 'thread-1',
  external_created_at: '2026-05-22T09:00:00.000Z',
  updated_at_ms: 42,
}, 'message.key');
assert(doc.id === 'message-1', 'nested primary key was not promoted to id');
assert(doc.message.key === 'message-1', 'nested primary key path was not preserved');

const values = indexValuesFor(indexes, doc);
assert(values.idx_0_thread_key_external_created_at.join('|') === 'thread-1|2026-05-22T09:00:00.000Z', 'compound index values mismatch');
assert(values.idx_1_updated_at_ms[0] === 42, 'single index value mismatch');

const selected = selectBestIndex(indexes, ['thread_key'], ['external_created_at']);
assert(selected?.name === 'idx_0_thread_key_external_created_at', 'best compound index was not selected');
assert(selected.matchedFields === 2, 'compound index match score mismatch');

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

console.log('ctox-rxdb-js storage index smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
