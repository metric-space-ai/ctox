// outbound.pipeline.outreach_draft.failed — Rückruf des Kerns (§16.30 Nr. 3, on_failed; Abbruch mit status
// 'cancelled') nach dem Entwurfsauftrag aus outbound.pipeline.outreach_draft (Strang SKL-O). Früher blieb der
// Spinner (contact.outreach_generating) stehen, wenn der Agent nie zurückschrieb; jetzt löst die App ihn selbst:
// outreach_generating false, outreach_status 'draft_failed' bzw. 'draft_cancelled', outreach_error (Zustandszone).
// Idempotent: Doppelzustellung oder neuerer Auftrag am Kontakt → keine Wirkung.
import { asI64, fail, load } from './_lib/outbound.js';
import { failureMessage, loadTaskCallback } from './_lib/agent-callback.js';
import { contactAwaitsTask } from './_lib/outreach-draft.js';

/** Kontakt auf „Entwurf gescheitert/abgebrochen“ setzen, nur wenn er noch auf genau diese Aufgabe wartet. */
export async function markDraftFailed(ctx, { pipelineId, contactIndex, taskId, outcome, message }) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const item = await load(tx, 'outbound_pipeline_items', pipelineId);
    if (!item) fail('not_found', 'pipeline item not found');
    const contact = item.contacts?.[contactIndex];
    const base = { pipeline_id: pipelineId, contact_index: contactIndex, task_id: taskId, status: outcome };
    if (!contactAwaitsTask(contact, taskId)) return { ok: true, applied: false, ...base };
    contact.outreach_generating = false;
    contact.outreach_status = outcome === 'cancelled' ? 'draft_cancelled' : 'draft_failed';
    contact.outreach_error = message;
    item.updated_at_ms = now;
    await tx.put('outbound_pipeline_items', item);
    return { ok: false, applied: true, ...base, error: message };
  });
}

export default async function handle(command, ctx) {
  const { taskId, status, taskPayload, payload } = await loadTaskCallback(command, ctx, 'outbound.pipeline.outreach_draft.failed', 'failed');
  const pipelineId = typeof taskPayload.pipeline_id === 'string' ? taskPayload.pipeline_id : '';
  const contactIndex = asI64(taskPayload.contact_index);
  if (!pipelineId || contactIndex == null) fail('not_found', `task ${taskId} carries no pipeline_id/contact_index`);
  return markDraftFailed(ctx, {
    pipelineId,
    contactIndex,
    taskId,
    outcome: status === 'cancelled' ? 'cancelled' : 'failed',
    message: failureMessage(payload),
  });
}
