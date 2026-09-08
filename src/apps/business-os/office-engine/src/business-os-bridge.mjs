// 20260908-office-source-upload-v1
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

const DEMAND_FILE_LOADER_READY_TIMEOUT_MS = 30_000;
const DEMAND_FILE_LOADER_RETRY_MS = 100;

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
    async stageSourceBlob({ recordId, versionId, blobId, mimeType, bytes } = {}) {
      if (!canWrite()) throw permissionError('CTOX product write permission is required');
      for (const [name, value] of Object.entries({ recordId, versionId, blobId, mimeType })) {
        if (typeof value !== 'string' || !value.trim()) {
          throw new TypeError(`CTOX product source ${name} is required`);
        }
      }
      const payloadBytes = normalizeBytes(bytes);
      return withChunkLease(ctx, config, `${config.module}-stage-source`, async (lease) => {
        const documents = await saveBlob(collection(config.chunks), config, {
          recordId, versionId, blobId, mimeType, bytes: payloadBytes, source: true,
        });
        // Creation/import must not publish references before the native peer
        // has the source. loadVersion runs before prepare and can use a remote
        // read-through query even immediately after the local write.
        await flushExactDocuments(ctx, lease, config.chunks, documents);
      });
    },

    async loadVersion({ recordId, versionId } = {}) {
      return withChunkLease(ctx, config, `${config.module}-load-version`, async (lease) => {
        const fileLoader = lazyDemandFileLoader(ctx, lease, config.chunks);
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
      return withChunkLease(ctx, config, `${config.module}-prepare`, async (sourceLease) => {
        // A locally created/imported file is not necessarily on the native
        // peer yet (especially in a follower tab). Publish its dependencies
        // before submitting the native conversion command. This is push-only:
        // opening one file must never await a complete collection download.
        await flushSourceLease(ctx, sourceLease, config.chunks);
        for (const name of [config.versions, config.records]) {
          const lease = await ctx.sync.leaseCollection(name, `${config.module}-prepare-source`);
          try {
            await flushSourceLease(ctx, lease, name);
          } finally {
            await lease?.release?.().catch(() => null);
          }
        }
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
        const documents = await saveBlob(collection(config.chunks), config, {
          blobId: editorBlobId,
          recordId,
          versionId: baseVersionId,
          bytes: payloadBytes,
        });
        const editorSha256 = await sha256Hex(payloadBytes);
        await flushSourceLease(ctx, lease, config.chunks, documents);
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

    async freezeEmailContent({ recordId, versionId } = {}) {
      if (kind !== 'document') throw new TypeError('Only CTOX documents can be compiled into email content');
      if (!canWrite()) throw permissionError('CTOX document write permission is required');
      return withChunkLease(ctx, config, `${config.module}-freeze-email-content`, async (lease) => {
        const result = await dispatch(ctx, config, 'freeze_email_content', recordId, {
          document_id: recordId,
          version_id: versionId,
        });
        const artifact = result?.artifact;
        if (!artifact?.html_blob_id || !artifact?.text_blob_id) {
          throw integrityError('Frozen email content is missing its transport blobs', 'email_artifact_invalid');
        }
        const fileLoader = lazyDemandFileLoader(ctx, lease, config.chunks);
        const [htmlBytes, textBytes] = await Promise.all([
          loadBlob(collection(config.chunks), artifact.html_blob_id, artifact.html_sha256, fileLoader),
          loadBlob(collection(config.chunks), artifact.text_blob_id, artifact.text_sha256, fileLoader),
        ]);
        const assets = await Promise.all((Array.isArray(artifact.assets) ? artifact.assets : []).map(async (asset) => {
          if (!asset?.content_id || !asset?.blob_id || !asset?.sha256 || !asset?.mime_type) {
            throw integrityError('Frozen email content contains an invalid CID asset', 'email_asset_invalid');
          }
          return {
            ...asset,
            bytes: await loadBlob(collection(config.chunks), asset.blob_id, asset.sha256, fileLoader),
          };
        }));
        const decoder = new TextDecoder('utf-8', { fatal: true });
        return {
          ...result,
          artifact,
          html: decoder.decode(htmlBytes),
          text: decoder.decode(textBytes),
          assets,
        };
      });
    },

    async export({ recordId, versionId, format } = {}) {
      return withChunkLease(ctx, config, `${config.module}-export`, async (lease) => {
        const result = await dispatch(ctx, config, 'export', recordId, {
          [`${kind}_id`]: recordId,
          version_id: versionId,
          format,
        });
        const bytes = await loadBlob(
          collection(config.chunks),
          result.blob_id,
          result.source_sha256,
          lazyDemandFileLoader(ctx, lease, config.chunks),
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
  const rows = await queryBlobChunks(chunks, blobId);
  let localError = null;
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

async function saveBlob(chunks, config, { blobId, recordId, versionId, bytes, mimeType = config.mime, source = false }) {
  // Source rows include base64 overhead within the 256KiB wire budget.
  const chunkSize = source ? 192000 : 256000;
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
      mime_type: mimeType,
      encoding: 'base64',
      data: uint8ToBase64(chunk),
      created_at_ms: now,
    });
  }
  // Keep an independent expectation: persistence must not mutate the input
  // and then use that mutation as evidence of a successful write.
  const expected = new Map(rows.map((row) => [row.id, { ...row }]));
  const stored = [];
  if (typeof chunks.bulkUpsert === 'function') {
    const result = await chunks.bulkUpsert(rows);
    if (Array.isArray(result)) stored.push(...result);
  } else {
    for (const row of rows) stored.push(await chunks.incrementalUpsert(row));
  }
  const invalid = () => integrityError('CTOX product staged blob rows are invalid', 'blob_staging_invalid');
  if (!stored.length || stored.length !== expected.size) throw invalid();
  const documents = [];
  for (const doc of stored) {
    if (typeof doc?.toJSON !== 'function') throw invalid();
    const row = doc.toJSON();
    const original = expected.get(row?.id);
    if (!original
      || Object.keys(original).some((key) => row[key] !== original[key])
      || row._deleted === true
      || !row._meta || typeof row._meta !== 'object' || Array.isArray(row._meta)
      || !Number.isFinite(row._meta.lwt) || row._meta.lwt <= 0) {
      throw invalid();
    }
    expected.delete(row.id);
    documents.push(row);
  }
  if (expected.size) throw invalid();
  return documents;
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

async function withChunkLease(ctx, config, reason, operation) {
  if (typeof ctx?.sync?.leaseCollection !== 'function') {
    const error = new Error(`${config.chunks} requires sync.leaseCollection().`);
    error.code = 'demand_only_lease_unavailable';
    throw error;
  }
  const lease = await ctx.sync.leaseCollection(config.chunks, reason);
  try {
    return await operation(lease);
  } finally {
    await lease?.release?.().catch(() => null);
  }
}

function lazyDemandFileLoader(ctx, lease, collectionName) {
  return {
    async fetchFile(fileId, options) {
      const loader = await ensureDemandFileLoader(ctx, lease, collectionName);
      if (typeof loader?.fetchFile !== 'function') {
        const error = new Error(`Demand file loader did not become ready: ${collectionName}`);
        error.code = 'demand_file_loader_unavailable';
        error.retryable = true;
        throw error;
      }
      return loader.fetchFile(fileId, options);
    },
  };
}

async function ensureDemandFileLoader(ctx, lease, collectionName) {
  const deadline = Date.now() + DEMAND_FILE_LOADER_READY_TIMEOUT_MS;
  const restartAfter = Date.now() + 5_000;
  let restarted = false;
  while (Date.now() < deadline) {
    const existing = demandFileLoaderFromLease(lease);
    if (typeof existing?.fetchFile === 'function') return existing;

    const bridge = lease?.bridge || null;
    if (bridge?.mode === 'follower' || bridge?.mode === 'pending' || !bridge?.state) {
      const next = await ctx.sync.startCollection(collectionName, {
        pin: false,
        forceDirect: true,
      });
      if (next) lease.bridge = next;
    } else {
      await bridge.state.enableDemandLoading?.();
    }

    const ready = demandFileLoaderFromLease(lease);
    if (typeof ready?.fetchFile === 'function') return ready;
    if (!restarted && Date.now() >= restartAfter && typeof ctx?.sync?.restartCollection === 'function') {
      const restartedBridge = await ctx.sync.restartCollection(collectionName, { forceDirect: true }).catch(() => null);
      if (restartedBridge) lease.bridge = restartedBridge;
      if (!restartedBridge || restartedBridge?.mode === 'follower' || restartedBridge?.mode === 'pending') {
        const directBridge = await ctx.sync.startCollection(collectionName, {
          pin: false,
          forceDirect: true,
        }).catch(() => null);
        if (directBridge) lease.bridge = directBridge;
      }
      restarted = true;
    }
    await delay(DEMAND_FILE_LOADER_RETRY_MS);
  }
  const error = new Error(`Demand file loader did not become ready: ${collectionName}`);
  error.code = 'demand_file_loader_timeout';
  error.retryable = true;
  throw error;
}

function demandFileLoaderFromLease(lease) {
  const bridge = lease?.bridge || lease || null;
  return bridge?.state?.demandFileLoader || lease?.state?.demandFileLoader || null;
}

async function flushSourceLease(ctx, lease, collectionName, documents) {
  if (documents !== undefined) {
    return flushExactDocuments(ctx, lease, collectionName, documents);
  }
  if (!lease?.bridge?.state || lease.bridge.mode === 'follower') {
    lease.bridge = await ctx.sync.startCollection(collectionName, { pin: false, forceDirect: true });
  }
  await flushBridgeSync(lease, collectionName, { pushOnly: true });
}

async function flushExactDocuments(ctx, lease, collectionName, documents) {
  const unavailable = () => new Error(`CTOX product sync push unavailable: ${collectionName}`);
  if (!Array.isArray(documents) || !documents.length) throw unavailable();
  const deadline = Date.now() + 60000;
  let phase = 'waiting_for_peer';
  let closed = false;
  let timer;
  const timeoutError = () => Object.assign(
    new Error(`CTOX product sync push timed out (${phase}): ${collectionName}`),
    { code: 'sync_timeout', phase },
  );
  const checkDeadline = () => {
    if (closed || Date.now() >= deadline) throw timeoutError();
  };
  try {
    return await Promise.race([
      new Promise((_, reject) => {
        timer = setTimeout(() => {
          closed = true;
          reject(timeoutError());
        }, 60000);
      }),
      Promise.resolve().then(async () => {
        checkDeadline();
        let bridge = lease?.bridge;
        if (!bridge?.state || bridge.mode === 'follower') {
          bridge = await ctx.sync.startCollection(collectionName, { pin: false, forceDirect: true });
          checkDeadline();
          lease.bridge = bridge;
        }
        if (!bridge?.state && bridge?.ready) {
          bridge = await bridge.ready;
          checkDeadline();
          lease.bridge = bridge;
        }
        const state = bridge?.state;
        const checkAvailable = () => {
          if (!state || bridge.mode === 'follower' || state.cancelled
            || typeof state.waitForOpenPeerId !== 'function'
            || typeof state.pushDocumentsToPeer !== 'function') throw unavailable();
        };
        checkAvailable();
        checkDeadline();
        const peerId = await state.waitForOpenPeerId(deadline - Date.now());
        // Promise.race does not cancel readiness. Never start a late write.
        checkDeadline();
        checkAvailable();
        if (!peerId) throw unavailable();
        phase = 'uploading';
        const acknowledged = await state.pushDocumentsToPeer(peerId, documents);
        checkDeadline();
        checkAvailable();
        if (acknowledged === false) throw unavailable();
      }),
    ]);
  } finally {
    closed = true;
    clearTimeout(timer);
  }
}

async function flushBridgeSync(value, collectionName, { pushOnly = false } = {}) {
  let bridge = value?.bridge || value || null;
  if (!bridge?.state && bridge?.ready) {
    bridge = await withSyncTimeout(() => bridge.ready, 60000,
      `CTOX product sync bridge timed out: ${collectionName}`);
  }
  if (bridge?.mode === 'follower' && typeof bridge.flush === 'function') {
    await withSyncTimeout(
      () => bridge.flush(),
      60000,
      `CTOX product sync push timed out: ${collectionName}`,
    );
    return;
  }
  const state = bridge?.state || value?.state || null;
  if (!state) {
    if (pushOnly) throw new Error(`CTOX product sync push unavailable: ${collectionName}`);
    return;
  }
  await withSyncTimeout(
    async () => {
      if (pushOnly && typeof state.waitForOpenPeerId === 'function' && typeof state.pushToPeer === 'function') {
        const peerId = await state.waitForOpenPeerId(60000);
        // Unlike the background all-peer sweep, this rejects transport errors
        // instead of merely scheduling a later retry and resolving early.
        return state.pushToPeer(peerId);
      }
      if (typeof state.pushToRemotePeers === 'function') return state.pushToRemotePeers();
      if (typeof state.scheduleLocalWritePush === 'function') return state.scheduleLocalWritePush();
      if (pushOnly) throw new Error(`CTOX product sync push unavailable: ${collectionName}`);
      return state.awaitInSync?.();
    },
    60000,
    `CTOX product sync push timed out: ${collectionName}`,
  );
}

async function withSyncTimeout(operation, timeoutMs, message) {
  let timeout = null;
  try {
    return await Promise.race([
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

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export const __officeBridgeTestHooks = { loadBlob, saveBlob };
