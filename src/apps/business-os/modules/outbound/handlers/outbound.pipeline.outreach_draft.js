// outbound.pipeline.outreach_draft — Spalte „Anschreiben generieren“ (index.js generateOutreachForContact).
// Strang SKL-O (Kern §16.30/§16.34): bisher reiner Queue-Typ ohne Handler (V4-HANDLERS.md „Nicht-outbound.*“).
// Template prompts/outreach-draft.md (wörtlich aus index.js t('outreachDraftInstruction'), Rückschrieb-Satz auf die
// Antwort umgestellt), Skillbook aus der eigenen Collection outbound_skillbooks eingebettet (statt MCP
// business_os.get_record durch den Agenten), App-Skill outbound/message-drafting, Rückrufe
// outbound.pipeline.outreach_draft.done/.failed. payload {pipeline_id, contact_index, company_id?, drafting_request}.
// Idempotent je Befehl (idempotency_key, outreach_request_id am Kontakt).
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { asI64, fail, isObject, load, loadRequired, requiredFromPayloadOrRecord, str } from './_lib/outbound.js';
import { DEFAULT_SKILLBOOK } from './_lib/drafting.js';
import { messageDraftingSkillbook } from './_lib/skillbook-defaults.js';
import { DRAFT_DONE, DRAFT_FAILED, DRAFT_SKILL } from './_lib/outreach-draft.js';

const SKILLBOOK_FIELDS = ['mission', 'non_negotiable_rules', 'workflow_backbone', 'routing_taxonomy', 'stop_rules'];

function contactAt(item, contactIndex) {
  if (!Array.isArray(item.contacts)) fail('invalid', 'pipeline item has no contacts array');
  if (contactIndex >= item.contacts.length || !isObject(item.contacts[contactIndex])) fail('invalid', 'contact_index out of range');
  return item.contacts[contactIndex];
}

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const pipelineId = requiredFromPayloadOrRecord(command, ['pipeline_id', 'id'], 'pipeline_id is required');
  const contactIndex = asI64(payload.contact_index) ?? 0;
  if (contactIndex < 0) fail('invalid', 'contact_index must be >= 0');
  if (!isObject(payload.drafting_request)) fail('invalid', 'drafting_request object is required');
  const requestId = String(command.id ?? '');
  const item = await loadRequired(ctx, 'outbound_pipeline_items', pipelineId, 'pipeline item not found');
  const contact = contactAt(item, contactIndex);
  if (requestId && contact.outreach_request_id === requestId && typeof contact.outreach_task_id === 'string' && contact.outreach_task_id) {
    return { ok: true, applied: false, pipeline_id: pipelineId, contact_index: contactIndex, task_id: contact.outreach_task_id };
  }
  const stored = await load(ctx, 'outbound_skillbooks', DEFAULT_SKILLBOOK);
  const book = stored ?? messageDraftingSkillbook();
  const skillbook = Object.fromEntries(SKILLBOOK_FIELDS.filter((key) => book[key] !== undefined).map((key) => [key, book[key]]));
  const instruction = await renderPrompt(ctx, 'outreach-draft', {
    pipeline_id: pipelineId,
    contact_index: contactIndex,
    drafting_request: JSON.stringify(payload.drafting_request, null, 2),
    skillbook_id: DEFAULT_SKILLBOOK,
    skillbook: JSON.stringify(skillbook, null, 2),
  });
  const companyName = str(payload, ['drafting_request', 'company', 'name']) ?? str(item, ['company_name']) ?? pipelineId;
  const task = await ctx.cap.agent.task.enqueue({
    title: `Anschreiben generieren: ${companyName}`, // ref: index.js t('outreachDraftTitle')
    instruction: instruction.replace(/\n$/, ''),
    skill: DRAFT_SKILL,
    thread_key: `business-os/outbound/outreach/${pipelineId}/${contactIndex}`,
    payload: {
      pipeline_id: pipelineId,
      contact_index: contactIndex,
      ...(str(payload, ['company_id']) ? { company_id: payload.company_id } : {}),
      request_command_id: requestId,
    },
    idempotency_key: `outbound-outreach-draft-${requestId || `${pipelineId}-${contactIndex}`}`,
    on_complete: DRAFT_DONE,
    on_failed: DRAFT_FAILED,
  });
  const taskId = typeof task?.task_id === 'string' ? task.task_id.trim() : '';
  if (!taskId) fail('unavailable', 'agent task enqueue did not return task_id');
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const current = await loadRequired(tx, 'outbound_pipeline_items', pipelineId, 'pipeline item not found');
    const c = contactAt(current, contactIndex);
    c.outreach_generating = true;
    c.outreach_status = 'queued';
    c.outreach_task_id = taskId;
    c.outreach_request_id = requestId;
    delete c.outreach_error;
    current.updated_at_ms = now;
    await tx.put('outbound_pipeline_items', current);
    return { ok: true, applied: true, status: 'accepted', pipeline_id: pipelineId, contact_index: contactIndex, task_id: taskId };
  });
}
