// Bereinigung und Endprüfungen des Recherche-Writebacks — Teilschritt 4 (Rest) des Ports
// von outbound.lead.research_writeback.
//
// ref: person_research_gap_closure.rs (origin/main 4794af03d)
//      :25 TERMINAL_FIELD_STATUSES, :276 FieldWritebackRevision, :283 stamp_writeback_field_revisions,
//      :2189 apply_custom_field_results, :2357 validate_original_research_command,
//      :2395 validate_task_correlation, :2434 sanitize_research_writeback,
//      :3096 validate_terminal_field, :3494 validate_attempt, :3514 validate_artifact_path,
//      :3562 action_required_has_auth_reference, :3765 add_field_status_evidence,
//      :3812 canonical_requested_fields, :3850 phase_a_verified_sources,
//      :3899 phase_a_attempted_sources, :3950 task_workspace, :3979 required_string,
//      :3988 field_definition
//      person_research_command.rs :2411 safe_workspace_segment
//
// FieldStatus shape (from parseResearchWritebackRequest):
//   {status, value, sources:[{source_id,url,quote,person_key,requires_credential,task_id,command_id}],
//    attempts:[{kind,query_or_url,result,artifact_path,at}], reason, extra}
// `field_status` is a Rust BTreeMap → JS object kept in code-point key order (sortKeys).
//
// OWNER-REGEL (THESEN): `requiredIndependentSources` = 1 für jedes Feld (Rust-Stand 4794af03d,
// „Owner rule 23.09.2026“) – unverändert übernommen, nicht ausgelegt.
import { fail, has, isObject } from './olg.js';
import { jsonEqual } from './research-source-adapter.js';
import { cmpStr } from './update-digest.js';
import { fieldKeyFromStr } from './research-writeback-payload.js';
import { researchValueIsPopulated } from './research-writeback-checks.js';
import { hostIsReservedForDocumentation, validateNoMatch, validateSource } from './research-validation.js';
import {
  CRM_SOURCE_SCHEME, crmAwareProviderCount, emailQuoteBacks, independentResearchEvidenceCount, quoteBacksValue,
  requiredIndependentSources,
} from './research-evidence.js';

export const TERMINAL_FIELD_STATUSES = ['verified', 'no_match', 'unsupported', 'action_required'];
export const GAP_METADATA_KEY = 'person_research_gap_closure';

const get = (v, k) => (has(v, k) ? v[k] : undefined);
const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));
const nonBlank = (v) => typeof v === 'string' && v.trim() !== '';
const asciiLower = (s) => String(s).replace(/[A-Z]/g, (c) => c.toLowerCase());

/** BTreeMap order: rebuild an object with code-point sorted keys (mutates in place). */
export function sortKeys(object) {
  const entries = Object.keys(object).sort(cmpStr).map((k) => [k, object[k]]);
  for (const [k] of entries) delete object[k];
  for (const [k, v] of entries) object[k] = v;
  return object;
}

/** `url::Url::parse` restricted the way the Rust callers use it; null when unparsable. */
function parseUrl(text) {
  try {
    return new URL(text);
  } catch {
    return null;
  }
}

/** Url parses, scheme http(s), host present. */
export function isHttpUrl(text) {
  const url = typeof text === 'string' ? parseUrl(text) : null;
  return url != null && (url.protocol === 'http:' || url.protocol === 'https:') && url.hostname !== '';
}

/** Rust `trim_start_matches("www.")` — repeated prefix. */
function trimWww(host) {
  let h = host;
  while (h.startsWith('www.')) h = h.slice(4);
  return h;
}

/** `match value { String => trim, Number => to_string, _ => "" }` */
function scalarText(value, trim = true) {
  if (typeof value === 'string') return trim ? value.trim() : value;
  if (typeof value === 'number') return String(value);
  return null;
}

function blankStatus(status, reason) {
  return { status, value: null, sources: [], attempts: [], reason, extra: {} };
}

/** ref: person_research_command.rs:2411 */
export function safeWorkspaceSegment(value) {
  const segment = Array.from(String(value)).map((ch) => (/^[A-Za-z0-9_-]$/.test(ch) ? ch : '-')).join('');
  const trimmed = segment.replace(/^-+|-+$/g, '');
  return trimmed === '' ? 'research' : Array.from(trimmed).slice(0, 120).join('');
}

/** ref: :3979 */
export function requiredContractString(value, key) {
  const v = get(value, key);
  if (typeof v === 'string' && v.trim() !== '') return v.trim();
  return fail('invalid', `gap closure contract ${key} is required`);
}

/** ref: :2357 — `original` = {command_type, module, record_id} | null (ctx.cap.command.get). */
export function validateOriginalResearchCommand(original, request) {
  if (original == null) fail('invalid', `research_command_id \`${request.research_command_id}\` does not exist`);
  // outbound.lead.research = Recherche-Auftrag der App (Strang SKL-C); business_os.chat.task nur noch für Alt-Aufträge.
  if (!['web_stack.person_research', 'outbound.lead.research', 'business_os.chat.task'].includes(original.command_type)) {
    fail('invalid', 'research_command_id must reference web_stack.person_research, outbound.lead.research or the chat assignment (business_os.chat.task)');
  }
  if (!(original.module === request.module && original.record_id === request.record_id)) {
    fail('invalid', 'research_command_id does not belong to the same module and record_id');
  }
}

/** ref: :2395 */
export function validateTaskCorrelation(contract, request) {
  const s = (k) => (typeof get(contract, k) === 'string' ? contract[k] : undefined);
  if (s('record_id') !== request.record_id) fail('invalid', 'gap task record_id does not match writeback');
  if (s('research_command_id') !== request.research_command_id) fail('invalid', 'gap task research_command_id does not match writeback');
  if (s('module') !== request.module) fail('invalid', 'gap task module does not match writeback');
  if (s('gap_task_id') !== request.gap_task_id) fail('invalid', 'gap task id does not match writeback');
}

/**
 * ref: :3950 task_workspace. ABWEICHUNG v4: der Pfad ist relativ zum CTOX-Root
 * (`task.workspace_root_rel` aus `cap.agent.task.get`), Rust vergleicht absolute Pfade.
 */
export function taskWorkspace(task, contract) {
  const researchCommandId = requiredContractString(contract, 'research_command_id');
  const expected = `runtime/research/person/${safeWorkspaceSegment(researchCommandId)}`;
  const actual = typeof task?.workspace_root_rel === 'string' ? task.workspace_root_rel : null;
  if (actual == null) fail('invalid', 'gap task workspace_root is missing');
  if (actual !== expected) fail('invalid', 'gap task workspace_root does not match the Phase-A workspace');
  return expected;
}

/**
 * ref: :3514 validate_artifact_path. Dateifakten über das Kernprimitiv (§16.30 flach)
 * `ctx.cap.agent.task.workspace.read({task_id, path}) → {text, sha256}`: der Kern kanonisiert den Pfad und verweigert
 * Ausbrüche aus dem Task-Workspace (Fehler); Verzeichnis oder fehlende Datei = Fehler. „Direkt unter dem
 * Versuchsverzeichnis“ prüft die App am normalisierten Pfad (keine `..`-Segmente, genau vier Komponenten).
 * ABWEICHUNG zu Rust: ein Symlink im Versuchsverzeichnis auf eine andere Datei IM Workspace ist nicht erkennbar
 * (workspace.read liefert keinen kanonischen Pfad); Ausbrüche aus dem Workspace weist der Kern ab.
 */
export async function validateArtifactPath(ctx, taskId, field, artifactPath) {
  const trimmed = String(artifactPath).trim();
  const requiredPrefix = `gap_closure/attempts/${field}/`;
  if (!(trimmed !== '' && !trimmed.startsWith('/') && trimmed.startsWith(requiredPrefix))) {
    fail('invalid', `field \`${field}\` artifact_path must be under \`${requiredPrefix}\``);
  }
  // Path::file_name: last normal component (a trailing `/` is ignored; `..` has none).
  const parts = trimmed.split('/').filter((p) => p !== '' && p !== '.');
  const fileName = parts.length && parts[parts.length - 1] !== '..' ? parts[parts.length - 1] : null;
  if (fileName == null) fail('invalid', 'attempt artifact_path must end in a UTF-8 file name');
  const stem = fileName.endsWith('.json') ? fileName.slice(0, -5) : null;
  // Rust `usize::from_str`: optional leading `+`, ASCII digits only.
  const sequence = stem != null && /^\+?[0-9]+$/.test(stem) ? Number(stem.replace(/^\+/, '')) : null;
  if (!(sequence != null && sequence > 0)) fail('invalid', `field \`${field}\` artifact_path must end in a positive numeric \`<n>.json\``);
  if (!(parts.length === 4 && parts[0] === 'gap_closure' && parts[1] === 'attempts' && parts[2] === field)) {
    fail('invalid', `field \`${field}\` artifact_path must remain directly under \`${requiredPrefix}\``);
  }
  let artifact;
  try {
    artifact = await ctx.cap.agent.task.workspace.read({ task_id: taskId, path: parts.join('/') });
  } catch (error) {
    fail('invalid', `field \`${field}\` artifact_path \`${artifactPath}\` does not exist: ${error?.message || error}`);
  }
  if (!(typeof artifact?.text === 'string' && artifact.text.length > 0)) {
    fail('invalid', `field \`${field}\` artifact_path must be an existing non-empty file`);
  }
}

/** ref: :3494 */
export async function validateAttempt(ctx, taskId, field, attempt) {
  if (!['web_search', 'web_read', 'browser_capture', 'adapter'].includes(attempt.kind)) {
    fail('invalid', `field \`${field}\` has unsupported attempt kind \`${attempt.kind}\``);
  }
  if (String(attempt.query_or_url).trim() === '') fail('invalid', `field \`${field}\` attempt query_or_url must not be empty`);
  if (attempt.at === null || attempt.at === undefined) fail('invalid', `field \`${field}\` attempt at must not be null`);
  await validateArtifactPath(ctx, taskId, field, attempt.artifact_path);
}

/** serde_json `Value::pointer` */
function pointer(value, path) {
  let cursor = value;
  for (const key of path.split('/').slice(1)) {
    if (Array.isArray(cursor) && /^(0|[1-9][0-9]*)$/.test(key)) cursor = cursor[Number(key)];
    else if (has(cursor, key)) cursor = cursor[key];
    else return undefined;
    if (cursor === undefined) return undefined;
  }
  return cursor;
}

/** ref: :3562 */
export function actionRequiredHasAuthReference(status, contract) {
  if (status.sources.some((s) => s.source_id.trim() !== '' && (s.requires_credential || s.task_id.trim() !== '' || s.command_id.trim() !== ''))) {
    return true;
  }
  const attempted = Array.isArray(get(contract, 'attempted_sources')) ? contract.attempted_sources : [];
  if (status.sources.some((s) => attempted.some((a) => get(a, 'source_id') === s.source_id && get(a, 'requires_credential') === true))) {
    return true;
  }
  return status.attempts.some((attempt) => {
    const r = attempt.result;
    let sid = get(r, 'source_id');
    if (sid === undefined) sid = pointer(r, '/auth_assist/source_id');
    const sourceId = typeof sid === 'string' ? sid : '';
    const taskOrCommand = ['/task_id', '/command_id', '/auth_assist/task_id', '/auth_assist/command_id']
      .some((p) => nonBlank(pointer(r, p)));
    return sourceId.trim() !== '' && taskOrCommand;
  });
}

/** ref: :3096 — `taskId` identifies the gap task whose workspace holds the attempt artifacts. */
export async function validateTerminalField(ctx, taskId, field, status, contract) {
  if (!TERMINAL_FIELD_STATUSES.includes(status.status)) {
    fail('invalid', `field \`${field}\` has non-terminal or unsupported status \`${status.status}\``);
  }
  for (const attempt of status.attempts) await validateAttempt(ctx, taskId, field, attempt);
  switch (status.status) {
    case 'verified': {
      if (!researchValueIsPopulated(status.value)) fail('invalid', `verified field \`${field}\` requires a populated scalar value`);
      const evidence = status.sources.map((s) => ({ field_key: field, source_id: s.source_id, source_url: s.url, quote: s.quote, person_key: s.person_key }));
      for (const source of status.sources) {
        validateSource(field, source);
        if (field.startsWith('person_') && !nonBlank(source.person_key)) {
          fail('invalid', `verified person field \`${field}\` evidence requires person_key`);
        }
      }
      const required = requiredIndependentSources(field);
      if (!(independentResearchEvidenceCount(evidence, field) >= required)) {
        fail('invalid', `verified field \`${field}\` requires at least ${required} independent sources on different hosts`);
      }
      break;
    }
    case 'no_match': validateNoMatch(field, status); break;
    case 'unsupported': break;
    case 'action_required':
      if (!actionRequiredHasAuthReference(status, contract)) {
        fail('invalid', `action_required field \`${field}\` requires an auth-assist reference or requires_credential source`);
      }
      break;
    default: break;
  }
}

/** ref: :283 — Rust inserts into the BTreeMap `extra` (sorted) and into the person status maps (preserve_order). */
export function stampWritebackFieldRevisions(request, revision) {
  for (const status of Object.values(request.field_status)) {
    status.extra.review = null;
    status.extra.revision = clone(revision);
    sortKeys(status.extra);
  }
  const people = request.result.person_field_status;
  if (isObject(people)) {
    for (const fields of Object.values(people).filter(isObject)) {
      for (const status of Object.values(fields).filter(isObject)) {
        status.review = null;
        status.revision = clone(revision);
      }
    }
  }
}

/**
 * ref: :2434 sanitize_research_writeback — mutates `request`, returns the rejection list.
 * `crm` is a CrmBaseline (research-validation.js).
 */
export function sanitizeResearchWriteback(request, requestedFields, crm) {
  const rejections = [];
  const requested = new Set(requestedFields);
  const verworfeneFelder = new Set();

  const fields = request.result.fields;
  if (!isObject(fields)) fail('invalid', 'research writeback result.fields must be an object');
  const entfernen = [];
  for (const [field, value] of Object.entries(fields)) {
    let grund = null;
    if (!requested.has(field)) grund = 'nicht angefordert';
    else if (fieldKeyFromStr(field) == null) grund = 'kein bekanntes Recherchefeld';
    else if (!isObject(value)) grund = 'kein strukturiertes Objekt';
    else if (has(value, 'value') && value.value !== null && !researchValueIsPopulated(value.value)) {
      grund = 'Wert ist weder leer noch ein befuellter Skalar';
    } else if (field.startsWith('person_') && has(value, 'value') && researchValueIsPopulated(value.value) && !nonBlank(get(value, 'person_key'))) {
      grund = 'person_key fehlt';
    }
    if (grund != null) {
      rejections.push(`result.fields.${field}: ${grund}`);
      entfernen.push(field);
    }
  }
  // RUST-EIGENHEIT: `Map::remove` unter preserve_order = swap_remove; die Reihenfolge der
  // verbliebenen Ergebnisfelder ist für spätere Iterationen (Projektion) sichtbar.
  for (const field of entfernen) {
    swapRemoveKey(fields, field);
    verworfeneFelder.add(field);
  }

  for (const [field, status] of Object.entries(request.field_status)) {
    if (status.status === 'verified') continue;
    let dropped = false;
    if (researchValueIsPopulated(status.value)) {
      status.value = null;
      dropped = true;
    }
    const entry = get(fields, field);
    if (isObject(entry) && has(entry, 'value') && researchValueIsPopulated(entry.value)) {
      entry.value = null;
      dropped = true;
    }
    if (dropped) rejections.push(`result.fields.${field}: Wert ohne verifizierten Status verworfen (${status.status})`);
  }

  const vorher = request.result.person_records.length;
  request.result.person_records = request.result.person_records.filter((p) => nonBlank(get(p, 'person_key')));
  if (request.result.person_records.length !== vorher) {
    rejections.push(`result.person_records: ${vorher - request.result.person_records.length} Eintraege ohne person_key verworfen`);
  }

  const belege = [];
  request.result.evidence.forEach((evidence, index) => {
    const rawField = has(evidence, 'field_key') ? evidence.field_key : get(evidence, 'field');
    const field = typeof rawField === 'string' ? rawField : null;
    const rawUrlValue = has(evidence, 'url') ? evidence.url : get(evidence, 'source_url');
    const rawUrl = typeof rawUrlValue === 'string' && rawUrlValue.trim() !== '' ? rawUrlValue.trim() : null;
    const quoteText = typeof get(evidence, 'quote') === 'string' ? evidence.quote : '';
    const crmCitation = field != null && rawUrl != null ? crm.check(field, rawUrl, quoteText) : null;
    const urlOk = crmCitation === true || (rawUrl != null && isHttpUrl(rawUrl));
    let grund = null;
    if (field == null) grund = 'field_key fehlt';
    else if (!requested.has(field)) grund = `Feld ${field} war nicht angefordert`;
    else if (verworfeneFelder.has(field)) grund = `Feld ${field} wurde bereits verworfen`;
    else {
      let value = typeof get(evidence, 'value') === 'string' ? evidence.value : null;
      if (value == null) {
        const status = get(request.field_status, field);
        value = status ? scalarText(status.value, false) : null;
      }
      if (!quoteBacksValue(field, value ?? '', quoteText)) {
        grund = `Zitat stuetzt den Wert von ${field} nicht (nur Spanne oder andere Zahl)`;
      } else if (fieldKeyFromStr(field) == null) grund = `Feld ${field} ist kein bekanntes Recherchefeld`;
      else if (crmCitation === false) {
        grund = `Sellify-Beleg passt zu keinem Wert, den der Auftrag aus Sellify mitgebracht hat (${crm.hint()})`;
      } else if (!urlOk) grund = 'Beleg-URL ist keine http(s)-Adresse';
      else if (!nonBlank(get(evidence, 'source_id'))) grund = 'source_id fehlt';
      else if (!nonBlank(get(evidence, 'quote'))) grund = 'Zitat fehlt';
      else if (field.startsWith('person_') && !nonBlank(get(evidence, 'person_key'))) grund = 'person_key fehlt';
    }
    if (grund != null) rejections.push(`result.evidence[${index}]: ${grund}`);
    else belege.push(clone(evidence));
  });
  request.result.evidence = belege;

  for (const [field, status] of Object.entries(request.field_status)) {
    const value = scalarText(status.value) ?? '';
    const before = status.sources.length;
    let contradicted = false;
    status.sources = status.sources.filter((source) => {
      const c = crm.check(field, source.url, source.quote);
      if (c === null) return true;
      if (c === false) return false;
      const backs = value === '' || crm.valueAgrees(field, source.url, value);
      contradicted = contradicted || !backs;
      return backs;
    });
    const vorMengenpruefung = status.sources.length;
    status.sources = status.sources.filter((source) => quoteBacksValue(field, value, source.quote));
    const mengenVerworfen = vorMengenpruefung - status.sources.length;
    if (mengenVerworfen > 0) {
      rejections.push(`field_status.${field}: ${mengenVerworfen} Beleg(e) verworfen (Zitat nennt den Wert nicht, nur eine Spanne oder eine andere Zahl)`);
    }
    const dropped = before - vorMengenpruefung;
    if (dropped > 0) {
      rejections.push(`field_status.${field}: ${dropped} Sellify-Beleg(e) verworfen (${contradicted ? 'Sellify fuehrt fuer dieses Feld einen anderen Wert' : crm.hint()})`);
    }
  }
  for (const person of request.result.person_records) {
    if (!isObject(person)) continue;
    const personEmail = typeof get(person, 'person_email') === 'string' ? person.person_email : '';
    for (const list of ['evidence', 'sources']) {
      const entries = get(person, list);
      if (!Array.isArray(entries)) continue;
      const before = entries.length;
      person[list] = entries.filter((entry) => {
        const u = has(entry, 'url') ? entry.url : get(entry, 'source_url');
        const url = typeof u === 'string' ? u : '';
        const f = has(entry, 'field_key') ? entry.field_key : get(entry, 'field');
        const field = typeof f === 'string' ? f : '';
        const quote = typeof get(entry, 'quote') === 'string' ? entry.quote : '';
        return crm.check(field, url, quote) !== false && emailQuoteBacks(field, personEmail, quote);
      });
      if (person[list].length !== before) {
        rejections.push(`result.person_records: ${before - person[list].length} Beleg(e) verworfen (Sellify ohne Deckung oder Zitat nennt die E-Mail-Adresse nicht; ${crm.hint()})`);
      }
    }
  }

  const fieldPersonKeys = new Map();
  if (isObject(request.result.fields)) {
    for (const [field, entry] of Object.entries(request.result.fields)) {
      const pk = get(entry, 'person_key');
      if (typeof pk === 'string' && pk.trim() !== '') fieldPersonKeys.set(field, pk.trim());
    }
  }
  for (const [field, status] of Object.entries(request.field_status)) {
    if (status.status !== 'verified' || status.sources.some((s) => crm.check(field, s.url, s.quote) === true)) continue;
    const value = scalarText(status.value);
    if (value == null) continue;
    const personKey = fieldPersonKeys.get(field) ?? status.sources.map((s) => s.person_key).find((k) => k != null) ?? null;
    const source = crm.agreeingSource(field, value, personKey);
    if (source) status.sources.push(source);
  }

  const fieldsRo = isObject(request.result.fields) ? clone(request.result.fields) : {};
  const demotieren = [];
  for (const [field, status] of Object.entries(request.field_status)) {
    if (!requested.has(field)) {
      demotieren.push([field, 'nicht angefordert']);
      continue;
    }
    const eintrag = get(fieldsRo, field);
    const ergebniswert = get(eintrag, 'value');
    if (status.status !== 'verified') continue;
    if (verworfeneFelder.has(field)) {
      demotieren.push([field, 'Ergebnisfeld wurde verworfen']);
      continue;
    }
    if (ergebniswert !== undefined && ergebniswert !== null && !jsonEqual(ergebniswert, status.value)) {
      demotieren.push([field, 'Wert in result und field_status stimmen nicht ueberein']);
      continue;
    }
    const hosts = new Set();
    for (const source of status.sources) {
      const c = crm.check(field, source.url, source.quote);
      if (c === true) hosts.add(CRM_SOURCE_SCHEME);
      else if (c === null) {
        const url = parseUrl(source.url.trim());
        // `host_str()` → None for a URL without host (JS: hostname '').
        if (url && url.hostname !== '') hosts.add(asciiLower(trimWww(url.hostname)));
      }
    }
    const sortedHosts = [...hosts].sort(cmpStr);
    const reserved = sortedHosts.find((h) => hostIsReservedForDocumentation(h));
    if (reserved !== undefined) {
      demotieren.push([field, `Beleg vom Dokumentations-Host \`${reserved}\` beweist nichts`]);
      continue;
    }
    const benoetigt = requiredIndependentSources(field);
    const unabhaengig = crmAwareProviderCount(sortedHosts.length, sortedHosts);
    if (unabhaengig === 0 && sortedHosts.length) {
      demotieren.push([field, 'Sellify allein belegt nichts: eine externe Quelle muss den Wert bestaetigen']);
      continue;
    }
    if (unabhaengig < benoetigt) {
      demotieren.push([field, `verified verlangt ${benoetigt} unabhaengige Quell-Hosts, gefunden: ${sortedHosts.length}`]);
      continue;
    }
    if (field.startsWith('person_')) {
      const pk = get(eintrag, 'person_key');
      const personKey = typeof pk === 'string' && pk.trim() !== '' ? pk.trim() : null;
      if (personKey == null) {
        demotieren.push([field, 'person_key im Ergebnis fehlt']);
        continue;
      }
      if (!status.sources.every((s) => (typeof s.person_key === 'string' ? s.person_key.trim() : null) === personKey)) {
        demotieren.push([field, 'person_key in Beleg und Ergebnis nicht identisch']);
        continue;
      }
    }
  }
  for (const [field, grund] of demotieren) {
    rejections.push(`field_status.${field}: ${grund}`);
    verworfeneFelder.add(field);
    if (isObject(request.result.fields)) swapRemoveKey(request.result.fields, field);
    if (requested.has(field)) {
      if (!has(request.field_status, field)) {
        request.field_status[field] = blankStatus('', '');
        sortKeys(request.field_status);
      }
      const status = request.field_status[field];
      status.status = 'unsupported';
      status.value = null;
      status.sources = [];
      status.reason = 'Vom Rueckschreiben verworfen, siehe rejections';
    } else {
      delete request.field_status[field];
    }
  }

  for (const [field, status] of Object.entries(request.field_status)) {
    if (status.status === 'verified') continue;
    if (researchValueIsPopulated(status.value)) {
      rejections.push(`field_status.${field}: Wert auf nicht-verifiziertem Feld entfernt`);
      status.value = null;
    }
    const entry = isObject(request.result.fields) ? get(request.result.fields, field) : undefined;
    if (entry !== undefined && has(entry, 'value') && researchValueIsPopulated(entry.value)) {
      rejections.push(`result.fields.${field}: Wert auf nicht-verifiziertem Feld entfernt`);
      if (isObject(entry)) entry.value = null;
    }
  }

  for (const field of requestedFields) {
    if (has(request.field_status, field)) continue;
    rejections.push(`field_status.${field}: nicht geliefert`);
    request.field_status[field] = blankStatus('unsupported', 'Vom Rueckschreiben nicht geliefert');
    sortKeys(request.field_status);
  }
  return rejections;
}

/** serde_json `Map::remove` under preserve_order = swap_remove. */
export function swapRemoveKey(object, key) {
  if (!has(object, key)) return undefined;
  const keys = Object.keys(object);
  const index = keys.indexOf(key);
  const value = object[key];
  const order = keys.slice();
  const last = order.pop();
  if (last !== key) order[index] = last;
  const values = order.map((k) => [k, object[k]]);
  for (const k of keys) delete object[k];
  for (const [k, v] of values) object[k] = v;
  return value;
}

/**
 * ref: :2189 apply_custom_field_results. Hinweis (V4-PORT §15a Befund): im echten Writeback ist
 * `request.result.custom_fields` wegen des Hoists immer `{}` – treu übernommen.
 */
export function applyCustomFieldResults(lead, results, researchPayload, nowMs) {
  const rejections = [];
  if (!isObject(results) || Object.keys(results).length === 0) return rejections;
  const defined = new Set(Array.isArray(get(researchPayload, 'custom_fields'))
    ? researchPayload.custom_fields.map((f) => get(f, 'key')).filter((k) => typeof k === 'string') : []);
  if (!isObject(lead.payload)) lead.payload = {};
  if (!isObject(lead.payload.custom_field_status)) lead.payload.custom_field_status = {};
  const evidence = [];
  for (const [key, entry] of Object.entries(results)) {
    if (!defined.has(key)) {
      rejections.push(`result.custom_fields.${key}: kein angelegtes Feld`);
      continue;
    }
    if (!isObject(entry)) {
      rejections.push(`result.custom_fields.${key}: kein strukturiertes Objekt`);
      continue;
    }
    const value = has(entry, 'value') && researchValueIsPopulated(entry.value) ? clone(entry.value) : null;
    const sources = Array.isArray(entry.sources)
      ? entry.sources.filter((s) => (nonBlank(get(s, 'url')) || nonBlank(get(s, 'source_url'))) && nonBlank(get(s, 'quote'))).map(clone)
      : [];
    const reason = typeof entry.reason === 'string' ? entry.reason.trim() : '';
    const status = value != null ? (sources.length ? 'verified' : 'action_required') : (reason !== '' ? 'no_match' : 'action_required');
    for (const source of sources) {
      evidence.push({
        field_key: key,
        value: value ?? null,
        source_id: has(source, 'source_id') ? source.source_id : null,
        source_url: has(source, 'url') ? source.url : has(source, 'source_url') ? source.source_url : null,
        quote: has(source, 'quote') ? source.quote : null,
        via: 'custom_field',
      });
    }
    lead.payload.custom_field_status[key] = { status, value: value ?? null, sources, reason, updated_at_ms: nowMs };
  }
  if (evidence.length) {
    if (!Array.isArray(lead.evidence)) lead.evidence = [];
    const keys = new Set(evidence.map((e) => e.field_key));
    lead.evidence = lead.evidence.filter((entry) => get(entry, 'via') !== 'custom_field'
      || !(typeof get(entry, 'field_key') === 'string' && keys.has(entry.field_key)));
    lead.evidence.push(...evidence);
  }
  return rejections;
}

/** ref: :3765 add_field_status_evidence — mutates `result.fields`. */
export function addFieldStatusEvidence(result, fieldStatus) {
  const fields = result.fields;
  if (!isObject(fields)) fail('invalid', 'projection result fields must be an object');
  for (const [field, status] of Object.entries(fieldStatus)) {
    if (status.status !== 'verified') continue;
    if (!has(fields, field)) fields[field] = {};
    const entry = fields[field];
    if (!isObject(entry)) fail('invalid', `verified result field \`${field}\` must be an object`);
    if (!has(entry, 'value') || entry.value === null) entry.value = clone(status.value);
    entry.candidates = status.sources.map((source) => ({
      value: clone(status.value),
      source_id: source.source_id,
      source_url: source.url,
      quote: source.quote,
      person_key: typeof source.person_key === 'string' && source.person_key.trim() !== ''
        ? source.person_key.trim() : (has(entry, 'person_key') ? clone(entry.person_key) : null),
      via: 'gap_closure',
    }));
  }
}

/** ref: web-stack OUTBOUND_RESEARCH_FIELDS (FieldKey order, Klon src/tools/web-stack/src/sources/mod.rs). */
export const OUTBOUND_RESEARCH_FIELDS = [
  'firma_name', 'firma_fruehere_namen', 'firma_aktivitaetsstatus', 'firma_anschrift', 'firma_besucheranschrift',
  'firma_postanschrift', 'firma_postfach', 'firma_plz', 'firma_ort', 'firma_land', 'firma_email', 'firma_domain',
  'firma_telefon', 'firma_fax', 'firma_geschaeftstaetigkeit', 'firma_homepage_fact_sheet', 'firma_geschaeftsfuehrung',
  'firma_prokura', 'wz_code', 'umsatz', 'mitarbeiter', 'crm_record_number', 'person_geschlecht', 'person_titel',
  'person_vorname', 'person_nachname', 'person_funktion', 'person_position', 'person_email', 'person_email_validation',
  'person_telefon', 'person_linkedin', 'person_xing',
];

/** ref: :3812 canonical_requested_fields */
export function canonicalRequestedFields(command, result) {
  let fields = Array.isArray(get(result, 'requested_fields')) ? result.requested_fields.filter((v) => typeof v === 'string') : [];
  if (!fields.length) {
    fields = Array.isArray(get(command.payload, 'fields')) ? command.payload.fields.filter((v) => typeof v === 'string') : [];
  }
  if (!fields.length) fields = OUTBOUND_RESEARCH_FIELDS.slice();
  for (const field of fields) {
    if (fieldKeyFromStr(field) == null) fail('invalid', `unsupported person-research field \`${field}\``);
  }
  return fields;
}

/** ref: :3850 phase_a_verified_sources */
export function phaseAVerifiedSources(evidence, field) {
  const out = [];
  for (const entry of evidence) {
    const f = has(entry, 'field_key') ? entry.field_key : get(entry, 'field');
    if (f !== field) continue;
    const u = has(entry, 'source_url') ? entry.source_url : get(entry, 'url');
    const url = typeof u === 'string' && u.trim() !== '' ? u.trim() : null;
    if (url == null || !isHttpUrl(url)) continue;
    const sid = get(entry, 'source_id');
    const sourceId = typeof sid === 'string' && sid.trim() !== '' ? sid.trim() : null;
    if (sourceId == null) continue;
    const q = has(entry, 'quote') ? entry.quote : has(entry, 'note') ? entry.note : get(entry, 'value');
    const quote = typeof q === 'string' ? q : (q !== undefined && q !== null ? JSON.stringify(q) : null);
    if (quote == null || quote.trim() === '') continue;
    out.push({ field_key: field, source_id: sourceId, source_url: url, url, quote, person_key: has(entry, 'person_key') ? clone(entry.person_key) : null });
  }
  return out;
}

/** ref: :3899 phase_a_attempted_sources */
export function phaseAAttemptedSources(result) {
  const attempted = [];
  for (const [key, kind] of [['plan', 'adapter'], ['search_runs', 'web_search'], ['read_runs', 'web_read'], ['scrape_runs', 'web_read'],
    ['browser_extract_runs', 'browser_capture'], ['authenticated_source_capture_runs', 'browser_capture']]) {
    const list = get(result, key);
    if (!Array.isArray(list)) continue;
    for (const raw of list) {
      const entry = clone(raw);
      if (isObject(entry) && !has(entry, 'attempt_kind')) entry.attempt_kind = kind;
      attempted.push(entry);
    }
  }
  return attempted;
}

const FIELD_DEFINITIONS = {
  firma_name: 'aktuelle rechtliche Firmierung',
  firma_fruehere_namen: 'belegte frühere Firmierungen',
  firma_aktivitaetsstatus: 'aktueller Register-/Geschäftsstatus',
  firma_anschrift: 'offizielle Hauptanschrift',
  firma_besucheranschrift: 'öffentlich ausgewiesene Besucheranschrift',
  firma_postanschrift: 'offizielle Postanschrift',
  firma_postfach: 'Postfachangabe',
  firma_plz: 'Postleitzahl der maßgeblichen Firmenanschrift',
  firma_ort: 'Ort der maßgeblichen Firmenanschrift',
  firma_land: 'Land der maßgeblichen Firmenanschrift',
  firma_email: 'veröffentlichte zentrale Firmen-E-Mail-Adresse',
  firma_domain: 'kanonische Unternehmensdomain',
  firma_telefon: 'veröffentlichte zentrale Firmentelefonnummer',
  firma_fax: 'veröffentlichte zentrale Faxnummer',
  firma_geschaeftstaetigkeit: 'sachliche Beschreibung der Geschäftstätigkeit',
  firma_homepage_fact_sheet: 'strukturierte Kernaussagen der offiziellen Homepage',
  firma_geschaeftsfuehrung: 'aktuell belegte Geschäftsführung',
  firma_prokura: 'aktuell belegte Prokuristinnen und Prokuristen',
  wz_code: 'belegter Wirtschaftszweig-/WZ-Code',
  umsatz: 'aktuellster belegbarer Umsatz mit Zeitraum und Einheit',
  mitarbeiter: 'aktuellste belegbare Mitarbeiterzahl mit Zeitraum',
  crm_record_number: 'Sellify-/CRM-Datensatznummer',
  person_geschlecht: 'belegbare Anrede-/Geschlechtsangabe der Person',
  person_titel: 'belegter akademischer oder beruflicher Titel',
  person_vorname: 'belegter Vorname der priorisierten Person',
  person_nachname: 'belegter Nachname der priorisierten Person',
  person_funktion: 'belegte organisatorische Funktion der Person',
  person_position: 'belegte konkrete Stellen-/Positionsbezeichnung',
  person_email: 'belegte geschäftliche E-Mail-Adresse der Person',
  person_email_validation: 'belegter Validierungsstatus der Personen-E-Mail',
  person_telefon: 'belegte geschäftliche Telefonnummer der Person',
  person_linkedin: 'kanonische LinkedIn-Profil-URL der Person',
  person_xing: 'kanonische XING-Profil-URL der Person',
};

/** ref: :3988 */
export function fieldDefinition(field) {
  return has(FIELD_DEFINITIONS, field) ? FIELD_DEFINITIONS[field] : 'Wert gemäß dem kanonischen CTOX-Personenrecherche-Feldvokabular';
}
