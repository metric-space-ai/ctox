// Lead-Ergebnispatch der Recherche — Teilschritt 5 des Ports von outbound.lead.research_writeback
// (auch Phase A von web_stack.person_research).
//
// ref: person_research_command.rs (origin/main 4794af03d)
//      :555 outbound_lead_generation_research_outcome_patch, :834 deduplicate_contacts,
//      :881 merge_researched_person_records_with_context, :937 keep_email_verdict,
//      :956 set_known_person_contacts, :982 known_person_contact, :1039 merge_known_contact_values,
//      :1053 merge_research_into_known_contact, :1089 researched_alias_has_canonical_value,
//      :1116 contact_for_person_key, :1136 contacts_match, :1168 profiles_match, :1186-1197 normalized_*,
//      :1205 contact_name_is_placeholder, :1269 same_person_by_name, :1321 candidate_person_email,
//      :1354 normalize_domain, :1383 german_fold, :1397 has_german_diacritics,
//      :1408 restore_german_spelling_from_quotes, :1440 restore_german_spelling_in_person_record,
//      :1484 restore_german_spelling, :1506 normalize_country_to_iso2, :1526 normalized_contact_name,
//      :1537 normalize_person_name_for_match, :1548 transliterate_name_token, :1563 json_value_is_empty,
//      :1569 lead_location_values, :1833 normalize_researched_contact_with_context, :1906 contact_string,
//      :1919 with_stable_contact_id, :1966 javascript_fingerprint, :1975 base36, :1988 merge_json_object_values
//      contact_email_validation.rs :204 is_email_verdict
//      web-stack person_ranking.rs (Klon src/tools/web-stack, Stand 3a9100ed7): role_category,
//      contains_role_keyword, role_is_valid
//
// serde_json läuft mit `preserve_order`: Objekte behalten Einfügeordnung, `Map::remove` ist
// swap_remove (RUST-EIGENHEIT, über swapRemoveKey nachgebildet).
import { has, isObject, utf8Len } from './olg.js';
import { jsonEqual } from './research-source-adapter.js';
import { unwrapItemCarriers } from './research-writeback-payload.js';
import { deduplicateResearchEvidence, matchingResearchEvidenceCount, requiredIndependentSources, researchScalarIsPopulated } from './research-evidence.js';
import { swapRemoveKey } from './research-writeback-sanitize.js';

const get = (v, k) => (has(v, k) ? v[k] : undefined);
const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));
const asciiLower = (s) => String(s).replace(/[A-Z]/g, (c) => c.toLowerCase());
const isAlphabetic = (c) => /^\p{Alphabetic}$/u.test(c);
const isAlphanumeric = (c) => /^[\p{Alphabetic}\p{N}]$/u.test(c);
const splitWhitespace = (s) => String(s).split(/\s+/u).filter((w) => w !== '');
const optStr = (v) => (typeof v === 'string' ? v : null);
/** `.and_then(as_str).map(trim).filter(non-empty)` */
const trimmedKey = (v) => (typeof v === 'string' && v.trim() !== '' ? v.trim() : null);

// ------------------------------------------------------------------ web-stack person_ranking

const ROLE_KEYWORD_GROUPS = [
  ['geschäftsführ', 'geschaeftsfuehr', 'ceo', 'managing director', 'inhaber', 'vorstand'],
  ['prokur'],
  ['cfo', 'finanz', 'controlling'],
  ['einkauf', 'procurement', 'purchasing'],
  ['supply chain', 'logistik', 'scm'],
  ['operations', 'coo', 'werkleit', 'betriebsleit', 'produktionsleit'],
  ['technik', 'cto', 'technische leitung', 'technischer leiter', 'technische leiterin', 'instandhaltung'],
  ['entwicklung', 'r&d', 'f&e', 'research'],
];
const GENERIC_ROLE_SUBSTRINGS = ['leiter', 'leiterin', 'leitung', 'head of', 'manager', 'director', 'direktor', 'vorstand',
  'assistenz', 'assistant', 'sales', 'verkauf', 'vertrieb', 'marketing', 'personal', 'human resources', 'informatik',
  'consultant', 'berater', 'referent', 'sachbearbeit', 'kaufmann', 'kauffrau', 'kaufmänn', 'controller', 'buchhalt',
  'ingenieur', 'engineer', 'specialist', 'spezialist', 'expert', 'koordinator', 'coordinator', 'projekt', 'product',
  'produkt', 'partner', 'gesellschafter', 'owner', 'founder', 'gründer', 'chef', 'officer', 'analyst', 'architekt',
  'architect', 'qualität', 'quality', 'service', 'support', 'recruit', 'key account'];
const GENERIC_ROLE_WORDS = ['hr', 'it', 'ceo', 'cfo', 'coo', 'cto', 'cio', 'cmo', 'cso', 'vp', 'pm'];

export function roleCategory(role) {
  const normalized = String(role).trim().toLowerCase();
  const index = ROLE_KEYWORD_GROUPS.findIndex((keywords) => keywords.some((k) => normalized.includes(k)));
  return index < 0 ? 99 : index + 1;
}

export function containsRoleKeyword(value) {
  const normalized = String(value).trim().toLowerCase();
  return roleCategory(value) !== 99
    || GENERIC_ROLE_SUBSTRINGS.some((k) => normalized.includes(k))
    || normalized.split(/[^\p{Alphabetic}\p{N}]/u).filter((w) => w !== '').some((w) => GENERIC_ROLE_WORDS.includes(w));
}

const normalizeComparison = (value) => Array.from(String(value)).flatMap((c) => Array.from(c.toLowerCase())).filter(isAlphanumeric).join('');

function isShortCapitalizedNameOrPlace(value) {
  const words = splitWhitespace(value);
  if (words.length === 0 || words.length > 3) return false;
  return words.every((word) => Array.from(word).every(isAlphabetic) && /^\p{Uppercase}/u.test(word));
}

export function roleIsValid(valueIn, personName, locations) {
  const value = String(valueIn).trim();
  if (value === '') return false;
  if (containsRoleKeyword(value)) return true;
  const normalized = normalizeComparison(value);
  if (normalized === '' || normalized === normalizeComparison(personName)
    || locations.some((location) => normalized === normalizeComparison(location))
    || ['dr', 'prof', 'professor'].includes(normalized)) return false;
  return !isShortCapitalizedNameOrPlace(value);
}

// ------------------------------------------------------------------ contact_email_validation

const INCONCLUSIVE_LABEL = 'unknown_catch_all';
/** ref: contact_email_validation.rs:204 */
export function isEmailVerdict(value) {
  const v = String(value).trim().toLowerCase();
  return ['valid', 'invalid', 'gültig', 'gueltig', 'ungültig', 'ungueltig', 'zustellbar', 'unzustellbar', 'bestätigt',
    'bestaetigt', INCONCLUSIVE_LABEL].some((word) => v.includes(word));
}

// ------------------------------------------------------------------ helpers

/** ref: :1906 */
export function contactString(contact, keys) {
  for (const key of keys) {
    const found = trimmedKey(get(contact, key));
    if (found != null) return found;
  }
  return '';
}

/** ref: :1563 */
export function jsonValueIsEmpty(value) {
  return value === null || value === undefined || (typeof value === 'string' && value.trim() === '') || (Array.isArray(value) && value.length === 0);
}

/** ref: :1988 — returns the merged target (Rust replaces `*target` when either side is no object). */
export function mergeJsonObjectValues(target, patch) {
  if (!isObject(target) || !isObject(patch)) return clone(patch);
  for (const [key, value] of Object.entries(patch)) {
    if (has(target, key) && isObject(value) && isObject(target[key])) mergeJsonObjectValues(target[key], value);
    else target[key] = clone(value);
  }
  return target;
}

/** ref: :1966 — FNV-1a over UTF-16 units of the lowercased text, base36. */
export function javascriptFingerprint(value) {
  const text = String(value).toLowerCase();
  let hash = 2166136261;
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 16777619) >>> 0;
  }
  return (hash >>> 0).toString(36);
}

const normalizedEmail = (value) => String(value).trim().toLowerCase();
const normalizedPhone = (value) => String(value).replace(/[^0-9]/g, '');
const normalizedProfile = (value) => String(value).trim().replace(/\/+$/, '').toLowerCase();

function transliterateNameToken(value) {
  let out = '';
  for (const ch of Array.from(String(value)).flatMap((c) => Array.from(c.toLowerCase()))) {
    if (ch === 'ä') out += 'ae';
    else if (ch === 'ö') out += 'oe';
    else if (ch === 'ü') out += 'ue';
    else if (ch === 'ß') out += 'ss';
    else if (isAlphabetic(ch)) out += ch;
  }
  return out;
}

const NAME_PARTICLES = ['da', 'de', 'del', 'den', 'der', 'di', 'du', 'la', 'le', 'van', 'von', 'zu', 'zum', 'zur'];
/** ref: :1537 */
export function normalizePersonNameForMatch(value) {
  return String(value).split(/[^\p{Alphabetic}]/u).map(transliterateNameToken)
    .filter((t) => t !== '' && !NAME_PARTICLES.includes(t)).join('');
}

function combinedName(contact) {
  const first = contactString(contact, ['person_vorname', 'first_name']);
  const last = contactString(contact, ['person_nachname', 'last_name']);
  return first === '' && last === '' ? contactString(contact, ['name']) : `${first} ${last}`;
}

const normalizedContactName = (contact) => normalizePersonNameForMatch(combinedName(contact));
const PROFILE_KEYS = ['person_linkedin', 'person_xing', 'linkedin', 'xing', 'source_url'];
const profileOf = (contact) => normalizedProfile(contactString(contact, PROFILE_KEYS));

/** ref: :1168 */
export function profilesMatch(left, right) {
  const l = profileOf(left);
  return l !== '' && l === profileOf(right);
}

/** ref: :1136 */
export function contactsMatch(left, right) {
  const lk = contactString(left, ['person_key']);
  const rk = contactString(right, ['person_key']);
  if (lk !== '' && rk !== '') return lk === rk;
  const le = normalizedEmail(contactString(left, ['person_email', 'email']));
  if (le !== '' && le === normalizedEmail(contactString(right, ['person_email', 'email']))) return true;
  if (profilesMatch(left, right)) return true;
  const ln = normalizedContactName(left);
  if (ln === '' || ln !== normalizedContactName(right)) return false;
  const lp = normalizedPhone(contactString(left, ['person_telefon', 'telefon', 'phone']));
  return lp !== '' && lp === normalizedPhone(contactString(right, ['person_telefon', 'telefon', 'phone']));
}

/** ref: :1269 */
export function samePersonByName(left, right) {
  const lp = profileOf(left);
  const rp = profileOf(right);
  if (lp !== '' && rp !== '' && lp !== rp) return false;
  const le = normalizedEmail(contactString(left, ['person_email', 'email']));
  const re = normalizedEmail(contactString(right, ['person_email', 'email']));
  if (le !== '' && re !== '' && le !== re) return false;
  const last = (c) => normalizePersonNameForMatch(contactString(c, ['person_nachname', 'last_name']));
  const firstToken = (c) => {
    const words = splitWhitespace(contactString(c, ['person_vorname', 'first_name']));
    return words.length ? normalizePersonNameForMatch(words[0]) : '';
  };
  const ll = last(left);
  if (ll === '' || ll !== last(right)) return false;
  const lf = firstToken(left);
  const rf = firstToken(right);
  return lf === '' || rf === '' || lf === rf;
}

const PLACEHOLDERS = ['test', 'testtest', 'na', 'n/a', 'nn', 'xxx', 'xx', 'unbekannt', 'unknown', 'dummy', 'muster',
  'beispiel', 'example', 'todo', 'tbd', 'keine', 'none', 'null'];
/** ref: :1205 */
export function contactNameIsPlaceholder(contact) {
  const tokens = splitWhitespace(combinedName(contact))
    .map((t) => Array.from(t).filter((c) => isAlphanumeric(c) || c === '/').join('').toLowerCase())
    .filter((t) => t !== '');
  if (tokens.length === 0 || !tokens.every((t) => PLACEHOLDERS.includes(t))) return false;
  return contactString(contact, ['person_email', 'email']) === ''
    && contactString(contact, ['person_telefon', 'phone']) === ''
    && contactString(contact, ['person_linkedin', 'person_xing', 'linkedin', 'xing']) === '';
}

/** ref: :1569 */
export function leadLocationValues(existing) {
  const out = [];
  for (const path of [['city'], ['firma_ort'], ['data', 'city'], ['data', 'firma_ort']]) {
    let cursor = existing;
    for (const key of path) cursor = get(cursor, key);
    const found = trimmedKey(cursor);
    if (found != null) out.push(found);
  }
  return out;
}

// ------------------------------------------------------------------ value normalisation

/** ref: :1354 */
export function normalizeDomain(value) {
  if (typeof value !== 'string') return clone(value);
  const trimmed = value.trim();
  if (trimmed === '' || /\s/u.test(trimmed)) return value;
  const parse = (text) => { try { return new URL(text); } catch { return null; } };
  // Url::parse(trimmed).ok().or_else(https://…) — a successful parse without host falls back to the text.
  const url = parse(trimmed) ?? parse(`https://${trimmed}`);
  let host = url != null && url.hostname !== '' ? url.hostname : trimmed;
  host = host.trim().replace(/\.+$/, '');
  while (host.startsWith('www.')) host = host.slice(4);
  host = asciiLower(host);
  if (host === '' || !host.includes('.')) return value;
  return host;
}

/** ref: :1383 */
export function germanFold(value) {
  let out = '';
  for (const c of String(value)) {
    if (c === 'ä' || c === 'Ä') out += 'ae';
    else if (c === 'ö' || c === 'Ö') out += 'oe';
    else if (c === 'ü' || c === 'Ü') out += 'ue';
    else if (c === 'ß') out += 'ss';
    else out += c.toLowerCase();
  }
  return out;
}

const hasGermanDiacritics = (value) => /[äöüÄÖÜß]/u.test(String(value));

/** ref: :1408 */
export function restoreGermanSpellingFromQuotes(value, quotes) {
  const trimmed = String(value).trim();
  if (trimmed === '' || hasGermanDiacritics(trimmed)) return null;
  const folded = germanFold(trimmed);
  if (folded === '') return null;
  const foldedLen = Array.from(folded).length;
  for (const quote of quotes) {
    if (utf8Len(quote) > 2000 || !hasGermanDiacritics(quote)) continue;
    const chars = Array.from(quote);
    for (let start = 0; start < chars.length; start += 1) {
      for (let end = start + 1; end <= chars.length; end += 1) {
        if (end - start > foldedLen) break;
        const candidate = chars.slice(start, end).join('');
        if (hasGermanDiacritics(candidate) && germanFold(candidate) === folded) return candidate;
      }
    }
  }
  return null;
}

/** ref: :1440 */
export function restoreGermanSpellingInPersonRecord(record) {
  const evidence = get(record, 'evidence');
  const quotes = (Array.isArray(evidence) ? evidence : [])
    .map((e) => (has(e, 'quote') ? e.quote : get(e, 'note'))).filter((q) => typeof q === 'string');
  if (quotes.length === 0 || !isObject(record)) return record;
  for (const [key, value] of Object.entries(record)) {
    if (key === 'evidence' || key === 'person_key' || key.endsWith('_url') || key.includes('email') || key.includes('linkedin')
      || key.includes('xing') || key.includes('telefon') || key.includes('phone')) continue;
    if (typeof value !== 'string') continue;
    const restored = restoreGermanSpellingFromQuotes(value, quotes);
    if (restored != null) record[key] = restored;
  }
  return record;
}

/** ref: :1484 */
export function restoreGermanSpelling(value, field) {
  if (typeof value !== 'string') return clone(value);
  const candidates = get(field, 'candidates');
  const quotes = (Array.isArray(candidates) ? candidates : [])
    .map((c) => (has(c, 'quote') ? c.quote : get(c, 'snippet'))).filter((q) => typeof q === 'string');
  return restoreGermanSpellingFromQuotes(value, quotes) ?? value;
}

/** ref: :1506 */
export function normalizeCountryToIso2(value) {
  if (typeof value !== 'string') return clone(value);
  const key = value.trim().toLowerCase();
  if (['de', 'deutschland', 'germany', 'bundesrepublik deutschland'].includes(key)) return 'DE';
  if (['at', 'österreich', 'oesterreich', 'austria'].includes(key)) return 'AT';
  if (['ch', 'schweiz', 'switzerland', 'suisse', 'svizzera'].includes(key)) return 'CH';
  const trimmed = key.trim();
  if (/^[A-Za-z]{2}$/.test(trimmed)) return trimmed.toUpperCase();
  return value;
}

/** ref: :1321 */
export function candidatePersonEmail(result) {
  const looksLikeAddress = (v) => {
    const t = v.trim();
    return t.includes('@') && t.includes('.') && !/\s/u.test(t);
  };
  const direct = get(get(get(result, 'fields'), 'person_email'), 'value');
  if (typeof direct === 'string' && looksLikeAddress(direct.trim())) return direct.trim();
  const records = get(result, 'person_records');
  for (const record of Array.isArray(records) ? records : []) {
    const raw = has(record, 'person_email') ? record.person_email : get(record, 'email');
    if (typeof raw === 'string' && looksLikeAddress(raw.trim())) return raw.trim();
  }
  return null;
}

// ------------------------------------------------------------------ contacts

/** ref: :1833 — returns the normalised contact or null. */
export function normalizeResearchedContact(contact, locations = []) {
  const first = (optStr(get(contact, 'person_vorname')) ?? '').trim();
  const last = (optStr(get(contact, 'person_nachname')) ?? '').trim();
  const structured = `${first} ${last}`.trim();
  const name = structured === '' ? contactString(contact, ['name']) : structured;
  const email = contactString(contact, ['person_email', 'email']);
  const phone = contactString(contact, ['person_telefon', 'phone']);
  const position = contactString(contact, ['person_position', 'position']);
  const profile = contactString(contact, ['person_linkedin', 'person_xing', 'linkedin', 'xing']);
  if (name === '' && email === '' && phone === '' && position === '' && profile === '') return null;
  const functionCandidate = contactString(contact, ['person_funktion']);
  const roleCandidates = [functionCandidate, contactString(contact, ['person_position']), contactString(contact, ['role'])];
  const role = roleCandidates.find((c) => roleIsValid(c, name, locations)) ?? '';
  if (functionCandidate !== '' && !roleIsValid(functionCandidate, name, locations)) {
    contact.person_funktion_candidate = functionCandidate;
    contact.person_funktion = '';
  } else if (role === '') {
    const candidate = roleCandidates.find((c) => c !== '');
    if (candidate !== undefined) contact.person_funktion_candidate = candidate;
  }
  Object.assign(contact, { name, role, position, email, phone });
  return contact;
}

/** ref: :1919 */
export function withStableContactId(leadId, contact, index) {
  if (trimmedKey(get(contact, 'id')) != null) return contact;
  let identity = ['name', 'first_name', 'last_name', 'person_vorname', 'person_nachname', 'email', 'person_email', 'phone',
    'person_telefon', 'linkedin', 'xing', 'person_linkedin', 'person_xing']
    .map((key) => (optStr(get(contact, key)) ?? '').trim().toLowerCase()).join('|');
  if (identity.replace(/\|/g, '') === '') identity = `position-${index}`;
  contact.id = `contact_${javascriptFingerprint(`${leadId}|${identity}`)}`;
  return contact;
}

/** ref: :834 */
export function deduplicateContacts(contacts) {
  const identity = (contact, key) => {
    const v = trimmedKey(get(contact, key));
    return v == null ? null : asciiLower(v);
  };
  const kept = [];
  for (const contact of contacts) {
    if (contactNameIsPlaceholder(contact)) continue;
    const id = identity(contact, 'id');
    const personKey = identity(contact, 'person_key');
    const duplicateOf = kept.findIndex((existing) => {
      const existingKey = identity(existing, 'person_key');
      if (personKey != null && existingKey != null) return existingKey === personKey;
      return id != null && identity(existing, 'id') === id;
    });
    if (duplicateOf >= 0) {
      kept[duplicateOf] = mergeJsonObjectValues(kept[duplicateOf], contact);
      continue;
    }
    kept.push(contact);
  }
  return kept;
}

/** ref: :937 */
function keepEmailVerdict(existing, incoming) {
  if (!isObject(incoming)) return;
  for (const key of ['person_email_validation', 'email_validation']) {
    const e = get(existing, key);
    const i = get(incoming, key);
    if (typeof e === 'string' && isEmailVerdict(e) && !(typeof i === 'string' && isEmailVerdict(i))) swapRemoveKey(incoming, key);
  }
}

/** ref: :1039 */
function mergeKnownContactValues(known, existing) {
  if (!isObject(known) || !isObject(existing)) return;
  for (const [key, value] of Object.entries(existing)) {
    if (!has(known, key) || jsonValueIsEmpty(known[key])) known[key] = clone(value);
  }
  known.source = 'sellify';
  known.crm_known = true;
}

/** ref: :1089 */
function researchedAliasHasCanonicalValue(field, researched) {
  const alias = { person_funktion: 'role', person_position: 'position', person_email: 'email', person_telefon: 'phone',
    person_vorname: 'name', person_nachname: 'name' }[field];
  return alias !== undefined && has(researched, alias) && !jsonValueIsEmpty(researched[alias]);
}

/** ref: :1053 */
export function mergeResearchIntoKnownContact(known, researched) {
  const sourceUrl = contactString(researched, ['source_url']);
  if (!isObject(known) || !isObject(researched)) return;
  const removed = swapRemoveKey(known, 'conflicts');
  const conflicts = Array.isArray(removed) ? removed : [];
  for (const [field, value] of Object.entries(researched)) {
    if (['id', 'conflicts', 'crm_known', 'source'].includes(field) || researchedAliasHasCanonicalValue(field, researched)) continue;
    if (!has(known, field) || jsonValueIsEmpty(known[field])) known[field] = clone(value);
    else if (!jsonValueIsEmpty(value) && !jsonEqual(known[field], value)) {
      const conflict = { field, value: clone(value), source_url: sourceUrl };
      if (!conflicts.some((c) => jsonEqual(c, conflict))) conflicts.push(conflict);
    }
  }
  known.conflicts = conflicts;
  known.source = 'sellify';
  known.crm_known = true;
}

/** ref: :1116 */
export function contactForPersonKey(contacts, key) {
  if (key == null) return {};
  const found = contacts.find((c) => [get(c, 'person_key'), get(c, 'sellify_person_id')].some((v) => typeof v === 'string' && v.trim() === key));
  return found !== undefined ? clone(found) : { person_key: key };
}

/** ref: :982 */
export function knownPersonContact(leadId, known, locations) {
  const firstName = contactString(known, ['vorname', 'person_vorname', 'first_name']);
  const lastName = contactString(known, ['nachname', 'person_nachname', 'last_name']);
  const fn = contactString(known, ['funktion', 'person_funktion', 'role']);
  const email = contactString(known, ['email', 'person_email']);
  const phone = contactString(known, ['telefon', 'person_telefon', 'phone']);
  const safeId = (v) => (/^[A-Za-z0-9_-]{1,64}$/.test(v) ? v : '');
  const sellifyContactId = safeId(contactString(known, ['sellify_contact_id']));
  const sellifyPersonId = safeId(contactString(known, ['sellify_person_id']));
  let knownSourceUrl = '';
  try {
    const url = new URL(contactString(known, ['source']));
    if (url.protocol === 'http:' || url.protocol === 'https:') knownSourceUrl = url.href;
  } catch { /* not a URL */ }
  const name = `${firstName} ${lastName}`.trim();
  if (name === '' && email === '' && phone === '') return null;
  const idSuffix = sellifyContactId !== '' ? sellifyContactId : sellifyPersonId !== '' ? sellifyPersonId
    : javascriptFingerprint(`${leadId}|${name}|${email}|${phone}`);
  let contact = {
    id: `contact_sellify_${idSuffix}`,
    person_vorname: firstName,
    person_nachname: lastName,
    person_funktion: fn,
    person_email: email,
    person_telefon: phone,
    sellify_contact_id: sellifyContactId,
    source_url: knownSourceUrl,
    source: 'sellify',
    crm_known: true,
  };
  if (sellifyPersonId !== '') {
    contact.sellify_person_id = sellifyPersonId;
    contact.person_key = sellifyPersonId;
  }
  contact = normalizeResearchedContact(contact, locations);
  if (contact == null) return null;
  contact.source = 'sellify';
  contact.crm_known = true;
  return contact;
}

/** ref: :956 — mutates `contacts`. */
export function setKnownPersonContacts(leadId, contacts, knownPersonRecords, locations) {
  for (const known of knownPersonRecords) {
    const contact = knownPersonContact(leadId, clone(known), locations);
    if (contact == null) continue;
    const index = contacts.findIndex((existing) => contactsMatch(existing, contact) || samePersonByName(existing, contact));
    if (index >= 0) {
      mergeKnownContactValues(contact, contacts[index]);
      contacts[index] = contact;
    } else {
      contacts.push(contact);
    }
  }
}

/** ref: :881 — mutates `contacts`. */
export function mergeResearchedPersonRecords(leadId, contacts, personRecords, locations = []) {
  personRecords.forEach((record, index) => {
    let normalized = normalizeResearchedContact(record, locations);
    if (normalized == null || contactNameIsPlaceholder(normalized)) return;
    normalized = withStableContactId(leadId, normalized, index);
    const personKey = (c) => trimmedKey(get(c, 'person_key'));
    const normalizedKey = personKey(normalized);
    const nid = optStr(get(normalized, 'id'));
    const existingIndex = contacts.findIndex((existing) => optStr(get(existing, 'id')) === nid
      || (normalizedKey != null && personKey(existing) === normalizedKey)
      || profilesMatch(existing, normalized)
      || samePersonByName(existing, normalized)
      || (get(existing, 'crm_known') === true && contactsMatch(existing, normalized)));
    if (existingIndex >= 0) {
      keepEmailVerdict(contacts[existingIndex], normalized);
      if (get(contacts[existingIndex], 'crm_known') === true) mergeResearchIntoKnownContact(contacts[existingIndex], normalized);
      else contacts[existingIndex] = mergeJsonObjectValues(contacts[existingIndex], normalized);
    } else {
      contacts.push(normalized);
    }
  });
}

// ------------------------------------------------------------------ outcome patch

function evidenceEntry(fieldKey, candidate, personKeyFallback) {
  const sourceId = optStr(get(candidate, 'source_id')) ?? '';
  const sourceUrl = optStr(get(candidate, 'source_url')) ?? '';
  const pick = (k) => (has(candidate, k) ? clone(candidate[k]) : null);
  let quote;
  if (has(candidate, 'quote')) quote = clone(candidate.quote);
  else if (has(candidate, 'note')) quote = clone(candidate.note);
  else quote = pick('value');
  return {
    field_key: fieldKey,
    value: pick('value'),
    confidence: pick('confidence'),
    source_id: sourceId,
    source_url: sourceUrl,
    person_key: has(candidate, 'person_key') ? clone(candidate.person_key) : personKeyFallback,
    quote,
    tier: pick('tier'),
    via: pick('via'),
    label: sourceId.trim() === '' ? sourceUrl : sourceId,
  };
}

/**
 * ref: :555 outbound_lead_generation_research_outcome_patch — pure function, returns the patch.
 */
export function researchOutcomePatch(existing, commandResultIn, now) {
  const commandResult = unwrapItemCarriers(clone(commandResultIn));
  const outcome = isObject(get(commandResult, 'result')) ? commandResult.result : commandResult;
  const leadId = optStr(get(existing, 'id')) ?? 'lead';
  const data = isObject(get(existing, 'data')) ? clone(existing.data) : {};
  let contacts = Array.isArray(get(existing, 'contacts')) ? clone(existing.contacts) : [];
  const leadLocations = leadLocationValues(existing);
  const known = get(outcome, 'known_person_records');
  setKnownPersonContacts(leadId, contacts, Array.isArray(known) ? clone(known) : [], leadLocations);
  const personContacts = []; // [key|null, contact]
  const rawRecords = get(outcome, 'person_records');
  let personRecords = Array.isArray(rawRecords) ? clone(rawRecords) : [];
  const hasPersonRecords = personRecords.length > 0;
  let evidence = Array.isArray(get(existing, 'evidence')) ? clone(existing.evidence) : [];
  const outcomeEvidence = get(outcome, 'evidence');
  if (Array.isArray(outcomeEvidence)) evidence.push(...clone(outcomeEvidence));
  const researchedFieldKeys = [];
  const unboundPersonFieldKeys = [];

  const fields = isObject(get(outcome, 'fields')) ? outcome.fields : {};
  for (const [fieldKey, field] of Object.entries(fields)) {
    const raw = get(field, 'value');
    if (raw === undefined || !researchScalarIsPopulated(raw)) continue;
    const fieldPersonKey = trimmedKey(get(field, 'person_key'));
    if (fieldKey.startsWith('person_') && !hasPersonRecords && contacts.length > 0 && fieldPersonKey == null) {
      unboundPersonFieldKeys.push(fieldKey);
      continue;
    }
    researchedFieldKeys.push(fieldKey);
    const value = restoreGermanSpelling(raw, field);
    if (fieldKey.startsWith('person_') && !hasPersonRecords) {
      let slot = personContacts.findIndex(([k]) => k === fieldPersonKey);
      if (slot < 0) {
        personContacts.push([fieldPersonKey, contactForPersonKey(contacts, fieldPersonKey)]);
        slot = personContacts.length - 1;
      }
      personContacts[slot][1][fieldKey] = clone(value);
    } else if (!fieldKey.startsWith('person_')) {
      data[fieldKey] = fieldKey === 'firma_land' ? normalizeCountryToIso2(value)
        : fieldKey === 'firma_domain' ? normalizeDomain(value) : clone(value);
    }
    const candidates = get(field, 'candidates');
    for (const candidate of Array.isArray(candidates) ? candidates : []) {
      const sourceId = optStr(get(candidate, 'source_id')) ?? '';
      const sourceUrl = optStr(get(candidate, 'source_url')) ?? '';
      if (sourceId.trim() === '' && sourceUrl.trim() === '') continue;
      evidence.push(evidenceEntry(fieldKey, candidate, null));
    }
  }

  personRecords = personRecords.map(restoreGermanSpellingInPersonRecord);
  for (const record of personRecords) {
    const list = get(record, 'evidence');
    for (const candidate of Array.isArray(list) ? list : []) {
      // RUST-EIGENHEIT: field_key kommt hier nur aus `field`, nicht aus `field_key`.
      const entry = evidenceEntry(has(candidate, 'field') ? clone(candidate.field) : null, candidate,
        has(record, 'person_key') ? clone(record.person_key) : null);
      evidence.push(entry);
    }
  }
  evidence = deduplicateResearchEvidence(evidence);
  if (hasPersonRecords) {
    mergeResearchedPersonRecords(leadId, contacts, personRecords, leadLocations);
  } else {
    personContacts.forEach(([, contact], index) => {
      let normalized = normalizeResearchedContact(contact, leadLocations);
      if (normalized == null) return;
      normalized = withStableContactId(leadId, normalized, index);
      if (contacts.length === 0) {
        contacts.push(normalized);
        return;
      }
      const existingIndex = contacts.findIndex((c) => contactsMatch(c, normalized));
      if (existingIndex >= 0) {
        if (get(contacts[existingIndex], 'crm_known') === true) mergeResearchIntoKnownContact(contacts[existingIndex], normalized);
        else contacts[existingIndex] = normalized;
      } else {
        contacts.push(normalized);
      }
    });
  }
  contacts = deduplicateContacts(contacts);
  const contactIds = new Set(contacts.map((c) => get(c, 'id')).filter((id) => typeof id === 'string'));
  const selected = get(existing, 'selected_contact_ids');
  const selectedContactIds = (Array.isArray(selected) ? selected : []).filter((id) => typeof id === 'string' && contactIds.has(id));
  const unverifiedFieldKeys = researchedFieldKeys.filter((fieldKey) => {
    const field = has(fields, fieldKey) ? fields[fieldKey] : null;
    return matchingResearchEvidenceCount(evidence, fieldKey, field, !hasPersonRecords) < requiredIndependentSources(fieldKey);
  });
  const verifiedFieldKeys = researchedFieldKeys.filter((k) => !unverifiedFieldKeys.includes(k));
  const pick = (k, fallback) => (has(outcome, k) ? clone(outcome[k]) : fallback);
  return {
    data,
    contacts,
    selected_contact_ids: selectedContactIds,
    evidence,
    research_status: researchedFieldKeys.length > 0 && unverifiedFieldKeys.length === 0 ? 'completed' : 'needs_review',
    research_error: null,
    research_updated_at_ms: now,
    payload: {
      researched_field_keys: researchedFieldKeys,
      verified_field_keys: verifiedFieldKeys,
      unverified_field_keys: unverifiedFieldKeys,
      unbound_person_field_keys: unboundPersonFieldKeys,
      research_finished_at_ms: now,
      research_tool: pick('tool', null),
      research_instructions_len: pick('research_instructions_len', null),
      browser_assist_tasks: pick('browser_assist_tasks', []),
      authenticated_source_capture_runs: pick('authenticated_source_capture_runs', []),
    },
  };
}
