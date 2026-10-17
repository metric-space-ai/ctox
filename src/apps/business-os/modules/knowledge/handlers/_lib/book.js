// Knowledge-Book-Datei aufnehmen (Strang KNOW2, Entscheidung: Wissensdaten gehören der App knowledge).
//
// Andere Apps übergeben ein fertiges Knowledge Book nur über den deklarierten Befehl knowledge.book.ingest
// (ctx.command an module 'knowledge', heute research.knowledge.refresh.complete). Die Datei liegt als files-Datensatz
// vor, der Befehl trägt nur die BlobRef {ref} (Kern §16.32). knowledge liest sie über ctx.files.get, prüft sie
// fail-closed gegen das eigene Format ctox.knowledge.book.v1 und legt die Elemente in knowledge_items /
// knowledge_runbooks ab. Den Vorgang spiegelt knowledge_tasks (id = Befehls-ID, kein Agenten-Auftrag): ein
// ungültiges oder fehlendes Book erscheint dort als failed und damit einmal in der Zustandszone der App.
// Idempotent: derselbe Befehl (gleiche ID) wirkt höchstens einmal.
import { TASKS, invalid } from './agent.js';
import { elementDoc } from './callback.js';

export const INGEST_COMMAND = 'knowledge.book.ingest';
export const BOOK_SCHEMA = 'ctox.knowledge.book.v1';
export const BOOK_MAX_BYTES = 4 * 1024 * 1024;
const ITEM_KINDS = new Set(['skillbook', 'skill', 'resource']);
const KINDS = new Set([...ITEM_KINDS, 'runbook']);
const TERMINAL = new Set(['completed', 'failed', 'cancelled']);
const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const text = (v) => (typeof v === 'string' ? v.trim() : '');

/** Book-Datei (JSON-Text) prüfen. Liefert {ok, book} oder {ok:false, reason}. */
export function validateBook(raw, expect = {}) {
  let data;
  try { data = JSON.parse(raw); } catch { return { ok: false, reason: 'knowledge book is not valid JSON' }; }
  if (!isObject(data)) return { ok: false, reason: 'knowledge book must be an object' };
  if (data.schema_version !== BOOK_SCHEMA) return { ok: false, reason: `schema_version must be ${BOOK_SCHEMA}` };
  if (!text(data.knowledge_domain)) return { ok: false, reason: 'knowledge_domain is required' };
  if (text(expect.knowledge_domain) && text(data.knowledge_domain) !== text(expect.knowledge_domain)) {
    return { ok: false, reason: 'knowledge_domain does not match the command' };
  }
  const elements = Array.isArray(data.elements) ? data.elements : [];
  if (!elements.length) return { ok: false, reason: 'elements must not be empty' };
  const seen = new Set();
  for (const [i, el] of elements.entries()) {
    if (!isObject(el) || !text(el.id) || !text(el.title) || !KINDS.has(el.kind)) return { ok: false, reason: `elements[${i}] needs id, title and a known kind` };
    if (el.id.length > 180) return { ok: false, reason: `elements[${i}].id longer than 180` };
    if (seen.has(el.id)) return { ok: false, reason: `element id ${el.id} is duplicated` };
    seen.add(el.id);
    if (el.citations !== undefined && !Array.isArray(el.citations)) return { ok: false, reason: `elements[${i}].citations must be an array` };
  }
  return { ok: true, book: { ...data, elements } };
}

async function readBook(ctx, ref) {
  if (typeof ctx.files?.get !== 'function') throw { code: 'unavailable', message: 'ctx.files.get is not available' };
  let blob;
  try {
    blob = await ctx.files.get(ref);
  } catch (error) {
    if (error?.code === 'not_found' || error?.code === 'revoked') return { ok: false, reason: `knowledge book ${ref}: ${error.code}` };
    throw error; // unavailable u. a.: wiederholbar, nichts geschrieben
  }
  const size = typeof blob === 'string' ? new TextEncoder().encode(blob).length : (blob?.size ?? blob?.byteLength ?? 0);
  if (size > BOOK_MAX_BYTES) return { ok: false, reason: `knowledge book larger than ${BOOK_MAX_BYTES} bytes` };
  if (typeof blob === 'string') return { ok: true, raw: blob };
  if (typeof blob?.text === 'function') return { ok: true, raw: await blob.text() };
  if (blob instanceof ArrayBuffer || ArrayBuffer.isView(blob)) {
    try { return { ok: true, raw: new TextDecoder('utf-8', { fatal: true }).decode(blob) }; } catch { return { ok: false, reason: 'knowledge book is not UTF-8' }; }
  }
  return { ok: false, reason: 'knowledge book is unreadable' };
}

/** knowledge.book.ingest: Book aus files lesen, prüfen, Elemente upserten, Vorgang in knowledge_tasks. */
export async function ingestBook(command, ctx) {
  const p = isObject(command.payload) ? command.payload : {};
  const ref = text(p.book?.ref);
  if (!ref) throw invalid('book.ref is required');
  const recordId = text(command.record_id) || text(p.record_id);
  const existing = await ctx.get(TASKS, command.id);
  if (existing && TERMINAL.has(existing.status)) return { ok: existing.status === 'completed', command_id: command.id, status: existing.status, duplicate: true };

  const read = await readBook(ctx, ref);
  const checked = read.ok ? validateBook(read.raw, { knowledge_domain: p.knowledge_domain }) : read;
  const now = ctx.now();
  const source = isObject(p.source) ? p.source : {};
  const title = text(p.title) || `Knowledge Book · ${text(checked.book?.title) || text(p.knowledge_domain) || recordId || ref.slice(0, 12)}`;
  await ctx.transaction(async (tx) => {
    const current = await tx.get(TASKS, command.id);
    if (current && TERMINAL.has(current.status)) return;
    const record = {
      id: command.id,
      command_id: command.id,
      command_type: command.type || INGEST_COMMAND,
      record_id: recordId,
      title,
      status: 'completed',
      created_at_ms: Number(current?.created_at_ms) || now,
      updated_at_ms: now,
    };
    if (!checked.ok) {
      record.status = 'failed';
      record.error = { status: 'failed', code: 'invalid_book', message: checked.reason };
      await tx.put(TASKS, record);
      return;
    }
    const { book } = checked;
    const provenance = {
      book_ref: ref,
      import_command_id: command.id,
      knowledge_domain: text(book.knowledge_domain),
      knowledge_version_id: text(book.knowledge_version_id) || text(p.knowledge_version_id),
      source,
    };
    const items = [];
    const runbooks = [];
    for (const el of book.elements) {
      const runbook = el.kind === 'runbook';
      const collection = runbook ? 'knowledge_runbooks' : 'knowledge_items';
      const doc = elementDoc({
        ...el,
        payload: { ...(isObject(el.payload) ? el.payload : {}), citations: Array.isArray(el.citations) ? el.citations : [], ...provenance },
      }, await tx.get(collection, el.id), el.kind, command.id, now);
      await tx.put(collection, doc);
      (runbook ? runbooks : items).push(el.id);
    }
    record.summary = text(book.summary);
    record.result = { item_ids: items, runbook_ids: runbooks, book_ref: ref, gaps: Array.isArray(book.gaps) ? book.gaps : [] };
    await tx.put(TASKS, record);
  });
  return checked.ok
    ? { ok: true, command_id: command.id, status: 'completed', elements: checked.book.elements.length }
    : { ok: false, command_id: command.id, status: 'failed', reason: checked.reason };
}
