// Origin: CTOX
// License: Apache-2.0
// office.document.prepare — ref: store_office_commands.rs:69-234 (GEMISCHT).
//
// FACHLOGIK (here): resolve record+version, map the version blob id to its
// files.ref, stamp the prepared editor metadata onto the version document.
// FAEHIGKEIT (cap.office.edit.prepare, Kernform §16.32): read the source blob,
// delimited→xlsx canonicalisation, the office engine prepare() pass; every
// produced blob appears as a files record (ref = sha256).
import {
  DOCUMENT_FEATURES, SPREADSHEET_FEATURES, kindBindingFromType, loadOfficeTarget,
  canonicalRef, prepareSession, refOf, blobIdForRef,
} from './_lib/office.js';

export default async function handle(command, ctx) {
  const binding = kindBindingFromType(command.type);
  const { recordId, versionId, version } = await loadOfficeTarget(command, ctx, binding);
  const sourceBlobId = typeof version.blob_id === 'string' && version.blob_id ? version.blob_id : undefined;
  const sourceRef = await canonicalRef(ctx, version);
  const implementedFeatures = binding.kind === 'document' ? DOCUMENT_FEATURES : SPREADSHEET_FEATURES;

  const prepared = await prepareSession(ctx, binding, sourceRef, implementedFeatures);
  const canonical = refOf(prepared.session.source);
  const editorRef = refOf(prepared.session.editor);
  const converted = canonical !== sourceRef;
  const canonicalBlobId = converted ? await blobIdForRef(ctx, canonical) : sourceBlobId;
  const editorBlobId = await blobIdForRef(ctx, editorRef);

  const now = ctx.now();
  const nextVersion = { ...version };
  nextVersion.editor_blob_id = editorBlobId;
  if (converted) {
    if (nextVersion.original_blob_id === undefined) nextVersion.original_blob_id = sourceBlobId;
    nextVersion.blob_id = canonicalBlobId;
    nextVersion.canonical_source_kind = 'delimited_text_to_xlsx';
    nextVersion.canonical_mime_type = binding.canonicalMime;
  }
  nextVersion.editor_protocol = prepared.protocol;
  nextVersion.editor_protocol_version = prepared.protocol_version;
  nextVersion.source_sha256 = canonical;
  nextVersion.editor_sha256 = typeof prepared.sha256 === 'string' && prepared.sha256 ? prepared.sha256 : editorRef;
  nextVersion.conversion_state = 'prepared';
  nextVersion.implemented_features = implementedFeatures;
  nextVersion.office_manifest = prepared.manifest;
  nextVersion.editor_manifest = prepared.model;
  nextVersion.updated_at_ms = now;
  nextVersion.id = versionId;
  await ctx.put(binding.versionsCollection, nextVersion);

  return {
    ok: true,
    operation: 'prepare',
    record_id: recordId,
    version_id: versionId,
    blob_id: canonicalBlobId,
    editor_blob_id: editorBlobId,
    editor_protocol: nextVersion.editor_protocol,
    editor_protocol_version: nextVersion.editor_protocol_version,
    source_sha256: nextVersion.source_sha256,
    editor_sha256: nextVersion.editor_sha256,
    manifest: prepared.manifest,
    editor_manifest: prepared.model,
    diagnostics: Array.isArray(prepared.diagnostics) ? prepared.diagnostics : [],
  };
}
