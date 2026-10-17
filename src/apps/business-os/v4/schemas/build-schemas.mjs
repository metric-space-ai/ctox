#!/usr/bin/env node
// Shell v4 – Strang SCH: korrigierte Collection-Schemata für den Basis-Release-Import der Schema-Registry
// (Architektur §16.21 „Schemata“, Format A9 §16.22). Erzeugt je App genau eine Datei
// v4/schemas/<app_id>.collections.schema.json plus migrations/<app>/<collection>.js.
//
// Quellen (nur gelesen):
//   modules/<app>/collections.schema.json, customer-modules/outbound-lead-generation/collections.schema.json
//   v4/tools/manifest/out/ownership.json (Eigentümer), v4/tools/manifest/out/drift.json (Migrationsstufen-Drift)
//   v4/manifests/<app>.manifest.json (Schemaquelle, wenn die Eigentümer-JSON die Collection nicht hat)
// Korrekturen: Tabelle FIXES unten; jede Zeile trägt ihren Rust-Beleg (Datei:Zeile im Schnappschuss
// ~/.local/state/shell-v4-src/4794af03d/src/core/business_os/).
//
// Aufruf: node src/apps/business-os/v4/schemas/build-schemas.mjs   (idempotent; überschreibt nur v4/schemas/*)

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BOS = path.resolve(HERE, '../..');
const BASE_RELEASE = 'main@4794af03d';
const FORMAT = 'ctox-v4-collections-a9';
const { classifyCollections, parseCollectionsFile } = await import(pathToFileURL(path.join(BOS, 'v4/source/schema-diff.js')).href);

const readJson = (p) => JSON.parse(fs.readFileSync(p, 'utf8'));
const clone = (v) => JSON.parse(JSON.stringify(v));

// ---------------------------------------------------------------- Apps und Eigentum

const PLATFORM = 'ctox-platform'; // §16.17: Plattformmanifest app_id='ctox-platform' (ownership.json: owner "shell")
const apps = {};
for (const dir of fs.readdirSync(path.join(BOS, 'modules')).sort()) {
  const f = path.join(BOS, 'modules', dir, 'collections.schema.json');
  if (fs.existsSync(f)) apps[dir] = { id: dir, src: `modules/${dir}/collections.schema.json`, json: readJson(f) };
}
apps['outbound-lead-generation'] = {
  id: 'outbound-lead-generation',
  src: 'customer-modules/outbound-lead-generation/collections.schema.json',
  json: readJson(path.join(BOS, 'customer-modules/outbound-lead-generation/collections.schema.json')),
};

const ownership = readJson(path.join(BOS, 'v4/tools/manifest/out/ownership.json')).ownership;
// Seit M2 (15.10. 15:22) trägt ownership.json die Kern-Entscheide §16.17/§16.21 selbst (owner ctox-platform bzw. OLG
// für outbound_lead_generation_adapters, sellify_*, outbound_research_policies); ältere Stände nennen 'shell'.
function ownerOf(name) {
  const o = ownership[name];
  if (o) return { owner: o.owner === 'shell' ? PLATFORM : o.owner, basis: `ownership.json: ${o.basis}` };
  const definers = Object.values(apps).filter((a) => a.json.collections?.[name]).map((a) => a.id);
  if (definers.length === 1) return { owner: definers[0], basis: 'nicht in ownership.json; einziger Definierer in collections.schema.json (neu seit Basis)' };
  throw new Error(`kein Eigentümer für ${name}: ${definers.join(',')}`);
}

// Plattformmanifest des Kerns (nur gelesen): Schemata der v4-eigenen Plattform-Collections (app_activations, …).
const PLATFORM_MANIFEST_PATH = path.resolve(BOS, '../../core/business_os/v4/ctox-platform.manifest.json');
const platformManifest = fs.existsSync(PLATFORM_MANIFEST_PATH) ? readJson(PLATFORM_MANIFEST_PATH) : { collections: {} };

const manifests = {};
for (const f of fs.readdirSync(path.join(BOS, 'v4/manifests'))) {
  if (f.endsWith('.manifest.json')) { const m = readJson(path.join(BOS, 'v4/manifests', f)); manifests[m.app_id] = m; }
}

// Hülle {schema, syncProfile|conflictStrategy|deleteStrategy, …} bleibt erhalten: die Geschwister sind hash-neutral
// (rxdb/src/rx-database.mjs:137-153 @4794af03d) und werden von schema-diff.js nicht verglichen.
const splitEntry = (entry) => {
  if (entry && entry.schema && typeof entry.schema === 'object' && !entry.properties) {
    const { schema, ...meta } = entry;
    return { schema, meta };
  }
  return { schema: entry, meta: {} };
};

// conflictStrategy 'field-merge' stand nur in schema.js (Geschwister von schema), nie in collections.schema.json.
// Ohne Übernahme fiele der Feld-Merge (docs/ctox-rxdb.md §8.2) mit dem Wegfall von schema.js (§16.13) weg.
const FIELD_MERGE = {
  notes: 'modules/notes/schema.js:29 @4794af03d',
  calendar_events: 'modules/calendar/schema.js:177 @4794af03d',
  customer_accounts: 'modules/customers/schema.js:431 @4794af03d',
  customer_contacts: 'modules/customers/schema.js:432 @4794af03d',
  customer_opportunities: 'modules/customers/schema.js:433 @4794af03d',
  customer_tasks: 'modules/customers/schema.js:434 @4794af03d',
  customer_notes: 'modules/customers/schema.js:435 @4794af03d',
};

// ---------------------------------------------------------------- Korrekturen (Befunde)
// op: type (properties[field].type ersetzen), prop (Feld ergänzen/ersetzen), required (Pflichtliste setzen),
//     note (keine Schemaänderung, nur Bericht). Jede Zeile: ev = Beleg, src = Befundquelle.

const RS = 'rs:'; // Präfix = ~/.local/state/shell-v4-src/4794af03d/src/core/business_os/
const FIXES = {
  kundenpipeline: {
    kundenpipeline_vorgaenge: [
      { op: 'type', field: 'triage_json', type: ['object', 'null'], ev: `${RS}decision_hub.rs:143`, src: 'kundenpipeline/V4-HANDLERS.md B1; handlers/tests/schema-befund.test.mjs' },
      { op: 'type', field: 'run_json', type: ['object', 'null'], ev: `${RS}decision_hub.rs:144`, src: 'kundenpipeline/V4-HANDLERS.md B1; handlers/tests/schema-befund.test.mjs' },
    ],
    kundenpipeline_projekte: [
      { op: 'note', field: 'adressen/domains/active', ev: `${RS}decision_hub.rs:1229, :1248, :1249 (liest aktiv, adressen_json, domains_json; schreibt kundenpipeline_projekte nirgends)`, src: 'kundenpipeline/V4-HANDLERS.md B2', text: 'Rust LIEST adressen_json/domains_json/aktiv, schreibt die Collection aber nicht → Schema bleibt (adressen/domains/active wie UI). Befund für Handler-/Ingest-Port: Routing muss die Schemanamen lesen.' },
    ],
  },
  invoices: {
    accounting_dunning_runs: [
      { op: 'prop', field: 'invoices_total', def: { type: 'array', items: { type: 'string' } }, ev: `${RS}invoices.rs:1693, :1704`, src: 'invoices/V4-HANDLERS.md I4; handlers/tests/schema-befund.test.mjs I4', was: 'number' },
      { op: 'prop', field: 'updated_at_ms', def: { type: 'number' }, ev: `${RS}invoices.rs:1708; store_projections.rs:1409 (upsert_business_record setzt updated_at_ms)`, src: 'Rust-Abgleich SCH' },
    ],
    accounting_payment_allocations: [
      { op: 'type', field: 'note', type: ['string', 'null'], ev: `${RS}invoices.rs:1588`, src: 'invoices/V4-HANDLERS.md I5; schema-befund.test.mjs I5' },
      { op: 'type', field: 'payment_id', type: ['string', 'null'], ev: `${RS}invoices.rs:1496-1500 (Option<String>), :1584`, src: 'invoices/V4-HANDLERS.md I5; schema-befund.test.mjs I5' },
      { op: 'prop', field: 'updated_at_ms', def: { type: 'number' }, ev: `${RS}invoices.rs:1591`, src: 'Rust-Abgleich SCH' },
    ],
    accounting_dunning_letters: [
      { op: 'type', field: 'pdf_attachment_id', type: ['string', 'null'], ev: `${RS}invoices.rs:1727`, src: 'invoices/V4-HANDLERS.md I6; schema-befund.test.mjs I6' },
      { op: 'type', field: 'sent_via', type: ['string', 'null'], ev: `${RS}invoices.rs:1728`, src: 'invoices/V4-HANDLERS.md I6; schema-befund.test.mjs I6' },
      { op: 'type', field: 'sent_at_ms', type: ['number', 'null'], ev: `${RS}invoices.rs:1729`, src: 'invoices/V4-HANDLERS.md I6; schema-befund.test.mjs I6' },
      { op: 'prop', field: 'updated_at_ms', def: { type: 'number' }, ev: `${RS}invoices.rs:1732`, src: 'Rust-Abgleich SCH' },
    ],
  },
  matching: Object.fromEntries(['matching_requirements', 'matching_objects', 'matching_results'].map((c) => [c, [
    { op: 'required', list: ['id', 'kind', 'updated_at_ms'], ev: `${RS}importer.rs:${{ matching_requirements: '724-799, :804-824', matching_objects: '939-991', matching_results: '1084-1107' }[c]} (flache Dokumente: id, kind, camelCase createdAt/updatedAt, kein data; kind=source/requirementSource/match ohne title, match ohne title); updated_at_ms setzt store_projections.rs:1409`, src: 'matching/V4-HANDLERS.md Abweichung 1 (Schema-Hülle)', was: 'id, kind, title, status, data, updated_at_ms' },
  ]])),
  outbound: {
    outbound_suppression_entries: [
      { op: 'required', list: ['id', 'reason', 'created_at_ms', 'updated_at_ms'], ev: `${RS}store_outbound_delivery_policy.rs:127-138`, src: 'outbound/V4-HANDLERS.md P9', was: 'id, scope, value, reason, active, created_at_ms, updated_at_ms' },
      ...['email', 'domain', 'status', 'suppression_reason', 'engagement_id', 'recipient_email'].map((field) => ({
        op: 'prop', field, def: { type: 'string' }, ev: field === 'recipient_email' ? `${RS}store_outbound_delivery_policy.rs:200 (gelesen; UI/Altbestand)` : `${RS}store_outbound_delivery_policy.rs:${{ email: 129, domain: 130, status: 131, suppression_reason: 133, engagement_id: 135 }[field]}`, src: 'outbound/V4-HANDLERS.md P9',
      })),
      { op: 'note', field: 'scope/value/active', ev: `${RS}store_outbound_delivery_policy.rs:175-215 (Versand-Gate liest nur email/recipient_email/domain/status/reason)`, src: 'outbound/V4-HANDLERS.md P9', text: 'Befund für den Handler-/UI-Port: Einträge in der Schemaform scope/value/active (UI) sieht das Rust-Versand-Gate nicht. Felder bleiben deklariert (optional).' },
    ],
    outbound_sender_assignments: [
      { op: 'required', list: ['id', 'sender_account_id', 'status', 'created_at_ms', 'updated_at_ms'], ev: `${RS}store_outbound_commands.rs:192-205 (outbound.engagement.assign_sender: Payload + id/engagement_id/sender_account_id/status, kein campaign_id)`, src: 'outbound/V4-HANDLERS.md P9', was: 'id, campaign_id, sender_account_id, status, created_at_ms, updated_at_ms' },
    ],
  },
  'outbound-lead-generation': {
    outbound_lead_generation_leads: [
      { op: 'prop', field: 'field_status', def: { type: 'object', additionalProperties: true }, ev: `${RS}contact_email_validation.rs:415-418 (lead["field_status"] = {} + Eintrag je Feld); outbound_field_review.rs:42 (Schreibwächter liest es)`, src: 'gen-manifest.mjs write_guard _todo (OLG); OLG V4-PORT.md §16.5' },
      { op: 'prop', field: 'person_field_status', def: { type: 'object', additionalProperties: true }, ev: `${RS}contact_email_validation.rs:364-375 (person_field_status[person_key][feld])`, src: 'gen-manifest.mjs write_guard _todo (OLG); OLG V4-PORT.md §16.5' },
    ],
    outbound_research_policies: [{ op: 'collection', ev: `${RS}store_outbound_commands.rs:4547-4592 (record = Payload + app, published_at_ms, policy_contract, id 'policy:<slug>'; updated_at_ms über store_projections.rs:1409)`, src: 'outbound/V4-HANDLERS.md P9/P20; OLG V4-PORT.md §16.4; Kern §16.21 Eigentum OLG',
      schema: { type: 'object', version: 0, primaryKey: 'id', additionalProperties: true, required: ['id', 'app', 'published_at_ms', 'policy_contract', 'updated_at_ms'],
        properties: { id: { type: 'string', maxLength: 180 }, app: { type: 'string' }, published_at_ms: { type: 'number' }, policy_contract: { type: 'string' }, updated_at_ms: { type: 'number' } } } }],
    ...Object.fromEntries(['sellify_companies', 'sellify_people', 'sellify_campaigns'].map((c) => [c, [{ op: 'collection', ev: `${RS}external_sql_sync.rs:612-638, :689-700 (Zeile = konfigurierte SQL-Spalten + id, is_deleted, _ctox_external_sql, updated_at_ms; Löschung: id, is_deleted, deleted_at_ms)`, src: 'OLG V4-PORT.md §16.3; Kern §16.21 Eigentum OLG',
      schema: { type: 'object', version: 0, primaryKey: 'id', additionalProperties: true, required: ['id', 'is_deleted', 'updated_at_ms'],
        properties: { id: { type: 'string', maxLength: 512 }, is_deleted: { type: 'boolean' }, deleted_at_ms: { type: 'number' }, updated_at_ms: { type: 'number' } } } }]])),
  },
  ctox: {
    workjet_session_transfers: [
      ...[
        ['applied_at_ms', { type: 'integer', minimum: 0 }, 2047],
        ['apply_mismatch_count', { type: 'integer', minimum: 0 }, 1996],
        ['apply_mismatch_sha256', { type: 'string', maxLength: 64 }, 1995],
        ['last_terminal_turn_id', { type: ['string', 'null'], maxLength: 160 }, '1385-1388'],
        ['observed_head', { type: 'string', maxLength: 64 }, 2050],
        ['observed_manifest_sha256', { type: 'string', maxLength: 64 }, 2048],
        ['observed_tree_sha256', { type: 'string', maxLength: 64 }, 2053],
        ['packed_at_ms', { type: 'integer', minimum: 0 }, 1744],
        ['pause_acked_at_ms', { type: 'integer', minimum: 0 }, 1389],
        ['recovery_aborting_at_ms', { type: 'integer', minimum: 0 }, 272],
        ['source_git_repository', { type: 'boolean' }, 1384],
        ['switched_at_ms', { type: 'integer', minimum: 0 }, 2346],
        ['target_partial_cleanup_required', { type: 'boolean' }, 275],
        ['worker_instance_id', { type: 'string', maxLength: 256 }, 2566],
      ].map(([field, def, line]) => ({ op: 'prop', field, def, ev: `${RS}store_workjet_sessions.rs:${line} (transfer["${field}"]; Projektion ungefiltert :2969-2977, additionalProperties:false)`, src: 'Rust-Abgleich SCH (ctox, additionalProperties:false)' })),
    ],
    workjet_sessions: [
      { op: 'type', field: 'last_terminal_turn_id', type: ['string', 'null'], ev: `${RS}store_workjet_sessions.rs:1393-1396 (unwrap_or(Value::Null)); Projektion :2959-2967`, src: 'Rust-Abgleich SCH (ctox)' },
    ],
    workjet_computers: [
      { op: 'note', field: 'capability_config/capability_epoch/agentless/native_device_binding', ev: `${RS}store_workjet_computers.rs:360-377 schreibt sie in den nativen Datensatz, :543-552 entfernt sie vor der RxDB-Projektion`, src: 'Rust-Abgleich SCH (ctox)', text: 'Bestand der Collection erfüllt das Schema (Rust filtert). Für den v4-Einheits-Store muss die Capability agent/computer diese Felder weiter außerhalb des Dokuments halten oder die „koordinierte Workjet-Schemaanhebung“ (Kommentar :543) nachholen – nicht im Schema geraten.' },
    ],
  },
  support: {
    support_view_filters: [
      { op: 'prop', field: 'value', def: { type: ['object', 'array', 'string', 'number', 'boolean', 'null'] }, ev: `${RS}support.rs:1936 (beliebiger JSON-Wert aus Payload, Rückfall Value::Null), :3493 (Projektion)`, src: 'Rust-Abgleich SCH (support)', was: 'object' },
    ],
  },
  threads: {
    user_thread_states: [
      { op: 'note', field: 'pinned/unread_count/last_seen_at_ms/snoozed_until_ms/follow_up_at_ms', ev: 'threads/V4-HANDLERS.md U-9 (Rust übernimmt Payloadwerte ungeprüft)', src: 'threads/V4-HANDLERS.md U-9', text: 'Durchreichen von Clientwerten, kein von Rust selbst geschriebenes null; UI sendet diese Felder nie als null (grep modules/threads, shared). Keine Schemaänderung; Bestandsmessung empfohlen.' },
    ],
  },
  // MAIL2 (15.10.2026, Kern §16.28): App-API 4.0 adressiert Dokumente über `id`; die Projektion schreibt `id` = Schlüssel
  // bereits seit der Basis (src/core/mission/channels/business_os_projection.rs:541-542, :657-658, :775-776 @4794af03d;
  // nicht im rs:-Schnappschuss, Beleg aus `git show 4794af03d:<pfad>`). Schema folgt: Primärschlüssel id, *_key bleibt
  // Pflicht- und Indexfeld. Migration kopiert id ← *_key (MIGRATE_BODY/RXDB_OPS unten). Befund mail V4-PORT.md OFFEN 6.
  conversations: Object.fromEntries([['communication_accounts', 'account_key', '541-542'], ['communication_threads', 'thread_key', '657-658'], ['communication_messages', 'message_key', '775-776']].map(([c, key, lines]) => {
    const ev = `src/core/mission/channels/business_os_projection.rs:${lines} @4794af03d ("id": ${key.replace('_key', '')}_key, "${key}": …)`;
    const src = 'mail/V4-PORT.md OFFEN 6; Strang MAIL2 C; Kern §16.28';
    return [c, [
      { op: 'prop', field: 'id', def: { type: 'string', maxLength: 256 }, ev, src },
      { op: 'primaryKey', key: 'id', ev, src },
      { op: 'requiredAdd', field: 'id', ev, src },
      { op: 'indexes', list: [key], ev, src },
    ]];
  })),
  customers: {
    customer_accounts: [
      { op: 'note', field: 'employee_count', ev: `${RS}store_customer_commands.rs:135-158 (customers_merge_fields kopiert Payloadwerte ungeprüft)`, src: 'customers/V4-HANDLERS.md A2', text: 'Nur Durchreichen von Clientwerten (z. B. "50" als String); Rust erzeugt den Typ nicht selbst. Keine Schemaänderung; Bestandsmessung empfohlen.' },
    ],
    customer_opportunities: [
      { op: 'note', field: 'primary_contact_id', ev: `${RS}store_customer_commands.rs:499-515 (merge)`, src: 'customers/V4-HANDLERS.md A2', text: 'null nur, wenn ein Client null sendet; keine Schemaänderung.' },
    ],
  },
};

// Migrationsstufen, die schema.js (Basis) hat und collections.schema.json nicht (DRIFT.md §2). Die Funktion
// retainV0BrowserDocument (modules/browser/schema.js:235 @4794af03d) ist die Identität → deklarativ operations: [].
const LEGACY_JS_STEPS = {
  browser_frames: { steps: [1], ev: 'modules/browser/schema.js:240 @4794af03d (retainV0BrowserDocument = ({...oldDoc}), :235)' },
  browser_input_events: { steps: [1], ev: 'modules/browser/schema.js:241 @4794af03d' },
  browser_sessions: { steps: [1], ev: 'modules/browser/schema.js:238 @4794af03d' },
  browser_tabs: { steps: [1], ev: 'modules/browser/schema.js:239 @4794af03d' },
};

// ---------------------------------------------------------------- Aufbau

const out = {}; // app -> { collections, migrations, report }
const ensureApp = (id) => (out[id] ||= { collections: {}, meta: {}, base: {}, migrations: {}, rxdb: {}, report: [], dropped: [], sources: {} });
for (const id of Object.keys(apps)) ensureApp(id);
ensureApp(PLATFORM);

const allNames = new Set([...Object.keys(ownership), ...Object.values(apps).flatMap((a) => Object.keys(a.json.collections || {}))]);
const CREATED = new Set(Object.values(FIXES).flatMap((cols) => Object.entries(cols).filter(([, fx]) => fx.some((f) => f.op === 'collection')).map(([n]) => n)));
for (const name of [...allNames].sort()) {
  if (CREATED.has(name)) continue;
  const { owner, basis } = ownerOf(name);
  const target = ensureApp(owner);
  const ownerJson = owner === PLATFORM ? apps.ctox.json : apps[owner]?.json;
  let schema = ownerJson?.collections?.[name];
  let source = owner === PLATFORM ? apps.ctox.src : apps[owner]?.src;
  let strategies = ownerJson?.migration_strategies?.[name];
  if ((!schema || name === 'business_commands') && owner === PLATFORM && platformManifest.collections?.[name]?.schema) {
    const { _note, schema: ps, ...pmeta } = platformManifest.collections[name];
    schema = { schema: ps, ...pmeta };
    source = 'src/core/business_os/v4/ctox-platform.manifest.json';
    target.report.push({ collection: name, field: '(Collection)', old: 'fehlt (v4-Plattform-Collection)', next: 'aus dem Plattformmanifest übernommen (Schema + class/rights/client_limit/byte_budget)', ev: `${source} collections.${name}`, src: 'ownership.json platform; Kern §16.17', kind: 'kompatibel (neue Collection)' });
  }
  if (!schema) {
    const m = manifests[owner]?.collections?.[name];
    // Manifeste lesen seit M2 v4/schemas zurück: diese Quelle wäre zirkulär und zählt nicht.
    if (!m?.schema || String(m._schema_source ?? '').startsWith('v4/')) {
      target.report.push({ collection: name, field: '(Collection)', old: '–', next: 'nicht aufgenommen', ev: `ownership.json owner ${owner}; keine Schemaquelle außer v4/schemas`, src: 'Strang SCH', kind: 'note', text: owner === PLATFORM ? 'Plattform-Collection ohne Schema im Plattformmanifest (z. B. app_drafts, §16.22 Kern-intern); Schema liefert der Kern.' : 'Keine Schemaquelle; wird über FIXES angelegt oder ist Kernsache.' });
      continue;
    }
    schema = m.schema;
    source = m._schema_source;
    const other = Object.values(apps).find((a) => a.src === source);
    strategies = other?.json.migration_strategies?.[name];
  }
  const { schema: plain, meta } = splitEntry(schema);
  target.collections[name] = clone(plain);
  target.base[name] = clone(plain);
  target.meta[name] = clone(meta);
  // Plattform-Collections: Klasse/Rechte/Limits aus dem Plattformmanifest (kompatibel, §16.4); Schema ist identisch.
  if (owner === PLATFORM && platformManifest.collections?.[name]) {
    const { _note, schema: _s, ...pmeta } = platformManifest.collections[name];
    target.meta[name] = { ...pmeta, ...target.meta[name] };
    if (!String(source).includes('ctox-platform.manifest.json')) target.report.push({ collection: name, field: '(class/rights/client_limit/byte_budget)', old: 'nicht gesetzt', next: JSON.stringify(pmeta), ev: `~/.local/state/ctox-sync-v4/docs/v4-manifests/ctox-platform.manifest.json collections.${name} (Schema identisch zu ${source})`, src: 'Kern §16.17 Plattformmanifest', kind: 'kompatibel' });
  }
  target.sources[name] = { source, basis };
  if (FIELD_MERGE[name] && target.meta[name].conflictStrategy !== 'field-merge') {
    target.meta[name].conflictStrategy = 'field-merge';
    target.report.push({ collection: name, field: '(conflictStrategy)', old: 'fehlt in collections.schema.json', next: '"field-merge" (Hüllen-Geschwister)', ev: FIELD_MERGE[name], src: 'Befund notes conflictStrategy (nur altes schema.js)', kind: 'kompatibel' });
  }
  // RxDB-Versionshistorie (migration_strategies, Format ctox-business-os-module-collections-v1) bleibt als eigener
  // Schlüssel erhalten; sie ist KEINE A9-Migration (A9 = Basis-App-Version → neue Fassung).
  if (strategies && Object.keys(strategies).length) target.rxdb[name] = clone(strategies);
  if (LEGACY_JS_STEPS[name]) {
    const l = LEGACY_JS_STEPS[name];
    const have = Object.keys(target.rxdb[name] ?? {});
    const add = l.steps.filter((v) => !have.includes(String(v)));
    if (add.length) {
      target.rxdb[name] = { ...(target.rxdb[name] ?? {}), ...Object.fromEntries(add.map((v) => [String(v), { operations: [] }])) };
      target.report.push({ collection: name, field: '(migration_strategies)', old: `[${have.join(',')}]`, next: `+ ${add.join(',')} (Identität)`, ev: l.ev, src: 'v4/tools/manifest/out/DRIFT.md §2', kind: 'kompatibel' });
    }
  }
}
// Fremdkopien je App protokollieren (werden nicht übernommen: §16.13 foreign_collections).
for (const app of Object.values(apps)) {
  for (const name of Object.keys(app.json.collections || {})) {
    const { owner } = ownerOf(name);
    if (owner !== app.id) out[app.id].dropped.push({ name, owner });
  }
}

// Korrekturen anwenden
for (const [appId, cols] of Object.entries(FIXES)) {
  const app = ensureApp(appId);
  for (const [name, fixes] of Object.entries(cols)) {
    for (const fx of fixes) {
      if (fx.op === 'collection') {
        if (app.collections[name]) throw new Error(`${name} existiert bereits`);
        app.collections[name] = clone(fx.schema);
        app.sources[name] = { source: '(neu, Strang SCH)', basis: 'Kern §16.21 Eigentum' };
        app.report.push({ collection: name, field: '(Collection)', old: 'fehlt (nur nativ business_records)', next: 'neu angelegt', ev: fx.ev, src: fx.src, kind: 'kompatibel (neue Collection)' });
        continue;
      }
      const s = app.collections[name];
      if (!s) throw new Error(`${appId}/${name} fehlt`);
      if (fx.op === 'note') { app.report.push({ collection: name, field: fx.field, old: '–', next: 'unverändert', ev: fx.ev, src: fx.src, kind: 'note', text: fx.text }); continue; }
      if (fx.op === 'type') {
        const old = s.properties[fx.field]?.type;
        if (old === undefined) throw new Error(`${name}.${fx.field} fehlt`);
        s.properties[fx.field] = { ...s.properties[fx.field], type: fx.type };
        app.report.push({ collection: name, field: fx.field, old: JSON.stringify(old), next: JSON.stringify(fx.type), ev: fx.ev, src: fx.src });
      } else if (fx.op === 'prop') {
        const old = s.properties[fx.field];
        s.properties[fx.field] = clone(fx.def);
        app.report.push({ collection: name, field: fx.field, old: old ? JSON.stringify(old) : 'nicht deklariert', next: JSON.stringify(fx.def), ev: fx.ev, src: fx.src });
      } else if (fx.op === 'primaryKey') {
        const old = s.primaryKey;
        if (!(fx.key in s.properties)) throw new Error(`${name}.${fx.key} fehlt (primaryKey)`);
        s.primaryKey = fx.key;
        app.report.push({ collection: name, field: 'primaryKey', old: JSON.stringify(old), next: JSON.stringify(fx.key), ev: fx.ev, src: fx.src });
      } else if (fx.op === 'requiredAdd') {
        const old = s.required;
        if (!old.includes(fx.field)) s.required = [fx.field, ...old];
        app.report.push({ collection: name, field: 'required', old: `[${old.join(', ')}]`, next: `[${s.required.join(', ')}]`, ev: fx.ev, src: fx.src });
      } else if (fx.op === 'indexes') {
        const old = s.indexes;
        s.indexes = [...(old ?? []), ...fx.list.filter((i) => !(old ?? []).includes(i))];
        app.report.push({ collection: name, field: 'indexes', old: old ? JSON.stringify(old) : 'keine', next: JSON.stringify(s.indexes), ev: fx.ev, src: fx.src });
      } else if (fx.op === 'required') {
        const old = s.required;
        s.required = [...fx.list];
        app.report.push({ collection: name, field: 'required', old: `[${old.join(', ')}]`, next: `[${fx.list.join(', ')}]`, ev: fx.ev, src: fx.src });
      } else throw new Error(`op ${fx.op}`);
    }
  }
}

// Drift-Befunde (DRIFT.md §2, §7), die das Eigentum bereits auflöst: Bericht ohne Schemaänderung, mit Nachweis.
const DRIFT_NOTES = [
  ['ctox_queue_tasks', 'DRIFT.md §2 threads/reports [1,2] vs. schema.js [1,2,3]', 'Eigentümer ctox-platform übernimmt die ctox-Fassung mit migration_strategies 1,2,3 (modules/ctox/collections.schema.json); die Kopien in threads/reports (nur 1,2) werden als Fremdkopie nicht übernommen.'],
  ['workjet_projects', 'DRIFT.md §2 reports [] vs. schema.js [1..5]', 'Eigentümer ctox trägt migration_strategies 1–5; Kopie in reports nicht übernommen.'],
  ['outbound_messages', 'DRIFT.md §2 mail [] vs. schema.js [1]', 'Eigentümer outbound trägt migration_strategies 1; Kopie in mail nicht übernommen.'],
  ['outbound_lead_generation_adapters', 'DRIFT.md §2 browser [] vs. schema.js [1,2]; OLG V4-PORT.md §16.2', 'Eigentümer outbound-lead-generation (Kern §16.21) trägt migration_strategies 1,2 und das Schema, das Rust schreibt (store_outbound_commands.rs:2472-2486: id, source_id, status, scrape_status, auth_status, last_command_id, last_task_id, last_error, payload, created_at_ms, updated_at_ms). Abweichende browser-Kopie nicht übernommen. Rust ergänzt auth_authenticated_at_ms (:13985-13989), durch additionalProperties:true gedeckt.'],
  ['business_commands', 'DRIFT.md §7 (Indizes command_id, status, status+updated_at_ms, module+command_type+status+updated_at_ms fehlten in 6 Kopien)', 'Eigentümer ctox-platform übernimmt die Referenzfassung (= business_os_schema_contract.json, modules/ctox) MIT allen vier Indizes und execution_progress/result/task_id/task_status; die sechs abweichenden Kopien werden nicht übernommen.'],
  ['business_module_catalog', 'DRIFT.md §7 (app-store/mail-Kopie ohne required modules/templates/updated_at_ms)', 'Eigentümer ctox-platform übernimmt die Referenzfassung; abweichende Kopien nicht übernommen.'],
];
for (const [name, ev, text] of DRIFT_NOTES) {
  const { owner } = ownerOf(name);
  out[owner].report.push({ collection: name, field: '(Drift)', old: '–', next: 'Referenzfassung beim Eigentümer', ev, src: 'v4/tools/manifest/out/DRIFT.md', kind: 'note', text });
}

// Einstufung §16.4 (schema-diff.js, streng) und Migrationen im Format A9 (Stub cff5ba5 index.d.ts:270-279):
//   { from_ver: <Basis-App-Version>, steps: MigrationStep[] }  oder  { from_ver, migrate_fn: '<Pfad im App-Paket>' }
// from_ver = Basis-Release (BASE_RELEASE); vergibt der Basis-Release-Import eine andere Basis-app_ver, ist nur
// dieser eine Wert zu ersetzen. Reine Erweiterungen (null erlaubt, Feld ergänzt, Pflicht gelockert) migrieren mit
// steps: [] (Identität: jedes Bestandsdokument bleibt gültig). Nur echte Umformungen bekommen eine migrate_fn-Datei
// v4/schemas/migrations/<app>/<collection>.js (im App-Paket: migrations/<collection>.js).
const MIG_DIR = path.join(HERE, 'migrations');
fs.rmSync(MIG_DIR, { recursive: true, force: true });
const MIGRATE_BODY = {
  accounting_dunning_runs: [
    '  // invoices_total: number → string[] (Rust schreibt die Liste der gemahnten Rechnungs-IDs, invoices.rs:1693, :1704).',
    '  // Einziger Schreiber ist Rust (immer Array); ein Zahl-Altwert ist nicht verlustfrei abbildbar und bleibt als',
    '  // invoices_total_count erhalten.',
    '  if (Array.isArray(doc.invoices_total)) return doc;',
    '  const next = { ...doc, invoices_total: [] };',
    "  if (typeof doc.invoices_total === 'number') next.invoices_total_count = doc.invoices_total;",
    '  return next;',
  ],
};
// MAIL2: id ← bisheriger Schlüssel (Kopie, *_key bleibt). Die A9-Schrittsprache (Stub cff5ba5 index.d.ts:276-280:
// rename/add/remove/cast) hat keine Kopie – rename würde *_key löschen –, und check-schemas.mjs bildet steps nicht
// nach; daher migrate_fn. Die RxDB-Versionshistorie (Format v1) bekommt die deklarative Kopie set_from_first_truthy.
const KEY_COPY = { communication_accounts: 'account_key', communication_threads: 'thread_key', communication_messages: 'message_key' };
for (const [name, key] of Object.entries(KEY_COPY)) {
  MIGRATE_BODY[name] = [
    `  // id ← ${key} (App-API 4.0 adressiert über id; ${key} bleibt als Pflicht-/Indexfeld). Bereits gesetzte id bleibt.`,
    "  if (typeof doc.id === 'string' && doc.id) return doc;",
    `  return { ...doc, id: doc.${key} };`,
  ];
}
const RXDB_OPS = Object.fromEntries(Object.entries(KEY_COPY).map(([name, key]) => [name, [{ default: '', field: 'id', op: 'set_from_first_truthy', paths: ['id', key] }]]));
const notVersion = (r) => !/Schema-Eigenschaft „version“/.test(r);
for (const [appId, app] of Object.entries(out)) {
  const base = parseCollectionsFile({ collections: app.base });
  const next = parseCollectionsFile({ collections: app.collections });
  const diff = classifyCollections(base, next);
  app.diff = diff;
  for (const d of diff) {
    if (d.kind !== 'generation') continue;
    const s = app.collections[d.name];
    const fromVer = Number(s.version ?? 0);
    // RxDB verlangt bei geändertem Schema eine neue version samt Strategie (sonst DB6-Hash-Konflikt).
    s.version = fromVer + 1;
    app.rxdb[d.name] = { ...(app.rxdb[d.name] ?? {}), [String(fromVer + 1)]: { operations: clone(RXDB_OPS[d.name] ?? []) } };
    const reasons = d.reasons.filter(notVersion);
    const entry = { from_ver: BASE_RELEASE };
    if (MIGRATE_BODY[d.name]) {
      const rel = `migrations/${appId}/${d.name}.js`;
      fs.mkdirSync(path.join(MIG_DIR, appId), { recursive: true });
      fs.writeFileSync(path.join(HERE, rel), [
        `// Migration ${appId}/${d.name}: Basis ${BASE_RELEASE} (Schema v${fromVer}) → Schema v${fromVer + 1}, neue Generation (§16.4).`,
        '// Erzeugt von v4/schemas/build-schemas.mjs (Strang SCH). Im App-Paket: migrations/' + d.name + '.js. Gründe:',
        ...reasons.map((r) => `//   - ${r}`),
        'export default function migrate(doc) {',
        ...MIGRATE_BODY[d.name],
        '}',
        '',
      ].join('\n'));
      entry.migrate_fn = `migrations/${d.name}.js`;
      entry._file = `v4/schemas/${rel}`;
    } else {
      entry.steps = [];
    }
    app.migrations[d.name] = { ...entry, _schema_version: { from: fromVer, to: fromVer + 1 }, _reasons: reasons };
    for (const r of app.report) if (r.collection === d.name && !r.kind) r.kind = 'inkompatibel';
  }
  for (const r of app.report) if (!r.kind) r.kind = 'kompatibel';
}

// Schreiben
const sortObj = (o) => Object.fromEntries(Object.keys(o).sort().map((k) => [k, o[k]]));
let files = 0;
for (const [appId, app] of Object.entries(out).sort()) {
  const file = {
    _app_id: appId,
    _format: FORMAT,
    _base_release: BASE_RELEASE,
    _generator: 'src/apps/business-os/v4/schemas/build-schemas.mjs',
    _sources: sortObj(app.sources),
    ...(app.dropped.length ? { _foreign_copies_not_taken: app.dropped.sort((a, b) => a.name.localeCompare(b.name)) } : {}),
    collections: Object.fromEntries(Object.keys(app.collections).sort().map((n) => [n, Object.keys(app.meta[n] ?? {}).length ? { schema: app.collections[n], ...sortObj(app.meta[n]) } : app.collections[n]])),
    migrations: sortObj(app.migrations),
    ...(Object.keys(app.rxdb).length ? { migration_strategies: sortObj(app.rxdb) } : {}),
  };
  fs.writeFileSync(path.join(HERE, `${appId}.collections.schema.json`), `${JSON.stringify(file, null, 2)}\n`);
  files += 1;
}
const md = [];
const esc = (v) => String(v ?? '').replace(/\|/g, '\\|').replace(/\n/g, ' ');
md.push('# Strang SCH – korrigierte Collection-Schemata (Shell v4)', '');
md.push('Generiert von `v4/schemas/build-schemas.mjs` (nicht von Hand ändern), geprüft von `v4/schemas/check-schemas.mjs`.');
md.push(`Basis: ${BASE_RELEASE}. Rust-Belege \`rs:\` = \`~/.local/state/shell-v4-src/4794af03d/src/core/business_os/\` (nur gelesen).`, '');
md.push('## Format und Entscheidungen', '');
md.push('- Datei je App `v4/schemas/<app_id>.collections.schema.json`, Format A9 (§16.22, Stub cff5ba5 `index.d.ts:270-279`): `collections` (Schema oder Hülle `{schema, syncProfile|conflictStrategy}`), `migrations` (`{from_ver, steps}` oder `{from_ver, migrate_fn}`). `_`-Schlüssel sind Herkunft/Bericht; `migration_strategies` ist die unveränderte RxDB-Versionshistorie (Format v1) plus je neuer Schema-version eine Identitätsstufe.');
md.push(`- \`from_ver\` = \`${BASE_RELEASE}\` (Basis-Release). Vergibt der Basis-Release-Import eine andere Basis-\`app_ver\`, ist nur dieser Wert zu ersetzen.`);
md.push('- Einstufung streng nach §16.4 mit `v4/source/schema-diff.js` (= Stub `classifyCollections`): jede Typänderung, jedes neue Feld ohne Default und jede geänderte Pflichtliste ist eine neue Generation. Reine Erweiterungen (null erlaubt, Feld ergänzt, Pflicht gelockert) migrieren mit `steps: []` (Identität, jedes Bestandsdokument bleibt gültig); echte Umformung nur `accounting_dunning_runs` (`migrate_fn`).');
md.push('- Bei neuer Generation steigt `version` um 1 (RxDB-Konvention, sonst Hash-Konflikt bei gleicher Version).');
md.push('- Je Collection genau ein Eigentümer (`v4/tools/manifest/out/ownership.json`, Kern §16.21: OLG besitzt `outbound_lead_generation_adapters`, `sellify_*`, `outbound_research_policies`; `shell` = `ctox-platform`). Fremdkopien stehen unter `_foreign_copies_not_taken`.');
md.push(`- ${Object.keys(apps).length} Apps + \`ctox-platform\` = ${Object.keys(out).length} Dateien. Apps ohne Befund: Schemata unverändert vom Eigentümer übernommen.`, '');
md.push('- Quelle ist die heutige Arbeitskopie von `collections.schema.json` (= upstream-main bis auf `knowledge`: dort zusätzlich `knowledge_views`/`knowledge_table_rows` aus Shell-v4-Commit 0387d3429; neue Collections, keine Migration).');
md.push('- Hüllen-Geschwister (`syncProfile`, `conflictStrategy`, Plattform-`class/rights/client_limit/byte_budget`) sind hash-neutral und nach §16.4 kompatibel. Der Stub cff5ba5 (`normalizeSchemaEntry`) trägt nur class/rights/client_limit/byte_budget ins Manifest; `conflictStrategy`/`syncProfile` muss der Kern-Import selbst übernehmen (Befund an den Kern).');
md.push('- `v4/tools/manifest/gen-manifest.mjs` liest diese Dateien mit Vorrang zurück; `build-schemas.mjs` nimmt deshalb keine Schemata aus Manifesten mit `_schema_source` v4/… (keine Zirkularität).');
md.push('- Abnahme: `node src/apps/business-os/v4/schemas/check-schemas.mjs` (A9-Form, Eigentum, Migration je Generation, 26 Rust-Dokumente gegen neu gültig und gegen Basis ungültig, Handler-Szenarien aus schema-befund*.test.mjs mit korrigiertem Schema).', '');
md.push('## Änderungen je App', '');
md.push('| App | Collection | Feld | alt | neu | Beleg | Befundquelle | Einstufung | Migration |', '|---|---|---|---|---|---|---|---|---|');
const noChange = [];
for (const [id, a] of Object.entries(out).sort()) {
  const rows = a.report.filter((r) => r.kind !== 'note');
  if (!rows.length) { noChange.push(id); continue; }
  for (const r of rows) {
    const m = a.migrations[r.collection];
    const mig = m ? (m.migrate_fn ? `migrate_fn ${m._file}` : `steps [] (v${m._schema_version.from}→v${m._schema_version.to})`) : '–';
    md.push(`| ${id} | ${r.collection} | ${esc(r.field)} | ${esc(r.old)} | ${esc(r.next)} | ${esc(r.ev)} | ${esc(r.src)} | ${r.kind} | ${esc(mig)} |`);
  }
}
md.push('', '## Befunde ohne Schemaänderung (nur notiert)', '');
md.push('| App | Collection | Feld | Beleg | Befund |', '|---|---|---|---|---|');
for (const [id, a] of Object.entries(out).sort()) for (const r of a.report.filter((x) => x.kind === 'note')) md.push(`| ${id} | ${r.collection} | ${esc(r.field)} | ${esc(r.ev)} | ${esc(r.text)} |`);
md.push('', '## Apps ohne Schemaänderung', '', noChange.map((x) => `\`${x}\``).join(', '), '');
md.push('## Fremdkopien (nicht übernommen, Eigentümer hält die Fassung)', '');
for (const [id, a] of Object.entries(out).sort()) if (a.dropped.length) md.push(`- ${id}: ${a.dropped.map((d) => `${d.name}→${d.owner}`).join(', ')}`);
md.push('');
fs.writeFileSync(path.join(HERE, 'REPORT.md'), `${md.join('\n')}\n`);
console.log(`geschrieben: ${files} Dateien, ${Object.values(out).reduce((n, a) => n + Object.keys(a.collections).length, 0)} Collections`);
