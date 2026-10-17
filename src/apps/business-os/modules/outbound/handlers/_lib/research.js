// Research processors shared helpers. ref: importer.rs:2045 domain_from_url, :3134 requested_research_fields,
// :3157 requested_contact_fields, :3179 company_field_keys, :3214 contact_field_keys, :3290 merge_json_objects,
// :3305 apply_company_research_fields, :3334 evidence_from_research, :3361 contacts_from_research,
// :3489 infer_name_from_profile_url, :3514 normalize_whitespace, :440-475 GDPR provenance.
// Recherche selbst (Rust: ctox_web_stack::run_ctox_person_research_tool, importer.rs:3266) läuft seit Strang OUT3 als
// agent.task mit Rückruf (_lib/research-task.js); fields sind FieldKey::as_str-Namen
// (src/tools/web-stack/src/sources/mod.rs:209), mode 'new_record' | 'update_person'.
import { sha256Hex, asciiLower, isObject } from './outbound.js';
import { firstNonempty } from './knowledge.js';

function fieldIds(list) {
  if (!Array.isArray(list)) return [];
  const out = [];
  for (const field of list) {
    const raw = typeof field?.id === 'string' ? field.id : typeof field === 'string' ? field : null;
    const trimmed = raw == null ? '' : raw.trim();
    if (trimmed) out.push(trimmed);
  }
  return out;
}

export const requestedResearchFields = (command) => fieldIds(command.payload?.research_request?.fields);
export const requestedContactFields = (command) => fieldIds(command.payload?.contact_fields);

export function companyFieldKeys(fields) {
  const map = {
    city: 'firma_ort', postal_code: 'firma_plz', street: 'firma_anschrift', email: 'firma_email',
    domain: 'firma_domain', industry_wz: 'wz_code', revenue_eur: 'umsatz', employee_count: 'mitarbeiter',
  };
  const out = [];
  for (const field of fields) {
    const key = map[field];
    if (key && !out.includes(key)) out.push(key);
  }
  if (out.length === 0) out.push('firma_anschrift', 'firma_plz', 'firma_ort', 'firma_email', 'firma_domain', 'wz_code', 'umsatz', 'mitarbeiter');
  return out;
}

export function contactFieldKeys(fields) {
  const out = [];
  const push = (key) => { if (!out.includes(key)) out.push(key); };
  for (const field of fields) {
    if (field === 'contact.people') ['person_vorname', 'person_nachname', 'person_funktion', 'person_linkedin', 'person_xing'].forEach(push);
    else if (field === 'contact.role' || field === 'contact.fit') ['person_funktion', 'person_position'].forEach(push);
    else if (field === 'contact.email') push('person_email');
    else if (field === 'contact.linkedin') ['person_linkedin', 'person_xing'].forEach(push);
    else if (field === 'contact.phone') push('person_telefon');
    else if (field.startsWith('contact_custom_')) ['person_vorname', 'person_nachname', 'person_funktion', 'person_position', 'person_linkedin', 'person_xing'].forEach(push);
  }
  if (out.length === 0) out.push('person_vorname', 'person_nachname', 'person_funktion', 'person_position', 'person_email', 'person_telefon', 'person_linkedin', 'person_xing');
  // #772 (1c269d63a, Skill outbound-lead-generation-research §5): jede zurückgeschriebene Person trägt
  // person_geschlecht und person_titel – sobald Personen (Name) recherchiert werden, gehören sie zur Anfrage.
  if (out.includes('person_vorname') || out.includes('person_nachname')) PERSON_MANDATORY_FIELDS.forEach(push);
  return out;
}

export const PERSON_MANDATORY_FIELDS = ['person_geschlecht', 'person_titel'];

export function domainFromUrl(url) {
  const value = String(url ?? '').trim();
  const withoutScheme = value.startsWith('https://') ? value.slice(8) : value.startsWith('http://') ? value.slice(7) : value;
  const host = (withoutScheme.split('/')[0] ?? '').trim();
  return asciiLower(host.replace(/^(www\.)+/, ''));
}

export function mergeJsonObjects(a, b) {
  return { ...(isObject(a) ? JSON.parse(JSON.stringify(a)) : {}), ...(isObject(b) ? JSON.parse(JSON.stringify(b)) : {}) };
}

function researchFieldValue(research, field) {
  const value = research?.fields?.[field]?.value;
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  return trimmed || null;
}

export function applyCompanyResearchFields(data, research) {
  for (const [webKey, outboundKey] of [
    ['firma_anschrift', 'street'], ['firma_plz', 'postal_code'], ['firma_ort', 'city'], ['firma_email', 'email'],
    ['firma_domain', 'domain'], ['wz_code', 'industry_wz'], ['umsatz', 'revenue_eur'], ['mitarbeiter', 'employee_count'],
  ]) {
    const value = researchFieldValue(research, webKey);
    if (value != null) {
      data[outboundKey] = value;
      data[webKey] = value;
    }
  }
}

export function evidenceFromResearch(research) {
  const evidence = [];
  if (!isObject(research?.fields)) return evidence;
  for (const [field, entry] of Object.entries(research.fields)) {
    if (Array.isArray(entry?.candidates)) {
      for (const candidate of entry.candidates.slice(0, 6)) {
        evidence.push({ ...(isObject(candidate) ? candidate : {}), field });
      }
    } else if (isObject(entry) && 'value' in entry) {
      const item = { field, value: entry.value ?? null };
      if ('source_url' in entry) item.source_url = entry.source_url;
      evidence.push(item);
    }
  }
  return evidence;
}

export function countryIsGermany(value) {
  return ['de', 'deu', 'germany', 'deutschland', 'bundesrepublik deutschland'].includes(asciiLower(String(value).trim()));
}

/** ref: :3489 infer_name_from_profile_url (minimal URL parse; no WHATWG URL in the sandbox) */
export function inferNameFromProfileUrl(url) {
  const match = /^[A-Za-z][A-Za-z0-9+.-]*:\/\/[^/?#]*([^?#]*)/.exec(String(url ?? '').trim());
  if (!match) return '';
  const segments = match[1].split('/').filter((segment) => segment !== '');
  const segment = segments[segments.length - 1];
  if (!segment) return '';
  return segment.split('-')
    .filter((part) => !/^[0-9]*$/.test(part))
    .map((part) => (part ? part[0].toUpperCase() + part.slice(1) : ''))
    .filter(Boolean)
    .join(' ');
}

const normalizeWhitespace = (value) => String(value).split(/\s+/).filter(Boolean).join(' ');
const byteOrder = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

/** ref: :3361 contacts_from_research (BTreeMap ⇒ sorted keys) */
export function contactsFromResearch(pipelineId, companyId, companyName, research) {
  const grouped = new Map();
  for (const [field, target] of [
    ['person_vorname', 'first_name'], ['person_nachname', 'last_name'], ['person_funktion', 'role'], ['person_position', 'role'],
    ['person_email', 'email'], ['person_telefon', 'phone'], ['person_linkedin', 'linkedin_url'], ['person_xing', 'xing_url'],
    ['person_geschlecht', 'gender'], ['person_titel', 'title'],
  ]) {
    const candidates = research?.fields?.[field]?.candidates;
    if (!Array.isArray(candidates)) continue;
    for (const candidate of candidates.slice(0, 8)) {
      const value = typeof candidate?.value === 'string' ? candidate.value.trim() : '';
      if (!value) continue;
      const sourceUrl = typeof candidate.source_url === 'string' ? candidate.source_url
        : typeof candidate.hit_url === 'string' ? candidate.hit_url : value;
      if (!grouped.has(sourceUrl)) grouped.set(sourceUrl, {});
      const entry = grouped.get(sourceUrl);
      if (!(target in entry)) entry[target] = value;
      if (!('evidence' in entry)) entry.evidence = [];
      if (Array.isArray(entry.evidence)) entry.evidence.push({ ...(isObject(candidate) ? candidate : {}), field });
    }
  }
  const byIdentity = new Map();
  for (const key of [...grouped.keys()].sort(byteOrder)) {
    const item = grouped.get(key);
    const first = typeof item.first_name === 'string' ? item.first_name.trim() : '';
    const last = typeof item.last_name === 'string' ? item.last_name.trim() : '';
    const inferred = inferNameFromProfileUrl(typeof item.linkedin_url === 'string' ? item.linkedin_url : typeof item.xing_url === 'string' ? item.xing_url : '');
    const contactName = firstNonempty([[first, last].filter(Boolean).join(' '), inferred]);
    const role = typeof item.role === 'string' ? item.role.trim() : '';
    const linkedin = typeof item.linkedin_url === 'string' ? item.linkedin_url.trim() : '';
    if (!contactName && !role && !linkedin) continue;
    item.contact_id = `contact_${sha256Hex(`${pipelineId}:${companyId}:${contactName}:${role}:${linkedin}`).slice(0, 16)}`;
    item.company_name = companyName;
    item.contact_name = contactName;
    const identity = firstNonempty([linkedin, typeof item.xing_url === 'string' ? item.xing_url : '', normalizeWhitespace(`${contactName} ${role}`)]);
    if (!byIdentity.has(identity)) byIdentity.set(identity, item);
  }
  const contacts = [...byIdentity.keys()].sort(byteOrder).map((key) => byIdentity.get(key));
  applyPersonSourceRule(contacts, grouped);
  return contacts;
}

/** Anbieter einer Quelle: source_id/provider, sonst Host ohne www (#772: Anbieter zählen, nicht Hosts). */
export function sourceProvider(candidate, sourceUrl) {
  const named = [candidate?.source_id, candidate?.provider, candidate?.source].find((v) => typeof v === 'string' && v.trim());
  if (named) return asciiLower(named.trim());
  return domainFromUrl(sourceUrl) || String(sourceUrl);
}

/**
 * #772 (1c269d63a): Personen folgen der Zwei-Quellen-Regel – Name und aktuelle Funktion brauchen zwei
 * unabhängige Anbieter (z. B. LinkedIn plus XING, Leadfeeder, D&B Hoovers oder Firmenseite). Ein Profil allein
 * ist nur `single source: <anbieter>` (Notfall, erst nachdem jede gelistete Quelle gefragt wurde). Jede
 * zurückgeschriebene Person trägt person_geschlecht und person_titel (leer = unbekannt, nie fehlend).
 */
export function applyPersonSourceRule(contacts, grouped) {
  const providersByName = new Map();
  for (const [sourceUrl, item] of grouped) {
    const first = typeof item.first_name === 'string' ? item.first_name.trim() : '';
    const last = typeof item.last_name === 'string' ? item.last_name.trim() : '';
    const name = asciiLower(normalizeWhitespace([first, last].filter(Boolean).join(' ')));
    if (!name) continue;
    const evidence = Array.isArray(item.evidence) ? item.evidence : [];
    const provider = sourceProvider(evidence[0], sourceUrl);
    if (!providersByName.has(name)) providersByName.set(name, new Set());
    providersByName.get(name).add(provider);
  }
  for (const contact of contacts) {
    const name = asciiLower(normalizeWhitespace(contact.contact_name ?? ''));
    const providers = [...(providersByName.get(name) ?? [])].sort(byteOrder);
    contact.person_sources = providers;
    contact.person_source_status = providers.length >= 2 ? 'verified' : 'single_source';
    contact.person_source_reason = providers.length >= 2 ? '' : `single source: ${providers.join(', ') || 'none'}`;
    if (typeof contact.gender !== 'string') contact.gender = '';
    if (typeof contact.title !== 'string') contact.title = '';
    contact.person_geschlecht = contact.gender;
    contact.person_titel = contact.title;
  }
  return contacts;
}

export const OUTBOUND_CONTACT_LEGAL_BASIS = 'legitimate_interest';
export const OUTBOUND_CONTACT_PURPOSE = 'outbound_b2b_contact_research';
export const OUTBOUND_CONTACT_BASIS_EVIDENCE = 'Operator-asserted B2B prospecting; operator must keep a documented Legitimate Interest Assessment (LIA) and honor Art. 17 erasure requests.';
export const OUTBOUND_CONTACT_RETENTION_MS = 180 * 24 * 60 * 60 * 1000;

/** ref: :455 outbound_contact_subject_key (Rust to_lowercase = Unicode lowercase) */
export function contactSubjectKey(contact, companyName) {
  const email = typeof contact?.email === 'string' ? contact.email.trim().toLowerCase() : '';
  const name = typeof contact?.contact_name === 'string' ? contact.contact_name.trim().toLowerCase() : '';
  const seed = email || `${name}|${String(companyName).trim().toLowerCase()}`;
  return `subj_${sha256Hex(seed).slice(0, 16)}`;
}
