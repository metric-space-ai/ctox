// outbound.campaign.setup.done — Rückruf des Kerns (§16.30 Nr. 3, on_complete) nach dem Setup-Auftrag aus
// outbound.campaign.setup.request (Strang SKL-O). payload {task_id, status:'completed', result}.
// Dünner Abbilder: campaign_id/source_command_id aus der Aufgabe, Antwort = erstes JSON-Objekt
// (ref: store.rs:23796 extract_json_object) → Patch nur mit Schlüsseln des früheren Schreibvertrags → vorhandener
// Befehl outbound.campaign.apply_setup (ref: store_outbound_commands.rs:5630). Früher schrieb der Agent selbst per
// CLI; jetzt ist der Rückweg der Vertrag. Unbrauchbare Antwort → campaign_setup_task.status 'failed' (sichtbar in der
// Zustandszone). Idempotent: Doppelzustellung, abgeschlossenes oder neueres Setup → keine Wirkung.
import { fail, load, putDefaultObject } from './_lib/outbound.js';
import { loadTaskCallback, replyObject } from './_lib/agent-callback.js';
import { SETUP_SKILL, SETUP_TERMINAL, parseSetupReply, setupMatchesTask } from './_lib/campaign-setup.js';
import applySetup from './outbound.campaign.apply_setup.js';

export default async function handle(command, ctx) {
  const { taskId, taskPayload, payload } = await loadTaskCallback(command, ctx, 'outbound.campaign.setup.done', 'completed');
  const campaignId = typeof taskPayload.campaign_id === 'string' ? taskPayload.campaign_id : '';
  if (!campaignId) fail('not_found', `task ${taskId} carries no campaign_id`);
  const sourceCommandId = typeof taskPayload.source_command_id === 'string' ? taskPayload.source_command_id : '';
  const campaign = await load(ctx, 'outbound_campaigns', campaignId);
  if (!campaign) fail('not_found', 'campaign not found');
  const setup = campaign.payload?.campaign_setup_task;
  if (!setupMatchesTask(setup, taskId, sourceCommandId) || SETUP_TERMINAL.includes(setup.status)) {
    return { ok: true, applied: false, campaign_id: campaignId, task_id: taskId };
  }
  const parsed = parseSetupReply(replyObject(payload.result));
  if (parsed.error) {
    const now = ctx.now();
    return ctx.transaction(async (tx) => {
      const current = await load(tx, 'outbound_campaigns', campaignId);
      if (!current) fail('not_found', 'campaign not found');
      putDefaultObject(current, 'payload');
      current.payload.campaign_setup_task = { ...current.payload.campaign_setup_task, task_id: taskId, status: 'failed', error: parsed.error, finished_at_ms: now };
      current.updated_at_ms = now;
      await tx.put('outbound_campaigns', current);
      return { ok: false, applied: true, campaign_id: campaignId, task_id: taskId, error: parsed.error };
    });
  }
  const out = await applySetup({
    id: `${command.id ?? taskId}:apply_setup`,
    type: 'outbound.campaign.apply_setup',
    app_id: command.app_id,
    payload: {
      campaign_id: campaignId,
      source_command_id: sourceCommandId,
      task_id: taskId,
      skill: SETUP_SKILL,
      ...(parsed.status ? { status: parsed.status } : {}),
      campaign_payload_patch: parsed.patch,
    },
  }, ctx);
  return { ...out, applied: true, task_id: taskId };
}
