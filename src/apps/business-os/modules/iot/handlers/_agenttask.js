// Origin: CTOX
// License: Apache-2.0
// Rückrufe der IoT-Alarm-Aufträge (Kern §16.30 Nr. 3, Strang SKL-A4): payload {task_id, status, result, error?,
// workspace_refs?}. Aufträge entstehen in _iotcore.evaluateRuleset (Regeltreffer) und ctox.iot.signal.evaluate
// (Widget-Wächter) mit skill 'iot/operations' und payload.alarm_id.
//
// Vorbild: vorher gab es keinen Rückweg (Bestandsaufnahme F R9, fire-and-forget). Der Core schrieb einen Queue-Task je
// Treffer (conditions.rs:1025-1066, widget_runtime.rs:289) und der Agent quittierte/löste den Alarm laut Skill selbst
// per CLI (`ctox iot alarm ack|resolve`, src/skills/system/host_ops/iot-operations/SKILL.md „Command Map“). Diese
// Alarm-Fortschreibung macht jetzt die App: der Skill liefert ein JSON-Urteil, der Rückruf legt es am Alarm ab
// (`data.agent_task`) und wendet die Alarmentscheidung mit den Übergangsregeln von alarms.rs:60-75 an
// (ref: alarms.rs:414 acknowledge, :424 resolve). Gerätschreibvorgänge führt der Rückruf nie aus; sie stehen nur als
// Vorschlag am Alarm.
//
// Idempotent: ein zweiter Rückruf für dieselbe task_id (gleich ob complete oder failed) ändert nichts.
import { fail, isObject } from './_base.js';
import { C, alarmRow, canTransition, compact } from './_iotcore.js';

export const TERMINAL = Object.freeze(['completed', 'failed', 'cancelled']);
export const DECISIONS = Object.freeze(['acknowledge', 'resolve', 'keep_open']);
const MAX_TEXT = 8000;
const MAX_WRITES = 20;
const text = (v) => (typeof v === 'string' ? v.trim().slice(0, MAX_TEXT) : '');

/** Rückruf-Payload lesen (§16.30 Nr. 3). */
export function callbackOf(command) {
  const p = isObject(command?.payload) ? command.payload : {};
  const taskId = typeof p.task_id === 'string' ? p.task_id.trim() : '';
  if (!taskId) fail('invalid', 'task_id is required');
  const status = typeof p.status === 'string' && p.status.trim() ? p.status.trim() : 'failed';
  const error = isObject(p.error) ? p.error : typeof p.error === 'string' ? { code: 'failed', message: p.error } : null;
  return { taskId, status, result: p.result ?? null, error };
}

const stripFence = (s) => s.trim().replace(/^```[a-zA-Z]*\s*\n?/, '').replace(/\n?```\s*$/, '').trim();

/** Antwort des Skills iot/operations normalisieren; Freitext statt JSON → Diagnose = Text, Entscheidung acknowledge. */
export function parseReply(result) {
  let reply = null;
  let raw = '';
  if (typeof result === 'string') raw = result;
  else if (isObject(result) && typeof result.text === 'string') raw = result.text;
  else if (isObject(result)) reply = result;
  if (reply === null && raw.trim() !== '') {
    try {
      const parsed = JSON.parse(stripFence(raw));
      if (isObject(parsed)) reply = parsed;
    } catch { /* Freitext */ }
  }
  if (reply === null) reply = { diagnose: raw };
  const decision = DECISIONS.includes(reply.alarm) ? reply.alarm : 'acknowledge';
  const writes = (Array.isArray(reply.schreibvorschlaege) ? reply.schreibvorschlaege : [])
    .filter((w) => isObject(w) && text(w.asset_id) && text(w.attribute))
    .slice(0, MAX_WRITES)
    .map((w) => ({ asset_id: text(w.asset_id), attribute: text(w.attribute), value: w.value ?? null, grund: text(w.grund) }));
  return { decision, diagnose: text(reply.diagnose), massnahme: text(reply.massnahme), notizen: text(reply.notizen), schreibvorschlaege: writes };
}

/** Zielstatus des Alarms für die Entscheidung; null = Status bleibt. ref: alarms.rs:60-75 can_transition_to */
export function nextStatus(current, decision) {
  if (decision === 'acknowledge') return current === 'Open' && canTransition(current, 'Acknowledged') ? 'Acknowledged' : null;
  if (decision === 'resolve') return current !== 'Resolved' && canTransition(current, 'Resolved') ? 'Resolved' : null;
  return null;
}

async function alarmIdOf(ctx, taskId) {
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  const alarmId = isObject(task?.payload) && typeof task.payload.alarm_id === 'string' ? task.payload.alarm_id : '';
  return alarmId;
}

/**
 * Ergebnis am Alarm ablegen. `apply(alarm, now)` liefert die neue agent_task-Spur und optional den Zielstatus.
 * Fehlt der Alarm (gelöscht), bleibt der Rückruf ohne Wirkung.
 */
async function recordOnAlarm(ctx, cb, apply) {
  const alarmId = await alarmIdOf(ctx, cb.taskId);
  if (!alarmId) return { ok: true, task_id: cb.taskId, skipped: 'no_alarm' };
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const row = await tx.get(C.alarms, alarmId);
    if (!row || !isObject(row.data)) return { ok: true, task_id: cb.taskId, alarm_id: alarmId, skipped: 'alarm_missing' };
    const prior = isObject(row.data.agent_task) ? row.data.agent_task : null;
    if (prior && prior.task_id === cb.taskId && TERMINAL.includes(prior.status)) {
      return { ok: true, task_id: cb.taskId, alarm_id: alarmId, status: prior.status, duplicate: true };
    }
    const alarm = structuredClone(row.data);
    const { trace, status } = apply(alarm, now);
    alarm.agent_task = trace;
    if (status) alarm.status = status;
    alarm.last_modified = now;
    await tx.put(C.alarms, compact({ ...alarmRow(alarm), id: alarm.id, created_at_ms: row.created_at_ms ?? alarm.created, updated_at_ms: now }));
    return { ok: true, task_id: cb.taskId, alarm_id: alarmId, status: trace.status, alarm_status: alarm.status, projections: [{ collection: C.alarms, id: alarmId }] };
  });
}

/** ctox.iot.task.complete: Urteil am Alarm ablegen, Alarm quittieren bzw. lösen. */
export function completeAlarmTask(ctx, cb) {
  const reply = parseReply(cb.result);
  return recordOnAlarm(ctx, cb, (alarm, now) => ({
    status: nextStatus(alarm.status, reply.decision),
    trace: {
      task_id: cb.taskId, status: 'completed', alarm_decision: reply.decision, diagnose: reply.diagnose,
      massnahme: reply.massnahme, notizen: reply.notizen, schreibvorschlaege: reply.schreibvorschlaege, finished_ms: now,
    },
  }));
}

/** ctox.iot.task.failed: Fehlschlag/Abbruch am Alarm festhalten; der Alarm bleibt offen für den Menschen. */
export function failAlarmTask(ctx, cb) {
  const status = cb.status === 'cancelled' ? 'cancelled' : 'failed';
  const error = cb.error ? { code: text(cb.error.code) || status, message: text(cb.error.message) } : { code: status, message: '' };
  return recordOnAlarm(ctx, cb, (_alarm, now) => ({ status: null, trace: { task_id: cb.taskId, status, error, finished_ms: now } }));
}
