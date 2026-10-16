// Origin: CTOX
// License: Apache-2.0
// ctox.cv_print.apply_parse.failed – Rückruf des Kerns, wenn die Agentenaufgabe fehlschlägt oder abgebrochen wird
// (§16.30 Nr. 3, on_failed; Abbruch kommt mit status 'cancelled'): payload {task_id, status, result:null, error?}.
// Den Ausgangsbefehl liest der Handler aus der Aufgabe (agent.task.get → payload.command) und hält den Ausgang am
// Profil fest (onFailed in ctox.cv_print.apply_parse.js). Idempotent: eine zweite Zustellung ändert nichts.
import { callCap, fail, isObject, payloadOf } from './_base.js';
import { onFailed } from './ctox.cv_print.apply_parse.js';

/** Fehlermeldung aus dem Rückruf: error.message, sonst result.error/message, sonst „task <status>“. */
export function failureMessage(payload) {
  const candidates = [payload?.error?.message, payload?.error, payload?.result?.error, payload?.result?.message, payload?.status_note];
  const hit = candidates.find((value) => typeof value === 'string' && value.trim() !== '');
  return hit ? hit.trim() : `task ${payload?.status ?? 'failed'}`;
}

export async function originalCommand(ctx, taskId) {
  const task = await callCap(ctx, 'agent.task.get', { task_id: taskId });
  const original = isObject(task?.payload?.command) ? task.payload.command : undefined;
  if (!original) fail('not_found', `cv-print task ${taskId} carries no source command`);
  return original;
}

export default async function handle(command, ctx) {
  const payload = payloadOf(command);
  const taskId = typeof payload.task_id === 'string' && payload.task_id ? payload.task_id : undefined;
  if (taskId === undefined) fail('invalid', 'cv-print callback is missing task_id');
  const original = await originalCommand(ctx, taskId);
  return onFailed(original, { status: payload.status, message: failureMessage(payload) }, ctx);
}
