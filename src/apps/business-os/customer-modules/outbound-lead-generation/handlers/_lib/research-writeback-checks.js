// Vorprüfungen des Recherche-Writebacks — Teilschritt 4a des Ports von
// outbound.lead.research_writeback (Handler bleibt bis zum Gesamtport legacy).
//
// ref: person_research_gap_closure.rs (origin/main 4794af03d)
//      :1638 is_filler_field_status, :1673 derive_result_fields_from_field_status,
//      :1707 reject_evidence_free_writeback, :3072 validate_field_status_keys,
//      :3612 validate_result_shape (CRM-Abgleich als injiziertes `crm.check`, Port von
//      CrmBaseline :3170-3360 = Teilschritt 4b), :3715 validate_result_field_status_consistency,
//      :3971 research_value_is_populated
//      person_research_command.rs :2122 parse_requested_fields
// Input: request as returned by parseResearchWritebackRequest (field_status values
// {status, value, sources, attempts, reason, extra}).
import { fail, has, isObject } from './olg.js';
import { jsonEqual, rustDebugStrings } from './research-source-adapter.js';
import { cmpStr } from './update-digest.js';
import { fieldKeyFromStr } from './research-writeback-payload.js';

const get = (v, k) => (has(v, k) ? v[k] : undefined);
const asciiLower = (s) => String(s).replace(/[A-Z]/g, (c) => c.toLowerCase());
const isVerifiedLoose = (s) => asciiLower(String(s).trim()) === 'verified';
const nonBlankString = (v) => typeof v === 'string' && v.trim() !== '';

/** ref: :3971 */
export function researchValueIsPopulated(value) {
  if (typeof value === 'string') return value.trim() !== '';
  return typeof value === 'number' || typeof value === 'boolean';
}

/** ref: person_research_command.rs:2122 → canonical keys; first unknown field fails. */
export function parseRequestedFields(fields) {
  return fields.map((field) => {
    const key = fieldKeyFromStr(field);
    if (key == null) fail('invalid', `unsupported person-research field \`${field}\``);
    return key;
  });
}

/** ref: :1638 (operates on the serialised status value) */
export function isFillerFieldStatus(status) {
  const s = get(status, 'status');
  const unsupported = asciiLower((typeof s === 'string' ? s : '').trim()) === 'unsupported';
  const sources = get(status, 'sources');
  const attempts = get(status, 'attempts');
  const noSources = Array.isArray(sources) ? sources.length === 0 : true;
  const noAttempts = Array.isArray(attempts) ? attempts.length === 0 : true;
  const reason = get(status, 'reason');
  const placeholder = typeof reason === 'string'
    && ['test', 'testing', 'probe', 'dummy', 'n/a', 'na', '-', 'tbd', 'todo'].includes(asciiLower(reason.trim()));
  return (unsupported && noSources && noAttempts) || placeholder;
}

/** ref: :1673 — mutates request.result.fields when empty or not an object. */
export function deriveResultFieldsFromFieldStatus(request) {
  const current = request.result.fields;
  const empty = isObject(current) ? Object.keys(current).length === 0 : true;
  if (!empty) return;
  const fields = {};
  for (const [key, status] of Object.entries(request.field_status)) {
    if (!isVerifiedLoose(status.status) || status.value === null) continue;
    const entry = { value: JSON.parse(JSON.stringify(status.value)), sources: JSON.parse(JSON.stringify(status.sources)) };
    if (has(status.extra, 'person_key')) entry.person_key = status.extra.person_key;
    fields[key] = entry;
  }
  request.result.fields = fields;
}

/** ref: :1707 */
export function rejectEvidenceFreeWriteback(request) {
  const statuses = Object.values(request.field_status);
  const anyVerified = statuses.some((s) => isVerifiedLoose(s.status));
  const anyEvidence = statuses.some((s) => s.sources.length > 0 || s.attempts.length > 0) || request.result.evidence.length > 0;
  const people = isObject(request.result.person_field_status) ? Object.values(request.result.person_field_status) : [];
  const personStatuses = people.filter(isObject).flatMap((fields) => Object.values(fields));
  const anyPersonVerified = personStatuses.some((s) => typeof get(s, 'status') === 'string' && s.status.trim() === 'verified');
  const anyPersonEvidence = personStatuses.some((s) => ['sources', 'attempts'].some((k) => Array.isArray(get(s, k)) && s[k].length > 0));
  if (!(anyVerified || anyEvidence || anyPersonVerified || anyPersonEvidence)) {
    fail('invalid', 'evidence-free research writeback rejected: no field is verified and no field carries a source or a documented attempt; a `no_match` needs the documented search and page reads that led to it');
  }
}

/** ref: :3072 */
export function validateFieldStatusKeys(requestedFields, fieldStatus) {
  parseRequestedFields(requestedFields);
  const submitted = Object.keys(fieldStatus);
  parseRequestedFields(submitted);
  const requested = new Set(requestedFields);
  const submittedSet = new Set(submitted);
  const missing = [...requested].filter((f) => !submittedSet.has(f)).sort(cmpStr);
  const unexpected = [...submittedSet].filter((f) => !requested.has(f)).sort(cmpStr);
  if (missing.length || unexpected.length) {
    fail('invalid', `field_status must cover all requested_fields exactly; missing=${rustDebugStrings(missing)}, unexpected=${rustDebugStrings(unexpected)}`);
  }
}

/** ref: :3612 — `crm.check(field, url, quote) → true | false | null` (CrmBaseline, Teilschritt 4b). */
export function validateResultShape(result, requestedFields, crm) {
  const fields = result.fields;
  if (!isObject(fields)) fail('invalid', 'research writeback result.fields must be an object');
  const keys = Object.keys(fields);
  parseRequestedFields(keys);
  if (!keys.every((k) => requestedFields.includes(k))) fail('invalid', 'research writeback result.fields contains a field that was not requested');
  for (const [field, value] of Object.entries(fields)) {
    if (!isObject(value)) fail('invalid', `research writeback result.fields.${field} must be a structured object, not free text`);
    if (has(value, 'value')) {
      const fv = value.value;
      if (!(fv === null || researchValueIsPopulated(fv))) fail('invalid', `research writeback result.fields.${field}.value must be a populated scalar or null`);
      if (field.startsWith('person_') && researchValueIsPopulated(fv) && !nonBlankString(get(value, 'person_key'))) {
        fail('invalid', `research writeback result.fields.${field} requires person_key`);
      }
    }
  }
  result.person_records.forEach((person, index) => {
    if (!nonBlankString(get(person, 'person_key'))) fail('invalid', `research writeback result.person_records[${index}] requires person_key`);
  });
  result.evidence.forEach((evidence, index) => {
    const rawField = has(evidence, 'field_key') ? evidence.field_key : get(evidence, 'field');
    if (typeof rawField !== 'string') fail('invalid', `research writeback result.evidence[${index}] requires field_key`);
    const field = rawField;
    parseRequestedFields([field]);
    if (!requestedFields.includes(field)) fail('invalid', `research writeback result.evidence[${index}] field was not requested`);
    if (!nonBlankString(get(evidence, 'source_id'))) fail('invalid', `research writeback result.evidence[${index}] requires source_id`);
    const rawUrl = has(evidence, 'url') ? evidence.url : get(evidence, 'source_url');
    const sourceUrl = typeof rawUrl === 'string' && rawUrl.trim() !== '' ? rawUrl.trim() : null;
    if (sourceUrl == null) fail('invalid', `research writeback result.evidence[${index}] requires URL`);
    const quote = typeof get(evidence, 'quote') === 'string' ? evidence.quote : '';
    if (crm.check(field, sourceUrl, quote) !== true) {
      let parsed;
      try {
        parsed = new URL(sourceUrl);
      } catch {
        fail('invalid', `research writeback result.evidence[${index}] has invalid URL`);
      }
      if (!((parsed.protocol === 'http:' || parsed.protocol === 'https:') && parsed.hostname !== '')) {
        fail('invalid', `research writeback result.evidence[${index}] URL must be HTTP(S)`);
      }
    }
    if (!nonBlankString(get(evidence, 'quote'))) fail('invalid', `research writeback result.evidence[${index}] requires quote`);
    if (field.startsWith('person_') && !nonBlankString(get(evidence, 'person_key'))) {
      fail('invalid', `research writeback person evidence[${index}] requires person_key`);
    }
  });
}

/** ref: :3715 — note the exact (untrimmed) `status == "verified"` here. */
export function validateResultFieldStatusConsistency(result, fieldStatus) {
  const fields = result.fields;
  if (!isObject(fields)) fail('invalid', 'research writeback result.fields must be an object');
  for (const [field, status] of Object.entries(fieldStatus)) {
    const entry = has(fields, field) ? fields[field] : undefined;
    const resultValue = get(entry, 'value');
    if (status.status === 'verified') {
      if (resultValue !== undefined && resultValue !== null && !jsonEqual(resultValue, status.value)) {
        fail('invalid', `verified result field \`${field}\` value does not match field_status value`);
      }
      if (field.startsWith('person_')) {
        const pk = get(entry, 'person_key');
        const personKey = typeof pk === 'string' && pk.trim() !== '' ? pk.trim() : null;
        if (personKey == null) fail('invalid', `verified person result field \`${field}\` requires person_key`);
        if (!status.sources.every((s) => (typeof s.person_key === 'string' ? s.person_key.trim() : null) === personKey)) {
          fail('invalid', `verified person field \`${field}\` evidence person_key must match result.fields`);
        }
      }
    } else {
      if (researchValueIsPopulated(status.value)) fail('invalid', `non-verified field \`${field}\` must not carry a populated field_status value`);
      if (resultValue !== undefined && researchValueIsPopulated(resultValue)) {
        fail('invalid', `non-verified result field \`${field}\` must not carry a populated value`);
      }
    }
  }
}
