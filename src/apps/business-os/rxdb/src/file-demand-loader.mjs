// V1.5 file demand loader.
//
// Streams large files over WebRTC in chunks. File metadata lives in the
// primary documents store like every other RxDB record. Chunk completeness
// (which sequences are locally present and verified) is tracked in the
// Sidecar so reloads can resume without re-fetching what's already there.

export const FILE_CHUNK_PRESENCE_KEY = (collection, fileId) => `${collection}|${fileId}`;
export const DEFAULT_FILE_RETURN_BUDGET_BYTES = 32 * 1024 * 1024;

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
  returnBudgetBytes = DEFAULT_FILE_RETURN_BUDGET_BYTES,
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
          const validChunks = [];
          const consumedSequences = new Set();
          let returnedBytes = 0;
          const consumeChunk = async (chunk) => {
            if (!chunk || typeof chunk !== 'object' || chunk.complete && chunk.sequence == null) return;
            const sequence = Number(chunk.sequence);
            if (!Number.isFinite(sequence) || consumedSequences.has(sequence)) return;
            const bytesBase64 = String(chunk.bytesBase64 || '');
            const decodedBytes = Math.floor(bytesBase64.length * 3 / 4);
            returnedBytes += decodedBytes;
            if (returnedBytes > returnBudgetBytes) {
              const error = new Error(`FILE_RETURN_BUDGET_EXCEEDED: ${returnedBytes} > ${returnBudgetBytes}; request a byte range`);
              error.code = 'FILE_RETURN_BUDGET_EXCEEDED';
              error.retryable = false;
              throw error;
            }
            consumedSequences.add(sequence);
            const normalized = {
              sequence,
              bytesBase64,
              hash: chunk.hash || null,
            };
            validChunks.push(normalized);
            bump(status, 'fileBytesReceived', bytesBase64.length);
            if (persistChunks) {
              await storageCollection.bulkWrite([{
                id: `${fileId}-${sequence}`,
                file_id: fileId,
                sequence,
                bytes_base64: bytesBase64,
                hash: normalized.hash,
              }], { replicationOrigin: resolveReplicationOrigin() });
            }
          };
          const chunks = await requestFileFetch({
            requestId,
            collectionName,
            fileId,
            range,
            knownSequences: presence?.presentSequences || [],
            onChunk: consumeChunk,
          });
          if (!Array.isArray(chunks)) {
            throw new TypeError('requestFileFetch must return an array of chunks');
          }
          for (const chunk of chunks) {
            await consumeChunk(chunk);
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
          return validChunks.sort((left, right) => left.sequence - right.sequence);
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
