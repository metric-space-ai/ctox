// web_stack.person_research — Teilschritt 8: Lebenszyklus, Lead-Projektion und Lückenaufgabe in JS.
//
// ref: person_research_command.rs (origin/main 4794af03d)
//      :59 start, :271 spawn_worker, :402 persist_completed_person_research_outcome,
//      :463 project_outbound_lead_generation_lead_state, :528 outbound_lead_generation_lead_state_document,
//      :2005 outbound_lead_generation_lead_state_patch, :2036 outbound_lead_generation_writeback_record_id
//      person_research_gap_closure.rs :22 GAP_CLOSURE_SKILL, :655 build_gap_closure_prompt,
//      :755 enqueue_gap_closure_if_needed, :2121 cancel_open_gap_task_for_new_research,
//      :2163 load_gap_task_by_idempotency_key, :3927 phase_a_projection_evidence, :3940 phase_a_workspace
//
// Phase A als Agentenauftrag (Strang OLG4; vorher OLG3 zwei Aufrufe ctx.cap.browser.capture an die Core-Recherchemaschine
// web-stack, die nicht portiert wird):
//   web_stack.person_research          Prüfungen (person-research-capture.js) → Fence je Lead (ttl, hält bis zum Rückruf)
//                                      → Template prompts/person-research.md → agent.task.enqueue mit Paket-Skill
//                                      outbound-lead-generation/person-research, on_complete/on_failed eigene Befehle
//   web_stack.person_research.done     Rückruf: JSON-Block → Phase-A-Ergebnis → Sellify-Grundlinie :2430 → Zusammenfassung
//                                      → Lückenaufgabe :755 → Lead-Projektion :402/:463 (Auswertung wie vorher) → Fence frei
//   web_stack.person_research.failed   Rückruf (auch Abbruch status 'cancelled'): Lead failed (:2075) → Fence frei
//   Beide Rückrufe idempotent je task_id (Marke payload.person_research_callback_task_id am Lead) und wirkungslos, wenn
//   der Lead schon an einem neueren Auftrag hängt. have_data/update_inventory_general (:124 mode_skipped) laufen ohne Agent.
//   ctx.cap.agent.task.get / .find({idempotency_key}) / .enqueue / .set_metadata / .update / .cancel
//   ctx.cap.fence.claim({key, ttl_ms}) / .release; Kontaktprüfung = App-Befehl outbound.contacts.email_validate (Strang OLG3)
//
// recover_once (:106, Wiederaufnahme nach Daemon-Neustart) entfällt: der Kern stellt den Rückruf zu; ein verlorener Auftrag
// gibt den Fence nach FENCE_TTL_MS frei.
import { fail, has, isObject } from './olg.js';
import { LEAD_COLLECTION } from './update-digest.js';
import { independentResearchEvidenceCount, requiredIndependentSources } from './research-evidence.js';
import { researchValueIsPopulated } from './research-writeback-checks.js';
import {
  GAP_METADATA_KEY, TERMINAL_FIELD_STATUSES, canonicalRequestedFields, fieldDefinition, phaseAAttemptedSources, phaseAVerifiedSources,
  requiredContractString, safeWorkspaceSegment,
} from './research-writeback-sanitize.js';
import { mergeJsonObjectValues, researchOutcomePatch } from './research-outcome-patch.js';
import { jsonEqual } from './research-source-adapter.js';
import { sellifyLookup } from './sellify-lookup.js';
import { spawnContactEmailValidation } from './contact-email-validation.js';
import {
  MODE_SKIPPED, checkRuntimeSources, emptyPlanResponse, normalizePhaseAOutcome, parseResearchRequest, personResearchTemplateValues,
  personWorkspaceRoot, researchSummary,
} from './person-research-capture.js';
import { failureText, parseJsonBlock, replyText } from './agent-reply.js';
import { renderPrompt } from 'ctox:shared/prompt-template.js';

export const GAP_CLOSURE_SKILL = 'outbound-lead-generation/research'; // Paket-Skill (vorher System-Skill outbound-lead-generation-research)
export const RESEARCH_DONE = 'outbound.lead.research.done';
export const RESEARCH_FAILED = 'outbound.lead.research.failed';
export const PERSON_RESEARCH_SKILL = 'outbound-lead-generation/person-research'; // Paket-Skill (Strang OLG4)
export const PERSON_RESEARCH_DONE = 'web_stack.person_research.done';
export const PERSON_RESEARCH_FAILED = 'web_stack.person_research.failed';
export const PERSON_TASK_KEY = 'person_research_phase_a'; // App-Daten im Task-payload (Befehl, Prüfergebnis, Fence)
export const FENCE_TTL_MS = 3 * 60 * 60 * 1000; // Phase A läuft 30–90 min; danach gibt der Kern den Fence frei
const get = (v, k) => (has(v, k) ? v[k] : undefined);
const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));
const trimmed = (v) => (typeof v === 'string' && v.trim() !== '' ? v.trim() : null);
const pretty = (v) => JSON.stringify(v, null, 2);

/** ref: :2036 */
export function writebackRecordId(command) {
  if (typeof command.module !== 'string' || command.module.trim() !== 'outbound-lead-generation') return null;
  const recordId = trimmed(command.record_id);
  if (recordId == null) return null;
  const contract = get(command.payload, 'writeback_contract');
  if (contract === undefined) return null;
  const allowed = get(contract, 'collection') === LEAD_COLLECTION
    || (Array.isArray(get(contract, 'allowed_collections')) && contract.allowed_collections.some((c) => c === LEAD_COLLECTION));
  if (!allowed) return null;
  const ids = get(contract, 'record_ids');
  return Array.isArray(ids) && ids.some((c) => c === recordId) ? recordId : null;
}

/** ref: :2005 */
function leadStatePatch(commandId, status, error, now) {
  const payload = {
    last_research_command_id: commandId,
    native_research_terminal_status: status === 'running' ? null : status,
  };
  if (status === 'running') payload.research_started_at_ms = now;
  else {
    payload.research_finished_at_ms = now;
    payload.research_execution_phase = null;
  }
  if (error != null && error.trim() !== '') payload.research_error = error;
  else if (status !== 'running') payload.research_error = null;
  return payload;
}

/** ref: :528 */
function leadStateDocument(recordId, commandId, status, error, now) {
  const doc = { id: recordId, research_status: status, command_id: commandId, task_id: '', payload: leadStatePatch(commandId, status, error, now) };
  if (status === 'running') {
    doc.research_phase = null;
    doc.gap_task_id = null;
  } else {
    doc.research_error = error != null && error.trim() !== '' ? error : null;
    doc.research_updated_at_ms = now;
  }
  return doc;
}

/** store::upsert_rxdb_collection_record — merge into the existing document (or create it). */
async function upsertMerged(ctx, recordId, document) {
  await ctx.transaction(async (tx) => {
    const existing = await tx.get(LEAD_COLLECTION, recordId);
    const merged = existing == null ? clone(document) : mergeJsonObjectValues(existing, document);
    if (isObject(merged) && !has(merged, 'id')) merged.id = recordId;
    await tx.put(LEAD_COLLECTION, merged);
  });
}

/** ref: :463 */
export async function projectLeadState(ctx, command, status, error, result) {
  const recordId = writebackRecordId(command);
  if (recordId == null) return;
  const now = ctx.now();
  const commandId = typeof command.id === 'string' ? command.id : '';
  let doc = leadStateDocument(recordId, commandId, status, error, now);
  if (result != null) {
    const existing = (await ctx.get(LEAD_COLLECTION, recordId)) ?? { id: recordId };
    doc = mergeJsonObjectValues(doc, researchOutcomePatch(existing, result, now));
    const gapTaskId = trimmed(get(get(result, 'gap_closure'), 'task_id'));
    if (gapTaskId != null) {
      const gap = result.gap_closure;
      doc.research_status = 'running';
      doc.research_phase = 'gap_closure';
      doc.gap_task_id = gapTaskId;
      doc.payload.gap_requested_fields = has(gap, 'requested_fields') ? clone(gap.requested_fields) : null;
      doc.payload.gap_open_fields = has(gap, 'open_fields') ? clone(gap.open_fields) : null;
      doc.payload.gap_terminal_fields = has(gap, 'terminal_fields') ? clone(gap.terminal_fields) : null;
    } else if (status === 'completed') {
      doc.research_status = 'completed';
    }
  }
  await upsertMerged(ctx, recordId, doc);
  if (result != null) await spawnContactEmailValidation(ctx, recordId);
}


/**
 * ref: gap_closure.rs:655 — Prompttext wörtlich (prompts/gap-closure.md). RUST-EIGENHEIT: nennt „zwei unabhängige Hosts“, der Code
 * verlangt 1 (Owner-Regel). ABWEICHUNG SKL-C: Abschluss über die Schlussantwort (Callback outbound.lead.research.done) statt
 * `ctox business-os commands dispatch`.
 */
export async function buildGapClosurePrompt(ctx, contract) {
  const company = requiredContractString(contract, 'company');
  const recordId = requiredContractString(contract, 'record_id');
  const researchCommandId = requiredContractString(contract, 'research_command_id');
  const module = requiredContractString(contract, 'module');
  const openFields = get(contract, 'open_fields');
  if (!Array.isArray(openFields)) fail('invalid', 'gap closure contract open_fields array is required');
  const instructions = typeof get(contract, 'research_instructions') === 'string' ? contract.research_instructions : '';
  const priorities = pretty(has(contract, 'person_priorities') ? contract.person_priorities : []);
  const knownPeople = pretty(has(contract, 'known_person_records') ? contract.known_person_records : []);
  const attempted = pretty(has(contract, 'attempted_sources') ? contract.attempted_sources : []);
  const terminalFields = pretty(has(contract, 'terminal_fields') ? contract.terminal_fields : {});
  if (!has(contract, 'writeback_contract')) fail('invalid', 'gap closure writeback_contract is required');
  const writebackContract = pretty(contract.writeback_contract);
  const fieldLines = openFields.map((field) => {
    if (typeof field !== 'string') fail('invalid', 'gap closure open field names must be strings');
    return `- \`${field}\`: ${fieldDefinition(field)}`;
  });
  // Prompttext als Paket-Template prompts/gap-closure.md (Strang SKL-C); Rückweg-Zeilen auf den Callback umgestellt.
  return renderPrompt(ctx, 'gap-closure', {
    company, record_id: recordId, module, research_command_id: researchCommandId, field_lines: fieldLines.join('\n'),
    attempted_sources: attempted, terminal_fields: terminalFields, research_instructions: instructions,
    person_priorities: priorities, known_person_records: knownPeople, writeback_contract: writebackContract,
  });
}

/** ref: gap_closure.rs:3927 */
function phaseAProjectionEvidence(recordId, result) {
  const patch = researchOutcomePatch({ id: recordId, data: {}, contacts: [], evidence: [] }, result, 0);
  return Array.isArray(patch.evidence) ? patch.evidence : [];
}

/** ref: gap_closure.rs:2121 — `task` = cap.agent.task.get → {route_status_terminal, …}. */
export async function cancelOpenGapTaskForNewResearch(ctx, command) {
  if (typeof command.record_id !== 'string') return false;
  const lead = await ctx.get(LEAD_COLLECTION, command.record_id);
  if (lead == null || get(lead, 'research_phase') !== 'gap_closure') return false;
  const taskId = trimmed(get(lead, 'gap_task_id'));
  if (taskId == null) return false;
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  if (task == null || task.route_status_terminal === true) return false;
  await ctx.cap.agent.task.cancel({
    task_id: taskId,
    status_note: `Cancelled because a new person research command \`${typeof command.id === 'string' ? command.id : ''}\` superseded this gap closure.`,
  });
  return true;
}

/** ref: gap_closure.rs:755 — mutates `result` (gap_closure), returns the queued task view or null. */
export async function enqueueGapClosureIfNeeded(ctx, command, result) {
  const commandId = typeof command.id === 'string' ? command.id : fail('invalid', 'person-research command id is required for gap closure');
  const recordId = writebackRecordId(command);
  if (recordId == null) return null;
  const requestedFields = canonicalRequestedFields(command, result);
  const evidence = phaseAProjectionEvidence(recordId, result);
  const terminalFields = {};
  const openFields = [];
  for (const field of requestedFields) {
    const fieldResult = get(get(result, 'fields'), field);
    const populated = has(fieldResult, 'value') && researchValueIsPopulated(fieldResult.value);
    const sources = phaseAVerifiedSources(evidence, field);
    const independent = independentResearchEvidenceCount(sources, field);
    const personKeys = new Set(sources.map((s) => get(s, 'person_key')).filter((v) => typeof v === 'string').map((v) => v.trim()).filter((v) => v !== ''));
    const personComplete = !field.startsWith('person_')
      || (personKeys.size === 1 && sources.every((s) => typeof get(s, 'person_key') === 'string' && s.person_key.trim() !== ''));
    if (populated && independent >= requiredIndependentSources(field) && personComplete) {
      terminalFields[field] = { status: 'verified', value: clone(fieldResult.value), sources, attempts: [], independent_sources: independent };
    } else {
      openFields.push(field);
    }
  }
  if (openFields.length === 0) {
    result.gap_closure = { required: false, requested_fields: requestedFields, terminal_fields: terminalFields };
    return null;
  }
  const company = trimmed(get(command.payload, 'company')) ?? fail('invalid', 'gap closure requires company');
  // phase_a_workspace :3940 — relativ zum CTOX-Root; der Core legt das Verzeichnis beim Einreihen an.
  const workspaceRootRel = `runtime/research/person/${safeWorkspaceSegment(commandId)}`;
  const writebackContract = {
    command_type: 'outbound.lead.research_writeback',
    record_id: recordId,
    module: 'outbound-lead-generation',
    research_command_id: commandId,
    gap_task_id: '<current queue task id>',
    field_status: {
      statuses: TERMINAL_FIELD_STATUSES,
      verified: 'value plus at least two sources on different hosts',
      no_match: 'at least one web_search plus two web_read/browser_capture artifacts',
      unsupported: 'field cannot be researched by the available contract',
      action_required: 'login or approval with auth-assist reference',
    },
    result: { fields: {}, person_records: [], evidence: [] },
  };
  const p = command.payload;
  const contract = {
    lead_id: recordId,
    record_id: recordId,
    company,
    module: command.module,
    research_command_id: commandId,
    requested_fields: requestedFields,
    open_fields: openFields,
    terminal_fields: terminalFields,
    attempted_sources: phaseAAttemptedSources(result),
    research_instructions: has(p, 'research_instructions') ? clone(p.research_instructions) : '',
    known_person_records: has(p, 'known_person_records') ? clone(p.known_person_records) : [],
    person_priorities: has(p, 'person_priorities') ? clone(p.person_priorities) : [],
    writeback_contract: writebackContract,
  };
  const prompt = await buildGapClosurePrompt(ctx, contract);
  const idempotencyKey = `gap:${commandId}`;
  const title = `Lückenschluss: ${company}`;
  const threadKey = `person-research-gap/${recordId}`;
  let task = await ctx.cap.agent.task.find({ idempotency_key: idempotencyKey });
  if (task == null) {
    // §16.30 flach: instruction/skill/idempotency_key als eigene Felder; Workspace-Wurzel und Vertrag als App-Daten
    // im payload (der Kern besitzt den Workspace; der endgültige Vertrag folgt per set_metadata). Rückweg (SKL-C):
    // Callback outbound.lead.research.done → outbound.lead.research_writeback mit gap_task_id; Fehlschlag/Abbruch
    // → outbound.lead.research.failed.
    task = await ctx.cap.agent.task.enqueue({
      title, instruction: prompt, skill: GAP_CLOSURE_SKILL, thread_key: threadKey, priority: 'high',
      idempotency_key: idempotencyKey,
      payload: { workspace_root_rel: workspaceRootRel, [GAP_METADATA_KEY]: contract },
      on_complete: RESEARCH_DONE, on_failed: RESEARCH_FAILED,
    });
  }
  const taskId = task.task_id;
  await ctx.cap.agent.task.set_metadata({ task_id: taskId, key: 'business_os_command_id', value: commandId });
  const finalContract = clone(contract);
  finalContract.gap_task_id = taskId;
  finalContract.writeback_contract.gap_task_id = taskId;
  await ctx.cap.agent.task.set_metadata({ task_id: taskId, key: GAP_METADATA_KEY, value: finalContract });
  // §16.30 flach (Strang OLG4, wie enqueue): `instruction` statt `prompt`; der Arbeitsbereich steht schon im payload
  // (agent-sim UPDATE_KEYS: title, instruction, priority, payload, thread_key).
  task = await ctx.cap.agent.task.update({
    task_id: taskId, title, instruction: await buildGapClosurePrompt(ctx, finalContract), thread_key: threadKey, priority: 'high',
  });
  const lead = await ctx.get(LEAD_COLLECTION, recordId);
  if (lead != null) {
    lead.research_status = 'running';
    lead.research_phase = 'gap_closure';
    lead.gap_task_id = taskId;
    await upsertMerged(ctx, recordId, lead);
  }
  const meta = get(get(task, 'metadata'), GAP_METADATA_KEY);
  result.gap_closure = {
    required: true,
    owner_command_id: commandId,
    task_id: taskId,
    workspace_root: task.workspace_root_rel ?? get(task.payload, 'workspace_root_rel') ?? null, // ABWEICHUNG: relativer Pfad statt absolutem
    requested_fields: has(meta, 'requested_fields') ? clone(meta.requested_fields) : null,
    open_fields: has(meta, 'open_fields') ? clone(meta.open_fields) : null,
    terminal_fields: has(meta, 'terminal_fields') ? clone(meta.terminal_fields) : null,
  };
  return task;
}

const LEGAL_TOKENS = ['gmbh', 'mbh', 'ag', 'se', 'kg', 'kgaa', 'ohg', 'ug', 'co', 'cokg', 'ev', 'eg', 'inc', 'ltd', 'llc', 'sa',
  'srl', 'bv', 'nv'];

/** ref: :2511 */
export function sellifyFuzzyCompanyProbe(company) {
  const core = String(company).split(/\s+/u).filter((t) => t !== '').filter((token) => {
    const normalized = Array.from(token).filter((c) => /^[\p{Alphabetic}\p{N}]$/u.test(c)).join('').toLowerCase();
    return normalized !== '' && !LEGAL_TOKENS.includes(normalized);
  }).join(' ').trim();
  return new TextEncoder().encode(core).length >= 3 && core !== String(company).trim() ? core : null;
}

/** ref: :2535 — mutates `result`, returns the number of injected candidates. */
export function injectSellifyCandidates(result, record) {
  const data = isObject(get(record, 'data')) ? record.data : undefined;
  const recordField = (keys) => {
    for (const key of keys) {
      const v = has(record, key) ? record[key] : get(data, key);
      if (typeof v === 'string' && v.trim() !== '') return v.trim();
    }
    return null;
  };
  const mappings = [
    ['firma_name', recordField(['name', 'company_name'])],
    ['firma_domain', recordField(['website'])],
    ['firma_email', recordField(['email'])],
    ['firma_telefon', recordField(['phone'])],
    ['crm_record_number', recordField(['contact_id', 'id'])],
  ];
  const fields = get(result, 'fields');
  if (!isObject(fields)) return 0;
  let added = 0;
  for (const [fieldKey, value] of mappings) {
    if (value == null) continue;
    const fieldResult = get(fields, fieldKey);
    if (!isObject(fieldResult)) continue;
    const candidate = { value, confidence: 'high', source_id: 'sellify', source_url: null, tier: 'baseline', via: 'sellify_crm',
      note: 'Sellify CRM Stammdatensatz' };
    if (!has(fieldResult, 'candidates')) fieldResult.candidates = [];
    const candidates = fieldResult.candidates;
    if (!Array.isArray(candidates)) continue;
    if (candidates.some((e) => jsonEqual(get(e, 'value'), candidate.value) && jsonEqual(get(e, 'source_id'), candidate.source_id))) continue;
    candidates.push(clone(candidate));
    const currentMissing = !has(fieldResult, 'value') || fieldResult.value === null || get(fieldResult, 'confidence') === 'missing';
    if (currentMissing) {
      for (const key of ['value', 'confidence', 'source_id', 'source_url', 'tier', 'note']) fieldResult[key] = clone(candidate[key]);
    }
    added += 1;
  }
  if (added > 0 && Array.isArray(get(result, 'plan')) && !result.plan.some((e) => get(e, 'source_id') === 'sellify')) {
    result.plan.push({ source_id: 'sellify', tier: 'baseline', api_path: false, target_key: 'sellify_companies',
      target_fields: ['firma_name', 'firma_domain', 'firma_email', 'firma_telefon', 'crm_record_number'] });
  }
  return added;
}

/**
 * ref: :2430 merge_sellify_baseline_evidence — mutates `result`; a lookup failure never fails the research
 * (execute :2246 swallows it), the receipt carries only `error_code`.
 */
export async function mergeSellifyBaselineEvidence(ctx, result, companyIn) {
  const company = String(companyIn).trim();
  if (company === '') return 0;
  const lookupPayload = { entity: 'company', selectors: [{ field: 'name', value: company }, { field: 'company_name', value: company }], limit: 3 };
  const probe = sellifyFuzzyCompanyProbe(company);
  if (probe != null) lookupPayload.fuzzy_selectors = [{ field: 'name', value: probe }];
  const startedAtMs = ctx.now();
  let records = null;
  let failure = null;
  try {
    const lookup = await sellifyLookup(ctx, clone(lookupPayload));
    if (get(lookup, 'ok') !== true) throw new Error('Sellify lookup did not report success');
    if (!Array.isArray(get(lookup, 'records'))) throw new Error('Sellify lookup is missing its records array');
    records = lookup.records;
  } catch (error) {
    failure = error;
  }
  const receipt = {
    source_id: 'sellify', via: 'sellify_crm', collection: 'sellify_companies', company,
    country: has(result, 'country') ? clone(result.country) : null,
    query: lookupPayload, started_at_ms: startedAtMs, completed_at_ms: ctx.now(), classification: 'failed',
    returned_record_count: null, selected_record_id: null, evidence_count: 0,
  };
  let added = 0;
  if (failure == null) {
    receipt.returned_record_count = records.length;
    receipt.classification = records.length === 0 ? 'completed_empty' : 'succeeded';
    if (records.length > 0) {
      receipt.selected_record_id = has(records[0], 'id') ? clone(records[0].id) : null;
      added = injectSellifyCandidates(result, records[0]);
    }
    receipt.evidence_count = added;
  } else {
    receipt.error_code = 'sellify_lookup_failed';
    ctx.log('warn', 'sellify baseline evidence merge failed', { company });
  }
  result.sellify_lookup_runs = [receipt];
  return added;
}

/** Fehlerpfad: Lead auf failed projizieren, Projektionsfehler nur protokollieren (:2075). */
async function projectFailure(ctx, command, error) {
  try {
    await projectLeadState(ctx, command, 'failed', String(error?.message ?? error), null);
  } catch (projectionError) {
    ctx.log('error', 'person research lead projection failed', { command_id: command.id, error: String(projectionError?.message || projectionError) });
  }
}

/**
 * Auswertung des Phase-A-Ergebnisses (vorher im Handler nach den Erfassungen, ref :2246 Sellify, :2367 Zusammenfassung,
 * :755 Lückenaufgabe, :402/:463 Lead-Projektion). Wird vom Complete-Rückruf und vom mode_skipped-Pfad benutzt.
 */
export async function finishPhaseA(ctx, command, request, outcome) {
  let gapTask;
  try {
    outcome.research_instructions_len = request.research_instructions_len;
    outcome.workspace_root = personWorkspaceRoot(command.id);
    await mergeSellifyBaselineEvidence(ctx, outcome, get(command.payload, 'company'));
    outcome.summary = researchSummary(request.company, outcome);
    gapTask = await enqueueGapClosureIfNeeded(ctx, command, outcome);
  } catch (error) {
    await projectFailure(ctx, command, error); // person_research / person_research_gap_closure
    throw error;
  }
  const leadStatus = gapTask ? 'running' : 'completed';
  // :402 persist_completed_person_research_outcome
  if (typeof command.module === 'string' && command.module.trim() === 'outbound-lead-generation'
    && trimmed(command.record_id) != null && writebackRecordId(command) == null) {
    fail('invalid', 'missing or mismatched bounded lead writeback contract');
  }
  try {
    await projectLeadState(ctx, command, leadStatus, null, outcome);
  } catch (error) {
    await projectFailure(ctx, command, error); // person_research_writeback
    throw error;
  }
  return outcome;
}

/** Befehlsschnappschuss für den Rückruf (ohne client_context: der Eigentümer ist vor dem Auftrag geprüft). */
function commandSnapshot(command) {
  return { id: command.id, type: command.type ?? 'web_stack.person_research', module: command.module ?? null,
    record_id: command.record_id ?? null, payload: clone(command.payload) };
}

/** ref: :59 start + :271 spawn_worker — prüfen, Fence holen, Phase A als Agentenauftrag einreihen. */
export async function handlePersonResearch(command, ctx) {
  if (typeof command.id !== 'string') fail('invalid', 'person-research command id is required');
  const running = { ok: true, status: 'running', summary: 'Recherche wurde gestartet.' };
  const guardRecordId = trimmed(command.record_id) ?? command.id;
  const fence = await ctx.cap.fence.claim({ key: `outbound-research:${guardRecordId}`, ttl_ms: FENCE_TTL_MS });
  // RUST-EIGENHEIT: ohne Fence startet kein Worker; der Befehl endet mit dem „running“-Ergebnis.
  if (fence?.token == null) return { ...running, worker_started: false };
  let handedOver = false; // ab dem Auftrag gehört der Fence dem Rückruf
  try {
    try {
      await cancelOpenGapTaskForNewResearch(ctx, command);
      await projectLeadState(ctx, command, 'running', null, null);
    } catch (error) {
      await projectFailure(ctx, command, error); // :87 person_research_start
      throw error;
    }
    let request;
    try {
      const clientContext = has(command, 'client_context') ? command.client_context : null;
      request = parseResearchRequest(command.payload, clientContext);
      checkRuntimeSources(request);
    } catch (error) {
      await projectFailure(ctx, command, error);
      throw error;
    }
    if (MODE_SKIPPED.includes(request.mode)) {
      return await finishPhaseA(ctx, command, request, emptyPlanResponse(request, command.payload, 'mode_skipped'));
    }
    let task;
    try {
      const instruction = await renderPrompt(ctx, 'person-research', personResearchTemplateValues(command, request));
      const recordId = trimmed(command.record_id);
      task = await ctx.cap.agent.task.enqueue({
        title: `Personenrecherche: ${request.company}`,
        instruction,
        skill: PERSON_RESEARCH_SKILL,
        thread_key: `person-research/${recordId ?? safeWorkspaceSegment(command.id)}`,
        priority: 'high',
        idempotency_key: `person-research:${command.id}`,
        payload: { workspace_root_rel: personWorkspaceRoot(command.id), [PERSON_TASK_KEY]: {
          command: commandSnapshot(command), request: { ...request, runtime_sources: clone(request.runtime_sources) }, fence_token: fence.token,
        } },
        on_complete: PERSON_RESEARCH_DONE,
        on_failed: PERSON_RESEARCH_FAILED,
      });
      handedOver = true;
      const leadId = writebackRecordId(command);
      if (leadId != null) await upsertMerged(ctx, leadId, { id: leadId, task_id: task.task_id, payload: { person_research_task_id: task.task_id } });
    } catch (error) {
      await projectFailure(ctx, command, error);
      throw error;
    }
    return { ...running, worker_started: true, task_id: task.task_id };
  } finally {
    if (!handedOver) await ctx.cap.fence.release({ token: fence.token });
  }
}

/** Rückruf → Task → Auftrag (Befehlsschnappschuss, Prüfergebnis, Fence). */
async function personTaskBinding(ctx, command, type) {
  const taskId = trimmed(get(command.payload, 'task_id'));
  if (taskId == null) fail('invalid', `${type} is missing task_id`);
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  if (task == null) fail('not_found', `agent task ${taskId} does not exist`);
  const binding = get(task.payload, PERSON_TASK_KEY);
  if (!isObject(binding) || !isObject(binding.command) || typeof binding.command.id !== 'string' || !isObject(binding.request)) {
    fail('invalid', `agent task ${taskId} carries no person research binding`);
  }
  return { taskId, research: binding.command, request: binding.request, fenceToken: binding.fence_token ?? null };
}

/** Zustellung schon verarbeitet (Marke) bzw. Lead hängt an einem neueren Auftrag → keine Wirkung. */
async function callbackState(ctx, research, taskId) {
  const recordId = writebackRecordId(research);
  if (recordId == null) return { recordId, skip: null };
  const lead = await ctx.get(LEAD_COLLECTION, recordId);
  const payload = isObject(get(lead, 'payload')) ? lead.payload : {};
  if (payload.person_research_callback_task_id === taskId) return { recordId, skip: 'duplicate' };
  if (lead == null || payload.person_research_task_id !== taskId) return { recordId, skip: 'superseded' };
  return { recordId, skip: null };
}

const skipped = (taskId, recordId, skip) => (skip === 'duplicate'
  ? { ok: true, task_id: taskId, record_id: recordId, duplicate: true }
  : { ok: true, task_id: taskId, record_id: recordId, ignored: skip });

async function markCallback(ctx, recordId, taskId) {
  if (recordId != null) await upsertMerged(ctx, recordId, { id: recordId, payload: { person_research_callback_task_id: taskId } });
}

async function releaseFence(ctx, token) {
  if (token == null) return;
  try {
    await ctx.cap.fence.release({ token });
  } catch (error) { // stale_fence: TTL abgelaufen, ein neuerer Lauf hält den Schlüssel
    ctx.log('warn', 'person research fence release failed', { error: String(error?.message || error) });
  }
}

/** web_stack.person_research.done {task_id, status, result} */
export async function handlePersonResearchDone(command, ctx) {
  if (get(command.payload, 'status') !== 'completed') return handlePersonResearchFailed(command, ctx);
  const { taskId, research, request, fenceToken } = await personTaskBinding(ctx, command, PERSON_RESEARCH_DONE);
  const { recordId, skip } = await callbackState(ctx, research, taskId);
  try {
    if (skip != null) return skipped(taskId, recordId, skip);
    const result = command.payload.result;
    const accept = (v) => isObject(v) && isObject(get(v, 'fields'));
    const block = accept(result) ? result : parseJsonBlock(replyText(result), accept);
    if (block == null) {
      const reason = 'Die Personenrecherche endete ohne JSON-Ergebnisblock; nichts gespeichert.';
      await projectFailure(ctx, research, reason);
      await markCallback(ctx, recordId, taskId);
      return { ok: false, task_id: taskId, record_id: recordId, reason: 'no_result_block' };
    }
    let outcome;
    try {
      outcome = await finishPhaseA(ctx, research, request, normalizePhaseAOutcome(block, research, request));
    } catch (error) {
      // Der Rückruf kommt höchstens einmal: der Lead trägt den Fehler (finishPhaseA projiziert), die Zustellung ist verbucht.
      await markCallback(ctx, recordId, taskId);
      return { ok: false, task_id: taskId, record_id: recordId, error: error.message, code: error.code ?? null };
    }
    await markCallback(ctx, recordId, taskId);
    return { ...outcome, task_id: taskId };
  } finally {
    if (skip !== 'duplicate') await releaseFence(ctx, fenceToken);
  }
}

/** web_stack.person_research.failed {task_id, status: failed|blocked|cancelled, error?} */
export async function handlePersonResearchFailed(command, ctx) {
  const { taskId, research, fenceToken } = await personTaskBinding(ctx, command, PERSON_RESEARCH_FAILED);
  const { recordId, skip } = await callbackState(ctx, research, taskId);
  try {
    if (skip != null) return skipped(taskId, recordId, skip);
    const status = trimmed(get(command.payload, 'status')) ?? 'failed';
    const reason = status === 'cancelled' ? 'Personenrecherche abgebrochen.' : failureText(command.payload);
    await projectFailure(ctx, research, reason); // on_failed: Lead failed wie der Worker-Fehlerpfad :2075
    await markCallback(ctx, recordId, taskId);
    return { ok: true, task_id: taskId, record_id: recordId, research_status: 'failed', reason };
  } finally {
    if (skip !== 'duplicate') await releaseFence(ctx, fenceToken);
  }
}
