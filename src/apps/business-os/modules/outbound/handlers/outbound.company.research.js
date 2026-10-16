// ref: importer.rs:228 → outbound_company_research :315 — GEMISCHT.
// Strang OUT3: die Web-Recherche ist kein Core-Aufruf mehr (vorher Core-Fähigkeit research.person). Phase 1 (hier):
// Prüfungen und Ids wie Rust, Knowledge-Vertrag sicherstellen, Recherche als agent.task.enqueue mit Rückruf
// `outbound.company.research.done` (_lib/research-task.js). Phase 2 (Rückruf): Feldabbildung, Fit-Score,
// Knowledge-Zeile (applyCompanyResearch) – unverändert aus dem bisherigen Handler.
import { fail, isObject } from './_lib/outbound.js';
import { outboundRefs, ensureOutboundKnowledgeContract, firstNonempty, strPath, appendKnowledgeRows, appendRunStatus } from './_lib/knowledge.js';
import {
  requestedResearchFields, companyFieldKeys, domainFromUrl, mergeJsonObjects, applyCompanyResearchFields,
  evidenceFromResearch, countryIsGermany,
} from './_lib/research.js';
import { enqueueResearch } from './_lib/research-task.js';

/** Abgeleitete Eingaben wie Rust :315-350 (deterministisch; Phase 1 und Rückruf rechnen gleich). */
export function companyResearchInput(command) {
  if (command.app_id !== 'outbound') fail('invalid', 'outbound research is only implemented for outbound');
  const refs = outboundRefs(command);
  if (!refs.domain || !refs.companies_key) fail('invalid', 'outbound company research requires Knowledge companies table');
  const payload = command.payload ?? {};
  const company = payload.company ?? null;
  const recordId = command.record_id ?? payload.record_id;
  const companyId = firstNonempty([
    strPath(payload, ['writeback_contract', 'stable_id_value']),
    strPath(company, ['id']),
    strPath(command.client_context ?? {}, ['company_id']),
    typeof recordId === 'string' ? recordId : '',
  ]);
  let titleName = strPath(payload, ['title']);
  while (titleName.startsWith('Unternehmensdaten recherchieren:')) titleName = titleName.slice('Unternehmensdaten recherchieren:'.length);
  titleName = titleName.trim();
  const companyName = firstNonempty([strPath(company, ['name']), strPath(payload, ['research_request', 'company']), titleName]);
  const country = firstNonempty([strPath(company, ['country']), strPath(payload, ['research_request', 'country']), 'DE']);
  const website = strPath(company, ['website']);
  const existingDomain = firstNonempty([strPath(company, ['domain']), domainFromUrl(website)]);
  return { refs, payload, company, companyId, companyName, country, website, existingDomain };
}

export default async function handle(command, ctx) {
  const input = companyResearchInput(command);
  const { refs } = input;
  await ensureOutboundKnowledgeContract(ctx, command, refs.domain, refs.companies_key, refs.campaign_id, refs.campaign_name);
  if (!input.companyId) fail('invalid', 'outbound company research requires company_id');
  if (!input.companyName) fail('invalid', 'outbound company research requires company name');
  return enqueueResearch(ctx, command, {
    title: `Unternehmensdaten recherchieren: ${input.companyName}`,
    request: { company: input.companyName, country: input.country, mode: 'new_record', fields: companyFieldKeys(requestedResearchFields(command)) },
  });
}

/** Phase 2 – ref :350-400 (unverändert aus dem bisherigen Handler). */
export async function applyCompanyResearch(ctx, command, research) {
  const { refs, payload, company, companyId, companyName, country, website, existingDomain } = companyResearchInput(command);
  if (!companyId) fail('invalid', 'outbound company research requires company_id');
  if (!companyName) fail('invalid', 'outbound company research requires company name');
  const companyData = mergeJsonObjects(isObject(company) ? company.company_data : undefined, research);
  applyCompanyResearchFields(companyData, research);
  const fromMap = (key) => (typeof companyData[key] === 'string' ? companyData[key].trim() : '');
  const city = firstNonempty([fromMap('city'), fromMap('firma_ort'), strPath(company, ['city'])]);
  const domain = firstNonempty([fromMap('domain'), fromMap('firma_domain'), existingDomain]);
  const fitScore = countryIsGermany(country) ? 80 : domain ? 65 : 50;
  const now = ctx.now();
  const row = {
    company_id: companyId,
    campaign_id: refs.campaign_id,
    campaign_name: refs.campaign_name,
    company_name: companyName,
    website,
    domain,
    city,
    country,
    qualification_status: 'qualified',
    research_status: 'researched',
    pipeline_status: firstNonempty([strPath(company, ['pipeline_status']), 'not_started']),
    fit_score: fitScore,
    fit_status: 'fit',
    requested_fields_json: JSON.stringify(payload.research_request?.fields ?? []),
    custom_instruction: strPath(payload, ['research_request', 'custom_instruction']),
    company_data_json: JSON.stringify(companyData),
    evidence_json: JSON.stringify(evidenceFromResearch(research)),
    updated_at_ms: now,
  };
  await appendKnowledgeRows(ctx, refs.domain, refs.companies_key, [row]);
  await appendRunStatus(ctx, refs, command, 'company_research', 'completed', now);
  return { collection: 'knowledge_data_rows', definition_id: 'outbound.company.research.v1', record_ids: [companyId], records_count: 1 };
}
