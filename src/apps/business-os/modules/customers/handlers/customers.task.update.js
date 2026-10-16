// customers.task.update – ref: store_customer_commands.rs:677-724 (customers_task_update) @ 4794af03d
import {
  requiredFromPayloadOrRecord, loadRequired, mergeFields, validateLinks, validateAllowed, requiredStr,
  CUSTOMER_TASK_STATUSES, refreshSearchText, putRecord, writeTaskActivity,
} from './_lib.js';

const FIELDS = [
  'title', 'body', 'status', 'due_at_ms', 'assignee_id', 'account_id', 'contact_id', 'opportunity_id', 'position',
  'payload',
];

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const taskId = requiredFromPayloadOrRecord(command, ['task_id', 'id'], 'task_id is required');
    const task = await loadRequired(tx, 'customer_tasks', taskId, 'customer task not found');
    mergeFields(task, command.payload, FIELDS);
    await validateLinks(tx, task, true);
    validateAllowed('task status', requiredStr(task, ['status']), CUSTOMER_TASK_STATUSES);
    refreshSearchText(task, ['title', 'body', 'status']);
    await putRecord(tx, 'customer_tasks', taskId, now, task);
    const activity = await writeTaskActivity(tx, command, now, task, 'task_updated', 'Aufgabe aktualisiert');
    return { ok: true, task, activity };
  });
}
