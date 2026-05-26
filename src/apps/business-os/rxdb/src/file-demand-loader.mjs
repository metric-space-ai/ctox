// V1.5 file demand loader.
//
// Streams large files over WebRTC in chunks. File metadata lives in the
// primary documents store like every other RxDB record. Chunk completeness
// (which sequences are locally present and verified) is tracked in the
// Sidecar so reloads can resume without re-fetching what's already there.

export const FILE_CHUNK_PRESENCE_KEY = (collection, fileId) => `${collection}|${fileId}`;

export function createFileDemandLoader({
  collectionName,
  storageCollection,
  sidecarBackend,
  requestFileFetch,
  status = null,
  clock = Date.now,
}) {
  if (!collectionName) throw new TypeError('file loader requires collectionName');
  if (!storageCollection) throw new TypeError('file loader requires storageCollection');
  if (!sidecarBackend) throw new TypeError('file loader requires sidecarBackend');
  if (typeof requestFileFetch !== 'function') {
    throw new TypeError('file loader requires requestFileFetch');
  }

  const inflight = new Map();

  return {
    async fetchFile(fileId, { range = null } = {}) {
      if (inflight.has(fileId)) {
        bump(status, 'fileStreamDedupHits');
        return inflight.get(fileId);
      }
      const job = (async () => {
        const startedAt = clock();
        bump(status, 'activeFileStreams', 1);
        try {
          const presence = await getPresence(sidecarBackend, collectionName, fileId);
          const chunks = await requestFileFetch({
            requestId: `file-${fileId}-${startedAt}`,
            collectionName,
            fileId,
            range,
            knownSequences: presence?.presentSequences || [],
          });
          if (!Array.isArray(chunks)) {
            throw new TypeError('requestFileFetch must return an array of chunks');
          }
          for (const chunk of chunks) {
            if (!chunk || typeof chunk !== 'object') continue;
            await storageCollection.bulkWrite([
              {
                id: `${fileId}-${chunk.sequence}`,
                file_id: fileId,
                sequence: chunk.sequence,
                bytes_base64: chunk.bytesBase64,
                hash: chunk.hash || null,
              },
            ]);
            bump(status, 'fileBytesReceived', (chunk.bytesBase64?.length || 0));
          }
          const sequences = chunks.map((c) => c.sequence).sort((a, b) => a - b);
          const expectedTotal = Math.max(
            ...sequences,
            presence?.expectedChunkCount || 0,
          ) + 1;
          await sidecarBackend.putDocumentAccess({
            collection: collectionName,
            id: `${fileId}-presence`,
            lastAccessedAt: clock(),
            pinReason: 'file-chunks',
            dirty: false,
            estimatedBytes: 0,
          });
          await putPresence(sidecarBackend, collectionName, fileId, {
            collection: collectionName,
            fileId,
            expectedChunkCount: expectedTotal,
            presentSequences: dedupeSorted([
              ...(presence?.presentSequences || []),
              ...sequences,
            ]),
            lastVerifiedAt: clock(),
          });
          if (status) status.lastFileFetchMs = clock() - startedAt;
          return chunks;
        } catch (error) {
          bump(status, 'fileStreamErrors');
          throw error;
        } finally {
          bump(status, 'activeFileStreams', -1);
          inflight.delete(fileId);
        }
      })();
      inflight.set(fileId, job);
      return job;
    },
    inflightSize() {
      return inflight.size;
    },
  };
}

async function getPresence(backend, collection, fileId) {
  // Stored as a special documentAccess record with id `${fileId}-presence`.
  const record = await backend.getDocumentAccess(collection, `${fileId}-presence`);
  if (!record || !record.fileChunkPresence) return null;
  return record.fileChunkPresence;
}

async function putPresence(backend, collection, fileId, presence) {
  await backend.putDocumentAccess({
    collection,
    id: `${fileId}-presence`,
    lastAccessedAt: presence.lastVerifiedAt,
    pinReason: 'file-chunks',
    dirty: false,
    estimatedBytes: 0,
    fileChunkPresence: presence,
  });
}

function bump(status, field, delta = 1) {
  if (!status) return;
  if (typeof status[field] !== 'number') status[field] = 0;
  status[field] += delta;
}

function dedupeSorted(values) {
  const sorted = values.slice().sort((a, b) => a - b);
  const out = [];
  for (const v of sorted) {
    if (out.length === 0 || out[out.length - 1] !== v) out.push(v);
  }
  return out;
}
