// Kampagnen-Setup als Agentenauftrag (Strang SKL-O, Kern §16.30).
//
// Vorher (F-automatisierung-skills §1 #16): die UI öffnete beim Speichern eines geänderten Briefings einen
// `business_os.chat.task` (index.js dispatchCampaignSetupPromptTask), der Prompt stand als String in index.js
// (campaignSetupPrompt) und der Agent schrieb laut Skill-Text selbst per CLI `outbound.campaign.apply_setup` zurück.
// Jetzt: outbound.campaign.setup.request füllt prompts/campaign-setup.md, stellt den Auftrag mit dem App-Skill
// outbound/campaign-setup und den Rückrufen outbound.campaign.setup.done/.failed; der Rückruf bildet die Antwort
// auf den vorhandenen Befehl outbound.campaign.apply_setup ab. Zustand am Datensatz:
// outbound_campaigns.payload.campaign_setup_task = {task_id, source_command_id, status, …}.
import { isObject, str } from './outbound.js';

export const SETUP_SKILL = 'outbound/campaign-setup';
export const SETUP_DONE = 'outbound.campaign.setup.done';
export const SETUP_FAILED = 'outbound.campaign.setup.failed';
export const SETUP_TERMINAL = ['completed', 'failed', 'cancelled'];

// Schreibvertrag des früheren Chat-Auftrags (index.js writebackContract.allowed_payload_fields).
export const ALLOWED_PATCH_KEYS = [
  'campaign_setup',
  'research_settings',
  'active_outreach',
  'communication_strategy',
  'sequence_strategy',
  'landing_page',
  'app_extension_requests',
];

/** ref: index.js campaignBriefing – Briefing-Text einer Campaign. */
export function campaignBriefing(campaign) {
  return campaign?.payload?.briefing
    || campaign?.payload?.campaign_idea
    || campaign?.payload?.scope
    || campaign?.objective
    || '';
}

/** Gehört der Rückruf zum aktuellen Setup der Campaign? (task_id, sonst Ausgangsbefehl) */
export function setupMatchesTask(setup, taskId, sourceCommandId) {
  if (!isObject(setup)) return false;
  if (typeof setup.task_id === 'string' && setup.task_id) return setup.task_id === taskId;
  return Boolean(sourceCommandId) && setup.source_command_id === sourceCommandId;
}

/**
 * Antwort des Agenten → {status?, patch} oder {error}. Form {status?, campaign_payload_patch:{…}} oder der Patch
 * selbst; nur Schlüssel des Schreibvertrags; campaign_setup mit status ist Pflicht (Completion-Gate des Skills).
 */
export function parseSetupReply(reply) {
  if (!isObject(reply)) return { error: 'campaign setup reply contained no JSON object' };
  const raw = isObject(reply.campaign_payload_patch) ? reply.campaign_payload_patch : reply;
  const patch = {};
  for (const key of ALLOWED_PATCH_KEYS) if (key in raw) patch[key] = raw[key];
  if (!isObject(patch.campaign_setup) || str(patch.campaign_setup, ['status']) == null) {
    return { error: 'campaign setup reply has no campaign_setup.status' };
  }
  const status = str(reply, ['status']);
  return { patch, ...(status != null ? { status } : {}) };
}
