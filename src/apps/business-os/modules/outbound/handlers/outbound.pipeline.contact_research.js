// ref: importer.rs:229 → outbound_contact_research :479 — GEMISCHT.
// Strang OUT3: die Web-Recherche ist kein Core-Aufruf mehr (vorher Core-Fähigkeit research.person). Phase 1 (hier):
// Prüfungen und Ids wie Rust, Knowledge-Vertrag, Recherche als agent.task.enqueue mit Rückruf
// `outbound.pipeline.contact_research.done` (_lib/research-task.js). Phase 2 (Rückruf): Kontakt-Gruppierung/
// Dedupe, Ids, DSGVO-Herkunft (:440-475), Knowledge-Zeilen (applyContactResearch) – unverändert.
import { fail } from './_lib/outbound.js';
import { outboundRefs, ensureOutboundKnowledgeContract, firstNonempty, strPath, appendKnowledgeRows, appendRunStatus } from './_lib/knowledge.js';
import {
  requestedContactFields, contactFieldKeys, contactsFromResearch, contactSubjectKey, OUTBOUND_CONTACT_LEGAL_BASIS,
  OUTBOUND_CONTACT_BASIS_EVIDENCE, OUTBOUND_CONTACT_PURPOSE, OUTBOUND_CONTACT_RETENTION_MS,
} from './_lib/research.js';
import { enqueueResearch } from './_lib/research-task.js';

const text = (value) => (typeof value === 'string' ? value : '');

/** Abgeleitete Eingaben wie Rust :479-520 (deterministisch; Phase 1 und Rückruf rechnen gleich). */
export function contactResearchInput(command) {
  if (command.app_id !== 'outbound') fail('invalid', 'outbound research is only implemented for outbound');
  const refs = outboundRefs(command);
  if (!refs.domain || !refs.contacts_key) fail('invalid', 'outbound contact research requires Knowledge contacts table');
  const payload = command.payload ?? {};
  const clientContext = command.client_context ?? {};
  const recordId = command.record_id ?? payload.record_id;
  const pipelineId = firstNonempty([
    strPath(payload, ['pipeline_id']),
    strPath(clientContext, ['pipeline_id']),
    strPath(payload, ['writeback_contract', 'stable_id_value']),
    typeof recordId === 'string' ? recordId : '',
  ]);
  const companyId = firstNonempty([
    strPath(payload, ['company_id']), strPath(clientContext, ['company_id']), strPath(payload, ['writeback_contract', 'company_id']),
  ]);
  let titleName = strPath(payload, ['title']);
  while (titleName.startsWith('Ansprechpartner recherchieren:')) titleName = titleName.slice('Ansprechpartner recherchieren:'.length);
  titleName = titleName.trim();
  const companyName = firstNonempty([strPath(payload, ['company_name']), titleName]);
  return { refs, payload, pipelineId, companyId, companyName };
}

export default async function handle(command, ctx) {
  const input = contactResearchInput(command);
  const { refs } = input;
  await ensureOutboundKnowledgeContract(ctx, command, refs.domain, refs.companies_key, refs.campaign_id, refs.campaign_name);
  if (!input.pipelineId) fail('invalid', 'outbound contact research requires pipeline_id');
  if (!input.companyName) fail('invalid', 'outbound contact research requires company name');
  return enqueueResearch(ctx, command, {
    title: `Ansprechpartner recherchieren: ${input.companyName}`,
    request: { company: input.companyName, country: 'DE', mode: 'update_person', fields: contactFieldKeys(requestedContactFields(command)) },
  });
}

/** Phase 2 – ref :520-590 (unverändert aus dem bisherigen Handler). */
export async function applyContactResearch(ctx, command, research) {
  const { refs, payload, pipelineId, companyId, companyName } = contactResearchInput(command);
  if (!pipelineId) fail('invalid', 'outbound contact research requires pipeline_id');
  if (!companyName) fail('invalid', 'outbound contact research requires company name');
  const contacts = contactsFromResearch(pipelineId, companyId, companyName, research);
  if (contacts.length === 0) fail('not_found', `ctox contact research found no public contacts for ${companyName}`);
  const now = ctx.now();
  const rows = contacts.map((contact) => ({
    pipeline_id: pipelineId,
    company_id: companyId,
    campaign_id: refs.campaign_id,
    campaign_name: refs.campaign_name,
    company_name: companyName,
    contact_id: text(contact.contact_id),
    contact_name: text(contact.contact_name),
    role: text(contact.role),
    email: text(contact.email),
    linkedin_url: text(contact.linkedin_url),
    contact_research_status: 'qualified',
    lead_status: 'open',
    stage: 'contact_qualified',
    contact_fields_json: JSON.stringify(payload.contact_fields ?? []),
    custom_instruction: strPath(payload, ['custom_instruction']),
    evidence_json: JSON.stringify(contact.evidence ?? []),
    updated_at_ms: now,
    legal_basis: OUTBOUND_CONTACT_LEGAL_BASIS,
    basis_evidence: OUTBOUND_CONTACT_BASIS_EVIDENCE,
    purpose: OUTBOUND_CONTACT_PURPOSE,
    granted_at_ms: now,
    expires_at_ms: now + OUTBOUND_CONTACT_RETENTION_MS,
    subject_key: contactSubjectKey(contact, companyName),
  }));
  await appendKnowledgeRows(ctx, refs.domain, refs.contacts_key, rows);
  await appendRunStatus(ctx, refs, command, 'contact_research', 'completed', now);
  return {
    collection: 'knowledge_data_rows',
    definition_id: 'outbound.pipeline.contact_research.v1',
    records_count: rows.length,
    record_ids: rows.map((row) => strPath(row, ['contact_id'])).filter(Boolean),
  };
}
