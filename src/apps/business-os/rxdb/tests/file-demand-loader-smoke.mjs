import {
  createFileDemandLoader,
  createMemoryMetaBackend,
} from '../dist/ctox-rxdb-js.mjs';

const written = [];
const storageCollection = {
  async bulkWrite(rows) {
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

console.log('ctox-rxdb-js file demand loader smoke OK');

function assert(c, m) { if (!c) throw new Error(m); }
