const KIND_CONFIG = Object.freeze({
  document: {
    module: 'documents',
    records: 'documents',
    versions: 'document_versions',
    chunks: 'document_blob_chunks',
    recordIdField: 'document_id',
    mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  },
  spreadsheet: {
    module: 'spreadsheets',
    records: 'spreadsheets',
    versions: 'spreadsheet_versions',
    chunks: 'spreadsheet_blob_chunks',
    recordIdField: 'spreadsheet_id',
    mime: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  },
});

export function createBusinessOsOfficeBridge(ctx, kind) {
  const config = KIND_CONFIG[kind];
  if (!config) throw new TypeError(`Unsupported CTOX product bridge kind: ${kind}`);
  const collection = (name) => {
    const value = ctx?.db?.collection?.(name);
    if (!value) throw new Error(`CTOX product collection is unavailable: ${name}`);
    return value;
  };
  const canWrite = () => ctx?.permissions?.canWriteCollection?.(config.records) !== false
    && ctx?.permissions?.canWriteCollection?.(config.versions) !== false
    && ctx?.permissions?.canWriteCollection?.(config.chunks) !== false;

  return Object.freeze({
    async loadVersion({ recordId, versionId } = {}) {
      const recordDoc = await collection(config.records).findOne(String(recordId || '')).exec();
      const record = toJson(recordDoc);
      if (!record) throw new Error(`${kind} record was not found: ${recordId}`);
      const resolvedVersionId = String(versionId || record.current_version_id || '');
      const versionDoc = resolvedVersionId ? await collection(config.versions).findOne(resolvedVersionId).exec() : null;
      const version = toJson(versionDoc);
      if (!version) throw new Error(`${kind} version was not found: ${resolvedVersionId}`);
      const canonicalBytes = await loadBlob(collection(config.chunks), version.blob_id, version.source_sha256);
      const editorBytes = version.editor_blob_id
        ? await loadBlob(collection(config.chunks), version.editor_blob_id, version.editor_sha256)
        : null;
      return { record, version, canonicalBytes, editorBytes };
    },

    async prepare({ recordId, versionId } = {}) {
      await awaitNativeSync(ctx, [config.records, config.versions, config.chunks]);
      return dispatch(ctx, config, 'prepare', recordId, {
        [`${kind}_id`]: recordId,
        version_id: versionId,
      });
    },

    async commit({ recordId, baseVersionId, editorProtocol, editorProtocolVersion, implementedFeatures, reason, bytes } = {}) {
      if (!canWrite()) throw permissionError('CTOX product write permission is required');
      const payloadBytes = normalizeBytes(bytes);
      const editorBlobId = `office_${kind}_${crypto.randomUUID()}`;
      await saveBlob(collection(config.chunks), config, {
        blobId: editorBlobId,
        recordId,
        versionId: baseVersionId,
        bytes: payloadBytes,
      });
      const editorSha256 = await sha256Hex(payloadBytes);
      await awaitNativeSync(ctx, [config.chunks]);
      return dispatch(ctx, config, 'commit', recordId, {
        [`${kind}_id`]: recordId,
        base_version_id: baseVersionId,
        editor_blob_id: editorBlobId,
        editor_protocol: editorProtocol,
        editor_protocol_version: editorProtocolVersion,
        editor_sha256: editorSha256,
        implemented_features: Array.isArray(implementedFeatures) ? implementedFeatures : [],
        reason,
      });
    },

    async export({ recordId, versionId, format } = {}) {
      await awaitNativeSync(ctx, [config.records, config.versions, config.chunks]);
      const result = await dispatch(ctx, config, 'export', recordId, {
        [`${kind}_id`]: recordId,
        version_id: versionId,
        format,
      });
      const bytes = await loadBlob(collection(config.chunks), result.blob_id, result.source_sha256);
      return { ...result, bytes };
    },

    reportIntegrityError(details = {}) {
      ctx?.reportFileIntegrityError?.(new Error(details.message || 'CTOX product integrity error'), {
        kind,
        code: details.code || 'office_integrity_error',
        ...details,
      });
      return { reported: true };
    },
  });
}

async function dispatch(ctx, config, operation, recordId, payload) {
  if (typeof ctx?.commandBus?.dispatch !== 'function') throw new Error('CTOX product command bus is unavailable');
  const result = await ctx.commandBus.dispatch({
    id: `cmd_office_${crypto.randomUUID()}`,
    module: config.module,
    type: `office.${config.module === 'documents' ? 'document' : 'spreadsheet'}.${operation}`,
    record_id: String(recordId || ''),
    payload,
    client_context: {
      source: 'ctox-office-esm',
      surface: `business-os-${config.module}`,
      transport: 'rxdb-webrtc',
    },
  }, { until: 'terminal' });
  const outcome = result?.payload?.outcome
    || result?.result?.outcome
    || result?.outcome
    || result?.result
    || result;
  if (outcome?.ok === false || result?.status === 'failed') {
    const error = new Error(outcome?.error || `CTOX product ${operation} failed`);
    error.code = outcome?.error_code || 'office_command_failed';
    throw error;
  }
  return outcome;
}

async function loadBlob(chunks, blobId, expectedSha256 = '') {
  if (!blobId) return null;
  const docs = await chunks.find({ selector: { blob_id: blobId }, sort: [{ idx: 'asc' }] }).exec();
  const rows = docs.map(toJson).filter(Boolean).sort((a, b) => Number(a.idx) - Number(b.idx));
  if (!rows.length) throw integrityError(`Blob has no chunks: ${blobId}`, 'blob_missing');
  const total = Number(rows[0].total);
  if (!Number.isInteger(total) || total < 1 || rows.length !== total) {
    throw integrityError(`Blob chunk set is incomplete: ${blobId}`, 'blob_incomplete');
  }
  for (let idx = 0; idx < rows.length; idx += 1) {
    if (Number(rows[idx].idx) !== idx || Number(rows[idx].total) !== total || rows[idx].blob_id !== blobId) {
      throw integrityError(`Blob chunk ordering is invalid: ${blobId}`, 'blob_invalid');
    }
  }
  const decoded = rows.map((row) => base64ToUint8(row.data || ''));
  const length = decoded.reduce((sum, bytes) => sum + bytes.length, 0);
  const result = new Uint8Array(length);
  let offset = 0;
  for (const bytes of decoded) { result.set(bytes, offset); offset += bytes.length; }
  const expected = String(expectedSha256 || '').trim().toLowerCase();
  if (expected && await sha256Hex(result) !== expected) {
    throw integrityError(`Blob hash does not match its version metadata: ${blobId}`, 'blob_hash_mismatch');
  }
  return result;
}

async function saveBlob(chunks, config, { blobId, recordId, versionId, bytes }) {
  const chunkSize = 256000;
  const total = Math.max(1, Math.ceil(bytes.length / chunkSize));
  const now = Date.now();
  const rows = [];
  for (let idx = 0; idx < total; idx += 1) {
    const chunk = bytes.subarray(idx * chunkSize, Math.min(bytes.length, (idx + 1) * chunkSize));
    rows.push({
      id: `${blobId}_${String(idx).padStart(4, '0')}`,
      blob_id: blobId,
      [config.recordIdField]: recordId,
      version_id: versionId,
      idx,
      total,
      mime_type: config.mime,
      encoding: 'base64',
      data: uint8ToBase64(chunk),
      created_at_ms: now,
    });
  }
  if (typeof chunks.bulkUpsert === 'function') await chunks.bulkUpsert(rows);
  else for (const row of rows) await chunks.incrementalUpsert(row);
}

function normalizeBytes(value) {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  throw new TypeError('CTOX product commit requires bytes');
}

function toJson(doc) { return doc?.toJSON?.() || doc || null; }

function uint8ToBase64(bytes) {
  let binary = '';
  for (let offset = 0; offset < bytes.length; offset += 0x8000) binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  return btoa(binary);
}

function base64ToUint8(value) {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let idx = 0; idx < binary.length; idx += 1) bytes[idx] = binary.charCodeAt(idx);
  return bytes;
}

async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function permissionError(message) { const error = new Error(message); error.code = 'permission_denied'; return error; }
function integrityError(message, code) { const error = new Error(message); error.code = code; return error; }

async function awaitNativeSync(ctx, collectionNames) {
  for (const collectionName of collectionNames) {
    const bridge = await ctx?.sync?.startCollection?.(collectionName);
    const state = bridge?.state;
    if (!state) continue;
    let timeout = null;
    try {
      await Promise.race([
        Promise.resolve().then(() => state.awaitInSync?.() || state.awaitInitialReplication?.()),
        new Promise((_, reject) => {
          timeout = setTimeout(() => {
            const error = new Error(`CTOX product sync timed out: ${collectionName}`);
            error.code = 'sync_timeout';
            reject(error);
          }, 30000);
        }),
      ]);
    } finally {
      if (timeout) clearTimeout(timeout);
    }
  }
}

export const __officeBridgeTestHooks = { loadBlob, saveBlob };
