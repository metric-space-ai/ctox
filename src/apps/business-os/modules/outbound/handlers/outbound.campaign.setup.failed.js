// outbound.campaign.setup.failed — Rückruf des Kerns (§16.30 Nr. 3, on_failed; Abbruch mit status 'cancelled') nach
// dem Setup-Auftrag aus outbound.campaign.setup.request (Strang SKL-O). Früher blieb ein gescheiterter Chat-Auftrag
// still auf „requested“; jetzt hält die App den Ausgang am Datensatz fest (campaign_setup_task.status 'failed' bzw.
// 'cancelled' + error), die UI meldet ihn in der Zustandszone. Idempotent wie .done.
import { fail, load, putDefaultObject } from './_lib/outbound.js';
import { failureMessage, loadTaskCallback } from './_lib/agent-callback.js';
import { SETUP_TERMINAL, setupMatchesTask } from './_lib/campaign-setup.js';

export default async function handle(command, ctx) {
  const { taskId, status, taskPayload, payload } = await loadTaskCallback(command, ctx, 'outbound.campaign.setup.failed', 'failed');
  const campaignId = typeof taskPayload.campaign_id === 'string' ? taskPayload.campaign_id : '';
  if (!campaignId) fail('not_found', `task ${taskId} carries no campaign_id`);
  const sourceCommandId = typeof taskPayload.source_command_id === 'string' ? taskPayload.source_command_id : '';
  const outcome = status === 'cancelled' ? 'cancelled' : 'failed';
  const message = failureMessage(payload);
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const campaign = await load(tx, 'outbound_campaigns', campaignId);
    if (!campaign) fail('not_found', 'campaign not found');
    const setup = campaign.payload?.campaign_setup_task;
    if (!setupMatchesTask(setup, taskId, sourceCommandId) || SETUP_TERMINAL.includes(setup.status)) {
      return { ok: true, applied: false, campaign_id: campaignId, task_id: taskId, status: outcome };
    }
    putDefaultObject(campaign, 'payload');
    campaign.payload.campaign_setup_task = { ...setup, task_id: taskId, status: outcome, error: message, finished_at_ms: now };
    campaign.updated_at_ms = now;
    await tx.put('outbound_campaigns', campaign);
    return { ok: false, applied: true, campaign_id: campaignId, task_id: taskId, status: outcome, error: message };
  });
}
