// ref: importer.rs:232 → outbound_lead_qualification :590 (deterministic, no agent / research call)
// Knowledge DataFrames are native: ctx.cap.knowledge.command(argv), see _lib/knowledge.js.
import { fail } from './_lib/outbound.js';
import {
  outboundRefs, ensureOutboundKnowledgeContract, firstNonempty, strPath, loadContactRowsForPipeline,
  appendKnowledgeRows, appendRunStatus,
} from './_lib/knowledge.js';

const LEAD_REASON = 'Öffentlich belegbarer Ansprechpartner mit Rolle/Profil für den Campaign Scope vorhanden.';

/** ref: importer.rs:3546 contact_row_is_qualifiable */
function isQualifiable(row) {
  return firstNonempty([
    strPath(row, ['contact_name']), strPath(row, ['role']), strPath(row, ['linkedin_url']), strPath(row, ['email']),
  ]) !== '';
}

export default async function handle(command, ctx) {
  // Rust: handle_outbound_research requires command.module == "outbound".
  if (command.app_id !== 'outbound') fail('invalid', 'outbound research is only implemented for outbound');
  const refs = outboundRefs(command);
  if (!refs.domain || !refs.contacts_key) fail('invalid', 'outbound lead qualification requires Knowledge contacts table');
  await ensureOutboundKnowledgeContract(ctx, command, refs.domain, refs.companies_key, refs.campaign_id, refs.campaign_name);
  const payload = command.payload ?? {};
  const recordId = command.record_id ?? payload.record_id;
  const pipelineId = firstNonempty([
    strPath(payload, ['pipeline_id']),
    strPath(command.client_context ?? {}, ['pipeline_id']),
    strPath(payload, ['writeback_contract', 'stable_id_value']),
    typeof recordId === 'string' ? recordId : '',
  ]);
  if (!pipelineId) fail('invalid', 'outbound lead qualification requires pipeline_id');
  const contactRows = await loadContactRowsForPipeline(ctx, refs, pipelineId);
  if (contactRows.length === 0) fail('not_found', `lead qualification requires contact rows in Knowledge for pipeline_id=${pipelineId}`);
  const now = ctx.now();
  const rows = contactRows.filter(isQualifiable).map((row) => ({
    ...(row && typeof row === 'object' && !Array.isArray(row) ? row : {}),
    lead_status: 'qualified',
    outreach_status: 'qualified',
    stage: 'lead_qualified',
    lead_score: 80,
    lead_reason: LEAD_REASON,
    updated_at_ms: now,
  }));
  if (rows.length === 0) fail('conflict', 'lead qualification found contacts, but none were qualifiable');
  await appendKnowledgeRows(ctx, refs.domain, refs.contacts_key, rows);
  await appendRunStatus(ctx, refs, command, 'lead_qualification', 'completed', now);
  return {
    collection: 'knowledge_data_rows',
    definition_id: 'outbound.pipeline.lead_qualification.v1',
    records_count: rows.length,
    record_ids: rows.map((row) => strPath(row, ['contact_id'])).filter(Boolean),
  };
}
