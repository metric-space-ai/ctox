// outbound.pipeline.outreach_draft.done — Rückruf des Kerns (§16.30 Nr. 3, on_complete) nach dem Entwurfsauftrag aus
// outbound.pipeline.outreach_draft (Strang SKL-O). payload {task_id, status:'completed', result}.
// Dünner Abbilder: pipeline_id/contact_index aus der Aufgabe, Antwort = erstes JSON-Objekt
// (ref: store.rs:23796 extract_json_object) mit den vier Nachrichtenfeldern → vorhandener Befehl
// outbound.pipeline.write_outreach_draft (ref: store_outbound_commands.rs:57-136: schreibt messages, löscht
// outreach_generating, outreach_status 'drafted'). Früher schrieb der Agent selbst per CLI zurück.
// Unbrauchbare Antwort → outreach_status 'draft_failed' + outreach_error. Idempotent: Doppelzustellung oder
// neuerer Auftrag am Kontakt → keine Wirkung.
import { asI64, fail, load } from './_lib/outbound.js';
import { loadTaskCallback, replyObject } from './_lib/agent-callback.js';
import { contactAwaitsTask, parseDraftReply } from './_lib/outreach-draft.js';
import { markDraftFailed } from './outbound.pipeline.outreach_draft.failed.js';
import writeOutreachDraft from './outbound.pipeline.write_outreach_draft.js';

export default async function handle(command, ctx) {
  const { taskId, taskPayload, payload } = await loadTaskCallback(command, ctx, 'outbound.pipeline.outreach_draft.done', 'completed');
  const pipelineId = typeof taskPayload.pipeline_id === 'string' ? taskPayload.pipeline_id : '';
  const contactIndex = asI64(taskPayload.contact_index);
  if (!pipelineId || contactIndex == null) fail('not_found', `task ${taskId} carries no pipeline_id/contact_index`);
  const item = await load(ctx, 'outbound_pipeline_items', pipelineId);
  if (!item) fail('not_found', 'pipeline item not found');
  const base = { pipeline_id: pipelineId, contact_index: contactIndex, task_id: taskId };
  if (!contactAwaitsTask(item.contacts?.[contactIndex], taskId)) return { ok: true, applied: false, ...base };
  const parsed = parseDraftReply(replyObject(payload.result));
  if (parsed.error) return markDraftFailed(ctx, { pipelineId, contactIndex, taskId, outcome: 'failed', message: parsed.error });
  const out = await writeOutreachDraft({
    id: `${command.id ?? taskId}:write_outreach_draft`,
    type: 'outbound.pipeline.write_outreach_draft',
    app_id: command.app_id,
    record_id: pipelineId,
    payload: { pipeline_id: pipelineId, contact_index: contactIndex, messages: parsed.messages },
  }, ctx);
  return { ...out, applied: true, task_id: taskId };
}
