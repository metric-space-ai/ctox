// Origin: CTOX
// License: Apache-2.0
// office.spreadsheet.commit — ref: store_office_commands.rs:235-323 (dispatch) +
// commit_office_spreadsheet_version :1002-1220 (GEMISCHT).
//
// FACHLOGIK (here): base-version conflict gate, deterministic new version id,
// idempotent replay, carry-over of ingestion/knowledge lineage, and the new
// spreadsheet_versions + spreadsheets records. FAEHIGKEIT (cap.office.edit.apply,
// Kernform §16.32, in _lib/office.js applyStagedEditor): apply + export the
// canonical workbook; the new blob appears as a files record.
import { kindBindingFromType, loadOfficeTarget, fail, shortHash, clampText, applyStagedEditor } from './_lib/office.js';

// ref: commit_office_spreadsheet_version :1083-1096 — lineage carried forward.
const LINEAGE_KEYS = [
  'ingestion_kind', 'linked_records', 'source_receipt_snapshot_hashes',
  'knowledge_version', 'knowledge_lineage',
];

export default async function handle(command, ctx) {
  const binding = kindBindingFromType(command.type);
  const { recordId, record, versionId, version } = await loadOfficeTarget(command, ctx, binding);
  const payload = command.payload ?? {};
  const spreadsheetId = recordId;

  const baseVersionId = typeof payload.base_version_id === 'string' && payload.base_version_id
    ? payload.base_version_id : undefined;
  if (!baseVersionId) fail('invalid', 'office commit requires base_version_id');
  if (baseVersionId !== versionId) {
    fail('conflict', 'version_conflict: requested base version differs from loaded version');
  }
  const currentVersionId = typeof record.current_version_id === 'string' ? record.current_version_id : '';
  if (currentVersionId !== baseVersionId) {
    fail('conflict', `version_conflict: current version is ${currentVersionId}, expected ${baseVersionId}`);
  }
  const stagedEditorBlobId = typeof payload.editor_blob_id === 'string' && payload.editor_blob_id
    ? payload.editor_blob_id : undefined;
  if (!stagedEditorBlobId) fail('invalid', 'office commit requires editor_blob_id');
  const commandId = command.id;
  if (!commandId) fail('invalid', 'office commit command id is required');

  const baseCanonicalBlobId = typeof version.blob_id === 'string' && version.blob_id ? version.blob_id : undefined;
  if (!baseCanonicalBlobId) fail('invalid', 'base office version has no blob');

  const versionNumber = (Number.isInteger(version.version) ? version.version : 0) + 1;
  const newVersionId = `${spreadsheetId}_office_v${versionNumber}_${shortHash(commandId)}`;
  // Vor §16.32 vergab der Handler die Blob-id; jetzt erzeugt office.edit.apply den Blob (files-Datensatz mit ref).
  const newBlobId = `${newVersionId}_blob`;

  const replay = await ctx.get(binding.versionsCollection, newVersionId);
  if (replay) {
    return {
      ok: true,
      idempotent_replay: true,
      operation: 'commit',
      spreadsheet_id: spreadsheetId,
      base_version_id: baseVersionId,
      version_id: newVersionId,
      blob_id: typeof replay.blob_id === 'string' ? replay.blob_id : newBlobId,
      editor_blob_id: typeof replay.editor_blob_id === 'string' ? replay.editor_blob_id : newBlobId,
      source_sha256: replay.source_sha256 ?? null,
      editor_sha256: replay.editor_sha256 ?? null,
      manifest: replay.office_manifest ?? null,
      diagnostics: replay.diagnostics ?? null,
    };
  }

  const implementedFeatures = Array.isArray(payload.implemented_features)
    ? payload.implemented_features.filter((value) => typeof value === 'string') : [];
  const result = await applyStagedEditor(ctx, binding, version, stagedEditorBlobId,
    typeof payload.editor_sha256 === 'string' ? payload.editor_sha256 : null, implementedFeatures);
  const { applied } = result;

  const now = ctx.now();
  const manifest = applied.manifest ?? {};
  const indexText = clampText(manifest.primary_text);
  const diagnostics = applied.diagnostics ?? [];

  const spreadsheetPayload = { ...record, id: spreadsheetId };
  spreadsheetPayload.current_version_id = newVersionId;
  spreadsheetPayload.status = 'Draft';
  spreadsheetPayload.source_sha256 = result.sha256;
  spreadsheetPayload.index_text = indexText;
  spreadsheetPayload.updated_at_ms = now;

  const versionPayload = {
    id: newVersionId,
    version_id: newVersionId,
    spreadsheet_id: spreadsheetId,
    version: versionNumber,
    source_kind: 'office_edited_xlsx',
    blob_id: result.blobId,
    base_version_id: baseVersionId,
    editor_blob_id: result.editorBlobId,
    staged_editor_blob_id: stagedEditorBlobId,
    editor_protocol: applied.protocol,
    editor_protocol_version: applied.protocol_version,
    source_sha256: result.sha256,
    editor_sha256: result.editorSha256,
    conversion_state: 'prepared',
    implemented_features: implementedFeatures,
    office_manifest: manifest,
    diagnostics,
    model_json: {
      type: 'xlsx',
      text: indexText,
      parts: Array.isArray(manifest.parts) ? manifest.parts.length : 0,
    },
    business_command_id: commandId,
    created_at_ms: now,
    updated_at_ms: now,
  };
  // Carry ingestion/knowledge lineage from the base version (or the record).
  for (const key of LINEAGE_KEYS) {
    const value = version[key] !== undefined ? version[key] : record[key];
    if (value !== undefined) versionPayload[key] = value;
  }

  await ctx.transaction(async (tx) => {
    await tx.put(binding.versionsCollection, versionPayload);
    await tx.put(binding.recordsCollection, spreadsheetPayload);
  });

  return {
    ok: true,
    operation: 'commit',
    spreadsheet_id: spreadsheetId,
    base_version_id: baseVersionId,
    version_id: newVersionId,
    blob_id: result.blobId,
    editor_blob_id: result.editorBlobId,
    staged_editor_blob_id: stagedEditorBlobId,
    source_sha256: result.sha256,
    editor_sha256: result.editorSha256,
    bytes: result.bytes,
    manifest,
    diagnostics,
  };
}
