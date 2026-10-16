// Origin: CTOX
// License: Apache-2.0
// ctox.cv_print.apply_parse.done – Rückruf des Kerns nach Abschluss der Agentenaufgabe (§16.30 Nr. 3, on_complete):
// payload {task_id, status:'completed', result, workspace_refs?}, höchstens einmal je task_id. Den Ausgangsbefehl
// liest der Handler aus der Aufgabe (agent.task.get → payload.command, gesetzt von ctox.cv_print.apply_parse) und
// führt dessen Rückschrieb aus: onReply (Profil-Version anlegen; Doppelzustellung ohne Wirkung). Fehlschlag und
// Abbruch kommen über on_failed als ctox.cv_print.apply_parse.failed; ein anderer Status hier wird dorthin geleitet.
import { fail, payloadOf } from './_base.js';
import { onReply, onFailed } from './ctox.cv_print.apply_parse.js';
import { failureMessage, originalCommand } from './ctox.cv_print.apply_parse.failed.js';

export default async function handle(command, ctx) {
  const payload = payloadOf(command);
  const taskId = typeof payload.task_id === 'string' && payload.task_id ? payload.task_id : undefined;
  if (taskId === undefined) fail('invalid', 'cv-print callback is missing task_id');
  const original = await originalCommand(ctx, taskId);
  if (payload.status !== 'completed') {
    return onFailed(original, { status: payload.status, message: failureMessage(payload) }, ctx);
  }
  const result = payload.result;
  const text = typeof result === 'string' ? result : (typeof result?.text === 'string' ? result.text : JSON.stringify(result ?? ''));
  return onReply(original, { task_id: taskId, text }, ctx);
}
