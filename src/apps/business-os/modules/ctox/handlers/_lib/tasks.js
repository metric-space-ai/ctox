// Helfer aus store.rs (4794af03d) für ctox.task.update/delete.
export const TERMINAL_ROUTE_STATUSES = ['handled', 'cancelled', 'failed', 'completed', 'superseded'];

// store.rs::normalize_task_id
export function normalizeTaskId(value) {
  const trimmed = String(value ?? '').trim();
  return trimmed.startsWith('queue-') ? trimmed.slice('queue-'.length) : trimmed;
}

// store.rs::resolve_ctox_task_id – Task-ID oder Befehls-ID → Task-ID.
export async function resolveTaskId(ctx, value) {
  const taskId = normalizeTaskId(value);
  if (!taskId) return taskId;
  if (await ctx.cap.agent.queue.task.get({ task_id: taskId })) return taskId;
  const resolved = await ctx.cap.agent.queue.task.for_command({ command_id: taskId });
  return (resolved && resolved.task_id) || taskId;
}

export function nonEmptyTrimmed(value) {
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

// store.rs::session_user_label – "{display_name} ({role})".
// UNKLAR: Handler-actor trägt nur {id, role}; display_name wird aus
// business_users gelesen, Rückfall auf actor.id.
export async function userLabel(ctx, actor) {
  if (!actor || !actor.id) return 'unknown user';
  const user = await ctx.get('business_users', actor.id).catch(() => null);
  const name = (user && (user.display_name || user.displayName)) || actor.id;
  return `${name} (${actor.role ?? user?.role ?? ''})`;
}

// store_projections.rs::queue_projection_command_id – nur Lesen; ctox_queue_tasks
// ist eine read-only Projektion der Core-Queue (Shell v4 §16.25).
export async function projectionCommandId(ctx, taskId) {
  const doc = await ctx.get('ctox_queue_tasks', taskId);
  const value = typeof doc?.command_id === 'string' ? doc.command_id.trim() : '';
  return value || null;
}
