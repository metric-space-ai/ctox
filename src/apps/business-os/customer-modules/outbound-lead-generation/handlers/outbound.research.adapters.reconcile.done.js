// outbound.research.adapters.reconcile.done — Rückruf des Kerns (§16.30 Nr. 3, on_complete/on_failed) nach der
// Abgleich-Aufgabe aus outbound.research.adapters.reconcile. payload {task_id, status, result, workspace_refs?}.
// Den Ausgangsbefehl liest der Handler aus der Aufgabe (agent.task.get → payload.command) und führt Phase 2 des
// Abgleichs aus (agent_reply {task_id, text} bzw. {task_id, error}; ref store.rs:10369-10404, :15080).
// Idempotent je task_id: trägt die Richtlinie schon ein Abgleichsergebnis dieser Aufgabe, ist der Rückruf ein
// Duplikat und schreibt nichts.
import { fail, isObject, str } from './_lib/olg.js';
import handleReconcile from './outbound.research.adapters.reconcile.js';

const TERMINAL_FAILED = ['failed', 'blocked', 'cancelled'];

function replyText(result) {
  if (typeof result === 'string') return result;
  if (isObject(result) && typeof result.text === 'string') return result.text;
  return JSON.stringify(result ?? '');
}

function failureText(payload) {
  const note = [payload?.status_note, payload?.result?.status_note, payload?.result?.error, payload?.result?.message]
    .find((v) => typeof v === 'string' && v.trim() !== '');
  return note ? note.trim() : `agent task ${payload.status}`;
}

async function alreadyApplied(ctx, original, taskId) {
  const policyId = str(original.payload, ['policy_id']);
  const collection = str(original.payload?.writeback_contract, ['policy_collection']);
  if (policyId == null || collection == null) return false;
  const policy = await ctx.get(collection, policyId).catch(() => null);
  return Boolean(policy && policy.reconciliation_task_id === taskId
    && typeof policy.reconciliation_status === 'string' && policy.reconciliation_status !== '');
}

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const taskId = typeof payload.task_id === 'string' ? payload.task_id : '';
  if (taskId === '') fail('invalid', 'outbound.research.adapters.reconcile.done is missing task_id');
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  const original = task?.payload?.command;
  if (!isObject(original) || original.type !== 'outbound.research.adapters.reconcile') {
    fail('not_found', `task ${taskId} carries no adapter reconciliation command`);
  }
  if (await alreadyApplied(ctx, original, taskId)) return { ok: true, task_id: taskId, duplicate: true };
  const status = typeof payload.status === 'string' ? payload.status : '';
  let agentReply;
  if (status === 'completed') agentReply = { task_id: taskId, text: replyText(payload.result) };
  else if (TERMINAL_FAILED.includes(status)) agentReply = { task_id: taskId, error: failureText(payload) };
  else fail('invalid', `outbound.research.adapters.reconcile.done: unknown task status ${status}`);
  // Umschlag (Akteur, app_id) vom Rückruf, Identität/Payload/Client-Kontext vom Ausgangsbefehl.
  return handleReconcile({
    ...command, id: original.id, type: original.type, payload: isObject(original.payload) ? original.payload : {},
    client_context: isObject(original.client_context) ? original.client_context : command.client_context, agent_reply: agentReply,
  }, ctx);
}
