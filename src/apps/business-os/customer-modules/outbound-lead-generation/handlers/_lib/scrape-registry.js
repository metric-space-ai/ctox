// Scrape-Registry als OLG-Collections (Strang OLGSCR, Owner-Regel „keine App-Logik im Core“).
//
// Vorher: Rust-Core `src/core/capabilities/scrape/` (SQLite-Tabellen scrape_target,
// scrape_script_revision, scrape_run, scrape_account_state) hinter Core-Fähigkeiten.
// Jetzt: vier App-Collections; Skripttexte liegen inhaltsadressiert in `files`
// (ctx.cap.files.put/get), die Registry speichert nur die Referenz.
//
// Die Fachlogik (Slug, Ziel-ID, Quellen-Normalisierung, Upsert-Regeln, Revisions-
// Dedupe, Registrierungsprüfung, Lese-Sichten) ist hier portiert; `ref:` nennt die
// Rust-Stelle (Klon-Stand 4794af03d: src/core/capabilities/scrape/…, Business-OS-
// Leser unter ~/.local/state/shell-v4-src/4794af03d/src/core/business_os/…).
import { asI64, fail, has, isObject, takeChars, utf8Len } from './olg.js';

export const TARGETS = 'outbound_lead_generation_scrape_targets';
export const SCRIPTS = 'outbound_lead_generation_scrape_scripts';
export const RUNS = 'outbound_lead_generation_scrape_runs';
export const ACCOUNT_STATES = 'outbound_lead_generation_scrape_account_states';

const get = (value, key) => (has(value, key) ? value[key] : undefined);
const clone = (value) => (value === undefined ? undefined : JSON.parse(JSON.stringify(value)));

// ---------------------------------------------------------------- hashing / files

const HEX = (buffer) => Array.from(new Uint8Array(buffer), (b) => b.toString(16).padStart(2, '0')).join('');

/** sha256 hex over UTF-8 bytes (Handler-Global crypto.subtle, §16.21). ref: mod.rs:1890 compute_sha256 */
export async function sha256Hex(input) {
  const bytes = typeof input === 'string' ? new TextEncoder().encode(input) : input;
  return HEX(await crypto.subtle.digest('SHA-256', bytes));
}

/** ref: mod.rs:1900 stable_digest — first 16 hex chars. */
export async function stableDigest(input) {
  return (await sha256Hex(input)).slice(0, 16);
}

/**
 * Text eines files-Datensatzes der App (CAP-3b #844 caps/files.rs):
 *   ctx.cap.files.get({ref}) → {id, ref, name, mime, size, owner_app, created_at_ms, text?}
 * `text` steht nur bei UTF-8 ≤ 512 KiB im Ergebnis (nie Base64). `not_found` → null (Datei fehlt
 * oder Hash passt nicht mehr). `unavailable` (files bis zum Cutover nicht gebunden) wird NICHT zu
 * null umgedeutet: sonst hielte die Registrierungsprüfung ein vorhandenes Skript für fehlend und
 * würde eine Neugenerierung auslösen. Der Befehl scheitert sauber mit Code `unavailable`.
 */
export async function readFileText(ctx, ref) {
  if (typeof ref !== 'string' || ref === '') return null;
  let file;
  try {
    file = await ctx.cap.files.get({ ref });
  } catch (error) {
    if (error?.code === 'not_found' || error?.code === 'invalid') return null;
    fail(error?.code || 'unavailable', `Dateiablage nicht verfügbar (files.get): ${error?.message || error}`);
  }
  if (!isObject(file)) return null;
  return typeof file.text === 'string' ? file.text : null;
}

/**
 * Text als files-Datensatz ablegen (CAP-3b #844): ctx.cap.files.put({name, mime, text}) → {id, ref};
 * ref = sha256 des Inhalts, owner_app setzt der Kern aus ctx. Liefert {ref, id, sha256, bytes}.
 * Der Name darf kein '/' oder '\\' enthalten (Kern: invalid).
 */
export async function putText(ctx, text, meta) {
  const name = String(meta?.name ?? 'file').replace(/[\\/]/g, '_');
  let answer;
  try {
    answer = await ctx.cap.files.put({ name, mime: meta?.content_type ?? 'application/octet-stream', text });
  } catch (error) {
    fail(error?.code || 'unavailable', `Dateiablage nicht verfügbar (files.put): ${error?.message || error}`);
  }
  if (!isObject(answer) || typeof answer.ref !== 'string') fail('unavailable', 'files.put returned no ref');
  return { ref: answer.ref, id: typeof answer.id === 'string' ? answer.id : null, sha256: await sha256Hex(text), bytes: utf8Len(text) };
}

// ---------------------------------------------------------------- pure helpers

/** ref: mod.rs:1870 slugify */
export function slugify(input) {
  let out = '';
  let lastDash = false;
  for (const ch of String(input).trim().toLowerCase()) {
    if (/^[a-z0-9]$/.test(ch)) {
      out += ch;
      lastDash = false;
    } else if (!lastDash) {
      out += '-';
      lastDash = true;
    }
  }
  const trimmed = out.replace(/^-+|-+$/g, '');
  return trimmed === '' ? 'scrape-target' : trimmed;
}

const trimmedStr = (object, key) => {
  const value = get(object, key);
  if (typeof value !== 'string') return null;
  const t = value.trim();
  return t === '' ? null : t;
};

/** ref: mod.rs:567-683 normalize_sources_from_config */
export function normalizeSourcesFromConfig(rawSources, startUrl, targetKey) {
  const out = [];
  const seen = new Set();
  if (Array.isArray(rawSources)) {
    rawSources.forEach((item, index) => {
      if (!isObject(item)) return;
      // serde: get("start_url").or_else(get("url")) — the fallback key counts only when start_url is absent.
      const sourceStartUrl = (has(item, 'start_url') ? trimmedStr(item, 'start_url') : trimmedStr(item, 'url')) ?? startUrl;
      let sourceKey = slugify(typeof item.source_key === 'string' ? item.source_key
        : typeof item.display_name === 'string' ? item.display_name : sourceStartUrl);
      if (sourceKey === '') sourceKey = `${targetKey}-source-${index + 1}`;
      if (seen.has(sourceKey)) return;
      const tags = Array.isArray(item.tags)
        ? item.tags.filter((t) => typeof t === 'string').map((t) => t.trim()).filter((t) => t !== '')
        : [];
      out.push({
        source_key: sourceKey,
        display_name: trimmedStr(item, 'display_name') ?? sourceKey,
        start_url: sourceStartUrl,
        source_kind: (has(item, 'source_kind') ? trimmedStr(item, 'source_kind') : trimmedStr(item, 'kind')) ?? 'generic',
        enabled: typeof item.enabled === 'boolean' ? item.enabled : true,
        extraction_module: trimmedStr(item, 'extraction_module') ?? `sources/${sourceKey}/extractor.js`,
        merge_strategy: trimmedStr(item, 'merge_strategy') ?? 'upsert_by_record_key',
        tags,
        notes: trimmedStr(item, 'notes'),
        config: has(item, 'config') ? clone(item.config) : {},
      });
      seen.add(sourceKey);
    });
  }
  if (out.length === 0) {
    out.push({
      source_key: 'primary',
      display_name: 'Primary Source',
      start_url: startUrl,
      source_kind: 'generic',
      enabled: true,
      extraction_module: 'sources/primary/extractor.js',
      merge_strategy: 'upsert_by_record_key',
      tags: ['primary'],
      notes: 'Default synthesized source for single-entry scrape targets.',
      config: {},
    });
  }
  return out;
}

/** ref: mod.rs:557 normalize_target_config */
export function normalizeTargetConfig(startUrl, targetKey, raw) {
  const object = isObject(raw) ? clone(raw) : {};
  object.sources = normalizeSourcesFromConfig(get(object, 'sources'), startUrl, targetKey);
  return object;
}

/** ref: mod.rs:1849 default_entry_command */
// §16.48 Nr. 3 (Strang OLG725): nur JavaScript und TypeScript (olg-scrape-js/-ts). bash/sh (olg-scrape-sh) ist abgelehnt;
// entry_command bleibt als Beleg am Skript-Dokument, gestartet wird über den Allowlist-Eintrag der Sprache.
export const SCRIPT_LANGUAGES = Object.freeze({ javascript: 'javascript', js: 'javascript', typescript: 'typescript', ts: 'typescript' });

/** Sprache normalisieren; bash/sh/unbekannt → invalid (Erzeugung auf JavaScript bzw. TypeScript umstellen). */
export function scriptLanguage(language) {
  const key = String(language ?? 'javascript').trim().toLowerCase();
  const normalized = SCRIPT_LANGUAGES[key];
  if (!normalized) fail('invalid', `scrape script language ${JSON.stringify(language)} is not allowed (javascript or typescript; sh/bash rejected, §16.48)`);
  return normalized;
}

export function defaultEntryCommand(language) {
  return scriptLanguage(language) === 'typescript' ? ['tsx', '{script_path}'] : ['node', '{script_path}'];
}

const isoOf = (ms) => new Date(ms).toISOString();

// ---------------------------------------------------------------- targets

/** ref: registry.rs:18 list_targets — ORDER BY updated_at DESC, target_key ASC. */
export async function listTargets(ctx) {
  const rows = await ctx.read(TARGETS, {});
  return rows
    .filter((row) => typeof row?.target_key === 'string')
    .sort((a, b) => (a.updated_at < b.updated_at ? 1 : a.updated_at > b.updated_at ? -1 : a.target_key < b.target_key ? -1 : a.target_key > b.target_key ? 1 : 0));
}

export async function loadTarget(ctx, targetKey) {
  if (typeof targetKey !== 'string' || targetKey === '') return null;
  const doc = await ctx.get(TARGETS, targetKey);
  return isObject(doc) ? doc : null;
}

async function scriptRevisions(ctx, targetKey) {
  const rows = await ctx.read(SCRIPTS, { target_key: targetKey });
  return rows.filter((r) => Number.isInteger(r?.revision_no)).sort((a, b) => b.revision_no - a.revision_no);
}

/** ref: registry.rs:103-219 upsert_target (Manifest-Datei + SQLite → Collection-Dokument). */
export async function upsertTarget(ctx, payload) {
  if (!isObject(payload)) fail('invalid', 'target payload must be a json object');
  const startUrl = trimmedStr(payload, 'start_url');
  if (startUrl == null) fail('invalid', 'target payload requires non-empty start_url');
  const targetKey = slugify(typeof payload.target_key === 'string' ? payload.target_key
    : typeof payload.display_name === 'string' ? payload.display_name : startUrl);
  const displayName = trimmedStr(payload, 'display_name') ?? targetKey;
  const targetKind = typeof payload.target_kind === 'string' ? payload.target_kind : 'generic';
  const status = typeof payload.status === 'string' ? payload.status : 'active';
  const scheduleHint = typeof payload.schedule_hint === 'string' ? payload.schedule_hint : null;
  const config = normalizeTargetConfig(startUrl, targetKey, has(payload, 'config') ? payload.config : {});
  const outputSchema = has(payload, 'output_schema') ? clone(payload.output_schema) : {};
  const targetId = `scrape_target-${await stableDigest(targetKey)}`;
  const now = ctx.now();
  const updatedAt = isoOf(now);
  return ctx.transaction(async (tx) => {
    const existing = await tx.get(TARGETS, targetKey);
    // ON CONFLICT(target_key): created_at and the active script stay; everything else is replaced.
    const doc = {
      id: targetKey,
      target_id: isObject(existing) && typeof existing.target_id === 'string' ? existing.target_id : targetId,
      target_key: targetKey,
      display_name: displayName,
      start_url: startUrl,
      target_kind: targetKind,
      status,
      schedule_hint: scheduleHint,
      config,
      output_schema: outputSchema,
      // Rust: runtime/scraping/targets/<key>; jetzt der Arbeitsbereich-Schlüssel für process.run (cwd_ref).
      workspace_dir: `outbound-lead-generation/scrape/targets/${targetKey}`,
      latest_script_revision_no: isObject(existing) ? (existing.latest_script_revision_no ?? null) : null,
      latest_script_sha256: isObject(existing) ? (existing.latest_script_sha256 ?? null) : null,
      script_registered: isObject(existing) ? existing.script_registered === true : false,
      created_at: isObject(existing) && typeof existing.created_at === 'string' ? existing.created_at : updatedAt,
      updated_at: updatedAt,
      updated_at_ms: now,
    };
    await tx.put(TARGETS, doc);
    return doc;
  });
}

/** ref: registry.rs:34 show_target (= registered_target_summary, mod.rs:39). */
export async function registeredTargetSummary(ctx, targetKey) {
  const target = await loadTarget(ctx, targetKey);
  if (target == null) return null;
  const revisions = (await scriptRevisions(ctx, target.target_key)).slice(0, 20).map((r) => ({
    revision_no: r.revision_no,
    script_path: r.script_ref ?? null,
    language: r.language ?? 'javascript',
    script_sha256: r.script_sha256 ?? null,
    change_reason: r.change_reason ?? null,
    notes: r.notes ?? null,
    created_at: r.created_at ?? null,
  }));
  return {
    target_id: target.target_id,
    target_key: target.target_key,
    display_name: target.display_name,
    start_url: target.start_url,
    target_kind: target.target_kind,
    status: target.status,
    schedule_hint: target.schedule_hint ?? null,
    config: clone(target.config ?? {}),
    sources: normalizeSourcesFromConfig(get(target.config, 'sources'), target.start_url, target.target_key),
    output_schema: clone(target.output_schema ?? {}),
    workspace_dir: target.workspace_dir,
    latest_script_revision_no: target.latest_script_revision_no ?? null,
    latest_script_sha256: target.latest_script_sha256 ?? null,
    created_at: target.created_at,
    updated_at: target.updated_at,
    revisions,
  };
}

async function activeRevision(ctx, target) {
  const no = asI64(get(target, 'latest_script_revision_no'));
  if (no == null) return null;
  const doc = await ctx.get(SCRIPTS, `${target.target_key}:r${no}`);
  return isObject(doc) ? doc : null;
}

/**
 * ref: registry.rs:66-93 target_script_registration — the active revision is usable only if
 * its body is non-empty, matches the target's hash and the stored file still hashes to it.
 */
export async function targetScriptRegistration(ctx, targetKey) {
  const target = await loadTarget(ctx, targetKey);
  if (target == null) return null;
  const active = await activeRevision(ctx, target);
  let usable = false;
  if (active != null && typeof active.script_sha256 === 'string') {
    const body = await readFileText(ctx, active.script_ref);
    usable = body != null && body.trim() !== ''
      && target.latest_script_sha256 === active.script_sha256
      && (await sha256Hex(body.trim())) === active.script_sha256;
  }
  return {
    ok: true,
    target_key: target.target_key,
    registered_from: 'app_registry',
    workspace_dir: target.workspace_dir,
    script_registered: usable,
    revision_no: target.latest_script_revision_no ?? null,
    script_sha256: target.latest_script_sha256 ?? null,
  };
}

/** Aktives Skript samt Text für den Läufer; ref: registry.rs:749 load_registered_target + :810. */
export async function loadRegisteredTarget(ctx, targetKey) {
  const target = await loadTarget(ctx, targetKey);
  if (target == null) return null;
  const active = await activeRevision(ctx, target);
  if (active == null) return null;
  const body = await readFileText(ctx, active.script_ref);
  if (body == null || (await sha256Hex(body.trim())) !== active.script_sha256) {
    fail('unavailable', 'persisted scrape script body does not match registered SHA-256');
  }
  const entry = Array.isArray(active.entry_command) && active.entry_command.every((p) => typeof p === 'string')
    ? active.entry_command
    : (SCRIPT_LANGUAGES[String(active.language ?? 'javascript').trim().toLowerCase()] ? defaultEntryCommand(active.language) : null);
  return { target, script: { ...active, entry_command: entry }, body };
}

/**
 * ref: registry.rs:221-428 register_script — dedupe by sha256 of the trimmed body: a known
 * body reactivates its revision; otherwise revision_no = max + 1. Body is stored in `files`.
 */
export async function registerScript(ctx, { target_key: targetKey, script_text: scriptText, language = 'javascript', change_reason: changeReason = null, notes = null, entry_command: entryCommand = null }) {
  const target = await loadTarget(ctx, targetKey);
  if (target == null) fail('not_found', 'target_key not found');
  if (typeof scriptText !== 'string' || scriptText.trim() === '') fail('invalid', `scrape script for ${targetKey} must contain non-whitespace content`);
  language = scriptLanguage(language);
  const sha = await sha256Hex(scriptText.trim());
  const revisions = await scriptRevisions(ctx, target.target_key);
  const known = revisions.find((r) => r.script_sha256 === sha);
  const now = ctx.now();
  const at = isoOf(now);
  let revisionNo;
  let doc;
  if (known) {
    revisionNo = known.revision_no;
    doc = known;
  } else {
    revisionNo = (revisions[0]?.revision_no ?? 0) + 1;
    const file = await putText(ctx, scriptText, {
      name: `rev${String(revisionNo).padStart(4, '0')}_${sha.slice(0, 8)}`,
      content_type: 'text/plain',
      collection: SCRIPTS,
    });
    doc = {
      id: `${target.target_key}:r${revisionNo}`,
      target_key: target.target_key,
      target_id: target.target_id,
      revision_no: revisionNo,
      language,
      entry_command: Array.isArray(entryCommand) ? entryCommand : defaultEntryCommand(language),
      script_sha256: sha,
      script_ref: file.ref,
      size_bytes: file.bytes,
      change_reason: changeReason,
      notes,
      created_at: at,
      updated_at_ms: now,
    };
  }
  await ctx.transaction(async (tx) => {
    const current = await tx.get(TARGETS, target.target_key);
    if (current == null) fail('not_found', 'target_key not found');
    if (!known) await tx.put(SCRIPTS, doc);
    await tx.put(TARGETS, {
      ...current,
      latest_script_revision_no: revisionNo,
      latest_script_sha256: sha,
      script_registered: true,
      updated_at: at,
      updated_at_ms: now,
    });
  });
  return {
    ok: true,
    target_key: target.target_key,
    revision_no: revisionNo,
    script_sha256: sha,
    script_ref: doc.script_ref,
    deduplicated: Boolean(known),
  };
}

// ---------------------------------------------------------------- read views

const TEXT_MAX = 300;
const resultText = (result, key) => (typeof get(result, key) === 'string' ? takeChars(result[key], TEXT_MAX) : null);

function runView(run) {
  const result = isObject(run.result) ? run.result : null;
  return {
    run_id: run.run_id ?? run.id,
    status: run.status,
    started_at: run.started_at ?? null,
    finished_at: run.finished_at ?? null,
    failure_mode: resultText(result, 'failure_mode'),
    detail: resultText(result, 'detail'),
  };
}

const SUCCESS = new Set(['succeeded', 'completed_empty']);
const laterRun = (a, b) => (b == null || String(a.started_at ?? '') > String(b.started_at ?? '') ? a : b);

/**
 * ref: store_outbound_commands.rs:1357-1418 outbound_registry_last_runs — per target_id the
 * latest run and the latest succeeded/completed_empty run; failure_mode/detail ≤ 300 chars.
 */
export async function lastRuns(ctx) {
  const latest = new Map();
  const latestOk = new Map();
  for (const run of await ctx.read(RUNS, {})) {
    if (typeof run?.target_id !== 'string') continue;
    latest.set(run.target_id, laterRun(run, latest.get(run.target_id)));
    if (SUCCESS.has(run.status)) latestOk.set(run.target_id, laterRun(run, latestOk.get(run.target_id)));
  }
  const out = {};
  for (const [targetId, run] of latest) {
    const ok = latestOk.get(targetId);
    out[targetId] = { last_run: runView(run), last_successful_run: ok ? runView(ok) : null };
  }
  return out;
}

/** ref: store_outbound_commands.rs:1306-1355 outbound_registry_account_states (keyed by target_id). */
export async function accountStates(ctx) {
  const now = ctx.now();
  const out = {};
  for (const s of await ctx.read(ACCOUNT_STATES, {})) {
    if (typeof s?.target_id !== 'string') continue;
    out[s.target_id] = {
      status: 'provider_account_inactive',
      reason: takeChars(s.reason ?? '', TEXT_MAX),
      causal_run_id: s.causal_run_id,
      last_probe_run_id: s.last_probe_run_id,
      failed_probes: s.failed_probes,
      next_probe_at_ms: s.next_probe_at_ms,
      next_probe_at: Number.isInteger(s.next_probe_at_ms) ? isoOf(s.next_probe_at_ms) : null,
      probe_in_progress: typeof s.probe_lease_owner === 'string' && s.probe_lease_until_ms > now,
      updated_at_ms: s.updated_at_ms,
    };
  }
  return out;
}

/**
 * ref: store_outbound_commands.rs:1420-1481 outbound_registry_latest_script — highest
 * revision_no, bounded to max_bytes (UTF-8), with the German truncation marker.
 */
export async function latestScript(ctx, targetKey, maxBytes = 150_000) {
  const target = await loadTarget(ctx, targetKey);
  if (target == null) fail('not_found', `scrape target ${targetKey} could not be read: target not found`);
  const revision = (await scriptRevisions(ctx, target.target_key))[0];
  if (!revision) return { target_key: targetKey, available: false };
  const text = await readFileText(ctx, revision.script_ref);
  if (text == null) {
    return { target_key: targetKey, available: false, revision_no: revision.revision_no ?? null, error: 'script file not readable: file not found' };
  }
  const bytes = new TextEncoder().encode(text);
  const truncated = bytes.length > maxBytes;
  let out = truncated ? new TextDecoder('utf-8', { fatal: false }).decode(bytes.slice(0, maxBytes)) : text;
  if (truncated) out += '\n/* … gekürzt: Skript größer als 150 KB … */\n';
  return {
    target_key: targetKey,
    available: true,
    revision_no: revision.revision_no ?? null,
    language: revision.language ?? null,
    script_sha256: revision.script_sha256 ?? null,
    created_at: revision.created_at ?? null,
    truncated,
    text: out,
  };
}

/**
 * ref: outbound_update_digest.rs:362-427 load_registry_runs, the two SQL queries:
 * last = latest run per target (joined to target_key), ok = MAX(started_at) of
 * succeeded/completed_empty per target_key. Folding stays in update-digest.js.
 */
export async function registryRuns(ctx) {
  const keyById = new Map();
  for (const t of await ctx.read(TARGETS, {})) if (typeof t?.target_id === 'string') keyById.set(t.target_id, t.target_key);
  const latest = new Map();
  const ok = new Map();
  for (const run of await ctx.read(RUNS, {})) {
    const key = keyById.get(run?.target_id);
    if (key == null || typeof run.status !== 'string') continue;
    latest.set(key, laterRun(run, latest.get(key)));
    if (SUCCESS.has(run.status)) ok.set(key, laterRun(run, ok.get(key)));
  }
  return {
    last: [...latest].map(([key, run]) => ({
      target_key: key,
      status: run.status,
      started_at: run.started_at ?? '',
      result_json: isObject(run.result) ? JSON.stringify(run.result) : '',
    })),
    ok: [...ok].map(([key, run]) => ({ target_key: key, started_at: run.started_at ?? '' })),
  };
}

