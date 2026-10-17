// customers.task.create – ref: store_customer_commands.rs:638-675 (customers_task_create) @ 4794af03d
import {
  idFromCommand, ensureAbsent, objectPayload, requiredStr, validateLinks, putDefaultString, putDefaultI64,
  putDefaultBool, putDefaultObject, validateAllowed, CUSTOMER_TASK_STATUSES, refreshSearchText, putRecord,
  writeTaskActivity,
} from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const taskId = idFromCommand(command, ['task_id', 'id'], 'task');
    await ensureAbsent(tx, 'customer_tasks', taskId, 'customer task already exists');
    const task = objectPayload(command.payload);
    task.id = taskId;
    requiredStr(task, ['title']);
    await validateLinks(tx, task, true);
    putDefaultString(task, 'status', 'open');
    putDefaultI64(task, 'position', now);
    putDefaultBool(task, 'is_deleted', false);
    putDefaultI64(task, 'created_at_ms', now);
    putDefaultObject(task, 'payload');
    validateAllowed('task status', requiredStr(task, ['status']), CUSTOMER_TASK_STATUSES);
    refreshSearchText(task, ['title', 'body', 'status']);
    await putRecord(tx, 'customer_tasks', taskId, now, task);
    const activity = await writeTaskActivity(tx, command, now, task, 'task_created', 'Aufgabe erstellt');
    return { ok: true, task, activity };
  });
}
