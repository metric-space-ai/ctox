// Origin: CTOX
// License: Apache-2.0
// office.document.freeze_email_content — ref: store_office_commands.rs:324-430 (GEMISCHT).
//
// FACHLOGIK (here): resolve the document version, guard the expected source
// hash, map produced refs to files ids and stamp the frozen
// `email_render_artifact` onto it. FAEHIGKEIT (Kernform §16.32):
// office.edit.prepare on the canonical blob, then office.render({session,
// to:'email_html'}); html/text/asset blobs appear as files records.
import {
  kindBindingFromType, loadOfficeTarget, fail, canonicalRef, prepareSession, refOf, blobIdForRef, bytesForRef,
  DOCUMENT_FEATURES,
} from './_lib/office.js';

// ref: office_engine.rs compile_document_to_email — Schema des eingefrorenen Artefakts.
const EMAIL_ARTIFACT_SCHEMA_VERSION = 1;

export default async function handle(command, ctx) {
  const binding = kindBindingFromType(command.type);
  if (binding.kind !== 'document') fail('invalid', 'email content can only be compiled from a document');
  const { recordId, versionId, version } = await loadOfficeTarget(command, ctx, binding);
  const sourceBlobId = typeof version.blob_id === 'string' && version.blob_id ? version.blob_id : undefined;
  if (!sourceBlobId) fail('invalid', 'office version has no canonical blob');
  const sourceRef = await canonicalRef(ctx, version);
  const expectedSource = typeof version.source_sha256 === 'string' && version.source_sha256 ? version.source_sha256 : null;

  const prepared = await prepareSession(ctx, binding, sourceRef, DOCUMENT_FEATURES);
  if (expectedSource && refOf(prepared.session.source) !== expectedSource) {
    fail('conflict', 'document source changed since the version was prepared');
  }
  const compiled = await ctx.cap.office.render({ session: prepared.session, to: 'email_html' });
  const htmlRef = refOf(compiled?.blob_id);
  const textRef = refOf(compiled?.text_blob);
  if (!htmlRef || !textRef) fail('unavailable', 'office.render returned no email transport blobs');
  const assets = [];
  for (const asset of Array.isArray(compiled.assets) ? compiled.assets : []) {
    const ref = refOf(asset?.blob_id);
    if (!ref) fail('unavailable', 'office.render returned an email asset without blob');
    assets.push({
      content_id: asset.content_id,
      mime_type: asset.mime_type,
      blob_id: await blobIdForRef(ctx, ref),
      sha256: ref,
      bytes: await bytesForRef(ctx, ref),
    });
  }

  const now = ctx.now();
  const artifact = {
    schema_version: EMAIL_ARTIFACT_SCHEMA_VERSION,
    state: 'frozen',
    compiler_id: compiled.compiler_id,
    document_id: recordId,
    document_version_id: versionId,
    source_blob_id: sourceBlobId,
    source_sha256: refOf(prepared.session.source),
    html_blob_id: await blobIdForRef(ctx, htmlRef),
    html_sha256: htmlRef,
    html_bytes: await bytesForRef(ctx, htmlRef),
    text_blob_id: await blobIdForRef(ctx, textRef),
    text_sha256: textRef,
    text_bytes: await bytesForRef(ctx, textRef),
    assets,
    diagnostics: Array.isArray(compiled.diagnostics) ? compiled.diagnostics : [],
    frozen_at_ms: now,
  };

  const nextVersion = { ...version, id: versionId };
  nextVersion.email_render_artifact = artifact;
  nextVersion.updated_at_ms = now;
  await ctx.put(binding.versionsCollection, nextVersion);

  return { ok: true, operation: 'freeze_email_content', record_id: recordId, version_id: versionId, artifact };
}
