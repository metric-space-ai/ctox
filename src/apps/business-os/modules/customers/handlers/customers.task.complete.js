// customers.task.complete – ref: store_customer_commands.rs:726-753 (customers_task_complete) @ 4794af03d
import {
  requiredFromPayloadOrRecord, loadRequired, refreshSearchText, putRecord, writeTaskActivity,
} from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const taskId = requiredFromPayloadOrRecord(command, ['task_id', 'id'], 'task_id is required');
    const task = await loadRequired(tx, 'customer_tasks', taskId, 'customer task not found');
    task.status = 'completed';
    task.completed_at_ms = now;
    refreshSearchText(task, ['title', 'body', 'status']);
    await putRecord(tx, 'customer_tasks', taskId, now, task);
    const activity = await writeTaskActivity(tx, command, now, task, 'task_completed', 'Aufgabe erledigt');
    return { ok: true, task, activity };
  });
}
