// Origin: CTOX
// License: Apache-2.0
//
// Faithful helpers for the documents report writeback (research.systematic.report.complete, owned by documents).
// ref: store.rs — document_report_lineage :23433, writeback_generated_docx
// :23498, and the resolver helpers :22945-24980 (read-only port).
//
// FACHLOGIK (here + handler): filename/document-id/title/tags resolution,
// lineage field extraction, version numbering and the documents/versions/blob
// records. FAEHIGKEIT: locating + validating + extracting + hashing + chunking
// the generated DOCX (`cap.office.render`) and verifying source-receipt
// snapshots (`cap.doc.extract.verify_source_receipts`).

export function fail(code, message) {
  throw { code, message };
}

function firstStringField(obj, keys) {
  if (!obj || typeof obj !== 'object') return undefined;
  for (const key of keys) {
    const value = obj[key];
    if (typeof value === 'string' && value.trim() !== '') return value;
  }
  return undefined;
}

// ref: expected_docx_filename :24757
export function expectedDocxFilename(command) {
  const contract = command.payload?.writeback_contract ?? null;
  const name = firstStringField(contract, ['filename', 'output_filename'])
    ?? firstStringField(command.payload, ['output_filename', 'filename'])
    ?? firstStringField(command.client_context, ['filename', 'output_filename']);
  if (name === undefined) return undefined;
  return name.toLowerCase().endsWith('.docx') ? name : `${name}.docx`;
}

// ref: slug_for_document_id :24964
function slugForDocumentId(value) {
  const stem = String(value).replace(/\.[^.]*$/, '');
  const slug = [...stem].map((ch) => (/[a-z0-9]/i.test(ch) ? ch.toLowerCase() : '-')).join('')
    .replace(/-+/g, '-').replace(/^-|-$/g, '');
  return slug === '' ? 'document' : slug;
}

// ref: preferred_document_id :24908
export function preferredDocumentId(command, filename) {
  const recordId = typeof command.record_id === 'string' ? command.record_id.trim() : '';
  if (recordId !== '') return recordId;
  const contractId = command.payload?.writeback_contract?.document_id;
  if (typeof contractId === 'string' && contractId.trim() !== '') return contractId.trim();
  return `doc_${slugForDocumentId(filename)}`;
}

// ref: title_from_filename :24955
export function titleFromFilename(filename) {
  const stem = String(filename).replace(/\.[^.]*$/, '').replace(/[_-]/g, ' ');
  return stem.trim() === '' ? 'Generated document' : stem;
}

// ref: tags_from_command :24928
export function tagsFromCommand(command) {
  const value = command.payload?.tags ?? command.client_context?.tags ?? null;
  if (Array.isArray(value)) {
    return value.filter((v) => typeof v === 'string').map((v) => v.trim()).filter((v) => v !== '');
  }
  if (typeof value === 'string') {
    return value.split(',').map((v) => v.trim()).filter((v) => v !== '');
  }
  return [];
}

// ref: document_knowledge_context :22945
export function knowledgeContext(command) {
  const payloadCtx = command.payload?.knowledge_context;
  if (payloadCtx && typeof payloadCtx === 'object' && !Array.isArray(payloadCtx)) return payloadCtx;
  const clientCtx = command.client_context?.knowledge_context;
  if (clientCtx && typeof clientCtx === 'object' && !Array.isArray(clientCtx)) return clientCtx;
  return null;
}

// ref: document_linked_records :22961
export function linkedRecords(command) {
  const contractLinks = command.payload?.writeback_contract?.linked_records;
  if (Array.isArray(contractLinks)) return contractLinks;
  const context = knowledgeContext(command);
  const id = context && typeof context.id === 'string' ? context.id.trim() : '';
  if (id === '') return [];
  return [{ type: 'knowledge', id, updated_at_ms: context.updated_at_ms ?? null }];
}

// ref: document_report_is_research :22997
export function isResearch(command) {
  if (command.type === 'research.systematic.report.create') return true;
  const mode = command.payload?.research_mode;
  if (typeof mode === 'string' && mode.trim() !== '') return true;
  const skills = command.payload?.required_skills;
  if (Array.isArray(skills) && skills.some((s) => typeof s === 'string' && s.trim() === 'systematic-research')) return true;
  return false;
}

// ref: document_knowledge_version :23096 (payload/client_context/context branches;
// the full linked-records fallback stays in the capability where receipts resolve).
export function knowledgeVersion(command) {
  for (const obj of [command.payload, command.client_context]) {
    if (!obj || typeof obj !== 'object') continue;
    if (obj.knowledge_version !== undefined && obj.knowledge_version !== null) return obj.knowledge_version;
    const vid = typeof obj.knowledge_version_id === 'string' ? obj.knowledge_version_id.trim() : '';
    if (vid !== '') return { version_id: vid };
  }
  const context = knowledgeContext(command);
  if (context && typeof context === 'object') {
    for (const key of ['knowledge_version', 'version_id', 'version']) {
      const value = context[key];
      if ((typeof value === 'string' && value.trim() !== '') || (value && typeof value === 'object')) {
        return key === 'knowledge_version' ? value : { [key]: value };
      }
    }
    if (typeof context.id === 'string' && context.id.trim() !== '' && Number(context.updated_at_ms) > 0) {
      const version = { id: context.id, updated_at_ms: context.updated_at_ms };
      for (const key of ['content_hash', 'schema_hash']) {
        if (typeof context[key] === 'string') version[key] = context[key];
      }
      return version;
    }
  }
  return null;
}

// ref: document_source_receipt_snapshot_hash :23194
function receiptSnapshotHash(link) {
  return firstStringField(link, ['snapshot_hash', 'source_receipt_snapshot_hash', 'receipt_snapshot_hash']);
}

// ref: document_source_receipt_links :23032 (link SELECTION only; resolution +
// eligibility + snapshot-byte verification is cap.doc.extract.verify_source_receipts).
export function sourceReceiptLinks(command, links) {
  const selected = links.filter((record) => {
    const kind = (firstStringField(record, ['kind', 'type', 'record_type', 'role']) ?? '').toLowerCase();
    const collection = (firstStringField(record, ['collection', 'table_collection']) ?? '').toLowerCase();
    return kind.includes('source_receipt') || kind.includes('source-receipt')
      || collection === 'source_receipts'
      || (collection === 'knowledge_tables' && receiptSnapshotHash(record) !== undefined)
      || (kind === 'source' && receiptSnapshotHash(record) !== undefined)
      || (kind.includes('evidence') && !kind.includes('knowledge'));
  });
  if (selected.length > 0) return selected;
  const extra = [];
  for (const key of ['source_receipt_links', 'source_receipts']) {
    const arr = command.payload?.[key];
    if (Array.isArray(arr)) extra.push(...arr);
  }
  return extra;
}

// ---------------------------------------------------------------------------
// Quellbeleg-Prüfung (Strang CAPSIG): früher cap.doc.extract.verify_source_receipts, jetzt Handler-Logik über
// generische Primitive. ref: store.rs document_source_receipt_candidate :23282 (Zeile aus Knowledge-Tabellen),
// _is_eligible :23289, _snapshot_matches :23395, document_report_lineage :23433.
//   Zeile:     ctx.cap.dataframe.window({table, skip, limit}) über die Knowledge-Tabelle (Knowledge-Katalog
//              knowledge_tables über foreign_collections, Export der App knowledge).
//   Snapshot:  §16.32 – Snapshot-Bytes liegen inhaltsadressiert in files (ref = sha256); der Kern prüft den Hash
//              beim Empfang. „Bytes passen zum Beleg-Hash“ = es gibt einen files-Datensatz mit ref = <hex>.
//              Altbestand (snapshot_path auf der Platte) kommt über den Kern-Import in files (V4-HANDLERS.md).

const WINDOW_LIMIT = 1000;

function receiptReference(link) {
  return firstStringField(link, ['source_id', 'receipt_id', 'evidence_id', 'record_id', 'id']);
}

// ref: knowledge_receipt_table_id — ein 'table:'-Präfix und ein ':chunk:<n>'-Suffix entfernen.
export function receiptTableId(raw) {
  const stripped = String(raw).startsWith('table:') ? String(raw).slice('table:'.length) : String(raw);
  const i = stripped.lastIndexOf(':chunk:');
  if (i >= 0 && /^\d+$/.test(stripped.slice(i + ':chunk:'.length))) return stripped.slice(0, i);
  return stripped;
}

function rowMatches(row, reference) {
  return ['source_id', 'receipt_id', 'evidence_id', 'id'].some((key) => typeof row?.[key] === 'string' && row[key] === reference);
}

async function receiptRowInTable(ctx, tableId, reference) {
  for (let skip = 0; ; skip += WINDOW_LIMIT) {
    let window;
    try {
      window = await ctx.cap.dataframe.window({ table: tableId, skip, limit: WINDOW_LIMIT });
    } catch (error) {
      if (error?.code === 'not_found') return null;
      throw error;
    }
    const rows = Array.isArray(window?.rows) ? window.rows : [];
    const hit = rows.find((row) => rowMatches(row, reference));
    if (hit) return hit;
    const total = Number.isInteger(window?.row_count) ? window.row_count : skip + rows.length;
    if (rows.length < WINDOW_LIMIT || skip + rows.length >= total) return null;
  }
}

async function activeKnowledgeTables(ctx) {
  const tables = await ctx.read('knowledge_tables', { selector: {} });
  return tables
    .filter((t) => t.status !== 'archived' && t.archived !== true && t._deleted !== true)
    .map((t) => receiptTableId(t.table_id ?? t.logical_table_id ?? t.id))
    .filter((id) => id);
}

async function receiptCandidate(ctx, link) {
  const reference = receiptReference(link);
  if (reference === undefined) return null;
  const tableRef = firstStringField(link, ['table_id', 'knowledge_table_id', 'table_record_id', 'table_ref']);
  let alreadyRead = null;
  if (tableRef !== undefined) {
    alreadyRead = receiptTableId(tableRef);
    const row = await receiptRowInTable(ctx, alreadyRead, reference);
    if (row) return row;
  }
  for (const tableId of await activeKnowledgeTables(ctx)) {
    if (tableId === alreadyRead) continue;
    const row = await receiptRowInTable(ctx, tableId, reference);
    if (row) return row;
  }
  return null;
}

const validSnapshotHash = (value) => typeof value === 'string' && /^sha256:[0-9a-fA-F]{64}$/.test(value.trim());

const METADATA_URL_PREFIXES = [
  'https://doi.org/', 'http://doi.org/', 'https://api.crossref.org/', 'https://api.openalex.org/',
  'https://api.semanticscholar.org/', 'https://www.semanticscholar.org/', 'https://scholar.google.',
  'https://www.researchgate.net/', 'https://www.academia.edu/',
];

// ref: document_source_receipt_is_eligible :23289 (wörtlich portiert)
export function receiptIsEligible(value) {
  const str = (key) => (typeof value?.[key] === 'string' ? value[key] : undefined);
  const has = (key) => (str(key) ?? '').trim() !== '';
  let canonicalUrl = false;
  const url = str('canonical_url');
  if (url !== undefined && url === url.trim()) {
    try {
      const parsed = new URL(url);
      canonicalUrl = (parsed.protocol === 'http:' || parsed.protocol === 'https:') && parsed.hostname !== '';
    } catch { canonicalUrl = false; }
  }
  const tier = (str('source_tier') ?? '').trim().toLowerCase();
  const type = (str('source_type') ?? '').trim().toLowerCase();
  const statuses = ['review_status', 'source_status', 'relevance_status', 'status']
    .map((k) => str(k)).filter((v) => v !== undefined).map((v) => v.trim().toLowerCase());
  const urlRole = (str('url_role') ?? '').trim().toLowerCase();
  const scope = (str('content_scope') ?? '').trim().toLowerCase();
  const score = value?.evidence_relevance_score;
  const http = value?.http_status;
  return validSnapshotHash(str('snapshot_hash'))
    && value.evidence_eligible === true
    && value.verification_status === 'verified'
    && value.transport_verified === true
    && value.content_extracted === true
    && value.actual_full_text_or_data === true
    && Number.isInteger(score) && score >= 8
    && Number.isInteger(http) && http >= 200 && http <= 299 && http !== 204
    && canonicalUrl
    && tier !== '' && !tier.includes('metadata') && !tier.includes('aggregat')
    && !type.includes('metadata') && type !== 'aggregator'
    && !METADATA_URL_PREFIXES.some((p) => url.trim().toLowerCase().startsWith(p))
    && value.metadata_only !== true
    && !((str('evidence_rejection_reason') ?? '').trim() !== '')
    && has('source_id') && (has('claim_id') || has('evidence_id')) && has('snapshot_id') && has('retrieved_at')
    && ['original_content', 'original_data', 'publisher_full_text', 'dataset_archive'].includes(urlRole)
    && ['full_text', 'original_data', 'full_dataset', 'dataset_archive'].includes(scope)
    && !statuses.some((s) => ['rejected', 'off_topic', 'off-topic', 'irrelevant', 'fachfremd'].includes(s));
}

/** Snapshot-Bytes vorhanden und passend: files-Datensatz mit ref = sha256 (§16.32, inhaltsadressiert). */
async function snapshotPersisted(ctx, snapshotHash) {
  const hex = snapshotHash.trim().slice('sha256:'.length).toLowerCase();
  const [doc] = await ctx.read('files', { selector: { ref: hex }, limit: 1 });
  return Boolean(doc);
}

/** ref: document_report_lineage :23433 – Schleife über die Beleg-Links; liefert die geprüften Snapshot-Hashes. */
export async function verifySourceReceipts(ctx, links) {
  const hashes = [];
  for (const link of links) {
    const candidate = await receiptCandidate(ctx, link);
    if (!candidate) fail('invalid', 'source receipt link did not resolve to a receipt');
    if (!receiptIsEligible(candidate)) {
      fail('invalid', 'research DOCX writeback requires an eligible source receipt with a verified snapshot hash');
    }
    const snapshotHash = candidate.snapshot_hash;
    if (!(await snapshotPersisted(ctx, snapshotHash))) {
      fail('conflict', 'research DOCX writeback requires persisted snapshot bytes matching the receipt hash');
    }
    const requested = firstStringField(link, ['snapshot_hash', 'source_receipt_snapshot_hash', 'receipt_snapshot_hash']);
    if (requested !== undefined && requested !== snapshotHash) {
      fail('conflict', 'source receipt snapshot hash changed; refusing to write unbound research DOCX');
    }
    if (!hashes.includes(snapshotHash)) hashes.push(snapshotHash);
  }
  return hashes;
}

/**
 * Erzeugte DOCX als files-ref: Writeback-Vertrag/Payload nennt sie (file_ref | blob_ref | output_file_ref,
 * auch {ref}), sonst die Datei im Workspace der Agentenaufgabe (`workspaceRef`, Rückruf §16.30), sonst der
 * jüngste files-Datensatz mit genau diesem Dateinamen.
 */
export async function generatedDocxRef(ctx, command, filename, workspaceRef = null) {
  const contract = command.payload?.writeback_contract ?? null;
  for (const obj of [contract, command.payload, command.client_context]) {
    if (!obj || typeof obj !== 'object') continue;
    for (const key of ['output_file_ref', 'file_ref', 'blob_ref', 'source_file_ref']) {
      const value = obj[key];
      if (typeof value === 'string' && value.trim()) return value.trim();
      if (value && typeof value === 'object' && typeof value.ref === 'string' && value.ref) return value.ref;
    }
  }
  if (typeof workspaceRef === 'function') {
    const ref = await workspaceRef(filename);
    if (typeof ref === 'string' && ref) return ref;
  }
  const files = await ctx.read('files', { selector: { name: filename } });
  const newest = files.sort((a, b) => (b.created_at_ms ?? b.updated_at_ms ?? 0) - (a.created_at_ms ?? a.updated_at_ms ?? 0))[0];
  return typeof newest?.ref === 'string' && newest.ref ? newest.ref : undefined;
}
