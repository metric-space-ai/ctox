// Origin: CTOX
// License: Apache-2.0
// ctox.cv_print.apply_parse – CV-PDF in ein `ctox.cv_print_profile.v1`-Profil überführen.
// ref (@ 4794af03d, src/core/business_os/store.rs):
//   default  : Queue-Zulassung. Heute sendet die UI `business_os.chat.task` mit writeback_contract
//              {command_type:'ctox.cv_print.apply_parse'} (index.js:1934-1975); der Kern erkennt das über
//              is_cv_print_parse_command :23772, baut den Prompt cv_print_command_prompt :27384 und reichert ihn mit
//              dem PDF-Text an (cv_print_pdf_text_prompt_enrichment :25926; Größengrenze :212 32 MiB, Text :211
//              24 000 Zeichen). Skill: App-Skill cv-print-builder/parser
//              (skills/parser, früher ctox-cv-print-parser über suggested_skill_for_command :27736).
//   onReply  : writeback_cv_print_parse :23866 – Profil-JSON aus der Antwort, Umschlag normalisieren, neue
//              document_versions-Version, documents patchen (process_cv_print_parse_command :24092).
// Fähigkeiten: Agenten-Queue → ctx.cap.agent.task.enqueue (§16.30, flach, Rückruf-Befehle
//              ctox.cv_print.apply_parse.done / .failed; Prompt aus prompts/parse.md über ctx.asset); PDF-Text → ctx.cap.doc.extract({blob_id:{ref}, kind:'pdf'}) (§16.32).
// Quellen (Strang CAPSIG): jede Quelle ist ein files-Datensatz (§16.32). Neue Profile tragen `source_file_ref`;
// ältere Profile (`source_file_id`) liegen nach dem Kern-Import desktop_files→files unter derselben id in files.
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { asU64, callCap, fail, firstStringField, isObject, payloadOf } from './_base.js';

export const CV_PRINT_PDF_MAX_BYTES = 32 * 1024 * 1024;
export const CV_PRINT_PDF_TEXT_MAX_CHARS = 24_000;
const MODULE_ID = 'cv-print-builder';

/** ref: store.rs:27724 truncate_text_preserve (Zeichen, nicht Bytes) */
export function truncateTextPreserve(value, maxChars) {
  const chars = [...value];
  if (chars.length <= maxChars) return value;
  return `${chars.slice(0, Math.max(0, maxChars - 80)).join('')}\n... truncated; full text is stored on the Business OS command record ...`;
}

/** ref: store.rs:23796 extract_json_object – erstes vollständiges JSON-Objekt einer Skill-Antwort */
export function extractJsonObject(text) {
  const trimmed = String(text ?? '').trim();
  try {
    const value = JSON.parse(trimmed);
    if (isObject(value)) return value;
  } catch { /* weiter mit Klammerzählung */ }
  const start = trimmed.indexOf('{');
  if (start < 0) return undefined;
  let depth = 0;
  let inString = false;
  let escaped = false;
  let end;
  for (let i = start; i < trimmed.length; i += 1) {
    const ch = trimmed[i];
    if (inString) {
      if (escaped) escaped = false;
      else if (ch === '\\') escaped = true;
      else if (ch === '"') inString = false;
      continue;
    }
    if (ch === '"') inString = true;
    else if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) { end = i + 1; break; }
    }
  }
  if (end === undefined) return undefined;
  try {
    const value = JSON.parse(trimmed.slice(start, end));
    return isObject(value) ? value : undefined;
  } catch {
    return undefined;
  }
}

/** ref: store.rs:23842 cv_print_index_text */
export function cvPrintIndexText(model, candidateName) {
  const parts = [candidateName];
  const candidate = isObject(model.candidate) ? model.candidate : {};
  for (const key of ['currentRole', 'location', 'email']) {
    if (typeof candidate[key] === 'string' && candidate[key].trim() !== '') parts.push(candidate[key].trim());
  }
  return parts.join(' · ');
}

function sourceFileId(payload) {
  const sourceFile = isObject(payload.source_file) ? payload.source_file : {};
  return firstStringField(payload, ['source_file_id', 'file_id']) ?? firstStringField(sourceFile, ['file_id', 'fileId']);
}

/** Neue Profile: Datei-Ref aus api.files (`source_file_ref` bzw. `source_file:{kind:'file', ref}`). */
export function sourceFileRef(payload) {
  const sourceFile = isObject(payload.source_file) ? payload.source_file : {};
  return firstStringField(payload, ['source_file_ref'])
    ?? (sourceFile.kind === 'file' ? firstStringField(sourceFile, ['ref']) : undefined);
}

/** Plattform-Collection `files` (id = Datei-ID, ref = sha256 des Inhalts, size in Bytes): erst per ID, sonst per Ref. */
async function filesDocFor(ctx, ref, fileId) {
  if (fileId !== undefined) {
    const byId = await ctx.get('files', fileId);
    if (byId && (ref === undefined || byId.ref === undefined || byId.ref === ref)) return byId;
  }
  if (ref === undefined) return null;
  const [byRef] = await ctx.read('files', { selector: { ref }, limit: 1 });
  return byRef ?? null;
}

/**
 * ref: store.rs:25926-25984 cv_print_pdf_text_prompt_enrichment – Quelle immer ein files-Datensatz (§16.32).
 * Neue Profile: `source_file_ref` (+ optional source_file.file_id). Ältere Profile (`source_file_id` aus
 * desktop_files): der einmalige Kern-Import desktop_files→files (V4-HANDLERS.md „Kern-Import“) legt je
 * Desktop-Datei einen files-Datensatz mit derselben id an; der Altpfad über desktop_files ist entfernt.
 * Text über cap.doc.extract({blob_id:{ref}, kind:'pdf'}), Größenprüfung über `files.size`.
 * Liefert die Werte des Abschnitts „CV PDF extracted text“ im Template prompts/parse.md (`pdf.*`).
 */
async function pdfTextValues(ctx, payload) {
  const ref = sourceFileRef(payload);
  const sourceFile = isObject(payload.source_file) ? payload.source_file : {};
  const legacyId = ref === undefined ? sourceFileId(payload) : undefined;
  if (ref === undefined && legacyId === undefined) fail('invalid', 'cv-print parse command is missing source_file_id');
  const fileId = firstStringField(sourceFile, ['file_id', 'fileId']) ?? legacyId;
  const fileDoc = await filesDocFor(ctx, ref, fileId);
  const blobRef = ref ?? (typeof fileDoc?.ref === 'string' && fileDoc.ref ? fileDoc.ref : undefined);
  if (blobRef === undefined) {
    fail('not_found', `cv-print source file ${fileId} is not available in files (Kern-Import desktop_files→files)`);
  }
  const declaredSize = asU64(fileDoc?.size) ?? 0;
  if (declaredSize > CV_PRINT_PDF_MAX_BYTES) {
    fail('invalid', `cv-print source PDF ${ref ?? fileId} is too large (${declaredSize} bytes, max ${CV_PRINT_PDF_MAX_BYTES})`);
  }
  const extracted = await callCap(ctx, 'doc.extract', { blob_id: { ref: blobRef }, kind: 'pdf' });
  const text = truncateTextPreserve(String(extracted?.text ?? ''), CV_PRINT_PDF_TEXT_MAX_CHARS).replaceAll('```', "'''");
  return {
    source_file_id: fileDoc?.id ?? fileId ?? '',
    ref_line: ref !== undefined ? `\n- source_file_ref: ${ref}` : '',
    filename: fileDoc?.name ?? firstStringField(payload, ['filename']) ?? '',
    mime_type: fileDoc?.mime ?? firstStringField(payload, ['mime_type']) ?? '',
    size_bytes: fileDoc?.size ?? '',
    sha256: blobRef,
    text,
  };
}

/** Name des Prompt-Templates im Paket (prompts/parse.md, gelesen über ctx.asset, §16.34). */
export const PARSE_PROMPT = 'parse';
/** App-Skill im eigenen Namensraum (skills/parser/SKILL.md, §16.30). */
export const PARSER_SKILL = `${MODULE_ID}/parser`;

/**
 * ref: store.rs:27384-27412 cv_print_command_prompt – Text wörtlich in prompts/parse.md (samt dem Abschnitt aus
 * cv_print_pdf_text_prompt_enrichment :25926); hier nur Platzhalter füllen und wie in Rust kürzen.
 */
export async function cvPrintCommandPrompt(ctx, command, payload, pdf) {
  const instructionRaw = [payload.instruction, payload.prompt]
    .find((value) => typeof value === 'string' && value.trim() !== '');
  const instruction = truncateTextPreserve(
    instructionRaw?.trim() ?? 'Parse the extracted CV PDF text into a CTOX CV print profile JSON.', 2_400);
  const prompt = await renderPrompt(ctx, PARSE_PROMPT, {
    instruction,
    pdf,
    command_id: command.id,
    module: MODULE_ID,
    type: command.type,
    document_id: firstStringField(payload, ['document_id']) ?? command.record_id ?? '',
    version_id: firstStringField(payload, ['version_id']) ?? '',
    source_file_id: sourceFileId(payload) ?? '',
  });
  // Rust-Text endet auf „.“; ein vom Editor angehängter Zeilenumbruch im Template fällt weg.
  return truncateTextPreserve(prompt.replace(/\n$/, ''), 32_000);
}

/** Rückruf-Befehlstypen dieser App (§16.30 Nr. 3): der Kern schreibt nach Abschluss genau einen Befehl. */
export const CV_PRINT_PARSE_DONE = 'ctox.cv_print.apply_parse.done';
export const CV_PRINT_PARSE_FAILED = 'ctox.cv_print.apply_parse.failed';

export default async function handle(command, ctx) {
  const payload = payloadOf(command);
  const pdf = await pdfTextValues(ctx, payload);
  // §16.30: agent.task.enqueue ist ein echtes Primitiv in flacher Form. Ausgangsbefehl, Ausgabevertrag und
  // Herkunft reisen im payload; das Ergebnis kommt als Befehl CV_PRINT_PARSE_DONE {task_id, status, result}.
  const task = await callCap(ctx, 'agent.task.enqueue', {
    title: firstStringField(payload, ['title']) ?? 'CV strukturieren',
    instruction: await cvPrintCommandPrompt(ctx, command, payload, pdf),
    skill: 'cv-print-builder/parser',
    thread_key: firstStringField(payload, ['chat_id']) ?? null,
    payload: {
      command: { id: command.id, type: command.type, record_id: command.record_id ?? null, payload },
      output_contract: 'ctox.cv_print_profile.v1',
      source: MODULE_ID,
    },
    idempotency_key: command.id,
    on_complete: CV_PRINT_PARSE_DONE,
    on_failed: CV_PRINT_PARSE_FAILED,
  });
  return { task_id: task?.task_id ?? null, status: 'queued' };
}

/** ref: store.rs:23866-24090 writeback_cv_print_parse; Ergebnis wie process_cv_print_parse_command :24112 `result`. */
export async function onReply(command, reply, ctx) {
  const payload = payloadOf(command);
  const replyText = String(reply?.text ?? '').trim();
  if (replyText === '') fail('invalid', 'cv-print parse reply was empty');
  const model = extractJsonObject(replyText);
  if (!model) fail('invalid', 'cv-print parse reply did not contain a JSON profile object');
  const writeback = isObject(payload.writeback_contract) ? payload.writeback_contract : undefined;
  const documentId = firstStringField(payload, ['document_id'])
    ?? (typeof writeback?.document_id === 'string' ? writeback.document_id : undefined);
  if (documentId === undefined) fail('invalid', 'cv-print parse command is missing document_id');
  const expectedSchema = typeof writeback?.expected_model_schema === 'string' ? writeback.expected_model_schema : 'ctox.cv_print_profile.v1';
  const sourceId = firstStringField(payload, ['source_file_id']) ?? '';
  const sourceRef = sourceFileRef(payload) ?? '';
  const sourceRefFileId = isObject(payload.source_file) ? firstStringField(payload.source_file, ['file_id', 'fileId']) ?? '' : '';
  const filename = firstStringField(payload, ['filename']) ?? '';
  const generationId = firstStringField(payload, ['generation_id']) ?? '';
  const mimeType = firstStringField(payload, ['mime_type']) ?? 'application/pdf';
  const sha256 = firstStringField(payload, ['sha256']) ?? '';
  const sizeBytes = asU64(payload.size_bytes) ?? 0;

  model.schema = expectedSchema;
  if (!('workflow' in model)) model.workflow = {};
  if (isObject(model.workflow)) Object.assign(model.workflow, { phase: 'review', view_mode: 'split', command_id: command.id });
  if (model.source === undefined) model.source = {};
  if (isObject(model.source)) {
    if (sourceId !== '') model.source.desktop_file_id = sourceId;
    // Neue Profile: Datei-Ref (api.files) in der neuen Version halten, sonst verliert die UI das Original-PDF.
    if (sourceRef !== '') model.source.file_ref = sourceRef;
    if (sourceRef !== '' && sourceRefFileId !== '') model.source.file_id = sourceRefFileId;
    if (generationId !== '') model.source.generation_id = generationId;
    if (filename !== '') model.source.filename = filename;
    model.source.mime_type = mimeType;
    if (sizeBytes > 0) model.source.size_bytes = sizeBytes;
    if (sha256 !== '') model.source.sha256 = sha256;
  }
  if (model.print === undefined) model.print = {};
  if (isObject(model.print)) {
    const defaults = { template: 'minimal', anonymize: false, showLogo: true, logoDataUrl: '', preset: 'standard', overrides: {} };
    for (const [key, value] of Object.entries(defaults)) if (!(key in model.print)) model.print[key] = value;
  }
  const candidateNameRaw = isObject(model.candidate) && typeof model.candidate.name === 'string' ? model.candidate.name.trim() : '';
  const candidateName = candidateNameRaw !== '' ? candidateNameRaw : 'Unbenanntes Profil';
  const indexText = cvPrintIndexText(model, candidateName);
  const now = ctx.now();

  return ctx.transaction(async (tx) => {
    // Rust: MAX(version) + 1 über nicht gelöschte document_versions dieses Dokuments.
    const versions = await tx.read('document_versions', { selector: { document_id: documentId } });
    // Doppelzustellung des Rückrufs (§16.30 Nr. 3): eine Version dieses Ausgangsbefehls gibt es schon → keine Wirkung.
    const applied = versions.find((doc) => doc.business_command_id === command.id && doc.source_kind === 'cv_pdf_parse' && doc._deleted !== true);
    if (applied) {
      const name = isObject(applied.model_json?.candidate) && typeof applied.model_json.candidate.name === 'string' && applied.model_json.candidate.name.trim() !== ''
        ? applied.model_json.candidate.name.trim() : 'Unbenanntes Profil';
      return { document_id: documentId, version_id: applied.id, version: applied.version, candidate_name: name, duplicate: true };
    }
    const maxVersion = versions.reduce((max, doc) => {
      const v = typeof doc.version === 'number' ? Math.trunc(doc.version) : Number.parseInt(doc.version, 10);
      return Number.isFinite(v) && v > max ? v : max;
    }, 0);
    const nextVersion = maxVersion + 1;
    const versionId = `${documentId}_v${nextVersion}`;
    const diagnostics = isObject(model.workflow) && model.workflow.diagnostics !== undefined ? model.workflow.diagnostics : [];
    await tx.put('document_versions', {
      id: versionId,
      version_id: versionId,
      document_id: documentId,
      version: nextVersion,
      source_kind: 'cv_pdf_parse',
      blob_id: sourceRef !== '' ? sourceRef : (sourceId === '' ? `${versionId}_blob` : sourceId),
      diagnostics,
      model_json: model,
      model: null,
      business_command_id: command.id,
      created_at_ms: now,
      updated_at_ms: now,
      _deleted: false,
    });
    const existing = await tx.get('documents', documentId);
    const doc = existing ? JSON.parse(JSON.stringify(existing)) : { id: documentId, document_id: documentId };
    doc.current_version_id = versionId;
    doc.status = 'review';
    doc.index_text = indexText;
    doc.title = candidateName;
    if (doc.display_cache === undefined) doc.display_cache = {};
    if (isObject(doc.display_cache)) Object.assign(doc.display_cache, { phase: 'review', candidate_name: candidateName });
    doc.updated_at_ms = now;
    doc._deleted = false;
    await tx.put('documents', doc);
    return { document_id: documentId, version_id: versionId, version: nextVersion, candidate_name: candidateName };
  });
}

/**
 * ref: process_cv_print_parse_command :24151 – Fehlschlag/Abbruch der Agentenaufgabe (Rückruf
 * ctox.cv_print.apply_parse.failed, status 'failed' | 'cancelled'). Rust vermerkte den Fehler nur am Befehl; im
 * Zielbild ist der Ausgangsbefehl längst erledigt (er hat nur eingereiht), deshalb hält die App den Ausgang am
 * Profil fest – genau wie die UI eine lokal abgelehnte Einreihung festhält (index.js onRejected): Version des
 * Laufs `workflow.phase = 'error'`, `task_status`, `error`; Dokument `status`/`display_cache.phase = 'error'`.
 * Nur wenn die Version noch auf diesen Lauf wartet (phase 'parsing', command_id = Ausgangsbefehl) – sonst und
 * bei Doppelzustellung keine Wirkung.
 */
export async function onFailed(command, failure, ctx) {
  const payload = payloadOf(command);
  const message = String(failure?.message ?? 'cv-print parse failed');
  const status = failure?.status === 'cancelled' ? 'cancelled' : 'failed';
  const versionId = firstStringField(payload, ['version_id']);
  const documentId = firstStringField(payload, ['document_id']) ?? command.record_id;
  if (!ctx || versionId === undefined) return { ok: false, status, error: message, applied: false };
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const version = await tx.get('document_versions', versionId);
    const workflow = isObject(version?.model_json?.workflow) ? version.model_json.workflow : null;
    if (!workflow || workflow.command_id !== command.id || workflow.phase !== 'parsing') {
      return { ok: false, status, error: message, applied: false };
    }
    const model = JSON.parse(JSON.stringify(version.model_json));
    Object.assign(model.workflow, { phase: 'error', view_mode: 'original', task_status: status, error: message, updated_at_ms: now });
    await tx.put('document_versions', { ...JSON.parse(JSON.stringify(version)), model_json: model, updated_at_ms: now });
    const existing = documentId === undefined ? null : await tx.get('documents', documentId);
    if (existing) {
      const doc = JSON.parse(JSON.stringify(existing));
      doc.status = 'error';
      if (doc.display_cache === undefined) doc.display_cache = {};
      if (isObject(doc.display_cache)) doc.display_cache.phase = 'error';
      doc.updated_at_ms = now;
      await tx.put('documents', doc);
    }
    return { ok: false, status, error: message, applied: true, version_id: versionId };
  });
}
