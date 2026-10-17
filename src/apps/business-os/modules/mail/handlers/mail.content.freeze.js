// Origin: CTOX
// License: Apache-2.0
// mail.content.freeze – Word-Inhalt einer Mail-Gruppe einfrieren (Strang MAIL3, Shell v4).
//
// Owner-Regel: mail hängt an keiner anderen App. Der Handler liest keine documents-Collections und sendet keinen
// documents-Befehl; die Quelle kommt als BlobRef im Payload, alles Native läuft über generische OS-Primitive
// (Vertrag: ~/.local/state/ctox-sync-v4/docs/befund-cap-2.md, „Parameter-/Ergebnisvertrag für die Ports“):
//   office.edit.prepare({kind:'document', blob_id:BlobRef}) → {session, …}
//   office.render({session, to:'email_html'})              → {blob_id:BlobRef(html), sha256, text_blob:BlobRef, assets, compiler_id}
//   blob.materialize({blob_id:BlobRef})                     → {blob_id, sha256}
// office.edit.apply wird bewusst NICHT gerufen: Einfrieren ändert den Inhalt nicht (apply = Änderungen anwenden).
// Fachliche Referenz (nur gelesen): modules/documents/handlers/office.document.freeze_email_content.js.
//
// Fachlogik hier: Payload prüfen (BlobRef-Form, erwarteter Quell-Hash), Ergebnis der Primitive prüfen
// (Session, HTML/Text vorhanden, materialisierte Hashes = Referenz), Ergebnis als files-Referenzen formen.
// Keine Schreibvorgänge: das Ergebnis steht im Befehlsergebnis, der Editor übernimmt es in die Inhaltsrevision.
import { callCap, fail, isObject, payloadOf, payloadString } from './_base.js';

const MAX_BLOB_BYTES = 64 * 1024 * 1024; // caps/blob.rs MAX_BLOB_BYTES
const SHA256_HEX = /^[0-9a-f]{64}$/;
export const HTML_MEDIA_TYPE = 'text/html; charset=utf-8';
export const TEXT_MEDIA_TYPE = 'text/plain; charset=utf-8';

/** BlobRef {sha256, size, transfer_id} nach caps/blob.rs BlobRef::validate; sonst `code` mit Pfad. */
export function blobRef(value, path, code = 'invalid') {
  if (!isObject(value)) fail(code, `${path} must be a BlobRef {sha256,size,transfer_id}`);
  const { sha256, size, transfer_id: transferId } = value;
  if (typeof sha256 !== 'string' || !SHA256_HEX.test(sha256)) fail(code, `${path}.sha256 must be 64 lowercase hex chars`);
  if (!Number.isInteger(size) || size < 0 || size > MAX_BLOB_BYTES) fail(code, `${path}.size must be an integer 0..${MAX_BLOB_BYTES}`);
  if (typeof transferId !== 'string' || transferId.trim() === '') fail(code, `${path}.transfer_id is required`);
  return { sha256, size, transfer_id: transferId };
}

/** files-Referenz (inhaltsadressiert, api.files.get(ref)) plus Transfer-BlobRef als Herkunft. */
function fileRef(ref, mediaType) {
  return { storage: 'files', ref: ref.sha256, sha256: ref.sha256, size: ref.size, media_type: mediaType, blob: ref };
}

async function materialized(ctx, ref, path) {
  const out = await callCap(ctx, 'blob.materialize', { blob_id: ref });
  if (!isObject(out) || out.sha256 !== ref.sha256) {
    fail('conflict', `${path}: materialized blob differs from its CAS reference`);
  }
  return ref;
}

export default async function handle(command, ctx) {
  const payload = payloadOf(command);
  const source = blobRef(payload.source, 'payload.source');
  const expected = payloadString(payload, 'expected_source_sha256');
  if (expected !== undefined && expected !== source.sha256) {
    fail('conflict', 'source blob differs from the expected source sha256');
  }

  const prepared = await callCap(ctx, 'office.edit.prepare', { kind: 'document', blob_id: source });
  const session = prepared?.session;
  if (!isObject(session) || session.kind !== 'document') fail('unavailable', 'office.edit.prepare returned no document session');
  const canonical = blobRef(session.source, 'session.source', 'unavailable');
  const editor = blobRef(session.editor, 'session.editor', 'unavailable');

  // OfficeSession ist deny_unknown_fields (caps/office.rs): genau {kind, source, editor} zurückgeben.
  const rendered = await callCap(ctx, 'office.render', {
    session: { kind: 'document', source: canonical, editor },
    to: 'email_html',
  });
  if (!isObject(rendered)) fail('unavailable', 'office.render returned no email result');
  const html = blobRef(rendered.blob_id, 'render.blob_id', 'unavailable');
  const text = blobRef(rendered.text_blob, 'render.text_blob', 'unavailable');
  const assets = Array.isArray(rendered.assets) ? rendered.assets : [];
  const assetRefs = assets.map((asset, index) => ({
    content_id: typeof asset?.content_id === 'string' ? asset.content_id : '',
    mime_type: typeof asset?.mime_type === 'string' ? asset.mime_type : 'application/octet-stream',
    ref: blobRef(asset?.blob_id, `render.assets[${index}].blob_id`, 'unavailable'),
  }));

  // Jede Transportdatei einmal materialisieren: lesbar und hash-gleich, bevor der Editor sie übernimmt.
  await materialized(ctx, html, 'html');
  await materialized(ctx, text, 'text');
  for (const [index, asset] of assetRefs.entries()) await materialized(ctx, asset.ref, `assets[${index}]`);

  const documentId = payloadString(payload, 'document_id');
  const versionId = payloadString(payload, 'version_id');
  const artifact = {
    schema_version: 1,
    state: 'frozen',
    compiler_id: typeof rendered.compiler_id === 'string' ? rendered.compiler_id : '',
    // Nur Herkunftsetiketten aus dem Payload; mail löst sie nicht auf.
    ...(documentId ? { document_id: documentId } : {}),
    ...(versionId ? { document_version_id: versionId } : {}),
    source: canonical,
    source_sha256: canonical.sha256,
    html: fileRef(html, HTML_MEDIA_TYPE),
    text: fileRef(text, TEXT_MEDIA_TYPE),
    assets: assetRefs.map(({ content_id: contentId, mime_type: mimeType, ref }) => ({
      content_id: contentId,
      ...fileRef(ref, mimeType),
    })),
    diagnostics: Array.isArray(rendered.diagnostics) ? rendered.diagnostics : [],
    frozen_at_ms: ctx.now(),
  };
  return { ok: true, operation: 'mail.content.freeze', artifact };
}
