// Origin: CTOX
// License: Apache-2.0
//
// Shared office business logic for the documents app v4 handlers.
// ref: src/core/business_os/store_office_commands.rs (read-only port).
//
// Split (Architektur §16.13): the FACHLOGIK lives here and in the handlers —
// record resolution, version-id generation, version-conflict + idempotency
// checks, and the version/record documents that get written. The native
// FAEHIGKEIT (open/render/edit the office package, read/write blob chunks,
// integrity verification) stays in the core capability `ctx.cap.office.*`,
// whose contract is specified in documents/V4-HANDLERS.md.

import { createHash } from 'node:crypto';

/** Handler-contract error: `throw { code, message }` (§16.16). */
export function fail(code, message) {
  throw { code, message };
}

/** First non-empty trimmed string among `keys` of `payload`. */
export function firstString(payload, keys) {
  if (!payload || typeof payload !== 'object') return undefined;
  for (const key of keys) {
    const value = payload[key];
    if (typeof value === 'string' && value.trim() !== '') return value;
  }
  return undefined;
}

// ref: hashing.rs:24 — first 10 chars of URL-safe-no-pad base64 of SHA-256.
export function shortHash(value) {
  return createHash('sha256').update(String(value)).digest('base64url').slice(0, 10);
}

// ref: store_office_commands.rs:118-131 — implemented feature set per kind.
export const DOCUMENT_FEATURES = [
  'document.open-render-zoom',
  'document.edit-save',
  'document.undo-clipboard-keyboard',
  'document.character-paragraph-formatting',
  'document.styles-lists-numbering',
  'document.tables',
  'document.images-positioning',
  'document.sections-headers-footers',
  'document.links-bookmarks-fields',
  'document.comments-track-changes',
  'document.drawings-charts',
];
export const SPREADSHEET_FEATURES = ['spreadsheet.open-render-sheets'];

// ref: store_office_commands.rs:22-40 — kind → collection names.
export function kindBindingFromType(commandType) {
  if (commandType.startsWith('office.document.')) {
    return {
      kind: 'document',
      moduleId: 'documents',
      recordsCollection: 'documents',
      versionsCollection: 'document_versions',
      chunksCollection: 'document_blob_chunks',
      idKeys: ['document_id', 'record_id'],
      idField: 'document_id',
      canonicalMime: 'application/vnd.ctox.euro-office-document',
    };
  }
  return {
    kind: 'spreadsheet',
    moduleId: 'spreadsheets',
    recordsCollection: 'spreadsheets',
    versionsCollection: 'spreadsheet_versions',
    chunksCollection: 'spreadsheet_blob_chunks',
    idKeys: ['spreadsheet_id', 'record_id'],
    idField: 'spreadsheet_id',
    canonicalMime: 'application/vnd.ctox.euro-office-spreadsheet',
  };
}

/**
 * Resolve record + version for an office command (shared by every arm).
 * ref: store_office_commands.rs:42-66.
 */
export async function loadOfficeTarget(command, ctx, binding) {
  if (command.app_id !== binding.moduleId) {
    fail('invalid', `office command module mismatch: expected ${binding.moduleId}, got ${command.app_id}`);
  }
  const payload = command.payload ?? {};
  const recordId = firstString(payload, binding.idKeys) ?? command.record_id;
  if (!recordId) fail('invalid', 'office command record id is required');
  const record = await ctx.get(binding.recordsCollection, recordId);
  if (!record) fail('not_found', `office record not found: ${recordId}`);
  const versionId = firstString(payload, ['version_id', 'base_version_id'])
    ?? (typeof record.current_version_id === 'string' && record.current_version_id ? record.current_version_id : undefined);
  if (!versionId) fail('invalid', 'office command version id is required');
  const version = await ctx.get(binding.versionsCollection, versionId);
  if (!version) fail('not_found', `office version not found: ${versionId}`);
  return { recordId, record, versionId, version };
}

/** Clamp `index_text`/model text at 20 000 Unicode scalar values. */
export function clampText(value) {
  return value === undefined || value === null ? '' : [...String(value)].slice(0, 20_000).join('');
}

// ---------------------------------------------------------------------------
// Kernform der Office-Primitive (Strang CAPSIG, §16.32). BlobRef = files.ref (sha256 des Inhalts), an die
// Primitive als {ref}. Versionsdokumente tragen weiter blob_id = id des files-Dokuments (UI: readFileBytes in
// office-bridge-v4.js) und *_sha256 = dessen ref. Die Zuordnung id ↔ ref ist Fachlogik dieses Handlers.
//   office.edit.prepare({kind, blob_id:{ref}, implemented_features}) → {session, model, manifest, protocol,
//     protocol_version, sha256}                      session = {kind, source:{ref}, editor:{ref}}
//   office.edit.apply({session, changes:{ref}, base_version, implemented_features}) → {session, blob_id:{ref},
//     sha256, manifest, model, protocol, protocol_version}
//   office.render({session, to}) → docx|xlsx: {blob_id:{ref}, sha256, mime_type};
//     email_html: {blob_id:{ref}, sha256, text_blob:{ref}, assets:[{content_id, mime_type, blob_id:{ref}}], compiler_id}
// pdf liefert der Kern nicht (unavailable) → der Handler lehnt vor dem Aufruf ab.

const SHA256_HEX = /^[0-9a-f]{64}$/;

/** ref aus einem BlobRef ({ref}) oder einer bereits nackten ref. */
export function refOf(value) {
  if (typeof value === 'string' && value) return value;
  if (value && typeof value === 'object' && typeof value.ref === 'string' && value.ref) return value.ref;
  return undefined;
}

/**
 * files.ref zu einer blob_id des Versionsdokuments: files-Dokument per id, sonst der sha256 aus den
 * Versionsmetadaten (Altbestand, wie readFileBytes in der UI), sonst die blob_id selbst, wenn sie eine ref ist.
 */
export async function refForBlob(ctx, blobId, fallbackSha256) {
  if (typeof blobId !== 'string' || !blobId) return undefined;
  const meta = await ctx.get('files', blobId);
  if (meta && typeof meta.ref === 'string' && meta.ref) return meta.ref;
  const fallback = typeof fallbackSha256 === 'string' ? fallbackSha256.trim().toLowerCase() : '';
  if (fallback) return fallback;
  if (SHA256_HEX.test(blobId)) return blobId;
  return undefined;
}

/** files-Dokument zu einer vom Primitiv erzeugten ref (§16.32: gleiche Transaktion, owner_app = diese App). */
export async function fileForRef(ctx, ref) {
  if (!ref) return null;
  const [doc] = await ctx.read('files', { selector: { ref }, limit: 1 });
  return doc ?? null;
}

/** blob_id fürs Versionsdokument: id des files-Dokuments mit dieser ref, sonst die ref selbst. */
export async function blobIdForRef(ctx, ref) {
  const doc = await fileForRef(ctx, ref);
  return typeof doc?.id === 'string' && doc.id ? doc.id : ref;
}

/** Bytegröße aus dem files-Dokument (die Primitive liefern keine Größe mehr). */
export async function bytesForRef(ctx, ref) {
  const doc = await fileForRef(ctx, ref);
  return Number.isInteger(doc?.size) ? doc.size : null;
}

/** Zielformat einer Ausgabe; pdf liefert der Kern nicht → sauber ablehnen (Befehlsstatus unavailable). */
export function renderTarget(binding, requested) {
  const format = typeof requested === 'string' && requested.trim() ? requested.trim().toLowerCase() : '';
  const native = binding.kind === 'document' ? 'docx' : 'xlsx';
  if (format === '' || format === native) return native;
  if (format === 'pdf') fail('unavailable', `${binding.kind} export as pdf is not provided by the core office renderer`);
  fail('invalid', `unsupported ${binding.kind} export format: ${format}`);
}

/** Session über office.edit.prepare für die kanonische ref der Version. */
export async function prepareSession(ctx, binding, sourceRef, implementedFeatures) {
  const prepared = await ctx.cap.office.edit.prepare({
    kind: binding.kind,
    blob_id: { ref: sourceRef },
    implemented_features: implementedFeatures,
  });
  const session = prepared?.session;
  if (!refOf(session?.source) || !refOf(session?.editor)) {
    fail('unavailable', 'office.edit.prepare returned no editor session');
  }
  return prepared;
}

/** Session aus der Version (kanonischer Blob + vorbereiteter Editor-Blob); null, wenn nie vorbereitet. */
export async function sessionFromVersion(ctx, binding, version) {
  const source = await refForBlob(ctx, version.blob_id, version.source_sha256);
  const editor = await refForBlob(ctx, version.editor_blob_id, version.editor_sha256);
  if (!source || !editor) return null;
  return { kind: binding.kind, source: { ref: source }, editor: { ref: editor } };
}

/** Kanonische ref der Version oder invalid. */
export async function canonicalRef(ctx, version, message = 'office version has no canonical blob') {
  const ref = await refForBlob(ctx, version.blob_id, version.source_sha256);
  if (!ref) fail('invalid', message);
  return ref;
}

/**
 * Commit-Kern: gestagte Editor-Datei (files-id + editor_sha256 = ref) auf die Session der Basisversion anwenden.
 * Fachlogik hier: id→ref-Zuordnung, Integritätsprüfung der gestagten Datei gegen editor_sha256, Session der
 * Basisversion (vorbereiteter Editor-Blob; nie vorbereitet → erst office.edit.prepare). Die Basisversion des
 * Editors ist seine ref (§16.32: ref = sha256); der Kern prüft sie gegen session.editor.
 */
export async function applyStagedEditor(ctx, binding, version, stagedEditorBlobId, expectedChangedSha256, implementedFeatures) {
  const changesRef = await refForBlob(ctx, stagedEditorBlobId, expectedChangedSha256);
  if (!changesRef) fail('invalid', 'office commit staged editor blob has no file reference');
  const expected = typeof expectedChangedSha256 === 'string' ? expectedChangedSha256.trim().toLowerCase() : '';
  if (expected && expected !== changesRef) {
    fail('conflict', 'staged editor blob does not match editor_sha256');
  }
  const session = (await sessionFromVersion(ctx, binding, version))
    ?? (await prepareSession(ctx, binding, await canonicalRef(ctx, version, 'base office version has no blob'),
      binding.kind === 'document' ? DOCUMENT_FEATURES : SPREADSHEET_FEATURES)).session;
  const applied = await ctx.cap.office.edit.apply({
    session,
    changes: { ref: changesRef },
    base_version: refOf(session.editor),
    implemented_features: implementedFeatures,
  });
  const canonical = refOf(applied?.blob_id);
  const editor = refOf(applied?.session?.editor);
  if (!canonical || !editor) fail('unavailable', 'office.edit.apply returned no canonical package');
  return {
    applied,
    sha256: typeof applied.sha256 === 'string' && applied.sha256 ? applied.sha256 : canonical,
    editorSha256: editor,
    blobId: await blobIdForRef(ctx, canonical),
    editorBlobId: await blobIdForRef(ctx, editor),
    bytes: await bytesForRef(ctx, canonical),
  };
}
