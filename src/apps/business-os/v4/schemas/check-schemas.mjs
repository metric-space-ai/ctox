#!/usr/bin/env node
// Shell v4 – Strang SCH: Abnahme der korrigierten Collection-Schemata (v4/schemas/*.collections.schema.json).
//   1. Jede Datei ist gültig im Format A9 (§16.22; Stub cff5ba5 index.d.ts:270-279) und vollständig (41 Apps + Plattform).
//   2. Jede Collection hat genau einen Eigentümer (= ownership.json), keine Doppelungen über Dateien.
//   3. Jede inkompatible Änderung (§16.4, schema-diff.js) gegenüber der Basisfassung hat eine Migration
//      (from_ver = Basis, steps oder vorhandene migrate_fn-Datei mit default export), und die Migration macht
//      ein Basis-Dokument zu einem gültigen Dokument.
//   4. Jedes in den Befunden (schema-befund*.test.mjs, V4-HANDLERS.md, V4-PORT.md) beanstandete Rust-Dokument
//      validiert gegen das korrigierte Schema – und scheitert an der Basisfassung (Befund reproduziert).
//   5. Die Handler-Szenarien aus schema-befund*.test.mjs laufen mit dem korrigierten Schema durch; alle
//      geschriebenen Dokumente validieren.
// Validator: type / properties / required / items / enum (+ additionalProperties:false), rekursiv.
// Aufruf: node src/apps/business-os/v4/schemas/check-schemas.mjs   → Exit 0 = grün.

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BOS = path.resolve(HERE, '../..');
const BASE_RELEASE = 'main@4794af03d';
const PLATFORM = 'ctox-platform';
const { classifyCollections, parseCollectionsFile } = await import(pathToFileURL(path.join(BOS, 'v4/source/schema-diff.js')).href);
const readJson = (p) => JSON.parse(fs.readFileSync(p, 'utf8'));

let failures = 0;
let passes = 0;
const fail = (msg) => { failures += 1; console.log(`FAIL ${msg}`); };
const ok = (cond, msg) => { if (cond) passes += 1; else fail(msg); return cond; };

// ---------------------------------------------------------------- Validator

const isObj = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
function typeOk(t, v) {
  if (t === 'null') return v === null;
  if (t === 'array') return Array.isArray(v);
  if (t === 'object') return isObj(v);
  if (t === 'integer') return Number.isInteger(v);
  if (t === 'number') return typeof v === 'number' && Number.isFinite(v);
  if (t === 'string' || t === 'boolean') return typeof v === t;
  return false;
}
export function validate(schema, value, at = '$') {
  const errs = [];
  if (!isObj(schema)) return errs;
  if (schema.type !== undefined) {
    const types = Array.isArray(schema.type) ? schema.type : [schema.type];
    if (!types.some((t) => typeOk(t, value))) { errs.push(`${at}: Typ ${JSON.stringify(schema.type)} erwartet, ist ${value === null ? 'null' : Array.isArray(value) ? 'array' : typeof value}`); return errs; }
  }
  if (Array.isArray(schema.enum) && !(value === null && Array.isArray(schema.type) && schema.type.includes('null'))) {
    if (!schema.enum.some((e) => JSON.stringify(e) === JSON.stringify(value))) errs.push(`${at}: ${JSON.stringify(value)} nicht in enum`);
  }
  if (isObj(value)) {
    for (const key of schema.required ?? []) if (!(key in value) || value[key] === undefined) errs.push(`${at}.${key}: Pflichtfeld fehlt`);
    const props = schema.properties ?? {};
    for (const [k, v] of Object.entries(value)) {
      if (k in props) errs.push(...validate(props[k], v, `${at}.${k}`));
      else if (schema.additionalProperties === false && !k.startsWith('_')) errs.push(`${at}.${k}: nicht deklariert (additionalProperties:false)`);
    }
  }
  if (Array.isArray(value) && isObj(schema.items)) value.forEach((item, i) => errs.push(...validate(schema.items, item, `${at}[${i}]`)));
  return errs;
}

// ---------------------------------------------------------------- 1. Dateien und A9-Format

const APPS = [
  ...fs.readdirSync(path.join(BOS, 'modules')).filter((d) => fs.existsSync(path.join(BOS, 'modules', d, 'collections.schema.json'))),
  'outbound-lead-generation',
].sort();
ok(APPS.length === 41, `41 Apps erwartet, gefunden ${APPS.length}`);
const files = {};
for (const id of [...APPS, PLATFORM]) {
  const f = path.join(HERE, `${id}.collections.schema.json`);
  if (!ok(fs.existsSync(f), `${id}: Datei fehlt`)) continue;
  try { files[id] = readJson(f); } catch (e) { fail(`${id}: kein JSON (${e.message})`); }
}
const extra = fs.readdirSync(HERE).filter((f) => f.endsWith('.collections.schema.json') && !files[f.replace('.collections.schema.json', '')]);
ok(extra.length === 0, `unerwartete Dateien: ${extra.join(', ')}`);

const STEP_OPS = { rename: ['from', 'to'], add: ['field'], remove: ['field'], cast: ['field', 'to'] };
const schemaOf = (entry) => (isObj(entry?.schema) && !entry.properties ? entry.schema : entry);

for (const [id, file] of Object.entries(files)) {
  ok(isObj(file) && isObj(file.collections) && isObj(file.migrations), `${id}: collections/migrations müssen Objekte sein`);
  ok(file._app_id === id, `${id}: _app_id ${file._app_id}`);
  try { parseCollectionsFile(file); passes += 1; } catch (e) { fail(`${id}: parseCollectionsFile: ${e.message}`); }
  for (const [name, entry] of Object.entries(file.collections ?? {})) {
    const s = schemaOf(entry);
    const where = `${id}/${name}`;
    ok(isObj(s) && s.type === 'object', `${where}: type object`);
    ok(typeof s.primaryKey === 'string' && isObj(s.properties) && s.primaryKey in s.properties, `${where}: primaryKey in properties`);
    ok(s.version === undefined || (Number.isInteger(s.version) && s.version >= 0), `${where}: version ganzzahlig`);
    ok(Array.isArray(s.required) && s.required.every((r) => r in s.properties), `${where}: required ⊆ properties (${(s.required ?? []).filter((r) => !(r in (s.properties ?? {}))).join(',')})`);
    for (const [field, def] of Object.entries(s.properties ?? {})) {
      const types = def.type === undefined ? [] : Array.isArray(def.type) ? def.type : [def.type];
      ok(types.every((t) => ['string', 'number', 'integer', 'boolean', 'object', 'array', 'null'].includes(t)), `${where}.${field}: unbekannter Typ ${JSON.stringify(def.type)}`);
    }
  }
  for (const [name, m] of Object.entries(file.migrations ?? {})) {
    const where = `${id}/migrations/${name}`;
    ok(name in file.collections, `${where}: Collection nicht in collections`);
    ok(typeof m.from_ver === 'string' && m.from_ver.length > 0, `${where}: from_ver muss String sein`);
    const hasSteps = Array.isArray(m.steps);
    const hasFn = typeof m.migrate_fn === 'string';
    ok(hasSteps !== hasFn, `${where}: genau eins von steps/migrate_fn`);
    if (hasSteps) for (const st of m.steps) ok(STEP_OPS[st?.op] && STEP_OPS[st.op].every((k) => k in st), `${where}: Schritt ${JSON.stringify(st)} ungültig`);
    if (hasFn) {
      const local = path.join(HERE, 'migrations', id, path.basename(m.migrate_fn));
      ok(m.migrate_fn === `migrations/${path.basename(m.migrate_fn)}` && fs.existsSync(local), `${where}: migrate_fn-Datei ${local} fehlt`);
    }
  }
}

// ---------------------------------------------------------------- 2. Eigentum

const ownership = readJson(path.join(BOS, 'v4/tools/manifest/out/ownership.json')).ownership;
const seen = new Map();
for (const [id, file] of Object.entries(files)) {
  for (const name of Object.keys(file.collections ?? {})) {
    ok(!seen.has(name), `${name}: doppelt in ${seen.get(name)} und ${id}`);
    seen.set(name, id);
    const o = ownership[name];
    if (o) ok((o.owner === 'shell' ? PLATFORM : o.owner) === id, `${name}: Eigentümer laut ownership.json ${o.owner}, Datei ${id}`);
  }
}
for (const name of ['outbound_lead_generation_adapters', 'outbound_research_policies', 'sellify_companies', 'sellify_people', 'sellify_campaigns']) {
  ok(seen.get(name) === 'outbound-lead-generation', `${name}: Eigentum OLG (Kern §16.21) erwartet, ist ${seen.get(name)}`);
}

// ---------------------------------------------------------------- 3. Inkompatible Änderungen haben Migrationen

const PLATFORM_MANIFEST = path.join(os.homedir(), '.local/state/ctox-sync-v4/docs/v4-manifests/ctox-platform.manifest.json');
function baseEntry(id, name, src) {
  if (!src || src.startsWith('(neu')) return null;
  if (src.includes('ctox-platform.manifest.json')) return fs.existsSync(PLATFORM_MANIFEST) ? readJson(PLATFORM_MANIFEST).collections?.[name]?.schema ?? null : null;
  const p = path.join(BOS, src);
  if (!fs.existsSync(p)) return null;
  const e = readJson(p).collections?.[name];
  return e ? schemaOf(e) : null;
}
const migratedBase = {}; // `${id}/${name}` -> {base, migrate}
for (const [id, file] of Object.entries(files)) {
  const baseCols = {};
  const nextCols = {};
  for (const [name, entry] of Object.entries(file.collections)) {
    const b = baseEntry(id, name, file._sources?.[name]?.source);
    if (b) baseCols[name] = b;
    nextCols[name] = schemaOf(entry);
  }
  const diff = classifyCollections(parseCollectionsFile({ collections: baseCols }), parseCollectionsFile({ collections: nextCols, migrations: file.migrations }), { baseVer: BASE_RELEASE });
  for (const d of diff) {
    if (d.kind !== 'generation') continue;
    if (!ok(d.migration, `${id}/${d.name}: inkompatibel (${d.reasons.join('; ')}) ohne passende Migration`)) continue;
    let migrate = (doc) => ({ ...doc });
    if (d.migration.migrate_fn) {
      const mod = await import(pathToFileURL(path.join(HERE, 'migrations', id, path.basename(d.migration.migrate_fn))).href);
      if (!ok(typeof mod.default === 'function', `${id}/${d.name}: migrate_fn ohne default export`)) continue;
      migrate = mod.default;
    } else if (d.migration.steps.length) {
      fail(`${id}/${d.name}: steps im Check nicht nachgebildet`);
    }
    migratedBase[`${id}/${d.name}`] = { base: baseCols[d.name], next: nextCols[d.name], migrate };
  }
}
ok(Object.keys(migratedBase).length > 0, 'keine einzige Generation gefunden (Check prüft nichts)');

// ---------------------------------------------------------------- 4. Beanstandete Rust-Dokumente

const NOW = 1_760_000_000_000;
const ISO = '2025-10-09T08:53:20.000Z';
// Jedes Dokument so, wie Rust es schreibt (Zeilen im Schnappschuss ~/.local/state/shell-v4-src/4794af03d/src/core/business_os/).
// upsert_business_record setzt id/_rev/_deleted/updated_at_ms (store_projections.rs:1405-1410).
const RUST_DOCS = [
  ['kundenpipeline', 'kundenpipeline_vorgaenge', 'decision_hub.rs:127-150 (project_inbound_messages)', {
    id: 'kpl-v-1', title: 'Export anpassen', status: 'eingegangen', kunde_id: '', kunde_name: '',
    quelle_json: { kanal: 'mail', message_ref: 'm1', thread_ref: 't1', absender: 'a@b.example', absender_name: 'A', betreff: 'Export', body_clean: 'x', eingegangen: ISO },
    triage_json: null, run_json: null, mails_json: [], audit_json: [{ at_ms: NOW, event: 'ingest', actor: 'system', role: 'system' }],
    notes: '', is_deleted: false, created_at_ms: NOW, updated_at_ms: NOW }],
  ['invoices', 'accounting_dunning_runs', 'invoices.rs:1694-1707', {
    id: 'run_1', run_date_ms: NOW, run_by: 'system', filter: {}, invoices_total: ['inv_1', 'inv_2'], letters_sent: 0, state: 'draft', created_at_ms: NOW, updated_at_ms: NOW }],
  ['invoices', 'accounting_payment_allocations', 'invoices.rs:1582-1592 (payment_id Option<String> = None)', {
    id: 'alloc_1', payment_id: null, invoice_id: 'inv_1', allocated_cents: 100, skonto_cents: 0, note: null, allocated_at_ms: NOW, allocated_by_command_id: 'cmd_1', updated_at_ms: NOW }],
  ['invoices', 'accounting_dunning_letters', 'invoices.rs:1718-1733', {
    id: 'let_run_1_inv_1', dunning_run_id: 'run_1', invoice_id: 'inv_1', level: 1, letter_date_ms: NOW, fee_cents: 0, interest_cents: 0, total_cents: 0,
    pdf_attachment_id: null, sent_via: null, sent_at_ms: null, status: 'draft', created_at_ms: NOW, updated_at_ms: NOW }],
  ['matching', 'matching_requirements', 'importer.rs:724-743 (kind source)', {
    id: 'src_1', kind: 'source', name: 'ACME', legalName: 'ACME', logoUrl: '', sourceUrl: 'https://acme.example/job', active: true,
    locations: [{ id: 'loc_1', city: 'Köln', country: 'Deutschland', postalCode: '50667', address: '', countryKey: 'DE', postalCodeKey: '50667' }],
    createdAt: ISO, updatedAt: ISO, updated_at_ms: NOW }],
  ['matching', 'matching_requirements', 'importer.rs:744-770 (kind requirement)', {
    id: 'req_1', kind: 'requirement', sourceId: 'src_1', sourceName: 'ACME', title: 'Elektriker (m/w/d)', internalReferenceId: '', status: 'open', location: 'Köln',
    locationIds: ['loc_1'], fachlevelClass: null, workModel: null, type: 'Vollzeit', remote: false, language: 'de', sourceUrl: 'https://acme.example/job', rawText: '…', active: true,
    createdAt: ISO, updatedAt: ISO, updated_at_ms: NOW }],
  ['matching', 'matching_requirements', 'importer.rs:771-799 (kind requirementSource)', {
    id: 'req_1::source', kind: 'requirementSource', requirementId: 'req_1', sourceId: 'src_1', source: 'stepstone', sourceUrl: 'https://acme.example/job', externalRef: '',
    rawText: '…', rawHtmlSha256: 'ab', parsed: { remote: false }, parsingMeta: { schemaVersion: 'matching.requirement.v1', confidence: 0.8, httpStatus: 200, command_id: 'cmd_1', queue_task_id: null },
    createdAt: ISO, updatedAt: ISO, updated_at_ms: NOW }],
  ['matching', 'matching_objects', 'importer.rs:939-991 (kind object)', {
    id: 'obj_1', kind: 'object', name: 'Max Muster', firstName: 'Max', lastName: 'Muster', email: '', phone: '', currentRole: '', desiredPosition: '', taxonomy: 'importiertes Objekt',
    region: '', languages: [], skills: [], executiveInfo: {}, active: true, activeKey: 1, documents: [{ id: 'obj_1::document', kind: 'document', filename: 'cv.pdf', mimeType: 'application/pdf' }],
    createdAt: ISO, updatedAt: ISO, updated_at_ms: NOW }],
  ['matching', 'matching_results', 'importer.rs:1084-1107 (kind match)', {
    id: 'match_1', kind: 'match', sourceId: 'src_1', requirementId: 'req_1', objectId: 'obj_1', requirementTitle: 'Elektriker', objectName: 'Max Muster', active: true, removed: false,
    progress: 10, status: 'prematch', score: 72, scoreKey: 72, notes: '', items: [{ label: 'Skill', score: 1 }], evidence: [{ label: 'Skill', score: 1 }],
    createdAt: ISO, updatedAt: ISO, command_id: 'cmd_1', queue_task_id: null, updated_at_ms: NOW }],
  ['outbound', 'outbound_suppression_entries', 'store_outbound_delivery_policy.rs:127-138', {
    id: 'supp_reply_ab12', email: 'x@acme.example', domain: 'acme.example', status: 'active', reason: 'unsubscribe', suppression_reason: 'unsubscribe',
    source: 'reply_classification', engagement_id: 'eng_1', created_at_ms: NOW, updated_at_ms: NOW }],
  ['outbound', 'outbound_sender_assignments', 'store_outbound_commands.rs:192-199 (Payload ohne campaign_id)', {
    id: 'assign_eng_1_snd_1', engagement_id: 'eng_1', sender_account_id: 'snd_1', status: 'active', created_at_ms: NOW, updated_at_ms: NOW }],
  ['outbound-lead-generation', 'outbound_research_policies', 'store_outbound_commands.rs:4553-4592 + store_projections.rs:1409', {
    id: 'policy:outbound-lead-generation', app: 'outbound-lead-generation', published_at_ms: NOW, policy_contract: 'ctox.outbound.research_policy.v1', steps: [{ field_key: 'firma_umsatz' }], updated_at_ms: NOW }],
  ...['sellify_companies', 'sellify_people', 'sellify_campaigns'].flatMap((c) => [
    ['outbound-lead-generation', c, 'external_sql_sync.rs:620-638 (Projektionszeile)', { id: `${c}:42`, name: 'ACME GmbH', is_deleted: false, _ctox_external_sql: { source: 'sellify', projection: c }, updated_at_ms: NOW }],
    ['outbound-lead-generation', c, 'external_sql_sync.rs:689-699 (Löschzeile)', { id: `${c}:43`, is_deleted: true, deleted_at_ms: NOW, updated_at_ms: NOW, _ctox_external_sql: { source: 'sellify', projection: c } }],
  ]),
  ['outbound-lead-generation', 'outbound_lead_generation_leads', 'outbound_lead_import.rs:160-196 + contact_email_validation.rs:364-375, :415-418', {
    id: 'lead_1', import_id: 'imp_1', campaign: 'K1', name: 'ACME', domain: 'acme.example', website: 'https://acme.example', city: 'Köln', country: 'DE', research_status: 'new',
    validation_status: 'pending', sellify_status: 'not_started', task_id: '', command_id: '', contacts: [{ person_key: 'p1', field_status: { person_email_validation: { value: 'valid' } } }],
    selected_contact_ids: [], evidence: [], data: { herkunft_import: 'x', statistische_kampagne: 'K1' }, payload: { import_source: 'ctox_action' },
    field_status: { person_email_validation: { value: 'valid', revision: 1 } }, person_field_status: { p1: { person_email_validation: { value: 'valid' } } },
    created_at_ms: NOW, updated_at_ms: NOW }],
  ['outbound-lead-generation', 'outbound_lead_generation_adapters', 'store_outbound_commands.rs:2472-2484, :13985-13989', {
    id: 'adp_1', source_id: 'src_1', status: 'ready', scrape_status: 'ok', auth_status: 'session_authenticated', last_command_id: 'cmd_1', last_task_id: 'task_1', last_error: '',
    payload: { configuration_digest: 'd', adapter_revision: null, script_path: null, test: null, secret_value_in_payload: false }, created_at_ms: NOW, updated_at_ms: NOW, auth_authenticated_at_ms: NOW }],
  ['ctox', 'workjet_sessions', 'store_workjet_sessions.rs:1392-1397 (last_terminal_turn_id Null)', {
    id: 'ses_1', project_id: 'prj_1', working_copy_id: 'wc_1', computer_id: 'cmp_1', run_status: 'paused', fence_epoch: 1, owner_user_id: 'u1',
    last_terminal_turn_id: null, created_at_ms: NOW, updated_at_ms: NOW, is_deleted: false }],
  ['ctox', 'workjet_session_transfers', 'store_workjet_sessions.rs:1383-1391, :1744, :1995-1996, :2047-2053, :2346, :2566, :272-275', {
    id: 'trf_1', session_id: 'ses_1', project_id: 'prj_1', source_working_copy_id: 'wc_1', source_computer_id: 'cmp_1', target_computer_id: 'cmp_2', target_path: '/w',
    state: 'applied', fence_epoch: 1, artifact_file_ids: [], deadline_at_ms: NOW, created_at_ms: NOW, updated_at_ms: NOW, owner_user_id: 'u1', is_deleted: false,
    source_git_repository: true, last_terminal_turn_id: null, pause_acked_at_ms: NOW, packed_at_ms: NOW, apply_mismatch_sha256: 'a'.repeat(64), apply_mismatch_count: 0,
    applied_at_ms: NOW, observed_manifest_sha256: 'b'.repeat(64), observed_head: 'c'.repeat(40), observed_tree_sha256: 'd'.repeat(64), switched_at_ms: NOW,
    worker_instance_id: 'wk_1', recovery_aborting_at_ms: NOW, target_partial_cleanup_required: false }],
  ...[null, ['open', 'pending'], 'open', 3].map((value) => ['support', 'support_view_filters', 'support.rs:1936 (json_field … unwrap_or(Value::Null)), :3493', {
    id: 'flt_1', view_id: 'view_1', field: 'status', operator: 'eq', value, position: 0, created_at_ms: NOW, updated_at_ms: NOW }]),
];

const collectionSchema = (app, name) => { const e = files[app]?.collections?.[name]; return e ? schemaOf(e) : null; };
// Nur-Deklaration: Felder waren über additionalProperties:true schon zulässig, werden jetzt typisiert deklariert.
const DECLARE_ONLY = new Set(['outbound_lead_generation_leads', 'outbound_lead_generation_adapters']);
for (const [app, name, ev, doc] of RUST_DOCS) {
  const schema = collectionSchema(app, name);
  if (!ok(schema, `${app}/${name}: Collection fehlt im korrigierten Schema`)) continue;
  const errs = validate(schema, doc);
  ok(errs.length === 0, `${app}/${name} [${ev}]: Rust-Dokument ungültig: ${errs.join('; ')}`);
  const base = baseEntry(app, name, files[app]._sources?.[name]?.source);
  if (base && !DECLARE_ONLY.has(name)) ok(validate(base, doc).length > 0, `${app}/${name} [${ev}]: Befund nicht reproduziert – Dokument war schon gegen die Basis gültig`);
}

// Migration: Basis-Dokument → gültig gegen die neue Fassung (accounting_dunning_runs mit Zahl-Altwert).
{
  const m = migratedBase['invoices/accounting_dunning_runs'];
  if (ok(m, 'invoices/accounting_dunning_runs: Migration erwartet')) {
    const legacy = { id: 'run_old', run_date_ms: NOW, invoices_total: 3, state: 'draft', created_at_ms: NOW, updated_at_ms: NOW };
    ok(validate(m.base, legacy).length === 0, 'Altdokument muss gegen die Basis gültig sein');
    const migrated = m.migrate(legacy);
    ok(validate(m.next, migrated).length === 0 && migrated.invoices_total_count === 3, `Migration accounting_dunning_runs: ${validate(m.next, migrated).join('; ')}`);
  }
}

// ---------------------------------------------------------------- 5. Handler-Szenarien aus schema-befund*.test.mjs

// Alle korrigierten Collections (Fremd-Collections wie accounting_number_series kommen vom Eigentümer).
const ALL = Object.fromEntries(Object.values(files).flatMap((f) => Object.entries(f.collections).map(([n, e]) => [n, schemaOf(e)])));
const flat = () => ({ collections: ALL });
async function scenario(label, app, run) {
  try {
    const { createCtx, cmd } = await import(pathToFileURL(path.join(BOS, `modules/${app}/handlers/tests/_ctx.mjs`)).href);
    const t = await run(createCtx, cmd, flat(files[app]));
    const written = Object.keys(ALL).flatMap((c) => t.all(c).map((d) => [c, d]));
    ok(written.length > 0, `${label}: keine Dokumente geschrieben`);
    for (const [c, d] of written) {
      const errs = validate(ALL[c], d);
      ok(errs.length === 0, `${label}: ${c}/${d.id} ungültig: ${errs.join('; ')}`);
    }
  } catch (e) {
    fail(`${label}: ${e?.code ?? ''} ${e?.message ?? e}`);
  }
}

await scenario('kundenpipeline B1 (triage.write auf Vorgang mit triage_json/run_json null)', 'kundenpipeline', async (createCtx, cmd, schema) => {
  const { vorgang } = await import(pathToFileURL(path.join(BOS, 'modules/kundenpipeline/handlers/tests/_fixtures.mjs')).href);
  const handle = (await import(pathToFileURL(path.join(BOS, 'modules/kundenpipeline/handlers/kundenpipeline.triage.write.js')).href)).default;
  const t = createCtx({ now: NOW, schema, seed: { kundenpipeline_vorgaenge: [vorgang({ triage_json: null, run_json: null })] }, caps: { 'identity.users.list': async () => [{ user_id: 'user_owner' }] } });
  await handle(cmd('kundenpipeline.triage.write', { vorgang_id: 'kpl-v-1', triage: {} }), t.ctx);
  return t;
});

const inv = (f) => import(pathToFileURL(path.join(BOS, `modules/invoices/handlers/${f}`)).href).then((m) => m.default);
const invFixtures = await import(pathToFileURL(path.join(BOS, 'modules/invoices/handlers/tests/_fixtures.mjs')).href);
const overdue = { id: 'x', invoice_type: 'sale_out', state: 'overdue', currency: 'EUR', search_text: '', is_deleted: false, created_at_ms: 1, updated_at_ms: 1, open_cents: 5, due_date_ms: NOW - 1 };

await scenario('invoices I4+I6 (dunning.run mit überfälliger Rechnung → Lauf + Mahnbrief mit null-Feldern)', 'invoices', async (createCtx, cmd, schema) => {
  const t = createCtx({ now: invFixtures.NOW, schema, seed: { accounting_invoices: [{ ...overdue, due_date_ms: invFixtures.NOW - 1 }] } });
  await (await inv('invoices.dunning.run.js'))(cmd('invoices.dunning.run', { run_id: 'r' }), t.ctx);
  ok(t.all('accounting_dunning_letters').some((l) => l.sent_via === null && l.sent_at_ms === null && l.pdf_attachment_id === null), 'I6: Mahnbrief mit null-Feldern erwartet');
  ok(t.all('accounting_dunning_runs').some((r) => Array.isArray(r.invoices_total)), 'I4: Lauf mit ID-Array erwartet');
  return t;
});

await scenario('invoices I5 (payment.allocate mit und ohne payment_id)', 'invoices', async (createCtx, cmd, schema) => {
  const t = createCtx({ now: invFixtures.NOW, schema });
  await (await inv('invoices.invoice.create.js'))(cmd('invoices.invoice.create', invFixtures.draftPayload()), t.ctx);
  await (await inv('invoices.invoice.post.js'))(cmd('invoices.invoice.post', { invoice_id: 'inv_post' }), t.ctx);
  const allocate = await inv('invoices.payment.allocate.js');
  await allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post', payment_id: 'p', amount_cents: 1 }), t.ctx);
  await allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post', amount_cents: 1 }, { id: 'cmd_alloc_2' }), t.ctx);
  ok(t.all('accounting_payment_allocations').some((a) => a.payment_id === null && a.note === null), 'I5: Zuordnung ohne payment_id mit note null erwartet');
  return t;
});

console.log(`\ncheck-schemas: ${passes} Prüfungen grün, ${failures} rot (${Object.keys(files).length} Dateien, ${seen.size} Collections, ${Object.keys(migratedBase).length} Generationen mit Migration, ${RUST_DOCS.length} Rust-Dokumente)`);
process.exit(failures ? 1 : 0);
