// Personenrecherche Phase A als Agentenauftrag (Strang OLG4): App-Prüfungen vor dem Auftrag, Template-Werte und die
// Normalisierung der Agenten-Antwort. Vorher (OLG3) zwei Aufrufe ctx.cap.browser.capture({query, extract_schema}) an die
// Core-Recherchemaschine (src/tools/web-stack/src/person_research.rs + sources/, ~17 000 Zeilen Rust); die Maschine wird
// nicht portiert: Suchplan, Quellenmodule und Personen-Gruppierung stehen jetzt als Arbeitsanweisung im Paket-Skill
// skills/person-research/SKILL.md und im Template prompts/person-research.md.
//
// App-Logik hier (ref person_research_command.rs, Rust-Stand 4794af03d):
//   :2187 client_context (JSON-Text oder Objekt), :2140 verified_research_owner_user_id,
//   :2193-2211 Pflichtfelder company/country/mode mit den Rust-Fehlertexten (Country::from_iso, ResearchMode::from_str
//   aus src/tools/web-stack/src/sources/mod.rs:60/:102), :2224 Eigentümerpflicht bei auto_browser_capture,
//   :2229 research_instructions_len, :2132/:2265 Bezeichner der Laufzeitquellen (jetzt VOR dem Auftrag geprüft),
//   :2367-2401 Zusammenfassungstext.
// ref web-stack person_research.rs: :124 mode_skipped (have_data/update_inventory_general ohne Recherche),
//   :2078 empty_plan_response, :479 Antwortform (plan/fields/person_records/*_runs), :1059 aggregate_fields
//   (fehlendes Feld = {value:null, confidence:'missing', candidates:[]}), :1179 „plan bleibt Autorität: fremde Felder
//   werden verworfen, nicht übernommen“.
import { fail, has, isObject } from './olg.js';
import { canonicalRequestedFields, fieldDefinition, safeWorkspaceSegment } from './research-writeback-sanitize.js';

const get = (v, k) => (has(v, k) ? v[k] : undefined);
const asciiUpper = (s) => String(s).replace(/[a-z]/g, (c) => c.toUpperCase());
const asciiLower = (s) => String(s).replace(/[A-Z]/g, (c) => c.toLowerCase());

/** ref: web-stack sources/mod.rs:60 Country::from_iso → ISO code or null. */
export function countryFromIso(raw) {
  const v = asciiUpper(String(raw).trim());
  if (['DE', 'DEU', 'GERMANY', 'DEUTSCHLAND'].includes(v)) return 'DE';
  if (['AT', 'AUT', 'AUSTRIA', 'OESTERREICH', 'ÖSTERREICH'].includes(v)) return 'AT';
  if (['CH', 'CHE', 'SCHWEIZ', 'SWITZERLAND'].includes(v)) return 'CH';
  return null;
}

/** ref: web-stack sources/mod.rs:102 ResearchMode::from_str → canonical mode or null. */
export function researchModeFromStr(raw) {
  const v = asciiLower(String(raw).trim());
  if (['have_data', 'a'].includes(v)) return 'have_data';
  if (['update_inventory_general', 'update_inventory', 'b'].includes(v)) return 'update_inventory_general';
  if (['update_person', 'b1', 'b 1'].includes(v)) return 'update_person';
  if (['update_firm', 'b2', 'b 2'].includes(v)) return 'update_firm';
  if (['new_record', 'neu'].includes(v)) return 'new_record';
  return null;
}

/** ref: :2140 verified_research_owner_user_id */
export function verifiedResearchOwnerUserId(clientContext) {
  for (const path of [['owner_user_id'], ['actor', 'id'], ['user_id'], ['native_authorization', 'actor', 'id']]) {
    let cursor = clientContext;
    for (const key of path) cursor = get(cursor, key);
    if (typeof cursor === 'string' && cursor.trim() !== '') return cursor.trim();
  }
  return null;
}

/** ref: :2132 safe_runtime_source_identifier (Bytes, [A-Za-z0-9._-]). */
export function safeRuntimeSourceIdentifier(value, maxLen) {
  return typeof value === 'string' && value !== '' && new TextEncoder().encode(value).length <= maxLen && /^[A-Za-z0-9._-]+$/.test(value);
}

/** ref: :2187 — client_context as JSON text is parsed, unparsable text counts as null. */
export function parseClientContext(raw) {
  if (typeof raw !== 'string') return raw ?? null;
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

/**
 * ref: :2191-2228 — the app-side checks before any capture work. Returns
 * {company, country, mode, auto_browser_capture, owner_user_id, research_instructions_len, runtime_sources}.
 * The strict field vocabulary (:2212 parse_requested_fields) stays with the capture engine (FieldKey list of the web stack).
 */
export function parseResearchRequest(payload, clientContextRaw) {
  if (!isObject(payload)) fail('invalid', 'invalid web_stack.person_research payload');
  for (const key of ['company', 'country', 'mode']) {
    if (typeof get(payload, key) !== 'string') fail('invalid', 'invalid web_stack.person_research payload');
  }
  const company = payload.company.trim();
  if (company === '') fail('invalid', 'company is required');
  const country = countryFromIso(payload.country);
  if (country == null) fail('invalid', `unsupported country \`${payload.country}\`; expected DE, AT or CH`);
  const mode = researchModeFromStr(payload.mode);
  if (mode == null) {
    fail('invalid', `unsupported research mode \`${payload.mode}\`; expected new_record, update_firm, update_person, update_inventory_general or have_data`);
  }
  const autoBrowserCapture = get(payload, 'auto_browser_capture') === true;
  const ownerUserId = verifiedResearchOwnerUserId(parseClientContext(clientContextRaw));
  // Authenticated capture runs as the human who asked; without a verified owner it is rejected up front.
  if (autoBrowserCapture && ownerUserId == null) fail('invalid', 'authenticated person-research capture requires a verified command owner');
  const instructions = typeof get(payload, 'research_instructions') === 'string' ? payload.research_instructions : '';
  const sources = get(get(payload, 'source_policy'), 'sources');
  const runtimeSources = Array.isArray(sources) ? sources : [];
  return {
    company, country, mode, auto_browser_capture: autoBrowserCapture, owner_user_id: ownerUserId,
    research_instructions_len: new TextEncoder().encode(instructions).length, runtime_sources: runtimeSources,
  };
}

/** ref: :2367-2401 — the summary line of a finished run. */
export function researchSummary(company, result) {
  const fields = get(result, 'fields');
  const populated = isObject(fields) ? Object.values(fields).filter((f) => has(f, 'value') && f.value !== null).length : 0;
  const requested = Array.isArray(get(result, 'requested_fields')) ? result.requested_fields.length : 0;
  const assists = Array.isArray(get(result, 'browser_assist_tasks')) ? result.browser_assist_tasks.length : 0;
  return assists > 0
    ? `Recherche für ${company} abgeschlossen: ${populated} von ${requested} Feldern gefunden. ${assists} Quelle(n) benötigen eine Browser-Autorisierung.`
    : `Recherche für ${company} abgeschlossen: ${populated} von ${requested} Feldern gefunden.`;
}

/** ref: :2265-2272 — runtime source identifiers; jetzt vor dem Auftrag (der Agent bekommt nur geprüfte Quellen). */
export function checkRuntimeSources(request) {
  for (const source of request.runtime_sources) {
    const id = typeof get(source, 'id') === 'string' ? source.id.trim() : '';
    const targetKey = typeof get(source, 'target_key') === 'string' ? source.target_key.trim() : '';
    if (!(safeRuntimeSourceIdentifier(id, 160) && safeRuntimeSourceIdentifier(targetKey, 128))) {
      fail('invalid', 'invalid runtime research source identifier');
    }
  }
}

/** ref: web-stack person_research.rs:124 — Modi ohne Recherche: leerer Plan, kein Agentenauftrag. */
export const MODE_SKIPPED = Object.freeze(['have_data', 'update_inventory_general']);

/** Arbeitsbereich der Personenrecherche relativ zum CTOX-Root (ABWEICHUNG wie gap_closure: der Kern besitzt den Workspace). */
export function personWorkspaceRoot(commandId) {
  return `runtime/research/person/${safeWorkspaceSegment(commandId)}`;
}

const pretty = (v) => JSON.stringify(v, null, 2);
const list = (v) => (Array.isArray(v) ? v : []);

/** ref: web-stack person_research.rs:2078 empty_plan_response */
export function emptyPlanResponse(request, payload, reason) {
  return {
    ok: true, tool: 'ctox_person_research', company: request.company, country: request.country, mode: request.mode,
    fields: {}, person_records: [], person_priorities: list(get(payload, 'person_priorities')),
    known_person_records: list(get(payload, 'known_person_records')), plan: [], search_runs: [], read_runs: [], scrape_runs: [],
    browser_extract_runs: [], browser_assist_tasks: [], skipped_reason: reason,
  };
}

/**
 * Werte für prompts/person-research.md. Feldliste wie :2212 parse_requested_fields streng (canonicalRequestedFields wirft bei
 * unbekanntem Feld), je Feld die Definition aus dem Feldvertrag. Sensible Teile des client_context gehen nie in den Text.
 */
export function personResearchTemplateValues(command, request) {
  const p = command.payload;
  const fields = canonicalRequestedFields(command, {});
  return {
    company: request.company, country: request.country, mode: request.mode,
    record_id: typeof command.record_id === 'string' && command.record_id.trim() !== '' ? command.record_id.trim() : '-',
    research_command_id: command.id,
    field_lines: fields.map((field) => `- \`${field}\`: ${fieldDefinition(field)}`).join('\n'),
    requested_fields: pretty(fields),
    include_private: pretty(list(get(p, 'include_private'))),
    runtime_sources: pretty(request.runtime_sources),
    research_instructions: typeof get(p, 'research_instructions') === 'string' ? p.research_instructions : '',
    person_priorities: pretty(list(get(p, 'person_priorities'))),
    known_person_records: pretty(list(get(p, 'known_person_records'))),
  };
}

const RUN_LISTS = ['plan', 'search_runs', 'read_runs', 'scrape_runs', 'browser_extract_runs', 'browser_assist_tasks'];

/**
 * Antwort des Agenten → Phase-A-Ergebnis in der Form der früheren Erfassung (person_research.rs:479). Der Plan der App bleibt
 * Autorität (:1179): nur angefragte Felder, fehlende als missing (:1059), Kandidaten nur als Objekte; Laufprotokolle als
 * Listen. Die Bewertung (verified/offen, Lückenschluss, Lead-Projektion) macht danach der Complete-Handler wie bisher.
 */
export function normalizePhaseAOutcome(block, command, request) {
  const requested = canonicalRequestedFields(command, {});
  const raw = isObject(get(block, 'fields')) ? block.fields : {};
  const fields = {};
  for (const field of requested) {
    const entry = get(raw, field);
    if (!isObject(entry) || !has(entry, 'value') || entry.value === null || entry.value === '') {
      fields[field] = { value: null, confidence: 'missing', candidates: isObject(entry) ? list(entry.candidates).filter(isObject) : [] };
      continue;
    }
    fields[field] = { ...entry, candidates: list(entry.candidates).filter(isObject) };
  }
  const outcome = {
    ok: true, tool: 'ctox_person_research', company: request.company, country: request.country, mode: request.mode,
    requested_fields: requested, fields, person_records: list(get(block, 'person_records')).filter(isObject),
    person_priorities: list(get(command.payload, 'person_priorities')), known_person_records: list(get(command.payload, 'known_person_records')),
  };
  for (const key of RUN_LISTS) outcome[key] = list(get(block, key)).filter(isObject);
  if (Array.isArray(get(block, 'evidence'))) outcome.evidence = block.evidence.filter(isObject);
  return outcome;
}
