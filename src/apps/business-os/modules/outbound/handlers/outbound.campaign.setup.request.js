// outbound.campaign.setup.request — Knopf „Campaign mit Briefing speichern“ (Briefing/Vorlage/Scope geändert).
// Strang SKL-O (Kern §16.30/§16.34): Template prompts/campaign-setup.md (wörtlich aus index.js campaignSetupPrompt,
// ref: index.js:6556; nur der Rückschrieb-Satz zeigt jetzt auf die Antwort), Auftrag über agent.task.enqueue mit dem
// App-Skill outbound/campaign-setup, Rückrufe outbound.campaign.setup.done/.failed. Ersetzt den
// business_os.chat.task aus dispatchCampaignSetupPromptTask (index.js:6453).
// payload {campaign_id, source_command_id?, briefing?, template?:{id,title}}; idempotent je source_command_id.
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { fail, isObject, loadRequired, putDefaultObject, requiredString, str } from './_lib/outbound.js';
import { SETUP_DONE, SETUP_FAILED, SETUP_SKILL, campaignBriefing } from './_lib/campaign-setup.js';

const TITLE = 'Outbound Campaign einrichten'; // ref: index.js t('campaignSetupTaskTitle')

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const campaignId = requiredString(payload, ['campaign_id']);
  const sourceCommandId = str(payload, ['source_command_id']) ?? String(command.id ?? '');
  if (!sourceCommandId) fail('invalid', 'source_command_id is required');
  const campaign = await loadRequired(ctx, 'outbound_campaigns', campaignId, 'campaign not found');
  const previous = campaign.payload?.campaign_setup_task;
  if (isObject(previous) && previous.source_command_id === sourceCommandId && typeof previous.task_id === 'string' && previous.task_id) {
    return { ok: true, applied: false, campaign_id: campaignId, task_id: previous.task_id };
  }
  const briefing = str(payload, ['briefing']) ?? String(campaignBriefing(campaign) || '').trim();
  if (!briefing) fail('invalid', 'campaign briefing is empty');
  const templateId = str(payload, ['template', 'id']);
  const template = templateId ? { id: templateId, title: str(payload, ['template', 'title']) ?? templateId } : null;
  const name = str(campaign, ['name']) ?? campaignId;
  const instruction = await renderPrompt(ctx, 'campaign-setup', {
    campaign_id: campaignId,
    campaign_name: name,
    command_id: sourceCommandId,
    template_id: template ? template.id : 'custom',
    template_title: template ? template.title : 'custom',
    briefing,
  });
  const selectedCampaign = { ...campaign, payload: { ...(campaign.payload ?? {}), briefing } };
  delete selectedCampaign.payload.campaign_setup_task;
  const task = await ctx.cap.agent.task.enqueue({
    title: `${TITLE} · ${name}`,
    instruction: instruction.replace(/\n$/, ''),
    skill: SETUP_SKILL,
    thread_key: `business-os/outbound/${campaignId}`,
    payload: {
      campaign_id: campaignId,
      source_command_id: sourceCommandId,
      selected_campaign: selectedCampaign,
      selected_template: template,
    },
    idempotency_key: `outbound-campaign-setup-${sourceCommandId}`,
    on_complete: SETUP_DONE,
    on_failed: SETUP_FAILED,
  });
  const taskId = typeof task?.task_id === 'string' ? task.task_id.trim() : '';
  if (!taskId) fail('unavailable', 'agent task enqueue did not return task_id');
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const current = await loadRequired(tx, 'outbound_campaigns', campaignId, 'campaign not found');
    current.id = campaignId;
    putDefaultObject(current, 'payload');
    current.payload.campaign_setup_task = {
      command_id: sourceCommandId,
      source_command_id: sourceCommandId,
      request_command_id: String(command.id ?? ''),
      task_id: taskId,
      status: 'queued',
      skill: SETUP_SKILL,
      briefing_template_id: template ? template.id : '',
      ...(str(current.payload, ['briefing_language']) ? { briefing_language: current.payload.briefing_language } : {}),
      requested_at_ms: now,
    };
    current.updated_at_ms = now;
    await tx.put('outbound_campaigns', current);
    return { ok: true, applied: true, status: 'accepted', campaign_id: campaignId, task_id: taskId };
  });
}
