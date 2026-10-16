// Tolerantes Einlesen der Writeback-Nutzlast — Teilschritt 3 des Ports von
// outbound.lead.research_writeback (Handler bleibt bis zum Gesamtport legacy).
//
// ref: person_research_gap_closure.rs (origin/main 4794af03d)
//      :28 ResearchWritebackRequest (deny_unknown_fields), :50 hoist_top_level_field_entries,
//      :193 lenient_field_status, :251 parse_field_status_entry, :262 FieldStatus,
//      :318 unwrap_item_carriers, :355 unwrap_single_item_container, :381 lenient_sources,
//      :428 FieldSource / RawFieldSource, :496 host_of_url, :512 lenient_bool, :528 lenient_string,
//      :560 FieldAttempt, :574 ResearchWritebackResult (deny_unknown_fields), :584 lenient_values,
//      :622 lenient_object, :643 lenient_attempts; :1752-1757 error prefix
//      src/tools/web-stack/src/sources/mod.rs FieldKey::from_str (Klon, außerhalb des Schnappschusses)
//
// RUST-EIGENHEITEN (treu übernommen):
//  - serde_json `Map::remove` ist unter `preserve_order` ein `swap_remove`: der letzte Schlüssel
//    rückt an die Stelle des entfernten (wirkt auf Schlüsselreihenfolge und damit auf die
//    Reihenfolge, in der unbekannte Felder gemeldet werden).
//  - `result.custom_fields` ist für den Hoist ein „Streuschlüssel“ und wandert nach
//    `result.fields.custom_fields`; `result.custom_fields` bleibt damit immer `{}`.
//    → Befund für eine Folgekarte (Fachfix nicht Teil des Umbaus, §16.19 Nr. 9).
// ABWEICHUNG: Texte von JSON-Syntaxfehlern stammen aus JSON.parse, nicht aus serde_json.
import { fail, isObject } from './olg.js';
import { cmpStr } from './update-digest.js';

const own = (o, k) => isObject(o) && Object.prototype.hasOwnProperty.call(o, k);
const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));
const asciiLower = (s) => String(s).replace(/[A-Z]/g, (c) => c.toLowerCase());

const FIELD_KEY_ALIASES = [
  ['firma_name', 'firmierung'], ['firma_fruehere_namen', 'fruehere_namen', 'frühere_namen'],
  ['firma_aktivitaetsstatus', 'aktivitaetsstatus', 'aktivitätsstatus'], ['firma_anschrift', 'anschrift'],
  ['firma_besucheranschrift', 'besucheranschrift'], ['firma_postanschrift', 'postanschrift'], ['firma_postfach', 'postfach'],
  ['firma_plz', 'plz'], ['firma_ort', 'ort'], ['firma_land', 'land'], ['firma_email', 'firmen_email'],
  ['firma_domain', 'domain'], ['firma_telefon', 'firmen_telefon'], ['firma_fax', 'fax'],
  ['firma_geschaeftstaetigkeit', 'geschaeftstaetigkeit', 'geschäftstätigkeit'], ['firma_homepage_fact_sheet', 'homepage_fact_sheet'],
  ['firma_geschaeftsfuehrung', 'geschaeftsfuehrung', 'geschäftsführung'], ['firma_prokura', 'prokura'], ['wz_code', 'wz'],
  ['umsatz'], ['mitarbeiter', 'anzahl_mitarbeiter'], ['crm_record_number', 'crm_nummer'], ['person_geschlecht', 'geschlecht'],
  ['person_titel', 'titel'], ['person_vorname', 'vorname'], ['person_nachname', 'nachname'], ['person_funktion', 'funktion'],
  ['person_position', 'position'], ['person_email'], ['person_email_validation', 'email_validation'],
  ['person_telefon', 'telefon', 'telefonnummer'], ['person_linkedin', 'linkedin'], ['person_xing', 'xing'],
];
const FIELD_KEY_INDEX = new Map(FIELD_KEY_ALIASES.flatMap((names) => names.map((n) => [n, names[0]])));

/** FieldKey::from_str → canonical key or null (trim + ASCII lowercase; ä/ü stay as written). */
export function fieldKeyFromStr(raw) {
  return FIELD_KEY_INDEX.get(asciiLower(String(raw).trim())) ?? null;
}

/** serde_json Map::remove under preserve_order = swap_remove (mutates, returns value). */
export function mapRemove(object, key) {
  if (!own(object, key)) return undefined;
  const keys = Object.keys(object);
  const value = object[key];
  const index = keys.indexOf(key);
  const last = keys[keys.length - 1];
  const order = keys.slice();
  if (last !== key) order[index] = last;
  order.pop();
  const values = Object.fromEntries(order.map((k) => [k, object[k]]));
  for (const k of keys) delete object[k];
  Object.assign(object, values);
  return value;
}

/** serde_json Value Display (compact JSON). */
const display = (v) => JSON.stringify(v);

/** serde "invalid type" phrase for a JSON value. */
function unexpected(v) {
  if (v === null) return 'null';
  if (typeof v === 'boolean') return `boolean \`${v}\``;
  if (typeof v === 'number') return Number.isInteger(v) ? `integer \`${v}\`` : `floating point \`${v}\``;
  if (typeof v === 'string') return `string ${JSON.stringify(v)}`;
  return Array.isArray(v) ? 'sequence' : 'map';
}

class SerdeError extends Error {}
const serdeFail = (message) => { throw new SerdeError(message); };

function expectOneOf(fields) {
  return fields.length === 1 ? `\`${fields[0]}\``
    : fields.length === 2 ? `\`${fields[0]}\` or \`${fields[1]}\``
      : `one of ${fields.map((f) => `\`${f}\``).join(', ')}`;
}

function parseJsonText(text, prefix) {
  try {
    return JSON.parse(text);
  } catch (error) {
    return serdeFail(`${prefix}${error.message}`);
  }
}

// ---------------------------------------------------------------- carriers

/** ref: :318 */
export function unwrapItemCarriers(value) {
  const carrier = (k) => ['item', 'items', 'entry', 'entries', 'element', 'elements', 'list'].includes(asciiLower(k.trim()));
  if (Array.isArray(value)) return value.map(unwrapItemCarriers);
  if (!isObject(value)) return value;
  const keys = Object.keys(value);
  if (keys.length === 1) {
    const inner = value[keys[0]];
    if (carrier(keys[0]) && (Array.isArray(inner) || isObject(inner))) {
      const unwrapped = unwrapItemCarriers(clone(inner));
      return Array.isArray(unwrapped) ? unwrapped : [unwrapped];
    }
  }
  return Object.fromEntries(keys.map((k) => [k, unwrapItemCarriers(value[k])]));
}

/** ref: :355 */
function unwrapSingleItemContainer(value) {
  if (!isObject(value)) return null;
  const keys = Object.keys(value);
  if (keys.length !== 1) return null;
  const inner = value[keys[0]];
  const carrier = ['item', 'items', 'entry', 'entries', 'element', 'elements', 'list', 'source', 'sources', 'value', 'values']
    .includes(asciiLower(keys[0].trim()));
  if (!carrier || !(Array.isArray(inner) || isObject(inner))) return null;
  return clone(inner);
}

// ---------------------------------------------------------------- hoist

const ENVELOPE = ['record_id', 'module', 'research_command_id', 'gap_task_id', 'field_status', 'result'];
const RESULT_MEMBERS = ['fields', 'person_records', 'evidence', 'person_field_status'];

/** ref: :50 hoist_top_level_field_entries (returns the reshaped payload). */
export function hoistTopLevelFieldEntries(payloadIn) {
  if (!isObject(payloadIn)) return payloadIn;
  const object = clone(payloadIn);
  for (const carrier of ['item', 'items', 'entry', 'entries']) {
    if (isObject(object[carrier]) && own(object, carrier)) {
      const inner = clone(object[carrier]);
      mapRemove(object, carrier);
      for (const [k, v] of Object.entries(inner)) if (!own(object, k)) object[k] = v;
    }
  }
  const bareValues = Object.keys(object).filter((k) => !ENVELOPE.includes(k) && !isObject(object[k]) && fieldKeyFromStr(k) != null);
  if (bareValues.length) {
    const removedResult = mapRemove(object, 'result');
    const result = isObject(removedResult) ? removedResult : {};
    const removedFields = mapRemove(result, 'fields');
    const fields = isObject(removedFields) ? removedFields : {};
    for (const key of bareValues) {
      if (own(object, key)) {
        const value = mapRemove(object, key);
        if (!own(fields, key)) fields[key] = value;
      }
    }
    result.fields = fields;
    object.result = result;
  }
  const stray = Object.keys(object).filter((k) => !ENVELOPE.includes(k) && isObject(object[k]) && own(object[k], 'status'));
  const removedResult = mapRemove(object, 'result');
  const result = isObject(removedResult) ? removedResult : {};
  let hoistedResult = false;
  for (const key of RESULT_MEMBERS) {
    if (own(object, key)) {
      const value = mapRemove(object, key);
      if (!own(result, key)) result[key] = value;
      hoistedResult = true;
    }
  }
  const strayResultKeys = Object.keys(result).filter((k) => !RESULT_MEMBERS.includes(k));
  if (strayResultKeys.length) {
    const removedFields = mapRemove(result, 'fields');
    const fields = isObject(removedFields) ? removedFields : {};
    for (const key of strayResultKeys) {
      if (own(result, key)) {
        const value = mapRemove(result, key);
        const entry = isObject(value) ? value : { value };
        if (!own(fields, key)) fields[key] = entry;
      }
    }
    result.fields = fields;
    hoistedResult = true;
  }
  if (hoistedResult || Object.keys(result).length) object.result = result;
  if (!stray.length) return object;
  const removedStatus = mapRemove(object, 'field_status');
  let fieldStatus = {};
  if (isObject(removedStatus)) fieldStatus = removedStatus;
  else if (typeof removedStatus === 'string') {
    try {
      const parsed = JSON.parse(removedStatus);
      fieldStatus = isObject(parsed) ? parsed : {};
    } catch {
      fieldStatus = {};
    }
  }
  for (const key of stray) {
    if (own(object, key)) {
      const entry = mapRemove(object, key);
      if (!own(fieldStatus, key)) fieldStatus[key] = entry;
    }
  }
  object.field_status = fieldStatus;
  return object;
}

// ---------------------------------------------------------------- lenient members

/** ref: :528 — null → "", number/bool rendered, string kept; else error. */
export function lenientString(v) {
  if (v === undefined || v === null) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return serdeFail(`expected a string, got ${display(v)}`);
}

/** ref: :512 */
export function lenientBool(v) {
  if (typeof v === 'boolean') return v;
  if (typeof v === 'number') return v !== 0;
  if (typeof v === 'string') return ['true', '1', 'yes', 'ja', 'y', 'j'].includes(asciiLower(v.trim()));
  return false;
}

/** ref: :496 */
export function hostOfUrl(url) {
  const trimmed = String(url).trim();
  const at = trimmed.indexOf('://');
  const without = at >= 0 ? trimmed.slice(at + 3) : trimmed;
  let host = without.split(/[/?#]/)[0] ?? '';
  while (host.startsWith('www.')) host = host.slice(4);
  return asciiLower(host);
}

/** ref: :428 RawFieldSource → FieldSource (serialised key order). */
function parseFieldSource(item) {
  if (!isObject(item)) serdeFail(`invalid type: ${unexpected(item)}, expected struct RawFieldSource`);
  const s = (k) => lenientString(item[k]);
  const personKey = item.person_key;
  if (!(personKey === undefined || personKey === null || typeof personKey === 'string')) {
    serdeFail(`invalid type: ${unexpected(personKey)}, expected a string`);
  }
  let sourceId = s('source_id').trim();
  if (!sourceId) sourceId = s('host').trim();
  if (!sourceId) sourceId = s('domain').trim();
  const url = s('url');
  if (!sourceId) sourceId = hostOfUrl(url);
  return {
    source_id: sourceId,
    url,
    quote: s('quote'),
    person_key: typeof personKey === 'string' ? personKey : null,
    requires_credential: lenientBool(item.requires_credential),
    task_id: s('task_id'),
    command_id: s('command_id'),
  };
}

/** ref: :381 lenient_sources */
export function lenientSources(value) {
  if (value === undefined || value === null) return [];
  if (Array.isArray(value)) return value.map(parseFieldSource);
  if (isObject(value)) {
    const inner = unwrapSingleItemContainer(value);
    if (inner != null) return lenientSources(inner);
    return [parseFieldSource(value)];
  }
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) return [];
    const parsed = parseJsonText(trimmed, 'sources must be a list of source objects, not a string: ');
    if (typeof parsed === 'string') serdeFail('sources must be a list of source objects, not a string');
    return lenientSources(parsed);
  }
  return serdeFail(`sources must be a list of source objects, got ${display(value)}`);
}

/** ref: :584 lenient_values */
export function lenientValues(value) {
  if (value === undefined || value === null) return [];
  if (Array.isArray(value)) return value;
  if (isObject(value)) {
    const inner = unwrapSingleItemContainer(value);
    if (inner != null) return lenientValues(inner);
    return [value];
  }
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) return [];
    const parsed = parseJsonText(trimmed, 'expected a list of objects, not a string: ');
    if (typeof parsed === 'string') serdeFail('expected a list of objects, not a string');
    return lenientValues(parsed);
  }
  return serdeFail(`expected a list of objects, got ${display(value)}`);
}

/** ref: :622 lenient_object — note `other => other` keeps arrays/numbers as they are. */
export function lenientObject(value) {
  if (value === undefined || value === null) return {};
  if (typeof value === 'string') {
    if (!value.trim()) return {};
    const parsed = parseJsonText(value.trim(), 'expected an object, not a string: ');
    if (!isObject(parsed)) serdeFail('expected an object of field entries');
    return parsed;
  }
  return value;
}

/** ref: :560 FieldAttempt (serialised key order). */
function parseAttempt(v) {
  if (!isObject(v)) serdeFail(`invalid type: ${unexpected(v)}, expected struct FieldAttempt`);
  return {
    kind: lenientString(v.kind),
    query_or_url: lenientString(v.query_or_url),
    result: own(v, 'result') ? v.result : null,
    artifact_path: lenientString(v.artifact_path),
    at: own(v, 'at') ? v.at : null,
  };
}

const FIELD_STATUS_KNOWN = ['status', 'value', 'sources', 'attempts', 'reason'];

/** ref: :251 parse_field_status_entry — any error or blank status → null (entry dropped). */
export function parseFieldStatusEntry(entry) {
  try {
    if (!isObject(entry)) return null;
    const status = lenientString(entry.status);
    const parsed = {
      status,
      value: own(entry, 'value') ? clone(entry.value) : null,
      sources: lenientSources(entry.sources),
      attempts: lenientValues(entry.attempts).map(parseAttempt),
      reason: lenientString(entry.reason),
      // #[serde(flatten)] BTreeMap → sorted keys.
      extra: Object.fromEntries(Object.keys(entry).filter((k) => !FIELD_STATUS_KNOWN.includes(k)).sort(cmpStr).map((k) => [k, clone(entry[k])])),
    };
    return parsed.status.trim() ? parsed : null;
  } catch (error) {
    if (error instanceof SerdeError) return null;
    throw error;
  }
}

/** serde Serialize of FieldStatus: declared members, then the flattened extra map. */
export function serializeFieldStatus(fs) {
  return { status: fs.status, value: fs.value, sources: fs.sources, attempts: fs.attempts, reason: fs.reason, ...fs.extra };
}

/** ref: :193 lenient_field_status → BTreeMap (sorted object). */
export function lenientFieldStatus(rawIn) {
  let raw = rawIn;
  if (typeof raw === 'string') raw = parseJsonText(raw, '');
  const entries = new Map();
  if (isObject(raw)) {
    for (const [field, status] of Object.entries(raw)) {
      if (!isObject(status)) continue;
      const parsed = parseFieldStatusEntry(status);
      if (parsed) entries.set(field, parsed);
    }
  } else if (Array.isArray(raw)) {
    for (const item of raw) {
      if (!isObject(item)) continue;
      const fieldName = ['field', 'key', 'name', 'field_key'].map((k) => item[k]).find((v) => typeof v === 'string');
      if (fieldName === undefined) continue;
      const status = clone(item);
      for (const k of ['field', 'key', 'name', 'field_key']) delete status[k];
      const parsed = parseFieldStatusEntry(status);
      if (parsed) entries.set(fieldName, parsed);
    }
  } else if (raw !== null) {
    serdeFail(`field_status must be an object keyed by field, got ${display(raw)}`);
  }
  return Object.fromEntries([...entries.keys()].sort(cmpStr).map((k) => [k, entries.get(k)]));
}

const RESULT_FIELDS = ['fields', 'person_records', 'evidence', 'person_field_status', 'custom_fields'];

function parseResult(value) {
  if (!isObject(value)) serdeFail(`invalid type: ${unexpected(value)}, expected struct ResearchWritebackResult`);
  const out = { fields: {}, person_records: [], evidence: [], person_field_status: {}, custom_fields: {} };
  for (const [key, v] of Object.entries(value)) {
    if (!RESULT_FIELDS.includes(key)) serdeFail(`unknown field \`${key}\`, expected ${expectOneOf(RESULT_FIELDS)}`);
    out[key] = key === 'person_records' || key === 'evidence' ? lenientValues(v) : lenientObject(v);
  }
  return out;
}

function requiredStringField(v) {
  if (typeof v !== 'string') serdeFail(`invalid type: ${unexpected(v)}, expected a string`);
  return v;
}

/**
 * ref: :1752-1757 — `serde_json::from_value(hoist(unwrap(payload)))`, mapped to
 * "invalid outbound.lead.research_writeback payload: {error}".
 * Returns {record_id, module, research_command_id, gap_task_id, field_status, result}.
 */
export function parseResearchWritebackRequest(payload) {
  try {
    const shaped = hoistTopLevelFieldEntries(unwrapItemCarriers(clone(payload)));
    if (!isObject(shaped)) serdeFail(`invalid type: ${unexpected(shaped)}, expected struct ResearchWritebackRequest`);
    const out = { gap_task_id: '', result: { fields: {}, person_records: [], evidence: [], person_field_status: {}, custom_fields: {} } };
    for (const [key, v] of Object.entries(shaped)) {
      if (!ENVELOPE.includes(key)) serdeFail(`unknown field \`${key}\`, expected ${expectOneOf(ENVELOPE)}`);
      if (key === 'field_status') out.field_status = lenientFieldStatus(v);
      else if (key === 'result') out.result = parseResult(v);
      else out[key] = requiredStringField(v);
    }
    for (const key of ['record_id', 'module', 'research_command_id', 'field_status']) {
      if (!own(out, key)) serdeFail(`missing field \`${key}\``);
    }
    return {
      record_id: out.record_id,
      module: out.module,
      research_command_id: out.research_command_id,
      gap_task_id: out.gap_task_id,
      field_status: out.field_status,
      result: out.result,
    };
  } catch (error) {
    if (error instanceof SerdeError) fail('invalid', `invalid outbound.lead.research_writeback payload: ${error.message}`);
    throw error;
  }
}
