// outbound.research_source.generate_adapter.done — Rückruf des Kerns (§16.30 Nr. 3, on_complete/on_failed) nach dem
// Generierungsauftrag aus outbound.research_source.generate_adapter (Strang OUT3; vorher scrape.repair_task.create ohne
// Rückweg). payload {task_id, status, result, workspace_refs?}. Den Adapter liest der Handler aus der Aufgabe
// (agent.task.get → payload.adapter_id/target_key).
//   completed  → Skript in der OLG-Registry registriert? (foreign read, registry.rs:66 über OLG) → adapter_ready/registered,
//                sonst adapter_failed/generation_incomplete
//   failed/blocked/cancelled → adapter_failed/generation_failed mit Fehlertext
// Idempotent je task_id (generation_task_outcome.task_id am Datensatz); ein Rückruf für einen älteren Auftrag als den
// zuletzt gestellten (scrape_registry_effect.generation_task.task_id) ändert den Status nicht.
// ref: Statuswerte wie store_outbound_commands.rs:3101 outbound_apply_research_adapter_scrape_effect.
import { fail, isObject, load, payloadInsert } from './_lib/outbound.js';
import { targetRegistration } from './_lib/olg-registry.js';
import { TERMINAL_FAILED, failureText } from './_lib/research-task.js';

const ADAPTERS = 'outbound_research_adapters';

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const taskId = typeof payload.task_id === 'string' ? payload.task_id.trim() : '';
  if (taskId === '') fail('invalid', 'outbound.research_source.generate_adapter.done is missing task_id');
  const status = typeof payload.status === 'string' ? payload.status : '';
  if (status !== 'completed' && !TERMINAL_FAILED.includes(status)) {
    fail('invalid', `outbound.research_source.generate_adapter.done: unknown task status ${status}`);
  }
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  const origin = task?.payload;
  if (!isObject(origin) || origin.source !== 'outbound.research_source.generate_adapter' || typeof origin.adapter_id !== 'string') {
    fail('not_found', `task ${taskId} carries no research adapter generation`);
  }
  const record = await load(ctx, ADAPTERS, origin.adapter_id);
  if (!record) fail('not_found', `research adapter ${origin.adapter_id} not found`);
  if (record.payload?.generation_task_outcome?.task_id === taskId) return { ok: true, task_id: taskId, duplicate: true };
  const current = record.payload?.scrape_registry_effect?.generation_task?.task_id;
  if (typeof current === 'string' && current !== taskId) return { ok: true, task_id: taskId, stale: true, current_task_id: current };

  const now = ctx.now();
  let outcome;
  if (status === 'completed') {
    const registration = await targetRegistration(ctx, origin.target_key);
    if (registration?.script_registered === true) {
      record.status = 'adapter_ready';
      record.scrape_status = 'registered';
      record.last_error = '';
      outcome = 'registered';
    } else {
      record.status = 'adapter_failed';
      record.scrape_status = 'generation_incomplete';
      record.last_error = 'generation task completed without a registered script';
      outcome = 'incomplete';
    }
  } else {
    record.status = 'adapter_failed';
    record.scrape_status = 'generation_failed';
    record.last_error = failureText(command);
    outcome = 'failed';
  }
  payloadInsert(record, 'generation_task_outcome', { task_id: taskId, task_status: status, outcome, at_ms: now });
  record.updated_at_ms = now;
  await ctx.put(ADAPTERS, record);
  return { ok: true, task_id: taskId, adapter_id: record.id, status: record.status, scrape_status: record.scrape_status, outcome };
}
