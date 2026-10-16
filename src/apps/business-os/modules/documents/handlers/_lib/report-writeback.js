// Origin: CTOX
// License: Apache-2.0
// DOCX-Rückschrieb für research.systematic.report.complete — ref: store.rs:21471 process_documents_report_command,
// :23433 document_report_lineage, :23498 writeback_generated_docx (GEMISCHT). Bis SKL-B3 lief diese Auswertung im
// Handler research.systematic.report.create bzw. nativ im Core (R1 complete_business_command_from_queue_reply
// :10325); jetzt im on_complete-Rückruf der Agentenaufgabe (Kern §16.30).
//
// FACHLOGIK (hier): filename/document-id/title/tags/lineage resolution, version numbering and the
// documents + document_versions records, and (Strang CAPSIG) the source-receipt verification (report.js
// verifySourceReceipts over dataframe.window + files). FAEHIGKEIT (Kernform §16.32): the generated DOCX is a files
// record (ref); cap.office.edit.prepare validates it as a document and returns its manifest (primary_text). Kein
// Rendern: die erzeugte Datei bleibt das Original.
import {
  fail, expectedDocxFilename, preferredDocumentId, titleFromFilename, tagsFromCommand,
  knowledgeContext, linkedRecords, isResearch, knowledgeVersion, sourceReceiptLinks,
  verifySourceReceipts, generatedDocxRef,
} from './report.js';
import { clampText, DOCUMENT_FEATURES, blobIdForRef, bytesForRef } from './office.js';

const MIME = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';

// ref: document_report_lineage :23433
async function resolveLineage(command, ctx) {
  const linked = linkedRecords(command);
  if (!isResearch(command)) {
    return { linked_records: linked, source_receipt_snapshot_hashes: [], knowledge_version: null };
  }
  const version = knowledgeVersion(command);
  if (version === null || version === undefined) {
    fail('invalid', 'research DOCX writeback requires an exact Knowledge version');
  }
  const links = sourceReceiptLinks(command, linked);
  if (links.length === 0) {
    fail('invalid', 'research DOCX writeback requires requested source receipt links; refusing fallback completion');
  }
  // Each link resolves to a Knowledge receipt row, is checked for eligibility and
  // for persisted snapshot bytes (files ref) — Fachlogik in _lib/report.js.
  const hashes = await verifySourceReceipts(ctx, links);
  if (hashes.length === 0) {
    fail('invalid', 'research DOCX writeback requires at least one eligible source receipt');
  }
  return { linked_records: linked, source_receipt_snapshot_hashes: hashes, knowledge_version: version };
}

/**
 * Rückschrieb der erzeugten DOCX (Fachlogik unverändert aus dem bisherigen Handler research.systematic.report.create,
 * Strang H7/CAPSIG). `command` ist der Ausgangsbefehl samt Agenten-Antwort (Lineage, Datei-Ref);
 * `workspaceRef` sucht die DOCX im Workspace der Aufgabe, bevor files nach Dateinamen gefragt wird.
 */
export async function writebackGeneratedDocx(command, ctx, { workspaceRef = null } = {}) {
  const commandId = command.id;
  if (!commandId) fail('invalid', 'documents report command id is required');
  const filename = expectedDocxFilename(command);
  if (filename === undefined) fail('invalid', `documents command ${commandId} has no expected DOCX filename`);

  const lineage = await resolveLineage(command, ctx);
  const documentId = preferredDocumentId(command, filename);
  const title = (typeof command.payload?.title === 'string' && command.payload.title.trim() !== '')
    ? command.payload.title : titleFromFilename(filename);
  const description = (typeof command.payload?.description === 'string' && command.payload.description.trim() !== ''
    ? command.payload.description
    : (typeof command.payload?.summary === 'string' ? command.payload.summary : undefined))
    ?? (typeof command.client_context?.description === 'string' ? command.client_context.description
      : (typeof command.client_context?.summary === 'string' ? command.client_context.summary : ''));
  const tags = tagsFromCommand(command);

  // Version numbering: MAX(version)+1 across this document's versions.
  const existing = await ctx.read('document_versions', { selector: { document_id: documentId } });
  const maxVersion = existing.reduce((m, v) => (Number.isInteger(v.version) && v.version > m ? v.version : m), 0);
  const version = maxVersion + 1;
  const versionId = `${documentId}_v${version}`;

  // The generated DOCX is a files record (ref = sha256). office.edit.prepare
  // validates it as a document (zip, [Content_Types].xml, word/document.xml) and
  // yields the manifest text; the generated file itself stays the version blob.
  const docxRef = await generatedDocxRef(ctx, command, filename, workspaceRef);
  if (docxRef === undefined) fail('not_found', `generated DOCX ${filename} is not available as a file`);
  const prepared = await ctx.cap.office.edit.prepare({
    kind: 'document',
    blob_id: { ref: docxRef },
    implemented_features: DOCUMENT_FEATURES,
  });
  const indexText = clampText(prepared?.manifest?.primary_text);
  const sourceSha256 = docxRef;
  const blobId = await blobIdForRef(ctx, docxRef);
  const sourcePath = null;

  const now = ctx.now();
  const existingDoc = await ctx.get('documents', documentId);
  const createdAtMs = Number.isInteger(existingDoc?.created_at_ms) ? existingDoc.created_at_ms : now;
  const knowledgeLineage = {
    knowledge_version: lineage.knowledge_version,
    source_receipt_snapshot_hashes: lineage.source_receipt_snapshot_hashes,
  };
  const modelJson = {
    type: 'docx',
    source_kind: 'ctox_generated_docx',
    filename,
    title,
    index_text: indexText,
    knowledge_context: knowledgeContext(command),
    knowledge_lineage: knowledgeLineage,
  };
  const diagnostics = [{
    level: 'info',
    message: 'DOCX structure validated during Business OS writeback',
    checks: ['zip', '[Content_Types].xml', 'word/document.xml'],
  }];

  const documentPayload = {
    id: documentId, document_id: documentId, title, filename, description,
    mime_type: MIME, status: 'Draft', document_type: 'word_document', owner_id: '',
    current_version_id: versionId, source_sha256: sourceSha256,
    source_receipt_snapshot_hashes: lineage.source_receipt_snapshot_hashes,
    knowledge_version: lineage.knowledge_version, knowledge_lineage: knowledgeLineage,
    page_count: 0, diagnostics_count: 1, linked_records: lineage.linked_records,
    display_cache: {}, tags, index_text: indexText, is_deleted: false,
    source_kind: 'ctox_generated_docx', business_command_id: commandId, source_path: sourcePath,
    created_at_ms: createdAtMs, updated_at_ms: now,
  };
  const versionPayload = {
    id: versionId, version_id: versionId, document_id: documentId, version,
    source_kind: 'ctox_generated_docx', blob_id: blobId, diagnostics,
    model_json: modelJson, source_receipt_snapshot_hashes: lineage.source_receipt_snapshot_hashes,
    knowledge_version: lineage.knowledge_version, knowledge_lineage: knowledgeLineage,
    model: null, business_command_id: commandId, source_path: sourcePath,
    created_at_ms: now, updated_at_ms: now,
  };

  await ctx.transaction(async (tx) => {
    await tx.put('document_versions', versionPayload);
    await tx.put('documents', documentPayload);
  });

  return {
    ok: true,
    document_id: documentId,
    version_id: versionId,
    blob_id: blobId,
    title,
    filename,
    mime_type: MIME,
    document_type: 'word_document',
    source_sha256: sourceSha256,
    bytes: await bytesForRef(ctx, docxRef),
    path: sourcePath,
    index_text_chars: [...indexText].length,
    source_receipt_snapshot_hashes: lineage.source_receipt_snapshot_hashes,
    knowledge_version: lineage.knowledge_version,
  };
}
