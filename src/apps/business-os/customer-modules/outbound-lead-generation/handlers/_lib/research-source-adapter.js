// Shared port of `outbound_handle_research_source_adapter` for the four
// research_source command types (upsert / generate_adapter / test / auth_assist).
//
// ref: store_outbound_commands.rs:2776-3035 outbound_handle_research_source_adapter
//      :3037-3099 outbound_apply_research_adapter_writeback
//      :3101-3548 outbound_apply_research_adapter_scrape_effect
//      :3550-3757 test_passed / test_evidence / artifact_evidence / persist_*
//      :3759-3871 outbound_queue_research_scraper_generation (prompt = Fachlogik)
//      :3873-3931 outbound_apply_research_adapter_credential_status
//      :3933-4006 outbound_register_research_scrape_target
//      :4008-4088 test_input / operation_timeout_ms / execution_args
//      :4239-4275 write_research_scrape_manifest / safe_path_component
//
// Fachlogik (record maintenance, status transitions, test classification,
// evidence rules, generator prompt, writeback scoping) lives here. The scrape
// registry is app data since Strang OLGSCR (_lib/scrape-registry.js: targets,
// script revisions in `files`, runs, account states); a test run goes through
// _lib/scrape-runner.js (ctx.cap.process.run + ctx.cap.files.*). Remaining
// generic capabilities:
//   ctx.cap.agent.task.enqueue({title, instruction, skill, thread_key, priority, idempotency_key, payload}) → {task_id, status?}
//   ctx.cap.secret.scope() → {scope};  ctx.cap.secret.exists({scope, name}) → {exists}
//   ctx.cap.browser.auth_assist.request({...}) → effect
//
// RUST-EIGENHEIT markers flag behavior kept on purpose (treue Portierung, §16.19 Nr. 9).
import { asI64, asU64, fail, firstString, has, isObject, objectPayload, str, utf8Len } from './olg.js';
import { TARGETS, registeredTargetSummary, targetScriptRegistration, upsertTarget } from './scrape-registry.js';
import { executeScrape, runFileFacts } from './scrape-runner.js';

const ADAPTERS = 'outbound_research_adapters';
export const GENERATION_DONE = 'outbound.research_source.generation.done';

// ---------------------------------------------------------------- record helpers
// ref: store.rs:18358-18410

export function putString(record, key, value) {
  if (isObject(record)) record[key] = String(value);
}
export function putI64(record, key, value) {
  if (isObject(record)) record[key] = value;
}
export function putDefaultString(record, key, value) {
  if (str(record, [key]) == null) putString(record, key, value);
}
export function putDefaultI64(record, key, value) {
  // as_i64 / as_u64: any integral number counts as present.
  const current = isObject(record) ? record[key] : undefined;
  if (!(typeof current === 'number' && Number.isInteger(current))) putI64(record, key, value);
}
export function putDefaultObject(record, key) {
  if (!isObject(isObject(record) ? record[key] : undefined) && isObject(record)) record[key] = {};
}
export function mergeFields(record, patch, keys) {
  if (!isObject(record)) return;
  for (const key of keys) if (has(patch, key)) record[key] = clone(patch[key]);
}
/** ref: store_outbound_commands.rs:5930 outbound_payload_insert */
export function payloadInsert(record, key, value) {
  if (!isObject(isObject(record) ? record.payload : undefined)) putDefaultObject(record, 'payload');
  if (isObject(record) && isObject(record.payload)) record.payload[key] = clone(value);
}

function clone(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

/** serde `Value::get(key)` on a possibly non-object value → value or undefined. */
function get(value, key) {
  return has(value, key) ? value[key] : undefined;
}

/** serde_json Value equality: objects compare as maps (order-insensitive). */
export function jsonEqual(a, b) {
  if (a === b) return true;
  if (Array.isArray(a) || Array.isArray(b)) {
    return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((v, i) => jsonEqual(v, b[i]));
  }
  if (isObject(a) && isObject(b)) {
    const ka = Object.keys(a);
    return ka.length === Object.keys(b).length && ka.every((k) => has(b, k) && jsonEqual(a[k], b[k]));
  }
  return false;
}

const asciiLower = (text) => String(text).replace(/[A-Z]/g, (c) => c.toLowerCase());

/**
 * Rust `{:?}` of Vec<String>: `["a", "b"]`.
 * RUST-EIGENHEIT: Debug escaping is approximated for \\ " \n \r \t and other
 * control characters (\u{..}); printable Unicode stays verbatim like in Rust.
 */
export function rustDebugStrings(items) {
  const esc = (s) => String(s).replace(/[\\"\n\r\t\u0000-\u001f\u007f]/g, (c) => {
    if (c === '\\') return '\\\\';
    if (c === '"') return '\\"';
    if (c === '\n') return '\\n';
    if (c === '\r') return '\\r';
    if (c === '\t') return '\\t';
    return `\\u{${c.codePointAt(0).toString(16)}}`;
  });
  return `[${items.map((s) => `"${esc(s)}"`).join(', ')}]`;
}

/** ref: store.rs:8090 runtime_app_starter_collection_name */
export function runtimeAppStarterCollectionName(moduleId) {
  let out = '';
  let lastUnderscore = false;
  for (const ch of String(moduleId).trim().toLowerCase()) {
    if (/^[a-z0-9]$/.test(ch)) {
      out += ch;
      lastUnderscore = false;
    } else if (!lastUnderscore) {
      out += '_';
      lastUnderscore = true;
    }
  }
  const prefix = out.replace(/^_+|_+$/g, '');
  return prefix ? `${prefix}_records` : 'agent_app_records';
}

/** ref: store.rs:14161 */
export function isSafeRxdbCollectionName(collection) {
  return collection !== '' && /^[a-z0-9_]+$/.test(collection);
}

/** ref: :4261 outbound_safe_path_component */
export function safePathComponent(value) {
  let out = '';
  for (const ch of String(value)) out += /^[A-Za-z0-9._-]$/.test(ch) ? ch : '-';
  return out || 'adapter';
}

/**
 * v4 commands carry no `client_context` (§16.16). Rust reads campaign_id,
 * actor/id, user_id and source_module from it. ABWEICHUNG: an optional
 * `command.client_context` is honoured when the runtime passes it; otherwise
 * the v4 fields stand in (actor.id, app_id) at the call sites below.
 */
function clientContext(command) {
  return isObject(command.client_context) ? command.client_context : {};
}

// ---------------------------------------------------------------- test input

const TIMEOUT_MESSAGE = 'test_input.operation_timeout_ms must be an integer between 1000 and 300000';

/** ref: :4054 — `None` → 90 000; otherwise u64 in 1 000..=300 000. */
export function scrapeOperationTimeoutMs(value) {
  if (value === undefined) return 90_000;
  const timeout = asU64(value);
  if (timeout == null) fail('invalid', TIMEOUT_MESSAGE);
  if (timeout < 1_000 || timeout > 300_000) fail('invalid', TIMEOUT_MESSAGE);
  return timeout;
}

/** ref: :4008 outbound_research_scrape_test_input */
export function researchScrapeTestInput(command, adapterPayload, sourceId) {
  const payload = command.payload;
  const company = firstString([str(payload, ['test_input', 'company']), str(payload, ['company'])]);
  if (company == null) {
    fail('invalid', 'Provide a non-empty test_input.company (or company) before testing this research adapter');
  }
  const countries = get(adapterPayload, 'countries');
  const firstCountry = Array.isArray(countries) && typeof countries[0] === 'string' ? countries[0] : null;
  const country = firstString([str(payload, ['test_input', 'country']), str(payload, ['country']), firstCountry]) ?? 'DE';
  const testInput = get(payload, 'test_input');
  const operationTimeoutMs = scrapeOperationTimeoutMs(has(testInput, 'operation_timeout_ms') ? testInput.operation_timeout_ms : undefined);
  const taskId = typeof command.id === 'string' && command.id.trim() !== '' ? command.id.trim() : null;
  const input = {
    company,
    country,
    source_id: sourceId,
    adapter_test: true,
    // Correlation only: the auth service must authorize this command.
    task_id: taskId,
    operation_timeout_ms: operationTimeoutMs,
  };
  const email = str(payload, ['test_input', 'email']);
  // `email.len()` = UTF-8 bytes; `split('@').count() == 2` = exactly one '@'.
  if (email != null && utf8Len(email) <= 254 && email.split('@').length === 2) input.email = email;
  return input;
}

/**
 * The generator removes `task_id` with serde_json `Map::remove`, which under
 * the workspace-wide `preserve_order` feature is `swap_remove`.
 * RUST-EIGENHEIT: the last key moves into task_id's slot, so the prompt shows
 * `…adapter_test, operation_timeout_ms` or `…adapter_test, email, operation_timeout_ms`.
 */
export function swapRemove(object, key) {
  const keys = Object.keys(object);
  const index = keys.indexOf(key);
  if (index < 0) return object;
  const last = keys[keys.length - 1];
  const order = keys.slice();
  if (last !== key) order[index] = last;
  order.pop();
  const out = {};
  for (const k of order) out[k] = object[k];
  return out;
}

// ---------------------------------------------------------------- evidence

/** File facts of a run artifact (files ref); an empty path is `is_file() == false` without a call. */
async function runFile(ctx, path) {
  if (!path) return { is_file: false, sha256: '', json: null };
  try {
    const facts = await runFileFacts(ctx, path);
    return {
      is_file: facts?.is_file === true,
      sha256: typeof facts?.sha256 === 'string' ? facts.sha256 : '',
      json: facts?.json === undefined ? null : facts.json,
    };
  } catch {
    // Rust: fs::read/file_sha256 errors are swallowed (`.ok()`, unwrap_or_default).
    return { is_file: false, sha256: '', json: null };
  }
}

/** ref: :3658 outbound_scrape_artifact_evidence */
export async function scrapeArtifactEvidence(ctx, manifest, artifactKind, expectedRecordCount) {
  const artifacts = Array.isArray(get(manifest, 'artifacts')) ? manifest.artifacts : null;
  const artifact = artifacts ? artifacts.find((a) => get(a, 'artifact_kind') === artifactKind) : undefined;
  const path = typeof get(artifact, 'path') === 'string' ? artifact.path : '';
  const declaredSha256 = typeof get(artifact, 'content_sha256') === 'string' ? artifact.content_sha256 : '';
  const recordCount = asI64(get(artifact, 'record_count'));
  const facts = await runFile(ctx, path);
  const actualSha256 = facts.sha256;
  const recordCountMatches = expectedRecordCount == null ? true : recordCount === expectedRecordCount;
  const valid = facts.is_file && declaredSha256 !== '' && declaredSha256 === actualSha256 && recordCountMatches;
  return {
    evidence: {
      valid,
      artifact_kind: artifactKind,
      path,
      exists: facts.is_file,
      declared_sha256: declaredSha256,
      actual_sha256: actualSha256,
      sha256_matches: declaredSha256 !== '' && declaredSha256 === actualSha256,
      record_count: recordCount,
      record_count_matches: recordCountMatches,
    },
    json: facts.json,
  };
}

/** ref: :3562 outbound_scrape_test_evidence */
export async function scrapeTestEvidence(ctx, testEffect) {
  const s = (key) => (typeof get(testEffect, key) === 'string' ? testEffect[key] : '');
  const runId = s('run_id');
  const targetKey = s('target_key');
  const status = s('status');
  const recordsFound = asI64(get(testEffect, 'records_found')) ?? -1;
  const runManifestPath = s('run_manifest_path');
  const manifestFacts = await runFile(ctx, runManifestPath);
  const manifest = manifestFacts.json;
  const parsed = manifest !== null;
  const runIdMatches = parsed && get(manifest, 'run_id') === runId && runId !== '';
  const targetKeyMatches = parsed && get(manifest, 'target_key') === targetKey && targetKey !== '';
  const statusMatches = parsed && get(manifest, 'status') === status && status !== '';
  const recordCountMatches = parsed && asI64(get(get(manifest, 'result'), 'records_found')) === recordsFound && recordsFound >= 0;
  const result = await scrapeArtifactEvidence(ctx, parsed ? manifest : null, 'result_json', null);
  const records = await scrapeArtifactEvidence(ctx, parsed ? manifest : null, 'records_json', recordsFound);
  const query = await scrapeArtifactEvidence(ctx, parsed ? manifest : null, 'query_completion_evidence', 0);
  const capturedQueryReceipt = query.json;
  const testQuery = get(testEffect, 'query_completion');
  const manifestQuery = get(get(manifest, 'result'), 'query_completion');
  const queryReceiptMatches = status !== 'completed_empty'
    || (recordsFound === 0
      && isObject(testQuery)
      && manifestQuery !== undefined && jsonEqual(testQuery, manifestQuery)
      && capturedQueryReceipt !== null && jsonEqual(capturedQueryReceipt, testQuery)
      && query.evidence.valid === true);
  const valid = manifestFacts.is_file && parsed && runIdMatches && targetKeyMatches && statusMatches
    && recordCountMatches && queryReceiptMatches && result.evidence.valid === true && records.evidence.valid === true;
  return {
    valid,
    run_id: runId,
    run_manifest_path: runManifestPath,
    run_manifest_exists: manifestFacts.is_file,
    run_manifest_parsed: parsed,
    run_id_matches: runIdMatches,
    target_key_matches: targetKeyMatches,
    status_matches: statusMatches,
    record_count_matches: recordCountMatches,
    query_receipt_matches: queryReceiptMatches,
    query_completion_artifact: query.evidence,
    result_artifact: result.evidence,
    records_artifact: records.evidence,
  };
}

/** ref: :3550 */
export function scrapeTestPassed(status, recordsFound, hasExpectedFields, evidenceValid) {
  return status === 'succeeded' && recordsFound > 0 && hasExpectedFields && evidenceValid;
}

/** ref: :3707 */
export function persistScrapeTestObservation(record, testEffect, evidence) {
  const testOk = get(testEffect, 'test_ok') === true;
  const latencyMs = asU64(get(testEffect, 'latency_ms')) ?? 0;
  if (isObject(record)) {
    record.test_ok = testOk;
    record.last_test = clone(testEffect);
    record.latency_ms = latencyMs;
    record.evidence = clone(evidence);
  }
  payloadInsert(record, 'scrape_test_diagnostics', testEffect);
  payloadInsert(record, 'last_test', testEffect);
  payloadInsert(record, 'test_latency_ms', latencyMs);
  payloadInsert(record, 'test_evidence', evidence);
}

/** ref: :3730 */
export function persistScrapeTestPreflightFailure(record, status, scrapeStatus, error, latencyMs, nowMs) {
  putString(record, 'status', status);
  putString(record, 'scrape_status', scrapeStatus);
  putString(record, 'last_error', error);
  const evidence = { valid: false, reason: 'scrape test did not produce a durable run outcome' };
  const testEffect = {
    ok: false,
    test_ok: false,
    status: scrapeStatus,
    records_found: 0,
    latency_ms: latencyMs,
    error,
    evidence: clone(evidence),
    tested_at_ms: nowMs,
  };
  persistScrapeTestObservation(record, testEffect, evidence);
  return testEffect;
}

/** ref: :3441-3487 — failed-test status pair. */
export function failedTestStatus(outcome, evidenceValid) {
  const reason = typeof outcome.reason === 'string' ? outcome.reason : '';
  const recordsFound = asI64(outcome.records_found) ?? 0;
  if (reason === 'explicit_failure_mode_auth_required') return ['test_auth_required', 'test_auth_required'];
  if (reason.startsWith('command_failed_exit_')) return ['test_failed', 'test_error'];
  if (recordsFound === 0 && reason === 'empty_record_set_on_reachable_portal') return ['test_zero_records', 'test_zero_records'];
  switch (outcome.status) {
    case 'completed_empty': return ['test_evidence_invalid', 'test_evidence_invalid'];
    case 'authorization_required': return ['test_auth_required', 'test_auth_required'];
    case 'provider_account_inactive': return ['test_provider_account_inactive', 'test_provider_account_inactive'];
    case 'invalid_input': return ['test_invalid_input', 'test_invalid_input'];
    case 'blocked': return ['test_blocked', 'test_blocked'];
    case 'portal_drift': return ['test_portal_drift', 'test_portal_drift'];
    case 'temporary_unreachable': return ['test_temporary_unreachable', 'test_temporary_unreachable'];
    case 'partial_output': return ['test_partial_output', 'test_partial_output'];
    case 'succeeded':
      if (recordsFound === 0) return ['test_zero_records', 'test_zero_records'];
      if (!evidenceValid) return ['test_evidence_invalid', 'test_evidence_invalid'];
      return ['test_fields_missing', 'test_fields_missing'];
    default:
      // Rust's match is exhaustive over ScrapeRunStatus; an unknown string from
      // the capability is a contract violation.
      fail('unavailable', `scrape outcome has unknown status ${JSON.stringify(outcome.status)}`);
  }
  return null;
}

// ---------------------------------------------------------------- registration / generation

/** ref: :3933 outbound_register_research_scrape_target */
export async function registerResearchScrapeTarget(ctx, adapterPayload, record, sourceId, targetKey) {
  const url = firstString([str(record, ['url']), str(adapterPayload, ['url'])]);
  const registration = await targetScriptRegistration(ctx, targetKey);
  if (registration != null) return registration;
  let manifest;
  if (has(adapterPayload, 'target_manifest')) manifest = clone(adapterPayload.target_manifest);
  else if (has(get(record, 'payload'), 'target_manifest')) manifest = clone(record.payload.target_manifest);
  else {
    const displayName = firstString([str(record, ['label']), str(adapterPayload, ['label']), sourceId]) ?? sourceId;
    const startUrl = url ?? `https://${sourceId}/`;
    const countryHints = has(adapterPayload, 'countries') ? clone(adapterPayload.countries) : [];
    manifest = {
      target_key: targetKey,
      display_name: displayName,
      start_url: startUrl,
      target_kind: 'prospect-research',
      status: 'active',
      config: {
        expected_provider: sourceId,
        country_hints: countryHints,
        record_key_fields: ['field', 'source_url'],
      },
      output_schema: {
        schema_key: 'prospect.v1',
        primary_artifact_kind: 'field_records_json',
        record_key_fields: ['field', 'source_url'],
      },
    };
  }
  // Rust wrote runtime/scraping/outbound-adapters/<safe>/target.json and ran
  // `scrape upsert-target` (ref :4239 + :3992); the registry document is the manifest now.
  const upsert = await upsertTarget(ctx, manifest);
  return {
    ok: true,
    target_key: targetKey,
    registered_from: 'adapter_manifest',
    target_manifest: `${TARGETS}/${upsert.target_key}`,
    script_registered: false,
    next_step: 'generate or register scripts/v1.js with universal-scraping',
  };
}

/** ref: :3759 outbound_queue_research_scraper_generation — prompt is Fachlogik. */
export async function queueResearchScraperGeneration(ctx, command, adapterPayload, sourceId, targetKey) {
  const label = firstString([str(adapterPayload, ['label']), sourceId]) ?? sourceId;
  const url = str(adapterPayload, ['url']) ?? '';
  const pick = (key) => {
    if (has(adapterPayload, key)) return adapterPayload[key];
    const inner = get(adapterPayload, 'payload');
    if (has(inner, key)) return inner[key];
    if (has(command.payload, key)) return command.payload[key];
    return {};
  };
  const manifest = pick('target_manifest');
  const contract = pick('scrape_contract');
  const registration = await targetScriptRegistration(ctx, targetKey);
  if (registration == null) fail('unavailable', 'generation requires a registered scrape target');
  const workspace = get(registration, 'workspace_dir');
  if (typeof workspace !== 'string') fail('unavailable', 'registered scrape target has no workspace');
  const commandId = typeof command.id === 'string' ? command.id : '';
  const taskIdempotencyKey = commandId === ''
    ? `outbound-research-adapter:${targetKey}:legacy:${ctx.now()}`
    : `outbound-research-adapter:${targetKey}:${commandId}`;
  let testInput = null;
  try {
    testInput = swapRemove(researchScrapeTestInput(command, adapterPayload, sourceId), 'task_id');
  } catch {
    testInput = null; // Rust `.ok()`
  }
  let testInstruction;
  if (testInput) {
    const timeout = scrapeOperationTimeoutMs(testInput.operation_timeout_ms);
    const runnerSeconds = Math.ceil(timeout / 1000) + 30;
    testInstruction = `Schreibe exakt diesen Testinput in eine JSON-Datei unter ${workspace}/scripts/: ${JSON.stringify(testInput)}. Ergaenze task_id nur aus der tatsaechlichen eigenen Queue-Aufgabe des Harness, nie aus Target-Config oder geratenen IDs. Fuehre den Business-OS-Befehl \`outbound.scrape.execute\` (App outbound-lead-generation) mit {"target_key":"${targetKey}","timeout_seconds":${runnerSeconds},"input":<Inhalt der JSON-Datei>} aus. Dokumentiere Run-ID, Quellen und Felder.`;
  } else {
    testInstruction = 'Es fehlt ein gueltiger expliziter Testinput mit company. Registriere den Adapter, aber fuehre keinen Recherche-Test aus und erfinde keine Firma. Melde input_required.';
  }
  const prompt = 'Erzeuge oder repariere einen CTOX Universal-Scraping Adapter fuer die Outbound Research Quelle.\n'
    + 'Ziel:\n'
    + `- source_id: ${sourceId}\n`
    + `- label: ${label}\n`
    + `- url: ${url}\n`
    + `- target_key: ${targetKey}\n\n`
    + 'Anforderungen:\n'
    + '- Nutze den universal-scraping Skill.\n'
    + `- Arbeite ausschliesslich im isolierten Target-Workspace ${workspace}; bearbeite scripts/ und sources/. Aendere keine Datenbank, Elternverzeichnisse oder Runtime-Symlinks. Pruefe vorhandene registrierte Revisionen vor einer Neuerstellung.\n`
    + `- Das Target ist in der App-Registry registriert. Registriere den Script-Stand mit dem Business-OS-Befehl \`outbound.scrape.script.register\` (App outbound-lead-generation) mit {"target_key":"${targetKey}","script_text":<Skriptinhalt aus ${workspace}/scripts/>,"language":"javascript"}. Registrierung und Ausfuehrung laufen nur ueber diese App-Befehle; keine direkten Datenbankschreibzugriffe.\n`
    + '- Der Scraper muss `prospect.v1` Records ausgeben: field, value, confidence, source_url, note.\n'
    + '- Verwende keine Credential-Werte im Prompt, Code oder Log. Nur Secret-Referenzen sind erlaubt.\n'
    + '- Das Skript ist JavaScript (CommonJS oder ES-Modul) oder TypeScript, nie ein Shell-Skript (bash/sh werden nicht ausgefuehrt). Es laeuft in der Sandbox ueber den App-Host (olg-scrape-js bzw. olg-scrape-ts); stdin traegt das Host-Dokument, daher stdin nicht lesen; die CTOX_SCRAPE_*-Werte stehen in process.env. Anbieter-Konfiguration und Secret-Referenzen aus CTOX_SCRAPE_MANIFEST_JSON (config), die konkrete Abfrage aus CTOX_SCRAPE_INPUT_JSON, operation_timeout_ms fuer die Browseroperation, Ausgabedateien unter CTOX_SCRAPE_OUTPUT_DIR; Ergebnis als JSON auf stdout. Config und Owner-Angaben ersetzen keine native Autorisierung; bei Auth-Ablehnung keine alternativen Routen versuchen.\n'
    + `- ${testInstruction}\n\n`
    + `target_manifest:\n${JSON.stringify(manifest)}\n\n`
    + `scrape_contract:\n${JSON.stringify(contract)}\n`;
  // ref: channels::create_scrape_repair_queue_task → generisches agent.task.enqueue (flache Form,
  // Orchestrator-Vorgabe 15.10.): Skill und Anweisung sind App-Daten, Idempotenz über idempotency_key.
  // Rückweg (Strang SKL-C, §16.30): Abschluss, Fehlschlag und Abbruch als Befehl GENERATION_DONE an die App.
  const task = await ctx.cap.agent.task.enqueue({
    title: `Outbound Scraper Adapter erzeugen: ${label}`,
    instruction: prompt,
    skill: 'universal-scraping',
    thread_key: `business-os/outbound/research-adapter/${targetKey}`,
    // Adapter generation is background maintenance; it must not outrank the
    // research that needs the adapter (thesen 07.09.2026).
    priority: 'low',
    idempotency_key: taskIdempotencyKey,
    payload: {
      kind: 'scrape_repair',
      channel: 'scrape',
      target_key: targetKey,
      workspace_root: workspace,
      business_os_command_id: commandId,
      source: 'outbound.research_source.generate_adapter',
      adapter_source_id: sourceId,
      scrape_repair: { workspace_root: workspace },
    },
    on_complete: GENERATION_DONE,
    on_failed: GENERATION_DONE,
  });
  const taskId = task?.task_id ?? task?.message_key;
  return {
    ok: true,
    task_id: taskId,
    // A fresh task without an explicit route status is pending (Rust: route_status of the new task).
    task_status: task?.status ?? task?.route_status ?? (taskId ? 'pending' : undefined),
    target_key: targetKey,
    suggested_skill: 'universal-scraping',
    secret_value_in_payload: false,
  };
}

// ---------------------------------------------------------------- scrape effect

/** ref: :3101 outbound_apply_research_adapter_scrape_effect → effect | null */
export async function applyResearchAdapterScrapeEffect(ctx, command, adapterPayload, adapterId, sourceId, record) {
  const commandType = command.type;
  if (commandType !== 'outbound.research_source.generate_adapter' && commandType !== 'outbound.research_source.test') return null;
  const isTest = commandType === 'outbound.research_source.test';
  const started = ctx.now();
  const elapsed = () => Math.max(0, ctx.now() - started);
  if (isObject(record)) record.test_ok = false;
  const adapterKind = str(record, ['adapter_kind']) ?? str(adapterPayload, ['adapter_kind']) ?? 'custom_url';
  if (adapterKind !== 'scrape_target' && adapterKind !== 'custom_url') {
    return { ok: true, skipped: true, reason: 'source_is_not_scrape_target', adapter_kind: adapterKind };
  }
  // Validate identity before registration, scraping, or automatic healing.
  let testInput = null;
  if (isTest) {
    try {
      testInput = researchScrapeTestInput(command, adapterPayload, sourceId);
    } catch (error) {
      const message = error.message;
      const test = persistScrapeTestPreflightFailure(record, 'test_input_required', 'input_required', message, elapsed(), ctx.now());
      return {
        ok: false,
        phase: 'input',
        status: 'input_required',
        test_skipped: true,
        error: message,
        required_fields: ['test_input.company'],
        test,
      };
    }
  }
  const targetKey = firstString([str(record, ['target_key']), str(adapterPayload, ['target_key']), str(command.payload, ['target_key'])]);
  if (targetKey == null) {
    const message = 'target_key is required for scrape adapter';
    const effect = { ok: false, error: message };
    if (isTest) {
      effect.test = persistScrapeTestPreflightFailure(record, 'test_failed', 'target_key_missing', message, elapsed(), ctx.now());
    } else {
      putString(record, 'status', 'adapter_failed');
      putString(record, 'scrape_status', 'target_key_missing');
      putString(record, 'last_error', message);
    }
    return effect;
  }

  // A test observes the active registry revision. Registering the bundled
  // template here can silently replace a tenant's specialized script.
  let effect;
  try {
    if (isTest) {
      const target = await registeredTargetSummary(ctx, targetKey);
      if (target == null) {
        effect = await registerResearchScrapeTarget(ctx, adapterPayload, record, sourceId, targetKey);
      } else {
        const scriptRevisionNo = asI64(get(target, 'latest_script_revision_no'));
        effect = {
          ok: true,
          target_key: targetKey,
          registered_from: 'existing_registry',
          script_registered: scriptRevisionNo != null,
          script_revision_no: scriptRevisionNo,
          // RUST-EIGENHEIT: `target.get(...)` serialises a missing key as null.
          script_sha256: has(target, 'latest_script_sha256') ? clone(target.latest_script_sha256) : null,
        };
      }
    } else {
      effect = await registerResearchScrapeTarget(ctx, adapterPayload, record, sourceId, targetKey);
    }
  } catch (error) {
    const message = error.message;
    const failed = { ok: false, phase: isTest ? 'registry_lookup' : 'register', target_key: targetKey, error: message };
    if (isTest) {
      failed.test = persistScrapeTestPreflightFailure(record, 'test_failed', 'registry_lookup_failed', message, elapsed(), ctx.now());
    } else {
      putString(record, 'status', 'adapter_failed');
      putString(record, 'scrape_status', 'registration_failed');
      putString(record, 'last_error', message);
    }
    return failed;
  }
  effect = isObject(effect) ? clone(effect) : effect;

  const hasScript = get(effect, 'script_registered') === true;
  if (commandType === 'outbound.research_source.generate_adapter' || !hasScript) {
    if (!hasScript) {
      try {
        const taskEffect = await queueResearchScraperGeneration(ctx, command, adapterPayload, sourceId, targetKey);
        const runnable = ['pending', 'leased', 'running', 'review_rework'].includes(taskEffect.task_status);
        if (isObject(effect)) effect.generation_task = taskEffect;
        const status = runnable ? 'generation_queued' : 'generation_blocked';
        putString(record, 'status', status);
        putString(record, 'scrape_status', status);
        return effect;
      } catch (error) {
        const message = error.message;
        if (isObject(effect)) effect.generation_task = { ok: false, error: message };
        putString(record, 'status', 'adapter_failed');
        putString(record, 'scrape_status', 'generation_failed');
        putString(record, 'last_error', message);
        return effect;
      }
    }
    putString(record, 'status', 'adapter_ready');
    putString(record, 'scrape_status', hasScript ? 'registered' : 'script_required');
    return effect;
  }

  // RUST-EIGENHEIT (:3290-3308): unreachable — `!has_script` already returned
  // above; kept for line-by-line parity.
  if (!hasScript) {
    const message = 'scrape target has no registered script yet';
    const testEffect = persistScrapeTestPreflightFailure(record, 'test_blocked', 'script_required', message, elapsed(), ctx.now());
    if (isObject(effect)) {
      effect.test_skipped = true;
      effect.test_skip_reason = message;
      effect.test = testEffect;
    }
    return effect;
  }

  // The source test is the operator's explicit request to check the source;
  // the probe grant comes from the command authorization inside the Core.
  const testStarted = ctx.now();
  let outcome;
  try {
    const timeoutMs = scrapeOperationTimeoutMs(testInput.operation_timeout_ms);
    outcome = await executeScrape(ctx, {
      target_key: targetKey,
      trigger_kind: 'manual',
      // Leave time to serialize browser evidence before the runner stops its tree.
      timeout_seconds: Math.ceil(timeoutMs / 1000) + 30,
      input: testInput,
      command_id: command.id,
      // ref: store_outbound_commands.rs:4123 — the source test carries the operator's probe grant;
      // authorization is the command right checked by the Core before this handler (§16.20).
      probe_account: true,
    });
    if (!isObject(outcome)) fail('unavailable', 'scrape execution returned no outcome');
  } catch (error) {
    const message = error.message;
    const latencyMs = Math.max(0, ctx.now() - testStarted);
    putString(record, 'status', 'test_failed');
    putString(record, 'scrape_status', 'test_error');
    putString(record, 'last_error', message);
    const testEffect = {
      ok: false,
      test_ok: false,
      status: 'execution_error',
      records_found: 0,
      latency_ms: latencyMs,
      error: message,
      evidence: { valid: false, reason: 'scrape execution failed before a durable run outcome was returned' },
      tested_at_ms: ctx.now(),
    };
    persistScrapeTestObservation(record, testEffect, testEffect.evidence);
    if (isObject(effect)) effect.test = testEffect;
    return effect;
  }

  const fieldKeys = has(record, 'field_keys') ? record.field_keys : get(adapterPayload, 'field_keys');
  const expectedFields = (Array.isArray(fieldKeys) ? fieldKeys : [])
    .filter((f) => typeof f === 'string').map((f) => f.trim()).filter((f) => f !== '');
  const extracted = Array.isArray(outcome.fields_extracted) ? outcome.fields_extracted.map(String) : [];
  const extractedLower = new Set(extracted.map(asciiLower));
  const missingFields = expectedFields.filter((f) => !extractedLower.has(asciiLower(f)));
  // At least one declared field with a real value proves the data access; one
  // company rarely carries every declared field. Missing fields stay listed.
  const hasExpectedFields = extracted.length > 0 && (expectedFields.length === 0 || missingFields.length < expectedFields.length);
  const testEffect = clone(outcome);
  testEffect.expected_fields = expectedFields;
  testEffect.missing_fields = missingFields;
  const evidence = await scrapeTestEvidence(ctx, testEffect);
  const evidenceValid = evidence.valid === true;
  const recordsFound = asI64(outcome.records_found) ?? 0;
  const queryCompletedEmpty = outcome.status === 'completed_empty' && recordsFound === 0
    && outcome.query_completion != null && evidenceValid;
  const testOk = scrapeTestPassed(outcome.status, recordsFound, hasExpectedFields, evidenceValid);
  testEffect.ok = testOk || queryCompletedEmpty;
  testEffect.test_ok = testOk;
  testEffect.query_completed = queryCompletedEmpty;
  testEffect.evidence = clone(evidence);
  testEffect.tested_at_ms = ctx.now();
  putString(record, 'last_run_id', typeof outcome.run_id === 'string' ? outcome.run_id : '');

  if (queryCompletedEmpty) {
    putString(record, 'status', 'test_completed_empty');
    putString(record, 'scrape_status', 'completed_empty');
    putString(record, 'last_error', '');
  } else if (testOk) {
    putString(record, 'status', 'test_ok');
    putString(record, 'scrape_status', 'test_executed');
    putString(record, 'last_error', '');
    putI64(record, 'last_success_at_ms', ctx.now());
  } else {
    const [status, scrapeStatus] = failedTestStatus(outcome, evidenceValid);
    const prefix = typeof outcome.error === 'string' ? outcome.error : `scrape test validation failed: status=${outcome.status}`;
    const latency = asU64(outcome.latency_ms) ?? 0;
    const diagnosis = `${prefix}; records_found=${recordsFound}; expected_fields=${rustDebugStrings(expectedFields)}; extracted_fields=${rustDebugStrings(extracted)}; missing_fields=${rustDebugStrings(missingFields)}; latency_ms=${latency}`;
    putString(record, 'status', status);
    putString(record, 'scrape_status', scrapeStatus);
    putString(record, 'last_error', diagnosis);
    if (status === 'test_auth_required') putString(record, 'auth_status', 'auth_required');
    testEffect.error = diagnosis;
  }
  persistScrapeTestObservation(record, testEffect, evidence);
  if (isObject(effect)) effect.test = testEffect;
  return effect;
}

// ---------------------------------------------------------------- credential status

/** ref: :3873 outbound_apply_research_adapter_credential_status */
export async function applyResearchAdapterCredentialStatus(ctx, adapterPayload, record, nextStatus) {
  const own = get(record, 'requires_credential');
  const requiresCredential = typeof own === 'boolean' ? own
    : (typeof get(adapterPayload, 'requires_credential') === 'boolean' ? adapterPayload.requires_credential : false);
  const { scope } = await ctx.cap.secret.scope();
  if (!requiresCredential) {
    putString(record, 'auth_status', 'not_required');
    return { required: false, scope, secret_value_in_payload: false };
  }
  const secretName = firstString([str(record, ['credential_secret_name']), str(adapterPayload, ['credential_secret_name'])]);
  if (secretName == null) {
    putString(record, 'auth_status', 'credential_name_missing');
    return { required: true, scope, name: '', exists: false, secret_value_in_payload: false };
  }
  let exists = false;
  try {
    exists = (await ctx.cap.secret.exists({ scope, name: secretName }))?.exists === true;
  } catch {
    exists = false; // Rust `.unwrap_or(false)`
  }
  const status = exists ? 'credential_available' : nextStatus === 'auth_requested' ? 'auth_requested' : 'credential_missing';
  putString(record, 'auth_status', status);
  return { required: true, scope, name: secretName, exists, status, secret_value_in_payload: false };
}

// ---------------------------------------------------------------- writeback

/** ref: :3037 outbound_apply_research_adapter_writeback */
export async function applyResearchAdapterWriteback(ctx, command, adapterId, record, now) {
  if (!has(command.payload, 'writeback')) return;
  const writeback = command.payload.writeback;
  const collection = str(writeback, ['collection']);
  if (collection == null) fail('invalid', 'adapter writeback collection is required');
  if (!isSafeRxdbCollectionName(collection)) fail('invalid', 'invalid adapter writeback collection');
  const cc = clientContext(command);
  // ABWEICHUNG: without client_context the v4 app_id is the source module.
  const sourceModule = firstString([str(cc, ['source_module']), str(cc, ['module_id']), str(cc, ['app_id']),
    typeof command.app_id === 'string' && command.app_id.trim() !== '' ? command.app_id.trim() : null]);
  if (sourceModule == null) fail('invalid', 'adapter writeback requires a source module');
  const modulePrefix = runtimeAppStarterCollectionName(sourceModule).replace(/(_records)+$/, '');
  if (!(modulePrefix !== '' && collection.startsWith(`${modulePrefix}_`))) {
    fail('invalid', 'adapter writeback collection must be scoped to the source module');
  }
  const recordId = str(writeback, ['record_id']) ?? adapterId;
  if (recordId !== adapterId) fail('invalid', 'adapter writeback record_id must match adapter_id');
  const projection = clone(record);
  putString(projection, 'id', recordId);
  putString(projection, 'last_command_id', typeof command.id === 'string' ? command.id : '');
  putI64(projection, 'updated_at_ms', now);
  await ctx.put(collection, projection);
}

// ---------------------------------------------------------------- handler

/** ref: :2776 outbound_handle_research_source_adapter */
export async function handleResearchSourceAdapter(command, ctx, nextStatus) {
  const payload = command.payload;
  const now = ctx.now();
  const adapterPayload = has(payload, 'adapter') ? payload.adapter : payload;
  const recordIdField = typeof command.record_id === 'string' ? command.record_id.trim() : null;
  const adapterId = firstString([str(payload, ['adapter_id']), str(adapterPayload, ['id']), recordIdField]);
  if (adapterId == null) fail('invalid', 'adapter_id is required');
  const sourceId = firstString([str(adapterPayload, ['source_id']), str(adapterPayload, ['id']), str(payload, ['source_id'])]);
  if (sourceId == null) fail('invalid', 'source_id is required');
  const url = firstString([str(adapterPayload, ['url']), str(payload, ['url'])]) ?? `https://${sourceId}/`;
  const cc = clientContext(command);

  const loaded = await ctx.get(ADAPTERS, adapterId);
  const record = loaded ?? objectPayload(adapterPayload);
  putString(record, 'id', adapterId);
  putString(record, 'campaign_id', firstString([
    str(adapterPayload, ['campaign_id']), str(payload, ['campaign_id']), str(cc, ['campaign_id']),
  ]) ?? '');
  putString(record, 'source_id', sourceId);
  putString(record, 'label', firstString([str(adapterPayload, ['label']), str(payload, ['label']), sourceId]) ?? sourceId);
  putString(record, 'url', url);
  putString(record, 'adapter_kind', firstString([str(adapterPayload, ['adapter_kind']), str(payload, ['adapter_kind'])]) ?? 'custom_url');
  putString(record, 'target_key', firstString([str(adapterPayload, ['target_key']), str(payload, ['target_key'])]) ?? '');
  putString(record, 'status', nextStatus);
  if (isObject(record)) {
    record.enabled = typeof get(adapterPayload, 'enabled') === 'boolean' ? adapterPayload.enabled : true;
    record.countries = has(adapterPayload, 'countries') ? clone(adapterPayload.countries) : [];
    record.field_keys = has(adapterPayload, 'field_keys') ? clone(adapterPayload.field_keys) : [];
    record.requires_credential = typeof get(adapterPayload, 'requires_credential') === 'boolean' ? adapterPayload.requires_credential : false;
  }
  mergeFields(record, adapterPayload, [
    'tier', 'credential_secret_name', 'auth_mode', 'auth_status', 'scrape_status', 'last_run_id', 'last_success_at_ms', 'last_error',
  ]);
  // Adapter schemas require a complete state tuple even when the command only
  // changes authentication.
  putDefaultString(record, 'scrape_status', 'target_available');
  if (nextStatus === 'adapter_requested') putString(record, 'scrape_status', 'registration_requested');
  else if (nextStatus === 'test_requested') putString(record, 'scrape_status', 'test_requested');
  else if (nextStatus === 'auth_requested') putString(record, 'auth_status', 'auth_requested');
  putString(record, 'last_error', '');
  putDefaultObject(record, 'payload');
  payloadInsert(record, 'last_command_id', typeof command.id === 'string' ? command.id : '');
  payloadInsert(record, 'last_command_type', command.type);
  if (has(payload, 'scrape_contract')) payloadInsert(record, 'scrape_contract', payload.scrape_contract);
  if (has(payload, 'target_manifest')) payloadInsert(record, 'target_manifest', payload.target_manifest);
  payloadInsert(record, 'secret_value_in_payload', false);
  const scrapeEffect = await applyResearchAdapterScrapeEffect(ctx, command, adapterPayload, adapterId, sourceId, record);
  if (scrapeEffect != null) payloadInsert(record, 'scrape_registry_effect', scrapeEffect);
  const credentialEffect = await applyResearchAdapterCredentialStatus(ctx, adapterPayload, record, nextStatus);
  if (str(record, ['status']) === 'test_auth_required') putString(record, 'auth_status', 'auth_required');
  payloadInsert(record, 'credential_ref', credentialEffect);
  putDefaultI64(record, 'created_at_ms', now);
  putI64(record, 'updated_at_ms', now);
  // Rust: upsert_business_record + upsert_rxdb_collection_record → one v4 write.
  await ctx.put(ADAPTERS, record);

  let authAssist = null;
  if (nextStatus === 'auth_requested') {
    const secretName = str(record, ['credential_secret_name']) ?? '';
    // CAP-3b #844 caps/browser.rs AuthAssistRequest: credential_ref nur als Verweis in den
    // Instanz-Schlüsselbund (^ctox-secret://credentials/), nie ein App-Scope und nie ein Wert.
    const credentialRef = secretName !== '' ? `ctox-secret://credentials/${secretName}` : null;
    // RUST-EIGENHEIT: filter on trim, but the untrimmed id is used.
    const requestingTaskId = typeof command.id === 'string' && command.id.trim() !== '' ? command.id : adapterId;
    const snapshot = get(payload, 'record_snapshot');
    const preferred = has(snapshot, 'auth_assist') ? snapshot.auth_assist : get(payload, 'auth_assist');
    // CAP-3b #844 caps/browser.rs AuthAssistRequest (deny_unknown_fields): nur Vertragsfelder. Der
    // Sitzungseigentuemer ist der Core-gebundene Akteur aus ctx; owner_user_id/deterministic_for_task/
    // accept_as_trusted_local entfallen. source_module muss der aufrufenden App entsprechen (Rust nahm
    // command.module, das die UI als 'outbound' sendet; v4 hat app_id).
    const effect = await ctx.cap.browser.auth_assist.request({
      source_id: sourceId,
      target_url_override: url,
      credential_ref: credentialRef,
      login_hint: null,
      preferred_auth_assist: preferred === undefined ? null : clone(preferred),
      requesting_task_id: requestingTaskId,
      source_module: command.app_id,
      command_path: command.type,
    });
    putString(record, 'auth_status', 'browser_session_requested');
    payloadInsert(record, 'auth_assist', effect);
    putI64(record, 'updated_at_ms', now);
    await ctx.put(ADAPTERS, record);
    authAssist = effect === undefined ? null : clone(effect);
  }
  await applyResearchAdapterWriteback(ctx, command, adapterId, record, now);
  return {
    ok: true,
    collection: ADAPTERS,
    adapter: clone(record),
    adapter_id: adapterId,
    command_effect: nextStatus,
    auth_assist: authAssist,
    secret_value_in_payload: false,
  };
}

// ---------------------------------------------------------------- generation callback (Strang SKL-C)

/**
 * outbound.research_source.generation.done {task_id, status, result?, error?} — Rückruf der Adapter-Erzeugung aus
 * queueResearchScraperGeneration (vorher ohne Rückweg, Stand nur über die Scrape-Registry, F §1 #26). Nur Adapter im Stand
 * `generation_queued` derselben Quelle werden fortgeschrieben (Doppelzustellung = keine Wirkung): abgeschlossen mit
 * registriertem Skript → adapter_ready/registered, sonst adapter_failed/generation_failed mit Grund.
 */
export async function handleGenerationDone(command, ctx) {
  const taskId = str(command.payload, ['task_id']);
  if (taskId == null) fail('invalid', `${GENERATION_DONE} is missing task_id`);
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  if (task == null) fail('not_found', `agent task ${taskId} does not exist`);
  const sourceId = str(task.payload, ['adapter_source_id']);
  const targetKey = str(task.payload, ['target_key']);
  if (sourceId == null || targetKey == null) fail('invalid', `agent task ${taskId} carries no adapter generation binding`);
  const rows = (await ctx.read(ADAPTERS, { selector: { source_id: sourceId } })).filter((r) => get(r, 'status') === 'generation_queued');
  if (rows.length === 0) return { ok: true, task_id: taskId, source_id: sourceId, updated: 0 };
  const status = str(command.payload, ['status']) ?? 'failed';
  const target = await ctx.get(TARGETS, targetKey);
  const registered = status === 'completed' && get(target, 'script_registered') === true;
  let reason = '';
  if (!registered) {
    if (status === 'completed') reason = 'Adaptererzeugung beendet ohne registriertes Skript';
    else if (status === 'cancelled') reason = 'Adaptererzeugung abgebrochen';
    else reason = firstString([str(command.payload, ['error']), str(command.payload, ['status_note']), str(command.payload?.result, ['error'])]) ?? `agent task ${status}`;
  }
  const now = ctx.now();
  for (const row of rows) {
    const next = clone(row);
    putString(next, 'status', registered ? 'adapter_ready' : 'adapter_failed');
    putString(next, 'scrape_status', registered ? 'registered' : 'generation_failed');
    putString(next, 'last_error', reason);
    putI64(next, 'updated_at_ms', now);
    await ctx.put(ADAPTERS, next);
  }
  return { ok: true, task_id: taskId, source_id: sourceId, updated: rows.length, status: registered ? 'adapter_ready' : 'adapter_failed' };
}
