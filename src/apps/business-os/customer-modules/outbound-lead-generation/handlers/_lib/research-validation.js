// Prüfregeln des Recherche-Writebacks — Teilschritt 2 des Ports von
// outbound.lead.research_writeback (Handler bleibt bis zum Gesamtport legacy).
//
// ref: person_research_gap_closure.rs (origin/main 4794af03d)
//      :3363 crm_record_keys, :3393 crm_agrees, :3406 crm_tokens, :3414 crm_token_window,
//      :3426 crm_comparable, :3433 validate_source, :3456 host_is_reserved_for_documentation,
//      :3476 validate_no_match, :3494 validate_attempt (ohne Artefaktpfad-Prüfung :3514,
//      die liest das Task-Workspace → Capability beim Gesamtport)
// Inputs are the already-deserialised shapes: FieldSource {source_id, url, quote},
// FieldStatus {attempts:[{kind, query_or_url, at, artifact_path}]} (lenient serde
// parsing :193-655 is Teilschritt 3).
import { fail } from './olg.js';

const CRM_RECORD_KEYS = {
  firma_name: ['name'],
  firma_anschrift: ['anschrift', 'plz', 'ort'],
  firma_besucheranschrift: ['anschrift', 'plz', 'ort'],
  firma_postanschrift: ['anschrift', 'plz', 'ort'],
  firma_plz: ['plz'],
  firma_ort: ['ort'],
  firma_land: ['land'],
  firma_email: ['email'],
  firma_domain: ['domain'],
  firma_telefon: ['telefon'],
  firma_fax: ['fax'],
  wz_code: ['wz_code'],
  mitarbeiter: ['mitarbeiter'],
  umsatz: ['umsatz'],
  person_vorname: ['person_vorname'],
  person_nachname: ['person_nachname'],
  person_funktion: ['person_funktion'],
  person_position: ['person_position', 'person_funktion'],
  person_email: ['person_email'],
  person_telefon: ['person_telefon'],
};

/** ref: :3363 */
export function crmRecordKeys(field) {
  return Object.prototype.hasOwnProperty.call(CRM_RECORD_KEYS, field) ? CRM_RECORD_KEYS[field] : [];
}

/** ref: :3406 — split on non-alphanumeric (Unicode), lowercase per char. */
export function crmTokens(text) {
  return String(text).split(/[^\p{Alphabetic}\p{N}]/u).filter((t) => t !== '')
    // char::to_lowercase per char (no context-sensitive final sigma).
    .map((t) => Array.from(t).map((c) => c.toLowerCase()).join(''));
}

/** ref: :3414 */
export function crmTokenWindow(tokens, needle) {
  for (let start = 0; start < tokens.length; start += 1) {
    let joined = '';
    for (let i = start; i < tokens.length; i += 1) {
      joined += tokens[i];
      if (joined === needle) return true;
    }
  }
  return false;
}

const charCount = (s) => Array.from(s).length;

/** ref: :3393 */
export function crmAgrees(text, stored) {
  const textTokens = crmTokens(text);
  const storedTokens = crmTokens(stored);
  const storedJoined = storedTokens.join('');
  const textJoined = textTokens.join('');
  if (charCount(storedJoined) < 2 || charCount(textJoined) < 2) return false;
  return crmTokenWindow(textTokens, storedJoined)
    || (crmTokenWindow(storedTokens, textJoined) && charCount(textJoined) * 2 >= charCount(storedJoined));
}

/** ref: :3426 */
export function crmComparable(text) {
  return Array.from(String(text)).filter((c) => /[\p{Alphabetic}\p{N}]/u.test(c)).map((c) => c.toLowerCase()).join('');
}

/** ref: :3433 */
export function validateSource(field, source) {
  if (String(source.source_id ?? '').trim() === '') fail('invalid', `verified field \`${field}\` source_id must not be empty`);
  if (String(source.quote ?? '').trim() === '') fail('invalid', `verified field \`${field}\` quote must not be empty`);
  let url;
  try {
    url = new URL(String(source.url ?? '').trim());
  } catch {
    fail('invalid', `verified field \`${field}\` has invalid source URL`);
  }
  if (!((url.protocol === 'http:' || url.protocol === 'https:') && url.hostname !== '')) {
    fail('invalid', `verified field \`${field}\` source URL must be HTTP(S)`);
  }
}

/** ref: :3456 — `.test` stays allowed on purpose (Rust comment). */
export function hostIsReservedForDocumentation(host) {
  return ['example.com', 'example.org', 'example.net', 'example.edu', 'example', 'localhost', 'invalid'].includes(host)
    || host.endsWith('.example') || host.endsWith('.invalid') || host.endsWith('.localhost')
    || host.endsWith('.example.com') || host.endsWith('.example.org');
}

/** ref: :3476 */
export function validateNoMatch(field, status) {
  const attempts = Array.isArray(status?.attempts) ? status.attempts : [];
  const searches = attempts.filter((a) => a?.kind === 'web_search').length;
  const reads = attempts.filter((a) => a?.kind === 'web_read' || a?.kind === 'browser_capture').length;
  if (!(searches >= 1 && reads >= 2)) {
    fail('invalid', `no_match field \`${field}\` requires at least 1 web_search and 2 web_read/browser_capture attempts`);
  }
}

/** ref: :3494 — without the workspace artifact check (:3514, capability at full port). */
export function validateAttemptShape(field, attempt) {
  const kind = String(attempt?.kind ?? '');
  if (!['web_search', 'web_read', 'browser_capture', 'adapter'].includes(kind)) {
    fail('invalid', `field \`${field}\` has unsupported attempt kind \`${kind}\``);
  }
  if (String(attempt?.query_or_url ?? '').trim() === '') fail('invalid', `field \`${field}\` attempt query_or_url must not be empty`);
  if (attempt?.at === null || attempt?.at === undefined) fail('invalid', `field \`${field}\` attempt at must not be null`);
}

// ---------------------------------------------------------------- Teilschritt 4b: CrmBaseline
// ref: person_research_gap_closure.rs:3170-3360 CrmBaseline {from_research_payload, check,
//      value_agrees, agreeing_source, hint}

const CRM = 'sellify';
const isObj = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const byteOrder = (a, b) => {
  const x = Array.from(a); const y = Array.from(b);
  for (let i = 0; i < Math.min(x.length, y.length); i += 1) {
    const d = x[i].codePointAt(0) - y[i].codePointAt(0);
    if (d) return d < 0 ? -1 : 1;
  }
  return x.length === y.length ? 0 : x.length < y.length ? -1 : 1;
};

function scalarFields(record) {
  const out = new Map();
  if (!isObj(record)) return out;
  for (const [key, value] of Object.entries(record)) {
    // ABWEICHUNG: a float like 1.0 renders "1" (JSON.parse loses the notation).
    const text = typeof value === 'string' ? value.trim() : typeof value === 'number' ? String(value) : null;
    if (text) out.set(key, text);
  }
  return out;
}

export class CrmBaseline {
  constructor(records = new Map()) {
    this.records = records; // key → Map(field → value); iterate via sortedKeys()
  }

  sortedKeys() {
    return [...this.records.keys()].sort(byteOrder);
  }

  static fromResearchPayload(payload) {
    const records = new Map();
    const company = isObj(payload) ? payload.sellify_company : undefined;
    if (isObj(company)) {
      const raw = company.contact_id;
      const id = typeof raw === 'string' ? raw : Number.isInteger(raw) ? String(raw) : null;
      const contactId = id != null && id.trim() !== '' ? id.trim() : null;
      if (contactId != null) {
        const fields = scalarFields(company);
        const leadId = typeof payload.lead_id === 'string' && payload.lead_id.trim() !== '' ? payload.lead_id.trim() : null;
        if (leadId != null) records.set(`company/${leadId}`, new Map(fields));
        records.set(`company/${contactId}`, fields);
      }
    }
    const people = isObj(payload) && Array.isArray(payload.known_person_records) ? payload.known_person_records : [];
    for (const person of people) {
      const pid = isObj(person) && typeof person.sellify_person_id === 'string' ? person.sellify_person_id.trim() : '';
      if (pid) records.set(`person/${pid}`, scalarFields(person));
    }
    return new CrmBaseline(records);
  }

  recordFor(url) {
    const trimmed = String(url).trim();
    const prefix = `${CRM}://`;
    if (!trimmed.replace(/[A-Z]/g, (c) => c.toLowerCase()).startsWith(prefix)) return undefined;
    return { path: trimmed.slice(prefix.length).replace(/\/+$/, '') };
  }

  /** null: not a Sellify citation; true: carried record states the quote; false: proves nothing. */
  check(field, url, quote) {
    const ref = this.recordFor(url);
    if (!ref) return null;
    const record = this.records.get(ref.path);
    if (!record) return false;
    if (Array.from(crmComparable(quote)).length < 2) return false;
    return crmRecordKeys(field).some((key) => record.has(key) && crmAgrees(quote, record.get(key)));
  }

  valueAgrees(field, url, value) {
    const ref = this.recordFor(url);
    if (!ref) return false;
    const record = this.records.get(ref.path);
    if (!record) return false;
    return crmRecordKeys(field).some((key) => record.has(key) && crmAgrees(value, record.get(key)));
  }

  agreeingSource(field, value, personKey) {
    let recordKey;
    if (field.startsWith('person_')) {
      if (personKey == null) return null;
      const key = `person/${personKey.trim()}`;
      if (!this.records.has(key)) return null;
      recordKey = key;
    } else {
      recordKey = this.sortedKeys().find((k) => k.startsWith('company/') && /^[0-9]+$/.test(k.slice(8)));
      if (recordKey === undefined) return null;
    }
    const url = `${CRM}://${recordKey}`;
    if (this.check(field, url, value) !== true) return null;
    const record = this.records.get(recordKey);
    const quote = crmRecordKeys(field).filter((k) => record.has(k)).map((k) => record.get(k)).find((stored) => crmAgrees(value, stored));
    if (quote === undefined) return null;
    return { source_id: CRM, url, quote, person_key: personKey ?? null, requires_credential: false, task_id: '', command_id: '' };
  }

  hint() {
    if (this.records.size === 0) return 'der Auftrag bringt keinen Sellify-Datensatz mit';
    const keys = this.sortedKeys();
    // RUST-EIGENHEIT: `key[8..].chars().all(digit)` is true for an empty id.
    const urls = [
      ...keys.filter((k) => k.startsWith('company/') && /^[0-9]*$/.test(k.slice(8))),
      ...keys.filter((k) => k.startsWith('person/')).slice(0, 3),
    ].map((k) => `${CRM}://${k}`);
    if (keys.filter((k) => k.startsWith('person/')).length > 3) urls.push('…');
    return `gueltig sind nur ${urls.join(', ')}; das Zitat muss den dort gespeicherten Wert des Feldes enthalten`;
  }
}
