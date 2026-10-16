// Origin: CTOX
// License: Apache-2.0
// office.document.commit — ref: store_office_commands.rs:235-323 (dispatch) +
// commit_office_document_version :718-999 (GEMISCHT).
//
// FACHLOGIK (here): base-version conflict gate, deterministic new version id,
// idempotent replay, and the new version + updated document records.
// FAEHIGKEIT (cap.office.edit.apply, Kernform §16.32, in _lib/office.js
// applyStagedEditor): apply the staged editor payload onto the base session,
// export the canonical package; the new blob appears as a files record.
import { kindBindingFromType, loadOfficeTarget, fail, shortHash, clampText, applyStagedEditor } from './_lib/office.js';

export default async function handle(command, ctx) {
  const binding = kindBindingFromType(command.type);
  const { recordId, record, versionId, version } = await loadOfficeTarget(command, ctx, binding);
  const payload = command.payload ?? {};
  const documentId = recordId;

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
  const newVersionId = `${documentId}_office_v${versionNumber}_${shortHash(commandId)}`;
  // Vor §16.32 vergab der Handler die Blob-id; jetzt erzeugt office.edit.apply den Blob (files-Datensatz mit ref).
  const newBlobId = `${newVersionId}_blob`;

  // Idempotent replay: a previously committed version wins unchanged.
  const replay = await ctx.get(binding.versionsCollection, newVersionId);
  if (replay) {
    return {
      ok: true,
      idempotent_replay: true,
      operation: 'commit',
      document_id: documentId,
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

  const documentPayload = { ...record, id: documentId };
  documentPayload.current_version_id = newVersionId;
  documentPayload.status = 'Draft';
  documentPayload.source_sha256 = result.sha256;
  documentPayload.index_text = indexText;
  documentPayload.updated_at_ms = now;

  const versionPayload = {
    id: newVersionId,
    version_id: newVersionId,
    document_id: documentId,
    version: versionNumber,
    source_kind: 'office_edited_docx',
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
      type: 'docx',
      text: indexText,
      parts: Array.isArray(manifest.parts) ? manifest.parts.length : 0,
    },
    business_command_id: commandId,
    created_at_ms: now,
    updated_at_ms: now,
  };

  await ctx.transaction(async (tx) => {
    await tx.put(binding.versionsCollection, versionPayload);
    await tx.put(binding.recordsCollection, documentPayload);
  });

  return {
    ok: true,
    operation: 'commit',
    document_id: documentId,
    base_version_id: baseVersionId,
    version_id: newVersionId,
    blob_id: result.blobId,
    editor_blob_id: result.editorBlobId,
    source_sha256: result.sha256,
    editor_sha256: result.editorSha256,
    bytes: result.bytes,
    manifest,
    diagnostics,
  };
}
