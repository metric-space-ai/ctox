// Lead- und Kampagnenrecherche als App-Handler (Strang SKL-C, Kern §16.30/§16.34).
//
// Vorher (Bestandsaufnahme F §1 #22/#23): der Browser schickte `business_os.chat.task` ohne App-Handler; der Core baute
// den Prompt (store.rs:27009 command_prompt mit dem Outbound-Sonderblock :27037-27056), wählte die Skills per
// required_skills und der Agent schrieb per MCP business_os.execute_writeback zurück. Die Kampagnenrecherche
// fächerte über einen Agenten-Planer per MCP web_stack.person_research je Lead auf.
// Jetzt:
//   outbound.lead.research        Template prompts/lead-research(-continuation).md füllen → agent.task.enqueue mit
//                                 Paket-Skill outbound-lead-generation/research, on_complete/on_failed eigene Befehle
//   outbound.campaign.research    der Handler fächert selbst je Lead auf (je Lead ein outbound.lead.research mit
//                                 idempotency_key `<run_id>:<lead_id>`; Agent-als-Planer entfällt)
//   outbound.lead.research.done   Rückruf (on_complete): JSON-Block der Schlussantwort → outbound.lead.research_writeback
//                                 (auch für den Lückenschluss-Task aus web_stack.person_research, dann mit gap_task_id)
//   outbound.lead.research.failed Rückruf (on_failed, auch Abbruch status 'cancelled'): Lead-Fehlerstand ohne Ergebnisverlust
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { fail, has, isObject, str } from './olg.js';
import { LEAD_COLLECTION } from './update-digest.js';
import { GAP_METADATA_KEY } from './research-writeback-sanitize.js';
import { handleResearchWriteback } from './research-writeback.js';
import { parseJsonBlock, replyText, failureText } from './agent-reply.js';

export const APP_ID = 'outbound-lead-generation';
export const LEAD_RESEARCH = 'outbound.lead.research';
export const CAMPAIGN_RESEARCH = 'outbound.campaign.research';
export const RESEARCH_DONE = 'outbound.lead.research.done';
export const RESEARCH_FAILED = 'outbound.lead.research.failed';
export const RESEARCH_SKILL = 'outbound-lead-generation/research';
export const WRITEBACK_COMMAND = 'outbound.lead.research_writeback';

const JSON_PREVIEW_CHARS = 32_000; // ref: store.rs:205 BUSINESS_OS_QUEUE_PROMPT_JSON_PREVIEW_CHARS
const MODES = { new_record: 'Neurecherche', update_firm: 'Nachrecherche' };
// ref: index.js researchLead (vor SKL-C :9420-9432) — D&B-Hinweise wörtlich.
const DNB_FIELDS = ['wz_code', 'umsatz', 'mitarbeiter', 'firma_geschaeftstaetigkeit'];
const DNB_SOURCE = 'dnbhoovers.com';
const DNB_PFLICHT = ' Pflicht vor allem anderen: ctox_web_scrape mit target_key "dnbhoovers-com" (mode execute, timeout_seconds 400, task_id = diese Auftrags-ID) ausführen; D&B meldet sich selbst an, ein Login ist kein Grund, es auszulassen.';
const DNB_ADAPTER = ' Die registrierten Adapter aus source_policy laufen zuerst, dnbhoovers-com eingeschlossen (timeout_seconds 400, task_id = diese Auftrags-ID); D&B meldet sich selbst an.';

const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));
const strings = (v) => (Array.isArray(v) ? v.filter((x) => typeof x === 'string' && x.trim() !== '').map((x) => x.trim()) : []);

// ref: store.rs:27126-27149 command_prompt_sensitive_key
const SENSITIVE = new Set(['authorization', 'proxy_authorization', 'cookie', 'set_cookie', 'password', 'passphrase', 'secret',
  'api_key', 'private_key', 'capability_token', 'access_token', 'refresh_token', 'id_token']);
const SENSITIVE_SUFFIX = ['_token', '_password', '_passphrase', '_secret', '_api_key', '_private_key'];
function sensitive(key) {
  const k = key.trim().toLowerCase().replaceAll('-', '_');
  return SENSITIVE.has(k) || SENSITIVE_SUFFIX.some((s) => k.endsWith(s));
}
/** ref: store.rs:27104 sanitize_command_prompt_preview */
export function sanitizePreview(value) {
  if (Array.isArray(value)) return value.map(sanitizePreview);
  if (!isObject(value)) return value;
  return Object.fromEntries(Object.entries(value).filter(([k]) => !sensitive(k)).map(([k, v]) => [k, sanitizePreview(v)]));
}
/** ref: store.rs:27708 prompt_json_preview */
export function jsonPreview(value, max = JSON_PREVIEW_CHARS) {
  const raw = JSON.stringify(value ?? {}, null, 2);
  const all = [...raw];
  if (all.length <= max) return raw;
  const preview = all.slice(0, max - 160).join('');
  return `${preview}\n... truncated ${all.length - [...preview].length} chars; full JSON is stored on the Business OS command record ...`;
}

/** D&B-Hinweis wie researchLead im Browser (vor SKL-C index.js :9416-9432). */
export function dnbHint(lead, payload, continuationFields) {
  const privateSources = strings(payload.include_private);
  if (!privateSources.includes(DNB_SOURCE)) return '';
  if (!continuationFields) return DNB_ADAPTER;
  const ohneDnb = continuationFields.some((key) => DNB_FIELDS.includes(key))
    && !JSON.stringify(lead?.evidence || []).includes('dnbhoovers')
    && !JSON.stringify(lead?.field_status || {}).includes('dnbhoovers');
  return ohneDnb ? DNB_PFLICHT : '';
}

/**
 * Anweisung füllen. ref: store.rs:27034-27066 — research_instructions als eigener Block (wörtlich, Rust-Überschrift),
 * aus der Payload-Vorschau entfernt (samt research_instructions_default), instruction/prompt entfernt, sensible Schlüssel
 * raus, Vorschau ≤ 32 000 Zeichen. Neu: stored_field_status (der Agent hat kein get_record mehr).
 */
export async function leadResearchInstruction(ctx, { recordId, commandId, lead, payload }) {
  const instructions = str(payload, ['research_instructions']);
  if (instructions == null) fail('invalid', 'research_instructions is required');
  const mode = str(payload, ['mode']) ?? 'new_record';
  if (!has(MODES, mode)) fail('invalid', `unsupported research mode ${mode}`);
  const leadName = str(payload, ['lead_name']) ?? str(lead, ['name']) ?? recordId;
  const continuation = isObject(payload.continuation) ? strings(payload.fields) : null;
  if (continuation && continuation.length === 0) fail('invalid', 'continuation requires fields');
  const preview = clone(payload);
  for (const key of ['instruction', 'prompt', 'user_message', 'text', 'research_instructions', 'research_instructions_default', 'idempotency_key']) delete preview[key];
  preview.stored_field_status = isObject(lead?.field_status) ? clone(lead.field_status) : {};
  const values = {
    lead_name: leadName, record_id: recordId, command_id: commandId, research_instructions: instructions,
    dnb_hint: dnbHint(lead, payload, continuation), payload_json: jsonPreview(sanitizePreview(preview)),
  };
  if (continuation) {
    return { title: `Lücken schließen: ${leadName}`, instruction: await renderPrompt(ctx, 'lead-research-continuation', { ...values, field_count: continuation.length, fields: continuation.join(', ') }) };
  }
  return { title: `${MODES[mode]}: ${leadName}`, instruction: await renderPrompt(ctx, 'lead-research', { ...values, research_label: MODES[mode] }) };
}

/** outbound.lead.research — Template füllen, Task anlegen, Lead auf „läuft“ mit echter Task-ID. */
export async function handleLeadResearch(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const recordId = str(command, ['record_id']) ?? str(payload, ['record_id']) ?? str(payload, ['lead_id']);
  if (recordId == null) fail('invalid', 'record_id (lead id) is required');
  const lead = await ctx.get(LEAD_COLLECTION, recordId);
  if (lead == null) fail('not_found', `lead ${recordId} does not exist`);
  const commandId = str(payload, ['command_id']) ?? command.id;
  const { title, instruction } = await leadResearchInstruction(ctx, { recordId, commandId, lead, payload });
  const runId = str(payload, ['campaign_run_id']) ?? '';
  const task = await ctx.cap.agent.task.enqueue({
    title: str(payload, ['title']) ?? title,
    instruction,
    skill: RESEARCH_SKILL,
    thread_key: str(payload, ['thread_key']) ?? `business-os/outbound-lead-generation/lead/${recordId}`,
    idempotency_key: str(payload, ['idempotency_key']) ?? `${LEAD_RESEARCH}:${command.id}`,
    payload: { lead_id: recordId, research_command_id: command.id, command_id: commandId, campaign_run_id: runId },
    on_complete: RESEARCH_DONE,
    on_failed: RESEARCH_FAILED,
  });
  const current = (await ctx.get(LEAD_COLLECTION, recordId)) ?? lead;
  await ctx.put(LEAD_COLLECTION, {
    ...current, id: recordId, research_status: 'running', research_error: '', command_id: commandId, task_id: task.task_id,
    research_updated_at_ms: ctx.now(),
  });
  return { ok: true, record_id: recordId, task_id: task.task_id, command_id: commandId, research_command_id: command.id };
}

// Felder, die bei einer Kampagne für alle Leads gleich sind (einmal im Befehl statt je Lead).
export const CAMPAIGN_SHARED_KEYS = Object.freeze(['fields', 'field_labels', 'custom_fields', 'custom_fields_note', 'field_labels_note',
  'include_private', 'person_priorities', 'research_instructions', 'research_instructions_variant', 'research_instructions_default',
  'source_policy', 'auto_browser_capture', 'writeback_contract']);

/**
 * outbound.campaign.research {campaign, run_id, scope?, shared:{…}, leads:[{lead_id, payload:{…}}]} — der Handler fächert je
 * Lead einen outbound.lead.research auf (idempotency_key `outbound.campaign.research:<run_id>:<lead_id>`: eine
 * Doppelzustellung legt keinen zweiten Task an). Ersetzt den Agenten-Planer (campaignResearchPrompt, MCP-Fan-out).
 */
export async function handleCampaignResearch(command, ctx) {
  const p = isObject(command.payload) ? command.payload : {};
  const campaign = str(p, ['campaign']);
  if (campaign == null) fail('invalid', 'campaign is required');
  const runId = str(p, ['run_id']);
  if (runId == null) fail('invalid', 'run_id is required');
  if (!Array.isArray(p.leads) || p.leads.length === 0) fail('invalid', 'leads must not be empty');
  const shared = isObject(p.shared) ? p.shared : {};
  const queued = [];
  const failed = [];
  const seen = new Set();
  for (const entry of p.leads) {
    const leadId = str(entry, ['lead_id']);
    if (leadId == null || seen.has(leadId)) continue;
    seen.add(leadId);
    const lead = await ctx.get(LEAD_COLLECTION, leadId);
    if (lead == null) { failed.push({ lead_id: leadId, error: 'Lead nicht gefunden.' }); continue; }
    const childPayload = {
      ...clone(shared), ...clone(isObject(entry.payload) ? entry.payload : {}),
      lead_id: leadId, campaign_run_id: runId, workflow_id: runId, idempotency_key: `${CAMPAIGN_RESEARCH}:${runId}:${leadId}`,
    };
    // Erst „wartet“, dann der Befehl: der Lead-Handler setzt danach „läuft“ und darf nicht zurückgestuft werden.
    if (!(lead.research_status === 'running' && lead.payload?.campaign_run_id === runId)) {
      await ctx.put(LEAD_COLLECTION, {
        ...lead, research_status: 'queued', research_error: '', research_updated_at_ms: ctx.now(),
        payload: { ...(isObject(lead.payload) ? lead.payload : {}), campaign_run_id: runId, research_queued_at_ms: ctx.now() },
      });
    }
    const child = await ctx.command(LEAD_RESEARCH, childPayload, { record_id: leadId, module: APP_ID });
    queued.push({ lead_id: leadId, command_id: child?.id ?? null });
  }
  return { ok: true, campaign, run_id: runId, scope: str(p, ['scope']) ?? 'campaign', queued, failed };
}

/** Aufgabe → (Lead, Recherchebefehl, Lückenschluss?) */
async function taskBinding(ctx, taskId) {
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  if (task == null) fail('not_found', `agent task ${taskId} does not exist`);
  const gap = isObject(task.payload) && isObject(task.payload[GAP_METADATA_KEY]) ? task.payload[GAP_METADATA_KEY] : null;
  const recordId = gap ? str(gap, ['record_id']) : str(task.payload, ['lead_id']);
  const researchCommandId = gap ? str(gap, ['research_command_id']) : str(task.payload, ['research_command_id']);
  if (recordId == null || researchCommandId == null) fail('invalid', `agent task ${taskId} carries no lead research binding`);
  return { task, gap, recordId, researchCommandId };
}

function callbackTaskId(command, type) {
  const taskId = str(command.payload, ['task_id']);
  if (taskId == null) fail('invalid', `${type} is missing task_id`);
  return taskId;
}

/** Ein Writeback dieses Rückrufs liegt schon im Lead (Revision trägt die writeback_id) → Doppelzustellung. */
export function callbackApplied(lead, writebackId) {
  const revisions = [];
  for (const status of Object.values(isObject(lead?.field_status) ? lead.field_status : {})) revisions.push(status?.revision);
  for (const fields of Object.values(isObject(lead?.person_field_status) ? lead.person_field_status : {})) {
    for (const status of Object.values(isObject(fields) ? fields : {})) revisions.push(status?.revision);
  }
  return revisions.some((r) => isObject(r) && r.writeback_id === writebackId);
}

function hasResearchResult(lead) {
  if (Array.isArray(lead?.payload?.researched_field_keys) && lead.payload.researched_field_keys.length) return true;
  return ['completed', 'needs_review'].includes(String(lead?.research_status || ''));
}

/** Fehlerstand ohne Ergebnisverlust (wie fehlerOhneErgebnisverlust im Browser, index.js). */
async function markLeadFailure(ctx, recordId, reason, taskId, { gap }) {
  const lead = await ctx.get(LEAD_COLLECTION, recordId);
  if (lead == null) return null;
  const now = ctx.now();
  const payload = { ...(isObject(lead.payload) ? lead.payload : {}), research_error: reason, research_failed_task_id: taskId };
  const next = { ...lead, research_updated_at_ms: now, payload };
  if (gap) next.research_phase = null;
  if (hasResearchResult(lead)) {
    next.research_status = lead.research_status === 'completed' ? 'completed' : 'needs_review';
    payload.research_last_failure = reason;
    payload.research_last_failure_at_ms = now;
  } else {
    next.research_status = 'failed';
    next.research_error = reason;
  }
  await ctx.put(LEAD_COLLECTION, next);
  return next.research_status;
}

/** outbound.lead.research.done {task_id, status, result} → outbound.lead.research_writeback. */
export async function handleResearchDone(command, ctx) {
  const taskId = callbackTaskId(command, RESEARCH_DONE);
  if (str(command.payload, ['status']) !== 'completed') return handleResearchFailed(command, ctx);
  const { gap, recordId, researchCommandId } = await taskBinding(ctx, taskId);
  const writebackId = `research-callback:${taskId}`;
  const lead = await ctx.get(LEAD_COLLECTION, recordId);
  if (lead == null) fail('not_found', `lead ${recordId} does not exist`);
  if (callbackApplied(lead, writebackId)) return { ok: true, task_id: taskId, record_id: recordId, duplicate: true };
  const result = command.payload.result;
  const block = isObject(result) && has(result, 'field_status') ? result : parseJsonBlock(replyText(result), (v) => isObject(v) && has(v, 'field_status'));
  if (block == null) {
    const status = await markLeadFailure(ctx, recordId, 'Die Recherche endete ohne JSON-Ergebnisblock; nichts gespeichert.', taskId, { gap });
    return { ok: false, task_id: taskId, record_id: recordId, research_status: status, reason: 'no_result_block' };
  }
  const writebackPayload = {
    record_id: recordId, module: APP_ID, research_command_id: researchCommandId,
    ...(gap ? { gap_task_id: taskId } : {}),
    field_status: block.field_status,
    ...(has(block, 'result') ? { result: block.result } : {}),
  };
  try {
    const out = await handleResearchWriteback({
      ...command, id: writebackId, type: WRITEBACK_COMMAND, module: APP_ID, record_id: recordId, payload: writebackPayload,
    }, ctx);
    return { ...out, task_id: taskId };
  } catch (error) {
    // Der Rückruf kommt höchstens einmal: ein abgelehnter Writeback darf nicht still verschwinden.
    const status = await markLeadFailure(ctx, recordId, `Rückschrieb abgelehnt: ${error.message}`, taskId, { gap });
    return { ok: false, task_id: taskId, record_id: recordId, research_status: status, error: error.message, code: error.code ?? null };
  }
}

/** outbound.lead.research.failed {task_id, status: failed|blocked|cancelled, error?} */
export async function handleResearchFailed(command, ctx) {
  const taskId = callbackTaskId(command, RESEARCH_FAILED);
  const { gap, recordId } = await taskBinding(ctx, taskId);
  const lead = await ctx.get(LEAD_COLLECTION, recordId);
  if (lead == null) return { ok: true, task_id: taskId, record_id: recordId, ignored: 'lead_missing' };
  if (lead.payload?.research_failed_task_id === taskId) return { ok: true, task_id: taskId, record_id: recordId, duplicate: true };
  const current = gap ? lead.gap_task_id === taskId && lead.research_phase === 'gap_closure' : lead.task_id === taskId;
  if (!current) return { ok: true, task_id: taskId, record_id: recordId, ignored: 'superseded' };
  const status = str(command.payload, ['status']) ?? 'failed';
  const reason = status === 'cancelled' ? 'Recherche abgebrochen.' : failureText(command.payload);
  const researchStatus = await markLeadFailure(ctx, recordId, reason, taskId, { gap });
  return { ok: true, task_id: taskId, record_id: recordId, research_status: researchStatus };
}
