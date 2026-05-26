import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const modulePath = resolve(scriptDir, '../shared/file-integrity.js');
const moduleSource = readFileSync(modulePath, 'utf8');
const {
  CtoxFileChunkIntegrityError,
  FILE_CHUNK_ERROR_CODES,
  FILE_CHUNK_ERROR_PHASE,
  FILE_CHUNK_HASH_SCHEME,
  FILE_CONTENT_HASH_SCHEME,
  isDeletedChunk,
  readStoredFileFromChunks,
  sha256Hex,
} = await import(`data:text/javascript,${encodeURIComponent(`${moduleSource}\n//# sourceURL=${pathToFileURL(modulePath).href}`)}`);

const fileId = 'file_integrity_test';
const payload = 'hello world';
const base64 = Buffer.from(payload, 'utf8').toString('base64');
const first = base64.slice(0, 5);
const second = base64.slice(5);
const contentHash = await sha256Hex(new Uint8Array(Buffer.from(payload, 'utf8')));
const firstHash = await sha256Hex(first);
const secondHash = await sha256Hex(second);
const now = 42;

await assertBlobText('valid chunks', validChunks(), payload);
await assertBlobText('deleted stale generation ignored', [...deletedStaleChunks(), ...validChunks()], payload);
await assertThrows('missing chunk', validChunks().slice(0, 1), 'Dateiinhalt fehlt.', defaultOptions(), FILE_CHUNK_ERROR_CODES.MISSING);
await assertThrows(
  'active generation tombstoned',
  validChunks().map((chunk) => ({ ...chunk, _deleted: true })),
  'Dateiinhalt fehlt.',
  defaultOptions(),
  FILE_CHUNK_ERROR_CODES.MISSING
);
await assertThrows('requested generation missing', validChunks(), 'Dateiinhalt fehlt.', { contentGenerationId: 'gen_other' }, FILE_CHUNK_ERROR_CODES.MISSING);
await assertThrows(
  'content hash mismatch in chunk metadata',
  validChunks().map((chunk, idx) => (idx === 1 ? { ...chunk, content_hash: 'wrong' } : chunk)),
  'Dateiinhalt gehört zu einer falschen Generation.',
  defaultOptions(),
  FILE_CHUNK_ERROR_CODES.GENERATION_MISMATCH
);
await assertThrows(
  'chunk hash mismatch',
  validChunks().map((chunk, idx) => (idx === 0 ? { ...chunk, chunk_hash: 'wrong' } : chunk)),
  'Dateiinhalt ist unvollständig oder beschädigt.',
  defaultOptions(),
  FILE_CHUNK_ERROR_CODES.INTEGRITY_MISMATCH
);
await assertThrows(
  'chunk total mismatch',
  validChunks().map((chunk, idx) => (idx === 1 ? { ...chunk, total: 3 } : chunk)),
  'Dateiinhalt ist unvollständig oder beschädigt.',
  defaultOptions(),
  FILE_CHUNK_ERROR_CODES.INTEGRITY_MISMATCH
);
await assertThrows(
  'chunk encoded size mismatch',
  validChunks().map((chunk, idx) => (idx === 0 ? { ...chunk, size_bytes: chunk.size_bytes + 1 } : chunk)),
  'Dateiinhalt ist unvollständig oder beschädigt.',
  defaultOptions(),
  FILE_CHUNK_ERROR_CODES.INTEGRITY_MISMATCH
);
await assertThrows(
  'decoded content hash mismatch',
  validChunks().map(({ content_hash, ...chunk }) => chunk),
  'Dateiinhalt ist unvollständig oder beschädigt.',
  { contentGenerationId: 'gen_current', contentHash: 'wrong', contentHashScheme: FILE_CONTENT_HASH_SCHEME },
  FILE_CHUNK_ERROR_CODES.INTEGRITY_MISMATCH
);
await assertBlobText(
  'legacy chunks',
  validChunks().map(({ content_hash, content_hash_scheme, chunk_hash, chunk_hash_scheme, ...chunk }) => chunk),
  payload,
  { contentGenerationId: '', contentHash: '', contentHashScheme: '' }
);
assertDeletedChunkMarkers();

console.log('File viewer integrity contract OK');

function validChunks() {
  return [
    {
      id: `${fileId}_gen_current_0`,
      file_id: fileId,
      generation_id: 'gen_current',
      content_hash: contentHash,
      content_hash_scheme: FILE_CONTENT_HASH_SCHEME,
      idx: 0,
      total: 2,
      encoding: 'base64',
      data: first,
      chunk_hash: '',
      chunk_hash_scheme: FILE_CHUNK_HASH_SCHEME,
      size_bytes: first.length,
      created_at_ms: now,
    },
    {
      id: `${fileId}_gen_current_1`,
      file_id: fileId,
      generation_id: 'gen_current',
      content_hash: contentHash,
      content_hash_scheme: FILE_CONTENT_HASH_SCHEME,
      idx: 1,
      total: 2,
      encoding: 'base64',
      data: second,
      chunk_hash: '',
      chunk_hash_scheme: FILE_CHUNK_HASH_SCHEME,
      size_bytes: second.length,
      created_at_ms: now,
    },
  ].map(asyncChunkHash);
}

function deletedStaleChunks() {
  return validChunks().map((chunk) => ({
    ...chunk,
    id: chunk.id.replace('gen_current', 'gen_stale'),
    generation_id: 'gen_stale',
    content_hash: 'stale-content-hash',
    data: '',
    chunk_hash: '',
    size_bytes: 0,
    created_at_ms: now + 1,
    _deleted: true,
    prune_reason: 'stale_generation',
  }));
}

function asyncChunkHash(chunk) {
  return { ...chunk, chunk_hash: chunk.chunk_hash || knownChunkHash(chunk.data) };
}

function knownChunkHash(data) {
  if (data === first) return firstHash;
  if (data === second) return secondHash;
  throw new Error(`Unexpected chunk data in integrity fixture: ${data}`);
}

async function assertBlobText(label, chunks, expected, options = defaultOptions()) {
  const blob = await readStoredFileFromChunks(chunks, fileId, 'text/plain', options);
  const actual = await blob.text();
  if (actual !== expected) {
    throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

async function assertThrows(label, chunks, expectedMessage, options = defaultOptions(), expectedCode = '') {
  try {
    await readStoredFileFromChunks(chunks, fileId, 'text/plain', options);
  } catch (error) {
    const message = String(error?.message || error);
    if (!message.includes(expectedMessage)) {
      throw new Error(`${label}: expected ${expectedMessage}, got ${message}`);
    }
    if (!(error instanceof CtoxFileChunkIntegrityError)) {
      throw new Error(`${label}: expected CtoxFileChunkIntegrityError, got ${error?.name || typeof error}`);
    }
    if (error.phase !== FILE_CHUNK_ERROR_PHASE) {
      throw new Error(`${label}: expected phase ${FILE_CHUNK_ERROR_PHASE}, got ${error.phase}`);
    }
    if (expectedCode && error.code !== expectedCode) {
      throw new Error(`${label}: expected code ${expectedCode}, got ${error.code}`);
    }
    return;
  }
  throw new Error(`${label}: expected readStoredFileFromChunks to throw`);
}

function defaultOptions() {
  return {
    contentGenerationId: 'gen_current',
    contentHash,
    contentHashScheme: FILE_CONTENT_HASH_SCHEME,
  };
}

function assertDeletedChunkMarkers() {
  for (const marker of ['_deleted', 'deleted', 'is_deleted']) {
    if (!isDeletedChunk({ [marker]: true })) {
      throw new Error(`isDeletedChunk does not recognize ${marker}`);
    }
  }
  if (isDeletedChunk({ _deleted: false, deleted: false, is_deleted: false })) {
    throw new Error('isDeletedChunk rejected a live chunk');
  }
}
