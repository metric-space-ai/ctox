// Gemeinsamer Ablauf aus harness_cockpit.rs::control (4794af03d, Z. 84-213):
// Absicht vor jeder Zustandsänderung auditieren (hart), Ergebnis verlustbehaftet
// auditieren. Die frühere Projektionsauffrischung (harness_cockpit::schedule_refresh)
// entfällt in v4: ctox_queue_tasks ist eine Live-Projektion des Core.
import { charCount, fail, invalid } from './errors.js';

// harness_cockpit.rs::optional_text
export function optionalText(payload, field, max) {
  const value = payload?.[field];
  if (value === undefined || value === null) return null;
  if (typeof value !== 'string') throw invalid(`${field} must be text`);
  if (charCount(value) > max) throw invalid(`${field} exceeds ${max} characters`);
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

// harness_cockpit.rs::control_audit_payload – nur erwartete Skalare, begrenzt.
export function controlAuditPayload(payload) {
  const audit = {};
  for (const [field, maxChars] of [
    ['task_id', 1024],
    ['priority', 16],
    ['reason', 1000],
    ['note', 1000],
  ]) {
    const value = payload?.[field];
    if (typeof value === 'string') audit[field] = [...value.trim()].slice(0, maxChars).join('');
  }
  const workers = payload?.workers;
  if (Number.isInteger(workers) && workers >= 0) audit.workers = workers;
  if (typeof payload?.paused === 'boolean') audit.paused = payload.paused;
  return audit;
}

export function cockpitTaskId(payload) {
  return typeof payload?.task_id === 'string' ? payload.task_id.trim() : '';
}

export async function runCockpitControl(command, ctx, body) {
  const payload = command.payload || {};
  const taskId = cockpitTaskId(payload);
  const actor = command.actor?.id ?? null;
  await ctx.cap.agent.flow.record({
    event_kind: 'cockpit.control',
    title: command.type,
    message_key: taskId || null,
    metadata: {
      actor,
      command_type: command.type,
      stage: 'requested',
      payload: controlAuditPayload(payload),
    },
  });
  let auditResult;
  try {
    if (command.type !== 'ctox.queue.pause' && command.type !== 'ctox.queue.capacity') {
      if (!taskId) throw invalid('task_id is required');
    }
    const result = await body(payload, taskId);
    auditResult = result;
    return result;
  } catch (error) {
    auditResult = { ok: false, error: String(error?.message ?? error) };
    throw error;
  } finally {
    try {
      await ctx.cap.agent.flow.record({
        event_kind: 'cockpit.control',
        title: command.type,
        message_key: taskId || null,
        metadata: { actor, command_type: command.type, stage: 'result', result: auditResult },
      });
    } catch (error) {
      ctx.log('warn', 'cockpit.control result audit failed', { error: String(error?.message || error) });
    }
  }
}

// Arm "ctox.queue.release" | "ctox.queue.block" | "ctox.queue.retry".
export async function controlQueueTask(command, ctx) {
  return runCockpitControl(command, ctx, async (payload, taskId) => {
    if (!taskId) throw invalid('task_id is required');
    const current = await ctx.cap.agent.queue.task.get({ task_id: taskId });
    if (!current) throw fail('not_found', 'queue task not found');
    if (current.route_status === 'leased') {
      throw fail('conflict', 'active leases must finish before release, block or retry; abort_turn is unsupported');
    }
    const block = command.type === 'ctox.queue.block';
    const retry = command.type === 'ctox.queue.retry';
    if (retry && !['failed', 'blocked'].includes(current.route_status)) {
      throw fail('conflict', 'retry requires a failed or blocked task');
    }
    const note = optionalText(payload, block ? 'reason' : 'note', 1000);
    if (block && note === null) throw invalid('reason is required');
    const priority = command.type === 'ctox.queue.release' ? optionalText(payload, 'priority', 16) : null;
    const task = await ctx.cap.agent.queue.control({
      action: block ? 'block' : retry ? 'retry' : 'release',
      task_id: taskId,
      value: {
        route_status: block ? 'blocked' : 'pending',
        priority,
        clear_note: retry,
        status_note: note,
        // channels::control_queue_task(…, reset = retry)
        reset_attempts: retry,
      },
    });
    return { ok: true, task };
  });
}
