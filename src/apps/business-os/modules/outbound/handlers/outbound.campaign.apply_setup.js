// ref: store_outbound_commands.rs:5630 outbound_handle_campaign_apply_setup
// Writeback of the campaign-setup agent skill. Rust wrote business_records AND
// the RxDB mirror (upsert_rxdb_collection_record); in v4 that is one ctx.put.
import { requiredString, loadRequired, str, fail, isObject, putDefaultObject } from './_lib/outbound.js';

const STATUSES = new Set(['setup_required', 'active', 'paused', 'closed']);

export default async function handle(command, ctx) {
  const now = ctx.now();
  const payload = command.payload ?? {};
  const campaignId = requiredString(payload, ['campaign_id']);
  const patch = payload.campaign_payload_patch;
  if (!isObject(patch)) fail('invalid', 'campaign_payload_patch object is required');
  return ctx.transaction(async (tx) => {
    const campaign = await loadRequired(tx, 'outbound_campaigns', campaignId, 'campaign not found');
    campaign.id = campaignId;
    putDefaultObject(campaign, 'payload');
    const status = str(payload, ['status']);
    if (status != null && STATUSES.has(status)) campaign.status = status;
    Object.assign(campaign.payload, JSON.parse(JSON.stringify(patch)));
    const applyCommandId = command.id ?? '';
    const sourceCommandId = str(payload, ['source_command_id']) ?? '';
    // Strang SKL-O: aus dem Rückruf outbound.campaign.setup.done kommt die task_id mit (Idempotenz je Aufgabe).
    const taskId = str(payload, ['task_id']);
    campaign.payload.campaign_setup_task = {
      ...(taskId != null ? { task_id: taskId } : {}),
      command_id: sourceCommandId ? sourceCommandId : applyCommandId,
      apply_command_id: applyCommandId,
      source_command_id: sourceCommandId,
      status: 'completed',
      skill: str(payload, ['skill']) ?? 'business-os-outbound-campaign-setup',
      applied_at_ms: now,
    };
    campaign.updated_at_ms = now;
    await tx.put('outbound_campaigns', campaign);
    return { ok: true, campaign, campaign_id: campaignId };
  });
}
