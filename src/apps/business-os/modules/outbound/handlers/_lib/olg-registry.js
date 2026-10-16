// Scrape-Registry der Eigentümer-App outbound-lead-generation – Lesesicht und Befehle für outbound (Strang OUT3).
//
// Ersetzt die Core-Fähigkeiten scrape.registry.read, scrape.target.registration, scrape.target.upsert und
// scrape.repair_task.create. Die Registry ist App-Logik der OLG-App (Strang OLGSCR, CATALOG „kein scrape-Primitiv“):
//   - Lesen: foreign_collections der OLG-Exporte (outbound_lead_generation_scrape_targets/runs ['read','watch'],
//     …_account_states/…_scripts ['read']), Manifest-Wunsch in ../../V4-HANDLERS.md.
//   - Schreiben: nur über den von OLG angebotenen Befehl outbound.scrape.target.upsert (ctx.command, Routing
//     (app_id, type), §16.25). ctx.command liefert die Annahme, nicht das Ergebnis (§16.21).
//   - Skripttext: Befehl outbound.scrape.script.read an OLG mit Rückruf (Strang OLG725; Kern §16.47 Nr. 4: files.get
//     nur für die eigene App). outbound liest keine OLG-Dateien.
// Lesesichten inhaltsgleich mit customer-modules/outbound-lead-generation/handlers/_lib/scrape-registry.js.
// ref: capabilities/scrape/{mod.rs:1870 slugify, registry.rs:18 list_targets},
//      store_outbound_commands.rs:1306-1355 outbound_registry_account_states, :1357-1418 outbound_registry_last_runs,
//      :1420-1481 outbound_registry_latest_script.
import { fail, isObject } from './outbound.js';
import { OWNER_APP } from './owner-app.js';

export const OLG_SCRAPE = {
  targets: 'outbound_lead_generation_scrape_targets',
  scripts: 'outbound_lead_generation_scrape_scripts',
  runs: 'outbound_lead_generation_scrape_runs',
  accountStates: 'outbound_lead_generation_scrape_account_states',
  upsertCommand: 'outbound.scrape.target.upsert',
};

const has = (value, key) => isObject(value) && Object.prototype.hasOwnProperty.call(value, key);
const takeChars = (text, n) => Array.from(String(text)).slice(0, n).join('');

/** ref: mod.rs:1870 slugify – so legt OLG den target_key (= Dokument-id) an. */
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

/** Arbeitsbereich-Schlüssel, den OLG beim Anlegen eines Ziels vergibt (OLG _lib/scrape-registry.js upsertTarget). */
export const olgWorkspaceDir = (targetKey) => `outbound-lead-generation/scrape/targets/${slugify(targetKey)}`;

/** „manifest_path“ in OLG-Form (OLG outbound.scrape.target.upsert: `${TARGETS}/${target_key}`). */
export const olgManifestPath = (targetKey) => `${OLG_SCRAPE.targets}/${slugify(targetKey)}`;

/**
 * Ersatz für scrape.target.registration: Ziel-Dokument der OLG-Registry (foreign read). script_registered pflegt
 * OLG bei outbound.scrape.script.register (inkl. Hash-Prüfung des Skripttexts, registry.rs:66).
 */
export async function targetRegistration(ctx, targetKey) {
  const doc = await ctx.get(OLG_SCRAPE.targets, slugify(targetKey));
  if (!isObject(doc)) return null;
  return {
    ok: true,
    target_key: doc.target_key ?? slugify(targetKey),
    registered_from: 'app_registry',
    workspace_dir: doc.workspace_dir,
    script_registered: doc.script_registered === true,
    revision_no: doc.latest_script_revision_no ?? null,
    script_sha256: doc.latest_script_sha256 ?? null,
  };
}

/** Ersatz für scrape.target.upsert: Befehl an die Eigentümer-App (Annahme; OLG schreibt das Ziel). */
export async function requestTargetUpsert(ctx, manifest, commandId) {
  if (typeof ctx.command !== 'function') fail('unavailable', 'ctx.command is not available');
  const sent = (await ctx.command(OLG_SCRAPE.upsertCommand, { manifest }, { id: commandId, module: OWNER_APP })) || {};
  const status = typeof sent.status === 'string' ? sent.status : 'accepted';
  if (status === 'rejected' || status === 'failed') {
    fail('unavailable', `${OWNER_APP} rejected ${OLG_SCRAPE.upsertCommand}: ${sent.reason ?? 'no reason'}`);
  }
  return { command_id: typeof sent.id === 'string' && sent.id !== '' ? sent.id : commandId, status };
}

// ---------------------------------------------------------------- Lesesichten (Ersatz für scrape.registry.read)

/** ref: registry.rs:18 list_targets — ORDER BY updated_at DESC, target_key ASC. */
export async function listTargets(ctx) {
  const rows = await ctx.read(OLG_SCRAPE.targets, {});
  return rows
    .filter((row) => typeof row?.target_key === 'string')
    .sort((a, b) => (a.updated_at < b.updated_at ? 1 : a.updated_at > b.updated_at ? -1 : a.target_key < b.target_key ? -1 : a.target_key > b.target_key ? 1 : 0));
}

const TEXT_MAX = 300;
const resultText = (result, key) => (has(result, key) && typeof result[key] === 'string' ? takeChars(result[key], TEXT_MAX) : null);

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

/** ref: :1357-1418 — je target_id letzter Lauf und letzter erfolgreicher (succeeded/completed_empty) Lauf. */
export async function lastRuns(ctx) {
  const latest = new Map();
  const latestOk = new Map();
  for (const run of await ctx.read(OLG_SCRAPE.runs, {})) {
    if (typeof run?.target_id !== 'string') continue;
    latest.set(run.target_id, laterRun(run, latest.get(run.target_id)));
    if (SUCCESS.has(run.status)) latestOk.set(run.target_id, laterRun(run, latestOk.get(run.target_id)));
  }
  const out = {};
  for (const [targetId, run] of latest) {
    const ok = latestOk.get(targetId);
    out[targetId] = { last: runView(run), last_successful: ok ? runView(ok) : null };
  }
  return out;
}

/** ref: :1306-1355 — je target_id; probe_in_progress nur bei nicht abgelaufener Lease (INV-SCR4). */
export async function accountStates(ctx) {
  const now = ctx.now();
  const out = {};
  for (const s of await ctx.read(OLG_SCRAPE.accountStates, {})) {
    if (typeof s?.target_id !== 'string') continue;
    out[s.target_id] = {
      status: 'provider_account_inactive',
      reason: takeChars(s.reason ?? '', TEXT_MAX),
      causal_run_id: s.causal_run_id,
      last_probe_run_id: s.last_probe_run_id,
      failed_probes: s.failed_probes,
      next_probe_at_ms: s.next_probe_at_ms,
      next_probe_at: Number.isInteger(s.next_probe_at_ms) ? new Date(s.next_probe_at_ms).toISOString() : null,
      probe_in_progress: typeof s.probe_lease_owner === 'string' && s.probe_lease_until_ms > now,
      updated_at_ms: s.updated_at_ms,
    };
  }
  return out;
}

// ---------------------------------------------------------------- Skripttext (Befehl an OLG, Strang OLG725)

export const SCRIPT_READ_COMMAND = 'outbound.scrape.script.read';
/** Rückruf-Befehl der eigenen App für on_complete UND on_failed (§16.30 Nr. 3, §16.36). */
export const SCRIPT_READ_DONE = 'outbound.research_source.script_read.done';
export const SCRIPT_READ_SCHEMA = 'ctox.outbound_lead_generation.scrape_script.v1';

/**
 * Ersatz für das frühere Lesen des Skripttexts über ctx.cap.files.get: Kern §16.47 Nr. 4 lässt files.get nur für die
 * eigene App zu, die Skripte gehören outbound-lead-generation. outbound schickt deshalb den von OLG angebotenen Befehl
 * outbound.scrape.script.read (commands_out ↔ commands_in from:['outbound'], §16.40); OLG liest mit seinen eigenen
 * Dateien (ref: store_outbound_commands.rs:1420-1481) und der Kern ruft SCRIPT_READ_DONE mit dem Ergebnis zurück.
 * Vorab prüft outbound per foreign read, dass das Ziel existiert (Rust :1420 not_found bleibt synchron).
 * Liefert die Annahme {target_key, status:'requested', command_id}, nie den Text.
 */
export async function requestLatestScript(ctx, targetKey, commandId) {
  const key = slugify(targetKey);
  const target = await ctx.get(OLG_SCRAPE.targets, key);
  if (!isObject(target)) fail('not_found', `scrape target ${targetKey} could not be read: target not found`);
  if (typeof ctx.command !== 'function') fail('unavailable', 'ctx.command is not available');
  const sent = (await ctx.command(SCRIPT_READ_COMMAND, { target_key: targetKey }, {
    id: commandId, module: OWNER_APP, on_complete: SCRIPT_READ_DONE, on_failed: SCRIPT_READ_DONE,
  })) || {};
  const status = typeof sent.status === 'string' ? sent.status : 'accepted';
  if (status === 'rejected' || status === 'failed') {
    fail('unavailable', `${OWNER_APP} rejected ${SCRIPT_READ_COMMAND}: ${sent.reason ?? 'no reason'}`);
  }
  return { target_key: targetKey, status: 'requested', command_id: typeof sent.id === 'string' && sent.id !== '' ? sent.id : commandId };
}
