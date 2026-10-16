// Rückrufe der Agenten-Aufträge (Kern §16.30 Nr. 3): payload {task_id, status, result, error?, workspace_refs?}.
// Der Core stellt höchstens einmal je task_id zu; die Handler sind trotzdem idempotent (Doppelzustellung ohne
// Wirkung) und schreiben nur in die eigenen Collections research_tasks, research_runs, research_notes. Das Knowledge
// Book geht als files-Datensatz per ctx.command (knowledge.book.ingest) an die App knowledge (Strang KNOW2).
import { fail, invalid, isPlainObject } from './errors.js';

const text = (v) => (typeof v === 'string' ? v.trim() : '');
const TERMINAL = new Set(['completed', 'failed', 'cancelled']);

export function callbackOf(command) {
  const p = isPlainObject(command.payload) ? command.payload : {};
  const taskId = text(p.task_id);
  if (!taskId) throw invalid('task_id is required');
  const status = text(p.status) || 'failed';
  const error = isPlainObject(p.error) ? p.error : (typeof p.error === 'string' ? { code: 'failed', message: p.error } : null);
  return { taskId, status, result: p.result ?? null, error, workspaceRefs: Array.isArray(p.workspace_refs) ? p.workspace_refs : [] };
}

/** Auftrag samt Payload der Aufnahme (business_command_id, research_* …) über agent.task.get. */
export async function loadTask(ctx, taskId) {
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  if (!task) throw fail('not_found', `agent task ${taskId} not found`);
  return { task, payload: isPlainObject(task.payload) ? task.payload : {} };
}

export function failureOf(cb) {
  const cancelled = cb.status === 'cancelled';
  return {
    task_id: cb.taskId,
    status: cancelled ? 'cancelled' : 'failed',
    code: text(cb.error?.code) || (cancelled ? 'cancelled' : 'failed'),
    message: text(cb.error?.message) || (cancelled ? 'cancelled' : 'failed'),
  };
}

async function findRun(tx, payload) {
  const runId = text(payload.research_run_id);
  if (runId) {
    const run = await tx.get('research_runs', runId);
    if (run) return run;
  }
  const commandId = text(payload.business_command_id);
  if (!commandId) return null;
  const docs = await tx.read('research_runs', { selector: { command_id: commandId }, limit: 1 });
  return docs[0] ?? null;
}

/**
 * research.systematic.run.failed: Lauf und Aufgabe als fehlgeschlagen bzw. abgebrochen markieren.
 * // ref: store.rs:22820-22943 (persist_systematic_research_failure: task failed + last_error, run failed + error)
 * Abbruch (status 'cancelled') setzt die Aufgabe zurück auf 'ready' statt 'failed'.
 */
export async function persistRunFailure(ctx, cb) {
  const { payload } = await loadTask(ctx, cb.taskId);
  const failure = { ...failureOf(cb), failed_at_ms: ctx.now() };
  let changed = false;
  await ctx.transaction(async (tx) => {
    const run = await findRun(tx, payload);
    if (run) {
      if (TERMINAL.has(run.status) && run.payload?.error?.task_id === cb.taskId) return;
      run.status = failure.status;
      run.task_queue_id = cb.taskId;
      run.updated_at_ms = failure.failed_at_ms;
      run.payload = { ...(isPlainObject(run.payload) ? run.payload : {}), error: failure };
      delete run._rev;
      await tx.put('research_runs', run);
      changed = true;
    }
    const researchTaskId = text(payload.record_id) || text(payload.research_task_id);
    const task = researchTaskId ? await tx.get('research_tasks', researchTaskId) : null;
    if (task && task.payload?.last_error?.task_id !== cb.taskId) {
      task.status = failure.status === 'cancelled' ? 'ready' : 'failed';
      task.updated_at_ms = failure.failed_at_ms;
      task.payload = { ...(isPlainObject(task.payload) ? task.payload : {}), last_error: failure };
      delete task._rev;
      await tx.put('research_tasks', task);
      changed = true;
    }
  });
  return { ok: true, task_id: cb.taskId, status: failure.status, duplicate: !changed };
}

// ------------------------------------------------------------------------------------- Knowledge Book (refresh)

export const REFRESH_RESULT_PATH = 'knowledge-refresh/result.json';
export const REFRESH_SCHEMA = 'ctox.research.knowledge-refresh.v1';
const ELEMENT_KINDS = new Set(['skillbook', 'skill', 'runbook', 'resource']);
const CITED_KINDS = new Set(['skillbook', 'skill', 'resource']);

async function readWorkspaceText(ctx, taskId, path) {
  let file;
  try {
    file = await ctx.cap.agent.task.workspace.read({ task_id: taskId, path });
  } catch (error) {
    if (error?.code === 'not_found') return null;
    throw error;
  }
  if (file == null) return null;
  return typeof file === 'string' ? file : (typeof file.text === 'string' ? file.text : null);
}

/**
 * Ergebnisdatei des Skills research/knowledge-refresh prüfen (fail-closed). Liefert {ok, result|reason}.
 * Neu (kein Rust-Vorbild: die Core-Heuristik kannte nur den Phantom-Skill `knowledge`, store.rs:27751, und
 * complete_business_command_from_queue_reply :10312 hatte keinen Zweig für research.knowledge.refresh).
 */
export function validateRefreshResult(raw, payload) {
  let data;
  try { data = JSON.parse(raw); } catch { return { ok: false, reason: `${REFRESH_RESULT_PATH} is not valid JSON` }; }
  if (!isPlainObject(data)) return { ok: false, reason: `${REFRESH_RESULT_PATH} must be an object` };
  if (data.schema_version !== REFRESH_SCHEMA) return { ok: false, reason: `schema_version must be ${REFRESH_SCHEMA}` };
  for (const key of ['research_task_id', 'research_run_id', 'research_command_id', 'knowledge_domain']) {
    const expected = typeof payload[key] === 'string' ? payload[key] : '';
    if ((typeof data[key] === 'string' ? data[key] : '') !== expected) return { ok: false, reason: `${key} does not match the task` };
  }
  const known = new Set((Array.isArray(payload.source_lineage) ? payload.source_lineage : []).map((r) => text(r?.source_id)).filter(Boolean));
  const elements = Array.isArray(data.elements) ? data.elements : [];
  if (!elements.length) return { ok: false, reason: 'elements must not be empty' };
  const seen = new Set();
  for (const [i, el] of elements.entries()) {
    if (!isPlainObject(el) || !text(el.id) || !text(el.title) || !ELEMENT_KINDS.has(el.kind)) return { ok: false, reason: `element ${i} needs id, title and a known kind` };
    if (seen.has(el.id)) return { ok: false, reason: `element id ${el.id} is duplicated` };
    seen.add(el.id);
    const citations = Array.isArray(el.citations) ? el.citations : [];
    if (CITED_KINDS.has(el.kind) && !citations.length) return { ok: false, reason: `element ${el.id} has no citation` };
    for (const c of citations) {
      if (!isPlainObject(c) || !known.has(text(c.source_id))) return { ok: false, reason: `element ${el.id} cites an unknown source ${text(c?.source_id) || '(none)'}` };
    }
  }
  return { ok: true, result: { ...data, elements } };
}

// Übergabe an die App knowledge (Strang KNOW2): Wissensdaten gehören knowledge. research legt das Book als
// files-Datensatz an (BlobRef, Kern §16.32) und sendet den deklarierten Befehl knowledge.book.ingest; research_notes
// behält nur den Verweis samt Status (ein Dokument je Research-Aufgabe).
export const KNOWLEDGE_APP = 'knowledge';
export const KNOWLEDGE_INGEST = 'knowledge.book.ingest';
export const KNOWLEDGE_BOOK_SCHEMA = 'ctox.knowledge.book.v1';

export const bookNoteId = (researchTaskId) => `kb:${researchTaskId}`.slice(0, 180);
export const ingestCommandId = (taskId) => `${taskId}:knowledge-book`;

/** research-Ergebnis → Format der App knowledge (ctox.knowledge.book.v1). */
export function knowledgeBook(result, payload, researchTaskId, taskId) {
  return {
    schema_version: KNOWLEDGE_BOOK_SCHEMA,
    knowledge_domain: text(payload.knowledge_domain),
    knowledge_version_id: text(payload.knowledge_version_id) || text(result.knowledge_version_id),
    title: text(payload.research_brief?.task_title) || text(payload.knowledge_domain),
    summary: typeof result.summary === 'string' ? result.summary : '',
    elements: result.elements.map((el) => ({
      id: el.id,
      kind: el.kind,
      title: el.title,
      markdown: typeof el.markdown === 'string' ? el.markdown : '',
      summary: typeof el.summary === 'string' ? el.summary : '',
      citations: Array.isArray(el.citations) ? el.citations : [],
    })),
    gaps: Array.isArray(result.gaps) ? result.gaps : [],
    provenance: {
      app: 'research',
      research_task_id: researchTaskId,
      research_run_id: text(payload.research_run_id),
      research_command_id: text(payload.research_command_id),
      agent_task_id: taskId,
    },
  };
}

async function putBookFile(ctx, book, researchTaskId) {
  if (typeof ctx.files?.put !== 'function') throw fail('unavailable', 'ctx.files.put is not available');
  const bytes = new TextEncoder().encode(JSON.stringify(book));
  const handle = await ctx.files.put(bytes, { name: `knowledge-book-${researchTaskId}.json`, mime: 'application/json' });
  const ref = await (typeof handle === 'string' ? handle : handle?.ref);
  if (typeof ref !== 'string' || !ref) throw fail('unavailable', 'files.put returned no ref');
  return ref;
}

function refreshState(task) {
  return isPlainObject(task?.payload?.knowledge_refresh) ? task.payload.knowledge_refresh : {};
}

async function writeRefreshState(tx, task, patch, now) {
  task.payload = { ...(isPlainObject(task.payload) ? task.payload : {}), knowledge_refresh: { ...refreshState(task), ...patch } };
  task.updated_at_ms = now;
  delete task._rev;
  await tx.put('research_tasks', task);
}

/**
 * research.knowledge.refresh.complete: Knowledge Book prüfen, als files-Datensatz an die App knowledge übergeben
 * (ctx.command knowledge.book.ingest), Verweis in research_notes + Stand in research_tasks.
 */
export async function completeKnowledgeRefresh(ctx, cb) {
  const { payload } = await loadTask(ctx, cb.taskId);
  const researchTaskId = text(payload.research_task_id) || text(payload.record_id);
  if (!researchTaskId) throw invalid('research_task_id is missing on the agent task');
  const current = await ctx.get('research_tasks', researchTaskId);
  if (!current) throw fail('not_found', `research task ${researchTaskId} not found`);
  const prev = refreshState(current);
  if (prev.task_id === cb.taskId && TERMINAL.has(prev.status)) return { ok: true, task_id: cb.taskId, status: prev.status, duplicate: true };

  const raw = await readWorkspaceText(ctx, cb.taskId, REFRESH_RESULT_PATH);
  let checked = raw == null ? { ok: false, reason: `${REFRESH_RESULT_PATH} missing in the task workspace` } : validateRefreshResult(raw, payload);
  let handover = null;
  if (checked.ok) {
    // Capabilities/Befehle außerhalb der Transaktion. Feste Befehls-ID: eine erneute Zustellung nach einem Abbruch
    // vor dem Schreiben unten trifft denselben knowledge-Befehl (knowledge.book.ingest ist je ID idempotent).
    const book = knowledgeBook(checked.result, payload, researchTaskId, cb.taskId);
    const ref = await putBookFile(ctx, book, researchTaskId);
    if (typeof ctx.command !== 'function') throw fail('unavailable', 'ctx.command is not available');
    const sent = (await ctx.command(KNOWLEDGE_INGEST, {
      book: { ref },
      title: `Knowledge Book · ${book.title}`,
      knowledge_domain: book.knowledge_domain,
      knowledge_version_id: book.knowledge_version_id,
      record_id: researchTaskId,
      source: book.provenance,
    }, { id: ingestCommandId(cb.taskId), module: KNOWLEDGE_APP, record_id: researchTaskId })) || {};
    if (sent.status === 'rejected' || sent.status === 'failed') {
      checked = { ok: false, code: 'knowledge_rejected', reason: `${KNOWLEDGE_INGEST} ${sent.status}: ${text(sent.reason) || 'no reason'}` };
    } else {
      handover = { ref, command_id: typeof sent.id === 'string' && sent.id ? sent.id : ingestCommandId(cb.taskId), book };
    }
  }
  const now = ctx.now();
  await ctx.transaction(async (tx) => {
    const task = await tx.get('research_tasks', researchTaskId);
    if (!checked.ok) {
      await writeRefreshState(tx, task, { task_id: cb.taskId, status: 'failed', error: { task_id: cb.taskId, status: 'failed', code: checked.code || 'invalid_result', message: checked.reason }, completed_at_ms: now }, now);
      return;
    }
    const elementIds = handover.book.elements.map((el) => el.id);
    const noteId = bookNoteId(researchTaskId);
    const existing = await tx.get('research_notes', noteId);
    await tx.put('research_notes', {
      id: noteId,
      task_id: researchTaskId,
      kind: 'knowledge_book',
      title: `Knowledge Book · ${handover.book.title}`,
      status: 'handed_over',
      payload: {
        owner_app: KNOWLEDGE_APP,
        book_ref: handover.ref,
        knowledge_command_id: handover.command_id,
        knowledge_element_ids: elementIds,
        knowledge_domain: handover.book.knowledge_domain,
        knowledge_version_id: handover.book.knowledge_version_id,
        research_run_id: payload.research_run_id ?? '',
        research_command_id: payload.research_command_id ?? '',
        agent_task_id: cb.taskId,
      },
      created_at_ms: Number(existing?.created_at_ms) || now,
      updated_at_ms: now,
    });
    await writeRefreshState(tx, task, {
      task_id: cb.taskId,
      status: 'completed',
      summary: handover.book.summary,
      note_id: noteId,
      book_ref: handover.ref,
      knowledge_command_id: handover.command_id,
      element_ids: elementIds,
      gaps: handover.book.gaps,
      knowledge_version_id: payload.knowledge_version_id ?? '',
      completed_at_ms: now,
      error: null,
    }, now);
  });
  return checked.ok
    ? { ok: true, task_id: cb.taskId, status: 'completed', elements: handover.book.elements.length, knowledge_command_id: handover.command_id }
    : { ok: false, task_id: cb.taskId, status: 'failed', reason: checked.reason };
}

/** research.knowledge.refresh.failed: Fehlschlag/Abbruch am Knowledge-Auftrag der Aufgabe vermerken. */
export async function failKnowledgeRefresh(ctx, cb) {
  const { payload } = await loadTask(ctx, cb.taskId);
  const researchTaskId = text(payload.research_task_id) || text(payload.record_id);
  if (!researchTaskId) throw invalid('research_task_id is missing on the agent task');
  const failure = failureOf(cb);
  let changed = false;
  await ctx.transaction(async (tx) => {
    const task = await tx.get('research_tasks', researchTaskId);
    if (!task) throw fail('not_found', `research task ${researchTaskId} not found`);
    const prev = refreshState(task);
    if (prev.task_id === cb.taskId && TERMINAL.has(prev.status)) return;
    const now = ctx.now();
    await writeRefreshState(tx, task, { task_id: cb.taskId, status: failure.status, error: failure, completed_at_ms: now }, now);
    changed = true;
  });
  return { ok: true, task_id: cb.taskId, status: failure.status, duplicate: !changed };
}
