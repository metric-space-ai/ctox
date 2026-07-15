export const DOCX_MIME_TYPE = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
export const DOCUMENT_BLOB_CHUNK_BASE64_SIZE = 256000;

const DOCUMENT_COLLECTION_NAMES = [
  'documents',
  'document_versions',
  'document_blob_chunks',
];

export function createDocumentsFacade({
  db,
  openApp,
  appId = 'documents',
  crypto: cryptoProvider = globalThis.crypto,
  now = () => Date.now(),
} = {}) {
  const pendingCreates = new Map();

  return Object.freeze({
    async loadVersion(request = {}) {
      const collections = requireDocumentCollections(db);
      return loadStoredVersion(collections, request, cryptoProvider);
    },

    async createDocx(input = {}) {
      const normalized = await normalizeCreateDocxInput(input, cryptoProvider);
      const fingerprint = canonicalJson({
        filename: normalized.filename,
        title: normalized.title,
        ownerId: normalized.ownerId,
        sha256: normalized.sha256,
        linkedRecords: normalized.linkedRecords,
        templateRef: normalized.templateRef,
        provenance: normalized.provenance,
      });
      const pending = pendingCreates.get(normalized.idempotencyKey);
      if (pending) {
        if (pending.fingerprint !== fingerprint) {
          throw documentsError(
            'DOCUMENTS_IDEMPOTENCY_CONFLICT',
            'idempotencyKey is already in use for a different DOCX payload.',
          );
        }
        return pending.promise;
      }

      const promise = createStoredDocx({
        collections: requireDocumentCollections(db, { write: true }),
        input: normalized,
        cryptoProvider,
        now,
      });
      pendingCreates.set(normalized.idempotencyKey, { fingerprint, promise });
      try {
        return await promise;
      } finally {
        if (pendingCreates.get(normalized.idempotencyKey)?.promise === promise) {
          pendingCreates.delete(normalized.idempotencyKey);
        }
      }
    },

    async open({ documentId, versionId } = {}) {
      const record = requireId(documentId, 'documentId');
      const version = versionId == null || versionId === ''
        ? ''
        : requireId(versionId, 'versionId');
      if (typeof openApp !== 'function') {
        throw documentsError(
          'DOCUMENTS_LAUNCH_UNAVAILABLE',
          'The Business OS app launcher is unavailable.',
        );
      }
      const launched = await openApp(requireId(appId, 'appId'), {
        args: {
          record,
          ...(version ? { version } : {}),
        },
      });
      if (!launched) {
        throw documentsError(
          'DOCUMENTS_LAUNCH_UNAVAILABLE',
          'The Documents app is not installed or available in this workspace.',
        );
      }
      return launched;
    },
  });
}

async function createStoredDocx({ collections, input, cryptoProvider, now }) {
  const idHash = await sha256Hex(
    new TextEncoder().encode(`ctox-documents:create-docx:${input.idempotencyKey}`),
    cryptoProvider,
  );
  const documentId = `doc_${idHash.slice(0, 40)}`;
  const versionId = `${documentId}_v1`;
  const blobId = `${versionId}_blob`;
  const createdAtMs = finiteTimestamp(now());
  const base64 = bytesToBase64(input.bytes);
  const totalChunks = Math.ceil(base64.length / DOCUMENT_BLOB_CHUNK_BASE64_SIZE) || 1;
  const chunks = Array.from({ length: totalChunks }, (_, idx) => ({
    id: `${blobId}_${idx}`,
    blob_id: blobId,
    document_id: documentId,
    version_id: versionId,
    idx,
    total: totalChunks,
    mime_type: DOCX_MIME_TYPE,
    encoding: 'base64',
    data: base64.slice(
      idx * DOCUMENT_BLOB_CHUNK_BASE64_SIZE,
      (idx + 1) * DOCUMENT_BLOB_CHUNK_BASE64_SIZE,
    ),
    created_at_ms: createdAtMs,
  }));
  const version = {
    id: versionId,
    document_id: documentId,
    version: 1,
    source_kind: 'app_created_docx',
    blob_id: blobId,
    source_sha256: input.sha256,
    filename: input.filename,
    mime_type: DOCX_MIME_TYPE,
    idempotency_key: input.idempotencyKey,
    linked_records: cloneJson(input.linkedRecords),
    template_ref: cloneJson(input.templateRef),
    provenance: cloneJson(input.provenance),
    model_json: {},
    diagnostics: [],
    created_at_ms: createdAtMs,
    updated_at_ms: createdAtMs,
  };
  const document = {
    id: documentId,
    title: input.title,
    filename: input.filename,
    mime_type: DOCX_MIME_TYPE,
    status: 'Created',
    document_type: 'word_document',
    owner_id: input.ownerId,
    current_version_id: versionId,
    source_sha256: input.sha256,
    page_count: 0,
    diagnostics_count: 0,
    linked_records: cloneJson(input.linkedRecords),
    template_ref: cloneJson(input.templateRef),
    provenance: cloneJson(input.provenance),
    idempotency_key: input.idempotencyKey,
    display_cache: {},
    index_text: '',
    is_deleted: false,
    created_at_ms: createdAtMs,
    updated_at_ms: createdAtMs,
  };
  const expected = { document, version, chunks, sha256: input.sha256 };
  const existing = await inspectIdempotentState(collections, expected);
  if (existing) return creationResult(existing, true);

  try {
    await insertChunks(collections.document_blob_chunks, chunks);
    await collections.document_versions.insert(version);
    await collections.documents.insert(document);
  } catch (error) {
    await cleanupFailedCreate(collections, expected).catch(() => {});
    throw error;
  }

  const stored = await inspectIdempotentState(collections, expected);
  if (!stored) {
    throw documentsError(
      'DOCUMENTS_PARTIAL_STATE',
      'DOCX creation did not produce a complete document, version, and chunk set.',
    );
  }
  await verifyStoredBytes(stored, input.sha256, cryptoProvider);
  return creationResult(stored, false);
}

async function inspectIdempotentState(collections, expected) {
  const [documentDoc, versionDoc, chunkDocs] = await Promise.all([
    findOne(collections.documents, expected.document.id),
    findOne(collections.document_versions, expected.version.id),
    findChunks(collections.document_blob_chunks, expected.version.blob_id),
  ]);
  const document = documentDoc ? documentJson(documentDoc) : null;
  const version = versionDoc ? documentJson(versionDoc) : null;
  const chunks = chunkDocs.map(documentJson);
  const populated = Number(Boolean(document)) + Number(Boolean(version)) + Number(chunks.length > 0);
  if (populated === 0) return null;
  if (populated !== 3) {
    throw documentsError(
      'DOCUMENTS_PARTIAL_STATE',
      'A partial document, version, or chunk set already exists for this idempotencyKey.',
    );
  }

  assertIdempotentDocument(document, expected.document);
  assertIdempotentVersion(version, expected.version);
  const sortedChunks = assertChunkSet(chunks, {
    blobId: expected.version.blob_id,
    documentId: expected.document.id,
    versionId: expected.version.id,
    expectedTotal: expected.chunks.length,
  });
  if (canonicalJson(sortedChunks.map((chunk) => chunk.data))
      !== canonicalJson(expected.chunks.map((chunk) => chunk.data))) {
    throw documentsError(
      'DOCUMENTS_IDEMPOTENCY_CONFLICT',
      'Stored DOCX chunks differ from the payload for this idempotencyKey.',
    );
  }
  return { document, version, chunks: sortedChunks };
}

function assertIdempotentDocument(actual, expected) {
  assertEquivalentFields(actual, expected, [
    'id',
    'current_version_id',
    'filename',
    'mime_type',
    'source_sha256',
    'idempotency_key',
    'linked_records',
    'template_ref',
    'provenance',
  ]);
}

function assertIdempotentVersion(actual, expected) {
  assertEquivalentFields(actual, expected, [
    'id',
    'document_id',
    'blob_id',
    'filename',
    'mime_type',
    'source_sha256',
    'idempotency_key',
    'linked_records',
    'template_ref',
    'provenance',
  ]);
}

function assertEquivalentFields(actual, expected, fields) {
  const differs = fields.some((field) => canonicalJson(actual[field]) !== canonicalJson(expected[field]));
  if (differs) {
    throw documentsError(
      'DOCUMENTS_IDEMPOTENCY_CONFLICT',
      'Stored document metadata differs from the payload for this idempotencyKey.',
    );
  }
}

async function loadStoredVersion(collections, request, cryptoProvider) {
  const documentId = requireId(request.documentId, 'documentId');
  const expectedSha256 = requireSha256(request.expectedSha256, 'expectedSha256');
  const documentDoc = await findOne(collections.documents, documentId);
  if (!documentDoc) {
    throw documentsError('DOCUMENTS_NOT_FOUND', `Document ${documentId} was not found.`);
  }
  const document = documentJson(documentDoc);
  validateStoredDocument(document);
  const versionId = request.versionId == null || request.versionId === ''
    ? requireId(document.current_version_id, 'document.current_version_id')
    : requireId(request.versionId, 'versionId');
  const versionDoc = await findOne(collections.document_versions, versionId);
  if (!versionDoc) {
    throw documentsError('DOCUMENTS_NOT_FOUND', `Document version ${versionId} was not found.`);
  }
  const version = documentJson(versionDoc);
  if (version.document_id !== documentId) {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Document version belongs to another document.');
  }
  const blobId = requireId(version.blob_id, 'version.blob_id');
  const chunks = (await findChunks(collections.document_blob_chunks, blobId)).map(documentJson);
  const stored = { document, version, chunks };
  const verified = await verifyStoredBytes(stored, expectedSha256, cryptoProvider);
  return {
    document,
    version,
    bytes: verified.bytes,
    sha256: verified.sha256,
    filename: document.filename,
    mimeType: DOCX_MIME_TYPE,
  };
}

async function verifyStoredBytes(stored, expectedSha256, cryptoProvider) {
  validateStoredDocument(stored.document);
  if (stored.version.mime_type && stored.version.mime_type !== DOCX_MIME_TYPE) {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Document version has an invalid DOCX MIME type.');
  }
  const chunks = assertChunkSet(stored.chunks, {
    blobId: stored.version.blob_id,
    documentId: stored.document.id,
    versionId: stored.version.id,
  });
  const bytes = base64ToBytes(chunks.map((chunk) => chunk.data).join(''));
  if (!bytes.length) {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Stored DOCX bytes are empty.');
  }
  const sha256 = await sha256Hex(bytes, cryptoProvider);
  const storedVersionSha = normalizeOptionalSha256(stored.version.source_sha256);
  const storedDocumentSha = stored.document.current_version_id === stored.version.id
    ? normalizeOptionalSha256(stored.document.source_sha256)
    : '';
  if (sha256 !== expectedSha256
      || (storedVersionSha && sha256 !== storedVersionSha)
      || (storedDocumentSha && sha256 !== storedDocumentSha)) {
    throw documentsError('DOCUMENTS_HASH_MISMATCH', 'Stored DOCX bytes do not match the expected SHA-256.');
  }
  return { bytes, sha256 };
}

function validateStoredDocument(document) {
  requireId(document.id, 'document.id');
  validateFilename(document.filename);
  if (document.mime_type !== DOCX_MIME_TYPE) {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Document has an invalid DOCX MIME type.');
  }
}

function assertChunkSet(chunks, { blobId, documentId, versionId, expectedTotal } = {}) {
  if (!chunks.length) {
    throw documentsError('DOCUMENTS_PARTIAL_STATE', 'Document blob chunks are missing.');
  }
  const sorted = [...chunks].sort((left, right) => Number(left.idx) - Number(right.idx));
  const total = Number(sorted[0].total);
  if (!Number.isSafeInteger(total) || total <= 0 || total !== sorted.length
      || (expectedTotal != null && total !== expectedTotal)) {
    throw documentsError(
      'DOCUMENTS_PARTIAL_STATE',
      `Document blob chunk count is inconsistent (declared=${total}, found=${sorted.length}, expected=${expectedTotal ?? 'n/a'}).`,
    );
  }
  for (let idx = 0; idx < sorted.length; idx += 1) {
    const chunk = sorted[idx];
    if (chunk.idx !== idx
        || chunk.total !== total
        || chunk.blob_id !== blobId
        || chunk.document_id !== documentId
        || chunk.version_id !== versionId
        || chunk.mime_type !== DOCX_MIME_TYPE
        || chunk.encoding !== 'base64'
        || typeof chunk.data !== 'string') {
      throw documentsError('DOCUMENTS_PARTIAL_STATE', 'Document blob chunk metadata is inconsistent.');
    }
  }
  return sorted;
}

async function normalizeCreateDocxInput(input, cryptoProvider) {
  if (!input || typeof input !== 'object' || Array.isArray(input)) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'createDocx input must be an object.');
  }
  if (input.mimeType !== DOCX_MIME_TYPE) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `mimeType must be ${DOCX_MIME_TYPE}.`);
  }
  const filename = validateFilename(input.filename);
  const bytes = await normalizeBytes(input.bytes);
  if (!bytes.length) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'DOCX bytes must not be empty.');
  }
  const idempotencyKey = requireText(input.idempotencyKey, 'idempotencyKey', 512);
  const title = input.title == null || input.title === ''
    ? filename.replace(/\.docx$/i, '')
    : requireText(input.title, 'title', 512);
  const ownerId = input.ownerId == null ? '' : requireText(input.ownerId, 'ownerId', 256, { allowEmpty: true });
  const linkedRecords = input.linkedRecords == null
    ? []
    : cloneJsonField(input.linkedRecords, 'linkedRecords');
  if (!Array.isArray(linkedRecords) || linkedRecords.some((entry) => !isPlainObject(entry))) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'linkedRecords must be an array of objects.');
  }
  const templateRef = input.templateRef == null
    ? null
    : cloneJsonField(input.templateRef, 'templateRef');
  const provenance = input.provenance == null
    ? {}
    : cloneJsonField(input.provenance, 'provenance');
  return {
    bytes,
    filename,
    title,
    ownerId,
    idempotencyKey,
    linkedRecords,
    templateRef,
    provenance,
    sha256: await sha256Hex(bytes, cryptoProvider),
  };
}

function requireDocumentCollections(db, { write = false } = {}) {
  const collections = Object.fromEntries(DOCUMENT_COLLECTION_NAMES.map((name) => [
    name,
    resolveCollection(db, name),
  ]));
  for (const name of DOCUMENT_COLLECTION_NAMES) {
    const collection = collections[name];
    const canRead = collection
      && typeof collection.findOne === 'function'
      && (name !== 'document_blob_chunks' || typeof collection.find === 'function');
    const canWrite = !write
      || (typeof collection?.insert === 'function'
        && (name !== 'document_blob_chunks'
          || typeof collection.bulkInsert === 'function'
          || typeof collection.insert === 'function'));
    if (!canRead || !canWrite) {
      throw documentsError(
        'DOCUMENTS_COLLECTIONS_UNAVAILABLE',
        `Required Business OS collection is unavailable: ${name}.`,
      );
    }
  }
  return collections;
}

function resolveCollection(db, name) {
  return db?.collection?.(name)
    || db?.collections?.[name]
    || db?.raw?.[name]
    || null;
}

async function findOne(collection, id) {
  const query = collection.findOne(id);
  if (!query || typeof query.exec !== 'function') {
    throw documentsError('DOCUMENTS_COLLECTIONS_UNAVAILABLE', 'RxDB findOne().exec() is required.');
  }
  return query.exec();
}

async function findChunks(collection, blobId) {
  const query = collection.find({
    selector: { blob_id: blobId },
    sort: [{ idx: 'asc' }],
  });
  if (!query || typeof query.exec !== 'function') {
    throw documentsError('DOCUMENTS_COLLECTIONS_UNAVAILABLE', 'RxDB find().exec() is required.');
  }
  const chunks = await query.exec();
  return Array.isArray(chunks) ? chunks : [];
}

async function insertChunks(collection, chunks) {
  if (typeof collection.bulkInsert === 'function') {
    await collection.bulkInsert(chunks);
    return;
  }
  for (const chunk of chunks) await collection.insert(chunk);
}

async function cleanupFailedCreate(collections, expected) {
  await removeMatchingDocument(collections.documents, expected.document.id, (document) => (
    document.idempotency_key === expected.document.idempotency_key
  ));
  await removeMatchingDocument(collections.document_versions, expected.version.id, (version) => (
    version.idempotency_key === expected.version.idempotency_key
  ));
  for (const expectedChunk of expected.chunks) {
    await removeMatchingDocument(collections.document_blob_chunks, expectedChunk.id, (chunk) => (
      chunk.blob_id === expectedChunk.blob_id && chunk.data === expectedChunk.data
    ));
  }
}

async function removeMatchingDocument(collection, id, matches) {
  const doc = await findOne(collection, id);
  if (!doc || !matches(documentJson(doc)) || typeof doc.remove !== 'function') return;
  await doc.remove();
}

function creationResult(stored, idempotent) {
  return {
    documentId: stored.document.id,
    versionId: stored.version.id,
    sha256: stored.version.source_sha256 || stored.document.source_sha256,
    document: cloneJson(stored.document),
    version: cloneJson(stored.version),
    idempotent,
  };
}

async function normalizeBytes(value) {
  if (value instanceof Uint8Array) return value.slice();
  if (value instanceof ArrayBuffer) return new Uint8Array(value.slice(0));
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength));
  }
  if (typeof Blob === 'function' && value instanceof Blob) {
    return new Uint8Array(await value.arrayBuffer());
  }
  throw documentsError(
    'DOCUMENTS_INVALID_INPUT',
    'bytes must be a Uint8Array, ArrayBuffer, ArrayBuffer view, or Blob.',
  );
}

function validateFilename(value) {
  const filename = requireText(value, 'filename', 255);
  if (!/\.docx$/i.test(filename)
      || filename.includes('/')
      || filename.includes('\\')
      || /[\u0000-\u001f\u007f]/.test(filename)) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'filename must be a safe .docx filename.');
  }
  return filename;
}

function requireId(value, field) {
  const id = requireText(value, field, 180);
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(id)) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `${field} contains invalid characters.`);
  }
  return id;
}

function requireText(value, field, maxLength, { allowEmpty = false } = {}) {
  if (typeof value !== 'string') {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `${field} must be a string.`);
  }
  const normalized = value.trim();
  if ((!allowEmpty && !normalized) || normalized.length > maxLength) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `${field} is empty or too long.`);
  }
  return normalized;
}

function requireSha256(value, field) {
  if (typeof value !== 'string' || !/^[a-fA-F0-9]{64}$/.test(value.trim())) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', `${field} must be a SHA-256 hex digest.`);
  }
  return value.trim().toLowerCase();
}

function normalizeOptionalSha256(value) {
  if (value == null || value === '') return '';
  return requireSha256(value, 'stored source_sha256');
}

function finiteTimestamp(value) {
  const timestamp = Number(value);
  if (!Number.isFinite(timestamp) || timestamp < 0) {
    throw documentsError('DOCUMENTS_INVALID_INPUT', 'now() must return a finite timestamp.');
  }
  return timestamp;
}

async function sha256Hex(bytes, cryptoProvider) {
  if (!cryptoProvider?.subtle?.digest) {
    throw documentsError('DOCUMENTS_CRYPTO_UNAVAILABLE', 'WebCrypto SHA-256 is unavailable.');
  }
  const digest = await cryptoProvider.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
}

function bytesToBase64(bytes) {
  if (typeof btoa !== 'function') {
    throw documentsError('DOCUMENTS_CRYPTO_UNAVAILABLE', 'Base64 encoding is unavailable.');
  }
  let binary = '';
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

function base64ToBytes(base64) {
  if (typeof atob !== 'function'
      || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(base64)) {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Stored document chunks are not valid Base64.');
  }
  let binary;
  try {
    binary = atob(base64);
  } catch {
    throw documentsError('DOCUMENTS_INTEGRITY_ERROR', 'Stored document chunks are not valid Base64.');
  }
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

function cloneJsonField(value, field) {
  try {
    assertJsonValue(value, new Set());
    return cloneJson(value);
  } catch (error) {
    if (error?.code === 'DOCUMENTS_INVALID_INPUT') throw error;
    throw documentsError('DOCUMENTS_INVALID_INPUT', `${field} must be JSON-compatible.`);
  }
}

function assertJsonValue(value, seen) {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return;
  if (typeof value === 'number' && Number.isFinite(value)) return;
  if (Array.isArray(value)) {
    if (seen.has(value)) throw new Error('cyclic JSON value');
    seen.add(value);
    for (const entry of value) assertJsonValue(entry, seen);
    seen.delete(value);
    return;
  }
  if (isPlainObject(value)) {
    if (seen.has(value)) throw new Error('cyclic JSON value');
    seen.add(value);
    for (const entry of Object.values(value)) assertJsonValue(entry, seen);
    seen.delete(value);
    return;
  }
  throw new Error('non-JSON value');
}

function isPlainObject(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function cloneJson(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

function canonicalJson(value) {
  if (value === undefined) return 'undefined';
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
}

function documentJson(document) {
  return typeof document?.toJSON === 'function' ? document.toJSON() : { ...document };
}

function documentsError(code, message) {
  const error = new Error(message);
  error.name = 'DocumentsFacadeError';
  error.code = code;
  return error;
}
