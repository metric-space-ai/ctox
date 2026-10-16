// ctox.task.update – Port von store.rs:15676-15708 + update_ctox_task :21801-21844.
// Autorisierung (CtoxTaskManage, Task-Scope) erfolgt zentral im Core vor dem
// Handler (enforce_command_policy); session_can_manage_task ist dieselbe Prüfung.
import { invalid } from './_lib/errors.js';
import { nonEmptyTrimmed, resolveTaskId, userLabel } from './_lib/tasks.js';

export default async function handle(command, ctx) {
  const p = command.payload || {};
  if (typeof p.task_id !== 'string') throw invalid('invalid ctox.task.update payload');
  for (const key of ['title', 'prompt', 'priority']) {
    if (p[key] !== undefined && p[key] !== null && typeof p[key] !== 'string') {
      throw invalid('invalid ctox.task.update payload');
    }
  }
  const taskId = await resolveTaskId(ctx, p.task_id);
  if (!taskId) throw invalid('task_id is required');
  const title = nonEmptyTrimmed(p.title);
  const prompt = nonEmptyTrimmed(p.prompt);
  const priority = nonEmptyTrimmed(p.priority);
  if (title === null && prompt === null && priority === null) throw invalid('nothing to update');
  const label = await userLabel(ctx, command.actor);
  // Shell v4 §16.25: ctox_queue_tasks ist read-only. channels::update_queue_task +
  // write_queue_task_projection laufen atomar im Core über die Capability
  // (V4-HANDLERS.md „Queue-Task-Schreib-Capabilities“); Rückgabe = neu
  // projiziertes ctox_queue_tasks-Dokument.
  const task = await ctx.cap.agent.queue.task.update({
    task_id: taskId,
    patch: { title, prompt, priority, status_note: `business-os: edited by ${label}` },
  });
  return { ok: true, task };
}
