// Feldstatus-Zusammenführung, Personenprojektion, native Prüfansicht und E-Mail-Urteile —
// Teilschritt 6 des Ports von outbound.lead.research_writeback.
//
// ref: person_research_gap_closure.rs (origin/main 4794af03d)
//      :948 previous_research_keys, :975 union_research_keys, :1021 merge_field_status,
//      :1053 apply_person_field_status, :1196 sanitize_person_status, :1302 keeps_unconfirmed_address,
//      :1320 validate_person_status_basics, :1348 contact_person_keys, :1382 project_person_field_status,
//      :1441 CONTACT_VALUE_FIELDS, :1459 fill_empty_contact_values, :1497 open_persons_for_field,
//      :1534 research_field_is_answered
//      outbound_field_review.rs :290 native_review_view_from_witnesses, :406 valid_id, :583 is_refuted_no_match
//      contact_email_validation.rs :84 normalize_email, :108 contact_email, :118 invalidate_changed_email_addresses,
//      :223 contact_has_verdict, :231 emails_needing_validation, :249 parse_verdict_note, :532 email_receipt_entries,
//      :797 native_status_label, :818 cited_verdicts, :842 is_cited_email_correction,
//      :878 restore_native_email_verdicts, :1027 lead_person_key, :1066 align_lead_email_validation
//
// Die Zeugen der Feldstatus-Ausgabe liegen seit Strang OLGSCR in der App-Collection
// `outbound_lead_generation_field_status_witnesses` (field-status-witnesses.js) als
// `[{scope, person_key, field, status}]`; vorher SQLite `outbound_native_field_status_witnesses` im Core.
import { has, isObject, utf8Len } from './olg.js';
import { cmpStr } from './update-digest.js';
import { jsonEqual } from './research-source-adapter.js';
import { isFillerFieldStatus, researchValueIsPopulated } from './research-writeback-checks.js';
import { parseFieldStatusEntry, serializeFieldStatus } from './research-writeback-payload.js';
import { hostIsReservedForDocumentation, validateSource } from './research-validation.js';
import { CRM_SOURCE_SCHEME, crmAwareProviderCount, emailQuoteBacks, quoteBacksValue, requiredIndependentSources } from './research-evidence.js';
import { TERMINAL_FIELD_STATUSES, swapRemoveKey, validateTerminalField } from './research-writeback-sanitize.js';
import { isEmailVerdict } from './research-outcome-patch.js';

const get = (v, k) => (has(v, k) ? v[k] : undefined);
const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));
const asciiLower = (s) => String(s).replace(/[A-Z]/g, (c) => c.toLowerCase());
const trimmedKey = (v) => (typeof v === 'string' && v.trim() !== '' ? v.trim() : null);
const strOf = (v) => (typeof v === 'string' ? v : null);
const sortedObject = (map) => Object.fromEntries([...map.keys()].sort(cmpStr).map((k) => [k, map.get(k)]));

// ------------------------------------------------------------------ research key lists

const KEY_LISTS = ['researched_field_keys', 'verified_field_keys', 'unverified_field_keys'];

/** ref: :948 */
export function previousResearchKeys(lead) {
  const out = {};
  for (const name of KEY_LISTS) {
    const list = get(get(lead, 'payload'), name);
    out[name] = (Array.isArray(list) ? list : []).filter((v) => typeof v === 'string');
  }
  return out;
}

/** ref: :975 — mutates lead.payload. */
export function unionResearchKeys(lead, previous) {
  const payload = get(lead, 'payload');
  if (!isObject(payload)) return;
  const union = (name) => {
    const keys = (previous[name] ?? []).slice();
    const current = get(payload, name);
    for (const key of (Array.isArray(current) ? current : []).filter((v) => typeof v === 'string')) {
      if (!keys.includes(key)) keys.push(key);
    }
    return keys;
  };
  const researched = union('researched_field_keys');
  const verified = union('verified_field_keys');
  const unverified = union('unverified_field_keys').filter((k) => !verified.includes(k));
  payload.researched_field_keys = researched;
  payload.verified_field_keys = verified;
  payload.unverified_field_keys = unverified;
}

/** ref: :1021 */
export function mergeFieldStatus(existing, incoming) {
  const merged = isObject(existing) ? clone(existing) : {};
  if (isObject(incoming)) {
    for (const [field, status] of Object.entries(incoming)) {
      if (isFillerFieldStatus(status)) {
        const previous = get(merged, field);
        const previousIsInformative = previous !== undefined
          && asciiLower((strOf(get(previous, 'status')) ?? 'unsupported').trim()) !== 'unsupported';
        if (previousIsInformative) continue;
      }
      merged[field] = clone(status);
    }
  }
  return merged;
}

// ------------------------------------------------------------------ person statuses

/** ref: :1302 */
function keepsUnconfirmedAddress(field, status) {
  return field === 'person_email' && status.status === 'action_required'
    && !asciiLower(status.reason.trimStart()).startsWith('conflict')
    && typeof status.value === 'string' && status.value.trim().includes('@') && !/\s/u.test(status.value.trim());
}

/** Rust `url::Url::parse(..).host_str()` → `trim_start_matches("www.")` + ASCII lower. */
function sourceHost(url) {
  let parsed;
  try { parsed = new URL(String(url).trim()); } catch { return null; }
  if (parsed.hostname === '') return null;
  let host = parsed.hostname;
  while (host.startsWith('www.')) host = host.slice(4);
  return asciiLower(host);
}

/** ref: :1196 — returns {status, notes} or throws a plain message (Rust Err(String)). */
export function sanitizePersonStatus(field, personKey, status, crm) {
  const notes = [];
  if (status.status !== 'verified') {
    if (researchValueIsPopulated(status.value) && !keepsUnconfirmedAddress(field, status)) {
      notes.push('Wert auf nicht-verifiziertem Feld entfernt');
      status.value = null;
    }
    return { status, notes };
  }
  const value = typeof status.value === 'string' ? status.value.trim() : typeof status.value === 'number' ? String(status.value) : '';
  if (value === '') throw new Error('verified ohne Wert');
  const foreign = status.sources.find((s) => (typeof s.person_key === 'string' ? s.person_key.trim() : null) !== personKey);
  if (foreign !== undefined) {
    throw new Error(`person_key in Beleg (${foreign.person_key ?? 'fehlt'}) und Personenschluessel nicht identisch`);
  }
  const before = status.sources.length;
  status.sources = status.sources.filter((s) => crm.check(field, s.url, s.quote) !== false
    && quoteBacksValue(field, value, s.quote) && emailQuoteBacks(field, value, s.quote));
  if (status.sources.length !== before) {
    notes.push(`${before - status.sources.length} Beleg(e) verworfen (Zitat nennt den Wert oder die Adresse nicht, oder Sellify ohne Deckung)`);
  }
  if (!status.sources.some((s) => crm.check(field, s.url, s.quote) === true)) {
    const source = crm.agreeingSource(field, value, personKey);
    if (source) status.sources.push(source);
  }
  const hosts = new Set();
  for (const s of status.sources) {
    const c = crm.check(field, s.url, s.quote);
    if (c === true) hosts.add(CRM_SOURCE_SCHEME);
    else if (c === null) {
      const h = sourceHost(s.url);
      if (h != null) hosts.add(h);
    }
  }
  const sorted = [...hosts].sort(cmpStr);
  const reserved = sorted.find((h) => hostIsReservedForDocumentation(h));
  if (reserved !== undefined) throw new Error(`Beleg vom Dokumentations-Host \`${reserved}\` beweist nichts`);
  const needed = requiredIndependentSources(field);
  const independent = crmAwareProviderCount(sorted.length, sorted);
  if (independent === 0) {
    throw new Error(sorted.length === 0 ? 'verified ohne gueltigen Beleg' : 'Sellify allein belegt nichts: eine externe Quelle muss den Wert bestaetigen');
  }
  if (independent < needed) throw new Error(`verified verlangt ${needed} unabhaengige Quell-Hosts, gefunden: ${sorted.length}`);
  return { status, notes };
}

/** ref: :1320 */
export function validatePersonStatusBasics(field, status) {
  if (!TERMINAL_FIELD_STATUSES.includes(status.status)) {
    throw new Error(`field \`${field}\` has non-terminal or unsupported status \`${status.status}\``);
  }
  if (status.status === 'verified') {
    if (status.sources.length === 0) throw new Error(`verified field \`${field}\` needs a source`);
    for (const source of status.sources) validateSource(field, source);
  } else if (status.status === 'no_match' && status.reason.trim() === '') {
    throw new Error(`no_match field \`${field}\` needs a reason`);
  }
}

/**
 * ref: :1053 apply_person_field_status — mutates `lead`, returns the rejection lines.
 * `terminal` = null (chat assignment) or {ctx, taskId, contract} (gap task: validateTerminalField).
 */
export async function applyPersonFieldStatus(lead, incoming, leadLevel, requestedFields, crm, terminal) {
  const requested = new Set(requestedFields);
  const rejections = [];
  const perPerson = new Map(); // person → object (insertion order = serde Map)
  const take = async (personKey, field, statusIn) => {
    const label = `person_field_status[${personKey}].${field}`;
    if (!field.startsWith('person_')) {
      rejections.push(`${label}: nur person_*-Felder sind personengebunden`);
      return;
    }
    if (!requested.has(field)) {
      rejections.push(`${label}: nicht angefordert`);
      return;
    }
    let status;
    try {
      const out = sanitizePersonStatus(field, personKey, statusIn, crm);
      rejections.push(...out.notes.map((note) => `${label}: ${note}`));
      status = out.status;
    } catch (error) {
      rejections.push(`${label}: ${error.message}`);
      return;
    }
    try {
      if (terminal) await validateTerminalField(terminal.ctx, terminal.taskId, field, status, terminal.contract);
      else validatePersonStatusBasics(field, status);
    } catch (error) {
      rejections.push(`${label}: ${error.message}`);
      return;
    }
    const value = clone(serializeFieldStatus(status));
    value.person_key = personKey;
    if (!perPerson.has(personKey)) perPerson.set(personKey, {});
    perPerson.get(personKey)[field] = value;
  };
  if (isObject(incoming)) {
    for (const [rawKey, fields] of Object.entries(incoming)) {
      const personKey = rawKey.trim();
      if (!isObject(fields) || personKey === '') {
        rejections.push(`person_field_status[${personKey}]: erwartet ein Objekt je person_key`);
        continue;
      }
      for (const [field, raw] of Object.entries(fields)) {
        const status = parseFieldStatusEntry(clone(raw));
        if (status) await take(personKey, field, status);
        else rejections.push(`person_field_status[${personKey}].${field}: ohne lesbaren status`);
      }
    }
  }
  for (const [field, status] of Object.entries(leadLevel)) {
    const bound = trimmedKey(get(status.extra, 'person_key'));
    if (bound == null || !field.startsWith('person_')) continue;
    if (!(perPerson.has(bound) && has(perPerson.get(bound), field))) await take(bound, field, clone(status));
  }
  if (perPerson.size === 0) return rejections;
  if (!isObject(lead.person_field_status)) lead.person_field_status = {};
  for (const personKey of [...perPerson.keys()].sort(cmpStr)) {
    lead.person_field_status[personKey] = mergeFieldStatus(get(lead.person_field_status, personKey), perPerson.get(personKey));
  }
  return rejections;
}

/** ref: :1348 */
export function contactPersonKeys(contact) {
  const keys = [];
  for (const member of ['person_key', 'sellify_person_id', 'person_id']) {
    const v = get(contact, member);
    const raw = typeof v === 'string' ? v.trim() : typeof v === 'number' ? String(v) : null;
    if (raw == null || raw === '') continue;
    const candidates = /^[0-9]+$/.test(raw) ? [`sellify-person-${raw}`, raw] : [raw];
    for (const key of candidates) if (!keys.includes(key)) keys.push(key);
  }
  return keys;
}

const CONTACT_VALUE_FIELDS = ['person_email', 'person_telefon', 'person_linkedin', 'person_xing', 'person_position',
  'person_funktion', 'person_titel', 'person_geschlecht'];

/** ref: :1459 */
function fillEmptyContactValues(contact, statuses) {
  for (const field of CONTACT_VALUE_FIELDS) {
    const status = get(statuses, field);
    if (status === undefined) continue;
    const s = get(status, 'status');
    if (!(s === 'verified' || (field === 'person_email' && s === 'action_required'))) continue;
    const value = trimmedKey(get(status, 'value'));
    if (value == null) continue;
    const current = (strOf(get(contact, field)) ?? '').trim();
    if (current === '') contact[field] = value;
  }
}

/** ref: :1382 — mutates lead.contacts. */
export function projectPersonFieldStatus(lead) {
  const stored = get(lead, 'person_field_status');
  if (!isObject(stored)) return;
  const contacts = get(lead, 'contacts');
  if (!Array.isArray(contacts)) return;
  const canonical = contacts.map((c) => trimmedKey(get(c, 'person_key')));
  const claimed = new Set(canonical.filter((k) => k != null));
  const aliasOwners = new Map();
  contacts.forEach((contact, index) => {
    for (const key of contactPersonKeys(contact)) {
      if (canonical[index] !== key) aliasOwners.set(key, (aliasOwners.get(key) ?? 0) + 1);
    }
  });
  contacts.forEach((contact, index) => {
    const combined = {};
    for (const key of contactPersonKeys(contact)) {
      if (canonical[index] === key || claimed.has(key) || (aliasOwners.get(key) ?? 0) !== 1) continue;
      if (isObject(get(stored, key))) Object.assign(combined, clone(stored[key]));
    }
    if (canonical[index] != null && isObject(get(stored, canonical[index]))) Object.assign(combined, clone(stored[canonical[index]]));
    if (Object.keys(combined).length === 0 || !isObject(contact)) return;
    fillEmptyContactValues(contact, combined);
    contact.field_status = mergeFieldStatus(get(contact, 'field_status'), combined);
  });
}

// ------------------------------------------------------------------ native review view

const REVIEW_SCHEMA = 'ctox.outbound.field_review.v1';

/** ref: outbound_field_review.rs:406 */
export function validId(value) {
  return typeof value === 'string' && value !== '' && utf8Len(value) <= 256 && value.trim() === value && !/\p{Cc}/u.test(value);
}

/** ref: outbound_field_review.rs:583 */
export function isRefutedNoMatch(status) {
  const revision = get(status, 'revision');
  const writeback = strOf(get(revision, 'writeback_id'));
  const command = strOf(get(revision, 'command_id'));
  if (!validId(writeback) || !validId(command) || !has(status, 'review')) return false;
  const review = status.review;
  const r = (k) => get(review, k);
  const attempt = r('attempt');
  const reviewedAt = r('reviewed_at_ms');
  if (get(status, 'status') !== 'no_match' || r('schema') !== REVIEW_SCHEMA || r('verdict') !== 'refuted'
    || r('claim_status') !== 'no_match' || strOf(get(r('revision_ref'), 'writeback_id')) !== writeback
    || r('command_id') !== command || r('reason_code') !== 'contradicted_by_saved_source'
    || !validId(strOf(r('review_attempt_id')))
    || !(Number.isInteger(attempt) && attempt >= 0) || !(Number.isInteger(reviewedAt) && reviewedAt > 0)) return false;
  const pk = get(status, 'person_key');
  if (typeof pk === 'string') return validId(pk) && r('person_key') === pk;
  if (pk === undefined || pk === null) return r('person_key') === undefined || r('person_key') === null;
  return false;
}

/**
 * ref: outbound_field_review.rs:290 native_review_view_from_witnesses. `witnesses` =
 * [{scope, person_key, field, status}] from field-status-witnesses.js readCurrentWitnesses.
 * ABWEICHUNG (nur Ansicht): Rust-IndexMut legt fehlende `field_status`-Schlüssel als null an; die
 * Ansicht wird nur gelesen, daher entfällt das.
 */
export function nativeReviewViewFromWitnesses(lead, witnesses) {
  const index = new Map((witnesses ?? []).map((w) => [`${w.scope}\u0000${w.person_key}\u0000${w.field}`, w.status]));
  const contains = (scope, person, field, status) => {
    const key = `${scope}\u0000${person}\u0000${field}`;
    return index.has(key) && jsonEqual(index.get(key), status);
  };
  const view = clone(lead);
  const retain = (scope, person, fields) => {
    if (!isObject(fields)) return;
    for (const [field, status] of Object.entries(fields)) {
      if (has(status, 'review') && status.review !== null && !contains(scope, person, field, status)) status.review = null;
    }
  };
  retain('lead', '', get(view, 'field_status'));
  if (isObject(get(view, 'person_field_status'))) {
    for (const [person, fields] of Object.entries(view.person_field_status)) retain('person', person, fields);
  }
  const counts = new Map();
  for (const contact of Array.isArray(get(lead, 'contacts')) ? lead.contacts : []) {
    const person = strOf(get(contact, 'person_key'));
    if (person != null) counts.set(person, (counts.get(person) ?? 0) + 1);
  }
  for (const contact of Array.isArray(get(view, 'contacts')) ? view.contacts : []) {
    const person = strOf(get(contact, 'person_key')) ?? '';
    if (counts.get(person) === 1) retain('contact', person, get(contact, 'field_status'));
    else if (isObject(get(contact, 'field_status'))) {
      for (const status of Object.values(contact.field_status)) if (isObject(status)) status.review = null;
    }
  }
  return view;
}

/** ref: :1534 */
export function researchFieldIsAnswered(status) {
  if (isRefutedNoMatch(status)) return false;
  const s = (strOf(get(status, 'status')) ?? '').trim();
  return s === 'verified' || s === 'no_match';
}

/** ref: :1497 — null when the lead holds no keyed contacts. */
export function openPersonsForField(lead, field) {
  if (!field.startsWith('person_')) return null;
  const contacts = get(lead, 'contacts');
  if (!Array.isArray(contacts)) return null;
  const persons = contacts.map((c) => [contactPersonKeys(c)[0], c]).filter(([k]) => k !== undefined);
  if (persons.length === 0) return null;
  const open = new Set(persons.filter(([, c]) => {
    const s = get(get(c, 'field_status'), field);
    return !(s !== undefined && researchFieldIsAnswered(s));
  }).map(([k]) => k));
  return [...open].sort(cmpStr);
}

// ------------------------------------------------------------------ contact e-mail verdicts

/** ref: contact_email_validation.rs:84 */
export function normalizeEmail(value) {
  const v = asciiLower(String(value).trim().replace(/^[<>]+|[<>]+$/g, ''));
  const at = v.indexOf('@');
  if (at < 0) return null;
  const local = v.slice(0, at);
  const host = v.slice(at + 1);
  if (local === '' || !host.includes('.') || /\s/u.test(v)) return null;
  if (['example.com', 'example.org', 'example.net', 'example.de', 'example.edu'].includes(host)
    || host.endsWith('.example') || host.endsWith('.invalid')) return null;
  return v;
}

/** ref: :108 */
export function contactEmail(contact) {
  for (const key of ['person_email', 'email']) {
    const v = get(contact, key);
    if (typeof v !== 'string') continue;
    const n = normalizeEmail(v);
    if (n != null) return n;
  }
  return null;
}

/**
 * ref: :118 — returns the number of changed contacts.
 * RUST-EIGENHEIT: `lead["person_field_status"].get_mut(..)` legt per IndexMut einen fehlenden
 * `person_field_status` als null an, sobald eine Adresse wechselte – treu nachgebildet.
 */
export function invalidateChangedEmailAddresses(lead, previousContacts) {
  const old = new Map();
  const duplicate = new Set();
  for (const contact of Array.isArray(previousContacts) ? previousContacts : []) {
    const key = trimmedKey(get(contact, 'person_key'));
    if (key == null) continue;
    if (old.has(key)) duplicate.add(key);
    old.set(key, contactEmail(contact));
  }
  const contacts = get(lead, 'contacts');
  if (!Array.isArray(contacts)) return 0;
  const changed = [];
  for (const contact of contacts) {
    const key = trimmedKey(get(contact, 'person_key'));
    if (key == null || duplicate.has(key)) continue;
    const oldEmail = old.get(key);
    if (oldEmail == null) continue;
    if (contactEmail(contact) === oldEmail) continue;
    if (isObject(contact)) {
      swapRemoveKey(contact, 'person_email_validation');
      swapRemoveKey(contact, 'email_validation');
      if (isObject(get(contact, 'field_status'))) swapRemoveKey(contact.field_status, 'person_email_validation');
    }
    changed.push(key);
  }
  if (changed.length === 0) return 0;
  if (isObject(lead) && !has(lead, 'person_field_status')) lead.person_field_status = null;
  for (const key of changed) {
    const fields = get(get(lead, 'person_field_status'), key);
    if (isObject(fields)) swapRemoveKey(fields, 'person_email_validation');
  }
  if (get(get(lead, 'field_status'), 'person_email_validation') !== undefined) {
    lead.field_status.person_email_validation = {
      status: 'action_required', value: null, sources: [], attempts: [],
      reason: 'E-Mail-Adresse geaendert; erneute native Pruefung erforderlich',
    };
  }
  return changed.length;
}

function contactHasVerdict(contact) {
  return ['person_email_validation', 'email_validation'].some((k) => typeof get(contact, k) === 'string' && isEmailVerdict(contact[k]));
}

/** ref: :231 */
export function emailsNeedingValidation(lead, limit) {
  const seen = new Set();
  const out = [];
  for (const contact of Array.isArray(get(lead, 'contacts')) ? lead.contacts : []) {
    if (out.length >= limit) break;
    if (contactHasVerdict(contact)) continue;
    const email = contactEmail(contact);
    if (email == null || seen.has(email)) continue;
    seen.add(email);
    out.push(email);
  }
  return out;
}

/** ref: :249 — [email, 'valid'|'invalid'|'unknown_catch_all'] or null. */
export function parseVerdictNote(note) {
  const i = note.indexOf('verdict:');
  if (i < 0) return null;
  const rest = note.slice(i + 'verdict:'.length);
  const bar = rest.indexOf('|');
  if (bar < 0) return null;
  const email = normalizeEmail(rest.slice(0, bar));
  if (email == null) return null;
  const label = rest.slice(bar + 1).trim().toLowerCase();
  if (['unbekannt', 'unknown', 'catch-all', 'catch all'].some((w) => label.includes(w))) return [email, 'unknown_catch_all'];
  if (['ungültig', 'ungueltig', 'invalid', 'unzustellbar', 'nicht'].some((w) => label.includes(w))) return [email, 'invalid'];
  if (['gültig', 'gueltig', 'valid', 'zustellbar'].some((w) => label.includes(w))) return [email, 'valid'];
  return null;
}

/** ref: :532 */
export function emailReceiptEntries(lead) {
  const history = get(get(lead, 'payload'), 'email_validation_receipts');
  if (isObject(history) && Object.keys(history).length > 0) return Object.values(history);
  const checked = get(get(get(lead, 'payload'), 'email_validation_pass'), 'checked');
  return Array.isArray(checked) ? checked : [];
}

const NATIVE_VIA = ['native_email_validation', 'native_email_validation_receipt'];

/** ref: :797 */
function nativeStatusLabel(status) {
  if (!NATIVE_VIA.includes(get(status, 'via'))) return null;
  const s = get(status, 'status');
  const v = get(status, 'value');
  if (s === 'verified' && v === 'valid') return 'valid';
  if (s === 'verified' && v === 'invalid') return 'invalid';
  if (s === 'action_required' && get(status, 'verdict') === 'unknown_catch_all') return 'unknown_catch_all';
  return null;
}

/** ref: :818 */
function citedVerdicts(status) {
  const sources = get(status, 'sources');
  const attempts = get(status, 'attempts');
  const quotes = (Array.isArray(sources) ? sources : []).map((s) => get(s, 'quote')).filter((q) => typeof q === 'string');
  const results = (Array.isArray(attempts) ? attempts : []).map((a) => get(a, 'result')).filter((r) => typeof r === 'string');
  return [...quotes, ...results].map(parseVerdictNote).filter((v) => v != null);
}

/** ref: :842 */
function isCitedEmailCorrection(status, email) {
  if (NATIVE_VIA.includes(get(status, 'via'))) return false;
  const cited = citedVerdicts(status).filter(([address]) => address === email).map(([, kind]) => kind);
  if (cited.length === 0) return false;
  const s = strOf(get(status, 'status'));
  if (s == null) return false;
  if (s.trim() === 'verified') {
    const claimed = get(status, 'value');
    if (claimed !== 'valid' && claimed !== 'invalid') return false;
    return cited.every((kind) => kind === claimed);
  }
  return true;
}

/** ref: :878 — mutates `lead`, returns the person keys whose native verdict was put back. */
export function restoreNativeEmailVerdicts(lead, previous) {
  if (!isObject(previous)) return [];
  const checked = emailReceiptEntries(lead);
  const current = has(lead, 'person_field_status') ? clone(lead.person_field_status) : null;
  const leadLevelStatus = get(get(lead, 'field_status'), 'person_email_validation');
  const contactsAll = Array.isArray(get(lead, 'contacts')) ? lead.contacts : null;
  const leadLevelCorrection = leadLevelStatus !== undefined
    && (contactsAll ?? []).map(contactEmail).filter((e) => e != null).some((e) => isCitedEmailCorrection(leadLevelStatus, e));
  if (contactsAll == null) return [];
  const restored = [];
  for (const contact of contactsAll) {
    const personKey = trimmedKey(get(contact, 'person_key'));
    if (personKey == null) continue;
    const email = contactEmail(contact);
    if (email == null) continue;
    const status = get(get(previous, personKey), 'person_email_validation');
    if (status === undefined) continue;
    const label = nativeStatusLabel(status);
    if (label == null) continue;
    const runId = strOf(get(status, 'run_id'));
    const validated = strOf(get(status, 'validated_email'));
    if (runId == null || runId === '' || get(status, 'person_key') !== personKey
      || (validated == null ? null : normalizeEmail(validated)) !== email) continue;
    const delivered = get(get(current, personKey), 'person_email_validation');
    if (delivered !== undefined && isCitedEmailCorrection(delivered, email)) continue;
    const valid = label === 'valid';
    if (!checked.some((entry) => get(entry, 'email') === email && get(entry, 'run_id') === runId && get(entry, 'valid') === valid
      && (typeof get(entry, 'verdict') !== 'string' || entry.verdict === label))) continue;
    if (!isObject(get(contact, 'field_status'))) contact.field_status = {};
    contact.field_status.person_email_validation = clone(status);
    contact.person_email_validation = label;
    restored.push([personKey, clone(status)]);
  }
  if (restored.length === 0) return [];
  const unboundValidAddress = contactsAll.some((contact) => get(contact, 'person_email_validation') === 'valid'
    && !restored.some(([key, status]) => get(contact, 'person_key') === key && contactEmail(contact) === strOf(get(status, 'validated_email'))));
  if (!isObject(get(lead, 'person_field_status'))) lead.person_field_status = {};
  for (const [personKey, status] of restored) {
    if (!isObject(get(lead.person_field_status, personKey))) lead.person_field_status[personKey] = {};
    lead.person_field_status[personKey].person_email_validation = clone(status);
  }
  const best = (restored.find(([, s]) => get(s, 'value') === 'valid') ?? restored[0])[1];
  if (!isObject(get(lead, 'field_status'))) lead.field_status = {};
  if (leadLevelCorrection) {
    // The writeback's own lead-level reading of a cited verdict stays.
  } else if (get(best, 'value') === 'invalid' && unboundValidAddress) {
    lead.field_status.person_email_validation = {
      status: 'action_required', value: null, sources: [], attempts: [],
      reason: 'Ein weiteres gueltiges Kontakturteil hat keinen gebundenen nativen Prueflauf',
    };
  } else {
    lead.field_status.person_email_validation = clone(best);
  }
  return restored.map(([key]) => key);
}

/** ref: :1027 */
function leadPersonKey(lead) {
  const contactKeys = new Set((Array.isArray(get(lead, 'contacts')) ? lead.contacts : [])
    .map((c) => get(c, 'person_key')).filter((k) => typeof k === 'string').map((k) => k.trim()).filter((k) => k !== ''));
  const statuses = get(lead, 'field_status');
  if (!isObject(statuses)) return null;
  const bound = (status) => trimmedKey(get(status, 'person_key'));
  if (has(statuses, 'person_email')) {
    const key = bound(statuses.person_email);
    if (key != null) return contactKeys.has(key) ? key : null;
  }
  const keys = new Set(Object.entries(statuses)
    .filter(([field]) => field.startsWith('person_') && field !== 'person_email_validation')
    .map(([, s]) => bound(s)).filter((k) => k != null));
  if (keys.size === 1) {
    const [key] = keys;
    return contactKeys.has(key) ? key : null;
  }
  return null;
}

/** ref: :1066 — returns whether the lead-level field changed. */
export function alignLeadEmailValidation(lead) {
  const personKey = leadPersonKey(lead);
  if (personKey == null) return false;
  let own = get(get(get(lead, 'person_field_status'), personKey), 'person_email_validation');
  if (own === undefined) {
    const contact = (Array.isArray(get(lead, 'contacts')) ? lead.contacts : [])
      .find((c) => typeof get(c, 'person_key') === 'string' && c.person_key.trim() === personKey);
    own = get(get(contact, 'field_status'), 'person_email_validation');
  }
  own = isObject(own) ? clone(own) : null;
  const current = get(get(lead, 'field_status'), 'person_email_validation');
  if (own != null) {
    own.person_key = personKey;
    if (current !== undefined && jsonEqual(current, own)) return false;
    if (!isObject(get(lead, 'field_status'))) lead.field_status = {};
    lead.field_status.person_email_validation = own;
    return true;
  }
  if (current === undefined) return false;
  const namesSomeone = ['person_key', 'validated_email'].some((k) => trimmedKey(get(current, k)) != null);
  if (!namesSomeone) return false;
  if (isObject(get(lead, 'field_status'))) swapRemoveKey(lead.field_status, 'person_email_validation');
  return true;
}
