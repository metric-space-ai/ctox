// outbound.lead.research_writeback — Teilschritt 7: der Handler selbst.
//
// ref: person_research_gap_closure.rs:1745 handle_research_writeback (origin/main 4794af03d),
//      :2306 research_command_payload, :2324 research_command_requested_fields,
//      store.rs:12720 upsert_native_research_writeback_record (optimistische Prüfung gegen expected_master)
//
// Native Fähigkeiten (V4-PORT §14):
//   ctx.cap.command.get({command_id}) → {command_type, module, record_id, payload} | null   (business_commands)
//   ctx.cap.agent.task.get({task_id}) → {attempt, metadata, workspace_root_rel} | null           (§16.30 flach)
//   ctx.cap.agent.task.find({command_id}) → {task_id, attempt, …} | null                        (Aufgabe zum Befehl)
//   ctx.cap.agent.task.workspace_file({task_id, path}) → {canonical, is_file, len}           (Artefaktpfade)
//   ctx.cap.fence.claim({key}) → {token} | {token:null} (belegt; CAP-3 #831), ctx.cap.fence.release({token})               (ActiveResearchCommandGuard)
//   Feld-Status-Zeugen: App-Collection outbound_lead_generation_field_status_witnesses (_lib/field-status-witnesses.js),
//   ausgestellt in derselben ctx.transaction wie das Lead-Schreiben (Strang OLGSCR; vorher Core-Fähigkeit).
//   Kontaktprüfung: App-Befehl outbound.contacts.email_validate (_lib/contact-email-validation.js, Strang OLG3;
//   vorher Core-Fähigkeit mail.validate_contacts = spawn_contact_email_validation)
//
// OWNER-REGEL (THESEN): Belegregeln unverändert aus dem Rust-Stand (required_independent_sources = 1 je Feld).
import { fail, has, isObject } from './olg.js';
import { cmpStr, LEAD_COLLECTION } from './update-digest.js';
import { jsonEqual } from './research-source-adapter.js';
import { parseResearchWritebackRequest, serializeFieldStatus } from './research-writeback-payload.js';
import {
  deriveResultFieldsFromFieldStatus, rejectEvidenceFreeWriteback, validateFieldStatusKeys, validateResultShape,
  validateResultFieldStatusConsistency,
} from './research-writeback-checks.js';
import { CrmBaseline } from './research-validation.js';
import {
  GAP_METADATA_KEY, addFieldStatusEvidence, applyCustomFieldResults, sanitizeResearchWriteback, stampWritebackFieldRevisions,
  taskWorkspace, validateOriginalResearchCommand, validateTaskCorrelation, validateTerminalField,
} from './research-writeback-sanitize.js';
import { mergeJsonObjectValues, researchOutcomePatch } from './research-outcome-patch.js';
import { loadWitnesses, persistWritebackWitnesses, readCurrentWitnesses } from './field-status-witnesses.js';
import {
  alignLeadEmailValidation, applyPersonFieldStatus, invalidateChangedEmailAddresses, mergeFieldStatus, nativeReviewViewFromWitnesses,
  openPersonsForField, previousResearchKeys, projectPersonFieldStatus, researchFieldIsAnswered, restoreNativeEmailVerdicts,
  unionResearchKeys,
} from './research-field-status.js';
import { spawnContactEmailValidation } from './contact-email-validation.js';

const get = (v, k) => (has(v, k) ? v[k] : undefined);
const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));

/** BTreeMap<String, FieldStatus> → serde Value (sorted keys, declared members then flattened extra). */
export function fieldStatusValue(fieldStatus) {
  return Object.fromEntries(Object.keys(fieldStatus).sort(cmpStr).map((k) => [k, clone(serializeFieldStatus(fieldStatus[k]))]));
}

/** ref: :2324 — trimmed non-empty strings of payload.fields, null when absent/empty or command missing. */
function requestedFieldsOf(original) {
  if (original == null) return null;
  const fields = get(original.payload, 'fields');
  if (!Array.isArray(fields)) return null;
  const out = fields.filter((v) => typeof v === 'string').map((v) => v.trim()).filter((v) => v !== '');
  return out.length ? out : null;
}

function anyNeedsReview(statusMap, requested) {
  return Object.entries(isObject(statusMap) ? statusMap : {})
    .some(([field, status]) => requested.includes(field) && ['no_match', 'action_required'].includes(get(status, 'status')));
}

// ref: store.rs:12753-12815 upsert_native_field_status_if_current — CAS gegen expected_master, Lead und
// Zeugen (outbound_field_review.rs:218 persist, Issuance::Writeback) in EINER Transaktion.
async function persistWriteback(ctx, recordId, lead, expectedMaster, revision, issuedKeys) {
  await ctx.transaction(async (tx) => {
    const current = await tx.get(LEAD_COLLECTION, recordId);
    if (current == null) fail('not_found', 'native research persistence cannot create or resurrect a missing/deleted lead');
    if (isObject(current) && !has(current, 'id')) current.id = recordId;
    if (!jsonEqual(current, expectedMaster)) fail('conflict', 'native lead changed before persistence; retry from the current record');
    const previous = await loadWitnesses(tx, recordId);
    await tx.put(LEAD_COLLECTION, lead);
    await persistWritebackWitnesses(tx, recordId, lead, previous, revision, issuedKeys, ctx.now());
  });
}

/** ref: person_research_gap_closure.rs:1745 */
export async function handleResearchWriteback(command, ctx) {
  const request = parseResearchWritebackRequest(command.payload);
  // §16.21: v4-Befehle tragen `module` und `record_id` wie die nativen BusinessCommands.
  if (!(request.module === 'outbound-lead-generation' && command.module === request.module)) {
    fail('invalid', 'research writeback module must be outbound-lead-generation');
  }
  if (command.record_id !== request.record_id) fail('invalid', 'research writeback record_id does not match command record_id');
  const fence = await ctx.cap.fence.claim({ key: `outbound-research:${request.record_id}` });
  if (fence?.token == null) fail('conflict', 'another person research or research writeback is active for this record_id');
  try {
    return await writeback(command, ctx, request);
  } finally {
    await ctx.cap.fence.release({ token: fence.token });
  }
}

async function writeback(command, ctx, requestIn) {
  const request = requestIn;
  // CAP-3b #844 caps/directory.rs: command.get({command_id}) → {found, command_id, command_type, module,
  // record_id, status, payload} (nur Befehle der eigenen App); unbekannt → {found:false, command_id}.
  const lookup = await ctx.cap.command.get({ command_id: request.research_command_id });
  const original = lookup?.found === true ? lookup : null;
  validateOriginalResearchCommand(original, request);
  const lead = await ctx.get(LEAD_COLLECTION, request.record_id);
  if (lead == null) fail('not_found', 'research writeback lead record does not exist');
  const expectedMaster = clone(lead);

  let gapTask = null;
  if (request.gap_task_id.trim() !== '') {
    if (get(lead, 'research_phase') !== 'gap_closure') fail('invalid', 'lead is not in research_phase gap_closure');
    if (get(lead, 'gap_task_id') !== request.gap_task_id) fail('invalid', 'research writeback gap_task_id does not match lead');
    const task = await ctx.cap.agent.task.get({ task_id: request.gap_task_id });
    if (task == null) fail('invalid', 'research writeback gap task does not exist');
    if (!has(task.metadata, GAP_METADATA_KEY)) fail('invalid', 'gap task is missing person_research_gap_closure metadata');
    const contract = clone(task.metadata[GAP_METADATA_KEY]);
    validateTaskCorrelation(contract, request);
    gapTask = { task, contract };
  }

  let requestedFields;
  if (gapTask) {
    const list = get(gapTask.contract, 'requested_fields');
    if (!Array.isArray(list)) fail('invalid', 'gap task requested_fields array is missing');
    requestedFields = list.map((v) => (typeof v === 'string' ? v : fail('invalid', 'gap task requested_fields must contain strings')));
  } else {
    requestedFields = requestedFieldsOf(original) ?? Object.keys(request.field_status);
  }
  if (requestedFields.length === 0) fail('invalid', 'research writeback field_status must not be empty');

  deriveResultFieldsFromFieldStatus(request);
  rejectEvidenceFreeWriteback(request);
  const deliveredFields = Object.keys(request.field_status).sort(cmpStr);
  const researchPayload = original != null && original.payload !== undefined ? clone(original.payload) : null;
  const crm = CrmBaseline.fromResearchPayload(researchPayload);
  const rejections = sanitizeResearchWriteback(request, requestedFields, crm).filter((e) => !e.endsWith(': nicht geliefert'));
  validateFieldStatusKeys(requestedFields, request.field_status);
  validateResultShape(request.result, requestedFields, crm);
  validateResultFieldStatusConsistency(request.result, request.field_status);
  let terminal = null;
  if (gapTask) {
    taskWorkspace(gapTask.task, gapTask.contract);
    terminal = { ctx, taskId: request.gap_task_id, contract: gapTask.contract };
    for (const [field, status] of Object.entries(request.field_status)) {
      await validateTerminalField(ctx, request.gap_task_id, field, status, gapTask.contract);
    }
  }

  let knownPersonRecords;
  if (gapTask && has(gapTask.contract, 'known_person_records')) knownPersonRecords = clone(gapTask.contract.known_person_records);
  else if (Array.isArray(get(researchPayload, 'known_person_records'))) knownPersonRecords = clone(researchPayload.known_person_records);
  else knownPersonRecords = [];

  const now = ctx.now();
  let writebackId;
  if (typeof command.id === 'string') {
    if (command.id.trim() === '') fail('invalid', 'writeback command id is empty');
    writebackId = command.id;
  } else {
    writebackId = `native-writeback:${typeof ctx.uuid === 'function' ? ctx.uuid() : crypto.randomUUID()}`;
  }
  let nativeAttempt;
  if (gapTask) nativeAttempt = gapTask.task.attempt;
  else {
    // §16.30: agent.task.get kennt nur {task_id}; die Aufgabe zum Recherchebefehl liefert agent.task.find.
    const queued = await ctx.cap.agent.task.find({ command_id: request.research_command_id });
    nativeAttempt = queued == null ? null : queued.attempt;
  }
  const revision = { writeback_id: writebackId, command_id: request.research_command_id, attempt: nativeAttempt ?? null, written_at_ms: now };
  stampWritebackFieldRevisions(request, revision);
  const projectionResult = {
    fields: clone(request.result.fields),
    person_records: clone(request.result.person_records),
    evidence: clone(request.result.evidence),
    known_person_records: knownPersonRecords,
  };
  addFieldStatusEvidence(projectionResult, request.field_status);
  const previousKeys = previousResearchKeys(lead);
  const previousPersonStatuses = has(lead, 'person_field_status') ? clone(lead.person_field_status) : null;
  const previousContacts = has(lead, 'contacts') ? clone(lead.contacts) : null;
  const patch = researchOutcomePatch(lead, projectionResult, now);
  let next = mergeJsonObjectValues(lead, patch);
  unionResearchKeys(next, previousKeys);
  next.field_status = mergeFieldStatus(get(next, 'field_status'), fieldStatusValue(request.field_status));
  rejections.push(...applyCustomFieldResults(next, request.result.custom_fields, researchPayload, now));
  if (gapTask) taskWorkspace(gapTask.task, gapTask.contract);
  rejections.push(...await applyPersonFieldStatus(next, request.result.person_field_status, request.field_status, requestedFields, crm, terminal));
  projectPersonFieldStatus(next);
  invalidateChangedEmailAddresses(next, previousContacts);
  const restored = restoreNativeEmailVerdicts(next, previousPersonStatuses);
  const lpk = get(get(request.field_status, 'person_email_validation')?.extra, 'person_key');
  const leadLevelPerson = typeof lpk === 'string' ? lpk.trim() : null;
  for (const personKey of restored) {
    const delivered = get(get(request.result.person_field_status, personKey), 'person_email_validation') !== undefined
      || leadLevelPerson === personKey;
    if (delivered) {
      rejections.push(`person_field_status[${personKey}].person_email_validation: nicht gespeichert; der native experte.de-Befund fuer die aktuelle Adresse bleibt, weil die Lieferung diesen Befund nicht zitiert oder ihm widerspricht`);
    }
  }
  alignLeadEmailValidation(next);

  const witnesses = await readCurrentWitnesses(ctx, LEAD_COLLECTION, request.record_id);
  const reviewed = nativeReviewViewFromWitnesses(next, witnesses);
  const leadLevelAnswered = (field) => {
    const s = get(get(reviewed, 'field_status'), field);
    return s !== undefined && researchFieldIsAnswered(s);
  };
  const openPersonFields = requestedFields.flatMap((field) => (openPersonsForField(reviewed, field) ?? []).map((pk) => `${pk}:${field}`));
  const fieldAnswered = (field) => {
    const open = openPersonsForField(reviewed, field);
    return open != null ? open.length === 0 : leadLevelAnswered(field);
  };
  const openFields = requestedFields.filter((f) => !fieldAnswered(f));
  const acceptedFields = requestedFields.filter((f) => fieldAnswered(f));
  if (gapTask) next.gap_task_id = request.gap_task_id;
  next.research_phase = null;
  const personNeedsReview = Object.values(isObject(get(next, 'person_field_status')) ? next.person_field_status : {})
    .some((fields) => isObject(fields) && anyNeedsReview(fields, requestedFields));
  const needsReview = rejections.length > 0 || openFields.length > 0 || personNeedsReview || anyNeedsReview(get(next, 'field_status'), requestedFields);
  const terminalStatus = needsReview ? 'needs_review' : 'completed';
  next.research_status = terminalStatus;
  if (!isObject(next.payload)) next.payload = {};
  next.payload.native_research_terminal_status = terminalStatus;
  next.payload.native_research_requested_fields = requestedFields.slice();
  next.payload.native_research_rejections_count = rejections.length;
  next.payload.research_finished_at_ms = now;
  next.research_error = null;
  next.research_updated_at_ms = now;

  // issued_keys: BTreeSet<(scope, person, field)>
  const keys = [];
  for (const field of Object.keys(request.field_status)) keys.push(['lead', '', field]);
  if (isObject(request.result.person_field_status)) {
    for (const [person, fields] of Object.entries(request.result.person_field_status)) {
      for (const field of Object.keys(isObject(fields) ? fields : {})) {
        keys.push(['person', person, field], ['contact', person, field]);
      }
    }
  }
  const issuedKeys = [...new Map(keys.map((k) => [JSON.stringify(k), k])).values()]
    .sort((a, b) => cmpStr(a[0], b[0]) || cmpStr(a[1], b[1]) || cmpStr(a[2], b[2]));

  // Zeugen jetzt App-Collection: Ausstellung in derselben Transaktion wie Rust (keine ABWEICHUNG mehr).
  await persistWriteback(ctx, request.record_id, next, expectedMaster, revision, issuedKeys);
  await spawnContactEmailValidation(ctx, request.record_id);

  const delivered = new Set(deliveredFields);
  const fieldStatusOut = fieldStatusValue(Object.fromEntries(Object.entries(request.field_status).filter(([k]) => delivered.has(k))));
  return {
    ok: true,
    record_id: request.record_id,
    writeback_id: revision.writeback_id,
    research_command_id: request.research_command_id,
    gap_task_id: request.gap_task_id,
    research_status: needsReview ? 'needs_review' : 'completed',
    field_status: fieldStatusOut,
    accepted_fields: acceptedFields,
    delivered_fields: deliveredFields,
    open_fields: openFields,
    open_person_fields: openPersonFields,
    rejections,
    summary: `${acceptedFields.length} Feld(er) gespeichert, ${rejections.length} Beleg(e) verworfen, ${openFields.length} Feld(er) noch offen. Ein Feld gilt erst als beantwortet, wenn es verifiziert ist (eine passende externe Quelle, deren Zitat den Wert nennt; Sellify allein belegt nichts; widersprechen sich Quellen, bleibt das Feld action_required) oder als no_match belegt wurde. Offene Felder sind keine Ablehnung: hole die fehlende externe Quelle bzw. den Wert und sende sie gesammelt in einem weiteren Aufruf; gespeicherte Felder nicht erneut senden.`,
  };
}
