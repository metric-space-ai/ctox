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
  requestFileCancel = null,
  status = null,
  clock = Date.now,
  persistChunks = true,
  // Origin stamp (object or provider fn): fetched chunk rows are master
  // state, not local writes — see query-demand-loader.mjs for the failure
  // modes an unstamped write causes (push echo, LWW veto of later pulls).
  replicationOrigin = null,
}) {
  if (!collectionName) throw new TypeError('file loader requires collectionName');
  if (!storageCollection) throw new TypeError('file loader requires storageCollection');
  if (!sidecarBackend) throw new TypeError('file loader requires sidecarBackend');
  if (typeof requestFileFetch !== 'function') {
    throw new TypeError('file loader requires requestFileFetch');
  }

  const inflight = new Map();
  let requestSequence = 0;
  const resolveReplicationOrigin = () => (
    (typeof replicationOrigin === 'function' ? replicationOrigin() : replicationOrigin) || null
  );

  return {
    async fetchFile(fileId, { range = null } = {}) {
      const inflightKey = fileInflightKey(fileId, range);
      if (inflight.has(inflightKey)) {
        bump(status, 'fileStreamDedupHits');
        return inflight.get(inflightKey).promise;
      }
      const startedAt = clock();
      const requestId = `file-${fileId}-${startedAt}-${++requestSequence}`;
      const job = (async () => {
        bump(status, 'activeFileStreams', 1);
        try {
          const presence = persistChunks ? await getPresence(sidecarBackend, collectionName, fileId) : null;
          const chunks = await requestFileFetch({
            requestId,
            collectionName,
            fileId,
            range,
            knownSequences: presence?.presentSequences || [],
          });
          if (!Array.isArray(chunks)) {
            throw new TypeError('requestFileFetch must return an array of chunks');
          }
          const validChunks = chunks.filter((chunk) => chunk && typeof chunk === 'object');
          for (const chunk of validChunks) bump(status, 'fileBytesReceived', (chunk?.bytesBase64?.length || 0));
          if (persistChunks) {
            const rows = validChunks.map((chunk) => ({
              id: `${fileId}-${chunk.sequence}`,
              file_id: fileId,
              sequence: chunk.sequence,
              bytes_base64: chunk.bytesBase64,
              hash: chunk.hash || null,
            }));
            if (rows.length) {
              await storageCollection.bulkWrite(rows, { replicationOrigin: resolveReplicationOrigin() });
            }
          }
          const sequences = validChunks.map((c) => c.sequence).sort((a, b) => a - b);
          if (persistChunks) {
            const highestSequence = sequences.length ? Math.max(...sequences) : -1;
            const expectedTotal = Math.max(
              highestSequence,
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
          }
          if (status) status.lastFileFetchMs = clock() - startedAt;
          return chunks;
        } catch (error) {
          bump(status, 'fileStreamErrors');
          throw error;
        } finally {
          bump(status, 'activeFileStreams', -1);
          if (inflight.get(inflightKey)?.requestId === requestId) {
            inflight.delete(inflightKey);
          }
        }
      })();
      inflight.set(inflightKey, { promise: job, requestId, fileId, range });
      return job;
    },
    inflightSize() {
      return inflight.size;
    },
    async abortAllInFlight(reason = 'reconnect') {
      const slots = [...inflight.values()];
      inflight.clear();
      for (const slot of slots) {
        try {
          slot.promise?.catch?.(() => {});
        } catch {}
        if (typeof requestFileCancel === 'function') {
          try {
            await requestFileCancel({
              requestId: slot.requestId,
              fileId: slot.fileId,
              range: slot.range,
              reason,
            });
          } catch {
            // best-effort cancel
          }
        }
      }
      return slots.length;
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

function fileInflightKey(fileId, range) {
  return `${String(fileId || '')}|${canonicalRangeKey(range)}`;
}

function canonicalRangeKey(range) {
  if (range == null) return 'full';
  if (Array.isArray(range)) return `[${range.map(canonicalRangeKey).join(',')}]`;
  if (typeof range === 'object') {
    return `{${Object.keys(range).sort().map((key) => `${key}:${canonicalRangeKey(range[key])}`).join(',')}}`;
  }
  return JSON.stringify(range);
}

function dedupeSorted(values) {
  const sorted = values.slice().sort((a, b) => a - b);
  const out = [];
  for (const v of sorted) {
    if (out.length === 0 || out[out.length - 1] !== v) out.push(v);
  }
  return out;
}
