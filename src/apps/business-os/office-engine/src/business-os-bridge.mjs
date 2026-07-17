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
  const preparedVersions = new Map();
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
      await awaitNativeSync(ctx, [config.records, config.versions]);
      return withChunkLease(ctx, config, `${config.module}-load-version`, async (lease) => {
        const fileLoader = demandFileLoaderFromLease(lease);
        const recordDoc = await collection(config.records).findOne(String(recordId || '')).exec();
        const record = toJson(recordDoc);
        if (!record) throw new Error(`${kind} record was not found: ${recordId}`);
        const resolvedVersionId = String(versionId || record.current_version_id || '');
        const versionDoc = resolvedVersionId ? await collection(config.versions).findOne(resolvedVersionId).exec() : null;
        const version = toJson(versionDoc);
        if (!version) throw new Error(`${kind} version was not found: ${resolvedVersionId}`);
        const effectiveVersion = mergePreparedVersion(version, preparedVersions.get(resolvedVersionId));
        const canonicalBytes = await loadBlob(collection(config.chunks), effectiveVersion.blob_id, effectiveVersion.source_sha256, fileLoader);
        const editorBytes = effectiveVersion.editor_blob_id
          ? await loadBlob(collection(config.chunks), effectiveVersion.editor_blob_id, effectiveVersion.editor_sha256, fileLoader)
          : null;
        return { record, version: effectiveVersion, canonicalBytes, editorBytes };
      });
    },

    async prepare({ recordId, versionId } = {}) {
      await awaitNativeSync(ctx, [config.records, config.versions]);
      return withChunkLease(ctx, config, `${config.module}-prepare`, async () => {
        const outcome = await dispatch(ctx, config, 'prepare', recordId, {
          [`${kind}_id`]: recordId,
          version_id: versionId,
        });
        const resolvedVersionId = String(outcome?.version_id || versionId || '');
        if (resolvedVersionId && outcome?.editor_blob_id) preparedVersions.set(resolvedVersionId, outcome);
        return outcome;
      });
    },

    async commit({ recordId, baseVersionId, editorProtocol, editorProtocolVersion, implementedFeatures, reason, bytes } = {}) {
      if (!canWrite()) throw permissionError('CTOX product write permission is required');
      const payloadBytes = normalizeBytes(bytes);
      const editorBlobId = `office_${kind}_${crypto.randomUUID()}`;
      return withChunkLease(ctx, config, `${config.module}-commit`, async (lease) => {
        await saveBlob(collection(config.chunks), config, {
          blobId: editorBlobId,
          recordId,
          versionId: baseVersionId,
          bytes: payloadBytes,
        });
        const editorSha256 = await sha256Hex(payloadBytes);
        await flushBridgeSync(lease, config.chunks);
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
      });
    },

    async export({ recordId, versionId, format } = {}) {
      await awaitNativeSync(ctx, [config.records, config.versions]);
      return withChunkLease(ctx, config, `${config.module}-export`, async (lease) => {
        const result = await dispatch(ctx, config, 'export', recordId, {
          [`${kind}_id`]: recordId,
          version_id: versionId,
          format,
        });
        await awaitBridgeSync(lease, config.chunks);
        const bytes = await loadBlob(
          collection(config.chunks),
          result.blob_id,
          result.source_sha256,
          demandFileLoaderFromLease(lease),
        );
        return { ...result, bytes };
      });
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
  const commandId = `cmd_office_${crypto.randomUUID()}`;
  const command = {
    id: commandId,
    module: config.module,
    type: `office.${config.module === 'documents' ? 'document' : 'spreadsheet'}.${operation}`,
    record_id: String(recordId || ''),
    payload,
    client_context: {
      source: 'ctox-office-esm',
      surface: `business-os-${config.module}`,
      transport: 'rxdb-webrtc',
    },
  };
  let result;
  try {
    result = await ctx.commandBus.dispatch(command, { until: 'terminal' });
  } catch (error) {
    result = await recoverTrackedCommand(ctx.commandBus, command, error);
  }
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

async function recoverTrackedCommand(commandBus, command, error) {
  if (error?.retryable !== true) throw error;
  const commandId = String(command?.id || '');
  if (!commandId) throw error;
  let status = null;
  if (typeof commandBus.getStatus === 'function') {
    status = await commandBus.getStatus(commandId).catch(() => null);
  }
  if (status && typeof commandBus.resumeTracking === 'function') {
    return commandBus.resumeTracking(commandId, { until: 'terminal', timeoutMs: 120000 });
  }
  return commandBus.dispatch(command, { until: 'terminal', timeoutMs: 120000 });
}

function mergePreparedVersion(version, prepared) {
  if (!prepared?.editor_blob_id) return version;
  return {
    ...version,
    editor_blob_id: prepared.editor_blob_id,
    editor_protocol: prepared.editor_protocol || version.editor_protocol,
    editor_protocol_version: prepared.editor_protocol_version || version.editor_protocol_version,
    editor_sha256: prepared.editor_sha256 || version.editor_sha256,
    source_sha256: prepared.source_sha256 || version.source_sha256,
    office_manifest: prepared.manifest || version.office_manifest,
    editor_manifest: prepared.editor_manifest || version.editor_manifest,
    conversion_state: 'prepared',
  };
}

async function loadBlob(chunks, blobId, expectedSha256 = '', fileLoader = null) {
  if (!blobId) return null;
  let rows = await queryBlobChunks(chunks, blobId);
  let localError = null;
  try {
    return await assembleBlob(rows, blobId, expectedSha256);
  } catch (error) {
    if (!isRecoverableBlobReadError(error)) throw error;
    localError = error;
  }
  rows = await queryBlobChunks(chunks, blobId, {
    requireRevision: `blob:${blobId}:${String(expectedSha256 || 'current').toLowerCase()}`,
  });
  try {
    return await assembleBlob(rows, blobId, expectedSha256);
  } catch (error) {
    if (!isRecoverableBlobReadError(error)) throw error;
    localError = error;
  }
  if (typeof fileLoader?.fetchFile === 'function') {
    const streamedChunks = await fileLoader.fetchFile(blobId);
    return assembleStreamedBlob(streamedChunks, blobId, expectedSha256);
  }
  throw localError;
}

async function queryBlobChunks(chunks, blobId, options = {}) {
  const docs = await chunks.find({
    selector: { blob_id: blobId },
    sort: [{ idx: 'asc' }],
    ...options,
  }).exec();
  return docs.map(toJson).filter(Boolean).sort((a, b) => Number(a.idx) - Number(b.idx));
}

async function assembleBlob(rows, blobId, expectedSha256 = '') {
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

function isRecoverableBlobReadError(error) {
  return ['blob_missing', 'blob_incomplete', 'blob_invalid', 'blob_hash_mismatch'].includes(error?.code);
}

async function assembleStreamedBlob(chunks, blobId, expectedSha256 = '') {
  const rows = (Array.isArray(chunks) ? chunks : [])
    .filter((chunk) => chunk && Number.isFinite(Number(chunk.sequence)))
    .sort((left, right) => Number(left.sequence) - Number(right.sequence));
  if (!rows.length) throw integrityError(`Blob has no streamed chunks: ${blobId}`, 'blob_missing');
  const decoded = [];
  for (const row of rows) {
    const bytes = base64ToUint8(row.bytesBase64 || '');
    const expectedChunkHash = String(row.hash || '').trim().toLowerCase();
    if (expectedChunkHash && await sha256Hex(bytes) !== expectedChunkHash) {
      throw integrityError(`Streamed blob chunk failed transport verification: ${blobId}#${row.sequence}`, 'blob_chunk_hash_mismatch');
    }
    decoded.push(bytes);
  }
  const length = decoded.reduce((sum, bytes) => sum + bytes.length, 0);
  const result = new Uint8Array(length);
  let offset = 0;
  for (const bytes of decoded) { result.set(bytes, offset); offset += bytes.length; }
  const expected = String(expectedSha256 || '').trim().toLowerCase();
  const actual = await sha256Hex(result);
  if (expected && actual !== expected) {
    throw integrityError(`Streamed blob hash does not match its version metadata: ${blobId} (expected ${expected}, received ${actual}, bytes ${result.length})`, 'blob_hash_mismatch');
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
    await awaitBridgeSync(bridge, collectionName);
  }
}

async function withChunkLease(ctx, config, reason, operation) {
  if (typeof ctx?.sync?.leaseCollection !== 'function') {
    const error = new Error(`${config.chunks} requires sync.leaseCollection().`);
    error.code = 'demand_only_lease_unavailable';
    throw error;
  }
  const lease = await ctx.sync.leaseCollection(config.chunks, reason);
  try {
    if (!demandFileLoaderFromLease(lease) && lease?.bridge?.mode === 'follower') {
      lease.bridge = await ctx.sync.startCollection(config.chunks, { pin: false, forceDirect: true });
    }
    await awaitBridgeSync(lease, config.chunks);
    return await operation(lease);
  } finally {
    await lease?.release?.().catch(() => null);
  }
}

function demandFileLoaderFromLease(lease) {
  const bridge = lease?.bridge || lease || null;
  return bridge?.state?.demandFileLoader || lease?.state?.demandFileLoader || null;
}

async function awaitBridgeSync(value, collectionName) {
  const bridge = value?.bridge || value || null;
  const state = bridge?.state || value?.state || null;
  if (!state) return;
  await withSyncTimeout(
    () => state.awaitInSync?.() || state.awaitInitialReplication?.(),
    30000,
    `CTOX product sync timed out: ${collectionName}`,
  );
}

async function flushBridgeSync(value, collectionName) {
  const bridge = value?.bridge || value || null;
  if (bridge?.mode === 'follower' && typeof bridge.flush === 'function') {
    await withSyncTimeout(
      () => bridge.flush(),
      60000,
      `CTOX product sync push timed out: ${collectionName}`,
    );
    return;
  }
  const state = bridge?.state || value?.state || null;
  if (!state) return;
  await withSyncTimeout(
    () => {
      if (typeof state.pushToRemotePeers === 'function') return state.pushToRemotePeers();
      if (typeof state.scheduleLocalWritePush === 'function') return state.scheduleLocalWritePush();
      return state.awaitInSync?.();
    },
    60000,
    `CTOX product sync push timed out: ${collectionName}`,
  );
}

async function withSyncTimeout(operation, timeoutMs, message) {
  let timeout = null;
  try {
    await Promise.race([
      Promise.resolve().then(operation),
      new Promise((_, reject) => {
        timeout = setTimeout(() => {
          const error = new Error(message);
          error.code = 'sync_timeout';
          reject(error);
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timeout) clearTimeout(timeout);
  }
}

export const __officeBridgeTestHooks = { loadBlob, saveBlob };
