// Origin: CTOX
// License: Apache-2.0
// Abschluss des agent.task-Rückschriebs an Fremdsysteme (Kern §16.30): der Core liefert nach Taskende genau einen
// Befehl ctox.ticket.writeback.complete bzw. .failed mit payload {task_id, status, result, error?, workspace?}.
// Die Writeback-Zeile wird über task_id gefunden; sie trägt Operation und Anfrage (payload), weil der
// Abschlussbefehl die Enqueue-Payload nicht mitliefert. Idempotent: nur eine Zeile im Status queued wird
// abgeschlossen, jede weitere Zustellung antwortet {status:'ignored'} ohne Schreibzugriff und ohne Audit.
// Fachliche Referenz: cases.rs comment :1016-1059, transition :1137-1192 (Fall closed), clarification_request
// :700-773, Fehler record_failed_writeback :1906-1955 bzw. send_failed :671-691.
// Antwortauswertung (Strang SKL-A1): der Skill tickets/zammad-writeback endet mit genau einem JSON-Objekt
// {remote_event_ids:[…]} bzw. {error:'…'} (skills/zammad-writeback/SKILL.md, Abschnitt Ergebnis). Das Objekt kann
// direkt als result kommen oder als letzter JSON-Block im Antworttext; {error} ohne IDs zählt als Fehlschlag.
// Abbruch kommt als on_failed mit status 'cancelled' → Zeile 'cancelled' (Rückfrage wie Fehler: send_failed).
import { fail, isObject, payloadOf } from './_base.js';
import {
  COLLECTIONS, clock, closureEvidence, enforceCloseTransition, recordAudit, setCaseState,
} from './_tickets.js';
import { markClarificationSent } from './ctox.ticket.publish_clarification.js';

function taskIdOf(command) {
  const taskId = payloadOf(command).task_id;
  if (typeof taskId !== 'string' || taskId.trim() === '') fail('invalid', 'task_id is required');
  return taskId.trim();
}

async function writebackForTask(db, taskId) {
  const rows = (await db.read(COLLECTIONS.writebacks, { selector: { task_id: taskId } })).filter((row) => !row.is_deleted);
  if (rows.length === 0) fail('not_found', `no ticket writeback for task ${taskId}`);
  return rows[0];
}

const requestOf = (doc) => (isObject(doc.payload) ? doc.payload : {});

// Letztes JSON-Objekt in einem Antworttext: zuerst ```-Blöcke (letzter zuerst), dann ab jeder `{` von hinten bis zur
// letzten `}`. Kein Treffer → null.
function lastJsonObject(text) {
  const fenced = [...text.matchAll(/```(?:json)?\s*([\s\S]*?)```/g)].map((m) => m[1]).reverse();
  const tails = [];
  for (let i = text.lastIndexOf('{'); i >= 0; i = i > 0 ? text.lastIndexOf('{', i - 1) : -1) tails.push(text.slice(i));
  for (const raw of [...fenced, ...tails]) {
    const trimmed = raw.trim();
    const end = trimmed.lastIndexOf('}');
    if (!trimmed.startsWith('{') || end < 0) continue;
    try {
      const value = JSON.parse(trimmed.slice(0, end + 1));
      if (isObject(value)) return value;
    } catch { /* nächster Kandidat */ }
  }
  return null;
}

/** Ergebnis des Skills auswerten: {remoteEventIds, error}. Akzeptiert Objekt, {text|output} oder Text. */
export function parseWritebackResult(result) {
  let value = result;
  if (typeof value === 'string') value = lastJsonObject(value) ?? {};
  else if (isObject(value) && !Array.isArray(value.remote_event_ids) && value.error === undefined) {
    const text = [value.text, value.output, value.final_text].find((x) => typeof x === 'string');
    if (text) value = lastJsonObject(text) ?? {};
  }
  if (!isObject(value)) value = {};
  const remoteEventIds = Array.isArray(value.remote_event_ids)
    ? value.remote_event_ids.map((x) => (typeof x === 'number' ? String(x) : x)).filter((x) => typeof x === 'string' && x.trim() !== '')
    : [];
  const error = typeof value.error === 'string' && value.error.trim() && !Array.isArray(value.remote_event_ids) ? value.error.trim() : null;
  return { remoteEventIds, error };
}

const errorText = (raw) => String(typeof raw === 'string' ? raw : isObject(raw) && typeof raw.message === 'string' ? raw.message : JSON.stringify(raw))
  .replace(/\s+/g, ' ').trim().slice(0, 1000);

export async function completeWriteback(command, ctx) {
  const taskId = taskIdOf(command);
  const { remoteEventIds, error } = parseWritebackResult(payloadOf(command).result);
  if (error) return settleFailure(ctx, taskId, errorText(error), 'failed');
  const time = clock(ctx);
  const done = await ctx.transaction(async (tx) => {
    const doc = await writebackForTask(tx, taskId);
    if (doc.status !== 'queued') return { ignored: true, doc };
    const request = requestOf(doc);
    await tx.patch(COLLECTIONS.writebacks, doc.id, { status: 'ok', payload: { ...request, remote_event_ids: remoteEventIds }, updated_at_ms: time.ms });
    if (doc.operation === 'clarification_request') {
      const clarification = await tx.get(COLLECTIONS.clarifications, request.clarification_id);
      const outboundKey = clarification
        ? await markClarificationSent(tx, time, clarification, { writebackId: doc.id, remoteEventIds, reviewedBy: request.reviewed_by, reviewSummary: request.review_summary })
        : null;
      return { doc, request, clarification, outboundKey };
    }
    const caseDoc = await tx.get(COLLECTIONS.cases, doc.case_id);
    if (doc.operation === 'transition' && caseDoc) {
      enforceCloseTransition(caseDoc, await closureEvidence(tx, caseDoc.case_id));
      await setCaseState(tx, caseDoc, 'closed', time, { closed_at: time.iso });
    } else if (caseDoc) {
      await tx.patch(COLLECTIONS.cases, caseDoc.id, { updated_at: time.iso, updated_at_ms: time.ms });
    }
    return { doc, request, caseDoc };
  });
  if (done.ignored) return { status: 'ignored', writeback_id: done.doc.id, task_id: taskId };
  const { doc, request, caseDoc } = done;
  if (doc.operation === 'clarification_request') {
    await recordAudit(ctx, time, {
      ticket_key: doc.ticket_key, case_id: done.clarification?.case_id ?? null, actor_type: 'clarification_engine',
      action_type: 'clarification_published',
      details: { clarification_id: request.clarification_id, reviewed_by: request.reviewed_by, review_summary: request.review_summary, outbound_message_key: done.outboundKey },
    });
    return { status: 'ok', writeback_id: doc.id, task_id: taskId };
  }
  const details = doc.operation === 'transition'
    ? { operation: 'transition', state: request.state, note_body: request.note_body ?? null, internal_note: Boolean(request.internal_note) }
    : { operation: 'comment', body: request.body, internal: Boolean(request.internal) };
  await recordAudit(ctx, time, { case: caseDoc, ticket_key: doc.ticket_key, case_id: doc.case_id, actor_type: 'writeback_engine', action_type: 'writeback_record', details });
  if (doc.operation === 'transition' && caseDoc) {
    await recordAudit(ctx, time, { case: caseDoc, ticket_key: doc.ticket_key, actor_type: 'control_plane', action_type: 'case_closed', details: { reason: 'writeback transition completed' } });
  }
  return { status: 'ok', writeback_id: doc.id, task_id: taskId };
}

export async function failWriteback(command, ctx) {
  const taskId = taskIdOf(command);
  const p = payloadOf(command);
  const cancelled = p.status === 'cancelled';
  const rawError = p.error ?? (isObject(p.result) ? p.result.error : undefined) ?? (cancelled ? 'writeback task cancelled' : 'writeback task failed');
  return settleFailure(ctx, taskId, errorText(rawError), cancelled ? 'cancelled' : 'failed');
}

async function settleFailure(ctx, taskId, error, status) {
  const time = clock(ctx);
  const done = await ctx.transaction(async (tx) => {
    const doc = await writebackForTask(tx, taskId);
    if (doc.status !== 'queued') return { ignored: true, doc };
    const request = requestOf(doc);
    await tx.patch(COLLECTIONS.writebacks, doc.id, { status, payload: { payload: request, error }, updated_at_ms: time.ms });
    if (doc.operation === 'clarification_request') {
      const clarification = await tx.get(COLLECTIONS.clarifications, request.clarification_id);
      if (clarification) {
        await tx.patch(COLLECTIONS.clarifications, clarification.id, {
          status: 'send_failed', metadata: { previous: clarification.metadata ?? {}, send_error: error }, updated_at: time.iso, updated_at_ms: time.ms,
        });
      }
      return { doc, caseDoc: null };
    }
    return { doc, caseDoc: await tx.get(COLLECTIONS.cases, doc.case_id) };
  });
  if (done.ignored) return { status: 'ignored', writeback_id: done.doc.id, task_id: taskId };
  if (done.doc.operation !== 'clarification_request') {
    await recordAudit(ctx, time, {
      case: done.caseDoc, ticket_key: done.doc.ticket_key, case_id: done.doc.case_id, actor_type: 'writeback_engine',
      action_type: 'writeback_failed', details: { operation: done.doc.operation, error, ...(status === 'cancelled' ? { cancelled: true } : {}) },
    });
  }
  return { status, writeback_id: done.doc.id, task_id: taskId, error };
}
