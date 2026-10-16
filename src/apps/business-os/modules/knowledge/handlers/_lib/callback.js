// Rückrufe der Knowledge-Aufträge (Kern §16.30 Nr. 3): payload {task_id, status, result, error?, workspace_refs?}.
// Der Core stellt höchstens einmal je task_id zu; die Handler sind trotzdem idempotent. Geschrieben wird nur in die
// eigenen Collections knowledge_tasks, knowledge_items, knowledge_runbooks.
//
// Kein Rust-Vorbild für die Auswertung: store.rs:10312-10411 (complete_business_command_from_queue_reply) hatte für
// ctox.knowledge.* keinen Zweig; der Agent schrieb selbst in den Core-Knowledge-Store (R6), das Ergebnis des Befehls
// war nur der Antworttext. Jetzt liefert der Skill knowledge/knowledge-maintenance knowledge/writeback.json, und
// knowledge.task.complete wendet es an (fail-closed: ungültige Datei → nichts angewendet, Auftrag failed).
import { TASKS, invalid } from './agent.js';

export const WRITEBACK_PATH = 'knowledge/writeback.json';
export const WRITEBACK_SCHEMA = 'ctox.knowledge.writeback.v1';
const ITEM_KINDS = new Set(['skillbook', 'skill', 'resource', 'dataframe']);
const TERMINAL = new Set(['completed', 'failed', 'cancelled']);
const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const text = (v) => (typeof v === 'string' ? v.trim() : '');
const optionalText = (v) => (typeof v === 'string' ? v : undefined);

export function callbackOf(command) {
  const p = isObject(command.payload) ? command.payload : {};
  const taskId = text(p.task_id);
  if (!taskId) throw invalid('task_id is required');
  const error = isObject(p.error) ? p.error : (typeof p.error === 'string' ? { code: 'failed', message: p.error } : null);
  return { taskId, status: text(p.status) || 'failed', result: p.result ?? null, error };
}

/** knowledge_tasks-Dokument des Auftrags; fehlt es (z. B. Aufnahme vor SKL-B2), wird es aus agent.task.get gebaut. */
async function taskRecord(ctx, taskId) {
  const existing = await ctx.get(TASKS, taskId);
  if (existing) return existing;
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  if (!task) throw { code: 'not_found', message: `agent task ${taskId} not found` };
  const p = isObject(task.payload) ? task.payload : {};
  const now = ctx.now();
  return {
    id: taskId,
    command_id: text(p.business_command_id) || text(task.idempotency_key) || taskId,
    command_type: text(p.command_type) || 'ctox.knowledge.unknown',
    record_id: text(p.record_id),
    title: text(task.title) || taskId,
    status: 'queued',
    created_at_ms: now,
    updated_at_ms: now,
  };
}

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

/** knowledge/writeback.json prüfen. Liefert {ok, writeback} oder {ok:false, reason}. */
export function validateWriteback(raw) {
  let data;
  try { data = JSON.parse(raw); } catch { return { ok: false, reason: `${WRITEBACK_PATH} is not valid JSON` }; }
  if (!isObject(data)) return { ok: false, reason: `${WRITEBACK_PATH} must be an object` };
  if (data.schema_version !== WRITEBACK_SCHEMA) return { ok: false, reason: `schema_version must be ${WRITEBACK_SCHEMA}` };
  const items = data.items ?? [];
  const runbooks = data.runbooks ?? [];
  if (!Array.isArray(items) || !Array.isArray(runbooks)) return { ok: false, reason: 'items and runbooks must be arrays' };
  for (const [list, kinds, name] of [[items, ITEM_KINDS, 'items'], [runbooks, new Set(['runbook']), 'runbooks']]) {
    const seen = new Set();
    for (const [i, el] of list.entries()) {
      if (!isObject(el) || !text(el.id) || !text(el.title) || !kinds.has(el.kind ?? (name === 'runbooks' ? 'runbook' : ''))) {
        return { ok: false, reason: `${name}[${i}] needs id, title and a known kind` };
      }
      if (el.id.length > 180) return { ok: false, reason: `${name}[${i}].id longer than 180` };
      if (seen.has(el.id)) return { ok: false, reason: `${name} id ${el.id} is duplicated` };
      seen.add(el.id);
    }
  }
  return { ok: true, writeback: { ...data, items, runbooks } };
}

export function elementDoc(el, existing, kind, taskId, now) {
  const doc = { ...(existing || {}) };
  delete doc._rev;
  Object.assign(doc, {
    id: el.id,
    kind,
    title: el.title,
    payload: { ...(isObject(existing?.payload) ? existing.payload : {}), ...(isObject(el.payload) ? el.payload : {}), last_task_id: taskId },
    updated_at: new Date(now).toISOString(),
    updated_at_ms: now,
  });
  for (const key of ['subtitle', 'summary', 'source_path', 'markdown', 'prompt']) {
    const v = optionalText(el[key]);
    if (v !== undefined) doc[key] = v;
  }
  if (Array.isArray(el.items)) doc.items = el.items;
  return doc;
}

/** knowledge.task.complete: Writeback anwenden, Auftrag in knowledge_tasks abschließen. */
export async function completeKnowledgeTask(ctx, cb) {
  const record = await taskRecord(ctx, cb.taskId);
  if (TERMINAL.has(record.status)) return { ok: true, task_id: cb.taskId, status: record.status, duplicate: true };
  const raw = await readWorkspaceText(ctx, cb.taskId, WRITEBACK_PATH);
  const checked = raw == null ? { ok: false, reason: `${WRITEBACK_PATH} missing in the task workspace` } : validateWriteback(raw);
  const now = ctx.now();
  const replyText = typeof cb.result === 'string' ? cb.result : (typeof cb.result?.text === 'string' ? cb.result.text : '');
  await ctx.transaction(async (tx) => {
    const current = (await tx.get(TASKS, cb.taskId)) ?? record;
    if (TERMINAL.has(current.status)) return;
    const next = { ...current, updated_at_ms: now };
    delete next._rev;
    if (!checked.ok) {
      Object.assign(next, { status: 'failed', error: { task_id: cb.taskId, status: 'failed', code: 'invalid_result', message: checked.reason } });
    } else {
      const { writeback } = checked;
      for (const el of writeback.items) {
        await tx.put('knowledge_items', elementDoc(el, await tx.get('knowledge_items', el.id), el.kind, cb.taskId, now));
      }
      for (const el of writeback.runbooks) {
        await tx.put('knowledge_runbooks', elementDoc(el, await tx.get('knowledge_runbooks', el.id), 'runbook', cb.taskId, now));
      }
      Object.assign(next, {
        status: 'completed',
        summary: text(writeback.summary) || replyText.slice(0, 1200),
        result: {
          item_ids: writeback.items.map((el) => el.id),
          runbook_ids: writeback.runbooks.map((el) => el.id),
          artifacts: Array.isArray(writeback.artifacts) ? writeback.artifacts.filter((a) => typeof a === 'string') : [],
          open_inputs: Array.isArray(writeback.open_inputs) ? writeback.open_inputs : [],
        },
      });
      delete next.error;
    }
    await tx.put(TASKS, next);
  });
  return checked.ok
    ? { ok: true, task_id: cb.taskId, status: 'completed', items: checked.writeback.items.length, runbooks: checked.writeback.runbooks.length }
    : { ok: false, task_id: cb.taskId, status: 'failed', reason: checked.reason };
}

/** knowledge.task.failed: Fehlschlag oder Abbruch (status 'cancelled') in knowledge_tasks vermerken. */
export async function failKnowledgeTask(ctx, cb) {
  const record = await taskRecord(ctx, cb.taskId);
  if (TERMINAL.has(record.status)) return { ok: true, task_id: cb.taskId, status: record.status, duplicate: true };
  const cancelled = cb.status === 'cancelled';
  const status = cancelled ? 'cancelled' : 'failed';
  const next = {
    ...record,
    status,
    error: {
      task_id: cb.taskId,
      status,
      code: text(cb.error?.code) || status,
      message: text(cb.error?.message) || status,
    },
    updated_at_ms: ctx.now(),
  };
  delete next._rev;
  await ctx.put(TASKS, next);
  return { ok: true, task_id: cb.taskId, status };
}
