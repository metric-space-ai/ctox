export const FILE_CONTENT_HASH_SCHEME = 'sha256-bytes-v1';
export const FILE_CHUNK_HASH_SCHEME = 'sha256-base64-chunk-v1';
export const FILE_CHUNK_ERROR_PHASE = 'file-chunk-reconstruct';
export const FILE_CHUNK_ERROR_CODES = Object.freeze({
  MISSING: 'ctox_file_chunk_missing',
  GENERATION_MISMATCH: 'ctox_file_chunk_generation_mismatch',
  INTEGRITY_MISMATCH: 'ctox_file_chunk_integrity_mismatch',
});

export class CtoxFileChunkIntegrityError extends Error {
  constructor(code, message, details = {}) {
    super(message);
    this.name = 'CtoxFileChunkIntegrityError';
    this.code = code;
    this.phase = FILE_CHUNK_ERROR_PHASE;
    this.details = details;
  }
}

export async function readStoredFileFromChunks(chunks, fileId, mimeType = 'application/octet-stream', options = {}) {
  const normalized = normalizeOptions(options);
  const allChunks = chunks.filter((chunk) => chunk.file_id === fileId && !isDeletedChunk(chunk));
  const generation = selectActiveChunkGeneration(allChunks, normalized.contentGenerationId);
  const total = Number(generation[0]?.total || generation.length || 0);
  if (!generation.length || total <= 0) {
    throw fileChunkError(FILE_CHUNK_ERROR_CODES.MISSING, 'Dateiinhalt fehlt.', { fileId });
  }

  const ordered = generation
    .filter((chunk) => Number(chunk.idx) < total)
    .sort((a, b) => Number(a.idx) - Number(b.idx));
  if (ordered.length !== total || ordered.some((chunk, idx) => Number(chunk.idx) !== idx)) {
    throw fileChunkError(FILE_CHUNK_ERROR_CODES.MISSING, 'Dateiinhalt fehlt.', { fileId, total, available: ordered.length });
  }

  validateGenerationContract(ordered, normalized.contentGenerationId, normalized.contentHash);
  await validateChunkHashes(ordered);
  const bytes = base64ToBytes(ordered.map((chunk) => chunk.data).join(''));
  await validateContentHash(bytes, normalized.contentHash, normalized.contentHashScheme);
  return new Blob([bytes], { type: mimeType || 'application/octet-stream' });
}

export function isDeletedChunk(chunk) {
  return chunk?._deleted === true || chunk?.deleted === true || chunk?.is_deleted === true;
}

export function selectActiveChunkGeneration(chunks, contentGenerationId = '') {
  if (contentGenerationId) return chunks.filter((chunk) => chunk.generation_id === contentGenerationId);
  const latestCreatedAt = Math.max(0, ...chunks.map((chunk) => Number(chunk.created_at_ms || 0)));
  return chunks.filter((chunk) => Number(chunk.created_at_ms || 0) === latestCreatedAt);
}

export function base64ToBytes(base64) {
  const binary = atob(String(base64 || ''));
  const bytes = new Uint8Array(binary.length);
  for (let idx = 0; idx < binary.length; idx += 1) bytes[idx] = binary.charCodeAt(idx);
  return bytes;
}

export async function sha256Hex(value) {
  const bytes = value instanceof Uint8Array ? value : new TextEncoder().encode(String(value ?? ''));
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function normalizeOptions(options) {
  if (typeof options === 'string') {
    return { contentGenerationId: options, contentHash: '', contentHashScheme: '' };
  }
  return {
    contentGenerationId: String(options?.contentGenerationId || options?.content_generation_id || ''),
    contentHash: String(options?.contentHash || options?.content_hash || ''),
    contentHashScheme: String(options?.contentHashScheme || options?.content_hash_scheme || ''),
  };
}

function validateGenerationContract(chunks, contentGenerationId, expectedContentHash) {
  if (contentGenerationId && chunks.some((chunk) => chunk.generation_id && chunk.generation_id !== contentGenerationId)) {
    throw fileChunkError(FILE_CHUNK_ERROR_CODES.GENERATION_MISMATCH, 'Dateiinhalt gehört zu einer falschen Generation.', {
      contentGenerationId,
    });
  }
  if (expectedContentHash && chunks.some((chunk) => chunk.content_hash && chunk.content_hash !== expectedContentHash)) {
    throw fileChunkError(FILE_CHUNK_ERROR_CODES.GENERATION_MISMATCH, 'Dateiinhalt gehört zu einer falschen Generation.', {
      contentHash: expectedContentHash,
    });
  }
}

async function validateChunkHashes(chunks) {
  for (const chunk of chunks) {
    if (chunk.chunk_hash_scheme === FILE_CHUNK_HASH_SCHEME && chunk.chunk_hash) {
      const actualChunkHash = await sha256Hex(String(chunk.data || ''));
      if (actualChunkHash !== chunk.chunk_hash) {
        throw fileChunkError(FILE_CHUNK_ERROR_CODES.INTEGRITY_MISMATCH, 'Dateiinhalt ist unvollständig oder beschädigt.', {
          chunkId: chunk.id || '',
        });
      }
    }
  }
}

async function validateContentHash(bytes, expectedContentHash, contentHashScheme) {
  if (contentHashScheme !== FILE_CONTENT_HASH_SCHEME || !expectedContentHash) return;
  const actualContentHash = await sha256Hex(bytes);
  if (actualContentHash !== expectedContentHash) {
    throw fileChunkError(FILE_CHUNK_ERROR_CODES.INTEGRITY_MISMATCH, 'Dateiinhalt ist unvollständig oder beschädigt.', {
      contentHash: expectedContentHash,
      actualContentHash,
    });
  }
}

function fileChunkError(code, message, details = {}) {
  return new CtoxFileChunkIntegrityError(code, message, details);
}
