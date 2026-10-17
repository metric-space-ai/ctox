// ctox.task.delete – Port von store.rs:15709-15740 + delete_ctox_task :21846-21903.
// Shell v4 §16.25: ctox_queue_tasks ist eine read-only Projektion der Core-Queue.
// Der Handler schreibt weder sie noch business_commands; Statuswechsel und
// Löschung laufen über ctx.cap.agent.queue.task.{update, delete} (Kernvertrag:
// Statuswechsel = task.update mit patch.route_status), der Core
// ändert Queue-Eintrag und business_commands-Dokument atomar und projiziert neu.
import { invalid } from './_lib/errors.js';
import {
  TERMINAL_ROUTE_STATUSES,
  nonEmptyTrimmed,
  projectionCommandId,
  resolveTaskId,
  userLabel,
} from './_lib/tasks.js';

export default async function handle(command, ctx) {
  const p = command.payload || {};
  if (typeof p.task_id !== 'string') throw invalid('invalid ctox.task.delete payload');
  if (p.command_id !== undefined && p.command_id !== null && typeof p.command_id !== 'string') {
    throw invalid('invalid ctox.task.delete payload');
  }
  const taskId = await resolveTaskId(ctx, p.task_id);
  if (!taskId) throw invalid('task_id is required');
  const current = await ctx.cap.agent.queue.task.get({ task_id: taskId });
  if (current && !TERMINAL_ROUTE_STATUSES.includes(current.route_status)) {
    // Fachregel: nur nicht-terminale Aufgaben werden vor dem Löschen storniert
    // (transition_business_command_for_task(…,'cancelled') mit Rückfall auf
    // update_queue_task(route_status='cancelled') – beides im Core).
    const reason = `business-os: deleted by ${await userLabel(ctx, command.actor)}`;
    await ctx.cap.agent.queue.task.update({ task_id: taskId, patch: { route_status: 'cancelled', status_note: reason } });
  }
  // Lesen der Projektion bleibt erlaubt (queue_projection_command_id).
  const commandId = nonEmptyTrimmed(p.command_id) ?? (await projectionCommandId(ctx, taskId));
  const removed = await ctx.cap.agent.queue.task.delete(
    commandId ? { task_id: taskId, command_id: commandId } : { task_id: taskId },
  );
  const effectiveCommandId =
    removed && Object.prototype.hasOwnProperty.call(removed, 'command_id') ? removed.command_id : commandId;
  return { ok: true, task_id: taskId, command_id: effectiveCommandId ?? null, status: 'cancelled' };
}
