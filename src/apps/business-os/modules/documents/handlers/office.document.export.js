// Origin: CTOX
// License: Apache-2.0
// office.document.export — ref: store_office_commands.rs:431-452 (GEMISCHT).
//
// FACHLOGIK (here): resolve the record+version, the requested format (pdf
// liefert der Kern nicht → unavailable vor jedem Aufruf), and the files id of
// the rendered package. FAEHIGKEIT (Kernform §16.32): office.edit.prepare on
// the canonical blob, then office.render({session, to}) on that session; the
// package appears as a files record (ref = sha256).
import {
  kindBindingFromType, loadOfficeTarget, fail, canonicalRef, prepareSession, renderTarget,
  refOf, blobIdForRef, bytesForRef, DOCUMENT_FEATURES, SPREADSHEET_FEATURES,
} from './_lib/office.js';

const EXTENSION = { docx: 'docx', xlsx: 'xlsx' };

export default async function handle(command, ctx) {
  const binding = kindBindingFromType(command.type);
  const payload = command.payload ?? {};
  const to = renderTarget(binding, payload.format ?? payload.to);
  const { recordId, versionId, version } = await loadOfficeTarget(command, ctx, binding);
  const sourceBlobId = typeof version.blob_id === 'string' && version.blob_id ? version.blob_id : undefined;
  if (!sourceBlobId) fail('invalid', 'office version has no export blob');
  const sourceRef = await canonicalRef(ctx, version, 'office version has no export blob');

  const prepared = await prepareSession(ctx, binding, sourceRef,
    binding.kind === 'document' ? DOCUMENT_FEATURES : SPREADSHEET_FEATURES);
  const pkg = await ctx.cap.office.render({ session: prepared.session, to });
  const ref = refOf(pkg?.blob_id);
  if (!ref) fail('unavailable', 'office.render returned no package');

  return {
    ok: true,
    operation: 'export',
    record_id: recordId,
    version_id: versionId,
    source_blob_id: sourceBlobId,
    blob_id: await blobIdForRef(ctx, ref),
    mime_type: pkg.mime_type,
    extension: EXTENSION[to],
    sha256: typeof pkg.sha256 === 'string' && pkg.sha256 ? pkg.sha256 : ref,
    source_sha256: typeof pkg.sha256 === 'string' && pkg.sha256 ? pkg.sha256 : ref,
    bytes: await bytesForRef(ctx, ref),
    manifest: prepared.manifest,
    diagnostics: Array.isArray(pkg.diagnostics) ? pkg.diagnostics : [],
  };
}
