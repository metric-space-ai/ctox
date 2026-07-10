import {
  createFileDemandLoader,
  createMemoryMetaBackend,
} from '../dist/ctox-rxdb-js.mjs';

const written = [];
const writeBatches = [];
const storageCollection = {
  async bulkWrite(rows) {
    writeBatches.push(rows.slice());
    for (const row of rows) written.push(row);
  },
};

const backend = createMemoryMetaBackend();
let fetchCalls = 0;
const status = {};

async function fakeFetch({ fileId }) {
  fetchCalls += 1;
  await new Promise((r) => setTimeout(r, 3));
  return [
    { sequence: 0, bytesBase64: 'AAAA', hash: 'h0' },
    { sequence: 1, bytesBase64: 'BBBB', hash: 'h1' },
    { sequence: 2, bytesBase64: 'CCCC', hash: 'h2' },
  ];
}

const loader = createFileDemandLoader({
  collectionName: 'desktop_files',
  storageCollection,
  sidecarBackend: backend,
  requestFileFetch: fakeFetch,
  status,
});

const chunks = await loader.fetchFile('file-1');
assert(chunks.length === 3, 'all 3 chunks returned');
assert(fetchCalls === 1, 'fetch called once');
assert(status.activeFileStreams === 0, 'inflight back to zero');
assert(written.length === 3, 'three chunk rows written to primary store');
assert(writeBatches.length === 3, `chunks persist incrementally (got ${writeBatches.length} writes)`);
assert(writeBatches.every((batch) => batch.length === 1), 'incremental writes retain at most one stream chunk');
assert(written[0].file_id === 'file-1', 'chunk row has file_id');
assert(written[0].sequence === 0, 'chunk row has sequence');

// Second fetch concurrent must dedup.
fetchCalls = 0;
status.fileStreamDedupHits = 0;
const [a, b] = await Promise.all([loader.fetchFile('file-2'), loader.fetchFile('file-2')]);
assert(a.length === b.length, 'dedup returns same shape');
assert(fetchCalls === 1, `concurrent dedup → 1 fetch (got ${fetchCalls})`);
assert(status.fileStreamDedupHits === 1, `dedup hit recorded (got ${status.fileStreamDedupHits})`);

// Presence persists in sidecar.
const presenceRecord = await backend.getDocumentAccess('desktop_files', 'file-1-presence');
assert(presenceRecord !== null, 'sidecar must record presence for file-1');
assert(presenceRecord.fileChunkPresence.presentSequences.length === 3, 'all sequences recorded');
assert(presenceRecord.fileChunkPresence.expectedChunkCount === 3, 'expected count recorded');

// Range-specific in-flight keys: same file + different ranges must not share
// the same promise; same range with different key order should still dedup.
let rangeFetchCalls = 0;
const rangeLoader = createFileDemandLoader({
  collectionName: 'desktop_files',
  storageCollection,
  sidecarBackend: backend,
  persistChunks: false,
  requestFileFetch: async ({ range }) => {
    rangeFetchCalls += 1;
    await new Promise((r) => setTimeout(r, 3));
    return [{ sequence: Number(range?.offset || 0), bytesBase64: String(range?.offset || 0), hash: null }];
  },
  status,
});
const [firstRange, secondRange] = await Promise.all([
  rangeLoader.fetchFile('file-range', { range: { offset: 0, limit: 1 } }),
  rangeLoader.fetchFile('file-range', { range: { limit: 1, offset: 1 } }),
]);
assert(rangeFetchCalls === 2, `different ranges must not dedup (got ${rangeFetchCalls})`);
assert(firstRange[0].sequence === 0, 'first range result is distinct');
assert(secondRange[0].sequence === 1, 'second range result is distinct');
rangeFetchCalls = 0;
const [sameRangeA, sameRangeB] = await Promise.all([
  rangeLoader.fetchFile('file-range-same', { range: { offset: 2, limit: 1 } }),
  rangeLoader.fetchFile('file-range-same', { range: { limit: 1, offset: 2 } }),
]);
assert(rangeFetchCalls === 1, `same canonical range should dedup (got ${rangeFetchCalls})`);
assert(sameRangeA[0].sequence === sameRangeB[0].sequence, 'same range result is shared');

// Error path.
const failingLoader = createFileDemandLoader({
  collectionName: 'desktop_files',
  storageCollection,
  sidecarBackend: backend,
  requestFileFetch: async () => {
    throw new Error('peer offline');
  },
  status,
});
status.fileStreamErrors = 0;
let caught = null;
try {
  await failingLoader.fetchFile('file-error');
} catch (e) {
  caught = e;
}
assert(caught && /peer offline/.test(caught.message), 'peer error propagates');
assert(status.fileStreamErrors === 1, 'error counter bumped');

const boundedLoader = createFileDemandLoader({
  collectionName: 'desktop_files',
  storageCollection,
  sidecarBackend: backend,
  persistChunks: false,
  returnBudgetBytes: 2,
  requestFileFetch: async () => [{ sequence: 0, bytesBase64: 'AAAA', hash: null }],
  status,
});
const budgetError = await boundedLoader.fetchFile('file-too-large').catch((error) => error);
assert(budgetError?.code === 'FILE_RETURN_BUDGET_EXCEEDED', 'whole-file return is bounded and requires ranges');

// Abort path: in-flight dedup slot is released and the transport cancel hook
// receives the correlated request id so peer loss cannot leave a hanging file
// collector behind.
let cancelRequest = null;
let rejectHangingFetch = null;
const abortingLoader = createFileDemandLoader({
  collectionName: 'desktop_files',
  storageCollection,
  sidecarBackend: backend,
  requestFileFetch: () => new Promise((resolve, reject) => {
    rejectHangingFetch = reject;
  }),
  requestFileCancel: async ({ requestId, fileId, reason }) => {
    cancelRequest = { requestId, fileId, reason };
    rejectHangingFetch?.(Object.assign(new Error(`FILE_CANCELLED: ${reason}`), { code: 'FILE_CANCELLED' }));
  },
  status,
});
const hanging = abortingLoader.fetchFile('file-abort').catch((error) => error);
await new Promise((r) => setImmediate(r));
assert(abortingLoader.inflightSize() === 1, 'file abort: one in-flight before abort');
const aborted = await abortingLoader.abortAllInFlight('peer-close');
assert(aborted === 1, 'file abort: one in-flight cancelled');
assert(abortingLoader.inflightSize() === 0, 'file abort: in-flight map cleared immediately');
assert(cancelRequest?.fileId === 'file-abort', 'file abort: cancel hook receives file id');
assert(cancelRequest?.requestId?.startsWith('file-file-abort-'), 'file abort: cancel hook receives request id');
const abortError = await hanging;
assert(abortError?.code === 'FILE_CANCELLED', 'file abort: fetch promise rejects with cancellation');

console.log('ctox-rxdb-js file demand loader smoke OK');

function assert(c, m) { if (!c) throw new Error(m); }
