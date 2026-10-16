// Office-Brücke der App spreadsheets auf App-API 4.0 (treue Kopie von modules/documents/office-bridge-v4.js – getrennte App,
// kein Cross-Import; ersetzt office-engine/src/business-os-bridge.mjs). RPC-Schnittstelle des Office-Editors:
// prepare, commit, freezeEmailContent, export, reportIntegrityError.
//
// v4-Abbildung (V4-PORT.md §Office-Brücke):
// - Datei-Bytes nur über api.files: `files.put(blob, {id: <blob_id>})` legt das Metadaten-Dokument `files/<blob_id>`
//   mit `ref = sha256` an; Lesen über `files/<blob_id>.ref` (Rückfall: sha256 der Version) und `files.get(ref)`.
//   Chunk-Collections, Leases und der Demand-File-Loader entfallen.
// - Office-Operationen sind Befehle `office.<kind>.<op>` (Handler + cap.office.* im Core). Der Editor braucht das
//   Ergebnis (RPC-Vertrag), daher löst die Brücke über den GEPUSHTEN Befehlsstatus auf (`done` → result,
//   `failed` → Fehler): kein Polling, kein Zeitlimit, keine Wiederholung. Die UI bleibt währenddessen bedienbar.
const KIND_CONFIG = Object.freeze({
  document: {
    module: 'documents',
    records: 'documents',
    versions: 'document_versions',
    recordIdField: 'document_id',
    mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  },
  spreadsheet: {
    module: 'spreadsheets',
    records: 'spreadsheets',
    versions: 'spreadsheet_versions',
    recordIdField: 'spreadsheet_id',
    mime: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  },
});

export function createOfficeBridgeV4(api, kind) {
  const config = KIND_CONFIG[kind];
  if (!config) throw new TypeError(`Unsupported CTOX product bridge kind: ${kind}`);
  const preparedVersions = new Map();
  const pending = new Set();
  const opName = config.module === 'documents' ? 'document' : 'spreadsheet';

  function runCommand(operation, recordId, payload) {
    const type = `office.${opName}.${operation}`;
    const handle = api.command(type, {
      ...payload,
      record_id: String(recordId || ''),
      module: config.module,
      client_context: { source: 'ctox-office-esm', surface: `business-os-${config.module}`, transport: 'app-api-4' },
    }, { record_id: String(recordId || '') });
    return new Promise((resolve, reject) => {
      let off = null;
      let settled = false;
      const entry = { cancel: () => finish(() => reject(cancelledError(type))) };
      const finish = (fn) => {
        if (settled) return;
        settled = true;
        pending.delete(entry);
        off?.();
        off = null;
        fn();
      };
      pending.add(entry);
      off = handle.status.subscribe((state) => {
        if (state?.status === 'done') {
          finish(() => {
            const result = state.result;
            const outcome = result?.payload?.outcome || result?.result?.outcome || result?.outcome || result?.result || result;
            if (outcome?.ok === false) {
              const error = new Error(outcome?.error || `CTOX product ${operation} failed`);
              error.code = outcome?.error_code || 'office_command_failed';
              reject(error);
            } else resolve(outcome);
          });
        } else if (state?.status === 'failed') {
          finish(() => {
            const error = new Error(state.error || `CTOX product ${operation} failed`);
            error.code = 'office_command_failed';
            reject(error);
          });
        }
      });
      if (settled) off?.();
      handle.local.catch((error) => finish(() => reject(error)));
    });
  }

  return Object.freeze({
    async stageSourceBlob({ recordId, versionId, blobId, mimeType, bytes, name } = {}) {
      for (const [field, value] of Object.entries({ recordId, versionId, blobId, mimeType })) {
        if (typeof value !== 'string' || !value.trim()) throw new TypeError(`CTOX product source ${field} is required`);
      }
      return putFile(api, blobId, normalizeBytes(bytes), mimeType, name);
    },

    async loadVersion({ recordId, versionId } = {}) {
      const record = api.get(config.records, String(recordId || '')).get();
      if (!record) throw notFoundError(`${kind} record was not found: ${recordId}`);
      const resolvedVersionId = String(versionId || record.current_version_id || '');
      const version = resolvedVersionId ? api.get(config.versions, resolvedVersionId).get() : null;
      if (!version) throw notFoundError(`${kind} version was not found: ${resolvedVersionId}`);
      const effectiveVersion = mergePreparedVersion(version, preparedVersions.get(resolvedVersionId));
      const canonicalBytes = await readFileBytes(api, effectiveVersion.blob_id, effectiveVersion.source_sha256);
      const editorBytes = effectiveVersion.editor_blob_id
        ? await readFileBytes(api, effectiveVersion.editor_blob_id, effectiveVersion.editor_sha256)
        : null;
      return { record, version: effectiveVersion, canonicalBytes, editorBytes };
    },

    async prepare({ recordId, versionId } = {}) {
      const outcome = await runCommand('prepare', recordId, { [`${kind}_id`]: recordId, version_id: versionId });
      const resolvedVersionId = String(outcome?.version_id || versionId || '');
      if (resolvedVersionId && outcome?.editor_blob_id) preparedVersions.set(resolvedVersionId, outcome);
      return outcome;
    },

    async commit({ recordId, baseVersionId, editorProtocol, editorProtocolVersion, implementedFeatures, reason, bytes } = {}) {
      const payloadBytes = normalizeBytes(bytes);
      const editorBlobId = `office_${kind}_${crypto.randomUUID()}`;
      const { ref: editorSha256 } = await putFile(api, editorBlobId, payloadBytes, config.mime);
      return runCommand('commit', recordId, {
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

    async freezeEmailContent({ recordId, versionId } = {}) {
      if (kind !== 'document') throw new TypeError('Only CTOX documents can be compiled into email content');
      const result = await runCommand('freeze_email_content', recordId, { document_id: recordId, version_id: versionId });
      const artifact = result?.artifact;
      if (!artifact?.html_blob_id || !artifact?.text_blob_id) {
        throw integrityError('Frozen email content is missing its transport blobs', 'email_artifact_invalid');
      }
      const [htmlBytes, textBytes] = await Promise.all([
        readFileBytes(api, artifact.html_blob_id, artifact.html_sha256),
        readFileBytes(api, artifact.text_blob_id, artifact.text_sha256),
      ]);
      const assets = await Promise.all((Array.isArray(artifact.assets) ? artifact.assets : []).map(async (asset) => {
        if (!asset?.content_id || !asset?.blob_id || !asset?.sha256 || !asset?.mime_type) {
          throw integrityError('Frozen email content contains an invalid CID asset', 'email_asset_invalid');
        }
        return { ...asset, bytes: await readFileBytes(api, asset.blob_id, asset.sha256) };
      }));
      const decoder = new TextDecoder('utf-8', { fatal: true });
      return { ...result, artifact, html: decoder.decode(htmlBytes), text: decoder.decode(textBytes), assets };
    },

    async export({ recordId, versionId, format } = {}) {
      const result = await runCommand('export', recordId, { [`${kind}_id`]: recordId, version_id: versionId, format });
      const bytes = await readFileBytes(api, result?.blob_id, result?.source_sha256);
      return { ...result, bytes };
    },

    reportIntegrityError(details = {}) {
      api.status.report({
        level: 'error',
        code: `${config.module}.office.integrity`,
        text: `${details.message || 'Office-Integritätsfehler'}${details.code ? ` (${details.code})` : ''}`,
      });
      return { reported: true };
    },

    // Offene Befehlsabos beim unmount lösen (nichts läuft über die Montage hinaus weiter).
    dispose() {
      for (const entry of [...pending]) entry.cancel();
    },
  });
}

// Datei schreiben: lokal sofort (files.put → local), Upload über die Outbox. Liefert {blob_id, ref}.
export async function putFile(api, blobId, bytes, mimeType, name) {
  const handle = api.files.put(new Blob([bytes], { type: mimeType || 'application/octet-stream' }), {
    id: blobId,
    name: String(name || blobId),
    mime: mimeType || 'application/octet-stream',
  });
  await handle.local;
  return { blob_id: blobId, ref: await handle.ref };
}

// Datei lesen: blob_id = id des files-Dokuments; dessen ref ist der sha256-Inhaltsschlüssel. Ohne files-Dokument
// (Altbestand) dient der sha256 aus den Versionsmetadaten als ref. Fehlt beides → blob_missing.
export async function readFileBytes(api, blobId, expectedSha256 = '') {
  if (!blobId) return null;
  const meta = api.get('files', String(blobId)).get();
  const expected = String(expectedSha256 || '').trim().toLowerCase();
  const ref = String(meta?.ref || expected || '').trim();
  if (!ref) throw integrityError(`Blob has no file reference: ${blobId}`, 'blob_missing');
  const blob = await api.files.get(ref);
  const bytes = new Uint8Array(await blob.arrayBuffer());
  if (expected && await sha256Hex(bytes) !== expected) {
    throw integrityError(`Blob hash does not match its version metadata: ${blobId}`, 'blob_hash_mismatch');
  }
  return bytes;
}

function mergePreparedVersion(version, prepared) {
  if (!prepared?.editor_blob_id) return version;
  return {
    ...version,
    // CSV/TSV preparation replaces the source with a canonical XLSX blob.
    blob_id: prepared.blob_id || version.blob_id,
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

function normalizeBytes(value) {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  throw new TypeError('CTOX product commit requires bytes');
}

export async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function integrityError(message, code) { const error = new Error(message); error.code = code; return error; }
function notFoundError(message) { const error = new Error(message); error.code = 'not_found'; return error; }
function cancelledError(type) { const error = new Error(`${type} cancelled (unmount)`); error.code = 'cancelled'; return error; }

export const __officeBridgeV4TestHooks = { mergePreparedVersion, readFileBytes, putFile };
