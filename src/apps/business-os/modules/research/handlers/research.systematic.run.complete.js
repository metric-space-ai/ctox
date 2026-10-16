// research.systematic.run.complete – NEUER Abschluss-Befehl (Vertragswunsch).
// Ersetzt den nativen Antwortpfad store.rs::complete_business_command_from_queue_reply
// → promote_systematic_research_workspace_outputs (:10336) +
//   process_systematic_research_command (:21570) für research.systematic.run.
// Der Core ruft ihn nach dem Agenten-Turn als Rückruf (§16.30 Nr. 3) mit
//   payload = {task_id, status, result, workspace_refs}
// auf (siehe ctx.cap.agent.task.enqueue({on_complete}) in research.systematic.run.js). Die Befehls-ID kommt aus
// der Aufgabe (agent.task.get → payload.business_command_id); command_id/reply_text direkt bleiben erlaubt.
// projection.refresh entfällt (§16.30: Projektion ist live).
import { fail, invalid } from './_lib/errors.js';
import { evidenceSnapshot, promoteWorkspaceOutputs } from './_lib/systematic.js';
import { persistFailure, persistSuccess } from './_lib/records.js';

export default async function handle(command, ctx) {
  const p = command.payload || {};
  // Doppelzustellung des Rückrufs (§16.30: höchstens 1× je task_id) bleibt ohne Wirkung.
  if (typeof p.task_id === 'string' && p.task_id.trim()) {
    const done = await ctx.read('research_runs', { selector: { task_queue_id: p.task_id.trim(), status: 'completed' }, limit: 1 });
    if (done.length) return { ok: true, command_id: done[0].command_id ?? null, status: 'completed', task_id: p.task_id.trim(), duplicate: true };
  }
  let commandId = typeof p.command_id === 'string' ? p.command_id.trim() : '';
  if (!commandId && typeof p.task_id === 'string' && p.task_id.trim()) {
    const task = await ctx.cap.agent.task.get({ task_id: p.task_id.trim() });
    const fromTask = task?.payload?.business_command_id ?? task?.idempotency_key;
    commandId = typeof fromTask === 'string' ? fromTask.trim() : '';
  }
  if (!commandId) throw invalid('command_id is required');
  const original = await ctx.get('business_commands', commandId);
  if (!original) throw fail('not_found', `business command ${commandId} not found`);
  const research = {
    module: original.module,
    command_type: original.command_type,
    record_id: original.record_id ?? null,
    payload: original.payload ?? {},
    client_context: original.client_context ?? {},
  };
  if (!(research.module === 'research' && research.command_type === 'research.systematic.run')) {
    throw invalid(`business command ${commandId} is not a systematic research run`);
  }
  // v4: api.command vergibt die Befehls-ID selbst; die App bindet Lauf, Manifest und Quittungen an ihre
  // eigene Kennung payload.research_command_id. Rust nutzte dafür die Befehls-ID (identisch in v3).
  const bindingId = typeof research.payload.research_command_id === 'string' && research.payload.research_command_id.trim()
    ? research.payload.research_command_id.trim()
    : commandId;
  const lookupId = typeof p.task_id === 'string' && p.task_id.trim()
    ? p.task_id.trim()
    : (await ctx.cap.agent.queue.task.for_command({ command_id: commandId }))?.task_id;
  const queueTask = lookupId ? await ctx.cap.agent.queue.task.get({ task_id: lookupId }) : null;

  // Rust: promote läuft vor process und bricht ohne Fehlerpersistenz ab (`?`).
  await promoteWorkspaceOutputs(ctx, bindingId, research, queueTask);

  const completedAtMs = ctx.now();
  let evidence;
  try {
    evidence = await evidenceSnapshot(ctx, bindingId, research);
  } catch (error) {
    await persistFailure(ctx, {
      commandId: bindingId,
      command: research,
      queueTask,
      message: String(error?.message ?? error),
      failedAtMs: completedAtMs,
    });
    throw error;
  }
  // Rust lädt die Kern-Aufgabe nach der Validierung erneut (terminaler Stand).
  const terminal = queueTask ? (await ctx.cap.agent.queue.task.get({ task_id: queueTask.message_key })) ?? queueTask : null;
  const taskQueueId = terminal?.message_key ?? (await ctx.cap.agent.queue.task.for_command({ command_id: commandId }))?.task_id ?? null;
  const result = await persistSuccess(ctx, {
    commandId: bindingId,
    command: research,
    queueTask: terminal,
    taskQueueId,
    replyText: typeof p.reply_text === 'string' ? p.reply_text
      : (typeof p.result === 'string' ? p.result : typeof p.result?.text === 'string' ? p.result.text : ''),
    evidence,
    completedAtMs,
  });
  return {
    ok: true,
    command_id: commandId,
    status: 'completed',
    task_id: taskQueueId,
    task_status: 'completed',
    result,
  };
}
