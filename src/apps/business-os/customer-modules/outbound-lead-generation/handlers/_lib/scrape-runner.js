// Scrape-Skriptlauf über das generische OS-Primitiv `process.run` (Strang OLGSCR, umgestellt in Strang OLG725 nach §16.48).
//
// Vorher: Core-Fähigkeiten Testlauf / Konto-Wiederprüfung / Laufdatei (Rust
// src/core/capabilities/scrape/execute.rs, classify.rs, account_state.rs). Jetzt:
//   - Skript + Eingabe aus der App-Registry (scrape-registry.js), Text in `files`;
//   - Ausführung (§16.48 Nr. 3): erzeugter Code läuft nur über die Allowlist-Einträge mit untrusted_code:true, also
//     nur in der Sandbox E20: `olg-scrape-js` (node + runtime/host.mjs) für JavaScript, `olg-scrape-ts`
//     (tsx + runtime/host.ts) für TypeScript. `olg-scrape-sh` ist abgelehnt; bash/sh-Skripte laufen nicht mehr.
//     Der Allowlist-Eintrag nennt das Host-Skript im App-Paket; stdin trägt EIN JSON-Dokument
//     {schema:'ctox.olg.scrape_host.v1', language, script_sha256, script, env:{CTOX_SCRAPE_*…}} (≤ 1 MiB, darüber
//     als files-Datensatz per stdin_ref). Kein args, kein env an process.run: die Grundumgebung PATH/CTOX_BIN/LANG setzt
//     die Plattform; die CTOX_SCRAPE_*-Werte (execute.rs:1240-1296) setzt der Host im Kind. Damit entfällt die frühere
//     4096-Byte-Grenze je Umgebungswert.
//   - Keine Sandbox → process.run liefert `unavailable`: Laufdokument mit Status `awaiting_approval` („wartet auf
//     Freigabe“), kein Konto-Zustand gebucht, Befehl scheitert typisiert mit `unavailable` (Zone-Meldung über den
//     Befehlsstatus). Es gibt KEINEN Rückfall auf einen direkten Interpreter-Aufruf.
//   - Auswertung (Payload, Records, Felder, Klassifikation, Konto-Zustand) hier;
//   - Laufbeleg: Run-Manifest und Artefakte als `files` (inhaltsadressiert), Laufdokument
//     in outbound_lead_generation_scrape_runs.
// ABWEICHUNGEN (V4-HANDLERS.md „Scrape-Läufer“): kein HTTP-Vorabtest des Portals (probe gilt als
// erreichbar; der Status kommt aus failure_mode/Exit/Zeitüberschreitung/Fehlertext); die
// query_completion-Quittung wird ohne Dateihash-Prüfung des Laufverzeichnisses übernommen;
// CTOX_SCRAPE_MANIFEST_PATH/SOURCES_MANIFEST_PATH/SOURCES_DIR (Dateien im Rust-Workspace) entfallen:
// statt der Manifestdatei steht das Manifest als CTOX_SCRAPE_MANIFEST_JSON in der Host-Konfiguration.
import { asI64, fail, has, isObject, takeChars } from './olg.js';
import { ACCOUNT_STATES, RUNS, loadRegisteredTarget, putText, readFileText, sha256Hex } from './scrape-registry.js';

const get = (value, key) => (has(value, key) ? value[key] : undefined);

// ref: account_state.rs:18-23
export const FIRST_BACKOFF_MS = 6 * 60 * 60 * 1000;
export const LATER_BACKOFF_MS = 24 * 60 * 60 * 1000;
export const PROBE_LEASE_MS = 30 * 60 * 1000;
export const RETRY_AFTER_OTHER_FAILURE_MS = 60 * 60 * 1000;

/** ref: account_state.rs:27 is_provider_account_inactive */
export function isProviderAccountInactive(detail) {
  const text = String(detail ?? '').replace(/[A-Z]/g, (c) => c.toLowerCase());
  return ['customer is not active', 'account is not active', 'account is inactive', 'account inactive',
    'account is suspended', 'account suspended', 'account has been suspended', 'account is disabled',
    'subscription expired', 'subscription is expired', 'subscription inactive', 'subscription is inactive',
  ].some((marker) => text.includes(marker));
}

/** ref: classify.rs contains_transient_hint */
export function containsTransientHint(text) {
  return ['timeout', 'timed out', 'temporary', 'temporarily', 'connection refused', 'connection reset',
    'network is unreachable', 'name or service not known', '429', '502', '503', '504', 'ssl', 'proxyerror', 'net::err_']
    .some((hint) => text.includes(hint));
}

/** ref: mod.rs:1033 parse_execution_payload — empty → {}, array → {records}, invalid → error. */
export function parseExecutionPayload(stdoutText) {
  const trimmed = String(stdoutText ?? '').trim();
  if (trimmed === '') return {};
  let value;
  try {
    value = JSON.parse(trimmed);
  } catch {
    fail('invalid', 'scrape script stdout must be valid json');
  }
  return Array.isArray(value) ? { records: value } : value;
}

/** ref: mod.rs:1047 normalize_records */
export function normalizeRecords(payload) {
  if (Array.isArray(payload)) return payload.slice();
  if (!isObject(payload)) return null;
  for (const key of ['records', 'jobs', 'items']) if (Array.isArray(payload[key])) return payload[key].slice();
  if (has(payload, 'result')) return normalizeRecords(payload.result);
  return null;
}

/** ref: mod.rs:1131 has_extracted_value */
function hasExtractedValue(value) {
  if (value === null || value === undefined) return false;
  if (typeof value === 'string') return value.trim() !== '';
  if (Array.isArray(value)) return value.length > 0;
  if (isObject(value)) return Object.keys(value).length > 0;
  return true;
}

/** ref: mod.rs:1115 extracted_record_fields — sorted, deduplicated. */
export function extractedRecordFields(records) {
  const fields = (records ?? []).flatMap((record) => {
    if (!isObject(record) || typeof record.field !== 'string' || !has(record, 'value')) return [];
    const field = record.field.trim();
    return field !== '' && hasExtractedValue(record.value) ? [field] : [];
  });
  return [...new Set(fields)].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

const result = (status, shouldQueueRepair, reason) => ({ status, should_queue_repair: shouldQueueRepair, reason });

/**
 * ref: classify.rs classify_outcome. Without the native HTTP pre-probe the probe branches
 * (human verification, 401/403/404/429/5xx, unreachable) are not taken; the order of the
 * remaining checks is unchanged.
 */
export function classifyOutcome(payload, execution, recordsFound, expectedMinRecords) {
  const explicit = typeof get(payload, 'failure_mode') === 'string' ? payload.failure_mode.trim() : '';
  const detail = typeof get(payload, 'detail') === 'string' ? payload.detail : '';
  if (explicit === 'invalid_input') return result('invalid_input', false, 'explicit_failure_mode_invalid_input');
  if (explicit === 'provider_account_inactive'
    || (['temporary_unreachable', 'blocked', 'auth_required', 'authorization_required'].includes(explicit) && isProviderAccountInactive(detail))) {
    return result('provider_account_inactive', false, 'provider_account_inactive');
  }
  if (explicit === 'temporary_unreachable') return result('temporary_unreachable', false, 'explicit_failure_mode_temporary_unreachable');
  if (explicit === 'portal_drift') return result('portal_drift', true, 'explicit_failure_mode_portal_drift');
  if (explicit === 'blocked') return result('blocked', true, 'explicit_failure_mode_blocked');
  if (explicit === 'auth_required') return result('blocked', true, 'explicit_failure_mode_auth_required');
  if (explicit === 'authorization_required') return result('authorization_required', true, 'explicit_failure_mode_authorization_required');
  if (explicit === 'partial_output' || get(payload, 'partial_output') === true) return result('partial_output', true, 'payload_marked_partial_output');
  const lower = `${execution.stderr_text}\n`.toLowerCase();
  const exit = execution.exit_code ?? 0;
  const fullSuccess = exit === 0 && recordsFound > 0 && recordsFound >= expectedMinRecords;
  if (execution.timed_out || (!fullSuccess && containsTransientHint(lower))) {
    return result('temporary_unreachable', false, execution.timed_out ? 'command_timed_out' : 'transient_error_hint');
  }
  if (expectedMinRecords > 0 && recordsFound > 0 && recordsFound < expectedMinRecords) {
    return result('partial_output', true, `records_found_below_expected_min:${recordsFound}<${expectedMinRecords}`);
  }
  // Rust formats Option<i32> with {:?} → "Some(2)" / "None".
  if (exit !== 0) return result('portal_drift', true, `command_failed_exit_${execution.exit_code == null ? 'None' : `Some(${execution.exit_code})`}`);
  if (recordsFound === 0) return result('portal_drift', true, 'empty_record_set_on_reachable_portal');
  return result('succeeded', false, 'ok');
}

/** ref: query_completion.rs:38-80 (Vorbedingungen) + execute.rs:656-695 (Umklassifizierung). */
export function applyQueryCompletion(classification, payload, execution) {
  if (!has(payload, 'query_completion')) return { classification, receipt: null };
  const blank = (v) => v === undefined || v === null || v === '';
  const valid = isObject(payload.query_completion)
    && Array.isArray(get(payload, 'records')) && payload.records.length === 0
    && blank(get(payload, 'failure_mode')) && blank(get(payload, 'error')) && get(payload, 'partial_output') !== true
    && !execution.timed_out && execution.exit_code === 0;
  if (valid && classification.reason === 'empty_record_set_on_reachable_portal') {
    return { classification: result('completed_empty', false, 'current_query_completed_without_matches'), receipt: payload.query_completion };
  }
  if (!valid && (classification.reason === 'empty_record_set_on_reachable_portal' || classification.status === 'succeeded')) {
    return { classification: result('portal_drift', true, 'invalid_query_completion:completed-empty query receipt rejected'), receipt: null };
  }
  return { classification, receipt: null };
}

// ---------------------------------------------------------------- account state (ref: account_state.rs)

/** ref: account_state.rs:127-213 admit — without credential versions (see V4-HANDLERS.md). */
export async function admit(ctx, targetKey, authorizedProbe, runId) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const state = await tx.get(ACCOUNT_STATES, targetKey);
    if (state == null) return { run: true, probe_generation: null };
    if (!(authorizedProbe || now >= state.next_probe_at_ms)) return { run: false, state };
    if (typeof state.probe_lease_owner === 'string' && state.probe_lease_until_ms >= now) return { run: false, state };
    await tx.put(ACCOUNT_STATES, { ...state, probe_lease_owner: runId, probe_lease_until_ms: now + PROBE_LEASE_MS, updated_at_ms: now });
    return { run: true, probe_generation: state.generation };
  });
}

const backoffAfter = (failedProbes) => (failedProbes <= 1 ? FIRST_BACKOFF_MS : LATER_BACKOFF_MS);

/** ref: account_state.rs:256-340 record_in_transaction */
export async function recordAccountState(ctx, target, runId, probeGeneration, runResult, reason) {
  const now = ctx.now();
  await ctx.transaction(async (tx) => {
    const state = await tx.get(ACCOUNT_STATES, target.target_key);
    const owns = state != null && state.generation === probeGeneration && state.probe_lease_owner === runId;
    if (runResult === 'account_inactive' && probeGeneration == null) {
      if (state != null) return; // INSERT OR IGNORE: a concurrent state stands.
      await tx.put(ACCOUNT_STATES, {
        id: target.target_key, target_key: target.target_key, target_id: target.target_id, generation: 1,
        reason, causal_run_id: runId, last_probe_run_id: runId, failed_probes: 1,
        next_probe_at_ms: now + FIRST_BACKOFF_MS, probe_lease_owner: null, probe_lease_until_ms: 0, updated_at_ms: now,
      });
    } else if (runResult === 'account_inactive' && owns) {
      const failed = (state.failed_probes ?? 0) + 1;
      await tx.put(ACCOUNT_STATES, {
        ...state, generation: state.generation + 1, reason, last_probe_run_id: runId, failed_probes: failed,
        next_probe_at_ms: now + backoffAfter(failed), probe_lease_owner: null, probe_lease_until_ms: 0, updated_at_ms: now,
      });
    } else if (runResult === 'succeeded' && owns) {
      await tx.remove(ACCOUNT_STATES, target.target_key);
    } else if (runResult === 'other_failure' && owns) {
      await tx.put(ACCOUNT_STATES, {
        ...state, probe_lease_owner: null, probe_lease_until_ms: 0, next_probe_at_ms: now + RETRY_AFTER_OTHER_FAILURE_MS, updated_at_ms: now,
      });
    }
  });
}

// ---------------------------------------------------------------- run

async function putJson(ctx, value, name) {
  return putText(ctx, JSON.stringify(value), { name, content_type: 'application/json', collection: RUNS });
}

// §16.48 Nr. 3: Sprache → Allowlist-Eintrag (untrusted_code:true, nur Sandbox E20). Sonst nichts.
export const HOST_PROGRAMS = Object.freeze({ javascript: 'olg-scrape-js', js: 'olg-scrape-js', typescript: 'olg-scrape-ts', ts: 'olg-scrape-ts' });
export const HOST_SCHEMA = 'ctox.olg.scrape_host.v1';
export const STDIN_MAX_BYTES = 1024 * 1024;
export const AWAITING_APPROVAL = 'awaiting_approval';

/** Allowlist-Eintrag für die Skriptsprache; bash/sh/unbekannt → invalid (olg-scrape-sh abgelehnt, §16.48). */
export function hostProgram(language) {
  const key = String(language ?? 'javascript').trim().toLowerCase();
  const program = HOST_PROGRAMS[key];
  if (!program) {
    fail('invalid', `Skriptsprache ${JSON.stringify(language)} wird nicht ausgeführt (zugelassen: javascript, typescript; olg-scrape-sh abgelehnt, §16.48) – Adapter als JavaScript neu erzeugen`);
  }
  return { program, language: program === 'olg-scrape-ts' ? 'typescript' : 'javascript' };
}

/** stdin-Dokument für den Host (runtime/host.mjs parseHostInput). */
export function hostInput(language, scriptSha256, body, env) {
  return JSON.stringify({ schema: HOST_SCHEMA, language, script_sha256: scriptSha256, script: body, env });
}

/**
 * CTOX_SCRAPE_*-Werte für das Kind (setzt runtime/host.mjs; §16.48: nicht mehr process.run env). Layout wie execute.rs:580:
 * cwd = <workspace>/runs/<run_id>, Ausgaben unter outputs/. Pfade relativ zum cwd.
 * ref: execute.rs:1240-1296 (CTOX_SCRAPE_*, CTOX_OWNER_USER_ID).
 */
export function scrapeEnv(target, runId, triggerKind, commandId, input) {
  const sources = get(target.config, 'sources') ?? [];
  const env = {
    CTOX_SCRAPE_TARGET_KEY: target.target_key,
    CTOX_SCRAPE_TARGET_DIR: '../..',
    CTOX_SCRAPE_RUN_DIR: '.',
    CTOX_SCRAPE_OUTPUT_DIR: 'outputs',
    CTOX_SCRAPE_RUN_ID: runId,
    CTOX_SCRAPE_TRIGGER_KIND: String(triggerKind ?? ''),
    CTOX_SCRAPE_START_URL: String(target.start_url ?? ''),
    CTOX_SCRAPE_SOURCES_JSON: JSON.stringify(sources),
    CTOX_SCRAPE_MANIFEST_JSON: JSON.stringify({ target_key: target.target_key, start_url: target.start_url ?? null, config: target.config ?? {} }),
  };
  if (typeof commandId === 'string' && commandId !== '') env.CTOX_SCRAPE_COMMAND_ID = commandId;
  if (isObject(input)) {
    env.CTOX_SCRAPE_INPUT_JSON = JSON.stringify(input);
    if (typeof input.owner_user_id === 'string' && input.owner_user_id.trim() !== '') env.CTOX_OWNER_USER_ID = input.owner_user_id.trim();
  }
  return env;
}

/**
 * One scrape run of the active registered script. Returns the ScrapeExecutionOutcome shape the
 * research_source handlers already evaluate (run_manifest_path = files ref of the run manifest).
 */
export async function executeScrape(ctx, { target_key: targetKey, trigger_kind: triggerKind, timeout_seconds: timeoutSeconds, input, command_id: commandId, probe_account: probeAccount = false }) {
  const registered = await loadRegisteredTarget(ctx, targetKey);
  if (registered == null) fail('not_found', `scrape target ${targetKey} has no registered script`);
  const { target, script } = registered;
  const started = ctx.now();
  const runId = `run-${ctx.uuid()}`;

  const admission = await admit(ctx, target.target_key, probeAccount === true, runId);
  if (!admission.run) {
    // ref: execute.rs:1147 suppressed_account_outcome — no provider call.
    const s = admission.state;
    return {
      ok: false, target_key: target.target_key, run_id: s.last_probe_run_id, status: 'provider_account_inactive',
      records_found: 0, fields_extracted: [], latency_ms: Math.max(0, ctx.now() - started),
      reason: 'provider_account_inactive_suppressed',
      error: `Konto beim Anbieter inaktiv: ${s.reason} (erfasst in ${s.causal_run_id}, zuletzt geprüft in ${s.last_probe_run_id}); kein Anbieteraufruf. Automatische Wiederprüfung beim nächsten Abruf ab ${new Date(s.next_probe_at_ms).toISOString()}, früher nach geänderten Zugangsdaten oder autorisierter Prüfung.`,
      query_completion: null, should_queue_repair: false, run_manifest_path: null,
    };
  }

  const host = hostProgram(script.language);
  const env = scrapeEnv(target, runId, triggerKind, commandId, input);
  const stdinText = hostInput(host.language, script.script_sha256, registered.body, env);
  const cwdRef = `${target.workspace_dir}/runs/${runId}`;
  const request = { program: host.program, cwd_ref: cwdRef, timeout_ms: Math.max(1, timeoutSeconds) * 1000 };
  if (new TextEncoder().encode(stdinText).length <= STDIN_MAX_BYTES) request.stdin = stdinText;
  else request.stdin_ref = (await putText(ctx, stdinText, { name: `${runId}-host-input.json`, content_type: 'application/json' })).ref;
  let proc;
  try {
    proc = await ctx.cap.process.run(request);
  } catch (error) {
    if (error?.code !== 'unavailable') throw error;
    // Keine Sandbox (E20): Lauf wartet auf Freigabe. Kein direkter Aufruf, kein Konto-Zustand, kein Laufergebnis.
    const reason = String(error?.message || 'sandbox unavailable');
    await ctx.put(RUNS, {
      id: runId, run_id: runId, target_key: target.target_key, target_id: target.target_id, trigger_kind: triggerKind,
      command_id: commandId ?? null, started_at: new Date(started).toISOString(), finished_at: null,
      status: AWAITING_APPROVAL, script_revision_no: script.revision_no, script_sha256: script.script_sha256,
      result: { records_found: 0, reason: 'sandbox_unavailable', detail: takeChars(reason, 300), program: host.program },
      created_at_ms: started, updated_at_ms: ctx.now(),
    });
    fail('unavailable', `Scrape-Lauf ${runId} für ${target.target_key} wartet auf Freigabe: erzeugter Code läuft nur in der Sandbox (${host.program}), die hier nicht verfügbar ist (${takeChars(reason, 200)})`);
  }
  const execution = {
    exit_code: Number.isInteger(proc?.exit) ? proc.exit : null,
    timed_out: proc?.timed_out === true,
    stdout_text: typeof proc?.stdout === 'string' ? proc.stdout : '',
    stderr_text: typeof proc?.stderr === 'string' ? proc.stderr : '',
  };

  let payload;
  let parseError = null;
  try {
    payload = parseExecutionPayload(execution.stdout_text);
  } catch (error) {
    payload = {};
    parseError = error.message;
  }
  const records = normalizeRecords(payload);
  const recordsFound = records ? records.length : 0;
  const expectedMin = asI64(get(get(target, 'config'), 'expected_min_records')) ?? 0;
  let { classification, receipt } = applyQueryCompletion(classifyOutcome(payload, execution, recordsFound, expectedMin), payload, execution);
  if (parseError && classification.status === 'portal_drift' && classification.reason === 'empty_record_set_on_reachable_portal') {
    classification = result('portal_drift', true, 'invalid_script_stdout');
  }
  const finished = ctx.now();
  const fieldsExtracted = extractedRecordFields(records);
  const errorText = classification.status === 'succeeded' || classification.status === 'completed_empty' ? null
    : (typeof get(payload, 'error') === 'string' && payload.error.trim() !== '' ? payload.error
      : parseError ?? `scrape run ${classification.status}: ${classification.reason}${execution.stderr_text.trim() ? `; stderr: ${takeChars(execution.stderr_text.trim(), 300)}` : ''}`);

  // Durable evidence: artifacts + manifest in files (content-addressed), run document in RUNS.
  const resultArtifact = await putJson(ctx, payload, `${runId}-result.json`);
  const recordsArtifact = await putJson(ctx, records ?? [], `${runId}-records.json`);
  const artifacts = [
    { artifact_kind: 'result_json', path: resultArtifact.ref, content_sha256: resultArtifact.sha256 },
    { artifact_kind: 'records_json', path: recordsArtifact.ref, content_sha256: recordsArtifact.sha256, record_count: recordsFound },
  ];
  if (receipt) {
    const q = await putJson(ctx, receipt, `${runId}-query-completion.json`);
    artifacts.push({ artifact_kind: 'query_completion_evidence', path: q.ref, content_sha256: q.sha256, record_count: 0 });
  }
  const runResult = {
    records_found: recordsFound, query_completion: receipt, reason: classification.reason,
    failure_mode: typeof get(payload, 'failure_mode') === 'string' ? payload.failure_mode : null,
    detail: typeof get(payload, 'detail') === 'string' ? payload.detail : null,
    fields_extracted: fieldsExtracted, exit_code: execution.exit_code, timed_out: execution.timed_out,
  };
  const manifest = { run_id: runId, target_key: target.target_key, status: classification.status, result: runResult, artifacts };
  const manifestFile = await putJson(ctx, manifest, `${runId}-run.json`);
  await ctx.put(RUNS, {
    id: runId, run_id: runId, target_key: target.target_key, target_id: target.target_id, trigger_kind: triggerKind,
    command_id: commandId ?? null, started_at: new Date(started).toISOString(), finished_at: new Date(finished).toISOString(),
    status: classification.status, script_revision_no: script.revision_no, script_sha256: script.script_sha256,
    result: runResult, run_manifest_ref: manifestFile.ref, artifacts, created_at_ms: started, updated_at_ms: finished,
  });

  // ref: execute.rs:909-929 — account state booking after the run is recorded.
  const accountResult = classification.status === 'provider_account_inactive' ? 'account_inactive'
    : classification.status === 'succeeded' || (classification.status === 'completed_empty' && triggerKind !== 'account_reprobe') ? 'succeeded'
      : 'other_failure';
  await recordAccountState(ctx, target, runId, admission.probe_generation, accountResult,
    typeof get(payload, 'detail') === 'string' ? payload.detail : 'provider_account_inactive');

  return {
    ok: classification.status === 'succeeded' || classification.status === 'completed_empty',
    target_key: target.target_key, run_id: runId, status: classification.status, records_found: recordsFound,
    fields_extracted: fieldsExtracted, latency_ms: Math.max(0, finished - started), reason: classification.reason,
    error: errorText, query_completion: receipt, should_queue_repair: classification.should_queue_repair,
    run_manifest_path: manifestFile.ref,
  };
}

/** File facts for the evidence check (vorher Core-Fähigkeit „Laufdatei“): {is_file, sha256, json}. */
export async function runFileFacts(ctx, ref) {
  const text = await readFileText(ctx, ref);
  if (text == null) return { is_file: false, sha256: '', json: null };
  let json = null;
  try {
    json = JSON.parse(text);
  } catch {
    json = null;
  }
  return { is_file: true, sha256: await sha256Hex(text), json };
}
